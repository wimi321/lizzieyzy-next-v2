use std::process::Command;

#[test]
fn prints_usage_and_exits_successfully() {
    let output = Command::new(env!("CARGO_BIN_EXE_fake-katago"))
        .output()
        .expect("fake-katago should start");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("usage should be UTF-8"),
        "Usage: fake-katago\nProtocol simulation is not implemented yet.\n"
    );
    assert!(output.stderr.is_empty());
}
