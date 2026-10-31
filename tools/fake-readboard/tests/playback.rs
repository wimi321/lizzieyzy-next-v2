#![allow(clippy::unwrap_used)]

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("fake-readboard-{}-{name}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn real_launch_arguments_replay_once_and_quit_is_observed() {
    let dir = Scratch::new("quit");
    fs::write(
        dir.0.join("replay.txt"),
        "# reconstructed input\nstart 2 2\nre=3,0\nre=0,0\nend\n",
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_fake-readboard"))
        .args(["yzy", " ", " ", " ", "0", "cn", "-1"])
        .current_dir(&dir.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin
        .write_all(b"version\nversion\nanalysisState running\nquit\n")
        .unwrap();
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        text,
        "ready\nversion 220430\nstart 2 2\nre=3,0\nre=0,0\nend\nversion 220430\n"
    );
    assert_eq!(
        fs::read_to_string(dir.0.join("commands.log")).unwrap(),
        "version\nversion\nanalysisState running\nquit\n"
    );
    assert_eq!(fs::read_to_string(dir.0.join("exited")).unwrap(), "quit\n");
}

#[test]
fn host_eof_terminates_helper() {
    let dir = Scratch::new("eof");
    fs::write(dir.0.join("replay.txt"), "").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fake-readboard"))
        .args(["yzy", " ", " ", " ", "0", "en", "-1"])
        .current_dir(&dir.0)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(fs::read_to_string(dir.0.join("exited")).unwrap(), "eof\n");
}

#[test]
fn invalid_invocation_fails_without_creating_runtime_files() {
    let dir = Scratch::new("arguments");
    let output = Command::new(env!("CARGO_BIN_EXE_fake-readboard"))
        .current_dir(&dir.0)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!dir.0.join("commands.log").exists());
}
