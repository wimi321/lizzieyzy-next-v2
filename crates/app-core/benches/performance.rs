//! Headless, event-complete AppCore benchmarks. Never linked into the desktop app.

mod memory;
mod scenarios;
mod support;

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use serde::{Deserialize, Serialize};
use support::{Limits, Report, Scenario, ScenarioResult};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, thiserror::Error)]
enum BenchmarkError {
    #[error("invalid benchmark arguments: {0}")]
    Arguments(String),
    #[error("benchmark contract failed: {0}")]
    Contract(&'static str),
    #[error("worker {scenario} failed ({status}): {stderr}")]
    Worker {
        scenario: String,
        status: String,
        stderr: String,
    },
    #[error("performance limits exceeded:\n{0}")]
    Limits(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct Sample {
    scenario: Scenario,
    elapsed_ms: f64,
    peak_rss_bytes: Option<u64>,
}

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("performance: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--worker") {
        if args.len() != 4 {
            return Err(
                BenchmarkError::Arguments("--worker SCENARIO FIXTURE ENGINE".into()).into(),
            );
        }
        let scenario = Scenario::ALL
            .into_iter()
            .find(|scenario| args[1] == scenario.as_str())
            .ok_or_else(|| BenchmarkError::Arguments("unknown worker scenario".into()))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        // A safety net, never a synchronization mechanism. No time is paused.
        let sample = runtime.block_on(async {
            tokio::time::timeout(
                std::time::Duration::from_secs(60),
                scenarios::measure(scenario, Path::new(&args[2]), Path::new(&args[3])),
            )
            .await?
        })?;
        println!("{}", serde_json::to_string(&sample)?);
        return Ok(());
    }

    let mut check = false;
    let mut summary = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--check" {
            check = true;
        } else if arg == "--summary" {
            summary = Some(PathBuf::from(args.next().ok_or_else(|| {
                BenchmarkError::Arguments("--summary needs a file".into())
            })?));
        } else if arg == "--bench" {
            // Cargo adds this flag when launching a harness=false bench.
        } else if arg == "--help" || arg == "-h" {
            println!(
                "cargo bench -p app-core --bench performance --locked -- [--check] [--summary FILE]"
            );
            return Ok(());
        } else {
            return Err(BenchmarkError::Arguments(arg.to_string_lossy().into_owned()).into());
        }
    }

    // Build the fake before any worker/timer. Children receive the resolved
    // path so cargo's own memory and build time cannot enter the measurements.
    let engine = fake_katago::binary_path()?;
    let inputs = tempfile::tempdir()?;
    let reviewed = inputs.path().join("reviewed.sgf");
    let tree = inputs.path().join("large-tree.sgf");
    for (path, text) in [
        (&reviewed, support::reviewed_game()?),
        (&tree, support::large_tree()?),
    ] {
        // Validate generated SGF in the parent. Workers measure only the
        // selected fixture and do not inherit generator/parser allocations.
        let collection = sgf::parse(text.as_bytes())?;
        if collection.games.len() != 1 || !collection.warnings.is_empty() {
            return Err(BenchmarkError::Contract("generated SGF is not one clean game").into());
        }
        fs::write(path, text)?;
    }

    let executable = env::current_exe()?;
    let mut results = Vec::new();
    for scenario in Scenario::ALL {
        eprintln!("Measuring {} (1 warmup + 7 samples)", scenario.as_str());
        let fixture = if scenario == Scenario::OpenLargeTree {
            &tree
        } else {
            &reviewed
        };
        let mut times = Vec::new();
        let mut memory = Vec::new();
        for iteration in 0..8 {
            let output = Command::new(&executable)
                .arg("--worker")
                .arg(scenario.as_str())
                .arg(fixture)
                .arg(&engine)
                .output()?;
            if !output.status.success() {
                return Err(BenchmarkError::Worker {
                    scenario: scenario.as_str().into(),
                    status: output.status.to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                }
                .into());
            }
            let sample: Sample = serde_json::from_slice(&output.stdout)?;
            let expects_memory =
                scenario.has_memory() && cfg!(any(target_os = "macos", target_os = "linux"));
            if sample.scenario != scenario || sample.peak_rss_bytes.is_some() != expects_memory {
                return Err(BenchmarkError::Contract("worker returned an unrelated sample").into());
            }
            if iteration > 0 {
                times.push(sample.elapsed_ms);
                memory.extend(sample.peak_rss_bytes);
            }
        }
        results.push(ScenarioResult::new(scenario, times, memory)?);
    }
    let report = Report {
        platform: env::consts::OS.into(),
        architecture: env::consts::ARCH.into(),
        warmups: 1,
        repetitions: 7,
        results,
    };
    let output = workspace().join("target/bench/performance.json");
    report.write_json(&output)?;
    let report = Report::read_json(&output)?;
    let table = report.markdown_table();
    println!("{table}");
    if let Some(path) = summary {
        let path = if path.is_absolute() {
            path
        } else {
            workspace().join(path)
        };
        report.append_summary(&path)?;
    }
    // Write evidence before checking, so failed CI runs still publish results.
    if check {
        let limits = Limits::read_json(&workspace().join("crates/app-core/benches/limits.json"))?;
        let exceeded = limits.check(&report)?;
        if !exceeded.is_empty() {
            return Err(BenchmarkError::Limits(exceeded.join("\n")).into());
        }
        println!("performance: limits passed");
    }
    Ok(())
}
