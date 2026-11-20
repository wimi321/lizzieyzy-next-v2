use std::{path::Path, time::Instant};

use app_core::{
    AnalysisSource, AppCore, AppEvent, AppSubscription, DocumentView, EngineProfile, EngineStatus,
    Settings,
};
use go_core::{Color, Move, Point};

use crate::{BenchmarkError, Result, Sample, memory, support::Scenario};

pub async fn measure(scenario: Scenario, fixture: &Path, engine: &Path) -> Result<Sample> {
    let directory = tempfile::tempdir()?;
    let settings_directory = directory.path().join("settings");
    let needs_engine = matches!(
        scenario,
        Scenario::EngineReady | Scenario::FirstAnalysis | Scenario::AnalysisAfterMove
    );
    let profile = if needs_engine {
        let model = directory.path().join("fake-model.bin");
        let config = directory.path().join("analysis.cfg");
        std::fs::write(&model, b"fake model, never loaded")?;
        std::fs::write(&config, b"reportAnalysisWinratesAs = BLACK\n")?;
        Some(EngineProfile {
            program: engine.to_owned(),
            model,
            config,
            extra_args: vec!["--instant-response".into()],
            ..Default::default()
        })
    } else {
        None
    };
    Settings {
        engine: profile,
        pondering: false,
        auto_quick_analyze_on_load: false,
        autosave: false,
        ..Settings::default()
    }
    .save(&settings_directory)
    .await?;

    // EngineReady includes loading saved settings, creating AppCore and the
    // actual engine-session query_version handshake, but no setup/build work.
    let started = Instant::now();
    let (app, mut events) = app_core::spawn(&settings_directory).await?;
    let initial = document(&mut events, 0).await?;
    let elapsed_ms = match scenario {
        Scenario::EngineReady => {
            ready(&mut events).await?;
            started.elapsed().as_secs_f64() * 1000.0
        }
        Scenario::OpenReviewedGame | Scenario::OpenLargeTree => {
            let (view, elapsed) = timed_document(app.open_path(fixture), &mut events, 1).await?;
            validate_open(&view, scenario)?;
            elapsed
        }
        Scenario::JumpToLastMove => {
            let revision = app.open_path(fixture).await?;
            let opened = document(&mut events, revision).await?;
            validate_open(&opened, Scenario::OpenReviewedGame)?;
            let last = opened
                .tree
                .main_line
                .last()
                .copied()
                .ok_or(BenchmarkError::Contract(
                    "reviewed main line has no last move",
                ))?;
            let (view, elapsed) =
                timed_document(app.go_to(last), &mut events, revision + 1).await?;
            if view.tree.current != last
                || view
                    .board
                    .as_ref()
                    .is_none_or(|board| board.move_number != 400)
            {
                return Err(BenchmarkError::Contract("navigation did not reach move 400").into());
            }
            elapsed
        }
        Scenario::FirstAnalysis | Scenario::AnalysisAfterMove => {
            ready(&mut events).await?;
            let started = Instant::now();
            let (command, observed) = tokio::join!(app.set_pondering(true), async {
                analysis(&mut events, 0, initial.tree.current).await?;
                Ok::<_, Box<dyn std::error::Error>>(started.elapsed().as_secs_f64() * 1000.0)
            });
            command?;
            let first_ms = observed?;
            if scenario == Scenario::FirstAnalysis {
                first_ms
            } else {
                after_move(&app, &mut events).await?
            }
        }
    };
    // RUSAGE_SELF belongs only to this one fresh worker on macOS/Linux.
    // Capture before shutdown/autosave/cleanup, never include the fake child.
    let peak_rss_bytes = if scenario.has_memory() {
        match memory::peak_rss_bytes() {
            Ok(bytes) => Some(bytes),
            Err(error)
                if cfg!(target_os = "windows")
                    && error.kind() == std::io::ErrorKind::Unsupported =>
            {
                None
            }
            Err(error) => return Err(error.into()),
        }
    } else {
        None
    };
    app.shutdown().await?;
    Ok(Sample {
        scenario,
        elapsed_ms,
        peak_rss_bytes,
    })
}

fn validate_open(view: &DocumentView, scenario: Scenario) -> Result<()> {
    if view.position_error.is_some() || view.board.is_none() || !view.warnings.is_empty() {
        return Err(BenchmarkError::Contract("fixture did not open as a valid document").into());
    }
    if scenario == Scenario::OpenReviewedGame
        && (view.tree.main_line.len() != 401
            || view
                .graph
                .points
                .iter()
                .filter(|point| point.summary.is_some())
                .count()
                != 400)
    {
        return Err(
            BenchmarkError::Contract("reviewed game did not load all 400 LZ summaries").into(),
        );
    }
    Ok(())
}

async fn after_move(app: &AppCore, events: &mut AppSubscription) -> Result<f64> {
    let started = Instant::now();
    let (command, observed) = tokio::join!(
        app.play(Move::Play(Color::Black, Point { x: 3, y: 3 })),
        async {
            let view = document(events, 1).await?;
            analysis(events, 1, view.tree.current).await?;
            Ok::<_, Box<dyn std::error::Error>>((view, started.elapsed().as_secs_f64() * 1000.0))
        }
    );
    if command? != 1 {
        return Err(
            BenchmarkError::Contract("move revision did not match the observed event").into(),
        );
    }
    let (view, elapsed) = observed?;
    if view
        .board
        .as_ref()
        .is_none_or(|board| board.move_number != 1)
    {
        return Err(BenchmarkError::Contract("analysis-after-move did not play a stone").into());
    }
    Ok(elapsed)
}

async fn timed_document(
    command: impl std::future::Future<Output = std::result::Result<u64, app_core::AppError>>,
    events: &mut AppSubscription,
    expected_revision: u64,
) -> Result<(DocumentView, f64)> {
    let started = Instant::now();
    // Poll both sides: stop the clock at event receipt, even if the API's
    // acknowledgement includes additional work after publishing that event.
    let (command, observed) = tokio::join!(command, async {
        let view = document(events, expected_revision).await?;
        Ok::<_, Box<dyn std::error::Error>>((view, started.elapsed().as_secs_f64() * 1000.0))
    });
    if command? != expected_revision {
        return Err(
            BenchmarkError::Contract("command revision did not match the observed event").into(),
        );
    }
    observed
}

async fn event(events: &mut AppSubscription) -> Result<AppEvent> {
    let event = events
        .recv()
        .await
        .ok_or(BenchmarkError::Contract("event stream closed"))?;
    match &event {
        AppEvent::EngineStatusChanged(EngineStatus::Failed { .. })
        | AppEvent::OpenFailed { .. } => {
            Err(std::io::Error::other(format!("AppCore failure: {event:?}")).into())
        }
        _ => Ok(event),
    }
}

async fn document(events: &mut AppSubscription, expected_revision: u64) -> Result<DocumentView> {
    loop {
        if let AppEvent::DocumentChanged { revision, view } = event(events).await?
            && revision == expected_revision
        {
            return Ok(view);
        }
    }
}

async fn ready(events: &mut AppSubscription) -> Result<()> {
    loop {
        if let AppEvent::EngineStatusChanged(EngineStatus::Ready { .. }) = event(events).await? {
            return Ok(());
        }
    }
}

async fn analysis(events: &mut AppSubscription, revision: u64, node: u32) -> Result<()> {
    loop {
        if let AppEvent::AnalysisUpdated(view) = event(events).await?
            && view.revision == revision
            && view.node_id == node
            && view.source == AnalysisSource::Live
            && view.visits > 0
        {
            return Ok(());
        }
    }
}
