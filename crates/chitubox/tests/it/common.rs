//! Shared helpers: locating and loading the native files.

use plinth_chitubox::{Model, Project, parse};
use std::path::PathBuf;

/// `test-data/chitubox-native` of this checkout.
pub fn native_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-data/chitubox-native")
}

/// The load-test folder (2026-10-30): one file plinth wrote and the same file re-saved by
/// ChituBox v1.4.0.
pub fn load_test_dir() -> PathBuf {
    native_dir().join("load-test-2026-10-30")
}

/// The file plinth's writer made. It is NOT native ground truth, so [`native_files`]
/// leaves it out. It sits next to its ChituBox re-save only to be compared with it.
pub const PLINTH_WRITTEN: &str = "Round 50.keepup.chitubox";

/// Every `.chitubox` file ChituBox wrote: the seven older ones, the eight v1.4.0 samples
/// and the ChituBox re-save in the load-test folder, sorted by path. The plinth-written
/// file in the load-test folder is excluded ([`PLINTH_WRITTEN`]).
pub fn native_files() -> Vec<PathBuf> {
    let dir = native_dir();
    let mut out = Vec::new();
    for d in [dir.clone(), dir.join("samples"), load_test_dir()] {
        for entry in std::fs::read_dir(&d).expect("read native dir") {
            let path = entry.expect("dir entry").path();
            let plinth_written = path.file_name().is_some_and(|n| n == PLINTH_WRITTEN);
            if path.extension().is_some_and(|e| e == "chitubox") && !plinth_written {
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
