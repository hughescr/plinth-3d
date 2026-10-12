//! **The joint is sized for what the contact can pass** (Craig 2026-10-11),
//! graded on a hand-made headed field: a single tip at a known lean, a head
//! another member lands on, and the sabotage that the old law fails.
//!
//! * the drawn cone, joint and sphere radius equal the certified profile, the
//!   payload the adapter hands the lowering carries that profile (R5), and every
//!   frustum and the section at `K` hold at or under the capacity (R4);
//! * the post under `K` is the running maximum of its certified needs, never
//!   narrower going down, never under the joint, never over the old law's radius;
//! * a head another member lands on above its joint is shared and keeps the old
//!   law (R1), counted;
//! * SABOTAGE: the old law draws a joint far over the certified one, so the same
//!   `joint <= certified` assertion goes red with the certificate switched off.

use crate::down_sweep::{resize_heads_certified, resize_heads_to_drawn_posts, SizingPolicy};
use crate::grid::Grid;
use crate::head_carrier_tests::{grid, straight, H};
use crate::heads::{Head, HeadLaw, OldLawReason};
use crate::joint_cert as jc;
use crate::joint_size::JointContext;
use crate::{Landing, SeatFace, SweepStrut};

/// A head nearly horizontal over a vertical post (the old law's fat joint) and a
/// head nearly along its post.
const FLAT: f64 = 10.0;
const STEEP: f64 = 0.1;
const HEAD_LENGTH_MM: f64 = 1.6;
const K_JOINT: usize = 100;
const TIP: f64 = 0.2;
const LINK_MM: f64 = 0.3;
const PITCH_MM: f64 = 0.019;
const COS_MAX: f64 = 0.5373;

fn sizing() -> SizingPolicy {
    SizingPolicy {
        x_allowed: 12.0,
        tip_radius_min_mm: 0.05,
        tip_radius_max_mm: 1.0,
        owner_growth_ceiling_mm: 0.4,
        alpha: 1.0,
        lamina_reach_mm: 2.13,
        straight_reach: true,
        tip_column_roots: true,
        per_event: true,
        handed_column_ledger: true,
        layer_height_mm: H,
        plate_z_mm: 0.0,
    }
}

fn cell_u32(cell: usize) -> u32 {
    u32::try_from(cell).expect("a small grid")
}

fn z(layer: usize) -> f64 {
    f64::from(u32::try_from(layer).expect("small")) * H
}

/// A headed plate arm whose head axis `u = (0, 10, -1)/sqrt 101` is nearly
/// horizontal, with its joint `K` exactly over a cell centre and a post that runs
/// `run_cols` columns away along x to the plate (0 = vertical). The head carries
/// a registered link, so the lowering derives a seat disc for it.
fn field(run_cols: usize, tilt: f64) -> (Grid, Vec<SweepStrut>) {
    let grid = grid();
    let (row, col) = (150, 270);
    let joint_cell = grid.index(row, col);
    let k = [grid.center_x_mm(col), grid.center_y_mm(row), z(K_JOINT)];
    // `u = (0, tilt, -1)/sqrt(1 + tilt^2)`, 1.6 mm long.
    let norm = (1.0 + tilt * tilt).sqrt();
    let u = [0.0, tilt / norm, -1.0 / norm];
    let d = [k[0], k[1] - HEAD_LENGTH_MM * u[1], k[2] - HEAD_LENGTH_MM * u[2]];
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small positive layer and row count from a model-scale length"
    )]
    let (k_top, rows) = (
        K_JOINT + (HEAD_LENGTH_MM * u[2].abs() / H).ceil() as usize,
        (HEAD_LENGTH_MM * u[1] / H).round() as usize,
    );
    let mut strut = straight(&grid, 0, row, col, k_top, 0.06);
    strut.cell = grid.index(row - rows, col);
    strut.center_mm = [d[0], d[1]];
    strut.radius_mm.truncate(k_top);
    *strut.radius_mm.last_mut().expect("a ledger") = TIP;
    let post: Vec<u32> = (0..=K_JOINT)
        .map(|slot| cell_u32(grid.index(row, col - run_cols + (run_cols * slot) / K_JOINT)))
        .collect();
    strut.head = Some(Head {
        contact_mm: d,
        unit: u,
        joint_mm: k,
        k_joint: K_JOINT,
        joint_cell: cell_u32(joint_cell),
        joint_radius_mm: 0.5,
        planned_tip_radius_mm: TIP,
        gap_mm: 0.3,
        cos_lean_max: COS_MAX,
        cells: vec![cell_u32(strut.cell); k_top - K_JOINT],
        ball_allowance_mm: 0.0,
        link_mm: LINK_MM,
        drawn_straight: false,
        law_planned: HeadLaw::envelope(COS_MAX),
        law_drawn: None,
        ledger_radius_mm: Vec::new(),
        certified: None,
        old_law: None,
    });
    let (landing, k_bottom) = (strut.landing, strut.k_bottom);
    strut.set_post(landing, k_bottom, post);
    assert!(strut.head_intact(), "the fixture's own head must be intact");
    strut.head.as_mut().expect("a head").ledger_radius_mm = strut.radius_mm.clone();
    (grid, vec![strut])
}

fn plan() -> plinth_core::LayerPlan {
    plinth_core::LayerPlan::new(0.0, vec![H; 300]).expect("a uniform plan")
}

fn context(plan: &plinth_core::LayerPlan) -> JointContext<'_> {
    JointContext {
        layers: plan,
        taper: crate::adapter::LoadStepTaper::OFF,
        slenderness_ratio: 40.0,
    }
}

/// The head as the lowering seats it, from the one batched disc call.
fn seat_of(grid: &Grid, strut: &SweepStrut, plan: &plinth_core::LayerPlan) -> jc::HeadSeat {
    let (seat, base) = crate::adapter::drawn_ends(grid, 0.0, H, strut);
    let arm = crate::adapter::sweep_arm(strut, seat, base, None, Vec::new());
    let discs = plinth_support::tip_contact_discs(&[arm], plan).expect("discs");
    let disc = discs[0].expect("a disc for a headed tip");
    crate::joint_size::head_seat_of(strut, &disc).expect("a head seat")
}

fn joint_slot() -> usize {
    K_JOINT
}

/// A single tip at a known lean: the certified joint, cone and sphere.
#[test]
fn a_single_tip_is_sized_for_what_its_link_can_pass() {
    let plan = plan();
    let (grid, mut old) = field(0, FLAT);
    assert_eq!(resize_heads_to_drawn_posts(&grid, &mut old, sizing()), 1);
    let old_joint = old[0].radius_mm[joint_slot()];
    let (grid, mut new) = field(0, FLAT);
    assert_eq!(resize_heads_certified(&grid, &mut new, sizing(), &context(&plan), PITCH_MM), 1);
    let head = new[0].head.as_ref().expect("a head");
    let cert = head.certified.as_ref().expect("a single tip is certified");
    assert_eq!(head.old_law, None);
    let profile = &cert.profile;
    // One profile (R5): the cone ends at the joint, the joint is the last knot.
    let last = profile.knots.last().expect("knots");
    assert!((last[1] - profile.joint_radius_mm).abs() < 1e-12);
    assert!((profile.knots[0][1] - TIP).abs() < 1e-12, "the cone starts at the link's radius");
    // The same profile the library gives for the seat the lowering derives.
    let seat = seat_of(&grid, &new[0], &plan);
    let again = jc::certified_profile(&seat, &[], PITCH_MM).expect("a profile");
    assert_eq!(&again, profile, "the stored profile is the certified solve");
    // R4: every frustum and the section at K hold at the capacity.
    let own = seat.own_source();
    for pair in profile.knots.windows(2) {
        let worst = own
            .axis_loads()
            .iter()
            .map(|v| jc::frustum_max(&own, v, pair[0][0], pair[0][1], pair[1][0], pair[1][1]))
            .fold(0.0, f64::max);
        assert!(worst <= 1.0 + 1e-9, "frustum {pair:?} reaches {worst}");
    }
    assert!(cert.drawn_max <= 1.0 + 1e-9, "drawn maximum {}", cert.drawn_max);
    // The joint is smaller than the old law's, which sized it for a load line
    // the contact cannot pass.
    assert!(
        profile.joint_radius_mm < 0.9 * old_joint,
        "certified joint {} against the old law's {old_joint}",
        profile.joint_radius_mm
    );
    // SABOTAGE: the same assertion on the old law (certificate off) goes red.
    assert!(
        old_joint > 1.05 * profile.joint_radius_mm,
        "the old law draws {old_joint}, the certificate {}",
        profile.joint_radius_mm
    );
    // The payload the adapter hands the lowering carries that profile verbatim.
    let payload = crate::adapter::head_payload(&new[0]).expect("a payload");
    assert!(payload.certified);
    assert_eq!(payload.profile_knots_mm, profile.knots);
    assert!((payload.joint_radius_mm - profile.joint_radius_mm).abs() < 1e-12);
    assert!((payload.link_radius_mm - TIP).abs() < 1e-12);
    let old_payload = crate::adapter::head_payload(&old[0]).expect("a payload");
    assert!(!old_payload.certified);
    assert!(old_payload.joint_radius_mm > 1.05 * payload.joint_radius_mm);
}

/// The post under `K` is the running maximum of its needs, never narrower going
/// down, never under the joint, never over the old law's radius, and each slot
/// holds at its own end needs.
#[test]
fn the_post_is_the_running_maximum_of_its_certified_needs() {
    for run_cols in [0, 40] {
        let plan = plan();
        let (grid, mut old) = field(run_cols, STEEP);
        resize_heads_to_drawn_posts(&grid, &mut old, sizing());
        let (grid, mut new) = field(run_cols, STEEP);
        resize_heads_certified(&grid, &mut new, sizing(), &context(&plan), PITCH_MM);
        let head = new[0].head.as_ref().expect("a head");
        let cert = head.certified.as_ref().expect("certified");
        let joint = cert.profile.joint_radius_mm;
        let mut thinner = 0;
        for slot in 0..=joint_slot() {
            assert!(
                new[0].radius_mm[slot] <= old[0].radius_mm[slot].max(joint) * (1.0 + 1e-12) + 1e-12,
                "run {run_cols}: slot {slot}: {} over the old law's {}",
                new[0].radius_mm[slot],
                old[0].radius_mm[slot]
            );
            if new[0].radius_mm[slot] < old[0].radius_mm[slot] - 1e-9 {
                thinner += 1;
            }
            // Below the joint slot (whose value is the section just under K,
            // which the old law also sets apart) the post never narrows going
            // down; the post under K is never thinner than the joint above.
            if slot + 1 < joint_slot() {
                assert!(
                    new[0].radius_mm[slot] >= new[0].radius_mm[slot + 1] - 1e-12,
                    "run {run_cols}: slot {slot} narrows going down"
                );
            }
        }
        assert!(new[0].radius_mm[joint_slot()] >= joint - 1e-12, "the joint slot is never thinner than the joint");
        assert!(new[0].radius_mm[joint_slot() - 1] >= joint - 1e-12, "the post is never thinner than the joint above");
        assert!(
            thinner > 0,
            "run {run_cols}: the certificate thinned no post slot (joint {joint}, old joint slot {}, \
             old post {:?}, new post {:?}, cert slots {}/{}/{})",
            old[0].radius_mm[joint_slot()],
            &old[0].radius_mm[95..=joint_slot()],
            &new[0].radius_mm[95..=joint_slot()],
            cert.slots_certified,
            cert.slots_kept,
            cert.slots_pinned
        );
        assert!(cert.slots_certified > 0, "run {run_cols}: no post slot was sized by the certificate");
        // The ledger is a floor.
        for slot in 0..new[0].radius_mm.len() {
            assert!(new[0].radius_mm[slot] >= head.ledger_radius_mm[slot] - 1e-12);
        }
        // The contact slot is untouched.
        assert_eq!(new[0].radius_mm[new[0].k_top - 1].to_bits(), old[0].radius_mm[old[0].k_top - 1].to_bits());
        // A second sizing from the same input is idempotent.
        let before = new[0].radius_mm.clone();
        resize_heads_certified(&grid, &mut new, sizing(), &context(&plan), PITCH_MM);
        assert_eq!(new[0].radius_mm, before);
    }
}

/// A head another member lands on above its joint is shared: it keeps the old law.
#[test]
fn a_head_another_member_lands_on_is_shared_and_keeps_the_old_law() {
    let plan = plan();
    let (grid, mut old) = field(0, FLAT);
    let lander = |grid: &Grid| {
        let mut s = straight(grid, 1, 150, 276, 90, 0.08);
        s.landing = Landing::Strut { id: 0, layer: K_JOINT + 3 };
        s.k_bottom = K_JOINT + 3;
        s.k_top = K_JOINT + 6;
        s.radius_mm = vec![0.08; 4];
        s.carried_mm2 = vec![0.0; 4];
        s.seat_face = SeatFace::default();
        s
    };
    old.push(lander(&grid));
    resize_heads_to_drawn_posts(&grid, &mut old, sizing());
    let (grid, mut shared) = field(0, FLAT);
    shared.push(lander(&grid));
    resize_heads_certified(&grid, &mut shared, sizing(), &context(&plan), PITCH_MM);
    let head = shared[0].head.as_ref().expect("a head");
    assert!(head.certified.is_none(), "a shared head is not certified");
    assert_eq!(head.old_law, Some(OldLawReason::Shared));
    assert_eq!(shared[0].radius_mm, old[0].radius_mm, "the old law, bit for bit");
    assert!(crate::adapter::head_payload(&shared[0]).is_some_and(|payload| !payload.certified));
}

/// A member of radius 0.08 standing over `(row, col)` from layer `k_bottom` to
/// `k_top`, landed on member 0 at `layer`.
fn lander_on_post(grid: &Grid, row: usize, col: usize, layer: usize, k_top: usize) -> SweepStrut {
    let mut s = straight(grid, 1, row, col, 90, 0.08);
    s.landing = Landing::Strut { id: 0, layer };
    s.k_bottom = layer;
    s.k_top = k_top;
    s.radius_mm = vec![0.08; k_top - layer + 1];
    s.carried_mm2 = vec![0.0; k_top - layer + 1];
    s.seat_face = SeatFace::default();
    s
}

/// Run the certified sizing on the fixture with one extra member and return the
/// head's reason for keeping the old law (`None`: certified).
fn old_law_reason_with(extra: impl Fn(&Grid) -> SweepStrut) -> Option<OldLawReason> {
    let plan = plan();
    let (grid, mut field) = field(0, FLAT);
    field.push(extra(&grid));
    resize_heads_certified(&grid, &mut field, sizing(), &context(&plan), PITCH_MM);
    let head = field[0].head.as_ref().expect("a head");
    assert_eq!(head.certified.is_none(), head.old_law.is_some(), "a head is certified or counted, never both");
    head.old_law
}

/// R1: a member that lands ON THE LAYER OF THE JOINT enters at `K`, where the
/// joint's section carries it, and the certified joint is sized for the head's
/// own contact alone: such a head keeps the old law.
#[test]
fn a_member_landing_on_the_joint_layer_makes_the_head_shared() {
    let reason = old_law_reason_with(|grid| lander_on_post(grid, 150, 290, K_JOINT, K_JOINT + 4));
    assert_eq!(reason, Some(OldLawReason::Shared));
}

/// R1: a member that lands BELOW the joint but whose drawn body crosses the cone
/// feeds the cone where it crosses, not where it lands: the head keeps the old
/// law. The same member standing clear of the head leaves it certified.
#[test]
fn a_member_landing_below_the_joint_but_crossing_the_cone_makes_the_head_shared() {
    let crossing = old_law_reason_with(|grid| lander_on_post(grid, 134, 270, K_JOINT - 3, K_JOINT + 6));
    assert_eq!(crossing, Some(OldLawReason::Shared), "a body through the cone loads it");
    let clear = old_law_reason_with(|grid| lander_on_post(grid, 150, 290, K_JOINT - 3, K_JOINT + 6));
    assert_eq!(clear, None, "a member standing clear below the joint is carried by the post only");
}
