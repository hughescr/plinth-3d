//! Shared test helpers. Most corpus specimens are committed under
//! `scripts/tests/fixtures/chitubox/` and resolve with no setup. The handful
//! that are too large to commit (the multi-tens-of-MB emitter deliverables)
//! still live outside the repo and are looked up under `CHITUBOX_CORPUS_DIR`
//! (default: the dnd-campaigns working dir). A missing corpus file makes the
//! test SKIP with a printed notice, never silently pass.
//!
//! See `scripts/tests/fixtures/chitubox/README.md` for what each file is.

#![allow(dead_code)]

use std::path::PathBuf;

/// Committed fixtures, resolved relative to this crate rather than the CWD.
pub fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/tests/fixtures/chitubox")
}

/// Fallback for specimens too large to commit.
pub fn corpus_dir() -> PathBuf {
    std::env::var("CHITUBOX_CORPUS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/Users/craig/code/hughescr/dnd-campaigns"))
}

pub fn corpus_path(name: &str) -> Option<PathBuf> {
    let fixture = fixture_dir().join(name);
    if fixture.exists() {
        return Some(fixture);
    }
    let external = corpus_dir().join(name);
    if external.exists() {
        return Some(external);
    }
    eprintln!(
        "SKIP: corpus file not found: {} (nor {})",
        fixture.display(),
        external.display()
    );
    None
}

pub fn read_corpus(name: &str) -> Option<Vec<u8>> {
    let p = corpus_path(name)?;
    Some(std::fs::read(p).expect("read corpus file"))
}

/// All corpus round-trip specimens (small + large).
pub const ROUNDTRIP_FILES: &[&str] = &[
    "empty.chitubox",
    "sphere-cylinder.chitubox",
    "sphere-cylinder-supported.chitubox",
    "sphere-moved.chitubox",
    "sphere-scaled.chitubox",
    "cylinder-rotated.chitubox",
    "morel.chitubox",
    "morel_mudskipper_supports.chitubox",
    // Large emitter deliverable: lives outside the repo, so this entry SKIPs
    // unless CHITUBOX_CORPUS_DIR has it. Replaced treant_v4_supports.chitubox,
    // which was lost with the v4 engine that produced it.
    "treant_v5_candidate.chitubox",
];

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
