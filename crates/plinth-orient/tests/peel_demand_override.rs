//! LANE 3 (`docs/plans/slice-sweep-core-2026-09-02.md` §12): the commit-tier
//! real-burden override, its two fidelities, its
//! emit-and-warn-never-refuse fallback, and the peel-force bracket's
//! robustness/flip-crossover records, exercised end to end through a
//! test-local `SupportDemandMeasurer`.
//!
//! `plinth-orient` cannot depend on `plinth-sweep`'s real `SweepDemand`
//! (see `plinth_orient::demand`'s own module doc for the cycle this avoids),
//! so this is the interface seam's own test double; `plinth-sweep`'s
//! `tests/orient_demand.rs` exercises the real implementation.

use std::cell::RefCell;

use plinth_mesh::{make_overhang_table, Mesh};
use plinth_orient::demand::{DemandFidelity, MeasuredSupportDemand, SupportDemandMeasurer};
use plinth_orient::search::{search, SweepRequest};
use plinth_orient::{OrientFaceTable, UpDirection};

fn measured(support_area_cm2: f64, fidelity: DemandFidelity) -> MeasuredSupportDemand {
    MeasuredSupportDemand {
        support_area_cm2,
        struts: 1,
        // The double never places a brace or a mast: it is a test-local stand-in
        // for the trait's contract, not a real sweep.
        braces: 0,
        masts: 0,
        total_scar_mm2: 0.25,
        achieved_pitch_mm: match fidelity {
            DemandFidelity::Prefilter => 0.5,
            DemandFidelity::Production => 0.05,
        },
        layers: 10,
        residual_demand_cells: 0,
        uncarried_load_mm2: 0.0,
        // The double reports no landing at all: a stated absence, which is
        // what a measurer that never looked at a support field must say.
        model_landings: 0,
        landing_scar_raw_mm2: None,
        // The double reports no tip contact either, for the same reason.
        tip_scar_raw_mm2: None,
        fidelity,
    }
}

/// Reports a FIXED burden for every candidate at either fidelity, so the
/// override's effect on `commit_tier_demand` and the winner's own recorded
/// terms is exactly known rather than dependent on real sweep geometry.
struct FixedDemand(f64);

impl SupportDemandMeasurer for FixedDemand {
    fn measure_support_demand(
        &self,
        _mesh: &Mesh,
        _table: &OrientFaceTable,
        candidates: &[UpDirection],
        fidelity: DemandFidelity,
    ) -> Vec<Result<MeasuredSupportDemand, String>> {
        candidates
            .iter()
            .map(|_| Ok(measured(self.0, fidelity)))
            .collect()
    }
}

struct AlwaysFails;

impl SupportDemandMeasurer for AlwaysFails {
    fn measure_support_demand(
        &self,
        _mesh: &Mesh,
        _table: &OrientFaceTable,
        candidates: &[UpDirection],
        _fidelity: DemandFidelity,
    ) -> Vec<Result<MeasuredSupportDemand, String>> {
        candidates
            .iter()
            .map(|_| Err("synthetic failure".to_owned()))
            .collect()
    }
}

/// Cheap at the prefilter, EXPENSIVE at the production pitch, and it counts
/// how many poses each stage was handed. A production measurement that made
/// its poses look worse is exactly the case where a naive score-only sort
/// would hand the win to a candidate nobody swept properly.
#[derive(Default)]
struct PricierAtProduction {
    batches: RefCell<Vec<(DemandFidelity, usize)>>,
}

impl SupportDemandMeasurer for PricierAtProduction {
    fn measure_support_demand(
        &self,
        _mesh: &Mesh,
        _table: &OrientFaceTable,
        candidates: &[UpDirection],
        fidelity: DemandFidelity,
    ) -> Vec<Result<MeasuredSupportDemand, String>> {
        self.batches.borrow_mut().push((fidelity, candidates.len()));
        let burden = match fidelity {
            DemandFidelity::Prefilter => 0.0,
            DemandFidelity::Production => 100.0,
        };
        candidates
            .iter()
            .map(|_| Ok(measured(burden, fidelity)))
            .collect()
    }
}

#[test]
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn the_commit_tier_override_replaces_the_proxy_and_is_recorded() {
    let mesh = make_overhang_table().expect("overhang table fixture");
    let request = SweepRequest::registered().expect("registered sweep configuration");
    let measurer = FixedDemand(0.0);
    let outcome = search(&mesh, &request, Some(&measurer)).expect("a scorable fixture");
    let record = outcome.record();

    assert!(
        !record.commit_tier_demand.is_empty(),
        "every commit-tier candidate should have been measured"
    );
    for entry in &record.commit_tier_demand {
        assert_eq!(
            entry.measurement_error, None,
            "the fixed measurer never fails"
        );
        let value = entry
            .measured_support_area_cm2
            .expect("a successful measurement")
            .get();
        assert_eq!(value, 0.0, "the fixed measurer always reports zero");
        assert!(
            entry.fidelity.is_some(),
            "a measured row always names the raster it was measured on"
        );
    }
    // The winner's own recorded term is the OVERRIDDEN value, not the proxy
    // `score_candidates` computed from the down-facing projected area.
    assert_eq!(record.winner.terms.support_area_cm2.get(), 0.0);
}

#[test]
fn only_the_commit_top_k_finalists_reach_the_production_pitch() {
    let mesh = make_overhang_table().expect("overhang table fixture");
    let request = SweepRequest::registered().expect("registered sweep configuration");
    let measurer = PricierAtProduction::default();
    let outcome = search(&mesh, &request, Some(&measurer)).expect("a scorable fixture");
    let record = outcome.record();

    let batches = measurer.batches.borrow().clone();
    assert_eq!(batches.len(), 2, "one prefilter batch, then one production batch");
    assert_eq!(batches[0].0, DemandFidelity::Prefilter);
    assert_eq!(batches[1].0, DemandFidelity::Production);
    assert_eq!(
        batches[0].1,
        record.commit_tier_demand.len(),
        "the prefilter measures every admitted commit-tier candidate"
    );
    let expected_finalists = request.ladder.commit_top_k().min(batches[0].1);
    assert_eq!(
        batches[1].1, expected_finalists,
        "N is ORIENT_COMMIT_TOP_K, the search's own commit width, not a number of its own"
    );
    assert!(
        batches[1].1 < batches[0].1,
        "the prefilter must actually prune, or the two-stage split buys nothing"
    );

    let production_rows = record
        .commit_tier_demand
        .iter()
        .filter(|row| row.fidelity.as_deref() == Some("production"))
        .count();
    assert_eq!(production_rows, expected_finalists);
    for row in &record.commit_tier_demand {
        assert!(
            row.prefilter_support_area_cm2.is_some(),
            "every row keeps the prefilter number it was pruned by"
        );
    }
}

#[test]
fn a_pose_measured_only_at_the_prefilter_pitch_may_not_win() {
    let mesh = make_overhang_table().expect("overhang table fixture");
    let request = SweepRequest::registered().expect("registered sweep configuration");
    // Prefilter burden 0, production burden 100: every finalist is scored
    // WORSE than every pose that was only prefiltered. A score-only sort
    // would therefore crown a coarse-raster measurement -- the exact error
    // the fidelity precedence exists to prevent, and the reason the coarse
    // raster's own arm counts (which rank a disc's 180 deg flip backwards)
    // are not allowed to decide.
    let measurer = PricierAtProduction::default();
    let outcome = search(&mesh, &request, Some(&measurer)).expect("a scorable fixture");
    let record = outcome.record();

    let winner_row = record
        .commit_tier_demand
        .iter()
        .find(|row| row.candidate == record.winner.candidate)
        .expect("the winner was measured");
    assert_eq!(
        winner_row.fidelity.as_deref(),
        Some("production"),
        "the winner must carry a production-pitch burden even when that made it look worse"
    );
    if let Some(runner_up) = &record.runner_up {
        let runner_row = record
            .commit_tier_demand
            .iter()
            .find(|row| row.candidate == runner_up.candidate)
            .expect("the runner-up was measured");
        assert_eq!(
            runner_row.fidelity.as_deref(),
            Some("production"),
            "the flip-crossover compares two poses, so both must be measured on the same raster"
        );
    }
}

#[test]
fn a_measurement_failure_falls_back_to_the_proxy_and_is_named_never_refused() {
    let mesh = make_overhang_table().expect("overhang table fixture");
    let request = SweepRequest::registered().expect("registered sweep configuration");
    let outcome = search(&mesh, &request, Some(&AlwaysFails))
        .expect("a demand-measurement failure must never refuse the search");
    let record = outcome.record();

    assert!(!record.commit_tier_demand.is_empty());
    for entry in &record.commit_tier_demand {
        assert_eq!(entry.measured_support_area_cm2, None);
        assert_eq!(
            entry.measurement_error.as_deref(),
            Some("synthetic failure")
        );
        assert!(entry.proxy_support_area_cm2.get().is_finite());
    }
    // The proxy value is what actually scored the winner -- an override that
    // always fails must reproduce the `None`-demand objective exactly.
    let unoverridden = search(&mesh, &request, None).expect("a scorable fixture");
    assert_eq!(
        record.winner.candidate,
        unoverridden.record().winner.candidate,
        "a demand measurer that always fails must change nothing about the winner"
    );
}

#[test]
fn the_peel_bracket_robustness_and_weight_flips_are_populated() {
    let mesh = make_overhang_table().expect("overhang table fixture");
    let request = SweepRequest::registered().expect("registered sweep configuration");
    let outcome = search(&mesh, &request, None).expect("a scorable fixture");
    let record = outcome.record();

    assert_eq!(
        record.peel_robustness.winner_at_each_corner.len(),
        4,
        "all four bracket corners (D14) must be named"
    );
    for (_, winner) in &record.peel_robustness.winner_at_each_corner {
        assert!(winner.is_some(), "the commit tier always has a ranking");
    }
    // Every price term this crate's objective has gets a flip-crossover
    // record (D15) whenever a runner-up exists to flip toward.
    if record.runner_up.is_some() {
        assert_eq!(record.weight_flips.len(), 4);
        for flip in &record.weight_flips {
            assert!(flip.registered_value.get() > 0.0);
        }
    }
}
