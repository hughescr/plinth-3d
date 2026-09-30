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

fn laid_tube() -> &'static Pair {
    static FIXTURE: OnceLock<Pair> = OnceLock::new();
    FIXTURE.get_or_init(|| pair(&make_laid_tube().expect("the laid tube")))
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

#[test]
fn a_lip_seat_relanding_laps_no_new_plate_root_on_the_stilt_plate() {
    assert_no_new_root_pair("make_tall_stilt_plate", stilt());
}

/// The laid tube's finished root pairs went 1 -> 7 at `ee07600`.
#[test]
fn a_lip_seat_relanding_laps_no_new_plate_root_on_the_laid_tube() {
    assert_no_new_root_pair("make_laid_tube", laid_tube());
}

/// **Remedy 2, the section-only search.** A tip whose disc is on its face
/// but whose shaft the trim cuts joins the footprint search; it moves only
/// onto a seat where the moved arm's shaft is NOT cut, and its disc stays on
/// the face. Graded on the emitted arms with the lowering's own measurement
/// of the cut ([`plinth_support::tip_shaft_seat_bites`]) and the footprint's
/// own status on each moved tip.
#[test]
fn a_tip_moved_for_its_section_keeps_its_section_and_its_disc_on_the_face() {
    let plan = &stilt().on;
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
    let plan = &sweep_with(
        &make_tall_stilt_plate().expect("the tall stilt plate"),
        0.0,
        &crate::harness::at_path_fixture_seating(policy()),
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
