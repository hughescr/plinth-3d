//! Bound, batch-first generic support perception for the frozen pre-P8 path.
//!
//! The older [`crate::perceive_contacts`] leaf remains the sole authority for
//! exterior/down-facing/near-plate/column-ray classification.  This module
//! adds the orchestration which turns that evidence into islands, dyadic
//! seats, print-layer births and local minima.  Its result is deliberately
//! opaque: downstream specialized placement can consume the evidence, but it
//! cannot manufacture or replay it against another mesh or layer schedule.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use geom_kernel::{
    KnnBatchLimits, KnnBatchPlan, NativeSurface, NativeSurfaceConfig, PlaneSliceLimits,
    PointCloudIndex, PointQueryError, RadiusQueryLimits,
};
use plinth_core::{m0_constants, LayerPlan, M0PlaceholderConstants, PositiveFinite};
use plinth_mesh::{mesh_fingerprint, FaceTopology, Mesh, WeldPolicy, WeldedView};
use thiserror::Error;

use crate::dyadic::{build_candidate_payload, CandidatePayloadConfig, DyadicError};
use crate::{
    perceive_contacts_on_surface, validate_perception_input, BuildFrame, ContactCandidate,
    ContactCandidateId, ContactCandidatePool, SupportIsland, SupportPerceptionConfig,
    SupportPerceptionError,
};

const LOOP_MERGE_MM: f64 = geom_kernel::PLANE_SLICE_MERGE_TOLERANCE_MM;
const BIRTH_MIN_AREA_MM2: f64 = 1.0e-3;

/// Explicit, result-inert refusal limits for generic perception.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GenericPerceptionLimits {
    pub max_planes: usize,
    pub max_triangle_plane_tests: usize,
    pub max_slice_intersections: usize,
    pub max_slice_output_bytes: usize,
    pub max_loop_vertices: usize,
    pub max_minima_vertices: usize,
    /// Caller-owned execution authority for the minima-to-face kNN batch.
    /// This guard refuses oversized work but never changes a successful plan.
    pub minima_knn: KnnBatchLimits,
    pub max_island_neighbor_pairs: usize,
    pub max_island_neighbor_bytes: usize,
    pub max_overlap_pairs: usize,
    pub max_overlap_bytes: usize,
    pub max_overlap_vertex_tests: usize,
    pub max_overlap_edge_tests: usize,
}

impl GenericPerceptionLimits {
    #[must_use]
    pub const fn production(minima_knn: KnnBatchLimits) -> Self {
        Self {
            max_planes: 20_000,
            max_triangle_plane_tests: 100_000_000,
            max_slice_intersections: 100_000_000,
            max_slice_output_bytes: 2 * 1024 * 1024 * 1024,
            max_loop_vertices: 100_000_000,
            max_minima_vertices: 20_000_000,
            minima_knn,
            max_island_neighbor_pairs: 100_000_000,
            max_island_neighbor_bytes: 2 * 1024 * 1024 * 1024,
            max_overlap_pairs: 100_000_000,
            max_overlap_bytes: 2 * 1024 * 1024 * 1024,
            max_overlap_vertex_tests: 100_000_000,
            max_overlap_edge_tests: 100_000_000,
        }
    }
}

/// A specialized owner known before generic load partitioning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SpecializedOwnerReservation {
    pub owner_id: u32,
    pub face_id: u32,
}

/// Closed physical-contact owner vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LoadOwner {
    Specialized(u32),
    GenericIsland(u32),
}

/// One already-placed physical contact presented to the load finalizer.
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicalContact {
    pub contact_id: u64,
    pub owner: LoadOwner,
    pub point_mm: [f64; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LoadFinalizeLimits {
    pub(crate) max_scratch_bytes: usize,
    /// `SUPPORT_SPECIALIZED_MAX_LOAD_OPERATIONS` where the specialized stage
    /// supplies it; never the whole cap on its own, see
    /// [`Self::operations_limit`].
    pub(crate) max_operations_floor: usize,
    /// `SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT`.
    pub(crate) operations_per_batch_unit: usize,
}

impl LoadFinalizeLimits {
    /// The load operation cap for one partition, materialized against the
    /// batch its resource plan was computed from:
    /// `max(floor, K_L × (rows + 1) × (contacts + 1))`.
    ///
    /// A **declared model-scaled cap**; the rule and its measurement live on
    /// [`crate::specialized::SpecializedLimits::load_operation_limit`], which
    /// is the same arithmetic over the same registered coefficient.
    pub(crate) const fn operations_limit(&self, rows: usize, contacts: usize) -> usize {
        crate::specialized::batch_product_scaled_cap(
            self.max_operations_floor,
            self.operations_per_batch_unit,
            rows,
            contacts,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LoadResourcePlan {
    pub(crate) scratch_bytes: usize,
    pub(crate) operations: usize,
    /// The row and contact counts this plan was computed from. They are carried
    /// so the operation cap can be materialized over the *same* batch the
    /// demand was stated over; see
    /// [`crate::specialized::SpecializedLimits::load_operation_limit`].
    pub(crate) rows: usize,
    pub(crate) contacts: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct LoadEligibility {
    face_id: u32,
    eligible_owners: Vec<LoadOwner>,
    centroid_mm: [f64; 3],
    area_mm2: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FinalizedFaceLoad {
    pub face_id: u32,
    pub contact_id: u64,
    pub owner: LoadOwner,
    pub carried_area_mm2: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FinalizedLoadPartition {
    authority: AuthorityFingerprint,
    contacts_fingerprint: u64,
    rows: Vec<FinalizedFaceLoad>,
}

impl FinalizedLoadPartition {
    #[must_use]
    pub fn rows(&self) -> &[FinalizedFaceLoad] {
        &self.rows
    }
}

/// Caller-owned bounds for folding the finalized unique-face partition into
/// one load and frozen tip radius per physical contact.  The final support
/// runner supplies these before allocating any contact-sized output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FinalizedContactLoadLimits {
    max_contacts: usize,
    max_rows: usize,
    max_bytes: usize,
}

impl FinalizedContactLoadLimits {
    pub(crate) const fn new(max_contacts: usize, max_rows: usize, max_bytes: usize) -> Self {
        Self {
            max_contacts,
            max_rows,
            max_bytes,
        }
    }
}

/// Immutable, batched fold of the sole finalized load authority.  It retains
/// every physical contact, including contacts that own no source face, so a
/// later stub constructor cannot accidentally change the contact namespace.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FinalizedContactLoads {
    authority: AuthorityFingerprint,
    contacts_fingerprint: u64,
    rows_fingerprint: u64,
    contact_ids: Vec<u64>,
    tip_policy: FrozenTipLoadPolicy,
    carried_area_mm2: Vec<f64>,
    tip_radii_mm: Vec<PositiveFinite>,
    digest: u64,
}

/// One immutable finalized-load leaf, looked up by the canonical physical
/// contact id.  Contact ids are deliberately opaque: callers must not infer
/// an array slot from a dense-builder implementation detail.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FinalizedContactLoadLeaf {
    contact_id: u64,
    carried_area_mm2: f64,
    tip_radius_mm: PositiveFinite,
}

impl FinalizedContactLoadLeaf {
    #[must_use]
    pub(crate) const fn contact_id(self) -> u64 {
        self.contact_id
    }

    #[must_use]
    pub(crate) const fn carried_area_mm2(self) -> f64 {
        self.carried_area_mm2
    }

    #[must_use]
    pub(crate) const fn tip_radius_mm(self) -> PositiveFinite {
        self.tip_radius_mm
    }
}

impl FinalizedContactLoads {
    /// Construct the one tip-sized load artifact retained by a specialized
    /// plan. The bounds are exact cardinality/byte bounds derived internally,
    /// so a caller cannot widen them or pair arbitrary contact ids with a
    /// partition while bypassing finalization.
    pub(crate) fn for_specialized_plan(
        partition: &FinalizedLoadPartition,
        canonical_contact_ids: &[u64],
    ) -> Result<Self, GenericPerceptionError> {
        Self::from_partition(
            partition,
            canonical_contact_ids,
            FinalizedContactLoadLimits::new(
                canonical_contact_ids.len(),
                partition.rows.len(),
                finalized_contact_load_required_bytes(canonical_contact_ids.len())?,
            ),
        )
    }

    /// Fold the stored rows in their frozen order into the caller's canonical
    /// physical-contact order. Stable ids are opaque and need not be dense.
    #[allow(
        dead_code,
        reason = "the sealed support runner consumes the specialized plan's canonical ids"
    )]
    pub(crate) fn from_partition(
        partition: &FinalizedLoadPartition,
        canonical_contact_ids: &[u64],
        limits: FinalizedContactLoadLimits,
    ) -> Result<Self, GenericPerceptionError> {
        Self::from_contact_ids(
            partition,
            canonical_contact_ids.iter().copied(),
            canonical_contact_ids.len(),
            limits,
        )
    }

    fn from_physical_contacts(
        partition: &FinalizedLoadPartition,
        contacts: &[PhysicalContact],
        limits: FinalizedContactLoadLimits,
    ) -> Result<Self, GenericPerceptionError> {
        Self::from_contact_ids(
            partition,
            contacts.iter().map(|contact| contact.contact_id),
            contacts.len(),
            limits,
        )
    }

    fn from_contact_ids(
        partition: &FinalizedLoadPartition,
        canonical_contact_ids: impl ExactSizeIterator<Item = u64>,
        contact_count: usize,
        limits: FinalizedContactLoadLimits,
    ) -> Result<Self, GenericPerceptionError> {
        if contact_count > limits.max_contacts {
            return Err(GenericPerceptionError::FinalizedLoadContactLimit {
                required: contact_count,
                limit: limits.max_contacts,
            });
        }
        if partition.rows.len() > limits.max_rows {
            return Err(GenericPerceptionError::FinalizedLoadRowLimit {
                required: partition.rows.len(),
                limit: limits.max_rows,
            });
        }
        let required_bytes = finalized_contact_load_required_bytes(contact_count)?;
        if required_bytes > limits.max_bytes {
            return Err(GenericPerceptionError::LoadScratchLimit {
                required: required_bytes,
                limit: limits.max_bytes,
            });
        }

        for row in &partition.rows {
            if !row.carried_area_mm2.is_finite() {
                return Err(GenericPerceptionError::NonFiniteFinalizedLoad {
                    contact_id: row.contact_id,
                });
            }
            if row.carried_area_mm2 < 0.0 {
                return Err(GenericPerceptionError::NegativeFinalizedLoad {
                    contact_id: row.contact_id,
                });
            }
        }

        let contact_ids = canonical_contact_ids.collect::<Vec<_>>();
        debug_assert_eq!(contact_ids.len(), contact_count);
        let mut id_to_slot = contact_ids
            .iter()
            .copied()
            .enumerate()
            .map(|(slot, contact_id)| (contact_id, slot))
            .collect::<Vec<_>>();
        id_to_slot.sort_unstable_by_key(|&(contact_id, _)| contact_id);
        if let Some(pair) = id_to_slot.windows(2).find(|pair| pair[0].0 == pair[1].0) {
            return Err(GenericPerceptionError::DuplicateFinalizedContactId {
                contact_id: pair[0].0,
            });
        }

        let policy = m0_constants()
            .map_err(|error| {
                GenericPerceptionError::Perception(
                    SupportPerceptionError::RegisteredConstantsUnavailable(error.to_string()),
                )
            })?
            .placeholders();
        let tip_policy = FrozenTipLoadPolicy::from_placeholders(policy);
        let mut carried_area_mm2 = vec![0.0_f64; contact_count];
        for row in &partition.rows {
            let slot = id_to_slot
                .binary_search_by_key(&row.contact_id, |&(contact_id, _)| contact_id)
                .ok()
                .map(|index| id_to_slot[index].1)
                .ok_or(GenericPerceptionError::UnknownFinalizedLoadContact {
                    contact_id: row.contact_id,
                })?;
            carried_area_mm2[slot] += row.carried_area_mm2;
        }
        if carried_area_mm2.iter().any(|area| !area.is_finite()) {
            return Err(GenericPerceptionError::NonFiniteFinalizedLoadTotal);
        }
        let tip_radii_mm = carried_area_mm2
            .iter()
            .copied()
            .map(|area| tip_policy.tip_radius_mm(area))
            .collect::<Result<Vec<_>, _>>()?;
        let rows_fingerprint = finalized_load_rows_fingerprint(&partition.rows);
        let digest = finalized_contact_load_digest(
            &partition.authority,
            partition.contacts_fingerprint,
            rows_fingerprint,
            tip_policy,
            &contact_ids,
            &carried_area_mm2,
            &tip_radii_mm,
        );
        Ok(Self {
            authority: partition.authority.clone(),
            contacts_fingerprint: partition.contacts_fingerprint,
            rows_fingerprint,
            contact_ids,
            tip_policy,
            carried_area_mm2,
            tip_radii_mm,
            digest,
        })
    }

    /// Reject reuse with another finalized partition, including a partition
    /// sharing the same generic authority but carrying different face rows.
    #[allow(
        dead_code,
        reason = "the sealed support runner and later verifier consume this authority check"
    )]
    pub(crate) fn validate_binding(
        &self,
        partition: &FinalizedLoadPartition,
        canonical_contact_ids: &[u64],
    ) -> Result<(), GenericPerceptionError> {
        if self.authority != partition.authority
            || self.contacts_fingerprint != partition.contacts_fingerprint
            || self.rows_fingerprint != finalized_load_rows_fingerprint(&partition.rows)
            || self.contact_ids != canonical_contact_ids
        {
            return Err(GenericPerceptionError::StaleAuthority);
        }
        Ok(())
    }

    #[allow(
        dead_code,
        reason = "the sealed support runner consumes carried contact loads"
    )]
    pub(crate) fn carried_area_mm2(&self) -> &[f64] {
        &self.carried_area_mm2
    }

    #[allow(
        dead_code,
        reason = "the sealed support runner consumes exact stable contact identities"
    )]
    pub(crate) fn contact_ids(&self) -> &[u64] {
        &self.contact_ids
    }

    /// Return the exact finalized load leaf for one stable physical-contact
    /// id.  This is intentionally an id lookup rather than an index API: the
    /// finalized namespace is stable but not required to be dense.
    #[allow(
        dead_code,
        reason = "the sealed support runner consumes physical-contact load leaves"
    )]
    pub(crate) fn leaf_for(&self, contact_id: u64) -> Option<FinalizedContactLoadLeaf> {
        self.contact_ids
            .iter()
            .position(|&id| id == contact_id)
            .map(|slot| FinalizedContactLoadLeaf {
                contact_id,
                carried_area_mm2: self.carried_area_mm2[slot],
                tip_radius_mm: self.tip_radii_mm[slot],
            })
    }

    #[allow(
        dead_code,
        reason = "the sealed support runner consumes frozen tip radii"
    )]
    pub(crate) fn tip_radii_mm(&self) -> &[PositiveFinite] {
        &self.tip_radii_mm
    }

    #[allow(
        dead_code,
        reason = "the sealed support runner and later verifier bind this digest"
    )]
    pub(crate) const fn digest(&self) -> u64 {
        self.digest
    }

    fn load_violation_count(&self) -> Result<usize, GenericPerceptionError> {
        Ok(self
            .carried_area_mm2
            .iter()
            .copied()
            .zip(self.tip_radii_mm.iter().copied())
            .filter(|(area, radius)| self.tip_policy.violates_safety_factor(*area, *radius))
            .count())
    }
}

/// The frozen tip-load policy.
///
/// `pub(crate)` so `crate::contact` can DELEGATE PR-4's area-sufficiency check
/// to [`FrozenTipLoadPolicy::violates_safety_factor`] rather than duplicate the
/// inequality.  Delegation inherits the 1e-6 safety-factor tolerance and makes
/// the contact check agree with the tip-sizing check at the `[0.2, 1.0]` clamp
/// boundaries by construction instead of by inspection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FrozenTipLoadPolicy {
    pressure_pa: f64,
    tensile_pa: f64,
    required_safety_factor: f64,
    min_radius_mm: f64,
    max_radius_mm: f64,
}

impl FrozenTipLoadPolicy {
    /// The area ratio this policy implies:
    /// `X = tensile_pa / (pressure_pa * safety_factor)`.
    ///
    /// This is an internal CONSISTENCY identity — a function of the
    /// Python-era `tip_safety_factor` = 8 — and NOT an independent measurement
    /// of the H2 plate's `X_ALLOWED` = 12.  Had the safety factor been 6 this
    /// would read 16.  What is notable is only that it does not EXCEED the
    /// measured allowance; that one-sided relationship is what the constants
    /// gate asserts.
    pub(crate) fn implied_area_ratio(self) -> f64 {
        if self.pressure_pa > 0.0 && self.required_safety_factor > 0.0 {
            self.tensile_pa / (self.pressure_pa * self.required_safety_factor)
        } else {
            0.0
        }
    }

    #[cfg(test)]
    pub(crate) fn for_contact_tests() -> Self {
        Self {
            pressure_pa: 3000.0,
            tensile_pa: 0.288 * 1.0e6,
            required_safety_factor: 8.0,
            min_radius_mm: 0.200,
            max_radius_mm: 1.0,
        }
    }

    pub(crate) fn from_placeholders(policy: &M0PlaceholderConstants) -> Self {
        Self {
            pressure_pa: policy.peel_pressure_pa().diagnostic().get(),
            tensile_pa: policy.tensile_green_mpa().diagnostic().get() * 1.0e6,
            required_safety_factor: policy.tip_safety_factor().diagnostic().get(),
            min_radius_mm: policy.tip_radius_default_mm().diagnostic().get(),
            max_radius_mm: policy.tip_radius_max_legacy_mm().diagnostic().get(),
        }
    }

    fn tip_radius_mm(self, area_mm2: f64) -> Result<PositiveFinite, GenericPerceptionError> {
        let force_n = area_mm2 * 1.0e-6 * self.pressure_pa;
        let radius_mm = (force_n * self.required_safety_factor
            / (std::f64::consts::PI * self.tensile_pa))
            .sqrt()
            * 1.0e3;
        PositiveFinite::new(radius_mm.clamp(self.min_radius_mm, self.max_radius_mm))
            .map_err(|_| GenericPerceptionError::NonFiniteFinalizedLoadTotal)
    }

    pub(crate) fn violates_safety_factor(self, area_mm2: f64, radius_mm: PositiveFinite) -> bool {
        let force_n = area_mm2 * 1.0e-6 * self.pressure_pa;
        let radius_m = radius_mm.get() / 1.0e3;
        let stress_pa = force_n / (std::f64::consts::PI * radius_m.max(1.0e-9).powi(2));
        let realized_sf = self.tensile_pa / stress_pa.max(1.0e-9);
        realized_sf < self.required_safety_factor - 1.0e-6
    }
}

fn finalized_load_rows_fingerprint(rows: &[FinalizedFaceLoad]) -> u64 {
    let mut hash = Fnv::new();
    hash.usize(rows.len());
    for row in rows {
        hash.u32(row.face_id);
        hash.u64(row.contact_id);
        hash_owner(&mut hash, row.owner);
        hash.f64(row.carried_area_mm2);
    }
    hash.finish()
}

fn finalized_contact_load_digest(
    authority: &AuthorityFingerprint,
    contacts_fingerprint: u64,
    rows_fingerprint: u64,
    tip_policy: FrozenTipLoadPolicy,
    contact_ids: &[u64],
    carried_area_mm2: &[f64],
    tip_radii_mm: &[PositiveFinite],
) -> u64 {
    let mut hash = Fnv::new();
    hash.bytes(authority.mesh.as_bytes());
    hash.u64(authority.native);
    hash.u64(authority.config);
    hash.u64(authority.frame);
    hash.u64(authority.layers);
    hash.u64(authority.specialized);
    hash.u64(contacts_fingerprint);
    hash.u64(rows_fingerprint);
    hash.f64(tip_policy.pressure_pa);
    hash.f64(tip_policy.tensile_pa);
    hash.f64(tip_policy.required_safety_factor);
    hash.f64(tip_policy.min_radius_mm);
    hash.f64(tip_policy.max_radius_mm);
    hash.usize(contact_ids.len());
    for ((contact_id, area), radius) in contact_ids.iter().zip(carried_area_mm2).zip(tip_radii_mm) {
        hash.u64(*contact_id);
        hash.f64(*area);
        hash.f64(radius.get());
    }
    hash.finish()
}

fn finalized_contact_load_required_bytes(
    contact_count: usize,
) -> Result<usize, GenericPerceptionError> {
    contact_count
        .checked_mul(std::mem::size_of::<u64>())
        .and_then(|bytes| {
            contact_count
                .checked_mul(std::mem::size_of::<(u64, usize)>())
                .and_then(|mapping| bytes.checked_add(mapping))
        })
        .and_then(|bytes| {
            contact_count
                .checked_mul(std::mem::size_of::<f64>())
                .and_then(|areas| bytes.checked_add(areas))
        })
        .and_then(|bytes| {
            contact_count
                .checked_mul(std::mem::size_of::<PositiveFinite>())
                .and_then(|radii| bytes.checked_add(radii))
        })
        .ok_or(GenericPerceptionError::LoadResourceOverflow)
}

/// One canonical solid boundary or hole at a real print layer.
#[derive(Clone, Debug, PartialEq)]
pub struct SliceLoop {
    points_xy_mm: Vec<[f64; 2]>,
    signed_area_mm2: f64,
    hole: bool,
}

impl SliceLoop {
    #[must_use]
    pub fn points_xy_mm(&self) -> &[[f64; 2]] {
        &self.points_xy_mm
    }
    #[must_use]
    pub const fn signed_area_mm2(&self) -> f64 {
        self.signed_area_mm2
    }
    #[must_use]
    pub const fn is_hole(&self) -> bool {
        self.hole
    }
}

/// One solid cross-section component with its owned void boundaries.
#[derive(Clone, Debug, PartialEq)]
pub struct SliceSolid {
    outer: SliceLoop,
    holes: Vec<SliceLoop>,
}

impl SliceSolid {
    #[must_use]
    pub const fn outer(&self) -> &SliceLoop {
        &self.outer
    }
    #[must_use]
    pub fn holes(&self) -> &[SliceLoop] {
        &self.holes
    }
    #[must_use]
    pub fn area_mm2(&self) -> f64 {
        self.outer.signed_area_mm2.abs()
            - self
                .holes
                .iter()
                .map(|hole| hole.signed_area_mm2.abs())
                .sum::<f64>()
    }
}

#[cfg(test)]
pub(crate) fn test_slice_solid(points_xy_mm: Vec<[f64; 2]>) -> SliceSolid {
    SliceSolid {
        outer: SliceLoop {
            signed_area_mm2: signed_area(&points_xy_mm),
            points_xy_mm,
            hole: false,
        },
        holes: Vec::new(),
    }
}

/// First printable appearance of one solid slice component.
#[derive(Clone, Debug, PartialEq)]
pub struct IslandBirth {
    pub layer_index: usize,
    pub z_mm: f64,
    pub centroid_xy_mm: [f64; 2],
    pub area_mm2: f64,
    footprint: SliceSolid,
}

impl IslandBirth {
    #[must_use]
    pub const fn footprint(&self) -> &SliceSolid {
        &self.footprint
    }
}

/// A welded surface-height minimum and its complete birth-band region.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalZMinimum {
    pub position_mm: [f64; 3],
    pub normal: [f64; 3],
    region_points_mm: Vec<[f64; 3]>,
}

impl LocalZMinimum {
    #[must_use]
    pub fn region_points_mm(&self) -> &[[f64; 3]] {
        &self.region_points_mm
    }
}

/// Opaque generic-perception authority.
#[derive(Clone, Debug, PartialEq)]
pub struct GenericContactPlan {
    authority: AuthorityFingerprint,
    islands: Vec<SupportIsland>,
    candidates: ContactCandidatePool,
    slice_layers: Vec<Vec<SliceSolid>>,
    island_births: Vec<IslandBirth>,
    local_z_minima: Vec<LocalZMinimum>,
    load_eligibility: Vec<LoadEligibility>,
    load_limits: LoadFinalizeLimits,
    slice_effective_threads: usize,
    slice_participating_threads: usize,
}

impl GenericContactPlan {
    #[must_use]
    pub fn islands(&self) -> &[SupportIsland] {
        &self.islands
    }
    #[must_use]
    pub fn candidates(&self) -> &ContactCandidatePool {
        &self.candidates
    }
    #[must_use]
    pub fn slice_layers(&self) -> &[Vec<SliceSolid>] {
        &self.slice_layers
    }
    #[must_use]
    pub fn island_births(&self) -> &[IslandBirth] {
        &self.island_births
    }
    #[must_use]
    pub fn local_z_minima(&self) -> &[LocalZMinimum] {
        &self.local_z_minima
    }
    #[must_use]
    pub const fn slice_thread_evidence(&self) -> (usize, usize) {
        (
            self.slice_effective_threads,
            self.slice_participating_threads,
        )
    }
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut hash = Fnv::new();
        hash.bytes(self.authority.mesh.as_bytes());
        hash.u64(self.authority.native);
        hash.u64(self.authority.config);
        hash.u64(self.authority.frame);
        hash.u64(self.authority.layers);
        hash.u64(self.authority.specialized);
        hash.u64(self.candidates.digest());
        for island in &self.islands {
            hash.u32(island.id);
            for &face in &island.face_ids {
                hash.u32(face);
            }
        }
        for birth in &self.island_births {
            hash.usize(birth.layer_index);
            hash.f64(birth.z_mm);
            hash.f64(birth.area_mm2);
            for point in birth.footprint.outer.points_xy_mm() {
                hash.f64(point[0]);
                hash.f64(point[1]);
            }
            for hole in &birth.footprint.holes {
                for point in hole.points_xy_mm() {
                    hash.f64(point[0]);
                    hash.f64(point[1]);
                }
            }
        }
        for minimum in &self.local_z_minima {
            for value in minimum.position_mm.iter().chain(minimum.normal.iter()) {
                hash.f64(*value);
            }
            for point in &minimum.region_points_mm {
                for value in point {
                    hash.f64(*value);
                }
            }
        }
        for layer in &self.slice_layers {
            for solid in layer {
                hash.f64(solid.area_mm2());
                for point in solid.outer.points_xy_mm() {
                    hash.f64(point[0]);
                    hash.f64(point[1]);
                }
                for hole in &solid.holes {
                    for point in hole.points_xy_mm() {
                        hash.f64(point[0]);
                        hash.f64(point[1]);
                    }
                }
            }
        }
        for row in &self.load_eligibility {
            hash.u32(row.face_id);
            for value in row.centroid_mm {
                hash.f64(value);
            }
            hash.f64(row.area_mm2);
            for owner in &row.eligible_owners {
                match owner {
                    LoadOwner::Specialized(id) => {
                        hash.u8(0);
                        hash.u32(*id);
                    }
                    LoadOwner::GenericIsland(id) => {
                        hash.u8(1);
                        hash.u32(*id);
                    }
                }
            }
        }
        hash.finish()
    }

    /// Reject replay against any different geometry, native setup, frame,
    /// result-changing configuration or layer schedule.
    #[allow(
        clippy::too_many_arguments,
        reason = "validation deliberately restates every independently stale-able authority input"
    )]
    pub fn validate_binding(
        &self,
        mesh: &Mesh,
        config: &SupportPerceptionConfig<'_>,
        frame: BuildFrame,
        layers: &LayerPlan,
        candidate_config: &CandidatePayloadConfig,
        specialized: &[SpecializedOwnerReservation],
        limits: GenericPerceptionLimits,
    ) -> Result<(), GenericPerceptionError> {
        if config.plate_z_mm != frame.plate_z_mm() {
            return Err(GenericPerceptionError::FrameMismatch);
        }
        let surface = build_surface(mesh, config)?;
        let current = authority_fingerprint(
            mesh,
            &surface,
            config,
            frame,
            layers,
            candidate_config,
            specialized,
            limits,
        )?;
        if current != self.authority {
            return Err(GenericPerceptionError::StaleAuthority);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AuthorityFingerprint {
    mesh: String,
    native: u64,
    config: u64,
    frame: u64,
    layers: u64,
    specialized: u64,
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum GenericPerceptionError {
    #[error(transparent)]
    Perception(#[from] SupportPerceptionError),
    #[error(transparent)]
    Dyadic(#[from] DyadicError),
    #[error("generic perception could not build topology: {0}")]
    Topology(String),
    #[error("generic perception could not build native surface: {0}")]
    Surface(String),
    #[error("generic perception could not query the island point index: {0}")]
    PointIndex(String),
    #[error("generic perception could not slice print layers: {0}")]
    Slice(String),
    #[error("slice layer {layer} contains an open or branched segment seam")]
    OpenSliceSeam { layer: usize },
    #[error("adequacy probe row {probe_row} contains an open or branched segment seam")]
    OpenAdequacyProbeSeam { probe_row: usize },
    #[error("slice loop vertex count {required} exceeds cap {limit}")]
    LoopVertexLimit { required: usize, limit: usize },
    #[error("welded minimum vertex count {required} exceeds cap {limit}")]
    MinimaVertexLimit { required: usize, limit: usize },
    #[error("specialized owner reservation names face {face_id} outside the mesh")]
    SpecializedFaceOutOfRange { face_id: u32 },
    #[error("specialized owner reservation duplicates owner {owner_id} on face {face_id}")]
    DuplicateSpecializedReservation { owner_id: u32, face_id: u32 },
    #[error("the layer schedule does not cover the support mesh z range")]
    LayerScheduleCoverage,
    #[error("support perception plate and build frame disagree")]
    FrameMismatch,
    #[error("island-neighbor query requires {required} output pairs; limit is {limit}")]
    IslandNeighborPairLimit { required: usize, limit: usize },
    #[error("island-neighbor query requires {required} output bytes; limit is {limit}")]
    IslandNeighborByteLimit { required: usize, limit: usize },
    #[error("slice overlap requires {required} candidate pairs; limit is {limit}")]
    OverlapPairLimit { required: usize, limit: usize },
    #[error("slice overlap requires {required} bytes; limit is {limit}")]
    OverlapByteLimit { required: usize, limit: usize },
    #[error("slice overlap requires at least {required} sampled vertex tests; limit is {limit}")]
    OverlapVertexTestLimit { required: usize, limit: usize },
    #[error("slice overlap requires at least {required} sampled edge tests; limit is {limit}")]
    OverlapEdgeTestLimit { required: usize, limit: usize },
    #[error("physical contact {contact_id} is non-finite")]
    NonFinitePhysicalContact { contact_id: u64 },
    #[error("physical contact id {contact_id} is duplicated")]
    DuplicatePhysicalContact { contact_id: u64 },
    #[error("support face {face_id} has no eligible physical contact")]
    MissingPhysicalContact { face_id: u32 },
    #[error("canonical physical contact id {contact_id} is duplicated")]
    DuplicateFinalizedContactId { contact_id: u64 },
    #[error("load partition names unknown physical contact id {contact_id}")]
    UnknownFinalizedLoadContact { contact_id: u64 },
    #[error("finalized contact-load fold requires {required} contacts; limit is {limit}")]
    FinalizedLoadContactLimit { required: usize, limit: usize },
    #[error("finalized contact-load fold requires {required} rows; limit is {limit}")]
    FinalizedLoadRowLimit { required: usize, limit: usize },
    #[error("finalized load row for physical contact {contact_id} is non-finite")]
    NonFiniteFinalizedLoad { contact_id: u64 },
    #[error("finalized load row for physical contact {contact_id} is negative")]
    NegativeFinalizedLoad { contact_id: u64 },
    #[error("finalized contact-load accumulation is non-finite")]
    NonFiniteFinalizedLoadTotal,
    #[error("load finalization scratch requires {required} bytes; limit is {limit}")]
    LoadScratchLimit { required: usize, limit: usize },
    #[error("load finalization requires {required} operations; limit is {limit}")]
    LoadOperationLimit { required: usize, limit: usize },
    #[error("load finalization resource arithmetic overflowed")]
    LoadResourceOverflow,
    #[error("generic perception produced an island without a legal dyadic candidate: {island_id}")]
    NoLegalCandidate { island_id: u32 },
    #[error("generic perception authority is stale or belongs to another input")]
    StaleAuthority,
}

/// Prepare the complete current generic evidence in batched native calls.
pub fn prepare_generic_contacts(
    mesh: &Mesh,
    config: &SupportPerceptionConfig<'_>,
    frame: BuildFrame,
    layers: &LayerPlan,
    candidate_config: &CandidatePayloadConfig,
    specialized: &[SpecializedOwnerReservation],
    limits: GenericPerceptionLimits,
) -> Result<GenericContactPlan, GenericPerceptionError> {
    if config.plate_z_mm != frame.plate_z_mm() {
        return Err(GenericPerceptionError::FrameMismatch);
    }
    let (topology, delicacy) = validate_perception_input(mesh, config)?;
    let surface = build_surface(mesh, config)?;
    let raw = perceive_contacts_on_surface(mesh, config, &topology, delicacy, &surface)?;
    let islands = connected_islands(&raw, &topology, &surface, limits)?;
    let payload = build_candidate_payload(mesh, &islands, candidate_config)?;
    let geometry = surface.geometry();
    let candidates = payload
        .rows()
        .iter()
        .enumerate()
        .map(|(row, candidate)| ContactCandidate {
            id: ContactCandidateId(row as u64),
            face_id: candidate.face_id,
            point_mm: candidate.point_mm,
            normal: candidate.normal,
            supported_area_mm2: geometry.areas()[candidate.face_id as usize],
            overhang_excess: -candidate.normal[2],
            delicacy: delicacy[candidate.face_id as usize],
        })
        .collect::<Vec<_>>();
    for island in &islands {
        if !candidates
            .iter()
            .any(|candidate| island.face_ids.binary_search(&candidate.face_id).is_ok())
        {
            return Err(GenericPerceptionError::NoLegalCandidate {
                island_id: island.id,
            });
        }
    }
    let candidates = ContactCandidatePool { candidates };

    let (layer_indices, planes) = layer_midplanes(mesh, layers)?;
    let sliced = surface
        .slice_z_batch(
            &planes,
            PlaneSliceLimits {
                max_planes: limits.max_planes,
                max_triangle_plane_tests: limits.max_triangle_plane_tests,
                max_intersections: limits.max_slice_intersections,
                max_output_bytes: limits.max_slice_output_bytes,
            },
        )
        .map_err(|error| GenericPerceptionError::Slice(error.to_string()))?;
    let mut loop_vertices = 0usize;
    let mut slice_layers = Vec::with_capacity(planes.len());
    for layer in 0..planes.len() {
        let loops = stitch_layer(SliceStitchSite::PrintLayer(layer), sliced.plane(layer))?;
        loop_vertices = loop_vertices
            .checked_add(
                loops
                    .iter()
                    .map(|solid| {
                        solid.outer.points_xy_mm.len()
                            + solid
                                .holes
                                .iter()
                                .map(|hole| hole.points_xy_mm.len())
                                .sum::<usize>()
                    })
                    .sum::<usize>(),
            )
            .ok_or(GenericPerceptionError::LoopVertexLimit {
                required: usize::MAX,
                limit: limits.max_loop_vertices,
            })?;
        if loop_vertices > limits.max_loop_vertices {
            return Err(GenericPerceptionError::LoopVertexLimit {
                required: loop_vertices,
                limit: limits.max_loop_vertices,
            });
        }
        slice_layers.push(loops);
    }
    let island_births = find_births(&layer_indices, &planes, &slice_layers, limits)?;
    let local_z_minima = find_local_minima(mesh, &surface, layers, &island_births, limits)?;
    let load_eligibility = build_load_eligibility(mesh, &surface, &islands, specialized)?;
    let authority = authority_fingerprint(
        mesh,
        &surface,
        config,
        frame,
        layers,
        candidate_config,
        specialized,
        limits,
    )?;
    Ok(GenericContactPlan {
        authority,
        islands,
        candidates,
        slice_layers,
        island_births,
        local_z_minima,
        load_eligibility,
        load_limits: LoadFinalizeLimits {
            max_scratch_bytes: limits.max_overlap_bytes,
            // The generic perception plan carries its own flat overlap budget,
            // not the specialized batch-product rule: a coefficient of 0 makes
            // `operations_limit` return this floor for every batch shape, which
            // is exactly the bound this stage had before the rule existed.
            max_operations_floor: limits
                .max_overlap_vertex_tests
                .checked_add(limits.max_overlap_edge_tests)
                .ok_or(GenericPerceptionError::LoadResourceOverflow)?,
            operations_per_batch_unit: 0,
        },
        slice_effective_threads: sliced.effective_threads,
        slice_participating_threads: sliced.participating_threads,
    })
}

fn build_surface(
    mesh: &Mesh,
    config: &SupportPerceptionConfig<'_>,
) -> Result<NativeSurface, GenericPerceptionError> {
    NativeSurface::from_indexed(
        mesh.verts(),
        mesh.tris(),
        NativeSurfaceConfig {
            grid_pitch_mm: config.surface_grid_pitch_mm.get(),
            max_grid_bytes: config.max_grid_bytes,
        },
    )
    .map_err(|error| GenericPerceptionError::Surface(error.to_string()))
}

fn connected_islands(
    raw: &ContactCandidatePool,
    topology: &FaceTopology,
    surface: &NativeSurface,
    limits: GenericPerceptionLimits,
) -> Result<Vec<SupportIsland>, GenericPerceptionError> {
    let mut unseen = raw
        .candidates()
        .iter()
        .map(|candidate| candidate.face_id as usize)
        .collect::<BTreeSet<_>>();
    if unseen.is_empty() {
        return Ok(Vec::new());
    }
    let radius = plinth_core::m0_constants()
        .map_err(|error| GenericPerceptionError::Topology(error.to_string()))?
        .placeholders()
        .island_merge_radius_mm()
        .diagnostic()
        .get();
    let centers = surface.geometry().centroids();
    let face_ids = unseen.iter().copied().collect::<Vec<_>>();
    let points = face_ids
        .iter()
        .map(|face| centers[*face])
        .collect::<Vec<_>>();
    let index = PointCloudIndex::new(&points)
        .map_err(|error| GenericPerceptionError::PointIndex(error.to_string()))?;
    // Gather every island-closing query, invoke the maintained spatial index
    // once, then scatter the CSR into source-face adjacency.
    let nearby = index
        .radius_neighbors_batch(
            &points,
            radius,
            RadiusQueryLimits {
                max_output_pairs: limits.max_island_neighbor_pairs,
                max_output_bytes: limits.max_island_neighbor_bytes,
            },
        )
        .map_err(|error| match error {
            PointQueryError::RadiusOutputPairLimit { required, limit } => {
                GenericPerceptionError::IslandNeighborPairLimit { required, limit }
            }
            PointQueryError::RadiusOutputByteLimit { required, limit } => {
                GenericPerceptionError::IslandNeighborByteLimit { required, limit }
            }
            other => GenericPerceptionError::PointIndex(other.to_string()),
        })?;
    let mut spatial = BTreeMap::<usize, Vec<usize>>::new();
    for (row, &face) in face_ids.iter().enumerate() {
        let range = nearby
            .row_range(row)
            .expect("one point-index row exists per gathered face");
        spatial.insert(
            face,
            nearby.point_ids[range]
                .iter()
                .map(|point_id| face_ids[*point_id as usize])
                .filter(|other| *other != face)
                .collect(),
        );
    }
    let mut groups = Vec::new();
    while let Some(&start) = unseen.first() {
        unseen.remove(&start);
        let mut queue = VecDeque::from([start]);
        let mut faces = Vec::new();
        while let Some(face) = queue.pop_front() {
            faces.push(face as u32);
            let mut neighbors = topology
                .neighbors(face)
                .unwrap_or(&[])
                .iter()
                .map(|value| *value as usize)
                .filter(|value| unseen.contains(value))
                .collect::<Vec<_>>();
            neighbors.extend(
                spatial[&face]
                    .iter()
                    .copied()
                    .filter(|other| unseen.contains(other)),
            );
            neighbors.sort_unstable();
            neighbors.dedup();
            for neighbor in neighbors {
                if unseen.remove(&neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }
        faces.sort_unstable();
        groups.push(faces);
    }
    groups.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    Ok(groups
        .into_iter()
        .enumerate()
        .map(|(id, face_ids)| SupportIsland {
            id: id as u32,
            face_ids,
        })
        .collect())
}

fn layer_midplanes(
    mesh: &Mesh,
    layers: &LayerPlan,
) -> Result<(Vec<usize>, Vec<f64>), GenericPerceptionError> {
    let zmin = mesh
        .verts()
        .iter()
        .map(|point| point[2])
        .fold(f64::INFINITY, f64::min);
    let zmax = mesh
        .verts()
        .iter()
        .map(|point| point[2])
        .fold(f64::NEG_INFINITY, f64::max);
    if layers.count() == 0
        || layers.z0_mm() > zmin
        || layers.z_of(layers.count() - 1).unwrap_or(f64::NEG_INFINITY)
            + layers.heights_mm()[layers.count() - 1]
            < zmax
    {
        return Err(GenericPerceptionError::LayerScheduleCoverage);
    }
    let indexed = (0..layers.count())
        .filter_map(|index| {
            let z = layers.z_of(index)? + layers.heights_mm()[index] * 0.5;
            (z >= zmin && z < zmax).then_some((index, z))
        })
        .collect::<Vec<_>>();
    Ok(indexed.into_iter().unzip())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PointKey(i64, i64);

fn point_key(point: [f64; 2]) -> PointKey {
    PointKey(
        (point[0] / LOOP_MERGE_MM).round() as i64,
        (point[1] / LOOP_MERGE_MM).round() as i64,
    )
}

/// Which caller a `stitch_layer` invocation belongs to.
///
/// The two call sites hand `stitch_layer` indices from different spaces: a
/// print-layer index and an adequacy-probe row. Carrying the space with the
/// index keeps a raised seam error from claiming a probe row is a layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SliceStitchSite {
    PrintLayer(usize),
    AdequacyProbe(usize),
}

impl SliceStitchSite {
    fn open_seam(self) -> GenericPerceptionError {
        match self {
            Self::PrintLayer(layer) => GenericPerceptionError::OpenSliceSeam { layer },
            Self::AdequacyProbe(probe_row) => {
                GenericPerceptionError::OpenAdequacyProbeSeam { probe_row }
            }
        }
    }
}

fn stitch_layer(
    site: SliceStitchSite,
    intersections: &[geom_kernel::PlaneIntersection],
) -> Result<Vec<SliceSolid>, GenericPerceptionError> {
    let mut points = BTreeMap::<PointKey, [f64; 2]>::new();
    let mut edges = BTreeSet::<(PointKey, PointKey)>::new();
    for event in intersections {
        let a = [event.segment[0][0], event.segment[0][1]];
        let b = [event.segment[1][0], event.segment[1][1]];
        let ka = point_key(a);
        let kb = point_key(b);
        if ka == kb {
            continue;
        }
        points.entry(ka).or_insert(a);
        points.entry(kb).or_insert(b);
        edges.insert(if ka < kb { (ka, kb) } else { (kb, ka) });
    }
    let mut adjacency = BTreeMap::<PointKey, Vec<PointKey>>::new();
    for &(a, b) in &edges {
        adjacency.entry(a).or_default().push(b);
        adjacency.entry(b).or_default().push(a);
    }
    if adjacency.values().any(|neighbors| neighbors.len() != 2) {
        return Err(site.open_seam());
    }
    for neighbors in adjacency.values_mut() {
        neighbors.sort_unstable();
    }
    let mut unused = edges;
    let mut raw = Vec::<Vec<[f64; 2]>>::new();
    while let Some(&(start, next)) = unused.first() {
        let mut keys = vec![start];
        let mut previous = start;
        let mut current = next;
        unused.remove(&(start, next));
        while current != start {
            keys.push(current);
            let neighbors = &adjacency[&current];
            let following = if neighbors[0] == previous {
                neighbors[1]
            } else {
                neighbors[0]
            };
            let edge = if current < following {
                (current, following)
            } else {
                (following, current)
            };
            if !unused.remove(&edge) {
                return Err(site.open_seam());
            }
            previous = current;
            current = following;
        }
        raw.push(keys.into_iter().map(|key| points[&key]).collect());
    }
    // Sub-threshold loops are dropped BEFORE the parity count, not after it.
    // Filtering afterwards let a loop contribute to every other loop's nesting
    // depth and then vanish from the set hole ownership searches, so depth and
    // ownership were decided against two different loop sets - a second way to
    // manufacture a hole no solid can claim.
    let kept = raw
        .into_iter()
        .filter(|points_xy| signed_area(points_xy).abs() >= BIRTH_MIN_AREA_MM2)
        .collect::<Vec<_>>();
    let mut loops = Vec::with_capacity(kept.len());
    for (index, points_xy) in kept.iter().enumerate() {
        // Still the loop's first vertex, and deliberately so: a strictly
        // interior probe is the better containment query (see
        // `interior_probe`), but swapping it in here re-classifies loops on
        // clean models too and moves frozen digests. Widening the query is
        // therefore confined to hole ownership, where it can only rescue a
        // hole the old code refused outright. Normalizing parity itself waits
        // on the fill-rule work.
        let probe = points_xy[0];
        let depth = kept
            .iter()
            .enumerate()
            .filter(|(other, polygon)| *other != index && point_in_polygon(probe, polygon))
            .count();
        let hole = depth % 2 == 1;
        let mut canonical = canonicalize_loop(points_xy.clone(), hole);
        let area = signed_area(&canonical);
        let desired_positive = !hole;
        if (area > 0.0) != desired_positive {
            canonical.reverse();
            canonical = canonicalize_loop(canonical, hole);
        }
        loops.push(SliceLoop {
            signed_area_mm2: signed_area(&canonical),
            points_xy_mm: canonical,
            hole,
        });
    }
    loops.sort_by(|a, b| {
        a.hole
            .cmp(&b.hole)
            .then_with(|| b.signed_area_mm2.abs().total_cmp(&a.signed_area_mm2.abs()))
            .then_with(|| point_key(a.points_xy_mm[0]).cmp(&point_key(b.points_xy_mm[0])))
    });
    let mut solids = loops
        .iter()
        .filter(|loop_| !loop_.hole)
        .cloned()
        .map(|outer| SliceSolid {
            outer,
            holes: Vec::new(),
        })
        .collect::<Vec<_>>();
    for hole in loops.into_iter().filter(|loop_| loop_.hole) {
        let Some(owner) = hole_owner(&solids, &hole) else {
            // A hole nothing can claim sits inside another void: every loop
            // containing it was itself classified a hole. Dropping it leaves
            // the region void, which is exactly what even-odd parity already
            // says it is, so the only cost is a bounded, local over-report of
            // solid area. That is not worth aborting a whole support run for.
            continue;
        };
        solids[owner].holes.push(hole);
    }
    for solid in &mut solids {
        solid.holes.sort_by(|left, right| {
            right
                .signed_area_mm2
                .abs()
                .total_cmp(&left.signed_area_mm2.abs())
                .then_with(|| {
                    point_key(left.points_xy_mm[0]).cmp(&point_key(right.points_xy_mm[0]))
                })
        });
    }
    Ok(solids)
}

/// Assign `hole` to the smallest solid that can claim it.
///
/// A sculpt whose sub-objects interpenetrate slices into contours that cross
/// themselves, and "inside" is undefined for a self-crossing polygon. Even-odd
/// containment stops being transitive there: a loop can be inside a hole that
/// is inside a solid, yet test outside that solid. So widen the question
/// instead of aborting, and let the caller drop what still has no owner.
///
/// The first step is the query this function has always run, so a hole the old
/// code could place is placed identically. Every later step only ever runs
/// where the old code raised `OpenSliceSeam` and gave up, which is what makes
/// the whole chain inert on any model that already sliced.
fn hole_owner(solids: &[SliceSolid], hole: &SliceLoop) -> Option<usize> {
    let &vertex = hole.points_xy_mm.first()?;
    if let Some(index) = smallest_solid(solids, |solid| {
        point_in_polygon(vertex, solid.outer.points_xy_mm())
    }) {
        return Some(index);
    }
    let probe = interior_probe(&hole.points_xy_mm);
    if let Some(index) = smallest_solid(solids, |solid| {
        point_in_polygon(probe, solid.outer.points_xy_mm())
    }) {
        return Some(index);
    }
    if let Some(index) = smallest_solid(solids, |solid| {
        hole.points_xy_mm
            .iter()
            .any(|point| point_in_polygon(*point, solid.outer.points_xy_mm()))
    }) {
        return Some(index);
    }
    let bounds = bounding_box(&hole.points_xy_mm)?;
    smallest_solid(solids, |solid| {
        bounding_box(solid.outer.points_xy_mm()).is_some_and(|outer| box_contains(outer, bounds))
    })
}

/// The smallest-area solid satisfying `claims`, ties broken by slice order.
fn smallest_solid(
    solids: &[SliceSolid],
    mut claims: impl FnMut(&SliceSolid) -> bool,
) -> Option<usize> {
    solids
        .iter()
        .enumerate()
        .filter(|(_, solid)| claims(solid))
        .min_by(|(_, left), (_, right)| {
            left.outer
                .signed_area_mm2
                .abs()
                .total_cmp(&right.outer.signed_area_mm2.abs())
        })
        .map(|(index, _)| index)
}

/// `[min_x, min_y, max_x, max_y]` over `points`.
fn bounding_box(points: &[[f64; 2]]) -> Option<[f64; 4]> {
    let first = points.first()?;
    Some(
        points
            .iter()
            .fold([first[0], first[1], first[0], first[1]], |bounds, point| {
                [
                    bounds[0].min(point[0]),
                    bounds[1].min(point[1]),
                    bounds[2].max(point[0]),
                    bounds[3].max(point[1]),
                ]
            }),
    )
}

const fn box_contains(outer: [f64; 4], inner: [f64; 4]) -> bool {
    outer[0] <= inner[0] && outer[1] <= inner[1] && outer[2] >= inner[2] && outer[3] >= inner[3]
}

/// How many of a loop's longest edges may seed an interior probe.
const INTERIOR_PROBE_EDGES: usize = 8;
/// Inward offsets, as a fraction of the seeding edge's length, tried in order.
const INTERIOR_PROBE_OFFSETS: [f64; 5] = [0.5, 0.125, 0.031_25, 1.0e-3, 1.0e-5];

/// A point strictly inside `polygon` under the even-odd rule.
///
/// Anchoring a containment query on a contour vertex is boundary-degenerate:
/// the vertex lies exactly ON its own loop, and in a tangle it can sit under a
/// micron from a neighbouring loop's boundary, where `point_in_polygon` has no
/// defined answer. Prefer the area centroid, fall back to inward offsets from
/// the longest edges, and only then to the vertex the query used to use.
///
/// Only `hole_owner` calls this, and only after the legacy vertex query has
/// already failed. Nesting parity still runs on the vertex: promoting this
/// probe there re-decides hole-versus-solid on clean models as well and is
/// gated behind the fill-rule normalization work.
fn interior_probe(polygon: &[[f64; 2]]) -> [f64; 2] {
    let fallback = polygon.first().copied().unwrap_or([0.0, 0.0]);
    if polygon.len() < 3 {
        return fallback;
    }
    let area = signed_area(polygon);
    if let Some(centroid) = area_centroid(polygon, area) {
        if point_in_polygon(centroid, polygon) {
            return centroid;
        }
    }
    // Interior lies to the left of `a -> b` exactly when the loop winds
    // counter-clockwise, i.e. when its signed area is positive.
    let inward = if area < 0.0 { -1.0 } else { 1.0 };
    let mut edges = (0..polygon.len())
        .map(|index| {
            let a = polygon[index];
            let b = polygon[(index + 1) % polygon.len()];
            (index, (b[0] - a[0]).hypot(b[1] - a[1]))
        })
        .collect::<Vec<_>>();
    edges.sort_by(|(left_index, left_length), (right_index, right_length)| {
        right_length
            .total_cmp(left_length)
            .then(left_index.cmp(right_index))
    });
    for &(index, length) in edges.iter().take(INTERIOR_PROBE_EDGES) {
        if !(length.is_finite() && length > 0.0) {
            break;
        }
        let a = polygon[index];
        let b = polygon[(index + 1) % polygon.len()];
        let midpoint = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        let normal = [
            -inward * (b[1] - a[1]) / length,
            inward * (b[0] - a[0]) / length,
        ];
        for offset in INTERIOR_PROBE_OFFSETS {
            let step = offset * length;
            let candidate = [
                midpoint[0] + normal[0] * step,
                midpoint[1] + normal[1] * step,
            ];
            if point_in_polygon(candidate, polygon) {
                return candidate;
            }
        }
    }
    fallback
}

/// The area centroid of `polygon`, or `None` when it is degenerate.
fn area_centroid(polygon: &[[f64; 2]], area: f64) -> Option<[f64; 2]> {
    if area == 0.0 || !area.is_finite() {
        return None;
    }
    let mut moment = [0.0f64; 2];
    for index in 0..polygon.len() {
        let a = polygon[index];
        let b = polygon[(index + 1) % polygon.len()];
        let cross = a[0] * b[1] - b[0] * a[1];
        moment[0] += (a[0] + b[0]) * cross;
        moment[1] += (a[1] + b[1]) * cross;
    }
    let scale = 1.0 / (6.0 * area);
    let centroid = [moment[0] * scale, moment[1] * scale];
    (centroid[0].is_finite() && centroid[1].is_finite()).then_some(centroid)
}

fn canonicalize_loop(mut points: Vec<[f64; 2]>, _hole: bool) -> Vec<[f64; 2]> {
    let first = points
        .iter()
        .enumerate()
        .min_by_key(|(_, point)| point_key(**point))
        .map(|(index, _)| index)
        .unwrap_or(0);
    points.rotate_left(first);
    points
}

fn signed_area(points: &[[f64; 2]]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    (0..points.len())
        .map(|i| {
            let next = (i + 1) % points.len();
            points[i][0] * points[next][1] - points[next][0] * points[i][1]
        })
        .sum::<f64>()
        * 0.5
}

fn point_in_polygon(point: [f64; 2], polygon: &[[f64; 2]]) -> bool {
    let mut inside = false;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        if (a[1] > point[1]) != (b[1] > point[1])
            && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
    }
    inside
}

fn segments_intersect(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let ccw = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        (r[1] - p[1]) * (q[0] - p[0]) > (q[1] - p[1]) * (r[0] - p[0])
    };
    ccw(a, c, d) != ccw(b, c, d) && ccw(a, b, c) != ccw(a, b, d)
}

fn find_births(
    layer_indices: &[usize],
    planes: &[f64],
    layers: &[Vec<SliceSolid>],
    limits: GenericPerceptionLimits,
) -> Result<Vec<IslandBirth>, GenericPerceptionError> {
    let mut births = Vec::new();
    let mut previous: &[SliceSolid] = &[];
    let mut operation_budget = OverlapOperationBudget::new(limits);
    for (layer_index, layer) in layers.iter().enumerate() {
        let overlaps = overlap_candidates(
            layer,
            previous,
            limits.max_overlap_pairs,
            limits.max_overlap_bytes,
        )?;
        for (current_index, solid) in layer.iter().enumerate() {
            let mut overlapped = false;
            if let Some(prior) = overlaps.get(&current_index) {
                for index in prior {
                    if solids_overlap(solid, &previous[*index], &mut operation_budget)? {
                        overlapped = true;
                        break;
                    }
                }
            }
            if !overlapped {
                let count = solid.outer.points_xy_mm.len() as f64;
                let centroid = [
                    solid
                        .outer
                        .points_xy_mm
                        .iter()
                        .map(|point| point[0])
                        .sum::<f64>()
                        / count,
                    solid
                        .outer
                        .points_xy_mm
                        .iter()
                        .map(|point| point[1])
                        .sum::<f64>()
                        / count,
                ];
                births.push(IslandBirth {
                    layer_index: layer_indices[layer_index],
                    z_mm: planes[layer_index],
                    centroid_xy_mm: centroid,
                    area_mm2: solid.area_mm2(),
                    footprint: solid.clone(),
                });
            }
        }
        previous = layer;
    }
    Ok(births)
}

fn overlap_candidates(
    current: &[SliceSolid],
    previous: &[SliceSolid],
    max_pairs: usize,
    max_bytes: usize,
) -> Result<BTreeMap<usize, Vec<usize>>, GenericPerceptionError> {
    if current.is_empty() || previous.is_empty() {
        return Ok(BTreeMap::new());
    }
    let upper = current.len().checked_mul(previous.len()).ok_or(
        GenericPerceptionError::OverlapPairLimit {
            required: usize::MAX,
            limit: max_pairs,
        },
    )?;
    if upper > max_pairs {
        return Err(GenericPerceptionError::OverlapPairLimit {
            required: upper,
            limit: max_pairs,
        });
    }
    let bytes = upper
        .checked_mul(2 * std::mem::size_of::<usize>() + std::mem::size_of::<f64>())
        .ok_or(GenericPerceptionError::OverlapByteLimit {
            required: usize::MAX,
            limit: max_bytes,
        })?;
    if bytes > max_bytes {
        return Err(GenericPerceptionError::OverlapByteLimit {
            required: bytes,
            limit: max_bytes,
        });
    }
    let bbox = |solid: &SliceSolid| {
        let xs = solid.outer.points_xy_mm.iter().map(|point| point[0]);
        let ys = solid.outer.points_xy_mm.iter().map(|point| point[1]);
        let min_x = xs.clone().fold(f64::INFINITY, f64::min);
        let max_x = xs.fold(f64::NEG_INFINITY, f64::max);
        let min_y = ys.clone().fold(f64::INFINITY, f64::min);
        let max_y = ys.fold(f64::NEG_INFINITY, f64::max);
        (
            [0.5 * (min_x + max_x), 0.5 * (min_y + max_y), 0.0],
            0.5 * ((max_x - min_x).hypot(max_y - min_y)),
        )
    };
    let previous_boxes = previous.iter().map(bbox).collect::<Vec<_>>();
    let current_boxes = current.iter().map(bbox).collect::<Vec<_>>();
    let radius = previous_boxes
        .iter()
        .map(|(_, radius)| *radius)
        .fold(0.0, f64::max)
        + current_boxes
            .iter()
            .map(|(_, radius)| *radius)
            .fold(0.0, f64::max);
    let index = PointCloudIndex::new(
        &previous_boxes
            .iter()
            .map(|(center, _)| *center)
            .collect::<Vec<_>>(),
    )
    .map_err(|error| GenericPerceptionError::PointIndex(error.to_string()))?;
    let nearby = index
        .radius_neighbors_batch(
            &current_boxes
                .iter()
                .map(|(center, _)| *center)
                .collect::<Vec<_>>(),
            radius,
            RadiusQueryLimits {
                max_output_pairs: upper,
                max_output_bytes: current
                    .len()
                    .checked_add(1)
                    .and_then(|rows| rows.checked_mul(std::mem::size_of::<usize>()))
                    .and_then(|offsets| {
                        upper
                            .checked_mul(std::mem::size_of::<u64>() + std::mem::size_of::<f64>())
                            .and_then(|pairs| offsets.checked_add(pairs))
                    })
                    .unwrap_or(usize::MAX),
            },
        )
        .map_err(|error| GenericPerceptionError::PointIndex(error.to_string()))?;
    if nearby.point_ids.len() > max_pairs {
        return Err(GenericPerceptionError::OverlapPairLimit {
            required: nearby.point_ids.len(),
            limit: max_pairs,
        });
    }
    Ok((0..current.len())
        .map(|row| {
            let range = nearby.row_range(row).expect("one row per current solid");
            (
                row,
                nearby.point_ids[range]
                    .iter()
                    .map(|id| *id as usize)
                    .collect(),
            )
        })
        .collect())
}

fn solid_contains(solid: &SliceSolid, point: [f64; 2]) -> bool {
    point_in_polygon(point, solid.outer.points_xy_mm())
        && !solid
            .holes
            .iter()
            .any(|hole| point_in_polygon(point, hole.points_xy_mm()))
}

struct OverlapOperationBudget {
    vertex_tests: usize,
    edge_tests: usize,
    max_vertex_tests: usize,
    max_edge_tests: usize,
}

impl OverlapOperationBudget {
    fn new(limits: GenericPerceptionLimits) -> Self {
        Self {
            vertex_tests: 0,
            edge_tests: 0,
            max_vertex_tests: limits.max_overlap_vertex_tests,
            max_edge_tests: limits.max_overlap_edge_tests,
        }
    }

    fn vertex(&mut self) -> Result<(), GenericPerceptionError> {
        self.vertex_tests = self.vertex_tests.saturating_add(1);
        if self.vertex_tests > self.max_vertex_tests {
            return Err(GenericPerceptionError::OverlapVertexTestLimit {
                required: self.vertex_tests,
                limit: self.max_vertex_tests,
            });
        }
        Ok(())
    }

    fn edge(&mut self) -> Result<(), GenericPerceptionError> {
        self.edge_tests = self.edge_tests.saturating_add(1);
        if self.edge_tests > self.max_edge_tests {
            return Err(GenericPerceptionError::OverlapEdgeTestLimit {
                required: self.edge_tests,
                limit: self.max_edge_tests,
            });
        }
        Ok(())
    }
}

fn solid_bbox(solid: &SliceSolid) -> [f64; 4] {
    let mut bbox = [
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for point in &solid.outer.points_xy_mm {
        bbox[0] = bbox[0].min(point[0]);
        bbox[1] = bbox[1].max(point[0]);
        bbox[2] = bbox[2].min(point[1]);
        bbox[3] = bbox[3].max(point[1]);
    }
    bbox
}

fn solids_overlap(
    a: &SliceSolid,
    b: &SliceSolid,
    budget: &mut OverlapOperationBudget,
) -> Result<bool, GenericPerceptionError> {
    let aa = solid_bbox(a);
    let bb = solid_bbox(b);
    if aa[1] < bb[0] || bb[1] < aa[0] || aa[3] < bb[2] || bb[3] < aa[2] {
        return Ok(false);
    }
    for (source, target) in [(&a.outer, &b.outer), (&b.outer, &a.outer)] {
        let step = (source.points_xy_mm.len() / 40).max(1);
        for index in (0..source.points_xy_mm.len()).step_by(step) {
            budget.vertex()?;
            if point_in_polygon(source.points_xy_mm[index], target.points_xy_mm()) {
                return Ok(true);
            }
        }
    }
    let left = &a.outer;
    let right = &b.outer;
    let left_step = (left.points_xy_mm.len() / 60).max(1);
    let right_step = (right.points_xy_mm.len() / 60).max(1);
    for i in (0..left.points_xy_mm.len()).step_by(left_step) {
        for j in (0..right.points_xy_mm.len()).step_by(right_step) {
            budget.edge()?;
            if segments_intersect(
                left.points_xy_mm[i],
                left.points_xy_mm[(i + left_step) % left.points_xy_mm.len()],
                right.points_xy_mm[j],
                right.points_xy_mm[(j + right_step) % right.points_xy_mm.len()],
            ) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn find_local_minima(
    mesh: &Mesh,
    surface: &NativeSurface,
    layers: &LayerPlan,
    births: &[IslandBirth],
    limits: GenericPerceptionLimits,
) -> Result<Vec<LocalZMinimum>, GenericPerceptionError> {
    let welded = WeldedView::build(mesh, WeldPolicy::ExactBits)
        .map_err(|error| GenericPerceptionError::Topology(error.to_string()))?;
    if welded.verts().len() > limits.max_minima_vertices {
        return Err(GenericPerceptionError::MinimaVertexLimit {
            required: welded.verts().len(),
            limit: limits.max_minima_vertices,
        });
    }
    let mut adjacency = vec![BTreeSet::<usize>::new(); welded.verts().len()];
    let mut incident = vec![Vec::<usize>::new(); welded.verts().len()];
    for (face, triangle) in mesh.tris().iter().enumerate() {
        let ids = triangle.map(|id| welded.map()[id as usize] as usize);
        for &(a, b) in &[(ids[0], ids[1]), (ids[1], ids[2]), (ids[2], ids[0])] {
            adjacency[a].insert(b);
            adjacency[b].insert(a);
        }
        for id in ids {
            incident[id].push(face);
        }
    }
    let constants = plinth_core::m0_constants()
        .map_err(|error| GenericPerceptionError::Topology(error.to_string()))?;
    let eps = constants.placeholders().minima_eps_mm().diagnostic().get();
    let connect_layers = constants
        .placeholders()
        .minima_connect_layers()
        .diagnostic()
        .get()
        .ceil() as usize;
    let normals = surface.geometry().normals();
    let mut candidates = (0..welded.verts().len())
        .filter(|&vertex| {
            adjacency[vertex]
                .iter()
                .all(|neighbor| welded.verts()[vertex][2] <= welded.verts()[*neighbor][2] + eps)
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| {
        welded.verts()[*a][2]
            .total_cmp(&welded.verts()[*b][2])
            .then_with(|| a.cmp(b))
    });
    let mut globally_visited = vec![false; welded.verts().len()];
    let mut minima = Vec::new();
    for start in candidates {
        if globally_visited[start] {
            continue;
        }
        let z = welded.verts()[start][2];
        let layer_index = layers.index_at(z).unwrap_or(0);
        let tolerance = layers.heights_mm()[layer_index..]
            .iter()
            .take(connect_layers)
            .sum::<f64>();
        let mut stack = vec![start];
        let mut seen = BTreeSet::from([start]);
        let mut local = vec![start];
        let mut reaches_lower = false;
        'flood: while let Some(vertex) = stack.pop() {
            for &neighbor in &adjacency[vertex] {
                if seen.contains(&neighbor) || welded.verts()[neighbor][2] > z + tolerance {
                    continue;
                }
                let nz = welded.verts()[neighbor][2];
                if nz < z - eps {
                    reaches_lower = true;
                    break 'flood;
                }
                seen.insert(neighbor);
                local.push(neighbor);
                stack.push(neighbor);
            }
        }
        for &vertex in &local {
            globally_visited[vertex] = true;
        }
        if reaches_lower {
            continue;
        }
        let representative = start;
        let mut normal = [0.0; 3];
        for &face in &incident[representative] {
            let source = mesh.tris()[face];
            let ids = source.map(|id| welded.map()[id as usize] as usize);
            let corner = ids
                .iter()
                .position(|id| *id == representative)
                .expect("incident face contains welded vertex");
            let p = welded.verts()[representative];
            let a = welded.verts()[ids[(corner + 1) % 3]];
            let b = welded.verts()[ids[(corner + 2) % 3]];
            let va = [a[0] - p[0], a[1] - p[1], a[2] - p[2]];
            let vb = [b[0] - p[0], b[1] - p[1], b[2] - p[2]];
            let la = (va[0] * va[0] + va[1] * va[1] + va[2] * va[2]).sqrt();
            let lb = (vb[0] * vb[0] + vb[1] * vb[1] + vb[2] * vb[2]).sqrt();
            let cosine =
                ((va[0] * vb[0] + va[1] * vb[1] + va[2] * vb[2]) / (la * lb)).clamp(-1.0, 1.0);
            let angle = cosine.acos();
            for (axis, value) in normal.iter_mut().enumerate() {
                *value += angle * normals[face][axis];
            }
        }
        let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        if length > 0.0 {
            for value in &mut normal {
                *value /= length;
            }
        }
        minima.push(LocalZMinimum {
            position_mm: welded.verts()[representative],
            normal,
            region_points_mm: local.iter().map(|id| welded.verts()[*id]).collect(),
        });
    }
    // Independent filter A: nearest face, queried once for every minimum.
    minima = filter_minima_to_exterior_faces(
        surface.geometry().centroids(),
        surface.exterior_face_mask(),
        minima,
        limits,
    )?;

    // Independent filter B: keep a true birth, or an inadequately carried
    // non-birth. The adequacy probes are gathered and sliced in one batch.
    let birth_match_layers = constants
        .placeholders()
        .minima_birth_match_layers()
        .diagnostic()
        .get()
        .ceil() as usize;
    let is_birth = minima
        .iter()
        .map(|minimum| {
            let z_tolerance = layer_tolerance(layers, minimum.position_mm[2], birth_match_layers);
            births.iter().any(|birth| {
                (birth.z_mm - minimum.position_mm[2]).abs() <= z_tolerance
                    && sampled_region_points(minimum)
                        .iter()
                        .any(|point| birth_xy_ok(&birth.footprint, [point[0], point[1]], 0.2))
            })
        })
        .collect::<Vec<_>>();
    let suspects = minima
        .iter()
        .enumerate()
        .filter(|(index, _)| !is_birth[*index])
        .collect::<Vec<_>>();
    let probe_heights = suspects
        .iter()
        .map(|(_, minimum)| {
            let index = layers
                .index_at(minimum.position_mm[2])
                .expect("validated layer schedule covers every minimum");
            minimum.position_mm[2] - layers.heights_mm()[index]
        })
        .collect::<Vec<_>>();
    let adequacy_slices = surface
        .slice_z_batch(
            &probe_heights,
            PlaneSliceLimits {
                max_planes: limits.max_planes,
                max_triangle_plane_tests: limits.max_triangle_plane_tests,
                max_intersections: limits.max_slice_intersections,
                max_output_bytes: limits.max_slice_output_bytes,
            },
        )
        .map_err(|error| GenericPerceptionError::Slice(error.to_string()))?;
    let threshold = constants
        .placeholders()
        .minima_support_area_tips()
        .diagnostic()
        .get()
        * std::f64::consts::PI
        * constants
            .placeholders()
            .tip_radius_default_mm()
            .diagnostic()
            .get()
            .powi(2);
    let mut adequate = vec![false; minima.len()];
    for (probe_row, (minimum_index, minimum)) in suspects.into_iter().enumerate() {
        let solids = stitch_layer(
            SliceStitchSite::AdequacyProbe(probe_row),
            adequacy_slices.plane(probe_row),
        )?;
        let point = [minimum.position_mm[0], minimum.position_mm[1]];
        let carried = carried_outer_area(&solids, point);
        adequate[minimum_index] = carried >= threshold;
    }
    Ok(minima
        .into_iter()
        .enumerate()
        .filter(|(index, _)| is_birth[*index] || !adequate[*index])
        .map(|(_, minimum)| minimum)
        .collect())
}

fn filter_minima_to_exterior_faces(
    face_centroids: &[[f64; 3]],
    exterior: &[bool],
    mut minima: Vec<LocalZMinimum>,
    limits: GenericPerceptionLimits,
) -> Result<Vec<LocalZMinimum>, GenericPerceptionError> {
    // This higher-level no-work fast path avoids gathering or index
    // construction when no exterior/interior comparison can change minima.
    // It is distinct from `KnnBatchPlan`, which explicitly rejects N = 0;
    // Q = 0 is the plan-level CSR no-op.
    if minima.is_empty()
        || !exterior.iter().any(|value| *value)
        || !exterior.iter().any(|value| !*value)
    {
        return Ok(minima);
    }
    // Count before gathering positions or constructing Kiddo: this is the
    // KNN authority carried by the public perception input.
    let plan = KnnBatchPlan::new(face_centroids.len(), minima.len(), 1, limits.minima_knn)
        .map_err(|error| GenericPerceptionError::PointIndex(error.to_string()))?;
    let queries = minima
        .iter()
        .map(|minimum| minimum.position_mm)
        .collect::<Vec<_>>();
    let nearest = PointCloudIndex::new_for_knn_batch(face_centroids, &plan)
        .map_err(|error| GenericPerceptionError::PointIndex(error.to_string()))?
        .knn_batch_planned(&queries, &plan)
        .map_err(|error| GenericPerceptionError::PointIndex(error.to_string()))?;
    minima = minima
        .into_iter()
        .enumerate()
        .filter(|(row, _)| {
            let range = nearest
                .row_range(*row)
                .expect("one all-face nearest row per minimum");
            exterior[nearest.point_ids[range.start] as usize]
        })
        .map(|(_, minimum)| minimum)
        .collect();
    Ok(minima)
}

fn carried_outer_area(solids: &[SliceSolid], point: [f64; 2]) -> f64 {
    // Python Path2D.discrete tests each positive outer loop independently.
    // A hollow cavity does not subtract from the load-bearing outer component
    // for this one adequacy predicate.
    solids
        .iter()
        .filter(|solid| point_in_polygon(point, solid.outer.points_xy_mm()))
        .map(|solid| solid.outer.signed_area_mm2)
        .fold(0.0, f64::max)
}

fn layer_tolerance(layers: &LayerPlan, z: f64, count: usize) -> f64 {
    let index = layers.index_at(z).unwrap_or(0);
    layers.heights_mm()[index..].iter().take(count).sum()
}

fn sampled_region_points(minimum: &LocalZMinimum) -> Vec<[f64; 3]> {
    let mut points = vec![minimum.position_mm];
    let region = &minimum.region_points_mm;
    if region.len() <= 64 {
        points.extend(region.iter().copied());
    } else {
        points.extend((0..64).map(|index| region[index * (region.len() - 1) / 63]));
    }
    points
}

fn birth_xy_ok(solid: &SliceSolid, point: [f64; 2], reach: f64) -> bool {
    if solid_contains(solid, point) {
        return true;
    }
    std::iter::once(&solid.outer)
        .chain(solid.holes.iter())
        .flat_map(|loop_| {
            (0..loop_.points_xy_mm.len()).map(move |index| {
                point_segment_distance(
                    point,
                    loop_.points_xy_mm[index],
                    loop_.points_xy_mm[(index + 1) % loop_.points_xy_mm.len()],
                )
            })
        })
        .fold(f64::INFINITY, f64::min)
        <= reach
}

fn point_segment_distance(point: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let denominator = ab[0] * ab[0] + ab[1] * ab[1];
    if denominator < 1.0e-15 {
        return (point[0] - a[0]).hypot(point[1] - a[1]);
    }
    let t = (((point[0] - a[0]) * ab[0] + (point[1] - a[1]) * ab[1]) / denominator).clamp(0.0, 1.0);
    (point[0] - (a[0] + t * ab[0])).hypot(point[1] - (a[1] + t * ab[1]))
}

fn build_load_eligibility(
    mesh: &Mesh,
    surface: &NativeSurface,
    islands: &[SupportIsland],
    specialized: &[SpecializedOwnerReservation],
) -> Result<Vec<LoadEligibility>, GenericPerceptionError> {
    let mut owners = BTreeMap::<u32, BTreeSet<LoadOwner>>::new();
    let mut specialized_pairs = BTreeSet::new();
    for reservation in specialized {
        if reservation.face_id as usize >= mesh.tris().len() {
            return Err(GenericPerceptionError::SpecializedFaceOutOfRange {
                face_id: reservation.face_id,
            });
        }
        if !specialized_pairs.insert((reservation.face_id, reservation.owner_id)) {
            return Err(GenericPerceptionError::DuplicateSpecializedReservation {
                owner_id: reservation.owner_id,
                face_id: reservation.face_id,
            });
        }
        owners
            .entry(reservation.face_id)
            .or_default()
            .insert(LoadOwner::Specialized(reservation.owner_id));
    }
    for island in islands {
        for &face in &island.face_ids {
            owners
                .entry(face)
                .or_default()
                .insert(LoadOwner::GenericIsland(island.id));
        }
    }
    Ok(owners
        .into_iter()
        .map(|(face_id, set)| LoadEligibility {
            face_id,
            eligible_owners: set.into_iter().collect(),
            centroid_mm: surface.geometry().centroids()[face_id as usize],
            area_mm2: surface.geometry().areas()[face_id as usize],
        })
        .collect())
}

/// Finalize the true unique-face load partition only after physical contacts
/// exist. Crate-private by design: generic perception cannot invent contacts.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "consumed by the next specialized-contact integration"
    )
)]
pub(crate) fn finalize_load_partition(
    plan: &GenericContactPlan,
    mesh: &Mesh,
    contacts: &[PhysicalContact],
) -> Result<FinalizedLoadPartition, GenericPerceptionError> {
    // FU-8f phase G2: this entry point is outside the merge gate's per-round
    // loop, so it keeps its own single hash rather than pulling a proof
    // through a public signature for a saving it does not make.
    finalize_load_rows(
        plan,
        &VerifiedMeshFingerprint::compute(mesh)?,
        &plan.load_eligibility,
        contacts,
        plan.load_limits,
        None,
    )
}

/// Test-only empty-contact load authority for exercising a downstream zero
/// branch that normal fixtures do not currently reach. It still traverses the
/// production finalizer with the genuine generic plan, mesh, and plan-owned
/// limits, so binding, retained-byte accounting, and the canonical empty
/// fingerprint are computed rather than copied or forged by the caller.
#[cfg(test)]
pub(crate) fn authority_consistent_empty_load_partition_for_test(
    plan: &GenericContactPlan,
    mesh: &Mesh,
) -> Result<FinalizedLoadPartition, GenericPerceptionError> {
    finalize_load_rows(
        plan,
        &VerifiedMeshFingerprint::compute(mesh)?,
        &[],
        &[],
        plan.load_limits,
        None,
    )
}

fn load_resource_plan_from_counts(
    face_count: usize,
    eligible_owner_count: usize,
    contact_count: usize,
    retained_bytes: usize,
) -> Result<LoadResourcePlan, GenericPerceptionError> {
    let scratch_bytes = retained_bytes
        .checked_add(
            contact_count
                .checked_mul(std::mem::size_of::<usize>())
                .ok_or(GenericPerceptionError::LoadResourceOverflow)?,
        )
        .and_then(|bytes| {
            face_count
                .checked_mul(std::mem::size_of::<FinalizedFaceLoad>())
                .and_then(|rows| bytes.checked_add(rows))
        })
        .and_then(|bytes| {
            face_count
                .checked_mul(std::mem::size_of::<Option<(usize, f64)>>())
                .and_then(|nearest| bytes.checked_add(nearest))
        })
        .ok_or(GenericPerceptionError::LoadResourceOverflow)?;
    let log = usize::BITS as usize - contact_count.max(1).leading_zeros() as usize;
    let owner_log = usize::BITS as usize - eligible_owner_count.max(1).leading_zeros() as usize;
    let face_log = usize::BITS as usize - face_count.max(1).leading_zeros() as usize;
    let operations = face_count
        .checked_mul(contact_count)
        .and_then(|pairs| pairs.checked_mul(owner_log.saturating_add(4)))
        .and_then(|work| {
            contact_count
                .checked_mul(log.saturating_add(2))
                .and_then(|contacts| work.checked_add(contacts))
        })
        .and_then(|work| {
            eligible_owner_count
                .checked_mul(owner_log.saturating_mul(2))
                .and_then(|owners| work.checked_add(owners))
        })
        .and_then(|work| {
            face_count
                .checked_mul(face_log.saturating_mul(2).saturating_add(1))
                .and_then(|faces| work.checked_add(faces))
        })
        .ok_or(GenericPerceptionError::LoadResourceOverflow)?;
    Ok(LoadResourcePlan {
        scratch_bytes,
        operations,
        rows: face_count,
        contacts: contact_count,
    })
}

/// The load finalizer's own exact resource plan.
///
/// `transpose_bytes` is FU-8f phase G1's **new** scratch-bytes term:
/// `cells x 4 B + (contacts + 1) x 4 B` for the contact-major
/// [`LoadEligibilityTranspose`] the finalizer's scan runs over — 32,848,040 B
/// on the Weeping Treant v1.2 (8,210,643 cells, 1,366 contacts), 6.1 % of the
/// 512 MiB `max_load_scratch_bytes`.  It is added **here** and not in
/// [`specialized_load_resource_plan`] because the term is the exact
/// eligible-cell count, which the preflight cannot state ahead of the
/// eligibility: bounding it by anything the preflight knows
/// (`owner_count x contact_count`, or `rows x contacts`) over-states by two to
/// three orders of magnitude and would refuse batches this build admits.  The
/// consequence, stated rather than left to be discovered: the preflight in
/// [`finalize_specialized_load_partition`] under-states this finalizer's
/// scratch by the transpose term.  That costs a **later** refusal, never an
/// unafforded allocation — the buffer is allocated only after the check below
/// passes, and the check below is the exact one.
fn load_resource_plan(
    load_eligibility: &[LoadEligibility],
    contacts: &[PhysicalContact],
    transpose_bytes: usize,
) -> Result<LoadResourcePlan, GenericPerceptionError> {
    let eligible_owner_count = load_eligibility.iter().try_fold(0usize, |total, row| {
        total
            .checked_add(row.eligible_owners.len())
            .ok_or(GenericPerceptionError::LoadResourceOverflow)
    })?;
    let mut plan = load_resource_plan_from_counts(
        load_eligibility.len(),
        eligible_owner_count,
        contacts.len(),
        0,
    )?;
    plan.scratch_bytes = plan
        .scratch_bytes
        .checked_add(transpose_bytes)
        .ok_or(GenericPerceptionError::LoadResourceOverflow)?;
    Ok(plan)
}

/// How many nearest eligible contacts per load row the **baseline** finalize
/// retains for the merge gate's per-proposal delta (FU-8d phase F3, part P2).
///
/// A merge proposal removes exactly two contacts and adds one, so three
/// retained witnesses always leave at least one survivor wherever the row had
/// three or more eligible contacts; where it had fewer, the retained set *is*
/// the whole set.  Either way "which eligible contact is nearest among the
/// survivors of a two-contact removal?" is answerable from the retained slots
/// alone.  This is [`crate::specialized::NEAREST_WITNESS_SLOTS`]'s argument,
/// applied to the load partition's argmin instead of the coverage nearest.
///
/// **The retention moves no charge.** The baseline finalize already visits
/// every `(row, eligible contact)` pair and already computes every distance;
/// keeping the three smallest instead of the one smallest costs at most
/// `LOAD_WITNESS_SLOTS` slot comparisons per *retained* pair, inside the
/// `owner_log + 4` per-pair allowance `load_resource_plan_from_counts` already
/// charges for the eligibility binary search.  Bytes do move and are charged,
/// as `SpecializedLoadBaseline::retained_bytes`.
pub(crate) const LOAD_WITNESS_SLOTS: usize = 3;

/// An unfilled [`LoadWitness`] slot.
const NO_LOAD_WITNESS: u32 = u32::MAX;

/// One retained baseline witness: an eligible contact the finalizer measured
/// against one load row, kept in the finalizer's own order.
///
/// Both fields are the finalizer's own values, copied and never re-derived —
/// `squared_distance` in particular is the exact `Σ (l − r)²` the argmin loop
/// computed, so a later comparison against a merged seat's distance is a
/// comparison of two values of the same expression.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LoadWitness {
    /// Index into the baseline variant's `contacts` slice, or
    /// [`NO_LOAD_WITNESS`].
    contact: u32,
    /// The finalizer's own squared distance from that contact to this row's
    /// centroid.
    squared_distance: f64,
}

impl LoadWitness {
    const UNUSED: Self = Self {
        contact: NO_LOAD_WITNESS,
        squared_distance: f64::INFINITY,
    };
}

/// What the baseline finalize retains for the merge gate's delta path, filled
/// only when the caller asks for it.
///
/// It is an out-parameter rather than a return value because it is optional:
/// the accepted-load path finalizes exactly one partition and reads nothing
/// back, and asking it to build a witness table would be bytes held for no
/// reader.
pub(crate) struct LoadRowRetention {
    /// One fixed-width row per load row, ascending by
    /// `(squared_distance, contact_id)` — the finalizer's own total order —
    /// padded with [`LoadWitness::UNUSED`].
    witnesses: Vec<[LoadWitness; LOAD_WITNESS_SLOTS]>,
    /// Each load row's centroid, in row order.  The delta needs it to measure
    /// a merged seat against the same point the finalizer measured every other
    /// contact against; taking it from `surface` instead would silently
    /// substitute the mesh's centroid for the generic authority's.
    centroids_mm: Vec<[f64; 3]>,
}

/// Insert one measured `(row, contact)` pair into a row's ascending witness
/// slots.
///
/// The order is the finalizer's own — `distance.total_cmp` then `contact_id` —
/// which is a strict total order because contact ids within one partition are
/// unique (`finalize_load_rows` refuses duplicates before this runs).  So slot
/// 0 is the argmin, exactly and unambiguously, and there is no tie for the
/// insertion order to break differently from the running argmin beside it.
fn insert_load_witness(
    slots: &mut [LoadWitness; LOAD_WITNESS_SLOTS],
    contacts: &[PhysicalContact],
    contact_index: usize,
    squared_distance: f64,
) {
    let candidate = LoadWitness {
        contact: contact_index as u32,
        squared_distance,
    };
    let precedes = |left: LoadWitness, right: LoadWitness| -> bool {
        if right.contact == NO_LOAD_WITNESS {
            return true;
        }
        left.squared_distance
            .total_cmp(&right.squared_distance)
            .then_with(|| {
                contacts[left.contact as usize]
                    .contact_id
                    .cmp(&contacts[right.contact as usize].contact_id)
            })
            .is_lt()
    };
    let Some(position) = slots.iter().position(|&slot| precedes(candidate, slot)) else {
        return;
    };
    for index in (position + 1..LOAD_WITNESS_SLOTS).rev() {
        slots[index] = slots[index - 1];
    }
    slots[position] = candidate;
}

/// The contact-major transpose of the load eligibility matrix, in CSR form.
///
/// FU-8f phase G1 (design record `handoff-fu8f-design-2026-08-07`, finding
/// F-3).  [`finalize_load_rows`] used to visit every `(contact, row)` cell and
/// `continue` on the ones the row's ascending `eligible_owners` did not admit.
/// Measured inside the Weeping Treant v1.2's merge-gate batch at `7aa6b73`:
/// **160,834,206 cells visited, of which 8,210,643 were eligible** — 94.9 % of
/// the visits were the `continue` — and that scan cost ~3,090 ms of a
/// 3,899.6 ms batch preparation, 79 % of the round.  The same loop body run
/// over only the eligible cells measured **53x** faster in-process on that
/// batch.  This is a batch-first defect of the same family as FU-8b/8c/8d.
///
/// **Bit-identity is by construction, not by argument.**  The buckets are
/// filled by walking the rows **ascending** and appending each row index to
/// every eligible contact's bucket, so every bucket is already strictly
/// ascending — no sort is performed, and none is needed (the in-process
/// replica's defensive sort was measured to be a no-op,
/// `buckets_already_sorted = true`).  Consuming the contacts **ascending**
/// therefore reproduces the old `for contact { for row { … } }` visit sequence
/// cell for cell: every cell the old scan did not skip is visited exactly once,
/// in the same order, by the same loop body.  So `nearest`'s running argmin
/// with its `(distance, contact_id)` tie-break, and `insert_load_witness`'s
/// slots, see an identical `(contact_index, distance)` sequence.  The replica
/// reported `nearest_bit_identical = true` on the treant's real batch, and
/// `fu8f_transposed_finalize_reproduces_the_scan_bit_for_bit` pins it against
/// the retained `contacts x rows` scan on fixtures.
///
/// **Charged.**  `cells x 4 B + (contacts + 1) x 4 B`, a **new**
/// [`LoadResourcePlan::scratch_bytes`] term introduced by this phase —
/// 32,848,040 B on the treant, 6.1 % of the 512 MiB `max_load_scratch_bytes`
/// and 3.1 % of the 1 GiB gate peak.  Nothing else moves.  The operation model
/// is deliberately **left standing and over-stating**: it still prices the scan
/// as `rows x contacts x (owner_log + 4)` (~4.34e9 on the treant) where the
/// executed work is now ~1e8.  That is the FU-8e Q5 precedent — a plan that
/// admits the general path a fortiori admits its specialisation — and the
/// repricing is deferred to phase G5 by design, so no charged operation count
/// moves in this phase.
///
/// Note (design record open question 7): the rows are **not** uniformly small.
/// One treant row carries 568 eligible owners of 1,366 contacts.  The transpose
/// handles it identically; nothing here assumes short buckets.
struct LoadEligibilityTranspose {
    /// Every eligible row index, contact-major.  Contact `contact`'s rows are
    /// `rows[starts[contact] as usize..starts[contact + 1] as usize]`, strictly
    /// ascending.
    rows: Vec<u32>,
    /// One boundary per contact plus a terminator, so `starts.len()` is
    /// `contacts.len() + 1` and `starts[contacts.len()]` is the cell count.
    starts: Vec<u32>,
}

impl LoadEligibilityTranspose {
    /// Contact `contact`'s ascending eligible rows.
    fn rows_of(&self, contact: usize) -> &[u32] {
        &self.rows[self.starts[contact] as usize..self.starts[contact + 1] as usize]
    }
}

/// The counting half of the transpose build: everything needed to **charge**
/// the buffer before any of it is allocated.
///
/// The split exists because [`LoadEligibilityTranspose`]'s size is the exact
/// eligible-cell count, which no preflight can state without walking the
/// eligibility — so the count runs first, the plan is charged and checked, and
/// only then is the cells-sized buffer allocated.  What the count itself
/// spends is stated rather than hidden, the same way [`GroupedOwnerFaces`]
/// states its cost: two `u32` per contact (`order` and `owner_rows`, together
/// exactly the `contact_count x size_of::<usize>()` the plan already charges),
/// and one binary search per `(row, eligible owner)` — 7,988,078 of them on the
/// treant, against the 4.34e9 the standing operation model already prices for
/// the scan those searches replace.
struct LoadTransposeCounts {
    /// Contact indices ascending by `(owner, contact index)`, so contacts
    /// sharing an owner form one contiguous ascending run.
    order: Vec<u32>,
    /// Indexed by a run's **start position in `order`**: how many rows that
    /// run's owner is eligible for.  Positions that do not start a run are
    /// never read.  Reused as the per-contact fill cursor in [`Self::finish`].
    owner_rows: Vec<u32>,
    /// `Σ` over contacts of the rows that contact is eligible for — the exact
    /// number of cells the transposed scan will visit.
    cells: usize,
}

impl LoadTransposeCounts {
    /// Count the eligible cells without allocating anything cells-sized.
    ///
    /// Contacts that share a [`LoadOwner`] have, by definition, the identical
    /// row set — eligibility is a predicate on the owner alone.  So the count
    /// is taken **once per owner run** over the `Σ|eligible_owners|` cells of
    /// the row-major eligibility, never once per `(row, contact)` cell.
    fn count(
        load_eligibility: &[LoadEligibility],
        contacts: &[PhysicalContact],
    ) -> Result<Self, GenericPerceptionError> {
        let contact_count = u32::try_from(contacts.len())
            .map_err(|_| GenericPerceptionError::LoadResourceOverflow)?;
        u32::try_from(load_eligibility.len())
            .map_err(|_| GenericPerceptionError::LoadResourceOverflow)?;
        let mut order: Vec<u32> = (0..contact_count).collect();
        order.sort_unstable_by_key(|&index| (contacts[index as usize].owner, index));
        let mut owner_rows = vec![0u32; contacts.len()];
        for eligibility in load_eligibility {
            // `eligible_owners` is ascending and `order` is ascending by owner,
            // so each row's searches only ever move forward.
            let mut lower = 0usize;
            for owner in &eligibility.eligible_owners {
                let start = lower
                    + order[lower..]
                        .partition_point(|&index| contacts[index as usize].owner < *owner);
                lower = start;
                if start < order.len() && contacts[order[start] as usize].owner == *owner {
                    // One row admits one owner at most once: `eligible_owners`
                    // is deduplicated, so this cannot exceed the row count,
                    // which `u32::try_from` above has already bounded.
                    owner_rows[start] += 1;
                }
            }
        }
        let mut cells = 0usize;
        let mut position = 0usize;
        while position < order.len() {
            let owner = contacts[order[position] as usize].owner;
            let rows = owner_rows[position] as usize;
            let mut end = position;
            while end < order.len() && contacts[order[end] as usize].owner == owner {
                end += 1;
            }
            cells = (end - position)
                .checked_mul(rows)
                .and_then(|run| cells.checked_add(run))
                .ok_or(GenericPerceptionError::LoadResourceOverflow)?;
            position = end;
        }
        u32::try_from(cells).map_err(|_| GenericPerceptionError::LoadResourceOverflow)?;
        Ok(Self {
            order,
            owner_rows,
            cells,
        })
    }

    /// What [`Self::finish`] will allocate: the phase's new charged term.
    fn scratch_bytes(&self, contact_count: usize) -> Result<usize, GenericPerceptionError> {
        self.cells
            .checked_mul(std::mem::size_of::<u32>())
            .and_then(|rows| {
                contact_count
                    .checked_add(1)
                    .and_then(|boundaries| boundaries.checked_mul(std::mem::size_of::<u32>()))
                    .and_then(|starts| rows.checked_add(starts))
            })
            .ok_or(GenericPerceptionError::LoadResourceOverflow)
    }

    /// Fill the buckets, rows ascending, so every bucket comes out ascending.
    fn finish(
        self,
        load_eligibility: &[LoadEligibility],
        contacts: &[PhysicalContact],
    ) -> LoadEligibilityTranspose {
        let Self {
            order,
            mut owner_rows,
            cells,
        } = self;
        // `starts[contact + 1]` first holds contact `contact`'s own row count,
        // then becomes the prefix sum.  Every contact of one owner run gets the
        // run's count, because eligibility is a predicate on the owner alone.
        let mut starts = vec![0u32; contacts.len() + 1];
        let mut position = 0usize;
        while position < order.len() {
            let owner = contacts[order[position] as usize].owner;
            let rows = owner_rows[position];
            let mut end = position;
            while end < order.len() && contacts[order[end] as usize].owner == owner {
                starts[order[end] as usize + 1] = rows;
                end += 1;
            }
            position = end;
        }
        for index in 0..contacts.len() {
            // The total is `cells`, which `count` proved fits in `u32`.
            starts[index + 1] += starts[index];
        }
        debug_assert_eq!(starts[contacts.len()] as usize, cells);
        let mut rows = vec![0u32; cells];
        // `owner_rows` is done; reuse it as the per-contact write cursor rather
        // than allocating a second contact-sized buffer.
        let cursors = &mut owner_rows;
        cursors.copy_from_slice(&starts[..contacts.len()]);
        for (face_index, eligibility) in load_eligibility.iter().enumerate() {
            let face = face_index as u32;
            let mut lower = 0usize;
            for owner in &eligibility.eligible_owners {
                let start = lower
                    + order[lower..]
                        .partition_point(|&index| contacts[index as usize].owner < *owner);
                lower = start;
                let mut end = start;
                while end < order.len() && contacts[order[end] as usize].owner == *owner {
                    let contact = order[end] as usize;
                    rows[cursors[contact] as usize] = face;
                    cursors[contact] += 1;
                    end += 1;
                }
            }
        }
        debug_assert!(
            (0..contacts.len()).all(|contact| cursors[contact] == starts[contact + 1]),
            "every bucket filled exactly to its boundary",
        );
        LoadEligibilityTranspose { rows, starts }
    }
}

/// The load finalizer's nearest-and-witness pass, over the transposed
/// eligibility.
///
/// This is the pre-FU-8f inner loop body, unchanged, driven by
/// [`LoadEligibilityTranspose`] instead of by a `contacts x rows` scan with a
/// `continue`.  It is a named function so the equivalence test can run the
/// production pass itself against the retained reference scan rather than a
/// re-typed copy of it.
fn scan_transposed_load(
    load_eligibility: &[LoadEligibility],
    contacts: &[PhysicalContact],
    transpose: &LoadEligibilityTranspose,
    retain: Option<&mut LoadRowRetention>,
) -> Vec<Option<(usize, f64)>> {
    // One flattened owner x face x contact batch.  The matrix is streamed into
    // a caller-budgeted nearest row rather than issuing scalar spatial calls.
    let mut nearest = vec![None::<(usize, f64)>; load_eligibility.len()];
    // FU-8d phase F3 (P2): when the caller asks, the same pass also keeps the
    // `LOAD_WITNESS_SLOTS` smallest pairs per row.  The running argmin below is
    // untouched — the retention is written *beside* it, never through it — so
    // the emitted partition cannot depend on whether anything was retained.
    // `debug_assert`s in the caller pin slot 0 against the argmin.
    let mut retention = retain;
    if let Some(retention) = retention.as_deref_mut() {
        retention.witnesses =
            vec![[LoadWitness::UNUSED; LOAD_WITNESS_SLOTS]; load_eligibility.len()];
        retention.centroids_mm = load_eligibility
            .iter()
            .map(|eligibility| eligibility.centroid_mm)
            .collect();
    }
    for (contact_index, contact) in contacts.iter().enumerate() {
        for &row in transpose.rows_of(contact_index) {
            let face_index = row as usize;
            let eligibility = &load_eligibility[face_index];
            let distance = contact
                .point_mm
                .iter()
                .zip(eligibility.centroid_mm)
                .map(|(left, right)| (left - right).powi(2))
                .sum::<f64>();
            if let Some(retention) = retention.as_deref_mut() {
                insert_load_witness(
                    &mut retention.witnesses[face_index],
                    contacts,
                    contact_index,
                    distance,
                );
            }
            let replace = nearest[face_index].is_none_or(|(old_index, old_distance)| {
                distance
                    .total_cmp(&old_distance)
                    .then_with(|| contact.contact_id.cmp(&contacts[old_index].contact_id))
                    .is_lt()
            });
            if replace {
                nearest[face_index] = Some((contact_index, distance));
            }
        }
    }
    nearest
}

/// The pre-FU-8f `contacts x rows` scan, retained verbatim as the equivalence
/// oracle [`scan_transposed_load`] is proved against.
///
/// Same shape as the retained references FU-8c and FU-8d left behind: the
/// replaced implementation stays compilable so the replacement is checked
/// against the thing it replaced, not against a restatement of it.  It visits
/// all `rows x contacts` cells — 160,834,206 on the treant against 8,210,643
/// eligible — which is exactly why it is `#[cfg(test)]` and fixture-sized.
#[cfg(test)]
type ReferenceScan = (
    Vec<Option<(usize, f64)>>,
    Vec<[LoadWitness; LOAD_WITNESS_SLOTS]>,
);

#[cfg(test)]
fn reference_contacts_x_rows_scan(
    load_eligibility: &[LoadEligibility],
    contacts: &[PhysicalContact],
) -> ReferenceScan {
    let mut nearest = vec![None::<(usize, f64)>; load_eligibility.len()];
    let mut witnesses = vec![[LoadWitness::UNUSED; LOAD_WITNESS_SLOTS]; load_eligibility.len()];
    for (contact_index, contact) in contacts.iter().enumerate() {
        for (face_index, eligibility) in load_eligibility.iter().enumerate() {
            if eligibility
                .eligible_owners
                .binary_search(&contact.owner)
                .is_err()
            {
                continue;
            }
            let distance = contact
                .point_mm
                .iter()
                .zip(eligibility.centroid_mm)
                .map(|(left, right)| (left - right).powi(2))
                .sum::<f64>();
            insert_load_witness(
                &mut witnesses[face_index],
                contacts,
                contact_index,
                distance,
            );
            let replace = nearest[face_index].is_none_or(|(old_index, old_distance)| {
                distance
                    .total_cmp(&old_distance)
                    .then_with(|| contact.contact_id.cmp(&contacts[old_index].contact_id))
                    .is_lt()
            });
            if replace {
                nearest[face_index] = Some((contact_index, distance));
            }
        }
    }
    (nearest, witnesses)
}

/// A mesh carried together with its own [`mesh_fingerprint`], hashed once.
///
/// # FU-8f phase G2 — why this type exists
///
/// `mesh_fingerprint` walks every triangle of the mesh.  On the Weeping Treant
/// v1.2 that is 429,120 triangles, and the merge gate used to pay for it
/// **twice per batch** — once in [`finalize_specialized_load_partition`] and
/// again in the [`finalize_load_rows`] it immediately calls — over a mesh that
/// cannot change inside a tournament, because the tournament merges *contacts*
/// and never touches geometry.  Measured at `7aa6b73` on the treant's real
/// gate batch: 85.1 ms + 81.5 ms = 166.6 ms per merge round, contended.
///
/// # What did NOT change, and must not
///
/// The **staleness comparison stays at every site**.  This type hoists the
/// *hash*, not the *check*: each consumer still compares the fingerprint
/// against its own `plan.authority.mesh` and still returns
/// [`GenericPerceptionError::StaleAuthority`] when they disagree.  "Verify
/// once, thread the proof" — never "stop verifying".  A caller that has no
/// proof to hand builds one with [`Self::compute`] and pays exactly what it
/// paid before; nothing anywhere is verified less than it was.
///
/// That claim is pinned, one arm per site, by
/// `fu8f_hoisted_fingerprint_still_refuses_a_foreign_mesh_at_both_finalizers`.
/// *Amended 2026-08-07*: the original G2 test pinned
/// [`finalize_specialized_load_partition`] only **by delegation** — deleting
/// that site's own comparison left the test green, because the call fell
/// through to the [`finalize_load_rows`] it invokes, which refused on the
/// same proof.  The test was strengthened so each arm can be satisfied only
/// by the site it names, and each site's deletion was measured to fail its
/// own arm; the arm shapes and the mutation results are in its doc comment.
///
/// # Why the mesh travels inside the proof
///
/// The consumers need the mesh itself (face counts, triangles) as well as the
/// digest.  Taking them as two arguments would admit a proof computed from one
/// mesh being presented alongside a different one — a fingerprint that attests
/// nothing.  The proof therefore *owns the reference*: [`Self::mesh`] is the
/// only mesh a holder can reach through it, so digest and geometry cannot
/// drift apart by construction rather than by discipline.
pub(crate) struct VerifiedMeshFingerprint<'mesh> {
    mesh: &'mesh Mesh,
    fingerprint: String,
}

impl<'mesh> VerifiedMeshFingerprint<'mesh> {
    /// Hash `mesh` once and bind the digest to it.
    pub(crate) fn compute(mesh: &'mesh Mesh) -> Result<Self, GenericPerceptionError> {
        let fingerprint = mesh_fingerprint(mesh)
            .map_err(|error| GenericPerceptionError::Topology(error.to_string()))?;
        Ok(Self { mesh, fingerprint })
    }

    /// The mesh this digest was computed from — the only one it attests to.
    pub(crate) fn mesh(&self) -> &'mesh Mesh {
        self.mesh
    }

    /// The digest, for the caller's own staleness comparison.
    pub(crate) fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

fn finalize_load_rows(
    plan: &GenericContactPlan,
    verified_mesh: &VerifiedMeshFingerprint<'_>,
    load_eligibility: &[LoadEligibility],
    contacts: &[PhysicalContact],
    limits: LoadFinalizeLimits,
    retain: Option<&mut LoadRowRetention>,
) -> Result<FinalizedLoadPartition, GenericPerceptionError> {
    // FU-8f phase G2: the digest is threaded in rather than recomputed here,
    // but the comparison it feeds is the same comparison this site has always
    // made, and it still refuses.  The mesh itself was only ever an input to
    // that hash here — every geometric quantity this finalizer reads comes
    // from `load_eligibility`, which is why the parameter is the proof and not
    // the mesh.
    if verified_mesh.fingerprint() != plan.authority.mesh {
        return Err(GenericPerceptionError::StaleAuthority);
    }
    // FU-8f phase G1.  The transpose is counted before it is charged and
    // charged before it is allocated: its size is the exact eligible-cell
    // count, which nothing upstream can state.  See [`LoadTransposeCounts`]
    // for what the count itself spends and why the standing operation model
    // already affords it.
    let transpose = LoadTransposeCounts::count(load_eligibility, contacts)?;
    let resource = load_resource_plan(
        load_eligibility,
        contacts,
        transpose.scratch_bytes(contacts.len())?,
    )?;
    if resource.scratch_bytes > limits.max_scratch_bytes {
        return Err(GenericPerceptionError::LoadScratchLimit {
            required: resource.scratch_bytes,
            limit: limits.max_scratch_bytes,
        });
    }
    let operation_limit = limits.operations_limit(resource.rows, resource.contacts);
    if limits.operations_per_batch_unit != 0 {
        // Declared model-scaled cap: report what this batch actually got.
        crate::census_record!(crate::census::record_scaled_cap(
            crate::census::ScaledCapRow {
                kind: "specialized_load_operations",
                floor: limits.max_operations_floor,
                per_batch_unit: limits.operations_per_batch_unit,
                rows: resource.rows,
                columns: resource.contacts,
                materialized: operation_limit,
            }
        ));
    }
    if resource.operations > operation_limit {
        return Err(GenericPerceptionError::LoadOperationLimit {
            required: resource.operations,
            limit: operation_limit,
        });
    }
    let mut contact_order = Vec::with_capacity(contacts.len());
    for contact in contacts {
        if contact.point_mm.iter().any(|value| !value.is_finite()) {
            return Err(GenericPerceptionError::NonFinitePhysicalContact {
                contact_id: contact.contact_id,
            });
        }
        contact_order.push(contact_order.len());
    }
    contact_order.sort_unstable_by_key(|&index| contacts[index].contact_id);
    for pair in contact_order.windows(2) {
        if contacts[pair[0]].contact_id == contacts[pair[1]].contact_id {
            return Err(GenericPerceptionError::DuplicatePhysicalContact {
                contact_id: contacts[pair[0]].contact_id,
            });
        }
    }
    let mut rows = Vec::with_capacity(load_eligibility.len());
    let transpose = transpose.finish(load_eligibility, contacts);
    let mut retention = retain;
    let nearest = scan_transposed_load(
        load_eligibility,
        contacts,
        &transpose,
        retention.as_deref_mut(),
    );
    if let Some(retention) = retention {
        debug_assert_eq!(retention.witnesses.len(), load_eligibility.len());
        for (row, slots) in retention.witnesses.iter().enumerate() {
            debug_assert_eq!(
                nearest[row].map(|(index, _)| index as u32),
                (slots[0].contact != NO_LOAD_WITNESS).then_some(slots[0].contact),
            );
            debug_assert_eq!(
                nearest[row].map(|(_, distance)| distance.to_bits()),
                (slots[0].contact != NO_LOAD_WITNESS)
                    .then_some(slots[0].squared_distance.to_bits()),
            );
        }
    }
    for (face_index, eligibility) in load_eligibility.iter().enumerate() {
        let (contact_index, _) =
            nearest[face_index].ok_or(GenericPerceptionError::MissingPhysicalContact {
                face_id: eligibility.face_id,
            })?;
        let contact = &contacts[contact_index];
        rows.push(FinalizedFaceLoad {
            face_id: eligibility.face_id,
            contact_id: contact.contact_id,
            owner: contact.owner,
            carried_area_mm2: eligibility.area_mm2,
        });
    }
    let mut contact_hash = Fnv::new();
    for index in contact_order {
        let contact = &contacts[index];
        contact_hash.u64(contact.contact_id);
        hash_owner(&mut contact_hash, contact.owner);
        for value in contact.point_mm {
            contact_hash.f64(value);
        }
    }
    Ok(FinalizedLoadPartition {
        authority: plan.authority.clone(),
        contacts_fingerprint: contact_hash.finish(),
        rows,
    })
}

/// Specialized `(owner_id, face_id)` pairs grouped by face id.
///
/// FU-8b defect (B).  The extended load authority holds one row per *face*,
/// but the pair list arrives one entry per `(owner, face)`: on the Weeping
/// Treant, 7,971,451 pairs covering 113,921 distinct faces, 70.0 pairs per
/// covered face, on a 429,120-face mesh.  Everything that *predicted* its size
/// from the pair count was therefore ~70x too large — the row count that
/// drives the `rows x contacts x (owner_log + 4)` load operation bound, and
/// the retained bytes that drive the load scratch bound.  The grouping is
/// built once, in one sort, and read by both.
///
/// Correction 2026-08-06 (record fix F4).  This comment previously read
/// "7,558,958 pairs over 429,120 faces … 18.6 per face" as though both were
/// observed.  Neither was: 7,558,958 was **derived by arithmetic**, the
/// falsified 7,988,078 predicted-row figure minus the 429,120 mesh faces, and
/// 429,120 is the whole mesh, not the faces the pairs reach.  The counts above
/// replace it and are read directly off `GroupedOwnerFaces` in one
/// scratch-instrumented treant run.
///
/// Note 2026-08-07 (FU-8f phase G1, design record open question 8) — **do not
/// "correct" 7,988,078 back.**  That figure appears in this codebase's records
/// as two unrelated quantities that happen to share their digits on the same
/// model.  Here it is the **falsified predicted row count** the paragraph above
/// strikes.  In [`LoadEligibilityTranspose`] and the caps record it is the
/// **measured `Σ |eligible_owners|` over the treant's 117,741 load rows**, a
/// real and current counter read at `7aa6b73`.  Both sentences are correct as
/// written, and neither number is evidence about the other.
///
/// Cost, stated because it is spent before this build's own resource check
/// (the upstream accepted-load preflight has already checked the same plan
/// against the same limits, with [`SpecializedLoadCounts::bounded`]): one
/// `(u32, u32)` per pair plus one `usize` per face and one more, i.e. at most
/// twice the pair slice the caller already holds materialized.
struct GroupedOwnerFaces {
    /// `(face_id, owner_id)` ascending and deduplicated, so one face's owners
    /// are one contiguous ascending run.
    entries: Vec<(u32, u32)>,
    /// Group boundaries into `entries`: group `index` is
    /// `entries[starts[index]..starts[index + 1]]`.  Empty when there are no
    /// pairs at all, otherwise one longer than the face count.
    starts: Vec<usize>,
}

impl GroupedOwnerFaces {
    /// One gather, one sort, one dedup, one boundary scan.  Nothing here
    /// rescans the pair list per pair or per row, which is the defect being
    /// replaced.
    fn from_pairs(specialized_owner_faces: &[(u32, u32)]) -> Self {
        let mut entries = Vec::with_capacity(specialized_owner_faces.len());
        entries.extend(
            specialized_owner_faces
                .iter()
                .map(|&(owner_id, face_id)| (face_id, owner_id)),
        );
        entries.sort_unstable();
        entries.dedup();
        let mut starts = Vec::new();
        if !entries.is_empty() {
            starts.push(0);
            starts.extend(
                entries
                    .windows(2)
                    .enumerate()
                    .filter_map(|(index, pair)| (pair[0].0 != pair[1].0).then_some(index + 1)),
            );
            starts.push(entries.len());
        }
        Self { entries, starts }
    }

    /// Distinct face ids among the pairs: the number of rows the extension can
    /// add, ascending by face id over `0..face_count()`.
    fn face_count(&self) -> usize {
        self.starts.len().saturating_sub(1)
    }

    /// Distinct `(owner, face)` entries: what the extension adds to the
    /// eligible-owner total.
    fn owner_entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Group `group`'s face id.  Ascending in `group`.
    fn face(&self, group: usize) -> u32 {
        self.entries[self.starts[group]].0
    }

    /// Group `group`'s ascending distinct owner ids.
    fn owners(&self, group: usize) -> impl Iterator<Item = u32> + '_ {
        self.entries[self.starts[group]..self.starts[group + 1]]
            .iter()
            .map(|&(_, owner_id)| owner_id)
    }

    fn group_len(&self, group: usize) -> usize {
        self.starts[group + 1] - self.starts[group]
    }
}

/// What the specialized extension contributes to the load resource plan.
///
/// The two counts are deliberately separate: a pair adds an *owner* to some
/// row, but only a distinct face adds a *row*.  Conflating them is FU-8b
/// defect (B).
#[derive(Clone, Copy, Debug)]
pub(crate) struct SpecializedLoadCounts {
    /// One per distinct `(owner, face)` pair.
    owner_entries: usize,
    /// Distinct faces among those pairs.  Never larger than `owner_entries`.
    faces: usize,
}

impl SpecializedLoadCounts {
    /// The exact counts, from the grouping the finalizer builds.
    fn exact(grouped: &GroupedOwnerFaces) -> Self {
        Self {
            owner_entries: grouped.owner_entry_count(),
            faces: grouped.face_count(),
        }
    }

    /// The counts a preflight can state before the pairs exist.
    ///
    /// Every served face comes from some island's `face_ids`, so the distinct
    /// pair faces are bounded both by the pair count and by the number of
    /// distinct island faces — whichever is smaller.  That is an upper bound
    /// on what [`Self::exact`] will report, which is what a refusal boundary
    /// evaluated ahead of the build requires.
    pub(crate) fn bounded(owner_entries: usize, island_face_count: usize) -> Self {
        Self {
            owner_entries,
            faces: owner_entries.min(island_face_count),
        }
    }
}

/// Extend the one generic load authority with the faces served by finalized
/// specialized physical contacts, then invoke the sole load finalizer.  The
/// extension is crate-private so callers cannot manufacture eligible load
/// routes or bypass the generic plan's mesh binding.
pub(crate) fn specialized_load_resource_plan(
    plan: &GenericContactPlan,
    contact_count: usize,
    counts: SpecializedLoadCounts,
) -> Result<LoadResourcePlan, GenericPerceptionError> {
    let SpecializedLoadCounts {
        owner_entries: specialized_owner_face_count,
        faces: specialized_face_count,
    } = counts;
    let base_owner_count = plan
        .load_eligibility
        .iter()
        .try_fold(0usize, |total, row| {
            total
                .checked_add(row.eligible_owners.len())
                .ok_or(GenericPerceptionError::LoadResourceOverflow)
        })?;
    // One row per face, never one row per (owner, face) pair (FU-8b defect B).
    let row_count = plan
        .load_eligibility
        .len()
        .checked_add(specialized_face_count)
        .ok_or(GenericPerceptionError::LoadResourceOverflow)?;
    let owner_count = base_owner_count
        .checked_add(specialized_owner_face_count)
        .ok_or(GenericPerceptionError::LoadResourceOverflow)?;
    // The grouping the finalizer builds from the pairs: one `(u32, u32)` per
    // pair plus one `usize` per face group boundary.
    let grouping_bytes = specialized_owner_face_count
        .checked_mul(std::mem::size_of::<(u32, u32)>())
        .and_then(|bytes| {
            specialized_face_count
                .checked_add(1)
                .and_then(|boundaries| boundaries.checked_mul(std::mem::size_of::<usize>()))
                .and_then(|starts| bytes.checked_add(starts))
        })
        .ok_or(GenericPerceptionError::LoadResourceOverflow)?;
    let retained_bytes = row_count
        .checked_mul(std::mem::size_of::<LoadEligibility>())
        .and_then(|bytes| {
            owner_count
                .checked_mul(std::mem::size_of::<LoadOwner>())
                .and_then(|owners| bytes.checked_add(owners))
        })
        .and_then(|bytes| bytes.checked_add(grouping_bytes))
        .ok_or(GenericPerceptionError::LoadResourceOverflow)?;
    let mut resource =
        load_resource_plan_from_counts(row_count, owner_count, contact_count, retained_bytes)?;
    // What `extend_load_eligibility` costs on top of the finalizer itself,
    // restated for the batched builder (FU-8b defect C).  The meter counts
    // elementary comparisons, the same unit the terms above count.
    //
    //   pairs * (ceil(log2(pairs + 1)) + 4)
    //       the grouping: one gather pass, `sort_unstable`'s comparison bound,
    //       one `dedup` pass, one group-boundary pass, one range-check pass.
    //   base * (ceil(log2(base + 1)) + 2)
    //       ordering the base rows by face id, then walking them in the merge.
    //   faces
    //       walking the groups once to emit the rows no base row covers.
    //
    // Its predecessor charged `pairs^2 + base * pairs` for the rescans it
    // really did: 5.71e13 + 3.24e12 on the Weeping Treant, which is why that
    // stage was measured at sixteen hours.  This is linearithmic where that
    // was quadratic — larger than the old charge only when `pairs` is under a
    // handful, and about 2.2e8 where the old charge was 6.0e13.
    let base_count = plan.load_eligibility.len();
    let pair_log = usize::BITS as usize - specialized_owner_face_count.leading_zeros() as usize;
    let base_log = usize::BITS as usize - base_count.leading_zeros() as usize;
    let extension = specialized_owner_face_count
        .checked_mul(pair_log.saturating_add(4))
        .and_then(|work| {
            base_count
                .checked_mul(base_log.saturating_add(2))
                .and_then(|base_work| work.checked_add(base_work))
        })
        .and_then(|work| work.checked_add(specialized_face_count))
        .ok_or(GenericPerceptionError::LoadResourceOverflow)?;
    resource.operations = resource
        .operations
        .checked_add(extension)
        .ok_or(GenericPerceptionError::LoadResourceOverflow)?;
    Ok(resource)
}

pub(crate) fn finalize_specialized_load_partition(
    plan: &GenericContactPlan,
    verified_mesh: &VerifiedMeshFingerprint<'_>,
    surface: &NativeSurface,
    contacts: &[PhysicalContact],
    specialized_owner_faces: &[(u32, u32)],
    limits: LoadFinalizeLimits,
    retain: Option<&mut LoadRowRetention>,
) -> Result<FinalizedLoadPartition, GenericPerceptionError> {
    // FU-8f phase G2: hoisted digest, unchanged comparison.  This site and the
    // `finalize_load_rows` it calls below used to hash the same unchanging
    // mesh twice per gate batch; they now share one hash and each still checks.
    //
    // This comparison must stay AHEAD of everything below it, not merely
    // present (amended 2026-08-07).  `extend_load_eligibility` takes its face
    // range from `verified_mesh.mesh()` while `surface` is an independent
    // argument, so a proof of a LARGER mesh would carry a face-range bound
    // past the surface it indexes.  Refusing here is what keeps those two
    // from ever being read against each other.
    let mesh = verified_mesh.mesh();
    if verified_mesh.fingerprint() != plan.authority.mesh {
        return Err(GenericPerceptionError::StaleAuthority);
    }
    // A child specialized-plan edit may intentionally remove every
    // demand-free regular contact.  There is then no physical namespace to
    // which generic faces may be assigned.  Canonicalize that real zero case
    // through the same finalizer, with an empty eligibility set, rather than
    // fabricating rows or silently retaining the parent's load artifact.
    if contacts.is_empty() {
        return finalize_load_rows(plan, verified_mesh, &[], contacts, limits, retain);
    }
    // Group the pairs by face once.  The row count, the eligible-owner count
    // and the extension below are all read off this one grouping instead of
    // rescanning the pair list (FU-8b defect B).
    let grouped = GroupedOwnerFaces::from_pairs(specialized_owner_faces);
    let resource = specialized_load_resource_plan(
        plan,
        contacts.len(),
        SpecializedLoadCounts::exact(&grouped),
    )?;
    if resource.scratch_bytes > limits.max_scratch_bytes {
        return Err(GenericPerceptionError::LoadScratchLimit {
            required: resource.scratch_bytes,
            limit: limits.max_scratch_bytes,
        });
    }
    let operation_limit = limits.operations_limit(resource.rows, resource.contacts);
    if limits.operations_per_batch_unit != 0 {
        // Declared model-scaled cap: report what this batch actually got.
        crate::census_record!(crate::census::record_scaled_cap(
            crate::census::ScaledCapRow {
                kind: "specialized_load_operations",
                floor: limits.max_operations_floor,
                per_batch_unit: limits.operations_per_batch_unit,
                rows: resource.rows,
                columns: resource.contacts,
                materialized: operation_limit,
            }
        ));
    }
    if resource.operations > operation_limit {
        return Err(GenericPerceptionError::LoadOperationLimit {
            required: resource.operations,
            limit: operation_limit,
        });
    }
    let extended = extend_load_eligibility(
        &plan.load_eligibility,
        mesh.tris().len(),
        surface,
        &grouped,
        specialized_owner_faces,
    )?;
    finalize_load_rows(plan, verified_mesh, &extended, contacts, limits, retain)
}

/// Merge the specialized owners into the generic load authority's rows.
///
/// The result is one row per distinct face of `base` and the pairs, ascending
/// by face id, each carrying the ascending distinct union of its generic and
/// specialized owners.  A face that `base` already knows keeps `base`'s
/// centroid and area; a face only the pairs reach takes them from `surface`.
///
/// FU-8b defect (C).  The predecessor rescanned the whole pair list twice for
/// every base row (once to size the row's owner vector, once to fill it) and,
/// for every pair, once more over `base` and once more over the pairs already
/// visited — about 6e13 scalar comparisons on the Weeping Treant, single
/// threaded, and charged by no cap, which is the 16-hour hang the instrumented
/// caps study found with 4,127 of 4,127 stack samples inside this function.
/// It now gathers once — `grouped` is the single sort the caller already paid
/// for — and scatters once, in one linear merge over the ascending face ids.
fn extend_load_eligibility(
    base: &[LoadEligibility],
    mesh_face_count: usize,
    surface: &NativeSurface,
    grouped: &GroupedOwnerFaces,
    specialized_owner_faces: &[(u32, u32)],
) -> Result<Vec<LoadEligibility>, GenericPerceptionError> {
    // Range-check every pair up front, in the order the pairs were supplied,
    // so the face id this reports is the one the per-pair scan reported.  The
    // row build below cannot fail, so hoisting the check changes nothing but
    // the number of times the pair list is walked.
    for &(_, face_id) in specialized_owner_faces {
        if face_id as usize >= mesh_face_count {
            return Err(GenericPerceptionError::SpecializedFaceOutOfRange { face_id });
        }
    }

    // Join the base rows to the groups in one merge over ascending face ids.
    // `base` is built by `build_load_eligibility` from a `BTreeMap` and is
    // already ascending, but nothing here depends on that: the merge walks an
    // explicit order, and duplicate base faces all bind to the same group,
    // exactly as the per-row filter they replace did.
    let group_count = grouped.face_count();
    let mut base_order = (0..base.len()).collect::<Vec<_>>();
    base_order.sort_by_key(|&index| base[index].face_id);
    let mut base_group = vec![usize::MAX; base.len()];
    let mut group_is_base_face = vec![false; group_count];
    let mut group = 0usize;
    for &index in &base_order {
        let face_id = base[index].face_id;
        while group < group_count && grouped.face(group) < face_id {
            group += 1;
        }
        if group < group_count && grouped.face(group) == face_id {
            base_group[index] = group;
            group_is_base_face[group] = true;
        }
    }

    let mut extended = Vec::with_capacity(base.len() + group_count);
    for (index, row) in base.iter().enumerate() {
        let specialized = base_group[index];
        let addition_count = if specialized == usize::MAX {
            0
        } else {
            grouped.group_len(specialized)
        };
        let mut owners = Vec::with_capacity(row.eligible_owners.len() + addition_count);
        owners.extend(row.eligible_owners.iter().copied());
        if specialized != usize::MAX {
            owners.extend(grouped.owners(specialized).map(LoadOwner::Specialized));
        }
        owners.sort_unstable();
        owners.dedup();
        extended.push(LoadEligibility {
            face_id: row.face_id,
            eligible_owners: owners,
            centroid_mm: row.centroid_mm,
            area_mm2: row.area_mm2,
        });
    }
    for (group, is_base_face) in group_is_base_face.iter().enumerate() {
        if *is_base_face {
            continue;
        }
        let face_id = grouped.face(group);
        let mut owners = Vec::with_capacity(grouped.group_len(group));
        owners.extend(grouped.owners(group).map(LoadOwner::Specialized));
        owners.sort_unstable();
        owners.dedup();
        extended.push(LoadEligibility {
            face_id,
            eligible_owners: owners,
            centroid_mm: surface.geometry().centroids()[face_id as usize],
            area_mm2: surface.geometry().areas()[face_id as usize],
        });
    }
    // Still a stable sort by face id: the appended rows carry faces `base`
    // does not, so only base's own order among equal face ids can survive it,
    // and that is the order the predecessor preserved too.
    extended.sort_by_key(|eligibility| eligibility.face_id);
    Ok(extended)
}

/// The FU-8b defect (C) algorithm, kept verbatim as the equivalence oracle for
/// [`extend_load_eligibility`].  It is quadratic by construction — that is the
/// defect — so it is only ever run against the synthetic fixtures in
/// `specialized_extension_matches_the_rescanning_algorithm_it_replaced`.
#[cfg(test)]
fn reference_extend_load_eligibility(
    base: &[LoadEligibility],
    mesh_face_count: usize,
    surface: &NativeSurface,
    specialized_owner_faces: &[(u32, u32)],
) -> Result<Vec<LoadEligibility>, GenericPerceptionError> {
    let mut extended = Vec::with_capacity(base.len() + specialized_owner_faces.len());
    for base_row in base {
        let additions = specialized_owner_faces
            .iter()
            .filter(|(_, face_id)| *face_id == base_row.face_id)
            .count();
        let mut owners = Vec::with_capacity(base_row.eligible_owners.len() + additions);
        owners.extend(base_row.eligible_owners.iter().copied());
        owners.extend(
            specialized_owner_faces
                .iter()
                .filter(|(_, face_id)| *face_id == base_row.face_id)
                .map(|(owner_id, _)| LoadOwner::Specialized(*owner_id)),
        );
        owners.sort_unstable();
        owners.dedup();
        extended.push(LoadEligibility {
            face_id: base_row.face_id,
            eligible_owners: owners,
            centroid_mm: base_row.centroid_mm,
            area_mm2: base_row.area_mm2,
        });
    }
    for (pair_index, &(_, face_id)) in specialized_owner_faces.iter().enumerate() {
        if face_id as usize >= mesh_face_count {
            return Err(GenericPerceptionError::SpecializedFaceOutOfRange { face_id });
        }
        if base.iter().any(|row| row.face_id == face_id)
            || specialized_owner_faces[..pair_index]
                .iter()
                .any(|(_, earlier_face)| *earlier_face == face_id)
        {
            continue;
        }
        let owner_count = specialized_owner_faces
            .iter()
            .filter(|(_, candidate_face)| *candidate_face == face_id)
            .count();
        let mut owners = Vec::with_capacity(owner_count);
        owners.extend(
            specialized_owner_faces
                .iter()
                .filter(|(_, candidate_face)| *candidate_face == face_id)
                .map(|(owner_id, _)| LoadOwner::Specialized(*owner_id)),
        );
        owners.sort_unstable();
        owners.dedup();
        extended.push(LoadEligibility {
            face_id,
            eligible_owners: owners,
            centroid_mm: surface.geometry().centroids()[face_id as usize],
            area_mm2: surface.geometry().areas()[face_id as usize],
        });
    }
    extended.sort_by_key(|eligibility| eligibility.face_id);
    Ok(extended)
}

/// Exact live `placement.tip_radius_mm` plus `verify.load_report` predicate.
/// The partition is the sole unique-face load authority; later support
/// verification reuses this function rather than reconstructing capacity.
pub(crate) fn load_violation_count(
    partition: &FinalizedLoadPartition,
    contacts: &[PhysicalContact],
) -> Result<usize, GenericPerceptionError> {
    let contact_count = contacts.len();
    let limits = FinalizedContactLoadLimits::new(
        contact_count,
        partition.rows.len(),
        finalized_contact_load_required_bytes(contact_count)?,
    );
    FinalizedContactLoads::from_physical_contacts(partition, contacts, limits)?
        .load_violation_count()
}

/// What one merge proposal costs the load partition, measured rather than
/// modelled (FU-8d phase F3, part P2).
///
/// The units follow the convention `GateProposalRoutedWork` already uses: one
/// unit per **row** or per **contact** actually visited, with the bounded
/// constant work per visit named in each field's documentation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SpecializedLoadDeltaWork {
    /// Rows in `elig(left) ∪ elig(right)`.  Each pays one merge step, one
    /// witness scan of at most [`LOAD_WITNESS_SLOTS`] slots, one squared
    /// distance against the merged seat, and one comparison.
    pub(crate) eligible_rows: usize,
    /// Rows re-summed to restate a touched contact's carried area **exactly**,
    /// in the finalizer's own ascending row order.  This term exists because
    /// `f64` addition is not associative: an incremental `-= area; += area`
    /// would be cheaper and would not reproduce the fold's bits.
    pub(crate) carried_rows: usize,
    /// Contacts whose carried area moved, and whose tip radius and
    /// safety-factor verdict are therefore re-evaluated.
    pub(crate) touched_contacts: usize,
}

impl SpecializedLoadDeltaWork {
    pub(crate) fn total(self) -> Result<usize, GenericPerceptionError> {
        self.eligible_rows
            .checked_add(self.carried_rows)
            .and_then(|work| work.checked_add(self.touched_contacts))
            .ok_or(GenericPerceptionError::LoadResourceOverflow)
    }

    pub(crate) fn record(&mut self, other: Self) -> Result<(), GenericPerceptionError> {
        self.eligible_rows = self
            .eligible_rows
            .checked_add(other.eligible_rows)
            .ok_or(GenericPerceptionError::LoadResourceOverflow)?;
        self.carried_rows = self
            .carried_rows
            .checked_add(other.carried_rows)
            .ok_or(GenericPerceptionError::LoadResourceOverflow)?;
        self.touched_contacts = self
            .touched_contacts
            .checked_add(other.touched_contacts)
            .ok_or(GenericPerceptionError::LoadResourceOverflow)?;
        Ok(())
    }
}

/// The merge gate's **baseline** load partition, plus everything a merge
/// proposal's delta reads from it (FU-8d phase F3, part P2).
///
/// # What this replaces
///
/// `finalize_specialized_load_partition` ran **once per variant**: a dense
/// `contacts × rows` argmin over the whole extended eligibility, preceded by a
/// per-variant rebuild of the `(owner, face)` pair list and the row extension.
/// On the Weeping Treant that is `(117,741 + 7,971,451) × 1,365 ≈ 1.10 × 10¹⁰`
/// per variant and `39,223` proposal variants — about `4.33 × 10¹⁴`, which is
/// the whole of FU-8d part (2) and was the treant's entire remaining wall.
///
/// # Why a proposal's answer is local, in **both** directions
///
/// A variant replaces contacts `left` and `right` with one merged seat.  Write
/// `elig(c)` for the load rows contact `c` is eligible for.
///
/// - **The merged seat serves exactly `elig(left) ∪ elig(right)`.** Its
///   `(owner, face)` pairs come from `visit_served_owner_faces` over
///   `ContactParts::new(left, Some(right))`, and `parts_serves_owner` is true
///   for an island exactly when `left` or `right` serves it.  So the union is
///   an identity, not a bound.
/// - **A row outside that union cannot move.** Neither member is eligible for
///   it, so removing them removes no candidate; the merged seat is not eligible
///   for it, so it adds none.  Its candidate set is bit-identical to the
///   baseline's, in the same order, so its argmin is the baseline's.
/// - **A row inside the union is answered from three retained witnesses.** Its
///   variant candidate set is `eligible(row) \ {left, right} ∪ {merged}`.  The
///   minimum over the survivors is the first retained witness outside the pair
///   ([`LOAD_WITNESS_SLOTS`] = 3 answers any two-contact removal), and the
///   merged seat contributes one squared distance.
///
/// The row **set** is variant-invariant for the same reason: the variant's rows
/// are the base rows plus the faces of its own pairs, and
/// `∪_survivors served ∪ served(merged) = ∪_all served`.
///
/// # The renumbering invariant, which is what makes the comparison sound
///
/// `append_variant_load_inputs` renumbers per variant: a contact's `contact_id`
/// is its position in that variant's physical list, and its specialized owner
/// id is `first_specialized_owner + contact_id`.  The finalizer's argmin
/// tie-breaks on `contact_id`, so the delta is only sound because the
/// renumbering is **order-preserving on the survivors** — survivors keep their
/// relative order, since the variant emits them in ascending baseline index —
/// **and the merged seat is last**, so it carries the largest id in its variant
/// and loses every exact distance tie to a survivor.  That is exactly how the
/// comparison below breaks a tie, and
/// `fu8d_load_delta_matches_the_per_variant_finalize` pins it with an
/// engineered exact-tie fixture.
///
/// One case would break the locality argument and is refused rather than
/// approximated: if `generic_owner_for_parts` returned `Some` for the merged
/// parts, the merged seat would emit **no** specialized pairs and would instead
/// be eligible for whatever base rows carry that generic island — a set that
/// need not lie inside `elig(left) ∪ elig(right)`, and one that would strand
/// every row only the pair served.  The caller detects it and falls back to a
/// full finalize for that variant; see
/// [`SpecializedLoadBaseline::merged_violation_count`]'s `merged_rows`
/// contract.
pub(crate) struct SpecializedLoadBaseline {
    /// Each load row's carried area, in the finalizer's row order.
    row_area_mm2: Vec<f64>,
    /// Each load row's centroid, in the finalizer's row order.
    row_centroid_mm: Vec<[f64; 3]>,
    /// The three nearest eligible contacts per row, ascending.
    witnesses: Vec<[LoadWitness; LOAD_WITNESS_SLOTS]>,
    /// Baseline winner per row: an index into the baseline contacts.
    winner: Vec<u32>,
    /// CSR over baseline contacts: the rows each one wins, ascending.
    won_starts: Vec<u32>,
    won_rows: Vec<u32>,
    /// Baseline carried area per contact, summed in the fold's own order.
    carried_area_mm2: Vec<f64>,
    /// The baseline variant's own `load_violation_count`.
    violation_count: usize,
    tip_policy: FrozenTipLoadPolicy,
    retained_bytes: usize,
}

/// Reusable per-proposal buffers for the load delta: allocated once for the
/// batch, cleared and refilled per proposal.
///
/// `row_stamp` is the one that must be a batch-level allocation rather than a
/// per-proposal `Vec`: it is `rows`-long, and building it per proposal would
/// trade the quadratic this phase removes for a linear allocation the phase
/// performs 39,223 times.
pub(crate) struct SpecializedLoadDeltaScratch {
    /// `elig(left) ∪ elig(right)`, ascending row indices.
    rows: Vec<u32>,
    /// The variant's winner for each entry of `rows`, positionally.
    new_winner: Vec<u32>,
    /// `row + 1` at the row's position in `rows`, `0` elsewhere.  Cleared by
    /// walking `rows`, never by reallocating.
    row_stamp: Vec<u32>,
    /// Contacts whose carried area moved, ascending, deduplicated.
    touched: Vec<u32>,
    /// Rows each touched contact gains, ascending, in `touched` order.
    gained_starts: Vec<u32>,
    gained_rows: Vec<u32>,
    /// Working carried areas for the touched contacts, in `touched` order.
    carried_scratch: Vec<f64>,
}

impl SpecializedLoadDeltaScratch {
    pub(crate) fn for_rows(row_count: usize) -> Self {
        Self {
            rows: Vec::new(),
            new_winner: Vec::new(),
            row_stamp: vec![0; row_count],
            touched: Vec::new(),
            gained_starts: Vec::new(),
            gained_rows: Vec::new(),
            carried_scratch: Vec::new(),
        }
    }

    pub(crate) fn retained_bytes(&self) -> Result<usize, GenericPerceptionError> {
        self.row_stamp
            .len()
            .checked_mul(std::mem::size_of::<u32>())
            .ok_or(GenericPerceptionError::LoadResourceOverflow)
    }

    /// The rows the last delta answered, ascending — the equivalence test's
    /// handle on "which rows did this variant touch?".
    #[cfg(test)]
    pub(crate) fn answered_rows(&self) -> &[u32] {
        &self.rows
    }

    /// The variant winner the last delta assigned to each entry of
    /// [`Self::answered_rows`], positionally.
    #[cfg(test)]
    pub(crate) fn answered_winners(&self) -> &[u32] {
        &self.new_winner
    }

    /// The contacts the last delta re-evaluated, ascending, paired with their
    /// restated carried areas.
    #[cfg(test)]
    pub(crate) fn touched_carried_areas(&self) -> impl Iterator<Item = (u32, f64)> + '_ {
        self.touched
            .iter()
            .copied()
            .zip(self.carried_scratch.iter().copied())
    }
}

/// One merge proposal's load answer: the variant's violation count and what
/// producing it cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SpecializedLoadDeltaOutcome {
    pub(crate) violation_count: usize,
    pub(crate) work: SpecializedLoadDeltaWork,
}

impl SpecializedLoadBaseline {
    /// Build the merge gate's baseline load state from the **variant 0**
    /// inputs, running the production finalizer exactly once.
    pub(crate) fn build(
        plan: &GenericContactPlan,
        mesh: &VerifiedMeshFingerprint<'_>,
        surface: &NativeSurface,
        contacts: &[PhysicalContact],
        specialized_owner_faces: &[(u32, u32)],
        limits: LoadFinalizeLimits,
    ) -> Result<(FinalizedLoadPartition, Self), GenericPerceptionError> {
        let mut retention = LoadRowRetention {
            witnesses: Vec::new(),
            centroids_mm: Vec::new(),
        };
        let partition = finalize_specialized_load_partition(
            plan,
            mesh,
            surface,
            contacts,
            specialized_owner_faces,
            limits,
            Some(&mut retention),
        )?;
        let loads = FinalizedContactLoads::from_physical_contacts(
            &partition,
            contacts,
            FinalizedContactLoadLimits::new(
                contacts.len(),
                partition.rows.len(),
                finalized_contact_load_required_bytes(contacts.len())?,
            ),
        )?;
        let violation_count = loads.load_violation_count()?;
        // The delta indexes contacts by slot, so the gate's own dense
        // `contact_id == index` numbering is a precondition, not an
        // assumption: `append_variant_load_inputs` emits exactly that, and a
        // partition that does not is refused rather than mis-indexed.
        for (index, contact) in contacts.iter().enumerate() {
            if contact.contact_id != index as u64 {
                return Err(GenericPerceptionError::UnknownFinalizedLoadContact {
                    contact_id: contact.contact_id,
                });
            }
        }

        let row_count = partition.rows.len();
        let mut winner = Vec::with_capacity(row_count);
        let mut won_counts = vec![0u32; contacts.len()];
        for slots in &retention.witnesses {
            if slots[0].contact == NO_LOAD_WITNESS {
                // `finalize_load_rows` has already refused this case with
                // `MissingPhysicalContact`, so it cannot be reached here.
                return Err(GenericPerceptionError::LoadResourceOverflow);
            }
            winner.push(slots[0].contact);
            won_counts[slots[0].contact as usize] += 1;
        }
        let mut won_starts = Vec::with_capacity(contacts.len() + 1);
        let mut running = 0u32;
        for count in &won_counts {
            won_starts.push(running);
            running += count;
        }
        won_starts.push(running);
        let mut fill = won_starts.clone();
        let mut won_rows = vec![0u32; row_count];
        for (row, &owner) in winner.iter().enumerate() {
            won_rows[fill[owner as usize] as usize] = row as u32;
            fill[owner as usize] += 1;
        }

        let row_area_mm2 = partition
            .rows
            .iter()
            .map(|row| row.carried_area_mm2)
            .collect::<Vec<_>>();
        let retained_bytes = [
            (row_count, std::mem::size_of::<f64>()),
            (row_count, std::mem::size_of::<[f64; 3]>()),
            (
                row_count,
                std::mem::size_of::<[LoadWitness; LOAD_WITNESS_SLOTS]>(),
            ),
            (row_count, std::mem::size_of::<u32>()),
            (row_count, std::mem::size_of::<u32>()),
            (contacts.len() + 1, std::mem::size_of::<u32>()),
            (contacts.len(), std::mem::size_of::<f64>()),
        ]
        .into_iter()
        .try_fold(0usize, |total, (count, width)| {
            count
                .checked_mul(width)
                .and_then(|bytes| total.checked_add(bytes))
                .ok_or(GenericPerceptionError::LoadResourceOverflow)
        })?;

        let baseline = Self {
            row_area_mm2,
            row_centroid_mm: retention.centroids_mm,
            witnesses: retention.witnesses,
            winner,
            won_starts,
            won_rows,
            carried_area_mm2: loads.carried_area_mm2().to_vec(),
            violation_count,
            tip_policy: loads.tip_policy,
            retained_bytes,
        };
        Ok((partition, baseline))
    }

    pub(crate) const fn violation_count(&self) -> usize {
        self.violation_count
    }

    pub(crate) const fn row_count(&self) -> usize {
        self.winner.len()
    }

    pub(crate) const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    fn violates(&self, area_mm2: f64) -> Result<bool, GenericPerceptionError> {
        if !area_mm2.is_finite() {
            return Err(GenericPerceptionError::NonFiniteFinalizedLoadTotal);
        }
        let radius = self.tip_policy.tip_radius_mm(area_mm2)?;
        Ok(self.tip_policy.violates_safety_factor(area_mm2, radius))
    }

    /// The variant's `load_violation_count` for the proposal that merges
    /// `left` and `right` into a seat at `merged_point_mm`.
    ///
    /// `merged_rows` must be `elig(left) ∪ elig(right)` as **ascending,
    /// deduplicated** row indices — exactly the rows the merged seat is
    /// eligible for.  The caller owns that derivation because it owns the
    /// `(owner, face)` pair list the union is read from, and because it is the
    /// caller that can see the one case this path must not be used for: a
    /// merged seat whose parts resolve to a generic owner, which serves base
    /// rows rather than the union.
    pub(crate) fn merged_violation_count(
        &self,
        left: usize,
        right: usize,
        merged_point_mm: [f64; 3],
        merged_rows: &[u32],
        scratch: &mut SpecializedLoadDeltaScratch,
    ) -> Result<SpecializedLoadDeltaOutcome, GenericPerceptionError> {
        debug_assert!(merged_rows.windows(2).all(|pair| pair[0] < pair[1]));
        let merged_seat = u32::try_from(self.carried_area_mm2.len())
            .map_err(|_| GenericPerceptionError::LoadResourceOverflow)?;
        let (left, right) = (left as u32, right as u32);

        scratch.rows.clear();
        scratch.rows.extend_from_slice(merged_rows);
        scratch.new_winner.clear();
        scratch.touched.clear();
        scratch.gained_starts.clear();
        scratch.gained_rows.clear();
        scratch.carried_scratch.clear();

        // Stamp the touched rows so a baseline row list can be filtered in O(1)
        // per row rather than by a second merge against `rows`.
        for (position, &row) in scratch.rows.iter().enumerate() {
            scratch.row_stamp[row as usize] = position as u32 + 1;
        }

        // Phase 1 — answer every touched row.
        for &row in &scratch.rows {
            let survivor = self.witnesses[row as usize]
                .iter()
                .find(|witness| {
                    witness.contact != NO_LOAD_WITNESS
                        && witness.contact != left
                        && witness.contact != right
                })
                .copied();
            let merged_squared = merged_point_mm
                .iter()
                .zip(self.row_centroid_mm[row as usize])
                .map(|(left, right)| (left - right).powi(2))
                .sum::<f64>();
            // The merged seat carries the largest `contact_id` in its variant,
            // so it replaces a survivor only on a *strictly* smaller distance.
            // That is the finalizer's own `(distance, contact_id)` order with
            // the renumbering's "merged seat last" fact substituted in.
            let winner = match survivor {
                Some(witness) if witness.squared_distance.total_cmp(&merged_squared).is_le() => {
                    witness.contact
                }
                _ => merged_seat,
            };
            scratch.new_winner.push(winner);
        }

        // Phase 2 — which contacts moved.  A row's baseline holder loses it
        // whenever the variant's winner differs; the variant's winner gains it.
        for (position, &row) in scratch.rows.iter().enumerate() {
            let before = self.winner[row as usize];
            let after = scratch.new_winner[position];
            if before != after {
                scratch.touched.push(before);
                scratch.touched.push(after);
            }
        }
        scratch.touched.push(left);
        scratch.touched.push(right);
        scratch.touched.push(merged_seat);
        scratch.touched.sort_unstable();
        scratch.touched.dedup();
        // `left` and `right` leave the variant entirely; they are carried into
        // `touched` only so their baseline violations are subtracted.
        let touched_count = scratch.touched.len();

        // Phase 3 — gather each touched contact's gained rows, ascending.
        let mut gained_counts = vec![0u32; touched_count];
        for (position, &winner) in scratch.new_winner.iter().enumerate() {
            let row = scratch.rows[position];
            if self.winner[row as usize] == winner {
                continue;
            }
            let slot = scratch
                .touched
                .binary_search(&winner)
                .map_err(|_| GenericPerceptionError::LoadResourceOverflow)?;
            gained_counts[slot] += 1;
        }
        let mut running = 0u32;
        for count in &gained_counts {
            scratch.gained_starts.push(running);
            running += count;
        }
        scratch.gained_starts.push(running);
        let mut fill = scratch.gained_starts.clone();
        scratch.gained_rows.resize(running as usize, 0);
        for (position, &winner) in scratch.new_winner.iter().enumerate() {
            let row = scratch.rows[position];
            if self.winner[row as usize] == winner {
                continue;
            }
            let slot = scratch
                .touched
                .binary_search(&winner)
                .map_err(|_| GenericPerceptionError::LoadResourceOverflow)?;
            scratch.gained_rows[fill[slot] as usize] = row;
            fill[slot] += 1;
        }

        // Phase 4 — restate each touched contact's carried area exactly, by
        // re-summing its variant rows in ascending row order.  That is the
        // order `FinalizedContactLoads` folds them in, and `f64` addition is
        // not associative, so an incremental adjustment would be cheaper and
        // wrong.
        let mut carried_rows = 0usize;
        let mut violation_count = self.violation_count;
        for (slot, &contact) in scratch.touched.iter().enumerate() {
            let before = if contact == merged_seat {
                0.0
            } else {
                self.carried_area_mm2[contact as usize]
            };
            let mut after = 0.0_f64;
            if contact != left && contact != right {
                let gained = &scratch.gained_rows[scratch.gained_starts[slot] as usize
                    ..scratch.gained_starts[slot + 1] as usize];
                let kept: &[u32] = if contact == merged_seat {
                    &[]
                } else {
                    &self.won_rows[self.won_starts[contact as usize] as usize
                        ..self.won_starts[contact as usize + 1] as usize]
                };
                let mut gained_index = 0usize;
                for &row in kept {
                    let stamp = scratch.row_stamp[row as usize];
                    if stamp != 0 && scratch.new_winner[stamp as usize - 1] != contact {
                        // The variant took this row away from its baseline
                        // holder.
                        continue;
                    }
                    while gained_index < gained.len() && gained[gained_index] < row {
                        after += self.row_area_mm2[gained[gained_index] as usize];
                        gained_index += 1;
                    }
                    after += self.row_area_mm2[row as usize];
                }
                for &row in &gained[gained_index..] {
                    after += self.row_area_mm2[row as usize];
                }
                carried_rows = carried_rows
                    .checked_add(kept.len())
                    .and_then(|total| total.checked_add(gained.len()))
                    .ok_or(GenericPerceptionError::LoadResourceOverflow)?;
            }
            scratch.carried_scratch.push(after);
            let was = self.violates(before)?;
            let now = if contact == left || contact == right {
                // Removed from the variant: it has no tip to violate.
                false
            } else {
                self.violates(after)?
            };
            violation_count = match (was, now) {
                (true, false) => violation_count
                    .checked_sub(1)
                    .ok_or(GenericPerceptionError::LoadResourceOverflow)?,
                (false, true) => violation_count
                    .checked_add(1)
                    .ok_or(GenericPerceptionError::LoadResourceOverflow)?,
                _ => violation_count,
            };
        }

        for &row in &scratch.rows {
            scratch.row_stamp[row as usize] = 0;
        }

        Ok(SpecializedLoadDeltaOutcome {
            violation_count,
            work: SpecializedLoadDeltaWork {
                eligible_rows: scratch.rows.len(),
                carried_rows,
                touched_contacts: touched_count,
            },
        })
    }
}

#[cfg_attr(
    not(test),
    allow(dead_code, reason = "used by the crate-private load finalizer")
)]
fn hash_owner(hash: &mut Fnv, owner: LoadOwner) {
    match owner {
        LoadOwner::Specialized(id) => {
            hash.u8(0);
            hash.u32(id);
        }
        LoadOwner::GenericIsland(id) => {
            hash.u8(1);
            hash.u32(id);
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the fingerprint must bind every result-shaping input without hiding an authority source"
)]
fn authority_fingerprint(
    mesh: &Mesh,
    surface: &NativeSurface,
    config: &SupportPerceptionConfig<'_>,
    frame_value: BuildFrame,
    layers: &LayerPlan,
    candidate_config: &CandidatePayloadConfig,
    specialized: &[SpecializedOwnerReservation],
    limits: GenericPerceptionLimits,
) -> Result<AuthorityFingerprint, GenericPerceptionError> {
    let mesh = mesh_fingerprint(mesh)
        .map_err(|error| GenericPerceptionError::Topology(error.to_string()))?;
    let mut native = Fnv::new();
    native.f64(config.surface_grid_pitch_mm.get());
    native.usize(config.max_grid_bytes);
    for value in surface
        .geometry()
        .normals()
        .iter()
        .flatten()
        .chain(surface.geometry().areas().iter())
    {
        native.f64(*value);
    }
    for value in surface.exterior_face_mask() {
        native.u8(u8::from(*value));
    }
    let mut result_config = Fnv::new();
    result_config.f64(config.column_pitch_mm.get());
    result_config.f64(config.overhang_angle_deg.get());
    result_config.f64(config.bridge_free_gap_mm.get());
    result_config.f64(config.near_plate_mm.get());
    if let Some(delicacy) = config.delicacy_by_face {
        for value in delicacy {
            result_config.f64(*value);
        }
    }
    let (depth, virtual_cells, candidates, compact_bytes, working_cells, edge_bits) =
        candidate_config.authority_fields();
    result_config.u32(depth);
    result_config.usize(virtual_cells);
    result_config.usize(candidates);
    result_config.usize(compact_bytes);
    result_config.usize(working_cells);
    result_config.u64(edge_bits);
    result_config.f64(config.plate_z_mm.get());
    result_config.usize(limits.max_planes);
    result_config.usize(limits.max_triangle_plane_tests);
    result_config.usize(limits.max_slice_intersections);
    result_config.usize(limits.max_slice_output_bytes);
    result_config.usize(limits.max_loop_vertices);
    result_config.usize(limits.max_minima_vertices);
    result_config.usize(limits.max_island_neighbor_pairs);
    result_config.usize(limits.max_island_neighbor_bytes);
    result_config.usize(limits.max_overlap_pairs);
    result_config.usize(limits.max_overlap_bytes);
    result_config.usize(limits.max_overlap_vertex_tests);
    result_config.usize(limits.max_overlap_edge_tests);
    let mut frame = Fnv::new();
    frame.f64(frame_value.plate_z_mm().get());
    frame.f64(frame_value.lift_mm().get());
    let mut layer_hash = Fnv::new();
    layer_hash.f64(layers.z0_mm());
    for value in layers.heights_mm() {
        layer_hash.f64(*value);
    }
    let mut specialized_hash = Fnv::new();
    let mut canonical_specialized = specialized.to_vec();
    canonical_specialized.sort_unstable();
    for reservation in canonical_specialized {
        specialized_hash.u32(reservation.owner_id);
        specialized_hash.u32(reservation.face_id);
    }
    Ok(AuthorityFingerprint {
        mesh,
        native: native.finish(),
        config: result_config.finish(),
        frame: frame.finish(),
        layers: layer_hash.finish(),
        specialized: specialized_hash.finish(),
    })
}

struct Fnv(u64);
impl Fnv {
    const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    fn bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
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
    const fn finish(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use geom_kernel::KnnBatchLimits;
    use plinth_core::{Finite, PositiveFinite};
    use plinth_mesh::{make_overhang_table, make_raking_blade, make_thin_fin, make_toed_slab};

    const SWEEP_CHILD: &str = "PLINTH_PERCEPTION_SWEEP_CHILD";
    const SWEEP_LINE: &str = "PLINTH_PERCEPTION_SWEEP=";

    const TEST_KNN_LIMITS: KnnBatchLimits = KnnBatchLimits {
        max_source_points: 20_000_000,
        max_query_points: 20_000_000,
        max_neighbors: 1,
        max_logical_peak_bytes: 2 * 1024 * 1024 * 1024,
        max_work: 100_000_000,
    };

    fn test_partition(area_mm2: f64) -> FinalizedLoadPartition {
        test_partition_rows(vec![FinalizedFaceLoad {
            face_id: 0,
            contact_id: 0,
            owner: LoadOwner::GenericIsland(0),
            carried_area_mm2: area_mm2,
        }])
    }

    fn test_partition_rows(rows: Vec<FinalizedFaceLoad>) -> FinalizedLoadPartition {
        FinalizedLoadPartition {
            authority: AuthorityFingerprint {
                mesh: String::new(),
                native: 0,
                config: 0,
                frame: 0,
                layers: 0,
                specialized: 0,
            },
            contacts_fingerprint: 0,
            rows,
        }
    }

    fn final_load_limits(contact_count: usize, row_count: usize) -> FinalizedContactLoadLimits {
        FinalizedContactLoadLimits::new(
            contact_count,
            row_count,
            finalized_contact_load_required_bytes(contact_count).unwrap(),
        )
    }

    fn test_contacts(ids: &[u64]) -> Vec<PhysicalContact> {
        ids.iter()
            .copied()
            .map(|contact_id| PhysicalContact {
                contact_id,
                owner: LoadOwner::GenericIsland(0),
                point_mm: [0.0; 3],
            })
            .collect()
    }

    #[test]
    fn finalized_contact_loads_preserve_zero_load_contacts_and_frozen_tip_clamps() {
        let empty = test_partition_rows(Vec::new());
        let contact_ids = [100, 10, u64::MAX];
        let empty_loads =
            FinalizedContactLoads::from_partition(&empty, &contact_ids, final_load_limits(3, 0))
                .unwrap();
        assert_eq!(empty_loads.contact_ids(), &contact_ids);
        assert_eq!(empty_loads.carried_area_mm2(), &[0.0, 0.0, 0.0]);
        assert_eq!(
            empty_loads
                .tip_radii_mm()
                .iter()
                .map(|radius| radius.get())
                .collect::<Vec<_>>(),
            vec![0.2, 0.2, 0.2]
        );

        let policy = m0_constants().unwrap().placeholders();
        let in_band_radius_mm: f64 = 0.5;
        let in_band_area_mm2 = (in_band_radius_mm / 1.0e3).powi(2)
            * std::f64::consts::PI
            * (policy.tensile_green_mpa().diagnostic().get() * 1.0e6)
            / (policy.peel_pressure_pa().diagnostic().get()
                * policy.tip_safety_factor().diagnostic().get())
            * 1.0e6;
        let in_band = test_partition(in_band_area_mm2);
        let in_band_loads =
            FinalizedContactLoads::from_partition(&in_band, &[0], final_load_limits(1, 1)).unwrap();
        assert!((in_band_loads.tip_radii_mm()[0].get() - in_band_radius_mm).abs() < 1.0e-12);

        let capped = test_partition(in_band_area_mm2 * 16.0);
        let capped_loads =
            FinalizedContactLoads::from_partition(&capped, &[0], final_load_limits(1, 1)).unwrap();
        assert_eq!(capped_loads.tip_radii_mm()[0].get(), 1.0);
        assert_eq!(capped_loads.load_violation_count(), Ok(1));
    }

    #[test]
    fn finalized_contact_loads_fold_frozen_rows_by_contact_and_bind_their_authority() {
        let partition = test_partition_rows(vec![
            FinalizedFaceLoad {
                face_id: 8,
                contact_id: 10,
                owner: LoadOwner::Specialized(4),
                carried_area_mm2: 3.0,
            },
            FinalizedFaceLoad {
                face_id: 2,
                contact_id: 100,
                owner: LoadOwner::GenericIsland(1),
                carried_area_mm2: 2.0,
            },
            FinalizedFaceLoad {
                face_id: 9,
                contact_id: 10,
                owner: LoadOwner::Specialized(4),
                carried_area_mm2: 5.0,
            },
        ]);
        let contact_ids = [100, 10, u64::MAX];
        let loads = FinalizedContactLoads::from_partition(
            &partition,
            &contact_ids,
            final_load_limits(3, 3),
        )
        .unwrap();
        assert_eq!(loads.contact_ids(), &contact_ids);
        assert_eq!(loads.carried_area_mm2(), &[2.0, 8.0, 0.0]);
        assert!(loads.validate_binding(&partition, &contact_ids).is_ok());

        let mut stale = partition.clone();
        stale.rows[0].carried_area_mm2 = 4.0;
        assert_eq!(
            loads.validate_binding(&stale, &contact_ids),
            Err(GenericPerceptionError::StaleAuthority)
        );
        let reordered_ids = [10, 100, u64::MAX];
        let reordered = FinalizedContactLoads::from_partition(
            &partition,
            &reordered_ids,
            final_load_limits(3, 3),
        )
        .unwrap();
        assert_eq!(reordered.contact_ids(), &reordered_ids);
        assert_eq!(reordered.carried_area_mm2(), &[8.0, 2.0, 0.0]);
        assert_ne!(loads.digest(), reordered.digest());
        assert_eq!(
            loads.validate_binding(&partition, &reordered_ids),
            Err(GenericPerceptionError::StaleAuthority)
        );
    }

    #[test]
    fn finalized_contact_loads_refuse_unknown_or_over_budget_input_before_output_allocation() {
        let unknown = test_partition_rows(vec![FinalizedFaceLoad {
            face_id: 0,
            contact_id: 2,
            owner: LoadOwner::GenericIsland(0),
            carried_area_mm2: 1.0,
        }]);
        assert_eq!(
            FinalizedContactLoads::from_partition(&unknown, &[100, 10], final_load_limits(2, 1)),
            Err(GenericPerceptionError::UnknownFinalizedLoadContact { contact_id: 2 })
        );
        assert_eq!(
            FinalizedContactLoads::from_partition(
                &test_partition_rows(Vec::new()),
                &[5, 5],
                final_load_limits(2, 0),
            ),
            Err(GenericPerceptionError::DuplicateFinalizedContactId { contact_id: 5 })
        );
        assert_eq!(
            FinalizedContactLoads::from_partition(
                &unknown,
                &[100, 10, 2],
                FinalizedContactLoadLimits::new(2, 1, usize::MAX),
            ),
            Err(GenericPerceptionError::FinalizedLoadContactLimit {
                required: 3,
                limit: 2,
            })
        );
        assert_eq!(
            FinalizedContactLoads::from_partition(
                &unknown,
                &[100, 10, 2],
                FinalizedContactLoadLimits::new(3, 0, usize::MAX),
            ),
            Err(GenericPerceptionError::FinalizedLoadRowLimit {
                required: 1,
                limit: 0,
            })
        );

        for (area_mm2, expected) in [
            (
                f64::NAN,
                GenericPerceptionError::NonFiniteFinalizedLoad { contact_id: 0 },
            ),
            (
                -1.0,
                GenericPerceptionError::NegativeFinalizedLoad { contact_id: 0 },
            ),
        ] {
            let invalid = test_partition(area_mm2);
            assert_eq!(
                FinalizedContactLoads::from_partition(&invalid, &[0], final_load_limits(1, 1)),
                Err(expected)
            );
        }
    }

    #[test]
    fn finalized_contact_load_digest_is_serially_repeatable() {
        let partition = test_partition_rows(vec![FinalizedFaceLoad {
            face_id: 0,
            contact_id: 0,
            owner: LoadOwner::GenericIsland(0),
            carried_area_mm2: 4.0,
        }]);
        let digests = std::array::from_fn::<_, 3, _>(|_| {
            FinalizedContactLoads::from_partition(&partition, &[0], final_load_limits(1, 1))
                .unwrap()
                .digest()
        });
        assert_eq!(digests, [digests[0]; 3]);
    }

    #[test]
    fn shared_load_gate_pins_the_live_one_e_minus_six_tolerance() {
        let policy = m0_constants().unwrap().placeholders();
        let pressure = policy.peel_pressure_pa().diagnostic().get();
        let tensile_pa = policy.tensile_green_mpa().diagnostic().get() * 1.0e6;
        let required = policy.tip_safety_factor().diagnostic().get();
        let radius_m = policy.tip_radius_max_legacy_mm().diagnostic().get() / 1.0e3;
        let boundary = tensile_pa * std::f64::consts::PI * radius_m.powi(2)
            / ((required - 1.0e-6) * pressure)
            * 1.0e6;
        let contacts = test_contacts(&[0]);
        assert_eq!(
            load_violation_count(&test_partition(boundary), &contacts),
            Ok(0)
        );
        assert_eq!(
            load_violation_count(
                &test_partition(f64::from_bits(boundary.to_bits() + 1)),
                &contacts,
            ),
            Ok(1)
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

    fn layer_plan() -> LayerPlan {
        LayerPlan::new(0.0, vec![0.05; 1_000]).unwrap()
    }

    fn frame() -> BuildFrame {
        BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap())
    }

    fn covering_layer_plan(mesh: &Mesh) -> LayerPlan {
        let zmin = mesh
            .verts()
            .iter()
            .map(|point| point[2])
            .fold(f64::INFINITY, f64::min);
        let zmax = mesh
            .verts()
            .iter()
            .map(|point| point[2])
            .fold(f64::NEG_INFINITY, f64::max);
        let count = ((zmax - zmin) / 0.05).ceil() as usize + 1;
        LayerPlan::new(zmin, vec![0.05; count]).unwrap()
    }

    #[test]
    fn frozen_birth_overlap_ignores_retained_negative_winding_holes_like_python() {
        let square =
            |x0: f64, y0: f64, x1: f64, y1: f64| vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
        let outer = SliceLoop {
            points_xy_mm: square(0.0, 0.0, 4.0, 4.0),
            signed_area_mm2: 16.0,
            hole: false,
        };
        let hole = SliceLoop {
            points_xy_mm: square(1.0, 1.0, 2.0, 2.0),
            signed_area_mm2: -1.0,
            hole: true,
        };
        let inside_prior_hole = SliceLoop {
            points_xy_mm: vec![[1.1, 1.1], [1.9, 1.1], [1.9, 1.3], [1.1, 1.8]],
            signed_area_mm2: 0.36,
            hole: false,
        };
        let disjoint_asymmetric = SliceLoop {
            points_xy_mm: vec![[5.1, 1.1], [5.9, 1.1], [5.9, 1.3], [5.1, 1.8]],
            signed_area_mm2: 0.36,
            hole: false,
        };
        let first = SliceSolid {
            outer: outer.clone(),
            holes: vec![hole],
        };
        assert_eq!(first.holes.len(), 1, "hole remains retained as evidence");
        let births = find_births(
            &[0, 1],
            &[0.025, 0.075],
            &[
                vec![first.clone()],
                vec![
                    first,
                    SliceSolid {
                        outer: inside_prior_hole,
                        holes: vec![],
                    },
                    SliceSolid {
                        outer: disjoint_asymmetric,
                        holes: vec![],
                    },
                ],
            ],
            GenericPerceptionLimits {
                max_overlap_pairs: 16,
                max_overlap_bytes: 4096,
                ..GenericPerceptionLimits::production(TEST_KNN_LIMITS)
            },
        )
        .unwrap();
        assert_eq!(births.len(), 2);
        assert_eq!(births[0].centroid_xy_mm, [2.0, 2.0]);
        assert!(
            !births
                .iter()
                .any(|birth| birth.centroid_xy_mm == [1.5, 1.325]),
            "a current outer inside the prior outer is overlapping in the live Python predicate, even when it lies in a retained hole"
        );
        // Boundary-point mean, matching frozen Path2D.discrete behavior;
        // deliberately asymmetric so an area centroid cannot pass.
        assert_eq!(births[1].centroid_xy_mm, [5.5, 1.325]);
    }

    #[test]
    fn hollow_adequacy_uses_positive_outer_area_even_when_point_is_in_hole() {
        let solid = SliceSolid {
            outer: SliceLoop {
                points_xy_mm: vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]],
                signed_area_mm2: 16.0,
                hole: false,
            },
            holes: vec![SliceLoop {
                points_xy_mm: vec![[1.0, 1.0], [1.0, 3.0], [3.0, 3.0], [3.0, 1.0]],
                signed_area_mm2: -4.0,
                hole: true,
            }],
        };
        assert_eq!(carried_outer_area(&[solid], [2.0, 2.0]), 16.0);
    }

    #[test]
    fn exterior_filter_uses_one_all_face_query_and_global_face_id_tie() {
        let minimum = LocalZMinimum {
            position_mm: [0.0, 0.0, 0.0],
            normal: [0.0, 0.0, -1.0],
            region_points_mm: vec![[0.0, 0.0, 0.0]],
        };
        let centroids = [[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        assert_eq!(
            filter_minima_to_exterior_faces(
                &centroids,
                &[false, true],
                vec![minimum.clone()],
                GenericPerceptionLimits::production(TEST_KNN_LIMITS),
            )
            .unwrap(),
            Vec::<LocalZMinimum>::new()
        );
        assert_eq!(
            filter_minima_to_exterior_faces(
                &centroids,
                &[true, false],
                vec![minimum.clone()],
                GenericPerceptionLimits::production(TEST_KNN_LIMITS),
            )
            .unwrap(),
            vec![minimum]
        );
    }

    #[test]
    fn exterior_filter_knn_caps_refuse_before_query_gather_and_larger_caps_preserve_rows() {
        let minimum = LocalZMinimum {
            position_mm: [0.0, 0.0, 0.0],
            normal: [0.0, 0.0, -1.0],
            region_points_mm: vec![[0.0, 0.0, 0.0]],
        };
        let centroids = [[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        let exact = GenericPerceptionLimits {
            minima_knn: KnnBatchLimits {
                max_source_points: 2,
                max_query_points: 1,
                max_neighbors: 1,
                max_logical_peak_bytes: 1024,
                max_work: 2,
            },
            ..GenericPerceptionLimits::production(TEST_KNN_LIMITS)
        };
        let expected = filter_minima_to_exterior_faces(
            &centroids,
            &[true, false],
            vec![minimum.clone()],
            exact,
        )
        .unwrap();
        assert_eq!(
            filter_minima_to_exterior_faces(
                &centroids,
                &[true, false],
                vec![minimum.clone()],
                GenericPerceptionLimits {
                    minima_knn: KnnBatchLimits {
                        max_query_points: 0,
                        ..exact.minima_knn
                    },
                    ..exact
                },
            ),
            Err(GenericPerceptionError::PointIndex(
                "kNN query count 1 exceeds limit 0".to_owned()
            ))
        );
        assert_eq!(
            filter_minima_to_exterior_faces(
                &centroids,
                &[true, false],
                vec![minimum],
                GenericPerceptionLimits {
                    minima_knn: KnnBatchLimits {
                        max_source_points: 3,
                        max_query_points: 2,
                        max_neighbors: 2,
                        max_logical_peak_bytes: 2048,
                        max_work: 4,
                    },
                    ..exact
                },
            )
            .unwrap(),
            expected
        );
    }

    #[test]
    fn island_neighbor_dense_output_accepts_exact_caps_and_rejects_cap_plus_one() {
        let mesh = plinth_mesh::make_box([0.01, 0.01, 0.01], [0.0; 3]).unwrap();
        let delicacy = vec![0.0; mesh.tris().len()];
        let config = perception_config(&delicacy);
        let surface = build_surface(&mesh, &config).unwrap();
        let topology = plinth_mesh::build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let raw = ContactCandidatePool {
            candidates: (0..mesh.tris().len())
                .map(|face| ContactCandidate {
                    id: ContactCandidateId(face as u64),
                    face_id: face as u32,
                    point_mm: surface.geometry().centroids()[face],
                    normal: surface.geometry().normals()[face],
                    supported_area_mm2: surface.geometry().areas()[face],
                    overhang_excess: 1.0,
                    delicacy: 0.0,
                })
                .collect(),
        };
        let count = mesh.tris().len();
        let pairs = count * count;
        let bytes = (count + 1) * std::mem::size_of::<usize>()
            + pairs * (std::mem::size_of::<u64>() + std::mem::size_of::<f64>());
        let exact = GenericPerceptionLimits {
            max_island_neighbor_pairs: pairs,
            max_island_neighbor_bytes: bytes,
            ..GenericPerceptionLimits::production(TEST_KNN_LIMITS)
        };
        assert_eq!(
            connected_islands(&raw, &topology, &surface, exact)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            connected_islands(
                &raw,
                &topology,
                &surface,
                GenericPerceptionLimits {
                    max_island_neighbor_pairs: pairs - 1,
                    ..exact
                }
            ),
            Err(GenericPerceptionError::IslandNeighborPairLimit {
                required: pairs,
                limit: pairs - 1,
            })
        );
        assert_eq!(
            connected_islands(
                &raw,
                &topology,
                &surface,
                GenericPerceptionLimits {
                    max_island_neighbor_bytes: bytes - 1,
                    ..exact
                }
            ),
            Err(GenericPerceptionError::IslandNeighborByteLimit {
                required: bytes,
                limit: bytes - 1,
            })
        );
    }

    #[test]
    fn overlap_sampling_enforces_exact_vertex_and_edge_operation_caps() {
        let solid = |points_xy_mm: Vec<[f64; 2]>| SliceSolid {
            outer: SliceLoop {
                signed_area_mm2: signed_area(&points_xy_mm),
                points_xy_mm,
                hole: false,
            },
            holes: vec![],
        };
        // Bboxes overlap, while the triangles have a deterministic gap.
        let a = solid(vec![[0.0, 0.0], [2.0, 0.0], [0.0, 2.0]]);
        let b = solid(vec![[2.0, 2.0], [2.0, 0.5], [0.5, 2.0]]);
        let mut exact = OverlapOperationBudget {
            vertex_tests: 0,
            edge_tests: 0,
            max_vertex_tests: 6,
            max_edge_tests: 9,
        };
        assert!(!solids_overlap(&a, &b, &mut exact).unwrap());
        assert_eq!((exact.vertex_tests, exact.edge_tests), (6, 9));

        let mut vertex_short = OverlapOperationBudget {
            max_vertex_tests: 5,
            ..exact
        };
        vertex_short.vertex_tests = 0;
        vertex_short.edge_tests = 0;
        assert_eq!(
            solids_overlap(&a, &b, &mut vertex_short),
            Err(GenericPerceptionError::OverlapVertexTestLimit {
                required: 6,
                limit: 5,
            })
        );
        let mut edge_short = OverlapOperationBudget {
            vertex_tests: 0,
            edge_tests: 0,
            max_vertex_tests: 6,
            max_edge_tests: 8,
        };
        assert_eq!(
            solids_overlap(&a, &b, &mut edge_short),
            Err(GenericPerceptionError::OverlapEdgeTestLimit {
                required: 9,
                limit: 8,
            })
        );
    }

    #[test]
    fn frozen_segment_vector_deduplicates_shared_owner_and_stitches_one_loop() {
        use geom_kernel::{PlaneIntersection, PlaneIntersectionKind};
        let edge = |face_id, a: [f64; 2], b: [f64; 2]| PlaneIntersection {
            face_id,
            kind: PlaneIntersectionKind::Basic,
            segment: [[a[0], a[1], 0.5], [b[0], b[1], 0.5]],
        };
        let events = vec![
            edge(0, [0.0, 0.0], [2.0, 0.0]),
            edge(1, [2.0, 0.0], [2.0, 2.0]),
            edge(2, [2.0, 2.0], [0.0, 2.0]),
            edge(3, [0.0, 2.0], [0.0, 0.0]),
            // Frozen native slicing deliberately retains duplicate positive
            // edge owners. Loop assembly owns this deterministic dedup.
            edge(4, [2.0, 0.0], [0.0, 0.0]),
        ];
        let loops = stitch_layer(SliceStitchSite::PrintLayer(7), &events).unwrap();
        assert_eq!(loops.len(), 1);
        assert_eq!(
            loops[0].outer().points_xy_mm(),
            &[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]
        );
        assert_eq!(loops[0].area_mm2(), 4.0);
        assert!(loops[0].holes().is_empty());
    }

    /// A self-crossing outer contour makes even-odd containment intransitive:
    /// the island is inside the middle loop, the middle loop is inside the
    /// outer loop, and yet the island tests OUTSIDE the outer loop because it
    /// lands in the outer loop's doubly-covered lobe. Parity therefore calls
    /// the island a hole that no solid contains. That used to abort the entire
    /// support run with `OpenSliceSeam`; the stitcher must now hand back
    /// solids.
    #[test]
    fn self_crossing_outer_contour_still_yields_solids_not_a_seam_error() {
        use geom_kernel::{PlaneIntersection, PlaneIntersectionKind};
        let mut face_id = 0u32;
        let mut ring = |points: &[[f64; 2]]| {
            (0..points.len())
                .map(|index| {
                    let a = points[index];
                    let b = points[(index + 1) % points.len()];
                    let event = PlaneIntersection {
                        face_id,
                        kind: PlaneIntersectionKind::Basic,
                        segment: [[a[0], a[1], 0.5], [b[0], b[1], 0.5]],
                    };
                    face_id += 1;
                    event
                })
                .collect::<Vec<_>>()
        };
        // Outer: one stroke that folds back over itself, so edge (6,0)-(6,4)
        // crosses edge (2,2)-(8,2) at (6,2) and the block x in [2,6],
        // y in [2,4] is covered twice - even-odd "outside".
        let mut events = ring(&[
            [0.0, 0.0],
            [6.0, 0.0],
            [6.0, 4.0],
            [2.0, 4.0],
            [2.0, 2.0],
            [8.0, 2.0],
            [8.0, 6.0],
            [0.0, 6.0],
        ]);
        // Middle: straddles the fold. Its interior probe lands below y = 2, so
        // parity puts it inside the outer loop - depth 1, a hole.
        events.extend(ring(&[[3.0, 0.2], [5.0, 0.2], [5.0, 2.4], [3.0, 2.4]]));
        // Island: wholly inside the middle loop, but inside the doubly-covered
        // block, so it tests outside the outer loop - depth 1, also a hole,
        // and no solid contains any of its points.
        events.extend(ring(&[[3.4, 2.05], [4.6, 2.05], [4.6, 2.35], [3.4, 2.35]]));

        let solids = stitch_layer(SliceStitchSite::PrintLayer(419), &events)
            .expect("a self-crossing contour is survivable, not a seam violation");
        assert_eq!(solids.len(), 1);
        assert_eq!(solids[0].outer().signed_area_mm2(), 52.0);
        // Both holes are claimed: the middle one by direct containment, the
        // island by the bounding-box fallback.
        assert_eq!(solids[0].holes().len(), 2);
        let areas = solids[0]
            .holes()
            .iter()
            .map(|hole| (hole.signed_area_mm2() * 1.0e6).round() / 1.0e6)
            .collect::<Vec<_>>();
        assert_eq!(areas, vec![-4.4, -0.36]);
    }

    #[test]
    fn a_hole_no_solid_can_reach_is_discarded_rather_than_raised() {
        let square = |x: f64, y: f64, size: f64| {
            vec![[x, y], [x + size, y], [x + size, y + size], [x, y + size]]
        };
        let solid = SliceSolid {
            outer: SliceLoop {
                points_xy_mm: square(0.0, 0.0, 1.0),
                signed_area_mm2: 1.0,
                hole: false,
            },
            holes: Vec::new(),
        };
        let reachable = SliceLoop {
            points_xy_mm: square(0.2, 0.2, 0.4),
            signed_area_mm2: -0.16,
            hole: true,
        };
        let unreachable = SliceLoop {
            points_xy_mm: square(10.0, 10.0, 0.4),
            signed_area_mm2: -0.16,
            hole: true,
        };
        let solids = std::slice::from_ref(&solid);
        assert_eq!(hole_owner(solids, &reachable), Some(0));
        assert_eq!(hole_owner(solids, &unreachable), None);
    }

    #[test]
    fn interior_probe_leaves_the_boundary_even_when_the_centroid_does_not_land_inside() {
        // A C-shape: the area centroid falls in the notch, outside the loop.
        let c_shape = [
            [0.0, 0.0],
            [3.0, 0.0],
            [3.0, 1.0],
            [1.0, 1.0],
            [1.0, 2.0],
            [3.0, 2.0],
            [3.0, 3.0],
            [0.0, 3.0],
        ];
        let centroid = area_centroid(&c_shape, signed_area(&c_shape)).expect("non-degenerate");
        assert!(!point_in_polygon(centroid, &c_shape));
        let probe = interior_probe(&c_shape);
        assert!(point_in_polygon(probe, &c_shape));
        assert!(c_shape.iter().all(|vertex| *vertex != probe));
    }

    #[test]
    fn an_adequacy_probe_seam_no_longer_masquerades_as_a_print_layer() {
        assert_eq!(
            SliceStitchSite::PrintLayer(9).open_seam(),
            GenericPerceptionError::OpenSliceSeam { layer: 9 }
        );
        assert_eq!(
            SliceStitchSite::AdequacyProbe(9).open_seam(),
            GenericPerceptionError::OpenAdequacyProbeSeam { probe_row: 9 }
        );
    }

    #[test]
    fn unique_face_partition_is_finalized_from_physical_contacts() {
        let mesh = make_overhang_table().unwrap();
        let delicacy = vec![0.0; mesh.tris().len()];
        let plan = prepare_generic_contacts(
            &mesh,
            &perception_config(&delicacy),
            frame(),
            &layer_plan(),
            &candidate_config(),
            &[],
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
        )
        .unwrap();
        let contacts = plan
            .islands()
            .iter()
            .flat_map(|island| {
                [
                    PhysicalContact {
                        contact_id: u64::from(island.id) + 100,
                        owner: LoadOwner::GenericIsland(island.id),
                        point_mm: [0.0, 0.0, 0.0],
                    },
                    PhysicalContact {
                        contact_id: u64::from(island.id) + 10,
                        owner: LoadOwner::GenericIsland(island.id),
                        point_mm: [0.0, 0.0, 0.0],
                    },
                ]
            })
            .collect::<Vec<_>>();
        let partition = finalize_load_partition(&plan, &mesh, &contacts).unwrap();
        let loads = FinalizedContactLoads::from_physical_contacts(
            &partition,
            &contacts,
            final_load_limits(contacts.len(), partition.rows().len()),
        )
        .unwrap();
        assert_eq!(
            loads.contact_ids(),
            contacts
                .iter()
                .map(|contact| contact.contact_id)
                .collect::<Vec<_>>()
        );
        assert!(partition
            .rows()
            .iter()
            .all(|row| loads.contact_ids().contains(&row.contact_id)));
        assert!(load_violation_count(&partition, &contacts).is_ok());
        // FU-8f phase G1: the finalizer's exact plan now also carries the
        // contact-major transpose it scans over, so the boundary this test
        // pins is stated over the same terms the finalizer charges.
        let transpose_bytes = LoadTransposeCounts::count(&plan.load_eligibility, &contacts)
            .unwrap()
            .scratch_bytes(contacts.len())
            .unwrap();
        let resource =
            load_resource_plan(&plan.load_eligibility, &contacts, transpose_bytes).unwrap();
        let verified_mesh = VerifiedMeshFingerprint::compute(&mesh).unwrap();
        assert!(finalize_load_rows(
            &plan,
            &verified_mesh,
            &plan.load_eligibility,
            &contacts,
            LoadFinalizeLimits {
                max_scratch_bytes: resource.scratch_bytes,
                max_operations_floor: resource.operations,
                operations_per_batch_unit: 0,
            },
            None,
        )
        .is_ok());
        assert!(matches!(
            finalize_load_rows(
                &plan,
                &verified_mesh,
                &plan.load_eligibility,
                &contacts,
                LoadFinalizeLimits {
                    max_scratch_bytes: resource.scratch_bytes - 1,
                    max_operations_floor: resource.operations,
                    operations_per_batch_unit: 0,
                },
                None,
            ),
            Err(GenericPerceptionError::LoadScratchLimit { .. })
        ));
        assert!(matches!(
            finalize_load_rows(
                &plan,
                &verified_mesh,
                &plan.load_eligibility,
                &contacts,
                LoadFinalizeLimits {
                    max_scratch_bytes: resource.scratch_bytes,
                    max_operations_floor: resource.operations - 1,
                    operations_per_batch_unit: 0,
                },
                None,
            ),
            Err(GenericPerceptionError::LoadOperationLimit { .. })
        ));
        let unique = partition
            .rows()
            .iter()
            .map(|row| row.face_id)
            .collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), partition.rows().len());
        assert!(partition
            .rows()
            .iter()
            .all(|row| row.carried_area_mm2 > 0.0));
        assert!(partition.rows().iter().all(|row| row.contact_id < 100));
        assert_eq!(
            partition
                .rows()
                .iter()
                .map(|row| row.carried_area_mm2)
                .sum::<f64>(),
            plan.load_eligibility
                .iter()
                .map(|row| row.area_mm2)
                .sum::<f64>()
        );
    }

    /// FU-8b defect (B).  The extended load authority holds one row per
    /// *face*, but the specialized pairs arrive one entry per `(owner, face)`
    /// — 18.6 per face on the Weeping Treant.  Sizing the row count by the
    /// pair count therefore inflated both the load operation bound
    /// (`rows x contacts x (owner_log + 4)`) and the retained-byte bound by
    /// that same factor, which is what put the treant's load stage over its
    /// caps.  This fixture reproduces the shape in miniature: 64 specialized
    /// owners all serving the same 8 faces, 512 pairs over 8 faces.
    #[test]
    fn specialized_load_rows_are_bounded_by_faces_not_by_owner_face_pairs() {
        const OWNERS: u32 = 64;
        const FACES: u32 = 8;
        let mesh = make_overhang_table().unwrap();
        let delicacy = vec![0.0; mesh.tris().len()];
        let config = perception_config(&delicacy);
        let plan = prepare_generic_contacts(
            &mesh,
            &config,
            frame(),
            &layer_plan(),
            &candidate_config(),
            &[],
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
        )
        .unwrap();
        let surface = build_surface(&mesh, &config).unwrap();
        let pairs = (0..OWNERS)
            .flat_map(|owner_id| (0..FACES).map(move |face_id| (owner_id, face_id)))
            .collect::<Vec<_>>();
        assert_eq!(pairs.len(), (OWNERS * FACES) as usize);

        let grouped = GroupedOwnerFaces::from_pairs(&pairs);
        assert_eq!(grouped.face_count(), FACES as usize);
        assert_eq!(grouped.owner_entry_count(), pairs.len());

        let contacts = plan
            .islands()
            .iter()
            .map(|island| PhysicalContact {
                contact_id: u64::from(island.id),
                owner: LoadOwner::GenericIsland(island.id),
                point_mm: [0.0, 0.0, 0.0],
            })
            .chain((0..OWNERS).map(|owner_id| PhysicalContact {
                contact_id: u64::from(owner_id) + 1_000,
                owner: LoadOwner::Specialized(owner_id),
                point_mm: [0.0, 0.0, 0.0],
            }))
            .collect::<Vec<_>>();

        // The row count is the base rows plus the distinct pair faces, never
        // the base rows plus the pairs.
        let by_face = specialized_load_resource_plan(
            &plan,
            contacts.len(),
            SpecializedLoadCounts::exact(&grouped),
        )
        .unwrap();
        let by_pair = specialized_load_resource_plan(
            &plan,
            contacts.len(),
            SpecializedLoadCounts::bounded(pairs.len(), pairs.len()),
        )
        .unwrap();
        assert!(
            by_face.operations < by_pair.operations,
            "grouped {} vs ungrouped {}",
            by_face.operations,
            by_pair.operations
        );
        assert!(by_face.scratch_bytes < by_pair.scratch_bytes);

        // The by-face plan is a real bound, not merely a smaller number: the
        // finalizer runs to completion inside it, and every row it emits is a
        // distinct face of the mesh.
        let partition = finalize_specialized_load_partition(
            &plan,
            &VerifiedMeshFingerprint::compute(&mesh).unwrap(),
            &surface,
            &contacts,
            &pairs,
            LoadFinalizeLimits {
                max_scratch_bytes: by_face.scratch_bytes,
                max_operations_floor: by_face.operations,
                operations_per_batch_unit: 0,
            },
            None,
        )
        .unwrap();
        let faces = partition
            .rows()
            .iter()
            .map(|row| row.face_id)
            .collect::<BTreeSet<_>>();
        assert_eq!(faces.len(), partition.rows().len());
        assert!(partition.rows().len() <= mesh.tris().len());
        assert!(partition.rows().len() < pairs.len());
        // Every pair face reached a row, so the collapse dropped no face.
        assert!((0..FACES).all(|face_id| faces.contains(&face_id)));
    }

    /// FU-8b defect (C).  The extended-eligibility builder used to rescan the
    /// whole pair list twice per base row, and then, per pair, once over the
    /// base rows and once over the pairs already visited — about 6e13 scalar
    /// comparisons on the Weeping Treant, uncharged by any cap, which is the
    /// 16-hour hang the caps study sampled.  Its replacement gathers once and
    /// scatters once.  The rows it emits must be identical, not merely
    /// equivalent-looking, so this drives both against the same fixtures and
    /// compares the whole `Vec`, `Err` included.
    /// FU-8d phase F3, part P2 — **the load delta reproduces the per-variant
    /// finalize it replaces, row by row and on `load_violation_count`.**
    ///
    /// The reference is the predecessor: for each proposal, build that
    /// variant's load inputs and run the *whole* production finalizer on them,
    /// then compare against the delta's answer for every row and for the
    /// violation count.  A merely-close answer fails; areas are compared on
    /// `f64::to_bits`.
    ///
    /// **The variant inputs are built here exactly as
    /// `append_variant_load_inputs` builds them**, and that mirroring is the
    /// point rather than an implementation detail.  The renumbering is what the
    /// delta's soundness rests on: survivors are emitted in ascending baseline
    /// order and take `contact_id = rank` with `owner = first + rank`, and the
    /// merged seat is emitted **last**, so it carries the largest id in its
    /// variant.  The finalizer's argmin tie-breaks on `contact_id`, so
    /// order-preservation on the survivors and last-place for the merged seat
    /// are together the reason a comparison against the *baseline's* indices
    /// gives the variant's answer.
    ///
    /// The fixture refuses to pass vacuously.  It asserts that it reached:
    ///
    /// - an **exact tie between two survivors**, which is the case the
    ///   renumbering could silently reorder and does not — two contacts share a
    ///   point, so their squared distances are bit-identical and only the id
    ///   order decides;
    /// - an **exact tie between a survivor and the merged seat**, which the
    ///   merged seat must lose because it is renumbered last;
    /// - a row with **no survivor** — served only by the merged pair, so the
    ///   merged seat is its only candidate;
    /// - a **displaced holder** — the row's baseline winner was one of the
    ///   merged pair;
    /// - a **steal** — the merged seat takes a row whose baseline holder
    ///   survived;
    /// - a row **outside** `elig(left) ∪ elig(right)`, whose answer the delta
    ///   never touches.
    #[test]
    fn fu8d_load_delta_matches_the_per_variant_finalize() {
        const CONTACTS: usize = 6;
        let mesh = make_overhang_table().unwrap();
        let delicacy = vec![0.0; mesh.tris().len()];
        let config = perception_config(&delicacy);
        let plan = prepare_generic_contacts(
            &mesh,
            &config,
            frame(),
            &layer_plan(),
            &candidate_config(),
            &[],
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
        )
        .unwrap();
        let surface = build_surface(&mesh, &config).unwrap();
        let centroids = surface.geometry().centroids();

        // Six specialized seats.  Faces 0..=7 are shared out so that some rows
        // are served only by the pair, some by a survivor as well, and some by
        // nobody in the pair at all.  Seats 0 and 1 share a point exactly, so
        // any row both serve is an exact survivor/survivor tie.
        let served: [&[u32]; CONTACTS] = [
            &[0, 1, 2],
            &[0, 1, 3],
            &[2, 3, 4],
            &[4, 5],
            &[5, 6],
            &[6, 7],
        ];
        let point = |face: u32, offset: f64| {
            let c = centroids[face as usize];
            [c[0] + offset, c[1], c[2]]
        };
        let points: [[f64; 3]; CONTACTS] = [
            point(1, 0.5),
            point(1, 0.5), // exactly seat 0's point: a survivor/survivor tie
            point(3, 0.25),
            point(5, 0.25),
            point(6, 0.25),
            point(7, 0.25),
        ];
        let pairs = (0..CONTACTS)
            .flat_map(|contact| {
                served[contact]
                    .iter()
                    .map(move |&face| (contact as u32, face))
            })
            .collect::<Vec<_>>();
        // The generic authority's own rows need a holder or the finalizer
        // refuses with `MissingPhysicalContact`, so the batch also carries one
        // seat per generic island — exactly as a real gate batch does.  They
        // are numbered after the specialized seats and never merge, and their
        // presence is what makes "a row outside `elig(left) ∪ elig(right)`"
        // reachable at all.
        let generic_islands = plan
            .islands()
            .iter()
            .map(|island| island.id)
            .collect::<Vec<_>>();
        let contacts = (0..CONTACTS)
            .map(|index| PhysicalContact {
                contact_id: index as u64,
                owner: LoadOwner::Specialized(index as u32),
                point_mm: points[index],
            })
            .chain(
                generic_islands
                    .iter()
                    .enumerate()
                    .map(|(slot, &island)| PhysicalContact {
                        contact_id: (CONTACTS + slot) as u64,
                        owner: LoadOwner::GenericIsland(island),
                        point_mm: [-5.0 - slot as f64, -5.0, -5.0],
                    }),
            )
            .collect::<Vec<_>>();
        let owner_of = |index: usize, rank: usize| -> LoadOwner {
            if index < CONTACTS {
                LoadOwner::Specialized(rank as u32)
            } else {
                LoadOwner::GenericIsland(generic_islands[index - CONTACTS])
            }
        };
        let total_contacts = contacts.len();
        let limits = LoadFinalizeLimits {
            max_scratch_bytes: usize::MAX,
            max_operations_floor: usize::MAX,
            operations_per_batch_unit: 0,
        };
        let (baseline_partition, baseline) = SpecializedLoadBaseline::build(
            &plan,
            &VerifiedMeshFingerprint::compute(&mesh).unwrap(),
            &surface,
            &contacts,
            &pairs,
            limits,
        )
        .expect("baseline");
        let mut scratch = SpecializedLoadDeltaScratch::for_rows(baseline.row_count());

        let face_rows = |face: u32| {
            baseline_partition
                .rows()
                .iter()
                .enumerate()
                .filter(move |(_, row)| row.face_id == face)
                .map(|(index, _)| index as u32)
                .collect::<Vec<_>>()
        };

        let mut saw_survivor_tie = false;
        let mut saw_merged_tie = false;
        let mut saw_no_survivor = false;
        let mut saw_displaced_holder = false;
        let mut saw_survivor_inherited_row = false;
        let mut saw_steal = false;
        let mut saw_untouched_row = false;

        for left in 0..CONTACTS {
            for right in left + 1..CONTACTS {
                for (case, merged_point) in [
                    // Far away: the merged seat wins only what it must.
                    ("far", [1.0e3, 1.0e3, 1.0e3]),
                    // On seat `left`'s point: an exact tie with any survivor
                    // sharing that point, which the merged seat must lose.
                    ("tie", points[left]),
                    // Very close to a face the pair does not serve's holder:
                    // steals whatever it can reach inside the union.
                    ("near", point(4, 0.01)),
                ] {
                    // `elig(left) ∪ elig(right)`, ascending row indices.
                    let mut union_faces = served[left].to_vec();
                    union_faces.extend_from_slice(served[right]);
                    union_faces.sort_unstable();
                    union_faces.dedup();
                    let merged_rows = union_faces
                        .iter()
                        .flat_map(|&face| face_rows(face))
                        .collect::<Vec<_>>();
                    let mut sorted_rows = merged_rows.clone();
                    sorted_rows.sort_unstable();
                    assert_eq!(merged_rows, sorted_rows, "union rows must be ascending");

                    // The reference: the variant's own inputs, renumbered as
                    // `append_variant_load_inputs` renumbers them, through the
                    // whole production finalizer.
                    let survivors = (0..total_contacts)
                        .filter(|index| *index != left && *index != right)
                        .collect::<Vec<_>>();
                    let mut variant_contacts = Vec::with_capacity(survivors.len() + 1);
                    let mut variant_pairs = Vec::new();
                    for (rank, &index) in survivors.iter().enumerate() {
                        variant_contacts.push(PhysicalContact {
                            contact_id: rank as u64,
                            owner: owner_of(index, rank),
                            point_mm: contacts[index].point_mm,
                        });
                        if index < CONTACTS {
                            variant_pairs
                                .extend(served[index].iter().map(|&face| (rank as u32, face)));
                        }
                    }
                    let merged_slot = survivors.len();
                    variant_contacts.push(PhysicalContact {
                        contact_id: merged_slot as u64,
                        owner: LoadOwner::Specialized(merged_slot as u32),
                        point_mm: merged_point,
                    });
                    variant_pairs
                        .extend(union_faces.iter().map(|&face| (merged_slot as u32, face)));
                    let reference = finalize_specialized_load_partition(
                        &plan,
                        &VerifiedMeshFingerprint::compute(&mesh).unwrap(),
                        &surface,
                        &variant_contacts,
                        &variant_pairs,
                        limits,
                        None,
                    )
                    .expect("reference variant finalize");
                    let reference_violations =
                        load_violation_count(&reference, &variant_contacts).expect("reference");

                    let outcome = baseline
                        .merged_violation_count(
                            left,
                            right,
                            merged_point,
                            &merged_rows,
                            &mut scratch,
                        )
                        .expect("delta");

                    assert_eq!(
                        outcome.violation_count, reference_violations,
                        "{case}: violations for merge {left}/{right}"
                    );
                    assert_eq!(
                        outcome.work.eligible_rows,
                        merged_rows.len(),
                        "{case}: every union row is charged"
                    );
                    assert_eq!(reference.rows().len(), baseline_partition.rows().len());

                    // Row by row.  A row inside the union takes the delta's
                    // answer; a row outside keeps the baseline's holder, and
                    // both must equal what the full finalize said, after the
                    // renumbering is applied.
                    let rank_of = |index: u32| -> u64 {
                        survivors
                            .iter()
                            .position(|&survivor| survivor as u32 == index)
                            .map_or(merged_slot as u64, |rank| rank as u64)
                    };
                    for (row, reference_row) in reference.rows().iter().enumerate() {
                        let row = row as u32;
                        let answered = scratch
                            .answered_rows()
                            .iter()
                            .position(|&touched| touched == row);
                        let winner = match answered {
                            Some(position) => scratch.answered_winners()[position],
                            None => {
                                saw_untouched_row = true;
                                baseline.winner[row as usize]
                            }
                        };
                        assert_eq!(
                            reference_row.contact_id,
                            rank_of(winner),
                            "{case}: row {row} of merge {left}/{right}"
                        );
                        assert_eq!(
                            reference_row.face_id,
                            baseline_partition.rows()[row as usize].face_id
                        );
                        assert_eq!(
                            reference_row.carried_area_mm2.to_bits(),
                            baseline_partition.rows()[row as usize]
                                .carried_area_mm2
                                .to_bits()
                        );
                    }

                    // The carried areas the violation count is derived from
                    // must be bit-identical to the fold's, not merely close:
                    // `f64` addition is not associative, so the delta re-sums
                    // each touched contact's variant rows in ascending row
                    // order instead of adjusting the baseline incrementally,
                    // and this is what pins that choice.
                    let mut reference_carried = vec![0.0_f64; variant_contacts.len()];
                    for reference_row in reference.rows() {
                        reference_carried[reference_row.contact_id as usize] +=
                            reference_row.carried_area_mm2;
                    }
                    for (contact, carried) in scratch.touched_carried_areas() {
                        if contact == left as u32 || contact == right as u32 {
                            continue;
                        }
                        let rank = rank_of(contact) as usize;
                        assert_eq!(
                            carried.to_bits(),
                            reference_carried[rank].to_bits(),
                            "{case}: carried area for contact {contact} of merge {left}/{right}"
                        );
                    }

                    // Arms reached, recorded so agreement cannot be vacuous.
                    for (position, &row) in scratch.answered_rows().iter().enumerate() {
                        let after = scratch.answered_winners()[position];
                        let before = baseline.winner[row as usize];
                        let slots = &baseline.witnesses[row as usize];
                        let survivor = slots.iter().find(|witness| {
                            witness.contact != NO_LOAD_WITNESS
                                && witness.contact != left as u32
                                && witness.contact != right as u32
                        });
                        if survivor.is_none() {
                            saw_no_survivor = true;
                        }
                        if before == left as u32 || before == right as u32 {
                            saw_displaced_holder = true;
                            if after != baseline.carried_area_mm2.len() as u32 {
                                // A survivor inherited a displaced holder's
                                // row: the holder was merged away and the
                                // merged seat did NOT win it back.
                                saw_survivor_inherited_row = true;
                            }
                        } else if after == baseline.carried_area_mm2.len() as u32 {
                            saw_steal = true;
                        }
                        if let Some(witness) = survivor {
                            let merged_squared = merged_point
                                .iter()
                                .zip(baseline.row_centroid_mm[row as usize])
                                .map(|(a, b)| (a - b).powi(2))
                                .sum::<f64>();
                            if witness.squared_distance.to_bits() == merged_squared.to_bits() {
                                saw_merged_tie = true;
                            }
                        }
                        let filled = slots
                            .iter()
                            .filter(|witness| witness.contact != NO_LOAD_WITNESS)
                            .collect::<Vec<_>>();
                        if filled.len() >= 2
                            && filled[0].squared_distance.to_bits()
                                == filled[1].squared_distance.to_bits()
                        {
                            saw_survivor_tie = true;
                        }
                    }
                }
            }
        }

        assert!(saw_survivor_tie, "no exact survivor/survivor distance tie");
        assert!(saw_merged_tie, "no exact survivor/merged-seat distance tie");
        assert!(saw_no_survivor, "no row was served only by the merged pair");
        assert!(
            saw_displaced_holder,
            "no row's baseline holder was merged away"
        );
        assert!(
            saw_survivor_inherited_row,
            "no displaced holder's row was inherited by a survivor"
        );
        assert!(
            saw_steal,
            "the merged seat never stole a surviving holder's row"
        );
        assert!(
            saw_untouched_row,
            "every row was inside some union, so the locality claim is untested"
        );
    }

    /// FU-8f phase G1: the transposed load finalize reproduces the
    /// `contacts x rows` scan it replaced, cell for cell.
    ///
    /// The oracle is [`reference_contacts_x_rows_scan`] — the replaced
    /// implementation itself, retained `#[cfg(test)]`, the same shape FU-8c and
    /// FU-8d used.  What is compared is the production pass
    /// ([`scan_transposed_load`], the one `finalize_load_rows` calls), row by
    /// row, on `to_bits`: the argmin's contact index *and* its squared
    /// distance, and all [`LOAD_WITNESS_SLOTS`] retained witness slots with
    /// both their fields.  `to_bits` rather than `==` because the claim is
    /// bit-identity, and `NaN`/`-0.0` would pass a float compare that
    /// bit-identity must not.
    ///
    /// Two datasets, both from real geometry: `make_overhang_table`'s own
    /// generic load authority, and the FU-8d phase F3 load fixture's extended
    /// eligibility (six specialized seats over shared faces, plus one seat per
    /// generic island), which is where owner runs, exact ties and rows served
    /// by several owners actually occur.
    ///
    /// Non-vacuity, the FU-8d way: the transpose must be *sparse*, so a
    /// silently-dense transpose (or an eligibility that admits everything)
    /// fails the test rather than passing it trivially.
    #[test]
    fn fu8f_transposed_finalize_reproduces_the_scan_bit_for_bit() {
        let mesh = make_overhang_table().unwrap();
        let delicacy = vec![0.0; mesh.tris().len()];
        let config = perception_config(&delicacy);
        let plan = prepare_generic_contacts(
            &mesh,
            &config,
            frame(),
            &layer_plan(),
            &candidate_config(),
            &[],
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
        )
        .unwrap();
        let surface = build_surface(&mesh, &config).unwrap();
        let centroids = surface.geometry().centroids();
        let generic_islands = plan
            .islands()
            .iter()
            .map(|island| island.id)
            .collect::<Vec<_>>();

        // Returns `(cells, rows x contacts)` so the caller can assert sparsity
        // on the dataset that carries it.
        let compare = |eligibility: &[LoadEligibility],
                       contacts: &[PhysicalContact],
                       label: &str|
         -> (usize, usize) {
            let counts = LoadTransposeCounts::count(eligibility, contacts).expect("count");
            let cells = counts.cells;
            let dense = eligibility.len() * contacts.len();
            assert_eq!(
                counts.scratch_bytes(contacts.len()).expect("bytes"),
                cells * 4 + (contacts.len() + 1) * 4,
                "{label}: the charged term is cells x 4 B + (contacts + 1) x 4 B",
            );
            let transpose = counts.finish(eligibility, contacts);

            // The CSR is exactly the eligible set, and every bucket is
            // strictly ascending *without a sort* — the property the
            // cell-for-cell visit-order claim rests on.
            let mut recovered = 0usize;
            for (contact_index, contact) in contacts.iter().enumerate() {
                let bucket = transpose.rows_of(contact_index);
                assert!(
                    bucket.windows(2).all(|pair| pair[0] < pair[1]),
                    "{label}: contact {contact_index}'s bucket is not strictly ascending",
                );
                for &row in bucket {
                    assert!(
                        eligibility[row as usize]
                            .eligible_owners
                            .binary_search(&contact.owner)
                            .is_ok(),
                        "{label}: bucket holds an ineligible cell",
                    );
                }
                recovered += bucket.len();
                let expected = eligibility
                    .iter()
                    .filter(|row| row.eligible_owners.binary_search(&contact.owner).is_ok())
                    .count();
                assert_eq!(
                    bucket.len(),
                    expected,
                    "{label}: contact {contact_index} lost or gained cells",
                );
            }
            assert_eq!(
                recovered, cells,
                "{label}: cells disagrees with the buckets"
            );

            let mut retention = LoadRowRetention {
                witnesses: Vec::new(),
                centroids_mm: Vec::new(),
            };
            let transposed =
                scan_transposed_load(eligibility, contacts, &transpose, Some(&mut retention));
            let (reference, reference_witnesses) =
                reference_contacts_x_rows_scan(eligibility, contacts);
            assert_eq!(transposed.len(), reference.len(), "{label}: row count");
            for row in 0..reference.len() {
                assert_eq!(
                    transposed[row].map(|(index, distance)| (index, distance.to_bits())),
                    reference[row].map(|(index, distance)| (index, distance.to_bits())),
                    "{label}: row {row}'s nearest is not bit-identical",
                );
                for (slot, (left, right)) in retention.witnesses[row]
                    .iter()
                    .zip(reference_witnesses[row].iter())
                    .enumerate()
                {
                    assert_eq!(
                        (left.contact, left.squared_distance.to_bits()),
                        (right.contact, right.squared_distance.to_bits()),
                        "{label}: row {row} witness slot {slot} is not bit-identical",
                    );
                }
            }
            assert_eq!(
                retention.centroids_mm.len(),
                eligibility.len(),
                "{label}: centroids not retained per row",
            );
            (cells, dense)
        };

        // Dataset 1 — `make_overhang_table`'s generic load authority, one
        // contact per generic island.
        let generic_contacts = generic_islands
            .iter()
            .enumerate()
            .map(|(slot, &island)| PhysicalContact {
                contact_id: slot as u64,
                owner: LoadOwner::GenericIsland(island),
                point_mm: [-5.0 - slot as f64, -5.0, -5.0],
            })
            .collect::<Vec<_>>();
        // No sparsity is claimed here and none exists: `make_overhang_table`'s
        // generic authority is a handful of rows over a single island, so its
        // matrix is dense by construction.  It is carried because it is the
        // *accepted-load* path's own shape — the path `Sphere`, `Cylinder` and
        // `make_box` exercise in the OD-8 census with no tournament at all —
        // and equivalence there is what the census digests depend on.
        let (generic_cells, generic_dense) = compare(
            &plan.load_eligibility,
            &generic_contacts,
            "make_overhang_table generic authority",
        );
        assert!(
            generic_cells <= generic_dense,
            "a transpose cannot hold more cells than the dense matrix has",
        );

        // Dataset 2 — the FU-8d phase F3 load fixture: six specialized seats
        // over shared faces (seats 0 and 1 share a point exactly, so rows both
        // serve are an exact tie the `contact_id` tie-break must resolve
        // identically), plus one seat per generic island.
        const CONTACTS: usize = 6;
        let served: [&[u32]; CONTACTS] = [
            &[0, 1, 2],
            &[0, 1, 3],
            &[2, 3, 4],
            &[4, 5],
            &[5, 6],
            &[6, 7],
        ];
        let point = |face: u32, offset: f64| {
            let c = centroids[face as usize];
            [c[0] + offset, c[1], c[2]]
        };
        let points: [[f64; 3]; CONTACTS] = [
            point(1, 0.5),
            point(1, 0.5),
            point(3, 0.25),
            point(5, 0.25),
            point(6, 0.25),
            point(7, 0.25),
        ];
        let pairs = (0..CONTACTS)
            .flat_map(|contact| {
                served[contact]
                    .iter()
                    .map(move |&face| (contact as u32, face))
            })
            .collect::<Vec<_>>();
        let fixture_contacts = (0..CONTACTS)
            .map(|index| PhysicalContact {
                contact_id: index as u64,
                owner: LoadOwner::Specialized(index as u32),
                point_mm: points[index],
            })
            .chain(
                generic_islands
                    .iter()
                    .enumerate()
                    .map(|(slot, &island)| PhysicalContact {
                        contact_id: (CONTACTS + slot) as u64,
                        owner: LoadOwner::GenericIsland(island),
                        point_mm: [-5.0 - slot as f64, -5.0, -5.0],
                    }),
            )
            .collect::<Vec<_>>();
        let grouped = GroupedOwnerFaces::from_pairs(&pairs);
        let extended = extend_load_eligibility(
            &plan.load_eligibility,
            mesh.tris().len(),
            &surface,
            &grouped,
            &pairs,
        )
        .expect("extended eligibility");
        let (fixture_cells, fixture_dense) =
            compare(&extended, &fixture_contacts, "FU-8d F3 load fixture");
        assert!(
            fixture_cells < fixture_dense,
            "the F3 fixture's transpose is dense: {fixture_cells} of {fixture_dense}",
        );

        // Dataset 3 — the non-vacuity witness the design record asks for
        // (cells sparser than `rows x contacts` by more than 10x).  Neither
        // named fixture can carry it: `make_overhang_table`'s generic
        // authority is one island over a couple of rows, and the F3 fixture's
        // six seats over eight shared faces is only ~4x.  The treant's own
        // ratio is 19.6x (8,210,643 eligible of 160,834,206 visited), and it
        // comes from *many contacts each eligible for a small fraction of the
        // rows* — so the witness reproduces that shape rather than that scale:
        // one seat per pair of faces, over the same mesh.
        const FAN_SEATS: usize = 24;
        let fan_pairs = (0..FAN_SEATS)
            .flat_map(|seat| {
                let face = 2 * seat as u32;
                [(seat as u32, face), (seat as u32, face + 1)]
            })
            .collect::<Vec<_>>();
        assert!(
            2 * FAN_SEATS <= mesh.tris().len(),
            "the fan fixture wants more faces than the mesh has",
        );
        let fan_contacts = (0..FAN_SEATS)
            .map(|index| PhysicalContact {
                contact_id: index as u64,
                owner: LoadOwner::Specialized(index as u32),
                point_mm: point(2 * index as u32, 0.25),
            })
            .chain(
                generic_islands
                    .iter()
                    .enumerate()
                    .map(|(slot, &island)| PhysicalContact {
                        contact_id: (FAN_SEATS + slot) as u64,
                        owner: LoadOwner::GenericIsland(island),
                        point_mm: [-5.0 - slot as f64, -5.0, -5.0],
                    }),
            )
            .collect::<Vec<_>>();
        let fan_grouped = GroupedOwnerFaces::from_pairs(&fan_pairs);
        let fan_extended = extend_load_eligibility(
            &plan.load_eligibility,
            mesh.tris().len(),
            &surface,
            &fan_grouped,
            &fan_pairs,
        )
        .expect("fan eligibility");
        let (fan_cells, fan_dense) = compare(&fan_extended, &fan_contacts, "wide specialized fan");
        assert!(
            fan_cells * 10 < fan_dense,
            "the transpose is not sparse by >10x: \
             {fan_cells} cells of {fan_dense} rows x contacts",
        );
    }

    /// FU-8f phase G2: hoisting the hash must not hoist the **check**.
    ///
    /// The gate stage now hashes the mesh once and threads the proof to both
    /// load finalizers instead of each re-hashing 429,120 triangles per merge
    /// round.  The saving is only legitimate if every consumer still compares
    /// the threaded digest against its own plan authority — "verify once,
    /// thread the proof", never "stop verifying".
    ///
    /// # Why the arms are shaped the way they are (strengthened 2026-08-07)
    ///
    /// The original G2 version of this test asserted `StaleAuthority` from
    /// each finalizer with empty specialized pairs.  That pinned
    /// [`finalize_load_rows`] honestly — it was called directly, and nothing
    /// else on its path can produce that error — but it pinned
    /// [`finalize_specialized_load_partition`] only **by delegation**: with
    /// that site's own comparison deleted, the call fell straight through to
    /// the `finalize_load_rows` it invokes, which refused on the same proof
    /// and produced the same error.  The assertion could not tell the two
    /// sites apart, so it did not prove what its name claimed.
    ///
    /// Each arm below therefore hands the specialized site a specialized
    /// pair whose face id makes the delegate **unreachable** with the site's
    /// own comparison gone, so the arm can only be satisfied by that site:
    ///
    /// * **Arm 1** — [`finalize_load_rows`], called directly.
    /// * **Arm 2** — [`finalize_specialized_load_partition`] with a foreign
    ///   mesh *smaller* than the real surface, and a pair naming a face the
    ///   real surface has and the foreign mesh does not.  Past a deleted
    ///   check, `extend_load_eligibility` sizes its range guard from
    ///   `verified_mesh.mesh()` and returns `SpecializedFaceOutOfRange`
    ///   **before** any delegation, so the arm sees a different variant.
    /// * **Arm 3** — the same site with a foreign mesh *larger* than the real
    ///   surface, and a pair naming a face the foreign mesh has and the real
    ///   surface does not.  This is the dangerous direction: `surface` is an
    ///   argument independent of the proof, so past a deleted check the
    ///   larger mesh's face count admits a face id that then indexes
    ///   `surface.geometry()` out of bounds.  Both finalizers are asserted on
    ///   the larger proof.  The assertion is on the exact error *variant*, so
    ///   it also proves the fingerprint comparison is genuinely fail-fast:
    ///   `StaleAuthority`, never the range error the same inputs would raise
    ///   downstream.
    ///
    /// # Mutation evidence — the arms are non-vacuous and independent
    ///
    /// Measured 2026-08-07 the FU-8e way, by disabling one comparison at a
    /// time and re-running this test, then restoring it:
    ///
    /// * `finalize_load_rows`'s comparison disabled → **arm 1 FAILED**
    ///   (`Ok`, i.e. `.err()` was `None`, against `Some(StaleAuthority)`).
    ///   Re-run with only the two direct `finalize_load_rows` assertions
    ///   neutralized, the same mutation left in place: **arms 2 and 3
    ///   passed** — the specialized site refuses on its own, which is the
    ///   delegation-independence this test previously lacked.
    /// * `finalize_specialized_load_partition`'s comparison disabled →
    ///   arm 1 passed and **arm 2 FAILED** with
    ///   `Some(SpecializedFaceOutOfRange { face_id: 12 })` against
    ///   `Some(StaleAuthority)` — the delegate was never reached, so nothing
    ///   but this site could have satisfied the arm.
    /// * Same mutation with arm 2's expectation relaxed so arm 3 is reached →
    ///   **arm 3 FAILED** by panic, `index out of bounds: the len is 60 but
    ///   the index is 60`, inside `extend_load_eligibility`'s
    ///   `surface.geometry().centroids()[face_id]` — the out-of-range read
    ///   the larger-mesh direction was added to cover, demonstrated to be
    ///   reachable and not merely argued.
    ///
    /// The positive arms are asserted alongside, including one with arm 2's
    /// own pairs, so no refusal passes for the trivial reason that these
    /// inputs are refused anyway.
    #[test]
    fn fu8f_hoisted_fingerprint_still_refuses_a_foreign_mesh_at_both_finalizers() {
        // The specialized owner the arms' pairs name.  It carries a physical
        // contact of its own so the pairs are admissible on their own merits
        // under the mesh's own proof — see the positive arms at the end.
        const SPECIALIZED_OWNER: u32 = 7;

        let mesh = make_overhang_table().unwrap();
        let faces = mesh.tris().len();
        let delicacy = vec![0.0; faces];
        let config = perception_config(&delicacy);
        let plan = prepare_generic_contacts(
            &mesh,
            &config,
            frame(),
            &layer_plan(),
            &candidate_config(),
            &[],
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
        )
        .unwrap();
        let surface = build_surface(&mesh, &config).unwrap();
        let mut contacts = plan
            .islands()
            .iter()
            .map(|island| PhysicalContact {
                contact_id: u64::from(island.id),
                owner: LoadOwner::GenericIsland(island.id),
                point_mm: [0.0, 0.0, 0.0],
            })
            .collect::<Vec<_>>();
        assert!(
            !contacts.is_empty(),
            "the fixture must reach the non-empty specialized path",
        );
        contacts.push(PhysicalContact {
            contact_id: u64::MAX,
            owner: LoadOwner::Specialized(SPECIALIZED_OWNER),
            point_mm: [0.0, 0.0, 0.0],
        });
        let limits = LoadFinalizeLimits {
            max_scratch_bytes: usize::MAX,
            max_operations_floor: usize::MAX,
            operations_per_batch_unit: 0,
        };

        // Two real, honestly computed proofs — each of the wrong mesh, and
        // deliberately one on each side of the real surface's face count,
        // because the two directions fail differently past a missing check.
        let smaller = plinth_mesh::make_box_default().unwrap();
        let larger = plinth_mesh::make_toed_slab().unwrap();
        assert!(
            smaller.tris().len() < faces && faces < larger.tris().len(),
            "the arms need one foreign mesh smaller and one larger than the \
             real surface's {faces} faces, got {} and {}",
            smaller.tris().len(),
            larger.tris().len(),
        );
        let smaller_proof = VerifiedMeshFingerprint::compute(&smaller).unwrap();
        let larger_proof = VerifiedMeshFingerprint::compute(&larger).unwrap();
        for proof in [&smaller_proof, &larger_proof] {
            assert_ne!(
                proof.fingerprint(),
                plan.authority.mesh,
                "the fixture meshes must actually differ, or this test proves nothing",
            );
        }

        // A face the real surface has and the smaller foreign mesh does not.
        let real_only_face = u32::try_from(smaller.tris().len()).unwrap();
        let near_pairs = [(SPECIALIZED_OWNER, real_only_face)];
        // A face the larger foreign mesh has and the real surface does not.
        let foreign_only_face = u32::try_from(faces).unwrap();
        let far_pairs = [(SPECIALIZED_OWNER, foreign_only_face)];

        // Arm 1 — `finalize_load_rows`'s own comparison, reached directly.
        // Nothing else on this path can raise `StaleAuthority`, and with the
        // comparison gone the call runs to completion and returns `Ok`.
        assert_eq!(
            finalize_load_rows(
                &plan,
                &smaller_proof,
                &plan.load_eligibility,
                &contacts,
                limits,
                None,
            )
            .err(),
            Some(GenericPerceptionError::StaleAuthority),
            "finalize_load_rows stopped checking the threaded fingerprint",
        );

        // Arm 2 — `finalize_specialized_load_partition`'s OWN comparison, not
        // its delegate's.  `real_only_face` is out of range for the proof's
        // mesh, so the only path past this site's check ends in
        // `SpecializedFaceOutOfRange` before `finalize_load_rows` is ever
        // called.  This arm is satisfiable by this site alone.
        assert_eq!(
            finalize_specialized_load_partition(
                &plan,
                &smaller_proof,
                &surface,
                &contacts,
                &near_pairs,
                limits,
                None,
            )
            .err(),
            Some(GenericPerceptionError::StaleAuthority),
            "finalize_specialized_load_partition stopped checking the threaded \
             fingerprint at its own site",
        );

        // Arm 3 — the dangerous direction: a foreign mesh with MORE faces
        // than the surface it is presented alongside.  `foreign_only_face` is
        // in range for the proof's mesh and out of range for `surface`, so
        // past this site's check the face-range guard would admit a face id
        // that indexes `surface.geometry()` out of bounds.  Asserting the
        // exact variant also pins the check as fail-fast: it must refuse
        // before the range guard, not after it.
        assert_eq!(
            finalize_specialized_load_partition(
                &plan,
                &larger_proof,
                &surface,
                &contacts,
                &far_pairs,
                limits,
                None,
            )
            .err(),
            Some(GenericPerceptionError::StaleAuthority),
            "finalize_specialized_load_partition admitted a LARGER foreign \
             mesh, whose face count would bound a range guard over a surface \
             it does not index",
        );
        assert_eq!(
            finalize_load_rows(
                &plan,
                &larger_proof,
                &plan.load_eligibility,
                &contacts,
                limits,
                None,
            )
            .err(),
            Some(GenericPerceptionError::StaleAuthority),
            "finalize_load_rows admitted a LARGER foreign mesh",
        );

        // The same inputs under the mesh's own proof are accepted, so the
        // refusals above are the fingerprint's doing and nothing else's —
        // including arm 2's pairs, which are a legal specialized assignment
        // for this mesh and are refused only because the proof is foreign.
        let proof = VerifiedMeshFingerprint::compute(&mesh).unwrap();
        assert_eq!(proof.fingerprint(), plan.authority.mesh);
        assert!(std::ptr::eq(proof.mesh(), &mesh));
        finalize_load_rows(
            &plan,
            &proof,
            &plan.load_eligibility,
            &contacts,
            limits,
            None,
        )
        .expect("the mesh's own proof admits the generic finalize");
        finalize_specialized_load_partition(&plan, &proof, &surface, &contacts, &[], limits, None)
            .expect("the mesh's own proof admits the specialized finalize");
        finalize_specialized_load_partition(
            &plan,
            &proof,
            &surface,
            &contacts,
            &near_pairs,
            limits,
            None,
        )
        .expect("the mesh's own proof admits arm 2's specialized pairs");
    }

    #[test]
    fn specialized_extension_matches_the_rescanning_algorithm_it_replaced() {
        let mesh = plinth_mesh::make_box_default().unwrap();
        let delicacy = vec![0.0; mesh.tris().len()];
        let surface = build_surface(&mesh, &perception_config(&delicacy)).unwrap();
        let faces = mesh.tris().len();
        assert!(faces >= 12, "fixture needs faces to spread the cases over");

        // Base centroids and areas are deliberately unlike the surface's own,
        // so a row that silently took its geometry from the wrong side shows
        // up as a mismatch rather than a coincidence.
        let base_row = |face_id: u32, owners: Vec<LoadOwner>| LoadEligibility {
            face_id,
            eligible_owners: owners,
            centroid_mm: [f64::from(face_id), -1.0, 0.25],
            area_mm2: f64::from(face_id) + 0.5,
        };
        let island = LoadOwner::GenericIsland;
        let bases: Vec<Vec<LoadEligibility>> = vec![
            Vec::new(),
            vec![base_row(0, vec![island(0)])],
            vec![
                base_row(0, vec![island(0), island(1)]),
                base_row(2, vec![island(1)]),
                base_row(5, vec![island(3), LoadOwner::Specialized(7)]),
                base_row(9, vec![island(0)]),
            ],
            // Two rows for one face: legal for this function, and the only
            // shape where the final sort's stability is observable.
            vec![
                base_row(4, vec![island(0)]),
                base_row(4, vec![island(1)]),
                base_row(1, vec![island(2)]),
            ],
        ];
        let pair_sets: Vec<Vec<(u32, u32)>> = vec![
            Vec::new(),
            // Every pair lands on a base face.
            vec![(3, 0), (4, 2), (1, 0)],
            // No pair lands on a base face.
            vec![(0, 11), (1, 7), (2, 11), (0, 7)],
            // Mixed, out of order, one owner serving several faces and one
            // face served by several owners, plus an exact duplicate pair.
            vec![
                (6, 9),
                (2, 11),
                (6, 5),
                (0, 0),
                (6, 11),
                (2, 11),
                (1, 4),
                (9, 2),
                (0, 8),
            ],
            // One face, many owners: the treant's shape in miniature.
            (0..32u32).map(|owner| (owner, 6)).collect(),
        ];

        for base in &bases {
            for pairs in &pair_sets {
                let grouped = GroupedOwnerFaces::from_pairs(pairs);
                let expected =
                    reference_extend_load_eligibility(base, faces, &surface, pairs).unwrap();
                let actual =
                    extend_load_eligibility(base, faces, &surface, &grouped, pairs).unwrap();
                assert_eq!(actual, expected, "base {base:?} pairs {pairs:?}");
                // The rows are one per distinct face, which is what defect (B)
                // sized the plan for.
                assert!(actual.len() <= base.len() + grouped.face_count());
            }
        }

        // An out-of-range face refuses identically, naming the same face: the
        // first offending pair in the order the pairs were supplied.
        let out_of_range = vec![
            (0, 2),
            (1, u32::try_from(faces).unwrap() + 3),
            (2, u32::try_from(faces).unwrap()),
        ];
        let grouped = GroupedOwnerFaces::from_pairs(&out_of_range);
        let expected = reference_extend_load_eligibility(&bases[2], faces, &surface, &out_of_range);
        let actual = extend_load_eligibility(&bases[2], faces, &surface, &grouped, &out_of_range);
        assert_eq!(
            expected,
            Err(GenericPerceptionError::SpecializedFaceOutOfRange {
                face_id: u32::try_from(faces).unwrap() + 3,
            })
        );
        assert_eq!(actual, expected);
    }

    #[test]
    fn box_full_birth_minimum_and_solid_area_vector_is_frozen() {
        let mesh = plinth_mesh::make_box_default().unwrap();
        let delicacy = vec![0.0; mesh.tris().len()];
        let plan = prepare_generic_contacts(
            &mesh,
            &perception_config(&delicacy),
            frame(),
            &layer_plan(),
            &candidate_config(),
            &[],
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
        )
        .unwrap();
        assert_eq!(plan.island_births().len(), 1);
        let birth = &plan.island_births()[0];
        assert_eq!(birth.layer_index, 0);
        assert_eq!(birth.z_mm, 0.025);
        assert_eq!(birth.centroid_xy_mm, [0.0, 0.0]);
        assert_eq!(birth.area_mm2, 100.0);
        assert!(birth.footprint().holes().is_empty());
        assert_eq!(plan.local_z_minima().len(), 1);
        let minimum = &plan.local_z_minima()[0];
        assert_eq!(minimum.position_mm, [-5.0, -5.0, 0.0]);
        let diagonal = -1.0 / 3.0_f64.sqrt();
        assert_eq!(minimum.normal, [diagonal, diagonal, diagonal]);
        let mut region = minimum.region_points_mm().to_vec();
        region.sort_by(|left, right| {
            left[0]
                .total_cmp(&right[0])
                .then_with(|| left[1].total_cmp(&right[1]))
                .then_with(|| left[2].total_cmp(&right[2]))
        });
        assert_eq!(
            region,
            vec![
                [-5.0, -5.0, 0.0],
                [-5.0, 5.0, 0.0],
                [5.0, -5.0, 0.0],
                [5.0, 5.0, 0.0],
            ]
        );
    }

    #[test]
    fn overlap_caps_have_exact_cap_and_cap_plus_one_boundaries() {
        let loop_ = SliceLoop {
            points_xy_mm: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            signed_area_mm2: 1.0,
            hole: false,
        };
        let solid = SliceSolid {
            outer: loop_,
            holes: vec![],
        };
        let rows = vec![solid.clone(), solid];
        assert!(overlap_candidates(&rows, &rows, 4, 96).is_ok());
        assert_eq!(
            overlap_candidates(&rows, &rows, 3, 96),
            Err(GenericPerceptionError::OverlapPairLimit {
                required: 4,
                limit: 3,
            })
        );
        assert_eq!(
            overlap_candidates(&rows, &rows, 4, 95),
            Err(GenericPerceptionError::OverlapByteLimit {
                required: 96,
                limit: 95,
            })
        );
    }

    #[test]
    fn table_toe_raking_and_thin_fin_are_bound() {
        for mesh in [
            make_overhang_table().unwrap(),
            make_toed_slab().unwrap(),
            make_raking_blade().unwrap(),
            make_thin_fin().unwrap(),
        ] {
            let delicacy = vec![0.0; mesh.tris().len()];
            let plan = prepare_generic_contacts(
                &mesh,
                &perception_config(&delicacy),
                frame(),
                &layer_plan(),
                &candidate_config(),
                &[],
                GenericPerceptionLimits::production(TEST_KNN_LIMITS),
            )
            .expect("required semantic fixture perception");
            plan.validate_binding(
                &mesh,
                &perception_config(&delicacy),
                frame(),
                &layer_plan(),
                &candidate_config(),
                &[],
                GenericPerceptionLimits::production(TEST_KNN_LIMITS),
            )
            .unwrap();
            assert_ne!(plan.digest(), 0);
        }
    }

    #[test]
    fn frozen_live_fixture_birth_and_filtered_minimum_counts_match_python() {
        let table = make_overhang_table().unwrap();
        let table_delicacy = vec![0.0; table.tris().len()];
        let table_plan = prepare_generic_contacts(
            &table,
            &perception_config(&table_delicacy),
            frame(),
            &layer_plan(),
            &candidate_config(),
            &[],
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
        )
        .unwrap();
        assert_eq!(table_plan.island_births().len(), 4);
        assert_eq!(table_plan.local_z_minima().len(), 4);

        let toe = make_toed_slab().unwrap();
        let toe_delicacy = vec![0.0; toe.tris().len()];
        let toe_plan = prepare_generic_contacts(
            &toe,
            &perception_config(&toe_delicacy),
            frame(),
            &layer_plan(),
            &candidate_config(),
            &[],
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
        )
        .unwrap();
        assert_eq!(toe_plan.local_z_minima().len(), 3);
        let toe_z = toe_plan
            .local_z_minima()
            .iter()
            .map(|minimum| minimum.position_mm[2])
            .collect::<Vec<_>>();
        assert!(toe_z.iter().any(|z| (*z - 9.0).abs() < 1.0e-3));
        assert!(toe_z.iter().any(|z| (*z - 11.0).abs() < 1.0e-3));

        let raking = make_raking_blade().unwrap();
        let raking_delicacy = vec![0.0; raking.tris().len()];
        let raking_plan = prepare_generic_contacts(
            &raking,
            &perception_config(&raking_delicacy),
            frame(),
            &layer_plan(),
            &candidate_config(),
            &[],
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
        )
        .unwrap();
        assert!(raking_plan.island_births().len() >= 5);
        assert!(raking_plan.local_z_minima().len() <= 2);
        assert!(raking_plan.island_births().len() > raking_plan.local_z_minima().len() + 3);
    }

    #[test]
    fn required_sphere_fixture_is_loaded_and_binding_rejects_another_mesh() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/tests/fixtures/chitubox/Sphere.stl");
        assert!(path.is_file(), "required Sphere.stl fixture is missing");
        let mesh = plinth_mesh::load_binary_stl(&path, WeldPolicy::None)
            .expect("required Sphere.stl must parse");
        let delicacy = vec![0.0; mesh.tris().len()];
        let layers = covering_layer_plan(&mesh);
        let plan = prepare_generic_contacts(
            &mesh,
            &perception_config(&delicacy),
            frame(),
            &layers,
            &candidate_config(),
            &[],
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
        )
        .expect("Sphere perception");
        let other = plinth_mesh::make_box_default().unwrap();
        let other_delicacy = vec![0.0; other.tris().len()];
        assert_eq!(
            plan.validate_binding(
                &other,
                &perception_config(&other_delicacy),
                frame(),
                &layers,
                &candidate_config(),
                &[],
                GenericPerceptionLimits::production(TEST_KNN_LIMITS),
            ),
            Err(GenericPerceptionError::StaleAuthority)
        );
    }

    #[test]
    fn authority_stale_matrix_binds_every_result_shaping_input() {
        let mesh = plinth_mesh::make_box_default().unwrap();
        let delicacy = vec![0.0; mesh.tris().len()];
        let config = perception_config(&delicacy);
        let layers = layer_plan();
        let candidates = candidate_config();
        let reservations = [SpecializedOwnerReservation {
            owner_id: 1,
            face_id: 0,
        }];
        let limits = GenericPerceptionLimits::production(TEST_KNN_LIMITS);
        let plan = prepare_generic_contacts(
            &mesh,
            &config,
            frame(),
            &layers,
            &candidates,
            &reservations,
            limits,
        )
        .unwrap();
        assert!(plan
            .validate_binding(
                &mesh,
                &config,
                frame(),
                &layers,
                &candidates,
                &reservations,
                limits,
            )
            .is_ok());
        let relaxed_knn_limits = GenericPerceptionLimits {
            minima_knn: KnnBatchLimits {
                max_source_points: TEST_KNN_LIMITS.max_source_points + 1,
                max_query_points: TEST_KNN_LIMITS.max_query_points + 1,
                max_neighbors: TEST_KNN_LIMITS.max_neighbors + 1,
                max_logical_peak_bytes: TEST_KNN_LIMITS.max_logical_peak_bytes + 1,
                max_work: TEST_KNN_LIMITS.max_work + 1,
            },
            ..limits
        };
        let relaxed_plan = prepare_generic_contacts(
            &mesh,
            &config,
            frame(),
            &layers,
            &candidates,
            &reservations,
            relaxed_knn_limits,
        )
        .unwrap();
        assert_eq!(relaxed_plan.digest(), plan.digest());
        assert!(plan
            .validate_binding(
                &mesh,
                &config,
                frame(),
                &layers,
                &candidates,
                &reservations,
                relaxed_knn_limits,
            )
            .is_ok());

        let other_mesh = plinth_mesh::make_box([11.0, 10.0, 10.0], [0.0; 3]).unwrap();
        let other_delicacy = vec![0.0; other_mesh.tris().len()];
        assert_eq!(
            plan.validate_binding(
                &other_mesh,
                &perception_config(&other_delicacy),
                frame(),
                &layers,
                &candidates,
                &reservations,
                limits,
            ),
            Err(GenericPerceptionError::StaleAuthority)
        );
        let changed_config = SupportPerceptionConfig::current(
            Finite::new(0.0).unwrap(),
            PositiveFinite::new(2.0).unwrap(),
            PositiveFinite::new(0.5).unwrap(),
            64 * 1024 * 1024,
            Some(&delicacy),
        )
        .unwrap();
        assert_eq!(
            plan.validate_binding(
                &mesh,
                &changed_config,
                frame(),
                &layers,
                &candidates,
                &reservations,
                limits,
            ),
            Err(GenericPerceptionError::StaleAuthority)
        );
        let changed_frame =
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(6.0).unwrap());
        assert_eq!(
            plan.validate_binding(
                &mesh,
                &config,
                changed_frame,
                &layers,
                &candidates,
                &reservations,
                limits,
            ),
            Err(GenericPerceptionError::StaleAuthority)
        );
        let mut changed_heights = layers.heights_mm().to_vec();
        changed_heights[10] = 0.051;
        let changed_layers = LayerPlan::new(0.0, changed_heights).unwrap();
        assert_eq!(
            plan.validate_binding(
                &mesh,
                &config,
                frame(),
                &changed_layers,
                &candidates,
                &reservations,
                limits,
            ),
            Err(GenericPerceptionError::StaleAuthority)
        );
        let changed_candidates =
            CandidatePayloadConfig::current(7, 1_000_000, 3_000_000, 256 * 1024 * 1024, 65_536)
                .unwrap();
        assert_eq!(
            plan.validate_binding(
                &mesh,
                &config,
                frame(),
                &layers,
                &changed_candidates,
                &reservations,
                limits,
            ),
            Err(GenericPerceptionError::StaleAuthority)
        );
        let changed_reservations = [SpecializedOwnerReservation {
            owner_id: 2,
            face_id: 0,
        }];
        assert_eq!(
            plan.validate_binding(
                &mesh,
                &config,
                frame(),
                &layers,
                &candidates,
                &changed_reservations,
                limits,
            ),
            Err(GenericPerceptionError::StaleAuthority)
        );
        for changed_limits in [
            GenericPerceptionLimits {
                max_island_neighbor_pairs: limits.max_island_neighbor_pairs - 1,
                ..limits
            },
            GenericPerceptionLimits {
                max_island_neighbor_bytes: limits.max_island_neighbor_bytes - 1,
                ..limits
            },
            GenericPerceptionLimits {
                max_overlap_pairs: limits.max_overlap_pairs - 1,
                ..limits
            },
            GenericPerceptionLimits {
                max_overlap_bytes: limits.max_overlap_bytes - 1,
                ..limits
            },
            GenericPerceptionLimits {
                max_overlap_vertex_tests: limits.max_overlap_vertex_tests - 1,
                ..limits
            },
            GenericPerceptionLimits {
                max_overlap_edge_tests: limits.max_overlap_edge_tests - 1,
                ..limits
            },
        ] {
            assert_eq!(
                plan.validate_binding(
                    &mesh,
                    &config,
                    frame(),
                    &layers,
                    &candidates,
                    &reservations,
                    changed_limits,
                ),
                Err(GenericPerceptionError::StaleAuthority)
            );
        }
        let wrong_plate = BuildFrame::new(
            Finite::new(-1.0).unwrap(),
            PositiveFinite::new(5.0).unwrap(),
        );
        assert_eq!(
            prepare_generic_contacts(
                &mesh,
                &config,
                wrong_plate,
                &layers,
                &candidates,
                &reservations,
                limits,
            ),
            Err(GenericPerceptionError::FrameMismatch)
        );
    }

    #[test]
    fn generic_plan_digest_is_identical_at_one_four_and_sixteen_threads() {
        if std::env::var(SWEEP_CHILD).as_deref() != Ok("1") {
            let rows = [1usize, 4, 16]
                .into_iter()
                .map(|threads| {
                    let output = std::process::Command::new(
                        std::env::current_exe().expect("current test executable"),
                    )
                    .args([
                        "--exact",
                        "perception::tests::generic_plan_digest_is_identical_at_one_four_and_sixteen_threads",
                        "--nocapture",
                    ])
                    .env("PLINTH_THREADS", threads.to_string())
                    .env(SWEEP_CHILD, "1")
                    .output()
                    .expect("spawn thread-sweep child");
                    assert!(
                        output.status.success(),
                        "thread child {threads} failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let line = stdout
                        .lines()
                        .find_map(|line| line.strip_prefix(SWEEP_LINE))
                        .expect("thread child emitted evidence");
                    let values = line
                        .split(',')
                        .map(|value| value.parse::<u64>().unwrap())
                        .collect::<Vec<_>>();
                    assert_eq!(values[2], threads as u64);
                    values
                })
                .collect::<Vec<_>>();
            assert_eq!(rows[0][0], rows[1][0]);
            assert_eq!(rows[0][0], rows[2][0]);
            assert_eq!(rows[0][1], rows[1][1]);
            assert_eq!(rows[0][1], rows[2][1]);
            return;
        }

        // One thousand real in-bounds slice tasks make participation evidence
        // robust even when the whole workspace test suite is concurrently
        // busy; a tiny sphere can legitimately be completed by one worker.
        let mesh = plinth_mesh::make_box([10.0, 10.0, 50.0], [0.0; 3]).unwrap();
        let delicacy = vec![0.0; mesh.tris().len()];
        let config = SupportPerceptionConfig::current(
            Finite::new(0.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            64 * 1024 * 1024,
            Some(&delicacy),
        )
        .unwrap();
        let layers = LayerPlan::new(0.0, vec![0.05; 1_001]).unwrap();
        let plan = prepare_generic_contacts(
            &mesh,
            &config,
            frame(),
            &layers,
            &candidate_config(),
            &[],
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
        )
        .expect("Sphere perception");
        let (effective, participating) = plan.slice_thread_evidence();
        let load_partition = test_partition_rows(vec![
            FinalizedFaceLoad {
                face_id: 0,
                contact_id: 100,
                owner: LoadOwner::GenericIsland(0),
                carried_area_mm2: 3.0,
            },
            FinalizedFaceLoad {
                face_id: 1,
                contact_id: 10,
                owner: LoadOwner::GenericIsland(0),
                carried_area_mm2: 5.0,
            },
        ]);
        let loads = FinalizedContactLoads::from_partition(
            &load_partition,
            &[10, 100, u64::MAX],
            final_load_limits(3, 2),
        )
        .unwrap();
        println!(
            "{SWEEP_LINE}{},{},{effective},{participating}",
            plan.digest(),
            loads.digest()
        );
    }
}
