use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{REPETITIONS, SupportError, WARMUPS};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scenario {
    OpenReviewedGame,
    OpenLargeTree,
    JumpToLastMove,
    EngineReady,
    FirstAnalysis,
    AnalysisAfterMove,
}

impl Scenario {
    pub const ALL: [Self; 6] = [
        Self::OpenReviewedGame,
        Self::OpenLargeTree,
        Self::JumpToLastMove,
        Self::EngineReady,
        Self::FirstAnalysis,
        Self::AnalysisAfterMove,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenReviewedGame => "open_reviewed_game",
            Self::OpenLargeTree => "open_large_tree",
            Self::JumpToLastMove => "jump_to_last_move",
            Self::EngineReady => "engine_ready",
            Self::FirstAnalysis => "first_analysis",
            Self::AnalysisAfterMove => "analysis_after_move",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::OpenReviewedGame => "Open reviewed game",
            Self::OpenLargeTree => "Open large tree",
            Self::JumpToLastMove => "Jump to last move",
            Self::EngineReady => "Engine ready",
            Self::FirstAnalysis => "First analysis",
            Self::AnalysisAfterMove => "Analysis after move",
        }
    }

    pub const fn has_memory(self) -> bool {
        matches!(
            self,
            Self::OpenReviewedGame | Self::OpenLargeTree | Self::FirstAnalysis
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioResult {
    pub scenario: Scenario,
    pub samples_ms: Vec<f64>,
    pub median_ms: f64,
    pub max_ms: f64,
    pub peak_rss_samples_bytes: Vec<u64>,
    pub peak_rss_bytes: Option<u64>,
}

impl ScenarioResult {
    pub fn new(
        scenario: Scenario,
        samples_ms: Vec<f64>,
        peak_rss_samples_bytes: Vec<u64>,
    ) -> Result<Self, SupportError> {
        if samples_ms.len() != REPETITIONS
            || samples_ms
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
        {
            return invalid("timings must contain seven finite, nonnegative samples");
        }
        if !peak_rss_samples_bytes.is_empty()
            && (!scenario.has_memory()
                || peak_rss_samples_bytes.len() != REPETITIONS
                || peak_rss_samples_bytes.contains(&0))
        {
            return invalid(
                "RSS must be unavailable or seven positive samples for memory scenarios",
            );
        }
        let mut sorted = samples_ms.clone();
        sorted.sort_by(f64::total_cmp);
        Ok(Self {
            scenario,
            samples_ms,
            median_ms: sorted[REPETITIONS / 2],
            max_ms: sorted[REPETITIONS - 1],
            peak_rss_bytes: peak_rss_samples_bytes.iter().copied().max(),
            peak_rss_samples_bytes,
        })
    }

    fn validate(&self) -> Result<(), SupportError> {
        let derived = Self::new(
            self.scenario,
            self.samples_ms.clone(),
            self.peak_rss_samples_bytes.clone(),
        )?;
        if self.median_ms != derived.median_ms
            || self.max_ms != derived.max_ms
            || self.peak_rss_bytes != derived.peak_rss_bytes
        {
            return invalid("summary statistics do not match raw samples");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub platform: String,
    pub architecture: String,
    pub warmups: usize,
    pub repetitions: usize,
    pub results: Vec<ScenarioResult>,
}

impl Report {
    pub fn validate(&self) -> Result<(), SupportError> {
        if !matches!(self.platform.as_str(), "linux" | "macos" | "windows")
            || self.architecture.trim().is_empty()
            || self.warmups != WARMUPS
            || self.repetitions != REPETITIONS
        {
            return invalid("report must identify its platform, architecture and 1+7 protocol");
        }
        let scenarios: BTreeSet<_> = self.results.iter().map(|item| item.scenario).collect();
        if self.results.len() != Scenario::ALL.len()
            || scenarios != Scenario::ALL.into_iter().collect()
        {
            return invalid("report must contain each of the six scenarios exactly once");
        }
        for result in &self.results {
            result.validate()?;
        }
        Ok(())
    }

    pub fn write_json(&self, path: &Path) -> Result<(), SupportError> {
        self.validate()?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let mut bytes = serde_json::to_vec_pretty(self)?;
        bytes.push(b'\n');
        fs::write(path, bytes)?;
        Ok(())
    }

    pub fn read_json(path: &Path) -> Result<Self, SupportError> {
        let report: Self = serde_json::from_slice(&fs::read(path)?)?;
        report.validate()?;
        Ok(report)
    }

    pub fn markdown_table(&self) -> String {
        let mut output = format!(
            "\nCore benchmarks: {} / {} ({} warm-up + {} measurements per scenario).\n\n\
             | Scenario | Median (ms) | Max (ms) | Peak RSS (MiB) |\n\
             | --- | ---: | ---: | ---: |\n",
            self.platform, self.architecture, self.warmups, self.repetitions
        );
        for result in &self.results {
            let rss = result
                .peak_rss_bytes
                .map(|bytes| format!("{:.2}", bytes as f64 / 1_048_576.0))
                .unwrap_or_else(|| "—".into());
            output.push_str(&format!(
                "| {} | {:.3} | {:.3} | {} |\n",
                result.scenario.label(),
                result.median_ms,
                result.max_ms,
                rss
            ));
        }
        output.push_str(
            "\nPeak RSS is the maximum of seven fresh benchmark workers, including core startup; \
             it excludes the fake engine process and the desktop/WebView.\n",
        );
        if self.platform == "windows" {
            output.push_str("Windows 不测峰值内存，Peak RSS 显示为 —。\n");
        }
        output
    }

    pub fn append_summary(&self, path: &Path) -> Result<(), SupportError> {
        self.validate()?;
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        file.write_all(self.markdown_table().as_bytes())?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limit {
    pub median_ms: f64,
    pub peak_rss_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Limits(pub BTreeMap<String, BTreeMap<Scenario, Limit>>);

impl Limits {
    pub fn read_json(path: &Path) -> Result<Self, SupportError> {
        let limits: Self = serde_json::from_slice(&fs::read(path)?)?;
        limits.validate()?;
        Ok(limits)
    }

    pub fn validate(&self) -> Result<(), SupportError> {
        if self.0.keys().map(String::as_str).collect::<BTreeSet<_>>()
            != ["linux", "macos", "windows"].into_iter().collect()
        {
            return invalid("limits must contain linux, macos and windows");
        }
        for platform in self.0.values() {
            if platform.keys().copied().collect::<BTreeSet<_>>()
                != Scenario::ALL.into_iter().collect()
            {
                return invalid("each platform needs all six scenario limits");
            }
            for (&scenario, limit) in platform {
                if !limit.median_ms.is_finite() || limit.median_ms < 50.0 {
                    return invalid("median limits must be finite and at least 50 ms");
                }
                match (scenario.has_memory(), limit.peak_rss_bytes) {
                    (true, Some(bytes)) if bytes > 0 => {}
                    (_, None) => {}
                    _ => return invalid("memory limits must match the memory scenarios"),
                }
            }
        }
        Ok(())
    }

    /// Equality passes. A null RSS limit disables that check; a configured RSS
    /// limit requires a measurement. The timing maximum remains diagnostic
    /// because shared hosts have jitter.
    pub fn check(&self, report: &Report) -> Result<Vec<String>, SupportError> {
        self.validate()?;
        report.validate()?;
        let limits = self
            .0
            .get(&report.platform)
            .ok_or_else(|| SupportError::Invalid(format!("no limits for {}", report.platform)))?;
        let mut violations = Vec::new();
        for result in &report.results {
            let limit = limits.get(&result.scenario).ok_or_else(|| {
                SupportError::Invalid(format!("missing limit for {}", result.scenario.as_str()))
            })?;
            if result.median_ms > limit.median_ms {
                violations.push(format!(
                    "{} median {:.3} ms exceeds {:.3} ms",
                    result.scenario.as_str(),
                    result.median_ms,
                    limit.median_ms
                ));
            }
            if let Some(maximum) = limit.peak_rss_bytes {
                match result.peak_rss_bytes {
                    Some(actual) if actual > maximum => violations.push(format!(
                        "{} peak RSS {actual} bytes exceeds {maximum} bytes",
                        result.scenario.as_str()
                    )),
                    None => violations.push(format!(
                        "{} peak RSS is unavailable but a limit of {maximum} bytes is configured",
                        result.scenario.as_str()
                    )),
                    Some(_) => {}
                }
            }
        }
        Ok(violations)
    }
}

fn invalid<T>(message: &str) -> Result<T, SupportError> {
    Err(SupportError::Invalid(message.into()))
}
