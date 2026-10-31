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
        ..SearchRoots::default()
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
        ..SearchRoots::default()
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

#[test]
fn counters_accept_millions_and_billions_and_reject_overflow() {
    assert_eq!(
        network_counter("kata1-tf3-b11-s12002M-d6304M.bin.gz", 's'),
        12_002_000_000
    );
    assert_eq!(
        network_counter("kata1-tf3-b11-s12002m-d6304m.bin.gz", 'd'),
        6_304_000_000
    );
    assert_eq!(
        network_counter("kata1-b18-s12g.bin.gz", 's'),
        12_000_000_000
    );
    assert_eq!(
        network_counter("kata1-b18-s18446744073709551615g.bin.gz", 's'),
        0
    );
    assert_eq!(network_counter("kata1-b18-sbroken.bin.gz", 's'), 0);
}

#[test]
fn discovers_explicit_resource_and_java_content_roots_on_all_platforms() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for (java_layout, platform, executable_name) in [
        (
            "Applications/LizzieYzy Next.app/Contents/app",
            "macos-arm64",
            "katago",
        ),
        (
            "home/Applications/LizzieYzy Next.app/Contents/app",
            "macos-amd64",
            "katago",
        ),
        (
            "Program Files/LizzieYzy Next/app",
            "windows-x64",
            "katago.exe",
        ),
        ("opt/Lizzieyzy", "linux-x64", "katago"),
    ] {
        let resource = root.join(platform).join("resources");
        let java = root.join(java_layout);
        let resource_program = executable(
            resource
                .join("engines/katago")
                .join(platform)
                .join(executable_name),
        );
        let java_program = executable(
            java.join("engines/katago")
                .join(platform)
                .join(executable_name),
        );
        let resource_model = file(resource.join("weights/custom.bin.gz"));
        let java_model = file(java.join("weights/default.bin.gz"));
        let config = file(java.join("engines/katago/configs/analysis.cfg"));
        executable(resource.join("engines/katago/unrequested-platform/katago"));
        let detected = detect_candidates(&SearchRoots {
            resource_dirs: vec![resource],
            java_install_dirs: vec![java],
            platform_dirs: vec![platform.into()],
            ..SearchRoots::default()
        });
        assert_eq!(
            detected.discovery.programs,
            [resource_program.clone(), java_program.clone()]
        );
        assert_eq!(
            detected.discovery.models,
            [resource_model.clone(), java_model.clone()]
        );
        assert_eq!(detected.discovery.configs, [config]);
        assert_eq!(
            detected.programs_for_model(&java_model),
            [java_program.clone(), resource_program.clone()]
        );
        assert_eq!(
            detected.programs_for_model(&resource_model),
            [resource_program, java_program]
        );
    }
}

#[test]
fn catalog_hashes_rank_renamed_b11_before_b28_b18_and_other_tiers() {
    use sha2::{Digest, Sha256};

    let temp = tempfile::tempdir().unwrap();
    let install = temp.path().join("katago");
    let b28 = file(install.join("kata1-b28-s999999999999-d99.bin.gz"));
    let b18 = file(install.join("kata1-b18-s999999999999-d98.bin.gz"));
    let canonical = "kata1-tf3-b11c768-s12002M-d6304M.bin.gz";
    let mut catalog = catalog::ModelCatalog {
        default_model_id: "b11".into(),
        ..catalog::ModelCatalog::default()
    };
    let mut ranked = Vec::new();
    for (id, name, tier) in [
        ("b11", "default.bin.gz", "strongest"),
        ("strong", "strong.bin.gz", "strongest"),
        ("balanced", "balanced.bin.gz", "balanced"),
        ("light", "light.bin.gz", "lightweight"),
    ] {
        let path = file(install.join(name));
        fs::write(&path, id).unwrap();
        catalog.models.insert(
            id.into(),
            CatalogModel {
                file_name: if id == "b11" {
                    canonical.into()
                } else {
                    name.into()
                },
                size_bytes: id.len() as u64,
                sha256: format!("{:x}", Sha256::digest(id.as_bytes())),
                tier: tier.into(),
                minimum_katago_version: "1.17.0".into(),
            },
        );
        ranked.push(path);
    }
    let roots = SearchRoots {
        application_dirs: vec![temp.path().into()],
        ..SearchRoots::default()
    };
    let detected = detect_with_catalog(&roots, &catalog);
    assert_eq!(detected.catalog_models[&ranked[0]].file_name, canonical);
    ranked.extend([b28, b18]);
    assert_eq!(detected.discovery.models, ranked);
    let canonical_path = install.join(canonical);
    fs::rename(&ranked[0], &canonical_path).unwrap();
    assert_eq!(
        detect_with_catalog(&roots, &catalog).discovery.models[0],
        canonical_path
    );
}

#[cfg(unix)]
#[test]
fn program_found_via_path_keeps_its_later_installation_association() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let own = temp.path().join("own");
    let other = temp.path().join("other");
    let own_program = executable(own.join("bin/katago"));
    let other_program = executable(other.join("bin/katago"));
    let model = file(own.join("models/custom.bin.gz"));
    let alias_dir = temp.path().join("path");
    fs::create_dir(&alias_dir).unwrap();
    let alias = alias_dir.join("katago");
    symlink(&own_program, &alias).unwrap();
    let detected = detect_candidates(&SearchRoots {
        path_dirs: vec![other.join("bin"), alias_dir],
        windows_dirs: vec![own, other],
        ..SearchRoots::default()
    });
    assert_eq!(
        detected.discovery.programs,
        [other_program.clone(), alias.clone()]
    );
    assert_eq!(detected.programs_for_model(&model), [alias, other_program]);
}
