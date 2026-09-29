//! Regression guard: the clearance predicate must never report a measurement
//! it did not take.
//!
//! THE DEFECT THESE TESTS WOULD HAVE CAUGHT. the retired float `segment_clearance` returned a
//! bare `f64` per row and encoded "this row was never tested" as
//! `f64::INFINITY`. That sentinel is in band, orderable, and larger than every
//! demanded gap a caller could compare it against, so a caller that graded with
//! `clearances.iter().all(|c| *c >= 0.0)` read "we did not look" as "enormous
//! clearance, definitely fine". `crates/plinth-support/src/rewire.rs:1088` is
//! such a caller. The untested row scored better than a row measured with a
//! metre of air around it.
//!
//! There are THREE ways to reach an untested row, and the float form collapsed
//! all three onto the same number as each other AND onto the same verdict as a
//! genuine pass. Each is exercised below against
//! `segment_clearance_excluding_classified`, which reports
//! `SegmentClearanceClassified::Unmeasured` and cannot be compared with a
//! threshold until the caller says, in code, what an untested row means to it.
//!
//! These tests do not re-implement the predicate. They assert which VARIANT
//! comes back for geometry whose classification is decided by inspection, and
//! they read the one genuine measurement's value from the fixture's own
//! arithmetic (stated in the comment beside it), never by recomputing it with a
//! copy of the kernel's formula.

use geom_kernel::testkit::build_from_tris;
use geom_kernel::{NativeSurface, SegmentClearanceClassified, UnmeasuredSegment};

/// Two parallel walls 3 mm apart in x, one triangle each, spanning y,z in
/// [0, 10]. Face 0 is the wall at x = 0; face 1 is the wall at x = 3.
fn crevice() -> NativeSurface {
    build_from_tris(vec![
        0.0, 0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 10.0, //
        3.0, 0.0, 0.0, 3.0, 10.0, 0.0, 3.0, 0.0, 10.0,
    ])
}

/// A single small triangle in the z = 0 plane, so a segment can be placed a
/// long, KNOWN distance from the whole surface.
fn lone_triangle() -> NativeSurface {
    build_from_tris(vec![
        0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, //
    ])
}

/// The four rows, side by side, in one batch: three ways to be unmeasured and
/// one genuine measurement. A batch is the right shape for this because the
/// defect is precisely that all four looked alike to the consumer.
fn four_rows(surface: &NativeSurface) -> Vec<SegmentClearanceClassified> {
    // Row 0: DegenerateAxis -- both endpoints at the same point.
    // Row 1: InsetsConsumeAxis -- a 4 mm axis with 2.5 mm masked off each end.
    // Row 2: AllFacesExcluded -- a normal axis, every face exempted.
    // Row 3: Measured -- the same axis as row 2 with nothing exempted.
    let p0 = vec![
        [1.0, 2.0, 2.0],
        [1.0, 2.0, 2.0],
        [1.0, 2.0, 2.0],
        [1.0, 2.0, 2.0],
    ];
    let p1 = vec![
        [1.0, 2.0, 2.0],
        [1.0, 2.0, 6.0],
        [1.0, 2.0, 6.0],
        [1.0, 2.0, 6.0],
    ];
    let r = vec![0.25; 4];
    let inset0 = vec![0.0, 2.5, 0.0, 0.0];
    let inset1 = vec![0.0, 2.5, 0.0, 0.0];
    let excluded: Vec<&[u32]> = vec![&[], &[], &[0, 1], &[]];
    surface
        .segment_clearance_excluding_classified(&p0, &p1, &r, &r, 0.0, &inset0, &inset1, &excluded)
        .expect("well-formed batch")
}

#[test]
fn every_unmeasured_reason_is_its_own_variant_and_none_is_a_number() {
    let rows = four_rows(&crevice());

    assert_eq!(
        rows[0],
        SegmentClearanceClassified::Unmeasured(UnmeasuredSegment::DegenerateAxis),
        "coincident endpoints leave no axis to sweep"
    );
    assert_eq!(
        rows[1],
        SegmentClearanceClassified::Unmeasured(UnmeasuredSegment::InsetsConsumeAxis),
        "masks that swallow the member leave no length to test"
    );
    assert_eq!(
        rows[2],
        SegmentClearanceClassified::Unmeasured(UnmeasuredSegment::AllFacesExcluded),
        "an exclusion set covering every face leaves nothing to measure AGAINST. \
         This is the variant the first typed implementation was missing: without \
         it the row comes back Measured with a clearance_mm of INFINITY, and a type \
         that asserts a measurement which never happened is worse than the float \
         sentinel it replaced"
    );

    for (row, value) in rows.iter().take(3).enumerate() {
        assert_eq!(
            value.clearance_mm(),
            None,
            "row {row} was never measured; there is no millimetre value to hand out"
        );
        assert_eq!(
            value.witness(),
            None,
            "row {row} has no nearest surface point either"
        );
        assert!(
            value.unmeasured().is_some(),
            "row {row} must be able to say WHY it was not measured"
        );
    }

    // Row 3 is the control: the same axis as row 2, exempting nothing. The
    // fixture's near wall is at x = 0 and the axis sits at x = 1.0, so the
    // surface distance is 1.0 mm; with r = 0.25 and standoff 0.0 the demanded
    // gap subtraction leaves 0.75 mm.
    let SegmentClearanceClassified::Measured {
        clearance_mm,
        witness,
    } = rows[3]
    else {
        panic!(
            "row 3 tests the whole surface and must be a measurement, got {:?}",
            rows[3]
        );
    };
    assert!(
        (clearance_mm - 0.75).abs() < 1e-12,
        "1.0 mm to the near wall minus the 0.25 mm arm radius, got {clearance_mm}"
    );
    assert!(
        witness.iter().all(|value| value.is_finite()),
        "a measured row names a real surface point, not NaN"
    );
}

/// The verdict half, which is where the defect actually bit.
///
/// `clears` has NO defaults: a caller must state which way an untested row
/// falls AND which way a one-sided (`UpperBound`) row falls. A refusing stage
/// passes `false` and refuses; a pruning stage passes `true` and keeps the
/// candidate for a later test. Under the float form neither stage could express
/// the first choice, because the value it received was already larger than any
/// demand it could name.
///
/// The two knobs are exercised independently below — `clears(true, false)`
/// admits an untested row while still refusing a one-sided one — so a
/// predicate that collapsed them back into one fallback fails here.
#[test]
fn an_untested_row_can_be_refused_and_a_measured_one_is_graded_on_its_number() {
    let rows = four_rows(&crevice());

    for (row, value) in rows.iter().take(3).enumerate() {
        assert!(
            !value.clears(false, false),
            "row {row} was never tested, so a REFUSING stage must be able to refuse it"
        );
        assert!(
            value.clears(true, false),
            "row {row} must also be admissible to a PRUNING stage that says so explicitly"
        );
    }

    assert!(
        rows[3].clears(false, false),
        "the measured row has 0.75 mm of real gap and passes on its own merits, \
         not on the caller's policy for untested rows"
    );
    assert!(
        rows[3].clears(true, true),
        "and its verdict does not depend on that policy at all"
    );
}

/// A row that VIOLATES its demanded gap must stay refused. Without this the
/// three tests above would pass against a `clears` that returned `true` for
/// every measurement.
#[test]
fn a_measured_violation_is_still_a_violation() {
    let surface = crevice();
    let p0 = vec![[1.0, 2.0, 2.0]];
    let p1 = vec![[1.0, 2.0, 6.0]];
    // 1.0 mm to the near wall, but the arm is 0.4 mm and the stage demands a
    // further 0.8 mm of standoff: 1.0 - (0.4 + 0.8) = -0.2 mm.
    let r = vec![0.4];
    let zero = vec![0.0];
    let rows = surface
        .segment_clearance_classified(&p0, &p1, &r, &r, 0.8, &zero, &zero)
        .expect("well-formed batch");

    let Some(clearance_mm) = rows[0].clearance_mm() else {
        panic!(
            "this row is fully testable and must be measured, got {:?}",
            rows[0]
        );
    };
    assert!(
        (clearance_mm + 0.2).abs() < 1e-12,
        "1.0 mm of surface distance against a 0.4 mm arm and a 0.8 mm demand, got {clearance_mm}"
    );
    assert!(
        !rows[0].clears(true, true),
        "a MEASURED violation is refused whatever the caller's policy for \
         untested rows is -- the policy must not be able to rescue it"
    );
}

/// THE THIRD STATE. A row whose walk skipped an undecidable pierce candidate is
/// neither `Measured` nor `Unmeasured`, and the type says so.
///
/// The fixture is the §O4 counterexample face itself, hit by a SEGMENT chain
/// rather than by a bare ray: triangle `(0,0,0),(2,0,0),(0,2,0)` with the axis
/// running from `(0.4, 0.5, -1e-16)` along `(1, 0, 2e-15)`. That gives
/// `det = -8e-15` — nonzero, inside `DET_EPS = 1e-14` — so whether the axis
/// pierces the face's interior is undecidable, while the endpoint candidates
/// still return a real, tiny distance.
///
/// What this pins is the DISTINCTION, not the number: the walk DID produce a
/// value and that value is one-sided. Reporting it as `Measured` would be the
/// `INFINITY` defect in a new costume — a number that passes every
/// `>= demand` test standing in for an answer nobody has. The true clearance
/// here may be zero (the axis may really pass through the face), and no
/// arithmetic at this scale can say.
#[test]
fn an_undecidable_pierce_makes_the_row_an_upper_bound_not_a_measurement() {
    // The §O4 triangle, as a one-face surface.
    let surface = build_from_tris(vec![
        0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0, 0.0, //
    ]);
    let p0 = vec![[0.4, 0.5, -1.0e-16]];
    let p1 = vec![[1.4, 0.5, 1.9e-15]];
    let r = vec![0.0];
    let zero = vec![0.0];

    let rows = surface
        .segment_clearance_classified(&p0, &p1, &r, &r, 0.0, &zero, &zero)
        .expect("well-formed batch");

    let SegmentClearanceClassified::UpperBound {
        clearance_mm,
        witness,
        near_parallel_faces,
    } = rows[0]
    else {
        panic!(
            "a skipped pierce candidate must surface as UpperBound, got {:?}",
            rows[0]
        );
    };
    assert_eq!(
        near_parallel_faces, 1,
        "the one face of this surface is the undecidable one"
    );
    assert!(
        clearance_mm.is_finite(),
        "an UpperBound row carries a real number; what it cannot do is affirm a gap"
    );
    assert!(
        witness.iter().all(|value| value.is_finite()),
        "and it names the point that produced the bound"
    );

    // It is neither of the other two states, asserted rather than assumed --
    // this is the whole point of adding a third variant.
    assert_eq!(
        rows[0].clearance_mm(),
        None,
        "an UpperBound is NOT a measured clearance"
    );
    assert_eq!(
        rows[0].unmeasured(),
        None,
        "and it is NOT an unmeasured row either -- a number does exist"
    );
    assert_eq!(rows[0].upper_bound_mm(), Some(clearance_mm));

    // The verdict half: a refusing stage refuses it whatever its policy for
    // untested rows is, and a pruning stage must SAY so to keep it.
    assert!(
        !rows[0].clears(true, false),
        "a REFUSING stage must be able to refuse a one-sided row even while \
         admitting untested ones"
    );
    assert!(
        rows[0].clears(false, true),
        "and a PRUNING stage keeps it only by stating that policy explicitly"
    );
}

/// The counter half of the same event: the process-global site counter for the
/// `seg_tri_dist` pierce candidate must actually move.
///
/// A counter that cannot fire is the failure mode this project keeps
/// reproducing, so it is asserted from a DELTA around a call known to fire.
/// The counters are process-global and other tests in this binary can only ever
/// add, so the assertion is `>=`.
#[test]
fn the_undecidable_pierce_charges_its_global_counter() {
    let surface = build_from_tris(vec![
        0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0, 0.0, //
    ]);
    let p0 = vec![[0.4, 0.5, -1.0e-16]];
    let p1 = vec![[1.4, 0.5, 1.9e-15]];
    let r = vec![0.0];
    let zero = vec![0.0];

    let before = geom_kernel::telemetry::near_parallel_counts();
    let rows = surface
        .segment_clearance_classified(&p0, &p1, &r, &r, 0.0, &zero, &zero)
        .expect("well-formed batch");
    let after = geom_kernel::telemetry::near_parallel_counts();

    assert!(
        matches!(rows[0], SegmentClearanceClassified::UpperBound { .. }),
        "the fixture must fire, or the counter assertion below proves nothing"
    );
    assert!(
        after.seg_tri_pierce - before.seg_tri_pierce >= 1,
        "the seg_tri pierce counter did not move: {before:?} -> {after:?}"
    );
}

/// A long-range control, so "unmeasured" cannot be confused with "far away".
///
/// The axis runs 1000 mm above a single 1 mm triangle. That is a real, large,
/// FINITE measurement. The typed form says `Measured`; the point of the whole
/// exercise is that this row and an untested one are not the same thing.
#[test]
fn a_genuinely_enormous_clearance_is_still_a_measurement() {
    let surface = lone_triangle();
    let p0 = vec![[0.25, 0.25, 1000.0]];
    let p1 = vec![[0.25, 0.25, 1004.0]];
    let r = vec![0.5];
    let zero = vec![0.0];

    let rows = surface
        .segment_clearance_classified(&p0, &p1, &r, &r, 0.0, &zero, &zero)
        .expect("well-formed batch");

    let Some(clearance_mm) = rows[0].clearance_mm() else {
        panic!(
            "a row 1000 mm from the surface was measured, not skipped: {:?}",
            rows[0]
        );
    };
    // The nearest point of the axis to the triangle is its lower endpoint,
    // directly above the triangle's interior: 1000 mm, less the 0.5 mm radius.
    assert!(
        (clearance_mm - 999.5).abs() < 1e-9,
        "1000 mm of air minus the 0.5 mm arm radius, got {clearance_mm}"
    );
    assert!(
        clearance_mm.is_finite(),
        "however large a real clearance is, it is finite -- INFINITY was never a \
         measurement and must not be reachable as one"
    );
    assert_eq!(
        rows[0].unmeasured(),
        None,
        "and nothing about being far away makes a row untested"
    );
}
