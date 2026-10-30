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
    /// nextest said it had no tests to run.
    pub no_tests_msg: bool,
}

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
        }
        if line.contains("no tests to run") {
            s.no_tests_msg = true;
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
    fn rebuilt_lists_package_names() {
        assert_eq!(rebuilt(LOG), vec!["plinth-geom", "plinth-support"]);
    }
}
