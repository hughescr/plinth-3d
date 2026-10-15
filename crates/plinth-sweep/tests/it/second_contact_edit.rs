//! **The second contact as a checked edit of the first plan** (SC-3,
//! `plinth-evidence/perf-2026-10-14/design/SC-DESIGN.md` §3), through the public
//! entry [`run_sweep_with_second_contact_seeded`].
//!
//! The decisions of the transaction (every exit, withdrawal and remap, the one
//! fallback, the sabotage of the compare, the blame and the dispatch) are tested
//! on scripted and real fields in `src/second_contact/edit/tests.rs`. This file
//! holds the claims that need the whole entry:
//!
//! * mode `Replan` is today's chooser: the seed is not read and the outcome says
//!   the edit was not tried;
//! * mode `Edit` without a usable seed returns the first plan itself;
//! * mode `EditThenReplan` without a usable seed is the re-plan, field for field;
//! * with the seed, an edit that is KEPT keeps every old member where the first
//!   plan put it and is no worse than the first plan on any key, and an edit that
//!   is thrown away leaves exactly the fallback's field.
//!
//! The first plan here has real short-tip rows injected on its six most loaded
//! tips (the fixture has no feature narrow enough to leave one short), so the
//! caps are real: they are the contacts a short tip "made", and the edit places
//! its carriers beside those tips.

use std::sync::{Arc, OnceLock};

use plinth_core::LayerPlan;
use plinth_mesh::{make_recessed_disc, Mesh};
use plinth_sweep::contact_growth::{
    ContactShort, EditExit, SecondContactMode, SecondContactResult, ShortReason,
};
use plinth_sweep::{
    run_sweep_with_holes_seeded, run_sweep_with_second_contact_seeded, EditSeed, StrutKind,
    SweepPlan, SweepPolicy,
};

use crate::harness;

struct Fixture {
    mesh: Mesh,
    layers: LayerPlan,
    policy: SweepPolicy,
    /// The first plan, with short-tip rows injected.
    first: SweepPlan,
    seed: Arc<EditSeed>,
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a small positive layer count"
)]
fn layer_plan(mesh: &Mesh) -> LayerPlan {
    let height = harness::layer_height_mm();
    let (min_z, max_z) = harness::z_span(mesh);
    let plate_z = min_z - harness::LIFT_MM;
    let layers = (((max_z - plate_z) / height).ceil() as usize).max(1);
    LayerPlan::new(plate_z, vec![height; layers]).expect("a uniform layer plan")
}

fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let mesh = make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96).expect("the Round 25 profile");
        let layers = layer_plan(&mesh);
        let policy = SweepPolicy::from_registry().expect("the registry");
        let (mut first, seed) =
            run_sweep_with_holes_seeded(&mesh, &layers, &policy, &[], &[]).expect("sweep");
        let mut loaded: Vec<(f64, usize)> = first
            .struts
            .iter()
            .enumerate()
            .filter(|(_, strut)| strut.kind == StrutKind::Arm)
            .filter_map(|(index, strut)| Some((*strut.carried_mm2.last()?, index)))
            .collect();
        loaded.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (load, index) in loaded.into_iter().take(6) {
            let need = load / policy.x_allowed;
            first.report.seat_contact.short.push(ContactShort {
                strut: index,
                real_mm2: 0.2 * need,
                planned_mm2: need,
                profile_disc_mm2: need,
                ball_radius_mm: 0.2,
                planned_ball_radius_mm: 0.2,
                reason: ShortReason::FeatureWidth,
            });
        }
        assert!(
            first.report.seat_contact.short.iter().all(|short| {
                short.real_mm2 >= first.grid.pitch_mm() * first.grid.pitch_mm()
            }),
            "every injected row is a contact that exists, so it caps"
        );
        Fixture { mesh, layers, policy, first, seed }
    })
}

fn run(mode: SecondContactMode, seed: Option<&EditSeed>) -> SweepPlan {
    let fixture = fixture();
    let policy = SweepPolicy { second_contact_mode: mode, ..fixture.policy };
    run_sweep_with_second_contact_seeded(
        fixture.first.clone(),
        seed,
        &fixture.mesh,
        &fixture.layers,
        &policy,
        &[],
        &[],
    )
    .expect("the second contact")
}

/// The field: every member, by complete `Debug` (equal text is equal bits).
fn members(plan: &SweepPlan) -> String {
    format!("{:?}", plan.struts)
}

#[test]
fn the_policy_knob_parses_and_defaults_to_replan() {
    assert_eq!(SecondContactMode::parse("replan"), Ok(SecondContactMode::Replan));
    assert_eq!(SecondContactMode::parse("edit"), Ok(SecondContactMode::Edit));
    assert_eq!(SecondContactMode::parse("edit-then-replan"), Ok(SecondContactMode::EditThenReplan));
    assert!(SecondContactMode::parse("Edit").is_err(), "a typo does not silently pick a mode");
    assert_eq!(SecondContactMode::default(), SecondContactMode::Replan);
    if std::env::var_os(SecondContactMode::ENV).is_none() {
        let policy = SweepPolicy::from_registry().expect("the registry");
        assert_eq!(policy.second_contact_mode, SecondContactMode::Replan);
        assert_eq!(policy.second_contact_withdraw_rounds, 2);
    }
}

/// The second-contact knobs are read after C, so a seed answers to any of them.
#[test]
fn a_seed_answers_to_every_second_contact_mode() {
    let fixture = fixture();
    for mode in [SecondContactMode::Replan, SecondContactMode::Edit, SecondContactMode::EditThenReplan] {
        for rounds in [0, 2, 5] {
            let policy = SweepPolicy {
                second_contact_mode: mode,
                second_contact_withdraw_rounds: rounds,
                ..fixture.policy
            };
            assert_eq!(
                fixture.seed.miss(
                    &fixture.mesh,
                    &fixture.layers,
                    &policy,
                    &[],
                    &[],
                    &fixture.first.report.sweep_digest
                ),
                None,
                "{mode:?} / {rounds} rounds"
            );
        }
    }
    // And a policy that DOES change the state at C is still refused.
    let mut other = fixture.policy;
    other.x_allowed += 1.0;
    assert!(fixture
        .seed
        .miss(&fixture.mesh, &fixture.layers, &other, &[], &[], &fixture.first.report.sweep_digest)
        .is_some());
}

#[test]
fn replan_mode_is_todays_chooser_and_never_reads_the_seed() {
    let fixture = fixture();
    let with_seed = run(SecondContactMode::Replan, Some(&fixture.seed));
    let without = run(SecondContactMode::Replan, None);
    assert_eq!(members(&with_seed), members(&without), "the seed was read in Replan mode");
    let outcome = &with_seed.report.seat_second_contact;
    assert_eq!(outcome.mode, SecondContactMode::Replan);
    assert!(outcome.attempted, "the re-plan ran");
    assert_eq!(outcome.edit.exit, EditExit::NotTried);
    assert!(!outcome.edit.attempted);
    assert!(matches!(outcome.result, SecondContactResult::First | SecondContactResult::Replanned));
    assert_eq!(outcome.tips_capped, 6);
}

#[test]
fn edit_mode_without_a_seed_returns_the_first_plan() {
    let fixture = fixture();
    let out = run(SecondContactMode::Edit, None);
    assert_eq!(members(&out), members(&fixture.first), "Edit returns P1");
    let outcome = &out.report.seat_second_contact;
    assert_eq!(outcome.edit.exit, EditExit::SeedMismatch);
    assert_eq!(outcome.result, SecondContactResult::First);
    assert!(!outcome.attempted, "no second plan was made");
}

#[test]
fn edit_then_replan_without_a_seed_is_the_replan() {
    let replan = run(SecondContactMode::Replan, None);
    let out = run(SecondContactMode::EditThenReplan, None);
    assert_eq!(members(&out), members(&replan), "today's re-plan and chooser, field for field");
    let outcome = &out.report.seat_second_contact;
    assert_eq!(outcome.edit.exit, EditExit::SeedMismatch);
    assert_eq!(outcome.mode, SecondContactMode::EditThenReplan);
    assert!(outcome.attempted);
}

/// With the seed: kept, or thrown away to exactly the fallback's field.
#[test]
fn an_edit_with_the_seed_is_kept_whole_or_discarded_to_the_fallback() {
    let fixture = fixture();
    let n_c = fixture.first.struts.len();
    let edit = run(SecondContactMode::Edit, Some(&fixture.seed));
    let outcome = &edit.report.seat_second_contact;
    let report = &outcome.edit;
    eprintln!(
        "SC3 fixture: exit {:?} ({}), inserted {}, trials {}, rounds {}, short {} -> {}, \
         shortfall {:.4} -> {:.4}, traced {:?}, untraced {:?}, global {:?}, new-arm warnings {}, \
         withdrawn {:?}, seconds {:.2}",
        report.exit,
        report.exit.word(),
        report.inserted,
        report.trials,
        report.rounds,
        report.first_short,
        report.edited_short,
        report.first_shortfall_mm2,
        report.edited_shortfall_mm2,
        report.traced_by_class,
        report.untraced_by_class,
        report.global_regressions,
        report.new_arm_warnings,
        report.withdrawn_by_class,
        report.total_seconds,
    );
    assert!(report.attempted && report.inserted > 0, "the caps placed carriers");
    assert!(report.seed_bytes > 0 && report.layer_cache_seconds >= 0.0);
    assert_eq!(report.arms_first, n_c);
    if report.exit == EditExit::Kept {
        assert_eq!(outcome.result, SecondContactResult::Edited);
        assert!(outcome.kept_second && outcome.attempted);
        assert!(report.kept_trial.is_some());
        assert!(edit.struts.len() > n_c, "the kept edit has its new arms");
        assert_eq!(outcome.first_arms, n_c);
        assert_eq!(outcome.second_arms, edit.struts.len());
        // An edit of P1: every old member keeps its seat, kind and landing.
        for (old, now) in fixture.first.struts.iter().zip(&edit.struts) {
            assert_eq!(
                (old.cell, old.k_top, old.kind, old.landing),
                (now.cell, now.k_top, now.kind, now.landing),
                "old member {} moved",
                old.id
            );
        }
        assert!(edit.struts[n_c..].iter().all(|strut| strut.inserted_by_load), "the tail is the edit's arms");
        assert!(report.edited_shortfall_mm2 < report.first_shortfall_mm2);
        assert!(report.edited_unbraced <= report.first_unbraced);
        assert!(edit.deferred_stage_shake.is_none(), "the kept plan's shake check was completed");
        assert!(edit.report.slenderness_lane.stage_shake.pressure_kpa > 0.0, "and ran");
    } else {
        assert_eq!(members(&edit), members(&fixture.first), "Edit discards to P1");
        assert_eq!(outcome.result, SecondContactResult::First);
        assert!(!outcome.attempted);
        // And EditThenReplan from the same discard is today's re-plan.
        let both = run(SecondContactMode::EditThenReplan, Some(&fixture.seed));
        let replan = run(SecondContactMode::Replan, None);
        assert_eq!(members(&both), members(&replan), "the discard dispatches to the re-plan");
        assert_eq!(both.report.seat_second_contact.edit.exit, report.exit);
    }
}

/// A first plan whose short-tip rows record contacts so large that the caps they
/// make ask for nothing beyond the area law, which P1 already meets: the
/// insertion phase places no arm.
fn no_insertion_first() -> SweepPlan {
    let fixture = fixture();
    let mut first = fixture.first.clone();
    first.report.seat_contact.short.clear();
    for index in 0..first.struts.len() {
        if first.struts[index].kind == StrutKind::Arm && first.report.seat_contact.short.len() < 4 {
            first.report.seat_contact.short.push(ContactShort {
                strut: index,
                real_mm2: 1000.0,
                planned_mm2: 1001.0,
                profile_disc_mm2: 1001.0,
                ball_radius_mm: 0.2,
                planned_ball_radius_mm: 0.2,
                reason: ShortReason::FeatureWidth,
            });
        }
    }
    first
}

fn run_on(first: SweepPlan, mode: SecondContactMode, seed: Option<&EditSeed>) -> SweepPlan {
    let fixture = fixture();
    let policy = SweepPolicy { second_contact_mode: mode, ..fixture.policy };
    run_sweep_with_second_contact_seeded(first, seed, &fixture.mesh, &fixture.layers, &policy, &[], &[])
        .expect("the second contact")
}

/// The no-insertion exit, in both edit modes: `Edit` returns P1, `EditThenReplan`
/// is today's re-plan and chooser, and both say why.
#[test]
fn no_insertion_discards_to_the_fallback_in_both_modes() {
    let fixture = fixture();
    let first = no_insertion_first();
    let edit = run_on(first.clone(), SecondContactMode::Edit, Some(&fixture.seed));
    assert_eq!(members(&edit), members(&first), "Edit returns P1");
    assert_eq!(edit.report.seat_second_contact.edit.exit, EditExit::NoInsertion);
    assert_eq!(edit.report.seat_second_contact.result, SecondContactResult::First);
    assert_eq!(edit.report.seat_second_contact.edit.inserted, 0);
    let both = run_on(first.clone(), SecondContactMode::EditThenReplan, Some(&fixture.seed));
    let replan = run_on(first, SecondContactMode::Replan, None);
    assert_eq!(members(&both), members(&replan), "EditThenReplan runs today's re-plan and chooser");
    assert_eq!(both.report.seat_second_contact.edit.exit, EditExit::NoInsertion);
    assert!(both.report.seat_second_contact.attempted, "the re-plan ran");
}
