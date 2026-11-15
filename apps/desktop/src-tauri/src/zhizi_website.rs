//! Fixed-destination browser action. No URL or program comes from the WebView.
use crate::error::CommandError;
use std::sync::Arc;

type OpenUrl = dyn Fn(&str) -> Result<(), CommandError> + Send + Sync;

pub(crate) struct ZhiziWebsite {
    open_url: Arc<OpenUrl>,
}

impl Default for ZhiziWebsite {
    fn default() -> Self {
        Self {
            open_url: Arc::new(|url| {
                tauri_plugin_opener::open_url(url, None::<&str>)
                    .map_err(|_| CommandError::BrowserUnavailable)
            }),
        }
    }
}

impl ZhiziWebsite {
    pub(crate) async fn open(&self) -> Result<(), CommandError> {
        let open_url = self.open_url.clone();
        tokio::task::spawn_blocking(move || open_url(app_core::zhizi::website_url()))
            .await
            .map_err(|_| CommandError::BrowserUnavailable)?
    }

    #[cfg(test)]
    pub(crate) fn with_opener(
        open_url: impl Fn(&str) -> Result<(), CommandError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            open_url: Arc::new(open_url),
        }
    }
}
