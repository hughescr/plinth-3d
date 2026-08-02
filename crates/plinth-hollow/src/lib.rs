//! Data boundary for hollowing, cavity extraction, drilling, and visible solid fallback.
//!
//! This checkpoint intentionally contains no voxel, boolean, or drain-search behavior. Behavior
//! lands on the parallel `plinth-hollow` implementation branch with this signature:
//!
//! `plinth_hollow::run(OrientedModel, &HollowRequest) -> Result<HollowedModel, HollowError>`
//!
//! The result always owns exactly one downstream mesh. A fallback moves the oriented solid into
//! that slot and records `SolidFallback`; it never retains a second hidden hollow candidate.
#![forbid(unsafe_code)]

use plinth_core::{Finite, PrinterProfile};
use plinth_mesh::Mesh;
use plinth_orient::OrientationHistory;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Hollowing policy is selected from the printer profile and registered constants.
#[derive(Clone, Debug, PartialEq)]
pub struct HollowRequest {
    pub printer: PrinterProfile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolidFallbackReason {
    IneligibleForHollowing,
    CavityNotDrainable,
    InternalSupportRefused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HollowDisposition {
    HollowedAndDrilled,
    SolidFallback { reason: SolidFallbackReason },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrainRecord {
    pub cavity_id: u32,
    pub entrance_mm: [Finite; 3],
    pub exit_mm: [Finite; 3],
    pub radius_mm: Finite,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CavityRecord {
    pub cavity_id: u32,
    pub volume_ml: Finite,
    pub drain_ids: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HollowReport {
    pub disposition: HollowDisposition,
    pub cavities: Vec<CavityRecord>,
    pub drains: Vec<DrainRecord>,
}

/// Accumulated report state after hollowing; it contains no historical mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct HollowHistory {
    orientation: OrientationHistory,
    hollow: HollowReport,
}

impl HollowHistory {
    /// Builds passive accumulated report state; this does not construct a hollowed model.
    #[must_use]
    pub fn from_reports(orientation: OrientationHistory, hollow: HollowReport) -> Self {
        Self {
            orientation,
            hollow,
        }
    }

    #[must_use]
    pub const fn orientation(&self) -> &OrientationHistory {
        &self.orientation
    }

    #[must_use]
    pub const fn hollow(&self) -> &HollowReport {
        &self.hollow
    }

    #[must_use]
    pub fn into_reports(self) -> (OrientationHistory, HollowReport) {
        (self.orientation, self.hollow)
    }
}

/// Move-only hollowed/drilled model or explicitly retained oriented solid.
#[derive(Debug)]
pub struct HollowedModel {
    mesh: Mesh,
    history: HollowHistory,
}

impl HollowedModel {
    #[must_use]
    pub fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    #[must_use]
    pub const fn history(&self) -> &HollowHistory {
        &self.history
    }

    /// Moves the sole downstream mesh and report history into support planning.
    #[must_use]
    pub fn into_parts(self) -> (Mesh, HollowHistory) {
        (self.mesh, self.history)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum HollowError {
    #[error("cavity extraction failed")]
    CavityExtractionFailed,
    #[error("drain geometry could not be constructed")]
    DrainGeometryFailed,
    #[error("the hollowed or retained-solid production mesh is invalid")]
    InvalidProductionMesh,
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_decimate::{DecimationDisposition, DecimationReport};
    use plinth_orient::{BaselineSelection, OrientationReport, RigidTransform};
    use std::path::PathBuf;

    #[test]
    fn solid_fallback_moves_one_mesh_and_is_visible_in_history() {
        let mesh = Mesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .expect("the test triangle is a valid mesh");
        let allocation = mesh.verts().as_ptr();
        let orientation = OrientationHistory::from_reports(
            DecimationReport {
                disposition: DecimationDisposition::AlreadyPrintScale,
                input_faces: 1,
                output_faces: 1,
                chosen_ratio: 1.0,
                p99_mm: 0.0,
                p99_9_mm: 0.0,
                boundary_edges_before: 3,
                boundary_edges_after: 3,
                nonmanifold_edges_before: 0,
                nonmanifold_edges_after: 0,
            },
            OrientationReport {
                pinned_report: PathBuf::from("baseline/model.pipeline.json"),
                selection: BaselineSelection::Identity,
                transform: RigidTransform::identity(),
            },
        );
        let report = HollowReport {
            disposition: HollowDisposition::SolidFallback {
                reason: SolidFallbackReason::CavityNotDrainable,
            },
            cavities: Vec::new(),
            drains: Vec::new(),
        };
        let model = HollowedModel {
            mesh,
            history: HollowHistory::from_reports(orientation, report),
        };
        let (mesh, history) = model.into_parts();
        assert_eq!(mesh.verts().as_ptr(), allocation);
        assert_eq!(
            history.hollow().disposition,
            HollowDisposition::SolidFallback {
                reason: SolidFallbackReason::CavityNotDrainable
            }
        );
    }
}
