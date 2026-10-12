//! **The joint the file draws is the joint the certificate sized** (Craig
//! 2026-10-11: "If the joint's mis-sized, let's fix THAT problem").
//!
//! The old head law sized the joint for a load line the contact cannot pass, and
//! the post under it inherited the size (Deinonychus support A: a joint sphere of
//! radius 1.333 under a link of 0.428). The production sweep now sizes the cone,
//! the joint and the post for what the link and the seat weld CAN pass
//! (`joint_cert.rs`, `joint_size.rs`), under the maximum-principal-stress
//! criterion, certified on every frustum.
//!
//! Graded on the EMITTED primitives of the production sweep and lowering, never
//! on the plan restated:
//!
//! | test | what it asserts |
//! |---|---|
//! | `the_emitted_cone_ends_and_the_sphere_are_the_certified_joint` | on a ball and a laid cylinder, for every certified head: exactly one joint sphere at `K` whose radius is the certified joint radius, the cone's last emitted frustum ends on `K` at that same radius (cone endpoint AND sphere, R5), and the link frustum is the tip's own radius |
//! | `the_certificate_off_draws_the_old_fat_joint` | SABOTAGE: with `without_joint_certificate` the same fixtures draw joints larger than the certified ones, so the assertion above goes red |
//! | `the_certified_posts_hold_the_capacity_of_their_sources` | the sweep's own check of every drawn chord reads at most `1 + 1e-9`, and the report counts what it certified and what stayed on the old law |

use plinth_core::{Finite, LayerPlan, PositiveFinite};
use plinth_mesh::Mesh;
use plinth_support::{BuildFrame, SupportPrimitive, SupportSegmentKind};
use plinth_sweep::{run_sweep_with_drains, to_primitives, SweepEmission, SweepPlan, SweepPolicy};

use crate::harness;

const MAX_PRIMITIVES: usize = 400_000;
const MAX_PRIMITIVE_BYTES: usize = 1 << 28;

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

struct Run {
    plan: SweepPlan,
    emission: SweepEmission,
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
    let frame = BuildFrame::new(
        Finite::new(plan.plate_z_mm).expect("a finite plate"),
        PositiveFinite::new(harness::LIFT_MM).expect("a positive lift"),
    );
    let emission = to_primitives(&plan, frame, &layers, MAX_PRIMITIVES, MAX_PRIMITIVE_BYTES)
        .expect("the lowering refuses only about the request");
    Run { plan, emission }
}

fn policy() -> SweepPolicy {
    SweepPolicy::from_registry().expect("the registry supplies every sweep number")
}

fn point(p: [Finite; 3]) -> [f64; 3] {
    p.map(Finite::get)
}

fn near(a: [f64; 3], b: [f64; 3], tolerance: f64) -> bool {
    geom_kernel::vec3::dist(a, b) <= tolerance
}

/// The emitted joint sphere at `k`, and the emitted frustum that ends on it.
fn emitted_joint(
    primitives: &[SupportPrimitive],
    k: [f64; 3],
) -> (Vec<&SupportPrimitive>, Vec<&SupportPrimitive>) {
    let spheres = primitives
        .iter()
        .filter(|p| p.segment_kind == SupportSegmentKind::JointSphere)
        .filter(|p| {
            let mid = std::array::from_fn(|axis| 0.5 * (point(p.start_mm)[axis] + point(p.end_mm)[axis]));
            near(mid, k, 1e-6)
        })
        .collect();
    let cones = primitives
        .iter()
        .filter(|p| p.segment_kind != SupportSegmentKind::JointSphere)
        .filter(|p| near(point(p.end_mm), k, 1e-9) && p.start_radius_mm.get() > 0.0)
        .filter(|p| !near(point(p.start_mm), k, 1e-9))
        .collect();
    (spheres, cones)
}

/// Every drawn head of the finished field.
fn headed(plan: &SweepPlan) -> Vec<usize> {
    (0..plan.struts.len())
        .filter(|&i| plan.struts[i].head.as_ref().is_some_and(|head| !head.drawn_straight))
        .collect()
}

fn fixtures() -> Vec<(&'static str, Mesh)> {
    vec![
        ("ball", ball(7.0, [0.0, 0.0, 7.0], 48, 72)),
        ("laid cylinder", laid_cylinder(5.0, 16.0, 72)),
    ]
}

#[test]
fn the_emitted_cone_ends_and_the_sphere_are_the_certified_joint() {
    let mut graded = 0_usize;
    for (name, mesh) in fixtures() {
        let run = run(&mesh, &policy());
        let plan = &run.plan;
        let certificate = plan.report.heads.joint_certificate;
        println!(
            "{name}: {} headed, certificate {certificate:?}",
            headed(plan).len()
        );
        for index in headed(plan) {
            let strut = &plan.struts[index];
            let head = strut.head.as_ref().expect("headed");
            let Some(cert) = &head.certified else {
                continue;
            };
            let joint = cert.profile.joint_radius_mm;
            let (spheres, cones) = emitted_joint(&run.emission.primitives, head.joint_mm);
            assert_eq!(spheres.len(), 1, "{name}: arm {index}: one joint sphere at K");
            let sphere_radius = spheres[0].start_radius_mm.get();
            assert!(
                (sphere_radius - joint).abs() <= 1e-9,
                "{name}: arm {index}: the sphere {sphere_radius} is not the certified joint {joint}"
            );
            // The cone's last emitted frustum ends on K at the same radius.
            assert!(!cones.is_empty(), "{name}: arm {index}: no frustum ends on K");
            assert!(
                cones.iter().any(|cone| (cone.end_radius_mm.get() - joint).abs() <= 1e-9),
                "{name}: arm {index}: no frustum ends on K at the certified radius {joint}: {:?}",
                cones.iter().map(|c| c.end_radius_mm.get()).collect::<Vec<_>>()
            );
            // The link is the tip's own radius.
            let tip = strut.tip_radius_mm();
            assert!(
                run.emission
                    .primitives
                    .iter()
                    .any(|p| p.segment_kind == SupportSegmentKind::ContactTaper
                        && (p.start_radius_mm.get() - tip).abs() <= 1e-9
                        && (p.end_radius_mm.get() - tip).abs() <= 1e-9),
                "{name}: arm {index}: no link of the tip's radius {tip}"
            );
            // The certified cone never exceeds the capacity (R4).
            assert!(cert.drawn_max <= 1.0 + 1e-9, "{name}: arm {index}: drawn maximum {}", cert.drawn_max);
            graded += 1;
        }
    }
    assert!(graded > 0, "no certified head was graded: the test would pass vacuously");
}

#[test]
fn the_certificate_off_draws_the_old_fat_joint() {
    let mut compared = 0_usize;
    for (name, mesh) in fixtures() {
        let on = run(&mesh, &policy());
        let off = run(&mesh, &policy().without_joint_certificate());
        assert_eq!(off.plan.report.heads.joint_certificate.certified, 0, "{name}: the switch fires");
        for index in headed(&on.plan) {
            let Some(cert) = on.plan.struts[index].head.as_ref().and_then(|head| head.certified.as_ref()) else {
                continue;
            };
            let Some(old) = off.plan.struts.iter().find(|s| s.id == on.plan.struts[index].id) else {
                continue;
            };
            let Some(old_head) = old.head.as_ref().filter(|head| !head.drawn_straight) else {
                continue;
            };
            let old_joint = old.radius_mm[old_head.k_joint - old.k_bottom];
            // SABOTAGE: the old law draws more than the certified joint, so the
            // emitted-sphere assertion of the other test would be red on it.
            assert!(
                old_joint >= cert.profile.joint_radius_mm * 0.999,
                "{name}: arm {index}: the old law {old_joint} under the certified {}",
                cert.profile.joint_radius_mm
            );
            compared += 1;
        }
    }
    assert!(compared > 0, "nothing was compared: the test would pass vacuously");
}

#[test]
fn the_certified_posts_hold_the_capacity_of_their_sources() {
    for (name, mesh) in fixtures() {
        let run = run(&mesh, &policy());
        let certificate = run.plan.report.heads.joint_certificate;
        println!("{name}: {certificate:?}");
        assert_eq!(certificate.over_one, 0, "{name}: a drawn chord exceeds the capacity");
        assert!(
            certificate.worst_drawn_max <= 1.0 + 1e-9,
            "{name}: worst drawn maximum {}",
            certificate.worst_drawn_max
        );
        // Every drawn head is accounted for: certified, on the old law (counted),
        // or never sized.
        let drawn = headed(&run.plan).len();
        assert_eq!(
            certificate.certified
                + certificate.old_law_shared
                + certificate.old_law_no_seat
                + certificate.old_law_infeasible
                + certificate.not_sized,
            drawn,
            "{name}: {certificate:?}"
        );
    }
}

fn junction_hulls(primitives: &[SupportPrimitive]) -> Vec<SupportPrimitive> {
    primitives
        .iter()
        .filter(|p| matches!(p.ring_normal, plinth_support::FrustumRingNormal::Junction(_)))
        .cloned()
        .collect()
}

/// R8: the junction hulls of the certified field are the old law's, record for
/// record. The sabotage that proves the equality has teeth is the unit test
/// `junction::tests::the_planned_hulls_are_kept_on_a_field_whose_radii_moved`
/// (the pass run on the moved field makes a different hull); on this fixture
/// the thinned shafts happen to leave the bare lowering's hulls unchanged too,
/// which the run prints.
#[test]
fn the_certified_field_keeps_every_junction_hull_of_the_old_law_field() {
    let (mut hulls, mut moved_without_the_reference, mut thinned) = (0_usize, 0_usize, 0_usize);
    // The tall stilt plate is the pinned fixture with mid-shaft landings.
    let with_hulls = vec![(
        "tall stilt plate",
        plinth_mesh::make_tall_stilt_plate().expect("the tall stilt plate fixture"),
    )];
    for (name, mesh) in with_hulls {
        let on = run(&mesh, &policy());
        let off = run(&mesh, &policy().without_joint_certificate());
        let (kept, old) = (junction_hulls(&on.emission.primitives), junction_hulls(&off.emission.primitives));
        assert_eq!(kept, old, "{name}: the certified field's hulls are not the old law's");
        hulls += old.len();
        thinned += on
            .plan
            .struts
            .iter()
            .zip(&off.plan.struts)
            .filter(|(a, b)| a.radius_mm.iter().zip(&b.radius_mm).any(|(x, y)| x < &(y - 1e-9)))
            .count();
        // The same plan lowered with no reference field.
        let frame = BuildFrame::new(
            Finite::new(on.plan.plate_z_mm).expect("a finite plate"),
            PositiveFinite::new(harness::LIFT_MM).expect("a positive lift"),
        );
        let (min_z, max_z) = harness::z_span(&mesh);
        let height = harness::layer_height_mm();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a layer count is a finite positive quotient of model-scale lengths"
        )]
        let count = (((max_z - (min_z - harness::LIFT_MM)) / height).ceil() as usize).max(1);
        let layers = LayerPlan::new(min_z - harness::LIFT_MM, vec![height; count]).expect("a uniform layer plan");
        let bare = plinth_support::lower_sweep_arms(
            &plinth_sweep::arms_of(&on.plan),
            frame,
            &layers,
            MAX_PRIMITIVES,
            MAX_PRIMITIVE_BYTES,
        )
        .expect("the lowering refuses only about the request");
        if junction_hulls(&bare.primitives) != old {
            moved_without_the_reference += 1;
        }
    }
    println!("hulls {hulls}, shafts thinned {thinned}, fixtures whose hulls move without the reference {moved_without_the_reference}");
    assert!(hulls > 0, "no fixture has a junction hull: the test would pass vacuously");
    assert!(thinned > 0, "the certificate thinned no shaft: the test would pass vacuously");
}
