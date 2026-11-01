//! Incremental decoder for KataGo's human-readable official benchmark output.
//!
//! Progress uses carriage returns, not just newlines. Speed maxima are not a
//! recommendation: KataGo also accounts for the strength cost of more threads.

use serde::{Deserialize, Serialize};

mod additional;

const MAX_FRAGMENT_BYTES: usize = 64 * 1024;
const MAX_THREADS: u32 = 4096;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkProgress {
    pub threads: u32,
    pub completed_positions: u32,
    pub total_positions: u32,
    pub visits_per_second: Option<f64>,
    pub nn_evals_per_second: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkMetrics {
    pub threads: u32,
    pub completed_positions: u32,
    pub total_positions: u32,
    pub visits_per_second: f64,
    pub nn_evals_per_second: f64,
    #[serde(default)]
    pub nn_max_batch_size: Option<u32>,
    #[serde(default)]
    pub num_nn_server_threads_per_model: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BenchmarkParseError {
    #[error("explicit benchmark threads must be between 0 and 4096")]
    InvalidExplicitThreads,
    #[error("invalid benchmark recommendation for {0}")]
    InvalidRecommendation(&'static str),
    #[error("benchmark output fragment exceeds 65536 bytes")]
    FragmentTooLong,
    #[error("benchmark output is not UTF-8")]
    InvalidUtf8,
    #[error("benchmark did not report a recommended thread count")]
    MissingRecommendation,
    #[error("benchmark recommended conflicting thread counts")]
    ConflictingRecommendations,
    #[error("benchmark recommendation has no complete, finite, positive speed measurements")]
    MissingMetrics,
    #[error("benchmark has no complete measurement for the adopted additional settings")]
    MissingAdoptedMetrics,
    #[error("benchmark emitted too many additional setting measurements")]
    TooManyAdditionalMeasurements,
    #[error("benchmark reported failure: {0}")]
    ReportedFailure(String),
}

/// A bounded streaming parser. `finish` must only be used after a successful
/// process exit; a recommendation printed before a crash is not a success.
#[derive(Default, Debug)]
pub struct BenchmarkParser {
    fragment: Vec<u8>,
    metrics: Vec<BenchmarkMetrics>,
    recommended: Option<u32>,
    explicit_threads: Option<u32>,
    nn_max_batch_size: Option<u32>,
    num_nn_server_threads_per_model: Option<u32>,
    server_thread_recommendation_block: bool,
    additional_tests: bool,
    additional: additional::Measurements,
    failure: Option<String>,
    gpu_tuning: bool,
    gpu_tuning_error: Option<String>,
}

impl BenchmarkParser {
    /// Only callers that actually requested `-t N` may provide a fallback.
    /// Automatic `-s` tuning must use Default and require an official marker.
    pub fn with_explicit_threads(threads: u32) -> Result<Self, BenchmarkParseError> {
        if threads > MAX_THREADS {
            return Err(BenchmarkParseError::InvalidExplicitThreads);
        }
        Ok(Self {
            explicit_threads: (threads > 0).then_some(threads),
            ..Self::default()
        })
    }

    /// Feed arbitrary byte chunks from one ordered output stream.
    /// CR, LF, and CRLF delimit fragments; partial UTF-8 is retained until then.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<BenchmarkProgress>, BenchmarkParseError> {
        let mut updates = Vec::new();
        for &byte in bytes {
            if byte == b'\r' || byte == b'\n' {
                if let Some(progress) = self.consume_fragment()? {
                    updates.push(progress);
                }
            } else {
                if self.fragment.len() == MAX_FRAGMENT_BYTES {
                    return Err(BenchmarkParseError::FragmentTooLong);
                }
                self.fragment.push(byte);
            }
        }
        Ok(updates)
    }

    /// Return measured speeds for the officially adopted settings, never the
    /// fastest unrelated row. The original search table remains unchanged.
    pub fn finish(&mut self) -> Result<BenchmarkMetrics, BenchmarkParseError> {
        self.finish_with_ignored_settings(false, false)
    }

    /// A same-valued explicit CLI override adopts the recommendation's settings
    /// too; a conflicting one keeps the user's value and excludes that advice
    /// from selection. Raw values come from the caller's merged CLI override.
    pub fn finish_with_overrides(
        &mut self,
        batch: Option<&str>,
        server: Option<&str>,
    ) -> Result<BenchmarkMetrics, BenchmarkParseError> {
        self.consume_fragment()?;
        let conflicts = |value: Option<&str>, recommended: Option<u32>| {
            value.is_some_and(|value| value.trim().parse::<u32>().ok() != recommended)
        };
        self.finish_with_ignored_settings(
            conflicts(batch, self.nn_max_batch_size),
            conflicts(server, self.num_nn_server_threads_per_model),
        )
    }

    /// Select the measured subset actually adopted by the caller. Advice fields
    /// still describe the official recommendation, preserving result metadata.
    /// Missing combined measurements fail rather than borrow an unrelated speed.
    pub fn finish_with_ignored_settings(
        &mut self,
        ignore_batch: bool,
        ignore_server: bool,
    ) -> Result<BenchmarkMetrics, BenchmarkParseError> {
        self.consume_fragment()?;
        if let Some(reason) = self.failure.as_ref().or(self.gpu_tuning_error.as_ref()) {
            return Err(BenchmarkParseError::ReportedFailure(reason.clone()));
        }
        let threads = self
            .recommended
            .or(self.explicit_threads)
            .ok_or(BenchmarkParseError::MissingRecommendation)?;
        let main = self
            .metrics
            .iter()
            .find(|row| row.threads == threads)
            .cloned()
            .ok_or(BenchmarkParseError::MissingMetrics)?;
        let adopted_batch = self.nn_max_batch_size.filter(|_| !ignore_batch);
        let adopted_server = self.num_nn_server_threads_per_model.is_some() && !ignore_server;
        let mut metrics = if adopted_batch.is_some() || adopted_server {
            self.additional
                .find(threads, adopted_batch, adopted_server)
                .cloned()
                .ok_or(BenchmarkParseError::MissingAdoptedMetrics)?
        } else {
            main
        };
        metrics.nn_max_batch_size = self.nn_max_batch_size;
        metrics.num_nn_server_threads_per_model = self.num_nn_server_threads_per_model;
        Ok(metrics)
    }

    /// Complete rows only, in first-observed order, one row per thread count.
    pub fn metrics(&self) -> &[BenchmarkMetrics] {
        &self.metrics
    }

    fn consume_fragment(&mut self) -> Result<Option<BenchmarkProgress>, BenchmarkParseError> {
        let bytes = std::mem::take(&mut self.fragment);
        let line = std::str::from_utf8(&bytes)
            .map_err(|_| BenchmarkParseError::InvalidUtf8)?
            .trim();
        self.inspect_failure(line);
        self.inspect_recommendations(line)?;
        if line.starts_with("Running additional tests of a few other settings") {
            self.additional_tests = true;
        }
        if self.additional_tests {
            self.additional.push(line)?;
            return Ok(None);
        }
        let Some((threads, body)) = thread_row(line) else {
            return Ok(None);
        };
        if body.ends_with("(recommended)") {
            if self.recommended.is_some_and(|previous| previous != threads) {
                return Err(BenchmarkParseError::ConflictingRecommendations);
            }
            self.recommended = Some(threads);
        }
        let Some(progress) = progress_row(threads, body) else {
            return Ok(None);
        };
        if progress.completed_positions == progress.total_positions
            && let (Some(visits_per_second), Some(nn_evals_per_second)) =
                (progress.visits_per_second, progress.nn_evals_per_second)
        {
            let metric = BenchmarkMetrics {
                threads,
                completed_positions: progress.completed_positions,
                total_positions: progress.total_positions,
                visits_per_second,
                nn_evals_per_second,
                nn_max_batch_size: None,
                num_nn_server_threads_per_model: None,
            };
            if let Some(previous) = self.metrics.iter_mut().find(|row| row.threads == threads) {
                *previous = metric;
            } else {
                self.metrics.push(metric);
            }
        }
        Ok(Some(progress))
    }

    fn inspect_recommendations(&mut self, line: &str) -> Result<(), BenchmarkParseError> {
        let line = line.to_ascii_lowercase();
        if let Some(value) = additional_assignment(&line, "numnnserverthreadspermodel") {
            self.num_nn_server_threads_per_model =
                recommendation(value, 256, "numNNServerThreadsPerModel")?;
            self.server_thread_recommendation_block = false;
        } else if line
            .starts_with("additional recommendation: 2 nn server threads per gpu measured faster.")
        {
            self.server_thread_recommendation_block = true;
        }
        if self.server_thread_recommendation_block
            && let Some(value) = assignment(&line, "numnnserverthreadspermodel", true)
        {
            self.num_nn_server_threads_per_model =
                recommendation(value, 256, "numNNServerThreadsPerModel")?;
            self.server_thread_recommendation_block = false;
        }
        if let Some(value) = additional_assignment(&line, "nnmaxbatchsize") {
            self.nn_max_batch_size = recommendation(value, 65536, "nnMaxBatchSize")?;
        }
        Ok(())
    }

    fn inspect_failure(&mut self, line: &str) {
        if line.starts_with("Beginning GPU tuning") {
            if self.gpu_tuning && self.gpu_tuning_error.is_some() {
                self.failure = self.gpu_tuning_error.take();
            }
            self.gpu_tuning = true;
        }
        let lower = line.to_ascii_lowercase();
        if failure_line(&lower) {
            let reason: String = line.chars().take(512).collect();
            // OpenCL tuning can reject one kernel and subsequently finish.
            // Only an explicit Done tuning recovers that narrow error case.
            if self.gpu_tuning
                && lower
                    .strip_prefix("error:")
                    .is_some_and(|tail| !failure_line(tail.trim()))
            {
                self.gpu_tuning_error = Some(reason);
            } else {
                self.failure = Some(reason);
            }
        }
        if self.gpu_tuning && line == "Done tuning" {
            self.gpu_tuning = false;
            self.gpu_tuning_error = None;
        }
    }
}

// Accept exactly the assignments Java recognizes: inline "set KEY = N" in
// an ADDITIONAL RECOMMENDATION, or a bare server-thread assignment in its block.
// Other config/log text cannot accidentally become a managed setting.
fn additional_assignment<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let body = line.strip_prefix("additional recommendation:")?;
    for (index, _) in body.match_indices("set") {
        if index > 0 && body.as_bytes()[index - 1].is_ascii_alphanumeric() {
            continue;
        }
        let tail = &body[index + 3..];
        if tail.starts_with(char::is_whitespace)
            && let Some(value) = assignment(tail.trim_start(), key, false)
        {
            return Some(value);
        }
    }
    None
}

fn assignment<'a>(line: &'a str, key: &str, entire_line: bool) -> Option<&'a str> {
    let value = line
        .strip_prefix(key)?
        .trim_start()
        .strip_prefix('=')?
        .trim_start();
    let digits = value.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let suffix = &value[digits..];
    if (entire_line && !suffix.trim().is_empty())
        || suffix.starts_with(|ch: char| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    Some(&value[..digits])
}

fn recommendation(
    value: &str,
    maximum: u32,
    key: &'static str,
) -> Result<Option<u32>, BenchmarkParseError> {
    let value: u32 = value
        .parse()
        .map_err(|_| BenchmarkParseError::InvalidRecommendation(key))?;
    if value > maximum {
        return Err(BenchmarkParseError::InvalidRecommendation(key));
    }
    Ok((value > 0).then_some(value))
}

fn failure_line(line: &str) -> bool {
    line.starts_with("error:")
        || line.starts_with("fatal:")
        || [
            "fatal error",
            "segmentation fault",
            "sigsegv",
            "core dumped",
            "uncaught exception",
            "terminate called",
            "failed to load model",
            "invalid metaldevicetousethread",
        ]
        .iter()
        .any(|needle| line.contains(needle))
}

fn thread_row(line: &str) -> Option<(u32, &str)> {
    let row = line.strip_prefix("numSearchThreads")?.trim_start();
    let (threads, body) = row.strip_prefix('=')?.split_once(':')?;
    let threads: u32 = threads.trim().parse().ok()?;
    (1..=MAX_THREADS)
        .contains(&threads)
        .then_some((threads, body.trim()))
}

fn progress_row(threads: u32, body: &str) -> Option<BenchmarkProgress> {
    let (positions, metrics) = body.split_once("positions,")?;
    let (completed, total) = positions.split_once('/')?;
    let completed_positions = completed.trim().parse().ok()?;
    let total_positions = total.trim().parse().ok()?;
    if total_positions == 0 || completed_positions > total_positions {
        return None;
    }
    Some(BenchmarkProgress {
        threads,
        completed_positions,
        total_positions,
        visits_per_second: speed(metrics, "visits/s"),
        nn_evals_per_second: speed(metrics, "nnEvals/s"),
    })
}

fn speed(metrics: &str, label: &str) -> Option<f64> {
    let (_, value) = metrics.split_once(label)?;
    let value = value.trim_start().strip_prefix('=')?.trim_start();
    let value: f64 = value.split_whitespace().next()?.parse().ok()?;
    (value.is_finite() && value > 0.0).then_some(value)
}

#[cfg(test)]
mod tests;
