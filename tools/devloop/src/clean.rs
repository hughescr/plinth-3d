//! `devloop gate --clean`: the Cold level (design dev-loop.md sections 3 and 9). A fresh clone
//! of this checkout, with its uncommitted files copied over so the exact tree is tested, is
//! made in the system temp folder, away from every sibling folder of the checkout. In it:
//!
//! 1. `cargo fetch --locked` for both Rust workspaces, timed and reported separately;
//! 2. a cold release build of the loop runner (what `scripts/setup` and `scripts/t` build first);
//! 3. L2 from fresh build folders (cold build of both workspaces, then the fast suite).
//!
//! Steps 2 and 3 together must fit the Cold budget (`loop-budgets.tsv`, 180 s). Exit: 0 pass;
//! 1 failure; 3 over budget; 4 setup error.

use crate::budget;
use crate::proc;
use crate::sys;
use crate::t;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

fn setup(msg: impl std::fmt::Display) -> i32 {
    eprintln!("gate --clean: SETUP ERROR: {msg}");
    4
}

/// Files that differ from HEAD in `root` (porcelain v1, NUL separated): (is_deleted, path).
fn dirty_paths(root: &Path) -> Result<Vec<(bool, String)>, String> {
    let out = Command::new("git")
        .args([
            "-c",
            "core.quotepath=off",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--no-renames",
        ])
        .current_dir(root)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    Ok(parse_status(&String::from_utf8_lossy(&out.stdout), root))
}

/// Entries of `git status --porcelain=v1 -z --no-renames`: `XY path` each, NUL terminated.
/// A path with no file in `root` is a deletion.
fn parse_status(text: &str, root: &Path) -> Vec<(bool, String)> {
    text.split('\0')
        .filter(|e| e.len() > 3)
        .map(|e| {
            let path = e[3..].to_owned();
            (!root.join(&path).is_file(), path)
        })
        .collect()
}

/// Make the clean copy: a clone of HEAD, then the working-tree differences.
fn make_copy(root: &Path, tmp: &Path) -> Result<usize, String> {
    let to = tmp.display().to_string();
    let from = root.display().to_string();
    proc::out(
        "git",
        &["clone", "--quiet", "--local", &from, &to],
        Path::new("/"),
    )?;
    let dirty = dirty_paths(root)?;
    for (deleted, path) in &dirty {
        let target = tmp.join(path);
        if *deleted {
            let _ = std::fs::remove_file(&target);
            continue;
        }
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{path}: {e}"))?;
        }
        std::fs::copy(root.join(path), &target).map_err(|e| format!("{path}: {e}"))?;
    }
    Ok(dirty.len())
}

/// Run `prog args` in `dir` with the allocation's job count; the seconds it took, or why not.
fn timed(dir: &Path, threads: u32, prog: &str, args: &[&str]) -> Result<f64, String> {
    let t0 = Instant::now();
    let out = Command::new(prog)
        .args(args)
        .current_dir(dir)
        .env("CARGO_BUILD_JOBS", threads.to_string())
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .map_err(|e| format!("cannot run {prog}: {e}"))?;
    if out.status.success() {
        Ok(t0.elapsed().as_secs_f64())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let lines: Vec<&str> = err.lines().collect();
        let from = lines.len().saturating_sub(30);
        Err(format!(
            "{prog} {} failed in {}:\n{}",
            args.join(" "),
            dir.display(),
            lines[from..].join("\n")
        ))
    }
}

fn run_in(copy: &Path, alloc: &t::Alloc, cold_s: u64) -> Result<(f64, f64, f64), i32> {
    let threads = alloc.threads;
    let fetch_main = timed(copy, threads, "cargo", &["fetch", "--locked"]).map_err(|e| {
        eprintln!("gate --clean: {e}");
        1
    })?;
    let fetch_checker = timed(
        &copy.join("checker"),
        threads,
        "cargo",
        &["fetch", "--locked"],
    )
    .map_err(|e| {
        eprintln!("gate --clean: {e}");
        1
    })?;
    let fetch = fetch_main + fetch_checker;
    println!(
        "gate --clean: locked fetch {fetch:.1} s (reported separately, not in the Cold budget)"
    );
    let runner = timed(
        &copy.join("tools/devloop"),
        threads,
        "cargo",
        &["build", "--release", "--locked", "--quiet"],
    )
    .map_err(|e| {
        eprintln!("gate --clean: {e}");
        1
    })?;
    println!("gate --clean: cold build of the loop runner {runner:.1} s");
    let left = cold_s.saturating_sub(runner.ceil() as u64).max(1);
    let given = t::Alloc {
        slot: "clean".into(),
        threads,
    };
    let mut plan = t::prepare_with(copy, &["L2".to_owned()], Some(given)).map_err(|e| {
        eprintln!("gate --clean: {}", e.msg);
        e.code
    })?;
    plan.relabel("Cold", left);
    let outcome = t::execute(&plan, &|| false).map_err(|e| {
        eprintln!("gate --clean: {}", e.msg);
        e.code
    })?;
    t::print(&outcome);
    let l2 = outcome.elapsed.as_secs_f64();
    if outcome.code != 0 {
        return Err(outcome.code);
    }
    Ok((fetch, runner, l2))
}

pub fn main(root: &Path) -> i32 {
    let cold_s = match budget::load(root, "Cold") {
        Ok(Some(r)) => r.budget_s,
        _ => return setup("loop-budgets.tsv has no Cold row"),
    };
    let alloc = match t::allocation(root) {
        Ok(a) => a,
        Err(e) => return setup(e),
    };
    let tmp: PathBuf = std::env::temp_dir().join(format!("plinth-clean-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let copied = match make_copy(root, &tmp) {
        Ok(n) => n,
        Err(e) => return setup(e),
    };
    println!(
        "gate --clean: fresh copy {} ({copied} uncommitted file(s) included), {} threads",
        tmp.display(),
        alloc.threads
    );
    let code = match run_in(&tmp, &alloc, cold_s) {
        Ok((fetch, runner, l2)) => {
            let cold = runner + l2;
            let load = sys::load1();
            println!(
                "gate --clean: fetch {fetch:.1} s | runner {runner:.1} s + cold build and L2 {l2:.1} s = Cold {cold:.1} s / budget {cold_s} s / load {load:.2}"
            );
            if cold > cold_s as f64 {
                let tag = if load > 8.0 { "LOADED " } else { "" };
                eprintln!("gate --clean: {tag}LOOP BUDGET EXCEEDED ({cold:.1} s over {cold_s} s)");
                3
            } else {
                println!("gate --clean: PASS");
                0
            }
        }
        Err(code) => code,
    };
    if code == 0 {
        let _ = std::fs::remove_dir_all(&tmp);
    } else {
        eprintln!("gate --clean: kept {} for inspection", tmp.display());
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_entries_become_copies_and_deletions() {
        let dir = std::env::temp_dir().join(format!("devloop-clean-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("here.rs"), "x").expect("write");
        let text = " M here.rs\0?? new name.rs\0 D gone.rs\0";
        std::fs::write(dir.join("new name.rs"), "y").expect("write");
        let got = parse_status(text, &dir);
        assert_eq!(
            got,
            [
                (false, "here.rs".to_owned()),
                (false, "new name.rs".to_owned()),
                (true, "gone.rs".to_owned())
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
