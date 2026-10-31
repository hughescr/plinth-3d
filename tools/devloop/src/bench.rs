//! `devloop loop-bench`: the edit-loop benchmark (design dev-loop.md section 3.3). Bench slot only,
//! in an exclusive epoch. Run it in the background: it takes minutes.
//!
//! For each case: one untimed settle run, then 3 repetitions. Before every repetition a fresh
//! edit changes the canary value of the edited crate, and the very next command is the timed
//! `t`. A repetition that rebuilds nothing is invalid. Prints every run and the maximum per case;
//! exit 0 only if every maximum is in budget and every repetition is valid.
//! Exit: 0 pass; 1 over budget, invalid repetition or a failed command; 4 setup error; 5 stopped.

use crate::budget;
use crate::identity;
use crate::proc::{self, Exit};
use crate::sys;
use crate::t::{self, Outcome};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const REPS: u64 = 3;
/// One `t` invocation: TERM at 115 s, KILL 5 s later.
const CALL_LIMIT_S: u64 = 120;
/// (level, crate to time, crate whose canary is edited)
const CASES: [(&str, &str, &str); 6] = [
    ("L0", "support", "support"), // clippy after a support edit
    ("L1", "support", "support"), // one crate
    ("L1", "geom", "geom"),       // the root crate
    ("L1d", "support", "geom"),   // geometry-to-dependent
    ("L1d", "cli", "support"),    // support-to-CLI: support, emit and cli rebuild
    ("L1", "check", "check"),     // checker-only
];

/// Aggregate deadline derived from the case list: every case runs reps + 1 bounded commands,
/// plus two Lnoop runs, plus 60 s of cleanup.
#[must_use]
pub fn aggregate_deadline_s() -> u64 {
    (CASES.len() as u64 * (REPS + 1) + 2) * CALL_LIMIT_S + 60
}

fn die(msg: impl std::fmt::Display) -> Exit {
    Exit::setup(format!("LOOP-BENCH SETUP ERROR: {msg}"))
}

fn canary_path(root: &Path, edited: &str) -> PathBuf {
    if edited == "check" {
        root.join("checker/src/loop_canary.rs")
    } else {
        root.join(format!("crates/{edited}/src/loop_canary.rs"))
    }
}

/// Replace the canary value line (four spaces and digits) with `new`. Returns the old value and
/// the new text, or None if the file has no such line.
#[must_use]
pub fn edit_canary(text: &str, new: u64) -> Option<(String, String)> {
    let mut old = None;
    let lines: Vec<String> = text
        .split('\n')
        .map(|l| match l.strip_prefix("    ") {
            Some(n) if old.is_none() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => {
                old = Some(n.to_owned());
                format!("    {new}")
            }
            _ => l.to_owned(),
        })
        .collect();
    old.map(|o| (o, lines.join("\n")))
}

struct Bench {
    root: PathBuf,
    fail: bool,
    seq: u64,
}

impl Bench {
    /// One `t` call, stoppable; setup errors become exit 4 with their message as the line.
    fn run_t(&self, args: &[&str], deadline: Instant) -> Outcome {
        let argv: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        let abort = move || Instant::now() >= deadline;
        match t::prepare(&self.root, &argv).and_then(|p| t::execute(&p, &abort)) {
            Ok(o) => o,
            Err(e) => Outcome {
                code: e.code,
                tail: String::new(),
                line: e.msg,
                elapsed: Duration::ZERO,
                rebuilt: vec![],
            },
        }
    }

    fn lnoop(&mut self, deadline: Instant) {
        self.run_t(&["Lnoop"], deadline);
        let o = self.run_t(&["Lnoop"], deadline);
        if o.code == 0 {
            println!("Lnoop (settled, no edit): {}", o.line);
        } else {
            println!("Lnoop FAILED: {}", o.line);
            self.fail = true;
        }
    }

    fn rep(
        &mut self,
        case: (&str, &str, &str),
        file: &Path,
        deadline: Instant,
        n: u64,
    ) -> Result<(String, bool, f64), Exit> {
        let (level, krate, _) = case;
        let text = std::fs::read_to_string(file).map_err(die)?;
        self.seq += 1;
        let new = proc::now_stamp().1 + self.seq;
        let (old, edited) = edit_canary(&text, new)
            .ok_or_else(|| die(format!("no canary value line in {}", file.display())))?;
        std::fs::write(file, edited).map_err(die)?;
        let o = self.run_t(&[level, krate], deadline); // the next command after the edit
        let sec = t::secs(o.elapsed);
        let built = o.rebuilt.join(" ");
        let valid = !o.rebuilt.is_empty() && matches!(o.code, 0 | 3);
        let state = if o.rebuilt.is_empty() {
            "INVALID".to_owned()
        } else if matches!(o.code, 0 | 3) {
            "ok".to_owned()
        } else {
            format!("FAILED(exit {})", o.code)
        };
        let rest = o.line.rsplit(" / ").next().unwrap_or("");
        let shown = if built.is_empty() { "nothing" } else { &built };
        println!(
            "{level} {krate} rep {}: {sec} s  edit {} {old} -> {new}  rebuilt: {shown}  [{state}] {rest}",
            n + 1,
            file.display()
        );
        Ok((sec, valid, o.elapsed.as_secs_f64()))
    }

    fn case(&mut self, case: (&str, &str, &str), deadline: Instant) -> Result<(), Exit> {
        let (level, krate, edited) = case;
        let file = canary_path(&self.root, edited);
        let budget = budget::load(&self.root, level)
            .map_err(die)?
            .ok_or_else(|| die(format!("no budget row for {level}")))?
            .budget_s;
        let settle = self.run_t(&[level, krate], deadline); // untimed; its result is not reported
        if !matches!(settle.code, 0 | 3) {
            println!(
                "{level} {krate} settle run failed (exit {}): {}",
                settle.code, settle.line
            );
            self.fail = true;
            return Ok(());
        }
        let backup = std::fs::read_to_string(&file).map_err(die)?;
        let reps = self.reps(case, &file, deadline);
        std::fs::write(&file, backup).map_err(die)?; // restore, whatever happened
        let (times, max, invalid) = reps?;
        let bad = max > budget as f64 || invalid;
        let verdict = if invalid {
            "FAIL (invalid or failed repetition)"
        } else if bad {
            "FAIL (maximum over budget)"
        } else {
            "PASS"
        };
        self.fail |= bad;
        println!(
            "{level} {krate}: runs {} / maximum {max:.2} s / budget {budget} s / {verdict}",
            times.join(" ")
        );
        Ok(())
    }

    fn reps(
        &mut self,
        case: (&str, &str, &str),
        file: &Path,
        deadline: Instant,
    ) -> Result<(Vec<String>, f64, bool), Exit> {
        let (mut times, mut max, mut invalid) = (vec![], 0.0_f64, false);
        for n in 0..REPS {
            let (sec, valid, exact) = self.rep(case, file, deadline, n)?;
            times.push(sec);
            max = max.max(exact);
            invalid |= !valid;
        }
        Ok((times, max, invalid))
    }
}

fn preconditions(root: &Path) -> Result<(), Exit> {
    let want = aggregate_deadline_s();
    let has = budget::load(root, "Bench")
        .map_err(die)?
        .map(|r| r.budget_s);
    if has != Some(want) {
        return Err(die(format!(
            "loop-budgets.tsv Bench row must have budget_s {want} (has '{}')",
            has.map_or_else(|| "none".into(), |h| h.to_string())
        )));
    }
    if root.file_name().is_none_or(|n| n != "bench") {
        return Err(die("run it in the bench slot (plinth-slots/bench)"));
    }
    if !proc::git(root, &["status", "--porcelain"])
        .map_err(die)?
        .is_empty()
    {
        return Err(die("the bench worktree is not clean"));
    }
    Ok(())
}

pub fn main(root: &Path) -> i32 {
    if let Err(e) = preconditions(root) {
        eprintln!("{}", e.msg);
        return e.code;
    }
    let begin = Instant::now();
    let deadline = begin + Duration::from_secs(aggregate_deadline_s());
    let mut b = Bench {
        root: root.to_path_buf(),
        fail: false,
        seq: 0,
    };
    b.lnoop(deadline);
    let mut error = None;
    for case in CASES {
        if sys::stop_requested() || Instant::now() >= deadline {
            break;
        }
        if let Err(e) = b.case(case, deadline) {
            error = Some(e);
            break;
        }
    }
    let clean = proc::git(root, &["status", "--porcelain"]).is_ok_and(|s| s.is_empty());
    println!(
        "bench duration {} s (not gate time); tree: {}",
        begin.elapsed().as_secs(),
        if clean { "restored" } else { "DIRTY" }
    );
    if let Some(e) = error {
        eprintln!("{}", e.msg);
        return e.code;
    }
    if sys::stop_requested() || Instant::now() >= deadline {
        println!("LOOP-BENCH: stopped");
        return 5;
    }
    let pass = !(b.fail || !clean);
    println!(
        "{}",
        if pass {
            "LOOP-BENCH PASS"
        } else {
            "LOOP-BENCH FAIL"
        }
    );
    if let Err(e) = write_record(root, pass) {
        eprintln!("LOOP-BENCH: cannot write the bench record: {e}");
        return 1;
    }
    i32::from(!pass)
}

/// Append this run to the bench record (read by `gate`, step 5 of design section 9).
fn write_record(root: &Path, pass: bool) -> Result<(), String> {
    use std::io::Write;
    let id = identity::current(root)?;
    let (stamp, _) = proc::now_stamp();
    let row = id.row(&stamp, if pass { "pass" } else { "fail" });
    let path = identity::record_path(root)?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(row.as_bytes()))
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CANARY: &str = "/// doc\n#[inline(never)]\npub fn loop_canary() -> u32 {\n    1\n}\n";

    #[test]
    fn edit_replaces_only_the_value_line() {
        let (old, text) = edit_canary(CANARY, 1234).expect("has a value line");
        assert_eq!(old, "1");
        assert_eq!(text, CANARY.replace("    1\n", "    1234\n"));
        let (old2, _) = edit_canary(&text, 5).expect("again");
        assert_eq!(old2, "1234");
    }

    #[test]
    fn a_file_without_a_value_line_is_refused() {
        assert!(edit_canary("fn x() {\n    let a = 1;\n}\n", 7).is_none());
    }

    #[test]
    fn deadline_matches_the_committed_budget_row() {
        // 6 cases x (3 reps + 1 settle) + 2 Lnoop = 26 commands of 120 s, plus 60 s.
        assert_eq!(aggregate_deadline_s(), 3180);
    }
}
