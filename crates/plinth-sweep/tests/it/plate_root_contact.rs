//! **A plate-rooted sweep member stands on a raft pad, and the pad on the
//! plate** — every branch of the sweep lowering's plate-root chain, one arm at
//! a time.
//!
//! The end-to-end guard is `crates/plinth-pipeline/tests/it/plate_contact.rs`,
//! which reads the production `Round 25` artifact back from disk. It can only
//! see branches that disc exercises, and on it every plate arm is tall enough
//! for a flare, so the SHORT-RUN branch — a plate arm too close to the plate
//! for a root boss, emitted as a straight column and counted in
//! `arms_without_root_flare` — is pinned here instead, beside the flared arm,
//! the mast, a wide foot and a model-landed control.
//!
//! The rule history:
//! - fix/plate-gap (2026-09-28, owner report of support bases floating in
//!   ChituBox): the path emitted NO raft, so a plate root's lowest solid had to
//!   reach `plate_z` itself, not the `plate_z + CHITUBOX_RAFT_HEIGHT_MM` top
//!   of a raft that was not there.
//! - fix/sweep-raft (2026-09-29, owner report that ChituBox's own raft caught
//!   only some feet; `raft-diagnosis.md`): the path emits the legacy
//!   bottom-connected raft again. Every plate foot ends on its PAD top,
//!   `NATIVE_FOOT_PAD_OVERLAP_MM` into the pad as native feet do, and the pad
//!   stands on `plate_z`. The pad is at least as wide as the foot (owner
//!   ruling: grow the pad, never cap the foot). The tests below were re-pointed
//!   from "the flare's foot is on the plate" to "the flare's foot is on its
//!   pad, and the pad is on the plate".

use plinth_core::{m0_constants, Finite, LayerPlan, PositiveFinite};
use plinth_support::{
    lower_sweep_arms, BuildFrame, SupportPrimitive, SupportSegmentKind, SweepArm, SweepArmLanding,
    SweepLowering, NATIVE_FOOT_PAD_OVERLAP_MM,
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

fn raft_height_mm() -> f64 {
    m0_constants()
        .expect("registry")
        .placeholders()
        .chitubox_raft_height_mm()
        .diagnostic()
        .get()
}

fn native_pad_diameter_mm() -> f64 {
    m0_constants()
        .expect("registry")
        .placeholders()
        .chitubox_pad_diameter_mm()
        .diagnostic()
        .get()
}

/// Where every plate foot must end: on the pad top, reaching the native
/// overlap into it.
fn foot_z_mm() -> f64 {
    PLATE_Z_MM + raft_height_mm() - NATIVE_FOOT_PAD_OVERLAP_MM
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

fn lower_all(arms: &[SweepArm]) -> SweepLowering {
    let height = layer_height_mm();
    let layers = LayerPlan::new(PLATE_Z_MM, vec![height; 400]).expect("a uniform layer plan");
    let frame = BuildFrame::new(
        Finite::new(PLATE_Z_MM).expect("finite plate"),
        PositiveFinite::new(LIFT_MM).expect("positive lift"),
    );
    lower_sweep_arms(arms, frame, &layers, MAX_PRIMITIVES, MAX_PRIMITIVE_BYTES)
        .expect("the lowering refuses only about the request")
}

fn lower(arm: SweepArm) -> SweepLowering {
    lower_all(&[arm])
}

fn arm_at(
    xy: [f64; 2],
    seat_z_mm: f64,
    base_z_mm: f64,
    root_radius_mm: f64,
    landing: SweepArmLanding,
    seats: bool,
) -> SweepArm {
    SweepArm {
        seat_mm: [xy[0], xy[1], seat_z_mm],
        base_mm: [xy[0], xy[1], base_z_mm],
        tip_radius_mm: 0.1,
        // A one-entry profile: the ledger's root section (the flare is sized
        // from `radius_profile_mm[0]`), constant up the rise.
        radius_profile_mm: vec![root_radius_mm],
        seat_normal_mm: [0.0, 0.0, -1.0],
        seat_face_point_mm: None,
        landing,
        seats_into_model: seats,
    }
}

fn arm(seat_z_mm: f64, base_z_mm: f64, landing: SweepArmLanding, seats: bool) -> SweepArm {
    arm_at([1.0, 2.0], seat_z_mm, base_z_mm, 0.3, landing, seats)
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

/// The lowest end point over every NON-raft primitive: where the support
/// itself stops.
fn lowest_support(primitives: &[SupportPrimitive]) -> (f64, &SupportPrimitive) {
    primitives
        .iter()
        .filter(|primitive| {
            !matches!(
                primitive.segment_kind,
                SupportSegmentKind::RaftPad | SupportSegmentKind::RaftBridge
            )
        })
        .map(|primitive| {
            (
                primitive.start_mm[2].get().min(primitive.end_mm[2].get()),
                primitive,
            )
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .expect("the arm lowered to at least one support primitive")
}

fn pads(lowered: &SweepLowering) -> Vec<&SupportPrimitive> {
    lowered
        .primitives
        .iter()
        .filter(|primitive| primitive.segment_kind == SupportSegmentKind::RaftPad)
        .collect()
}

/// One pad, on the plate, centred under `xy`: the raft half of the rule.
fn assert_one_pad_on_the_plate_under(lowered: &SweepLowering, xy: [f64; 2]) -> f64 {
    let pads = pads(lowered);
    assert_eq!(pads.len(), 1, "one plate foot, one pad");
    assert_eq!(lowered.raft_pads, 1);
    let pad = pads[0];
    assert!(
        (pad.start_mm[2].get() - PLATE_Z_MM).abs() <= PLATE_CONTACT_TOLERANCE_MM,
        "the pad stands {:+.4} mm off the plate",
        pad.start_mm[2].get() - PLATE_Z_MM
    );
    assert!(
        (pad.start_mm[0].get() - xy[0]).abs() <= 1e-12
            && (pad.start_mm[1].get() - xy[1]).abs() <= 1e-12,
        "the pad is not centred under the foot"
    );
    // `end_radius` of a pad is its HEIGHT (the type-6 units trap).
    assert!((pad.end_radius_mm.get() - raft_height_mm()).abs() <= 1e-12);
    let (lowest_z, holder) = lowest(&lowered.primitives);
    assert_eq!(holder.segment_kind, SupportSegmentKind::RaftPad);
    assert!((lowest_z - PLATE_Z_MM).abs() <= PLATE_CONTACT_TOLERANCE_MM);
    // `start_radius` of a pad is its DIAMETER.
    pad.start_radius_mm.get() * 0.5
}

#[test]
fn a_flared_plate_arm_stands_its_full_flare_on_a_raft_pad() {
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
        (flare.end_mm[2].get() - foot_z_mm()).abs() <= 1e-12,
        "the flare's foot is {:+.4} mm from its pad top",
        flare.end_mm[2].get() - (PLATE_Z_MM + raft_height_mm())
    );
    let (support_z, holder) = lowest_support(&lowered.primitives);
    assert_eq!(holder.segment_kind, SupportSegmentKind::RootFlare);
    assert!((support_z - foot_z_mm()).abs() <= 1e-12);
    // The FULL footprint: the foot radius is the registered flare of the root
    // radius, not something narrower that merely reaches down.
    let foot_radius = 0.3 * flare_factor();
    assert!(
        (flare.end_radius_mm.get() - foot_radius).abs() <= 1e-12,
        "the flare foot radius is {} mm",
        flare.end_radius_mm.get()
    );
    let pad_radius = assert_one_pad_on_the_plate_under(&lowered, [1.0, 2.0]);
    assert!(
        pad_radius >= foot_radius,
        "pad {pad_radius} under foot {foot_radius}"
    );
    // A 0.48 mm foot fits the native 2.2 mm pad: nothing grown, one pad, no
    // bridge.
    assert_eq!(lowered.raft_pads_grown, 0);
    assert!((pad_radius * 2.0 - native_pad_diameter_mm()).abs() <= 1e-12);
    assert_eq!(lowered.raft_bridges, 0);
}

/// Owner ruling 2026-09-29: GROW THE PAD, NEVER CAP THE FOOT. A foot wider
/// than the native pad keeps its full radius, and its pad grows past it.
#[test]
fn a_foot_wider_than_the_native_pad_grows_its_pad_and_keeps_its_radius() {
    let root = 1.0;
    let lowered = lower(arm_at(
        [1.0, 2.0],
        PLATE_Z_MM + 6.0,
        PLATE_Z_MM,
        root,
        SweepArmLanding::Plate,
        true,
    ));
    let foot_radius = root * flare_factor();
    assert!(
        2.0 * foot_radius > native_pad_diameter_mm(),
        "the premise: this foot is wider than the native pad"
    );
    let flare = lowered
        .primitives
        .iter()
        .find(|primitive| primitive.segment_kind == SupportSegmentKind::RootFlare)
        .expect("a flare");
    assert!(
        (flare.end_radius_mm.get() - foot_radius).abs() <= 1e-12,
        "the foot was capped to {}",
        flare.end_radius_mm.get()
    );
    let pad_radius = assert_one_pad_on_the_plate_under(&lowered, [1.0, 2.0]);
    // PATH HIT: the grow branch fired.
    assert_eq!(lowered.raft_pads_grown, 1);
    // Strictly wider: the pad is emitted as a polygon INSCRIBED in its record
    // circle, so a pad exactly as wide as the foot would leave the foot's ring
    // poking out between the polygon's corners. The end-to-end test grades
    // that coverage on the emitted triangles themselves.
    assert!(
        pad_radius > foot_radius,
        "pad radius {pad_radius} does not exceed foot radius {foot_radius}"
    );
}

#[test]
fn a_mast_stands_its_flare_on_a_raft_pad() {
    let lowered = lower(arm(
        PLATE_Z_MM + 6.0,
        PLATE_Z_MM,
        SweepArmLanding::Plate,
        false,
    ));
    let (support_z, holder) = lowest_support(&lowered.primitives);
    assert_eq!(holder.segment_kind, SupportSegmentKind::RootFlare);
    assert!(
        (support_z - foot_z_mm()).abs() <= 1e-12,
        "the mast's foot is {:+.4} mm from its pad top",
        support_z - (PLATE_Z_MM + raft_height_mm())
    );
    assert_one_pad_on_the_plate_under(&lowered, [1.0, 2.0]);
}

#[test]
fn a_plate_arm_too_short_for_a_boss_still_stands_on_a_raft_pad() {
    // Seated just under the no-bend height: no boss fits, so the arm is the
    // straight-column branch — and that column, too, must stand on its pad.
    let seat_z = PLATE_Z_MM + 0.8 * root_no_bend_mm();
    let lowered = lower(arm(seat_z, PLATE_Z_MM, SweepArmLanding::Plate, true));
    // PATH HIT: the short-run branch fired, and nothing else.
    assert_eq!(lowered.arms_without_root_flare, 1);
    assert!(lowered
        .primitives
        .iter()
        .all(|primitive| primitive.segment_kind != SupportSegmentKind::RootFlare));
    // The COLUMN's foot: the lowest shaft record. (On this deliberately short
    // fixture, 0.4 mm above the plate, the tip ball's own lower pole dips
    // into the pad; a real model stands the registered lift above it.)
    let (column_z, column) = lowered
        .primitives
        .iter()
        .filter(|primitive| {
            matches!(
                primitive.segment_kind,
                SupportSegmentKind::ContactTaper | SupportSegmentKind::Member
            )
        })
        .map(|primitive| {
            (
                primitive.start_mm[2].get().min(primitive.end_mm[2].get()),
                primitive,
            )
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .expect("a shaft");
    assert!(
        (column_z - foot_z_mm()).abs() <= 1e-12,
        "the short plate column's foot is {:+.4} mm from its pad top",
        column_z - (PLATE_Z_MM + raft_height_mm())
    );
    let column_foot = column.end_radius_mm.get();
    let pad_radius = assert_one_pad_on_the_plate_under(&lowered, [1.0, 2.0]);
    assert!(pad_radius >= column_foot);
}

/// Two plate feet: two pads and ONE bridge between them, so the raft is one
/// piece. Bridges come before pads, the native record order, and every raft
/// record comes after every support record.
#[test]
fn two_plate_feet_get_two_pads_joined_by_one_bridge() {
    let lowered = lower_all(&[
        arm_at(
            [1.0, 2.0],
            PLATE_Z_MM + 6.0,
            PLATE_Z_MM,
            0.3,
            SweepArmLanding::Plate,
            true,
        ),
        arm_at(
            [6.0, 2.0],
            PLATE_Z_MM + 6.0,
            PLATE_Z_MM,
            0.3,
            SweepArmLanding::Plate,
            true,
        ),
    ]);
    assert_eq!(lowered.raft_pads, 2);
    assert_eq!(lowered.raft_bridges, 1);
    let kinds: Vec<SupportSegmentKind> = lowered
        .primitives
        .iter()
        .map(|primitive| primitive.segment_kind)
        .collect();
    let first_raft = kinds
        .iter()
        .position(|kind| {
            matches!(
                kind,
                SupportSegmentKind::RaftPad | SupportSegmentKind::RaftBridge
            )
        })
        .expect("a raft");
    assert_eq!(
        &kinds[first_raft..],
        &[
            SupportSegmentKind::RaftBridge,
            SupportSegmentKind::RaftPad,
            SupportSegmentKind::RaftPad
        ]
    );
    let bridge = &lowered.primitives[first_raft];
    let ends = [
        [bridge.start_mm[0].get(), bridge.start_mm[1].get()],
        [bridge.end_mm[0].get(), bridge.end_mm[1].get()],
    ];
    assert!(
        ends.contains(&[1.0, 2.0]) && ends.contains(&[6.0, 2.0]),
        "{ends:?}"
    );
    // The raft is resin the file costs, and it is published apart.
    assert!(lowered.raft_volume_mm3 > 0.0);
    assert!(lowered.drawn_volume_mm3 > lowered.raft_volume_mm3);
}

/// Negative control: a member that lands on the MODEL (or another arm) keeps
/// its own landing point, and gets no pad. The rule moves plate roots only.
#[test]
fn a_landed_arm_keeps_its_own_landing_point_and_gets_no_pad() {
    let base_z = PLATE_Z_MM + 3.0;
    let lowered = lower(arm(PLATE_Z_MM + 6.0, base_z, SweepArmLanding::Landed, true));
    let (lowest_z, holder) = lowest(&lowered.primitives);
    assert_eq!(holder.segment_kind, SupportSegmentKind::ContactTaper);
    assert!((lowest_z - base_z).abs() <= 1e-12, "lowest {lowest_z}");
    assert_eq!(lowered.raft_pads, 0);
    assert_eq!(lowered.raft_bridges, 0);
    assert!(pads(&lowered).is_empty());
}
