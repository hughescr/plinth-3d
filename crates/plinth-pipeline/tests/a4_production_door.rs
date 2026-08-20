//! **SPEC-A4 design §G.1 stage 4 — the production door reaches the one-winner
//! transactional controller, and swapping it back turns this file red.**
//!
//! Stage 4 is one line in `prepare::seal_and_emit`: the sealed support run is
//! `plinth_support::run_support_transactional` instead of the batch loop
//! `plinth_support::run_support`. A one-line swap is exactly the kind of change
//! that a suite full of green tests will not notice being undone — every
//! digest, every gate verdict and every island count can be identical either
//! side of it on a model that does not need repair, and on a model that does,
//! "it still emits" is true under both loops.
//!
//! So the proof is a MARKER only the new loop emits. [`RepairRunReport`] is the
//! §A.8 instrument the transactional controller fills on every path, sealed or
//! refused; `run_support` builds its authority with `None` there and has no
//! ledger to fill it from. Reverting the door therefore makes the report
//! disappear from the run report, and both tests below fail — the first because
//! there is no report at all, the second because the trials it counts are gone.
//!
//! **Read through the public pipeline API, not through `plinth-support`.** The
//! assertions below travel `plinth_pipeline::prepare::run_from_oriented` ->
//! `seal_and_emit` -> `ChainOutcome`, which is the same path production takes.
//! A test that called `run_support_transactional` itself would prove the
//! controller works and prove nothing at all about which controller the
//! pipeline calls, which is the entire content of stage 4.
//!
//! Both fixtures are procedural — built from `plinth_mesh::make_*` against a
//! checked-in `corpus/baseline/*.pipeline.json` — so this file runs on a clean
//! worktree with no `corpus/models/` present (task #16's known hole).

#[path = "../examples/shared/first_artifact_chain.rs"]
mod chain;

use std::path::PathBuf;

use plinth_mesh::{make_overhang_table, make_toed_slab};

/// A per-test scratch path that is removed on the way in and on the way out.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "plinth-a4-production-door-{label}-{}.chitubox",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// **The revert-proof, on a model that NEEDS repair.**
///
/// `make_overhang_table` does not cover on its baseline layout: it closes only
/// after the controller commits repair rows. Under the transactional loop that
/// costs measurable trials and commits, and the §A.8 report carries them out to
/// the run report. Under the batch loop there is no report to carry.
///
/// The assertions are on the SHAPE of what the loop spent, not on frozen
/// counts, because the count is a property of the geometry and the ranking and
/// will move when either moves; what may not move is that a production
/// emission's cost is stated at all. The three that must hold:
///
/// * a report exists — the marker itself;
/// * at least one step COMMITTED — this model reached its artifact through
///   repair, so a report claiming zero commits would be describing a different
///   run than the one that just emitted;
/// * every commit was a trial — the two counters come from the same loop and
///   `commits > trials` would mean the accounting is invented.
#[test]
fn the_production_door_reports_what_the_one_winner_controller_spent() {
    let scratch = Scratch::new("overhang-table");
    let outcome = chain::run_procedural_model(
        "make_overhang_table",
        make_overhang_table().expect("procedural overhang table"),
        scratch.path(),
    )
    .expect("make_overhang_table emits through coverage repair");

    let repair = outcome.repair_report.as_ref().expect(
        "SPEC-A4 §G.1 stage 4: the production door is the transactional controller, and it \
         reports on every path. `None` here means `seal_and_emit` is calling the batch loop \
         `run_support` again — that is the revert this test exists to catch",
    );

    assert!(
        repair.commits >= 1,
        "this fixture reaches its artifact THROUGH repair, so the run report must say at least \
         one repair step earned its place: {repair:?}"
    );
    assert!(
        repair.trials >= repair.commits,
        "every commit is a trial, so an accounting with more commits than trials is not a \
         measurement of anything: {repair:?}"
    );
    assert_eq!(
        repair.trials - repair.commits,
        repair.rejections_by_class.values().sum::<usize>(),
        "every trial that did not commit must appear in the rejection histogram by class, or \
         the cost R10 traded the failed-tries cap for is not fully stated: {repair:?}"
    );

    // The operator-visible line R10's trade depends on, reached through the
    // same value the CLI prints. A report that exists but summarises to
    // nothing is a report nobody can read.
    let line = repair.summary_line();
    for field in ["trials=", "commits=", "remaining=", "wall_total="] {
        assert!(
            line.contains(field),
            "the run report's per-model repair line must carry {field}: {line}"
        );
    }
}

/// **The same marker on a model that needs NO repair, which is the harder
/// half.**
///
/// `make_toed_slab` covers on its baseline layout and never enters a
/// transaction (design §E.5 case 6), so its emitted artifact is bit-identical
/// under both controllers — there is no digest, no island count and no gate
/// verdict that can tell the two loops apart on this model. The report is the
/// only thing that can, and it is present and reads zero.
///
/// That is why `trials=0 commits=0` is asserted as a VALUE rather than allowed
/// to be `None`: "this model paid nothing" and "nobody was counting" are
/// different facts, and only the second one is what a reverted door produces.
#[test]
fn a_model_that_needs_no_repair_still_reports_that_it_spent_nothing() {
    let scratch = Scratch::new("toed-slab");
    let outcome = chain::run_procedural_model(
        "make_toed_slab",
        make_toed_slab().expect("procedural toed slab"),
        scratch.path(),
    )
    .expect("make_toed_slab emits on its baseline layout");

    let repair = outcome.repair_report.as_ref().expect(
        "a sealing model reports too: `None` here is the batch loop, not a model that paid \
         nothing",
    );
    assert_eq!(
        (repair.trials, repair.commits),
        (0, 0),
        "the toed slab covers on its baseline layout, so the controller must have cost it \
         nothing — a non-zero count here means a model that never needed a transaction entered \
         one, and its frozen digest is no longer safe: {repair:?}"
    );
    assert!(
        repair.rejections_by_class.is_empty(),
        "no trial means no rejection to classify: {repair:?}"
    );
    assert_eq!(
        repair.wall_time_total_ms, 0,
        "no trial means no trial time to total: {repair:?}"
    );
}
