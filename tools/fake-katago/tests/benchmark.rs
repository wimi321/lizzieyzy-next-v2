#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::{
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use common::Engine;
use katago_protocol::benchmark::BenchmarkParser;

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fake-katago"));
    command.args([
        "benchmark",
        "-config",
        "unused.cfg",
        "-model",
        "unused.bin.gz",
        "-v",
        "800",
        "-n",
        "6",
        "-time",
        "5",
        "-s",
    ]);
    command
}

struct TempDirectory(PathBuf);

impl TempDirectory {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("fake-benchmark-{}-{unique}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn benchmark_replays_real_recording_without_changing_cr_bytes() {
    let output = command()
        .args([
            "-override-config",
            "numAnalysisThreads=,numSearchThreadsPerAnalysisThread=,numSearchThreads=1",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        output.stdout,
        include_bytes!("../../../fixtures/katago/benchmark-tune-b11-metal.txt")
    );
    let mut parser = BenchmarkParser::default();
    assert_eq!(parser.push(&output.stdout).unwrap().len(), 30);
    assert_eq!(parser.finish().unwrap().threads, 6);
}

#[test]
fn benchmark_honors_replay_duration_and_failure_controls() {
    let temp = TempDirectory::new();
    let replay = temp.0.join("replay.txt");
    fs::write(&replay, "numSearchThreads = 2: 1 / 1 positions, visits/s = 120 nnEvals/s = 110\r\nnumSearchThreads = 2: (recommended)\n").unwrap();
    let started = Instant::now();
    let output = command()
        .arg("--benchmark-replay")
        .arg(replay)
        .args(["--benchmark-duration-ms", "25"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(started.elapsed() >= Duration::from_millis(25));
    let mut parser = BenchmarkParser::default();
    parser.push(&output.stdout).unwrap();
    assert_eq!(parser.finish().unwrap().threads, 2);

    let output = command().arg("--benchmark-fail").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("injected benchmark failure")
    );
}

#[test]
fn benchmark_gate_emits_progress_then_can_be_killed_and_reaped() {
    let temp = TempDirectory::new();
    let replay = temp.0.join("replay.txt");
    let pid = temp.0.join("pid");
    fs::write(&replay, "numSearchThreads = 2: 0 / 1 positions, visits/s = nan\nnumSearchThreads = 2: 1 / 1 positions, visits/s = 120 nnEvals/s = 110\nnumSearchThreads = 2: (recommended)\n").unwrap();
    let mut process = Process(
        command()
            .arg("--benchmark-replay")
            .arg(replay)
            .arg("--benchmark-gate-file")
            .arg(temp.0.join("gate"))
            .arg("--benchmark-pid-file")
            .arg(&pid)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let stdout = process.0.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            sender.send(Some(line.unwrap())).unwrap();
        }
        sender.send(None).unwrap();
    });
    let first = receiver
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert!(first.contains("0 / 1 positions"));
    assert_eq!(fs::read_to_string(pid).unwrap(), process.0.id().to_string());
    assert!(process.0.try_wait().unwrap().is_none());
    process.0.kill().unwrap();
    assert_eq!(receiver.recv_timeout(Duration::from_secs(5)).unwrap(), None);
    assert!(!process.0.wait().unwrap().success());
    reader.join().unwrap();
}

#[test]
fn analysis_ignores_benchmark_controls_in_shared_profile_args() {
    let mut engine = Engine::spawn(&[
        "--benchmark-replay",
        "missing.txt",
        "--benchmark-duration-ms",
        "60000",
        "--benchmark-fail",
        "--benchmark-gate-file",
        "missing-gate",
        "--benchmark-pid-file",
        "unused-pid",
    ]);
    engine.ready();
    engine.finish();
}

#[test]
fn launch_log_records_actual_arguments_for_both_subcommands() {
    let temp = TempDirectory::new();
    let log = temp.0.join("launch.jsonl");
    let path = log.to_str().unwrap();
    let mut engine = Engine::spawn(&[
        "--launch-log",
        path,
        "-override-config",
        "numSearchThreadsPerAnalysisThread=6",
    ]);
    engine.ready();
    engine.finish();
    assert!(
        command()
            .arg("--launch-log")
            .arg(&log)
            .output()
            .unwrap()
            .status
            .success()
    );
    let rows: Vec<Vec<String>> = fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][0], "analysis");
    assert!(
        rows[0]
            .iter()
            .any(|arg| arg == "numSearchThreadsPerAnalysisThread=6")
    );
    assert_eq!(rows[1][0], "benchmark");
    assert!(rows[1].iter().any(|arg| arg == "-s"));
}
