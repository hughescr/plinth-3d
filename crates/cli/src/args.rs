//! Command-line parsing (the only module that may read the process arguments).
//!
//! ```text
//! plinth [--max-threads N] --out-dir DIR [--report FILE] [--checker FILE] [--limits FILE]
//!        [--candidate] <input.stl>...
//! plinth ... --review-construction [--plan-budget SECS] [--checker-timeout SECS] <input.stl>...
//! ```
//!
//! `--review-construction` builds supports for any already posed STL (resting up +Z) to LOOK
//! at; the output is never a print candidate and the flag cannot be combined with
//! `--candidate`.
//!
//! There is no flag that replaces an existing output file (PRD FR-37).

use lexopt::prelude::*;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Default thread cap during development (PRD NFR-9).
pub const DEFAULT_MAX_THREADS: usize = 8;
/// Largest accepted `--max-threads`.
pub const MAX_THREADS_LIMIT: usize = 256;

/// The usage line.
pub const USAGE: &str = "usage: plinth [--max-threads N] --out-dir DIR [--report FILE] [--checker FILE] [--limits FILE] [--candidate | --review-construction [--hollow-fixture] [--plan-budget SECS] [--checker-timeout SECS]] <input.stl>...";

/// Default wall-clock budget of review-mode planning, seconds.
pub const DEFAULT_PLAN_BUDGET_SECS: f64 = 120.0;
/// Default checker timeout in review mode, seconds.
pub const DEFAULT_CHECKER_TIMEOUT_SECS: f64 = 300.0;
/// Largest accepted budget or timeout, seconds.
const MAX_SECS: f64 = 86_400.0;

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Input STL files, as given.
    pub inputs: Vec<PathBuf>,
    /// Directory the finished `.chitubox` files go into.
    pub out_dir: PathBuf,
    /// Cap on worker threads for every parallel step.
    pub max_threads: usize,
    /// Where to write the JSON run report (standard output when absent).
    pub report: Option<PathBuf>,
    /// The `plinth-check` program (next to the checkout's build folder when absent).
    pub checker: Option<PathBuf>,
    /// The limits file handed to the checker (`limits.toml` of the checkout when absent).
    pub limits: Option<PathBuf>,
    /// Offer print candidates only: publish a file only when the checker passed it with no
    /// applicable rule unverified or violated.
    pub candidate: bool,
    /// Review mode: supports for any posed STL, to look at, never a print candidate.
    pub review: bool,
    /// Review-only convex hollowing with two axial 3 mm openings.
    pub hollow_fixture: bool,
    /// `--plan-budget`, when given (review mode only).
    pub plan_budget: Option<std::time::Duration>,
    /// `--checker-timeout`, when given (review mode only).
    pub checker_timeout: Option<std::time::Duration>,
}

/// Where the checker program and `limits.toml` are, from the path of the running `plinth`
/// binary: `<checkout>/target/<profile>/plinth` gives
/// `<checkout>/checker/target/<profile>/plinth-check` and `<checkout>/limits.toml`.
/// `None` when the binary is not inside a checkout's build folder.
#[must_use]
pub fn checkout_defaults(exe: &Path) -> Option<(PathBuf, PathBuf)> {
    let profile_dir = exe.parent()?;
    let profile = profile_dir.file_name()?;
    let root = profile_dir.parent()?.parent()?;
    let checker = root
        .join("checker/target")
        .join(profile)
        .join("plinth-check");
    Some((checker, root.join("limits.toml")))
}

fn threads(text: &str) -> Result<usize, String> {
    match text.parse::<usize>() {
        Ok(n) if (1..=MAX_THREADS_LIMIT).contains(&n) => Ok(n),
        _ => Err(format!(
            "--max-threads must be a whole number from 1 to {MAX_THREADS_LIMIT}, got '{text}'"
        )),
    }
}

fn seconds(flag: &str, text: &str) -> Result<std::time::Duration, String> {
    match text.parse::<f64>() {
        Ok(s) if s.is_finite() && s > 0.0 && s <= MAX_SECS => {
            Ok(std::time::Duration::from_secs_f64(s))
        }
        _ => Err(format!(
            "{flag} must be a number of seconds above 0 and at most {MAX_SECS}, got '{text}'"
        )),
    }
}

fn value(p: &mut lexopt::Parser) -> Result<OsString, String> {
    p.value().map_err(|e| e.to_string())
}

/// Parse the arguments after the program name.
///
/// # Errors
/// Returns a one-line message for an unknown flag, a missing value, a bad thread count, no
/// output directory or no input.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Options, String> {
    let mut p = lexopt::Parser::from_args(args);
    let mut o = Options {
        inputs: Vec::new(),
        out_dir: PathBuf::new(),
        max_threads: DEFAULT_MAX_THREADS,
        report: None,
        checker: None,
        limits: None,
        candidate: false,
        review: false,
        hollow_fixture: false,
        plan_budget: None,
        checker_timeout: None,
    };
    let mut have_out = false;
    while let Some(arg) = p.next().map_err(|e| e.to_string())? {
        match arg {
            Long("max-threads") => {
                let text = value(&mut p)?.string().map_err(|e| e.to_string())?;
                o.max_threads = threads(&text)?;
            }
            Long("out-dir") => {
                o.out_dir = value(&mut p)?.into();
                have_out = true;
            }
            Long("report") => o.report = Some(value(&mut p)?.into()),
            Long("checker") => o.checker = Some(value(&mut p)?.into()),
            Long("limits") => o.limits = Some(value(&mut p)?.into()),
            Long("candidate") => o.candidate = true,
            Long("review-construction") => o.review = true,
            Long("hollow-fixture") => o.hollow_fixture = true,
            Long("plan-budget") => {
                let text = value(&mut p)?.string().map_err(|e| e.to_string())?;
                o.plan_budget = Some(seconds("--plan-budget", &text)?);
            }
            Long("checker-timeout") => {
                let text = value(&mut p)?.string().map_err(|e| e.to_string())?;
                o.checker_timeout = Some(seconds("--checker-timeout", &text)?);
            }
            Value(v) => o.inputs.push(v.into()),
            other => return Err(other.unexpected().to_string()),
        }
    }
    if !have_out {
        return Err("--out-dir is required".into());
    }
    if o.inputs.is_empty() {
        return Err("no input files".into());
    }
    if o.hollow_fixture && (!o.review || o.candidate) {
        return Err(
            "--hollow-fixture requires --review-construction and refuses --candidate".into(),
        );
    }
    if o.review && o.candidate {
        return Err(
            "--review-construction cannot be combined with --candidate: review files are never print candidates".into(),
        );
    }
    if !o.review && (o.plan_budget.is_some() || o.checker_timeout.is_some()) {
        return Err("--plan-budget and --checker-timeout need --review-construction".into());
    }
    Ok(o)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    #[test]
    fn parses_flags_and_inputs() {
        let o = parse(a(&[
            "--max-threads",
            "3",
            "--out-dir",
            "o",
            "x.stl",
            "y.stl",
        ]))
        .unwrap();
        assert_eq!(o.max_threads, 3);
        assert_eq!(o.inputs, [PathBuf::from("x.stl"), PathBuf::from("y.stl")]);
        assert_eq!(parse(a(&["--out-dir=o", "x"])).unwrap().max_threads, 8);
        assert!(!parse(a(&["--out-dir=o", "x"])).unwrap().candidate);
        assert!(
            parse(a(&["--candidate", "--out-dir=o", "x"]))
                .unwrap()
                .candidate
        );
    }

    #[test]
    fn hollowing_is_explicitly_review_only() {
        let good = [
            "--hollow-fixture",
            "--review-construction",
            "--out-dir=o",
            "x",
        ];
        assert!(parse(a(&good)).unwrap().hollow_fixture);
        for bad in [
            &good[..1],
            &["--hollow-fixture", "--out-dir=o", "x"],
            &["--hollow-fixture", "--candidate", "--out-dir=o", "x"],
            &[
                "--hollow-fixture",
                "--review-construction",
                "--candidate",
                "--out-dir=o",
                "x",
            ],
        ] {
            assert!(parse(a(bad)).is_err());
        }
    }

    #[test]
    fn review_mode_never_combines_with_candidate_and_owns_its_budgets() {
        let o = parse(a(&["--review-construction", "--out-dir=o", "x"])).unwrap();
        assert!(o.review && !o.candidate && o.plan_budget.is_none());
        let o = parse(a(&[
            "--review-construction",
            "--plan-budget",
            "0.5",
            "--checker-timeout=9",
            "--out-dir=o",
            "x",
        ]))
        .unwrap();
        assert_eq!(o.plan_budget, Some(std::time::Duration::from_millis(500)));
        assert_eq!(o.checker_timeout, Some(std::time::Duration::from_secs(9)));
        for bad in [
            &["--review-construction", "--candidate", "--out-dir=o", "x"][..],
            &["--candidate", "--review-construction", "--out-dir=o", "x"],
            &["--plan-budget", "5", "--out-dir=o", "x"],
            &["--checker-timeout", "5", "--out-dir=o", "x"],
            &[
                "--review-construction",
                "--plan-budget",
                "0",
                "--out-dir=o",
                "x",
            ],
            &[
                "--review-construction",
                "--plan-budget",
                "nan",
                "--out-dir=o",
                "x",
            ],
            &[
                "--review-construction",
                "--plan-budget",
                "-1",
                "--out-dir=o",
                "x",
            ],
        ] {
            assert!(parse(a(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn rejects_bad_usage_and_has_no_overwrite_flag() {
        for bad in [
            &["--out-dir", "o"][..],
            &["x.stl"],
            &["--out-dir", "o", "--max-threads", "0", "x"],
            &["--out-dir", "o", "--max-threads", "abc", "x"],
            &["--out-dir", "o", "--max-threads", "1000", "x"],
            &["--out-dir", "o", "--nope", "x"],
            &["--out-dir", "o", "--overwrite", "x"],
            &["--out-dir"],
        ] {
            assert!(parse(a(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn checkout_defaults_follow_the_build_folder() {
        let (checker, limits) = checkout_defaults(Path::new("/r/target/debug/plinth")).unwrap();
        assert_eq!(checker, Path::new("/r/checker/target/debug/plinth-check"));
        assert_eq!(limits, Path::new("/r/limits.toml"));
        assert!(checkout_defaults(Path::new("plinth")).is_none());
    }
}
