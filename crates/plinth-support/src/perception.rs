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

/// One model face this partition charges to **no** physical contact.
///
/// SPEC-A3-A4 §6, task #49. Load eligibility is a *provenance* relation, not a
/// reach test: [`build_load_eligibility`] makes a face eligible for
/// `LoadOwner::GenericIsland(id)` iff the face is IN that island, and for
/// `LoadOwner::Specialized(owner)` iff a reservation names it. So a face
/// arrives here with no eligible contact exactly when every contact of every
/// owner eligible for it is gone — after §6's removal, when every such contact
/// went unrouted.
///
/// **The honest answer is that nothing carries it, and the finalizer says so.**
/// The predecessor refused instead, which asserted more than it had measured:
/// eligibility is not reach, so "no eligible owner has a contact" is not "no
/// support stands under this face", and the quantity that decides whether the
/// face is a problem — how much of it the routed layout actually leaves
/// uncovered — is measured downstream by the coverage judgment, over its own
/// samples, and forgiven or refused there under the registered A3 censor. This
/// row is the disclosure that makes the two legible side by side.
///
/// The area is the eligibility row's own `area_mm2`, bit for bit — the same
/// number a row that DID find an owner would have carried.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OwnerlessLoadFace {
    pub face_id: u32,
    pub area_mm2: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FinalizedLoadPartition {
    authority: AuthorityFingerprint,
    contacts_fingerprint: u64,
    rows: Vec<FinalizedFaceLoad>,
    /// Every eligibility row that found no contact, ascending by the order the
    /// eligibility itself was walked. Empty on every model that has a contact
    /// for each of its eligible owners, which is the ordinary case.
    ownerless: Vec<OwnerlessLoadFace>,
}

impl FinalizedLoadPartition {
    #[must_use]
    pub fn rows(&self) -> &[FinalizedFaceLoad] {
        &self.rows
    }

    /// The faces this partition charges to nobody — see [`OwnerlessLoadFace`].
    #[must_use]
    pub fn ownerless_faces(&self) -> &[OwnerlessLoadFace] {
        &self.ownerless
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
        self.validate_binding_hoisted(
            partition,
            canonical_contact_ids,
            finalized_load_rows_fingerprint(&partition.rows),
        )
    }

    /// The same binding validation, against a row fingerprint hashed **once**
    /// by a caller that holds the partition immutably for a whole stage.
    ///
    /// # FU-32 phase J1 — the G2 rule, applied here
    ///
    /// `finalized_load_rows_fingerprint` walks every `FinalizedFaceLoad` row —
    /// 70,737 rows on the Weeping Treant v1.2 — and the CCO routing loop paid
    /// for it **twice per grown terminal**, over a partition owned by a plan
    /// held behind a shared reference for the entire loop, in a type with no
    /// interior mutability.
    ///
    /// **Every comparison stays.**  All four arms still run at this site; only
    /// the row walk behind the third one moved to the caller's once-per-stage
    /// hoist.  That arm is not vacuous: `self.rows_fingerprint` is this
    /// artifact's *own* sealed field, so a hoisted fingerprint taken from a
    /// different partition — or a corrupted one — disagrees with it and
    /// refuses.  See [`VerifiedMeshFingerprint`] for the G2 precedent this
    /// copies.
    #[allow(
        dead_code,
        reason = "the sealed support runner consumes the hoisted-authority form"
    )]
    pub(crate) fn validate_binding_hoisted(
        &self,
        partition: &FinalizedLoadPartition,
        canonical_contact_ids: &[u64],
        hoisted_rows_fingerprint: u64,
    ) -> Result<(), GenericPerceptionError> {
        if self.authority != partition.authority
            || self.contacts_fingerprint != partition.contacts_fingerprint
            || self.rows_fingerprint != hoisted_rows_fingerprint
            || self.contact_ids != canonical_contact_ids
        {
            return Err(GenericPerceptionError::StaleAuthority);
        }
        Ok(())
    }

    /// Hash a partition's rows once, for a caller that will then present the
    /// value to [`Self::validate_binding_hoisted`] at every site that used to
    /// recompute it.  FU-32 phase J1.
    #[allow(
        dead_code,
        reason = "the sealed support runner hoists this once per routing stage"
    )]
    pub(crate) fn hoist_rows_fingerprint(partition: &FinalizedLoadPartition) -> u64 {
        finalized_load_rows_fingerprint(&partition.rows)
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
/// the contact check agree with the tip-sizing check at the `[0.05, 1.0]`
/// mm-RADIUS clamp boundaries (`[0.1, 2.0]` mm DIAMETER) by construction
/// instead of by inspection.
///
/// UNITS: every radius on this struct is a RADIUS in mm.  The owner and the
/// ChituBox UI speak DIAMETER, so a number transcribed from either lands 2x
/// wrong unless it is halved first.  The minimum is `TIP_RADIUS_DEFAULT_MM`
/// = 0.05 mm radius (0.1 mm diameter) since the owner ruling of 2026-08-19,
/// from 0.2 mm radius (0.4 mm diameter); the maximum is
/// `TIP_RADIUS_MAX_LEGACY_MM` = 1.0 mm radius (2.0 mm diameter), which that
/// same ruling deliberately left alone.
///
/// The minimum is a PERMISSION, not a target.  `tip_radius_mm` still sizes
/// every tip from the load law, so the minimum can only bind where the
/// carried area is at or below `implied_area_ratio() * pi * 0.05^2`
/// = 0.0942 mm2.  A large carried area is sized up, and refused by
/// `violates_safety_factor`, exactly as it was at 0.2 mm radius.
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

    /// The largest carried area this policy can legally hold on ONE contact:
    /// `X * pi * r_max^2`, where `X` is [`Self::implied_area_ratio`] and
    /// `r_max` is the registered tip-radius clamp maximum (mm RADIUS).
    ///
    /// It is the same inequality [`Self::violates_safety_factor`] evaluates,
    /// solved for area at `r = r_max`, so the two agree at the boundary by
    /// construction rather than by inspection — the delegation rule
    /// `crate::contact` already follows for the other direction. Nothing new
    /// is registered: both terms come from the frozen policy this struct was
    /// built from.
    pub(crate) fn max_carriable_area_mm2(self) -> f64 {
        self.implied_area_ratio() * std::f64::consts::PI * self.max_radius_mm * self.max_radius_mm
    }

    /// Hand-built mirror of the production policy for `crate::contact`'s
    /// tests.  It is a MIRROR: when a registry value moves, this moves with
    /// it, or the contact tests quietly start testing a policy that no
    /// longer ships.  `min_radius_mm` is 0.05 mm RADIUS (0.1 mm DIAMETER)
    /// to match `TIP_RADIUS_DEFAULT_MM` after the owner ruling of
    /// 2026-08-19; it was 0.2 mm radius (0.4 mm diameter).
    #[cfg(test)]
    pub(crate) fn for_contact_tests() -> Self {
        Self {
            pressure_pa: 3000.0,
            tensile_pa: 0.288 * 1.0e6,
            required_safety_factor: 8.0,
            min_radius_mm: 0.050,
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
    #[error("slice orientation names face {face_id} outside the {faces}-face normal table")]
    OrientationFaceOutOfRange { face_id: u32, faces: usize },
    #[error("slice orientation could not start its thread pool: {0}")]
    OrientationPool(String),
    #[error("slice layers could not be normalized: {0}")]
    LayerUnion(String),
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
    #[error(
        "an incremental load append was handed a prior partition that is not this plan's parent"
    )]
    MismatchedPriorLoadPartition,
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
    // One batched stitch -> orient -> normalize for the whole slice batch.
    // The hand-summed `max_loop_vertices` accounting that used to sit in this
    // loop now rides inside the normalizer as its output-vertex cap (design
    // sec 3.4), so the count is charged against normalized output rather than
    // the pre-union walk.
    let slice_layers = normalized_slice_layers(
        SliceStitchSpace::PrintLayer,
        &sliced,
        geometry.normals(),
        limits,
    )?;
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

/// Which caller a slice-stitch invocation belongs to.
///
/// The two call sites hand the stitcher indices from different spaces: a
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

/// The index space a whole stitch batch belongs to.
///
/// [`SliceStitchSite`] names one layer; this names the space a batch's indices
/// live in, so a batched call can mint the right site per index without either
/// call site walking layers itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SliceStitchSpace {
    PrintLayer,
    AdequacyProbe,
}

impl SliceStitchSpace {
    const fn site(self, index: usize) -> SliceStitchSite {
        match self {
            Self::PrintLayer => SliceStitchSite::PrintLayer(index),
            Self::AdequacyProbe => SliceStitchSite::AdequacyProbe(index),
        }
    }
}

/// Stitch, orient and normalize every layer of one kernel slice batch.
///
/// FU-10 phase C: the cutover both stitch sites now go through -- both stitch
/// sites consume normalized output (design history: phase table row C, git
/// log only, the design doc that recorded it has been retired). The
/// pipeline is exactly the three stages that design prescribes -- stitch (the
/// balanced directed walk inside
/// [`orientation::orient_layer`]) -> orient (the phase-A `z_hat x n`
/// derivation) -> normalize (the phase-B per-layer union) -- and it is
/// batch-first at every stage: one `slice_z_batch` upstream, one
/// `orient_layers_batch`, one `normalize_layers_batch`. No per-layer entry
/// point survives in either hot path.
///
/// # 2026-08-07: the deferred parity work is done
///
/// The stitcher this replaces classified hole-versus-solid by the even-odd
/// nesting parity of each loop's FIRST VERTEX -- a point that lies exactly ON
/// its own loop and, in a tangle, within `LOOP_MERGE_MM` of a neighbour's,
/// where ray parity has no defined answer. Two comments in that code admitted
/// the debt and deferred it by name: "Normalizing parity itself waits on the
/// fill-rule work", and, of the better interior probe, "gated behind the
/// fill-rule normalization work". **This is that work, and this note is its
/// dated resolution.** Parity is not normalized here -- it is retired. Hole
/// ownership is the union engine's own shape structure, decided in exact
/// integer arithmetic on explicitly quantized coordinates, so:
///
/// * no containment question is asked at a boundary-degenerate point at all,
///   and the `interior_probe` that existed to dodge that degeneracy is
///   deleted along with the parity count that made it necessary;
/// * the Tier-1 unowned-hole fallback chain -- vertex probe, then interior
///   probe, then any-vertex, then **bounding-box containment**, and finally a
///   **silent drop** of any hole nothing claimed -- is deleted entire. On
///   normalized output a hole's owner is unique by construction: there is no
///   query to widen and nothing to drop. FU-16, whose closure measurement was
///   "bbox claiming can under-report a solid's area; the Tier 2 union
///   supersedes it", loses its subject here.
///
/// Already-normal layers are unaffected BY CONSTRUCTION, not by luck: the
/// phase-B normalizer compares its union output against its input under
/// canonical integer form and, on equality, emits the ORIGINAL f64 loops
/// verbatim (design sec 3.5). Measured at the phase-C tip, that holds on 100%
/// of already-normal layers.
///
/// It does NOT hold for the whole corpus, and the design record says why
/// rather than hiding it: sec 8.5 records the census run at this tip, where
/// three of seven digests are byte-identical and four moved. The moving layers
/// carry two or more counter-clockwise loops that ABUT -- contours touching
/// where the vertex-only legacy stitcher could not see it -- and the nonzero
/// union merges them, which is design case D1 arriving on the clean corpus
/// rather than only on the treant. Nothing was re-pinned to absorb it.
fn normalized_slice_layers(
    space: SliceStitchSpace,
    batch: &geom_kernel::PlaneSliceBatch,
    normals: &[[f64; 3]],
    limits: GenericPerceptionLimits,
) -> Result<Vec<Vec<SliceSolid>>, GenericPerceptionError> {
    let oriented = orientation::orient_layers_batch(space, batch, normals)?;
    let layers = oriented
        .into_iter()
        .map(|layer| plinth_layers::LayerLoops {
            // Design sec 3.3's degraded path: a layer whose derived winding
            // was locally inconsistent carries no meaningful direction, so
            // parity is the only defensible fill for it. Typed, per-layer,
            // and never an abort.
            fill_rule: if layer.degraded.is_some() {
                plinth_layers::LayerFillRule::EvenOdd
            } else {
                plinth_layers::LayerFillRule::NonZero
            },
            loops: layer
                .loops
                .into_iter()
                .map(|oriented_loop| oriented_loop.points_xy_mm)
                .collect(),
        })
        .collect::<Vec<_>>();
    // The union's caps are derived, not invented. Perception already caps the
    // kernel slice stream, and a stitched layer holds at most two endpoints
    // per admitted intersection, so `max_slice_intersections * 2` is the exact
    // input-vertex bound this stage inherits; the operation and scratch caps
    // follow from it (`LayerUnionLimits::from_input_bound`). `max_loop_vertices`
    // is the authority-carried output cap that the print-layer call site used
    // to sum by hand; design sec 3.4 moves it onto normalized output, which is
    // where it now binds -- including for the adequacy probes, which never had
    // it. Adding new cap fields to `GenericPerceptionLimits` was the
    // alternative and is refused: they would be unregistered constants, and
    // any that entered `authority_fingerprint` would move every frozen digest
    // for a guard that cannot change a successful plan.
    let union_limits = plinth_layers::LayerUnionLimits::from_input_bound(
        limits.max_slice_intersections.saturating_mul(2),
        limits.max_loop_vertices,
    );
    let normalized =
        plinth_layers::normalize_layers_batch(&layers, BIRTH_MIN_AREA_MM2, &union_limits).map_err(
            |error| match error {
                plinth_layers::LayerUnionError::OutputVertexCap { requested, cap } => {
                    GenericPerceptionError::LoopVertexLimit {
                        required: requested,
                        limit: cap,
                    }
                }
                other => GenericPerceptionError::LayerUnion(other.to_string()),
            },
        )?;
    Ok(normalized
        .layers
        .into_iter()
        .map(|layer| {
            layer
                .solids
                .into_iter()
                .map(|solid| SliceSolid {
                    outer: normalized_slice_loop(solid.outer, false),
                    holes: solid
                        .holes
                        .into_iter()
                        .map(|hole| normalized_slice_loop(hole, true))
                        .collect(),
                })
                .collect()
        })
        .collect())
}

/// Carry one normalized loop into the perception loop type.
///
/// A pure re-wrapping: the points and their shoelace area come from the
/// normalizer unmodified, and `hole` is the engine's structural verdict
/// (outer contour versus interior contour), not a re-derived parity answer.
fn normalized_slice_loop(source: plinth_layers::NormalizedLoop, hole: bool) -> SliceLoop {
    SliceLoop {
        points_xy_mm: source.points_xy_mm,
        signed_area_mm2: source.signed_area_mm2,
        hole,
    }
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

/// FU-10 phase A -- consumer-side in-plane loop orientation.
///
/// Design history: phase table row A (git log only, the design doc that
/// recorded it has been retired). The frozen kernel slice stream carries no in-plane
/// winding convention: segment endpoint order is an artifact of which triangle
/// vertex is the odd one out (`geom_kernel::slicing`). For a face with outward
/// normal `n` cut by a horizontal plane, the in-plane direction that keeps the
/// solid interior on the LEFT is `d = z_hat x n = (-n.y, n.x)`. Orienting
/// every stitched segment along `d` turns each closed loop into a wound
/// contour: outer boundaries walk counter-clockwise (positive shoelace area)
/// and hole boundaries clockwise (negative) -- the nonzero-rule input the
/// phase-B union normalizer consumes.
///
/// Everything here is consumer-side: `geom_kernel::slicing` and its frozen
/// Trimesh vectors are untouched. Since the phase-C cutover
/// ([`normalized_slice_layers`]) this is the ONLY stitch in perception -- the
/// undirected walk survives inside it as the degraded path, not as a second
/// copy.
///
/// Task #32 replaced this stitcher's seam guard and its walk: the segment
/// multiset is now directed and uncollapsed, the guard is directed balance,
/// and the walk is a Hierholzer decomposition. See [`orientation::orient_layer`]
/// for the invariant and why the old one was wrong.
mod orientation {
    use std::collections::{BTreeMap, BTreeSet};

    use geom_kernel::{PlaneIntersection, PlaneSliceBatch};
    use rayon::prelude::*;

    use super::{
        canonicalize_loop, point_key, signed_area, GenericPerceptionError, PointKey,
        SliceStitchSite, SliceStitchSpace,
    };

    /// One stitched slice loop carrying its derived winding.
    #[derive(Clone, Debug, PartialEq)]
    pub(super) struct OrientedLoop {
        /// Loop vertices in walk order, rotated so the minimum [`point_key`]
        /// vertex comes first. Never reversed: on a non-degraded layer the
        /// walk order IS the derived direction.
        pub(super) points_xy_mm: Vec<[f64; 2]>,
        /// Shoelace area over `points_xy_mm`. On a non-degraded layer the
        /// sign is the loop's nonzero-rule verdict: positive bounds solid on
        /// its left (an outer boundary), negative bounds a hole.
        pub(super) signed_area_mm2: f64,
    }

    /// Typed per-layer diagnostic for the spec sec 3.3 degraded path: why the
    /// derived winding was locally inconsistent and loop direction reverted
    /// to the raw undirected walk (even-odd fallback territory for the
    /// phase-B union). "Not a silent fallback, not an abort."
    ///
    /// Only one thing degrades a layer now: a segment whose direction vote
    /// abstained, which leaves that segment with no direction to walk. The
    /// two vote-bookkeeping counts this carried before (`unvoted_edges` and
    /// `conflicting_edges`) belonged to the collapsed undirected edge map and
    /// died with it -- one directed edge per emitted segment has nothing to
    /// conflict with -- and an unbalanced vertex is no longer a degradation
    /// at all: it is the refusal below.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(super) struct WindingDegradation {
        /// Segments whose alignment vote abstained: the segment lies closer
        /// to perpendicular to `z_hat x n` than parallel, as happens on a
        /// near-horizontal face or a degenerate normal.
        pub(super) ambiguous_segments: usize,
    }

    /// All stitched loops of one slice plane plus the winding verdict.
    #[derive(Clone, Debug, PartialEq)]
    pub(super) struct OrientedLayer {
        /// Deterministic order: ascending [`point_key`] of each loop's first
        /// (minimum-key) vertex. A vertex may appear in more than one loop
        /// (a pinch), so the order is made total by the walk's own start
        /// order rather than by vertex ownership.
        pub(super) loops: Vec<OrientedLoop>,
        /// `None` when every loop carries a consistent derived winding;
        /// `Some` says exactly why orientation degraded for this layer.
        pub(super) degraded: Option<WindingDegradation>,
    }

    /// The `z_hat x n` alignment vote for one segment: `Some(true)` keeps
    /// the endpoint order `a -> b`, `Some(false)` reverses it, and `None`
    /// abstains because the segment lies closer to perpendicular to the
    /// derived direction than parallel (near-horizontal face, degenerate
    /// normal, or non-finite input -- NaN comparisons abstain rather than
    /// guess).
    pub(super) fn direction_vote(a: [f64; 2], b: [f64; 2], normal: [f64; 3]) -> Option<bool> {
        let segment = [b[0] - a[0], b[1] - a[1]];
        // d = z_hat x n projected to the slice plane.
        let derived = [-normal[1], normal[0]];
        let along = segment[0] * derived[0] + segment[1] * derived[1];
        // cross(segment, d) = segment . n_xy: deviation from parallel.
        let across = segment[0] * derived[1] - segment[1] * derived[0];
        (along.abs() > across.abs()).then_some(along > 0.0)
    }

    /// Stitch one plane's segments into closed loops with derived winding.
    ///
    /// # The seam invariant is directed balance, not degree two
    ///
    /// This used to collapse coincident segments into one undirected edge and
    /// demand undirected degree exactly two at every snapped vertex. That is a
    /// *simple-loops-only* rule, strictly stronger than closure, and correct
    /// closed geometry violates it in two measured ways (task #32 diagnosis,
    /// secs 3 and 7): the plane meets a coplanar facet corner, where four
    /// geometrically distinct contour edges genuinely meet at one point key;
    /// and the plane contains a tangent keel edge whose two owner faces are
    /// both above it, so the frozen kernel emits that edge twice.
    ///
    /// What a closed, consistently wound mesh actually guarantees is that the
    /// DIRECTED segment multiset balances: at every point key, out-degree
    /// equals in-degree. So each emitted segment now becomes one directed
    /// edge -- coincident segments are kept, not collapsed -- balance is the
    /// guard, and the loops come out of a Hierholzer decomposition that
    /// consumes every edge exactly once. A genuine hole or a flipped face
    /// still fails balance and still raises the site's seam error, at every
    /// plane the defect spans.
    ///
    /// Unlike the legacy path there is NO area filter here: the phase-B union
    /// must see every loop, including large self-cancelling ones (spec sec
    /// 3.2), so degenerate-output filtering happens after the union, never
    /// before it. The one exception is structural rather than metric: a walk
    /// that closes on fewer than three distinct point keys is not a polygon at
    /// all (a doubly-emitted tangent edge can close as A->B->A), carries no
    /// area to lose, and is dropped before the union -- which would discard it
    /// anyway, since `plinth_layers::normalize_layers_batch` feeds the overlay
    /// only loops of three points or more.
    pub(super) fn orient_layer(
        site: SliceStitchSite,
        intersections: &[PlaneIntersection],
        normals: &[[f64; 3]],
    ) -> Result<OrientedLayer, GenericPerceptionError> {
        let mut points = BTreeMap::<PointKey, [f64; 2]>::new();
        // One directed edge per emitted segment, bucketed by source. Nothing
        // is deduplicated: multiplicity is the whole point of the invariant.
        let mut out_edges = BTreeMap::<PointKey, Vec<PointKey>>::new();
        let mut ambiguous_segments = 0usize;
        for event in intersections {
            let a = [event.segment[0][0], event.segment[0][1]];
            let b = [event.segment[1][0], event.segment[1][1]];
            let ka = point_key(a);
            let kb = point_key(b);
            if ka == kb {
                continue;
            }
            let Some(&normal) = normals.get(event.face_id as usize) else {
                return Err(GenericPerceptionError::OrientationFaceOutOfRange {
                    face_id: event.face_id,
                    faces: normals.len(),
                });
            };
            points.entry(ka).or_insert(a);
            points.entry(kb).or_insert(b);
            match direction_vote(a, b, normal) {
                Some(true) => out_edges.entry(ka).or_default().push(kb),
                Some(false) => out_edges.entry(kb).or_default().push(ka),
                None => ambiguous_segments += 1,
            }
        }

        // An abstaining segment contributes no directed edge, so directed
        // balance cannot be asked of a layer that has one: the missing edge
        // would look exactly like a hole. Such a layer degrades to the
        // direction-free walk, which checks balance's undirected consequence
        // (even degree everywhere) instead. Measured across the whole corpus:
        // 0 abstaining votes on 4,049 planes, so this path stays narrow --
        // which matters, because the union runs it under EvenOdd, and EvenOdd
        // is not inert to how a pinch is paired.
        let consistent = ambiguous_segments == 0;
        let key_loops = if consistent {
            let mut in_degree = BTreeMap::<PointKey, usize>::new();
            for destinations in out_edges.values() {
                for &destination in destinations {
                    *in_degree.entry(destination).or_default() += 1;
                }
            }
            // The seam guard: out-degree == in-degree at every point key.
            let balanced = points.keys().all(|key| {
                out_edges.get(key).map_or(0, Vec::len) == in_degree.get(key).copied().unwrap_or(0)
            });
            // Early exit, not the only guard: an unbalanced multiset would
            // also be refused by `walk_directed`'s starved arm below, since
            // no walk leaving an unbalanced vertex can return to its start.
            // This check just says so up front, without running a walk first.
            if !balanced {
                return Err(site.open_seam());
            }
            walk_directed(&mut out_edges, site)?
        } else {
            // The same multiset the main loop built above, with direction
            // discarded, rebuilt here rather than accumulated on every
            // layer: measured 0 abstains across 4,049 corpus planes, so this
            // branch — and the allocation it needs — is cold in practice.
            let undirected = intersections
                .iter()
                .filter_map(|event| {
                    let a = [event.segment[0][0], event.segment[0][1]];
                    let b = [event.segment[1][0], event.segment[1][1]];
                    let ka = point_key(a);
                    let kb = point_key(b);
                    (ka != kb).then(|| if ka < kb { (ka, kb) } else { (kb, ka) })
                })
                .collect::<Vec<_>>();
            walk_undirected(&undirected, site)?
        };

        let mut loops = key_loops
            .into_iter()
            .filter(|keys| keys.iter().copied().collect::<BTreeSet<_>>().len() >= 3)
            .map(|keys| {
                let canonical =
                    canonicalize_loop(keys.into_iter().map(|key| points[&key]).collect(), false);
                OrientedLoop {
                    signed_area_mm2: signed_area(&canonical),
                    points_xy_mm: canonical,
                }
            })
            .collect::<Vec<_>>();
        // Two loops can now share a minimum point key -- that is what a pinch
        // is -- so this key alone is no longer a total order. The sort is
        // stable, so ties keep the walk's own pinned order and the result stays
        // deterministic.
        loops.sort_by(|left, right| {
            point_key(left.points_xy_mm[0]).cmp(&point_key(right.points_xy_mm[0]))
        });
        Ok(OrientedLayer {
            loops,
            degraded: (!consistent).then_some(WindingDegradation { ambiguous_segments }),
        })
    }

    /// Decompose the balanced directed multigraph into closed walks
    /// (Hierholzer), consuming every edge exactly once.
    ///
    /// # Determinism is pinned, not incidental
    ///
    /// Any Eulerian decomposition of a multigraph depends on the order edges
    /// are taken, and that order reaches the frozen slice digests. The rule is
    /// therefore fixed here and must not be "tidied": start keys ascending,
    /// each out-edge list sorted ascending, and the NEXT edge popped from the
    /// END of that list. Measured (diagnosis secs 9 and 11): that exact
    /// ordering reproduced all 8,099 passing layers of the parity corpus
    /// byte-for-byte. On the degree-two graphs that make up almost every layer
    /// each list holds one entry, so the walk is the same permutation walk it
    /// replaces.
    ///
    /// Balance guarantees a walk leaving `start` can always leave any vertex
    /// it enters, so it must return to `start`. The starved arm cannot be
    /// reached from a balanced multiset; it refuses rather than emit an open
    /// chain as if it were a loop.
    fn walk_directed(
        out_edges: &mut BTreeMap<PointKey, Vec<PointKey>>,
        site: SliceStitchSite,
    ) -> Result<Vec<Vec<PointKey>>, GenericPerceptionError> {
        for destinations in out_edges.values_mut() {
            destinations.sort_unstable();
        }
        let starts = out_edges.keys().copied().collect::<Vec<_>>();
        let mut loops = Vec::new();
        for start in starts {
            while let Some(first) = out_edges.get_mut(&start).and_then(Vec::pop) {
                let mut keys = vec![start];
                let mut current = first;
                while current != start {
                    keys.push(current);
                    let Some(next) = out_edges.get_mut(&current).and_then(Vec::pop) else {
                        return Err(site.open_seam());
                    };
                    current = next;
                }
                loops.push(keys);
            }
        }
        Ok(loops)
    }

    /// The direction-free walk for a degraded layer, over the same segment
    /// multiset.
    ///
    /// Multigraph-safe: edges are consumed by identity, so a doubly-emitted
    /// tangent edge is traversed twice rather than mistaken for one edge, and
    /// a pinch vertex of degree four is walked through twice rather than
    /// indexed as `neighbors[0]`/`neighbors[1]`. The guard it can apply is the
    /// direction-free consequence of directed balance -- every point key must
    /// have even degree -- which still refuses a hole; it cannot see a flipped
    /// face, because without a direction vote there is no direction to
    /// contradict. Same pinned ordering as [`walk_directed`].
    fn walk_undirected(
        undirected: &[(PointKey, PointKey)],
        site: SliceStitchSite,
    ) -> Result<Vec<Vec<PointKey>>, GenericPerceptionError> {
        let mut incidence = BTreeMap::<PointKey, Vec<(PointKey, usize)>>::new();
        for (index, &(a, b)) in undirected.iter().enumerate() {
            incidence.entry(a).or_default().push((b, index));
            incidence.entry(b).or_default().push((a, index));
        }
        for incident in incidence.values_mut() {
            incident.sort_unstable();
        }
        if incidence.values().any(|incident| incident.len() % 2 != 0) {
            return Err(site.open_seam());
        }
        let mut used = vec![false; undirected.len()];
        let starts = incidence.keys().copied().collect::<Vec<_>>();
        let mut loops = Vec::new();
        for start in starts {
            while let Some((first, edge)) = pop_unused(&mut incidence, &used, start) {
                used[edge] = true;
                let mut keys = vec![start];
                let mut current = first;
                while current != start {
                    keys.push(current);
                    let Some((next, edge)) = pop_unused(&mut incidence, &used, current) else {
                        return Err(site.open_seam());
                    };
                    used[edge] = true;
                    current = next;
                }
                loops.push(keys);
            }
        }
        Ok(loops)
    }

    /// Take one unused incident edge of `vertex`, newest first, discarding
    /// entries already consumed from their other endpoint.
    fn pop_unused(
        incidence: &mut BTreeMap<PointKey, Vec<(PointKey, usize)>>,
        used: &[bool],
        vertex: PointKey,
    ) -> Option<(PointKey, usize)> {
        let incident = incidence.get_mut(&vertex)?;
        while let Some((neighbor, edge)) = incident.pop() {
            if !used[edge] {
                return Some((neighbor, edge));
            }
        }
        None
    }

    /// Orient every layer of a kernel slice batch in one call.
    ///
    /// Batch-first per the standing rule and spec sec 3.4: one rayon
    /// `par_iter` over layers (layers are independent), deterministic
    /// assembly by layer index, index-parallel map-collect only. The pool
    /// width follows the same `PLINTH_THREADS` authority as the kernel's own
    /// batch calls.
    pub(super) fn orient_layers_batch(
        space: SliceStitchSpace,
        batch: &PlaneSliceBatch,
        normals: &[[f64; 3]],
    ) -> Result<Vec<OrientedLayer>, GenericPerceptionError> {
        let layer_count = batch.plane_offsets.len().saturating_sub(1);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(geom_kernel::current_threads())
            .build()
            .map_err(|error| GenericPerceptionError::OrientationPool(error.to_string()))?;
        let layers: Vec<Result<OrientedLayer, GenericPerceptionError>> = pool.install(|| {
            (0..layer_count)
                .into_par_iter()
                .map(|layer| orient_layer(space.site(layer), batch.plane(layer), normals))
                .collect()
        });
        // Collected in index order, so a raised seam error is the LOWEST
        // failing layer's -- the same one the per-layer stitch loops this
        // replaced would have surfaced first.
        layers.into_iter().collect()
    }
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
    // Task #62: the adequacy yardstick is a REACH, in mm, not a carried area.
    // Derivation: docs/evidence/derivations/low-point-reach-2026-08-19.md.
    // Both inputs are read from the M0 registry here, so no tip decision and
    // no placeholder tuning can move a low-point verdict again.
    let allowance_mm = registry_low_point_reach_allowance_mm(constants);
    // OQ-3, answered yes in the design: the adequacy probes go through the
    // same normalized path as the print layers -- one code path, and adequacy
    // areas are exactly where the deleted bbox/drop distortions bit (sec 2 D5).
    // Batched with the print-layer site, not per probe row.
    let probe_layers = normalized_slice_layers(
        SliceStitchSpace::AdequacyProbe,
        &adequacy_slices,
        surface.geometry().normals(),
        limits,
    )?;
    let mut adequate = vec![false; minima.len()];
    for (probe_row, (minimum_index, minimum)) in suspects.into_iter().enumerate() {
        let reach_mm = region_reach_mm(&probe_layers[probe_row], minimum);
        adequate[minimum_index] = reach_mm <= allowance_mm;
    }
    Ok(minima
        .into_iter()
        .enumerate()
        .filter(|(index, _)| minimum_demands_support(is_birth[*index], adequate[*index]))
        .map(|(_, minimum)| minimum)
        .collect())
}

/// Root bending-moment coefficient for the load concentrated at the peel
/// front, the conservative end of the derived criterion.
///
/// This is a closed-form beam coefficient, not a tunable and not a
/// measurement, so it is deliberately not a registry constant. A shelf of
/// reach `L` and thickness `t` under a total peel force `F` carries root
/// stress `6·F·L/(W·t²)` when that force sits at the free edge, against
/// `3·F·L/(W·t²)` when it is spread evenly over the shelf. Peel is by
/// definition a force concentrated at a travelling separation line, and the
/// root moment peaks when that line reaches the free edge, so 6 is the
/// physical default and 3 is the optimistic bound.
///
/// Craig's ruling, 2026-08-19: ship the conservative end.
const PEEL_FRONT_ROOT_MOMENT_COEFFICIENT: f64 = 6.0;

/// The greatest reach a non-birth low point may have and still be adequately
/// carried, in mm.
///
/// `K = sqrt(X_ALLOWED / 6)` and the allowance is `K · h`. Both `X_ALLOWED`
/// (the registered area law, `Derived` from calibration plate 5) and `h`
/// (`LAYER_HEIGHT_DEFAULT_MM`, `Measured` on calibration plate 6) come from
/// the caller so this formula can be exercised away from the registered
/// values; [`registry_low_point_reach_allowance_mm`] is the one production
/// reader and supplies both from the registry.
///
/// At the registered values the allowance is `sqrt(12/6) × 0.050 mm`
/// = 0.0707 mm of reach.
fn low_point_reach_allowance_mm(x_allowed: f64, layer_height_mm: f64) -> f64 {
    (x_allowed / PEEL_FRONT_ROOT_MOMENT_COEFFICIENT).sqrt() * layer_height_mm
}

/// The sole production reader of the low-point reach allowance, and the sole
/// production reader of `X_ALLOWED` anywhere in the engine (task #57,
/// registry-bypass finding, constants audit sec 2f).
///
/// The layer height comes from the registry, per Craig's ruling, not from the
/// `LayerPlan`. Recorded divergence, not a defect today: the probe slice one
/// layer down is taken at `z - layers.heights_mm()[index_at(z)]`, so if a plan
/// ever carries per-layer heights that differ from `LAYER_HEIGHT_DEFAULT_MM`,
/// the allowance and the probe depth stop describing the same layer. Every
/// plan in the engine is built at the registered height today, so the two
/// agree. If per-layer heights ever land, this is the site that has to choose,
/// and the physical answer is the shelf's own thickness -- the plan value.
///
/// Disclosure: no test here can distinguish this registry read from a
/// hardcoded literal at today's values, because `M0EvidenceConstants`
/// exposes only `new()` -- there is no way to construct one with a
/// different evidence set to substitute in a test. The mitigation
/// is `the_reach_allowance_is_read_from_the_registry_not_hardcoded`, which
/// pins `x_allowed == 12.0` and `layer_height_mm == 0.050`: a future
/// registry change to either value flips that assertion red, even though it
/// cannot prove today that this function reads the registry rather than a
/// constant.
fn registry_low_point_reach_allowance_mm(constants: &plinth_core::M0Constants) -> f64 {
    let evidence = constants.evidence();
    low_point_reach_allowance_mm(
        *evidence.x_allowed().value(),
        *evidence.layer_height_default_mm().value(),
    )
}

/// A minimum reaches the support planner when it is a true island birth, or
/// when it is a non-birth minimum that is not adequately carried.
///
/// Births are unconditional: nothing at all is under a birth, so no reach
/// measurement can excuse one. The birth filter is upstream and untouched by
/// task #62; this predicate is where its unconditional standing is pinned.
const fn minimum_demands_support(is_birth: bool, adequate: bool) -> bool {
    is_birth || !adequate
}

/// How far a low point's region hangs out past the material carrying it, in mm.
///
/// The physics is a cantilever: at a non-birth low point one layer of green
/// resin cures with nothing beneath it, joined sideways within its own layer
/// to resin that IS sitting on the previous layer, and the peel pull bends
/// that tongue at its root. Reach is the tongue's length, so it is the
/// quantity the derived criterion bounds.
///
/// Zero when every sampled region point sits inside the solid one layer down.
/// Otherwise the largest point-to-solid distance over the sampled region:
/// that point is the free end of the shelf.
///
/// KNOWN GAP, filed and not fixed here (1 of 2): this is reach only. A wide
/// shelf hanging off a NARROW ROOT amplifies root stress by
/// `W_shelf / W_root`, without bound, and no reach measurement sees it. The
/// criterion therefore does not catch a wide ledge on a thin stalk, nor a
/// patch joined to carried material by a hair-thin in-plane neck (which is
/// physically a birth island on a thread yet classifies as non-birth with a
/// short reach).
///
/// KNOWN GAP, filed and not fixed here (2 of 2): there is no connectivity
/// requirement. The nearest solid one layer down may belong to a different
/// component -- a neighbouring limb, a separate island, or another model on a
/// crowded plate -- in which case a short reach is scored where there is no
/// load path at all. `carried_outer_area`, the predicate this replaces, had
/// exactly the same hole (`point_in_polygon` over every solid, no component
/// identity), so this is disclosed, not worsened.
///
/// Cost is one point-to-polygon scan per sampled region point, the same order
/// and the same shape of work as [`birth_xy_ok`] in the birth test above.
fn region_reach_mm(solids: &[SliceSolid], minimum: &LocalZMinimum) -> f64 {
    sampled_region_points(minimum)
        .iter()
        .map(|point| point_reach_mm(solids, [point[0], point[1]]))
        .fold(0.0, f64::max)
}

/// Distance in mm from one point to the nearest carrying solid, zero when the
/// point is inside one.
///
/// Infinite when there is no solid at all one layer down, which no finite
/// allowance admits -- a low point over nothing always demands a support.
fn point_reach_mm(solids: &[SliceSolid], point: [f64; 2]) -> f64 {
    if solids.iter().any(|solid| solid_contains(solid, point)) {
        return 0.0;
    }
    solids
        .iter()
        .flat_map(|solid| std::iter::once(&solid.outer).chain(solid.holes.iter()))
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

/// The load finalizer's operation and scratch model, from counts alone.
///
/// # FU-8f phase G5 — the honest post-transpose scan charge
///
/// Until this phase the leading operation term was `face_count x contact_count
/// x (owner_log + 4)`: one binary search into a row's `eligible_owners` for
/// every `(row, contact)` cell, because that is exactly what the finalizer did
/// — it visited every cell and skipped the ineligible ones.  Phase G1 replaced
/// that scan with a contact-major CSR transpose and **deliberately left this
/// model standing and over-stating**, on the FU-8e Q5 precedent that a plan
/// admitting the general path a fortiori admits its specialisation.  Phase G5
/// pays that debt: the quadratic is gone from the charge as well as from the
/// code, and the stated demand falls with it.
///
/// What the finalizer now spends, term by term, in the elementary-comparison
/// unit every term here counts:
///
/// - `eligible_owner_count * (log + 2) * 2` — [`LoadTransposeCounts::count`]
///   and [`LoadTransposeCounts::finish`] each walk the row-major eligibility
///   once and, per `(row, eligible owner)` entry, run one forward
///   `partition_point` over the contact order (`log` comparisons) plus a
///   bounds-and-equality test.  **This is the term that replaces the
///   quadratic**, and its shape is the point: `Σ |eligible_owners|`, never
///   `rows x contacts`.
/// - `contact_count * (log + 6)` — `order.sort_unstable_by_key`'s comparison
///   bound (`log + 2`, the term this line already carried) plus the
///   transpose's four contact-sized passes: the two owner-run walks, the
///   `starts` prefix sum, and the cursor copy.
/// - `eligible_owner_count * (owner_log * 2)` and
///   `face_count * (face_log * 2 + 1)` — unchanged, and unrelated to the
///   transpose: the eligibility's own ordering and the row emit.
///
/// What is **not** charged here is the per-cell cost of the fill and of the
/// scan the transpose drives.  That term is exactly `cells`-shaped, `cells` is
/// not a count any caller of this function holds, and it is added by
/// [`load_resource_plan`] from [`LoadTransposeCounts::operations`] — the same
/// split, for the same reason, that phase G1 used for the transpose's *bytes*.
/// The consequence is stated rather than left to be discovered: the preflight
/// [`specialized_load_resource_plan`] under-states this finalizer's operations
/// by the cells term, exactly as it already under-states its scratch bytes.
/// That costs a **later** refusal, never unafforded execution — the fill and
/// the scan run only after [`finalize_load_rows`]'s own exact check, and that
/// check is the one carrying the cells term.
///
/// Measured on the Weeping Treant v1.2, 2026-08-07: the deleted quadratic
/// charged 117,741 x 1,366 x 27 = 4,342,523,562 for the gate batch's scan;
/// what replaces it is 7,988,078 x 13 x 2 = 207,690,028 here plus
/// 8,210,643 x 8 = 65,685,144 in [`load_resource_plan`].  **No cap is raised,
/// lowered or touched by this repricing; a falling demand can create no
/// refusal.**
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
    let operations = eligible_owner_count
        .checked_mul(log.saturating_add(2))
        .and_then(|pass| pass.checked_mul(2))
        .and_then(|work| {
            contact_count
                .checked_mul(log.saturating_add(6))
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
///
/// `transpose_operations` is FU-8f phase **G5**'s new **operation** term, and
/// it is here for exactly the reasons the byte term above is here: it is
/// `cells`-shaped, `cells` is knowable only after
/// [`LoadTransposeCounts::count`] has walked the eligibility, and the check
/// below is the last one before the fill and the scan it prices actually run.
/// See [`LoadTransposeCounts::operations`] for its decomposition and
/// [`load_resource_plan_from_counts`] for the quadratic it retires.
fn load_resource_plan(
    load_eligibility: &[LoadEligibility],
    contacts: &[PhysicalContact],
    transpose_bytes: usize,
    transpose_operations: usize,
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
    plan.operations = plan
        .operations
        .checked_add(transpose_operations)
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
///
/// **Dated addendum 2026-08-07 — FU-8f phase G5.  The paragraph above stands as
/// the history of this retention's charge, and its allowance no longer
/// exists.**  G5 retired the `rows x contacts x (owner_log + 4)` model, so
/// there is no eligibility binary search left for these slot comparisons to
/// hide inside.  They are now charged explicitly and per cell, as the
/// `LOAD_WITNESS_SLOTS` component of [`LoadTransposeCounts::operations`].  The
/// retention still moves **no** measured work — the pass was already visiting
/// the pair — but it is no longer free of charge, and saying so is the point of
/// this addendum.  No cap moved with it.
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
    /// FU-8f phase G6a: the transpose the finalize built and scanned, moved
    /// out instead of dropped, so [`RetainedLoadTranspose::absorb_full_build`]
    /// can retain it across merge-gate rounds.  Filled by
    /// [`finalize_load_rows`] whenever retention is requested at all — the
    /// buffer was already paid for and built, so moving it costs nothing and
    /// keeps the retained payload the build's own rather than a restatement.
    transpose: Option<LoadEligibilityTranspose>,
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
/// **Dated addendum 2026-08-07 — FU-8f phase G5.  The paragraph above is left
/// standing and its last two sentences have come due.**  G5 repriced the model:
/// `rows x contacts x (owner_log + 4)` is gone from
/// [`load_resource_plan_from_counts`], replaced by the transpose's own
/// `Σ |eligible_owners|`-shaped search term there and by
/// [`LoadTransposeCounts::operations`]'s `cells`-shaped term in
/// [`load_resource_plan`].  On the treant's gate batch 4,342,523,562 becomes
/// 207,690,028 + 65,685,144.  The charged operation count therefore **does**
/// move in G5, and it moves **downward at every site**, so no refusal can be
/// created by it.  **No cap is raised, lowered or touched.**  The bytes
/// described above are unchanged, to the byte.
///
/// Note (design record open question 7): the rows are **not** uniformly small.
/// One treant row carries 568 eligible owners of 1,366 contacts.  The transpose
/// handles it identically; nothing here assumes short buckets.
pub(crate) struct LoadEligibilityTranspose {
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

    /// What [`Self::finish`] and [`scan_transposed_load`] will **spend** on the
    /// cells this count has just sized — FU-8f phase G5's new charged operation
    /// term, and the exact replacement for the `rows x contacts` quadratic that
    /// phase deletes from [`load_resource_plan_from_counts`].
    ///
    /// `cells x (LOAD_WITNESS_SLOTS + 5)`, in the elementary unit every other
    /// term of that model counts:
    ///
    /// - **1** per cell for [`Self::finish`]'s row store and cursor bump — the
    ///   only per-cell work the fill does, the rest of it being the per-owner
    ///   searches charged from the counts.
    /// - **4** per cell for the squared distance and the running-argmin
    ///   comparison.  That is precisely the allowance the superseded
    ///   `owner_log + 4` model carried for everything that was *not* the
    ///   eligibility binary search, kept at its own value rather than
    ///   re-derived.
    /// - **[`LOAD_WITNESS_SLOTS`]** per cell for [`insert_load_witness`], which
    ///   FU-8d charged *inside* that binary-search allowance ("at most
    ///   `LOAD_WITNESS_SLOTS` slot comparisons per retained pair, inside the
    ///   `owner_log + 4` per-pair allowance").  The allowance it was hiding in
    ///   no longer exists, so the slots are charged in the open.  They are
    ///   charged unconditionally, including on the accepted-load path that
    ///   retains nothing, because a bound may over-state and must not
    ///   under-state.
    ///
    /// On the Weeping Treant v1.2's merge-gate batch: 8,210,643 x 8 =
    /// 65,685,144, against the 4,342,523,562 the deleted quadratic charged for
    /// the same work.  Single specimen, 2026-08-07 (n = 1, per R4/FU-23).
    fn operations(&self) -> Result<usize, GenericPerceptionError> {
        self.cells
            .checked_mul(LOAD_WITNESS_SLOTS.saturating_add(5))
            .ok_or(GenericPerceptionError::LoadResourceOverflow)
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
///
/// FU-8f phase G6b: the loop body lives in [`scan_transposed_load_core`],
/// shared verbatim with the retained fast path
/// ([`RetainedLoadTranspose::finalize_retained_round`]).  The two callers
/// differ only in where the buckets and the row centroids are stored — the
/// fresh CSR and `load_eligibility` here, the retained arena and the retained
/// `row_centroid_mm` there — and both are the same values bit for bit, so the
/// bit-identity argument is G1's argument carried by shared code rather than
/// by parallel implementations.
fn scan_transposed_load(
    load_eligibility: &[LoadEligibility],
    contacts: &[PhysicalContact],
    transpose: &LoadEligibilityTranspose,
    retain: Option<&mut LoadRowRetention>,
) -> Vec<Option<(usize, f64)>> {
    scan_transposed_load_core(
        load_eligibility.len(),
        |row| load_eligibility[row].centroid_mm,
        |contact| transpose.rows_of(contact),
        contacts,
        retain,
    )
}

/// The one nearest-and-witness loop body, generic over where the buckets and
/// the row centroids live (FU-8f phase G6b).
///
/// `rows_of(contact)` must yield strictly ascending row indices and the
/// contacts are consumed ascending, so the `(contact_index, distance)`
/// sequence every row observes is identical for any two sources that store
/// the same buckets — which is exactly what [`scan_transposed_load`]'s
/// bit-identity claim ([`LoadEligibilityTranspose`]) rests on, and what lets
/// the retained path reuse this body unmodified.
fn scan_transposed_load_core<'buckets>(
    row_count: usize,
    centroid_of: impl Fn(usize) -> [f64; 3],
    rows_of: impl Fn(usize) -> &'buckets [u32],
    contacts: &[PhysicalContact],
    retain: Option<&mut LoadRowRetention>,
) -> Vec<Option<(usize, f64)>> {
    // One flattened owner x face x contact batch.  The matrix is streamed into
    // a caller-budgeted nearest row rather than issuing scalar spatial calls.
    let mut nearest = vec![None::<(usize, f64)>; row_count];
    // FU-8d phase F3 (P2): when the caller asks, the same pass also keeps the
    // `LOAD_WITNESS_SLOTS` smallest pairs per row.  The running argmin below is
    // untouched — the retention is written *beside* it, never through it — so
    // the emitted partition cannot depend on whether anything was retained.
    // `debug_assert`s in the caller pin slot 0 against the argmin.
    let mut retention = retain;
    if let Some(retention) = retention.as_deref_mut() {
        retention.witnesses = vec![[LoadWitness::UNUSED; LOAD_WITNESS_SLOTS]; row_count];
        retention.centroids_mm = (0..row_count).map(&centroid_of).collect();
    }
    for (contact_index, contact) in contacts.iter().enumerate() {
        for &row in rows_of(contact_index) {
            let face_index = row as usize;
            let distance = contact
                .point_mm
                .iter()
                .zip(centroid_of(face_index))
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

/// Why the merge gate may reuse a load transpose across rounds, and the four
/// guards that decide when it must not (FU-8f phase G6a, design record
/// `docs/engineering-journal/fu8f-g6-design-2026-08-07.md`, D-G6-1..D-G6-3).
///
/// # What is retained
///
/// One committed merge renumbers every contact index, every specialized owner
/// id past the insertion point and every CSR offset — but it is content-local:
/// survivor buckets are content-invariant (a commit touches no survivor's
/// routes or demands), the merged seat's bucket is the ascending dedup union
/// of its parents' (exactly [`crate::specialized`]'s `merged_rows` shape), and
/// the row set, row order, row areas and centroids are merge-invariant
/// (FU-8d F3's ∪-argument; G4's constant 117,741-row observation on the
/// treant).  So the transpose is retained by *content* — an append-only arena
/// of bucket payloads plus per-contact `(offset, len)` refs — and one commit
/// costs `O(|elig(l)| + |elig(r)| + contacts)`, not a rebuild.
///
/// # Guards — staleness is refused, never risked
///
/// The G2 verification history (a stale structure meeting a larger mesh
/// produced a real out-of-bounds panic in this file) is the precedent: every
/// reuse is gated, any miss falls back to a **counted** full rebuild
/// (`retained_load_rebuilds` in the census), and a rebuild is always correct.
///
/// 1. **Mesh fingerprint** ([`Self::entry_rebuild_cause`]): retained row
///    indices must never index a different mesh's eligibility.
/// 2. **Contact witness** ([`Self::entry_rebuild_cause`]): the retained
///    `(owner, point_mm)` list must equal the fresh physical list on
///    `to_bits`, which catches every contact mutation not routed through
///    [`Self::record_commit`] — `coalesce_exact_regular_seats` before the
///    terminal audit, attach paths, and future drift nobody wires up.
/// 3. **Merged-resolves-specialized** ([`Self::record_commit`]): the
///    bucket-union identity holds only when the committed seat resolves to a
///    specialized owner; a generic resolution poisons retention instead of
///    silently serving wrong rows.
/// 4. **Cardinality cross-check** ([`Self::cardinality_rebuild_cause`]): the
///    gate's own freshly-derived `contact_rows` must equal every retained
///    bucket's length — an `O(contacts)` content-drift check at zero added
///    derivation cost (F-G6-5).
///
/// # G6a landed the carrier rebuild-always; G6b flipped the fast path in
///
/// Phase G6a landed the guards, counted and mutation-proven, while every
/// round still executed the full build and repopulated this carrier from the
/// build's own transpose ([`Self::absorb_full_build`]) — byte-identical by
/// construction, so the guards were proven before anything depended on them.
/// Phase G6b (2026-08-07) made guard-accepted rounds take
/// [`Self::finalize_retained_round`] instead: the same scan loop body over
/// the retained buckets, the same finalize tail, the same baseline post-pass,
/// with `GroupedOwnerFaces`/`extend_load_eligibility`/[`LoadTransposeCounts`]
/// falling off retained rounds entirely (F-G6-2/F-G6-3).  Guard-miss rounds
/// still take the counted full rebuild.  Compaction of the arena (D-G6-2
/// step 4) landed with the fast path, at the 2x-waste trigger.
///
/// # Ownership
///
/// `consolidate` owns one `Option<RetainedLoadTranspose>` on its stack,
/// threads it as `&mut` through `evaluate_gate_batch` into
/// `build_gate_load_state`, and drops it on return — retention never escapes
/// the tournament, and every non-gate load finalize is untouched.
pub(crate) struct RetainedLoadTranspose {
    /// Bucket payloads, append-only arena.  A committed merge appends the
    /// merged bucket; parents' payloads become dead until compaction (G6b).
    arena: Vec<u32>,
    /// Per contact, in current contact order: `(offset, len)` into `arena`.
    buckets: Vec<(u32, u32)>,
    /// Live cells `= Σ len`, maintained by ± deltas; the charged scan size.
    cells: usize,
    /// Row invariants, retained from the full build, never touched by updates.
    row_face_id: Vec<u32>,
    row_area_mm2: Vec<f64>,
    row_centroid_mm: Vec<[f64; 3]>,
    /// The contact list this state is synchronized to: `(owner, point_mm)` per
    /// contact, in order.  `contact_id` is positional so it is not stored.
    contact_witness: Vec<(LoadOwner, [f64; 3])>,
    /// The digest of the mesh the full build ran under (G2's proof).  The
    /// design record sketched a `u64`; the landed G2 proof digest is a string
    /// and is stored as one rather than re-hashed into a weaker form.
    mesh_fingerprint: String,
    /// `Some` when [`Self::record_commit`] refuses to trust its own remap
    /// (guard 3, or an identity walk that did not see "survivors in preserved
    /// order plus exactly one insertion") — carrying WHICH of the three
    /// [`PoisonCause`] groups fired (FU-8g H4, D6, the rider's split of the
    /// collapsed `Poisoned` label).  A poisoned carrier answers every guard
    /// query with a rebuild cause until the next repopulation.
    poison_cause: Option<PoisonCause>,
    /// Counted guard misses, carried across repopulations — the test-visible
    /// twin of the `retained_load_rebuilds` census counter.
    rebuild_count: usize,
    /// FU-8f phase G6c — the O(|l| + |r| + contacts) remap cost the most
    /// recent [`Self::record_commit`] actually performed against this
    /// carrier's own contact count at the time, `0` when this carrier is
    /// freshly absorbed from a full build (no commit has touched it since).
    /// Read by [`Self::executed_operations`], the retained-round meter
    /// (D-G6-4); never charged on its own, and never fed back into the
    /// round's stated charge, which stays the admissible full-build worst
    /// case regardless (see [`Self::executed_operations`]'s doc comment).
    last_update_operations: usize,
}

/// FU-8g H4 (D6, F11) — which of the three genuinely distinct bugs poisoned
/// the carrier.  `RetainedLoadRebuildCause::Poisoned` used to collapse all
/// nine `record_commit` refusal sites into one census label; the treant's 21
/// poison rebuilds (`docs/engineering-journal/support-resource-caps-2026-08-05.md`
/// §"Retention, measured on a whole tournament, with the pre-G6 round as an
/// internal control") were counted but never attributed.  This is the split,
/// one variant per group:
///
/// - [`Self::Guard3Generic`] — the merged seat resolved GENERIC, so the
///   bucket-union identity (F-G6-1) does not hold.  A silent-wrongness guard;
///   the design predicted this would never fire on real geometry (F11), so a
///   nonzero count here is a NEW finding, not a confirmation.
/// - [`Self::CommitShape`] — the caller handed `record_commit` something that
///   is not a tournament merge, or an internal bookkeeping invariant the
///   merge itself depends on did not hold: out-of-range/equal `left`/`right`,
///   a committed list of the wrong length, a parent that is not
///   `LoadOwner::Specialized`, the `first_specialized_owner` recovery
///   underflowing, or (H4 addition, see the doc comment on the `poison`
///   call site) the merged arena offset not fitting `u32`.
/// - [`Self::IdentityWalk`] — the identity walk could not name the remap:
///   a position-bit tie at the insertion point (G6b), the inserted seat not
///   matching the committed position, a second divergence, an unmatched
///   survivor, or no insertion ever recorded.
///
/// # Deviation from F11's inventory, recorded rather than absorbed
///
/// F11 (`fu8g-design-2026-08-08.md`) counted **nine** `poison()` call sites
/// in `record_commit` and bounded its own search to `:3378–3480`. Source
/// review at H4 found a **tenth**, one line past that bound: the
/// `u32::try_from(merged_offset).is_err()` guard, which protects the arena
/// offset arithmetic from overflowing `u32` once the identity walk has
/// already succeeded. F11 did not classify it. It is unreachable on any
/// mesh this engine has ever run (the arena would need to exceed
/// `u32::MAX` live+dead `u32` cells) and is defensive rather than
/// caller-shape or position-matching in character — the same character as
/// the `first_specialized_owner` underflow check F11 itself placed in
/// **commit shape**. Classified there by that precedent, not left
/// unclassified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PoisonCause {
    Guard3Generic,
    CommitShape,
    IdentityWalk,
}

impl PoisonCause {
    /// The census label for this cause (FU-8g H4, D6).
    #[cfg(feature = "census")]
    const fn label(self) -> &'static str {
        match self {
            Self::Guard3Generic => "poisoned_guard3",
            Self::CommitShape => "poisoned_commit_shape",
            Self::IdentityWalk => "poisoned_identity_walk",
        }
    }
}

/// Why one gate round could not (or, in G6a, additionally did not need to)
/// reuse the retained transpose.  Every variant maps to one counted rebuild.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetainedLoadRebuildCause {
    /// [`RetainedLoadTranspose::record_commit`] invalidated the carrier:
    /// guard 3 fired, or the identity walk diverged from the one-insertion
    /// shape it requires.  FU-8g H4: carries WHICH of the three
    /// [`PoisonCause`] groups fired, instead of collapsing them.
    Poisoned(PoisonCause),
    /// Guard 1: the retained transpose met a different mesh.
    MeshFingerprint,
    /// Guard 2: the fresh physical list is not the witnessed one.
    ContactWitness,
    /// Guard 4: some retained bucket's length disagrees with the freshly
    /// derived `contact_rows`.
    BucketCardinality,
}

impl RetainedLoadRebuildCause {
    /// The census label for this cause.
    #[cfg(feature = "census")]
    const fn label(self) -> &'static str {
        match self {
            Self::Poisoned(cause) => cause.label(),
            Self::MeshFingerprint => "mesh_fingerprint",
            Self::ContactWitness => "contact_witness",
            Self::BucketCardinality => "bucket_cardinality",
        }
    }
}

/// `point_mm` compared the way guard 2 states it: on bits, never on `==`,
/// because bit-identity is the claim and `NaN`/`-0.0` must not slip a compare.
fn position_bits(position_mm: [f64; 3]) -> [u64; 3] {
    position_mm.map(f64::to_bits)
}

impl RetainedLoadTranspose {
    /// Counted guard misses so far — the test-visible twin of the
    /// `retained_load_rebuilds` census counter, which is the production
    /// observable.
    #[cfg(test)]
    pub(crate) const fn rebuild_count(&self) -> usize {
        self.rebuild_count
    }

    /// Guards 1 and 2 (and the poison latch), in the design's order, at gate
    /// entry.  `None` means the retained state may be reused as far as these
    /// guards can see; guard 4 runs separately once `contact_rows` exists.
    pub(crate) fn entry_rebuild_cause(
        &self,
        verified_mesh: &VerifiedMeshFingerprint<'_>,
        contacts: &[PhysicalContact],
    ) -> Option<RetainedLoadRebuildCause> {
        if let Some(cause) = self.poison_cause {
            return Some(RetainedLoadRebuildCause::Poisoned(cause));
        }
        if self.mesh_fingerprint != verified_mesh.fingerprint() {
            return Some(RetainedLoadRebuildCause::MeshFingerprint);
        }
        if self.contact_witness.len() != contacts.len() {
            return Some(RetainedLoadRebuildCause::ContactWitness);
        }
        let witnessed =
            self.contact_witness
                .iter()
                .zip(contacts)
                .all(|(&(owner, point_mm), contact)| {
                    owner == contact.owner
                        && position_bits(point_mm) == position_bits(contact.point_mm)
                });
        if !witnessed {
            return Some(RetainedLoadRebuildCause::ContactWitness);
        }
        None
    }

    /// Guard 4: every **specialized** contact's retained bucket length against
    /// the gate's own freshly-derived `contact_rows`.
    ///
    /// # Deviation from the design record's F-G6-5, and why
    ///
    /// F-G6-5 states that `build_gate_load_state`'s `contact_rows[c]` is
    /// "each contact's eligible-row count", making the comparison total.  It
    /// is not, and this was found at implementation by giving the guard's
    /// positive control an independent derivation
    /// (`fu8f_retained_transpose_rebuilds_when_a_bucket_length_drifts`).
    /// `contact_rows[c]` is derived from contact `c`'s own
    /// `(owner, face)` run in `specialized_owner_faces`, and
    /// `append_variant_load_inputs` emits such pairs **only** on the branch
    /// that resolves a contact specialized — a generic seat contributes an
    /// empty run.  Meanwhile the transpose's bucket for a generic seat is not
    /// empty at all: eligibility is a predicate on the `LoadOwner`, and a
    /// generic island owner is eligible for the generic authority's rows.  So
    /// the total comparison would report `BucketCardinality` on **every**
    /// round of **every** batch that carries a generic seat — which is every
    /// real gate batch, since the finalizer refuses with
    /// `MissingPhysicalContact` unless the generic authority's rows have
    /// holders.  Taken literally, F-G6-5 would have made the rebuild counter
    /// fire always and destroyed the very measurement phase G6a exists to
    /// land.
    ///
    /// Restricting the comparison to specialized contacts keeps every part of
    /// F-G6-5 that is true — it is still `O(contacts)`, still reads a
    /// `contact_rows` the round computes for `proposal_load_bound` regardless,
    /// and so still costs **zero added derivation** — and it still covers the
    /// contacts the update actually acts on: the merged seat and every
    /// specialized survivor whose id is renumbered.  Restoring totality would
    /// mean deriving each generic island's eligible-row count fresh, an
    /// `O(rows x owners)` walk over the row-major eligibility — precisely the
    /// walk G6 exists to delete — so it is declined on the design's own terms.
    /// Generic seats' buckets are covered instead by guard 2 (their owner and
    /// position are witnessed every round), by the row-set invariance argument
    /// (F-G6-1: a commit touches no survivor's routes or demands, and a
    /// generic island's row set is not a function of the contact list at all),
    /// and by phase G6b's field-by-field equivalence harness, which compares
    /// bucket **content** and does not depend on this guard.
    ///
    /// Owners are read from the **fresh** contact list rather than from the
    /// retained witness, so this guard does not inherit guard 2's verdict even
    /// though `build_gate_load_state` happens to run it only after guard 2 has
    /// passed.
    pub(crate) fn cardinality_rebuild_cause(
        &self,
        contacts: &[PhysicalContact],
        contact_rows: &[usize],
    ) -> Option<RetainedLoadRebuildCause> {
        if let Some(cause) = self.poison_cause {
            return Some(RetainedLoadRebuildCause::Poisoned(cause));
        }
        if self.buckets.len() != contact_rows.len() || contacts.len() != contact_rows.len() {
            return Some(RetainedLoadRebuildCause::BucketCardinality);
        }
        let agrees = self.buckets.iter().zip(contact_rows).zip(contacts).all(
            |((&(_, len), &rows), contact)| {
                !matches!(contact.owner, LoadOwner::Specialized(_)) || len as usize == rows
            },
        );
        if !agrees {
            return Some(RetainedLoadRebuildCause::BucketCardinality);
        }
        None
    }

    /// Invalidate the carrier and release its payload.  The next gate round
    /// takes a counted full rebuild ([`RetainedLoadRebuildCause::Poisoned`]),
    /// attributed to `cause` (FU-8g H4, D6).
    fn poison(&mut self, cause: PoisonCause) {
        self.poison_cause = Some(cause);
        self.arena = Vec::new();
        self.buckets = Vec::new();
        self.cells = 0;
        self.row_face_id = Vec::new();
        self.row_area_mm2 = Vec::new();
        self.row_centroid_mm = Vec::new();
        self.contact_witness = Vec::new();
        self.last_update_operations = 0;
    }

    /// Synchronize the carrier with one committed merge (D-G6-2), called by
    /// `consolidate` immediately after the committed contact list is in place
    /// and before the next round's gather.
    ///
    /// - The insertion point `p` is derived by an **identity walk** over
    ///   positions on bits — old witness minus `{left, right}` against the new
    ///   list — never by comparator re-derivation.  Anything other than
    ///   "survivors in preserved relative order plus exactly one insertion"
    ///   poisons the carrier instead of guessing.
    /// - The merged bucket is the linear ascending dedup merge of the parents'
    ///   retained buckets, recomputed here rather than captured from the G3
    ///   variant loop's `merged_rows` (design open question 2: decoupling the
    ///   retention from the acceptance fold's internals is worth the
    ///   microseconds, and gives the equivalence tests one seam).
    /// - Witness owners are recomputed positionally: a surviving specialized
    ///   owner at new index `i` is `first_specialized_owner + i` by
    ///   `append_variant_load_inputs`'s own construction, generic owners are
    ///   position-independent, and the merged seat — guard 3 requires it
    ///   specialized — takes `first_specialized_owner + p`.
    ///
    /// `merged_resolves_specialized` is guard 3's verdict, computed by the
    /// caller by re-running the `generic_owner_for_parts` predicate on the
    /// **committed** contact (never trusted from the proposal).
    pub(crate) fn record_commit(
        &mut self,
        left: usize,
        right: usize,
        positions_after: impl ExactSizeIterator<Item = [f64; 3]>,
        committed_position_mm: [f64; 3],
        merged_resolves_specialized: bool,
    ) {
        if self.poison_cause.is_some() {
            return;
        }
        // Guard 3.  A generic-resolving merged seat's bucket is NOT the union
        // of its parents' buckets, so the remap below would be silently wrong.
        if !merged_resolves_specialized {
            self.poison(PoisonCause::Guard3Generic);
            return;
        }
        let old_len = self.contact_witness.len();
        if left >= old_len || right >= old_len || left == right {
            self.poison(PoisonCause::CommitShape);
            return;
        }
        if positions_after.len() + 1 != old_len {
            self.poison(PoisonCause::CommitShape);
            return;
        }
        // The bucket-union identity additionally needs both parents
        // specialized (F-G6-1); a committed tournament merge always is, and a
        // caller outside that construction is refused rather than mis-mapped.
        let (LoadOwner::Specialized(left_owner), LoadOwner::Specialized(_)) =
            (self.contact_witness[left].0, self.contact_witness[right].0)
        else {
            self.poison(PoisonCause::CommitShape);
            return;
        };
        // `owner = first_specialized_owner + index` by construction, so the
        // base recovers from any specialized survivor — the left parent here.
        let Some(first_specialized_owner) = left_owner.checked_sub(left as u32) else {
            self.poison(PoisonCause::CommitShape);
            return;
        };
        // Identity walk: match survivors in order; the single divergence is
        // the merged seat's insertion point.  The new list is one shorter than
        // the old and carries one non-survivor, so a walk that never diverges
        // is impossible: either `insertion` is set exactly once below, or the
        // carrier is poisoned.
        let mut survivor_indices = (0..old_len).filter(|&index| index != left && index != right);
        let mut survivor = survivor_indices.next();
        let mut insertion: Option<usize> = None;
        for (new_index, position_mm) in positions_after.enumerate() {
            let bits = position_bits(position_mm);
            match survivor {
                Some(old_index) if position_bits(self.contact_witness[old_index].1) == bits => {
                    // FU-8f phase G6b, closing the hazard the G6a
                    // re-verification recorded (CURRENT-STATUS 2026-08-07,
                    // "the identity walk's position-key hazard"): positions
                    // are only the FIRST key of `compare_contacts`, so a
                    // committed seat may legitimately carry bits identical to
                    // the survivor this slot would consume.  From position
                    // bits alone the two readings — "this slot is the
                    // survivor" and "this slot is the committed seat" — are
                    // then indistinguishable, and the tie was really decided
                    // by comparator tie-breakers this walk deliberately does
                    // not re-derive.  Guessing wrong used to derive `p + 1`
                    // silently; in G6a only the guard chain consumed that,
                    // but G6b's fast path would serve wrong buckets.  So the
                    // ambiguity is refused: one counted rebuild instead of a
                    // guess, in a case no corpus commit has ever produced
                    // (all 21 `make_raking_blade` remaps were exact).
                    if insertion.is_none() && bits == position_bits(committed_position_mm) {
                        self.poison(PoisonCause::IdentityWalk);
                        return;
                    }
                    survivor = survivor_indices.next();
                }
                _ if insertion.is_none() => {
                    if bits != position_bits(committed_position_mm) {
                        self.poison(PoisonCause::IdentityWalk);
                        return;
                    }
                    insertion = Some(new_index);
                }
                _ => {
                    // A second divergence: the new list is not "survivors in
                    // order plus one insertion".
                    self.poison(PoisonCause::IdentityWalk);
                    return;
                }
            }
        }
        if survivor.is_some() {
            // A survivor was never matched, so the walk cannot name the
            // remap it would need.
            self.poison(PoisonCause::IdentityWalk);
            return;
        }
        let Some(insert_at) = insertion else {
            self.poison(PoisonCause::IdentityWalk);
            return;
        };
        // Merged bucket: linear ascending dedup merge of the parents' buckets,
        // appended to the arena.  Indexed reads, so the arena can grow while
        // its earlier payloads are read.
        let (left_offset, left_len) = self.buckets[left];
        let (right_offset, right_len) = self.buckets[right];
        let merged_offset = self.arena.len();
        // FU-8g H4 deviation from F11's inventory (see the doc comment on
        // `PoisonCause::CommitShape`): a tenth `poison()` site F11 did not
        // enumerate, classified `CommitShape` by the same precedent as the
        // `first_specialized_owner` underflow check above.
        if u32::try_from(merged_offset).is_err() {
            self.poison(PoisonCause::CommitShape);
            return;
        }
        let (mut cursor_l, end_l) = (left_offset as usize, (left_offset + left_len) as usize);
        let (mut cursor_r, end_r) = (right_offset as usize, (right_offset + right_len) as usize);
        while cursor_l < end_l || cursor_r < end_r {
            let row = if cursor_l < end_l && cursor_r < end_r {
                let (a, b) = (self.arena[cursor_l], self.arena[cursor_r]);
                match a.cmp(&b) {
                    std::cmp::Ordering::Less => {
                        cursor_l += 1;
                        a
                    }
                    std::cmp::Ordering::Greater => {
                        cursor_r += 1;
                        b
                    }
                    std::cmp::Ordering::Equal => {
                        cursor_l += 1;
                        cursor_r += 1;
                        a
                    }
                }
            } else if cursor_l < end_l {
                let a = self.arena[cursor_l];
                cursor_l += 1;
                a
            } else {
                let b = self.arena[cursor_r];
                cursor_r += 1;
                b
            };
            self.arena.push(row);
        }
        let merged_len = self.arena.len() - merged_offset;
        // Remap: remove two, insert one at `insert_at`, order preserved.
        let mut next_buckets = Vec::with_capacity(self.buckets.len() - 1);
        let mut next_witness = Vec::with_capacity(self.contact_witness.len() - 1);
        let merged_bucket = (merged_offset as u32, merged_len as u32);
        for old_index in 0..self.contact_witness.len() {
            if old_index == left || old_index == right {
                continue;
            }
            if next_buckets.len() == insert_at {
                next_buckets.push(merged_bucket);
                next_witness.push((
                    LoadOwner::Specialized(first_specialized_owner + insert_at as u32),
                    committed_position_mm,
                ));
            }
            let new_index = next_buckets.len();
            next_buckets.push(self.buckets[old_index]);
            let (owner, point_mm) = self.contact_witness[old_index];
            let owner = match owner {
                LoadOwner::GenericIsland(island) => LoadOwner::GenericIsland(island),
                LoadOwner::Specialized(_) => {
                    LoadOwner::Specialized(first_specialized_owner + new_index as u32)
                }
            };
            next_witness.push((owner, point_mm));
        }
        if next_buckets.len() == insert_at {
            next_buckets.push(merged_bucket);
            next_witness.push((
                LoadOwner::Specialized(first_specialized_owner + insert_at as u32),
                committed_position_mm,
            ));
        }
        self.cells = self.cells + merged_len - left_len as usize - right_len as usize;
        // FU-8f phase G6c (D-G6-4): the O(|l| + |r| + contacts) remap this
        // call just performed, against the POST-merge contact count —
        // `next_witness.len()`, one shorter than `old_len` — because that is
        // the bookkeeping pass size the loop above actually ran (one entry
        // written per surviving contact plus the merged seat).  Read by
        // [`Self::executed_operations`] on the round that follows this
        // commit; overwritten by the next commit or zeroed by the next full
        // absorption, never accumulated.
        self.last_update_operations = (left_len as usize)
            .saturating_add(right_len as usize)
            .saturating_add(next_witness.len());
        self.buckets = next_buckets;
        self.contact_witness = next_witness;
        // FU-8f phase G6b (design D-G6-2 step 4): compact the arena at 2x
        // waste — one O(cells) copy in bucket order, amortized over
        // O(cells / avg merged bucket) commits, which is what bounds held
        // bytes at `2 x cells x 4 B + (contacts + 1) x 8 B` (the D-G6-4
        // ceiling; charged by phase G6c).  Bucket order is preserved exactly,
        // so the scan's visit sequence — and therefore bit-identity — cannot
        // be affected, and `fu8f_retained_arena_compacts_at_twice_the_live_cells`
        // pins both the trigger and the preservation.
        if self.arena.len() > self.cells.saturating_mul(2) {
            let mut arena = Vec::with_capacity(self.cells);
            for bucket in &mut self.buckets {
                let (offset, len) = *bucket;
                // `cells` fits `u32` (the absorbed CSR's own bound), and every
                // compacted offset is below `cells`.
                let start = arena.len() as u32;
                arena.extend_from_slice(
                    &self.arena[offset as usize..offset as usize + len as usize],
                );
                *bucket = (start, len);
            }
            debug_assert_eq!(arena.len(), self.cells);
            self.arena = arena;
        }
    }

    /// Contact `contact`'s retained ascending eligible rows — the retained
    /// twin of [`LoadEligibilityTranspose::rows_of`].
    fn retained_rows_of(&self, contact: usize) -> &[u32] {
        let (offset, len) = self.buckets[contact];
        &self.arena[offset as usize..offset as usize + len as usize]
    }

    /// The retained partition rows' face ids, in row order — what the gate
    /// derives `face_row`/`row_face` from on a retained round instead of a
    /// fresh partition (identical by row-set invariance, F-G6-1; retained
    /// verbatim from the build and untouched by updates, pinned by
    /// `fu8f_retained_row_invariants_are_the_builds_own_and_survive_a_commit`).
    pub(crate) fn row_face_ids(&self) -> &[u32] {
        &self.row_face_id
    }

    /// FU-8f phase G6c — the executed retained-path operation meter
    /// (D-G6-4), read by [`crate::specialized::build_gate_load_state`]
    /// beside — never instead of — the round's stated charge.
    ///
    /// **Charging stays untouched.** Every round is still charged the full
    /// build's own figure, `SpecializedLoadBaseline::build`'s
    /// `specialized_load_resource_plan` operations count, because no plan
    /// can know at plan time whether a guard will miss (D-G6-4, the G3/FU-8d
    /// severance shape carried over). This is the OTHER number: what a
    /// retained round actually spent, so the two can be compared.
    ///
    /// Priced from the same per-cell allowance G5 gave the full build's own
    /// fill/scan pass — `LOAD_WITNESS_SLOTS + 5` per cell, see
    /// [`LoadTransposeCounts::operations`] — applied to the retained `cells`
    /// [`Self::finalize_retained_round`] actually scans (never the two
    /// `LoadTransposeCounts::count`/`::finish` eligibility traversals the
    /// full build pays twice, F-G6-2: a retained round never runs them, so
    /// they are not in this figure at all), plus the O(rows) row fill and
    /// the O(contacts) contact-hash pass [`Self::finalize_retained_round`]
    /// also runs, plus [`Self::last_update_operations`] — the most recent
    /// [`Self::record_commit`]'s own O(|l| + |r| + contacts) remap, `0` on a
    /// round that reused a build with no intervening commit (the design's
    /// "even the no-merge case saves one full build" lifecycle case).
    pub(crate) fn executed_operations(
        &self,
        contact_count: usize,
    ) -> Result<usize, GenericPerceptionError> {
        self.cells
            .checked_mul(LOAD_WITNESS_SLOTS + 5)
            .and_then(|scan| scan.checked_add(self.row_face_id.len()))
            .and_then(|work| work.checked_add(contact_count))
            .and_then(|work| work.checked_add(self.last_update_operations))
            .ok_or(GenericPerceptionError::LoadResourceOverflow)
    }

    /// FU-8f phase G6c — what the carrier holds across rounds, charged as
    /// part of `gate_load_state_bytes` (D-G6-4).
    ///
    /// Retention EXTENDS the lifetime of state phase G1 already charged per
    /// round (the 32,848,040 B `LoadEligibilityTranspose` scratch term,
    /// charged inside [`load_resource_plan`] every time
    /// `SpecializedLoadBaseline::build` runs): on a retained round that
    /// build never runs at all, so this term REPLACES G1's rather than
    /// joining it, exactly once, here.
    ///
    /// The arena term is charged at [`Self::record_commit`]'s own
    /// 2×-compaction CEILING (`2 × cells × 4 B`), not the arena's live
    /// length, because compaction runs only once waste crosses that bound —
    /// charging the live size would under-state the peak the arena is
    /// actually allowed to reach between compactions. `buckets` and
    /// `contact_witness` are one entry per contact and are charged at their
    /// real (checked) sizes, not the design record's `(contacts + 1)`
    /// CSR-splice approximation (D-G6-2's recorded fallback shape, which
    /// this carrier is not — it is the arena, so its own real layout is
    /// charged instead of an estimate for a structure it does not use).
    pub(crate) fn retained_bytes(&self) -> Result<usize, GenericPerceptionError> {
        let contacts = self.buckets.len();
        let arena_ceiling_bytes = self
            .cells
            .checked_mul(2)
            .and_then(|doubled| doubled.checked_mul(std::mem::size_of::<u32>()))
            .ok_or(GenericPerceptionError::LoadResourceOverflow)?;
        let bucket_bytes = contacts
            .checked_mul(std::mem::size_of::<(u32, u32)>())
            .ok_or(GenericPerceptionError::LoadResourceOverflow)?;
        let row_bytes = self
            .row_face_id
            .len()
            .checked_mul(
                std::mem::size_of::<u32>()
                    + std::mem::size_of::<f64>()
                    + std::mem::size_of::<[f64; 3]>(),
            )
            .ok_or(GenericPerceptionError::LoadResourceOverflow)?;
        let witness_bytes = contacts
            .checked_mul(std::mem::size_of::<(LoadOwner, [f64; 3])>())
            .ok_or(GenericPerceptionError::LoadResourceOverflow)?;
        arena_ceiling_bytes
            .checked_add(bucket_bytes)
            .and_then(|total| total.checked_add(row_bytes))
            .and_then(|total| total.checked_add(witness_bytes))
            .ok_or(GenericPerceptionError::LoadResourceOverflow)
    }

    /// FU-8f phase G6b — the retained fast path: produce the round's finalized
    /// partition and gate baseline from the retained transpose, without
    /// `GroupedOwnerFaces`, `extend_load_eligibility` or
    /// [`LoadTransposeCounts`] (design D-G6-1 step 3; findings F-G6-2/F-G6-3).
    ///
    /// What runs is exactly the full build's own tail, on retained inputs:
    ///
    /// - the [`finalize_load_rows`] prologue verbatim — the
    ///   `plan.authority.mesh` staleness comparison, the finite-contact check
    ///   and the duplicate-contact-id refusal;
    /// - [`scan_transposed_load_core`] — the one shared loop body — over the
    ///   retained buckets (ascending, contacts ascending: the identical visit
    ///   sequence, so bit-identity of `nearest` and every witness slot is by
    ///   G1's construction, not by a parallel implementation);
    /// - the row fill from the retained `row_face_id`/`row_area_mm2` (the
    ///   build's own values bit for bit) and the contact FNV hash, verbatim;
    /// - [`SpecializedLoadBaseline::assemble`] — the same post-pass the full
    ///   build runs, so carried areas are **re-summed fresh in the fold's own
    ///   order every round, never adjusted incrementally**.
    ///
    /// # What is deliberately NOT run here, and why that is sound
    ///
    /// The full build's resource statements — [`LoadTransposeCounts::count`],
    /// [`load_resource_plan`], [`specialized_load_resource_plan`] and their
    /// scratch/operation refusals — price work this round does not perform:
    /// the grouping, the extension and the transpose count/fill are exactly
    /// what retention removes (F-G6-2/F-G6-3).  The work this round *does*
    /// perform — the scan over `cells` and the O(rows + contacts) tail — is a
    /// strict subset of what the tournament's first build (or the last
    /// guard-miss rebuild) stated, had checked against the same untouched
    /// limits, and was admitted for: `cells`, rows and contacts only shrink
    /// under committed merges (rows are merge-invariant, contacts fall by one,
    /// the merged bucket is never larger than its parents together).  A plan
    /// that admits the general path admits its specialisation a fortiori —
    /// the FU-8e Q5 shape.  Every guard-miss round re-enters the full path
    /// with every check intact, and the gate-level charge is untouched in
    /// this phase: `build_gate_load_state` still charges the round the full
    /// build's own figures (D-G6-4's "charge the admissible worst case");
    /// phase G6c lands the retained-round metering beside it.  **No cap,
    /// floor or coefficient moves here.**
    ///
    /// The caller (`build_gate_load_state`) has already run the guard chain;
    /// this method requires it and asserts the one precondition it can check
    /// cheaply.
    pub(crate) fn finalize_retained_round(
        &self,
        plan: &GenericContactPlan,
        verified_mesh: &VerifiedMeshFingerprint<'_>,
        contacts: &[PhysicalContact],
    ) -> Result<(FinalizedLoadPartition, SpecializedLoadBaseline), GenericPerceptionError> {
        debug_assert!(
            self.poison_cause.is_none(),
            "the guard chain admits no poisoned carrier"
        );
        debug_assert_eq!(
            self.buckets.len(),
            contacts.len(),
            "guard 2 has already matched the witness against this contact list",
        );
        // The `finalize_load_rows` prologue, verbatim: the staleness
        // comparison this site has always made (guard 1 matched the retained
        // digest to `verified_mesh`; this ties both to the plan's authority),
        // then the finite and duplicate-contact refusals.
        if verified_mesh.fingerprint() != plan.authority.mesh {
            return Err(GenericPerceptionError::StaleAuthority);
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
        let mut retention = LoadRowRetention {
            witnesses: Vec::new(),
            centroids_mm: Vec::new(),
            transpose: None,
        };
        let nearest = scan_transposed_load_core(
            self.row_face_id.len(),
            |row| self.row_centroid_mm[row],
            |contact| self.retained_rows_of(contact),
            contacts,
            Some(&mut retention),
        );
        debug_assert_eq!(retention.witnesses.len(), self.row_face_id.len());
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
        let mut rows = Vec::with_capacity(self.row_face_id.len());
        // Task #49: the same disclosure the full build makes, over the same
        // retained quantities, so a retained round and a rebuilt one cannot
        // disagree about what carries a face.
        let mut ownerless = Vec::new();
        for (row_index, &face_id) in self.row_face_id.iter().enumerate() {
            let Some((contact_index, _)) = nearest[row_index] else {
                ownerless.push(OwnerlessLoadFace {
                    face_id,
                    area_mm2: self.row_area_mm2[row_index],
                });
                continue;
            };
            let contact = &contacts[contact_index];
            rows.push(FinalizedFaceLoad {
                face_id,
                contact_id: contact.contact_id,
                owner: contact.owner,
                // The build's own `eligibility.area_mm2`, retained bit for bit.
                carried_area_mm2: self.row_area_mm2[row_index],
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
        let partition = FinalizedLoadPartition {
            authority: plan.authority.clone(),
            contacts_fingerprint: contact_hash.finish(),
            rows,
            ownerless,
        };
        let baseline = SpecializedLoadBaseline::assemble(
            &partition,
            contacts,
            retention.witnesses,
            retention.centroids_mm,
        )?;
        Ok((partition, baseline))
    }

    /// (Re)populate the carrier from a full build's own transpose and
    /// partition, counting `cause` when it was a guard miss.
    ///
    /// `cause == None` with an existing carrier is a round whose guards all
    /// passed — in G6a that round still ran the build (rebuild-always) and
    /// reached here; from G6b on such a round takes
    /// [`Self::finalize_retained_round`] instead and never calls this.
    /// Nothing is counted for it either way, because the counter's meaning is
    /// *guard misses*, not builds.  A `None` slot is the first build of a
    /// tournament: also not a miss.
    pub(crate) fn absorb_full_build(
        slot: &mut Option<Self>,
        cause: Option<RetainedLoadRebuildCause>,
        verified_mesh: &VerifiedMeshFingerprint<'_>,
        transpose: LoadEligibilityTranspose,
        partition: &FinalizedLoadPartition,
        baseline: &SpecializedLoadBaseline,
        contacts: &[PhysicalContact],
    ) {
        let miss = slot.is_some() && cause.is_some();
        let rebuild_count =
            slot.as_ref().map_or(0, |state| state.rebuild_count) + usize::from(miss);
        crate::census_record!(crate::census::record_retained_load_build(
            cause.filter(|_| miss).map(RetainedLoadRebuildCause::label)
        ));
        let LoadEligibilityTranspose { rows, starts } = transpose;
        let buckets = starts
            .windows(2)
            .map(|bounds| (bounds[0], bounds[1] - bounds[0]))
            .collect::<Vec<_>>();
        debug_assert_eq!(buckets.len(), contacts.len());
        // Task #49. This equality holds only because the merge gate refuses a
        // partition with any ownerless row before absorbing it: `rows()` is one
        // row per OWNED face while `row_centroid_mm` is one per eligibility
        // row, and those two counts diverge by exactly the ownerless count. If
        // that refusal is ever lifted — see the note at the gate — the retained
        // transpose would silently drop the ownerless rows instead of failing
        // here, so the two must move together.
        debug_assert_eq!(partition.rows().len(), baseline.row_centroid_mm.len());
        let cells = rows.len();
        *slot = Some(Self {
            arena: rows,
            buckets,
            cells,
            row_face_id: partition.rows().iter().map(|row| row.face_id).collect(),
            row_area_mm2: baseline.row_area_mm2.clone(),
            row_centroid_mm: baseline.row_centroid_mm.clone(),
            contact_witness: contacts
                .iter()
                .map(|contact| (contact.owner, contact.point_mm))
                .collect(),
            mesh_fingerprint: verified_mesh.fingerprint().to_owned(),
            poison_cause: None,
            rebuild_count,
            // FU-8f phase G6c: a freshly absorbed carrier has had no commit
            // applied to IT yet — the build that produced it already paid
            // (and was charged) its own full cost, and that is not an
            // "update" in [`Self::record_commit`]'s sense.
            last_update_operations: 0,
        });
    }

    /// Contact `contact`'s retained ascending eligible rows.
    #[cfg(test)]
    pub(crate) fn bucket_rows_for_test(&self, contact: usize) -> &[u32] {
        self.retained_rows_of(contact)
    }

    /// A carrier synchronized to `contacts` with caller-supplied buckets and
    /// EMPTY row invariants — for the committed-renumbering arm in
    /// `specialized.rs` only, which pins [`Self::record_commit`]'s
    /// identity-walk `p` and witness remap against the actual
    /// `append_variant_load_inputs` output and never scans.  Every content
    /// claim about buckets and row invariants is pinned elsewhere against
    /// [`Self::absorb_full_build`]'s real payload, never against this
    /// restatement.
    #[cfg(test)]
    pub(crate) fn from_witness_for_test(
        contacts: &[PhysicalContact],
        buckets: Vec<Vec<u32>>,
    ) -> Self {
        assert_eq!(buckets.len(), contacts.len());
        let mut arena = Vec::new();
        let refs = buckets
            .iter()
            .map(|bucket| {
                let offset = arena.len() as u32;
                arena.extend_from_slice(bucket);
                (offset, bucket.len() as u32)
            })
            .collect::<Vec<_>>();
        let cells = arena.len();
        Self {
            arena,
            buckets: refs,
            cells,
            row_face_id: Vec::new(),
            row_area_mm2: Vec::new(),
            row_centroid_mm: Vec::new(),
            contact_witness: contacts
                .iter()
                .map(|contact| (contact.owner, contact.point_mm))
                .collect(),
            mesh_fingerprint: String::new(),
            poison_cause: None,
            rebuild_count: 0,
            last_update_operations: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn bucket_count_for_test(&self) -> usize {
        self.buckets.len()
    }

    #[cfg(test)]
    pub(crate) const fn cells_for_test(&self) -> usize {
        self.cells
    }

    /// Live plus dead payload — what the compaction trigger compares against
    /// `2 x cells`.
    #[cfg(test)]
    pub(crate) fn arena_len_for_test(&self) -> usize {
        self.arena.len()
    }

    #[cfg(test)]
    pub(crate) fn witness_for_test(&self) -> &[(LoadOwner, [f64; 3])] {
        &self.contact_witness
    }

    #[cfg(test)]
    pub(crate) const fn is_poisoned_for_test(&self) -> bool {
        self.poison_cause.is_some()
    }

    /// FU-8g H4 — which [`PoisonCause`] group fired, if any.  The
    /// mutation-proven twin of [`Self::is_poisoned_for_test`]: that method
    /// only proves poisoning happened, this one proves the split attributes
    /// it correctly.
    #[cfg(test)]
    pub(crate) const fn poison_cause_for_test(&self) -> Option<PoisonCause> {
        self.poison_cause
    }

    /// The row invariants a retained round will read instead of rebuilding
    /// them: `(face_id, area_mm2, centroid_mm)` per row.
    #[cfg(test)]
    pub(crate) fn row_invariants_for_test(&self) -> (&[u32], &[f64], &[[f64; 3]]) {
        (&self.row_face_id, &self.row_area_mm2, &self.row_centroid_mm)
    }

    /// Deliberate corruption hook for the guard-4 mutation proof: shrink one
    /// bucket's recorded length without touching anything else.
    #[cfg(test)]
    pub(crate) fn corrupt_bucket_len_for_test(&mut self, contact: usize) {
        let (offset, len) = self.buckets[contact];
        assert!(len > 0, "corrupting an empty bucket proves nothing");
        self.buckets[contact] = (offset, len - 1);
        self.cells -= 1;
    }
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
        transpose.operations()?,
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
    // TASK #68 CONTRACT (2), AND EXACTLY WHERE IT STOPS.
    //
    // The charge below runs through the one carriable-partition rule,
    // [`charge_face_from_takers`]. The maximum handed to it HERE is
    // deliberately unbounded, which makes [`carriable_share_count`] answer
    // `Some(1)` for every face and this call charge one carrier per face —
    // the predecessor's behaviour, bit for bit, on every model.
    //
    // It is not unbounded because the area law stops applying. It is
    // unbounded because of what consumes THIS partition. The merge
    // tournament's retained load delta ([`SpecializedLoadBaseline::assemble`])
    // indexes `row_centroid_mm`, `witnesses` and `row_area_mm2` BY ELIGIBILITY
    // ROW and carries one `winner` per row, with only `LOAD_WITNESS_SLOTS` = 3
    // retained neighbours to re-derive a winner from. `assemble`'s own comment
    // already states that a partition with FEWER rows than eligibility rows
    // shears that alignment — which is why an ownerless face is refused there
    // rather than disclosed. A face carried by `k` contacts is the same shear
    // in the other direction, and `k` reaches 15 on a 546 mm² face. Splitting
    // here without rebuilding that machine does not produce a stricter
    // partition; it produces a delta that disagrees with the finalize it is
    // checked against.
    //
    // The seam this task's probe measured — the trial append — has no such
    // reader: [`append_to_specialized_load_partition`] passes the real
    // registered maximum, so an appended contact is never handed a block it
    // cannot legally carry. Making the tournament's representation
    // multi-carrier is the escalation, recorded on task #68.
    let max_carriable_mm2 = CarriableMaximum::SingleCarrier;
    let contact_order = validated_contact_order(contacts)?;
    let mut rows = Vec::with_capacity(load_eligibility.len());
    let transpose = transpose.finish(load_eligibility, contacts);
    let mut retention = retain;
    let nearest = scan_transposed_load(
        load_eligibility,
        contacts,
        &transpose,
        retention.as_deref_mut(),
    );
    if let Some(retention) = retention.as_deref_mut() {
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
    // SPEC-A3-A4 §6, task #49. A row with no eligible contact is DISCLOSED,
    // not refused: see `OwnerlessLoadFace`. Nothing is fabricated and nothing
    // is hidden — the face gets no row, so no contact is sized as if carrying
    // it, and the face itself is listed with its own area.
    //
    // Task #68 contract (2). A face LARGER than one contact may legally carry
    // is SPLIT here, where the charge is created, or refused with the same
    // disclosure — see [`carriable_share_count`] and [`charge_one_face`].
    let mut ownerless = Vec::new();
    let mut takers = Vec::new();
    for (face_index, eligibility) in load_eligibility.iter().enumerate() {
        charge_one_face(
            eligibility,
            face_index,
            nearest[face_index],
            contacts,
            &transpose,
            max_carriable_mm2,
            &mut takers,
            &mut rows,
            &mut ownerless,
        );
    }
    if let Some(retention) = retention {
        // FU-8f phase G6a: the scan is done with the transpose, so move it to
        // the caller instead of dropping it — see `LoadRowRetention::transpose`.
        retention.transpose = Some(transpose);
    }
    Ok(FinalizedLoadPartition {
        authority: plan.authority.clone(),
        contacts_fingerprint: hash_physical_contacts(contacts, &contact_order),
        rows,
        ownerless,
    })
}

/// The physical contacts' indices ascending by contact id, with the two checks
/// the partition has always made on the way: no non-finite seat, no duplicate
/// id.  Order of checks preserved (finite first, in input order; then the
/// sort; then adjacency), so the error a bad contact set reports is unchanged.
fn validated_contact_order(
    contacts: &[PhysicalContact],
) -> Result<Vec<usize>, GenericPerceptionError> {
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
    Ok(contact_order)
}

/// [`FinalizedLoadPartition::contacts_fingerprint`], over the ascending order
/// [`validated_contact_order`] produced.  One writer, so an incremental append
/// and a full replay of the same contact set fingerprint identically.
fn hash_physical_contacts(contacts: &[PhysicalContact], ascending_order: &[usize]) -> u64 {
    let mut contact_hash = Fnv::new();
    for &index in ascending_order {
        let contact = &contacts[index];
        contact_hash.u64(contact.contact_id);
        hash_owner(&mut contact_hash, contact.owner);
        for value in contact.point_mm {
            contact_hash.f64(value);
        }
    }
    contact_hash.finish()
}

/// Extend a finalized partition with contacts that were APPENDED to the set it
/// was built over — SPEC-A4 task #68 contract (1).
///
/// # What "incremental" means here, exactly
///
/// The predecessor derived a trial child by replaying
/// [`finalize_specialized_load_partition`] over the WHOLE contact set. That
/// replay re-ran the nearest-eligible-contact assignment from scratch, so
/// appending one repair row could move any face to any other contact:
/// measured on the dust fixture, every one of 182 trials moved a 546.0 mm²
/// face onto the very contact that was providing island 4's coverage, that
/// contact then failed routing on clearance, and the 0.6998 mm² the previous
/// commit had bought evaporated — 182 of 182, bit-exact.
///
/// This path does not re-derive anything. **Every prior row is copied**, value
/// for value, contact for contact, so:
///
/// - every pre-existing contact's `carried_area_mm2` is bit-identical, because
///   it is the same addends summed in the same order (new rows only ever name
///   appended contacts, and the merge keeps each contact's own rows in their
///   prior relative order);
/// - the routing cadence is therefore bit-identical too, since
///   `initial::routing_order` sorts by carried area and nothing any surviving
///   contact carries has moved;
/// - and routability follows the cadence and the tip the carried area sizes.
///
/// The appended contacts are charged **only the load nothing carried before**:
/// the faces the prior partition disclosed as ownerless, plus any face the
/// appended contacts' own reservations bring into eligibility for the first
/// time. That is monotone by construction — an append can never reduce a
/// surviving contact's charge, so it can never under-size one — and it is the
/// honest ledger: total charged area only grows, and no face is charged twice.
///
/// The split/refuse rule of contract (2) applies unchanged to what the
/// appended contacts take; [`charge_face_from_takers`] is literally the same
/// function the full replay calls.
///
/// # What this costs, MEASURED 2026-08-21 — say it here rather than find it later
///
/// On a model whose every eligibility face is already carried, an appended
/// contact takes **nothing**: 60 of 60 appends on the overhang table and 60 of
/// 60 on the thin fin produced an appended contact with
/// `carried_area_mm2 == 0.0`, exactly. That is what "only the load it newly
/// takes" MEANS when nothing is going spare, and it is conservative for every
/// incumbent — but it hands PR-4's area law a zero load for the new contact,
/// so every interface is trivially sufficient and the tip is sized at the
/// registered minimum (0.05 mm RADIUS, 0.1 mm DIAMETER). That is the condition
/// `contact_regression_gate`'s MAJOR-7 entry names.
///
/// It is the SAME missing capability that stops contract (2) at the replay
/// seam: this ledger cannot say **a face is carried by more than one contact**.
/// Give it that and both follow at once — an over-block splits across the
/// contacts that really hold it, and an appended contact takes its honest
/// share of a face its neighbour is already carrying instead of taking zero.
/// Until then the appended contact is a coverage contact that the load law
/// grades against nothing, and this paragraph is the disclosure.
///
/// # Refusals
///
/// The prior partition must belong to this plan and must have been built over
/// the leading `prior_contact_count` contacts of `contacts`, both checked
/// against fingerprints rather than assumed, and every face it disclosed must
/// still measure the same area on this surface. A caller that hands over a
/// partition from anywhere else gets
/// [`GenericPerceptionError::MismatchedPriorLoadPartition`], never a silently
/// wrong ledger.
#[allow(
    clippy::too_many_arguments,
    reason = "the full replay next door takes seven of these eight and this path must be handed \
              exactly the same authorities plus the prior partition; bundling them would hide an \
              authority source behind a struct nobody else builds"
)]
pub(crate) fn append_to_specialized_load_partition(
    plan: &GenericContactPlan,
    verified_mesh: &VerifiedMeshFingerprint<'_>,
    surface: &NativeSurface,
    prior: &FinalizedLoadPartition,
    contacts: &[PhysicalContact],
    prior_contact_count: usize,
    specialized_owner_faces: &[(u32, u32)],
    limits: LoadFinalizeLimits,
) -> Result<FinalizedLoadPartition, GenericPerceptionError> {
    // Same guard, same place as the full replay: a proof of another mesh must
    // never carry a face range past the surface it indexes.
    let mesh = verified_mesh.mesh();
    if verified_mesh.fingerprint() != plan.authority.mesh {
        return Err(GenericPerceptionError::StaleAuthority);
    }
    if prior.authority != plan.authority {
        return Err(GenericPerceptionError::StaleAuthority);
    }
    if prior_contact_count > contacts.len() {
        return Err(GenericPerceptionError::MismatchedPriorLoadPartition);
    }
    let prior_contacts = &contacts[..prior_contact_count];
    let prior_order = validated_contact_order(prior_contacts)?;
    if hash_physical_contacts(prior_contacts, &prior_order) != prior.contacts_fingerprint {
        return Err(GenericPerceptionError::MismatchedPriorLoadPartition);
    }
    let contact_order = validated_contact_order(contacts)?;

    let max_carriable_mm2 = CarriableMaximum::AreaLaw(registered_max_carriable_area_mm2()?);
    let geometry = surface.geometry();

    // What the prior partition already charged, and what it disclosed. A face
    // is charged or disclosed, never both, so a face's uncarried area is its
    // whole area when it was disclosed (or never an eligibility row at all)
    // and nothing otherwise.
    let mut charged_faces = prior.rows.iter().map(|row| row.face_id).collect::<Vec<_>>();
    charged_faces.sort_unstable();
    charged_faces.dedup();
    let mut disclosed = prior
        .ownerless
        .iter()
        .map(|face| (face.face_id, face.area_mm2))
        .collect::<Vec<_>>();
    disclosed.sort_unstable_by_key(|&(face_id, _)| face_id);
    // The disclosure carries the eligibility row's own area; re-reading that
    // area off this surface proves the two agree bit for bit before any of it
    // is charged to an appended contact.
    for &(face_id, area_mm2) in &disclosed {
        let index = face_id as usize;
        if index >= mesh.tris().len() {
            return Err(GenericPerceptionError::SpecializedFaceOutOfRange { face_id });
        }
        if geometry.areas()[index].to_bits() != area_mm2.to_bits() {
            return Err(GenericPerceptionError::MismatchedPriorLoadPartition);
        }
    }

    // Every face each appended contact is eligible for, ascending and
    // deduplicated: an island owner reaches its island's faces, a specialized
    // owner reaches the faces its own reservation pairs name.
    let mut candidates: Vec<(u32, f64, u64, usize)> = Vec::new();
    let mut appended_work = 0usize;
    for (contact_index, contact) in contacts.iter().enumerate().skip(prior_contact_count) {
        let faces = appended_contact_faces(contact, plan.islands(), specialized_owner_faces)?;
        appended_work = appended_work.saturating_add(faces.len());
        for face_id in faces {
            let index = face_id as usize;
            if index >= mesh.tris().len() {
                return Err(GenericPerceptionError::SpecializedFaceOutOfRange { face_id });
            }
            // Charged already? Then this append takes nothing of it: the
            // contact that carries it goes on carrying it, bit for bit.
            if charged_faces.binary_search(&face_id).is_ok() {
                continue;
            }
            let centroid = geometry.centroids()[index];
            let distance = contact
                .point_mm
                .iter()
                .zip(centroid)
                .map(|(left, right)| (left - right).powi(2))
                .sum::<f64>();
            candidates.push((face_id, distance, contact.contact_id, contact_index));
        }
    }
    let operations = appended_work
        .saturating_add(candidates.len())
        .saturating_add(prior.rows.len())
        .saturating_add(prior.ownerless.len());
    let operation_limit = limits.operations_limit(prior.rows.len(), contacts.len());
    if operations > operation_limit {
        return Err(GenericPerceptionError::LoadOperationLimit {
            required: operations,
            limit: operation_limit,
        });
    }
    // Face-major, then the taker order contract (2) shares with the replay.
    candidates.sort_unstable_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.total_cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });

    let mut taken_rows: Vec<FinalizedFaceLoad> = Vec::new();
    let mut still_ownerless: Vec<OwnerlessLoadFace> = Vec::new();
    let mut takers: Vec<(f64, u64, usize)> = Vec::new();
    let mut start = 0usize;
    while start < candidates.len() {
        let face_id = candidates[start].0;
        let mut end = start;
        while end < candidates.len() && candidates[end].0 == face_id {
            end += 1;
        }
        takers.clear();
        takers.extend(candidates[start..end].iter().map(
            |&(_, distance, contact_id, contact_index)| (distance, contact_id, contact_index),
        ));
        // A face the prior partition disclosed keeps the disclosure's own
        // area; a face no prior eligibility row ever named takes the surface's.
        let area_mm2 = disclosed
            .binary_search_by_key(&face_id, |&(id, _)| id)
            .map_or_else(
                |_| geometry.areas()[face_id as usize],
                |index| disclosed[index].1,
            );
        charge_face_from_takers(
            face_id,
            area_mm2,
            &takers,
            contacts,
            max_carriable_mm2,
            &mut taken_rows,
            &mut still_ownerless,
        );
        start = end;
    }

    // Merge, ascending by face id, prior rows first among equals: the prior
    // rows keep their order and so does every contact's addend sequence.
    let mut rows = Vec::with_capacity(prior.rows.len() + taken_rows.len());
    merge_ascending_by(
        &prior.rows,
        &taken_rows,
        |row| row.face_id,
        |row| row.face_id,
        &mut rows,
    );

    // Ownerless: every prior disclosure an appended contact did NOT take,
    // plus whatever the appends could not legally carry either.
    let taken = taken_rows
        .iter()
        .map(|row| row.face_id)
        .collect::<BTreeSet<_>>();
    let surviving = prior
        .ownerless
        .iter()
        .copied()
        .filter(|face| !taken.contains(&face.face_id))
        .collect::<Vec<_>>();
    let fresh = still_ownerless
        .into_iter()
        .filter(|face| {
            prior
                .ownerless
                .binary_search_by_key(&face.face_id, |prior_face| prior_face.face_id)
                .is_err()
        })
        .collect::<Vec<_>>();
    let mut ownerless = Vec::with_capacity(surviving.len() + fresh.len());
    merge_ascending_by(
        &surviving,
        &fresh,
        |face| face.face_id,
        |face| face.face_id,
        &mut ownerless,
    );

    Ok(FinalizedLoadPartition {
        authority: plan.authority.clone(),
        contacts_fingerprint: hash_physical_contacts(contacts, &contact_order),
        rows,
        ownerless,
    })
}

/// The faces one appended contact is eligible for, ascending and distinct.
///
/// Read from the SAME two provenance relations
/// [`build_load_eligibility`]/[`extend_load_eligibility`] read — an island
/// owner reaches its island's faces, a specialized owner reaches the faces its
/// reservation pairs name — so eligibility cannot mean one thing on the append
/// path and another on the replay.
fn appended_contact_faces(
    contact: &PhysicalContact,
    islands: &[SupportIsland],
    specialized_owner_faces: &[(u32, u32)],
) -> Result<Vec<u32>, GenericPerceptionError> {
    let mut faces = match contact.owner {
        LoadOwner::GenericIsland(island_id) => islands
            .iter()
            .find(|island| island.id == island_id)
            .ok_or(GenericPerceptionError::MismatchedPriorLoadPartition)?
            .face_ids
            .clone(),
        LoadOwner::Specialized(owner_id) => specialized_owner_faces
            .iter()
            .filter(|&&(owner, _)| owner == owner_id)
            .map(|&(_, face_id)| face_id)
            .collect(),
    };
    faces.sort_unstable();
    faces.dedup();
    Ok(faces)
}

/// Stable merge of two ascending slices into `out`, left first among equals.
fn merge_ascending_by<T: Clone, K: Ord>(
    left: &[T],
    right: &[T],
    left_key: impl Fn(&T) -> K,
    right_key: impl Fn(&T) -> K,
    out: &mut Vec<T>,
) {
    let (mut i, mut j) = (0usize, 0usize);
    while i < left.len() && j < right.len() {
        if right_key(&right[j]) < left_key(&left[i]) {
            out.push(right[j].clone());
            j += 1;
        } else {
            out.push(left[i].clone());
            i += 1;
        }
    }
    out.extend_from_slice(&left[i..]);
    out.extend_from_slice(&right[j..]);
}

/// The area law's **single-contact maximum** carried area, in mm².
///
/// `X * pi * r_max^2`, every term registry-read: `X` is the engine's implied
/// area ratio (`tensile / (peel * safety factor)` = 12, the one-sided partner
/// of the registered `X_ALLOWED`) and `r_max` is the registered
/// `TIP_RADIUS_MAX_LEGACY_MM` = 1.000 mm RADIUS (2.000 mm DIAMETER). It
/// currently reads 37.699 mm².
///
/// This is a READ of the law, not a new threshold: it is the exact area at
/// which [`FrozenTipLoadPolicy::tip_radius_mm`] reaches `r_max`, so a contact
/// charged more than this is one whose load-sized tip the clamp denied, and
/// [`FrozenTipLoadPolicy::violates_safety_factor`] is satisfied with equality
/// there rather than by a hair either side (see the unit test
/// `the_single_contact_maximum_is_exactly_where_the_law_runs_out`).
pub(crate) fn registered_max_carriable_area_mm2() -> Result<f64, GenericPerceptionError> {
    let constants = m0_constants().map_err(|error| {
        GenericPerceptionError::Perception(SupportPerceptionError::RegisteredConstantsUnavailable(
            error.to_string(),
        ))
    })?;
    Ok(FrozenTipLoadPolicy::from_placeholders(constants.placeholders()).max_carriable_area_mm2())
}

/// The area law's tip RADIUS, in mm, for one contact charged with `area_mm2`.
///
/// **A READ of the shipping law, not a second copy of it.** It calls the same
/// [`FrozenTipLoadPolicy::tip_radius_mm`] every contact stub is sized by, built
/// from the same registered placeholders, so a caller cannot drift from what
/// the engine builds. Both registered clamps are INSIDE the answer, exactly
/// where production applies them: an area above the single-contact maximum
/// comes back at `r_max` rather than at the unclamped law radius, and an area
/// at or near zero comes back at the registered minimum. Nothing new is
/// registered and nothing here decides anything.
///
/// **Why it is public.** [`crate::EmittedContactDisclosure`] states a contact's
/// carried area and the radius of the tip that carries it as two INDEPENDENTLY
/// sourced numbers — the area from the accepted plan, the radius from the final
/// live forest — joined only by contact id. The law is what ties them together,
/// so without a way to evaluate it a reader can check that each field is a
/// number but not that the two still belong to each other. The `prepare_folder`
/// gate uses it for exactly that.
///
/// UNITS: `area_mm2` is an AREA in mm²; the answer is a RADIUS in mm.
pub fn registered_tip_radius_mm_for_carried_area(
    area_mm2: f64,
) -> Result<f64, GenericPerceptionError> {
    let constants = m0_constants().map_err(|error| {
        GenericPerceptionError::Perception(SupportPerceptionError::RegisteredConstantsUnavailable(
            error.to_string(),
        ))
    })?;
    Ok(
        FrozenTipLoadPolicy::from_placeholders(constants.placeholders())
            .tip_radius_mm(area_mm2)?
            .get(),
    )
}

/// What one contact may legally be charged for a single face — task #68
/// contract (2).
///
/// Two states, and the second is an escalation marker rather than a physical
/// claim: see the block comment at [`finalize_load_rows`]'s charge for why the
/// merge tournament's partition is still built `SingleCarrier`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CarriableMaximum {
    /// The area law's single-contact maximum in mm², from
    /// [`registered_max_carriable_area_mm2`]. A face above it is split or
    /// refused.
    AreaLaw(f64),
    /// One carrier per face whatever its area — the predecessor's behaviour,
    /// kept only where a reader indexes the partition's rows positionally and
    /// cannot express a face carried by several contacts.
    SingleCarrier,
}

/// How many contacts one face's area has to be spread over for every share to
/// be law-legal.
///
/// `Some(1)` for every face at or below the single-contact maximum, which is
/// every face the predecessor ever charged correctly; `Some(k)` with `k >= 2`
/// for a face that needs `k` carriers; `None` when no legal split can even be
/// STATED (a non-finite area, or a maximum that is not a usable positive
/// number), which the caller turns into the refusal arm rather than a guess.
fn carriable_share_count(area_mm2: f64, maximum: CarriableMaximum) -> Option<usize> {
    if !area_mm2.is_finite() {
        return None;
    }
    let CarriableMaximum::AreaLaw(max_carriable_mm2) = maximum else {
        return Some(1);
    };
    if !max_carriable_mm2.is_finite() || max_carriable_mm2 <= 0.0 {
        return None;
    }
    if area_mm2 <= max_carriable_mm2 {
        return Some(1);
    }
    let shares = (area_mm2 / max_carriable_mm2).ceil();
    if !shares.is_finite() || shares > usize::MAX as f64 {
        return None;
    }
    // `shares >= 2` here: the branch above took every area at or below the
    // maximum, and the maximum is finite and positive.
    Some(shares as usize)
}

/// Every contact eligible for one load row, with the SAME squared distance and
/// the SAME tie-break [`scan_transposed_load_core`] uses.
///
/// Written as a membership test against the transpose's own ascending row list
/// rather than as a second eligibility rule, so "eligible" cannot drift
/// between the scan and the split. Only faces above the single-contact maximum
/// ever reach it, and the caller charges the visits it costs.
fn gather_eligible_contacts(
    transpose: &LoadEligibilityTranspose,
    contacts: &[PhysicalContact],
    centroid_mm: [f64; 3],
    face_index: usize,
    out: &mut Vec<(f64, u64, usize)>,
) {
    out.clear();
    let Ok(row) = u32::try_from(face_index) else {
        return;
    };
    for (contact_index, contact) in contacts.iter().enumerate() {
        if transpose
            .rows_of(contact_index)
            .binary_search(&row)
            .is_err()
        {
            continue;
        }
        let distance = contact
            .point_mm
            .iter()
            .zip(centroid_mm)
            .map(|(left, right)| (left - right).powi(2))
            .sum::<f64>();
        out.push((distance, contact.contact_id, contact_index));
    }
    out.sort_unstable_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
    });
}

/// Charge one load row to the contacts that carry it — task #68 contract (2).
///
/// Three outcomes, and no fourth:
///
/// - **one carrier.** The face fits under the single-contact maximum, so it
///   goes whole to the nearest eligible contact. This is the predecessor's
///   only behaviour and it is reached through the predecessor's own `nearest`
///   argmin, so every face a correct partition ever charged is charged
///   bit-identically.
/// - **`k` carriers.** The face is bigger than one contact may legally carry,
///   so its area is divided EQUALLY over the `k` nearest eligible contacts,
///   `k = ceil(area / maximum)`, and every share is strictly below the
///   maximum. Nearest-first is the established rule and nothing else about it
///   moves; the last carrier takes the running remainder so the shares sum to
///   the face's own area rather than to `k` roundings of it.
/// - **no carrier.** Fewer than `k` contacts are eligible — including the
///   `k = 1`, zero-eligible case §6 already disclosed — so no legal split
///   exists and the WHOLE face is disclosed through
///   [`OwnerlessLoadFace`], named and measured. A face is charged or
///   disclosed, never both, which is the invariant task #49's fixture asserts.
#[allow(
    clippy::too_many_arguments,
    reason = "every argument is a distinct authority the charge reads — the eligibility row, its \
              index, the scan's own argmin, the contact set, the transpose the split reads \
              membership from, the maximum, and the three buffers the caller owns; bundling them \
              would hide which of them the rule actually uses"
)]
fn charge_one_face(
    eligibility: &LoadEligibility,
    face_index: usize,
    nearest: Option<(usize, f64)>,
    contacts: &[PhysicalContact],
    transpose: &LoadEligibilityTranspose,
    max_carriable_mm2: CarriableMaximum,
    takers: &mut Vec<(f64, u64, usize)>,
    rows: &mut Vec<FinalizedFaceLoad>,
    ownerless: &mut Vec<OwnerlessLoadFace>,
) {
    // One carrier is the overwhelmingly common case and the argmin the scan
    // already produced IS the head of the sorted taker list — same squared
    // distance, same `(distance, contact_id)` tie-break — so the cheap path
    // stages that one candidate instead of gathering the whole column. The
    // RULE below is not restated for it; there is one rule.
    let single = carriable_share_count(eligibility.area_mm2, max_carriable_mm2) == Some(1);
    if single {
        takers.clear();
        if let Some((contact_index, distance)) = nearest {
            takers.push((distance, contacts[contact_index].contact_id, contact_index));
        }
    } else {
        gather_eligible_contacts(
            transpose,
            contacts,
            eligibility.centroid_mm,
            face_index,
            takers,
        );
    }
    charge_face_from_takers(
        eligibility.face_id,
        eligibility.area_mm2,
        takers,
        contacts,
        max_carriable_mm2,
        rows,
        ownerless,
    );
}

/// The one carriable-partition rule, over a taker list already ordered by
/// `(squared distance, contact id)`.
///
/// Shared verbatim by the full replay and by the incremental append, so the
/// two cannot disagree about what a legal charge is.
///
/// # One behaviour change at the replay seam, named rather than discovered
///
/// A **non-finite** eligibility area is now disclosed through
/// [`OwnerlessLoadFace`] instead of being charged whole to the nearest contact,
/// under BOTH maxima — [`carriable_share_count`] answers `None` before it looks
/// at the maximum at all. That is the only input on which this function differs
/// from its predecessor, it is almost certainly unreachable (areas are read off
/// a validated surface), and it replaces a non-finite ledger entry with a
/// disclosure. `a_non_finite_area_is_disclosed_rather_than_charged_under_either_maximum`
/// measures it.
fn charge_face_from_takers(
    face_id: u32,
    area_mm2: f64,
    takers: &[(f64, u64, usize)],
    contacts: &[PhysicalContact],
    max_carriable_mm2: CarriableMaximum,
    rows: &mut Vec<FinalizedFaceLoad>,
    ownerless: &mut Vec<OwnerlessLoadFace>,
) {
    let charged = carriable_share_count(area_mm2, max_carriable_mm2)
        .filter(|shares| *shares <= takers.len())
        .and_then(|shares| {
            Some((
                shares,
                equal_shares_mm2(area_mm2, shares, max_carriable_mm2)?,
            ))
        });
    let Some((shares, split)) = charged else {
        ownerless.push(OwnerlessLoadFace { face_id, area_mm2 });
        return;
    };
    for (&(_, _, contact_index), carried_area_mm2) in takers[..shares].iter().zip(split) {
        let contact = &contacts[contact_index];
        rows.push(FinalizedFaceLoad {
            face_id,
            contact_id: contact.contact_id,
            owner: contact.owner,
            carried_area_mm2,
        });
    }
}

/// `shares` law-legal pieces of `area_mm2` that sum to `area_mm2`.
///
/// The first `shares - 1` are the exact equal share; the last is the running
/// remainder, so the ledger totals the face's own area instead of `shares`
/// separately-rounded copies of a quotient. `None` when the result would not
/// be a legal split after all — a non-finite or negative piece, or a piece
/// above the single-contact maximum — which is the honest "no legal split
/// exists" answer rather than a charge that breaks the law it was built to
/// keep.
fn equal_shares_mm2(area_mm2: f64, shares: usize, maximum: CarriableMaximum) -> Option<Vec<f64>> {
    let share = area_mm2 / shares as f64;
    let mut pieces = Vec::with_capacity(shares);
    let mut remaining = area_mm2;
    for _ in 0..shares - 1 {
        pieces.push(share);
        remaining -= share;
    }
    pieces.push(remaining);
    let legal = |piece: &f64| {
        piece.is_finite()
            && *piece >= 0.0
            && match maximum {
                CarriableMaximum::AreaLaw(max_carriable_mm2) => *piece <= max_carriable_mm2,
                CarriableMaximum::SingleCarrier => true,
            }
    };
    pieces.iter().all(legal).then_some(pieces)
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
/// built once, in one sort, and read by both.  (That operation bound is quoted
/// as it stood on 2026-08-06; FU-8f phase G5 replaced it on 2026-08-07 with the
/// transpose's own terms.  The row count still drives the replacement — it is
/// the `face_count` of [`load_resource_plan_from_counts`] and the row set the
/// eligibility entries are counted over — so this paragraph's point is
/// unaffected.)
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
///
/// # The two per-proposal allocations that are still here, and why they stay
///
/// [`SpecializedLoadBaseline::merged_violation_count`] allocates twice per
/// proposal outside this struct — `vec![0u32; touched_count]` for the gained-row
/// counts and a `gained_starts.clone()` for the fill cursors.  FU-8e phase E4
/// listed folding both into this struct as an optional, zero-risk change, and
/// **measured it instead of assuming it**.  It is NOT adopted, because the
/// measurement says it would buy nothing.
///
/// Measured 2026-08-07 on the Weeping Treant (`PLINTH_CENSUS=1`, release,
/// `--features census`, `PLINTH_THREADS=4`), with env-gated scratch timers at
/// the branch tip that were reverted by editing afterwards: across 33 merge
/// rounds of 39,096–39,223 proposals each, the two allocations together cost a
/// mean **6.7 ms per round** — **0.13 %** of the 4.98 s that
/// `merged_violation_count` and its `merged_rows` gather spend, and 0.05 % of
/// the 13.8 s round.  The reason is that `touched_count` is *small*: a merge
/// proposal touches a mean of **3.77 contacts**, not 1,365, so each `vec!` is
/// about fifteen bytes.  What `merged_violation_count` actually costs is the row
/// work over `elig(left) ∪ elig(right)`, which no buffer reuse touches.
///
/// The general lesson is the one FU-8e's two framing corrections already made
/// twice: at this scale allocation counts are a poor proxy for cost, and the
/// row traffic is the thing.  Per ruling R-e the totals above are a dated
/// transcript, not a pinned literal.
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
    ///
    /// FU-8f phase G6a: the finalize's own [`LoadEligibilityTranspose`] is
    /// returned beside the partition and the baseline rather than dropped, so
    /// the gate can retain it across rounds
    /// ([`RetainedLoadTranspose::absorb_full_build`]).
    pub(crate) fn build(
        plan: &GenericContactPlan,
        mesh: &VerifiedMeshFingerprint<'_>,
        surface: &NativeSurface,
        contacts: &[PhysicalContact],
        specialized_owner_faces: &[(u32, u32)],
        limits: LoadFinalizeLimits,
    ) -> Result<(FinalizedLoadPartition, Self, LoadEligibilityTranspose), GenericPerceptionError>
    {
        let mut retention = LoadRowRetention {
            witnesses: Vec::new(),
            centroids_mm: Vec::new(),
            transpose: None,
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
        let transpose = retention
            .transpose
            .take()
            .expect("finalize_load_rows retains the transpose whenever retention is requested");
        let baseline = Self::assemble(
            &partition,
            contacts,
            retention.witnesses,
            retention.centroids_mm,
        )?;
        Ok((partition, baseline, transpose))
    }

    /// The post-scan half of [`Self::build`]: fold the finalized partition and
    /// the scan's own witness table into the gate's baseline state.
    ///
    /// FU-8f phase G6b: extracted so the retained fast path
    /// ([`RetainedLoadTranspose::finalize_retained_round`]) runs the **same**
    /// code as the full build — carried areas are re-summed fresh from the
    /// partition in the fold's own order every round, never adjusted
    /// incrementally (`f64` addition is not associative; FU-8d F3's own rule),
    /// and the winner/won CSR is rebuilt from the scan's witnesses, so the
    /// assembled baseline cannot depend on which path produced its inputs.
    fn assemble(
        partition: &FinalizedLoadPartition,
        contacts: &[PhysicalContact],
        witnesses: Vec<[LoadWitness; LOAD_WITNESS_SLOTS]>,
        centroids_mm: Vec<[f64; 3]>,
    ) -> Result<Self, GenericPerceptionError> {
        // Task #49, stated where the limitation actually is. The finalizer now
        // DISCLOSES a face with no eligible contact instead of refusing on it
        // (`OwnerlessLoadFace`), but this baseline's arrays are indexed by
        // ELIGIBILITY ROW — `row_centroid_mm`, `witnesses` and `row_area_mm2`
        // are read at the same index by `merged_violation_count`, whose
        // `merged_rows` are row indices the caller derives from its own
        // `(owner, face)` pair list. A partition with fewer rows than
        // eligibility rows would silently shear that alignment.
        //
        // So the merge gate keeps the refusal it has always had, with the same
        // variant and the same face id, and it keeps it for a reason it can
        // state: this representation cannot express "carried by nobody". It is
        // NOT the §6 site — the gate's tournament runs before routing, over the
        // full candidate set, where an owner with no contact at all means the
        // selection stage produced one, not that a contact failed to route.
        if let Some(face) = partition.ownerless.first() {
            return Err(GenericPerceptionError::MissingPhysicalContact {
                face_id: face.face_id,
            });
        }
        let loads = FinalizedContactLoads::from_physical_contacts(
            partition,
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
        for slots in &witnesses {
            if slots[0].contact == NO_LOAD_WITNESS {
                // Unreachable, and the guard that makes it so is the merge
                // gate's own early return ~30 lines above: a partition with
                // any `ownerless` row is refused with `MissingPhysicalContact`
                // before this loop runs, and a witness with no contact is
                // exactly an ownerless row.
                //
                // Task #49 corrected this rationale. It used to name
                // `finalize_load_rows` as the thing that had already refused.
                // `finalize_load_rows` no longer refuses at all — it discloses
                // ownerless faces (`OwnerlessLoadFace`) and returns — so if
                // that early return is ever lifted, this is a live path and
                // this comment is the notice.
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

        Ok(Self {
            row_area_mm2,
            row_centroid_mm: centroids_mm,
            witnesses,
            winner,
            won_starts,
            won_rows,
            carried_area_mm2: loads.carried_area_mm2().to_vec(),
            violation_count,
            tip_policy: loads.tip_policy,
            retained_bytes,
        })
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
    use plinth_mesh::{
        make_box, make_overhang_table, make_raking_blade, make_thin_fin, make_toed_slab,
    };

    const SWEEP_CHILD: &str = "PLINTH_PERCEPTION_SWEEP_CHILD";
    const SWEEP_LINE: &str = "PLINTH_PERCEPTION_SWEEP=";

    const TEST_KNN_LIMITS: KnnBatchLimits = KnnBatchLimits {
        max_source_points: 20_000_000,
        max_query_points: 20_000_000,
        max_neighbors: 1,
        max_logical_peak_bytes: 2 * 1024 * 1024 * 1024,
        max_work: 100_000_000,
    };

    /// The production tip policy, read from the registry exactly as the
    /// finalizer reads it — never a hand-typed mirror.
    fn registered_tip_policy() -> FrozenTipLoadPolicy {
        FrozenTipLoadPolicy::from_placeholders(
            m0_constants().expect("registered constants").placeholders(),
        )
    }

    /// **Task #68 contract (2), part 1 — the single-contact maximum is a READ
    /// of the law, not a new threshold.**
    ///
    /// `X * pi * r_max^2` is exactly the area at which the load law's own tip
    /// sizing reaches the registered clamp maximum, and exactly the area at
    /// which the engine's own `violates_safety_factor` runs out of margin. If
    /// a future recalibration moves either term this test moves with it,
    /// because it reads both from the registry rather than restating them.
    ///
    /// MEASURED 2026-08-21: 37.69911184307752 mm², at `r_max` = 1.000 mm
    /// RADIUS (2.000 mm DIAMETER).
    #[test]
    fn the_single_contact_maximum_is_exactly_where_the_law_runs_out() {
        let policy = registered_tip_policy();
        let maximum = policy.max_carriable_area_mm2();
        let r_max = PositiveFinite::new(policy.max_radius_mm).expect("positive clamp maximum");

        assert!(
            !policy.violates_safety_factor(maximum, r_max),
            "the maximum itself must be legal at the biggest legal tip: {maximum} mm²"
        );
        assert!(
            policy.violates_safety_factor(maximum * 1.001, r_max),
            "a thousandth above the maximum must not be: {maximum} mm²"
        );
        assert!(
            (policy
                .tip_radius_mm(maximum)
                .expect("the maximum sizes a tip")
                .get()
                - policy.max_radius_mm)
                .abs()
                < 1.0e-12,
            "the maximum is where the load law's own sizing reaches the clamp"
        );
        // The one-sided anchor relationship the constants gate already states:
        // the engine's implied ratio never EXCEEDS the registered `X_ALLOWED`.
        let allowance = *m0_constants()
            .expect("registered constants")
            .evidence()
            .x_allowed()
            .value();
        assert!(
            policy.implied_area_ratio() <= allowance + 1.0e-9,
            "the maximum must be anchored to X_ALLOWED, not to a hand-typed twin"
        );
        assert_eq!(
            registered_max_carriable_area_mm2().expect("registry read"),
            maximum,
            "the finalizer and this test must read one number"
        );
    }

    /// **Task #68 contract (2), part 2 — an over-block is SPLIT law-legally or
    /// REFUSED with disclosure, and never assigned whole.**
    ///
    /// The block is the dust fixture's own: one triangle of the 42 x 26 mm
    /// table top, 546.0 mm², **14.48x** the single-contact maximum. Under the
    /// predecessor that whole block was charged to whichever contact the
    /// nearest-eligible scan named; that contact's tip was then clamped at the
    /// 1.000 mm RADIUS maximum and it failed routing on clearance
    /// (`AllFeasibleCandidatesRefusedByClearance`, 92 feasible candidates all
    /// refused), which is the mechanism task #68 measured 182 times out of 182.
    ///
    /// **Revert-proof. Both sabotages MEASURED 2026-08-21, not asserted.**
    ///
    /// 1. `charge_face_from_takers` reverted to the predecessor — the whole
    ///    area to `takers[0]` — fails here with
    ///    `left: 1, right: 15` and the offending row printed:
    ///    `contact_id: 0, carried_area_mm2: 546.0`.
    /// 2. The refusal arm removed — the share-count filter turned into
    ///    `shares.min(takers.len())` AND `equal_shares_mm2`'s legality guard
    ///    dropped — fails on the disclosure assertion with fourteen rows of
    ///    **39.0 mm²**, each above the maximum. Removing only ONE of those two
    ///    guards is caught by the other and this test stays green: they are
    ///    deliberate defence in depth, and this note is the record of which
    ///    single edit is not independently detected.
    ///
    /// No other test in the suite catches either sabotage.
    #[test]
    fn an_over_block_is_split_law_legally_or_refused_but_never_charged_whole() {
        let policy = registered_tip_policy();
        let maximum = policy.max_carriable_area_mm2();
        let r_max = PositiveFinite::new(policy.max_radius_mm).expect("positive clamp maximum");
        let law = CarriableMaximum::AreaLaw(maximum);
        let block_mm2 = 546.0_f64;

        let shares = carriable_share_count(block_mm2, law)
            .expect("a finite block under a finite maximum states its share count");
        assert_eq!(
            shares, 15,
            "the dust fixture's block needs fifteen carriers"
        );

        // Ascending distance, ascending id: the taker order the replay's own
        // `(distance, contact_id)` scan produces.
        let contacts = (0..shares as u64)
            .map(|contact_id| PhysicalContact {
                contact_id,
                owner: LoadOwner::GenericIsland(0),
                point_mm: [contact_id as f64, 0.0, 0.0],
            })
            .collect::<Vec<_>>();
        let takers = contacts
            .iter()
            .enumerate()
            .map(|(index, contact)| (index as f64, contact.contact_id, index))
            .collect::<Vec<_>>();

        // 1. Enough eligible contacts — SPLIT.
        let mut rows = Vec::new();
        let mut ownerless = Vec::new();
        charge_face_from_takers(
            7,
            block_mm2,
            &takers,
            &contacts,
            law,
            &mut rows,
            &mut ownerless,
        );
        assert!(
            ownerless.is_empty(),
            "a block fifteen contacts can legally carry must not be disclosed as unowned"
        );
        assert_eq!(rows.len(), shares, "one row per carrier: {rows:?}");
        for row in &rows {
            assert!(
                !policy.violates_safety_factor(row.carried_area_mm2, r_max),
                "share {} mm² is above what one contact may legally carry",
                row.carried_area_mm2
            );
        }
        let total = rows.iter().map(|row| row.carried_area_mm2).sum::<f64>();
        assert!(
            (total - block_mm2).abs() < 1.0e-12,
            "the shares must total the block, not fifteen roundings of a quotient: {total}"
        );
        assert_eq!(
            rows.iter().map(|row| row.contact_id).collect::<Vec<_>>(),
            (0..shares as u64).collect::<Vec<_>>(),
            "the carriers are the NEAREST eligible contacts, in the scan's own order"
        );

        // 2. One contact short — REFUSED with disclosure, nothing charged.
        let mut rows = Vec::new();
        let mut ownerless = Vec::new();
        charge_face_from_takers(
            7,
            block_mm2,
            &takers[..shares - 1],
            &contacts,
            law,
            &mut rows,
            &mut ownerless,
        );
        assert!(
            rows.is_empty(),
            "a block no legal split can carry must be charged to nobody: {rows:?}"
        );
        assert_eq!(
            ownerless,
            vec![OwnerlessLoadFace {
                face_id: 7,
                area_mm2: block_mm2
            }],
            "the refusal must NAME and MEASURE the block it could not place"
        );

        // 3. The predecessor's own case — one eligible contact — is the one
        //    the contract forbids outright.
        let mut rows = Vec::new();
        let mut ownerless = Vec::new();
        charge_face_from_takers(
            7,
            block_mm2,
            &takers[..1],
            &contacts,
            law,
            &mut rows,
            &mut ownerless,
        );
        assert!(
            rows.is_empty() && ownerless.len() == 1,
            "one contact must never be handed the whole 546 mm² block"
        );

        // 4. And the tournament's `SingleCarrier` representation still does
        //    exactly that. This is the ESCALATION recorded on task #68 —
        //    asserted here so it cannot be lost quietly, and so that the day
        //    `SpecializedLoadBaseline` can express a multi-carrier face, this
        //    assertion is what fails and points at the switch.
        let mut rows = Vec::new();
        let mut ownerless = Vec::new();
        charge_face_from_takers(
            7,
            block_mm2,
            &takers[..1],
            &contacts,
            CarriableMaximum::SingleCarrier,
            &mut rows,
            &mut ownerless,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].carried_area_mm2, block_mm2);
        assert!(ownerless.is_empty());
    }

    /// Every face at or below the maximum is charged exactly as it always was,
    /// under BOTH maxima — the split path adds a case, it does not move one.
    #[test]
    fn a_face_within_the_law_is_charged_whole_to_the_nearest_under_either_maximum() {
        let maximum = registered_max_carriable_area_mm2().expect("registry read");
        let contacts = (0..3u64)
            .map(|contact_id| PhysicalContact {
                contact_id,
                owner: LoadOwner::GenericIsland(0),
                point_mm: [contact_id as f64, 0.0, 0.0],
            })
            .collect::<Vec<_>>();
        let takers = contacts
            .iter()
            .enumerate()
            .map(|(index, contact)| (index as f64, contact.contact_id, index))
            .collect::<Vec<_>>();
        for area_mm2 in [1.0e-9, 0.36, maximum] {
            for maximum in [
                CarriableMaximum::AreaLaw(maximum),
                CarriableMaximum::SingleCarrier,
            ] {
                let mut rows = Vec::new();
                let mut ownerless = Vec::new();
                charge_face_from_takers(
                    9,
                    area_mm2,
                    &takers,
                    &contacts,
                    maximum,
                    &mut rows,
                    &mut ownerless,
                );
                assert!(ownerless.is_empty());
                assert_eq!(rows.len(), 1, "{area_mm2} mm² under {maximum:?}");
                assert_eq!(rows[0].contact_id, 0);
                assert_eq!(
                    rows[0].carried_area_mm2.to_bits(),
                    area_mm2.to_bits(),
                    "a face within the law keeps its own area, bit for bit"
                );
            }
        }
        // No eligible contact at all is the §6 disclosure, unchanged.
        let mut rows = Vec::new();
        let mut ownerless = Vec::new();
        charge_face_from_takers(
            9,
            0.36,
            &[],
            &contacts,
            CarriableMaximum::AreaLaw(maximum),
            &mut rows,
            &mut ownerless,
        );
        assert!(rows.is_empty());
        assert_eq!(
            ownerless,
            vec![OwnerlessLoadFace {
                face_id: 9,
                area_mm2: 0.36
            }]
        );
    }

    /// Everything one DIRECT call to [`append_to_specialized_load_partition`]
    /// needs, over the retention fixture's real mesh, plan and surface.
    ///
    /// The prior partition is hand-built because no full replay can produce the
    /// state under test: the replay charges with
    /// [`CarriableMaximum::SingleCarrier`], so it never leaves an over-maximum
    /// face disclosed. The partition is nonetheless a LEGAL one — its authority
    /// is the plan's, its contacts fingerprint is hashed by the production
    /// writer over the production order, its rows are ascending by face id, and
    /// every disclosed area is the surface's own area for that face, bit for
    /// bit. That is exactly what the append's own provenance checks demand, and
    /// [`an_append_refuses_a_prior_partition_that_is_not_this_plans`] proves
    /// each of those demands is really made.
    struct AppendSeamCase {
        fixture: RetentionFixture,
        prior: FinalizedLoadPartition,
        contacts: Vec<PhysicalContact>,
        prior_contact_count: usize,
        pairs: Vec<(u32, u32)>,
        /// The appended contacts eligible for `over_face`, in the order the
        /// append's own `(squared distance, contact id)` sort puts them.
        appended_ids: Vec<u64>,
        /// One further appended contact, eligible for `under_face` ALONE.
        ///
        /// It is separate on purpose. The area law bounds what one contact may
        /// carry in TOTAL, while contract (2) bounds one BLOCK; giving a block
        /// carrier the small face too would have put 37.5 + 8.0 = 45.5 mm² on
        /// one contact — legal block by block, over the law in total.
        /// (MEASURED 2026-08-21: that arrangement made
        /// `FinalizedContactLoads::load_violation_count` answer 1.) That gap is
        /// real and it is task #59's `PreWriteGate::Load` to close, not this
        /// seam's; keeping the two arms on separate carriers lets this test
        /// assert the seam's own contribution — zero violations — without
        /// asserting something the seam does not promise.
        under_taker_id: u64,
        /// The biggest face of the fixture mesh — half its 30 x 30 mm table
        /// top, far above what one contact may legally carry.
        over_face: u32,
        over_area_mm2: f64,
        /// The smallest face of the fixture mesh, which one contact carries
        /// whole.
        under_face: u32,
        under_area_mm2: f64,
        /// `carriable_share_count` of `over_area_mm2` — how many contacts the
        /// area law needs before that block may be charged at all.
        shares: usize,
    }

    /// Build the case with `appended_takers` contacts eligible for the
    /// over-maximum face, plus one more eligible for the small disclosed face
    /// alone, all appended after the fixture's own contacts.
    fn append_seam_case(appended_takers: usize) -> AppendSeamCase {
        let fixture = retention_fixture();
        let maximum = registered_max_carriable_area_mm2().expect("registry read");
        let areas = fixture.surface.geometry().areas().to_vec();
        let centroids = fixture.surface.geometry().centroids().to_vec();

        let mut by_area = (0..areas.len() as u32).collect::<Vec<_>>();
        by_area.sort_unstable_by(|&left, &right| {
            areas[left as usize]
                .total_cmp(&areas[right as usize])
                .then(left.cmp(&right))
        });
        let under_face = by_area[0];
        let under_area_mm2 = areas[under_face as usize];
        let over_face = *by_area.last().expect("the fixture mesh has faces");
        let over_area_mm2 = areas[over_face as usize];
        assert_eq!(
            (under_area_mm2, over_area_mm2),
            (8.0, 450.0),
            "MEASURED 2026-08-21: the overhang table's smallest face is a 4 x 4 mm leg end and its \
             biggest is half the 30 x 30 mm top"
        );
        assert!(
            under_area_mm2 <= maximum && over_area_mm2 > maximum,
            "the case needs one face inside the area law and one above it, against {maximum} mm²"
        );
        let shares = carriable_share_count(over_area_mm2, CarriableMaximum::AreaLaw(maximum))
            .expect("a finite block under a finite maximum states its share count");
        assert_eq!(
            shares, 12,
            "MEASURED 2026-08-21: 450.0 mm² needs twelve carriers at {maximum} mm² each"
        );

        // Two more small faces, CHARGED by the prior partition, so the append's
        // copy-the-survivors path has survivors to copy.
        let mut charged = [by_area[1], by_area[2]];
        charged.sort_unstable();
        assert!(
            ![under_face, over_face].contains(&charged[0])
                && ![under_face, over_face].contains(&charged[1]),
            "the charged faces must be distinct from the disclosed ones"
        );
        let rows = charged
            .iter()
            .enumerate()
            .map(|(index, &face_id)| FinalizedFaceLoad {
                face_id,
                contact_id: fixture.contacts[index].contact_id,
                owner: fixture.contacts[index].owner,
                carried_area_mm2: areas[face_id as usize],
            })
            .collect::<Vec<_>>();
        let mut ownerless = vec![
            OwnerlessLoadFace {
                face_id: under_face,
                area_mm2: under_area_mm2,
            },
            OwnerlessLoadFace {
                face_id: over_face,
                area_mm2: over_area_mm2,
            },
        ];
        ownerless.sort_unstable_by_key(|face| face.face_id);

        let prior_contact_count = fixture.contacts.len();
        let prior_order =
            validated_contact_order(&fixture.contacts).expect("the fixture's contacts are valid");
        let prior = FinalizedLoadPartition {
            authority: fixture.plan.authority.clone(),
            contacts_fingerprint: hash_physical_contacts(&fixture.contacts, &prior_order),
            rows,
            ownerless,
        };

        // Appended owners and ids well clear of the fixture's own, so nothing
        // here can be charged by accident of a collision.
        const FIRST_APPENDED_OWNER: u32 = 1_000;
        let first_appended_id = fixture
            .contacts
            .iter()
            .map(|contact| contact.contact_id)
            .max()
            .expect("the fixture has contacts")
            + 1;
        let over_centroid = centroids[over_face as usize];
        let mut contacts = fixture.contacts.clone();
        let mut pairs = fixture.pairs.clone();
        let mut appended_ids = Vec::with_capacity(appended_takers);
        for index in 0..appended_takers {
            let owner_id = FIRST_APPENDED_OWNER + index as u32;
            let contact_id = first_appended_id + index as u64;
            appended_ids.push(contact_id);
            // Strictly increasing distance from the block's centroid, so the
            // taker order the append sorts into is this appended order.
            contacts.push(PhysicalContact {
                contact_id,
                owner: LoadOwner::Specialized(owner_id),
                point_mm: [
                    over_centroid[0] + 1.0 + index as f64,
                    over_centroid[1],
                    over_centroid[2],
                ],
            });
            pairs.push((owner_id, over_face));
        }
        // One further appended contact, reaching the small face and NOTHING
        // else, so the same call delivers the single-carrier arm too.
        let under_owner = FIRST_APPENDED_OWNER + appended_takers as u32;
        let under_taker_id = first_appended_id + appended_takers as u64;
        contacts.push(PhysicalContact {
            contact_id: under_taker_id,
            owner: LoadOwner::Specialized(under_owner),
            point_mm: centroids[under_face as usize],
        });
        pairs.push((under_owner, under_face));
        pairs.sort_unstable();

        AppendSeamCase {
            fixture,
            prior,
            contacts,
            prior_contact_count,
            pairs,
            appended_ids,
            under_taker_id,
            over_face,
            over_area_mm2,
            under_face,
            under_area_mm2,
            shares,
        }
    }

    impl AppendSeamCase {
        fn append(&self) -> Result<FinalizedLoadPartition, GenericPerceptionError> {
            append_with(
                self,
                &self.fixture.proof(),
                &self.prior,
                &self.contacts,
                self.prior_contact_count,
            )
        }
    }

    /// The same call with each authority presentable separately, so a refusal
    /// arm perturbs exactly one of them and nothing else.
    fn append_with(
        case: &AppendSeamCase,
        proof: &VerifiedMeshFingerprint<'_>,
        prior: &FinalizedLoadPartition,
        contacts: &[PhysicalContact],
        prior_contact_count: usize,
    ) -> Result<FinalizedLoadPartition, GenericPerceptionError> {
        append_to_specialized_load_partition(
            &case.fixture.plan,
            proof,
            &case.fixture.surface,
            prior,
            contacts,
            prior_contact_count,
            &case.pairs,
            retention_limits(),
        )
    }

    /// **Task #68 contract (2), part 3 — the seam DELIVERS the area law, it
    /// does not merely have a rule that could.**
    ///
    /// Parts 1 and 2 prove the rule ([`charge_face_from_takers`], called with
    /// hand-built taker tuples). Neither of them runs the one production site
    /// where contract (2) is live, so neither notices if that site stops
    /// passing [`CarriableMaximum::AreaLaw`]. This test runs it: a real mesh, a
    /// real surface, the real plan authority, a real prior partition, and the
    /// real 450.0 mm² block arriving as a disclosure the appended contacts pick
    /// up.
    ///
    /// It also carries the k >= 2 partition on into
    /// [`FinalizedContactLoads::for_specialized_plan`], because until now no
    /// test had ever put a face carried by more than one contact through that
    /// fold — the safety of it was read, not measured.
    ///
    /// **Revert-proof, MEASURED 2026-08-21.** Change the single live line in
    /// [`append_to_specialized_load_partition`] from
    /// `CarriableMaximum::AreaLaw(registered_max_carriable_area_mm2()?)` to
    /// `CarriableMaximum::SingleCarrier` — deleting contract (2) from the only
    /// place production runs it — and this test fails with
    /// `assertion left == right failed: the 450 mm² block must be SPLIT across
    /// all twelve appended contacts, left: 1, right: 12`, and its refusal twin
    /// fails with `a block no legal split can carry must be charged to nobody`.
    /// Before these two tests existed that edit left the whole scoped gate
    /// green — all 559 `plinth-support` unit tests included.
    #[test]
    fn the_append_seam_delivers_the_area_law_over_a_real_prior_partition() {
        let policy = registered_tip_policy();
        let r_max = PositiveFinite::new(policy.max_radius_mm).expect("positive clamp maximum");
        let case = append_seam_case(12);
        assert_eq!(case.appended_ids.len(), case.shares);
        let child = case.append().expect("the append must succeed");

        // 1. Every prior row survives, verbatim and in order — the append is
        //    incremental, contract (1), and this test is the direct-call twin
        //    of the plan-level sweep in `specialized`.
        let prior_ids = case.contacts[..case.prior_contact_count]
            .iter()
            .map(|contact| contact.contact_id)
            .collect::<BTreeSet<_>>();
        let kept = child
            .rows
            .iter()
            .filter(|row| prior_ids.contains(&row.contact_id))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            kept, case.prior.rows,
            "an append must COPY the prior partition's rows, not re-derive them"
        );
        for (before, after) in case.prior.rows.iter().zip(&kept) {
            assert_eq!(
                before.carried_area_mm2.to_bits(),
                after.carried_area_mm2.to_bits(),
                "face {} moved by an ulp across the append",
                before.face_id
            );
        }

        // 2. The over-maximum block is SPLIT across the appended contacts, and
        //    every share is legal at the biggest legal tip.
        let split = child
            .rows
            .iter()
            .filter(|row| row.face_id == case.over_face)
            .collect::<Vec<_>>();
        assert_eq!(
            split.len(),
            case.shares,
            "the 450 mm² block must be SPLIT across all twelve appended contacts"
        );
        assert_eq!(
            split.iter().map(|row| row.contact_id).collect::<Vec<_>>(),
            case.appended_ids,
            "the carriers are the nearest eligible contacts, in the append's own order"
        );
        for row in &split {
            assert!(
                !policy.violates_safety_factor(row.carried_area_mm2, r_max),
                "share {} mm² is above what one contact may legally carry",
                row.carried_area_mm2
            );
        }
        let total = split.iter().map(|row| row.carried_area_mm2).sum::<f64>();
        assert!(
            (total - case.over_area_mm2).abs() < 1.0e-12,
            "the shares must total the block itself: {total}"
        );
        assert!(
            child
                .ownerless
                .iter()
                .all(|face| face.face_id != case.over_face),
            "a block that was split must not ALSO be disclosed as unowned"
        );

        // 3. The small disclosure goes whole to the one appended contact that
        //    reaches it — the single-carrier arm, through the same call.
        let under = child
            .rows
            .iter()
            .filter(|row| row.face_id == case.under_face)
            .collect::<Vec<_>>();
        assert_eq!(under.len(), 1, "a face inside the law takes one carrier");
        assert_eq!(under[0].contact_id, case.under_taker_id);
        assert_eq!(
            under[0].carried_area_mm2.to_bits(),
            case.under_area_mm2.to_bits(),
            "a face within the law keeps its own area, bit for bit"
        );
        assert!(
            child.ownerless.is_empty(),
            "both disclosures were taken, so nothing is left unowned: {:?}",
            child.ownerless
        );

        // 4. The merged ledger keeps the invariants the next append relies on.
        assert!(
            child
                .rows
                .windows(2)
                .all(|pair| pair[0].face_id <= pair[1].face_id),
            "rows must stay ascending by face id, or the next append's merge is wrong"
        );
        let contact_order =
            validated_contact_order(&case.contacts).expect("the case's contacts are valid");
        assert_eq!(
            child.contacts_fingerprint,
            hash_physical_contacts(&case.contacts, &contact_order),
            "the child partition must fingerprint the WHOLE contact set"
        );
        assert_eq!(child.authority, case.fixture.plan.authority);

        // 5. The k >= 2 partition folds to one load per contact, and the split
        //    is what makes the fold law-legal: twelve shares of 37.5 mm² each
        //    violate nothing, where the 450 mm² whole block is one violation.
        let contact_ids = case
            .contacts
            .iter()
            .map(|contact| contact.contact_id)
            .collect::<Vec<_>>();
        let folded = FinalizedContactLoads::for_specialized_plan(&child, &contact_ids)
            .expect("a multi-carrier partition must fold to one load per contact");
        assert_eq!(
            folded.load_violation_count().expect("violation count"),
            0,
            "every contact's folded load must be inside the area law"
        );
        for (index, &contact_id) in case.appended_ids.iter().enumerate() {
            let leaf = folded
                .leaf_for(contact_id)
                .expect("every appended contact has a folded leaf");
            let expected = split[index].carried_area_mm2;
            assert!(
                (leaf.carried_area_mm2() - expected).abs() < 1.0e-12,
                "contact {contact_id} folded to {} mm², expected {expected} mm²",
                leaf.carried_area_mm2()
            );
        }
        assert_eq!(
            folded
                .leaf_for(case.under_taker_id)
                .expect("the small face's carrier has a folded leaf")
                .carried_area_mm2()
                .to_bits(),
            case.under_area_mm2.to_bits()
        );
    }

    /// **Task #68 contract (2) at the seam, refusal arm — one taker short, the
    /// whole block stays disclosed and NOBODY is handed it.**
    ///
    /// Eleven contacts cannot legally carry a block that needs twelve, so the
    /// append charges none of it. The disclosure survives with its own area,
    /// bit for bit, and exactly once: a face is charged or disclosed, never
    /// both, and never disclosed twice by the merge.
    ///
    /// **Revert-proof, MEASURED 2026-08-21** by the same seam flip its sibling
    /// records: under `CarriableMaximum::SingleCarrier` this test fails with
    /// `a block no legal split can carry must be charged to nobody` — the
    /// nearest of the eleven is handed all 450.0 mm².
    #[test]
    fn the_append_seam_refuses_a_block_one_taker_short_and_keeps_the_disclosure() {
        let case = append_seam_case(11);
        let child = case.append().expect("the append must succeed");

        assert!(
            child.rows.iter().all(|row| row.face_id != case.over_face),
            "a block no legal split can carry must be charged to nobody"
        );
        let disclosed = child
            .ownerless
            .iter()
            .filter(|face| face.face_id == case.over_face)
            .collect::<Vec<_>>();
        assert_eq!(
            disclosed.len(),
            1,
            "the surviving disclosure must appear exactly once: {:?}",
            child.ownerless
        );
        assert_eq!(
            disclosed[0].area_mm2.to_bits(),
            case.over_area_mm2.to_bits(),
            "the refusal must NAME and MEASURE the whole block it could not place"
        );
        // The small face is still taken, so the refusal is specific to the
        // block and not a wholesale failure to charge anything.
        assert!(
            child
                .rows
                .iter()
                .any(|row| row.face_id == case.under_face && row.contact_id == case.under_taker_id),
            "the face inside the law must still be charged"
        );
    }

    /// **Provenance is PROVED at the append seam, not assumed.**
    ///
    /// [`GenericPerceptionError::MismatchedPriorLoadPartition`] had no test at
    /// all before this one: every arm below was asserted in code and never
    /// executed. Each arm perturbs exactly ONE authority away from the positive
    /// control and nothing else, so a passing arm cannot be a refusal of
    /// everything.
    #[test]
    fn an_append_refuses_a_prior_partition_that_is_not_this_plans() {
        let case = append_seam_case(2);
        let proof = case.fixture.proof();

        // Positive control: unperturbed, this exact call succeeds.
        assert!(
            append_with(
                &case,
                &proof,
                &case.prior,
                &case.contacts,
                case.prior_contact_count
            )
            .is_ok(),
            "the unperturbed case must succeed, or every arm below proves nothing"
        );

        // 1. A prior contact count past the end of the contact list.
        assert_eq!(
            append_with(
                &case,
                &proof,
                &case.prior,
                &case.contacts,
                case.contacts.len() + 1
            ),
            Err(GenericPerceptionError::MismatchedPriorLoadPartition),
            "a prior count past the end of the set names contacts that are not there"
        );

        // 2. A contact MOVED behind the partition — the prefix no longer
        //    hashes to what the prior partition was built over.
        let mut moved = case.contacts.clone();
        moved[0].point_mm[0] += 1.0;
        assert_eq!(
            append_with(&case, &proof, &case.prior, &moved, case.prior_contact_count),
            Err(GenericPerceptionError::MismatchedPriorLoadPartition),
            "a moved prior contact must not be accepted as the set the partition was built over"
        );

        // 3. A disclosed area shifted by ONE ULP. The check is a bit
        //    comparison for exactly this reason: an area that is not the
        //    surface's own is a partition of some other surface.
        let mut shifted = case.prior.clone();
        let area = shifted.ownerless[0].area_mm2;
        shifted.ownerless[0].area_mm2 = f64::from_bits(area.to_bits() + 1);
        assert_eq!(
            append_with(
                &case,
                &proof,
                &shifted,
                &case.contacts,
                case.prior_contact_count
            ),
            Err(GenericPerceptionError::MismatchedPriorLoadPartition),
            "a disclosure one ulp off this surface's own area must be refused"
        );

        // 4. A disclosed face that is not on this mesh at all.
        let mut out_of_range = case.prior.clone();
        let past_the_end = case.fixture.mesh.tris().len() as u32;
        out_of_range.ownerless.push(OwnerlessLoadFace {
            face_id: past_the_end,
            area_mm2: 1.0,
        });
        assert_eq!(
            append_with(
                &case,
                &proof,
                &out_of_range,
                &case.contacts,
                case.prior_contact_count
            ),
            Err(GenericPerceptionError::SpecializedFaceOutOfRange {
                face_id: past_the_end
            }),
            "a disclosure past the end of the mesh must be refused by name"
        );

        // 5. An appended contact owned by an island this plan does not have.
        let mut unknown_island = case.contacts.clone();
        unknown_island
            .last_mut()
            .expect("the case appends contacts")
            .owner = LoadOwner::GenericIsland(u32::MAX);
        assert_eq!(
            append_with(
                &case,
                &proof,
                &case.prior,
                &unknown_island,
                case.prior_contact_count
            ),
            Err(GenericPerceptionError::MismatchedPriorLoadPartition),
            "an appended contact owned by an unknown island has no readable eligibility"
        );

        // 6. A prior partition from another authority entirely.
        let mut foreign_authority = case.prior.clone();
        foreign_authority.authority.native ^= 1;
        assert_eq!(
            append_with(
                &case,
                &proof,
                &foreign_authority,
                &case.contacts,
                case.prior_contact_count
            ),
            Err(GenericPerceptionError::StaleAuthority),
            "a partition of another authority must never be extended onto this plan"
        );

        // 7. A proof of another mesh. Same guard, same place as the replay:
        //    a face range proved on one mesh must not index another.
        let foreign_mesh = make_toed_slab().unwrap();
        assert_eq!(
            append_with(
                &case,
                &VerifiedMeshFingerprint::compute(&foreign_mesh).unwrap(),
                &case.prior,
                &case.contacts,
                case.prior_contact_count
            ),
            Err(GenericPerceptionError::StaleAuthority),
            "a proof of a foreign mesh must be refused before any face is indexed"
        );
    }

    /// [`gather_eligible_contacts`] reads the transpose's OWN membership, in
    /// the scan's own `(squared distance, contact id)` order.
    ///
    /// It is reached only for a face above the single-contact maximum, so today
    /// it runs on the append path and — while the merge tournament's partition
    /// is still built `SingleCarrier` (the task #69 escalation) — never on the
    /// replay path. That is precisely why it needs a test of its own: nothing
    /// else exercises it.
    #[test]
    fn gather_eligible_contacts_reads_the_transposes_own_membership() {
        // Contact 0 is eligible for rows 0 and 2, contact 1 for row 2 only,
        // contact 2 for nothing.
        let transpose = LoadEligibilityTranspose {
            rows: vec![0, 2, 2],
            starts: vec![0, 2, 3, 3],
        };
        let mut contacts = test_contacts(&[7, 5, 9]);
        contacts[0].point_mm = [3.0, 0.0, 0.0];
        contacts[1].point_mm = [3.0, 0.0, 0.0];
        contacts[2].point_mm = [0.0, 0.0, 0.0];

        let mut out = vec![(f64::NAN, 0u64, 0usize)];
        gather_eligible_contacts(&transpose, &contacts, [0.0, 0.0, 0.0], 2, &mut out);
        assert_eq!(
            out,
            vec![(9.0, 5, 1), (9.0, 7, 0)],
            "both eligible contacts, tied on distance, broken by ASCENDING contact id"
        );

        gather_eligible_contacts(&transpose, &contacts, [0.0, 0.0, 0.0], 0, &mut out);
        assert_eq!(
            out,
            vec![(9.0, 7, 0)],
            "only contact 0 is eligible for row 0"
        );

        gather_eligible_contacts(&transpose, &contacts, [0.0, 0.0, 0.0], 1, &mut out);
        assert!(
            out.is_empty(),
            "no contact is eligible for row 1, and the output must be CLEARED"
        );
    }

    /// A non-finite eligibility area is disclosed, under BOTH maxima.
    ///
    /// The predecessor charged it whole to the nearest contact, which put a
    /// non-finite load into the ledger for the fold to reject downstream. It is
    /// almost certainly unreachable — `build_load_eligibility` reads areas off
    /// a validated surface — but it IS a behaviour change at the replay seam,
    /// so it is named and measured here rather than left to be discovered.
    #[test]
    fn a_non_finite_area_is_disclosed_rather_than_charged_under_either_maximum() {
        let maximum = registered_max_carriable_area_mm2().expect("registry read");
        let contacts = test_contacts(&[0]);
        let takers = vec![(0.0_f64, 0_u64, 0_usize)];
        for area_mm2 in [f64::NAN, f64::INFINITY] {
            for maximum in [
                CarriableMaximum::AreaLaw(maximum),
                CarriableMaximum::SingleCarrier,
            ] {
                let mut rows = Vec::new();
                let mut ownerless = Vec::new();
                charge_face_from_takers(
                    3,
                    area_mm2,
                    &takers,
                    &contacts,
                    maximum,
                    &mut rows,
                    &mut ownerless,
                );
                assert!(
                    rows.is_empty(),
                    "a non-finite area under {maximum:?} must be charged to nobody"
                );
                assert_eq!(ownerless.len(), 1);
                assert_eq!(ownerless[0].face_id, 3);
                assert_eq!(ownerless[0].area_mm2.to_bits(), area_mm2.to_bits());
            }
        }
    }

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
            ownerless: Vec::new(),
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
            // A zero-carried-area contact takes the clamp minimum: 0.05 mm
            // RADIUS (0.1 mm DIAMETER) since the owner ruling of 2026-08-19,
            // re-pinned here from 0.2 mm radius (0.4 mm diameter).
            vec![0.05, 0.05, 0.05]
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

        // FU-32 phase J1.  The hoisted form must be exactly as strict as the
        // recomputing form, and its row arm must bite: a hoisted fingerprint
        // that does not belong to this partition attests nothing.
        let honest = FinalizedContactLoads::hoist_rows_fingerprint(&partition);
        assert!(loads
            .validate_binding_hoisted(&partition, &contact_ids, honest)
            .is_ok());
        assert_eq!(
            loads.validate_binding_hoisted(&partition, &contact_ids, honest ^ 1),
            Err(GenericPerceptionError::StaleAuthority),
            "the hoisted row arm stopped comparing"
        );
        assert_eq!(
            loads.validate_binding_hoisted(
                &partition,
                &contact_ids,
                FinalizedContactLoads::hoist_rows_fingerprint(&stale),
            ),
            Err(GenericPerceptionError::StaleAuthority),
            "a foreign partition's hoisted row fingerprint was accepted"
        );
        assert_eq!(
            loads.validate_binding_hoisted(&partition, &reordered_ids, honest),
            Err(GenericPerceptionError::StaleAuthority),
            "the hoist must not weaken the contact-id arm"
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

    /// **The 2026-08-19 tip minimum binds ONLY where the carried area is
    /// tiny, and weakens no refusal anywhere else.**
    ///
    /// This is the revert guard for the owner's ruling lowering
    /// `TIP_RADIUS_DEFAULT_MM` from 0.2 mm radius (0.4 mm diameter) to
    /// 0.05 mm radius (0.1 mm diameter).  It has two halves that fail in
    /// opposite directions, which is the whole point:
    ///
    /// - The BINDING half asserts the clamp minimum is 0.05 mm radius and
    ///   that a zero-area contact takes it.  On `develop` (0.2 mm radius)
    ///   this half is RED.  That is the sabotage direction: reverting the
    ///   constant re-breaks it.
    /// - The INVARIANCE half asserts that every LARGE carried area is sized
    ///   by the load law and NOT by the clamp, against radii computed from
    ///   the law rather than from the constant.  This half is GREEN both
    ///   before and after the ruling, and it is what proves the ruling did
    ///   not weaken a single refusal.  If a future edit lets the minimum
    ///   reach a large area, this half goes red instead.
    ///
    /// ORDERING CAVEAT, so the claim above is not read as more than it is:
    /// in the RED state the binding half aborts this test at its first
    /// assertion, so the invariance half never executes there.  Its "green
    /// at both values" was MEASURED by reverting the constant in isolation
    /// and running it, not observed during a revert of this test.  It also
    /// holds STRUCTURALLY: `violates_safety_factor` never reads
    /// `min_radius_mm` at all -- the constant's only behavioural reader is
    /// the `clamp` in `tip_radius_mm` -- so no value of the minimum can
    /// move a refusal on an area the minimum does not reach.
    ///
    /// UNITS: radii here are RADII in mm.  The owner and ChituBox speak
    /// DIAMETER; every radius below is half the diameter he would quote.
    #[test]
    fn tip_minimum_binds_only_tiny_areas_and_leaves_every_large_area_refusal_identical() {
        let policy = FrozenTipLoadPolicy::from_placeholders(m0_constants().unwrap().placeholders());

        // BINDING half -- RED on develop, where the minimum is 0.2 mm radius.
        assert_eq!(
            policy.min_radius_mm, 0.05,
            "the tip clamp minimum is 0.05 mm RADIUS (0.1 mm DIAMETER) by the owner ruling of \
             2026-08-19; it was 0.2 mm radius (0.4 mm diameter)"
        );
        assert_eq!(
            policy.tip_radius_mm(0.0).unwrap().get(),
            0.05,
            "a contact carrying no area takes the clamp minimum"
        );
        // The maximum is deliberately untouched by that ruling.
        assert_eq!(
            policy.max_radius_mm, 1.0,
            "the tip clamp maximum stays 1.0 mm RADIUS (2.0 mm DIAMETER)"
        );

        // INVARIANCE half -- GREEN on develop AND here.  `x` is the area
        // ratio the engine implies; the law says r = sqrt(area / (x * pi)).
        let x = policy.implied_area_ratio();
        let law_radius_mm = |area_mm2: f64| (area_mm2 / (x * std::f64::consts::PI)).sqrt();

        // These areas are all ABOVE the largest area the OLD 0.2 mm-radius
        // minimum could reach (x * pi * 0.2^2 = 1.50796 mm2), so the law --
        // not either clamp -- sized them before this ruling and sizes them
        // now.  Their radii are therefore bit-identical pre- and
        // post-ruling, which is exactly what "no refusal was weakened"
        // means.  They are also below the area at which the untouched
        // 1.0 mm-radius maximum starts clamping.
        let old_minimum_max_area_mm2 = x * std::f64::consts::PI * 0.2_f64.powi(2);
        for area_mm2 in [1.6_f64, 2.0, 5.0, 12.0, 30.0, 37.0] {
            assert!(
                area_mm2 > old_minimum_max_area_mm2,
                "this group must sit above the OLD minimum's reach, or it proves nothing about \
                 invariance"
            );
            let sized = policy.tip_radius_mm(area_mm2).unwrap().get();
            assert!(
                (sized - law_radius_mm(area_mm2)).abs() < 1.0e-12,
                "a {area_mm2} mm2 carried area must be sized by the load law, not by either \
                 clamp: got {sized} mm radius"
            );
            assert!(
                sized > policy.min_radius_mm,
                "the clamp minimum must not reach a {area_mm2} mm2 carried area"
            );
            // The refusal predicate is unchanged on large areas: the
            // law-sized tip is accepted, and a tip one ULP thinner is not.
            assert!(
                !policy.violates_safety_factor(area_mm2, PositiveFinite::new(sized).unwrap()),
                "the law-sized tip for {area_mm2} mm2 must still be accepted"
            );
            // One ULP under is deliberately still accepted -- the predicate
            // carries a 1e-6 safety-factor tolerance (see
            // `shared_load_gate_pins_the_live_one_e_minus_six_tolerance`).
            // A tip meaningfully under the law-required radius is not.
            let clearly_thinner = sized * 0.99;
            assert!(
                policy.violates_safety_factor(
                    area_mm2,
                    PositiveFinite::new(clearly_thinner).unwrap()
                ),
                "a tip 1% under the law-required radius for {area_mm2} mm2 must still be \
                 refused -- this ruling lowered a minimum, it did not move the law"
            );
            // And the new minimum itself is nowhere near sufficient here:
            // a large area on a 0.05 mm tip is still refused outright.
            assert!(
                policy.violates_safety_factor(
                    area_mm2,
                    PositiveFinite::new(policy.min_radius_mm).unwrap()
                ),
                "a {area_mm2} mm2 carried area on a minimum-radius tip must still be refused"
            );
        }

        // THE BAND THAT MOVED, stated explicitly so the change is legible
        // rather than implied.  Between the new minimum's reach
        // (0.09425 mm2) and the old one's (1.50796 mm2), a tip used to be
        // clamped up to 0.2 mm radius against its own physics; now the law
        // sizes it.  This is the ruling's entire effect on output.
        let new_minimum_max_area_mm2 = x * std::f64::consts::PI * policy.min_radius_mm.powi(2);
        for area_mm2 in [0.1_f64, 0.25, 0.5, 1.0, 1.5] {
            assert!(area_mm2 > new_minimum_max_area_mm2 && area_mm2 < old_minimum_max_area_mm2);
            let sized = policy.tip_radius_mm(area_mm2).unwrap().get();
            assert!(
                (sized - law_radius_mm(area_mm2)).abs() < 1.0e-12,
                "a {area_mm2} mm2 carried area is now sized by the law, not clamped to 0.2 mm"
            );
            assert!(
                sized < 0.2,
                "a {area_mm2} mm2 carried area used to be over-sized to 0.2 mm radius; it is now \
                 {sized} mm radius"
            );
            // Still legal: the law-sized tip carries this area safely.
            assert!(
                !policy.violates_safety_factor(area_mm2, PositiveFinite::new(sized).unwrap()),
                "the law-sized tip must be accepted -- thinner here is correct, not permissive"
            );
        }
    }

    /// **The smallest legal tip is 0.05 mm RADIUS (0.1 mm DIAMETER), and the
    /// most it may legally carry is `X * pi * 0.05^2` = 0.09425 mm2.**
    ///
    /// That area is the whole span over which the 2026-08-19 minimum can
    /// bind at all.  Above it the load law sizes the tip; the old 0.2 mm
    /// radius (0.4 mm diameter) minimum bound everything under
    /// `X * pi * 0.2^2` = 1.50796 mm2, so the ruling narrowed the
    /// over-sizing band by exactly 16x (the square of the 4x radius drop),
    /// and changed nothing else.
    #[test]
    fn smallest_legal_tip_is_0_05_mm_radius_0_1_mm_diameter_carrying_at_most_0_09425_mm2() {
        let policy = FrozenTipLoadPolicy::from_placeholders(m0_constants().unwrap().placeholders());
        let x = policy.implied_area_ratio();
        assert_eq!(x, 12.0, "the engine's implied area ratio X");

        let smallest_tip_radius_mm = policy.min_radius_mm;
        assert_eq!(smallest_tip_radius_mm, 0.05);
        assert_eq!(
            smallest_tip_radius_mm * 2.0,
            0.1,
            "stated as the owner states it: 0.1 mm DIAMETER"
        );

        // The area at which the law asks for exactly the minimum radius.
        let max_area_mm2 = x * std::f64::consts::PI * smallest_tip_radius_mm.powi(2);
        assert!(
            (max_area_mm2 - 0.094_247_779_607_693_8).abs() < 1.0e-15,
            "the smallest legal tip carries at most {max_area_mm2} mm2"
        );

        // At that area the law and the clamp agree exactly.
        assert!(
            (policy.tip_radius_mm(max_area_mm2).unwrap().get() - smallest_tip_radius_mm).abs()
                < 1.0e-15
        );
        // At that area the minimum-radius tip is legal ...
        assert!(!policy.violates_safety_factor(
            max_area_mm2,
            PositiveFinite::new(smallest_tip_radius_mm).unwrap()
        ));
        // ... and just above it, it is not: the law takes over and refuses.
        let just_over_mm2 = max_area_mm2 * 1.01;
        assert!(policy.violates_safety_factor(
            just_over_mm2,
            PositiveFinite::new(smallest_tip_radius_mm).unwrap()
        ));
        assert!(policy.tip_radius_mm(just_over_mm2).unwrap().get() > smallest_tip_radius_mm);

        // The band this ruling narrowed, stated in both radii and areas.
        let old_minimum_radius_mm: f64 = 0.2;
        let old_max_area_mm2 = x * std::f64::consts::PI * old_minimum_radius_mm.powi(2);
        assert!((old_max_area_mm2 - 1.507_964_473_723_100_5).abs() < 1.0e-12);
        assert!((old_max_area_mm2 / max_area_mm2 - 16.0).abs() < 1.0e-12);
    }

    /// **The scar bound is for GROWTH. The LOAD LAW is untouched by it.**
    ///
    /// `TIP_RADIUS_MAX_GROWN_SCAR_MM` = 0.400 mm RADIUS (0.800 mm DIAMETER) is
    /// Craig's field ceiling on the scar a grow-for-ATTACHMENT rescue may leave.
    /// It is emphatically not a clamp on the tip-sizing law: a genuinely heavy
    /// area needs a genuinely wide tip, and refusing to build one would be
    /// refusing physics on the strength of a preference about scars.
    ///
    /// So this pins the counterpart claim to the growth guards in
    /// `designed_embed.rs`: hand the registered load law an area that DEMANDS
    /// more than the scar bound and check it gets what it demands. The area is
    /// derived from the registered constants — `pi r^2 = area / X`, inverted at
    /// the bound — never hand-typed, so a constants move carries the fixture
    /// with it.
    #[test]
    fn the_load_law_may_size_a_tip_above_the_grown_scar_maximum_and_is_not_clamped_to_it() {
        let constants = m0_constants().unwrap();
        let placeholders = constants.placeholders();
        let policy = FrozenTipLoadPolicy::from_placeholders(placeholders);
        let scar_bound_radius_mm = *constants.evidence().tip_radius_max_grown_scar_mm().value();
        let legacy_max_radius_mm = placeholders.tip_radius_max_legacy_mm().diagnostic().get();
        assert!(
            scar_bound_radius_mm < legacy_max_radius_mm,
            "this test only says something while the scar maximum is BELOW the \
             load law's own clamp maximum: {scar_bound_radius_mm} mm radius \
             against {legacy_max_radius_mm} mm radius"
        );

        // The area at which the law asks for exactly the scar bound, inverted
        // from the registered chain rather than typed: X = tensile / (peel *
        // safety), and pi r^2 = area / X.
        let x = placeholders.tensile_green_mpa().diagnostic().get() * 1.0e6
            / (placeholders.peel_pressure_pa().diagnostic().get()
                * placeholders.tip_safety_factor().diagnostic().get());
        let area_at_bound_mm2 = x * std::f64::consts::PI * scar_bound_radius_mm.powi(2);

        // A HEAVY area — four times that, so the law wants twice the radius.
        let heavy_mm2 = 4.0 * area_at_bound_mm2;
        let sized = policy.tip_radius_mm(heavy_mm2).unwrap().get();
        assert!(
            (sized - 2.0 * scar_bound_radius_mm).abs() < 1.0e-9,
            "the area law must size a {heavy_mm2} mm² contact at \
             {} mm radius ({} mm diameter) — four times the area is twice the \
             radius — and NOT be clamped to the {scar_bound_radius_mm} mm radius \
             ({} mm diameter) grown-scar maximum; got {sized} mm radius",
            2.0 * scar_bound_radius_mm,
            4.0 * scar_bound_radius_mm,
            scar_bound_radius_mm * 2.0
        );
        assert!(
            sized > scar_bound_radius_mm,
            "a load-sized tip above the grown-scar maximum is legal; the bound \
             applies to growth-for-attachment only"
        );
        // And the law's own clamp is still the only ceiling it has.
        let crushing_mm2 = 1.0e6 * area_at_bound_mm2;
        assert!(
            (policy.tip_radius_mm(crushing_mm2).unwrap().get() - legacy_max_radius_mm).abs()
                < 1.0e-12,
            "the sizing law's ceiling is TIP_RADIUS_MAX_LEGACY_MM = \
             {legacy_max_radius_mm} mm radius ({} mm diameter), unmoved by the \
             grown-scar maximum",
            legacy_max_radius_mm * 2.0
        );
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

    /// The reach criterion reverses the old area predicate over a cavity, and
    /// it does so deliberately.
    ///
    /// `carried_outer_area` credited the whole positive outer loop to a point
    /// standing over a hole, so a low point suspended over a cavity read as
    /// well carried. Physically there is nothing under it: the material that
    /// holds it is the cavity wall, and the distance to that wall is the
    /// cantilever it has to bridge. Reach measures that distance.
    #[test]
    fn a_low_point_over_a_cavity_reaches_to_the_cavity_wall() {
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
        // The cavity centre is 1.0 mm from every wall, 14x the 0.0707 mm
        // allowance.
        assert!((point_reach_mm(std::slice::from_ref(&solid), [2.0, 2.0]) - 1.0).abs() < 1.0e-12);
        // A point on the solid annulus is carried outright.
        assert_eq!(point_reach_mm(&[solid], [0.5, 0.5]), 0.0);
    }

    /// A 10 x 10 mm carrying solid one layer down, with its lower edge on
    /// `y = 0`, so a region point at `y = -d` has reach exactly `d` mm.
    fn reach_fixture_probe() -> Vec<SliceSolid> {
        vec![test_slice_solid(vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 10.0],
        ])]
    }

    /// A minimum whose region has one point inside the carrying solid and one
    /// point hanging `overhang_mm` past its `y = 0` edge. A negative
    /// `overhang_mm` puts that second point inside the solid too, which is the
    /// zero-reach dimple case.
    fn reach_fixture_minimum(z_mm: f64, overhang_mm: f64) -> LocalZMinimum {
        LocalZMinimum {
            position_mm: [5.0, 5.0, z_mm],
            normal: [0.0, 0.0, -1.0],
            region_points_mm: vec![[5.0, 5.0, z_mm], [5.0, -overhang_mm, z_mm]],
        }
    }

    /// NOT a corpus-parity check. The three reach values below are literals
    /// copied from the instrumented `b8edcb8` dumps (derivation's 57-minima
    /// table,
    /// `docs/evidence/derivations/low-point-reach-2026-08-19.md`
    /// sec 3), not re-derived from a mesh here, so the test is tautological
    /// on those values by construction. What it actually pins: (a) the
    /// comparison direction at both ends -- `>` the allowance demands a
    /// support, `<=` is adequate; (b) the two non-trivial margins, 1.4756x
    /// and 0.2827x the allowance; and (c) that `region_reach_mm` takes the
    /// max over every sampled region point rather than only the minimum's
    /// representative point -- `reach_fixture_minimum`'s representative
    /// point always sits inside the carrying solid, so a reach read off it
    /// alone would read zero in every case here, and the second region point
    /// is what the max must pick up. The real corpus-parity evidence is the
    /// byte-identical emissions recorded in the derivation's provenance
    /// appendix, not this fixture.
    ///
    /// MEASURED reach values, read off the instrumented `b8edcb8` dumps:
    ///
    /// - Crawler Zombie minimum 13 -- reach 0.104343 mm. 148% of the
    ///   0.0707 mm allowance, so it DEMANDS a support.
    /// - Thorny Vegepygmy minimum 17 -- reach 0.019988 mm. 28% of the
    ///   allowance, so it is adequate.
    /// - the other 55 -- reach 0.000000 mm exactly, every sampled region point
    ///   inside the solid below. Adequate with full margin.
    ///
    /// These are the only three reach values in the whole corpus. Reverting to
    /// any area-shaped yardstick, or hardcoding an allowance, changes at least
    /// one of the three verdicts or the margins asserted with them.
    #[test]
    fn the_corpus_reach_table_reproduces_at_the_registered_allowance() {
        let constants = plinth_core::m0_constants().expect("M0 constants");
        let allowance_mm = registry_low_point_reach_allowance_mm(constants);
        let probe = reach_fixture_probe();

        // Crawler Zombie minimum 13, z = 0.855195 mm.
        let crawler_zombie = region_reach_mm(&probe, &reach_fixture_minimum(0.855195, 0.104343));
        assert!((crawler_zombie - 0.104343).abs() < 1.0e-12);
        assert!(
            crawler_zombie > allowance_mm,
            "Crawler Zombie minimum 13 reaches {crawler_zombie} mm against a {allowance_mm} mm \
             allowance and must demand a support"
        );
        assert!(minimum_demands_support(
            false,
            crawler_zombie <= allowance_mm
        ));
        // 148% of the allowance. The margin is asserted, not just the verdict:
        // the corpus alone cannot tell the conservative coefficient 6 from the
        // optimistic 3 (this case demands a support at either), so the margin
        // is what pins Craig's conservative-end ruling here.
        assert!((crawler_zombie / allowance_mm - 1.4756).abs() < 1.0e-3);

        // Thorny Vegepygmy minimum 17, z = 2.355105 mm.
        let thorny_vegepygmy = region_reach_mm(&probe, &reach_fixture_minimum(2.355105, 0.019988));
        assert!((thorny_vegepygmy - 0.019988).abs() < 1.0e-12);
        assert!(
            thorny_vegepygmy <= allowance_mm,
            "Thorny Vegepygmy minimum 17 reaches {thorny_vegepygmy} mm against a {allowance_mm} mm \
             allowance and is adequately carried"
        );
        assert!(!minimum_demands_support(
            false,
            thorny_vegepygmy <= allowance_mm
        ));
        // 28% of the allowance.
        assert!((thorny_vegepygmy / allowance_mm - 0.2827).abs() < 1.0e-3);

        // The other 55: a dimple wholly inside the material below.
        let dimple = region_reach_mm(&probe, &reach_fixture_minimum(6.421297, -0.5));
        assert_eq!(dimple, 0.0);
        assert!(!minimum_demands_support(false, dimple <= allowance_mm));
    }

    /// A low point with nothing at all one layer down is never adequate, at
    /// any finite allowance.
    #[test]
    fn a_low_point_over_empty_space_always_demands_a_support() {
        let constants = plinth_core::m0_constants().expect("M0 constants");
        let allowance_mm = registry_low_point_reach_allowance_mm(constants);
        let reach_mm = region_reach_mm(&[], &reach_fixture_minimum(1.0, 0.0));
        assert!(reach_mm.is_infinite());
        assert!(minimum_demands_support(false, reach_mm <= allowance_mm));
    }

    /// The birth filter is untouched by the reach criterion: a birth demands a
    /// support whatever the reach measurement says, because there is nothing
    /// beneath a birth for a reach to be measured against.
    #[test]
    fn a_birth_demands_a_support_however_the_reach_verdict_falls() {
        assert!(minimum_demands_support(true, true));
        assert!(minimum_demands_support(true, false));
        assert!(!minimum_demands_support(false, true));
        assert!(minimum_demands_support(false, false));
    }

    /// Sabotage: the allowance must be a live function of two registered
    /// constants, not a literal.
    ///
    /// Mutating a test-local copy of either registered input moves the
    /// allowance by the amount the derivation predicts -- `sqrt` in the area
    /// law, linear in the layer height. A hardcoded 0.0707 mm survives
    /// neither check.
    #[test]
    fn the_reach_allowance_is_read_from_the_registry_not_hardcoded() {
        let constants = plinth_core::m0_constants().expect("M0 constants");
        let evidence = constants.evidence();
        let x_allowed = *evidence.x_allowed().value();
        let layer_height_mm = *evidence.layer_height_default_mm().value();

        // The production reader composes exactly these two registered values
        // and nothing else.
        let registered = registry_low_point_reach_allowance_mm(constants);
        assert_eq!(
            registered,
            low_point_reach_allowance_mm(x_allowed, layer_height_mm)
        );

        // Craig's conservative-end ruling, 2026-08-19: coefficient 6, the peel
        // front concentrated at the free edge. sqrt(12/6) * 0.050 mm.
        assert!((PEEL_FRONT_ROOT_MOMENT_COEFFICIENT - 6.0).abs() < 1.0e-12);
        assert!((x_allowed - 12.0).abs() < 1.0e-12);
        assert!((layer_height_mm - 0.050).abs() < 1.0e-12);
        assert!((registered - 0.070_710_678_118_654_75).abs() < 1.0e-15);

        // Sabotage the area law: four times X_ALLOWED must double the reach.
        let sabotaged_law = low_point_reach_allowance_mm(4.0 * x_allowed, layer_height_mm);
        assert!((sabotaged_law / registered - 2.0).abs() < 1.0e-12);

        // Sabotage the layer height: a doubled layer doubles the reach.
        let sabotaged_layer = low_point_reach_allowance_mm(x_allowed, 2.0 * layer_height_mm);
        assert!((sabotaged_layer / registered - 2.0).abs() < 1.0e-12);
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

    /// Run the FU-10 phase-C stitch -> orient -> normalize path over one
    /// hand-built plane of kernel intersections.
    ///
    /// The batched entry point is the only one there is (that is the point of
    /// the cutover), so a single-layer test wraps its events in a one-plane
    /// batch rather than reaching for a per-layer back door.
    fn normalized_test_layer(
        space: SliceStitchSpace,
        events: Vec<geom_kernel::PlaneIntersection>,
        normals: &[[f64; 3]],
    ) -> Result<Vec<SliceSolid>, GenericPerceptionError> {
        let batch = geom_kernel::PlaneSliceBatch {
            plane_offsets: vec![0, events.len()],
            intersections: events,
            effective_threads: 1,
            participating_threads: 1,
            candidate_work: 0,
        };
        normalized_slice_layers(
            space,
            &batch,
            normals,
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
        )
        .map(|mut layers| layers.remove(0))
    }

    /// V3 disposition (FU-10 phase C, re-aimed by task #32): this test used to
    /// call the legacy `stitch_layer`, then the normalized path, and it pinned
    /// a DEDUPLICATION -- two owners of one on-plane edge collapsing into a
    /// single undirected edge.
    ///
    /// That dedup is gone, and with it this fixture's premise. The frozen
    /// kernel does retain duplicate positive-edge owners (the design's D4
    /// case), but for a consistently wound mesh the two owners can never vote
    /// the same way. Let the shared on-plane edge run `P -> Q`, let each owner
    /// keep its third vertex above the plane (the only way both emit), and let
    /// `w` and `v` be those two vertices' offsets. Consistent winding makes
    /// the faces `(P, Q, R)` and `(Q, P, S)`, so their in-plane derived
    /// directions are `w.z * (Q - P)` and `-v.z * (Q - P)` with both `z`
    /// components positive: exactly opposite, always. That is the measured
    /// tangent-keel case M2, covered by
    /// [`a_doubly_emitted_tangent_keel_edge_sections_and_leaves_no_spur`].
    ///
    /// So a duplicate whose owners agree is not a real mesh's D4 case -- it is
    /// an inconsistently wound one, and it is now refused by name instead of
    /// being averaged away. Same fixture, opposite verdict, stated mechanism.
    #[test]
    fn duplicate_owners_that_vote_alike_are_an_inconsistent_winding_and_are_refused() {
        use geom_kernel::{PlaneIntersection, PlaneIntersectionKind};
        let edge = |face_id, a: [f64; 2], b: [f64; 2]| PlaneIntersection {
            face_id,
            kind: PlaneIntersectionKind::Basic,
            segment: [[a[0], a[1], 0.5], [b[0], b[1], 0.5]],
        };
        let normals = [
            [0.0, -1.0, 0.0], // y = 0 wall, outward -y
            [1.0, 0.0, 0.0],  // x = 2 wall, outward +x
            [0.0, 1.0, 0.0],  // y = 2 wall, outward +y
            [-1.0, 0.0, 0.0], // x = 0 wall, outward -x
            [0.0, -1.0, 0.0], // a second owner of the y = 0 wall, voting alike
        ];
        let events = vec![
            edge(0, [0.0, 0.0], [2.0, 0.0]),
            edge(1, [2.0, 0.0], [2.0, 2.0]),
            edge(2, [2.0, 2.0], [0.0, 2.0]),
            edge(3, [0.0, 2.0], [0.0, 0.0]),
            edge(4, [2.0, 0.0], [0.0, 0.0]),
        ];
        // (0,0) leaves twice and is entered once: unbalanced, so refused.
        assert_eq!(
            orientation::orient_layer(SliceStitchSite::PrintLayer(7), &events, &normals),
            Err(GenericPerceptionError::OpenSliceSeam { layer: 7 })
        );
        // Drop the duplicate and the same four walls stitch as they always
        // did, values unchanged, through the normalized path.
        let loops = normalized_test_layer(
            SliceStitchSpace::PrintLayer,
            events[..4].to_vec(),
            &normals,
        )
        .unwrap();
        assert_eq!(loops.len(), 1);
        assert_eq!(
            loops[0].outer().points_xy_mm(),
            &[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]
        );
        assert_eq!(loops[0].area_mm2(), 4.0);
        assert!(loops[0].holes().is_empty());
    }

    /// FU-10 phase A: the `z_hat x n` rule must impose counter-clockwise
    /// walk order on an outer boundary regardless of the kernel's arbitrary
    /// segment endpoint order, and regardless of the frozen intersection
    /// kind.
    #[test]
    fn derived_orientation_walks_an_outer_loop_counter_clockwise() {
        use geom_kernel::{PlaneIntersection, PlaneIntersectionKind};
        let wall = |face_id, kind, a: [f64; 2], b: [f64; 2]| PlaneIntersection {
            face_id,
            kind,
            segment: [[a[0], a[1], 0.5], [b[0], b[1], 0.5]],
        };
        let normals = [
            [0.0, -1.0, 0.0], // y = 0 wall, outward -y
            [1.0, 0.0, 0.0],  // x = 2 wall, outward +x
            [0.0, 1.0, 0.0],  // y = 2 wall, outward +y
            [-1.0, 0.0, 0.0], // x = 0 wall, outward -x
        ];
        // Every segment is listed AGAINST its derived direction to prove
        // endpoint order is an artifact the derivation overrides.
        let events = vec![
            wall(0, PlaneIntersectionKind::Basic, [2.0, 0.0], [0.0, 0.0]),
            wall(1, PlaneIntersectionKind::Vertex, [2.0, 2.0], [2.0, 0.0]),
            wall(2, PlaneIntersectionKind::Basic, [0.0, 2.0], [2.0, 2.0]),
            wall(
                3,
                PlaneIntersectionKind::PositiveEdge,
                [0.0, 0.0],
                [0.0, 2.0],
            ),
        ];
        let layer =
            orientation::orient_layer(SliceStitchSite::PrintLayer(0), &events, &normals).unwrap();
        assert_eq!(layer.degraded, None);
        assert_eq!(layer.loops.len(), 1);
        assert_eq!(
            layer.loops[0].points_xy_mm,
            vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]
        );
        assert_eq!(layer.loops[0].signed_area_mm2, 4.0);
    }

    /// FU-10 phase A: a hole boundary's outward-from-solid normals point
    /// INTO the void, so the same `z_hat x n` rule walks it clockwise --
    /// negative area, the nonzero-rule hole verdict -- with no parity probe
    /// involved anywhere.
    #[test]
    fn derived_orientation_walks_a_hole_loop_clockwise() {
        use geom_kernel::{PlaneIntersection, PlaneIntersectionKind};
        let wall = |face_id, a: [f64; 2], b: [f64; 2]| PlaneIntersection {
            face_id,
            kind: PlaneIntersectionKind::Basic,
            segment: [[a[0], a[1], 0.5], [b[0], b[1], 0.5]],
        };
        let normals = [
            // Outer square (0,0)-(4,4): outward normals.
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0, 0.0, 0.0],
            // Inner square hole (1,1)-(3,3): normals leave the solid annulus,
            // pointing toward the hole's center.
            [1.0, 0.0, 0.0],  // x = 1 wall, solid at x < 1
            [0.0, -1.0, 0.0], // y = 3 wall, solid at y > 3
            [-1.0, 0.0, 0.0], // x = 3 wall, solid at x > 3
            [0.0, 1.0, 0.0],  // y = 1 wall, solid at y < 1
        ];
        let events = vec![
            wall(0, [0.0, 0.0], [4.0, 0.0]),
            wall(1, [4.0, 0.0], [4.0, 4.0]),
            wall(2, [4.0, 4.0], [0.0, 4.0]),
            wall(3, [0.0, 4.0], [0.0, 0.0]),
            wall(4, [1.0, 1.0], [1.0, 3.0]),
            wall(5, [1.0, 3.0], [3.0, 3.0]),
            wall(6, [3.0, 3.0], [3.0, 1.0]),
            wall(7, [3.0, 1.0], [1.0, 1.0]),
        ];
        let layer =
            orientation::orient_layer(SliceStitchSite::PrintLayer(0), &events, &normals).unwrap();
        assert_eq!(layer.degraded, None);
        assert_eq!(layer.loops.len(), 2);
        assert_eq!(layer.loops[0].signed_area_mm2, 16.0);
        assert_eq!(layer.loops[1].signed_area_mm2, -4.0);
        assert_eq!(
            layer.loops[1].points_xy_mm,
            vec![[1.0, 1.0], [1.0, 3.0], [3.0, 3.0], [3.0, 1.0]]
        );
    }

    /// Task #32: locally inconsistent winding (here one flipped face normal)
    /// used to degrade to the undirected walk and carry on under even-odd.
    /// It is now what the balance guard exists to catch -- one face pointing
    /// the wrong way makes two vertices unbalanced -- so the layer is refused
    /// by name instead of filled with a guess. This is the synthetic twin of
    /// the flipped-winding mesh sabotage.
    #[test]
    fn one_flipped_face_normal_is_refused_by_the_balance_guard() {
        use geom_kernel::{PlaneIntersection, PlaneIntersectionKind};
        let wall = |face_id, a: [f64; 2], b: [f64; 2]| PlaneIntersection {
            face_id,
            kind: PlaneIntersectionKind::Basic,
            segment: [[a[0], a[1], 0.5], [b[0], b[1], 0.5]],
        };
        let normals = [
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0],
            // Flipped: should be outward +y for the y = 2 wall.
            [0.0, -1.0, 0.0],
            [-1.0, 0.0, 0.0],
        ];
        let events = vec![
            wall(0, [0.0, 0.0], [2.0, 0.0]),
            wall(1, [2.0, 0.0], [2.0, 2.0]),
            wall(2, [2.0, 2.0], [0.0, 2.0]),
            wall(3, [0.0, 2.0], [0.0, 0.0]),
        ];
        assert_eq!(
            orientation::orient_layer(SliceStitchSite::PrintLayer(0), &events, &normals),
            Err(GenericPerceptionError::OpenSliceSeam { layer: 0 })
        );
        // Restore that one normal and the same four walls stitch cleanly, so
        // the refusal is the flip and nothing else.
        let repaired = [
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0, 0.0, 0.0],
        ];
        let layer =
            orientation::orient_layer(SliceStitchSite::PrintLayer(0), &events, &repaired).unwrap();
        assert_eq!(layer.degraded, None);
        assert_eq!(layer.loops[0].signed_area_mm2, 4.0);
    }

    /// FU-10 phase A degraded path: a near-horizontal face has no reliable
    /// in-plane direction, so its segment abstains rather than guesses, and
    /// a segment with no usable vote degrades the layer.
    #[test]
    fn an_ambiguous_direction_vote_degrades_rather_than_guesses() {
        use geom_kernel::{PlaneIntersection, PlaneIntersectionKind};
        let wall = |face_id, a: [f64; 2], b: [f64; 2]| PlaneIntersection {
            face_id,
            kind: PlaneIntersectionKind::Basic,
            segment: [[a[0], a[1], 0.5], [b[0], b[1], 0.5]],
        };
        let normals = [
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            // Degenerate for a wall: `z_hat x n` vanishes, the vote abstains.
            [0.0, 0.0, 1.0],
        ];
        let events = vec![
            wall(0, [0.0, 0.0], [2.0, 0.0]),
            wall(1, [2.0, 0.0], [2.0, 2.0]),
            wall(2, [2.0, 2.0], [0.0, 2.0]),
            wall(3, [0.0, 2.0], [0.0, 0.0]),
        ];
        let layer =
            orientation::orient_layer(SliceStitchSite::PrintLayer(0), &events, &normals).unwrap();
        assert_eq!(
            layer.degraded,
            Some(orientation::WindingDegradation {
                ambiguous_segments: 1,
            })
        );
        assert_eq!(layer.loops.len(), 1);
    }

    /// FU-10 phase A: the oriented stitch still raises a seam error for a
    /// genuinely open seam, naming the caller's own index space.
    #[test]
    fn oriented_stitch_still_raises_the_site_seam_error_for_open_seams() {
        use geom_kernel::{PlaneIntersection, PlaneIntersectionKind};
        let wall = |face_id, a: [f64; 2], b: [f64; 2]| PlaneIntersection {
            face_id,
            kind: PlaneIntersectionKind::Basic,
            segment: [[a[0], a[1], 0.5], [b[0], b[1], 0.5]],
        };
        let normals = [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0]];
        let events = vec![
            wall(0, [0.0, 0.0], [1.0, 0.0]),
            wall(1, [1.0, 0.0], [1.0, 1.0]),
        ];
        assert_eq!(
            orientation::orient_layer(SliceStitchSite::PrintLayer(3), &events, &normals),
            Err(GenericPerceptionError::OpenSliceSeam { layer: 3 })
        );
        assert_eq!(
            orientation::orient_layer(SliceStitchSite::AdequacyProbe(9), &events, &normals),
            Err(GenericPerceptionError::OpenAdequacyProbeSeam { probe_row: 9 })
        );
    }

    /// FU-10 phase A: a face id outside the normal table is a typed error,
    /// not a guess and not a panic.
    #[test]
    fn orientation_refuses_a_face_outside_the_normal_table() {
        use geom_kernel::{PlaneIntersection, PlaneIntersectionKind};
        let events = vec![PlaneIntersection {
            face_id: 5,
            kind: PlaneIntersectionKind::Basic,
            segment: [[0.0, 0.0, 0.5], [1.0, 0.0, 0.5]],
        }];
        let normals = [[0.0, -1.0, 0.0]];
        assert_eq!(
            orientation::orient_layer(SliceStitchSite::PrintLayer(0), &events, &normals),
            Err(GenericPerceptionError::OrientationFaceOutOfRange {
                face_id: 5,
                faces: 1,
            })
        );
    }

    /// FU-10 phase A, spec V2 orientation cross-check brought forward: on a
    /// real kernel slice of an analytic convex solid the `z_hat x n`
    /// derivation must agree with the shoelace sign -- every layer stitches
    /// one counter-clockwise outer loop of the exact cross-section area,
    /// with nothing degraded, through the batched entry point.
    #[test]
    fn kernel_sliced_box_layers_orient_counter_clockwise() {
        let mesh = make_box([10.0, 8.0, 6.0], [0.0, 0.0, 0.0]).unwrap();
        let surface = NativeSurface::from_indexed(
            mesh.verts(),
            mesh.tris(),
            NativeSurfaceConfig {
                grid_pitch_mm: 0.5,
                max_grid_bytes: 16 * 1024 * 1024,
            },
        )
        .unwrap();
        let planes = [0.75, 1.5, 3.0, 4.5, 5.25];
        let batch = surface
            .slice_z_batch(
                &planes,
                PlaneSliceLimits {
                    max_planes: usize::MAX,
                    max_triangle_plane_tests: usize::MAX,
                    max_intersections: usize::MAX,
                    max_output_bytes: usize::MAX,
                },
            )
            .unwrap();
        let layers = orientation::orient_layers_batch(
            SliceStitchSpace::PrintLayer,
            &batch,
            surface.geometry().normals(),
        )
        .unwrap();
        assert_eq!(layers.len(), planes.len());
        for layer in &layers {
            assert_eq!(layer.degraded, None);
            assert_eq!(layer.loops.len(), 1);
            let area = layer.loops[0].signed_area_mm2;
            assert!(
                (area - 80.0).abs() < 1.0e-9,
                "expected +80 mm^2 counter-clockwise, got {area}"
            );
        }
    }

    // -- task #32: the seam invariant is directed balance -----------------

    /// Every directed half-edge appears exactly once and every one has its
    /// reverse: the triangle soup is closed and consistently wound. This is
    /// the hypothesis the balance guard rests on, so the seam fixtures state
    /// it about themselves rather than assume it -- and the sabotage fixtures
    /// use it to prove the sabotage really broke the mesh.
    fn closed_and_consistently_wound(tris: &[[u32; 3]]) -> bool {
        let mut directed = BTreeSet::<(u32, u32)>::new();
        for triangle in tris {
            for &(a, b) in &[
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                if !directed.insert((a, b)) {
                    return false;
                }
            }
        }
        directed.iter().all(|&(a, b)| directed.contains(&(b, a)))
    }

    /// Slice one triangle soup at `planes` through the frozen kernel, keeping
    /// the raw per-plane events and the face normals.
    fn stitch_events(
        verts: &[[f64; 3]],
        tris: &[[u32; 3]],
        planes: &[f64],
    ) -> (Vec<Vec<geom_kernel::PlaneIntersection>>, Vec<[f64; 3]>) {
        let surface = NativeSurface::from_indexed(
            verts,
            tris,
            NativeSurfaceConfig {
                grid_pitch_mm: 0.5,
                max_grid_bytes: 16 * 1024 * 1024,
            },
        )
        .unwrap();
        let batch = surface
            .slice_z_batch(
                planes,
                PlaneSliceLimits {
                    max_planes: usize::MAX,
                    max_triangle_plane_tests: usize::MAX,
                    max_intersections: usize::MAX,
                    max_output_bytes: usize::MAX,
                },
            )
            .unwrap();
        let events = (0..planes.len())
            .map(|index| batch.plane(index).to_vec())
            .collect();
        (events, surface.geometry().normals().to_vec())
    }

    /// Slice one triangle soup at `planes` and return the per-plane stitch
    /// verdict, so a test can say WHICH planes refuse rather than only that
    /// some plane did.
    fn stitch_verdicts(
        verts: &[[f64; 3]],
        tris: &[[u32; 3]],
        planes: &[f64],
    ) -> Vec<Result<orientation::OrientedLayer, GenericPerceptionError>> {
        let (events, normals) = stitch_events(verts, tris, planes);
        events
            .into_iter()
            .enumerate()
            .map(|(index, plane)| {
                orientation::orient_layer(SliceStitchSite::PrintLayer(index), &plane, &normals)
            })
            .collect()
    }

    /// M1, as a closed manifold: a solid whose top surface is a saddle whose
    /// pass sits EXACTLY on the slice plane.
    ///
    /// `V` is on the plane; its four link neighbours alternate above, below,
    /// above, below, so the surface crosses the plane four times around one
    /// vertex and four contour edges meet at that one point key. The section
    /// is two unit squares that touch at `V` -- a legitimate section of a
    /// perfectly closed, consistently wound, manifold solid, and exactly the
    /// shape the old degree-two guard called a branched seam.
    fn saddle_pinch_mesh() -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        let verts = vec![
            [0.0, 0.0, 1.0],    // 0  V, the saddle pass, exactly on z = 1
            [1.0, 1.0, 2.0],    // 1  A, above
            [1.0, -1.0, 0.0],   // 2  B, below
            [-1.0, -1.0, 2.0],  // 3  C, above
            [-1.0, 1.0, 0.0],   // 4  D, below
            [1.0, 1.0, -1.0],   // 5  A'
            [1.0, -1.0, -1.0],  // 6  B'
            [-1.0, -1.0, -1.0], // 7  C'
            [-1.0, 1.0, -1.0],  // 8  D'
        ];
        let tris = vec![
            // Top: a four-triangle fan around V, normals up.
            [0, 1, 4],
            [0, 4, 3],
            [0, 3, 2],
            [0, 2, 1],
            // Bottom, normals down.
            [5, 6, 7],
            [5, 7, 8],
            // Four planar side walls.
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 4, 8],
            [3, 8, 7],
            [4, 1, 5],
            [4, 5, 8],
        ];
        (verts, tris)
    }

    /// Task #32 mechanism M1: four contour edges meeting at one point key is
    /// correct closed geometry, not a branched seam.
    ///
    /// Pre-fix this refused with `OpenSliceSeam`, because the pinch vertex has
    /// undirected degree four. Post-fix it balances -- two edges in, two out --
    /// and sections into the two unit squares the solid actually has there.
    #[test]
    fn a_saddle_pass_on_the_plane_pinches_the_contour_and_still_sections() {
        let (verts, tris) = saddle_pinch_mesh();
        assert!(closed_and_consistently_wound(&tris));
        let verdicts = stitch_verdicts(&verts, &tris, &[1.0]);
        let layer = verdicts[0].as_ref().expect("a saddle pass is not a seam");
        assert_eq!(layer.degraded, None);
        // One closed walk that passes through the pinch twice: 10 vertices,
        // (0,0) among them twice, both unit squares in one stroke.
        assert_eq!(layer.loops.len(), 1);
        assert_eq!(
            layer.loops[0].points_xy_mm,
            vec![
                [-1.0, -1.0],
                [0.0, -1.0],
                [0.0, 0.0],
                [1.0, 0.0],
                [1.0, 0.333_333_333_333_333_26],
                [1.0, 1.0],
                [0.0, 1.0],
                [0.0, 0.0],
                [-1.0, 0.0],
                [-1.0, -0.333_333_333_333_333_26],
            ]
        );
        assert_eq!(layer.loops[0].signed_area_mm2, 2.0);

        // The adequacy probes stitch through this same code, so they gain the
        // same section rather than a probe-shaped seam refusal.
        let (events, normals) = stitch_events(&verts, &tris, &[1.0]);
        let probe =
            orientation::orient_layer(SliceStitchSite::AdequacyProbe(4), &events[0], &normals)
                .expect("the probe path is the same path");
        assert_eq!(probe.loops, layer.loops);

        // And through the union: 2 mm^2 of solid, in two squares.
        let solids = normalized_test_layer(
            SliceStitchSpace::PrintLayer,
            events[0].clone(),
            &normals,
        )
        .unwrap();
        assert_eq!(
            solids
                .iter()
                .map(|solid| round6(solid.area_mm2()))
                .collect::<Vec<_>>(),
            vec![1.0, 1.0]
        );
    }

    /// M2, as a closed manifold: a prism with a keel -- a horizontal ridge
    /// edge lying exactly on the plane with BOTH of its owner faces above it.
    ///
    /// The frozen kernel's positive-side-owner rule therefore emits that one
    /// mesh edge twice, and consistent winding makes the two emissions point
    /// opposite ways. Profile in (y, z): (0,3) (4,3) (4,2) (2,1) (0,2),
    /// extruded along x from 0 to 4; the keel is the extruded (2,1) corner.
    fn tangent_keel_mesh() -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        let verts = vec![
            [0.0, 0.0, 3.0], // 0 P0
            [0.0, 4.0, 3.0], // 1 P1
            [0.0, 4.0, 2.0], // 2 P2
            [0.0, 2.0, 1.0], // 3 P3, the keel's front end, exactly on z = 1
            [0.0, 0.0, 2.0], // 4 P4
            [4.0, 0.0, 3.0], // 5 P0'
            [4.0, 4.0, 3.0], // 6 P1'
            [4.0, 4.0, 2.0], // 7 P2'
            [4.0, 2.0, 1.0], // 8 P3', the keel's back end
            [4.0, 0.0, 2.0], // 9 P4'
        ];
        let tris = vec![
            // Front cap (x = 0, normals -x) and back cap (x = 4, normals +x).
            [0, 1, 2],
            [0, 2, 3],
            [0, 3, 4],
            [5, 7, 6],
            [5, 8, 7],
            [5, 9, 8],
            // The five extruded profile edges, two triangles each.
            [0, 5, 6],
            [0, 6, 1],
            [1, 6, 7],
            [1, 7, 2],
            [2, 7, 8],
            [2, 8, 3],
            [3, 8, 9],
            [3, 9, 4],
            [4, 9, 5],
            [4, 5, 0],
        ];
        (verts, tris)
    }

    /// Task #32 mechanism M2: a doubly-emitted tangent edge sections cleanly,
    /// and the zero-area walk it closes as is dropped rather than shipped.
    ///
    /// Pre-fix this refused with `OpenSliceSeam`: the two emissions collapsed
    /// into one undirected edge, leaving both keel ends at degree one.
    /// Post-fix the two directed copies balance each other, Hierholzer closes
    /// them as the two-vertex walk `P3 -> P3' -> P3`, and that walk is dropped
    /// for having fewer than three distinct point keys -- it is not a polygon
    /// and carries no area. The plane touches the solid along a line only, so
    /// an empty layer is the right answer.
    #[test]
    fn a_doubly_emitted_tangent_keel_edge_sections_and_leaves_no_spur() {
        let (verts, tris) = tangent_keel_mesh();
        assert!(closed_and_consistently_wound(&tris));
        // The keel really is emitted twice, and the two emissions really do
        // point opposite ways -- the premise, measured rather than assumed.
        let (events, normals) = stitch_events(&verts, &tris, &[1.0]);
        assert_eq!(events[0].len(), 2);
        let directed = events[0]
            .iter()
            .map(|event| {
                let a = [event.segment[0][0], event.segment[0][1]];
                let b = [event.segment[1][0], event.segment[1][1]];
                match orientation::direction_vote(a, b, normals[event.face_id as usize]) {
                    Some(true) => (a, b),
                    Some(false) => (b, a),
                    None => panic!("a keel owner's vote must not abstain"),
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            directed,
            vec![
                ([4.0, 2.0], [0.0, 2.0]),
                ([0.0, 2.0], [4.0, 2.0]),
            ]
        );

        let verdicts = stitch_verdicts(&verts, &tris, &[1.0, 1.5]);
        let keel_layer = verdicts[0].as_ref().expect("a tangent keel is not a seam");
        assert_eq!(keel_layer.degraded, None);
        assert!(keel_layer.loops.is_empty());
        // One layer higher the same solid sections normally: y from 1 to 3
        // over the full 4 mm of x.
        let above = verdicts[1].as_ref().unwrap();
        assert_eq!(above.loops.len(), 1);
        assert_eq!(above.loops[0].signed_area_mm2, 8.0);
    }

    /// Task #32, determinism pin. The Hierholzer decomposition of a multigraph
    /// is only well defined once an edge order is fixed, and that order
    /// reaches the frozen slice digests. The rule -- out-edge lists sorted
    /// ascending, next edge popped from the END -- is what reproduced all
    /// 8,099 passing corpus layers byte-for-byte, so it is pinned here on a
    /// fixture that actually exercises the choice: two counter-clockwise unit
    /// squares sharing the corner (2,2), where the walk must pick one of two
    /// out-edges. Every value below is fixed, not re-derived.
    #[test]
    fn the_hierholzer_walk_is_pinned_to_one_deterministic_loop_set() {
        use geom_kernel::{PlaneIntersection, PlaneIntersectionKind};
        let wall = |face_id, a: [f64; 2], b: [f64; 2]| PlaneIntersection {
            face_id,
            kind: PlaneIntersectionKind::Basic,
            segment: [[a[0], a[1], 0.5], [b[0], b[1], 0.5]],
        };
        let normals = [
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0, 0.0, 0.0],
        ];
        let events = vec![
            wall(0, [0.0, 0.0], [2.0, 0.0]),
            wall(1, [2.0, 0.0], [2.0, 2.0]),
            wall(2, [2.0, 2.0], [0.0, 2.0]),
            wall(3, [0.0, 2.0], [0.0, 0.0]),
            wall(4, [2.0, 2.0], [4.0, 2.0]),
            wall(5, [4.0, 2.0], [4.0, 4.0]),
            wall(6, [4.0, 4.0], [2.0, 4.0]),
            wall(7, [2.0, 4.0], [2.0, 2.0]),
        ];
        let layer =
            orientation::orient_layer(SliceStitchSite::PrintLayer(0), &events, &normals).unwrap();
        assert_eq!(layer.degraded, None);
        assert_eq!(layer.loops.len(), 1);
        assert_eq!(
            layer.loops[0].points_xy_mm,
            vec![
                [0.0, 0.0],
                [2.0, 0.0],
                [2.0, 2.0],
                [4.0, 2.0],
                [4.0, 4.0],
                [2.0, 4.0],
                [2.0, 2.0],
                [0.0, 2.0],
            ]
        );
        assert_eq!(layer.loops[0].signed_area_mm2, 8.0);
    }

    /// Task #32: a two-vertex walk is dropped, and dropping it changes no
    /// area. The fixture is a 4 mm square contour plus a doubly-emitted keel
    /// edge inside it -- the cavity-surface shape the corpus actually hits --
    /// and the layer comes out identical to the same square with no keel at
    /// all, through the union as well as at the stitcher.
    #[test]
    fn a_two_vertex_spur_is_dropped_and_the_areas_do_not_move() {
        use geom_kernel::{PlaneIntersection, PlaneIntersectionKind};
        let wall = |face_id, a: [f64; 2], b: [f64; 2]| PlaneIntersection {
            face_id,
            kind: PlaneIntersectionKind::Basic,
            segment: [[a[0], a[1], 0.5], [b[0], b[1], 0.5]],
        };
        let normals = [
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0, 0.0, 0.0],
            // The keel's two owners, leaning opposite ways as consistent
            // winding forces them to.
            [0.0, -1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let square = vec![
            wall(0, [0.0, 0.0], [4.0, 0.0]),
            wall(1, [4.0, 0.0], [4.0, 4.0]),
            wall(2, [4.0, 4.0], [0.0, 4.0]),
            wall(3, [0.0, 4.0], [0.0, 0.0]),
        ];
        let mut with_keel = square.clone();
        with_keel.push(wall(4, [1.0, 2.0], [3.0, 2.0]));
        with_keel.push(wall(5, [1.0, 2.0], [3.0, 2.0]));

        let plain =
            orientation::orient_layer(SliceStitchSite::PrintLayer(0), &square, &normals).unwrap();
        let keeled =
            orientation::orient_layer(SliceStitchSite::PrintLayer(0), &with_keel, &normals)
                .unwrap();
        assert_eq!(keeled.degraded, None);
        assert_eq!(keeled.loops, plain.loops);
        assert_eq!(keeled.loops.len(), 1);
        assert_eq!(keeled.loops[0].signed_area_mm2, 16.0);

        let plain_solids =
            normalized_test_layer(SliceStitchSpace::PrintLayer, square, &normals).unwrap();
        let keeled_solids =
            normalized_test_layer(SliceStitchSpace::PrintLayer, with_keel, &normals).unwrap();
        assert_eq!(
            keeled_solids
                .iter()
                .map(|solid| round6(solid.area_mm2()))
                .collect::<Vec<_>>(),
            vec![16.0]
        );
        assert_eq!(
            keeled_solids
                .iter()
                .map(|solid| solid.outer().points_xy_mm().to_vec())
                .collect::<Vec<_>>(),
            plain_solids
                .iter()
                .map(|solid| solid.outer().points_xy_mm().to_vec())
                .collect::<Vec<_>>()
        );
    }

    /// Task #32 sabotage, both ways: the guard is narrowed to the correct
    /// invariant, NOT weakened. Delete one triangle from a closed box, or
    /// reverse one triangle's winding, and every plane that triangle spans
    /// must still be refused by name -- for every triangle in the mesh, not a
    /// hand-picked one. The intact box passes all of the same planes, so the
    /// refusals are the sabotage and nothing else.
    #[test]
    fn a_deleted_or_flipped_triangle_is_still_refused_at_every_plane_it_spans() {
        let mesh = make_box([10.0, 8.0, 6.0], [0.0, 0.0, 0.0]).unwrap();
        let verts = mesh.verts().to_vec();
        let tris = mesh.tris().to_vec();
        assert!(closed_and_consistently_wound(&tris));
        let planes = [0.75, 1.5, 3.0, 4.5, 5.25];
        for verdict in stitch_verdicts(&verts, &tris, &planes) {
            assert!(verdict.is_ok(), "the intact box must section at every plane");
        }

        let mut checked = 0usize;
        for victim in 0..tris.len() {
            let zs = tris[victim].map(|index| verts[index as usize][2]);
            let low = zs.iter().copied().fold(f64::INFINITY, f64::min);
            let high = zs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let spanned = planes
                .iter()
                .copied()
                .filter(|z| *z > low && *z < high)
                .collect::<Vec<_>>();
            if spanned.is_empty() {
                continue;
            }

            let mut dropped = tris.clone();
            dropped.remove(victim);
            assert!(
                !closed_and_consistently_wound(&dropped),
                "deleting triangle {victim} must really open the mesh"
            );
            let mut flipped = tris.clone();
            flipped[victim] = [tris[victim][0], tris[victim][2], tris[victim][1]];
            assert!(
                !closed_and_consistently_wound(&flipped),
                "flipping triangle {victim} must really break the winding"
            );

            for (label, sabotaged) in [("deleted", &dropped), ("flipped", &flipped)] {
                for (offset, verdict) in stitch_verdicts(&verts, sabotaged, &spanned)
                    .into_iter()
                    .enumerate()
                {
                    assert_eq!(
                        verdict.err(),
                        Some(GenericPerceptionError::OpenSliceSeam { layer: offset }),
                        "{label} triangle {victim} must be refused at z = {}",
                        spanned[offset]
                    );
                    checked += 1;
                }
            }
        }
        // Every side triangle of the box spans all five planes; the two caps
        // span none. 8 side triangles x 5 planes x 2 sabotages.
        assert_eq!(checked, 80);
    }

    /// Two unit-ish squares sharing exactly the point `(0, 0)`: undirected
    /// degree four there (two edges from each square), degree two everywhere
    /// else -- the same M1 pinch mechanism as [`saddle_pinch_mesh`], minimal
    /// and built directly on the plane rather than sliced out of a real mesh,
    /// so it exercises `orientation`'s walk in isolation.
    fn pinch_two_squares_fixture() -> Vec<geom_kernel::PlaneIntersection> {
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
        let mut events = ring(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]);
        events.extend(ring(&[[0.0, 0.0], [-2.0, 0.0], [-2.0, -2.0], [0.0, -2.0]]));
        events
    }

    /// FU-45 follow-up #3 -- CHARACTERIZATION, not a spec. The degraded
    /// path's guard (`walk_undirected`, above) is even-degree: strictly
    /// weaker than the directed path's balance guard, and weaker still than
    /// the pre-task-#32 degree-2-only stitcher. A degree-four pinch is EVEN,
    /// so nothing in the degraded path refuses it. That gap is disclosed in
    /// `walk_undirected`'s own doc comment ("it cannot see a flipped face"),
    /// measured unreachable today -- 0 abstains across the corpus's 4,049
    /// planes, so the degraded path never meets a real pinch in production --
    /// and until this test, never pinned.
    ///
    /// This records what today's code actually does when the two coincide:
    /// it walks the pinch and lets EvenOdd fill it, exactly what the old
    /// degree-2 rule refused (contrast
    /// `a_saddle_pass_on_the_plane_pinches_the_contour_and_still_sections`,
    /// where the directed path meets the identical geometric mechanism and
    /// sections it correctly instead of guessing). This is not an
    /// endorsement -- the risk is real, only currently unreachable. The day
    /// the degraded path becomes reachable on a genuinely pinched layer,
    /// this test is the tripwire: a changed value here means the risk went
    /// live, and it needs a real fix, not a quiet re-pin.
    #[test]
    fn a_degree_four_pinch_on_the_degraded_path_is_walked_not_refused() {
        let events = pinch_two_squares_fixture();
        let normals = degenerate_normals(events.len());
        let layer = orientation::orient_layer(SliceStitchSite::PrintLayer(0), &events, &normals)
            .expect("even degree everywhere: the degraded-path guard cannot see the pinch");
        assert_eq!(
            layer.degraded,
            Some(orientation::WindingDegradation {
                ambiguous_segments: events.len(),
            })
        );
        // PIN: the walk does not keep the two squares apart. `pop_unused`
        // pairs the pinch's four incident edges LIFO by point key, so the
        // Hierholzer-style decomposition comes out as ONE eight-vertex
        // stroke through `(0, 0)` twice -- both squares stitched into a
        // single connected loop -- rather than the two four-vertex loops the
        // directed path would derive from the same geometry (compare
        // `a_saddle_pass_on_the_plane_pinches_the_contour_and_still_sections`).
        // That pairing is exactly the "how a pinch is paired" sensitivity
        // `orient_layer`'s doc comment warns EvenOdd is not inert to.
        assert_eq!(layer.loops.len(), 1, "PIN: today's pairing at the pinch");
        assert_eq!(
            layer.loops[0].points_xy_mm,
            vec![
                [-2.0, -2.0],
                [0.0, -2.0],
                [0.0, 0.0],
                [2.0, 0.0],
                [2.0, 2.0],
                [0.0, 2.0],
                [0.0, 0.0],
                [-2.0, 0.0],
            ],
            "PIN: today's walk order through the pinch"
        );
        assert_eq!(
            layer.loops[0].signed_area_mm2, 8.0,
            "PIN: raw shoelace of the merged stroke"
        );

        // The union still recovers the two disjoint 2x2 squares here, because
        // this fixture is symmetric enough that EvenOdd's crossing count
        // happens to land right. That is a property of THIS geometry, not a
        // guarantee: the walk's arbitrary pairing at the pinch is the thing
        // that is unreachable-but-live, and a less symmetric pinch could pair
        // the same way and have EvenOdd disagree with the true answer. This
        // assertion pins today's output; it is not evidence the mechanism is
        // safe in general.
        let solids = normalized_test_layer(SliceStitchSpace::PrintLayer, events, &normals)
            .expect("the walk closes, so the union runs, EvenOdd and all");
        assert_eq!(
            solids
                .iter()
                .map(|solid| (
                    solid.outer().points_xy_mm().to_vec(),
                    round6(solid.area_mm2())
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    vec![[-2.0, -2.0], [0.0, -2.0], [0.0, 0.0], [-2.0, 0.0]],
                    4.0
                ),
                (vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]], 4.0),
            ],
            "PIN: today's EvenOdd-filled solids"
        );
        let filled = solids.iter().map(SliceSolid::area_mm2).sum::<f64>();
        assert_eq!(round6(filled), 8.0, "PIN: today's EvenOdd-filled area");
    }

    /// The three-ring self-crossing fixture of the D2/D1 cases: an outer
    /// stroke that folds back over itself, a middle ring straddling the fold,
    /// and an island inside the middle ring but inside the outer's
    /// doubly-covered lobe.
    fn self_crossing_fixture() -> Vec<geom_kernel::PlaneIntersection> {
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
        // Middle: straddles the fold. Under the legacy vertex-parity probe it
        // landed below y = 2 and so was called a hole of the outer loop.
        events.extend(ring(&[[3.0, 0.2], [5.0, 0.2], [5.0, 2.4], [3.0, 2.4]]));
        // Island: wholly inside the middle loop, but inside the doubly-covered
        // block, so under parity it tested OUTSIDE the outer loop - a hole no
        // solid contained, which the deleted Tier-1 chain claimed by bounding
        // box.
        events.extend(ring(&[[3.4, 2.05], [4.6, 2.05], [4.6, 2.35], [3.4, 2.35]]));
        events
    }

    /// A mesh defect has no defensible outward normal at the crossing, so
    /// every `z_hat x n` vote abstains and sec 3.3's degraded even-odd path is
    /// what a self-crossing layer actually gets. Modelled here by a straight-up
    /// normal, for which the derived in-plane direction is the zero vector.
    fn degenerate_normals(count: usize) -> Vec<[f64; 3]> {
        vec![[0.0, 0.0, 1.0]; count]
    }

    /// V3 disposition (FU-10 phase C): kept, and re-aimed at normalized
    /// output. The old expectation -- outer `52.0` with holes `[-4.4, -0.36]`
    /// -- was the parity stitcher's, and most of it was fallback machinery:
    /// `52.0` is the raw shoelace of a self-crossing stroke (an area no fill
    /// rule agrees with), the middle ring was called a hole by a first-vertex
    /// probe, and the island was claimed only by the deleted bounding-box
    /// tier. The union answers what the fallbacks were guessing at, and the
    /// answer is checkable by hand.
    ///
    /// Under even-odd the outer stroke's crossing number partitions the plane
    /// into 3x3 blocks about x = 0, 2, 6, 8 and y = 0, 2, 4, 6. Filled
    /// (odd) are every block except `x in [6,8], y in [0,2]` (0 crossings)
    /// and `x in [2,6], y in [2,4]` (the doubly covered fold, 2 crossings):
    /// `12 + 8 + 16 = 36 mm^2`. The middle ring adds one crossing over
    /// `x in [3,5]`: its `y in [0.2,2]` part goes even and voids
    /// `2 * 1.8 = 3.6`, its `y in [2,2.4]` part goes odd and fills
    /// `2 * 0.4 = 0.8`. The island adds one more crossing inside that
    /// 0.8 patch, voiding `1.2 * 0.3 = 0.36` back out as a hole. So:
    /// `36 - 3.6 = 32.4` for the main solid -- the 3.6 void is not a hole
    /// because it reaches the exterior through the pinch points at (3,2),
    /// (5,2) and (6,2) -- plus a separate `0.8` solid holding the `-0.36`
    /// island. Total filled area `32.84 mm^2`.
    #[test]
    fn self_crossing_outer_contour_still_yields_solids_not_a_seam_error() {
        let events = self_crossing_fixture();
        let normals = degenerate_normals(events.len());
        let solids = normalized_test_layer(SliceStitchSpace::PrintLayer, events, &normals)
            .expect("a self-crossing contour is survivable, not a seam violation");
        let shape = solids
            .iter()
            .map(|solid| {
                (
                    round6(solid.outer().signed_area_mm2()),
                    solid
                        .holes()
                        .iter()
                        .map(|hole| round6(hole.signed_area_mm2()))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(shape, vec![(32.4, vec![]), (0.8, vec![-0.36])]);
        let filled = solids.iter().map(SliceSolid::area_mm2).sum::<f64>();
        assert_eq!(round6(filled), 32.84);
    }

    fn round6(value: f64) -> f64 {
        (value * 1.0e6).round() / 1.0e6
    }

    /// V3 disposition (FU-10 phase C): the named successor to
    /// `a_hole_no_solid_can_reach_is_discarded_rather_than_raised`, which
    /// tested the deleted `hole_owner` fallback chain directly. What that test
    /// pinned -- that a hole no solid can claim is silently dropped -- has no
    /// referent on normalized output, because ownership is structural rather
    /// than probed. This asserts the property that replaces it, on the very
    /// fixture that used to manufacture the unclaimable hole: every emitted
    /// hole lies strictly inside the outer that owns it, and the filled area
    /// accounts for every square millimetre the fill rule says is solid, so
    /// nothing was claimed by bounding box and nothing was dropped.
    #[test]
    fn every_normalized_hole_is_owned_by_construction_so_none_is_ever_dropped() {
        let events = self_crossing_fixture();
        let normals = degenerate_normals(events.len());
        let solids = normalized_test_layer(SliceStitchSpace::PrintLayer, events, &normals).unwrap();
        assert!(!solids.is_empty(), "the fixture must yield solids");
        for solid in &solids {
            assert!(
                solid.outer().signed_area_mm2() > 0.0,
                "outers wind counter-clockwise"
            );
            for hole in solid.holes() {
                assert!(hole.signed_area_mm2() < 0.0, "holes wind clockwise");
                assert!(
                    hole.points_xy_mm()
                        .iter()
                        .all(|point| point_in_polygon(*point, solid.outer().points_xy_mm())),
                    "a normalized hole is strictly inside the outer that owns it"
                );
            }
        }
        // The area the deleted chain could distort in either direction: a
        // dropped hole over-reports, a bbox-claimed one under-reports. Neither
        // is possible against the hand-derived even-odd fill.
        assert_eq!(
            round6(solids.iter().map(SliceSolid::area_mm2).sum::<f64>()),
            32.84
        );
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
        // pins is stated over the same terms the finalizer charges.  Phase G5
        // added the transpose's *operation* term beside its byte term, for the
        // same reason and from the same count, so both are taken from it here.
        let transpose = LoadTransposeCounts::count(&plan.load_eligibility, &contacts).unwrap();
        let resource = load_resource_plan(
            &plan.load_eligibility,
            &contacts,
            transpose.scratch_bytes(contacts.len()).unwrap(),
            transpose.operations().unwrap(),
        )
        .unwrap();
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
    /// (`rows x contacts x (owner_log + 4)`, as that bound stood in 2026-08-06
    /// and until FU-8f phase G5 repriced it on 2026-08-07) and the retained-byte
    /// bound by that same factor, which is what put the treant's load stage over
    /// its caps.  What this fixture pins is what `rows` **counts**, which no
    /// repricing touches.  This fixture reproduces the shape in miniature: 64 specialized
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
        let (baseline_partition, baseline, _transpose) = SpecializedLoadBaseline::build(
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
                transpose: None,
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

    /// FU-8f phase G5: the load charge is no longer quadratic in
    /// `rows x contacts`, and what replaced it is the transpose's own shape.
    ///
    /// The old model's whole defect was its **shape**: it charged one
    /// eligibility binary search per `(row, contact)` cell, so holding the
    /// eligibility fixed and widening the contact set multiplied the charge by
    /// the contact count even though the finalizer's work did not move.  That
    /// is the property this test pins, and it pins it as a *shape* rather than
    /// as a literal, per ruling R-e: both arms are derivations, and the
    /// superseded expression is written out here so the comparison is against
    /// the thing that was replaced rather than against a remembered number.
    ///
    /// Arm 1 — the treant's own gate-batch counts, 2026-08-07: 117,741 rows,
    /// 1,366 contacts, `Σ |eligible_owners|` = 7,988,078.  The superseded
    /// expression charged 4,342,523,562 for the scan alone and 4,714,113,843 in
    /// total; the repriced model states 579,285,773 here plus the 65,685,144
    /// cells term [`load_resource_plan`] adds — a **21x** fall on the scan
    /// term and an **8.1x** fall on the total.  The gap between those two
    /// factors is deliberate and is stated so nobody reads the smaller one as a
    /// disappointment: what is left is dominated by
    /// `Σ |eligible_owners| x owner_log x 2` = 367,451,588, which prices the
    /// eligibility's own ordering, is not the scan, and G5 did not touch.  The
    /// assertion is therefore 5x, which the 8.1x clears with margin while
    /// still failing outright if the quadratic ever returns.
    ///
    /// Arm 2 — non-vacuity against a silent revert.  Widening the contact count
    /// 100x with the eligibility held fixed must **not** multiply the charge:
    /// under the old model it grew 100x exactly, and the transpose's only
    /// contact-shaped costs are the `log` inside each search and the
    /// contact-linear passes.
    #[test]
    fn fu8f_g5_load_charge_is_no_longer_quadratic_in_rows_times_contacts() {
        /// The pre-G5 model, written out so this test compares against the
        /// expression it replaced rather than against a copied total.
        fn superseded(
            face_count: usize,
            eligible_owner_count: usize,
            contact_count: usize,
        ) -> usize {
            let log = usize::BITS as usize - contact_count.max(1).leading_zeros() as usize;
            let owner_log =
                usize::BITS as usize - eligible_owner_count.max(1).leading_zeros() as usize;
            let face_log = usize::BITS as usize - face_count.max(1).leading_zeros() as usize;
            face_count * contact_count * (owner_log + 4)
                + contact_count * (log + 2)
                + eligible_owner_count * (owner_log * 2)
                + face_count * (face_log * 2 + 1)
        }

        const TREANT_GATE_LOAD_ROWS: usize = 117_741;
        const TREANT_CONTACTS: usize = 1_366;
        const TREANT_OWNER_ENTRIES: usize = 7_988_078;

        let charged = load_resource_plan_from_counts(
            TREANT_GATE_LOAD_ROWS,
            TREANT_OWNER_ENTRIES,
            TREANT_CONTACTS,
            0,
        )
        .expect("the treant's gate-batch load counts must plan")
        .operations;
        let old = superseded(TREANT_GATE_LOAD_ROWS, TREANT_OWNER_ENTRIES, TREANT_CONTACTS);
        assert!(
            charged * 5 < old,
            "the repriced load model must state less than a fifth of the \
             superseded one on the treant's gate batch: {charged} against {old}",
        );

        // The cells term is the only part of the finalizer's charge that is
        // cells-shaped, and it is added by `load_resource_plan`, not here.
        let contacts = (0..3u32)
            .map(|index| PhysicalContact {
                contact_id: u64::from(index),
                owner: LoadOwner::Specialized(index),
                point_mm: [0.0, 0.0, 0.0],
            })
            .collect::<Vec<_>>();
        let eligibility = (0..4u32)
            .map(|face| LoadEligibility {
                face_id: face,
                eligible_owners: vec![LoadOwner::Specialized(face % 3)],
                centroid_mm: [0.0, 0.0, 0.0],
                area_mm2: 1.0,
            })
            .collect::<Vec<_>>();
        let transpose = LoadTransposeCounts::count(&eligibility, &contacts).expect("count");
        assert_eq!(
            transpose.operations().expect("transpose operations"),
            transpose.cells * (LOAD_WITNESS_SLOTS + 5),
            "the cells term must be exactly what its doc comment states",
        );

        // Arm 2: 100x the contacts, the same eligibility.
        let narrow = load_resource_plan_from_counts(1_000, 4_000, 10, 0)
            .expect("narrow")
            .operations;
        let wide = load_resource_plan_from_counts(1_000, 4_000, 1_000, 0)
            .expect("wide")
            .operations;
        assert!(
            wide < narrow * 3,
            "widening the contact set 100x with the eligibility fixed must not \
             multiply the charge: {narrow} -> {wide}",
        );
        assert!(
            superseded(1_000, 4_000, 1_000) > superseded(1_000, 4_000, 10) * 50,
            "the superseded model really was contact-multiplied, so arm 2 is \
             not vacuous",
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

    // ----------------------------------------------------------------------
    // FU-8f phase G6a — the retained load transpose's four guards.
    //
    // Design record: `docs/engineering-journal/fu8f-g6-design-2026-08-07.md`,
    // D-G6-3.  Every guard below is **mutation-proven**: the doc comment on
    // each test names the exact deletion in `RetainedLoadTranspose` that the
    // test is required to catch, and the transcript of that proof is recorded
    // in the landing commit.  A guard whose test stays green after its compare
    // is deleted is a guard that is not there, and must not land — this is the
    // FU-8e/G1 non-vacuity rule applied to a new staleness class.
    //
    // The precedent that makes these mandatory rather than decorative is in
    // this file: a stale structure meeting a *larger* mesh produced a real
    // `index out of bounds` panic (G2 arm 3).  Guard 1 is the direct heir.
    // ----------------------------------------------------------------------

    /// The shared retention fixture: six specialized seats over
    /// `make_overhang_table`, plus one seat per generic island so the
    /// finalizer's rows all have holders — the same shape a real gate batch
    /// hands `build_gate_load_state`, and deliberately the same shape the
    /// FU-8d delta fixture uses, so the seats' served-face overlaps
    /// (`elig(0) ∩ elig(1)` non-empty, `elig(5)` disjoint from both) are the
    /// already-exercised ones.
    ///
    /// Specialized owners are `Specialized(index)` at contact position
    /// `index`, which is `append_variant_load_inputs`'s own
    /// `first_specialized_owner + contact_id` construction with
    /// `first_specialized_owner == 0` — the invariant `record_commit`'s
    /// positional renumbering depends on.
    struct RetentionFixture {
        mesh: Mesh,
        plan: GenericContactPlan,
        surface: NativeSurface,
        contacts: Vec<PhysicalContact>,
        pairs: Vec<(u32, u32)>,
        specialized_seats: usize,
    }

    /// Seat 5's `z` is exactly `+0.0` so guard 2's bit-comparison can be
    /// separated from an `==` comparison, which `-0.0` would pass.
    const RETENTION_ZERO_Z_SEAT: usize = 5;

    fn retention_fixture() -> RetentionFixture {
        const SEATS: usize = 6;
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
        // Overlapping served-face sets: seats 0 and 1 share faces 0 and 1, so
        // their bucket union is a real dedup merge and not a concatenation.
        let served: [&[u32]; SEATS] = [
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
        let mut points: [[f64; 3]; SEATS] = [
            point(1, 0.5),
            point(1, 0.25),
            point(3, 0.25),
            point(5, 0.25),
            point(6, 0.25),
            point(7, 0.25),
        ];
        points[RETENTION_ZERO_Z_SEAT][2] = 0.0;
        let pairs = (0..SEATS)
            .flat_map(|seat| served[seat].iter().map(move |&face| (seat as u32, face)))
            .collect::<Vec<_>>();
        let generic_islands = plan
            .islands()
            .iter()
            .map(|island| island.id)
            .collect::<Vec<_>>();
        assert!(
            !generic_islands.is_empty(),
            "the fixture needs generic seats, or the positional-owner arm of \
             record_commit is never exercised",
        );
        let contacts = (0..SEATS)
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
                        contact_id: (SEATS + slot) as u64,
                        owner: LoadOwner::GenericIsland(island),
                        point_mm: [-5.0 - slot as f64, -5.0, -5.0],
                    }),
            )
            .collect::<Vec<_>>();
        RetentionFixture {
            mesh,
            plan,
            surface,
            contacts,
            pairs,
            specialized_seats: SEATS,
        }
    }

    fn retention_limits() -> LoadFinalizeLimits {
        LoadFinalizeLimits {
            max_scratch_bytes: usize::MAX,
            max_operations_floor: usize::MAX,
            operations_per_batch_unit: 0,
        }
    }

    impl RetentionFixture {
        fn proof(&self) -> VerifiedMeshFingerprint<'_> {
            VerifiedMeshFingerprint::compute(&self.mesh).unwrap()
        }

        /// One full build, absorbed into `slot` exactly as
        /// `build_gate_load_state` does it — from the build's own transpose
        /// and partition, never from a restatement of them.
        fn absorb(
            &self,
            slot: &mut Option<RetainedLoadTranspose>,
            cause: Option<RetainedLoadRebuildCause>,
            contacts: &[PhysicalContact],
        ) {
            let proof = self.proof();
            let (partition, baseline, transpose) = SpecializedLoadBaseline::build(
                &self.plan,
                &proof,
                &self.surface,
                contacts,
                &self.pairs,
                retention_limits(),
            )
            .expect("the retention fixture must build");
            RetainedLoadTranspose::absorb_full_build(
                slot, cause, &proof, transpose, &partition, &baseline, contacts,
            );
        }

        fn retained(&self) -> RetainedLoadTranspose {
            let mut slot = None;
            self.absorb(&mut slot, None, &self.contacts);
            slot.expect("absorb_full_build always populates the slot")
        }

        /// Per-contact eligible-row counts derived the way
        /// `build_gate_load_state` derives them — from `pairs` and the
        /// partition's rows through `face_row`/`row_face` — so guard 4's
        /// positive control is an **independent** derivation and not the
        /// retained buckets restated.
        fn contact_rows(&self) -> Vec<usize> {
            let proof = self.proof();
            let (partition, _, _) = SpecializedLoadBaseline::build(
                &self.plan,
                &proof,
                &self.surface,
                &self.contacts,
                &self.pairs,
                retention_limits(),
            )
            .expect("the retention fixture must build");
            let mut face_row = vec![u32::MAX; self.mesh.tris().len()];
            let mut row_face = Vec::with_capacity(partition.rows().len());
            for (row, load) in partition.rows().iter().enumerate() {
                row_face.push(load.face_id);
                if face_row[load.face_id as usize] == u32::MAX {
                    face_row[load.face_id as usize] = row as u32;
                }
            }
            let mut counts = vec![0usize; self.contacts.len()];
            for (contact, count) in counts.iter_mut().enumerate() {
                // The fixture's `pairs` are the specialized seats' own
                // `(owner, face)` runs; generic seats contribute none, exactly
                // as `append_variant_load_inputs` emits them.
                for &(owner, face_id) in &self.pairs {
                    if owner as usize != contact {
                        continue;
                    }
                    let mut row = face_row[face_id as usize];
                    if row == u32::MAX {
                        continue;
                    }
                    while (row as usize) < row_face.len() && row_face[row as usize] == face_id {
                        *count += 1;
                        row += 1;
                    }
                }
            }
            counts
        }
    }

    /// Guard 1 (design D-G6-3): a retained transpose must refuse a mesh that
    /// is not the one it was built under.  Retained row indices name rows of
    /// *this* mesh's eligibility; against another mesh they are at best wrong
    /// and at worst out of range — the exact shape of the real panic recorded
    /// at `fu8f_hoisted_fingerprint_still_refuses_a_foreign_mesh_at_both_finalizers`.
    ///
    /// Both directions are presented deliberately, because past a missing
    /// check they fail differently: a *smaller* foreign mesh silently serves
    /// rows that do not exist there, a *larger* one is the G2 arm-3
    /// out-of-bounds shape.
    ///
    /// # Mutation proof
    ///
    /// Delete the `self.mesh_fingerprint != verified_mesh.fingerprint()` arm
    /// of [`RetainedLoadTranspose::entry_rebuild_cause`] and this test fails:
    /// both foreign arms return `None` where `Some(MeshFingerprint)` is
    /// asserted.  Transcript, run 2026-08-07 (mutation M1, the compare
    /// short-circuited to `false`):
    /// `fu8f_retained_transpose_refuses_a_foreign_mesh ... FAILED`,
    /// `assertion 'left == right' failed: a retained transpose met a SMALLER
    /// foreign mesh and did not refuse it` — the smaller arm is asserted
    /// first, and the larger arm fails identically once it is reached.
    #[test]
    fn fu8f_retained_transpose_refuses_a_foreign_mesh() {
        let fixture = retention_fixture();
        let retained = fixture.retained();

        // Positive control first: under its own proof the carrier is reusable.
        // Without this the test could pass by refusing everything.
        assert_eq!(
            retained.entry_rebuild_cause(&fixture.proof(), &fixture.contacts),
            None,
            "the carrier must be reusable under the very mesh it was built \
             under, or this test proves only that it refuses everything",
        );

        let smaller = plinth_mesh::make_box_default().unwrap();
        let larger = make_toed_slab().unwrap();
        let faces = fixture.mesh.tris().len();
        assert!(
            smaller.tris().len() < faces && faces < larger.tris().len(),
            "the arms need one foreign mesh smaller and one larger than the \
             fixture's {faces} faces, got {} and {}",
            smaller.tris().len(),
            larger.tris().len(),
        );
        assert_eq!(
            retained.entry_rebuild_cause(
                &VerifiedMeshFingerprint::compute(&smaller).unwrap(),
                &fixture.contacts,
            ),
            Some(RetainedLoadRebuildCause::MeshFingerprint),
            "a retained transpose met a SMALLER foreign mesh and did not refuse it",
        );
        assert_eq!(
            retained.entry_rebuild_cause(
                &VerifiedMeshFingerprint::compute(&larger).unwrap(),
                &fixture.contacts,
            ),
            Some(RetainedLoadRebuildCause::MeshFingerprint),
            "a retained transpose met a LARGER foreign mesh and did not refuse it",
        );
    }

    /// Guard 2 (design D-G6-3, finding F-G6-4): the retained contact witness
    /// must equal the fresh physical list, so that every contact mutation not
    /// routed through [`RetainedLoadTranspose::record_commit`] forces a
    /// counted rebuild.  The known live instance is
    /// `coalesce_exact_regular_seats`, which mutates contacts *after* the
    /// tournament loop and before the terminal proposal-free audit; the guard
    /// is written as equality rather than a round counter precisely so future
    /// mutation sites nobody remembers to wire are caught too.
    ///
    /// The `-0.0` arm is not decoration: it is what separates a bit
    /// comparison from `==`, which would accept `-0.0` for `+0.0` and retain
    /// a transpose synchronized to a contact list that is not the one being
    /// scanned.
    ///
    /// # Mutation proof
    ///
    /// Delete the `witnessed` comparison (or the length check) in
    /// [`RetainedLoadTranspose::entry_rebuild_cause`] and this test fails.
    /// Transcript, run 2026-08-07 (mutation M2):
    /// `fu8f_retained_transpose_rebuilds_when_contacts_moved_behind_it ... FAILED`,
    /// `assertion 'left == right' failed: a contact MOVED behind the retained
    /// transpose and it did not refuse reuse`.  Separately (mutation M2b),
    /// replacing the bit comparison with `point_mm == contact.point_mm` fails
    /// the `-0.0` arm and only that arm:
    /// `assertion 'left == right' failed: +0.0 became -0.0 behind the retained
    /// transpose and it did not refuse reuse — the witness is comparing
    /// values, not bits`.  Which is that arm's whole job.
    #[test]
    fn fu8f_retained_transpose_rebuilds_when_contacts_moved_behind_it() {
        let fixture = retention_fixture();
        let retained = fixture.retained();
        let proof = fixture.proof();

        assert_eq!(
            retained.entry_rebuild_cause(&proof, &fixture.contacts),
            None,
            "an unmutated contact list must be reusable",
        );

        // (a) one contact moved by one ULP — the smallest real perturbation.
        let mut moved = fixture.contacts.clone();
        moved[2].point_mm[0] = f64::from_bits(moved[2].point_mm[0].to_bits() + 1);
        assert_eq!(
            retained.entry_rebuild_cause(&proof, &moved),
            Some(RetainedLoadRebuildCause::ContactWitness),
            "a contact MOVED behind the retained transpose and it did not refuse reuse",
        );

        // (b) `+0.0` replaced by `-0.0`: equal under `==`, different bits, and
        // a different contact as far as any downstream `to_bits` witness is
        // concerned.
        let mut negated_zero = fixture.contacts.clone();
        assert_eq!(
            negated_zero[RETENTION_ZERO_Z_SEAT].point_mm[2].to_bits(),
            0.0f64.to_bits(),
            "the fixture must actually seat a +0.0 coordinate for this arm",
        );
        negated_zero[RETENTION_ZERO_Z_SEAT].point_mm[2] = -0.0;
        assert_eq!(
            retained.entry_rebuild_cause(&proof, &negated_zero),
            Some(RetainedLoadRebuildCause::ContactWitness),
            "+0.0 became -0.0 behind the retained transpose and it did not \
             refuse reuse — the witness is comparing values, not bits",
        );

        // (c) an owner reassigned without the position moving: the coalesce
        // shape, where a seat's identity changes under a fixed point.
        let mut reowned = fixture.contacts.clone();
        reowned[0].owner = LoadOwner::Specialized(u32::MAX);
        assert_eq!(
            retained.entry_rebuild_cause(&proof, &reowned),
            Some(RetainedLoadRebuildCause::ContactWitness),
            "a contact's OWNER changed behind the retained transpose and it \
             did not refuse reuse",
        );

        // (d) a contact removed outright — the coalesce's actual effect.
        let mut shortened = fixture.contacts.clone();
        shortened.pop();
        assert_eq!(
            retained.entry_rebuild_cause(&proof, &shortened),
            Some(RetainedLoadRebuildCause::ContactWitness),
            "a contact was REMOVED behind the retained transpose and it did \
             not refuse reuse",
        );
    }

    /// Guard 3 (design D-G6-3, finding F-G6-1): the merged seat's bucket is
    /// the union of its parents' buckets **only when the merged seat resolves
    /// specialized**.  When `generic_owner_for_parts` returns `Some`, the
    /// merged seat serves the generic authority's rows instead, the union
    /// identity is false, and a retained carrier would serve silently wrong
    /// eligibility.  So retention is poisoned instead, and the next round
    /// pays one counted full rebuild.
    ///
    /// The negative arm is the load-bearing one; the positive arm exists so
    /// the test cannot pass by poisoning unconditionally.
    ///
    /// # Mutation proof
    ///
    /// Delete the `if !merged_resolves_specialized { self.poison(); return; }`
    /// arm of [`RetainedLoadTranspose::record_commit`] and this test fails.
    /// Transcript, run 2026-08-07 (mutation M3):
    /// `fu8f_retained_transpose_invalidates_on_a_generic_resolving_merge ... FAILED`,
    /// panicked at `perception.rs`: `a generic-resolving merge left retention
    /// live`.
    #[test]
    fn fu8f_retained_transpose_invalidates_on_a_generic_resolving_merge() {
        let fixture = retention_fixture();
        let proof = fixture.proof();
        // Merge seats 0 and 1; the committed seat lands at position 0.
        let (left, right) = (0usize, 1usize);
        let merged_position = [1.5, 2.5, 3.5];
        let positions_after = |merged_at: usize| -> Vec<[f64; 3]> {
            let survivors = (0..fixture.contacts.len())
                .filter(|index| *index != left && *index != right)
                .map(|index| fixture.contacts[index].point_mm);
            let mut list = survivors.collect::<Vec<_>>();
            list.insert(merged_at, merged_position);
            list
        };

        // Negative arm: the merged seat resolved GENERIC.
        let mut poisoned = fixture.retained();
        poisoned.record_commit(
            left,
            right,
            positions_after(0).into_iter(),
            merged_position,
            false,
        );
        assert!(
            poisoned.is_poisoned_for_test(),
            "a generic-resolving merge left retention live",
        );
        assert_eq!(
            poisoned.entry_rebuild_cause(&proof, &fixture.contacts),
            Some(RetainedLoadRebuildCause::Poisoned(
                PoisonCause::Guard3Generic
            )),
            "a poisoned carrier must answer every entry guard with a rebuild",
        );
        assert_eq!(
            poisoned.cardinality_rebuild_cause(&fixture.contacts, &fixture.contact_rows()),
            Some(RetainedLoadRebuildCause::Poisoned(
                PoisonCause::Guard3Generic
            )),
            "a poisoned carrier must answer the cardinality guard too",
        );
        assert_eq!(
            poisoned.poison_cause_for_test(),
            Some(PoisonCause::Guard3Generic),
            "FU-8g H4: a generic-resolving merge must attribute to Guard3Generic, \
             not collapse into the old single Poisoned label",
        );

        // Positive arm: the same commit, resolving specialized, is accepted.
        let mut live = fixture.retained();
        live.record_commit(
            left,
            right,
            positions_after(0).into_iter(),
            merged_position,
            true,
        );
        assert!(
            !live.is_poisoned_for_test(),
            "a specialized-resolving merge must not poison retention, or the \
             negative arm above proves nothing",
        );
    }

    /// Guard 4 (design D-G6-3, finding F-G6-5): the gate's own freshly-derived
    /// `contact_rows` must equal every retained bucket's length.  This is the
    /// free half of the content-drift surface — `contact_rows` is computed
    /// each round for `proposal_load_bound` regardless, so the cross-check
    /// adds no derivation at all.  It cannot see a *permuted* bucket (that is
    /// the G6b equivalence harness's job); it does see every cardinality
    /// drift, which is what a wrong merge, a skipped removal or a truncated
    /// union all produce.
    ///
    /// The positive control derives `contact_rows` **independently**, from
    /// `pairs` and the partition through `face_row`/`row_face` exactly as
    /// `build_gate_load_state` does, so it pins a real claim: the retained
    /// buckets agree with the gate's own derivation, contact for contact.
    /// That independence is not decoration — it is what caught the design
    /// record's F-G6-5 overstating `contact_rows` as *every* contact's
    /// eligible-row count, when a generic seat emits no `(owner, face)` pairs
    /// and yet holds a non-empty transpose bucket.  The scope of the guard,
    /// and the deviation, are recorded on
    /// [`RetainedLoadTranspose::cardinality_rebuild_cause`]; the arms below
    /// pin both halves of it.
    ///
    /// # Mutation proof
    ///
    /// Delete the `agrees` comparison (or the `buckets.len()` check) in
    /// [`RetainedLoadTranspose::cardinality_rebuild_cause`] and this test
    /// fails.  Transcript, run 2026-08-07 (mutation M4):
    /// `fu8f_retained_transpose_rebuilds_when_a_bucket_length_drifts ... FAILED`,
    /// `assertion 'left == right' failed: a retained bucket's length drifted
    /// from the gate's own contact_rows and the guard did not see it`.
    #[test]
    fn fu8f_retained_transpose_rebuilds_when_a_bucket_length_drifts() {
        let fixture = retention_fixture();
        let contact_rows = fixture.contact_rows();
        let mut retained = fixture.retained();

        assert_eq!(
            retained.bucket_count_for_test(),
            contact_rows.len(),
            "the retained carrier must hold one bucket per contact",
        );
        assert!(
            contact_rows[..fixture.specialized_seats]
                .iter()
                .all(|&rows| rows > 0),
            "every specialized seat must have a non-empty bucket, or the \
             cardinality guard is being proven against zero counts",
        );
        assert_eq!(
            retained.cardinality_rebuild_cause(&fixture.contacts, &contact_rows),
            None,
            "the retained buckets must agree with an INDEPENDENT derivation \
             of contact_rows",
        );

        // The half of F-G6-5 that is NOT true, pinned so a later phase cannot
        // "restore totality" without re-reading why: generic seats emit no
        // `(owner, face)` pairs, so their `contact_rows` entry is 0 while
        // their transpose bucket is not empty at all.
        for (contact, &rows) in contact_rows
            .iter()
            .enumerate()
            .skip(fixture.specialized_seats)
        {
            assert!(
                matches!(fixture.contacts[contact].owner, LoadOwner::GenericIsland(_)),
                "the fixture's tail must be the generic seats",
            );
            assert_eq!(
                rows, 0,
                "a generic seat emits no (owner, face) pairs, so the gate's \
                 contact_rows entry for it is 0",
            );
            assert!(
                !retained.bucket_rows_for_test(contact).is_empty(),
                "a generic seat's transpose bucket is the generic authority's \
                 rows and is NOT empty — this is exactly why guard 4 cannot \
                 be the total comparison F-G6-5 described",
            );
        }

        // A cardinality drift of exactly one row on a SPECIALIZED seat — the
        // smallest the guard is required to see, and the shape mutation (c) of
        // the G6b triad (dropping the last row of a merged bucket) produces.
        let drifting = (0..fixture.specialized_seats)
            .find(|&contact| contact_rows[contact] > 0)
            .expect("a non-empty specialized bucket to corrupt");
        retained.corrupt_bucket_len_for_test(drifting);
        assert_eq!(
            retained.cardinality_rebuild_cause(&fixture.contacts, &contact_rows),
            Some(RetainedLoadRebuildCause::BucketCardinality),
            "a retained bucket's length drifted from the gate's own \
             contact_rows and the guard did not see it",
        );

        // A whole contact's worth of drift — the length mismatch arm.
        let fresh = fixture.retained();
        assert_eq!(
            fresh.cardinality_rebuild_cause(
                &fixture.contacts,
                &contact_rows[..contact_rows.len() - 1],
            ),
            Some(RetainedLoadRebuildCause::BucketCardinality),
            "the retained bucket COUNT disagreed with contact_rows and the \
             guard did not see it",
        );
    }

    /// Every guard miss is a **counted** full rebuild (design D-G6-3): a miss
    /// that is not counted is a miss nobody can attribute after the fact, and
    /// the whole point of landing the guards before the fast path is that
    /// `retained_load_rebuilds == 0` on the corpus is then a live proof that
    /// the carrier stayed synchronized.
    ///
    /// Two things are pinned here that are easy to get backwards: the FIRST
    /// build of a tournament is not a miss (the slot was empty — there was
    /// nothing to reuse), and a round whose guards all passed is not a miss
    /// either, even though phase G6a still rebuilds on it (rebuild-always).
    /// The counter counts guard verdicts, not builds.
    ///
    /// # Mutation proof
    ///
    /// Replace `usize::from(miss)` with `0` in
    /// [`RetainedLoadTranspose::absorb_full_build`], or drop the
    /// `slot.is_some()` conjunct from `miss`, and this test fails.
    /// Transcript, run 2026-08-07 (mutation M5, `+ 0 * usize::from(miss)`):
    /// `fu8f_retained_transpose_counts_every_guard_miss_as_a_rebuild ... FAILED`,
    /// `assertion 'left == right' failed: a guard miss was not counted as a
    /// rebuild (MeshFingerprint)`.
    #[test]
    fn fu8f_retained_transpose_counts_every_guard_miss_as_a_rebuild() {
        let fixture = retention_fixture();
        let mut slot = None;

        // The first build of a tournament: nothing to reuse, so not a miss.
        fixture.absorb(&mut slot, None, &fixture.contacts);
        assert_eq!(
            slot.as_ref().unwrap().rebuild_count(),
            0,
            "the first build of a tournament is not a guard miss",
        );

        // A round whose guards all passed. G6a rebuilds anyway; still no miss.
        fixture.absorb(&mut slot, None, &fixture.contacts);
        assert_eq!(
            slot.as_ref().unwrap().rebuild_count(),
            0,
            "a round whose guards all passed is not a guard miss, even though \
             phase G6a rebuilds on it",
        );

        for (round, cause) in [
            RetainedLoadRebuildCause::MeshFingerprint,
            RetainedLoadRebuildCause::ContactWitness,
            RetainedLoadRebuildCause::BucketCardinality,
            RetainedLoadRebuildCause::Poisoned(PoisonCause::IdentityWalk),
        ]
        .into_iter()
        .enumerate()
        {
            fixture.absorb(&mut slot, Some(cause), &fixture.contacts);
            assert_eq!(
                slot.as_ref().unwrap().rebuild_count(),
                round + 1,
                "a guard miss was not counted as a rebuild ({cause:?})",
            );
        }
    }

    /// The remap itself (design D-G6-2), pinned before the fast path consumes
    /// it — because in phase G6a the ONLY consumer of `record_commit`'s output
    /// is the next round's guard chain, and a wrong remap would show up there
    /// as a counted rebuild rather than as wrong geometry.  That makes the
    /// corpus's `retained_load_rebuilds == 0` a real measurement, so what the
    /// remap claims has to be pinned directly.
    ///
    /// Three claims, all of which G6b's fast path will depend on:
    ///
    /// 1. the merged bucket is the ascending **dedup** union of its parents'
    ///    (seats 0 and 1 share faces, so this is a merge, not a concatenation);
    /// 2. the insertion point is derived by the identity walk, and an
    ///    **interior** one is handled — the parent design's riskiest claim,
    ///    since `next.sort_by(compare_contacts)` places the merged seat at its
    ///    sorted position and not last;
    /// 3. survivors' specialized owner ids are renumbered **positionally**
    ///    (`first_specialized_owner + index`), while generic owners are
    ///    position-independent and must not be renumbered at all.
    ///
    /// # Mutation proof
    ///
    /// This is the G6a-reachable member of the G6b mutation triad — triad
    /// member (a), "insert the merged bucket at `p + 1`".  Change
    /// `if next_buckets.len() == insert_at` to `insert_at + 1` in
    /// [`RetainedLoadTranspose::record_commit`] and this test fails.
    /// Transcript, run 2026-08-07 (mutation M6):
    /// `fu8f_retained_commit_remaps_the_witness_and_merges_the_parent_buckets ... FAILED`,
    /// `assertion 'left == right' failed: the merged bucket is not the
    /// ascending dedup union of the parents' retained buckets`.  Triad
    /// members (b) and (c) act on the retained *fast path*, which does not
    /// exist until G6b; they land with it, per the design's phase table.
    #[test]
    fn fu8f_retained_commit_remaps_the_witness_and_merges_the_parent_buckets() {
        let fixture = retention_fixture();
        let (left, right) = (0usize, 1usize);
        // Strictly interior: survivors are seats 2,3,4,5 then the generic
        // seats, and the merged seat lands between seats 3 and 4.
        const MERGED_AT: usize = 2;
        assert!(
            MERGED_AT > 0 && MERGED_AT + 1 < fixture.contacts.len() - 2,
            "the insertion point must be strictly interior, or the riskiest \
             claim in the design goes unpinned",
        );

        let before = fixture.retained();
        let expected_union = {
            let mut union = before.bucket_rows_for_test(left).to_vec();
            union.extend_from_slice(before.bucket_rows_for_test(right));
            union.sort_unstable();
            union.dedup();
            union
        };
        assert!(
            expected_union.len()
                < before.bucket_rows_for_test(left).len()
                    + before.bucket_rows_for_test(right).len(),
            "the parents must actually share rows, or the dedup arm of the \
             merge is never exercised",
        );
        let before_cells = before.cells_for_test();
        let before_witness = before.witness_for_test().to_vec();
        let before_buckets = (0..before.bucket_count_for_test())
            .map(|contact| before.bucket_rows_for_test(contact).to_vec())
            .collect::<Vec<_>>();

        let merged_position = [11.5, 12.5, 13.5];
        let mut positions_after = (0..fixture.contacts.len())
            .filter(|index| *index != left && *index != right)
            .map(|index| fixture.contacts[index].point_mm)
            .collect::<Vec<_>>();
        positions_after.insert(MERGED_AT, merged_position);

        let mut after = fixture.retained();
        after.record_commit(
            left,
            right,
            positions_after.iter().copied(),
            merged_position,
            true,
        );
        assert!(
            !after.is_poisoned_for_test(),
            "a well-formed commit must not poison retention",
        );

        // Claim 1: the merged bucket is the dedup union of its parents'.
        assert_eq!(
            after.bucket_rows_for_test(MERGED_AT),
            expected_union.as_slice(),
            "the merged bucket is not the ascending dedup union of the \
             parents' retained buckets",
        );
        assert_eq!(
            after.cells_for_test(),
            before_cells - before_buckets[left].len() - before_buckets[right].len()
                + expected_union.len(),
            "live cells did not follow the merge's own arithmetic",
        );
        assert_eq!(
            after.bucket_count_for_test(),
            before.bucket_count_for_test() - 1,
            "a commit removes two contacts and inserts one",
        );

        // Claim 2: survivors keep their bucket CONTENT, only their position
        // shifts — a commit touches no survivor's routes or demands (F-G6-1).
        let survivors = (0..fixture.contacts.len())
            .filter(|index| *index != left && *index != right)
            .collect::<Vec<_>>();
        for (rank, &old_index) in survivors.iter().enumerate() {
            let new_index = if rank < MERGED_AT { rank } else { rank + 1 };
            assert_eq!(
                after.bucket_rows_for_test(new_index),
                before_buckets[old_index].as_slice(),
                "survivor {old_index} -> {new_index} did not keep its bucket content",
            );
        }

        // Claim 3: positional renumbering of specialized owners, generic
        // owners untouched, and the merged seat taking the insertion point's
        // id — the `first_specialized_owner + index` construction.
        let witness = after.witness_for_test();
        assert_eq!(witness.len(), before_witness.len() - 1);
        assert_eq!(
            witness[MERGED_AT],
            (LoadOwner::Specialized(MERGED_AT as u32), merged_position),
            "the merged seat's witness is not the committed seat at its own \
             sorted position",
        );
        let mut saw_generic = false;
        for (rank, &old_index) in survivors.iter().enumerate() {
            let new_index = if rank < MERGED_AT { rank } else { rank + 1 };
            let (owner, point_mm) = witness[new_index];
            assert_eq!(
                position_bits(point_mm),
                position_bits(before_witness[old_index].1),
                "survivor {old_index} -> {new_index} moved, and a commit moves \
                 no survivor",
            );
            match before_witness[old_index].0 {
                LoadOwner::Specialized(_) => assert_eq!(
                    owner,
                    LoadOwner::Specialized(new_index as u32),
                    "specialized survivor {old_index} -> {new_index} was not \
                     renumbered positionally",
                ),
                LoadOwner::GenericIsland(island) => {
                    saw_generic = true;
                    assert_eq!(
                        owner,
                        LoadOwner::GenericIsland(island),
                        "a generic owner is position-independent and must not \
                         be renumbered",
                    );
                }
            }
        }
        assert!(
            saw_generic,
            "the fixture must carry generic seats past the insertion point",
        );
        assert!(
            fixture.specialized_seats > MERGED_AT,
            "some specialized survivor must sit past the insertion point, or \
             the renumbering arm is vacuous",
        );
    }

    /// The row invariants are exactly the build's own, and a committed merge
    /// does not touch them (design D-G6-1, finding F-G6-1).  This is the claim
    /// that lets a retained round skip `GroupedOwnerFaces`/
    /// `extend_load_eligibility` entirely in phase G6b — the rows it fills
    /// from are these — so it is pinned in G6a, while the full build still
    /// runs beside it and can be compared against.
    ///
    /// Row-set invariance across merges is FU-8d F3's ∪-argument and is not
    /// re-proved here; what is proved here is that the carrier *retains* the
    /// build's rows verbatim and that `record_commit` leaves them alone.
    #[test]
    fn fu8f_retained_row_invariants_are_the_builds_own_and_survive_a_commit() {
        let fixture = retention_fixture();
        let proof = fixture.proof();
        let (partition, baseline, _) = SpecializedLoadBaseline::build(
            &fixture.plan,
            &proof,
            &fixture.surface,
            &fixture.contacts,
            &fixture.pairs,
            retention_limits(),
        )
        .expect("the retention fixture must build");

        let mut carrier = fixture.retained();
        let (face_id, area_mm2, centroid_mm) = carrier.row_invariants_for_test();
        assert!(!face_id.is_empty(), "the fixture must produce rows");
        assert_eq!(
            face_id,
            partition
                .rows()
                .iter()
                .map(|row| row.face_id)
                .collect::<Vec<_>>(),
            "the retained face ids are not the build's own partition rows",
        );
        assert_eq!(
            area_mm2
                .iter()
                .map(|area| area.to_bits())
                .collect::<Vec<_>>(),
            baseline
                .row_area_mm2
                .iter()
                .map(|area| area.to_bits())
                .collect::<Vec<_>>(),
            "the retained row areas are not the build's own, bit for bit",
        );
        assert_eq!(
            centroid_mm
                .iter()
                .copied()
                .map(position_bits)
                .collect::<Vec<_>>(),
            baseline
                .row_centroid_mm
                .iter()
                .map(|centroid| position_bits(*centroid))
                .collect::<Vec<_>>(),
            "the retained row centroids are not the build's own, bit for bit",
        );

        let before = (face_id.to_vec(), area_mm2.to_vec(), centroid_mm.to_vec());
        let merged_position = [31.5, 32.5, 33.5];
        let mut positions_after = (2..fixture.contacts.len())
            .map(|index| fixture.contacts[index].point_mm)
            .collect::<Vec<_>>();
        positions_after.insert(0, merged_position);
        carrier.record_commit(0, 1, positions_after.into_iter(), merged_position, true);
        assert!(!carrier.is_poisoned_for_test());
        let (face_id, area_mm2, centroid_mm) = carrier.row_invariants_for_test();
        assert_eq!(face_id, before.0.as_slice());
        assert_eq!(
            area_mm2
                .iter()
                .map(|area| area.to_bits())
                .collect::<Vec<_>>(),
            before
                .1
                .iter()
                .map(|area| area.to_bits())
                .collect::<Vec<_>>(),
            "a committed merge moved a retained row area",
        );
        assert_eq!(
            centroid_mm
                .iter()
                .copied()
                .map(position_bits)
                .collect::<Vec<_>>(),
            before
                .2
                .iter()
                .map(|centroid| position_bits(*centroid))
                .collect::<Vec<_>>(),
            "a committed merge moved a retained row centroid",
        );
    }

    /// `record_commit` refuses to guess (design D-G6-2 step 1): anything other
    /// than "survivors in preserved relative order plus exactly one insertion"
    /// poisons the carrier, and the next round pays one counted full rebuild.
    /// The identity walk is deliberately not a comparator re-derivation, so
    /// code drift, an unstable-sort regression or a tie behaving unexpectedly
    /// all land here rather than in a silently wrong remap.
    ///
    /// # Mutation proof
    ///
    /// Each arm below names the refusal it exercises; delete that refusal in
    /// [`RetainedLoadTranspose::record_commit`] and the arm fails, leaving the
    /// carrier live with a remap it could not name.  Stated honestly: the
    /// terminal `if survivor.is_some()` check is **defense in depth and is not
    /// separately reachable** — with the length already pinned to
    /// "old minus one", a list that leaves a survivor unmatched must also
    /// diverge twice, which the second-divergence arm refuses first.  It is
    /// kept because it costs nothing and the walk's invariant should not
    /// depend on that arithmetic staying true, and it is recorded here as
    /// unproven rather than claimed as proven.
    #[test]
    fn fu8f_retained_commit_poisons_when_it_cannot_name_the_remap() {
        let fixture = retention_fixture();
        let (left, right) = (0usize, 1usize);
        let merged_position = [21.5, 22.5, 23.5];
        let survivors = || {
            (0..fixture.contacts.len())
                .filter(|index| *index != left && *index != right)
                .map(|index| fixture.contacts[index].point_mm)
                .collect::<Vec<_>>()
        };

        // (a) the committed list is not one shorter than the witnessed one:
        // the length check, before any walking happens.
        let mut dropped = survivors();
        dropped.insert(0, merged_position);
        dropped.remove(dropped.len() - 1);
        let mut carrier = fixture.retained();
        carrier.record_commit(left, right, dropped.into_iter(), merged_position, true);
        assert!(
            carrier.is_poisoned_for_test(),
            "a commit whose contact list is the wrong length did not poison retention",
        );
        // FU-8g H4 mutation proof: this arm is `PoisonCause::CommitShape`.
        // Delete `PoisonCause::CommitShape` from the `positions_after.len() +
        // 1 != old_len` arm of `record_commit` (or otherwise stop it from
        // being reached, e.g. by folding the length check into the identity
        // walk) and this assertion fails.
        assert_eq!(
            carrier.poison_cause_for_test(),
            Some(PoisonCause::CommitShape),
            "a wrong-length commit must attribute to CommitShape",
        );

        // (a2) the parents are not two distinct contacts of the witnessed
        // list: the bounds/distinctness check.
        for (left, right) in [(0usize, 0usize), (0, fixture.contacts.len())] {
            let mut carrier = fixture.retained();
            carrier.record_commit(left, right, survivors().into_iter(), merged_position, true);
            assert!(
                carrier.is_poisoned_for_test(),
                "a commit naming parents ({left}, {right}) did not poison retention",
            );
        }

        // (b) survivors reordered: not a stable-sort remap.
        let mut reordered = survivors();
        reordered.swap(0, 1);
        reordered.insert(0, merged_position);
        let mut carrier = fixture.retained();
        carrier.record_commit(left, right, reordered.into_iter(), merged_position, true);
        assert!(
            carrier.is_poisoned_for_test(),
            "a commit that reordered survivors did not poison retention",
        );

        // (c) the inserted seat is not the committed one.
        let mut foreign = survivors();
        foreign.insert(0, [99.0, 99.0, 99.0]);
        let mut carrier = fixture.retained();
        carrier.record_commit(left, right, foreign.into_iter(), merged_position, true);
        assert!(
            carrier.is_poisoned_for_test(),
            "a commit whose inserted seat is not the committed contact did \
             not poison retention",
        );
        // FU-8g H4 mutation proof: this arm is `PoisonCause::IdentityWalk`.
        // Delete the `bits != position_bits(committed_position_mm)` refusal
        // in the `_ if insertion.is_none()` arm of `record_commit` (or the
        // `PoisonCause::IdentityWalk` argument to its `self.poison(...)`
        // call) and this assertion fails.
        assert_eq!(
            carrier.poison_cause_for_test(),
            Some(PoisonCause::IdentityWalk),
            "a foreign inserted seat must attribute to IdentityWalk",
        );

        // (d) the parents are not both specialized: the union identity's other
        // precondition (F-G6-1).
        let generic_parent = fixture.specialized_seats;
        assert!(matches!(
            fixture.contacts[generic_parent].owner,
            LoadOwner::GenericIsland(_)
        ));
        let mut mixed = (0..fixture.contacts.len())
            .filter(|index| *index != left && *index != generic_parent)
            .map(|index| fixture.contacts[index].point_mm)
            .collect::<Vec<_>>();
        mixed.insert(0, merged_position);
        let mut carrier = fixture.retained();
        carrier.record_commit(
            left,
            generic_parent,
            mixed.into_iter(),
            merged_position,
            true,
        );
        assert!(
            carrier.is_poisoned_for_test(),
            "a merge with a generic parent did not poison retention",
        );
        // FU-8g H4 mutation proof: this arm is `PoisonCause::CommitShape`.
        // Delete the `LoadOwner::Specialized` match guard's `else` arm's
        // `PoisonCause::CommitShape` argument in `record_commit` and this
        // assertion fails.
        assert_eq!(
            carrier.poison_cause_for_test(),
            Some(PoisonCause::CommitShape),
            "a generic parent must attribute to CommitShape",
        );

        // (e) a poisoned carrier stays poisoned and answers every guard.
        assert_eq!(
            carrier.entry_rebuild_cause(&fixture.proof(), &fixture.contacts),
            Some(RetainedLoadRebuildCause::Poisoned(PoisonCause::CommitShape)),
        );
        carrier.record_commit(left, right, survivors().into_iter(), merged_position, true);
        assert!(
            carrier.is_poisoned_for_test(),
            "a poisoned carrier must stay poisoned until it is repopulated",
        );
    }

    // ----------------------------------------------------------------------
    // FU-8f phase G6b — the retained fast path and its equivalence proof.
    // ----------------------------------------------------------------------

    /// One tournament state of the G6b harness: the specialized seats as
    /// `(position, served faces)`, from which the physical list and pair list
    /// are rebuilt exactly as `append_variant_load_inputs` renumbers them —
    /// `contact_id = position`, specialized `owner = position`
    /// (`first_specialized_owner == 0`), generic tail owners
    /// position-independent.
    fn harness_inputs(
        fixture: &RetentionFixture,
        seats: &[([f64; 3], Vec<u32>)],
    ) -> (Vec<PhysicalContact>, Vec<(u32, u32)>) {
        let generic = &fixture.contacts[fixture.specialized_seats..];
        let contacts = seats
            .iter()
            .enumerate()
            .map(|(index, (point, _))| PhysicalContact {
                contact_id: index as u64,
                owner: LoadOwner::Specialized(index as u32),
                point_mm: *point,
            })
            .chain(
                generic
                    .iter()
                    .enumerate()
                    .map(|(slot, contact)| PhysicalContact {
                        contact_id: (seats.len() + slot) as u64,
                        owner: contact.owner,
                        point_mm: contact.point_mm,
                    }),
            )
            .collect::<Vec<_>>();
        let pairs = seats
            .iter()
            .enumerate()
            .flat_map(|(index, (_, served))| served.iter().map(move |&face| (index as u32, face)))
            .collect::<Vec<_>>();
        (contacts, pairs)
    }

    /// Guard 4's `contact_rows`, derived the way the retained path derives it
    /// in `build_gate_load_state`: `face_row`/`row_face` from the retained
    /// `row_face_id`, then each specialized contact's `(owner, face)` run
    /// walked through them.
    fn contact_rows_from_row_faces(
        row_face: &[u32],
        mesh_faces: usize,
        pairs: &[(u32, u32)],
        contact_count: usize,
    ) -> Vec<usize> {
        let mut face_row = vec![u32::MAX; mesh_faces];
        for (row, &face) in row_face.iter().enumerate() {
            if face_row[face as usize] == u32::MAX {
                face_row[face as usize] = row as u32;
            }
        }
        let mut counts = vec![0usize; contact_count];
        for &(owner, face_id) in pairs {
            let mut row = face_row[face_id as usize];
            if row == u32::MAX {
                continue;
            }
            while (row as usize) < row_face.len() && row_face[row as usize] == face_id {
                counts[owner as usize] += 1;
                row += 1;
            }
        }
        counts
    }

    /// FU-8f phase G6b — THE bit-identity harness (design D-G6-3's
    /// equivalence proof): after every one of N = 4 committed merges on real
    /// fixture geometry, the retained fast path's output is compared on
    /// `to_bits`, field by field, against BOTH the production fresh build
    /// (`SpecializedLoadBaseline::build`) and the retained
    /// [`reference_contacts_x_rows_scan`] oracle — the pre-FU-8f
    /// `contacts x rows` scan itself, not a restatement.
    ///
    /// The carrier is populated once, from the initial full build, and then
    /// crosses all four commits on `record_commit` alone — **nothing
    /// repopulates it between rounds**, which is the "actually retains across
    /// rounds" claim itself.  Every round first asserts that no guard fired
    /// (`entry_rebuild_cause` and `cardinality_rebuild_cause` both `None`):
    /// the mutation triad below is caught either by a guard firing here or by
    /// a bit mismatch in the comparisons, and this test states which.
    ///
    /// The merge schedule is chosen to pin the claims the corpus cannot:
    ///
    /// - rounds 1 and 3 merge parents at **nonzero** contact indices
    ///   ((2, 4) and (1, 3)) — verifier finding F-3 from G3–G5: every corpus
    ///   round observed so far accepted proposal 0 (the treant reaches
    ///   `safe = Some(2)` only at round 46), so nonzero indices must be
    ///   exercised by fixture;
    /// - rounds 1–3 insert the merged seat at a **strictly interior** sorted
    ///   position (the committed renumbering, distinct from FU-8d's
    ///   merged-LAST variant pin — both hold, see
    ///   `fu8f_committed_renumbering_matches_append_variant_load_inputs`);
    /// - rounds 2 and 4 merge parents with overlapping served faces, so the
    ///   merged bucket is a real dedup union, and rounds 2–4 merge
    ///   already-merged seats, so retained payloads feed later merges.
    ///
    /// # The mutation triad (design D-G6-3) — applied, caught, reverted
    ///
    /// Three deliberate corruptions of `record_commit`'s update, each applied
    /// as a source edit, run, observed to fail exactly as recorded below, and
    /// reverted (run 2026-08-07):
    ///
    /// - **(a) insert the merged bucket at `p + 1`** (`insert_at + 1` in both
    ///   remap insertion arms): caught by guard 2 — the remapped witness no
    ///   longer matches the fresh physical list —
    ///   `fu8f_retained_transpose_matches_a_fresh_rebuild_after_n_commits ...
    ///   FAILED`, `assertion 'left == right' failed: round 0: an entry guard
    ///   fired on a well-formed commit ... left: Some(ContactWitness)`.
    /// - **(b) leave `right`'s bucket in place** (skip the `old_index ==
    ///   right` removal): caught by guard 2's length arm — the witness keeps
    ///   `old_len` entries against `old_len - 1` fresh contacts — same
    ///   failure shape as (a): `round 0: an entry guard fired ... left:
    ///   Some(ContactWitness)`.
    /// - **(c) drop the last row of the merged bucket** (`merged_len - 1`):
    ///   caught by guard 4 — the merged bucket's cardinality disagrees with
    ///   the freshly-derived `contact_rows` —
    ///   `assertion 'left == right' failed: round 0: guard 4 fired on a
    ///   well-formed commit ... left: Some(BucketCardinality)`.
    ///
    /// A harness that stayed green under any of the three would be vacuous
    /// and must not land (the FU-8e/G1 rule); all three failed.
    #[test]
    fn fu8f_retained_transpose_matches_a_fresh_rebuild_after_n_commits() {
        let fixture = retention_fixture();
        let proof = fixture.proof();
        let mesh_faces = fixture.mesh.tris().len();

        // The evolving tournament state, seeded from the fixture's own seats.
        let served: [&[u32]; 6] = [
            &[0, 1, 2],
            &[0, 1, 3],
            &[2, 3, 4],
            &[4, 5],
            &[5, 6],
            &[6, 7],
        ];
        let mut seats = fixture.contacts[..fixture.specialized_seats]
            .iter()
            .zip(served)
            .map(|(contact, served)| (contact.point_mm, served.to_vec()))
            .collect::<Vec<_>>();

        // Round 0: the initial full build populates the carrier — the one and
        // only population this test performs.
        let (contacts, pairs) = harness_inputs(&fixture, &seats);
        let mut slot = None;
        {
            let (partition, baseline, transpose) = SpecializedLoadBaseline::build(
                &fixture.plan,
                &proof,
                &fixture.surface,
                &contacts,
                &pairs,
                retention_limits(),
            )
            .expect("the harness fixture must build");
            RetainedLoadTranspose::absorb_full_build(
                &mut slot, None, &proof, transpose, &partition, &baseline, &contacts,
            );
        }
        let mut carrier = slot.expect("absorb_full_build always populates the slot");

        // (left, right, insert_at): see the doc comment for why each round.
        let schedule = [(2usize, 4usize, 1usize), (0, 2, 2), (1, 3, 1), (0, 2, 1)];
        let mut saw_dedup_union = false;
        for (round, &(left, right, insert_at)) in schedule.iter().enumerate() {
            let merged_point = [50.0 + round as f64, 51.5, 52.25];
            let mut merged_served = seats[left].1.clone();
            merged_served.extend_from_slice(&seats[right].1);
            let concat_len = merged_served.len();
            merged_served.sort_unstable();
            merged_served.dedup();
            saw_dedup_union |= merged_served.len() < concat_len;
            let mut next = seats
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != left && *index != right)
                .map(|(_, seat)| seat.clone())
                .collect::<Vec<_>>();
            assert!(insert_at <= next.len());
            next.insert(insert_at, (merged_point, merged_served));

            let (new_contacts, new_pairs) = harness_inputs(&fixture, &next);
            // The identity walk resolves on position bits; the fixture must
            // not accidentally manufacture the bit-tie the hazard test covers.
            let distinct = new_contacts
                .iter()
                .map(|contact| position_bits(contact.point_mm))
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                distinct.len(),
                new_contacts.len(),
                "round {round}: positions collide"
            );

            carrier.record_commit(
                left,
                right,
                new_contacts.iter().map(|contact| contact.point_mm),
                merged_point,
                true,
            );
            assert!(
                !carrier.is_poisoned_for_test(),
                "round {round}: a well-formed commit poisoned retention",
            );

            // The guard chain must ACCEPT the round — a corruption that
            // forces a counted rebuild fails here, and the assertion names
            // which guard saw it (mutation triad arms (a)/(b) land on the
            // first, (c) on the second).
            assert_eq!(
                carrier.entry_rebuild_cause(&proof, &new_contacts),
                None,
                "round {round}: an entry guard fired on a well-formed commit",
            );
            let contact_rows = contact_rows_from_row_faces(
                carrier.row_face_ids(),
                mesh_faces,
                &new_pairs,
                new_contacts.len(),
            );
            assert_eq!(
                carrier.cardinality_rebuild_cause(&new_contacts, &contact_rows),
                None,
                "round {round}: guard 4 fired on a well-formed commit",
            );

            // The retained fast path's own output for this round.
            let (partition_retained, baseline_retained) = carrier
                .finalize_retained_round(&fixture.plan, &proof, &new_contacts)
                .expect("the retained round must finalize");

            // The production fresh build, and the retained reference oracle.
            let (partition_fresh, baseline_fresh, transpose_fresh) =
                SpecializedLoadBaseline::build(
                    &fixture.plan,
                    &proof,
                    &fixture.surface,
                    &new_contacts,
                    &new_pairs,
                    retention_limits(),
                )
                .expect("the fresh rebuild must build");
            let grouped = GroupedOwnerFaces::from_pairs(&new_pairs);
            let extended = extend_load_eligibility(
                &fixture.plan.load_eligibility,
                mesh_faces,
                &fixture.surface,
                &grouped,
                &new_pairs,
            )
            .expect("the oracle's extended eligibility must build");
            let (oracle_nearest, oracle_witnesses) =
                reference_contacts_x_rows_scan(&extended, &new_contacts);

            // Transpose buckets: payload and order, against the fresh CSR.
            assert_eq!(
                carrier.bucket_count_for_test(),
                new_contacts.len(),
                "round {round}: bucket count",
            );
            assert_eq!(
                carrier.cells_for_test(),
                transpose_fresh.rows.len(),
                "round {round}: live cells",
            );
            for contact in 0..new_contacts.len() {
                assert_eq!(
                    carrier.bucket_rows_for_test(contact),
                    transpose_fresh.rows_of(contact),
                    "round {round}: contact {contact}'s retained bucket is not the fresh CSR's",
                );
            }

            // Partition rows, field by field, areas on bits.
            assert_eq!(
                partition_retained.rows.len(),
                partition_fresh.rows.len(),
                "round {round}: row count",
            );
            for (row, (retained, fresh)) in partition_retained
                .rows
                .iter()
                .zip(&partition_fresh.rows)
                .enumerate()
            {
                assert_eq!(
                    (
                        retained.face_id,
                        retained.contact_id,
                        retained.owner,
                        retained.carried_area_mm2.to_bits(),
                    ),
                    (
                        fresh.face_id,
                        fresh.contact_id,
                        fresh.owner,
                        fresh.carried_area_mm2.to_bits(),
                    ),
                    "round {round}: partition row {row} diverged",
                );
            }
            assert_eq!(
                partition_retained.contacts_fingerprint, partition_fresh.contacts_fingerprint,
                "round {round}: contacts fingerprint",
            );
            assert_eq!(
                partition_retained.authority, partition_fresh.authority,
                "round {round}: authority",
            );

            // The baseline: every field the delta path reads, on bits, against
            // the fresh build AND (for the scan outputs) the oracle.
            for (row, ((retained, fresh), oracle)) in baseline_retained
                .witnesses
                .iter()
                .zip(&baseline_fresh.witnesses)
                .zip(&oracle_witnesses)
                .enumerate()
            {
                for slot in 0..LOAD_WITNESS_SLOTS {
                    let bits = |witness: &LoadWitness| {
                        (witness.contact, witness.squared_distance.to_bits())
                    };
                    assert_eq!(
                        bits(&retained[slot]),
                        bits(&fresh[slot]),
                        "round {round}: row {row} witness slot {slot} vs fresh",
                    );
                    assert_eq!(
                        bits(&retained[slot]),
                        bits(&oracle[slot]),
                        "round {round}: row {row} witness slot {slot} vs the oracle",
                    );
                }
                let (oracle_contact, oracle_distance) =
                    oracle_nearest[row].expect("every fixture row has a holder");
                assert_eq!(
                    (
                        baseline_retained.winner[row],
                        baseline_retained.witnesses[row][0]
                            .squared_distance
                            .to_bits(),
                    ),
                    (oracle_contact as u32, oracle_distance.to_bits()),
                    "round {round}: row {row}'s argmin is not the oracle's",
                );
            }
            assert_eq!(
                baseline_retained.winner, baseline_fresh.winner,
                "round {round}: winner",
            );
            assert_eq!(
                baseline_retained.won_starts, baseline_fresh.won_starts,
                "round {round}: won_starts",
            );
            assert_eq!(
                baseline_retained.won_rows, baseline_fresh.won_rows,
                "round {round}: won_rows",
            );
            let bits_of = |values: &[f64]| values.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
            assert_eq!(
                bits_of(&baseline_retained.carried_area_mm2),
                bits_of(&baseline_fresh.carried_area_mm2),
                "round {round}: carried areas per contact",
            );
            assert_eq!(
                baseline_retained.violation_count, baseline_fresh.violation_count,
                "round {round}: violation count",
            );
            assert_eq!(
                bits_of(&baseline_retained.row_area_mm2),
                bits_of(&baseline_fresh.row_area_mm2),
                "round {round}: row areas",
            );
            assert_eq!(
                baseline_retained
                    .row_centroid_mm
                    .iter()
                    .copied()
                    .map(position_bits)
                    .collect::<Vec<_>>(),
                baseline_fresh
                    .row_centroid_mm
                    .iter()
                    .copied()
                    .map(position_bits)
                    .collect::<Vec<_>>(),
                "round {round}: row centroids",
            );

            seats = next;
        }
        assert!(
            saw_dedup_union,
            "no round exercised a real dedup union, so the merge arm is untested",
        );
        assert_eq!(
            carrier.rebuild_count(),
            0,
            "no round of this harness may pay a rebuild",
        );
    }

    /// FU-8f phase G6c (D-G6-4) — the charge-vs-meter invariant, pinned as a
    /// test rather than left to the production `debug_assert!`s alone
    /// (`build_gate_load_state` and `evaluate_gate_batch` in `specialized.rs`
    /// both check it too, on the real gate state, but a `debug_assert!`
    /// compiles out of release — the same disposition the G3 executed/charged
    /// invariant took, see the "Appended 2026-08-07" note on that assert).
    ///
    /// One committed merge (round 0 of the equivalence harness's own
    /// schedule, a real dedup union: seats 2 and 4 share no face, so this
    /// also exercises the concatenation arm rather than only the dedup one
    /// the sibling test names): [`RetainedLoadTranspose::executed_operations`]
    /// on the retained round is compared against
    /// [`specialized_load_resource_plan`] — the SAME function
    /// `build_gate_load_state` calls for the round's CHARGE — on the round's
    /// own shape. The executed figure must never exceed it, because a
    /// retained round is a strict subset of what a full build of the same
    /// shape would do (F-G6-2/F-G6-3: the two eligibility traversals never
    /// run on this path at all).
    #[test]
    fn fu8f_g6c_retained_round_executes_no_more_than_it_is_charged() {
        let fixture = retention_fixture();
        let proof = fixture.proof();
        let mesh_faces = fixture.mesh.tris().len();
        let served: [&[u32]; 6] = [
            &[0, 1, 2],
            &[0, 1, 3],
            &[2, 3, 4],
            &[4, 5],
            &[5, 6],
            &[6, 7],
        ];
        let seats = fixture.contacts[..fixture.specialized_seats]
            .iter()
            .zip(served)
            .map(|(contact, served)| (contact.point_mm, served.to_vec()))
            .collect::<Vec<_>>();
        let (contacts, pairs) = harness_inputs(&fixture, &seats);
        let mut slot = None;
        {
            let (partition, baseline, transpose) = SpecializedLoadBaseline::build(
                &fixture.plan,
                &proof,
                &fixture.surface,
                &contacts,
                &pairs,
                retention_limits(),
            )
            .expect("the harness fixture must build");
            RetainedLoadTranspose::absorb_full_build(
                &mut slot, None, &proof, transpose, &partition, &baseline, &contacts,
            );
        }
        let mut carrier = slot.expect("absorb_full_build always populates the slot");

        // Round 0 of the sibling equivalence test's own schedule: merge
        // seats 2 and 4, inserted at position 1 — proven well-formed there,
        // reused here rather than re-derived.
        let (left, right, insert_at) = (2usize, 4usize, 1usize);
        let merged_point = [50.0, 51.5, 52.25];
        let mut merged_served = seats[left].1.clone();
        merged_served.extend_from_slice(&seats[right].1);
        merged_served.sort_unstable();
        merged_served.dedup();
        let mut next = seats
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != left && *index != right)
            .map(|(_, seat)| seat.clone())
            .collect::<Vec<_>>();
        next.insert(insert_at, (merged_point, merged_served));
        let (new_contacts, new_pairs) = harness_inputs(&fixture, &next);

        carrier.record_commit(
            left,
            right,
            new_contacts.iter().map(|contact| contact.point_mm),
            merged_point,
            true,
        );
        assert!(
            !carrier.is_poisoned_for_test(),
            "a well-formed commit must not poison retention",
        );
        assert_eq!(
            carrier.entry_rebuild_cause(&proof, &new_contacts),
            None,
            "an entry guard fired on a well-formed commit",
        );
        let contact_rows = contact_rows_from_row_faces(
            carrier.row_face_ids(),
            mesh_faces,
            &new_pairs,
            new_contacts.len(),
        );
        assert_eq!(
            carrier.cardinality_rebuild_cause(&new_contacts, &contact_rows),
            None,
            "guard 4 fired on a well-formed commit",
        );

        // The round is retained: `build_gate_load_state` would take this
        // exact branch. Meter it, and price the SAME round's full-build
        // charge from the SAME function `build_gate_load_state` calls.
        let executed = carrier
            .executed_operations(new_contacts.len())
            .expect("the meter must not overflow on a fixture-sized round");
        let charged = specialized_load_resource_plan(
            &fixture.plan,
            new_contacts.len(),
            SpecializedLoadCounts::bounded(new_pairs.len(), mesh_faces),
        )
        .expect("the charge must not overflow on a fixture-sized round")
        .operations;
        assert!(
            executed > 0,
            "a non-empty retained round must meter SOME work",
        );
        assert!(
            executed <= charged,
            "a retained round executed {executed} operations against a charge of {charged}",
        );

        // The retained carrier's own held-bytes ceiling must be computable
        // and, on this tiny fixture, dwarfed by the physical-contact list
        // alone would be on a real gate batch — a sanity bound, not a pinned
        // figure (the design's own ~66 MB estimate is treant-scale, this
        // fixture is not).
        let retained_bytes = carrier
            .retained_bytes()
            .expect("the byte ceiling must not overflow on a fixture-sized round");
        assert!(retained_bytes > 0, "a populated carrier holds SOME bytes");
    }

    /// FU-8f phase G6b — the identity walk's position-key hazard, CLOSED
    /// (recorded 2026-08-07 in CURRENT-STATUS by G6a's adversarial
    /// re-verification as a G6b precondition).
    ///
    /// `compare_contacts` orders by position and then by four tie-breakers,
    /// so a committed seat may carry position bits identical to a survivor's.
    /// The walk then cannot tell "this slot is the survivor" from "this slot
    /// is the committed seat" — the sort's tie-breakers decided, and the walk
    /// deliberately does not re-derive them.  G6a's walk consumed the
    /// survivor and silently derived `p + 1`; harmless under rebuild-always,
    /// wrong buckets under G6b's fast path.  The closure: the ambiguity is
    /// detected and POISONED — one counted rebuild, never a guess.  Both
    /// sort orders of the tied pair are asserted, because the walk cannot
    /// know which one the comparator chose — that is the ambiguity.
    ///
    /// # Mutation proof
    ///
    /// Delete the `insertion.is_none() && bits ==
    /// position_bits(committed_position_mm)` refusal inside the
    /// survivor-match arm of [`RetainedLoadTranspose::record_commit`] and
    /// both tied arms fail (`a commit whose seat ties a survivor's position
    /// bits was not refused`), while the positive arm keeps passing —
    /// transcript in the G6b landing commit.
    #[test]
    fn fu8f_retained_commit_poisons_on_a_position_bit_tie_at_the_insertion_point() {
        let fixture = retention_fixture();
        let (left, right) = (0usize, 1usize);
        // The committed seat lands ON survivor seat 2's position, bit for bit.
        let tied_position = fixture.contacts[2].point_mm;
        let survivors = (0..fixture.contacts.len())
            .filter(|index| *index != left && *index != right)
            .map(|index| fixture.contacts[index].point_mm)
            .collect::<Vec<_>>();

        for merged_at in [0usize, 1usize] {
            let mut positions_after = survivors.clone();
            positions_after.insert(merged_at, tied_position);
            let mut carrier = fixture.retained();
            carrier.record_commit(
                left,
                right,
                positions_after.into_iter(),
                tied_position,
                true,
            );
            assert!(
                carrier.is_poisoned_for_test(),
                "a commit whose seat ties a survivor's position bits was not \
                 refused (merged_at {merged_at})",
            );
            assert_eq!(
                carrier.entry_rebuild_cause(&fixture.proof(), &fixture.contacts),
                Some(RetainedLoadRebuildCause::Poisoned(
                    PoisonCause::IdentityWalk
                )),
                "the tie must surface as a counted rebuild",
            );
            // FU-8g H4 mutation proof: this is `PoisonCause::IdentityWalk`.
            // Delete the `PoisonCause::IdentityWalk` argument from the
            // position-bit-tie `self.poison(...)` call in `record_commit`
            // and this assertion fails.
            assert_eq!(
                carrier.poison_cause_for_test(),
                Some(PoisonCause::IdentityWalk),
                "the position-bit tie must attribute to IdentityWalk",
            );
        }

        // Positive control: the same commit with a distinct position stays
        // live, so the refusal above is the tie's and not unconditional.
        let mut positions_after = survivors;
        let distinct = [41.0, 42.0, 43.0];
        positions_after.insert(0, distinct);
        let mut carrier = fixture.retained();
        carrier.record_commit(left, right, positions_after.into_iter(), distinct, true);
        assert!(
            !carrier.is_poisoned_for_test(),
            "a distinct committed position must not trip the tie refusal",
        );
    }

    /// FU-8f phase G6b — arena compaction (design D-G6-2 step 4): the arena
    /// compacts exactly when dead payload exceeds live cells (`arena >
    /// 2 x cells`), to exactly `cells`, preserving every bucket's content and
    /// order — and does NOT compact below that threshold, which is what makes
    /// the copy amortized rather than per-commit.
    ///
    /// Built on [`RetainedLoadTranspose::from_witness_for_test`] so the
    /// bucket sizes are chosen to straddle the threshold; the content claims
    /// against real builds are the harness's job, not this test's.
    #[test]
    fn fu8f_retained_arena_compacts_at_twice_the_live_cells() {
        let contacts = (0..3u32)
            .map(|index| PhysicalContact {
                contact_id: u64::from(index),
                owner: LoadOwner::Specialized(index),
                point_mm: [f64::from(index), 0.0, 0.0],
            })
            .collect::<Vec<_>>();
        let wide = (0u32..8).collect::<Vec<_>>();
        let high = (8u32..16).collect::<Vec<_>>();
        let mut carrier = RetainedLoadTranspose::from_witness_for_test(
            &contacts,
            vec![wide.clone(), wide.clone(), high.clone()],
        );
        assert_eq!(carrier.cells_for_test(), 24);
        assert_eq!(carrier.arena_len_for_test(), 24);

        // Commit 1: merge contacts 0 and 1 (identical buckets, union = 8
        // rows).  Arena 24 + 8 = 32 against cells 16: NOT past 2x, so no
        // compaction — the trigger is a threshold, not every commit.
        let merged_first = [10.0, 0.0, 0.0];
        carrier.record_commit(
            0,
            1,
            [merged_first, [2.0, 0.0, 0.0]].into_iter(),
            merged_first,
            true,
        );
        assert!(!carrier.is_poisoned_for_test());
        assert_eq!(carrier.cells_for_test(), 16);
        assert_eq!(
            carrier.arena_len_for_test(),
            32,
            "below the 2x threshold the arena must NOT compact",
        );
        assert_eq!(carrier.bucket_rows_for_test(0), wide.as_slice());
        assert_eq!(carrier.bucket_rows_for_test(1), high.as_slice());

        // Commit 2: merge the two survivors (disjoint buckets, union = 16
        // rows).  Arena 32 + 16 = 48 against cells 16: past 2x, compacts to
        // exactly `cells`, content and order preserved.
        let merged_second = [11.0, 0.0, 0.0];
        carrier.record_commit(0, 1, [merged_second].into_iter(), merged_second, true);
        assert!(!carrier.is_poisoned_for_test());
        assert_eq!(carrier.cells_for_test(), 16);
        assert_eq!(
            carrier.arena_len_for_test(),
            16,
            "past the 2x threshold the arena must compact to the live cells",
        );
        let full = (0u32..16).collect::<Vec<_>>();
        assert_eq!(carrier.bucket_rows_for_test(0), full.as_slice());
        assert_eq!(
            carrier.witness_for_test(),
            &[(LoadOwner::Specialized(0), merged_second)],
        );
    }
}
