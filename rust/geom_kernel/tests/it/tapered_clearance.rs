//! A tapered member is measured as the frustum it is, not as a capsule of its
//! widest radius (rule R3, `clearance-census-diagnosis.md`, 2026-10-04).
//!
//! `segment_clearance_classified` subtracts `max(r_lo, r_hi)` for the whole
//! segment, so a face beside the THIN end of a long taper was read at the thick
//! end's radius. The clearance of a frustum to a face is
//! `min over s of (distance(axis(s), face) - r(s))`, `r` linear along the axis.

use geom_kernel::testkit::build_from_tris;
use geom_kernel::SegmentClearanceClassified;

fn measured_mm(row: SegmentClearanceClassified) -> f64 {
    match row {
        SegmentClearanceClassified::Measured { clearance_mm, .. } => clearance_mm,
        other => panic!("expected a measured row, got {other:?}"),
    }
}

/// The axis runs from `a = (0, 0, 6)` (radius 0.3) down to `b = (0, 0, 0)`
/// (radius 0.9): the radius is `0.3 + 0.1 (6 - z)`.
const A: [f64; 3] = [0.0, 0.0, 6.0];
const B: [f64; 3] = [0.0, 0.0, 0.0];

/// A wall at `x = 1` that stands beside the upper (thin) part of the axis only:
/// the triangle covers `z` in `[4, 8]` on the line `y = 0`.
fn wall_beside_the_thin_end() -> geom_kernel::NativeSurface {
    build_from_tris(vec![
        1.0, -2.0, 4.0, 1.0, 2.0, 4.0, 1.0, 0.0, 8.0, //
    ])
}

/// The same wall beside the lower (thick) part: `z` in `[-2, 2]`, so the axis
/// meets it only down to `z = 0`, where the radius is 0.9.
fn wall_beside_the_thick_end() -> geom_kernel::NativeSurface {
    build_from_tris(vec![
        1.0, -2.0, -2.0, 1.0, 2.0, -2.0, 1.0, 0.0, 2.0, //
    ])
}

fn one(surface: &geom_kernel::NativeSurface, p0: [f64; 3], p1: [f64; 3], r0: f64, r1: f64) -> f64 {
    measured_mm(
        surface
            .segment_frustum_clearance_classified(&[p0], &[p1], &[r0], &[r1], 0.0, &[0.0], &[0.0])
            .unwrap()[0],
    )
}

/// RED before the fix: the kernel returned `1.0 - 0.9 = 0.1`.
///
/// The nearest axis point to the wall's bottom edge is not the edge's own
/// height: the radius grows by 0.1 mm per mm of descent, so the cost
/// `sqrt(1 + x^2) - 0.5 - 0.1 x` (x mm below the edge) is least at
/// `x = 0.1 / sqrt(1 - 0.01)`, where it is `sqrt(1 - 0.01) - 0.5`.
#[test]
fn a_face_beside_the_thin_end_is_read_at_the_radius_there() {
    let surface = wall_beside_the_thin_end();
    let expected = (1.0_f64 - 0.01).sqrt() - 0.5;
    let got = one(&surface, A, B, 0.3, 0.9);
    assert!(
        (got - expected).abs() < 1e-9,
        "the frustum clears the wall by {expected}; the kernel read {got} (the widest-end reading is 0.1)"
    );
}

/// The same member drawn the other way up (thick end at `a`) is the same
/// solid, so the same number: the reading cannot depend on which end is first.
#[test]
fn the_reading_does_not_depend_on_which_end_is_first() {
    let surface = wall_beside_the_thin_end();
    let forward = one(&surface, A, B, 0.3, 0.9);
    let backward = one(&surface, B, A, 0.9, 0.3);
    assert!(
        (forward - backward).abs() < 1e-9,
        "{forward} against {backward}"
    );
}

/// A face beside the THICK end is read at the thick radius: no relaxation
/// where the solid really is wide. (Also true before the fix.)
#[test]
fn a_face_beside_the_thick_end_is_still_read_at_the_thick_radius() {
    let surface = wall_beside_the_thick_end();
    let got = one(&surface, A, B, 0.3, 0.9);
    assert!((got - 0.1).abs() < 1e-9, "1.0 - 0.9 at the foot: {got}");
}

/// The frustum is inside its widest capsule, so the exact reading is never
/// stricter than the old one, and a constant radius is unchanged.
#[test]
fn the_frustum_never_reads_tighter_than_the_widest_capsule_and_a_constant_radius_is_unchanged() {
    for surface in [wall_beside_the_thin_end(), wall_beside_the_thick_end()] {
        let widest_capsule = one(&surface, A, B, 0.9, 0.9);
        let frustum = one(&surface, A, B, 0.3, 0.9);
        assert!(
            frustum >= widest_capsule - 1e-12,
            "{frustum} against {widest_capsule}"
        );
        // A constant radius is the plain distance minus the radius, 1.0 here
        // (the wall stands 1 mm off the axis).
        assert!((widest_capsule - 0.1).abs() < 1e-12, "{widest_capsule}");
    }
}

/// The insets are lengths along the axis from each end, and the radius at a
/// clipped end is the radius THERE, not the root's. `b` is at `z = 0`, so an
/// inset of 2 mm at `b` ends the axis at `z = 2`, which still holds the
/// minimum (at `z = 3.9`, `r = 0.51`): the unclipped reading. An inset of 4 mm
/// ends it at `z = 4`, the wall's bottom edge, where `r = 0.5`: the reading is
/// exactly `1.0 - 0.5`.
#[test]
fn an_inset_clips_the_frustum_at_its_own_radius() {
    let surface = wall_beside_the_thin_end();
    let read = |inset: f64| {
        measured_mm(
            surface
                .segment_frustum_clearance_classified(
                    &[A],
                    &[B],
                    &[0.3],
                    &[0.9],
                    0.0,
                    &[0.0],
                    &[inset],
                )
                .unwrap()[0],
        )
    };
    let unclipped = (1.0_f64 - 0.01).sqrt() - 0.5;
    assert!((read(2.0) - unclipped).abs() < 1e-9, "{}", read(2.0));
    assert!((read(4.0) - 0.5).abs() < 1e-9, "{}", read(4.0));
}

/// The capsule entry point is NOT changed: it still reads a tapered row at the
/// wider radius all along (`1.0 - 0.9`), which is the reading the support
/// engine's brace, rewire and CCO gates were built and pinned against (changing
/// it turned 16 `plinth-support` unit tests red, measured 2026-10-04: a
/// different brace field lowers to `RootRunTooShort`). Only the frustum entry
/// point reads the drawn solid. A constant radius reads the same on both,
/// bit for bit.
#[test]
fn the_capsule_entry_point_still_reads_the_wider_radius_and_agrees_on_a_constant_radius() {
    let surface = wall_beside_the_thin_end();
    let capsule = measured_mm(
        surface
            .segment_clearance_classified(&[A], &[B], &[0.3], &[0.9], 0.0, &[0.0], &[0.0])
            .unwrap()[0],
    );
    assert!((capsule - 0.1).abs() < 1e-12, "capsule reading: {capsule}");
    for r in [0.05, 0.4, 0.9] {
        let plain = surface
            .segment_clearance_classified(&[A], &[B], &[r], &[r], 0.3, &[0.0], &[0.0])
            .unwrap();
        let frustum = surface
            .segment_frustum_clearance_classified(&[A], &[B], &[r], &[r], 0.3, &[0.0], &[0.0])
            .unwrap();
        assert_eq!(
            plain, frustum,
            "a constant radius {r} reads the same on both entry points"
        );
    }
}
