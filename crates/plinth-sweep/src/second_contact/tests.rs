//! The acceptance contract's own tests (SC-0). Each fixture is a defect the
//! contract must see; the sabotage notes say which line, broken, turns it red.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    canonical, compare, final_checks, orphaned_by, overgrown_merges, remap_for, remap_refused,
    withdrawal_closure, Class, FinalEnv, Finished, Globals, Key, Provenance, Warning,
};
use crate::grid::lay_out_at;
use crate::heads::{JointCertState, OldLawReason};
use crate::slenderness::SlendernessLaw;
use crate::{Landing, SeatFace, StrutKind, SweepStrut};

/// A member with the fields the contract reads; everything else inert.
fn member(id: u32, landing: Landing, seat_on: Option<u32>, radius: f64) -> SweepStrut {
    SweepStrut {
        id,
        cell: 0,
        center_mm: [0.0, 0.0],
        kind: StrutKind::Arm,
        min_radius_mm: 0.0,
        thickened_mm: Vec::new(),
        foot_radius_mm: None,
        seat_on_strut: seat_on,
        k_top: 20,
        k_bottom: 0,
        landing,
        interior: false,
        radius_mm: vec![radius; 21],
        carried_mm2: vec![0.0; 21],
        covered_demand: Vec::new(),
        held_births: Vec::new(),
        twin_host: false,
        inserted_by_load: false,
        path: Vec::new(),
        straight_fallback: None,
        fallback_path: Vec::new(),
        seat_face: SeatFace::default(),
        seat_normal: [0.0; 3],
        head: None,
    }
}

fn warning(class: Class, a: u32, b: u32, severity: f64) -> Warning {
    Warning { key: Key { class, a, b }, severity }
}

fn finished(warnings: Vec<Warning>) -> Finished {
    Finished::from_parts(warnings, Globals::default())
}

/// `n` members all on the plate, so ids are indexes.
fn field(n: u32) -> Vec<SweepStrut> {
    (0..n).map(|id| member(id, Landing::Plate, None, 0.2)).collect()
}

fn provenance(n_c: u32) -> Provenance {
    Provenance { n_c, ..Provenance::default() }
}

// ------------------------------------------------------------- warning keys

/// **An equal-count swap is a regression** (astra round 2 finding 2). The first
/// plan's tip 3 is short; the edit's tip 3 is fixed and tip 7 is short instead:
/// one warning each, the same count, the same summed shortfall. A count or a sum
/// reads it as unchanged; the keys read a NEW warning on member 7. Nothing links
/// it to a new arm, so the edit is discarded; with tip 7 the trigger host of new
/// arm 10 it is traced to that arm, which is withdrawn instead.
/// SABOTAGE: comparing `warnings.len()` (or any sum) in `compare` turns it red.
#[test]
fn an_equal_count_warning_swap_is_a_regression_on_the_member_that_is_new() {
    let first = finished(vec![warning(Class::ContactShort, 3, 0, 0.5)]);
    let edited = finished(vec![warning(Class::ContactShort, 7, 0, 0.5)]);
    assert_eq!(first.warnings.len(), edited.warnings.len());
    let struts = field(11);
    let verdict = compare(&first, &edited, &struts, &provenance(10));
    assert_eq!(verdict.untraced.len(), 1, "{verdict:?}");
    assert_eq!(verdict.untraced[0].key, Key { class: Class::ContactShort, a: 7, b: 0 });
    assert!(verdict.discard() && verdict.withdraw.is_empty());
    let mut linked = provenance(10);
    linked.trigger_host.insert(10, 7);
    let verdict = compare(&first, &edited, &struts, &linked);
    assert!(!verdict.discard(), "{verdict:?}");
    assert_eq!(verdict.withdraw, BTreeSet::from([10]));
    assert_eq!(verdict.traced[0].1, vec![10]);
}

/// **A worse existing warning is a regression; a better or an equal one is
/// not.** Tip 3 was short by 0.5 mm²: 0.6 is worse, 0.4 and 0.5 are not, and
/// float noise on 0.5 is not.
/// SABOTAGE: `worse` always false turns the first assertion red; always true,
/// the second.
#[test]
fn a_worsened_existing_warning_is_a_regression_and_an_eased_one_is_not() {
    let first = finished(vec![warning(Class::ContactShort, 3, 0, 0.5)]);
    let struts = field(4);
    let run = |severity: f64| {
        let edited = finished(vec![warning(Class::ContactShort, 3, 0, severity)]);
        compare(&first, &edited, &struts, &provenance(10))
    };
    let worse = run(0.6);
    assert_eq!(worse.untraced.len(), 1, "{worse:?}");
    assert_eq!(worse.untraced[0].first, Some(0.5));
    for ok in [0.4, 0.5, 0.5 + 1.0e-12] {
        assert!(run(ok).clean(), "severity {ok} is not worse than 0.5");
    }
}

/// **An old tip newly short is a regression though the summed shortfall falls**
/// (`lib.rs:1878-1906` keeps on the sum). First plan: tips 1 and 2 short by 0.5
/// each (sum 1.0). Edit: tip 1 short by 0.1, tip 3 short by 0.2 (sum 0.3).
#[test]
fn an_old_tip_newly_short_regresses_while_the_shortfall_sum_falls() {
    let first = finished(vec![
        warning(Class::ContactShort, 1, 0, 0.5),
        warning(Class::ContactShort, 2, 0, 0.5),
    ]);
    let edited = finished(vec![
        warning(Class::ContactShort, 1, 0, 0.1),
        warning(Class::ContactShort, 3, 0, 0.2),
    ]);
    let verdict = compare(&first, &edited, &field(4), &provenance(10));
    assert_eq!(verdict.untraced.len(), 1, "{verdict:?}");
    assert_eq!(verdict.untraced[0].key.a, 3);
}

/// **An unrelated neighbour thinner and over its run limit is untraced and
/// discards the edit.** Old member 5 gains an unbraced warning; new arm 12 is
/// nowhere near it (no trigger, no chain, no ground window).
#[test]
fn an_unrelated_neighbour_over_its_run_limit_discards_the_edit() {
    let first = finished(Vec::new());
    let edited = finished(vec![warning(Class::Unbraced, 5, 0, 1.2)]);
    let struts = field(13);
    let verdict = compare(&first, &edited, &struts, &provenance(10));
    assert!(verdict.discard(), "{verdict:?}");
    assert_eq!(verdict.untraced[0].key.class, Class::Unbraced);
}

/// **An old tip obstructed by a new arm's ball is traced to that arm**: the
/// collision pair names the new arm, so it is withdrawn and the edit stands.
#[test]
fn an_old_tip_obstructed_by_a_new_arm_is_traced_to_it() {
    let first = finished(Vec::new());
    let edited = finished(vec![warning(Class::Collision, 4, 12, 0.07)]);
    let verdict = compare(&first, &edited, &field(13), &provenance(10));
    assert!(!verdict.discard());
    assert_eq!(verdict.withdraw, BTreeSet::from([12]));
    assert_eq!(verdict.traced.len(), 1);
    assert_eq!(verdict.traced[0].1, vec![12]);
}

/// **A warning on a new arm withdraws it**, whatever its class.
#[test]
fn a_warning_on_a_new_arm_withdraws_that_arm() {
    let first = finished(Vec::new());
    let edited = finished(vec![
        warning(Class::ContactShort, 11, 0, 0.3),
        warning(Class::Unbraced, 12, 0, 1.1),
    ]);
    let verdict = compare(&first, &edited, &field(13), &provenance(10));
    assert!(!verdict.discard());
    assert_eq!(verdict.withdraw, BTreeSet::from([11, 12]));
    assert_eq!(verdict.new_arm_warnings.len(), 2);
}

/// The three structural links, one at a time: the trigger host, a member on the
/// new arm's chain of landings and seats, and (ground lost) a tip whose window
/// holds the new arm's ball. Each traces; with none of them the same warning is
/// untraced.
/// SABOTAGE: drop any one link in `compare` and its assertion goes red.
#[test]
fn the_structural_links_trace_a_regression_to_the_new_arm() {
    let first = finished(Vec::new());
    let on_chain = |warn: Warning, mut prov: Provenance, struts: &[SweepStrut]| {
        prov.n_c = 10;
        compare(&first, &finished(vec![warn]), struts, &prov)
    };
    let mut struts = field(12);
    // New arm 11 lands on arm 10, which lands on old member 6.
    struts[10].landing = Landing::Strut { id: 6, layer: 3 };
    struts[11].landing = Landing::Strut { id: 10, layer: 5 };
    let unrelated = warning(Class::LipBite, 8, 0, 0.1);
    assert!(on_chain(unrelated, Provenance::default(), &struts).discard(), "no link");
    let chain = on_chain(warning(Class::LipBite, 6, 0, 0.1), Provenance::default(), &struts);
    assert_eq!(chain.withdraw, BTreeSet::from([10, 11]), "{chain:?}");
    let mut trigger = Provenance::default();
    trigger.trigger_host.insert(11, 8);
    let by_trigger = on_chain(unrelated, trigger, &struts);
    assert_eq!(by_trigger.withdraw, BTreeSet::from([11]), "{by_trigger:?}");
    let mut ground = Provenance::default();
    ground.near_new.insert(8, vec![10]);
    let by_ground = on_chain(warning(Class::GroundLost, 8, 0, 0.0), ground, &struts);
    assert_eq!(by_ground.withdraw, BTreeSet::from([10]), "{by_ground:?}");
}

/// Global quantities: a load that rises beyond float noise, a count that rises,
/// and a flag that turns on each discard the edit; the same numbers or lower do
/// not. Never blamed on an arm.
#[test]
fn a_global_regression_discards_the_edit_and_is_never_blamed() {
    let first = Finished::from_parts(Vec::new(), Globals { uncarried_load_mm2: 2.0, landings_unrooted: 1, ..Globals::default() });
    let run = |globals: Globals| compare(&first, &Finished::from_parts(Vec::new(), globals), &field(2), &provenance(1));
    assert!(run(Globals { uncarried_load_mm2: 2.0, landings_unrooted: 1, ..Globals::default() }).clean());
    assert!(run(Globals { uncarried_load_mm2: 1.0, ..Globals::default() }).clean());
    let more_load = run(Globals { uncarried_load_mm2: 2.1, landings_unrooted: 1, ..Globals::default() });
    assert!(more_load.discard() && more_load.global[0].field == "uncarried_load_mm2");
    let more_count = run(Globals { uncarried_load_mm2: 2.0, landings_unrooted: 2, ..Globals::default() });
    assert!(more_count.discard() && more_count.global[0].field == "landings_unrooted");
    let flag = run(Globals { uncarried_load_mm2: 2.0, landings_unrooted: 1, still_congested: true, ..Globals::default() });
    assert!(flag.discard());
}

/// **The same field gives the same list and the same verdict in any order**: the
/// warnings are sorted by key and merged by worst severity, and the verdict is a
/// function of the sorted lists (stable at 1, 4 and 12 threads by construction).
#[test]
fn warnings_and_verdicts_do_not_depend_on_input_order() {
    let rows = vec![
        warning(Class::Collision, 4, 12, 0.07),
        warning(Class::ContactShort, 2, 0, 0.5),
        warning(Class::ContactShort, 2, 0, 0.9),
        warning(Class::Unbraced, 5, 0, 1.2),
        warning(Class::Landing, 3, 4, 0.0),
    ];
    let mut reversed = rows.clone();
    reversed.reverse();
    let mut rotated = rows.clone();
    rotated.rotate_left(2);
    let canon = canonical(rows.clone());
    assert_eq!(canon, canonical(reversed.clone()));
    assert_eq!(canon, canonical(rotated.clone()));
    assert_eq!(canon.len(), 4, "equal keys merge: {canon:?}");
    assert_eq!(canon.iter().find(|w| w.key.class == Class::ContactShort).map(|w| w.severity), Some(0.9));
    let struts = field(13);
    let first = finished(Vec::new());
    let verdicts: Vec<_> = [rows, reversed, rotated]
        .into_iter()
        .map(|order| compare(&first, &finished(order), &struts, &provenance(10)))
        .collect();
    assert_eq!(verdicts[0], verdicts[1]);
    assert_eq!(verdicts[0], verdicts[2]);
}

// --------------------------------------------------------------- withdrawal

/// A field of 2 old members and 4 new arms: 2 on the plate, 3 landing on 2, 4
/// landing on 3, and 5 on the plate. Withdrawing 3 takes 4 with it (the closure),
/// the remap is `drop_members`' own, the prefix is bit-equal, nothing falls to
/// the plate, and provenance and refused keys are remapped.
/// SABOTAGE: a closure that is not transitive leaves 4 resting on a dropped 3
/// (`orphaned_by` names it, and `drop_members` would drop it to the plate).
#[test]
fn a_withdrawal_takes_what_rests_on_it_and_remaps_ids_landings_and_provenance() {
    let mut struts = field(6);
    struts[3].landing = Landing::Strut { id: 2, layer: 4 };
    struts[4].landing = Landing::Strut { id: 3, layer: 4 };
    struts[4].seat_on_strut = Some(1);
    struts[5].seat_on_strut = Some(4);
    let seeds = BTreeSet::from([3]);
    let closure = withdrawal_closure(&struts, &seeds, 2);
    assert_eq!(closure.ids, BTreeSet::from([3, 4, 5]), "{closure:?}");
    assert!(closure.old_reached.is_empty());
    assert_eq!(orphaned_by(&struts, &closure.ids), Vec::<u32>::new());
    assert_eq!(orphaned_by(&struts, &seeds), vec![4], "without the closure 4 would fall to the plate");

    let remap = remap_for(struts.len(), &closure.ids);
    let mut dropped = struts.clone();
    let doomed: Vec<bool> = (0..struts.len()).map(|i| closure.ids.contains(&u32::try_from(i).unwrap())).collect();
    let real = crate::drop_members(&mut dropped, &doomed);
    assert_eq!(remap, real, "the pure remap is drop_members' own");
    assert_eq!(dropped.len(), 3);
    assert_eq!(&dropped[..2], &struts[..2], "the first plan's prefix is bit-equal");
    assert_eq!(dropped[2], struts[2]);

    let mut provenance = Provenance { n_c: 2, ..Provenance::default() };
    provenance.trigger_host = BTreeMap::from([(2, 0), (3, 2), (4, 1)]);
    provenance.near_new = BTreeMap::from([(1, vec![3, 4, 2])]);
    let moved = provenance.remapped(&remap);
    assert_eq!(moved.n_c, 2);
    assert_eq!(moved.trigger_host, BTreeMap::from([(2, 0)]), "dropped arms lose their rows");
    assert_eq!(moved.near_new, BTreeMap::from([(1, vec![2])]));
    // A survivor whose trigger host is dropped keeps its row with no host.
    provenance.trigger_host.insert(2, 3);
    assert_eq!(provenance.remapped(&remap).trigger_host.get(&2), Some(&u32::MAX));

    let refused = vec![(5, 7), (3, 1), (2, 9), (0, 2)];
    assert_eq!(remap_refused(&refused, &remap), vec![(0, 2), (2, 9)]);
}

/// A closure that reaches an OLD member is reported, so the transaction can
/// discard instead of dropping a first-plan member.
#[test]
fn a_closure_that_reaches_an_old_member_says_so() {
    let mut struts = field(5);
    struts[1].landing = Landing::Strut { id: 3, layer: 4 };
    let closure = withdrawal_closure(&struts, &BTreeSet::from([3]), 2);
    assert_eq!(closure.old_reached, vec![1], "{closure:?}");
}

// ------------------------------------------------------- overgrown merges

/// **The capacity predicate covers inserted arms the repair filters skip**
/// (G9, astra round 2 finding 2). Trunk 0 stands at the registered maximum tip
/// radius (0.5 here). Arm 1 merged into it (an up-sweep arm with a recorded
/// fallback): both the predicate and `unmerge_overgrown` see it. Arm 2 was
/// inserted by the load (`inserted_by_load`, no fallback): the predicate sees
/// it, `unmerge_overgrown` leaves it alone, which is exactly the hole.
/// SABOTAGE: adding `!inserted_by_load` to `overgrown_merges` turns the second
/// assertion red.
#[test]
fn the_capacity_predicate_sees_inserted_arms_the_repair_pass_skips() {
    let mut struts = field(3);
    struts[0].radius_mm = vec![0.5; 21];
    struts[1].landing = Landing::Strut { id: 0, layer: 10 };
    struts[1].path = vec![0];
    struts[1].straight_fallback = Some((Landing::Plate, 0));
    struts[2].landing = Landing::Strut { id: 0, layer: 12 };
    struts[2].inserted_by_load = true;
    let found = overgrown_merges(&struts, 0.5);
    assert_eq!(found.iter().map(|m| m.child).collect::<Vec<_>>(), vec![1, 2], "{found:?}");
    assert!(overgrown_merges(&struts, 0.6).is_empty(), "under the ceiling nothing is overgrown");
    // The repair pass: only arm 1 is eligible.
    let mut repaired = struts.clone();
    let undone = crate::unmerge_overgrown(&mut repaired, 0.5);
    assert_eq!(undone.undone, 1, "only the eligible arm is put back");
    assert_eq!(repaired[2].landing, struts[2].landing, "the inserted arm is not touched");
    // Read-only: the predicate changed nothing.
    assert_eq!(struts[1].landing, Landing::Strut { id: 0, layer: 10 });
}

// ------------------------------------------------------ joint certificate

fn headed(id: u32, state: JointCertState) -> SweepStrut {
    let mut strut = member(id, Landing::Plate, None, 0.2);
    let certified = |drawn_max: f64, under: usize, worst: Option<(f64, f64)>| crate::heads::CertifiedHead {
        profile: crate::joint_cert::CertifiedProfile {
            knots: vec![[0.3, 0.1], [0.6, 0.2]],
            joint_radius_mm: 0.2,
            k_demand_mm: 0.1,
            certified_max: 1.0,
        },
        slots_certified: 1,
        slots_kept: 0,
        slots_pinned: 0,
        drawn_max,
        slots_under_need: under,
        worst_under_need: worst,
        today_radius_mm: Vec::new(),
    };
    let (certified_head, old_law) = match state {
        JointCertState::Certified { drawn_max, slots_under_need, worst_need_over_drawn } => (
            Some(certified(drawn_max, slots_under_need, worst_need_over_drawn.map(|ratio| (ratio * 0.1, 0.1)))),
            None,
        ),
        JointCertState::OldLaw(reason) => (None, Some(reason)),
        JointCertState::NotSized => (None, None),
    };
    strut.head = Some(crate::heads::Head {
        contact_mm: [0.0, 0.0, 1.0],
        unit: [0.0, 0.0, -1.0],
        joint_mm: [0.0, 0.0, 0.4],
        k_joint: 4,
        joint_cell: 0,
        joint_radius_mm: 0.2,
        planned_tip_radius_mm: 0.1,
        gap_mm: 0.3,
        cos_lean_max: 0.5373,
        cells: Vec::new(),
        ball_allowance_mm: 0.0,
        link_mm: 0.0,
        drawn_straight: false,
        law_planned: crate::heads::HeadLaw::envelope(0.5373),
        law_drawn: None,
        ledger_radius_mm: Vec::new(),
        certified: certified_head,
        old_law,
    });
    strut
}

/// **The joint certificate is read head by head.** The counts of the two fields
/// are equal (one certified head under its need) and the single "worst member"
/// is the same number, yet the heads are different ones: the keyed rows differ,
/// and the swap is a regression on the newly under-need head.
#[test]
fn the_joint_certificate_swap_that_equal_counts_hide_is_a_regression() {
    let under = |ratio: f64| JointCertState::Certified {
        drawn_max: 1.0,
        slots_under_need: 1,
        worst_need_over_drawn: Some(ratio),
    };
    let clean = JointCertState::Certified { drawn_max: 1.0, slots_under_need: 0, worst_need_over_drawn: None };
    let one = vec![headed(0, under(1.5)), headed(1, clean), headed(2, JointCertState::OldLaw(OldLawReason::Shared))];
    let swapped = vec![headed(0, clean), headed(1, under(1.5)), headed(2, JointCertState::OldLaw(OldLawReason::Shared))];
    let counts = |struts: &[SweepStrut]| crate::heads::JointCertificate::of(struts);
    let (c1, c2) = (counts(&one), counts(&swapped));
    assert_eq!((c1.heads_under_need, c1.certified, c1.old_law_shared), (c2.heads_under_need, c2.certified, c2.old_law_shared));
    // The rows are a faithful keyed copy of the counts.
    let rows = crate::heads::joint_certificate_rows(&one);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows.iter().filter(|r| matches!(r.state, JointCertState::Certified { .. })).count(), c1.certified);
    assert_eq!(rows.iter().filter(|r| matches!(r.state, JointCertState::OldLaw(OldLawReason::Shared))).count(), c1.old_law_shared);
    let warnings_of = |struts: &[SweepStrut]| {
        let mut out = Vec::new();
        let census = super::Census {
            collisions: Vec::new(),
            overgrown: Vec::new(),
            joint_rows: crate::heads::joint_certificate_rows(struts),
            finals: super::FinalChecks {
                landings: crate::landing_check::LandingCheck::default(),
                unbraced: Vec::new(),
                braces_thicker_than_a_post: 0,
                shake: None,
                escape: None,
            },
            ground_lost: Vec::new(),
        };
        super::classify_census(&blank_report(), &census, &mut out, &mut Globals::default());
        out
    };
    let first = finished(warnings_of(&one));
    let edited = finished(warnings_of(&swapped));
    assert_eq!(first.warnings.len(), edited.warnings.len(), "the swap keeps the count");
    let verdict = compare(&first, &edited, &swapped, &provenance(9));
    assert!(
        verdict.untraced.iter().any(|r| r.key.class == Class::JointUnderNeed && r.key.a == 1),
        "head 1 is newly under its need: {verdict:?}"
    );
}

/// The keyed landing census names WHO is defective: two checks with the same
/// counts and different members have different keys, in sorted order, and a
/// duplicate listing collapses.
#[test]
fn the_keyed_landing_census_names_the_member_not_the_count() {
    use crate::landing_check::{LandingCheck, LandingDefect};
    let first = LandingCheck { unrooted: vec![3, 3, 9], model_feet_partial: vec![(4, 700)], ..LandingCheck::default() };
    let edited = LandingCheck { unrooted: vec![9, 5], model_feet_partial: vec![(4, 700)], ..LandingCheck::default() };
    assert_eq!(first.unrooted.len(), edited.unrooted.len() + 1);
    assert_eq!(
        first.keyed(),
        vec![(LandingDefect::Unrooted, 3, 0), (LandingDefect::Unrooted, 9, 0), (LandingDefect::FootPartial, 4, 700)]
    );
    assert_eq!(
        edited.keyed(),
        vec![(LandingDefect::Unrooted, 5, 0), (LandingDefect::Unrooted, 9, 0), (LandingDefect::FootPartial, 4, 700)]
    );
}

// ------------------------------------------------------------ ground lost

/// **A shrunk or moved contact that leaves an island over air is ground lost,
/// and the new arm near it is named.** A floating 0.8 mm island held by a
/// 0.4 mm ball under its middle (old tip 3). In the edit the ball is moved 3 mm
/// away (a new arm 12's ball stands beside the old one): the cells the old ball
/// held no longer join ground. The same move under an island that stands on a
/// slab keeps the ground. The compare-level consequence: `GroundLost{3}` is a
/// new key, traced to the new arm 12 by `near_new`.
#[test]
fn ground_lost_names_the_old_tip_and_the_new_arm_near_it() {
    use crate::footprint::ContactBall;
    use plinth_support::perception::{slice_stack, SliceStackLimits};
    let ball = |x: f64, y: f64, r: f64| ContactBall {
        centre_mm: [x, y, 0.95],
        radius_mm: r,
        planned_radius_mm: r,
        normal: [0.0, 0.0, -1.0],
    };
    let lost_with = |slab: bool| {
        let mut boxes = vec![([0.0, 0.0, 1.0], [0.8, 0.8, 1.5])];
        if slab {
            boxes.push(([-0.5, -0.5, 0.0], [1.3, 1.3, 1.0]));
        }
        let (verts, tris) = crate::member_clearance::boxes_mesh(&boxes);
        let mesh = plinth_mesh::Mesh::new(verts, tris).expect("closed boxes");
        let planes: Vec<f64> = (0..40).map(|layer| (f64::from(layer) + 0.5) * 0.05).collect();
        let slices = slice_stack(&mesh, &planes, SliceStackLimits::production()).expect("slices");
        let grid = lay_out_at(-1.0, -1.0, 0.05, 120, 120);
        let old = [(3_u32, ball(0.4, 0.4, 0.4)), (4, ball(2.0, 2.0, 0.2))];
        let edited = [(3_u32, ball(3.0, 3.0, 0.4)), (4, ball(2.0, 2.0, 0.2)), (12, ball(3.0, 3.0, 0.3))];
        super::ground_lost(&grid, &slices, 0.0, 0.05, 10, &old, &edited)
    };
    let free = lost_with(false);
    assert_eq!(free.len(), 1, "{free:?}");
    assert_eq!(free[0], (3, vec![12]));
    assert!(lost_with(true).is_empty(), "an island on a slab is carried from below");
    let first = finished(Vec::new());
    let edited = finished(vec![warning(Class::GroundLost, 3, 0, 0.0)]);
    let mut prov = provenance(10);
    prov.near_new.insert(3, vec![12]);
    let verdict = compare(&first, &edited, &field(13), &prov);
    assert_eq!(verdict.withdraw, BTreeSet::from([12]), "{verdict:?}");
}

// -------------------------------------------------------- final predicates

/// A plate post of 80 layers (4.0 mm) at radius `radius`, on a grid with
/// 0.05 mm layers, and the law that limits an unbraced run to 50 radii.
fn plate_post(grid: &crate::grid::Grid, radius: f64) -> SweepStrut {
    let (row, col) = (100, 100);
    let mut post = member(0, Landing::Plate, None, radius);
    post.cell = grid.index(row, col);
    post.center_mm = [grid.center_x_mm(col), grid.center_y_mm(row)];
    post.k_top = 80;
    post.radius_mm = vec![radius; 80];
    post.carried_mm2 = vec![0.0; 80];
    post
}

/// **The pre-seat diagnostic passes and the final check fails** (astra round 2
/// finding 3). A 4.0 mm post at radius 0.20 mm has an unbraced run of about
/// 3 mm against a 10 mm limit (50 radii): clean, and the run the sweep published
/// (`publish_runs` keeps the limit it measured) says so. The final head resize
/// then shrinks the post to 0.05 mm: its limit is 2.5 mm and the same run is now
/// over it. The published runs still read the old limit; `final_checks`
/// recomputes it and names the member.
/// SABOTAGE: reading the stored runs in `final_checks` instead of recomputing
/// turns the last assertion red.
#[test]
fn a_late_head_resize_passes_the_pre_seat_diagnostic_and_fails_the_final_check() {
    let grid = lay_out_at(0.0, 0.0, 0.05, 200, 200);
    let env = |grid| FinalEnv {
        grid,
        plate_z_mm: 0.0,
        layer_height_mm: 0.05,
        law: SlendernessLaw::new(50.0, 0.05, 1.0, 32.5),
        taper: crate::adapter::LoadStepTaper::OFF,
        shake_pressure_kpa: None,
        escape: None,
    };
    let mut struts = vec![plate_post(&grid, 0.20)];
    let published = crate::slenderness::unbraced_runs_drawn(
        &grid, &struts, 0.05, &SlendernessLaw::new(50.0, 0.05, 1.0, 32.5), crate::adapter::LoadStepTaper::OFF,
    );
    assert!(published.iter().all(|run| !run.over_limit()), "anti-vacuity: clean before the resize: {published:?}");
    assert!(final_checks(&env(&grid), &struts).unbraced.is_empty());
    // The late resize.
    struts[0].radius_mm = vec![0.05; 80];
    let stale = published.iter().filter(|run| run.over_limit()).count();
    assert_eq!(stale, 0, "the published diagnostic still passes");
    let last = final_checks(&env(&grid), &struts);
    assert_eq!(last.unbraced.len(), 1, "{:?}", last.unbraced);
    assert_eq!(last.unbraced[0].member, 0);
    assert!(last.unbraced[0].length_mm > last.unbraced[0].limit_mm);
}

// ------------------------------------------------------------ real report

/// A real finished plan: a 6 x 4 x 1.5 mm slab lifted 5 mm, planned by the
/// production sweep. The report classes without a panic, the census measures,
/// a plan compared with itself is clean, and the final predicates agree with
/// the history counters on a field nothing mutated late.
fn slab_plan() -> (crate::SweepPlan, plinth_core::LayerPlan, crate::SweepPolicy) {
    let mesh = plinth_mesh::make_box([6.0, 4.0, 1.5], [0.0, 0.0, 0.0]).expect("a box");
    let height = *plinth_core::m0_constants()
        .expect("registry")
        .evidence()
        .layer_height_default_mm()
        .value();
    let min_z = mesh.verts().iter().map(|v| v[2]).fold(f64::INFINITY, f64::min);
    let max_z = mesh.verts().iter().map(|v| v[2]).fold(f64::NEG_INFINITY, f64::max);
    let plate_z = min_z - 5.0;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a layer count is a finite positive quotient of model-scale lengths"
    )]
    let count = (((max_z - plate_z) / height).ceil() as usize).saturating_add(1);
    let layers = plinth_core::LayerPlan::new(plate_z, vec![height; count]).expect("a uniform plan");
    let policy = crate::SweepPolicy::from_registry().expect("the registry supplies the sweep numbers");
    let plan = crate::run_sweep(&mesh, &layers, &policy).expect("the sweep emits and warns");
    (plan, layers, policy)
}

fn blank_report() -> crate::report::SweepRunReport {
    slab_plan().0.report
}

#[test]
fn a_real_plan_classes_measures_and_compares_clean_with_itself() {
    let (plan, layers, policy) = slab_plan();
    let env = FinalEnv::of_plan(&plan, &policy, None);
    let census = super::Census::measure(&plan, &layers, &env, policy.tip_radius_max_mm);
    let once = Finished::of(&plan.report, &plan.struts, &census);
    let again = Finished::of(&plan.report, &plan.struts, &census);
    assert_eq!(once, again, "deterministic");
    let verdict = compare(&once, &again, &plan.struts, &provenance(u32::try_from(plan.struts.len()).unwrap()));
    assert!(verdict.clean(), "{verdict:?}");
    // On a field no late mutation touched, the final unbraced runs are the
    // history's.
    let history: BTreeSet<u32> = plan.report.slenderness_lane.warnings.iter().map(|w| w.strut_id).collect();
    let last: BTreeSet<u32> = census.finals.unbraced.iter().map(|run| run.member).collect();
    assert_eq!(history, last);
    assert!(!plan.struts.is_empty());
    // Keys are sorted.
    assert!(once.warnings.windows(2).all(|pair| pair[0].key < pair[1].key));
}
