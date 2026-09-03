//! LANE 3 (`docs/plans/slice-sweep-core-2026-09-02.md` §12): the commit-tier
//! real-demand override, its emit-and-warn-never-refuse fallback, and the
//! peel-force bracket's robustness/flip-crossover records, exercised end to
//! end through a test-local `SupportDemandMeasurer`.
//!
//! `plinth-orient` cannot depend on `plinth-sweep`'s real `CoarseSweepDemand`
//! (see `plinth_orient::demand`'s own module doc for the cycle this avoids),
//! so this is the interface seam's own test double; `plinth-sweep`'s
//! `tests/orient_demand.rs` exercises the real implementation.

use plinth_mesh::{make_overhang_table, Mesh};
use plinth_orient::demand::{MeasuredSupportDemand, SupportDemandMeasurer};
use plinth_orient::search::{search, SweepRequest};
use plinth_orient::UpDirection;

/// Reports a FIXED `support_area_cm2` for every candidate, so the override's
/// effect on `commit_tier_demand` and the winner's own recorded terms is
/// exactly known rather than dependent on real sweep geometry.
struct FixedDemand(f64);

impl SupportDemandMeasurer for FixedDemand {
    fn measure_support_area_cm2(
        &self,
        _mesh: &Mesh,
        _candidate: UpDirection,
    ) -> Result<MeasuredSupportDemand, String> {
        Ok(MeasuredSupportDemand {
            support_area_cm2: self.0,
            struts: 1,
            achieved_pitch_mm: 0.5,
            layers: 10,
            residual_demand_cells: 0,
            uncarried_load_mm2: 0.0,
        })
    }
}

struct AlwaysFails;

impl SupportDemandMeasurer for AlwaysFails {
    fn measure_support_area_cm2(
        &self,
        _mesh: &Mesh,
        _candidate: UpDirection,
    ) -> Result<MeasuredSupportDemand, String> {
        Err("synthetic failure".to_owned())
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
        let measured = entry
            .measured_support_area_cm2
            .expect("a successful measurement")
            .get();
        assert_eq!(measured, 0.0, "the fixed measurer always reports zero");
    }
    // The winner's own recorded term is the OVERRIDDEN value, not the proxy
    // `score_candidates` computed from the down-facing projected area.
    assert_eq!(record.winner.terms.support_area_cm2.get(), 0.0);
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
