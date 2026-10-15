//! Perf lane B1: the certified early stop of the production batch.
//!
//! What these pin, and what each one would miss:
//!
//! * the bound is `<=` the full score for every non-negative measurement, on
//!   real scored candidates, and is the full score's own arithmetic with the
//!   measured terms zeroed (`bound_is_the_full_score_with_the_measurements_zeroed`);
//! * a tie is never certified, a strictly higher bound always is, and the
//!   winner never is (`certify_losers_*`);
//! * the race stops exactly the poses whose bound is strictly above a
//!   finished score (`race_*`);
//! * end to end, through a measurer that honours the race, the winner, its
//!   terms and the whole record are the same whichever order the poses finish
//!   in, and the winner is the one a measurer that never stops picks
//!   (`the_record_does_not_depend_on_finish_order`).
//!
//! The sabotage drill (`sabotage_a_measured_term_in_the_bound`) puts a
//! measured term INTO the bound and shows the bound test go red.

use std::sync::atomic::Ordering;

use plinth_mesh::{make_overhang_table, Mesh};

use crate::demand::{
    DemandFidelity, MeasuredSupportDemand, ProductionRace, RaceOutcome, SupportDemandMeasurer,
};
use crate::score::{ObjectiveWeights, OrientFaceTable, UpDirection};
use crate::search::{certify_losers, search, DenseSweepRecord, SweepRequest, TermsRecord};

fn measured(
    support_area_cm2: f64,
    landing: Option<f64>,
    tip: Option<f64>,
) -> MeasuredSupportDemand {
    MeasuredSupportDemand {
        support_area_cm2,
        struts: 1,
        braces: 0,
        masts: 0,
        total_scar_mm2: 0.25,
        achieved_pitch_mm: 0.05,
        layers: 10,
        residual_demand_cells: 0,
        uncarried_load_mm2: 0.0,
        model_landings: 0,
        landing_scar_raw_mm2: landing,
        tip_scar_raw_mm2: tip,
        fidelity: DemandFidelity::Production,
    }
}

fn commit_terms(record: &DenseSweepRecord) -> Vec<TermsRecord> {
    record
        .tiers
        .last()
        .expect("a commit tier")
        .ranking
        .iter()
        .map(|entry| entry.terms)
        .collect()
}

/// The terms of every scored candidate at every tier of a search with no
/// demand measurer: real candidates, real peel corners and scars.
fn real_candidate_terms() -> Vec<TermsRecord> {
    let mesh = make_overhang_table().expect("overhang table fixture");
    let request = SweepRequest::registered().expect("registered sweep configuration");
    let outcome = search(&mesh, &request, None).expect("a scorable fixture");
    outcome
        .record()
        .tiers
        .iter()
        .flat_map(|tier| tier.ranking.iter().map(|entry| entry.terms))
        .collect()
}

/// The measurement grid: zeros, tiny, ordinary and huge, with the scars
/// present, absent, and in every combination.
fn measurement_grid() -> Vec<(f64, Option<f64>, Option<f64>)> {
    let burdens = [0.0, 1.0e-9, 0.02, 0.5, 3.0, 1.0e6];
    let scars = [
        None,
        Some(0.0),
        Some(1.0e-9),
        Some(1.05),
        Some(40.0),
        Some(1.0e7),
    ];
    let mut grid = Vec::new();
    for &burden in &burdens {
        for &landing in &scars {
            for &tip in &scars {
                grid.push((burden, landing, tip));
            }
        }
    }
    grid
}

#[test]
fn bound_never_exceeds_the_full_score_for_any_non_negative_measurement() {
    let weights = ObjectiveWeights::registered().expect("registered weights");
    let candidates = real_candidate_terms();
    assert!(!candidates.is_empty());
    let mut checked = 0_usize;
    for terms in &candidates {
        let peel = terms.peel_force_n.low_pressure_low_exponent_n.get();
        let bound = terms
            .production_lower_bound(weights, peel)
            .expect("the registered weights certify a bound");
        for (burden, landing, tip) in measurement_grid() {
            let full = terms
                .repriced(weights, burden, peel, landing, tip)
                .score
                .get();
            assert!(
                bound <= full,
                "bound {bound} above full score {full} for burden {burden}, landing {landing:?}, tip {tip:?}"
            );
            checked += 1;
        }
    }
    assert!(checked > 1000, "the grid really ran ({checked})");
}

#[test]
fn bound_is_the_full_score_with_the_measurements_zeroed() {
    let weights = ObjectiveWeights::registered().expect("registered weights");
    for terms in real_candidate_terms() {
        let peel = terms.peel_force_n.low_pressure_low_exponent_n.get();
        let bound = terms
            .production_lower_bound(weights, peel)
            .expect("a bound");
        let zeroed = terms
            .repriced(weights, 0.0, peel, Some(0.0), Some(0.0))
            .score
            .get();
        assert_eq!(
            bound.to_bits(),
            zeroed.to_bits(),
            "the bound is `repriced` with A = l = t = 0, bit for bit"
        );
    }
}

#[test]
fn bound_is_tight_when_nothing_is_added() {
    // A candidate whose production sweep measured a zero burden and no scar
    // scores AT its bound: the bound is attainable, so a tie is possible and
    // `certify_losers` must treat equality as "not certified".
    let weights = ObjectiveWeights::registered().expect("registered weights");
    let terms = real_candidate_terms()[0];
    let peel = terms.peel_force_n.low_pressure_low_exponent_n.get();
    let bound = terms
        .production_lower_bound(weights, peel)
        .expect("a bound");
    let score = terms
        .repriced(weights, 0.0, peel, Some(0.0), Some(0.0))
        .score
        .get();
    assert_eq!(bound.to_bits(), score.to_bits());
}

#[test]
fn bound_is_refused_when_a_weight_is_negative_or_not_finite() {
    let terms = real_candidate_terms()[0];
    let peel = terms.peel_force_n.low_pressure_low_exponent_n.get();
    let weights = ObjectiveWeights::registered().expect("registered weights");
    assert!(terms.production_lower_bound(weights, peel).is_some());
    for bad in [-0.5, f64::NAN, f64::INFINITY] {
        assert!(
            terms
                .production_lower_bound(weights.with_support_area_weight(bad), peel)
                .is_none(),
            "a support weight of {bad} voids the certificate"
        );
        assert!(
            terms
                .production_lower_bound(weights.with_scar_weight(bad), peel)
                .is_none(),
            "a scar weight of {bad} voids the certificate"
        );
    }
}

#[test]
fn certify_losers_stops_nothing_on_a_tie_and_everything_strictly_above() {
    let ids = [10, 11, 12, 13];
    // Winner: index 0 at 5.0. Index 1's bound EQUALS it (a possible tie).
    // Index 2's bound is above. Index 3 has no bound.
    let verdict = certify_losers(
        &ids,
        &[Some(4.0), Some(5.0), Some(5.0 + 1.0e-12), None],
        &[Some(5.0), Some(5.5), None, Some(9.0)],
    );
    assert_eq!(verdict.winner, Some(0));
    assert_eq!(
        verdict.certified,
        vec![None, None, Some(5.0 + 1.0e-12), None]
    );
    assert!(verdict.violations.is_empty());
}

#[test]
fn certify_losers_never_certifies_the_winner_and_breaks_ties_by_candidate_id() {
    // Two finalists finish at exactly the same score: the lower id wins, and
    // the other, whose bound is below, is not certified.
    let verdict = certify_losers(&[7, 3], &[Some(1.0), Some(1.0)], &[Some(2.0), Some(2.0)]);
    assert_eq!(verdict.winner, Some(1), "candidate 3 wins the tie");
    assert_eq!(verdict.certified, vec![None, None]);
    // A bound above its own winner's score is certified, even when that
    // candidate also finished (its measurement is set aside, deterministic).
    let verdict = certify_losers(&[7, 3], &[Some(3.0), Some(1.0)], &[Some(4.0), Some(2.0)]);
    assert_eq!(verdict.winner, Some(1));
    assert_eq!(verdict.certified, vec![Some(3.0), None]);
}

#[test]
fn certify_losers_flags_a_finished_score_below_its_own_bound() {
    // Index 1 finished BELOW its bound: the premise failed. It is a violation,
    // not a certified loser, and its measurement stays.
    let verdict = certify_losers(&[0, 1], &[Some(1.0), Some(9.0)], &[Some(2.0), Some(3.0)]);
    assert_eq!(verdict.winner, Some(0));
    assert_eq!(verdict.certified, vec![None, None]);
    assert_eq!(verdict.violations, vec![1]);
}

#[test]
fn certify_losers_with_nothing_finished_certifies_nothing() {
    let verdict = certify_losers(&[0, 1], &[Some(1.0), Some(2.0)], &[None, None]);
    assert_eq!(verdict.winner, None);
    assert_eq!(verdict.certified, vec![None, None]);
}

fn race_over(bounds: Vec<Option<f64>>) -> ProductionRace {
    ProductionRace::new(
        bounds,
        // The price of a finished pose is its burden, so a test reads the
        // score off the measurement it hands in.
        Box::new(|_, measured| Some(measured.support_area_cm2)),
    )
}

#[test]
fn race_stops_exactly_the_poses_strictly_above_a_finished_score() {
    let race = race_over(vec![Some(4.0), Some(5.0), Some(5.5), None, Some(9.0)]);
    race.finished(0, &measured(5.0, None, None));
    assert!(!race.is_stopped(0), "the finisher is never stopped");
    assert!(
        !race.is_stopped(1),
        "a bound equal to the leader could still tie"
    );
    assert!(race.is_stopped(2));
    assert!(!race.is_stopped(3), "a pose with no bound is never stopped");
    assert!(race.is_stopped(4));
    // A later, better finisher can only stop more.
    race.finished(1, &measured(4.5, None, None));
    assert!(
        !race.is_stopped(0),
        "bound 4.0 is below 4.5: it can still win"
    );
    assert!(race.is_stopped(2) && race.is_stopped(4));
}

#[test]
fn race_never_stops_a_pose_that_can_still_win() {
    // Whatever order poses finish in, a pose is stopped only when some
    // finished score is strictly below its bound.
    let bounds = [Some(3.0), Some(3.5), Some(4.0), Some(6.0)];
    for order in [[0, 1, 2, 3], [3, 2, 1, 0], [1, 3, 0, 2], [2, 0, 3, 1]] {
        let race = race_over(bounds.to_vec());
        let scores = [3.2, 3.6, 4.5, 7.0];
        let mut best = f64::INFINITY;
        for index in order {
            race.finished(index, &measured(scores[index], None, None));
            best = best.min(scores[index]);
            for (other, bound) in bounds.iter().enumerate() {
                // Sound (stopped implies a finished score is strictly below
                // the bound) and complete (every such pose is stopped).
                assert_eq!(
                    race.is_stopped(other),
                    bound.is_some_and(|bound| bound > best),
                    "order {order:?}, pose {other}, best finished {best}"
                );
            }
        }
    }
}

#[test]
fn race_is_poisoned_by_a_negative_measurement_and_stops_nothing_more() {
    let race = race_over(vec![Some(4.0), Some(9.0)]);
    race.finished(0, &measured(-1.0, None, None));
    assert!(race.poisoned());
    assert!(!race.is_stopped(1));
    let race = race_over(vec![Some(4.0), Some(9.0)]);
    race.finished(0, &measured(1.0, Some(-0.5), None));
    assert!(race.poisoned() && !race.is_stopped(1));
    let race = race_over(vec![Some(4.0), Some(9.0)]);
    race.finished(0, &measured(f64::NAN, None, None));
    assert!(race.poisoned() && !race.is_stopped(1));
}

#[test]
fn a_race_that_never_stops_stops_nothing() {
    let race = ProductionRace::never_stops(3);
    race.finished(0, &measured(0.0, None, None));
    assert!((0..3).all(|index| !race.is_stopped(index)));
}

// ---------------------------------------------------------------------------
// End to end through `search`.
// ---------------------------------------------------------------------------

/// Reports a scripted production burden per finalist index (the finalists
/// arrive in ranking order), the prefilter burden 0 for every pose.
/// `honour_race` makes it run the poses one at a time in `order`, telling the
/// race as each finishes and skipping any pose already stopped, as the real
/// measurer does on its threads; without it the trait default measures all.
struct Scripted {
    production_burden: Vec<f64>,
    /// The burden every pose reports at the prefilter. A bound that read the
    /// prefilter's burden would see this, and only this, wrongly.
    prefilter_burden: f64,
    order: Vec<usize>,
    honour_race: bool,
    race_calls: std::cell::Cell<usize>,
    stopped: std::cell::RefCell<Vec<usize>>,
    /// The finalists' directions, in the order the batch was handed them.
    directions: std::cell::RefCell<Vec<[f64; 3]>>,
}

impl Scripted {
    fn new(production_burden: Vec<f64>, order: Vec<usize>, honour_race: bool) -> Self {
        Self {
            production_burden,
            prefilter_burden: 0.0,
            order,
            honour_race,
            race_calls: std::cell::Cell::new(0),
            stopped: std::cell::RefCell::new(Vec::new()),
            directions: std::cell::RefCell::new(Vec::new()),
        }
    }

    fn with_prefilter_burden(mut self, burden: f64) -> Self {
        self.prefilter_burden = burden;
        self
    }

    /// THE ORACLE: the winner a batch with no early stop would pick, from the
    /// record's own entries. Every finalist (stopped or not) is priced as
    /// `repriced` prices a finished pose, with the burden the script gave it;
    /// the lowest score wins, ties to the lower candidate id. It reads no
    /// certified-loser field, so it cannot agree with the search by sharing
    /// its mistake.
    fn oracle_winner(&self, record: &DenseSweepRecord) -> usize {
        let weights = ObjectiveWeights::registered().expect("registered weights");
        let commit = record.tiers.last().expect("a commit tier");
        let directions = self.directions.borrow();
        let mut priced: Vec<(f64, usize)> = Vec::new();
        for (index, direction) in directions.iter().enumerate() {
            let entry = commit
                .ranking
                .iter()
                .find(|entry| {
                    entry
                        .up_model_frame
                        .iter()
                        .zip(direction)
                        .all(|(a, b)| a.get().to_bits() == b.to_bits())
                })
                .expect("a finalist is in the commit ranking");
            let peel = entry.terms.peel_force_n.low_pressure_low_exponent_n.get();
            let score = entry
                .terms
                .repriced(weights, self.production_burden[index], peel, None, None)
                .score
                .get();
            priced.push((score, entry.candidate));
        }
        priced
            .into_iter()
            .min_by(|left, right| left.0.total_cmp(&right.0).then(left.1.cmp(&right.1)))
            .map(|(_, candidate)| candidate)
            .expect("at least one finalist")
    }
}

impl SupportDemandMeasurer for Scripted {
    fn measure_support_demand(
        &self,
        _mesh: &Mesh,
        _table: &OrientFaceTable,
        candidates: &[UpDirection],
        fidelity: DemandFidelity,
    ) -> Vec<Result<MeasuredSupportDemand, String>> {
        candidates
            .iter()
            .enumerate()
            .map(|(index, _)| {
                let burden = match fidelity {
                    DemandFidelity::Prefilter => self.prefilter_burden,
                    DemandFidelity::Production => self.production_burden[index],
                };
                let mut one = measured(burden, None, None);
                one.fidelity = fidelity;
                Ok(one)
            })
            .collect()
    }

    fn measure_production_race(
        &self,
        mesh: &Mesh,
        table: &OrientFaceTable,
        candidates: &[UpDirection],
        race: &ProductionRace,
    ) -> Vec<RaceOutcome> {
        self.race_calls.set(self.race_calls.get() + 1);
        *self.directions.borrow_mut() = candidates.iter().map(|up| up.get()).collect();
        if !self.honour_race {
            return self
                .measure_support_demand(mesh, table, candidates, DemandFidelity::Production)
                .into_iter()
                .map(|result| RaceOutcome::Measured(result.expect("the script never fails")))
                .collect();
        }
        let mut outcomes: Vec<Option<RaceOutcome>> = vec![None; candidates.len()];
        for &index in &self.order {
            if race.is_stopped(index) {
                race.note_stopped(index);
                self.stopped.borrow_mut().push(index);
                outcomes[index] = Some(RaceOutcome::Stopped);
                continue;
            }
            let one = measured(self.production_burden[index], None, None);
            race.finished(index, &one);
            outcomes[index] = Some(RaceOutcome::Measured(one));
        }
        outcomes
            .into_iter()
            .map(|outcome| outcome.expect("every pose is in the order"))
            .collect()
    }
}

fn finalist_count(record: &DenseSweepRecord) -> usize {
    record
        .commit_tier_demand
        .iter()
        .filter(|row| row.measured_at_production() || row.certified_loser.is_some())
        .count()
}

fn run(measurer: &Scripted) -> DenseSweepRecord {
    let mesh = make_overhang_table().expect("overhang table fixture");
    let request = SweepRequest::registered().expect("registered sweep configuration");
    search(&mesh, &request, Some(measurer))
        .expect("a scorable fixture")
        .into_record()
}

/// A zero production burden for every finalist: each scores AT its bound, so
/// the commit-tier scores of the finalists ARE the bounds.
fn finalist_bounds() -> Vec<(usize, f64)> {
    let count = SweepRequest::registered()
        .expect("registered")
        .ladder
        .commit_top_k();
    let record = run(&Scripted::new(
        vec![0.0; count],
        (0..count).collect(),
        false,
    ));
    let commit = record.tiers.last().expect("a commit tier");
    commit
        .ranking
        .iter()
        .take(count)
        .map(|entry| (entry.candidate, entry.terms.score.get()))
        .collect()
}

#[test]
fn the_record_does_not_depend_on_finish_order() {
    let bounds = finalist_bounds();
    let count = bounds.len();
    assert!(count >= 3, "the fixture promotes at least three finalists");
    // The finalist the zero-burden run ranks first has the lowest bound. Give
    // it burden 0 and every other finalist a heavy burden, so it wins and the
    // others have bounds strictly above its score.
    let mut burden = vec![50.0; count];
    burden[0] = 0.0;
    let forward: Vec<usize> = (0..count).collect();
    let backward: Vec<usize> = (0..count).rev().collect();

    let plain_measurer = Scripted::new(burden.clone(), forward.clone(), false);
    let plain = run(&plain_measurer);
    let early = Scripted::new(burden.clone(), forward, true);
    let late = Scripted::new(burden, backward, true);
    let raced_early = run(&early);
    let raced_late = run(&late);

    // The winner and its terms are the plain batch's, and the one an
    // independent pricing of every finalist picks.
    assert_eq!(raced_early.winner, plain.winner);
    assert_eq!(raced_late.winner, plain.winner);
    assert_eq!(
        raced_early.winner.candidate,
        early.oracle_winner(&raced_early)
    );
    assert_eq!(raced_late.winner.candidate, late.oracle_winner(&raced_late));

    // The two orders stop different poses (forward stops the losers; backward
    // finishes the winner last so nothing is stopped), and the RECORDS are
    // still equal, digest included.
    assert!(
        !early.stopped.borrow().is_empty(),
        "forward order: the winner finishes first and its losers are stopped"
    );
    assert!(
        late.stopped.borrow().is_empty(),
        "backward order: the winner finishes last, nothing was running to stop"
    );
    assert_eq!(
        raced_early, raced_late,
        "the record is a function of the inputs"
    );
    assert_eq!(raced_early.search_digest, raced_late.search_digest);

    // The certified set is exactly the finalists whose bound is strictly above
    // the winner's score, and no other.
    let winner_score = raced_early.winner.terms.score.get();
    let commit = raced_early.tiers.last().expect("a commit tier");
    let certified: Vec<usize> = commit
        .ranking
        .iter()
        .filter(|entry| entry.certified_loser.is_some())
        .map(|entry| entry.candidate)
        .collect();
    let expected: Vec<usize> = bounds
        .iter()
        .skip(1)
        .filter(|(_, bound)| *bound > winner_score)
        .map(|(candidate, _)| *candidate)
        .collect();
    let mut certified_sorted = certified.clone();
    certified_sorted.sort_unstable();
    let mut expected_sorted = expected;
    expected_sorted.sort_unstable();
    assert_eq!(certified_sorted, expected_sorted);
    assert!(!certified.is_empty(), "something was certified");
    assert_eq!(finalist_count(&raced_early), count);

    // A certified loser says so everywhere it is read: its row stays at the
    // prefilter fidelity, carries the bound, and ranks behind the winner.
    for entry in &commit.ranking {
        let Some(loser) = entry.certified_loser else {
            continue;
        };
        assert!(loser.lower_bound_score.get() > winner_score);
        let row = raced_early
            .commit_tier_demand
            .iter()
            .find(|row| row.candidate == entry.candidate)
            .expect("a row");
        assert_eq!(row.certified_loser, Some(loser));
        assert_eq!(row.fidelity.as_deref(), Some("prefilter"));
        assert!(entry.rank > 0, "the winner is first");
    }
    assert_eq!(commit.ranking[0].candidate, raced_early.winner.candidate);
    assert!(commit.ranking[0].certified_loser.is_none());
}

#[test]
fn a_certified_runner_up_has_no_score_only_a_bound() {
    let count = SweepRequest::registered()
        .expect("registered")
        .ladder
        .commit_top_k();
    let mut burden = vec![50.0; count];
    burden[0] = 0.0;
    let record = run(&Scripted::new(burden, (0..count).collect(), true));
    let runner_up = record.runner_up.as_ref().expect("a runner-up");
    let loser = runner_up
        .certified_loser
        .expect("the runner-up is a certified loser");
    assert_eq!(
        record.fidelity.winner_runner_up_margin, None,
        "its score is unknown"
    );
    let margin_bound = record
        .fidelity
        .winner_runner_up_margin_lower_bound
        .expect("a lower bound on the margin")
        .get();
    assert!(margin_bound > 0.0);
    assert_eq!(
        margin_bound.to_bits(),
        (loser.lower_bound_score.get() - record.winner.terms.score.get()).to_bits()
    );
    assert!(
        record.weight_flips.is_empty(),
        "no crossover against an unknown score"
    );
    // Every corner the certified loser might have won is listed, never
    // silently decided.
    let robustness = &record.peel_robustness;
    for (label, winner) in &robustness.winner_at_each_corner {
        if robustness.undetermined_corners.contains(label) {
            assert_eq!(*winner, None);
        } else {
            assert!(winner.is_some());
        }
    }
    assert_eq!(
        robustness.agrees_at_every_corner,
        robustness.disagreeing_corners.is_empty() && robustness.undetermined_corners.is_empty()
    );
}

#[test]
fn a_measurer_that_ignores_the_race_makes_the_same_record() {
    // The certified set is recomputed from the winner's score after the
    // batch, so a measurer that stops nothing (the trait default) and one
    // that stops what it can make one record.
    let count = SweepRequest::registered()
        .expect("registered")
        .ladder
        .commit_top_k();
    let plain = run(&Scripted::new(
        vec![0.0; count],
        (0..count).collect(),
        false,
    ));
    let raced = run(&Scripted::new(vec![0.0; count], (0..count).collect(), true));
    assert_eq!(raced.winner, plain.winner);
    assert_eq!(
        raced, plain,
        "a zero-burden batch: same record, same digest"
    );
}

#[test]
fn the_winner_is_never_the_stopped_pose_under_any_burdens_and_orders() {
    let count = SweepRequest::registered()
        .expect("registered")
        .ladder
        .commit_top_k();
    let burdens: Vec<Vec<f64>> = vec![
        vec![0.0; count],
        (0..count).map(|index| index as f64 * 0.3).collect(),
        (0..count)
            .map(|index| (count - index) as f64 * 0.3)
            .collect(),
        (0..count)
            .map(|index| if index == count - 1 { 0.0 } else { 20.0 })
            .collect(),
        (0..count)
            .map(|index| if index == 1 { 0.0 } else { 20.0 })
            .collect(),
    ];
    let orders: Vec<Vec<usize>> = vec![
        (0..count).collect(),
        (0..count).rev().collect(),
        (0..count).map(|index| (index + 1) % count).collect(),
    ];
    for burden in &burdens {
        let plain = run(&Scripted::new(burden.clone(), (0..count).collect(), false));
        for order in &orders {
            let measurer = Scripted::new(burden.clone(), order.clone(), true);
            let raced = run(&measurer);
            assert_eq!(
                raced.winner, plain.winner,
                "burden {burden:?}, order {order:?}"
            );
            assert_eq!(
                raced.winner.candidate,
                measurer.oracle_winner(&raced),
                "burden {burden:?}, order {order:?}: the winner is the true minimum"
            );
            let again = run(&Scripted::new(burden.clone(), orders[1].clone(), true));
            assert_eq!(
                raced, again,
                "burden {burden:?}: the record ignores the order"
            );
        }
    }
}

#[test]
fn a_better_pose_is_never_stopped_by_a_worse_one_that_finished_first() {
    // The prefilter priced every pose's burden high; the production sweep
    // finds every pose's burden is zero. The pose with the lowest price must
    // win, although the worse pose finishes FIRST. A bound built from the
    // prefilter's burden (a measured term of a coarser raster) would stop the
    // better pose on the strength of a number the production sweep retracts.
    let count = SweepRequest::registered()
        .expect("registered")
        .ladder
        .commit_top_k();
    let order: Vec<usize> = (1..count).chain(std::iter::once(0)).collect();
    let measurer = Scripted::new(vec![0.0; count], order, true).with_prefilter_burden(40.0);
    let record = run(&measurer);
    assert_eq!(
        record.winner.candidate,
        measurer.oracle_winner(&record),
        "the winner is the true minimum, not the first to finish"
    );
    let commit = record.tiers.last().expect("a commit tier");
    // The finalist that sorts first at the prefilter has the lowest price and
    // wins at equal production burdens.
    assert_eq!(commit.ranking[0].candidate, record.winner.candidate);
    assert!(
        measurer
            .stopped
            .borrow()
            .iter()
            .all(|&index| index != 0 || count == 1),
        "the best-priced finalist (index 0) is never the one stopped: {:?}",
        measurer.stopped.borrow()
    );
}

#[test]
fn sabotage_a_measured_term_in_the_bound() {
    // SABOTAGE DRILL. A bound that includes a measured term is no longer a
    // bound: here, one that adds the burden's price back in (what a bound
    // built from the prefilter score would do). The bound test goes red on
    // the same grid, which is what shows the real test can fail.
    let weights = ObjectiveWeights::registered().expect("registered weights");
    let mut broke = 0_usize;
    for terms in real_candidate_terms() {
        let peel = terms.peel_force_n.low_pressure_low_exponent_n.get();
        let honest = terms
            .production_lower_bound(weights, peel)
            .expect("a bound");
        // The sabotaged bound: priced at a measured burden of 3.0 cm2.
        let sabotaged = terms
            .repriced(weights, 3.0, peel, Some(0.0), Some(0.0))
            .score
            .get();
        assert!(sabotaged > honest);
        for (burden, landing, tip) in measurement_grid() {
            let full = terms
                .repriced(weights, burden, peel, landing, tip)
                .score
                .get();
            if sabotaged > full {
                broke += 1;
            }
        }
    }
    assert!(
        broke > 0,
        "a bound with a measured term is above some real score"
    );
}

#[test]
fn the_tokens_start_clear_and_are_the_races_own() {
    let race = race_over(vec![Some(1.0), Some(2.0)]);
    for index in 0..2 {
        assert!(!race.token(index).expect("a token").load(Ordering::Acquire));
    }
    assert!(race.token(2).is_none());
    // `commit_terms` is exercised so a fixture without a commit tier is loud.
    let mesh = make_overhang_table().expect("overhang table fixture");
    let request = SweepRequest::registered().expect("registered sweep configuration");
    let record = search(&mesh, &request, None)
        .expect("a scorable fixture")
        .into_record();
    assert!(!commit_terms(&record).is_empty());
}

#[test]
fn a_record_that_certified_nothing_is_byte_for_byte_the_old_record() {
    // The new fields are omitted from the serialization when empty, so the
    // record (and the digest over it) of every search that stops nothing is
    // the one the search made before this lane.
    let mesh = make_overhang_table().expect("overhang table fixture");
    let request = SweepRequest::registered().expect("registered sweep configuration");
    let record = search(&mesh, &request, None)
        .expect("a scorable fixture")
        .into_record();
    let json = serde_json::to_string(&record).expect("serializable");
    for key in [
        "certified_loser",
        "undetermined_corners",
        "winner_runner_up_margin_lower_bound",
    ] {
        assert!(
            !json.contains(key),
            "{key} must be absent when nothing was certified"
        );
    }
}

// ---------------------------------------------------------------------------
// The diagnostics read the certificate (review defect, gpt-sol-high).
// ---------------------------------------------------------------------------

fn diagnose(measurer: &Scripted) -> (DenseSweepRecord, crate::diag::SweepDiagnostics) {
    let mesh = make_overhang_table().expect("overhang table fixture");
    let request = SweepRequest::registered().expect("registered sweep configuration");
    let (outcome, diagnostics) = crate::search::search_with_diagnostics(
        &mesh,
        &request,
        Some(measurer),
        &crate::diag::DiagnosticsConfig::default(),
    )
    .expect("a scorable fixture");
    (outcome.into_record(), diagnostics)
}

#[test]
fn a_certified_runner_up_keeps_its_certificate_in_the_diagnostics() {
    use crate::diag::CutBasis;

    let count = SweepRequest::registered()
        .expect("registered")
        .ladder
        .commit_top_k();
    let mut burden = vec![50.0; count];
    burden[0] = 0.0;
    let (record, d) = diagnose(&Scripted::new(burden, (0..count).collect(), true));
    let runner_up = record.runner_up.as_ref().expect("a runner-up");
    let loser = runner_up
        .certified_loser
        .expect("the runner-up is a certified loser");

    // The after-production snapshot row keeps the certificate.
    let after = d.snapshots.after_production.as_ref().expect("after production");
    let row = after
        .iter()
        .find(|row| row.candidate == runner_up.candidate)
        .expect("the runner-up is in the snapshot");
    assert_eq!(row.certified_loser, Some(loser));
    assert!(!row.measured_at_production, "its burden is the prefilter's");
    assert!(after[0].certified_loser.is_none(), "the winner is not certified");

    // C4 names the certificate, not production fidelity, and its gap is the
    // bound over the winner's score, as `rerank_commit` compares them.
    let c4 = d.cuts.iter().find(|cut| cut.name == "C4").expect("C4");
    assert_eq!(c4.decided_by, CutBasis::CertifiedBound);
    let rejected = c4.first_rejected.as_ref().expect("the runner-up");
    assert_eq!(rejected.candidate, runner_up.candidate);
    assert_eq!(rejected.certified_loser, Some(loser));
    let gap = c4.score_gap.expect("a bound gap");
    assert_eq!(
        gap.to_bits(),
        (loser.lower_bound_score.get() - record.winner.terms.score.get()).to_bits()
    );
    assert!(gap > 0.0, "a certificate is strictly above the winner");

    // The serialized row carries the key only because it has the field.
    let json = serde_json::to_string(rejected).expect("serializable");
    assert!(json.contains("certified_loser"));
    assert!(serde_json::to_string(c4.last_kept.as_ref().expect("winner"))
        .is_ok_and(|winner| !winner.contains("certified_loser")));
}

#[test]
fn diagnostics_of_a_search_that_certified_nothing_have_no_certified_key() {
    // No measurer: no production batch, so nothing is certified.
    let mesh = make_overhang_table().expect("overhang table fixture");
    let request = SweepRequest::registered().expect("registered sweep configuration");
    let (_, d) = crate::search::search_with_diagnostics(
        &mesh,
        &request,
        None,
        &crate::diag::DiagnosticsConfig::default(),
    )
    .expect("a scorable fixture");
    assert!(!d.cuts.is_empty());
    let json = serde_json::to_string(&d).expect("serializable");
    assert!(
        !json.contains("certified_loser"),
        "no certificate means no new key"
    );
}
