#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{application_directories, java_content_roots, platform_directory};

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

#[test]
fn packaged_platforms_match_java_layout_and_never_guess_other_architectures() {
    assert_eq!(platform_directory("macos", "aarch64"), Some("macos-arm64"));
    assert_eq!(platform_directory("macos", "x86_64"), Some("macos-amd64"));
    assert_eq!(platform_directory("windows", "x86_64"), Some("windows-x64"));
    assert_eq!(platform_directory("linux", "x86_64"), Some("linux-x64"));
    assert_eq!(platform_directory("linux", "aarch64"), None);
    assert_eq!(platform_directory("windows", "aarch64"), None);
}

#[test]
fn java_content_roots_use_only_fixed_installer_and_portable_locations() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let installed = root.join("installed");
    let home = root.join("home");
    let applications = [root.join("current-app")];
    let mac = java_content_roots(
        "macos",
        &applications,
        Some(&home),
        std::slice::from_ref(&installed),
    );
    assert_eq!(
        mac,
        [
            installed.join("LizzieYzy Next.app/Contents/app"),
            home.join("Applications/LizzieYzy Next.app/Contents/app"),
            applications[0].join("LizzieYzy Next.app/Contents/app"),
        ]
    );
    let windows = java_content_roots(
        "windows",
        &applications,
        Some(&home),
        std::slice::from_ref(&installed),
    );
    assert!(windows.contains(&installed.join("LizzieYzy Next/app")));
    assert!(windows.contains(&installed.join("LizzieYzy Next NVIDIA/app")));
    assert!(windows.contains(&installed.join("LizzieYzy Next OpenCL/app")));
    assert!(windows.contains(&applications[0].join("app")));
    assert!(!windows.iter().any(|path| path.starts_with(&home)));
    let linux = java_content_roots(
        "linux",
        &applications,
        Some(&home),
        std::slice::from_ref(&installed),
    );
    assert_eq!(
        linux,
        [
            installed.join("Lizzieyzy"),
            applications[0].join("Lizzieyzy"),
            home.join("Lizzieyzy"),
        ]
    );
}
