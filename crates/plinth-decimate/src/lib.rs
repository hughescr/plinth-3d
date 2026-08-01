//! Data boundary for protected production decimation.
//!
//! This checkpoint intentionally contains no decimation algorithm. Behavior lands on the
//! parallel `plinth-decimate` implementation branch. That branch implements the documented
//! entry point:
//!
//! `plinth_decimate::run(&Mesh, &Budget) -> Result<DecimatedModel, DecimateError>`
//!
//! `DecimatedModel` is move-only: downstream stages can borrow its sole production mesh or
//! consume the model to move that mesh into the next stage. There is no public constructor
//! that can bless an undecimated mesh.
//!
//! ```compile_fail
//! use plinth_decimate::DecimatedModel;
//! fn duplicate(model: DecimatedModel) {
//!     let first = model;
//!     let second = model;
//! }
//! ```
#![forbid(unsafe_code)]

use plinth_core::{ContentDigest, NonNegativeFinite, PositiveFinite, UnitFraction};
use plinth_mesh::Mesh;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stable identity and two-sided surface-error limits for one production budget.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub id: ContentDigest,
    pub p99_mm: PositiveFinite,
    pub p999_mm: PositiveFinite,
}

/// Evidence produced by the protected production-decimation stage.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecimationReport {
    pub budget_id: ContentDigest,
    pub input_faces: usize,
    pub output_faces: usize,
    pub chosen_ratio: UnitFraction,
    pub p99_mm: NonNegativeFinite,
    pub p999_mm: NonNegativeFinite,
    pub boundary_edges_before: usize,
    pub boundary_edges_after: usize,
    pub nonmanifold_edges_before: usize,
    pub nonmanifold_edges_after: usize,
}

/// The only value accepted by the orientation stage.
///
/// Its fields are private so only this crate's real `run` implementation can establish the
/// production-decimation invariant.
#[derive(Debug)]
pub struct DecimatedModel {
    mesh: Mesh,
    report: DecimationReport,
}

impl DecimatedModel {
    #[must_use]
    pub fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    #[must_use]
    pub const fn report(&self) -> &DecimationReport {
        &self.report
    }

    /// Moves the sole production mesh and its evidence into orientation.
    #[must_use]
    pub fn into_parts(self) -> (Mesh, DecimationReport) {
        (self.mesh, self.report)
    }
}

/// Semantic failures already fixed by the M3 contract.
#[derive(Clone, Copy, Debug, PartialEq, Error)]
pub enum DecimateError {
    #[error("the model scale is outside the production-decimation contract")]
    ScaleOutsideContract,
    #[error("the production decimation budget is unreachable at ratio 0.50")]
    DecimationBudgetUnreachable,
    #[error("decimation would increase a protected topology defect count")]
    TopologyRegression,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> DecimatedModel {
        let mesh = Mesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .expect("the test triangle is a valid mesh");
        DecimatedModel {
            mesh,
            report: DecimationReport {
                budget_id: ContentDigest(7),
                input_faces: 1,
                output_faces: 1,
                chosen_ratio: UnitFraction::new(1.0).expect("unit fixture ratio"),
                p99_mm: NonNegativeFinite::new(0.0).expect("nonnegative fixture error"),
                p999_mm: NonNegativeFinite::new(0.0).expect("nonnegative fixture error"),
                boundary_edges_before: 3,
                boundary_edges_after: 3,
                nonmanifold_edges_before: 0,
                nonmanifold_edges_after: 0,
            },
        }
    }

    #[test]
    fn consuming_transition_moves_the_same_mesh_allocation() {
        let model = fixture();
        let allocation = model.mesh().verts().as_ptr();
        let (mesh, report) = model.into_parts();
        assert_eq!(mesh.verts().as_ptr(), allocation);
        assert_eq!(report.budget_id, ContentDigest(7));
    }
}
