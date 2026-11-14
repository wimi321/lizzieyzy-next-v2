use std::{collections::BTreeSet, path::PathBuf};

use thiserror::Error;

pub const USAGE: &str = "Usage: fake-katago analysis|benchmark -config FILE -model FILE [OPTIONS]\n\
Options:\n\
  --visits-per-sec N       Visits per simulated second (default 1000)\n\
  --time-scale N           Simulated seconds per real second (default 1)\n\
  --startup-delay-ms N     Delay readiness in simulated milliseconds\n\
  --startup-gate-file PATH Wait for an externally created file before readiness\n\
  --request-log PATH      Append received protocol lines for behavioral assertions\n\
  --launch-log PATH       Append JSON argv for each analysis/benchmark process\n\
  --exit-log PATH         Append a process ID after the engine loop exits\n\
  --play-script PATH      Per-turn JSON evaluations, candidates and injected failures\n\
  -human-model FILE       Enable deterministic HumanSL responses\n\
  --fail-startup           Report a missing model and exit with code 1\n\
  --fail-human-model       Fail startup only when -human-model is supplied\n\
  --crash-after-queries N  Exit on accepting the Nth analysis query\n\
  --crash-after-ms N       Exit after N simulated milliseconds\n\
  --hang-after-queries N   Silence output from the Nth analysis query\n\
  --fault-once-file PATH   Inject crash/hang/startup faults only when creating this marker\n\
  --garbage-every N        Insert an invalid line after every N output lines\n\
  --threads N              Concurrent searches (overrides numAnalysisThreads, default 1)\n\
  -override-config K=V,... One comma-separated list of configuration overrides\n\
  --default-perspective-to-play  Simulate an unset reportAnalysisWinratesAs\n\
Benchmark options (accepted but ignored by analysis):\n\
  -v N -n N -time N -s     Official benchmark flags (accepted, replay is unchanged)\n\
  --benchmark-replay PATH   Replay this output (default: recorded Metal/B11 benchmark)\n\
  --benchmark-duration-ms N Pace the replay over this many milliseconds (default 0)\n\
  --benchmark-gate-file PATH Wait after the first progress row until this file exists\n\
  --benchmark-fail          Report an injected failure and exit with code 1\n\
  --benchmark-pid-file PATH Write process ID before emitting output\n\
Config/model paths are accepted but are not read. Zero query thresholds fire immediately.";

#[derive(Debug, Error)]
#[error("{0}")]
pub struct CliError(pub String);

#[derive(Clone, Copy, Debug)]
pub enum ReportPerspective {
    Black,
    White,
    ToPlay,
}

#[derive(Debug)]
pub struct Options {
    pub benchmark: bool,
    pub benchmark_replay: Option<PathBuf>,
    pub benchmark_duration_ms: u64,
    pub benchmark_gate_file: Option<PathBuf>,
    pub benchmark_fail: bool,
    pub benchmark_pid_file: Option<PathBuf>,
    pub visits_per_sec: f64,
    pub time_scale: f64,
    pub startup_delay_ms: u64,
    pub startup_gate_file: Option<PathBuf>,
    pub request_log: Option<PathBuf>,
    pub launch_log: Option<PathBuf>,
    pub exit_log: Option<PathBuf>,
    pub play_script: Option<PathBuf>,
    pub human_model: bool,
    pub fail_startup: bool,
    pub fail_human_model: bool,
    pub crash_after_queries: Option<u64>,
    pub crash_after_ms: Option<u64>,
    pub hang_after_queries: Option<u64>,
    pub fault_once_file: Option<PathBuf>,
    pub garbage_every: Option<u64>,
    pub threads: usize,
    pub default_perspective_to_play: bool,
    pub report_perspective: Option<ReportPerspective>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            benchmark: false,
            benchmark_replay: None,
            benchmark_duration_ms: 0,
            benchmark_gate_file: None,
            benchmark_fail: false,
            benchmark_pid_file: None,
            visits_per_sec: 1000.0,
            time_scale: 1.0,
            startup_delay_ms: 0,
            startup_gate_file: None,
            request_log: None,
            launch_log: None,
            exit_log: None,
            play_script: None,
            human_model: false,
            fail_startup: false,
            fail_human_model: false,
            crash_after_queries: None,
            crash_after_ms: None,
            hang_after_queries: None,
            fault_once_file: None,
            garbage_every: None,
            threads: 1,
            default_perspective_to_play: false,
            report_perspective: None,
        }
    }
}

impl Options {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Self>, CliError> {
        let mut args = args.into_iter();
        let Some(command) = args.next() else {
            return Ok(None);
        };
        if command == "--help" || command == "-h" {
            return Ok(None);
        }
        if command != "analysis" && command != "benchmark" {
            return Err(CliError(
                "expected the analysis or benchmark subcommand".into(),
            ));
        }
        let mut opts = Self {
            benchmark: command == "benchmark",
            ..Self::default()
        };
        let mut seen = BTreeSet::new();
        let mut explicit_threads = None;
        while let Some(key) = args.next() {
            if key == "--help" || key == "-h" {
                return Ok(None);
            }
            // Match KataGo's single-use TCLAP arguments: a second occurrence
            // is invalid even if both occurrences carry the same value.
            if !seen.insert(key.clone()) {
                return Err(CliError(format!("Argument already set: {key}")));
            }
            if key == "--fail-startup" {
                opts.fail_startup = true;
                continue;
            }
            if key == "--fail-human-model" {
                opts.fail_human_model = true;
                continue;
            }
            if key == "--default-perspective-to-play" {
                opts.default_perspective_to_play = true;
                continue;
            }
            if key == "--benchmark-fail" {
                opts.benchmark_fail = true;
                continue;
            }
            if key == "-s" && opts.benchmark {
                continue;
            }
            let value = args
                .next()
                .ok_or_else(|| CliError(format!("missing value for {key}")))?;
            match key.as_str() {
                "-config" | "-model" => {}
                "-human-model" => opts.human_model = true,
                "-override-config" => opts.override_config(&value)?,
                "-v" | "-n" | "-time" if opts.benchmark => {
                    positive_number(&key, &value)?;
                }
                "--benchmark-replay" => opts.benchmark_replay = Some(value.into()),
                "--benchmark-duration-ms" => {
                    let millis = integer(&key, &value)?;
                    if millis > 86_400_000 {
                        return Err(CliError(
                            "--benchmark-duration-ms must be at most one day".into(),
                        ));
                    }
                    opts.benchmark_duration_ms = millis;
                }
                "--benchmark-gate-file" => opts.benchmark_gate_file = Some(value.into()),
                "--benchmark-pid-file" => opts.benchmark_pid_file = Some(value.into()),
                "--visits-per-sec" => opts.visits_per_sec = positive_number(&key, &value)?,
                "--time-scale" => opts.time_scale = positive_number(&key, &value)?,
                "--startup-delay-ms" => opts.startup_delay_ms = integer(&key, &value)?,
                "--startup-gate-file" => opts.startup_gate_file = Some(value.into()),
                "--request-log" => opts.request_log = Some(value.into()),
                "--launch-log" => opts.launch_log = Some(value.into()),
                "--exit-log" => opts.exit_log = Some(value.into()),
                "--play-script" => opts.play_script = Some(value.into()),
                "--crash-after-queries" => opts.crash_after_queries = Some(integer(&key, &value)?),
                "--crash-after-ms" => opts.crash_after_ms = Some(integer(&key, &value)?),
                "--hang-after-queries" => opts.hang_after_queries = Some(integer(&key, &value)?),
                "--fault-once-file" => opts.fault_once_file = Some(value.into()),
                "--garbage-every" => {
                    let n = integer(&key, &value)?;
                    if n == 0 {
                        return Err(CliError(format!("{key} must be positive")));
                    }
                    opts.garbage_every = Some(n);
                }
                "--threads" => {
                    let n = integer(&key, &value)?;
                    if n == 0 || n > 1024 {
                        return Err(CliError("--threads must be in 1..=1024".into()));
                    }
                    opts.threads = n as usize;
                    explicit_threads = Some(n as usize);
                }
                _ => return Err(CliError(format!("unknown option {key}"))),
            }
        }
        if let Some(threads) = explicit_threads {
            opts.threads = threads;
        }
        Ok(Some(opts))
    }

    pub fn perspective(&self) -> ReportPerspective {
        self.report_perspective
            .unwrap_or(if self.default_perspective_to_play {
                ReportPerspective::ToPlay
            } else {
                ReportPerspective::Black
            })
    }

    fn override_config(&mut self, value: &str) -> Result<(), CliError> {
        for assignment in value.split(',') {
            let Some((key, value)) = assignment.split_once('=') else {
                return Err(CliError(
                    "-override-config expects KEY=VALUE[,KEY=VALUE…]".into(),
                ));
            };
            let (key, value) = (key.trim(), value.trim());
            if key.is_empty() {
                return Err(CliError("-override-config requires nonempty keys".into()));
            }
            // KataGo treats KEY= as removal of that setting. In the fake,
            // removing a simulated key restores its pre-override default.
            if value.is_empty() {
                if key == "reportAnalysisWinratesAs" {
                    self.report_perspective = None;
                }
                if key == "numAnalysisThreads" {
                    self.threads = 1;
                }
                continue;
            }
            if key == "reportAnalysisWinratesAs" {
                self.report_perspective = Some(match value.to_ascii_uppercase().as_str() {
                    "BLACK" => ReportPerspective::Black,
                    "WHITE" => ReportPerspective::White,
                    "SIDETOMOVE" => ReportPerspective::ToPlay,
                    _ => {
                        return Err(CliError(format!(
                            "invalid reportAnalysisWinratesAs: {value}"
                        )));
                    }
                });
            }
            if key == "numAnalysisThreads" {
                let threads = integer(key, value)?;
                if threads == 0 || threads > 1024 {
                    return Err(CliError("numAnalysisThreads must be in 1..=1024".into()));
                }
                self.threads = threads as usize;
            }
        }
        Ok(())
    }
}

fn positive_number(key: &str, value: &str) -> Result<f64, CliError> {
    let number: f64 = value
        .parse()
        .map_err(|_| CliError(format!("invalid number for {key}")))?;
    if !number.is_finite() || !(1e-9..=1e9).contains(&number) {
        return Err(CliError(format!("{key} must be finite and in 1e-9..=1e9")));
    }
    Ok(number)
}

fn integer(key: &str, value: &str) -> Result<u64, CliError> {
    value
        .parse()
        .map_err(|_| CliError(format!("invalid nonnegative integer for {key}")))
}

#[cfg(test)]
mod tests;
