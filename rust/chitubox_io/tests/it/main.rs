//! The one integration-test program for `chitubox_io`.
//!
//! Each file below used to be its own test program, and cargo links every
//! test program separately against the whole dependency graph. As modules of
//! one program they share that link. Test names gain the file stem as a prefix
//! (`roundtrip::name`).
//!
//! The shared helpers stay at `tests/common/mod.rs`, so a top-level test file
//! can still declare `mod common;`; here they are one module that every file
//! reaches as `crate::common`. A new top-level `tests/<name>.rs` still builds
//! and runs on its own, but a new test belongs here as `tests/it/<name>.rs`
//! plus a `mod` line. See `docs/development.md`, "Where integration tests go".

#[path = "../common/mod.rs"]
mod common;

mod durable_rename;
mod errors;
mod front_descriptors;
mod frozen_baseline;
mod preview_fallback;
mod regression_blockers;
mod roundtrip;
mod support_parity;
mod synth_compare;
mod transforms;
