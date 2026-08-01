use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use plinth_mesh::{
    load_binary_stl, load_binary_stls, write_binary_stl, write_binary_stls, Mesh, MeshIoError,
    WeldPolicy,
};

const CORPUS: [&str; 8] = [
    "Sphere.stl",
    "Cylinder.stl",
    "make_box.stl",
    "make_overhang_table.stl",
    "make_toed_slab.stl",
    "make_raking_blade.stl",
    "make_thin_fin.stl",
    "Weeping Treant v1.2.stl",
];

#[test]
fn corpus_binary_stls_round_trip_as_arrays_not_file_bytes() {
    let input_dir = workspace_root().join("corpus/models");
    let paths = CORPUS
        .iter()
        .map(|name| required_fixture(&input_dir.join(name)))
        .collect::<Vec<_>>();
    let loaded = load_binary_stls(&paths, WeldPolicy::None).expect("all required corpus STLs load");
    let out_dir = temporary_directory("corpus-roundtrip");
    fs::create_dir_all(&out_dir).expect("create round-trip output directory");
    let outputs = CORPUS
        .iter()
        .zip(&loaded)
        .map(|(name, mesh)| (out_dir.join(name), mesh.clone()))
        .collect::<Vec<_>>();
    write_binary_stls(&outputs).expect("write corpus round-trip outputs");
    let reread_paths = outputs
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    let reread = load_binary_stls(&reread_paths, WeldPolicy::None).expect("fresh disk re-read");
    for ((name, before), after) in CORPUS.iter().zip(&loaded).zip(&reread) {
        assert_mesh_bits_equal(before, after, name);
        assert_eq!(
            after.weld_policy(),
            WeldPolicy::None,
            "{name} changed weld policy"
        );
    }
    fs::remove_dir_all(out_dir).expect("remove test-only output directory");
}

#[test]
fn reader_preserves_triangle_soup_order_repetition_and_signed_zero() {
    let path = temporary_directory("signed-zero").join("source.stl");
    fs::create_dir_all(path.parent().expect("test parent")).expect("create test directory");
    let source = face_soup_with_signed_zero();
    write_binary_stl(&path, &source).expect("write f32-exact source");
    let loaded = load_binary_stl(&path, WeldPolicy::None).expect("load source");
    assert_eq!(loaded.verts.len(), 3, "reader must not weld triangle soup");
    assert_eq!(
        loaded.tris,
        vec![[0, 1, 2]],
        "reader must not reorder indices"
    );
    assert_eq!(loaded.verts[0][0].to_bits(), (-0.0_f64).to_bits());
    assert_eq!(loaded.verts[1][0].to_bits(), (-0.0_f64).to_bits());
    fs::remove_dir_all(path.parent().expect("test parent"))
        .expect("remove test-only output directory");
}

#[test]
fn malformed_nonfinite_and_lossy_stls_are_semantic_errors() {
    let dir = temporary_directory("bad-stl");
    fs::create_dir_all(&dir).expect("create test directory");

    let ascii = dir.join("ascii.stl");
    fs::write(&ascii, b"solid this is text\nendsolid\n").expect("write malformed fixture");
    assert!(matches!(
        load_binary_stl(&ascii, WeldPolicy::None),
        Err(MeshIoError::AsciiStlUnsupported { .. })
    ));

    let trailing = dir.join("trailing.stl");
    let mut bytes = one_facet_bytes();
    bytes.push(0);
    fs::write(&trailing, bytes).expect("write trailing fixture");
    assert!(matches!(
        load_binary_stl(&trailing, WeldPolicy::None),
        Err(MeshIoError::LengthMismatch { .. })
    ));

    let nonfinite = dir.join("nonfinite.stl");
    let mut bytes = one_facet_bytes();
    bytes[84..88].copy_from_slice(&f32::NAN.to_le_bytes());
    fs::write(&nonfinite, bytes).expect("write nonfinite fixture");
    assert!(matches!(
        load_binary_stl(&nonfinite, WeldPolicy::None),
        Err(MeshIoError::NonFinite {
            field: "normal",
            ..
        })
    ));

    let lossy = dir.join("lossy.stl");
    let mesh = Mesh::new(
        vec![
            [1.0 + f64::EPSILON, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ],
        vec![[0, 1, 2]],
    )
    .expect("finite fixture");
    assert!(matches!(
        write_binary_stl(&lossy, &mesh),
        Err(MeshIoError::LossyF32Encoding { .. })
    ));
    fs::remove_dir_all(dir).expect("remove test-only output directory");
}

#[test]
fn binary_stl_declared_count_is_checked_before_allocation() {
    let path = temporary_directory("count-overflow").join("huge.stl");
    fs::create_dir_all(path.parent().expect("test parent")).expect("create test directory");
    let mut bytes = vec![0_u8; 84];
    bytes[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
    fs::write(&path, bytes).expect("write count fixture");
    assert!(matches!(
        load_binary_stl(&path, WeldPolicy::None),
        Err(MeshIoError::LengthMismatch { .. } | MeshIoError::CountOverflow { .. })
    ));
    fs::remove_dir_all(path.parent().expect("test parent"))
        .expect("remove test-only output directory");
}

fn face_soup_with_signed_zero() -> Mesh {
    Mesh::new(
        vec![[-0.0, 0.0, 0.0], [-0.0, 1.0, 0.0], [-0.0, 0.0, 1.0]],
        vec![[0, 1, 2]],
    )
    .expect("valid signed-zero face soup")
}

fn one_facet_bytes() -> Vec<u8> {
    let mut bytes = vec![0_u8; 134];
    bytes[80..84].copy_from_slice(&1_u32.to_le_bytes());
    let values = [
        0.0_f32, 0.0, 1.0, // normal
        0.0, 0.0, 0.0, // a
        1.0, 0.0, 0.0, // b
        0.0, 1.0, 0.0, // c
    ];
    for (index, value) in values.iter().enumerate() {
        let start = 84 + index * 4;
        bytes[start..start + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn assert_mesh_bits_equal(expected: &Mesh, actual: &Mesh, name: &str) {
    assert_eq!(
        expected.tris, actual.tris,
        "{name} triangle identity/order differs"
    );
    assert_eq!(
        expected.verts.len(),
        actual.verts.len(),
        "{name} vertex length differs"
    );
    for (vertex, (left, right)) in expected.verts.iter().zip(&actual.verts).enumerate() {
        for axis in 0..3 {
            assert_eq!(
                left[axis].to_bits(),
                right[axis].to_bits(),
                "{name} vertex {vertex} axis {axis} changed bits"
            );
        }
    }
}

fn required_fixture(path: &Path) -> PathBuf {
    assert!(
        path.is_file(),
        "required corpus fixture missing: {}; provision the exact eight-model corpus before this gate",
        path.display()
    );
    path.to_path_buf()
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate has workspace parent")
        .to_path_buf()
}

fn temporary_directory(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("plinth-mesh-{label}-{unique}"))
}
