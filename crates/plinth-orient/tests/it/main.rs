//! The one integration-test program for `plinth-orient`.
//!
//! Each file below used to be its own test program, and cargo links every
//! test program separately against the whole dependency graph. As modules of
//! one program they share that link. Test names gain the file stem as a prefix
//! (`orientation_scoring::name`).
//!
//! Kept as its own program: `tests/orient_thread_sweep.rs`, which re-runs its
//! own executable with `--exact <test name>` under a changed `PLINTH_THREADS`.
//! A new top-level `tests/<name>.rs` still builds and runs on its own, but a
//! new test belongs here as `tests/it/<name>.rs` plus a `mod` line. See
//! `docs/development.md`, "Where integration tests go".

mod display_face_objective;
mod flat_face_poses;
mod no_private_vector_algebra;
mod orientation_dense_sweep;
mod orientation_rederivation;
mod orientation_scoring;
mod peel_demand_override;
mod tessellation_invariance;
