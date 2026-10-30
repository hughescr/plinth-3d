//! Shared helpers: locating and loading the native files.

use plinth_chitubox::{Model, Project, parse};
use std::path::PathBuf;

/// `test-data/chitubox-native` of this checkout.
pub fn native_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-data/chitubox-native")
}

/// Every `.chitubox` file ChituBox wrote: the seven older ones and the eight v1.4.0
/// samples, sorted by path.
pub fn native_files() -> Vec<PathBuf> {
    let dir = native_dir();
    let mut out = Vec::new();
    for d in [dir.clone(), dir.join("samples")] {
        for entry in std::fs::read_dir(&d).expect("read native dir") {
            let path = entry.expect("dir entry").path();
            if path.extension().is_some_and(|e| e == "chitubox") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Bytes of one named sample (for example `01-round25-plain`).
pub fn sample_bytes(stem: &str) -> Vec<u8> {
    let path = native_dir()
        .join("samples")
        .join(format!("{stem}.chitubox"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The bit patterns of some floats, for exact comparison.
pub fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}

/// Parse one named sample.
pub fn sample(stem: &str) -> Project {
    parse(&sample_bytes(stem)).unwrap_or_else(|e| panic!("parse {stem}: {e}"))
}

/// The single model of a one-model project.
pub fn only_model(p: &Project) -> &Model {
    assert_eq!(p.models.len(), 1);
    &p.models[0]
}
