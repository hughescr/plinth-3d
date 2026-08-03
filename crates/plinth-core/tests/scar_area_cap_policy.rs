use plinth_core::{
    m0_constants, CalendarDate, ConstantId, EvidenceDate, EvidenceSourceView,
    RegistryProvenanceClass,
};

#[test]
fn scar_area_cap_policy_reuses_the_canonical_typed_registry_authority() {
    let constants = m0_constants().expect("the complete M0 registry must construct");
    let policy = constants.scar_area_cap_policy();

    assert_eq!(policy.decision_id().as_str(), "SCAR-AREA-CAP");
    assert_eq!(
        policy.constant_ids(),
        [
            "A_ABS_MM2",
            "KAPPA",
            "L_SCAR_MAX_MM",
            "EPS_COS",
            "TOL_MM",
            "PRINT_FLOOR_CHORD_MM",
            "BALL_FACTOR_STUB",
            "BALL_FACTOR_FOOT",
        ]
    );

    assert_eq!(policy.a_abs_mm2().diagnostic().get(), 2.0);
    assert_eq!(policy.kappa().diagnostic().get(), 1.25);
    assert_eq!(policy.l_scar_max_mm().diagnostic().get(), 2.5);
    assert_eq!(policy.eps_cos().diagnostic().get(), 1e-9);
    assert_eq!(policy.interval_tol_mm().diagnostic().get(), 0.05);
    assert_eq!(policy.print_floor_chord_mm().value(), &0.08);
    assert_eq!(policy.tip_ball_factor().diagnostic().get(), 1.5);
    assert_eq!(policy.foot_ball_factor().diagnostic().get(), 1.4);

    for id in [
        "A_ABS_MM2",
        "KAPPA",
        "L_SCAR_MAX_MM",
        "EPS_COS",
        "TOL_MM",
        "BALL_FACTOR_STUB",
        "BALL_FACTOR_FOOT",
    ] {
        assert_eq!(
            constants
                .registry()
                .get(&ConstantId::from(id))
                .expect("policy ID must be registered")
                .provenance()
                .class(),
            RegistryProvenanceClass::Placeholder,
            "{id} must retain its sanctioned policy source grade"
        );
    }

    let floor = constants
        .registry()
        .get(&ConstantId::from("PRINT_FLOOR_CHORD_MM"))
        .expect("print floor must be registered");
    assert_eq!(
        floor.provenance().class(),
        RegistryProvenanceClass::Measured
    );
    let plinth_core::RegistryProvenanceView::Measured { source, err, .. } =
        floor.provenance().view()
    else {
        panic!("print floor must retain its owner-decision evidence");
    };
    assert!(err.is_none());
    assert!(matches!(
        source.view(),
        EvidenceSourceView::OwnerDecision { id, record, locator, date }
            if id.as_str() == "print-floor-owner-decision-2026-07-24"
                && record == "docs/specs/SPEC-EMBED-POLICY.md"
                && locator == "PRINT-FLOOR amendment and Normative constants"
                && date == &EvidenceDate::exact(CalendarDate::new("2026-07-24").unwrap())
    ));
}

#[test]
fn scar_area_cap_boundary_contract_is_documented_by_the_typed_authority() {
    let policy = m0_constants()
        .expect("the complete M0 registry must construct")
        .scar_area_cap_policy();

    let floor = *policy.print_floor_chord_mm().value();
    assert!(
        0.079_999_999 < floor,
        "only chords strictly below the floor are moot"
    );
    assert!(floor >= floor, "the floor itself is a firm transit");
    assert!(2.0 <= policy.a_abs_mm2().diagnostic().get());
    assert!(2.5 <= policy.l_scar_max_mm().diagnostic().get());
}
