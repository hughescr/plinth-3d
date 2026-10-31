//! H9: the union width upper bound spans all the section polygons, and every bound carries the
//! record's measured deviation from its canonical mesh (round-8 review, findings 1 and 2).

use super::tests::run;
use super::*;
use crate::report::Status;
use crate::testkit::{self as kit, test_limits};

#[test]
fn the_union_extent_is_taken_over_all_the_polygons_not_the_widest_one() {
    // Two unit squares whose centres differ by (0.5, 0.5). Each is 1 across, and the largest
    // single extent along x or y is 1, but the union spans 1.5 along x and along y, and
    // 1.414 along the diagonal: the width of the union's hull is 1.414, not 1.
    let sq = |x: f64, y: f64| vec![[x, y], [x + 1.0, y], [x + 1.0, y + 1.0], [x, y + 1.0]];
    let w = union_width_upper(&[sq(0.0, 0.0), sq(0.5, 0.5)]);
    assert!((w - std::f64::consts::SQRT_2).abs() < 1e-12, "{w}");
    // One polygon alone: its own width.
    assert!((union_width_upper(&[sq(0.0, 0.0)]) - 1.0).abs() < 1e-12);
}

/// Round-8 review: a ball whose centre is 0.0009 mm off the cone's axis (accepted as an axial
/// partner, which allows up to 0.001) and a cone that widens from 0.0249 to 0.8249 mm across.
/// At axial position 0.1925 the written sections are 0.10015 mm across as a union; the
/// largest single polygon span is under 0.1, so the old bound called it thin.
fn off_axis_pair() -> Vec<Part> {
    vec![
        kit::part(0, 9, [0.0009, 0.0, 10.2], [0.0009, 0.0, 9.8], 0.2, 0.2),
        kit::part(
            1,
            1,
            [0.0, 0.0, 10.0],
            [0.0, 0.0, 8.0],
            0.012_468_356,
            0.412_468_34,
        ),
    ]
}

#[test]
fn a_ball_a_little_off_the_axis_does_not_make_a_valid_union_too_thin() {
    let scene = Scene::from_parts(&test_limits(), Vec::new(), [0.0; 3], off_axis_pair());
    assert!(scene.parts.iter().all(Part::is_valid), "both are canonical");
    let axis = axis_of(&scene, &scene.parts[1]).expect("the cone has an axis");
    assert_eq!(axis.balls.len(), 1, "the ball is an axial partner");
    let t = 0.192_535_421_65;
    let written = axis.ub_written(t);
    assert!(
        written >= 0.100_14,
        "the union of the written sections is 0.10015 across, got {written}"
    );
    assert!(
        axis.ub(t) >= written,
        "the bound only grows by the allowances"
    );
    let o = run(off_axis_pair());
    assert_ne!(o.status, Status::Violation, "{o:?}");
}

/// The radius of a canonical 16-gon rod that is `w` across its flats.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the file's precision is f32"
)]
fn radius_for_flats(w: f64) -> f32 {
    (0.5 * w / (std::f64::consts::PI / 16.0).cos()) as f32
}

/// A rod at z 200..202 of `w` across flats, whose written mesh is moved sideways by
/// `ulps` units in the last place of its largest coordinate (validation allows 2).
#[allow(
    clippy::cast_possible_truncation,
    reason = "the file's precision is f32"
)]
fn shifted_rod(w: f64, ulps: f64) -> Part {
    let (a, b, r) = ([0.0, 0.0, 200.0], [0.0, 0.0, 202.0], radius_for_flats(w));
    let mut coords = kit::frustum_coords(a, b, r, r);
    let shift = (ulps * crate::canon::ulp32(202.0)) as f32;
    for v in coords.chunks_exact_mut(3) {
        v[0] += shift;
    }
    kit::part_with_mesh(0, &kit::rec(3, a, b, r, r), &coords)
}

#[test]
fn a_width_within_the_record_deviation_of_the_limit_is_unverified_not_decided() {
    // Round-8 review: nominal 0.10001 across flats at z = 200, written mesh moved 1.75 units
    // in the last place sideways (2.67e-5 mm, accepted). The written mesh is 0.10001 across, but
    // the canonical mesh is within that deviation, so the lower bound 0.100009 less twice
    // the deviation is under the limit: Unverified.
    let part = shifted_rod(0.100_01, 1.75);
    assert!(part.is_valid(), "validation allows it: {:?}", part.invalid);
    let dev = part.mesh.as_ref().map_or(f64::NAN, Solid::dev);
    assert!(
        (2.0e-5..3.5e-5).contains(&dev),
        "the measured deviation is the shift: {dev}"
    );
    let o = run(vec![part]);
    assert_eq!(o.status, Status::Unverified, "{o:?}");
    // Clear of the deviation either way: decided.
    let wide = run(vec![shifted_rod(0.1003, 1.75)]);
    assert_eq!(wide.status, Status::Pass, "{wide:?}");
    let thin = run(vec![shifted_rod(0.0997, 1.75)]);
    assert_eq!(thin.status, Status::Violation, "{thin:?}");
}
