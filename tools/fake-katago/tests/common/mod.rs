#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

pub const SAFETY_TIMEOUT: Duration = Duration::from_secs(5);

pub struct Engine {
    child: Child,
    input: Option<ChildStdin>,
    output: Receiver<Option<String>>,
    stderr: Option<JoinHandle<String>>,
    pub started: Instant,
}

impl Engine {
    pub fn spawn(args: &[&str]) -> Self {
        Self::spawn_at(env!("CARGO_BIN_EXE_fake-katago"), args)
    }

    pub fn spawn_at(path: impl AsRef<std::ffi::OsStr>, args: &[&str]) -> Self {
        let started = Instant::now();
        let mut child = Command::new(path)
            .args([
                "analysis",
                "-config",
                "unused.cfg",
                "-model",
                "unused.bin.gz",
            ])
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("fake-katago must start");
        let input = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (tx, output) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if tx
                    .send(Some(line.expect("stdout must be readable")))
                    .is_err()
                {
                    return;
                }
            }
            let _ = tx.send(None);
        });
        let stderr = thread::spawn(move || {
            BufReader::new(stderr)
                .lines()
                .map(|line| line.expect("stderr must be readable"))
                .collect::<Vec<_>>()
                .join("\n")
        });
        Self {
            child,
            input,
            output,
            stderr: Some(stderr),
            started,
        }
    }

    pub fn send(&mut self, value: &Value) {
        self.send_raw(&value.to_string());
    }

    pub fn send_raw(&mut self, line: &str) {
        let input = self.input.as_mut().expect("stdin is open");
        writeln!(input, "{line}").expect("write request");
        input.flush().expect("flush request");
    }

    pub fn raw(&self) -> Option<String> {
        self.output
            .recv_timeout(SAFETY_TIMEOUT.saturating_sub(self.started.elapsed()))
            .expect("engine must produce output or EOF within safety deadline")
    }

    pub fn recv(&self) -> Value {
        let line = self.raw().expect("expected protocol line, got EOF");
        katago_protocol::parse_line(&line)
            .unwrap_or_else(|error| panic!("unparseable fake output {line}: {error}"));
        serde_json::from_str(&line).unwrap()
    }

    pub fn ready(&mut self) {
        self.send(&json!({"id":"ready", "action":"query_version"}));
        let reply = self.recv();
        assert_eq!(reply["id"], "ready");
        assert_eq!(reply["action"], "query_version");
        assert!(reply["version"].as_str().unwrap().contains("fake"));
    }

    pub fn close_input(&mut self) {
        self.input.take();
    }

    /// Call only after EOF, so wait has no unbounded synchronization role.
    pub fn exited(&mut self) -> (ExitStatus, String) {
        let status = self.child.wait().expect("wait after EOF");
        let stderr = self.stderr.take().unwrap().join().unwrap();
        (status, stderr)
    }

    pub fn finish(&mut self) {
        self.close_input();
        assert_eq!(self.raw(), None, "unexpected trailing output");
        let (status, stderr) = self.exited();
        assert!(status.success(), "status {status}; stderr {stderr}");
        assert!(stderr.is_empty(), "unexpected stderr: {stderr}");
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn query(id: &str) -> Value {
    json!({
        "id": id, "moves": [], "rules": "chinese", "komi": 7.5,
        "boardXSize": 19, "boardYSize": 19, "maxVisits": 10
    })
}

pub fn stream(id: &str) -> Value {
    let mut request = query(id);
    request["maxVisits"] = json!(100_000);
    request["reportDuringSearchEvery"] = json!(0.02);
    request["firstReportDuringSearchAfter"] = json!(0.001);
    request
}
