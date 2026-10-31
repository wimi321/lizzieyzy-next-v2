//! Cross-platform playback helper. Place replay.txt beside the copied executable
//! and use that directory as cwd. The seven arguments match native readboard.
//! Outputs ready, replays on the first version request, then awaits quit or EOF.

use std::fs::{self, File};
use std::io::{self, BufRead, Write};
use std::process::ExitCode;

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 7 || args[0] != "yzy" || args[4] != "0" || args[6] != "-1" {
        return Err("usage: fake-readboard yzy TIME VISITS FIRST_VISITS 0 LANGUAGE -1".into());
    }
    let replay = fs::read_to_string("replay.txt")?;
    let mut commands = File::create("commands.log")?;
    let stdout = io::stdout();
    let mut output = stdout.lock();
    writeln!(output, "ready")?;
    output.flush()?;
    let mut replayed = false;
    for command in io::stdin().lock().lines() {
        let command = command?;
        writeln!(commands, "{command}")?;
        commands.flush()?;
        match command.as_str() {
            "quit" => {
                fs::write("exited", "quit\n")?;
                return Ok(());
            }
            "version" => {
                writeln!(output, "version 220430")?;
                if !replayed {
                    for line in replay
                        .lines()
                        .filter(|line| !line.starts_with('#') && !line.is_empty())
                    {
                        writeln!(output, "{line}")?;
                        output.flush()?;
                    }
                    replayed = true;
                }
                output.flush()?;
            }
            _ => {}
        }
    }
    fs::write("exited", "eof\n")?;
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
