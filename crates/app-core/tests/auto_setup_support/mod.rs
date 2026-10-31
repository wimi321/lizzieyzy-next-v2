#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::core_types::{AppConfig, AppEvent, AppSubscription, EngineStatus};
use std::{
    future::Future,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

pub async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("safety deadline")
}

pub fn config() -> AppConfig {
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = 1;
    config.analysis.live.report_interval = Duration::from_secs(60);
    config.session.write_timeout = Duration::from_secs(60);
    config
}

fn quoted(value: &Path) -> String {
    format!("'{}'", value.to_str().unwrap().replace('\'', "'\\''"))
}

/// fake-katago deliberately only implements analysis. This test-local launcher
/// supplies its CLI version response, and execs the actual fake for every trial.
pub fn program(directory: &Path, version: &str, fail: bool) -> PathBuf {
    let args = if fail {
        vec!["--fail-startup".to_owned()]
    } else {
        Vec::new()
    };
    program_with_args(directory, version, &args)
}

pub fn program_with_args(directory: &Path, version: &str, args: &[String]) -> PathBuf {
    let fake = fake_katago::binary_path().unwrap();
    std::fs::create_dir_all(directory).unwrap();
    let program = directory.join("katago");
    let log = quoted(&directory.join("trials"));
    let version_log = quoted(&directory.join("versions"));
    let body = format!(
        "#!/bin/sh\nif [ \"$1\" = version ]; then\n printf 'version\\n' >> {version_log}\n printf '%s\\n' '{}'\n exit 0\nfi\nprintf 'trial\\n' >> {log}\nexec {} \"$@\" {}\n",
        version.replace('\'', "'\\''"),
        quoted(&fake),
        args.iter()
            .map(|arg| quoted(Path::new(arg)))
            .collect::<Vec<_>>()
            .join(" ")
    );
    std::fs::write(&program, body).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    program
}

pub fn template(directory: &Path) -> PathBuf {
    let template = directory.join("bundled.cfg");
    std::fs::write(&template, "reportAnalysisWinratesAs = BLACK\n").unwrap();
    template
}

pub async fn analyzed(output: &mut AppSubscription) {
    within(async {
        loop {
            match output.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(view) if !view.is_during_search => {
                    assert!(view.visits > 0);
                    return;
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("{reason}")
                }
                _ => {}
            }
        }
    })
    .await;
}
