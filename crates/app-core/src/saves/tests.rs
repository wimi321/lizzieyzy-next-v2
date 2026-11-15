#![allow(clippy::unwrap_used)]

use super::*;
use std::fs;

use crate::test_support::within;

fn game() -> GameDocument {
    let tree = sgf::parse(b"(;SZ[9];B[aa](;W[bb])(;W[cc];B[dd]))").unwrap();
    let mut document = GameDocument::from_sgf(&tree.games[0]).unwrap();
    document
        .apply(document::Command::GoTo(document::NodeId(4)))
        .unwrap();
    document
}

fn bytes(document: &GameDocument) -> Vec<u8> {
    sgf::serialize(&sgf::Collection {
        games: vec![document.to_sgf()],
        encoding: "UTF-8".into(),
        warnings: Vec::new(),
    })
    .into_bytes()
}

fn store(root: &tempfile::TempDir) -> SlotStore {
    SlotStore::new(root.path().to_owned())
}

fn metadata_path(root: &tempfile::TempDir, id: &str) -> PathBuf {
    root.path().join("saves").join(id).join("metadata.json")
}

fn edit_metadata(root: &tempfile::TempDir, id: &str, edit: impl FnOnce(&mut serde_json::Value)) {
    let path = metadata_path(root, id);
    let mut json = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    edit(&mut json);
    fs::write(path, serde_json::to_vec(&json).unwrap()).unwrap();
}

#[tokio::test]
async fn slots_store_full_tree_path_thumbnail_and_default_name_sorted_newest_first() {
    let root = tempfile::tempdir().unwrap();
    let store = store(&root);
    assert!(within(store.list()).await.is_empty());
    assert!(!root.path().join("saves").exists());
    let document = game();
    let first = within(store.create(
        " ".into(),
        "players.sgf".into(),
        &document,
        bytes(&document),
    ))
    .await
    .unwrap();
    assert_eq!(first.name, "players.sgf");
    assert_eq!(first.node_path, vec![0, 1, 0]);
    assert_eq!(first.move_number, 3);
    assert!(!first.is_autosave);
    assert!(!first.corrupt);
    let board = first.board.unwrap();
    assert_eq!((board.width, board.height, board.stones.len()), (9, 9, 81));
    assert_eq!(board.stones[0], Some(ColorView::Black));
    assert_eq!(board.stones[20], Some(ColorView::White));
    assert_eq!(board.stones[30], Some(ColorView::Black));
    let second = within(store.create(
        "  next  ".into(),
        "unused".into(),
        &document,
        bytes(&document),
    ))
    .await
    .unwrap();
    assert_eq!(second.name, "next");
    edit_metadata(&root, &first.id, |json| {
        json["slot"]["saved_at_unix_ms"] = 2.into()
    });
    edit_metadata(&root, &second.id, |json| {
        json["slot"]["saved_at_unix_ms"] = 4.into()
    });
    let slots = within(store.list()).await;
    assert_eq!(
        slots.iter().map(|slot| &slot.id).collect::<Vec<_>>(),
        vec![&second.id, &first.id]
    );
    let (path, loaded) = within(store.load(&first.id)).await.unwrap();
    assert_eq!(loaded.node_path, vec![0, 1, 0]);
    assert_eq!(fs::read(path).unwrap(), bytes(&document));
}

#[tokio::test]
async fn overwrite_commits_a_new_generation_then_removes_previous_sgf() {
    let root = tempfile::tempdir().unwrap();
    let store = store(&root);
    let mut document = game();
    let slot = within(store.create(
        "original".into(),
        "unused".into(),
        &document,
        bytes(&document),
    ))
    .await
    .unwrap();
    let (old_path, _) = within(store.load(&slot.id)).await.unwrap();
    document
        .apply(document::Command::SetComment("saved change".into()))
        .unwrap();
    let updated = within(store.overwrite(&slot.id, &document, bytes(&document)))
        .await
        .unwrap();
    assert_eq!(updated.id, slot.id);
    assert_eq!(updated.name, "original");
    let (new_path, _) = within(store.load(&slot.id)).await.unwrap();
    assert_ne!(new_path, old_path);
    assert!(!old_path.exists());
    assert_eq!(fs::read(&new_path).unwrap(), bytes(&document));
    assert_eq!(fs::read_dir(new_path.parent().unwrap()).unwrap().count(), 2);
    assert_eq!(within(store.list()).await.len(), 1);
}

#[tokio::test]
async fn rename_trims_name_and_rejects_empty_long_or_control_names_without_changes() {
    let root = tempfile::tempdir().unwrap();
    let store = store(&root);
    let document = game();
    let slot = within(store.create("first".into(), "unused".into(), &document, bytes(&document)))
        .await
        .unwrap();
    let renamed = within(store.rename(&slot.id, "  棋局  ".into()))
        .await
        .unwrap();
    assert_eq!(renamed.name, "棋局");
    assert_eq!(renamed.saved_at_unix_ms, slot.saved_at_unix_ms);
    let before = fs::read(metadata_path(&root, &slot.id)).unwrap();
    for name in [
        "".to_owned(),
        "  ".to_owned(),
        "x\n".to_owned(),
        "\tname".to_owned(),
        "name\0x".to_owned(),
        "局".repeat(121),
    ] {
        assert_eq!(
            within(store.rename(&slot.id, name.clone()))
                .await
                .unwrap_err(),
            SaveSlotsError::InvalidName
        );
        if !name.trim().is_empty() || name.chars().any(char::is_control) {
            assert_eq!(
                within(store.create(name, "fallback".into(), &document, bytes(&document)))
                    .await
                    .unwrap_err(),
                SaveSlotsError::InvalidName
            );
        }
        assert_eq!(fs::read(metadata_path(&root, &slot.id)).unwrap(), before);
    }
    assert_eq!(
        within(store.rename(&slot.id, "局".repeat(120)))
            .await
            .unwrap()
            .name
            .chars()
            .count(),
        120
    );
}

#[tokio::test]
async fn automatic_slot_is_distinct_read_only_and_survives_delete_all() {
    let root = tempfile::tempdir().unwrap();
    let store = store(&root);
    let document = game();
    let first = within(store.create(
        "manual".into(),
        "unused".into(),
        &document,
        bytes(&document),
    ))
    .await
    .unwrap();
    let automatic = within(store.autosave("last.sgf".into(), &document, bytes(&document)))
        .await
        .unwrap();
    assert_eq!(automatic.id, AUTOSAVE_ID);
    assert!(automatic.is_autosave);
    assert_eq!(
        within(store.rename(AUTOSAVE_ID, "oops".into()))
            .await
            .unwrap_err(),
        SaveSlotsError::ReadOnlyAutosave
    );
    assert_eq!(
        within(store.overwrite(AUTOSAVE_ID, &document, bytes(&document)))
            .await
            .unwrap_err(),
        SaveSlotsError::ReadOnlyAutosave
    );
    within(store.delete_all()).await.unwrap();
    let slots = within(store.list()).await;
    assert_eq!(slots, vec![automatic]);
    assert!(matches!(
        within(store.load(&first.id)).await,
        Err(SaveSlotsError::Missing { .. })
    ));
    within(store.delete(AUTOSAVE_ID)).await.unwrap();
    assert!(within(store.list()).await.is_empty());
}

#[tokio::test]
async fn manual_slots_limit_is_100_and_does_not_count_autosave() {
    let root = tempfile::tempdir().unwrap();
    let store = store(&root);
    let document = GameDocument::new();
    within(store.autosave("last.sgf".into(), &document, bytes(&document)))
        .await
        .unwrap();
    for index in 0..MAX_SLOTS {
        within(store.create(
            index.to_string(),
            "unused".into(),
            &document,
            bytes(&document),
        ))
        .await
        .unwrap();
    }
    assert_eq!(within(store.count()).await, 101);
    assert_eq!(
        within(store.create(
            "overflow".into(),
            "unused".into(),
            &document,
            bytes(&document)
        ))
        .await
        .unwrap_err(),
        SaveSlotsError::Capacity
    );
    let id = within(store.list())
        .await
        .into_iter()
        .find(|slot| !slot.is_autosave)
        .unwrap()
        .id;
    within(store.delete(&id)).await.unwrap();
    within(store.create(
        "replacement".into(),
        "unused".into(),
        &document,
        bytes(&document),
    ))
    .await
    .unwrap();
}

#[tokio::test]
async fn damaged_sgf_missing_or_invalid_metadata_are_listed_and_can_be_deleted() {
    let root = tempfile::tempdir().unwrap();
    let store = store(&root);
    let document = game();
    let a = within(store.create("a".into(), "unused".into(), &document, bytes(&document)))
        .await
        .unwrap();
    let b = within(store.create("b".into(), "unused".into(), &document, bytes(&document)))
        .await
        .unwrap();
    let c = within(store.create("c".into(), "unused".into(), &document, bytes(&document)))
        .await
        .unwrap();
    let (path, _) = within(store.load(&a.id)).await.unwrap();
    fs::write(&path, b"truncated").unwrap();
    fs::write(metadata_path(&root, &b.id), b"not JSON").unwrap();
    fs::remove_file(metadata_path(&root, &c.id)).unwrap();
    let slots = within(store.list()).await;
    assert_eq!(slots.len(), 3);
    assert!(slots.iter().all(|slot| slot.corrupt));
    for id in [&a.id, &b.id, &c.id] {
        assert!(matches!(
            within(store.load(id)).await,
            Err(SaveSlotsError::Corrupt { .. })
        ));
        within(store.delete(id)).await.unwrap();
    }
    assert!(within(store.list()).await.is_empty());
}

#[tokio::test]
async fn invalid_paths_and_metadata_cannot_reference_files_outside_slot() {
    let root = tempfile::tempdir().unwrap();
    let store = store(&root);
    let document = game();
    let slot = within(store.create("a".into(), "unused".into(), &document, bytes(&document)))
        .await
        .unwrap();
    for id in [
        "../outside",
        "/tmp/file",
        "slot-a/../b",
        "slot-a\\b",
        "slot-..",
    ] {
        assert_eq!(
            within(store.load(id)).await.unwrap_err(),
            SaveSlotsError::InvalidId
        );
        assert_eq!(
            within(store.delete(id)).await.unwrap_err(),
            SaveSlotsError::InvalidId
        );
    }
    edit_metadata(&root, &slot.id, |json| {
        json["sgf_file"] = "../outside.sgf".into()
    });
    assert!(matches!(
        within(store.load(&slot.id)).await,
        Err(SaveSlotsError::Corrupt { .. })
    ));
    assert!(within(store.list()).await[0].corrupt);
}

#[tokio::test]
async fn metadata_commit_failure_removes_new_generation_and_preserves_old_sgf() {
    let root = tempfile::tempdir().unwrap();
    let store = store(&root);
    let document = game();
    let slot = within(store.create("a".into(), "unused".into(), &document, bytes(&document)))
        .await
        .unwrap();
    let (old_path, _) = within(store.load(&slot.id)).await.unwrap();
    let metadata = metadata_path(&root, &slot.id);
    let old_metadata = fs::read(&metadata).unwrap();
    fs::remove_file(&metadata).unwrap();
    fs::create_dir(&metadata).unwrap();
    assert!(
        within(store.overwrite(&slot.id, &document, bytes(&document)))
            .await
            .is_err()
    );
    assert_eq!(fs::read(&old_path).unwrap(), bytes(&document));
    assert_eq!(fs::read_dir(old_path.parent().unwrap()).unwrap().count(), 2);
    fs::remove_dir(&metadata).unwrap();
    fs::write(&metadata, old_metadata).unwrap();
    assert!(!within(store.list()).await[0].corrupt);
}

#[tokio::test]
async fn inaccessible_optional_store_lists_empty_and_count_never_reads_contents() {
    let root = tempfile::tempdir().unwrap();
    let store = store(&root);
    fs::write(root.path().join("saves"), b"not a directory").unwrap();
    assert!(within(store.list()).await.is_empty());
    assert_eq!(within(store.count()).await, 0);
    fs::remove_file(root.path().join("saves")).unwrap();
    fs::create_dir_all(root.path().join("saves/slot-damaged")).unwrap();
    fs::create_dir(root.path().join("saves/autosave")).unwrap();
    fs::write(
        root.path().join("saves/slot-damaged/metadata.json"),
        b"private invalid metadata",
    )
    .unwrap();
    assert_eq!(within(store.count()).await, 2);
    assert_eq!(within(store.list()).await.len(), 2);
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn linked_slot_is_damaged_and_delete_unlinks_without_touching_target() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("keep"), b"original").unwrap();
    fs::create_dir(root.path().join("saves")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), root.path().join("saves/slot-link")).unwrap();
    #[cfg(windows)]
    match std::os::windows::fs::symlink_dir(outside.path(), root.path().join("saves/slot-link")) {
        Ok(()) => {}
        // Creating symlinks requires Developer Mode or the Windows privilege.
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("cannot create test directory link: {error}"),
    }
    let store = store(&root);
    assert!(within(store.list()).await[0].corrupt);
    within(store.delete("slot-link")).await.unwrap();
    assert_eq!(fs::read(outside.path().join("keep")).unwrap(), b"original");
}
