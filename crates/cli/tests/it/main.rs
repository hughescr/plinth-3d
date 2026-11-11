//! The one integration-test binary of this crate (design section 4.3).
//!
//! * `failures`: bad input, name collisions, interrupted writes, a full disk, a crash before
//!   publishing and a misbehaving checker, all through the library with a fake checker so
//!   they need nothing built outside this workspace.
//! * `e2e_*` (in `e2e`): the real `plinth` binary and the real `plinth-check` program on the
//!   three original Round STLs, against the committed stage manifest.

//! * `broken_candidate_*` (in `candidate`): a print candidate is never published while a rule
//!   is unverified or violated, with a control that publishes.

//! * `review_*` (in `review`): review mode builds supports for any posed STL to look at; it
//!   never yields a print candidate, and keeps a checker-failed file only there.

mod candidate;
mod common;
mod e2e;
mod failures;
mod review;
