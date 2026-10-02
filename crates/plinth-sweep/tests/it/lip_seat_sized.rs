//! **A lip-seat re-landing stands on the SIZED field: it overloads no host
//! and laps no plate root the field did not already lap; and a tip searched
//! for its section alone moves only onto a seat where its shaft keeps it**
//! (fix/lip-seat round 2, 2026-09-29).
//!
//! # The defects, measured by the challenger on `ee07600`
//!
//! The lip-seat rule re-lands an arm whose shaft its seat trim would cut, and
//! hands the arm's whole load to a new host chain, or makes the arm a new
//! plate root. Only the down-sweep that follows knows how thick that makes the
//! chain and its root, and the rule never looked:
//!
//! * `make_tall_stilt_plate` (pinned, identity): arm 69 overloaded on 20
//!   layers, 40.30 mm² at `X` 12.83 against `X_ALLOWED` 12 (for-craig-v4: 0);
//! * `make_laid_tube`: overlapping plate-root pairs on the finished field
//!   1 -> 7; Round_25 and Round_100 0 -> 2.
//!
//! # What is graded, and how
//!
//! Every claim reads the finished strut set and the lowering's drawn arms
//! ([`plinth_sweep::arms_of`]). The overload test is the area law read off the
//! ledger with the registry's `X_ALLOWED` and maximum radius; the root discs
//! are the drawn roots ([`plinth_support::drawn_root_radius_mm`], the
//! lowering's one rule) flared by the registry's factor, compared pair by pair
//! in `O(n²)` (not the production sweep line). Each fixture is also swept with
//! the rule OFF ([`SweepPolicy::without_lip_seat`]): the rule may not add an
//! overloaded member or an overlapping root pair that field does not have.

// Test-local arithmetic over bounded fixture coordinates.
#![allow(clippy::cast_precision_loss)]

use std::collections::BTreeSet;
use std::sync::OnceLock;

use crate::harness;

use harness::sweep_with;
use plinth_mesh::{make_laid_tube, make_tall_stilt_plate, Mesh};
use plinth_sweep::{arms_of, Landing, StrutKind, SweepPlan, SweepPolicy};

fn policy() -> SweepPolicy {
    SweepPolicy::from_registry().expect("the registry supplies every number the sweep reads")
}

struct Pair {
    on: SweepPlan,
    off: SweepPlan,
}

fn pair(mesh: &Mesh) -> Pair {
    Pair {
        on: sweep_with(mesh, 0.0, &policy()),
        off: sweep_with(mesh, 0.0, &policy().without_lip_seat()),
    }
}

fn stilt() -> &'static Pair {
    static FIXTURE: OnceLock<Pair> = OnceLock::new();
    FIXTURE.get_or_init(|| pair(&make_tall_stilt_plate().expect("the tall stilt plate")))
}

/// The stilt plate, rule ON, graded at the X its section-move path was built at
/// (`crate::harness::PATH_FIXTURE_X_ALLOWED` = 12).
///
/// RE-SCOPED 2026-09-30 (fix/chitubox-geometry), measured: with the owner's
/// ChituBox ball ratios (`CHITUBOX_TIP_CONTACT_PROFILES`, 1.25-1.5 by tip size
/// where every sweep ball was 1.5 × r before) the production stilt plate at
/// X 18 searches 2 tips for their section, moves 0 and refuses 2,844 slides,
/// so the two section-move tests below graded nothing. The rules they grade
/// read no ball factor and no X; only the fixture stopped reaching them.
fn stilt_at_path_x() -> &'static SweepPlan {
    static FIXTURE: OnceLock<SweepPlan> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        sweep_with(
            &make_tall_stilt_plate().expect("the tall stilt plate"),
            0.0,
            &crate::harness::at_path_fixture_x(policy()),
        )
    })
}

fn laid_tube() -> &'static Pair {
    static FIXTURE: OnceLock<Pair> = OnceLock::new();
    FIXTURE.get_or_init(|| pair(&make_laid_tube().expect("the laid tube")))
}

/// The laid tube, rule ON and OFF, at the registry's S, with the slenderness
/// TIES (`|X|`, thicken, mast) off in both, so the field the root-pair test
/// reads is the field the rule decided on.
///
/// RE-SCOPED 2026-10-01 (round-10 milestone), measured
/// (`plinth-evidence/.../field-2026-09-28/lanes/round10-milestone/`). Bisect:
/// green at `247b8d5` and `ca2c545`, red from the bracing rework's first
/// commit `6732343` (S 50 → 30). At S 30 the finished rule-ON field has the
/// pair (4, 103): centres 1.373 mm apart, flared roots 1.001 + 0.421 mm.
/// Root 4 is the foot of the re-landings of arms 16 and 45 (load 15.30 →
/// 22.12 mm², drawn root 0.520 → 0.625 mm), and the rule's own last sized
/// check passed it (`new_root_pairs_unattributed` 0). Arm 103 carries the
/// same load with the rule on and off (1.585 mm², ledger root 0.167 mm), but
/// its DRAWN root is 0.263 mm with the rule on against 0.167 mm off: the ties
/// pass, which runs after the rule, seats three braces on it with the rule on
/// (141, 224, 226) and one with it off (186), and a brace junction raises the
/// trunk below it to the brace. A cross's root growth is published in
/// `cross_roots_lapping`, not refused (`slenderness::grow_foot_disc`), and the
/// rule-OFF field at S 30 has a different set of crosses, so on the finished
/// field the comparison no longer isolates the rule. With the ties off, at
/// S 30: rule on 2 pairs, rule off 5, 2 moves put back for roots (path hits).
/// The claim and its comparison are unchanged; the overload test above keeps
/// the full field. NOT A PUT-BACK WITNESS: with the sized check's put-back
/// skipped (sabotage, round-10 milestone) this fixture still has no new pair,
/// with the ties off or at S 50, so this test grades the outcome only.
fn laid_tube_untied() -> &'static Pair {
    static FIXTURE: OnceLock<Pair> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let mesh = make_laid_tube().expect("the laid tube");
        let untied = SweepPolicy {
            slenderness_brace: false,
            slenderness_thicken: false,
            slenderness_mast: false,
            ..policy()
        };
        Pair {
            on: sweep_with(&mesh, 0.0, &untied),
            off: sweep_with(&mesh, 0.0, &untied.without_lip_seat()),
        }
    })
}

/// Every member of `plan` that carries more at some layer than `X_ALLOWED`
/// allows at the registered maximum radius, with its largest such load.
fn overloaded(plan: &SweepPlan) -> Vec<(u32, f64)> {
    let policy = policy();
    let limit = policy.x_allowed
        * core::f64::consts::PI
        * policy.tip_radius_max_mm
        * policy.tip_radius_max_mm;
    plan.struts
        .iter()
        .filter_map(|strut| {
            let worst = strut.carried_mm2.iter().copied().fold(0.0_f64, f64::max);
            (worst > limit).then_some((strut.id, worst))
        })
        .collect()
}

/// Every overlapping pair of flared plate roots as the file draws them,
/// `(lower id, higher id)`.
fn root_pairs(plan: &SweepPlan) -> BTreeSet<(u32, u32)> {
    let flare = policy().flare_r_factor;
    let arms = arms_of(plan);
    let roots: Vec<(u32, f64, f64, f64)> = plan
        .struts
        .iter()
        .zip(&arms)
        .filter(|(strut, _)| {
            matches!(strut.landing, Landing::Plate) && strut.kind != StrutKind::Brace
        })
        .filter_map(|(strut, arm)| {
            let root =
                plinth_support::drawn_root_radius_mm(&arm.radius_profile_mm, arm.tip_radius_mm);
            (root.is_finite() && root > 0.0).then(|| {
                let cell = strut.base_cell();
                (
                    strut.id,
                    plan.grid.center_x_mm(plan.grid.col_of(cell)),
                    plan.grid.center_y_mm(plan.grid.row_of(cell)),
                    root * flare,
                )
            })
        })
        .collect();
    let mut out = BTreeSet::new();
    for (slot, a) in roots.iter().enumerate() {
        for b in &roots[slot + 1..] {
            if (b.1 - a.1).hypot(b.2 - a.2) < a.3 + b.3 {
                out.insert((a.0.min(b.0), a.0.max(b.0)));
            }
        }
    }
    out
}

/// The members a re-landed arm's load now goes through: itself, and down its
/// landings to its foot.
fn chains_of_relanded(plan: &SweepPlan) -> Vec<Vec<u32>> {
    plan.report
        .lip_seat
        .relanded_ids
        .iter()
        .map(|&id| {
            let mut chain = Vec::new();
            let mut at = id as usize;
            for _ in 0..=plan.struts.len() {
                let Some(member) = plan.struts.get(at) else {
                    break;
                };
                chain.push(member.id);
                match member.landing {
                    Landing::Strut { id, .. } => at = id as usize,
                    _ => break,
                }
            }
            chain
        })
        .collect()
}

fn assert_no_new_overload(name: &str, fixture: &Pair) {
    let on = overloaded(&fixture.on);
    let off = overloaded(&fixture.off);
    let lip = &fixture.on.report.lip_seat;
    assert!(
        lip.arms_relanded > 0,
        "{name}: the rule re-landed nothing, so this test grades nothing"
    );
    let was = |id: u32| {
        off.iter()
            .find(|(member, _)| *member == id)
            .map(|row| row.1)
    };
    // THE CLAIM: no member the re-landings load is overloaded where the
    // field without the rule was not, or carries more past its limit.
    let chains = chains_of_relanded(&fixture.on);
    let fed: BTreeSet<u32> = chains.iter().flatten().copied().collect();
    let broken: Vec<(u32, f64, Option<f64>)> = on
        .iter()
        .filter(|(id, _)| fed.contains(id))
        .filter(|(id, carried)| was(*id).is_none_or(|before| *carried > before + 1e-9))
        .map(|(id, carried)| (*id, *carried, was(*id)))
        .collect();
    println!(
        "{name}: re-landed {} (put back for host load {}, for roots {}; {} sized checks); \
         overloaded members rule on {:?}, rule off {:?}",
        lip.arms_relanded,
        lip.moves_put_back_host_load,
        lip.moves_put_back_roots,
        lip.sizing_checks,
        on,
        off
    );
    assert!(
        broken.is_empty(),
        "{name}: members a lip-seat re-landing loads came out overloaded (id, carried mm2, \
         carried with the rule off): {broken:?}"
    );
}

fn assert_no_new_root_pair(name: &str, fixture: &Pair) {
    let on = root_pairs(&fixture.on);
    let off = root_pairs(&fixture.off);
    let feet: BTreeSet<u32> = chains_of_relanded(&fixture.on)
        .iter()
        .filter_map(|chain| chain.last().copied())
        .collect();
    let new: Vec<&(u32, u32)> = on
        .iter()
        .filter(|pair| !off.contains(pair))
        .filter(|(a, b)| feet.contains(a) || feet.contains(b))
        .collect();
    let lip = &fixture.on.report.lip_seat;
    println!(
        "{name}: overlapping root pairs rule on {} {on:?}, rule off {} {off:?}; feet of \
         re-landed chains {}; put back for roots {}; new pairs no re-landing explains at the \
         rule's last check {}",
        on.len(),
        off.len(),
        feet.len(),
        lip.moves_put_back_roots,
        lip.new_root_pairs_unattributed
    );
    assert!(
        new.is_empty(),
        "{name}: plate roots at the foot of a lip-seat re-landing lap a root the field without \
         the rule does not: {new:?}"
    );
}

/// The stilt plate's arm 69 was overloaded on 20 layers at `ee07600`.
#[test]
fn a_lip_seat_relanding_overloads_no_host_on_the_stilt_plate() {
    assert_no_new_overload("make_tall_stilt_plate", stilt());
}

#[test]
fn a_lip_seat_relanding_overloads_no_host_on_the_laid_tube() {
    assert_no_new_overload("make_laid_tube", laid_tube());
}

/// The laid tube at the slenderness ratio registered until 2026-10-01
/// (`harness::PATH_FIXTURE_SLENDERNESS_RATIO`, S = 50): the field on which the
/// lip-seat rule left a host over its limit across passes.
fn laid_tube_at_s_50() -> &'static Pair {
    static FIXTURE: OnceLock<Pair> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let mesh = make_laid_tube().expect("the laid tube");
        let at_50 = crate::harness::at_path_fixture_slenderness(policy());
        Pair {
            on: sweep_with(&mesh, 0.0, &at_50),
            off: sweep_with(&mesh, 0.0, &at_50.without_lip_seat()),
        }
    })
}

/// **Across passes** (fix/creep-45 round 11, 2026-10-02). Measured on
/// `cd4efe7` at the then-registered S = 50, creep cut on, and red again
/// (same 56.5825 mm²) at S = 50 on the round-11 merge `ab597a4`; the
/// production laid tube at S = 30 no longer reaches it, so the fixture pins
/// S = 50 (`harness::at_path_fixture_slenderness`). Arm 21 is re-landed in the rule's FIRST
/// pass onto member 7's shaft (+2.62 mm²); that pass's sized check finds 7
/// under its limit. A later pass's moves shift load onto 7, and its check
/// finds 7 at 56.5825 mm² against 18 × π × 1.0² = 56.549 mm², but no move OF
/// THAT PASS feeds 7's chain, so it was judged "unattributed" and left
/// standing. Every check now judges every move still standing, from every
/// pass, against the field before the rule's first move.
#[test]
fn a_lip_seat_relanding_overloads_no_host_on_the_laid_tube_across_passes() {
    let fixture = laid_tube_at_s_50();
    let lip = &fixture.on.report.lip_seat;
    println!(
        "make_laid_tube at S = 50: across passes {} moves refused, {} re-runs, {} runs withdrawn; \
         unattributed at the last check {} overloads, {} root pairs",
        lip.moves_refused_across_passes,
        lip.reruns,
        lip.runs_withdrawn,
        lip.new_overloads_unattributed,
        lip.new_root_pairs_unattributed
    );
    // PATH HIT: the cross-pass judgement is what fixed it (measured 2026-10-02:
    // 1 earlier-pass move refused, 1 re-run), not a withdrawal of the run.
    assert!(
        lip.moves_refused_across_passes > 0,
        "the cross-pass path never fired"
    );
    assert_eq!(lip.runs_withdrawn, 0, "the run was withdrawn whole");
    assert_no_new_overload("make_laid_tube at S = 50", fixture);
}

/// **Sabotage** of the test above: each check judging only its own pass's
/// moves, against the field the previous pass left
/// ([`SweepPolicy::without_lip_seat_across_passes`]), leaves member 7 over
/// its limit again, and the cross-pass path never fires.
#[test]
fn judged_pass_by_pass_the_laid_tube_at_s_50_keeps_a_host_over_its_limit() {
    let mesh = make_laid_tube().expect("the laid tube");
    let sabotaged = sweep_with(
        &mesh,
        0.0,
        &crate::harness::at_path_fixture_slenderness(policy()).without_lip_seat_across_passes(),
    );
    let lip = &sabotaged.report.lip_seat;
    let over = overloaded(&sabotaged);
    println!(
        "make_laid_tube at S = 50, pass by pass: overloaded {over:?}; across passes {} moves \
         refused, {} re-runs; unattributed at the last check {}",
        lip.moves_refused_across_passes, lip.reruns, lip.new_overloads_unattributed
    );
    assert_eq!(lip.moves_refused_across_passes, 0);
    assert_eq!(lip.reruns, 0);
    assert!(
        !over.is_empty(),
        "judged pass by pass, the laid tube at S = 50 no longer keeps a host over its limit, so \
         the test above witnesses nothing"
    );
}

#[test]
fn a_lip_seat_relanding_laps_no_new_plate_root_on_the_stilt_plate() {
    assert_no_new_root_pair("make_tall_stilt_plate", stilt());
}

/// The laid tube's finished root pairs went 1 -> 7 at `ee07600`. Graded with
/// the ties off since 2026-10-01; [`laid_tube_untied`] gives the measured
/// reason.
#[test]
fn a_lip_seat_relanding_laps_no_new_plate_root_on_the_laid_tube() {
    assert_no_new_root_pair("make_laid_tube", laid_tube_untied());
}

/// **Remedy 2, the section-only search.** A tip whose disc is on its face
/// but whose shaft the trim cuts joins the footprint search; it moves only
/// onto a seat where the moved arm's shaft is NOT cut, and its disc stays on
/// the face. Graded on the emitted arms with the lowering's own measurement
/// of the cut ([`plinth_support::tip_shaft_seat_bites`]) and the footprint's
/// own status on each moved tip.
#[test]
fn a_tip_moved_for_its_section_keeps_its_section_and_its_disc_on_the_face() {
    let plan = stilt_at_path_x();
    let footprint = &plan.report.seat_footprint;
    println!(
        "stilt: footprint {} tips searched for their section, {} moved, {} slides refused",
        footprint.tips_graded_for_section,
        footprint.tips_moved_for_section,
        footprint.moves_refused_section
    );
    assert!(
        footprint.tips_moved_for_section > 0,
        "no tip moved for its section on the stilt plate, so this test grades nothing"
    );
    let arms = arms_of(plan);
    let layer_plan = plinth_core::LayerPlan::new(
        plan.plate_z_mm,
        vec![plan.layer_height_mm; plan.layers.max(1)],
    )
    .expect("the plan's own uniform layers");
    let bites = plinth_support::tip_shaft_seat_bites(&arms, &layer_plan)
        .expect("the lowering measures the emitted arms");
    let mut graded = 0;
    let mut broken: Vec<(u32, String)> = Vec::new();
    for &id in &footprint.moved_for_section {
        let Some(index) = plan.struts.iter().position(|strut| strut.id == id) else {
            broken.push((id, "not in the finished field".into()));
            continue;
        };
        graded += 1;
        if plan.struts[index].seat_face.footprint != plinth_sweep::Footprint::MovedOntoFace {
            broken.push((
                id,
                format!("status {:?}", plan.struts[index].seat_face.footprint),
            ));
        }
        if let Some(bite) = bites[index].as_ref().filter(|bite| bite.bite_mm > 1e-9) {
            broken.push((
                id,
                format!("shaft cut {:.4} mm outside its ball", bite.bite_mm),
            ));
        }
    }
    println!("stilt: {graded} tips moved for their section graded; broken {broken:?}");
    assert_eq!(graded, footprint.tips_moved_for_section);
    assert!(
        broken.is_empty(),
        "tips moved along their face for their section do not hold: {broken:?}"
    );
}

/// **A put-back leaves nothing standing where a moved shaft was**
/// (round5-review-fixes 2026-09-29). Every re-landing the sized field put
/// back takes the arms its judging sizing inserted with it
/// (`lip_seat::put_back_all`), so every member-on-member landing stands after
/// each lip-seat pass: the production landing check the run records after
/// each pass. Fails on zero put-backs.
fn assert_every_landing_stands_after_the_lip_seat_passes(name: &str, plan: &SweepPlan) {
    let lip = &plan.report.lip_seat;
    let put_back = lip.moves_put_back_host_load + lip.moves_put_back_roots;
    let passes: Vec<_> = plan
        .report
        .landing_check
        .passes
        .iter()
        .filter(|pass| pass.pass.starts_with("lip seats"))
        .collect();
    println!(
        "{name}: {put_back} re-landings put back; {} inserted arms withdrawn with them ({} on a \
         move put back); landing checks after the lip-seat passes {passes:?}",
        lip.inserted_arms_withdrawn, lip.inserted_on_a_put_back_move
    );
    assert!(
        put_back > 0,
        "{name}: nothing was put back, so this grades nothing"
    );
    assert!(
        !passes.is_empty(),
        "{name}: no landing check after a lip-seat pass"
    );
    for pass in passes {
        assert!(
            pass.invalid_landings == 0 && pass.unrooted == 0,
            "{name}: after '{}' {} landings do not stand and {} members are unrooted",
            pass.pass,
            pass.invalid_landings,
            pass.unrooted
        );
    }
}

#[test]
fn every_landing_stands_after_a_lip_seat_put_back_on_the_stilt_plate() {
    assert_every_landing_stands_after_the_lip_seat_passes("make_tall_stilt_plate", &stilt().on);
}

#[test]
fn every_landing_stands_after_a_lip_seat_put_back_on_the_laid_tube() {
    // Graded at the X this fixture was built at (12,
    // `crate::harness::PATH_FIXTURE_X_ALLOWED`): at the ruled 18 the laid tube
    // puts no re-landing back (measured 2026-09-30), so the shared production
    // plan grades nothing here. The put-back rule reads no X.
    let plan = sweep_with(
        &make_laid_tube().expect("the laid tube"),
        0.0,
        &crate::harness::at_path_fixture_x(policy()),
    );
    assert_every_landing_stands_after_the_lip_seat_passes("make_laid_tube", &plan);
}

/// **A section move taken back leaves every other move within the rules**
/// (round5-review-fixes 2026-09-29). The footprint rule admits a set of moves
/// together, so a branch's slide can hold the lean law only because its base
/// follows its host's slide; when the host's slide is taken back for a shaft
/// still cut, every move left is judged again (`footprint::settle`). Graded
/// on the emitted arms against the arms with every footprint move taken back
/// ([`crate::footprint_moves_hold_the_rules::grade`]). Fails unless a section
/// move was taken back on this field.
#[test]
fn a_section_move_taken_back_leaves_every_displaced_member_within_the_rules() {
    // Graded at the seat placement the fixture was built for
    // (`harness::at_path_fixture_seating`): at the production placement the
    // stilt plate takes no section move back (measured 2026-09-30: 0), so
    // there would be nothing displaced to grade. The rule reads no placement.
    //
    // MERGED 2026-10-01 (land of fix/chitubox-geometry onto develop 3372093):
    // fix/chitubox-geometry graded this at the path X (`stilt_at_path_x`), and
    // fix/lattice-align at the path seating. The two answer two different
    // causes and the merged tree has both, so the stilt plate is graded under
    // both.
    let plan = &sweep_with(
        &make_tall_stilt_plate().expect("the tall stilt plate"),
        0.0,
        &crate::harness::at_path_fixture_seating(crate::harness::at_path_fixture_x(policy())),
    );
    let footprint = &plan.report.seat_footprint;
    let graded = crate::footprint_moves_hold_the_rules::grade(plan);
    println!(
        "stilt: {} section moves taken back, then {} moves taken back by the rules over {} \
         re-checks ({} member checks); {} members displaced; lean limit {:.4}; over the lean law \
         {:?}; over a run {:?}",
        footprint.section_moves_taken_back,
        footprint.moves_taken_back_limits,
        footprint.settle_rechecks,
        footprint.members_checked,
        graded.displaced,
        graded.max_lean,
        graded.over_lean,
        graded.over_run
    );
    assert!(
        footprint.section_moves_taken_back > 0 && footprint.settle_rechecks > 0,
        "no section move was taken back on the stilt plate, so this grades nothing"
    );
    assert!(
        graded.over_lean.is_empty() && graded.over_run.is_empty(),
        "members displaced by the moves left after a take-back break a rule: lean {:?}, run {:?}",
        graded.over_lean,
        graded.over_run
    );
}

/// **A re-landing is put back only for load its move ADDED; the longest
/// offender goes first** (fix/lip-seat-blame, 2026-10-01).
///
/// for-craig-v8 (`247b8d5`), the stilt plate at the production policy: the
/// arms seated in the cells at (-8.675, 14.325) and (-1.375, 14.325) (engine
/// arms 78 and 80) first re-landed DOWN the near post they already stood on,
/// 3.65 and 3.6 mm sideways. Each was put back because that post came out
/// overloaded, although other arms' moves had added the load
/// (`lip_seat::blame`), and the refusal held for the rest of the rule: they
/// ended 9.60 and 9.55 mm sideways on a far post, 13.5 mm branches
/// (`stilt-and-brace-pair-diagnosis.md` item 1). The posts stand in rows
/// 7.25 mm apart; the near post is 3.65 mm from each seat, the next 6.25 and
/// then 9.6 mm, so 3.7 mm (the near post and the grid's rounding of it)
/// separates the near post from every other host.
#[test]
fn the_stilt_plate_arms_that_stood_on_the_near_post_end_on_it() {
    const NEAR_POST_MM: f64 = 3.7;
    let plan = &stilt().on;
    let grid = &plan.grid;
    let centre = |cell: usize| {
        (
            grid.center_x_mm(grid.col_of(cell)),
            grid.center_y_mm(grid.row_of(cell)),
        )
    };
    let lateral = |cell: usize, base: usize| {
        let (a, b) = (centre(cell), centre(base));
        (b.0 - a.0).hypot(b.1 - a.1)
    };
    let lip = &plan.report.lip_seat;
    // The same blame sent arms 24 and 63 7.26 and 6.25 mm sideways at v8; no
    // seat on this lattice is further than 3.65 mm from a post column, so no
    // re-landing has cause to travel past the near one.
    let mut far: Vec<(u32, f64)> = Vec::new();
    for &id in &lip.relanded_ids {
        let strut = &plan.struts[id as usize];
        let travel = lateral(strut.cell, strut.base_cell());
        if travel > NEAR_POST_MM {
            far.push((id, travel));
        }
        println!(
            "stilt: re-landed arm {id} seat {:?} base {:?} lateral {:.3} mm landing {:?}",
            centre(strut.cell),
            centre(strut.base_cell()),
            lateral(strut.cell, strut.base_cell()),
            strut.landing
        );
    }
    println!(
        "stilt: put back for host load {}, for roots {}; spared (load already on the member) \
         {}; held behind a longer offender {}; {} sized checks",
        lip.moves_put_back_host_load,
        lip.moves_put_back_roots,
        lip.blame_spared_already_carried,
        lip.blame_held_behind_longer,
        lip.sizing_checks
    );
    let mut graded = 0;
    for seat in [(-8.675, 14.325), (-1.375, 14.325)] {
        let Some(strut) = plan.struts.iter().find(|strut| {
            strut.kind == StrutKind::Arm && {
                let (x, y) = centre(strut.cell);
                (x - seat.0).abs() < grid.pitch_mm() / 2.0
                    && (y - seat.1).abs() < grid.pitch_mm() / 2.0
            }
        }) else {
            panic!("no arm seated in the cell at {seat:?}: the fixture moved");
        };
        graded += 1;
        let travel = lateral(strut.cell, strut.base_cell());
        assert!(
            travel <= NEAR_POST_MM,
            "arm {} seated at {seat:?} travels {travel:.3} mm sideways to {:?} ({:?}), past the \
             near post {NEAR_POST_MM} mm away",
            strut.id,
            centre(strut.base_cell()),
            strut.landing
        );
    }
    assert_eq!(graded, 2);
    assert!(
        !lip.relanded_ids.is_empty() && far.is_empty(),
        "re-landed arms travel past the near post (id, mm): {far:?}"
    );
}
