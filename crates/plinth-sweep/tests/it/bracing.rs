//! **Bracing, reworked (2026-09-29): every tie is needed, every tie holds in
//! every direction, and a post that only needs to be stiffer is made stiffer.**
//!
//! The field report of 2026-09-28 (`braces-masts.md`) found five defects in the
//! slenderness lane's ladder, each measured on the pinned fixtures:
//!
//! * a double `|X|` on one pair of posts, "slightly shifted in Z" — the
//!   over-limit run list was measured once and never again, so a partner that
//!   an earlier cross had already tied still built its own;
//! * a mast beside a post that was already tied (the brace pair's "close pair
//!   with a tiny X");
//! * a mast where a second partner in reach would have done (the stilt plate);
//! * posts left over the limit and only warned about, when a thicker root
//!   would have held them;
//! * a log line that called descent merges "forced merges put back".
//!
//! The owner's rulings (2026-09-29): the order is merge, `|X|`, THICKEN (raise
//! the root, keep the tip, 1.0 mm maximum), mast, warn; bracing has to hold in
//! EVERY direction, thickening preferred; `S` stays 50. (Craig's rulings of
//! 2026-10-01: `S` = 30, one-direction crosses are kept, a landing ties its
//! host from its seat; see `bracing_rework.rs` and the re-derivations below.)
//!
//! Every assertion reads the finished strut set, or the production
//! measurement ([`plinth_sweep::unbraced_runs`]) applied to a strut set the
//! test has edited — never a copy of the lane's own logic.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::harness;

use harness::sweep_with;
use plinth_core::m0_constants;
use plinth_mesh::{make_brace_pair, make_lonely_peak, make_tall_stilt_plate};
use plinth_sweep::{
    unbraced_runs, Landing, SlendernessLaw, StrutKind, SweepPlan, SweepPolicy, SweepStrut,
};

fn policy() -> SweepPolicy {
    SweepPolicy::from_registry().expect("the registry supplies every number the sweep reads")
}

/// The law as the production policy builds it, read from the registry here.
fn law() -> SlendernessLaw {
    let constants = m0_constants().expect("the registry constructs");
    law_at(
        constants
            .placeholders()
            .slenderness_limit_ratio()
            .diagnostic()
            .get(),
    )
}

fn stilt() -> &'static SweepPlan {
    static PLAN: OnceLock<SweepPlan> = OnceLock::new();
    PLAN.get_or_init(|| {
        let mesh = make_tall_stilt_plate().expect("the tall stilt plate fixture");
        sweep_with(&mesh, 0.0, &policy())
    })
}

/// The stilt plate at the ratio in force until 2026-10-01
/// (`harness::PATH_FIXTURE_SLENDERNESS_RATIO`), where every one of its posts
/// has an admissible remedy: the ladder's completeness is graded there.
fn stilt_at_s_50() -> &'static SweepPlan {
    static PLAN: OnceLock<SweepPlan> = OnceLock::new();
    PLAN.get_or_init(|| {
        let mesh = make_tall_stilt_plate().expect("the tall stilt plate fixture");
        sweep_with(
            &mesh,
            0.0,
            &harness::at_path_fixture_slenderness(policy()),
        )
    })
}

/// [`law`] at a given ratio, every other input read from the registry.
fn law_at(ratio: f64) -> SlendernessLaw {
    let constants = m0_constants().expect("the registry constructs");
    let placeholders = constants.placeholders();
    SlendernessLaw::new(
        ratio,
        placeholders.tip_radius_default_mm().diagnostic().get(),
        placeholders.tip_radius_max_legacy_mm().diagnostic().get(),
        placeholders.overhang_angle_deg().diagnostic().get()
            - 0.5
                * placeholders
                    .overhang_weight_transition_deg()
                    .diagnostic()
                    .get(),
    )
}

fn brace_pair() -> &'static SweepPlan {
    static PLAN: OnceLock<SweepPlan> = OnceLock::new();
    PLAN.get_or_init(|| {
        let mesh = make_brace_pair().expect("the brace pair fixture");
        // Graded at the seat placement this fixture was built for
        // (`harness::at_path_fixture_seating`): under the straight-line reach
        // and the island seating the pair's posts take one forced merge and
        // need no `|X|` at all (0 braces, measured 2026-09-30), so the tie
        // tests would grade nothing. No bracing rule reads the placement.
        sweep_with(&mesh, 0.0, &harness::at_path_fixture_seating(policy()))
    })
}

/// At the S the peak was built at (`harness::PATH_FIXTURE_SLENDERNESS_RATIO`,
/// 50). At the ruled 30 its 35 mm arms need a root past the 1.0 mm maximum
/// and no mast cell holds them, so both runs are warnings (measured
/// 2026-10-01): the claim graded here, that a thicker root beats a mast
/// WHERE it is admissible, is then never reached.
fn lonely_peak() -> &'static SweepPlan {
    static PLAN: OnceLock<SweepPlan> = OnceLock::new();
    PLAN.get_or_init(|| {
        let mesh = make_lonely_peak().expect("the lonely peak fixture");
        sweep_with(
            &mesh,
            0.0,
            &harness::at_path_fixture_slenderness(policy()),
        )
    })
}

/// One `|X|`: its two diagonals, the unordered pair of posts it ties, and its
/// band. A diagonal seats on one post at the band's top and lands on the other
/// at its bottom; its twin does the reverse over the same band.
#[derive(Clone, Debug)]
struct Cross {
    pair: (u32, u32),
    k_lo: usize,
    k_top: usize,
    diagonals: Vec<usize>,
}

fn crosses(plan: &SweepPlan) -> Vec<Cross> {
    let mut grouped: BTreeMap<((u32, u32), usize, usize), Vec<usize>> = BTreeMap::new();
    for (index, strut) in plan.struts.iter().enumerate() {
        if strut.kind != StrutKind::Brace {
            continue;
        }
        let (Some(seat), Landing::Strut { id: base, .. }) = (strut.seat_on_strut, strut.landing)
        else {
            continue;
        };
        let pair = (seat.min(base), seat.max(base));
        grouped
            .entry((pair, strut.k_bottom, strut.k_top))
            .or_default()
            .push(index);
    }
    grouped
        .into_iter()
        .map(|((pair, k_lo, k_top), diagonals)| Cross {
            pair,
            k_lo,
            k_top,
            diagonals,
        })
        .collect()
}

/// Take one diagonal out of a strut set without shifting any index: no bottom
/// layer (the run measurement skips it), and joined to nothing (so no post is
/// drawn thicker for it).
fn detach(diagonal: &mut SweepStrut) {
    diagonal.k_bottom = usize::MAX;
    diagonal.landing = Landing::Plate;
    diagonal.seat_on_strut = None;
}

fn count_kind(plan: &SweepPlan, kind: StrutKind) -> usize {
    plan.struts
        .iter()
        .filter(|strut| strut.kind == kind)
        .count()
}

/// Test 1 of the field report: **no two `|X|`s on one pair overlap in
/// height.** Two crosses on the same two posts whose bands overlap are one tie
/// built twice (red on d103c48: stilt 75/121 at k 261..328 and 257..324, pair
/// 9/10 at 289..336 and 269..316).
fn assert_no_overlapping_crosses(name: &str, plan: &SweepPlan) {
    let all = crosses(plan);
    assert!(
        !all.is_empty() || name == "tall stilt plate",
        "{name}: no cross at all, so this grades nothing: {:?}",
        plan.report.slenderness_lane
    );
    let mut overlaps = Vec::new();
    for (i, one) in all.iter().enumerate() {
        for two in &all[i + 1..] {
            // Two closed bands overlap when each starts before the other ends.
            let overlap = one.k_lo.max(two.k_lo) <= one.k_top.min(two.k_top);
            if one.pair == two.pair && overlap {
                overlaps.push((one.pair, (one.k_lo, one.k_top), (two.k_lo, two.k_top)));
            }
        }
    }
    println!(
        "{name}: {} crosses, {} overlapping pairs of crosses on one pair of posts",
        all.len(),
        overlaps.len()
    );
    assert!(
        overlaps.is_empty(),
        "{name}: {} crosses on one pair of posts overlap in height (one tie built twice): {overlaps:?}",
        overlaps.len()
    );
}

#[test]
fn no_two_crosses_on_one_pair_overlap_in_height_on_the_stilt_plate() {
    assert_no_overlapping_crosses("tall stilt plate", stilt());
}

#[test]
fn no_two_crosses_on_one_pair_overlap_in_height_on_the_brace_pair() {
    assert_no_overlapping_crosses("brace pair", brace_pair());
}

/// Test 2 of the field report: **every `|X|` is necessary.** With any one
/// cross taken out of the finished field, the production measurement finds a
/// run over the limit on one of the two posts it tied, and that run passes
/// through the band the cross stood in. A cross whose removal leaves both
/// posts inside their limits was never needed (red on d103c48: stilt X
/// 141/142, pair X 31/32 and the mast's X 27/28).
///
/// **RE-DERIVED 2026-10-01 (Craig's ruling: keep one-direction crosses).** A
/// cross holds its own plane; a THICKENING sized for the direction across it
/// also holds the cross's plane, and the ruling is that this never makes the
/// cross unneeded. On the finished field the ledger already carries the
/// thickening, so such a cross reads as not needed here; it is admitted only
/// where one of its two posts carries a thickening. Every other cross must
/// still be needed (stilt plate: X 75/80 over 544..605 is the one admitted,
/// measured 2026-10-01).
fn assert_every_cross_is_needed(name: &str, plan: &SweepPlan) {
    let law = law();
    let all = crosses(plan);
    let thickened = |post: u32| {
        plan.struts[post as usize]
            .thickened_mm
            .iter()
            .any(|&radius| radius > 0.0)
    };
    let mut on_a_thickened_post = Vec::new();
    let mut redundant = Vec::new();
    for cross in &all {
        let mut struts: Vec<SweepStrut> = plan.struts.clone();
        for &diagonal in &cross.diagonals {
            detach(&mut struts[diagonal]);
        }
        let runs = unbraced_runs(&plan.grid, &struts, plan.layer_height_mm, &law);
        let needed = runs.iter().any(|run| {
            run.over_limit()
                && (run.strut_id == cross.pair.0 || run.strut_id == cross.pair.1)
                && run.k_lo < cross.k_top
                && cross.k_lo < run.k_top
        });
        if !needed {
            if thickened(cross.pair.0) || thickened(cross.pair.1) {
                on_a_thickened_post.push((cross.pair, cross.k_lo, cross.k_top));
            } else {
                redundant.push((cross.pair, cross.k_lo, cross.k_top));
            }
        }
    }
    println!(
        "{name}: {} crosses graded, {} not needed: {redundant:?}; {} kept on a thickened \
         post: {on_a_thickened_post:?}",
        all.len(),
        redundant.len(),
        on_a_thickened_post.len()
    );
    assert!(
        redundant.is_empty(),
        "{name}: {} crosses tie posts that are inside their limits without them: {redundant:?}",
        redundant.len()
    );
}

#[test]
fn every_cross_is_needed_on_the_stilt_plate() {
    assert_every_cross_is_needed("tall stilt plate", stilt());
}

#[test]
fn every_cross_is_needed_on_the_brace_pair() {
    assert_every_cross_is_needed("brace pair", brace_pair());
}

/// Tests 3 and 5 of the field report, RE-DERIVED on purpose (2026-09-29).
///
/// As written they were "the brace pair places no mast" (its mast 26 on
/// d103c48 stood beside arm 8, which arms 4 and 5 had already tied) and "the
/// stilt plate places no mast" (arm 81's mast, where arm 65 was in reach).
/// Both were red on d103c48 with one mast each, and both went green under the
/// first form of this lane, which credited a cross in every direction but the
/// one straight across it.
///
/// The owner's ruling that bracing hold in EVERY direction, applied honestly
/// (a cross holds the sway within 45° of its plane), leaves each fixture ONE
/// post whose last direction no partner in reach can hold and whose thicker
/// root would lay its flare onto a neighbour's: stilt arm 81 (root 0.621 ->
/// 0.656 mm) and brace-pair arm 8 (0.388 -> 0.767 mm). The ladder then drops
/// a mast, as the ruling orders. So the assertion is the defect the field
/// report actually found — a REDUNDANT mast, or one dropped where a thicker
/// root would have done — and not a count:
///
/// * every mast is needed: with it and its crosses taken out, the arm it
///   ties has a run over the limit through the mast's band;
/// * no more masts than thickenings refused (a mast follows a refused
///   thickening, never an admissible one).
fn assert_every_mast_is_needed(name: &str, plan: &SweepPlan) {
    let law = law();
    let lane = &plan.report.slenderness_lane;
    let masts: Vec<usize> = plan
        .struts
        .iter()
        .enumerate()
        .filter(|(_, strut)| strut.kind == StrutKind::Mast)
        .map(|(index, _)| index)
        .collect();
    let mut redundant = Vec::new();
    for &mast in &masts {
        let mast_id = plan.struts[mast].id;
        let mut struts: Vec<SweepStrut> = plan.struts.clone();
        let mut arms_and_bands: Vec<(u32, usize, usize)> = Vec::new();
        for cross in crosses(plan) {
            let (one, two) = cross.pair;
            if one != mast_id && two != mast_id {
                continue;
            }
            let arm = if one == mast_id { two } else { one };
            arms_and_bands.push((arm, cross.k_lo, cross.k_top));
            for &diagonal in &cross.diagonals {
                detach(&mut struts[diagonal]);
            }
        }
        detach(&mut struts[mast]);
        let runs = unbraced_runs(&plan.grid, &struts, plan.layer_height_mm, &law);
        let needed = arms_and_bands.iter().any(|&(arm, lo, top)| {
            runs.iter().any(|run| {
                run.strut_id == arm && run.over_limit() && run.k_lo < top && lo < run.k_top
            })
        });
        if !needed {
            redundant.push((mast_id, arms_and_bands));
        }
    }
    let refused = lane.thicken_refused_disc + lane.thicken_refused_radius;
    println!(
        "{name}: {} masts, {} redundant {redundant:?}; thickenings refused {refused} \
         ({} disc, {} radius)",
        masts.len(),
        redundant.len(),
        lane.thicken_refused_disc,
        lane.thicken_refused_radius
    );
    assert!(
        redundant.is_empty(),
        "{name}: {} masts tie arms that are inside their limits without them: {redundant:?}",
        redundant.len()
    );
    assert!(
        masts.len() <= refused,
        "{name}: {} masts but only {refused} thickenings refused: a mast was dropped where a \
         thicker root was admissible",
        masts.len()
    );
}

#[test]
fn the_brace_pair_places_no_redundant_mast() {
    assert_every_mast_is_needed("brace pair", brace_pair());
}

#[test]
fn the_stilt_plate_places_no_redundant_mast() {
    assert_every_mast_is_needed("tall stilt plate", stilt());
}

/// Test 4 of the field report: **the stilt plate leaves no warning.** Every
/// post on it is 35 mm tall, well inside the 50 mm a 1.0 mm post may stand, so
/// every over-limit run has an admissible remedy; a warning there is a remedy
/// the ladder did not try (red on d103c48: the 7 plate posts the un-merge pass
/// put back after the lane had finished).
///
/// **RE-PINNED 2026-10-01 to `S` = 50** (`harness::PATH_FIXTURE_SLENDERNESS_RATIO`),
/// where the premise above holds. At the ruled `S` = 30 a 35 mm post needs
/// `r_eq` 1.17 mm, past the 1.0 mm maximum, so three posts (11, 70, 76) have
/// no admissible remedy in the direction their crosses do not hold and are
/// WARNED (measured 2026-10-01); production is graded on that contract by
/// `the_stilt_plate_warns_every_run_it_leaves_over_the_limit`.
#[test]
fn the_stilt_plate_leaves_no_unbraced_run() {
    let plan = stilt_at_s_50();
    let lane = &plan.report.slenderness_lane;
    let law = law_at(harness::PATH_FIXTURE_SLENDERNESS_RATIO);
    let runs = unbraced_runs(&plan.grid, &plan.struts, plan.layer_height_mm, &law);
    let over: Vec<_> = runs.iter().filter(|run| run.over_limit()).collect();
    println!(
        "tall stilt plate at S = 50: {} runs measured, {} over the limit, {} warnings",
        runs.len(),
        over.len(),
        lane.warnings.len()
    );
    assert!(
        over.is_empty() && lane.warnings.is_empty(),
        "the stilt plate leaves {} runs over the limit ({} warnings): {:?}",
        over.len(),
        lane.warnings.len(),
        over.iter().take(10).collect::<Vec<_>>()
    );
}

/// The stilt plate as production places it (`S` = 40 since Craig's ruling of
/// 2026-10-02; one warned post, 11, measured then): every run over the
/// limit is a published warning (emit and warn, nothing hidden), and each
/// warned member is one the thicken rung refused past the maximum radius.
#[test]
fn the_stilt_plate_warns_every_run_it_leaves_over_the_limit() {
    let plan = stilt();
    let lane = &plan.report.slenderness_lane;
    let runs = unbraced_runs(&plan.grid, &plan.struts, plan.layer_height_mm, &law());
    let mut over: Vec<(u32, usize, usize)> = runs
        .iter()
        .filter(|run| run.over_limit())
        .map(|run| (run.strut_id, run.k_lo, run.k_top))
        .collect();
    over.sort_unstable();
    over.dedup();
    let mut warned: Vec<(u32, usize, usize)> = lane
        .warnings
        .iter()
        .map(|warning| (warning.strut_id, warning.k_lo, warning.k_top))
        .collect();
    warned.sort_unstable();
    warned.dedup();
    println!(
        "tall stilt plate: {} runs over the limit {over:?}; warnings {warned:?}; thicken refused \
         past the maximum {}",
        over.len(),
        lane.thicken_refused_radius
    );
    assert_eq!(over, warned, "a run over the limit that is not warned, or the reverse");
    let members = {
        let mut ids: Vec<u32> = warned.iter().map(|(id, ..)| *id).collect();
        ids.dedup();
        ids.len()
    };
    assert!(
        lane.thicken_refused_radius >= members,
        "{members} warned members but only {} thickenings refused past the maximum",
        lane.thicken_refused_radius
    );
}

/// **A tie holds in every direction** (owner ruling 2026-09-29). An `|X|` is a
/// flat truss: it holds its two posts in its own plane and nothing across it.
/// So wherever a post NEEDS its crosses — with every cross on it taken out of
/// the measurement it has a run over the limit — the crosses over that run
/// must hold every direction of sway. A cross holds the sway within 45° of its
/// own plane; crosses meeting the post at one band-top layer are credited
/// together (`Σ cos² ≥ ½`, ruling of 2026-09-29, fix/member-clearance). One
/// plane is half a tie. (On d103c48 every tie was one cross in one plane.)
///
/// The measurement here is taken with ALL of the post's crosses removed, so it
/// reads no plane at all: the planes are read off the finished geometry by the
/// test itself, from the two posts' axis cells at the top of each band.
fn assert_ties_hold_in_every_direction(name: &str, plan: &SweepPlan, law: &SlendernessLaw) {
    let (graded, one_plane) = ties_left_unheld(plan, law);
    println!(
        "{name}: {graded} needed ties graded, {} leave a direction of sway unheld: {one_plane:?}",
        one_plane.len()
    );
    assert!(graded > 0, "{name}: anti-vacuity, no tie needed a cross");
    assert!(
        one_plane.is_empty(),
        "{name}: {} ties leave a direction of sway unheld: {one_plane:?}",
        one_plane.len()
    );
}

/// A post's run over the limit with its crosses out, and the planes of the
/// crosses over it: `(post, k_lo, k_top, planes)`.
type UnheldTie = (u32, usize, usize, Vec<(i64, i64)>);

/// The needed ties graded, and those whose crosses leave a direction of sway
/// unheld (see [`assert_ties_hold_in_every_direction`]).
fn ties_left_unheld(plan: &SweepPlan, law: &SlendernessLaw) -> (usize, Vec<UnheldTie>) {
    let grid = &plan.grid;
    let all = crosses(plan);
    let plane_of = |cross: &Cross| {
        let one = plan.struts[cross.pair.0 as usize].cell_at(cross.k_top);
        let two = plan.struts[cross.pair.1 as usize].cell_at(cross.k_top);
        let step = |x: usize, y: usize| i64::try_from(y).unwrap() - i64::try_from(x).unwrap();
        (
            step(grid.row_of(one), grid.row_of(two)),
            step(grid.col_of(one), grid.col_of(two)),
        )
    };
    let mut posts: Vec<u32> = all.iter().flat_map(|c| [c.pair.0, c.pair.1]).collect();
    posts.sort_unstable();
    posts.dedup();
    let mut graded = 0_usize;
    let mut one_plane = Vec::new();
    for post in posts {
        let mine: Vec<&Cross> = all
            .iter()
            .filter(|c| c.pair.0 == post || c.pair.1 == post)
            .collect();
        let mut struts: Vec<SweepStrut> = plan.struts.clone();
        for cross in &mine {
            for &diagonal in &cross.diagonals {
                detach(&mut struts[diagonal]);
            }
        }
        let runs = unbraced_runs(grid, &struts, plan.layer_height_mm, law);
        for run in runs
            .iter()
            .filter(|run| run.strut_id == post && run.over_limit())
        {
            let over: Vec<(i64, i64)> = mine
                .iter()
                .filter(|c| run.k_lo < c.k_top && c.k_lo < run.k_top)
                .map(|c| plane_of(c))
                .collect();
            if over.is_empty() {
                continue;
            }
            graded += 1;
            // RE-DERIVED 2026-09-29 (fix/member-clearance, ruling 3): crosses
            // that meet the post at ONE band-top layer are credited together,
            // holding a direction φ when Σ cos²(φ − θ) ≥ ½ over their planes (a
            // single plane: the ±45° arc, exactly the old rule). Every direction
            // on the half-turn, sampled every 0.5°, must be held by the crosses
            // of at least one band-top layer inside the run. Until then the
            // test asked for the planes' 45° arcs to cover the half-turn, which
            // refuses two planes 60° apart that jointly hold every direction.
            let mut by_top: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
            for cross in mine
                .iter()
                .filter(|c| run.k_lo < c.k_top && c.k_lo < run.k_top)
            {
                let p = plane_of(cross);
                #[expect(clippy::cast_precision_loss, reason = "grid steps are small integers")]
                let deg = (p.1 as f64)
                    .atan2(p.0 as f64)
                    .to_degrees()
                    .rem_euclid(180.0);
                let list = by_top.entry(cross.k_top).or_default();
                if !list.iter().any(|known| (known - deg).abs() < 1.0e-9) {
                    list.push(deg);
                }
            }
            let unheld = (0..360).map(|i| f64::from(i) * 0.5).find(|phi| {
                !by_top.values().any(|planes| {
                    planes
                        .iter()
                        .map(|theta| (phi - theta).to_radians().cos().powi(2))
                        .sum::<f64>()
                        >= 0.5 - 1.0e-9
                })
            });
            if unheld.is_some() {
                one_plane.push((post, run.k_lo, run.k_top, over));
            }
        }
    }
    (graded, one_plane)
}

/// The lane's contract where a tie cannot be closed in every direction: every
/// needed tie that leaves a direction unheld stands on a post the lane WARNS
/// about over that run (emit and warn), never silently.
fn assert_unheld_ties_are_warned(name: &str, plan: &SweepPlan) {
    let (graded, unheld) = ties_left_unheld(plan, &law());
    assert!(graded > 0, "{name}: anti-vacuity, no tie needed a cross");
    let warned = &plan.report.slenderness_lane.warnings;
    let silent: Vec<_> = unheld
        .iter()
        .filter(|(post, k_lo, k_top, _)| {
            !warned.iter().any(|warning| {
                warning.strut_id == *post && warning.k_lo <= *k_lo && *k_top <= warning.k_top
            })
        })
        .collect();
    println!(
        "{name}: {graded} needed ties graded, {} left unheld, warnings {warned:?}",
        unheld.len()
    );
    assert!(
        silent.is_empty(),
        "{name}: ties leave a direction of sway unheld and the lane does not warn: {silent:?}"
    );
}

/// **RE-DERIVED 2026-10-01 (Craig's rulings: `S` = 30; keep one-direction
/// crosses).** At `S` = 30 three of the stilt plate's 35 mm posts (11, 70,
/// 76) have one plane no partner in reach can hold, a thicker root past the
/// 1.0 mm maximum, and no mast cell whose panel holds them: the lane keeps
/// their one-plane crosses (ruling 2) and WARNS for the direction across
/// them (measured 2026-10-01). So the stilt plate is graded on the lane's
/// contract, as the brace pair's production field already is: every
/// one-plane tie is named by a warning. The strict every-direction claim is
/// still graded at `S` = 50 below. At `S` = 40 (Craig ruling 2026-10-02) the
/// field places 4 masts and one post (11) is warned: 17 needed ties graded,
/// 1 left unheld and warned (measured 2026-10-02).
#[test]
fn every_needed_tie_holds_in_every_direction_on_the_stilt_plate() {
    assert_unheld_ties_are_warned("tall stilt plate", stilt());
    assert_ties_hold_in_every_direction(
        "tall stilt plate at S = 50",
        stilt_at_s_50(),
        &law_at(harness::PATH_FIXTURE_SLENDERNESS_RATIO),
    );
}

#[test]
fn every_needed_tie_holds_in_every_direction_on_the_brace_pair() {
    // RE-SCOPED 2026-09-30 (fix/chitubox-geometry), measured. The strict
    // claim is graded WITH THE LIP-SEAT RULE OFF, as the head-join fixtures
    // are: with the owner's ChituBox ball ratio (a smaller ball on every tip)
    // the lip-seat rule, which measures the seat trim's bite into a shaft with
    // the ball the lowering draws, re-lands 14 of the pair's 22 arms, and arm
    // 18 (a 27-layer branch on post 4 at layer 673) becomes a 700-layer
    // (35 mm) plate post of root radius 0.179 mm against an unbraced limit of
    // 8.9 mm. The lane ties it to post 4 in one plane only (thicken refused
    // for a root disc that would touch another, no mast admitted) and WARNS
    // (`UnbracedWarning { strut_id: 18, k_lo: 0, k_top: 700 }`). With the
    // lip-seat rule off arm 18 stays the branch, and every needed tie holds in
    // every direction (4 graded, 0 unheld). On the production field the lane's
    // contract is graded instead: a tie it could not close in every direction
    // is NAMED by its warnings (emit and warn), never left silent.
    //
    // MERGED 2026-10-01 (land of fix/chitubox-geometry onto develop 3372093):
    // the lip-seat-off field also runs at the seat placement the fixture was
    // built for (`harness::at_path_fixture_seating`), as `brace_pair()` does.
    // At the production placement the pair needs no `|X|` at all
    // (fix/lattice-align, measured 2026-09-30), and the merged tree graded 0
    // ties here (measured 2026-10-01): the anti-vacuity check went red.
    static NO_LIP_SEAT: OnceLock<SweepPlan> = OnceLock::new();
    let without_lip_seat = NO_LIP_SEAT.get_or_init(|| {
        let mesh = make_brace_pair().expect("the brace pair fixture");
        sweep_with(
            &mesh,
            0.0,
            &harness::at_path_fixture_seating(policy().without_lip_seat()),
        )
    });
    //
    // RE-SCOPED 2026-10-03 (fix/brace-window-cap, Craig's interim rules R2,
    // R7 + D5, D9): the lip-seat-off field is graded on the lane's contract
    // too. Under R2 its post 10 (a 700-layer, 35 mm post) keeps two crosses
    // in one plane and nothing across it: 5 needed ties graded, 1 unheld
    // (measured 2026-10-03). The posts stand in a line, and the study names
    // exactly this: "posts in a line can never be held across the line by
    // crosses between themselves" (`bracing-mechanics.md` §3); across the
    // line it takes thickening, a mast or a warning. The claim kept here is
    // that every such tie is NAMED by a warning (emit and warn).
    assert_unheld_ties_are_warned("brace pair, lip-seat rule off", without_lip_seat);
    assert_unheld_ties_are_warned("brace pair", brace_pair());
}

/// Test 8 of the field report: **"forced merges put back" counts only merges
/// the lane forced.** On d103c48 it read 8 on the stilt plate, and all 8 were
/// DESCENT merges the un-merge pass undid after forced merges loaded their
/// trunks; none of the lane's own was undone.
///
/// A forced merge that is put back returns to the descent the placement gave
/// it, so it stands exactly where it stands with the lane switched off; a
/// descent merge that is put back does not (with the lane off it stays
/// merged). Every id the lane publishes has to pass that test, and the two
/// kinds are published apart.
#[test]
fn forced_merges_put_back_are_the_lanes_own_and_descent_merges_are_counted_apart() {
    // Graded at the X this fixture was built at (12): at the ruled 18 the stilt
    // plate forces 3 merges and puts none back (measured 2026-09-30), so both
    // counts read 0 and the test grades nothing. The put-back rule reads no X.
    let mesh = make_tall_stilt_plate().expect("the tall stilt plate fixture");
    let plan = &sweep_with(&mesh, 0.0, &harness::at_path_fixture_x(policy()));
    let lane = &plan.report.slenderness_lane;
    let without = sweep_with(
        &mesh,
        0.0,
        &harness::at_path_fixture_x(policy()).without_slenderness(),
    );
    println!(
        "tall stilt plate: {} forced merges, {} put back {:?}, {} descent merges put back",
        lane.merges_forced,
        lane.merges_undone_by_radius,
        lane.merges_put_back_ids,
        lane.descent_merges_put_back
    );
    assert_eq!(lane.merges_undone_by_radius, lane.merges_put_back_ids.len());
    assert!(
        lane.merges_undone_by_radius <= lane.merges_forced,
        "more forced merges put back ({}) than were forced ({})",
        lane.merges_undone_by_radius,
        lane.merges_forced
    );
    for id in &lane.merges_put_back_ids {
        let with = &plan.struts[*id as usize];
        let before = &without.struts[*id as usize];
        assert!(
            with.landing == before.landing && with.k_bottom == before.k_bottom,
            "arm {id} is published as a forced merge put back, but it does not stand where the \
             placement had it: {:?}@{} against {:?}@{}",
            with.landing,
            with.k_bottom,
            before.landing,
            before.k_bottom
        );
    }
    assert!(
        lane.merges_undone_by_radius > 0 && lane.descent_merges_put_back > 0,
        "anti-vacuity: the stilt plate must exercise both counts: {lane:?}"
    );
}

/// **The mast rung, graded where it has to fire.** With the crosses and the
/// thickening deleted, the lonely peak's two arms can only be tied to masts.
/// Every mast must then (a) be the only mast on its arm in its plane — an
/// arm's mast is extended to a later tie rather than joined by a second one —
/// and (b) keep its root clear of every other root once the field is sized
/// (on d103c48 both of the peak's masts lapped their arms' roots, placed at
/// the minimum disc clearance, 1.2 mm, inside the 2.2-2.4 mm the braced panel
/// needs).
#[test]
fn masts_stand_where_the_panel_holds_one_to_an_arm_in_each_plane() {
    let mesh = make_lonely_peak().expect("the lonely peak fixture");
    // Graded at the X this fixture was built at (12). At the ruled 18 the
    // peak's arms are thinner than any mast the law would stand beside them, so
    // every mast cell is refused as thicker than its member (23,448 refusals,
    // 0 masts; measured 2026-09-30) and this rung is never reached.
    // And at the S it was built at (`harness::PATH_FIXTURE_SLENDERNESS_RATIO`):
    // at the ruled 30 the panel check refuses every mast cell (23,448
    // refusals, 0 masts; measured 2026-10-01).
    let plan = sweep_with(
        &mesh,
        0.0,
        &harness::at_path_fixture_slenderness(harness::at_path_fixture_x(policy()))
            .without_slenderness_thicken(),
    );
    let lane = &plan.report.slenderness_lane;
    let masts: Vec<&SweepStrut> = plan
        .struts
        .iter()
        .filter(|strut| strut.kind == StrutKind::Mast)
        .collect();
    println!(
        "lonely peak, masts only: {} masts, panel-refused cells {}, extended {}, lane {lane:?}",
        masts.len(),
        lane.mast_cells_refused_by_panel,
        lane.masts_extended
    );
    assert!(
        !masts.is_empty(),
        "anti-vacuity: no mast on the lonely peak"
    );
    // (arm, mast) for every cross that ties an arm to a mast.
    type MastInPlane = (u32, (i64, i64));
    let mut per_arm: BTreeMap<u32, Vec<MastInPlane>> = BTreeMap::new();
    let grid = &plan.grid;
    for cross in crosses(&plan) {
        let (one, two) = cross.pair;
        let (arm, mast) = match (
            plan.struts[one as usize].kind,
            plan.struts[two as usize].kind,
        ) {
            (StrutKind::Mast, _) => (two, one),
            (_, StrutKind::Mast) => (one, two),
            _ => continue,
        };
        let a = plan.struts[arm as usize].cell_at(cross.k_top);
        let m = plan.struts[mast as usize].cell;
        let step = |x: usize, y: usize| i64::try_from(y).unwrap() - i64::try_from(x).unwrap();
        let (mut dr, mut dc) = (
            step(grid.row_of(a), grid.row_of(m)),
            step(grid.col_of(a), grid.col_of(m)),
        );
        let g = gcd(dr.abs(), dc.abs()).max(1);
        (dr, dc) = (dr / g, dc / g);
        if dr < 0 || (dr == 0 && dc < 0) {
            (dr, dc) = (-dr, -dc);
        }
        let list = per_arm.entry(arm).or_default();
        if !list.contains(&(mast, (dr, dc))) {
            list.push((mast, (dr, dc)));
        }
    }
    for (arm, list) in &per_arm {
        let mut planes: Vec<(i64, i64)> = list.iter().map(|(_, plane)| *plane).collect();
        planes.sort_unstable();
        let distinct = planes.len();
        planes.dedup();
        assert_eq!(
            planes.len(),
            distinct,
            "arm {arm} carries two masts in one plane: {list:?}"
        );
    }
    assert_eq!(
        lane.mast_roots_overlapping_after_sizing, 0,
        "a mast root laps another root once the field is sized"
    );
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// The lonely peak is the fixture BUILT to have no partner: its two long arms
/// stand either side of a stalk that blocks their mutual cross, and nothing
/// else is in reach. On d103c48 it got two masts at 1.2 mm, inside the 2.2-2.4
/// mm the braced panel needs, lapping their arms' roots after sizing. The
/// thicker root holds both arms in every direction; the panel does not.
/// (Re-derived 2026-10-01, below: one arm is now warned.)
#[test]
fn the_lonely_peak_is_held_without_a_mast_and_without_a_warning() {
    let plan = lonely_peak();
    let lane = &plan.report.slenderness_lane;
    let masts = count_kind(plan, StrutKind::Mast);
    println!("lonely peak: {masts} masts, lane {lane:?}");
    assert!(
        lane.runs_over_limit_before > 0,
        "anti-vacuity: the lonely peak has no over-limit run to remedy"
    );
    assert_eq!(masts, 0, "the lonely peak placed {masts} masts");
    // RE-DERIVED 2026-10-01 (Craig's ruling: a landing ties its host only
    // from its seat). Measured to the seat, arm 4's run is the whole 35 mm
    // post, and the root that would hold it lays its flare onto a
    // neighbour's (3 thickenings refused by the disc); no mast cell holds it
    // either, so it is WARNED. Arm 1 is still held by its thicker root. The
    // claim kept is the ladder's: no warning without a refused thickening.
    let refused = lane.thicken_refused_disc + lane.thicken_refused_radius;
    assert!(
        lane.members_thickened > 0,
        "anti-vacuity: the thicken rung never fired on the lonely peak"
    );
    assert!(
        lane.warnings.len() <= refused,
        "the lonely peak leaves {} warnings but only {refused} thickenings were refused: {:?}",
        lane.warnings.len(),
        lane.warnings
    );
}
