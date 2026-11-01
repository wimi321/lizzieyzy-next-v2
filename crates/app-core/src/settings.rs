use std::{
    collections::BTreeMap,
    ffi::OsString,
    io,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use engine_session::TransportSpec;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const SETTINGS_VERSION: u32 = 1;

/// P1 persists one active engine profile. Empty paths remain editable settings;
/// they become readable engine failures when the profile is started.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct EngineProfile {
    pub program: PathBuf,
    pub model: PathBuf,
    pub config: PathBuf,
    pub extra_args: Vec<String>,
    // Catalog identity verified by size and SHA-256; display metadata only.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub canonical_model_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub engine: Option<EngineProfile>,
    pub pondering: bool,
    #[ts(as = "Option<crate::AnalysisLimitSettings>", optional)]
    pub analysis_limits: crate::AnalysisLimitSettings,
    #[ts(as = "Option<bool>", optional)]
    pub auto_quick_analyze_on_load: bool,
    pub fox_history: Vec<crate::fox::FoxAccount>,
    pub move_rank: crate::MoveRankSettings,
    pub tsumego: crate::TsumegoOptions,
    #[ts(as = "Option<BTreeMap<String, crate::BenchmarkResult>>", optional)]
    pub benchmarks: BTreeMap<String, crate::BenchmarkResult>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            engine: None,
            pondering: true,
            analysis_limits: crate::AnalysisLimitSettings::default(),
            auto_quick_analyze_on_load: true,
            fox_history: Vec::new(),
            move_rank: crate::MoveRankSettings::default(),
            tsumego: crate::TsumegoOptions::default(),
            benchmarks: BTreeMap::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("cannot access {kind} at {path}: {source}")]
    Io {
        kind: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid settings JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("settings version {0} is newer than supported version {SETTINGS_VERSION}")]
    FutureVersion(u32),
    #[error("invalid engine profile: {0}")]
    InvalidProfile(String),
    #[error(transparent)]
    MoveRank(#[from] crate::MoveRankError),
    #[error(transparent)]
    Tsumego(#[from] crate::TsumegoError),
    #[error(transparent)]
    AnalysisLimits(#[from] analysis::AnalysisError),
}

impl Settings {
    pub async fn load(directory: &Path) -> Result<Self, SettingsError> {
        let path = directory.join("settings.json");
        let bytes = match tokio::fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(SettingsError::Io {
                    kind: "settings",
                    path,
                    source,
                });
            }
        };
        let settings: Self = serde_json::from_slice(&bytes)?;
        settings.normalized()
    }

    pub async fn save(&self, directory: &Path) -> Result<(), SettingsError> {
        let bytes = serde_json::to_vec_pretty(&self.clone().normalized()?)?;
        tokio::fs::create_dir_all(directory)
            .await
            .map_err(|source| SettingsError::Io {
                kind: "settings directory",
                path: directory.to_owned(),
                source,
            })?;
        let path = directory.join("settings.json");
        crate::storage::write(path.clone(), bytes)
            .await
            .map_err(|source| SettingsError::Io {
                kind: "settings",
                path,
                source,
            })
    }

    fn normalized(mut self) -> Result<Self, SettingsError> {
        if self.version > SETTINGS_VERSION {
            return Err(SettingsError::FutureVersion(self.version));
        }
        self.version = SETTINGS_VERSION;
        crate::fox::normalize_history(&mut self.fox_history);
        self.move_rank.validate()?;
        self.tsumego.validate()?;
        self.analysis_limits.validate()?;
        if self
            .benchmarks
            .iter()
            .any(|(key, result)| key != &result.engine_fingerprint || !result.valid())
        {
            return Err(SettingsError::InvalidProfile(
                "invalid saved benchmark result".into(),
            ));
        }
        Ok(self)
    }
}

impl EngineProfile {
    /// Identity is inexpensive even for multi-gigabyte models. The caller must
    /// replace the coordinator when this value changes; it is not a file digest.
    pub async fn fingerprint(&self) -> Result<String, SettingsError> {
        let mut files = Vec::new();
        for (kind, path) in [
            ("engine program", &self.program),
            ("engine model", &self.model),
            ("engine configuration", &self.config),
        ] {
            let metadata = tokio::fs::metadata(path)
                .await
                .map_err(|source| SettingsError::Io {
                    kind,
                    path: path.clone(),
                    source,
                })?;
            if !metadata.is_file() {
                return Err(SettingsError::InvalidProfile(format!(
                    "{kind} at {} is not a regular file",
                    path.display()
                )));
            }
            let modified = metadata.modified().map_err(|source| SettingsError::Io {
                kind,
                path: path.clone(),
                source,
            })?;
            let (before_epoch, age) = match modified.duration_since(UNIX_EPOCH) {
                Ok(age) => (false, age),
                Err(error) => (true, error.duration()),
            };
            files.push((
                path,
                metadata.len(),
                before_epoch,
                age.as_secs(),
                age.subsec_nanos(),
            ));
        }
        let encoded = serde_json::to_vec(&(
            "lizzieyzy-next2:engine-profile:v2-two-search-slots",
            files,
            &self.extra_args,
        ))?;
        Ok(blake3::hash(&encoded).to_hex().to_string())
    }

    pub fn transport(&self) -> Result<TransportSpec, SettingsError> {
        self.transport_with_benchmark(None)
    }

    pub(crate) fn transport_with_benchmark(
        &self,
        benchmark: Option<&crate::BenchmarkResult>,
    ) -> Result<TransportSpec, SettingsError> {
        let (overrides, extra_args) = self.prepare_extra_args()?;
        let overrides = if let Some(benchmark) = benchmark {
            if !benchmark.valid() {
                return Err(SettingsError::InvalidProfile(
                    "invalid benchmark recommendation".into(),
                ));
            }
            let mut entries = overrides
                .split(',')
                .filter(|entry| {
                    let key = entry.split('=').next().unwrap_or_default().trim();
                    !key.eq_ignore_ascii_case("numSearchThreads")
                        && !key.eq_ignore_ascii_case("numSearchThreadsPerAnalysisThread")
                })
                .map(str::to_owned)
                .collect::<Vec<_>>();
            // KataGo rejects both aliases together. Empty overrides delete keys
            // even when the GTP alias originated in an included config file.
            entries.push("numSearchThreads=".into());
            entries.push(format!(
                "numSearchThreadsPerAnalysisThread={}",
                benchmark.recommended_threads
            ));
            // Match Java's managed GPU settings: override configuration-file
            // defaults, but preserve an explicit user CLI override of either key.
            for (key, value) in [
                ("nnMaxBatchSize", benchmark.nn_max_batch_size),
                (
                    "numNNServerThreadsPerModel",
                    benchmark.num_nn_server_threads_per_model,
                ),
            ] {
                if let Some(value) = value
                    && !entries.iter().any(|entry| {
                        entry
                            .split_once('=')
                            .is_some_and(|(existing, _)| existing.trim().eq_ignore_ascii_case(key))
                    })
                {
                    entries.push(format!("{key}={value}"));
                }
            }
            entries.join(",")
        } else {
            overrides
        };
        let mut args = vec![
            OsString::from("analysis"),
            OsString::from("-config"),
            self.config.as_os_str().to_owned(),
            OsString::from("-model"),
            self.model.as_os_str().to_owned(),
            OsString::from("-override-config"),
            OsString::from(overrides),
        ];
        args.extend(extra_args);
        Ok(TransportSpec::LocalProcess {
            program: self.program.clone(),
            args,
            cwd: None,
            env: BTreeMap::new(),
        })
    }

    /// KataGo accepts this CLI flag only once. Preserve all other override
    /// entries in their original order, remove redundant explicit BLACK entries,
    /// then append the required perspective once at the end of the merged value.
    fn prepare_extra_args(&self) -> Result<(String, Vec<OsString>), SettingsError> {
        let mut overrides = Vec::new();
        let mut extra_args = Vec::new();
        let mut args = self.extra_args.iter();
        while let Some(arg) = args.next() {
            let flag = arg.split_once('=').map_or(arg.as_str(), |(flag, _)| flag);
            if matches!(flag, "-config" | "-model") {
                return Err(SettingsError::InvalidProfile(format!(
                    "extra arguments cannot replace {flag}; edit the profile path instead"
                )));
            }
            if flag == "-override-config" {
                let value = match arg.split_once('=') {
                    Some((_, value)) => value,
                    None => args.next().ok_or_else(|| {
                        SettingsError::InvalidProfile("-override-config needs a value".into())
                    })?,
                };
                for setting in value.split(',') {
                    let (key, value) = setting.split_once('=').ok_or_else(|| {
                        SettingsError::InvalidProfile(
                            "-override-config expects KEY=VALUE entries".into(),
                        )
                    })?;
                    if key.trim().is_empty() || value.trim().is_empty() {
                        return Err(SettingsError::InvalidProfile(
                            "-override-config expects nonempty KEY=VALUE entries".into(),
                        ));
                    }
                    if key.trim().eq_ignore_ascii_case("reportAnalysisWinratesAs") {
                        if value.trim() != "BLACK" {
                            return Err(SettingsError::InvalidProfile(
                                "reportAnalysisWinratesAs must remain BLACK".into(),
                            ));
                        }
                    } else if !key.trim().eq_ignore_ascii_case("numAnalysisThreads") {
                        overrides.push(setting);
                    }
                }
            } else {
                extra_args.push(OsString::from(arg));
            }
        }
        // One unbounded live search and one background position can progress
        // together even when a supplied config originally used a single slot.
        overrides.push("numAnalysisThreads=2");
        overrides.push("reportAnalysisWinratesAs=BLACK");
        Ok((overrides.join(","), extra_args))
    }
}

#[cfg(test)]
mod tests;
