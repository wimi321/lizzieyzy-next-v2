//! E2E-only executable adapter for tauri-driver's --native-driver option.
//! Built directly with rustc; never linked into the desktop application.

use std::{
    env,
    ffi::OsString,
    fs::{self, OpenOptions},
    io,
    path::PathBuf,
    process::{Command, ExitStatus, Stdio},
};

fn required_path(name: &str) -> io::Result<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("{name} is required")))
}

fn run() -> io::Result<ExitStatus> {
    let driver = required_path("TAURI_E2E_NATIVE_DRIVER")?;
    let diagnostics = required_path("TAURI_E2E_DIAGNOSTICS")?;
    fs::create_dir_all(&diagnostics)?;
    let output = |name: &str| {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(diagnostics.join(name))
    };
    let mut log_path = OsString::from("--log-path=");
    log_path.push(diagnostics.join("msedgedriver.log"));
    // Keep arguments as native strings; no shell, joining, or path quoting.
    // tauri-driver discards native stdout, so save both native streams here.
    // Application output is captured only when the driver passes these handles
    // to its child; these files are not guaranteed application-only logs.
    let mut child = Command::new(&driver)
        .args(env::args_os().skip(1))
        .arg("--verbose")
        .arg(log_path)
        .env("CHROME_LOG_FILE", diagnostics.join("webview2.log"))
        .stdout(Stdio::from(output("native-stdout.log")?))
        .stderr(Stdio::from(output("native-stderr.log")?))
        .spawn()?;
    eprintln!(
        "Native WebDriver started: pid={}, executable={driver:?}",
        child.id()
    );
    let status = child.wait()?;
    eprintln!("Native WebDriver exited: {status}");
    Ok(status)
}

fn main() {
    let code = match run() {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            eprintln!("Native WebDriver wrapper failed: {error}");
            1
        }
    };
    std::process::exit(code);
}
