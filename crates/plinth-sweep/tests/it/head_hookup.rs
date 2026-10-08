//! **A headed strut is sized and drawn correctly** (articulated heads, lane
//! `head-hookup`, Craig 2026-10-06; `heads.rs` carries the law).
//!
//! A hand-built field: one headed arm (`u` 41.3 deg from straight down, tip
//! radius 0.05 mm, a straight post from the joint to the plate) and one branch
//! that lands on the post below the joint. It runs through
//! `down_sweep::run` (the per-layer head minimum), then `adapter::arms_of`
//! (the drawing payload) and `to_primitives` (the lowering). There is no
//! producer of heads yet, so the head is built here by hand, from the same law
//! the up-sweep will use.
//!
//! Sabotages, each turning its test red:
//!
//! * the head minimum applied at the contact slot too: the tip rises
//!   (`the_tip_keeps_the_pure_area_law`);
//! * the load-step ceiling at the joint removed: the cone climbs the head
//!   (`a_load_step_cone_ends_at_the_joint`);
//! * the joint held at its planned radius instead of the law at the final tip
//!   (`a_fatter_tip_grows_the_joint_and_the_deficit_is_reported`,
//!   `a_final_tip_below_the_planned_one_shrinks_the_joint_and_the_post_keeps_up`);
//! * the radius profile not cut to the post
//!   (`a_headed_strut_is_sized_and_drawn_as_a_head`).

use plinth_core::{m0_constants, Finite, LayerPlan, PositiveFinite};
use plinth_mesh::make_lonely_peak;
use plinth_support::BuildFrame;
use plinth_sweep::down_sweep::{self, InsertionPolicy, SizingPolicy, UnrootedColumns};
use plinth_sweep::heads::{
    final_joint_radius_mm, head_polyline_mm, head_section_mm, solve_joint, Head,
};
use plinth_sweep::{
    arms_of, to_primitives, DrainKeepOut, Landing, SeatFace, StrutKind, SweepEmission, SweepPlan,
    SweepPolicy, SweepStrut,
};

use crate::harness;

const LIFT_MM: f64 = 5.0;
const MAX_PRIMITIVES: usize = 100_000;
const MAX_PRIMITIVE_BYTES: usize = 1 << 26;
/// `cos` of the lean law's largest angle, the envelope the plan values use.
const GAP_MM: f64 = 0.3;
const PHI_DEG: f64 = 41.3;
/// The seat layer of the headed arm, and the layer the branch lands on.
const K_TOP: usize = 60;
const LANDING_LAYER: usize = 47;

fn cos_lean() -> f64 {
    57.5_f64.to_radians().cos()
}

struct Fixture {
    plan: SweepPlan,
    layers: LayerPlan,
    sizing: SizingPolicy,
    slices: Vec<Vec<plinth_support::perception::SliceSolid>>,
    /// The two struts before the down-sweep: the headed arm, the branch.
    struts: Vec<SweepStrut>,
    k_joint: usize,
    planned_joint_mm: f64,
    /// `|DK|`, the frozen head length the drawn joint is sized over.
    length_mm: f64,
}

fn cell_of(grid: &plinth_sweep::grid::Grid, x: f64, y: f64) -> u32 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a coordinate inside the grid"
    )]
    let (col, row) = (
        grid.col_coord(x).floor() as usize,
        grid.row_coord(y).floor() as usize,
    );
    u32::try_from(grid.index(row, col)).expect("a small grid")
}

/// The headed arm and its branch on a real plan's grid and frame.
fn fixture(headed: bool) -> Fixture {
    fixture_planned(headed, None)
}

/// [`fixture`] with the head PLANNED at `planned_tip_mm` (the policy's own
/// minimum tip when `None`): the planned joint and the frozen `|DK|` follow it,
/// whatever the final tip turns out to be.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "layer counts and offsets of a small fixture"
)]
fn fixture_planned(headed: bool, planned_tip_mm: Option<f64>) -> Fixture {
    let mesh = make_lonely_peak().expect("the lonely peak fixture");
    let (min_z, max_z) = harness::z_span(&mesh);
    let plate_z = min_z - LIFT_MM;
    let h = *m0_constants()
        .expect("registry")
        .evidence()
        .layer_height_default_mm()
        .value();
    let count = (((max_z - plate_z) / h).ceil() as usize).saturating_add(1);
    let layers = LayerPlan::new(plate_z, vec![h; count]).expect("a uniform layer plan");
    let policy = SweepPolicy::from_registry().expect("the registry");
    let mut plan = plinth_sweep::run_sweep(&mesh, &layers, &policy).expect("the sweep emits");
    let grid = plan.grid;

    let c = cos_lean();
    let r_t = planned_tip_mm.unwrap_or(policy.tip_radius_min_mm);
    let joint = solve_joint(r_t, GAP_MM, c, policy.tip_radius_max_mm).expect("a thin tip");
    let phi = PHI_DEG.to_radians();
    let unit = [phi.sin(), 0.0, -phi.cos()];
    // K stands over the centre of a cell, so the straight post is exactly
    // under it.
    let (row, col) = (grid.rows() / 2, grid.cols() / 2);
    let k_xy = [grid.center_x_mm(col), grid.center_y_mm(row)];
    let contact = [
        k_xy[0] - joint.length_mm * unit[0],
        k_xy[1],
        plate_z + K_TOP as f64 * h,
    ];
    let joint_mm = [
        contact[0] + joint.length_mm * unit[0],
        contact[1],
        contact[2] + joint.length_mm * unit[2],
    ];
    let k_joint = ((joint_mm[2] - plate_z) / h).floor() as usize;
    let joint_cell = cell_of(&grid, joint_mm[0], joint_mm[1]);
    let seat_cell = cell_of(&grid, contact[0], contact[1]);
    let head_cells: Vec<u32> = (k_joint + 1..=K_TOP)
        .map(|layer| {
            let z = plate_z + layer as f64 * h;
            let t = ((z - contact[2]) / unit[2]).clamp(0.0, joint.length_mm);
            cell_of(&grid, contact[0] + t * unit[0], contact[1])
        })
        .collect();
    assert_eq!(head_cells.last(), Some(&seat_cell));
    let head = Head {
        contact_mm: contact,
        unit,
        joint_mm,
        k_joint,
        joint_cell,
        joint_radius_mm: joint.joint_radius_mm,
        planned_tip_radius_mm: r_t,
        gap_mm: GAP_MM,
        cos_lean_max: c,
        cells: head_cells.clone(),
        ball_allowance_mm: 0.0,
        drawn_straight: false,
        law_planned: plinth_sweep::heads::HeadLaw::envelope(c),
        law_drawn: None,
        ledger_radius_mm: Vec::new(),
    };
    let mut path = vec![joint_cell; k_joint + 1];
    path.extend_from_slice(&head_cells);
    let thickened: Vec<f64> = (0..count)
        .map(|layer| if layer <= LANDING_LAYER { 1.0 } else { 0.0 })
        .collect();
    let host = SweepStrut {
        id: 0,
        cell: seat_cell as usize,
        center_mm: [contact[0], contact[1]],
        kind: StrutKind::Arm,
        min_radius_mm: 0.0,
        thickened_mm: thickened,
        seat_on_strut: None,
        k_top: K_TOP,
        k_bottom: 0,
        landing: Landing::Plate,
        interior: false,
        radius_mm: Vec::new(),
        carried_mm2: Vec::new(),
        covered_demand: Vec::new(),
        inserted_by_load: false,
        path: if headed { path } else { Vec::new() },
        straight_fallback: None,
        fallback_path: Vec::new(),
        head: headed.then_some(head),
        seat_face: SeatFace::default(),
        seat_normal: [0.0; 3],
    };
    // The branch: an arm eight cells off the post, landing on it below the joint.
    let branch_cell = grid.index(row, col + 8);
    let branch = SweepStrut {
        id: 1,
        cell: branch_cell,
        center_mm: [grid.center_x_mm(col + 8), grid.center_y_mm(row)],
        kind: StrutKind::Arm,
        min_radius_mm: 0.0,
        thickened_mm: Vec::new(),
        seat_on_strut: None,
        k_top: LANDING_LAYER + 6,
        k_bottom: LANDING_LAYER,
        landing: Landing::Strut {
            id: 0,
            layer: LANDING_LAYER,
        },
        interior: false,
        radius_mm: Vec::new(),
        carried_mm2: Vec::new(),
        covered_demand: Vec::new(),
        inserted_by_load: false,
        path: Vec::new(),
        straight_fallback: None,
        fallback_path: Vec::new(),
        head: None,
        seat_face: SeatFace::default(),
        seat_normal: [0.0; 3],
    };
    plan.struts = Vec::new();
    let sizing = SizingPolicy {
        x_allowed: policy.x_allowed,
        tip_radius_min_mm: policy.tip_radius_min_mm,
        tip_radius_max_mm: policy.tip_radius_max_mm,
        owner_growth_ceiling_mm: policy.owner_growth_ceiling_mm,
        alpha: plinth_sweep::peel::alpha(policy.peel_field_mode),
        lamina_reach_mm: policy.kappa * h,
        straight_reach: policy.straight_reach,
        tip_column_roots: true,
        per_event: true,
        handed_column_ledger: true,
        layer_height_mm: h,
        plate_z_mm: plate_z,
    };
    let slices = vec![Vec::new(); plan.layers];
    Fixture {
        plan,
        layers,
        sizing,
        slices,
        struts: vec![host, branch],
        k_joint,
        planned_joint_mm: joint.joint_radius_mm,
        length_mm: joint.length_mm,
    }
}

/// Run the down-sweep over `struts`, put them in the plan, and lower it.
fn size_and_draw(
    fx: &mut Fixture,
    struts: Vec<SweepStrut>,
) -> (Vec<SweepStrut>, Vec<plinth_support::SweepArm>, SweepEmission) {
    let mut struts = struts;
    down_sweep::run(
        &fx.plan.grid,
        &fx.slices,
        &mut struts,
        fx.sizing,
        InsertionPolicy::disabled(),
        &DrainKeepOut::none(),
        &UnrootedColumns::none(),
    );
    fx.plan.struts = struts.clone();
    let arms = arms_of(&fx.plan);
    let frame = BuildFrame::new(
        Finite::new(fx.plan.plate_z_mm).expect("a finite plate"),
        PositiveFinite::new(LIFT_MM).expect("a positive lift"),
    );
    let emission = to_primitives(
        &fx.plan,
        frame,
        &fx.layers,
        MAX_PRIMITIVES,
        MAX_PRIMITIVE_BYTES,
    )
    .expect("the lowering refuses only about the request");
    (struts, arms, emission)
}

#[test]
#[expect(
    clippy::float_cmp,
    reason = "the arm's seat normal and face point are the head's CARRIED unit and contact, \
              copied, never re-derived, so they are pinned bit for bit"
)]
fn a_headed_strut_is_sized_and_drawn_as_a_head() {
    let mut fx = fixture(true);
    let struts = fx.struts.clone();
    let (struts, arms, emission) = size_and_draw(&mut fx, struts);
    let k_joint = fx.k_joint;
    let c = cos_lean();
    // (a) one head drawn; the radius profile is the post only.
    assert_eq!(emission.tips_headed, 1, "the head was drawn as a head");
    assert_eq!(emission.heads_drawn_straight, 0);
    assert_eq!(
        arms[0].radius_profile_mm.len(),
        k_joint - struts[0].k_bottom + 1,
        "the post's slots k_bottom..=k_joint, and not the head's"
    );
    let payload = arms[0].head.as_ref().expect("the arm carries its head");
    let head = struts[0].head.as_ref().expect("the strut keeps its head");
    assert_eq!(arms[0].seat_normal_mm, head.unit);
    assert_eq!(arms[0].seat_face_point_mm, Some(head.contact_mm));
    // (b) the head's section is the tangent polyline of the law, from the
    // contact (s = 0, 1.364 r_t) to the joint (s = |DK|, the joint's radius):
    // at most four frusta, and the knots rise with s.
    let r_t = struts[0].tip_radius_mm();
    let knots = &payload.profile_knots_mm;
    assert_eq!(*knots, head_polyline_mm(r_t, fx.length_mm, c));
    assert!((2..=5).contains(&knots.len()), "at most four frusta: {knots:?}");
    assert!(
        knots.windows(2).all(|w| w[0][0] < w[1][0] && w[0][1] <= w[1][1]),
        "the section grows from the contact to the joint: {knots:?}"
    );
    let at_contact = knots[0][1];
    let neck = r_t / c.sqrt();
    assert!((neck / r_t - 1.364).abs() < 1e-3, "the neck starts at 1.364 r_t");
    assert!(
        knots[0][0] == 0.0 && (at_contact - neck).abs() < 1e-12,
        "the section at the contact is {at_contact}, the neck {neck}"
    );
    let last = knots.last().expect("a polyline");
    assert!(
        (last[0] - fx.length_mm).abs() < 1e-9
            && (last[1] - payload.joint_radius_mm).abs() < 1e-9,
        "the polyline reaches the joint: {knots:?} vs r_K {}",
        payload.joint_radius_mm
    );
    // (c) the joint is the law at the FINAL tip over the frozen |DK|; with the
    // final tip equal to the planned one that is the planned joint, and it
    // stands exactly g off the seat plane.
    let final_joint = final_joint_radius_mm(r_t, fx.length_mm, c);
    assert!(
        (payload.joint_radius_mm - final_joint).abs() < 1e-9
            && (final_joint - fx.planned_joint_mm).abs() < 1e-9,
        "drawn joint {} against the final-tip law {final_joint}, planned {}",
        payload.joint_radius_mm,
        fx.planned_joint_mm
    );
    assert_eq!(emission.joints_grown, 0);
    assert!((emission.max_standoff_deficit_mm).abs() < 1e-12);
    assert_eq!(emission.head_standoff_mm.len(), 1);
    assert!(
        (emission.head_standoff_mm[0] - GAP_MM).abs() < 1e-9,
        "stand-off {} against the gap {GAP_MM}",
        emission.head_standoff_mm[0]
    );
    // The ledger beside the law: the head layers read the head law, and the
    // joint slot holds the joint.
    let slot = |layer: usize| layer - struts[0].k_bottom;
    assert!(struts[0].radius_mm[slot(k_joint)] >= final_joint - 1e-12);
    let h = fx.sizing.layer_height_mm;
    for layer in k_joint + 1..K_TOP - 1 {
        let z = fx.plan.plate_z_mm + layer as f64 * h;
        let s = ((z - head.contact_mm[2]) / head.unit[2]).clamp(0.0, 0.52);
        let law = head_section_mm(r_t, s, c);
        assert!(
            struts[0].radius_mm[slot(layer)] >= law - 1e-12,
            "layer {layer} under the head law"
        );
    }
    // The post is never narrower going down.
    assert!(
        struts[0].radius_mm[..slot(k_joint)]
            .windows(2)
            .all(|w| w[0] >= w[1] - 1e-12),
        "the post narrows going down"
    );
}

#[test]
fn the_tip_keeps_the_pure_area_law() {
    // (d) The tip slot is the ball and the contact: it reads the area law and
    // nothing else, headed or not.
    let mut fx = fixture(true);
    let headed = fx.struts.clone();
    let (headed, _, _) = size_and_draw(&mut fx, headed);
    let mut fx = fixture(false);
    let straight = fx.struts.clone();
    let (straight, _, _) = size_and_draw(&mut fx, straight);
    assert!(straight[0].tip_radius_mm() > 0.0);
    assert!(
        (headed[0].tip_radius_mm() - straight[0].tip_radius_mm()).abs() < 1e-15,
        "the head minimum raised the tip: {} against the area law's {}",
        headed[0].tip_radius_mm(),
        straight[0].tip_radius_mm()
    );
    assert_eq!(headed[0].tip_radius_mm().to_bits(), 0.05_f64.to_bits());
}

#[test]
fn a_load_step_cone_ends_at_the_joint() {
    // (e) The branch lands on the post below the joint; its load step is
    // drawn as a cone from the landing layer upward, and the cone stops at the
    // joint slot instead of climbing the head.
    let mut fx = fixture(true);
    let struts = fx.struts.clone();
    let (struts, arms, emission) = size_and_draw(&mut fx, struts);
    assert_eq!(emission.load_steps_tapered, 1, "the fixture draws one cone");
    assert_eq!(
        emission.load_step_layers_raised,
        fx.k_joint - LANDING_LAYER,
        "the cone raised the layers above the landing up to the joint, no higher"
    );
    // The joint slot is raised by the cone: it stands wider than the drawn
    // joint (the law at the final tip, which is the planned one here).
    let post = &arms[0].radius_profile_mm;
    let final_joint = final_joint_radius_mm(struts[0].tip_radius_mm(), fx.length_mm, cos_lean());
    assert!((final_joint - fx.planned_joint_mm).abs() < 1e-9, "anti-vacuity");
    assert!(post[fx.k_joint] > final_joint + 1e-6);
}

#[test]
fn a_fatter_tip_grows_the_joint_and_the_deficit_is_reported() {
    // (f) The final tip radius is above the planned one: the joint is the law
    // at the final tip over the frozen |DK| (never capped), the stand-off
    // falls under the gap by exactly the growth, and the report says so.
    let mut fx = fixture(true);
    let mut struts = fx.struts.clone();
    struts[0].min_radius_mm = 0.10;
    let (struts, arms, emission) = size_and_draw(&mut fx, struts);
    let c = cos_lean();
    let r_t = struts[0].tip_radius_mm();
    assert!((r_t - 0.10).abs() < 1e-12, "the tip is the minimum, {r_t}");
    let grown = final_joint_radius_mm(r_t, fx.length_mm, c);
    assert!(grown > fx.planned_joint_mm + 0.05, "anti-vacuity: {grown}");
    // The law at the frozen |DK|, not the self-consistent re-solve: a fatter
    // tip's ball does not move K, so the lever stays |DK|.
    let residual = grown.powi(3) - r_t * r_t / c * (grown + 4.0 * fx.length_mm);
    assert!(residual.abs() < 1e-9, "the cubic at |DK| {}: {residual}", fx.length_mm);
    let payload = arms[0].head.as_ref().expect("a head");
    assert!(
        (payload.joint_radius_mm - grown).abs() < 1e-9,
        "the joint is drawn at the grown radius {grown}, not {}",
        payload.joint_radius_mm
    );
    assert_eq!(emission.joints_grown, 1);
    let deficit = grown - fx.planned_joint_mm;
    assert!(
        (emission.max_standoff_deficit_mm - deficit).abs() < 1e-9,
        "deficit {} against {deficit}",
        emission.max_standoff_deficit_mm
    );
    assert_eq!(emission.tips_headed, 1);
    assert!(
        (emission.head_standoff_mm[0] - (GAP_MM - deficit)).abs() < 1e-9,
        "the drawn stand-off {} is the gap less the growth",
        emission.head_standoff_mm[0]
    );
    // The head's neck follows the final tip radius too.
    assert!(
        payload.profile_knots_mm.first().map_or(0.0, |knot| knot[1])
            >= head_section_mm(r_t, 0.0, c) - 1e-12
    );
    let _ = struts;
}

#[test]
fn a_field_with_no_heads_is_the_area_law_and_a_straight_arm() {
    // (g) With no head the ledger is the area law, the member minimum and the
    // thickened layers, nothing else; the arm is straight, with its whole
    // ledger as its profile, and every head counter reads zero.
    let mut fx = fixture(false);
    let struts = fx.struts.clone();
    let (struts, arms, emission) = size_and_draw(&mut fx, struts);
    let host = &struts[0];
    for (slot, radius) in host.radius_mm.iter().enumerate() {
        let thickened = host.thickened_mm.get(slot).copied().unwrap_or(0.0);
        assert!(
            (radius - thickened.max(0.05)).abs() < 1e-15,
            "slot {slot}: {radius} against thickened {thickened}"
        );
    }
    assert!(arms.iter().all(|arm| arm.head.is_none()));
    assert_eq!(arms[0].radius_profile_mm.len(), host.k_top - host.k_bottom);
    assert_eq!(emission.tips_headed, 0);
    assert_eq!(emission.joints_grown, 0);
    assert!(emission.max_standoff_deficit_mm.abs() < 1e-15);
    assert!(emission.head_stalk_mm.is_empty());
}

/// Lean the headed post: its base stands `cols` cells off the joint's column,
/// so the arm is a LEANING plate post and the boss stands over a different foot.
fn lean(fx: &Fixture, host: &mut SweepStrut, cols: usize) {
    let grid = &fx.plan.grid;
    let (row, col) = (grid.rows() / 2, grid.cols() / 2);
    host.path[0] = u32::try_from(grid.index(row, col + cols)).expect("a small grid");
}

#[test]
fn a_joint_the_placement_maximum_would_refuse_still_grows() {
    // Review finding 1: placement refuses a joint above the registered
    // maximum radius (no head exists yet), but a FROZEN head whose final tip
    // is fat must still grow its joint, never cap it. A 0.45 mm tip over the
    // frozen |DK| of this head needs a joint wider than 1.0 mm, the registered
    // maximum radius.
    let mut fx = fixture(true);
    let mut struts = fx.struts.clone();
    struts[0].min_radius_mm = 0.45;
    let (struts, arms, emission) = size_and_draw(&mut fx, struts);
    let c = cos_lean();
    let r_t = struts[0].tip_radius_mm();
    assert!((r_t - 0.45).abs() < 1e-12, "the tip is the minimum, {r_t}");
    assert!(
        solve_joint(r_t, GAP_MM, c, 1.0).is_none(),
        "anti-vacuity: the placement law refuses this joint"
    );
    let slot = |layer: usize| layer - struts[0].k_bottom;
    let r_k = struts[0].radius_mm[slot(fx.k_joint)];
    assert!(r_k > 1.0, "the joint grew past the placement maximum: {r_k}");
    let residual = r_k * r_k * r_k - r_t * r_t / c * (r_k + 4.0 * fx.length_mm);
    assert!(residual >= -1e-9, "the joint carries its own law at |DK|: {residual}");
    let payload = arms[0].head.as_ref().expect("a head");
    assert!((payload.joint_radius_mm - r_k).abs() < 1e-12);
    assert_eq!(emission.joints_grown, 1);
    assert!(
        (emission.max_standoff_deficit_mm - (r_k - fx.planned_joint_mm)).abs() < 1e-9,
        "the deficit is the whole growth: {}",
        emission.max_standoff_deficit_mm
    );
    // The post under the joint is sized for its OWN requirement (Craig's
    // ruling A, v19 fix: the joint size is not carried down the column): at
    // least the section of a lever within 10 percent of |DK|.
    let just_under = struts[0].radius_mm[slot(fx.k_joint) - 1];
    let lower_bound = plinth_sweep::heads::post_section_mm(r_t, r_t, 0.9 * fx.length_mm, c);
    assert!(
        just_under >= lower_bound,
        "post {just_under} under the lever's section {lower_bound}"
    );
}

#[test]
fn a_final_tip_below_the_planned_one_shrinks_the_joint_and_the_post_keeps_up() {
    // The head was PLANNED at a fat tip (0.15 mm) but the final tip is the
    // minimum (0.05 mm). The joint is the law at the FINAL tip over the frozen
    // |DK|, not the planned joint; the post under K is never narrower than it.
    let mut fx = fixture_planned(true, Some(0.15));
    let mut struts = fx.struts.clone();
    // One headed arm: no thickened layers and no branch, so the ledger is the
    // pure area law and the head minimum alone sizes the joint and the post.
    struts[0].thickened_mm = vec![0.0; struts[0].thickened_mm.len()];
    struts.truncate(1);
    let (struts, arms, _) = size_and_draw(&mut fx, struts);
    let c = cos_lean();
    let r_t = struts[0].tip_radius_mm();
    assert!((r_t - 0.05).abs() < 1e-12, "the final tip is the minimum, {r_t}");
    let expected = final_joint_radius_mm(r_t, fx.length_mm, c);
    assert!(
        fx.planned_joint_mm > expected + 0.1,
        "anti-vacuity: planned {} against final {expected}",
        fx.planned_joint_mm
    );
    let slot = |layer: usize| layer - struts[0].k_bottom;
    let joint = struts[0].radius_mm[slot(fx.k_joint)];
    assert!(
        (joint - expected).abs() < 1e-9,
        "the joint slot holds the final-tip law {expected}, not the planned {}: {joint}",
        fx.planned_joint_mm
    );
    // The post is sized for its own requirement and never narrows going down
    // (ruling A: the joint's size is not carried down the column).
    let post = &struts[0].radius_mm[..slot(fx.k_joint)];
    assert!(
        post.windows(2).all(|pair| pair[0] >= pair[1] - 1e-12),
        "the post narrows going down: {post:?}"
    );
    let payload = arms[0].head.as_ref().expect("a head");
    assert!((payload.joint_radius_mm - expected).abs() < 1e-9);
}

#[test]
fn a_headed_leaning_post_is_measured_from_its_joint_over_the_real_plate() {
    // Review findings 2 and 3: the drawn line of a headed leaning post is the
    // lowering's own (from K, the head recognised through the contact seat),
    // and the offsets are read at K and at the base, over the REAL plate (the
    // head's points are absolute, and this plate stands under z = 0).
    let mut fx = fixture(true);
    let mut struts = fx.struts.clone();
    lean(&fx, &mut struts[0], 12);
    let (struts, arms, _) = size_and_draw(&mut fx, struts);
    let plate = fx.plan.plate_z_mm;
    assert!(plate < -1.0, "a lifted model: the plate is under z = 0");
    let flare = plinth_support::SweepRootFlare::registered(fx.sizing.layer_height_mm)
        .expect("the registered flare");
    let head = struts[0].head.as_ref().expect("a head");
    let (top, bottom) = flare
        .plate_shaft_of(&arms[0], plate)
        .expect("a plate post's shaft");
    for axis in 0..3 {
        assert!(
            (top[axis] - head.joint_mm[axis]).abs() < 1e-9,
            "the post starts at K: {top:?} against {:?}",
            head.joint_mm
        );
    }
    let offset = plinth_sweep::adapter::drawn_line_offset(&flare, &arms[0], plate)
        .expect("a leaning post has an offset");
    // The drawn line starts AT the planned top, K.
    assert!(
        offset.0.iter().all(|v| v.abs() < 1e-9),
        "no offset at K: {:?}",
        offset.0
    );
    let base = arms[0].base_mm;
    let expected = (base[0] - head.joint_mm[0]) * (bottom[2] - base[2]) / (head.joint_mm[2] - bottom[2]);
    assert!(expected.abs() > 1e-3, "anti-vacuity: {expected}");
    assert!(
        (offset.1[0] - expected).abs() < 1e-9,
        "offset at the base {:?} against {expected}",
        offset.1
    );
    // The in-field reader, before any face is measured, reads the same line.
    let planned = plinth_sweep::adapter::planned_line_offset(
        &flare,
        &fx.plan.grid,
        plate,
        fx.sizing.layer_height_mm,
        &struts[0],
    )
    .expect("a leaning headed post has a planned offset");
    for axis in 0..3 {
        assert!((planned.0[axis] - offset.0[axis]).abs() < 1e-9, "{planned:?} {offset:?}");
        assert!((planned.1[axis] - offset.1[axis]).abs() < 1e-9, "{planned:?} {offset:?}");
    }
}
