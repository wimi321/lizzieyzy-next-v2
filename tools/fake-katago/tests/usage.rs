#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::process::Command;

use common::Engine;

#[test]
fn prints_usage_and_exits_successfully() {
    let output = Command::new(env!("CARGO_BIN_EXE_fake-katago"))
        .arg("--help")
        .output()
        .expect("fake-katago should start");
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("analysis"));
    assert!(help.contains("--time-scale"));
    assert!(help.contains("--garbage-every"));
    assert!(output.stderr.is_empty());
}

#[test]
fn binary_path_returns_a_runnable_analysis_engine() {
    let path = fake_katago::binary_path().expect("resolve fake executable");
    assert!(path.is_file());
    let mut process = Engine::spawn_at(path, &[]);
    process.ready();
    process.finish();
}
