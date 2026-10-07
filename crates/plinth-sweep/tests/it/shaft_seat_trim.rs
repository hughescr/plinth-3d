//! **Every span of a seated shaft that reaches the model side of its seat
//! plane carries the seat trim** — not only the record named `ContactTaper`
//! (review-fixes, 2026-09-29).
//!
//! # The defect
//!
//! The lowering trimmed the tip ball and the FIRST shaft record to the seat
//! face's plane. The drawn profile (`drawn_spans`) breaks one straight shaft
//! into several records wherever the ledger steps, and on a sloped seat face a
//! boundary one slot below the seat still leaves the next record's ring
//! reaching onto the model side of the plane: a vertical arm under a face with
//! outward normal `(0.866, 0, −0.5)`, a 0.30 mm tip and a profile that widens a
//! slot under the seat puts a `Member` record 0.05 mm below the seat whose
//! ring reaches `0.05·0.5 − r·0.866` onto the model side. That `Member` was
//! emitted whole. The production tip-trim test grades TIP and TAPER records
//! only, so it could not see this.
//!
//! # The rule
//!
//! The trim is about the shaft's SOLID, not about how it is cut into records:
//! the taper keeps its trim, and every further `Member` span whose solid
//! reaches the model side of the plane carries the same trim. A span wholly on
//! the support side carries none and emits as before.
//!
//! # How it is graded
//!
//! On the production lowering's own records. Each shaft record's two end
//! rings are SAMPLED (64 points each, from the record's own centre, radius and
//! ring normal) and every sample is measured against the plane the production
//! put on the arm's ball. A record with a sample on the model side must carry
//! that trim; a `Member` with none must carry none. Sampling is the grader's
//! own method, not the lowering's closed form. Each test fails on zero graded
//! crossing members, so it cannot pass vacuously.
//!
//! Sabotage: trim only the `ContactTaper` again (the pre-fix lowering) and
//! `a_sloped_seat_trims_every_shaft_span_it_crosses` goes red.

use plinth_core::{m0_constants, Finite, LayerPlan, PositiveFinite};
use plinth_support::{
    lower_sweep_arms, BuildFrame, FrustumRingNormal, SeatTrim, SupportPrimitive,
    SupportSegmentKind, SweepArm, SweepArmLanding, SweepLowering,
};

const PLATE_Z_MM: f64 = -5.0;
const LIFT_MM: f64 = 5.0;
const MAX_PRIMITIVES: usize = 1_000;
const MAX_PRIMITIVE_BYTES: usize = 1 << 20;
/// Samples per end ring.
const RING_SAMPLES: usize = 64;
/// A sample this far (mm) on the model side counts; float noise does not.
const SIDE_TOLERANCE_MM: f64 = 1e-9;

fn layer_height_mm() -> f64 {
    *m0_constants()
        .expect("registry")
        .evidence()
        .layer_height_default_mm()
        .value()
}

fn lower(arms: &[SweepArm]) -> SweepLowering {
    let height = layer_height_mm();
    let layers = LayerPlan::new(PLATE_Z_MM, vec![height; 400]).expect("a uniform layer plan");
    let frame = BuildFrame::new(
        Finite::new(PLATE_Z_MM).expect("finite plate"),
        PositiveFinite::new(LIFT_MM).expect("positive lift"),
    );
    lower_sweep_arms(arms, frame, &layers, MAX_PRIMITIVES, MAX_PRIMITIVE_BYTES)
        .expect("the lowering refuses only about the request")
}

/// A vertical arm landed 5 mm below its seat, tip 0.30 mm, whose ledger is the
/// tip radius in the top `tip_slots` slots under the seat and `root` in every
/// slot below them — a junction that far under the seat, which makes the
/// drawn profile break the shaft there if the break lies below the ball's
/// centre. One slot per layer.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a 5 mm rise over a layer height of hundredths of a mm is a small positive count"
)]
fn stepped_arm(seat_normal: [f64; 3], root: f64, tip_slots: usize) -> SweepArm {
    let seat = [1.0, 2.0, 5.0];
    let rise = 5.0;
    let slots = (rise / layer_height_mm()).round() as usize;
    let mut profile = vec![root; slots];
    for radius in profile.iter_mut().skip(slots - tip_slots) {
        *radius = 0.30;
    }
    SweepArm {
        seat_mm: seat,
        base_mm: [seat[0], seat[1], seat[2] - rise],
        tip_radius_mm: 0.30,
        radius_profile_mm: profile,
        seat_normal_mm: seat_normal,
        seat_face_point_mm: Some(seat),
        landing: SweepArmLanding::Landed,
        seats_into_model: true,
        head: None,
    }
}

/// The lowest signed distance (support side positive) any sample of a
/// record's two end rings reaches from `trim`'s plane.
fn lowest_sample(primitive: &SupportPrimitive, trim: SeatTrim) -> f64 {
    let start = primitive.start_mm.map(Finite::get);
    let end = primitive.end_mm.map(Finite::get);
    let normal = match primitive.ring_normal {
        FrustumRingNormal::SegmentAxis => {
            geom_kernel::vec3::normalize(geom_kernel::vec3::sub(end, start)).expect("an axis")
        }
        FrustumRingNormal::PlateZ => [0.0, 0.0, 1.0],
    };
    let reference = if normal[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let e1 = geom_kernel::vec3::normalize(geom_kernel::vec3::cross(normal, reference))
        .expect("a ring direction");
    let e2 = geom_kernel::vec3::cross(normal, e1);
    let mut lowest = f64::INFINITY;
    for (centre, radius) in [
        (start, primitive.start_radius_mm.get()),
        (end, primitive.end_radius_mm.get()),
    ] {
        for step in 0..RING_SAMPLES {
            #[expect(clippy::cast_precision_loss, reason = "a sample index below 64")]
            let angle = std::f64::consts::TAU * step as f64 / RING_SAMPLES as f64;
            let point = geom_kernel::vec3::add(
                centre,
                geom_kernel::vec3::add(
                    geom_kernel::vec3::scale(e1, radius * angle.cos()),
                    geom_kernel::vec3::scale(e2, radius * angle.sin()),
                ),
            );
            lowest = lowest.min(trim.support_side_distance_mm(point));
        }
    }
    lowest
}

/// Grade one lowered arm: returns (members graded, members crossing the plane).
fn grade(lowered: &SweepLowering) -> (usize, usize) {
    let ball = lowered
        .primitives
        .iter()
        .find(|primitive| primitive.segment_kind == SupportSegmentKind::ContactSphere)
        .expect("a seated arm has a tip ball");
    let trim = ball.seat_trim.expect("the tip ball carries its seat trim");
    let mut members = 0;
    let mut crossing = 0;
    for primitive in &lowered.primitives {
        let kind = primitive.segment_kind;
        if !matches!(
            kind,
            SupportSegmentKind::ContactTaper | SupportSegmentKind::Member
        ) {
            continue;
        }
        let lowest = lowest_sample(primitive, trim);
        let reaches = lowest < -SIDE_TOLERANCE_MM;
        if kind == SupportSegmentKind::Member {
            members += 1;
            crossing += usize::from(reaches);
        }
        if reaches || kind == SupportSegmentKind::ContactTaper {
            assert_eq!(
                primitive.seat_trim,
                Some(trim),
                "a {kind:?} record reaches {:.4} mm onto the MODEL side of its seat plane \
                 and is emitted untrimmed",
                -lowest
            );
        } else {
            assert_eq!(
                primitive.seat_trim, None,
                "a Member wholly on the support side (by {lowest:.4} mm) carries a seat trim"
            );
        }
    }
    (members, crossing)
}

/// The reviewer's case: a sloped face, the profile broken one slot under the
/// seat. The span below the break reaches the model side and must be trimmed.
#[test]
fn a_sloped_seat_trims_every_shaft_span_it_crosses() {
    let normal = [3.0_f64.sqrt() / 2.0, 0.0, -0.5];
    let lowered = lower(&[stepped_arm(normal, 0.45, 1)]);
    assert!(
        lowered.arms_drawn_in_spans > 0,
        "the fixture no longer breaks the shaft, so it grades nothing"
    );
    let (members, crossing) = grade(&lowered);
    println!(
        "sloped seat: {members} Member spans, {crossing} reaching the model side, {} trimmed",
        lowered.shaft_members_trimmed_to_seat
    );
    assert!(
        crossing > 0,
        "no Member span reaches the model side, so the rule is graded on nothing"
    );
    assert_eq!(lowered.shaft_members_trimmed_to_seat, crossing);
}

/// The control: a flat underside, a break ten slots under the seat (below
/// this ball's centre, which a flat seat drives lower than a sloped one).
/// Nothing below the taper reaches the model side, so no Member carries a
/// trim and the records are the ones the lowering always emitted.
#[test]
fn a_flat_seat_leaves_the_spans_below_the_taper_untrimmed() {
    let lowered = lower(&[stepped_arm([0.0, 0.0, -1.0], 0.45, 10)]);
    let (members, crossing) = grade(&lowered);
    assert!(members > 0, "the fixture no longer breaks the shaft");
    assert_eq!(crossing, 0);
    assert_eq!(lowered.shaft_members_trimmed_to_seat, 0);
}
