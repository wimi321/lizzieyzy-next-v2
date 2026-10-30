#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

fn file(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "fixture").unwrap();
    path.to_owned()
}

fn executable(path: impl AsRef<Path>) -> PathBuf {
    let path = file(path);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

#[test]
fn empty_roots_do_not_search_host_path_or_homebrew() {
    assert_eq!(detect(&SearchRoots::default()), EngineDiscovery::default());
}

#[test]
fn discovers_path_homebrew_application_and_windows_layouts() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let path_program = executable(root.join("path/katago"));
    let brew_program = executable(root.join("brew/bin/katago"));
    let brew_model = file(root.join("brew/share/katago/kata1-b18-s100-d200.bin.gz"));
    let brew_config = file(root.join("brew/share/katago/analysis.cfg"));
    let app_program = executable(root.join("app/katago/bin/katago"));
    let app_model = file(root.join("app/katago/models/kata1-b18-s200-d300.bin.gz"));
    let app_config = file(root.join("app/katago/config/analysis.cfg"));
    let windows_program = executable(root.join("Program Files/KataGo/katago.exe"));
    let windows_model = file(root.join("Program Files/KataGo/g170e-b20-s500-d600.txt.gz"));
    let windows_config = file(root.join("Program Files/KataGo/default.cfg"));
    let result = detect(&SearchRoots {
        path_dirs: vec![root.join("path")],
        homebrew_prefixes: vec![root.join("brew")],
        application_dirs: vec![root.join("app")],
        windows_dirs: vec![root.join("Program Files/KataGo")],
    });
    assert_eq!(
        result.programs,
        [path_program, brew_program, app_program, windows_program]
    );
    assert_eq!(result.models, [app_model, brew_model, windows_model]);
    let mut configs = vec![brew_config, app_config, windows_config];
    configs.sort();
    assert_eq!(result.configs, configs);
}

#[test]
fn ranks_families_and_numeric_training_counters_deterministically() {
    let temp = tempfile::tempdir().unwrap();
    let install = temp.path().join("katago");
    let names = [
        "custom.bin",
        "g170e-b40-s999999-d999999.bin.gz",
        "kata1-b40-s9-d900.bin.gz",
        "kata1-b18-s100-d99.bin.gz",
        "kata1-b18-s100-d100.bin.gz",
        "kata1-b6-s100-d100.bin.gz",
        "KATA1-b40-s200-d10.BIN.GZ",
    ];
    for name in names {
        file(install.join(name));
    }
    let roots = SearchRoots {
        application_dirs: vec![temp.path().to_owned()],
        ..SearchRoots::default()
    };
    let expected = [6, 4, 5, 3, 2, 1, 0].map(|index| install.join(names[index]));
    assert_eq!(detect(&roots).models, expected);
    assert_eq!(detect(&roots).models, expected);
}

#[test]
fn skips_missing_roots_directories_and_unrelated_files_without_recursing() {
    let temp = tempfile::tempdir().unwrap();
    let install = temp.path().join("katago");
    fs::create_dir_all(install.join("katago")).unwrap();
    fs::create_dir_all(install.join("directory.bin.gz")).unwrap();
    file(install.join("README.txt"));
    file(install.join("other.exe"));
    file(install.join("nested/deep/kata1.bin.gz"));
    file(install.join(".bin.gz"));
    let model = file(install.join("networks/network.txt.gz"));
    let config = file(install.join("configs/custom.cfg"));
    let result = detect(&SearchRoots {
        path_dirs: vec![temp.path().join("missing-path")],
        homebrew_prefixes: vec![temp.path().join("missing-brew")],
        application_dirs: vec![temp.path().to_owned()],
        windows_dirs: vec![temp.path().join("missing-windows")],
    });
    assert!(result.programs.is_empty());
    assert_eq!(result.models, [model]);
    assert_eq!(result.configs, [config]);
}

#[test]
fn duplicate_roots_do_not_duplicate_candidates() {
    let temp = tempfile::tempdir().unwrap();
    let install = temp.path().join("katago");
    let program = executable(install.join("katago"));
    let model = file(install.join("kata1-b18-s1-d1.bin.gz"));
    let config = file(install.join("analysis.cfg"));
    let result = detect(&SearchRoots {
        path_dirs: vec![install.clone(), install.clone()],
        application_dirs: vec![temp.path().to_owned()],
        windows_dirs: vec![install],
        ..SearchRoots::default()
    });
    assert_eq!(result.programs, [program]);
    assert_eq!(result.models, [model]);
    assert_eq!(result.configs, [config]);
}

#[cfg(unix)]
#[test]
fn symlinks_are_deduplicated_broken_links_and_nonexecutables_are_skipped() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let install = temp.path().join("katago");
    let program = executable(install.join("katago"));
    file(install.join("bin/katago"));
    let model = file(install.join("kata1-b18-s1-d1.bin.gz"));
    let config = file(install.join("analysis.cfg"));
    symlink(&program, install.join("katago.exe")).unwrap();
    symlink(&model, install.join("network.bin.gz")).unwrap();
    symlink(&config, install.join("duplicate.cfg")).unwrap();
    symlink(install.join("missing"), install.join("broken.bin.gz")).unwrap();
    let result = detect(&SearchRoots {
        application_dirs: vec![temp.path().to_owned()],
        ..SearchRoots::default()
    });
    assert_eq!(result.programs, [program]);
    assert_eq!(result.models, [model]);
    assert_eq!(result.configs, [config]);
}
