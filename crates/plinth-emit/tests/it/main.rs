//! The one integration-test program for `plinth-emit`.
//!
//! Each file below used to be its own test program, and cargo links every
//! test program separately against the whole dependency graph. As modules of
//! one program they share that link. Test names gain the file stem as a prefix
//! (`native_winding_oracle::name`).
//!
//! A new top-level `tests/<name>.rs` still builds and runs on its own, but a
//! new test belongs here as `tests/it/<name>.rs` plus a `mod` line. See
//! `docs/development.md`, "Where integration tests go".

mod native_winding_oracle;
mod no_private_vector_algebra;
