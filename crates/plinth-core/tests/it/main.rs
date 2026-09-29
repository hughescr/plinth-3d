//! The one integration-test program for `plinth-core`.
//!
//! Each file below used to be its own test program, and cargo links every
//! test program separately against the whole dependency graph. As modules of
//! one program they share that link. Test names gain the file stem as a prefix
//! (`chord_contracts::name`), so `cargo test -p plinth-core --test it
//! chord_contracts::` runs one former file.
//!
//! Kept as its own program: `tests/compile_fail.rs` (trybuild drives nested
//! `cargo` builds of its own). A new top-level `tests/<name>.rs` still builds
//! and runs on its own, but a new test belongs here as `tests/it/<name>.rs`
//! plus a `mod` line. See `docs/development.md`, "Where integration tests go".

mod artifact_format_contracts;
mod chord_contracts;
mod constants_evidence;
mod constants_golden;
mod constants_ledger_consistency;
mod constants_placeholders;
mod constants_registry;
mod core_contracts;
mod coverage_defect_contracts;
mod no_private_vector_algebra;
mod printer_profile;
mod scar_area_cap_policy;
