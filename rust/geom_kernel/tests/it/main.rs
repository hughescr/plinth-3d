//! The one integration-test program for `geom_kernel`.
//!
//! Each file below used to be its own test program, and cargo links every
//! test program separately against the whole dependency graph. As modules of
//! one program they share that link. Test names gain the file stem as a prefix
//! (`geometry::name`).
//!
//! Kept as their own programs: `tests/allowed_face_projection.rs`,
//! `tests/native_api.rs` and `tests/point_index.rs`, each of which re-runs its
//! own executable with `--exact <test name>` under a changed `PLINTH_THREADS`;
//! and `tests/allowed_face_projection_index_reuse.rs`, which diffs the
//! process-wide BVH and parity build counters and so must not share a process
//! with tests that build surfaces in parallel.
//! A new top-level `tests/<name>.rs` still builds and runs on its own, but a
//! new test belongs here as `tests/it/<name>.rs` plus a `mod` line. See
//! `docs/development.md`, "Where integration tests go".

mod canon_contracts;
mod face_excluded_clearance;
mod geometry;
mod kn_degenerate_face;
mod kn_import_rule_guard;
mod kn_no_inline_geometry_guard;
mod kn_point_nearest_segment_batch;
mod kn_segment_clearance_unmeasured;
mod near_wall_thin_feature;
mod ray_class;
mod segseg_min;
mod tapered_clearance;
mod walk_classified;
