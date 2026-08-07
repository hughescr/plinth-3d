//! Bound, batch-first consolidation of current minimum and island-birth seats.
//!
//! Generic perception owns every geometric observation.  This module accepts
//! only its opaque plan plus an opaque regular-contact selection, validates
//! both against the complete authority matrix, and returns an opaque physical
//! contact plan.  Logical obligations remain distinct from physical seats.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use geom_kernel::{
    knn_single_source_planned_into, KnnBatchLimits, KnnBatchPlan, NativeSurface,
    NativeSurfaceConfig, PointCloudIndex, RadiusQueryLimits,
};
use plinth_core::{m0_constants, LayerPlan};
use plinth_mesh::{build_face_topology, mesh_fingerprint, Mesh, WeldPolicy};
use thiserror::Error;

use crate::dyadic::{build_candidate_payload, CandidatePayloadConfig, DyadicError};
use crate::perception::{
    finalize_specialized_load_partition, load_violation_count, specialized_load_resource_plan,
    FinalizedContactLoads, FinalizedLoadPartition, GenericContactPlan, GenericPerceptionError,
    GenericPerceptionLimits, LoadFinalizeLimits, LoadOwner, PhysicalContact, SliceSolid,
    SpecializedLoadBaseline, SpecializedLoadCounts, SpecializedLoadDeltaScratch,
    SpecializedLoadDeltaWork, SpecializedOwnerReservation,
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
    pub max_gate_operations_floor: usize,
    pub max_gate_window_variants: usize,
    pub max_coverage_scratch_bytes: usize,
    pub max_coverage_operations: usize,
    pub max_load_scratch_bytes: usize,
    pub max_load_operations_floor: usize,
    pub max_gate_peak_bytes: usize,
    /// `SUPPORT_GATE_MAX_WORK_PER_BATCH_UNIT`. Never the whole cap on its own;
    /// see [`SpecializedLimits::gate_operation_limit`].
    pub gate_work_per_batch_unit: usize,
    /// `SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT`. Never the whole cap on its own;
    /// see [`SpecializedLimits::load_operation_limit`].
    pub load_work_per_batch_unit: usize,
}

/// The two `Derived` batch-product coefficients, read from the one M0
/// publication so this crate invents no second numeric owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpecializedBatchWork {
    /// `SUPPORT_GATE_MAX_WORK_PER_BATCH_UNIT`.
    pub gate_per_batch_unit: usize,
    /// `SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT`.
    pub load_per_batch_unit: usize,
}

/// The registered coefficients, for tests that build production limits.
///
/// Tests read the *registry*, never a literal, so a coefficient change moves
/// the tests with it rather than leaving a second numeric owner behind.
#[cfg(test)]
pub(crate) fn test_batch_work() -> SpecializedBatchWork {
    SpecializedBatchWork::registered().expect("registered support batch coefficients")
}

impl SpecializedBatchWork {
    /// Reads both coefficients from the registered support resource-cap policy.
    ///
    /// # Errors
    ///
    /// Returns the registry's own construction error, or
    /// [`SpecializedError::UnrepresentableBatchWork`] if a registered
    /// coefficient does not fit a `usize` on this target.
    pub fn registered() -> Result<Self, SpecializedError> {
        let caps = plinth_core::m0_constants()
            .map_err(|error| SpecializedError::RegisteredConstantsUnavailable(error.to_string()))?
            .support_resource_cap_policy();
        let gate_per_batch_unit = usize::try_from(*caps.gate_max_work_per_batch_unit().value())
            .map_err(|_| SpecializedError::UnrepresentableBatchWork)?;
        let load_per_batch_unit = usize::try_from(*caps.load_max_work_per_batch_unit().value())
            .map_err(|_| SpecializedError::UnrepresentableBatchWork)?;
        Ok(Self {
            gate_per_batch_unit,
            load_per_batch_unit,
        })
    }
}

/// Materializes one batch-product-scaled cap: `max(floor, per_unit × units)`,
/// where `units = (rows + 1) × (columns + 1)`.
///
/// Saturating, so an absurd batch yields `usize::MAX` rather than a
/// wrapped-around tiny cap. The `+ 1` on each factor keeps the product
/// non-zero for an empty batch, so the floor always governs there.
#[must_use]
pub(crate) const fn batch_product_scaled_cap(
    floor: usize,
    per_unit: usize,
    rows: usize,
    columns: usize,
) -> usize {
    let units = rows
        .saturating_add(1)
        .saturating_mul(columns.saturating_add(1));
    let scaled = per_unit.saturating_mul(units);
    if scaled > floor {
        scaled
    } else {
        floor
    }
}

impl SpecializedLimits {
    /// The gate operation cap **for one gate batch**, materialized against that
    /// batch's own shape: `max(floor, K_G × (coverage_samples + 1) × (contacts + 1))`.
    ///
    /// This is a **declared model-scaled cap**. It is not a constant, and it is
    /// deliberately not scaled on the face count: gate work was measured not to
    /// track faces (24-face `make_raking_blade` charges 612,887,212 gate
    /// operations against the 429,120-face Weeping Treant's 12,714,828,544), so
    /// a per-face rule would refuse the small model and admit anything on the
    /// large one. What it still refuses is work that is superlinear in the
    /// batch: the treant's 39,223-proposal merge gate planned 21,755,124,103,146
    /// operations on a 553,835,949-unit batch — 613× this coefficient — and was
    /// refused (FU-8c).
    ///
    /// **Corrected 2026-08-06 (FU-8c fixed) — the sentence above is left
    /// standing, moved into the past tense it now belongs in.** That plan was
    /// the gate re-evaluating the whole contact set once per merge proposal, and
    /// the gate no longer does it; see [`GateBatchIndex`]. The same treant merge
    /// batch is now priced at 554,926,572 routed rows plus 4,618,272,984 face
    /// rows = 5,173,199,556, inside the 35,445,500,736 this cap materializes for
    /// that shape. **FU-8c did not change this cap**: the demand came down to
    /// it, which is what the example is still here to illustrate.
    ///
    /// **Correction added 2026-08-06 to the paragraph immediately above — it is
    /// left standing, and it is incomplete in a way that reads as the opposite
    /// of the measurement.** "Now priced at … 5,173,199,556, inside the
    /// 35,445,500,736" states a *floor*, not the treant's price, and standing
    /// beside the cap the way it does it invites the inference that the treant
    /// is now admitted at this gate. **It is not. The treant still refuses
    /// here.** 5,173,199,556 is the repriced plan's two *count-derived* terms —
    /// `baseline_rows` 554,926,572 + `face_rows` 4,618,272,984 — evaluated with
    /// the third term, `measured proposal work`, at **zero**; that is exactly
    /// what `fu8c_repriced_plan_admits_the_treant_gate_shape_under_the_unchanged_cap`
    /// asserts, and its name is exact: the *shape* fits. The real batch supplies
    /// real proposal work, so the demand this cap is handed is
    ///
    /// ```text
    /// baseline_rows 554,926,572 + face_rows 4,618,272,984
    ///   + demand_rows 42,949,185 + proposal work 196,264,386,588
    ///   = 201,480,535,329   against 35,445,500,736  -> refused, 5.68x over
    /// ```
    ///
    /// — 363.8× this coefficient's linear budget on the same 553,835,949-unit
    /// batch, measured at `a3308ec` and reproduced byte-identically at
    /// `8695398`. FU-8c is **closed** on the defect it named — the per-proposal
    /// re-evaluation is gone and the demand fell 108×, from 21,755,124,103,146
    /// to 201,480,535,329, against this **unchanged** cap — but the treant's
    /// stopping point did not move: it is still this gate. The residual
    /// 196,264,386,588 (97.4 % of the demand) is a *different* defect, opened as
    /// **FU-8d**. Read 5,173,199,556 as the floor of the repriced plan, never as
    /// the treant's price. See
    /// `docs/rust-migration/support-resource-caps-2026-08-05.md` §7 and
    /// `docs/rust-migration/CURRENT-STATUS.md` (*FU-8c fixed 2026-08-06*).
    ///
    /// **Dated addendum 2026-08-06 — FU-8d phase F2. The block above is left
    /// standing exactly as written; every number in it was true when measured
    /// and two of them have since moved.** F2 replaced the per-proposal
    /// `sources × samples` re-sweep with a merged-seat single batch read
    /// against the baseline's retained witnesses. The `proposal work` line is
    /// now `nearest_work 1,041,544,686 + touched_rows 1,041,544,686 +
    /// key_visits 78,446` = **2,083,167,818** where it was 196,264,386,588 —
    /// **94.2× less** — and the demand this cap is handed is
    ///
    /// ```text
    /// baseline_rows 554,926,572 + face_rows 4,618,272,984
    ///   + demand_rows 42,949,185 + proposal work 2,083,167,818
    ///   = 7,299,316,559   against 35,445,500,736  -> FITS (20.6 % of cap)
    /// ```
    ///
    /// **The cap did not move and no cap code was touched.** The treant still
    /// refuses at this gate, but no longer *here*: `gate_resource_plan` now
    /// passes, and the refusal comes from the `exact_operations` fold, where
    /// `variant_specialized_owner_face_count`'s descriptor budget crosses this
    /// same limit at **variant 275 of 39,224**, printing
    /// `required: 35445618202, limit: 35445500736`. That residual is FU-8d
    /// part (2) — the per-variant load/owner-face path — and it is what phase
    /// F4 is for. See `support-resource-caps-2026-08-05.md` §9 and
    /// `CURRENT-STATUS.md` (*FU-8d phase F2 landed 2026-08-06*).
    ///
    /// **Dated addendum 2026-08-06 — FU-8d phases F3 and F4. Every block above
    /// is left standing exactly as written; the F2 block's closing claim is the
    /// one that has stopped being true.** F3 finalizes the load partition
    /// **once** per batch rather than once per variant, deletes the per-variant
    /// window matrices, collapses the four per-proposal planning passes to one
    /// behind a count-derived pre-check, and replaces the gate's hand-built
    /// `variants × faces` load model with `specialized_load_resource_plan`'s own
    /// figure. The descriptor scan that crossed at variant 275 falls to
    /// **136,791,451 charged once**, and `unique_face_finalizer_work` ceases to
    /// exist as a term. The demand this cap is handed becomes
    ///
    /// ```text
    /// baseline_rows 554,926,572 + load 6,592,813,742
    ///   + demand_rows 42,949,185 + proposal work 2,083,167,818
    ///   = 9,273,857,317   against 35,445,500,736  -> FITS (26.2 % of cap)
    /// ```
    ///
    /// with `exact_operations` 10,457,910,996 (29.5 %). **The treant's merge
    /// gate no longer refuses at all**: it executes the 39,224-variant batch,
    /// accepts a merge and opens the next. Read "the treant still refuses at
    /// this gate" above as history — the standing figure is a demand that
    /// *passes*. **The cap still did not move and no cap code was touched by
    /// F3 or F4.** What stops the treant now is wall-clock rather than a limit:
    /// F4's clean run at this tip is an EXCEEDS-TIMEBOX at 600 s, and its
    /// successor defect is opened as **FU-8e**. See
    /// `support-resource-caps-2026-08-05.md` §10 and §11, and
    /// `CURRENT-STATUS.md` (*FU-8d phase F3 landed 2026-08-06*, *FU-8d phase F4
    /// measured 2026-08-06*).
    ///
    /// **Dated addendum 2026-08-07 — FU-8e phases E1–E4 landed. Every block
    /// above is left standing, and this one exists to say that nothing in them
    /// moved.** FU-8e removed the gate's per-proposal kd-tree construction (E1's
    /// [`knn_single_source_planned_into`], argued bit-identical to the tree's
    /// answer in five clauses) and its per-variant `assess_coverage` (E3's
    /// retained baseline objective). **The demand this limit is handed is
    /// unchanged at 9,273,857,317**, because `plan_proposal_nearest_delta` is
    /// untouched: `work()` is still `1 × touched_rows`, and the treant's
    /// `touched_rows` re-measured at **1,041,544,686, to the row**. E3 adds the
    /// one new charge in the loop — a fallback variant's
    /// `coverage_assessment_resource_plan_for(samples).operations`, spent into
    /// the same `load_delta_limit` headroom `executed_load_work` draws on and
    /// checked as their **sum**; on the treant it is zero, because zero variants
    /// fell back. **The cap still did not move and no cap code was touched.**
    /// What stops the treant is still wall-clock: EXCEEDS-TIMEBOX at 601 s, now
    /// 33 merge rounds of an estimated ~460, so **ruling R-f remains
    /// unsatisfied**. See `support-resource-caps-2026-08-05.md` §12 and
    /// `CURRENT-STATUS.md` (*FU-8e phases E1–E4 landed 2026-08-07*).
    ///
    /// **Ruling note 2026-08-06 on the 12,714,828,544 quoted at the top of this
    /// comment.** It is a dated FU-8b-era charge for the treant's proposal-free
    /// gate batch, quoted here only for the face-count spread it demonstrates
    /// against `make_raking_blade`, and that use is unaffected. As an *input* it
    /// is plausibly superseded by F3's repricing, and no re-measurement of it
    /// exists at this tip. The primary record is the ruling note on
    /// `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED`
    /// (`plinth-core/src/constants/evidence.rs`) and
    /// `support-resource-caps-2026-08-05.md` §7.1.
    ///
    /// Every corpus model sits at the floor;
    /// `the_batch_scaled_caps_are_the_flat_floor_for_every_corpus_batch_shape`
    /// (`crates/plinth-support/tests/resource_cap_policy.rs`) pins that.
    #[must_use]
    pub const fn gate_operation_limit(&self, coverage_samples: usize, contacts: usize) -> usize {
        batch_product_scaled_cap(
            self.max_gate_operations_floor,
            self.gate_work_per_batch_unit,
            coverage_samples,
            contacts,
        )
    }

    /// The load operation cap **for one load partition**, materialized against
    /// that partition's own shape:
    /// `max(floor, K_L × (rows + 1) × (contacts + 1))`.
    ///
    /// A **declared model-scaled cap** on the same terms as
    /// [`Self::gate_operation_limit`]. `rows` is the row count the partition's
    /// own resource plan was computed from — one row per distinct mesh face,
    /// never one per `(owner, face)` pair (FU-8b defect B) — so the cap and the
    /// demand it bounds are always stated over the same batch.
    #[must_use]
    pub const fn load_operation_limit(&self, rows: usize, contacts: usize) -> usize {
        batch_product_scaled_cap(
            self.max_load_operations_floor,
            self.load_work_per_batch_unit,
            rows,
            contacts,
        )
    }

    #[must_use]
    pub const fn production(
        coverage_route_knn: KnnBatchLimits,
        batch_work: SpecializedBatchWork,
    ) -> Self {
        Self {
            gate_work_per_batch_unit: batch_work.gate_per_batch_unit,
            load_work_per_batch_unit: batch_work.load_per_batch_unit,
            // FU-8d ruling R-b (2026-08-06).  The merge gate's baseline
            // coverage sweep asks the kernel for `min(NEAREST_WITNESS_SLOTS,
            // source_count)` neighbours per sample so that it can retain the
            // witnesses a proposal delta reads, so this batch's neighbour
            // ceiling must admit that shape.
            //
            // **It is a `KnnBatchLimits` shape field, not an FU-8 cap.**  It is
            // raised here rather than at each caller so that one authority
            // governs every construction of the specialized limits — the
            // production set and every test set alike — and so that raising it
            // cannot leak into the generic perception limits, which are built
            // from the caller's `KnnBatchLimits` unchanged and still ask for
            // one neighbour.  What it admits is bytes, not operations: a
            // `KnnBatchPlan`'s charged work is `source_count × query_count`,
            // k-independent, so nothing it permits is charged differently
            // (ruling R-a; see [`NEAREST_WITNESS_SLOTS`]).  The bytes it does
            // admit are charged — per route inside
            // `KnnBatchPlan::logical_peak_bytes` (`+2 × query_count × 16 B`,
            // single-digit MB on the Weeping Treant's widest route against this
            // batch's 2 GB logical-peak limit) and per batch as
            // [`GatePeakUsage::nearest_witness_bytes`] (~19 MB over that
            // model's 405,146 coverage samples, against a 1 GiB gate peak whose
            // last measured value was 216,203,088 B).
            coverage_route_knn: KnnBatchLimits {
                max_neighbors: NEAREST_WITNESS_SLOTS,
                ..coverage_route_knn
            },
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
            max_gate_operations_floor: 2_000_000_000,
            max_gate_window_variants: 1_024,
            max_coverage_scratch_bytes: 512 * 1024 * 1024,
            max_coverage_operations: 2_000_000_000,
            max_load_scratch_bytes: 512 * 1024 * 1024,
            max_load_operations_floor: 2_000_000_000,
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
                // Already a remaining budget carved out of the batch-materialized
                // cap upstream, so it is the whole bound here: coefficient 0.
                max_operations_floor: load_inputs.remaining_load_operations,
                operations_per_batch_unit: 0,
            },
            None,
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
    #[error("a registered batch-work coefficient does not fit this target's usize")]
    UnrepresentableBatchWork,
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
            // Already a remaining budget carved out of the batch-materialized
            // cap upstream, so it is the whole bound here: coefficient 0.
            max_operations_floor: load_inputs.remaining_load_operations,
            operations_per_batch_unit: 0,
        },
        None,
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

/// Build the accepted-load inputs, within this stage's operation and byte
/// bounds.
///
/// Byte semantics (record fix F9, 2026-08-06). This path allocates its own
/// shared distinct-face `Vec<u32>` — the third such buffer FU-8b fix (A)
/// introduced, after the two on the gate path that record fix F6 charged —
/// and reuses it across `unique_island_face_count`,
/// `variant_specialized_owner_face_count` and `append_variant_load_inputs`.
/// It was charged against nothing: the `max_load_scratch_bytes` check priced
/// only `retained_bytes` and the finalizer's own scratch. It is now the
/// `face_scratch_bytes` term of [`LoadScratchUsage`], on the same terms the
/// gate charges its own:
///
/// - The term is a **capacity bound, not an observation**. It is priced from
///   the islands' `face_ids` lengths rather than from the buffer's own
///   capacity, so it over-states rather than under-states whatever the buffer
///   has actually grown to; see [`island_face_scratch_bytes`] for why it
///   doubles. It is *not* a claim about ordering — the buffer is filled by the
///   `unique_island_face_count` call above before this check runs, exactly as
///   on the gate path.
/// - The term is **small against the bound**, measured 2026-08-06 by
///   instrumenting this check over the 8-model census. Each of the seven
///   reachable corpus models enters here exactly once; the widest requirement
///   is `Sphere`'s 78,408 B, of which the new term is 4,592 B, against the
///   536,870,912 B cap — 0.015 % of it. The other six are between 1,344 B and
///   11,680 B, with terms of 16 B to 488 B. Nothing is within four orders of
///   magnitude of the cap, so no refusal can flip, and none did: the census was
///   re-run at this tip with all seven frozen digests byte-identical and the
///   treant's single stage error unchanged.
/// - The **Weeping Treant never reaches this function** — its `Preparation`
///   refuses at the merge gate first (FU-8c). The 144,753,256 B "load scratch
///   bytes" figure of record for the treant is therefore *not* this check's; it
///   belongs to `finalize_specialized_load_partition`'s own
///   `LoadFinalizeLimits::max_scratch_bytes` check in `perception.rs`, which
///   F9 does not touch. That figure is unaffected by this change.
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
        limits.max_load_operations_floor,
    )?;
    // The two gathers below run *before* the row count exists, so they are
    // budgeted against the floor rather than against the batch-materialized
    // cap.  The floor is never above that cap, so this is strictly
    // conservative: it can refuse earlier than the batch bound would, never
    // later.  On the Weeping Treant the larger of the two charges 2.2e6 against
    // a 2e9 floor, so neither is anywhere near binding.
    //
    // One distinct-face buffer for the whole accepted-load build; both the
    // count and the fill below reuse it rather than allocating per contact.
    let mut face_scratch: Vec<u32> = Vec::new();
    // The distinct island faces bound how many *rows* the specialized pairs can
    // add to the extended load authority, however many `(owner, face)` pairs
    // those rows carry (FU-8b defect B).  Charged on its own counter:
    // `descriptor_work` is the fill pass's exact prediction and must stay that.
    let mut island_face_work = 0usize;
    let island_face_count = unique_island_face_count(
        islands,
        &mut face_scratch,
        &mut island_face_work,
        limits.max_load_operations_floor,
    )?;
    let mut descriptor_work = 0usize;
    let owner_face_count = variant_specialized_owner_face_count(
        contacts,
        None,
        generic.islands(),
        islands,
        &mut face_scratch,
        &mut descriptor_work,
        limits.max_load_operations_floor,
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
    let finalizer = specialized_load_resource_plan(
        generic,
        contacts.len(),
        SpecializedLoadCounts::bounded(owner_face_count, island_face_count),
    )?;
    let expected_fill_work = fill_operation_count(descriptor_work, reservations.len(), 1)?;
    let required_operations = descriptor_work
        .checked_add(expected_fill_work)
        .and_then(|work| work.checked_add(island_face_work))
        .and_then(|work| work.checked_add(finalizer.operations))
        .ok_or(SpecializedError::GateResourceOverflow)?;
    // The cap is materialized over the batch the plan above was computed from,
    // so the demand and the bound are always stated over the same rows and
    // contacts.
    let load_operation_limit = limits.load_operation_limit(finalizer.rows, finalizer.contacts);
    crate::census_record!(crate::census::record_scaled_cap(
        crate::census::ScaledCapRow {
            kind: "specialized_load_operations",
            floor: limits.max_load_operations_floor,
            per_batch_unit: limits.load_work_per_batch_unit,
            rows: finalizer.rows,
            columns: finalizer.contacts,
            materialized: load_operation_limit,
        }
    ));
    if required_operations > load_operation_limit {
        return Err(GenericPerceptionError::LoadOperationLimit {
            required: required_operations,
            limit: load_operation_limit,
        }
        .into());
    }
    let fill_limit = load_operation_limit
        .checked_sub(descriptor_work)
        .and_then(|remaining| remaining.checked_sub(island_face_work))
        .and_then(|remaining| remaining.checked_sub(finalizer.operations))
        .ok_or(SpecializedError::GateResourceOverflow)?;
    // Record fix F9 (2026-08-06): `face_scratch` above is the third buffer
    // FU-8b fix (A) introduced and the last one to be charged. The gate's two
    // are `GatePeakUsage::face_scratch_bytes` (record fix F6); this one is real
    // memory this path holds across the count, the fill and the finalizer plan,
    // and until now it was priced against no byte bound at all.
    check_load_scratch(
        LoadScratchUsage {
            retained_bytes,
            finalizer_scratch_bytes: finalizer.scratch_bytes,
            face_scratch_bytes: island_face_scratch_bytes(islands)?,
        },
        limits.max_load_scratch_bytes,
    )?;
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
        None,
        &mut face_scratch,
        &mut fill_work,
        fill_limit,
    )?;
    debug_assert_eq!(fill_work, expected_fill_work);
    let remaining_load_operations = load_operation_limit
        .checked_sub(descriptor_work)
        .and_then(|remaining| remaining.checked_sub(island_face_work))
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
///
/// `iterations` is the running total already spent by
/// `attach_exact_mixed_contacts` and the gated tournament; this pass folds
/// its own merges into that same count for the `max_merge_iterations` cap
/// check, so one `consolidate` call cannot spend the budget twice over. The
/// returned `usize` is still this pass's own coalesce count (for
/// `coalesced_seat_count`), not the cumulative total.
fn coalesce_exact_regular_seats(
    contacts: &mut Vec<WorkingContact>,
    limits: SpecializedLimits,
    telemetry: &mut MergeAllocationTelemetry,
    iterations: usize,
) -> Result<usize, SpecializedError> {
    let mut coalesced = 0usize;
    let mut iterations = iterations;
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
        let required_iterations = iterations
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
        iterations = required_iterations;
        coalesced += 1;
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
    let coalesced_seat_count =
        coalesce_exact_regular_seats(contacts, limits, &mut telemetry, iterations)?;
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

/// Worst-case bytes a shared distinct-face buffer may hold.
///
/// FU-8b fix (A) introduced one `Vec<u32>` per gate batch, reused by every
/// distinct-face gather in it (see [`distinct_faces_in_place`]).  It was
/// threaded through the batch without ever becoming a term in
/// [`GatePeakUsage`]; record fix F6 (2026-08-06) makes it one, so the gate's
/// byte bound covers every allocation the gate holds rather than every
/// allocation someone remembered.
///
/// Record fix F9 (2026-08-06) applies the same term to the **accepted-load**
/// bound.  Fix (A) created a *third* such buffer, in [`working_load_inputs`],
/// which is charged against no byte bound at all: that path checks only the
/// retained contacts and pairs plus the finalizer's own scratch against
/// `max_load_scratch_bytes`.  It is now [`LoadScratchUsage::face_scratch_bytes`]
/// there.  Both callers hand this function the same `islands` slice and get the
/// same bound, because both fill their buffer the same way.
///
/// The bound is the largest single gather the batch can perform.
/// `unique_island_face_count` gathers *every* island's `face_ids` before
/// deduping, and every other gather in the batch
/// (`visit_served_owner_faces`) gathers a subset of the same ids, so the
/// islands' total `face_ids` length bounds them all.  The doubling covers
/// `Vec`'s amortized growth: a buffer filled by repeated `extend_from_slice`
/// and never shrunk can sit at up to twice its live length.  That factor is a
/// design bound on this buffer, not a guarantee extracted from `Vec` — `Vec`
/// documents no exact growth policy — which is why the term is deliberately
/// generous rather than exact.
fn island_face_scratch_bytes(islands: &[SupportIsland]) -> Result<usize, SpecializedError> {
    islands
        .iter()
        .try_fold(0usize, |total, island| {
            total.checked_add(island.face_ids.len())
        })
        .and_then(|entries| entries.checked_mul(std::mem::size_of::<u32>()))
        .and_then(|bytes| bytes.checked_mul(2))
        .ok_or(SpecializedError::GateResourceOverflow)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GatePeakUsage {
    proposal_bytes: usize,
    result_bytes: usize,
    /// The batch's **one** set of load inputs and its baseline partition, from
    /// [`gate_load_state_bytes`] (FU-8d phase F3, part P2).
    ///
    /// It was `matrix_bytes`: a window of up to `max_gate_window_variants`
    /// `PhysicalContact` and `(owner, face)` matrices, one pair of them per
    /// variant in the window.  On the Weeping Treant the pair matrix alone is
    /// 7,971,451 entries **per variant**, so `gate_window_matrix_bytes` stated
    /// tens of gigabytes against a 512 MiB limit — a wall that had never fired
    /// only because `exact_operations` refused the batch before the window loop
    /// ran.  Removing the operations wall without removing the matrices would
    /// have moved the refusal onto that byte limit instead, which is why the
    /// matrices are structurally gone rather than merely repriced.
    load_baseline_bytes: usize,
    /// The batch's shared distinct-face buffer, from
    /// [`island_face_scratch_bytes`].  Retained, not phase-local: the buffer
    /// is allocated once at the top of the batch and lives across the
    /// coverage, nearest and load phases alike.
    face_scratch_bytes: usize,
    /// The batch's proposal-invariant route/witness tables, from
    /// [`GateBatchIndex`].  Retained, not phase-local: built once at the top of
    /// the batch and read by every variant (FU-8c, 2026-08-06).
    batch_index_bytes: usize,
    /// The two `samples`-long nearest arrays the batch retains: the baseline
    /// answers and the working copy each variant perturbs and restores.  Before
    /// FU-8c this was a `window_variants × samples` matrix charged as part of
    /// `matrix_bytes`; the batch now holds two rows instead of up to 1,024.
    nearest_bytes: usize,
    /// The baseline sweep's retained witness table, from
    /// [`gate_nearest_witness_bytes`]: `samples × NEAREST_WITNESS_SLOTS` fixed
    /// slots, held for the whole batch (FU-8d, 2026-08-06).  Retained, not
    /// phase-local, and charged from the first batch that builds one — an
    /// allocation this stage holds and does not charge for is the defect record
    /// fixes F6 and F9 found twice already.
    nearest_witness_bytes: usize,
    /// The batch's retained per-proposal nearest plans, from
    /// [`GatePlannedNearest::retained_plan_bytes`] (FU-8d phase F3,
    /// 2026-08-06).  Retained, not phase-local: planning them once instead of
    /// four times means all of them are live from the plan until the last
    /// variant is evaluated, where before only the widest was.  Charged for the
    /// same reason record fixes F6 and F9 charged their buffers — an allocation
    /// the stage holds and does not charge for is a hole in the bound, and
    /// trading operations for uncharged bytes would be the same trade in
    /// reverse.
    proposal_plan_bytes: usize,
    coverage_scratch_bytes: usize,
    coverage_nearest_bytes: usize,
    coverage_retained_bytes: usize,
    load_scratch_bytes: usize,
}

fn check_gate_peak(usage: GatePeakUsage, limit: usize) -> Result<usize, SpecializedError> {
    let retained = usage
        .proposal_bytes
        .checked_add(usage.result_bytes)
        .and_then(|bytes| bytes.checked_add(usage.load_baseline_bytes))
        .and_then(|bytes| bytes.checked_add(usage.face_scratch_bytes))
        .and_then(|bytes| bytes.checked_add(usage.batch_index_bytes))
        .and_then(|bytes| bytes.checked_add(usage.nearest_bytes))
        .and_then(|bytes| bytes.checked_add(usage.nearest_witness_bytes))
        .and_then(|bytes| bytes.checked_add(usage.proposal_plan_bytes))
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

/// What the **accepted-load** build holds at once, checked against
/// `max_load_scratch_bytes` by [`check_load_scratch`].
///
/// Record fix F9 (2026-08-06) introduced this struct so the accepted-load byte
/// bound states its terms the way [`GatePeakUsage`] states the gate's, and so
/// the term F9 added can be pinned. Before F9 the check was two inline
/// `checked_add`s over `retained_bytes` and the finalizer's scratch, and the
/// distinct-face buffer beside them was charged against nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LoadScratchUsage {
    /// The contacts and the `(owner, face)` pairs the build retains.
    retained_bytes: usize,
    /// The load finalizer's own scratch, from `specialized_load_resource_plan`.
    finalizer_scratch_bytes: usize,
    /// The build's shared distinct-face buffer, from
    /// [`island_face_scratch_bytes`] — the same allocation the gate charges as
    /// [`GatePeakUsage::face_scratch_bytes`], on the path taken after the gate
    /// accepts. Added by record fix F9 (2026-08-06); it was held and never
    /// charged before that.
    face_scratch_bytes: usize,
}

fn check_load_scratch(usage: LoadScratchUsage, limit: usize) -> Result<usize, SpecializedError> {
    let required = usage
        .retained_bytes
        .checked_add(usage.finalizer_scratch_bytes)
        .and_then(|bytes| bytes.checked_add(usage.face_scratch_bytes))
        .ok_or(SpecializedError::GateResourceOverflow)?;
    if required > limit {
        return Err(GenericPerceptionError::LoadScratchLimit { required, limit }.into());
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
    // The batch's own materialized gate cap: the header preflight and every
    // check below in this batch use the one bound stated for this batch shape.
    if limits.gate_operation_limit(sample_count, contact_count) == 0
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
    /// The load term inside `operations`, from
    /// [`crate::perception::specialized_load_resource_plan`] plus the
    /// per-proposal bound (FU-8d phase F3, part P2).
    load_operations: usize,
    /// The per-proposal share of `load_operations`, kept so the execution's
    /// running meter can be given the batch's whole remaining headroom rather
    /// than only what the bound claimed.
    load_proposal_bound: usize,
}

/// The batch's measured per-proposal routed work, supplied to
/// [`gate_resource_plan`] by the caller that built the batch index.
///
/// Before FU-8c the plan could state the routed term from counts alone,
/// because the gate re-evaluated *every* contact against *every* coverage
/// sample once per proposal: the shape was `(samples + demands + 1) ×
/// (proposals × (contacts - 1) + contacts)` and needed nothing but counts.  The
/// new evaluation is route-local, so its per-proposal cost depends on which
/// route keys each proposal's pair serves and how many samples and servers
/// those keys carry.  Counting that honestly requires the data, so the caller
/// measures it in one cheap pass and hands the totals here rather than the plan
/// pretending to a worst case it no longer pays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GateProposalRoutedWork {
    /// `Σ_proposals 1 × touched_rows` — one merged-seat kNN batch per proposal,
    /// in the same `source_count × query_count` unit
    /// `plan_route_points_nearest` charges for the baseline sweep.
    ///
    /// **FU-8d phase F2 (2026-08-06) is what made this term small.**  It was
    /// `Σ_proposals Σ_{perturbed routes} source_count × query_count`, a full
    /// re-sweep of every perturbed key against its whole surviving source set;
    /// the survivors' answer is now read from the baseline witness table, so
    /// the merged seat is the only source whose distances are still computed.
    nearest_work: usize,
    /// `Σ_proposals` sample rows overwritten and restored.  Each row also pays
    /// one bounded witness scan — at most `NEAREST_WITNESS_SLOTS` slot tests —
    /// and one minimum, the same "one row unit, bounded constant work"
    /// convention `demand_rows` below uses.
    touched_rows: usize,
    /// `Σ_proposals routes(left) + demands(left) + routes(right) +
    /// demands(right)` — the route-key slots visited to build each proposal's
    /// `K(left) ∪ K(right)`.
    ///
    /// Charged from FU-8d phase F2 because after F2 it is no longer lost in the
    /// noise of the term above: deciding a proposal's *shape* is now a
    /// comparable fraction of evaluating it, and an uncharged decision cost is
    /// exactly the hole FU-8d part (3) names.
    key_visits: usize,
    /// `Σ_proposals` demand rows re-tested against the merged seat, i.e.
    /// `proposals × demands`: one witness lookup plus at most one `satisfies`
    /// call each, replacing the old `demands × contacts` sweep per variant.
    demand_rows: usize,
}

impl GateProposalRoutedWork {
    fn total(self) -> Result<usize, SpecializedError> {
        self.nearest_work
            .checked_add(self.touched_rows)
            .and_then(|work| work.checked_add(self.key_visits))
            .and_then(|work| work.checked_add(self.demand_rows))
            .ok_or(SpecializedError::GateResourceOverflow)
    }
}

/// What the load phase costs a merge batch, from the load planner itself
/// (FU-8d phase F3, part P2).
///
/// Before F3 the gate charged the load phase with a hand-built model —
/// `variants × (faces + owner_faces)` inside `variant_operation_count` — that
/// sat beside `specialized_load_resource_plan`'s own figure and disagreed with
/// it: FU-8d part (2) recorded that the model charged `variants × faces` where
/// the fold really did `variants × faces × contacts`, and that the under-charge
/// is why the term was never the refusing one.  There is now **one authority**.
/// `baseline_operations` is what `specialized_load_resource_plan` returns for
/// the batch's single baseline finalize, and `proposal_bound` is
/// [`proposal_load_bound`]'s count-derived allowance for the deltas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GateLoadWork {
    baseline_operations: usize,
    proposal_bound: usize,
}

impl GateLoadWork {
    fn total(self) -> Result<usize, SpecializedError> {
        self.baseline_operations
            .checked_add(self.proposal_bound)
            .ok_or(SpecializedError::GateResourceOverflow)
    }
}

fn gate_resource_plan(
    contact_count: usize,
    proposal_count: usize,
    sample_count: usize,
    demand_count: usize,
    proposal_routed_work: GateProposalRoutedWork,
    load_work: GateLoadWork,
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
    // FU-8c (2026-08-06).  The routed term is now the *baseline* sweep plus the
    // measured per-proposal deltas, not one full sweep per variant.
    //
    // Old:  routed_rows = (samples + demands + 1) × (proposals × (contacts - 1) + contacts)
    // New:  routed_rows = (samples + demands + 1) × contacts + proposal_routed_work
    //
    // The first term is unchanged in shape and prices exactly what it always
    // priced — one pass in which every contact is routed against every coverage
    // sample and every demand, plus one visit per contact.  What changed is that
    // the gate performs that pass **once for the batch** (`GateBatchIndex` plus
    // the variant-0 nearest fill) instead of once per variant, because a merge
    // proposal can only perturb the route keys its own pair serves and can only
    // unseat a demand one of its own pair was witnessing; see [`GateBatchIndex`]
    // for the argument.  The second term is what the proposals actually cost
    // after that: their perturbed-route kNN work, the sample rows they overwrite
    // and restore, and one demand row each.
    //
    // Worked on the Weeping Treant's gate batch (samples 405,146; demands 1,095;
    // contacts 1,366; proposals 39,223; distinct faces 117,741):
    //   baseline_rows = 406,242 × 1,366           =       554,926,572
    //   face_rows     = 39,224 × 117,741          =     4,618,272,984
    // against `gate_operation_limit(405,146, 1,366)` = 35,445,500,736.  The
    // predecessor charged 21,750,505,830,162 for the routed term alone.
    //
    // FU-8d phase F2 (2026-08-06) leaves both shapes above exactly as they are
    // and changes only what `proposal_routed_work` measures.  `nearest_work`
    // was `Σ_proposals Σ_{perturbed routes} source_count × query_count` — on
    // this batch 196,264,386,588, i.e. 97.4 % of the whole stated demand — and
    // is now `Σ_proposals 1 × touched_rows`, the merged seat being the only
    // source whose distances are still computed.  `key_visits` is new and is
    // the route-key gather that decides each proposal's shape.  Both are
    // measured, not modelled; ruling R-e forbids pinning a literal for the
    // post-fix treant total here, and the derivation-shaped pins live in the
    // tests instead.  **No cap moved for F2.**
    let baseline_rows = sample_count
        .checked_add(demand_count)
        .and_then(|value| value.checked_add(1))
        .and_then(|value| value.checked_mul(contact_count))
        .ok_or(SpecializedError::GateResourceOverflow)?;
    let routed_rows = baseline_rows
        .checked_add(proposal_routed_work.total()?)
        .ok_or(SpecializedError::GateResourceOverflow)?;
    // FU-8d phase F3 (P2): `face_rows = variants × distinct faces` is gone.
    // It was the gate's own model of a per-variant load finalize, and it was
    // wrong by a factor of the contact count — part (2) measured the real fold
    // at `variants × faces × contacts`, ≈ 4.33 × 10¹⁴ on the treant against a
    // charge of 4,618,272,984.  The finalize now runs **once**, and what it
    // costs is what its own planner says it costs.
    let load_operations = load_work.total()?;
    let operations = routed_rows
        .checked_add(load_operations)
        .ok_or(SpecializedError::GateResourceOverflow)?;
    // The plan is bounded by the cap materialized for *this batch's* shape.
    // The proposal count is deliberately not a factor in that shape: a gate
    // that re-evaluates every contact once per merge proposal is superlinear
    // in the batch, and that is exactly what this cap exists to refuse.  After
    // FU-8c the gate no longer does that, and the plan above no longer charges
    // for it; the cap is unchanged and was not touched.
    let operation_limit = limits.gate_operation_limit(sample_count, contact_count);
    if operations > operation_limit {
        return Err(SpecializedError::GateOperationLimit {
            required: operations,
            limit: operation_limit,
        });
    }
    Ok(GateResourcePlan {
        variants,
        vector_bytes,
        contact_window_bytes,
        operations,
        load_operations,
        load_proposal_bound: load_work.proposal_bound,
    })
}

/// Gather one island's face ids into the shared distinct-face scratch.
///
/// The whole island is charged in a single bulk add *before* a single id is
/// copied, so an island that cannot fit inside the remaining budget is refused
/// without growing the buffer.  The charge is one unit per id, the same unit
/// the surrounding descriptor scans charge for inspecting one element.
fn gather_island_faces(
    scratch: &mut Vec<u32>,
    island: &SupportIsland,
    work: &mut usize,
    limit: usize,
) -> Result<(), SpecializedError> {
    charge_load_descriptor_work(work, island.face_ids.len(), limit)?;
    scratch.extend_from_slice(&island.face_ids);
    Ok(())
}

/// Collapse a gathered face-id buffer to its ascending distinct ids in place.
///
/// Charging semantics (FU-8b fix A, 2026-08-06).  The meter counts elementary
/// comparisons, matching every other `charge_load_descriptor_work` site.  The
/// batch dedup performs at most `n * ceil(log2(n + 1))` comparisons — the
/// comparison bound for `sort_unstable` — plus one adjacent-pair pass of `n`
/// for `dedup`, so it charges `n * (ceil(log2(n + 1)) + 1)` in one bulk add
/// (`gather_island_faces` has already charged the `n` inspections that filled
/// the buffer).  The predecessor was a repeated-minimum selection sort that
/// charged one unit per inner comparison, i.e. `n * (distinct + 1)`: quadratic
/// in the face count.  The emitted values — the count and the ascending
/// distinct order — are unchanged; only the meter moved.
///
/// Byte semantics (record fixes F6 and F9, 2026-08-06).  `scratch` is the one
/// `Vec<u32>` its caller's batch allocates and every gather in that batch
/// reuses — that reuse is the point of fix (A), which otherwise would have
/// traded a quadratic charge for a per-contact allocation.  Fix (A) threaded
/// three such buffers through three batches and left all three out of their
/// byte bounds, so each was real memory the stage held and never charged for.
/// All three are charged now:
///
/// - the gate's two, in `evaluate_gate_batch` and
///   `whole_gate_route_preflight`, as the `face_scratch_bytes` term of
///   [`GatePeakUsage`] — added to that check's *retained* total, because the
///   buffer outlives every individual phase (record fix F6);
/// - the accepted-load path's, in [`working_load_inputs`], as the
///   `face_scratch_bytes` term of [`LoadScratchUsage`], checked against
///   `max_load_scratch_bytes` (record fix F9, which found the omission F6 had
///   fixed only on the gate side).
///
/// All three use [`island_face_scratch_bytes`], and two things follow at every
/// site.  Both are deliberate:
///
/// - The term is a **capacity bound, not an observation**.  It is priced from
///   the islands' `face_ids` lengths rather than from the buffer's own
///   capacity, so it over-states rather than under-states whatever the buffer
///   has actually grown to; see [`island_face_scratch_bytes`] for why it
///   doubles.  (Corrected 2026-08-06, record fix F8: this bullet previously
///   said the term "is priced before any gather runs", which is false at every
///   production site — `evaluate_gate_batch`, `whole_gate_route_preflight` and
///   [`working_load_inputs`] each fill the buffer via
///   `unique_island_face_count` before reaching their check.  The pricing
///   basis, not the ordering, is what makes the term an over-statement, and
///   the ordering claim is dropped.)
/// - The term is **small against everything else in the bound**.  Measured on
///   the Weeping Treant's gate batch, 2026-08-06: 570,584 B of live face ids,
///   charged as 1,141,168 B after the doubling.  That takes the gate peak from
///   215,061,920 B to 216,203,088 B against the unchanged 1,073,741,824 B cap
///   — 0.53 % of the peak and 0.11 % of the cap, leaving the batch at 20.1 %
///   of its bound.  Charging it therefore flips no refusal anywhere on the
///   corpus, which was proved by re-running the census: the seven frozen
///   digests stayed byte-identical and the treant's refusal line stayed
///   `GateOperationLimit { required: 21755124103146, limit: 35445500736 }`.
///   The accepted-load site is smaller again: it is the path the seven
///   reachable corpus models actually take, and their whole load-scratch
///   requirement peaks at `Sphere`'s 78,408 B — 0.015 % of the 536,870,912 B
///   cap, with the new term 4,592 B of it — so F9 too flipped no refusal,
///   proved by the same census re-run.  Each is charged because the stage must
///   bound what it holds, not because anything was observed to be near a
///   limit.
fn distinct_faces_in_place(
    scratch: &mut Vec<u32>,
    work: &mut usize,
    limit: usize,
) -> Result<(), SpecializedError> {
    let count = scratch.len();
    // `usize::BITS - leading_zeros` is the significant-bit count, i.e.
    // `ceil(log2(count + 1))`, and is 0 for an empty gather.
    let log2_ceiling = usize::try_from(usize::BITS - count.leading_zeros())
        .map_err(|_| SpecializedError::GateResourceOverflow)?;
    let comparisons = log2_ceiling
        .checked_add(1)
        .and_then(|per_element| count.checked_mul(per_element))
        .ok_or(SpecializedError::GateResourceOverflow)?;
    charge_load_descriptor_work(work, comparisons, limit)?;
    scratch.sort_unstable();
    scratch.dedup();
    Ok(())
}

/// Count the distinct face ids owned by `islands`.
///
/// `scratch` is a caller-owned buffer reused across every distinct-face gather
/// in one gate batch, so no gather allocates per island or per contact.  Its
/// capacity is bounded by the largest single gather: at most one `u32` per
/// entry in the islands' own `face_ids`, i.e. no more than the face-id arrays
/// the caller already holds (and, independently, at most `limit` entries, since
/// every gathered id is charged before it is copied).
fn unique_island_face_count(
    islands: &[SupportIsland],
    scratch: &mut Vec<u32>,
    work: &mut usize,
    limit: usize,
) -> Result<usize, SpecializedError> {
    scratch.clear();
    for island in islands {
        gather_island_faces(scratch, island, work, limit)?;
    }
    distinct_faces_in_place(scratch, work, limit)?;
    Ok(scratch.len())
}

/// How one variant pays for its routed and satisfaction rows.
///
/// Variant 0 sweeps: every contact is routed against every coverage sample and
/// every demand, and every demand is tested against every contact.  A proposal
/// variant re-answers only the route keys its merged pair serves and re-tests
/// each demand once against the merged seat; the rest is the baseline's, read
/// from [`GateBatchIndex`] (FU-8c, 2026-08-06).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VariantRoutedWork {
    BaselineSweep {
        sample_count: usize,
    },
    ProposalDelta {
        nearest_work: usize,
        touched_rows: usize,
        key_visits: usize,
    },
}

/// The batch-level totals every variant's operation count is stated over,
/// computed **once** for the batch (FU-8d phase F3, part P5).
///
/// # Why these are batch-level and not per-variant
///
/// Both totals are proposal-invariant, and `variant_operation_count` re-derived
/// both of them for every one of the `proposals + 1` variants — the same defect
/// FU-8c removed from the nearest phase and FU-8b removed from the load
/// extension, in its fourth dress.  On the Weeping Treant that is
/// `39,224 × 1,366` contact visits and `39,224 × 1,095` demand visits to
/// recompute two numbers that never change.
///
/// **The proposal variant's logical contact work is `baseline − 1`, exactly.**
/// Writing `n` for the contact count, `R` for `Σ regular_routes` and `D` for
/// `Σ demands`, the baseline sums `1 + routes + demands` over every contact, so
/// it is `n + R + D`.  A proposal variant skips `left` and `right` and then
/// adds one merged seat carrying the union of their route and demand slots:
/// `(n − 2) + (R − r_l − r_r) + (D − d_l − d_r)` plus
/// `1 + r_l + r_r + d_l + d_r`, i.e. `n − 1 + R + D`.  Every proposal-specific
/// term cancels, which is why the derivation needs no knowledge of *which* pair
/// merged.  Its physical contact count is `n − 1` for the same reason: two
/// seats leave, one arrives.
///
/// Pinned by `fu8d_variant_totals_are_derived_not_rescanned`, which compares
/// the derivation against the rescan it replaces over every proposal of the
/// delta fixture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GateVariantTotals {
    /// `Σ_contacts 1 + regular_routes + demands`, the baseline variant's own
    /// logical contact work.
    logical_contact_work: usize,
    /// The batch's contact count, i.e. the baseline variant's physical seats.
    physical_contacts: usize,
    /// `Σ_demands demand_satisfaction_work(demand)`.
    demand_satisfaction: usize,
}

impl GateVariantTotals {
    fn build(
        contacts: &[WorkingContact],
        demands: &[SpecializedDemand],
    ) -> Result<Self, SpecializedError> {
        let logical_contact_work = contacts.iter().try_fold(0usize, |total, contact| {
            total
                .checked_add(1)
                .and_then(|work| work.checked_add(contact.regular_routes.len()))
                .and_then(|work| work.checked_add(contact.demands.len()))
                .ok_or(SpecializedError::GateResourceOverflow)
        })?;
        let demand_satisfaction = demands.iter().try_fold(0usize, |total, demand| {
            total
                .checked_add(demand_satisfaction_work(demand)?)
                .ok_or(SpecializedError::GateResourceOverflow)
        })?;
        Ok(Self {
            logical_contact_work,
            physical_contacts: contacts.len(),
            demand_satisfaction,
        })
    }

    /// This batch's totals as one variant sees them.  See the type's own
    /// documentation for why the proposal case is `baseline − 1` on both
    /// counts and needs no proposal argument.
    fn for_variant(self, merged: bool) -> Result<(usize, usize), SpecializedError> {
        if !merged {
            return Ok((self.logical_contact_work, self.physical_contacts));
        }
        let logical = self
            .logical_contact_work
            .checked_sub(1)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        let physical = self
            .physical_contacts
            .checked_sub(1)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        Ok((logical, physical))
    }
}

fn variant_operation_count(
    totals: GateVariantTotals,
    merged: bool,
    routed: VariantRoutedWork,
    demand_count: usize,
) -> Result<usize, SpecializedError> {
    let (logical_contact_work, physical_contacts) = totals.for_variant(merged)?;
    let demand_satisfaction = totals.demand_satisfaction;
    // FU-8c: only the baseline variant routes every contact against every
    // sample and demand.  A proposal variant pays for the routes it perturbs,
    // the sample rows it rewrites, and one merged-seat test per demand.
    // FU-8d F2: "the routes it perturbs" is now the merged seat's single kNN
    // batch over those routes' rows, plus the key gather that found them.
    let (routing_work, satisfaction_work) = match routed {
        VariantRoutedWork::BaselineSweep { sample_count } => {
            let routed_queries = sample_count
                .checked_add(demand_count)
                .ok_or(SpecializedError::GateResourceOverflow)?;
            (
                logical_contact_work
                    .checked_mul(routed_queries)
                    .ok_or(SpecializedError::GateResourceOverflow)?,
                physical_contacts
                    .checked_mul(demand_satisfaction)
                    .ok_or(SpecializedError::GateResourceOverflow)?,
            )
        }
        VariantRoutedWork::ProposalDelta {
            nearest_work,
            touched_rows,
            key_visits,
        } => (
            nearest_work
                .checked_add(touched_rows)
                .and_then(|work| work.checked_add(key_visits))
                .ok_or(SpecializedError::GateResourceOverflow)?,
            demand_satisfaction,
        ),
    };
    // FU-8d phase F3 (P2): `unique_face_finalizer_work` — the gate's own
    // `variants × (faces + owner_faces) × physical_contacts` model of a
    // per-variant load finalize — is gone from here.  The finalize runs once
    // for the batch and its cost is charged where it is *measured*, by
    // `specialized_load_resource_plan`, as `GateLoadWork`.
    routing_work
        .checked_add(logical_contact_work)
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
    // One distinct-face buffer for the whole gate batch: every face gather
    // below reuses this allocation instead of building a set per contact.
    let mut face_scratch: Vec<u32> = Vec::new();
    let mut face_count_operations = 0usize;
    // One materialized bound for the whole batch, stated once from the batch's
    // own shape and then used by every charge site below, so the gather budget
    // and the final check cannot disagree about what this batch may spend.
    let gate_operation_limit = limits.gate_operation_limit(coverage_samples.len(), contacts.len());
    crate::census_record!(crate::census::record_scaled_cap(
        crate::census::ScaledCapRow {
            kind: "specialized_gate_operations",
            floor: limits.max_gate_operations_floor,
            per_batch_unit: limits.gate_work_per_batch_unit,
            rows: coverage_samples.len(),
            columns: contacts.len(),
            materialized: gate_operation_limit,
        }
    ));
    // FU-8c observation O1: what this batch offers the per-proposal delta path.
    // Recorded before any charge site below can refuse, so a batch that trips
    // the cap still reports its proposal count.
    crate::census_record!(crate::census::record_gate_proposals(proposals.len()));
    let face_count = unique_island_face_count(
        islands,
        &mut face_scratch,
        &mut face_count_operations,
        gate_operation_limit,
    )?;
    // FU-8c: the batch's proposal-invariant state, built once.  Everything the
    // merge proposals below need that does not depend on which pair is merged
    // is derived here and read, never recomputed.
    let batch_index = GateBatchIndex::build(
        contacts,
        coverage_samples,
        authoritative_demands,
        layers,
        attach_reach,
    )?;
    let planned_nearest = plan_gate_nearest_batch(
        contacts,
        proposals,
        coverage_samples,
        authoritative_demands.len(),
        &batch_index,
        limits.coverage_route_knn,
        gate_operation_limit,
    )?;
    let mut descriptor_operations = 0usize;
    // FU-8d phase F3 (P2): the batch's ONE set of load inputs and its ONE
    // finalized load partition.  Both were per variant; see [`GateLoadState`]
    // for the locality argument that makes a proposal's answer a delta against
    // this, and for why the per-variant window matrices had to go with them.
    let load_state = build_gate_load_state(
        contacts,
        generic,
        mesh,
        surface,
        islands,
        face_count,
        reservations,
        &mut face_scratch,
        &mut descriptor_operations,
        gate_operation_limit,
        limits,
    )?;
    let plan = gate_resource_plan(
        contacts.len(),
        proposals.len(),
        coverage_samples.len(),
        authoritative_demands.len(),
        planned_nearest.batch.proposal_work,
        GateLoadWork {
            baseline_operations: load_state.baseline_operations,
            proposal_bound: proposal_load_bound(&load_state, proposals)?,
        },
        limits,
    )?;
    // FU-8d phase F3 (P5): the batch's proposal-invariant contact and demand
    // totals, computed once.  `variant_operation_count` used to re-derive both
    // for every variant; see [`GateVariantTotals`] for why the proposal
    // variant's are the baseline's minus one, exactly, with no proposal in the
    // derivation.
    let variant_totals = GateVariantTotals::build(contacts, authoritative_demands)?;
    let exact_operations = (0..plan.variants).try_fold(0usize, |total, variant| {
        let routed = match variant.checked_sub(1) {
            None => VariantRoutedWork::BaselineSweep {
                sample_count: coverage_samples.len(),
            },
            Some(index) => {
                // FU-8d phase F3: the second of the four planning passes lived
                // here.  The fold now *charges* the plan the batch already made
                // instead of re-deriving it, so the charge and the plan cannot
                // disagree by construction.
                let delta = &planned_nearest.proposals[index];
                VariantRoutedWork::ProposalDelta {
                    nearest_work: delta.work,
                    touched_rows: delta.touched_rows,
                    key_visits: delta.key_visits,
                }
            }
        };
        total
            .checked_add(variant_operation_count(
                variant_totals,
                variant != 0,
                routed,
                authoritative_demands.len(),
            )?)
            .ok_or(SpecializedError::GateResourceOverflow)
    })?;
    // FU-8d phase F3: the descriptor term is what building the batch's **one**
    // set of load inputs cost, charged once.  It was `× 4 × variants` — each
    // variant's descriptors were scanned for the operation plan, for the window
    // matrix size, for the load scratch plan and again while filling the
    // matrix — and on the treant that product alone crossed the cap at variant
    // 275 of 39,224.  Three of those four scans belonged to matrices that no
    // longer exist and the fourth now happens once.
    let exact_operations = exact_operations
        .checked_add(face_count_operations)
        .and_then(|work| work.checked_add(descriptor_operations))
        .and_then(|work| work.checked_add(reservations.len()))
        .and_then(|work| work.checked_add(plan.load_operations))
        .ok_or(SpecializedError::GateResourceOverflow)?;
    if exact_operations > gate_operation_limit {
        return Err(SpecializedError::GateOperationLimit {
            required: exact_operations,
            limit: gate_operation_limit,
        });
    }
    // What the per-proposal load deltas may spend, measured as they run.  The
    // plan above charged a count-derived bound for them; this is the whole
    // headroom the batch has left once every other term is paid, so a proposal
    // set whose measured delta work exceeds its bound is still admitted if the
    // batch can afford it — and refused the moment it cannot.  It is the same
    // "state a bound, then meter the real spend against it" shape
    // `charge_load_descriptor_work` uses everywhere else in this stage.
    let load_delta_limit = gate_operation_limit
        .checked_sub(
            exact_operations
                .checked_sub(plan.load_proposal_bound)
                .ok_or(SpecializedError::GateResourceOverflow)?,
        )
        .ok_or(SpecializedError::GateResourceOverflow)?;
    // Plan every route in every gate window before allocating the first
    // nearest matrix.  A window-local plan alone would permit a cap failure in
    // a later window after earlier windows had already gathered/queryed data.
    // Each preflight window temporarily builds bounded `Vec`/`BTree*` route
    // payloads, then retains only its arithmetic summary. It therefore does
    // not retain data for the full gate payload.
    let mut load_scratch = SpecializedLoadDeltaScratch::for_rows(load_state.baseline.row_count());
    let load_baseline_bytes = gate_load_state_bytes(&load_state, &load_scratch)?;
    whole_gate_route_preflight_then_execute(
        proposal_storage_bytes,
        topology,
        islands,
        coverage_samples,
        &planned_nearest,
        load_baseline_bytes,
        load_state.baseline_scratch_bytes,
        &batch_index,
        limits,
        &plan,
        |whole_gate_route_plan| {
            let mut vectors = Vec::with_capacity(plan.variants);
            let mut executed_nearest = GateNearestBatchPlan::seeded();
            // The batch's one baseline nearest sweep.  Every proposal variant
            // below reads its answers and rewrites only the rows its own merged
            // pair can move (FU-8c).
            let baseline_route_plan = plan_variant_nearest_routes(
                contacts,
                None,
                coverage_samples,
                limits.coverage_route_knn,
                NearestWitnessRetention::RetainWitnesses,
            )?;
            executed_nearest.record_baseline(&baseline_route_plan);
            let mut baseline_nearest = vec![f64::INFINITY; coverage_samples.len()];
            // FU-8d: the sweep also retains up to `NEAREST_WITNESS_SLOTS`
            // nearest sources per sample.  Retaining them changes no answer
            // (see [`fill_route_points_nearest`]) and changes no charge (ruling
            // R-a); its bytes are charged as
            // `GatePeakUsage::nearest_witness_bytes`.
            let mut baseline_witnesses = NearestWitnessTable::seeded(coverage_samples.len());
            fill_variant_nearest(
                contacts,
                None,
                coverage_samples,
                &mut baseline_nearest,
                baseline_route_plan,
                Some(&mut baseline_witnesses),
            )?;
            debug_assert_eq!(baseline_witnesses.rows.len(), coverage_samples.len());
            let mut working_nearest = baseline_nearest.clone();
            let mut touched_rows: Vec<usize> = Vec::new();
            // FU-8d F2: one query-point buffer for the whole batch.  Every
            // proposal refills it; none allocates its own.  FU-8e E2 added its
            // output twin — the merged seat's squared distances now land in a
            // batch-retained buffer instead of in a per-proposal `NeighborCsr`.
            let mut proposal_nearest_scratch = ProposalNearestScratch::default();
            let mut executed_whole_gate_route_plan = GateWholeRoutePlan::default();
            // FU-8d phase F3 (P2): there are no windows any more, because there
            // are no per-window matrices for a window to bound.  The batch
            // holds one physical list, one `(owner, face)` list and one
            // finalized partition, so the peak is stated once for the batch
            // rather than once per window of variants.
            let coverage_scratch = coverage_assessment_resource_plan(coverage_samples, topology)?;
            let coverage_retained_bytes = coverage_samples
                .len()
                .checked_mul(std::mem::size_of::<usize>())
                .ok_or(SpecializedError::GateResourceOverflow)?;
            let mut merged_rows: Vec<u32> = Vec::new();
            let executed_peak_bytes = check_gate_peak(
                GatePeakUsage {
                    proposal_bytes: proposal_storage_bytes,
                    result_bytes: plan.vector_bytes,
                    load_baseline_bytes: gate_load_state_bytes(&load_state, &load_scratch)?,
                    face_scratch_bytes: island_face_scratch_bytes(islands)?,
                    batch_index_bytes: batch_index.retained_bytes,
                    nearest_bytes: gate_nearest_array_bytes(coverage_samples.len())?,
                    nearest_witness_bytes: gate_nearest_witness_bytes(coverage_samples.len())?,
                    proposal_plan_bytes: planned_nearest.retained_plan_bytes,
                    coverage_scratch_bytes: coverage_scratch.scratch_bytes,
                    coverage_nearest_bytes: planned_nearest.batch.peak_bytes()?,
                    coverage_retained_bytes,
                    load_scratch_bytes: load_state.baseline_scratch_bytes,
                },
                limits.max_gate_peak_bytes,
            )?;
            executed_whole_gate_route_plan.record(executed_peak_bytes)?;
            // FU-8e phase E3: the batch's **one** full coverage assessment.
            // Variant 0 is the baseline, so this is literally the call its
            // iteration used to make, hoisted out of the loop; every proposal
            // variant that leaves the uncovered rows alone is handed this same
            // objective instead of rescanning the nearest array for it.  See
            // [`variant_coverage_objective`].
            let coverage_question = GateCoverageQuestion {
                samples: coverage_samples,
                topology,
                radius_mm: coverage_radius,
                area_floor_mm2: coverage_area_floor_mm2,
                limits: CoverageAssessmentLimits {
                    max_scratch_bytes: limits.max_coverage_scratch_bytes,
                    max_operations: limits.max_coverage_operations,
                },
            };
            let baseline_coverage_objective = coverage_question.objective_of(&baseline_nearest)?;
            let mut executed_load_work = SpecializedLoadDeltaWork::default();
            // FU-8e phase E3: what the coverage fallbacks have spent so far.
            let mut executed_coverage_work = 0usize;
            let mut executed_touched_rows = 0usize;
            {
                for variant in 0..plan.variants {
                    let proposal = variant
                        .checked_sub(1)
                        .and_then(|index| proposals.get(index));
                    // FU-8c: variant 0 reads the batch's baseline answers; a
                    // proposal variant overwrites only the rows its merged pair
                    // can move, and restores them before the next variant.
                    touched_rows.clear();
                    // Variant 0 *is* the baseline, so it moves nothing.
                    let mut coverage_delta = ProposalCoverageDelta::default();
                    let nearest: &[f64] = match proposal {
                        None => &baseline_nearest,
                        Some(proposal) => {
                            // FU-8d phase F3: the fourth planning pass lived
                            // here.  Execution reads the batch's own plan and
                            // records that it consumed it, which is what the
                            // re-founded `GateRoutePlanMismatch` now attests:
                            // that every planned proposal was executed, once
                            // and in order — not that a second planning pass
                            // agreed with the first.
                            let delta = &planned_nearest.proposals[variant - 1];
                            executed_nearest.record_proposal(delta)?;
                            coverage_delta = fill_proposal_nearest_delta(
                                proposal,
                                GateNearestBaseline {
                                    samples: coverage_samples,
                                    index: &batch_index,
                                    witnesses: &baseline_witnesses,
                                    nearest: &baseline_nearest,
                                },
                                delta,
                                &mut proposal_nearest_scratch,
                                &mut working_nearest,
                                &mut touched_rows,
                                coverage_radius,
                            )?;
                            executed_touched_rows = executed_touched_rows
                                .checked_add(touched_rows.len())
                                .ok_or(SpecializedError::GateResourceOverflow)?;
                            &working_nearest
                        }
                    };
                    // FU-8e phase E3.  A variant that moved an uncovered row
                    // re-runs the full assessment, and that assessment's work
                    // is charged HERE, as it is spent, rather than bounded up
                    // front: the batch cannot know how many variants will need
                    // one without evaluating them.  Spending before checking is
                    // only acceptable because the identical shape sits a few
                    // lines below in this same loop — `executed_load_work`
                    // against `load_delta_limit` — and it is called out rather
                    // than left to be noticed.  Both spends draw on the ONE
                    // headroom `load_delta_limit` states, so it is their SUM
                    // that is checked; metering them separately would let the
                    // pair exceed the `gate_operation_limit` the batch was
                    // admitted under.  `coverage_scratch.operations` is
                    // `assess_coverage`'s own stated cost for these samples,
                    // the number it already checks per call against
                    // `max_coverage_operations` — which, until now, was the
                    // only bound this stage's coverage work ever met.
                    if coverage_delta.requires_reassessment() {
                        executed_coverage_work = executed_coverage_work
                            .checked_add(coverage_scratch.operations)
                            .ok_or(SpecializedError::GateResourceOverflow)?;
                        let spent = executed_load_work
                            .total()?
                            .checked_add(executed_coverage_work)
                            .ok_or(SpecializedError::GateResourceOverflow)?;
                        if spent > load_delta_limit {
                            return Err(SpecializedError::GateOperationLimit {
                                required: spent,
                                limit: load_delta_limit,
                            });
                        }
                    }
                    let coverage_objective = variant_coverage_objective(
                        coverage_question,
                        nearest,
                        coverage_delta,
                        baseline_coverage_objective,
                    )?;
                    // FU-8d phase F3 (P2): variant 0 reads the batch's one
                    // finalized partition; a proposal variant re-answers only
                    // `elig(left) ∪ elig(right)` against it.
                    let load_violations = match proposal {
                        None => load_state.baseline.violation_count(),
                        Some(proposal) => {
                            if load_state.merged_seat_is_specialized(
                                contacts,
                                generic.islands(),
                                proposal,
                            )? {
                                load_state.merged_rows(
                                    proposal.left,
                                    proposal.right,
                                    &mut merged_rows,
                                );
                                let outcome = load_state.baseline.merged_violation_count(
                                    proposal.left,
                                    proposal.right,
                                    proposal.position_mm,
                                    &merged_rows,
                                    &mut load_scratch,
                                )?;
                                executed_load_work.record(outcome.work)?;
                                // FU-8e phase E3: plus whatever the coverage
                                // fallbacks have spent out of the same
                                // headroom.  It is zero unless a variant moved
                                // an uncovered row.
                                let spent = executed_load_work
                                    .total()?
                                    .checked_add(executed_coverage_work)
                                    .ok_or(SpecializedError::GateResourceOverflow)?;
                                if spent > load_delta_limit {
                                    return Err(SpecializedError::GateOperationLimit {
                                        required: spent,
                                        limit: load_delta_limit,
                                    });
                                }
                                outcome.violation_count
                            } else {
                                // The one shape the delta's locality argument
                                // does not cover, and one a production merge
                                // batch cannot produce: finalize it in full
                                // rather than answer it approximately.
                                variant_load_violation_count_in_full(
                                    contacts,
                                    proposal,
                                    generic,
                                    mesh,
                                    surface,
                                    islands,
                                    reservations,
                                    &mut face_scratch,
                                    limits,
                                )?
                            }
                        }
                    };
                    let unsupported_specialized_count = variant_unsupported_count(
                        contacts,
                        proposal,
                        authoritative_demands,
                        &batch_index,
                        layers,
                        attach_reach,
                    );
                    vectors.push(FrozenSpecializedGateVector::from_live(
                        coverage_objective,
                        load_violations,
                        unsupported_specialized_count,
                    ));
                    // Restore the baseline rows this variant borrowed, so the
                    // next proposal starts from the batch's own answers.
                    for &row in &touched_rows {
                        working_nearest[row] = baseline_nearest[row];
                    }
                }
            }
            executed_nearest.record_demand_rows(
                proposals
                    .len()
                    .checked_mul(authoritative_demands.len())
                    .ok_or(SpecializedError::GateResourceOverflow)?,
            )?;
            executed_whole_gate_route_plan.nearest = executed_nearest;
            executed_whole_gate_route_plan.touched_rows = executed_touched_rows;
            if whole_gate_route_plan != executed_whole_gate_route_plan {
                return Err(SpecializedError::GateRoutePlanMismatch);
            }
            Ok(vectors)
        },
    )
}

/// Build the merge gate's one set of load inputs and its one baseline
/// partition (FU-8d phase F3, part P2).  See [`GateLoadState`].
#[allow(clippy::too_many_arguments)]
fn build_gate_load_state(
    contacts: &[WorkingContact],
    generic: &GenericContactPlan,
    mesh: &Mesh,
    surface: &NativeSurface,
    islands: &[SupportIsland],
    island_face_count: usize,
    reservations: &[SpecializedOwnerReservation],
    face_scratch: &mut Vec<u32>,
    work: &mut usize,
    limit: usize,
    limits: SpecializedLimits,
) -> Result<GateLoadState, SpecializedError> {
    let mut physical = Vec::with_capacity(contacts.len());
    let mut owner_faces = Vec::new();
    let mut pair_starts = Vec::with_capacity(contacts.len() + 1);
    append_variant_load_inputs(
        contacts,
        None,
        generic.islands(),
        islands,
        reservations,
        &mut physical,
        &mut owner_faces,
        Some(&mut pair_starts),
        face_scratch,
        work,
        limit,
    )?;
    let load_limits = LoadFinalizeLimits {
        max_scratch_bytes: limits.max_load_scratch_bytes,
        max_operations_floor: limits.max_load_operations_floor,
        operations_per_batch_unit: limits.load_work_per_batch_unit,
    };
    // `specialized_load_resource_plan` is the ONE authority for what the load
    // phase costs and holds.  The gate no longer states a hand-built model of
    // it beside it — the `variants × faces × contacts` term FU-8d part (2)
    // named as under-charged is gone, and this figure, for one baseline, is
    // what `gate_resource_plan` charges in its place.
    let baseline_resource = specialized_load_resource_plan(
        generic,
        physical.len(),
        SpecializedLoadCounts::bounded(owner_faces.len(), island_face_count),
    )?;
    let (partition, baseline) = SpecializedLoadBaseline::build(
        generic,
        mesh,
        surface,
        &physical,
        &owner_faces,
        load_limits,
    )?;
    // `face_row` is one `u32` per mesh face, independent of the batch's
    // contact count.
    let mut face_row = vec![u32::MAX; mesh.tris().len()];
    let mut row_face = Vec::with_capacity(partition.rows().len());
    for (row, load) in partition.rows().iter().enumerate() {
        row_face.push(load.face_id);
        // Rows are ascending by face id, so the first row carrying a face is
        // the one recorded and the rest of its run follows contiguously.
        if face_row[load.face_id as usize] == u32::MAX {
            face_row[load.face_id as usize] =
                u32::try_from(row).map_err(|_| SpecializedError::GateResourceOverflow)?;
        }
    }
    let mut contact_rows = vec![0usize; contacts.len()];
    for (contact, count) in contact_rows.iter_mut().enumerate() {
        for &(_, face_id) in &owner_faces[pair_starts[contact]..pair_starts[contact + 1]] {
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
    // One unit per pair inspected while indexing them, the same unit every
    // other descriptor scan in this stage charges.
    charge_load_descriptor_work(work, owner_faces.len(), limit)?;
    Ok(GateLoadState {
        physical,
        owner_faces,
        pair_starts,
        face_row,
        row_face,
        contact_rows,
        baseline,
        baseline_operations: baseline_resource.operations,
        baseline_scratch_bytes: baseline_resource.scratch_bytes,
    })
}

/// What the batch's retained load state holds, charged as one term of the gate
/// peak (FU-8d phase F3, part P2).
///
/// It replaces `matrix_bytes`, and it is smaller by construction: one physical
/// list and one pair list for the batch where there were up to
/// `max_gate_window_variants` of each.
fn gate_load_state_bytes(
    state: &GateLoadState,
    scratch: &SpecializedLoadDeltaScratch,
) -> Result<usize, SpecializedError> {
    [
        checked_route_bytes::<PhysicalContact>(state.physical.len())?,
        checked_route_bytes::<(u32, u32)>(state.owner_faces.len())?,
        checked_route_bytes::<usize>(state.pair_starts.len())?,
        checked_route_bytes::<u32>(state.face_row.len())?,
        checked_route_bytes::<u32>(state.row_face.len())?,
        checked_route_bytes::<usize>(state.contact_rows.len())?,
        state.baseline.retained_bytes(),
        scratch.retained_bytes()?,
    ]
    .into_iter()
    .try_fold(0usize, |total, bytes| {
        total
            .checked_add(bytes)
            .ok_or(SpecializedError::GateResourceOverflow)
    })
}

/// The count-derived bound the plan charges for the per-proposal load deltas
/// (FU-8d phase F3, parts P2 and (3)).
///
/// `|elig(left) ∪ elig(right)|` is at most `|elig(left)| + |elig(right)|`, and
/// both counts are read from `contact_rows`, which was built once with the load
/// state.  The `+ 3` per proposal is the floor on touched contacts: `left`,
/// `right` and the merged seat are always re-evaluated.  Stating it before the
/// loop is what keeps the gate from having to spend the deltas in order to
/// discover it cannot afford them — the hole FU-8d part (3) names — and the
/// real spend is then metered against `load_delta_limit` as it happens, the
/// same shape `charge_load_descriptor_work` uses throughout this stage.
fn proposal_load_bound(
    state: &GateLoadState,
    proposals: &[MergeProposal],
) -> Result<usize, SpecializedError> {
    proposals.iter().try_fold(0usize, |total, proposal| {
        state.contact_rows[proposal.left]
            .checked_add(state.contact_rows[proposal.right])
            .and_then(|rows| rows.checked_add(3))
            .and_then(|rows| total.checked_add(rows))
            .ok_or(SpecializedError::GateResourceOverflow)
    })
}

/// Finalize one proposal variant's load partition in full and count its
/// violations, for the single shape
/// [`GateLoadState::merged_seat_is_specialized`] rejects.
///
/// This is the predecessor's per-variant path, kept for a case a production
/// merge batch cannot produce: it costs a whole finalize, which is exactly what
/// FU-8d phase F3 exists to stop doing 39,223 times, and it is reached only
/// when the merged seat would not serve `elig(left) ∪ elig(right)`.
#[allow(clippy::too_many_arguments)]
fn variant_load_violation_count_in_full(
    contacts: &[WorkingContact],
    proposal: &MergeProposal,
    generic: &GenericContactPlan,
    mesh: &Mesh,
    surface: &NativeSurface,
    islands: &[SupportIsland],
    reservations: &[SpecializedOwnerReservation],
    face_scratch: &mut Vec<u32>,
    limits: SpecializedLimits,
) -> Result<usize, SpecializedError> {
    let mut physical = Vec::with_capacity(contacts.len());
    let mut owner_faces = Vec::new();
    let mut work = 0usize;
    append_variant_load_inputs(
        contacts,
        Some(proposal),
        generic.islands(),
        islands,
        reservations,
        &mut physical,
        &mut owner_faces,
        None,
        face_scratch,
        &mut work,
        usize::MAX,
    )?;
    let partition = finalize_specialized_load_partition(
        generic,
        mesh,
        surface,
        &physical,
        &owner_faces,
        LoadFinalizeLimits {
            max_scratch_bytes: limits.max_load_scratch_bytes,
            max_operations_floor: limits.max_load_operations_floor,
            operations_per_batch_unit: limits.load_work_per_batch_unit,
        },
        None,
    )?;
    Ok(load_violation_count(&partition, &physical)?)
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

/// The retained-plus-largest-batch rule the nearest phase's byte bound follows.
/// [`GateNearestBatchPlan::peak_bytes`] is the production statement of it since
/// FU-8c; this remains as the pinned two-term form the byte tests exercise.
#[cfg(test)]
fn coverage_route_nearest_peak_bytes(
    retained_base_logical_bytes: usize,
    max_route_knn_logical_peak_bytes: usize,
) -> Result<usize, SpecializedError> {
    retained_base_logical_bytes
        .checked_add(max_route_knn_logical_peak_bytes)
        .ok_or(SpecializedError::GateResourceOverflow)
}

const GATE_FINGERPRINT_SEED: u64 = 0xcbf2_9ce4_8422_2325_u64;
const GATE_FINGERPRINT_PRIME: u64 = 0x100_0000_01b3;

fn mix_fingerprint(fingerprint: &mut u64, word: u64) {
    *fingerprint ^= word;
    *fingerprint = fingerprint.wrapping_mul(GATE_FINGERPRINT_PRIME);
}

/// The whole batch's coverage-nearest plan: one baseline sweep plus one
/// route-local delta per merge proposal (FU-8c, 2026-08-06).
///
/// Before FU-8c this was a per-window `GateRouteWindowPlan` folded over
/// `GateWholeRoutePlan`, because every variant planned and executed its own
/// full sweep.  The preflight/execution fingerprint agreement it existed to
/// prove is unchanged in kind: preflight derives this summary from the batch,
/// execution re-derives it while filling, and the two must match exactly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GateNearestBatchPlan {
    baseline_routes: usize,
    proposal_routes: usize,
    proposal_work: GateProposalRoutedWork,
    baseline_work: usize,
    retained_base_logical_bytes: usize,
    max_proposal_retained_bytes: usize,
    max_knn_logical_peak_bytes: usize,
    fingerprint: u64,
}

impl GateNearestBatchPlan {
    fn seeded() -> Self {
        Self {
            fingerprint: GATE_FINGERPRINT_SEED,
            ..Self::default()
        }
    }

    fn record_baseline(&mut self, planned: &CoverageRouteNearestPlan) {
        self.baseline_routes = planned.route_plans.len();
        self.baseline_work = planned.total_work;
        self.retained_base_logical_bytes = planned.retained_base_logical_bytes;
        self.max_knn_logical_peak_bytes = self
            .max_knn_logical_peak_bytes
            .max(planned.max_route_knn_logical_peak_bytes);
        for route in &planned.route_plans {
            for word in [
                u64::from(route.route.island_id),
                u64::from(route.route.component_id),
                route.source_count as u64,
                route.query_count as u64,
                route.knn.logical_peak_bytes() as u64,
            ] {
                mix_fingerprint(&mut self.fingerprint, word);
            }
        }
    }

    fn record_proposal(&mut self, planned: &ProposalNearestPlan) -> Result<(), SpecializedError> {
        self.proposal_routes = self
            .proposal_routes
            .checked_add(planned.deltas.len())
            .ok_or(SpecializedError::GateResourceOverflow)?;
        self.proposal_work.nearest_work = self
            .proposal_work
            .nearest_work
            .checked_add(planned.work)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        self.proposal_work.touched_rows = self
            .proposal_work
            .touched_rows
            .checked_add(planned.touched_rows)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        self.proposal_work.key_visits = self
            .proposal_work
            .key_visits
            .checked_add(planned.key_visits)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        self.max_proposal_retained_bytes = self
            .max_proposal_retained_bytes
            .max(planned.retained_logical_bytes);
        self.max_knn_logical_peak_bytes = self
            .max_knn_logical_peak_bytes
            .max(planned.max_knn_logical_peak_bytes);
        for delta in &planned.deltas {
            for word in [
                u64::from(delta.route.island_id),
                u64::from(delta.route.component_id),
                delta.source_count as u64,
                delta.query_count as u64,
            ] {
                mix_fingerprint(&mut self.fingerprint, word);
            }
        }
        // FU-8d F2: one batch answers the whole proposal, so its shape is mixed
        // once per proposal rather than once per perturbed key.
        for word in [
            planned.key_visits as u64,
            planned.merged_seat_knn.query_count() as u64,
            planned.merged_seat_knn.logical_peak_bytes() as u64,
        ] {
            mix_fingerprint(&mut self.fingerprint, word);
        }
        Ok(())
    }

    fn record_demand_rows(&mut self, rows: usize) -> Result<(), SpecializedError> {
        self.proposal_work.demand_rows = self
            .proposal_work
            .demand_rows
            .checked_add(rows)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        Ok(())
    }

    /// Bytes the nearest phase holds at its peak: the retained route tables of
    /// the baseline plan, the largest proposal delta's own buffers, and the
    /// largest single kNN batch.
    fn peak_bytes(&self) -> Result<usize, SpecializedError> {
        self.retained_base_logical_bytes
            .checked_add(self.max_proposal_retained_bytes)
            .and_then(|bytes| bytes.checked_add(self.max_knn_logical_peak_bytes))
            .ok_or(SpecializedError::GateResourceOverflow)
    }
}

#[cfg(test)]
thread_local! {
    /// How many times [`plan_proposal_nearest_delta`] has run **on this
    /// thread**.
    ///
    /// Test-only, and the pin behind FU-8d phase F3's "4 → 1": counting the
    /// passes is the only way to observe a *removed repetition*, since three of
    /// the four passes computed the same value the first one did and so left no
    /// trace in any result.
    /// `fu8d_a_gate_batch_plans_each_proposal_delta_exactly_once` reads it.
    ///
    /// Thread-local rather than process-wide because the test harness runs
    /// tests in parallel while the planning path is serial: a global counter
    /// would measure whatever else happened to be planning at the same moment.
    static PROPOSAL_DELTA_PLANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The batch's nearest plan **and** the per-proposal deltas it was summed
/// from, retained so no later site has to re-derive one (FU-8d phase F3).
///
/// # Why the deltas are retained rather than recomputed
///
/// FU-8d part (3) recorded that `plan_proposal_nearest_delta` ran **four**
/// times for every proposal in a batch — once to build the batch plan, once
/// inside the `exact_operations` fold that charges it, once inside
/// `whole_gate_route_preflight`'s own re-plan, and once more in the execution
/// loop — while the check that can refuse the batch sits after the first two.
/// Every one of those passes computes the same function of the same immutable
/// inputs (`contacts`, the proposal, the proposal-invariant [`GateBatchIndex`],
/// and the limits), so three of the four were pure repetition.  They are now
/// planned once, here, and read everywhere else: **4 → 1**.
///
/// The retention is charged.  `retained_plan_bytes` is the logical size of this
/// vector — one [`ProposalNearestPlan`] per proposal plus each plan's own
/// [`ProposalRouteDelta`] rows — and it enters [`GatePeakUsage`] as
/// `proposal_plan_bytes`.  Before F3 only the *widest* proposal's execution
/// scratch was charged (`max_proposal_retained_bytes`), because only one plan
/// existed at a time.
struct GatePlannedNearest {
    batch: GateNearestBatchPlan,
    proposals: Vec<ProposalNearestPlan>,
    retained_plan_bytes: usize,
}

/// Bound the per-proposal planning pass **before** running it (FU-8d part (3)).
///
/// The hole part (3) names is not only that the planning passes were repeated:
/// it is that *none of them was charged before it ran*.  A resource plan that
/// cannot be afforded until after it has been executed bounds nothing, and on
/// the Weeping Treant the gate burned 124 s deciding it was not allowed to
/// proceed.  The fix has two halves and this is the first: a bound derived from
/// the batch header alone — no route gather, no `BTreeMap`, no kNN arithmetic —
/// checked against the same materialized `gate_operation_limit` the batch will
/// be held to.
///
/// The bound is `Σ_proposals routes(left) + demands(left) + routes(right) +
/// demands(right)`, which is exactly [`ProposalNearestPlan::key_visits`]
/// summed, because [`push_contact_route_keys`] pushes one key per regular route
/// and one per demand and [`proposal_route_keys`] records the pushed length
/// before it sorts.  So this is not a *model* of the planning pass: it is the
/// planning pass's own charged term, computed from counts the header already
/// holds.  The second half is that the same term is then charged for real, as
/// `GateProposalRoutedWork::key_visits`; a `debug_assert` in
/// [`plan_gate_nearest_batch`] pins the two against each other.
///
/// Reading two `Vec` lengths per proposal is the one thing this cannot avoid,
/// and it is `4 × proposals` slot reads against a bound that is itself at least
/// the batch's `samples × contacts` floor — parts per million of what it
/// guards.
fn proposal_planning_precheck(
    contacts: &[WorkingContact],
    proposals: &[MergeProposal],
    limit: usize,
) -> Result<usize, SpecializedError> {
    let mut key_visits = 0usize;
    for proposal in proposals {
        for contact in [&contacts[proposal.left], &contacts[proposal.right]] {
            key_visits = key_visits
                .checked_add(contact.regular_routes.len())
                .and_then(|total| total.checked_add(contact.demands.len()))
                .ok_or(SpecializedError::GateResourceOverflow)?;
        }
    }
    if key_visits > limit {
        return Err(SpecializedError::GateOperationLimit {
            required: key_visits,
            limit,
        });
    }
    Ok(key_visits)
}

/// Plan the whole batch's nearest phase without gathering a point or building
/// an index: one baseline route plan, then one route-local delta per proposal.
///
/// FU-8d phase F3: the deltas are **retained** in the returned
/// [`GatePlannedNearest`] rather than discarded, and the loop below is now the
/// only place in a gate batch that runs one.  It is preceded by
/// [`proposal_planning_precheck`], so a batch whose planning pass alone would
/// exceed its operation cap is refused before the first key is gathered.
fn plan_gate_nearest_batch(
    contacts: &[WorkingContact],
    proposals: &[MergeProposal],
    coverage_samples: &[CoverageSample],
    demand_count: usize,
    index: &GateBatchIndex,
    limits: KnnBatchLimits,
    gate_operation_limit: usize,
) -> Result<GatePlannedNearest, SpecializedError> {
    let planning_bound = proposal_planning_precheck(contacts, proposals, gate_operation_limit)?;
    let mut plan = GateNearestBatchPlan::seeded();
    let baseline = plan_variant_nearest_routes(
        contacts,
        None,
        coverage_samples,
        limits,
        NearestWitnessRetention::RetainWitnesses,
    )?;
    plan.record_baseline(&baseline);
    let mut retained_plan_bytes = checked_route_bytes::<ProposalNearestPlan>(proposals.len())?;
    let mut retained = Vec::with_capacity(proposals.len());
    for proposal in proposals {
        let delta = plan_proposal_nearest_delta(contacts, proposal, index, limits)?;
        plan.record_proposal(&delta)?;
        retained_plan_bytes = checked_route_bytes::<ProposalRouteDelta>(delta.deltas.len())
            .and_then(|bytes| {
                retained_plan_bytes
                    .checked_add(bytes)
                    .ok_or(SpecializedError::GateResourceOverflow)
            })?;
        retained.push(delta);
    }
    plan.record_demand_rows(
        proposals
            .len()
            .checked_mul(demand_count)
            .ok_or(SpecializedError::GateResourceOverflow)?,
    )?;
    debug_assert_eq!(plan.proposal_work.key_visits, planning_bound);
    Ok(GatePlannedNearest {
        batch: plan,
        proposals: retained,
        retained_plan_bytes,
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GateWholeRoutePlan {
    window_count: usize,
    nearest: GateNearestBatchPlan,
    /// Sample rows the nearest deltas actually wrote, summed over proposals.
    ///
    /// FU-8d phase F3 re-founds `GateRoutePlanMismatch` on this kind of term.
    /// Before F3 the check compared a plan against a **second plan**: the
    /// preflight planned every proposal's delta, execution planned them all
    /// again, and the two were compared.  Execution now reads the batch's one
    /// plan, so that comparison would be a tautology.  What execution can still
    /// disagree with the plan about is what it *did*, and this is that: it is
    /// counted from `touched.len()` as the delta fills each variant, not read
    /// back from the plan.  The check therefore attests "the batch executed the
    /// work it planned, once per proposal and in order" rather than "two
    /// identical computations agreed".  That is a weaker statement than the
    /// predecessor's, and deliberately so — the predecessor bought its extra
    /// strength by paying for the whole planning pass twice, which is the
    /// defect FU-8d part (3) opened.
    touched_rows: usize,
    max_peak_bytes: usize,
}

impl GateWholeRoutePlan {
    fn record(&mut self, peak_bytes: usize) -> Result<(), SpecializedError> {
        self.window_count = self
            .window_count
            .checked_add(1)
            .ok_or(SpecializedError::GateResourceOverflow)?;
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

// FU-8d phase F3 (P2) removed `traverse_whole_gate_windows` and
// `gate_window_matrix_bytes`.  Both existed to size and bound the per-variant
// `PhysicalContact` / `(owner, face)` window matrices, and there are no such
// matrices any more: the batch holds one of each.  Nothing in the gate's byte
// bound varies by variant, so the peak is stated once for the batch by
// `whole_gate_route_preflight` and re-measured once by the execution.

/// Route-plan all windows before execution. Each window uses bounded temporary
/// `Vec`/`BTree*` payloads for counting and `KnnBatchPlan` arithmetic; it does
/// not gather points or construct an index.
#[allow(clippy::too_many_arguments)]
fn whole_gate_route_preflight(
    proposal_storage_bytes: usize,
    topology: &plinth_mesh::FaceTopology,
    islands: &[SupportIsland],
    coverage_samples: &[CoverageSample],
    planned_nearest: &GatePlannedNearest,
    load_baseline_bytes: usize,
    load_scratch_bytes: usize,
    index: &GateBatchIndex,
    limits: SpecializedLimits,
    gate_plan: &GateResourcePlan,
) -> Result<GateWholeRoutePlan, SpecializedError> {
    // FU-8d phase F3: the third of the four `plan_proposal_nearest_delta`
    // passes lived here.  The plan is now the caller's, computed once by
    // `plan_gate_nearest_batch` before the batch's operation check, and read.
    let nearest = planned_nearest.batch;
    let nearest_bytes = gate_nearest_array_bytes(coverage_samples.len())?;
    let nearest_witness_bytes = gate_nearest_witness_bytes(coverage_samples.len())?;
    let coverage_nearest_bytes = nearest.peak_bytes()?;
    let coverage_scratch = coverage_assessment_resource_plan(coverage_samples, topology)?;
    let coverage_retained_bytes = coverage_samples
        .len()
        .checked_mul(std::mem::size_of::<usize>())
        .ok_or(SpecializedError::GateResourceOverflow)?;
    // FU-8d phase F3 (P2): the window traversal is gone with the matrices it
    // sized.  Nothing in the gate's byte bound varies by variant any more — the
    // batch holds one physical list, one `(owner, face)` list, one finalized
    // partition and two `samples`-long nearest arrays — so the peak is one
    // number for the batch, stated once here and re-measured once by the
    // execution.
    let peak_bytes = check_gate_peak(
        GatePeakUsage {
            proposal_bytes: proposal_storage_bytes,
            result_bytes: gate_plan.vector_bytes,
            load_baseline_bytes,
            face_scratch_bytes: island_face_scratch_bytes(islands)?,
            batch_index_bytes: index.retained_bytes,
            nearest_bytes,
            nearest_witness_bytes,
            proposal_plan_bytes: planned_nearest.retained_plan_bytes,
            coverage_scratch_bytes: coverage_scratch.scratch_bytes,
            coverage_nearest_bytes,
            coverage_retained_bytes,
            load_scratch_bytes,
        },
        limits.max_gate_peak_bytes,
    )?;
    let mut preflight = GateWholeRoutePlan {
        nearest,
        touched_rows: nearest.proposal_work.touched_rows,
        ..GateWholeRoutePlan::default()
    };
    preflight.record(peak_bytes)?;
    Ok(preflight)
}

/// The two `samples`-long `f64` arrays the batch retains for the nearest phase:
/// the baseline answers and the working copy each variant perturbs and restores
/// (FU-8c).
fn gate_nearest_array_bytes(sample_count: usize) -> Result<usize, SpecializedError> {
    sample_count
        .checked_mul(2 * std::mem::size_of::<f64>())
        .ok_or(SpecializedError::GateResourceOverflow)
}

#[allow(clippy::too_many_arguments)]
fn whole_gate_route_preflight_then_execute<T>(
    proposal_storage_bytes: usize,
    topology: &plinth_mesh::FaceTopology,
    islands: &[SupportIsland],
    coverage_samples: &[CoverageSample],
    planned_nearest: &GatePlannedNearest,
    load_baseline_bytes: usize,
    load_scratch_bytes: usize,
    index: &GateBatchIndex,
    limits: SpecializedLimits,
    gate_plan: &GateResourcePlan,
    execute: impl FnOnce(GateWholeRoutePlan) -> Result<T, SpecializedError>,
) -> Result<T, SpecializedError> {
    preflight_then_execute(
        whole_gate_route_preflight(
            proposal_storage_bytes,
            topology,
            islands,
            coverage_samples,
            planned_nearest,
            load_baseline_bytes,
            load_scratch_bytes,
            index,
            limits,
            gate_plan,
        ),
        execute,
    )
}

/// How many nearest sources per coverage sample the **baseline** sweep retains.
///
/// A merge proposal removes exactly two contacts, so three retained witnesses
/// always leave at least one survivor wherever the route had three or more
/// servers; where it had fewer, the retained set *is* the whole set.  Either
/// way "which source is nearest among the survivors of a two-contact removal?"
/// is answerable from the retained slots alone — the same top-`k` argument
/// [`DEMAND_WITNESS_SLOTS`] makes for the demands, applied to the nearest
/// (FU-8d part (1), 2026-08-06).
///
/// **Ruling R-a (2026-08-06): retaining `k = min(3, source_count)` witnesses
/// leaves the sweep's *charge* exactly where it was, and that silence is
/// deliberate rather than an oversight.**  A `KnnBatchPlan`'s work term is
/// `source_count × query_count` and is k-independent, and
/// [`plan_route_points_nearest`] charges precisely that term, so the baseline
/// sweep costs what it cost before the retention existed.  What `k` does move
/// is bytes, and both terms are charged: the kernel's CSR pair arrays are
/// `query_count × k`, i.e. `+2 × query_count × 16 B` per route inside each
/// route's `KnnBatchPlan::logical_peak_bytes`, and the retained table itself is
/// charged as [`GatePeakUsage::nearest_witness_bytes`].
const NEAREST_WITNESS_SLOTS: usize = 3;

/// One retained baseline witness: a source the kernel named for one coverage
/// sample, kept in the kernel's own emission order.
///
/// Both fields are the kernel's own output, copied and never re-derived — the
/// distance in particular is `SquaredEuclidean` as `knn_batch_planned` computed
/// it, so a later comparison against a fresh kernel answer is a comparison of
/// two outputs of the same pure function.
#[derive(Clone, Copy, Debug, PartialEq)]
struct NearestWitness {
    /// The kernel's `item` for this neighbour: an index into the route's
    /// gathered source list, or [`NO_WITNESS`] for an unfilled slot.
    source_slot: usize,
    /// The kernel's squared distance for this neighbour.
    squared_distance: f64,
}

impl NearestWitness {
    const UNUSED: Self = Self {
        source_slot: NO_WITNESS,
        squared_distance: f64::INFINITY,
    };
}

/// The baseline sweep's retained witnesses: one fixed-width row per coverage
/// sample, in the kernel's emission order (ascending
/// `(squared_distance, source_slot)`), padded with [`NearestWitness::UNUSED`].
///
/// A sample carries exactly one route key, so one row per sample is a complete
/// record.  Rows for samples on a route no contact serves stay unused, which is
/// the same "stays infinite" case the nearest array itself has.
pub(crate) struct NearestWitnessTable {
    rows: Vec<[NearestWitness; NEAREST_WITNESS_SLOTS]>,
}

impl NearestWitnessTable {
    fn seeded(sample_count: usize) -> Self {
        Self {
            rows: vec![[NearestWitness::UNUSED; NEAREST_WITNESS_SLOTS]; sample_count],
        }
    }

    /// The baseline's nearest **squared** distance among the sources that
    /// survive removing `pair`, or `+∞` when the removal leaves none.
    ///
    /// This is the read FU-8d phase F2 exists for, and it is a *value* lookup
    /// rather than a re-derivation: the returned `f64` is the bits the kernel
    /// emitted during the baseline sweep, so a caller may compare it against a
    /// fresh kernel answer by `to_bits`.  `sources` must be
    /// [`GateBatchIndex::sources_for`]'s ascending contact list for the route
    /// this sample carries — the same list the retained `source_slot` indexes,
    /// because the baseline gather walks contacts in ascending index and
    /// deduplicates by `physical_id`, which *is* the contact index for
    /// variant 0.
    ///
    /// Correctness rests on the retention lemma
    /// (`fu8d_three_retained_witnesses_answer_any_two_removal`): a merge
    /// proposal removes exactly two contacts, so scanning at most
    /// [`NEAREST_WITNESS_SLOTS`] slots either finds a survivor — necessarily
    /// *the* nearest survivor, because the slots are in the kernel's ascending
    /// `(distance, item)` emission order — or proves the route had at most two
    /// servers and the removal took all of them.
    fn surviving_squared_distance(
        &self,
        sample_index: usize,
        sources: &[usize],
        pair: MergedPair,
    ) -> f64 {
        self.rows[sample_index]
            .iter()
            .find(|witness| {
                witness.source_slot != NO_WITNESS && !pair.contains(sources[witness.source_slot])
            })
            .map_or(f64::INFINITY, |witness| witness.squared_distance)
    }
}

/// Bytes the baseline witness table retains for the whole batch (FU-8d).
fn gate_nearest_witness_bytes(sample_count: usize) -> Result<usize, SpecializedError> {
    sample_count
        .checked_mul(std::mem::size_of::<[NearestWitness; NEAREST_WITNESS_SLOTS]>())
        .ok_or(SpecializedError::GateResourceOverflow)
}

/// Whether one route-batched nearest sweep retains its top-`k` witnesses.
///
/// The choice is the caller's because it is the caller that knows whether
/// anything will read them, and asking for neighbours nobody reads would move
/// that caller's charged logical peak for nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NearestWitnessRetention {
    /// One neighbour per query, answer only.  The coverage **measurement** path
    /// takes this arm: it evaluates no merge proposals, so it has nothing to
    /// answer from witnesses, and its charged peak is unchanged by FU-8d.
    NearestOnly,
    /// `min(NEAREST_WITNESS_SLOTS, source_count)` neighbours per query,
    /// retained.  The merge gate's baseline sweep takes this arm (FU-8d).
    RetainWitnesses,
}

impl NearestWitnessRetention {
    /// Neighbours to request for a route with `source_count` distinct servers.
    ///
    /// `KnnBatchPlan::new` refuses `neighbors > source_count`, and every route
    /// planned here has at least one server, so the clamp is what keeps a
    /// one-server or two-server route planning at all.
    const fn neighbors_for(self, source_count: usize) -> usize {
        match self {
            Self::NearestOnly => 1,
            Self::RetainWitnesses => {
                if source_count < NEAREST_WITNESS_SLOTS {
                    source_count
                } else {
                    NEAREST_WITNESS_SLOTS
                }
            }
        }
    }
}

pub(crate) fn plan_route_points_nearest(
    points: &[CoverageRoutePoint],
    samples: &[CoverageSample],
    limits: KnnBatchLimits,
    retention: NearestWitnessRetention,
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
        let knn = KnnBatchPlan::new(
            source_ids.len(),
            query_count,
            retention.neighbors_for(source_ids.len()),
            limits,
        )
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
    retention: NearestWitnessRetention,
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
    plan_route_points_nearest(&points, samples, limits, retention)
}

fn fill_variant_nearest(
    contacts: &[WorkingContact],
    proposal: Option<&MergeProposal>,
    samples: &[CoverageSample],
    nearest: &mut [f64],
    planned: CoverageRouteNearestPlan,
    witnesses: Option<&mut NearestWitnessTable>,
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
    fill_route_points_nearest(&points, samples, nearest, planned, witnesses)
}

/// Fill one variant's nearest answers from a checked route plan, optionally
/// retaining that plan's top-`k` witnesses per sample.
///
/// **The answers are exactly what they were before FU-8d.** `nearest[sample]`
/// is `squared_distances[row.start].sqrt()` at every `k`, and row 0 of a
/// planned kNN batch does not depend on `k`: the boundary is the max over the
/// `k` candidates, the tie refetch is inclusive with `tie_cap = source_count`,
/// so a larger `k` only enlarges the candidate set, and the row is sorted by
/// the total order `(distance.total_cmp, item)` before truncation.  Pinned
/// against the kernel itself by
/// `fu8d_knn_row_zero_is_k_independent_bit_for_bit`, over engineered exact ties
/// as well as random clouds.
pub(crate) fn fill_route_points_nearest(
    points: &[CoverageRoutePoint],
    samples: &[CoverageSample],
    nearest: &mut [f64],
    planned: CoverageRouteNearestPlan,
    mut witnesses: Option<&mut NearestWitnessTable>,
) -> Result<(), SpecializedError> {
    debug_assert_eq!(nearest.len(), samples.len());
    debug_assert!(witnesses
        .as_ref()
        .is_none_or(|table| table.rows.len() == samples.len()));
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
            // The answer, unchanged: row start, one square root, as before.
            nearest[sample_index] = rows.squared_distances[row.start].sqrt();
            if let Some(table) = witnesses.as_mut() {
                let slots = &mut table.rows[sample_index];
                for (slot, position) in slots.iter_mut().zip(row.clone()) {
                    *slot = NearestWitness {
                        source_slot: usize::try_from(rows.point_ids[position])
                            .map_err(|_| SpecializedError::GateResourceOverflow)?,
                        squared_distance: rows.squared_distances[position],
                    };
                }
                // Slot 0 *is* the answer's own row, not a re-derivation of it.
                debug_assert_eq!(
                    slots[0].squared_distance.to_bits(),
                    rows.squared_distances[row.start].to_bits()
                );
                debug_assert_eq!(
                    u64::try_from(slots[0].source_slot).unwrap_or(u64::MAX),
                    rows.point_ids[row.start]
                );
            }
        }
    }
    Ok(())
}

/// The predecessor's per-variant demand-satisfaction predicate, retained as the
/// **reference** the FU-8c witness path is proved against
/// (`fu8c_witness_demand_counts_reproduce_the_full_sweep`).  It is no longer on
/// the hot path — a `demands × contacts` sweep per variant is the quadratic
/// FU-8c removed — so it is test-only by construction rather than by accident.
#[cfg(test)]
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

/// The two contacts a merge proposal replaces with one merged seat.
///
/// Every routed row a variant differs from the baseline in is reachable from
/// this pair alone; see [`GateBatchIndex`] for the argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MergedPair {
    left: usize,
    right: usize,
}

impl MergedPair {
    const fn of(proposal: &MergeProposal) -> Self {
        Self {
            left: proposal.left,
            right: proposal.right,
        }
    }

    const fn contains(self, index: usize) -> bool {
        index == self.left || index == self.right
    }
}

/// Sentinel for an unused slot in [`GateBatchIndex::demand_witnesses`].  A
/// contact index is an index into the batch's `contacts`, so `usize::MAX` can
/// never be one.
const NO_WITNESS: usize = usize::MAX;

/// How many baseline satisfying contacts one demand retains.
///
/// A merge proposal removes exactly two contacts, so three retained witnesses
/// always leave at least one survivor whenever the baseline had three or more.
/// If the baseline had fewer than three, the retained set *is* the whole set.
/// Either way "does some contact outside the merged pair satisfy this demand?"
/// is answered from the retained slots alone.
const DEMAND_WITNESS_SLOTS: usize = 3;

/// Route key of one regular route.
const fn regular_route_key(route: &RegularContactRoute) -> CoverageRoute {
    CoverageRoute {
        island_id: route.owner_island_id,
        component_id: route.component_id,
    }
}

/// Route key of one specialized demand.
const fn demand_route_key(demand: &SpecializedDemand) -> CoverageRoute {
    CoverageRoute {
        island_id: demand.owner_island_id,
        component_id: demand.coverage_component_id,
    }
}

/// Append every route key one contact serves, in the order the gate's point
/// gather emits them (regular routes first, then demands).
fn push_contact_route_keys(contact: &WorkingContact, out: &mut Vec<CoverageRoute>) {
    out.extend(contact.regular_routes.iter().map(regular_route_key));
    out.extend(contact.demands.iter().map(demand_route_key));
}

/// Proposal-invariant gate state, built **once per batch** (FU-8c).
///
/// # Why one batch-level index replaces one full pass per proposal
///
/// The merge gate evaluates `proposals + 1` variants.  Variant 0 is the
/// baseline; variant `p + 1` replaces contacts `left` and `right` with a single
/// merged seat at `proposal.position_mm` whose route set is the *union* of the
/// two.  Every other contact keeps its position and its routes verbatim.
///
/// Two consequences follow, and they are what make the per-proposal work local:
///
/// - **Nearest.** A coverage sample's nearest distance is a minimum over the
///   contacts that serve *its own route key*.  If neither `left` nor `right`
///   serves that key, the variant's source set for it is bit-identical to the
///   baseline's — same members, same gather order — so the variant's answer is
///   the baseline's answer.  The merged seat serves exactly `K(left) ∪
///   K(right)`, so it can only perturb those same keys.  A proposal therefore
///   re-evaluates only the route keys its own pair serves.
/// - **Demand satisfaction.** A demand is satisfied by *some* contact.  Removing
///   two contacts can only unseat it if one of them was a witness, and three
///   retained witnesses per demand are enough to answer that without a scan
///   (see [`DEMAND_WITNESS_SLOTS`]).  The merged seat's own contribution is one
///   `satisfies` test.
///
/// The predecessor recomputed both from scratch for every variant, which is the
/// `proposals` factor the gate plan used to charge for
/// (`(samples + demands + 1) × (proposals × (contacts - 1) + contacts)`).
struct GateBatchIndex {
    /// Coverage-sample rows grouped by route key: ascending by key, ascending
    /// within each key, and therefore in the same order
    /// [`fill_route_points_nearest`] visits its queries.
    sample_rows: Vec<(CoverageRoute, Vec<usize>)>,
    /// Contact indices serving each route key: ascending by key, ascending
    /// within each key, and therefore in the same order
    /// [`fill_route_points_nearest`] gathers its sources.
    route_sources: Vec<(CoverageRoute, Vec<usize>)>,
    /// Up to [`DEMAND_WITNESS_SLOTS`] baseline contact indices satisfying each
    /// demand, ascending, padded with [`NO_WITNESS`].
    demand_witnesses: Vec<[usize; DEMAND_WITNESS_SLOTS]>,
    /// Logical bytes the three tables above retain together.
    retained_bytes: usize,
}

impl GateBatchIndex {
    /// Build the index.  Cost is one pass over the samples, one pass over the
    /// contacts' routes, and one baseline satisfaction sweep
    /// (`demands × contacts`) — i.e. exactly what a *single* variant used to
    /// cost, spent once for the whole batch.
    fn build(
        contacts: &[WorkingContact],
        samples: &[CoverageSample],
        demands: &[SpecializedDemand],
        layers: &LayerPlan,
        attach_reach: f64,
    ) -> Result<Self, SpecializedError> {
        let mut sample_map = BTreeMap::<CoverageRoute, Vec<usize>>::new();
        for (index, sample) in samples.iter().enumerate() {
            sample_map
                .entry(CoverageRoute {
                    island_id: sample.island_id,
                    component_id: sample.component_id,
                })
                .or_default()
                .push(index);
        }
        let mut source_map = BTreeMap::<CoverageRoute, Vec<usize>>::new();
        let mut keys = Vec::new();
        for (index, contact) in contacts.iter().enumerate() {
            keys.clear();
            push_contact_route_keys(contact, &mut keys);
            keys.sort_unstable();
            keys.dedup();
            for key in &keys {
                source_map.entry(*key).or_default().push(index);
            }
        }
        let mut demand_witnesses = Vec::with_capacity(demands.len());
        for demand in demands {
            let mut slots = [NO_WITNESS; DEMAND_WITNESS_SLOTS];
            let mut filled = 0usize;
            for (index, contact) in contacts.iter().enumerate() {
                if filled == DEMAND_WITNESS_SLOTS {
                    break;
                }
                if contact_serves_owner(contact, demand.owner_island_id)
                    && satisfies(contact.position_mm, demand, layers, attach_reach)
                {
                    slots[filled] = index;
                    filled += 1;
                }
            }
            demand_witnesses.push(slots);
        }
        let sample_rows: Vec<(CoverageRoute, Vec<usize>)> = sample_map.into_iter().collect();
        let route_sources: Vec<(CoverageRoute, Vec<usize>)> = source_map.into_iter().collect();
        let retained_bytes = Self::retained_bytes_for(&sample_rows, &route_sources, demands.len())?;
        Ok(Self {
            sample_rows,
            route_sources,
            demand_witnesses,
            retained_bytes,
        })
    }

    /// Logical element capacity of the three retained tables.  Row headers are
    /// charged per route key; payloads are charged per stored index.
    fn retained_bytes_for(
        sample_rows: &[(CoverageRoute, Vec<usize>)],
        route_sources: &[(CoverageRoute, Vec<usize>)],
        demand_count: usize,
    ) -> Result<usize, SpecializedError> {
        let table_bytes =
            |rows: &[(CoverageRoute, Vec<usize>)]| -> Result<usize, SpecializedError> {
                let headers = checked_route_bytes::<(CoverageRoute, Vec<usize>)>(rows.len())?;
                let payload = rows.iter().try_fold(0usize, |total, (_, indices)| {
                    total
                        .checked_add(indices.len())
                        .ok_or(SpecializedError::GateResourceOverflow)
                })?;
                checked_route_bytes::<usize>(payload)?
                    .checked_add(headers)
                    .ok_or(SpecializedError::GateResourceOverflow)
            };
        table_bytes(sample_rows)?
            .checked_add(table_bytes(route_sources)?)
            .and_then(|bytes| {
                demand_count
                    .checked_mul(std::mem::size_of::<[usize; DEMAND_WITNESS_SLOTS]>())
                    .and_then(|witnesses| bytes.checked_add(witnesses))
            })
            .ok_or(SpecializedError::GateResourceOverflow)
    }

    fn sample_rows_for(&self, route: CoverageRoute) -> &[usize] {
        self.sample_rows
            .binary_search_by(|(key, _)| compare_coverage_routes(*key, route))
            .map_or(&[], |position| self.sample_rows[position].1.as_slice())
    }

    fn sources_for(&self, route: CoverageRoute) -> &[usize] {
        self.route_sources
            .binary_search_by(|(key, _)| compare_coverage_routes(*key, route))
            .map_or(&[], |position| self.route_sources[position].1.as_slice())
    }

    /// The baseline's answer to "does a contact outside `pair` satisfy this
    /// demand?", read from the retained witnesses alone.
    fn demand_survives(&self, demand_index: usize, pair: MergedPair) -> bool {
        self.demand_witnesses[demand_index]
            .iter()
            .any(|&witness| witness != NO_WITNESS && !pair.contains(witness))
    }
}

fn compare_coverage_routes(left: CoverageRoute, right: CoverageRoute) -> Ordering {
    left.island_id
        .cmp(&right.island_id)
        .then_with(|| left.component_id.cmp(&right.component_id))
}

/// One route key a merge proposal perturbs, and the shape of the rows it
/// re-answers there.
///
/// It carried its own `KnnBatchPlan` before FU-8d phase F2, because each
/// perturbed key was re-swept against its whole surviving source set.  It no
/// longer does: the proposal asks the kernel for **one** batch over the single
/// merged seat covering every perturbed key at once, and takes the survivors'
/// side from the retained baseline witnesses.  `source_count` stays because it
/// is what the plan/execution fingerprint is stated over.
struct ProposalRouteDelta {
    route: CoverageRoute,
    source_count: usize,
    query_count: usize,
}

/// Everything one merge proposal's nearest phase costs and holds.
struct ProposalNearestPlan {
    /// Perturbed route keys that actually carry coverage samples, ascending.
    /// A key with no samples costs its visit and nothing else.
    deltas: Vec<ProposalRouteDelta>,
    /// Sample rows this proposal overwrites and then restores.  Each also pays
    /// one bounded witness scan (at most [`NEAREST_WITNESS_SLOTS`] slot tests)
    /// and one minimum against the merged seat's distance.
    touched_rows: usize,
    /// Route-key slots visited to build `K(left) ∪ K(right)`:
    /// `routes(left) + demands(left) + routes(right) + demands(right)`.  This
    /// is the term FU-8d part (3)'s pre-check is derived from, and it is
    /// charged here so that the cost of *deciding* a proposal's shape is not
    /// free (FU-8d part (1)/(3) corollary).
    key_visits: usize,
    /// The single merged-seat kNN batch: `1 × touched_rows`, the same
    /// `source_count × query_count` unit [`plan_route_points_nearest`] charges.
    ///
    /// Before FU-8d F2 this was `Σ_{perturbed routes} source_count ×
    /// query_count` — a full re-sweep of every perturbed key against its whole
    /// surviving source set, which on the Weeping Treant was 97.4 % of the
    /// gate's entire stated demand.  The survivors' contribution is now a
    /// *lookup* in the baseline witness table, so the only distances this
    /// proposal asks the kernel for are the merged seat's.
    work: usize,
    /// The merged seat's batch, planned before a point is gathered.
    merged_seat_knn: KnnBatchPlan,
    max_knn_logical_peak_bytes: usize,
    retained_logical_bytes: usize,
}

/// Route keys a proposal's merged seat serves: `K(left) ∪ K(right)`, ascending
/// and deduplicated, with the number of key slots visited to build them.
fn proposal_route_keys(
    contacts: &[WorkingContact],
    pair: MergedPair,
) -> (Vec<CoverageRoute>, usize) {
    let mut keys = Vec::new();
    push_contact_route_keys(&contacts[pair.left], &mut keys);
    push_contact_route_keys(&contacts[pair.right], &mut keys);
    let key_visits = keys.len();
    keys.sort_unstable();
    keys.dedup();
    (keys, key_visits)
}

/// How many distinct source seats a perturbed route has in the proposal's
/// variant: the baseline's servers minus whichever of the merged pair were
/// among them, plus the one merged seat.
///
/// The merged seat always serves a *perturbed* route by construction, so the
/// `+ 1` is unconditional here and the count is never zero.
fn proposal_route_source_count(sources: &[usize], pair: MergedPair) -> usize {
    let removed = usize::from(sources.binary_search(&pair.left).is_ok())
        + usize::from(sources.binary_search(&pair.right).is_ok());
    sources.len() - removed + 1
}

/// Plan one proposal's nearest delta: the route keys it perturbs, the sample
/// rows those keys carry, and the **one** merged-seat kNN batch that answers
/// all of them (FU-8d phase F2, 2026-08-06).
///
/// The predecessor planned one kNN batch per perturbed key over that key's
/// whole surviving source set, so it charged `Σ source_count × query_count` —
/// on the Weeping Treant, 196,264,386,588 of a stated 201,480,535,329.  The
/// survivors' answer is now read from the baseline witness table instead of
/// recomputed, so the only distances still asked for are the merged seat's:
/// `1 × touched_rows`, one batch for the whole proposal.
fn plan_proposal_nearest_delta(
    contacts: &[WorkingContact],
    proposal: &MergeProposal,
    index: &GateBatchIndex,
    limits: KnnBatchLimits,
) -> Result<ProposalNearestPlan, SpecializedError> {
    #[cfg(test)]
    PROPOSAL_DELTA_PLANS.with(|count| count.set(count.get() + 1));
    let pair = MergedPair::of(proposal);
    let (keys, key_visits) = proposal_route_keys(contacts, pair);
    let mut deltas = Vec::with_capacity(keys.len());
    let mut touched_rows = 0usize;
    for route in keys {
        let query_count = index.sample_rows_for(route).len();
        if query_count == 0 {
            continue;
        }
        let sources = index.sources_for(route);
        let source_count = proposal_route_source_count(sources, pair);
        touched_rows = touched_rows
            .checked_add(query_count)
            .ok_or(SpecializedError::GateResourceOverflow)?;
        deltas.push(ProposalRouteDelta {
            route,
            source_count,
            query_count,
        });
    }
    // Checked as an operation demand before the kernel's own work check, so a
    // proposal too wide for this batch refuses with the gate's error rather
    // than with a `KnnBatchPlanError` string.  The merged seat is one point, so
    // `1 × touched_rows` is the work either way.
    if touched_rows > limits.max_work {
        return Err(SpecializedError::GateOperationLimit {
            required: touched_rows,
            limit: limits.max_work,
        });
    }
    // FU-8e phase E2 (2026-08-07): UNCHANGED, deliberately.  Execution stopped
    // building a one-point kd-tree, but this plan still describes one — the same
    // `KnnBatchPlan::new(1, touched_rows, 1, limits)`, so `work()` is still
    // `1 × touched_rows` and the treant's stated gate demand does not move by a
    // single unit.  `logical_peak_bytes()` therefore still carries the Kiddo
    // terms for a tree nobody builds and the CSR terms for a structure nobody
    // materialises: at `source_count == 1` that over-states the executed peak by
    // 64 B per proposal (leaf coordinates, leaf items, leaf extents, stems, the
    // build sort indices and the inclusive tie scratch) plus 16 B per query row
    // (the CSR offsets and point IDs; the CSR *distances* term is real — it is
    // exactly the retained output buffer the fast path fills).
    //
    // The over-statement is conservatism, not staleness, and it is kept on
    // purpose: a plan that admits the general path a fortiori admits its
    // specialisation, so nothing this branch executes can exceed what was
    // checked.  Restating the formula would move a charged number — the gate
    // peak, via `GatePlannedNearest::peak_bytes` — for a saving of tens of bytes
    // per proposal, and every record site that quotes the demand would have to
    // move with it.
    let merged_seat_knn = KnnBatchPlan::new(1, touched_rows, 1, limits)
        .map_err(|error| SpecializedError::CoverageNearest(error.to_string()))?;
    let retained_logical_bytes = [
        checked_route_bytes::<ProposalRouteDelta>(deltas.len())?,
        // The merged seat's own source point, and the gathered query points and
        // restored-row record, all `touched_rows` long.
        checked_route_bytes::<[f64; 3]>(1)?,
        checked_route_bytes::<[f64; 3]>(touched_rows)?,
        checked_route_bytes::<usize>(touched_rows)?,
    ]
    .into_iter()
    .try_fold(0usize, |total, bytes| {
        total
            .checked_add(bytes)
            .ok_or(SpecializedError::GateResourceOverflow)
    })?;
    Ok(ProposalNearestPlan {
        deltas,
        touched_rows,
        key_visits,
        work: merged_seat_knn.work(),
        max_knn_logical_peak_bytes: merged_seat_knn.logical_peak_bytes(),
        merged_seat_knn,
        retained_logical_bytes,
    })
}

/// The batch-level state one merge proposal's nearest delta reads: the coverage
/// samples, the proposal-invariant [`GateBatchIndex`], and the baseline sweep's
/// retained witnesses (FU-8d phase F2).
///
/// They are grouped because they are only meaningful together — a witness's
/// `source_slot` indexes [`GateBatchIndex::sources_for`] for the route one of
/// *these* samples carries — so passing them separately would invite pairing a
/// witness table with the wrong batch.
#[derive(Clone, Copy)]
struct GateNearestBaseline<'a> {
    samples: &'a [CoverageSample],
    index: &'a GateBatchIndex,
    witnesses: &'a NearestWitnessTable,
    /// The batch's baseline nearest answers (FU-8e phase E3).
    ///
    /// [`fill_proposal_nearest_delta`] reads each row's *baseline* value out of
    /// the **working** array, immediately before overwriting it, because the
    /// caller restores every touched row from this one after each variant — so
    /// the two agree bit for bit on entry, and the working read costs nothing
    /// on a cache line the store is about to claim anyway.  This slice is what
    /// turns that invariant from prose into a `debug_assert`: it is checked on
    /// every delta row in a debug or test build and compiled away in release.
    nearest: &'a [f64],
}

/// The two buffers one proposal's nearest delta refills: gathered query points
/// in, merged-seat squared distances out.
///
/// Both are retained for the whole gate batch and reused by every proposal, so
/// neither is a per-proposal allocation.  They travel together because they are
/// only correct together — `merged_squared[i]` is the distance to
/// `queries[i]` — and because the batch that fills them fills both at once.
///
/// Neither adds an uncharged byte.  `queries` is the FU-8d gather buffer under a
/// new name, charged as the plan's gathered-query-coordinates term and again as
/// `ProposalNearestPlan::retained_logical_bytes`.  `merged_squared` is exactly
/// the CSR squared-distances term (`query_count × neighbors × 8` at
/// `neighbors == 1`) that [`KnnBatchPlan::logical_peak_bytes`] already charges —
/// the fast path writes the same values into the caller's buffer instead of into
/// a `NeighborCsr` it would then throw away.
#[derive(Default)]
struct ProposalNearestScratch {
    queries: Vec<[f64; 3]>,
    merged_squared: Vec<f64>,
}

/// What one proposal's nearest delta observed about the rows it rewrote, as
/// far as [`assess_coverage`] can tell them apart (FU-8e phase E3).
///
/// The merge gate consumes exactly one thing from a variant's coverage
/// assessment — [`CoverageObjective`] — and the objective is *not* a function
/// of the whole nearest array.  It is a function of the **uncovered rows**:
/// which they are, and what value the uncovered ones hold.  This counts the
/// delta rows that can move either, so the caller can tell a variant whose
/// objective is the batch baseline's from one whose is not.
///
/// Both counters are folded into the delta's own row loop, which the plan
/// already charges `touched_rows` for; neither adds a charged unit, and neither
/// adds a load — `previous` and `updated` are already in registers there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ProposalCoverageDelta {
    /// Rows whose covered/uncovered **status** changed: `nearest > radius`
    /// answers differently before and after.  Recorded because it is the
    /// headline rate for the record (measured zero on both proposal-carrying
    /// corpus models); the reassessment trigger is the field below, which
    /// strictly contains it.
    flipped_rows: usize,
    /// Rows that force a full reassessment: a row that is uncovered on
    /// **either** side and whose value moved.
    ///
    /// This is a superset of `flipped_rows` — a status change needs two
    /// different values, one of them above the radius — and the extra rows it
    /// catches are the ones a pure flip test misses.  See
    /// [`variant_coverage_objective`] for why they are not optional.
    moved_uncovered_rows: usize,
}

impl ProposalCoverageDelta {
    /// Fold one rewritten row in.  `previous` is the row's baseline value and
    /// `updated` is the variant's; `radius_mm` is the same `f64` the coverage
    /// assessment compares against, and the comparison is the same `>`.
    #[inline]
    fn observe(&mut self, previous: f64, updated: f64, radius_mm: f64) {
        let was_uncovered = previous > radius_mm;
        let is_uncovered = updated > radius_mm;
        if was_uncovered != is_uncovered {
            self.flipped_rows += 1;
        }
        if (was_uncovered || is_uncovered) && previous.to_bits() != updated.to_bits() {
            self.moved_uncovered_rows += 1;
        }
    }

    /// Whether this variant needs its own [`assess_coverage`] call.
    const fn requires_reassessment(self) -> bool {
        self.moved_uncovered_rows > 0
    }
}

/// One variant's [`CoverageObjective`]: the batch baseline's, or a fresh
/// assessment (FU-8e phase E3, 2026-08-07).
///
/// # What the gate actually asked for
///
/// The merge gate calls [`assess_coverage`] once per variant and keeps
/// `objective`; `failing_sample_indices` and `patches` are built and dropped.
/// On the Weeping Treant that is 39,224 assessments of 405,146 samples per
/// merge round, 17.05 s of a 63.83 s round in the design pass — and the cost is
/// not allocation churn.  In `RepairNoEvidence` mode the traversal only visits
/// *uncovered* rows, but it brackets that traversal with two unconditional full
/// scans of the 3.24 MB nearest array: the uncovered filter and the
/// `worst_gap_mm` fold.  About 254 GB of reads per gate batch, at memory
/// bandwidth.  So the fix is to not scan, not to not allocate.
///
/// # Why the baseline's objective is the variant's, exactly
///
/// A proposal variant's nearest array differs from the baseline's only at the
/// rows its delta rewrote, and the caller restores those rows afterwards.  Take
/// a variant where every rewritten row was `observe`d as neither flipping nor
/// moving-while-uncovered.  Then:
///
/// - the **uncovered set** is identical.  Only a rewritten row can change
///   status, and none did.  Every downstream structure — the sort, the
///   `(island, face)` groups, the connected patches, the area floor test — is a
///   pure function of that set with the samples and topology, both
///   proposal-invariant here, so `failing_uncovered_area_mm2`,
///   `max_patch_area_mm2` and `failing_patch_count` are identical, and so is
///   the failing-island set the last field folds over.
/// - `worst_gap_mm` is identical too, and this is the field that makes the
///   second counter necessary.  It is a `max` over the nearest **values** of
///   every sample in a failing island, so it is *not* determined by the
///   uncovered set alone: a row that stays uncovered but moves from 12 mm to
///   11 mm lowers it without flipping anything.  A covered row can never
///   determine it — a failing island holds at least one uncovered sample, whose
///   value exceeds the radius, which no covered value can — so it is enough to
///   know that no uncovered row moved.  That is exactly
///   `moved_uncovered_rows == 0`, and the fold then runs over identical values
///   in identical order for an identical `f64`.
///
/// The trigger is deliberately wider than the uncovered rows of a *failing*
/// island: telling those apart would mean retaining the baseline's failing
/// islands and charging their bytes against the gate peak, to save a fallback
/// on rows in patches under the area floor.  Uncovered rows are the rows the
/// traversal already treats as rare.
///
/// # The other arm is the same function it always was
///
/// A variant that does move an uncovered row calls [`assess_coverage`]
/// verbatim, on the same array, with the same limits.  There is no incremental
/// objective and no patch delta: the fallback is exact by being the original.
///
/// So bit-identity holds on both arms by construction, and what is
/// **model-dependent is only the speed**.  The fast path is taken whenever a
/// merge leaves the uncovered rows alone.  Measured 2026-08-07 on the corpus,
/// that is *every* proposal of `make_raking_blade` — the only proposal-carrying
/// model among the seven frozen digests — with not one fallback and not one
/// moved uncovered row across its gate batches; the Weeping Treant's rate is
/// phase E4's measurement and is not claimed here.  A model whose merges churn
/// uncovered geometry would fall back more often and land back at the pre-FU-8e
/// cost.  It can never land on a different answer.
fn variant_coverage_objective(
    question: GateCoverageQuestion<'_>,
    nearest: &[f64],
    delta: ProposalCoverageDelta,
    baseline_objective: CoverageObjective,
) -> Result<CoverageObjective, SpecializedError> {
    debug_assert!(
        delta.flipped_rows <= delta.moved_uncovered_rows,
        "a status flip is a moved uncovered row"
    );
    if !delta.requires_reassessment() {
        return Ok(baseline_objective);
    }
    question.objective_of(nearest)
}

/// Everything the merge gate's coverage assessment holds still for a whole
/// batch: the samples, their topology, the radius that decides covered from
/// uncovered, the area floor that decides a failing patch from a tolerable one,
/// and the limits the assessment checks itself against (FU-8e phase E3).
///
/// They travel together because the batch asks the *same* question of every
/// variant and only the nearest array differs — and because the baseline's
/// assessment and a fallback variant's must be the same call, or the fast path
/// would be comparing answers to two different questions.
#[derive(Clone, Copy)]
struct GateCoverageQuestion<'a> {
    samples: &'a [CoverageSample],
    topology: &'a plinth_mesh::FaceTopology,
    radius_mm: f64,
    area_floor_mm2: f64,
    limits: CoverageAssessmentLimits,
}

impl GateCoverageQuestion<'_> {
    /// [`assess_coverage`], verbatim, keeping the one field the gate consumes.
    fn objective_of(self, nearest: &[f64]) -> Result<CoverageObjective, SpecializedError> {
        Ok(assess_coverage(
            self.samples,
            self.topology,
            nearest,
            self.radius_mm,
            self.area_floor_mm2,
            self.limits,
        )?
        .objective)
    }
}

/// Overwrite `nearest` in place with one proposal variant's answers, recording
/// every row written into `touched` so the caller can restore the baseline.
///
/// # Why this is the full sweep's answer, bit for bit
///
/// A perturbed key's variant source set is `survivors(key) ∪ {merged seat}`,
/// and the variant's answer for one sample is the minimum squared distance over
/// that set, square-rooted once.  The minimum splits exactly:
///
/// - **over the survivors** — read from the baseline witness table as a
///   *value*, never recomputed.  The kernel wrote those bits during the
///   baseline sweep, and three retained slots answer any two-contact removal
///   (see [`NearestWitnessTable::surviving_squared_distance`]).
/// - **over the merged seat** — one kernel batch, `SquaredEuclidean` over the
///   single merged point.  It is the kernel's own answer and not a
///   hand-computed distance, so it is the same pure function applied to the
///   same two points the full sweep would have applied it to.
///
/// # Why the merged seat's batch no longer builds a tree (FU-8e phase E2)
///
/// That batch used to be a [`PointCloudIndex::new_for_knn_batch`] over one point
/// followed by `knn_batch_planned`.  FU-8d's merge message blamed the resulting
/// 62 % of the gate's variant loop on building those trees; the FU-8e
/// measurement says otherwise.  One such index costs 83.3 ns, and the 39,223 of
/// them in a Weeping Treant gate batch total ≈3.27 ms — the trees are
/// free.  (Arithmetic corrected 2026-08-07, records fix: this total was first
/// written as "16.7 microseconds", an arithmetic slip in the design pass rather
/// than a second measurement — 39,223 × 83.3 ns = 3.267 ms, and 16.7 µs would
/// have implied 0.426 ns per construction.  The per-build 83.3 ns is retained as
/// measured; only the product is corrected, and the conclusion this comment
/// rests on is unchanged: 3.27 ms is 0.009 % of the 37.33 s
/// `fill_proposal_nearest_delta` frame, so construction is negligible and the
/// fix targets the query loop.  Primary note:
/// `docs/rust-migration/support-resource-caps-2026-08-05.md` §12.2.)
/// The cost was the *query* loop: per query, a `nearest_n` descent that
/// allocates a `SortedVec`, a second `nearest_n_within_exclusive` descent that
/// allocates a `Vec`, and a `sort_by`, for about 1.04e9 query iterations and
/// 2.08e9 heap allocations per batch.
///
/// So this now calls [`knn_single_source_planned_into`], which answers the same
/// question without the tree.  It takes the **same** [`KnnBatchPlan`] this
/// proposal already planned, so the charged demand is untouched (see
/// [`plan_proposal_nearest_delta`]), and it is a specialisation rather than a
/// second opinion: at `source_count == 1` its value is bit-identical to
/// `squared_distances[row.start]`, argued in five clauses on that function and
/// pinned by `fu8e_single_source_is_the_tree_answer_bit_for_bit`.  The point ID
/// the old path read is dropped because it was never consulted: one source seat
/// means `point_ids[row.start]` was always `0`.
///
/// The proof that this changed nothing is that
/// `fu8d_proposal_nearest_deltas_reproduce_the_full_sweep_bit_for_bit` — which
/// compares every row against the retained full sweep on `to_bits` — was not
/// rewritten for it and still passes.
///
/// The two are combined by `f64::total_cmp` and the square root is taken
/// **once, after** the minimum — so the arithmetic performed on the winning
/// value is identical to the sweep's `squared_distances[row.start].sqrt()`.
/// An exact tie between a survivor and the merged seat is answered by two
/// bit-identical `f64`s, which is why the direction the tie breaks cannot move
/// the answer.  Pinned against the retained full sweep by
/// `fu8d_proposal_nearest_deltas_reproduce_the_full_sweep_bit_for_bit`.
///
/// `scratch` is the caller's reusable pair of buffers: one allocation each for
/// the whole batch rather than one per proposal.
///
/// The read-only pieces of batch state travel together in
/// [`GateNearestBaseline`], because they are only ever correct *together*: the
/// witnesses index the batch index's source lists, and both are stated over
/// these samples.
///
/// # What it reports back (FU-8e phase E3)
///
/// The returned [`ProposalCoverageDelta`] is folded in this loop rather than
/// scanned for afterwards, because the loop already holds what it needs: the
/// row's baseline value, which the caller restores and this function is about
/// to overwrite, and the winner it is about to write.  `coverage_radius_mm` is
/// the same radius the variant's assessment will compare against, and the
/// comparison is the same `>`, so the two agree on every row by construction.
/// The rows it counts are already charged — they are `touched_rows`, which
/// [`plan_proposal_nearest_delta`] states and the gate has already paid for —
/// so this adds no operation to any demand.  See [`variant_coverage_objective`]
/// for what the caller does with it.
fn fill_proposal_nearest_delta(
    proposal: &MergeProposal,
    baseline: GateNearestBaseline<'_>,
    plan: &ProposalNearestPlan,
    scratch: &mut ProposalNearestScratch,
    nearest: &mut [f64],
    touched: &mut Vec<usize>,
    coverage_radius_mm: f64,
) -> Result<ProposalCoverageDelta, SpecializedError> {
    let GateNearestBaseline {
        samples,
        index,
        witnesses,
        nearest: baseline_nearest,
    } = baseline;
    debug_assert_eq!(nearest.len(), samples.len());
    debug_assert_eq!(baseline_nearest.len(), samples.len());
    debug_assert_eq!(witnesses.rows.len(), samples.len());
    let mut coverage_delta = ProposalCoverageDelta::default();
    if plan.deltas.is_empty() {
        return Ok(coverage_delta);
    }
    let pair = MergedPair::of(proposal);
    scratch.queries.clear();
    scratch.queries.reserve(plan.touched_rows);
    for delta in &plan.deltas {
        let rows = index.sample_rows_for(delta.route);
        debug_assert_eq!(rows.len(), delta.query_count);
        scratch
            .queries
            .extend(rows.iter().map(|&row| samples[row].point_mm));
    }
    debug_assert_eq!(scratch.queries.len(), plan.touched_rows);
    knn_single_source_planned_into(
        &proposal.position_mm,
        &scratch.queries,
        &plan.merged_seat_knn,
        &mut scratch.merged_squared,
    )
    .map_err(|error| SpecializedError::CoverageNearest(error.to_string()))?;
    debug_assert_eq!(scratch.merged_squared.len(), plan.touched_rows);
    let mut query_row = 0usize;
    for delta in &plan.deltas {
        let sources = index.sources_for(delta.route);
        for &sample_index in index.sample_rows_for(delta.route) {
            let merged_squared = scratch.merged_squared[query_row];
            let survivor_squared =
                witnesses.surviving_squared_distance(sample_index, sources, pair);
            let winner = if survivor_squared.total_cmp(&merged_squared).is_le() {
                survivor_squared
            } else {
                merged_squared
            };
            // The row's baseline value, read from the working array on the
            // cache line the store below is about to claim.  The caller
            // restores every touched row from `baseline_nearest` after each
            // variant, so the two hold the same bits here; that invariant is
            // asserted rather than assumed in debug and test builds.
            let previous = nearest[sample_index];
            debug_assert_eq!(
                previous.to_bits(),
                baseline_nearest[sample_index].to_bits(),
                "row {sample_index} was not restored to the batch baseline"
            );
            let updated = winner.sqrt();
            coverage_delta.observe(previous, updated, coverage_radius_mm);
            nearest[sample_index] = updated;
            touched.push(sample_index);
            query_row += 1;
        }
    }
    debug_assert_eq!(query_row, plan.touched_rows);
    Ok(coverage_delta)
}

/// Count the demands one variant leaves unsupported, from the batch index
/// rather than from a fresh `demands × contacts` sweep.
///
/// Equivalent to `authoritative_demands.iter().filter(|d|
/// !variant_satisfies_demand(..)).count()` by construction: the first disjunct
/// of `variant_satisfies_demand` asks whether some contact outside the merged
/// pair satisfies the demand, which [`GateBatchIndex::demand_survives`]
/// answers, and the second disjunct is reproduced verbatim below.
fn variant_unsupported_count(
    contacts: &[WorkingContact],
    proposal: Option<&MergeProposal>,
    demands: &[SpecializedDemand],
    index: &GateBatchIndex,
    layers: &LayerPlan,
    attach_reach: f64,
) -> usize {
    let Some(proposal) = proposal else {
        // Variant 0 removes nothing, so a demand is supported exactly when it
        // retained at least one witness.
        return (0..demands.len())
            .filter(|&demand_index| {
                index.demand_witnesses[demand_index]
                    .iter()
                    .all(|&witness| witness == NO_WITNESS)
            })
            .count();
    };
    let pair = MergedPair::of(proposal);
    demands
        .iter()
        .enumerate()
        .filter(|(demand_index, demand)| {
            !(index.demand_survives(*demand_index, pair)
                || ((contact_serves_owner(&contacts[pair.left], demand.owner_island_id)
                    || contact_serves_owner(&contacts[pair.right], demand.owner_island_id))
                    && satisfies(proposal.position_mm, demand, layers, attach_reach)))
        })
        .count()
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
    scratch: &mut Vec<u32>,
    work: &mut usize,
    limit: usize,
    visit: impl FnMut(u32) -> Result<(), SpecializedError>,
) -> Result<Option<u32>, SpecializedError> {
    if let Some(owner) = generic_owner_for_parts(parts, generic_islands, work, limit)? {
        return Ok(Some(owner));
    }
    visit_served_owner_faces(parts, islands, scratch, work, limit, visit)?;
    Ok(None)
}

/// Visit the ascending distinct face ids of every island `parts` serves.
///
/// The emitted sequence is exactly `BTreeSet` order over the served islands'
/// face ids, which is what keeps the load rows canonical even when island face
/// ranges interleave.  It is produced by gathering each served island's ids
/// once and collapsing the gather in one batch, so each island is tested for
/// service exactly once and each id is inspected exactly once.  Its predecessor
/// re-scanned every island — service test included — once per emitted face, a
/// selection sort quadratic in the served face count (FU-8b defect A).
///
/// `scratch` is a caller-owned buffer reused across contacts and variants; see
/// [`unique_island_face_count`] for its bound.
fn visit_served_owner_faces(
    parts: ContactParts<'_>,
    islands: &[SupportIsland],
    scratch: &mut Vec<u32>,
    work: &mut usize,
    limit: usize,
    mut visit: impl FnMut(u32) -> Result<(), SpecializedError>,
) -> Result<(), SpecializedError> {
    scratch.clear();
    for island in islands {
        if !parts_serves_owner(parts, island.id, work, limit)? {
            continue;
        }
        gather_island_faces(scratch, island, work, limit)?;
    }
    distinct_faces_in_place(scratch, work, limit)?;
    for &face_id in scratch.iter() {
        visit(face_id)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn variant_specialized_owner_face_count(
    contacts: &[WorkingContact],
    proposal: Option<&MergeProposal>,
    generic_islands: &[SupportIsland],
    islands: &[SupportIsland],
    scratch: &mut Vec<u32>,
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
            scratch,
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
            scratch,
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
    mut pair_starts: Option<&mut Vec<usize>>,
    scratch: &mut Vec<u32>,
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
    // FU-8d phase F3 (P2): `pair_starts`, when asked for, records where each
    // appended contact's `(owner, face)` run begins, so a later reader can
    // recover `elig(contact)` without a second gather.  It is the merge gate's
    // handle on `elig(left) ∪ elig(right)`; every other caller passes `None`.
    if let Some(starts) = pair_starts.as_deref_mut() {
        starts.clear();
    }
    let mut append =
        |position_mm: [f64; 3], parts: ContactParts<'_>| -> Result<(), SpecializedError> {
            if let Some(starts) = pair_starts.as_deref_mut() {
                starts.push(specialized_owner_faces.len());
            }
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
                visit_served_owner_faces(parts, islands, scratch, work, limit, |face_id| {
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
    if let Some(starts) = pair_starts {
        starts.push(specialized_owner_faces.len());
    }
    Ok(())
}

/// The merge gate's load state, built **once** for the batch (FU-8d phase F3,
/// part P2).
///
/// It holds variant 0's load inputs, the baseline partition those inputs
/// finalize to, and the two index arrays a proposal needs to name
/// `elig(left) ∪ elig(right)` without re-gathering anything:
///
/// - `pair_starts`, from [`append_variant_load_inputs`], slices
///   `owner_faces` into one ascending distinct face run per contact;
/// - `face_row` maps a face id to the first load row carrying it, and
///   `row_face` lets the run of rows carrying that face be walked (rows are
///   ascending by face id, so equal faces are contiguous — a base authority
///   that carries the same face twice therefore yields two rows, and both are
///   found).
///
/// Because each contact's face run is ascending and rows are ordered by face,
/// a contact's eligible rows come out ascending, and two contacts' runs merge
/// in one linear pass with no sort.
///
/// **The per-variant window matrices are gone, not merely unpriced.** Before
/// F3 the gate materialized, for every window of up to
/// `max_gate_window_variants` variants, one `PhysicalContact` matrix and one
/// `(owner, face)` matrix covering every variant in the window. On the Weeping
/// Treant the second of those is 7,971,451 pairs × 8 B **per variant**;
/// `gate_window_matrix_bytes` therefore states a figure in the tens of
/// gigabytes against `max_gate_contact_matrix_bytes` = 512 MiB, and the only
/// reason that check never fired is that `exact_operations` refused the batch
/// first. Removing the operations wall without removing the matrices would
/// have moved the refusal onto a byte limit instead. There is now exactly one
/// physical list and one pair list for the whole batch.
struct GateLoadState {
    physical: Vec<PhysicalContact>,
    owner_faces: Vec<(u32, u32)>,
    pair_starts: Vec<usize>,
    face_row: Vec<u32>,
    row_face: Vec<u32>,
    /// How many load rows each baseline contact is eligible for.  Read by the
    /// plan's count-derived bound; never used as an answer.
    contact_rows: Vec<usize>,
    baseline: SpecializedLoadBaseline,
    baseline_operations: usize,
    baseline_scratch_bytes: usize,
}

impl GateLoadState {
    /// `elig(left) ∪ elig(right)`, ascending and deduplicated, in one linear
    /// merge over the two contacts' ascending face runs.
    fn merged_rows(&self, left: usize, right: usize, out: &mut Vec<u32>) {
        out.clear();
        let (mut index, end) = (self.pair_starts[left], self.pair_starts[left + 1]);
        let (mut other, other_end) = (self.pair_starts[right], self.pair_starts[right + 1]);
        while index < end || other < other_end {
            let face_id = if index < end && other < other_end {
                let (a, b) = (self.owner_faces[index].1, self.owner_faces[other].1);
                match a.cmp(&b) {
                    std::cmp::Ordering::Less => {
                        index += 1;
                        a
                    }
                    std::cmp::Ordering::Greater => {
                        other += 1;
                        b
                    }
                    std::cmp::Ordering::Equal => {
                        index += 1;
                        other += 1;
                        a
                    }
                }
            } else if index < end {
                let a = self.owner_faces[index].1;
                index += 1;
                a
            } else {
                let b = self.owner_faces[other].1;
                other += 1;
                b
            };
            let mut row = self.face_row[face_id as usize];
            if row == u32::MAX {
                continue;
            }
            while (row as usize) < self.row_face.len() && self.row_face[row as usize] == face_id {
                out.push(row);
                row += 1;
            }
        }
        debug_assert!(out.windows(2).all(|pair| pair[0] < pair[1]));
    }

    /// Whether this proposal's merged seat serves exactly
    /// `elig(left) ∪ elig(right)`, which is what the delta assumes.
    ///
    /// It does whenever all three of `left`, `right` and the merged parts
    /// resolve to *specialized* owners, and in a production merge batch they
    /// always do: `gather_pair_proposals` only offers a pair when **both**
    /// members carry demands, and `generic_owner_for_parts` returns `None` for
    /// any parts that carry a demand.  The check is here anyway because the
    /// consequence of being wrong is silent — a generic merged seat serves the
    /// base rows of one island instead of the union, stranding every row only
    /// the pair served — and because a caller outside that construction path
    /// would otherwise get a wrong answer rather than a slow one.  When it is
    /// false the caller finalizes that variant in full.
    fn merged_seat_is_specialized(
        &self,
        contacts: &[WorkingContact],
        generic_islands: &[SupportIsland],
        proposal: &MergeProposal,
    ) -> Result<bool, SpecializedError> {
        if !matches!(
            self.physical[proposal.left].owner,
            LoadOwner::Specialized(_)
        ) || !matches!(
            self.physical[proposal.right].owner,
            LoadOwner::Specialized(_)
        ) {
            return Ok(false);
        }
        let mut work = 0usize;
        let merged = generic_owner_for_parts(
            ContactParts::new(&contacts[proposal.left], Some(&contacts[proposal.right])),
            generic_islands,
            &mut work,
            usize::MAX,
        )?;
        Ok(merged.is_none())
    }
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
        SpecializedLimits::production(TEST_PAIR_KNN_LIMITS, crate::specialized::test_batch_work()),
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
    hash.usize(limits.max_gate_operations_floor);
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
    hash.usize(limits.max_gate_operations_floor);
    hash.usize(limits.max_gate_window_variants);
    hash.usize(limits.max_coverage_scratch_bytes);
    hash.usize(limits.max_coverage_operations);
    hash.usize(limits.max_load_scratch_bytes);
    hash.usize(limits.max_load_operations_floor);
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
    hash.usize(limits.max_load_operations_floor);
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
    use std::cell::Cell;
    use std::collections::BTreeSet;

    use super::*;
    use geom_kernel::KnnBatchLimits;
    use plinth_core::{Finite, PositiveFinite};
    use plinth_mesh::{make_overhang_table, make_raking_blade, make_thin_fin, make_toed_slab};

    /// The gate's own kNN limits for tests.  `max_neighbors` matches what
    /// `SpecializedLimits::production` states for `coverage_route_knn`: the
    /// baseline sweep asks for `min(NEAREST_WITNESS_SLOTS, source_count)`
    /// neighbours, so a ceiling of 1 would refuse every witness-retaining plan
    /// (FU-8d ruling R-b; a `KnnBatchLimits` shape field, not an FU-8 cap).
    const TEST_KNN_LIMITS: KnnBatchLimits = KnnBatchLimits {
        max_source_points: 20_000_000,
        max_query_points: 20_000_000,
        max_neighbors: NEAREST_WITNESS_SLOTS,
        max_logical_peak_bytes: 2 * 1024 * 1024 * 1024,
        max_work: 100_000_000,
    };

    /// Re-pinned 2026-08-06 (FU-8d phase F3, part P2).  The predecessor drove
    /// `traverse_whole_gate_windows` over five variants in windows of two and
    /// asserted that a failure in the *second* window stopped the traversal and
    /// never reached execution.  There are no windows left to traverse — the
    /// per-variant matrices they sized are gone — so what survives, and what is
    /// pinned here, is the ordering guarantee itself: a failed preflight is
    /// never executed.
    #[test]
    fn a_failed_gate_preflight_is_never_executed() {
        let execution_calls = Cell::new(0usize);
        let result = preflight_then_execute(
            Err(SpecializedError::GateOperationLimit {
                required: 5,
                limit: 4,
            }),
            |_: GateWholeRoutePlan| {
                execution_calls.set(execution_calls.get() + 1);
                Ok(())
            },
        );

        assert_eq!(
            result,
            Err(SpecializedError::GateOperationLimit {
                required: 5,
                limit: 4,
            })
        );
        assert_eq!(execution_calls.get(), 0);
    }

    /// Re-pinned 2026-08-06 (FU-8d phase F3, part P2).  This pinned that a
    /// window traversal carried the *maximum* window peak into the gate peak
    /// check; with the windows gone, what it pins is the check itself at the
    /// cap and one byte over.  The two numbers are the ones it always pinned.
    #[test]
    fn the_gate_peak_check_admits_the_cap_and_refuses_one_byte_over() {
        const CAP: usize = 32;
        let mut summary = GateWholeRoutePlan::default();
        summary.record(CAP - 1).expect("one peak recorded");
        assert_eq!(summary.window_count, 1);
        assert_eq!(summary.max_peak_bytes, CAP - 1);
        let usage = GatePeakUsage {
            proposal_bytes: 0,
            result_bytes: 0,
            load_baseline_bytes: summary.max_peak_bytes,
            // Re-pinned 2026-08-06 by record fix F6, which added
            // `face_scratch_bytes` to the retained total. Zero here: this test
            // pins how a *window summary's* max peak flows into the check, and
            // the face buffer is not the term under test. Both assertions below
            // keep their pre-F6 values, which is the point of pinning it at 0.
            face_scratch_bytes: 0,
            // Zero for the same reason, added 2026-08-06 (FU-8c).
            batch_index_bytes: 0,
            nearest_bytes: 0,
            // Zero for the same reason, added 2026-08-06 (FU-8d).
            nearest_witness_bytes: 0,
            // Zero for the same reason, added 2026-08-06 (FU-8d phase F3).
            proposal_plan_bytes: 0,
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

    /// FU-8c equivalence fixture: six contacts over three route keys, coverage
    /// samples on each key plus one key no contact serves, and demands both
    /// inside and outside every contact's reach.
    fn fu8c_fixture() -> (
        Vec<WorkingContact>,
        Vec<CoverageSample>,
        Vec<SpecializedDemand>,
        Vec<MergeProposal>,
        LayerPlan,
    ) {
        let route = |candidate: u64, island: u32, component: u32, face: u32| RegularContactRoute {
            candidate_id: candidate,
            owner_island_id: island,
            coverage_face_id: face,
            component_id: component,
            source_kind: RepairRowSourceKind::Generic,
        };
        let up = [0.0, 0.0, 1.0];
        let mut contacts = vec![
            // Island 1 / component 10.
            test_regular_contact([0.0, 0.0, 0.0], up, route(1, 1, 10, 0), Vec::new()),
            test_regular_contact([2.0, 0.0, 0.0], up, route(2, 1, 10, 1), Vec::new()),
            test_regular_contact([4.0, 0.0, 0.0], up, route(3, 1, 10, 2), Vec::new()),
            // Island 1 / component 20.
            test_regular_contact([0.0, 3.0, 0.0], up, route(4, 1, 20, 3), Vec::new()),
            test_regular_contact([2.5, 3.0, 0.0], up, route(5, 1, 20, 4), Vec::new()),
            // Island 2 / component 10, and it also carries a demand on
            // island 2 / component 30, so one contact serves two route keys.
            test_regular_contact(
                [1.0, 1.0, 0.0],
                up,
                route(6, 2, 10, 5),
                vec![minimum_demand(2, 6, 30, [1.0, 1.0, 0.0])],
            ),
        ];
        // A contact whose only route is a demand, on island 1 / component 20.
        contacts.push(WorkingContact::specialized(minimum_demand(
            1,
            7,
            20,
            [4.0, 3.0, 0.0],
        )));

        let sample = |island: u32, component: u32, face: u32, point: [f64; 3]| CoverageSample {
            island_id: island,
            face_id: face,
            component_id: component,
            point_mm: point,
            area_mm2: 1.0,
        };
        let samples = vec![
            sample(1, 10, 0, [0.1, 0.0, 0.0]),
            sample(1, 10, 0, [1.0, 0.2, 0.0]),
            sample(1, 10, 1, [2.4, 0.0, 0.0]),
            sample(1, 10, 2, [3.9, 0.5, 0.0]),
            sample(1, 20, 3, [0.2, 3.0, 0.0]),
            sample(1, 20, 4, [2.0, 3.4, 0.0]),
            sample(1, 20, 4, [4.1, 3.0, 0.0]),
            sample(2, 10, 5, [1.2, 1.1, 0.0]),
            sample(2, 30, 6, [1.0, 1.4, 0.0]),
            // A route key no contact serves: must stay infinite in every variant.
            sample(9, 99, 7, [8.0, 8.0, 0.0]),
        ];
        let demands = vec![
            minimum_demand(1, 0, 10, [0.0, 0.0, 0.0]),
            minimum_demand(1, 4, 20, [2.5, 3.0, 0.0]),
            minimum_demand(2, 6, 30, [1.0, 1.0, 0.0]),
            // Nothing is within reach of this one in any variant.
            minimum_demand(1, 2, 10, [40.0, 40.0, 0.0]),
        ];

        let merged = |contacts: &[WorkingContact], left: usize, right: usize| MergeProposal {
            left,
            right,
            representative_component_id: 10,
            representative_position_mm: contacts[left].position_mm,
            representative_face_id: 0,
            representative_normal: up,
            primary_regular: contacts[left].primary_regular,
            seat_provenance: contacts[left].seat_provenance,
            position_mm: [
                (contacts[left].position_mm[0] + contacts[right].position_mm[0]) / 2.0,
                (contacts[left].position_mm[1] + contacts[right].position_mm[1]) / 2.0,
                (contacts[left].position_mm[2] + contacts[right].position_mm[2]) / 2.0,
            ],
            normal: up,
        };
        let proposals = vec![
            // Both members on one route key.
            merged(&contacts, 0, 1),
            merged(&contacts, 1, 2),
            // Members on different route keys, so the merged seat serves both.
            merged(&contacts, 0, 3),
            merged(&contacts, 2, 4),
            // A member that serves two route keys itself.
            merged(&contacts, 1, 5),
            // A member whose only route is a demand.
            merged(&contacts, 4, 6),
            merged(&contacts, 5, 6),
        ];
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).expect("layer plan");
        (contacts, samples, demands, proposals, layers)
    }

    /// FU-8c.  The batch-level nearest phase must answer every merge proposal
    /// **bit for bit** the way the predecessor's per-proposal full sweep did.
    ///
    /// This is the swap test: the reference side is the untouched
    /// `plan_variant_nearest_routes` / `fill_variant_nearest` pair evaluated
    /// with `Some(proposal)` — literally the code the hot path used to run once
    /// per variant — and the candidate side is the baseline sweep plus
    /// `plan_proposal_nearest_delta` / `fill_proposal_nearest_delta`.  Rows are
    /// compared by `f64::to_bits`, so an answer that is merely *close* fails.
    /// FU-8d phase F2 equivalence fixture: [`fu8c_fixture`] grown until every
    /// arm of the witness-plus-merged-seat delta is reached.
    ///
    /// Three contacts are added on a new route key `island 3 / component 10` at
    /// exactly representable coordinates, and the outer two also serve a second
    /// key `island 4 / component 10` that **only they** serve:
    ///
    /// ```text
    ///   A(-3, 0)          merged(A,B) = (-1, 0)          B(1, 0)
    ///                     C(0, 1)
    ///          S(0, 0)          T(-1, 0.05)          U(10, 0)  [on key (4,10)]
    /// ```
    ///
    /// - `S` — baseline nearest is `B` at `d² = 1`, and `B` is merged away, so
    ///   the witness scan must skip slot 0; the surviving `C` is also at
    ///   `d² = 1` and the merged seat is at `d² = 1`, an **exact three-way
    ///   tie** whose bits must survive the minimum and the single square root.
    /// - `T` — the baseline holder `C` *survives*, and the merged seat is
    ///   nearer (`d² = 0.0025` against `1.9025`): a **steal**.
    /// - `U` — key `(4, 10)` is served by `A` and `B` alone, so merging them
    ///   leaves **no survivor** and the merged seat answers by itself.
    fn fu8d_delta_fixture() -> (
        Vec<WorkingContact>,
        Vec<CoverageSample>,
        Vec<SpecializedDemand>,
        Vec<MergeProposal>,
        LayerPlan,
    ) {
        let (mut contacts, mut samples, demands, mut proposals, layers) = fu8c_fixture();
        let up = [0.0, 0.0, 1.0];
        let route = |candidate: u64, island: u32, component: u32, face: u32| RegularContactRoute {
            candidate_id: candidate,
            owner_island_id: island,
            coverage_face_id: face,
            component_id: component,
            source_kind: RepairRowSourceKind::Generic,
        };
        let left = contacts.len();
        for (candidate, face, position) in [
            (12u64, 12u32, [-3.0, 0.0, 0.0]),
            (13, 13, [1.0, 0.0, 0.0]),
            (14, 14, [0.0, 1.0, 0.0]),
        ] {
            contacts.push(test_regular_contact(
                position,
                up,
                route(candidate, 3, 10, face),
                Vec::new(),
            ));
        }
        let right = left + 1;
        let survivor = left + 2;
        // A second key served by the outer two and nobody else, so merging them
        // removes every server it has.
        contacts[left].regular_routes.push(route(15, 4, 10, 15));
        contacts[right].regular_routes.push(route(16, 4, 10, 16));

        for (island, component, face, point) in [
            (3u32, 10u32, 12u32, [0.0, 0.0, 0.0]),
            (3, 10, 13, [-1.0, 0.05, 0.0]),
            (4, 10, 15, [10.0, 0.0, 0.0]),
        ] {
            samples.push(CoverageSample {
                island_id: island,
                face_id: face,
                component_id: component,
                point_mm: point,
                area_mm2: 1.0,
            });
        }

        let merged = |contacts: &[WorkingContact], left: usize, right: usize| MergeProposal {
            left,
            right,
            representative_component_id: 10,
            representative_position_mm: contacts[left].position_mm,
            representative_face_id: 0,
            representative_normal: up,
            primary_regular: contacts[left].primary_regular,
            seat_provenance: contacts[left].seat_provenance,
            position_mm: [
                (contacts[left].position_mm[0] + contacts[right].position_mm[0]) / 2.0,
                (contacts[left].position_mm[1] + contacts[right].position_mm[1]) / 2.0,
                (contacts[left].position_mm[2] + contacts[right].position_mm[2]) / 2.0,
            ],
            normal: up,
        };
        proposals.extend([
            // The tie, the steal, and the key whose every server is removed.
            merged(&contacts, left, right),
            // Removals that leave a survivor on the wide key, from both sides.
            merged(&contacts, left, survivor),
            merged(&contacts, right, survivor),
            // A pair straddling two route keys, one of which nothing else
            // serves in the merged variant.
            merged(&contacts, 0, left),
        ]);
        (contacts, samples, demands, proposals, layers)
    }

    /// What one equivalence sweep actually exercised, so a fixture can be
    /// asserted **non-vacuous** rather than merely passing.
    #[derive(Debug, Default)]
    struct ProposalDeltaCoverage {
        /// Perturbed keys carrying fewer servers than the retained slot count.
        narrow_key_visits: usize,
        /// Touched rows where the removal left the route no server at all, so
        /// the survivors' side of the minimum is `+∞`.
        rows_with_no_survivor: usize,
        /// Touched rows whose baseline nearest was one of the merged pair.
        removed_nearest_rows: usize,
        /// Touched rows the merged seat wins outright from a *surviving*
        /// baseline holder — a steal.
        steal_rows: usize,
        /// Touched rows where a survivor and the merged seat are at exactly
        /// equal squared distance, bit for bit.
        exact_tie_rows: usize,
        /// Rows the reference full sweep moved off the baseline.
        moved_rows: usize,
    }

    /// Run the swap test: for every proposal, the retained `#[cfg(test)]`
    /// full-sweep reference against the delta path, row by row on
    /// `f64::to_bits`, plus the touched-superset and restore invariants.
    ///
    /// The reference side is `plan_variant_nearest_routes` /
    /// `fill_variant_nearest` with `Some(proposal)` — literally the code the
    /// hot path ran once per variant before FU-8c — and the candidate side is
    /// the baseline sweep plus `plan_proposal_nearest_delta` /
    /// `fill_proposal_nearest_delta`.  Returns what it observed so the caller
    /// can refuse a fixture that never reaches the interesting arms.
    fn assert_proposal_deltas_match_the_full_sweep(
        contacts: &[WorkingContact],
        samples: &[CoverageSample],
        demands: &[SpecializedDemand],
        proposals: &[MergeProposal],
        layers: &LayerPlan,
    ) -> ProposalDeltaCoverage {
        let index =
            GateBatchIndex::build(contacts, samples, demands, layers, 1.0).expect("batch index");

        let baseline_plan = plan_variant_nearest_routes(
            contacts,
            None,
            samples,
            TEST_KNN_LIMITS,
            NearestWitnessRetention::RetainWitnesses,
        )
        .unwrap();
        let mut baseline = vec![f64::INFINITY; samples.len()];
        let mut witnesses = NearestWitnessTable::seeded(samples.len());
        fill_variant_nearest(
            contacts,
            None,
            samples,
            &mut baseline,
            baseline_plan,
            Some(&mut witnesses),
        )
        .unwrap();

        // The kernel's own distance from one point to one query: the same pure
        // function the delta asks for the merged seat, used here only to
        // classify what each row exercised.
        let kernel_squared = |source: [f64; 3], query: [f64; 3]| -> f64 {
            let plan = KnnBatchPlan::new(1, 1, 1, TEST_KNN_LIMITS).expect("plan");
            let cloud = PointCloudIndex::new_for_knn_batch(&[source], &plan).expect("index");
            let answers = cloud.knn_batch_planned(&[query], &plan).expect("query");
            let row = answers.row_range(0).expect("planned row");
            answers.squared_distances[row.start]
        };

        let mut coverage = ProposalDeltaCoverage::default();
        let mut working = baseline.clone();
        let mut touched = Vec::new();
        let mut nearest_scratch = ProposalNearestScratch::default();

        for (proposal_index, proposal) in proposals.iter().enumerate() {
            let reference_plan = plan_variant_nearest_routes(
                contacts,
                Some(proposal),
                samples,
                TEST_KNN_LIMITS,
                NearestWitnessRetention::RetainWitnesses,
            )
            .unwrap();
            let mut reference = vec![f64::INFINITY; samples.len()];
            fill_variant_nearest(
                contacts,
                Some(proposal),
                samples,
                &mut reference,
                reference_plan,
                None,
            )
            .unwrap();

            touched.clear();
            let delta =
                plan_proposal_nearest_delta(contacts, proposal, &index, TEST_KNN_LIMITS).unwrap();
            // The plan's own shape is part of the claim: one merged-seat batch
            // covering exactly the rows the perturbed keys carry.
            assert_eq!(delta.merged_seat_knn.source_count(), 1);
            assert_eq!(delta.merged_seat_knn.query_count(), delta.touched_rows);
            assert_eq!(delta.work, delta.touched_rows);
            let pair = MergedPair::of(proposal);
            for route_delta in &delta.deltas {
                if index.sources_for(route_delta.route).len() < NEAREST_WITNESS_SLOTS {
                    coverage.narrow_key_visits += 1;
                }
            }
            fill_proposal_nearest_delta(
                proposal,
                GateNearestBaseline {
                    samples,
                    index: &index,
                    witnesses: &witnesses,
                    nearest: &baseline,
                },
                &delta,
                &mut nearest_scratch,
                &mut working,
                &mut touched,
                // The radius this fixture asserts nothing about: it drives only
                // the FU-8e coverage delta, which
                // `fu8e_coverage_delta_matches_assess_coverage` owns.  Every
                // row assertion below is on the nearest values themselves.
                1.0,
            )
            .unwrap();

            for row in 0..samples.len() {
                assert_eq!(
                    working[row].to_bits(),
                    reference[row].to_bits(),
                    "proposal {proposal_index} row {row}: delta {} vs full sweep {}",
                    working[row],
                    reference[row]
                );
            }
            // Every row the reference disagrees with the baseline on must have
            // been touched, or the delta got the right answer by luck.
            for row in 0..samples.len() {
                if reference[row].to_bits() != baseline[row].to_bits() {
                    coverage.moved_rows += 1;
                    assert!(
                        touched.contains(&row),
                        "proposal {proposal_index} moved row {row} without touching it"
                    );
                }
            }
            for &row in &touched {
                let sources = index.sources_for(CoverageRoute {
                    island_id: samples[row].island_id,
                    component_id: samples[row].component_id,
                });
                let survivor = witnesses.surviving_squared_distance(row, sources, pair);
                let merged = kernel_squared(proposal.position_mm, samples[row].point_mm);
                let holder = witnesses.rows[row][0].source_slot;
                let removed_nearest = holder != NO_WITNESS && pair.contains(sources[holder]);
                if survivor.is_infinite() {
                    coverage.rows_with_no_survivor += 1;
                }
                if removed_nearest {
                    coverage.removed_nearest_rows += 1;
                } else if survivor.is_finite() && merged < survivor {
                    coverage.steal_rows += 1;
                }
                if survivor.to_bits() == merged.to_bits() {
                    coverage.exact_tie_rows += 1;
                }
                working[row] = baseline[row];
            }
            assert!(
                working
                    .iter()
                    .zip(&baseline)
                    .all(|(left, right)| left.to_bits() == right.to_bits()),
                "proposal {proposal_index} did not restore the baseline"
            );
        }
        coverage
    }

    #[test]
    fn fu8c_proposal_nearest_deltas_reproduce_the_full_sweep_bit_for_bit() {
        let (contacts, samples, demands, proposals, layers) = fu8c_fixture();
        let baseline_plan = plan_variant_nearest_routes(
            &contacts,
            None,
            &samples,
            TEST_KNN_LIMITS,
            NearestWitnessRetention::RetainWitnesses,
        )
        .unwrap();
        let mut baseline = vec![f64::INFINITY; samples.len()];
        fill_variant_nearest(
            &contacts,
            None,
            &samples,
            &mut baseline,
            baseline_plan,
            None,
        )
        .unwrap();
        // The unserved route key really is unserved, or the fixture is not
        // exercising the infinite-row case it claims to.
        assert!(baseline[9].is_infinite());

        let coverage = assert_proposal_deltas_match_the_full_sweep(
            &contacts, &samples, &demands, &proposals, &layers,
        );
        assert!(
            coverage.moved_rows > 0,
            "no proposal moved a row, so agreement is vacuous"
        );
    }

    /// FU-8d phase F2, the swap test for the **new** delta.
    ///
    /// Same reference as FU-8c's — the retained `#[cfg(test)]` full sweep — but
    /// the candidate is now the merged-seat single batch plus witness-slot
    /// inspection, which is a different algorithm producing the same bits.  The
    /// fixture is grown past FU-8c's until every arm of that algorithm is
    /// reached, and the assertions below **refuse to pass vacuously**:
    ///
    /// - a perturbed key with fewer than [`NEAREST_WITNESS_SLOTS`] servers, so
    ///   the "retained set *is* the whole set" arm runs;
    /// - a proposal that removes *every* server of a key, so the survivors'
    ///   side of the minimum is `+∞` and the merged seat answers alone;
    /// - a proposal whose baseline nearest is one of the merged pair, so the
    ///   witness scan has to skip past slot 0;
    /// - a **steal** — the merged seat wins a row whose baseline holder
    ///   survived, which no removal-only argument would produce;
    /// - an **exact distance tie** between a survivor and the merged seat,
    ///   which is where a `partial_cmp`-shaped minimum or a per-side square
    ///   root would be free to disagree with the sweep and does not.
    #[test]
    fn fu8d_proposal_nearest_deltas_reproduce_the_full_sweep_bit_for_bit() {
        let (contacts, samples, demands, proposals, layers) = fu8d_delta_fixture();
        let coverage = assert_proposal_deltas_match_the_full_sweep(
            &contacts, &samples, &demands, &proposals, &layers,
        );
        assert!(
            coverage.moved_rows > 0,
            "no proposal moved a row, so agreement is vacuous"
        );
        assert!(
            coverage.narrow_key_visits > 0,
            "no perturbed key carried fewer than {NEAREST_WITNESS_SLOTS} servers"
        );
        assert!(
            coverage.rows_with_no_survivor > 0,
            "no proposal removed every server of a key it perturbs"
        );
        assert!(
            coverage.removed_nearest_rows > 0,
            "no touched row had its baseline nearest removed by the merge"
        );
        assert!(
            coverage.steal_rows > 0,
            "the merged seat never stole a row from a surviving holder"
        );
        assert!(
            coverage.exact_tie_rows > 0,
            "no survivor tied the merged seat exactly, so the tie arm is untested"
        );
    }

    /// Four samples on a closed tetrahedron, two islands, one sample per face.
    ///
    /// Faces of a tetrahedron are all mutually adjacent, but a coverage patch
    /// never crosses an island, so island 7 (faces 0, 1) and island 8
    /// (faces 2, 3) form two independent patches — which is what lets one
    /// island fail the area floor while the other passes, and therefore what
    /// lets `worst_gap_mm` fold over a strict subset of the samples.
    fn fu8e_coverage_fixture() -> (Mesh, Vec<CoverageSample>) {
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
        let samples = [(7u32, 0u32, 1.0), (7, 1, 1.0), (8, 2, 0.4), (8, 3, 0.4)]
            .into_iter()
            .map(|(island_id, face_id, area_mm2)| CoverageSample {
                island_id,
                face_id,
                component_id: 0,
                point_mm: [0.0, 0.0, 0.0],
                area_mm2,
            })
            .collect();
        (mesh, samples)
    }

    /// FU-8e phase E3 — **the coverage delta answers what `assess_coverage`
    /// would have answered, on both of its arms.**
    ///
    /// The gate keeps one thing from a variant's assessment, its
    /// [`CoverageObjective`], and E3 stops recomputing that objective for a
    /// variant that cannot have changed it.  This is the fixture that says when
    /// "cannot" is true.  Every case compares all four objective fields against
    /// a real [`assess_coverage`] call on the variant's own array —
    /// `f64::to_bits` on the three floats, so an answer that is merely close
    /// fails — and asserts what the delta counted while it got there.
    ///
    /// NON-VACUITY IS ASSERTED, NOT HOPED FOR.  Three of the five cases also
    /// assert that the true objective **differs** from the batch baseline's, so
    /// a `variant_coverage_objective` that lost its fallback — or a
    /// `ProposalCoverageDelta` that stopped counting — fails here rather than
    /// silently handing the gate a stale objective.  Two of those three flip a
    /// row's covered/uncovered status, one in each direction; the third flips
    /// nothing at all.
    #[test]
    fn fu8e_coverage_delta_matches_assess_coverage() {
        let (mesh, samples) = fu8e_coverage_fixture();
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let radius_mm = 1.0;
        let question = GateCoverageQuestion {
            samples: &samples,
            topology: &topology,
            radius_mm,
            area_floor_mm2: 0.5,
            limits: CoverageAssessmentLimits::UNBOUNDED,
        };
        let objective_of = |nearest: &[f64]| question.objective_of(nearest).expect("assessment");

        // Island 7 is uncovered and over the floor; island 8 is covered.
        let covered = [3.0, 2.0, 0.5, 0.5];
        // Island 8 uncovered too: its patch is 0.8 mm², also over the floor.
        let uncovered = [3.0, 2.0, 1.5, 1.5];
        let baseline_objective = objective_of(&covered);
        assert_eq!(baseline_objective.failing_patch_count, 1);
        assert_eq!(baseline_objective.worst_gap_mm, 3.0);

        struct Case {
            name: &'static str,
            baseline: [f64; 4],
            variant: [f64; 4],
            flips: usize,
            reassessment: bool,
        }
        let case = |name, baseline, variant, flips, reassessment| Case {
            name,
            baseline,
            variant,
            flips,
            reassessment,
        };
        let cases = [
            // The variant is the baseline: nothing to do, and the fast path
            // must be the one that does it.
            case("unchanged", covered, covered, 0, false),
            // A covered row moves and stays covered.  It cannot reach
            // `worst_gap_mm` — a failing island always holds a sample above the
            // radius, which no covered value can exceed — so the objective is
            // the baseline's, and the fast path must take it.
            case("covered row moves", covered, [3.0, 2.0, 0.9, 0.5], 0, false),
            // NO FLIP, BUT THE OBJECTIVE MOVES.  Row 0 stays uncovered and
            // drops from 3 mm to 2.5 mm, which lowers `worst_gap_mm` while the
            // uncovered set, the patches and the failing islands all stand
            // still.  A trigger that only watched for status flips would hand
            // the gate 3.0 here.
            case(
                "uncovered row moves without flipping",
                covered,
                [2.5, 2.0, 0.5, 0.5],
                0,
                true,
            ),
            // Covered → uncovered, both rows of island 8: a second failing
            // patch appears.
            case("covered becomes uncovered", covered, uncovered, 2, true),
            // …and back the other way, from the uncovered baseline.
            case("uncovered becomes covered", uncovered, covered, 2, true),
        ];

        for Case {
            name,
            baseline,
            variant,
            flips: expected_flips,
            reassessment: expected_reassessment,
        } in cases
        {
            let baseline_objective = objective_of(&baseline);
            let expected = objective_of(&variant);
            // The gate observes exactly the rows its delta rewrote.  Observing
            // every row instead is the same count: a row whose bits did not
            // move contributes to neither counter, and FU-8c's own equivalence
            // fixture already refuses a delta that moves a row it did not
            // touch.
            let mut delta = ProposalCoverageDelta::default();
            for row in 0..samples.len() {
                delta.observe(baseline[row], variant[row], radius_mm);
            }
            assert_eq!(delta.flipped_rows, expected_flips, "{name}: flips");
            assert_eq!(
                delta.requires_reassessment(),
                expected_reassessment,
                "{name}: reassessment"
            );
            if expected_reassessment {
                assert_ne!(
                    (
                        expected.failing_uncovered_area_mm2.to_bits(),
                        expected.max_patch_area_mm2.to_bits(),
                        expected.failing_patch_count,
                        expected.worst_gap_mm.to_bits(),
                    ),
                    (
                        baseline_objective.failing_uncovered_area_mm2.to_bits(),
                        baseline_objective.max_patch_area_mm2.to_bits(),
                        baseline_objective.failing_patch_count,
                        baseline_objective.worst_gap_mm.to_bits(),
                    ),
                    "{name}: the baseline objective would have been accepted, \
                     so the fallback is untested here"
                );
            }
            let answer = variant_coverage_objective(question, &variant, delta, baseline_objective)
                .expect("variant objective");
            assert_eq!(
                answer.failing_uncovered_area_mm2.to_bits(),
                expected.failing_uncovered_area_mm2.to_bits(),
                "{name}: failing_uncovered_area_mm2"
            );
            assert_eq!(
                answer.max_patch_area_mm2.to_bits(),
                expected.max_patch_area_mm2.to_bits(),
                "{name}: max_patch_area_mm2"
            );
            assert_eq!(
                answer.failing_patch_count, expected.failing_patch_count,
                "{name}: failing_patch_count"
            );
            assert_eq!(
                answer.worst_gap_mm.to_bits(),
                expected.worst_gap_mm.to_bits(),
                "{name}: worst_gap_mm"
            );
        }
    }

    /// FU-8e phase E3 — the row-level counters, at the boundary.
    ///
    /// `assess_coverage` calls a row uncovered when `nearest > radius`, so the
    /// delta must too: a row that lands exactly *on* the radius is covered, and
    /// moving onto or off that value is a flip.  The other half of the claim is
    /// the containment `flipped_rows <= moved_uncovered_rows`, which is what
    /// lets one counter be the trigger and the other be a report.
    #[test]
    fn fu8e_coverage_delta_counts_rows_the_way_assess_coverage_reads_them() {
        let radius = 1.0_f64;
        let just_over = f64::from_bits(radius.to_bits() + 1);
        let mut delta = ProposalCoverageDelta::default();
        // Exactly the radius is covered, on both sides: no flip, no move.
        delta.observe(radius, radius, radius);
        assert_eq!(delta, ProposalCoverageDelta::default());
        // One ULP over is uncovered, so this is a flip — and a move.
        delta.observe(radius, just_over, radius);
        assert_eq!(delta.flipped_rows, 1);
        assert_eq!(delta.moved_uncovered_rows, 1);
        // Uncovered on both sides, different values: a move, not a flip.
        delta.observe(3.0, 2.0, radius);
        assert_eq!(delta.flipped_rows, 1);
        assert_eq!(delta.moved_uncovered_rows, 2);
        // Covered on both sides, different values: neither.
        delta.observe(0.25, 0.5, radius);
        assert_eq!(delta.flipped_rows, 1);
        assert_eq!(delta.moved_uncovered_rows, 2);
        // An unreachable row never reached: infinity is uncovered and equal to
        // itself, so it neither flips nor moves.
        delta.observe(f64::INFINITY, f64::INFINITY, radius);
        assert_eq!(delta.moved_uncovered_rows, 2);
        assert!(delta.requires_reassessment());
        assert!(delta.flipped_rows <= delta.moved_uncovered_rows);
    }

    /// FU-8d phase F3, part (3) — **the planning pass runs once per proposal,
    /// not four times.**
    ///
    /// A removed repetition leaves no trace in any result: all four passes
    /// computed the same value of the same pure function, so nothing but a call
    /// count can observe the difference.  This reads the count directly.
    ///
    /// It also pins that the retained plans *are* the plans — each one equals
    /// what a fresh pass would produce for the same proposal — because "read
    /// the plan instead of recomputing it" is only sound if the two agree.  The
    /// re-derivation below is the last remaining place in the crate that plans
    /// a delta outside `plan_gate_nearest_batch`, and it is test-only.
    #[test]
    fn fu8d_a_gate_batch_plans_each_proposal_delta_exactly_once() {
        let (contacts, samples, demands, proposals, layers) = fu8d_delta_fixture();
        let index =
            GateBatchIndex::build(&contacts, &samples, &demands, &layers, 1.0).expect("index");
        assert!(
            proposals.len() > 1,
            "a one-proposal fixture cannot tell 1 pass per proposal from 1 per batch"
        );

        let before = PROPOSAL_DELTA_PLANS.with(std::cell::Cell::get);
        let planned = plan_gate_nearest_batch(
            &contacts,
            &proposals,
            &samples,
            demands.len(),
            &index,
            TEST_KNN_LIMITS,
            usize::MAX,
        )
        .expect("planned batch");
        let spent = PROPOSAL_DELTA_PLANS.with(std::cell::Cell::get) - before;
        assert_eq!(
            spent,
            proposals.len(),
            "planning the batch cost {spent} delta passes for {} proposals",
            proposals.len()
        );
        assert_eq!(planned.proposals.len(), proposals.len());

        for (proposal, retained) in proposals.iter().zip(&planned.proposals) {
            let fresh =
                plan_proposal_nearest_delta(&contacts, proposal, &index, TEST_KNN_LIMITS).unwrap();
            assert_eq!(retained.touched_rows, fresh.touched_rows);
            assert_eq!(retained.key_visits, fresh.key_visits);
            assert_eq!(retained.work, fresh.work);
            assert_eq!(retained.deltas.len(), fresh.deltas.len());
            for (left, right) in retained.deltas.iter().zip(&fresh.deltas) {
                assert_eq!(left.route, right.route);
                assert_eq!(left.source_count, right.source_count);
                assert_eq!(left.query_count, right.query_count);
            }
        }

        // The retention is charged, and the charge is the vector's own shape.
        let expected_bytes = proposals.len() * std::mem::size_of::<ProposalNearestPlan>()
            + planned
                .proposals
                .iter()
                .map(|plan| plan.deltas.len() * std::mem::size_of::<ProposalRouteDelta>())
                .sum::<usize>();
        assert_eq!(planned.retained_plan_bytes, expected_bytes);
        assert!(planned.retained_plan_bytes > 0);
    }

    /// FU-8d phase F3, part P2 — **a merged seat a proposal offers is always
    /// specialized, which is why the delta's locality argument holds.**
    ///
    /// The delta assumes the merged seat serves exactly
    /// `elig(left) ∪ elig(right)`.  That is true when the merged parts resolve
    /// to a *specialized* owner and false when they resolve to a generic one —
    /// a generic seat serves one island's base rows instead, which need not lie
    /// inside the union and would strand every row only the pair served.
    /// `GateLoadState::merged_seat_is_specialized` guards it and
    /// `variant_load_violation_count_in_full` handles the rejected case.
    ///
    /// This pins that the guard is a guard and not a live branch:
    /// `gather_pair_proposals` only offers a pair when **both** members carry
    /// demands, and `generic_owner_for_parts` returns `None` for any parts that
    /// carry a demand — so for every proposal the fixture produces, the merged
    /// parts and each member alone resolve specialized.  The second half shows
    /// the guard is not vacuous either: a demand-free pair on one shared route
    /// *does* resolve generic, which is exactly the shape it rejects.
    #[test]
    fn fu8d_a_proposals_merged_seat_always_resolves_specialized() {
        let generic_islands = [SupportIsland {
            id: 3,
            face_ids: vec![0, 1],
        }];
        let route = RegularContactRoute {
            candidate_id: 41,
            owner_island_id: 3,
            coverage_face_id: 0,
            component_id: 0,
            source_kind: RepairRowSourceKind::Generic,
        };

        // Half one: a pair whose members both carry demands — which is the only
        // kind `gather_pair_proposals` offers — resolves specialized, as merged
        // parts and as each member alone.
        let demanding = WorkingContact::specialized(minimum_demand(0, 3, 7, [1.0, 2.0, 0.1]));
        let mut work = 0usize;
        for parts in [
            ContactParts::new(&demanding, Some(&demanding)),
            ContactParts::new(&demanding, None),
        ] {
            assert!(parts.has_demands());
            assert_eq!(
                generic_owner_for_parts(parts, &generic_islands, &mut work, usize::MAX),
                Ok(None),
                "a part carrying a demand can never resolve to a generic owner"
            );
        }

        // Half two: `gather_pair_proposals` refuses to offer a pair either of
        // whose members is demand-free, so the demand-carrying precondition
        // above is the *only* shape a real proposal has.
        let regular = test_regular_contact([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], route, Vec::new());
        let contacts = vec![regular.clone(), demanding.clone()];
        let layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let limits =
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work());
        let mut descriptor_operations = 0usize;
        let mut proposals = Vec::new();
        gather_pair_proposals(
            &contacts,
            0,
            1,
            &layers,
            1.0,
            1.0,
            limits,
            &mut descriptor_operations,
            &mut proposals,
        )
        .expect("gathering a demand-free pair is not an error");
        assert!(
            proposals.is_empty(),
            "a pair with a demand-free member must not be offered at all"
        );

        // And the guard is not vacuous: the shape it rejects is real.  Two
        // demand-free contacts on one shared route whose island is generic do
        // resolve to a generic owner, and a merged seat built from them would
        // serve that island's base rows rather than `elig(left) ∪ elig(right)`.
        let mut work = 0usize;
        assert_eq!(
            generic_owner_for_parts(
                ContactParts::new(&regular, Some(&regular)),
                &generic_islands,
                &mut work,
                usize::MAX,
            ),
            Ok(Some(3)),
            "the guard's rejected case must be reachable, or the guard is decoration"
        );
    }

    /// FU-8d phase F3, part P5 — **the variant totals are derived, not
    /// rescanned, and the derivation is exact.**
    ///
    /// [`GateVariantTotals::for_variant`] claims a proposal variant's logical
    /// contact work and physical seat count are the baseline's minus one, with
    /// no dependence on *which* pair merged.  That is an arithmetic identity
    /// (see the type's docs), and this is its check: the rescan it replaces —
    /// skip both members, then add one merged seat carrying the union of their
    /// route and demand slots — is written out below and compared against the
    /// derivation for **every** proposal of the fixture.
    ///
    /// A fixture whose contacts all carried the same route and demand counts
    /// could not tell the identity from a coincidence, so the assertion below
    /// refuses to pass unless the members' own slot counts actually vary.
    #[test]
    fn fu8d_variant_totals_are_derived_not_rescanned() {
        let (contacts, _samples, demands, proposals, _layers) = fu8d_delta_fixture();
        let totals = GateVariantTotals::build(&contacts, &demands).expect("totals");

        assert_eq!(
            totals.for_variant(false),
            Ok((totals.logical_contact_work, contacts.len()))
        );

        let mut member_shapes = std::collections::BTreeSet::new();
        for proposal in &proposals {
            // The rescan `variant_operation_count` performed per variant before
            // FU-8d phase F3, verbatim in effect.
            let mut logical = 0usize;
            let mut physical = 0usize;
            for (index, contact) in contacts.iter().enumerate() {
                if index == proposal.left || index == proposal.right {
                    continue;
                }
                logical += 1 + contact.regular_routes.len() + contact.demands.len();
                physical += 1;
            }
            let left = &contacts[proposal.left];
            let right = &contacts[proposal.right];
            logical += 1
                + left.regular_routes.len()
                + right.regular_routes.len()
                + left.demands.len()
                + right.demands.len();
            physical += 1;
            member_shapes.insert((
                left.regular_routes.len() + left.demands.len(),
                right.regular_routes.len() + right.demands.len(),
            ));

            assert_eq!(
                totals.for_variant(true),
                Ok((logical, physical)),
                "derived totals disagree with the rescan for proposal {}/{}",
                proposal.left,
                proposal.right
            );
        }
        assert!(
            member_shapes.len() > 1,
            "every proposal merged identically shaped members, so the \
             cancellation is untested"
        );
    }

    /// FU-8d phase F3, part (3) — **the planning pass is bounded before it
    /// runs.**
    ///
    /// Part (3)'s complaint is not only repetition: the whole cost of deciding
    /// that a batch may not proceed was paid *before* the check that refuses
    /// it, and none of it was charged.  This pins the pre-check that closes
    /// that hole: with a limit one under the batch's own `key_visits` total,
    /// the batch is refused with the gate's `GateOperationLimit` and **no delta
    /// is planned at all** — the call counter does not move.
    #[test]
    fn fu8d_the_planning_precheck_refuses_before_any_delta_is_planned() {
        let (contacts, samples, demands, proposals, layers) = fu8d_delta_fixture();
        let index =
            GateBatchIndex::build(&contacts, &samples, &demands, &layers, 1.0).expect("index");
        let admitted = plan_gate_nearest_batch(
            &contacts,
            &proposals,
            &samples,
            demands.len(),
            &index,
            TEST_KNN_LIMITS,
            usize::MAX,
        )
        .expect("planned batch");
        let key_visits = admitted.batch.proposal_work.key_visits;
        assert!(key_visits > 0);

        // The pre-check is stated over the header alone and agrees exactly with
        // the term the batch is charged.
        assert_eq!(
            proposal_planning_precheck(&contacts, &proposals, usize::MAX),
            Ok(key_visits)
        );

        let before = PROPOSAL_DELTA_PLANS.with(std::cell::Cell::get);
        assert_eq!(
            plan_gate_nearest_batch(
                &contacts,
                &proposals,
                &samples,
                demands.len(),
                &index,
                TEST_KNN_LIMITS,
                key_visits - 1,
            )
            .err(),
            Some(SpecializedError::GateOperationLimit {
                required: key_visits,
                limit: key_visits - 1,
            })
        );
        assert_eq!(
            PROPOSAL_DELTA_PLANS.with(std::cell::Cell::get),
            before,
            "the refusal planned a delta, so the cost was paid before the check"
        );

        // Exactly at the bound the batch is admitted, so the check is a
        // boundary and not a blanket refusal.
        assert!(plan_gate_nearest_batch(
            &contacts,
            &proposals,
            &samples,
            demands.len(),
            &index,
            TEST_KNN_LIMITS,
            key_visits,
        )
        .is_ok());
    }

    /// FU-8d part (1), the retention lemma.  **Three retained witnesses answer
    /// any two-contact removal exactly, as a value.**
    ///
    /// The delta path FU-8d builds re-answers a merge proposal's nearest by
    /// taking, per touched sample, the first retained witness whose source is
    /// outside the merged pair, and comparing that against one distance to the
    /// merged seat.  This test pins the removal half of that claim, and pins it
    /// against the kernel rather than against hand-computed geometry: the
    /// reference is a fresh `PointCloudIndex` over exactly the surviving
    /// sources, queried with the same `SquaredEuclidean` the retention itself
    /// recorded, and the two are compared by `f64::to_bits`.
    ///
    /// It also pins the slot→contact mapping that path depends on:
    /// `source_slot` indexes [`GateBatchIndex::sources_for`]'s ascending list,
    /// because the baseline gather walks contacts in ascending index and
    /// deduplicates by `physical_id`, which is the contact index for variant 0.
    ///
    /// The FU-8c fixture is densified on one route key first, because its
    /// widest key carries exactly three servers — where the retained set *is*
    /// the whole set and the lemma is trivially true.  Truncation has to bite
    /// for this to prove anything, and the assertions below refuse to pass
    /// vacuously: a route wider than [`NEAREST_WITNESS_SLOTS`], a case answered
    /// from the last retained slot, and a case where the removal leaves no
    /// survivor at all must each occur.
    #[test]
    fn fu8d_three_retained_witnesses_answer_any_two_removal() {
        let (mut contacts, mut samples, demands, _proposals, layers) = fu8c_fixture();
        let up = [0.0, 0.0, 1.0];
        // Four more servers on island 1 / component 10, taking that key to
        // seven — comfortably past the three retained slots.
        for (candidate, face, x) in [
            (8u64, 8u32, 1.5f64),
            (9, 9, 3.0),
            (10, 10, 5.0),
            (11, 11, 6.0),
        ] {
            contacts.push(test_regular_contact(
                [x, 0.0, 0.0],
                up,
                RegularContactRoute {
                    candidate_id: candidate,
                    owner_island_id: 1,
                    coverage_face_id: face,
                    component_id: 10,
                    source_kind: RepairRowSourceKind::Generic,
                },
                Vec::new(),
            ));
        }
        for point in [[1.6, 0.1, 0.0], [3.1, 0.0, 0.0], [5.5, 0.2, 0.0]] {
            samples.push(CoverageSample {
                island_id: 1,
                face_id: 8,
                component_id: 10,
                point_mm: point,
                area_mm2: 1.0,
            });
        }

        let index =
            GateBatchIndex::build(&contacts, &samples, &demands, &layers, 1.0).expect("index");
        assert!(
            index
                .route_sources
                .iter()
                .any(|(_, sources)| sources.len() > NEAREST_WITNESS_SLOTS),
            "fixture must carry a route wider than the retained slot count"
        );

        let planned = plan_variant_nearest_routes(
            &contacts,
            None,
            &samples,
            TEST_KNN_LIMITS,
            NearestWitnessRetention::RetainWitnesses,
        )
        .expect("baseline plan");
        let mut nearest = vec![f64::INFINITY; samples.len()];
        let mut witnesses = NearestWitnessTable::seeded(samples.len());
        fill_variant_nearest(
            &contacts,
            None,
            &samples,
            &mut nearest,
            planned,
            Some(&mut witnesses),
        )
        .expect("baseline fill");

        let sample_route = |sample: &CoverageSample| CoverageRoute {
            island_id: sample.island_id,
            component_id: sample.component_id,
        };
        // The kernel's own answer over an explicit source set: the reference
        // this lemma is measured against, never a hand-rolled distance.
        let kernel_nearest = |point: [f64; 3], survivors: &[usize]| -> Option<(usize, u64)> {
            if survivors.is_empty() {
                return None;
            }
            let points = survivors
                .iter()
                .map(|&contact| contacts[contact].position_mm)
                .collect::<Vec<_>>();
            let plan = KnnBatchPlan::new(points.len(), 1, 1, TEST_KNN_LIMITS).expect("plan");
            let cloud = PointCloudIndex::new_for_knn_batch(&points, &plan).expect("index");
            let answers = cloud.knn_batch_planned(&[point], &plan).expect("query");
            let row = answers.row_range(0).expect("planned row");
            let slot = usize::try_from(answers.point_ids[row.start]).expect("slot fits usize");
            Some((
                survivors[slot],
                answers.squared_distances[row.start].to_bits(),
            ))
        };

        // Slot 0 is the batch's own answer, before any removal.
        for (sample_index, sample) in samples.iter().enumerate() {
            let sources = index.sources_for(sample_route(sample));
            let row = witnesses.rows[sample_index];
            let filled = row
                .iter()
                .filter(|witness| witness.source_slot != NO_WITNESS)
                .count();
            assert_eq!(
                filled,
                sources.len().min(NEAREST_WITNESS_SLOTS),
                "sample {sample_index} retained the wrong number of witnesses"
            );
            if filled == 0 {
                assert!(nearest[sample_index].is_infinite());
                continue;
            }
            assert_eq!(
                row[0].squared_distance.sqrt().to_bits(),
                nearest[sample_index].to_bits(),
                "sample {sample_index}: slot 0 is not the answer the sweep wrote"
            );
            assert_eq!(
                Some((
                    sources[row[0].source_slot],
                    row[0].squared_distance.to_bits()
                )),
                kernel_nearest(sample.point_mm, sources),
                "sample {sample_index}: slot 0 disagrees with the kernel over the full source set"
            );
        }

        let mut answered_from_last_slot = 0usize;
        let mut removed_every_server = 0usize;
        for left in 0..contacts.len() {
            for right in (left + 1)..contacts.len() {
                let pair = MergedPair { left, right };
                for (sample_index, sample) in samples.iter().enumerate() {
                    let sources = index.sources_for(sample_route(sample));
                    let survivors = sources
                        .iter()
                        .copied()
                        .filter(|&contact| !pair.contains(contact))
                        .collect::<Vec<_>>();
                    let from_witnesses = witnesses.rows[sample_index]
                        .iter()
                        .position(|witness| {
                            witness.source_slot != NO_WITNESS
                                && !pair.contains(sources[witness.source_slot])
                        })
                        .map(|slot| {
                            let witness = witnesses.rows[sample_index][slot];
                            (
                                slot,
                                sources[witness.source_slot],
                                witness.squared_distance.to_bits(),
                            )
                        });
                    assert_eq!(
                        from_witnesses.map(|(_, contact, distance)| (contact, distance)),
                        kernel_nearest(sample.point_mm, &survivors),
                        "removing ({left}, {right}) from sample {sample_index}: \
                         the retained witnesses disagree with the kernel over the survivors"
                    );
                    match from_witnesses {
                        Some((slot, _, _)) if slot + 1 == NEAREST_WITNESS_SLOTS => {
                            answered_from_last_slot += 1;
                        }
                        None if !sources.is_empty() => removed_every_server += 1,
                        _ => {}
                    }
                }
            }
        }
        assert!(
            answered_from_last_slot > 0,
            "no case was answered from the last retained slot, so truncation was never tested"
        );
        assert!(
            removed_every_server > 0,
            "no case removed a route's every server, so the no-survivor arm was never tested"
        );
    }

    /// FU-8c.  The witness-backed unsupported-demand count must agree with the
    /// predecessor's `demands × contacts` sweep on every variant, including
    /// variant 0.
    #[test]
    fn fu8c_witness_demand_counts_reproduce_the_full_sweep() {
        let (contacts, samples, demands, proposals, layers) = fu8c_fixture();
        let attach_reach = 1.0;
        let index =
            GateBatchIndex::build(&contacts, &samples, &demands, &layers, attach_reach).unwrap();

        let reference = |proposal: Option<&MergeProposal>| {
            demands
                .iter()
                .filter(|demand| {
                    !variant_satisfies_demand(&contacts, proposal, demand, &layers, attach_reach)
                })
                .count()
        };

        assert_eq!(
            variant_unsupported_count(&contacts, None, &demands, &index, &layers, attach_reach),
            reference(None),
        );
        // The fixture must actually contain both outcomes, or agreement is
        // vacuous.
        assert!(reference(None) > 0 && reference(None) < demands.len());
        for (proposal_index, proposal) in proposals.iter().enumerate() {
            assert_eq!(
                variant_unsupported_count(
                    &contacts,
                    Some(proposal),
                    &demands,
                    &index,
                    &layers,
                    attach_reach
                ),
                reference(Some(proposal)),
                "proposal {proposal_index}"
            );
        }
    }

    /// FU-8c.  The repriced plan must charge the batch's *own* shape: one
    /// baseline sweep plus the measured proposal deltas, not one sweep per
    /// variant.  Pinned on the Weeping Treant's recorded gate shape, which is
    /// the batch the predecessor refused.
    #[test]
    fn fu8c_repriced_plan_admits_the_treant_gate_shape_under_the_unchanged_cap() {
        const SAMPLES: usize = 405_146;
        const DEMANDS: usize = 1_095;
        const CONTACTS: usize = 1_366;
        const PROPOSALS: usize = 39_223;
        // The batch's 117,741 distinct island faces were a `FACES` constant
        // here until FU-8d phase F3 (part P2) removed the `variants x faces`
        // term they fed; `gate_resource_plan` no longer takes a face count.
        let limits =
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work());
        let cap = limits.gate_operation_limit(SAMPLES, CONTACTS);
        assert_eq!(cap, 35_445_500_736);

        // The predecessor's routed term, restated here so the comparison is on
        // the record rather than in prose:
        // `(405,146 + 1,095 + 1) x (39,223 x 1,365 + 1,366)` = 21,750,505,830,162.
        let old_routed_rows = (SAMPLES + DEMANDS + 1) * (PROPOSALS * (CONTACTS - 1) + CONTACTS);
        assert_eq!(old_routed_rows, 21_750_505_830_162);
        assert!(old_routed_rows > cap);

        // Re-pinned 2026-08-06 (FU-8d phase F3, part P2).  The struck
        // assertions were `plan.operations == (SAMPLES + DEMANDS + 1) *
        // CONTACTS + (PROPOSALS + 1) * FACES` and its evaluation
        // `554,926,572 + 4,618,272,984`.  The second term was the gate's own
        // model of a per-variant load finalize; it is gone, replaced by what
        // `specialized_load_resource_plan` states for the batch's **one**
        // baseline plus the count-derived per-proposal bound, which the caller
        // supplies.  `FACES` no longer enters this plan at all.  The load
        // figures below are stand-ins for that supplied pair, chosen as
        // distinct powers of ten so the assertion stays a derivation.
        //
        // The new routed term with *zero* measured proposal work is the
        // baseline sweep alone.
        const BASELINE_LOAD: usize = 5_402_535_619;
        const PROPOSAL_LOAD_BOUND: usize = 1_000_000;
        let plan = gate_resource_plan(
            CONTACTS,
            PROPOSALS,
            SAMPLES,
            DEMANDS,
            GateProposalRoutedWork::default(),
            GateLoadWork {
                baseline_operations: BASELINE_LOAD,
                proposal_bound: PROPOSAL_LOAD_BOUND,
            },
            limits,
        )
        .expect("the repriced plan admits the treant's gate shape");
        assert_eq!(plan.variants, PROPOSALS + 1);
        assert_eq!(
            plan.operations,
            (SAMPLES + DEMANDS + 1) * CONTACTS + BASELINE_LOAD + PROPOSAL_LOAD_BOUND
        );
        assert_eq!(plan.load_operations, BASELINE_LOAD + PROPOSAL_LOAD_BOUND);
        assert_eq!(plan.load_proposal_bound, PROPOSAL_LOAD_BOUND);
        assert!(plan.operations < cap);

        // And the per-proposal term is charged, not ignored: one more unit of
        // measured proposal work is one more operation.
        let charged = gate_resource_plan(
            CONTACTS,
            PROPOSALS,
            SAMPLES,
            DEMANDS,
            // FU-8d F2 added `key_visits`; the distinct powers of two keep the
            // assertion below a *derivation*, not a coincidence.
            GateProposalRoutedWork {
                nearest_work: 1,
                touched_rows: 2,
                key_visits: 8,
                demand_rows: 4,
            },
            GateLoadWork {
                baseline_operations: BASELINE_LOAD,
                proposal_bound: PROPOSAL_LOAD_BOUND,
            },
            limits,
        )
        .expect("plan");
        assert_eq!(charged.operations, plan.operations + 15);
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
        let planned = plan_variant_nearest_routes(
            &contacts,
            None,
            &samples,
            TEST_KNN_LIMITS,
            NearestWitnessRetention::RetainWitnesses,
        )
        .unwrap();
        assert_eq!(
            planned.measurement_counts(),
            (3, 2, 2, 1, 2, 2),
            "duplicate logical rows remain memberships while each route index deduplicates physical seats"
        );
        fill_variant_nearest(&contacts, None, &samples, &mut nearest, planned, None).unwrap();
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

        let planned = plan_route_points_nearest(
            &points,
            &samples,
            TEST_KNN_LIMITS,
            NearestWitnessRetention::RetainWitnesses,
        )
        .expect("duplicate logical rows are one indexed seat");
        assert_eq!(planned.measurement_counts(), (2, 2, 1, 1, 1, 1));
        let mut nearest = vec![f64::INFINITY; samples.len()];
        fill_route_points_nearest(&points, &samples, &mut nearest, planned, None)
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
            plan_variant_nearest_routes(
                &contacts,
                None,
                &samples,
                refused,
                NearestWitnessRetention::RetainWitnesses,
            ),
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
        let planned = plan_variant_nearest_routes(
            &contacts,
            None,
            &samples,
            TEST_KNN_LIMITS,
            NearestWitnessRetention::RetainWitnesses,
        )
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
            load_baseline_bytes: 0,
            // Re-pinned 2026-08-06 by record fix F6. Zero: this test pins that
            // serial route batches are charged as one maximum rather than a
            // sum, so every term but `coverage_nearest_bytes` must stay 0 for
            // the assertion to mean what it says. Its two pinned values are
            // unchanged by F6.
            face_scratch_bytes: 0,
            // Zero for the same reason, added 2026-08-06 (FU-8c).
            batch_index_bytes: 0,
            nearest_bytes: 0,
            // Zero for the same reason, added 2026-08-06 (FU-8d).
            nearest_witness_bytes: 0,
            // Zero for the same reason, added 2026-08-06 (FU-8d phase F3).
            proposal_plan_bytes: 0,
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
        let mut limits =
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work());
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
        let mut limits =
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work());
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
        let mut limits =
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work());
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
        let mut operation_limits =
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work());
        // Re-pinned 2026-08-06 (FU-8b batch-product caps): the gate operation cap
        // is now max(floor, K_G x batch_product), so a zero *floor* is only a zero
        // *cap* when the coefficient is zero too.  This test is about the zero-cap
        // refusal path, so it disables the coefficient and pins exactly what it
        // pinned before: floor 0 refuses a non-empty batch without allocating.
        operation_limits.gate_work_per_batch_unit = 0;
        operation_limits.max_gate_operations_floor = 0;
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

        let mut peak_limits =
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work());
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
        let mut scratch = Vec::new();
        // FU-8b fix A re-pin (2026-08-06): `required` was 11, now 10_001.
        // The old selection sort charged one unit per inner comparison, so it
        // tripped the cap eleven comparisons into its first pass (one demand
        // service test plus ten face comparisons).  The batch gather charges
        // the island's whole face count in one add *before* copying any id, so
        // the same refusal now states the real demand (1 service test + 10_000
        // faces) instead of the point at which a quadratic scan happened to
        // cross the line.  The refusal is still allocation-free: `scratch` is
        // untouched, which the assertion below pins.
        assert_eq!(
            variant_specialized_owner_face_count(
                &contacts,
                None,
                &[],
                &islands,
                &mut scratch,
                &mut work,
                10,
            ),
            Err(SpecializedError::GateOperationLimit {
                required: 10_001,
                limit: 10,
            })
        );
        assert!(scratch.is_empty());
    }

    /// The pre-FU-8b repeated-minimum selection sort, kept as a test-only
    /// oracle.  Fix A is allowed to change what the work meter charges; it is
    /// not allowed to change what the gatherers emit, so the fixtures below are
    /// checked against the algorithm that was replaced rather than against
    /// hand-written expectations.
    fn selection_sort_distinct_faces(islands: &[SupportIsland], served: &[bool]) -> Vec<u32> {
        let mut emitted = Vec::new();
        let mut last: Option<u32> = None;
        loop {
            let mut next: Option<u32> = None;
            for (island, serves) in islands.iter().zip(served) {
                if !serves {
                    continue;
                }
                for &face_id in &island.face_ids {
                    if last.is_none_or(|last| face_id > last)
                        && next.is_none_or(|next| face_id < next)
                    {
                        next = Some(face_id);
                    }
                }
            }
            let Some(face_id) = next else {
                break;
            };
            emitted.push(face_id);
            last = Some(face_id);
        }
        emitted
    }

    fn contact_routed_to(owners: &[u32]) -> WorkingContact {
        let route = |owner: u32| RegularContactRoute {
            candidate_id: u64::from(owner),
            owner_island_id: owner,
            coverage_face_id: 0,
            component_id: 0,
            source_kind: RepairRowSourceKind::Generic,
        };
        let mut contact = test_regular_contact(
            [0.0, 0.0, 0.1],
            [0.0, 0.0, -1.0],
            route(owners[0]),
            Vec::new(),
        );
        contact.regular_routes = owners.iter().copied().map(route).collect();
        contact
    }

    /// `(name, islands, island ids the probe contact routes to)`.
    fn distinct_face_fixtures() -> Vec<(&'static str, Vec<SupportIsland>, Vec<u32>)> {
        vec![
            ("no islands at all", Vec::new(), vec![0]),
            (
                "one island, one face",
                vec![SupportIsland {
                    id: 0,
                    face_ids: vec![7],
                }],
                vec![0],
            ),
            (
                "duplicates inside one island",
                vec![SupportIsland {
                    id: 0,
                    face_ids: vec![5, 5, 5, 2, 2, 9],
                }],
                vec![0],
            ),
            (
                "unsorted interleaved island ranges",
                vec![
                    SupportIsland {
                        id: 0,
                        face_ids: vec![9, 3, 7],
                    },
                    SupportIsland {
                        id: 1,
                        face_ids: vec![8, 1, 4],
                    },
                ],
                vec![0, 1],
            ),
            (
                "the same face shared by two islands",
                vec![
                    SupportIsland {
                        id: 0,
                        face_ids: vec![4, 2],
                    },
                    SupportIsland {
                        id: 1,
                        face_ids: vec![2, 4, 6],
                    },
                ],
                vec![0, 1],
            ),
            (
                "an empty island among served islands",
                vec![
                    SupportIsland {
                        id: 0,
                        face_ids: Vec::new(),
                    },
                    SupportIsland {
                        id: 1,
                        face_ids: vec![3, 1],
                    },
                ],
                vec![0, 1],
            ),
            (
                "an unserved island must contribute nothing",
                vec![
                    SupportIsland {
                        id: 0,
                        face_ids: vec![1, 2],
                    },
                    SupportIsland {
                        id: 1,
                        face_ids: vec![100, 101],
                    },
                ],
                vec![0],
            ),
            (
                "a fully descending run",
                vec![SupportIsland {
                    id: 0,
                    face_ids: (0..64u32).rev().collect(),
                }],
                vec![0],
            ),
            (
                "a wide island that is mostly duplicates",
                vec![SupportIsland {
                    id: 0,
                    face_ids: (0..1_000u32).map(|index| index % 250).collect(),
                }],
                vec![0],
            ),
        ]
    }

    #[test]
    fn batched_distinct_face_gathers_emit_exactly_what_the_selection_sort_emitted() {
        // One buffer across every fixture, so buffer reuse between unrelated
        // gathers is exercised too: a stale id would show up as a mismatch.
        let mut scratch = Vec::new();
        for (name, islands, routed) in distinct_face_fixtures() {
            let all_served = vec![true; islands.len()];
            let expected_all = selection_sort_distinct_faces(&islands, &all_served);
            let mut work = 0usize;
            let count =
                unique_island_face_count(&islands, &mut scratch, &mut work, usize::MAX).unwrap();
            assert_eq!(count, expected_all.len(), "{name}: distinct face count");
            assert_eq!(
                scratch, expected_all,
                "{name}: buffer left in ascending distinct order"
            );

            let contact = contact_routed_to(&routed);
            let parts = ContactParts::new(&contact, None);
            let served = islands
                .iter()
                .map(|island| routed.contains(&island.id))
                .collect::<Vec<_>>();
            let expected_served = selection_sort_distinct_faces(&islands, &served);
            let mut emitted = Vec::new();
            let mut served_work = 0usize;
            visit_served_owner_faces(
                parts,
                &islands,
                &mut scratch,
                &mut served_work,
                usize::MAX,
                |face_id| {
                    emitted.push(face_id);
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(emitted, expected_served, "{name}: served face visit order");
        }
    }

    #[test]
    fn the_batched_distinct_face_charge_is_linearithmic_where_the_sort_was_quadratic() {
        // 1_024 distinct ids in descending order: the shape that made the
        // Weeping Treant refuse at limit+1 (FU-8b defect A).
        let islands = [SupportIsland {
            id: 0,
            face_ids: (0..1_024u32).rev().collect(),
        }];
        let mut scratch = Vec::new();
        let mut work = 0usize;
        let count =
            unique_island_face_count(&islands, &mut scratch, &mut work, usize::MAX).unwrap();
        assert_eq!(count, 1_024);
        // Gather charges 1_024 inspections; the batch dedup charges
        // 1_024 * (ceil(log2(1_025)) + 1) = 1_024 * (11 + 1) = 12_288.
        assert_eq!(work, 1_024 + 12_288);
        // The selection sort charged one unit per inner comparison across
        // `distinct + 1` passes over every id.
        const SELECTION_SORT_CHARGE: usize = 1_024 * (1_024 + 1);
        assert!(work * 78 < SELECTION_SORT_CHARGE);
    }

    #[test]
    fn the_batched_distinct_face_charge_accepts_the_cap_and_refuses_cap_minus_one() {
        let islands = [SupportIsland {
            id: 0,
            face_ids: vec![4, 2, 2, 9],
        }];
        // Gather 4, then dedup 4 * (ceil(log2(5)) + 1) = 4 * (3 + 1) = 16.
        const EXACT: usize = 4 + 16;
        let mut scratch = Vec::new();
        let mut work = 0usize;
        assert_eq!(
            unique_island_face_count(&islands, &mut scratch, &mut work, EXACT),
            Ok(3)
        );
        assert_eq!(work, EXACT);
        let mut work = 0usize;
        assert_eq!(
            unique_island_face_count(&islands, &mut scratch, &mut work, EXACT - 1),
            Err(SpecializedError::GateOperationLimit {
                required: EXACT,
                limit: EXACT - 1,
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
        let mut scratch = Vec::new();
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
                None,
                &mut scratch,
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
            None,
            &mut scratch,
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
                None,
                &mut scratch,
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
            load_baseline_bytes: 30,
            // Re-pinned 2026-08-06 by record fix F6, which added this term to
            // the retained total. Held at 0 so the boundary this test pins —
            // exactly at the cap, and one byte over — stays the same pair of
            // numbers it pinned before F6 (100 / 101). The term's own effect on
            // the boundary is pinned separately by
            // `the_shared_face_buffer_is_charged_against_the_gate_peak`.
            face_scratch_bytes: 0,
            // Zero for the same reason, added 2026-08-06 (FU-8c).
            batch_index_bytes: 0,
            nearest_bytes: 0,
            // Zero for the same reason, added 2026-08-06 (FU-8d).
            nearest_witness_bytes: 0,
            // Zero for the same reason, added 2026-08-06 (FU-8d phase F3).
            proposal_plan_bytes: 0,
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

    /// Record fix F6 (2026-08-06). FU-8b fix (A) allocates one `Vec<u32>` per
    /// gate batch and threads it through every distinct-face gather, but never
    /// made it a term in the gate's byte bound: it was memory the gate held and
    /// did not charge for. These pin that it is charged, that it is charged as
    /// *retained* rather than phase-local, and that the number charged is the
    /// islands' own `face_ids`.
    #[test]
    fn the_shared_face_buffer_is_charged_against_the_gate_peak() {
        // Two islands, 3 + 5 face ids: 8 entries x 4 bytes x 2 for growth = 64.
        // Duplicate ids across islands still count, because the gather copies
        // before it dedups — the buffer really does hold both.
        let islands = [
            SupportIsland {
                id: 1,
                face_ids: vec![7, 8, 9],
            },
            SupportIsland {
                id: 2,
                face_ids: vec![7, 8, 9, 4, 5],
            },
        ];
        assert_eq!(island_face_scratch_bytes(&islands), Ok(64));
        assert_eq!(island_face_scratch_bytes(&[]), Ok(0));

        let base = GatePeakUsage {
            proposal_bytes: 10,
            result_bytes: 20,
            load_baseline_bytes: 30,
            face_scratch_bytes: 0,
            // Zero for the same reason, added 2026-08-06 (FU-8c).
            batch_index_bytes: 0,
            nearest_bytes: 0,
            // Zero for the same reason, added 2026-08-06 (FU-8d).
            nearest_witness_bytes: 0,
            // Zero for the same reason, added 2026-08-06 (FU-8d phase F3).
            proposal_plan_bytes: 0,
            coverage_scratch_bytes: 40,
            coverage_nearest_bytes: 0,
            coverage_retained_bytes: 8,
            load_scratch_bytes: 32,
        };
        // Without the term, the batch fits a 100-byte cap exactly.
        assert_eq!(check_gate_peak(base, 100), Ok(100));
        let charged = GatePeakUsage {
            face_scratch_bytes: island_face_scratch_bytes(&islands).expect("scratch bytes"),
            ..base
        };
        // With it, the requirement rises by exactly the buffer's bound, and the
        // same cap now refuses. The term is genuinely part of the bound, not
        // decoration.
        assert_eq!(
            check_gate_peak(charged, 100),
            Err(SpecializedError::GatePeakByteLimit {
                required: 164,
                limit: 100,
            })
        );
        assert_eq!(check_gate_peak(charged, 164), Ok(164));

        // Retained, not phase-local. With every phase term zeroed the whole
        // requirement is the retained set, and it still carries the 64 — the
        // buffer is charged whichever phase turns out to be the widest.
        let retained_only = GatePeakUsage {
            coverage_scratch_bytes: 0,
            coverage_nearest_bytes: 0,
            coverage_retained_bytes: 0,
            load_scratch_bytes: 0,
            ..charged
        };
        assert_eq!(check_gate_peak(retained_only, usize::MAX), Ok(124));
        // 10 + 20 + 30 = 60 without it, so the whole difference is the buffer.
        assert_eq!(
            check_gate_peak(
                GatePeakUsage {
                    face_scratch_bytes: 0,
                    ..retained_only
                },
                usize::MAX
            ),
            Ok(60)
        );
    }

    /// Record fix F9 (2026-08-06). FU-8b fix (A) left a *third* shared
    /// distinct-face buffer uncharged, in `working_load_inputs` — the same
    /// omission F6 fixed on the gate side, on the path taken after the gate
    /// accepts. These pin that the accepted-load byte bound now carries it, and
    /// that it is the islands' own `face_ids` that price it.
    #[test]
    fn the_shared_face_buffer_is_charged_against_the_load_scratch_bound() {
        // The same two islands the gate test uses: 3 + 5 face ids, 8 entries x
        // 4 bytes x 2 for growth = 64. Both paths price the buffer from the
        // same `islands` slice through `island_face_scratch_bytes`, so the
        // number is the same number.
        let islands = [
            SupportIsland {
                id: 1,
                face_ids: vec![7, 8, 9],
            },
            SupportIsland {
                id: 2,
                face_ids: vec![7, 8, 9, 4, 5],
            },
        ];

        let base = LoadScratchUsage {
            retained_bytes: 60,
            finalizer_scratch_bytes: 40,
            face_scratch_bytes: 0,
        };
        // Pre-F9 arithmetic: retained + finalizer only, fitting a 100-byte cap
        // exactly. Held at 0 so this pins the same pair of numbers the inline
        // check pinned before F9 (100 accepts, 101 would not).
        assert_eq!(check_load_scratch(base, 100), Ok(100));
        assert_eq!(
            check_load_scratch(
                LoadScratchUsage {
                    retained_bytes: 61,
                    ..base
                },
                100
            ),
            Err(GenericPerceptionError::LoadScratchLimit {
                required: 101,
                limit: 100,
            }
            .into())
        );

        // With the term, the requirement rises by exactly the buffer's bound
        // and the same cap refuses. The term is part of the bound, not
        // decoration.
        let charged = LoadScratchUsage {
            face_scratch_bytes: island_face_scratch_bytes(&islands).expect("scratch bytes"),
            ..base
        };
        assert_eq!(
            check_load_scratch(charged, 100),
            Err(GenericPerceptionError::LoadScratchLimit {
                required: 164,
                limit: 100,
            }
            .into())
        );
        assert_eq!(check_load_scratch(charged, 164), Ok(164));
        // No islands, no charge: the term is the islands' `face_ids` and
        // nothing else.
        assert_eq!(
            check_load_scratch(
                LoadScratchUsage {
                    face_scratch_bytes: island_face_scratch_bytes(&[]).expect("scratch bytes"),
                    ..base
                },
                100
            ),
            Ok(100)
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
        let mut scratch = Vec::new();
        let owner_faces = variant_specialized_owner_face_count(
            &contacts,
            None,
            &[],
            &islands,
            &mut scratch,
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
            None,
            &mut scratch,
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

        let limits =
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work());
        // Re-pinned 2026-08-06 (FU-8c).  The routed term stopped being derivable
        // from counts alone when the gate stopped re-sweeping every contact
        // against every coverage sample once per proposal, so this fixture now
        // states the per-proposal work its caller measures.  The values are
        // chosen so the pinned total is *unchanged*: old `contact_visits =
        // 2 x 2 + 3 = 7` and `routed_rows = 17 x 7 = 119`; new `baseline_rows =
        // 17 x 3 = 51` plus `40 + 20 + 8 = 68`, also 119, with `face_rows =
        // 3 x 7 = 21` either way.  `exact.operations` is therefore still 140 and
        // every cap-minus-one refusal below still pins the same boundary.
        //
        // Re-split 2026-08-06 (FU-8d phase F2), same total.  The measured
        // proposal work gained a `key_visits` term, so the 20 that was all
        // `touched_rows` is now `14 + 6`; `40 + 14 + 6 + 8 = 68` exactly as
        // before, and this test's boundary is deliberately untouched by F2.
        //
        // Re-pinned 2026-08-06 (FU-8d phase F3, part P2), same total.  The
        // `face_rows = variants x faces = 3 x 7 = 21` term is gone — the gate
        // no longer models a per-variant load finalize — and the caller now
        // supplies what the load planner itself states.  `21 + 0` keeps this
        // fixture's boundary at exactly 140, so every cap-minus-one refusal
        // below still pins the same numbers it always did.  The `faces`
        // argument is gone from the signature with the term it fed.
        const FIXTURE_PROPOSAL_WORK: GateProposalRoutedWork = GateProposalRoutedWork {
            nearest_work: 40,
            touched_rows: 14,
            key_visits: 6,
            demand_rows: 8,
        };
        const FIXTURE_LOAD_WORK: GateLoadWork = GateLoadWork {
            baseline_operations: 21,
            proposal_bound: 0,
        };
        let exact = gate_resource_plan(
            3,
            2,
            5,
            11,
            FIXTURE_PROPOSAL_WORK,
            FIXTURE_LOAD_WORK,
            limits,
        )
        .unwrap();
        assert_eq!(exact.variants, 3);
        assert_eq!(exact.operations, 140);

        let mut at_cap = limits;
        // Re-pinned 2026-08-06 (FU-8b batch-product caps): `exact.operations` is 140
        // on this fixture, far below K_G x (5 + 1) x (3 + 1) = 1,536, so with the
        // coefficient live the cap would never be the floor and the cap-minus-one
        // refusal could not be expressed.  The coefficient is disabled here so the
        // test keeps pinning what it was written to pin, the exact flat bound and
        // its one-unit-past refusal; the rule itself is pinned in
        // `tests/resource_cap_policy.rs`.
        at_cap.gate_work_per_batch_unit = 0;
        at_cap.max_gate_variants = exact.variants;
        at_cap.max_gate_vector_bytes = exact.vector_bytes;
        at_cap.max_gate_contact_matrix_bytes = exact.contact_window_bytes;
        at_cap.max_gate_operations_floor = exact.operations;
        assert_eq!(
            gate_resource_plan(
                3,
                2,
                5,
                11,
                FIXTURE_PROPOSAL_WORK,
                FIXTURE_LOAD_WORK,
                at_cap
            ),
            Ok(exact)
        );

        let mut below = at_cap;
        below.max_gate_variants -= 1;
        assert_eq!(
            gate_resource_plan(3, 2, 5, 11, FIXTURE_PROPOSAL_WORK, FIXTURE_LOAD_WORK, below),
            Err(SpecializedError::GateVariantLimit {
                required: exact.variants,
                limit: exact.variants - 1,
            })
        );
        below = at_cap;
        below.max_gate_vector_bytes -= 1;
        assert_eq!(
            gate_resource_plan(3, 2, 5, 11, FIXTURE_PROPOSAL_WORK, FIXTURE_LOAD_WORK, below),
            Err(SpecializedError::GateVectorByteLimit {
                required: exact.vector_bytes,
                limit: exact.vector_bytes - 1,
            })
        );
        below = at_cap;
        below.max_gate_contact_matrix_bytes -= 1;
        assert_eq!(
            gate_resource_plan(3, 2, 5, 11, FIXTURE_PROPOSAL_WORK, FIXTURE_LOAD_WORK, below),
            Err(SpecializedError::GateContactMatrixByteLimit {
                required: exact.contact_window_bytes,
                limit: exact.contact_window_bytes - 1,
            })
        );
        below = at_cap;
        below.max_gate_operations_floor -= 1;
        assert_eq!(
            gate_resource_plan(3, 2, 5, 11, FIXTURE_PROPOSAL_WORK, FIXTURE_LOAD_WORK, below),
            Err(SpecializedError::GateOperationLimit {
                required: exact.operations,
                limit: exact.operations - 1,
            })
        );
        below = at_cap;
        below.max_gate_window_variants = 0;
        assert_eq!(
            gate_resource_plan(3, 2, 5, 11, FIXTURE_PROPOSAL_WORK, FIXTURE_LOAD_WORK, below),
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
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work()),
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
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work()),
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
                SpecializedLimits::production(
                    TEST_KNN_LIMITS,
                    crate::specialized::test_batch_work()
                ),
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
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work()),
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
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work()),
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
        changed_limits.limits_authority.max_load_operations_floor += 1;
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
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work()),
        )
        .expect("required zero-child fixture can form a regular load input");
        regular_only.load_partition = finalize_specialized_load_partition(
            &generic,
            &mesh,
            &NativeSurface::from_indexed(mesh.verts(), mesh.tris(), edit_surface_config()).unwrap(),
            &inputs.contacts,
            &inputs.specialized_owner_faces,
            LoadFinalizeLimits {
                max_scratch_bytes: SpecializedLimits::production(
                    TEST_KNN_LIMITS,
                    crate::specialized::test_batch_work(),
                )
                .max_load_scratch_bytes,
                max_operations_floor: inputs.remaining_load_operations,
                operations_per_batch_unit: 0,
            },
            None,
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
                process_resolution_mm: 0.05,
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
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work()),
            &mut telemetry,
            0,
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
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work()),
            &mut telemetry,
            0,
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
                SpecializedLimits::production(
                    TEST_KNN_LIMITS,
                    crate::specialized::test_batch_work(),
                ),
                &mut telemetry,
                0,
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
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work()),
            &mut telemetry,
            0,
        )
        .expect("coalesce");
        assert_eq!(coalesced, 0);
        assert!(same_working_contacts(&contacts, &before));
        assert_eq!(telemetry.deep_clone_count, 0);
    }

    /// The cap check inside `coalesce_exact_regular_seats` must bound the
    /// *whole-consolidate* iteration count, not restart from zero: a caller
    /// that has already spent its budget in `attach_exact_mixed_contacts`
    /// and the gated tournament passes that spend in as `iterations`, and
    /// one more coalesce here must trip `max_merge_iterations` rather than
    /// get its own fresh allowance.
    #[test]
    fn terminal_coalescing_shares_the_whole_consolidate_iteration_budget() {
        let (elected_route, other_route) = coincident_collision_routes();
        let position = [3.0, -4.0, 5.5];
        let elected_normal = [0.0, 0.0, -1.0];
        let other_normal = [0.0, f64::from_bits(1), -1.0];
        let mut contacts = vec![
            test_regular_contact(position, elected_normal, elected_route, Vec::new()),
            test_regular_contact(position, other_normal, other_route, Vec::new()),
        ];
        let mut telemetry = MergeAllocationTelemetry::default();
        let mut limits =
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work());
        limits.max_merge_iterations = 1;

        // A caller that already spent the entire budget upstream (attach
        // prepass + tournament) leaves no room for even a single coalesce.
        assert_eq!(
            coalesce_exact_regular_seats(&mut contacts, limits, &mut telemetry, 1),
            Err(SpecializedError::MergeIterationLimit {
                required: 2,
                limit: 1,
            })
        );
        assert_eq!(contacts.len(), 2, "the refused merge must not commit");

        // The same pass starting from zero spend fits inside the cap.
        let coalesced = coalesce_exact_regular_seats(&mut contacts, limits, &mut telemetry, 0)
            .expect("one coalesce fits the whole-consolidate cap when nothing else spent it");
        assert_eq!(coalesced, 1);
        assert_eq!(contacts.len(), 1);
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
        let limits =
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work());
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
        changed.max_load_operations_floor += 1;
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
