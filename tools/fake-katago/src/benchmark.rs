//! Recorded-output simulator only; this binary never runs an actual benchmark.

use std::{
    fs,
    io::{self, Write},
    time::{Duration, Instant},
};

use crate::{RunError, cli::Options};

const RECORDING: &[u8] = include_bytes!("../../../fixtures/katago/benchmark-tune-b11-metal.txt");

pub fn run(options: &Options) -> Result<(), RunError> {
    if let Some(path) = &options.benchmark_pid_file {
        fs::write(path, std::process::id().to_string())?;
    }
    if options.benchmark_fail {
        return Err(RunError::BenchmarkFailure);
    }
    let recording = match &options.benchmark_replay {
        Some(path) => fs::read(path)?,
        None => RECORDING.to_vec(),
    };
    let fragments: Vec<_> = recording
        .split_inclusive(|byte| *byte == b'\r' || *byte == b'\n')
        .collect();
    let started = Instant::now();
    let duration = Duration::from_millis(options.benchmark_duration_ms);
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let mut gate_waited = false;
    for (index, fragment) in fragments.iter().enumerate() {
        let progress = index as f64 / fragments.len().saturating_sub(1).max(1) as f64;
        let remaining = duration.mul_f64(progress).saturating_sub(started.elapsed());
        if !remaining.is_zero() {
            std::thread::sleep(remaining);
        }
        output.write_all(fragment)?;
        output.flush()?;
        if !gate_waited
            && fragment
                .windows(b"positions,".len())
                .any(|part| part == b"positions,")
        {
            gate_waited = true;
            if let Some(path) = &options.benchmark_gate_file {
                loop {
                    match fs::metadata(path) {
                        Ok(metadata) if metadata.is_file() => break,
                        Ok(_) => {
                            return Err(io::Error::other("benchmark gate is not a file").into());
                        }
                        Err(error) if error.kind() == io::ErrorKind::NotFound => {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => return Err(error.into()),
                    }
                }
            }
        }
    }
    Ok(())
}
