use super::*;
use crate::assets::runtime_test_support::test_acceleration_catalog;
use std::{collections::BTreeMap, path::PathBuf, sync::OnceLock};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::oneshot,
};

pub(super) fn binaries() -> (PathBuf, PathBuf) {
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

pub(super) struct Fixture {
    pub directory: tempfile::TempDir,
    pub core: AppCore,
    pub events: crate::AppSubscription,
    pub original: Settings,
    pub archives: Vec<PathBuf>,
    pub gate: PathBuf,
    stop: oneshot::Sender<()>,
    server: tokio::task::JoinHandle<usize>,
}

impl Fixture {
    pub async fn new(startup: serde_json::Value, timeout: Duration) -> Self {
        Self::with_download(startup, timeout, false, None).await
    }

    pub async fn with_download(
        mut startup: serde_json::Value,
        timeout: Duration,
        corrupt: bool,
        response_gate: Option<oneshot::Receiver<()>>,
    ) -> Self {
        // Build before real sockets/processes or any deadline exist.
        let (gpu, cpu) = binaries();
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().join("data");
        let gate = directory.path().join("ready");
        if startup.get("gate").and_then(serde_json::Value::as_bool) == Some(true) {
            startup.as_object_mut().unwrap().remove("gate");
            startup["startup_gate_file"] = gate.to_string_lossy().into_owned().into();
        }
        let gpu_bytes = std::fs::read(gpu).unwrap();
        let cpu_bytes = std::fs::read(&cpu).unwrap();
        let (mut catalog, mut archives) =
            test_acceleration_catalog(&gpu_bytes, &cpu_bytes, &startup);
        if startup["backend"] == "opencl" {
            let spec = &mut catalog.specs[0];
            spec.id = "windows-opencl".into();
            spec.file_name = "windows-opencl.zip".into();
            spec.runtime_profile = Some("opencl".into());
            archives[0].0 = spec.relative_path();
            let mut profile = catalog.profiles.remove("cuda12.8-cudnn9").unwrap();
            profile.engine_asset = "windows-opencl".into();
            profile.assets[0] = "windows-opencl".into();
            catalog.profiles.insert("opencl".into(), profile);
        }
        let listener = within(TcpListener::bind("127.0.0.1:0")).await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut bodies = BTreeMap::new();
        for spec in catalog
            .specs
            .iter_mut()
            .filter(|spec| spec.runtime_profile.is_some() || spec.id.starts_with("runtime-"))
        {
            spec.url = format!("http://{address}/{}", spec.id);
            let body = archives
                .iter()
                .find(|(path, _)| *path == spec.relative_path())
                .unwrap()
                .1
                .clone();
            bodies.insert(format!("/{}", spec.id), body);
        }
        if corrupt {
            let body = bodies.get_mut("/windows-nvidia").unwrap();
            body[0] ^= 1;
        }
        let (stop, stopped) = oneshot::channel();
        let server = tokio::spawn(serve(listener, bodies, stopped, response_gate));
        let model = directory.path().join("model.bin.gz");
        let config_file = directory.path().join("analysis.cfg");
        std::fs::write(&model, "test model").unwrap();
        std::fs::write(
            &config_file,
            "numSearchThreads = 1\nreportAnalysisWinratesAs = BLACK\n",
        )
        .unwrap();
        let launch_log = directory.path().join("launches.jsonl");
        let profile = EngineProfile {
            program: cpu,
            model,
            config: config_file,
            extra_args: vec![
                "--startup-script-relative".into(),
                "../engine/startup.json".into(),
                "--benchmark-gate-file".into(),
                directory
                    .path()
                    .join("benchmark-ready")
                    .to_string_lossy()
                    .into_owned(),
                "--launch-log".into(),
                launch_log.to_string_lossy().into_owned(),
            ],
            canonical_model_name: Some("test-model".into()),
            ..Default::default()
        };
        let mut config = crate::AppConfig {
            runtime_platform: true,
            asset_catalog: Some(catalog),
            acceleration_timeout: Some(timeout),
            cache_directory: Some(data.clone()),
            ..Default::default()
        };
        config.analysis.live.max_visits = 1;
        config.analysis.live.report_interval = Duration::from_millis(10);
        config.session.max_restarts = 0;
        config.session.startup_timeout = Duration::from_secs(60);
        config.session.response_timeout = Duration::from_secs(60);
        config.session.write_timeout = Duration::from_secs(60);
        config.session.shutdown_timeout = Duration::from_millis(100);
        let (core, mut events) = within(crate::spawn_with_config(directory.path(), config))
            .await
            .unwrap();
        let settings = within(core.add_engine("Original CPU".into(), profile))
            .await
            .unwrap();
        let original = within(core.select_main_engine(settings.engines[0].id.clone()))
            .await
            .unwrap();
        analysis(&mut events, None).await;
        Self {
            directory,
            core,
            events,
            original,
            gate,
            archives: archives
                .into_iter()
                .map(|(relative, _)| data.join(relative))
                .collect(),
            stop,
            server,
        }
    }

    pub fn enabling(&self) -> tokio::task::JoinHandle<Result<Settings, AppError>> {
        let core = self.core.clone();
        tokio::spawn(async move { core.enable_acceleration(AccelerationBackend::Cuda).await })
    }

    pub async fn phase(&mut self, wanted: AccelerationPhase) -> crate::AccelerationProgress {
        within(async {
            loop {
                if let Some(AppEvent::AccelerationProgress(progress)) = self.events.recv().await {
                    assert_ne!(progress.phase, AccelerationPhase::Failed, "{progress:?}");
                    if progress.phase == wanted {
                        return progress;
                    }
                }
            }
        })
        .await
    }

    pub async fn assert_original_and_analyzing(&mut self) {
        assert_eq!(within(self.core.settings()).await.unwrap(), self.original);
        assert_eq!(
            within(Settings::load(self.directory.path())).await.unwrap(),
            self.original
        );
        let revision = within(self.core.play(go_core::Move::Pass(go_core::Color::Black)))
            .await
            .unwrap();
        analysis(&mut self.events, Some(revision)).await;
    }

    pub async fn close(self) -> usize {
        within(self.core.shutdown()).await.unwrap();
        let _ = self.stop.send(());
        within(self.server).await.unwrap()
    }
}

pub(super) async fn analysis(events: &mut crate::AppSubscription, revision: Option<u64>) {
    within(async {
        loop {
            if let Some(AppEvent::AnalysisUpdated(view)) = events.recv().await
                && revision.is_none_or(|revision| revision == view.revision)
                && !view.is_during_search
                && view.visits > 0
            {
                return;
            }
        }
    })
    .await
}

pub(super) async fn serve(
    listener: TcpListener,
    bodies: BTreeMap<String, Vec<u8>>,
    mut stop: oneshot::Receiver<()>,
    mut gate: Option<oneshot::Receiver<()>>,
) -> usize {
    let mut count = 0;
    loop {
        // The service lives across multiple bounded test operations. Its idle
        // lifetime is controlled by stop, not by a request safety deadline.
        let accepted = tokio::select! {
            _ = &mut stop => None,
            accepted = listener.accept() => Some(accepted),
        };
        let Some(accepted) = accepted else {
            return count;
        };
        let (mut socket, _) = accepted.unwrap();
        let mut request = Vec::new();
        let complete = within(async {
            while !request.ends_with(b"\r\n\r\n") {
                match socket.read_u8().await {
                    Ok(byte) => request.push(byte),
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::UnexpectedEof
                                | std::io::ErrorKind::ConnectionReset
                                | std::io::ErrorKind::ConnectionAborted
                        ) =>
                    {
                        return false;
                    }
                    Err(error) => panic!("download request header read failed: {error}"),
                }
            }
            true
        })
        .await;
        // Cancellation can close the connection before all headers arrive.
        // It must not consume the response gate or count as a served request.
        if !complete {
            continue;
        }
        if let Some(mut ready) = gate.take() {
            let released = tokio::select! {
                _ = &mut stop => false,
                _ = &mut ready => true,
            };
            if !released {
                return count;
            }
        }
        let request = String::from_utf8(request).unwrap();
        let target = request.split_whitespace().nth(1).unwrap();
        let body = bodies.get(target).unwrap();
        count += 1;
        let result = within(async {
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await?;
            socket.write_all(body).await
        })
        .await;
        // Cancellation can intentionally close the receiving socket.
        if result.is_err() {
            continue;
        }
    }
}
