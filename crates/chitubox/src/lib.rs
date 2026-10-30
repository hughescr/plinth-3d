//! plinth-chitubox: typed reader and writer for ChituBox v1.4.0 `.chitubox` files.
//!
//! Every region of the file is typed data (header, preview with its run-length codec,
//! model records, front block with descriptors, presets and attrs, support sections,
//! records and trailers). [`parse`] follows the pointer chain, checks that every byte
//! belongs to exactly one region, and checks all sizes before allocating.
//! [`Project::to_bytes`] rebuilds every offset, count and length. A file that parses
//! writes back byte for byte. Format reference: `docs/format/chitubox-v1.4.0.md`.
//!
//! Nothing here has been shown to load in ChituBox after a write; see the format
//! document, section 0.

#![warn(clippy::cast_possible_truncation)]

mod bytes;
pub mod error;
pub mod loop_canary;
pub mod mesh;
mod parse;
pub mod preview;
pub mod project;
pub mod supports;
mod write;

pub use error::{ParseError, ParseErrorKind, WriteError};
pub use loop_canary::loop_canary;
pub use mesh::Triangle;
pub use parse::parse;
pub use preview::Preview;
pub use project::{Model, Project};
pub use supports::{Deviation, Preset, RecordType, SupportRecord, Supports};

/// Canary values of this crate and every crate below it, summed. Calling it from
/// production code makes a canary edit drive the real rebuild chain.
#[must_use]
pub fn canary_chain() -> u32 {
    loop_canary()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_is_own_canary() {
        assert_eq!(canary_chain(), loop_canary());
    }
}
