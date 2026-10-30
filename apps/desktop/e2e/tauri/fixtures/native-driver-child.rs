//! Small real process used only by native-driver.test.py.
use std::{env, ffi::OsString, fs, path::PathBuf, process::Command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args == [OsString::from("--application-child")] {
        println!("inherited application stdout");
        eprintln!("inherited application stderr");
        return Ok(());
    }
    let directory =
        PathBuf::from(env::var_os("TAURI_E2E_DIAGNOSTICS").ok_or("missing diagnostics")?);
    let mut log_path = OsString::from("--log-path=");
    log_path.push(directory.join("msedgedriver.log"));
    assert_eq!(
        args,
        [
            OsString::from("--port=4445"),
            OsString::from("--host=127.0.0.1"),
            OsString::from(r#"--path=C:\folder with spaces\a "quoted" file.sgf"#),
            OsString::from("--verbose"),
            log_path,
        ]
    );
    assert_eq!(env::var("TAURI_WEBVIEW_AUTOMATION")?, "true");
    assert_eq!(env::var("TAURI_AUTOMATION")?, "true");
    assert_eq!(
        env::var_os("CHROME_LOG_FILE"),
        Some(directory.join("webview2.log").into_os_string())
    );
    println!("native stdout; arguments and environment verified");
    eprintln!("native stderr");
    fs::write(
        directory.join("msedgedriver.log"),
        "verbose native driver log\n",
    )?;
    fs::write(directory.join("webview2.log"), "webview diagnostic log\n")?;
    let child = Command::new(env::current_exe()?)
        .arg("--application-child")
        .status()?;
    assert!(child.success());
    std::process::exit(23);
}
