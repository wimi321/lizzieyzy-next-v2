#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn partial_write_failure_keeps_target_and_removes_temporary_file() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("slot.json");
    std::fs::write(&target, b"original").unwrap();
    let result = persist_with(target.clone(), false, |file| {
        file.write_all(b"partial")?;
        Err(io::Error::other("injected failure after partial write"))
    });
    assert!(result.is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);

    let absent = directory.path().join("new.json");
    assert!(
        persist_with(absent.clone(), true, |file| {
            file.write_all(b"partial")?;
            Err(io::Error::other("injected write failure"))
        })
        .is_err()
    );
    assert!(!absent.exists());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn rename_failure_removes_temporary_file_and_keeps_existing_directory() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("slot.json");
    std::fs::create_dir(&target).unwrap();
    assert!(persist_with(target.clone(), false, |file| file.write_all(b"complete")).is_err());
    assert!(target.is_dir());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
