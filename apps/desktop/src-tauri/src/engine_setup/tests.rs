#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::application_directories;

#[test]
fn application_roots_include_portable_bundle_neighbors() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let executable = root.join("installed/lizzieyzy.exe");
    assert_eq!(
        application_directories(&executable, None),
        [root.join("installed")]
    );
    let mac_executable = root.join("Apps/LizzieYzy.app/Contents/MacOS/lizzieyzy");
    assert_eq!(
        application_directories(&mac_executable, None),
        [
            root.join("Apps/LizzieYzy.app/Contents/MacOS"),
            root.join("Apps"),
        ]
    );
    let linux_executable = root.join(".mount_lizzie/usr/bin/lizzieyzy");
    let appimage = root.join("downloads/LizzieYzy.AppImage");
    assert_eq!(
        application_directories(&linux_executable, Some(&appimage)),
        [root.join(".mount_lizzie/usr/bin"), root.join("downloads"),]
    );
}
