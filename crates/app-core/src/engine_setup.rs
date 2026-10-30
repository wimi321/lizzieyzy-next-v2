//! Best-effort discovery in caller-provided locations. Discovery never executes
//! a candidate or reads process environment variables; trial startup validates it.

use std::{
    cmp::Reverse,
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// All search locations are explicit so discovery is independent of the host
/// environment in tests. Windows roots are concrete KataGo installation folders.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchRoots {
    pub path_dirs: Vec<PathBuf>,
    pub homebrew_prefixes: Vec<PathBuf>,
    pub application_dirs: Vec<PathBuf>,
    pub windows_dirs: Vec<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct EngineDiscovery {
    pub programs: Vec<PathBuf>,
    pub models: Vec<PathBuf>,
    pub configs: Vec<PathBuf>,
}

/// Search only the supplied roots and well-known immediate subdirectories.
/// Missing/unreadable paths are skipped. Programs retain search-root priority;
/// model recommendations are a filename heuristic, not a hardware suitability
/// or playing-strength measurement.
pub fn detect(roots: &SearchRoots) -> EngineDiscovery {
    let mut candidates = Candidates::default();
    for directory in &roots.path_dirs {
        candidates.programs_in(directory);
    }
    for prefix in &roots.homebrew_prefixes {
        candidates.programs_in(&prefix.join("bin"));
        candidates.files_in(&prefix.join("share/katago"));
    }
    for directory in &roots.application_dirs {
        candidates.installation(&directory.join("katago"));
    }
    for directory in &roots.windows_dirs {
        candidates.installation(directory);
    }
    candidates.result.models.sort_by_cached_key(|path| {
        let name = filename(path);
        let family = if name.starts_with("kata1-") {
            0
        } else if name.starts_with("g170") {
            1
        } else {
            2
        };
        (
            family,
            Reverse(network_counter(&name, 's')),
            Reverse(network_counter(&name, 'd')),
            name,
            path.clone(),
        )
    });
    candidates.result.configs.sort();
    candidates.result
}

#[derive(Default)]
struct Candidates {
    result: EngineDiscovery,
    programs_seen: BTreeSet<PathBuf>,
    models_seen: BTreeSet<PathBuf>,
    configs_seen: BTreeSet<PathBuf>,
}

impl Candidates {
    fn programs_in(&mut self, directory: &Path) {
        for name in ["katago", "katago.exe"] {
            let path = directory.join(name);
            if is_executable_file(&path) {
                add_unique(&mut self.result.programs, &mut self.programs_seen, path);
            }
        }
    }

    fn installation(&mut self, directory: &Path) {
        self.programs_in(directory);
        self.programs_in(&directory.join("bin"));
        self.files_in(directory);
        for child in ["models", "networks", "config", "configs"] {
            self.files_in(&directory.join(child));
        }
    }

    fn files_in(&mut self, directory: &Path) {
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        // read_dir order is unspecified, including when two symlinks name the
        // same file. Sort before deduplication so the selected name is stable.
        let mut paths: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            if !path.is_file() {
                continue;
            }
            let name = filename(&path);
            if model_stem(&name).is_some() {
                add_unique(&mut self.result.models, &mut self.models_seen, path);
            } else if name.ends_with(".cfg") {
                add_unique(&mut self.result.configs, &mut self.configs_seen, path);
            }
        }
    }
}

fn add_unique(paths: &mut Vec<PathBuf>, seen: &mut BTreeSet<PathBuf>, path: PathBuf) {
    if let Ok(identity) = fs::canonicalize(&path)
        && seen.insert(identity)
    {
        paths.push(path);
    }
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn filename(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn model_stem(name: &str) -> Option<&str> {
    [".bin.gz", ".txt.gz", ".bin"]
        .into_iter()
        .find_map(|suffix| name.strip_suffix(suffix))
        .filter(|stem| !stem.is_empty())
}

fn network_counter(name: &str, prefix: char) -> u64 {
    model_stem(name)
        .into_iter()
        .flat_map(|stem| stem.split('-'))
        .find_map(|part| part.strip_prefix(prefix)?.parse().ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
