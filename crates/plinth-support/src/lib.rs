//! Data boundary for current support and raft behavior.
//!
//! This checkpoint intentionally contains no placement, support, or raft algorithm. Behavior
//! lands on the parallel `plinth-support` implementation branch with this signature:
//!
//! `plinth_support::run(HollowedModel, &SupportRequest) -> Result<SupportedModel, SupportError>`
//!
//! Support planning depends on hollowing, never on emission. The scene reuses
//! `chitubox_io::SegmentType` as the canonical segment identity while retaining f64 geometry;
//! `plinth-emit` performs the sole f32 wire conversion later.
#![forbid(unsafe_code)]

use chitubox_io::SegmentType;
use plinth_core::{Finite, PositiveFinite};
use plinth_hollow::HollowHistory;
use plinth_mesh::Mesh;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Request data shared by the support and raft pass.
#[derive(Clone, Debug, PartialEq)]
pub struct SupportRequest {
    /// The single lift value used by support placement and later emission.
    pub lift_mm: PositiveFinite,
}

/// One f64 support/raft primitive. `segment_type` is the canonical ChituBox record identity.
#[derive(Clone, Debug, PartialEq)]
pub struct SupportPrimitive {
    pub segment_type: SegmentType,
    pub start_mm: [Finite; 3],
    pub end_mm: [Finite; 3],
    pub start_radius_mm: PositiveFinite,
    pub end_radius_mm: PositiveFinite,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportReport {
    pub support_primitive_count: usize,
    pub raft_primitive_count: usize,
}

/// Accumulated report state after support planning; it contains no historical mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct SupportHistory {
    hollow: HollowHistory,
    support: SupportReport,
}

impl SupportHistory {
    /// Builds passive accumulated report state; this does not construct a supported model.
    #[must_use]
    pub fn from_reports(hollow: HollowHistory, support: SupportReport) -> Self {
        Self { hollow, support }
    }

    #[must_use]
    pub const fn hollow(&self) -> &HollowHistory {
        &self.hollow
    }

    #[must_use]
    pub const fn support(&self) -> &SupportReport {
        &self.support
    }

    #[must_use]
    pub fn into_reports(self) -> (HollowHistory, SupportReport) {
        (self.hollow, self.support)
    }
}

/// Move-only supported scene with one model mesh plus support and raft primitives.
#[derive(Debug)]
pub struct SupportedModel {
    mesh: Mesh,
    primitives: Vec<SupportPrimitive>,
    history: SupportHistory,
}

impl SupportedModel {
    #[must_use]
    pub fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    #[must_use]
    pub fn primitives(&self) -> &[SupportPrimitive] {
        &self.primitives
    }

    #[must_use]
    pub const fn history(&self) -> &SupportHistory {
        &self.history
    }

    /// Moves the sole model mesh, all f64 primitives, and report history together.
    #[must_use]
    pub fn into_parts(self) -> (Mesh, Vec<SupportPrimitive>, SupportHistory) {
        (self.mesh, self.primitives, self.history)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum SupportError {
    #[error("no legal support and raft plan exists for the model")]
    NoLegalSupportPlan,
    #[error("support planning produced an invalid f64 primitive")]
    InvalidPrimitive,
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_core::{ContentDigest, NonNegativeFinite, UnitFraction};
    use plinth_decimate::DecimationReport;
    use plinth_hollow::{HollowDisposition, HollowedModel, SolidFallbackReason};
    use plinth_orient::{BaselineSelection, OrientationHistory, OrientationReport, RigidTransform};
    use std::path::PathBuf;

    #[test]
    fn consuming_scene_transition_moves_model_and_primitives_together() {
        let mesh = Mesh::new(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        )
        .expect("the test tetrahedron is a valid mesh");
        let allocation = mesh.verts().as_ptr();
        let decimation = DecimationReport {
            budget_id: ContentDigest(13),
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
        let orientation = OrientationHistory::from_reports(
            decimation,
            OrientationReport {
                pinned_report: PathBuf::from("baseline/model.pipeline.json"),
                selection: BaselineSelection::Identity,
                transform: RigidTransform::identity(),
            },
        );
        let hollowed = HollowedModel::solid_fallback(
            mesh,
            orientation,
            SolidFallbackReason::CavityNotDrainable,
        )
        .expect("public solid fallback must enter the support type path");
        let (mesh, hollow) = hollowed.into_parts();
        let primitives = vec![SupportPrimitive {
            segment_type: SegmentType::SHAFT,
            start_mm: [Finite::new(0.0).expect("finite fixture coordinate"); 3],
            end_mm: [Finite::new(1.0).expect("finite fixture coordinate"); 3],
            start_radius_mm: PositiveFinite::new(0.5).expect("positive fixture radius"),
            end_radius_mm: PositiveFinite::new(0.5).expect("positive fixture radius"),
        }];
        let model = SupportedModel {
            mesh,
            primitives,
            history: SupportHistory::from_reports(
                hollow,
                SupportReport {
                    support_primitive_count: 1,
                    raft_primitive_count: 0,
                },
            ),
        };
        let (mesh, primitives, history) = model.into_parts();
        assert_eq!(mesh.verts().as_ptr(), allocation);
        assert_eq!(primitives.len(), 1);
        assert_eq!(history.support().support_primitive_count, 1);
        assert_eq!(
            history.hollow().hollow().disposition,
            HollowDisposition::SolidFallback {
                reason: SolidFallbackReason::CavityNotDrainable
            }
        );
    }
}
