use super::artifact_path;
use std::path::PathBuf;

#[test]
fn executable_is_read_from_cargo_artifact_not_target_layout() {
    let messages = concat!(
        "not a cargo message\n",
        "{\"reason\":\"compiler-artifact\",\"target\":{\"name\":\"fake_katago\",\"kind\":[\"lib\"]},\"executable\":null}\n",
        "{\"reason\":\"compiler-artifact\",\"target\":{\"name\":\"different-binary\",\"kind\":[\"bin\"]},\"executable\":\"/wrong\"}\n",
        "{\"reason\":\"compiler-artifact\",\"target\":{\"name\":\"fake-katago\",\"kind\":[\"bin\"]},\"executable\":\"custom target/profile/fake-katago.exe\"}\n",
        "{\"reason\":\"build-finished\",\"success\":true}\n",
    );
    assert_eq!(
        artifact_path(messages),
        Some(PathBuf::from("custom target/profile/fake-katago.exe"))
    );
}

#[test]
fn absent_or_incomplete_executable_artifacts_are_rejected() {
    for messages in [
        "",
        "{}",
        "{\"reason\":\"compiler-artifact\",\"target\":{\"name\":\"fake-katago\",\"kind\":[\"bin\"]},\"executable\":null}",
        "{\"reason\":\"compiler-artifact\",\"target\":{\"name\":\"fake-katago\"},\"executable\":\"/wrong\"}",
    ] {
        assert_eq!(artifact_path(messages), None);
    }
}
