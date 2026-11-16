//! Only explicitly embedded spectator assets may leave the desktop shell.
use app_core::{WebBoardAsset, WebBoardAssets};
use tauri::Runtime;

pub(crate) struct EmbeddedWebAssets<R: Runtime>(pub tauri::AppHandle<R>);

impl<R: Runtime> std::fmt::Debug for EmbeddedWebAssets<R> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("EmbeddedWebAssets")
    }
}

impl<R: Runtime> WebBoardAssets for EmbeddedWebAssets<R> {
    fn get(&self, path: &str) -> Option<WebBoardAsset> {
        if !allowed_path(path) {
            return None;
        }
        let resolver = self.0.asset_resolver();
        let expected = tauri::utils::assets::AssetKey::from(path);
        // AssetResolver::get performs an index.html SPA fallback. Check exact
        // membership first so a missing web.html never serves the desktop UI.
        if !resolver
            .iter()
            .any(|(key, _)| key.as_ref() == expected.as_ref())
        {
            return None;
        }
        let asset = resolver.get(path.to_owned())?;
        Some(WebBoardAsset {
            bytes: asset.bytes,
            mime: asset.mime_type,
        })
    }
}

fn allowed_path(path: &str) -> bool {
    (path == "web.html" || path.starts_with("assets/"))
        && !path.contains(['\\', '%', '?', '#'])
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

#[cfg(test)]
mod tests;
