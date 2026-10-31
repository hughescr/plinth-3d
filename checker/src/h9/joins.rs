//! H9 on end-face gaps, necks, tiny balls, rafts and joins of overlapping parts, measured on
//! the written meshes.

use super::tests::run;
use super::*;
use crate::report::Status;
use crate::testkit as kit;

#[test]
fn an_end_face_gap_is_never_a_join() {
    // Round-4 review: two canonical radius 0.3 rods on the z axis, [-2, 0] and [9e-7, 2],
    // have an end-face gap of 9e-7 mm in the meshes. Growing both by 2e-6 mm manufactured
    // a join and a third measurement. Contact is established on the written bodies as they
    // are: a gap is no join, so the rods are two separate fat parts (two measurements).
    let rods = |start_of_second: f32| {
        vec![
            kit::part(0, 3, [0.0, 0.0, -2.0], [0.0, 0.0, 0.0], 0.3, 0.3),
            kit::part(1, 3, [0.0, 0.0, start_of_second], [0.0, 0.0, 2.0], 0.3, 0.3),
        ]
    };
    for gap in [9e-7f32, 1e-6, 5e-6, 1e-4] {
        let o = run(rods(gap));
        assert_eq!(o.status, Status::Pass, "gap {gap}: {o:?}");
        let Some(Json::Uint(n)) = o.detail.get("measured") else {
            panic!("no count in {o:?}");
        };
        assert_eq!(*n, 2, "no join across a gap, gap {gap}: {o:?}");
    }
    // Faces that meet exactly, and rods that overlap by the same amount, are a fat join.
    for start in [0.0f32, -9e-7] {
        let o = run(rods(start));
        assert_eq!(o.status, Status::Pass, "start {start}: {o:?}");
        let Some(Json::Uint(n)) = o.detail.get("measured") else {
            panic!("no count in {o:?}");
        };
        assert_eq!(*n, 3, "two sections and one join: {o:?}");
    }
    // Far enough apart (over the polygon sagitta) they are two separate parts.
    let apart = run(rods(0.05));
    assert_eq!(apart.status, Status::Pass, "{apart:?}");
    let Some(Json::Uint(n)) = apart.detail.get("measured") else {
        panic!("no count in {apart:?}");
    };
    assert_eq!(*n, 2, "no join between separate parts: {apart:?}");
}

#[test]
fn the_neck_of_a_ball_and_a_flaring_cone_is_scanned_through_the_transition() {
    // Ball R 0.2 with a coaxial frustum r(t) = 0.012 + 0.2 t from its centre: the centreline
    // exit (t = 0.2) is 0.104 across the radii but the union is narrower at the crossing
    // near t = 0.19: under 0.1 on the written mesh.
    let ball = kit::part(0, 9, [0.0, 0.0, 0.2], [0.0, 0.0, -0.2], 0.2, 0.2);
    let cone = kit::part(1, 1, [0.0, 0.0, 0.0], [0.0, 0.0, -2.0], 0.012, 0.412);
    let o = run(vec![ball, cone]);
    assert_eq!(o.status, Status::Violation, "{o:?}");
    let Some(Json::Num(w)) = o.detail.get("thinnest_width_mm") else {
        panic!("no thinnest width in {o:?}");
    };
    assert!((0.098..0.1).contains(w), "{w}");
    // A fatter cone (r(t) = 0.06 + 0.2 t) is fine.
    let ball2 = kit::part(0, 9, [0.0, 0.0, 0.2], [0.0, 0.0, -0.2], 0.2, 0.2);
    let cone2 = kit::part(1, 1, [0.0, 0.0, 0.0], [0.0, 0.0, -2.0], 0.06, 0.46);
    let ok = run(vec![ball2, cone2]);
    assert_eq!(ok.status, Status::Pass, "{ok:?}");
}

#[test]
fn a_tiny_joint_ball_and_a_thin_raft_are_violations() {
    let ball = kit::part(0, 9, [0.0, 0.0, -0.04], [0.0, 0.0, 0.04], 0.04, 0.04);
    assert_eq!(run(vec![ball]).status, Status::Violation);
    let thin = kit::part_with_mesh(
        0,
        &kit::rec(6, [0.0; 3], [0.0; 3], 2.2, 0.08),
        &kit::pad_coords([0.0, 0.0], -1.0, -1.08, 1.1),
    );
    assert_eq!(run(vec![thin]).status, Status::Violation);
    let ok = kit::raft_pad(0, [0.0, 0.0], -1.0);
    assert_eq!(run(vec![ok]).status, Status::Pass);
}

#[test]
fn a_tip_ball_alone_is_a_tip_not_a_part() {
    let ball = kit::part(0, 7, [0.0, 0.0, -0.03], [0.0, 0.0, 0.03], 0.03, 0.03);
    let o = run(vec![ball]);
    assert_eq!(o.status, Status::Pass, "{o:?}");
}

#[test]
fn a_rod_just_over_the_limit_standing_in_a_raft_pad_is_certified() {
    // 0.05099 is 0.100022 across flats: the join with the pad must be certified too.
    let pad = kit::raft_pad(0, [4.0, 4.0], -9.8);
    let rod = kit::part(
        1,
        3,
        [4.0, 4.0, -9.802],
        [4.0, 4.0, -7.0],
        0.050_99,
        0.050_99,
    );
    let o = run(vec![pad, rod]);
    assert_eq!(o.status, Status::Pass, "{o:?}");
    let low = kit::part(
        1,
        3,
        [4.0, 4.0, -9.802],
        [4.0, 4.0, -7.0],
        0.050_97,
        0.050_97,
    );
    let pad = kit::raft_pad(0, [4.0, 4.0], -9.8);
    assert_eq!(run(vec![pad, low]).status, Status::Violation);
}

fn two_rods(offset: f32) -> Vec<Part> {
    vec![
        kit::part(0, 3, [0.0, 0.0, 0.0], [0.0, 0.0, -5.0], 0.3, 0.3),
        kit::part(1, 3, [offset, 0.0, -0.5], [offset, 0.0, -4.5], 0.3, 0.3),
    ]
}

#[test]
fn two_thick_rods_joined_by_a_thin_offset_lens_are_a_violation() {
    // Two radius 0.3 rods side by side, 0.59 mm apart: they share a lens a few hundredths
    // of a millimetre across, far thinner than either rod.
    let o = run(two_rods(0.59));
    assert_eq!(o.status, Status::Violation, "{o:?}");
    assert!(o.reason.contains("thinner"), "{}", o.reason);
    // Overlapping deeply (0.5 apart) is a fat join.
    let ok = run(two_rods(0.5));
    assert_eq!(ok.status, Status::Pass, "{ok:?}");
    // Rods that do not touch share no join (each is thick on its own).
    assert_eq!(run(two_rods(0.7)).status, Status::Pass);
}

#[test]
fn a_rod_on_a_flare_and_a_brace_on_a_column_through_a_ball_are_wide_joins() {
    let column = kit::part(0, 3, [0.0, 0.0, 0.0], [0.0, 0.0, -6.0], 0.25, 0.25);
    let flare = kit::part(1, 4, [0.0, 0.0, -5.998], [0.0, 0.0, -7.0], 0.25, 1.1);
    let joint = kit::part(2, 9, [0.0, 0.0, -3.26], [0.0, 0.0, -2.74], 0.26, 0.26);
    let brace = kit::part(3, 3, [0.0, 0.0, -3.0], [2.12, 0.0, -0.88], 0.25, 0.25);
    let o = run(vec![column, flare, joint, brace]);
    assert_eq!(o.status, Status::Pass, "{o:?}");
}
