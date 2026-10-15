//! **The footprint rule's answer does not depend on the thread count** (perf
//! lane FF, 2026-10-15).
//!
//! The rule's per-tip work (each tip's candidate walk with its ground checks,
//! each proposal's verdict with its disc samples) runs side by side on the
//! rayon pool and is applied in tip order; the box tree answers the seat-face
//! question. Both must leave the finished plan as the one-thread run leaves
//! it: the footprint outcome (every count, move and refusal), every strut's
//! seat verdict and the arms the file gets, compared whole.
//!
//! ANTI-VACUITY: the fixtures move seats, refuse moves, and grade more tips
//! than the pool-split threshold, so the parallel path and the move
//! application both ran.
//!
//! Sabotage (recorded in the lane report): applying the walks in reverse tip
//! order, or dropping the lowest-id rule from the tree, turns this red.

use plinth_core::{m0_constants, LayerPlan};
use plinth_mesh::Mesh;
use plinth_sweep::{arms_of, SweepPlan, SweepPolicy};

const LIFT_MM: f64 = 5.0;

fn layer_height_mm() -> f64 {
    *m0_constants()
        .expect("registry")
        .evidence()
        .layer_height_default_mm()
        .value()
}

/// `Round 25` turned 180° about X: the stiffener ring down, where the
/// footprint rule moves rim seats.
fn round_25_flipped() -> Mesh {
    let mesh = plinth_mesh::make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96)
        .expect("the Round 25 fixture");
    let verts = mesh
        .verts()
        .iter()
        .map(|vertex| [vertex[0], -vertex[1], -vertex[2]])
        .collect();
    Mesh::new(verts, mesh.tris().to_vec()).expect("a rigid rotation preserves a valid mesh")
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a layer count is a finite positive quotient of model-scale lengths"
)]
fn sweep(mesh: &Mesh) -> SweepPlan {
    let mut min_z = f64::INFINITY;
    let mut max_z = f64::NEG_INFINITY;
    for vertex in mesh.verts() {
        min_z = min_z.min(vertex[2]);
        max_z = max_z.max(vertex[2]);
    }
    let plate_z = min_z - LIFT_MM;
    let height = layer_height_mm();
    let count = (((max_z - plate_z) / height).ceil() as usize).saturating_add(1);
    let layers = LayerPlan::new(plate_z, vec![height; count]).expect("a uniform layer plan");
    let policy = SweepPolicy::from_registry().expect("the registry supplies every sweep number");
    plinth_sweep::run_sweep(mesh, &layers, &policy)
        .expect("the sweep emits and warns; it never refuses")
}

/// Everything the footprint rule decided, as text: the outcome, every strut's
/// seat verdict, the arms the file gets.
fn decisions(plan: &SweepPlan) -> String {
    let verdicts: Vec<String> = plan
        .struts
        .iter()
        .map(|strut| format!("{:?} {:?}", strut.seat_face, strut.seat_normal))
        .collect();
    format!(
        "{:?}\n{:?}\n{}\n{:?}",
        plan.report.seat_footprint,
        plan.report.seat_footprint_normal,
        verdicts.join("\n"),
        arms_of(plan)
    )
}

/// A 6 x 4 x 1.5 mm slab turned 25 degrees about Y: slides up its tilted
/// underside are refused for leaving their seat.
fn tilted_slab() -> Mesh {
    let mesh = plinth_mesh::make_box([6.0, 4.0, 1.5], [0.0, 0.0, 0.0]).expect("a box");
    let (sin, cos) = 25.0_f64.to_radians().sin_cos();
    let verts = mesh
        .verts()
        .iter()
        .map(|v| [cos * v[0] + sin * v[2], v[1], -sin * v[0] + cos * v[2]])
        .collect();
    Mesh::new(verts, mesh.tris().to_vec()).expect("a rigid rotation preserves a valid mesh")
}

/// The plan of `mesh` swept in a pool of each thread count, compared whole
/// with the one-thread plan; the footprint outcome of each is returned.
fn at_every_thread_count(mesh: &Mesh) -> plinth_sweep::footprint::FootprintOutcome {
    let mut reference: Option<(String, plinth_sweep::footprint::FootprintOutcome)> = None;
    for threads in [1_usize, 4, 12] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("a rayon pool");
        let plan = pool.install(|| sweep(mesh));
        let text = decisions(&plan);
        match &reference {
            None => reference = Some((text, plan.report.seat_footprint.clone())),
            Some((first, _)) => assert_eq!(first, &text, "{threads} threads differ from 1"),
        }
    }
    reference.expect("a first run").1
}

#[test]
fn the_footprint_rule_leaves_the_same_plan_at_1_4_and_12_threads() {
    let flipped = at_every_thread_count(&round_25_flipped());
    assert!(
        flipped.tips_moved_onto_face + flipped.tips_moved_for_section > 4,
        "the flipped disc moves more tips than one round splits across tasks: {flipped:?}"
    );
    assert!(flipped.rounds > 0 && flipped.tips_graded > 4);
    let slab = at_every_thread_count(&tilted_slab());
    assert!(
        slab.moves_refused_off_seat > 0,
        "the slab refuses no slide, so the refusal counts were not compared: {slab:?}"
    );
}
