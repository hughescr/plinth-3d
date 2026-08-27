//! Regression guard: the clearance predicate must never report a measurement
//! it did not take.
//!
//! THE DEFECT THESE TESTS WOULD HAVE CAUGHT. `segment_clearance` returned a
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
//! `segment_clearance_excluding_typed`, which reports
//! `SegmentClearance::Unmeasured` and cannot be compared with a threshold until
//! the caller says, in code, what an untested row means to it.
//!
//! These tests do not re-implement the predicate. They assert which VARIANT
//! comes back for geometry whose classification is decided by inspection, and
//! they read the one genuine measurement's value from the fixture's own
//! arithmetic (stated in the comment beside it), never by recomputing it with a
//! copy of the kernel's formula.

use geom_kernel::testkit::build_from_tris;
use geom_kernel::{NativeSurface, SegmentClearance, UnmeasuredSegment};

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
fn four_rows(surface: &NativeSurface) -> Vec<SegmentClearance> {
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
        .segment_clearance_excluding_typed(&p0, &p1, &r, &r, 0.0, &inset0, &inset1, &excluded)
        .expect("well-formed batch")
}

#[test]
fn every_unmeasured_reason_is_its_own_variant_and_none_is_a_number() {
    let rows = four_rows(&crevice());

    assert_eq!(
        rows[0],
        SegmentClearance::Unmeasured(UnmeasuredSegment::DegenerateAxis),
        "coincident endpoints leave no axis to sweep"
    );
    assert_eq!(
        rows[1],
        SegmentClearance::Unmeasured(UnmeasuredSegment::InsetsConsumeAxis),
        "masks that swallow the member leave no length to test"
    );
    assert_eq!(
        rows[2],
        SegmentClearance::Unmeasured(UnmeasuredSegment::AllFacesExcluded),
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
    let SegmentClearance::Measured {
        clearance_mm,
        witness,
    } = rows[3]
    else {
        panic!("row 3 tests the whole surface and must be a measurement, got {:?}", rows[3]);
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
/// `clears_or` has no default: a caller must state which way an untested row
/// falls. A refusing stage passes `false` and refuses; a pruning stage passes
/// `true` and keeps the candidate for a later test. Under the float form
/// neither stage could express the first choice, because the value it received
/// was already larger than any demand it could name.
#[test]
fn an_untested_row_can_be_refused_and_a_measured_one_is_graded_on_its_number() {
    let rows = four_rows(&crevice());

    for (row, value) in rows.iter().take(3).enumerate() {
        assert!(
            !value.clears_or(false),
            "row {row} was never tested, so a REFUSING stage must be able to refuse it"
        );
        assert!(
            value.clears_or(true),
            "row {row} must also be admissible to a PRUNING stage that says so explicitly"
        );
    }

    assert!(
        rows[3].clears_or(false),
        "the measured row has 0.75 mm of real gap and passes on its own merits, \
         not on the caller's policy for untested rows"
    );
    assert!(
        rows[3].clears_or(true),
        "and its verdict does not depend on that policy at all"
    );
}

/// A row that VIOLATES its demanded gap must stay refused. Without this the
/// three tests above would pass against a `clears_or` that returned `true` for
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
        .segment_clearance_typed(&p0, &p1, &r, &r, 0.8, &zero, &zero)
        .expect("well-formed batch");

    let Some(clearance_mm) = rows[0].clearance_mm() else {
        panic!("this row is fully testable and must be measured, got {:?}", rows[0]);
    };
    assert!(
        (clearance_mm + 0.2).abs() < 1e-12,
        "1.0 mm of surface distance against a 0.4 mm arm and a 0.8 mm demand, got {clearance_mm}"
    );
    assert!(
        !rows[0].clears_or(true),
        "a MEASURED violation is refused whatever the caller's policy for \
         untested rows is -- the policy must not be able to rescue it"
    );
}

/// The lossiness of the retained float shim, pinned as a fact rather than left
/// as a claim in a doc comment.
///
/// Note what can and cannot be shown here. No finite mesh can produce a
/// measured clearance of exactly `+INFINITY`, so the collapse is not visible as
/// two equal bit patterns; it is visible as two things that matter more:
///   * all THREE unmeasured reasons become one indistinguishable float, so a
///     consumer of the shim cannot even ask why a row was not tested; and
///   * the untested row and a genuinely well-cleared row return the SAME
///     VERDICT under every `>= demand` test a caller can write, which is the
///     comparison callers actually make.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn the_float_shim_collapses_all_three_unmeasured_reasons_onto_one_pass() {
    let surface = crevice();
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

    let (floats, witnesses) = surface
        .segment_clearance_excluding(&p0, &p1, &r, &r, 0.0, &inset0, &inset1, &excluded)
        .expect("well-formed batch");

    assert_eq!(
        floats[0], floats[1],
        "a degenerate axis and a fully masked axis are different facts and the \
         shim gives them the same number"
    );
    assert_eq!(
        floats[1], floats[2],
        "and a fully exempted row is a third different fact with that same number"
    );
    for row in 0..3 {
        assert!(
            floats[row].is_infinite() && floats[row] > 0.0,
            "row {row} comes back as +INFINITY -- the most permissive value in \
             the ordering, chosen to mean the least permissive thing"
        );
        assert!(
            witnesses[row].iter().all(|value| value.is_nan()),
            "the NaN witness is the only trace the shim leaves, and no production \
             caller reads it"
        );
    }

    // THE BUG, in the shape callers actually write it. Every untested row
    // passes the same test the well-cleared row passes, at every demand.
    for demand in [0.0_f64, 0.5, 5.0, 1.0e9] {
        for (row, &value) in floats.iter().enumerate().take(3) {
            assert!(
                value >= demand,
                "untested row {row} passes a demand of {demand} mm on the shim"
            );
        }
    }
    assert!(
        floats[3].is_finite(),
        "while the row that WAS measured is a finite 0.75 mm and can be failed \
         by a demand above it"
    );
    assert!(
        floats[3] < 1.0e9,
        "which is the asymmetry: no real measurement can beat the sentinel"
    );
}

/// The shim must remain a faithful re-encoding of the typed form, or migrating
/// a call site would change numbers as well as types and no reviewer could tell
/// the two effects apart.
#[test]
fn the_shim_is_exactly_the_typed_form_re_encoded() {
    let surface = crevice();
    let p0 = vec![[1.0, 2.0, 2.0], [0.5, 3.0, 1.0], [1.0, 2.0, 2.0]];
    let p1 = vec![[1.0, 2.0, 6.0], [2.5, 3.0, 5.0], [1.0, 2.0, 2.0]];
    let r0 = vec![0.2, 0.4, 0.1];
    let r1 = vec![0.2, 0.4, 0.1];
    let zero = vec![0.0; 3];

    let typed = surface
        .segment_clearance_typed(&p0, &p1, &r0, &r1, 0.3, &zero, &zero)
        .expect("well-formed batch");
    let (floats, witnesses) = surface
        .segment_clearance(&p0, &p1, &r0, &r1, 0.3, &zero, &zero)
        .expect("well-formed batch");

    assert_eq!(typed.len(), floats.len());
    for (row, value) in typed.iter().enumerate() {
        let (clearance, witness) = value.to_lossy_pair();
        assert_eq!(
            clearance.to_bits(),
            floats[row].to_bits(),
            "row {row}: the shim must return the typed value's own re-encoding, \
             bit for bit"
        );
        for axis in 0..3 {
            assert_eq!(
                witness[axis].is_nan(),
                witnesses[row][axis].is_nan(),
                "row {row} axis {axis}: witness NaN-ness must agree"
            );
            if !witness[axis].is_nan() {
                assert_eq!(witness[axis].to_bits(), witnesses[row][axis].to_bits());
            }
        }
    }
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
        .segment_clearance_typed(&p0, &p1, &r, &r, 0.0, &zero, &zero)
        .expect("well-formed batch");

    let Some(clearance_mm) = rows[0].clearance_mm() else {
        panic!("a row 1000 mm from the surface was measured, not skipped: {:?}", rows[0]);
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
