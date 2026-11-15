#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;

use app_core::{AppCore, AppError, AppEvent, AppSubscription, DocumentView, Settings};
use common::within;
use serde_json::{Value, json};
use std::path::Path;

const BRANCHES: &[u8] = b"(;SZ[9:13]C[root];B[aa](;W[bb];B[cc])(;W[dd]C[side];B[ee]))";

async fn start(directory: &Path) -> (AppCore, AppSubscription) {
    within(
        Settings {
            autosave: false,
            auto_quick_analyze_on_load: false,
            cache_enabled: false,
            ..Settings::default()
        }
        .save(directory),
    )
    .await
    .unwrap();
    within(app_core::spawn(directory)).await.unwrap()
}

async fn document(events: &mut AppSubscription, revision: u64) -> DocumentView {
    within(async {
        loop {
            if let AppEvent::DocumentChanged {
                revision: actual,
                view,
            } = events.recv().await.unwrap()
                && actual == revision
            {
                return view;
            }
        }
    })
    .await
}

fn metadata(directory: &Path, id: &str) -> (std::path::PathBuf, Value) {
    let path = directory.join("saves").join(id).join("metadata.json");
    let json = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    (path, json)
}

#[tokio::test]
async fn all_slot_commands_preserve_document_state_and_roundtrip_current_branch() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("named source.sgf");
    std::fs::write(&source, BRANCHES).unwrap();
    let (app, mut events) = start(directory.path()).await;
    within(app.open_path(&source)).await.unwrap();
    let revision = within(app.go_to(4)).await.unwrap();
    let before = document(&mut events, revision).await;
    assert_eq!(before.comment.as_deref(), Some("side"));
    let saved = within(app.save_to_new_slot("  ".into())).await.unwrap();
    assert_eq!(saved.name, "named source.sgf");
    assert_eq!(saved.node_path, [0, 1]);
    assert_eq!(saved.move_number, 2);
    let board = saved.board.as_ref().unwrap();
    assert_eq!((board.width, board.height), (9, 13));
    assert_eq!(board.stones, before.board.as_ref().unwrap().stones);
    assert!(!saved.is_autosave);
    assert!(!saved.corrupt);

    let other = within(app.save_to_new_slot("  second  ".into()))
        .await
        .unwrap();
    assert_eq!(other.name, "second");
    assert_ne!(saved.id, other.id);
    // Filesystem metadata values are intentional fixture input; deterministic
    // ordering does not depend on two saves landing in different milliseconds.
    let (path, mut json) = metadata(directory.path(), &saved.id);
    json["slot"]["saved_at_unix_ms"] = json!(1);
    std::fs::write(path, serde_json::to_vec(&json).unwrap()).unwrap();
    let slots = within(app.save_slots()).await.unwrap();
    assert_eq!(
        slots.iter().map(|slot| &slot.id).collect::<Vec<_>>(),
        [&other.id, &saved.id]
    );

    within(app.set_comment("changed after save".into()))
        .await
        .unwrap();
    let renamed = within(app.rename_slot(saved.id.clone(), "  remembered  ".into()))
        .await
        .unwrap();
    assert_eq!(renamed.name, "remembered");
    assert_eq!(renamed.saved_at_unix_ms, 1);
    let revision = within(app.step(1)).await.unwrap();
    let still_modified = document(&mut events, revision).await;
    assert!(still_modified.modified);
    assert_eq!(still_modified.file_path, before.file_path);

    let replaced = within(app.overwrite_slot(saved.id.clone())).await.unwrap();
    assert_eq!(replaced.name, "remembered");
    assert_eq!(replaced.node_path, [0, 1, 0]);
    assert_eq!(replaced.move_number, 3);
    assert_eq!(within(app.save_slots()).await.unwrap().len(), 2);
    let revision = within(app.load_slot(other.id.clone())).await.unwrap();
    let loaded = document(&mut events, revision).await;
    assert_eq!(loaded.comment.as_deref(), Some("side"));
    assert_eq!(loaded.board, before.board);
    assert!(!loaded.modified);
    assert!(!loaded.can_undo);
    assert_eq!(
        loaded.file_path, None,
        "private SGF generations cannot become a Save target"
    );

    within(app.delete_slot(other.id.clone())).await.unwrap();
    assert!(!directory.path().join("saves").join(&other.id).exists());
    assert_eq!(within(app.save_slots()).await.unwrap()[0].id, saved.id);
    within(app.delete_all_slots()).await.unwrap();
    assert!(within(app.save_slots()).await.unwrap().is_empty());
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn names_are_validated_at_actor_boundary_without_changing_saved_slot() {
    let directory = tempfile::tempdir().unwrap();
    let (app, _) = start(directory.path()).await;
    let saved = within(app.save_to_new_slot(String::new())).await.unwrap();
    assert!(!saved.name.is_empty());
    assert!(saved.name.ends_with(".sgf"));
    for name in [
        String::new(),
        " \t ".into(),
        "bad\nname".into(),
        "bad\0name".into(),
        "x".repeat(1025),
    ] {
        assert!(
            within(app.rename_slot(saved.id.clone(), name))
                .await
                .is_err()
        );
        assert_eq!(within(app.save_slots()).await.unwrap(), vec![saved.clone()]);
    }
    for name in ["bad\nname".to_owned(), "bad\0name".into(), "x".repeat(1025)] {
        assert!(within(app.save_to_new_slot(name)).await.is_err());
    }
    let renamed = within(app.rename_slot(saved.id.clone(), "  中文存档  ".into()))
        .await
        .unwrap();
    assert_eq!(renamed.name, "中文存档");
    for id in ["../outside", "/tmp/outside", "", "autosave/../../outside"] {
        assert!(within(app.load_slot(id.into())).await.is_err());
        assert!(within(app.delete_slot(id.into())).await.is_err());
        assert!(within(app.overwrite_slot(id.into())).await.is_err());
    }
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn invalid_node_path_falls_back_to_same_mainline_move_then_last_move() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = start(directory.path()).await;
    within(app.open_bytes(BRANCHES.to_vec())).await.unwrap();
    within(app.go_to(4)).await.unwrap();
    let slot = within(app.save_to_new_slot("branch".into())).await.unwrap();
    let (path, mut json) = metadata(directory.path(), &slot.id);
    json["slot"]["node_path"] = json!([0, 9]);
    std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    let revision = within(app.load_slot(slot.id.clone())).await.unwrap();
    let fallback = document(&mut events, revision).await;
    assert_eq!(fallback.tree.current, 2);
    assert_eq!(fallback.board.unwrap().move_number, 2);

    json["slot"]["move_number"] = json!(999);
    std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    let revision = within(app.load_slot(slot.id)).await.unwrap();
    let last = document(&mut events, revision).await;
    assert_eq!(last.tree.current, 3);
    assert_eq!(last.board.unwrap().move_number, 3);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn slot_load_reuses_trial_guard_and_failed_load_preserves_current_document() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = start(directory.path()).await;
    within(app.open_bytes(BRANCHES.to_vec())).await.unwrap();
    let slot = within(app.save_to_new_slot("original".into()))
        .await
        .unwrap();
    within(app.start_trial()).await.unwrap();
    let revision = within(app.set_comment("trial edits".into())).await.unwrap();
    let before = document(&mut events, revision).await;
    assert!(matches!(
        within(app.load_slot(slot.id.clone())).await,
        Err(AppError::Trial(_))
    ));
    assert!(matches!(
        within(app.overwrite_slot(slot.id.clone())).await,
        Err(AppError::Trial(_))
    ));
    let trial_export = within(app.save_to_new_slot("trial export".into()))
        .await
        .unwrap();
    assert_ne!(slot.id, trial_export.id);
    assert_eq!(
        within(app.variation_tree()).await.unwrap().revision,
        before.tree.structure_revision
    );
    within(app.end_trial()).await.unwrap();

    let (metadata_path, _) = metadata(directory.path(), &slot.id);
    std::fs::write(&metadata_path, b"{ broken metadata").unwrap();
    let tree = within(app.variation_tree()).await.unwrap();
    let slots = within(app.save_slots()).await.unwrap();
    assert!(
        slots
            .iter()
            .find(|entry| entry.id == slot.id)
            .unwrap()
            .corrupt
    );
    assert!(within(app.load_slot(slot.id.clone())).await.is_err());
    assert_eq!(within(app.variation_tree()).await.unwrap(), tree);
    within(app.delete_slot(slot.id.clone())).await.unwrap();
    assert!(
        within(app.save_slots())
            .await
            .unwrap()
            .iter()
            .all(|entry| entry.id != slot.id)
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn unreadable_save_directory_and_broken_sgf_do_not_break_listing_or_startup() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("saves"), b"not a directory").unwrap();
    let (app, _) = start(directory.path()).await;
    assert!(within(app.save_slots()).await.unwrap().is_empty());
    assert!(within(app.save_to_new_slot("new".into())).await.is_err());
    within(app.shutdown()).await.unwrap();

    let directory = tempfile::tempdir().unwrap();
    let (app, _) = start(directory.path()).await;
    let slot = within(app.save_to_new_slot("broken SGF".into()))
        .await
        .unwrap();
    let (_, json) = metadata(directory.path(), &slot.id);
    let path = directory
        .path()
        .join("saves")
        .join(&slot.id)
        .join(json["sgf_file"].as_str().unwrap());
    std::fs::write(path, b"half an SGF (").unwrap();
    assert!(within(app.save_slots()).await.unwrap()[0].corrupt);
    assert!(within(app.load_slot(slot.id.clone())).await.is_err());
    within(app.delete_slot(slot.id)).await.unwrap();
    assert!(within(app.save_slots()).await.unwrap().is_empty());
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn diagnostics_include_only_slot_count_and_no_saved_name_metadata_or_sgf() {
    use std::io::Read;

    let directory = tempfile::tempdir().unwrap();
    let (app, _) = start(directory.path()).await;
    within(app.open_bytes(b"(;SZ[9]PB[PrivatePlayer765]C[PrivateComment987];B[aa])".to_vec()))
        .await
        .unwrap();
    let saved = within(app.save_to_new_slot("PrivateSlotName123".into()))
        .await
        .unwrap();
    let (path, _) = metadata(directory.path(), &saved.id);
    // Even corrupt metadata is counted without being read into the bundle.
    std::fs::write(path, b"PrivateBrokenMetadata456").unwrap();
    within(app.save_to_new_slot("PrivateSlotName789".into()))
        .await
        .unwrap();
    let bundle =
        within(app.export_diagnostics(app_core::RepairResources::default(), "test-slots".into()))
            .await
            .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(bundle).unwrap()).unwrap();
    let mut entries = std::collections::BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let name = entry.name().unwrap().into_owned();
        assert!(!name.starts_with("saves/"));
        let mut text = String::new();
        entry.read_to_string(&mut text).unwrap();
        entries.insert(name, text);
    }
    let system: Value = serde_json::from_str(&entries["system.json"]).unwrap();
    assert_eq!(system["save_slot_count"], 2);
    let all = entries.values().cloned().collect::<String>();
    for private in [
        "PrivatePlayer765",
        "PrivateComment987",
        "PrivateSlotName123",
        "PrivateSlotName789",
        "PrivateBrokenMetadata456",
        "B[aa]",
    ] {
        assert!(!all.contains(private), "diagnostics leaked {private}");
    }
    within(app.shutdown()).await.unwrap();
}
