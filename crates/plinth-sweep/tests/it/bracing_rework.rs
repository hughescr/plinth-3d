//! **Bracing reworked again (Craig's rulings of 2026-10-01).**
//!
//! Field feedback on `for-craig-v8` (`stilt-and-brace-pair-diagnosis.md` item
//! 2, `brace-pair-resin.md`): the brace pair came out with no braces and four
//! thickened columns. Two `|X|` were built, each holding one plane (the
//! model's own posts stand between every other pair), then the columns were
//! thickened for the direction across those planes, and the prune took both
//! crosses out because the thickened columns held every direction without
//! them. The rulings:
//!
//! 1. `S` 50 -> 30 (interim, pending a slenderness coupon);
//! 2. KEEP one-direction crosses: a cross that ties two columns counts in its
//!    own plane, and thickening or a mast covers the other direction; it is
//!    never pruned because thickening also holds;
//! 3. a landing branch ties its host only from the layer where the branch is
//!    held at its top (its seat has printed);
//! 4. every brace-band refusal is counted by its reason.
//!
//! Every assertion reads the finished strut set or the lane's own published
//! counts; nothing re-implements the lane.

use std::sync::OnceLock;

use crate::harness;

use harness::sweep_with;
use plinth_mesh::make_brace_pair;
use plinth_sweep::{Landing, StrutKind, SweepPlan, SweepPolicy};

fn policy() -> SweepPolicy {
    SweepPolicy::from_registry().expect("the registry supplies every number the sweep reads")
}

/// The brace pair under the PRODUCTION policy (the one `for-craig-v8` ran),
/// with `S` overridden to the ratio in force until 2026-10-01: the regime in
/// which the thicken rung is admissible on these columns, so the prune had a
/// thickening to judge the crosses against. On develop `247b8d5` this field
/// is v8's: columns 2, 5, 9 and 11 thickened, 2 crosses built and pruned.
/// (The fixture-seating policy `bracing.rs` uses keeps 14 crosses on develop
/// and so witnesses nothing here.)
fn brace_pair_at_s_50() -> &'static SweepPlan {
    static PLAN: OnceLock<SweepPlan> = OnceLock::new();
    PLAN.get_or_init(|| {
        let mesh = make_brace_pair().expect("the brace pair fixture");
        let mut policy = policy();
        policy.slenderness_ratio = 50.0;
        sweep_with(&mesh, 0.0, &policy)
    })
}

/// The brace pair exactly as production places it now (the registered `S`:
/// 40 since Craig's ruling of 2026-10-02; 30 from 2026-10-01).
fn brace_pair() -> &'static SweepPlan {
    static PLAN: OnceLock<SweepPlan> = OnceLock::new();
    PLAN.get_or_init(|| {
        let mesh = make_brace_pair().expect("the brace pair fixture");
        sweep_with(&mesh, 0.0, &policy())
    })
}

/// The `|X|` crosses of a finished field, as (seat post, landing post, band
/// bottom, band top), one entry per diagonal.
fn diagonals(plan: &SweepPlan) -> Vec<(u32, u32, usize, usize)> {
    plan.struts
        .iter()
        .filter(|strut| strut.kind == StrutKind::Brace)
        .filter_map(|strut| match (strut.seat_on_strut, strut.landing) {
            (Some(seat), Landing::Strut { id, .. }) => {
                Some((seat, id, strut.k_bottom, strut.k_top))
            }
            _ => None,
        })
        .collect()
}

/// RULING 2 (with 3): on the brace pair at `S` = 50 the columns ARE
/// thickened (the rung is admissible there), and the one-plane crosses built
/// before the thickening still stand. Red on develop `247b8d5`: 2 crosses
/// built, 2 pruned, 0 kept, 4 columns thickened.
///
/// On this branch the two crosses are kept by the prune's FIRST test: with
/// the tie-timing rule column 5 keeps a run over the limit (one warning), so
/// its crosses are needed as measured. The own-plane rule's direct witness is
/// the unit test `a_one_plane_cross_is_kept_when_thickening_holds_the_other_direction`
/// in `slenderness.rs`, and `crosses_kept_in_their_own_plane` in each model
/// run's log.
#[test]
fn the_brace_pair_keeps_its_one_plane_crosses_where_thickening_also_holds() {
    let plan = brace_pair_at_s_50();
    let lane = &plan.report.slenderness_lane;
    let crosses = diagonals(plan);
    let thickened_posts: Vec<u32> = plan
        .struts
        .iter()
        .filter(|strut| strut.thickened_mm.iter().any(|&radius| radius > 0.0))
        .map(|strut| strut.id)
        .collect();
    let on_thickened = crosses
        .iter()
        .filter(|(seat, base, ..)| thickened_posts.contains(seat) || thickened_posts.contains(base))
        .count();
    println!(
        "brace pair at S = 50: {} diagonals, {} on a thickened post; {} members thickened \
         ({thickened_posts:?}), {} crosses pruned, {} masts, {} warnings",
        crosses.len(),
        on_thickened,
        lane.members_thickened,
        lane.crosses_pruned,
        lane.masts_dropped,
        lane.warnings.len()
    );
    assert!(
        lane.members_thickened > 0,
        "anti-vacuity: no column was thickened, so no cross was judged against a thickening"
    );
    assert!(
        on_thickened > 0,
        "no cross stands on a thickened column: the one-plane crosses were pruned because the \
         thickening also holds ({} pruned)",
        lane.crosses_pruned
    );
}

/// RULING 4: every brace band the lane refuses is counted under exactly one
/// reason, on the brace pair as production places it and at `S` = 50. Until
/// 2026-10-01 only the model gap and the member test were itemised (v8's
/// brace pair: 9 bands planned, 7 refused, 1 reason logged).
#[test]
fn every_brace_band_refusal_names_one_reason() {
    let mut refused = 0;
    for (name, plan) in [
        ("brace pair", brace_pair()),
        ("brace pair at S = 50", brace_pair_at_s_50()),
    ] {
        let lane = &plan.report.slenderness_lane;
        println!(
            "{name}: {} bands planned, {} refused: {:?}; {} overlapping",
            lane.cross_bands_planned,
            lane.brace_bands_refused,
            lane.brace_band_refusals,
            lane.crosses_refused_overlapping
        );
        assert_eq!(
            lane.brace_band_refusals.total(),
            lane.brace_bands_refused,
            "{name}: refusals without a reason: {:?}",
            lane.brace_band_refusals
        );
        refused += lane.brace_bands_refused;
    }
    assert!(refused > 0, "anti-vacuity: no band was refused, so no reason was graded");
}

/// RULING 3, on the production field: the brace pair's columns carry
/// branches that land on them below the caps and seat in the caps, so the
/// timing rule must fire (a path hit), and every run the lane publishes on a
/// column that carries such a branch reaches at least to that branch's seat
/// or starts at or above its landing: no run ENDS at a landing whose branch
/// is not yet seated.
#[test]
fn a_column_is_measured_up_to_its_branches_seats_on_the_brace_pair() {
    let plan = brace_pair();
    let lane = &plan.report.slenderness_lane;
    let runs = plinth_sweep::unbraced_runs(
        &plan.grid,
        &plan.struts,
        plan.layer_height_mm,
        &plinth_sweep::SlendernessLaw::new(
            policy().slenderness_ratio,
            policy().tip_radius_min_mm,
            policy().tip_radius_max_mm,
            policy().overhang_centre_deg - 0.5 * policy().overhang_transition_deg,
        ),
    );
    let mut late = 0;
    let mut wrong = Vec::new();
    for branch in &plan.struts {
        let Landing::Strut { id: host, layer } = branch.landing else {
            continue;
        };
        if branch.kind == StrutKind::Brace || branch.k_top <= layer {
            continue;
        }
        late += 1;
        for run in runs.iter().filter(|run| run.strut_id == host) {
            if run.k_top == layer && branch.k_top.min(plan.struts[host as usize].k_top) != layer {
                wrong.push((host, branch.id, layer, branch.k_top, run.k_lo, run.k_top));
            }
        }
    }
    println!(
        "brace pair: {late} branches landing below their seats; lane counted {} landing ties \
         held late; {} runs ending at an unseated branch's landing: {wrong:?}",
        lane.landing_ties_held_late,
        wrong.len()
    );
    assert!(late > 0, "anti-vacuity: no branch lands on a column");
    assert!(
        lane.landing_ties_held_late > 0,
        "path hit: the lane held no landing tie late"
    );
    assert!(
        wrong.is_empty(),
        "runs end at the landing of a branch whose seat has not printed: {wrong:?}"
    );
}

/// The brace pair as production places it now (the registered `S`): it
/// carries crosses (v8 carried none). Prints what the lane did, for the lane
/// report. Renamed 2026-10-02 from `..._at_s_30_...` when `S` went to 40: the
/// field it grades is the production one, whatever `S` is registered.
#[test]
fn the_brace_pair_as_production_places_it_keeps_every_cross_it_builds() {
    let plan = brace_pair();
    let lane = &plan.report.slenderness_lane;
    let crosses = diagonals(plan).len() / 2;
    println!(
        "brace pair at S = {}: {crosses} crosses ({} first plane, {} further), {} pruned, {} kept \
         in their own plane; {} thickened ({} refused past the maximum, {} by a disc); {} masts \
         ({} cells refused, {} by the panel); {} warnings; longest run {:.3} mm against {:.3} mm",
        lane.ratio,
        lane.crosses_first_plane,
        lane.crosses_further_plane,
        lane.crosses_pruned,
        lane.crosses_kept_in_their_own_plane,
        lane.members_thickened,
        lane.thicken_refused_radius,
        lane.thicken_refused_disc,
        lane.masts_dropped,
        lane.mast_cells_refused,
        lane.mast_cells_refused_by_panel,
        lane.warnings.len(),
        lane.longest_run_mm,
        lane.longest_run_limit_mm
    );
    assert!(crosses > 0, "the brace pair has no cross at S = {}", lane.ratio);
}

/// The brace pair at `S` = 30, the ratio `for-craig-v10` ran (Craig's ruling
/// of 2026-10-01, superseded on 2026-10-02 by 40).
fn brace_pair_at_s_30() -> &'static SweepPlan {
    static PLAN: OnceLock<SweepPlan> = OnceLock::new();
    PLAN.get_or_init(|| {
        let mesh = make_brace_pair().expect("the brace pair fixture");
        let mut policy = policy();
        policy.slenderness_ratio = 30.0;
        sweep_with(&mesh, 0.0, &policy)
    })
}

/// QUESTION (lane slenderness-40, 2026-10-02): `for-craig-v10`'s brace pair
/// prints four `unbraced_run_over_limit` lines, each a FULL 35 mm run (layers
/// 0..700) on arms 2, 5, 9 and 11, although three of those columns carry
/// crosses. Do the crosses count against those runs?
///
/// Measured on the finished field with the lane's own public measurement
/// ([`plinth_sweep::unbraced_runs`]), nothing re-derived: every warning is a
/// run the finished field still has, and on a column that carries a cross the
/// warned run is measured in ONE sway direction (`sway_deg` is `Some`) while
/// the same column has at least one other direction in which the crosses DO
/// cut it short. So the crosses count, in the plane they hold; the full-height
/// run is the direction across that plane, which no cross on the column holds.
#[test]
fn a_warned_full_run_on_a_crossed_column_is_the_direction_its_crosses_do_not_hold() {
    let plan = brace_pair_at_s_30();
    let lane = &plan.report.slenderness_lane;
    let mut policy = policy();
    policy.slenderness_ratio = 30.0;
    let law = plinth_sweep::SlendernessLaw::new(
        policy.slenderness_ratio,
        policy.tip_radius_min_mm,
        policy.tip_radius_max_mm,
        policy.overhang_centre_deg - 0.5 * policy.overhang_transition_deg,
    );
    let runs = plinth_sweep::unbraced_runs(&plan.grid, &plan.struts, plan.layer_height_mm, &law);
    let crosses = diagonals(plan);
    let pitch = plan.grid.pitch_mm();
    let centre = |strut: &plinth_sweep::SweepStrut| strut.center_mm;
    let mut crossed_warned = 0;
    let mut problems = Vec::new();
    for warning in &lane.warnings {
        let post = &plan.struts[warning.strut_id as usize];
        let on_post: Vec<&(u32, u32, usize, usize)> = crosses
            .iter()
            .filter(|(seat, base, ..)| *seat == post.id || *base == post.id)
            .collect();
        let post_runs: Vec<&plinth_sweep::UnbracedRun> =
            runs.iter().filter(|run| run.strut_id == post.id).collect();
        println!(
            "warning: arm {} layers {}..{} runs {:.3} mm against {:.3} mm; post at ({:.3}, {:.3}) \
             k {}..{}, pitch {pitch}",
            warning.strut_id,
            warning.k_lo,
            warning.k_top,
            warning.length_mm,
            warning.limit_mm,
            centre(post)[0],
            centre(post)[1],
            post.k_bottom,
            post.k_top
        );
        for (seat, base, lo, hi) in &on_post {
            let other = if *seat == post.id { *base } else { *seat };
            let partner = &plan.struts[other as usize];
            println!(
                "    diagonal seat {seat} base {base} band {lo}..{hi}; partner {other} at \
                 ({:.3}, {:.3}), offset ({:+.3}, {:+.3}) mm",
                centre(partner)[0],
                centre(partner)[1],
                centre(partner)[0] - centre(post)[0],
                centre(partner)[1] - centre(post)[1]
            );
        }
        for run in &post_runs {
            println!(
                "    run {}..{} {:.3} mm limit {:.3} mm sway {:?} over {}",
                run.k_lo,
                run.k_top,
                run.length_mm,
                run.limit_mm,
                run.sway_deg,
                run.over_limit()
            );
        }
        let warned = post_runs
            .iter()
            .find(|run| run.k_lo == warning.k_lo && run.k_top == warning.k_top);
        let Some(warned) = warned else {
            problems.push(format!(
                "arm {}: warning {}..{} is no run of the finished field",
                post.id, warning.k_lo, warning.k_top
            ));
            continue;
        };
        if !warned.over_limit() {
            problems.push(format!("arm {}: the warned run is inside its limit", post.id));
        }
        if on_post.is_empty() {
            continue;
        }
        crossed_warned += 1;
        if warned.sway_deg.is_none() {
            problems.push(format!(
                "arm {}: carries {} diagonals but its warned run is measured in no direction, \
                 so no cross counted",
                post.id,
                on_post.len()
            ));
        }
        let cut = post_runs
            .iter()
            .any(|run| run.sway_deg != warned.sway_deg && run.k_top - run.k_lo < post.k_top - post.k_bottom);
        if !cut {
            problems.push(format!(
                "arm {}: carries {} diagonals and no direction is cut short by them",
                post.id,
                on_post.len()
            ));
        }
    }
    println!(
        "brace pair at S = 30: {} warnings, {crossed_warned} on a crossed column; {} diagonals",
        lane.warnings.len(),
        crosses.len()
    );
    assert!(
        crossed_warned > 0,
        "anti-vacuity: no warning stands on a crossed column, so the question is not reached"
    );
    assert!(problems.is_empty(), "{problems:#?}");
}
