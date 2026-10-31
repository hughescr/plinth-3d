//! `devloop gate`: the merge gate (design dev-loop.md section 9), budget 180 s. Run it in the
//! background. Steps, in order, each stopping the gate at its first failure:
//!
//! * clippy in every Rust workspace (`-D warnings`);
//! * the loop runner's own unit tests;
//! * L2, the fast suite, and L3, the Round bases;
//! * static checks: checker independence, text rules over the whole tree, the raw-record grep,
//!   the provenance of `test-data/` and `reference/`, and that nothing under them changed;
//! * the loop-bench record, when the build configuration changed (design section 9, step 5);
//! * commit signatures.
//!
//! Exit: 0 pass; 1 a step failed; 3 over the gate budget; 4 setup error; 5 interrupted.
//! `gate --clean` is the Cold level (see `clean.rs`).

use crate::budget;
use crate::identity;
use crate::independence;
use crate::proc::{self, Exit};
use crate::provenance;
use crate::scan;
use crate::supervise::{self, End, Spec};
use crate::sys;
use crate::t;
use std::path::Path;
use std::time::{Duration, Instant};

type Step = Result<(), Exit>;

pub fn fail(code: i32, msg: impl Into<String>) -> Exit {
    Exit {
        code,
        msg: msg.into(),
    }
}

/// The last `n` lines of a log.
fn tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

struct Ctx<'a> {
    root: &'a Path,
    alloc: t::Alloc,
}

/// Run a shell command in the checkout under supervision (own process group, deadline,
/// log); a nonzero exit, a timeout or a build-folder lock wait fails the step.
fn shell(c: &Ctx, name: &str, script: &str, limit_s: u64) -> Step {
    shell_in(c.root, c.alloc.threads, name, script, limit_s).map(|_| ())
}

/// [`shell`] for any checkout and thread count; the seconds the command took.
pub fn shell_in(
    root: &Path,
    threads: u32,
    name: &str,
    script: &str,
    limit_s: u64,
) -> Result<f64, Exit> {
    let jobs = threads.to_string();
    let log = root.join(format!("target/gate-{name}.log"));
    std::fs::create_dir_all(root.join("target")).map_err(|e| fail(4, e.to_string()))?;
    let spec = Spec {
        argv: vec!["sh".into(), "-c".into(), script.into()],
        cwd: root.to_path_buf(),
        env: vec![
            ("CARGO_BUILD_JOBS".into(), jobs),
            ("CARGO_TERM_COLOR".into(), "never".into()),
        ],
        log: log.clone(),
        term_after: Duration::from_secs(limit_s),
        grace: Duration::from_secs(5),
    };
    let ran = supervise::run(&spec, &|| false).map_err(|e| fail(4, e))?;
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    let why = match ran.end {
        End::Exited(0) if ran.group_gone => return Ok(ran.elapsed.as_secs_f64()),
        End::Exited(0) => "a child process was left running".to_owned(),
        End::Exited(n) => format!("exit code {n}"),
        End::TimedOut => format!("timed out after {limit_s} s"),
        End::LockWait => {
            "LOCK WAIT: another cargo process is using this slot's build folder".into()
        }
        End::Interrupted => return Err(fail(5, "interrupted")),
    };
    Err(fail(
        1,
        format!(
            "{name}: {why}\n{}\n(full log: {})",
            tail(&text, 40),
            log.display()
        ),
    ))
}

fn clippy(c: &Ctx) -> Step {
    shell(
        c,
        "clippy",
        "cargo clippy --workspace --all-targets --locked -- -D warnings \
         && (cd checker && cargo clippy --all-targets --locked -- -D warnings) \
         && (cd tools/devloop && cargo clippy --all-targets --locked -- -D warnings)",
        100,
    )
}

fn devloop_tests(c: &Ctx) -> Step {
    shell(
        c,
        "devloop-tests",
        "cd tools/devloop && cargo test --locked --quiet",
        100,
    )
}

fn level(c: &Ctx, name: &str) -> Step {
    let plan = t::prepare(c.root, &[name.to_owned()])?;
    let outcome = t::execute(&plan, &|| false)?;
    t::print(&outcome);
    if outcome.code == 0 {
        Ok(())
    } else {
        Err(fail(outcome.code, format!("{name} did not pass")))
    }
}

fn l2(c: &Ctx) -> Step {
    level(c, "L2")
}

fn l3(c: &Ctx) -> Step {
    level(c, "L3")
}

/// The branch the lane started from.
pub fn base_ref(root: &Path) -> Result<String, String> {
    ["develop", "main", "origin/develop", "origin/main"]
        .iter()
        .find(|b| proc::git_ok(root, &["rev-parse", "--verify", "--quiet", **b]))
        .map(|b| (*b).to_owned())
        .ok_or_else(|| "no base branch (develop or main) to compare against".to_owned())
}

const QUOTE_OFF: [&str; 2] = ["-c", "core.quotepath=off"];

/// `git` with quoted paths turned off, for output that is parsed.
fn git_plain(root: &Path, args: &[&str]) -> Result<String, String> {
    let all: Vec<&str> = QUOTE_OFF.iter().chain(args).copied().collect();
    proc::git(root, &all)
}

/// Paths changed under the read-only folders since the lane left its base, including
/// uncommitted and new files.
fn protected(root: &Path, base: &str) -> Result<Vec<scan::Finding>, String> {
    let mb = proc::git(root, &["merge-base", base, "HEAD"])?;
    let mut ns = git_plain(root, &["diff", "--name-status", "--no-renames", &mb])?;
    let new = git_plain(
        root,
        &[
            "ls-files",
            "--others",
            "--exclude-standard",
            "--",
            "test-data",
            "reference",
        ],
    )?;
    for p in new.lines() {
        ns.push_str(&format!("\nA\t{p}"));
    }
    Ok(scan::protected_changes(&ns))
}

fn tree_findings(root: &Path) -> Result<Vec<scan::Finding>, String> {
    let listed = git_plain(
        root,
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
            "*.rs",
        ],
    )?;
    let mut found = Vec::new();
    for path in listed.lines() {
        let Ok(text) = std::fs::read_to_string(root.join(path)) else {
            continue;
        };
        found.extend(scan::scan_text(path, &text));
        found.extend(scan::too_long(path, text.lines().count()));
        found.extend(scan::raw_records(path, &text));
    }
    Ok(found)
}

fn statics(c: &Ctx) -> Step {
    if independence::main(c.root) != 0 {
        return Err(fail(1, "checker independence failed"));
    }
    let base = base_ref(c.root).map_err(|e| fail(4, e))?;
    let mut found = tree_findings(c.root).map_err(|e| fail(4, e))?;
    found.extend(protected(c.root, &base).map_err(|e| fail(4, e))?);
    if !found.is_empty() {
        scan::report(&found);
        return Err(fail(1, format!("{} text rule violation(s)", found.len())));
    }
    let drift = provenance::check(c.root).map_err(|e| fail(4, e))?;
    if !drift.is_empty() {
        return Err(fail(
            1,
            format!(
                "provenance of test-data and reference:\n  {}",
                drift.join("\n  ")
            ),
        ));
    }
    Ok(())
}

/// Step 5 of the design: when the build configuration changed, a passing loop-bench record for
/// exactly this tree and configuration must exist. The gate does not run the benchmark.
fn bench_record(c: &Ctx) -> Step {
    let base = base_ref(c.root).map_err(|e| fail(4, e))?;
    let mb = proc::git(c.root, &["merge-base", &base, "HEAD"]).map_err(|e| fail(4, e))?;
    let changed = proc::git(c.root, &["diff", "--name-only", &mb]).map_err(|e| fail(4, e))?;
    let touched = identity::config_paths(changed.lines());
    if touched.is_empty() {
        println!("gate: build configuration unchanged since {base}; no bench record needed");
        return Ok(());
    }
    let dirty = !proc::git(c.root, &["status", "--porcelain"])
        .map_err(|e| fail(4, e))?
        .is_empty();
    let setup = |why: &str| {
        fail(
            4,
            format!(
                "SETUP ERROR: build configuration changed ({}); {why}. Schedule scripts/loop-bench in the bench slot on this exact commit.",
                touched.join(", ")
            ),
        )
    };
    if dirty {
        return Err(setup(
            "the tree is not clean, so no bench record can match it",
        ));
    }
    let id = identity::current(c.root).map_err(|e| fail(4, e))?;
    let record = identity::record_path(c.root).map_err(|e| fail(4, e))?;
    let text = std::fs::read_to_string(&record).unwrap_or_default();
    if id.has_passing_row(&text) {
        println!("gate: a passing bench record matches tree {}", id.tree);
        Ok(())
    } else {
        Err(setup(
            "no passing bench record matches this tree, configuration, toolchain, fixtures and stage manifest",
        ))
    }
}

/// Signature letters that fail a commit.
fn bad_signature(letter: &str) -> bool {
    letter != "G"
}

fn signatures(c: &Ctx) -> Step {
    let base = base_ref(c.root).map_err(|e| fail(4, e))?;
    let range = format!("{base}..HEAD");
    let log =
        proc::git(c.root, &["log", "--format=%G?%x09%h%x09%s", &range]).map_err(|e| fail(4, e))?;
    let bad: Vec<&str> = log
        .lines()
        .filter(|l| bad_signature(l.split('\t').next().unwrap_or("N")))
        .collect();
    if bad.is_empty() {
        Ok(())
    } else {
        Err(fail(
            1,
            format!(
                "commits without a good signature (letter, hash, subject):\n  {}",
                bad.join("\n  ")
            ),
        ))
    }
}

fn record(root: &Path, secs: f64, budget_s: u64, verdict: &str) {
    use std::io::Write;
    let tsv = root.join("target/gate-log.tsv");
    let header = "time\ttree\tclean\tseconds\tbudget_s\tload\tverdict\n";
    let (stamp, _) = proc::now_stamp();
    let tree = proc::git(root, &["rev-parse", "HEAD^{tree}"]).unwrap_or_default();
    let clean = proc::git(root, &["status", "--porcelain"]).is_ok_and(|s| s.is_empty());
    let row = format!(
        "{stamp}\t{tree}\t{clean}\t{secs:.2}\t{budget_s}\t{:.2}\t{verdict}\n",
        sys::load1()
    );
    let fresh = !tsv.exists();
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&tsv);
    if let Ok(mut f) = file {
        let _ = f.write_all(format!("{}{row}", if fresh { header } else { "" }).as_bytes());
    }
}

type StepFn = fn(&Ctx) -> Step;

const STEPS: [(&str, StepFn); 7] = [
    ("clippy", clippy),
    ("devloop-tests", devloop_tests),
    ("L2", l2),
    ("L3", l3),
    ("static", statics),
    ("bench-record", bench_record),
    ("signatures", signatures),
];

pub fn main(root: &Path, args: &[String]) -> i32 {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        [] => run(root),
        ["--clean"] => crate::clean::main(root),
        _ => {
            eprintln!("usage: scripts/gate [--clean]");
            4
        }
    }
}

fn run(root: &Path) -> i32 {
    let started = Instant::now();
    let budget_s = match budget::load(root, "Gate") {
        Ok(Some(r)) => r.budget_s,
        _ => {
            eprintln!("gate: loop-budgets.tsv has no Gate row");
            return 4;
        }
    };
    let alloc = match t::allocation(root) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("gate: SETUP ERROR: {e}");
            return 4;
        }
    };
    let ctx = Ctx { root, alloc };
    for (name, step) in STEPS {
        let t0 = Instant::now();
        if let Err(e) = step(&ctx) {
            eprintln!(
                "gate: {name} FAILED ({:.1} s)\n{}",
                t0.elapsed().as_secs_f64(),
                e.msg
            );
            let total = started.elapsed().as_secs_f64();
            record(root, total, budget_s, &format!("FAIL {name}"));
            return e.code;
        }
        println!("gate: {name} ok ({:.1} s)", t0.elapsed().as_secs_f64());
    }
    let secs = started.elapsed().as_secs_f64();
    let load = sys::load1();
    println!(
        "gate: {secs:.1} s / budget {budget_s} s / load {load:.2} / threads {}",
        ctx.alloc.threads
    );
    if secs > budget_s as f64 {
        let tag = if load > 8.0 { "LOADED " } else { "" };
        eprintln!("gate: {tag}LOOP BUDGET EXCEEDED ({secs:.1} s over {budget_s} s)");
        record(root, secs, budget_s, "BUDGET");
        return 3;
    }
    record(root, secs, budget_s, "PASS");
    println!("gate: PASS");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_good_signature_passes() {
        assert!(!bad_signature("G"));
        for l in ["N", "B", "U", "E", "X", "Y", "R", ""] {
            assert!(bad_signature(l), "{l}");
        }
    }

    #[test]
    fn tail_keeps_the_last_lines() {
        assert_eq!(tail("a\nb\nc\nd", 2), "c\nd");
        assert_eq!(tail("a", 5), "a");
    }
}
