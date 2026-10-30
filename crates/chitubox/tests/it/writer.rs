//! The writer rebuilds every offset, count and length after an edit, and refuses
//! projects it cannot write before producing any byte.

use crate::common::{bits, sample, sample_bytes};
use plinth_chitubox::{Model, Preview, Project, WriteError, parse};

#[test]
fn writer_lift_edit_changes_only_the_translate_z_bytes() {
    // 01 -> 02 in the typed model: set Z to 5.0 and nothing else.
    let original = sample_bytes("01-round25-plain");
    let mut p = parse(&original).unwrap();
    p.models[0].translate[2] = 5.0;
    let out = p.to_bytes().unwrap();
    assert_eq!(out.len(), original.len());
    let off8 = u32::from_le_bytes([original[8], original[9], original[10], original[11]]) as usize;
    let diff: Vec<usize> = (0..out.len()).filter(|&i| out[i] != original[i]).collect();
    assert!(!diff.is_empty());
    assert!(
        diff.iter().all(|&i| (off8 + 264..off8 + 268).contains(&i)),
        "{diff:?}"
    );
    assert_eq!(
        bits(&parse(&out).unwrap().models[0].translate),
        bits(&[0.0, 0.0, 5.0])
    );
}

#[test]
fn writer_shrinking_supports_rebuilds_every_pointer() {
    let original = sample_bytes("04-round25-supports-raft-connected");
    let mut p = parse(&original).unwrap();
    let s = p.models[0].supports.as_mut().unwrap();
    // Drop the nine bridges and ten pads (68..87) and their attrs.
    let removed_mesh: usize = s.records.drain(68..87).map(|r| r.mesh.len() * 36).sum();
    s.attrs.drain(68..87);
    let out = p.to_bytes().unwrap();
    assert_eq!(out.len(), original.len() - removed_mesh - 19 * 72 - 19 * 8);
    let back = parse(&out).unwrap();
    assert_eq!(back, p);
    assert!(back.deviations().is_empty());
    assert_eq!(back.models[0].supports.as_ref().unwrap().records.len(), 88);
}

#[test]
fn writer_adding_a_model_to_an_opened_project_works() {
    // The old crate refused this. The new one rebuilds the table, descriptors and offsets.
    let mut p = sample("04-round25-supports-raft-connected");
    let mut second = p.models[0].clone();
    second.name = "Round 25.stl #1".into();
    second.translate[0] = 254.999_97;
    second.supports = None;
    p.models.push(second);
    let out = p.to_bytes().unwrap();
    let back = parse(&out).unwrap();
    assert_eq!(back, p);
    assert!(back.models[1].supports.is_none());
}

#[test]
fn writer_rejects_bad_projects_before_writing() {
    let model = |name: &str| Model {
        name: name.into(),
        translate: [0.0; 3],
        mesh: vec![],
        supports: None,
    };
    let with = |m: Model| Project {
        models: vec![m],
        ..Project::default()
    };
    assert_eq!(
        with(model(&"x".repeat(256))).to_bytes(),
        Err(WriteError::BadName { model: 0 })
    );
    assert_eq!(
        with(model("a\0b")).to_bytes(),
        Err(WriteError::BadName { model: 0 })
    );
    assert!(with(model(&"x".repeat(255))).to_bytes().is_ok());
    let mut m = model("a");
    m.translate[1] = f32::NAN;
    assert!(matches!(
        with(m).to_bytes(),
        Err(WriteError::NonFinite { .. })
    ));
    let mut m = model("a");
    m.mesh = vec![[[f32::INFINITY; 3]; 3]];
    assert!(matches!(
        with(m).to_bytes(),
        Err(WriteError::NonFinite { .. })
    ));
    let p = Project {
        preview: Some(Preview {
            width: 2,
            height: 2,
            pixels: vec![0; 3],
        }),
        ..Project::default()
    };
    assert_eq!(p.to_bytes(), Err(WriteError::BadPreview));
}

#[test]
fn writer_empty_project_is_the_empty_native_file() {
    let bytes = std::fs::read(crate::common::native_dir().join("empty.chitubox")).unwrap();
    assert_eq!(Project::default().to_bytes().unwrap(), bytes);
}
