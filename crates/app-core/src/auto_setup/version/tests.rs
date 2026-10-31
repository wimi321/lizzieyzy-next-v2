#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn minimum_version_is_numeric_and_unknown_versions_are_rejected() {
    for version in [
        "1.16.5",
        "v1.16.5",
        "1.16.5-fake",
        "1.16.5+build",
        "1.17.0",
        "2.0.0",
    ] {
        assert!(supports(version, "1.16.5"), "{version}");
    }
    for version in ["1.16.4", "1.9.99", "0.99.99"] {
        assert!(!supports(version, "1.16.5"), "{version}");
    }
    for version in [
        "",
        "unknown",
        "1.16",
        "1.16.5.1",
        "1.16.5x",
        "1.16.5-",
        "-1.16.5",
        "1.16.18446744073709551616",
    ] {
        assert!(!supports(version, "1.16.5"), "{version}");
        assert!(!supports("1.16.5", version), "{version}");
    }
}

#[test]
fn output_accepts_katago_banner_or_bare_version_without_using_other_build_versions() {
    assert_eq!(
        output_version("KataGo v1.16.5\nGit revision: abc\n"),
        Some("v1.16.5".into())
    );
    assert_eq!(output_version("1.16.5-fake\n"), Some("1.16.5-fake".into()));
    assert_eq!(output_version("CUDA 12.4.0\nunknown version\n"), None);
    assert_eq!(output_version("KataGo v1.16\n"), None);
}

#[tokio::test]
async fn output_limits_accept_exact_limit_and_reject_each_oversized_pipe() {
    let exact = vec![b'x'; OUTPUT_LIMIT];
    assert_eq!(
        read_bounded(exact.as_slice(), "stdout")
            .await
            .unwrap()
            .len(),
        OUTPUT_LIMIT
    );
    for name in ["stdout", "stderr"] {
        let excess = vec![b'x'; OUTPUT_LIMIT + 1];
        assert!(
            read_bounded(excess.as_slice(), name)
                .await
                .unwrap_err()
                .contains(name)
        );
    }
}

#[cfg(unix)]
mod processes {
    use super::*;
    use std::{os::unix::fs::PermissionsExt, path::PathBuf};
    use tempfile::TempDir;

    fn program(directory: &TempDir, body: &str) -> PathBuf {
        let path = directory.path().join("katago with spaces");
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\n[ \"$1\" = version ] || exit 19\nprintf '%s\\n' \"$$\" > \"${{0%/*}}/pid\"\n{body}\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::time::timeout(Duration::from_secs(5), future)
            .await
            .expect("safety deadline")
    }

    fn assert_reaped(directory: &TempDir) {
        let pid = std::fs::read_to_string(directory.path().join("pid")).unwrap();
        let status = std::process::Command::new("/bin/kill")
            .args(["-0", pid.trim()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(!status.success(), "probe child {pid} was not reaped");
    }

    #[tokio::test]
    async fn version_subcommand_returns_stdout_or_stderr_and_reports_failure() {
        let directory = TempDir::new().unwrap();
        for redirect in ["", ">&2"] {
            let path = program(
                &directory,
                &format!("printf 'KataGo v1.16.5\\n' {redirect}"),
            );
            assert_eq!(within(probe(&path)).await.unwrap(), "v1.16.5");
        }
        let path = program(&directory, "printf 'missing library\\n' >&2\nexit 7");
        let error = within(probe(&path)).await.unwrap_err();
        assert!(
            error.contains("7") && error.contains("missing library"),
            "{error}"
        );
        let path = program(&directory, "printf 'no version\\n'");
        assert!(
            within(probe(&path))
                .await
                .unwrap_err()
                .contains("no recognizable")
        );
    }

    #[tokio::test]
    async fn oversized_output_terminates_the_probe_without_waiting_for_eof() {
        let directory = TempDir::new().unwrap();
        for redirect in ["", ">&2"] {
            let path = program(
                &directory,
                &format!("while :; do printf '0123456789abcdef\\n' {redirect}; done"),
            );
            let error = within(probe(&path)).await.unwrap_err();
            assert!(error.contains("exceeds"), "{error}");
            assert_reaped(&directory);
        }
    }

    #[tokio::test]
    async fn hung_version_process_is_killed_at_the_deadline() {
        let directory = TempDir::new().unwrap();
        let path = program(&directory, "while :; do :; done");
        let attempt = tokio::spawn(async move { probe(&path).await });
        within(async {
            while !std::fs::read_to_string(directory.path().join("pid"))
                .is_ok_and(|pid| pid.trim().parse::<u32>().is_ok())
            {
                tokio::task::yield_now().await;
            }
        })
        .await;
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(4)).await;
        tokio::time::resume();
        let error = within(attempt).await.unwrap().unwrap_err();
        assert!(error.contains("timed out"), "{error}");
        assert_reaped(&directory);
    }
}
