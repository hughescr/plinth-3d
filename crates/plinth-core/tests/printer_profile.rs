use plinth_core::{
    BuildVolumeError, BuildVolumeMm, EvidenceDate, EvidenceId, EvidenceSource, Measured,
    MeasuredPayload, Placeholder, PrinterProfile, PrinterProfileError, PrinterProfileId,
    ProvenanceError, YDirection,
};
use serde_json::{json, Value};

fn source(locator: &str) -> EvidenceSource {
    EvidenceSource::project_record(
        EvidenceId::new("saturn-profile-record").unwrap(),
        "records/printer-profile.md",
        locator,
        EvidenceDate::Unknown,
    )
    .unwrap()
}

fn measured<T: MeasuredPayload>(value: T, locator: &str) -> Measured<T> {
    Measured::new(value, source(locator), None, "profile evidence").unwrap()
}

#[allow(
    clippy::too_many_arguments,
    reason = "tests vary exact passive profile fields"
)]
fn profile_with(
    id: &str,
    pixel_pitch_x_um: f64,
    pixel_pitch_y_um: f64,
    default_layer_height_mm: f64,
    lift_default_mm: f64,
    leading_release_edge: YDirection,
    release_front_sweep_direction: YDirection,
    release_film_thickness_mm: f64,
) -> Result<PrinterProfile, PrinterProfileError> {
    PrinterProfile::new(
        id,
        measured(
            BuildVolumeMm::new(211.68, 118.37, 220.0).unwrap(),
            "#build-volume",
        ),
        measured(pixel_pitch_x_um, "#pixel-x"),
        measured(pixel_pitch_y_um, "#pixel-y"),
        measured(default_layer_height_mm, "#layer-height"),
        Placeholder::new(lift_default_mm, "local lift default"),
        measured(leading_release_edge, "#leading-edge"),
        measured(release_front_sweep_direction, "#front-sweep"),
        measured(release_film_thickness_mm, "#release-film"),
    )
}

fn valid_profile() -> PrinterProfile {
    profile_with(
        "saturn4-ultra",
        14.0,
        19.0,
        0.05,
        5.0,
        YDirection::NegativeY,
        YDirection::PositiveY,
        0.127,
    )
    .unwrap()
}

#[test]
fn build_volume_is_strict_positive_xyz_and_exact_on_the_wire() {
    let volume = BuildVolumeMm::new(211.68, 118.37, 220.0).unwrap();
    assert_eq!(
        (volume.x(), volume.y(), volume.z()),
        (211.68, 118.37, 220.0)
    );
    assert_eq!(
        serde_json::to_value(volume).unwrap(),
        json!({"x": 211.68, "y": 118.37, "z": 220.0})
    );
    assert_eq!(
        serde_json::from_value::<BuildVolumeMm>(json!({
            "x": 211.68,
            "y": 118.37,
            "z": 220.0
        }))
        .unwrap(),
        volume
    );

    for (axis, values) in [
        ("x", [0.0, -0.0, -1.0, f64::NAN, f64::INFINITY]),
        ("y", [0.0, -0.0, -1.0, f64::NAN, f64::NEG_INFINITY]),
        ("z", [0.0, -0.0, -1.0, f64::NAN, f64::INFINITY]),
    ] {
        for invalid in values {
            let result = match axis {
                "x" => BuildVolumeMm::new(invalid, 1.0, 1.0),
                "y" => BuildVolumeMm::new(1.0, invalid, 1.0),
                "z" => BuildVolumeMm::new(1.0, 1.0, invalid),
                _ => unreachable!(),
            };
            if invalid.is_finite() {
                assert_eq!(result, Err(BuildVolumeError::NonPositiveAxis { axis }));
            } else {
                assert_eq!(result, Err(BuildVolumeError::NonFiniteAxis { axis }));
            }
        }
    }

    for malformed in [
        r#"{"x":1.0,"y":2.0}"#,
        r#"{"x":1.0,"y":2.0,"z":3.0,"extra":4.0}"#,
        r#"{"x":1.0,"x":2.0,"y":2.0,"z":3.0}"#,
        r#"{"x":-0.0,"y":2.0,"z":3.0}"#,
        r#"{"x":1e9999,"y":2.0,"z":3.0}"#,
    ] {
        assert!(
            serde_json::from_str::<BuildVolumeMm>(malformed).is_err(),
            "malformed build volume unexpectedly accepted: {malformed}"
        );
    }
}

#[test]
fn y_direction_wire_is_narrow_and_exact() {
    assert_eq!(YDirection::NegativeY.as_str(), "negative_y");
    assert_eq!(YDirection::PositiveY.as_str(), "positive_y");
    assert_eq!(
        serde_json::to_string(&YDirection::NegativeY).unwrap(),
        r#""negative_y""#
    );
    assert_eq!(
        serde_json::from_str::<YDirection>(r#""positive_y""#).unwrap(),
        YDirection::PositiveY
    );
    for malformed in [r#""negative-y""#, r#""NegativeY""#, r#""peel""#, "{}"] {
        assert!(serde_json::from_str::<YDirection>(malformed).is_err());
    }
}

#[test]
fn categorical_and_structured_measurements_forbid_error_payloads() {
    assert_eq!(
        Measured::new(true, source("#bool"), Some(false), "invalid bool error"),
        Err(ProvenanceError::CategoricalErrorBar)
    );
    let volume = BuildVolumeMm::new(1.0, 2.0, 3.0).unwrap();
    assert_eq!(
        Measured::new(
            volume,
            source("#volume"),
            Some(volume),
            "invalid volume error"
        ),
        Err(ProvenanceError::CategoricalErrorBar)
    );
    assert_eq!(
        Measured::new(
            YDirection::NegativeY,
            source("#direction"),
            Some(YDirection::PositiveY),
            "invalid direction error"
        ),
        Err(ProvenanceError::CategoricalErrorBar)
    );

    let mut bool_wire = serde_json::to_value(measured(true, "#bool-wire")).unwrap();
    bool_wire["err"] = json!(false);
    assert!(serde_json::from_value::<Measured<bool>>(bool_wire).is_err());

    let mut volume_wire = serde_json::to_value(measured(volume, "#volume-wire")).unwrap();
    volume_wire["err"] = json!({"x": 1.0, "y": 2.0, "z": 3.0});
    assert!(serde_json::from_value::<Measured<BuildVolumeMm>>(volume_wire).is_err());

    let mut direction_wire =
        serde_json::to_value(measured(YDirection::NegativeY, "#direction-wire")).unwrap();
    direction_wire["err"] = json!("positive_y");
    assert!(serde_json::from_value::<Measured<YDirection>>(direction_wire).is_err());
}

#[test]
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn printer_profile_preserves_wrappers_axes_and_distinct_release_fields() {
    let profile = valid_profile();
    assert_eq!(profile.id().as_str(), "saturn4-ultra");
    assert_eq!(
        (
            profile.build_volume_mm().value().x(),
            profile.build_volume_mm().value().y(),
            profile.build_volume_mm().value().z(),
        ),
        (211.68, 118.37, 220.0)
    );
    assert_eq!(*profile.pixel_pitch_x_um().value(), 14.0);
    assert_eq!(*profile.pixel_pitch_y_um().value(), 19.0);
    assert_eq!(*profile.default_layer_height_mm().value(), 0.05);
    assert_eq!(*profile.lift_default_mm().diagnostic(), 5.0);
    assert_eq!(
        *profile.leading_release_edge().value(),
        YDirection::NegativeY
    );
    assert_eq!(
        *profile.release_front_sweep_direction().value(),
        YDirection::PositiveY
    );
    assert_eq!(*profile.release_film_thickness_mm().value(), 0.127);
    assert_ne!(
        profile.leading_release_edge().source(),
        profile.release_front_sweep_direction().source()
    );

    let plan = profile.layer_plan(3).unwrap();
    assert_eq!(plan.heights_mm(), &[0.05, 0.05, 0.05]);
    assert_eq!(plan.count(), 3);
}

#[test]
fn printer_profile_serde_is_exact_fallible_and_keeps_unknown_dates_typed() {
    let profile = valid_profile();
    let wire = serde_json::to_value(&profile).unwrap();
    let expected = json!({
        "id": "saturn4-ultra",
        "build_volume_mm": serde_json::to_value(profile.build_volume_mm()).unwrap(),
        "pixel_pitch_x_um": serde_json::to_value(profile.pixel_pitch_x_um()).unwrap(),
        "pixel_pitch_y_um": serde_json::to_value(profile.pixel_pitch_y_um()).unwrap(),
        "default_layer_height_mm": serde_json::to_value(profile.default_layer_height_mm()).unwrap(),
        "lift_default_mm": {"value": 5.0, "note": "local lift default"},
        "leading_release_edge": serde_json::to_value(profile.leading_release_edge()).unwrap(),
        "release_front_sweep_direction": serde_json::to_value(profile.release_front_sweep_direction()).unwrap(),
        "release_film_thickness_mm": serde_json::to_value(profile.release_film_thickness_mm()).unwrap(),
    });
    assert_eq!(wire, expected);
    assert_eq!(
        serde_json::from_value::<PrinterProfile>(wire.clone()).unwrap(),
        profile
    );
    assert_eq!(
        wire["build_volume_mm"]["source"]["date"],
        json!({"precision": "unknown"})
    );

    let mut missing = wire.clone();
    missing.as_object_mut().unwrap().remove("pixel_pitch_y_um");
    assert!(serde_json::from_value::<PrinterProfile>(missing).is_err());

    let mut unknown = wire.clone();
    unknown["extra"] = json!(true);
    assert!(serde_json::from_value::<PrinterProfile>(unknown).is_err());

    let mut legacy_peel = wire.clone();
    legacy_peel["peel_direction"] = json!("negative_y");
    assert!(serde_json::from_value::<PrinterProfile>(legacy_peel).is_err());

    let mut nested_unknown = wire.clone();
    nested_unknown["lift_default_mm"]["extra"] = json!(true);
    assert!(serde_json::from_value::<PrinterProfile>(nested_unknown).is_err());

    let mut invalid_slug = wire.clone();
    invalid_slug["id"] = json!("Saturn4-ultra");
    assert!(serde_json::from_value::<PrinterProfile>(invalid_slug).is_err());

    let mut raw_url = wire;
    raw_url["leading_release_edge"]["source"]["locator"] = json!("https://example.test/raw");
    assert!(serde_json::from_value::<PrinterProfile>(raw_url).is_err());

    let text = serde_json::to_string(&profile).unwrap();
    let duplicate = text.replacen(
        r#""id":"saturn4-ultra""#,
        r#""id":"saturn4-ultra","id":"saturn4-ultra""#,
        1,
    );
    assert_ne!(duplicate, text);
    assert!(serde_json::from_str::<PrinterProfile>(&duplicate).is_err());
}

#[test]
fn printer_profile_rejects_invalid_slugs_scalars_and_release_pairings() {
    for invalid in [
        "",
        "Saturn4-ultra",
        "saturn4_ultra",
        "saturn4/ultra",
        "-saturn4",
        "saturn4-",
        "saturn4--ultra",
    ] {
        assert!(PrinterProfileId::new(invalid).is_err());
        assert!(serde_json::from_value::<PrinterProfileId>(json!(invalid)).is_err());
    }
    assert_eq!(
        PrinterProfileId::new("saturn4-ultra").unwrap().as_str(),
        "saturn4-ultra"
    );

    for (field, result) in [
        (
            "pixel_pitch_x_um",
            profile_with(
                "valid",
                0.0,
                19.0,
                0.05,
                5.0,
                YDirection::NegativeY,
                YDirection::PositiveY,
                0.127,
            ),
        ),
        (
            "pixel_pitch_y_um",
            profile_with(
                "valid",
                14.0,
                -0.0,
                0.05,
                5.0,
                YDirection::NegativeY,
                YDirection::PositiveY,
                0.127,
            ),
        ),
        (
            "default_layer_height_mm",
            profile_with(
                "valid",
                14.0,
                19.0,
                -1.0,
                5.0,
                YDirection::NegativeY,
                YDirection::PositiveY,
                0.127,
            ),
        ),
        (
            "lift_default_mm",
            profile_with(
                "valid",
                14.0,
                19.0,
                0.05,
                -0.0,
                YDirection::NegativeY,
                YDirection::PositiveY,
                0.127,
            ),
        ),
        (
            "release_film_thickness_mm",
            profile_with(
                "valid",
                14.0,
                19.0,
                0.05,
                5.0,
                YDirection::NegativeY,
                YDirection::PositiveY,
                -1.0,
            ),
        ),
    ] {
        assert_eq!(
            result,
            Err(PrinterProfileError::NonPositiveScalar { field })
        );
    }

    assert_eq!(
        profile_with(
            "valid",
            14.0,
            19.0,
            0.05,
            5.0,
            YDirection::NegativeY,
            YDirection::NegativeY,
            0.127,
        ),
        Err(PrinterProfileError::NonOpposingReleaseDirections)
    );

    let profile = valid_profile();
    let text = serde_json::to_string(&profile).unwrap();
    let nonfinite = text.replacen(
        r#""pixel_pitch_x_um":{"value":14.0"#,
        r#""pixel_pitch_x_um":{"value":1e9999"#,
        1,
    );
    assert_ne!(nonfinite, text);
    assert!(serde_json::from_str::<PrinterProfile>(&nonfinite).is_err());

    let mut same_direction = serde_json::to_value(profile).unwrap();
    same_direction["release_front_sweep_direction"]["value"] = json!("negative_y");
    assert!(serde_json::from_value::<PrinterProfile>(same_direction).is_err());
}

#[test]
fn printer_profile_wire_rejects_non_null_direction_and_volume_errors() {
    let profile = valid_profile();
    let mut direction_error = serde_json::to_value(&profile).unwrap();
    direction_error["leading_release_edge"]["err"] = json!("positive_y");
    assert!(serde_json::from_value::<PrinterProfile>(direction_error).is_err());

    let mut volume_error = serde_json::to_value(profile).unwrap();
    volume_error["build_volume_mm"]["err"] = json!({"x": 211.68, "y": 118.37, "z": 220.0});
    assert!(serde_json::from_value::<PrinterProfile>(volume_error).is_err());
}

#[test]
fn malformed_profile_scalar_json_is_rejected_without_sentinels() {
    let profile = valid_profile();
    let mut wire: Value = serde_json::to_value(profile).unwrap();
    wire["default_layer_height_mm"]["value"] = json!(-0.0);
    assert!(serde_json::from_value::<PrinterProfile>(wire).is_err());
}
