use std::{collections::BTreeMap, path::Path};

use serde::Deserialize;

use super::{AssetError, OfficialWeight};

const SOURCE: &str = include_str!("../../../../resources/katago/katago-assets.json");

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    #[serde(default, alias = "assetName")]
    file_name: String,
    #[serde(default)]
    purpose: String,
    size_bytes: u64,
    sha256: String,
    download_url: Option<String>,
    published_at: Option<String>,
    runtime_profile: Option<String>,
    executable_sha256: Option<String>,
    backend: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceCatalog {
    default_model_id: String,
    model_release_tag: String,
    engine_release_repository: String,
    engine_release_tag: String,
    models: BTreeMap<String, Entry>,
    assets: BTreeMap<String, Entry>,
    runtime_packages: BTreeMap<String, Entry>,
    runtime_profiles: BTreeMap<String, RuntimeProfileSpec>,
}

#[derive(Clone, Debug)]
pub(crate) struct AssetSpec {
    pub id: String,
    pub file_name: String,
    pub size: u64,
    pub sha256: String,
    pub url: String,
    pub weight: bool,
    pub human_style: bool,
    pub published_at: Option<String>,
    pub runtime_profile: Option<String>,
    pub executable_sha256: Option<String>,
}

impl AssetSpec {
    pub fn relative_path(&self) -> std::path::PathBuf {
        Path::new(if self.weight { "weights" } else { "components" }).join(&self.file_name)
    }

    pub fn weight_view(&self, downloaded: bool, path: String) -> OfficialWeight {
        OfficialWeight {
            id: self.id.clone(),
            name: self.file_name.clone(),
            size_bytes: self.size,
            published_at: self.published_at.clone(),
            downloaded,
            path: downloaded.then_some(path),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AssetCatalog {
    pub specs: Vec<AssetSpec>,
    pub profiles: BTreeMap<String, RuntimeProfileSpec>,
}

impl AssetCatalog {
    pub fn bundled() -> Result<Self, AssetError> {
        let raw: SourceCatalog =
            serde_json::from_str(SOURCE).map_err(|error| AssetError::Catalog(error.to_string()))?;
        let mut specs = Vec::new();
        for (id, entry) in raw.models {
            let url = entry.download_url.clone().unwrap_or_else(|| {
                format!(
                    "https://github.com/lightvector/KataGo/releases/download/{}/{}",
                    raw.model_release_tag, entry.file_name
                )
            });
            specs.push(spec(id, entry, url, true)?);
        }
        specs.sort_by_key(|entry| (entry.id != raw.default_model_id, entry.id.clone()));
        for (id, entry) in raw.assets {
            if !matches!(id.as_str(), "windows-nvidia" | "windows-tensorrt") {
                continue;
            }
            let url = format!(
                "https://github.com/{}/releases/download/{}/{}",
                raw.engine_release_repository, raw.engine_release_tag, entry.file_name
            );
            specs.push(spec(id, entry, url, false)?);
        }
        for (id, entry) in raw.runtime_packages {
            let url = entry
                .download_url
                .clone()
                .ok_or_else(|| AssetError::Catalog(id.clone()))?;
            specs.push(spec(format!("runtime-{id}"), entry, url, false)?);
        }
        Ok(Self {
            specs,
            profiles: raw.runtime_profiles,
        })
    }

    pub fn get(&self, id: &str) -> Result<&AssetSpec, AssetError> {
        self.specs
            .iter()
            .find(|spec| spec.id == id)
            .ok_or_else(|| AssetError::UnknownAsset(id.into()))
    }
}

fn spec(id: String, entry: Entry, url: String, weight: bool) -> Result<AssetSpec, AssetError> {
    if entry.file_name.is_empty()
        || entry.file_name.contains(['/', '\\'])
        || entry.file_name == "."
        || entry.file_name == ".."
        || entry.size_bytes == 0
        || entry.sha256.len() != 64
        || !entry.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || !url.starts_with("https://")
    {
        return Err(AssetError::Catalog(id));
    }
    Ok(AssetSpec {
        id,
        file_name: entry.file_name,
        size: entry.size_bytes,
        sha256: entry.sha256,
        url,
        weight,
        human_style: entry.purpose == "human-style",
        published_at: entry.published_at,
        runtime_profile: entry.runtime_profile,
        executable_sha256: entry.executable_sha256,
    })
}

#[cfg(test)]
impl AssetCatalog {
    pub(crate) fn test_weight(id: &str, url: &str, bytes: &[u8]) -> Self {
        use sha2::{Digest, Sha256};
        Self {
            specs: vec![AssetSpec {
                id: id.into(),
                file_name: format!("{id}.bin.gz"),
                size: bytes.len() as u64,
                sha256: format!("{:x}", Sha256::digest(bytes)),
                url: url.into(),
                weight: true,
                human_style: id == "human-sl",
                published_at: Some("2026-01-01".into()),
                runtime_profile: None,
                executable_sha256: None,
            }],
            profiles: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeProfileSpec {
    pub engine_asset: String,
    pub assets: Vec<String>,
    pub required_files: Vec<String>,
}

impl AssetCatalog {
    pub(crate) fn profile(&self, id: &str) -> Result<&RuntimeProfileSpec, AssetError> {
        self.profiles
            .get(id)
            .ok_or_else(|| AssetError::UnknownAsset(id.into()))
    }
}

pub(super) fn backend_for_digest(sha256: &str) -> Option<String> {
    let source: SourceCatalog = serde_json::from_str(SOURCE).ok()?;
    source
        .assets
        .into_values()
        .find(|entry| {
            entry
                .executable_sha256
                .as_deref()
                .is_some_and(|pin| pin.eq_ignore_ascii_case(sha256))
        })
        .and_then(|entry| entry.backend)
}
