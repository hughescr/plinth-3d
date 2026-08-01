use plinth_core::{
    digest_f64_slice, AcceptSafe, BoundKind, CalendarDate, ConstantId, CostError, CostVector,
    DecisionId, Defect, DefectReport, Derived, DigestError, EvidenceDate, EvidenceId,
    EvidenceSource, Fidelity, Finite, FiniteError, FiniteInterval, FixerId, LayerPlan,
    LayerPlanError, LoadSharing, LoadSharingError, Measured, MetricId, MetricSet, MetricValue,
    ModelId, OptimizationError, OptimizationSummary, OrientationCost, OrientationId, ParetoPoint,
    PartialCost, Pending, PlateId, ProvenanceError, ProvenanceManifest, ProvenanceRef, ReportError,
    Resin, Risk, Scar, SearchEvidence, Severity, SiteId, Stage, StageId, Termination,
    TerminationError, ViolationKind,
};
use serde_json::json;

fn valid_cost(z_height_mm: f64) -> CostVector {
    CostVector::new(
        z_height_mm,
        1_200.0,
        Resin::new(3.0, 0.5, 0.0).unwrap(),
        Scar::new(1.0, 2.0, 0.2).unwrap(),
        Risk::new(5.0, 0, 8.0).unwrap(),
        Fidelity::Exact,
    )
    .unwrap()
}

fn valid_summary() -> OptimizationSummary {
    let cost = valid_cost(42.0);
    OptimizationSummary::new(
        OrientationCost::new(OrientationId::from("orient-1"), cost),
        24,
        vec![ParetoPoint::new(OrientationId::from("orient-1"), cost)],
    )
    .unwrap()
}

fn calibration_source(date: &str) -> EvidenceSource {
    EvidenceSource::calibration_plate(
        PlateId::from("plate-6"),
        "fixtures/calibration.json",
        "/measurements/0",
        EvidenceDate::exact(CalendarDate::new(date).unwrap()),
    )
    .unwrap()
}

#[test]
fn evidence_source_and_date_wire_contracts_are_strict() {
    for date in [
        "0000-01-01",
        "0000-02-29",
        "1900-02-29",
        "2100-02-29",
        "2024-02-30",
    ] {
        assert!(
            CalendarDate::new(date).is_err(),
            "accepted invalid date {date}"
        );
    }
    for date in ["2000-02-29", "2400-02-29"] {
        assert!(CalendarDate::new(date).is_ok(), "rejected leap date {date}");
    }
    assert_eq!(
        serde_json::to_value(EvidenceDate::exact(
            CalendarDate::new("2024-02-29").unwrap()
        ))
        .unwrap(),
        json!({"precision":"exact","date":"2024-02-29"})
    );
    for evidence_date in [
        EvidenceDate::exact(CalendarDate::new("2024-02-29").unwrap()),
        EvidenceDate::on_or_before(CalendarDate::new("2024-02-29").unwrap()),
        EvidenceDate::Unknown,
    ] {
        assert_eq!(
            serde_json::from_value::<EvidenceDate>(serde_json::to_value(&evidence_date).unwrap())
                .unwrap(),
            evidence_date
        );
    }
    for malformed in [
        r#"{"precision":"unknown","date":"2024-02-29"}"#,
        r#"{"precision":"exact"}"#,
        r#"{"precision":"exact","date":"2024-02-29","extra":true}"#,
        r#"{"precision":"exact","precision":"unknown","date":"2024-02-29"}"#,
    ] {
        assert!(
            serde_json::from_str::<EvidenceDate>(malformed).is_err(),
            "accepted {malformed}"
        );
    }

    let source = calibration_source("2026-07-31");
    assert_eq!(
        serde_json::to_value(&source).unwrap(),
        json!({
            "kind":"calibration_plate", "plate":"plate-6", "record":"fixtures/calibration.json",
            "locator":"/measurements/0", "date":{"precision":"exact","date":"2026-07-31"}
        })
    );
    let vendor = EvidenceSource::vendor_document(
        EvidenceId::new("vendor-42").unwrap(),
        "docs/vendor.pdf",
        "#section-2",
        EvidenceDate::Unknown,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(vendor).unwrap()["kind"],
        "vendor_document"
    );
    let id = EvidenceId::new("source-1").unwrap();
    for source in [
        calibration_source("2026-07-31"),
        EvidenceSource::vendor_document(id.clone(), "docs/vendor", "#v", EvidenceDate::Unknown)
            .unwrap(),
        EvidenceSource::installed_hardware(
            id.clone(),
            "inventory/hardware",
            "#h",
            EvidenceDate::Unknown,
        )
        .unwrap(),
        EvidenceSource::corpus_study(id.clone(), "corpus/study", "#c", EvidenceDate::Unknown)
            .unwrap(),
        EvidenceSource::project_record(id.clone(), "records/project", "#p", EvidenceDate::Unknown)
            .unwrap(),
        EvidenceSource::owner_decision(id, "records/decisions", "#o", EvidenceDate::Unknown)
            .unwrap(),
    ] {
        assert_eq!(
            serde_json::from_value::<EvidenceSource>(serde_json::to_value(&source).unwrap())
                .unwrap(),
            source
        );
    }
    for field in ["record", "locator"] {
        let value = if field == "record" {
            r#"{"kind":"calibration_plate","plate":"p","record":" ","locator":"x","date":{"precision":"unknown"}}"#
        } else {
            r#"{"kind":"calibration_plate","plate":"p","record":"a","locator":" ","date":{"precision":"unknown"}}"#
        };
        assert!(
            serde_json::from_str::<EvidenceSource>(value).is_err(),
            "accepted blank {field}"
        );
    }
    for invalid in [
        "/absolute",
        "../escape",
        "..\\escape",
        "C:/drive",
        "C:\\drive",
        "\\\\server\\share",
        "https://example.test/a",
    ] {
        assert!(
            EvidenceSource::calibration_plate(
                PlateId::from("p"),
                invalid,
                "#x",
                EvidenceDate::Unknown
            )
            .is_err(),
            "accepted record {invalid}"
        );
    }
    for locator in [
        "https://example.test/a",
        "file:///tmp/a",
        "mailto:a@example.test",
    ] {
        assert!(
            EvidenceSource::calibration_plate(
                PlateId::from("p"),
                "fixtures/a",
                locator,
                EvidenceDate::Unknown
            )
            .is_err(),
            "accepted locator {locator}"
        );
    }
    for malformed in [
        r#"{"kind":"calibration_plate","plate":"p","record":"a","locator":"x","date":{"precision":"unknown"},"extra":true}"#,
        r#"{"kind":"calibration_plate","plate":"p","record":"a","record":"b","locator":"x","date":{"precision":"unknown"}}"#,
        r#"{"kind":"calibration_plate","kind":"vendor_document","plate":"p","record":"a","locator":"x","date":{"precision":"unknown"}}"#,
        r#"{"kind":"calibration_plate","plate":"p","record":"https://example.test/a","locator":"x","date":{"precision":"unknown"}}"#,
        r#"{"kind":"vendor_document","id":"x","record":"a","locator":"https://example.test/a","date":{"precision":"unknown"}}"#,
        r#"{"kind":"vendor_document","id":" ","record":"a","locator":"x","date":{"precision":"unknown"}}"#,
        r#"{"kind":"calibration_plate","plate":"p","record":"a","locator":"x","date":{"precision":"unknown"},"id":"bad"}"#,
    ] {
        assert!(
            serde_json::from_str::<EvidenceSource>(malformed).is_err(),
            "accepted {malformed}"
        );
    }
}

#[test]
fn finite_intervals_and_measured_evidence_have_exact_boundaries() {
    let interval =
        FiniteInterval::new(0.0, 1.0, BoundKind::Inclusive, BoundKind::Exclusive).unwrap();
    assert!(interval.contains(0.0));
    assert!(interval.contains(0.5));
    assert!(!interval.contains(1.0));
    assert!(!interval.contains(f64::NAN));
    assert_eq!(
        serde_json::to_string(&BoundKind::Inclusive).unwrap(),
        r#""inclusive""#
    );
    assert_eq!(
        serde_json::to_string(&BoundKind::Exclusive).unwrap(),
        r#""exclusive""#
    );
    assert_eq!(
        serde_json::to_value(interval).unwrap(),
        json!({"lower":0.0,"upper":1.0,"lower_bound":"inclusive","upper_bound":"exclusive"})
    );
    for malformed in [
        r#"{"lower":1.0,"upper":1.0,"lower_bound":"inclusive","upper_bound":"exclusive"}"#,
        r#"{"lower":0.0,"upper":1.0,"lower_bound":"inclusive"}"#,
        r#"{"lower":0.0,"lower":0.1,"upper":1.0,"lower_bound":"inclusive","upper_bound":"exclusive"}"#,
        r#"{"lower":0.0,"upper":1.0,"lower_bound":"inclusive","upper_bound":"exclusive","extra":true}"#,
    ] {
        assert!(
            serde_json::from_str::<FiniteInterval>(malformed).is_err(),
            "accepted {malformed}"
        );
    }
    assert_eq!(
        Measured::new(
            interval,
            calibration_source("2026-07-31"),
            Some(interval),
            "interval"
        ),
        Err(ProvenanceError::IntervalErrorBar)
    );
    let measured =
        Measured::new(interval, calibration_source("2026-07-31"), None, "interval").unwrap();
    let mut non_null = serde_json::to_value(measured).unwrap();
    non_null["err"] = serde_json::to_value(interval).unwrap();
    assert!(serde_json::from_value::<Measured<FiniteInterval>>(non_null).is_err());

    for malformed in [
        r#"{"value":{"kind":"zipper"},"source":{"kind":"calibration_plate","plate":"p","record":"a","locator":"x","date":{"precision":"unknown"}},"note":"n"}"#,
        r#"{"value":{"kind":"zipper"},"source":{"kind":"calibration_plate","plate":"p","record":"a","locator":"x","date":{"precision":"unknown"}},"err":null,"note":"n","plate":"legacy"}"#,
        r#"{"value":{"kind":"zipper"},"source":{"kind":"calibration_plate","plate":"p","record":"a","locator":"x","date":{"precision":"unknown"}},"err":null,"note":"n","date":"legacy"}"#,
        r#"{"value":{"kind":"zipper"},"source":{"kind":"calibration_plate","plate":"p","record":"a","locator":"x","date":{"precision":"unknown"}},"err":null,"err":null,"note":"n"}"#,
    ] {
        assert!(
            serde_json::from_str::<Measured<LoadSharing>>(malformed).is_err(),
            "accepted {malformed}"
        );
    }
    for malformed in [
        r#"{"kind":"measured","source":{"kind":"calibration_plate","plate":"p","record":"a","locator":"x","date":{"precision":"unknown"}},"note":"n","extra":true}"#,
        r#"{"kind":"measured","kind":"derived","source":{"kind":"calibration_plate","plate":"p","record":"a","locator":"x","date":{"precision":"unknown"}},"note":"n"}"#,
        r#"{"kind":"measured","note":"n"}"#,
    ] {
        assert!(
            serde_json::from_str::<ProvenanceRef>(malformed).is_err(),
            "accepted {malformed}"
        );
    }
    assert!(Measured::new(1.0_f64, calibration_source("2026-07-31"), None, " ").is_err());
}

fn terminal_defect(site: SiteId) -> Defect {
    Defect::new(
        ViolationKind::EmitBoundaryEdge,
        site,
        Severity::Blocking,
        -1.0,
        "terminal audit defect",
    )
    .unwrap()
}

fn report(defects: Vec<Defect>, termination: Termination) -> Result<DefectReport, ReportError> {
    DefectReport::new(
        ModelId::from("treant"),
        defects,
        MetricSet::new([(
            MetricId::from("signed_margin"),
            MetricValue::new(-0.25).unwrap(),
        )]),
        StageId::Audit,
        ProvenanceManifest::default(),
        termination,
        SearchEvidence::NotEvaluated,
    )
}

fn valid_cap_report() -> DefectReport {
    let site = SiteId::Member(7);
    let cap_defect = Defect::new(
        ViolationKind::RepairCapExhausted,
        site.clone(),
        Severity::Blocking,
        -1.0,
        "repair budget exhausted",
    )
    .unwrap();
    let termination = Termination::cap_exhausted(
        ViolationKind::LoadExceeded,
        FixerId::SplitLoadAcrossArms,
        site,
        3,
        -2.0,
        -1.0,
        "no further improvement",
    )
    .unwrap();
    report(vec![cap_defect], termination).unwrap()
}

fn valid_pool_report() -> DefectReport {
    let site = SiteId::Contact(4);
    let defect = Defect::new(
        ViolationKind::UnroutedContact,
        site.clone(),
        Severity::Blocking,
        1.0,
        "no candidates remain",
    )
    .unwrap();
    report(
        vec![defect],
        Termination::pool_exhausted(ViolationKind::UnroutedContact, site),
    )
    .unwrap()
}

#[test]
fn finite_rejects_nonfinite_and_preserves_signed_zero() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(Finite::new(value), Err(FiniteError::NonFinite));
    }
    let negative_zero = Finite::new(-0.0).unwrap();
    assert!(negative_zero.get().is_sign_negative());
    let round_trip: Finite =
        serde_json::from_str(&serde_json::to_string(&negative_zero).unwrap()).unwrap();
    assert!(round_trip.get().is_sign_negative());
    assert!(serde_json::from_str::<Finite>("1e9999").is_err());
}

#[test]
fn cost_physical_magnitudes_are_finite_and_nonnegative() {
    assert!(matches!(
        Resin::new(-1.0, 0.0, 0.0),
        Err(CostError::Negative { field: "model_ml" })
    ));
    assert!(matches!(
        Resin::new(0.0, -1.0, 0.0),
        Err(CostError::Negative {
            field: "support_ml"
        })
    ));
    assert!(matches!(
        Resin::new(0.0, 0.0, -1.0),
        Err(CostError::Negative {
            field: "trapped_ml"
        })
    ));
    assert!(Scar::new(-1.0, 0.0, 0.0).is_err());
    assert!(Scar::new(0.0, -1.0, 0.0).is_err());
    assert!(Scar::new(0.0, 0.0, -1.0).is_err());
    assert!(Risk::new(-1.0, 0, 0.0).is_err());
    assert!(Risk::new(0.0, 0, -1.0).is_err());

    let resin = Resin::new(0.0, 0.0, 0.0).unwrap();
    let scar = Scar::new(0.0, 0.0, 0.0).unwrap();
    let risk = Risk::new(0.0, 0, 0.0).unwrap();
    assert!(CostVector::new(-1.0, 0.0, resin, scar, risk, Fidelity::Exact).is_err());
    assert!(CostVector::new(0.0, -1.0, resin, scar, risk, Fidelity::Exact).is_err());
    assert!(CostVector::new(f64::NAN, 0.0, resin, scar, risk, Fidelity::Exact).is_err());
    assert!(CostVector::new(0.0, f64::NAN, resin, scar, risk, Fidelity::Exact).is_err());
    assert!(Resin::new(f64::INFINITY, 0.0, 0.0).is_err());
    assert!(Resin::new(0.0, f64::NAN, 0.0).is_err());
    assert!(Resin::new(0.0, 0.0, f64::NEG_INFINITY).is_err());
    assert!(Scar::new(f64::NAN, 0.0, 0.0).is_err());
    assert!(Scar::new(0.0, f64::INFINITY, 0.0).is_err());
    assert!(Scar::new(0.0, 0.0, f64::NEG_INFINITY).is_err());
    assert!(Risk::new(f64::NAN, 0, 0.0).is_err());
    assert!(Risk::new(0.0, 0, f64::INFINITY).is_err());
    assert!(PartialCost::new(Fidelity::Surrogate, Some(-1.0), None, None, None, None).is_err());
    assert!(PartialCost::new(Fidelity::Surrogate, None, Some(f64::NAN), None, None, None).is_err());

    let zero = CostVector::new(-0.0, 0.0, resin, scar, risk, Fidelity::Exact).unwrap();
    assert!(zero.z_height_mm().is_sign_negative());
    assert_eq!(
        serde_json::from_value::<CostVector>(serde_json::to_value(zero).unwrap()).unwrap(),
        zero
    );

    let valid = valid_cost(42.0);
    let valid_value = serde_json::to_value(valid).unwrap();
    for pointer in [
        "/z_height_mm",
        "/print_time_s",
        "/resin/model_ml",
        "/resin/support_ml",
        "/resin/trapped_ml",
        "/scar/area_mm2",
        "/scar/delicacy_weighted",
        "/scar/worst_patch_mm2",
        "/risk/max_effective_x",
        "/risk/min_lateral_spread_mm",
    ] {
        let mut negative = valid_value.clone();
        *negative.pointer_mut(pointer).unwrap() = json!(-1.0);
        assert!(
            serde_json::from_value::<CostVector>(negative).is_err(),
            "negative JSON unexpectedly accepted at {pointer}"
        );
    }

    let valid_text = serde_json::to_string(&valid).unwrap();
    for (field, rendered) in [
        ("z_height_mm", "42.0"),
        ("print_time_s", "1200.0"),
        ("model_ml", "3.0"),
        ("support_ml", "0.5"),
        ("trapped_ml", "0.0"),
        ("area_mm2", "1.0"),
        ("delicacy_weighted", "2.0"),
        ("worst_patch_mm2", "0.2"),
        ("max_effective_x", "5.0"),
        ("min_lateral_spread_mm", "8.0"),
    ] {
        let original = format!("\"{field}\":{rendered}");
        let replacement = format!("\"{field}\":1e9999");
        let nonfinite = valid_text.replacen(&original, &replacement, 1);
        assert_ne!(valid_text, nonfinite, "test did not find {field}");
        assert!(
            serde_json::from_str::<CostVector>(&nonfinite).is_err(),
            "non-finite JSON unexpectedly accepted for {field}"
        );
    }

    let partial =
        PartialCost::new(Fidelity::Surrogate, Some(1.0), Some(2.0), None, None, None).unwrap();
    let canonical_partial = json!({
        "fidelity": "surrogate",
        "z_height_mm": 1.0,
        "print_time_s": 2.0,
        "resin": null,
        "scar": null,
        "risk": null
    });
    assert_eq!(serde_json::to_value(partial).unwrap(), canonical_partial);
    assert_eq!(
        serde_json::from_value::<PartialCost>(canonical_partial.clone()).unwrap(),
        partial
    );
    let mut partial_json = canonical_partial;
    partial_json["z_height_mm"] = json!(-1.0);
    assert!(serde_json::from_value::<PartialCost>(partial_json).is_err());
}

#[test]
fn signed_report_measures_are_allowed_but_nonfinite_values_are_rejected() {
    let metric = MetricValue::new(-4.5).unwrap();
    assert_eq!(metric.get(), -4.5);
    assert!(MetricValue::new(f64::NEG_INFINITY).is_err());
    assert_eq!(serde_json::from_str::<MetricValue>("-4.5").unwrap(), metric);
    assert!(serde_json::from_str::<MetricValue>("1e9999").is_err());

    let defect = Defect::new(
        ViolationKind::LoadExceeded,
        SiteId::Member(1),
        Severity::Warning,
        -2.0,
        "signed delta",
    )
    .unwrap();
    assert_eq!(defect.measure.get(), -2.0);
    assert!(Defect::new(
        ViolationKind::LoadExceeded,
        SiteId::Member(1),
        Severity::Warning,
        f64::NAN,
        "bad"
    )
    .is_err());
    assert!(serde_json::from_str::<Defect>(
        r#"{"kind":"load_exceeded","site":{"kind":"member","id":1},"severity":"warning","measure":1e9999,"detail":"bad"}"#
    )
    .is_err());
}

#[test]
fn load_sharing_is_tagged_and_validated_at_the_measured_boundary() {
    let additive = LoadSharing::additive(1.0).unwrap();
    assert_eq!(additive.n_eff_cap(), Some(1.0));
    assert_eq!(
        serde_json::to_value(additive).unwrap(),
        json!({"kind": "additive", "n_eff_cap": 1.0})
    );
    assert_eq!(
        serde_json::from_value::<LoadSharing>(serde_json::to_value(additive).unwrap()).unwrap(),
        additive
    );
    assert_eq!(
        serde_json::from_value::<LoadSharing>(json!({"kind": "zipper"})).unwrap(),
        LoadSharing::Zipper
    );
    assert_eq!(
        LoadSharing::additive(0.999),
        Err(LoadSharingError::BelowOne)
    );
    assert_eq!(
        LoadSharing::additive(f64::INFINITY),
        Err(LoadSharingError::NonFinite)
    );
    assert!(
        serde_json::from_value::<LoadSharing>(json!({"kind": "additive", "n_eff_cap": 0.999}))
            .is_err()
    );
    assert!(
        serde_json::from_str::<LoadSharing>(r#"{"kind":"additive","n_eff_cap":1e9999}"#).is_err()
    );
    assert!(serde_json::from_str::<LoadSharing>(r#"{"kind":"zipper","n_eff_cap":1.0}"#).is_err());
    assert!(serde_json::from_str::<LoadSharing>(
        r#"{"kind":"additive","n_eff_cap":1.0,"extra":true}"#
    )
    .is_err());

    let measured = Measured::new(
        LoadSharing::additive(6.0).unwrap(),
        calibration_source("2026-07-31"),
        None,
        "categorical measurement",
    )
    .unwrap();
    let measured_json = serde_json::to_value(&measured).unwrap();
    assert!(measured_json["err"].is_null());
    assert_eq!(measured_json["value"]["kind"], "additive");
    assert_eq!(measured_json["value"]["n_eff_cap"], 6.0);
    assert_eq!(
        serde_json::from_value::<Measured<LoadSharing>>(measured_json.clone()).unwrap(),
        measured
    );
    assert_eq!(
        Measured::new(
            LoadSharing::Zipper,
            calibration_source("2026-07-31"),
            Some(LoadSharing::Zipper),
            "invalid categorical error bar",
        ),
        Err(ProvenanceError::CategoricalErrorBar)
    );
    let mut categorical_error = measured_json.clone();
    categorical_error["err"] = json!({"kind": "zipper"});
    assert!(serde_json::from_value::<Measured<LoadSharing>>(categorical_error).is_err());
}

#[test]
fn provenance_is_nested_deterministic_and_measured_dates_are_required() {
    let mut manifest = ProvenanceManifest::default();
    manifest
        .record(
            DecisionId::from("decision_z"),
            ConstantId::from("constant_z"),
            ProvenanceRef::Derived {
                from: vec!["source".into()],
                note: "z".into(),
            },
        )
        .unwrap();
    manifest
        .record(
            DecisionId::from("decision_a"),
            ConstantId::from("constant_z"),
            ProvenanceRef::Derived {
                from: vec!["source".into()],
                note: "z".into(),
            },
        )
        .unwrap();
    manifest
        .record(
            DecisionId::from("decision_a"),
            ConstantId::from("constant_a"),
            ProvenanceRef::Measured {
                source: calibration_source("2026-07-31"),
                note: "a".into(),
            },
        )
        .unwrap();
    let text = serde_json::to_string(&manifest).unwrap();
    assert!(text.find("decision_a").unwrap() < text.find("decision_z").unwrap());
    assert!(text.find("constant_a").unwrap() < text.find("constant_z").unwrap());
    assert_eq!(
        serde_json::from_str::<ProvenanceManifest>(&text).unwrap(),
        manifest
    );

    let measured =
        Measured::new(0.05_f64, calibration_source("2026-07-31"), None, "caliper").unwrap();
    let mut value = serde_json::to_value(&measured).unwrap();
    assert!(value["err"].is_null());
    assert_eq!(
        measured.provenance(),
        ProvenanceRef::Measured {
            source: calibration_source("2026-07-31"),
            note: "caliper".into(),
        }
    );
    value.as_object_mut().unwrap().remove("source");
    assert!(serde_json::from_value::<Measured<f64>>(value).is_err());
}

#[test]
fn measured_payload_policy_validates_numeric_errors_and_required_wire_fields() {
    let numeric = Measured::new(
        12.0_f64,
        calibration_source("2026-07-24"),
        Some(0.5),
        "numeric bracket",
    )
    .unwrap();
    assert_eq!(numeric.value(), &12.0);
    assert_eq!(numeric.err(), Some(&0.5));
    assert_eq!(numeric.note(), "numeric bracket");
    assert_eq!(
        serde_json::from_value::<Measured<f64>>(serde_json::to_value(&numeric).unwrap()).unwrap(),
        numeric
    );

    let no_error = Measured::new(
        6.0_f64,
        calibration_source("2026-07-31"),
        None,
        "explicitly no numeric error bar",
    )
    .unwrap();
    let no_error_json = serde_json::to_value(&no_error).unwrap();
    assert!(no_error_json["err"].is_null());
    assert_eq!(
        serde_json::from_value::<Measured<f64>>(no_error_json.clone()).unwrap(),
        no_error
    );
    let mut missing_error = no_error_json.clone();
    missing_error.as_object_mut().unwrap().remove("err");
    assert!(serde_json::from_value::<Measured<f64>>(missing_error).is_err());
    let mut missing_source = no_error_json;
    missing_source.as_object_mut().unwrap().remove("source");
    assert!(serde_json::from_value::<Measured<f64>>(missing_source).is_err());

    assert_eq!(
        Measured::new(
            f64::NAN,
            calibration_source("2026-07-31"),
            None,
            "bad value",
        ),
        Err(ProvenanceError::NonFiniteMeasuredValue)
    );
    assert_eq!(
        Measured::new(
            1.0_f64,
            calibration_source("2026-07-31"),
            Some(f64::INFINITY),
            "bad error",
        ),
        Err(ProvenanceError::NonFiniteMeasuredError)
    );
    assert!(serde_json::from_str::<Measured<f64>>(
        r#"{"value":1e9999,"source":{"kind":"calibration_plate","plate":"plate","record":"a","locator":"x","date":{"precision":"unknown"}},"err":null,"note":"bad"}"#
    )
    .is_err());

    let finite = Measured::new(
        Finite::new(2.0).unwrap(),
        calibration_source("2026-07-31"),
        Some(Finite::new(0.1).unwrap()),
        "validated wrapper",
    )
    .unwrap();
    assert_eq!(
        serde_json::from_value::<Measured<Finite>>(serde_json::to_value(&finite).unwrap()).unwrap(),
        finite
    );
}

#[test]
fn provenance_manifest_merges_api_batches_but_rejects_conflicts_and_wire_duplicates() {
    let decision = DecisionId::from("accept");
    let constant_a = ConstantId::from("a");
    let constant_b = ConstantId::from("b");
    let provenance_a = ProvenanceRef::Derived {
        from: vec!["source-a".into()],
        note: "a".into(),
    };
    let provenance_b = ProvenanceRef::Derived {
        from: vec!["source-b".into()],
        note: "b".into(),
    };
    let conflicting = ProvenanceRef::Derived {
        from: vec!["other".into()],
        note: "conflict".into(),
    };

    let mut manifest = ProvenanceManifest::new(vec![
        (
            decision.clone(),
            vec![(constant_a.clone(), provenance_a.clone())],
        ),
        (
            decision.clone(),
            vec![(constant_b.clone(), provenance_b.clone())],
        ),
        (
            decision.clone(),
            vec![(constant_a.clone(), provenance_a.clone())],
        ),
    ])
    .unwrap();
    assert_eq!(manifest.iter().len(), 1);
    assert_eq!(manifest.get(&decision, &constant_a), Some(&provenance_a));
    assert_eq!(manifest.get(&decision, &constant_b), Some(&provenance_b));
    manifest
        .record(decision.clone(), constant_a.clone(), provenance_a.clone())
        .unwrap();
    assert_eq!(
        manifest.record(decision.clone(), constant_a.clone(), conflicting.clone()),
        Err(ProvenanceError::ConflictingRecord {
            decision: decision.clone(),
            constant: constant_a.clone(),
        })
    );
    assert_eq!(
        ProvenanceManifest::new(vec![(
            decision.clone(),
            vec![
                (constant_a.clone(), provenance_a.clone()),
                (constant_a.clone(), conflicting),
            ],
        )]),
        Err(ProvenanceError::ConflictingRecord {
            decision: decision.clone(),
            constant: constant_a.clone(),
        })
    );

    let duplicate_outer = r#"{
        "accept":{"a":{"kind":"derived","from":["source-a"],"note":"a"}},
        "accept":{"b":{"kind":"derived","from":["source-b"],"note":"b"}}
    }"#;
    assert!(serde_json::from_str::<ProvenanceManifest>(duplicate_outer).is_err());
    let conflicting_outer = r#"{
        "accept":{"a":{"kind":"derived","from":["source-a"],"note":"a"}},
        "accept":{"a":{"kind":"derived","from":["other"],"note":"conflict"}}
    }"#;
    assert!(serde_json::from_str::<ProvenanceManifest>(conflicting_outer).is_err());
    let duplicate_inner = r#"{
        "accept":{
            "a":{"kind":"derived","from":["source-a"],"note":"a"},
            "a":{"kind":"derived","from":["source-a"],"note":"a"}
        }
    }"#;
    assert!(serde_json::from_str::<ProvenanceManifest>(duplicate_inner).is_err());
    let conflicting_inner = r#"{
        "accept":{
            "a":{"kind":"derived","from":["source-a"],"note":"a"},
            "a":{"kind":"derived","from":["other"],"note":"conflict"}
        }
    }"#;
    assert!(serde_json::from_str::<ProvenanceManifest>(conflicting_inner).is_err());
}

#[test]
fn report_requires_provenance_and_only_blocking_defects_control_acceptance() {
    let nonblocking = vec![
        Defect::new(
            ViolationKind::ScarCapExceeded,
            SiteId::Face(1),
            Severity::Info,
            -2.0,
            "informational signed delta",
        )
        .unwrap(),
        Defect::new(
            ViolationKind::ArrangementInstability,
            SiteId::Member(2),
            Severity::Warning,
            3.0,
            "warning remains visible",
        )
        .unwrap(),
    ];
    let metrics = MetricSet::new([(
        MetricId::from("independent_metric"),
        MetricValue::new(-99.0).unwrap(),
    )]);
    let search = SearchEvidence::Evaluated(valid_summary());
    let accepted = DefectReport::new(
        ModelId::from("treant"),
        nonblocking.clone(),
        metrics.clone(),
        StageId::Audit,
        ProvenanceManifest::default(),
        Termination::Converged,
        search.clone(),
    )
    .unwrap();
    assert!(accepted.accepted());
    let mut blocking = nonblocking;
    blocking.push(terminal_defect(SiteId::Model(ModelId::from("treant"))));
    let rejected = DefectReport::new(
        ModelId::from("treant"),
        blocking,
        metrics,
        StageId::Audit,
        ProvenanceManifest::default(),
        Termination::Converged,
        search,
    )
    .unwrap();
    assert!(!rejected.accepted());
    let rejected_round_trip =
        serde_json::from_value::<DefectReport>(serde_json::to_value(&rejected).unwrap()).unwrap();
    assert_eq!(rejected_round_trip, rejected);
    assert!(!rejected_round_trip.accepted());

    let mut json = serde_json::to_value(accepted).unwrap();
    json.as_object_mut().unwrap().remove("provenance");
    assert!(serde_json::from_value::<DefectReport>(json).is_err());

    let manifest = ProvenanceManifest::new([(
        DecisionId::from("load_acceptance"),
        [(
            ConstantId::from("load_sharing"),
            ProvenanceRef::Measured {
                source: calibration_source("2026-07-31"),
                note: "measured".into(),
            },
        )],
    )])
    .unwrap();
    let with_provenance = DefectReport::new(
        ModelId::from("treant"),
        vec![],
        MetricSet::default(),
        StageId::Audit,
        manifest,
        Termination::Converged,
        SearchEvidence::NotEvaluated,
    )
    .unwrap();
    let mut missing_date = serde_json::to_value(with_provenance).unwrap();
    missing_date["provenance"]["load_acceptance"]["load_sharing"]["source"]
        .as_object_mut()
        .unwrap()
        .remove("date");
    assert!(serde_json::from_value::<DefectReport>(missing_date).is_err());
}

#[test]
fn cap_exhaustion_requires_complete_termination_and_matching_blocker() {
    assert_eq!(
        Termination::cap_exhausted(
            ViolationKind::LoadExceeded,
            FixerId::SplitLoadAcrossArms,
            SiteId::Member(7),
            0,
            1.0,
            1.0,
            "bad"
        ),
        Err(TerminationError::ZeroAttempts)
    );
    assert_eq!(
        Termination::cap_exhausted(
            ViolationKind::RepairCapExhausted,
            FixerId::SplitLoadAcrossArms,
            SiteId::Member(7),
            1,
            1.0,
            1.0,
            "bad"
        ),
        Err(TerminationError::RepairCapAsOriginalKind)
    );
    assert!(Termination::cap_exhausted(
        ViolationKind::LoadExceeded,
        FixerId::SplitLoadAcrossArms,
        SiteId::Member(7),
        1,
        f64::NAN,
        1.0,
        "bad"
    )
    .is_err());
    assert!(Termination::cap_exhausted(
        ViolationKind::LoadExceeded,
        FixerId::SplitLoadAcrossArms,
        SiteId::Member(7),
        1,
        1.0,
        f64::INFINITY,
        "bad"
    )
    .is_err());

    let termination = Termination::cap_exhausted(
        ViolationKind::LoadExceeded,
        FixerId::SplitLoadAcrossArms,
        SiteId::Member(7),
        1,
        -2.0,
        -1.0,
        "signed finite deltas are valid",
    )
    .unwrap();
    assert_eq!(
        report(vec![], termination),
        Err(ReportError::MissingCapExhaustedDefect)
    );

    let wrong_site_termination = Termination::cap_exhausted(
        ViolationKind::LoadExceeded,
        FixerId::SplitLoadAcrossArms,
        SiteId::Member(7),
        1,
        2.0,
        1.0,
        "wrong matching site",
    )
    .unwrap();
    let wrong_site = Defect::new(
        ViolationKind::RepairCapExhausted,
        SiteId::Member(8),
        Severity::Blocking,
        1.0,
        "wrong site",
    )
    .unwrap();
    assert_eq!(
        report(vec![wrong_site], wrong_site_termination),
        Err(ReportError::MissingCapExhaustedDefect)
    );

    let wrong_kind_termination = Termination::cap_exhausted(
        ViolationKind::LoadExceeded,
        FixerId::SplitLoadAcrossArms,
        SiteId::Member(7),
        1,
        2.0,
        1.0,
        "wrong matching kind",
    )
    .unwrap();
    let wrong_kind = Defect::new(
        ViolationKind::LoadExceeded,
        SiteId::Member(7),
        Severity::Blocking,
        1.0,
        "wrong kind",
    )
    .unwrap();
    assert_eq!(
        report(vec![wrong_kind], wrong_kind_termination),
        Err(ReportError::MissingCapExhaustedDefect)
    );

    let valid = valid_cap_report();
    let mut missing_fixer = serde_json::to_value(&valid).unwrap();
    missing_fixer["termination"]
        .as_object_mut()
        .unwrap()
        .remove("fixer");
    assert!(serde_json::from_value::<DefectReport>(missing_fixer).is_err());

    let mut missing_blocker = serde_json::to_value(&valid).unwrap();
    missing_blocker["defects"] = json!([]);
    assert!(serde_json::from_value::<DefectReport>(missing_blocker).is_err());

    let mut zero_attempts = serde_json::to_value(&valid).unwrap();
    zero_attempts["termination"]["attempts"] = json!(0);
    assert!(serde_json::from_value::<DefectReport>(zero_attempts).is_err());

    let mut repair_cap_kind = serde_json::to_value(&valid).unwrap();
    repair_cap_kind["termination"]["kind"] = json!("repair_cap_exhausted");
    assert!(serde_json::from_value::<DefectReport>(repair_cap_kind).is_err());

    let mut wrong_site = serde_json::to_value(&valid).unwrap();
    wrong_site["defects"][0]["site"]["id"] = json!(8);
    assert!(serde_json::from_value::<DefectReport>(wrong_site).is_err());

    let mut wrong_kind = serde_json::to_value(&valid).unwrap();
    wrong_kind["defects"][0]["kind"] = json!("load_exceeded");
    assert!(serde_json::from_value::<DefectReport>(wrong_kind).is_err());

    let text = serde_json::to_string(&valid).unwrap();
    let nonfinite = text.replace("\"before\":-2.0", "\"before\":1e9999");
    assert_ne!(text, nonfinite);
    assert!(serde_json::from_str::<DefectReport>(&nonfinite).is_err());
    let nonfinite = text.replace("\"after\":-1.0", "\"after\":-1e9999");
    assert_ne!(text, nonfinite);
    assert!(serde_json::from_str::<DefectReport>(&nonfinite).is_err());
    assert_eq!(serde_json::from_str::<DefectReport>(&text).unwrap(), valid);
}

#[test]
fn pool_exhaustion_requires_a_matching_blocking_original_defect() {
    assert_eq!(
        report(
            vec![],
            Termination::pool_exhausted(ViolationKind::UnroutedContact, SiteId::Contact(4))
        ),
        Err(ReportError::MissingPoolExhaustedDefect)
    );
    let wrong_site = Defect::new(
        ViolationKind::UnroutedContact,
        SiteId::Contact(5),
        Severity::Blocking,
        1.0,
        "wrong site",
    )
    .unwrap();
    assert_eq!(
        report(
            vec![wrong_site],
            Termination::pool_exhausted(ViolationKind::UnroutedContact, SiteId::Contact(4))
        ),
        Err(ReportError::MissingPoolExhaustedDefect)
    );
    let wrong_kind = Defect::new(
        ViolationKind::LoadExceeded,
        SiteId::Contact(4),
        Severity::Blocking,
        1.0,
        "wrong kind",
    )
    .unwrap();
    assert_eq!(
        report(
            vec![wrong_kind],
            Termination::pool_exhausted(ViolationKind::UnroutedContact, SiteId::Contact(4))
        ),
        Err(ReportError::MissingPoolExhaustedDefect)
    );
    let valid = valid_pool_report();
    let mut wrong_severity = serde_json::to_value(&valid).unwrap();
    wrong_severity["defects"][0]["severity"] = json!("warning");
    assert!(serde_json::from_value::<DefectReport>(wrong_severity).is_err());

    let mut wrong_site = serde_json::to_value(&valid).unwrap();
    wrong_site["defects"][0]["site"]["id"] = json!(5);
    assert!(serde_json::from_value::<DefectReport>(wrong_site).is_err());

    let mut wrong_kind = serde_json::to_value(&valid).unwrap();
    wrong_kind["defects"][0]["kind"] = json!("load_exceeded");
    assert!(serde_json::from_value::<DefectReport>(wrong_kind).is_err());
    assert_eq!(
        serde_json::from_value::<DefectReport>(serde_json::to_value(&valid).unwrap()).unwrap(),
        valid
    );
}

#[test]
fn aborted_termination_is_rejected() {
    let valid = report(vec![], Termination::Converged).unwrap();
    let mut value = serde_json::to_value(valid).unwrap();
    value["termination"] = json!({"outcome": "aborted", "reason": "legacy"});
    assert!(serde_json::from_value::<DefectReport>(value).is_err());
}

#[test]
fn fixer_vocabulary_is_exhaustive_and_wire_names_remain_stable() {
    fn wire_name(fixer: FixerId) -> &'static str {
        match fixer {
            FixerId::AddSeatAtMarginalArea => "add_seat_at_marginal_area",
            FixerId::AddBirthLadderSeat => "add_birth_ladder_seat",
            FixerId::AddMinimumSeat => "add_minimum_seat",
            FixerId::SplitLoadAcrossArms => "split_load_across_arms",
            FixerId::RelocateForLateralSpread => "relocate_for_lateral_spread",
            FixerId::ReaimReseatOrQuarantine => "reaim_reseat_or_quarantine",
            FixerId::SlideOrReseatJoint => "slide_or_reseat_joint",
            FixerId::ReaimReduceEmbed => "reaim_reduce_embed",
            FixerId::RerouteToPlateOrAnchor => "reroute_to_plate_or_anchor",
            FixerId::SteepenOrBrace => "steepen_or_brace",
            FixerId::ResizeAncestors => "resize_ancestors",
            FixerId::RerouteOrDropRegular => "reroute_or_drop_regular",
            FixerId::AddAuditDrainOrRefill => "add_audit_drain_or_refill",
        }
    }

    for fixer in [
        FixerId::AddSeatAtMarginalArea,
        FixerId::AddBirthLadderSeat,
        FixerId::AddMinimumSeat,
        FixerId::SplitLoadAcrossArms,
        FixerId::RelocateForLateralSpread,
        FixerId::ReaimReseatOrQuarantine,
        FixerId::SlideOrReseatJoint,
        FixerId::ReaimReduceEmbed,
        FixerId::RerouteToPlateOrAnchor,
        FixerId::SteepenOrBrace,
        FixerId::ResizeAncestors,
        FixerId::RerouteOrDropRegular,
        FixerId::AddAuditDrainOrRefill,
    ] {
        assert_eq!(
            serde_json::to_value(fixer).unwrap(),
            json!(wire_name(fixer))
        );
    }
}

#[test]
fn search_and_termination_wire_tags_and_numeric_shapes_remain_stable() {
    let not_evaluated = SearchEvidence::NotEvaluated;
    assert_eq!(
        serde_json::to_value(&not_evaluated).unwrap(),
        json!({"status": "not_evaluated"})
    );
    assert_eq!(
        serde_json::from_value::<SearchEvidence>(serde_json::to_value(&not_evaluated).unwrap())
            .unwrap(),
        not_evaluated
    );

    let evaluated = SearchEvidence::Evaluated(valid_summary());
    let value = serde_json::to_value(&evaluated).unwrap();
    assert_eq!(value["status"], "evaluated");
    assert!(value["summary"]["selected"]["cost"]["z_height_mm"].is_number());
    assert_eq!(
        serde_json::from_value::<SearchEvidence>(value).unwrap(),
        evaluated
    );

    let converged = Termination::Converged;
    let converged_json = json!({"outcome": "converged"});
    assert_eq!(serde_json::to_value(&converged).unwrap(), converged_json);
    assert_eq!(
        serde_json::from_value::<Termination>(converged_json).unwrap(),
        converged
    );

    let converged_report = report(vec![], Termination::Converged).unwrap();
    let converged_report_json = json!({
        "subject": "treant",
        "defects": [],
        "metrics": {"signed_margin": -0.25},
        "stage": "audit",
        "provenance": {},
        "termination": {"outcome": "converged"},
        "search": {"status": "not_evaluated"}
    });
    assert_eq!(
        serde_json::to_value(&converged_report).unwrap(),
        converged_report_json
    );
    assert_eq!(
        serde_json::from_value::<DefectReport>(converged_report_json).unwrap(),
        converged_report
    );

    let cap = serde_json::to_value(valid_cap_report()).unwrap();
    assert_eq!(cap["termination"]["outcome"], "cap_exhausted");
    assert_eq!(cap["termination"]["fixer"], "split_load_across_arms");
    assert!(cap["termination"]["before"].is_number());
    assert!(cap["termination"]["after"].is_number());
    assert!(cap["defects"][0]["measure"].is_number());
}

#[test]
fn layer_plan_has_canonical_wire_data_and_validates_every_boundary() {
    let height = Measured::new(
        0.05_f64,
        calibration_source("2026-07-31"),
        Some(0.001),
        "caliper",
    )
    .unwrap();
    let plan = LayerPlan::constant(&height, 3).unwrap();
    assert_eq!(plan.heights_mm(), &[0.05, 0.05, 0.05]);
    assert_eq!(plan.z_of(2), Some(0.1));
    assert_eq!(plan.index_at(0.049), Some(0));
    assert_eq!(plan.index_at(0.05), Some(1));
    let top = plan.z_of(2).unwrap() + plan.heights_mm()[2];
    assert_eq!(plan.index_at(top), None);
    assert_eq!(plan.z_of(plan.count()), None);

    let positive_zero_origin = LayerPlan::new(0.0, vec![0.05]).unwrap();
    assert_eq!(positive_zero_origin.index_at(-0.0), Some(0));
    assert_eq!(positive_zero_origin.index_at(0.0), Some(0));
    let negative_zero_origin = LayerPlan::new(-0.0, vec![0.05]).unwrap();
    assert_eq!(negative_zero_origin.index_at(-0.0), Some(0));
    assert_eq!(negative_zero_origin.index_at(0.0), Some(0));

    let value = serde_json::to_value(&plan).unwrap();
    assert_eq!(
        value,
        json!({"z0_mm": 0.0, "heights_mm": [0.05, 0.05, 0.05]})
    );
    assert!(value.get("cumulative_mm").is_none());
    assert_eq!(serde_json::from_value::<LayerPlan>(value).unwrap(), plan);
    assert!(serde_json::from_value::<LayerPlan>(json!({
        "z0_mm": 0.0,
        "heights_mm": [0.05],
        "cumulative_mm": [0.0, 99.0]
    }))
    .is_err());

    assert_eq!(
        LayerPlan::new(f64::NAN, vec![0.05]),
        Err(LayerPlanError::NonFiniteOrigin)
    );
    assert_eq!(
        LayerPlan::new(0.0, vec![0.0]),
        Err(LayerPlanError::NonPositiveHeight { index: 0 })
    );
    assert_eq!(
        LayerPlan::new(0.0, vec![-0.05]),
        Err(LayerPlanError::NonPositiveHeight { index: 0 })
    );
    assert_eq!(
        LayerPlan::new(0.0, vec![f64::INFINITY]),
        Err(LayerPlanError::NonFiniteHeight { index: 0 })
    );
    assert_eq!(
        LayerPlan::new(f64::MAX, vec![f64::MAX]),
        Err(LayerPlanError::CumulativeOverflow { index: 0 })
    );
    assert_eq!(
        LayerPlan::new(9_007_199_254_740_992.0, vec![1.0]),
        Err(LayerPlanError::NonIncreasingBoundary { index: 0 })
    );
    assert!(serde_json::from_str::<LayerPlan>(r#"{"z0_mm":0.0,"heights_mm":[1e9999]}"#).is_err());
    assert!(serde_json::from_str::<LayerPlan>(r#"{"z0_mm":1e9999,"heights_mm":[0.05]}"#).is_err());
    for height in [0.0, -0.05] {
        assert!(serde_json::from_value::<LayerPlan>(json!({
            "z0_mm": 0.0,
            "heights_mm": [height]
        }))
        .is_err());
    }
    assert!(serde_json::from_value::<LayerPlan>(json!({
        "z0_mm": f64::MAX,
        "heights_mm": [f64::MAX]
    }))
    .is_err());
    assert!(serde_json::from_value::<LayerPlan>(json!({
        "z0_mm": 9_007_199_254_740_992.0,
        "heights_mm": [1.0]
    }))
    .is_err());

    let large_valid = LayerPlan::new(9_007_199_254_740_992.0, vec![2.0]).unwrap();
    assert_eq!(large_valid.index_at(large_valid.z0_mm()), Some(0));
    assert_eq!(
        large_valid.index_at(large_valid.z0_mm() + large_valid.heights_mm()[0]),
        None
    );
    assert_eq!(large_valid.index_at(large_valid.z0_mm() - 2.0), None);

    let empty = LayerPlan::new(-1.0, vec![]).unwrap();
    assert_eq!(empty.count(), 0);
    assert_eq!(empty.z_of(0), None);
    assert_eq!(empty.index_at(-1.0), None);
    assert_eq!(
        serde_json::from_str::<LayerPlan>(r#"{"z0_mm":-1.0,"heights_mm":[]}"#).unwrap(),
        empty
    );
}

#[test]
fn optimization_summary_constructor_enforces_search_evidence() {
    let cost = valid_cost(42.0);
    let selected = OrientationCost::new(OrientationId::from("one"), cost);
    let one = ParetoPoint::new(OrientationId::from("one"), cost);
    let two = ParetoPoint::new(OrientationId::from("two"), valid_cost(43.0));

    assert_eq!(
        OptimizationSummary::new(selected.clone(), 0, vec![one.clone()]),
        Err(OptimizationError::NoCandidatesExamined)
    );
    assert_eq!(
        OptimizationSummary::new(selected.clone(), 1, vec![]),
        Err(OptimizationError::EmptyPareto)
    );
    assert_eq!(
        OptimizationSummary::new(selected.clone(), 1, vec![one.clone(), two]),
        Err(OptimizationError::ParetoExceedsCandidates)
    );
    assert_eq!(
        OptimizationSummary::new(selected.clone(), 2, vec![one.clone(), one.clone()]),
        Err(OptimizationError::DuplicateOrientation {
            orientation: OrientationId::from("one")
        })
    );
    assert_eq!(
        OptimizationSummary::new(
            OrientationCost::new(OrientationId::from("missing"), cost),
            1,
            vec![one.clone()]
        ),
        Err(OptimizationError::SelectedNotInPareto)
    );
    assert_eq!(
        OptimizationSummary::new(
            OrientationCost::new(OrientationId::from("one"), valid_cost(99.0)),
            1,
            vec![one]
        ),
        Err(OptimizationError::SelectedNotInPareto)
    );
}

#[test]
fn optimization_summary_json_revalidates_every_cross_field_invariant() {
    let valid = valid_summary();
    let value = serde_json::to_value(&valid).unwrap();

    let mut zero = value.clone();
    zero["candidates_examined"] = json!(0);
    assert!(serde_json::from_value::<OptimizationSummary>(zero).is_err());

    let mut empty = value.clone();
    empty["pareto"] = json!([]);
    assert!(serde_json::from_value::<OptimizationSummary>(empty).is_err());

    let mut too_small = value.clone();
    let mut second = too_small["pareto"][0].clone();
    second["orientation"] = json!("orient-2");
    too_small["pareto"].as_array_mut().unwrap().push(second);
    too_small["candidates_examined"] = json!(1);
    assert!(serde_json::from_value::<OptimizationSummary>(too_small).is_err());

    let mut duplicate = value.clone();
    let duplicate_point = duplicate["pareto"][0].clone();
    duplicate["pareto"]
        .as_array_mut()
        .unwrap()
        .push(duplicate_point);
    duplicate["candidates_examined"] = json!(2);
    assert!(serde_json::from_value::<OptimizationSummary>(duplicate).is_err());

    let mut absent = value.clone();
    absent["selected"]["orientation"] = json!("missing");
    assert!(serde_json::from_value::<OptimizationSummary>(absent).is_err());

    let mut different_cost = value.clone();
    different_cost["selected"]["cost"]["z_height_mm"] = json!(99.0);
    assert!(serde_json::from_value::<OptimizationSummary>(different_cost).is_err());

    assert_eq!(
        serde_json::from_value::<OptimizationSummary>(value).unwrap(),
        valid
    );
}

#[test]
fn pending_resolves_to_accept_safe_derived_conservative_value() {
    let pending = Pending::new(12.0_f64, 18.0, PlateId::from("plate-7"), "load limit");
    let resolved: Derived<f64> = pending.resolve();
    assert_eq!(resolved.get(), 12.0);
    assert!(resolved.note.contains("conservatively"));
}

#[test]
fn stage_contract_compiles_with_context_and_ok_or_report_error() {
    struct AddOne;

    impl Stage for AddOne {
        type Input = u32;
        type Output = u32;
        type Context = str;

        const ID: StageId = StageId::Build;

        fn run(
            &self,
            input: Self::Input,
            ctx: &Self::Context,
        ) -> Result<Self::Output, DefectReport> {
            if ctx == "fail" {
                Err(report(
                    vec![terminal_defect(SiteId::Model(ModelId::from("stage-input")))],
                    Termination::Converged,
                )
                .unwrap())
            } else {
                Ok(input + 1)
            }
        }
    }

    let stage = AddOne;
    assert_eq!(AddOne::ID, StageId::Build);
    assert_eq!(stage.run(4, "ok").unwrap(), 5);
    assert!(!stage.run(4, "fail").unwrap_err().accepted());
}

#[test]
fn digest_preserves_signed_zero_and_rejects_nonfinite_values() {
    fn fnv_one(bits: u64) -> u64 {
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;
        bits.to_le_bytes().into_iter().fold(OFFSET, |state, byte| {
            (state ^ u64::from(byte)).wrapping_mul(PRIME)
        })
    }

    assert_ne!(
        digest_f64_slice(&[-0.0]).unwrap(),
        digest_f64_slice(&[0.0]).unwrap()
    );
    assert_eq!(
        digest_f64_slice(&[-0.0]).unwrap().0,
        fnv_one((-0.0_f64).to_bits())
    );
    assert_eq!(
        digest_f64_slice(&[0.0]).unwrap().0,
        fnv_one(0.0_f64.to_bits())
    );
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            digest_f64_slice(&[1.0, value]),
            Err(DigestError::NonFinite { index: 1 })
        );
    }
}

#[test]
fn public_validation_errors_have_concrete_display_messages() {
    assert_eq!(FiniteError::NonFinite.to_string(), "value must be finite");
    assert!(LayerPlanError::CumulativeOverflow { index: 3 }
        .to_string()
        .contains("index 3"));
    assert!(OptimizationError::DuplicateOrientation {
        orientation: OrientationId::from("duplicate-id")
    }
    .to_string()
    .contains("duplicate-id"));
    assert!(ReportError::MissingCapExhaustedDefect
        .to_string()
        .contains("matching blocking"));
}
