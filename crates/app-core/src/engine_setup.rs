//! Best-effort discovery in caller-provided locations. Discovery never executes
//! a candidate or reads process environment variables; trial startup validates it.

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub(crate) mod catalog;
pub(crate) use catalog::CatalogModel;

/// All search locations are explicit so discovery is independent of the host
/// environment in tests. Windows roots are concrete KataGo installation folders.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchRoots {
    pub path_dirs: Vec<PathBuf>,
    pub homebrew_prefixes: Vec<PathBuf>,
    pub application_dirs: Vec<PathBuf>,
    pub windows_dirs: Vec<PathBuf>,
    /// Content roots containing `engines/katago` and `weights`.
    pub resource_dirs: Vec<PathBuf>,
    pub java_install_dirs: Vec<PathBuf>,
    /// Concrete platform directory names, supplied by the shell, never guessed
    /// by traversing the installation (e.g. `macos-arm64`, `windows-x64`).
    pub platform_dirs: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct EngineDiscovery {
    pub programs: Vec<PathBuf>,
    pub models: Vec<PathBuf>,
    pub configs: Vec<PathBuf>,
}

/// Search only the supplied roots and well-known immediate subdirectories.
/// Missing/unreadable paths are skipped. Programs retain search-root priority;
/// catalog models are identified by size and SHA-256 before applying the
/// filename fallback. Discovery does not establish hardware compatibility.
pub fn detect(roots: &SearchRoots) -> EngineDiscovery {
    detect_candidates(roots).discovery
}

pub(crate) struct DetectedEngines {
    pub discovery: EngineDiscovery,
    pub catalog_models: BTreeMap<PathBuf, CatalogModel>,
    installations: BTreeMap<PathBuf, BTreeSet<PathBuf>>,
}

impl DetectedEngines {
    /// Keep discovery order within each group; a model's own installation is
    /// preferred even if its program was first encountered through PATH.
    pub fn programs_for_model(&self, model: &Path) -> Vec<PathBuf> {
        let model_roots = fs::canonicalize(model)
            .ok()
            .and_then(|path| self.installations.get(&path));
        let mut programs = self.discovery.programs.clone();
        programs.sort_by_key(|program| {
            let program_roots = fs::canonicalize(program)
                .ok()
                .and_then(|path| self.installations.get(&path));
            !model_roots
                .zip(program_roots)
                .is_some_and(|(models, programs)| models.iter().any(|root| programs.contains(root)))
        });
        programs
    }
}

pub(crate) fn detect_candidates(roots: &SearchRoots) -> DetectedEngines {
    detect_with_catalog(roots, &catalog::ModelCatalog::bundled())
}

pub(crate) fn detect_with_catalog(
    roots: &SearchRoots,
    catalog: &catalog::ModelCatalog,
) -> DetectedEngines {
    let mut candidates = Candidates::default();
    for directory in &roots.path_dirs {
        candidates.programs_in(directory, None);
    }
    for prefix in &roots.homebrew_prefixes {
        candidates.programs_in(&prefix.join("bin"), Some(prefix));
        candidates.files_in(&prefix.join("share/katago"), prefix);
    }
    for directory in &roots.application_dirs {
        candidates.installation(&directory.join("katago"));
    }
    for directory in &roots.windows_dirs {
        candidates.installation(directory);
    }
    for directory in roots.resource_dirs.iter().chain(&roots.java_install_dirs) {
        candidates.bundled_installation(directory, &roots.platform_dirs);
    }
    let catalog_models: BTreeMap<_, _> = candidates
        .result
        .models
        .iter()
        .filter_map(|path| catalog.identify(path).map(|model| (path.clone(), model)))
        .collect();
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
            catalog.rank(catalog_models.get(path)),
            family,
            Reverse(network_counter(&name, 's')),
            Reverse(network_counter(&name, 'd')),
            name,
            path.clone(),
        )
    });
    candidates.result.configs.sort();
    DetectedEngines {
        discovery: candidates.result,
        catalog_models,
        installations: candidates.installations,
    }
}

#[derive(Default)]
struct Candidates {
    result: EngineDiscovery,
    programs_seen: BTreeSet<PathBuf>,
    models_seen: BTreeSet<PathBuf>,
    configs_seen: BTreeSet<PathBuf>,
    installations: BTreeMap<PathBuf, BTreeSet<PathBuf>>,
}

impl Candidates {
    fn programs_in(&mut self, directory: &Path, installation: Option<&Path>) {
        for name in ["katago", "katago.exe"] {
            let path = directory.join(name);
            if is_executable_file(&path) {
                if let Some(root) = installation {
                    self.record_installation(&path, root);
                }
                add_unique(&mut self.result.programs, &mut self.programs_seen, path);
            }
        }
    }

    fn installation(&mut self, directory: &Path) {
        self.programs_in(directory, Some(directory));
        self.programs_in(&directory.join("bin"), Some(directory));
        self.files_in(directory, directory);
        for child in ["models", "networks", "config", "configs"] {
            self.files_in(&directory.join(child), directory);
        }
    }

    fn bundled_installation(&mut self, directory: &Path, platforms: &[String]) {
        for platform in platforms {
            // A platform is one path component, not an escape from this root.
            if !platform.is_empty()
                && platform
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            {
                self.programs_in(
                    &directory.join("engines/katago").join(platform),
                    Some(directory),
                );
            }
        }
        self.files_in(&directory.join("weights"), directory);
        self.files_in(&directory.join("engines/katago/configs"), directory);
    }

    fn record_installation(&mut self, path: &Path, root: &Path) {
        if let (Ok(path), Ok(root)) = (fs::canonicalize(path), fs::canonicalize(root)) {
            self.installations.entry(path).or_default().insert(root);
        }
    }

    fn files_in(&mut self, directory: &Path, installation: &Path) {
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
                self.record_installation(&path, installation);
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
        .find_map(|part| {
            let count = part.strip_prefix(prefix)?;
            let (digits, multiplier) = if let Some(digits) =
                count.strip_suffix('m').or_else(|| count.strip_suffix('M'))
            {
                (digits, 1_000_000)
            } else if let Some(digits) = count.strip_suffix('g').or_else(|| count.strip_suffix('G'))
            {
                (digits, 1_000_000_000)
            } else {
                (count, 1)
            };
            digits.parse::<u64>().ok()?.checked_mul(multiplier)
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
