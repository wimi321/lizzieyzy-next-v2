#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use std::{borrow::Cow, collections::BTreeMap};
use tauri::{
    Assets,
    test::{MockRuntime, mock_builder},
    utils::assets::{AssetKey, AssetsIter, CspHash},
};

struct TestAssets(BTreeMap<String, Vec<u8>>);

impl Assets<MockRuntime> for TestAssets {
    fn get(&self, key: &AssetKey) -> Option<Cow<'_, [u8]>> {
        self.0
            .get(key.as_ref())
            .map(|bytes| Cow::Borrowed(bytes.as_slice()))
    }

    fn iter(&self) -> Box<AssetsIter<'_>> {
        Box::new(
            self.0
                .iter()
                .map(|(key, bytes)| (Cow::Borrowed(key.as_str()), Cow::Borrowed(bytes.as_slice()))),
        )
    }

    fn csp_hashes(&self, _: &AssetKey) -> Box<dyn Iterator<Item = CspHash<'_>> + '_> {
        Box::new(std::iter::empty())
    }
}

fn app(with_web: bool) -> tauri::App<MockRuntime> {
    let mut context = crate::context();
    // Exercise the production embedded resolver rather than its devUrl disk fallback.
    context.config_mut().build.dev_url = None;
    let mut assets = BTreeMap::from([
        ("/index.html".into(), b"desktop private UI".to_vec()),
        (
            "/assets/board.js".into(),
            b"export const board = true;".to_vec(),
        ),
        ("/secret.json".into(), b"private".to_vec()),
    ]);
    if with_web {
        assets.insert(
            "/web.html".into(),
            b"<!doctype html><title>Spectator</title>".to_vec(),
        );
    }
    context.set_assets(Box::new(TestAssets(assets)));
    mock_builder().build(context).unwrap()
}

#[test]
fn only_exact_spectator_assets_are_served_without_spa_fallback() {
    let app = app(true);
    let provider = EmbeddedWebAssets(app.handle().clone());
    let web = provider.get("web.html").unwrap();
    assert_eq!(web.bytes, b"<!doctype html><title>Spectator</title>");
    assert!(web.mime.starts_with("text/html"));
    assert_eq!(
        provider.get("assets/board.js").unwrap().bytes,
        b"export const board = true;"
    );
    for path in [
        "index.html",
        "secret.json",
        "",
        "/web.html",
        "assets/missing.js",
        "assets/../index.html",
        "assets/./board.js",
        "assets\\board.js",
        "assets/%2e%2e/index.html",
        "assets/%2fsecret.json",
        "assets/",
    ] {
        assert!(provider.get(path).is_none(), "unexpected asset at {path}");
    }
}

#[test]
fn missing_web_entry_is_not_the_desktop_index() {
    let app = app(false);
    assert!(app.asset_resolver().get("web.html".into()).is_some());
    assert!(
        EmbeddedWebAssets(app.handle().clone())
            .get("web.html")
            .is_none()
    );
}
