//! `devloop setup`: the first command after a clone. Points git at the committed hooks and checks
//! the tools. Safe to run again; it changes only this clone's git config.

use crate::proc;
use std::path::Path;

/// (program, how to get it)
const TOOLS: [(&str, &str); 4] = [
    ("cargo", "install rustup from https://rustup.rs"),
    ("jq", "brew install jq"),
    ("hyperfine", "brew install hyperfine"),
    ("cargo-nextest", "cargo install cargo-nextest --locked"),
];

/// The tools from `TOOLS` that `present` does not find.
fn missing(present: &dyn Fn(&str) -> bool) -> Vec<(&'static str, &'static str)> {
    TOOLS
        .iter()
        .filter(|(name, _)| !present(name))
        .copied()
        .collect()
}

fn show(root: &Path, args: &[&str]) {
    match proc::out("cargo", args, root) {
        Ok(v) => println!("{v}"),
        Err(e) => eprintln!("{e}"),
    }
}

pub fn main(root: &Path) -> i32 {
    let mut fail = false;
    for (name, how) in missing(&proc::has_tool) {
        eprintln!("MISSING: {name} ({how})");
        fail = true;
    }
    if std::env::var_os("RUSTFLAGS").is_some_and(|v| !v.is_empty()) {
        eprintln!("RUSTFLAGS is set; unset it. It would throw away every build artifact.");
        fail = true;
    }
    // rust-toolchain.toml pins the channel; running cargo here installs it if needed.
    for args in [
        &["--version"][..],
        &["fmt", "--version"],
        &["clippy", "--version"],
    ] {
        show(root, args);
    }
    if let Err(e) = proc::out("cargo", &["--version"], &root.join("checker")) {
        eprintln!("{e}");
        fail = true;
    }
    match proc::git(root, &["config", "core.hooksPath", ".githooks"])
        .and_then(|_| proc::git(root, &["config", "core.hooksPath"]))
    {
        Ok(v) => println!("core.hooksPath = {v}"),
        Err(e) => {
            eprintln!("{e}");
            fail = true;
        }
    }
    i32::from(fail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_exactly_the_missing_tools() {
        let got = missing(&|n| n != "jq" && n != "cargo-nextest");
        let names: Vec<&str> = got.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, vec!["jq", "cargo-nextest"]);
        assert!(missing(&|_| true).is_empty());
    }
}
