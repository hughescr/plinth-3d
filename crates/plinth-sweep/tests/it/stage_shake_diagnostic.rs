//! **The construction-stage shake check is a WARNING and feeds nothing**
//! (`perf-2026-10-09/plan/r1-stage-shake-design.md`, §2 and §3 (a)).
//!
//! Its result goes to one log line and one WARNING per shaken top
//! (`prepare.rs`); no score, no size and no byte of the `.chitubox` reads it.
//! So the orientation search's pricing entry, [`plinth_sweep::run_sweep`],
//! does not run it (its 3 production poses spent 37 % of their busy time in
//! it), and the emitted field's entry does.
//!
//! | test | red when |
//! |---|---|
//! | `the_search_entry_skips_the_shake_and_the_field_is_the_same` | `run_sweep` stops switching the check off (its result is no longer the empty one), or the check changes a strut or any other report field |
//! | `the_emitting_entry_runs_the_check` | the policy default turns the check off for the emitted field (the witness would then print "0 stages solved"), or the fixture stops exercising it (which would make the test above vacuous) |

use plinth_core::LayerPlan;
use plinth_mesh::make_brace_pair;
use plinth_sweep::stage_shake::StageShake;
use plinth_sweep::{run_sweep, SweepPolicy};

use crate::harness::{self, sweep_with};

/// The brace pair at its authored pose, under the production policy.
#[test]
fn the_emitting_entry_runs_the_check() {
    let mesh = make_brace_pair().expect("the brace pair fixture");
    let policy = SweepPolicy::from_registry().expect("the registry");
    assert!(policy.stage_shake_diagnostic, "the production policy runs the check");
    let plan = sweep_with(&mesh, 0.0, &policy);
    let shake = &plan.report.slenderness_lane.stage_shake;
    println!(
        "brace pair: {} diagonals, {} stages solved in {} batch(es), largest skyline {} f64s",
        shake.diagonals, shake.stages, shake.batches, shake.skyline_entries_max
    );
    assert!(shake.stages >= 1, "the check never ran: {shake:?}");
    assert!(shake.frames_solved >= 1 && shake.frames_solved == shake.stages);
    assert!(shake.batches >= 1 && shake.skyline_entries_max > 0);
}

#[test]
fn the_search_entry_skips_the_shake_and_the_field_is_the_same() {
    let mesh = make_brace_pair().expect("the brace pair fixture");
    let policy = SweepPolicy::from_registry().expect("the registry");
    let mut with = sweep_with(&mesh, 0.0, &policy);
    // The same layer plan and lift the harness uses, through the search's entry.
    let height = harness::layer_height_mm();
    let (min_z, max_z) = harness::z_span(&mesh);
    let plate_z = min_z - harness::LIFT_MM;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the harness's own layer-count arithmetic"
    )]
    let layers = (((max_z - plate_z) / height).ceil() as usize).max(1);
    let layer_plan = LayerPlan::new(plate_z, vec![height; layers]).expect("a uniform layer plan");
    let mut without = run_sweep(&mesh, &layer_plan, &policy).expect("the sweep emits");

    // PATH HIT: the emitting entry ran the check; the search entry did not.
    assert!(with.report.slenderness_lane.stage_shake.stages >= 1);
    assert_eq!(without.report.slenderness_lane.stage_shake, StageShake::default());
    // The one diagnostic the search entry also skips (its own test), and the
    // shake, are the only differences: with both zeroed, the plans are equal.
    for plan in [&mut with, &mut without] {
        plan.report.rigid_share = Default::default();
        plan.report.slenderness_lane.stage_shake = StageShake::default();
    }
    assert_eq!(with.struts, without.struts, "the check changed a strut");
    assert_eq!(with.report, without.report, "the check changed the report");
}
