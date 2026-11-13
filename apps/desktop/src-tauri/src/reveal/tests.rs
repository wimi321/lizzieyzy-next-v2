#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn native_platform_rejects_missing_file_before_opening_a_file_manager() {
    let directory = tempfile::TempDir::new().unwrap();
    assert!(
        NativePlatform
            .reveal(&directory.path().join("missing.zip"))
            .is_err()
    );
}

#[test]
fn linux_fallback_opens_only_the_folder_as_one_native_argument() {
    let directory = tempfile::TempDir::new().unwrap();
    let folder = directory.path().join("日志 spaces,comma;$(literal)");
    let file = folder.join("诊断, with spaces.zip");
    assert_eq!(fallback_folder(&file, false).unwrap(), folder);
    assert_eq!(fallback_folder(&folder, true).unwrap(), folder);
    let command = linux_fallback_command(&folder);
    assert_eq!(command.get_program(), "xdg-open");
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        vec![folder.as_os_str()]
    );
}
