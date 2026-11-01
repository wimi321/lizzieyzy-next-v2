//! The official extra tests are sequential: an accepted NN server trial is
//! retained while testing batch size. A rejected trial restores the baseline.
use super::{BenchmarkMetrics, BenchmarkParseError, progress_row, thread_row};

const MAX_MEASUREMENTS: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Settings {
    batch: Option<u32>,
    // The heading is per GPU, but the final recommendation is a total. Keep the
    // trial's identity here so two GPUs can correctly recommend four servers.
    doubled_servers: bool,
}

#[derive(Clone, Copy, Debug)]
enum Trial {
    Servers,
    Batch,
}

#[derive(Debug)]
struct Row {
    settings: Settings,
    metrics: BenchmarkMetrics,
}

#[derive(Debug, Default)]
pub(super) struct Measurements {
    accepted: Settings,
    current: Option<(Trial, Settings)>,
    rows: Vec<Row>,
}

impl Measurements {
    pub(super) fn push(&mut self, line: &str) -> Result<(), BenchmarkParseError> {
        if line.starts_with("Re-measuring the current recommendation as a baseline:") {
            self.current = None;
        } else if line.starts_with("Testing 2 NN server threads per GPU.") {
            self.current = Some((
                Trial::Servers,
                Settings {
                    doubled_servers: true,
                    ..self.accepted
                },
            ));
        } else if let Some(value) = line.strip_prefix("Testing a max batch size of ") {
            let batch = value
                .split(|ch: char| ch == ',' || ch.is_whitespace())
                .next()
                .and_then(|s| s.parse::<u32>().ok());
            self.current = batch.filter(|n| (1..=65536).contains(n)).map(|batch| {
                (
                    Trial::Batch,
                    Settings {
                        batch: Some(batch),
                        ..self.accepted
                    },
                )
            });
        } else if line.starts_with("Testing ") {
            // Do not attribute an unrecognized experiment's rows to the last
            // known trial, even if the unknown experiment happens to be faster.
            self.current = None;
        } else if line.starts_with("2 NN server threads per GPU was ") {
            self.decision(line, true);
        } else if line.starts_with("Half batch size was ") {
            self.decision(line, false);
        }

        let Some((_, settings)) = self.current else {
            return Ok(());
        };
        let Some((threads, body)) = thread_row(line) else {
            return Ok(());
        };
        let Some(progress) = progress_row(threads, body) else {
            return Ok(());
        };
        if progress.completed_positions != progress.total_positions {
            return Ok(());
        }
        let (Some(visits_per_second), Some(nn_evals_per_second)) =
            (progress.visits_per_second, progress.nn_evals_per_second)
        else {
            return Ok(());
        };
        let metrics = BenchmarkMetrics {
            threads,
            completed_positions: progress.completed_positions,
            total_positions: progress.total_positions,
            visits_per_second,
            nn_evals_per_second,
            nn_max_batch_size: None,
            num_nn_server_threads_per_model: None,
        };
        if let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| row.settings == settings && row.metrics.threads == threads)
        {
            row.metrics = metrics;
        } else {
            if self.rows.len() == MAX_MEASUREMENTS {
                return Err(BenchmarkParseError::TooManyAdditionalMeasurements);
            }
            self.rows.push(Row { settings, metrics });
        }
        Ok(())
    }

    fn decision(&mut self, line: &str, servers: bool) {
        if let Some((trial, settings)) = self.current.take()
            && matches!(
                (trial, servers),
                (Trial::Servers, true) | (Trial::Batch, false)
            )
            && line.ends_with("will recommend it.")
        {
            self.accepted = settings;
        }
    }

    pub(super) fn find(
        &self,
        threads: u32,
        batch: Option<u32>,
        doubled_servers: bool,
    ) -> Option<&BenchmarkMetrics> {
        let settings = Settings {
            batch,
            doubled_servers,
        };
        self.rows
            .iter()
            .find(|row| row.settings == settings && row.metrics.threads == threads)
            .map(|row| &row.metrics)
    }
}
