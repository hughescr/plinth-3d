//! SPEC-CHITUBOX-RS R1: a native opaque preview is installed exactly once for
//! from-scratch populated projects, while every parsed nonempty preview is
//! preserved verbatim.

mod common;

use chitubox_io::{ModelGeometry, ModelSource, Placement, Project, TriangleSoup};
use common::read_required_fixture;
use sha2::{Digest, Sha256};

const NATIVE_PREVIEW: &[u8] = include_bytes!("../assets/two-supports-preview.bin");
const PREVIEW_SHA256: &str = "e7c0f523b3d56a1aad13a465247a032d3695d38b395df2e5fe1b64bb7061ffde";
const EMPTY_TEMPLATE: &[u8] = include_bytes!("../assets/empty.chitubox");

fn preview(bytes: &[u8]) -> &[u8] {
    let off8 = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    &bytes[420..off8]
}

fn source(name: &str) -> ModelSource {
    ModelSource {
        name: name.into(),
        geometry: ModelGeometry::Triangles(TriangleSoup::new(vec![
            1.0, 0.0, 0.0, -1.0, 1.0, 0.0, 0.0, -1.0, 0.0,
        ])),
        placement: Placement::default(),
    }
}

fn assert_layout(bytes: &[u8], model_count: usize) {
    let off8 = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let off20 = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
    assert_eq!(off8, 420 + NATIVE_PREVIEW.len());
    assert_eq!(off20, off8 + 680 * model_count);
    assert_eq!(preview(bytes), NATIVE_PREVIEW);
}

#[test]
fn committed_preview_is_the_exact_native_fixture_span_and_pinned_sha() {
    assert_eq!(NATIVE_PREVIEW.len(), 26_996);
    assert_eq!(
        format!("{:x}", Sha256::digest(NATIVE_PREVIEW)),
        PREVIEW_SHA256
    );

    let fixture = read_required_fixture("two-supports.chitubox");
    let parsed = Project::from_bytes(fixture).expect("parse native UI fixture");
    let roundtripped = parsed.to_bytes().expect("round-trip native UI fixture");
    assert_eq!(preview(&roundtripped), NATIVE_PREVIEW);
}

#[test]
fn zero_model_empty_project_remains_exact_420_byte_template() {
    let bytes = Project::new_empty().to_bytes().expect("serialize empty");
    assert_eq!(bytes.len(), 420);
    assert_eq!(bytes, EMPTY_TEMPLATE);
}

#[test]
fn first_one_or_six_from_scratch_models_get_native_preview_and_roundtrip() {
    for count in [1, 6] {
        let mut project = Project::new_empty();
        let sources: Vec<_> = (0..count)
            .map(|index| source(&format!("m{index}")))
            .collect();
        assert_eq!(
            project.add_models(&sources).unwrap(),
            (0..count).collect::<Vec<_>>()
        );

        let bytes = project.to_bytes().expect("serialize populated project");
        assert_layout(&bytes, count);
        let reparsed = Project::from_bytes(bytes.clone()).expect("reparse populated project");
        assert_eq!(reparsed.model_count(), count);
        assert_eq!(reparsed.to_bytes().unwrap(), bytes);
    }
}

#[test]
fn existing_nonempty_preview_is_preserved_when_placement_changes() {
    let fixture = read_required_fixture("two-supports.chitubox");
    let native_preview = preview(&fixture).to_vec();
    let mut project = Project::from_bytes(fixture).expect("parse native UI fixture");
    project
        .set_placement(
            0,
            Placement {
                x: 1.0,
                y: -2.0,
                z_lift: 0.0,
            },
        )
        .expect("change placement");

    let bytes = project
        .to_bytes()
        .expect("serialize mutated native project");
    assert_eq!(preview(&bytes), native_preview);
}

#[test]
fn later_model_additions_do_not_duplicate_preview_and_are_deterministic() {
    let mut incremental = Project::new_empty();
    incremental.add_model(source("first")).unwrap();
    let once = incremental.to_bytes().unwrap();
    assert_layout(&once, 1);
    incremental
        .add_models(&[source("second"), source("third")])
        .unwrap();
    let incremental_bytes = incremental.to_bytes().unwrap();
    assert_layout(&incremental_bytes, 3);

    let mut batched = Project::new_empty();
    batched
        .add_models(&[source("first"), source("second"), source("third")])
        .unwrap();
    assert_eq!(incremental_bytes, batched.to_bytes().unwrap());
}
