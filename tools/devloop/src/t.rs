//! `devloop t <level> [crate] [nextest args]`: the one wrapper for every loop command
//! (design dev-loop.md section 3.2). Exit: 0 pass or NOCHANGE; 1 failure (tests, no tests
//! selected, LOCK WAIT, package-cache wait over 2 s, leftover process); 3 LOOP BUDGET EXCEEDED;
//! 4 setup error; 5 timeout or interruption.

use crate::budget;
use crate::logscan;
use crate::proc::{self, Exit};
use crate::supervise::{self, Spec};
use crate::sys;
use crate::verdict::{self, Facts, Kind};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// TERM here; KILL 5 s later: the nominal 120 s bound.
const TERM_AFTER: Duration = Duration::from_secs(115);
const GRACE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Noop,
    L0,
    L1,
    L1d,
    L2,
}

impl Level {
    fn kind(self) -> Kind {
        match self {
            Self::Noop => Kind::Noop,
            Self::L0 => Kind::Check,
            Self::L1 | Self::L1d | Self::L2 => Kind::Test,
        }
    }
}

fn parse_level(s: &str) -> Result<Level, String> {
    match s {
        "Lnoop" => Ok(Level::Noop),
        "L0" => Ok(Level::L0),
        "L1" => Ok(Level::L1),
        "L1d" => Ok(Level::L1d),
        "L2" => Ok(Level::L2),
        "L3" | "L4" | "L5" => Err(format!(
            "level {s} is not available yet (its suites arrive in later steps)"
        )),
        _ => Err(format!("unknown level: {s}")),
    }
}

/// Package name for a crate argument: `geom`, `plinth-geom`, `check` and `checker` all work.
#[must_use]
pub fn package_of(arg: &str) -> String {
    match arg {
        "check" | "checker" | "plinth-check" => "plinth-check".into(),
        a if a.starts_with("plinth-") => a.into(),
        a => format!("plinth-{a}"),
    }
}

/// The command to run, where to run it from, and the crate label for the verdict line.
#[must_use]
pub fn build_command(
    root: &Path,
    level: Level,
    crate_arg: &str,
    extra: &[String],
) -> (Vec<String>, PathBuf, String) {
    let s = |x: &str| x.to_owned();
    if level == Level::Noop {
        let sh = "cargo build --locked && cd checker && cargo build --locked";
        return (vec![s("sh"), s("-c"), s(sh)], root.to_path_buf(), s("-"));
    }
    if level == Level::L2 {
        // Both workspaces, then the explicit doctest run (nextest skips doctests). The
        // independence check joins this chain when its script arrives.
        let sh = format!(
            "cargo nextest run --workspace --locked && cargo test --doc -p plinth-support --locked \
             && cd checker && cargo nextest run --config-file '{}/.config/nextest.toml' --locked",
            root.display()
        );
        return (vec![s("sh"), s("-c"), sh], root.to_path_buf(), s("all"));
    }
    let pkg = package_of(crate_arg);
    let is_checker = pkg == "plinth-check";
    let cwd = if is_checker {
        root.join("checker")
    } else {
        root.to_path_buf()
    };
    let mut cmd: Vec<String> = if level == Level::L0 {
        vec![s("cargo"), s("clippy")]
    } else {
        vec![s("cargo"), s("nextest"), s("run")]
    };
    if !is_checker {
        cmd.extend([s("-p"), pkg]);
    } else if level != Level::L0 {
        cmd.extend([
            s("--config-file"),
            root.join(".config/nextest.toml").display().to_string(),
        ]);
    }
    if level == Level::L0 {
        cmd.extend([
            s("--all-targets"),
            s("--locked"),
            s("--"),
            s("-D"),
            s("warnings"),
        ]);
    } else {
        cmd.push(s("--locked"));
        cmd.extend(extra.iter().cloned());
    }
    (cmd, cwd, crate_arg.to_owned())
}

/// The thread allocation of a slot: fixed per epoch, read from the slot marker, never computed.
#[derive(Debug, PartialEq, Eq)]
pub struct Alloc {
    pub slot: String,
    pub threads: u32,
}

#[must_use]
pub fn marker_threads(text: &str) -> Option<u32> {
    let n: u32 = text
        .lines()
        .find_map(|l| l.strip_prefix("threads="))?
        .trim()
        .parse()
        .ok()?;
    (n > 0).then_some(n)
}

fn allocation(root: &Path) -> Result<Alloc, String> {
    let gitdir = PathBuf::from(proc::git(root, &["rev-parse", "--absolute-git-dir"])?);
    let common = PathBuf::from(proc::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?);
    if gitdir == common {
        return Ok(Alloc {
            slot: "main".into(),
            threads: proc::env_count("PLINTH_MAIN_THREADS", 2)?,
        });
    }
    let slot = root
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let threads = match std::fs::read_to_string(gitdir.join("slot-active")) {
        Ok(text) => marker_threads(&text).ok_or("bad thread allocation in the slot marker")?,
        Err(_) if slot == "bench" => proc::env_count("PLINTH_BENCH_THREADS", 8)?,
        Err(_) => {
            return Err(format!(
                "slot {slot} has no thread allocation (scripts/slot {slot} start lane/<name>)"
            ));
        }
    };
    if slot == "bench" && any_slot_active(&common) {
        return Err(
            "the bench slot runs only in an exclusive epoch; a writing slot holds an allocation"
                .into(),
        );
    }
    Ok(Alloc { slot, threads })
}

fn any_slot_active(common: &Path) -> bool {
    std::fs::read_dir(common.join("worktrees"))
        .is_ok_and(|d| d.flatten().any(|e| e.path().join("slot-active").exists()))
}

/// Everything decided before the command starts.
#[derive(Debug)]
pub struct Plan {
    root: PathBuf,
    level_name: String,
    level: Level,
    crate_label: String,
    budget_s: u64,
    target: String,
    cmd: Vec<String>,
    cwd: PathBuf,
    alloc: Alloc,
}

fn setup(msg: impl std::fmt::Display) -> Exit {
    Exit::setup(format!("SETUP ERROR: {msg}"))
}

pub fn prepare(root: &Path, args: &[String]) -> Result<Plan, Exit> {
    let level_name = args
        .first()
        .ok_or_else(|| setup("usage: scripts/t <level> [crate] [nextest args]"))?
        .clone();
    let row = budget::load(root, &level_name)
        .map_err(setup)?
        .ok_or_else(|| setup(format!("level {level_name} is not in loop-budgets.tsv")))?;
    if std::env::var_os("RUSTFLAGS").is_some()
        || std::env::var_os("CARGO_ENCODED_RUSTFLAGS").is_some()
    {
        return Err(setup(
            "RUSTFLAGS is set; unset it (it would throw away every build artifact)",
        ));
    }
    let alloc = allocation(root).map_err(setup)?;
    let level = parse_level(&level_name).map_err(setup)?;
    let crate_arg = args.get(1).cloned().unwrap_or_default();
    if !matches!(level, Level::Noop | Level::L2) && crate_arg.is_empty() {
        return Err(setup(format!("{level_name} needs a crate")));
    }
    if matches!(level, Level::L1 | Level::L1d | Level::L2) && !proc::has_tool("cargo-nextest") {
        return Err(setup(
            "cargo-nextest is not installed (cargo install cargo-nextest --locked)",
        ));
    }
    let extra = args.get(2..).unwrap_or_default();
    let (cmd, cwd, crate_label) = build_command(root, level, &crate_arg, extra);
    Ok(Plan {
        root: root.to_path_buf(),
        level_name,
        level,
        crate_label,
        budget_s: row.budget_s,
        target: row.target,
        cmd,
        cwd,
        alloc,
    })
}

#[derive(Debug)]
pub struct Outcome {
    pub code: i32,
    /// The last 40 lines of the log, for a failure; empty for a pass.
    pub tail: String,
    /// The one verdict line.
    pub line: String,
    pub elapsed: Duration,
    pub rebuilt: Vec<String>,
}

#[must_use]
pub fn secs(d: Duration) -> String {
    format!("{}.{:02}", d.as_secs(), d.subsec_millis() / 10)
}

fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

pub fn execute(plan: &Plan, abort: &dyn Fn() -> bool) -> Result<Outcome, Exit> {
    let log = plan.root.join("target/loop-last.log");
    std::fs::create_dir_all(plan.root.join("target")).map_err(setup)?;
    let jobs = plan.alloc.threads.to_string();
    let spec = Spec {
        argv: plan.cmd.clone(),
        cwd: plan.cwd.clone(),
        env: vec![
            ("CARGO_BUILD_JOBS".into(), jobs.clone()),
            ("NEXTEST_TEST_THREADS".into(), jobs),
            ("CARGO_TERM_COLOR".into(), "never".into()),
        ],
        log: log.clone(),
        term_after: TERM_AFTER,
        grace: GRACE,
    };
    let ran = supervise::run(&spec, abort).map_err(setup)?;
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    let stats = logscan::scan(&text);
    let load = sys::load1();
    let facts = Facts {
        kind: plan.level.kind(),
        end: ran.end,
        lock_wait: ran.watch.build_dir,
        leaked: ran.leaked,
        group_gone: ran.group_gone,
        pgid: ran.pgid,
        stats: &stats,
        pc_wait: ran.watch.pc_wait,
        elapsed: ran.elapsed,
        budget: Duration::from_secs(plan.budget_s),
        load,
    };
    let v = verdict::decide(&facts);
    let line = format!(
        "{} {} {} s / budget {} s / target {} s / load {:.2} / tests {} / pkg-cache waits {} ({} s) / threads {} / {}",
        plan.level_name,
        plan.crate_label,
        secs(ran.elapsed),
        plan.budget_s,
        plan.target,
        load,
        stats.tests,
        ran.watch.pc_count,
        secs(ran.watch.pc_wait),
        plan.alloc.threads,
        v.text
    );
    record(plan, &stats, &ran, load, &v.text);
    let tail = if v.code == 0 {
        String::new()
    } else {
        last_lines(&text, 40)
    };
    Ok(Outcome {
        code: v.code,
        tail,
        line,
        elapsed: ran.elapsed,
        rebuilt: logscan::rebuilt(&text),
    })
}

/// Append one row to `target/loop-log.tsv` (local, never committed).
fn record(
    plan: &Plan,
    stats: &logscan::Stats,
    ran: &supervise::Ran,
    load: f64,
    verdict_text: &str,
) {
    let tsv = plan.root.join("target/loop-log.tsv");
    let header = "time\tslot\trev\tdiff\tlevel\tcrate\tthreads\tseconds\tbudget_s\tload\ttests\tpkg_waits\tpkg_wait_s\tverdict\trun\trepeat_of\n";
    let (stamp, epoch) = proc::now_stamp();
    let rev = proc::git(&plan.root, &["rev-parse", "--short", "HEAD"]).unwrap_or_default();
    let diff = std::process::Command::new("git")
        .args(["diff", "HEAD"])
        .current_dir(&plan.root)
        .output()
        .map(|o| o.stdout)
        .unwrap_or_default();
    let repeat = std::env::var("PLINTH_REPEAT_OF").unwrap_or_default();
    let row = format!(
        "{stamp}\t{}\t{rev}\t{}\t{}\t{}\t{}\t{}\t{}\t{load:.2}\t{}\t{}\t{}\t{verdict_text}\t{epoch}-{}\t{repeat}\n",
        plan.alloc.slot,
        proc::fingerprint(&plan.root, &diff),
        plan.level_name,
        plan.crate_label,
        plan.alloc.threads,
        secs(ran.elapsed),
        plan.budget_s,
        stats.tests,
        ran.watch.pc_count,
        secs(ran.watch.pc_wait),
        std::process::id()
    );
    let fresh = !tsv.exists();
    let mut text = if fresh {
        header.to_owned()
    } else {
        String::new()
    };
    text.push_str(&row);
    use std::io::Write;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&tsv);
    if let Ok(mut f) = file {
        let _ = f.write_all(text.as_bytes());
    }
}

/// Print the failure tail and the verdict line; the process exit code is the outcome's.
pub fn print(o: &Outcome) {
    if !o.tail.is_empty() {
        println!("{}", o.tail);
    }
    println!("{}", o.line);
}

pub fn main(root: &Path, args: &[String]) -> i32 {
    match prepare(root, args).and_then(|p| execute(&p, &|| false)) {
        Ok(o) => {
            print(&o);
            o.code
        }
        Err(e) => {
            eprintln!("{}", e.msg);
            e.code
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &[&str]) -> Vec<String> {
        x.iter().map(|v| (*v).to_owned()).collect()
    }

    #[test]
    fn package_names_accept_every_spelling() {
        assert_eq!(package_of("geom"), "plinth-geom");
        assert_eq!(package_of("plinth-geom"), "plinth-geom");
        assert_eq!(package_of("check"), "plinth-check");
        assert_eq!(package_of("checker"), "plinth-check");
    }

    #[test]
    fn l1_command_uses_nextest_locked_with_extra_args() {
        let (cmd, cwd, label) =
            build_command(Path::new("/r"), Level::L1, "geom", &s(&["-E", "test(x)"]));
        assert_eq!(
            cmd,
            s(&[
                "cargo",
                "nextest",
                "run",
                "-p",
                "plinth-geom",
                "--locked",
                "-E",
                "test(x)"
            ])
        );
        assert_eq!((cwd, label.as_str()), (PathBuf::from("/r"), "geom"));
    }

    #[test]
    fn l0_command_is_clippy_with_warnings_denied() {
        let (cmd, _, _) = build_command(Path::new("/r"), Level::L0, "support", &[]);
        assert_eq!(
            cmd,
            s(&[
                "cargo",
                "clippy",
                "-p",
                "plinth-support",
                "--all-targets",
                "--locked",
                "--",
                "-D",
                "warnings"
            ])
        );
    }

    #[test]
    fn checker_runs_in_its_own_workspace_with_the_shared_nextest_config() {
        let (cmd, cwd, _) = build_command(Path::new("/r"), Level::L1, "check", &[]);
        assert_eq!(cwd, PathBuf::from("/r/checker"));
        assert_eq!(
            cmd,
            s(&[
                "cargo",
                "nextest",
                "run",
                "--config-file",
                "/r/.config/nextest.toml",
                "--locked"
            ])
        );
        let (c0, _, _) = build_command(Path::new("/r"), Level::L0, "check", &[]);
        assert!(!c0.contains(&"-p".to_owned()) && c0.contains(&"clippy".to_owned()));
    }

    #[test]
    fn l2_runs_both_workspaces_and_the_doctests() {
        let (cmd, cwd, label) = build_command(Path::new("/r"), Level::L2, "", &[]);
        let sh = &cmd[2];
        assert!(sh.starts_with("cargo nextest run --workspace --locked"));
        assert!(sh.contains("cargo test --doc -p plinth-support"));
        assert!(
            sh.contains("cd checker && cargo nextest run --config-file '/r/.config/nextest.toml'")
        );
        assert_eq!((cwd, label.as_str()), (PathBuf::from("/r"), "all"));
    }

    #[test]
    fn lnoop_builds_both_workspaces() {
        let (cmd, cwd, label) = build_command(Path::new("/r"), Level::Noop, "", &[]);
        assert!(cmd[2].contains("cargo build --locked && cd checker && cargo build --locked"));
        assert_eq!((cwd, label.as_str()), (PathBuf::from("/r"), "-"));
    }

    #[test]
    fn later_levels_are_setup_errors_never_passes() {
        for l in ["L3", "L4", "L5"] {
            assert!(parse_level(l).unwrap_err().contains("not available yet"));
        }
        assert!(parse_level("L9").is_err());
        assert_eq!(parse_level("L1d"), Ok(Level::L1d));
        assert_eq!(parse_level("L2"), Ok(Level::L2));
    }

    #[test]
    fn marker_threads_parses_the_slot_marker() {
        assert_eq!(
            marker_threads("lane=lane/x\nthreads=3\nbase=abc\n"),
            Some(3)
        );
        assert_eq!(marker_threads("threads=0\n"), None);
        assert_eq!(marker_threads("lane=x\n"), None);
    }

    #[test]
    fn seconds_format_truncates_to_hundredths() {
        assert_eq!(secs(Duration::from_millis(3456)), "3.45");
        assert_eq!(secs(Duration::from_millis(50)), "0.05");
    }

    #[test]
    fn failure_tail_keeps_the_last_lines() {
        let text: String = (1..=100).map(|i| format!("l{i}\n")).collect();
        let tail = last_lines(&text, 40);
        assert_eq!(tail.lines().count(), 40);
        assert!(tail.starts_with("l61") && tail.ends_with("l100"));
    }
}
