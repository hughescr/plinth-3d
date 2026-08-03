use plinth_core::{Defect, Severity, SiteId, ViolationKind};

#[test]
fn uncovered_island_defect_has_a_stable_finite_area_wire_contract() {
    let defect = Defect::new(
        ViolationKind::UncoveredIsland,
        SiteId::Island(17),
        Severity::Blocking,
        2.75,
        "failing uncovered area in mm2",
    )
    .unwrap();

    let exact = r#"{"kind":"uncovered_island","site":{"kind":"island","id":17},"severity":"blocking","measure":2.75,"detail":"failing uncovered area in mm2"}"#;
    assert_eq!(serde_json::to_string(&defect).unwrap(), exact);
    assert_eq!(serde_json::from_str::<Defect>(exact).unwrap(), defect);
}
