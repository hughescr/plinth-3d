//! The stable, dependency-light vocabulary shared by Plinth crates.
//!
//! This crate intentionally contains data contracts and type-level safety
//! mechanisms only. Geometry, scoring algorithms, and acceptance policy live
//! in their owning crates; [`DefectReport`] is the one acceptance authority.

mod cost;
mod defect;
mod digest;
mod frame;
mod id;
mod layer;
mod load;
mod metric;
mod number;
mod provenance;
mod stage;
mod violation;

pub use cost::{
    CostError, CostVector, Fidelity, OptimizationError, OptimizationSummary, OrientationCost,
    ParetoPoint, PartialCost, Resin, Risk, Scar, SearchEvidence,
};
pub use defect::{
    Defect, DefectError, DefectReport, ReportError, Severity, SiteId, Termination, TerminationError,
};
pub use digest::{canonical_f64_bits, digest_f64, digest_f64_slice, ContentDigest, DigestError};
pub use frame::{Centered, Frame, Framed, Plated, Source};
pub use id::{ConstantId, DecisionId, FixerId, ModelId, OrientationId, PlateId, StageId};
pub use layer::{LayerPlan, LayerPlanError};
pub use load::{EffectiveArmCount, LoadSharing, LoadSharingError};
pub use metric::{MetricId, MetricSet, MetricValue};
pub use number::{Finite, FiniteError};
pub use provenance::{
    AcceptSafe, Derived, Measured, Pending, Placeholder, ProvenanceManifest, ProvenanceRef,
};
pub use stage::Stage;
pub use violation::ViolationKind;
