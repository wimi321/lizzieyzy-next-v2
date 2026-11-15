use super::Actor;
use crate::{AppError, ClipboardError, everyday::decode_png, storage};
use std::path::PathBuf;

impl Actor {
    pub(super) fn valid_saved_nodes(&self) -> Vec<document::NodeId> {
        self.review
            .results
            .keys()
            .copied()
            .filter(|id| {
                self.stored_analysis(id.0)
                    .is_some_and(|stored| stored.saved.is_some())
            })
            .collect()
    }

    pub(super) async fn copy_sgf(&self) -> Result<(), AppError> {
        let clipboard = self
            .config
            .clipboard
            .as_ref()
            .ok_or(ClipboardError::Unavailable)?;
        let (tree, _) = self.export_document_with_analysis()?;
        clipboard.write_text(serialize(tree)).await?;
        Ok(())
    }

    pub(super) async fn paste_sgf(&mut self) -> Result<(), AppError> {
        let clipboard = self
            .config
            .clipboard
            .as_ref()
            .ok_or(ClipboardError::Unavailable)?;
        let text = clipboard
            .read_text()
            .await?
            .filter(|text| !text.trim().is_empty())
            .ok_or(ClipboardError::EmptyText)?;
        // Reuse format detection, document replacement, saved-analysis import,
        // live focus and automatic-curve scheduling used by ordinary opens.
        self.open(text.into_bytes(), None).await?;
        self.file_state.mark_unsaved();
        self.publish_document()?;
        Ok(())
    }

    pub(super) async fn clipboard_link(&self) -> Option<String> {
        let clipboard = self.config.clipboard.as_ref()?;
        // Clipboard errors may contain platform text; neither it nor the
        // clipboard contents belong in logs or in a prefill error response.
        let text = clipboard.read_text().await.ok()??;
        validated_link(&text)
    }

    pub(super) async fn save_clean_sgf(
        &self,
        path: PathBuf,
        comments: bool,
        branch: bool,
    ) -> Result<(), AppError> {
        if super::game_files::is_gib(&path) {
            return Err(AppError::GibSaveTarget);
        }
        let bytes = serialize(self.document.to_clean_sgf(comments, branch)).into_bytes();
        if self.move_trial.is_some() {
            storage::write_new(path, bytes).await?;
        } else {
            storage::write(path, bytes).await?;
        }
        Ok(())
    }

    pub(super) async fn save_png(&self, path: PathBuf, bytes: Vec<u8>) -> Result<(), AppError> {
        let (bytes, _) = decode_png(bytes).await?;
        storage::write(path, bytes).await?;
        Ok(())
    }

    pub(super) async fn copy_png(&self, bytes: Vec<u8>) -> Result<(), AppError> {
        let clipboard = self
            .config
            .clipboard
            .as_ref()
            .ok_or(ClipboardError::Unavailable)?;
        let (_, image) = decode_png(bytes).await?;
        clipboard.write_image(image).await?;
        Ok(())
    }
}

fn validated_link(text: &str) -> Option<String> {
    let text = text.trim();
    let text = ['"', '\'']
        .into_iter()
        .find_map(|quote| text.strip_prefix(quote)?.strip_suffix(quote))
        .unwrap_or(text)
        .trim();
    if text.len() > 4 * 1024
        || text.chars().any(|c| c.is_whitespace() || c.is_control())
        || text.contains('\\')
        || text.starts_with(['"', '\''])
        || text.ends_with(['"', '\''])
    {
        return None;
    }
    // Require an explicit authority before parsing: Url can repair missing
    // slashes/hosts and discard empty user-info such as `https://@example.org`.
    let (_, rest) = text.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let parsed = url::Url::parse(text).ok()?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none_or(str::is_empty)
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return None;
    }
    // Preserve signed paths/query strings instead of serializing the parsed URL.
    Some(text.to_owned())
}

fn serialize(tree: sgf::GameTree) -> String {
    sgf::serialize(&sgf::Collection {
        games: vec![tree],
        encoding: "UTF-8".into(),
        warnings: Vec::new(),
    })
}
