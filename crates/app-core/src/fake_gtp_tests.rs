#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::{future::Future, path::PathBuf, sync::OnceLock};

fn binary() -> &'static PathBuf {
    // Both Zhizi actor test modules execute in this one lib-test process.
    // A separate OnceLock in each module would still let cargo replace the
    // executable while another test is starting or running it (notably Windows).
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output =
            std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
                .args(["build", "--locked", "--manifest-path"])
                .arg(root.join("tools/fake-gtp/Cargo.toml"))
                .arg("--target-dir")
                .arg(root.join("target"))
                .arg("--message-format=json")
                .output()
                .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .find_map(|line| {
                let value: serde_json::Value = serde_json::from_str(line).ok()?;
                (value["reason"] == "compiler-artifact" && value["target"]["name"] == "fake-gtp")
                    .then(|| value["executable"].as_str().map(PathBuf::from))
                    .flatten()
            })
            .expect("fake-gtp executable artifact")
    })
}

pub(crate) fn run<F: Future<Output = ()>>(test: impl FnOnce(PathBuf) -> F) {
    // Synchronous fixture preparation must finish before a runtime exists:
    // cargo and OnceLock contention must never block a live socket/engine task
    // or prevent that task's five-second safety deadline from being polled.
    let program = binary().clone();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(test(program));
}
