//! Bound, batch-first consolidation of current minimum and island-birth seats.
//!
//! Generic perception owns every geometric observation.  This module accepts
//! only its opaque plan plus an opaque regular-contact selection, validates
//! both against the complete authority matrix, and returns an opaque physical
//! contact plan.  Logical obligations remain distinct from physical seats.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use geom_kernel::{
    KnnBatchLimits, KnnBatchPlan, NativeSurface, NativeSurfaceConfig, PointCloudIndex,
    RadiusQueryLimits,
};
use plinth_core::{m0_constants, LayerPlan};
use plinth_mesh::{build_face_topology, mesh_fingerprint, Mesh, WeldPolicy};
use thiserror::Error;

use crate::dyadic::{build_candidate_payload, CandidatePayloadConfig, DyadicError};
use crate::perception::{
    finalize_specialized_load_partition, load_violation_count, specialized_load_resource_plan,
    FinalizedContactLoads, FinalizedLoadPartition, GenericContactPlan, GenericPerceptionError,
    GenericPerceptionLimits, LoadFinalizeLimits, LoadOwner, PhysicalContact, SliceSolid,
    SpecializedOwnerReservation,
};
use crate::{
    assess_coverage, build_dyadic_coverage_samples, coverage_assessment_resource_plan, BuildFrame,
    ContactCandidateId, ContactCandidatePool, ContactSelectionConfig, ContactSelectionError,
    CoverageAssessmentLimits, CoverageObjective, CoverageSample, SelectedContact, SelectedContacts,
    SupportIsland, SupportPerceptionConfig,
};

const SYNTHETIC_PATCH_RADIUS_MM: f64 = 1.5;
const BIRTH_LINK_RADIUS_MM: f64 = 0.6;

/// Result-inert refusal limits for specialized orchestration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpecializedLimits {
    /// Caller-owned cap for the route-keyed coverage kNN batches.
    ///
    /// This is execution-only refusal authority, deliberately excluded from
    /// semantic and result digests.
    pub coverage_route_knn: KnnBatchLimits,
    pub max_synthetic_neighbor_pairs: usize,
    pub max_synthetic_neighbor_bytes: usize,
    pub max_satisfaction_tests: usize,
    pub max_satisfaction_neighbor_pairs: usize,
    pub max_satisfaction_neighbor_bytes: usize,
    pub max_merge_pairs: usize,
    pub max_merge_proposals: usize,
    pub max_merge_proposal_bytes: usize,
    pub max_merge_descriptor_operations: usize,
    pub max_merge_total_proposals: usize,
    pub max_merge_iterations: usize,
    pub max_merge_commit_bytes: usize,
    pub max_gate_variants: usize,
    pub max_gate_vector_bytes: usize,
    pub max_gate_contact_matrix_bytes: usize,
    pub max_gate_operations: usize,
    pub max_gate_window_variants: usize,
    pub max_coverage_scratch_bytes: usize,
    pub max_coverage_operations: usize,
    pub max_load_scratch_bytes: usize,
    pub max_load_operations: usize,
    pub max_gate_peak_bytes: usize,
}

impl SpecializedLimits {
    #[must_use]
    pub const fn production(coverage_route_knn: KnnBatchLimits) -> Self {
        Self {
            coverage_route_knn,
            max_synthetic_neighbor_pairs: 20_000_000,
            max_synthetic_neighbor_bytes: 512 * 1024 * 1024,
            max_satisfaction_tests: 100_000_000,
            max_satisfaction_neighbor_pairs: 100_000_000,
            max_satisfaction_neighbor_bytes: 2 * 1024 * 1024 * 1024,
            max_merge_pairs: 20_000_000,
            max_merge_proposals: 20_000_000,
            max_merge_proposal_bytes: 512 * 1024 * 1024,
            max_merge_descriptor_operations: 2_000_000_000,
            max_merge_total_proposals: 100_000_000,
            max_merge_iterations: 100_000,
            max_merge_commit_bytes: 512 * 1024 * 1024,
            max_gate_variants: 20_000_001,
            max_gate_vector_bytes: 512 * 1024 * 1024,
            max_gate_contact_matrix_bytes: 512 * 1024 * 1024,
            max_gate_operations: 2_000_000_000,
            max_gate_window_variants: 1_024,
            max_coverage_scratch_bytes: 512 * 1024 * 1024,
            max_coverage_operations: 2_000_000_000,
            max_load_scratch_bytes: 512 * 1024 * 1024,
            max_load_operations: 2_000_000_000,
            max_gate_peak_bytes: 1024 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecializedDemandKind {
    Minimum,
    IslandBirth,
}

/// Immutable logical obligation.  Fields are read-only so downstream stages
/// can audit routes without forging new geometric authority.
#[derive(Clone, Debug, PartialEq)]
pub struct SpecializedDemand {
    kind: SpecializedDemandKind,
    owner_island_id: u32,
    coverage_face_id: u32,
    coverage_component_id: u32,
    nominated_position_mm: [f64; 3],
    nominated_normal: [f64; 3],
    region_points_mm: Vec<[f64; 3]>,
    birth_z_mm: Option<f64>,
    birth_layer_index: Option<usize>,
    birth_footprint: Option<SliceSolid>,
}

impl SpecializedDemand {
    #[must_use]
    pub const fn kind(&self) -> SpecializedDemandKind {
        self.kind
    }
    #[must_use]
    pub const fn owner_island_id(&self) -> u32 {
        self.owner_island_id
    }
    #[must_use]
    pub const fn coverage_face_id(&self) -> u32 {
        self.coverage_face_id
    }
    #[must_use]
    pub const fn coverage_component_id(&self) -> u32 {
        self.coverage_component_id
    }
    #[must_use]
    pub const fn nominated_position_mm(&self) -> [f64; 3] {
        self.nominated_position_mm
    }
    #[must_use]
    pub const fn nominated_normal(&self) -> [f64; 3] {
        self.nominated_normal
    }
}

/// One physical seat, possibly carrying several logical obligations.
#[derive(Clone, Debug, PartialEq)]
pub struct SpecializedPhysicalContact {
    contact_id: u64,
    position_mm: [f64; 3],
    normal: [f64; 3],
    seat_provenance: PhysicalSeatProvenance,
    carried_area_mm2: f64,
    primary_regular: Option<PrimaryRegularIdentity>,
    regular_routes: Vec<RegularContactRoute>,
    demands: Vec<SpecializedDemand>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PrimaryRegularIdentity {
    route: RegularContactRoute,
    position_mm: [f64; 3],
    normal: [f64; 3],
}

/// The exact logical authority selected for a finalized physical seat.
///
/// A regular or shared seat retains the original regular identity. A
/// pure-specialized seat retains one deterministic representative demand;
/// it is deliberately not rewritten as a fabricated regular contact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PhysicalSeatProvenance {
    Regular(PrimaryRegularIdentity),
    Specialized(SpecializedSeatRepresentative),
}

/// Exact specialized demand retained as a physical-seat representative.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpecializedSeatRepresentative {
    kind: SpecializedDemandKind,
    owner_island_id: u32,
    coverage_face_id: u32,
    coverage_component_id: u32,
    nominated_position_mm: [f64; 3],
    nominated_normal: [f64; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RegularContactRoute {
    candidate_id: u64,
    owner_island_id: u32,
    coverage_face_id: u32,
    component_id: u32,
    source_kind: RepairRowSourceKind,
}

/// Which immutable candidate authority supplied a repair row.  A raw
/// candidate number is not enough to name a route-expanded repair seat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum RepairRowSourceKind {
    Generic,
    SyntheticIsland,
}

/// Opaque, stable identity of one route-expanded repair row carried in public
/// error evidence. It can be compared, hashed, and rendered for diagnostics,
/// but only the specialized planner may construct one. The owner, topology
/// component, and source remain part of the identity because one source
/// candidate can legally be available to more than one logical island.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepairRowId {
    owner_island_id: u32,
    candidate_id: ContactCandidateId,
    face_id: u32,
    component_id: u32,
    source_kind: RepairRowSourceKind,
}

/// One frozen candidate seat retained verbatim for a later regular-only
/// physical contact.  No later stage may recreate this geometry from ids.
#[derive(Clone, Debug, PartialEq)]
pub struct RepairRow {
    id: RepairRowId,
    order: usize,
    candidate: crate::ContactCandidate,
}

impl RepairRow {
    /// The planner-issued, source-qualified identity of this frozen row.
    #[must_use]
    pub const fn id(&self) -> RepairRowId {
        self.id
    }

    /// The stable repair-pool order established by the specialized pass.
    #[must_use]
    pub const fn order(&self) -> usize {
        self.order
    }

    /// Frozen source-seat geometry and score evidence.
    #[must_use]
    pub const fn candidate(&self) -> &crate::ContactCandidate {
        &self.candidate
    }
}

/// Read-only occupancy evidence for one frozen repair row. A demanded route is
/// occupied but must not be removed by a later repair controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RepairRowOccupancy {
    row_id: RepairRowId,
    shared_demanded: bool,
}

impl RepairRowOccupancy {
    /// The exact source-qualified occupied row.
    #[must_use]
    pub const fn row_id(&self) -> RepairRowId {
        self.row_id
    }

    /// Whether the row also serves a logical demanded region.
    #[must_use]
    pub const fn shared_demanded(&self) -> bool {
        self.shared_demanded
    }
}

/// One immutable, route-expanded repair authority spanning generic and
/// synthetic-island candidates produced by the single specialized pass.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RepairPool {
    lineage_root: u64,
    rows: Vec<RepairRow>,
    digest: u64,
}

impl RepairPool {
    fn build(
        lineage_root: u64,
        generic: &ContactCandidatePool,
        generic_islands: &[SupportIsland],
        synthetic: Option<&ContactCandidatePool>,
        synthetic_islands: &[SupportIsland],
        components: &[u32],
    ) -> Result<Self, SpecializedError> {
        let mut rows = Vec::new();
        Self::append_rows(
            &mut rows,
            generic.candidates(),
            generic_islands,
            components,
            RepairRowSourceKind::Generic,
        )?;
        if let Some(synthetic) = synthetic {
            Self::append_rows(
                &mut rows,
                synthetic.candidates(),
                synthetic_islands,
                components,
                RepairRowSourceKind::SyntheticIsland,
            )?;
        }
        let mut pool = Self {
            lineage_root,
            rows,
            digest: 0,
        };
        pool.digest = pool.compute_digest();
        Ok(pool)
    }

    fn append_rows(
        rows: &mut Vec<RepairRow>,
        candidates: &[crate::ContactCandidate],
        islands: &[SupportIsland],
        components: &[u32],
        source_kind: RepairRowSourceKind,
    ) -> Result<(), SpecializedError> {
        for island in islands {
            for candidate in candidates {
                if island.face_ids.binary_search(&candidate.face_id).is_err() {
                    continue;
                }
                let component_id = *components
                    .get(candidate.face_id as usize)
                    .ok_or(SpecializedError::InvalidRepairRow)?;
                let id = RepairRowId {
                    owner_island_id: island.id,
                    candidate_id: candidate.id,
                    face_id: candidate.face_id,
                    component_id,
                    source_kind,
                };
                if rows.iter().any(|row| row.id == id) {
                    return Err(SpecializedError::DuplicateRepairRow { id });
                }
                let order = rows.len();
                rows.push(RepairRow {
                    id,
                    order,
                    candidate: candidate.clone(),
                });
            }
        }
        Ok(())
    }

    fn row(&self, id: RepairRowId) -> Option<&RepairRow> {
        self.rows.get(id_index(&self.rows, id)?)
    }

    fn resolve_route(&self, route: RegularContactRoute) -> Result<RepairRowId, SpecializedError> {
        let mut matching = None;
        for row in &self.rows {
            if row.id.owner_island_id == route.owner_island_id
                && row.id.candidate_id.0 == route.candidate_id
                && row.id.face_id == route.coverage_face_id
                && row.id.component_id == route.component_id
                && row.id.source_kind == route.source_kind
                && matching.replace(row.id).is_some()
            {
                return Err(SpecializedError::AmbiguousRepairRoute { route });
            }
        }
        matching.ok_or(SpecializedError::MissingRepairRoute { route })
    }

    fn validate_contacts(
        &self,
        contacts: &[SpecializedPhysicalContact],
    ) -> Result<(), SpecializedError> {
        if self.compute_digest() != self.digest {
            return Err(SpecializedError::StaleRepairPool);
        }
        let mut occupied = BTreeSet::new();
        for contact in contacts {
            for &route in contact.regular_routes() {
                let id = self.resolve_route(route)?;
                if !occupied.insert(id) {
                    return Err(SpecializedError::DuplicateRepairRoute { id });
                }
            }
        }
        Ok(())
    }

    fn occupied_for_contacts(
        &self,
        contacts: &[SpecializedPhysicalContact],
    ) -> Result<Vec<RepairRowOccupancy>, SpecializedError> {
        self.validate_contacts(contacts)?;
        let mut rows = Vec::new();
        for contact in contacts {
            for &route in contact.regular_routes() {
                rows.push(RepairRowOccupancy {
                    row_id: self.resolve_route(route)?,
                    shared_demanded: !contact.demands().is_empty(),
                });
            }
        }
        rows.sort_unstable_by_key(|row| self.row(row.row_id).map_or(usize::MAX, RepairRow::order));
        Ok(rows)
    }

    fn eligible_rows(
        &self,
        island_id: u32,
        component_id: u32,
        excluded: &BTreeSet<RepairRowId>,
    ) -> Vec<&RepairRow> {
        self.rows
            .iter()
            .filter(|row| {
                row.id.owner_island_id == island_id
                    && row.id.component_id == component_id
                    && !excluded.contains(&row.id)
            })
            .collect()
    }

    fn validate_lineage(&self, lineage_root: u64) -> Result<(), SpecializedError> {
        if self.lineage_root != lineage_root || self.compute_digest() != self.digest {
            return Err(SpecializedError::StaleRepairPool);
        }
        Ok(())
    }

    fn compute_digest(&self) -> u64 {
        let mut hash = Fnv::new();
        hash.u64(self.lineage_root);
        for row in &self.rows {
            hash.usize(row.order);
            hash.u32(row.id.owner_island_id);
            hash.u64(row.id.candidate_id.0);
            hash.u32(row.id.face_id);
            hash.u32(row.id.component_id);
            hash.u8(match row.id.source_kind {
                RepairRowSourceKind::Generic => 0,
                RepairRowSourceKind::SyntheticIsland => 1,
            });
            hash.u64(row.candidate.id.0);
            hash.u32(row.candidate.face_id);
            hash.point3(row.candidate.point_mm);
            hash.point3(row.candidate.normal);
            hash.f64(row.candidate.supported_area_mm2);
            hash.f64(row.candidate.overhang_excess);
            hash.f64(row.candidate.delicacy);
        }
        hash.finish()
    }

    fn hash_into(&self, hash: &mut Fnv) {
        hash.u64(self.lineage_root);
        hash.u64(self.digest);
        for row in &self.rows {
            hash.usize(row.order);
            hash.u32(row.id.owner_island_id);
            hash.u64(row.id.candidate_id.0);
            hash.u32(row.id.face_id);
            hash.u32(row.id.component_id);
            hash.u8(match row.id.source_kind {
                RepairRowSourceKind::Generic => 0,
                RepairRowSourceKind::SyntheticIsland => 1,
            });
            hash.u64(row.candidate.id.0);
            hash.u32(row.candidate.face_id);
            hash.point3(row.candidate.point_mm);
            hash.point3(row.candidate.normal);
            hash.f64(row.candidate.supported_area_mm2);
            hash.f64(row.candidate.overhang_excess);
            hash.f64(row.candidate.delicacy);
        }
    }
}

fn id_index(rows: &[RepairRow], id: RepairRowId) -> Option<usize> {
    rows.iter().position(|row| row.id == id)
}

impl SpecializedPhysicalContact {
    #[must_use]
    pub const fn contact_id(&self) -> u64 {
        self.contact_id
    }
    #[must_use]
    pub const fn position_mm(&self) -> [f64; 3] {
        self.position_mm
    }
    #[must_use]
    pub const fn normal(&self) -> [f64; 3] {
        self.normal
    }
    #[must_use]
    pub const fn seat_provenance(&self) -> PhysicalSeatProvenance {
        self.seat_provenance
    }
    #[must_use]
    pub const fn carried_area_mm2(&self) -> f64 {
        self.carried_area_mm2
    }
    #[must_use]
    pub fn demands(&self) -> &[SpecializedDemand] {
        &self.demands
    }
    #[must_use]
    pub fn regular_routes(&self) -> &[RegularContactRoute] {
        &self.regular_routes
    }
    #[must_use]
    pub const fn primary_regular(&self) -> Option<PrimaryRegularIdentity> {
        self.primary_regular
    }
    #[must_use]
    pub const fn is_shared_regular(&self) -> bool {
        !self.regular_routes.is_empty() && !self.demands.is_empty()
    }
}

impl PhysicalSeatProvenance {
    #[must_use]
    pub const fn regular_identity(self) -> Option<PrimaryRegularIdentity> {
        match self {
            Self::Regular(identity) => Some(identity),
            Self::Specialized(_) => None,
        }
    }
}

impl SpecializedSeatRepresentative {
    #[must_use]
    pub const fn kind(self) -> SpecializedDemandKind {
        self.kind
    }
    #[must_use]
    pub const fn owner_island_id(self) -> u32 {
        self.owner_island_id
    }
    #[must_use]
    pub const fn coverage_face_id(self) -> u32 {
        self.coverage_face_id
    }
    #[must_use]
    pub const fn coverage_component_id(self) -> u32 {
        self.coverage_component_id
    }
    #[must_use]
    pub const fn nominated_position_mm(self) -> [f64; 3] {
        self.nominated_position_mm
    }
    #[must_use]
    pub const fn nominated_normal(self) -> [f64; 3] {
        self.nominated_normal
    }
}

impl RegularContactRoute {
    #[must_use]
    pub const fn candidate_id(self) -> u64 {
        self.candidate_id
    }
    #[must_use]
    pub const fn owner_island_id(self) -> u32 {
        self.owner_island_id
    }
    #[must_use]
    pub const fn coverage_face_id(self) -> u32 {
        self.coverage_face_id
    }
    #[must_use]
    pub const fn component_id(self) -> u32 {
        self.component_id
    }
    #[must_use]
    pub(crate) const fn source_kind(self) -> RepairRowSourceKind {
        self.source_kind
    }
}

impl PrimaryRegularIdentity {
    #[must_use]
    pub const fn route(self) -> RegularContactRoute {
        self.route
    }
    #[must_use]
    pub const fn position_mm(self) -> [f64; 3] {
        self.position_mm
    }
    #[must_use]
    pub const fn normal(self) -> [f64; 3] {
        self.normal
    }
}

/// Opaque output bound to generic-plan, regular-contact, mesh and layer
/// authority.  It includes the one finalized unique-face load partition.
#[derive(Clone, Debug, PartialEq)]
pub struct SpecializedContactPlan {
    // The original selected-contact authority is retained separately from the
    // child plan's own digest. A child may change physical contacts but may
    // never exchange the generic/load-finalizer inputs that certify them.
    source_authority_digest: u64,
    semantic_authority_digest: u64,
    generic_slice_thread_evidence: (usize, usize),
    islands: Vec<SupportIsland>,
    coverage_samples: Vec<CoverageSample>,
    repair_pool: RepairPool,
    generic_authority: GenericContactPlan,
    mesh_authority: Mesh,
    mesh_authority_fingerprint: String,
    reservations_authority: Vec<SpecializedOwnerReservation>,
    limits_authority: SpecializedLimits,
    surface_config_authority: NativeSurfaceConfig,
    contacts: Vec<SpecializedPhysicalContact>,
    // Allocation history is append-only even when the current geometry has
    // holes. `None` means the u64 namespace is exhausted.
    next_contact_id: Option<u64>,
    load_partition: FinalizedLoadPartition,
    finalized_loads: FinalizedContactLoads,
    // This is intentionally separate from `digest()`: including it in the
    // digest would be circular.  It seals the finalized load artifact to the
    // complete plan authority which produced it.
    finalized_load_plan_digest: u64,
    gate_evidence: SpecializedGateEvidence,
}

/// A checked, immutable edit session for a specialized plan.
///
/// The session owns the one native surface required by the authoritative load
/// finalizer.  It deliberately exposes only regular-contact removal/addition:
/// demand-bearing geometry is not normalization authority.
pub struct SpecializedContactPlanEdit<'a> {
    plan: &'a SpecializedContactPlan,
    surface: NativeSurface,
}

impl SpecializedContactPlan {
    #[must_use]
    pub fn islands(&self) -> &[SupportIsland] {
        &self.islands
    }
    #[must_use]
    pub fn coverage_samples(&self) -> &[CoverageSample] {
        &self.coverage_samples
    }

    /// Retrieves a verbatim planner-frozen repair row by its opaque,
    /// source-qualified identity. The returned row is read-only.
    #[must_use]
    pub fn repair_row(&self, id: RepairRowId) -> Option<&RepairRow> {
        self.repair_pool.row(id)
    }

    /// Enumerates occupied immutable rows in stable pool order, including
    /// whether each row is also shared with a demanded logical region.
    pub fn occupied_repair_rows(&self) -> Result<Vec<RepairRowOccupancy>, SpecializedError> {
        self.repair_pool.occupied_for_contacts(&self.contacts)
    }

    /// Returns stable-order rows eligible for a repair controller. Exact
    /// source-qualified occupied and caller-recorded failed identities are
    /// excluded; zero-score candidates remain eligible.
    pub fn eligible_repair_rows(
        &self,
        island_id: u32,
        component_id: u32,
        failed: &BTreeSet<RepairRowId>,
    ) -> Result<Vec<&RepairRow>, SpecializedError> {
        let mut excluded = failed.clone();
        excluded.extend(
            self.occupied_repair_rows()?
                .into_iter()
                .map(|occupancy| occupancy.row_id),
        );
        Ok(self
            .repair_pool
            .eligible_rows(island_id, component_id, &excluded))
    }

    #[must_use]
    pub fn contacts(&self) -> &[SpecializedPhysicalContact] {
        &self.contacts
    }
    #[must_use]
    pub const fn load_partition(&self) -> &FinalizedLoadPartition {
        &self.load_partition
    }

    /// The exact plan-owned, tip-sized fold of [`Self::load_partition`].
    /// Crate-private integration may borrow it only after the plan digest has
    /// been checked; public callers cannot manufacture an alternate pairing.
    pub(crate) fn finalized_loads(&self) -> Result<&FinalizedContactLoads, SpecializedError> {
        let contact_ids = self
            .contacts
            .iter()
            .map(SpecializedPhysicalContact::contact_id)
            .collect::<Vec<_>>();
        self.finalized_loads
            .validate_binding(&self.load_partition, &contact_ids)
            .map_err(SpecializedError::Generic)?;
        if self.finalized_load_plan_digest != self.digest() {
            return Err(SpecializedError::StaleFinalizedLoadArtifact);
        }
        Ok(&self.finalized_loads)
    }
    #[must_use]
    pub const fn gate_evidence(&self) -> SpecializedGateEvidence {
        self.gate_evidence
    }
    #[must_use]
    pub const fn generic_slice_thread_evidence(&self) -> (usize, usize) {
        self.generic_slice_thread_evidence
    }

    /// Start a checked child-plan edit using only the authority frozen in this
    /// parent. No caller can substitute reservations, limits, mesh, generic
    /// evidence, or native surface configuration beneath a child plan.
    pub fn begin_regular_edit(&self) -> Result<SpecializedContactPlanEdit<'_>, SpecializedError> {
        self.validate_edit_binding()?;
        let surface = NativeSurface::from_indexed(
            self.mesh_authority.verts(),
            self.mesh_authority.tris(),
            self.surface_config_authority,
        )
        .map_err(|error| SpecializedError::Surface(error.to_string()))?;
        Ok(SpecializedContactPlanEdit {
            plan: self,
            surface,
        })
    }

    /// The digest of the plan to which the current finalized load partition is
    /// sealed.  Consumers must treat a mismatch as stale load authority.
    #[must_use]
    pub const fn finalized_load_plan_digest(&self) -> u64 {
        self.finalized_load_plan_digest
    }

    /// Reject replay against any changed generic, selected-contact, selection
    /// policy or result-inert refusal authority.
    #[allow(clippy::too_many_arguments)]
    pub fn validate_binding(
        &self,
        mesh: &Mesh,
        perception_config: &SupportPerceptionConfig<'_>,
        frame: BuildFrame,
        layers: &LayerPlan,
        candidate_config: &CandidatePayloadConfig,
        reservations: &[SpecializedOwnerReservation],
        perception_limits: GenericPerceptionLimits,
        generic: &GenericContactPlan,
        regular: &SelectedContacts,
        selection_config: &ContactSelectionConfig<'_>,
        limits: SpecializedLimits,
    ) -> Result<(), SpecializedError> {
        if mesh != &self.mesh_authority
            || generic != &self.generic_authority
            || reservations != self.reservations_authority
            || limits != self.limits_authority
            || perception_config.surface_grid_pitch_mm.get()
                != self.surface_config_authority.grid_pitch_mm
            || perception_config.max_grid_bytes != self.surface_config_authority.max_grid_bytes
        {
            return Err(SpecializedError::StaleAuthority);
        }
        generic.validate_binding(
            mesh,
            perception_config,
            frame,
            layers,
            candidate_config,
            reservations,
            perception_limits,
        )?;
        validate_regular(mesh, generic, regular, selection_config)?;
        self.repair_pool
            .validate_lineage(repair_pool_lineage_root(generic))?;
        self.repair_pool.validate_contacts(&self.contacts)?;
        self.finalized_loads()?;
        if self.finalized_load_plan_digest != self.digest() {
            return Err(SpecializedError::StaleFinalizedLoadArtifact);
        }
        if bound_authority_digest(generic, regular, selection_config, limits)
            != self.source_authority_digest
        {
            return Err(SpecializedError::StaleAuthority);
        }
        Ok(())
    }

    fn validate_edit_binding(&self) -> Result<(), SpecializedError> {
        let live_mesh_fingerprint = mesh_fingerprint(&self.mesh_authority)
            .map_err(|error| SpecializedError::Topology(error.to_string()))?;
        if live_mesh_fingerprint != self.mesh_authority_fingerprint {
            return Err(SpecializedError::FrozenMeshAuthorityMismatch);
        }
        self.repair_pool
            .validate_lineage(repair_pool_lineage_root(&self.generic_authority))?;
        self.repair_pool.validate_contacts(&self.contacts)?;
        self.finalized_loads()?;
        if self.finalized_load_plan_digest != self.digest() {
            return Err(SpecializedError::StaleFinalizedLoadArtifact);
        }
        Ok(())
    }

    fn seal_finalized_load_digest(mut self) -> Self {
        self.finalized_load_plan_digest = self.digest();
        self
    }
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut hash = Fnv::new();
        hash.u64(self.semantic_authority_digest);
        // Bind the complete frozen child-finalization input bundle. The
        // original source authority remains auditable above, while this plan
        // digest changes whenever a child-relevant input changes.
        hash.u64(self.generic_authority.digest());
        hash.bytes(self.mesh_authority_fingerprint.as_bytes());
        for reservation in &self.reservations_authority {
            hash.u32(reservation.owner_id);
            hash.u32(reservation.face_id);
        }
        hash_specialized_limits(&mut hash, self.limits_authority);
        hash.f64(self.surface_config_authority.grid_pitch_mm);
        hash.usize(self.surface_config_authority.max_grid_bytes);
        match self.next_contact_id {
            Some(next) => {
                hash.u8(1);
                hash.u64(next);
            }
            None => hash.u8(0),
        }
        self.repair_pool.hash_into(&mut hash);
        for island in &self.islands {
            hash.u32(island.id);
            for &face in &island.face_ids {
                hash.u32(face);
            }
        }
        for sample in &self.coverage_samples {
            hash.u32(sample.island_id);
            hash.u32(sample.face_id);
            hash.u32(sample.component_id);
            hash.point3(sample.point_mm);
            hash.f64(sample.area_mm2);
        }
        for contact in &self.contacts {
            hash.u64(contact.contact_id);
            hash.point3(contact.position_mm);
            hash.point3(contact.normal);
            hash_physical_seat_provenance(&mut hash, contact.seat_provenance);
            hash.f64(contact.carried_area_mm2);
            if let Some(primary) = contact.primary_regular {
                hash.u8(1);
                hash.u64(primary.route.candidate_id);
                hash.u32(primary.route.owner_island_id);
                hash.u32(primary.route.coverage_face_id);
                hash.u32(primary.route.component_id);
                hash_repair_row_source_kind(&mut hash, primary.route.source_kind);
                hash.point3(primary.position_mm);
                hash.point3(primary.normal);
            } else {
                hash.u8(0);
            }
            for route in &contact.regular_routes {
                hash.u64(route.candidate_id);
                hash.u32(route.owner_island_id);
                hash.u32(route.coverage_face_id);
                hash.u32(route.component_id);
                hash_repair_row_source_kind(&mut hash, route.source_kind);
            }
            for demand in &contact.demands {
                hash.u8(match demand.kind {
                    SpecializedDemandKind::Minimum => 0,
                    SpecializedDemandKind::IslandBirth => 1,
                });
                hash.u32(demand.owner_island_id);
                hash.u32(demand.coverage_face_id);
                hash.u32(demand.coverage_component_id);
                hash.point3(demand.nominated_position_mm);
                hash.point3(demand.nominated_normal);
            }
        }
        for row in self.load_partition.rows() {
            hash.u32(row.face_id);
            hash.u64(row.contact_id);
            match row.owner {
                LoadOwner::Specialized(owner) => {
                    hash.u8(0);
                    hash.u32(owner);
                }
                LoadOwner::GenericIsland(owner) => {
                    hash.u8(1);
                    hash.u32(owner);
                }
            }
            hash.f64(row.carried_area_mm2);
        }
        for vector in [self.gate_evidence.baseline, self.gate_evidence.accepted] {
            hash.f64(vector.failing_uncovered_area_mm2);
            hash.f64(vector.max_patch_area_mm2);
            hash.usize(vector.failing_patch_count);
            hash.f64(vector.worst_gap_mm);
            hash.usize(vector.load_violation_count);
            hash.usize(vector.unsupported_specialized_count);
        }
        hash.usize(self.gate_evidence.evaluated_pair_count);
        hash.usize(self.gate_evidence.accepted_pair_count);
        hash.usize(self.gate_evidence.deep_clone_count);
        hash.usize(self.gate_evidence.max_commit_bytes);
        hash.finish()
    }
}

impl<'a> SpecializedContactPlanEdit<'a> {
    /// Derive a child that omits exactly one demand-free regular physical
    /// contact.  The original contact vector is cloned and filtered; surviving
    /// records are never reconsolidated, re-numbered, or rebuilt from a
    /// candidate.  Consequently removed ids leave observable holes.
    pub fn remove_demand_free_regular(
        &self,
        contact_id: u64,
    ) -> Result<SpecializedContactPlan, SpecializedError> {
        let contact = self
            .plan
            .contacts
            .iter()
            .find(|contact| contact.contact_id == contact_id)
            .ok_or(SpecializedError::UnknownPhysicalContact { contact_id })?;
        if !contact.demands.is_empty() {
            return Err(SpecializedError::DemandBearingContactEdit { contact_id });
        }
        if contact.primary_regular.is_none() || contact.regular_routes.is_empty() {
            return Err(SpecializedError::NonRegularContactEdit { contact_id });
        }
        let contacts = self
            .plan
            .contacts
            .iter()
            .filter(|contact| contact.contact_id != contact_id)
            .cloned()
            .collect();
        self.derive_child(contacts, self.plan.next_contact_id)
    }

    /// Derive a child that appends one previously unoccupied repair row.  A
    /// fresh id is allocated strictly above every id that has ever appeared in
    /// this lineage, including holes left by removal.  This method is the only
    /// regular-contact addition seam; its geometry and route are copied from
    /// the immutable repair pool rather than caller data.
    pub fn append_regular_repair_row(
        &self,
        row_id: RepairRowId,
    ) -> Result<SpecializedContactPlan, SpecializedError> {
        let row = self
            .plan
            .repair_pool
            .row(row_id)
            .ok_or(SpecializedError::UnknownRepairRow { row_id })?;
        if self
            .plan
            .occupied_repair_rows()?
            .iter()
            .any(|occupancy| occupancy.row_id == row_id)
        {
            return Err(SpecializedError::OccupiedRepairRow { row_id });
        }
        if self
            .plan
            .contacts
            .iter()
            .any(|contact| contact.position_mm == row.candidate.point_mm)
        {
            return Err(SpecializedError::DuplicateChildPhysicalSeat {
                contact_id: row.candidate.id.0,
            });
        }
        let contact_id = self
            .plan
            .next_contact_id
            .ok_or(SpecializedError::PhysicalContactIdOverflow)?;
        let route = RegularContactRoute {
            candidate_id: row.id.candidate_id.0,
            owner_island_id: row.id.owner_island_id,
            coverage_face_id: row.id.face_id,
            component_id: row.id.component_id,
            source_kind: row.id.source_kind,
        };
        let primary = PrimaryRegularIdentity {
            route,
            position_mm: row.candidate.point_mm,
            normal: row.candidate.normal,
        };
        let mut contacts = self.plan.contacts.clone();
        contacts.push(SpecializedPhysicalContact {
            contact_id,
            position_mm: row.candidate.point_mm,
            normal: row.candidate.normal,
            seat_provenance: PhysicalSeatProvenance::Regular(primary),
            carried_area_mm2: 0.0,
            primary_regular: Some(primary),
            regular_routes: vec![route],
            demands: Vec::new(),
        });
        self.derive_child(contacts, contact_id.checked_add(1))
    }

    fn derive_child(
        &self,
        mut contacts: Vec<SpecializedPhysicalContact>,
        next_contact_id: Option<u64>,
    ) -> Result<SpecializedContactPlan, SpecializedError> {
        // An edit never runs selected-to-specialized consolidation.  It only
        // proves the retained records and replays the one authoritative load
        // finalizer over them.
        validate_child_contacts(&contacts, &self.plan.repair_pool)?;
        for contact in &mut contacts {
            contact.carried_area_mm2 = 0.0;
        }
        let load_inputs = stable_load_inputs(
            &contacts,
            &self.plan.generic_authority,
            &self.plan.islands,
            &self.plan.reservations_authority,
            self.plan.limits_authority,
        )?;
        let load_partition = finalize_specialized_load_partition(
            &self.plan.generic_authority,
            &self.plan.mesh_authority,
            &self.surface,
            &load_inputs.contacts,
            &load_inputs.specialized_owner_faces,
            LoadFinalizeLimits {
                max_scratch_bytes: self.plan.limits_authority.max_load_scratch_bytes,
                max_operations: load_inputs.remaining_load_operations,
            },
        )?;
        for row in load_partition.rows() {
            let contact = contacts
                .iter_mut()
                .find(|contact| contact.contact_id == row.contact_id)
                .ok_or(SpecializedError::MissingSeatProvenance {
                    contact_id: row.contact_id,
                })?;
            contact.carried_area_mm2 += row.carried_area_mm2;
        }
        self.plan.repair_pool.validate_contacts(&contacts)?;
        let contact_ids = contacts
            .iter()
            .map(SpecializedPhysicalContact::contact_id)
            .collect::<Vec<_>>();
        let finalized_loads =
            FinalizedContactLoads::for_specialized_plan(&load_partition, &contact_ids)?;
        Ok(SpecializedContactPlan {
            source_authority_digest: self.plan.source_authority_digest,
            semantic_authority_digest: self.plan.semantic_authority_digest,
            generic_slice_thread_evidence: self.plan.generic_slice_thread_evidence,
            islands: self.plan.islands.clone(),
            coverage_samples: self.plan.coverage_samples.clone(),
            repair_pool: self.plan.repair_pool.clone(),
            generic_authority: self.plan.generic_authority.clone(),
            mesh_authority: self.plan.mesh_authority.clone(),
            mesh_authority_fingerprint: self.plan.mesh_authority_fingerprint.clone(),
            reservations_authority: self.plan.reservations_authority.clone(),
            limits_authority: self.plan.limits_authority,
            surface_config_authority: self.plan.surface_config_authority,
            contacts,
            next_contact_id,
            load_partition,
            finalized_loads,
            finalized_load_plan_digest: 0,
            gate_evidence: self.plan.gate_evidence,
        }
        .seal_finalized_load_digest())
    }
}

fn next_contact_id(contacts: &[SpecializedPhysicalContact]) -> Option<u64> {
    contacts
        .iter()
        .map(SpecializedPhysicalContact::contact_id)
        .max()
        .map_or(Some(0), |id| id.checked_add(1))
}

fn validate_child_contacts(
    contacts: &[SpecializedPhysicalContact],
    repair_pool: &RepairPool,
) -> Result<(), SpecializedError> {
    let mut ids = BTreeSet::new();
    for contact in contacts {
        if !ids.insert(contact.contact_id) {
            return Err(SpecializedError::DuplicateChildContactId {
                contact_id: contact.contact_id,
            });
        }
        match contact.primary_regular {
            Some(primary)
                if contact.regular_routes.contains(&primary.route)
                    && contact.position_mm == primary.position_mm
                    && contact.normal == primary.normal => {}
            None if contact.regular_routes.is_empty() => {}
            _ => return Err(SpecializedError::PrimaryRegularIdentityMismatch),
        }
    }
    for right in 0..contacts.len() {
        if contacts[..right]
            .iter()
            .any(|left| left.position_mm == contacts[right].position_mm)
        {
            return Err(SpecializedError::DuplicateChildPhysicalSeat {
                contact_id: contacts[right].contact_id,
            });
        }
    }
    repair_pool.validate_contacts(contacts)
}

fn stable_load_inputs(
    contacts: &[SpecializedPhysicalContact],
    generic: &GenericContactPlan,
    islands: &[SupportIsland],
    reservations: &[SpecializedOwnerReservation],
    limits: SpecializedLimits,
) -> Result<WorkingLoadInputs, SpecializedError> {
    let working = contacts
        .iter()
        .map(|contact| WorkingContact {
            position_mm: contact.position_mm,
            normal: contact.normal,
            seat_provenance: contact.seat_provenance,
            primary_regular: contact.primary_regular,
            regular_routes: contact.regular_routes.clone(),
            demands: contact.demands.clone(),
        })
        .collect::<Vec<_>>();
    let mut inputs = working_load_inputs(&working, generic, islands, reservations, limits)?;
    let first_specialized_owner =
        reservations
            .iter()
            .map(|row| row.owner_id)
            .max()
            .map_or(Ok(0), |owner| {
                owner
                    .checked_add(1)
                    .ok_or(SpecializedError::OwnerIdOverflow)
            })?;
    let stable_id_for_dense = |dense: u32| -> Result<u64, SpecializedError> {
        contacts
            .get(dense as usize)
            .map(SpecializedPhysicalContact::contact_id)
            .ok_or(SpecializedError::OwnerIdOverflow)
    };
    let stable_specialized_owner = |owner: u32| -> Result<u32, SpecializedError> {
        let dense = owner
            .checked_sub(first_specialized_owner)
            .ok_or(SpecializedError::OwnerIdOverflow)?;
        let stable = u32::try_from(stable_id_for_dense(dense)?)
            .map_err(|_| SpecializedError::OwnerIdOverflow)?;
        first_specialized_owner
            .checked_add(stable)
            .ok_or(SpecializedError::OwnerIdOverflow)
    };
    for physical in &mut inputs.contacts {
        let dense =
            u32::try_from(physical.contact_id).map_err(|_| SpecializedError::OwnerIdOverflow)?;
        physical.contact_id = stable_id_for_dense(dense)?;
        if let LoadOwner::Specialized(owner) = physical.owner {
            physical.owner = LoadOwner::Specialized(stable_specialized_owner(owner)?);
        }
    }
    for (owner, _) in &mut inputs.specialized_owner_faces {
        *owner = stable_specialized_owner(*owner)?;
    }
    Ok(inputs)
}

/// Frozen gate vector used by the one batched merge verdict.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrozenSpecializedGateVector {
    pub failing_uncovered_area_mm2: f64,
    pub max_patch_area_mm2: f64,
    pub failing_patch_count: usize,
    pub worst_gap_mm: f64,
    pub load_violation_count: usize,
    pub unsupported_specialized_count: usize,
}

impl FrozenSpecializedGateVector {
    fn from_live(
        coverage: CoverageObjective,
        load_violation_count: usize,
        unsupported_specialized_count: usize,
    ) -> Self {
        Self {
            failing_uncovered_area_mm2: coverage.failing_uncovered_area_mm2,
            max_patch_area_mm2: coverage.max_patch_area_mm2,
            failing_patch_count: coverage.failing_patch_count,
            worst_gap_mm: coverage.worst_gap_mm,
            load_violation_count,
            unsupported_specialized_count,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpecializedGateEvidence {
    pub baseline: FrozenSpecializedGateVector,
    pub accepted: FrozenSpecializedGateVector,
    pub evaluated_pair_count: usize,
    pub accepted_pair_count: usize,
    /// Seats removed by the terminal exactly-coincident regular coalescing
    /// pass.  That pass is canonicalization, not the gated merge tournament,
    /// so its count is reported separately and never folded into
    /// `accepted_pair_count`.  Every coalesced seat still commits through
    /// `commit_merge`, so the sealed digest binds the work through
    /// `deep_clone_count` and `max_commit_bytes`.
    pub coalesced_seat_count: usize,
    pub deep_clone_count: usize,
    pub max_commit_bytes: usize,
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum SpecializedError {
    #[error(transparent)]
    Generic(#[from] GenericPerceptionError),
    #[error(transparent)]
    Selection(#[from] ContactSelectionError),
    #[error(transparent)]
    Dyadic(#[from] DyadicError),
    #[error("specialized contact could not build topology: {0}")]
    Topology(String),
    #[error("specialized contact could not build the canonical surface: {0}")]
    Surface(String),
    #[error("regular contact evidence is stale or does not belong to the generic plan")]
    StaleRegularContacts,
    #[error("specialized contact primary regular identity is inconsistent")]
    PrimaryRegularIdentityMismatch,
    #[error("specialized contact authority is stale or belongs to another input")]
    StaleAuthority,
    #[error("the immutable repair pool is stale or its lineage does not match this plan")]
    StaleRepairPool,
    #[error("synthetic candidate row cannot be represented as an identity")]
    SyntheticCandidateIdOverflow,
    #[error("repair pool contains an invalid source row")]
    InvalidRepairRow,
    #[error("repair pool contains a duplicate route-expanded row {id:?}")]
    DuplicateRepairRow { id: RepairRowId },
    #[error("regular route {route:?} does not map to an immutable repair row")]
    MissingRepairRoute { route: RegularContactRoute },
    #[error("regular route {route:?} maps to more than one immutable repair row")]
    AmbiguousRepairRoute { route: RegularContactRoute },
    #[error("more than one physical contact occupies immutable repair row {id:?}")]
    DuplicateRepairRoute { id: RepairRowId },
    #[error("specialized projection returned an invalid face")]
    InvalidProjection,
    #[error("specialized satisfaction work requires {required} tests; limit is {limit}")]
    SatisfactionLimit { required: usize, limit: usize },
    #[error("specialized satisfaction neighbor query failed: {0}")]
    SatisfactionNeighbors(String),
    #[error("specialized merge work requires {required} pairs; limit is {limit}")]
    MergePairLimit { required: usize, limit: usize },
    #[error("specialized merge produced {required} proposals; limit is {limit}")]
    MergeProposalLimit { required: usize, limit: usize },
    #[error("specialized merge descriptors require {required} bytes; limit is {limit}")]
    MergeProposalByteLimit { required: usize, limit: usize },
    #[error(
        "specialized merge descriptor generation requires {required} operations; limit is {limit}"
    )]
    MergeDescriptorOperationLimit { required: usize, limit: usize },
    #[error("specialized merge evaluated {required} total proposals; limit is {limit}")]
    MergeTotalProposalLimit { required: usize, limit: usize },
    #[error("specialized merge requires {required} iterations; limit is {limit}")]
    MergeIterationLimit { required: usize, limit: usize },
    #[error("specialized merge commit requires {required} deep bytes; limit is {limit}")]
    MergeCommitByteLimit { required: usize, limit: usize },
    #[error("specialized gate requires {required} variants; limit is {limit}")]
    GateVariantLimit { required: usize, limit: usize },
    #[error("specialized gate vector requires {required} bytes; limit is {limit}")]
    GateVectorByteLimit { required: usize, limit: usize },
    #[error("specialized gate contact window requires {required} bytes; limit is {limit}")]
    GateContactMatrixByteLimit { required: usize, limit: usize },
    #[error("specialized gate peak requires {required} bytes; limit is {limit}")]
    GatePeakByteLimit { required: usize, limit: usize },
    #[error("specialized gate requires {required} operations; limit is {limit}")]
    GateOperationLimit { required: usize, limit: usize },
    #[error("specialized gate window size must be positive")]
    InvalidGateWindow,
    #[error("specialized gate resource arithmetic overflowed")]
    GateResourceOverflow,
    #[error("specialized gate route preflight did not match execution planning")]
    GateRoutePlanMismatch,
    #[error("specialized route-keyed coverage nearest query failed: {0}")]
    CoverageNearest(String),
    #[error("specialized synthetic neighbor query failed: {0}")]
    SyntheticNeighbors(String),
    #[error("specialized demand has no component-local synthetic face")]
    MissingSyntheticPatch,
    #[error("specialized demand {demand_index} has no satisfying physical representative")]
    UnsatisfiedDemand { demand_index: usize },
    #[error("specialized contacts {left} and {right} retain a duplicate physical seat")]
    DuplicatePhysicalSeat { left: usize, right: usize },
    #[error("specialized contacts {left} and {right} retain a compatible unmerged pair")]
    UnmergedCompatiblePair { left: usize, right: usize },
    #[error("the combined specialized merge batch worsened a frozen gate")]
    MergeGateRegression,
    #[error("specialized demand owner {island_id} does not contain routed face {face_id}")]
    OwnerRouteMismatch { island_id: u32, face_id: u32 },
    #[error("specialized physical owner id overflowed u32")]
    OwnerIdOverflow,
    #[error("specialized physical contact {contact_id} has no honest seat provenance")]
    MissingSeatProvenance { contact_id: u64 },
    #[error("specialized plan edit names unknown physical contact {contact_id}")]
    UnknownPhysicalContact { contact_id: u64 },
    #[error("specialized plan edit may not remove demand-bearing contact {contact_id}")]
    DemandBearingContactEdit { contact_id: u64 },
    #[error(
        "specialized plan edit may only remove a regular contact; {contact_id} is not regular"
    )]
    NonRegularContactEdit { contact_id: u64 },
    #[error("specialized plan edit names unknown repair row {row_id:?}")]
    UnknownRepairRow { row_id: RepairRowId },
    #[error("specialized plan edit may not reuse occupied repair row {row_id:?}")]
    OccupiedRepairRow { row_id: RepairRowId },
    #[error("specialized plan edit would duplicate the physical seat for contact {contact_id}")]
    DuplicateChildPhysicalSeat { contact_id: u64 },
    #[error("specialized child plan duplicates physical contact id {contact_id}")]
    DuplicateChildContactId { contact_id: u64 },
    #[error("specialized child plan cannot allocate another physical contact id")]
    PhysicalContactIdOverflow,
    #[error("specialized plan's frozen mesh no longer matches its stored fingerprint")]
    FrozenMeshAuthorityMismatch,
    #[error("specialized finalized load artifact is stale or belongs to another child plan")]
    StaleFinalizedLoadArtifact,
    #[error("registered specialized constants are unavailable: {0}")]
    RegisteredConstantsUnavailable(String),
}

/// Construct and consolidate current specialized contacts.  The long
/// signature is intentional: replay validation restates every independently
/// stale-able authority input rather than accepting caller assertions.
#[allow(clippy::too_many_arguments)]
pub fn prepare_specialized_contacts(
    mesh: &Mesh,
    perception_config: &SupportPerceptionConfig<'_>,
    frame: BuildFrame,
    layers: &LayerPlan,
    candidate_config: &CandidatePayloadConfig,
    reservations: &[SpecializedOwnerReservation],
    perception_limits: GenericPerceptionLimits,
    generic: &GenericContactPlan,
    regular: &SelectedContacts,
    selection_config: &ContactSelectionConfig<'_>,
    limits: SpecializedLimits,
) -> Result<SpecializedContactPlan, SpecializedError> {
    generic.validate_binding(
        mesh,
        perception_config,
        frame,
        layers,
        candidate_config,
        reservations,
        perception_limits,
    )?;
    let topology = build_face_topology(mesh, WeldPolicy::ExactBits)
        .map_err(|error| SpecializedError::Topology(error.to_string()))?;
    validate_regular(mesh, generic, regular, selection_config)?;
    let surface_config_authority = NativeSurfaceConfig {
        grid_pitch_mm: perception_config.surface_grid_pitch_mm.get(),
        max_grid_bytes: perception_config.max_grid_bytes,
    };
    let surface = NativeSurface::from_indexed(mesh.verts(), mesh.tris(), surface_config_authority)
        .map_err(|error| SpecializedError::Surface(error.to_string()))?;
    let exterior_face_ids = surface
        .exterior_face_mask()
        .iter()
        .enumerate()
        .filter_map(|(face, exterior)| exterior.then_some(face as u32))
        .collect::<Vec<_>>();
    let exterior_surface = surface
        .subset(&exterior_face_ids)
        .map_err(|error| SpecializedError::Surface(error.to_string()))?;

    let mut positions =
        Vec::with_capacity(generic.local_z_minima().len() + generic.island_births().len());
    positions.extend(
        generic
            .local_z_minima()
            .iter()
            .map(|minimum| minimum.position_mm),
    );
    positions.extend(
        generic
            .island_births()
            .iter()
            .map(|birth| [birth.centroid_xy_mm[0], birth.centroid_xy_mm[1], birth.z_mm]),
    );
    // One allowed-face projection batch. Native nearest ties resolve by the
    // lowest global face id, so neither an interior face nor caller order can
    // become route authority.
    let (_distance, _projected, face_ids, _normals) = exterior_surface.project(&positions);
    if face_ids
        .iter()
        .any(|face| *face < 0 || *face as usize >= mesh.tris().len())
    {
        return Err(SpecializedError::InvalidProjection);
    }

    let mut islands = generic.islands().to_vec();
    let mut owners = owners_by_face(&islands);
    let mut seeds = Vec::<DemandSeed>::with_capacity(positions.len());
    for (index, minimum) in generic.local_z_minima().iter().enumerate() {
        seeds.push(DemandSeed {
            kind: SpecializedDemandKind::Minimum,
            position_mm: minimum.position_mm,
            normal: minimum.normal,
            face_id: face_ids[index] as u32,
            region_points_mm: minimum.region_points_mm().to_vec(),
            birth: None,
        });
    }
    let minima_count = generic.local_z_minima().len();
    for (offset, birth) in generic.island_births().iter().enumerate() {
        let index = minima_count + offset;
        seeds.push(DemandSeed {
            kind: SpecializedDemandKind::IslandBirth,
            position_mm: positions[index],
            normal: surface.geometry().normals()[face_ids[index] as usize],
            face_id: face_ids[index] as u32,
            region_points_mm: Vec::new(),
            birth: Some((birth.layer_index, birth.z_mm, birth.footprint().clone())),
        });
    }
    add_synthetic_islands(
        mesh,
        &surface,
        topology.component_ids(),
        &seeds,
        &mut islands,
        &mut owners,
        limits,
    )?;

    let coverage_samples =
        build_dyadic_coverage_samples(mesh, &islands, topology.component_ids(), selection_config)?;
    let attach_reach = m0_constants()
        .map_err(|error| SpecializedError::RegisteredConstantsUnavailable(error.to_string()))?
        .placeholders()
        .island_attach_reach_mm()
        .diagnostic()
        .get();
    let spacing_floor = m0_constants()
        .map_err(|error| SpecializedError::RegisteredConstantsUnavailable(error.to_string()))?
        .placeholders()
        .spacing_floor_mm()
        .diagnostic()
        .get();

    let first_synthetic_id = generic
        .islands()
        .iter()
        .map(|island| island.id)
        .max()
        .map_or(0, |id| id.saturating_add(1));
    let synthetic_islands = islands
        .iter()
        .filter(|island| island.id >= first_synthetic_id)
        .cloned()
        .collect::<Vec<_>>();
    let mut synthetic_candidates = None;
    let mut synthetic_selected = None;
    if !synthetic_islands.is_empty() {
        let payload = build_candidate_payload(mesh, &synthetic_islands, candidate_config)?;
        let delicacy = selection_config
            .delicacy_by_face
            .ok_or(ContactSelectionError::MissingDelicacySidecar)?;
        let candidates = crate::ContactCandidatePool {
            candidates: payload
                .rows()
                .iter()
                .enumerate()
                .map(|(row, candidate)| {
                    let row = u64::try_from(row)
                        .map_err(|_| SpecializedError::SyntheticCandidateIdOverflow)?;
                    Ok(crate::ContactCandidate {
                        id: crate::ContactCandidateId(row),
                        face_id: candidate.face_id,
                        point_mm: candidate.point_mm,
                        normal: candidate.normal,
                        supported_area_mm2: surface.geometry().areas()[candidate.face_id as usize],
                        overhang_excess: -candidate.normal[2],
                        delicacy: delicacy[candidate.face_id as usize],
                    })
                })
                .collect::<Result<Vec<_>, SpecializedError>>()?,
        };
        // Synthetic rows enter the one immutable repair pool before any
        // repair controller exists.  They therefore take only the shared S1
        // baseline, never the legacy pre-routing A4 adapter.
        let selected =
            select_synthetic_baseline(mesh, &candidates, &synthetic_islands, selection_config)?;
        let expected_synthetic_samples = coverage_samples
            .iter()
            .filter(|sample| sample.island_id >= first_synthetic_id)
            .cloned()
            .collect::<Vec<_>>();
        if selected.coverage_samples() != expected_synthetic_samples {
            return Err(SpecializedError::StaleRegularContacts);
        }
        synthetic_candidates = Some(candidates);
        synthetic_selected = Some(selected);
    }
    let repair_pool = RepairPool::build(
        repair_pool_lineage_root(generic),
        generic.candidates(),
        generic.islands(),
        synthetic_candidates.as_ref(),
        &synthetic_islands,
        topology.component_ids(),
    )?;
    let mut physical = regular
        .contacts()
        .iter()
        .map(|contact| {
            WorkingContact::regular(
                contact,
                topology.component_ids(),
                RepairRowSourceKind::Generic,
                &repair_pool,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(selected) = synthetic_selected.as_ref() {
        physical.extend(
            selected
                .contacts()
                .iter()
                .map(|contact| {
                    WorkingContact::regular(
                        contact,
                        topology.component_ids(),
                        RepairRowSourceKind::SyntheticIsland,
                        &repair_pool,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    let mut demands = Vec::new();
    for seed in seeds {
        let seed_owners = owners.get(&seed.face_id).cloned().unwrap_or_default();
        for owner in seed_owners {
            demands.push(seed.to_demand(owner, topology.component_ids()[seed.face_id as usize]));
        }
    }
    demands.sort_by(compare_demands);
    demands.dedup_by(|left, right| compare_demands(left, right) == Ordering::Equal);
    let authoritative_demands = demands.clone();
    let dispositions = regular_dispositions(&physical, &demands, layers, attach_reach, limits)?;
    for (demand_index, demand) in demands.into_iter().enumerate() {
        if let Some(index) = dispositions[demand_index].exact_regular {
            physical[index].demands.push(demand);
            continue;
        }
        if dispositions[demand_index].covered {
            continue;
        }
        if !satisfies(demand.nominated_position_mm, &demand, layers, attach_reach) {
            return Err(SpecializedError::UnsatisfiedDemand { demand_index });
        }
        physical.push(WorkingContact::specialized(demand));
    }
    let gate_evidence = consolidate(
        &mut physical,
        generic,
        mesh,
        &surface,
        &topology,
        &islands,
        reservations,
        &coverage_samples,
        &authoritative_demands,
        layers,
        attach_reach,
        spacing_floor,
        selection_config,
        limits,
    )?;
    canonicalize_and_validate(&mut physical, &islands, layers, attach_reach)?;

    let load_inputs = working_load_inputs(&physical, generic, &islands, reservations, limits)?;
    let mut contacts = physical
        .into_iter()
        .enumerate()
        .map(|(index, mut contact)| {
            contact.demands.sort_by(compare_demands);
            let contact_id = index as u64;
            Ok(SpecializedPhysicalContact {
                contact_id,
                position_mm: contact.position_mm,
                normal: contact.normal,
                seat_provenance: contact.seat_provenance,
                carried_area_mm2: 0.0,
                primary_regular: contact.primary_regular,
                regular_routes: contact.regular_routes,
                demands: contact.demands,
            })
        })
        .collect::<Result<Vec<_>, SpecializedError>>()?;
    let load_partition = finalize_specialized_load_partition(
        generic,
        mesh,
        &surface,
        &load_inputs.contacts,
        &load_inputs.specialized_owner_faces,
        LoadFinalizeLimits {
            max_scratch_bytes: limits.max_load_scratch_bytes,
            max_operations: load_inputs.remaining_load_operations,
        },
    )?;
    for row in load_partition.rows() {
        let contact = contacts
            .get_mut(
                usize::try_from(row.contact_id).map_err(|_| SpecializedError::OwnerIdOverflow)?,
            )
            .filter(|contact| contact.contact_id == row.contact_id)
            .ok_or(SpecializedError::MissingSeatProvenance {
                contact_id: row.contact_id,
            })?;
        contact.carried_area_mm2 += row.carried_area_mm2;
    }
    repair_pool.validate_contacts(&contacts)?;
    let next_contact_id = next_contact_id(&contacts);
    let contact_ids = contacts
        .iter()
        .map(SpecializedPhysicalContact::contact_id)
        .collect::<Vec<_>>();
    let finalized_loads =
        FinalizedContactLoads::for_specialized_plan(&load_partition, &contact_ids)?;
    Ok(SpecializedContactPlan {
        source_authority_digest: bound_authority_digest(generic, regular, selection_config, limits),
        semantic_authority_digest: bound_semantic_authority_digest(
            generic,
            regular,
            selection_config,
            limits,
        ),
        generic_slice_thread_evidence: generic.slice_thread_evidence(),
        islands,
        coverage_samples,
        repair_pool,
        generic_authority: generic.clone(),
        mesh_authority: mesh.clone(),
        mesh_authority_fingerprint: topology.mesh_fingerprint().to_owned(),
        reservations_authority: reservations.to_vec(),
        limits_authority: limits,
        surface_config_authority,
        contacts,
        next_contact_id,
        load_partition,
        finalized_loads,
        finalized_load_plan_digest: 0,
        gate_evidence,
    }
    .seal_finalized_load_digest())
}

/// The synthetic source has no historical public handoff, so it must never
/// enter the compatibility-only pre-routing repair adapter.
fn select_synthetic_baseline(
    mesh: &Mesh,
    candidates: &ContactCandidatePool,
    islands: &[SupportIsland],
    config: &ContactSelectionConfig<'_>,
) -> Result<SelectedContacts, ContactSelectionError> {
    crate::select_contacts_baseline(mesh, candidates, islands, config)
}

/// Construct an empty-cardinality authority solely to exercise the initial
/// builder's otherwise unreachable zero-contact branch. The production path
/// first creates a genuine specialized plan from every supplied authority;
/// only then does this cfg(test)-only adapter erase its physical contacts and
/// recompute the empty load partition through the production finalizer. The
/// resulting plan keeps the real authority digests and limits and must pass
/// normal replay validation; callers cannot inject digest or resource bytes.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn authority_consistent_empty_plan_for_initial_test(
    mesh: &Mesh,
    perception_config: &SupportPerceptionConfig<'_>,
    frame: BuildFrame,
    layers: &LayerPlan,
    candidate_config: &CandidatePayloadConfig,
    reservations: &[SpecializedOwnerReservation],
    perception_limits: GenericPerceptionLimits,
    generic: &GenericContactPlan,
    regular: &SelectedContacts,
    selection_config: &ContactSelectionConfig<'_>,
    limits: SpecializedLimits,
) -> Result<SpecializedContactPlan, SpecializedError> {
    let mut plan = prepare_specialized_contacts(
        mesh,
        perception_config,
        frame,
        layers,
        candidate_config,
        reservations,
        perception_limits,
        generic,
        regular,
        selection_config,
        limits,
    )?;
    plan.contacts.clear();
    plan.load_partition =
        crate::perception::authority_consistent_empty_load_partition_for_test(generic, mesh)?;
    plan.finalized_loads = FinalizedContactLoads::for_specialized_plan(&plan.load_partition, &[])?;
    plan.finalized_load_plan_digest = plan.digest();
    debug_assert!(plan.contacts.is_empty());
    debug_assert_eq!(
        plan.contacts
            .iter()
            .map(|contact| contact.demands.len())
            .sum::<usize>(),
        0
    );
    debug_assert!(plan.load_partition.rows().is_empty());
    plan.validate_binding(
        mesh,
        perception_config,
        frame,
        layers,
        candidate_config,
        reservations,
        perception_limits,
        generic,
        regular,
        selection_config,
        limits,
    )?;
    Ok(plan)
}

fn validate_regular(
    mesh: &Mesh,
    generic: &GenericContactPlan,
    regular: &SelectedContacts,
    config: &ContactSelectionConfig<'_>,
) -> Result<(), SpecializedError> {
    if generic.islands().is_empty() {
        if !regular.contacts().is_empty() || !regular.coverage_samples().is_empty() {
            return Err(SpecializedError::StaleRegularContacts);
        }
        if config.surface_max_grid_bytes == 0 {
            return Err(ContactSelectionError::InvalidResourceRequest.into());
        }
        return Ok(());
    }
    let replay = crate::select_contacts(mesh, generic.candidates(), generic.islands(), config)?;
    if replay.digest() != regular.digest() || replay != *regular {
        return Err(SpecializedError::StaleRegularContacts);
    }
    Ok(())
}

#[derive(Clone)]
struct DemandSeed {
    kind: SpecializedDemandKind,
    position_mm: [f64; 3],
    normal: [f64; 3],
    face_id: u32,
    region_points_mm: Vec<[f64; 3]>,
    birth: Option<(usize, f64, SliceSolid)>,
}

impl DemandSeed {
    fn to_demand(&self, owner: u32, component: u32) -> SpecializedDemand {
        SpecializedDemand {
            kind: self.kind,
            owner_island_id: owner,
            coverage_face_id: self.face_id,
            coverage_component_id: component,
            nominated_position_mm: self.position_mm,
            nominated_normal: self.normal,
            region_points_mm: self.region_points_mm.clone(),
            birth_z_mm: self.birth.as_ref().map(|birth| birth.1),
            birth_layer_index: self.birth.as_ref().map(|birth| birth.0),
            birth_footprint: self.birth.as_ref().map(|birth| birth.2.clone()),
        }
    }
}

fn owners_by_face(islands: &[SupportIsland]) -> BTreeMap<u32, Vec<u32>> {
    let mut owners = BTreeMap::<u32, Vec<u32>>::new();
    for island in islands {
        for &face in &island.face_ids {
            owners.entry(face).or_default().push(island.id);
        }
    }
    for values in owners.values_mut() {
        values.sort_unstable();
        values.dedup();
    }
    owners
}

#[allow(clippy::too_many_arguments)]
fn add_synthetic_islands(
    mesh: &Mesh,
    surface: &NativeSurface,
    components: &[u32],
    seeds: &[DemandSeed],
    islands: &mut Vec<SupportIsland>,
    owners: &mut BTreeMap<u32, Vec<u32>>,
    limits: SpecializedLimits,
) -> Result<(), SpecializedError> {
    let unowned = seeds
        .iter()
        .filter(|seed| !owners.contains_key(&seed.face_id))
        .collect::<Vec<_>>();
    if unowned.is_empty() {
        return Ok(());
    }
    let centers = surface.geometry().centroids();
    let index = PointCloudIndex::new(centers)
        .map_err(|error| SpecializedError::SyntheticNeighbors(error.to_string()))?;
    let neighbors = index
        .radius_neighbors_batch(
            &unowned
                .iter()
                .map(|seed| seed.position_mm)
                .collect::<Vec<_>>(),
            SYNTHETIC_PATCH_RADIUS_MM,
            RadiusQueryLimits {
                max_output_pairs: limits.max_synthetic_neighbor_pairs,
                max_output_bytes: limits.max_synthetic_neighbor_bytes,
            },
        )
        .map_err(|error| SpecializedError::SyntheticNeighbors(error.to_string()))?;
    let exterior = surface.exterior_face_mask();
    let mut next_id = islands
        .iter()
        .map(|island| island.id)
        .max()
        .map_or(Ok(0), |id| {
            id.checked_add(1).ok_or(SpecializedError::OwnerIdOverflow)
        })?;
    for (row, seed) in unowned.into_iter().enumerate() {
        if owners.contains_key(&seed.face_id) {
            continue;
        }
        let component = components[seed.face_id as usize];
        let range = neighbors
            .row_range(row)
            .ok_or(SpecializedError::MissingSyntheticPatch)?;
        let mut faces = neighbors.point_ids[range]
            .iter()
            .map(|face| u32::try_from(*face).expect("mesh face ids are u32"))
            .filter(|face| {
                let face = *face as usize;
                exterior[face] && components[face] == component
            })
            .collect::<Vec<_>>();
        faces.push(seed.face_id);
        faces.sort_unstable();
        faces.dedup();
        if faces.is_empty() {
            return Err(SpecializedError::MissingSyntheticPatch);
        }
        let island = SupportIsland {
            id: next_id,
            face_ids: faces,
        };
        for &face in &island.face_ids {
            owners.entry(face).or_default().push(next_id);
            owners.get_mut(&face).expect("inserted").sort_unstable();
            owners.get_mut(&face).expect("inserted").dedup();
        }
        islands.push(island);
        next_id = next_id
            .checked_add(1)
            .ok_or(SpecializedError::OwnerIdOverflow)?;
    }
    islands.sort_by_key(|island| island.id);
    let _ = mesh;
    Ok(())
}

#[derive(Clone)]
struct WorkingContact {
    position_mm: [f64; 3],
    normal: [f64; 3],
    seat_provenance: PhysicalSeatProvenance,
    primary_regular: Option<PrimaryRegularIdentity>,
    regular_routes: Vec<RegularContactRoute>,
    demands: Vec<SpecializedDemand>,
}

impl WorkingContact {
    fn regular(
        contact: &SelectedContact,
        components: &[u32],
        source_kind: RepairRowSourceKind,
        repair_pool: &RepairPool,
    ) -> Result<Self, SpecializedError> {
        let route = RegularContactRoute {
            candidate_id: contact.candidate_id.0,
            owner_island_id: contact.island_id,
            coverage_face_id: contact.served_face_id,
            component_id: components[contact.served_face_id as usize],
            source_kind,
        };
        repair_pool.resolve_route(route)?;
        let primary = PrimaryRegularIdentity {
            route,
            position_mm: contact.point_mm,
            normal: contact.normal,
        };
        Ok(Self {
            position_mm: contact.point_mm,
            normal: contact.normal,
            seat_provenance: PhysicalSeatProvenance::Regular(primary),
            primary_regular: Some(primary),
            regular_routes: vec![route],
            demands: Vec::new(),
        })
    }
    fn specialized(demand: SpecializedDemand) -> Self {
        let seat_provenance = specialized_seat_representative(&demand);
        Self {
            position_mm: demand.nominated_position_mm,
            normal: demand.nominated_normal,
            seat_provenance: PhysicalSeatProvenance::Specialized(seat_provenance),
            primary_regular: None,
            regular_routes: Vec::new(),
            demands: vec![demand],
        }
    }
}

const fn specialized_seat_representative(
    demand: &SpecializedDemand,
) -> SpecializedSeatRepresentative {
    SpecializedSeatRepresentative {
        kind: demand.kind,
        owner_island_id: demand.owner_island_id,
        coverage_face_id: demand.coverage_face_id,
        coverage_component_id: demand.coverage_component_id,
        nominated_position_mm: demand.nominated_position_mm,
        nominated_normal: demand.nominated_normal,
    }
}

struct WorkingLoadInputs {
    contacts: Vec<PhysicalContact>,
    specialized_owner_faces: Vec<(u32, u32)>,
    remaining_load_operations: usize,
}

fn fill_operation_count(
    descriptor_operations: usize,
    reservation_count: usize,
    variants: usize,
) -> Result<usize, SpecializedError> {
    reservation_count
        .checked_mul(variants)
        .and_then(|reservation_work| descriptor_operations.checked_add(reservation_work))
        .ok_or(SpecializedError::GateResourceOverflow)
}

fn accepted_load_header_preflight(
    contact_count: usize,
    island_count: usize,
    reservation_count: usize,
    operation_limit: usize,
) -> Result<(), SpecializedError> {
    if operation_limit == 0 && (contact_count != 0 || island_count != 0 || reservation_count != 0) {
        return Err(GenericPerceptionError::LoadOperationLimit {
            required: 1,
            limit: 0,
        }
        .into());
    }
    Ok(())
}

fn working_load_inputs(
    contacts: &[WorkingContact],
    generic: &GenericContactPlan,
    islands: &[SupportIsland],
    reservations: &[SpecializedOwnerReservation],
    limits: SpecializedLimits,
) -> Result<WorkingLoadInputs, SpecializedError> {
    accepted_load_header_preflight(
        contacts.len(),
        islands.len(),
        reservations.len(),
        limits.max_load_operations,
    )?;
    let mut descriptor_work = 0usize;
    let owner_face_count = variant_specialized_owner_face_count(
        contacts,
        None,
        generic.islands(),
        islands,
        &mut descriptor_work,
        limits.max_load_operations,
    )?;
    let retained_bytes = contacts
        .len()
        .checked_mul(std::mem::size_of::<PhysicalContact>())
        .and_then(|bytes| {
            owner_face_count
                .checked_mul(std::mem::size_of::<(u32, u32)>())
                .and_then(|owner_bytes| bytes.checked_add(owner_bytes))
        })
        .ok_or(SpecializedError::GateResourceOverflow)?;
    let finalizer = specialized_load_resource_plan(generic, contacts.len(), owner_face_count)?;
    let expected_fill_work = fill_operation_count(descriptor_work, reservations.len(), 1)?;
    let required_operations = descriptor_work
        .checked_add(expected_fill_work)
        .and_then(|work| work.checked_add(finalizer.operations))
        .ok_or(SpecializedError::GateResourceOverflow)?;
    if required_operations > limits.max_load_operations {
        return Err(GenericPerceptionError::LoadOperationLimit {
            required: required_operations,
            limit: limits.max_load_operations,
        }
        .into());
    }
    let fill_limit = limits
        .max_load_operations
        .checked_sub(descriptor_work)
        .and_then(|remaining| remaining.checked_sub(finalizer.operations))
        .ok_or(SpecializedError::GateResourceOverflow)?;
    let required_peak = retained_bytes
        .checked_add(finalizer.scratch_bytes)
        .ok_or(SpecializedError::GateResourceOverflow)?;
    if required_peak > limits.max_load_scratch_bytes {
        return Err(GenericPerceptionError::LoadScratchLimit {
            required: required_peak,
            limit: limits.max_load_scratch_bytes,
        }
        .into());
    }
    let mut physical = Vec::with_capacity(contacts.len());
    let mut specialized_owner_faces = Vec::with_capacity(owner_face_count);
    let mut fill_work = 0usize;
    append_variant_load_inputs(
        contacts,
        None,
        generic.islands(),
        islands,
        reservations,
        &mut physical,
        &mut specialized_owner_faces,
        &mut fill_work,
        fill_limit,
    )?;
    debug_assert_eq!(fill_work, expected_fill_work);
    let remaining_load_operations = limits
        .max_load_operations
        .checked_sub(descriptor_work)
        .and_then(|remaining| remaining.checked_sub(fill_work))
        .ok_or(SpecializedError::GateResourceOverflow)?;
    Ok(WorkingLoadInputs {
        contacts: physical,
        specialized_owner_faces,
        remaining_load_operations,
    })
}

fn check_satisfaction_limit(
    tests: usize,
    limits: SpecializedLimits,
) -> Result<(), SpecializedError> {
    if tests > limits.max_satisfaction_tests {
        return Err(SpecializedError::SatisfactionLimit {
            required: tests,
            limit: limits.max_satisfaction_tests,
        });
    }
    Ok(())
}

#[derive(Clone, Copy, Default)]
struct RegularDisposition {
    exact_regular: Option<usize>,
    covered: bool,
}

/// Gather every minimum-region point, make one bounded point-index call, and
/// scatter by bound owner. Birth polygons have no native batch primitive, so
/// their z-windowed local predicates are counted against one explicit cap.
fn regular_dispositions(
    contacts: &[WorkingContact],
    demands: &[SpecializedDemand],
    layers: &LayerPlan,
    attach_reach: f64,
    limits: SpecializedLimits,
) -> Result<Vec<RegularDisposition>, SpecializedError> {
    let regular = contacts
        .iter()
        .enumerate()
        .flat_map(|(index, contact)| {
            contact
                .regular_routes
                .iter()
                .map(move |route| (index, route.owner_island_id, contact.position_mm))
        })
        .collect::<Vec<_>>();
    let mut out = vec![RegularDisposition::default(); demands.len()];
    if regular.is_empty() || demands.is_empty() {
        return Ok(out);
    }
    let index = PointCloudIndex::new(
        &regular
            .iter()
            .map(|(_, _, point)| *point)
            .collect::<Vec<_>>(),
    )
    .map_err(|error| SpecializedError::SatisfactionNeighbors(error.to_string()))?;
    let mut queries = Vec::new();
    let mut query_demands = Vec::new();
    for (demand_index, demand) in demands.iter().enumerate() {
        if demand.kind == SpecializedDemandKind::Minimum {
            for &point in &demand.region_points_mm {
                queries.push(point);
                query_demands.push(demand_index);
            }
        }
    }
    if !queries.is_empty() {
        let neighbors = index
            .radius_neighbors_batch(
                &queries,
                attach_reach,
                RadiusQueryLimits {
                    max_output_pairs: limits.max_satisfaction_neighbor_pairs,
                    max_output_bytes: limits.max_satisfaction_neighbor_bytes,
                },
            )
            .map_err(|error| SpecializedError::SatisfactionNeighbors(error.to_string()))?;
        for (query, &demand_index) in query_demands.iter().enumerate() {
            let demand = &demands[demand_index];
            let range = neighbors
                .row_range(query)
                .expect("one satisfaction row per gathered region point");
            for &point_id in &neighbors.point_ids[range] {
                let (contact_index, owner, point) = regular[point_id as usize];
                // Python's exact mixed-class prepass is owner-independent:
                // one physical axis retains the regular seat while carrying
                // every specialized obligation it exactly satisfies.
                if same_point(point, demand.nominated_position_mm) {
                    out[demand_index].exact_regular = Some(
                        out[demand_index]
                            .exact_regular
                            .map_or(contact_index, |old| old.min(contact_index)),
                    );
                }
                if owner != demand.owner_island_id {
                    continue;
                }
                out[demand_index].covered = true;
            }
        }
    }
    let mut local_tests = 0usize;
    for (demand_index, demand) in demands.iter().enumerate() {
        if demand.kind != SpecializedDemandKind::IslandBirth {
            continue;
        }
        for &(contact_index, owner, point) in &regular {
            if same_point(point, demand.nominated_position_mm) {
                out[demand_index].exact_regular = Some(
                    out[demand_index]
                        .exact_regular
                        .map_or(contact_index, |old| old.min(contact_index)),
                );
            }
            if owner != demand.owner_island_id {
                continue;
            }
            local_tests = local_tests.saturating_add(1);
            check_satisfaction_limit(local_tests, limits)?;
            if satisfies(point, demand, layers, attach_reach) {
                out[demand_index].covered = true;
            }
        }
    }
    Ok(out)
}

fn attach_exact_mixed_contacts(
    contacts: &mut Vec<WorkingContact>,
    layers: &LayerPlan,
    attach_reach: f64,
    limits: SpecializedLimits,
    telemetry: &mut MergeAllocationTelemetry,
) -> Result<usize, SpecializedError> {
    let mut attached = 0usize;
    let mut descriptor_operations = 0usize;
    loop {
        check_merge_pair_limit(contacts.len(), limits.max_merge_pairs)?;
        let mut chosen = None;
        'pairs: for left in 0..contacts.len() {
            for right in left + 1..contacts.len() {
                if !same_point(contacts[left].position_mm, contacts[right].position_mm) {
                    continue;
                }
                let specialized = (
                    !contacts[left].demands.is_empty(),
                    !contacts[right].demands.is_empty(),
                );
                if specialized.0 == specialized.1 {
                    continue;
                }
                let (regular_index, specialized_index) = if specialized.0 {
                    (right, left)
                } else {
                    (left, right)
                };
                let specialized_contact = &contacts[specialized_index];
                let satisfaction_work =
                    specialized_contact
                        .demands
                        .iter()
                        .try_fold(0usize, |total, demand| {
                            total
                                .checked_add(demand_satisfaction_work(demand)?)
                                .ok_or(SpecializedError::GateResourceOverflow)
                        })?;
                let required = descriptor_operations
                    .checked_add(satisfaction_work)
                    .ok_or(SpecializedError::GateResourceOverflow)?;
                if required > limits.max_merge_descriptor_operations {
                    return Err(SpecializedError::MergeDescriptorOperationLimit {
                        required,
                        limit: limits.max_merge_descriptor_operations,
                    });
                }
                descriptor_operations = required;
                let regular_position = contacts[regular_index].position_mm;
                if !specialized_contact
                    .demands
                    .iter()
                    .all(|demand| satisfies(regular_position, demand, layers, attach_reach))
                {
                    continue;
                }
                let representative = specialized_contact
                    .demands
                    .iter()
                    .map(demand_representative_key)
                    .min_by(compare_representative_keys)
                    .expect("specialized class carries a demand");
                let primary_regular = contacts[regular_index]
                    .primary_regular
                    .ok_or(SpecializedError::PrimaryRegularIdentityMismatch)?;
                chosen = Some(MergeProposal {
                    left,
                    right,
                    representative_component_id: representative.component_id,
                    representative_position_mm: representative.position_mm,
                    representative_face_id: representative.face_id,
                    representative_normal: representative.normal,
                    primary_regular: Some(primary_regular),
                    seat_provenance: PhysicalSeatProvenance::Regular(primary_regular),
                    position_mm: regular_position,
                    normal: contacts[regular_index].normal,
                });
                break 'pairs;
            }
        }
        let Some(proposal) = chosen else {
            break;
        };
        let required_iterations = attached
            .checked_add(1)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        if required_iterations > limits.max_merge_iterations {
            return Err(SpecializedError::MergeIterationLimit {
                required: required_iterations,
                limit: limits.max_merge_iterations,
            });
        }
        let mut merged = Some(commit_merge(
            &proposal,
            contacts,
            limits.max_merge_commit_bytes,
            telemetry,
        )?);
        let mut next = Vec::with_capacity(contacts.len().saturating_sub(1));
        for (index, contact) in contacts.drain(..).enumerate() {
            if index == proposal.left {
                next.push(merged.take().expect("left index is visited once"));
            } else if index != proposal.right {
                next.push(contact);
            }
        }
        next.sort_by(compare_contacts);
        *contacts = next;
        attached = required_iterations;
    }
    Ok(attached)
}

/// Union exactly-coincident demand-free regular seats into one physical seat.
///
/// Candidate points are nominated at dyadic cell vertices, and a vertex that
/// falls on an edge shared by two triangles is produced bit-identically by
/// both faces' subdivisions.  Candidate dedup is island-scoped by design (each
/// island keeps its own coverage route), so two islands can legitimately elect
/// the *same physical tip*.  Neither existing merge machine unions that pair:
/// `attach_exact_mixed_contacts` handles only regular x specialized, and the
/// gated tournament refuses demand-free pairs outright.  Without this pass the
/// duplicate survives to `canonicalize_and_validate` and trips
/// `DuplicatePhysicalSeat`.
///
/// One tip serving several islands is the ratified data model: `WorkingContact`
/// carries `regular_routes: Vec<RegularContactRoute>` and `commit_merge`
/// already unions and dedups them.  So this is *canonicalization*, not a
/// discretionary relocation: it is deliberately not screened through
/// `evaluate_gate_batch`, because a gate that could refuse it would leave the
/// duplicate in place and re-trip the error.  It runs immediately before the
/// terminal gate evaluation, so `gate_not_worse` still audits the result.
///
/// The surviving seat is elected deterministically as the smaller
/// `primary_regular` under `compare_primary_regular`, independent of the two
/// seats' positions in the working vector.
fn coalesce_exact_regular_seats(
    contacts: &mut Vec<WorkingContact>,
    limits: SpecializedLimits,
    telemetry: &mut MergeAllocationTelemetry,
) -> Result<usize, SpecializedError> {
    let mut coalesced = 0usize;
    loop {
        check_merge_pair_limit(contacts.len(), limits.max_merge_pairs)?;
        let mut chosen = None;
        'pairs: for left in 0..contacts.len() {
            for right in left + 1..contacts.len() {
                if !same_point(contacts[left].position_mm, contacts[right].position_mm) {
                    continue;
                }
                if !contacts[left].demands.is_empty() || !contacts[right].demands.is_empty() {
                    continue;
                }
                let elected = if compare_primary_regular(
                    contacts[left].primary_regular,
                    contacts[right].primary_regular,
                ) == Ordering::Greater
                {
                    right
                } else {
                    left
                };
                let primary = contacts[elected]
                    .primary_regular
                    .ok_or(SpecializedError::PrimaryRegularIdentityMismatch)?;
                chosen = Some(MergeProposal {
                    left,
                    right,
                    representative_component_id: primary.route.component_id,
                    representative_position_mm: primary.position_mm,
                    representative_face_id: primary.route.coverage_face_id,
                    representative_normal: primary.normal,
                    primary_regular: Some(primary),
                    seat_provenance: PhysicalSeatProvenance::Regular(primary),
                    position_mm: primary.position_mm,
                    normal: primary.normal,
                });
                break 'pairs;
            }
        }
        let Some(proposal) = chosen else {
            break;
        };
        let required_iterations = coalesced
            .checked_add(1)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        if required_iterations > limits.max_merge_iterations {
            return Err(SpecializedError::MergeIterationLimit {
                required: required_iterations,
                limit: limits.max_merge_iterations,
            });
        }
        let mut merged = Some(commit_merge(
            &proposal,
            contacts,
            limits.max_merge_commit_bytes,
            telemetry,
        )?);
        let mut next = Vec::with_capacity(contacts.len().saturating_sub(1));
        for (index, contact) in contacts.drain(..).enumerate() {
            if index == proposal.left {
                next.push(merged.take().expect("left index is visited once"));
            } else if index != proposal.right {
                next.push(contact);
            }
        }
        next.sort_by(compare_contacts);
        *contacts = next;
        coalesced = required_iterations;
    }
    Ok(coalesced)
}

#[allow(clippy::too_many_arguments)]
fn consolidate(
    contacts: &mut Vec<WorkingContact>,
    generic: &GenericContactPlan,
    mesh: &Mesh,
    surface: &NativeSurface,
    topology: &plinth_mesh::FaceTopology,
    islands: &[SupportIsland],
    reservations: &[SpecializedOwnerReservation],
    coverage_samples: &[CoverageSample],
    authoritative_demands: &[SpecializedDemand],
    layers: &LayerPlan,
    attach_reach: f64,
    spacing_floor: f64,
    selection_config: &ContactSelectionConfig<'_>,
    limits: SpecializedLimits,
) -> Result<SpecializedGateEvidence, SpecializedError> {
    contacts.sort_by(compare_contacts);
    let mut telemetry = MergeAllocationTelemetry::default();
    let mut iterations =
        attach_exact_mixed_contacts(contacts, layers, attach_reach, limits, &mut telemetry)?;
    let coverage_radius =
        selection_config.coverage_radius_factor.get() * selection_config.target_spacing_mm.get();
    let initial_baseline = evaluate_gate_batch(
        contacts,
        &[],
        0,
        generic,
        mesh,
        surface,
        topology,
        islands,
        reservations,
        coverage_samples,
        authoritative_demands,
        layers,
        attach_reach,
        coverage_radius,
        selection_config.voxel_pitch_mm.get().powi(2),
        limits,
    )?[0];
    let mut total_proposals = 0usize;
    loop {
        check_merge_pair_limit(contacts.len(), limits.max_merge_pairs)?;
        let mut proposals = Vec::new();
        let mut descriptor_operations = 0usize;
        for left in 0..contacts.len() {
            for right in left + 1..contacts.len() {
                gather_pair_proposals(
                    contacts,
                    left,
                    right,
                    layers,
                    attach_reach,
                    spacing_floor,
                    limits,
                    &mut descriptor_operations,
                    &mut proposals,
                )?;
            }
        }
        total_proposals = total_proposals
            .checked_add(proposals.len())
            .ok_or(SpecializedError::GateResourceOverflow)?;
        if total_proposals > limits.max_merge_total_proposals {
            return Err(SpecializedError::MergeTotalProposalLimit {
                required: total_proposals,
                limit: limits.max_merge_total_proposals,
            });
        }
        if proposals.is_empty() {
            break;
        }
        let vectors = evaluate_gate_batch(
            contacts,
            &proposals,
            proposals
                .capacity()
                .checked_mul(std::mem::size_of::<MergeProposal>())
                .ok_or(SpecializedError::GateResourceOverflow)?,
            generic,
            mesh,
            surface,
            topology,
            islands,
            reservations,
            coverage_samples,
            authoritative_demands,
            layers,
            attach_reach,
            coverage_radius,
            selection_config.voxel_pitch_mm.get().powi(2),
            limits,
        )?;
        let baseline = vectors[0];
        let Some(proposal_index) = first_safe_proposal(&vectors, baseline) else {
            break;
        };
        let required_iterations = iterations
            .checked_add(1)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        if required_iterations > limits.max_merge_iterations {
            return Err(SpecializedError::MergeIterationLimit {
                required: required_iterations,
                limit: limits.max_merge_iterations,
            });
        }
        let proposal = &proposals[proposal_index];
        let mut merged = Some(commit_merge(
            proposal,
            contacts,
            limits.max_merge_commit_bytes,
            &mut telemetry,
        )?);
        let mut next = Vec::with_capacity(contacts.len().saturating_sub(1));
        for (index, contact) in contacts.drain(..).enumerate() {
            if index == proposal.left {
                next.push(merged.take().expect("left index is visited once"));
            } else if index != proposal.right {
                next.push(contact);
            }
        }
        next.sort_by(compare_contacts);
        *contacts = next;
        iterations = required_iterations;
    }
    // Terminal canonicalization. Placement is load-bearing: running the union
    // earlier also unblocks the affected models but perturbs the gated
    // tournament's own ledger and every frozen fixture digest. Running it here
    // leaves already-covered fixtures byte-identical and still lets the
    // terminal `gate_not_worse` audit below observe the coalesced result.
    let coalesced_seat_count = coalesce_exact_regular_seats(contacts, limits, &mut telemetry)?;
    let accepted_vector = evaluate_gate_batch(
        contacts,
        &[],
        0,
        generic,
        mesh,
        surface,
        topology,
        islands,
        reservations,
        coverage_samples,
        authoritative_demands,
        layers,
        attach_reach,
        coverage_radius,
        selection_config.voxel_pitch_mm.get().powi(2),
        limits,
    )?[0];
    if !gate_not_worse(accepted_vector, initial_baseline) {
        return Err(SpecializedError::MergeGateRegression);
    }
    Ok(SpecializedGateEvidence {
        baseline: initial_baseline,
        accepted: accepted_vector,
        evaluated_pair_count: total_proposals,
        accepted_pair_count: iterations,
        coalesced_seat_count,
        deep_clone_count: telemetry.deep_clone_count,
        max_commit_bytes: telemetry.max_commit_bytes,
    })
}

fn check_merge_pair_limit(contact_count: usize, limit: usize) -> Result<(), SpecializedError> {
    let required = contact_count
        .checked_mul(contact_count.saturating_sub(1))
        .and_then(|value| value.checked_div(2))
        .unwrap_or(usize::MAX);
    if required > limit {
        return Err(SpecializedError::MergePairLimit { required, limit });
    }
    Ok(())
}

fn check_merge_proposal_limit(required: usize, limit: usize) -> Result<(), SpecializedError> {
    if required > limit {
        return Err(SpecializedError::MergeProposalLimit { required, limit });
    }
    Ok(())
}

#[derive(Clone)]
struct MergeProposal {
    left: usize,
    right: usize,
    representative_component_id: u32,
    representative_position_mm: [f64; 3],
    representative_face_id: u32,
    representative_normal: [f64; 3],
    primary_regular: Option<PrimaryRegularIdentity>,
    seat_provenance: PhysicalSeatProvenance,
    position_mm: [f64; 3],
    normal: [f64; 3],
}

impl MergeProposal {
    const fn representative_key(&self) -> RepresentativeKey {
        RepresentativeKey {
            component_id: self.representative_component_id,
            position_mm: self.representative_position_mm,
            face_id: self.representative_face_id,
            normal: self.representative_normal,
        }
    }
}

#[derive(Clone, Copy)]
struct RepresentativeKey {
    component_id: u32,
    position_mm: [f64; 3],
    face_id: u32,
    normal: [f64; 3],
}

fn compare_representative_keys(left: &RepresentativeKey, right: &RepresentativeKey) -> Ordering {
    left.component_id
        .cmp(&right.component_id)
        .then_with(|| compare_point3(left.position_mm, right.position_mm))
        .then_with(|| left.face_id.cmp(&right.face_id))
        .then_with(|| compare_point3(left.normal, right.normal))
}

const fn demand_representative_key(demand: &SpecializedDemand) -> RepresentativeKey {
    RepresentativeKey {
        component_id: demand.coverage_component_id,
        position_mm: demand.nominated_position_mm,
        face_id: demand.coverage_face_id,
        normal: demand.nominated_normal,
    }
}

fn gate_not_worse(
    candidate: FrozenSpecializedGateVector,
    baseline: FrozenSpecializedGateVector,
) -> bool {
    coverage_key(candidate) <= coverage_key(baseline)
        && candidate.load_violation_count <= baseline.load_violation_count
        && candidate.unsupported_specialized_count <= baseline.unsupported_specialized_count
}

fn first_safe_proposal(
    vectors: &[FrozenSpecializedGateVector],
    baseline: FrozenSpecializedGateVector,
) -> Option<usize> {
    vectors
        .iter()
        .skip(1)
        .position(|vector| gate_not_worse(*vector, baseline))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GatePeakUsage {
    proposal_bytes: usize,
    result_bytes: usize,
    matrix_bytes: usize,
    coverage_scratch_bytes: usize,
    coverage_nearest_bytes: usize,
    coverage_retained_bytes: usize,
    load_scratch_bytes: usize,
}

fn check_gate_peak(usage: GatePeakUsage, limit: usize) -> Result<usize, SpecializedError> {
    let retained = usage
        .proposal_bytes
        .checked_add(usage.result_bytes)
        .and_then(|bytes| bytes.checked_add(usage.matrix_bytes))
        .ok_or(SpecializedError::GateResourceOverflow)?;
    let coverage_phase = retained
        .checked_add(usage.coverage_scratch_bytes)
        .ok_or(SpecializedError::GateResourceOverflow)?;
    let coverage_nearest_phase = retained
        .checked_add(usage.coverage_nearest_bytes)
        .ok_or(SpecializedError::GateResourceOverflow)?;
    let load_phase = retained
        .checked_add(usage.coverage_retained_bytes)
        .and_then(|bytes| bytes.checked_add(usage.load_scratch_bytes))
        .ok_or(SpecializedError::GateResourceOverflow)?;
    let required = coverage_phase.max(coverage_nearest_phase).max(load_phase);
    if required > limit {
        return Err(SpecializedError::GatePeakByteLimit { required, limit });
    }
    Ok(required)
}

#[derive(Default)]
struct GatePreflightTelemetry {
    payload_allocations: usize,
    payload_operations: usize,
}

#[allow(clippy::too_many_arguments)]
fn gate_header_preflight(
    contact_count: usize,
    proposal_count: usize,
    proposal_storage_bytes: usize,
    sample_count: usize,
    demand_count: usize,
    island_count: usize,
    reservation_count: usize,
    limits: SpecializedLimits,
    _telemetry: &mut GatePreflightTelemetry,
) -> Result<(), SpecializedError> {
    if limits.max_gate_operations == 0
        && (contact_count != 0
            || proposal_count != 0
            || sample_count != 0
            || demand_count != 0
            || island_count != 0
            || reservation_count != 0)
    {
        return Err(SpecializedError::GateOperationLimit {
            required: 1,
            limit: 0,
        });
    }
    let result_bytes = proposal_count
        .checked_add(1)
        .and_then(|count| count.checked_mul(std::mem::size_of::<FrozenSpecializedGateVector>()))
        .ok_or(SpecializedError::GateResourceOverflow)?;
    let required = proposal_storage_bytes
        .checked_add(result_bytes)
        .ok_or(SpecializedError::GateResourceOverflow)?;
    if required > limits.max_gate_peak_bytes {
        return Err(SpecializedError::GatePeakByteLimit {
            required,
            limit: limits.max_gate_peak_bytes,
        });
    }
    Ok(())
}

fn coverage_key(
    vector: FrozenSpecializedGateVector,
) -> (OrderedF64, OrderedF64, usize, OrderedF64) {
    (
        OrderedF64(vector.failing_uncovered_area_mm2),
        OrderedF64(vector.max_patch_area_mm2),
        vector.failing_patch_count,
        OrderedF64(vector.worst_gap_mm),
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct OrderedF64(f64);

impl Eq for OrderedF64 {}
impl PartialOrd for OrderedF64 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for OrderedF64 {
    fn cmp(&self, other: &Self) -> Ordering {
        numeric_cmp(self.0, other.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GateResourcePlan {
    variants: usize,
    vector_bytes: usize,
    contact_window_bytes: usize,
    operations: usize,
}

fn gate_resource_plan(
    contact_count: usize,
    proposal_count: usize,
    sample_count: usize,
    face_count: usize,
    demand_count: usize,
    limits: SpecializedLimits,
) -> Result<GateResourcePlan, SpecializedError> {
    if limits.max_gate_window_variants == 0 {
        return Err(SpecializedError::InvalidGateWindow);
    }
    let variants = proposal_count
        .checked_add(1)
        .ok_or(SpecializedError::GateResourceOverflow)?;
    if variants > limits.max_gate_variants {
        return Err(SpecializedError::GateVariantLimit {
            required: variants,
            limit: limits.max_gate_variants,
        });
    }
    let vector_bytes = variants
        .checked_mul(std::mem::size_of::<FrozenSpecializedGateVector>())
        .ok_or(SpecializedError::GateResourceOverflow)?;
    if vector_bytes > limits.max_gate_vector_bytes {
        return Err(SpecializedError::GateVectorByteLimit {
            required: vector_bytes,
            limit: limits.max_gate_vector_bytes,
        });
    }
    let window = variants.min(limits.max_gate_window_variants);
    let contact_window_bytes = window
        .checked_mul(contact_count.saturating_add(1))
        .and_then(|value| value.checked_mul(std::mem::size_of::<WorkingContact>()))
        .ok_or(SpecializedError::GateResourceOverflow)?;
    if contact_window_bytes > limits.max_gate_contact_matrix_bytes {
        return Err(SpecializedError::GateContactMatrixByteLimit {
            required: contact_window_bytes,
            limit: limits.max_gate_contact_matrix_bytes,
        });
    }
    let contact_visits = proposal_count
        .checked_mul(contact_count.saturating_sub(1))
        .and_then(|value| value.checked_add(contact_count))
        .ok_or(SpecializedError::GateResourceOverflow)?;
    let routed_rows = sample_count
        .checked_add(demand_count)
        .and_then(|value| value.checked_add(1))
        .and_then(|value| value.checked_mul(contact_visits))
        .ok_or(SpecializedError::GateResourceOverflow)?;
    let face_rows = variants
        .checked_mul(face_count)
        .ok_or(SpecializedError::GateResourceOverflow)?;
    let operations = routed_rows
        .checked_add(face_rows)
        .ok_or(SpecializedError::GateResourceOverflow)?;
    if operations > limits.max_gate_operations {
        return Err(SpecializedError::GateOperationLimit {
            required: operations,
            limit: limits.max_gate_operations,
        });
    }
    Ok(GateResourcePlan {
        variants,
        vector_bytes,
        contact_window_bytes,
        operations,
    })
}

fn unique_island_face_count(
    islands: &[SupportIsland],
    work: &mut usize,
    limit: usize,
) -> Result<usize, SpecializedError> {
    let mut count = 0usize;
    let mut last = None;
    loop {
        let mut next = None;
        for island in islands {
            for &face_id in &island.face_ids {
                charge_load_descriptor_work(work, 1, limit)?;
                if last.is_none_or(|last| face_id > last) && next.is_none_or(|next| face_id < next)
                {
                    next = Some(face_id);
                }
            }
        }
        let Some(face_id) = next else {
            break;
        };
        count = count
            .checked_add(1)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        last = Some(face_id);
    }
    Ok(count)
}

fn variant_operation_count(
    contacts: &[WorkingContact],
    proposal: Option<&MergeProposal>,
    sample_count: usize,
    demands: &[SpecializedDemand],
    face_count: usize,
    owner_face_count: usize,
) -> Result<usize, SpecializedError> {
    let mut logical_contact_work = 0usize;
    let mut physical_contacts = 0usize;
    for (index, contact) in contacts.iter().enumerate() {
        if proposal.is_some_and(|p| index == p.left || index == p.right) {
            continue;
        }
        logical_contact_work = logical_contact_work
            .checked_add(1)
            .and_then(|work| work.checked_add(contact.regular_routes.len()))
            .and_then(|work| work.checked_add(contact.demands.len()))
            .ok_or(SpecializedError::GateResourceOverflow)?;
        physical_contacts = physical_contacts
            .checked_add(1)
            .ok_or(SpecializedError::GateResourceOverflow)?;
    }
    if let Some(proposal) = proposal {
        logical_contact_work = logical_contact_work
            .checked_add(1)
            .and_then(|work| work.checked_add(contacts[proposal.left].regular_routes.len()))
            .and_then(|work| work.checked_add(contacts[proposal.right].regular_routes.len()))
            .and_then(|work| work.checked_add(contacts[proposal.left].demands.len()))
            .and_then(|work| work.checked_add(contacts[proposal.right].demands.len()))
            .ok_or(SpecializedError::GateResourceOverflow)?;
        physical_contacts = physical_contacts
            .checked_add(1)
            .ok_or(SpecializedError::GateResourceOverflow)?;
    }
    let demand_satisfaction = demands.iter().try_fold(0usize, |total, demand| {
        total
            .checked_add(demand_satisfaction_work(demand)?)
            .ok_or(SpecializedError::GateResourceOverflow)
    })?;
    let routed_queries = sample_count
        .checked_add(demands.len())
        .ok_or(SpecializedError::GateResourceOverflow)?;
    let routing_work = logical_contact_work
        .checked_mul(routed_queries)
        .ok_or(SpecializedError::GateResourceOverflow)?;
    let unique_face_finalizer_work = face_count
        .checked_mul(physical_contacts)
        .and_then(|work| {
            owner_face_count
                .checked_mul(physical_contacts)
                .and_then(|owners| work.checked_add(owners))
        })
        .ok_or(SpecializedError::GateResourceOverflow)?;
    let satisfaction_work = physical_contacts
        .checked_mul(demand_satisfaction)
        .ok_or(SpecializedError::GateResourceOverflow)?;
    routing_work
        .checked_add(logical_contact_work)
        .and_then(|work| work.checked_add(unique_face_finalizer_work))
        .and_then(|work| work.checked_add(satisfaction_work))
        .ok_or(SpecializedError::GateResourceOverflow)
}

#[allow(clippy::too_many_arguments)]
fn evaluate_gate_batch(
    contacts: &[WorkingContact],
    proposals: &[MergeProposal],
    proposal_storage_bytes: usize,
    generic: &GenericContactPlan,
    mesh: &Mesh,
    surface: &NativeSurface,
    topology: &plinth_mesh::FaceTopology,
    islands: &[SupportIsland],
    reservations: &[SpecializedOwnerReservation],
    coverage_samples: &[CoverageSample],
    authoritative_demands: &[SpecializedDemand],
    layers: &LayerPlan,
    attach_reach: f64,
    coverage_radius: f64,
    coverage_area_floor_mm2: f64,
    limits: SpecializedLimits,
) -> Result<Vec<FrozenSpecializedGateVector>, SpecializedError> {
    let mut preflight_telemetry = GatePreflightTelemetry::default();
    gate_header_preflight(
        contacts.len(),
        proposals.len(),
        proposal_storage_bytes,
        coverage_samples.len(),
        authoritative_demands.len(),
        islands.len(),
        reservations.len(),
        limits,
        &mut preflight_telemetry,
    )?;
    debug_assert_eq!(preflight_telemetry.payload_allocations, 0);
    debug_assert_eq!(preflight_telemetry.payload_operations, 0);
    let mut face_count_operations = 0usize;
    let face_count = unique_island_face_count(
        islands,
        &mut face_count_operations,
        limits.max_gate_operations,
    )?;
    let plan = gate_resource_plan(
        contacts.len(),
        proposals.len(),
        coverage_samples.len(),
        face_count,
        authoritative_demands.len(),
        limits,
    )?;
    let mut descriptor_operations = 0usize;
    let exact_operations = (0..plan.variants).try_fold(0usize, |total, variant| {
        let proposal = variant
            .checked_sub(1)
            .and_then(|index| proposals.get(index));
        let owner_face_count = variant_specialized_owner_face_count(
            contacts,
            proposal,
            generic.islands(),
            islands,
            &mut descriptor_operations,
            limits.max_gate_operations,
        )?;
        total
            .checked_add(variant_operation_count(
                contacts,
                proposal,
                coverage_samples.len(),
                authoritative_demands,
                face_count,
                owner_face_count,
            )?)
            .ok_or(SpecializedError::GateResourceOverflow)
    })?;
    // Each variant descriptor is scanned once for the global operation plan,
    // once for the window matrix size, once for the load scratch plan, and
    // once while filling the preallocated matrix.
    let expected_fill_operations =
        fill_operation_count(descriptor_operations, reservations.len(), plan.variants)?;
    let descriptor_operations = descriptor_operations
        .checked_mul(4)
        .ok_or(SpecializedError::GateResourceOverflow)?;
    let exact_operations = exact_operations
        .checked_add(face_count_operations)
        .and_then(|work| work.checked_add(descriptor_operations))
        .and_then(|work| {
            reservations
                .len()
                .checked_mul(plan.variants)
                .and_then(|reservation_work| work.checked_add(reservation_work))
        })
        .ok_or(SpecializedError::GateResourceOverflow)?;
    if exact_operations > limits.max_gate_operations {
        return Err(SpecializedError::GateOperationLimit {
            required: exact_operations,
            limit: limits.max_gate_operations,
        });
    }
    let fill_operation_limit = limits
        .max_gate_operations
        .checked_sub(
            exact_operations
                .checked_sub(expected_fill_operations)
                .ok_or(SpecializedError::GateResourceOverflow)?,
        )
        .ok_or(SpecializedError::GateResourceOverflow)?;
    // Plan every route in every gate window before allocating the first
    // nearest matrix.  A window-local plan alone would permit a cap failure in
    // a later window after earlier windows had already gathered/queryed data.
    // Each preflight window temporarily builds bounded `Vec`/`BTree*` route
    // payloads, then retains only its arithmetic summary. It therefore does
    // not retain data for the full gate payload.
    whole_gate_route_preflight_then_execute(
        contacts,
        proposals,
        proposal_storage_bytes,
        generic,
        topology,
        islands,
        coverage_samples,
        limits,
        &plan,
        |whole_gate_route_plan| {
            let mut vectors = Vec::with_capacity(plan.variants);
            let mut append_work = 0usize;
            let mut executed_whole_gate_route_plan = GateWholeRoutePlan::default();
            for window_start in (0..plan.variants).step_by(limits.max_gate_window_variants) {
                let window_end =
                    (window_start + limits.max_gate_window_variants).min(plan.variants);
                let window_variants = window_end - window_start;
                let nearest_len = window_variants
                    .checked_mul(coverage_samples.len())
                    .ok_or(SpecializedError::GateResourceOverflow)?;
                let physical_len =
                    (window_start..window_end).try_fold(0usize, |total, variant| {
                        total
                            .checked_add(variant_contact_count(contacts.len(), variant))
                            .ok_or(SpecializedError::GateResourceOverflow)
                    })?;
                let owner_face_len =
                    (window_start..window_end).try_fold(0usize, |total, variant| {
                        let proposal = variant
                            .checked_sub(1)
                            .and_then(|index| proposals.get(index));
                        let mut work = 0usize;
                        total
                            .checked_add(variant_specialized_owner_face_count(
                                contacts,
                                proposal,
                                generic.islands(),
                                islands,
                                &mut work,
                                usize::MAX,
                            )?)
                            .ok_or(SpecializedError::GateResourceOverflow)
                    })?;
                let working_bytes = nearest_len
                    .checked_mul(std::mem::size_of::<f64>())
                    .and_then(|bytes| {
                        physical_len
                            .checked_mul(std::mem::size_of::<PhysicalContact>())
                            .and_then(|physical| bytes.checked_add(physical))
                    })
                    .and_then(|bytes| {
                        owner_face_len
                            .checked_mul(std::mem::size_of::<(u32, u32)>())
                            .and_then(|owners| bytes.checked_add(owners))
                    })
                    .and_then(|bytes| {
                        window_variants
                            .checked_add(1)
                            .and_then(|offsets| {
                                offsets.checked_mul(2 * std::mem::size_of::<usize>())
                            })
                            .and_then(|offsets| bytes.checked_add(offsets))
                    })
                    .ok_or(SpecializedError::GateResourceOverflow)?;
                if working_bytes > limits.max_gate_contact_matrix_bytes {
                    return Err(SpecializedError::GateContactMatrixByteLimit {
                        required: working_bytes,
                        limit: limits.max_gate_contact_matrix_bytes,
                    });
                }
                let coverage_scratch =
                    coverage_assessment_resource_plan(coverage_samples, topology)?;
                let max_load_scratch =
                    (window_start..window_end).try_fold(0usize, |maximum, variant| {
                        let proposal = variant
                            .checked_sub(1)
                            .and_then(|index| proposals.get(index));
                        let mut work = 0usize;
                        let owner_faces = variant_specialized_owner_face_count(
                            contacts,
                            proposal,
                            generic.islands(),
                            islands,
                            &mut work,
                            usize::MAX,
                        )?;
                        let resource = specialized_load_resource_plan(
                            generic,
                            variant_contact_count(contacts.len(), variant),
                            owner_faces,
                        )?;
                        Ok::<_, SpecializedError>(maximum.max(resource.scratch_bytes))
                    })?;
                let coverage_retained_bytes = coverage_samples
                    .len()
                    .checked_mul(std::mem::size_of::<usize>())
                    .ok_or(SpecializedError::GateResourceOverflow)?;
                let mut nearest_route_plans = Vec::with_capacity(window_variants);
                let mut retained_nearest_route_bytes =
                    checked_route_bytes::<CoverageRouteNearestPlan>(window_variants)?;
                let mut max_nearest_route_knn_bytes = 0usize;
                for variant in window_start..window_end {
                    let proposal = variant
                        .checked_sub(1)
                        .and_then(|index| proposals.get(index));
                    let route_plan = plan_variant_nearest_routes(
                        contacts,
                        proposal,
                        coverage_samples,
                        limits.coverage_route_knn,
                    )?;
                    retained_nearest_route_bytes = retained_nearest_route_bytes
                        .checked_add(route_plan.retained_base_logical_bytes)
                        .ok_or(SpecializedError::GateResourceOverflow)?;
                    max_nearest_route_knn_bytes = max_nearest_route_knn_bytes
                        .max(route_plan.max_route_knn_logical_peak_bytes);
                    nearest_route_plans.push(route_plan);
                }
                let executed_route_plan =
                    GateRouteWindowPlan::from_route_plans(&nearest_route_plans, window_variants)?;
                let coverage_nearest_bytes = coverage_route_nearest_peak_bytes(
                    retained_nearest_route_bytes,
                    max_nearest_route_knn_bytes,
                )?;
                let executed_peak_bytes = check_gate_peak(
                    GatePeakUsage {
                        proposal_bytes: proposal_storage_bytes,
                        result_bytes: plan.vector_bytes,
                        matrix_bytes: working_bytes,
                        coverage_scratch_bytes: coverage_scratch.scratch_bytes,
                        coverage_nearest_bytes,
                        coverage_retained_bytes,
                        load_scratch_bytes: max_load_scratch,
                    },
                    limits.max_gate_peak_bytes,
                )?;
                executed_whole_gate_route_plan.record(executed_route_plan, executed_peak_bytes)?;
                let mut nearest_matrix = vec![f64::INFINITY; nearest_len];
                let mut physical_matrix = Vec::with_capacity(physical_len);
                let mut physical_offsets = Vec::with_capacity(window_variants + 1);
                let mut owner_face_matrix = Vec::with_capacity(owner_face_len);
                let mut owner_face_offsets = Vec::with_capacity(window_variants + 1);
                physical_offsets.push(0);
                owner_face_offsets.push(0);
                for ((window_row, variant), nearest_route_plan) in (window_start..window_end)
                    .enumerate()
                    .zip(nearest_route_plans)
                {
                    let proposal = variant
                        .checked_sub(1)
                        .and_then(|index| proposals.get(index));
                    let nearest = &mut nearest_matrix[window_row * coverage_samples.len()
                        ..(window_row + 1) * coverage_samples.len()];
                    fill_variant_nearest(
                        contacts,
                        proposal,
                        coverage_samples,
                        nearest,
                        nearest_route_plan,
                    )?;
                    append_variant_load_inputs(
                        contacts,
                        proposal,
                        generic.islands(),
                        islands,
                        reservations,
                        &mut physical_matrix,
                        &mut owner_face_matrix,
                        &mut append_work,
                        fill_operation_limit,
                    )?;
                    physical_offsets.push(physical_matrix.len());
                    owner_face_offsets.push(owner_face_matrix.len());
                }
                debug_assert_eq!(physical_matrix.len(), physical_len);
                debug_assert_eq!(owner_face_matrix.len(), owner_face_len);
                for (window_row, variant) in (window_start..window_end).enumerate() {
                    let proposal = variant
                        .checked_sub(1)
                        .and_then(|index| proposals.get(index));
                    let nearest = &nearest_matrix[window_row * coverage_samples.len()
                        ..(window_row + 1) * coverage_samples.len()];
                    let coverage = assess_coverage(
                        coverage_samples,
                        topology,
                        nearest,
                        coverage_radius,
                        coverage_area_floor_mm2,
                        CoverageAssessmentLimits {
                            max_scratch_bytes: limits.max_coverage_scratch_bytes,
                            max_operations: limits.max_coverage_operations,
                        },
                    )?;
                    let physical = &physical_matrix
                        [physical_offsets[window_row]..physical_offsets[window_row + 1]];
                    let owner_faces = &owner_face_matrix
                        [owner_face_offsets[window_row]..owner_face_offsets[window_row + 1]];
                    let partition = finalize_specialized_load_partition(
                        generic,
                        mesh,
                        surface,
                        physical,
                        owner_faces,
                        LoadFinalizeLimits {
                            max_scratch_bytes: limits.max_load_scratch_bytes,
                            max_operations: limits.max_load_operations,
                        },
                    )?;
                    let unsupported_specialized_count = authoritative_demands
                        .iter()
                        .filter(|demand| {
                            !variant_satisfies_demand(
                                contacts,
                                proposal,
                                demand,
                                layers,
                                attach_reach,
                            )
                        })
                        .count();
                    vectors.push(FrozenSpecializedGateVector::from_live(
                        coverage.objective,
                        load_violation_count(&partition, physical)?,
                        unsupported_specialized_count,
                    ));
                }
            }
            if whole_gate_route_plan != executed_whole_gate_route_plan {
                return Err(SpecializedError::GateRoutePlanMismatch);
            }
            debug_assert_eq!(append_work, expected_fill_operations);
            Ok(vectors)
        },
    )
}

fn variant_contact_count(contact_count: usize, variant: usize) -> usize {
    if variant == 0 {
        contact_count
    } else {
        contact_count.saturating_sub(1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CoverageRoute {
    island_id: u32,
    component_id: u32,
}

/// Crate-private normalized authority for the shared route-batched nearest
/// pass.  The public verification boundary never exposes this representation.
#[derive(Clone, Copy)]
pub(crate) struct CoverageRoutePoint {
    pub(crate) physical_id: u64,
    pub(crate) island_id: u32,
    pub(crate) component_id: u32,
    pub(crate) position_mm: [f64; 3],
}

pub(crate) struct CoverageRouteKnnPlan {
    route: CoverageRoute,
    source_count: usize,
    query_count: usize,
    knn: KnnBatchPlan,
}

/// The route-local state retained while one variant's nearest matrix is filled.
///
/// `KnnBatchPlan::logical_peak_bytes` already includes the gathered source and
/// query points, immutable index build, CSR output, and tie scratch.  The
/// retained base below deliberately adds only the route normalization and
/// grouping state outside that plan. BTree node/allocation metadata is excluded
/// by the logical-capacity convention; its route keys and element payloads are
/// counted explicitly.
pub(crate) struct CoverageRouteNearestPlan {
    // These remain live through the serial route queries, so their logical
    // payload is part of the peak even though only `route_plans` is read.
    _source_counts: BTreeMap<CoverageRoute, BTreeSet<u64>>,
    // Logical rows remain distinct from the physical seats used to build each
    // route index.  In particular, two source-qualified rows may name the
    // same seat and route key without becoming one membership.
    _route_membership_counts: BTreeMap<CoverageRoute, usize>,
    _query_counts: BTreeMap<CoverageRoute, usize>,
    route_plans: Vec<CoverageRouteKnnPlan>,
    retained_base_logical_bytes: usize,
    max_route_knn_logical_peak_bytes: usize,
    route_membership_sum: usize,
    sequential_route_membership_peak: usize,
    indexed_point_count: usize,
    sequential_indexed_point_peak: usize,
    query_row_count: usize,
    total_work: usize,
}

impl CoverageRouteNearestPlan {
    pub(crate) fn measurement_counts(&self) -> (usize, usize, usize, usize, usize, usize) {
        (
            self.route_membership_sum,
            self.sequential_route_membership_peak,
            self.indexed_point_count,
            self.sequential_indexed_point_peak,
            self.query_row_count,
            self.total_work,
        )
    }

    pub(crate) const fn retained_base_bytes(&self) -> usize {
        self.retained_base_logical_bytes
    }

    pub(crate) const fn max_knn_peak_bytes(&self) -> usize {
        self.max_route_knn_logical_peak_bytes
    }

    pub(crate) fn route_plan_count(&self) -> usize {
        self.route_plans.len()
    }
}

fn checked_route_bytes<T>(count: usize) -> Result<usize, SpecializedError> {
    count
        .checked_mul(std::mem::size_of::<T>())
        .ok_or(SpecializedError::GateResourceOverflow)
}

fn coverage_route_nearest_peak_bytes(
    retained_base_logical_bytes: usize,
    max_route_knn_logical_peak_bytes: usize,
) -> Result<usize, SpecializedError> {
    retained_base_logical_bytes
        .checked_add(max_route_knn_logical_peak_bytes)
        .ok_or(SpecializedError::GateResourceOverflow)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GateRouteWindowPlan {
    route_count: usize,
    total_work: usize,
    retained_base_logical_bytes: usize,
    max_route_knn_logical_peak_bytes: usize,
    fingerprint: u64,
}

impl GateRouteWindowPlan {
    fn from_route_plans(
        route_plans: &[CoverageRouteNearestPlan],
        capacity: usize,
    ) -> Result<Self, SpecializedError> {
        let mut route_count = 0usize;
        let mut total_work = 0usize;
        let mut retained_base_logical_bytes =
            checked_route_bytes::<CoverageRouteNearestPlan>(capacity)?;
        let mut max_route_knn_logical_peak_bytes = 0usize;
        let mut fingerprint = 0xcbf2_9ce4_8422_2325_u64;
        for planned in route_plans {
            route_count = route_count
                .checked_add(planned.route_plans.len())
                .ok_or(SpecializedError::GateResourceOverflow)?;
            total_work = total_work
                .checked_add(planned.total_work)
                .ok_or(SpecializedError::GateResourceOverflow)?;
            retained_base_logical_bytes = retained_base_logical_bytes
                .checked_add(planned.retained_base_logical_bytes)
                .ok_or(SpecializedError::GateResourceOverflow)?;
            max_route_knn_logical_peak_bytes =
                max_route_knn_logical_peak_bytes.max(planned.max_route_knn_logical_peak_bytes);
            for route in &planned.route_plans {
                for word in [
                    u64::from(route.route.island_id),
                    u64::from(route.route.component_id),
                    route.source_count as u64,
                    route.query_count as u64,
                    route.knn.logical_peak_bytes() as u64,
                ] {
                    fingerprint ^= word;
                    fingerprint = fingerprint.wrapping_mul(0x100_0000_01b3);
                }
            }
        }
        Ok(Self {
            route_count,
            total_work,
            retained_base_logical_bytes,
            max_route_knn_logical_peak_bytes,
            fingerprint,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GateWholeRoutePlan {
    window_count: usize,
    total_routes: usize,
    total_work: usize,
    fingerprint: u64,
    max_peak_bytes: usize,
}

#[derive(Clone, Copy, Debug)]
struct GateWindowPreflightSummary {
    routes: GateRouteWindowPlan,
    peak_bytes: usize,
}

impl GateWholeRoutePlan {
    fn record(
        &mut self,
        routes: GateRouteWindowPlan,
        peak_bytes: usize,
    ) -> Result<(), SpecializedError> {
        self.window_count = self
            .window_count
            .checked_add(1)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        self.total_routes = self
            .total_routes
            .checked_add(routes.route_count)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        self.total_work = self
            .total_work
            .checked_add(routes.total_work)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        self.fingerprint ^= routes.fingerprint;
        self.fingerprint = self.fingerprint.wrapping_mul(0x100_0000_01b3);
        self.max_peak_bytes = self.max_peak_bytes.max(peak_bytes);
        Ok(())
    }
}

/// Run execution only after the preflight result has succeeded.
fn preflight_then_execute<T, U>(
    preflight: Result<T, SpecializedError>,
    execute: impl FnOnce(T) -> Result<U, SpecializedError>,
) -> Result<U, SpecializedError> {
    execute(preflight?)
}

/// Traverse every gate window and retain only checked summaries.
fn traverse_whole_gate_windows(
    variants: usize,
    max_window_variants: usize,
    mut plan_window: impl FnMut(usize, usize) -> Result<GateWindowPreflightSummary, SpecializedError>,
) -> Result<GateWholeRoutePlan, SpecializedError> {
    let mut preflight = GateWholeRoutePlan::default();
    for window_start in (0..variants).step_by(max_window_variants) {
        let window_end = (window_start + max_window_variants).min(variants);
        let summary = plan_window(window_start, window_end)?;
        preflight.record(summary.routes, summary.peak_bytes)?;
    }
    preflight
        .total_work
        .checked_add(preflight.total_routes)
        .ok_or(SpecializedError::GateResourceOverflow)?;
    Ok(preflight)
}

/// Route-plan all windows before execution. Each window uses bounded temporary
/// `Vec`/`BTree*` payloads for counting and `KnnBatchPlan` arithmetic; it does
/// not gather points or construct an index.
#[allow(clippy::too_many_arguments)]
fn whole_gate_route_preflight(
    contacts: &[WorkingContact],
    proposals: &[MergeProposal],
    proposal_storage_bytes: usize,
    generic: &GenericContactPlan,
    topology: &plinth_mesh::FaceTopology,
    islands: &[SupportIsland],
    coverage_samples: &[CoverageSample],
    limits: SpecializedLimits,
    gate_plan: &GateResourcePlan,
) -> Result<GateWholeRoutePlan, SpecializedError> {
    let coverage_scratch = coverage_assessment_resource_plan(coverage_samples, topology)?;
    let coverage_retained_bytes = coverage_samples
        .len()
        .checked_mul(std::mem::size_of::<usize>())
        .ok_or(SpecializedError::GateResourceOverflow)?;
    traverse_whole_gate_windows(
        gate_plan.variants,
        limits.max_gate_window_variants,
        |window_start, window_end| {
            let window_variants = window_end - window_start;
            let nearest_len = window_variants
                .checked_mul(coverage_samples.len())
                .ok_or(SpecializedError::GateResourceOverflow)?;
            let physical_len = (window_start..window_end).try_fold(0usize, |total, variant| {
                total
                    .checked_add(variant_contact_count(contacts.len(), variant))
                    .ok_or(SpecializedError::GateResourceOverflow)
            })?;
            let owner_face_len =
                (window_start..window_end).try_fold(0usize, |total, variant| {
                    let proposal = variant
                        .checked_sub(1)
                        .and_then(|index| proposals.get(index));
                    let mut work = 0usize;
                    total
                        .checked_add(variant_specialized_owner_face_count(
                            contacts,
                            proposal,
                            generic.islands(),
                            islands,
                            &mut work,
                            usize::MAX,
                        )?)
                        .ok_or(SpecializedError::GateResourceOverflow)
                })?;
            let working_bytes = nearest_len
                .checked_mul(std::mem::size_of::<f64>())
                .and_then(|bytes| {
                    physical_len
                        .checked_mul(std::mem::size_of::<PhysicalContact>())
                        .and_then(|physical| bytes.checked_add(physical))
                })
                .and_then(|bytes| {
                    owner_face_len
                        .checked_mul(std::mem::size_of::<(u32, u32)>())
                        .and_then(|owners| bytes.checked_add(owners))
                })
                .and_then(|bytes| {
                    window_variants
                        .checked_add(1)
                        .and_then(|offsets| offsets.checked_mul(2 * std::mem::size_of::<usize>()))
                        .and_then(|offsets| bytes.checked_add(offsets))
                })
                .ok_or(SpecializedError::GateResourceOverflow)?;
            if working_bytes > limits.max_gate_contact_matrix_bytes {
                return Err(SpecializedError::GateContactMatrixByteLimit {
                    required: working_bytes,
                    limit: limits.max_gate_contact_matrix_bytes,
                });
            }
            let max_load_scratch =
                (window_start..window_end).try_fold(0usize, |maximum, variant| {
                    let proposal = variant
                        .checked_sub(1)
                        .and_then(|index| proposals.get(index));
                    let mut work = 0usize;
                    let owner_faces = variant_specialized_owner_face_count(
                        contacts,
                        proposal,
                        generic.islands(),
                        islands,
                        &mut work,
                        usize::MAX,
                    )?;
                    let resource = specialized_load_resource_plan(
                        generic,
                        variant_contact_count(contacts.len(), variant),
                        owner_faces,
                    )?;
                    Ok::<_, SpecializedError>(maximum.max(resource.scratch_bytes))
                })?;
            let mut route_plans = Vec::with_capacity(window_variants);
            for variant in window_start..window_end {
                let proposal = variant
                    .checked_sub(1)
                    .and_then(|index| proposals.get(index));
                route_plans.push(plan_variant_nearest_routes(
                    contacts,
                    proposal,
                    coverage_samples,
                    limits.coverage_route_knn,
                )?);
            }
            let routes = GateRouteWindowPlan::from_route_plans(&route_plans, window_variants)?;
            let coverage_nearest_bytes = coverage_route_nearest_peak_bytes(
                routes.retained_base_logical_bytes,
                routes.max_route_knn_logical_peak_bytes,
            )?;
            let peak_bytes = check_gate_peak(
                GatePeakUsage {
                    proposal_bytes: proposal_storage_bytes,
                    result_bytes: gate_plan.vector_bytes,
                    matrix_bytes: working_bytes,
                    coverage_scratch_bytes: coverage_scratch.scratch_bytes,
                    coverage_nearest_bytes,
                    coverage_retained_bytes,
                    load_scratch_bytes: max_load_scratch,
                },
                limits.max_gate_peak_bytes,
            )?;
            Ok(GateWindowPreflightSummary { routes, peak_bytes })
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn whole_gate_route_preflight_then_execute<T>(
    contacts: &[WorkingContact],
    proposals: &[MergeProposal],
    proposal_storage_bytes: usize,
    generic: &GenericContactPlan,
    topology: &plinth_mesh::FaceTopology,
    islands: &[SupportIsland],
    coverage_samples: &[CoverageSample],
    limits: SpecializedLimits,
    gate_plan: &GateResourcePlan,
    execute: impl FnOnce(GateWholeRoutePlan) -> Result<T, SpecializedError>,
) -> Result<T, SpecializedError> {
    preflight_then_execute(
        whole_gate_route_preflight(
            contacts,
            proposals,
            proposal_storage_bytes,
            generic,
            topology,
            islands,
            coverage_samples,
            limits,
            gate_plan,
        ),
        execute,
    )
}

pub(crate) fn plan_route_points_nearest(
    points: &[CoverageRoutePoint],
    samples: &[CoverageSample],
    limits: KnnBatchLimits,
) -> Result<CoverageRouteNearestPlan, SpecializedError> {
    let mut source_counts = BTreeMap::<CoverageRoute, BTreeSet<u64>>::new();
    let mut route_membership_counts = BTreeMap::<CoverageRoute, usize>::new();
    let mut query_counts = BTreeMap::<CoverageRoute, usize>::new();
    let mut route_membership_sum = 0usize;
    let mut sequential_route_membership_peak = 0usize;
    for point in points {
        let route = CoverageRoute {
            island_id: point.island_id,
            component_id: point.component_id,
        };
        let memberships = route_membership_counts.entry(route).or_default();
        *memberships = memberships
            .checked_add(1)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        route_membership_sum = route_membership_sum
            .checked_add(1)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        sequential_route_membership_peak = sequential_route_membership_peak.max(*memberships);
        source_counts
            .entry(route)
            .or_default()
            .insert(point.physical_id);
    }
    for sample in samples {
        let route = CoverageRoute {
            island_id: sample.island_id,
            component_id: sample.component_id,
        };
        let count = query_counts.entry(route).or_default();
        *count = count
            .checked_add(1)
            .ok_or(SpecializedError::GateResourceOverflow)?;
    }
    let mut route_plans = Vec::with_capacity(source_counts.len());
    let mut total_work = 0usize;
    let mut total_indexed_points = 0usize;
    let mut total_queries = 0usize;
    let mut max_route_peak = 0usize;
    let mut max_source_count = 0usize;
    let mut max_query_count = 0usize;
    for (route, source_ids) in &source_counts {
        let query_count = query_counts.get(route).copied().unwrap_or(0);
        total_indexed_points = total_indexed_points
            .checked_add(source_ids.len())
            .ok_or(SpecializedError::GateResourceOverflow)?;
        let work = source_ids
            .len()
            .checked_mul(query_count)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        total_work = total_work
            .checked_add(work)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        if source_ids.is_empty() || query_count == 0 {
            continue;
        }
        total_queries = total_queries
            .checked_add(query_count)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        let knn = KnnBatchPlan::new(source_ids.len(), query_count, 1, limits)
            .map_err(|error| SpecializedError::CoverageNearest(error.to_string()))?;
        max_route_peak = max_route_peak.max(knn.logical_peak_bytes());
        max_source_count = max_source_count.max(source_ids.len());
        max_query_count = max_query_count.max(query_count);
        route_plans.push(CoverageRouteKnnPlan {
            route: *route,
            source_count: source_ids.len(),
            query_count,
            knn,
        });
    }
    debug_assert!(max_route_peak <= limits.max_logical_peak_bytes);
    if total_work > limits.max_work {
        return Err(SpecializedError::GateOperationLimit {
            required: total_work,
            limit: limits.max_work,
        });
    }
    let retained_base_logical_bytes = [
        checked_route_bytes::<CoverageRoute>(source_counts.len())?,
        checked_route_bytes::<(CoverageRoute, usize)>(route_membership_counts.len())?,
        checked_route_bytes::<u64>(total_indexed_points)?,
        checked_route_bytes::<(CoverageRoute, usize)>(query_counts.len())?,
        checked_route_bytes::<CoverageRouteKnnPlan>(route_plans.len())?,
        checked_route_bytes::<u64>(max_source_count)?,
        checked_route_bytes::<usize>(max_query_count)?,
    ]
    .into_iter()
    .try_fold(0usize, |total, bytes| {
        total
            .checked_add(bytes)
            .ok_or(SpecializedError::GateResourceOverflow)
    })?;
    Ok(CoverageRouteNearestPlan {
        _source_counts: source_counts,
        _route_membership_counts: route_membership_counts,
        _query_counts: query_counts,
        route_plans,
        retained_base_logical_bytes,
        max_route_knn_logical_peak_bytes: max_route_peak,
        route_membership_sum,
        sequential_route_membership_peak,
        indexed_point_count: total_indexed_points,
        sequential_indexed_point_peak: max_source_count,
        query_row_count: total_queries,
        total_work,
    })
}

fn plan_variant_nearest_routes(
    contacts: &[WorkingContact],
    proposal: Option<&MergeProposal>,
    samples: &[CoverageSample],
    limits: KnnBatchLimits,
) -> Result<CoverageRouteNearestPlan, SpecializedError> {
    let mut points = Vec::new();
    let mut add_contact = |physical_id: u64, position_mm: [f64; 3], contact: &WorkingContact| {
        points.extend(
            contact
                .regular_routes
                .iter()
                .map(|route| CoverageRoutePoint {
                    physical_id,
                    island_id: route.owner_island_id,
                    component_id: route.component_id,
                    position_mm,
                }),
        );
        points.extend(contact.demands.iter().map(|demand| CoverageRoutePoint {
            physical_id,
            island_id: demand.owner_island_id,
            component_id: demand.coverage_component_id,
            position_mm,
        }));
    };
    for (physical_id, contact) in contacts.iter().enumerate() {
        if !proposal.is_some_and(|p| physical_id == p.left || physical_id == p.right) {
            add_contact(
                u64::try_from(physical_id).map_err(|_| SpecializedError::GateResourceOverflow)?,
                contact.position_mm,
                contact,
            );
        }
    }
    if let Some(proposal) = proposal {
        let physical_id =
            u64::try_from(proposal.left).map_err(|_| SpecializedError::GateResourceOverflow)?;
        add_contact(physical_id, proposal.position_mm, &contacts[proposal.left]);
        add_contact(physical_id, proposal.position_mm, &contacts[proposal.right]);
    }
    plan_route_points_nearest(&points, samples, limits)
}

fn fill_variant_nearest(
    contacts: &[WorkingContact],
    proposal: Option<&MergeProposal>,
    samples: &[CoverageSample],
    nearest: &mut [f64],
    planned: CoverageRouteNearestPlan,
) -> Result<(), SpecializedError> {
    let mut points = Vec::new();
    let mut add = |physical_id: u64, position_mm: [f64; 3], contact: &WorkingContact| {
        points.extend(
            contact
                .regular_routes
                .iter()
                .map(|candidate| CoverageRoutePoint {
                    physical_id,
                    island_id: candidate.owner_island_id,
                    component_id: candidate.component_id,
                    position_mm,
                }),
        );
        points.extend(contact.demands.iter().map(|demand| CoverageRoutePoint {
            physical_id,
            island_id: demand.owner_island_id,
            component_id: demand.coverage_component_id,
            position_mm,
        }));
    };
    for (physical_id, contact) in contacts.iter().enumerate() {
        if !proposal.is_some_and(|p| physical_id == p.left || physical_id == p.right) {
            add(
                u64::try_from(physical_id).expect("usize fits u64"),
                contact.position_mm,
                contact,
            );
        }
    }
    if let Some(proposal) = proposal {
        let physical_id = u64::try_from(proposal.left).expect("usize fits u64");
        add(physical_id, proposal.position_mm, &contacts[proposal.left]);
        add(physical_id, proposal.position_mm, &contacts[proposal.right]);
    }
    fill_route_points_nearest(&points, samples, nearest, planned)
}

pub(crate) fn fill_route_points_nearest(
    points: &[CoverageRoutePoint],
    samples: &[CoverageSample],
    nearest: &mut [f64],
    planned: CoverageRouteNearestPlan,
) -> Result<(), SpecializedError> {
    debug_assert_eq!(nearest.len(), samples.len());
    /* All route plans are checked before any geometric gather or query. */
    for route_plan in planned.route_plans {
        let route = route_plan.route;
        let query_count = route_plan.query_count;
        let plan = route_plan.knn;
        let mut source_points = Vec::with_capacity(route_plan.source_count);
        let mut seen = BTreeSet::new();
        for point in points.iter().copied().filter(|point| {
            point.island_id == route.island_id && point.component_id == route.component_id
        }) {
            if seen.insert(point.physical_id) {
                source_points.push(point.position_mm);
            }
        }
        debug_assert_eq!(source_points.len(), plan.source_count());
        let mut query_indices = Vec::with_capacity(query_count);
        let mut query_points = Vec::with_capacity(query_count);
        for (sample_index, sample) in samples.iter().enumerate() {
            if (CoverageRoute {
                island_id: sample.island_id,
                component_id: sample.component_id,
            }) == route
            {
                query_indices.push(sample_index);
                query_points.push(sample.point_mm);
            }
        }
        let index = PointCloudIndex::new_for_knn_batch(&source_points, &plan)
            .map_err(|error| SpecializedError::CoverageNearest(error.to_string()))?;
        let rows = index
            .knn_batch_planned(&query_points, &plan)
            .map_err(|error| SpecializedError::CoverageNearest(error.to_string()))?;
        for (query_row, &sample_index) in query_indices.iter().enumerate() {
            let row = rows.row_range(query_row).expect("planned row exists");
            nearest[sample_index] = rows.squared_distances[row.start].sqrt();
        }
    }
    Ok(())
}

fn variant_satisfies_demand(
    contacts: &[WorkingContact],
    proposal: Option<&MergeProposal>,
    demand: &SpecializedDemand,
    layers: &LayerPlan,
    attach_reach: f64,
) -> bool {
    contacts.iter().enumerate().any(|(index, contact)| {
        !proposal.is_some_and(|p| index == p.left || index == p.right)
            && contact_serves_owner(contact, demand.owner_island_id)
            && satisfies(contact.position_mm, demand, layers, attach_reach)
    }) || proposal.is_some_and(|p| {
        (contact_serves_owner(&contacts[p.left], demand.owner_island_id)
            || contact_serves_owner(&contacts[p.right], demand.owner_island_id))
            && satisfies(p.position_mm, demand, layers, attach_reach)
    })
}

#[derive(Clone, Copy)]
struct ContactParts<'a> {
    left: &'a WorkingContact,
    right: Option<&'a WorkingContact>,
}

impl<'a> ContactParts<'a> {
    const fn new(left: &'a WorkingContact, right: Option<&'a WorkingContact>) -> Self {
        Self { left, right }
    }

    fn route_count(self) -> usize {
        self.left.regular_routes.len() + self.right.map_or(0, |right| right.regular_routes.len())
    }

    fn route(self, index: usize) -> RegularContactRoute {
        if index < self.left.regular_routes.len() {
            self.left.regular_routes[index]
        } else {
            self.right
                .expect("route index belongs to right contact")
                .regular_routes[index - self.left.regular_routes.len()]
        }
    }

    fn has_demands(self) -> bool {
        !self.left.demands.is_empty() || self.right.is_some_and(|right| !right.demands.is_empty())
    }
}

fn charge_load_descriptor_work(
    work: &mut usize,
    amount: usize,
    limit: usize,
) -> Result<(), SpecializedError> {
    let required = work
        .checked_add(amount)
        .ok_or(SpecializedError::GateResourceOverflow)?;
    if required > limit {
        return Err(SpecializedError::GateOperationLimit { required, limit });
    }
    *work = required;
    Ok(())
}

fn parts_serves_owner(
    parts: ContactParts<'_>,
    owner: u32,
    work: &mut usize,
    limit: usize,
) -> Result<bool, SpecializedError> {
    for route_index in 0..parts.route_count() {
        charge_load_descriptor_work(work, 1, limit)?;
        if parts.route(route_index).owner_island_id == owner {
            return Ok(true);
        }
    }
    for demand in parts
        .left
        .demands
        .iter()
        .chain(parts.right.into_iter().flat_map(|right| &right.demands))
    {
        charge_load_descriptor_work(work, 1, limit)?;
        if demand.owner_island_id == owner {
            return Ok(true);
        }
    }
    Ok(false)
}

fn generic_owner_for_parts(
    parts: ContactParts<'_>,
    generic_islands: &[SupportIsland],
    work: &mut usize,
    limit: usize,
) -> Result<Option<u32>, SpecializedError> {
    if parts.has_demands() {
        return Ok(None);
    }
    let mut unique_route = None;
    for route_index in 0..parts.route_count() {
        let route = parts.route(route_index);
        let mut seen = false;
        for prior_index in 0..route_index {
            charge_load_descriptor_work(work, 1, limit)?;
            if parts.route(prior_index) == route {
                seen = true;
                break;
            }
        }
        if seen {
            continue;
        }
        if unique_route.is_some() {
            return Ok(None);
        }
        unique_route = Some(route);
    }
    let Some(route) = unique_route else {
        return Ok(None);
    };
    for island in generic_islands {
        charge_load_descriptor_work(work, 1, limit)?;
        if island.id == route.owner_island_id {
            return Ok(Some(route.owner_island_id));
        }
    }
    Ok(None)
}

fn visit_specialized_owner_faces(
    parts: ContactParts<'_>,
    generic_islands: &[SupportIsland],
    islands: &[SupportIsland],
    work: &mut usize,
    limit: usize,
    visit: impl FnMut(u32) -> Result<(), SpecializedError>,
) -> Result<Option<u32>, SpecializedError> {
    if let Some(owner) = generic_owner_for_parts(parts, generic_islands, work, limit)? {
        return Ok(Some(owner));
    }
    visit_served_owner_faces(parts, islands, work, limit, visit)?;
    Ok(None)
}

fn visit_served_owner_faces(
    parts: ContactParts<'_>,
    islands: &[SupportIsland],
    work: &mut usize,
    limit: usize,
    mut visit: impl FnMut(u32) -> Result<(), SpecializedError>,
) -> Result<(), SpecializedError> {
    // Match BTreeSet iteration without allocating it: repeatedly select the
    // smallest served face greater than the last emitted face. This keeps the
    // load rows canonical even when island face ranges interleave.
    let mut last = None;
    loop {
        let mut next = None;
        for island in islands {
            if !parts_serves_owner(parts, island.id, work, limit)? {
                continue;
            }
            for &face_id in &island.face_ids {
                charge_load_descriptor_work(work, 1, limit)?;
                if last.is_none_or(|last| face_id > last) && next.is_none_or(|next| face_id < next)
                {
                    next = Some(face_id);
                }
            }
        }
        let Some(face_id) = next else {
            break;
        };
        visit(face_id)?;
        last = Some(face_id);
    }
    Ok(())
}

fn variant_specialized_owner_face_count(
    contacts: &[WorkingContact],
    proposal: Option<&MergeProposal>,
    generic_islands: &[SupportIsland],
    islands: &[SupportIsland],
    work: &mut usize,
    limit: usize,
) -> Result<usize, SpecializedError> {
    let mut count = 0usize;
    for (index, contact) in contacts.iter().enumerate() {
        if proposal.is_some_and(|p| index == p.left || index == p.right) {
            continue;
        }
        visit_specialized_owner_faces(
            ContactParts::new(contact, None),
            generic_islands,
            islands,
            work,
            limit,
            |_| {
                count = count
                    .checked_add(1)
                    .ok_or(SpecializedError::GateResourceOverflow)?;
                Ok(())
            },
        )?;
    }
    if let Some(proposal) = proposal {
        visit_specialized_owner_faces(
            ContactParts::new(&contacts[proposal.left], Some(&contacts[proposal.right])),
            generic_islands,
            islands,
            work,
            limit,
            |_| {
                count = count
                    .checked_add(1)
                    .ok_or(SpecializedError::GateResourceOverflow)?;
                Ok(())
            },
        )?;
    }
    Ok(count)
}

#[allow(clippy::too_many_arguments)]
fn append_variant_load_inputs(
    contacts: &[WorkingContact],
    proposal: Option<&MergeProposal>,
    generic_islands: &[SupportIsland],
    islands: &[SupportIsland],
    reservations: &[SpecializedOwnerReservation],
    physical: &mut Vec<PhysicalContact>,
    specialized_owner_faces: &mut Vec<(u32, u32)>,
    work: &mut usize,
    limit: usize,
) -> Result<(), SpecializedError> {
    let mut maximum_reserved_owner = None;
    for reservation in reservations {
        charge_load_descriptor_work(work, 1, limit)?;
        maximum_reserved_owner = Some(
            maximum_reserved_owner.map_or(reservation.owner_id, |owner: u32| {
                owner.max(reservation.owner_id)
            }),
        );
    }
    let first_specialized_owner = maximum_reserved_owner.map_or(Ok(0), |owner| {
        owner
            .checked_add(1)
            .ok_or(SpecializedError::OwnerIdOverflow)
    })?;
    let start = physical.len();
    let mut append =
        |position_mm: [f64; 3], parts: ContactParts<'_>| -> Result<(), SpecializedError> {
            let contact_id = u64::try_from(physical.len() - start)
                .map_err(|_| SpecializedError::OwnerIdOverflow)?;
            let owner = if let Some(generic_owner) =
                generic_owner_for_parts(parts, generic_islands, work, limit)?
            {
                LoadOwner::GenericIsland(generic_owner)
            } else {
                let specialized_owner = first_specialized_owner
                    .checked_add(
                        u32::try_from(contact_id).map_err(|_| SpecializedError::OwnerIdOverflow)?,
                    )
                    .ok_or(SpecializedError::OwnerIdOverflow)?;
                visit_served_owner_faces(parts, islands, work, limit, |face_id| {
                    specialized_owner_faces.push((specialized_owner, face_id));
                    Ok(())
                })?;
                LoadOwner::Specialized(specialized_owner)
            };
            physical.push(PhysicalContact {
                contact_id,
                owner,
                point_mm: position_mm,
            });
            Ok(())
        };
    for (index, contact) in contacts.iter().enumerate() {
        if proposal.is_some_and(|p| index == p.left || index == p.right) {
            continue;
        }
        append(contact.position_mm, ContactParts::new(contact, None))?;
    }
    if let Some(proposal) = proposal {
        append(
            proposal.position_mm,
            ContactParts::new(&contacts[proposal.left], Some(&contacts[proposal.right])),
        )?;
    }
    Ok(())
}

fn contact_serves_owner(contact: &WorkingContact, owner: u32) -> bool {
    contact
        .regular_routes
        .iter()
        .any(|route| route.owner_island_id == owner)
        || contact
            .demands
            .iter()
            .any(|demand| demand.owner_island_id == owner)
}

fn demand_satisfaction_work(demand: &SpecializedDemand) -> Result<usize, SpecializedError> {
    match demand.kind {
        SpecializedDemandKind::Minimum => Ok(demand.region_points_mm.len().max(1)),
        SpecializedDemandKind::IslandBirth => {
            demand.birth_footprint.as_ref().map_or(Ok(1), |solid| {
                solid
                    .holes()
                    .iter()
                    .try_fold(solid.outer().points_xy_mm().len(), |total, hole| {
                        total
                            .checked_add(hole.points_xy_mm().len())
                            .ok_or(SpecializedError::GateResourceOverflow)
                    })?
                    .checked_mul(2)
                    .and_then(|work| work.checked_add(1))
                    .ok_or(SpecializedError::GateResourceOverflow)
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn gather_pair_proposals(
    contacts: &[WorkingContact],
    left_index: usize,
    right_index: usize,
    layers: &LayerPlan,
    attach_reach: f64,
    spacing_floor: f64,
    limits: SpecializedLimits,
    descriptor_operations: &mut usize,
    proposals: &mut Vec<MergeProposal>,
) -> Result<(), SpecializedError> {
    if limits.max_merge_descriptor_operations == 0 {
        return Err(SpecializedError::MergeDescriptorOperationLimit {
            required: 1,
            limit: 0,
        });
    }
    let left = &contacts[left_index];
    let right = &contacts[right_index];
    if left.demands.is_empty() || right.demands.is_empty() {
        return Ok(());
    }
    let classes = (
        !left.regular_routes.is_empty(),
        !right.regular_routes.is_empty(),
    );
    let shared_regular = (
        classes.0 && !left.demands.is_empty(),
        classes.1 && !right.demands.is_empty(),
    );
    let distance = distance3(left.position_mm, right.position_mm);
    if classes.0 || classes.1 {
        if !same_point(left.position_mm, right.position_mm) {
            return Ok(());
        }
    } else if distance >= spacing_floor {
        return Ok(());
    }
    let demands = || left.demands.iter().chain(&right.demands);
    if demands().all(|demand| demand.kind == SpecializedDemandKind::IslandBirth)
        && distance > BIRTH_LINK_RADIUS_MM
    {
        return Ok(());
    }
    let component = demands()
        .next()
        .expect("both specialized contacts carry a demand")
        .coverage_component_id;
    if demands().any(|demand| demand.coverage_component_id != component) {
        return Ok(());
    }
    let demand_count = left.demands.len().saturating_add(right.demands.len());
    let candidate_count = demand_count;
    let satisfaction_work = demands().try_fold(0usize, |total, demand| {
        total
            .checked_add(demand_satisfaction_work(demand)?)
            .ok_or(SpecializedError::GateResourceOverflow)
    })?;
    let pair_work = candidate_count
        .checked_mul(candidate_count)
        .and_then(|selection| {
            candidate_count
                .checked_mul(satisfaction_work)
                .and_then(|satisfaction| selection.checked_add(satisfaction))
        })
        .ok_or(SpecializedError::GateResourceOverflow)?;
    let required_work = descriptor_operations
        .checked_add(pair_work)
        .ok_or(SpecializedError::GateResourceOverflow)?;
    if required_work > limits.max_merge_descriptor_operations {
        return Err(SpecializedError::MergeDescriptorOperationLimit {
            required: required_work,
            limit: limits.max_merge_descriptor_operations,
        });
    }
    *descriptor_operations = required_work;
    let mut append = |representative: RepresentativeKey,
                      primary_regular: Option<PrimaryRegularIdentity>,
                      seat_provenance: PhysicalSeatProvenance,
                      position_mm: [f64; 3],
                      normal: [f64; 3]|
     -> Result<(), SpecializedError> {
        if !demands().all(|demand| satisfies(position_mm, demand, layers, attach_reach)) {
            return Ok(());
        }
        let required = proposals
            .len()
            .checked_add(1)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        check_merge_proposal_limit(required, limits.max_merge_proposals)?;
        let required_bytes = required
            .checked_mul(std::mem::size_of::<MergeProposal>())
            .ok_or(SpecializedError::GateResourceOverflow)?;
        if required_bytes > limits.max_merge_proposal_bytes {
            return Err(SpecializedError::MergeProposalByteLimit {
                required: required_bytes,
                limit: limits.max_merge_proposal_bytes,
            });
        }
        if proposals.len() == proposals.capacity() {
            proposals
                .try_reserve_exact(1)
                .map_err(|_| SpecializedError::GateResourceOverflow)?;
        }
        proposals.push(MergeProposal {
            left: left_index,
            right: right_index,
            representative_component_id: representative.component_id,
            representative_position_mm: representative.position_mm,
            representative_face_id: representative.face_id,
            representative_normal: representative.normal,
            primary_regular,
            seat_provenance,
            position_mm,
            normal,
        });
        Ok(())
    };
    let mut last = None;
    loop {
        let next = demands()
            .map(demand_representative_key)
            .filter(|key| {
                !(shared_regular.0 || shared_regular.1)
                    || (shared_regular.0 && same_point(key.position_mm, left.position_mm))
                    || (shared_regular.1 && same_point(key.position_mm, right.position_mm))
            })
            .filter(|key| last.is_none_or(|last| compare_representative_keys(key, &last).is_gt()))
            .min_by(compare_representative_keys);
        let Some(next) = next else {
            break;
        };
        let selected_demand = demands()
            .filter(|demand| {
                compare_representative_keys(&demand_representative_key(demand), &next).is_eq()
            })
            .min_by(|left, right| compare_demands(left, right))
            .expect("representative key is drawn from these demands");
        let physical = if shared_regular.0 {
            left
        } else if shared_regular.1 {
            right
        } else {
            append(
                next,
                None,
                PhysicalSeatProvenance::Specialized(specialized_seat_representative(
                    selected_demand,
                )),
                next.position_mm,
                next.normal,
            )?;
            last = Some(next);
            continue;
        };
        let primary_regular = physical
            .primary_regular
            .ok_or(SpecializedError::PrimaryRegularIdentityMismatch)?;
        append(
            next,
            Some(primary_regular),
            PhysicalSeatProvenance::Regular(primary_regular),
            physical.position_mm,
            physical.normal,
        )?;
        last = Some(next);
    }
    Ok(())
}

#[cfg(test)]
const TEST_PAIR_KNN_LIMITS: KnnBatchLimits = KnnBatchLimits {
    max_source_points: 20_000_000,
    max_query_points: 20_000_000,
    max_neighbors: 1,
    max_logical_peak_bytes: 2 * 1024 * 1024 * 1024,
    max_work: 100_000_000,
};

#[cfg(test)]
fn test_pair_proposals(
    contacts: &[WorkingContact],
    layers: &LayerPlan,
    attach_reach: f64,
    spacing_floor: f64,
) -> Vec<MergeProposal> {
    let mut proposals = Vec::new();
    let mut operations = 0;
    gather_pair_proposals(
        contacts,
        0,
        1,
        layers,
        attach_reach,
        spacing_floor,
        SpecializedLimits::production(TEST_PAIR_KNN_LIMITS),
        &mut operations,
        &mut proposals,
    )
    .unwrap();
    proposals
}

#[derive(Default)]
struct MergeAllocationTelemetry {
    deep_clone_count: usize,
    max_commit_bytes: usize,
}

fn demand_deep_bytes(demand: &SpecializedDemand) -> Result<usize, SpecializedError> {
    let mut bytes = std::mem::size_of::<SpecializedDemand>()
        .checked_add(
            demand
                .region_points_mm
                .len()
                .checked_mul(std::mem::size_of::<[f64; 3]>())
                .ok_or(SpecializedError::GateResourceOverflow)?,
        )
        .ok_or(SpecializedError::GateResourceOverflow)?;
    if let Some(solid) = &demand.birth_footprint {
        bytes = bytes
            .checked_add(std::mem::size_of_val(solid.holes()))
            .ok_or(SpecializedError::GateResourceOverflow)?;
        bytes = bytes
            .checked_add(
                solid
                    .outer()
                    .points_xy_mm()
                    .len()
                    .checked_mul(std::mem::size_of::<[f64; 2]>())
                    .ok_or(SpecializedError::GateResourceOverflow)?,
            )
            .and_then(|value| {
                value.checked_add(
                    solid
                        .holes()
                        .iter()
                        .map(|hole| hole.points_xy_mm().len())
                        .sum::<usize>()
                        .checked_mul(std::mem::size_of::<[f64; 2]>())?,
                )
            })
            .ok_or(SpecializedError::GateResourceOverflow)?;
    }
    Ok(bytes)
}

fn commit_merge(
    proposal: &MergeProposal,
    contacts: &[WorkingContact],
    byte_limit: usize,
    telemetry: &mut MergeAllocationTelemetry,
) -> Result<WorkingContact, SpecializedError> {
    let key = proposal.representative_key();
    debug_assert_eq!(compare_representative_keys(&key, &key), Ordering::Equal);
    let left = &contacts[proposal.left];
    let right = &contacts[proposal.right];
    let route_bytes = left
        .regular_routes
        .len()
        .checked_add(right.regular_routes.len())
        .and_then(|count| count.checked_mul(std::mem::size_of::<RegularContactRoute>()))
        .ok_or(SpecializedError::GateResourceOverflow)?;
    let demand_bytes =
        left.demands
            .iter()
            .chain(&right.demands)
            .try_fold(0usize, |total, demand| {
                total
                    .checked_add(demand_deep_bytes(demand)?)
                    .ok_or(SpecializedError::GateResourceOverflow)
            })?;
    let required = route_bytes
        .checked_add(demand_bytes)
        .ok_or(SpecializedError::GateResourceOverflow)?;
    if required > byte_limit {
        return Err(SpecializedError::MergeCommitByteLimit {
            required,
            limit: byte_limit,
        });
    }
    let mut routes = Vec::with_capacity(left.regular_routes.len() + right.regular_routes.len());
    routes.extend(left.regular_routes.iter().copied());
    routes.extend(right.regular_routes.iter().copied());
    routes.sort_unstable();
    routes.dedup();
    let mut demands = Vec::with_capacity(left.demands.len() + right.demands.len());
    demands.extend(left.demands.iter().cloned());
    demands.extend(right.demands.iter().cloned());
    demands.sort_by(compare_demands);
    demands.dedup_by(|a, b| compare_demands(a, b) == Ordering::Equal);
    telemetry.deep_clone_count = telemetry.deep_clone_count.saturating_add(1);
    telemetry.max_commit_bytes = telemetry.max_commit_bytes.max(required);
    Ok(WorkingContact {
        position_mm: proposal
            .primary_regular
            .map_or(proposal.position_mm, |primary| primary.position_mm),
        normal: proposal
            .primary_regular
            .map_or(proposal.normal, |primary| primary.normal),
        seat_provenance: proposal.seat_provenance,
        primary_regular: proposal.primary_regular,
        regular_routes: routes,
        demands,
    })
}

fn canonicalize_and_validate(
    contacts: &mut [WorkingContact],
    islands: &[SupportIsland],
    layers: &LayerPlan,
    attach_reach: f64,
) -> Result<(), SpecializedError> {
    contacts.sort_by(compare_contacts);
    for contact in contacts.iter_mut() {
        match contact.primary_regular {
            Some(primary)
                if contact.regular_routes.contains(&primary.route)
                    && contact.position_mm == primary.position_mm
                    && contact.normal == primary.normal => {}
            None if contact.regular_routes.is_empty() => {}
            _ => return Err(SpecializedError::PrimaryRegularIdentityMismatch),
        }
        contact.demands.sort_by(compare_demands);
        for (demand_index, demand) in contact.demands.iter().enumerate() {
            if !satisfies(contact.position_mm, demand, layers, attach_reach) {
                return Err(SpecializedError::UnsatisfiedDemand { demand_index });
            }
            let island = islands
                .iter()
                .find(|island| island.id == demand.owner_island_id)
                .ok_or(SpecializedError::OwnerRouteMismatch {
                    island_id: demand.owner_island_id,
                    face_id: demand.coverage_face_id,
                })?;
            if island
                .face_ids
                .binary_search(&demand.coverage_face_id)
                .is_err()
            {
                return Err(SpecializedError::OwnerRouteMismatch {
                    island_id: demand.owner_island_id,
                    face_id: demand.coverage_face_id,
                });
            }
        }
    }
    for right in 0..contacts.len() {
        if let Some(left) = contacts[..right]
            .iter()
            .position(|other| same_point(other.position_mm, contacts[right].position_mm))
        {
            return Err(SpecializedError::DuplicatePhysicalSeat { left, right });
        }
    }
    Ok(())
}

fn satisfies(
    position: [f64; 3],
    demand: &SpecializedDemand,
    layers: &LayerPlan,
    attach_reach: f64,
) -> bool {
    match demand.kind {
        SpecializedDemandKind::Minimum => demand
            .region_points_mm
            .iter()
            .any(|point| distance3(position, *point) <= attach_reach),
        SpecializedDemandKind::IslandBirth => {
            let Some(layer) = demand.birth_layer_index else {
                return false;
            };
            let Some(z) = demand.birth_z_mm else {
                return false;
            };
            let z_tolerance = layer_tolerance(layers, layer, 2);
            (position[2] - z).abs() <= z_tolerance
                && demand.birth_footprint.as_ref().is_some_and(|solid| {
                    point_in_solid([position[0], position[1]], solid, attach_reach)
                })
        }
    }
}

fn layer_tolerance(layers: &LayerPlan, layer: usize, count: usize) -> f64 {
    (layer..layer.saturating_add(count))
        .filter_map(|index| layers.heights_mm().get(index))
        .sum()
}

fn point_in_solid(point: [f64; 2], solid: &SliceSolid, tolerance: f64) -> bool {
    let outer = solid.outer().points_xy_mm();
    let inside_outer =
        point_in_polygon(point, outer) || boundary_distance(point, outer) <= tolerance;
    inside_outer
        && !solid.holes().iter().any(|hole| {
            point_in_polygon(point, hole.points_xy_mm())
                && boundary_distance(point, hole.points_xy_mm()) > tolerance
        })
}

fn point_in_polygon(point: [f64; 2], polygon: &[[f64; 2]]) -> bool {
    let mut inside = false;
    for index in 0..polygon.len() {
        let a = polygon[index];
        let b = polygon[(index + 1) % polygon.len()];
        if (a[1] > point[1]) != (b[1] > point[1])
            && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
    }
    inside
}

fn boundary_distance(point: [f64; 2], polygon: &[[f64; 2]]) -> f64 {
    polygon
        .iter()
        .enumerate()
        .map(|(index, &a)| {
            let b = polygon[(index + 1) % polygon.len()];
            segment_distance(point, a, b)
        })
        .fold(f64::INFINITY, f64::min)
}

fn segment_distance(point: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let edge = [b[0] - a[0], b[1] - a[1]];
    let length_squared = edge[0] * edge[0] + edge[1] * edge[1];
    if length_squared == 0.0 {
        return (point[0] - a[0]).hypot(point[1] - a[1]);
    }
    let t = (((point[0] - a[0]) * edge[0] + (point[1] - a[1]) * edge[1]) / length_squared)
        .clamp(0.0, 1.0);
    (point[0] - (a[0] + t * edge[0])).hypot(point[1] - (a[1] + t * edge[1]))
}

fn compare_demands(left: &SpecializedDemand, right: &SpecializedDemand) -> Ordering {
    left.coverage_component_id
        .cmp(&right.coverage_component_id)
        .then_with(|| compare_point3(left.nominated_position_mm, right.nominated_position_mm))
        .then_with(|| left.coverage_face_id.cmp(&right.coverage_face_id))
        .then_with(|| left.owner_island_id.cmp(&right.owner_island_id))
        .then_with(|| demand_kind_key(left.kind).cmp(&demand_kind_key(right.kind)))
        .then_with(|| {
            compare_optional_f64_array_keys(
                (left.kind == SpecializedDemandKind::Minimum)
                    .then_some(left.region_points_mm.as_slice()),
                (right.kind == SpecializedDemandKind::Minimum)
                    .then_some(right.region_points_mm.as_slice()),
            )
        })
        .then_with(|| {
            numeric_cmp(
                left.birth_z_mm.unwrap_or(0.0),
                right.birth_z_mm.unwrap_or(0.0),
            )
        })
        .then_with(|| {
            compare_optional_f64_array_keys(
                left.birth_footprint
                    .as_ref()
                    .map(|solid| solid.outer().points_xy_mm()),
                right
                    .birth_footprint
                    .as_ref()
                    .map(|solid| solid.outer().points_xy_mm()),
            )
        })
}

const fn demand_kind_key(kind: SpecializedDemandKind) -> (bool, bool) {
    match kind {
        SpecializedDemandKind::Minimum => (true, false),
        SpecializedDemandKind::IslandBirth => (false, true),
    }
}

fn compare_optional_f64_array_keys<const N: usize>(
    left: Option<&[[f64; N]]>,
    right: Option<&[[f64; N]]>,
) -> Ordering {
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(left), Some(right)) => compare_f64_array_keys(
            &[left.len(), N],
            left.iter().flat_map(|point| point.iter().copied()),
            &[right.len(), N],
            right.iter().flat_map(|point| point.iter().copied()),
        ),
    }
}

fn compare_f64_array_keys<L, R>(
    left_shape: &[usize],
    left_values: L,
    right_shape: &[usize],
    right_values: R,
) -> Ordering
where
    L: IntoIterator<Item = f64>,
    R: IntoIterator<Item = f64>,
{
    left_shape.cmp(right_shape).then_with(|| {
        let mut left = left_values.into_iter();
        let mut right = right_values.into_iter();
        loop {
            match (left.next(), right.next()) {
                (Some(a), Some(b)) => {
                    let ordering = a.to_ne_bytes().cmp(&b.to_ne_bytes());
                    if ordering != Ordering::Equal {
                        return ordering;
                    }
                }
                (None, None) => return Ordering::Equal,
                (None, Some(_)) => return Ordering::Less,
                (Some(_), None) => return Ordering::Greater,
            }
        }
    })
}

fn compare_contacts(left: &WorkingContact, right: &WorkingContact) -> Ordering {
    compare_point3(left.position_mm, right.position_mm)
        .then_with(|| compare_primary_regular(left.primary_regular, right.primary_regular))
        .then_with(|| left.regular_routes.cmp(&right.regular_routes))
        .then_with(|| compare_point3(left.normal, right.normal))
        .then_with(|| compare_demand_slices(&left.demands, &right.demands))
}

fn compare_primary_regular(
    left: Option<PrimaryRegularIdentity>,
    right: Option<PrimaryRegularIdentity>,
) -> Ordering {
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(left), Some(right)) => left
            .route
            .cmp(&right.route)
            .then_with(|| compare_point3(left.position_mm, right.position_mm))
            .then_with(|| compare_point3(left.normal, right.normal)),
    }
}

fn compare_demand_slices(left: &[SpecializedDemand], right: &[SpecializedDemand]) -> Ordering {
    left.iter()
        .zip(right)
        .find_map(|(a, b)| {
            let ordering = compare_demands(a, b);
            (ordering != Ordering::Equal).then_some(ordering)
        })
        .unwrap_or_else(|| left.len().cmp(&right.len()))
}

fn compare_point3(left: [f64; 3], right: [f64; 3]) -> Ordering {
    numeric_cmp(left[0], right[0])
        .then_with(|| numeric_cmp(left[1], right[1]))
        .then_with(|| numeric_cmp(left[2], right[2]))
}

fn numeric_cmp(left: f64, right: f64) -> Ordering {
    if left == right {
        Ordering::Equal
    } else {
        left.total_cmp(&right)
    }
}

fn same_point(left: [f64; 3], right: [f64; 3]) -> bool {
    left == right
}

fn distance3(left: [f64; 3], right: [f64; 3]) -> f64 {
    (left[0] - right[0])
        .hypot(left[1] - right[1])
        .hypot(left[2] - right[2])
}

fn bound_authority_base(
    generic: &GenericContactPlan,
    regular: &SelectedContacts,
    config: &ContactSelectionConfig<'_>,
    limits: SpecializedLimits,
) -> Fnv {
    let mut hash = Fnv::new();
    hash.u64(generic.digest());
    hash.u64(regular.digest());
    hash.usize(config.coverage_samples_max_virtual_cells);
    hash.usize(config.coverage_samples_max_compact_payload_bytes);
    hash.usize(config.coverage_samples_max_working_cells);
    hash.usize(config.coverage_samples_max_peak_bytes);
    hash.usize(config.coverage_score_matrix_max_entries);
    hash.usize(config.surface_max_grid_bytes);
    hash.f64(config.target_spacing_mm.get());
    hash.f64(config.spacing_floor_mm.get());
    hash.f64(config.coverage_radius_factor.get());
    hash.f64(config.voxel_pitch_mm.get());
    hash.usize(config.min_contacts_per_island);
    hash.usize(config.coverage_repair_max_attempts);
    match config.delicacy_by_face {
        Some(values) => {
            hash.u8(1);
            hash.usize(values.len());
            for &value in values {
                hash.f64(value);
            }
        }
        None => hash.u8(0),
    }
    hash.usize(limits.max_synthetic_neighbor_pairs);
    hash.usize(limits.max_synthetic_neighbor_bytes);
    hash.usize(limits.max_satisfaction_tests);
    hash.usize(limits.max_satisfaction_neighbor_pairs);
    hash.usize(limits.max_satisfaction_neighbor_bytes);
    hash.usize(limits.max_merge_pairs);
    hash.usize(limits.max_merge_proposals);
    hash.usize(limits.max_merge_total_proposals);
    hash.usize(limits.max_merge_iterations);
    hash.usize(limits.max_merge_commit_bytes);
    hash.usize(limits.max_gate_variants);
    hash.usize(limits.max_gate_vector_bytes);
    hash.usize(limits.max_gate_contact_matrix_bytes);
    hash.usize(limits.max_gate_operations);
    hash.usize(limits.max_gate_window_variants);
    hash
}

fn hash_specialized_limits(hash: &mut Fnv, limits: SpecializedLimits) {
    hash.usize(limits.max_synthetic_neighbor_pairs);
    hash.usize(limits.max_synthetic_neighbor_bytes);
    hash.usize(limits.max_satisfaction_tests);
    hash.usize(limits.max_satisfaction_neighbor_pairs);
    hash.usize(limits.max_satisfaction_neighbor_bytes);
    hash.usize(limits.max_merge_pairs);
    hash.usize(limits.max_merge_proposals);
    hash.usize(limits.max_merge_proposal_bytes);
    hash.usize(limits.max_merge_descriptor_operations);
    hash.usize(limits.max_merge_total_proposals);
    hash.usize(limits.max_merge_iterations);
    hash.usize(limits.max_merge_commit_bytes);
    hash.usize(limits.max_gate_variants);
    hash.usize(limits.max_gate_vector_bytes);
    hash.usize(limits.max_gate_contact_matrix_bytes);
    hash.usize(limits.max_gate_operations);
    hash.usize(limits.max_gate_window_variants);
    hash.usize(limits.max_coverage_scratch_bytes);
    hash.usize(limits.max_coverage_operations);
    hash.usize(limits.max_load_scratch_bytes);
    hash.usize(limits.max_load_operations);
    hash.usize(limits.max_gate_peak_bytes);
}

/// The pool is rooted in the complete generic observation plan, including the
/// synthetic-demand seeds from which its second source is deterministically
/// derived.  Selection policy is separately bound by the plan authority.
fn repair_pool_lineage_root(generic: &GenericContactPlan) -> u64 {
    let mut hash = Fnv::new();
    hash.u64(generic.digest());
    hash.u64(generic.candidates().digest());
    hash.finish()
}

fn bound_semantic_authority_digest(
    generic: &GenericContactPlan,
    regular: &SelectedContacts,
    config: &ContactSelectionConfig<'_>,
    limits: SpecializedLimits,
) -> u64 {
    bound_authority_base(generic, regular, config, limits).finish()
}

fn bound_authority_digest(
    generic: &GenericContactPlan,
    regular: &SelectedContacts,
    config: &ContactSelectionConfig<'_>,
    limits: SpecializedLimits,
) -> u64 {
    let mut hash = bound_authority_base(generic, regular, config, limits);
    hash.usize(limits.max_merge_proposal_bytes);
    hash.usize(limits.max_merge_descriptor_operations);
    hash.usize(limits.max_coverage_scratch_bytes);
    hash.usize(limits.max_coverage_operations);
    hash.usize(limits.max_load_scratch_bytes);
    hash.usize(limits.max_load_operations);
    hash.usize(limits.max_gate_peak_bytes);
    hash.finish()
}

struct Fnv(u64);

fn hash_physical_seat_provenance(hash: &mut Fnv, provenance: PhysicalSeatProvenance) {
    match provenance {
        PhysicalSeatProvenance::Regular(primary) => {
            hash.u8(0);
            let route = primary.route();
            hash.u64(route.candidate_id());
            hash.u32(route.owner_island_id());
            hash.u32(route.coverage_face_id());
            hash.u32(route.component_id());
            hash_repair_row_source_kind(hash, route.source_kind());
            hash.point3(primary.position_mm());
            hash.point3(primary.normal());
        }
        PhysicalSeatProvenance::Specialized(representative) => {
            hash.u8(1);
            hash.u8(match representative.kind() {
                SpecializedDemandKind::Minimum => 0,
                SpecializedDemandKind::IslandBirth => 1,
            });
            hash.u32(representative.owner_island_id());
            hash.u32(representative.coverage_face_id());
            hash.u32(representative.coverage_component_id());
            hash.point3(representative.nominated_position_mm());
            hash.point3(representative.nominated_normal());
        }
    }
}

fn hash_repair_row_source_kind(hash: &mut Fnv, source_kind: RepairRowSourceKind) {
    hash.u8(match source_kind {
        RepairRowSourceKind::Generic => 0,
        RepairRowSourceKind::SyntheticIsland => 1,
    });
}

impl Fnv {
    const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    fn bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    fn u8(&mut self, value: u8) {
        self.bytes(&[value]);
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
    fn f64(&mut self, value: f64) {
        self.u64(value.to_bits());
    }
    fn point3(&mut self, point: [f64; 3]) {
        for value in point {
            self.f64(value);
        }
    }
    const fn finish(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeSet;

    use super::*;
    use geom_kernel::KnnBatchLimits;
    use plinth_core::{Finite, PositiveFinite};
    use plinth_mesh::{make_overhang_table, make_raking_blade, make_thin_fin, make_toed_slab};

    const TEST_KNN_LIMITS: KnnBatchLimits = KnnBatchLimits {
        max_source_points: 20_000_000,
        max_query_points: 20_000_000,
        max_neighbors: 1,
        max_logical_peak_bytes: 2 * 1024 * 1024 * 1024,
        max_work: 100_000_000,
    };

    fn gate_window_summary(
        route_count: usize,
        total_work: usize,
        fingerprint: u64,
        peak_bytes: usize,
    ) -> GateWindowPreflightSummary {
        GateWindowPreflightSummary {
            routes: GateRouteWindowPlan {
                route_count,
                total_work,
                retained_base_logical_bytes: 0,
                max_route_knn_logical_peak_bytes: 0,
                fingerprint,
            },
            peak_bytes,
        }
    }

    #[test]
    fn whole_gate_window_traversal_stops_before_execution_after_later_window_failure() {
        let planned_windows = RefCell::new(Vec::new());
        let execution_calls = Cell::new(0usize);
        let result = preflight_then_execute(
            traverse_whole_gate_windows(5, 2, |window_start, window_end| {
                planned_windows
                    .borrow_mut()
                    .push((window_start, window_end));
                if window_start == 0 {
                    Ok(gate_window_summary(2, 3, 7, 11))
                } else {
                    Err(SpecializedError::GateOperationLimit {
                        required: 5,
                        limit: 4,
                    })
                }
            }),
            |_| {
                execution_calls.set(execution_calls.get() + 1);
                Ok(())
            },
        );

        assert_eq!(planned_windows.into_inner(), vec![(0, 2), (2, 4)]);
        assert_eq!(
            result,
            Err(SpecializedError::GateOperationLimit {
                required: 5,
                limit: 4,
            })
        );
        assert_eq!(execution_calls.get(), 0);
    }

    #[test]
    fn whole_gate_window_traversal_accumulates_peak_at_cap_minus_one() {
        const CAP: usize = 32;
        let summary = traverse_whole_gate_windows(5, 2, |window_start, window_end| {
            Ok(gate_window_summary(
                window_end - window_start,
                window_start + 1,
                window_start as u64,
                if window_start == 2 { CAP - 1 } else { 9 },
            ))
        })
        .expect("all windows must plan");

        assert_eq!(summary.window_count, 3);
        assert_eq!(summary.total_routes, 5);
        assert_eq!(summary.total_work, 9);
        assert_eq!(summary.max_peak_bytes, CAP - 1);
        let usage = GatePeakUsage {
            proposal_bytes: 0,
            result_bytes: 0,
            matrix_bytes: summary.max_peak_bytes,
            coverage_scratch_bytes: 0,
            coverage_nearest_bytes: 0,
            coverage_retained_bytes: 0,
            load_scratch_bytes: 0,
        };
        assert_eq!(check_gate_peak(usage, CAP - 1), Ok(CAP - 1));
        assert_eq!(
            check_gate_peak(usage, CAP - 2),
            Err(SpecializedError::GatePeakByteLimit {
                required: CAP - 1,
                limit: CAP - 2,
            })
        );
    }

    fn minimum_demand(
        owner: u32,
        face: u32,
        component: u32,
        position: [f64; 3],
    ) -> SpecializedDemand {
        SpecializedDemand {
            kind: SpecializedDemandKind::Minimum,
            owner_island_id: owner,
            coverage_face_id: face,
            coverage_component_id: component,
            nominated_position_mm: position,
            nominated_normal: [0.0, 0.0, -1.0],
            region_points_mm: vec![position],
            birth_z_mm: None,
            birth_layer_index: None,
            birth_footprint: None,
        }
    }

    fn test_regular_contact(
        position_mm: [f64; 3],
        normal: [f64; 3],
        route: RegularContactRoute,
        demands: Vec<SpecializedDemand>,
    ) -> WorkingContact {
        WorkingContact {
            position_mm,
            normal,
            seat_provenance: PhysicalSeatProvenance::Regular(PrimaryRegularIdentity {
                route,
                position_mm,
                normal,
            }),
            primary_regular: Some(PrimaryRegularIdentity {
                route,
                position_mm,
                normal,
            }),
            regular_routes: vec![route],
            demands,
        }
    }

    #[test]
    fn route_keyed_nearest_is_exact_deduplicated_and_leaves_unrouted_rows_infinite() {
        let first_route = RegularContactRoute {
            candidate_id: 7,
            owner_island_id: 3,
            coverage_face_id: 0,
            component_id: 11,
            source_kind: RepairRowSourceKind::Generic,
        };
        let second_route = RegularContactRoute {
            candidate_id: 8,
            owner_island_id: 3,
            coverage_face_id: 1,
            component_id: 12,
            source_kind: RepairRowSourceKind::Generic,
        };
        let mut first =
            test_regular_contact([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], first_route, Vec::new());
        first.regular_routes.push(first_route); // Same seat and key must remain one source.
        first.regular_routes.push(second_route); // A second key on the same seat remains distinct.
        let contacts = vec![first];
        let samples = vec![
            CoverageSample {
                island_id: 3,
                face_id: 0,
                component_id: 11,
                point_mm: [3.0, 4.0, 0.0],
                area_mm2: 1.0,
            },
            CoverageSample {
                island_id: 3,
                face_id: 1,
                component_id: 12,
                point_mm: [0.0, 0.0, 2.0],
                area_mm2: 1.0,
            },
            CoverageSample {
                island_id: 4,
                face_id: 2,
                component_id: 11,
                point_mm: [0.0, 0.0, 0.0],
                area_mm2: 1.0,
            },
        ];
        let mut nearest = vec![f64::INFINITY; samples.len()];
        let planned =
            plan_variant_nearest_routes(&contacts, None, &samples, TEST_KNN_LIMITS).unwrap();
        assert_eq!(
            planned.measurement_counts(),
            (3, 2, 2, 1, 2, 2),
            "duplicate logical rows remain memberships while each route index deduplicates physical seats"
        );
        fill_variant_nearest(&contacts, None, &samples, &mut nearest, planned).unwrap();
        assert_eq!(nearest[0], 5.0);
        assert_eq!(nearest[1], 2.0);
        assert!(nearest[2].is_infinite());
    }

    #[test]
    fn route_membership_count_preserves_duplicate_logical_rows_before_index_deduplication() {
        let points = vec![
            CoverageRoutePoint {
                physical_id: 9,
                island_id: 3,
                component_id: 11,
                position_mm: [0.0, 0.0, 0.0],
            },
            CoverageRoutePoint {
                physical_id: 9,
                island_id: 3,
                component_id: 11,
                position_mm: [0.0, 0.0, 0.0],
            },
        ];
        let samples = vec![CoverageSample {
            island_id: 3,
            face_id: 0,
            component_id: 11,
            point_mm: [0.0, 0.0, 0.0],
            area_mm2: 1.0,
        }];

        let planned = plan_route_points_nearest(&points, &samples, TEST_KNN_LIMITS)
            .expect("duplicate logical rows are one indexed seat");
        assert_eq!(planned.measurement_counts(), (2, 2, 1, 1, 1, 1));
        let mut nearest = vec![f64::INFINITY; samples.len()];
        fill_route_points_nearest(&points, &samples, &mut nearest, planned)
            .expect("the deduplicated index remains valid");
        assert_eq!(nearest, vec![0.0]);
    }

    #[test]
    fn route_keyed_nearest_refuses_knn_caps_before_build_or_query() {
        let route = RegularContactRoute {
            candidate_id: 7,
            owner_island_id: 3,
            coverage_face_id: 0,
            component_id: 11,
            source_kind: RepairRowSourceKind::Generic,
        };
        let contacts = vec![test_regular_contact(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            route,
            Vec::new(),
        )];
        let samples = vec![CoverageSample {
            island_id: 3,
            face_id: 0,
            component_id: 11,
            point_mm: [0.0, 0.0, 0.0],
            area_mm2: 1.0,
        }];
        let refused = KnnBatchLimits {
            max_source_points: 0,
            ..TEST_KNN_LIMITS
        };
        assert!(matches!(
            plan_variant_nearest_routes(&contacts, None, &samples, refused),
            Err(SpecializedError::CoverageNearest(_))
        ));
    }

    #[test]
    fn route_nearest_peak_counts_retained_metadata_and_serial_knn_maximum() {
        let first_route = RegularContactRoute {
            candidate_id: 7,
            owner_island_id: 3,
            coverage_face_id: 0,
            component_id: 11,
            source_kind: RepairRowSourceKind::Generic,
        };
        let second_route = RegularContactRoute {
            candidate_id: 8,
            owner_island_id: 4,
            coverage_face_id: 1,
            component_id: 12,
            source_kind: RepairRowSourceKind::Generic,
        };
        let contacts = vec![
            test_regular_contact([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], first_route, Vec::new()),
            test_regular_contact([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], second_route, Vec::new()),
        ];
        let samples = vec![
            CoverageSample {
                island_id: 3,
                face_id: 0,
                component_id: 11,
                point_mm: [0.0, 0.0, 0.0],
                area_mm2: 1.0,
            },
            CoverageSample {
                island_id: 4,
                face_id: 1,
                component_id: 12,
                point_mm: [1.0, 0.0, 0.0],
                area_mm2: 1.0,
            },
        ];
        let planned = plan_variant_nearest_routes(&contacts, None, &samples, TEST_KNN_LIMITS)
            .expect("two independent route batches plan");
        let max_route_peak = planned
            .route_plans
            .iter()
            .map(|route| route.knn.logical_peak_bytes())
            .max()
            .expect("two planned routes");
        let summed_route_peaks = planned
            .route_plans
            .iter()
            .map(|route| route.knn.logical_peak_bytes())
            .sum::<usize>();
        let expected_retained_base = 2 * std::mem::size_of::<CoverageRoute>()
            + 2 * std::mem::size_of::<(CoverageRoute, usize)>()
            + 2 * std::mem::size_of::<u64>()
            + 2 * std::mem::size_of::<(CoverageRoute, usize)>()
            + 2 * std::mem::size_of::<CoverageRouteKnnPlan>()
            + std::mem::size_of::<u64>()
            + std::mem::size_of::<usize>();
        assert_eq!(
            planned.retained_base_logical_bytes, expected_retained_base,
            "route keys, deduplicated source ids, grouped row keys/indices, planned-route metadata, and serial gather dedup/index vectors are all counted"
        );
        let logical_peak_bytes = coverage_route_nearest_peak_bytes(
            planned.retained_base_logical_bytes,
            planned.max_route_knn_logical_peak_bytes,
        )
        .expect("checked route peak");
        assert_eq!(
            logical_peak_bytes,
            planned.retained_base_logical_bytes + max_route_peak,
            "serial route batches retain one maximum, not their sum"
        );
        assert_ne!(
            logical_peak_bytes,
            planned.retained_base_logical_bytes + summed_route_peaks,
            "two route batches must not be charged as simultaneous kNN work"
        );

        let usage = GatePeakUsage {
            proposal_bytes: 0,
            result_bytes: 0,
            matrix_bytes: 0,
            coverage_scratch_bytes: 0,
            coverage_nearest_bytes: logical_peak_bytes,
            coverage_retained_bytes: 0,
            load_scratch_bytes: 0,
        };
        assert_eq!(
            check_gate_peak(usage, logical_peak_bytes),
            Ok(logical_peak_bytes)
        );
        assert_eq!(
            check_gate_peak(usage, logical_peak_bytes - 1),
            Err(SpecializedError::GatePeakByteLimit {
                required: logical_peak_bytes,
                limit: logical_peak_bytes - 1,
            })
        );
    }

    #[test]
    fn route_nearest_peak_overflow_is_refused() {
        assert_eq!(
            coverage_route_nearest_peak_bytes(usize::MAX, 1),
            Err(SpecializedError::GateResourceOverflow)
        );
    }

    fn repair_candidate(id: u64, face_id: u32, point_mm: [f64; 3]) -> crate::ContactCandidate {
        crate::ContactCandidate {
            id: ContactCandidateId(id),
            face_id,
            point_mm,
            normal: [0.0, 0.0, -1.0],
            supported_area_mm2: 1.0,
            overhang_excess: 1.0,
            delicacy: 0.0,
        }
    }

    #[test]
    fn repair_pool_binds_generic_and_synthetic_rows_in_exact_route_order() {
        let generic = ContactCandidatePool {
            candidates: vec![repair_candidate(7, 0, [1.0, 0.0, 0.0])],
        };
        let synthetic = ContactCandidatePool {
            candidates: vec![repair_candidate(8, 1, [2.0, 0.0, 0.0])],
        };
        let pool = RepairPool::build(
            41,
            &generic,
            &[
                SupportIsland {
                    id: 3,
                    face_ids: vec![0],
                },
                SupportIsland {
                    id: 4,
                    face_ids: vec![0],
                },
            ],
            Some(&synthetic),
            &[SupportIsland {
                id: 5,
                face_ids: vec![1],
            }],
            &[11, 12],
        )
        .unwrap();
        assert_eq!(pool.lineage_root, 41);
        assert_eq!(pool.rows.len(), 3);
        assert_eq!(
            pool.rows.iter().map(|row| row.order).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(pool.rows[0].candidate.point_mm, [1.0, 0.0, 0.0]);
        assert_eq!(pool.rows[2].candidate.point_mm, [2.0, 0.0, 0.0]);
        assert_eq!(pool.rows[0].id.source_kind, RepairRowSourceKind::Generic);
        assert_eq!(
            pool.rows[2].id.source_kind,
            RepairRowSourceKind::SyntheticIsland
        );
        assert_eq!(
            pool.compute_digest(),
            RepairPool::build(
                41,
                &generic,
                &[
                    SupportIsland {
                        id: 3,
                        face_ids: vec![0],
                    },
                    SupportIsland {
                        id: 4,
                        face_ids: vec![0],
                    },
                ],
                Some(&synthetic),
                &[SupportIsland {
                    id: 5,
                    face_ids: vec![1],
                }],
                &[11, 12],
            )
            .unwrap()
            .digest
        );
    }

    #[test]
    fn repair_pool_qualifies_equal_numeric_ids_by_source_and_validates_occupancy() {
        let generic = ContactCandidatePool {
            candidates: vec![
                repair_candidate(7, 0, [1.0, 0.0, 0.0]),
                repair_candidate(8, 0, [3.0, 0.0, 0.0]),
                crate::ContactCandidate {
                    id: ContactCandidateId(0),
                    face_id: 0,
                    point_mm: [4.0, 0.0, 0.0],
                    normal: [0.0, 0.0, -1.0],
                    supported_area_mm2: 0.0,
                    overhang_excess: 0.0,
                    delicacy: 0.0,
                },
            ],
        };
        let synthetic = ContactCandidatePool {
            candidates: vec![repair_candidate(7, 0, [2.0, 0.0, 0.0])],
        };
        let pool = RepairPool::build(
            1,
            &generic,
            &[SupportIsland {
                id: 3,
                face_ids: vec![0],
            }],
            Some(&synthetic),
            &[SupportIsland {
                id: 3,
                face_ids: vec![0],
            }],
            &[11],
        )
        .unwrap();
        let generic_route = RegularContactRoute {
            candidate_id: 7,
            owner_island_id: 3,
            coverage_face_id: 0,
            component_id: 11,
            source_kind: RepairRowSourceKind::Generic,
        };
        let synthetic_route = RegularContactRoute {
            source_kind: RepairRowSourceKind::SyntheticIsland,
            ..generic_route
        };
        assert_ne!(
            pool.resolve_route(generic_route).unwrap(),
            pool.resolve_route(synthetic_route).unwrap()
        );
        let generic_contact = SpecializedPhysicalContact {
            contact_id: 0,
            position_mm: [1.0, 0.0, 0.0],
            normal: [0.0, 0.0, -1.0],
            seat_provenance: PhysicalSeatProvenance::Regular(PrimaryRegularIdentity {
                route: generic_route,
                position_mm: [1.0, 0.0, 0.0],
                normal: [0.0, 0.0, -1.0],
            }),
            carried_area_mm2: 0.0,
            primary_regular: None,
            regular_routes: vec![generic_route],
            demands: vec![minimum_demand(3, 0, 11, [1.0, 0.0, 0.0])],
        };
        let synthetic_contact = SpecializedPhysicalContact {
            contact_id: 1,
            position_mm: [2.0, 0.0, 0.0],
            normal: [0.0, 0.0, -1.0],
            seat_provenance: PhysicalSeatProvenance::Regular(PrimaryRegularIdentity {
                route: synthetic_route,
                position_mm: [2.0, 0.0, 0.0],
                normal: [0.0, 0.0, -1.0],
            }),
            carried_area_mm2: 0.0,
            primary_regular: None,
            regular_routes: vec![synthetic_route],
            demands: Vec::new(),
        };

        // Occupancy and failure are exact opaque identities: occupying the
        // generic candidate 7 must not alias the synthetic candidate 7.
        let generic_only_occupied = pool
            .occupied_for_contacts(std::slice::from_ref(&generic_contact))
            .unwrap();
        let generic_only_ids = generic_only_occupied
            .iter()
            .map(RepairRowOccupancy::row_id)
            .collect::<BTreeSet<_>>();
        let synthetic_twin = pool.resolve_route(synthetic_route).unwrap();
        let generic_twin = pool.resolve_route(generic_route).unwrap();
        assert_eq!(
            pool.row(synthetic_twin).unwrap().candidate().id,
            ContactCandidateId(7)
        );
        assert_eq!(
            pool.row(generic_twin).unwrap().candidate().id,
            ContactCandidateId(7)
        );
        let no_occupied_rows = BTreeSet::new();
        let failed_generic_twin = BTreeSet::from([generic_twin]);
        let generic_failed_only = no_occupied_rows
            .union(&failed_generic_twin)
            .copied()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            pool.eligible_rows(3, 11, &generic_failed_only)
                .iter()
                .map(|row| row.order())
                .collect::<Vec<_>>(),
            vec![1, 2, 3],
            "failing generic candidate 7 must leave synthetic candidate 7 eligible"
        );
        let failed_synthetic_twin = BTreeSet::from([synthetic_twin]);
        let synthetic_failed_only = no_occupied_rows
            .union(&failed_synthetic_twin)
            .copied()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            pool.eligible_rows(3, 11, &synthetic_failed_only)
                .iter()
                .map(|row| row.order())
                .collect::<Vec<_>>(),
            vec![0, 1, 2],
            "failing synthetic candidate 7 must leave generic candidate 7 eligible"
        );
        assert_eq!(
            pool.eligible_rows(3, 11, &generic_only_ids)
                .iter()
                .map(|row| row.order())
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        let failed_generic_eight = BTreeSet::from([pool.rows[1].id()]);
        let occupied_and_failed = generic_only_ids
            .union(&failed_generic_eight)
            .copied()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            pool.eligible_rows(3, 11, &occupied_and_failed)
                .iter()
                .map(|row| row.order())
                .collect::<Vec<_>>(),
            vec![2, 3],
            "the unoccupied synthetic candidate-7 twin remains eligible"
        );
        let occupied_and_both_failed = occupied_and_failed
            .union(&BTreeSet::from([synthetic_twin]))
            .copied()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            pool.eligible_rows(3, 11, &occupied_and_both_failed)
                .iter()
                .map(|row| row.order())
                .collect::<Vec<_>>(),
            vec![2]
        );

        let occupied = pool
            .occupied_for_contacts(&[generic_contact.clone(), synthetic_contact])
            .unwrap();
        assert_eq!(occupied.len(), 2);
        assert_eq!(occupied[0].row_id(), pool.rows[0].id());
        assert!(occupied[0].shared_demanded());
        assert_eq!(occupied[1].row_id(), pool.rows[3].id());
        assert!(!occupied[1].shared_demanded());
        assert_eq!(
            pool.row(occupied[0].row_id()).unwrap().candidate().id,
            ContactCandidateId(7)
        );

        let occupied_ids = occupied
            .iter()
            .map(RepairRowOccupancy::row_id)
            .collect::<BTreeSet<_>>();
        let eligible = pool.eligible_rows(3, 11, &occupied_ids);
        assert_eq!(
            eligible.iter().map(|row| row.order()).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(eligible[1].candidate().id, ContactCandidateId(0));
        let failed = BTreeSet::from([eligible[0].id()]);
        assert_eq!(
            pool.eligible_rows(3, 11, &failed)
                .iter()
                .map(|row| row.order())
                .collect::<Vec<_>>(),
            vec![0, 2, 3]
        );
        let excluded = occupied_ids
            .union(&failed)
            .copied()
            .collect::<BTreeSet<_>>();
        let eligible_after_failure = pool.eligible_rows(3, 11, &excluded);
        assert_eq!(eligible_after_failure.len(), 1);
        assert_eq!(
            eligible_after_failure[0].candidate().id,
            ContactCandidateId(0)
        );
        assert!(matches!(
            pool.validate_contacts(&[generic_contact.clone(), generic_contact]),
            Err(SpecializedError::DuplicateRepairRoute { .. })
        ));
    }

    #[test]
    fn repair_pool_rejects_stale_geometry() {
        let generic = ContactCandidatePool {
            candidates: vec![
                repair_candidate(7, 0, [1.0, 0.0, 0.0]),
                repair_candidate(8, 0, [2.0, 0.0, 0.0]),
            ],
        };
        let mut pool = RepairPool::build(
            9,
            &generic,
            &[SupportIsland {
                id: 3,
                face_ids: vec![0],
            }],
            None,
            &[],
            &[11],
        )
        .unwrap();
        pool.rows[0].candidate.point_mm[0] = 99.0;
        assert!(matches!(
            pool.validate_lineage(9),
            Err(SpecializedError::StaleRepairPool)
        ));
    }

    #[test]
    fn synthetic_selection_is_baseline_only_not_the_legacy_repair_adapter() {
        let initial_delicacy = [0.0; 4];
        let mut selection =
            ContactSelectionConfig::current(Some(&initial_delicacy), 65_536, 65_536, 1 << 20)
                .expect("registered selection policy");
        // Keep one initial S1 seat. The second legal seat is close enough to
        // be rejected by Poisson spacing, but shifted far enough to repair
        // uncovered dyadic samples on this deliberately large underside.
        selection.min_contacts_per_island = 1;
        selection.coverage_repair_max_attempts = 1;
        let spacing = selection.target_spacing_mm.get();
        let side = selection.coverage_radius_factor.get() * spacing + spacing * 0.5;
        let height = spacing * 0.5;
        let mesh = Mesh::new(
            vec![
                [0.0, 0.0, 0.0],
                [side, 0.0, 0.0],
                [0.0, height, 0.0],
                [0.0, 0.0, side],
            ],
            vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]],
        )
        .expect("closed synthetic selection fixture");
        let delicacy = vec![0.0; mesh.tris().len()];
        selection.delicacy_by_face = Some(&delicacy);
        let first = [spacing * 0.2, spacing * 0.1, 0.0];
        let candidates = ContactCandidatePool {
            candidates: vec![
                crate::ContactCandidate {
                    id: ContactCandidateId(1),
                    face_id: 0,
                    point_mm: first,
                    normal: [0.0, 0.0, -1.0],
                    supported_area_mm2: side * height * 0.5,
                    overhang_excess: 1.0,
                    delicacy: 0.0,
                },
                crate::ContactCandidate {
                    id: ContactCandidateId(2),
                    face_id: 0,
                    point_mm: [first[0] + spacing * 0.75, first[1], 0.0],
                    normal: [0.0, 0.0, -1.0],
                    supported_area_mm2: side * height * 0.5,
                    overhang_excess: 1.0,
                    delicacy: 0.0,
                },
            ],
        };
        let islands = vec![SupportIsland {
            id: 91,
            face_ids: vec![0],
        }];
        let baseline = crate::select_contacts_baseline(&mesh, &candidates, &islands, &selection)
            .expect("baseline selection");
        let legacy = crate::select_contacts(&mesh, &candidates, &islands, &selection)
            .expect("legacy repair selection");
        assert_eq!(baseline.contacts().len(), 1);
        assert_eq!(legacy.contacts().len(), 2);
        assert_eq!(legacy.contacts()[1].candidate_id, ContactCandidateId(2));
        let selected = select_synthetic_baseline(&mesh, &candidates, &islands, &selection)
            .expect("synthetic baseline selection");
        assert_eq!(selected, baseline);
        assert_ne!(selected, legacy);
    }

    #[test]
    fn exact_near_and_cross_face_demands_emit_every_canonical_representative() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let left = minimum_demand(0, 2, 7, [1.0, 2.0, 0.1]);
        let right = minimum_demand(1, 9, 7, [1.1, 2.0, 0.1]);
        let contacts = vec![
            WorkingContact::specialized(right),
            WorkingContact::specialized(left),
        ];
        let proposals = test_pair_proposals(&contacts, &layers, 0.2, 2.5);
        assert_eq!(proposals.len(), 2);
        assert_eq!(proposals[0].position_mm, [1.0, 2.0, 0.1]);
        assert_eq!(proposals[1].position_mm, [1.1, 2.0, 0.1]);
        let proposal = &proposals[0];
        let merged = commit_merge(
            proposal,
            &contacts,
            usize::MAX,
            &mut MergeAllocationTelemetry::default(),
        )
        .unwrap();
        assert_eq!(merged.position_mm, [1.0, 2.0, 0.1]);
        assert_eq!(merged.demands.len(), 2);
        assert_eq!(merged.demands[0].coverage_face_id, 2);
        assert_eq!(merged.demands[1].coverage_face_id, 9);
    }

    #[test]
    fn pure_regular_and_specialized_contacts_are_never_merge_candidates() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let demand = minimum_demand(0, 2, 7, [1.0, 2.0, 0.1]);
        let contacts = vec![
            test_regular_contact(
                [1.0, 2.0, 0.1],
                [0.0, 0.0, -1.0],
                RegularContactRoute {
                    candidate_id: 44,
                    owner_island_id: 0,
                    coverage_face_id: 2,
                    component_id: 7,
                    source_kind: RepairRowSourceKind::Generic,
                },
                Vec::new(),
            ),
            WorkingContact::specialized(demand),
        ];
        let proposals = test_pair_proposals(&contacts, &layers, 0.2, 2.5);
        assert!(proposals.is_empty());
    }

    #[test]
    fn merge_pair_refusal_is_exact_at_cap_and_cap_plus_one() {
        assert_eq!(check_merge_pair_limit(2, 1), Ok(()));
        assert_eq!(
            check_merge_pair_limit(2, 0),
            Err(SpecializedError::MergePairLimit {
                required: 1,
                limit: 0,
            })
        );
    }

    #[test]
    fn proposal_generation_refuses_before_the_cap_plus_one_push_or_byte_allocation() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let contacts = vec![
            WorkingContact::specialized(minimum_demand(0, 2, 7, [1.0, 2.0, 0.1])),
            WorkingContact::specialized(minimum_demand(1, 9, 7, [1.1, 2.0, 0.1])),
        ];
        let mut proposals = Vec::new();
        let mut operations = 0;
        let mut limits = SpecializedLimits::production(TEST_KNN_LIMITS);
        limits.max_merge_proposals = 1;
        assert_eq!(
            gather_pair_proposals(
                &contacts,
                0,
                1,
                &layers,
                0.2,
                2.5,
                limits,
                &mut operations,
                &mut proposals,
            ),
            Err(SpecializedError::MergeProposalLimit {
                required: 2,
                limit: 1,
            })
        );
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals.capacity(), 1);

        proposals.clear();
        operations = 0;
        limits.max_merge_proposals = 2;
        limits.max_merge_proposal_bytes = 2 * std::mem::size_of::<MergeProposal>() - 1;
        assert_eq!(
            gather_pair_proposals(
                &contacts,
                0,
                1,
                &layers,
                0.2,
                2.5,
                limits,
                &mut operations,
                &mut proposals,
            ),
            Err(SpecializedError::MergeProposalByteLimit {
                required: 2 * std::mem::size_of::<MergeProposal>(),
                limit: 2 * std::mem::size_of::<MergeProposal>() - 1,
            })
        );
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals.capacity(), 1);
    }

    #[test]
    fn representative_selection_skips_a_worse_lowest_seat_for_the_next_safe_seat() {
        let baseline = FrozenSpecializedGateVector {
            failing_uncovered_area_mm2: 0.0,
            max_patch_area_mm2: 0.0,
            failing_patch_count: 0,
            worst_gap_mm: 0.0,
            load_violation_count: 0,
            unsupported_specialized_count: 0,
        };
        let mut worse = baseline;
        worse.load_violation_count = 1;
        assert_eq!(
            first_safe_proposal(&[baseline, worse, baseline], baseline),
            Some(1)
        );
    }

    #[test]
    fn descriptor_work_caps_huge_region_and_polygon_payloads_before_generation() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let mut huge_region = minimum_demand(0, 2, 7, [0.0, 0.0, 0.1]);
        huge_region.region_points_mm = vec![[0.0, 0.0, 0.1]; 10_000];
        let polygon = crate::perception::test_slice_solid(
            (0..10_000)
                .map(|index| {
                    let angle = index as f64 * std::f64::consts::TAU / 10_000.0;
                    [angle.cos(), angle.sin()]
                })
                .collect(),
        );
        let birth = SpecializedDemand {
            kind: SpecializedDemandKind::IslandBirth,
            owner_island_id: 1,
            coverage_face_id: 9,
            coverage_component_id: 7,
            nominated_position_mm: [0.1, 0.0, 0.1],
            nominated_normal: [0.0, 0.0, -1.0],
            region_points_mm: Vec::new(),
            birth_z_mm: Some(0.1),
            birth_layer_index: Some(0),
            birth_footprint: Some(polygon),
        };
        let contacts = vec![
            WorkingContact::specialized(huge_region),
            WorkingContact::specialized(birth),
        ];
        let satisfaction = contacts
            .iter()
            .flat_map(|contact| &contact.demands)
            .try_fold(0usize, |total, demand| {
                total
                    .checked_add(demand_satisfaction_work(demand)?)
                    .ok_or(SpecializedError::GateResourceOverflow)
            })
            .unwrap();
        let exact = 4 + 2 * satisfaction;
        let mut limits = SpecializedLimits::production(TEST_KNN_LIMITS);
        limits.max_merge_descriptor_operations = exact - 1;
        let mut operations = 0;
        let mut proposals = Vec::new();
        assert_eq!(
            gather_pair_proposals(
                &contacts,
                0,
                1,
                &layers,
                0.2,
                2.5,
                limits,
                &mut operations,
                &mut proposals,
            ),
            Err(SpecializedError::MergeDescriptorOperationLimit {
                required: exact,
                limit: exact - 1,
            })
        );
        assert!(proposals.is_empty());
    }

    #[test]
    fn zero_descriptor_cap_refuses_before_payload_work_or_proposal_allocation() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let mut demand = minimum_demand(0, 2, 7, [0.0, 0.0, 0.1]);
        demand.region_points_mm = vec![[0.0, 0.0, 0.1]; 10_000];
        let contacts = vec![
            WorkingContact::specialized(demand.clone()),
            WorkingContact::specialized(demand),
        ];
        let mut limits = SpecializedLimits::production(TEST_KNN_LIMITS);
        limits.max_merge_descriptor_operations = 0;
        let mut operations = 0;
        let mut proposals = Vec::new();
        assert_eq!(
            gather_pair_proposals(
                &contacts,
                0,
                1,
                &layers,
                0.2,
                2.5,
                limits,
                &mut operations,
                &mut proposals,
            ),
            Err(SpecializedError::MergeDescriptorOperationLimit {
                required: 1,
                limit: 0,
            })
        );
        assert_eq!(operations, 0);
        assert_eq!(proposals.capacity(), 0);
    }

    #[test]
    fn same_seat_representatives_keep_face_identity_and_python_key_order() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let position = [1.0, 2.0, 0.1];
        let mut face_ten = minimum_demand(0, 10, 7, position);
        face_ten.nominated_normal = [-1.0, 0.0, 0.0];
        let mut face_one = minimum_demand(1, 1, 7, position);
        face_one.nominated_normal = [1.0, 0.0, 0.0];
        let proposals = test_pair_proposals(
            &[
                WorkingContact::specialized(face_ten),
                WorkingContact::specialized(face_one),
            ],
            &layers,
            0.2,
            2.5,
        );
        assert_eq!(proposals.len(), 2);
        assert_eq!(proposals[0].representative_face_id, 1);
        assert_eq!(proposals[1].representative_face_id, 10);
    }

    #[test]
    fn candidate_normals_enumerate_seats_but_do_not_split_one_logical_demand() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let position = [1.0, 2.0, 0.1];
        let mut first = minimum_demand(0, 10, 7, position);
        first.nominated_normal = [-1.0, 0.0, 0.0];
        let mut second = first.clone();
        second.nominated_normal = [1.0, 0.0, 0.0];
        assert_eq!(compare_demands(&first, &second), Ordering::Equal);

        let contacts = vec![
            WorkingContact::specialized(first),
            WorkingContact::specialized(second),
        ];
        let proposals = test_pair_proposals(&contacts, &layers, 0.2, 2.5);
        assert_eq!(proposals.len(), 2);
        assert_eq!(proposals[0].representative_normal, [-1.0, 0.0, 0.0]);
        assert_eq!(proposals[1].representative_normal, [1.0, 0.0, 0.0]);

        let merged = commit_merge(
            &proposals[0],
            &contacts,
            usize::MAX,
            &mut MergeAllocationTelemetry::default(),
        )
        .unwrap();
        assert_eq!(merged.demands.len(), 1);
        assert_eq!(merged.demands[0].nominated_normal, [-1.0, 0.0, 0.0]);
    }

    #[test]
    fn shared_regular_merge_enumerates_logical_keys_not_route_provenance() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let position = [1.0, 2.0, 0.1];
        let mut face_ten = minimum_demand(0, 10, 7, position);
        face_ten.nominated_normal = [-1.0, 0.0, 0.0];
        let mut face_one = minimum_demand(0, 1, 7, position);
        face_one.nominated_normal = [1.0, 0.0, 0.0];
        let contacts = vec![
            test_regular_contact(
                position,
                [0.0, 0.0, -1.0],
                RegularContactRoute {
                    candidate_id: 44,
                    owner_island_id: 0,
                    coverage_face_id: 99,
                    component_id: 88,
                    source_kind: RepairRowSourceKind::Generic,
                },
                vec![face_ten],
            ),
            WorkingContact::specialized(face_one),
        ];
        let proposals = test_pair_proposals(&contacts, &layers, 0.2, 2.5);
        assert_eq!(proposals.len(), 2);
        assert_eq!(proposals[0].representative_component_id, 7);
        assert_eq!(proposals[0].representative_face_id, 1);
        assert_eq!(proposals[0].representative_normal, [1.0, 0.0, 0.0]);
        // Logical keys still enumerate demand provenance while physical fields
        // retain the shared regular contact exactly.
        assert_eq!(proposals[0].normal, contacts[0].normal);
        assert_eq!(proposals[1].representative_component_id, 7);
        assert_eq!(proposals[1].representative_face_id, 10);
        assert_eq!(proposals[1].representative_normal, [-1.0, 0.0, 0.0]);
        assert_eq!(proposals[1].normal, contacts[0].normal);
        assert!(proposals
            .iter()
            .all(|proposal| proposal.representative_face_id != 99));
    }

    #[test]
    fn shared_regular_rejects_logical_keys_not_at_an_original_regular_lock() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let locked = [1.0, 2.0, 0.1];
        let displaced = [1.1, 2.0, 0.1];
        let specialized = vec![
            WorkingContact::specialized(minimum_demand(0, 1, 7, locked)),
            WorkingContact::specialized(minimum_demand(1, 2, 7, displaced)),
        ];
        let first_proposals = test_pair_proposals(&specialized, &layers, 0.2, 2.5);
        assert_eq!(first_proposals[0].position_mm, locked);
        let combined = commit_merge(
            &first_proposals[0],
            &specialized,
            usize::MAX,
            &mut MergeAllocationTelemetry::default(),
        )
        .unwrap();
        assert_eq!(combined.demands.len(), 2);
        assert!(matches!(
            combined.seat_provenance,
            PhysicalSeatProvenance::Specialized(representative)
                if representative.coverage_face_id() == 1
                    && representative.owner_island_id() == 0
                    && representative.nominated_position_mm() == locked
        ));

        let shared_proposals = test_pair_proposals(
            &[
                test_regular_contact(
                    locked,
                    [0.0, 0.0, -1.0],
                    RegularContactRoute {
                        candidate_id: 44,
                        owner_island_id: 0,
                        coverage_face_id: 99,
                        component_id: 7,
                        source_kind: RepairRowSourceKind::Generic,
                    },
                    // An exact regular disposition makes this contact
                    // specialized before consolidation begins.
                    vec![minimum_demand(2, 3, 7, locked)],
                ),
                combined,
            ],
            &layers,
            0.2,
            2.5,
        );
        assert_eq!(shared_proposals.len(), 2);
        assert_eq!(shared_proposals[0].representative_face_id, 1);
        assert_eq!(shared_proposals[1].representative_face_id, 3);
        assert!(shared_proposals
            .iter()
            .all(|proposal| proposal.representative_face_id != 2));
        assert!(shared_proposals
            .iter()
            .all(|proposal| proposal.representative_position_mm == locked));
    }

    #[test]
    fn iterative_shared_commits_keep_regular_physical_normal() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let locked = [1.0, 2.0, 0.1];
        let regular_normal = [0.0, 0.0, -1.0];
        let mut first = minimum_demand(0, 10, 7, locked);
        first.nominated_normal = [1.0, 0.0, 0.0];
        let mut second = minimum_demand(1, 11, 7, locked);
        second.nominated_normal = [0.0, 1.0, 0.0];
        let mut third = minimum_demand(2, 12, 7, locked);
        third.nominated_normal = [-1.0, 0.0, 0.0];
        let mut shared = test_regular_contact(
            locked,
            regular_normal,
            RegularContactRoute {
                candidate_id: 44,
                owner_island_id: 0,
                coverage_face_id: 99,
                component_id: 7,
                source_kind: RepairRowSourceKind::Generic,
            },
            vec![first],
        );
        for next in [second, third] {
            let contacts = vec![shared, WorkingContact::specialized(next)];
            let proposals = test_pair_proposals(&contacts, &layers, 0.2, 2.5);
            assert!(!proposals.is_empty());
            assert_eq!(proposals[0].normal, regular_normal);
            assert_ne!(proposals[0].representative_normal, regular_normal);
            assert_eq!(
                proposals[0].representative_key().normal,
                proposals[0].representative_normal
            );
            shared = commit_merge(
                &proposals[0],
                &contacts,
                usize::MAX,
                &mut MergeAllocationTelemetry::default(),
            )
            .unwrap();
            assert_eq!(shared.position_mm, locked);
            assert_eq!(shared.normal, regular_normal);
        }
        assert_eq!(shared.demands.len(), 3);
    }

    #[test]
    fn route_union_sorting_does_not_replace_the_first_regular_primary_identity() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let position = [1.0, 2.0, 0.1];
        let first_normal = [0.0, 0.0, -1.0];
        let contacts = vec![
            test_regular_contact(
                position,
                first_normal,
                RegularContactRoute {
                    candidate_id: 99,
                    owner_island_id: 8,
                    coverage_face_id: 80,
                    component_id: 800,
                    source_kind: RepairRowSourceKind::Generic,
                },
                vec![minimum_demand(0, 10, 7, position)],
            ),
            test_regular_contact(
                position,
                [1.0, 0.0, 0.0],
                RegularContactRoute {
                    candidate_id: 1,
                    owner_island_id: 9,
                    coverage_face_id: 90,
                    component_id: 900,
                    source_kind: RepairRowSourceKind::Generic,
                },
                vec![minimum_demand(1, 11, 7, position)],
            ),
        ];
        let proposals = test_pair_proposals(&contacts, &layers, 0.2, 2.5);
        let merged = commit_merge(
            &proposals[0],
            &contacts,
            usize::MAX,
            &mut MergeAllocationTelemetry::default(),
        )
        .unwrap();

        assert_eq!(merged.regular_routes[0].candidate_id, 1);
        let primary = merged.primary_regular.unwrap();
        assert_eq!(primary.route.candidate_id, 99);
        assert_eq!(primary.route.owner_island_id, 8);
        assert_eq!(primary.route.coverage_face_id, 80);
        assert_eq!(primary.route.component_id, 800);
        assert_eq!(primary.position_mm, position);
        assert_eq!(primary.normal, first_normal);
        assert_eq!(merged.position_mm, primary.position_mm);
        assert_eq!(merged.normal, primary.normal);
    }

    #[test]
    fn zero_gate_caps_refuse_without_payload_allocation_or_work() {
        let mut telemetry = GatePreflightTelemetry::default();
        let mut operation_limits = SpecializedLimits::production(TEST_KNN_LIMITS);
        operation_limits.max_gate_operations = 0;
        assert_eq!(
            gate_header_preflight(
                2,
                1,
                std::mem::size_of::<MergeProposal>(),
                3,
                4,
                5,
                6,
                operation_limits,
                &mut telemetry,
            ),
            Err(SpecializedError::GateOperationLimit {
                required: 1,
                limit: 0,
            })
        );
        assert_eq!(telemetry.payload_allocations, 0);
        assert_eq!(telemetry.payload_operations, 0);

        let mut peak_limits = SpecializedLimits::production(TEST_KNN_LIMITS);
        peak_limits.max_gate_peak_bytes = 0;
        let expected = std::mem::size_of::<MergeProposal>()
            + 2 * std::mem::size_of::<FrozenSpecializedGateVector>();
        assert_eq!(
            gate_header_preflight(
                2,
                1,
                std::mem::size_of::<MergeProposal>(),
                3,
                4,
                5,
                6,
                peak_limits,
                &mut telemetry,
            ),
            Err(SpecializedError::GatePeakByteLimit {
                required: expected,
                limit: 0,
            })
        );
        assert_eq!(telemetry.payload_allocations, 0);
        assert_eq!(telemetry.payload_operations, 0);
    }

    #[test]
    fn huge_owner_face_payload_refuses_during_allocation_free_count() {
        let contacts = vec![WorkingContact::specialized(minimum_demand(
            0,
            2,
            7,
            [0.0, 0.0, 0.1],
        ))];
        let islands = [SupportIsland {
            id: 0,
            face_ids: (0..10_000).collect(),
        }];
        let mut work = 0usize;
        assert_eq!(
            variant_specialized_owner_face_count(&contacts, None, &[], &islands, &mut work, 10,),
            Err(SpecializedError::GateOperationLimit {
                required: 11,
                limit: 10,
            })
        );
    }

    #[test]
    fn reservation_scans_are_exactly_charged_and_zero_cap_does_no_fill_work() {
        let reservations = (0..10_000)
            .map(|owner_id| SpecializedOwnerReservation {
                owner_id,
                face_id: 0,
            })
            .collect::<Vec<_>>();
        assert_eq!(fill_operation_count(17, reservations.len(), 3), Ok(30_017));
        assert_eq!(
            accepted_load_header_preflight(0, 0, 1, 0),
            Err(SpecializedError::Generic(
                GenericPerceptionError::LoadOperationLimit {
                    required: 1,
                    limit: 0,
                }
            ))
        );

        let mut physical = Vec::new();
        let mut owner_faces = Vec::new();
        let mut work = 0usize;
        assert_eq!(
            append_variant_load_inputs(
                &[],
                None,
                &[],
                &[],
                &reservations,
                &mut physical,
                &mut owner_faces,
                &mut work,
                reservations.len() - 1,
            ),
            Err(SpecializedError::GateOperationLimit {
                required: reservations.len(),
                limit: reservations.len() - 1,
            })
        );
        assert!(physical.is_empty());
        assert!(owner_faces.is_empty());

        work = 0;
        append_variant_load_inputs(
            &[],
            None,
            &[],
            &[],
            &reservations,
            &mut physical,
            &mut owner_faces,
            &mut work,
            reservations.len(),
        )
        .unwrap();
        assert_eq!(work, reservations.len());

        work = 0;
        assert_eq!(
            append_variant_load_inputs(
                &[],
                None,
                &[],
                &[],
                &reservations[..1],
                &mut physical,
                &mut owner_faces,
                &mut work,
                0,
            ),
            Err(SpecializedError::GateOperationLimit {
                required: 1,
                limit: 0,
            })
        );
        assert_eq!(work, 0);
    }

    #[test]
    fn combined_gate_peak_accepts_the_cap_and_refuses_cap_plus_one() {
        let exact = GatePeakUsage {
            proposal_bytes: 10,
            result_bytes: 20,
            matrix_bytes: 30,
            coverage_scratch_bytes: 40,
            coverage_nearest_bytes: 0,
            coverage_retained_bytes: 8,
            load_scratch_bytes: 32,
        };
        assert_eq!(check_gate_peak(exact, 100), Ok(100));
        let below = GatePeakUsage {
            load_scratch_bytes: 33,
            ..exact
        };
        assert_eq!(
            check_gate_peak(below, 100),
            Err(SpecializedError::GatePeakByteLimit {
                required: 101,
                limit: 100,
            })
        );
    }

    #[test]
    fn merge_commit_preflights_deep_payload_at_exact_cap_before_clone() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let mut left_demand = minimum_demand(0, 2, 7, [1.0, 2.0, 0.1]);
        left_demand.region_points_mm = vec![[1.0, 2.0, 0.1]; 10_000];
        let contacts = vec![
            WorkingContact::specialized(left_demand),
            WorkingContact::specialized(minimum_demand(1, 9, 7, [1.1, 2.0, 0.1])),
        ];
        let proposals = test_pair_proposals(&contacts, &layers, 0.2, 2.5);
        let proposal = &proposals[0];
        let exact = contacts
            .iter()
            .flat_map(|contact| contact.demands.iter())
            .try_fold(0usize, |total, demand| {
                total
                    .checked_add(demand_deep_bytes(demand)?)
                    .ok_or(SpecializedError::GateResourceOverflow)
            })
            .unwrap();
        let mut refused = MergeAllocationTelemetry::default();
        assert!(matches!(
            commit_merge(proposal, &contacts, exact - 1, &mut refused),
            Err(SpecializedError::MergeCommitByteLimit {
                required: exact,
                limit,
            })
            if limit == exact - 1
        ));
        assert_eq!(refused.deep_clone_count, 0);
        assert_eq!(refused.max_commit_bytes, 0);
        let mut accepted = MergeAllocationTelemetry::default();
        let merged = commit_merge(proposal, &contacts, exact, &mut accepted).unwrap();
        assert_eq!(merged.demands.len(), 2);
        assert_eq!(accepted.deep_clone_count, 1);
        assert_eq!(accepted.max_commit_bytes, exact);
    }

    #[test]
    fn synthetic_only_route_contributes_its_claimed_face_to_load_eligibility() {
        let contacts = vec![test_regular_contact(
            [1.0, 2.0, 0.1],
            [0.0, 0.0, -1.0],
            RegularContactRoute {
                candidate_id: 44,
                owner_island_id: 9,
                coverage_face_id: 3,
                component_id: 7,
                source_kind: RepairRowSourceKind::Generic,
            },
            Vec::new(),
        )];
        let islands = [SupportIsland {
            id: 9,
            face_ids: vec![3],
        }];
        let mut work = 0usize;
        let owner_faces = variant_specialized_owner_face_count(
            &contacts,
            None,
            &[],
            &islands,
            &mut work,
            usize::MAX,
        )
        .unwrap();
        let mut physical = Vec::with_capacity(1);
        let mut specialized_owner_faces = Vec::with_capacity(owner_faces);
        let mut fill_work = 0usize;
        append_variant_load_inputs(
            &contacts,
            None,
            &[],
            &islands,
            &[],
            &mut physical,
            &mut specialized_owner_faces,
            &mut fill_work,
            usize::MAX,
        )
        .unwrap();
        assert_eq!(physical.len(), 1);
        assert_eq!(physical[0].owner, LoadOwner::Specialized(0));
        assert_eq!(specialized_owner_faces, vec![(0, 3)]);
    }

    #[test]
    fn merge_gate_resource_caps_are_exact_and_cap_minus_one_refuses() {
        assert_eq!(check_merge_proposal_limit(1, 1), Ok(()));
        assert_eq!(
            check_merge_proposal_limit(1, 0),
            Err(SpecializedError::MergeProposalLimit {
                required: 1,
                limit: 0,
            })
        );

        let limits = SpecializedLimits::production(TEST_KNN_LIMITS);
        let exact = gate_resource_plan(3, 2, 5, 7, 11, limits).unwrap();
        assert_eq!(exact.variants, 3);
        assert_eq!(exact.operations, 140);

        let mut at_cap = limits;
        at_cap.max_gate_variants = exact.variants;
        at_cap.max_gate_vector_bytes = exact.vector_bytes;
        at_cap.max_gate_contact_matrix_bytes = exact.contact_window_bytes;
        at_cap.max_gate_operations = exact.operations;
        assert_eq!(gate_resource_plan(3, 2, 5, 7, 11, at_cap), Ok(exact));

        let mut below = at_cap;
        below.max_gate_variants -= 1;
        assert_eq!(
            gate_resource_plan(3, 2, 5, 7, 11, below),
            Err(SpecializedError::GateVariantLimit {
                required: exact.variants,
                limit: exact.variants - 1,
            })
        );
        below = at_cap;
        below.max_gate_vector_bytes -= 1;
        assert_eq!(
            gate_resource_plan(3, 2, 5, 7, 11, below),
            Err(SpecializedError::GateVectorByteLimit {
                required: exact.vector_bytes,
                limit: exact.vector_bytes - 1,
            })
        );
        below = at_cap;
        below.max_gate_contact_matrix_bytes -= 1;
        assert_eq!(
            gate_resource_plan(3, 2, 5, 7, 11, below),
            Err(SpecializedError::GateContactMatrixByteLimit {
                required: exact.contact_window_bytes,
                limit: exact.contact_window_bytes - 1,
            })
        );
        below = at_cap;
        below.max_gate_operations -= 1;
        assert_eq!(
            gate_resource_plan(3, 2, 5, 7, 11, below),
            Err(SpecializedError::GateOperationLimit {
                required: exact.operations,
                limit: exact.operations - 1,
            })
        );
        below = at_cap;
        below.max_gate_window_variants = 0;
        assert_eq!(
            gate_resource_plan(3, 2, 5, 7, 11, below),
            Err(SpecializedError::InvalidGateWindow)
        );
    }

    #[test]
    fn owner_route_mismatch_fails_typed() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let mut contacts = vec![WorkingContact::specialized(minimum_demand(
            0,
            3,
            7,
            [1.0, 2.0, 0.1],
        ))];
        assert_eq!(
            canonicalize_and_validate(
                &mut contacts,
                &[SupportIsland {
                    id: 0,
                    face_ids: vec![2],
                }],
                &layers,
                0.2,
            ),
            Err(SpecializedError::OwnerRouteMismatch {
                island_id: 0,
                face_id: 3,
            })
        );
    }

    #[test]
    fn frozen_numeric_order_treats_signed_zero_as_a_numeric_tie() {
        assert_eq!(numeric_cmp(-0.0, 0.0), Ordering::Equal);
        let a = minimum_demand(0, 2, 7, [-0.0, 1.0, 2.0]);
        let b = minimum_demand(0, 3, 7, [0.0, 1.0, 2.0]);
        assert_eq!(compare_demands(&a, &b), Ordering::Less);
    }

    #[test]
    fn demand_key_orders_coincident_birth_before_minimum_by_python_boolean_tuple() {
        let position = [0.0, 0.0, 0.1];
        let minimum = minimum_demand(0, 2, 7, position);
        let birth = SpecializedDemand {
            kind: SpecializedDemandKind::IslandBirth,
            owner_island_id: 0,
            coverage_face_id: 2,
            coverage_component_id: 7,
            nominated_position_mm: position,
            nominated_normal: [0.0, 0.0, -1.0],
            region_points_mm: Vec::new(),
            birth_z_mm: Some(0.1),
            birth_layer_index: Some(2),
            birth_footprint: Some(crate::perception::test_slice_solid(vec![
                [-1.0, -1.0],
                [1.0, -1.0],
                [1.0, 1.0],
                [-1.0, 1.0],
            ])),
        };
        assert_eq!(demand_kind_key(birth.kind), (false, true));
        assert_eq!(demand_kind_key(minimum.kind), (true, false));
        assert_eq!(compare_demands(&birth, &minimum), Ordering::Less);
        let mut demands = [minimum, birth];
        demands.sort_by(compare_demands);
        assert_eq!(demands[0].kind, SpecializedDemandKind::IslandBirth);
    }

    #[test]
    fn demand_array_key_distinguishes_signed_zero_raw_bytes() {
        let position = [0.0, 0.0, 0.1];
        let mut positive = minimum_demand(0, 2, 7, position);
        positive.region_points_mm = vec![[0.0, 1.0, 2.0]];
        let mut negative = positive.clone();
        negative.region_points_mm[0][0] = -0.0;
        assert_eq!(compare_demands(&positive, &negative), Ordering::Less);

        let mut demands = vec![negative, positive];
        demands.sort_by(compare_demands);
        demands.dedup_by(|left, right| compare_demands(left, right) == Ordering::Equal);
        assert_eq!(demands.len(), 2);
        assert_eq!(demands[0].region_points_mm[0][0].to_bits(), 0);
        assert_eq!(demands[1].region_points_mm[0][0].to_bits(), 1_u64 << 63);
    }

    #[test]
    fn demand_array_key_compares_shape_before_identical_flat_bytes() {
        let flat = [1.0, 2.0];
        assert_eq!(
            compare_f64_array_keys(&[1, 2], flat, &[2, 1], flat),
            Ordering::Less
        );
    }

    #[test]
    fn frozen_python_minimum_attach_vector_is_closed_at_point_two_mm() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let demand = minimum_demand(0, 2, 7, [0.0, 0.0, 0.0]);
        assert!(satisfies([0.2, 0.0, 0.0], &demand, &layers, 0.2));
        assert!(!satisfies(
            [f64::from_bits(0.2_f64.to_bits() + 1), 0.0, 0.0],
            &demand,
            &layers,
            0.2,
        ));
    }

    #[test]
    fn already_covered_regular_is_owner_scoped_and_suppresses_a_new_seat() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let regular = test_regular_contact(
            [0.1, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            RegularContactRoute {
                candidate_id: 7,
                owner_island_id: 3,
                coverage_face_id: 2,
                component_id: 7,
                source_kind: RepairRowSourceKind::Generic,
            },
            Vec::new(),
        );
        let matching = minimum_demand(3, 2, 7, [0.0, 0.0, 0.0]);
        let other_owner = minimum_demand(4, 2, 7, [0.0, 0.0, 0.0]);
        let exact_other_owner = minimum_demand(4, 2, 7, [0.1, 0.0, 0.0]);
        let dispositions = regular_dispositions(
            &[regular],
            &[matching, other_owner, exact_other_owner],
            &layers,
            0.2,
            SpecializedLimits::production(TEST_KNN_LIMITS),
        )
        .unwrap();
        assert!(dispositions[0].covered);
        assert_eq!(dispositions[0].exact_regular, None);
        assert!(!dispositions[1].covered);
        assert_eq!(dispositions[2].exact_regular, Some(0));
        assert!(!dispositions[2].covered);
    }

    #[test]
    fn different_owner_exact_prepass_makes_duplicate_invariant_unreachable() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let position = [0.1, 0.0, 0.0];
        let regular_normal = [0.0, 0.0, -1.0];
        let mut demand = minimum_demand(4, 2, 7, position);
        demand.nominated_normal = [1.0, 0.0, 0.0];
        let mut physical = vec![
            test_regular_contact(
                position,
                regular_normal,
                RegularContactRoute {
                    candidate_id: 7,
                    owner_island_id: 3,
                    coverage_face_id: 2,
                    component_id: 7,
                    source_kind: RepairRowSourceKind::Generic,
                },
                Vec::new(),
            ),
            WorkingContact::specialized(demand),
        ];
        let attached = attach_exact_mixed_contacts(
            &mut physical,
            &layers,
            0.2,
            SpecializedLimits::production(TEST_KNN_LIMITS),
            &mut MergeAllocationTelemetry::default(),
        )
        .unwrap();
        assert_eq!(attached, 1);
        canonicalize_and_validate(
            &mut physical,
            &[SupportIsland {
                id: 4,
                face_ids: vec![2],
            }],
            &layers,
            0.2,
        )
        .unwrap();
        assert_eq!(physical.len(), 1);
        assert_eq!(physical[0].normal, regular_normal);
        let primary = physical[0].primary_regular.unwrap();
        assert_eq!(primary.route.candidate_id, 7);
        assert_eq!(primary.route.owner_island_id, 3);
        assert_eq!(primary.route.coverage_face_id, 2);
        assert_eq!(primary.route.component_id, 7);
        assert_eq!(primary.position_mm, position);
        assert_eq!(primary.normal, regular_normal);
        assert_eq!(physical[0].regular_routes[0].owner_island_id, 3);
        assert_eq!(physical[0].demands[0].owner_island_id, 4);
    }

    #[test]
    fn exact_mixed_prepass_replaces_an_old_shared_primary_with_the_pure_regular() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let position = [0.1, 0.0, 0.0];
        let pure_normal = [0.0, 1.0, 0.0];
        let mut physical = vec![
            test_regular_contact(
                position,
                [1.0, 0.0, 0.0],
                RegularContactRoute {
                    candidate_id: 1,
                    owner_island_id: 1,
                    coverage_face_id: 10,
                    component_id: 7,
                    source_kind: RepairRowSourceKind::Generic,
                },
                vec![minimum_demand(1, 10, 7, position)],
            ),
            test_regular_contact(
                position,
                pure_normal,
                RegularContactRoute {
                    candidate_id: 99,
                    owner_island_id: 9,
                    coverage_face_id: 90,
                    component_id: 70,
                    source_kind: RepairRowSourceKind::Generic,
                },
                Vec::new(),
            ),
        ];
        assert_eq!(
            attach_exact_mixed_contacts(
                &mut physical,
                &layers,
                0.2,
                SpecializedLimits::production(TEST_KNN_LIMITS),
                &mut MergeAllocationTelemetry::default(),
            )
            .unwrap(),
            1
        );

        let primary = physical[0].primary_regular.unwrap();
        assert_eq!(primary.route.candidate_id, 99);
        assert_eq!(primary.route.owner_island_id, 9);
        assert_eq!(primary.route.coverage_face_id, 90);
        assert_eq!(primary.route.component_id, 70);
        assert_eq!(primary.position_mm, position);
        assert_eq!(primary.normal, pure_normal);
        assert_eq!(physical[0].position_mm, position);
        assert_eq!(physical[0].normal, pure_normal);
        assert_eq!(physical[0].regular_routes.len(), 2);
    }

    #[test]
    fn inconsistent_primary_regular_identity_fails_typed() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let mut contact = test_regular_contact(
            [0.1, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            RegularContactRoute {
                candidate_id: 7,
                owner_island_id: 3,
                coverage_face_id: 2,
                component_id: 7,
                source_kind: RepairRowSourceKind::Generic,
            },
            Vec::new(),
        );
        contact.regular_routes[0].candidate_id = 8;
        assert_eq!(
            canonicalize_and_validate(&mut [contact], &[], &layers, 0.2),
            Err(SpecializedError::PrimaryRegularIdentityMismatch)
        );
    }

    fn perception_config<'a>(delicacy: &'a [f64]) -> SupportPerceptionConfig<'a> {
        SupportPerceptionConfig::current(
            Finite::new(0.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            64 * 1024 * 1024,
            Some(delicacy),
        )
        .unwrap()
    }

    fn candidate_config() -> CandidatePayloadConfig {
        CandidatePayloadConfig::current(8, 1_000_000, 3_000_000, 256 * 1024 * 1024, 65_536).unwrap()
    }

    fn frame() -> BuildFrame {
        BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap())
    }

    fn full_fixture(mesh: Mesh, name: &str) -> Result<SpecializedContactPlan, String> {
        let delicacy = vec![0.0; mesh.tris().len()];
        let layers = LayerPlan::new(0.0, vec![0.05; 1_000]).unwrap();
        let perception_config = perception_config(&delicacy);
        let candidate_config = candidate_config();
        let generic = crate::perception::prepare_generic_contacts(
            &mesh,
            &perception_config,
            frame(),
            &layers,
            &candidate_config,
            &[],
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
        )
        .map_err(|error| format!("required {name} generic fixture failed: {error}"))?;
        let selection_config = ContactSelectionConfig::current_defaults(
            Some(&delicacy),
            perception_config.max_grid_bytes,
        )
        .map_err(|error| format!("required {name} selection config failed: {error}"))?;
        let regular = if generic.islands().is_empty() {
            SelectedContacts {
                contacts: Vec::new(),
                coverage_samples: Vec::new(),
            }
        } else {
            crate::select_contacts(
                &mesh,
                generic.candidates(),
                generic.islands(),
                &selection_config,
            )
            .map_err(|error| format!("required {name} regular selection failed: {error}"))?
        };
        prepare_specialized_contacts(
            &mesh,
            &perception_config,
            frame(),
            &layers,
            &candidate_config,
            &[],
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
            &generic,
            &regular,
            &selection_config,
            SpecializedLimits::production(TEST_KNN_LIMITS),
        )
        .map_err(|error| format!("required {name} specialized fixture failed: {error}"))
    }

    fn edit_fixture(
        mesh: Mesh,
        name: &str,
    ) -> Result<(Mesh, GenericContactPlan, SpecializedContactPlan), String> {
        let delicacy = vec![0.0; mesh.tris().len()];
        let layers = LayerPlan::new(0.0, vec![0.05; 1_000]).unwrap();
        let perception_config = perception_config(&delicacy);
        let candidate_config = candidate_config();
        let generic = crate::perception::prepare_generic_contacts(
            &mesh,
            &perception_config,
            frame(),
            &layers,
            &candidate_config,
            &[],
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
        )
        .map_err(|error| format!("required {name} generic fixture failed: {error}"))?;
        let selection_config = ContactSelectionConfig::current_defaults(
            Some(&delicacy),
            perception_config.max_grid_bytes,
        )
        .map_err(|error| format!("required {name} selection config failed: {error}"))?;
        let regular = if generic.islands().is_empty() {
            SelectedContacts {
                contacts: Vec::new(),
                coverage_samples: Vec::new(),
            }
        } else {
            crate::select_contacts(
                &mesh,
                generic.candidates(),
                generic.islands(),
                &selection_config,
            )
            .map_err(|error| format!("required {name} regular fixture failed: {error}"))?
        };
        let plan = prepare_specialized_contacts(
            &mesh,
            &perception_config,
            frame(),
            &layers,
            &candidate_config,
            &[],
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
            &generic,
            &regular,
            &selection_config,
            SpecializedLimits::production(TEST_KNN_LIMITS),
        )
        .map_err(|error| format!("required {name} specialized fixture failed: {error}"))?;
        Ok((mesh, generic, plan))
    }

    fn edit_surface_config() -> NativeSurfaceConfig {
        NativeSurfaceConfig {
            grid_pitch_mm: 1.0,
            max_grid_bytes: 64 * 1024 * 1024,
        }
    }

    #[test]
    fn regular_child_edit_preserves_sparse_survivors_and_rebinds_loads() {
        let (_mesh, _generic, plan) = edit_fixture(make_overhang_table().unwrap(), "child edit")
            .unwrap_or_else(|error| panic!("{error}"));
        let removable = plan
            .contacts()
            .iter()
            .find(|contact| contact.demands().is_empty() && contact.primary_regular().is_some())
            .cloned()
            .expect("required child-edit fixture has a demand-free regular contact");
        let survivor = plan
            .contacts()
            .iter()
            .find(|contact| contact.contact_id() != removable.contact_id())
            .cloned()
            .expect("required child-edit fixture has a surviving contact");
        let child = plan
            .begin_regular_edit()
            .unwrap()
            .remove_demand_free_regular(removable.contact_id())
            .unwrap();
        assert!(child
            .contacts()
            .iter()
            .all(|contact| contact.contact_id() != removable.contact_id()));
        let child_survivor = child
            .contacts()
            .iter()
            .find(|contact| contact.contact_id() == survivor.contact_id())
            .expect("surviving stable id must remain present");
        assert_eq!(child_survivor.position_mm(), survivor.position_mm());
        assert_eq!(child_survivor.normal(), survivor.normal());
        assert_eq!(child_survivor.seat_provenance(), survivor.seat_provenance());
        assert_eq!(child_survivor.primary_regular(), survivor.primary_regular());
        assert_eq!(child_survivor.regular_routes(), survivor.regular_routes());
        assert_eq!(child_survivor.demands(), survivor.demands());
        assert_eq!(child.finalized_load_plan_digest(), child.digest());
        assert_ne!(child.digest(), plan.digest());
        let parent_ids = plan
            .contacts()
            .iter()
            .map(SpecializedPhysicalContact::contact_id)
            .collect::<Vec<_>>();
        let parent_loads = crate::perception::FinalizedContactLoads::from_partition(
            plan.load_partition(),
            &parent_ids,
            crate::perception::FinalizedContactLoadLimits::new(
                parent_ids.len(),
                plan.load_partition().rows().len(),
                usize::MAX,
            ),
        )
        .unwrap();
        let child_ids = child
            .contacts()
            .iter()
            .map(SpecializedPhysicalContact::contact_id)
            .collect::<Vec<_>>();
        assert_eq!(
            parent_loads.validate_binding(child.load_partition(), &child_ids),
            Err(GenericPerceptionError::StaleAuthority)
        );

        let second = child
            .contacts()
            .iter()
            .find(|contact| contact.demands().is_empty() && contact.primary_regular().is_some())
            .expect("required child-edit fixture retains a second regular contact")
            .contact_id();
        let grandchild = child
            .begin_regular_edit()
            .unwrap()
            .remove_demand_free_regular(second)
            .unwrap();
        assert!(grandchild
            .contacts()
            .iter()
            .all(|contact| contact.contact_id() != removable.contact_id()
                && contact.contact_id() != second));

        let route_seed = plan
            .contacts()
            .iter()
            .find_map(|contact| contact.regular_routes().first().copied())
            .expect("required child-edit fixture has a regular route");
        let row = grandchild
            .eligible_repair_rows(
                route_seed.owner_island_id(),
                route_seed.component_id(),
                &BTreeSet::new(),
            )
            .unwrap()
            .into_iter()
            .find(|row| {
                grandchild
                    .contacts()
                    .iter()
                    .all(|contact| contact.position_mm() != row.candidate().point_mm)
            })
            .expect("required child-edit fixture has an unoccupied repair row")
            .id();
        let frozen_row = grandchild.repair_row(row).unwrap().clone();
        let frozen_route = RegularContactRoute {
            candidate_id: row.candidate_id.0,
            owner_island_id: row.owner_island_id,
            coverage_face_id: row.face_id,
            component_id: row.component_id,
            source_kind: row.source_kind,
        };
        let frozen_primary = PrimaryRegularIdentity {
            route: frozen_route,
            position_mm: frozen_row.candidate().point_mm,
            normal: frozen_row.candidate().normal,
        };
        let appended = grandchild
            .begin_regular_edit()
            .unwrap()
            .append_regular_repair_row(row)
            .unwrap();
        let appended_id = appended
            .contacts()
            .iter()
            .map(SpecializedPhysicalContact::contact_id)
            .max()
            .expect("appended child has a contact");
        let parent_max_id = plan
            .contacts()
            .iter()
            .map(SpecializedPhysicalContact::contact_id)
            .max()
            .expect("required child-edit fixture has contacts");
        assert!(appended_id > removable.contact_id());
        assert!(appended_id > second);
        assert!(appended_id > parent_max_id);
        assert_eq!(appended.contacts().len(), grandchild.contacts().len() + 1);
        assert_eq!(
            appended.contacts()[..grandchild.contacts().len()]
                .iter()
                .map(SpecializedPhysicalContact::contact_id)
                .collect::<Vec<_>>(),
            grandchild
                .contacts()
                .iter()
                .map(SpecializedPhysicalContact::contact_id)
                .collect::<Vec<_>>(),
            "addition preserves every existing contact's stable placement"
        );
        assert_eq!(
            appended
                .contacts()
                .last()
                .map(SpecializedPhysicalContact::contact_id),
            Some(appended_id),
            "addition appends in stable plan order"
        );
        let appended_contact = appended.contacts().last().unwrap();
        assert_eq!(
            appended_contact.position_mm(),
            frozen_row.candidate().point_mm
        );
        assert_eq!(appended_contact.normal(), frozen_row.candidate().normal);
        assert_eq!(appended_contact.regular_routes(), &[frozen_route]);
        assert_eq!(appended_contact.primary_regular(), Some(frozen_primary));
        assert_eq!(
            appended_contact.seat_provenance(),
            PhysicalSeatProvenance::Regular(frozen_primary)
        );
        assert!(appended_contact.demands().is_empty());
        assert_eq!(appended.finalized_load_plan_digest(), appended.digest());
    }

    #[test]
    fn regular_child_edit_rejects_unknown_occupied_and_stale_row_authority() {
        let (_mesh, _generic, plan) = edit_fixture(make_overhang_table().unwrap(), "row errors")
            .unwrap_or_else(|error| panic!("{error}"));
        let occupied = plan
            .occupied_repair_rows()
            .unwrap()
            .first()
            .expect("required row-error fixture has an occupied row")
            .row_id();
        assert_eq!(
            plan.begin_regular_edit()
                .unwrap()
                .append_regular_repair_row(occupied),
            Err(SpecializedError::OccupiedRepairRow { row_id: occupied })
        );

        let mut unknown = occupied;
        unknown.owner_island_id = u32::MAX;
        assert!(plan.repair_row(unknown).is_none());
        assert_eq!(
            plan.begin_regular_edit()
                .unwrap()
                .append_regular_repair_row(unknown),
            Err(SpecializedError::UnknownRepairRow { row_id: unknown })
        );

        let mut stale = plan.clone();
        stale.repair_pool.rows[0].candidate.point_mm[0] += 1.0;
        assert!(matches!(
            stale.begin_regular_edit(),
            Err(SpecializedError::StaleRepairPool)
        ));
    }

    #[test]
    fn demanded_child_edit_is_a_typed_hard_failure() {
        let (_mesh, _generic, plan) = edit_fixture(make_overhang_table().unwrap(), "demanded edit")
            .unwrap_or_else(|error| panic!("{error}"));
        let demanded = plan
            .contacts()
            .iter()
            .find(|contact| !contact.demands().is_empty())
            .expect("required demanded-edit fixture has a demanded contact")
            .contact_id();
        let result = plan
            .begin_regular_edit()
            .unwrap()
            .remove_demand_free_regular(demanded);
        assert_eq!(
            result,
            Err(SpecializedError::DemandBearingContactEdit {
                contact_id: demanded
            })
        );
    }

    #[test]
    fn child_edit_freezes_parent_finalizer_inputs_and_rejects_tampering() {
        let (_mesh, _generic, plan) = edit_fixture(make_overhang_table().unwrap(), "frozen edit")
            .unwrap_or_else(|error| panic!("{error}"));
        let digest = plan.digest();
        let mut changed_limits = plan.clone();
        changed_limits.limits_authority.max_load_operations += 1;
        assert_ne!(changed_limits.digest(), digest);
        assert!(matches!(
            changed_limits.begin_regular_edit(),
            Err(SpecializedError::StaleFinalizedLoadArtifact)
        ));
        let mut changed_reservations = plan.clone();
        changed_reservations
            .reservations_authority
            .push(SpecializedOwnerReservation {
                owner_id: 9_999,
                face_id: 0,
            });
        assert_ne!(changed_reservations.digest(), digest);
        assert!(matches!(
            changed_reservations.begin_regular_edit(),
            Err(SpecializedError::StaleFinalizedLoadArtifact)
        ));
        let mut changed_surface = plan.clone();
        changed_surface.surface_config_authority.grid_pitch_mm = 2.0;
        assert_ne!(changed_surface.digest(), digest);
        assert!(matches!(
            changed_surface.begin_regular_edit(),
            Err(SpecializedError::StaleFinalizedLoadArtifact)
        ));
        let mut changed_mesh = plan.clone();
        changed_mesh.mesh_authority = make_thin_fin().unwrap();
        assert_ne!(
            mesh_fingerprint(&changed_mesh.mesh_authority).unwrap(),
            changed_mesh.mesh_authority_fingerprint
        );
        // A zero native budget would fail surface construction if the frozen
        // mesh check did not run first.
        changed_mesh.surface_config_authority.max_grid_bytes = 0;
        assert!(matches!(
            changed_mesh.begin_regular_edit(),
            Err(SpecializedError::FrozenMeshAuthorityMismatch)
        ));
        let mut coherently_substituted_mesh = plan.clone();
        coherently_substituted_mesh.mesh_authority = make_thin_fin().unwrap();
        coherently_substituted_mesh.mesh_authority_fingerprint =
            mesh_fingerprint(&coherently_substituted_mesh.mesh_authority).unwrap();
        assert_ne!(coherently_substituted_mesh.digest(), digest);
        assert!(matches!(
            coherently_substituted_mesh.begin_regular_edit(),
            Err(SpecializedError::StaleFinalizedLoadArtifact)
        ));
        // The public edit start has no parameters: these parent-owned values
        // are the only possible finalizer inputs.
        assert!(plan.begin_regular_edit().is_ok());
    }

    #[test]
    fn removing_all_regular_contacts_yields_canonical_zero_load_geometry() {
        let (mesh, generic, source) = edit_fixture(make_overhang_table().unwrap(), "zero child")
            .unwrap_or_else(|error| panic!("{error}"));
        let mut regular_only = source.clone();
        regular_only.contacts = source
            .contacts()
            .iter()
            .filter(|contact| contact.primary_regular().is_some())
            .cloned()
            .map(|mut contact| {
                contact.demands.clear();
                contact
            })
            .collect();
        assert!(
            !regular_only.contacts.is_empty(),
            "required zero-child fixture has regular contacts"
        );
        let inputs = stable_load_inputs(
            &regular_only.contacts,
            &generic,
            &regular_only.islands,
            &[],
            SpecializedLimits::production(TEST_KNN_LIMITS),
        )
        .expect("required zero-child fixture can form a regular load input");
        regular_only.load_partition = finalize_specialized_load_partition(
            &generic,
            &mesh,
            &NativeSurface::from_indexed(mesh.verts(), mesh.tris(), edit_surface_config()).unwrap(),
            &inputs.contacts,
            &inputs.specialized_owner_faces,
            LoadFinalizeLimits {
                max_scratch_bytes: SpecializedLimits::production(TEST_KNN_LIMITS)
                    .max_load_scratch_bytes,
                max_operations: inputs.remaining_load_operations,
            },
        )
        .expect("required zero-child fixture can finalize a regular-only load partition");
        for contact in &mut regular_only.contacts {
            contact.carried_area_mm2 = regular_only
                .load_partition
                .rows()
                .iter()
                .filter(|row| row.contact_id == contact.contact_id)
                .map(|row| row.carried_area_mm2)
                .sum();
        }
        let regular_only_ids = regular_only
            .contacts()
            .iter()
            .map(SpecializedPhysicalContact::contact_id)
            .collect::<Vec<_>>();
        regular_only.finalized_loads = FinalizedContactLoads::for_specialized_plan(
            &regular_only.load_partition,
            &regular_only_ids,
        )
        .expect("required zero-child fixture can tip-size the regular-only partition");
        regular_only = regular_only.seal_finalized_load_digest();
        let ids = regular_only
            .contacts()
            .iter()
            .map(SpecializedPhysicalContact::contact_id)
            .collect::<Vec<_>>();
        let mut child = regular_only;
        for contact_id in ids {
            child = child
                .begin_regular_edit()
                .unwrap()
                .remove_demand_free_regular(contact_id)
                .unwrap();
        }
        assert!(child.contacts().is_empty());
        assert!(child.load_partition().rows().is_empty());
        assert_eq!(child.finalized_load_plan_digest(), child.digest());
    }

    #[test]
    fn finalized_cco_rejects_equal_load_contact_swap_before_mutation() {
        use crate::{
            anchor::{AnchorGeometryAuthority, AnchorSearchConfig, SurfaceAnchorPool},
            cco::{CcoController, CcoError, FinalizedAnchorTerminalRoute, TerminalCcoRequest},
            forest::{
                ContactEnd, Forest, Member, MemberKind, MemberSpec, Owner, RootwardEndRadius,
                TerminalEndRadius,
            },
            perception::{FinalizedContactLoadLimits, FinalizedContactLoads},
        };

        let mesh = make_overhang_table().unwrap();
        let plan = full_fixture(mesh.clone(), "equal-load contact swap").unwrap();
        let contact_ids = plan
            .contacts()
            .iter()
            .map(SpecializedPhysicalContact::contact_id)
            .collect::<Vec<_>>();
        let loads = FinalizedContactLoads::from_partition(
            plan.load_partition(),
            &contact_ids,
            FinalizedContactLoadLimits::new(
                contact_ids.len(),
                plan.load_partition().rows().len(),
                usize::MAX,
            ),
        )
        .unwrap();
        let (bound_contact, rebound_contact) = plan
            .contacts()
            .iter()
            .enumerate()
            .flat_map(|(left_index, left)| {
                plan.contacts()[left_index + 1..]
                    .iter()
                    .map(move |right| (left, right))
            })
            .find(|(left, right)| {
                let left_load = loads.leaf_for(left.contact_id()).unwrap();
                let right_load = loads.leaf_for(right.contact_id()).unwrap();
                left.contact_id() != right.contact_id()
                    && left_load.carried_area_mm2().to_bits()
                        == right_load.carried_area_mm2().to_bits()
                    && left_load.tip_radius_mm() == right_load.tip_radius_mm()
            })
            .expect("required fixture must contain two distinct equal-load contacts");
        let bound_load = loads.leaf_for(bound_contact.contact_id()).unwrap();

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
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let delicacy = vec![0.0; mesh.tris().len()];
        let pool = SurfaceAnchorPool::from_geometry(&geometry, &delicacy).unwrap();

        let terminal_id = u32::try_from(bound_contact.contact_id()).unwrap();
        let p0_mm = bound_contact
            .position_mm()
            .map(|value| Finite::new(value).unwrap());
        let mut p1_mm = p0_mm;
        p1_mm[2] = Finite::new(p1_mm[2].get() - 1.0).unwrap();
        let mut forest = Forest::new();
        let stub = forest.add_member(Member::new(MemberSpec {
            p0_mm,
            p1_mm,
            r0_mm: TerminalEndRadius::new(bound_load.tip_radius_mm()),
            r1_mm: RootwardEndRadius::new(bound_load.tip_radius_mm()),
            kind: MemberKind::Stub,
            owner: Owner::Contact(terminal_id),
            model_contact_end: Some(ContactEnd::Terminalward),
            carried_mm2: Finite::new(bound_load.carried_area_mm2()).unwrap(),
        }));
        forest.register_terminal(terminal_id, stub).unwrap();
        let request = TerminalCcoRequest::bind(&forest, terminal_id, frame()).unwrap();
        let before = forest.digest();
        let result = CcoController::default().grow_terminal_with_finalized_anchor(
            &mut forest,
            &request,
            FinalizedAnchorTerminalRoute {
                plan: &plan,
                contact: rebound_contact,
                contact_id: rebound_contact.contact_id(),
                loads: &loads,
                pool: &pool,
                config: AnchorSearchConfig::current().unwrap(),
                geometry: &geometry,
            },
        );
        assert!(matches!(result, Err(CcoError::FinalizedContactMismatch)));
        assert_eq!(forest.digest(), before);
    }

    fn oriented_corpus_mesh(mesh: Mesh, baseline: &str) -> Mesh {
        let budget = plinth_decimate::DecimationBudget::ratified_default(
            plinth_decimate::ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0),
        )
        .expect("ratified decimation budget");
        let decimated = plinth_decimate::run(&mesh, budget)
            .expect("decimate procedural corpus model")
            .into_decimated_model()
            .expect("print-scale procedural corpus model");
        let oriented = plinth_orient::run(
            decimated,
            &plinth_orient::OrientRequest {
                pinned: plinth_orient::PinnedBaseline {
                    pipeline_report: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("../../corpus/baseline")
                        .join(format!("{baseline}.pipeline.json")),
                },
            },
        )
        .expect("orient from the checked-in corpus baseline");
        oriented.into_parts().0
    }

    /// `WorkingContact` is deliberately not `PartialEq` (demands carry
    /// footprints), so structural comparison goes through the canonical
    /// ordering plus the finalized provenance.
    fn same_working_contacts(left: &[WorkingContact], right: &[WorkingContact]) -> bool {
        left.len() == right.len()
            && left.iter().zip(right).all(|(left, right)| {
                compare_contacts(left, right) == Ordering::Equal
                    && left.seat_provenance == right.seat_provenance
            })
    }

    /// The two routes of the observed raking-blade collision: one generic and
    /// one synthetic-island route that nominate the bit-identical dyadic cell
    /// vertex on an edge shared by two triangles.
    fn coincident_collision_routes() -> (RegularContactRoute, RegularContactRoute) {
        (
            RegularContactRoute {
                candidate_id: 156,
                owner_island_id: 4,
                coverage_face_id: 23,
                component_id: 0,
                source_kind: RepairRowSourceKind::SyntheticIsland,
            },
            RegularContactRoute {
                candidate_id: 161,
                owner_island_id: 1,
                coverage_face_id: 22,
                component_id: 0,
                source_kind: RepairRowSourceKind::Generic,
            },
        )
    }

    #[test]
    fn terminal_coalescing_unions_exactly_coincident_demand_free_regular_seats() {
        let (elected_route, other_route) = coincident_collision_routes();
        assert_eq!(elected_route.cmp(&other_route), Ordering::Less);
        let position = [
            0.001_307_336_147_874_593_7,
            -1.873_496_965_216_322_3,
            5.487_058_875_988_102,
        ];
        // The observed pair's normals differ by exactly one ULP, so the elected
        // seat's normal is observable in the union's output.
        let elected_normal: [f64; 3] = [0.5, 0.0, -0.866_025_403_784_438_7];
        let other_normal: [f64; 3] = [
            0.5,
            0.0,
            f64::from_bits((-0.866_025_403_784_438_7_f64).to_bits() - 1),
        ];
        assert_ne!(elected_normal[2].to_bits(), other_normal[2].to_bits());

        // The elected seat is deliberately *second* in the working vector: the
        // election is by identity, never by position in the vector.
        let mut contacts = vec![
            test_regular_contact(position, other_normal, other_route, Vec::new()),
            test_regular_contact(position, elected_normal, elected_route, Vec::new()),
        ];
        let mut telemetry = MergeAllocationTelemetry::default();
        let coalesced = coalesce_exact_regular_seats(
            &mut contacts,
            SpecializedLimits::production(TEST_KNN_LIMITS),
            &mut telemetry,
        )
        .expect("coalesce the exactly coincident demand-free regular pair");

        assert_eq!(coalesced, 1);
        assert_eq!(contacts.len(), 1);
        let seat = &contacts[0];
        assert_eq!(seat.regular_routes, vec![elected_route, other_route]);
        assert_eq!(
            seat.primary_regular.map(|primary| primary.route),
            Some(elected_route)
        );
        assert_eq!(
            seat.normal.map(f64::to_bits),
            elected_normal.map(f64::to_bits)
        );
        assert_eq!(
            seat.position_mm.map(f64::to_bits),
            position.map(f64::to_bits)
        );
        assert_eq!(
            seat.seat_provenance,
            PhysicalSeatProvenance::Regular(
                seat.primary_regular
                    .expect("coalesced seat keeps a primary")
            )
        );
        assert!(seat.demands.is_empty());
        assert_eq!(telemetry.deep_clone_count, 1);

        // Idempotent: a second pass over the canonical vector finds nothing.
        let before = contacts.clone();
        let again = coalesce_exact_regular_seats(
            &mut contacts,
            SpecializedLimits::production(TEST_KNN_LIMITS),
            &mut telemetry,
        )
        .expect("second coalescing pass");
        assert_eq!(again, 0);
        assert!(same_working_contacts(&contacts, &before));
    }

    #[test]
    fn terminal_coalescing_election_is_independent_of_input_order() {
        let (elected_route, other_route) = coincident_collision_routes();
        let position = [3.0, -4.0, 5.5];
        let elected_normal = [0.0, 0.0, -1.0];
        let other_normal = [0.0, f64::from_bits(1), -1.0];
        let coalesce_once = |ordered: Vec<WorkingContact>| {
            let mut contacts = ordered;
            let mut telemetry = MergeAllocationTelemetry::default();
            let count = coalesce_exact_regular_seats(
                &mut contacts,
                SpecializedLimits::production(TEST_KNN_LIMITS),
                &mut telemetry,
            )
            .expect("coalesce");
            (count, contacts)
        };
        let forward = coalesce_once(vec![
            test_regular_contact(position, elected_normal, elected_route, Vec::new()),
            test_regular_contact(position, other_normal, other_route, Vec::new()),
        ]);
        let reversed = coalesce_once(vec![
            test_regular_contact(position, other_normal, other_route, Vec::new()),
            test_regular_contact(position, elected_normal, elected_route, Vec::new()),
        ]);
        assert_eq!(forward.0, 1);
        assert_eq!(reversed.0, 1);
        assert!(same_working_contacts(&forward.1, &reversed.1));
        assert_eq!(
            forward.1[0].primary_regular.map(|primary| primary.route),
            Some(elected_route)
        );
    }

    #[test]
    fn terminal_coalescing_leaves_demanded_and_separated_seats_alone() {
        let (first_route, second_route) = coincident_collision_routes();
        let position = [1.0, 2.0, 3.0];
        let normal = [0.0, 0.0, -1.0];
        let demand = minimum_demand(0, 0, 0, position);
        let mut contacts = vec![
            // Coincident, but one side carries a demand: that is the mixed
            // attach prepass's business, never this canonicalization's.
            test_regular_contact(position, normal, first_route, vec![demand]),
            test_regular_contact(position, normal, second_route, Vec::new()),
            // Demand-free regular, but a different physical point.
            test_regular_contact([1.0, 2.0, 3.5], normal, second_route, Vec::new()),
        ];
        let before = contacts.clone();
        let mut telemetry = MergeAllocationTelemetry::default();
        let coalesced = coalesce_exact_regular_seats(
            &mut contacts,
            SpecializedLimits::production(TEST_KNN_LIMITS),
            &mut telemetry,
        )
        .expect("coalesce");
        assert_eq!(coalesced, 0);
        assert!(same_working_contacts(&contacts, &before));
        assert_eq!(telemetry.deep_clone_count, 0);
    }

    /// Both models nominate one dyadic cell vertex from two islands under
    /// their own ratified corpus orientation, which used to fail closed with
    /// `DuplicatePhysicalSeat` before consolidation could finish.
    #[test]
    fn oriented_toed_slab_and_raking_blade_finalize_after_terminal_seat_coalescing() {
        let mut failures = Vec::new();
        for (name, baseline, mesh, expected_digest, expected_counts, expected_shared_seats) in [
            (
                "toed slab",
                "make_toed_slab",
                make_toed_slab().unwrap(),
                18_042_644_323_102_122_916_u64,
                (72, 50_688, 9),
                4_usize,
            ),
            (
                "raking blade",
                "make_raking_blade",
                make_raking_blade().unwrap(),
                208_221_774_708_787_011_u64,
                (40, 35_008, 11),
                3,
            ),
        ] {
            let plan = full_fixture(oriented_corpus_mesh(mesh, baseline), name)
                .unwrap_or_else(|error| panic!("{error}"));
            let gate = plan.gate_evidence();
            let actual_counts = (
                plan.contacts().len(),
                plan.coverage_samples().len(),
                plan.load_partition().rows().len(),
            );
            let positions = plan
                .contacts()
                .iter()
                .map(|contact| contact.position_mm().map(f64::to_bits))
                .collect::<BTreeSet<_>>();
            let shared_seats = plan
                .contacts()
                .iter()
                .filter(|contact| contact.regular_routes().len() > 1)
                .count();
            if plan.digest() != expected_digest
                || actual_counts != expected_counts
                || gate.coalesced_seat_count != 1
                || gate.baseline != gate.accepted
                || positions.len() != plan.contacts().len()
                || shared_seats != expected_shared_seats
            {
                failures.push(format!(
                    "{name}: digest actual={} expected={expected_digest}; counts actual={actual_counts:?} expected={expected_counts:?}; coalesced={} accepted_pairs={} non_worsening={}; distinct_positions={} of {}; shared_seats actual={shared_seats} expected={expected_shared_seats}",
                    plan.digest(),
                    gate.coalesced_seat_count,
                    gate.accepted_pair_count,
                    gate.baseline == gate.accepted,
                    positions.len(),
                    plan.contacts().len(),
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn required_table_raking_and_thin_fin_fixtures_finalize_full_evidence() {
        let mut failures = Vec::new();
        for (name, mesh, expected_digest, expected_counts, expected_gate) in [
            (
                "overhang table",
                make_overhang_table().unwrap(),
                // Updated for exact two-stage Python parity: the owner-independent
                // mixed-class attach prepass precedes specialized-only gating, and
                // the digest binds finalized physical-seat provenance and
                // append-only child-allocation history explicitly.
                // This is not a corpus, baseline, physics, or reference re-freeze.
                8_928_188_536_911_449_669_u64,
                (95, 51_200, 14),
                (6, 0, 4, 0),
            ),
            (
                // Added with the terminal exactly-coincident regular seat
                // coalescing pass. In this build frame the toed slab nominates
                // no coincident pair, so its evidence is a fresh freeze rather
                // than a regeneration of an older one.
                "toed slab",
                make_toed_slab().unwrap(),
                11_859_964_443_161_510_103_u64,
                (73, 50_176, 7),
                (3, 0, 3, 0),
            ),
            (
                "raking blade",
                make_raking_blade().unwrap(),
                // Updated for exact two-stage Python parity: the owner-independent
                // mixed-class attach prepass precedes specialized-only gating, and
                // the digest binds finalized physical-seat provenance and
                // append-only child-allocation history explicitly.
                // This is not a corpus, baseline, physics, or reference re-freeze.
                12_073_756_428_966_718_576_u64,
                (40, 40_192, 13),
                (3, 10, 8, 0),
            ),
            (
                "thin fin",
                make_thin_fin().unwrap(),
                // Updated for exact two-stage Python parity: the owner-independent
                // mixed-class attach prepass precedes specialized-only gating, and
                // the digest binds finalized physical-seat provenance and
                // append-only child-allocation history explicitly.
                // This is not a corpus, baseline, physics, or reference re-freeze.
                16_790_786_111_947_059_800_u64,
                (19, 20_480, 2),
                (2, 0, 1, 0),
            ),
        ] {
            let plan = full_fixture(mesh, name).unwrap_or_else(|error| panic!("{error}"));
            let actual_digest = plan.digest();
            for contact in plan.contacts() {
                match contact.seat_provenance() {
                    PhysicalSeatProvenance::Regular(primary) => {
                        assert_eq!(contact.primary_regular(), Some(primary));
                    }
                    PhysicalSeatProvenance::Specialized(representative) => {
                        assert_eq!(contact.primary_regular(), None);
                        assert!(contact.regular_routes().is_empty());
                        assert_eq!(
                            contact.position_mm().map(f64::to_bits),
                            representative.nominated_position_mm().map(f64::to_bits)
                        );
                        assert_eq!(
                            contact.normal().map(f64::to_bits),
                            representative.nominated_normal().map(f64::to_bits)
                        );
                        assert!(contact.demands().iter().any(|demand| {
                            demand.kind() == representative.kind()
                                && demand.owner_island_id() == representative.owner_island_id()
                                && demand.coverage_face_id() == representative.coverage_face_id()
                                && demand.coverage_component_id()
                                    == representative.coverage_component_id()
                                && demand.nominated_position_mm().map(f64::to_bits)
                                    == representative.nominated_position_mm().map(f64::to_bits)
                        }));
                    }
                }
            }
            let actual_counts = (
                plan.contacts().len(),
                plan.coverage_samples().len(),
                plan.load_partition().rows().len(),
            );
            let gate = plan.gate_evidence();
            let actual_gate = (
                gate.accepted.load_violation_count,
                gate.evaluated_pair_count,
                gate.accepted_pair_count,
                gate.coalesced_seat_count,
            );
            let carried_faces = plan
                .load_partition()
                .rows()
                .iter()
                .map(|row| row.face_id)
                .collect::<BTreeSet<_>>();
            let unique_faces = carried_faces.len() == plan.load_partition().rows().len();
            let thin_fin_coverage = name != "thin fin" || !plan.coverage_samples().is_empty();
            if actual_digest != expected_digest
                || actual_counts != expected_counts
                || gate.baseline != gate.accepted
                || actual_gate != expected_gate
                || !unique_faces
                || !thin_fin_coverage
            {
                failures.push(format!(
                    "{name}: digest actual={actual_digest} expected={expected_digest}; counts actual={actual_counts:?} expected={expected_counts:?}; gate actual={actual_gate:?} expected={expected_gate:?}; non_worsening={}; unique_faces={unique_faces}; thin_fin_coverage={thin_fin_coverage}",
                    gate.baseline == gate.accepted,
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn specialized_binding_rejects_plate_selection_and_limit_staleness() {
        let mesh = make_overhang_table().unwrap();
        let delicacy = vec![0.0; mesh.tris().len()];
        let layers = LayerPlan::new(0.0, vec![0.05; 1_000]).unwrap();
        let perception = perception_config(&delicacy);
        let candidate = candidate_config();
        let perception_limits = GenericPerceptionLimits::production(TEST_KNN_LIMITS);
        let generic = crate::perception::prepare_generic_contacts(
            &mesh,
            &perception,
            frame(),
            &layers,
            &candidate,
            &[],
            perception_limits,
        )
        .unwrap();
        let selection =
            ContactSelectionConfig::current_defaults(Some(&delicacy), perception.max_grid_bytes)
                .unwrap();
        let regular =
            crate::select_contacts(&mesh, generic.candidates(), generic.islands(), &selection)
                .unwrap();
        let limits = SpecializedLimits::production(TEST_KNN_LIMITS);
        let plan = prepare_specialized_contacts(
            &mesh,
            &perception,
            frame(),
            &layers,
            &candidate,
            &[],
            perception_limits,
            &generic,
            &regular,
            &selection,
            limits,
        )
        .unwrap();
        assert_eq!(
            plan.source_authority_digest, 9_893_034_867_712_234_337_u64,
            "bound resource authority"
        );
        assert_eq!(
            plan.validate_binding(
                &mesh,
                &perception,
                frame(),
                &layers,
                &candidate,
                &[],
                perception_limits,
                &generic,
                &regular,
                &selection,
                limits,
            ),
            Ok(())
        );

        let mut tampered_regular = regular.clone();
        tampered_regular.contacts[0].point_mm[0] += 0.01;
        assert_eq!(
            plan.validate_binding(
                &mesh,
                &perception,
                frame(),
                &layers,
                &candidate,
                &[],
                perception_limits,
                &generic,
                &tampered_regular,
                &selection,
                limits,
            ),
            Err(SpecializedError::StaleRegularContacts)
        );

        let wrong_plate = SupportPerceptionConfig::current(
            Finite::new(0.1).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            perception.max_grid_bytes,
            Some(&delicacy),
        )
        .unwrap();
        assert_eq!(
            plan.validate_binding(
                &mesh,
                &wrong_plate,
                frame(),
                &layers,
                &candidate,
                &[],
                perception_limits,
                &generic,
                &regular,
                &selection,
                limits,
            ),
            Err(SpecializedError::Generic(
                GenericPerceptionError::FrameMismatch
            ))
        );

        let changed_delicacy = vec![0.25; mesh.tris().len()];
        let changed_selection = ContactSelectionConfig::current_defaults(
            Some(&changed_delicacy),
            perception.max_grid_bytes,
        )
        .unwrap();
        assert_eq!(
            plan.validate_binding(
                &mesh,
                &perception,
                frame(),
                &layers,
                &candidate,
                &[],
                perception_limits,
                &generic,
                &regular,
                &changed_selection,
                limits,
            ),
            Err(SpecializedError::Selection(
                ContactSelectionError::CandidateDelicacyMismatch {
                    candidate_id: crate::ContactCandidateId(0),
                }
            ))
        );

        let mut zero_grid = selection;
        zero_grid.surface_max_grid_bytes = 0;
        assert_eq!(
            plan.validate_binding(
                &mesh,
                &perception,
                frame(),
                &layers,
                &candidate,
                &[],
                perception_limits,
                &generic,
                &regular,
                &zero_grid,
                limits,
            ),
            Err(SpecializedError::Selection(
                ContactSelectionError::InvalidResourceRequest
            ))
        );

        let mut changed_limits = limits;
        changed_limits.max_merge_pairs += 1;
        assert_eq!(
            plan.validate_binding(
                &mesh,
                &perception,
                frame(),
                &layers,
                &candidate,
                &[],
                perception_limits,
                &generic,
                &regular,
                &selection,
                changed_limits,
            ),
            Err(SpecializedError::StaleAuthority)
        );

        let substituted_reservations = [SpecializedOwnerReservation {
            owner_id: 99,
            face_id: 0,
        }];
        assert_eq!(
            plan.validate_binding(
                &mesh,
                &perception,
                frame(),
                &layers,
                &candidate,
                &substituted_reservations,
                perception_limits,
                &generic,
                &regular,
                &selection,
                limits,
            ),
            Err(SpecializedError::StaleAuthority)
        );
        let substituted_surface = SupportPerceptionConfig::current(
            Finite::new(0.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            PositiveFinite::new(2.0).unwrap(),
            perception.max_grid_bytes,
            Some(&delicacy),
        )
        .unwrap();
        assert_eq!(
            plan.validate_binding(
                &mesh,
                &substituted_surface,
                frame(),
                &layers,
                &candidate,
                &[],
                perception_limits,
                &generic,
                &regular,
                &selection,
                limits,
            ),
            Err(SpecializedError::StaleAuthority)
        );

        let mut new_limit_mutations = Vec::new();
        let mut changed = limits;
        changed.max_merge_proposal_bytes += 1;
        new_limit_mutations.push(changed);
        changed = limits;
        changed.max_merge_descriptor_operations += 1;
        new_limit_mutations.push(changed);
        changed = limits;
        changed.max_coverage_scratch_bytes += 1;
        new_limit_mutations.push(changed);
        changed = limits;
        changed.max_coverage_operations += 1;
        new_limit_mutations.push(changed);
        changed = limits;
        changed.max_load_scratch_bytes += 1;
        new_limit_mutations.push(changed);
        changed = limits;
        changed.max_load_operations += 1;
        new_limit_mutations.push(changed);
        changed = limits;
        changed.max_gate_peak_bytes += 1;
        new_limit_mutations.push(changed);
        for changed in new_limit_mutations {
            assert_eq!(
                plan.validate_binding(
                    &mesh,
                    &perception,
                    frame(),
                    &layers,
                    &candidate,
                    &[],
                    perception_limits,
                    &generic,
                    &regular,
                    &selection,
                    changed,
                ),
                Err(SpecializedError::StaleAuthority)
            );
        }
    }

    const SWEEP_CHILD: &str = "PLINTH_SPECIALIZED_SWEEP_CHILD";
    const SWEEP_LINE: &str = "PLINTH_SPECIALIZED_SWEEP=";

    #[test]
    fn specialized_thread_sweep_child() {
        if std::env::var_os(SWEEP_CHILD).is_none() {
            return;
        }
        let plans = [
            full_fixture(make_overhang_table().unwrap(), "overhang table").unwrap(),
            full_fixture(make_raking_blade().unwrap(), "raking blade").unwrap(),
            full_fixture(make_thin_fin().unwrap(), "thin fin").unwrap(),
        ];
        let (effective, participating) = plans
            .iter()
            .map(SpecializedContactPlan::generic_slice_thread_evidence)
            .max()
            .unwrap();
        assert!(effective >= 1);
        assert!(participating >= 1);
        let requested = std::env::var("PLINTH_THREADS")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        if requested > 1 {
            assert!(
                effective > 1 && participating > 1,
                "required fixture corpus must exercise more than one worker at {requested} threads"
            );
        }
        println!(
            "{SWEEP_LINE}{}",
            plans
                .iter()
                .map(|plan| format!(
                    "{}:{}:{}:{}",
                    plan.digest(),
                    plan.contacts().len(),
                    plan.coverage_samples().len(),
                    plan.load_partition().rows().len()
                ))
                .collect::<Vec<_>>()
                .join("|")
        );
    }

    #[test]
    fn full_specialized_evidence_is_identical_at_one_four_and_sixteen_threads() {
        if std::env::var_os(SWEEP_CHILD).is_some() {
            return;
        }
        let executable = std::env::current_exe().expect("current plinth-support test binary");
        let signatures = [1, 4, 16]
            .into_iter()
            .map(|threads| {
                let output = std::process::Command::new(&executable)
                    .arg("--exact")
                    .arg("specialized::tests::specialized_thread_sweep_child")
                    .arg("--nocapture")
                    .env(SWEEP_CHILD, "1")
                    .env("PLINTH_THREADS", threads.to_string())
                    .output()
                    .expect("specialized thread-sweep child must start");
                assert!(
                    output.status.success(),
                    "specialized {threads}-thread child failed:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let stdout = String::from_utf8(output.stdout).expect("test output is UTF-8");
                stdout
                    .split_once(SWEEP_LINE)
                    .and_then(|(_, signature)| signature.lines().next())
                    .map(str::to_owned)
                    .expect("specialized child must emit its evidence signature")
            })
            .collect::<Vec<_>>();
        assert_eq!(signatures[0], signatures[1]);
        assert_eq!(signatures[0], signatures[2]);
    }

    const CHILD_EDIT_SWEEP_CHILD: &str = "PLINTH_SPECIALIZED_CHILD_EDIT_SWEEP_CHILD";
    const CHILD_EDIT_SWEEP_LINE: &str = "PLINTH_SPECIALIZED_CHILD_EDIT_SWEEP=";

    #[test]
    fn regular_child_edit_thread_sweep_child() {
        if std::env::var_os(CHILD_EDIT_SWEEP_CHILD).is_none() {
            return;
        }
        let (_mesh, _generic, plan) = edit_fixture(make_overhang_table().unwrap(), "edit sweep")
            .unwrap_or_else(|error| panic!("{error}"));
        let removable = plan
            .contacts()
            .iter()
            .find(|contact| contact.demands().is_empty() && contact.primary_regular().is_some())
            .expect("required edit-sweep fixture has a demand-free regular contact")
            .contact_id();
        let child = plan
            .begin_regular_edit()
            .unwrap()
            .remove_demand_free_regular(removable)
            .unwrap();
        println!(
            "{CHILD_EDIT_SWEEP_LINE}{}:{}:{}",
            child.digest(),
            child.finalized_load_plan_digest(),
            child.load_partition().rows().len(),
        );
    }

    #[test]
    fn regular_child_edit_is_identical_at_one_four_and_sixteen_threads() {
        if std::env::var_os(CHILD_EDIT_SWEEP_CHILD).is_some() {
            return;
        }
        let executable = std::env::current_exe().expect("current plinth-support test binary");
        let signatures = [1, 4, 16]
            .into_iter()
            .map(|threads| {
                let output = std::process::Command::new(&executable)
                    .arg("--exact")
                    .arg("specialized::tests::regular_child_edit_thread_sweep_child")
                    .arg("--nocapture")
                    .env(CHILD_EDIT_SWEEP_CHILD, "1")
                    .env("PLINTH_THREADS", threads.to_string())
                    .output()
                    .expect("child edit thread-sweep child must start");
                assert!(
                    output.status.success(),
                    "child edit {threads}-thread child failed:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                String::from_utf8(output.stdout)
                    .expect("child edit output is UTF-8")
                    .split_once(CHILD_EDIT_SWEEP_LINE)
                    .and_then(|(_, signature)| signature.lines().next())
                    .map(str::to_owned)
                    .expect("child edit sweep must emit a signature")
            })
            .collect::<Vec<_>>();
        assert_eq!(signatures[0], signatures[1]);
        assert_eq!(signatures[0], signatures[2]);
    }
}
