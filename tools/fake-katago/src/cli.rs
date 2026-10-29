use thiserror::Error;

pub const USAGE: &str = "Usage: fake-katago analysis -config FILE -model FILE [OPTIONS]\n\
Options:\n\
  --visits-per-sec N       Visits per simulated second (default 1000)\n\
  --time-scale N           Simulated seconds per real second (default 1)\n\
  --startup-delay-ms N     Delay readiness in simulated milliseconds\n\
  --fail-startup           Report a missing model and exit with code 1\n\
  --crash-after-queries N  Exit on accepting the Nth analysis query\n\
  --crash-after-ms N       Exit after N simulated milliseconds\n\
  --hang-after-queries N   Silence output from the Nth analysis query\n\
  --garbage-every N        Insert an invalid line after every N output lines\n\
  --threads N              Concurrent searches (default 1)\n\
Config/model paths are accepted but are not read. Zero query thresholds fire immediately.";

#[derive(Debug, Error)]
#[error("{0}")]
pub struct CliError(pub String);

#[derive(Debug)]
pub struct Options {
    pub visits_per_sec: f64,
    pub time_scale: f64,
    pub startup_delay_ms: u64,
    pub fail_startup: bool,
    pub crash_after_queries: Option<u64>,
    pub crash_after_ms: Option<u64>,
    pub hang_after_queries: Option<u64>,
    pub garbage_every: Option<u64>,
    pub threads: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            visits_per_sec: 1000.0,
            time_scale: 1.0,
            startup_delay_ms: 0,
            fail_startup: false,
            crash_after_queries: None,
            crash_after_ms: None,
            hang_after_queries: None,
            garbage_every: None,
            threads: 1,
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
        if command != "analysis" {
            return Err(CliError("expected the analysis subcommand".into()));
        }
        let mut opts = Self::default();
        while let Some(key) = args.next() {
            if key == "--help" || key == "-h" {
                return Ok(None);
            }
            if key == "--fail-startup" {
                opts.fail_startup = true;
                continue;
            }
            let value = args
                .next()
                .ok_or_else(|| CliError(format!("missing value for {key}")))?;
            match key.as_str() {
                "-config" | "-model" => {}
                "--visits-per-sec" => opts.visits_per_sec = positive_number(&key, &value)?,
                "--time-scale" => opts.time_scale = positive_number(&key, &value)?,
                "--startup-delay-ms" => opts.startup_delay_ms = integer(&key, &value)?,
                "--crash-after-queries" => opts.crash_after_queries = Some(integer(&key, &value)?),
                "--crash-after-ms" => opts.crash_after_ms = Some(integer(&key, &value)?),
                "--hang-after-queries" => opts.hang_after_queries = Some(integer(&key, &value)?),
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
                }
                _ => return Err(CliError(format!("unknown option {key}"))),
            }
        }
        Ok(Some(opts))
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
