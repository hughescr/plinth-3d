//! **The edit transaction** (SC-3, `SC-DESIGN.md` §3 and §5): the second contact
//! made as a checked edit of the finished first plan (P1), instead of a second
//! plan.
//!
//! One insertion phase runs on the pre-finish working state W (a clone of the
//! seed's state at checkpoint C): the contact caps of P1's short tips ask the
//! down-sweep for carriers beside them. Each TRIAL finishes a CLONE of W, so a
//! failed trial's head downgrades, seat slides and footprint moves die with it
//! (W is never finished). The finished trial is set against finished P1 on the
//! acceptance contract of the parent module, on final predicates recomputed
//! after the last mutation of both. A new arm that fails, or that an old
//! member's regression is structurally linked to, is withdrawn ON W with its
//! dependants; W is re-sized with insertion off and a fresh clone is finished.
//! Every doubt resolves to the fallback ([`fallback`]): the edit is thrown
//! away, and the mode decides between P1 and today's re-plan.
//!
//! The decisions are in [`drive`], generic over a [`World`], so that every exit
//! is exercised by scripted trials; [`SweepWorld`] is the production world.
//! Nothing here reads a model's name.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use plinth_core::LayerPlan;
use plinth_mesh::Mesh;

use super::{
    compare, contact_balls, ground_lost, orphaned_by, remap_for, withdrawal_closure, Census,
    EscapeInputs, FinalChecks, FinalEnv, Finished, Provenance, Verdict,
};
use crate::contact_growth::{EditExit, EditReport, SecondContactMode};
use crate::down_sweep::{self, ContactCaps, InsertionPolicy};
use crate::footprint::ContactBall;
use crate::layer_cache::LayerCache;
use crate::sweep_state::{EditSeed, LaneState, SweepEnv};
use crate::{landing_check, SweepError, SweepPlan, SweepPolicy, SweepStrut};

// -------------------------------------------------------------- the fallback

/// Where a discarded edit goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fallback {
    /// The first plan stands.
    First,
    /// Today's re-plan runs, and today's chooser decides (the first plan stands
    /// where it rejects).
    Replan,
}

/// **The one fallback every discard exit goes through** (`SC-DESIGN.md` §3):
/// mode `Edit` returns P1; mode `EditThenReplan` runs today's re-plan and chooser;
/// mode `Replan` never reaches an edit, and is today's re-plan. It never looks at
/// the reason (the reason is counted by the caller) and never at a model.
#[must_use]
pub(crate) fn fallback(mode: SecondContactMode, reason: EditExit) -> Fallback {
    debug_assert!(
        !matches!(reason, EditExit::Kept | EditExit::NotTried),
        "a kept or untried edit has no fallback"
    );
    match mode {
        SecondContactMode::Edit => Fallback::First,
        SecondContactMode::Replan | SecondContactMode::EditThenReplan => Fallback::Replan,
    }
}

// ------------------------------------------------------------- the keep test

/// What today's keep test reads of a finished field: the tips short of their
/// need and the summed shortfall (the seat-contact rows of the finished plan, on
/// the final balls), and the unbraced runs over their limit on the FINAL
/// predicates (not the history counter).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Metrics {
    pub short: usize,
    pub shortfall_mm2: f64,
    pub unbraced: usize,
    /// For the report only: unbraced runs at twice their limit or more, and the
    /// worst run's length over its limit.
    pub unbraced_ge2: usize,
    pub unbraced_max: f64,
}

impl Metrics {
    pub(crate) fn of(plan: &SweepPlan, finals: &FinalChecks) -> Self {
        let ratio = |run: &super::FinalRun| run.length_mm / run.limit_mm.max(f64::MIN_POSITIVE);
        Self {
            short: plan.report.seat_contact.short.len(),
            shortfall_mm2: plan
                .report
                .seat_contact
                .short
                .iter()
                .map(|short| (short.planned_mm2 - short.real_mm2).max(0.0))
                .sum(),
            unbraced: finals.unbraced.len(),
            unbraced_ge2: finals.unbraced.iter().filter(|run| ratio(run) >= 2.0).count(),
            unbraced_max: finals.unbraced.iter().map(ratio).fold(0.0, f64::max),
        }
    }
}

/// The keyed warnings of a finished field, counted by class name (the report's
/// quality table: collisions, clearance, landings, heads, ...).
fn classes(finished: &Finished) -> BTreeMap<String, usize> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for warning in &finished.warnings {
        *counts.entry(class_name(warning.key.class)).or_default() += 1;
    }
    counts
}

/// **Today's keep test** (`lib.rs`, the chooser's `kept_second`), on final
/// predicates on both sides: the shortfall strictly lower and no more unbraced
/// runs over their limit.
#[must_use]
pub(crate) fn keeps(first: &Metrics, edited: &Metrics) -> bool {
    edited.shortfall_mm2 < first.shortfall_mm2 && edited.unbraced <= first.unbraced
}

// -------------------------------------------------------------- the driver

/// One finished trial, judged.
pub(crate) struct Judged<P> {
    pub plan: P,
    pub finished: Finished,
    pub metrics: Metrics,
    /// The provenance this trial's verdict is read with (trigger hosts of the
    /// surviving new arms, the new arms near each old tip that lost ground).
    pub provenance: Provenance,
}

/// What the driver asks of the field it edits.
pub(crate) trait World {
    type Plan;

    /// Members at checkpoint C: ids below are old, ids from it are new.
    fn n_c(&self) -> u32;

    /// The members of a finished trial (for the structural links of blame).
    fn struts_of<'a>(&'a self, plan: &'a Self::Plan) -> &'a [SweepStrut];

    /// The members of the working state W (what a withdrawal closes over).
    fn work_struts(&self) -> &[SweepStrut];

    /// Finish a CLONE of W and judge it; W is not changed.
    ///
    /// # Errors
    ///
    /// As the sweep: a cancelled run.
    fn trial(&mut self, trial: usize, report: &mut EditReport) -> Result<Judged<Self::Plan>, SweepError>;

    /// Withdraw `doomed` (a dependency closure of new arms) from W, remap its
    /// provenance, and re-size it with insertion off. `false` where the
    /// withdrawal would leave a member resting on a dropped one (nothing is
    /// changed then).
    ///
    /// # Errors
    ///
    /// As the sweep: a cancelled run.
    fn withdraw(&mut self, doomed: &BTreeSet<u32>, report: &mut EditReport) -> Result<bool, SweepError>;
}

/// How the transaction ended.
pub(crate) enum Driven<P> {
    Kept { plan: P, trial: usize },
    Discarded(EditExit),
}

/// Test-only switches that turn one decision of the transaction off, so a test
/// can show that the decision is what stands between a bad edit and the file.
/// Production passes [`Sabotage::NONE`].
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Sabotage {
    /// Judge nothing: every trial is kept as it stands.
    pub skip_compare: bool,
    /// Blame nothing: a regression with a structural link is untraced.
    pub no_blame: bool,
}

impl Sabotage {
    pub(crate) const NONE: Self = Self { skip_compare: false, no_blame: false };
}

fn class_name(class: super::Class) -> String {
    format!("{class:?}")
}

/// Record a verdict in the report (the last judged trial's).
fn note_verdict(report: &mut EditReport, verdict: &Verdict) {
    report.global_regressions = verdict.global.iter().map(|g| g.field.to_owned()).collect();
    report.new_arm_warnings = verdict.new_arm_warnings.len();
    report.traced_by_class.clear();
    report.untraced_by_class.clear();
    for (regression, _) in &verdict.traced {
        *report.traced_by_class.entry(class_name(regression.key.class)).or_default() += 1;
    }
    for regression in &verdict.untraced {
        *report.untraced_by_class.entry(class_name(regression.key.class)).or_default() += 1;
    }
}

/// **The transaction's decisions** (`SC-DESIGN.md` §3). `first` is finished P1
/// judged once; `rounds` is `R`, the withdrawal rounds. Every trial `t` in
/// `0..=R` is judged by the same contract and the same keep test, and a passing
/// trial at `t = R` is emitted; `Budget` is only where trial `R` still has traced
/// regressions. The new arms strictly shrink each round (a closure is not
/// empty), so the loop also ends without `R`.
///
/// # Errors
///
/// As the world's: a cancelled run.
pub(crate) fn drive<W: World>(
    world: &mut W,
    first: &Finished,
    first_metrics: &Metrics,
    rounds: usize,
    sabotage: Sabotage,
    report: &mut EditReport,
) -> Result<Driven<W::Plan>, SweepError> {
    let n_c = world.n_c();
    for trial in 0..=rounds {
        let judged = world.trial(trial, report)?;
        report.trials = trial + 1;
        report.edited_short = judged.metrics.short;
        report.edited_shortfall_mm2 = judged.metrics.shortfall_mm2;
        report.edited_unbraced = judged.metrics.unbraced;
        report.edited_unbraced_ge2 = judged.metrics.unbraced_ge2;
        report.edited_unbraced_max = judged.metrics.unbraced_max;
        report.edited_classes = classes(&judged.finished);
        let started = Instant::now();
        let verdict = if sabotage.skip_compare {
            Verdict::default()
        } else {
            let mut verdict = compare(first, &judged.finished, world.struts_of(&judged.plan), &judged.provenance);
            if sabotage.no_blame {
                // Blame nothing: every traced regression has no link.
                let traced = std::mem::take(&mut verdict.traced);
                verdict.untraced.extend(traced.into_iter().map(|(regression, _)| regression));
                verdict.withdraw.clear();
                verdict.withdraw.extend(
                    verdict.new_arm_warnings.iter().flat_map(|w| w.key.members()).filter(|id| *id >= n_c),
                );
            }
            verdict
        };
        report.compare_seconds.push(started.elapsed().as_secs_f64());
        note_verdict(report, &verdict);
        if !verdict.global.is_empty() {
            return Ok(Driven::Discarded(EditExit::Global));
        }
        if !verdict.untraced.is_empty() {
            return Ok(Driven::Discarded(EditExit::Untraced));
        }
        if verdict.withdraw.is_empty() {
            return Ok(if keeps(first_metrics, &judged.metrics) {
                Driven::Kept { plan: judged.plan, trial }
            } else {
                Driven::Discarded(EditExit::NoGain)
            });
        }
        if trial == rounds {
            return Ok(Driven::Discarded(EditExit::Budget));
        }
        let closure = withdrawal_closure(world.work_struts(), &verdict.withdraw, n_c);
        if !closure.old_reached.is_empty() {
            return Ok(Driven::Discarded(EditExit::ClosureHitsOld));
        }
        let tail = world.work_struts().len().saturating_sub(n_c as usize);
        if closure.ids.len() >= tail {
            return Ok(Driven::Discarded(EditExit::Empty));
        }
        for (regression, arms) in &verdict.traced {
            *report.withdrawn_by_class.entry(class_name(regression.key.class)).or_default() += arms.len();
        }
        for warning in &verdict.new_arm_warnings {
            let arms = warning.key.members().into_iter().filter(|id| *id >= n_c).count();
            *report.withdrawn_by_class.entry(class_name(warning.key.class)).or_default() += arms;
        }
        report.withdrawn += closure.ids.len();
        if !world.withdraw(&closure.ids, report)? {
            return Ok(Driven::Discarded(EditExit::ClosureHitsOld));
        }
        report.rounds += 1;
    }
    // The loop always returns: trial `rounds` is the last and returns above.
    Ok(Driven::Discarded(EditExit::Budget))
}

// ------------------------------------------------------- measuring a field

/// A finished field as the contract reads it, with the contact balls of its
/// seated tips and, for an edit, the new arms near each old tip that lost ground.
pub(crate) struct Measured {
    pub finished: Finished,
    pub metrics: Metrics,
    pub balls: Vec<(u32, ContactBall)>,
    pub near_new: BTreeMap<u32, Vec<u32>>,
}

/// Measure a finished plan: the census, the final predicates (with the escape
/// flood on the env's slices and drains), the keyed warnings, and the balls.
/// `old_balls` is P1's, `None` for P1 itself; `n_c` splits old from new.
pub(crate) fn measure(
    plan: &SweepPlan,
    env: &SweepEnv,
    layers: &LayerPlan,
    policy: &SweepPolicy,
    old_balls: Option<&[(u32, ContactBall)]>,
    n_c: u32,
) -> Measured {
    let escape = EscapeInputs {
        slices: &env.slices,
        drains: &env.drains,
        voxel_pitch_mm: policy.passage_voxel_pitch_mm,
        tip_radius_max_mm: policy.tip_radius_max_mm,
    };
    let final_env = FinalEnv::of_plan(plan, policy, Some(escape));
    let mut census = Census::measure(plan, layers, &final_env, policy.tip_radius_max_mm);
    let balls = contact_balls(&plan.struts, &crate::adapter::arms_of(plan), layers);
    let mut near_new: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    if let Some(old) = old_balls {
        let lost = ground_lost(
            &plan.grid,
            &env.slices,
            plan.plate_z_mm,
            plan.layer_height_mm,
            n_c,
            old,
            &balls,
        );
        census.ground_lost = lost.iter().map(|(id, _)| *id).collect();
        near_new = lost.into_iter().collect();
    }
    let metrics = Metrics::of(plan, &census.finals);
    let finished = Finished::of(&plan.report, &plan.struts, &census);
    Measured { finished, metrics, balls, near_new }
}

// ------------------------------------------------------ the production world

/// W and everything a trial reads: the shared env, the working state, the layer
/// cache rebuilt from the env, P1's balls, and the provenance of the new arms.
pub(crate) struct SweepWorld<'a> {
    env: &'a SweepEnv,
    mesh: &'a Mesh,
    layers: &'a LayerPlan,
    policy: &'a SweepPolicy,
    /// The policy a trial finishes under: the shake check deferred (the final
    /// predicates recompute it), run on the kept plan alone.
    trial_policy: SweepPolicy,
    work: LaneState,
    cache: LayerCache<'a>,
    n_c: u32,
    first_balls: Vec<(u32, ContactBall)>,
    trigger_host: BTreeMap<u32, u32>,
    /// `(per_event::entry(host, layer), limit bits)` of the triggers the
    /// insertion phase's geometry refused, renumbered through every withdrawal.
    refused: Vec<(u64, u64)>,
    inserted_by: Vec<(u32, u64)>,
    last_provenance: Provenance,
}

/// The seed's keys against the request, and the capped tips against the seed:
/// the first exit of the transaction.
fn seed_miss(
    first: &SweepPlan,
    seed: &EditSeed,
    mesh: &Mesh,
    layers: &LayerPlan,
    policy: &SweepPolicy,
    drains: &[crate::DrainSpec],
    vents: &[crate::DrainSpec],
) -> bool {
    if seed
        .miss(mesh, layers, policy, drains, vents, &first.report.sweep_digest)
        .is_some()
    {
        return true;
    }
    let held = seed.lane().struts();
    if held.len() != first.struts.len() {
        return true;
    }
    // A cap is keyed `(cell, k_top)`: finishing changes neither field, so each
    // short tip must stand at the same key at C.
    first.report.seat_contact.short.iter().any(|short| {
        match (first.struts.get(short.strut), held.get(short.strut)) {
            (Some(finished), Some(at_c)) => {
                finished.cell != at_c.cell || finished.k_top != at_c.k_top
            }
            _ => true,
        }
    })
}

impl<'a> SweepWorld<'a> {
    /// Insert: clone W from the seed, rebuild the layer cache, run the ONE
    /// insertion phase with the caps (pass 1 from the seed's route log where it
    /// is valid), and check the old members' landings did not move. `Err(exit)`
    /// where the edit is discarded before any trial.
    pub(crate) fn insert(
        seed: &'a EditSeed,
        caps: &'a ContactCaps,
        mesh: &'a Mesh,
        layers: &'a LayerPlan,
        policy: &'a SweepPolicy,
        report: &mut EditReport,
    ) -> Result<Result<Self, EditExit>, SweepError> {
        let env: &SweepEnv = seed.env();
        let started = Instant::now();
        let cache = LayerCache::build(&env.grid, &env.slices);
        report.layer_cache_seconds = started.elapsed().as_secs_f64();
        let mut work = seed.lane_clone();
        let n_c = u32::try_from(work.struts.len()).unwrap_or(u32::MAX);
        let started = Instant::now();
        let run_seed = seed.run_seed();
        let insertion = InsertionPolicy::new(policy.limits.max_struts).with_contact_caps(caps);
        let mut outcome = down_sweep::run_seeded(
            &cache,
            &mut work.struts,
            env.sizing,
            insertion,
            &env.keep_out,
            &env.unrooted,
            run_seed.as_ref(),
        );
        report.insertion_seconds = started.elapsed().as_secs_f64();
        report.insertion_passes = outcome.per_event.passes;
        report.seeded_log_valid = outcome.seeded_pass;
        report.seeded_log_stale = outcome.seed_stale;
        report.refused = outcome.refused.len();
        report.arms_first = n_c as usize;
        report.inserted = work.struts.len().saturating_sub(n_c as usize);
        if report.inserted == 0 {
            return Ok(Err(EditExit::NoInsertion));
        }
        // The edit is an edit of P1: the routing cells, paths and landings of
        // every old member stay where P1 put them.
        let moved = seed
            .lane()
            .struts()
            .iter()
            .zip(&work.struts)
            .any(|(old, now)| old.landing != now.landing || old.cell != now.cell || old.k_top != now.k_top);
        if moved {
            return Ok(Err(EditExit::OldLandingMoved));
        }
        // The new arms start with `interior` unset; the test looks UP, so it is
        // asked of the field once the arms are in.
        crate::set_interior(&mut work.struts, &env.history);
        work.landings.record(
            "second contact edit",
            &landing_check::check_landings(&work.struts, env.height),
        );
        work.moves.note("second contact edit", &work.struts);
        let trigger_host: BTreeMap<u32, u32> = outcome
            .inserted_by
            .iter()
            .filter(|(arm, _)| *arm >= n_c)
            .map(|&(arm, key)| (arm, u32::try_from(crate::per_event::unpack(key).0).unwrap_or(u32::MAX)))
            .collect();
        let inserted_by = outcome.inserted_by.clone();
        let refused = outcome.refused.clone();
        outcome.arms_inserted_by_load = (work.struts.len() - n_c as usize) as u64;
        work.outcome = outcome;
        Ok(Ok(Self {
            env,
            mesh,
            layers,
            policy,
            trial_policy: crate::orient_demand::production_measurement_policy(*policy),
            work,
            cache,
            n_c,
            first_balls: Vec::new(),
            trigger_host,
            refused,
            inserted_by,
            last_provenance: Provenance::default(),
        }))
    }

    /// P1's contact balls, for the ground the edit must not lose.
    pub(crate) fn set_first_balls(&mut self, balls: Vec<(u32, ContactBall)>) {
        self.first_balls = balls;
    }

    /// The working state (tests read it to prove a trial left it alone).
    #[cfg(test)]
    pub(crate) fn work(&self) -> &LaneState {
        &self.work
    }

    /// The provenance of the new arms at the last trial.
    #[cfg(test)]
    pub(crate) fn provenance(&self) -> &Provenance {
        &self.last_provenance
    }

    /// The triggers the insertion phase's geometry refused, as renumbered.
    #[cfg(test)]
    pub(crate) fn refused(&self) -> &[(u64, u64)] {
        &self.refused
    }
}

impl World for SweepWorld<'_> {
    type Plan = SweepPlan;

    fn n_c(&self) -> u32 {
        self.n_c
    }

    fn struts_of<'a>(&'a self, plan: &'a SweepPlan) -> &'a [SweepStrut] {
        &plan.struts
    }

    fn work_struts(&self) -> &[SweepStrut] {
        &self.work.struts
    }

    fn trial(&mut self, _trial: usize, report: &mut EditReport) -> Result<Judged<SweepPlan>, SweepError> {
        let started = Instant::now();
        // A CLONE: `finish` mutates seats, slides and heads, and a failed trial's
        // changes must not survive into the next one.
        let plan = crate::finish(
            self.env,
            self.work.clone(),
            self.mesh,
            self.layers,
            &self.trial_policy,
        )?;
        report.finish_seconds.push(started.elapsed().as_secs_f64());
        let started = Instant::now();
        let measured = measure(
            &plan,
            self.env,
            self.layers,
            self.policy,
            Some(&self.first_balls),
            self.n_c,
        );
        report.checks_seconds.push(started.elapsed().as_secs_f64());
        let provenance = Provenance {
            n_c: self.n_c,
            trigger_host: self.trigger_host.clone(),
            near_new: measured.near_new,
        };
        self.last_provenance = provenance.clone();
        report.arms_edited = plan.struts.len();
        Ok(Judged {
            plan,
            finished: measured.finished,
            metrics: measured.metrics,
            provenance,
        })
    }

    fn withdraw(&mut self, doomed: &BTreeSet<u32>, report: &mut EditReport) -> Result<bool, SweepError> {
        let started = Instant::now();
        // The closure leaves nobody resting on a dropped member; a survivor that
        // would fall to the plate stops the withdrawal before anything changes.
        if !orphaned_by(&self.work.struts, doomed).is_empty() {
            return Ok(false);
        }
        let count = self.work.struts.len();
        let flags: Vec<bool> = (0..count)
            .map(|index| doomed.contains(&u32::try_from(index).unwrap_or(u32::MAX)))
            .collect();
        let expected = remap_for(count, doomed);
        let remap = crate::drop_members(&mut self.work.struts, &flags);
        debug_assert_eq!(remap, expected, "drop_members and remap_for agree");
        let old_untouched = (0..self.n_c as usize).all(|index| remap.get(index) == u32::try_from(index).ok().as_ref());
        if !old_untouched {
            // An old member was renumbered: not an edit of P1. Unreachable for a
            // closure of new arms, kept as a guard.
            return Ok(false);
        }
        let provenance = Provenance {
            n_c: self.n_c,
            trigger_host: self.trigger_host.clone(),
            near_new: BTreeMap::new(),
        }
        .remapped(&remap);
        self.trigger_host = provenance.trigger_host;
        let to = |id: u32| remap.get(id as usize).copied().filter(|fresh| *fresh != u32::MAX);
        self.inserted_by = self
            .inserted_by
            .iter()
            .filter_map(|&(arm, key)| {
                let (host, layer) = crate::per_event::unpack(key);
                let host = to(u32::try_from(host).ok()?)?;
                Some((to(arm)?, crate::per_event::entry(host as usize, layer)))
            })
            .collect();
        self.refused = self
            .refused
            .iter()
            .filter_map(|&(key, limit)| {
                let (host, layer) = crate::per_event::unpack(key);
                let host = to(u32::try_from(host).ok()?)?;
                Some((crate::per_event::entry(host as usize, layer), limit))
            })
            .collect();
        self.refused.sort_unstable();
        // Clear the routing (the old log is of a field that no longer exists) and
        // size the field again with insertion OFF: exactly one pass.
        let mut outcome = down_sweep::run_seeded(
            &self.cache,
            &mut self.work.struts,
            self.env.sizing,
            InsertionPolicy::disabled(),
            &self.env.keep_out,
            &self.env.unrooted,
            None,
        );
        outcome.arms_inserted_by_load = (self.work.struts.len() - self.n_c as usize) as u64;
        outcome.inserted_by = self.inserted_by.clone();
        self.work.outcome = outcome;
        crate::set_interior(&mut self.work.struts, &self.env.history);
        self.work.landings.record(
            "second contact withdrawal",
            &landing_check::check_landings(&self.work.struts, self.env.height),
        );
        self.work.moves.note("second contact withdrawal", &self.work.struts);
        report.withdraw_seconds.push(started.elapsed().as_secs_f64());
        Ok(true)
    }
}

// ------------------------------------------------------------- the entry

/// What the transaction returned.
pub(crate) enum EditResult {
    /// The edited plan, finished, shake completed.
    Kept(Box<SweepPlan>, EditReport),
    /// Thrown away; the caller dispatches through [`fallback`].
    Discarded(EditExit, EditReport),
}

/// **The edit transaction on a finished first plan** (`SC-DESIGN.md` §3).
/// `first` is P1, `seed` the sweep's state at C that finished into it, `caps` the
/// contacts of P1's short tips (non-empty: the caller's early exits are
/// identical in every mode).
///
/// # Errors
///
/// As the sweep: a cancelled run.
#[expect(clippy::too_many_arguments, reason = "the sweep's own argument list")]
pub(crate) fn run_edit(
    first: &SweepPlan,
    seed: Option<&EditSeed>,
    caps: &ContactCaps,
    mesh: &Mesh,
    layers: &LayerPlan,
    policy: &SweepPolicy,
    drains: &[crate::DrainSpec],
    vents: &[crate::DrainSpec],
    sabotage: Sabotage,
) -> Result<EditResult, SweepError> {
    let total = Instant::now();
    let mut report = EditReport::default();
    let discard = |exit: EditExit, mut report: EditReport, total: Instant| -> Result<EditResult, SweepError> {
        report.exit = exit;
        report.total_seconds = total.elapsed().as_secs_f64();
        Ok(EditResult::Discarded(exit, report))
    };
    let Some(seed) = seed else {
        return discard(EditExit::SeedMismatch, report, total);
    };
    report.seed_bytes = seed.approx_bytes();
    if seed_miss(first, seed, mesh, layers, policy, drains, vents) {
        return discard(EditExit::SeedMismatch, report, total);
    }
    report.attempted = true;
    let mut world = match SweepWorld::insert(seed, caps, mesh, layers, policy, &mut report)? {
        Ok(world) => world,
        Err(exit) => return discard(exit, report, total),
    };
    // F1: once per edit, read-only on P1.
    let started = Instant::now();
    let n_c = world.n_c();
    let first_measured = measure(first, seed.env(), layers, policy, None, n_c);
    report.first_checks_seconds = started.elapsed().as_secs_f64();
    report.first_short = first_measured.metrics.short;
    report.first_shortfall_mm2 = first_measured.metrics.shortfall_mm2;
    report.first_unbraced = first_measured.metrics.unbraced;
    report.first_unbraced_ge2 = first_measured.metrics.unbraced_ge2;
    report.first_unbraced_max = first_measured.metrics.unbraced_max;
    report.first_classes = classes(&first_measured.finished);
    world.set_first_balls(first_measured.balls);
    let driven = drive(
        &mut world,
        &first_measured.finished,
        &first_measured.metrics,
        policy.second_contact_withdraw_rounds,
        sabotage,
        &mut report,
    )?;
    match driven {
        Driven::Discarded(exit) => discard(exit, report, total),
        Driven::Kept { mut plan, trial } => {
            // The trial deferred its shake check; the emitted plan carries it
            // run (the same numbers the in-run check gives).
            crate::complete_for_emission(&mut plan);
            report.exit = EditExit::Kept;
            report.kept_trial = Some(trial);
            report.total_seconds = total.elapsed().as_secs_f64();
            Ok(EditResult::Kept(Box::new(plan), report))
        }
    }
}

#[cfg(test)]
mod tests;
