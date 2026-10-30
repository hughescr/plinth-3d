//! plinth-check: the independent checker.
//!
//! ```text
//! plinth-check dump <file.chitubox | file.stl>
//! plinth-check check --chitubox <file> --limits <limits.toml> [--stl <file>] [--out <report.json>]
//! ```
//!
//! `dump` prints the file's structure. `check` writes the JSON report (to stdout, or to `--out`).
//! It takes paths and flags only, no environment settings. Exit codes: `check` gives 0 for a
//! pass, 2 when some rule is unverified, 1 for a failure (a violation, or a file it cannot read
//! or that does not match the format). `dump` gives 0 for a clean file and 1 otherwise. 4 means
//! a usage or I/O error.

#![warn(clippy::cast_possible_truncation)]

use plinth_check::{dump, limits, loop_canary, report};
use std::process::ExitCode;

const USAGE: &str = "usage:\n  plinth-check dump <file.chitubox | file.stl>\n  plinth-check check --chitubox <file> --limits <limits.toml> [--stl <file>] [--out <report.json>]";

fn usage_error(msg: &str) -> ExitCode {
    eprintln!("plinth-check: {msg}\n{USAGE}");
    ExitCode::from(4)
}

fn read_input(path: &str) -> report::Input {
    report::Input {
        path: path.to_owned(),
        data: std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}")),
    }
}

fn run_dump(args: &[String]) -> ExitCode {
    let [path] = args else {
        return usage_error("dump takes exactly one file");
    };
    let input = read_input(path);
    let data = match input.data {
        Ok(d) => d,
        Err(e) => return usage_error(&e),
    };
    match dump::dump(path, &data) {
        Ok(d) => {
            print!("{}", d.text);
            ExitCode::from(u8::from(!d.clean))
        }
        Err(e) => {
            eprintln!("plinth-check: {path}: {e}");
            ExitCode::from(1)
        }
    }
}

/// The value after each `--flag`, or an error for an unknown flag or a missing value.
fn flags(args: &[String]) -> Result<Vec<(&str, &str)>, String> {
    let mut out = Vec::new();
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        if !["--chitubox", "--limits", "--stl", "--out"].contains(&flag.as_str()) {
            return Err(format!("unknown argument {flag}"));
        }
        let Some(value) = it.next() else {
            return Err(format!("{flag} needs a value"));
        };
        out.push((flag.as_str(), value.as_str()));
    }
    Ok(out)
}

fn run_check(args: &[String]) -> ExitCode {
    let pairs = match flags(args) {
        Ok(p) => p,
        Err(e) => return usage_error(&e),
    };
    let get = |name: &str| pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| *v);
    let (Some(chitubox), Some(limits_path)) = (get("--chitubox"), get("--limits")) else {
        return usage_error("check needs --chitubox and --limits");
    };
    let limits = match std::fs::read_to_string(limits_path)
        .map_err(|e| format!("cannot read {limits_path}: {e}"))
        .and_then(|t| limits::parse(&t).map_err(|e| e.to_string()))
    {
        Ok(l) => l,
        Err(e) => return usage_error(&e),
    };
    let stl = get("--stl").map(read_input);
    let rep = report::build(&limits, &read_input(chitubox), stl.as_ref());
    let text = rep.json.render();
    match get("--out") {
        Some(out) => {
            if let Err(e) = std::fs::write(out, &text) {
                return usage_error(&format!("cannot write {out}: {e}"));
            }
            println!(
                "plinth-check: verdict {} (report: {out})",
                rep.verdict.name()
            );
        }
        None => print!("{text}"),
    }
    ExitCode::from(u8::try_from(rep.verdict.exit_code()).unwrap_or(1))
}

fn main() -> ExitCode {
    // Keeps the canary in the binary so a canary edit relinks it.
    std::hint::black_box(loop_canary::loop_canary());
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.split_first() {
        Some((cmd, rest)) if cmd == "dump" => run_dump(rest),
        Some((cmd, rest)) if cmd == "check" => run_check(rest),
        _ => usage_error("expected a command: dump or check"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_pairs_up_values_and_rejects_strays() {
        let a = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(flags(&a(&["--stl", "x"])), Ok(vec![("--stl", "x")]));
        assert!(flags(&a(&["--stl"])).is_err());
        assert!(flags(&a(&["--nope", "x"])).is_err());
    }

    #[test]
    fn canary_is_stable_within_a_run() {
        assert_eq!(loop_canary::loop_canary(), loop_canary::loop_canary());
    }
}
