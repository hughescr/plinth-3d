use super::*;
use crate::parts::Part;
use crate::report::Status;
use crate::testkit::{self as kit, test_limits};

fn declared_scene(
    limits: &crate::limits::Limits,
    model: Vec<Tri>,
    translate: V,
    parts: Vec<Part>,
) -> Scene {
    Scene::from_parts(limits, model, translate, parts).with_declared_hollow()
}

// Closed material surface of revolution. No producer code, sibling fixtures, drain list
// or inside tags. Values are rounded to the same f32 precision as an emitted mesh.
fn sphere(two_holes: bool) -> Vec<Tri> {
    sphere_hole(two_holes, 1.51)
}

fn sphere_hole(two_holes: bool, hole: f64) -> Vec<Tri> {
    // Circumscribed 32-gon: 1.51 gives emitted inscribed width above 3 mm.
    let (outer, inner) = (4.0_f64, 2.8_f64);
    let mut profile = Vec::new();
    let start = (hole / outer).asin();
    let end = if two_holes {
        std::f64::consts::PI - start
    } else {
        std::f64::consts::PI
    };
    for k in 0..=24 {
        let a = start + (end - start) * k as f64 / 24.0;
        profile.push([outer * a.sin(), -outer * a.cos()]);
    }
    let start = (hole / inner).asin();
    let end = if two_holes {
        std::f64::consts::PI - start
    } else {
        std::f64::consts::PI
    };
    for k in (0..=24).rev() {
        let a = start + (end - start) * k as f64 / 24.0;
        profile.push([inner * a.sin(), -inner * a.cos()]);
    }
    let vertex = |p: [f64; 2], k: usize| {
        let a = 2.0 * std::f64::consts::PI * (k % 32) as f64 / 32.0;
        #[allow(clippy::cast_possible_truncation, reason = "emitted f32 fixture")]
        [
            if p[0] < 1e-10 { 0.0 } else { p[0] * a.cos() },
            if p[0] < 1e-10 { 0.0 } else { p[0] * a.sin() },
            p[1],
        ]
        .map(|x| f64::from(x as f32))
    };
    let mut tris = Vec::new();
    for i in 0..profile.len() {
        for k in 0..32 {
            let [a, b, c, d] = [
                vertex(profile[i], k),
                vertex(profile[i], k + 1),
                vertex(profile[(i + 1) % profile.len()], k + 1),
                vertex(profile[(i + 1) % profile.len()], k),
            ];
            for t in [[a, b, c], [a, c, d]] {
                if len(cross(sub(t[1], t[0]), sub(t[2], t[0]))) > 1e-12 {
                    tris.push(t);
                }
            }
        }
    }
    tris
}

fn scene(two: bool, parts: Vec<Part>) -> Scene {
    declared_scene(&test_limits(), sphere(two), [0.0, 0.0, 5.0], parts)
}

#[test]
fn one_hole_hollow_is_not_a_topological_exemption_and_two_holes_find_both_mouths() {
    let one = scene(false, vec![]);
    assert!(one.model_closed);
    let t = crate::h5::analyze(&one).expect("topology");
    assert_eq!(t.euler, vec![2], "single-hole hollow is genus zero");
    let one = one.hollow();
    assert!(one.cavities > 0, "{one:?}");
    assert!(one.missing_drain > 0 || one.missing_vent > 0);
    assert_eq!(one.outcome().status, Status::Violation);
    let two = scene(true, vec![]);
    assert!(two.model_closed);
    let h = two.hollow();
    assert_eq!(h.cavities, 1, "{h:?}");
    assert_eq!(h.bores.len(), 2, "{h:?}");
    assert_eq!(h.missing_drain + h.missing_vent, 0);
    assert_eq!(h.widths_short, 0);
    assert_eq!(h.bulk_routes_missing, 0);
    assert_eq!(h.distinct_mouths_missing, 0);
    // Fixed surface cells remain ambiguous; two openings alone must NEVER pass H5.
    assert_eq!(h.outcome().status, Status::Unverified);
    assert!(!h.drained);
}

#[test]
fn complete_ball_brace_and_raft_meshes_hit_when_the_column_clears() {
    let column = || kit::part(0, 3, [8.0, 0.0, -5.0], [8.0, 0.0, 0.0], 0.2, 0.2);
    let probes = [
        (
            kit::part(1, 7, [0.0, 0.0, -3.8], [0.0, 0.0, -3.2], 0.3, 0.3),
            kit::part(1, 7, [8.0, 0.0, -3.8], [8.0, 0.0, -3.2], 0.3, 0.3),
        ),
        (
            kit::part(1, 3, [-2.0, 0.0, -3.5], [2.0, 0.0, -3.5], 0.2, 0.2),
            kit::part(1, 3, [6.0, 0.0, -3.5], [10.0, 0.0, -3.5], 0.2, 0.2),
        ),
        (
            kit::raft_pad(1, [0.0, 0.0], -3.5),
            kit::raft_pad(1, [8.0, 0.0], -3.5),
        ),
        (
            kit::part(1, 2, [0.0, 0.0, -3.8], [0.0, 0.0, -3.2], 0.4, 0.2),
            kit::part(1, 2, [8.0, 0.0, -3.8], [8.0, 0.0, -3.2], 0.4, 0.2),
        ),
        (
            kit::part(1, 8, [0.0, 0.0, -3.8], [0.0, 0.0, -3.2], 0.3, 0.3),
            kit::part(1, 8, [8.0, 0.0, -3.8], [8.0, 0.0, -3.2], 0.3, 0.3),
        ),
    ];
    let mut limits = test_limits();
    // Isolate the bore, so a large mouth ball cannot make this test pass by accident.
    limits.mouth_clearance_mm = 0.01;
    for (probe, clear) in probes {
        let s = declared_scene(
            &limits,
            sphere(true),
            [0.0, 0.0, 5.0],
            vec![column(), probe],
        );
        let h = s.hollow();
        assert!(
            h.intrusion
                .iter()
                .any(|l| l.location.record == Some(1) && l.reason.contains("detected bore")),
            "{h:?}"
        );
        assert!(!h.intrusion.iter().any(|l| l.location.record == Some(0)));
        assert_eq!(crate::h5::measure(&s).status, Status::Violation);
        let s = declared_scene(
            &limits,
            sphere(true),
            [0.0, 0.0, 5.0],
            vec![column(), clear],
        );
        assert!(s.hollow().intrusion.is_empty(), "{:?}", s.hollow());
        assert_eq!(s.hollow().outcome().status, Status::Unverified);
    }
}

#[test]
fn clearance_is_local_not_a_strip_to_the_plate_and_inside_labels_are_not_proof() {
    let s = scene(
        true,
        vec![kit::part(0, 9, [0.0, 0.0, 0.0], [0.0, 0.0, 0.4], 0.2, 0.2)],
    );
    // This part is physically in the void but drainage is still uncertain. No exemption.
    assert!(!s.hollow().wholly_inside(&s, 0));
    let s = scene(
        true,
        vec![kit::part(0, 9, [8.0, 0.0, -0.2], [8.0, 0.0, 0.2], 0.2, 0.2)],
    );
    assert!(!s.hollow().wholly_inside(&s, 0));
    let b = Bore {
        centre: [0.0; 3],
        axis: 2,
        span: [-4.0, -2.0],
        mouth: [0.0, 0.0, -4.0],
        radius: 1.5,
        chamber: 0,
    };
    let s = declared_scene(
        &test_limits(),
        kit::closed_box([-1.0; 3], [1.0; 3]),
        [0.0; 3],
        vec![kit::part(
            0,
            3,
            [0.0, 0.0, -12.0],
            [0.0, 0.0, -10.0],
            0.2,
            0.2,
        )],
    );
    assert!(bores::intrusions(&s, &[b]).0.is_empty());
}

#[test]
fn an_outside_native_foot_does_not_waive_patch_fusion_or_clearance_rules() {
    let s = scene(
        true,
        vec![
            kit::part(0, 8, [4.0, -0.15, -0.7], [4.0, -0.15, 0.7], 0.7, 0.7),
            kit::part(1, 8, [4.0, 0.15, -0.7], [4.0, 0.15, 0.7], 0.7, 0.7),
            kit::part(2, 3, [3.8, 0.0, -1.0], [3.8, 0.0, 1.0], 0.3, 0.3),
        ],
    );
    let p = crate::patch::measure(&s);
    assert!(!s.hollow().wholly_inside(&s, 0));
    assert_eq!(crate::h12::h1(&s, &p).status, Status::Violation);
    assert_eq!(crate::h12::h2(&s, &p).status, Status::Violation);
    assert_eq!(crate::h6::measure(&s, &p).status, Status::Violation);
    let clean = declared_scene(
        &test_limits(),
        kit::closed_box([-2.0, -2.0, 0.0], [2.0, 2.0, 2.0]),
        [0.0, 0.0, 5.0],
        vec![
            kit::part(0, 8, [-0.75, 0.0, -0.3], [-0.75, 0.0, 0.1], 0.2, 0.2),
            kit::part(1, 8, [0.75, 0.0, -0.3], [0.75, 0.0, 0.1], 0.2, 0.2),
        ],
    );
    let p = crate::patch::measure(&clean);
    assert_eq!(crate::h12::h1(&clean, &p).status, Status::Pass);
    assert_eq!(crate::h12::h2(&clean, &p).status, Status::Pass);
    assert_eq!(crate::h6::measure(&clean, &p).status, Status::Pass);
}

#[test]
fn mouth_limits_and_cached_measurements_are_scene_local() {
    let make = |clearance| {
        let mut limits = test_limits();
        limits.mouth_clearance_mm = clearance;
        declared_scene(
            &limits,
            sphere(true),
            [0.0, 0.0, 15.0],
            vec![kit::part(
                0,
                9,
                [3.0, 0.0, -5.0],
                [3.0, 0.0, -4.6],
                0.2,
                0.2,
            )],
        )
    };
    // The same written part clears the bore but enters only the larger mouth ball.
    let (small, large) = (make(0.01), make(5.0));
    assert!(small.hollow().intrusion.is_empty());
    let h = large.hollow();
    assert_eq!(h.intrusion.len(), 1, "{h:?}");
    assert!(h.intrusion[0].reason.contains("local mouth keep-out"));
    assert_eq!(crate::h5::measure(&large).status, Status::Violation);
    // Re-read the cached small scene after measuring the large one: no shared settings.
    assert!(small.hollow().intrusion.is_empty());
    assert_eq!(small.hollow().undecided_intrusions, 0);
}

#[test]
fn support_material_obstructs_routes_but_is_not_counted_as_trapped_resin() {
    let mut limits = test_limits();
    limits.mouth_clearance_mm = 0.01;
    let make = |radius| {
        declared_scene(
            &limits,
            sphere(true),
            [0.0, 0.0, 5.0],
            vec![kit::part(
                0,
                3,
                [0.0, 0.0, -0.4],
                [0.0, 0.0, 0.4],
                radius,
                radius,
            )],
        )
    };
    let clear = make(0.6);
    let h = clear.hollow();
    let g = h.grid.as_ref().expect("raster measured");
    let (down, air) = (g.directed(true, true), g.directed(false, true));
    let occupied: Vec<_> = (0..g.model.len())
        .filter(|&i| g.labels[i] != 0 && g.assembly[i] == MATERIAL)
        .collect();
    assert!(
        occupied.len() > 100,
        "support interior must exercise the solid-cell path"
    );
    assert!(occupied.iter().all(|&i| !down[i] && !air[i]));
    assert_eq!(h.missing_drain + h.missing_vent, 0, "{h:?}");
    assert!(h.intrusion.is_empty(), "{h:?}");
    // The wider support meets the inner wall and splits the resin, without entering
    // either bore. Treating its filled cells as open would miss both directed failures.
    let blocked = make(3.0);
    let h = blocked.hollow();
    assert!(h.intrusion.is_empty(), "{h:?}");
    assert_eq!((h.missing_drain, h.missing_vent), (1, 1), "{h:?}");
    assert_eq!(h.outcome().status, Status::Violation);
}

#[test]
fn bulk_width_search_sees_an_inside_dam_that_point_routes_do_not() {
    let mut limits = test_limits();
    limits.mouth_clearance_mm = 0.01;
    let good = declared_scene(&limits, sphere(true), [0.0, 0.0, 5.0], vec![]);
    assert_eq!(good.hollow().bulk_routes_missing, 0);
    // Does not enter either bore; leaves a narrow annular point-route around itself.
    let blocked = declared_scene(
        &limits,
        sphere(true),
        [0.0, 0.0, 5.0],
        vec![kit::part(0, 3, [0.0, 0.0, -1.0], [0.0, 0.0, 1.0], 1.5, 1.5)],
    );
    let h = blocked.hollow();
    assert!(h.intrusion.is_empty(), "{h:?}");
    assert_eq!(h.missing_drain + h.missing_vent, 0);
    assert_eq!(h.bulk_routes_missing, 1, "{h:?}");
    assert_eq!(h.outcome().status, Status::Unverified);
}

#[test]
fn side_facing_mouths_are_found_and_clearance_is_local_on_each_axis() {
    for axis in 0..3 {
        // Cyclic coordinate permutation preserves outward winding.
        let rotate = |p: V| {
            let mut q = [0.0; 3];
            for k in 0..3 {
                q[(axis + k + 1) % 3] = p[k];
            }
            q
        };
        let model: Vec<Tri> = sphere(true).iter().map(|t| t.map(rotate)).collect();
        let mut limits = test_limits();
        limits.mouth_clearance_mm = 5.0;
        let make = |along: f32| {
            let mut a = [0.0_f32; 3];
            let mut b = a;
            a[axis] = along;
            b[axis] = along + 0.4;
            // Clear the bore laterally but enter the 5 mm ball near this mouth.
            a[(axis + 1) % 3] = 3.0;
            b[(axis + 1) % 3] = 3.0;
            kit::part(0, 9, a, b, 0.2, 0.2)
        };
        let s = declared_scene(&limits, model.clone(), [0.0, 0.0, 15.0], vec![make(-5.0)]);
        let h = s.hollow();
        assert_eq!(
            h.bores.iter().filter(|b| b.axis == axis).count(),
            2,
            "axis {axis}: {h:?}"
        );
        assert!(
            h.intrusion
                .iter()
                .any(|l| l.reason.contains("local mouth keep-out")),
            "axis {axis}: {h:?}"
        );
        let s = declared_scene(&limits, model, [0.0, 0.0, 15.0], vec![make(-11.0)]);
        assert!(
            s.hollow().intrusion.is_empty(),
            "axis {axis}: {:?}",
            s.hollow()
        );
    }
}

// Boundary of a union of rectilinear cells. Internal faces are omitted, so the leg
// really joins the shell rather than adding an intersecting/disconnected surface.
fn recessed_box(holes: &[[f64; 2]], leg: bool, through: bool) -> Vec<Tri> {
    box_with_recess(holes, leg, through, false)
}

fn box_with_recess(holes: &[[f64; 2]], leg: bool, through: bool, recess: bool) -> Vec<Tri> {
    let mut xy = vec![-7.0, -5.0, 5.0, 7.0];
    for &[centre, half] in holes {
        xy.extend([centre - half, centre + half, -half, half]);
    }
    xy.sort_by(f64::total_cmp);
    xy.dedup();
    if recess {
        xy.push(0.0);
        xy.sort_by(f64::total_cmp);
        xy.dedup();
    }
    let zs: [f64; 6] = [-14.0, -4.0, -3.0, -2.0, 2.0, 4.0];
    let n = xy.len() - 1;
    let material = |x: usize, y: usize, z: usize| {
        let (a, b, h) = (
            (xy[x] + xy[x + 1]) * 0.5,
            (xy[y] + xy[y + 1]) * 0.5,
            (zs[z] + zs[z + 1]) * 0.5,
        );
        if h < -4.0 {
            return leg && a > 5.0 && b > 5.0;
        }
        if recess && h < -3.0 && a > 0.0 {
            return false;
        }
        let void = a.abs() < 5.0 && b.abs() < 5.0 && (through || h.abs() < 2.0);
        let hole = holes.iter().any(|&[c, r]| (a - c).abs() < r && b.abs() < r);
        !void && !hole
    };
    let dirs = [
        (2, false),
        (2, true),
        (1, false),
        (1, true),
        (0, false),
        (0, true),
    ];
    let mut out = Vec::new();
    for z in 0..zs.len() - 1 {
        for y in 0..n {
            for x in 0..n {
                if !material(x, y, z) {
                    continue;
                }
                let cell =
                    kit::closed_box([xy[x], xy[y], zs[z]], [xy[x + 1], xy[y + 1], zs[z + 1]]);
                for (face, &(axis, up)) in dirs.iter().enumerate() {
                    let mut p = [x, y, z];
                    let end = [n, n, zs.len() - 1][axis];
                    let neighbour = if up {
                        (p[axis] + 1 < end).then_some(p[axis] + 1)
                    } else {
                        p[axis].checked_sub(1)
                    };
                    let covered = neighbour.is_some_and(|k| {
                        p[axis] = k;
                        material(p[0], p[1], p[2])
                    });
                    if !covered {
                        out.extend_from_slice(&cell[2 * face..2 * face + 2]);
                    }
                }
            }
        }
    }
    out
}

#[test]
fn a_recessed_mouth_and_its_throat_do_not_extend_to_the_leg_extreme() {
    let make = |part| {
        declared_scene(
            &test_limits(),
            recessed_box(&[[0.0, 1.5]], true, false),
            [0.0, 0.0, 20.0],
            vec![part],
        )
    };
    // 6–10 mm below the actual hole, along its axis, but outside the local ball.
    let clear = make(kit::part(
        0,
        3,
        [0.0, 0.0, -14.0],
        [0.0, 0.0, -10.0],
        0.2,
        0.2,
    ));
    assert!(clear.model_closed);
    let h = clear.hollow();
    let bottom = h
        .bores
        .iter()
        .find(|b| b.axis == 2 && b.mouth[2] < 0.0)
        .expect("bottom mouth");
    assert!((bottom.mouth[2] + 4.0).abs() < 1e-8, "{bottom:?}");
    assert_eq!(
        bottom.span.map(f64::to_bits),
        [-4.0_f64, -2.0].map(f64::to_bits)
    );
    assert!(h.intrusion.is_empty(), "{h:?}");
    let near = make(kit::part(
        0,
        9,
        [3.0, 0.0, -4.2],
        [3.0, 0.0, -3.8],
        0.1,
        0.1,
    ));
    assert!(
        near.hollow()
            .intrusion
            .iter()
            .any(|l| l.reason.contains("local mouth keep-out")),
        "{:?}",
        near.hollow()
    );
    let bore = make(kit::part(
        0,
        3,
        [0.0, 0.0, -3.7],
        [0.0, 0.0, -2.3],
        0.2,
        0.2,
    ));
    assert!(
        bore.hollow()
            .intrusion
            .iter()
            .any(|l| l.reason.contains("detected bore")),
        "{:?}",
        bore.hollow()
    );
}

#[test]
fn minimum_width_is_a_violation_even_when_point_routes_are_open() {
    for (half, status, short) in [(1.0, Status::Violation, 2), (1.5, Status::Unverified, 0)] {
        let s = declared_scene(
            &test_limits(),
            recessed_box(&[[0.0, half]], false, false),
            [0.0, 0.0, 5.0],
            vec![],
        );
        let h = s.hollow();
        assert_eq!(h.bores.len(), 2, "{h:?}");
        assert_eq!(h.widths_short, short, "{h:?}");
        assert_eq!(
            h.missing_drain + h.missing_vent,
            0,
            "must isolate width failure"
        );
        assert_eq!(crate::h5::measure(&s).status, status, "{h:?}");
    }
}

#[test]
fn topology_uncertainty_cannot_hide_a_measured_intrusion() {
    let mut model = recessed_box(&[[0.0, 1.5]], true, false);
    // A tiny remote closed component makes topology arithmetic undecided, without
    // crossing either throat or supplying the intrusion witness.
    let tiny = kit::closed_box([8.0, 0.0, 0.0], [8.000_01, 0.000_01, 0.000_01]);
    #[allow(clippy::cast_possible_truncation, reason = "emitted f32 fixture")]
    model.extend(
        tiny.into_iter()
            .map(|t| t.map(|p| p.map(|v| f64::from(v as f32)))),
    );
    let s = declared_scene(
        &test_limits(),
        model,
        [0.0, 0.0, 20.0],
        vec![kit::part(
            0,
            3,
            [0.0, 0.0, -3.7],
            [0.0, 0.0, -2.3],
            0.2,
            0.2,
        )],
    );
    let t = crate::h5::analyze(&s).expect("topology");
    assert_eq!(t.intersections, 0, "{t:?}");
    assert!(
        t.ambiguous > 0,
        "must hit the unclear-topology branch: {t:?}"
    );
    assert!(!s.hollow().intrusion.is_empty());
    let out = crate::h5::measure(&s);
    assert_eq!(out.status, Status::Violation, "{out:?}");
    assert!(out.located.iter().any(|l| l.location.record == Some(0)));
    assert!(out.detail.get("topology").is_some());
    assert!(out.detail.get("hollow").is_some());
}

#[test]
fn distinct_mouth_requirement_is_exercised_by_a_single_opening() {
    let s = scene(false, vec![]);
    assert_eq!(s.hollow().bores.len(), 1);
    assert_eq!(s.hollow().distinct_mouths_missing, 1);
    assert_eq!(scene(true, vec![]).hollow().distinct_mouths_missing, 0);
}

#[test]
fn two_openings_on_one_layer_are_not_averaged_into_the_wall() {
    let s = declared_scene(
        &test_limits(),
        recessed_box(&[[-2.5, 1.5], [2.5, 1.5]], false, false),
        [0.0, 0.0, 5.0],
        vec![],
    );
    assert!(s.model_closed);
    let h = s.hollow();
    assert_eq!(h.bores.len(), 4, "{h:?}");
    assert!(
        h.bores
            .iter()
            .all(|b| (b.mouth[0].abs() - 2.5).abs() < CELL_MM),
        "{h:?}"
    );
    assert_eq!(h.outcome().status, Status::Unverified);
}

#[test]
fn an_opening_off_the_chambers_extreme_layer_is_explicitly_unverified() {
    let s = declared_scene(
        &test_limits(),
        box_with_recess(&[[-2.5, 1.5], [2.5, 1.5]], false, false, true),
        [0.0, 0.0, 5.0],
        vec![],
    );
    assert!(s.model_closed);
    let h = s.hollow();
    assert!(
        !h.bores.is_empty(),
        "must exercise a partial, not empty inventory"
    );
    assert!(h.discovery_incomplete, "{h:?}");
    assert_eq!(h.outcome().status, Status::Unverified, "{h:?}");
    assert!(
        h.outcome()
            .reason
            .contains("opening not on the chamber's extreme layer"),
        "{h:?}"
    );
    assert_eq!(crate::h5::measure(&s).status, Status::Unverified);
}

#[test]
fn a_straight_through_bore_is_explicitly_unverified_not_clean() {
    let s = declared_scene(
        &test_limits(),
        recessed_box(&[], false, true),
        [0.0, 0.0, 5.0],
        vec![],
    );
    assert!(s.model_closed);
    let h = s.hollow();
    assert!(h.bores.is_empty(), "{h:?}");
    assert_eq!(h.outcome().status, Status::Unverified);
    assert!(h.outcome().reason.contains("zero mouths found"));
    assert!(h.outcome().reason.contains("straight through-bore"));
}

#[test]
fn undeclared_hollow_keeps_the_old_topological_exemption_without_a_search() {
    let s = Scene::from_parts(&test_limits(), sphere(false), [0.0, 0.0, 5.0], vec![]);
    assert_eq!(crate::h5::measure(&s).status, Status::NotApplicable);
    assert!(!s.wholly_inside(0));
    let p = crate::patch::measure(&s);
    let _ = crate::h12::h1(&s, &p);
    let _ = crate::h12::h2(&s, &p);
    let _ = crate::h6::measure(&s, &p);
    assert!(s.hollow_analysis_unused());
}

#[test]
fn sealed_cavity_fails_but_a_solid_convex_control_is_not_applicable() {
    let mut m = kit::closed_box([-4.0; 3], [4.0; 3]);
    m.extend(
        kit::closed_box([-2.0; 3], [2.0; 3])
            .into_iter()
            .map(|[a, b, c]| [a, c, b]),
    );
    let s = declared_scene(&test_limits(), m, [0.0, 0.0, 5.0], vec![]);
    assert_eq!(crate::h5::measure(&s).status, Status::Violation);
    let s = Scene::from_parts(
        &test_limits(),
        kit::closed_box([-4.0; 3], [4.0; 3]),
        [0.0, 0.0, 5.0],
        vec![],
    );
    assert_eq!(crate::h5::measure(&s).status, Status::NotApplicable);
}

#[test]
fn declaring_convex_solid_material_hollow_is_an_explicit_inconsistency() {
    let s = declared_scene(
        &test_limits(),
        kit::closed_box([-4.0; 3], [4.0; 3]),
        [0.0, 0.0, 5.0],
        vec![],
    );
    let out = crate::h5::measure(&s);
    assert_eq!(out.status, Status::Unverified, "{out:?}");
    assert!(
        out.reason
            .contains("declared hollow but no cavity was found")
    );
    assert!(out.reason.contains("zero mouths found"));
}
