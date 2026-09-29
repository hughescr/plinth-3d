//! The one integration-test program for `plinth-mesh`.
//!
//! Each file below used to be its own test program, and cargo links every
//! test program separately against the whole dependency graph. As modules of
//! one program they share that link. Test names gain the file stem as a prefix
//! (`repair::name`).
//!
//! A new top-level `tests/<name>.rs` still builds and runs on its own, but a
//! new test belongs here as `tests/it/<name>.rs` plus a `mod` line. See
//! `docs/development.md`, "Where integration tests go".

mod display_field;
mod fixtures;
mod no_private_vector_algebra;
mod repair;
mod stl_roundtrip;
mod strict_audit;
mod topology_triage;
mod triage_admission;
