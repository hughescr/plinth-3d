//! **Hollow first** (lane hf, 2026-10-16; `docs/plans/hollow-first-2026-10-16.md`).
//!
//! With a hollow request on the retaining measurer, every production pose runs
//! the hollow stage on its own oriented solid and is swept as the drilled shell
//! it would print as, drains and vents included. The kept plan then carries
//! those holes, and the winner-plan reuse key compares them bit for bit.
//!
//! Path hits are asserted first: each pose must have been measured on a shell
//! (its kept mesh is not its solid, and it has drains), or the key checks below
//! would only be testing solids.
//!
//! The fixture is an 18 x 20 x 22 mm block, over the 10 mm bulk minimum and,
//! at the registered 2.5 mm wall, a cavity of about 3.4 mL, over the 1.5 mL
//! savings minimum. Its two poses (authored, and lying on its side) are
//! different shapes in their own frames, so their drains differ. One
//! measurement serves both checks: a production sweep of each pose is the
//! cost of this file.

use plinth_mesh::Mesh;
use plinth_orient::demand::{DemandFidelity, SupportDemandMeasurer};
use plinth_orient::{OrientFaceTable, UpDirection};
use plinth_sweep::orient_demand::{
    pose_shell, reuse_miss, RetainedPlan, RetainingSweepDemand, ReuseMiss,
};
use plinth_sweep::SweepPolicy;

fn block() -> Mesh {
    plinth_mesh::make_box([18.0, 20.0, 22.0], [0.0, 0.0, 0.0]).expect("a block")
}

fn request() -> plinth_hollow::HollowRequest {
    let constants = plinth_core::m0_constants().expect("the registry");
    plinth_hollow::HollowRequest::with_registered_wall_default(constants.saturn4_ultra().clone())
        .expect("the registered wall")
}

fn directions() -> Vec<UpDirection> {
    vec![
        UpDirection::MODEL_UP,
        UpDirection::new([1.0, 0.0, 0.0]).expect("a direction"),
    ]
}

fn measured_shells(mesh: &Mesh) -> Vec<RetainedPlan> {
    let table = OrientFaceTable::build(mesh, "exact").expect("a closed fixture always scores");
    let demand = RetainingSweepDemand::production().hollow_first(request());
    for one in demand.measure_support_demand(mesh, &table, &directions(), DemandFidelity::Production)
    {
        let one = one.expect("a measurable fixture");
        assert!(
            one.landing_scar_raw_mm2.is_some() && one.tip_scar_raw_mm2.is_some(),
            "the shell's scars are measured (the carried fields fit the shell's faces)"
        );
    }
    demand.drain()
}

fn bits(mesh: &Mesh) -> (Vec<[u64; 3]>, Vec<[u32; 3]>) {
    (
        mesh.verts().iter().map(|vertex| vertex.map(f64::to_bits)).collect(),
        mesh.tris().to_vec(),
    )
}

/// 1. Each pose is swept as its own drilled shell, and the hollow stage run
///    again on the pose's solid (what the seal does) makes the same shell and
///    the same holes, so the key hits.
/// 2. **Sabotage: drains drilled for one pose and applied to another fail the
///    key**, four ways: the winner's plan offered the loser's holes; the
///    loser's plan relabelled with the winner's mesh and layers (the mesh and
///    layer checks pass, the holes do not); the winner's plan offered an
///    emission with no holes; one drain one ulp off.
#[test]
fn each_pose_is_swept_as_its_shell_and_only_its_own_holes_hit_the_key() {
    let mesh = block();
    let plans = measured_shells(&mesh);
    assert_eq!(plans.len(), 2, "both poses kept");
    let emitted = SweepPolicy::from_registry().expect("the registry");
    for (plan, up) in plans.iter().zip(directions()) {
        assert_eq!(plan.up, up);
        let solid = plan.pose_solid.as_ref().expect("path hit: the pose was measured on a shell");
        assert_ne!(bits(solid), bits(&plan.mesh), "the kept mesh is the shell, not the solid");
        assert!(!plan.drains.is_empty(), "path hit: the shell's drains were planned around");
        assert!(
            plan.mesh.tris().len() > solid.tris().len(),
            "a shell has a cavity surface and bores"
        );
        let again = pose_shell(solid, &request(), up).expect("the pose hollows again");
        assert_eq!(bits(&again.mesh), bits(&plan.mesh), "the hollow stage is deterministic");
        assert_eq!(
            reuse_miss(plan, &again.mesh, &plan.layers, &emitted, &again.drains, &again.vents),
            None,
            "the seal's own shell and holes hit the key"
        );
    }

    let (winner, loser) = (&plans[0], &plans[1]);
    // Both poses drain at the bottom centre, which recentring puts at the
    // origin, so their drains can be the same bits; their vents, at each
    // pose's own top, cannot.
    assert_ne!(
        format!("{:?}", (&winner.drains, &winner.vents)),
        format!("{:?}", (&loser.drains, &loser.vents)),
        "the two poses' holes differ, or the sabotage proves nothing"
    );
    let key = |retained: &RetainedPlan, drains, vents| {
        reuse_miss(retained, &winner.mesh, &winner.layers, &emitted, drains, vents)
    };
    assert_eq!(key(winner, &loser.drains, &loser.vents), Some(ReuseMiss::HolesDiffer));
    let mut relabelled = loser.clone();
    relabelled.mesh = winner.mesh.clone();
    relabelled.layers = winner.layers.clone();
    assert_eq!(
        key(&relabelled, &winner.drains, &winner.vents),
        Some(ReuseMiss::HolesDiffer)
    );
    assert_eq!(key(winner, &[], &[]), Some(ReuseMiss::HolesDiffer));
    let mut nudged = winner.drains.clone();
    nudged[0].outlet_mm[2] = f64::from_bits(nudged[0].outlet_mm[2].to_bits() + 1);
    assert_eq!(key(winner, &nudged, &winner.vents), Some(ReuseMiss::HolesDiffer));
}
