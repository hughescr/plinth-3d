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

/// Which normal defines capped-frustum end rings. A plate-root column uses
/// horizontal rings even when its centreline is angled; every other rod uses
/// ordinary centreline-normal rings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrustumRingNormal {
    SegmentAxis,
    PlateZ,
}

/// Canonical authority for native sphere records.  The serialized endpoints
/// are a projection of this value, never an alternate geometric input.
#[derive(Clone, Debug, PartialEq)]
pub struct SphereGeometry(SphereGeometryKind);

/// Private so that the only way to create a sphere authority is through its
/// checked constructors.  In particular, safe external Rust cannot smuggle a
/// zero or non-unit contact axis into the emit boundary.
#[derive(Clone, Debug, PartialEq)]
enum SphereGeometryKind {
    Tip {
        center_mm: [Finite; 3],
        outward_axis_unit: [Finite; 3],
    },
    Joint {
        center_mm: [Finite; 3],
    },
}

impl SphereGeometry {
    /// Normalize an arbitrary finite outward direction once at the typed
    /// boundary, yielding the canonical semantic axis for a type-7 contact.
    pub fn tip(center_mm: [Finite; 3], axis: [Finite; 3]) -> Result<Self, SupportError> {
        let axis = axis.map(Finite::get);
        let length_squared = axis.iter().map(|value| value * value).sum::<f64>();
        if !length_squared.is_finite() || length_squared <= 0.0 {
            return Err(SupportError::InvalidPrimitive);
        }
        let length = length_squared.sqrt();
        let outward_axis_unit = axis
            .map(|value| Finite::new(value / length).map_err(|_| SupportError::InvalidPrimitive))
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .map_err(|_| SupportError::InvalidPrimitive)?;
        Ok(Self(SphereGeometryKind::Tip {
            center_mm,
            outward_axis_unit,
        }))
    }

    /// Checked convenience constructor for callers that have not yet crossed
    /// the finite-number boundary.  This deliberately rejects NaN and
    /// infinity rather than allowing them to become geometric authority.
    pub fn tip_from_f64(center_mm: [f64; 3], axis: [f64; 3]) -> Result<Self, SupportError> {
        let center_mm = finite_array(center_mm)?;
        let axis = finite_array(axis)?;
        Self::tip(center_mm, axis)
    }

    #[must_use]
    pub const fn joint(center_mm: [Finite; 3]) -> Self {
        Self(SphereGeometryKind::Joint { center_mm })
    }

    /// Checked f64 convenience constructor matching [`Self::tip_from_f64`].
    pub fn joint_from_f64(center_mm: [f64; 3]) -> Result<Self, SupportError> {
        Ok(Self::joint(finite_array(center_mm)?))
    }

    #[must_use]
    pub fn canonical_poles(&self, radius: PositiveFinite) -> ([Finite; 3], [Finite; 3]) {
        let radius = radius.get();
        match &self.0 {
            SphereGeometryKind::Tip {
                center_mm,
                outward_axis_unit,
            } => {
                let axis = outward_axis_unit.map(Finite::get);
                let center = center_mm.map(Finite::get);
                let start = std::array::from_fn(|index| {
                    Finite::new(center[index] - radius * axis[index])
                        .expect("finite typed sphere pole")
                });
                let end = std::array::from_fn(|index| {
                    Finite::new(center[index] + radius * axis[index])
                        .expect("finite typed sphere pole")
                });
                (start, end)
            }
            SphereGeometryKind::Joint { center_mm } => {
                let center = center_mm.map(Finite::get);
                let start = [
                    Finite::new(center[0]).expect("finite typed joint pole"),
                    Finite::new(center[1]).expect("finite typed joint pole"),
                    Finite::new(center[2] + radius).expect("finite typed joint pole"),
                ];
                let end = [
                    Finite::new(center[0]).expect("finite typed joint pole"),
                    Finite::new(center[1]).expect("finite typed joint pole"),
                    Finite::new(center[2] - radius).expect("finite typed joint pole"),
                ];
                (start, end)
            }
        }
    }

    #[must_use]
    pub fn is_tip(&self) -> bool {
        matches!(&self.0, SphereGeometryKind::Tip { .. })
    }

    #[must_use]
    pub fn is_joint(&self) -> bool {
        matches!(&self.0, SphereGeometryKind::Joint { .. })
    }

    /// Immutable, already-normalized semantic authority for independently
    /// checking the type-7 projection.
    #[must_use]
    pub fn tip_authority(&self) -> Option<([Finite; 3], [Finite; 3])> {
        match &self.0 {
            SphereGeometryKind::Tip {
                center_mm,
                outward_axis_unit,
            } => Some((*center_mm, *outward_axis_unit)),
            SphereGeometryKind::Joint { .. } => None,
        }
    }

    /// Immutable semantic authority for independently checking a type-9
    /// vertical joint sphere.
    #[must_use]
    pub fn joint_center(&self) -> Option<[Finite; 3]> {
        match &self.0 {
            SphereGeometryKind::Tip { .. } => None,
            SphereGeometryKind::Joint { center_mm } => Some(*center_mm),
        }
    }

    fn has_finite_canonical_fields(&self) -> bool {
        let finite = |values: &[Finite; 3]| values.iter().all(|value| value.get().is_finite());
        match &self.0 {
            SphereGeometryKind::Tip {
                center_mm,
                outward_axis_unit,
            } => {
                finite(center_mm)
                    && finite(outward_axis_unit)
                    && outward_axis_unit
                        .map(Finite::get)
                        .iter()
                        .map(|value| value * value)
                        .sum::<f64>()
                        .is_finite()
            }
            SphereGeometryKind::Joint { center_mm } => finite(center_mm),
        }
    }
}

fn finite_array(values: [f64; 3]) -> Result<[Finite; 3], SupportError> {
    values
        .map(|value| Finite::new(value).map_err(|_| SupportError::InvalidPrimitive))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| SupportError::InvalidPrimitive)
}

/// The one physical frame selected while planning supports.  `plate_z_mm` is
/// expressed in the same un-centred model-local coordinates as every support
/// primitive; the emitter centres both together before serialising.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildFrame {
    plate_z_mm: Finite,
    lift_mm: PositiveFinite,
}

impl BuildFrame {
    #[must_use]
    pub const fn new(plate_z_mm: Finite, lift_mm: PositiveFinite) -> Self {
        Self {
            plate_z_mm,
            lift_mm,
        }
    }

    #[must_use]
    pub const fn plate_z_mm(self) -> Finite {
        self.plate_z_mm
    }

    #[must_use]
    pub const fn lift_mm(self) -> PositiveFinite {
        self.lift_mm
    }
}

/// One f64 support/raft primitive. `segment_type` is the canonical ChituBox record identity.
#[derive(Clone, Debug, PartialEq)]
pub struct SupportPrimitive {
    pub segment_type: SegmentType,
    pub start_mm: [Finite; 3],
    pub end_mm: [Finite; 3],
    pub start_radius_mm: PositiveFinite,
    pub end_radius_mm: PositiveFinite,
    /// The support planner's configured contact embed depth.  It exists only
    /// on a type-1 contact taper and is written verbatim into that record's
    /// trailing field by `plinth-emit`; it is never a format default.
    pub contact_depth_mm: Option<PositiveFinite>,
    /// Explicit native geometry provenance for capped-frustum end rings.
    pub ring_normal: FrustumRingNormal,
    /// Required authority for type-7/type-9 sphere records; absent for
    /// frusta and raft records.
    pub sphere_geometry: Option<SphereGeometry>,
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
    frame: BuildFrame,
}

impl SupportHistory {
    /// Builds passive accumulated report state; this does not construct a supported model.
    #[must_use]
    pub fn from_reports(hollow: HollowHistory, support: SupportReport, frame: BuildFrame) -> Self {
        Self {
            hollow,
            support,
            frame,
        }
    }

    #[must_use]
    pub const fn hollow(&self) -> &HollowHistory {
        &self.hollow
    }

    #[must_use]
    pub const fn support(&self) -> &SupportReport {
        &self.support
    }

    /// The sole lift authority shared by support planning and emission.
    #[must_use]
    pub const fn lift_mm(&self) -> PositiveFinite {
        self.frame.lift_mm()
    }

    /// The one plate plane selected by support planning.
    #[must_use]
    pub const fn plate_z_mm(&self) -> Finite {
        self.frame.plate_z_mm()
    }

    #[must_use]
    pub fn into_reports(self) -> (HollowHistory, SupportReport, BuildFrame) {
        (self.hollow, self.support, self.frame)
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
    /// Constructs a scene only when its sole mesh, primitive vector, and
    /// auditable report agree. This is the production hand-off constructor for
    /// support planning and the emitter; no caller can manufacture a scene
    /// with stale counts or an invalid source mesh.
    pub fn from_parts(
        mesh: Mesh,
        primitives: Vec<SupportPrimitive>,
        history: SupportHistory,
    ) -> Result<Self, SupportError> {
        mesh.audit_strict()
            .map_err(|_| SupportError::InvalidModelMesh)?;
        let reported = history
            .support()
            .support_primitive_count
            .checked_add(history.support().raft_primitive_count)
            .ok_or(SupportError::InconsistentPrimitiveCount)?;
        if reported != primitives.len() {
            return Err(SupportError::InconsistentPrimitiveCount);
        }
        if primitives
            .iter()
            .any(|primitive| !primitive.is_semantically_valid())
        {
            return Err(SupportError::InvalidPrimitive);
        }
        Ok(Self {
            mesh,
            primitives,
            history,
        })
    }

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

    /// The one lift value selected by support planning and consumed by emit.
    #[must_use]
    pub const fn lift_mm(&self) -> PositiveFinite {
        self.history.lift_mm()
    }

    /// The sole physical plate plane shared by support planning and emission.
    #[must_use]
    pub const fn plate_z_mm(&self) -> Finite {
        self.history.plate_z_mm()
    }

    /// Moves the sole model mesh, all f64 primitives, and report history together.
    #[must_use]
    pub fn into_parts(self) -> (Mesh, Vec<SupportPrimitive>, SupportHistory) {
        (self.mesh, self.primitives, self.history)
    }
}

impl SupportPrimitive {
    fn is_semantically_valid(&self) -> bool {
        let taper_depth_matches =
            (self.segment_type == SegmentType::TAPER) == self.contact_depth_mm.is_some();
        let ring_provenance_matches = match self.ring_normal {
            FrustumRingNormal::SegmentAxis => true,
            FrustumRingNormal::PlateZ => self.segment_type == SegmentType::JOINER,
        };
        let sphere_poles_match = match (self.segment_type, &self.sphere_geometry) {
            (SegmentType::TIP, Some(geometry)) if geometry.is_tip() => {
                self.start_radius_mm == self.end_radius_mm
                    && geometry.has_finite_canonical_fields()
                    && {
                        let (start, end) = geometry.canonical_poles(self.start_radius_mm);
                        self.start_mm == start && self.end_mm == end
                    }
            }
            (SegmentType::SHAFT, Some(geometry)) if geometry.is_joint() => {
                self.start_radius_mm == self.end_radius_mm
                    && geometry.has_finite_canonical_fields()
                    && {
                        let (start, end) = geometry.canonical_poles(self.start_radius_mm);
                        self.start_mm == start && self.end_mm == end
                    }
            }
            (SegmentType::TIP | SegmentType::SHAFT, _) => false,
            (_, None) => true,
            _ => false,
        };
        taper_depth_matches && ring_provenance_matches && sphere_poles_match
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum SupportError {
    #[error("no legal support and raft plan exists for the model")]
    NoLegalSupportPlan,
    #[error("support planning produced an invalid f64 primitive")]
    InvalidPrimitive,
    #[error("supported model mesh fails the strict production geometry audit")]
    InvalidModelMesh,
    #[error("support report primitive counts do not match the scene primitive vector")]
    InconsistentPrimitiveCount,
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_core::{ContentDigest, NonNegativeFinite, UnitFraction};
    use plinth_decimate::DecimationReport;
    use plinth_hollow::{HollowDisposition, HollowReport};
    use plinth_orient::{BaselineSelection, OrientationHistory, OrientationReport, RigidTransform};
    use std::path::PathBuf;

    #[test]
    fn consuming_scene_transition_moves_model_and_primitives_together() {
        let mesh = Mesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .expect("the test triangle is a valid mesh");
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
        let hollow = HollowHistory::from_reports(
            orientation,
            HollowReport {
                disposition: HollowDisposition::HollowedAndDrilled,
                cavities: Vec::new(),
                drains: Vec::new(),
            },
        );
        let primitives = vec![SupportPrimitive {
            segment_type: SegmentType::SHAFT,
            start_mm: [
                Finite::new(0.0).expect("finite fixture coordinate"),
                Finite::new(0.0).expect("finite fixture coordinate"),
                Finite::new(0.5).expect("finite fixture coordinate"),
            ],
            end_mm: [
                Finite::new(0.0).expect("finite fixture coordinate"),
                Finite::new(0.0).expect("finite fixture coordinate"),
                Finite::new(-0.5).expect("finite fixture coordinate"),
            ],
            start_radius_mm: PositiveFinite::new(0.5).expect("positive fixture radius"),
            end_radius_mm: PositiveFinite::new(0.5).expect("positive fixture radius"),
            contact_depth_mm: None,
            ring_normal: FrustumRingNormal::SegmentAxis,
            sphere_geometry: Some(SphereGeometry::joint([
                Finite::new(0.0).unwrap(),
                Finite::new(0.0).unwrap(),
                Finite::new(0.0).unwrap(),
            ])),
        }];
        let lift_mm = PositiveFinite::new(5.0).expect("positive fixture lift");
        let mismatch = SupportedModel::from_parts(
            mesh.clone(),
            Vec::new(),
            SupportHistory::from_reports(
                hollow.clone(),
                SupportReport {
                    support_primitive_count: 1,
                    raft_primitive_count: 0,
                },
                BuildFrame::new(Finite::new(-5.0).unwrap(), lift_mm),
            ),
        );
        assert!(matches!(
            mismatch,
            Err(SupportError::InconsistentPrimitiveCount)
        ));
        let mut malformed_poles = primitives.clone();
        malformed_poles[0].end_mm[0] = Finite::new(0.25).unwrap();
        let malformed = SupportedModel::from_parts(
            mesh.clone(),
            malformed_poles,
            SupportHistory::from_reports(
                hollow.clone(),
                SupportReport {
                    support_primitive_count: 1,
                    raft_primitive_count: 0,
                },
                BuildFrame::new(Finite::new(-5.0).unwrap(), lift_mm),
            ),
        );
        assert!(matches!(malformed, Err(SupportError::InvalidPrimitive)));
        let mut malformed_normal = primitives.clone();
        malformed_normal[0].ring_normal = FrustumRingNormal::PlateZ;
        let malformed = SupportedModel::from_parts(
            mesh.clone(),
            malformed_normal,
            SupportHistory::from_reports(
                hollow.clone(),
                SupportReport {
                    support_primitive_count: 1,
                    raft_primitive_count: 0,
                },
                BuildFrame::new(Finite::new(-5.0).unwrap(), lift_mm),
            ),
        );
        assert!(matches!(malformed, Err(SupportError::InvalidPrimitive)));
        assert!(matches!(
            SphereGeometry::tip(
                [Finite::new(0.0).unwrap(); 3],
                [Finite::new(0.0).unwrap(); 3],
            ),
            Err(SupportError::InvalidPrimitive)
        ));
        assert!(matches!(
            SphereGeometry::tip_from_f64([0.0; 3], [f64::NAN, 0.0, 1.0]),
            Err(SupportError::InvalidPrimitive)
        ));
        assert!(matches!(
            SphereGeometry::tip_from_f64([0.0; 3], [f64::INFINITY, 0.0, 1.0]),
            Err(SupportError::InvalidPrimitive)
        ));
        let model = SupportedModel::from_parts(
            mesh,
            primitives,
            SupportHistory::from_reports(
                hollow,
                SupportReport {
                    support_primitive_count: 1,
                    raft_primitive_count: 0,
                },
                BuildFrame::new(Finite::new(-5.0).unwrap(), lift_mm),
            ),
        )
        .expect("scene report and primitive vector agree");
        let (mesh, primitives, history) = model.into_parts();
        assert_eq!(mesh.verts().as_ptr(), allocation);
        assert_eq!(primitives.len(), 1);
        assert_eq!(history.support().support_primitive_count, 1);
        assert_eq!(history.lift_mm(), lift_mm);
        assert_eq!(history.plate_z_mm().get(), -5.0);
    }
}
