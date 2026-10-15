//! **The member profile, head to foot** (round 6, 2026-10-14;
//! `plinth-evidence/.../round6-2026-10-14/PROFILE-DESIGN.md`): the owner's
//! pictures 30 and 31, read on the records the real lowering emits. A ball is
//! wider than the neck it stands on, and a shaft leaves its ball at the neck's
//! radius, never at the ledger's value one layer down. Each test names the
//! sabotage that must make it fail.

use plinth_core::{Finite, LayerPlan, PositiveFinite};
use plinth_support::{
    lower_sweep_arms, BuildFrame, SupportPrimitive, SupportSegmentKind, SweepArm, SweepArmLanding,
};

const H: f64 = 0.05;
const PLATE: f64 = -5.0;

fn lowered(arm: &SweepArm) -> Vec<SupportPrimitive> {
    let layers = LayerPlan::new(PLATE, vec![H; 400]).expect("a uniform layer plan");
    let frame = BuildFrame::new(
        Finite::new(PLATE).expect("finite plate"),
        PositiveFinite::new(5.0).expect("positive lift"),
    );
    lower_sweep_arms(std::slice::from_ref(arm), frame, &layers, 100_000, 1 << 30)
        .expect("the lowering refuses only about the request")
        .primitives
}

/// A straight-down member seated at z = 3 on a flat face, standing on the plate.
fn straight(tip: f64, profile: Vec<f64>, grown: Option<f64>) -> SweepArm {
    SweepArm {
        seat_mm: [0.0, 0.0, 3.0],
        base_mm: [0.0, 0.0, 0.0],
        tip_radius_mm: tip,
        radius_profile_mm: profile,
        seat_normal_mm: [0.0, 0.0, -1.0],
        seat_face_point_mm: Some([0.0, 0.0, 3.0]),
        landing: SweepArmLanding::Plate,
        seats_into_model: true,
        head: None,
        contact_ball_radius_mm: grown,
    }
}

/// The ball and the frustum that leaves its centre: `(ball radius, neck radius)`.
fn ball_and_neck(primitives: &[SupportPrimitive]) -> (f64, f64) {
    let ball = primitives
        .iter()
        .find(|p| p.segment_kind == SupportSegmentKind::ContactSphere)
        .expect("a contact ball");
    let radius = ball.start_radius_mm.get();
    let centre: [f64; 3] = std::array::from_fn(|axis| {
        0.5 * (ball.start_mm[axis].get() + ball.end_mm[axis].get())
    });
    let shaft = primitives
        .iter()
        .find(|p| {
            matches!(p.segment_kind, SupportSegmentKind::ContactTaper | SupportSegmentKind::Member)
                && geom_kernel::vec3::dist(p.start_mm.map(Finite::get), centre) < 1e-6
        })
        .expect("a frustum that leaves the ball's centre");
    (radius, shaft.start_radius_mm.get())
}

/// **T1: a ball is wider than the neck it stands on**, whatever the tip. The
/// owner's table runs 1.25 to 1.5; round 5 drew 12 of 35 Bat_Cat_Owl balls as
/// wide as their own shaft (picture 30).
///
/// SABOTAGE: `contact_growth::grow` reducing a ball to its shaft end (the round-5
/// rule) is caught by `a_ball_is_no_wider_than_the_feature_it_touches`, which
/// fails on a ball drawn at the shaft end; here the ratio is read off the
/// emitted records for tips across the owner's range.
#[test]
fn a_ball_is_wider_than_the_neck_it_stands_on() {
    for tip in [0.05, 0.075, 0.1, 0.163, 0.1952, 0.3, 0.6, 1.0] {
        let primitives = lowered(&straight(tip, vec![tip; 60], None));
        let (ball, neck) = ball_and_neck(&primitives);
        assert!(
            ball >= 1.25 * neck - 1e-9 && ball <= 1.5 * neck + 1e-9,
            "tip {tip}: ball {ball} on a neck of {neck} (ratio {})",
            ball / neck
        );
    }
}

/// **T2: no shaft is drawn straight into a contact.** The picture-31 post: a
/// hairline 0.05 mm tip over a ledger of 0.233 mm one layer down (the head
/// minimum a straight fallback head kept). The shaft leaves the ball at the neck
/// (0.05), and is at least the ledger from the first slot down: a cone under the
/// ball, not a flat 0.198 mm top with a 0.067 mm ball dimpling it.
///
/// SABOTAGE: starting the first span at `start_radius.max(need[0])` (the old
/// line in `drawn_chords`) draws the neck at 0.233 and fails the first assertion.
#[test]
fn a_shaft_leaves_its_ball_at_the_neck_and_widens_to_the_ledger() {
    let primitives = lowered(&straight(0.05, vec![0.233; 60], None));
    let (ball, neck) = ball_and_neck(&primitives);
    assert!((neck - 0.05).abs() < 1e-9, "the shaft leaves the ball at the neck: {neck}");
    assert!(ball > 1.25 * neck - 1e-9, "the ball ({ball}) is wider than the neck ({neck})");
    let shaft: Vec<&SupportPrimitive> = primitives
        .iter()
        .filter(|p| {
            matches!(p.segment_kind, SupportSegmentKind::ContactTaper | SupportSegmentKind::Member)
        })
        .collect();
    // The ledger is carried: every frustum below the first reaches it, and the
    // widening is at most the first layers.
    let widest = shaft
        .iter()
        .map(|p| p.start_radius_mm.get().max(p.end_radius_mm.get()))
        .fold(0.0_f64, f64::max);
    assert!((widest - 0.233).abs() < 1e-9, "the ledger is carried down: {widest}");
    // THE HEAD CONE (the owner's `supporttiplength` at a 0.1 mm neck: 1.5 mm): one
    // straight cone from the neck to the ledger, 1.5 mm along the axis.
    let first = shaft[0];
    let length = geom_kernel::vec3::dist(
        first.start_mm.map(Finite::get),
        first.end_mm.map(Finite::get),
    );
    assert!((length - 1.5).abs() < 1e-6, "the head cone is the owner's tip length: {length} mm");
    assert!(
        (first.start_radius_mm.get() - 0.05).abs() < 1e-9
            && (first.end_radius_mm.get() - 0.233).abs() < 1e-9,
        "from the neck to the ledger: {} to {}",
        first.start_radius_mm.get(),
        first.end_radius_mm.get()
    );
    // And the post below it is the ledger, unchanged.
    for span in &shaft[1..] {
        assert!(
            span.start_radius_mm.get() >= 0.233 - 1e-9 && span.end_radius_mm.get() >= 0.233 - 1e-9,
            "below the cone the shaft keeps the ledger: {span:?}"
        );
    }
}

/// A shaft that is exactly as wide as its load needs is its own neck: nothing
/// is drawn but the ball and the shaft, bit for bit as before round 6.
#[test]
fn a_shaft_as_wide_as_its_load_needs_is_its_own_neck() {
    let primitives = lowered(&straight(0.1, vec![0.1; 60], None));
    let shaft: Vec<&SupportPrimitive> = primitives
        .iter()
        .filter(|p| {
            matches!(p.segment_kind, SupportSegmentKind::ContactTaper | SupportSegmentKind::Member)
        })
        .collect();
    assert_eq!(shaft.len(), 1, "one constant span: {shaft:?}");
    assert!((shaft[0].start_radius_mm.get() - 0.1).abs() < 1e-12);
    assert!((shaft[0].end_radius_mm.get() - 0.1).abs() < 1e-12);
}

/// Two arms through the real lowering: a straight post `A` (seat z 3, base z 0,
/// neck 0.05) and a landed arm `B` that ends on `A`'s axis `down_mm` under `A`'s
/// seat. `A`'s ledger is `upper` for its top `upper_mm` and `lower` under it (the
/// step the landing makes).
fn post_with_a_lander(down_mm: f64, upper: f64, upper_mm: f64, lower: f64) -> Vec<SupportPrimitive> {
    let slots = 60_usize;
    let slot_mm = 3.0 / 60.0;
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a small positive slot count")]
    let upper_slots = (upper_mm / slot_mm).round() as usize;
    let profile: Vec<f64> = (0..slots)
        .map(|slot| if slot >= slots - upper_slots { upper } else { lower })
        .collect();
    let host = straight(0.05, profile, None);
    let lander = SweepArm {
        seat_mm: [1.5, 0.0, 3.9],
        base_mm: [0.0, 0.0, 3.0 - down_mm],
        tip_radius_mm: 0.1,
        radius_profile_mm: vec![0.1; 30],
        seat_normal_mm: [0.0, 0.0, -1.0],
        seat_face_point_mm: Some([1.5, 0.0, 3.9]),
        landing: SweepArmLanding::Landed,
        seats_into_model: true,
        head: None,
        contact_ball_radius_mm: None,
    };
    let layers = LayerPlan::new(PLATE, vec![H; 400]).expect("a uniform layer plan");
    let frame = BuildFrame::new(
        Finite::new(PLATE).expect("finite plate"),
        PositiveFinite::new(5.0).expect("positive lift"),
    );
    lower_sweep_arms(&[host, lander], frame, &layers, 100_000, 1 << 30)
        .expect("the lowering refuses only about the request")
        .primitives
}

/// The widest section any frustum on the post's axis (x = y = 0, both ends) has at
/// height `z`, or `None` where none stands.
fn post_radius_at(primitives: &[SupportPrimitive], z: f64) -> Option<f64> {
    primitives
        .iter()
        .filter(|p| {
            matches!(p.segment_kind, SupportSegmentKind::ContactTaper | SupportSegmentKind::Member)
                && p.start_mm[0].get().abs() < 1e-9
                && p.start_mm[1].get().abs() < 1e-9
                && p.end_mm[0].get().abs() < 1e-9
                && p.end_mm[1].get().abs() < 1e-9
        })
        .filter_map(|p| {
            let (z0, z1) = (p.start_mm[2].get(), p.end_mm[2].get());
            let (hi, lo) = (z0.max(z1), z0.min(z1));
            if z > hi + 1e-9 || z < lo - 1e-9 || hi - lo < 1e-9 {
                return None;
            }
            let t = (z0 - z) / (z0 - z1);
            Some(p.start_radius_mm.get() + (p.end_radius_mm.get() - p.start_radius_mm.get()) * t)
        })
        .reduce(f64::max)
}

/// **T3: the head cone never replaces the section a landed load needs** (round 6
/// review). A 0.05 mm neck over a 0.100 mm ledger for the top 0.5 mm and 0.233 mm
/// under a landing 0.5 mm down: the cone (1.5 mm long) used to run through the
/// landing and read 0.123 mm at 0.6 mm down, 28 % of the section the landed load
/// needs. The cone ends at the landing; the section at and under it is the ledger's.
///
/// SABOTAGE: `lander_z_mm` ignored by `head_cone_from_the_neck` (the old cone) fails
/// the section assertion at 0.6 mm down.
#[test]
fn the_head_cone_ends_at_a_landing_and_the_ledger_stands_under_it() {
    let primitives = post_with_a_lander(0.5, 0.1, 0.5, 0.233);
    // Neck at the ball centre, still.
    let neck = post_radius_at(&primitives, 2.999).expect("a section at the top");
    assert!(neck < 0.06, "the shaft still leaves the ball at the neck: {neck}");
    // From the landing's layer down the ledger (0.233) is the section, at every height.
    for tenth_down in 6..=29 {
        let z = 3.0 - f64::from(tenth_down) / 10.0;
        let radius = post_radius_at(&primitives, z).expect("the post stands");
        assert!(
            radius >= 0.233 - 1e-6,
            "{:.1} mm down the post is {radius} mm, the ledger under the landing is 0.233",
            f64::from(tenth_down) / 10.0
        );
    }
    // Above the landing it is still a cone from the neck to the ledger under the landing:
    // narrower than that ledger at 0.2 mm down, wider than the neck.
    let above = post_radius_at(&primitives, 2.8).expect("a section above the landing");
    assert!(
        above > 0.06 && above < 0.233 - 1e-3,
        "a cone from the neck to the ledger stands above the landing: {above}"
    );
}

/// CONTROL for T3: a landing deeper than the cone leaves the owner's 1.5 mm head cone
/// as it was.
#[test]
fn a_landing_under_the_head_cone_leaves_the_cone_alone() {
    let primitives = post_with_a_lander(2.0, 0.1, 2.0, 0.233);
    let first = primitives
        .iter()
        .find(|p| p.segment_kind == SupportSegmentKind::ContactTaper)
        .expect("the post's taper");
    let length = geom_kernel::vec3::dist(
        first.start_mm.map(Finite::get),
        first.end_mm.map(Finite::get),
    );
    assert!((length - 1.5).abs() < 1e-6, "the owner's cone, 1.5 mm: {length}");
}
