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

use crate::dyadic::CandidatePayloadConfig;
use crate::perception::{
    finalize_specialized_load_partition, FinalizedLoadPartition, GenericContactPlan,
    GenericPerceptionError, GenericPerceptionLimits, LoadOwner, PhysicalContact, SliceSolid,
    SpecializedOwnerReservation,
};
use crate::{
    build_dyadic_coverage_samples, BuildFrame, ContactSelectionConfig, ContactSelectionError,
    CoverageSample, SelectedContact, SelectedContacts, SupportIsland, SupportPerceptionConfig,
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
    demands: Vec<SpecializedDemand>,
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
    pub const fn is_shared_regular(&self) -> bool {
        self.regular_candidate_id.is_some() && !self.demands.is_empty()
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
    pub const fn generic_slice_thread_evidence(&self) -> (usize, usize) {
        self.generic_slice_thread_evidence
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
        for contact in &self.contacts {
            hash.u64(contact.contact_id);
            hash.point3(contact.position_mm);
            hash.point3(contact.normal);
            hash.u64(contact.regular_candidate_id.unwrap_or(u64::MAX));
            hash.u32(contact.regular_owner_island_id.unwrap_or(u32::MAX));
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
            hash.f64(row.carried_area_mm2);
        }
        hash.finish()
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum SpecializedError {
    #[error(transparent)]
    Generic(#[from] GenericPerceptionError),
    #[error(transparent)]
    Selection(#[from] ContactSelectionError),
    #[error("specialized contact could not build topology: {0}")]
    Topology(String),
    #[error("specialized contact could not build the canonical surface: {0}")]
    Surface(String),
    #[error("regular contact evidence is stale or does not belong to the generic plan")]
    StaleRegularContacts,
    #[error("specialized projection returned an invalid face")]
    InvalidProjection,
    #[error("specialized satisfaction work requires {required} tests; limit is {limit}")]
    SatisfactionLimit { required: usize, limit: usize },
    #[error("specialized satisfaction neighbor query failed: {0}")]
    SatisfactionNeighbors(String),
    #[error("specialized merge work requires {required} pairs; limit is {limit}")]
    MergePairLimit { required: usize, limit: usize },
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
    validate_regular(
        mesh,
        generic,
        regular,
        selection_config,
        topology.component_ids(),
    )?;
    let surface = NativeSurface::from_indexed(
        mesh.verts(),
        mesh.tris(),
        NativeSurfaceConfig {
            grid_pitch_mm: perception_config.surface_grid_pitch_mm.get(),
            max_grid_bytes: perception_config.max_grid_bytes,
        },
    )
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
    let (_distance, _projected, face_ids, normals) = surface.project(&positions);
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
            normal: normals[index],
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
            normal: normals[index],
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
        .map(WorkingContact::regular)
        .collect::<Vec<_>>();
    let mut demands = Vec::new();
    for seed in seeds {
        let seed_owners = owners.get(&seed.face_id).cloned().unwrap_or_default();
        for owner in seed_owners {
            demands.push(seed.to_demand(owner, topology.component_ids()[seed.face_id as usize]));
        }
    }
    demands.sort_by(compare_demands);
    demands.dedup_by(|left, right| compare_demands(left, right) == Ordering::Equal);
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
    consolidate(&mut physical, layers, attach_reach, spacing_floor, limits)?;
    canonicalize_and_validate(&mut physical, &islands, layers, attach_reach)?;

    let mut specialized_owner_faces = Vec::new();
    let mut finalized_contacts = Vec::with_capacity(physical.len());
    let first_specialized_owner = reservations
        .iter()
        .map(|reservation| reservation.owner_id)
        .max()
        .map_or(Ok(0), |owner| {
            owner
                .checked_add(1)
                .ok_or(SpecializedError::OwnerIdOverflow)
        })?;
    let contacts = physical
        .into_iter()
        .enumerate()
        .map(|(index, mut contact)| {
            contact.demands.sort_by(compare_demands);
            let contact_id = index as u64;
            let owner = if contact.demands.is_empty() {
                LoadOwner::GenericIsland(
                    contact
                        .regular_owner_island_id
                        .expect("a demand-free contact is regular"),
                )
            } else {
                let specialized_owner = first_specialized_owner
                    .checked_add(
                        u32::try_from(contact_id).map_err(|_| SpecializedError::OwnerIdOverflow)?,
                    )
                    .ok_or(SpecializedError::OwnerIdOverflow)?;
                let mut owner_ids = contact
                    .demands
                    .iter()
                    .map(|demand| demand.owner_island_id)
                    .collect::<BTreeSet<_>>();
                if let Some(regular_owner) = contact.regular_owner_island_id {
                    owner_ids.insert(regular_owner);
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
                    .collect::<Vec<_>>();
                specialized_owner_faces.push((specialized_owner, faces));
                LoadOwner::Specialized(specialized_owner)
            };
            finalized_contacts.push(PhysicalContact {
                contact_id,
                owner,
                point_mm: contact.position_mm,
            });
            Ok(SpecializedPhysicalContact {
                contact_id,
                position_mm: contact.position_mm,
                normal: contact.normal,
                regular_candidate_id: contact.regular_candidate_id,
                regular_owner_island_id: contact.regular_owner_island_id,
                demands: contact.demands,
            })
        })
        .collect::<Result<Vec<_>, SpecializedError>>()?;
    let load_partition = finalize_specialized_load_partition(
        generic,
        mesh,
        &surface,
        &finalized_contacts,
        &specialized_owner_faces,
    )?;
    let mut authority = Fnv::new();
    authority.u64(generic.digest());
    authority.u64(regular.digest());
    authority.usize(layers.count());
    for height in layers.heights_mm() {
        authority.f64(*height);
    }
    Ok(SpecializedContactPlan {
        authority_digest: authority.finish(),
        generic_slice_thread_evidence: generic.slice_thread_evidence(),
        islands,
        coverage_samples,
        contacts,
        load_partition,
    })
}

fn validate_regular(
    mesh: &Mesh,
    generic: &GenericContactPlan,
    regular: &SelectedContacts,
    config: &ContactSelectionConfig<'_>,
    components: &[u32],
) -> Result<(), SpecializedError> {
    let expected = build_dyadic_coverage_samples(mesh, generic.islands(), components, config)?;
    if expected != regular.coverage_samples() {
        return Err(SpecializedError::StaleRegularContacts);
    }
    for contact in regular.contacts() {
        let candidate = generic
            .candidates()
            .candidates()
            .iter()
            .find(|candidate| candidate.id == contact.candidate_id)
            .ok_or(SpecializedError::StaleRegularContacts)?;
        let island = generic
            .islands()
            .iter()
            .find(|island| island.id == contact.island_id)
            .ok_or(SpecializedError::StaleRegularContacts)?;
        if candidate.face_id != contact.served_face_id
            || candidate.point_mm != contact.point_mm
            || candidate.normal != contact.normal
            || candidate.delicacy.to_bits() != contact.delicacy.to_bits()
            || candidate.supported_area_mm2.to_bits() != contact.supported_area_mm2.to_bits()
            || island
                .face_ids
                .binary_search(&contact.served_face_id)
                .is_err()
        {
            return Err(SpecializedError::StaleRegularContacts);
        }
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
    regular_candidate_id: Option<u64>,
    regular_owner_island_id: Option<u32>,
    demands: Vec<SpecializedDemand>,
}

impl WorkingContact {
    fn regular(contact: &SelectedContact) -> Self {
        Self {
            position_mm: contact.point_mm,
            normal: contact.normal,
            regular_candidate_id: Some(contact.candidate_id.0),
            regular_owner_island_id: Some(contact.island_id),
            demands: Vec::new(),
        }
    }
    fn specialized(demand: SpecializedDemand) -> Self {
        Self {
            position_mm: demand.nominated_position_mm,
            normal: demand.nominated_normal,
            regular_candidate_id: None,
            regular_owner_island_id: None,
            demands: vec![demand],
        }
    }
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
        .filter_map(|(index, contact)| {
            contact
                .regular_owner_island_id
                .map(|owner| (index, owner, contact.position_mm))
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

fn consolidate(
    contacts: &mut Vec<WorkingContact>,
    layers: &LayerPlan,
    attach_reach: f64,
    spacing_floor: f64,
    limits: SpecializedLimits,
) -> Result<(), SpecializedError> {
    let pairs = contacts
        .len()
        .checked_mul(contacts.len().saturating_sub(1))
        .and_then(|value| value.checked_div(2))
        .unwrap_or(usize::MAX);
    if pairs > limits.max_merge_pairs {
        return Err(SpecializedError::MergePairLimit {
            required: pairs,
            limit: limits.max_merge_pairs,
        });
    }
    contacts.sort_by(compare_contacts);
    let mut consolidated: Vec<WorkingContact> = Vec::with_capacity(contacts.len());
    for contact in std::mem::take(contacts) {
        let mut pending = Some(contact);
        for existing in &mut consolidated {
            let Some(candidate) = pending.as_ref() else {
                break;
            };
            if let Some(merged) =
                accepted_merge(existing, candidate, layers, attach_reach, spacing_floor)
            {
                *existing = merged;
                pending = None;
                break;
            }
        }
        if let Some(contact) = pending {
            consolidated.push(contact);
        }
    }
    consolidated.sort_by(compare_contacts);
    for left in 0..consolidated.len() {
        for right in left + 1..consolidated.len() {
            if accepted_merge(
                &consolidated[left],
                &consolidated[right],
                layers,
                attach_reach,
                spacing_floor,
            )
            .is_some()
            {
                return Err(SpecializedError::UnmergedCompatiblePair { left, right });
            }
        }
    }
    *contacts = consolidated;
    Ok(())
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
) -> Option<WorkingContact> {
    let classes = (
        left.regular_candidate_id.is_some(),
        right.regular_candidate_id.is_some(),
    );
    if classes == (true, true) {
        return None;
    }
    let distance = distance3(left.position_mm, right.position_mm);
    if classes.0 || classes.1 {
        if !same_point(left.position_mm, right.position_mm) {
            return None;
        }
    } else if distance >= spacing_floor {
        return None;
    }
    let mut demands = left.demands.clone();
    demands.extend(right.demands.clone());
    demands.sort_by(compare_demands);
    demands.dedup_by(|a, b| compare_demands(a, b) == Ordering::Equal);
    if demands
        .iter()
        .all(|demand| demand.kind == SpecializedDemandKind::IslandBirth)
        && distance > BIRTH_LINK_RADIUS_MM
    {
        return None;
    }
    if demands
        .iter()
        .map(|demand| demand.coverage_component_id)
        .collect::<BTreeSet<_>>()
        .len()
        != 1
    {
        return None;
    }
    let representative = if classes.0 {
        Some((left.position_mm, left.normal))
    } else if classes.1 {
        Some((right.position_mm, right.normal))
    } else {
        let mut candidates = demands
            .iter()
            .map(|demand| (demand.nominated_position_mm, demand.nominated_normal))
            .collect::<Vec<_>>();
        candidates.sort_by(compare_seats);
        candidates.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
        candidates.into_iter().find(|(position, _)| {
            demands
                .iter()
                .all(|demand| satisfies(*position, demand, layers, attach_reach))
        })
    }?;
    if !demands
        .iter()
        .all(|demand| satisfies(representative.0, demand, layers, attach_reach))
    {
        return None;
    }
    let locked = if classes.0 {
        left.clone()
    } else if classes.1 {
        right.clone()
    } else {
        WorkingContact {
            position_mm: representative.0,
            normal: representative.1,
            regular_candidate_id: None,
            regular_owner_island_id: None,
            demands: Vec::new(),
        }
    };
    Some(WorkingContact { demands, ..locked })
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
        .then_with(|| left.regular_candidate_id.cmp(&right.regular_candidate_id))
        .then_with(|| {
            left.regular_owner_island_id
                .cmp(&right.regular_owner_island_id)
        })
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
        let mut contacts = vec![
            WorkingContact::specialized(right),
            WorkingContact::specialized(left),
        ];
        consolidate(
            &mut contacts,
            &layers,
            0.2,
            2.5,
            SpecializedLimits::production(),
        )
        .unwrap();
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0].position_mm, [1.0, 2.0, 0.1]);
        assert_eq!(contacts[0].demands.len(), 2);
        assert_eq!(contacts[0].demands[0].coverage_face_id, 2);
        assert_eq!(contacts[0].demands[1].coverage_face_id, 9);
    }

    #[test]
    fn exact_regular_seat_is_locked_and_carries_the_specialized_demand() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let demand = minimum_demand(0, 2, 7, [1.0, 2.0, 0.1]);
        let mut contacts = vec![
            WorkingContact {
                position_mm: [1.0, 2.0, 0.1],
                normal: [0.0, 0.0, -1.0],
                regular_candidate_id: Some(44),
                regular_owner_island_id: Some(0),
                demands: Vec::new(),
            },
            WorkingContact::specialized(demand),
        ];
        consolidate(
            &mut contacts,
            &layers,
            0.2,
            2.5,
            SpecializedLimits::production(),
        )
        .unwrap();
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0].regular_candidate_id, Some(44));
        assert_eq!(contacts[0].demands.len(), 1);
    }

    #[test]
    fn merge_pair_refusal_is_exact_at_cap_and_cap_plus_one() {
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let contacts = || {
            vec![
                WorkingContact::specialized(minimum_demand(0, 2, 7, [1.0, 2.0, 0.1])),
                WorkingContact::specialized(minimum_demand(1, 9, 7, [1.1, 2.0, 0.1])),
            ]
        };
        let mut at_cap = contacts();
        let mut limits = SpecializedLimits::production();
        limits.max_merge_pairs = 1;
        consolidate(&mut at_cap, &layers, 0.2, 2.5, limits).unwrap();
        assert_eq!(at_cap.len(), 1);

        let mut above_cap = contacts();
        limits.max_merge_pairs = 0;
        assert_eq!(
            consolidate(&mut above_cap, &layers, 0.2, 2.5, limits),
            Err(SpecializedError::MergePairLimit {
                required: 1,
                limit: 0,
            })
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
            regular_candidate_id: Some(7),
            regular_owner_island_id: Some(3),
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
        for (name, mesh) in [
            ("overhang table", make_overhang_table().unwrap()),
            ("raking blade", make_raking_blade().unwrap()),
            ("thin fin", make_thin_fin().unwrap()),
        ] {
            let plan = full_fixture(mesh, name).unwrap_or_else(|error| panic!("{error}"));
            assert_ne!(plan.digest(), 0, "required {name} digest");
            assert!(
                !plan.load_partition().rows().is_empty(),
                "required {name} load"
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

    const SWEEP_CHILD: &str = "PLINTH_SPECIALIZED_SWEEP_CHILD";
    const SWEEP_LINE: &str = "PLINTH_SPECIALIZED_SWEEP=";

    #[test]
    fn specialized_thread_sweep_child() {
        if std::env::var_os(SWEEP_CHILD).is_none() {
            return;
        }
        let plan = full_fixture(make_raking_blade().unwrap(), "raking blade").unwrap();
        let (effective, participating) = plan.generic_slice_thread_evidence();
        assert!(effective >= 1);
        assert!(participating >= 1);
        let requested = std::env::var("PLINTH_THREADS")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        if requested > 1 {
            assert!(
                effective > 1 && participating > 1,
                "raking fixture must exercise more than one worker at {requested} threads"
            );
        }
        println!(
            "{SWEEP_LINE}{}:{}:{}:{}",
            plan.digest(),
            plan.contacts().len(),
            plan.coverage_samples().len(),
            plan.load_partition().rows().len()
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
