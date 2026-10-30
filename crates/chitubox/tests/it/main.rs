//! The one integration-test binary of this crate (design section 4.3).
//!
//! `native_*` tests read the files ChituBox wrote (T1). Property tests (T0p) generate
//! projects and malformed bytes. Ground truth is only the files in
//! `test-data/chitubox-native/`; this crate's own output is never ground truth.

mod common;
mod malformed;
mod native;
mod pairs;
mod props;
mod writer;
