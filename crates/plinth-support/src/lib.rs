//! Data boundary for current support and raft behavior.
//!
//! This checkpoint intentionally contains no placement, support, or raft algorithm. Behavior
//! lands on the parallel `plinth-support` implementation branch with this signature:
//!
//! `plinth_support::run(HollowedModel, &SupportRequest) -> Result<SupportedModel, SupportError>`
//!
//! Support planning depends on hollowing, never on emission. The scene retains semantic support
//! kinds and f64 geometry; `plinth-emit` alone maps them to ChituBox record identities and f32.
#![forbid(unsafe_code)]

use geom_kernel::{NativeSurface, NativeSurfaceConfig};
use plinth_core::{m0_constants, Finite, PositiveFinite};
use plinth_hollow::HollowHistory;
use plinth_mesh::{build_face_topology, Mesh, WeldPolicy};
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

/// Closed support-domain vocabulary. Wire-format identities belong to `plinth-emit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportSegmentKind {
    ContactSphere,
    ContactTaper,
    JointSphere,
    Member,
    RootFlare,
    RaftPadOrBridge,
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

/// One semantic f64 support/raft primitive.
#[derive(Clone, Debug, PartialEq)]
pub struct SupportPrimitive {
    pub segment_kind: SupportSegmentKind,
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
        let taper_depth_matches = (self.segment_kind == SupportSegmentKind::ContactTaper)
            == self.contact_depth_mm.is_some();
        let ring_provenance_matches = match self.ring_normal {
            FrustumRingNormal::SegmentAxis => true,
            FrustumRingNormal::PlateZ => self.segment_kind == SupportSegmentKind::Member,
        };
        let sphere_poles_match = match (self.segment_kind, &self.sphere_geometry) {
            (SupportSegmentKind::ContactSphere, Some(geometry)) if geometry.is_tip() => {
                self.start_radius_mm == self.end_radius_mm
                    && geometry.has_finite_canonical_fields()
                    && {
                        let (start, end) = geometry.canonical_poles(self.start_radius_mm);
                        self.start_mm == start && self.end_mm == end
                    }
            }
            (SupportSegmentKind::JointSphere, Some(geometry)) if geometry.is_joint() => {
                self.start_radius_mm == self.end_radius_mm
                    && geometry.has_finite_canonical_fields()
                    && {
                        let (start, end) = geometry.canonical_poles(self.start_radius_mm);
                        self.start_mm == start && self.end_mm == end
                    }
            }
            (SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere, _) => false,
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

/// Stable identity of one legal source-face contact seat.
///
/// S0 assigns it directly from the source face id. Later placement can add a
/// local row identity without severing the face provenance this value pins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContactCandidateId(pub u64);

/// A legal, exterior, down-facing contact seat on the one support input mesh.
///
/// `normal_mm` is deliberately the native surface normal for `face_id`, never
/// a vertex normal. `supported_area_mm2` and `overhang_excess` are retained as
/// evidence for the subsequent placement/load stages; S0 makes no placement
/// decision from them.
#[derive(Clone, Debug, PartialEq)]
pub struct ContactCandidate {
    pub id: ContactCandidateId,
    pub face_id: u32,
    pub point_mm: [f64; 3],
    pub normal: [f64; 3],
    pub supported_area_mm2: f64,
    pub overhang_excess: f64,
    pub delicacy: f64,
}

/// Immutable source-face candidate pool in deterministic source-face order.
#[derive(Clone, Debug, PartialEq)]
pub struct ContactCandidatePool {
    candidates: Vec<ContactCandidate>,
}

impl ContactCandidatePool {
    #[must_use]
    pub fn candidates(&self) -> &[ContactCandidate] {
        &self.candidates
    }

    /// Stable FNV-1a digest for thread-count and integration checks.
    #[must_use]
    pub fn digest(&self) -> u64 {
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;
        let mut state = OFFSET;
        for candidate in &self.candidates {
            for byte in candidate.id.0.to_le_bytes() {
                state ^= u64::from(byte);
                state = state.wrapping_mul(PRIME);
            }
            for byte in candidate.face_id.to_le_bytes() {
                state ^= u64::from(byte);
                state = state.wrapping_mul(PRIME);
            }
            for value in candidate
                .point_mm
                .iter()
                .chain(candidate.normal.iter())
                .chain(
                    [
                        candidate.supported_area_mm2,
                        candidate.overhang_excess,
                        candidate.delicacy,
                    ]
                    .iter(),
                )
            {
                for byte in value.to_bits().to_le_bytes() {
                    state ^= u64::from(byte);
                    state = state.wrapping_mul(PRIME);
                }
            }
        }
        state
    }
}

/// Perception inputs whose values are not globally registered constants.
///
/// The three behavioral thresholds are read from `plinth-core` by
/// [`SupportPerceptionConfig::current`]. The column pitch is deliberately a
/// local typed field: current Python fixes it at 1 mm but M0 has no registered
/// authority for it yet, so this crate must not invent a second global owner.
#[derive(Clone, Copy, Debug)]
pub struct SupportPerceptionConfig<'a> {
    pub plate_z_mm: Finite,
    pub column_pitch_mm: PositiveFinite,
    pub surface_grid_pitch_mm: PositiveFinite,
    pub max_grid_bytes: usize,
    /// Required face-order sidecar. S0 transports it; it does not recreate a
    /// competing delicacy implementation.
    pub delicacy_by_face: Option<&'a [f64]>,
    overhang_angle_deg: PositiveFinite,
    bridge_free_gap_mm: PositiveFinite,
    near_plate_mm: PositiveFinite,
}

impl<'a> SupportPerceptionConfig<'a> {
    /// Binds the current perception thresholds to their registered M0 values.
    pub fn current(
        plate_z_mm: Finite,
        column_pitch_mm: PositiveFinite,
        surface_grid_pitch_mm: PositiveFinite,
        max_grid_bytes: usize,
        delicacy_by_face: Option<&'a [f64]>,
    ) -> Result<Self, SupportPerceptionError> {
        let constants = m0_constants().map_err(|error| {
            SupportPerceptionError::RegisteredConstantsUnavailable(error.to_string())
        })?;
        let placeholders = constants.placeholders();
        Ok(Self {
            plate_z_mm,
            column_pitch_mm,
            surface_grid_pitch_mm,
            max_grid_bytes,
            delicacy_by_face,
            overhang_angle_deg: *placeholders.overhang_angle_deg().diagnostic(),
            bridge_free_gap_mm: *placeholders.bridge_free_gap_mm().diagnostic(),
            near_plate_mm: *placeholders.near_plate_mm().diagnostic(),
        })
    }
}

/// Named refusal modes for the S0 perception boundary.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum SupportPerceptionError {
    #[error("support perception requires a face-order delicacy sidecar")]
    MissingDelicacySidecar,
    #[error("delicacy sidecar has {actual} rows; support mesh has {expected} faces")]
    DelicacyLength { expected: usize, actual: usize },
    #[error("delicacy sidecar value {index} is not finite")]
    NonFiniteDelicacy { index: usize },
    #[error("support perception rejected strict mesh geometry: {0}")]
    StrictMesh(String),
    #[error("support perception rejected open or invalid topology at face {face} (cut flags {cut_flags:#x})")]
    OpenOrInvalidTopology { face: usize, cut_flags: u8 },
    #[error("support perception could not build the sole native surface: {0}")]
    NativeSurface(String),
    #[error("support perception column coordinate cannot be represented for face {face}")]
    UnqueryableColumn { face: usize },
    #[error("registered M0 constants are unavailable: {0}")]
    RegisteredConstantsUnavailable(String),
}

/// Perceive legal regular-contact seats on the current hollowed or solid-fallback mesh.
///
/// This leaf is intentionally not a support plan and does not expose `run`.
/// It first validates a closed strict mesh, then constructs exactly one
/// `NativeSurface::from_indexed` authority. Candidate face centroids and their
/// column ray origins are gathered before one native batched ray call and
/// scattered back to source-face order.
pub fn perceive_contacts(
    mesh: &Mesh,
    config: &SupportPerceptionConfig<'_>,
) -> Result<ContactCandidatePool, SupportPerceptionError> {
    mesh.audit_strict()
        .map_err(|error| SupportPerceptionError::StrictMesh(error.to_string()))?;
    let topology = build_face_topology(mesh, WeldPolicy::ExactBits)
        .map_err(|error| SupportPerceptionError::StrictMesh(error.to_string()))?;
    if let Some((face, &cut_flags)) = topology
        .cut_flags()
        .iter()
        .enumerate()
        .find(|(_, flags)| **flags != 0)
    {
        return Err(SupportPerceptionError::OpenOrInvalidTopology { face, cut_flags });
    }

    let delicacy = config
        .delicacy_by_face
        .ok_or(SupportPerceptionError::MissingDelicacySidecar)?;
    if delicacy.len() != mesh.tris().len() {
        return Err(SupportPerceptionError::DelicacyLength {
            expected: mesh.tris().len(),
            actual: delicacy.len(),
        });
    }
    if let Some((index, _)) = delicacy
        .iter()
        .enumerate()
        .find(|(_, value)| !value.is_finite())
    {
        return Err(SupportPerceptionError::NonFiniteDelicacy { index });
    }

    let surface = NativeSurface::from_indexed(
        mesh.verts(),
        mesh.tris(),
        NativeSurfaceConfig {
            grid_pitch_mm: config.surface_grid_pitch_mm.get(),
            max_grid_bytes: config.max_grid_bytes,
        },
    )
    .map_err(|error| SupportPerceptionError::NativeSurface(error.to_string()))?;
    let geometry = surface.geometry();
    let normals = geometry.normals();
    let centroids = geometry.centroids();
    let areas = geometry.areas();
    let exterior = surface.exterior_face_mask();
    let down_limit = -config.overhang_angle_deg.get().to_radians().sin();
    let plate_z = config.plate_z_mm.get();
    let near_plate = config.near_plate_mm.get();

    let candidates: Vec<usize> = (0..surface.nfaces())
        .filter(|&face| {
            normals[face][2] < down_limit && exterior[face] && {
                let plate_gap = centroids[face][2] - plate_z;
                !(plate_gap >= 0.0 && plate_gap <= near_plate)
            }
        })
        .collect();
    if candidates.is_empty() {
        return Ok(ContactCandidatePool {
            candidates: Vec::new(),
        });
    }

    let xmin = candidates
        .iter()
        .map(|&face| centroids[face][0])
        .fold(f64::INFINITY, f64::min);
    let ymin = candidates
        .iter()
        .map(|&face| centroids[face][1])
        .fold(f64::INFINITY, f64::min);
    let pitch = config.column_pitch_mm.get();
    let mut columns: std::collections::BTreeMap<(i64, i64), Vec<usize>> =
        std::collections::BTreeMap::new();
    for &face in &candidates {
        let x = ((centroids[face][0] - xmin) / pitch).floor();
        let y = ((centroids[face][1] - ymin) / pitch).floor();
        if !x.is_finite()
            || !y.is_finite()
            || x < i64::MIN as f64
            || x > i64::MAX as f64
            || y < i64::MIN as f64
            || y > i64::MAX as f64
        {
            return Err(SupportPerceptionError::UnqueryableColumn { face });
        }
        columns.entry((x as i64, y as i64)).or_default().push(face);
    }
    let bounds = geometry.bounds();
    let ztop = bounds[1][2] + 1.0;
    let t_max = ztop - bounds[0][2] + 1.0;
    let origins: Vec<[f64; 3]> = columns
        .keys()
        .map(|&(x, y)| {
            [
                xmin + (x as f64 + 0.5) * pitch,
                ymin + (y as f64 + 0.5) * pitch,
                ztop,
            ]
        })
        .collect();
    let directions = vec![[0.0, 0.0, -1.0]; origins.len()];
    let (offsets, hits, _) = surface
        .cast_rays(&origins, &directions, t_max)
        .map_err(|error| SupportPerceptionError::NativeSurface(error.to_string()))?;

    let mut grounded = vec![false; surface.nfaces()];
    for (column, faces) in columns.values().enumerate() {
        let start = offsets[column] as usize;
        let end = offsets[column + 1] as usize;
        for &face in faces {
            let face_z = centroids[face][2];
            let below = hits[start..end]
                .iter()
                .map(|&t| ztop - t)
                .filter(|&z| z < face_z - 1e-3)
                .fold(f64::NEG_INFINITY, f64::max);
            grounded[face] = below.is_finite() && face_z - below <= config.bridge_free_gap_mm.get();
        }
    }

    let candidates = candidates
        .into_iter()
        .filter(|&face| !grounded[face])
        .map(|face| ContactCandidate {
            id: ContactCandidateId(face as u64),
            face_id: face as u32,
            point_mm: centroids[face],
            normal: normals[face],
            supported_area_mm2: areas[face],
            overhang_excess: -normals[face][2] - down_limit.abs(),
            delicacy: delicacy[face],
        })
        .collect();
    Ok(ContactCandidatePool { candidates })
}

/// One caller-owned support island.  S1 deliberately consumes this sidecar
/// rather than trying to rediscover islands (which belongs to perception).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupportIsland {
    pub id: u32,
    pub face_ids: Vec<u32>,
}

/// The one generic dyadic A3 sample representation used by S1 coverage.
#[derive(Clone, Debug, PartialEq)]
pub struct CoverageSample {
    pub island_id: u32,
    pub face_id: u32,
    pub component_id: u32,
    pub point_mm: [f64; 3],
    pub area_mm2: f64,
}

/// A selected, face-owned S0 contact with the evidence S2 needs to route it.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectedContact {
    pub island_id: u32,
    pub candidate_id: ContactCandidateId,
    pub served_face_id: u32,
    pub point_mm: [f64; 3],
    pub normal: [f64; 3],
    pub delicacy: f64,
    pub supported_area_mm2: f64,
}

/// Immutable S1 hand-off.  This is a placement plan, not a scene or a run API.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectedContacts {
    contacts: Vec<SelectedContact>,
    coverage_samples: Vec<CoverageSample>,
}

impl SelectedContacts {
    #[must_use]
    pub fn contacts(&self) -> &[SelectedContact] {
        &self.contacts
    }

    #[must_use]
    pub fn coverage_samples(&self) -> &[CoverageSample] {
        &self.coverage_samples
    }

    /// Stable FNV-1a digest; selection itself makes no new native query.
    #[must_use]
    pub fn digest(&self) -> u64 {
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;
        let mut state = OFFSET;
        let mut add = |bytes: &[u8]| {
            for &byte in bytes {
                state ^= u64::from(byte);
                state = state.wrapping_mul(PRIME);
            }
        };
        for contact in &self.contacts {
            add(&contact.island_id.to_le_bytes());
            add(&contact.candidate_id.0.to_le_bytes());
            add(&contact.served_face_id.to_le_bytes());
            for value in contact
                .point_mm
                .iter()
                .chain(contact.normal.iter())
                .chain([contact.delicacy, contact.supported_area_mm2].iter())
            {
                add(&value.to_bits().to_le_bytes());
            }
        }
        for sample in &self.coverage_samples {
            add(&sample.island_id.to_le_bytes());
            add(&sample.face_id.to_le_bytes());
            add(&sample.component_id.to_le_bytes());
            for value in sample.point_mm.iter().chain([sample.area_mm2].iter()) {
                add(&value.to_bits().to_le_bytes());
            }
        }
        state
    }
}

/// Current placement policy.  Resource limits remain request fields because
/// M0 records them as non-authoritative guards rather than global constants.
#[derive(Clone, Copy, Debug)]
pub struct ContactSelectionConfig<'a> {
    /// Required S0-delicacy sidecar; it is revalidated at this boundary.
    pub delicacy_by_face: Option<&'a [f64]>,
    /// Explicit integration request for the single dyadic sample builder.
    pub max_coverage_samples: usize,
    /// Explicit integration request for bounded candidate/sample scoring.
    pub max_score_matrix_entries: usize,
    target_spacing_mm: PositiveFinite,
    spacing_floor_mm: PositiveFinite,
    coverage_radius_factor: PositiveFinite,
    voxel_pitch_mm: PositiveFinite,
    min_contacts_per_island: usize,
    coverage_repair_max_attempts: usize,
}

impl<'a> ContactSelectionConfig<'a> {
    /// Binds all result-changing S1 policy values to the M0 registry.
    pub fn current(
        delicacy_by_face: Option<&'a [f64]>,
        max_coverage_samples: usize,
        max_score_matrix_entries: usize,
    ) -> Result<Self, ContactSelectionError> {
        let constants = m0_constants().map_err(|error| {
            ContactSelectionError::RegisteredConstantsUnavailable(error.to_string())
        })?;
        let policy = constants.placeholders();
        Ok(Self {
            delicacy_by_face,
            max_coverage_samples,
            max_score_matrix_entries,
            target_spacing_mm: *policy.target_spacing_mm().diagnostic(),
            spacing_floor_mm: *policy.spacing_floor_mm().diagnostic(),
            coverage_radius_factor: *policy.coverage_radius_factor().diagnostic(),
            voxel_pitch_mm: *policy.voxel_pitch_mm().diagnostic(),
            min_contacts_per_island: usize::try_from(
                policy.min_contacts_per_island().diagnostic().get(),
            )
            .map_err(|_| ContactSelectionError::CountNotRepresentable)?,
            coverage_repair_max_attempts: usize::try_from(
                policy.coverage_repair_max_attempts().diagnostic().get(),
            )
            .map_err(|_| ContactSelectionError::CountNotRepresentable)?,
        })
    }
}

/// Named refusals at the S1 sidecar/coverage boundary.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum ContactSelectionError {
    #[error("contact selection requires a face-order delicacy sidecar")]
    MissingDelicacySidecar,
    #[error(
        "contact-selection delicacy sidecar has {actual} rows; support mesh has {expected} faces"
    )]
    DelicacyLength { expected: usize, actual: usize },
    #[error("contact-selection delicacy sidecar value {index} is not finite")]
    NonFiniteDelicacy { index: usize },
    #[error("contact selection requires at least one support island")]
    MissingIslands,
    #[error("support island {island_id} has no source faces")]
    EmptyIsland { island_id: u32 },
    #[error("support island {island_id} has duplicate or unordered source face {face_id}")]
    NonCanonicalIsland { island_id: u32, face_id: u32 },
    #[error("support island {island_id} names source face {face_id} outside mesh")]
    IslandFaceOutOfRange { island_id: u32, face_id: u32 },
    #[error("S0 candidate {candidate_id:?} has invalid source face {face_id}")]
    CandidateFaceOutOfRange {
        candidate_id: ContactCandidateId,
        face_id: u32,
    },
    #[error("S0 candidate {candidate_id:?} disagrees with required delicacy sidecar")]
    CandidateDelicacyMismatch { candidate_id: ContactCandidateId },
    #[error("contact-selection resource request must be positive")]
    InvalidResourceRequest,
    #[error("dyadic coverage samples exceed requested limit {limit}")]
    CoverageSampleLimit { limit: usize },
    #[error("contact selection could not build face topology: {0}")]
    Topology(String),
    #[error("support island {island_id} has no legal S0 contact candidate")]
    NoLegalCandidate { island_id: u32 },
    #[error("coverage remains unsatisfied for support island {island_id} after {attempts} repair attempts")]
    UnsatisfiedCoverage { island_id: u32, attempts: usize },
    #[error("registered M0 constants are unavailable: {0}")]
    RegisteredConstantsUnavailable(String),
    #[error("registered count cannot be represented on this platform")]
    CountNotRepresentable,
}

/// Select current regular contacts from S0 candidates and prove A3 coverage.
///
/// Frozen call graph: `engine.prepare` builds `placement.candidate_pool` and
/// `coverage.build_coverage_samples`; `engine.run` calls
/// `placement.sample_candidates`, then `verify.coverage_report`, then bounded
/// `_rank_coverage_repair_rows`.  S0 replaces the candidate-pool build here;
/// S1 ports those three production-called selection/coverage decisions.  It
/// intentionally excludes the dead `_add_fix_contacts` and all CCO/forest,
/// brace, rewire, primitive-meshing, and emit paths.
pub fn select_contacts(
    mesh: &Mesh,
    candidates: &ContactCandidatePool,
    islands: &[SupportIsland],
    config: &ContactSelectionConfig<'_>,
) -> Result<SelectedContacts, ContactSelectionError> {
    let delicacy = config
        .delicacy_by_face
        .ok_or(ContactSelectionError::MissingDelicacySidecar)?;
    if delicacy.len() != mesh.tris().len() {
        return Err(ContactSelectionError::DelicacyLength {
            expected: mesh.tris().len(),
            actual: delicacy.len(),
        });
    }
    if let Some((index, _)) = delicacy
        .iter()
        .enumerate()
        .find(|(_, value)| !value.is_finite())
    {
        return Err(ContactSelectionError::NonFiniteDelicacy { index });
    }
    if islands.is_empty() {
        return Err(ContactSelectionError::MissingIslands);
    }
    if config.max_coverage_samples == 0 || config.max_score_matrix_entries == 0 {
        return Err(ContactSelectionError::InvalidResourceRequest);
    }
    for island in islands {
        if island.face_ids.is_empty() {
            return Err(ContactSelectionError::EmptyIsland {
                island_id: island.id,
            });
        }
        let mut previous = None;
        for &face_id in &island.face_ids {
            if usize::try_from(face_id)
                .ok()
                .is_none_or(|face| face >= mesh.tris().len())
            {
                return Err(ContactSelectionError::IslandFaceOutOfRange {
                    island_id: island.id,
                    face_id,
                });
            }
            if previous.is_some_and(|old| old >= face_id) {
                return Err(ContactSelectionError::NonCanonicalIsland {
                    island_id: island.id,
                    face_id,
                });
            }
            previous = Some(face_id);
        }
    }
    for candidate in candidates.candidates() {
        let face = usize::try_from(candidate.face_id).map_err(|_| {
            ContactSelectionError::CandidateFaceOutOfRange {
                candidate_id: candidate.id,
                face_id: candidate.face_id,
            }
        })?;
        if face >= mesh.tris().len() {
            return Err(ContactSelectionError::CandidateFaceOutOfRange {
                candidate_id: candidate.id,
                face_id: candidate.face_id,
            });
        }
        if candidate.delicacy.to_bits() != delicacy[face].to_bits() {
            return Err(ContactSelectionError::CandidateDelicacyMismatch {
                candidate_id: candidate.id,
            });
        }
    }

    let topology = build_face_topology(mesh, WeldPolicy::ExactBits)
        .map_err(|error| ContactSelectionError::Topology(error.to_string()))?;
    let samples = build_dyadic_coverage_samples(mesh, islands, topology.component_ids(), config)?;
    let mut selected: Vec<SelectedContact> = Vec::new();
    for island in islands {
        let mut eligible: Vec<&ContactCandidate> = candidates
            .candidates()
            .iter()
            .filter(|candidate| island.face_ids.binary_search(&candidate.face_id).is_ok())
            .collect();
        if eligible.is_empty() {
            return Err(ContactSelectionError::NoLegalCandidate {
                island_id: island.id,
            });
        }
        // `sample_candidates`: low Z wins; exact geometric ties retain a
        // deterministic least-scar preference before source identity.
        eligible.sort_by(|left, right| {
            left.point_mm[2]
                .total_cmp(&right.point_mm[2])
                .then_with(|| left.delicacy.total_cmp(&right.delicacy))
                .then_with(|| left.point_mm[0].total_cmp(&right.point_mm[0]))
                .then_with(|| left.point_mm[1].total_cmp(&right.point_mm[1]))
                .then_with(|| left.id.cmp(&right.id))
        });
        let start = selected.len();
        for candidate in &eligible {
            if selected[start..].iter().all(|contact| {
                squared_distance(contact.point_mm, candidate.point_mm).sqrt()
                    >= config.target_spacing_mm.get()
            }) {
                selected.push(selected_contact(island.id, candidate));
            }
        }
        // Frozen `_farthest_point_fallback`: first ordered seat, then the
        // greatest minimum distance, ties preserving lower pool/source order.
        while selected.len() - start < config.min_contacts_per_island.min(eligible.len()) {
            // The frozen helper accepts this authority but does not use it in
            // its farthest-point arithmetic; preserving the read documents
            // that S1 did not silently create a second spacing-floor owner.
            debug_assert!(config.spacing_floor_mm.get() > 0.0);
            let best = eligible
                .iter()
                .filter(|candidate| {
                    !selected[start..]
                        .iter()
                        .any(|contact| contact.candidate_id == candidate.id)
                })
                .max_by(|left, right| {
                    minimum_distance_sq(&selected[start..], left.point_mm)
                        .total_cmp(&minimum_distance_sq(&selected[start..], right.point_mm))
                        .then_with(|| right.id.cmp(&left.id))
                })
                .expect("minimum fallback has an unselected legal candidate");
            selected.push(selected_contact(island.id, best));
        }
    }

    for attempt in 0..=config.coverage_repair_max_attempts {
        let failing = failing_coverage_samples(
            &samples,
            &selected,
            &topology,
            config.coverage_radius_factor.get() * config.target_spacing_mm.get(),
            config.voxel_pitch_mm.get(),
        );
        if failing.is_empty() {
            return Ok(SelectedContacts {
                contacts: selected,
                coverage_samples: samples,
            });
        }
        if attempt == config.coverage_repair_max_attempts {
            return Err(ContactSelectionError::UnsatisfiedCoverage {
                island_id: samples[failing[0]].island_id,
                attempts: attempt,
            });
        }
        let repair = best_coverage_repair(
            candidates.candidates(),
            &samples,
            &failing,
            &selected,
            config.coverage_radius_factor.get() * config.target_spacing_mm.get(),
            config.max_score_matrix_entries,
        )
        .ok_or(ContactSelectionError::UnsatisfiedCoverage {
            island_id: samples[failing[0]].island_id,
            attempts: attempt,
        })?;
        selected.push(selected_contact(repair.0, repair.1));
    }
    unreachable!("bounded repair loop returns before exhaustion")
}

fn selected_contact(island_id: u32, candidate: &ContactCandidate) -> SelectedContact {
    SelectedContact {
        island_id,
        candidate_id: candidate.id,
        served_face_id: candidate.face_id,
        point_mm: candidate.point_mm,
        normal: candidate.normal,
        delicacy: candidate.delicacy,
        supported_area_mm2: candidate.supported_area_mm2,
    }
}

fn build_dyadic_coverage_samples(
    mesh: &Mesh,
    islands: &[SupportIsland],
    components: &[u32],
    config: &ContactSelectionConfig<'_>,
) -> Result<Vec<CoverageSample>, ContactSelectionError> {
    let mut samples = Vec::new();
    for island in islands {
        for &face_id in &island.face_ids {
            let triangle = mesh.tris()[face_id as usize];
            let cell = [
                mesh.verts()[triangle[0] as usize],
                mesh.verts()[triangle[1] as usize],
                mesh.verts()[triangle[2] as usize],
            ];
            let max_edge = max_edge_mm(cell);
            let depth = if max_edge <= config.voxel_pitch_mm.get() {
                0
            } else {
                (max_edge / config.voxel_pitch_mm.get()).log2().ceil() as u32
            };
            let count =
                4_usize
                    .checked_pow(depth)
                    .ok_or(ContactSelectionError::CoverageSampleLimit {
                        limit: config.max_coverage_samples,
                    })?;
            if samples
                .len()
                .checked_add(count)
                .is_none_or(|total| total > config.max_coverage_samples)
            {
                return Err(ContactSelectionError::CoverageSampleLimit {
                    limit: config.max_coverage_samples,
                });
            }
            let area = triangle_area_mm2(cell) / count as f64;
            let mut cells = vec![cell];
            for _ in 0..depth {
                let mut next = Vec::with_capacity(cells.len() * 4);
                for [a, b, c] in cells {
                    let ab = midpoint(a, b);
                    let bc = midpoint(b, c);
                    let ca = midpoint(c, a);
                    next.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
                }
                cells = next;
            }
            for [a, b, c] in cells {
                samples.push(CoverageSample {
                    island_id: island.id,
                    face_id,
                    component_id: components[face_id as usize],
                    point_mm: [
                        (a[0] + b[0] + c[0]) / 3.0,
                        (a[1] + b[1] + c[1]) / 3.0,
                        (a[2] + b[2] + c[2]) / 3.0,
                    ],
                    area_mm2: area,
                });
            }
        }
    }
    Ok(samples)
}

fn failing_coverage_samples(
    samples: &[CoverageSample],
    contacts: &[SelectedContact],
    topology: &plinth_mesh::FaceTopology,
    radius_mm: f64,
    area_floor_mm2: f64,
) -> Vec<usize> {
    let mut uncovered_by_face = std::collections::BTreeMap::<(u32, u32), (f64, Vec<usize>)>::new();
    for (sample_index, sample) in samples.iter().enumerate() {
        let covered = contacts.iter().any(|contact| {
            contact.island_id == sample.island_id
                && topology.component_ids()[contact.served_face_id as usize] == sample.component_id
                && squared_distance(contact.point_mm, sample.point_mm).sqrt() <= radius_mm
        });
        if !covered {
            let row = uncovered_by_face
                .entry((sample.island_id, sample.face_id))
                .or_insert((0.0, Vec::new()));
            row.0 += sample.area_mm2;
            row.1.push(sample_index);
        }
    }
    let mut failing = Vec::new();
    let mut visited = std::collections::BTreeSet::new();
    for &(island_id, face_id) in uncovered_by_face.keys() {
        if !visited.insert((island_id, face_id)) {
            continue;
        }
        let mut stack = vec![face_id];
        let mut patch_area = 0.0;
        let mut patch_ambiguous = false;
        let mut patch_samples: Vec<usize> = Vec::new();
        while let Some(face) = stack.pop() {
            let Some((area, rows)) = uncovered_by_face.get(&(island_id, face)) else {
                continue;
            };
            patch_area += area;
            patch_ambiguous |= topology.cut_flags()[face as usize] != 0;
            patch_samples.extend(rows);
            for &neighbor in topology.neighbors(face as usize).unwrap_or(&[]) {
                if uncovered_by_face.contains_key(&(island_id, neighbor))
                    && visited.insert((island_id, neighbor))
                {
                    stack.push(neighbor);
                }
            }
        }
        if patch_area > area_floor_mm2 || patch_ambiguous {
            failing.extend(patch_samples);
        }
    }
    failing.sort_unstable();
    failing
}

fn best_coverage_repair<'a>(
    candidates: &'a [ContactCandidate],
    samples: &[CoverageSample],
    failing: &[usize],
    selected: &[SelectedContact],
    radius_mm: f64,
    matrix_budget: usize,
) -> Option<(u32, &'a ContactCandidate)> {
    let radius_sq = radius_mm * radius_mm;
    let mut best: Option<(f64, u32, &'a ContactCandidate)> = None;
    for candidate in candidates {
        let mut islands = std::collections::BTreeSet::new();
        for &sample_index in failing {
            let sample = &samples[sample_index];
            if sample.face_id == candidate.face_id {
                islands.insert(sample.island_id);
            }
        }
        for island_id in islands {
            if selected.iter().any(|contact| {
                contact.island_id == island_id && contact.candidate_id == candidate.id
            }) {
                continue;
            }
            let mut score = 0.0;
            // A4's score matrix is tiled.  Arithmetic is local over S0/A3
            // evidence; S1 creates no scalar or extra native geometry query.
            for rows in failing.chunks(matrix_budget) {
                for &sample_index in rows {
                    let sample = &samples[sample_index];
                    if sample.island_id == island_id
                        && sample.component_id == sample_component(candidate, samples, island_id)
                        && squared_distance(candidate.point_mm, sample.point_mm) <= radius_sq
                    {
                        score += sample.area_mm2;
                    }
                }
            }
            if score == 0.0 {
                continue;
            }
            let replace = best.is_none_or(|(best_score, best_island, best_candidate)| {
                score > best_score
                    || (score == best_score
                        && (candidate.delicacy < best_candidate.delicacy
                            || (candidate.delicacy == best_candidate.delicacy
                                && (island_id, candidate.id) < (best_island, best_candidate.id))))
            });
            if replace {
                best = Some((score, island_id, candidate));
            }
        }
    }
    best.map(|(_score, island_id, candidate)| (island_id, candidate))
}

fn sample_component(
    candidate: &ContactCandidate,
    samples: &[CoverageSample],
    island_id: u32,
) -> u32 {
    samples
        .iter()
        .find(|sample| sample.island_id == island_id && sample.face_id == candidate.face_id)
        .map_or(u32::MAX, |sample| sample.component_id)
}

fn squared_distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    (left[0] - right[0]).mul_add(
        left[0] - right[0],
        (left[1] - right[1]).mul_add(
            left[1] - right[1],
            (left[2] - right[2]) * (left[2] - right[2]),
        ),
    )
}

fn minimum_distance_sq(contacts: &[SelectedContact], point: [f64; 3]) -> f64 {
    contacts
        .iter()
        .map(|contact| squared_distance(contact.point_mm, point))
        .min_by(f64::total_cmp)
        .unwrap_or(f64::INFINITY)
}

fn midpoint(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        (left[0] + right[0]) * 0.5,
        (left[1] + right[1]) * 0.5,
        (left[2] + right[2]) * 0.5,
    ]
}

fn max_edge_mm([a, b, c]: [[f64; 3]; 3]) -> f64 {
    squared_distance(a, b)
        .max(squared_distance(b, c))
        .max(squared_distance(c, a))
        .sqrt()
}

fn triangle_area_mm2([a, b, c]: [[f64; 3]; 3]) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let cross = [
        ab[1].mul_add(ac[2], -ab[2] * ac[1]),
        ab[2].mul_add(ac[0], -ab[0] * ac[2]),
        ab[0].mul_add(ac[1], -ab[1] * ac[0]),
    ];
    0.5 * squared_distance(cross, [0.0; 3]).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_core::{ContentDigest, NonNegativeFinite, UnitFraction};
    use plinth_decimate::DecimationReport;
    use plinth_hollow::{HollowDisposition, HollowReport};
    use plinth_orient::{BaselineSelection, OrientationHistory, OrientationReport, RigidTransform};
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    fn config<'a>(_mesh: &Mesh, delicacy: &'a [f64]) -> SupportPerceptionConfig<'a> {
        SupportPerceptionConfig::current(
            Finite::new(-5.0).expect("finite fixture plate"),
            PositiveFinite::new(1.0).expect("positive fixture column pitch"),
            PositiveFinite::new(0.5).expect("positive fixture native-grid pitch"),
            usize::MAX,
            Some(delicacy),
        )
        .expect("registered M0 constants are available")
    }

    fn perceive_fixture(mesh: &Mesh) -> ContactCandidatePool {
        let delicacy = vec![0.25; mesh.tris().len()];
        perceive_contacts(mesh, &config(mesh, &delicacy)).expect("closed fixture is perceptible")
    }

    #[test]
    fn fixture_contacts_are_exterior_down_faces_in_source_order() {
        for mesh in [
            plinth_mesh::make_raking_blade().expect("raking blade"),
            plinth_mesh::make_overhang_table().expect("overhang table"),
            plinth_mesh::make_thin_fin().expect("thin fin"),
        ] {
            let pool = perceive_fixture(&mesh);
            assert!(
                !pool.candidates().is_empty(),
                "fixture must retain a real underside contact candidate"
            );
            let mut previous = None;
            for candidate in pool.candidates() {
                assert!(candidate.normal[2] < -45_f64.to_radians().sin());
                assert_eq!(candidate.id.0, u64::from(candidate.face_id));
                assert!(candidate.supported_area_mm2 > 0.0);
                assert!(candidate.overhang_excess > 0.0);
                if let Some(face) = previous {
                    assert!(face < candidate.face_id, "source face order is stable");
                }
                previous = Some(candidate.face_id);
            }
        }
    }

    #[test]
    fn contact_normal_is_the_specific_shared_face_normal_not_vertex_average() {
        let mesh = Mesh::new(
            vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 2.0],
            ],
            vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]],
        )
        .expect("closed tetrahedron");
        let pool = perceive_fixture(&mesh);
        let base = pool
            .candidates()
            .iter()
            .find(|candidate| candidate.face_id == 0)
            .expect("downward base is a candidate");
        assert_eq!(base.normal, [0.0, 0.0, -1.0]);
        assert_ne!(base.normal, [0.0, 0.0, -0.5]);
    }

    #[test]
    fn exact_overhang_threshold_is_excluded_and_steeper_underside_is_included() {
        let prism = |depth: f64| {
            Mesh::new(
                vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 0.0, -depth],
                    [0.0, 1.0, 0.0],
                    [1.0, 1.0, 0.0],
                    [0.0, 1.0, -depth],
                ],
                vec![
                    [0, 2, 1],
                    [3, 4, 5],
                    [0, 1, 4],
                    [0, 4, 3],
                    [0, 3, 5],
                    [0, 5, 2],
                    [1, 2, 5],
                    [1, 5, 4],
                ],
            )
            .expect("closed triangular prism")
        };
        assert!(perceive_fixture(&prism(1.0)).candidates().is_empty());
        let steeper_mesh = prism(0.99);
        let steeper = perceive_fixture(&steeper_mesh);
        assert!(
            steeper
                .candidates()
                .iter()
                .any(|candidate| candidate.normal[2] < -45_f64.to_radians().sin()),
            "{steeper:?}"
        );
    }

    #[test]
    fn near_plate_down_faces_are_excluded() {
        let mesh = plinth_mesh::make_box_default().expect("closed box");
        let delicacy = vec![0.0; mesh.tris().len()];
        let config = SupportPerceptionConfig::current(
            Finite::new(-1.0).expect("finite fixture plate"),
            PositiveFinite::new(1.0).expect("positive fixture column pitch"),
            PositiveFinite::new(0.5).expect("positive fixture native-grid pitch"),
            usize::MAX,
            Some(&delicacy),
        )
        .expect("registered M0 constants are available");
        assert!(perceive_contacts(&mesh, &config)
            .expect("closed box")
            .candidates()
            .is_empty());
    }

    #[test]
    fn strict_open_and_missing_delicacy_inputs_fail_named() {
        let open = Mesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .expect("representable open triangle");
        let delicacy = vec![0.0];
        assert!(matches!(
            perceive_contacts(&open, &config(&open, &delicacy)),
            Err(SupportPerceptionError::OpenOrInvalidTopology { .. })
        ));

        let closed = plinth_mesh::make_box_default().expect("closed box");
        let missing = SupportPerceptionConfig::current(
            Finite::new(-5.0).expect("finite plate"),
            PositiveFinite::new(1.0).expect("positive column pitch"),
            PositiveFinite::new(0.5).expect("positive grid pitch"),
            usize::MAX,
            None,
        )
        .expect("registered constants");
        assert!(matches!(
            perceive_contacts(&closed, &missing),
            Err(SupportPerceptionError::MissingDelicacySidecar)
        ));

        let degenerate = Mesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .expect("representable degenerate mesh");
        let degenerate_delicacy = vec![0.0];
        assert!(matches!(
            perceive_contacts(&degenerate, &config(&degenerate, &degenerate_delicacy)),
            Err(SupportPerceptionError::StrictMesh(_))
        ));
    }

    #[test]
    fn gathered_native_perception_is_thread_count_invariant() {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("test environment lock");
        let previous = std::env::var_os("PLINTH_THREADS");
        let mesh = plinth_mesh::make_overhang_table().expect("overhang table");
        let delicacy = vec![0.75; mesh.tris().len()];
        let mut digest = None;
        for threads in ["1", "4", "16"] {
            std::env::set_var("PLINTH_THREADS", threads);
            let candidate_digest = perceive_contacts(&mesh, &config(&mesh, &delicacy))
                .expect("closed fixture")
                .digest();
            assert_eq!(Some(candidate_digest), digest.or(Some(candidate_digest)));
            digest = Some(candidate_digest);
        }
        if let Some(previous) = previous {
            std::env::set_var("PLINTH_THREADS", previous);
        } else {
            std::env::remove_var("PLINTH_THREADS");
        }
    }

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
            segment_kind: SupportSegmentKind::JointSphere,
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

    fn selection_config<'a>(delicacy: &'a [f64]) -> ContactSelectionConfig<'a> {
        ContactSelectionConfig::current(Some(delicacy), 200_000, 64)
            .expect("registered selection constants")
    }

    fn pool_islands(pool: &ContactCandidatePool) -> Vec<SupportIsland> {
        vec![SupportIsland {
            id: 7,
            face_ids: pool
                .candidates()
                .iter()
                .map(|candidate| candidate.face_id)
                .collect(),
        }]
    }

    fn small_closed_tetra() -> Mesh {
        Mesh::new(
            vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 2.0],
            ],
            vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]],
        )
        .expect("closed tetrahedron")
    }

    #[test]
    fn s1_consumes_s0_fixture_and_retains_face_specific_evidence() {
        let mesh = small_closed_tetra();
        let pool = perceive_fixture(&mesh);
        let delicacy = vec![0.25; mesh.tris().len()];
        let selected = select_contacts(
            &mesh,
            &pool,
            &pool_islands(&pool),
            &selection_config(&delicacy),
        )
        .expect("S0 legal pool can be selected");
        assert!(!selected.contacts().is_empty());
        assert!(!selected.coverage_samples().is_empty());
        for contact in selected.contacts() {
            let candidate = pool
                .candidates()
                .iter()
                .find(|candidate| candidate.id == contact.candidate_id)
                .expect("selection retains an S0 candidate identity");
            assert_eq!(contact.served_face_id, candidate.face_id);
            assert_eq!(
                contact.normal, candidate.normal,
                "never substitute a vertex normal"
            );
            assert_eq!(contact.point_mm, candidate.point_mm);
            assert_eq!(contact.supported_area_mm2, candidate.supported_area_mm2);
        }
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).expect("topology");
        assert!(failing_coverage_samples(
            selected.coverage_samples(),
            selected.contacts(),
            &topology,
            4.0 * 1.5,
            0.4,
        )
        .is_empty());
    }

    #[test]
    fn separated_and_overlapping_islands_remain_logically_owned() {
        let mesh = small_closed_tetra();
        let delicacy = vec![0.25; mesh.tris().len()];
        let pool = ContactCandidatePool {
            candidates: vec![
                ContactCandidate {
                    id: ContactCandidateId(0),
                    face_id: 0,
                    point_mm: [0.0, -0.333_333, 0.0],
                    normal: [0.0, 0.0, -1.0],
                    supported_area_mm2: 2.0,
                    overhang_excess: 1.0,
                    delicacy: 0.25,
                },
                ContactCandidate {
                    id: ContactCandidateId(1),
                    face_id: 1,
                    point_mm: [0.0, -0.666_667, 0.666_667],
                    normal: [0.0, -0.894_427, 0.447_214],
                    supported_area_mm2: 2.236,
                    overhang_excess: 1.0,
                    delicacy: 0.25,
                },
            ],
        };
        let islands = vec![
            SupportIsland {
                id: 10,
                face_ids: vec![0],
            },
            SupportIsland {
                id: 20,
                face_ids: vec![1],
            },
            SupportIsland {
                id: 30,
                face_ids: vec![0],
            },
        ];
        let selected = select_contacts(&mesh, &pool, &islands, &selection_config(&delicacy))
            .expect("each named island has a legal contact");
        for island_id in [10, 20, 30] {
            assert!(selected
                .contacts()
                .iter()
                .any(|contact| contact.island_id == island_id));
        }
        let first = selected
            .contacts()
            .iter()
            .find(|contact| contact.island_id == 10)
            .unwrap();
        let overlap = selected
            .contacts()
            .iter()
            .find(|contact| contact.island_id == 30)
            .unwrap();
        assert_eq!(
            first.candidate_id, overlap.candidate_id,
            "overlap is a logical owner, not a fake new seat"
        );
    }

    #[test]
    fn delicacy_and_exact_id_ties_have_stable_selection_order() {
        let mesh = plinth_mesh::make_overhang_table().expect("table fixture");
        let original = perceive_fixture(&mesh);
        let mut candidates = original.candidates()[..2].to_vec();
        candidates[0].point_mm[2] = 0.0;
        candidates[1].point_mm[2] = 0.0;
        let mut delicacy = vec![0.25; mesh.tris().len()];
        delicacy[candidates[0].face_id as usize] = 0.8;
        delicacy[candidates[1].face_id as usize] = 0.1;
        candidates[0].delicacy = 0.8;
        candidates[1].delicacy = 0.1;
        let pool = ContactCandidatePool { candidates };
        let islands = vec![SupportIsland {
            id: 1,
            face_ids: pool
                .candidates()
                .iter()
                .map(|candidate| candidate.face_id)
                .collect(),
        }];
        let selected = select_contacts(&mesh, &pool, &islands, &selection_config(&delicacy))
            .expect("tie fixture selects");
        assert_eq!(selected.contacts()[0].candidate_id, pool.candidates()[1].id);

        let mut equal = pool.clone();
        for candidate in &mut equal.candidates {
            candidate.delicacy = 0.2;
            delicacy[candidate.face_id as usize] = 0.2;
        }
        let selected = select_contacts(&mesh, &equal, &islands, &selection_config(&delicacy))
            .expect("exact-id tie fixture selects");
        let expected = equal
            .candidates()
            .iter()
            .map(|candidate| candidate.id)
            .min()
            .unwrap();
        assert_eq!(selected.contacts()[0].candidate_id, expected);
    }

    #[test]
    fn unsatisfied_coverage_and_missing_sidecars_fail_named() {
        let mesh = plinth_mesh::make_box_default().expect("box fixture");
        let delicacy = vec![0.0; mesh.tris().len()];
        let pool = ContactCandidatePool {
            candidates: vec![ContactCandidate {
                id: ContactCandidateId(0),
                face_id: 0,
                point_mm: [10_000.0, 10_000.0, 10_000.0],
                normal: [0.0, 0.0, -1.0],
                supported_area_mm2: 1.0,
                overhang_excess: 1.0,
                delicacy: 0.0,
            }],
        };
        let islands = vec![SupportIsland {
            id: 1,
            face_ids: vec![0],
        }];
        assert!(matches!(
            select_contacts(&mesh, &pool, &islands, &selection_config(&delicacy)),
            Err(ContactSelectionError::UnsatisfiedCoverage { .. })
        ));
        let missing = ContactSelectionConfig::current(None, 1, 1).expect("constants");
        assert!(matches!(
            select_contacts(&mesh, &pool, &islands, &missing),
            Err(ContactSelectionError::MissingDelicacySidecar)
        ));
    }

    #[test]
    fn s1_selection_digest_is_thread_setting_invariant_and_adds_no_native_query() {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("environment lock");
        let previous = std::env::var_os("PLINTH_THREADS");
        let mesh = small_closed_tetra();
        let pool = perceive_fixture(&mesh);
        let delicacy = vec![0.25; mesh.tris().len()];
        let islands = pool_islands(&pool);
        let mut digest = None;
        for threads in ["1", "4", "16"] {
            std::env::set_var("PLINTH_THREADS", threads);
            let plan = select_contacts(&mesh, &pool, &islands, &selection_config(&delicacy))
                .expect("selection uses only already-gathered S0 evidence");
            assert_eq!(Some(plan.digest()), digest.or(Some(plan.digest())));
            digest = Some(plan.digest());
        }
        if let Some(previous) = previous {
            std::env::set_var("PLINTH_THREADS", previous);
        } else {
            std::env::remove_var("PLINTH_THREADS");
        }
    }
}
