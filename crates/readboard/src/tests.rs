use super::*;
use proptest::prelude::*;

// Reconstructed wire examples, pinned to upstream a292eea71caaf0cc2f087d3a7563e35db01684b4.
// https://github.com/qiyi71w/readboard/blob/a292eea71caaf0cc2f087d3a7563e35db01684b4/readboard/Core/Protocol/OutboundBoardSnapshotEmitter.cs
// No Windows recognition or live-session recording was available for this task.
const GOLDEN: &str = include_str!("../../../fixtures/readboard/reconstructed-session.txt");

#[test]
fn golden_session_preserves_dimensions_order_markers_and_context() {
    let mut decoder = Decoder::default();
    let events = GOLDEN
        .lines()
        .filter_map(|line| decoder.push_line(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(events[0], Event::Ready);
    assert_eq!(events[1], Event::Version(220430));
    assert_eq!(events[2], Event::Started(BoardSize::new(5, 3).unwrap()));
    assert_eq!(events[3], Event::Sync);
    let frames = events
        .iter()
        .filter_map(|event| match event {
            Event::Snapshot(frame) => Some(frame),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(frames.len(), 3);
    assert_eq!(
        frames[0].last_move(),
        Some((Color::Black, Point { x: 2, y: 1 }))
    );
    assert_eq!(frames[0].fox_move_number, Some(1));
    assert_eq!(
        frames[0].context.room_token.as_deref(),
        Some("opaque-test-room")
    );
    assert_eq!(
        frames[1].last_move(),
        Some((Color::White, Point { x: 3, y: 1 }))
    );
    assert_eq!(frames[1].stones()[7], Some(Color::Black));
    assert_eq!(frames[1].context.room_token, None);
    assert!(frames[2].force_rebuild);
    assert_eq!(frames[2].context.record_current_move, Some(42));
    assert_eq!(frames[2].context.record_total_move, Some(88));
    assert_eq!(frames[2].context.record_at_end, Some(false));
    assert_eq!(frames[2].context.live_title_move, Some(88));
    assert_eq!(events[events.len() - 2..], [Event::Stopped, Event::Ended]);
}

#[test]
fn malformed_frame_is_discarded_and_next_frame_recovers() {
    for bad_row in ["re=0,5", "re=00,0", "re=0,0,", "re=0", "re=00"] {
        let mut decoder = Decoder::default();
        decoder.push_line("start 2 2").unwrap();
        assert!(decoder.push_line(bad_row).is_err());
        assert!(decoder.push_line("re=0,0").is_err());
        assert_eq!(decoder.push_line("end"), Err(ProtocolError::InvalidFrame));
        assert_eq!(decoder.push_line("re=0,3"), Ok(None));
        assert_eq!(decoder.push_line("re=0,0"), Ok(None));
        assert!(matches!(
            decoder.push_line("end"),
            Ok(Some(Event::Snapshot(_)))
        ));
    }
}

#[test]
fn rejects_incomplete_excess_rows_duplicate_markers_and_invalid_sizes() {
    for lines in [
        vec!["re=0,0"],
        vec!["re=0,0", "re=0,0", "re=0,0"],
        vec!["re=3,0", "re=0,4"],
    ] {
        let mut decoder = Decoder::default();
        decoder.push_line("start 2 2").unwrap();
        for line in lines {
            let _ = decoder.push_line(line);
        }
        assert_eq!(decoder.push_line("end"), Err(ProtocolError::InvalidFrame));
    }
    for line in [
        "start 0 19",
        "start 26 19",
        "start 19",
        "start -2 2",
        "start 999 19",
    ] {
        assert_eq!(
            Decoder::default().push_line(line),
            Err(ProtocolError::InvalidSize)
        );
    }
}

#[test]
fn clear_and_restart_discard_metadata_and_partial_rows() {
    let mut decoder = Decoder::default();
    for line in [
        "start 2 2",
        "foxMoveNumber 3",
        "forceRebuild",
        "re=3,0",
        "clear",
        "re=0,0",
        "re=0,0",
    ] {
        decoder.push_line(line).unwrap();
    }
    let Event::Snapshot(frame) = decoder.push_line("end").unwrap().unwrap() else {
        panic!("missing frame")
    };
    assert_eq!(frame.fox_move_number, None);
    assert!(!frame.force_rebuild);
    decoder.push_line("re=0,0").unwrap();
    decoder.push_line("start 3 2 424242").unwrap();
    decoder.push_line("re=0,0,0").unwrap();
    assert_eq!(decoder.push_line("end"), Err(ProtocolError::InvalidFrame));
}

#[test]
fn accepts_crlf_and_ignores_unknown_optional_capabilities() {
    let mut decoder = Decoder::default();
    assert_eq!(decoder.push_line("ready\r\n"), Ok(Some(Event::Ready)));
    assert_eq!(
        decoder.push_line("readboardUpdatePackageV2Supported"),
        Ok(None)
    );
    assert_eq!(decoder.push_line("bothSync"), Ok(None));
    assert_eq!(decoder.push_line("pass"), Ok(Some(Event::ExchangeTurn)));
    assert_eq!(decoder.push_line("clearBoard"), Ok(Some(Event::ClearBoard)));
    assert_eq!(
        decoder.push_line("ready\nsync"),
        Err(ProtocolError::InvalidLine)
    );
    assert_eq!(
        decoder.push_line(&"x".repeat(MAX_LINE_BYTES + 1)),
        Err(ProtocolError::InvalidLine)
    );
}

#[test]
fn launch_and_host_commands_preserve_legacy_boundaries() {
    assert_eq!(
        launch_arguments(Language::Chinese),
        ["yzy", " ", " ", " ", "0", "cn", "-1"]
    );
    assert_eq!(launch_arguments(Language::English)[5], "en");
    assert_eq!(launch_arguments(Language::Japanese)[5], "jp");
    assert_eq!(launch_arguments(Language::Korean)[5], "kr");
    assert_eq!(Command::Version.encode(), "version\n");
    assert_eq!(Command::Quit.encode(), "quit\n");
    assert_eq!(
        Command::AnalysisState { running: true }.encode(),
        "analysisState running\n"
    );
    assert_eq!(
        Command::AnalysisState { running: false }.encode(),
        "analysisState paused\n"
    );
}

proptest! {
    #[test]
    fn snapshot_round_trip_preserves_all_cells_and_metadata(
        width in 2u8..=25, height in 2u8..=25,
        codes in prop::collection::vec(0u8..=2, 625),
        marker in 0usize..625, marked in any::<bool>(),
        fox_move_number in prop::option::of(any::<u32>()), force_rebuild in any::<bool>(),
    ) {
        let area = usize::from(width) * usize::from(height);
        let mut cells = codes[..area].iter().map(|n| match n { 0 => Cell::Empty, 1 => Cell::Black, _ => Cell::White }).collect::<Vec<_>>();
        if marked { cells[marker%area] = Cell::LastWhite; }
        let snapshot = Snapshot {
            size: BoardSize::new(width, height).unwrap(), cells, fox_move_number, force_rebuild,
            context: RemoteContext { platform: Some("fox".to_owned()), record_at_end: Some(false), ..RemoteContext::default() },
        };
        let encoded = encode_snapshot(&snapshot).unwrap();
        let mut decoder = Decoder::default();
        let mut result = None;
        for line in encoded.lines() {
            if let Some(Event::Snapshot(frame)) = decoder.push_line(line).unwrap() { result = Some(frame); }
        }
        prop_assert_eq!(result, Some(snapshot));
    }

    #[test]
    fn arbitrary_utf8_never_panics(input in ".{0,512}") {
        let mut decoder = Decoder::default();
        let _ = decoder.push_line(&input);
        let _ = decoder.push_line("end");
    }
}

#[test]
fn per_frame_metadata_does_not_leak_after_success_or_failure() {
    let mut decoder = Decoder::default();
    for line in [
        "start 2 2",
        "syncPlatform fox",
        "roomToken room",
        "foxMoveNumber 10",
        "liveTitleMove 10",
        "recordCurrentMove 10",
        "recordTotalMove 30",
        "recordAtEnd 0",
        "recordTitleFingerprint title",
        "lastMoveSource redBlueMarker",
        "forceRebuild",
        "re=0,3",
        "re=0,0",
    ] {
        decoder.push_line(line).unwrap();
    }
    assert!(matches!(
        decoder.push_line("end"),
        Ok(Some(Event::Snapshot(_)))
    ));
    for line in ["re=0,3", "re=0,0"] {
        decoder.push_line(line).unwrap();
    }
    let Some(Event::Snapshot(frame)) = decoder.push_line("end").unwrap() else {
        panic!("missing frame")
    };
    assert_eq!(frame.context, RemoteContext::default());
    assert_eq!(frame.fox_move_number, None);
    assert!(!frame.force_rebuild);
    decoder.push_line("foxMoveNumber 10").unwrap();
    assert!(decoder.push_line("re=0,5").is_err());
    assert!(decoder.push_line("end").is_err());
    for line in ["re=0,3", "re=0,0"] {
        decoder.push_line(line).unwrap();
    }
    let Some(Event::Snapshot(frame)) = decoder.push_line("end").unwrap() else {
        panic!("missing frame")
    };
    assert_eq!(frame.fox_move_number, None);
}

#[test]
fn helper_analysis_controls_remain_separate_from_frames() {
    let mut decoder = Decoder::default();
    assert_eq!(
        decoder.push_line("noponder"),
        Ok(Some(Event::PauseAnalysis))
    );
    assert_eq!(
        decoder.push_line("resumeponder"),
        Ok(Some(Event::ResumeAnalysis))
    );
}
