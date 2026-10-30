//! The verdict of one `t` run, as a pure function of the facts, so every failure rule is
//! unit-tested. Order matters: the first rule that fires wins.

use crate::logscan::Stats;
use crate::supervise::End;
use std::time::Duration;

/// Package-cache wait allowed in one command.
pub const PC_LIMIT: Duration = Duration::from_secs(2);

const LOCK_WAIT: &str = "LOCK WAIT: another cargo process is using this slot's build folder";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// No edit: both builds must finish and rebuild nothing.
    Noop,
    /// Clippy.
    Check,
    /// Tests: a positive selected-test count is required.
    Test,
}

pub struct Facts<'a> {
    pub kind: Kind,
    pub end: End,
    pub lock_wait: bool,
    pub leaked: bool,
    pub group_gone: bool,
    pub pgid: i32,
    pub stats: &'a Stats,
    pub pc_wait: Duration,
    pub elapsed: Duration,
    pub budget: Duration,
    pub load: f64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Verdict {
    pub text: String,
    pub code: i32,
}

fn v(text: impl Into<String>, code: i32) -> Verdict {
    Verdict {
        text: text.into(),
        code,
    }
}

fn command_failed(f: &Facts, rc: i32) -> Verdict {
    if f.stats.no_tests_msg {
        v("FAIL: NO TESTS SELECTED", 1)
    } else {
        v(format!("FAIL (exit {rc})"), 1)
    }
}

fn level_checks(f: &Facts) -> Option<Verdict> {
    let s = f.stats;
    match f.kind {
        Kind::Noop if s.compiled != 0 => Some(v(
            format!("FAIL: {} Compiling lines on a no-change build", s.compiled),
            1,
        )),
        Kind::Noop if s.finished < 2 => Some(v("FAIL: a build did not finish", 1)),
        Kind::Check if s.finished == 0 => Some(v("FAIL: clippy did not finish", 1)),
        Kind::Test if s.tests == 0 => Some(v("FAIL: NO TESTS SELECTED", 1)),
        _ => None,
    }
}

#[must_use]
pub fn decide(f: &Facts) -> Verdict {
    if f.lock_wait || f.end == End::LockWait {
        return v(LOCK_WAIT, 1);
    }
    let rc = match f.end {
        End::Interrupted => return v("INTERRUPTED: command stopped", 5),
        End::TimedOut => {
            let gone = if f.group_gone { "yes" } else { "NO" };
            return v(
                format!(
                    "TIMEOUT: stopped at the 120 s bound; process group {} gone: {gone}",
                    f.pgid
                ),
                5,
            );
        }
        End::LockWait => return v(LOCK_WAIT, 1),
        End::Exited(rc) => rc,
    };
    if rc != 0 {
        return command_failed(f, rc);
    }
    if f.leaked {
        let gone = if f.group_gone { "yes" } else { "NO" };
        return v(
            format!("FAIL: the command left processes behind (killed; group gone: {gone})"),
            1,
        );
    }
    if let Some(bad) = level_checks(f) {
        return bad;
    }
    if f.pc_wait > PC_LIMIT {
        return v("FAIL: PACKAGE-CACHE WAIT over 2 s", 1);
    }
    if f.elapsed > f.budget {
        let loaded = if f.load > 8.0 {
            format!(" (LOADED: load {:.2} is above 8)", f.load)
        } else {
            String::new()
        };
        return v(format!("LOOP BUDGET EXCEEDED{loaded}"), 3);
    }
    if f.kind != Kind::Noop && f.stats.touched == 0 {
        return v("NOCHANGE (nothing rebuilt; not an edit-loop result)", 0);
    }
    v("PASS", 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(compiled: u32, touched: u32, finished: u32, tests: u32) -> Stats {
        Stats {
            compiled,
            touched,
            finished,
            tests,
            skipped: 0,
            no_tests_msg: false,
        }
    }

    fn facts(kind: Kind, end: End, stats: &Stats) -> Facts<'_> {
        Facts {
            kind,
            end,
            lock_wait: false,
            leaked: false,
            group_gone: true,
            pgid: 42,
            stats,
            pc_wait: Duration::ZERO,
            elapsed: Duration::from_secs(3),
            budget: Duration::from_secs(15),
            load: 1.0,
        }
    }

    #[test]
    fn a_clean_test_run_passes() {
        let s = stats(2, 2, 1, 4);
        assert_eq!(decide(&facts(Kind::Test, End::Exited(0), &s)), v("PASS", 0));
    }

    #[test]
    fn zero_tests_run_is_a_failure_even_with_exit_zero() {
        let s = stats(1, 1, 1, 0);
        let r = decide(&facts(Kind::Test, End::Exited(0), &s));
        assert_eq!((r.text.as_str(), r.code), ("FAIL: NO TESTS SELECTED", 1));
    }

    #[test]
    fn nextest_no_tests_exit_is_reported_as_no_tests() {
        let mut s = stats(0, 0, 1, 0);
        s.no_tests_msg = true;
        assert_eq!(
            decide(&facts(Kind::Test, End::Exited(4), &s)).text,
            "FAIL: NO TESTS SELECTED"
        );
    }

    #[test]
    fn budget_miss_is_exit_3_even_when_tests_pass() {
        let s = stats(1, 1, 1, 4);
        let mut f = facts(Kind::Test, End::Exited(0), &s);
        f.elapsed = Duration::from_secs(16);
        assert_eq!(decide(&f), v("LOOP BUDGET EXCEEDED", 3));
        f.load = 9.5;
        assert!(decide(&f).text.contains("LOADED: load 9.50"));
    }

    #[test]
    fn timeout_is_exit_5_and_reports_the_group() {
        let s = stats(0, 0, 0, 0);
        let mut f = facts(Kind::Test, End::TimedOut, &s);
        assert_eq!(decide(&f).code, 5);
        assert!(decide(&f).text.contains("process group 42 gone: yes"));
        f.group_gone = false;
        assert!(decide(&f).text.ends_with("gone: NO"));
    }

    #[test]
    fn a_build_directory_lock_wait_fails_first() {
        let s = stats(0, 0, 0, 0);
        let mut f = facts(Kind::Test, End::LockWait, &s);
        assert!(decide(&f).text.starts_with("LOCK WAIT"));
        f.end = End::Exited(0);
        f.lock_wait = true;
        assert_eq!(decide(&f).code, 1);
    }

    #[test]
    fn package_cache_wait_up_to_two_seconds_passes_and_over_fails() {
        let s = stats(1, 1, 1, 4);
        let mut f = facts(Kind::Test, End::Exited(0), &s);
        f.pc_wait = Duration::from_millis(2000);
        assert_eq!(decide(&f).text, "PASS");
        f.pc_wait = Duration::from_millis(2001);
        assert_eq!(decide(&f).text, "FAIL: PACKAGE-CACHE WAIT over 2 s");
    }

    #[test]
    fn noop_needs_zero_compiles_and_two_finished_builds() {
        let ok = stats(0, 0, 2, 0);
        assert_eq!(decide(&facts(Kind::Noop, End::Exited(0), &ok)).text, "PASS");
        let rebuilt = stats(1, 1, 2, 0);
        assert!(
            decide(&facts(Kind::Noop, End::Exited(0), &rebuilt))
                .text
                .contains("1 Compiling lines")
        );
        let short = stats(0, 0, 1, 0);
        assert_eq!(
            decide(&facts(Kind::Noop, End::Exited(0), &short)).text,
            "FAIL: a build did not finish"
        );
    }

    #[test]
    fn nothing_rebuilt_is_nochange_not_pass_for_edit_levels() {
        let s = stats(0, 0, 1, 4);
        assert!(
            decide(&facts(Kind::Test, End::Exited(0), &s))
                .text
                .starts_with("NOCHANGE")
        );
        let c = stats(0, 0, 1, 0);
        assert!(
            decide(&facts(Kind::Check, End::Exited(0), &c))
                .text
                .starts_with("NOCHANGE")
        );
    }

    #[test]
    fn leftover_processes_fail() {
        let s = stats(1, 1, 1, 4);
        let mut f = facts(Kind::Test, End::Exited(0), &s);
        f.leaked = true;
        assert!(
            decide(&f)
                .text
                .starts_with("FAIL: the command left processes behind")
        );
    }

    #[test]
    fn a_failing_command_is_exit_1() {
        let s = stats(1, 1, 1, 4);
        assert_eq!(
            decide(&facts(Kind::Test, End::Exited(100), &s)),
            v("FAIL (exit 100)", 1)
        );
    }

    #[test]
    fn clippy_that_never_finished_fails() {
        let s = stats(0, 1, 0, 0);
        assert_eq!(
            decide(&facts(Kind::Check, End::Exited(0), &s)).text,
            "FAIL: clippy did not finish"
        );
    }
}
