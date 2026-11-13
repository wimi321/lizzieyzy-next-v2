#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use std::path::PathBuf;
#[cfg(unix)]
use std::time::Duration;

fn directory() -> tempfile::TempDir {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    std::fs::create_dir_all(&root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}

fn installed(directory: PathBuf) -> RuntimeInstallation {
    RuntimeInstallation {
        profile_id: "cuda12.8-cudnn9".into(),
        ready: true,
        directory: Some(directory),
        program: None,
        missing: Vec::new(),
        error: None,
    }
}

#[test]
fn missing_runtime_is_diagnosed_and_cannot_be_added_to_a_child_environment() {
    let mut value = installed("missing-runtime".into());
    value.ready = false;
    value.missing = vec!["cudart64_12.dll".into()];
    let check = diagnostic(Some(&value));
    assert_eq!(check.kind, DiagnosticKind::RuntimeComponents);
    assert_eq!(check.status, DiagnosticStatus::Error);
    assert_eq!(check.repairs, [DiagnosticRepair::InstallRuntime]);
    assert!(check.detail.unwrap().contains("cudart64_12.dll"));
    assert!(environment(&value).is_err());
    assert_eq!(diagnostic(None).status, DiagnosticStatus::Skipped);
    assert!(diagnostic(None).repairs.is_empty());
}

#[test]
fn verified_runtime_precedes_inherited_search_path_without_changing_the_parent() {
    let directory = directory();
    let before = std::env::var_os("PATH");
    let env = environment(&installed(directory.path().to_owned())).unwrap();
    let paths =
        std::env::split_paths(env.get(&OsString::from("PATH")).unwrap()).collect::<Vec<_>>();
    assert_eq!(paths[0], directory.path());
    if let Some(inherited) = &before {
        for path in std::env::split_paths(inherited).filter(|path| !path.as_os_str().is_empty()) {
            assert!(paths.contains(&path));
        }
    }
    assert_eq!(std::env::var_os("PATH"), before);
    assert_eq!(
        diagnostic(Some(&installed(directory.path().to_owned()))).status,
        DiagnosticStatus::Ok
    );
}

#[cfg(unix)]
#[tokio::test]
async fn fake_engine_child_observes_installed_runtime_path_and_parent_is_unchanged() {
    // The shell is the system binary, with code passed as arguments. No newly
    // written executable is needed. The actual analysis session is fake-katago.
    let binary = fake_katago::binary_path().unwrap();
    let directory = directory();
    let runtime = directory.path().join("runtime bin");
    std::fs::create_dir(&runtime).unwrap();
    let observed = directory.path().join("child-path.txt");
    let before = std::env::var_os("PATH");
    let profile = crate::EngineProfile {
        program: binary.clone(),
        model: binary.clone(),
        config: binary.clone(),
        extra_args: vec![],
        canonical_model_name: None,
    };
    let mut transport = profile.transport().unwrap();
    let engine_session::TransportSpec::LocalProcess {
        program, args, env, ..
    } = &mut transport;
    *program = "/bin/sh".into();
    let mut wrapper = vec![
        OsString::from("-c"),
        OsString::from(
            "printf '%s' \"$PATH\" > \"$RUNTIME_ENV_RESULT\"; exec \"$RUNTIME_TEST_ENGINE\" \"$@\"",
        ),
        OsString::from("runtime-env-probe"),
    ];
    wrapper.append(args);
    *args = wrapper;
    env.extend(environment(&installed(runtime.clone())).unwrap());
    env.insert(
        "RUNTIME_ENV_RESULT".into(),
        observed.clone().into_os_string(),
    );
    env.insert("RUNTIME_TEST_ENGINE".into(), binary.into_os_string());
    let engine = engine_session::spawn(
        transport,
        engine_session::SessionConfig {
            max_restarts: 0,
            ..Default::default()
        },
    );
    let mut status = engine.status();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match status.borrow_and_update().clone() {
                engine_session::EngineStatus::Ready { version } => {
                    assert!(version.contains("fake"));
                    break;
                }
                engine_session::EngineStatus::Failed { reason, .. } => panic!("{reason}"),
                _ => {}
            }
            status.changed().await.unwrap();
        }
    })
    .await
    .expect("fake engine readiness safety deadline");
    let path = std::fs::read_to_string(observed).unwrap();
    assert_eq!(std::env::split_paths(&path).next().unwrap(), runtime);
    assert_eq!(std::env::var_os("PATH"), before);
    tokio::time::timeout(Duration::from_secs(5), engine.shutdown())
        .await
        .unwrap()
        .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn runtime_repair_installs_then_switches_the_pinned_engine_and_rediagnoses() {
    use std::os::unix::fs::PermissionsExt;
    let binary = fake_katago::binary_path().unwrap();
    let directory = directory();
    let data = directory.path().join("data");
    let observed = directory.path().join("runtime-child-path");
    let launched = directory.path().join("runtime-launched");
    let quoted =
        |path: &std::path::Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"));
    let program = format!(
        "#!/bin/sh\nprintf 'launched' >> {}\nif [ \"$1\" = version ]; then printf 'KataGo v1.18.2\\n'; exit 0; fi\nprintf '%s' \"$PATH\" > {}\nexec {} \"$@\"\n",
        quoted(&launched),
        quoted(&observed),
        quoted(&binary)
    );
    let (catalog, archives) = crate::assets::test_runtime_catalog(program.as_bytes());
    for (relative, bytes) in archives {
        let target = data.join(relative);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, bytes).unwrap();
    }
    let external = directory.path().join("selected-cuda-engine");
    std::fs::write(&external, program).unwrap();
    std::fs::set_permissions(&external, std::fs::Permissions::from_mode(0o755)).unwrap();
    let model = directory.path().join("model.bin");
    let config = directory.path().join("analysis.cfg");
    std::fs::write(&model, b"tiny model").unwrap();
    std::fs::write(&config, b"numSearchThreadsPerAnalysisThread = 1\n").unwrap();
    let profile = crate::EngineProfile {
        program: external.clone(),
        model,
        config,
        extra_args: vec![],
        canonical_model_name: None,
    };
    let config = crate::AppConfig {
        asset_catalog: Some(catalog),
        runtime_platform: true,
        cache_directory: Some(data.clone()),
        ..Default::default()
    };
    let (core, _) = crate::spawn_with_config(directory.path().join("settings"), config)
        .await
        .unwrap();
    core.configure_engine(Some(profile.clone())).await.unwrap();
    let report = core.diagnose_engine(Default::default()).await.unwrap();
    let runtime = report
        .checks
        .iter()
        .find(|check| check.kind == DiagnosticKind::RuntimeComponents)
        .unwrap();
    assert_eq!(runtime.status, DiagnosticStatus::Error);
    assert_eq!(runtime.repairs, [DiagnosticRepair::InstallRuntime]);
    assert!(
        !launched.exists(),
        "neither startup nor version may load an unverified runtime"
    );
    let inherited = std::env::var_os("PATH");
    let repaired = tokio::time::timeout(
        Duration::from_secs(5),
        core.repair_engine(DiagnosticRepair::InstallRuntime, Default::default()),
    )
    .await
    .expect("runtime repair safety deadline")
    .unwrap();
    let runtime = repaired
        .checks
        .iter()
        .find(|check| check.kind == DiagnosticKind::RuntimeComponents)
        .unwrap();
    assert_eq!(runtime.status, DiagnosticStatus::Ok);
    assert!(runtime.repairs.is_empty());
    let current = core.settings().await.unwrap().engine.unwrap();
    assert_ne!(current.program, external);
    assert!(current.program.starts_with(data.join("runtimes")));
    assert!(
        current
            .program
            .parent()
            .unwrap()
            .join("runtime.dll")
            .is_file()
    );
    let child_path = std::fs::read_to_string(observed).unwrap();
    assert_eq!(
        std::env::split_paths(&child_path).next().unwrap(),
        current.program.parent().unwrap()
    );
    assert_eq!(std::env::var_os("PATH"), inherited);
    assert_eq!(
        core.diagnose_engine(Default::default()).await.unwrap(),
        repaired
    );
    assert_eq!(
        std::fs::read(external).unwrap(),
        std::fs::read(current.program).unwrap()
    );
    tokio::time::timeout(Duration::from_secs(5), core.shutdown())
        .await
        .unwrap()
        .unwrap();
}
