use super::valid_standard_time;

#[test]
fn standard_time_commands_reject_fractional_seconds_and_bad_arguments() {
    for args in [
        vec!["60", "30", "1"],
        vec!["0", "1", "1"],
        vec!["0", "0", "0"],
    ] {
        assert!(valid_standard_time("time_settings", &args));
    }
    for args in [
        vec!["black", "60", "0"],
        vec!["W", "30", "1"],
        vec!["b", "0", "0"],
    ] {
        assert!(valid_standard_time("time_left", &args));
    }
    for args in [
        vec!["60.5", "30", "1"],
        vec!["60", "30.5", "1"],
        vec!["60", "30", "1.5"],
        vec!["-1", "30", "1"],
        vec!["2147483648", "30", "1"],
        vec!["60", "30"],
        vec!["60", "30", "1", "extra"],
    ] {
        assert!(!valid_standard_time("time_settings", &args));
    }
    for args in [
        vec!["b", "50.5", "0"],
        vec!["b", "-1", "0"],
        vec!["b", "2147483648", "0"],
        vec!["b", "50", "0.5"],
        vec!["invalid", "50", "0"],
        vec!["b", "50"],
        vec!["b", "50", "0", "extra"],
    ] {
        assert!(!valid_standard_time("time_left", &args));
    }
    assert!(valid_standard_time(
        "kata-time_settings",
        &["fischer", "60.5", "0.5"]
    ));
}
