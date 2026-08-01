use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use plinth_mesh::{load_binary_stl, write_binary_stl, Mesh, MeshIoError, WeldPolicy};

#[test]
fn self_contained_two_facet_triangle_soup_round_trip_preserves_order_repetition_and_signed_zero() {
    let path = temporary_directory("signed-zero").join("source.stl");
    fs::create_dir_all(path.parent().expect("test parent")).expect("create test directory");
    fs::write(&path, two_facet_triangle_soup_bytes()).expect("write tracked in-memory fixture");
    let loaded = load_binary_stl(&path, WeldPolicy::None).expect("load source");
    assert_two_facet_source_order(&loaded);
    let output = path.parent().expect("test parent").join("roundtrip.stl");
    write_binary_stl(&output, &loaded).expect("write source without repair");
    let reread = load_binary_stl(&output, WeldPolicy::None).expect("fresh disk re-read");
    assert_two_facet_source_order(&reread);
    assert_mesh_bits_equal(&loaded, &reread, "two-facet in-memory fixture");
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
fn requested_welding_is_rejected_before_stl_decode() {
    let unreadable = PathBuf::from("this-file-must-not-be-read.stl");
    assert!(matches!(
        load_binary_stl(&unreadable, WeldPolicy::ExactBits),
        Err(MeshIoError::UnsupportedWeldPolicy {
            policy: WeldPolicy::ExactBits
        })
    ));
    assert!(matches!(
        load_binary_stl(&unreadable, WeldPolicy::Epsilon(0.1)),
        Err(MeshIoError::UnsupportedWeldPolicy {
            policy: WeldPolicy::Epsilon(value)
        }) if value == 0.1
    ));
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

fn two_facet_triangle_soup_bytes() -> Vec<u8> {
    let mut bytes = vec![0_u8; 84 + 2 * 50];
    bytes[80..84].copy_from_slice(&2_u32.to_le_bytes());
    let facets = [
        [
            0.0_f32, 0.0, 1.0, // normal
            -0.0, 0.0, 0.0, // a
            1.0, 0.0, 0.0, // b
            0.0, 1.0, 0.0, // c
        ],
        [
            1.0_f32, 0.0, 0.0, // normal
            -0.0, 0.0, 0.0, // repeated a, distinct soup vertex
            0.0, 1.0, 0.0, // repeated c, distinct soup vertex
            0.0, 0.0, 1.0, // d
        ],
    ];
    for (facet, values) in facets.iter().enumerate() {
        for (index, value) in values.iter().enumerate() {
            let start = 84 + facet * 50 + index * 4;
            bytes[start..start + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    bytes
}

fn assert_two_facet_source_order(mesh: &Mesh) {
    let expected_verts = [
        [-0.0_f64, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [-0.0_f64, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ];
    let expected_tris = [[0, 1, 2], [3, 4, 5]];
    assert_eq!(
        mesh.tris(),
        expected_tris,
        "facet block order or indices changed"
    );
    assert_eq!(
        mesh.verts().len(),
        expected_verts.len(),
        "triangle soup was deduplicated"
    );
    for (vertex, (actual, expected)) in mesh.verts().iter().zip(expected_verts).enumerate() {
        for axis in 0..3 {
            assert_eq!(
                actual[axis].to_bits(),
                expected[axis].to_bits(),
                "source vertex {vertex} axis {axis} changed bits"
            );
        }
    }
}

fn assert_mesh_bits_equal(expected: &Mesh, actual: &Mesh, name: &str) {
    assert_eq!(
        expected.tris(),
        actual.tris(),
        "{name} triangle identity/order differs"
    );
    assert_eq!(
        expected.verts().len(),
        actual.verts().len(),
        "{name} vertex length differs"
    );
    for (vertex, (left, right)) in expected.verts().iter().zip(actual.verts()).enumerate() {
        for axis in 0..3 {
            assert_eq!(
                left[axis].to_bits(),
                right[axis].to_bits(),
                "{name} vertex {vertex} axis {axis} changed bits"
            );
        }
    }
}

fn temporary_directory(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("plinth-mesh-{label}-{unique}"))
}
