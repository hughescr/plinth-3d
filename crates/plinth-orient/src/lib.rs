//! Data boundary for the pinned-baseline orientation stage.
//!
//! This checkpoint intentionally contains no orientation algorithm or baseline reader. Behavior
//! lands on the parallel `plinth-orient` implementation branch with this signature:
//!
//! `plinth_orient::run(DecimatedModel, &OrientRequest) -> Result<OrientedModel, OrientError>`
//!
//! Dense orientation search is deliberately absent until after Python elimination. The output
//! replaces the production mesh with the transformed mesh and retains only reports, never an
//! untransformed second mesh.
#![forbid(unsafe_code)]

use plinth_core::Finite;
use plinth_decimate::DecimationReport;
use plinth_mesh::Mesh;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// The only orientation policy required on the elimination path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedBaseline {
    pub pipeline_report: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrientRequest {
    pub pinned: PinnedBaseline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaselineAxis {
    X,
    Y,
}

/// The pinned baseline may explicitly select a rotation or implicitly select identity.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BaselineSelection {
    Identity,
    AxisDegrees { axis: BaselineAxis, degrees: Finite },
}

/// The chosen rigid transform, including the required post-rotation re-centering translation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RigidTransform {
    pub rotation: [[Finite; 3]; 3],
    pub translation_mm: [Finite; 3],
}

impl RigidTransform {
    pub fn identity() -> Self {
        let zero = Finite::new(0.0).expect("zero is finite");
        let one = Finite::new(1.0).expect("one is finite");
        Self {
            rotation: [[one, zero, zero], [zero, one, zero], [zero, zero, one]],
            translation_mm: [zero; 3],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrientationReport {
    pub pinned_report: PathBuf,
    pub selection: BaselineSelection,
    pub transform: RigidTransform,
}

/// Accumulated report state after orientation; it contains no historical mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct OrientationHistory {
    decimation: DecimationReport,
    orientation: OrientationReport,
}

impl OrientationHistory {
    /// Builds passive accumulated report state; this does not construct an oriented model.
    #[must_use]
    pub fn from_reports(decimation: DecimationReport, orientation: OrientationReport) -> Self {
        Self {
            decimation,
            orientation,
        }
    }

    #[must_use]
    pub const fn decimation(&self) -> &DecimationReport {
        &self.decimation
    }

    #[must_use]
    pub const fn orientation(&self) -> &OrientationReport {
        &self.orientation
    }

    #[must_use]
    pub fn into_reports(self) -> (DecimationReport, OrientationReport) {
        (self.decimation, self.orientation)
    }
}

/// Move-only oriented production model.
#[derive(Debug)]
pub struct OrientedModel {
    mesh: Mesh,
    history: OrientationHistory,
}

impl OrientedModel {
    #[must_use]
    pub fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    #[must_use]
    pub const fn history(&self) -> &OrientationHistory {
        &self.history
    }

    /// Moves the transformed sole production mesh and report history into hollowing.
    #[must_use]
    pub fn into_parts(self) -> (Mesh, OrientationHistory) {
        (self.mesh, self.history)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum OrientError {
    #[error("cannot read pinned baseline report {path}")]
    PinnedBaselineUnreadable { path: PathBuf },
    #[error("pinned baseline report {path} has an invalid orientation record")]
    PinnedBaselineInvalid { path: PathBuf },
    #[error("the rigid transform produced an invalid production mesh")]
    InvalidTransformedMesh,
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_core::{ContentDigest, NonNegativeFinite, UnitFraction};

    #[test]
    fn consuming_transition_keeps_only_the_transformed_mesh() {
        let mesh = Mesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .expect("the test triangle is a valid mesh");
        let allocation = mesh.verts().as_ptr();
        let decimation = DecimationReport {
            budget_id: ContentDigest(9),
            input_faces: 1,
            output_faces: 1,
            chosen_ratio: UnitFraction::new(1.0).expect("unit fixture ratio"),
            p99_mm: NonNegativeFinite::new(0.0).expect("nonnegative fixture error"),
            p999_mm: NonNegativeFinite::new(0.0).expect("nonnegative fixture error"),
            boundary_edges_before: 3,
            boundary_edges_after: 3,
            nonmanifold_edges_before: 0,
            nonmanifold_edges_after: 0,
        };
        let orientation = OrientationReport {
            pinned_report: PathBuf::from("baseline/model.pipeline.json"),
            selection: BaselineSelection::Identity,
            transform: RigidTransform::identity(),
        };
        let model = OrientedModel {
            mesh,
            history: OrientationHistory::from_reports(decimation, orientation),
        };
        let (mesh, history) = model.into_parts();
        assert_eq!(mesh.verts().as_ptr(), allocation);
        assert_eq!(history.decimation().budget_id, ContentDigest(9));
        assert_eq!(history.orientation().selection, BaselineSelection::Identity);
    }
}
