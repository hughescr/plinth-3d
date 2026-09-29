//! The one integration-test program for `plinth-verify`.
//!
//! Each file below used to be its own test program, and cargo links every
//! test program separately against the whole dependency graph. As modules of
//! one program they share that link. Test names gain the file stem as a prefix
//! (`format_gate::name`).
//!
//! Kept as its own program: `tests/coverage_gate.rs`, which re-runs its own
//! executable with `--exact <test name>` under a changed `PLINTH_THREADS`.
//! A new top-level `tests/<name>.rs` still builds and runs on its own, but a
//! new test belongs here as `tests/it/<name>.rs` plus a `mod` line. See
//! `docs/development.md`, "Where integration tests go".

mod format_gate;
mod no_python_guard;
