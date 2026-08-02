//! Two-phase, surface-exact model-anchor authority for CCO.
//!
//! Discovery proves an anchor *seat* without claiming a foot radius.  Only
//! after CCO has computed the candidate's canonical final rootward radius does
//! the second phase gather every foot ball into one native breach batch.  This
//! ordering prevents an undersized provisional radius from certifying geometry
//! that the actually emitted support would breach.

use geom_kernel::{indexed_triangle_soup_digest, NativeSurface, SurfaceTriangleSoupDigest};
use plinth_core::{m0_constants, Finite, NonNegativeFinite, PositiveFinite};
use plinth_mesh::{mesh_fingerprint, validate_face_topology, FaceTopology, Mesh};
use thiserror::Error;

#[cfg(test)]
use crate::SelectedContact;
use crate::{
    forest::MemberKey,
    specialized::{PhysicalSeatProvenance, PrimaryRegularIdentity, SpecializedContactPlan},
};

const BELOW_LEVEL_EPS_MM: f64 = 1e-6;
const LEAN_EPS_DEG: f64 = 1e-9;
const RAY_T_MAX: f64 = 1.0;
// Frozen kernel cone defaults from SPEC-RUST-KERNEL. These shape the thickness
// probe; result-changing foot radius/epsilon come from M0 below.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const FOOT_BREACH_HALF_DEG: f64 = 25.0;
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const FOOT_BREACH_CONE_RAYS: usize = 8;

/// One deterministic exterior-surface pool row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SurfaceAnchorSample {
    pub(crate) source_face_id: u32,
    pub(crate) point_mm: [f64; 3],
    pub(crate) delicacy: f64,
}

/// Exact CCO terminal identity and the already-created stub endpoint (`se`).
/// Construction is crate-private: callers outside support planning cannot
/// rebind an arbitrary point to a terminal identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorTerminalBinding {
    terminal_id: u32,
    stub: MemberKey,
    stub_end_mm: [Finite; 3],
}

impl AnchorTerminalBinding {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn new(terminal_id: u32, stub: MemberKey, stub_end_mm: [Finite; 3]) -> Self {
        Self {
            terminal_id,
            stub,
            stub_end_mm,
        }
    }

    #[must_use]
    pub const fn terminal_id(self) -> u32 {
        self.terminal_id
    }

    #[must_use]
    pub const fn stub(self) -> MemberKey {
        self.stub
    }

    #[must_use]
    pub const fn stub_end_mm(self) -> [Finite; 3] {
        self.stub_end_mm
    }
}

/// Exact immutable rewire checkpoint identity. Construction is crate-private;
/// CCO creates it only from a validated `RewireCcoBinding`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RewireAnchorBinding {
    root: MemberKey,
    live_digest: u64,
    snapshot_digest: u64,
    attach_point_mm: [Finite; 3],
}

impl RewireAnchorBinding {
    pub(crate) const fn new(
        root: MemberKey,
        live_digest: u64,
        snapshot_digest: u64,
        attach_point_mm: [Finite; 3],
    ) -> Self {
        Self {
            root,
            live_digest,
            snapshot_digest,
            attach_point_mm,
        }
    }
}

/// Production anchor policy. Every result-changing scalar is bound to M0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorSearchConfig {
    min_length_mm: PositiveFinite,
    max_length_mm: PositiveFinite,
    max_lean_deg: PositiveFinite,
    max_delicacy: f64,
    candidate_cap: usize,
    rootward_clearance_inset_mm: NonNegativeFinite,
    ball_factor_foot: PositiveFinite,
    ball_breach_eps_mm: PositiveFinite,
}

impl AnchorSearchConfig {
    pub fn current() -> Result<Self, AnchorSearchError> {
        let constants = m0_constants().map_err(|error| {
            AnchorSearchError::RegisteredConstantsUnavailable(error.to_string())
        })?;
        let policy = constants.placeholders();
        Ok(Self {
            min_length_mm: *policy.strut_model_anchor_min_len_mm().diagnostic(),
            max_length_mm: *policy.strut_max_length_mm().diagnostic(),
            max_lean_deg: *policy.strut_fallback_max_angle_deg().diagnostic(),
            max_delicacy: policy.strut_model_anchor_max_delicacy().diagnostic().get(),
            candidate_cap: usize::try_from(
                policy
                    .strut_model_anchor_max_candidates()
                    .diagnostic()
                    .get(),
            )
            .map_err(|_| AnchorSearchError::CountNotRepresentable)?,
            rootward_clearance_inset_mm: NonNegativeFinite::new(
                policy.standoff_mm().diagnostic().get(),
            )
            .map_err(|_| {
                AnchorSearchError::RegisteredConstantsUnavailable("invalid standoff".into())
            })?,
            ball_factor_foot: *policy.ball_factor_foot().diagnostic(),
            ball_breach_eps_mm: *policy.ball_breach_eps_mm().diagnostic(),
        })
    }

    fn fingerprint(self) -> ConfigFingerprint {
        ConfigFingerprint {
            scalar_bits: [
                self.min_length_mm.get().to_bits(),
                self.max_length_mm.get().to_bits(),
                self.max_lean_deg.get().to_bits(),
                self.max_delicacy.to_bits(),
                self.rootward_clearance_inset_mm.get().to_bits(),
                self.ball_factor_foot.get().to_bits(),
                self.ball_breach_eps_mm.get().to_bits(),
            ],
            candidate_cap: self.candidate_cap,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GeometryFingerprint {
    mesh_sha256: String,
    topology_sha256: String,
    native_triangle_soup_digest: SurfaceTriangleSoupDigest,
    // Grid pitch changes native query results. `max_grid_bytes` is only a
    // construction refusal cap and is result-inert once a surface exists.
    native_grid_pitch_bits: u64,
}

/// One checked geometry authority shared by both phases. It binds the native
/// query engine to the exact indexed coordinates and topology sidecar.
///
/// Authority does not come from treating `SurfaceTriangleSoupDigest` as a
/// capability: callers can reproducibly compute that content identity. It
/// comes from comparing the mesh-computed digest with the immutable digest
/// retained by `NativeSurface`, binding the mesh/topology/config fingerprint,
/// and retaining the exact surface reference for pointer-identity rebound
/// checks. Callers cannot overwrite the digest stored by a native surface.
pub struct AnchorGeometryAuthority<'a> {
    surface: &'a NativeSurface,
    fingerprint: GeometryFingerprint,
}

impl<'a> AnchorGeometryAuthority<'a> {
    pub fn new(
        mesh: &Mesh,
        topology: &FaceTopology,
        surface: &'a NativeSurface,
    ) -> Result<Self, AnchorSearchError> {
        validate_face_topology(topology)
            .map_err(|error| AnchorSearchError::InvalidTopology(error.to_string()))?;
        let actual_mesh = mesh_fingerprint(mesh)
            .map_err(|error| AnchorSearchError::InvalidTopology(error.to_string()))?;
        if topology.mesh_fingerprint() != actual_mesh {
            return Err(AnchorSearchError::TopologyMeshMismatch);
        }
        let native_triangle_soup_digest = validate_surface_matches_mesh(mesh, surface)?;
        Ok(Self {
            surface,
            fingerprint: GeometryFingerprint {
                mesh_sha256: actual_mesh,
                topology_sha256: topology.content_digest().to_owned(),
                native_triangle_soup_digest,
                native_grid_pitch_bits: surface.grid_pitch_mm.to_bits(),
            },
        })
    }

    #[must_use]
    pub const fn surface(&self) -> &NativeSurface {
        self.surface
    }

    #[must_use]
    pub(crate) fn fingerprint(&self) -> GeometryFingerprint {
        self.fingerprint.clone()
    }

    /// Revalidate the exact mesh/native pairing before a downstream stage
    /// stages or allocates result-shaped work.
    pub(crate) fn validate_mesh(&self, mesh: &Mesh) -> Result<(), AnchorSearchError> {
        let actual_mesh = mesh_fingerprint(mesh)
            .map_err(|error| AnchorSearchError::InvalidTopology(error.to_string()))?;
        if actual_mesh != self.fingerprint.mesh_sha256 {
            return Err(AnchorSearchError::TopologyMeshMismatch);
        }
        validate_surface_matches_mesh(mesh, self.surface).map(|_| ())
    }

    /// Stable binding for the validated indexed mesh, topology, and native
    /// grid pitch. No native surface may be substituted after construction.
    pub(crate) fn digest(&self) -> u64 {
        let mut hash = AnchorFnv::new();
        hash.bytes(self.fingerprint.mesh_sha256.as_bytes());
        hash.bytes(self.fingerprint.topology_sha256.as_bytes());
        hash.bytes(self.fingerprint.native_triangle_soup_digest.as_bytes());
        hash.u64(self.fingerprint.native_grid_pitch_bits);
        hash.finish()
    }
}

/// Opaque, native-surface-bound exterior centroid authority. Production
/// discovery can only draw samples from this pool; arbitrary sample rows are
/// deliberately crate-private test/legacy compatibility data.
#[allow(
    dead_code,
    reason = "the pending controller consumes this bounded authority seam"
)]
pub(crate) struct SurfaceAnchorPool<'a> {
    surface: &'a NativeSurface,
    geometry: GeometryFingerprint,
    digest: u64,
    rows: Vec<SurfaceAnchorSample>,
    delicacy_by_face: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceAnchorPoolLimits {
    pub(crate) max_rows: usize,
    pub(crate) max_bytes: usize,
}

impl SurfaceAnchorPoolLimits {
    const fn production() -> Self {
        Self {
            max_rows: 20_000_000,
            max_bytes: 512 * 1024 * 1024,
        }
    }
}

#[allow(
    dead_code,
    reason = "the pending controller consumes this bounded authority seam"
)]
impl<'a> SurfaceAnchorPool<'a> {
    pub(crate) fn from_geometry(
        geometry: &'a AnchorGeometryAuthority<'a>,
        delicacy_by_face: &[f64],
    ) -> Result<Self, AnchorSearchError> {
        Self::from_geometry_with_limits(
            geometry,
            delicacy_by_face,
            SurfaceAnchorPoolLimits::production(),
        )
    }

    pub(crate) fn from_geometry_with_limits(
        geometry: &'a AnchorGeometryAuthority<'a>,
        delicacy_by_face: &[f64],
        limits: SurfaceAnchorPoolLimits,
    ) -> Result<Self, AnchorSearchError> {
        validate_sidecar(Some(delicacy_by_face), geometry.surface())?;
        let exterior = geometry.surface().exterior_face_mask();
        let row_count = exterior.iter().filter(|&&value| value).count();
        if row_count > limits.max_rows {
            return Err(AnchorSearchError::SurfaceAnchorPoolRowLimit {
                required: row_count,
                limit: limits.max_rows,
            });
        }
        let required_bytes = surface_anchor_pool_required_bytes(
            &geometry.fingerprint,
            delicacy_by_face.len(),
            row_count,
        )?;
        if required_bytes > limits.max_bytes {
            return Err(AnchorSearchError::SurfaceAnchorPoolByteLimit {
                required: required_bytes,
                limit: limits.max_bytes,
            });
        }
        let owned_geometry = try_clone_geometry_fingerprint(&geometry.fingerprint, required_bytes)?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(row_count).map_err(|_| {
            AnchorSearchError::SurfaceAnchorPoolAllocation {
                required: required_bytes,
            }
        })?;
        let mut owned_delicacy = Vec::new();
        owned_delicacy
            .try_reserve_exact(delicacy_by_face.len())
            .map_err(|_| AnchorSearchError::SurfaceAnchorPoolAllocation {
                required: required_bytes,
            })?;
        owned_delicacy.extend_from_slice(delicacy_by_face);
        for (face_id, &is_exterior) in exterior.iter().enumerate() {
            if is_exterior {
                rows.push(SurfaceAnchorSample {
                    source_face_id: u32::try_from(face_id)
                        .map_err(|_| AnchorSearchError::FaceIdNotRepresentable)?,
                    point_mm: geometry.surface().geometry().centroids()[face_id],
                    delicacy: delicacy_by_face[face_id],
                });
            }
        }
        let digest = surface_anchor_pool_digest(&geometry.fingerprint, delicacy_by_face, &rows);
        Ok(Self {
            surface: geometry.surface(),
            geometry: owned_geometry,
            digest,
            rows,
            delicacy_by_face: owned_delicacy,
        })
    }

    #[must_use]
    pub(crate) const fn digest(&self) -> u64 {
        self.digest
    }

    #[must_use]
    pub(crate) fn rows(&self) -> &[SurfaceAnchorSample] {
        &self.rows
    }

    pub(crate) fn validate_against(
        &self,
        geometry: &AnchorGeometryAuthority<'_>,
    ) -> Result<(), AnchorSearchError> {
        if !std::ptr::eq(self.surface, geometry.surface()) || self.geometry != geometry.fingerprint
        {
            return Err(AnchorSearchError::StaleSurfaceAnchorPool);
        }
        if self.digest
            != surface_anchor_pool_digest(&self.geometry, &self.delicacy_by_face, &self.rows)
        {
            return Err(AnchorSearchError::StaleSurfaceAnchorPool);
        }
        Ok(())
    }
}

fn surface_anchor_pool_required_bytes(
    geometry: &GeometryFingerprint,
    sidecar_len: usize,
    row_count: usize,
) -> Result<usize, AnchorSearchError> {
    let sidecar_bytes = sidecar_len
        .checked_mul(std::mem::size_of::<f64>())
        .ok_or(AnchorSearchError::SurfaceAnchorPoolResourceOverflow)?;
    let row_bytes = row_count
        .checked_mul(std::mem::size_of::<SurfaceAnchorSample>())
        .ok_or(AnchorSearchError::SurfaceAnchorPoolResourceOverflow)?;
    geometry
        .mesh_sha256
        .len()
        .checked_add(geometry.topology_sha256.len())
        .and_then(|bytes| {
            bytes.checked_add(std::mem::size_of_val(&geometry.native_triangle_soup_digest))
        })
        .and_then(|bytes| bytes.checked_add(sidecar_bytes))
        .and_then(|bytes| bytes.checked_add(row_bytes))
        .ok_or(AnchorSearchError::SurfaceAnchorPoolResourceOverflow)
}

fn try_clone_geometry_fingerprint(
    geometry: &GeometryFingerprint,
    required_bytes: usize,
) -> Result<GeometryFingerprint, AnchorSearchError> {
    let mut mesh_sha256 = String::new();
    mesh_sha256
        .try_reserve_exact(geometry.mesh_sha256.len())
        .map_err(|_| AnchorSearchError::SurfaceAnchorPoolAllocation {
            required: required_bytes,
        })?;
    mesh_sha256.push_str(&geometry.mesh_sha256);
    let mut topology_sha256 = String::new();
    topology_sha256
        .try_reserve_exact(geometry.topology_sha256.len())
        .map_err(|_| AnchorSearchError::SurfaceAnchorPoolAllocation {
            required: required_bytes,
        })?;
    topology_sha256.push_str(&geometry.topology_sha256);
    Ok(GeometryFingerprint {
        mesh_sha256,
        topology_sha256,
        native_triangle_soup_digest: geometry.native_triangle_soup_digest,
        native_grid_pitch_bits: geometry.native_grid_pitch_bits,
    })
}

#[allow(
    dead_code,
    reason = "the pending controller consumes this bounded authority seam"
)]
fn surface_anchor_pool_digest(
    geometry: &GeometryFingerprint,
    delicacy_by_face: &[f64],
    rows: &[SurfaceAnchorSample],
) -> u64 {
    let mut hash = AnchorFnv::new();
    hash.bytes(geometry.mesh_sha256.as_bytes());
    hash.bytes(geometry.topology_sha256.as_bytes());
    hash.bytes(geometry.native_triangle_soup_digest.as_bytes());
    hash.u64(geometry.native_grid_pitch_bits);
    hash.usize(delicacy_by_face.len());
    for &delicacy in delicacy_by_face {
        hash.u64(delicacy.to_bits());
    }
    hash.usize(rows.len());
    for row in rows {
        hash.u32(row.source_face_id);
        for value in row.point_mm {
            hash.u64(value.to_bits());
        }
        hash.u64(row.delicacy.to_bits());
    }
    hash.finish()
}

#[allow(
    dead_code,
    reason = "the pending controller consumes this bounded authority seam"
)]
struct AnchorFnv(u64);

impl AnchorFnv {
    const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    fn bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }
    fn usize(&mut self, value: usize) {
        self.u64(value as u64);
    }
    const fn finish(self) -> u64 {
        self.0
    }
}

/// A finalized contact plus the carried-area seam supplied by the authoritative
/// load stage. This type does not calculate load; it only binds it.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(
    dead_code,
    reason = "the pending controller consumes this finalized authority seam"
)]
pub(crate) struct BoundPhysicalAnchorContact {
    fingerprint: FinalizedPhysicalContactFingerprint,
    regular_identity: Option<PrimaryRegularIdentity>,
}

#[allow(
    dead_code,
    reason = "the pending controller consumes this finalized authority seam"
)]
impl BoundPhysicalAnchorContact {
    pub(crate) fn new(
        plan: &SpecializedContactPlan,
        contact_id: u64,
    ) -> Result<Self, AnchorSearchError> {
        let contact = plan
            .contacts()
            .iter()
            .find(|contact| contact.contact_id() == contact_id)
            .ok_or(AnchorSearchError::FinalizedContactNotFound { contact_id })?;
        let provenance = contact.seat_provenance();
        Ok(Self {
            fingerprint: FinalizedPhysicalContactFingerprint::from_physical(
                plan.digest(),
                contact_id,
                provenance,
                contact.position_mm(),
                contact.normal(),
                contact.carried_area_mm2(),
            ),
            regular_identity: provenance.regular_identity(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConfigFingerprint {
    scalar_bits: [u64; 7],
    candidate_cap: usize,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ContactFingerprint {
    island_id: u32,
    candidate_id: u64,
    served_face_id: u32,
    point_bits: [u64; 3],
    normal_bits: [u64; 3],
    delicacy_bits: u64,
    supported_area_bits: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "the pending controller consumes this finalized authority seam"
)]
struct FinalizedPhysicalContactFingerprint {
    plan_digest: u64,
    contact_id: u64,
    provenance: PhysicalSeatFingerprint,
    point_bits: [u64; 3],
    normal_bits: [u64; 3],
    carried_area_bits: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "the pending controller consumes this finalized authority seam"
)]
enum PhysicalSeatFingerprint {
    Regular {
        candidate_id: u64,
        owner_island_id: u32,
        face_id: u32,
        component_id: u32,
        position_bits: [u64; 3],
        normal_bits: [u64; 3],
    },
    Specialized {
        kind: u8,
        owner_island_id: u32,
        face_id: u32,
        component_id: u32,
        nominated_position_bits: [u64; 3],
        nominated_normal_bits: [u64; 3],
    },
}

#[allow(
    dead_code,
    reason = "the pending controller consumes this finalized authority seam"
)]
impl FinalizedPhysicalContactFingerprint {
    fn from_physical(
        plan_digest: u64,
        contact_id: u64,
        provenance: PhysicalSeatProvenance,
        position_mm: [f64; 3],
        normal: [f64; 3],
        carried_area_mm2: f64,
    ) -> Self {
        let provenance = match provenance {
            PhysicalSeatProvenance::Regular(primary) => {
                let route = primary.route();
                PhysicalSeatFingerprint::Regular {
                    candidate_id: route.candidate_id(),
                    owner_island_id: route.owner_island_id(),
                    face_id: route.coverage_face_id(),
                    component_id: route.component_id(),
                    position_bits: bits3(primary.position_mm()),
                    normal_bits: bits3(primary.normal()),
                }
            }
            PhysicalSeatProvenance::Specialized(representative) => {
                PhysicalSeatFingerprint::Specialized {
                    kind: match representative.kind() {
                        crate::specialized::SpecializedDemandKind::Minimum => 0,
                        crate::specialized::SpecializedDemandKind::IslandBirth => 1,
                    },
                    owner_island_id: representative.owner_island_id(),
                    face_id: representative.coverage_face_id(),
                    component_id: representative.coverage_component_id(),
                    nominated_position_bits: bits3(representative.nominated_position_mm()),
                    nominated_normal_bits: bits3(representative.nominated_normal()),
                }
            }
        };
        Self {
            plan_digest,
            contact_id,
            provenance,
            point_bits: bits3(position_mm),
            normal_bits: bits3(normal),
            carried_area_bits: carried_area_mm2.to_bits(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "the pending controller consumes this finalized authority seam"
)]
enum AnchorContactFingerprint {
    #[cfg(test)]
    Selected(ContactFingerprint),
    Finalized(FinalizedPhysicalContactFingerprint),
}

#[cfg(test)]
impl ContactFingerprint {
    fn from_contact(contact: &SelectedContact) -> Self {
        Self {
            island_id: contact.island_id,
            candidate_id: contact.candidate_id.0,
            served_face_id: contact.served_face_id,
            point_bits: bits3(contact.point_mm),
            normal_bits: bits3(contact.normal),
            delicacy_bits: contact.delicacy.to_bits(),
            supported_area_bits: contact.supported_area_mm2.to_bits(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalFingerprint {
    terminal_id: u32,
    stub: MemberKey,
    stub_end_bits: [u64; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RewireFingerprint {
    root: MemberKey,
    live_digest: u64,
    snapshot_digest: u64,
    attach_point_bits: [u64; 3],
}

impl RewireFingerprint {
    fn from_binding(binding: RewireAnchorBinding) -> Self {
        Self {
            root: binding.root,
            live_digest: binding.live_digest,
            snapshot_digest: binding.snapshot_digest,
            attach_point_bits: binding.attach_point_mm.map(|value| value.get().to_bits()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnchorSubjectFingerprint {
    Terminal(TerminalFingerprint),
    Rewire(RewireFingerprint),
}

impl TerminalFingerprint {
    fn from_binding(binding: AnchorTerminalBinding) -> Self {
        Self {
            terminal_id: binding.terminal_id,
            stub: binding.stub,
            stub_end_bits: binding.stub_end_mm.map(|value| value.get().to_bits()),
        }
    }
}

/// Stable identity retained by both successful and refused candidates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorCandidateProvenance {
    regular_identity: Option<PrimaryRegularIdentity>,
    physical_contact_id: Option<u64>,
    sample_index: usize,
    source_sample_face_id: u32,
}

impl AnchorCandidateProvenance {
    #[must_use]
    pub const fn regular_identity(self) -> Option<PrimaryRegularIdentity> {
        self.regular_identity
    }
    #[must_use]
    pub const fn physical_contact_id(self) -> Option<u64> {
        self.physical_contact_id
    }
    #[must_use]
    pub const fn sample_index(self) -> usize {
        self.sample_index
    }
    #[must_use]
    pub const fn source_sample_face_id(self) -> u32 {
        self.source_sample_face_id
    }
}

/// Surface-exact but deliberately unsized anchor seat.
///
/// ```compile_fail
/// use plinth_support::anchor::QualifiedAnchorSeat;
/// let forged = QualifiedAnchorSeat {};
/// ```
#[derive(Debug)]
pub struct QualifiedAnchorSeat {
    provenance: AnchorCandidateProvenance,
    #[cfg_attr(not(test), allow(dead_code))]
    contact: Option<AnchorContactFingerprint>,
    #[cfg_attr(not(test), allow(dead_code))]
    subject: AnchorSubjectFingerprint,
    #[cfg_attr(not(test), allow(dead_code))]
    geometry: GeometryFingerprint,
    #[cfg_attr(not(test), allow(dead_code))]
    config: ConfigFingerprint,
    #[cfg_attr(not(test), allow(dead_code))]
    pool_digest: Option<u64>,
    snapped_point_mm: [Finite; 3],
    snapped_normal: [Finite; 3],
    snapped_face_id: u32,
    delicacy: Finite,
    length_mm: PositiveFinite,
    lean_deg: NonNegativeFinite,
    first_hit_t: Finite,
    rootward_clearance_inset_mm: NonNegativeFinite,
}

impl QualifiedAnchorSeat {
    #[must_use]
    pub const fn provenance(&self) -> AnchorCandidateProvenance {
        self.provenance
    }
    #[must_use]
    pub const fn point_mm(&self) -> &[Finite; 3] {
        &self.snapped_point_mm
    }
    #[must_use]
    pub const fn normal(&self) -> &[Finite; 3] {
        &self.snapped_normal
    }
    #[must_use]
    pub const fn snapped_face_id(&self) -> u32 {
        self.snapped_face_id
    }
    #[must_use]
    pub const fn delicacy(&self) -> Finite {
        self.delicacy
    }
    #[must_use]
    pub const fn length_mm(&self) -> PositiveFinite {
        self.length_mm
    }
    #[must_use]
    pub const fn lean_deg(&self) -> NonNegativeFinite {
        self.lean_deg
    }
    #[must_use]
    pub const fn first_hit_t(&self) -> Finite {
        self.first_hit_t
    }
    #[must_use]
    pub const fn rootward_clearance_inset_mm(&self) -> NonNegativeFinite {
        self.rootward_clearance_inset_mm
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorSeatRefusalReason {
    SampleNonFinite,
    SampleFaceOutOfRange,
    SampleDelicacyMismatch,
    AboveContact,
    TooShort,
    TooLong,
    TooDelicate,
    TooLean,
    NoFirstHit,
    NearerNonExteriorObstruction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnchorSeatRefusal {
    pub sample_index: usize,
    pub source_sample_face_id: u32,
    pub reason: AnchorSeatRefusalReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoAnchorSeatReason {
    NoQualifiedSamples,
    NoExteriorFirstHit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnchorSeatQueryCounts {
    pub ray_batches: usize,
}

#[derive(Debug)]
pub struct AnchorSeatDiscovery {
    seats: Vec<QualifiedAnchorSeat>,
    refusals: Vec<AnchorSeatRefusal>,
    no_seat_reason: Option<NoAnchorSeatReason>,
    query_counts: AnchorSeatQueryCounts,
}

impl AnchorSeatDiscovery {
    #[must_use]
    pub fn seats(&self) -> &[QualifiedAnchorSeat] {
        &self.seats
    }
    #[must_use]
    pub fn into_seats(self) -> Vec<QualifiedAnchorSeat> {
        self.seats
    }
    #[must_use]
    pub fn refusals(&self) -> &[AnchorSeatRefusal] {
        &self.refusals
    }
    #[must_use]
    pub const fn no_seat_reason(&self) -> Option<NoAnchorSeatReason> {
        self.no_seat_reason
    }
    #[must_use]
    pub const fn query_counts(&self) -> AnchorSeatQueryCounts {
        self.query_counts
    }
}

/// Direct port of frozen qualification plus one gathered first-hit ray batch.
#[cfg(test)]
pub(crate) fn discover_anchor_seats(
    contact: &SelectedContact,
    terminal: AnchorTerminalBinding,
    samples: &[SurfaceAnchorSample],
    delicacy_by_face: Option<&[f64]>,
    config: AnchorSearchConfig,
    geometry: &AnchorGeometryAuthority<'_>,
) -> Result<AnchorSeatDiscovery, AnchorSearchError> {
    validate_contact_and_sidecar(contact, delicacy_by_face, geometry.surface)?;
    discover_anchor_seats_bound(
        terminal.stub_end_mm,
        Some(AnchorContactFingerprint::Selected(
            ContactFingerprint::from_contact(contact),
        )),
        AnchorSubjectFingerprint::Terminal(TerminalFingerprint::from_binding(terminal)),
        None,
        None,
        samples,
        delicacy_by_face,
        config,
        geometry,
        None,
    )
}

/// Rewire-specific discovery. The resulting move-only seats are bound to the
/// exact live digest, prepared checkpoint digest, generational root, attach
/// point, geometry authority, and current anchor policy.
pub(crate) fn discover_rewire_anchor_seats(
    binding: RewireAnchorBinding,
    samples: &[SurfaceAnchorSample],
    delicacy_by_face: Option<&[f64]>,
    config: AnchorSearchConfig,
    geometry: &AnchorGeometryAuthority<'_>,
) -> Result<AnchorSeatDiscovery, AnchorSearchError> {
    validate_sidecar(delicacy_by_face, geometry.surface)?;
    discover_anchor_seats_bound(
        binding.attach_point_mm,
        None,
        AnchorSubjectFingerprint::Rewire(RewireFingerprint::from_binding(binding)),
        None,
        None,
        samples,
        delicacy_by_face,
        config,
        geometry,
        None,
    )
}

/// Production terminal discovery from a finalized physical contact and the
/// canonical exterior-centroid pool. No caller-provided point collection can
/// become model-anchor authority through this path.
#[allow(
    dead_code,
    reason = "the pending controller consumes this finalized authority seam"
)]
pub(crate) fn discover_finalized_anchor_seats(
    contact: BoundPhysicalAnchorContact,
    terminal: AnchorTerminalBinding,
    pool: &SurfaceAnchorPool<'_>,
    config: AnchorSearchConfig,
    geometry: &AnchorGeometryAuthority<'_>,
) -> Result<AnchorSeatDiscovery, AnchorSearchError> {
    pool.validate_against(geometry)?;
    discover_anchor_seats_bound(
        terminal.stub_end_mm,
        Some(AnchorContactFingerprint::Finalized(contact.fingerprint)),
        AnchorSubjectFingerprint::Terminal(TerminalFingerprint::from_binding(terminal)),
        contact.regular_identity,
        Some(contact.fingerprint.contact_id),
        pool.rows(),
        Some(&pool.delicacy_by_face),
        config,
        geometry,
        Some(pool.digest()),
    )
}

#[allow(clippy::too_many_arguments)]
fn discover_anchor_seats_bound(
    anchor_start_mm: [Finite; 3],
    contact_fingerprint: Option<AnchorContactFingerprint>,
    subject_fingerprint: AnchorSubjectFingerprint,
    provenance_regular_identity: Option<PrimaryRegularIdentity>,
    provenance_physical_contact_id: Option<u64>,
    samples: &[SurfaceAnchorSample],
    delicacy_by_face: Option<&[f64]>,
    config: AnchorSearchConfig,
    geometry: &AnchorGeometryAuthority<'_>,
    pool_digest: Option<u64>,
) -> Result<AnchorSeatDiscovery, AnchorSearchError> {
    let sidecar = delicacy_by_face.expect("validated above");
    let stub_end = finite3_to_f64(anchor_start_mm);
    let mut refusals = Vec::new();
    let mut qualified = Vec::new();
    for (sample_index, sample) in samples.iter().copied().enumerate() {
        let face = sample.source_face_id as usize;
        let refusal = if face >= geometry.surface.nfaces() {
            Some(AnchorSeatRefusalReason::SampleFaceOutOfRange)
        } else if !sample.delicacy.is_finite()
            || sample.point_mm.iter().any(|value| !value.is_finite())
        {
            Some(AnchorSeatRefusalReason::SampleNonFinite)
        } else if sample.delicacy.to_bits() != sidecar[face].to_bits() {
            Some(AnchorSeatRefusalReason::SampleDelicacyMismatch)
        } else {
            let delta = sub(sample.point_mm, stub_end);
            let length = norm(delta);
            let lean = lean_deg(delta, length);
            if sample.point_mm[2] > stub_end[2] + BELOW_LEVEL_EPS_MM {
                Some(AnchorSeatRefusalReason::AboveContact)
            } else if length < config.min_length_mm.get() {
                Some(AnchorSeatRefusalReason::TooShort)
            } else if length > config.max_length_mm.get() {
                Some(AnchorSeatRefusalReason::TooLong)
            } else if sample.delicacy > config.max_delicacy {
                Some(AnchorSeatRefusalReason::TooDelicate)
            } else if lean > config.max_lean_deg.get() + LEAN_EPS_DEG {
                Some(AnchorSeatRefusalReason::TooLean)
            } else {
                qualified.push(QualifiedSample {
                    sample_index,
                    sample,
                    length,
                });
                None
            }
        };
        if let Some(reason) = refusal {
            refusals.push(AnchorSeatRefusal {
                sample_index,
                source_sample_face_id: sample.source_face_id,
                reason,
            });
        }
    }
    qualified.sort_by(|left, right| {
        left.length
            .total_cmp(&right.length)
            .then(left.sample_index.cmp(&right.sample_index))
    });
    qualified.truncate(config.candidate_cap);
    if qualified.is_empty() {
        return Ok(AnchorSeatDiscovery {
            seats: Vec::new(),
            refusals,
            no_seat_reason: Some(NoAnchorSeatReason::NoQualifiedSamples),
            query_counts: AnchorSeatQueryCounts::default(),
        });
    }

    let origins = vec![stub_end; qualified.len()];
    let directions: Vec<[f64; 3]> = qualified
        .iter()
        .map(|candidate| sub(candidate.sample.point_mm, stub_end))
        .collect();
    let (offsets, ts, faces) = geometry
        .surface
        .cast_rays(&origins, &directions, RAY_T_MAX)
        .map_err(|error| AnchorSearchError::NativeQuery(error.to_string()))?;
    let exterior = geometry.surface.exterior_face_mask();
    let config_fingerprint = config.fingerprint();
    let mut seats = Vec::new();
    for (ordinal, candidate) in qualified.into_iter().enumerate() {
        let start = offsets[ordinal] as usize;
        let end = offsets[ordinal + 1] as usize;
        let refusal = if start >= end {
            Some(AnchorSeatRefusalReason::NoFirstHit)
        } else {
            let face = faces[start];
            if face < 0 || !exterior.get(face as usize).copied().unwrap_or(false) {
                Some(AnchorSeatRefusalReason::NearerNonExteriorObstruction)
            } else {
                let t = ts[start];
                let point = add(stub_end, scale(directions[ordinal], t));
                let length = norm(sub(point, stub_end));
                let lean = lean_deg(sub(point, stub_end), length);
                let face = face as usize;
                seats.push(QualifiedAnchorSeat {
                    provenance: AnchorCandidateProvenance {
                        regular_identity: provenance_regular_identity,
                        physical_contact_id: provenance_physical_contact_id,
                        sample_index: candidate.sample_index,
                        source_sample_face_id: candidate.sample.source_face_id,
                    },
                    contact: contact_fingerprint,
                    subject: subject_fingerprint,
                    geometry: geometry.fingerprint.clone(),
                    config: config_fingerprint,
                    pool_digest,
                    snapped_point_mm: finite3(point, "snapped anchor point")?,
                    snapped_normal: finite3(
                        geometry.surface.geometry().normals()[face],
                        "snapped face normal",
                    )?,
                    snapped_face_id: face as u32,
                    delicacy: finite(candidate.sample.delicacy, "anchor delicacy")?,
                    length_mm: positive(length, "snapped anchor length")?,
                    lean_deg: nonnegative(lean, "snapped anchor lean")?,
                    first_hit_t: finite(t, "first-hit ray parameter")?,
                    rootward_clearance_inset_mm: config.rootward_clearance_inset_mm,
                });
                None
            }
        };
        if let Some(reason) = refusal {
            refusals.push(AnchorSeatRefusal {
                sample_index: candidate.sample_index,
                source_sample_face_id: candidate.sample.source_face_id,
                reason,
            });
        }
    }
    let no_seat_reason = seats
        .is_empty()
        .then_some(NoAnchorSeatReason::NoExteriorFirstHit);
    Ok(AnchorSeatDiscovery {
        seats,
        refusals,
        no_seat_reason,
        query_counts: AnchorSeatQueryCounts { ray_batches: 1 },
    })
}

#[derive(Clone, Copy)]
struct QualifiedSample {
    sample_index: usize,
    sample: SurfaceAnchorSample,
    length: f64,
}

/// Crate-private CCO hand-off. A public caller cannot bind a seat to a radius.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct AnchorFootRequest {
    seat: QualifiedAnchorSeat,
    final_rootward_radius_mm: PositiveFinite,
}

impl AnchorFootRequest {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn new(
        seat: QualifiedAnchorSeat,
        final_rootward_radius_mm: PositiveFinite,
    ) -> Self {
        Self {
            seat,
            final_rootward_radius_mm,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorFootRefusalReason {
    ReboundContact,
    ReboundTerminal,
    ReboundRewireBinding,
    StaleConfig,
    StaleGeometry,
    StalePool,
    FootBallBreached,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorFootRefusal {
    provenance: AnchorCandidateProvenance,
    reason: AnchorFootRefusalReason,
}

impl AnchorFootRefusal {
    #[must_use]
    pub const fn provenance(self) -> AnchorCandidateProvenance {
        self.provenance
    }
    #[must_use]
    pub const fn reason(self) -> AnchorFootRefusalReason {
        self.reason
    }
}

/// Exact certificate for the emitted model-foot ball.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorFootCertificate {
    center_mm: [Finite; 3],
    final_member_radius_mm: PositiveFinite,
    foot_ball_radius_mm: PositiveFinite,
    nearest_wall_thickness_mm: Option<Finite>,
    poke_depth_mm: Finite,
}

impl AnchorFootCertificate {
    #[must_use]
    pub const fn center_mm(&self) -> &[Finite; 3] {
        &self.center_mm
    }
    #[must_use]
    pub const fn final_member_radius_mm(self) -> PositiveFinite {
        self.final_member_radius_mm
    }
    #[must_use]
    pub const fn foot_ball_radius_mm(self) -> PositiveFinite {
        self.foot_ball_radius_mm
    }
    #[must_use]
    pub const fn nearest_wall_thickness_mm(self) -> Option<Finite> {
        self.nearest_wall_thickness_mm
    }
    #[must_use]
    pub const fn poke_depth_mm(self) -> Finite {
        self.poke_depth_mm
    }
}

/// Move-only capability accepted by CCO after final sizing.
///
/// ```compile_fail
/// use plinth_support::anchor::ValidatedModelAnchor;
/// let forged = ValidatedModelAnchor {};
/// ```
///
/// ```compile_fail
/// use plinth_support::anchor::AnchorFootRequest;
/// ```
#[derive(Debug)]
pub struct ValidatedModelAnchor {
    seat: QualifiedAnchorSeat,
    foot: AnchorFootCertificate,
}

impl ValidatedModelAnchor {
    #[must_use]
    pub const fn seat(&self) -> &QualifiedAnchorSeat {
        &self.seat
    }
    #[must_use]
    pub const fn foot(&self) -> AnchorFootCertificate {
        self.foot
    }
}

#[derive(Debug)]
pub enum AnchorFootOutcome {
    Validated(Box<ValidatedModelAnchor>),
    Refused(AnchorFootRefusal),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnchorFootQueryCounts {
    pub sphere_breach_batches: usize,
}

#[derive(Debug)]
pub struct AnchorFootValidation {
    outcomes: Vec<AnchorFootOutcome>,
    query_counts: AnchorFootQueryCounts,
}

impl AnchorFootValidation {
    #[must_use]
    pub fn outcomes(&self) -> &[AnchorFootOutcome] {
        &self.outcomes
    }
    #[must_use]
    pub fn into_outcomes(self) -> Vec<AnchorFootOutcome> {
        self.outcomes
    }
    #[must_use]
    pub const fn query_counts(&self) -> AnchorFootQueryCounts {
        self.query_counts
    }
}

/// Validate the exactly sized model feet in one gathered sphere-breach batch.
/// Invalid/rebound seats are refused before the batch and scattered back into
/// their original request positions.
#[cfg(test)]
pub(crate) fn validate_anchor_feet(
    requests: Vec<AnchorFootRequest>,
    contact: &SelectedContact,
    terminal: AnchorTerminalBinding,
    config: AnchorSearchConfig,
    geometry: &AnchorGeometryAuthority<'_>,
) -> Result<AnchorFootValidation, AnchorSearchError> {
    validate_anchor_feet_bound(
        requests,
        Some(AnchorContactFingerprint::Selected(
            ContactFingerprint::from_contact(contact),
        )),
        AnchorSubjectFingerprint::Terminal(TerminalFingerprint::from_binding(terminal)),
        config,
        geometry,
        None,
    )
}

pub(crate) fn validate_rewire_anchor_feet(
    requests: Vec<AnchorFootRequest>,
    binding: RewireAnchorBinding,
    config: AnchorSearchConfig,
    geometry: &AnchorGeometryAuthority<'_>,
) -> Result<AnchorFootValidation, AnchorSearchError> {
    validate_anchor_feet_bound(
        requests,
        None,
        AnchorSubjectFingerprint::Rewire(RewireFingerprint::from_binding(binding)),
        config,
        geometry,
        None,
    )
}

/// Final-foot validation paired with `discover_finalized_anchor_seats`.
/// Rebinding requires the exact plan/contact/provenance/area fingerprint and
/// the exact pool digest that certified discovery.
#[allow(
    dead_code,
    reason = "the pending controller consumes this finalized authority seam"
)]
pub(crate) fn validate_finalized_anchor_feet(
    requests: Vec<AnchorFootRequest>,
    contact: BoundPhysicalAnchorContact,
    terminal: AnchorTerminalBinding,
    pool: &SurfaceAnchorPool<'_>,
    config: AnchorSearchConfig,
    geometry: &AnchorGeometryAuthority<'_>,
) -> Result<AnchorFootValidation, AnchorSearchError> {
    pool.validate_against(geometry)?;
    validate_anchor_feet_bound(
        requests,
        Some(AnchorContactFingerprint::Finalized(contact.fingerprint)),
        AnchorSubjectFingerprint::Terminal(TerminalFingerprint::from_binding(terminal)),
        config,
        geometry,
        Some(pool.digest()),
    )
}

fn validate_anchor_feet_bound(
    requests: Vec<AnchorFootRequest>,
    contact_fingerprint: Option<AnchorContactFingerprint>,
    subject_fingerprint: AnchorSubjectFingerprint,
    config: AnchorSearchConfig,
    geometry: &AnchorGeometryAuthority<'_>,
    pool_digest: Option<u64>,
) -> Result<AnchorFootValidation, AnchorSearchError> {
    let config_fingerprint = config.fingerprint();
    let rebound_subject = match subject_fingerprint {
        AnchorSubjectFingerprint::Terminal(_) => AnchorFootRefusalReason::ReboundTerminal,
        AnchorSubjectFingerprint::Rewire(_) => AnchorFootRefusalReason::ReboundRewireBinding,
    };
    let mut slots: Vec<Option<AnchorFootOutcome>> = (0..requests.len()).map(|_| None).collect();
    let mut valid = Vec::new();
    for (ordinal, request) in requests.into_iter().enumerate() {
        let reason = if request.seat.contact != contact_fingerprint {
            Some(AnchorFootRefusalReason::ReboundContact)
        } else if request.seat.subject != subject_fingerprint {
            Some(rebound_subject)
        } else if request.seat.config != config_fingerprint {
            Some(AnchorFootRefusalReason::StaleConfig)
        } else if request.seat.geometry != geometry.fingerprint {
            Some(AnchorFootRefusalReason::StaleGeometry)
        } else if request.seat.pool_digest != pool_digest {
            Some(AnchorFootRefusalReason::StalePool)
        } else {
            None
        };
        if let Some(reason) = reason {
            slots[ordinal] = Some(AnchorFootOutcome::Refused(AnchorFootRefusal {
                provenance: request.seat.provenance,
                reason,
            }));
        } else {
            valid.push((ordinal, request));
        }
    }
    if valid.is_empty() {
        return Ok(AnchorFootValidation {
            outcomes: slots.into_iter().flatten().collect(),
            query_counts: AnchorFootQueryCounts::default(),
        });
    }

    let centers: Vec<[f64; 3]> = valid
        .iter()
        .map(|(_, request)| finite3_to_f64(request.seat.snapped_point_mm))
        .collect();
    let radii: Vec<f64> = valid
        .iter()
        .map(|(_, request)| config.ball_factor_foot.get() * request.final_rootward_radius_mm.get())
        .collect();
    let (breached, thickness, poke) = geometry
        .surface
        .sphere_breach(
            &centers,
            &radii,
            config.ball_breach_eps_mm.get(),
            FOOT_BREACH_HALF_DEG,
            FOOT_BREACH_CONE_RAYS,
        )
        .map_err(|error| AnchorSearchError::NativeQuery(error.to_string()))?;
    for (batch_index, (ordinal, request)) in valid.into_iter().enumerate() {
        slots[ordinal] = Some(if breached[batch_index] {
            AnchorFootOutcome::Refused(AnchorFootRefusal {
                provenance: request.seat.provenance,
                reason: AnchorFootRefusalReason::FootBallBreached,
            })
        } else {
            AnchorFootOutcome::Validated(Box::new(ValidatedModelAnchor {
                foot: AnchorFootCertificate {
                    center_mm: request.seat.snapped_point_mm,
                    final_member_radius_mm: request.final_rootward_radius_mm,
                    foot_ball_radius_mm: positive(radii[batch_index], "foot-ball radius")?,
                    nearest_wall_thickness_mm: finite_or_infinite(
                        thickness[batch_index],
                        "foot-ball thickness",
                    )?,
                    poke_depth_mm: finite(poke[batch_index], "foot-ball poke depth")?,
                },
                seat: request.seat,
            }))
        });
    }
    Ok(AnchorFootValidation {
        outcomes: slots
            .into_iter()
            .map(|slot| slot.expect("every request scattered exactly once"))
            .collect(),
        query_counts: AnchorFootQueryCounts {
            sphere_breach_batches: 1,
        },
    })
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum AnchorSearchError {
    #[error("registered M0 constants are unavailable: {0}")]
    RegisteredConstantsUnavailable(String),
    #[error("registered model-anchor count cannot be represented")]
    CountNotRepresentable,
    #[error("native face id cannot be represented as u32")]
    FaceIdNotRepresentable,
    #[error("finalized physical contact {contact_id} was not found in its plan")]
    FinalizedContactNotFound { contact_id: u64 },
    #[error("surface anchor pool belongs to a different native surface or has changed")]
    StaleSurfaceAnchorPool,
    #[error("surface anchor pool requires {required} rows; limit is {limit}")]
    SurfaceAnchorPoolRowLimit { required: usize, limit: usize },
    #[error("surface anchor pool requires {required} bytes; limit is {limit}")]
    SurfaceAnchorPoolByteLimit { required: usize, limit: usize },
    #[error("surface anchor pool resource arithmetic overflowed")]
    SurfaceAnchorPoolResourceOverflow,
    #[error("surface anchor pool could not allocate its {required}-byte payload")]
    SurfaceAnchorPoolAllocation { required: usize },
    #[error("model-anchor search requires a face-order delicacy sidecar")]
    MissingDelicacySidecar,
    #[error("delicacy sidecar has {actual} rows; surface has {expected} faces")]
    DelicacyLength { expected: usize, actual: usize },
    #[error("delicacy sidecar value {index} is not finite")]
    NonFiniteDelicacy { index: usize },
    #[error("selected contact face {face_id} is outside the native surface")]
    ContactFaceOutOfRange { face_id: u32 },
    #[error("selected contact delicacy disagrees with its face sidecar")]
    ContactDelicacyMismatch,
    #[error("selected contact contains non-finite evidence")]
    ContactNonFinite,
    #[error("face topology is invalid: {0}")]
    InvalidTopology(String),
    #[error("face topology belongs to a different coordinate/index mesh")]
    TopologyMeshMismatch,
    #[error("native surface does not exactly match the supplied indexed mesh at face {face}")]
    SurfaceMeshMismatch { face: usize },
    #[error("native model-anchor query failed: {0}")]
    NativeQuery(String),
    #[error("model-anchor derived non-finite or non-positive {field}")]
    InvalidDerived { field: &'static str },
}

#[cfg(test)]
fn validate_contact_and_sidecar(
    contact: &SelectedContact,
    sidecar: Option<&[f64]>,
    surface: &NativeSurface,
) -> Result<(), AnchorSearchError> {
    validate_sidecar(sidecar, surface)?;
    let sidecar = sidecar.expect("validated above");
    let face = contact.served_face_id as usize;
    if face >= surface.nfaces() {
        return Err(AnchorSearchError::ContactFaceOutOfRange {
            face_id: contact.served_face_id,
        });
    }
    if contact
        .point_mm
        .iter()
        .chain(contact.normal.iter())
        .chain([contact.delicacy, contact.supported_area_mm2].iter())
        .any(|value| !value.is_finite())
    {
        return Err(AnchorSearchError::ContactNonFinite);
    }
    if contact.delicacy.to_bits() != sidecar[face].to_bits() {
        return Err(AnchorSearchError::ContactDelicacyMismatch);
    }
    Ok(())
}

fn validate_sidecar(
    sidecar: Option<&[f64]>,
    surface: &NativeSurface,
) -> Result<(), AnchorSearchError> {
    let sidecar = sidecar.ok_or(AnchorSearchError::MissingDelicacySidecar)?;
    if sidecar.len() != surface.nfaces() {
        return Err(AnchorSearchError::DelicacyLength {
            expected: surface.nfaces(),
            actual: sidecar.len(),
        });
    }
    if let Some((index, _)) = sidecar
        .iter()
        .enumerate()
        .find(|(_, value)| !value.is_finite())
    {
        return Err(AnchorSearchError::NonFiniteDelicacy { index });
    }
    Ok(())
}

fn validate_surface_matches_mesh(
    mesh: &Mesh,
    surface: &NativeSurface,
) -> Result<SurfaceTriangleSoupDigest, AnchorSearchError> {
    if mesh.tris().len() != surface.nfaces() {
        return Err(AnchorSearchError::SurfaceMeshMismatch { face: 0 });
    }
    let digest = indexed_triangle_soup_digest(mesh.verts(), mesh.tris())
        .expect("Mesh validates every triangle index");
    if digest != surface.source_triangle_soup_digest() {
        return Err(AnchorSearchError::SurfaceMeshMismatch { face: 0 });
    }
    // Digest equality is the exact coordinate-content check, not a capability
    // proof. The authority also retains the native pointer and binds mesh,
    // topology, and result-changing config. Keep the metadata check because
    // `from_triangle_soup` admits explicitly supplied normals, areas, and
    // bounds that can alter downstream query semantics.
    let geometry = surface.geometry();
    for (face, indices) in mesh.tris().iter().enumerate() {
        let [a, b, c] = indices.map(|index| mesh.verts()[index as usize]);
        let e1 = sub(b, a);
        let e2 = sub(c, a);
        let cross = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let length = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        let normal = cross.map(|value| value / length.max(1e-30));
        let area = 0.5 * length;
        let centroid = [
            (a[0] + b[0] + c[0]) / 3.0,
            (a[1] + b[1] + c[1]) / 3.0,
            (a[2] + b[2] + c[2]) / 3.0,
        ];
        let bounds = [
            [
                a[0].min(b[0]).min(c[0]),
                a[1].min(b[1]).min(c[1]),
                a[2].min(b[2]).min(c[2]),
            ],
            [
                a[0].max(b[0]).max(c[0]),
                a[1].max(b[1]).max(c[1]),
                a[2].max(b[2]).max(c[2]),
            ],
        ];
        if bits3(normal) != bits3(geometry.normals()[face])
            || area.to_bits() != geometry.areas()[face].to_bits()
            || bits3(centroid) != bits3(geometry.centroids()[face])
            || bounds.map(bits3) != geometry.face_bounds()[face].map(bits3)
        {
            return Err(AnchorSearchError::SurfaceMeshMismatch { face });
        }
    }
    Ok(digest)
}

fn finite(value: f64, field: &'static str) -> Result<Finite, AnchorSearchError> {
    Finite::new(value).map_err(|_| AnchorSearchError::InvalidDerived { field })
}
fn finite3(values: [f64; 3], field: &'static str) -> Result<[Finite; 3], AnchorSearchError> {
    Ok([
        finite(values[0], field)?,
        finite(values[1], field)?,
        finite(values[2], field)?,
    ])
}
#[cfg_attr(not(test), allow(dead_code))]
fn finite_or_infinite(
    value: f64,
    field: &'static str,
) -> Result<Option<Finite>, AnchorSearchError> {
    if value == f64::INFINITY {
        Ok(None)
    } else {
        finite(value, field).map(Some)
    }
}
fn positive(value: f64, field: &'static str) -> Result<PositiveFinite, AnchorSearchError> {
    PositiveFinite::new(value).map_err(|_| AnchorSearchError::InvalidDerived { field })
}
fn nonnegative(value: f64, field: &'static str) -> Result<NonNegativeFinite, AnchorSearchError> {
    NonNegativeFinite::new(value).map_err(|_| AnchorSearchError::InvalidDerived { field })
}
fn finite3_to_f64(values: [Finite; 3]) -> [f64; 3] {
    values.map(Finite::get)
}
fn bits3(values: [f64; 3]) -> [u64; 3] {
    values.map(f64::to_bits)
}
fn sub(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}
fn add(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}
fn scale(value: [f64; 3], factor: f64) -> [f64; 3] {
    [value[0] * factor, value[1] * factor, value[2] * factor]
}
fn norm(value: [f64; 3]) -> f64 {
    value[0].hypot(value[1]).hypot(value[2])
}
fn lean_deg(delta: [f64; 3], length: f64) -> f64 {
    if length < 1e-9 {
        0.0
    } else {
        delta[0].hypot(delta[1]).atan2(delta[2].abs()).to_degrees()
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use geom_kernel::NativeSurfaceConfig;
    use plinth_mesh::{build_face_topology, WeldPolicy};

    use super::*;
    use crate::forest::{
        Forest, Member, MemberKind, MemberSpec, Owner, RootwardEndRadius, TerminalEndRadius,
    };
    use crate::ContactCandidateId;

    fn f(value: f64) -> Finite {
        Finite::new(value).unwrap()
    }
    fn p(value: f64) -> PositiveFinite {
        PositiveFinite::new(value).unwrap()
    }

    fn box_parts(z_top: f64) -> (Mesh, FaceTopology, NativeSurface) {
        let vertices = vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, z_top],
            [10.0, 0.0, z_top],
            [10.0, 10.0, z_top],
            [0.0, 10.0, z_top],
        ];
        let faces = vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ];
        parts(vertices, faces)
    }

    fn nested_box_parts() -> (Mesh, FaceTopology, NativeSurface) {
        let (outer, _, _) = box_parts(10.0);
        let mut vertices = outer.verts().to_vec();
        vertices.extend([
            [4.0, 4.0, 4.0],
            [6.0, 4.0, 4.0],
            [6.0, 6.0, 4.0],
            [4.0, 6.0, 4.0],
            [4.0, 4.0, 6.0],
            [6.0, 4.0, 6.0],
            [6.0, 6.0, 6.0],
            [4.0, 6.0, 6.0],
        ]);
        let mut faces = outer.tris().to_vec();
        faces.extend([
            [8, 10, 9],
            [8, 11, 10],
            [12, 13, 14],
            [12, 14, 15],
            [8, 9, 13],
            [8, 13, 12],
            [9, 10, 14],
            [9, 14, 13],
            [10, 11, 15],
            [10, 15, 14],
            [11, 8, 12],
            [11, 12, 15],
        ]);
        parts(vertices, faces)
    }

    fn parts(vertices: Vec<[f64; 3]>, faces: Vec<[u32; 3]>) -> (Mesh, FaceTopology, NativeSurface) {
        let mesh = Mesh::new(vertices, faces).unwrap();
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let surface = NativeSurface::from_indexed(
            mesh.verts(),
            mesh.tris(),
            NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: 64 * 1024 * 1024,
            },
        )
        .unwrap();
        (mesh, topology, surface)
    }

    fn contact(z: f64) -> SelectedContact {
        SelectedContact {
            island_id: 7,
            candidate_id: ContactCandidateId(2),
            served_face_id: 2,
            point_mm: [5.0, 5.0, z],
            normal: [0.0, 0.0, 1.0],
            delicacy: 0.25,
            supported_area_mm2: 50.0,
        }
    }

    fn member() -> Member {
        Member::new(MemberSpec {
            p0_mm: [f(5.0), f(5.0), f(10.0)],
            p1_mm: [f(5.0), f(5.0), f(12.5)],
            r0_mm: TerminalEndRadius::new(p(0.2)),
            r1_mm: RootwardEndRadius::new(p(0.35)),
            kind: MemberKind::Stub,
            owner: Owner::Contact(2),
            embed_end: None,
            carried_mm2: f(1.0),
        })
    }

    fn bindings() -> (Forest, AnchorTerminalBinding, AnchorTerminalBinding) {
        let mut forest = Forest::new();
        let first = forest.add_member(member());
        let second = forest.add_member(member());
        (
            forest,
            AnchorTerminalBinding::new(2, first, [f(5.0), f(5.0), f(12.5)]),
            AnchorTerminalBinding::new(2, second, [f(5.0), f(5.0), f(12.5)]),
        )
    }

    fn sample() -> SurfaceAnchorSample {
        SurfaceAnchorSample {
            source_face_id: 0,
            point_mm: [5.0, 5.0, 0.0],
            delicacy: 0.25,
        }
    }

    #[test]
    fn surface_anchor_pool_is_exterior_face_ordered_and_binds_sidecar_and_native_pitch() {
        let (mesh, topology, surface) = box_parts(10.0);
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let delicacy = (0..surface.nfaces())
            .map(|index| index as f64 + 0.25)
            .collect::<Vec<_>>();
        let pool = SurfaceAnchorPool::from_geometry(&geometry, &delicacy).unwrap();
        let expected_faces = surface
            .exterior_face_mask()
            .iter()
            .enumerate()
            .filter_map(|(index, exterior)| exterior.then_some(index as u32))
            .collect::<Vec<_>>();
        assert_eq!(
            pool.rows()
                .iter()
                .map(|row| row.source_face_id)
                .collect::<Vec<_>>(),
            expected_faces
        );
        for row in pool.rows() {
            assert_eq!(
                row.point_mm.map(f64::to_bits),
                surface.geometry().centroids()[row.source_face_id as usize].map(f64::to_bits)
            );
            assert_eq!(
                row.delicacy.to_bits(),
                delicacy[row.source_face_id as usize].to_bits()
            );
        }

        let mut changed_delicacy = delicacy.clone();
        changed_delicacy[0] = 99.0;
        assert_ne!(
            pool.digest(),
            SurfaceAnchorPool::from_geometry(&geometry, &changed_delicacy)
                .unwrap()
                .digest()
        );

        // An enclosed shell supplies known non-exterior faces. Mutating only
        // one of those sidecar rows must still change pool authority even
        // though every derived exterior row remains bit-identical.
        let (nested_mesh, nested_topology, nested_surface) = nested_box_parts();
        let nested_geometry =
            AnchorGeometryAuthority::new(&nested_mesh, &nested_topology, &nested_surface).unwrap();
        let nested_delicacy = vec![0.25; nested_surface.nfaces()];
        let nested_pool =
            SurfaceAnchorPool::from_geometry(&nested_geometry, &nested_delicacy).unwrap();
        let nested_required_bytes = surface_anchor_pool_required_bytes(
            &nested_geometry.fingerprint,
            nested_delicacy.len(),
            nested_pool.rows().len(),
        )
        .unwrap();
        let exact_cap_pool = SurfaceAnchorPool::from_geometry_with_limits(
            &nested_geometry,
            &nested_delicacy,
            SurfaceAnchorPoolLimits {
                max_rows: nested_pool.rows().len(),
                max_bytes: nested_required_bytes,
            },
        )
        .unwrap();
        assert_eq!(exact_cap_pool.digest(), nested_pool.digest());
        assert!(matches!(
            SurfaceAnchorPool::from_geometry_with_limits(
                &nested_geometry,
                &nested_delicacy,
                SurfaceAnchorPoolLimits {
                    max_rows: nested_pool.rows().len(),
                    max_bytes: nested_required_bytes - 1,
                },
            ),
            Err(AnchorSearchError::SurfaceAnchorPoolByteLimit {
                required,
                limit,
            }) if required == nested_required_bytes && limit == nested_required_bytes - 1
        ));
        assert_eq!(
            surface_anchor_pool_required_bytes(
                &nested_geometry.fingerprint,
                usize::MAX,
                nested_pool.rows().len(),
            ),
            Err(AnchorSearchError::SurfaceAnchorPoolResourceOverflow)
        );
        let non_exterior_face = nested_surface
            .exterior_face_mask()
            .iter()
            .position(|exterior| !exterior)
            .expect("nested shell has non-exterior faces");
        let mut changed_non_exterior = nested_delicacy.clone();
        changed_non_exterior[non_exterior_face] = 0.75;
        let rebound_pool =
            SurfaceAnchorPool::from_geometry(&nested_geometry, &changed_non_exterior).unwrap();
        assert_eq!(nested_pool.rows(), rebound_pool.rows());
        assert_ne!(nested_pool.digest(), rebound_pool.digest());

        // A separately allocated native surface over the same exact source
        // geometry has the same value fingerprint, but remains stale: rebound
        // checks intentionally retain pointer identity.
        let identical_surface = NativeSurface::from_indexed(
            mesh.verts(),
            mesh.tris(),
            NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: 64 * 1024 * 1024,
            },
        )
        .unwrap();
        let identical_geometry =
            AnchorGeometryAuthority::new(&mesh, &topology, &identical_surface).unwrap();
        assert_eq!(geometry.fingerprint(), identical_geometry.fingerprint());
        assert_eq!(
            pool.validate_against(&identical_geometry),
            Err(AnchorSearchError::StaleSurfaceAnchorPool)
        );

        let (_, terminal, _) = bindings();
        let config = AnchorSearchConfig::current().unwrap();
        let subject =
            AnchorSubjectFingerprint::Terminal(TerminalFingerprint::from_binding(terminal));
        let seat = QualifiedAnchorSeat {
            provenance: AnchorCandidateProvenance {
                regular_identity: None,
                physical_contact_id: None,
                sample_index: 0,
                source_sample_face_id: 0,
            },
            contact: None,
            subject,
            geometry: nested_geometry.fingerprint(),
            config: config.fingerprint(),
            pool_digest: Some(nested_pool.digest()),
            snapped_point_mm: [f(5.0), f(5.0), f(0.0)],
            snapped_normal: [f(0.0), f(0.0), f(-1.0)],
            snapped_face_id: 0,
            delicacy: f(0.25),
            length_mm: p(12.5),
            lean_deg: NonNegativeFinite::new(0.0).unwrap(),
            first_hit_t: f(1.0),
            rootward_clearance_inset_mm: NonNegativeFinite::new(0.0).unwrap(),
        };
        let rebound = validate_anchor_feet_bound(
            vec![AnchorFootRequest::new(seat, p(0.35))],
            None,
            subject,
            config,
            &nested_geometry,
            Some(rebound_pool.digest()),
        )
        .unwrap();
        assert_eq!(
            rebound.query_counts(),
            AnchorFootQueryCounts {
                sphere_breach_batches: 0
            }
        );
        assert!(matches!(
            rebound.outcomes(),
            [AnchorFootOutcome::Refused(refusal)]
                if refusal.reason() == AnchorFootRefusalReason::StalePool
        ));

        let other_surface = NativeSurface::from_indexed(
            mesh.verts(),
            mesh.tris(),
            NativeSurfaceConfig {
                grid_pitch_mm: 0.5,
                max_grid_bytes: 64 * 1024 * 1024,
            },
        )
        .unwrap();
        let other_geometry =
            AnchorGeometryAuthority::new(&mesh, &topology, &other_surface).unwrap();
        assert_eq!(
            pool.validate_against(&other_geometry),
            Err(AnchorSearchError::StaleSurfaceAnchorPool)
        );
        assert!(matches!(
            SurfaceAnchorPool::from_geometry_with_limits(
                &geometry,
                &delicacy,
                SurfaceAnchorPoolLimits {
                    max_rows: 0,
                    max_bytes: usize::MAX
                },
            ),
            Err(AnchorSearchError::SurfaceAnchorPoolRowLimit { .. })
        ));
        assert!(matches!(
            SurfaceAnchorPool::from_geometry_with_limits(
                &geometry,
                &delicacy,
                SurfaceAnchorPoolLimits {
                    max_rows: usize::MAX,
                    max_bytes: 0
                },
            ),
            Err(AnchorSearchError::SurfaceAnchorPoolByteLimit { .. })
        ));
    }

    fn discover_one(
        contact: &SelectedContact,
        terminal: AnchorTerminalBinding,
        mesh: &Mesh,
        topology: &FaceTopology,
        surface: &NativeSurface,
        sidecar: &[f64],
        config: AnchorSearchConfig,
    ) -> QualifiedAnchorSeat {
        let geometry = AnchorGeometryAuthority::new(mesh, topology, surface).unwrap();
        discover_anchor_seats(
            contact,
            terminal,
            &[sample()],
            Some(sidecar),
            config,
            &geometry,
        )
        .unwrap()
        .into_seats()
        .remove(0)
    }

    #[test]
    fn rewire_seat_is_bound_to_exact_root_checkpoint_and_validated_in_one_batch() {
        let (mesh, topology, surface) = box_parts(10.0);
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let (forest, terminal, _) = bindings();
        let binding = RewireAnchorBinding::new(
            terminal.stub(),
            forest.digest(),
            forest.digest().wrapping_add(1),
            terminal.stub_end_mm(),
        );
        let sidecar = vec![0.25; surface.nfaces()];
        let discovery = discover_rewire_anchor_seats(
            binding,
            &[sample()],
            Some(&sidecar),
            AnchorSearchConfig::current().unwrap(),
            &geometry,
        )
        .unwrap();
        assert_eq!(discovery.query_counts().ray_batches, 1);
        let seat = discovery.into_seats().remove(0);

        let rebound = RewireAnchorBinding::new(
            terminal.stub(),
            forest.digest(),
            forest.digest().wrapping_add(2),
            terminal.stub_end_mm(),
        );
        let validation = validate_rewire_anchor_feet(
            vec![AnchorFootRequest::new(seat, p(0.35))],
            rebound,
            AnchorSearchConfig::current().unwrap(),
            &geometry,
        )
        .unwrap();
        assert_eq!(validation.query_counts().sphere_breach_batches, 0);
        assert!(matches!(
            validation.outcomes(),
            [AnchorFootOutcome::Refused(refusal)]
                if refusal.reason() == AnchorFootRefusalReason::ReboundRewireBinding
        ));
    }

    #[test]
    fn frozen_qualification_and_stable_cap_vectors() {
        let (mesh, topology, surface) = box_parts(10.0);
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let (_, terminal, _) = bindings();
        let mut sidecar = vec![0.25; 12];
        sidecar[1] = 0.5;
        sidecar[4] = 0.500_000_1;
        let mut samples = vec![
            SurfaceAnchorSample {
                source_face_id: 0,
                point_mm: [5.0, 5.0, 8.0],
                delicacy: 0.25,
            },
            SurfaceAnchorSample {
                source_face_id: 1,
                point_mm: [5.0, 5.0, 0.0],
                delicacy: 0.5,
            },
            SurfaceAnchorSample {
                source_face_id: 0,
                point_mm: [5.0, 5.0, -11.0],
                delicacy: 0.25,
            },
            SurfaceAnchorSample {
                source_face_id: 4,
                point_mm: [5.0, 5.0, 0.0],
                delicacy: 0.500_000_1,
            },
        ];
        samples.extend(vec![sample(); 41]);
        let result = discover_anchor_seats(
            &contact(10.0),
            terminal,
            &samples,
            Some(&sidecar),
            AnchorSearchConfig::current().unwrap(),
            &geometry,
        )
        .unwrap();
        assert_eq!(result.seats().len(), 40);
        let ids: Vec<_> = result
            .seats()
            .iter()
            .map(|seat| seat.provenance().sample_index())
            .collect();
        assert_eq!(ids[0], 1);
        assert_eq!(&ids[1..], &(4usize..43).collect::<Vec<_>>());
        assert!(result
            .refusals()
            .iter()
            .any(|row| row.sample_index == 0 && row.reason == AnchorSeatRefusalReason::TooShort));
        assert!(result
            .refusals()
            .iter()
            .any(|row| row.sample_index == 2 && row.reason == AnchorSeatRefusalReason::TooLong));
        assert!(
            result
                .refusals()
                .iter()
                .any(|row| row.sample_index == 3
                    && row.reason == AnchorSeatRefusalReason::TooDelicate)
        );
        assert_eq!(
            result.query_counts(),
            AnchorSeatQueryCounts { ray_batches: 1 }
        );
    }

    #[test]
    fn actual_nearer_non_exterior_obstruction_is_not_skipped() {
        let (mesh, topology, surface) = nested_box_parts();
        assert!(!surface.exterior_face_mask()[14]);
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let (_, base, _) = bindings();
        let terminal =
            AnchorTerminalBinding::new(base.terminal_id(), base.stub(), [f(5.0), f(5.0), f(7.0)]);
        let sidecar = vec![0.25; surface.nfaces()];
        let result = discover_anchor_seats(
            &contact(10.0),
            terminal,
            &[sample()],
            Some(&sidecar),
            AnchorSearchConfig::current().unwrap(),
            &geometry,
        )
        .unwrap();
        assert!(result.seats().is_empty());
        assert_eq!(
            result.refusals()[0].reason,
            AnchorSeatRefusalReason::NearerNonExteriorObstruction
        );
    }

    #[test]
    fn final_radius_is_used_and_can_reverse_the_provisional_breach_verdict() {
        let (mesh, topology, surface) = box_parts(0.1);
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let (_, terminal, _) = bindings();
        let contact = contact(0.1);
        let sidecar = vec![0.25; 12];
        let config = AnchorSearchConfig::current().unwrap();
        let small = discover_one(
            &contact, terminal, &mesh, &topology, &surface, &sidecar, config,
        );
        let large = discover_one(
            &contact, terminal, &mesh, &topology, &surface, &sidecar, config,
        );
        let result = validate_anchor_feet(
            vec![
                AnchorFootRequest::new(small, p(0.05)),
                AnchorFootRequest::new(large, p(0.35)),
            ],
            &contact,
            terminal,
            config,
            &geometry,
        )
        .unwrap();
        let expected_small_foot = config.ball_factor_foot.get() * 0.05;
        assert!(
            matches!(&result.outcomes()[0], AnchorFootOutcome::Validated(anchor) if anchor.foot().foot_ball_radius_mm().get().to_bits() == expected_small_foot.to_bits())
        );
        assert!(
            matches!(&result.outcomes()[1], AnchorFootOutcome::Refused(row) if row.reason() == AnchorFootRefusalReason::FootBallBreached)
        );
        assert_eq!(
            result.query_counts(),
            AnchorFootQueryCounts {
                sphere_breach_batches: 1
            }
        );
    }

    #[test]
    fn stale_and_rebound_authorities_refuse_before_any_sphere_query() {
        let (mesh, topology, surface) = box_parts(10.0);
        let (other_mesh, other_topology, other_surface) = box_parts(9.0);
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let other_geometry =
            AnchorGeometryAuthority::new(&other_mesh, &other_topology, &other_surface).unwrap();
        let (_, terminal, rebound_terminal) = bindings();
        let contact = contact(10.0);
        let sidecar = vec![0.25; 12];
        let config = AnchorSearchConfig::current().unwrap();
        let seat = |cfg| {
            discover_one(
                &contact, terminal, &mesh, &topology, &surface, &sidecar, cfg,
            )
        };

        let mut changed_contact = contact.clone();
        changed_contact.candidate_id = ContactCandidateId(99);
        let r = validate_anchor_feet(
            vec![AnchorFootRequest::new(seat(config), p(0.35))],
            &changed_contact,
            terminal,
            config,
            &geometry,
        )
        .unwrap();
        assert!(
            matches!(&r.outcomes()[0], AnchorFootOutcome::Refused(row) if row.reason() == AnchorFootRefusalReason::ReboundContact)
        );
        let r = validate_anchor_feet(
            vec![AnchorFootRequest::new(seat(config), p(0.35))],
            &contact,
            rebound_terminal,
            config,
            &geometry,
        )
        .unwrap();
        assert!(
            matches!(&r.outcomes()[0], AnchorFootOutcome::Refused(row) if row.reason() == AnchorFootRefusalReason::ReboundTerminal)
        );
        let mut changed_config = config;
        changed_config.max_length_mm = p(19.0);
        let r = validate_anchor_feet(
            vec![AnchorFootRequest::new(seat(config), p(0.35))],
            &contact,
            terminal,
            changed_config,
            &geometry,
        )
        .unwrap();
        assert!(
            matches!(&r.outcomes()[0], AnchorFootOutcome::Refused(row) if row.reason() == AnchorFootRefusalReason::StaleConfig)
        );
        let r = validate_anchor_feet(
            vec![AnchorFootRequest::new(seat(config), p(0.35))],
            &contact,
            terminal,
            config,
            &other_geometry,
        )
        .unwrap();
        assert!(
            matches!(&r.outcomes()[0], AnchorFootOutcome::Refused(row) if row.reason() == AnchorFootRefusalReason::StaleGeometry)
        );
        assert_eq!(r.query_counts().sphere_breach_batches, 0);
    }

    #[test]
    fn mixed_rebound_valid_and_breach_outcomes_scatter_in_request_order() {
        let (mesh, topology, surface) = box_parts(0.1);
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let (_, terminal, rebound_terminal) = bindings();
        let contact = contact(0.1);
        let sidecar = vec![0.25; 12];
        let config = AnchorSearchConfig::current().unwrap();
        let rebound = discover_one(
            &contact,
            rebound_terminal,
            &mesh,
            &topology,
            &surface,
            &sidecar,
            config,
        );
        let valid = discover_one(
            &contact, terminal, &mesh, &topology, &surface, &sidecar, config,
        );
        let breach = discover_one(
            &contact, terminal, &mesh, &topology, &surface, &sidecar, config,
        );
        let result = validate_anchor_feet(
            vec![
                AnchorFootRequest::new(rebound, p(0.05)),
                AnchorFootRequest::new(valid, p(0.05)),
                AnchorFootRequest::new(breach, p(0.35)),
            ],
            &contact,
            terminal,
            config,
            &geometry,
        )
        .unwrap();
        assert!(
            matches!(&result.outcomes()[0], AnchorFootOutcome::Refused(row) if row.reason() == AnchorFootRefusalReason::ReboundTerminal)
        );
        assert!(matches!(
            &result.outcomes()[1],
            AnchorFootOutcome::Validated(_)
        ));
        assert!(
            matches!(&result.outcomes()[2], AnchorFootOutcome::Refused(row) if row.reason() == AnchorFootRefusalReason::FootBallBreached)
        );
        assert_eq!(result.query_counts().sphere_breach_batches, 1);
    }

    #[test]
    fn sidecars_and_geometry_rebinding_fail_named() {
        let (mesh, topology, surface) = box_parts(10.0);
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let (_, terminal, _) = bindings();
        let config = AnchorSearchConfig::current().unwrap();
        assert!(matches!(
            discover_anchor_seats(
                &contact(10.0),
                terminal,
                &[sample()],
                None,
                config,
                &geometry
            ),
            Err(AnchorSearchError::MissingDelicacySidecar)
        ));
        assert!(matches!(
            discover_anchor_seats(
                &contact(10.0),
                terminal,
                &[sample()],
                Some(&[0.25; 11]),
                config,
                &geometry
            ),
            Err(AnchorSearchError::DelicacyLength {
                expected: 12,
                actual: 11
            })
        ));
        let (_, _, alien_surface) = box_parts(9.0);
        assert!(matches!(
            AnchorGeometryAuthority::new(&mesh, &topology, &alien_surface),
            Err(AnchorSearchError::SurfaceMeshMismatch { .. })
        ));
    }

    #[test]
    fn geometry_authority_rejects_distinct_triangles_with_identical_derived_metadata() {
        // Negating each centered vertex preserves this triangle's centroid,
        // AABB, normal, and area, while changing its exact triangle soup.
        // The old derived-metadata comparison accepted this collision.
        let source_vertices = vec![[-1.0, 0.0, 1.0], [0.0, 1.0, -1.0], [1.0, -1.0, 0.0]];
        let surface_vertices = vec![[1.0, 0.0, -1.0], [0.0, -1.0, 1.0], [-1.0, 1.0, 0.0]];
        let faces = vec![[0, 1, 2]];
        let mesh = Mesh::new(source_vertices, faces.clone()).unwrap();
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let surface = NativeSurface::from_indexed(
            &surface_vertices,
            &faces,
            NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: 64 * 1024 * 1024,
            },
        )
        .unwrap();

        let expected = NativeSurface::from_indexed(
            mesh.verts(),
            mesh.tris(),
            NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: 64 * 1024 * 1024,
            },
        )
        .unwrap();
        assert_eq!(
            expected.geometry().normals()[0].map(f64::to_bits),
            surface.geometry().normals()[0].map(f64::to_bits)
        );
        assert_eq!(
            expected.geometry().areas()[0].to_bits(),
            surface.geometry().areas()[0].to_bits()
        );
        assert_eq!(
            expected.geometry().centroids()[0].map(f64::to_bits),
            surface.geometry().centroids()[0].map(f64::to_bits)
        );
        assert_eq!(
            expected.geometry().face_bounds()[0].map(bits3),
            surface.geometry().face_bounds()[0].map(bits3)
        );
        assert!(matches!(
            AnchorGeometryAuthority::new(&mesh, &topology, &surface),
            Err(AnchorSearchError::SurfaceMeshMismatch { face: 0 })
        ));
    }

    #[test]
    fn thread_report_child() {
        if std::env::var_os("PLINTH_ANCHOR_REPORT_CHILD").is_none() {
            return;
        }
        println!("ANCHOR_REPORT:{}", deterministic_report());
    }

    #[test]
    fn actual_subprocess_reports_match_at_one_four_and_sixteen_threads() {
        let executable = std::env::current_exe().unwrap();
        let reports: Vec<String> = [1, 4, 16]
            .into_iter()
            .map(|threads| {
                let output = Command::new(&executable)
                    .arg("--exact")
                    .arg("anchor::tests::thread_report_child")
                    .arg("--nocapture")
                    .env("PLINTH_ANCHOR_REPORT_CHILD", "1")
                    .env("PLINTH_THREADS", threads.to_string())
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .find_map(|line| line.strip_prefix("ANCHOR_REPORT:").map(str::to_owned))
                    .expect("child report marker")
            })
            .collect();
        assert_eq!(reports[0], reports[1]);
        assert_eq!(reports[1], reports[2]);
    }

    fn deterministic_report() -> String {
        let (mesh, topology, surface) = box_parts(10.0);
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let (_, terminal, _) = bindings();
        let contact = contact(10.0);
        let sidecar = vec![0.25; 12];
        let config = AnchorSearchConfig::current().unwrap();
        let seats = discover_anchor_seats(
            &contact,
            terminal,
            &vec![sample(); 16],
            Some(&sidecar),
            config,
            &geometry,
        )
        .unwrap()
        .into_seats();
        let requests = seats
            .into_iter()
            .map(|seat| AnchorFootRequest::new(seat, p(0.35)))
            .collect();
        let validated =
            validate_anchor_feet(requests, &contact, terminal, config, &geometry).unwrap();
        let mut out = String::new();
        for outcome in validated.outcomes() {
            match outcome {
                AnchorFootOutcome::Validated(anchor) => {
                    out.push('V');
                    out.push_str(&format!(
                        "{:016x}{:016x}",
                        anchor.seat().first_hit_t().get().to_bits(),
                        anchor.foot().foot_ball_radius_mm().get().to_bits()
                    ));
                }
                AnchorFootOutcome::Refused(row) => out.push_str(&format!("R{:?}", row.reason())),
            }
        }
        out.push_str(":surface=");
        for byte in geometry.fingerprint.native_triangle_soup_digest.as_bytes() {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }
}
