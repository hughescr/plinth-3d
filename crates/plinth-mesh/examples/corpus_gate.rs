//! Explicit audit of the ignored, locally provisioned eight-model corpus.
//!
//! This is deliberately an example rather than an integration-test feature:
//! `cargo test --workspace --all-features` must remain a clean-checkout gate.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use plinth_mesh::{load_binary_stls, write_binary_stls, Mesh, WeldPolicy};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const EXPECTED: [ExpectedMember; 8] = [
    ExpectedMember::new(
        "Sphere",
        "corpus/models/Sphere.stl",
        "48c27bb596a265900a614171d7ffe3a7a4ea80d95f1c620cf3ab00f67fe0a1d5",
    ),
    ExpectedMember::new(
        "Cylinder",
        "corpus/models/Cylinder.stl",
        "7bd60bd7fb83882f2bdc9c977e4f15e55f5cf0342c05c31584c586782505f741",
    ),
    ExpectedMember::new(
        "make_box",
        "corpus/models/make_box.stl",
        "aada5b79e87461c6f74afe4e16a5f651aed47fd6638a50a225e35760a8010c3b",
    ),
    ExpectedMember::new(
        "make_overhang_table",
        "corpus/models/make_overhang_table.stl",
        "a776132a2cc4a43298d9ca646a4265a7ba60ed7d37de7618147ac754dfe8c996",
    ),
    ExpectedMember::new(
        "make_toed_slab",
        "corpus/models/make_toed_slab.stl",
        "ceb70f2b549fb131b0f177033401c50879a8fbc6d575b27459c0f9574d9f1169",
    ),
    ExpectedMember::new(
        "make_raking_blade",
        "corpus/models/make_raking_blade.stl",
        "0979f5812c26b3bd88f6e36bb792573fbdc9c023656c46544f2bffdecddefb97",
    ),
    ExpectedMember::new(
        "make_thin_fin",
        "corpus/models/make_thin_fin.stl",
        "09bec5c6ff8ff169f91438c209b12c3e6d9eb0dbb7e2fd7611e98d1884516c4c",
    ),
    ExpectedMember::new(
        "Weeping Treant v1.2",
        "corpus/models/Weeping Treant v1.2.stl",
        "4c027b7849019ab9598c85f285b9df7d480aacfd1733123a32a8a81239525552",
    ),
];

#[derive(Clone, Copy)]
struct ExpectedMember {
    name: &'static str,
    stl: &'static str,
    sha256: &'static str,
}

impl ExpectedMember {
    const fn new(name: &'static str, stl: &'static str, sha256: &'static str) -> Self {
        Self { name, stl, sha256 }
    }
}

#[derive(Deserialize)]
struct Manifest {
    models: Vec<ManifestModel>,
}

#[derive(Deserialize)]
struct ManifestModel {
    name: String,
    stl: String,
    source_digest: SourceDigest,
}

#[derive(Deserialize)]
struct SourceDigest {
    algorithm: String,
    hex: String,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => {
            println!(
                "corpus gate passed: exact eight-member manifest and fresh disk array round-trip"
            );
            ExitCode::SUCCESS
        }
        Err(reason) => {
            eprintln!("corpus gate failed: {reason}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let root = workspace_root()?;
    let manifest = read_manifest(&root.join("corpus/manifest.json"))?;
    validate_manifest(&manifest)?;
    let input_paths = EXPECTED
        .iter()
        .map(|expected| verify_input(&root.join(expected.stl), expected))
        .collect::<Result<Vec<_>, _>>()?;
    let loaded = load_binary_stls(&input_paths, WeldPolicy::None)
        .map_err(|error| format!("failed to load exact provisioned corpus: {error}"))?;
    let out_dir = temporary_directory("corpus-gate")?;
    fs::create_dir_all(&out_dir).map_err(|error| {
        format!(
            "cannot create corpus-gate scratch {}: {error}",
            out_dir.display()
        )
    })?;
    let outputs = EXPECTED
        .iter()
        .zip(&loaded)
        .map(|(expected, mesh)| {
            let filename = Path::new(expected.stl)
                .file_name()
                .ok_or_else(|| format!("invalid expected STL path {}", expected.stl))?;
            Ok((out_dir.join(filename), mesh.clone()))
        })
        .collect::<Result<Vec<(PathBuf, Mesh)>, String>>()?;
    write_binary_stls(&outputs)
        .map_err(|error| format!("could not write corpus-gate fresh disk artifacts: {error}"))?;
    let reread_paths = outputs
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    let reread = load_binary_stls(&reread_paths, WeldPolicy::None)
        .map_err(|error| format!("could not fresh re-read corpus-gate artifacts: {error}"))?;
    for ((expected, before), after) in EXPECTED.iter().zip(&loaded).zip(&reread) {
        assert_mesh_bits_equal(before, after, expected.name)?;
    }
    fs::remove_dir_all(&out_dir).map_err(|error| {
        format!(
            "could not remove corpus-gate scratch {}: {error}",
            out_dir.display()
        )
    })?;
    Ok(())
}

fn read_manifest(path: &Path) -> Result<Manifest, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "required manifest missing or unreadable at {}: {error}",
            path.display()
        )
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "manifest at {} is not valid typed JSON: {error}",
            path.display()
        )
    })
}

fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    if manifest.models.len() != EXPECTED.len() {
        return Err(format!(
            "manifest models has {} rows; expected exactly {} ordered rows",
            manifest.models.len(),
            EXPECTED.len()
        ));
    }
    for (row, expected) in manifest.models.iter().zip(EXPECTED) {
        if row.name != expected.name {
            return Err(format!(
                "manifest row name/order mismatch: expected {}, found {}",
                expected.name, row.name
            ));
        }
        if row.stl != expected.stl {
            return Err(format!(
                "manifest {} STL path mismatch: expected {}, found {}",
                expected.name, expected.stl, row.stl
            ));
        }
        if row.source_digest.algorithm != "sha256" || row.source_digest.hex != expected.sha256 {
            return Err(format!(
                "manifest {} source_digest mismatch: expected sha256:{}, found {}:{}",
                expected.name, expected.sha256, row.source_digest.algorithm, row.source_digest.hex
            ));
        }
    }
    Ok(())
}

fn verify_input(path: &Path, expected: &ExpectedMember) -> Result<PathBuf, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "required corpus model {} missing or unreadable at {}: {error}",
            expected.name,
            path.display()
        )
    })?;
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual != expected.sha256 {
        return Err(format!(
            "corpus digest mismatch for {} at {}: expected {}, found {}",
            expected.name,
            path.display(),
            expected.sha256,
            actual
        ));
    }
    Ok(path.to_path_buf())
}

fn assert_mesh_bits_equal(expected: &Mesh, actual: &Mesh, name: &str) -> Result<(), String> {
    if expected.tris() != actual.tris() {
        return Err(format!(
            "{name} fresh disk triangle index identity/order differs"
        ));
    }
    if expected.verts().len() != actual.verts().len() {
        return Err(format!("{name} fresh disk vertex length differs"));
    }
    for (vertex, (left, right)) in expected.verts().iter().zip(actual.verts()).enumerate() {
        for axis in 0..3 {
            if left[axis].to_bits() != right[axis].to_bits() {
                return Err(format!(
                    "{name} fresh disk vertex {vertex} axis {axis} changed f64 bits"
                ));
            }
        }
    }
    Ok(())
}

fn workspace_root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .ok_or_else(|| "plinth-mesh example has no workspace parent".to_owned())
}

fn temporary_directory(label: &str) -> Result<PathBuf, String> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before unix epoch: {error}"))?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!("plinth-mesh-{label}-{unique}")))
}
