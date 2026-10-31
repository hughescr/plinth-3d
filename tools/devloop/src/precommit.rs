//! `devloop pre-commit`: the fast checks of the pre-commit hook (design dev-loop.md section 9),
//! budget 5 s. Format check in every Rust workspace, the checker independence check, and the
//! text rules over the staged change. Exit: 0 pass; 1 a check failed (commit refused); 3 over
//! the 5 s budget; 4 setup error.

use crate::budget;
use crate::independence;
use crate::proc;
use crate::scan::{self, Finding};
use crate::sys;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

/// Every Rust workspace of the checkout, relative to the root: the main workspace, the checker,
/// and the loop runner.
const WORKSPACES: [&str; 3] = [".", "checker", "tools/devloop"];

/// `cargo fmt --check` in one workspace: an empty string when clean, else why not.
fn fmt_check(root: &Path, ws: &str) -> String {
    let dir = root.join(ws);
    let mut cmd = Command::new("cargo");
    cmd.args(["fmt", "--check"]).current_dir(&dir);
    if ws == "." {
        cmd.arg("--all");
    }
    match cmd.output() {
        Ok(o) if o.status.success() => String::new(),
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            let head: Vec<&str> = text.lines().take(30).collect();
            format!("cargo fmt --check fails in {ws}:\n{}", head.join("\n"))
        }
        Err(e) => format!("cannot run cargo fmt in {ws}: {e}"),
    }
}

/// Format checks of all workspaces at once (they share no lock).
fn fmt_all(root: &Path) -> Vec<String> {
    std::thread::scope(|s| {
        let handles: Vec<_> = WORKSPACES
            .iter()
            .map(|ws| s.spawn(move || fmt_check(root, ws)))
            .collect();
        handles
            .into_iter()
            .filter_map(|h| h.join().ok())
            .filter(|m| !m.is_empty())
            .collect()
    })
}

/// Text rules over the staged change; the index is whatever git is committing.
fn staged_findings(root: &Path) -> Result<Vec<Finding>, String> {
    let status = proc::git(
        root,
        &[
            "-c",
            "core.quotepath=off",
            "diff",
            "--cached",
            "--name-status",
            "--no-renames",
        ],
    )?;
    let mut found = scan::protected_changes(&status);
    let diff = proc::git(
        root,
        &[
            "-c",
            "core.quotepath=off",
            "diff",
            "--cached",
            "-U4",
            "--no-color",
            "--no-ext-diff",
            "--no-renames",
            "--diff-filter=ACM",
            // A user setting (diff.mnemonicPrefix) may change the "a/" and "b/" prefixes.
            "--src-prefix=a/",
            "--dst-prefix=b/",
        ],
    )?;
    found.extend(scan::scan_diff(&diff));
    for line in status.lines() {
        let mut f = line.split('\t');
        let (kind, path) = (f.next().unwrap_or(""), f.next().unwrap_or(""));
        if kind != "D" && path.ends_with(".rs") {
            let blob = format!(":{path}");
            let text = proc::git(root, &["show", &blob])?;
            found.extend(scan::too_long(path, text.lines().count()));
        }
    }
    Ok(found)
}

pub fn main(root: &Path) -> i32 {
    let started = Instant::now();
    let budget_s = match budget::load(root, "Pre-commit") {
        Ok(Some(r)) => r.budget_s,
        _ => {
            eprintln!("pre-commit: loop-budgets.tsv has no Pre-commit row");
            return 4;
        }
    };
    let mut failed = false;
    let fmt = fmt_all(root);
    for m in &fmt {
        eprintln!("pre-commit: {m}");
        failed = true;
    }
    if independence::main(root) != 0 {
        eprintln!("pre-commit: checker independence failed");
        failed = true;
    }
    match staged_findings(root) {
        Ok(found) if found.is_empty() => {}
        Ok(found) => {
            eprintln!("pre-commit: the staged change breaks these rules:");
            scan::report(&found);
            failed = true;
        }
        Err(e) => {
            eprintln!("pre-commit: cannot read the staged change: {e}");
            return 4;
        }
    }
    let secs = started.elapsed().as_secs_f64();
    let load = sys::load1();
    println!("pre-commit: {secs:.2} s / budget {budget_s} s / load {load:.2}");
    if failed {
        eprintln!("pre-commit: commit refused");
        return 1;
    }
    if secs > budget_s as f64 {
        let tag = if load > 8.0 { "LOADED " } else { "" };
        eprintln!("pre-commit: {tag}LOOP BUDGET EXCEEDED ({secs:.2} s over {budget_s} s)");
        return 3;
    }
    0
}
