use super::*;
use crate::{Settings, SettingsError};
use std::time::Duration;
use tokio::time::Instant;

#[test]
fn defaults_follow_card_and_missing_settings_fields() {
    let setup: GameSetup = serde_json::from_str("{}").unwrap();
    assert_eq!(setup, GameSetup::default());
    assert_eq!(setup.black, Player::Human);
    assert_eq!(
        setup.white,
        Player::Ai(AiPlayer::KataGo(KataGoPlay::default()))
    );
    assert!(setup.human_clock.is_none());
    assert!(setup.ponder && setup.auto_save);
    assert_eq!(
        HumanClock::default(),
        HumanClock {
            main_seconds: 1200,
            byoyomi_seconds: 30,
            periods: 3
        }
    );
    assert_eq!(KataGoPlay::default().seconds, Some(2.0));
    assert_eq!(
        KataGoPlay::default().resign,
        Some(ResignRule {
            after_moves: 150,
            consecutive: 2,
            below_winrate: 0.05
        })
    );
    assert!(KataGoPlay::default().random_opening.is_none());
    assert_eq!(
        RandomOpening::default(),
        RandomOpening {
            within_moves: 20,
            max_winrate_drop: 0.005,
            min_visit_share: 0.05
        }
    );
    let settings: Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(settings.game_setup, setup);
    assert!(settings.games_directory.is_none());
}

#[test]
fn validation_rejects_unbounded_ai_and_empty_or_inconsistent_clock() {
    let mut play = KataGoPlay {
        seconds: None,
        visits: None,
        ..Default::default()
    };
    assert!(play.validate().is_err());
    play.pure_network = true;
    assert!(play.validate().is_ok());
    play.seconds = Some(f64::NAN);
    assert!(play.validate().is_err());
    play.seconds = None;
    play.visits = Some(0);
    assert!(play.validate().is_err());
    play.visits = Some(100);
    assert!(play.validate().is_ok());
    play.resign.as_mut().unwrap().consecutive = 0;
    assert!(play.validate().is_err());
    play.resign = None;
    play.random_opening = Some(RandomOpening {
        min_visit_share: 1.1,
        ..Default::default()
    });
    assert!(play.validate().is_err());
    assert!(
        HumanClock {
            main_seconds: 0,
            byoyomi_seconds: 0,
            periods: 0
        }
        .validate()
        .is_err()
    );
    assert!(
        HumanClock {
            main_seconds: 10,
            byoyomi_seconds: 0,
            periods: 0
        }
        .validate()
        .is_ok()
    );
    assert!(
        HumanClock {
            main_seconds: 0,
            byoyomi_seconds: 10,
            periods: 1
        }
        .validate()
        .is_ok()
    );
    assert!(
        HumanClock {
            main_seconds: 10,
            byoyomi_seconds: 10,
            periods: 0
        }
        .validate()
        .is_err()
    );
    assert!(
        GameSetup {
            black: Player::Ai(AiPlayer::default()),
            ..Default::default()
        }
        .validate()
        .is_err()
    );
}

#[tokio::test]
async fn game_settings_round_trip_and_invalid_save_preserves_previous_file() {
    let directory = tempfile::tempdir().unwrap();
    let setup = GameSetup {
        human_clock: Some(HumanClock::default()),
        ponder: false,
        start: GameStart::CurrentPosition,
        ..Default::default()
    };
    let settings = Settings {
        game_setup: setup.clone(),
        games_directory: Some(directory.path().join("MyGames")),
        ..Default::default()
    };
    settings.save(directory.path()).await.unwrap();
    let loaded = Settings::load(directory.path()).await.unwrap();
    assert_eq!(loaded.game_setup, setup);
    assert_eq!(loaded.games_directory, settings.games_directory);
    let before = tokio::fs::read(directory.path().join("settings.json"))
        .await
        .unwrap();
    let mut invalid = settings;
    invalid.game_setup.human_clock = Some(HumanClock {
        main_seconds: 0,
        byoyomi_seconds: 0,
        periods: 0,
    });
    assert!(matches!(
        invalid.save(directory.path()).await,
        Err(SettingsError::Game(_))
    ));
    assert_eq!(
        tokio::fs::read(directory.path().join("settings.json"))
            .await
            .unwrap(),
        before
    );
}

#[tokio::test(start_paused = true)]
async fn clock_consumes_main_time_then_periods_and_times_out_at_exact_boundary() {
    let mut clock = ClockState::new(HumanClock {
        main_seconds: 2,
        byoyomi_seconds: 3,
        periods: 2,
    });
    clock.start(Instant::now());
    tokio::time::advance(Duration::from_secs(2)).await;
    assert!(!clock.update(Instant::now()));
    assert_eq!(
        clock.view(Instant::now()),
        HumanClockView {
            main_seconds: 0.0,
            byoyomi_seconds: 3.0,
            periods: 2
        }
    );
    tokio::time::advance(Duration::from_secs(3)).await;
    assert!(!clock.update(Instant::now()));
    assert_eq!(clock.view(Instant::now()).periods, 1);
    tokio::time::advance(Duration::from_secs(3)).await;
    assert!(clock.update(Instant::now()));
    assert_eq!(clock.view(Instant::now()).periods, 0);
}

#[tokio::test(start_paused = true)]
async fn clock_pause_preserves_partial_period_and_next_move_resets_only_period() {
    let mut clock = ClockState::new(HumanClock {
        main_seconds: 1,
        byoyomi_seconds: 3,
        periods: 2,
    });
    clock.start(Instant::now());
    tokio::time::advance(Duration::from_secs(3)).await;
    assert!(!clock.pause(Instant::now()));
    assert_eq!(clock.view(Instant::now()).byoyomi_seconds, 1.0);
    assert!(clock.deadline().is_none());
    tokio::time::advance(Duration::from_secs(100)).await;
    assert_eq!(clock.view(Instant::now()).byoyomi_seconds, 1.0);
    clock.start(Instant::now());
    assert_eq!(
        clock.deadline(),
        Some(Instant::now() + Duration::from_secs(1))
    );
    assert!(!clock.finish_turn(Instant::now()));
    assert_eq!(
        clock.view(Instant::now()),
        HumanClockView {
            main_seconds: 0.0,
            byoyomi_seconds: 3.0,
            periods: 2
        }
    );
}

#[tokio::test(start_paused = true)]
async fn clock_handles_main_only_and_large_time_jumps() {
    let mut clock = ClockState::new(HumanClock {
        main_seconds: 10,
        byoyomi_seconds: 0,
        periods: 0,
    });
    clock.start(Instant::now());
    tokio::time::advance(Duration::from_secs(9)).await;
    assert!(!clock.update(Instant::now()));
    tokio::time::advance(Duration::from_secs(1)).await;
    assert!(clock.finish_turn(Instant::now()));
    let mut clock = ClockState::new(HumanClock::default());
    clock.start(Instant::now());
    tokio::time::advance(Duration::from_secs(1_000_000)).await;
    assert!(clock.update(Instant::now()));
}

#[test]
fn scoring_and_filename_have_stable_safe_output() {
    assert_eq!(score_result(2.1, 7.5), Some("B+2.5".into()));
    assert_eq!(score_result(-2.1, 7.5), Some("W+2.5".into()));
    assert_eq!(score_result(0.1, 7.0), Some("0".into()));
    assert_eq!(score_result(2.4, 7.6), Some("B+2.4".into()));
    assert_eq!(score_result(0.0, 0.0000001), Some("W+0.0000001".into()));
    assert_eq!(score_result(10.0, 7.0), Some("B+10".into()));
    assert_eq!(score_result(f64::NAN, 7.5), None);
    assert_eq!(
        game_file_name("20261113123456", "a/b:c*?", "白棋. "),
        Some("2026-11-13-123456_a_b_c___vs_白棋.sgf".into())
    );
    assert_eq!(game_file_name("invalid", "Black", "White"), None);
}

#[test]
fn gtp_clock_tracks_canadian_stones_main_time_and_fischer_increment() {
    let mut clock = GtpClock::new(GtpTime::Byoyomi {
        main: 5,
        period: 10,
        stones: 2,
    });
    assert_eq!(
        clock.time_left(go_core::Color::Black).as_deref(),
        Some("time_left black 5 0")
    );
    clock.spend(3.0, true);
    assert_eq!(
        clock.time_left(go_core::Color::Black).as_deref(),
        Some("time_left black 2 0")
    );
    clock.spend(4.0, true);
    assert_eq!(
        clock.time_left(go_core::Color::Black).as_deref(),
        Some("time_left black 8 1")
    );
    clock.spend(3.0, false);
    assert_eq!(
        clock.time_left(go_core::Color::Black).as_deref(),
        Some("time_left black 5 1")
    );
    clock.spend(1.0, true);
    assert_eq!(
        clock.time_left(go_core::Color::Black).as_deref(),
        Some("time_left black 10 2")
    );
    let mut clock = GtpClock::new(GtpTime::Fischer {
        main: 5,
        increment: 2,
    });
    clock.spend(3.0, true);
    assert_eq!(
        clock.time_left(go_core::Color::White).as_deref(),
        Some("time_left white 4 0")
    );
    clock.spend(1.0, false);
    assert_eq!(
        clock.time_left(go_core::Color::White).as_deref(),
        Some("time_left white 3 0")
    );
}

#[test]
fn gtp_time_validation_rejects_zero_clocks_and_multiline_custom_injection() {
    for time in [
        GtpTime::PerMove(0),
        GtpTime::Absolute { main: 0 },
        GtpTime::Fischer {
            main: 0,
            increment: 0,
        },
        GtpTime::Byoyomi {
            main: 10,
            period: 5,
            stones: 0,
        },
        GtpTime::Custom(vec!["time_settings 0 1 1\nquit".into()]),
    ] {
        assert!(time.validate().is_err());
    }
    assert!(GtpTime::EngineDefault.validate().is_ok());
    assert!(
        GtpTime::Custom(vec!["echo exact text".into()])
            .validate()
            .is_ok()
    );
}

#[test]
fn gtp_clock_emits_standard_integer_seconds_and_clamps_fischer_growth() {
    let mut clock = GtpClock::new(GtpTime::Byoyomi {
        main: 3,
        period: 5,
        stones: 2,
    });
    clock.spend(0.25, false);
    assert_eq!(
        clock.time_left(go_core::Color::Black).as_deref(),
        Some("time_left black 2 0")
    );
    clock.spend(3.0, true);
    assert_eq!(
        clock.time_left(go_core::Color::Black).as_deref(),
        Some("time_left black 4 1")
    );
    let mut clock = GtpClock::new(GtpTime::Fischer {
        main: i32::MAX as u32,
        increment: 10,
    });
    clock.spend(0.25, true);
    assert_eq!(
        clock.time_left(go_core::Color::White).as_deref(),
        Some("time_left white 2147483647 0")
    );
    for time in [
        GtpTime::PerMove(u32::MAX),
        GtpTime::Absolute { main: u32::MAX },
        GtpTime::Fischer {
            main: 10,
            increment: u32::MAX,
        },
        GtpTime::Byoyomi {
            main: u32::MAX,
            period: 1,
            stones: 1,
        },
        GtpTime::Byoyomi {
            main: 1,
            period: u32::MAX,
            stones: 1,
        },
        GtpTime::Byoyomi {
            main: 1,
            period: 1,
            stones: u32::MAX,
        },
    ] {
        assert!(time.validate().is_err());
    }
}

#[test]
fn gtp_genmove_timeout_covers_known_remaining_budget_and_preserves_unknown_modes() {
    let default = Duration::from_millis(100);
    for (time, expected) in [
        (GtpTime::PerMove(300), Duration::from_secs(305)),
        (GtpTime::Absolute { main: 300 }, Duration::from_secs(305)),
        (
            GtpTime::Fischer {
                main: 300,
                increment: 10,
            },
            Duration::from_secs(305),
        ),
        (
            GtpTime::Byoyomi {
                main: 300,
                period: 30,
                stones: 2,
            },
            Duration::from_secs(335),
        ),
        (GtpTime::EngineDefault, default),
        (
            GtpTime::Custom(vec!["time_settings 0 30 1".into()]),
            default,
        ),
    ] {
        let clock = GtpClock::new(time);
        assert_eq!(clock.response_timeout(default), expected);
        assert_eq!(
            clock.response_timeout(Duration::from_secs(1000)),
            Duration::from_secs(1000)
        );
    }
    let mut clock = GtpClock::new(GtpTime::Byoyomi {
        main: 10,
        period: 20,
        stones: 2,
    });
    clock.spend(21.0, true);
    assert_eq!(clock.response_timeout(default), Duration::from_secs(14));
    let mut clock = GtpClock::new(GtpTime::Fischer {
        main: 10,
        increment: 2,
    });
    clock.spend(3.0, true);
    assert_eq!(clock.response_timeout(default), Duration::from_secs(14));
}
