#![cfg(windows)]
#![allow(clippy::expect_used)]

use std::{process::Stdio, time::Duration};

use engine_session::spawn_process;
use tokio::{process::Command, time::timeout};

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
    assert_eq!(console_window(false).await, "console:present");
    assert_eq!(console_window(true).await, "console:none");
}
