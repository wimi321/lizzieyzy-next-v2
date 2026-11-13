//! Official Tauri updater adapter. Endpoint, public key and installation
//! location are supplied by the shell, never by an IPC caller.
use std::time::Duration;

use app_core::updates::{
    UpdateBackend, UpdateCheck, UpdateDownloadStatus, UpdateError, UpdateFuture, UpdateRelease,
};
use tauri::{AppHandle, Runtime};
use tauri_plugin_updater::{Update, Updater, UpdaterExt};
use tokio::sync::{oneshot, watch};

/// The plugin buffers the whole signed package. Bound that buffer rather than
/// allowing a malformed endpoint to consume arbitrary memory.
const MAX_PACKAGE_BYTES: u64 = 1024 * 1024 * 1024;

fn normalized_public_key(key: Option<&str>) -> Option<&str> {
    key.map(str::trim).filter(|key| !key.is_empty())
}

fn configured_public_key() -> Option<&'static str> {
    normalized_public_key(option_env!("LIZZIE_UPDATER_PUBKEY"))
}

pub(crate) fn register<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    register_with_key(builder, configured_public_key())
}

fn register_with_key<R: Runtime>(
    builder: tauri::Builder<R>,
    key: Option<&str>,
) -> tauri::Builder<R> {
    match normalized_public_key(key) {
        Some(key) => builder.plugin(tauri_plugin_updater::Builder::new().pubkey(key).build()),
        None => builder,
    }
}

pub fn backend<R: Runtime>(app: &AppHandle<R>) -> Result<Box<dyn UpdateBackend>, UpdateError> {
    backend_with_key(app, configured_public_key())
}

fn backend_with_key<R: Runtime>(
    app: &AppHandle<R>,
    key: Option<&str>,
) -> Result<Box<dyn UpdateBackend>, UpdateError> {
    // Check before accessing the plugin State: absent signing configuration
    // deliberately means that the updater plugin was never registered.
    let key = normalized_public_key(key).ok_or(UpdateError::SigningNotConfigured)?;
    let updater = app
        .updater_builder()
        .pubkey(key)
        .timeout(Duration::from_secs(30))
        .configure_client(|client| {
            client
                .connect_timeout(Duration::from_secs(20))
                .read_timeout(Duration::from_secs(30))
        })
        .build()
        .map_err(failure)?;
    Ok(Box::new(OfficialUpdater {
        updater,
        selected: None,
        #[cfg(test)]
        allow_loopback: false,
    }))
}

struct OfficialUpdater {
    updater: Updater,
    selected: Option<Update>,
    #[cfg(test)]
    allow_loopback: bool,
}

impl UpdateBackend for OfficialUpdater {
    fn check(&mut self) -> UpdateFuture<'_, Option<UpdateRelease>> {
        Box::pin(async move {
            self.selected = None;
            let Some(update) = self.updater.check().await.map_err(failure)? else {
                return Ok(None);
            };
            let secure = update.download_url.scheme() == "https";
            #[cfg(test)]
            let secure = secure
                || (self.allow_loopback && update.download_url.host_str() == Some("127.0.0.1"));
            if !secure {
                return Err(UpdateError::Backend("package URL must use HTTPS".into()));
            }
            let release = UpdateRelease {
                sha256: expected_hash(&update)?,
                view: UpdateCheck {
                    available: true,
                    version: Some(update.version.clone()),
                    notes: update.body.clone(),
                    published_at: update
                        .raw_json
                        .get("pub_date")
                        .and_then(|value| value.as_str())
                        .map(str::to_owned),
                },
            };
            self.selected = Some(update);
            Ok(Some(release))
        })
    }

    fn download(
        &mut self,
        progress: watch::Sender<UpdateDownloadStatus>,
    ) -> UpdateFuture<'_, Vec<u8>> {
        Box::pin(async move {
            let update = self.selected.as_ref().ok_or(UpdateError::NotAvailable)?;
            let (limit, exceeded) = oneshot::channel();
            let mut limit = Some(limit);
            let mut received = 0u64;
            let download = update.download(
                |length, total| {
                    received = received.saturating_add(length as u64);
                    if (received > MAX_PACKAGE_BYTES
                        || total.is_some_and(|total| total > MAX_PACKAGE_BYTES))
                        && let Some(limit) = limit.take()
                    {
                        let _ = limit.send(());
                    }
                    progress.send_modify(|status| {
                        status.downloaded_bytes = received;
                        status.total_bytes = total;
                    });
                },
                || {},
            );
            tokio::pin!(download);
            tokio::select! {
                biased;
                _ = exceeded => Err(UpdateError::Backend("update package exceeds 1 GiB".into())),
                bytes = &mut download => bytes.map_err(failure),
            }
        })
    }

    fn install(&mut self, bytes: Vec<u8>) -> UpdateFuture<'_, ()> {
        Box::pin(async move {
            let update = self.selected.clone().ok_or(UpdateError::NotAvailable)?;
            // install verifies again before touching the app bundle or invoking
            // an installer. Move its filesystem work off the Tokio executor.
            tokio::task::spawn_blocking(move || update.install(bytes))
                .await
                .map_err(failure)?
                .map_err(failure)?;
            Ok(())
        })
    }
}

fn failure(error: impl std::fmt::Display) -> UpdateError {
    UpdateError::Backend(error.to_string())
}

fn expected_hash(update: &Update) -> Result<[u8; 32], UpdateError> {
    let platforms = update
        .raw_json
        .get("platforms")
        .and_then(|value| value.as_object());
    let entries: Vec<_> = match platforms {
        Some(platforms) => platforms
            .values()
            .filter(|value| {
                value.get("url").and_then(|value| value.as_str())
                    == Some(update.download_url.as_str())
                    && value.get("signature").and_then(|value| value.as_str())
                        == Some(update.signature.as_str())
            })
            .collect(),
        None => vec![&update.raw_json],
    };
    let mut expected = None;
    for entry in entries {
        let text = entry
            .get("sha256")
            .and_then(|value| value.as_str())
            .ok_or_else(|| failure("manifest package is missing sha256"))?;
        if text.len() != 64 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(failure(
                "manifest sha256 must contain 64 hexadecimal digits",
            ));
        }
        let mut hash = [0; 32];
        for (index, byte) in hash.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).map_err(failure)?;
        }
        if expected.is_some_and(|previous| previous != hash) {
            return Err(failure("ambiguous manifest package sha256"));
        }
        expected = Some(hash);
    }
    expected.ok_or_else(|| failure("manifest package is missing sha256"))
}

#[cfg(test)]
mod tests;
