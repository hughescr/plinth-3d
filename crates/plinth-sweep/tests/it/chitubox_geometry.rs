//! **The sweep's tip balls are the owner's own ChituBox contact spheres**
//! (ruling of 2026-09-30: support geometry comes from his ChituBox practice;
//! import geometry RATIOS and fixed SHAPES only, while every arm and tip radius
//! stays sized by the area law).
//!
//! His exported profiles (`docs/evidence/chitubox-support-profiles-2026-09-30.cfgx`)
//! draw a contact sphere of `supporttouchtipdiameter` on a cone whose top is
//! `supporttipupdiameter`, in DIAMETER:
//!
//! | profile | tip-up | touch | ratio |
//! |---|---|---|---|
//! | Ultralight | 0.15 | 0.20 | 4/3 |
//! | Light | 0.20 | 0.30 | 1.5 |
//! | Middle | 0.40 | 0.50 | 1.25 |
//! | Heavy | 0.60 | 0.80 | 4/3 |
//!
//! The rule: a sweep tip of load-sized RADIUS `r` gets a ball of radius
//! `f(2r) · r`, with `f` linear in tip diameter between the two bracketing
//! profiles' ratios and held at the end profile's ratio outside 0.15-0.60 mm.
//! Until 2026-09-30 every sweep ball was the Placeholder `BALL_FACTOR_STUB`
//! = 1.5 × r, whatever the tip.
//!
//! **Nothing here reads the rule it grades.** The expected ball radii are
//! worked by hand from the table above and typed. The Heavy row is also the
//! one native ChituBox file we hold with this profile
//! (`Chitubox Projects/cylinder-cube-sphere.chitubox`): a type-7 sphere of
//! radius 0.40 on a type-1 taper that starts at radius 0.30, so the 0.30 mm
//! tip below must come out at exactly the native 0.40 mm ball.
//!
//! Sabotage: put `BALL_FACTOR_STUB` back as the sweep's ball factor and every
//! row but the Light one (whose ratio IS 1.5) goes red.

use plinth_core::{m0_constants, Finite, LayerPlan, PositiveFinite};
use plinth_support::{
    lower_sweep_arms, BuildFrame, SupportSegmentKind, SweepArm, SweepArmLanding, SweepLowering,
};

const PLATE_Z_MM: f64 = -5.0;
const LIFT_MM: f64 = 5.0;

/// `(tip RADIUS, expected ball RADIUS)`, each worked by hand from the owner's
/// table (DIAMETERS there, radii here).
const ROWS: [(f64, f64); 7] = [
    // d 0.10: below Ultralight's 0.15, held at its 0.20/0.15 = 4/3.
    (0.05, 0.05 * 4.0 / 3.0),
    // d 0.15: Ultralight itself, 4/3.
    (0.075, 0.1),
    // d 0.20: Light itself, 0.30/0.20 = 1.5.
    (0.1, 0.15),
    // d 0.30: halfway from Light (1.5) to Middle (1.25), 1.375.
    (0.15, 0.206_25),
    // d 0.40: Middle itself, 0.50/0.40 = 1.25.
    (0.2, 0.25),
    // d 0.60: Heavy itself, 0.80/0.60 = 4/3 -- the native file's 0.40 ball.
    (0.3, 0.4),
    // d 1.00: above Heavy's 0.60, held at its 4/3; never extrapolated.
    (0.5, 0.5 * 4.0 / 3.0),
];

fn lower_all(arms: &[SweepArm]) -> SweepLowering {
    let height = *m0_constants()
        .expect("registry")
        .evidence()
        .layer_height_default_mm()
        .value();
    let layers = LayerPlan::new(PLATE_Z_MM, vec![height; 400]).expect("a uniform layer plan");
    let frame = BuildFrame::new(
        Finite::new(PLATE_Z_MM).expect("finite plate"),
        PositiveFinite::new(LIFT_MM).expect("positive lift"),
    );
    lower_sweep_arms(arms, frame, &layers, 10_000, 1 << 22)
        .expect("the lowering refuses only about the request")
}

/// A vertical plate arm under a flat, down-facing face at `x`, its tip at the
/// load-sized `tip_radius_mm`, widening to a root a little wider.
#[expect(
    clippy::cast_precision_loss,
    reason = "a row index of a seven-row table"
)]
fn arm(index: usize, tip_radius_mm: f64) -> SweepArm {
    let x = 5.0 * index as f64;
    SweepArm {
        seat_mm: [x, 0.0, PLATE_Z_MM + 8.0],
        base_mm: [x, 0.0, PLATE_Z_MM],
        tip_radius_mm,
        radius_profile_mm: vec![tip_radius_mm * 1.2],
        seat_normal_mm: [0.0, 0.0, -1.0],
        seat_face_point_mm: Some([x, 0.0, PLATE_Z_MM + 8.0]),
        landing: SweepArmLanding::Plate,
        seats_into_model: true,
    }
}

#[test]
fn every_sweep_tip_ball_is_the_owners_profile_ratio_at_its_own_tip_size() {
    let arms: Vec<SweepArm> = ROWS
        .iter()
        .enumerate()
        .map(|(index, (tip, _))| arm(index, *tip))
        .collect();
    let lowered = lower_all(&arms);

    // PATH HIT: every tip read its factor off the profile table.
    assert_eq!(
        lowered.tip_ball_factor.len(),
        ROWS.len(),
        "one profile-table factor per seated tip"
    );
    let balls: Vec<f64> = lowered
        .primitives
        .iter()
        .filter(|primitive| primitive.segment_kind == SupportSegmentKind::ContactSphere)
        .map(|ball| ball.start_radius_mm.get())
        .collect();
    assert_eq!(balls.len(), ROWS.len(), "one contact sphere per arm");
    for ((tip, expected), (ball, factor)) in
        ROWS.iter().zip(balls.iter().zip(&lowered.tip_ball_factor))
    {
        assert!(
            (ball - expected).abs() <= 1e-12,
            "a {tip} mm tip got a {ball} mm ball; the owner's profiles draw {expected} mm"
        );
        assert!(
            (factor * tip - expected).abs() <= 1e-12,
            "the published factor {factor} is not the one the ball was drawn with"
        );
    }
    // The seat law is solved for THAT ball: the delivered cut disc still meets
    // `a >= r_tip` on every tip, at every ratio the table gives.
    assert_eq!(
        lowered.tips_below_area_guarantee(),
        0,
        "cut radii {:?} against tips {:?}",
        lowered.tip_cut_radius_mm,
        lowered.tip_radius_mm
    );
}
