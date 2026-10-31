//! H9 on frustums whose written rings are turned against each other, or moved sideways, by
//! what validation allows (two units in the last place per coordinate). The width of a section
//! is then not the blend of the canonical ring widths, and the certified lower bound must hold
//! for the mesh as written (round-8 review of the round-7 interpolation).

use super::*;
use crate::canon::ulp32;
use crate::report::Status;
use crate::testkit::{self as kit, test_limits};

/// A rod from `a` to `b` (both on the z axis) of radius `r`, whose start ring is turned about
/// the z axis by `turn` radians and whose end ring by `-turn`; both rings stay regular.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the file's precision is f32"
)]
fn twisted_rod(a: [f32; 3], b: [f32; 3], r: f32, turn: f64) -> Part {
    let mut coords = kit::frustum_coords(a, b, r, r);
    for v in coords.chunks_exact_mut(3) {
        let th = if v[2].to_bits() == a[2].to_bits() {
            turn
        } else {
            -turn
        };
        let (x, y) = (f64::from(v[0]), f64::from(v[1]));
        v[0] = (x * th.cos() - y * th.sin()) as f32;
        v[1] = (x * th.sin() + y * th.cos()) as f32;
    }
    kit::part_with_mesh(0, &kit::rec(3, a, b, r, r), &coords)
}

fn scene_of(part: Part) -> Scene {
    Scene::from_parts(&test_limits(), Vec::new(), [0.0; 3], vec![part])
}

/// The width of the written mesh's section at axial position `t` (the exact width of the
/// convex section polygon).
fn section_width(p: &Part, t: f64) -> f64 {
    let Geom::Frust { a, b, .. } = p.geom else {
        panic!("a frustum");
    };
    hull_width(&section_poly(&p.tris, a, unit(sub(b, a)), t))
}

#[test]
fn rings_twisted_by_the_validation_allowance_inside_the_build_volume_do_not_pass() {
    // Round-8 review, scaled into the build volume: the rod stands at z 200 to 202, where two
    // units in the last place of a coordinate are 3.05e-5 mm. The rings are turned by
    // +/- 1.8 of those units at their vertices (a turn of 5.4e-4 radian), which validation
    // accepts. The middle section is then narrower than either ring: the vertices of the
    // blend of two turned rings lie on a smaller circle. Among radii a unit in the last place
    // apart, the ones whose turned rod is under the limit in the middle while the straight rod
    // is over it must never pass. They are under the limit by less than the record's deviation
    // (the turn is the deviation, 3e-5 mm), which every bound carries (ruling a), so they are
    // Unverified, not decided either way.
    let (a, b) = ([0.0, 0.0, 200.0], [0.0, 0.0, 202.0]);
    let turn = 1.8 * ulp32(202.0) / 0.051;
    let first = (0.100_000_001 / (2.0 * (std::f64::consts::PI / 16.0).cos())) as f32;
    let (mut found, mut straight_not_thin) = (0, 0);
    for k in 0..40 {
        let r = f32::from_bits(first.to_bits() + k);
        let (twisted, straight) = (twisted_rod(a, b, r, turn), twisted_rod(a, b, r, 0.0));
        assert!(twisted.is_valid(), "{:?}", twisted.invalid);
        assert!(
            twisted.canon_ulps <= 2.0 && twisted.canon_ulps > 1.0,
            "{}",
            twisted.canon_ulps
        );
        if section_width(&twisted, 1.0) >= 0.1 {
            continue;
        }
        found += 1;
        let o = measure(&scene_of(twisted));
        assert_eq!(o.status, Status::Unverified, "radius {r}: {o:?}");
        // The straight rod of the same radius is never a violation of the turn; it is over the
        // limit by less than the arithmetic allowance or clear of it, so a pass or undecided.
        if section_width(&straight, 1.0) >= 0.1 {
            straight_not_thin += 1;
            let s = measure(&scene_of(straight));
            assert_ne!(s.status, Status::Violation, "radius {r}: {s:?}");
        }
    }
    assert!(
        found >= 3,
        "radii with a turned rod under the limit: {found}"
    );
    assert!(
        straight_not_thin >= 1,
        "radii that separate the turned rod from the straight one: {straight_not_thin}"
    );
}

#[test]
fn the_reviewers_turned_rings_at_z_3000_are_not_certified() {
    // The round-8 reviewer's rod (z 3000 to 3002, radii 0.100002 across flats, rings turned by
    // +/- 0.009 radian): the middle section is 0.099998 mm across. Files that far from the
    // build volume are refused as files; the bound itself holds there too. The turn is a
    // deviation of 0.009 * 0.051 mm from the canonical rings, far more than the 2e-6 mm the
    // section is under the limit, so it is Unverified.
    let (a, b) = ([0.0, 0.0, 3000.0], [0.0, 0.0, 3002.0]);
    let r = (0.100_002 / (2.0 * (std::f64::consts::PI / 16.0).cos())) as f32;
    let twisted = twisted_rod(a, b, r, 0.009);
    assert!(twisted.is_valid(), "{:?}", twisted.invalid);
    assert!(section_width(&twisted, 1.0) < 0.1);
    let o = measure(&scene_of(twisted));
    assert_eq!(o.status, Status::Unverified, "{o:?}");
}

#[test]
fn the_lower_bound_never_exceeds_a_real_section_for_turned_or_tapered_rings() {
    // For several meshes, the certified lower bound over an interval must stay under the width
    // of the real section at every position inside it.
    let sections = |p: &Part| -> Vec<(f64, f64)> {
        let scene = scene_of(p.clone());
        let axis = axis_of(&scene, &scene.parts[0]).expect("a frustum");
        let (lo, hi) = axis.span;
        assert!(lo < hi, "a span to measure: {:?}", axis.span);
        let mut out = Vec::new();
        for pieces in [1usize, 2, 7, 64] {
            for i in 0..pieces {
                let (t0, t1) = (
                    lo + (hi - lo) * i as f64 / pieces as f64,
                    lo + (hi - lo) * (i + 1) as f64 / pieces as f64,
                );
                let lb = axis.lb(t0, t1);
                let thinnest = (0..=8)
                    .map(|k| section_width(p, t0 + (t1 - t0) * f64::from(k) / 8.0))
                    .fold(f64::INFINITY, f64::min);
                out.push((lb, thinnest));
            }
        }
        out
    };
    let (a, b) = ([0.0, 0.0, 200.0], [0.0, 0.0, 202.0]);
    let tapered = kit::part(0, 1, [0.0, 0.0, 10.0], [0.5, 0.2, 8.0], 0.06, 0.4);
    let meshes = [
        twisted_rod(a, b, 0.051, 1.8 * ulp32(202.0) / 0.051),
        twisted_rod([0.0, 0.0, 3000.0], [0.0, 0.0, 3002.0], 0.051, 0.009),
        twisted_rod(a, b, 0.3, 1.8 * ulp32(202.0) / 0.3),
        tapered,
    ];
    for p in &meshes {
        for (lb, real) in sections(p) {
            assert!(lb <= real, "lower bound {lb} over a real section {real}");
        }
    }
}

#[test]
fn a_part_shorter_than_the_flatness_of_its_end_faces_is_undecided_not_passed() {
    // The span of measurable sections is empty when the start ring's highest vertex is not
    // below the end ring's lowest: nothing is certified.
    let scene = scene_of(twisted_rod([0.0, 0.0, 200.0], [0.0, 0.0, 202.0], 0.3, 0.0));
    let mut axis = axis_of(&scene, &scene.parts[0]).expect("a frustum");
    axis.span = (axis.span.1, axis.span.0);
    match axis.scan(0.1) {
        Fate::Undecided { lb, .. } => assert!(lb <= 0.0),
        other => panic!("{other:?}"),
    }
}
