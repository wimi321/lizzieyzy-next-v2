//! Scriptable, std-only GTP peer. It never evaluates a position or loads a model.
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::{self, BufRead, Write},
    path::Path,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("fake-gtp: {error}");
        std::process::exit(1);
    }
}

fn options() -> Result<BTreeMap<String, String>, String> {
    let mut result = BTreeMap::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--no-undo" | "--kata" | "--no-final-score" | "--fail-startup" => {
                result.insert(arg, String::new());
            }
            "--moves"
            | "--final-score"
            | "--hang-command"
            | "--exit-command"
            | "--fail-command"
            | "--stderr-lines"
            | "--launch-log"
            | "--command-log"
            | "--exit-log"
            | "--handicap"
            | "--name"
            | "--version"
            | "--protocol-version"
            | "--malformed-command"
            | "--script" => {
                let value = args
                    .next()
                    .ok_or_else(|| format!("missing argument for {arg}"))?;
                result.insert(arg, value);
            }
            _ => return Err(format!("unknown argument {arg}")),
        }
    }
    Ok(result)
}

fn append(path: Option<&String>, text: &str) -> Result<(), String> {
    if let Some(path) = path {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        writeln!(file, "{text}").map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn run() -> Result<(), String> {
    let options = options()?;
    append(options.get("--launch-log"), &std::process::id().to_string())?;
    if options.contains_key("--fail-startup") {
        return Err("scripted startup failure".into());
    }
    let moves = options
        .get("--moves")
        .map(String::as_str)
        .unwrap_or("pass")
        .split(',')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut move_index: usize = 0;
    let mut script = BTreeMap::new();
    if let Some(path) = options.get("--script") {
        // Each line is command<TAB>body; a leading ? in body rejects the command.
        for line in std::fs::read_to_string(Path::new(path))
            .map_err(|e| e.to_string())?
            .lines()
        {
            if let Some((name, body)) = line.split_once('\t') {
                script.insert(name.to_owned(), body.replace("\\n", "\n"));
            }
        }
    }
    let stderr_lines = options
        .get("--stderr-lines")
        .map(|v| v.parse::<usize>())
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or(0);
    for i in 0..stderr_lines {
        eprintln!("fake stderr {i}");
    }
    let mut supported = vec![
        "protocol_version",
        "name",
        "version",
        "known_command",
        "list_commands",
        "boardsize",
        "clear_board",
        "komi",
        "fixed_handicap",
        "place_free_handicap",
        "set_free_handicap",
        "play",
        "genmove",
        "time_settings",
        "time_left",
        "quit",
        "echo",
        "spam",
    ];
    if !options.contains_key("--no-undo") {
        supported.push("undo");
    }
    if !options.contains_key("--no-final-score") {
        supported.push("final_score");
    }
    if options.contains_key("--kata") {
        supported.extend(["kata-set-rules", "kata-time_settings"]);
    }
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        append(options.get("--command-log"), &line)?;
        let mut words = line.split_ascii_whitespace();
        let Some(first) = words.next() else {
            continue;
        };
        let (id, name) = if first.bytes().all(|b| b.is_ascii_digit()) {
            (first, words.next().unwrap_or(""))
        } else {
            ("", first)
        };
        let args = words.collect::<Vec<_>>();
        if options
            .get("--exit-command")
            .is_some_and(|target| target == name)
        {
            return Err(format!("scripted exit on {name}"));
        }
        if options
            .get("--hang-command")
            .is_some_and(|target| target == name)
        {
            continue;
        }
        if options
            .get("--malformed-command")
            .is_some_and(|target| target == name)
        {
            writeln!(stdout, "not a response\n").map_err(|e| e.to_string())?;
            stdout.flush().map_err(|e| e.to_string())?;
            continue;
        }
        let mut success = true;
        let body = if !valid_standard_time(name, &args) {
            success = false;
            "invalid time arguments: expected nonnegative integers".to_owned()
        } else if options
            .get("--fail-command")
            .is_some_and(|target| target == name)
        {
            success = false;
            "scripted command failure".to_owned()
        } else if let Some(body) = script.get(name) {
            if let Some(message) = body.strip_prefix('?') {
                success = false;
                message.trim_start().to_owned()
            } else {
                body.clone()
            }
        } else if !supported.contains(&name) {
            success = false;
            "unknown command".to_owned()
        } else {
            match name {
                "protocol_version" => options
                    .get("--protocol-version")
                    .cloned()
                    .unwrap_or_else(|| "2".into()),
                "name" => options
                    .get("--name")
                    .cloned()
                    .unwrap_or_else(|| "Fake GTP".into()),
                "version" => options
                    .get("--version")
                    .cloned()
                    .unwrap_or_else(|| "1.0-fake".into()),
                "list_commands" => supported.join("\n"),
                "known_command" => args
                    .first()
                    .is_some_and(|name| supported.contains(name))
                    .to_string(),
                "genmove" => {
                    let value = moves
                        .get(move_index)
                        .or_else(|| moves.last())
                        .cloned()
                        .unwrap_or_else(|| "pass".into());
                    move_index += 1;
                    value
                }
                "clear_board" => {
                    move_index = 0;
                    String::new()
                }
                "undo" => {
                    move_index = move_index.saturating_sub(1);
                    String::new()
                }
                "fixed_handicap" | "place_free_handicap" => options
                    .get("--handicap")
                    .cloned()
                    .unwrap_or_else(|| "C3 G7".into())
                    .replace(',', " "),
                "final_score" => options
                    .get("--final-score")
                    .cloned()
                    .unwrap_or_else(|| "B+2.5".into()),
                "echo" => args.join(" "),
                "spam" => {
                    let count = args
                        .first()
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(0)
                        .min(10000);
                    for i in 0..count {
                        eprintln!("fake spam {i}");
                    }
                    String::new()
                }
                _ => String::new(),
            }
        };
        writeln!(stdout, "{}{id} {body}\n", if success { '=' } else { '?' })
            .map_err(|e| e.to_string())?;
        stdout.flush().map_err(|e| e.to_string())?;
        if name == "quit" {
            break;
        }
    }
    append(options.get("--exit-log"), &std::process::id().to_string())?;
    Ok(())
}

// GTP 2 specifies integer seconds for both standard time commands. KataGo
// extensions intentionally have their own grammar and are not checked here.
fn valid_standard_time(name: &str, args: &[&str]) -> bool {
    match name {
        "time_settings" => {
            args.len() == 3
                && args
                    .iter()
                    .all(|value| value.parse::<i32>().is_ok_and(|value| value >= 0))
        }
        "time_left" => {
            args.len() == 3
                && ["b", "w", "black", "white"]
                    .iter()
                    .any(|color| args[0].eq_ignore_ascii_case(color))
                && args[1..]
                    .iter()
                    .all(|value| value.parse::<i32>().is_ok_and(|value| value >= 0))
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests;
