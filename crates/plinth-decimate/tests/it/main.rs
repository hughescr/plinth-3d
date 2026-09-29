//! The one integration-test program for `plinth-decimate`.
//!
//! Each file below used to be its own test program, and cargo links every
//! test program separately against the whole dependency graph. As modules of
//! one program they share that link. Test names gain the file stem as a prefix
//! (`corpus_budgets::name`), so `cargo test -p plinth-decimate --test it
//! corpus_budgets::` runs one former file.
//!
//! Kept as its own program: `tests/thread_sweep.rs`, which re-runs its own
//! executable with `--exact <test name>` under a changed `PLINTH_THREADS`.
//! A new top-level `tests/<name>.rs` still builds and runs on its own, but a
//! new test belongs here as `tests/it/<name>.rs` plus a `mod` line. See
//! `docs/development.md`, "Where integration tests go".

mod corpus_budgets;
mod execution_budget_refusal;
mod local_minis_repair;
mod no_private_vector_algebra;
