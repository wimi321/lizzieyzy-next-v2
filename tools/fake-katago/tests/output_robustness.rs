#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

struct RawEngine {
    child: Child,
    input: Option<ChildStdin>,
    output: Receiver<Option<Vec<u8>>>,
    diagnostics: Receiver<Vec<u8>>,
    started: Instant,
}

impl RawEngine {
    fn spawn(args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_fake-katago"))
            .arg("analysis")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();
        let (sent, output) = mpsc::channel();
        std::thread::spawn(move || {
            let mut stdout = BufReader::new(stdout);
            loop {
                let mut bytes = Vec::new();
                if stdout.read_until(b'\n', &mut bytes).unwrap() == 0 {
                    let _ = sent.send(None);
                    break;
                }
                if sent.send(Some(bytes)).is_err() {
                    break;
                }
            }
        });
        let (sent, diagnostics) = mpsc::channel();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stderr.read_to_end(&mut bytes).unwrap();
            let _ = sent.send(bytes);
        });
        Self {
            child,
            input,
            output,
            diagnostics,
            started: Instant::now(),
        }
    }

    fn remaining(&self) -> Duration {
        Duration::from_secs(5).saturating_sub(self.started.elapsed())
    }

    fn send(&mut self, value: Value) {
        let input = self.input.as_mut().unwrap();
        writeln!(input, "{value}").unwrap();
        input.flush().unwrap();
    }

    fn raw(&self) -> Option<Vec<u8>> {
        self.output
            .recv_timeout(self.remaining())
            .expect("raw engine output within five-second safety deadline")
    }

    fn response(&self) -> Value {
        serde_json::from_slice(&self.raw().expect("expected a response")).unwrap()
    }

    fn ready(&mut self) {
        self.send(json!({"id":"ready", "action":"query_version"}));
        assert_eq!(self.response()["id"], "ready");
    }

    fn query(&mut self, id: &str) {
        self.send(json!({
            "id":id, "moves":[], "rules":"chinese", "komi":7.5,
            "boardXSize":19, "boardYSize":19, "maxVisits":1
        }));
    }

    fn finish(&mut self) -> Vec<u8> {
        self.input.take();
        assert_eq!(self.raw(), None);
        let diagnostics = self
            .diagnostics
            .recv_timeout(self.remaining())
            .expect("stderr EOF within five-second safety deadline");
        assert!(self.child.wait().unwrap().success());
        diagnostics
    }
}

impl Drop for RawEngine {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn startup_gbk_path_preserves_literal_bytes_and_readiness() {
    let mut engine = RawEngine::spawn(&["--startup-gbk-path"]);
    engine.ready();
    assert_eq!(
        engine.finish(),
        b"Loading model C:\\Users\\\xD5\xC5\xC8\xFD\\katago\\model.bin.gz\n"
    );
}

#[test]
fn padded_stdout_is_exact_length_and_only_before_selected_query() {
    for bytes in [8 * 1024 * 1024, 9 * 1024 * 1024] {
        let size = bytes.to_string();
        let mut engine = RawEngine::spawn(&[
            "--stdout-line-before-query",
            "2",
            "--stdout-line-bytes",
            &size,
        ]);
        engine.ready();
        engine.query("one");
        assert_eq!(engine.response()["id"], "one");
        engine.query("two");
        let line = engine.raw().unwrap();
        assert_eq!(line.len(), bytes + 1);
        assert_eq!(line.last(), Some(&b'\n'));
        assert_eq!(
            serde_json::from_slice::<Value>(&line).unwrap(),
            json!({"warning":"fake padded stdout"})
        );
        assert_eq!(engine.response()["id"], "two");
        engine.query("three");
        assert_eq!(engine.response()["id"], "three");
        assert!(engine.finish().is_empty());
    }
}

#[test]
fn invalid_stdout_is_one_line_before_selected_query() {
    let mut engine = RawEngine::spawn(&["--stdout-invalid-before-query", "2"]);
    engine.ready();
    engine.query("one");
    assert_eq!(engine.response()["id"], "one");
    engine.query("two");
    assert_eq!(engine.raw().unwrap(), b"\xFF\n");
    assert_eq!(engine.response()["id"], "two");
    assert!(engine.finish().is_empty());
}

#[test]
fn invalid_stderr_and_exact_flood_are_emitted_before_first_query_reply() {
    let bytes = 1024 * 1024;
    let mut engine = RawEngine::spawn(&["--stderr-invalid-flood-bytes", &bytes.to_string()]);
    engine.ready();
    engine.query("one");
    assert_eq!(engine.response()["id"], "one");
    engine.query("two");
    assert_eq!(engine.response()["id"], "two");
    let diagnostics = engine.finish();
    assert!(diagnostics.starts_with(b"\xFF\n"));
    assert!(diagnostics[2..bytes + 2].iter().all(|&byte| byte == b'f'));
    assert_eq!(&diagnostics[bytes + 2..], b"\nfake stderr flood complete\n");
}
