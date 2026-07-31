//! Error cases (§7.5): malformed input and invalid API use never panic and
//! return the right typed error; failed batches leave the Project unchanged.

mod common;
use chitubox_io::*;
use common::*;

#[test]
fn wrong_magic() {
    let mut bytes = vec![0u8; 420];
    bytes[0..4].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
    match Project::from_bytes(bytes) {
        Err(ChituboxError::NotChitubox { found_magic }) => assert_eq!(found_magic, 0xDEAD_BEEF),
        other => panic!("expected NotChitubox, got {other:?}"),
    }
}

#[test]
fn truncated_header() {
    let bytes = vec![0x43, 0x12, 0x23, 0xAB, 0, 0]; // magic ok, too short
    assert!(matches!(
        Project::from_bytes(bytes),
        Err(ChituboxError::Truncated { .. })
    ));
}

#[test]
fn truncated_body() {
    let Some(mut bytes) = read_corpus("sphere-cylinder.chitubox") else {
        return;
    };
    bytes.truncate(bytes.len() / 2); // cut through the mesh region
    assert!(matches!(
        Project::from_bytes(bytes),
        Err(ChituboxError::Truncated { .. })
    ));
}

#[test]
fn model_index_out_of_range() {
    let mut proj = Project::new_empty();
    assert!(matches!(
        proj.set_placement(0, Placement::default()),
        Err(ChituboxError::ModelIndexOutOfRange { index: 0, count: 0 })
    ));
    assert!(matches!(
        proj.model(5),
        Err(ChituboxError::ModelIndexOutOfRange { .. })
    ));
    assert!(matches!(
        proj.rotate_model_z(0, 10.0),
        Err(ChituboxError::ModelIndexOutOfRange { .. })
    ));
}

#[test]
fn duplicate_batch_index_leaves_project_unchanged() {
    let mut proj = one_model_project();
    let before = proj.placements();
    let err = proj.set_placements(&[
        PlacementUpdate {
            model_index: 0,
            placement: Placement {
                x: 1.0,
                y: 2.0,
                z_lift: 0.0,
            },
        },
        PlacementUpdate {
            model_index: 0,
            placement: Placement {
                x: 9.0,
                y: 9.0,
                z_lift: 0.0,
            },
        },
    ]);
    assert!(matches!(
        err,
        Err(ChituboxError::DuplicateBatchIndex { index: 0 })
    ));
    assert_eq!(proj.placements(), before, "no partial mutation");
}

#[test]
fn bad_triangle_buffer() {
    let mut proj = Project::new_empty();
    let err = proj.add_model(ModelSource {
        name: "x".into(),
        geometry: ModelGeometry::Triangles(TriangleSoup::new(vec![0.0, 1.0, 2.0, 3.0])), // 4 floats
        placement: Placement::default(),
    });
    assert!(matches!(
        err,
        Err(ChituboxError::BadTriangleBuffer { len: 4 })
    ));
    assert_eq!(proj.model_count(), 0);
}

#[test]
fn nan_placement() {
    let mut proj = one_model_project();
    let err = proj.set_placement(
        0,
        Placement {
            x: f32::NAN,
            y: 0.0,
            z_lift: 0.0,
        },
    );
    assert!(matches!(err, Err(ChituboxError::NonFiniteValue { .. })));
}

#[test]
fn name_too_long_and_embedded_nul() {
    let mut proj = Project::new_empty();
    let long = "a".repeat(64);
    assert!(matches!(
        proj.add_model(ModelSource {
            name: long,
            geometry: ModelGeometry::Triangles(unit_tri()),
            placement: Placement::default(),
        }),
        Err(ChituboxError::NameInvalid { .. })
    ));
    assert!(matches!(
        proj.add_model(ModelSource {
            name: "a\0b".into(),
            geometry: ModelGeometry::Triangles(unit_tri()),
            placement: Placement::default(),
        }),
        Err(ChituboxError::NameInvalid { .. })
    ));
    assert_eq!(proj.model_count(), 0);
}

#[test]
fn ascii_stl_unsupported() {
    // "solid ..." text that is NOT a valid binary STL (size won't match).
    let ascii = b"solid teapot\nfacet normal 0 0 0\nendsolid\n".to_vec();
    let mut proj = Project::new_empty();
    let err = proj.add_model(ModelSource {
        name: "t".into(),
        geometry: ModelGeometry::StlPath(write_tmp("ascii.stl", &ascii)),
        placement: Placement::default(),
    });
    assert!(matches!(err, Err(ChituboxError::AsciiStlUnsupported)));
}

#[test]
fn oversized_opaque_overwrite() {
    let Some(proj) =
        read_corpus("sphere-cylinder.chitubox").map(|b| Project::from_bytes(b).unwrap())
    else {
        return;
    };
    let mut proj = proj;
    let regions = proj.opaque_regions();
    let settings = regions.iter().find(|r| r.kind == "settings").unwrap();
    let err = proj.overwrite_opaque_region_unchecked(settings.id, &vec![0u8; settings.len + 1]);
    assert!(matches!(
        err,
        Err(ChituboxError::OpaqueLengthMismatch { .. })
    ));
    // same-length succeeds and preserves round-trip structure
    proj.overwrite_opaque_region_unchecked(settings.id, &vec![0u8; settings.len])
        .unwrap();
    assert!(proj.to_bytes().is_ok());
}

#[test]
fn corrupt_support_pointer() {
    let Some(mut bytes) = read_corpus("sphere-cylinder-supported.chitubox") else {
        return;
    };
    // model 0 record off268 -> point into the header (garbage)
    let off8 = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let ptr_pos = off8 + 268;
    bytes[ptr_pos..ptr_pos + 4].copy_from_slice(&4u32.to_le_bytes());
    let err = Project::from_bytes(bytes);
    assert!(
        matches!(
            err,
            Err(ChituboxError::SupportSectionMalformed { .. })
                | Err(ChituboxError::SupportPointerInvalid { .. })
                | Err(ChituboxError::Truncated { .. })
        ),
        "got {err:?}"
    );
}

// -- helpers ----------------------------------------------------------------

fn unit_tri() -> TriangleSoup {
    TriangleSoup::new(vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0])
}

fn one_model_project() -> Project {
    let mut proj = Project::new_empty();
    proj.add_model(ModelSource {
        name: "m".into(),
        geometry: ModelGeometry::Triangles(unit_tri()),
        placement: Placement::default(),
    })
    .unwrap();
    proj
}

fn write_tmp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let dir = std::env::temp_dir();
    let p = dir.join(format!("chitubox_io_test_{}_{name}", std::process::id()));
    std::fs::write(&p, bytes).unwrap();
    p
}
