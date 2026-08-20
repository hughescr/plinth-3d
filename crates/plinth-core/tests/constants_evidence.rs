use plinth_core::{
    BoundKind, ConstantDescriptor, ConstantId, ConstantValueView, ConstantsRegistry, EvidenceDate,
    EvidenceSource, EvidenceSourceView, FiniteInterval, LoadSharing, M0EvidenceConstants, Measured,
    MicronGranularity, ProvenanceError, RegistryProvenanceClass, RegistryProvenanceView,
    YDirection,
};
use std::path::Path;

const EXPECTED_EVIDENCE_IDS: [&str; 31] = [
    "CCO_X_PAIR_LOAD_DISCOUNT",
    "DECIMATION_MAX_ABSOLUTE_DEVIATION_MM",
    "DECIMATION_MAX_P999_DEVIATION_MM",
    "DECIMATION_MAX_P99_DEVIATION_MM",
    "LAYER_HEIGHT_DEFAULT_MM",
    "LOAD_SHARING",
    "MINIMA_DROP_INTERIOR_FACES",
    "MINIMA_REQUIRE_ISLAND_BIRTH",
    "N_EFF_CAP",
    "P8_CHORD_CANON_MICRON",
    "P8_POCKET_VOXEL_MM",
    "PIXEL_PITCH_X_UM",
    "PIXEL_PITCH_Y_UM",
    "PRINT_FLOOR_CHORD_MM",
    "SATURN_BUILD_VOLUME_MM",
    "SATURN_LEADING_RELEASE_EDGE",
    "SATURN_RELEASE_FILM_THICKNESS_MM",
    "SATURN_RELEASE_FRONT_SWEEP_DIRECTION",
    "SUPPORT_GATE_MAX_WORK_PER_BATCH_UNIT",
    "SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED",
    "SUPPORT_KNN_MAX_WORK_PER_FACE",
    "SUPPORT_KNN_WORK_PER_FACE_OBSERVED",
    "SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT",
    "SUPPORT_LOAD_WORK_PER_BATCH_UNIT_OBSERVED",
    "SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE",
    "SUPPORT_PARITY_GRID_REFS_PER_FACE_OBSERVED",
    "TIP_DIAMETER_MIN_ALLOWED_MM",
    "TIP_RADIUS_MIN_ALLOWED_MM",
    "X_ALLOWED",
    "X_STAR_CONE",
    "X_STAR_CYLINDER",
];

const EXPECTED_DESCRIPTOR_CONTRACT: [(&str, &str, &str); 31] = [
    ("CCO_X_PAIR_LOAD_DISCOUNT", "1", "scalar"),
    ("DECIMATION_MAX_ABSOLUTE_DEVIATION_MM", "mm", "scalar"),
    ("DECIMATION_MAX_P999_DEVIATION_MM", "mm", "scalar"),
    ("DECIMATION_MAX_P99_DEVIATION_MM", "mm", "scalar"),
    ("LAYER_HEIGHT_DEFAULT_MM", "mm", "scalar"),
    ("LOAD_SHARING", "1", "load_sharing"),
    ("MINIMA_DROP_INTERIOR_FACES", "1", "scalar"),
    ("MINIMA_REQUIRE_ISLAND_BIRTH", "1", "scalar"),
    ("N_EFF_CAP", "count", "scalar"),
    ("P8_CHORD_CANON_MICRON", "µm", "scalar"),
    ("P8_POCKET_VOXEL_MM", "mm", "scalar"),
    ("PIXEL_PITCH_X_UM", "µm", "scalar"),
    ("PIXEL_PITCH_Y_UM", "µm", "scalar"),
    ("PRINT_FLOOR_CHORD_MM", "mm", "scalar"),
    ("SATURN_BUILD_VOLUME_MM", "mm", "record{x,y,z}"),
    ("SATURN_LEADING_RELEASE_EDGE", "enum", "y_direction"),
    ("SATURN_RELEASE_FILM_THICKNESS_MM", "mm", "scalar"),
    (
        "SATURN_RELEASE_FRONT_SWEEP_DIRECTION",
        "enum",
        "y_direction",
    ),
    (
        "SUPPORT_GATE_MAX_WORK_PER_BATCH_UNIT",
        "count/unit",
        "scalar",
    ),
    (
        "SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED",
        "count/unit",
        "scalar",
    ),
    ("SUPPORT_KNN_MAX_WORK_PER_FACE", "count/face", "scalar"),
    ("SUPPORT_KNN_WORK_PER_FACE_OBSERVED", "count/face", "scalar"),
    (
        "SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT",
        "count/unit",
        "scalar",
    ),
    (
        "SUPPORT_LOAD_WORK_PER_BATCH_UNIT_OBSERVED",
        "count/unit",
        "scalar",
    ),
    ("SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE", "B/face", "scalar"),
    (
        "SUPPORT_PARITY_GRID_REFS_PER_FACE_OBSERVED",
        "count/face",
        "scalar",
    ),
    ("TIP_DIAMETER_MIN_ALLOWED_MM", "mm", "scalar"),
    ("TIP_RADIUS_MIN_ALLOWED_MM", "mm", "scalar"),
    ("X_ALLOWED", "1", "scalar"),
    ("X_STAR_CONE", "1", "interval"),
    ("X_STAR_CYLINDER", "1", "interval"),
];

struct ExpectedSource {
    constant: &'static str,
    kind: &'static str,
    identity: &'static str,
    record: &'static str,
    locator: &'static str,
    precision: &'static str,
    date: &'static str,
}

const EXPECTED_SOURCES: [ExpectedSource; 20] = [
    ExpectedSource {
        constant: "LAYER_HEIGHT_DEFAULT_MM",
        kind: "project_record",
        identity: "saturn4-ultra-layer-profile-plate6",
        record: "docs/engineering-journal/reference/plate-06-results.md",
        locator: "§1 Layer height",
        precision: "exact",
        date: "2026-07-30",
    },
    ExpectedSource {
        constant: "LOAD_SHARING",
        kind: "calibration_plate",
        identity: "COUPON-PLATE-6-support-arm-capacity",
        record: "docs/engineering-journal/reference/plate-06-results.md",
        locator: "§5 Finding 1",
        precision: "exact",
        date: "2026-07-31",
    },
    ExpectedSource {
        constant: "MINIMA_DROP_INTERIOR_FACES",
        kind: "project_record",
        identity: "minima-contact-fix-2026-07-24",
        record: "docs/engineering-journal/reference/acceptance-run-2026-07-24-contactfix.md",
        locator: "Provenance and What moved",
        precision: "exact",
        date: "2026-07-24",
    },
    ExpectedSource {
        constant: "MINIMA_REQUIRE_ISLAND_BIRTH",
        kind: "project_record",
        identity: "minima-contact-fix-2026-07-24",
        record: "docs/engineering-journal/reference/acceptance-run-2026-07-24-contactfix.md",
        locator: "Provenance and What moved",
        precision: "exact",
        date: "2026-07-24",
    },
    ExpectedSource {
        constant: "P8_CHORD_CANON_MICRON",
        kind: "corpus_study",
        identity: "phase8-micron-chord-canonicalization-2026-07-24",
        record: "docs/engineering-journal/specs/SPEC-P8-RELOCATE.md",
        locator: "§5.3 Layer 3/amendment G",
        precision: "exact",
        date: "2026-07-24",
    },
    ExpectedSource {
        constant: "P8_POCKET_VOXEL_MM",
        kind: "owner_decision",
        identity: "p8-pocket-voxel-owner-decision",
        record: "docs/engineering-journal/specs/SPEC-P8-RELOCATE.md",
        locator: "§4.1",
        precision: "exact",
        date: "2026-07-25",
    },
    ExpectedSource {
        constant: "PIXEL_PITCH_X_UM",
        kind: "vendor_document",
        identity: "elegoo-saturn4-ultra-16k-press-2025-01-22",
        record: "docs/datasheets/printers/saturn-4-ultra-16k.md",
        locator: "Physical parameters plus project axis mapping",
        precision: "exact",
        date: "2025-01-22",
    },
    ExpectedSource {
        constant: "PIXEL_PITCH_Y_UM",
        kind: "vendor_document",
        identity: "elegoo-saturn4-ultra-16k-press-2025-01-22",
        record: "docs/datasheets/printers/saturn-4-ultra-16k.md",
        locator: "Physical parameters plus project axis mapping",
        precision: "exact",
        date: "2025-01-22",
    },
    ExpectedSource {
        constant: "PRINT_FLOOR_CHORD_MM",
        kind: "owner_decision",
        identity: "print-floor-owner-decision-2026-07-24",
        record: "docs/engineering-journal/specs/SPEC-EMBED-POLICY.md",
        locator: "PRINT-FLOOR amendment and Normative constants",
        precision: "exact",
        date: "2026-07-24",
    },
    ExpectedSource {
        constant: "SATURN_BUILD_VOLUME_MM",
        kind: "vendor_document",
        identity: "elegoo-saturn4-ultra-16k-press-2025-01-22",
        record: "docs/datasheets/printers/saturn-4-ultra-16k.md",
        locator: "Physical parameters plus project axis mapping",
        precision: "exact",
        date: "2025-01-22",
    },
    ExpectedSource {
        constant: "SATURN_LEADING_RELEASE_EDGE",
        kind: "project_record",
        identity: "saturn4-ultra-release-front-plate6",
        record: "docs/engineering-journal/reference/plate-06-results.md",
        locator: "§1 Printer",
        precision: "exact",
        date: "2026-07-31",
    },
    ExpectedSource {
        constant: "SATURN_RELEASE_FILM_THICKNESS_MM",
        kind: "installed_hardware",
        identity: "saturn4-ultra-pfa-install-2026-07-25",
        record: "docs/engineering-journal/03-physics-and-constants.md",
        locator: "§3.7 release film",
        precision: "exact",
        date: "2026-07-25",
    },
    ExpectedSource {
        constant: "SATURN_RELEASE_FRONT_SWEEP_DIRECTION",
        kind: "project_record",
        identity: "saturn4-ultra-release-front-plate6",
        record: "docs/engineering-journal/reference/plate-06-results.md",
        locator: "§1 Printer",
        precision: "exact",
        date: "2026-07-31",
    },
    ExpectedSource {
        constant: "SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED",
        kind: "corpus_study",
        identity: "support-gate-load-batch-caps-weeping-treant-2026-08-06",
        record: "docs/engineering-journal/support-resource-caps-2026-08-05.md",
        locator: "§7 FU-8b post-fix measurement (2026-08-06)",
        precision: "exact",
        date: "2026-08-06",
    },
    ExpectedSource {
        constant: "SUPPORT_LOAD_WORK_PER_BATCH_UNIT_OBSERVED",
        kind: "corpus_study",
        identity: "support-gate-load-batch-caps-weeping-treant-2026-08-06",
        record: "docs/engineering-journal/support-resource-caps-2026-08-05.md",
        locator: "§7 FU-8b post-fix measurement (2026-08-06)",
        precision: "exact",
        date: "2026-08-06",
    },
    ExpectedSource {
        constant: "SUPPORT_KNN_WORK_PER_FACE_OBSERVED",
        kind: "corpus_study",
        identity: "support-resource-caps-weeping-treant-2026-08-05",
        record: "docs/engineering-journal/support-resource-caps-2026-08-05.md",
        locator: "§2 Measured requirement",
        precision: "exact",
        date: "2026-08-05",
    },
    ExpectedSource {
        constant: "SUPPORT_PARITY_GRID_REFS_PER_FACE_OBSERVED",
        kind: "corpus_study",
        identity: "support-resource-caps-weeping-treant-2026-08-05",
        record: "docs/engineering-journal/support-resource-caps-2026-08-05.md",
        locator: "§2 Measured requirement",
        precision: "exact",
        date: "2026-08-05",
    },
    ExpectedSource {
        // Superseded 2026-08-19: the owner's ChituBox field practice (working
        // 0.100 mm DIAMETER tips) replaced the earlier 0.150 mm diameter desk
        // decision, so the date is now EXACT rather than on-or-before.
        constant: "TIP_DIAMETER_MIN_ALLOWED_MM",
        kind: "owner_decision",
        identity: "tip-diameter-floor-owner-decision",
        record: "docs/engineering-journal/03-physics-and-constants.md",
        locator: "§3.2",
        precision: "exact",
        date: "2026-08-19",
    },
    ExpectedSource {
        constant: "X_STAR_CONE",
        kind: "calibration_plate",
        identity: "COUPON-PLATE-5-compliant-vs-stiff",
        record: "docs/engineering-journal/reference/plate-05-results.md",
        locator: "§1 and §5",
        precision: "on_or_before",
        date: "2026-07-30",
    },
    ExpectedSource {
        constant: "X_STAR_CYLINDER",
        kind: "calibration_plate",
        identity: "COUPON-PLATE-6-support-arm-capacity",
        record: "docs/engineering-journal/reference/plate-06-results.md",
        locator: "§5 Finding 2",
        precision: "exact",
        date: "2026-07-31",
    },
];

fn component() -> M0EvidenceConstants {
    M0EvidenceConstants::new().expect("the closed evidence declaration set must construct")
}

fn descriptor<'a>(descriptors: &'a [ConstantDescriptor], id: &str) -> &'a ConstantDescriptor {
    descriptors
        .iter()
        .find(|descriptor| descriptor.id().as_str() == id)
        .unwrap_or_else(|| panic!("missing independently expected evidence descriptor {id}"))
}

fn scalar(descriptors: &[ConstantDescriptor], id: &str) -> f64 {
    let ConstantValueView::FiniteScalar(value) = descriptor(descriptors, id).value().view() else {
        panic!("{id} must be a finite scalar");
    };
    value
}

#[test]
fn declaration_batch_has_exact_membership_order_shapes_and_provenance_counts() {
    let descriptors = component().descriptors().unwrap();
    let actual_ids = descriptors
        .iter()
        .map(|descriptor| descriptor.id().as_str())
        .collect::<Vec<_>>();
    assert_eq!(actual_ids, EXPECTED_EVIDENCE_IDS);
    assert_eq!(descriptors.len(), 31);

    for (descriptor, (id, unit, shape)) in descriptors.iter().zip(EXPECTED_DESCRIPTOR_CONTRACT) {
        assert_eq!(descriptor.id().as_str(), id, "ID contract drift for {id}");
        assert_eq!(descriptor.unit(), unit, "unit contract drift for {id}");
        assert_eq!(descriptor.shape(), shape, "shape contract drift for {id}");
    }

    let measured = descriptors
        .iter()
        .filter(|descriptor| descriptor.provenance().class() == RegistryProvenanceClass::Measured)
        .count();
    let derived = descriptors
        .iter()
        .filter(|descriptor| descriptor.provenance().class() == RegistryProvenanceClass::Derived)
        .count();
    assert_eq!((measured, derived), (20, 11));
    assert!(descriptors.iter().all(|descriptor| !matches!(
        descriptor.provenance().class(),
        RegistryProvenanceClass::Placeholder | RegistryProvenanceClass::Pending
    )));
}

#[test]
fn descriptor_values_preserve_exact_scalar_record_direction_and_interval_types() {
    let descriptors = component().descriptors().unwrap();
    assert_eq!(
        scalar(&descriptors, "CCO_X_PAIR_LOAD_DISCOUNT"),
        0.840_896_415_253_714_5
    );
    assert_eq!(scalar(&descriptors, "LAYER_HEIGHT_DEFAULT_MM"), 0.050);
    assert_eq!(scalar(&descriptors, "N_EFF_CAP"), 6.0);
    assert_eq!(scalar(&descriptors, "P8_POCKET_VOXEL_MM"), 6.0);
    assert_eq!(scalar(&descriptors, "PIXEL_PITCH_X_UM"), 14.0);
    assert_eq!(scalar(&descriptors, "PIXEL_PITCH_Y_UM"), 19.0);
    assert_eq!(scalar(&descriptors, "PRINT_FLOOR_CHORD_MM"), 0.08);
    assert_eq!(
        scalar(&descriptors, "SATURN_RELEASE_FILM_THICKNESS_MM"),
        0.127
    );
    assert_eq!(scalar(&descriptors, "TIP_DIAMETER_MIN_ALLOWED_MM"), 0.100);
    assert_eq!(scalar(&descriptors, "TIP_RADIUS_MIN_ALLOWED_MM"), 0.050);
    assert_eq!(scalar(&descriptors, "X_ALLOWED"), 12.0);

    for id in ["MINIMA_DROP_INTERIOR_FACES", "MINIMA_REQUIRE_ISLAND_BIRTH"] {
        assert_eq!(
            descriptor(&descriptors, id).value().view(),
            ConstantValueView::Bool(true)
        );
    }
    assert_eq!(
        descriptor(&descriptors, "P8_CHORD_CANON_MICRON")
            .value()
            .view(),
        ConstantValueView::UnsignedInteger(1)
    );
    // The support resource-cap rule, stated as arithmetic the test can check:
    // the coefficient is next_power_of_two(2 x observed_per_face), and the
    // parity-grid observation reaches bytes through parity.rs's own exact
    // projection term 81*faces + 36*references + 4.
    assert_eq!(
        descriptor(&descriptors, "SUPPORT_KNN_WORK_PER_FACE_OBSERVED")
            .value()
            .view(),
        ConstantValueView::UnsignedInteger(696)
    );
    assert_eq!(298_667_520_u64 / 429_120, 696);
    assert_eq!(
        descriptor(&descriptors, "SUPPORT_KNN_MAX_WORK_PER_FACE")
            .value()
            .view(),
        ConstantValueView::UnsignedInteger((2 * 696_u64).next_power_of_two())
    );
    // The same rule applied to the two batch-product coefficients. The scale
    // is the batch's own product, not the face count: gate and load work were
    // measured not to track faces (24-face make_raking_blade charges
    // 612,887,212 gate operations, 429,120-face Weeping Treant 12,714,828,544).
    let gate_batch_units = (405_146_u64 + 1) * (1_366 + 1);
    assert_eq!(gate_batch_units, 553_835_949);
    assert_eq!(
        descriptor(&descriptors, "SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED")
            .value()
            .view(),
        ConstantValueView::UnsignedInteger(12_714_828_544_u64.div_ceil(gate_batch_units))
    );
    assert_eq!(
        descriptor(&descriptors, "SUPPORT_GATE_MAX_WORK_PER_BATCH_UNIT")
            .value()
            .view(),
        ConstantValueView::UnsignedInteger(
            (2 * 12_714_828_544_u64.div_ceil(gate_batch_units)).next_power_of_two()
        )
    );
    // Re-pinned 2026-08-06 by record fix F1. The demand 5,402,535,619 was
    // instrumented back to `finalize_specialized_load_partition`, whose plan
    // states 130,548 rows — the same row count the census scaled-cap row
    // prints. The 134,368 this line used to name belongs to no planned batch,
    // so the batch units and the observed ceiling both move (29.41 -> 30.28,
    // 30 -> 31). The coefficient below is unchanged at 64: next_power_of_two
    // rounds 60 and 62 alike.
    let load_batch_units = (130_548_u64 + 1) * (1_366 + 1);
    assert_eq!(load_batch_units, 178_460_483);
    assert_eq!(
        descriptor(&descriptors, "SUPPORT_LOAD_WORK_PER_BATCH_UNIT_OBSERVED")
            .value()
            .view(),
        ConstantValueView::UnsignedInteger(5_402_535_619_u64.div_ceil(load_batch_units))
    );
    assert_eq!(
        descriptor(&descriptors, "SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT")
            .value()
            .view(),
        ConstantValueView::UnsignedInteger(
            (2 * 5_402_535_619_u64.div_ceil(load_batch_units)).next_power_of_two()
        )
    );
    let refs_per_face = scalar(&descriptors, "SUPPORT_PARITY_GRID_REFS_PER_FACE_OBSERVED");
    assert_eq!(refs_per_face, 1_214_544.0 / 429_120.0);
    assert_eq!(81 * 429_120_u64 + 36 * 1_214_544 + 4, 78_482_308);
    let observed_bytes_per_face = 81.0 + 36.0 * refs_per_face;
    assert_eq!(
        descriptor(&descriptors, "SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE")
            .value()
            .view(),
        ConstantValueView::UnsignedInteger(
            ((2.0 * observed_bytes_per_face) as u64).next_power_of_two()
        )
    );
    assert_eq!(
        descriptor(&descriptors, "SATURN_LEADING_RELEASE_EDGE")
            .value()
            .view(),
        ConstantValueView::Text("negative_y")
    );
    assert_eq!(
        descriptor(&descriptors, "SATURN_RELEASE_FRONT_SWEEP_DIRECTION")
            .value()
            .view(),
        ConstantValueView::Text("positive_y")
    );

    let ConstantValueView::LoadSharing(sharing) =
        descriptor(&descriptors, "LOAD_SHARING").value().view()
    else {
        panic!("LOAD_SHARING must retain its typed registry variant");
    };
    assert_eq!(sharing, &LoadSharing::additive(6.0).unwrap());

    let ConstantValueView::Record(volume) = descriptor(&descriptors, "SATURN_BUILD_VOLUME_MM")
        .value()
        .view()
    else {
        panic!("SATURN_BUILD_VOLUME_MM must retain its typed record");
    };
    assert_eq!(volume.len(), 3);
    assert_eq!(volume["x"].view(), ConstantValueView::FiniteScalar(211.68));
    assert_eq!(volume["y"].view(), ConstantValueView::FiniteScalar(118.37));
    assert_eq!(volume["z"].view(), ConstantValueView::FiniteScalar(220.0));

    assert_interval(
        descriptor(&descriptors, "X_STAR_CONE"),
        25.00,
        25.34,
        BoundKind::Inclusive,
        BoundKind::Inclusive,
    );
    assert_interval(
        descriptor(&descriptors, "X_STAR_CYLINDER"),
        19.91,
        30.48,
        BoundKind::Exclusive,
        BoundKind::Exclusive,
    );
}

fn assert_interval(
    descriptor: &ConstantDescriptor,
    lower: f64,
    upper: f64,
    lower_bound: BoundKind,
    upper_bound: BoundKind,
) {
    let ConstantValueView::FiniteInterval(interval) = descriptor.value().view() else {
        panic!(
            "{} must retain its finite interval",
            descriptor.id().as_str()
        );
    };
    assert_eq!(interval.lower(), lower);
    assert_eq!(interval.upper(), upper);
    assert_eq!(interval.lower_bound(), lower_bound);
    assert_eq!(interval.upper_bound(), upper_bound);
}

#[test]
fn every_measured_descriptor_has_the_exact_source_date_and_null_error() {
    let descriptors = component().descriptors().unwrap();
    for expected in EXPECTED_SOURCES {
        let descriptor = descriptor(&descriptors, expected.constant);
        let RegistryProvenanceView::Measured { source, err, .. } = descriptor.provenance().view()
        else {
            panic!("{} must be measured", expected.constant);
        };
        assert!(err.is_none(), "{} must carry err: null", expected.constant);
        assert_source(source, &expected);
    }
}

fn assert_source(source: &EvidenceSource, expected: &ExpectedSource) {
    let (kind, identity, record, locator, date) = match source.view() {
        EvidenceSourceView::CalibrationPlate {
            plate,
            record,
            locator,
            date,
        } => ("calibration_plate", plate.as_str(), record, locator, date),
        EvidenceSourceView::VendorDocument {
            id,
            record,
            locator,
            date,
        } => ("vendor_document", id.as_str(), record, locator, date),
        EvidenceSourceView::InstalledHardware {
            id,
            record,
            locator,
            date,
        } => ("installed_hardware", id.as_str(), record, locator, date),
        EvidenceSourceView::CorpusStudy {
            id,
            record,
            locator,
            date,
        } => ("corpus_study", id.as_str(), record, locator, date),
        EvidenceSourceView::ProjectRecord {
            id,
            record,
            locator,
            date,
        } => ("project_record", id.as_str(), record, locator, date),
        EvidenceSourceView::OwnerDecision {
            id,
            record,
            locator,
            date,
        } => ("owner_decision", id.as_str(), record, locator, date),
    };
    assert_eq!(kind, expected.kind, "source kind for {}", expected.constant);
    assert_eq!(
        identity, expected.identity,
        "source identity for {}",
        expected.constant
    );
    assert_eq!(
        record, expected.record,
        "source record for {}",
        expected.constant
    );
    assert_eq!(
        locator, expected.locator,
        "source locator for {}",
        expected.constant
    );
    let (precision, value) = match date {
        EvidenceDate::Exact(date) => ("exact", date.as_str()),
        EvidenceDate::OnOrBefore(date) => ("on_or_before", date.as_str()),
        EvidenceDate::Unknown => ("unknown", ""),
    };
    assert_eq!(
        precision, expected.precision,
        "date precision for {}",
        expected.constant
    );
    assert_eq!(value, expected.date, "date for {}", expected.constant);
}

#[test]
fn every_measured_wrapper_roundtrips_with_an_explicit_null_error() {
    let constants = component();
    macro_rules! assert_null_roundtrip {
        ($measured:expr, $payload:ty, $id:literal) => {{
            let wire = serde_json::to_value($measured).unwrap();
            assert_eq!(wire.get("err"), Some(&serde_json::Value::Null), $id);
            let decoded: Measured<$payload> = serde_json::from_value(wire).unwrap();
            assert_eq!(&decoded, $measured, $id);
        }};
    }

    assert_null_roundtrip!(constants.layer_height_default_mm(), f64, "layer height");
    assert_null_roundtrip!(constants.load_sharing(), LoadSharing, "load sharing");
    assert_null_roundtrip!(constants.minima_drop_interior_faces(), bool, "drop faces");
    assert_null_roundtrip!(
        constants.minima_require_island_birth(),
        bool,
        "island birth"
    );
    assert_null_roundtrip!(
        constants.p8_chord_canon_micron(),
        MicronGranularity,
        "micron canon"
    );
    assert_null_roundtrip!(constants.p8_pocket_voxel_mm(), f64, "pocket voxel");
    assert_null_roundtrip!(constants.pixel_pitch_x_um(), f64, "pitch X");
    assert_null_roundtrip!(constants.pixel_pitch_y_um(), f64, "pitch Y");
    assert_null_roundtrip!(constants.print_floor_chord_mm(), f64, "print floor");
    assert_null_roundtrip!(
        constants.saturn_build_volume_mm(),
        plinth_core::BuildVolumeMm,
        "build volume"
    );
    assert_null_roundtrip!(
        constants.saturn_leading_release_edge(),
        YDirection,
        "leading edge"
    );
    assert_null_roundtrip!(
        constants.saturn_release_film_thickness_mm(),
        f64,
        "release film"
    );
    assert_null_roundtrip!(
        constants.saturn_release_front_sweep_direction(),
        YDirection,
        "front sweep"
    );
    assert_null_roundtrip!(constants.tip_diameter_min_allowed_mm(), f64, "tip diameter");
    assert_null_roundtrip!(constants.x_star_cone(), FiniteInterval, "cone bracket");
    assert_null_roundtrip!(
        constants.x_star_cylinder(),
        FiniteInterval,
        "cylinder bracket"
    );
}

#[test]
fn micron_measurement_is_sealed_to_null_error_and_unsigned_registry_value() {
    let source = component().p8_chord_canon_micron().source().clone();
    assert_eq!(
        Measured::new(
            MicronGranularity::ONE_MICRON,
            source,
            Some(MicronGranularity::ONE_MICRON),
            "invalid error bar"
        ),
        Err(ProvenanceError::CategoricalErrorBar)
    );
    assert_eq!(
        component()
            .descriptors()
            .unwrap()
            .iter()
            .find(|descriptor| descriptor.id().as_str() == "P8_CHORD_CANON_MICRON")
            .unwrap()
            .value()
            .view(),
        ConstantValueView::UnsignedInteger(1)
    );
}

#[test]
fn derived_sources_and_relations_are_exact_and_do_not_launder_authority() {
    let constants = component();
    let descriptors = constants.descriptors().unwrap();
    let expected = [
        ("CCO_X_PAIR_LOAD_DISCOUNT", &[][..]),
        ("N_EFF_CAP", &["LOAD_SHARING"] as &[&str]),
        (
            "TIP_RADIUS_MIN_ALLOWED_MM",
            &["TIP_DIAMETER_MIN_ALLOWED_MM"] as &[&str],
        ),
        ("X_ALLOWED", &["X_STAR_CONE"] as &[&str]),
    ];
    for (id, expected_from) in expected {
        let RegistryProvenanceView::Derived { from, note } =
            descriptor(&descriptors, id).provenance().view()
        else {
            panic!("{id} must be derived");
        };
        assert_eq!(
            from.iter().map(String::as_str).collect::<Vec<_>>(),
            expected_from,
            "derived source list for {id}"
        );
        assert!(!note.trim().is_empty(), "{id} must retain its proof note");
    }

    assert_eq!(
        *constants.cco_x_pair_load_discount().value(),
        0.5_f64.powf(0.25)
    );
    assert_eq!(
        *constants.n_eff_cap().value(),
        constants.load_sharing().value().n_eff_cap().unwrap()
    );
    assert_eq!(
        *constants.tip_radius_min_allowed_mm().value(),
        constants.tip_diameter_min_allowed_mm().value() / 2.0
    );
    assert_eq!(
        *constants.x_allowed().value(),
        (constants.x_star_cone().value().lower() / 2.0).floor()
    );
    assert!(constants
        .cco_x_pair_load_discount()
        .note
        .contains("docs/engineering-journal/reference/physics_transient_peel.md"));
    assert!(constants.x_allowed().note.contains("19.91/12 = 1.66×"));
}

#[test]
fn load_sharing_note_carries_every_checkpoint_caveat_and_override_marker() {
    let constants = component();
    let note = constants.load_sharing().note();
    for marker in [
        "one cap area (30 mm²)",
        "one arm diameter (0.798 mm)",
        "one arm length (4.00 mm)",
        "No strip failed",
        "N > 6 is extrapolation",
        "19.75% section bonus",
        "at least N×",
        "never evidence for super-additive",
        "algebraically degenerate with total arm area",
        "rests on plate 5",
        "30.077 alive versus 30.481 dead",
        "1.33% window",
        "does not prove exact additivity",
        "§5 Finding 3",
        "both axes",
        "zero-variance and near-boundary axis effects untested",
        "four X-axis cells were not zipper tests",
        "docs/engineering-journal/reference/plate-06-results.md §6",
        "post-hoc",
        "counterargument unanswered",
    ] {
        assert!(note.contains(marker), "LOAD_SHARING note missing: {marker}");
    }
}

#[test]
fn evidence_record_paths_are_named_and_repository_locatable() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (constant, record) in [
        (
            "LOAD_SHARING and plate-6 declarations",
            "docs/engineering-journal/reference/plate-06-results.md",
        ),
        (
            "MINIMA declarations",
            "docs/engineering-journal/reference/acceptance-run-2026-07-24-contactfix.md",
        ),
        (
            "Saturn vendor declarations",
            "docs/datasheets/printers/saturn-4-ultra-16k.md",
        ),
        (
            "installed film and tip floor",
            "docs/engineering-journal/03-physics-and-constants.md",
        ),
        (
            "Phase-8 declarations",
            "docs/engineering-journal/specs/SPEC-P8-RELOCATE.md",
        ),
        (
            "print-floor chord",
            "docs/engineering-journal/specs/SPEC-EMBED-POLICY.md",
        ),
        (
            "X_STAR_CONE",
            "docs/engineering-journal/reference/plate-05-results.md",
        ),
        (
            "CCO_X_PAIR_LOAD_DISCOUNT proof",
            "docs/engineering-journal/reference/physics_transient_peel.md",
        ),
    ] {
        let path = repository.join(record);
        assert!(
            path.is_file(),
            "evidence record for {constant} is missing: {}",
            path.display()
        );
    }
}

#[test]
fn registry_rendering_is_deterministic_complete_and_in_stable_order() {
    let descriptors = component().descriptors().unwrap();
    let registry = ConstantsRegistry::build_batch(descriptors.clone()).unwrap();
    let reversed = ConstantsRegistry::build_batch(descriptors.into_iter().rev()).unwrap();
    assert_eq!(registry, reversed);
    let rendered = registry.render_markdown();
    assert_eq!(rendered, reversed.render_markdown());
    assert!(!rendered.contains("Placeholder"));
    assert!(!rendered.contains("Pending"));
    assert!(!rendered.contains("placeholder"));
    assert!(!rendered.contains("pending"));

    // Anchor on the row's own leading cell, not on any occurrence of the ID.
    // A `Derived` entry names its inputs in `from=[...]` and every note may
    // name a sibling constant, so a bare substring search finds cross
    // references rather than rows, and reports drift where there is none.
    let mut previous = 0;
    for id in EXPECTED_EVIDENCE_IDS {
        let row_anchor = format!("\n| {} | ", id.replace('_', "&#95;"));
        let position = rendered
            .find(&row_anchor)
            .unwrap_or_else(|| panic!("deterministic renderer omitted {id}"));
        assert!(position >= previous, "renderer order drift at {id}");
        previous = position;
    }
}

#[test]
fn release_directions_are_distinct_named_facts() {
    let constants = component();
    assert_eq!(
        constants.saturn_leading_release_edge().value(),
        &YDirection::NegativeY
    );
    assert_eq!(
        constants.saturn_release_front_sweep_direction().value(),
        &YDirection::PositiveY
    );
    assert_ne!(
        ConstantId::from("SATURN_LEADING_RELEASE_EDGE"),
        ConstantId::from("SATURN_RELEASE_FRONT_SWEEP_DIRECTION")
    );
}

/// Craig's 2026-08-19 supersession, pinned in BOTH units and in both
/// directions of the diameter/radius conversion.
///
/// The earlier owner decision — 0.150 mm DIAMETER (0.075 mm RADIUS), dated
/// on-or-before 2026-07-31 — would have refused the tips Craig prints today.
/// The ruling that lowered the generation clamp `TIP_RADIUS_DEFAULT_MM` to
/// 0.050 mm radius (0.100 mm diameter) rests on the same field data, so the
/// validator minimum moved with it and the conflict the earlier lane flagged
/// is closed.
///
/// **What this test does NOT assert.** It does not assert that the validator
/// minimum is DERIVED from the generation clamp, and the two must never be
/// written in terms of each other: one is what the sizing policy is allowed to
/// ask for, the other is what the validator will accept. They are equal today
/// because one person ruled once on one body of evidence. If a later ruling
/// moves only one of them, this test goes red and that is correct — it is a
/// prompt to re-read both notes, not a licence to re-point one at the other.
#[test]
fn the_validator_tip_minimum_was_superseded_to_the_owners_own_field_practice() {
    let descriptors = component().descriptors().unwrap();

    // The registered pair, stated in the unit each one is registered in.
    let diameter_min_mm = scalar(&descriptors, "TIP_DIAMETER_MIN_ALLOWED_MM");
    let radius_min_mm = scalar(&descriptors, "TIP_RADIUS_MIN_ALLOWED_MM");
    assert_eq!(
        diameter_min_mm, 0.100,
        "validator minimum is 0.100 mm DIAMETER since Craig's 2026-08-19 \
         supersession, from 0.150 mm diameter"
    );
    assert_eq!(
        radius_min_mm, 0.050,
        "validator minimum is 0.050 mm RADIUS since Craig's 2026-08-19 \
         supersession, from 0.075 mm radius"
    );
    // The derivation identity itself, so a hand-typed radius that drifts from
    // its parent diameter cannot pass.
    assert_eq!(
        radius_min_mm,
        diameter_min_mm / 2.0,
        "TIP_RADIUS_MIN_ALLOWED_MM is exactly half its parent DIAMETER"
    );

    // The validator minimum no longer refuses what the sizing policy is
    // allowed to generate. Before the supersession the generation clamp
    // minimum (0.050 mm radius) sat BELOW the validator minimum (0.075 mm
    // radius), so every tip the policy sized at its own minimum was one the
    // validator would have rejected had anything read it.
    let generation_clamp_min_radius_mm = plinth_core::m0_constants()
        .unwrap()
        .placeholders()
        .tip_radius_default_mm()
        .diagnostic()
        .get();
    assert_eq!(
        generation_clamp_min_radius_mm, 0.050,
        "TIP_RADIUS_DEFAULT_MM is 0.050 mm RADIUS (0.100 mm DIAMETER)"
    );
    assert!(
        generation_clamp_min_radius_mm >= radius_min_mm,
        "the smallest tip the sizing policy may generate ({generation_clamp_min_radius_mm} mm \
         radius / {} mm diameter) must not be one the validator minimum \
         ({radius_min_mm} mm radius / {diameter_min_mm} mm diameter) refuses",
        generation_clamp_min_radius_mm * 2.0
    );

    // The supersession is on the record as an EXACT date, not the earlier
    // on-or-before. A supersession whose provenance still pointed at
    // 2026-07-31 would be claiming the old decision produced the new number.
    let RegistryProvenanceView::Measured { source, err, .. } =
        descriptor(&descriptors, "TIP_DIAMETER_MIN_ALLOWED_MM")
            .provenance()
            .view()
    else {
        panic!("the validator minimum is a Measured owner decision");
    };
    assert!(err.is_none());
    assert_source(
        source,
        &ExpectedSource {
            constant: "TIP_DIAMETER_MIN_ALLOWED_MM",
            kind: "owner_decision",
            identity: "tip-diameter-floor-owner-decision",
            record: "docs/engineering-journal/03-physics-and-constants.md",
            locator: "§3.2",
            precision: "exact",
            date: "2026-08-19",
        },
    );
}
