//! **Path-hit witnesses for the sabotage suite.**
//!
//! A suite that cannot be made to fail by breaking what it tests is not
//! evidence, and a GREEN oracle that never ran the path it claims to test is
//! worse than a red one — the seal-landing lesson of 2026-08-23, where a
//! byte-identity proof used a model that never fired the changed path and two
//! ACCEPTs went by before a tripwire caught the flip.
//!
//! So every sabotage row in the design of record carries two halves:
//!
//! 1. **A mutation**, applied to the source, run against the named oracle, and
//!    reverted. Those runs are transcripts, not tests; they live in
//!    `plinth-evidence/slice-sweep-2026-09-02/lane1/`.
//! 2. **An asserted path-hit count**, which lives here, permanently, and runs
//!    on every build. It is what makes a green oracle mean the path RAN.
//!
//! Each test below names the sabotage row it guards, the mutation that row
//! applies, and the count that mutation would drive to zero.

// Test-local integer-width conversions between the report's u64 counters and
// the usize counts they are checked against, on a 64-bit target.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

mod harness;

use harness::sweep;
use plinth_mesh::{make_cavity_box, make_disc_fan, make_overhang_table, make_recessed_disc};

fn round_50() -> plinth_mesh::Mesh {
    make_recessed_disc(24.0, 25.0, 3.5, 2.5, 22.5, 128).expect("the Round 50 profile")
}

/// S1 (reach test) and S2 (re-seeded rootedness) — the disc.
///
/// S1 replaces the rootedness test with `c ∈ Cured(k)` and S2 re-seeds `U := 0`
/// over any cured cell. Both make every model cell rooted, both take the disc
/// to zero arms, and both are invisible unless the reach test is known to have
/// been evaluated and the vertical-inheritance branch is known to have been
/// taken.
#[test]
fn the_reach_test_and_the_vertical_inheritance_branch_both_fire_on_the_disc() {
    let plan = sweep(&round_50(), 0.05);
    let hits = plan.report.counters;
    println!(
        "S1/S2 witnesses: reach tests {}, vertical inheritance hits {}, layers {}",
        hits.reach_tests, hits.vertical_inheritance_hits, plan.report.layers
    );
    // One reach evaluation per NON-EMPTY layer. The lift's layers hold no
    // model, so the count is the model's own layer span and not the stack.
    assert!(
        hits.reach_tests >= 1,
        "the reach test never ran; a green disc oracle would mean nothing"
    );
    assert!(
        hits.vertical_inheritance_hits >= 100_000,
        "the vertical-inheritance branch was hit {} times; S2 mutates a path that never ran",
        hits.vertical_inheritance_hits
    );
    // And the outcome S1/S2 would destroy.
    assert!(plan.report.struts_total > 0);
}

/// S3 (allocation renormalisation) and S4 (struts spread) — the disc.
///
/// S3 drops the weight renormalisation so weights no longer sum to one; the
/// conservation assertion in the down-sweep fires. S4 deletes the column rule
/// and lets an arm spread, which collapses every per-arm load below the
/// smallest printable tip. Both need the spread search and the per-arm share
/// read to have run.
#[test]
fn the_spread_search_and_the_strut_share_read_both_fire_on_the_disc() {
    let plan = sweep(&round_50(), 0.05);
    let hits = plan.report.counters;
    println!(
        "S3/S4 witnesses: spread searches {}, strut share reads {}, struts {}",
        hits.multi_carrier_split_hits, hits.strut_share_reads, plan.report.struts_total
    );
    assert!(
        hits.multi_carrier_split_hits >= 1,
        "no load was ever routed through the nearest-carrier search; S3 mutates a dead path"
    );
    // Every arm reads its share on every layer it occupies.
    let occupied: usize = plan.struts.iter().map(|strut| strut.carried_mm2.len()).sum();
    assert_eq!(
        hits.strut_share_reads as usize, occupied,
        "an arm skipped a layer's share read"
    );
    assert!(hits.strut_share_reads >= plan.report.struts_total as u64);
}

/// S5 (landing injection) — the cavity box, with the count named exactly.
///
/// S5 deletes the injection of a landing arm's carried load into the model it
/// seats on. The plate arms under the cavity then lose the routed ceiling
/// load and are under-sized. The injection must fire exactly once per arm that
/// lands on model.
#[test]
fn the_landing_injection_fires_exactly_once_per_model_landing() {
    let plan = sweep(&make_cavity_box(40.0, 3.0).unwrap(), 0.10);
    let hits = plan.report.counters;
    let model_landings = plan.report.landings.model + plan.report.landings.strut;
    println!(
        "S5 witness: landing injections {} against {} model/strut landings",
        hits.landing_injections, model_landings
    );
    assert_eq!(
        hits.landing_injections as usize, model_landings,
        "the injection path ran a different number of times than there are landings on material"
    );
    assert!(model_landings > 0, "the cavity box must land arms on its own floor");
}

/// S6 (triangulation invariance) — the rasteriser entered once per layer per
/// mesh.
///
/// S6 re-introduces RC4 by charging triangles instead of cells. Its oracle is
/// in `oracle_disc.rs`; what lives here is the witness that the rasteriser ran
/// at all, and ran the same number of times for two triangulations of one
/// solid.
#[test]
fn the_rasteriser_runs_once_per_layer_per_sweep_and_is_blind_to_triangulation() {
    let fan = sweep(&make_disc_fan(30.0, 4.0, 128).unwrap(), 0.05);
    let uniform = sweep(&make_disc_fan(30.0, 4.0, 512).unwrap(), 0.05);
    println!(
        "S6 witness: rasteriser entries {} (128-gon) against {} (512-gon), layers {}",
        fan.report.counters.rasteriser_entries,
        uniform.report.counters.rasteriser_entries,
        fan.report.layers
    );
    // Once per layer on the way up and once per layer on each down-sweep pass.
    assert!(fan.report.counters.rasteriser_entries >= 2 * fan.report.layers as u64);
    assert_eq!(
        fan.report.counters.rasteriser_entries, uniform.report.counters.rasteriser_entries,
        "the rasteriser's work moved with the triangle count"
    );
}

/// S7 (max over events replaced by a sum) — the max-combine must fire on every
/// carrier cell of every layer.
#[test]
fn the_max_combine_fires_on_every_carrier_cell_of_every_layer() {
    let plan = sweep(&make_overhang_table().unwrap(), 0.05);
    let hits = plan.report.counters;
    println!(
        "S7 witness: max combines {} over {} layers",
        hits.max_combine_hits, plan.report.layers
    );
    assert!(
        hits.max_combine_hits >= plan.report.layers as u64,
        "the per-cell maximum over events ran {} times over {} layers; S7 mutates a path that \
         barely runs",
        hits.max_combine_hits,
        plan.report.layers
    );
}

/// S8 (placement `κ` swapped with the audit's gate `κ`) — both constants are
/// read exactly once per run and the reporting flag depends on the difference.
#[test]
fn both_reach_constants_are_read_exactly_once_and_the_audit_flag_depends_on_them() {
    let plan = sweep(&round_50(), 0.05);
    let hits = plan.report.counters;
    assert_eq!(hits.kappa_placement_reads, 1);
    assert_eq!(hits.kappa_audit_gate_reads, 1);
    // The two are different numbers and the report says which is which. If a
    // swap ever happens, this equality is what stops it being silent.
    assert!(
        plan.report.reach.kappa_used > plan.report.reach.kappa_audit_gate,
        "placement reads {} and the audit gate reads {}; placement must be the MEASURED value, \
         not the gate's",
        plan.report.reach.kappa_used,
        plan.report.reach.kappa_audit_gate
    );
    println!(
        "S8 witness: kappa_used {:.7}, kappa_audit_gate {:.7}, will_pass_audit_span_check {}",
        plan.report.reach.kappa_used,
        plan.report.reach.kappa_audit_gate,
        plan.report.will_pass_audit_span_check
    );
}

/// S9 (the cell-layer budget set below the model's need) — **it must COARSEN
/// AND WARN AND STILL EMIT.** A refusal here is the RC3 regression.
#[test]
fn an_impossible_budget_coarsens_and_warns_and_still_emits() {
    use plinth_core::LayerPlan;
    use plinth_sweep::{run_sweep, SweepPolicy};

    let mesh = round_50();
    let height = harness::layer_height_mm();
    let (min_z, max_z) = harness::z_span(&mesh);
    let plate_z = min_z - harness::LIFT_MM;
    let layers = (((max_z - plate_z) / height).ceil() as usize).max(1);
    let plan = LayerPlan::new(plate_z, vec![height; layers]).unwrap();
    let mut policy = SweepPolicy::from_registry().unwrap().with_working_pitch_mm(0.05);
    policy.limits.cell_layer_budget = 5_000_000;
    let swept = run_sweep(&mesh, &plan, &policy).expect("a starved budget must never refuse");
    println!(
        "S9 witness: coarsening steps {}, achieved pitch {:.4} mm, struts {}, residual demand {}",
        swept.report.coarsening_steps,
        swept.report.achieved_pitch_mm,
        swept.report.struts_total,
        swept.report.residual_demand_cells,
    );
    assert!(
        swept.report.coarsening_steps > 0,
        "a 5e6 cell-layer budget on a 50 mm disc must coarsen"
    );
    assert!(swept.report.achieved_pitch_mm > 0.05);
    assert_eq!(
        swept.report.counters.coarsening_loop_hits,
        u64::from(swept.report.coarsening_steps)
    );
    assert!(swept.report.struts_total > 0, "a coarsened run still emits a field");
}

/// S10 (the lift zeroed) — a zero-lift pose is never a candidate, and the
/// refusal is on ENTRY rather than a verdict about the model.
#[test]
fn a_zero_lift_pose_is_refused_on_entry_and_the_assertion_fires_once_otherwise() {
    use plinth_core::LayerPlan;
    use plinth_sweep::{run_sweep, SweepError, SweepPolicy};

    let mesh = round_50();
    let height = harness::layer_height_mm();
    let (min_z, max_z) = harness::z_span(&mesh);
    let policy = SweepPolicy::from_registry().unwrap().with_working_pitch_mm(0.10);

    // No lift: the plate sits exactly at the model's own minimum.
    let layers = (((max_z - min_z) / height).ceil() as usize).max(1);
    let flat = LayerPlan::new(min_z, vec![height; layers]).unwrap();
    let refused = run_sweep(&mesh, &flat, &policy);
    println!("S10 witness: zero-lift outcome {refused:?}");
    assert!(matches!(refused, Err(SweepError::ZeroLift { .. })));

    // And with the lift applied, the entry assertion fires exactly once.
    let lifted = sweep(&mesh, 0.10);
    assert_eq!(lifted.report.counters.lift_assertions, 1);
}

/// S11 (the ARM ledger's max over events replaced by a sum) — the strut share
/// read must fire on many layers per arm, which is what makes `+` and `max`
/// different numbers.
///
/// S7 above guards the MODEL raster's combine; it cannot see this one. The arm
/// ledger summed distinct peel events for the whole of LANE 1 and every flat
/// oracle stayed green, because on a flat underside each arm is fed by a single
/// birth layer and `max(a, 0) == a + 0`. The discriminator is an arm that reads
/// a share on many layers; `oracle_ramp` is the graded fixture and this is its
/// path-hit witness.
#[test]
fn every_arm_reads_a_share_on_many_layers_so_a_sum_and_a_maximum_differ() {
    let plan = sweep(&round_50(), 0.05);
    let hits = plan.report.counters;
    let arms = plan.report.struts_total as u64;
    println!(
        "S11 witness: strut share reads {} over {} arms and {} layers",
        hits.strut_share_reads, arms, plan.report.layers
    );
    assert!(arms > 0, "no arms means no ledger to sabotage");
    assert!(
        hits.strut_share_reads > arms * 8,
        "arms read a share {} times over {arms} arms; a ledger read once per arm cannot tell a \
         sum from a maximum, so S11 would mutate a path that never runs",
        hits.strut_share_reads,
    );
}
