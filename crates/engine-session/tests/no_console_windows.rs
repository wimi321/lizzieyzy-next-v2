#![cfg(windows)]
#![allow(clippy::expect_used)]

use std::{process::Stdio, time::Duration};

use engine_session::spawn_process;
use tokio::{process::Command, time::timeout};

const DETACHED_PROCESS: u32 = 0x0000_0008;
const CHILD_MODE_ENV: &str = "LIZZIE_P3_35_DETACHED_TEST_CHILD";
const RESULT_PREFIX: &str = "NO_CONSOLE_RESULT ";

const CONSOLE_PROBE: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition 'using System;
using System.Runtime.InteropServices;
public static class ConsoleProbe {
    [DllImport("kernel32.dll")]
    public static extern IntPtr GetConsoleWindow();
}'
if ([ConsoleProbe]::GetConsoleWindow() -eq [IntPtr]::Zero) {
    [Console]::WriteLine('console:none')
} else {
    [Console]::WriteLine('console:present')
}
"#;

async fn console_window(use_spawn_process: bool) -> String {
    // Each probe gets a fresh command so the control cannot inherit the flags
    // applied by spawn_process to the other command.
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            CONSOLE_PROBE,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = timeout(Duration::from_secs(5), async {
        let child = if use_spawn_process {
            spawn_process(&mut command).await
        } else {
            command.spawn()
        }
        .expect("start the PowerShell console probe");
        child.wait_with_output().await
    })
    .await
    .expect("console probe must exit within the safety deadline")
    .expect("collect the console probe output");
    assert!(
        output.status.success(),
        "console probe failed: status={}, stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("console probe emits ASCII")
        .trim()
        .to_owned()
}

#[tokio::test]
async fn spawn_process_suppresses_console_while_plain_spawn_has_one() {
    if std::env::var_os(CHILD_MODE_ENV).is_some() {
        let plain = console_window(false).await;
        let spawned = console_window(true).await;
        // Start a new line even if libtest printed its test-name prefix first.
        println!("\n{RESULT_PREFIX}plain={plain} spawn_process={spawned}");
        return;
    }

    // CI can itself run inside a windowless console. Detach the probe's parent
    // entirely to reproduce a GUI application's lack of an attached console.
    let mut command = Command::new(std::env::current_exe().expect("locate this test executable"));
    command
        .args([
            stringify!(spawn_process_suppresses_console_while_plain_spawn_has_one),
            "--exact",
            "--nocapture",
        ])
        .env(CHILD_MODE_ENV, "1")
        .creation_flags(DETACHED_PROCESS)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    // The detached child runs two independently bounded PowerShell probes.
    let output = timeout(Duration::from_secs(15), async {
        command
            .spawn()
            .expect("start the detached test child")
            .wait_with_output()
            .await
    })
    .await
    .expect("detached test child must exit within the safety deadline")
    .expect("collect the detached test child output");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "detached test child failed: status={}, stdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
    let result = stdout.lines().find(|line| line.starts_with(RESULT_PREFIX));
    assert_eq!(
        result,
        Some("NO_CONSOLE_RESULT plain=console:present spawn_process=console:none"),
        "missing or unexpected console result; stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
