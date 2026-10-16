//! **A winner plan the orientation search kept is the plan the seal would
//! compute** (`r1-winner-reuse`, 2026-10-09).
//!
//! The orientation search sweeps each of its three finalists at production
//! fidelity. [`RetainingSweepDemand`] keeps those plans; the pipeline emits the
//! winner's instead of sweeping the same mesh again. That is only sound if the
//! kept plan, once its deferred shake check has run, is the value a fresh
//! `run_sweep_with_holes` under the emitted policy returns, to the last bit, at
//! every thread count. These tests compare the two with `Debug` (f64 `Debug` is
//! the shortest round trip, so equal text is equal bits) and their lowerings
//! with `==`.
//!
//! Path hits are asserted, or a comparison of two runs of the same cold path
//! would pass: the store holds all three finalists, the frame ledger classified
//! pairs, the rigid-share diagnostic solved events, and the shake check ran on
//! completion and not before.
//!
//! The sabotage tests at the bottom break the retained plan three ways and show
//! the comparison turns red. They test the comparison; the source sabotages of
//! the change itself are recorded in the lane notes.

use plinth_core::{Finite, LayerPlan, PositiveFinite};
use plinth_mesh::{make_brace_pair, Mesh};
use plinth_orient::demand::{DemandFidelity, SupportDemandMeasurer};
use plinth_orient::{OrientFaceTable, UpDirection};
use plinth_support::BuildFrame;
use plinth_sweep::orient_demand::{
    production_measurement_policy, reuse_for, reuse_miss, RetainedPlan, RetainingSweepDemand,
    ReuseMiss,
};
use plinth_sweep::{
    complete_for_emission, run_sweep_with_holes, to_primitives, SweepDemand, SweepPlan, SweepPolicy,
};

/// Three finalist-like directions: the authored one and two tilts.
fn directions() -> Vec<UpDirection> {
    vec![
        UpDirection::MODEL_UP,
        UpDirection::new([0.35, 0.10, 0.93]).expect("a direction"),
        UpDirection::new([-0.20, 0.45, 0.87]).expect("a direction"),
    ]
}

fn table(mesh: &Mesh) -> OrientFaceTable {
    OrientFaceTable::build(mesh, "exact").expect("a closed fixture always scores")
}

/// Measure `directions()` through the retaining measurer inside a rayon pool of
/// `threads`, and return the numbers beside the kept plans.
fn retained_in_pool(
    mesh: &Mesh,
    threads: usize,
) -> (Vec<plinth_orient::demand::MeasuredSupportDemand>, Vec<RetainedPlan>) {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .expect("a rayon pool");
    pool.install(|| {
        let demand = RetainingSweepDemand::production();
        let measured = demand
            .measure_support_demand(mesh, &table(mesh), &directions(), DemandFidelity::Production)
            .into_iter()
            .map(|one| one.expect("a measurable fixture"))
            .collect();
        (measured, demand.drain())
    })
}

/// The plan the seal would compute for a retained pose: the same mesh and
/// layers, the emitted (registered) policy, no holes.
fn fresh(retained: &RetainedPlan) -> SweepPlan {
    let emitted = SweepPolicy::from_registry().expect("the registry");
    run_sweep_with_holes(&retained.mesh, &retained.layers, &emitted, &[], &[])
        .expect("the sweep emits and warns")
}

fn lowered(plan: &SweepPlan, layers: &LayerPlan) -> plinth_sweep::SweepEmission {
    let plate_z = layers.z0_mm();
    let frame = BuildFrame::new(
        Finite::new(plate_z).expect("a finite plate"),
        PositiveFinite::new(0.4).expect("a lift"),
    );
    to_primitives(plan, frame, layers, 4_000_000, 1 << 30).expect("the lowering")
}

/// The numbers the search ranks on do not move when the finalists are kept.
#[test]
fn keeping_the_finalists_changes_no_measured_number() {
    let mesh = make_brace_pair().expect("fixture");
    let plain = SweepDemand::production().measure_support_demand(
        &mesh,
        &table(&mesh),
        &directions(),
        DemandFidelity::Production,
    );
    let (kept, _plans) = retained_in_pool(&mesh, 4);
    assert_eq!(plain.len(), kept.len());
    for (plain, kept) in plain.iter().zip(&kept) {
        assert_eq!(plain.as_ref().expect("measured"), kept, "a kept finalist is measured the same");
    }
}

/// Path hits, and the equality, at 1, 4 and 16 threads.
#[test]
fn a_retained_plan_is_the_plan_the_seal_computes() {
    let mesh = make_brace_pair().expect("fixture");
    for threads in [1_usize, 4, 16] {
        let (_measured, plans) = retained_in_pool(&mesh, threads);
        // Path hit: all three finalists were kept, in candidate order.
        assert_eq!(plans.len(), 3, "{threads} threads: the store holds the three finalists");
        for (plan, up) in plans.iter().zip(directions()) {
            assert_eq!(plan.up, up, "{threads} threads: candidate order");
        }
        let emitted = SweepPolicy::from_registry().expect("the registry");
        for retained in &plans {
            // Path hits on the three report-only passes the measurement now runs.
            let lane = &retained.plan.report.slenderness_lane;
            assert!(
                lane.frame_pairs_past_ceiling + lane.frame_pairs_without_room
                    + lane.frame_sets_refused_incomplete
                    > 0,
                "{threads} threads: the frame ledger classified pairs"
            );
            assert!(
                retained.plan.deferred_rigid_share.is_some(),
                "{threads} threads: the rigid-share diagnostic is deferred, not run"
            );
            assert!(
                !retained.plan.report.rigid_share.ran,
                "{threads} threads: the rigid-share diagnostic has not run in the measurement"
            );
            assert!(
                retained.plan.deferred_stage_shake.is_some(),
                "{threads} threads: the shake check is deferred, not run"
            );
            assert_eq!(
                lane.stage_shake.stages, 0,
                "{threads} threads: the shake check has not run in the measurement"
            );

            let want = fresh(retained);
            let got = reuse_for(retained, &retained.mesh, &retained.layers, &emitted, &[], &[])
                .expect("the key matches the plan's own mesh, layers and policy");
            assert!(
                got.report.slenderness_lane.stage_shake.stages > 0,
                "{threads} threads: the shake check ran on completion"
            );
            assert!(got.deferred_stage_shake.is_none());
            assert!(got.deferred_rigid_share.is_none());
            assert!(
                got.report.rigid_share.events_solved > 0,
                "{threads} threads: the rigid-share diagnostic solved events on completion"
            );
            assert_eq!(
                format!("{got:?}"),
                format!("{want:?}"),
                "{threads} threads: the reused plan is the fresh plan, bit for bit"
            );
            assert_eq!(
                lowered(&got, &retained.layers),
                lowered(&want, &retained.layers),
                "{threads} threads: and so is what the lowering makes of it"
            );
        }
    }
}

/// The key refuses everything that is not the measured sweep.
#[test]
fn the_reuse_key_refuses_a_different_mesh_layers_policy_or_holes() {
    let mesh = make_brace_pair().expect("fixture");
    let (_measured, plans) = retained_in_pool(&mesh, 4);
    let emitted = SweepPolicy::from_registry().expect("the registry");
    let (winner, loser) = (&plans[0], &plans[1]);
    let key = |retained: &RetainedPlan,
               mesh: &Mesh,
               layers: &LayerPlan,
               policy: &SweepPolicy,
               drains: &[plinth_sweep::DrainSpec]| {
        reuse_miss(retained, mesh, layers, policy, drains, &[])
    };

    assert_eq!(key(winner, &winner.mesh, &winner.layers, &emitted, &[]), None);
    // Another pose's mesh.
    assert_eq!(
        key(winner, &loser.mesh, &winner.layers, &emitted, &[]),
        Some(ReuseMiss::MeshDiffers)
    );
    // One vertex coordinate one ulp away.
    let mut verts = winner.mesh.verts().to_vec();
    verts[0][2] = f64::from_bits(verts[0][2].to_bits() + 1);
    let nudged = Mesh::new(verts, winner.mesh.tris().to_vec()).expect("a mesh");
    assert_eq!(
        key(winner, &nudged, &winner.layers, &emitted, &[]),
        Some(ReuseMiss::MeshDiffers)
    );
    // A plate one ulp away, and a layer one ulp thicker.
    let moved_plate = LayerPlan::new(
        f64::from_bits(winner.layers.z0_mm().to_bits() + 1),
        winner.layers.heights_mm().to_vec(),
    )
    .expect("a layer plan");
    assert_eq!(
        key(winner, &winner.mesh, &moved_plate, &emitted, &[]),
        Some(ReuseMiss::LayersDiffer)
    );
    let mut heights = winner.layers.heights_mm().to_vec();
    heights[0] = f64::from_bits(heights[0].to_bits() + 1);
    let thicker = LayerPlan::new(winner.layers.z0_mm(), heights).expect("a layer plan");
    assert_eq!(
        key(winner, &winner.mesh, &thicker, &emitted, &[]),
        Some(ReuseMiss::LayersDiffer)
    );
    // A policy that differs in a field the key does not name one by one.
    let other = emitted.without_slenderness_brace();
    assert_eq!(
        key(winner, &winner.mesh, &winner.layers, &other, &[]),
        Some(ReuseMiss::PolicyDiffers)
    );
    // A drain.
    let drain = plinth_sweep::DrainSpec::from_bore([0.0, 0.0, 1.0], [0.0, 0.0, 5.0], 0.5);
    assert_eq!(
        key(winner, &winner.mesh, &winner.layers, &emitted, &[drain]),
        Some(ReuseMiss::HolesPresent)
    );
}

/// **Sabotage (a):** the production measurement with the frame ledger OFF.
/// The kept plan then differs from the fresh one, so the comparison above would
/// go red if the measurement ever dropped the ledger.
#[test]
fn sabotage_a_measurement_without_the_ledger_is_not_the_emitted_plan() {
    let mesh = make_brace_pair().expect("fixture");
    let (_measured, plans) = retained_in_pool(&mesh, 4);
    let retained = &plans[0];
    let emitted = SweepPolicy::from_registry().expect("the registry");
    let policy = production_measurement_policy(emitted.without_frame_ledger());
    let mut sabotaged = run_sweep_with_holes(&retained.mesh, &retained.layers, &policy, &[], &[])
        .expect("the sweep emits and warns");
    complete_for_emission(&mut sabotaged);
    assert_ne!(
        format!("{sabotaged:?}"),
        format!("{:?}", fresh(retained)),
        "a ledger-off measurement must be told apart from the emitted plan"
    );
}

/// **Sabotage (b):** the kept plan emitted WITHOUT completing its shake check.
#[test]
fn sabotage_b_an_uncompleted_plan_is_not_the_emitted_plan() {
    let mesh = make_brace_pair().expect("fixture");
    let (_measured, plans) = retained_in_pool(&mesh, 4);
    let retained = &plans[0];
    assert_ne!(
        format!("{:?}", retained.plan),
        format!("{:?}", fresh(retained)),
        "skipping complete_for_emission must be told apart"
    );
}

/// **Sabotage (c):** a key that ignored the mesh would hand the loser's plan to
/// the winner's mesh; that plan is not the winner's.
#[test]
fn sabotage_c_a_losers_plan_is_not_the_winners_plan() {
    let mesh = make_brace_pair().expect("fixture");
    let (_measured, plans) = retained_in_pool(&mesh, 4);
    let (winner, loser) = (&plans[0], &plans[1]);
    let mut loser_plan = loser.plan.clone();
    complete_for_emission(&mut loser_plan);
    assert_ne!(
        format!("{loser_plan:?}"),
        format!("{:?}", fresh(winner)),
        "another pose's plan must be told apart from this pose's"
    );
}
