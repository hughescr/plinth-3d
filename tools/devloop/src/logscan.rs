//! Facts read from a finished command log: what was rebuilt and how many tests ran.

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// `Compiling` lines.
    pub compiled: u32,
    /// `Compiling` plus `Checking` lines.
    pub touched: u32,
    /// `Finished` lines.
    pub finished: u32,
    /// Tests run, summed over every nextest summary line (L2 runs two workspaces; 0 when
    /// there is none).
    pub tests: u32,
    /// Tests nextest left out (`N skipped` on its summary lines), for any reason. Outside the
    /// bench epoch this includes the `threads_bench_*` tests the profile's `default-filter`
    /// drops; the log does not split the two.
    pub skipped: u32,
    /// nextest said it had no tests to run.
    pub no_tests_msg: bool,
    /// Lines that start with [`UNVERIFIED_MARK`]: things a test could not check against a
    /// native file and refused, rather than pass on a guess (design section 3, 6.2).
    pub unverified: u32,
}

/// A test that meets something no native file settles prints one line that starts with this
/// text and then the kind and the reason. `.config/nextest.toml` keeps the output of the
/// `native_unverified_*` tests (a passing test's output is otherwise dropped) so the runner
/// can count the lines and print `unverified N` on the verdict line.
pub const UNVERIFIED_MARK: &str = "PLINTH-UNVERIFIED:";

/// The cargo status verb of a line (`Compiling`, `Finished`, ...), if it is one.
fn verb(line: &str) -> Option<&str> {
    line.split_whitespace().next()
}

/// `Summary [   0.004s] 12 tests run: ...` gives 12.
fn summary_tests(line: &str) -> Option<u32> {
    let rest = line.split_once("Summary [")?.1.split_once("] ")?.1;
    let (num, tail) = rest.split_once(' ')?;
    let n: u32 = num.parse().ok()?;
    (tail.starts_with("test run") || tail.starts_with("tests run")).then_some(n)
}

/// `... 12 passed, 3 skipped` on a summary line gives 3.
fn summary_skipped(line: &str) -> Option<u32> {
    line.split_once("Summary [")?;
    let head = line.split_once(" skipped")?.0;
    head.rsplit(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

#[must_use]
pub fn scan(log: &str) -> Stats {
    let mut s = Stats::default();
    for line in log.lines() {
        match verb(line) {
            Some("Compiling") => {
                s.compiled += 1;
                s.touched += 1;
            }
            Some("Checking") => s.touched += 1,
            Some("Finished") => s.finished += 1,
            _ => {}
        }
        if let Some(n) = summary_tests(line) {
            s.tests = s.tests.saturating_add(n);
            s.skipped = s.skipped.saturating_add(summary_skipped(line).unwrap_or(0));
        }
        if line.contains("no tests to run") {
            s.no_tests_msg = true;
        }
        if line.trim_start().starts_with(UNVERIFIED_MARK) {
            s.unverified = s.unverified.saturating_add(1);
        }
    }
    s
}

/// Package names of the `Compiling` and `Checking` lines, in order.
#[must_use]
pub fn rebuilt(log: &str) -> Vec<String> {
    log.lines()
        .filter(|l| matches!(verb(l), Some("Compiling" | "Checking")))
        .filter_map(|l| l.split_whitespace().nth(1).map(str::to_owned))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "   Compiling plinth-geom v0.0.0 (/x)\n   Checking plinth-support v0.0.0 (/x)\n\
        ------------\n     Summary [   0.004s] 3 tests run: 3 passed, 0 skipped\n    Finished `dev` profile in 1s\n";

    #[test]
    fn counts_verbs_and_tests() {
        let s = scan(LOG);
        assert_eq!((s.compiled, s.touched, s.finished, s.tests), (1, 2, 1, 3));
        assert!(!s.no_tests_msg);
    }

    #[test]
    fn skipped_tests_are_counted_from_the_summary() {
        let s = scan(
            "Summary [ 0.1s] 5 tests run: 5 passed, 2 skipped\nSummary [ 0.1s] 1 test run: 1 passed, 1 skipped\n",
        );
        assert_eq!((s.tests, s.skipped), (6, 3));
        assert_eq!(scan(LOG).skipped, 0);
        assert_eq!(scan("Summary [ 0.1s] 1 test run: 1 passed\n").skipped, 0);
    }

    #[test]
    fn single_test_wording_and_summaries_add_up() {
        let s =
            scan("Summary [ 0.1s] 1 test run: 1 passed\nSummary [ 0.2s] 5 tests run: 5 passed\n");
        assert_eq!(s.tests, 6);
        assert_eq!(scan("Summary [ 0.1s] 1 test run: 1 passed\n").tests, 1);
    }

    #[test]
    fn no_tests_message_and_zero_count() {
        let s = scan("error: no tests to run\n");
        assert!(s.no_tests_msg);
        assert_eq!(s.tests, 0);
    }

    #[test]
    fn a_test_name_that_mentions_compiling_is_not_a_compile() {
        assert_eq!(
            scan("        PASS [ 0.01s] x::Compiling_is_fun\n").compiled,
            0
        );
    }

    #[test]
    fn unverified_lines_are_counted_only_at_the_start_of_a_line() {
        let s = scan(
            "PLINTH-UNVERIFIED: hex raft mat: no rule\n    PLINTH-UNVERIFIED: other: why\n\
             a PLINTH-UNVERIFIED: mid-line mention\nplain\n",
        );
        assert_eq!(s.unverified, 2);
        assert_eq!(scan(LOG).unverified, 0);
    }

    #[test]
    fn rebuilt_lists_package_names() {
        assert_eq!(rebuilt(LOG), vec!["plinth-geom", "plinth-support"]);
    }
}
