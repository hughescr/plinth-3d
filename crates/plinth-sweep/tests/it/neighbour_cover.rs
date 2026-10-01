//! **An under-loaded arm goes only if the reach law, re-run without it, still
//! roots everything it rooted** (lane neighbour-cover, 2026-09-30).
//!
//! The removal pass used to accept coverage only from an arm seated on the
//! SAME layer, by straight distance, so a low-load creep arm on a flank could
//! never be removed (`oversupport-attribution.md`, item 4). The rule now asks
//! the two exact questions instead: the up-sweep's multi-layer reach
//! potential, replayed over the field the removal would leave, and the
//! down-sweep's uncarried load and overloaded arms.
//!
//! # What these tests grade, on `make_laid_tube`
//!
//! The laid tube's flank carries under-loaded creep arms. Measured on it:
//! with the replay switched off the pass removes some of them, the down-sweep
//! reads no uncarried load, and the slenderness lane then places fewer braces
//! — so the LOAD does not need them. With the replay on, every one is kept,
//! because each roots a few cells no other arm's potential reaches.
//!
//! * `the_reach_law_keeps_the_flank_arms_the_load_alone_would_drop`: the
//!   replay ran, over layers, found the cells, and kept the arms; the
//!   replay-off drill drops them with the same uncarried load. This is the
//!   sabotage: a pass that never asks the reach law cannot pass it.
//! * `the_rule_until_2026_09_30_never_asks_the_reach_law`: the same-layer
//!   rule is graded by the same predicate and goes red on the path hit.
//!
//! The hand-derived geometry (an arm seated above does not cover the layer
//! below; two neighbours that reach every cell let the middle arm go) is in
//! the unit tests of `src/demand_replay.rs`. No test here re-implements the
//! potential.

use crate::harness;

use harness::sweep_with;
use plinth_mesh::make_laid_tube;
use plinth_sweep::{StrutKind, SweepPlan, SweepPolicy};

fn count(plan: &SweepPlan, kind: StrutKind) -> usize {
    plan.struts.iter().filter(|s| s.kind == kind).count()
}

fn describe(name: &str, plan: &SweepPlan) -> String {
    let r = &plan.report;
    format!(
        "{name}: arms {} braces {} removals {} (span-refused {}, rolled back {}); uncarried {:.4} \
         mm2; residual {} cells; overloaded {}; {:?}",
        count(plan, StrutKind::Arm),
        count(plan, StrutKind::Brace),
        r.counters.strut_removals,
        r.counters.strut_removals_refused_by_span,
        r.counters.removal_passes_rolled_back,
        r.uncarried_load_mm2,
        r.residual_demand_cells,
        r.overloaded_struts.len(),
        r.removal_coverage,
    )
}

/// The predicate: the reach law was asked of every candidate, over real
/// layers, and it found cells the removal would have left unrooted.
fn asked_the_reach_law(plan: &SweepPlan) -> Result<(), String> {
    let c = &plan.report.removal_coverage;
    if c.candidates == 0 {
        return Err("no under-loaded candidate: the fixture tests nothing".into());
    }
    if c.replays == 0 || c.replay_trial_layers == 0 {
        return Err(format!("the reach replay never ran: {c:?}"));
    }
    if c.replay_newly_unrooted_cells == 0 || c.refused_by_replay == 0 {
        return Err(format!("the replay ran and found nothing to keep: {c:?}"));
    }
    Ok(())
}

#[test]
fn the_reach_law_keeps_the_flank_arms_the_load_alone_would_drop() {
    let mesh = make_laid_tube().expect("the laid tube fixture");
    let policy = SweepPolicy::from_registry().expect("registry");
    let kept = sweep_with(&mesh, 0.0, &policy);
    let dropped = sweep_with(&mesh, 0.0, &policy.without_removal_replay());
    println!("{}", describe("production", &kept));
    println!("{}", describe("replay off", &dropped));

    asked_the_reach_law(&kept).unwrap_or_else(|why| panic!("production: {why}"));
    let coverage = &kept.report.removal_coverage;
    // Measured on this fixture: every candidate roots cells nobody else
    // reaches, so every one is kept and none is removed.
    assert_eq!(
        coverage.refused_by_replay, coverage.candidates,
        "every flank candidate on the laid tube roots a cell no other arm reaches: {coverage:?}"
    );
    assert_eq!(kept.report.counters.strut_removals, 0);
    // The replay reproduces the placement: on the field as placed it finds
    // exactly the residual the up-sweep reported.
    assert_eq!(
        coverage.replay_baseline_unrooted_cells,
        u64::try_from(kept.report.residual_demand_cells).expect("a cell count fits u64"),
        "the replay must measure the placed field as the placement did"
    );

    // THE DRILL. Without the replay the same pass removes arms, and the
    // down-sweep is content: the load does not need them. Only the reach law
    // keeps them, so a pass that never asks it cannot pass the predicate.
    assert!(
        asked_the_reach_law(&dropped).is_err(),
        "the replay-off drill must fail the predicate: {}",
        describe("replay off", &dropped)
    );
    assert!(
        dropped.report.counters.strut_removals > 0,
        "the drill must actually remove arms, or it proves nothing: {}",
        describe("replay off", &dropped)
    );
    assert!(
        count(&kept, StrutKind::Arm) > count(&dropped, StrutKind::Arm),
        "production keeps arms the drill drops"
    );
    assert!(
        dropped.report.uncarried_load_mm2 <= kept.report.uncarried_load_mm2 + 1.0e-9,
        "the drill's removals pass the load check ({} vs {} mm2): the reach law alone keeps them",
        dropped.report.uncarried_load_mm2,
        kept.report.uncarried_load_mm2
    );
}

#[test]
fn the_rule_until_2026_09_30_never_asks_the_reach_law() {
    let mesh = make_laid_tube().expect("the laid tube fixture");
    let policy = SweepPolicy::from_registry()
        .expect("registry")
        .with_same_layer_removal();
    let plan = sweep_with(&mesh, 0.0, &policy);
    println!("{}", describe("same-layer", &plan));
    assert!(
        plan.report.removal_coverage.candidates > 0,
        "the same fixture offers the same candidates"
    );
    assert!(
        asked_the_reach_law(&plan).is_err(),
        "the same-layer rule must fail the predicate: it decides by straight distance on the \
         seat layer and never replays the potential"
    );
}
