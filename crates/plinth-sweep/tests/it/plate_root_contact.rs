//! **A plate-rooted sweep member reaches the plate** — every branch of the
//! sweep lowering's plate-root chain, one arm at a time.
//!
//! The end-to-end guard is `crates/plinth-pipeline/tests/it/plate_contact.rs`,
//! which reads the production `Round 25` artifact back from disk. It can only
//! see branches that disc exercises, and on it every plate arm is tall enough
//! for a flare, so the SHORT-RUN branch — a plate arm too close to the plate
//! for a root boss, emitted as a straight column and counted in
//! `arms_without_root_flare` — is pinned here instead, beside the flared arm,
//! the mast and a model-landed control.
//!
//! The rule (owner report 2026-09-28, support bases floating in ChituBox): the
//! slice-sweep path emits NO raft (`docs/plans/slice-sweep-core-2026-09-02.md`
//! §14 OR-3), so a plate-rooted member's lowest solid must reach `plate_z`
//! itself, not the `plate_z + CHITUBOX_RAFT_HEIGHT_MM` top of a raft that is
//! not there.

use plinth_core::{m0_constants, Finite, LayerPlan, PositiveFinite};
use plinth_support::{
    lower_sweep_arms, BuildFrame, SupportPrimitive, SupportSegmentKind, SweepArm, SweepArmLanding,
    SweepLowering,
};

const PLATE_Z_MM: f64 = -5.0;
const LIFT_MM: f64 = 5.0;
const PLATE_CONTACT_TOLERANCE_MM: f64 = 1e-4;
const MAX_PRIMITIVES: usize = 1_000;
const MAX_PRIMITIVE_BYTES: usize = 1 << 20;

fn layer_height_mm() -> f64 {
    *m0_constants()
        .expect("registry")
        .evidence()
        .layer_height_default_mm()
        .value()
}

fn flare_factor() -> f64 {
    m0_constants()
        .expect("registry")
        .placeholders()
        .flare_r_factor()
        .diagnostic()
        .get()
}

/// The registered no-bend height a root boss needs above the plate — the
/// larger of `CHITUBOX_ROOT_HEIGHT_MM` and `CHITUBOX_ROOT_LAYERS` layers,
/// read from the registry so the short-run arm below is short by the
/// production's own measure, not by a typed number.
fn root_no_bend_mm() -> f64 {
    let constants = m0_constants().expect("registry");
    let policy = constants.placeholders();
    let layers = policy.chitubox_root_layers().diagnostic().get();
    #[expect(
        clippy::cast_precision_loss,
        reason = "a registered layer count of ten; exact in f64"
    )]
    let layer_run = layers as f64 * layer_height_mm();
    policy
        .chitubox_root_height_mm()
        .diagnostic()
        .get()
        .max(layer_run)
}

fn lower(arm: SweepArm) -> SweepLowering {
    let height = layer_height_mm();
    let layers = LayerPlan::new(PLATE_Z_MM, vec![height; 400]).expect("a uniform layer plan");
    let frame = BuildFrame::new(
        Finite::new(PLATE_Z_MM).expect("finite plate"),
        PositiveFinite::new(LIFT_MM).expect("positive lift"),
    );
    lower_sweep_arms(&[arm], frame, &layers, MAX_PRIMITIVES, MAX_PRIMITIVE_BYTES)
        .expect("the lowering refuses only about the request")
}

fn arm(seat_z_mm: f64, base_z_mm: f64, landing: SweepArmLanding, seats: bool) -> SweepArm {
    SweepArm {
        seat_mm: [1.0, 2.0, seat_z_mm],
        base_mm: [1.0, 2.0, base_z_mm],
        tip_radius_mm: 0.1,
        // A one-entry profile: the ledger's root section is 0.3 mm (the
        // flare is sized from `radius_profile_mm[0]`), constant up the rise.
        radius_profile_mm: vec![0.3],
        seat_normal_mm: [0.0, 0.0, -1.0],
        available_seat_depth_mm: 0.0,
        landing,
        seats_into_model: seats,
    }
}

/// The lowest end point over every primitive the arm lowered to, and the
/// primitive that holds it.
fn lowest(primitives: &[SupportPrimitive]) -> (f64, &SupportPrimitive) {
    primitives
        .iter()
        .map(|primitive| {
            (
                primitive.start_mm[2].get().min(primitive.end_mm[2].get()),
                primitive,
            )
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .expect("the arm lowered to at least one primitive")
}

#[test]
fn a_flared_plate_arm_lands_its_full_flare_on_the_plate() {
    let lowered = lower(arm(
        PLATE_Z_MM + 6.0,
        PLATE_Z_MM,
        SweepArmLanding::Plate,
        true,
    ));
    assert_eq!(lowered.arms_without_root_flare, 0);
    let flares: Vec<&SupportPrimitive> = lowered
        .primitives
        .iter()
        .filter(|primitive| primitive.segment_kind == SupportSegmentKind::RootFlare)
        .collect();
    assert_eq!(
        flares.len(),
        1,
        "a tall plate arm carries exactly one root flare"
    );
    let flare = flares[0];
    assert!(
        (flare.end_mm[2].get() - PLATE_Z_MM).abs() <= PLATE_CONTACT_TOLERANCE_MM,
        "the flare's foot is {:+.4} mm above the plate",
        flare.end_mm[2].get() - PLATE_Z_MM
    );
    let (lowest_z, holder) = lowest(&lowered.primitives);
    assert!((lowest_z - PLATE_Z_MM).abs() <= PLATE_CONTACT_TOLERANCE_MM);
    assert_eq!(holder.segment_kind, SupportSegmentKind::RootFlare);
    // The FULL footprint: the foot radius is the registered flare of the root
    // radius, not something narrower that merely reaches down.
    assert!(
        (flare.end_radius_mm.get() - 0.3 * flare_factor()).abs() <= 1e-12,
        "the flare foot radius is {} mm",
        flare.end_radius_mm.get()
    );
}

#[test]
fn a_mast_lands_its_flare_on_the_plate() {
    let lowered = lower(arm(
        PLATE_Z_MM + 6.0,
        PLATE_Z_MM,
        SweepArmLanding::Plate,
        false,
    ));
    let (lowest_z, holder) = lowest(&lowered.primitives);
    assert_eq!(holder.segment_kind, SupportSegmentKind::RootFlare);
    assert!(
        (lowest_z - PLATE_Z_MM).abs() <= PLATE_CONTACT_TOLERANCE_MM,
        "the mast's foot is {:+.4} mm above the plate",
        lowest_z - PLATE_Z_MM
    );
}

#[test]
fn a_plate_arm_too_short_for_a_boss_still_reaches_the_plate() {
    // Seated just under the no-bend height: no boss fits, so the arm is the
    // straight-column branch — and that column, too, must stand on the plate.
    let seat_z = PLATE_Z_MM + 0.8 * root_no_bend_mm();
    let lowered = lower(arm(seat_z, PLATE_Z_MM, SweepArmLanding::Plate, true));
    // PATH HIT: the short-run branch fired, and nothing else.
    assert_eq!(lowered.arms_without_root_flare, 1);
    assert!(lowered
        .primitives
        .iter()
        .all(|primitive| primitive.segment_kind != SupportSegmentKind::RootFlare));
    let (lowest_z, _) = lowest(&lowered.primitives);
    assert!(
        (lowest_z - PLATE_Z_MM).abs() <= PLATE_CONTACT_TOLERANCE_MM,
        "the short plate column's foot is {:+.4} mm above the plate",
        lowest_z - PLATE_Z_MM
    );
}

/// Negative control: a member that lands on the MODEL (or another arm) keeps
/// its own landing point. The rule moves plate roots only.
#[test]
fn a_landed_arm_keeps_its_own_landing_point() {
    let base_z = PLATE_Z_MM + 3.0;
    let lowered = lower(arm(PLATE_Z_MM + 6.0, base_z, SweepArmLanding::Landed, true));
    let (lowest_z, holder) = lowest(&lowered.primitives);
    assert_eq!(holder.segment_kind, SupportSegmentKind::ContactTaper);
    assert!((lowest_z - base_z).abs() <= 1e-12, "lowest {lowest_z}");
}
