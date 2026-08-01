use plinth_core::{
    BoundKind, CalendarDate, ConstantDescriptor, ConstantId, ConstantValue, ConstantValueView,
    ConstantsRegistry, Derived, EnumValue, EvidenceDate, EvidenceSource, FiniteInterval,
    LoadSharing, Measured, Pending, Placeholder, PlateId, RegistryError, RegistryProvenanceClass,
    RegistryProvenanceView, ToConstantValue,
};
use std::collections::BTreeMap;

fn measured_source() -> EvidenceSource {
    EvidenceSource::calibration_plate(
        PlateId::from("plate-1"),
        "records/evidence.md",
        "#row|1",
        EvidenceDate::exact(CalendarDate::new("2026-07-31").unwrap()),
    )
    .unwrap()
}

fn sample_descriptors() -> Vec<ConstantDescriptor> {
    let measured = Measured::new(-0.0_f64, measured_source(), Some(0.25), "measured note").unwrap();
    let derived = Derived::new(vec![2_i64, 1_i64], ["alpha|measured"], "computed | stable");
    let pending = Pending::new(false, true, PlateId::from("plate-next"), "choose branch");
    let placeholder = Placeholder::new(EnumValue::new("negative_y").unwrap(), "guess <only>");

    vec![
        ConstantDescriptor::from_placeholder(
            ConstantId::from("gamma"),
            "enum",
            "scalar",
            &placeholder,
        )
        .unwrap(),
        ConstantDescriptor::from_pending(ConstantId::from("delta"), "1", "branch", &pending)
            .unwrap(),
        ConstantDescriptor::from_derived(ConstantId::from("beta"), "count", "list[2]", &derived)
            .unwrap(),
        ConstantDescriptor::from_measured(
            ConstantId::from("alpha|measured"),
            "mm|s",
            "scalar",
            &measured,
        )
        .unwrap(),
    ]
}

#[test]
fn complete_batch_is_atomic_immutable_and_sorted() {
    let descriptors = sample_descriptors();
    let registry = ConstantsRegistry::build_complete(descriptors.clone()).unwrap();
    assert_eq!(registry.len(), 4);
    assert!(!registry.is_empty());
    assert_eq!(
        registry
            .iter()
            .map(|descriptor| descriptor.id().as_str())
            .collect::<Vec<_>>(),
        ["alpha|measured", "beta", "delta", "gamma"]
    );
    assert_eq!(
        registry.get(&ConstantId::from("beta")).unwrap().shape(),
        "list[2]"
    );

    let mut reversed = descriptors;
    reversed.reverse();
    let reversed_registry = ConstantsRegistry::build_complete(reversed).unwrap();
    assert_eq!(registry, reversed_registry);
    assert_eq!(
        registry.render_markdown(),
        reversed_registry.render_markdown()
    );
}

#[test]
fn duplicate_or_empty_batch_never_publishes_a_registry() {
    assert_eq!(
        ConstantsRegistry::build_complete(Vec::new()),
        Err(RegistryError::EmptyBatch)
    );

    let duplicate = Derived::new(2_i64, ["source"], "derived");
    let first = ConstantDescriptor::from_derived(
        ConstantId::from("duplicate"),
        "count",
        "scalar",
        &duplicate,
    )
    .unwrap();
    let second = ConstantDescriptor::from_derived(
        ConstantId::from("duplicate"),
        "count",
        "scalar",
        &duplicate,
    )
    .unwrap();
    assert_eq!(
        ConstantsRegistry::build_complete([first, second]),
        Err(RegistryError::DuplicateConstantId {
            id: ConstantId::from("duplicate")
        })
    );
}

#[test]
fn every_constant_value_variant_converts_without_debug_fallbacks() {
    assert_eq!(
        true.to_constant_value().unwrap().view(),
        ConstantValueView::Bool(true)
    );
    assert_eq!(
        (-4_i64).to_constant_value().unwrap().view(),
        ConstantValueView::SignedInteger(-4)
    );
    assert_eq!(
        9_u64.to_constant_value().unwrap().view(),
        ConstantValueView::UnsignedInteger(9)
    );
    let negative_zero = (-0.0_f64).to_constant_value().unwrap();
    let ConstantValueView::FiniteScalar(value) = negative_zero.view() else {
        panic!("finite scalar converted to the wrong variant");
    };
    assert_eq!(value.to_bits(), 0.0_f64.to_bits());
    assert_eq!(
        "text".to_constant_value().unwrap().view(),
        ConstantValueView::Text("text")
    );
    let enumeration = EnumValue::new("positive_y").unwrap();
    assert_eq!(
        enumeration.to_constant_value().unwrap().view(),
        ConstantValueView::Enumeration("positive_y")
    );

    let interval =
        FiniteInterval::new(1.0, 2.0, BoundKind::Exclusive, BoundKind::Inclusive).unwrap();
    assert_eq!(
        interval.to_constant_value().unwrap().view(),
        ConstantValueView::FiniteInterval(&interval)
    );
    let sharing = LoadSharing::additive(6.0).unwrap();
    assert_eq!(
        sharing.to_constant_value().unwrap().view(),
        ConstantValueView::LoadSharing(&sharing)
    );

    let list = vec![1_i64, 2_i64].to_constant_value().unwrap();
    let ConstantValueView::List(values) = list.view() else {
        panic!("list converted to the wrong variant");
    };
    assert_eq!(values.len(), 2);

    let mut fields = BTreeMap::new();
    fields.insert("b".to_owned(), ConstantValue::boolean(false));
    fields.insert("a".to_owned(), ConstantValue::unsigned_integer(3));
    let record = fields.to_constant_value().unwrap();
    let ConstantValueView::Record(fields) = record.view() else {
        panic!("record converted to the wrong variant");
    };
    assert_eq!(
        fields.keys().map(String::as_str).collect::<Vec<_>>(),
        ["a", "b"]
    );
}

#[test]
fn invalid_metadata_and_values_fail_before_batch_publication() {
    assert_eq!(
        f64::NAN.to_constant_value(),
        Err(RegistryError::NonFiniteScalar)
    );
    assert!(ConstantValue::text("").is_err());
    assert!(ConstantValue::text(" text").is_err());
    assert!(EnumValue::new("enum ").is_err());
    assert_eq!(
        ConstantValue::record([("field", true), ("field", false)]),
        Err(RegistryError::DuplicateRecordField {
            field: "field".to_owned()
        })
    );

    let valid = Derived::new(1_i64, ["source"], "note");
    assert!(
        ConstantDescriptor::from_derived(ConstantId::from(""), "count", "scalar", &valid).is_err()
    );
    assert!(
        ConstantDescriptor::from_derived(ConstantId::from(" id"), "count", "scalar", &valid)
            .is_err()
    );
    assert!(
        ConstantDescriptor::from_derived(ConstantId::from("id"), "", "scalar", &valid).is_err()
    );
    assert!(
        ConstantDescriptor::from_derived(ConstantId::from("id"), "count", "scalar ", &valid)
            .is_err()
    );

    let no_sources = Derived::new(1_i64, std::iter::empty::<&str>(), "note");
    assert_eq!(
        ConstantDescriptor::from_derived(
            ConstantId::from("no_sources"),
            "count",
            "scalar",
            &no_sources
        ),
        Err(RegistryError::EmptyDerivedSources)
    );
    let bad_note = Placeholder::new(1_i64, " note");
    assert!(ConstantDescriptor::from_placeholder(
        ConstantId::from("bad_note"),
        "count",
        "scalar",
        &bad_note
    )
    .is_err());
    let bad_pending = Pending::new(1_i64, 2_i64, PlateId::from(""), "resolve measurement");
    assert!(ConstantDescriptor::from_pending(
        ConstantId::from("bad_pending"),
        "count",
        "branch",
        &bad_pending
    )
    .is_err());
}

#[test]
fn provenance_is_extracted_only_from_actual_wrappers() {
    let measured = Measured::new(3.0_f64, measured_source(), Some(0.1), "measured").unwrap();
    let measured_descriptor =
        ConstantDescriptor::from_measured(ConstantId::from("measured"), "mm", "scalar", &measured)
            .unwrap();
    assert_eq!(
        measured_descriptor.provenance().class(),
        RegistryProvenanceClass::Measured
    );
    let RegistryProvenanceView::Measured {
        source,
        err: Some(err),
        note,
    } = measured_descriptor.provenance().view()
    else {
        panic!("measured wrapper produced the wrong registry provenance");
    };
    assert_eq!(source, measured.source());
    assert_eq!(err.view(), ConstantValueView::FiniteScalar(0.1));
    assert_eq!(note, "measured");

    let derived = Derived::new(4_i64, ["measured"], "derived");
    let derived_descriptor =
        ConstantDescriptor::from_derived(ConstantId::from("derived"), "count", "scalar", &derived)
            .unwrap();
    assert_eq!(
        derived_descriptor.provenance().view(),
        RegistryProvenanceView::Derived {
            from: &["measured".to_owned()],
            note: "derived"
        }
    );

    let placeholder = Placeholder::new(false, "placeholder");
    let placeholder_descriptor = ConstantDescriptor::from_placeholder(
        ConstantId::from("placeholder"),
        "1",
        "scalar",
        &placeholder,
    )
    .unwrap();
    assert_eq!(
        placeholder_descriptor.provenance().view(),
        RegistryProvenanceView::Placeholder {
            note: "placeholder"
        }
    );

    let pending = Pending::new(
        5_i64,
        6_i64,
        PlateId::from("plate-future"),
        "measure branch",
    );
    let pending_descriptor =
        ConstantDescriptor::from_pending(ConstantId::from("pending"), "count", "branch", &pending)
            .unwrap();
    assert_eq!(
        pending_descriptor.provenance().class(),
        RegistryProvenanceClass::Pending
    );
    let RegistryProvenanceView::Pending {
        conservative,
        optimistic,
        resolves_on,
        question,
    } = pending_descriptor.provenance().view()
    else {
        panic!("pending wrapper produced the wrong registry provenance");
    };
    assert_eq!(conservative.view(), ConstantValueView::SignedInteger(5));
    assert_eq!(optimistic.view(), ConstantValueView::SignedInteger(6));
    assert_eq!(resolves_on, &PlateId::from("plate-future"));
    assert_eq!(question, "measure branch");
}

#[test]
fn markdown_renderer_is_exact_escaped_and_input_order_invariant() {
    let registry = ConstantsRegistry::build_complete(sample_descriptors()).unwrap();
    assert_eq!(
        registry.render_markdown(),
        r##"# Constants Registry

| ID | Value | Unit | Shape | Provenance |
| --- | --- | --- | --- | --- |
| alpha&#124;measured | 0.0 | mm&#124;s | scalar | measured(source=calibration_plate(plate="plate-1",record="records/evidence.md",locator="#row&#124;1",date=exact("2026-07-31")),err=0.25,note="measured note") |
| beta | [2,1] | count | list[2] | derived(from=["alpha&#124;measured"],note="computed &#124; stable") |
| delta | {"conservative":false,"optimistic":true} | 1 | branch | pending(conservative=false,optimistic=true,resolves_on="plate-next",question="choose branch") |
| gamma | enum("negative_y") | enum | scalar | placeholder(note="guess &lt;only&gt;") |
"##
    );
}
