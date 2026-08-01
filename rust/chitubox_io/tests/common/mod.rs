//! Shared test helpers. Committed specimens under
//! `scripts/tests/fixtures/chitubox/` are required and fail by named path when
//! absent. The sole optional specimen is the oversized
//! `treant_v5_candidate.chitubox`, found outside the repo through
//! `CHITUBOX_CORPUS_DIR`; only that exact fixture may print `SKIP:`.
//!
//! See `scripts/tests/fixtures/chitubox/README.md` for what each file is.

#![allow(dead_code)]

use std::path::PathBuf;

/// Committed fixtures, resolved relative to this crate rather than the CWD.
pub fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/tests/fixtures/chitubox")
}

/// The sole optional oversized specimen. No other test fixture may use this
/// fallback: required fixture absence is a test failure.
pub const OPTIONAL_TREANT_CANDIDATE: &str = "treant_v5_candidate.chitubox";

/// Fallback directory for the sole optional oversized specimen.
fn optional_treant_dir() -> PathBuf {
    std::env::var("CHITUBOX_CORPUS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/Users/craig/code/hughescr/dnd-campaigns"))
}

/// Resolve a committed fixture. This intentionally never falls back outside
/// the repository and names the missing fixture in the failure.
pub fn required_fixture_path(name: &str) -> PathBuf {
    assert_ne!(
        name, OPTIONAL_TREANT_CANDIDATE,
        "the optional Treant candidate must use optional_treant_candidate_path"
    );
    let fixture = fixture_dir().join(name);
    assert!(
        fixture.is_file(),
        "required committed ChituBox fixture is missing: {}",
        fixture.display()
    );
    fixture
}

pub fn read_required_fixture(name: &str) -> Vec<u8> {
    let path = required_fixture_path(name);
    std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "required committed ChituBox fixture is unreadable ({}): {error}",
            path.display()
        )
    })
}

/// All required committed round-trip specimens.
pub const REQUIRED_ROUNDTRIP_FILES: &[&str] = &[
    "empty.chitubox",
    "sphere-cylinder.chitubox",
    "sphere-cylinder-supported.chitubox",
    "sphere-moved.chitubox",
    "sphere-scaled.chitubox",
    "cylinder-rotated.chitubox",
    "morel.chitubox",
    "morel_mudskipper_supports.chitubox",
];

/// Return the sole optional oversized Treant candidate, if it exists.
pub fn optional_treant_candidate_path() -> Option<PathBuf> {
    let path = optional_treant_dir().join(OPTIONAL_TREANT_CANDIDATE);
    if path.is_file() {
        Some(path)
    } else {
        eprintln!(
            "SKIP: optional oversized ChituBox fixture is not available: {}",
            path.display()
        );
        None
    }
}

pub fn read_optional_treant_candidate() -> Option<Vec<u8>> {
    let path = optional_treant_candidate_path()?;
    Some(std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "optional oversized ChituBox fixture is unreadable ({}): {error}",
            path.display()
        )
    }))
}

/// Reshape a flat 9-f32/tri stream into points sorted by total_cmp — for
/// order-independent geometry comparison (ChituBox reorders triangles).
pub fn sorted_points(xyz: &[f32]) -> Vec<[f32; 3]> {
    let mut pts: Vec<[f32; 3]> = xyz.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    pts.sort_by(|a, b| {
        a[0].total_cmp(&b[0])
            .then(a[1].total_cmp(&b[1]))
            .then(a[2].total_cmp(&b[2]))
    });
    pts
}

/// Max abs per-coordinate difference between two sorted point sets (panics if
/// counts differ).
pub fn sorted_set_max_diff(a: &[f32], b: &[f32]) -> f32 {
    let pa = sorted_points(a);
    let pb = sorted_points(b);
    assert_eq!(pa.len(), pb.len(), "vertex count mismatch");
    let mut m = 0.0f32;
    for (x, y) in pa.iter().zip(&pb) {
        for k in 0..3 {
            m = m.max((x[k] - y[k]).abs());
        }
    }
    m
}

/// Axis extents (max-min per axis) of a flat 3-f32/vertex stream.
pub fn extents(xyz: &[f32]) -> [f32; 3] {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for v in xyz.chunks_exact(3) {
        for k in 0..3 {
            min[k] = min[k].min(v[k]);
            max[k] = max[k].max(v[k]);
        }
    }
    [max[0] - min[0], max[1] - min[1], max[2] - min[2]]
}
