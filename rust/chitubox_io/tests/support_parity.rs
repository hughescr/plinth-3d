//! Support parity (§7.4): parse the two real emitter outputs, census + type
//! histogram + field decode; then replace/append round-trips and the
//! asymmetric multi-model case.

mod common;
use chitubox_io::{Project, SegmentType, SupportBatch, SupportMode, SupportRecord, TriangleSoup};
use common::*;
use std::collections::BTreeMap;

fn open_required(name: &str) -> Project {
    let bytes = read_required_fixture(name);
    Project::from_bytes(bytes).unwrap_or_else(|error| panic!("parse {name}: {error}"))
}

fn open_optional_treant_candidate() -> Option<Project> {
    read_optional_treant_candidate().map(|bytes| {
        Project::from_bytes(bytes)
            .unwrap_or_else(|error| panic!("parse {OPTIONAL_TREANT_CANDIDATE}: {error}"))
    })
}

fn assert_real_emitter_output(name: &str, expect: usize, proj: Project) {
    let counts = proj.support_counts();
    let total: usize = counts.iter().sum();
    assert_eq!(total, expect, "{name}: record count");

    // Decode every record; build a type histogram; fields must be finite.
    let mut hist: BTreeMap<u32, usize> = BTreeMap::new();
    for model_index in 0..proj.model_count() {
        for record in proj.supports(model_index).unwrap() {
            *hist.entry(record.segment_type.0).or_default() += 1;
            assert!(record.start.iter().all(|value| value.is_finite()));
            assert!(record.end.iter().all(|value| value.is_finite()));
            assert!(record.r_start.is_finite() && record.r_end.is_finite());
            assert!(record.mesh.xyz.len().is_multiple_of(9));
        }
    }
    let decoded: usize = hist.values().sum();
    assert_eq!(decoded, expect, "{name}: decoded all records");
    assert!(!hist.is_empty(), "{name}: empty histogram");
    eprintln!("{name}: type histogram {hist:?}");
}

/// A minimal well-formed support record with a 1-triangle mesh.
fn tiny_record(t: SegmentType, x: f32) -> SupportRecord {
    SupportRecord::new(
        t,
        [x, 0.0, -8.0],
        [x, 0.0, 3.0],
        0.4,
        0.75,
        TriangleSoup::new(vec![x, 0.0, 0.0, x + 1.0, 0.0, 0.0, x, 1.0, 0.0]),
    )
}

#[test]
fn parse_real_emitter_outputs() {
    assert_real_emitter_output(
        "morel_mudskipper_supports.chitubox",
        243,
        open_required("morel_mudskipper_supports.chitubox"),
    );

    // The oversized candidate is the sole sanctioned optional fixture.
    if let Some(proj) = open_optional_treant_candidate() {
        assert_real_emitter_output(OPTIONAL_TREANT_CANDIDATE, 1360, proj);
    }
}

#[test]
fn replace_with_same_records_roundtrips() {
    // supports() -> attach_supports(Replace, same records) -> re-parse equal.
    let mut proj = open_required("sphere-cylinder-supported.chitubox");
    let orig0 = proj.supports(0).unwrap();
    let orig1 = proj.supports(1).unwrap();
    proj.attach_supports(&[
        SupportBatch {
            model_index: 0,
            records: orig0.clone(),
            mode: SupportMode::Replace,
            lift_mm: None,
        },
        SupportBatch {
            model_index: 1,
            records: orig1.clone(),
            mode: SupportMode::Replace,
            lift_mm: None,
        },
    ])
    .unwrap();
    let bytes = proj.to_bytes().expect("replace verifies");
    let re = Project::from_bytes(bytes).unwrap();
    assert_eq!(re.support_counts(), vec![orig0.len(), orig1.len()]);
    // decoded records equal (geometry + fields), modulo dead-blob layout
    assert_eq!(re.supports(0).unwrap(), orig0);
    assert_eq!(re.supports(1).unwrap(), orig1);
}

#[test]
fn asymmetric_multi_model_supports() {
    // 3 records on model 0, 17 on model 1 — the interleaved-offset stress case.
    let mut proj = open_required("sphere-cylinder.chitubox");
    let recs0: Vec<_> = (0..3)
        .map(|i| tiny_record(SegmentType::SHAFT, i as f32))
        .collect();
    let recs1: Vec<_> = (0..17)
        .map(|i| tiny_record(SegmentType::TIP, i as f32))
        .collect();
    proj.attach_supports(&[
        SupportBatch {
            model_index: 0,
            records: recs0.clone(),
            mode: SupportMode::Replace,
            lift_mm: Some(3.0),
        },
        SupportBatch {
            model_index: 1,
            records: recs1.clone(),
            mode: SupportMode::Replace,
            lift_mm: Some(3.0),
        },
    ])
    .unwrap();
    let bytes = proj.to_bytes().expect("asymmetric verifies");
    let re = Project::from_bytes(bytes).unwrap();
    assert_eq!(re.support_counts(), vec![3, 17]);
    // both sections decode and preserve geometry/type
    assert_eq!(re.supports(0).unwrap(), recs0);
    assert_eq!(re.supports(1).unwrap(), recs1);
    // lift applied
    assert_eq!(re.placements()[0].z_lift, 3.0);
    assert_eq!(re.placements()[1].z_lift, 3.0);
}

#[test]
fn append_mode_extends_existing_section() {
    let mut proj = open_required("sphere-cylinder-supported.chitubox");
    let before = proj.support_counts()[0];
    let extra: Vec<_> = (0..3)
        .map(|i| tiny_record(SegmentType::JOINER, 10.0 + i as f32))
        .collect();
    proj.attach_supports(&[SupportBatch {
        model_index: 0,
        records: extra.clone(),
        mode: SupportMode::Append,
        lift_mm: None,
    }])
    .unwrap();
    let bytes = proj.to_bytes().expect("append verifies");
    let re = Project::from_bytes(bytes).unwrap();
    assert_eq!(re.support_counts()[0], before + 3);
    // the appended records are the last 3 and decode to what we sent
    let got = re.supports(0).unwrap();
    assert_eq!(&got[got.len() - 3..], &extra[..]);
}

#[test]
fn replace_empty_removes_section() {
    let mut proj = open_required("sphere-cylinder-supported.chitubox");
    assert!(proj.model(0).unwrap().has_supports);
    proj.attach_supports(&[SupportBatch {
        model_index: 0,
        records: vec![],
        mode: SupportMode::Replace,
        lift_mm: None,
    }])
    .unwrap();
    assert!(!proj.model(0).unwrap().has_supports);
    let bytes = proj.to_bytes().expect("removal verifies");
    let re = Project::from_bytes(bytes).unwrap();
    assert_eq!(re.support_counts(), vec![0, 13]);
}
