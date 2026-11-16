use super::*;
use crate::{AppConfig, AppCore, EngineProfile, EngineSetupProfile, assets::runtime_test_support};
use std::{path::PathBuf, sync::OnceLock};
use tokio::sync::oneshot;

fn binaries() -> (PathBuf, PathBuf) {
    static BINARIES: OnceLock<(PathBuf, PathBuf)> = OnceLock::new();
    BINARIES
        .get_or_init(|| {
            (
                fake_katago::binary_path().unwrap(),
                fake_katago::cpu_binary_path().unwrap(),
            )
        })
        .clone()
}

struct Fixture {
    directory: tempfile::TempDir,
    core: AppCore,
    data: PathBuf,
    log: PathBuf,
    profile: EngineProfile,
    managed: PathBuf,
}

impl Fixture {
    async fn new() -> Self {
        Self::with_cpu_pin(true).await
    }

    async fn with_cpu_pin(pinned: bool) -> Self {
        let (gpu, cpu) = binaries();
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().join("data");
        let (mut catalog, archives) = runtime_test_support::test_acceleration_catalog(
            &std::fs::read(&gpu).unwrap(),
            &std::fs::read(&cpu).unwrap(),
            &serde_json::json!({}),
        );
        if !pinned {
            catalog
                .specs
                .iter_mut()
                .find(|spec| spec.id == "windows-cpu")
                .unwrap()
                .executable_sha256 = None;
        }
        for (relative, contents) in archives {
            let path = data.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        let (assets, mut progress) =
            crate::assets::spawn_with_catalog(data.clone(), catalog.clone());
        within(assets.install_runtime("cuda12.8-cudnn9".into()))
            .await
            .unwrap();
        within(async {
            loop {
                let status = progress.borrow_and_update().clone();
                assert_ne!(status.phase, crate::DownloadPhase::Failed, "{status:?}");
                if status.phase == crate::DownloadPhase::Completed {
                    break;
                }
                progress.changed().await.unwrap();
            }
        })
        .await;
        let managed = within(assets.runtime_installation("cuda12.8-cudnn9".into()))
            .await
            .unwrap()
            .program
            .unwrap();
        within(assets.shutdown()).await.unwrap();
        let model = directory.path().join("model.bin.gz");
        let config = directory.path().join("analysis.cfg");
        std::fs::write(&model, "fake model").unwrap();
        std::fs::write(&config, "numSearchThreads=1\n").unwrap();
        let log = directory.path().join("launches.jsonl");
        let profile = EngineProfile {
            program: cpu,
            model,
            config,
            extra_args: vec![
                "--launch-log".into(),
                log.to_string_lossy().into_owned(),
                "--benchmark-gate-file".into(),
                directory
                    .path()
                    .join("benchmark-ready")
                    .to_string_lossy()
                    .into_owned(),
            ],
            ..Default::default()
        };
        let config = AppConfig {
            cache_directory: Some(data.clone()),
            asset_catalog: Some(catalog),
            runtime_platform: true,
            ..Default::default()
        };
        let (core, _) = within(crate::spawn_with_config(
            directory.path().join("settings"),
            config,
        ))
        .await
        .unwrap();
        Self {
            directory,
            core,
            data,
            log,
            profile,
            managed,
        }
    }

    async fn launches(&self, count: usize) -> Vec<Vec<String>> {
        within(async {
            loop {
                if let Ok(contents) = std::fs::read_to_string(&self.log) {
                    let lines: Vec<Vec<String>> = contents
                        .lines()
                        .filter_map(|line| serde_json::from_str(line).ok())
                        .collect();
                    if lines.len() >= count {
                        return lines;
                    }
                }
                tokio::task::yield_now().await;
            }
        })
        .await
    }

    fn assert_defaults(&self, args: &[String]) {
        let args: Vec<_> = args.iter().map(OsString::from).collect();
        let entries: Vec<_> = overrides(&args).split(',').collect();
        assert!(entries.contains(&"logToStderr=true"), "{entries:?}");
        assert!(
            entries.contains(
                &format!("homeDataDir={}", self.data.join("katago-home").display()).as_str()
            ),
            "{entries:?}"
        );
        assert!(self.data.join("katago-home").is_dir());
    }

    async fn close(self) {
        within(self.core.shutdown()).await.unwrap();
        within(tokio::task::spawn_blocking(move || self.directory.close()))
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn bundled_and_managed_engines_share_home_in_trial_main_startup_and_benchmark() {
    for managed in [false, true] {
        let mut fixture = Fixture::new().await;
        if managed {
            fixture.profile.program = fixture.managed.clone();
        }
        within(fixture.core.try_configure_engine(
            EngineSetupProfile {
                program: fixture.profile.program.clone(),
                model: fixture.profile.model.clone(),
                config: Some(fixture.profile.config.clone()),
                extra_args: fixture.profile.extra_args.clone(),
            },
            fixture.profile.config.clone(),
        ))
        .await
        .unwrap();
        fixture.assert_defaults(&fixture.launches(1).await[0]);
        // The legacy configure entry runs the ordinary main-session path,
        // distinct from the trial used by the first configuration above.
        within(fixture.core.close_engine()).await.unwrap();
        within(fixture.core.configure_engine(Some(fixture.profile.clone())))
            .await
            .unwrap();
        fixture.assert_defaults(&fixture.launches(2).await[1]);
        within(fixture.core.start_benchmark()).await.unwrap();
        let launches = fixture.launches(3).await;
        assert_eq!(launches[2][0], "benchmark");
        fixture.assert_defaults(&launches[2]);
        within(fixture.core.cancel_benchmark()).await.unwrap();
        fixture.assert_defaults(&fixture.launches(4).await[3]);
        assert_eq!(
            within(fixture.core.settings())
                .await
                .unwrap()
                .engine
                .unwrap()
                .extra_args,
            fixture.profile.extra_args
        );
        fixture.close().await;
    }
}

#[tokio::test]
async fn auto_setup_candidate_uses_the_same_home_defaults() {
    let fixture = Fixture::new().await;
    let (reply, response) = oneshot::channel();
    within(
        fixture
            .core
            .commands
            .send(crate::actor::Command::BeginAutoSetup(reply)),
    )
    .await
    .unwrap();
    let crate::auto_setup::Start::Pending(ticket) = within(response).await.unwrap().unwrap() else {
        panic!("expected fresh auto setup");
    };
    let (reply, response) = oneshot::channel();
    within(
        fixture
            .core
            .commands
            .send(crate::actor::Command::TryAutoEngine(
                crate::auto_setup::Trial {
                    generation: ticket.generation,
                    profile: EngineSetupProfile {
                        program: fixture.profile.program.clone(),
                        model: fixture.profile.model.clone(),
                        config: Some(fixture.profile.config.clone()),
                        extra_args: fixture.profile.extra_args.clone(),
                    },
                    source: fixture.profile.config.clone(),
                    canonical_model_name: None,
                    deadline: tokio::time::Instant::now() + Duration::from_secs(4),
                },
                reply,
            )),
    )
    .await
    .unwrap();
    within(response).await.unwrap().unwrap();
    fixture.assert_defaults(&fixture.launches(1).await[0]);
    fixture.close().await;
}

#[tokio::test]
async fn external_engine_gets_no_home_or_stderr_defaults() {
    let fixture = Fixture::with_cpu_pin(false).await;
    within(fixture.core.configure_engine(Some(fixture.profile.clone())))
        .await
        .unwrap();
    let launches = fixture.launches(1).await;
    let args: Vec<_> = launches[0].iter().map(OsString::from).collect();
    let overrides = overrides(&args);
    assert!(!overrides.contains("homeDataDir="));
    assert!(!overrides.contains("logToStderr="));
    assert!(!fixture.data.join("katago-home").exists());
    within(fixture.core.start_benchmark()).await.unwrap();
    let launches = fixture.launches(2).await;
    assert_eq!(launches[1][0], "benchmark");
    assert!(
        !launches[1]
            .iter()
            .any(|arg| arg.contains("homeDataDir=") || arg.contains("logToStderr="))
    );
    fixture.close().await;
}

#[tokio::test]
async fn managed_benchmark_and_trial_preserve_explicit_home_and_stderr_choices() {
    let mut fixture = Fixture::new().await;
    fixture.profile.program = fixture.managed.clone();
    fixture
        .profile
        .extra_args
        .extend(["-override-config=homeDataDir=,logToStderr=false".into()]);
    let settings = within(
        fixture
            .core
            .add_engine("managed".into(), fixture.profile.clone()),
    )
    .await
    .unwrap();
    within(
        fixture
            .core
            .select_main_engine(settings.engines[0].id.clone()),
    )
    .await
    .unwrap();
    within(fixture.core.start_benchmark()).await.unwrap();
    for launch in fixture.launches(2).await {
        let args: Vec<_> = launch.iter().map(OsString::from).collect();
        let entries: Vec<_> = overrides(&args).split(',').collect();
        assert!(entries.contains(&"homeDataDir="));
        assert!(entries.contains(&"logToStderr=false"));
        assert!(!entries.contains(&"logToStderr=true"));
    }
    assert!(!fixture.data.join("katago-home").exists());
    fixture.close().await;
}
