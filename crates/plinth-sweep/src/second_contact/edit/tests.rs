//! The edit transaction's decisions (SC-3): every exit of [`drive`] on scripted
//! trials, the one fallback every discard goes through, and the sabotage of the
//! three decisions (the authoritative compare, the blame, the fallback
//! dispatch). The real field, its W isolation and its bytes are tested in
//! `tests/it/second_contact_edit.rs`.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    drive, fallback, keeps, Driven, Fallback, Judged, Metrics, Sabotage, World,
};
use crate::contact_growth::{EditExit, EditReport, SecondContactMode};
use crate::second_contact::{Class, Finished, Globals, Key, Provenance, Warning};
use crate::{Landing, SeatFace, StrutKind, SweepError, SweepStrut};

/// A member with the fields the transaction reads; everything else inert.
fn member(id: u32, landing: Landing) -> SweepStrut {
    SweepStrut {
        id,
        cell: 0,
        center_mm: [0.0, 0.0],
        kind: StrutKind::Arm,
        min_radius_mm: 0.0,
        thickened_mm: Vec::new(),
        foot_radius_mm: None,
        seat_on_strut: None,
        k_top: 20,
        k_bottom: 0,
        landing,
        interior: false,
        radius_mm: vec![0.2; 21],
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

/// One scripted trial: what finishing the working state "measured".
#[derive(Clone)]
struct Script {
    finished: Finished,
    metrics: Metrics,
}

/// A field of `n_old` old members and `n_new` new arms, all on the plate unless
/// a test re-lands some.
struct Mock {
    n_c: u32,
    work: Vec<SweepStrut>,
    scripts: Vec<Script>,
    trigger_host: BTreeMap<u32, u32>,
    /// The new arms near each old tip that lost ground in the scripted trials.
    near_new: BTreeMap<u32, Vec<u32>>,
    trials_run: usize,
    withdrawn: Vec<BTreeSet<u32>>,
    refuse_withdraw: bool,
}

impl Mock {
    fn new(n_old: u32, n_new: u32, scripts: Vec<Script>) -> Self {
        Self {
            n_c: n_old,
            work: (0..n_old + n_new).map(|id| member(id, Landing::Plate)).collect(),
            scripts,
            trigger_host: BTreeMap::new(),
            near_new: BTreeMap::new(),
            trials_run: 0,
            withdrawn: Vec::new(),
            refuse_withdraw: false,
        }
    }

    fn land(&mut self, id: u32, on: u32) {
        self.work[id as usize].landing = Landing::Strut { id: on, layer: 3 };
    }
}

impl World for Mock {
    type Plan = Vec<SweepStrut>;

    fn n_c(&self) -> u32 {
        self.n_c
    }

    fn struts_of<'a>(&'a self, plan: &'a Vec<SweepStrut>) -> &'a [SweepStrut] {
        plan
    }

    fn work_struts(&self) -> &[SweepStrut] {
        &self.work
    }

    fn trial(&mut self, _trial: usize, _report: &mut EditReport) -> Result<Judged<Vec<SweepStrut>>, SweepError> {
        let script = self.scripts[self.trials_run.min(self.scripts.len() - 1)].clone();
        self.trials_run += 1;
        Ok(Judged {
            plan: self.work.clone(),
            finished: script.finished,
            metrics: script.metrics,
            provenance: Provenance {
                n_c: self.n_c,
                trigger_host: self.trigger_host.clone(),
                near_new: self.near_new.clone(),
            },
        })
    }

    fn withdraw(&mut self, doomed: &BTreeSet<u32>, _report: &mut EditReport) -> Result<bool, SweepError> {
        if self.refuse_withdraw {
            return Ok(false);
        }
        let flags: Vec<bool> = (0..self.work.len())
            .map(|index| doomed.contains(&u32::try_from(index).unwrap()))
            .collect();
        let remap = crate::drop_members(&mut self.work, &flags);
        let provenance = Provenance { n_c: self.n_c, trigger_host: self.trigger_host.clone(), near_new: BTreeMap::new() }
            .remapped(&remap);
        self.trigger_host = provenance.trigger_host;
        self.withdrawn.push(doomed.clone());
        Ok(true)
    }
}

const FIRST: Metrics = Metrics { short: 3, shortfall_mm2: 1.0, unbraced: 1, unbraced_ge2: 0, unbraced_max: 1.2 };
const BETTER: Metrics = Metrics { short: 1, shortfall_mm2: 0.4, unbraced: 1, unbraced_ge2: 0, unbraced_max: 1.2 };

fn first_field() -> Finished {
    // Old tip 1 is short in P1, and old member 0 has an unbraced run.
    finished(vec![
        warning(Class::ContactShort, 1, 0, 0.5),
        warning(Class::Unbraced, 0, 0, 1.2),
    ])
}

fn clean_better() -> Script {
    Script {
        finished: finished(vec![
            warning(Class::ContactShort, 1, 0, 0.1),
            warning(Class::Unbraced, 0, 0, 1.2),
        ]),
        metrics: BETTER,
    }
}

fn with(extra: Warning) -> Script {
    let mut warnings = vec![warning(Class::ContactShort, 1, 0, 0.1), warning(Class::Unbraced, 0, 0, 1.2)];
    warnings.push(extra);
    Script { finished: finished(warnings), metrics: BETTER }
}

fn run(mock: &mut Mock, rounds: usize, sabotage: Sabotage) -> (Driven<Vec<SweepStrut>>, EditReport) {
    let mut report = EditReport::default();
    let driven = drive(mock, &first_field(), &FIRST, rounds, sabotage, &mut report).expect("no cancel");
    (driven, report)
}

fn exit_of(driven: &Driven<Vec<SweepStrut>>) -> Option<EditExit> {
    match driven {
        Driven::Discarded(exit) => Some(*exit),
        Driven::Kept { .. } => None,
    }
}

// ------------------------------------------------------------- the exits

#[test]
fn a_clean_better_edit_is_kept_at_trial_zero() {
    let mut mock = Mock::new(4, 3, vec![clean_better()]);
    let (driven, report) = run(&mut mock, 2, Sabotage::NONE);
    assert!(matches!(driven, Driven::Kept { trial: 0, .. }));
    assert_eq!((report.trials, report.rounds, report.withdrawn), (1, 0, 0));
    assert!(mock.withdrawn.is_empty());
    assert_eq!(report.edited_short, 1);
}

#[test]
fn a_clean_edit_that_does_not_help_is_no_gain() {
    // The shortfall is not strictly lower.
    let mut equal = clean_better();
    equal.metrics = FIRST;
    let (driven, _) = run(&mut Mock::new(4, 3, vec![equal]), 2, Sabotage::NONE);
    assert_eq!(exit_of(&driven), Some(EditExit::NoGain));
    // The shortfall falls and an unbraced run is added: today's rule refuses it.
    let mut longer = clean_better();
    longer.metrics = Metrics { unbraced: 2, ..BETTER };
    let (driven, _) = run(&mut Mock::new(4, 3, vec![longer]), 2, Sabotage::NONE);
    assert_eq!(exit_of(&driven), Some(EditExit::NoGain));
    assert!(keeps(&FIRST, &BETTER));
    assert!(!keeps(&FIRST, &FIRST));
}

#[test]
fn a_global_regression_discards_and_nothing_is_withdrawn() {
    let mut script = clean_better();
    script.finished = Finished::from_parts(
        script.finished.warnings.clone(),
        Globals { uncarried_load_mm2: 0.5, ..Globals::default() },
    );
    let mut mock = Mock::new(4, 3, vec![script]);
    let (driven, report) = run(&mut mock, 2, Sabotage::NONE);
    assert_eq!(exit_of(&driven), Some(EditExit::Global));
    assert!(mock.withdrawn.is_empty(), "a global regression is never blamed");
    assert_eq!(report.global_regressions, vec!["uncarried_load_mm2".to_owned()]);
}

#[test]
fn an_old_member_that_got_worse_for_no_linked_reason_is_untraced() {
    // Old member 2 gains an unbraced run; no new arm stands on its chain, is
    // hosted by it, or sits near it.
    let mut mock = Mock::new(4, 3, vec![with(warning(Class::Unbraced, 2, 0, 1.5))]);
    let (driven, report) = run(&mut mock, 2, Sabotage::NONE);
    assert_eq!(exit_of(&driven), Some(EditExit::Untraced));
    assert!(mock.withdrawn.is_empty());
    assert_eq!(report.untraced_by_class.get("Unbraced"), Some(&1));
}

#[test]
fn a_closure_that_reaches_an_old_member_discards() {
    // Old member 3 lands on new arm 5; arm 5 warns, so withdrawing it would drop
    // member 3 to the plate.
    let mut mock = Mock::new(4, 3, vec![with(warning(Class::ContactShort, 5, 0, 0.2))]);
    mock.land(3, 5);
    let (driven, _) = run(&mut mock, 2, Sabotage::NONE);
    assert_eq!(exit_of(&driven), Some(EditExit::ClosureHitsOld));
    assert!(mock.withdrawn.is_empty());
    // And a world that refuses a withdrawal (a survivor would fall to the plate)
    // ends the same way.
    let mut refuse = Mock::new(4, 3, vec![with(warning(Class::ContactShort, 5, 0, 0.2))]);
    refuse.refuse_withdraw = true;
    let (driven, _) = run(&mut refuse, 2, Sabotage::NONE);
    assert_eq!(exit_of(&driven), Some(EditExit::ClosureHitsOld));
}

#[test]
fn a_withdrawal_that_takes_every_new_arm_leaves_nothing() {
    let mut script = clean_better();
    for arm in 4..7 {
        script.finished.warnings.push(warning(Class::ContactShort, arm, 0, 0.2));
    }
    script.finished = Finished::from_parts(script.finished.warnings.clone(), Globals::default());
    let mut mock = Mock::new(4, 3, vec![script]);
    let (driven, _) = run(&mut mock, 2, Sabotage::NONE);
    assert_eq!(exit_of(&driven), Some(EditExit::Empty));
    assert!(mock.withdrawn.is_empty());
}

#[test]
fn a_trial_that_still_regresses_at_the_last_round_is_the_budget() {
    // Every trial warns on the arm at id 5.
    let mut mock = Mock::new(4, 4, vec![with(warning(Class::ContactShort, 5, 0, 0.2))]);
    let (driven, report) = run(&mut mock, 2, Sabotage::NONE);
    assert_eq!(exit_of(&driven), Some(EditExit::Budget));
    assert_eq!((report.trials, report.rounds), (3, 2), "R = 2 rounds, 3 trials");
    assert_eq!(mock.work.len(), 4 + 4 - 2, "two arms withdrawn, one per round");
    // R = 0: the first trial is the last.
    let mut none = Mock::new(4, 4, vec![with(warning(Class::ContactShort, 5, 0, 0.2))]);
    let (driven, report) = run(&mut none, 0, Sabotage::NONE);
    assert_eq!(exit_of(&driven), Some(EditExit::Budget));
    assert_eq!((report.trials, report.rounds), (1, 0));
}

// ----------------------------------------------------------- withdrawals

/// Four new arms (ids 4..8); arm 6 lands on arm 5; arm 5 fails.
fn middle_tail() -> Mock {
    let mut mock = Mock::new(
        4,
        4,
        vec![with(warning(Class::ContactShort, 5, 0, 0.2)), clean_better()],
    );
    mock.land(6, 5);
    mock.trigger_host = BTreeMap::from([(4, 0), (5, 1), (6, 1), (7, 2)]);
    mock
}

#[test]
fn a_middle_tail_withdrawal_takes_its_dependants_and_is_kept_at_trial_one() {
    let mut mock = middle_tail();
    let before: Vec<SweepStrut> = mock.work[..4].to_vec();
    let (driven, report) = run(&mut mock, 2, Sabotage::NONE);
    assert!(matches!(driven, Driven::Kept { trial: 1, .. }));
    assert_eq!(mock.withdrawn, vec![BTreeSet::from([5, 6])], "arm 5 and the arm that lands on it");
    assert_eq!((report.trials, report.rounds, report.withdrawn), (2, 1, 2));
    assert_eq!(report.withdrawn_by_class.get("ContactShort"), Some(&1));
    assert_eq!(mock.work.len(), 6);
    assert_eq!(&mock.work[..4], &before[..], "the old prefix is bit-equal");
    assert!(mock.work.iter().enumerate().all(|(i, s)| s.id as usize == i), "ids are dense");
    assert!(
        mock.work.iter().all(|s| !matches!(s.landing, Landing::Strut { .. })),
        "no lander was left pointing at a dropped member"
    );
    // The survivors keep their trigger hosts at their new ids: 4 -> 4, 7 -> 5.
    assert_eq!(mock.trigger_host, BTreeMap::from([(4, 0), (5, 2)]));
}

#[test]
fn a_passing_trial_at_the_last_round_is_emitted_not_budgeted() {
    let mut mock = middle_tail();
    let (driven, report) = run(&mut mock, 1, Sabotage::NONE);
    assert!(matches!(driven, Driven::Kept { trial: 1, .. }), "R = 1 and trial 1 passes");
    assert_eq!((report.trials, report.rounds), (2, 1));
}

#[test]
fn an_old_member_regression_with_a_structural_link_withdraws_the_linked_arms() {
    // Old member 1 gains a collision-class warning against old member 2; arm 6 is
    // hosted by member 1 (its trigger), so it is blamed, not the edit discarded.
    let mut script = with(warning(Class::Unbraced, 1, 0, 1.4));
    script.metrics = BETTER;
    let mut mock = Mock::new(4, 3, vec![script, clean_better()]);
    mock.trigger_host = BTreeMap::from([(4, 0), (5, 3), (6, 1)]);
    let (driven, report) = run(&mut mock, 2, Sabotage::NONE);
    assert!(matches!(driven, Driven::Kept { trial: 1, .. }), "{:?}", exit_of(&driven));
    assert_eq!(mock.withdrawn, vec![BTreeSet::from([6])]);
    assert_eq!(report.traced_by_class.get("Unbraced"), None, "the report keeps the LAST trial's regressions");
    assert_eq!(report.withdrawn_by_class.get("Unbraced"), Some(&1));
}

/// **Late-finish regression.** A new arm's foot slides at finish and leaves an
/// OLD foot partial on the model: the warning exists only on the finished field
/// (the pre-finish state had none), names an old member, and is withdrawn with
/// the arm whose ball stands within the old tip's window.
#[test]
fn a_late_finish_regression_on_an_old_foot_withdraws_the_arm_that_caused_it() {
    let partial = warning(Class::Landing, 2, crate::landing_check::LandingDefect::FootPartial as u32, 30.0);
    let mut mock = Mock::new(4, 3, vec![with(partial), clean_better()]);
    mock.near_new = BTreeMap::from([(2, vec![5])]);
    let (driven, report) = run(&mut mock, 2, Sabotage::NONE);
    assert!(matches!(driven, Driven::Kept { trial: 1, .. }), "{:?}", exit_of(&driven));
    assert_eq!(mock.withdrawn, vec![BTreeSet::from([5])]);
    assert_eq!(report.withdrawn_by_class.get("Landing"), Some(&1));
    // Without the link the same warning is untraced and the edit is discarded.
    let mut unlinked = Mock::new(
        4,
        3,
        vec![with(warning(Class::Landing, 2, crate::landing_check::LandingDefect::FootPartial as u32, 30.0))],
    );
    let (driven, _) = run(&mut unlinked, 2, Sabotage::NONE);
    assert_eq!(exit_of(&driven), Some(EditExit::Untraced));
}

/// **Validation off: a collision reaches the file.** A new arm that collides with
/// an old member is withdrawn; with the compare skipped the very same trial is
/// kept, collision and all.
#[test]
fn sabotage_a_collision_reaches_the_kept_plan_when_the_compare_is_off() {
    let collision = warning(Class::Collision, 2, 5, 0.3);
    let honest_mock = || Mock::new(4, 3, vec![with(collision), clean_better()]);
    let (honest, _) = run(&mut honest_mock(), 2, Sabotage::NONE);
    assert!(matches!(honest, Driven::Kept { trial: 1, .. }), "the colliding arm was withdrawn first");
    let mut mock = honest_mock();
    let (sabotaged, _) = run(&mut mock, 2, Sabotage { skip_compare: true, no_blame: false });
    assert!(matches!(sabotaged, Driven::Kept { trial: 0, .. }));
    assert!(mock.withdrawn.is_empty(), "the colliding arm stayed in the kept plan");
}

// ----------------------------------------------------------- the fallback

/// Every exit but the kept one, and the two modes: ONE dispatch.
const DISCARDS: [EditExit; 9] = [
    EditExit::SeedMismatch,
    EditExit::NoInsertion,
    EditExit::OldLandingMoved,
    EditExit::Global,
    EditExit::Untraced,
    EditExit::NoGain,
    EditExit::ClosureHitsOld,
    EditExit::Empty,
    EditExit::Budget,
];

#[test]
fn every_discard_exit_dispatches_through_the_one_fallback() {
    for exit in DISCARDS {
        assert_eq!(fallback(SecondContactMode::Edit, exit), Fallback::First, "{exit:?}: Edit returns P1");
        assert_eq!(
            fallback(SecondContactMode::EditThenReplan, exit),
            Fallback::Replan,
            "{exit:?}: EditThenReplan runs today's re-plan and chooser"
        );
        assert_eq!(fallback(SecondContactMode::Replan, exit), Fallback::Replan, "{exit:?}: Replan is today's");
    }
}

/// **Sabotage of the dispatch.** A dispatcher keyed on anything but the mode (the
/// reason here, standing for any other input such as a model name) is told from
/// the real one by the table above: it answers differently for some reason.
#[test]
fn sabotage_a_dispatch_keyed_on_the_reason_fails_the_table() {
    let keyed_on_reason = |mode: SecondContactMode, exit: EditExit| -> Fallback {
        if exit == EditExit::Budget {
            Fallback::First
        } else {
            fallback(mode, exit)
        }
    };
    let differs = DISCARDS.iter().any(|exit| {
        keyed_on_reason(SecondContactMode::EditThenReplan, *exit) != fallback(SecondContactMode::EditThenReplan, *exit)
    });
    assert!(differs, "the table of the one dispatch distinguishes a dispatch that reads the reason");
}

// -------------------------------------------------------------- sabotage

/// **Sabotage of the authoritative compare.** The same global regression that
/// discards the edit is kept when the compare is skipped: the compare is the
/// decision that stands between a bad edit and the file.
#[test]
fn sabotage_without_the_compare_a_global_regression_is_kept() {
    let mut script = clean_better();
    script.finished = Finished::from_parts(
        script.finished.warnings.clone(),
        Globals { uncarried_load_mm2: 0.5, ..Globals::default() },
    );
    let (honest, _) = run(&mut Mock::new(4, 3, vec![script.clone()]), 2, Sabotage::NONE);
    assert_eq!(exit_of(&honest), Some(EditExit::Global));
    let (sabotaged, _) = run(
        &mut Mock::new(4, 3, vec![script]),
        2,
        Sabotage { skip_compare: true, no_blame: false },
    );
    assert!(matches!(sabotaged, Driven::Kept { trial: 0, .. }), "the skipped compare keeps it");
}

/// **Sabotage of the blame.** A regression with a structural link is withdrawn
/// and the edit kept at the next trial; with the blame off the same regression is
/// untraced and the edit is discarded.
#[test]
fn sabotage_without_the_blame_a_linked_regression_discards_the_edit() {
    let make = || {
        let mut mock = Mock::new(
            4,
            3,
            vec![with(warning(Class::Unbraced, 1, 0, 1.4)), clean_better()],
        );
        mock.trigger_host = BTreeMap::from([(4, 0), (5, 3), (6, 1)]);
        mock
    };
    let (honest, _) = run(&mut make(), 2, Sabotage::NONE);
    assert!(matches!(honest, Driven::Kept { trial: 1, .. }));
    let (sabotaged, _) = run(&mut make(), 2, Sabotage { skip_compare: false, no_blame: true });
    assert_eq!(exit_of(&sabotaged), Some(EditExit::Untraced));
}

/// The transaction's code does not read a model's name: no identifier in the
/// module (or in the entry's dispatch) names one, and the entry has no parameter
/// that could carry it.
#[test]
fn no_code_path_reads_the_model_name() {
    let source = include_str!("../edit.rs");
    let code: String = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for word in ["model_name", "ModelId", "Deinonychus", "Cylinder", "Bat_Cat_Owl", "file_name", "name()"] {
        assert!(!code.contains(word), "the edit transaction names `{word}`");
    }
}

// ============================================================ a real field

use std::sync::{Arc, OnceLock};

use plinth_core::LayerPlan;
use plinth_mesh::Mesh;

use super::{run_edit, EditResult, SweepWorld};
use crate::down_sweep::ContactCaps;
use crate::{EditSeed, SweepPlan, SweepPolicy};

/// A real sweep of the Round 25 disc, its seed, and caps that ask for a second
/// contact beside the six most loaded tips.
struct Real {
    mesh: Mesh,
    layers: LayerPlan,
    policy: SweepPolicy,
    plan: SweepPlan,
    seed: Arc<EditSeed>,
    caps: ContactCaps,
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a layer count is a finite positive quotient of model-scale lengths"
)]
fn real() -> &'static Real {
    static REAL: OnceLock<Real> = OnceLock::new();
    REAL.get_or_init(|| {
        let mesh = plinth_mesh::make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96).expect("the Round 25 profile");
        let height = *plinth_core::m0_constants()
            .expect("registry")
            .evidence()
            .layer_height_default_mm()
            .value();
        let min_z = mesh.verts().iter().map(|v| v[2]).fold(f64::INFINITY, f64::min);
        let max_z = mesh.verts().iter().map(|v| v[2]).fold(f64::NEG_INFINITY, f64::max);
        let plate_z = min_z - 5.0;
        let count = (((max_z - plate_z) / height).ceil() as usize).max(1);
        let layers = LayerPlan::new(plate_z, vec![height; count]).expect("a uniform plan");
        let policy = SweepPolicy::from_registry().expect("the registry");
        let (plan, seed) = crate::run_sweep_with_holes_seeded(&mesh, &layers, &policy, &[], &[]).expect("sweep");
        let mut loaded: Vec<(f64, usize, usize)> = plan
            .struts
            .iter()
            .filter(|strut| strut.kind == StrutKind::Arm)
            .filter_map(|strut| Some((*strut.carried_mm2.last()?, strut.cell, strut.k_top)))
            .collect();
        loaded.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut caps = ContactCaps::default();
        for (load, cell, k_top) in loaded.into_iter().take(6) {
            caps.insert(cell, k_top, 0.2 * load / policy.x_allowed);
        }
        Real { mesh, layers, policy, plan, seed, caps }
    })
}

fn text<T: std::fmt::Debug>(value: &T) -> String {
    format!("{value:?}")
}

fn world(real: &'static Real, report: &mut EditReport) -> SweepWorld<'static> {
    SweepWorld::insert(&real.seed, &real.caps, &real.mesh, &real.layers, &real.policy, report)
        .expect("no cancel")
        .unwrap_or_else(|exit| panic!("the insertion phase exited with {exit:?}"))
}

/// **W isolation.** A trial finishes a CLONE: W is bit-equal after it, finishing
/// really changes the members (so the check is not vacuous), and a second trial
/// from the same W gives the same field.
#[test]
fn a_trial_finishes_a_clone_and_leaves_w_alone() {
    let real = real();
    let mut report = EditReport::default();
    let mut world = world(real, &mut report);
    assert!(report.inserted > 0, "the caps placed carriers: this is the second-contact topology");
    assert_eq!(report.arms_first, real.plan.struts.len());
    let before = text(&world.work().struts);
    let first = world.trial(0, &mut report).expect("trial");
    assert_eq!(before, text(&world.work().struts), "finishing a clone changed W");
    assert_ne!(before, text(&first.plan.struts), "finishing changes the members (anti-vacuity)");
    let again = world.trial(1, &mut report).expect("trial");
    assert_eq!(text(&first.plan.struts), text(&again.plan.struts), "two trials of one W differ");
    assert_eq!(text(&first.finished), text(&again.finished));
}

/// **Withdrawing every new arm gives back the first plan's state at C**: the old
/// prefix, bit for bit, after the insertion-OFF re-size.
#[test]
fn withdrawing_every_new_arm_restores_the_state_at_c() {
    let real = real();
    let mut report = EditReport::default();
    let mut world = world(real, &mut report);
    let n_c = world.n_c();
    let all_new: BTreeSet<u32> = (n_c..u32::try_from(world.work_struts().len()).unwrap()).collect();
    assert!(world.withdraw(&all_new, &mut report).expect("no cancel"));
    assert_eq!(world.work_struts().len(), n_c as usize);
    let at_c = real.seed.lane().struts();
    let same = world
        .work_struts()
        .iter()
        .zip(at_c)
        .filter(|(now, held)| text(now) != text(held))
        .count();
    assert_eq!(same, 0, "{same} old members differ from the state at C after withdrawing the edit");
    assert!(world.provenance().trigger_host.is_empty() || world.provenance().n_c == n_c);
    assert_eq!(report.withdraw_seconds.len(), 1);
}

/// **Provenance remap on a real field.** Withdraw the first new arm: the
/// survivors renumber, their trigger hosts follow, the old prefix is untouched,
/// and a trial after it equals a trial of a world that never ran one first (the
/// failed trial's head and seat changes did not survive).
#[test]
fn a_withdrawal_remaps_provenance_and_a_failed_trial_leaves_nothing_behind() {
    let real = real();
    let mut report = EditReport::default();
    let mut used = world(real, &mut report);
    let n_c = used.n_c();
    let total = u32::try_from(used.work_struts().len()).unwrap();
    assert!(total > n_c + 1, "at least two new arms to tell a renumbering");
    let hosts_before = {
        let judged = used.trial(0, &mut report).expect("trial");
        judged.provenance.trigger_host
    };
    assert_eq!(hosts_before.len(), (total - n_c) as usize, "every new arm has a trigger host");
    assert!(hosts_before.values().all(|host| *host < n_c), "hosts are old members");
    let doomed = BTreeSet::from([n_c]);
    assert!(used.withdraw(&doomed, &mut report).expect("no cancel"));
    assert_eq!(u32::try_from(used.work_struts().len()).unwrap(), total - 1);
    assert!(used.work_struts().iter().enumerate().all(|(i, s)| s.id as usize == i), "ids are dense");
    let judged = used.trial(1, &mut report).expect("trial");
    let expected: BTreeMap<u32, u32> = hosts_before
        .iter()
        .filter(|(arm, _)| **arm != n_c)
        .map(|(arm, host)| (arm - 1, *host))
        .collect();
    assert_eq!(judged.provenance.trigger_host, expected, "the survivors' hosts at their new ids");
    // A world that withdrew the same arm without ever finishing a trial.
    let mut fresh = world(real, &mut report);
    assert!(fresh.withdraw(&doomed, &mut report).expect("no cancel"));
    let control = fresh.trial(0, &mut report).expect("trial");
    assert_eq!(
        text(&judged.plan.struts),
        text(&control.plan.struts),
        "a failed trial's head downgrade or seat slide survived into the next trial"
    );
}

/// The edit is judged and decided the same at 1, 4 and 12 threads.
#[test]
fn the_edit_gives_one_result_at_any_thread_count() {
    let real = real();
    let one_run = |threads: usize| -> (String, String) {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().expect("a pool");
        pool.install(|| {
            let result = run_edit(
                &real.plan,
                Some(&real.seed),
                &real.caps,
                &real.mesh,
                &real.layers,
                &real.policy,
                &[],
                &[],
                Sabotage::NONE,
            )
            .expect("no cancel");
            match result {
                EditResult::Kept(plan, report) => (format!("kept {:?}", report.kept_trial), text(&plan.struts)),
                EditResult::Discarded(exit, _) => (format!("discarded {exit:?}"), String::new()),
            }
        })
    };
    let baseline = one_run(1);
    println!("SC3 thread determinism: {}", baseline.0);
    for threads in [4, 12] {
        assert_eq!(one_run(threads), baseline, "{threads} threads differ from 1");
    }
}

/// A seed that is not the first plan's is a seed mismatch, before any work.
#[test]
fn a_foreign_or_missing_seed_is_a_seed_mismatch() {
    let real = real();
    let run = |seed: Option<&EditSeed>, plan: &SweepPlan| {
        run_edit(plan, seed, &real.caps, &real.mesh, &real.layers, &real.policy, &[], &[], Sabotage::NONE)
            .expect("no cancel")
    };
    let EditResult::Discarded(exit, report) = run(None, &real.plan) else { panic!("no seed, no edit") };
    assert_eq!(exit, EditExit::SeedMismatch);
    assert!(!report.attempted, "nothing was tried");
    // A first plan whose digest is not the one the seed finished into.
    let mut other = real.plan.clone();
    other.report.sweep_digest.push('x');
    let EditResult::Discarded(exit, _) = run(Some(&real.seed), &other) else { panic!("a foreign plan") };
    assert_eq!(exit, EditExit::SeedMismatch);
}
