//! The coverage-closure controller: FU-56.
//!
//! Production support planning used to return from `select_contacts_baseline`
//! one line before the legacy pre-routing repair loop
//! (`select_contacts_with_mode`, `lib.rs`), so `COVERAGE_REPAIR_MAX_ATTEMPTS`
//! had no production reader and the immutable [`crate::specialized::RepairPool`]
//! — built, lineage-validated, contact-validated and hashed — sat unused. This
//! module is the controller the pool was built for.
//!
//! **What closes, and what never moves.** Closure is achieved ONLY by adding
//! immutable repair-pool contacts to the plan, never by relaxing a gate: the
//! coverage radius, the patch-area floor and the zero uncovered-area tolerance
//! are read from the SAME registry the pre-write gate reads
//! ([`registered_coverage_request`], shared with
//! `plinth_pipeline::gates::registered_coverage_request`), and
//! [`coverage_patch_is_blocking`] mirrors `plinth_verify::defects_for_patches`'s
//! blocking predicate exactly rather than restating it — see
//! `plinth-verify/tests/coverage_gate.rs` for the drift alarm that keeps the
//! two definitions from diverging silently. The one geometric measurement is
//! made over ROUTED contacts, because that is what the gate itself measures;
//! a contact this controller adds but the CCO transcript does not admit
//! contributes zero coverage, which is exactly why closure iterates and
//! re-measures rather than trusting a single pre-routing pass.
//!
//! The controller loop itself lives in [`crate::runner::run_support`], because
//! it needs the sealed run's private typestate machinery
//! (`prepare_support_context`, `prepared_run_for_plan`,
//! `LoweredSupportRun::into_context`) that this module — a sibling, not a
//! descendant, of `runner` — cannot name. This module supplies the parts that
//! do not need that machinery: the typed refusal evidence
//! ([`CoverageClosureRefusal`]) FU-57 persists, the shared blocking predicate
//! and registered request, and the batch-first nomination algorithm
//! ([`nominate_repair_rows`]) that turns one attempt's blocking patches into a
//! deterministic, deduplicated set of repair-row ids to append.

use std::collections::{BTreeMap, BTreeSet};

use plinth_core::m0_constants;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::specialized::{RepairRow, RepairRowId, SpecializedContactPlan, SpecializedError};
use crate::{
    CoverageGap, CoverageMeasurementError, CoverageMeasurementRequest, SupportPreparationLimits,
};

/// Mirrors `plinth_verify::defects_for_patches`'s per-patch blocking predicate
/// exactly: a patch blocks emission iff its assessment failed, its cut was
/// ambiguous, or it has positive uncovered area. Kept as the ONE
/// implementation rather than restated independently in the verifier and the
/// controller — `plinth-verify/tests/coverage_gate.rs` proves the pairing
/// stays exact.
#[must_use]
pub const fn coverage_patch_is_blocking(
    uncovered_area_mm2: f64,
    cut_ambiguous: bool,
    assessment_failed: bool,
) -> bool {
    assessment_failed || cut_ambiguous || uncovered_area_mm2 > 0.0
}

/// Failures in assembling the registered coverage-closure/gate request.
#[derive(Clone, Debug, PartialEq, Error)]
pub enum CoverageClosureSetupError {
    #[error("registered M0 constants are unavailable: {0}")]
    RegisteredConstantsUnavailable(String),
    #[error("the production support resource policy is unavailable: {0}")]
    ResourcePolicyUnavailable(String),
    #[error("the registered coverage-closure request is invalid: {0}")]
    InvalidCoverageRequest(#[source] CoverageMeasurementError),
}

/// The one production implementation of the pre-write gate's own coverage
/// measurement request: radius = `COVERAGE_RADIUS_FACTOR × TARGET_SPACING_MM`,
/// patch-area floor = `VOXEL_PITCH_MM²`, ceilings from
/// [`SupportPreparationLimits::coverage_measurement_limits`].
/// `plinth_pipeline::gates::registered_coverage_request` delegates here so the
/// controller and the gate can never independently drift onto two different
/// radii — the controller closes to exactly what the gate will re-measure.
///
/// # Errors
///
/// Returns [`CoverageClosureSetupError::RegisteredConstantsUnavailable`] or
/// [`CoverageClosureSetupError::ResourcePolicyUnavailable`] if the registry
/// cannot be read, and [`CoverageClosureSetupError::InvalidCoverageRequest`]
/// if the registered values do not form a valid request.
pub fn registered_coverage_request(
) -> Result<CoverageMeasurementRequest, CoverageClosureSetupError> {
    let limits = SupportPreparationLimits::production().map_err(|error| {
        CoverageClosureSetupError::ResourcePolicyUnavailable(error.to_string())
    })?;
    let constants = m0_constants().map_err(|error| {
        CoverageClosureSetupError::RegisteredConstantsUnavailable(error.to_string())
    })?;
    let policy = constants.placeholders();
    let radius_mm = policy.coverage_radius_factor().diagnostic().get()
        * policy.target_spacing_mm().diagnostic().get();
    let patch_area_floor_mm2 = policy.voxel_pitch_mm().diagnostic().get().powi(2);
    let limits = limits
        .coverage_measurement_limits()
        .map_err(CoverageClosureSetupError::InvalidCoverageRequest)?;
    CoverageMeasurementRequest::from_f64(radius_mm, patch_area_floor_mm2, limits)
        .map_err(CoverageClosureSetupError::InvalidCoverageRequest)
}

/// Why closure stopped without admitting every measured island.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CoverageClosureReason {
    /// The registered attempt bound
    /// (`ContactSelectionConfig::coverage_repair_max_attempts`, registered
    /// value 8) was reached with blocking patches still measured.
    AttemptsExhausted,
    /// **No** blocking patch anywhere had an unoccupied, unfailed repair-pool
    /// row in its own `(island_id, component_id)`, so the attempt produced an
    /// empty nomination set and closure could not shrink anything at all.
    ///
    /// This is deliberately the whole-attempt condition, not a per-patch one:
    /// a model with one row-less island and one island that still has rows
    /// keeps iterating, because refusing the moment ANY island runs dry would
    /// abandon islands that were still closing. The per-island distinction
    /// the owner needs — "never had a row" versus "nominated every attempt
    /// and never closed" — is carried on
    /// [`CoverageClosureIsland::eligible_repair_rows`] and
    /// [`CoverageClosureIsland::nominated_repair_rows`] instead, on EVERY
    /// refusal reason including [`Self::AttemptsExhausted`].
    NoEligibleRepairRows,
    /// The routed-contact coverage measurement itself could not be completed.
    MeasurementRefused(String),
    /// A plan the controller had ALREADY extended with repair-pool contacts
    /// could not be re-planned: one of routing, rewire, brace, structure,
    /// lowering or the re-preparation refused the extended plan.
    ///
    /// **This is a coverage verdict, not a generic sealed-run failure.**
    /// Attempt 0 — the model's own plan, untouched by this controller —
    /// sealed all the way through lowering, so the geometry demonstrably
    /// seals; what failed is the controller's own extension of it. Reporting
    /// it as [`crate::SupportRunError::Failed`] would make a coverage refusal
    /// indistinguishable from a routing failure the model would have had
    /// anyway: it would lose the coverage evidence FU-57 requires, and it
    /// would let the oriented-solid reserve substitute a different geometry
    /// after a coverage verdict, which the terminal-refusal design forbids.
    RepairPlanRefused {
        /// The stage that refused the repair-extended plan.
        phase: crate::SupportRunPhase,
        /// That stage's own error, as it was reported.
        detail: String,
    },
}

/// A worst routed-contact gap, projected from [`CoverageGap`] for a refusal
/// that must outlive the borrowed verification snapshot it was measured
/// against.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CoverageClosureGap {
    Finite(f64),
    NoRoutedContact,
}

impl From<CoverageGap> for CoverageClosureGap {
    fn from(gap: CoverageGap) -> Self {
        match gap {
            CoverageGap::Finite(value) => Self::Finite(value.get()),
            CoverageGap::NoRoutedContact => Self::NoRoutedContact,
        }
    }
}

/// One measured island still uncovered when closure stopped, aggregated over
/// every blocking patch sharing its `(island_id, component_id)` key exactly
/// as `plinth_verify::defects_for_patches` aggregates by island: the area sum
/// is finite and additive, and the worst gap is the least favourable of the
/// group's (`NoRoutedContact` outranks any finite distance, matching the
/// gate's own reading that "no route reached this patch at all" is the worse
/// evidence).
///
/// This is evidence only. It is never read by `judge_pre_write_gates`, and
/// FU-57 is the row that persists it as the sidecar/card/render triple.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoverageClosureIsland {
    pub island_id: u32,
    pub component_id: u32,
    pub representative_face_id: u32,
    pub sorted_face_ids: Vec<u32>,
    pub uncovered_area_mm2: f64,
    pub worst_gap: CoverageClosureGap,
    pub patch_count: usize,
    /// How many repair-pool rows the controller nominated for this island
    /// over the whole run, summed across attempts.
    ///
    /// **Zero means this island was never even attempted** — it had no
    /// unoccupied, unfailed repair-pool row in its own
    /// `(island_id, component_id)` on any attempt that reached nomination.
    /// A positive count with the island still blocking means the opposite:
    /// rows WERE appended and the CCO transcript did not turn them into
    /// coverage. Those are different problems with different fixes (the pool
    /// or the mesh, versus the router), and before this field the refusal
    /// could not tell them apart.
    #[serde(default)]
    pub nominated_repair_rows: usize,
    /// How many repair-pool rows were still eligible for this island the last
    /// time nomination ran. Zero beside a positive
    /// [`Self::nominated_repair_rows`] means the pool for this island drained.
    #[serde(default)]
    pub eligible_repair_rows: usize,
}

/// One closure round's measured shape, kept for refusal evidence: how many
/// patches were still blocking when this attempt measured, and how many rows
/// this attempt nominated in response (zero on the terminal attempt, which
/// measures but does not nominate).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageClosureAttempt {
    pub attempt: usize,
    pub blocking_patch_count: usize,
    pub nominated_rows: usize,
}

/// The typed refusal FU-57 persists: closure could not admit every measured
/// island within the registered attempt bound. Boxed at its one call site
/// ([`crate::SupportRunError::CoverageClosure`]) because it carries the whole
/// per-attempt history and every still-uncovered island's evidence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoverageClosureRefusal {
    pub attempts: usize,
    pub reason: CoverageClosureReason,
    pub islands: Vec<CoverageClosureIsland>,
    pub history: Vec<CoverageClosureAttempt>,
}

/// One blocking patch's routing key and exact face membership, projected out
/// of a live `CoverageMeasurementPatchEvidence` so nomination can outlive the
/// borrowed verification snapshot the measurement was taken against. Built
/// only from a live measurement, never assembled as a planning input.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BlockingPatchKey {
    pub(crate) island_id: u32,
    pub(crate) component_id: u32,
    pub(crate) representative_face_id: u32,
    pub(crate) sorted_face_ids: Vec<u32>,
    pub(crate) uncovered_area_mm2_bits: u64,
    pub(crate) worst_gap: CoverageClosureGap,
}

/// Aggregate one attempt's blocking patches into the per-island evidence
/// [`CoverageClosureRefusal`] carries. One gathered pass, grouped by
/// `(island_id, component_id)`, with each island's nomination history stamped
/// on from `tally` in the same pass.
pub(crate) fn islands_from_blocking(
    patches: &[BlockingPatchKey],
    tally: &BTreeMap<(u32, u32), KeyNomination>,
) -> Vec<CoverageClosureIsland> {
    let mut grouped: BTreeMap<(u32, u32), CoverageClosureIsland> = BTreeMap::new();
    for patch in patches {
        let key = (patch.island_id, patch.component_id);
        let uncovered_area_mm2 = f64::from_bits(patch.uncovered_area_mm2_bits);
        grouped
            .entry(key)
            .and_modify(|island| {
                island.representative_face_id = island
                    .representative_face_id
                    .min(patch.representative_face_id);
                island.sorted_face_ids.extend(patch.sorted_face_ids.iter());
                island.sorted_face_ids.sort_unstable();
                island.sorted_face_ids.dedup();
                island.uncovered_area_mm2 += uncovered_area_mm2;
                island.worst_gap = worse_gap(island.worst_gap, patch.worst_gap);
                island.patch_count += 1;
            })
            .or_insert_with(|| CoverageClosureIsland {
                island_id: patch.island_id,
                component_id: patch.component_id,
                representative_face_id: patch.representative_face_id,
                sorted_face_ids: patch.sorted_face_ids.clone(),
                uncovered_area_mm2,
                worst_gap: patch.worst_gap,
                patch_count: 1,
                nominated_repair_rows: 0,
                eligible_repair_rows: 0,
            });
    }
    for (key, island) in &mut grouped {
        let counts = tally.get(key).copied().unwrap_or_default();
        island.nominated_repair_rows = counts.nominated_rows;
        island.eligible_repair_rows = counts.eligible_rows;
    }
    grouped.into_values().collect()
}

fn worse_gap(left: CoverageClosureGap, right: CoverageClosureGap) -> CoverageClosureGap {
    match (left, right) {
        (CoverageClosureGap::NoRoutedContact, _) | (_, CoverageClosureGap::NoRoutedContact) => {
            CoverageClosureGap::NoRoutedContact
        }
        (CoverageClosureGap::Finite(left), CoverageClosureGap::Finite(right)) => {
            CoverageClosureGap::Finite(left.max(right))
        }
    }
}

/// What nomination found for one blocking `(island_id, component_id)` key.
///
/// Kept per key rather than summed, because the two ways a key can fail to
/// close are the two different actions an owner would take (FU-57's card
/// exists to tell them apart): "this key never had a single unoccupied,
/// unfailed repair-pool row" says the pool or the mesh is wrong, while
/// "rows were nominated every attempt and the area never moved" says the
/// transcript is not admitting them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct KeyNomination {
    /// Rows still eligible for this key at the moment nomination ran.
    pub(crate) eligible_rows: usize,
    /// Rows this attempt actually nominated for this key.
    pub(crate) nominated_rows: usize,
}

/// One attempt's nomination outcome: the flat, deduplicated row list to
/// append, plus the per-key accounting the refusal evidence needs.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Nominations {
    pub(crate) rows: Vec<RepairRowId>,
    pub(crate) per_key: BTreeMap<(u32, u32), KeyNomination>,
}

/// Batch-first, deterministic nomination of repair-pool rows for one attempt's
/// blocking patches.
///
/// **Candidates are scored against the samples the measurement found
/// UNCOVERED, never against every sample on a blocking patch's faces.** A
/// blocking patch is a connected group of faces each of which has at least
/// one uncovered sample; a face can be — and on real geometry usually is —
/// partially covered. Scoring against every sample on those faces sums
/// already-covered area into the score, which lets a candidate sitting beside
/// a large covered region outrank the candidate that actually closes the
/// residual gap, and (because the score set is fixed across attempts) makes
/// the whole 8-round ranking walk useless rows while a viable closer waits
/// below them. `uncovered_samples` is the measurement's own
/// [`crate::CoverageMeasurement::uncovered_sample_indices`], the same
/// discipline the legacy `rank_coverage_repair_rows` (`lib.rs`) applies when
/// it scores against failing sample rows only.
///
/// Occupancy is read ONCE ([`SpecializedContactPlan::occupied_repair_rows`]),
/// and the whole pool is grouped by `(island_id, component_id)` ONCE
/// ([`SpecializedContactPlan::all_repair_rows`]) — never per patch, which is
/// what [`SpecializedContactPlan::eligible_repair_rows`] would cost if called
/// once per blocking patch (it re-derives occupancy on every call). Scoring
/// within one `(island_id, component_id)` group is tiled exactly as
/// `rank_coverage_repair_rows` (`lib.rs`) tiles it: the same registered
/// `coverage_score_matrix_max_entries` budget, clamped to the same 65,536
/// production ceiling, the same `sample_tile × candidate_tile` shape, and the
/// same tie-break (score descending, then pool order ascending) — so ordering
/// stays total and deterministic across repeated runs on the same plan.
///
/// One row is nominated PER blocking patch (not one per attempt and not one
/// per key), up to the number of eligible rows the key actually has, so a
/// component with several simultaneous blocking patches can close more than
/// one of them in a single attempt. Nominated rows are deduplicated by id
/// AND by candidate seat point across the whole attempt, matching
/// `append_regular_repair_row`'s own `DuplicateChildPhysicalSeat` refusal.
///
/// # Errors
///
/// Propagates [`SpecializedError`] from
/// [`SpecializedContactPlan::occupied_repair_rows`].
pub(crate) fn nominate_repair_rows(
    plan: &SpecializedContactPlan,
    blocking: &[BlockingPatchKey],
    uncovered_samples: &[usize],
    radius_mm: f64,
    matrix_budget: usize,
    failed: &BTreeSet<RepairRowId>,
) -> Result<Nominations, SpecializedError> {
    let mut excluded: BTreeSet<RepairRowId> = plan
        .occupied_repair_rows()?
        .into_iter()
        .map(|occupancy| occupancy.row_id())
        .collect();
    excluded.extend(failed.iter().copied());

    // A repair-pool row's identity is source-qualified by its OWNER ISLAND
    // (`RepairPool` doc, specialized.rs): the same physical candidate seat
    // legally appears as a distinct `RepairRowId` under every island whose
    // face set contains it, so excluding by row id alone does not exclude an
    // already-occupied seat's OTHER island-owned rows. `append_regular_repair_rows`
    // itself refuses a coincident seat with `DuplicateChildPhysicalSeat`
    // regardless of owner, so nomination must apply that same physical-point
    // exclusion BEFORE proposing a row, not discover it only when the append
    // refuses. One pass over the plan's existing contacts, gathered once.
    let mut claimed_points: Vec<[f64; 3]> = plan
        .contacts()
        .iter()
        .map(|contact| contact.position_mm())
        .collect();

    let mut rows_by_key: BTreeMap<(u32, u32), Vec<&RepairRow>> = BTreeMap::new();
    for row in plan.all_repair_rows() {
        if excluded.contains(&row.id())
            || claimed_points
                .iter()
                .any(|point| *point == row.candidate().point_mm)
        {
            continue;
        }
        rows_by_key
            .entry((row.id().owner_island_id(), row.id().component_id()))
            .or_default()
            .push(row);
    }

    // Only the measurement's uncovered rows are grouped: one gathered pass
    // over the uncovered set (never over every sample), so the scoring matrix
    // below is sized by what still needs covering rather than by the whole
    // island. `uncovered_samples` is ascending, so each group stays ascending
    // and the tiling below is order-stable.
    let samples = plan.coverage_samples();
    let mut samples_by_key: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for &index in uncovered_samples {
        let Some(sample) = samples.get(index) else {
            // The measurement is taken against this plan's own sample slice,
            // so this cannot happen; skipping rather than indexing keeps a
            // future caller mismatch from panicking inside the controller.
            continue;
        };
        samples_by_key
            .entry((sample.island_id, sample.component_id))
            .or_default()
            .push(index);
    }

    let mut patches_by_key: BTreeMap<(u32, u32), Vec<&BlockingPatchKey>> = BTreeMap::new();
    for patch in blocking {
        patches_by_key
            .entry((patch.island_id, patch.component_id))
            .or_default()
            .push(patch);
    }

    let radius_sq = radius_mm * radius_mm;
    let matrix_budget = matrix_budget.clamp(1, 65_536);
    let sample_tile = ((matrix_budget as f64).sqrt() as usize).max(1);
    let candidate_tile = (matrix_budget / sample_tile).max(1);

    // `claimed_points` already carries every seat this plan's existing
    // contacts occupy (seeded above); picking below extends it in place so a
    // seat chosen for an earlier key in THIS attempt is excluded from every
    // later key too.
    let mut nominated = Nominations::default();

    for (key, patches) in patches_by_key {
        let Some(candidate_rows) = rows_by_key.get(&key) else {
            // Recorded rather than silently skipped: "this blocking key had
            // zero eligible repair-pool rows" is the distinction the refusal
            // evidence must carry, and it used to leave no trace at all.
            nominated.per_key.insert(key, KeyNomination::default());
            continue;
        };
        let mut sorted_faces: Vec<u32> = patches
            .iter()
            .flat_map(|patch| patch.sorted_face_ids.iter().copied())
            .collect();
        sorted_faces.sort_unstable();
        sorted_faces.dedup();
        let scoring_samples: Vec<usize> = samples_by_key
            .get(&key)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&index| {
                sorted_faces
                    .binary_search(&plan.coverage_samples()[index].face_id)
                    .is_ok()
            })
            .collect();

        let mut scores = vec![0.0_f64; candidate_rows.len()];
        let candidate_indices: Vec<usize> = (0..candidate_rows.len()).collect();
        for candidate_block in candidate_indices.chunks(candidate_tile) {
            for sample_block in scoring_samples.chunks(sample_tile) {
                debug_assert!(candidate_block.len() * sample_block.len() <= matrix_budget);
                for &candidate_offset in candidate_block {
                    let point = candidate_rows[candidate_offset].candidate().point_mm;
                    for &sample_index in sample_block {
                        let sample = &plan.coverage_samples()[sample_index];
                        if squared_distance(point, sample.point_mm) <= radius_sq {
                            scores[candidate_offset] += sample.area_mm2;
                        }
                    }
                }
            }
        }

        let mut ranked: Vec<usize> = candidate_indices.clone();
        ranked.sort_by(|&left, &right| {
            scores[left].total_cmp(&scores[right]).reverse().then_with(|| {
                candidate_rows[left]
                    .order()
                    .cmp(&candidate_rows[right].order())
            })
        });

        let mut picked = 0usize;
        for offset in ranked {
            if picked >= patches.len() {
                break;
            }
            let row = candidate_rows[offset];
            let point = row.candidate().point_mm;
            if claimed_points.contains(&point) {
                continue;
            }
            claimed_points.push(point);
            nominated.rows.push(row.id());
            picked += 1;
        }
        nominated.per_key.insert(
            key,
            KeyNomination {
                eligible_rows: candidate_rows.len(),
                nominated_rows: picked,
            },
        );
    }

    Ok(nominated)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn key(island_id: u32, component_id: u32, faces: &[u32]) -> BlockingPatchKey {
        BlockingPatchKey {
            island_id,
            component_id,
            representative_face_id: faces[0],
            sorted_face_ids: faces.to_vec(),
            uncovered_area_mm2_bits: 1.5_f64.to_bits(),
            worst_gap: CoverageClosureGap::Finite(0.5),
        }
    }

    #[test]
    fn blocking_predicate_matches_the_verifier_truth_table() {
        for (area, cut_ambiguous, assessment_failed, expected) in [
            (0.0, false, false, false),
            (0.0, true, false, true),
            (0.0, false, true, true),
            (1e-12, false, false, true),
            (3.4, false, false, true),
        ] {
            assert_eq!(
                coverage_patch_is_blocking(area, cut_ambiguous, assessment_failed),
                expected,
                "area={area} cut_ambiguous={cut_ambiguous} assessment_failed={assessment_failed}"
            );
        }
    }

    #[test]
    fn islands_from_blocking_aggregates_by_island_and_component_like_the_verifier() {
        let patches = vec![
            key(2, 0, &[10, 11]),
            key(2, 0, &[12]),
            key(7, 1, &[20]),
        ];
        // Island 2 was nominated for twice and still has rows left; island 7
        // never had an eligible row at all. Those are the two different
        // stories the refusal must be able to tell apart.
        let mut tally = BTreeMap::new();
        tally.insert(
            (2, 0),
            KeyNomination {
                eligible_rows: 4,
                nominated_rows: 2,
            },
        );
        tally.insert((7, 1), KeyNomination::default());
        let islands = islands_from_blocking(&patches, &tally);
        assert_eq!(islands.len(), 2);
        let island_two = islands
            .iter()
            .find(|island| island.island_id == 2)
            .expect("island 2 present");
        assert_eq!(island_two.patch_count, 2);
        assert_eq!(island_two.sorted_face_ids, vec![10, 11, 12]);
        assert!((island_two.uncovered_area_mm2 - 3.0).abs() < 1e-9);
        assert_eq!(
            (
                island_two.nominated_repair_rows,
                island_two.eligible_repair_rows
            ),
            (2, 4)
        );
        let island_seven = islands
            .iter()
            .find(|island| island.island_id == 7)
            .expect("island 7 present");
        assert_eq!(
            (
                island_seven.nominated_repair_rows,
                island_seven.eligible_repair_rows
            ),
            (0, 0),
            "an island that never had an eligible repair row must be distinguishable from one \
             that was nominated for and never closed"
        );
    }

    #[test]
    fn worse_gap_prefers_no_routed_contact_over_any_finite_distance() {
        assert_eq!(
            worse_gap(
                CoverageClosureGap::Finite(10.0),
                CoverageClosureGap::NoRoutedContact
            ),
            CoverageClosureGap::NoRoutedContact
        );
        assert_eq!(
            worse_gap(CoverageClosureGap::Finite(1.0), CoverageClosureGap::Finite(2.0)),
            CoverageClosureGap::Finite(2.0)
        );
    }
}
