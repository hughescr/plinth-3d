//! plinth-cli library: the command line's logic, so tests can call it.
//!
//! Step 7 holds the transport path only: each input STL is read, lifted off the plate and
//! written alone (no supports) as a `.chitubox`, checked by the separate `plinth-check`
//! program on those exact bytes, and published only if the checker read it cleanly. Every
//! file is labelled "not a print candidate".

pub mod args;
pub mod check;
pub mod json;
pub mod jsonin;
mod load_report;
pub mod loop_canary;
pub mod mem;
pub mod names;
pub mod publish;
pub mod report;
pub mod run;
pub mod sha256;

use std::path::Path;

pub use loop_canary::loop_canary;

/// Resolve the checker and limits defaults from the path of the running binary.
///
/// # Errors
/// A message when a default is needed but the binary is not inside a checkout's build
/// folder.
pub fn config_from(opts: &args::Options, exe: &Path) -> Result<run::Config, String> {
    let defaults = args::checkout_defaults(exe);
    let pick = |given: &Option<std::path::PathBuf>, which: usize, flag: &str| {
        given
            .clone()
            .or_else(|| {
                defaults
                    .as_ref()
                    .map(|d| if which == 0 { d.0.clone() } else { d.1.clone() })
            })
            .ok_or_else(|| {
                format!("{flag} is required: plinth is not running from a checkout's build folder")
            })
    };
    Ok(run::Config {
        inputs: opts.inputs.clone(),
        out_dir: opts.out_dir.clone(),
        max_threads: opts.max_threads,
        checker: pick(&opts.checker, 0, "--checker")?,
        limits: pick(&opts.limits, 1, "--limits")?,
        candidate: opts.candidate,
        review: opts.review.then(|| run::Review {
            plan_budget: opts
                .plan_budget
                .unwrap_or(std::time::Duration::from_secs_f64(
                    args::DEFAULT_PLAN_BUDGET_SECS,
                )),
            checker_timeout: opts
                .checker_timeout
                .unwrap_or(std::time::Duration::from_secs_f64(
                    args::DEFAULT_CHECKER_TIMEOUT_SECS,
                )),
        }),
    })
}

/// Canary values of this crate and every crate below it, summed. Calling it from
/// production code makes a canary edit drive the real rebuild chain.
#[must_use]
pub fn canary_chain() -> u32 {
    plinth_geom::loop_canary()
        .wrapping_add(plinth_support::loop_canary())
        .wrapping_add(plinth_chitubox::loop_canary())
        .wrapping_add(plinth_emit::loop_canary())
        .wrapping_add(loop_canary())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn chain_is_emit_chain_plus_own() {
        assert_eq!(
            canary_chain(),
            plinth_emit::canary_chain().wrapping_add(loop_canary())
        );
    }

    fn opts() -> args::Options {
        args::parse(["--out-dir", "o", "x.stl"].map(Into::into)).unwrap()
    }

    #[test]
    fn defaults_come_from_the_checkout_and_flags_win() {
        let c = config_from(&opts(), Path::new("/r/target/debug/plinth")).unwrap();
        assert_eq!(
            c.checker,
            PathBuf::from("/r/checker/target/debug/plinth-check")
        );
        assert_eq!(c.limits, PathBuf::from("/r/limits.toml"));
        let mut o = opts();
        o.checker = Some("mine".into());
        o.limits = Some("lim".into());
        let c = config_from(&o, Path::new("plinth")).unwrap();
        assert_eq!((c.checker, c.limits), ("mine".into(), "lim".into()));
        assert!(config_from(&opts(), Path::new("plinth")).is_err());
    }
}
