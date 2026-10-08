#![allow(
    clippy::needless_range_loop,
    reason = "axis loops over parallel [f64; 3] arrays read better as 0..3 than \
              as a zip of three iterators; the index IS the axis"
)]

//! # plinth-audit -- the emitted-bytes print-candidate audit
//!
//! This crate reads **only the emitted bytes** of a `.chitubox` file and grades
//! whether the file is worth printing. It is a standalone crate with its own
//! `Cargo.toml`, deliberately **not** a workspace member, with **zero**
//! dependencies -- no `plinth-*` crate, no `geom_kernel`, and no external
//! crate of any kind. It reimplements every primitive it needs.
//!
//! That independence is the whole value of the instrument. The engine's own
//! records are known to lie: every support record declares a seat depth of
//! exactly 0.300 mm and the geometry delivers it on 11.6 % of 189 real-mini
//! tips; a clearance predicate returns a value meaning "measured nothing" and
//! three production sites read it as a pass. **An audit that consumes the
//! engine's disclosures inherits every one of those defects. Wanting to import
//! an engine type is the signal to reimplement.**
//!
//! Its two specifications are `docs/specs/audit-contract.md` (what the bytes
//! can answer) and `docs/specs/audit-invariants.md` (the ten gating
//! invariants, the report, and the output schema). Where an engine source and
//! `audit-invariants.md` disagree, that file governs: the audit is not
//! permitted to agree with the engine by construction.
//!
//! ## Four rules that no implementation convenience overrides
//!
//! 1. **Reconstruct where geometry allows it.** A fact recoverable from the
//!    triangles or the record coordinates is `RECONSTRUCTED`; one readable only
//!    from a stored field is `DECLARED`. Every output value is tagged.
//! 2. **A disagreement between declared and reconstructed is a hard failure.**
//! 3. **"Not measured", "not implemented" and "indeterminate" never map to
//!    pass.** [`check::Status`] has exactly two values and
//!    [`check::CheckOutcome::status`] enforces it.
//! 4. **Every invariant is a number, not a judgement.**
//!
//! ## Module map
//!
//! | module | what it owns |
//! |---|---|
//! | [`consts`] | every pinned literal, restated with its citation |
//! | [`reader`] | the container parse: header, models, index, 72-byte records, meshes |
//! | [`vec3`] | vector algebra, point-triangle distance, ray/triangle, segment/segment |
//! | [`mesh`] | welding, closure, winding, components, spatial index, ray parity, nearest surface |
//! | [`primitives`] | analytic support solids, exact chords, and mesh-versus-record verification |
//! | [`geometry`] | the `geometry` subcommand: heads, intrusion, junction topology and contact angles, measured from the emitted file |
//! | [`graph`] | support-graph reconstruction from endpoint coincidence, with ambiguity represented |
//! | [`seat`] | the shared seat reconstruction I1, I2, I3, I5 and I6 all read |
//! | [`raster`] | the run-length printer-grid rasteriser, full containment |
//! | [`ownership`] | the demand set and the complete Voronoi partition I2, I3 and I4 share |
//! | [`sidecar`] | the optional `.coverage.repair.json`, where `null` never becomes zero |
//! | [`check`] | the [`check::Check`] trait, [`check::CheckOutcome`], [`check::Failure`] |
//! | [`checks`] | the ten invariants plus the non-gating report, one file each |
//! | [`context`] | declared inputs and the shared per-model context |
//! | [`run`] | the driver and the output document |
//! | [`json`] | hand-rolled JSON value, reader and writer |
//! | [`sha256`] | hand-rolled SHA-256 for `file.sha256` |
//!
//! ## Implementing one invariant
//!
//! Edit exactly one file under `src/checks/`. `src/checks/mod.rs` already
//! declares and registers all nine modules, so nobody has to touch a shared
//! module list. Read that file's header for the per-invariant brief.

pub mod check;
pub mod checks;
pub mod consts;
pub mod context;
pub mod geometry;
pub mod graph;
pub mod json;
pub mod mesh;
pub mod ownership;
pub mod primitives;
pub mod raster;
pub mod reader;
pub mod run;
pub mod seat;
pub mod sha256;
pub mod sidecar;
pub mod synth;
pub mod vec3;
