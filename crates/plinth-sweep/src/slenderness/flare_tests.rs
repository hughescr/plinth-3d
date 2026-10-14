//! Flare lane tests (round 5, 2026-10-14): a model foot is read through its
//! neck and cone, the thickening search sees it, a bay-local thickening is
//! planned, checked on a connected frame and committed as planned, and every
//! production panel carries the foot. Each test names the sabotage that must
//! make it fail.

use super::{
    member_runs, SlendernessContext, SlendernessLaw, SlendernessOutcome, TieState, UnbracedRun,
};
use crate::{Landing, StrutKind, SweepStrut};

const H: f64 = 0.05;

/// The tests' law: `S` = 40 (the sweep's), model feet read at `X_ALLOWED` 12.
fn feet_law() -> SlendernessLaw {
    SlendernessLaw::new(40.0, 0.05, 1.0, 32.5).with_foot_sizing(12.0)
}

/// The same law reading no feet (the engine before the flare lane).
fn plain_law() -> SlendernessLaw {
    SlendernessLaw::new(40.0, 0.05, 1.0, 32.5)
}

fn context<'a>(
    grid: &'a crate::grid::Grid,
    history: &'a crate::up_sweep::ColumnHistory,
    keep_out: &'a crate::drain::DrainKeepOut,
    lean: &'a crate::lean::LeanLaw,
    law: &'a SlendernessLaw,
) -> SlendernessContext<'a> {
    SlendernessContext {
        grid,
        history,
        keep_out,
        lean,
        law,
        layer_height_mm: H,
        layers: 400,
        flare_factor: 1.6,
        mast_root_clearance: true,
        final_clearance_mm: 0.3,
        member_clearance: true,
        model_surface: None,
        plate_z_mm: 0.0,
        shaft_landings: true,
        merge: true,
        brace: true,
        thicken: true,
        mast: true,
        max_struts: 1_000,
        x_allowed: 12.0,
        seat_faces: crate::tip_landing::SeatFaceRule::from_overhang_band(45.0, 25.0),
        load_step_taper: crate::adapter::LoadStepTaper::of(lean),
        brace_cap: true,
        brace_window: true,
        frame_bands: true,
        frame_ledger: true,
        model_tie: true,
    }
}

/// A vertical post of `k_top` layers whose ledger radius at layer `j` is
/// `radius(j)`, standing on `landing`. A zero carried load: its contact is the
/// registered minimum tip (0.05 mm).
fn post(
    grid: &crate::grid::Grid,
    id: u32,
    col: usize,
    k_top: usize,
    landing: Landing,
    radius: &dyn Fn(usize) -> f64,
) -> SweepStrut {
    let row = 100;
    SweepStrut {
        id,
        cell: grid.index(row, col),
        center_mm: [grid.center_x_mm(col), grid.center_y_mm(row)],
        kind: StrutKind::Arm,
        min_radius_mm: 0.0,
        thickened_mm: Vec::new(), foot_radius_mm: None,
        seat_on_strut: None,
        k_top,
        k_bottom: 0,
        landing,
        interior: false,
        radius_mm: (0..k_top).map(radius).collect(),
        carried_mm2: vec![0.0; k_top],
        covered_demand: Vec::new(),
        held_births: Vec::new(),
        twin_host: false,
        inserted_by_load: false,
        path: Vec::new(),
        seat_face: crate::SeatFace::default(),
        seat_normal: [0.0; 3],
        straight_fallback: None,
        fallback_path: Vec::new(),
        head: None,
    }
}

fn over_runs(
    grid: &crate::grid::Grid,
    state: &TieState,
    struts: &[SweepStrut],
    law: &SlendernessLaw,
    index: usize,
) -> Vec<UnbracedRun> {
    member_runs(grid, &struts[index], &state.ties[index], &state.profiles[index], H, law)
        .into_iter()
        .filter(UnbracedRun::over_limit)
        .collect()
}

/// The carrot: a 11.3 mm post, 0.404 mm at its root tapering to the 0.05 mm
/// tip, reads as holding (0.98 of the pixel) as a plate post, and as far over
/// its limit as its neck makes it when it stands on a 0.05 mm model contact.
///
/// SABOTAGE: reading the run without its foot (drop the neck slots) makes the
/// model-footed post read as the plate post, and the foot assertions fail.
#[test]
fn a_foot_run_is_read_through_its_neck() {
    let grid = crate::grid::lay_out_at(0.0, 0.0, H, 200, 200);
    let history = crate::up_sweep::ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let law = feet_law();
    let context = context(&grid, &history, &keep_out, &lean, &law);
    let taper = |j: usize| 0.404 + (0.05 - 0.404) * (j as f64) / 225.0;
    let on_plate = vec![post(&grid, 0, 100, 226, Landing::Plate, &taper)];
    let on_model = vec![post(&grid, 0, 100, 226, Landing::Model { layer: 0 }, &taper)];
    let plate_state = TieState::build(&context, &on_plate);
    let model_state = TieState::build(&context, &on_model);
    let plate_runs = member_runs(
        &grid,
        &on_plate[0],
        &plate_state.ties[0],
        &plate_state.profiles[0],
        H,
        &law,
    );
    assert_eq!(plate_runs.len(), 1, "{plate_runs:?}");
    assert!(plate_runs[0].foot.is_none() && plate_runs[0].neck_share == 0.0);
    assert!(
        !plate_runs[0].over_limit(),
        "on the plate the carrot holds: {:?}",
        plate_runs[0]
    );
    let model_runs = over_runs(&grid, &model_state, &on_model, &law, 0);
    assert_eq!(model_runs.len(), 1, "on a hairline contact the same post is over its limit");
    let run = model_runs[0];
    assert!(run.foot.is_some());
    assert!(
        (10.0..40.0).contains(&run.neck_share),
        "the neck and cone use many pixels of the one: N = {}",
        run.neck_share
    );
    // The shaft alone, clamped at the contact, still holds: the neck is the
    // whole difference.
    assert!(run.length_mm <= law.max_unbraced_mm(run.shaft_radius_mm) * 1.05);
    // A law that reads no feet sees the plate reading.
    let blind = plain_law();
    let blind_runs = member_runs(
        &grid,
        &on_model[0],
        &model_state.ties[0],
        &model_state.profiles[0],
        H,
        &blind,
    );
    assert!(!blind_runs[0].over_limit() && blind_runs[0].foot.is_none());
}

/// The thicken rung sizes a foot run through its neck, by the real planning
/// path, and refuses one the neck cannot hold.
///
/// * A 0.05 mm post 11.3 mm long on a hairline contact: no root holds it. The
///   rung returns false, counts `thicken_refused_neck`, and writes nothing.
/// * A post short enough for the neck to share the pixel (N between 0 and 1):
///   the rung thickens it, the committed profile re-read through the
///   measurement is inside its limit with the neck in, and its root is larger
///   than a law blind to feet would have chosen (the blind root leaves the run
///   over its limit once the neck is read).
///
/// SABOTAGE: remove the refusal (return the old Radius answer) and the first
/// half fails; size the root by the shaft alone and the second half fails.
#[test]
fn thicken_refuses_a_foot_run_its_neck_cannot_hold_and_sizes_the_rest() {
    let grid = crate::grid::lay_out_at(0.0, 0.0, H, 200, 200);
    let history = crate::up_sweep::ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let law = feet_law();
    let context = context(&grid, &history, &keep_out, &lean, &law);
    // Half one.
    let mut struts = vec![post(&grid, 0, 100, 226, Landing::Model { layer: 0 }, &|_| 0.05)];
    let mut state = TieState::build(&context, &struts);
    let over = over_runs(&grid, &state, &struts, &law, 0);
    assert_eq!(over.len(), 1);
    let mut outcome = SlendernessOutcome::default();
    // The shaft cannot hold the run through a 0.05 mm neck at any width, so the
    // neck is what grows (round 6, the enlarge-foot rung): counted as a neck
    // refusal first, then handed to the foot.
    assert!(super::thicken(&context, &mut state, &mut struts, &over[0], &mut outcome));
    assert_eq!(outcome.thicken_refused_neck, 1, "{outcome:?}");
    assert_eq!(outcome.feet_enlarged, 1, "{outcome:?}");
    assert!(
        struts[0].foot_radius_mm.is_some_and(|radius| radius > 0.05 + 1e-9),
        "the foot is wider than the hairline it was: {:?}",
        struts[0].foot_radius_mm
    );
    // A run no foot up to the registered maximum holds goes on to the other rungs:
    // the store is put back, nothing is written, and the refusal is counted.
    let mut hopeless = vec![post(&grid, 0, 100, 4000, Landing::Model { layer: 0 }, &|_| 0.05)];
    let hopeless_context = super::SlendernessContext { layers: 4001, ..context };
    let mut hopeless_state = TieState::build(&hopeless_context, &hopeless);
    let hopeless_over = over_runs(&grid, &hopeless_state, &hopeless, &law, 0);
    let mut hopeless_outcome = SlendernessOutcome::default();
    assert!(!super::thicken(
        &hopeless_context,
        &mut hopeless_state,
        &mut hopeless,
        &hopeless_over[0],
        &mut hopeless_outcome
    ));
    assert_eq!(hopeless_outcome.foot_enlarge_refused, 1, "{hopeless_outcome:?}");
    assert_eq!(hopeless[0].foot_radius_mm, None, "the store is put back");
    assert!(hopeless[0].thickened_mm.is_empty(), "nothing is written for a run no foot holds");
    // The same post on the plate is thickened: the refusal is the foot's.
    let mut plate = vec![post(&grid, 0, 100, 226, Landing::Plate, &|_| 0.05)];
    let mut plate_state = TieState::build(&context, &plate);
    let plate_over = over_runs(&grid, &plate_state, &plate, &law, 0);
    let mut plate_outcome = SlendernessOutcome::default();
    assert!(super::thicken(&context, &mut plate_state, &mut plate, &plate_over[0], &mut plate_outcome));
    // Half two: find a length where the neck takes part of the pixel.
    let mut found = None;
    for k_top in (40..=140).step_by(4) {
        let probe = vec![post(&grid, 0, 100, k_top, Landing::Model { layer: 0 }, &|_| 0.05)];
        let probe_state = TieState::build(&context, &probe);
        let probe_over = over_runs(&grid, &probe_state, &probe, &law, 0);
        if let Some(run) = probe_over.first() {
            if (0.2..0.9).contains(&run.neck_share) {
                found = Some(k_top);
                break;
            }
        }
    }
    let k_top = found.expect("a length whose neck takes a share of the pixel exists");
    let thicken_with = |law: &SlendernessLaw| {
        let context = super::SlendernessContext { law, ..context };
        let mut struts = vec![post(&grid, 0, 100, k_top, Landing::Model { layer: 0 }, &|_| 0.05)];
        let mut state = TieState::build(&context, &struts);
        let over = over_runs(&grid, &state, &struts, law, 0);
        let mut outcome = SlendernessOutcome::default();
        let done = super::thicken(&context, &mut state, &mut struts, &over[0], &mut outcome);
        (done, struts, state, outcome)
    };
    let (done, thick, state, outcome) = thicken_with(&law);
    assert!(done, "a neck that holds part of the pixel leaves a shaft that can hold the rest: {outcome:?}");
    assert_eq!(outcome.thicken_refused_neck, 0);
    let after = member_runs(&grid, &thick[0], &state.ties[0], &state.profiles[0], H, &law);
    assert!(
        after.iter().all(|run| !run.over_limit()),
        "re-read through the neck the thickened run holds: {after:?}"
    );
    let (blind_done, blind, blind_state, _) = thicken_with(&plain_law());
    assert!(blind_done);
    assert!(
        thick[0].thickened_mm[0] > blind[0].thickened_mm[0],
        "the root seen through the neck ({}) is larger than the blind root ({})",
        thick[0].thickened_mm[0],
        blind[0].thickened_mm[0]
    );
    let blind_after = member_runs(&grid, &blind[0], &blind_state.ties[0], &blind_state.profiles[0], H, &law);
    assert!(
        blind_after.iter().any(UnbracedRun::over_limit),
        "the blind root leaves the run over its limit once the neck is read: {blind_after:?}"
    );
}

/// **T5 (round 6): an enlarged foot is the foot that is read, measured and
/// drawn** (challenger point 3). A 0.05 mm hairline foot under an 11.3 mm run no
/// shaft can hold is enlarged by the rung; the radius it stores is the one the
/// structural reading takes ([`SlendernessLaw::foot_contact`], which
/// `run_reading` and the frame checks call), the one the final measurement takes
/// (`model_feet::foot_radius_mm`, the same function, which a late measurement
/// can only raise), the one the adapter hands the lowering, and the radius of the
/// foot ball the lowering emits.
///
/// SABOTAGE: an `enlarge_foot` that returns false (the rung off) leaves the
/// store `None` and fails the first assertion; a measurement that sized the foot
/// by the area law alone (the radius before this round, 0.05 mm) fails the
/// second; a lowering that took the root radius (not the stored one) fails the
/// last.
#[test]
fn an_enlarged_foot_is_the_foot_that_is_read_measured_and_drawn() {
    use plinth_core::{Finite, LayerPlan, PositiveFinite};
    use plinth_support::{lower_sweep_arms, BuildFrame, SupportSegmentKind};
    let grid = crate::grid::lay_out_at(0.0, 0.0, H, 200, 200);
    let history = crate::up_sweep::ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    // The production law: the owner's foot rule on, as the final measurement has it.
    let law = feet_law().with_owner_foot_rule();
    let context = context(&grid, &history, &keep_out, &lean, &law);
    let mut struts = vec![post(&grid, 0, 100, 226, Landing::Model { layer: 0 }, &|_| 0.05)];
    let mut state = TieState::build(&context, &struts);
    let over = over_runs(&grid, &state, &struts, &law, 0);
    let mut outcome = SlendernessOutcome::default();
    assert!(super::thicken(&context, &mut state, &mut struts, &over[0], &mut outcome), "{outcome:?}");
    let enlarged = struts[0].foot_radius_mm.expect("the rung stored the foot it chose");
    assert!(enlarged > 0.05 + 1e-9, "anti-vacuity: the foot grew past the hairline: {enlarged}");
    // THE STRUCTURAL READING takes the stored radius.
    let (read, _) = law.foot_contact(&struts[0]).expect("a model foot is read");
    assert!(read >= enlarged - 1e-12, "the reading ({read}) is the stored foot ({enlarged}) or wider");
    // THE FINAL MEASUREMENT takes it too, and cannot overwrite it with the area law's.
    let body = crate::model_feet::foot_body_radius_mm(&struts[0]);
    let measured = crate::model_feet::foot_radius_mm(0.0, 12.0, 0.05, 1.0, body, struts[0].foot_radius_mm);
    assert!(
        (measured - read).abs() < 1e-12,
        "measurement ({measured}) and reading ({read}) are one function"
    );
    let overwritten = crate::model_feet::foot_radius_mm(0.0, 12.0, 0.05, 1.0, 0.05, None);
    assert!(
        (overwritten - 0.05).abs() < 1e-12 && measured > overwritten + 1e-9,
        "anti-vacuity: the area law alone reads {overwritten}"
    );
    // THE ADAPTER AND THE LOWERING: the foot ball is wider by the owner's ratio.
    let centre = struts[0].center_mm;
    let strut = {
        let mut strut = struts[0].clone();
        strut.seat_face.foot = Some(crate::model_feet::FootFace {
            face: Some(crate::model_feet::LandedFace {
                normal: [0.0, 0.0, 1.0],
                point_mm: [centre[0], centre[1], 0.0],
                face: 0,
            }),
            contact_radius_mm: measured,
            need_mm2: 0.0,
            on_model_cells: 1,
            disc_cells: 1,
            planned_mm2: 0.0,
            real_mm2: None,
        });
        strut
    };
    let arm = crate::adapter::sweep_arm(
        &strut,
        [centre[0], centre[1], 226.0 * H],
        [centre[0], centre[1], 0.0],
        None,
        state.profiles[0].clone(),
    );
    let layers = LayerPlan::new(0.0, vec![H; 400]).expect("a uniform layer plan");
    let frame = BuildFrame::new(
        Finite::new(0.0).expect("finite plate"),
        PositiveFinite::new(5.0).expect("positive lift"),
    );
    let lowered = lower_sweep_arms(std::slice::from_ref(&arm), frame, &layers, 100_000, 1 << 30)
        .expect("the lowering refuses only about the request");
    let root = state.profiles[0].first().copied().expect("a root");
    let drawn_foot = lowered.model_foot_radius_mm[0];
    assert!(
        (drawn_foot - measured.min(root)).abs() < 1e-9,
        "the lowering draws the foot the structure was read through: {drawn_foot} against {measured} (root {root})"
    );
    let foot_ball = lowered
        .primitives
        .iter()
        .rfind(|p| p.segment_kind == SupportSegmentKind::ContactSphere)
        .expect("a foot ball");
    assert!(
        foot_ball.start_radius_mm.get() >= 1.25 * drawn_foot - 1e-9,
        "the ball ({}) is wider than its neck ({drawn_foot})",
        foot_ball.start_radius_mm.get()
    );
}

/// T5. A spindle (thin ends, a wide middle) between two ties is the plan the
/// design rejected: read as the cantilever it is while it prints, it is far
/// over its limit, so no run is ever read as pinned at both ends.
///
/// SABOTAGE: reading a run with the pinned-pinned kernel (`qL^4 / 384 EI`) makes
/// the spindle hold and this fails.
#[test]
fn no_run_is_read_pinned_at_both_ends() {
    let grid = crate::grid::lay_out_at(0.0, 0.0, H, 200, 200);
    let history = crate::up_sweep::ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let law = plain_law();
    let context = context(&grid, &history, &keep_out, &lean, &law);
    // 8 mm, 0.05 mm ends, mid radius L / (1.76 S).
    let mid = 8.0 / (1.76 * 40.0);
    let spindle = |j: usize| {
        let t = j as f64 / 159.0;
        0.05 + (mid - 0.05) * (core::f64::consts::PI * t).sin()
    };
    let struts = vec![post(&grid, 0, 100, 160, Landing::Plate, &spindle)];
    let state = TieState::build(&context, &struts);
    let over = over_runs(&grid, &state, &struts, &law, 0);
    assert_eq!(over.len(), 1, "the spindle reads over its limit as a cantilever");
    assert!(over[0].length_mm > law.max_unbraced_mm(over[0].radius_mm) * 1.5, "{:?}", over[0]);
}

/// Two model-footed posts `gap_cols` cells apart (1 col = 0.05 mm) joined by
/// the `|X|` bands in `bands` (`(bottom, top)` layers), built the way the ladder
/// builds them. Posts are `k_top` layers tall at `radius`.
fn footed_pair<'a>(
    grid: &'a crate::grid::Grid,
    context: &SlendernessContext<'a>,
    bands: &[(usize, usize)],
    k_top: usize,
    radius: f64,
    gap_cols: usize,
    outcome: &mut SlendernessOutcome,
) -> (Vec<SweepStrut>, TieState) {
    let mut struts = vec![
        post(grid, 0, 100, k_top, Landing::Model { layer: 0 }, &|_| radius),
        post(grid, 1, 100 + gap_cols, k_top, Landing::Model { layer: 0 }, &|_| radius),
    ];
    let mut state = TieState::build(context, &struts);
    for &(k_lo, k) in bands {
        let plan = super::CrossPlan {
            a: 0,
            b: 1,
            k,
            k_lo,
            a_top: struts[0].cell,
            b_top: struts[1].cell,
            radius_mm: 0.058,
            first: Vec::new(),
            second: Vec::new(),
            plane: super::plane_between(grid, struts[0].cell, struts[1].cell),
            volume_mm3: 0.0,
            widened: false,
            moved: false,
            moved_off_the_plate: false,
            raises: Vec::new(),
        };
        super::commit_cross(context, &mut state, &mut struts, plan, outcome);
    }
    (struts, state)
}

/// The in-plane run above the top band of `struts[0]`, over its limit.
fn run_above_the_bands(
    grid: &crate::grid::Grid,
    state: &TieState,
    struts: &[SweepStrut],
    law: &SlendernessLaw,
) -> UnbracedRun {
    let runs = over_runs(grid, state, struts, law, 0);
    *runs
        .iter()
        .find(|run| run.bay_bottom.is_some())
        .unwrap_or_else(|| panic!("a run above a band is over its limit: {runs:?}"))
}

/// **A bay-local thickening is planned, checked and committed as one thing**
/// (flare lane, T3; challengers 2 and 6). Two model-footed 0.10 mm posts 2 mm
/// apart with three `|X|` bands (the top one at layers 80..100), 10 mm tall: the
/// run above the top band is over its limit.
///
/// * The plan is bay-local, and its root is the one the connected FRAME holds
///   (the local run's own root is smaller: the integral clamps the base).
/// * It writes nothing below its ramp, its chain is the member alone (no trunk is
///   raised through a bay), and its resin is exactly the layers it writes.
/// * A cured wall beside the post's foot, below the bay, refuses the OLD plan
///   (raised from the foot) and not this one: the clearance read is the plan's.
/// * Committed, the ledger entries below the ramp are untouched.
///
/// SABOTAGE: start the plan at the member's foot (restore `k_bottom`) and the
/// start, chain-below, wall and ledger assertions fail; size the root by the
/// local integral alone and the frame assertion fails.
#[test]
fn a_bay_local_thickening_is_planned_checked_and_committed_as_one_thing() {
    let grid = crate::grid::lay_out_at(0.0, 0.0, H, 200, 200);
    let mut history = crate::up_sweep::ColumnHistory::new(&grid);
    // A cured wall 0.4 mm from the post's axis over the first 15 layers: the
    // post's foot is near it, the bay is far above it.
    for row in 90..=110 {
        let cell = grid.index(row, 108);
        history.model_head[cell] = u32::try_from(history.model_spans.len()).expect("a few spans");
        history.model_spans.push(crate::up_sweep::ShaftSpan {
            k_bottom: 0,
            k_top: 15,
            id: 0,
            next: u32::MAX,
            landable_end: u32::MAX,
        });
    }
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let law = feet_law();
    let context = context(&grid, &history, &keep_out, &lean, &law);
    let mut outcome = SlendernessOutcome::default();
    let bands = [(20, 40), (50, 70), (80, 100)];
    let (mut struts, mut state) = footed_pair(&grid, &context, &bands, 200, 0.10, 40, &mut outcome);
    let run = run_above_the_bands(&grid, &state, &struts, &law);
    assert_eq!((run.k_lo, run.k_top, run.bay_bottom), (100, 200, Some(80)));
    assert!(run.foot.is_none(), "the run is above its band, not on the foot");
    let fits = |post: &crate::foot_neck::Reading| run.length_mm <= law.max_unbraced_mm(post.radius_mm);
    let plan = super::plan_thicken(&context, &state, &struts, &run, &fits)
        .expect("a plan")
        .expect("not refused");
    let local = plan.local.expect("the plan is bay-local");
    assert!(!plan.kept_by_frame && !plan.joint_raise);
    assert!(local.start > struts[0].k_bottom + 1 && local.start <= 80, "{local:?}");
    assert_eq!(plan.chain, vec![(0, run.k_lo)], "no trunk below the bay is raised");
    // Its root is the frame's, larger than the local run's own.
    let local_root = {
        let alone = |post: &crate::foot_neck::Reading| {
            run.length_mm <= law.max_unbraced_mm(post.radius_mm)
        };
        let mut old = super::plan_thicken(
            &SlendernessContext { member_clearance: false, ..context },
            &state,
            &struts[..],
            &UnbracedRun { bay_bottom: None, ..run },
            &alone,
        )
        .expect("a plan")
        .expect("not refused");
        old.local = None;
        old.root
    };
    assert!(plan.root > local_root, "frame root {} against the local run's {local_root}", plan.root);
    let drag = super::local_frame_drag(&context, &state, &struts, &run, &plan).expect("a frame");
    assert!(drag <= 1.0 + 1.0e-6 && drag > 0.9, "the frame is held with the root it needs: {drag}");
    // Nothing below the ramp; the resin is the layers it writes.
    for layer in 0..local.start {
        assert!(plan.wanted(0, layer) == 0.0, "layer {layer}");
    }
    assert_eq!(plan.first(0, struts[0].k_bottom), local.start);
    let per_layer = super::run_length(&grid, &struts[0], 0, 1, H);
    let mut by_hand = 0.0;
    for layer in local.start..plan.upto(0, 0) {
        let (wanted, now) = (
            plan.wanted(0, layer),
            super::drawn_at(&struts[0], &state.profiles[0], layer).unwrap_or(0.0),
        );
        if wanted > now {
            by_hand += core::f64::consts::PI * (wanted * wanted - now * now) * per_layer;
        }
    }
    assert!((by_hand - plan.added_mm3).abs() < 1.0e-12, "{by_hand} against {}", plan.added_mm3);
    // The same plan raised from the foot costs more, and enters the wall.
    let old_plan = super::ThickenPlan { local: None, ..plan.clone() };
    assert!(
        super::thickening_enters_the_gap(&context, &state, &struts, &old_plan),
        "raised from the foot the post grows into the wall beside it"
    );
    assert!(
        !super::thickening_enters_the_gap(&context, &state, &struts, &plan),
        "the bay-local plan writes nothing near the foot"
    );
    let mut old_cost = 0.0;
    for layer in 0..old_plan.upto(0, 0) {
        let (wanted, now) = (
            old_plan.wanted(0, layer),
            super::drawn_at(&struts[0], &state.profiles[0], layer).unwrap_or(0.0),
        );
        if wanted > now {
            old_cost += core::f64::consts::PI * (wanted * wanted - now * now) * per_layer;
        }
    }
    assert!(plan.added_mm3 < old_cost, "{} against {old_cost}", plan.added_mm3);
    // Committed: the ledger below the ramp is untouched, the bay is raised.
    let before: Vec<f64> = struts[0].thickened_mm.clone();
    super::commit_thicken(&mut state, &mut struts, &plan, &mut outcome);
    assert_eq!(outcome.thicken_bay_local, 1);
    for layer in 0..local.start {
        let (now, was) = (
            struts[0].thickened_mm.get(layer).copied().unwrap_or(0.0),
            before.get(layer).copied().unwrap_or(0.0),
        );
        assert!((now - was).abs() < 1.0e-15, "layer {layer}: {now} against {was}");
    }
    assert!(struts[0].thickened_mm[local.apex] >= plan.root - 1.0e-12);
    assert!(state.profiles[0][local.apex] >= plan.root - 1.0e-12);
}

/// **T3 (round 6): a post under a band widens in ONE cone from its foot to the
/// root at the run's base, never a hairline under a fat section** (the owner's
/// picture 28, the middle post: "a hairline lower half"). The same two footed
/// posts and bands as above with no wall beside the foot: the plan's ramp starts at
/// the foot's first layer, reaches the root at the run's base, rises by the same
/// amount every layer and never steeper than the permitted underside slope, so
/// the radius along the post is unimodal (widest at the base of the run, none
/// under the cone thinner than the cone). With a cured wall beside the foot the
/// cone would enter the final gap and the steepest ramp higher up is taken
/// (`a_bay_local_thickening_is_planned_checked_and_committed_as_one_thing`).
///
/// SABOTAGE: `bay_local_span(.., lower_cone = false)` (the round-5 ramp) starts the
/// ramp at the bay, not at the foot, and fails the first assertion.
#[test]
fn a_post_under_a_band_widens_in_one_cone_from_its_foot() {
    let grid = crate::grid::lay_out_at(0.0, 0.0, H, 200, 200);
    let history = crate::up_sweep::ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let law = feet_law();
    let context = context(&grid, &history, &keep_out, &lean, &law);
    let mut outcome = SlendernessOutcome::default();
    let bands = [(20, 40), (50, 70), (80, 100)];
    let (struts, state) = footed_pair(&grid, &context, &bands, 200, 0.10, 40, &mut outcome);
    let run = run_above_the_bands(&grid, &state, &struts, &law);
    let fits = |post: &crate::foot_neck::Reading| run.length_mm <= law.max_unbraced_mm(post.radius_mm);
    let base = super::plan_thicken(&context, &state, &struts, &run, &fits)
        .expect("a plan")
        .expect("not refused");
    // The cone's own plan (the ladder takes it where it is the cheaper of the
    // two plans; here its root is the connected frame's, larger than the local
    // run's, and the plan taken may be either).
    let plan = super::bay_local_plan(&context, &state, &struts, &run, &base, true)
        .expect("a lower-cone plan whose connected frame holds");
    let local = plan.local.expect("the plan is bay-local");
    assert_eq!(local.start, struts[0].k_bottom + 1, "the cone starts at the foot: {local:?}");
    assert_eq!(local.apex, run.k_lo, "and reaches the root at the run's base: {local:?}");
    // One straight cone: the same gain every layer, never under the foot's own radius,
    // never steeper than the permitted underside slope per layer.
    let slope_per_layer = law.cone_slope() * H;
    assert!(local.gain_per_layer_mm <= slope_per_layer + 1e-12, "{local:?} against {slope_per_layer}");
    let foot = super::drawn_at(&struts[0], &state.profiles[0], struts[0].k_bottom).expect("a foot radius");
    let mut previous = plan.wanted(0, local.start);
    assert!((previous - foot).abs() < 1e-9, "the cone starts at the foot's radius: {previous} against {foot}");
    for layer in local.start + 1..=local.apex {
        let here = plan.wanted(0, layer);
        assert!(
            (here - previous - local.gain_per_layer_mm).abs() < 1e-9 || layer == local.apex,
            "layer {layer}: {previous} -> {here}"
        );
        assert!(here >= previous - 1e-12, "unimodal: no layer is thinner than the one below it");
        previous = here;
    }
    assert!((plan.wanted(0, local.apex) - plan.root).abs() < 1e-9);
    // The steepest-ramp form it replaces starts higher, and is the same root.
    let steep = super::bay_local_span(&context, &state, &struts, &run, plan.root, false)
        .expect("the steepest ramp");
    assert!(steep.start > local.start, "anti-vacuity: the old ramp starts at {} not at the foot", steep.start);
}

/// **R2 for a model-footed post: raise it at the joint, never refuse the band for
/// the post's width** (flare lane). Two 0.05 mm posts 2 mm apart: the diagonal
/// that ties them needs 0.058 mm, so R2 refuses a band between two PLATE posts
/// (counted, no plan) and, between two MODEL-FOOTED posts, plans the band with a
/// raise of each post at the joints, a ramp under the band, the resin charged to
/// the band, and the raise committed with it.
///
/// SABOTAGE: return `None` from `joint_raise_plan` and the footed band is
/// refused like the plate band.
#[test]
fn a_band_on_two_thin_model_footed_posts_raises_them_at_the_joints() {
    let grid = crate::grid::lay_out_at(0.0, 0.0, H, 200, 200);
    let history = crate::up_sweep::ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let law = feet_law();
    let context = context(&grid, &history, &keep_out, &lean, &law);
    let run = UnbracedRun {
        strut_id: 0,
        k_lo: 0,
        k_top: 160,
        length_mm: 8.0,
        radius_mm: 0.05,
        limit_mm: 2.0,
        sway_deg: None,
        shaft_radius_mm: 0.05,
        neck_share: 0.0,
        foot: None,
        bay_bottom: None,
    };
    let try_band = |landing: Landing| {
        let struts = vec![
            post(&grid, 0, 100, 160, landing, &|_| 0.05),
            post(&grid, 1, 140, 160, landing, &|_| 0.05),
        ];
        let state = TieState::build(&context, &struts);
        let mut outcome = SlendernessOutcome::default();
        let plan = super::plan_cross(&context, &state, &struts, 0, 1, 90, &run, true, &mut outcome);
        (plan, outcome, struts, state)
    };
    let (plate, plate_outcome, ..) = try_band(Landing::Plate);
    assert!(plate.is_none(), "a plate band on 0.05 mm posts is refused by R2");
    assert!(plate_outcome.brace_band_refusals.thicker_than_post > 0);
    let (footed, _, mut struts, mut state) = try_band(Landing::Model { layer: 0 });
    let plan = footed.expect("the model-footed band is planned with its joint raises");
    assert_eq!(plan.raises.len(), 2, "each post is raised");
    for raise in &plan.raises {
        assert!(raise.joint_raise && raise.local.is_some() && raise.added_mm3 > 0.0);
        assert!((raise.root - plan.radius_mm).abs() < 1.0e-12);
    }
    assert!(plan.volume_mm3 > 0.0);
    let mut outcome = SlendernessOutcome::default();
    super::commit_cross(&context, &mut state, &mut struts, plan.clone(), &mut outcome);
    assert_eq!(outcome.posts_raised_at_a_joint, 2);
    for (post_index, strut) in struts.iter().enumerate().take(2) {
        let at = |layer: usize| super::drawn_at(strut, &state.profiles[post_index], layer).unwrap_or(0.0);
        assert!(at(plan.k) >= plan.radius_mm - 1.0e-12 && at(plan.k_lo) >= plan.radius_mm - 1.0e-12, "the joints are as wide as the diagonal");
        assert!((at(0) - 0.05).abs() < 1.0e-12, "nothing is raised at the foot");
    }
}

/// **Every production panel carries the foot** (flare lane, challenger 5): two
/// 0.4 mm model-footed posts 6 mm tall on 0.05 mm contacts are ties-free over
/// their limit once the neck is read (as plate posts they would hold), and the
/// ladder braces them through the counted bands: the ordinary selection
/// (`frame_ledger` on) and the deferred one (`frame_ledger` off) build the
/// bands, the footed pair needs a band the plate pair does not, and the band
/// the frame counts stands LOW (the neck needs it).
///
/// SABOTAGE: build the panels without the feet (`pair_panel` without
/// `post_foot`/`partner_foot`) and no band is counted for the footed pair.
#[test]
fn the_ordinary_and_the_deferred_band_counts_read_the_feet() {
    let grid = crate::grid::lay_out_at(0.0, 0.0, H, 200, 200);
    let history = crate::up_sweep::ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let law = feet_law();
    for ledger in [true, false] {
        let context = SlendernessContext {
            frame_ledger: ledger,
            thicken: false,
            mast: false,
            model_tie: false,
            ..context(&grid, &history, &keep_out, &lean, &law)
        };
        let ties = |landing: Landing| {
            let mut struts = vec![
                post(&grid, 0, 100, 120, landing, &|_| 0.4),
                post(&grid, 1, 140, 120, landing, &|_| 0.4),
            ];
            let mut outcome = SlendernessOutcome::default();
            super::apply_ties(&context, &mut struts, &mut outcome);
            (outcome, struts)
        };
        let (plate, _) = ties(Landing::Plate);
        let (footed, struts) = ties(Landing::Model { layer: 0 });
        assert_eq!(plate.braces_added, 0, "ledger {ledger}: plate posts hold unbraced: {plate:?}");
        assert!(footed.foot_runs_read_through_neck > 0, "ledger {ledger}: path hit");
        assert!(footed.braces_added > 0, "ledger {ledger}: the footed pair is braced: {footed:?}");
        // The bands cut the foot run: re-read from the finished field, every
        // run that still starts on a foot has the neck inside its pixel.
        let state = TieState::build(&context, &struts);
        let feet: Vec<UnbracedRun> = (0..2)
            .flat_map(|index| {
                member_runs(&grid, &struts[index], &state.ties[index], &state.profiles[index], H, &law)
            })
            .filter(|run| run.foot.is_some())
            .collect();
        let in_plane: Vec<&UnbracedRun> =
            feet.iter().filter(|run| run.sway_deg == Some(90.0)).collect();
        assert!(!in_plane.is_empty(), "ledger {ledger}: anti-vacuity: foot runs remain");
        assert!(
            in_plane.iter().all(|run| run.neck_share < 1.0 && !run.over_limit()),
            "ledger {ledger}: the band holds the foot run in its plane: {in_plane:?}"
        );
        // Across the band's plane nothing ties the post, and the neck alone
        // takes more than the pixel: the honest reading is over its limit
        // (`foot_neck_over_limit`), not a hidden carrot.
        assert!(
            feet.iter().any(|run| run.sway_deg == Some(0.0) && run.over_limit() && run.neck_share >= 1.0),
            "ledger {ledger}: {feet:?}"
        );
    }
}
