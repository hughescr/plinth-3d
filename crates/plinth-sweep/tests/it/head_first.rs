//! **The up-sweep builds articulated heads** (lane `head-first`, Craig
//! 2026-10-06; `heads.rs` carries the law and `up_sweep::commit_seat` the
//! producer).
//!
//! On a steep model face the arm's top is a short straight head along the
//! footprint normal `u`, from the contact centre `D` to a joint sphere at
//! `K = D + (g + hb + r_K)·u`, and the post runs from `K`. These tests drive the
//! production sweep, the production adapter and the production lowering on two
//! convex fixtures (a ball, and a laid cylinder) and grade the heads the file
//! gets:
//!
//! * `heads_are_built_and_kept_intact_on_a_ball_and_a_laid_cylinder`: built > 0
//!   on each; every headed arm keeps its head on its path, its lowered ball's
//!   axis is parallel to `u`, the joint stands at least the final gap off the
//!   seat plane, no lip-seat bite lands on a headed arm, the landing ledger
//!   reads 0 heads not intact and 0 joins above a joint, and every member
//!   landing on a headed host lands at a joinable layer;
//! * `the_heads_switch_fires_both_ways`: with `heads` off a flat fixture's
//!   digest is the one it has with heads on (no steep face, no head) and the
//!   ball's digest differs.
//!
//! The descent-level mechanisms (the K-descent never lands on the model
//! between the joint and the seat; a straight descent is blocked by a head)
//! are graded at their functions in `up_sweep.rs::head_first_tests`.
//!
//! Sabotages, each run by hand against the production code (recorded in the
//! lane report): the head switch ignored (the second test); the head's cells
//! not stamped as a shaft (the ledger reads joins above a joint on the ball);
//! the post descent read from the running state (`head_first_tests`); the
//! straight descent never blocked (`head_first_tests`).

use plinth_core::{Finite, LayerPlan, PositiveFinite};
use plinth_mesh::Mesh;
use plinth_support::BuildFrame;
use plinth_sweep::{
    arms_of, run_sweep_with_drains, to_primitives, Landing, SweepEmission, SweepPlan, SweepPolicy,
};

use crate::harness;

const MAX_PRIMITIVES: usize = 400_000;
const MAX_PRIMITIVE_BYTES: usize = 1 << 28;
const GAP_MM: f64 = 0.3;

/// A closed UV sphere about `centre`, outward normals.
fn ball(radius: f64, centre: [f64; 3], rings: u32, segments: u32) -> Mesh {
    let mut verts: Vec<[f64; 3]> = vec![[centre[0], centre[1], centre[2] + radius]];
    for ring in 1..rings {
        let theta = core::f64::consts::PI * f64::from(ring) / f64::from(rings);
        for segment in 0..segments {
            let phi = 2.0 * core::f64::consts::PI * f64::from(segment) / f64::from(segments);
            verts.push([
                centre[0] + radius * theta.sin() * phi.cos(),
                centre[1] + radius * theta.sin() * phi.sin(),
                centre[2] + radius * theta.cos(),
            ]);
        }
    }
    verts.push([centre[0], centre[1], centre[2] - radius]);
    let south = 1 + (rings - 1) * segments;
    let at = |ring: u32, segment: u32| 1 + (ring - 1) * segments + segment % segments;
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for segment in 0..segments {
        tris.push([0, at(1, segment), at(1, segment + 1)]);
        tris.push([south, at(rings - 1, segment + 1), at(rings - 1, segment)]);
    }
    for ring in 1..rings - 1 {
        for segment in 0..segments {
            tris.push([at(ring, segment), at(ring + 1, segment), at(ring + 1, segment + 1)]);
            tris.push([at(ring, segment), at(ring + 1, segment + 1), at(ring, segment + 1)]);
        }
    }
    Mesh::new(verts, tris).expect("a closed ball")
}

/// A closed cylinder lying on its side, axis along x, with flat end caps.
fn laid_cylinder(radius: f64, length: f64, segments: u32) -> Mesh {
    let mut verts: Vec<[f64; 3]> = Vec::new();
    for end in [0.0, length] {
        for segment in 0..segments {
            let phi = 2.0 * core::f64::consts::PI * f64::from(segment) / f64::from(segments);
            verts.push([end, radius * phi.cos(), radius * phi.sin()]);
        }
    }
    let (centre_a, centre_b) = (u32::try_from(verts.len()).expect("small"), u32::try_from(verts.len() + 1).expect("small"));
    verts.push([0.0, 0.0, 0.0]);
    verts.push([length, 0.0, 0.0]);
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for segment in 0..segments {
        let next = (segment + 1) % segments;
        let (a0, a1, b0, b1) = (segment, next, segments + segment, segments + next);
        tris.push([a0, b1, b0]);
        tris.push([a0, a1, b1]);
        tris.push([centre_a, a1, a0]);
        tris.push([centre_b, b0, b1]);
    }
    Mesh::new(verts, tris).expect("a closed cylinder")
}

/// A flat slab: a box whose underside is one horizontal plane.
fn flat_slab() -> Mesh {
    plinth_mesh::make_box([12.0, 12.0, 3.0], [0.0, 0.0, 0.0]).expect("a box")
}

struct Run {
    plan: SweepPlan,
    layers: LayerPlan,
}

fn run(mesh: &Mesh, policy: &SweepPolicy) -> Run {
    let height = harness::layer_height_mm();
    let (min_z, max_z) = harness::z_span(mesh);
    let plate_z = min_z - harness::LIFT_MM;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a layer count is a finite positive quotient of model-scale lengths"
    )]
    let count = (((max_z - plate_z) / height).ceil() as usize).max(1);
    let layers = LayerPlan::new(plate_z, vec![height; count]).expect("a uniform layer plan");
    let plan = run_sweep_with_drains(mesh, &layers, policy, &[])
        .expect("the sweep emits and warns; it never refuses");
    Run { plan, layers }
}

fn lower(run: &Run) -> SweepEmission {
    let frame = BuildFrame::new(
        Finite::new(run.plan.plate_z_mm).expect("a finite plate"),
        PositiveFinite::new(harness::LIFT_MM).expect("a positive lift"),
    );
    to_primitives(&run.plan, frame, &run.layers, MAX_PRIMITIVES, MAX_PRIMITIVE_BYTES)
        .expect("the lowering refuses only about the request")
}

fn policy() -> SweepPolicy {
    SweepPolicy::from_registry().expect("the registry supplies every sweep number")
}

/// What one fixture's heads look like on the finished field; asserts every
/// per-head property, returns `(built, headed)`.
fn grade(name: &str, mesh: &Mesh) -> (usize, usize) {
    let run = run(mesh, &policy());
    let plan = &run.plan;
    let heads = &plan.report.heads;
    // A head given up after the final sizing keeps its route and is drawn
    // straight (`drawn_straight`); it is not a head on the finished field.
    let headed: Vec<usize> = (0..plan.struts.len())
        .filter(|&i| plan.struts[i].head.as_ref().is_some_and(|head| !head.drawn_straight))
        .collect();
    println!(
        "{name}: {} arms, {} built, {} headed on the finished field; refusals {:?}; placement \
         residual max {:.3} deg",
        plan.struts.len(),
        heads.built,
        headed.len(),
        heads.refusals,
        heads.placement_residual_deg_max,
    );
    assert!(heads.built > 0, "{name}: no head was built, so nothing is graded");
    // A head the final sizing grew across the clearance went back to the
    // straight tip (orchestrator ruling 2026-10-06): built = kept + downgraded.
    let downgraded = &heads.downgraded_after_sizing;
    assert_eq!(
        headed.len() + downgraded.len(),
        heads.built,
        "{name}: every head built is kept or downgraded after the sizing"
    );
    for &id in downgraded {
        assert!(
            plan.struts[id as usize].head.as_ref().is_some_and(|head| head.drawn_straight),
            "{name}: downgraded member {id} is still drawn as a head"
        );
    }
    let height = plan.layer_height_mm;
    let arms = arms_of(plan);
    for &index in &headed {
        let strut = &plan.struts[index];
        let head = strut.head.as_ref().expect("headed");
        let joint_slot = head.k_joint - strut.k_bottom;
        println!(
            "{name}: arm {index}: planned tip {:.3} joint {:.3} (K at layer {}), final tip {:.3} \
             joint ledger {:.3}, post {:?} from layer {}, |DK| {:.3}",
            head.planned_tip_radius_mm,
            head.joint_radius_mm,
            head.k_joint,
            strut.tip_radius_mm(),
            strut.radius_mm[joint_slot],
            strut.landing,
            strut.k_bottom,
            geom_kernel::vec3::dist(head.contact_mm, head.joint_mm),
        );
        assert!(strut.head_intact(), "{name}: arm {index} lost its head from its path");
        // The lowered ball's axis is the head's `u`.
        let normal = arms[index].seat_normal_mm;
        let cross = geom_kernel::vec3::cross(normal, head.unit);
        assert!(
            geom_kernel::vec3::dot(cross, cross).sqrt() < 1e-9
                && geom_kernel::vec3::dot(normal, head.unit) > 0.0,
            "{name}: arm {index}: seat normal {normal:?} is not the head's u {:?}",
            head.unit
        );
        // The joint stands the final gap off the seat plane.
        let stand_off = geom_kernel::vec3::dot(
            geom_kernel::vec3::sub(head.joint_mm, head.contact_mm),
            head.unit,
        ) - head.joint_radius_mm;
        assert!(
            stand_off >= GAP_MM - 1e-9,
            "{name}: arm {index}: planned stand-off {stand_off} under the gap {GAP_MM}"
        );
    }
    // Every member landing on a headed host lands at a joinable layer.
    for strut in &plan.struts {
        if let Landing::Strut { id, layer } = strut.landing {
            let host = &plan.struts[id as usize];
            if host.head.is_some() {
                assert!(
                    host.joinable_at(layer, height),
                    "{name}: member {} lands on headed host {id} at layer {layer}, above its \
                     joinable top",
                    strut.id
                );
            }
        }
    }
    // The ledger and the lip-seat census.
    let finished = &plan.report.landing_check.finished;
    assert!(finished.heads_not_intact.is_empty(), "{name}: {finished:?}");
    assert!(finished.joins_above_joint.is_empty(), "{name}: {finished:?}");
    assert_eq!(
        plan.report.lip_seat.headed_bites,
        0,
        "{name}: a lip-seat bite on a headed arm: {:?}",
        plan.report
            .lip_seat
            .over_limit_after
            .iter()
            .filter(|warning| plan.struts[warning.arm as usize]
                .head
                .as_ref()
                .is_some_and(|head| !head.drawn_straight))
            .collect::<Vec<_>>()
    );
    assert_eq!(plan.report.clearance_reroute.headed_bites, 0, "{name}: a re-route bite on a headed arm");
    // The drawing: every head built is drawn.
    let emission = lower(&run);
    println!(
        "{name}: {} drawn as heads, {} drawn straight, stand-offs {:?}, stalks {:?}",
        emission.tips_headed,
        emission.heads_drawn_straight,
        emission.head_standoff_mm,
        emission.head_stalk_mm,
    );
    assert_eq!(emission.tips_headed, headed.len(), "{name}: built is drawn");
    assert_eq!(emission.heads_drawn_straight, 0, "{name}");
    // The drawn stand-off is the planned one less whatever the joint grew by
    // when the final tip needed a wider joint than the placement's lower bound
    // (the joint is frozen at K and only grows; the deficit is a warning). A
    // head whose joint did not grow stands at least the final gap off.
    assert_eq!(emission.head_standoff_mm.len(), headed.len(), "{name}");
    // Every head that is drawn stands the final gap off its seat plane,
    // whatever its joint grew by: the grown ones were downgraded.
    for (&index, &stand_off) in headed.iter().zip(&emission.head_standoff_mm) {
        assert!(
            stand_off >= GAP_MM - 1e-9,
            "{name}: arm {index}: drawn stand-off {stand_off} under the gap {GAP_MM}"
        );
    }
    assert_eq!(emission.joints_grown, 0, "{name}: no drawn joint is grown across the clearance");
    // THE VISIBLE NECK: the bare stalk between the ball's far side and the
    // joint sphere, as the lowering draws it, is positive on every drawn head
    // (a head with no bare stalk is a ball fused to its joint, and was
    // downgraded) and at least one layer.
    assert_eq!(emission.head_stalk_mm.len(), headed.len(), "{name}");
    for (&index, &stalk) in headed.iter().zip(&emission.head_stalk_mm) {
        assert!(
            stalk >= height,
            "{name}: arm {index}: the drawn neck {stalk} mm is under one layer ({height} mm)"
        );
    }
    println!(
        "{name}: {} downgraded after sizing {:?}; {} kept",
        downgraded.len(),
        downgraded,
        headed.len()
    );
    (heads.built, headed.len())
}

#[test]
fn heads_are_built_and_kept_intact_on_a_ball_and_a_laid_cylinder() {
    let (ball_built, _) = grade("ball", &ball(7.0, [0.0, 0.0, 7.0], 48, 72));
    let (cylinder_built, _) = grade("laid cylinder", &laid_cylinder(5.0, 16.0, 72));
    println!("heads built: ball {ball_built}, laid cylinder {cylinder_built}");
}

#[test]
fn the_heads_switch_fires_both_ways() {
    // Anti-vacuity first: the registry's switch is on.
    assert!(policy().heads);
    let off = policy().without_heads();
    // A flat underside has no steep face: heads on or off, the same field.
    let slab = flat_slab();
    let slab_on = run(&slab, &policy());
    let slab_off = run(&slab, &off);
    assert_eq!(slab_on.plan.report.heads.built, 0, "a flat face gets no head");
    assert_eq!(
        slab_on.plan.report.sweep_digest, slab_off.plan.report.sweep_digest,
        "a flat fixture digests the same with heads on and off"
    );
    // The ball: heads built on, none off, and the digests differ.
    let mesh = ball(7.0, [0.0, 0.0, 7.0], 48, 72);
    let on = run(&mesh, &policy());
    let none = run(&mesh, &off);
    assert!(on.plan.report.heads.built > 0, "the ball gets heads");
    assert_eq!(none.plan.report.heads.built, 0, "heads off builds none");
    assert!(none.plan.struts.iter().all(|strut| strut.head.is_none()));
    assert_ne!(
        on.plan.report.sweep_digest, none.plan.report.sweep_digest,
        "the switch changes the ball's field"
    );
}
