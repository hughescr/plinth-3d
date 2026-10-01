use plinth_core::{
    BoundKind, ConstantDescriptor, ConstantId, ConstantValueView, ConstantsRegistry, EvidenceDate,
    EvidenceSource, EvidenceSourceView, FiniteInterval, LoadSharing, M0EvidenceConstants, Measured,
    MicronGranularity, ProvenanceError, RegistryProvenanceClass, RegistryProvenanceView,
    YDirection,
};
use std::path::Path;

const EXPECTED_EVIDENCE_IDS: [&str; 52] = [
    "CCO_X_PAIR_LOAD_DISCOUNT",
    "CHITUBOX_BOSS_BOTTOM_DIAMETER_MM",
    "CHITUBOX_BOSS_HEIGHT_MM",
    "CHITUBOX_PAD_DIAMETER_MM",
    "CHITUBOX_RAFT_HEIGHT_MM",
    "CHITUBOX_ROOT_BOSS_PROFILE",
    "CHITUBOX_TIP_CONTACT_PROFILES",
    "CONDITION_SHIFT_MAX",
    "DECIMATION_MAX_ABSOLUTE_DEVIATION_MM",
    "DECIMATION_MAX_P999_DEVIATION_MM",
    "DECIMATION_MAX_P99_DEVIATION_MM",
    "FINAL_CLEARANCE_GAP_MM",
    "LAMINA_REACH_RATIO",
    "LAMINA_REACH_RATIO_AUDIT_GATE",
    "LAMINA_REACH_RATIO_FIRST_LOSS",
    "LAYER_HEIGHT_DEFAULT_MM",
    "LOAD_SHARING",
    "L_FIRST_LOSS_MM",
    "L_SPAN_ALLOWED_MM",
    "L_SPAN_CRIT_MM",
    "L_SURVIVED_MAX_MM",
    "MINIMA_DROP_INTERIOR_FACES",
    "MINIMA_REQUIRE_ISLAND_BIRTH",
    "N_EFF_CAP",
    "ORIENT_PEEL_AREA_EXPONENT_HIGH",
    "ORIENT_PEEL_AREA_EXPONENT_LOW",
    "ORIENT_PEEL_REFERENCE_AREA_MM2",
    "ORIENT_PEEL_REFERENCE_PRESSURE_PA_HIGH",
    "ORIENT_PEEL_REFERENCE_PRESSURE_PA_LOW",
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
    "TIP_RADIUS_MAX_GROWN_SCAR_MM",
    "TIP_RADIUS_MIN_ALLOWED_MM",
    "X_ALLOWED",
    "X_STAR_CONE",
    "X_STAR_CYLINDER",
];

const EXPECTED_DESCRIPTOR_CONTRACT: [(&str, &str, &str); 52] = [
    ("CCO_X_PAIR_LOAD_DISCOUNT", "1", "scalar"),
    ("CHITUBOX_BOSS_BOTTOM_DIAMETER_MM", "mm", "scalar"),
    ("CHITUBOX_BOSS_HEIGHT_MM", "mm", "scalar"),
    ("CHITUBOX_PAD_DIAMETER_MM", "mm", "scalar"),
    ("CHITUBOX_RAFT_HEIGHT_MM", "mm", "scalar"),
    (
        "CHITUBOX_ROOT_BOSS_PROFILE",
        "mm",
        "record{name,shaft_diameter_mm,boss_bottom_diameter_mm}",
    ),
    (
        "CHITUBOX_TIP_CONTACT_PROFILES",
        "mm",
        "list[4]{name,tip_up_diameter_mm,touch_diameter_mm}",
    ),
    ("CONDITION_SHIFT_MAX", "1", "scalar"),
    ("DECIMATION_MAX_ABSOLUTE_DEVIATION_MM", "mm", "scalar"),
    ("DECIMATION_MAX_P999_DEVIATION_MM", "mm", "scalar"),
    ("DECIMATION_MAX_P99_DEVIATION_MM", "mm", "scalar"),
    ("FINAL_CLEARANCE_GAP_MM", "mm", "scalar"),
    ("LAMINA_REACH_RATIO", "1", "scalar"),
    ("LAMINA_REACH_RATIO_AUDIT_GATE", "1", "scalar"),
    ("LAMINA_REACH_RATIO_FIRST_LOSS", "1", "scalar"),
    ("LAYER_HEIGHT_DEFAULT_MM", "mm", "scalar"),
    ("LOAD_SHARING", "1", "load_sharing"),
    ("L_FIRST_LOSS_MM", "mm", "scalar"),
    ("L_SPAN_ALLOWED_MM", "mm", "scalar"),
    ("L_SPAN_CRIT_MM", "mm", "scalar"),
    ("L_SURVIVED_MAX_MM", "mm", "scalar"),
    ("MINIMA_DROP_INTERIOR_FACES", "1", "scalar"),
    ("MINIMA_REQUIRE_ISLAND_BIRTH", "1", "scalar"),
    ("N_EFF_CAP", "count", "scalar"),
    ("ORIENT_PEEL_AREA_EXPONENT_HIGH", "1", "scalar"),
    ("ORIENT_PEEL_AREA_EXPONENT_LOW", "1", "scalar"),
    ("ORIENT_PEEL_REFERENCE_AREA_MM2", "mm²", "scalar"),
    ("ORIENT_PEEL_REFERENCE_PRESSURE_PA_HIGH", "Pa", "scalar"),
    ("ORIENT_PEEL_REFERENCE_PRESSURE_PA_LOW", "Pa", "scalar"),
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
    ("TIP_RADIUS_MAX_GROWN_SCAR_MM", "mm", "scalar"),
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

const EXPECTED_SOURCES: [ExpectedSource; 36] = [
    ExpectedSource {
        constant: "CHITUBOX_BOSS_BOTTOM_DIAMETER_MM",
        kind: "owner_decision",
        identity: "craig-chitubox-profile-export-2026-09-30-diameterofbossbottom",
        record: "docs/evidence/chitubox-support-profiles-2026-09-30.cfgx",
        locator: "support[0..3].support_general_para.diameterofbossbottom",
        precision: "exact",
        date: "2026-09-30",
    },
    ExpectedSource {
        constant: "CHITUBOX_BOSS_HEIGHT_MM",
        kind: "owner_decision",
        identity: "craig-chitubox-profile-export-2026-09-30-heightofboss",
        record: "docs/evidence/chitubox-support-profiles-2026-09-30.cfgx",
        locator: "support[0..3].support_general_para.heightofboss",
        precision: "exact",
        date: "2026-09-30",
    },
    ExpectedSource {
        constant: "CHITUBOX_RAFT_HEIGHT_MM",
        kind: "owner_decision",
        identity: "craig-chitubox-profile-export-2026-09-30-raftthickness",
        record: "docs/evidence/chitubox-support-profiles-2026-09-30.cfgx",
        locator: "support[0..3].raft_support.raftthickness",
        precision: "exact",
        date: "2026-09-30",
    },
    ExpectedSource {
        constant: "CHITUBOX_ROOT_BOSS_PROFILE",
        kind: "owner_decision",
        identity: "craig-chitubox-profile-export-2026-09-30-heavy-boss-ratio",
        record: "docs/evidence/chitubox-support-profiles-2026-09-30.cfgx",
        locator: "support[2] (Heavy).middle_support.supportmiddlediameter and support[2].support_general_para.diameterofbossbottom",
        precision: "exact",
        date: "2026-09-30",
    },
    ExpectedSource {
        constant: "CHITUBOX_TIP_CONTACT_PROFILES",
        kind: "owner_decision",
        identity: "craig-chitubox-profile-export-2026-09-30-supporttouchtipdiameter",
        record: "docs/evidence/chitubox-support-profiles-2026-09-30.cfgx",
        locator: "support[0..3].top_support.supporttouchtipdiameter and .supporttipupdiameter",
        precision: "exact",
        date: "2026-09-30",
    },
    ExpectedSource {
        constant: "FINAL_CLEARANCE_GAP_MM",
        kind: "owner_decision",
        identity: "final-clearance-demanded-gap-owner-field-data",
        record: "docs/engineering-journal/03-physics-and-constants.md",
        locator: "§3.5",
        precision: "exact",
        date: "2026-08-22",
    },
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
        constant: "TIP_RADIUS_MAX_GROWN_SCAR_MM",
        kind: "owner_decision",
        identity: "tip-grown-scar-maximum-owner-field-data",
        record: "docs/engineering-journal/03-physics-and-constants.md",
        locator: "§3.2",
        precision: "exact",
        date: "2026-08-20",
    },
    ExpectedSource {
        // Craig's ruling of 2026-09-30 replaced the derivation
        // floor(lower(X_STAR_CONE)/2) = 12 with a ruled design ratio of 18.
        // The plates bound it; they no longer compute it.
        constant: "X_ALLOWED",
        kind: "owner_decision",
        identity: "x-allowed-design-load-ratio-owner-ruling",
        record: "docs/engineering-journal/03-physics-and-constants.md",
        locator: "§3.1",
        precision: "exact",
        date: "2026-09-30",
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
    ExpectedSource {
        constant: "L_SURVIVED_MAX_MM",
        kind: "calibration_plate",
        identity: "COUPON-PLATE-1-overhang-tilt",
        record: "docs/specs/audit-span-criterion.md",
        locator: "§2.5 The bracket (pooling plates 1 and 4)",
        precision: "exact",
        date: "2026-07-29",
    },
    ExpectedSource {
        constant: "L_FIRST_LOSS_MM",
        kind: "calibration_plate",
        identity: "COUPON-PLATE-4-overhang-extended",
        record: "docs/specs/audit-span-criterion.md",
        locator: "§2.5 first loss recorded at (plate 4, G1aU, 1 of 5)",
        precision: "exact",
        date: "2026-07-29",
    },
    ExpectedSource {
        constant: "CONDITION_SHIFT_MAX",
        kind: "calibration_plate",
        identity: "COUPON-PLATE-4-overhang-extended",
        record: "docs/engineering-journal/reference/plate-04-results.md",
        locator: "§3.3 length alone does not determine survival",
        precision: "exact",
        date: "2026-07-29",
    },
    ExpectedSource {
        constant: "ORIENT_PEEL_AREA_EXPONENT_LOW",
        kind: "project_record",
        identity: "orient-peel-area-exponent-porosity-manizani-2026-07-22",
        record: "docs/evidence/literature/LITERATURE-NOTES.md",
        locator: "§1b(i), line 813: Manizani Table 1, four 20 mm lattice discs of differing porosity at a constant outer envelope, FEP 100 µm, speed 50 mm/min -- 2.68x area change -> 1.26x force change",
        precision: "exact",
        date: "2026-07-22",
    },
    ExpectedSource {
        constant: "ORIENT_PEEL_AREA_EXPONENT_HIGH",
        kind: "project_record",
        identity: "orient-peel-area-exponent-envelope-manizani-2026-07-22",
        record: "docs/evidence/literature/LITERATURE-NOTES.md",
        locator: "§1b(ii), line 815: Manizani Fig. 4, three pyramids shrinking WITH their envelope over 33 printed layers, FEP 100 µm -- 59x area reduction -> 6.5x force reduction",
        precision: "exact",
        date: "2026-07-22",
    },
    ExpectedSource {
        constant: "ORIENT_PEEL_REFERENCE_AREA_MM2",
        kind: "project_record",
        identity: "orient-peel-reference-area-manizani-2026-07-22",
        record: "docs/evidence/literature/LITERATURE-NOTES.md",
        locator: "§1a and the peel_pressure_pa review, lines 770-774 and 826: Manizani Table 3, solid 20 mm-diameter disc, A = 314.16 mm^2, rounded to 314 in this review's own anchor statement",
        precision: "exact",
        date: "2026-07-22",
    },
    ExpectedSource {
        constant: "ORIENT_PEEL_REFERENCE_PRESSURE_PA_LOW",
        kind: "project_record",
        identity: "orient-peel-reference-pressure-manizani-2mms-2026-07-22",
        record: "docs/evidence/literature/LITERATURE-NOTES.md",
        locator: "lines 136 and 772: Manizani Table 3, FEP 100 µm, 2 mm/s (120 mm/min), A = 314 mm^2, derived peel pressure ~9.6 kPa, rounded to the review's own anchor of 9 kPa at line 826",
        precision: "exact",
        date: "2026-07-22",
    },
    ExpectedSource {
        constant: "ORIENT_PEEL_REFERENCE_PRESSURE_PA_HIGH",
        kind: "project_record",
        identity: "orient-peel-reference-pressure-pan-cylinder-2026-07-22",
        record: "docs/evidence/literature/lit-green-peel.md",
        locator: "line 28: Pan, He, Xu & Feinerman 2017 (peer-reviewed), solid cylinder at fixed 314 mm^2 cross-section, 1.56 mm/s lift speed, 6.16 N -> 1.96 N/cm^2",
        precision: "exact",
        date: "2026-07-22",
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
    assert_eq!(descriptors.len(), 52);

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
    // X_ALLOWED moved from Derived to Measured (an owner ruling) on 2026-09-30.
    // The owner's ChituBox profile export (2026-09-30) added four Measured
    // (boss foot, boss height, raft height, tip-contact profiles) and one
    // Derived (the pad, from the boss foot): 31/15 became 35/16. The owner's
    // ruling of the same day that the boss is scaled to its root added one
    // more Measured (the root-boss profiles, since his final ruling that day
    // the one Heavy root-boss profile): 36/16.
    assert_eq!((measured, derived), (36, 16));
    assert!(descriptors.iter().all(|descriptor| !matches!(
        descriptor.provenance().class(),
        RegistryProvenanceClass::Placeholder | RegistryProvenanceClass::Pending
    )));
}

#[test]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
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
    assert_eq!(scalar(&descriptors, "X_ALLOWED"), 18.0);

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

#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
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
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn derived_sources_and_relations_are_exact_and_do_not_launder_authority() {
    let constants = component();
    let descriptors = constants.descriptors().unwrap();
    let expected = [
        ("CCO_X_PAIR_LOAD_DISCOUNT", &[][..]),
        (
            "CHITUBOX_PAD_DIAMETER_MM",
            &["CHITUBOX_BOSS_BOTTOM_DIAMETER_MM"] as &[&str],
        ),
        ("N_EFF_CAP", &["LOAD_SHARING"] as &[&str]),
        (
            "TIP_RADIUS_MIN_ALLOWED_MM",
            &["TIP_DIAMETER_MIN_ALLOWED_MM"] as &[&str],
        ),
        // X_ALLOWED left this list on 2026-09-30: it is Craig's ruled design
        // ratio (Measured, owner_decision), bounded by the plates rather than
        // computed from them. See
        // `the_design_load_ratio_is_craigs_ruling_and_sits_inside_the_measured_brackets`.
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
    assert!(constants
        .cco_x_pair_load_discount()
        .note
        .contains("docs/engineering-journal/reference/physics_transient_peel.md"));
}

/// **`X_ALLOWED` is Craig's ruled design load ratio, 18, and the plates bound
/// it.**
///
/// Until 2026-09-30 `X_ALLOWED` was `Derived` as
/// `floor(lower(X_STAR_CONE) / 2) = floor(25.00 / 2) = 12`: the plate-5 snap
/// ratio with a 2x design factor. Craig ruled it to 18 on 2026-09-30 after the
/// over-support attribution showed that every arm tip sits at exactly
/// `load / (π · 12)`, so the 2x was the whole of the margin. 18 is not
/// computed from anything, so it is no longer `Derived`: it is a `Measured`
/// owner decision, the house form for a ruling (`TIP_DIAMETER_MIN_ALLOWED_MM`,
/// `TIP_RADIUS_MAX_GROWN_SCAR_MM`, `FINAL_CLEARANCE_GAP_MM`).
///
/// This test pins four things:
///
/// 1. **the number**, 18;
/// 2. **the class**, `Measured` from an `owner_decision` dated exactly
///    2026-09-30, never `Derived` again (there is no derivation to claim);
/// 3. **the relation to the measured brackets**, which is what keeps the
///    ruling honest: 18 must sit strictly below BOTH the plate-5 cone lower
///    edge (25.00) and the plate-6 cylinder lower edge (19.91, exclusive). A
///    later plate that moved either edge to or below 18 turns this red, and it
///    also stops the registry from constructing (the guard in
///    `M0EvidenceConstants::new`), so a recalibration cannot leave a ruling
///    standing above the measured snap;
/// 4. **the note**, which must name the ruling, both margins and what the
///    plates still say.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn the_design_load_ratio_is_craigs_ruling_and_sits_inside_the_measured_brackets() {
    let constants = component();
    let descriptors = constants.descriptors().unwrap();

    // (1) The number.
    let x_allowed = scalar(&descriptors, "X_ALLOWED");
    assert_eq!(
        x_allowed, 18.0,
        "X_ALLOWED is 18 by Craig's ruling of 2026-09-30 (it was 12 = floor(25.00/2))"
    );
    assert_eq!(*constants.x_allowed().value(), x_allowed);

    // (2) The class.
    let RegistryProvenanceView::Measured { source, err, note } =
        descriptor(&descriptors, "X_ALLOWED").provenance().view()
    else {
        panic!(
            "X_ALLOWED is a Measured owner decision since 2026-09-30 -- a ruling, \
             not a derivation"
        );
    };
    assert!(err.is_none());
    assert_source(
        source,
        &ExpectedSource {
            constant: "X_ALLOWED",
            kind: "owner_decision",
            identity: "x-allowed-design-load-ratio-owner-ruling",
            record: "docs/engineering-journal/03-physics-and-constants.md",
            locator: "§3.1",
            precision: "exact",
            date: "2026-09-30",
        },
    );

    // (3) The relation to the measured brackets.
    let cone = constants.x_star_cone().value();
    let cylinder = constants.x_star_cylinder().value();
    assert!(
        x_allowed < cone.lower(),
        "the design ratio {x_allowed} must sit below the plate-5 cone snap edge {}",
        cone.lower()
    );
    assert!(
        x_allowed < cylinder.lower(),
        "the design ratio {x_allowed} must sit below the plate-6 cylinder bracket's lower \
         edge {} (exclusive)",
        cylinder.lower()
    );
    let cone_margin = cone.lower() / x_allowed;
    let cylinder_margin = cylinder.lower() / x_allowed;
    assert!((cone_margin - 1.3889).abs() < 5.0e-5, "{cone_margin}");
    assert!((cylinder_margin - 1.1061).abs() < 5.0e-5, "{cylinder_margin}");

    // (4) The note.
    for phrase in [
        "Craig ruling 2026-09-30",
        "was 12 = floor(25.00/2)",
        "25.00/18 = 1.389×",
        "19.91/18 = 1.106×",
        "X_STAR_CONE",
        "X_STAR_CYLINDER",
        "not a derivation",
        "oversupport-attribution.md",
    ] {
        assert!(
            note.contains(phrase),
            "the X_ALLOWED note must say {phrase:?}; it is the record of what the ruling \
             rests on and how much margin it leaves"
        );
    }
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
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
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

/// **Craig's 2026-08-20 scar maximum, pinned in BOTH units and honestly
/// labelled.**
///
/// `TIP_RADIUS_MAX_GROWN_SCAR_MM` is the largest tip a grow-for-ATTACHMENT
/// rescue may reach: 0.400 mm RADIUS = 0.800 mm DIAMETER. Its whole authority
/// is field data — the heaviest support Craig has ever needed is a 0.800 mm
/// DIAMETER sphere — so this test pins the three things that could rot
/// silently:
///
/// 1. **the number, in both units**, because the registry speaks RADIUS while
///    Craig and the ChituBox UI speak DIAMETER, and a value transcribed between
///    them without halving lands 2x wrong;
/// 2. **the class**, `Measured` from an `owner_decision` dated exactly
///    2026-08-20 — an honestly labelled EXTERNAL REQUIREMENT, not a `Derived`
///    physics result and not a `Placeholder` guess. Re-labelling it `Derived`
///    would be claiming a derivation nobody has;
/// 3. **its RELATION to the load law's own clamp**, which is what makes it a
///    growth bound rather than a general tip cap: it must sit strictly BELOW
///    `TIP_RADIUS_MAX_LEGACY_MM`, so `min(bound, legacy)` is the bound and the
///    truncation has something to do. If a later ruling raises it to or above
///    the legacy maximum, this goes red — correctly, because at that point the
///    growth ladder has no separate ceiling and the disclosure would start
///    naming a constant that never binds.
///
/// **What it deliberately does not assert**: any physics. There is none here to
/// assert. The note says so and this test does not pretend otherwise.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn the_grown_scar_maximum_is_craigs_field_ceiling_labelled_as_field_data() {
    let descriptors = component().descriptors().unwrap();

    let scar_max_radius_mm = scalar(&descriptors, "TIP_RADIUS_MAX_GROWN_SCAR_MM");
    assert_eq!(
        scar_max_radius_mm, 0.400,
        "the grown-scar maximum is 0.400 mm RADIUS, Craig's ruling of 2026-08-20"
    );
    assert_eq!(
        scar_max_radius_mm * 2.0,
        0.800,
        "the same maximum is 0.800 mm DIAMETER — the heaviest support Craig has \
         ever needed, and the unit he and the slicer UI speak in"
    );

    // (2) The class. Field data recorded as an owner decision, on an exact
    // date, with no error bar claimed for a number that has none.
    let RegistryProvenanceView::Measured { source, err, note } =
        descriptor(&descriptors, "TIP_RADIUS_MAX_GROWN_SCAR_MM")
            .provenance()
            .view()
    else {
        panic!(
            "the grown-scar maximum is a Measured owner decision carrying field \
             data — never Derived (there is no derivation) and never a bare \
             Placeholder (there is real evidence)"
        );
    };
    assert!(err.is_none());
    assert_source(
        source,
        &ExpectedSource {
            constant: "TIP_RADIUS_MAX_GROWN_SCAR_MM",
            kind: "owner_decision",
            identity: "tip-grown-scar-maximum-owner-field-data",
            record: "docs/engineering-journal/03-physics-and-constants.md",
            locator: "§3.2",
            precision: "exact",
            date: "2026-08-20",
        },
    );

    // The note must say out loud what kind of number this is, must state both
    // units, and must record the distinction that keeps the load law free.
    for phrase in [
        "EXTERNAL REQUIREMENT",
        "NOT physics",
        "0.400 mm RADIUS",
        "0.800 mm DIAMETER",
        "GROWTH-FOR-ATTACHMENT only",
        "WHAT IT DOES NOT BOUND",
        "TIP_RADIUS_MAX_LEGACY_MM",
        "ABANDONED LOAD",
    ] {
        assert!(
            note.contains(phrase),
            "the grown-scar maximum's ledger note must say {phrase:?} — it is \
             the difference between an honest external requirement and a number \
             that will be mistaken for physics later"
        );
    }

    // (3) The relation that makes it a GROWTH bound.
    let legacy_max_radius_mm = plinth_core::m0_constants()
        .unwrap()
        .placeholders()
        .tip_radius_max_legacy_mm()
        .diagnostic()
        .get();
    assert!(
        scar_max_radius_mm < legacy_max_radius_mm,
        "the grown-scar maximum ({scar_max_radius_mm} mm radius / {} mm \
         diameter) must sit strictly below the load law's own clamp maximum \
         ({legacy_max_radius_mm} mm radius / {} mm diameter), or the growth \
         ladder has no separate ceiling to truncate at",
        scar_max_radius_mm * 2.0,
        legacy_max_radius_mm * 2.0
    );
}

/// The two-tier demanded gaps (Craig's ruling of 2026-08-22).
///
/// The engine grades clearance at two stages with two different demands and
/// ONE predicate. This test pins the numbers, their ORDER, and the labelling
/// that keeps them from being mistaken for each other — because the whole
/// point of the split is that only one of them refuses.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn the_two_tier_demanded_gaps_are_a_pruning_0_6_and_a_refusing_0_3() {
    let descriptors = component().descriptors().unwrap();

    // (1) The FINAL demand: the only refusing clearance number in the engine.
    let final_gap_mm = scalar(&descriptors, "FINAL_CLEARANCE_GAP_MM");
    assert_eq!(
        final_gap_mm, 0.300,
        "the final demanded gap is 0.300 mm of TRUE surface-to-surface \
         clearance, Craig's ruling of 2026-08-22"
    );

    // (2) The PLACEMENT demand: a search margin, registered separately.
    let placement_gap_mm = plinth_core::m0_constants()
        .unwrap()
        .placeholders()
        .standoff_mm()
        .diagnostic()
        .get();
    assert_eq!(placement_gap_mm, 0.600);
    assert!(
        final_gap_mm < placement_gap_mm,
        "the refusing demand ({final_gap_mm} mm) must sit strictly BELOW the \
         pruning demand ({placement_gap_mm} mm). Inverted, the search would \
         admit candidates the final pass refuses and the two-tier split would \
         cost emissions instead of saving them"
    );

    // (3) The class. Field data recorded as an owner decision, exact date, no
    // error bar claimed for a number that has none.
    let RegistryProvenanceView::Measured { source, err, note } =
        descriptor(&descriptors, "FINAL_CLEARANCE_GAP_MM")
            .provenance()
            .view()
    else {
        panic!(
            "the final demanded gap is a Measured owner decision carrying field \
             data — never Derived (there is no derivation) and never a bare \
             Placeholder (there is real evidence)"
        );
    };
    assert!(err.is_none());
    assert_source(
        source,
        &ExpectedSource {
            constant: "FINAL_CLEARANCE_GAP_MM",
            kind: "owner_decision",
            identity: "final-clearance-demanded-gap-owner-field-data",
            record: "docs/engineering-journal/03-physics-and-constants.md",
            locator: "§3.5",
            precision: "exact",
            date: "2026-08-22",
        },
    );

    // (4) The note must say what kind of number it is, what quantity it grades,
    // which stage it decides, and which stage it does NOT.
    for phrase in [
        "EXTERNAL REQUIREMENT",
        "NOT physics",
        "0.3mm gap features are preserved fine in prints",
        "TRUE surface-to-surface gap",
        "WHAT IT DECIDES",
        "WHAT IT IS NOT",
        "STANDOFF_MM",
    ] {
        assert!(
            note.contains(phrase),
            "the final demanded gap's note must say {phrase:?} — it is the \
             difference between an honest external requirement and a number \
             that will be mistaken for physics later"
        );
    }

    // (5) The PLACEMENT demand's own label. Before 2026-08-22 this entry said
    // only "legacy clearance policy", which is what let a search margin be
    // mistaken for a refusing gate. The relabel is part of the ruling, so it is
    // pinned here rather than left to prose review.
    let registry = plinth_core::m0_constants().unwrap().registry();
    let standoff = registry
        .get(&plinth_core::ConstantId::from("STANDOFF_MM"))
        .expect("STANDOFF_MM must be published");
    let RegistryProvenanceView::Placeholder {
        note: standoff_note,
    } = standoff.provenance().view()
    else {
        panic!("STANDOFF_MM is a Placeholder: no measurement derives 0.600 mm");
    };
    for phrase in [
        "PLACEMENT-FAMILY",
        "SEARCH MARGIN",
        "prunes",
        "refuses nothing final",
        "FINAL_CLEARANCE_GAP_MM",
        "CENTERLINE",
        "ONE PREDICATE, TWO DEMANDS",
    ] {
        assert!(
            standoff_note
                .to_lowercase()
                .contains(&phrase.to_lowercase()),
            "the placement demand's note must say {phrase:?} — a bare \"legacy\" \
             label is exactly how a pruning margin gets read as a refusing gate"
        );
    }
}

/// The lamina reach ratios and the span allowance must be COMPUTED from their
/// registered parents, never transcribed.
///
/// This test deliberately re-derives each value from the parent descriptors the
/// registry itself publishes and compares bit-for-bit, rather than pinning a
/// decimal literal. A literal pin passes just as well when someone hard-codes
/// 42.6006450 and lets a parent move underneath it, which is the exact failure
/// `docs/specs/audit-span-criterion.md`:225-229 forbids for this family.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "bit-for-bit re-derivation from the registry's own parent values is the point of this \
          test; a tolerance here would admit the transcribed literal it exists to refuse"
)]
fn the_reach_ratios_are_recomputed_from_their_registered_parents_bit_for_bit() {
    let descriptors = component().descriptors().unwrap();

    let l_survived = scalar(&descriptors, "L_SURVIVED_MAX_MM");
    let l_first_loss = scalar(&descriptors, "L_FIRST_LOSS_MM");
    let condition_shift = scalar(&descriptors, "CONDITION_SHIFT_MAX");
    let layer_height = scalar(&descriptors, "LAYER_HEIGHT_DEFAULT_MM");
    let pitch_x_um = scalar(&descriptors, "PIXEL_PITCH_X_UM");
    let pitch_y_um = scalar(&descriptors, "PIXEL_PITCH_Y_UM");

    let span_crit = l_survived / condition_shift;
    assert_eq!(scalar(&descriptors, "L_SPAN_CRIT_MM"), span_crit);

    let pixel_diagonal = (pitch_x_um / 1000.0).hypot(pitch_y_um / 1000.0);
    let span_allowed = span_crit - 2.0 * pixel_diagonal;
    assert_eq!(scalar(&descriptors, "L_SPAN_ALLOWED_MM"), span_allowed);

    assert_eq!(
        scalar(&descriptors, "LAMINA_REACH_RATIO"),
        l_survived / layer_height
    );
    assert_eq!(
        scalar(&descriptors, "LAMINA_REACH_RATIO_FIRST_LOSS"),
        l_first_loss / layer_height
    );
    assert_eq!(
        scalar(&descriptors, "LAMINA_REACH_RATIO_AUDIT_GATE"),
        span_allowed / layer_height
    );

    // The magnitudes the design of record and every run report quote. These are
    // a READING CHECK on the derivation above, not the derivation: they are
    // asserted to four significant figures precisely so they cannot stand in
    // for the bit-for-bit identities.
    assert!((scalar(&descriptors, "LAMINA_REACH_RATIO") - 42.600_645).abs() < 5e-6);
    assert!((scalar(&descriptors, "LAMINA_REACH_RATIO_FIRST_LOSS") - 51.840_72).abs() < 5e-5);
    assert!((scalar(&descriptors, "LAMINA_REACH_RATIO_AUDIT_GATE") - 26.899_525).abs() < 5e-6);
    assert!((scalar(&descriptors, "L_SPAN_ALLOWED_MM") - 1.344_976_246).abs() < 5e-9);

    // The placement value is strictly inside the measured bracket and strictly
    // above the gate. If this ever inverts, placement has started reading the
    // gate's number (sabotage S8's lesion) or the parents have moved.
    let placement = scalar(&descriptors, "LAMINA_REACH_RATIO");
    assert!(placement > scalar(&descriptors, "LAMINA_REACH_RATIO_AUDIT_GATE"));
    assert!(placement < scalar(&descriptors, "LAMINA_REACH_RATIO_FIRST_LOSS"));
}

/// **The support geometry is the owner's own ChituBox practice, read off his
/// exported profiles** (ruling of 2026-09-30).
///
/// The export is committed verbatim at
/// `docs/evidence/chitubox-support-profiles-2026-09-30.cfgx` (JSON). This test
/// PARSES that file and holds every registered number to it, profile by
/// profile, so the registry cannot drift from the owner's own record and a
/// re-export with different numbers turns this red until the registry is
/// re-read. It reads the export's fields by name and compares; it does not
/// compute anything the registry computes.
///
/// * `CHITUBOX_TIP_CONTACT_PROFILES` holds one row per profile, by name, with
///   that profile's `top_support.supporttipupdiameter` and
///   `.supporttouchtipdiameter`; all four profiles are read.
/// * `CHITUBOX_ROOT_BOSS_PROFILE` is the export's `support[2]`, named
///   "Heavy", with that profile's `middle_support.supportmiddlediameter` (the
///   shaft) and `support_general_para.diameterofbossbottom` (the boss foot)
///   (Craig ruling 2026-09-30: Heavy profile ratio, under every root).
/// * `CHITUBOX_BOSS_BOTTOM_DIAMETER_MM`, `CHITUBOX_BOSS_HEIGHT_MM` and
///   `CHITUBOX_RAFT_HEIGHT_MM` equal the `diameterofbossbottom`,
///   `heightofboss` and `raftthickness` of EVERY profile (they agree; a
///   profile that disagreed would have to be ruled on, not averaged).
/// * `CHITUBOX_PAD_DIAMETER_MM` is Derived from the boss foot and equals it.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "the registry transcribes the export's decimal literals exactly; equality is the claim"
)]
fn chitubox_geometry_is_read_off_the_committed_profile_export() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("docs/evidence/chitubox-support-profiles-2026-09-30.cfgx");
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "the committed export {} is unreadable: {error}",
            path.display()
        )
    });
    let export: serde_json::Value =
        serde_json::from_str(&raw).expect("the committed ChituBox export is JSON");
    let profiles = export["support"]
        .as_array()
        .expect("a support profile array");
    assert_eq!(profiles.len(), 4, "the owner exported four profiles");
    let field = |profile: &serde_json::Value, section: &str, name: &str| -> f64 {
        profile[section][name]["currentvalue"]
            .as_f64()
            .unwrap_or_else(|| panic!("{} has no {section}.{name}", profile["name"]))
    };

    let constants = component();
    let table = constants.chitubox_tip_contact_profiles().value();
    assert_eq!(table.profiles().len(), profiles.len());
    // The root boss is the export's profile 2, and that profile is Heavy.
    let boss = constants.chitubox_root_boss_profile().value();
    let heavy = &profiles[2];
    assert_eq!(heavy["name"].as_str(), Some("Heavy"), "support[2] is Heavy");
    assert_eq!(boss.name(), "Heavy");
    assert_eq!(
        boss.shaft_diameter_mm(),
        field(heavy, "middle_support", "supportmiddlediameter"),
        "Heavy: supportmiddlediameter"
    );
    assert_eq!(
        boss.boss_bottom_diameter_mm(),
        field(heavy, "support_general_para", "diameterofbossbottom"),
        "Heavy: diameterofbossbottom (root boss)"
    );
    for profile in profiles {
        let name = profile["name"].as_str().expect("a profile name");
        let row = table
            .profiles()
            .iter()
            .find(|row| row.name() == name)
            .unwrap_or_else(|| panic!("profile {name} is missing from the registered table"));
        assert_eq!(
            row.tip_up_diameter_mm(),
            field(profile, "top_support", "supporttipupdiameter"),
            "{name}: supporttipupdiameter"
        );
        assert_eq!(
            row.touch_diameter_mm(),
            field(profile, "top_support", "supporttouchtipdiameter"),
            "{name}: supporttouchtipdiameter"
        );
        assert_eq!(
            *constants.chitubox_boss_bottom_diameter_mm().value(),
            field(profile, "support_general_para", "diameterofbossbottom"),
            "{name}: diameterofbossbottom"
        );
        assert_eq!(
            *constants.chitubox_boss_height_mm().value(),
            field(profile, "support_general_para", "heightofboss"),
            "{name}: heightofboss"
        );
        assert_eq!(
            *constants.chitubox_raft_height_mm().value(),
            field(profile, "raft_support", "raftthickness"),
            "{name}: raftthickness"
        );
    }
    assert_eq!(
        *constants.chitubox_pad_diameter_mm().value(),
        *constants.chitubox_boss_bottom_diameter_mm().value()
    );
    // The owner's own reading of his profiles, 2026-09-30: the ball-to-tip
    // ratios he prints with are 1.25 to 1.5. A table whose rows were swapped
    // between the two diameters would read below one here.
    for row in table.profiles() {
        let ratio = row.ball_factor();
        assert!(
            (1.25 - 1e-12..=1.5 + 1e-12).contains(&ratio),
            "{}: ball-to-tip ratio {ratio} is outside the owner's 1.25-1.5",
            row.name()
        );
    }
    // The boss half (Craig ruling 2026-09-30: Heavy profile ratio, about 1.47
    // times the support's own root). Typed here from the export's Heavy row,
    // 2.2 / 1.5; a row whose two diameters were swapped would read below one.
    assert!(
        (boss.boss_factor() - 2.2 / 1.5).abs() <= 1e-12,
        "the registered boss-to-shaft ratio is {}, not Heavy's 2.2 / 1.5",
        boss.boss_factor()
    );
}
