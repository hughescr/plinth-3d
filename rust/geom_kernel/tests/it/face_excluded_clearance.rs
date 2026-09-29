//! The kernel half of the honest contact exemption (#36).
//!
//! Two new entry points, and one property that is the whole reason they exist:
//! an exemption that names FACES cannot invert with radius, where the
//! radius-proportional skip-length it replaces demonstrably did
//! (`.git/mask-honesty-evidence`: the same axis passed at a radius that put it
//! 0.242 mm inside the model and refused at a thinner one with 0.569 mm of real
//! gap).

use geom_kernel::testkit::build_from_tris;
use geom_kernel::{
    NativeSurface, NearFaceBatchError, NearFaceBatchLimits, SegmentClearanceClassified,
    UnmeasuredSegment,
};

fn uncapped() -> NearFaceBatchLimits {
    NearFaceBatchLimits {
        max_rows: usize::MAX,
        max_faces_per_row: usize::MAX,
        max_total_faces: usize::MAX,
    }
}

/// Two parallel walls 3 mm apart in x, each one triangle.
///
/// Face 0 is the wall at `x = 0`; face 1 is the wall at `x = 3`. A segment run
/// between them is near BOTH, which is what makes it a usable stand-in for the
/// crevice geometry the audit measured: a seat on one wall, a foreign feature
/// on the other.
fn crevice() -> NativeSurface {
    build_from_tris(vec![
        // Face 0 — the near wall at x = 0, spanning y,z in [0,10].
        0.0, 0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 10.0, //
        // Face 1 — the far wall at x = 3.
        3.0, 0.0, 0.0, 3.0, 10.0, 0.0, 3.0, 0.0, 10.0,
    ])
}

/// The measured millimetres of one row, or a panic naming what came back
/// instead.
///
/// Deliberately NOT a fallback. A helper that quietly accepted an `UpperBound`
/// or an `Unmeasured` here would put the test back in the position the type
/// exists to prevent: grading a number nobody measured.
fn measured_mm(row: SegmentClearanceClassified) -> f64 {
    match row {
        SegmentClearanceClassified::Measured { clearance_mm, .. } => clearance_mm,
        other => panic!("expected a measured row, got {other:?}"),
    }
}

#[test]
fn an_empty_exclusion_set_is_bit_identical_to_the_unrestricted_test() {
    let surface = crevice();
    let p0 = vec![[1.0, 2.0, 2.0], [0.5, 3.0, 1.0]];
    let p1 = vec![[1.0, 2.0, 6.0], [2.5, 3.0, 5.0]];
    let r0 = vec![0.2, 0.4];
    let r1 = vec![0.2, 0.4];
    let insets = vec![0.0, 0.0];

    let plain = surface
        .segment_clearance_classified(&p0, &p1, &r0, &r1, 0.3, &insets, &insets)
        .unwrap();
    let empty = surface
        .segment_clearance_excluding_classified(&p0, &p1, &r0, &r1, 0.3, &insets, &insets, &[])
        .unwrap();
    let per_row: Vec<&[u32]> = vec![&[], &[]];
    let per_row_empty = surface
        .segment_clearance_excluding_classified(&p0, &p1, &r0, &r1, 0.3, &insets, &insets, &per_row)
        .unwrap();

    // Whole rows, not just their numbers: the classified type carries the
    // variant and the uncertainty count too, and all of it must match.
    assert_eq!(plain, empty);
    assert_eq!(plain, per_row_empty);
}

#[test]
fn excluding_the_nearest_face_hands_the_verdict_to_the_next_one() {
    let surface = crevice();
    // Axis at x = 1.0: 1.0 mm from face 0, 2.0 mm from face 1.
    let p0 = vec![[1.0, 2.0, 2.0]];
    let p1 = vec![[1.0, 2.0, 6.0]];
    let r = vec![0.25];
    let insets = vec![0.0];

    let unrestricted = surface
        .segment_clearance_classified(&p0, &p1, &r, &r, 0.0, &insets, &insets)
        .unwrap();
    assert!(
        (measured_mm(unrestricted[0]) - 0.75).abs() < 1e-12,
        "1.0 − 0.25"
    );

    let near_wall: Vec<&[u32]> = vec![&[0]];
    let excluded = surface
        .segment_clearance_excluding_classified(&p0, &p1, &r, &r, 0.0, &insets, &insets, &near_wall)
        .unwrap();
    assert!(
        (measured_mm(excluded[0]) - 1.75).abs() < 1e-12,
        "with face 0 exempt the verdict is the FAR wall's 2.0 − 0.25, not infinity: \
         exempting a face never exempts the rest of the surface"
    );
}

/// A fully exempted row is UNTESTED, and the kernel must say so IN THE TYPE.
///
/// This test used to assert `clearance[0].is_infinite()`, on the reasoning that
/// "the caller's admissibility rule is what must refuse it". That reasoning was
/// wrong in one specific way, and the wrongness is the whole defect: `INFINITY`
/// is not a refusal a caller can act on, it is the single most permissive value
/// in the ordering. Every caller that grades with `clearance >= demand` passes
/// this row, and rewire.rs:1088 is a caller that does exactly that. A row where
/// nothing was measured was scoring better than a row measured with a metre of
/// air around it.
///
/// So the assertion moved from the float to the variant. `AllFacesExcluded` is
/// not comparable to a demand, which is the point: a caller has to say what an
/// untested row means before it can grade one.
///
/// The float shim this test used to contrast against
/// (`segment_clearance_excluding`, returning `(Vec<f64>, Vec<V3>)`) is now
/// REMOVED, not merely discouraged: it could not carry the third clearance
/// state either, and leaving a lossy entry point beside a classified one is an
/// invitation to collapse the uncertainty back to a boolean at the first
/// awkward call site. There is no longer an API through which an untested row
/// can present itself as `+INFINITY`.
#[test]
fn excluding_every_face_leaves_nothing_to_grade() {
    let surface = crevice();
    let p0 = vec![[1.0, 2.0, 2.0]];
    let p1 = vec![[1.0, 2.0, 6.0]];
    let r = vec![0.25];
    let insets = vec![0.0];
    let all: Vec<&[u32]> = vec![&[0, 1]];

    let typed = surface
        .segment_clearance_excluding_classified(&p0, &p1, &r, &r, 0.0, &insets, &insets, &all)
        .unwrap();
    assert_eq!(
        typed[0],
        SegmentClearanceClassified::Unmeasured(UnmeasuredSegment::AllFacesExcluded),
        "a row whose exclusion set covers every face was never measured; the \
         kernel must not hand back a number for it"
    );
    assert_eq!(
        typed[0].clearance_mm(),
        None,
        "there is no clearance to report"
    );
    assert!(
        !typed[0].clears(false, false),
        "a stage that REFUSES must be able to refuse an untested row"
    );
    assert_eq!(
        typed[0].upper_bound_mm(),
        None,
        "and an untested row is not a one-sided row either -- there is no \
         number at all, not even a bound"
    );
    assert_eq!(
        typed[0].near_parallel_faces(),
        0,
        "no face was tested, so no face can have been undecidable"
    );
}

/// THE THEOREM, at the kernel boundary.
///
/// Same axis, same FROZEN exclusion set, two radii: the thinner arm may never
/// grade worse than the fatter one. This holds because the tested face set does
/// not move with the radius, so the measured distance is identical and only the
/// `r_max` subtraction differs. The retired skip-length had no such guarantee —
/// its tested REGION shrank as the radius grew, so a fatter arm could be graded
/// somewhere else entirely, and was.
#[test]
fn a_thinner_arm_never_grades_worse_than_a_fatter_one_on_a_frozen_face_set() {
    let surface = crevice();
    let p0 = [1.0, 2.0, 2.0];
    let p1 = [1.0, 2.0, 6.0];
    let insets = vec![0.0];
    // The set a 0.9 mm-radius arm would have frozen at creation. It stays this
    // set for BOTH radii below: that is what "frozen" means.
    let frozen: Vec<&[u32]> = vec![&[0]];

    let mut previous = f64::NEG_INFINITY;
    for radius in [0.9_f64, 0.6, 0.45, 0.3, 0.1] {
        let rows = surface
            .segment_clearance_excluding_classified(
                &[p0],
                &[p1],
                &[radius],
                &[radius],
                0.3,
                &insets,
                &insets,
                &frozen,
            )
            .unwrap();
        let clearance = measured_mm(rows[0]);
        assert!(
            clearance >= previous,
            "clearance must not fall as the arm thins: {radius} mm radius graded \
             {clearance} against the previous {previous}"
        );
        previous = clearance;
    }
}

#[test]
fn the_band_collects_exactly_the_faces_within_reach_sorted_and_deduplicated() {
    let surface = crevice();
    let p0 = vec![[1.0, 2.0, 2.0]];
    let p1 = vec![[1.0, 2.0, 6.0]];

    // 0.5 mm reaches neither wall (1.0 and 2.0 away).
    let none = surface
        .faces_near_segments_batch_classified(&p0, &p1, &[0.5], uncapped())
        .unwrap();
    assert_eq!(none.rows, vec![Vec::<u32>::new()]);
    assert_eq!(
        none.near_parallel,
        vec![0],
        "nothing undecidable on axis-aligned walls"
    );

    // 1.5 mm reaches the near wall only.
    let near = surface
        .faces_near_segments_batch_classified(&p0, &p1, &[1.5], uncapped())
        .unwrap();
    assert_eq!(near.rows, vec![vec![0]]);
    assert_eq!(near.near_parallel, vec![0]);

    // 2.5 mm reaches both, ascending.
    let both = surface
        .faces_near_segments_batch_classified(&p0, &p1, &[2.5], uncapped())
        .unwrap();
    assert_eq!(both.rows, vec![vec![0, 1]]);
    assert_eq!(both.near_parallel, vec![0]);
}

#[test]
fn a_face_exactly_at_the_band_edge_is_inside_the_band() {
    let surface = crevice();
    let exact = surface
        .faces_near_segments_batch_classified(
            &[[1.0, 2.0, 2.0]],
            &[[1.0, 2.0, 6.0]],
            &[1.0],
            uncapped(),
        )
        .unwrap();
    assert_eq!(
        exact.rows,
        vec![vec![0]],
        "the seat exemption must not turn on a knife edge; admitting the \
         boundary face can only widen the seat's own zone by one face"
    );
}

#[test]
fn the_batch_refuses_rather_than_returning_a_truncated_band() {
    let surface = crevice();
    let p0 = vec![[1.0, 2.0, 2.0]];
    let p1 = vec![[1.0, 2.0, 6.0]];

    let per_row = NearFaceBatchLimits {
        max_faces_per_row: 1,
        ..uncapped()
    };
    assert!(matches!(
        surface.faces_near_segments_batch_classified(&p0, &p1, &[2.5], per_row),
        Err(NearFaceBatchError::RowFaceLimitExceeded { row: 0, limit: 1 })
    ));

    let total = NearFaceBatchLimits {
        max_total_faces: 1,
        ..uncapped()
    };
    assert!(matches!(
        surface.faces_near_segments_batch_classified(&p0, &p1, &[2.5], total),
        Err(NearFaceBatchError::TotalFaceLimitExceeded {
            required: 2,
            limit: 1
        })
    ));

    let rows = NearFaceBatchLimits {
        max_rows: 0,
        ..uncapped()
    };
    assert!(matches!(
        surface.faces_near_segments_batch_classified(&p0, &p1, &[2.5], rows),
        Err(NearFaceBatchError::RowLimitExceeded {
            required: 1,
            limit: 0
        })
    ));
}

#[test]
fn the_batch_refuses_a_non_finite_radius_or_endpoint() {
    let surface = crevice();
    assert!(matches!(
        surface.faces_near_segments_batch_classified(
            &[[1.0, 2.0, 2.0]],
            &[[1.0, 2.0, 6.0]],
            &[f64::NAN],
            uncapped()
        ),
        Err(NearFaceBatchError::InvalidRadius { row: 0, .. })
    ));
    assert!(matches!(
        surface.faces_near_segments_batch_classified(
            &[[1.0, 2.0, 2.0]],
            &[[1.0, 2.0, 6.0]],
            &[-1.0],
            uncapped()
        ),
        Err(NearFaceBatchError::InvalidRadius { row: 0, .. })
    ));
    assert!(matches!(
        surface.faces_near_segments_batch_classified(
            &[[f64::INFINITY, 2.0, 2.0]],
            &[[1.0, 2.0, 6.0]],
            &[1.0],
            uncapped()
        ),
        Err(NearFaceBatchError::NonFinitePoint { row: 0, axis: 0 })
    ));
}

#[test]
fn companion_slices_must_match_the_batch_length() {
    let surface = crevice();
    assert!(matches!(
        surface.faces_near_segments_batch_classified(&[[0.0; 3]], &[], &[1.0], uncapped()),
        Err(NearFaceBatchError::BatchInput(_))
    ));
    let one = vec![0.0];
    let two: Vec<&[u32]> = vec![&[], &[]];
    assert!(surface
        .segment_clearance_excluding_classified(
            &[[0.0; 3]],
            &[[1.0; 3]],
            &one,
            &one,
            0.3,
            &one,
            &one,
            &two
        )
        .is_err());
}
