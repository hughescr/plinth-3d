use plinth_core::{
    canonical_f64_bits, digest_f64_slice, AcceptSafe, CostVector, Defect, DefectReport, Derived,
    DigestError, Fidelity, LayerPlan, LayerPlanError, Measured, MetricId, MetricSet, MetricValue,
    ModelId, OptimizationSummary, OrientationCost, OrientationId, ParetoPoint, Pending, PlateId,
    Resin, Risk, Scar, Severity, SiteId, StageId, Termination, ViolationKind,
};

#[test]
fn only_blocking_defects_reject_a_report_independent_of_metrics_and_search() {
    let cost = CostVector::new(
        42.0,
        1_200.0,
        Resin::new(3.0, 0.5, 0.0),
        Scar::new(1.0, 2.0, 0.2),
        Risk::new(5.0, 0, 8.0),
        Fidelity::Exact,
    );
    let search = plinth_core::SearchEvidence::Evaluated(OptimizationSummary::new(
        OrientationCost::new(OrientationId::from("orient-1"), cost),
        24,
        vec![ParetoPoint::new(OrientationId::from("orient-1"), cost)],
    ));
    let metrics = MetricSet::new([(MetricId::from("boundary_edges"), MetricValue::new(0.0))]);
    let report = DefectReport::new(
        ModelId::from("treant"),
        vec![Defect {
            kind: ViolationKind::ScarCapExceeded,
            site: SiteId::Face(9),
            severity: Severity::Warning,
            measure: 2.0,
            detail: "visible but waived by policy outside this type".into(),
        }],
        metrics.clone(),
        StageId::Audit,
        Termination::Converged,
        search.clone(),
    );
    assert!(report.accepted());

    let rejected = DefectReport::new(
        report.subject.clone(),
        vec![Defect {
            severity: Severity::Blocking,
            ..report.defects[0].clone()
        }],
        metrics,
        StageId::Audit,
        Termination::Converged,
        search,
    );
    assert!(!rejected.accepted());
}

#[test]
fn measured_none_error_is_json_null() {
    let measured = Measured::new(0.05_f64, PlateId::from("plate-6"), None, "caliper");
    let json = serde_json::to_value(measured).unwrap();
    assert!(json.get("err").unwrap().is_null());
}

#[test]
fn pending_resolves_to_accept_safe_derived_conservative_value() {
    let pending = Pending::new(12.0_f64, 18.0, PlateId::from("plate-7"), "load limit");
    let resolved: Derived<f64> = pending.resolve();
    assert_eq!(resolved.get(), 12.0);
    assert!(resolved.note.contains("conservatively"));
}

#[test]
fn layer_plan_is_per_layer_and_binary_searches_boundaries() {
    let height = Measured::new(0.05_f64, PlateId::from("plate-6"), Some(0.001), "caliper");
    let plan = LayerPlan::constant(&height, 3).unwrap();
    assert_eq!(plan.count(), 3);
    assert_eq!(plan.z_of(2), Some(0.1));
    assert_eq!(plan.index_at(0.049), Some(0));
    assert_eq!(plan.index_at(0.05), Some(1));
    assert_eq!(plan.index_at(plan.z_of(plan.count()).unwrap()), None);
}

#[test]
fn layer_plan_rejects_a_nonfinite_origin() {
    assert_eq!(
        LayerPlan::new(f64::NAN, vec![0.05]),
        Err(LayerPlanError::NonFiniteOrigin)
    );
}

#[test]
fn digest_canonicalizes_negative_zero() {
    assert_eq!(canonical_f64_bits(-0.0), canonical_f64_bits(0.0));
    assert_eq!(
        digest_f64_slice(&[-0.0]).unwrap(),
        digest_f64_slice(&[0.0]).unwrap()
    );
}

#[test]
fn digest_rejects_every_nonfinite_value() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            digest_f64_slice(&[1.0, value]),
            Err(DigestError::NonFinite { index: 1 })
        );
    }
}

#[test]
fn no_search_evidence_uses_the_explicit_variant() {
    let report = DefectReport::new(
        ModelId::from("unoriented"),
        vec![],
        MetricSet::default(),
        StageId::Triage,
        Termination::Converged,
        plinth_core::SearchEvidence::NotEvaluated,
    );
    assert!(matches!(
        report.search,
        plinth_core::SearchEvidence::NotEvaluated
    ));
}

#[test]
fn evaluated_search_always_has_a_selected_cost() {
    let cost = CostVector::new(
        42.0,
        1_200.0,
        Resin::new(3.0, 0.5, 0.0),
        Scar::new(1.0, 2.0, 0.2),
        Risk::new(5.0, 0, 8.0),
        Fidelity::Exact,
    );
    let search = plinth_core::SearchEvidence::Evaluated(OptimizationSummary::new(
        OrientationCost::new(OrientationId::from("orient-1"), cost),
        24,
        vec![ParetoPoint::new(OrientationId::from("orient-1"), cost)],
    ));
    let plinth_core::SearchEvidence::Evaluated(summary) = search else {
        panic!("the test constructs evaluated evidence");
    };
    assert_eq!(summary.selected().cost(), cost);
    assert_eq!(summary.candidates_examined(), 24);

    let json = serde_json::to_value(plinth_core::SearchEvidence::Evaluated(summary)).unwrap();
    assert_eq!(json["status"], "evaluated");
    assert_eq!(json["summary"]["candidates_examined"], 24);
}

#[test]
fn search_evidence_json_round_trips_both_variants() {
    let not_evaluated = plinth_core::SearchEvidence::NotEvaluated;
    let not_evaluated_json = serde_json::to_string(&not_evaluated).unwrap();
    assert_eq!(
        serde_json::from_str::<plinth_core::SearchEvidence>(&not_evaluated_json).unwrap(),
        not_evaluated
    );

    let cost = CostVector::new(
        42.0,
        1_200.0,
        Resin::new(3.0, 0.5, 0.0),
        Scar::new(1.0, 2.0, 0.2),
        Risk::new(5.0, 0, 8.0),
        Fidelity::Exact,
    );
    let evaluated = plinth_core::SearchEvidence::Evaluated(OptimizationSummary::new(
        OrientationCost::new(OrientationId::from("orient-1"), cost),
        24,
        vec![ParetoPoint::new(OrientationId::from("orient-1"), cost)],
    ));
    let evaluated_json = serde_json::to_string(&evaluated).unwrap();
    assert_eq!(
        serde_json::from_str::<plinth_core::SearchEvidence>(&evaluated_json).unwrap(),
        evaluated
    );
}
