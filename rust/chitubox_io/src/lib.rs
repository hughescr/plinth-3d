//! `chitubox_io` — read and write ChituBox Basic 1.3.0 `.chitubox` project
//! files without a pre-saved template containing the model: start from an
//! empty project, add STL models, attach per-model support geometry records,
//! set plate placement, save. Pure Rust, wrapper-friendly (plain owned
//! types), batch-first. Every write is verified by structural re-parse before
//! the file is called done.
//!
//! Gold invariants: (a) parse-then-save of an unmodified file is
//! byte-identical; (b) every write re-parses and structurally verifies before
//! done; (c) unknown/opaque regions are preserved verbatim, never zeroed.
//!
//! See `docs/specs/SPEC-CHITUBOX-RS.md` and `docs/specs/CHITUBOX-FORMAT.md`.
//!
//! Honesty clause: a passing `verify` means the output is structurally
//! self-consistent (magics, offsets, coverage) — NOT that ChituBox will load
//! it. ChituBox itself is the only semantic oracle.

// The crate contains zero `unsafe` (audited); pin that so a future edit can't
// reintroduce it unnoticed under the wrapper-safety mandate.
#![forbid(unsafe_code)]

mod codec;
mod consts;
mod document;
mod error;
mod parse;
mod project;
#[cfg(feature = "python")]
mod py;
mod stl;
mod support;
mod transform;
mod types;
mod verify;
mod write;

/// Version of the `chitubox_io._native` Python surface. Bumped whenever that
/// surface changes shape; `scripts/tests/conftest.py` fails the `chitubox`-
/// marked tests when the built extension disagrees with the Python side's
/// expectation, so a stale extension can never certify itself green.
pub const CHITUBOX_API_VERSION: u32 = 1;

pub use error::ChituboxError;
pub use project::Project;
pub use types::{
    ModelGeometry, ModelInfo, ModelSource, OpaqueRegionId, OpaqueRegionInfo, ParseWarning,
    Placement, PlacementUpdate, SegmentType, SupportBatch, SupportMode, SupportRecord,
    TransformInfo, TriangleSoup, ZRotation,
};
