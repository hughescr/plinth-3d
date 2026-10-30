//! devloop: the plinth loop runner (design dev-loop.md sections 3.2, 3.3, 5.5, 5.6 and 10 step 3).
//!
//! Subcommands: `t`, `loop-bench`, `slot`, `setup` (later: `gate`, `check-independence`).
//! The `scripts/*` files are thin shims that build this program if needed and run it with
//! `--root <checkout>`. It depends on no plinth crate and no registry crate.

#![deny(unsafe_code)]

mod bench;
mod budget;
mod lockwatch;
mod logscan;
mod proc;
mod setup;
mod slot;
mod supervise;
mod sys;
mod t;
mod verdict;

use std::path::PathBuf;

const USAGE: &str = "usage: devloop [--root <checkout>] t|loop-bench|slot|setup [args...]";

fn root_and_rest(args: &[String]) -> Result<(PathBuf, &[String]), String> {
    if args.first().is_some_and(|a| a == "--root") {
        let dir = args.get(1).ok_or(USAGE)?;
        return Ok((PathBuf::from(dir), &args[2..]));
    }
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let top = proc::git(&cwd, &["rev-parse", "--show-toplevel"])?;
    Ok((PathBuf::from(top), args))
}

fn run(args: &[String]) -> i32 {
    let (root, rest) = match root_and_rest(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("SETUP ERROR: {e}");
            return 4;
        }
    };
    sys::install_stop_handlers();
    match rest.first().map(String::as_str) {
        Some("t") => t::main(&root, &rest[1..]),
        Some("loop-bench") => bench::main(&root),
        Some("slot") => slot::main(&root, &rest[1..]),
        Some("setup") => setup::main(&root),
        _ => {
            eprintln!("{USAGE}");
            4
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(run(&args));
}
