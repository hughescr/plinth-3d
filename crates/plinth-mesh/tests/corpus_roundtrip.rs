//! The provisioned-corpus gate is intentionally not part of the default test
//! suite: source STLs are ignored local inputs.  Cargo enables this target only
//! with `--features corpus-gate`; once invoked it fails loudly for every absent
//! or altered exact member.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use plinth_mesh::{
    load_binary_stl, load_binary_stls, make_box_default, make_overhang_table, make_raking_blade,
    make_thin_fin, make_toed_slab, write_binary_stls, Mesh, WeldPolicy,
};
use sha2::{Digest, Sha256};

const CORPUS: [CorpusMember; 8] = [
    CorpusMember::new(
        "Sphere",
        "Sphere.stl",
        "48c27bb596a265900a614171d7ffe3a7a4ea80d95f1c620cf3ab00f67fe0a1d5",
    ),
    CorpusMember::new(
        "Cylinder",
        "Cylinder.stl",
        "7bd60bd7fb83882f2bdc9c977e4f15e55f5cf0342c05c31584c586782505f741",
    ),
    CorpusMember::new(
        "make_box",
        "make_box.stl",
        "aada5b79e87461c6f74afe4e16a5f651aed47fd6638a50a225e35760a8010c3b",
    ),
    CorpusMember::new(
        "make_overhang_table",
        "make_overhang_table.stl",
        "a776132a2cc4a43298d9ca646a4265a7ba60ed7d37de7618147ac754dfe8c996",
    ),
    CorpusMember::new(
        "make_toed_slab",
        "make_toed_slab.stl",
        "ceb70f2b549fb131b0f177033401c50879a8fbc6d575b27459c0f9574d9f1169",
    ),
    CorpusMember::new(
        "make_raking_blade",
        "make_raking_blade.stl",
        "0979f5812c26b3bd88f6e36bb792573fbdc9c023656c46544f2bffdecddefb97",
    ),
    CorpusMember::new(
        "make_thin_fin",
        "make_thin_fin.stl",
        "09bec5c6ff8ff169f91438c209b12c3e6d9eb0dbb7e2fd7611e98d1884516c4c",
    ),
    CorpusMember::new(
        "Weeping Treant v1.2",
        "Weeping Treant v1.2.stl",
        "4c027b7849019ab9598c85f285b9df7d480aacfd1733123a32a8a81239525552",
    ),
];

#[derive(Clone, Copy)]
struct CorpusMember {
    name: &'static str,
    file: &'static str,
    sha256: &'static str,
}

impl CorpusMember {
    const fn new(name: &'static str, file: &'static str, sha256: &'static str) -> Self {
        Self { name, file, sha256 }
    }
}

#[test]
fn exact_eight_member_manifest_and_array_roundtrip_gate() {
    let root = workspace_root();
    assert_manifest_contract(&root.join("corpus/manifest.json"));
    let input_dir = root.join("corpus/models");
    let paths = CORPUS
        .iter()
        .map(|member| required_digest_match(&input_dir.join(member.file), member))
        .collect::<Vec<_>>();
    let loaded = load_binary_stls(&paths, WeldPolicy::None).expect("all required corpus STLs load");
    let output_dir = temporary_directory("corpus-roundtrip");
    fs::create_dir_all(&output_dir).expect("create gate output directory");
    let outputs = CORPUS
        .iter()
        .zip(&loaded)
        .map(|(member, mesh)| (output_dir.join(member.file), mesh.clone()))
        .collect::<Vec<_>>();
    write_binary_stls(&outputs).expect("write corpus round-trip outputs");
    let reread_paths = outputs
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    let reread = load_binary_stls(&reread_paths, WeldPolicy::None).expect("fresh disk re-read");
    for ((member, before), after) in CORPUS.iter().zip(&loaded).zip(&reread) {
        assert_mesh_bits_equal(before, after, member.name);
        assert_eq!(
            after.weld_policy(),
            WeldPolicy::None,
            "{} changed weld policy",
            member.name
        );
    }
    fs::remove_dir_all(output_dir).expect("remove test-only output directory");
}

#[test]
fn native_builders_match_the_five_frozen_synthetic_stls() {
    let root = workspace_root();
    let input_dir = root.join("corpus/models");
    let builders: [(&CorpusMember, Result<Mesh, plinth_mesh::MeshError>); 5] = [
        (&CORPUS[2], make_box_default()),
        (&CORPUS[3], make_overhang_table()),
        (&CORPUS[4], make_toed_slab()),
        (&CORPUS[5], make_raking_blade()),
        (&CORPUS[6], make_thin_fin()),
    ];
    for (member, native) in builders {
        let path = required_digest_match(&input_dir.join(member.file), member);
        let frozen = load_binary_stl(&path, WeldPolicy::None)
            .unwrap_or_else(|error| panic!("could not load {}: {error}", path.display()));
        assert_builder_matches_stl(&native.expect("valid native fixture"), &frozen, member.name);
    }
}

fn assert_manifest_contract(path: &Path) {
    let contents = fs::read_to_string(path).unwrap_or_else(|error| {
        panic!(
            "required corpus manifest missing at {}: {error}",
            path.display()
        )
    });
    assert!(
        !contents.contains("Cone"),
        "manifest must not contain a Cone corpus member"
    );
    let mut cursor = 0;
    for member in CORPUS {
        let entry = format!("\"name\": \"{}\"", member.name);
        let position = contents[cursor..]
            .find(&entry)
            .unwrap_or_else(|| panic!("manifest missing required ordered member {}", member.name));
        cursor += position + entry.len();
        assert!(
            contents.contains(member.sha256),
            "manifest does not declare expected SHA-256 for {}",
            member.name
        );
    }
}

fn required_digest_match(path: &Path, member: &CorpusMember) -> PathBuf {
    let bytes = fs::read(path).unwrap_or_else(|error| {
        panic!(
            "required corpus model {} missing or unreadable at {}: {error}",
            member.name,
            path.display()
        )
    });
    let actual = format!("{:x}", Sha256::digest(bytes));
    assert_eq!(
        actual,
        member.sha256,
        "corpus digest mismatch for {} at {}",
        member.name,
        path.display()
    );
    path.to_path_buf()
}

fn assert_builder_matches_stl(native: &Mesh, frozen: &Mesh, name: &str) {
    assert_eq!(
        native.tris().len(),
        frozen.tris().len(),
        "{name} triangle count differs"
    );
    for (face, indices) in native.tris().iter().enumerate() {
        for (corner, index) in indices.iter().enumerate() {
            let actual = native.verts()[*index as usize];
            let expected = frozen.verts()[face * 3 + corner];
            for axis in 0..3 {
                assert_eq!(
                    f64::from(actual[axis] as f32).to_bits(),
                    expected[axis].to_bits(),
                    "{name} face {face} corner {corner} axis {axis} differs from frozen Python STL"
                );
            }
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
