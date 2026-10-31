#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use crate as core_types;
use crate::engine_setup::{
    catalog::{CatalogModel, ModelCatalog},
    detect_with_catalog,
};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

#[path = "../../tests/auto_setup_support/mod.rs"]
mod support;
use support::*;

#[tokio::test]
async fn old_program_never_trials_b11_and_catalog_name_is_persisted() {
    let dir = tempdir().unwrap();
    let old = dir.path().join("old");
    let good = dir.path().join("good");
    program(&old, "1.16.5-fake", false);
    let expected = program(&good, "1.17.0-fake", false);
    let bytes = b"tiny B11 stand-in";
    std::fs::write(old.join("default.bin.gz"), bytes).unwrap();
    let name = "kata1-tf3-b11c768-s12002M-d6304M.bin.gz";
    let catalog = ModelCatalog {
        default_model_id: "b11".into(),
        models: BTreeMap::from([(
            "b11".into(),
            CatalogModel {
                file_name: name.into(),
                size_bytes: bytes.len() as u64,
                sha256: format!("{:x}", Sha256::digest(bytes)),
                tier: "lightweight".into(),
                minimum_katago_version: "1.17.0".into(),
            },
        )]),
    };
    let roots = SearchRoots {
        windows_dirs: vec![old.clone(), good],
        ..Default::default()
    };
    let settings = dir.path().join("settings");
    let (core, mut output) = crate::spawn_with_config(&settings, config()).await.unwrap();
    let selected = within(core.auto_configure_using(
        move || detect_with_catalog(&roots, &catalog),
        template(dir.path()),
    ))
    .await
    .unwrap();
    assert_eq!(selected.program, expected);
    assert_eq!(selected.canonical_model_name.as_deref(), Some(name));
    assert_eq!(
        crate::Settings::load(&settings).await.unwrap().engine,
        Some(selected)
    );
    assert!(!old.join("trials").exists());
    assert_eq!(
        std::fs::read_to_string(old.join("versions")).unwrap(),
        "version\n"
    );
    analyzed(&mut output).await;
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn pair_budget_stops_after_six_failures_and_caches_versions() {
    let dir = tempdir().unwrap();
    program(dir.path(), "1.16.5-fake", true);
    for n in 0..8 {
        std::fs::write(dir.path().join(format!("model-{n}.bin.gz")), "fake model").unwrap();
    }
    let settings = dir.path().join("settings");
    let (core, _output) = crate::spawn_with_config(&settings, config()).await.unwrap();
    let error = within(core.auto_configure_engine(
        SearchRoots {
            windows_dirs: vec![dir.path().to_owned()],
            ..Default::default()
        },
        template(dir.path()),
    ))
    .await
    .unwrap_err();
    let AppError::AutoSetup(AutoSetupError::AllFailed { attempts, reason }) = error else {
        panic!("{error}")
    };
    assert_eq!(attempts.len(), MAX_ATTEMPTS);
    assert!(reason.contains("6-pair"));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("trials"))
            .unwrap()
            .lines()
            .count(),
        MAX_ATTEMPTS
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("versions")).unwrap(),
        "version\n"
    );
    assert!(!settings.join("settings.json").exists());
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn superseded_ticket_cannot_start_or_save_a_trial() {
    let dir = tempdir().unwrap();
    let (core, _output) = crate::spawn(dir.path()).await.unwrap();
    let (reply, response) = oneshot::channel();
    core.commands
        .send(Command::BeginAutoSetup(reply))
        .await
        .unwrap();
    let Start::Pending(ticket) = response.await.unwrap().unwrap() else {
        panic!("expected ticket")
    };
    within(core.configure_engine(None)).await.unwrap();
    let (reply, response) = oneshot::channel();
    core.commands
        .send(Command::TryAutoEngine(
            Trial {
                generation: ticket.generation,
                profile: EngineSetupProfile::default(),
                source: "unused".into(),
                canonical_model_name: None,
                deadline: Instant::now() + TOTAL_TIMEOUT,
            },
            reply,
        ))
        .await
        .unwrap();
    assert!(matches!(
        within(response).await.unwrap(),
        Err(AppError::EngineSetup(crate::EngineSetupError::Cancelled))
    ));
    assert!(core.settings().await.unwrap().engine.is_none());
    assert!(!dir.path().join("analysis.cfg").exists());
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn startup_and_total_deadlines_stop_hung_trials_without_saving() {
    for advance in [Duration::from_secs(21), Duration::from_secs(121)] {
        let dir = tempdir().unwrap();
        let marker = dir.path().join("hung");
        program_with_args(
            dir.path(),
            "1.16.5-fake",
            &[
                "--hang-after-queries".into(),
                "0".into(),
                "--fault-once-file".into(),
                marker.to_str().unwrap().into(),
            ],
        );
        std::fs::write(dir.path().join("model.bin.gz"), b"fake model").unwrap();
        let settings = dir.path().join("settings");
        let (core, _output) = crate::spawn_with_config(&settings, config()).await.unwrap();
        let pending_core = core.clone();
        let mut roots = SearchRoots {
            windows_dirs: vec![dir.path().to_owned()],
            ..Default::default()
        };
        let next = dir.path().join("next");
        if advance > TOTAL_TIMEOUT {
            program(&next, "1.16.5-fake", false);
            roots.windows_dirs.push(next.clone());
        }
        let source = template(dir.path());
        let task =
            tokio::spawn(async move { pending_core.auto_configure_engine(roots, source).await });
        within(async {
            while !marker.is_file() {
                tokio::task::yield_now().await;
            }
        })
        .await;
        tokio::time::pause();
        tokio::time::advance(advance).await;
        tokio::time::resume();
        let error = within(task).await.unwrap().unwrap_err();
        let AppError::AutoSetup(AutoSetupError::AllFailed { attempts, .. }) = error else {
            panic!("{error}")
        };
        assert_eq!(attempts.len(), 1);
        assert!(attempts[0].reason.contains("timed out"), "{attempts:?}");
        // Once the total budget expires, even a healthy next program must
        // remain unprobed; a per-trial deadline alone would try and save it.
        assert!(!next.join("versions").exists());
        assert!(!next.join("trials").exists());
        assert!(!settings.join("settings.json").exists());
        assert!(core.settings().await.unwrap().engine.is_none());
        within(core.shutdown()).await.unwrap();
    }
}
