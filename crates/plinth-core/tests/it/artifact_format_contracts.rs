use plinth_core::{ArtifactId, Defect, Severity, SiteId, ViolationKind};

#[test]
fn artifact_site_and_format_violation_kinds_have_stable_wire_forms() {
    let site = SiteId::Artifact(ArtifactId::from("candidate-42"));
    assert_eq!(
        serde_json::to_string(&site).unwrap(),
        r#"{"kind":"artifact","id":"candidate-42"}"#
    );
    assert_eq!(
        serde_json::from_str::<SiteId>(r#"{"kind":"artifact","id":"candidate-42"}"#).unwrap(),
        site
    );

    for (kind, exact) in [
        (
            ViolationKind::ArtifactUnreadable,
            r#""artifact_unreadable""#,
        ),
        (
            ViolationKind::ChituboxStructuralFormat,
            r#""chitubox_structural_format""#,
        ),
        (
            ViolationKind::ChituboxLoadability,
            r#""chitubox_loadability""#,
        ),
    ] {
        assert_eq!(serde_json::to_string(&kind).unwrap(), exact);
        assert_eq!(serde_json::from_str::<ViolationKind>(exact).unwrap(), kind);
    }
}

#[test]
fn artifact_failure_count_is_serialized_as_a_blocking_single_failure() {
    let defect = Defect::new(
        ViolationKind::ChituboxStructuralFormat,
        SiteId::Artifact(ArtifactId::from("candidate-42")),
        Severity::Blocking,
        1.0,
        "independent structural audit rejected artifact",
    )
    .unwrap();
    let json = serde_json::to_value(&defect).unwrap();
    assert_eq!(json["measure"], 1.0);
    assert_eq!(json["severity"], "blocking");
    assert_eq!(json["kind"], "chitubox_structural_format");
}
