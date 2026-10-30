//! `devloop slot`: fixed worktree slots for parallel agents (design dev-loop.md sections 5.5, 5.6).
//!
//!   slot init                           make the worktrees s1 s2 s3 bench (safe to repeat)
//!   slot status                         one line per slot
//!   slot <s1|s2|s3> start lane/<name> [--threads N]
//!   slot <s1|s2|s3> end                 retire after the orchestrator merged the lane
//!
//! Slots live in `../plinth-slots/` next to the main checkout. Exit codes: 0 ok; 4 setup error
//! or refusal.

use crate::proc::{self, Exit};
use std::path::{Path, PathBuf};
use std::time::Duration;

const ALL: [&str; 4] = ["s1", "s2", "s3", "bench"];
const WRITING: [&str; 3] = ["s1", "s2", "s3"];
/// The owner's cap: all slots plus main together.
const TOTAL_THREADS: u32 = 8;
const WARN_KB: u64 = 12 * 1024 * 1024;

fn die(msg: impl std::fmt::Display) -> Exit {
    Exit::setup(format!("SLOT: {msg}"))
}

fn g(dir: &Path, args: &[&str]) -> Result<String, Exit> {
    proc::git(dir, args).map_err(die)
}

struct Ctx {
    root: PathBuf,
    slots_dir: PathBuf,
    main_branch: String,
    main_threads: u32,
}

impl Ctx {
    fn dir(&self, slot: &str) -> PathBuf {
        self.slots_dir.join(slot)
    }

    fn marker(&self, slot: &str) -> Result<PathBuf, Exit> {
        Ok(
            PathBuf::from(g(&self.dir(slot), &["rev-parse", "--absolute-git-dir"])?)
                .join("slot-active"),
        )
    }

    fn is_active(&self, slot: &str) -> bool {
        self.dir(slot).join(".git").exists() && self.marker(slot).is_ok_and(|m| m.exists())
    }

    fn threads_of(&self, slot: &str) -> u32 {
        let text = self
            .marker(slot)
            .ok()
            .and_then(|m| std::fs::read_to_string(m).ok())
            .unwrap_or_default();
        crate::t::marker_threads(&text).unwrap_or(0)
    }
}

fn context(here: &Path) -> Result<Ctx, Exit> {
    // The main checkout is the first worktree git lists, so this behaves the same when it runs
    // from a slot's copy of the scripts.
    let list = g(here, &["worktree", "list", "--porcelain"])?;
    let root = list
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("worktree "))
        .map(PathBuf::from)
        .ok_or_else(|| die("cannot find the main checkout"))?;
    let slots_dir = root
        .parent()
        .ok_or_else(|| die("the main checkout has no parent directory"))?
        .join("plinth-slots");
    let main_branch = match std::env::var("PLINTH_MAIN") {
        Ok(b) if !b.is_empty() => b,
        _ => proc::git(&root, &["symbolic-ref", "--short", "-q", "HEAD"])
            .map_err(|_| die("the main checkout has a detached HEAD; set PLINTH_MAIN=<branch>"))?,
    };
    let main_threads = proc::env_count("PLINTH_MAIN_THREADS", 2).map_err(die)?;
    Ok(Ctx {
        root,
        slots_dir,
        main_branch,
        main_threads,
    })
}

/// Processes whose current directory is inside `dir`. Fails if lsof cannot answer: "cannot tell"
/// must never read as "nobody".
fn running_in(dir: &Path) -> Result<Vec<String>, Exit> {
    let all = proc::out("lsof", &["-d", "cwd", "-Fpn"], Path::new("/")).map_err(|e| {
        die(format!(
            "lsof gave no answer ({e}); cannot tell whether commands still run in the slot"
        ))
    })?;
    if all.is_empty() {
        return Err(die(
            "lsof gave no answer; cannot tell whether commands still run in the slot",
        ));
    }
    Ok(pids_in(&all, &dir.display().to_string()))
}

/// Parse `lsof -Fpn` output: pids whose `n` path is `dir` or inside it.
fn pids_in(lsof: &str, dir: &str) -> Vec<String> {
    let prefix = format!("{dir}/");
    let (mut pid, mut found) = (String::new(), Vec::new());
    for line in lsof.lines() {
        if let Some(p) = line.strip_prefix('p') {
            pid = p.to_owned();
        } else if let Some(n) = line.strip_prefix('n')
            && format!("{n}/").starts_with(&prefix)
            && !found.contains(&pid)
        {
            found.push(pid.clone());
        }
    }
    found
}

fn kb_of(paths: &[PathBuf]) -> u64 {
    paths
        .iter()
        .filter(|p| p.exists())
        .filter_map(|p| proc::out("du", &["-sk", &p.display().to_string()], Path::new("/")).ok())
        .filter_map(|o| {
            o.split_whitespace()
                .next()
                .and_then(|n| n.parse::<u64>().ok())
        })
        .sum()
}

fn disk_check(c: &Ctx, slot: &str) {
    let d = c.dir(slot);
    let kb = kb_of(&[d.join("target"), d.join("checker/target")]);
    println!("{slot} build folders: {} MB", kb / 1024);
    if kb > WARN_KB {
        eprintln!("SLOT WARNING: {slot} build folders are over 12 GB");
        if proc::has_tool("cargo-sweep") {
            let _ = proc::out("cargo", &["sweep", "--time", "7"], &d);
            let _ = proc::out("cargo", &["sweep", "--time", "7"], &d.join("checker"));
        } else {
            eprintln!("SLOT WARNING: cargo-sweep is not installed; run: cargo install cargo-sweep");
        }
    }
}

fn cmd_init(c: &Ctx) -> Result<(), Exit> {
    std::fs::create_dir_all(&c.slots_dir).map_err(die)?;
    for s in ALL {
        let d = c.dir(s);
        if !d.join(".git").exists() {
            let ds = d.display().to_string();
            g(
                &c.root,
                &["worktree", "add", "--detach", &ds, &c.main_branch],
            )?;
            println!(
                "{s}: created at {}",
                g(&d, &["rev-parse", "--short", "HEAD"])?
            );
            continue;
        }
        let busy = c.is_active(s)
            || !g(&d, &["status", "--porcelain"])?.is_empty()
            || proc::git_ok(&d, &["symbolic-ref", "-q", "HEAD"]);
        if busy {
            println!("{s}: exists (left as it is)");
        } else if g(&d, &["rev-parse", "HEAD"])? == g(&c.root, &["rev-parse", &c.main_branch])? {
            println!("{s}: exists, at {}", c.main_branch);
        } else {
            g(&d, &["switch", "--detach", &c.main_branch])?;
            println!(
                "{s}: moved to {}",
                g(&d, &["rev-parse", "--short", "HEAD"])?
            );
        }
    }
    Ok(())
}

fn status_line(c: &Ctx, s: &str) -> Result<String, Exit> {
    let d = c.dir(s);
    if !d.join(".git").exists() {
        return Ok(format!("{s}: MISSING (run scripts/slot init)"));
    }
    let br = proc::git(&d, &["symbolic-ref", "--short", "-q", "HEAD"]).unwrap_or_else(|_| {
        format!(
            "detached@{}",
            proc::git(&d, &["rev-parse", "--short", "HEAD"]).unwrap_or_default()
        )
    });
    let dirty = if g(&d, &["status", "--porcelain"])?.is_empty() {
        "clean"
    } else {
        "DIRTY"
    };
    let act = if c.is_active(s) {
        format!("active threads={}", c.threads_of(s))
    } else {
        "idle".into()
    };
    Ok(format!("{s}: {br} {dirty} {act}"))
}

fn cmd_status(c: &Ctx) -> Result<(), Exit> {
    for s in ALL {
        println!("{}", status_line(c, s)?);
    }
    Ok(())
}

/// Parse `[--threads N]`; the default is 2.
fn parse_threads(opts: &[String]) -> Result<u32, String> {
    let mut threads = 2;
    let mut it = opts.iter();
    while let Some(o) = it.next() {
        if o != "--threads" {
            return Err(format!("unknown option: {o}"));
        }
        threads = it
            .next()
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|n| *n > 0)
            .ok_or("--threads must be a positive integer")?;
    }
    Ok(threads)
}

/// Would `new` threads fit beside main and the `active` allocations?
fn fits(main: u32, active: &[u32], new: u32) -> Result<(), String> {
    let used: u32 = active.iter().sum();
    if main + used + new <= TOTAL_THREADS {
        Ok(())
    } else {
        Err(format!(
            "no thread allocation left: main {main} + active slots {used} + new {new} > {TOTAL_THREADS}"
        ))
    }
}

fn require_writing(s: &str) -> Result<(), Exit> {
    if WRITING.contains(&s) {
        Ok(())
    } else {
        Err(die(format!("{s} is not a writing slot (use s1, s2 or s3)")))
    }
}

fn cmd_start(c: &Ctx, s: &str, rest: &[String]) -> Result<(), Exit> {
    require_writing(s)?;
    let lane = rest.first().ok_or_else(|| {
        die(format!(
            "usage: scripts/slot {s} start lane/<name> [--threads N]"
        ))
    })?;
    let threads = parse_threads(&rest[1..]).map_err(die)?;
    if !lane.starts_with("lane/") || lane.len() <= 5 {
        return Err(die("branch must be named lane/<name>"));
    }
    let d = c.dir(s);
    if !d.join(".git").exists() {
        return Err(die(format!("{s} is missing; run scripts/slot init")));
    }
    if c.is_active(s) {
        return Err(die(format!("{s} already has an active lane")));
    }
    if !g(&d, &["status", "--porcelain"])?.is_empty() {
        return Err(die(format!(
            "{s} is dirty; commit or clean it first (nothing was changed)"
        )));
    }
    let active: Vec<u32> = WRITING
        .iter()
        .filter(|o| c.is_active(o))
        .map(|o| c.threads_of(o))
        .collect();
    fits(c.main_threads, &active, threads).map_err(die)?;
    // Reserve first, then switch; undo the reservation if the switch fails. The caller holds the
    // exclusive slot lock, so no other start sees a half state.
    let marker = c.marker(s)?;
    let base = g(&c.root, &["rev-parse", "--short", &c.main_branch])?;
    std::fs::write(
        &marker,
        format!("lane={lane}\nthreads={threads}\nbase={base}\n"),
    )
    .map_err(die)?;
    if let Err(e) = proc::git(&d, &["switch", "-c", lane, &c.main_branch]) {
        let _ = std::fs::remove_file(&marker);
        return Err(die(format!(
            "{s}: could not create {lane} (allocation released): {e}"
        )));
    }
    println!("{s}: started {lane} with {threads} threads");
    disk_check(c, s);
    Ok(())
}

fn cmd_end(c: &Ctx, s: &str) -> Result<(), Exit> {
    require_writing(s)?;
    let d = c.dir(s);
    if !c.is_active(s) {
        return Err(die(format!("{s} has no active lane")));
    }
    let marker = c.marker(s)?;
    let text = std::fs::read_to_string(&marker).map_err(die)?;
    let lane = text
        .lines()
        .find_map(|l| l.strip_prefix("lane="))
        .ok_or_else(|| die("the slot marker has no lane"))?
        .to_owned();
    let pids = running_in(&d)?;
    if !pids.is_empty() {
        return Err(die(format!(
            "processes still run in {s} (pids: {}); stop retirement, nothing was changed",
            pids.join(" ")
        )));
    }
    if !g(&d, &["status", "--porcelain"])?.is_empty() {
        return Err(die(format!("{s} is dirty; nothing was changed")));
    }
    if !proc::git_ok(&d, &["merge-base", "--is-ancestor", &lane, &c.main_branch]) {
        return Err(die(format!(
            "{lane} is not merged into {}; nothing was changed",
            c.main_branch
        )));
    }
    g(&d, &["switch", "--detach", &c.main_branch])?;
    g(&d, &["branch", "-d", &lane])?;
    let _ = std::fs::remove_file(&marker);
    println!(
        "{s}: retired {lane}; now detached at {}",
        g(&d, &["rev-parse", "--short", "HEAD"])?
    );
    Ok(())
}

fn dispatch(c: &Ctx, args: &[String]) -> Result<(), Exit> {
    let first = args.first().map_or("", String::as_str);
    match first {
        "init" => cmd_init(c),
        "status" => cmd_status(c),
        s if ALL.contains(&s) => match args.get(1).map(String::as_str) {
            Some("start") => cmd_start(c, s, &args[2..]),
            Some("end") => cmd_end(c, s),
            Some(v) => Err(die(format!("unknown verb: {v}"))),
            None => Err(die(format!("usage: scripts/slot {s} start|end"))),
        },
        "" => Err(die(
            "usage: scripts/slot init | status | <slot> start lane/<name> [--threads N] | <slot> end",
        )),
        other => Err(die(format!("unknown command or slot: {other}"))),
    }
}

/// Every command that changes slots, or reads the allocation to decide, runs under one exclusive
/// kernel lock (section 5.6): the check and the reservation are one critical section.
pub fn main(here: &Path, args: &[String]) -> i32 {
    let result = context(here).and_then(|c| {
        let _lock = if args.first().is_some_and(|a| a == "status") {
            None
        } else {
            std::fs::create_dir_all(&c.slots_dir).map_err(die)?;
            let f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(c.slots_dir.join(".slot.lock"))
                .map_err(|e| die(format!("cannot open the lock file: {e}")))?;
            if !crate::sys::lock_exclusive(&f, Duration::from_secs(60)) {
                return Err(die(
                    "timed out after 60 s waiting for another scripts/slot command",
                ));
            }
            Some(f)
        };
        dispatch(&c, args)
    });
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{}", e.msg);
            e.code
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation_never_exceeds_eight() {
        assert!(fits(2, &[3], 3).is_ok());
        assert!(fits(2, &[3, 3], 1).is_err());
        assert!(fits(2, &[2, 2], 2).is_ok());
        assert!(fits(2, &[2, 2], 3).is_err());
    }

    #[test]
    fn threads_option_parses_and_rejects() {
        assert_eq!(parse_threads(&[]), Ok(2));
        assert_eq!(parse_threads(&["--threads".into(), "3".into()]), Ok(3));
        assert!(parse_threads(&["--threads".into(), "0".into()]).is_err());
        assert!(parse_threads(&["--threads".into()]).is_err());
        assert!(parse_threads(&["--bogus".into()]).is_err());
    }

    #[test]
    fn lsof_output_maps_to_pids_inside_the_slot_only() {
        let out = "p100\nn/work/slots/s1\np200\nn/work/slots/s1/crates/x\np300\nn/work/slots/s10\np400\nn/work/slots/s2\n";
        assert_eq!(pids_in(out, "/work/slots/s1"), vec!["100", "200"]);
        assert!(pids_in(out, "/work/slots/s3").is_empty());
    }

    #[test]
    fn only_writing_slots_start() {
        assert!(require_writing("s2").is_ok());
        assert!(require_writing("bench").is_err());
    }
}
