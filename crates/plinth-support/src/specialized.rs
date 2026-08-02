//! Bound, batch-first consolidation of current minimum and island-birth seats.
//!
//! Generic perception owns every geometric observation.  This module accepts
//! only its opaque plan plus an opaque regular-contact selection, validates
//! both against the complete authority matrix, and returns an opaque physical
//! contact plan.  Logical obligations remain distinct from physical seats.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use geom_kernel::{NativeSurface, NativeSurfaceConfig, PointCloudIndex, RadiusQueryLimits};
use plinth_core::{m0_constants, LayerPlan};
use plinth_mesh::{build_face_topology, Mesh, WeldPolicy};
use thiserror::Error;

use crate::dyadic::{build_candidate_payload, CandidatePayloadConfig, DyadicError};
use crate::perception::{
    finalize_specialized_load_partition, load_violation_count, FinalizedLoadPartition,
    GenericContactPlan, GenericPerceptionError, GenericPerceptionLimits, LoadOwner,
    PhysicalContact, SliceSolid, SpecializedOwnerReservation,
};
use crate::{
    assess_coverage, build_dyadic_coverage_samples, BuildFrame, ContactSelectionConfig,
    ContactSelectionError, CoverageObjective, CoverageSample, SelectedContact, SelectedContacts,
    SupportIsland, SupportPerceptionConfig,
};

const SYNTHETIC_PATCH_RADIUS_MM: f64 = 1.5;
const BIRTH_LINK_RADIUS_MM: f64 = 0.6;

/// Result-inert refusal limits for specialized orchestration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpecializedLimits {
    pub max_synthetic_neighbor_pairs: usize,
    pub max_synthetic_neighbor_bytes: usize,
    pub max_satisfaction_tests: usize,
    pub max_satisfaction_neighbor_pairs: usize,
    pub max_satisfaction_neighbor_bytes: usize,
    pub max_merge_pairs: usize,
    pub max_merge_proposals: usize,
    pub max_merge_total_proposals: usize,
    pub max_merge_iterations: usize,
    pub max_merge_commit_bytes: usize,
    pub max_gate_variants: usize,
    pub max_gate_vector_bytes: usize,
    pub max_gate_contact_matrix_bytes: usize,
    pub max_gate_operations: usize,
    pub max_gate_window_variants: usize,
}

impl SpecializedLimits {
    #[must_use]
    pub const fn production() -> Self {
        Self {
            max_synthetic_neighbor_pairs: 20_000_000,
            max_synthetic_neighbor_bytes: 512 * 1024 * 1024,
            max_satisfaction_tests: 100_000_000,
            max_satisfaction_neighbor_pairs: 100_000_000,
            max_satisfaction_neighbor_bytes: 2 * 1024 * 1024 * 1024,
            max_merge_pairs: 20_000_000,
            max_merge_proposals: 20_000_000,
            max_merge_total_proposals: 100_000_000,
            max_merge_iterations: 100_000,
            max_merge_commit_bytes: 512 * 1024 * 1024,
            max_gate_variants: 20_000_001,
            max_gate_vector_bytes: 512 * 1024 * 1024,
            max_gate_contact_matrix_bytes: 512 * 1024 * 1024,
            max_gate_operations: 2_000_000_000,
            max_gate_window_variants: 1_024,
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
    pub const fn nominated_position_mm(&self) -> [f64; 3] {
        self.nominated_position_mm
    }
}

/// One physical seat, possibly carrying several logical obligations.
#[derive(Clone, Debug, PartialEq)]
pub struct SpecializedPhysicalContact {
    contact_id: u64,
    position_mm: [f64; 3],
    normal: [f64; 3],
    regular_candidate_id: Option<u64>,
    regular_owner_island_id: Option<u32>,
    regular_routes: Vec<RegularContactRoute>,
    demands: Vec<SpecializedDemand>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RegularContactRoute {
    candidate_id: u64,
    owner_island_id: u32,
    coverage_face_id: u32,
    component_id: u32,
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
    pub fn demands(&self) -> &[SpecializedDemand] {
        &self.demands
    }
    #[must_use]
    pub fn regular_routes(&self) -> &[RegularContactRoute] {
        &self.regular_routes
    }
    #[must_use]
    pub const fn is_shared_regular(&self) -> bool {
        !self.regular_routes.is_empty() && !self.demands.is_empty()
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
}

/// Opaque output bound to generic-plan, regular-contact, mesh and layer
/// authority.  It includes the one finalized unique-face load partition.
#[derive(Clone, Debug, PartialEq)]
pub struct SpecializedContactPlan {
    authority_digest: u64,
    generic_slice_thread_evidence: (usize, usize),
    islands: Vec<SupportIsland>,
    coverage_samples: Vec<CoverageSample>,
    contacts: Vec<SpecializedPhysicalContact>,
    load_partition: FinalizedLoadPartition,
    gate_evidence: SpecializedGateEvidence,
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
    #[must_use]
    pub fn contacts(&self) -> &[SpecializedPhysicalContact] {
        &self.contacts
    }
    #[must_use]
    pub const fn load_partition(&self) -> &FinalizedLoadPartition {
        &self.load_partition
    }
    #[must_use]
    pub const fn gate_evidence(&self) -> SpecializedGateEvidence {
        self.gate_evidence
    }
    #[must_use]
    pub const fn generic_slice_thread_evidence(&self) -> (usize, usize) {
        self.generic_slice_thread_evidence
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
        if bound_authority_digest(generic, regular, selection_config, limits)
            != self.authority_digest
        {
            return Err(SpecializedError::StaleAuthority);
        }
        Ok(())
    }
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut hash = Fnv::new();
        hash.u64(self.authority_digest);
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
            hash.u64(contact.regular_candidate_id.unwrap_or(u64::MAX));
            hash.u32(contact.regular_owner_island_id.unwrap_or(u32::MAX));
            for route in &contact.regular_routes {
                hash.u64(route.candidate_id);
                hash.u32(route.owner_island_id);
                hash.u32(route.coverage_face_id);
                hash.u32(route.component_id);
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
    #[error("specialized contact authority is stale or belongs to another input")]
    StaleAuthority,
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
    #[error("specialized gate requires {required} operations; limit is {limit}")]
    GateOperationLimit { required: usize, limit: usize },
    #[error("specialized gate window size must be positive")]
    InvalidGateWindow,
    #[error("specialized gate resource arithmetic overflowed")]
    GateResourceOverflow,
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
    let surface = NativeSurface::from_indexed(
        mesh.verts(),
        mesh.tris(),
        NativeSurfaceConfig {
            grid_pitch_mm: perception_config.surface_grid_pitch_mm.get(),
            max_grid_bytes: perception_config.max_grid_bytes,
        },
    )
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

    let mut physical = regular
        .contacts()
        .iter()
        .map(|contact| WorkingContact::regular(contact, topology.component_ids()))
        .collect::<Vec<_>>();
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
    if !synthetic_islands.is_empty() {
        let payload = build_candidate_payload(mesh, &synthetic_islands, candidate_config)?;
        let delicacy = selection_config
            .delicacy_by_face
            .ok_or(ContactSelectionError::MissingDelicacySidecar)?;
        let candidate_offset = generic
            .candidates()
            .candidates()
            .iter()
            .map(|candidate| candidate.id.0)
            .max()
            .map_or(0, |id| id.saturating_add(1));
        let synthetic_candidates = crate::ContactCandidatePool {
            candidates: payload
                .rows()
                .iter()
                .enumerate()
                .map(|(row, candidate)| crate::ContactCandidate {
                    id: crate::ContactCandidateId(candidate_offset.saturating_add(row as u64)),
                    face_id: candidate.face_id,
                    point_mm: candidate.point_mm,
                    normal: candidate.normal,
                    supported_area_mm2: surface.geometry().areas()[candidate.face_id as usize],
                    overhang_excess: -candidate.normal[2],
                    delicacy: delicacy[candidate.face_id as usize],
                })
                .collect(),
        };
        let selected = crate::select_contacts(
            mesh,
            &synthetic_candidates,
            &synthetic_islands,
            selection_config,
        )?;
        physical.extend(
            selected
                .contacts()
                .iter()
                .map(|contact| WorkingContact::regular(contact, topology.component_ids())),
        );
        let expected_synthetic_samples = coverage_samples
            .iter()
            .filter(|sample| sample.island_id >= first_synthetic_id)
            .cloned()
            .collect::<Vec<_>>();
        if selected.coverage_samples() != expected_synthetic_samples {
            return Err(SpecializedError::StaleRegularContacts);
        }
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

    let load_inputs = working_load_inputs(&physical, generic.islands(), &islands, reservations)?;
    let contacts = physical
        .into_iter()
        .enumerate()
        .map(|(index, mut contact)| {
            contact.demands.sort_by(compare_demands);
            let contact_id = index as u64;
            Ok(SpecializedPhysicalContact {
                contact_id,
                position_mm: contact.position_mm,
                normal: contact.normal,
                regular_candidate_id: contact
                    .regular_routes
                    .first()
                    .map(|route| route.candidate_id),
                regular_owner_island_id: contact
                    .regular_routes
                    .first()
                    .map(|route| route.owner_island_id),
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
    )?;
    Ok(SpecializedContactPlan {
        authority_digest: bound_authority_digest(generic, regular, selection_config, limits),
        generic_slice_thread_evidence: generic.slice_thread_evidence(),
        islands,
        coverage_samples,
        contacts,
        load_partition,
        gate_evidence,
    })
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
        .map_or(0, |id| id.saturating_add(1));
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
        next_id = next_id.saturating_add(1);
    }
    islands.sort_by_key(|island| island.id);
    let _ = mesh;
    Ok(())
}

#[derive(Clone)]
struct WorkingContact {
    position_mm: [f64; 3],
    normal: [f64; 3],
    regular_routes: Vec<RegularContactRoute>,
    demands: Vec<SpecializedDemand>,
}

impl WorkingContact {
    fn regular(contact: &SelectedContact, components: &[u32]) -> Self {
        Self {
            position_mm: contact.point_mm,
            normal: contact.normal,
            regular_routes: vec![RegularContactRoute {
                candidate_id: contact.candidate_id.0,
                owner_island_id: contact.island_id,
                coverage_face_id: contact.served_face_id,
                component_id: components[contact.served_face_id as usize],
            }],
            demands: Vec::new(),
        }
    }
    fn specialized(demand: SpecializedDemand) -> Self {
        Self {
            position_mm: demand.nominated_position_mm,
            normal: demand.nominated_normal,
            regular_routes: Vec::new(),
            demands: vec![demand],
        }
    }
}

struct WorkingLoadInputs {
    contacts: Vec<PhysicalContact>,
    specialized_owner_faces: Vec<(u32, u32)>,
}

fn working_load_inputs(
    contacts: &[WorkingContact],
    generic_islands: &[SupportIsland],
    islands: &[SupportIsland],
    reservations: &[SpecializedOwnerReservation],
) -> Result<WorkingLoadInputs, SpecializedError> {
    let mut physical = Vec::with_capacity(contacts.len());
    let mut specialized_owner_faces = Vec::new();
    append_variant_load_inputs(
        contacts,
        None,
        generic_islands,
        islands,
        reservations,
        &mut physical,
        &mut specialized_owner_faces,
    )?;
    Ok(WorkingLoadInputs {
        contacts: physical,
        specialized_owner_faces,
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
                if owner != demand.owner_island_id {
                    continue;
                }
                out[demand_index].covered = true;
                if same_point(point, demand.nominated_position_mm) {
                    out[demand_index].exact_regular = Some(
                        out[demand_index]
                            .exact_regular
                            .map_or(contact_index, |old| old.min(contact_index)),
                    );
                }
            }
        }
    }
    let mut local_tests = 0usize;
    for (demand_index, demand) in demands.iter().enumerate() {
        if demand.kind != SpecializedDemandKind::IslandBirth {
            continue;
        }
        for &(contact_index, owner, point) in &regular {
            if owner != demand.owner_island_id {
                continue;
            }
            local_tests = local_tests.saturating_add(1);
            check_satisfaction_limit(local_tests, limits)?;
            if satisfies(point, demand, layers, attach_reach) {
                out[demand_index].covered = true;
                if same_point(point, demand.nominated_position_mm) {
                    out[demand_index].exact_regular = Some(
                        out[demand_index]
                            .exact_regular
                            .map_or(contact_index, |old| old.min(contact_index)),
                    );
                }
            }
        }
    }
    Ok(out)
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
    let coverage_radius =
        selection_config.coverage_radius_factor.get() * selection_config.target_spacing_mm.get();
    let initial_baseline = evaluate_gate_batch(
        contacts,
        &[],
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
    let mut iterations = 0usize;
    let mut telemetry = MergeAllocationTelemetry::default();
    loop {
        check_merge_pair_limit(contacts.len(), limits.max_merge_pairs)?;
        let mut proposals = Vec::new();
        for left in 0..contacts.len() {
            for right in left + 1..contacts.len() {
                let Some(mut proposal) = accepted_merge(
                    &contacts[left],
                    &contacts[right],
                    layers,
                    attach_reach,
                    spacing_floor,
                ) else {
                    continue;
                };
                let required = proposals
                    .len()
                    .checked_add(1)
                    .ok_or(SpecializedError::GateResourceOverflow)?;
                check_merge_proposal_limit(required, limits.max_merge_proposals)?;
                proposal.left = left;
                proposal.right = right;
                proposals.push(proposal);
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
        let Some(proposal_index) = proposals
            .iter()
            .enumerate()
            .find_map(|(index, _)| gate_not_worse(vectors[index + 1], baseline).then_some(index))
        else {
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
    let accepted_vector = evaluate_gate_batch(
        contacts,
        &[],
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
    position_mm: [f64; 3],
    normal: [f64; 3],
}

fn gate_not_worse(
    candidate: FrozenSpecializedGateVector,
    baseline: FrozenSpecializedGateVector,
) -> bool {
    coverage_key(candidate) <= coverage_key(baseline)
        && candidate.load_violation_count <= baseline.load_violation_count
        && candidate.unsupported_specialized_count <= baseline.unsupported_specialized_count
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

fn variant_operation_count(
    contacts: &[WorkingContact],
    proposal: Option<&MergeProposal>,
    sample_count: usize,
    demand_count: usize,
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
    let routed_queries = sample_count
        .checked_add(demand_count)
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
    routing_work
        .checked_add(logical_contact_work)
        .and_then(|work| work.checked_add(unique_face_finalizer_work))
        .ok_or(SpecializedError::GateResourceOverflow)
}

#[allow(clippy::too_many_arguments)]
fn evaluate_gate_batch(
    contacts: &[WorkingContact],
    proposals: &[MergeProposal],
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
    let plan = gate_resource_plan(
        contacts.len(),
        proposals.len(),
        coverage_samples.len(),
        owners_by_face(islands).len(),
        authoritative_demands.len(),
        limits,
    )?;
    let face_count = owners_by_face(islands).len();
    let exact_operations = (0..plan.variants).try_fold(0usize, |total, variant| {
        let proposal = variant
            .checked_sub(1)
            .and_then(|index| proposals.get(index));
        let owner_face_count =
            variant_specialized_owner_face_count(contacts, proposal, generic.islands(), islands)?;
        total
            .checked_add(variant_operation_count(
                contacts,
                proposal,
                coverage_samples.len(),
                authoritative_demands.len(),
                face_count,
                owner_face_count,
            )?)
            .ok_or(SpecializedError::GateResourceOverflow)
    })?;
    if exact_operations > limits.max_gate_operations {
        return Err(SpecializedError::GateOperationLimit {
            required: exact_operations,
            limit: limits.max_gate_operations,
        });
    }
    let mut vectors = Vec::with_capacity(plan.variants);
    for window_start in (0..plan.variants).step_by(limits.max_gate_window_variants) {
        let window_end = (window_start + limits.max_gate_window_variants).min(plan.variants);
        let window_variants = window_end - window_start;
        let nearest_len = window_variants
            .checked_mul(coverage_samples.len())
            .ok_or(SpecializedError::GateResourceOverflow)?;
        let physical_len = (window_start..window_end).try_fold(0usize, |total, variant| {
            total
                .checked_add(variant_contact_count(contacts.len(), variant))
                .ok_or(SpecializedError::GateResourceOverflow)
        })?;
        let owner_face_len = (window_start..window_end).try_fold(0usize, |total, variant| {
            let proposal = variant
                .checked_sub(1)
                .and_then(|index| proposals.get(index));
            total
                .checked_add(variant_specialized_owner_face_count(
                    contacts,
                    proposal,
                    generic.islands(),
                    islands,
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
        let mut nearest_matrix = vec![f64::INFINITY; nearest_len];
        let mut physical_matrix = Vec::with_capacity(physical_len);
        let mut physical_offsets = Vec::with_capacity(window_variants + 1);
        let mut owner_face_matrix = Vec::with_capacity(owner_face_len);
        let mut owner_face_offsets = Vec::with_capacity(window_variants + 1);
        physical_offsets.push(0);
        owner_face_offsets.push(0);
        for (window_row, variant) in (window_start..window_end).enumerate() {
            let proposal = variant
                .checked_sub(1)
                .and_then(|index| proposals.get(index));
            let nearest = &mut nearest_matrix
                [window_row * coverage_samples.len()..(window_row + 1) * coverage_samples.len()];
            fill_variant_nearest(contacts, proposal, coverage_samples, nearest);
            append_variant_load_inputs(
                contacts,
                proposal,
                generic.islands(),
                islands,
                reservations,
                &mut physical_matrix,
                &mut owner_face_matrix,
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
            let nearest = &nearest_matrix
                [window_row * coverage_samples.len()..(window_row + 1) * coverage_samples.len()];
            let coverage = assess_coverage(
                coverage_samples,
                topology,
                nearest,
                coverage_radius,
                coverage_area_floor_mm2,
            )?;
            let physical =
                &physical_matrix[physical_offsets[window_row]..physical_offsets[window_row + 1]];
            let owner_faces = &owner_face_matrix
                [owner_face_offsets[window_row]..owner_face_offsets[window_row + 1]];
            let partition =
                finalize_specialized_load_partition(generic, mesh, surface, physical, owner_faces)?;
            let unsupported_specialized_count = authoritative_demands
                .iter()
                .filter(|demand| {
                    !variant_satisfies_demand(contacts, proposal, demand, layers, attach_reach)
                })
                .count();
            vectors.push(FrozenSpecializedGateVector::from_live(
                coverage.objective,
                load_violation_count(&partition, physical.len())?,
                unsupported_specialized_count,
            ));
        }
    }
    Ok(vectors)
}

fn variant_contact_count(contact_count: usize, variant: usize) -> usize {
    if variant == 0 {
        contact_count
    } else {
        contact_count.saturating_sub(1)
    }
}

fn proposal_serves_sample(
    proposal: &MergeProposal,
    contacts: &[WorkingContact],
    sample: &CoverageSample,
) -> bool {
    contact_serves_sample(&contacts[proposal.left], sample)
        || contact_serves_sample(&contacts[proposal.right], sample)
}

fn fill_variant_nearest(
    contacts: &[WorkingContact],
    proposal: Option<&MergeProposal>,
    samples: &[CoverageSample],
    nearest: &mut [f64],
) {
    for (contact_index, contact) in contacts.iter().enumerate() {
        if proposal.is_some_and(|p| contact_index == p.left || contact_index == p.right) {
            continue;
        }
        for (sample_index, sample) in samples.iter().enumerate() {
            if contact_serves_sample(contact, sample) {
                nearest[sample_index] =
                    nearest[sample_index].min(distance3(contact.position_mm, sample.point_mm));
            }
        }
    }
    if let Some(proposal) = proposal {
        for (sample_index, sample) in samples.iter().enumerate() {
            if proposal_serves_sample(proposal, contacts, sample) {
                nearest[sample_index] =
                    nearest[sample_index].min(distance3(proposal.position_mm, sample.point_mm));
            }
        }
    }
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

fn contact_load_descriptor(
    left: &WorkingContact,
    right: Option<&WorkingContact>,
    generic_owner_ids: &BTreeSet<u32>,
    islands: &[SupportIsland],
) -> (Option<u32>, Vec<u32>) {
    let mut routes = left.regular_routes.iter().copied().collect::<BTreeSet<_>>();
    let mut owner_ids = left
        .regular_routes
        .iter()
        .map(|route| route.owner_island_id)
        .chain(left.demands.iter().map(|demand| demand.owner_island_id))
        .collect::<BTreeSet<_>>();
    let mut has_demands = !left.demands.is_empty();
    if let Some(right) = right {
        routes.extend(right.regular_routes.iter().copied());
        owner_ids.extend(
            right
                .regular_routes
                .iter()
                .map(|route| route.owner_island_id)
                .chain(right.demands.iter().map(|demand| demand.owner_island_id)),
        );
        has_demands |= !right.demands.is_empty();
    }
    if !has_demands && routes.len() == 1 {
        let owner = routes.first().expect("one route").owner_island_id;
        if generic_owner_ids.contains(&owner) {
            return (Some(owner), Vec::new());
        }
    }
    let faces = owner_ids
        .into_iter()
        .flat_map(|id| {
            islands
                .iter()
                .find(|island| island.id == id)
                .into_iter()
                .flat_map(|island| island.face_ids.iter().copied())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    (None, faces)
}

fn variant_specialized_owner_face_count(
    contacts: &[WorkingContact],
    proposal: Option<&MergeProposal>,
    generic_islands: &[SupportIsland],
    islands: &[SupportIsland],
) -> Result<usize, SpecializedError> {
    let generic_owner_ids = generic_islands
        .iter()
        .map(|island| island.id)
        .collect::<BTreeSet<_>>();
    let mut count = 0usize;
    for (index, contact) in contacts.iter().enumerate() {
        if proposal.is_some_and(|p| index == p.left || index == p.right) {
            continue;
        }
        count = count
            .checked_add(
                contact_load_descriptor(contact, None, &generic_owner_ids, islands)
                    .1
                    .len(),
            )
            .ok_or(SpecializedError::GateResourceOverflow)?;
    }
    if let Some(proposal) = proposal {
        count = count
            .checked_add(
                contact_load_descriptor(
                    &contacts[proposal.left],
                    Some(&contacts[proposal.right]),
                    &generic_owner_ids,
                    islands,
                )
                .1
                .len(),
            )
            .ok_or(SpecializedError::GateResourceOverflow)?;
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
) -> Result<(), SpecializedError> {
    let first_specialized_owner = reservations
        .iter()
        .map(|reservation| reservation.owner_id)
        .max()
        .map_or(Ok(0), |owner| {
            owner
                .checked_add(1)
                .ok_or(SpecializedError::OwnerIdOverflow)
        })?;
    let generic_owner_ids = generic_islands
        .iter()
        .map(|island| island.id)
        .collect::<BTreeSet<_>>();
    let start = physical.len();
    let mut append = |position_mm: [f64; 3],
                      descriptor: (Option<u32>, Vec<u32>)|
     -> Result<(), SpecializedError> {
        let contact_id =
            u64::try_from(physical.len() - start).map_err(|_| SpecializedError::OwnerIdOverflow)?;
        let owner = if let Some(generic_owner) = descriptor.0 {
            LoadOwner::GenericIsland(generic_owner)
        } else {
            let specialized_owner = first_specialized_owner
                .checked_add(
                    u32::try_from(contact_id).map_err(|_| SpecializedError::OwnerIdOverflow)?,
                )
                .ok_or(SpecializedError::OwnerIdOverflow)?;
            specialized_owner_faces.extend(
                descriptor
                    .1
                    .into_iter()
                    .map(|face_id| (specialized_owner, face_id)),
            );
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
        append(
            contact.position_mm,
            contact_load_descriptor(contact, None, &generic_owner_ids, islands),
        )?;
    }
    if let Some(proposal) = proposal {
        append(
            proposal.position_mm,
            contact_load_descriptor(
                &contacts[proposal.left],
                Some(&contacts[proposal.right]),
                &generic_owner_ids,
                islands,
            ),
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

fn contact_serves_sample(contact: &WorkingContact, sample: &CoverageSample) -> bool {
    contact.regular_routes.iter().any(|route| {
        route.owner_island_id == sample.island_id && route.component_id == sample.component_id
    }) || contact.demands.iter().any(|demand| {
        demand.owner_island_id == sample.island_id
            && demand.coverage_component_id == sample.component_id
    })
}

/// The sole acceptance predicate, used both by construction and the final
/// invariant.  It checks physical locking, topology routing and every logical
/// minimum/birth gate together; no second verifier can disagree about a pair.
fn accepted_merge(
    left: &WorkingContact,
    right: &WorkingContact,
    layers: &LayerPlan,
    attach_reach: f64,
    spacing_floor: f64,
) -> Option<MergeProposal> {
    let classes = (
        !left.regular_routes.is_empty(),
        !right.regular_routes.is_empty(),
    );
    let distance = distance3(left.position_mm, right.position_mm);
    if classes.0 || classes.1 {
        if !same_point(left.position_mm, right.position_mm) {
            return None;
        }
    } else if distance >= spacing_floor {
        return None;
    }
    let demands = || left.demands.iter().chain(&right.demands);
    if demands().all(|demand| demand.kind == SpecializedDemandKind::IslandBirth)
        && distance > BIRTH_LINK_RADIUS_MM
    {
        return None;
    }
    if let Some(component) = demands().next().map(|demand| demand.coverage_component_id) {
        if demands().any(|demand| demand.coverage_component_id != component) {
            return None;
        }
    } else if !(classes.0 && classes.1) {
        return None;
    }
    let representative = if classes.0 {
        Some((left.position_mm, left.normal))
    } else if classes.1 {
        Some((right.position_mm, right.normal))
    } else {
        demands()
            .filter_map(|candidate| {
                let seat = (candidate.nominated_position_mm, candidate.nominated_normal);
                demands()
                    .all(|demand| satisfies(seat.0, demand, layers, attach_reach))
                    .then_some(seat)
            })
            .min_by(compare_seats)
    }?;
    if !demands().all(|demand| satisfies(representative.0, demand, layers, attach_reach)) {
        return None;
    }
    Some(MergeProposal {
        left: 0,
        right: 0,
        position_mm: representative.0,
        normal: representative.1,
    })
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
        position_mm: proposal.position_mm,
        normal: proposal.normal,
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
        .then_with(|| (left.kind as u8).cmp(&(right.kind as u8)))
        .then_with(|| compare_points3(&left.region_points_mm, &right.region_points_mm))
        .then_with(|| compare_option_f64(left.birth_z_mm, right.birth_z_mm))
        .then_with(|| compare_optional_solids(&left.birth_footprint, &right.birth_footprint))
}

fn compare_optional_solids(left: &Option<SliceSolid>, right: &Option<SliceSolid>) -> Ordering {
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(left), Some(right)) => {
            compare_points2(left.outer().points_xy_mm(), right.outer().points_xy_mm())
                .then_with(|| left.holes().len().cmp(&right.holes().len()))
                .then_with(|| {
                    left.holes()
                        .iter()
                        .zip(right.holes())
                        .find_map(|(a, b)| {
                            let ordering = compare_points2(a.points_xy_mm(), b.points_xy_mm());
                            (ordering != Ordering::Equal).then_some(ordering)
                        })
                        .unwrap_or(Ordering::Equal)
                })
        }
    }
}

fn compare_points2(left: &[[f64; 2]], right: &[[f64; 2]]) -> Ordering {
    left.iter()
        .zip(right)
        .find_map(|(a, b)| {
            let ordering = numeric_cmp(a[0], b[0]).then_with(|| numeric_cmp(a[1], b[1]));
            (ordering != Ordering::Equal).then_some(ordering)
        })
        .unwrap_or_else(|| left.len().cmp(&right.len()))
}

fn compare_contacts(left: &WorkingContact, right: &WorkingContact) -> Ordering {
    compare_point3(left.position_mm, right.position_mm)
        .then_with(|| left.regular_routes.cmp(&right.regular_routes))
        .then_with(|| compare_point3(left.normal, right.normal))
        .then_with(|| compare_demand_slices(&left.demands, &right.demands))
}

fn compare_seats(left: &([f64; 3], [f64; 3]), right: &([f64; 3], [f64; 3])) -> Ordering {
    compare_point3(left.0, right.0).then_with(|| compare_point3(left.1, right.1))
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

fn compare_points3(left: &[[f64; 3]], right: &[[f64; 3]]) -> Ordering {
    left.iter()
        .zip(right)
        .find_map(|(a, b)| {
            let ordering = compare_point3(*a, *b);
            (ordering != Ordering::Equal).then_some(ordering)
        })
        .unwrap_or_else(|| left.len().cmp(&right.len()))
}

fn compare_point3(left: [f64; 3], right: [f64; 3]) -> Ordering {
    numeric_cmp(left[0], right[0])
        .then_with(|| numeric_cmp(left[1], right[1]))
        .then_with(|| numeric_cmp(left[2], right[2]))
}

fn compare_option_f64(left: Option<f64>, right: Option<f64>) -> Ordering {
    match (left, right) {
        (Some(a), Some(b)) => numeric_cmp(a, b),
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
    }
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

fn bound_authority_digest(
    generic: &GenericContactPlan,
    regular: &SelectedContacts,
    config: &ContactSelectionConfig<'_>,
    limits: SpecializedLimits,
) -> u64 {
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
    hash.finish()
}

struct Fnv(u64);

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
    use super::*;
    use plinth_core::{Finite, PositiveFinite};
    use plinth_mesh::{make_overhang_table, make_raking_blade, make_thin_fin};

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

    #[test]
    fn exact_near_and_cross_face_demands_use_one_canonical_representative() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let left = minimum_demand(0, 2, 7, [1.0, 2.0, 0.1]);
        let right = minimum_demand(1, 9, 7, [1.1, 2.0, 0.1]);
        let contacts = vec![
            WorkingContact::specialized(right),
            WorkingContact::specialized(left),
        ];
        let mut proposal = accepted_merge(&contacts[0], &contacts[1], &layers, 0.2, 2.5).unwrap();
        proposal.left = 0;
        proposal.right = 1;
        let merged = commit_merge(
            &proposal,
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
    fn exact_regular_seat_is_locked_and_carries_the_specialized_demand() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let demand = minimum_demand(0, 2, 7, [1.0, 2.0, 0.1]);
        let contacts = vec![
            WorkingContact {
                position_mm: [1.0, 2.0, 0.1],
                normal: [0.0, 0.0, -1.0],
                regular_routes: vec![RegularContactRoute {
                    candidate_id: 44,
                    owner_island_id: 0,
                    coverage_face_id: 2,
                    component_id: 7,
                }],
                demands: Vec::new(),
            },
            WorkingContact::specialized(demand),
        ];
        let mut proposal = accepted_merge(&contacts[0], &contacts[1], &layers, 0.2, 2.5).unwrap();
        proposal.left = 0;
        proposal.right = 1;
        let merged = commit_merge(
            &proposal,
            &contacts,
            usize::MAX,
            &mut MergeAllocationTelemetry::default(),
        )
        .unwrap();
        assert_eq!(merged.regular_routes[0].candidate_id, 44);
        assert_eq!(merged.demands.len(), 1);
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
    fn merge_commit_preflights_deep_payload_at_exact_cap_before_clone() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let mut left_demand = minimum_demand(0, 2, 7, [1.0, 2.0, 0.1]);
        left_demand.region_points_mm = vec![[1.0, 2.0, 0.1]; 10_000];
        let contacts = vec![
            WorkingContact::specialized(left_demand),
            WorkingContact::specialized(minimum_demand(1, 9, 7, [1.1, 2.0, 0.1])),
        ];
        let mut proposal = accepted_merge(&contacts[0], &contacts[1], &layers, 0.2, 2.5).unwrap();
        proposal.left = 0;
        proposal.right = 1;
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
            commit_merge(&proposal, &contacts, exact - 1, &mut refused),
            Err(SpecializedError::MergeCommitByteLimit {
                required: exact,
                limit,
            })
            if limit == exact - 1
        ));
        assert_eq!(refused.deep_clone_count, 0);
        assert_eq!(refused.max_commit_bytes, 0);
        let mut accepted = MergeAllocationTelemetry::default();
        let merged = commit_merge(&proposal, &contacts, exact, &mut accepted).unwrap();
        assert_eq!(merged.demands.len(), 2);
        assert_eq!(accepted.deep_clone_count, 1);
        assert_eq!(accepted.max_commit_bytes, exact);
    }

    #[test]
    fn synthetic_only_route_contributes_its_claimed_face_to_load_eligibility() {
        let contacts = vec![WorkingContact {
            position_mm: [1.0, 2.0, 0.1],
            normal: [0.0, 0.0, -1.0],
            regular_routes: vec![RegularContactRoute {
                candidate_id: 44,
                owner_island_id: 9,
                coverage_face_id: 3,
                component_id: 7,
            }],
            demands: Vec::new(),
        }];
        let inputs = working_load_inputs(
            &contacts,
            &[],
            &[SupportIsland {
                id: 9,
                face_ids: vec![3],
            }],
            &[],
        )
        .unwrap();
        assert_eq!(inputs.contacts.len(), 1);
        assert_eq!(inputs.contacts[0].owner, LoadOwner::Specialized(0));
        assert_eq!(inputs.specialized_owner_faces, vec![(0, 3)]);
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

        let limits = SpecializedLimits::production();
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
        let regular = WorkingContact {
            position_mm: [0.1, 0.0, 0.0],
            normal: [0.0, 0.0, -1.0],
            regular_routes: vec![RegularContactRoute {
                candidate_id: 7,
                owner_island_id: 3,
                coverage_face_id: 2,
                component_id: 7,
            }],
            demands: Vec::new(),
        };
        let matching = minimum_demand(3, 2, 7, [0.0, 0.0, 0.0]);
        let other_owner = minimum_demand(4, 2, 7, [0.0, 0.0, 0.0]);
        let dispositions = regular_dispositions(
            &[regular],
            &[matching, other_owner],
            &layers,
            0.2,
            SpecializedLimits::production(),
        )
        .unwrap();
        assert!(dispositions[0].covered);
        assert_eq!(dispositions[0].exact_regular, None);
        assert!(!dispositions[1].covered);
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
            GenericPerceptionLimits::production(),
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
            GenericPerceptionLimits::production(),
            &generic,
            &regular,
            &selection_config,
            SpecializedLimits::production(),
        )
        .map_err(|error| format!("required {name} specialized fixture failed: {error}"))
    }

    #[test]
    fn required_table_raking_and_thin_fin_fixtures_finalize_full_evidence() {
        for (name, mesh, expected_digest, expected_counts, expected_gate) in [
            (
                "overhang table",
                make_overhang_table().unwrap(),
                2_380_499_546_943_577_664_u64,
                (95, 51_200, 14),
                (6, 10, 4),
            ),
            (
                "raking blade",
                make_raking_blade().unwrap(),
                6_330_805_288_930_875_620_u64,
                (40, 40_192, 13),
                (3, 43, 8),
            ),
            (
                "thin fin",
                make_thin_fin().unwrap(),
                1_794_713_636_406_357_628_u64,
                (19, 20_480, 2),
                (2, 1, 1),
            ),
        ] {
            let plan = full_fixture(mesh, name).unwrap_or_else(|error| panic!("{error}"));
            assert_eq!(
                plan.digest(),
                expected_digest,
                "required {name} exact vector"
            );
            assert_eq!(
                (
                    plan.contacts().len(),
                    plan.coverage_samples().len(),
                    plan.load_partition().rows().len(),
                ),
                expected_counts,
                "required {name} exact row counts",
            );
            let gate = plan.gate_evidence();
            assert_eq!(
                gate.baseline, gate.accepted,
                "required {name} non-worsening gate"
            );
            assert_eq!(
                (
                    gate.accepted.load_violation_count,
                    gate.evaluated_pair_count,
                    gate.accepted_pair_count,
                ),
                expected_gate,
                "required {name} exact load and consolidation gate",
            );
            let carried_faces = plan
                .load_partition()
                .rows()
                .iter()
                .map(|row| row.face_id)
                .collect::<BTreeSet<_>>();
            assert_eq!(
                carried_faces.len(),
                plan.load_partition().rows().len(),
                "required {name} carries every face once"
            );
            if name == "thin fin" {
                assert!(
                    !plan.coverage_samples().is_empty(),
                    "required thin-fin synthetic island must use the shared dyadic coverage builder"
                );
            }
        }
    }

    #[test]
    fn specialized_binding_rejects_plate_selection_and_limit_staleness() {
        let mesh = make_overhang_table().unwrap();
        let delicacy = vec![0.0; mesh.tris().len()];
        let layers = LayerPlan::new(0.0, vec![0.05; 1_000]).unwrap();
        let perception = perception_config(&delicacy);
        let candidate = candidate_config();
        let perception_limits = GenericPerceptionLimits::production();
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
        let limits = SpecializedLimits::production();
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
}
