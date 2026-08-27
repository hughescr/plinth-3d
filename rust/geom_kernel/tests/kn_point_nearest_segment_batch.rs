//! Regression guard for the per-point-nearest reduction.
//!
//! THE DEFECT THIS WOULD HAVE CAUGHT. `point_segment_distances_batch` could
//! only return the dense `points x segments` matrix. A caller that wanted
//! "which segment is nearest, and how near" therefore had two bad options:
//! allocate the whole matrix and reduce it, or skip this module and hand-roll
//! the arithmetic. `crates/plinth-support/src/rewire.rs` took the second --
//! `assert_no_stale_braces` (rewire.rs:1143-1152) folds a private
//! `point_segment_distance` (rewire.rs:1202) over every host with
//! `.fold(f64::INFINITY, f64::min)`, and that private copy carries its own
//! `l2 < 1e-18` degeneracy epsilon, a duplicate of the kernel's. Two copies of
//! one predicate drift, and the copy in the caller is the one nobody tests.
//!
//! So the tests below pin the two properties that make the kernel form a safe
//! replacement for the hand-rolled fold:
//!   * its answer is EXACTLY the minimum of the dense form's row, bit for bit,
//!     including for a degenerate segment -- one predicate, not two; and
//!   * an empty segment set yields one `None` per point rather than a float
//!     that compares as "very far", which is the failure mode of the
//!     `f64::INFINITY` seed the fold used.
//!
//! These tests do not re-implement point-to-segment distance. Where a number is
//! asserted it is either read from the dense entry point (the reference this
//! function is defined as a reduction OVER) or it is a distance decided by
//! inspection of the fixture and stated in the comment beside it.

use geom_kernel::{
    point_nearest_segment_batch, point_segment_distances_batch, PointSegmentBatchError,
    PointSegmentBatchLimits,
};

/// Four axis-aligned segments at known offsets from the origin, plus one
/// zero-length segment so the degeneracy path is inside the comparison rather
/// than beside it.
fn segments() -> (Vec<[f64; 3]>, Vec<[f64; 3]>) {
    let starts = vec![
        [10.0, 0.0, 0.0],  // 0: vertical line at x = 10
        [3.0, 0.0, 0.0],   // 1: vertical line at x = 3
        [-7.0, 0.0, 0.0],  // 2: vertical line at x = -7
        [0.0, 25.0, 0.0],  // 3: a single POINT at y = 25 (zero length)
        [0.0, -40.0, 0.0], // 4: vertical line at y = -40
    ];
    let ends = vec![
        [10.0, 0.0, 5.0],
        [3.0, 0.0, 5.0],
        [-7.0, 0.0, 5.0],
        [0.0, 25.0, 0.0],
        [0.0, -40.0, 5.0],
    ];
    (starts, ends)
}

fn points() -> Vec<[f64; 3]> {
    vec![
        [0.0, 0.0, 1.0],
        [9.0, 0.0, 2.0],
        [-6.5, 0.0, 4.0],
        [0.0, 20.0, 0.0],
        [0.5, 0.5, 0.5],
    ]
}

fn limits(points: usize, segments: usize) -> PointSegmentBatchLimits {
    PointSegmentBatchLimits::for_dimensions(points, segments).expect("budget fits")
}

/// The reduction must agree with the matrix it reduces, to the bit.
///
/// This is the property that lets a caller delete its hand-rolled fold without
/// a parity argument: the nearest distance is not merely close to the row
/// minimum, it IS the row minimum, because both come from the same evaluation.
#[test]
fn the_nearest_distance_is_bit_identical_to_the_dense_row_minimum() {
    let (starts, ends) = segments();
    let points = points();
    let dense = point_segment_distances_batch(&points, &starts, &ends, limits(5, 5))
        .expect("dense batch fits its budget");
    let nearest = point_nearest_segment_batch(&points, &starts, &ends, limits(5, 5))
        .expect("reduction fits the same budget");

    assert_eq!(nearest.point_count(), points.len());
    for point in 0..points.len() {
        let row = dense.row(point).expect("one dense row per point");
        let found = nearest.nearest(point).expect("a non-empty segment set has a nearest");
        let minimum = row
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        assert_eq!(
            found.distance_mm().to_bits(),
            minimum.to_bits(),
            "point {point}: the reduction must return the dense row's own minimum, \
             not a separately computed value that happens to be near it"
        );
        assert_eq!(
            row[found.segment()].to_bits(),
            minimum.to_bits(),
            "point {point}: and the reported segment index must be the one holding \
             that minimum"
        );
    }
}

/// The zero-length segment is inside the comparison above, not exempt from it.
///
/// Segment 3 is a single point at (0, 25, 0). Query point 3 is at (0, 20, 0),
/// exactly 5 mm away, and every other segment in the fixture is further. The
/// hand-rolled caller reached this case through its OWN `l2 < 1e-18` branch;
/// here it is reached through the kernel's, and the two must not be able to
/// disagree because there is now only one.
#[test]
fn a_zero_length_segment_is_measured_by_the_same_predicate_as_every_other() {
    let (starts, ends) = segments();
    let points = points();
    let nearest = point_nearest_segment_batch(&points, &starts, &ends, limits(5, 5))
        .expect("reduction fits its budget");

    let found = nearest.nearest(3).expect("a non-empty segment set has a nearest");
    assert_eq!(
        found.segment(),
        3,
        "the degenerate segment is the nearest to this point and must be selectable"
    );
    assert!(
        (found.distance_mm() - 5.0).abs() < 1e-12,
        "(0, 20, 0) to the point (0, 25, 0) is 5 mm, got {}",
        found.distance_mm()
    );
}

/// Ties resolve to the LOWEST segment index, so the answer does not depend on
/// how rayon happened to schedule the batch.
#[test]
fn a_tie_resolves_to_the_lowest_segment_index() {
    // Three segments at exactly the same place. Every distance ties.
    let starts = vec![[2.0, 0.0, 0.0]; 3];
    let ends = vec![[2.0, 0.0, 4.0]; 3];
    let points = vec![[0.0, 0.0, 1.0], [0.0, 0.0, 3.0]];
    let nearest = point_nearest_segment_batch(&points, &starts, &ends, limits(2, 3))
        .expect("reduction fits its budget");

    for point in 0..points.len() {
        let found = nearest.nearest(point).expect("a nearest exists");
        assert_eq!(
            found.segment(),
            0,
            "point {point}: three identical segments must always name the first"
        );
        assert!(
            (found.distance_mm() - 2.0).abs() < 1e-12,
            "point {point}: all three are 2 mm away"
        );
    }
}

/// AN EMPTY SEGMENT SET HAS NO NEAREST SEGMENT, and the batch must say so once
/// per point.
///
/// This is the shape that makes the type safe to hand a caller. The failure it
/// guards against is subtle and was live in one draft of this function: return
/// an EMPTY `nearest` vector for the empty-segment case, so `point_count()`
/// reports 0 for a batch that was handed 100 points, and a caller zipping the
/// rows against its own points silently processes nothing at all. That is the
/// same class of defect as the `INFINITY` clearance sentinel -- an absent
/// answer wearing the costume of a valid one -- except that it fails silently
/// rather than permissively.
///
/// The contract asserted here: one row per point, always; each row `None`; and
/// `point_count()` equal to the number of points the caller supplied.
#[test]
fn an_empty_segment_set_yields_one_none_per_point_not_an_empty_batch() {
    let points = points();
    let nearest = point_nearest_segment_batch(&points, &[], &[], limits(points.len(), 0))
        .expect("an empty segment set is a legal batch, not an error");

    assert_eq!(
        nearest.point_count(),
        points.len(),
        "the batch was handed {} points and must account for every one of them; \
         reporting 0 here would make a caller's zip against its own points \
         iterate zero times and report success",
        points.len()
    );
    assert_eq!(
        nearest.all().len(),
        points.len(),
        "and the row slice must be the same length, for the same reason"
    );
    for point in 0..points.len() {
        assert_eq!(
            nearest.nearest(point),
            None,
            "point {point}: there is no nearest segment in an empty set, and no \
             float stands in for one"
        );
    }
}

/// Asking with an index the caller does not own is `None`, not a panic and not
/// a neighbouring point's answer.
#[test]
fn an_out_of_range_point_index_is_none() {
    let (starts, ends) = segments();
    let points = points();
    let nearest = point_nearest_segment_batch(&points, &starts, &ends, limits(5, 5))
        .expect("reduction fits its budget");
    assert_eq!(nearest.nearest(points.len()), None);
    assert_eq!(nearest.nearest(usize::MAX), None);
}

/// The reduction is held to the SAME input contract as the dense form: same
/// mismatch error, same finiteness checks, same budget errors. A second copy of
/// those checks would be a second thing to keep in step.
#[test]
fn the_reduction_shares_the_dense_forms_input_contract() {
    let (starts, ends) = segments();
    let points = points();

    let mismatched = point_nearest_segment_batch(&points, &starts, &ends[..3], limits(5, 5));
    assert_eq!(
        mismatched,
        Err(PointSegmentBatchError::SegmentEndpointCountMismatch {
            starts: 5,
            ends: 3
        }),
        "start/end count mismatch is rejected before any work"
    );

    let mut bad_point = points.clone();
    bad_point[2][1] = f64::NAN;
    assert_eq!(
        point_nearest_segment_batch(&bad_point, &starts, &ends, limits(5, 5)),
        Err(PointSegmentBatchError::NonFinitePoint {
            collection: "points",
            item: 2,
            axis: 1
        }),
        "a non-finite query point is an error, never a distance to minimise over"
    );

    let mut bad_segment = starts.clone();
    bad_segment[1][0] = f64::INFINITY;
    assert_eq!(
        point_nearest_segment_batch(&points, &bad_segment, &ends, limits(5, 5)),
        Err(PointSegmentBatchError::NonFinitePoint {
            collection: "segment_starts",
            item: 1,
            axis: 0
        }),
        "and so is a non-finite segment endpoint"
    );

    assert_eq!(
        point_nearest_segment_batch(
            &points,
            &starts,
            &ends,
            PointSegmentBatchLimits::new(4, usize::MAX)
        ),
        Err(PointSegmentBatchError::OutputPairLimitExceeded {
            required: 25,
            maximum: 4
        }),
        "the work budget is charged the same way, in the same order"
    );
}
