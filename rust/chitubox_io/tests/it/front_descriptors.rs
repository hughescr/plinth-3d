//! Native front-descriptor layout: only `new_empty()` may synthesize the
//! opaque 24-byte-per-model prefix. Parsed projects never resize it.

use chitubox_io::{ChituboxError, ModelGeometry, ModelSource, Placement, Project, TriangleSoup};

const DESCRIPTOR_LEN: usize = 24;

fn source(name: &str) -> ModelSource {
    ModelSource {
        name: name.into(),
        geometry: ModelGeometry::Triangles(TriangleSoup::new(vec![
            1.0, 0.0, 0.0, -1.0, 1.0, 0.0, 0.0, -1.0, 0.0,
        ])),
        placement: Placement::default(),
    }
}

fn u32_at(bytes: &[u8], offset: usize) -> usize {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize
}

#[test]
fn scratch_projects_have_exact_zero_descriptors_before_the_first_mesh() {
    for count in [1, 2, 6] {
        let mut project = Project::new_empty();
        let models: Vec<_> = (0..count)
            .map(|index| source(&format!("model-{index}")))
            .collect();
        project.add_models(&models).unwrap();
        let bytes = project.to_bytes().unwrap();

        let off8 = u32_at(&bytes, 8);
        let off20 = u32_at(&bytes, 20);
        let first_mesh = u32_at(&bytes, off8 + 272);
        let descriptor_end = off20 + DESCRIPTOR_LEN * count;
        assert_eq!(off20, off8 + 680 * count);
        assert_eq!(first_mesh, descriptor_end);
        assert_eq!(&bytes[off20..first_mesh], vec![0; DESCRIPTOR_LEN * count]);

        let reparsed = Project::from_bytes(bytes.clone()).unwrap();
        assert_eq!(reparsed.to_bytes().unwrap(), bytes);
    }
}

#[test]
fn parsed_project_model_addition_fails_closed_without_mutation() {
    let mut source_project = Project::new_empty();
    source_project.add_model(source("native-shaped")).unwrap();
    let bytes = source_project.to_bytes().unwrap();
    let mut parsed = Project::from_bytes(bytes.clone()).unwrap();

    assert!(matches!(
        parsed.add_model(source("would-require-new-descriptor")),
        Err(ChituboxError::UnsupportedOpaqueLayoutMutation { .. })
    ));
    assert_eq!(parsed.to_bytes().unwrap(), bytes);
}

#[test]
#[expect(
    clippy::cast_possible_truncation,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn underlength_descriptor_region_is_rejected_before_mesh_decode() {
    let mut project = Project::new_empty();
    let models: Vec<_> = (0..6)
        .map(|index| source(&format!("model-{index}")))
        .collect();
    project.add_models(&models).unwrap();
    let mut bytes = project.to_bytes().unwrap();
    let off8 = u32_at(&bytes, 8);
    let off20 = u32_at(&bytes, 20);
    // Recreate the former fixed-48-byte layout: model 0 then overlaps the
    // descriptor slots for models 2..5. The parser/independent audit must not
    // accept it as a merely opaque gap.
    bytes[off8 + 272..off8 + 276].copy_from_slice(&((off20 + 48) as u32).to_le_bytes());

    assert!(matches!(
        Project::from_bytes(bytes),
        Err(ChituboxError::HeaderInvariant { .. })
    ));
}
