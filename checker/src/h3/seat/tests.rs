use super::super::measure;
use super::*;
use crate::patch;
use crate::report::Status;
use crate::testkit::{self as kit, test_limits};
use crate::tri::Tri;

/// A closed 10 mm cube centred at the origin lifted 5 mm: the plate is at local z = -10.
fn cube_scene(parts: Vec<Part>) -> Scene {
    Scene::from_parts(
        &test_limits(),
        kit::closed_box([-5.0; 3], [5.0; 3]),
        [0.0, 0.0, 5.0],
        parts,
    )
}

fn outcome_of(s: &Scene) -> crate::outcome::Outcome {
    let p = patch::measure(s);
    measure(s, &p)
}

/// A pad, a radius 0.3 column from the pad to the cube's underside, and a shallow rod of radius
/// 0.052 from `a` to `b`.
fn column_and_shallow_rod(a: [f32; 3], b: [f32; 3]) -> Scene {
    let scene = cube_scene(vec![
        kit::raft_pad(0, [0.0, 0.0], -9.8),
        kit::part(1, 3, [0.0, 0.0, -9.85], [0.0, 0.0, -4.95], 0.3, 0.3),
        kit::part(2, 3, a, b, 0.052, 0.052),
    ]);
    assert!(scene.parts.iter().all(Part::is_valid), "{:?}", scene.parts);
    scene
}

#[test]
fn a_shallow_rod_that_prints_beside_its_carrier_before_it_reaches_it_is_a_violation() {
    // Round-6 review: the rod is 0.052 mm in radius and rises 0.003 mm over its 3 mm. Its
    // lowest 0.0011 mm print a strip of material (wider than a sliver) at x from 0.5 to 3
    // mm, 0.2 mm beside the column (radius 0.3); only a little higher does the strip reach the
    // column. The section where the rod is wide enough to be rooted is held by the column,
    // but the strip under it is in the air.
    let s = column_and_shallow_rod([3.0, 0.0, -6.970], [0.0, 0.0, -6.967]);
    let o = outcome_of(&s);
    assert_eq!(o.status, Status::Violation, "{o:?}");
    assert!(
        o.located.iter().any(|l| l.location.record == Some(2)),
        "{o:?}"
    );
}

#[test]
fn a_rotated_shallow_rod_whose_strip_prints_beside_its_carrier_is_not_a_pass() {
    // Round-6 review: rotated by pi/16, the rod prints a strip wider than a sliver, 0.6 mm
    // from the column, at z -7.0207: the proof starts where the written mesh first prints it.
    let a = std::f32::consts::PI / 16.0;
    let scene = column_and_shallow_rod(
        [3.0 * a.cos(), 3.0 * a.sin(), -6.970],
        [0.0, 0.0, -6.970 + 0.000_4],
    );
    let o = outcome_of(&scene);
    assert_ne!(o.status, Status::Pass, "{o:?}");
}

#[test]
fn the_proof_stretch_never_starts_above_what_the_written_mesh_prints() {
    // Sabotage-visible directly: for the part's start under either reading, the proof stretch
    // starts no higher than the printable start.
    let scene = column_and_shallow_rod([3.0, 0.0, -6.970], [0.0, 0.0, -6.967]);
    let p = &scene.parts[2];
    let seat = first_thick(p, 0.043).expect("a seat");
    let start = printable_start(p, 0.0215, seat);
    assert!(start < seat);
    for reading in [Reading::strict(), Reading::lenient()] {
        let ctx = Ctx {
            scene: &scene,
            infos: scene.parts.iter().map(|p| part_info(p, -10.0)).collect(),
            reading,
            seat_sliver: 0.0,
        };
        let span = ctx.span(2, seat, start).expect("a span");
        assert!(span.start <= start + 1e-12, "{} {start}", span.start);
    }
}

#[test]
fn a_shallow_rod_whose_whole_start_lies_over_its_carrier_is_held() {
    // The same rod from the column's axis out to 0.2 mm: every section of its start lies
    // inside the column's, by more than the rounding.
    let s = column_and_shallow_rod([0.0, 0.0, -6.970], [0.2, 0.0, -6.967]);
    let o = outcome_of(&s);
    assert_eq!(o.status, Status::Pass, "{o:?}");
}

#[test]
fn a_shallow_rod_that_starts_half_over_its_carrier_is_never_a_pass() {
    // Half of the strip lies over the column, half beside it: not shown to stand on the
    // column through its whole first stretch (it is not inside it), so not a pass.
    for x in [0.2f32, 0.3, 0.4, 0.6] {
        let s = column_and_shallow_rod([x + 2.0, 0.0, -6.970], [x - 0.2, 0.0, -6.967]);
        let o = outcome_of(&s);
        assert_ne!(o.status, Status::Pass, "x {x}: {o:?}");
    }
}

#[test]
fn a_shallow_tapered_frustum_is_decided_on_the_written_mesh_and_the_analytic_difference_is_information()
 {
    // Round-7 review: a frustum from [0,0,-6.97] (radius 0.2) to [2.9423556,0.585271,-6.6744885]
    // (radius 0.5) beside a radius-0.6 column on a pad. The analytic solid first appears at the
    // larger-radius end, 7.1720803 mm below the plate-relative origin, while the written 16-gon
    // mesh first appears at -7.1652122: the analytic solid starts 0.0068682 mm lower. The verdict
    // is the written mesh's, and the analytic difference is reported as information only.
    let scene = cube_scene(vec![
        kit::raft_pad(0, [0.0, 0.0], -9.8),
        kit::part(1, 3, [0.0, 0.0, -9.85], [0.0, 0.0, -4.95], 0.6, 0.6),
        kit::part(
            2,
            3,
            [0.0, 0.0, -6.97],
            [2.942_355_6, 0.585_271, -6.674_488_5],
            0.2,
            0.5,
        ),
    ]);
    assert!(scene.parts.iter().all(Part::is_valid), "{:?}", scene.parts);
    let o = outcome_of(&scene);
    assert_eq!(o.status, Status::Pass, "{o:?}");
    let info = o.detail.get("analytic_information").expect("information");
    assert_eq!(
        info.get("decides_nothing"),
        Some(&crate::json::Json::Bool(true))
    );
    let Some(crate::json::Json::Num(d)) = info.get("largest_lowest_point_difference_mm") else {
        panic!("{info:?}");
    };
    assert!((d - 0.006_868_2).abs() < 2e-5, "{d}");
    // The written mesh first prints at the near end, inside the radius-0.6 column; the
    // analytic solid's lower start is at the far end, and is not the verdict's business.
    let low = scene.parts[2]
        .tris
        .iter()
        .flatten()
        .min_by(|a, b| a[2].total_cmp(&b[2]))
        .expect("a vertex");
    assert!((low[2] - (-7.165_212_154_4)).abs() < 1e-6, "{low:?}");
    assert!(low[0].hypot(low[1]) < 0.6, "{low:?}");
}

#[test]
fn the_print_start_is_no_higher_than_the_seat_and_a_flat_start_has_no_thin_stretch() {
    let rod = kit::part(1, 3, [0.0, 0.0, -9.0], [0.0, 0.0, -6.0], 0.3, 0.3);
    let seat = first_thick(&rod, 0.043).expect("a seat");
    let start = printable_start(&rod, 0.0215, seat);
    assert!(start <= seat);
    assert!((seat - start).abs() < 1e-9, "{start} {seat}");
    // A ball starts at a point: its first printing height is below its seat.
    let ball = kit::part(2, 9, [0.0, 0.0, -9.3], [0.0, 0.0, -8.7], 0.3, 0.3);
    let seat = first_thick(&ball, 0.043).expect("a seat");
    let start = printable_start(&ball, 0.0215, seat);
    assert!(start < seat, "{start} {seat}");
    assert!(width_at(&ball, start) >= 0.0215 - 1e-12);
    assert!(width_at(&ball, start - 1e-4) < 0.0215);
}

#[test]
fn the_overlap_depth_of_two_sections_is_concave_in_the_height() {
    // The proof settles a stretch from its two ends: the depth at the middle is at least the
    // mean of the depths at the ends, for a shallow rod and a column.
    let col = kit::part(1, 3, [0.0, 0.0, -9.0], [0.0, 0.0, -5.0], 0.3, 0.3);
    let rod = kit::part(2, 3, [1.0, 0.0, -7.2], [-0.5, 0.0, -6.8], 0.2, 0.2);
    let depth = |z: f64| {
        let (a, b): (Option<Sec>, Option<Sec>) =
            (section(&col.tris, z, 0.0), section(&rod.tris, z, 0.0));
        pen_depth(&a.expect("column"), &b.expect("rod"))
    };
    let tris: &[Tri] = &rod.tris;
    let (lo, hi) = tris
        .iter()
        .flatten()
        .fold((f64::MAX, f64::MIN), |(l, h), v| (l.min(v[2]), h.max(v[2])));
    let (z1, z2) = (lo + 0.02, hi - 0.02);
    for k in 1..20 {
        let t = f64::from(k) / 20.0;
        let mid = depth((1.0 - t) * z1 + t * z2);
        let chord = (1.0 - t) * depth(z1) + t * depth(z2);
        assert!(mid >= chord - 1e-9, "t {t}: {mid} < {chord}");
    }
}
