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
    finalize_load_rows(
        plan,
        mesh,
        &plan.load_eligibility,
        contacts,
        plan.load_limits,
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
    finalize_load_rows(plan, mesh, &[], &[], plan.load_limits)
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

fn load_resource_plan(
    load_eligibility: &[LoadEligibility],
    contacts: &[PhysicalContact],
) -> Result<LoadResourcePlan, GenericPerceptionError> {
    let eligible_owner_count = load_eligibility.iter().try_fold(0usize, |total, row| {
        total
            .checked_add(row.eligible_owners.len())
            .ok_or(GenericPerceptionError::LoadResourceOverflow)
    })?;
    load_resource_plan_from_counts(
        load_eligibility.len(),
        eligible_owner_count,
        contacts.len(),
        0,
    )
}

fn finalize_load_rows(
    plan: &GenericContactPlan,
    mesh: &Mesh,
    load_eligibility: &[LoadEligibility],
    contacts: &[PhysicalContact],
    limits: LoadFinalizeLimits,
) -> Result<FinalizedLoadPartition, GenericPerceptionError> {
    let fingerprint = mesh_fingerprint(mesh)
        .map_err(|error| GenericPerceptionError::Topology(error.to_string()))?;
    if fingerprint != plan.authority.mesh {
        return Err(GenericPerceptionError::StaleAuthority);
    }
    let resource = load_resource_plan(load_eligibility, contacts)?;
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
    // One flattened owner x face x contact batch.  The matrix is streamed into
    // a caller-budgeted nearest row rather than issuing scalar spatial calls.
    let mut nearest = vec![None::<(usize, f64)>; load_eligibility.len()];
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
/// but the pair list arrives one entry per `(owner, face)`: 7,558,958 pairs
/// over 429,120 faces on the Weeping Treant, 18.6 per face.  Everything that
/// sized itself on the pair count was therefore 18.6x too large — the row
/// count that drives the `rows x contacts x (owner_log + 4)` load operation
/// bound, and the retained bytes that drive the load scratch bound.  The
/// grouping is built once, in one sort, and read by both.
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
    mesh: &Mesh,
    surface: &NativeSurface,
    contacts: &[PhysicalContact],
    specialized_owner_faces: &[(u32, u32)],
    limits: LoadFinalizeLimits,
) -> Result<FinalizedLoadPartition, GenericPerceptionError> {
    let fingerprint = mesh_fingerprint(mesh)
        .map_err(|error| GenericPerceptionError::Topology(error.to_string()))?;
    if fingerprint != plan.authority.mesh {
        return Err(GenericPerceptionError::StaleAuthority);
    }
    // A child specialized-plan edit may intentionally remove every
    // demand-free regular contact.  There is then no physical namespace to
    // which generic faces may be assigned.  Canonicalize that real zero case
    // through the same finalizer, with an empty eligibility set, rather than
    // fabricating rows or silently retaining the parent's load artifact.
    if contacts.is_empty() {
        return finalize_load_rows(plan, mesh, &[], contacts, limits);
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
    finalize_load_rows(plan, mesh, &extended, contacts, limits)
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
        let resource = load_resource_plan(&plan.load_eligibility, &contacts).unwrap();
        assert!(finalize_load_rows(
            &plan,
            &mesh,
            &plan.load_eligibility,
            &contacts,
            LoadFinalizeLimits {
                max_scratch_bytes: resource.scratch_bytes,
                max_operations_floor: resource.operations,
                operations_per_batch_unit: 0,
            },
        )
        .is_ok());
        assert!(matches!(
            finalize_load_rows(
                &plan,
                &mesh,
                &plan.load_eligibility,
                &contacts,
                LoadFinalizeLimits {
                    max_scratch_bytes: resource.scratch_bytes - 1,
                    max_operations_floor: resource.operations,
                    operations_per_batch_unit: 0,
                },
            ),
            Err(GenericPerceptionError::LoadScratchLimit { .. })
        ));
        assert!(matches!(
            finalize_load_rows(
                &plan,
                &mesh,
                &plan.load_eligibility,
                &contacts,
                LoadFinalizeLimits {
                    max_scratch_bytes: resource.scratch_bytes,
                    max_operations_floor: resource.operations - 1,
                    operations_per_batch_unit: 0,
                },
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
            &mesh,
            &surface,
            &contacts,
            &pairs,
            LoadFinalizeLimits {
                max_scratch_bytes: by_face.scratch_bytes,
                max_operations_floor: by_face.operations,
                operations_per_batch_unit: 0,
            },
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
