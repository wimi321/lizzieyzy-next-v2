//! The model subset of the bundled Java catalog; engine/download metadata is
//! deliberately not deserialized or used by local discovery.

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Read},
    path::Path,
};

use serde::Deserialize;
use sha2::{Digest, Sha256};

const CATALOG: &str = include_str!("../../../../resources/katago/katago-assets.json");

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CatalogModel {
    pub file_name: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub tier: String,
    #[serde(rename = "minimumKataGoVersion")]
    pub minimum_katago_version: String,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelCatalog {
    pub default_model_id: String,
    pub models: BTreeMap<String, CatalogModel>,
}

impl ModelCatalog {
    pub fn bundled() -> Self {
        // A broken catalog must never identify an arbitrary model. The unit
        // test parses the embedded bytes strictly to catch packaging mistakes.
        serde_json::from_str(CATALOG).unwrap_or_default()
    }

    pub fn identify(&self, path: &Path) -> Option<CatalogModel> {
        self.identify_with(path, hash_file)
    }

    fn identify_with(
        &self,
        path: &Path,
        hash: impl FnOnce(&Path, u64) -> io::Result<String>,
    ) -> Option<CatalogModel> {
        let metadata = fs::metadata(path).ok()?;
        let size = metadata.len();
        if !metadata.is_file() || !self.models.values().any(|model| model.size_bytes == size) {
            return None;
        }
        let digest = hash(path, size).ok()?;
        self.models
            .values()
            .find(|model| model.size_bytes == size && model.sha256.eq_ignore_ascii_case(&digest))
            .cloned()
    }

    pub fn rank(&self, model: Option<&CatalogModel>) -> u8 {
        let Some(model) = model else {
            return 5;
        };
        if self.models.get(&self.default_model_id) == Some(model) {
            return 0;
        }
        match model.tier.as_str() {
            "strongest" => 1,
            "balanced" => 2,
            "lightweight" => 3,
            _ => 4,
        }
    }
}

fn hash_file(path: &Path, expected_size: u64) -> io::Result<String> {
    // Bound reads to the metadata size plus one byte, including if another
    // process replaces or extends a file while discovery is running.
    let mut file = File::open(path)?.take(expected_size.saturating_add(1));
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    let mut total = 0;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        digest.update(&buffer[..count]);
    }
    if total != expected_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "model size changed",
        ));
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
mod tests;
