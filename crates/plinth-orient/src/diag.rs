//! Pose-search diagnostics (pose-stability design, Lane 0).
//!
//! The sweep decides a pose through a funnel of discrete cuts (the design's
//! G1-G4 on the grid, C1-C4 on the candidates). Any such cut has boundaries a
//! last-bit change can cross. This module RECORDS every boundary and the
//! margin to it, keyed by provenance rather than by candidate id, so a later
//! lane can say which stage first lost a pose and by how much. It decides
//! nothing.
//!
//! # The one rule: nothing here feeds the search
//!
//! * [`SweepDiagnostics`] is a separate value. [`crate::search::DenseSweepRecord`]
//!   and [`crate::search::RankedCandidate`] are unchanged, so
//!   `search_digest` (which hashes the whole serialized record) is unchanged,
//!   with the diagnostics on or off.
//! * Provenance keys ([`CandidateKey`]) live only in the diagnostics.
//! * The extra production measurements (`prod-all`, `extra=`) run AFTER the
//!   record and its digest are final, through a SEPARATE non-retaining
//!   measurer ([`SupportDemandMeasurer::diagnostic_measurer`]). A retaining
//!   measurer replaces its retained production batch on every production call,
//!   so a diagnostic batch through it would evict the winner's plan. What the
//!   separate measurer returns is written to the diagnostics and goes nowhere
//!   else; it never reaches `rerank_commit`.
//! * Collection is batch-first: G1 and G2 are read off the grid's own binning
//!   pass through an observer ([`GridObserver`]; the production scan is the
//!   one scan, and with no observer it carries no extra work), one dense pass
//!   for G4, bounded selections for the "smallest margin" lists, and one
//!   production batch for every extra pose.
//! * The diagnostics' time is not the pipeline's: [`crate::search`] runs the
//!   build and the `orient.json` write in `geom_kernel::perf::Phase::Diagnostic`
//!   and restores the phase afterwards, so the perf sidecar's own phases
//!   (`OrientTable`, `Prefilter`, `Production`) do not count it.
//!
//! # Switches (`PLINTH_ORIENT_DIAG`)
//!
//! Unset: with `--perf` on, the diagnostics are collected and written to
//! `<perf-dir>/<model>.orient.json` (the record and the diagnostics); with
//! `--perf` off, nothing is collected. Comma-separated words:
//!
//! | word | effect |
//! |---|---|
//! | `off` | collect nothing, even with `--perf` |
//! | `digest` | print `[orient] search_digest <sha256>` to stderr (needs no `--perf`) |
//! | `prod-all` | also measure EVERY commit-admitted pose at production pitch |
//! | `extra=X,Y,Z;X,Y,Z` | also score these directions the same way (last word: it takes the rest) |
//!
//! `prod-all` and `extra=` run production sweeps (seconds each on a big
//! model) and add to the perf counters; a run that uses them is not a timing
//! run.

use std::cmp::Ordering;
use std::collections::{BTreeSet, BinaryHeap};

use geom_kernel::vec3;
use plinth_mesh::Mesh;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ablate::Ablation;
use crate::demand::{DemandFidelity, MeasuredSupportDemand, SupportDemandMeasurer};
use crate::grid::{
    hex, kept_bins, BinObserver, CandidateBlock, CandidateKey, GridAttempt, GridSettings, RestBins,
    RestOrder, SweepGrid,
};
use crate::score::{
    score_candidates, CandidateOutcome, ObjectiveWeights, OrientFaceTable, UpDirection,
};
use crate::search::{DenseSweepRecord, RankedCandidate, SweepRequest, TermsRecord, TierRecord};

/// The diagnostics' own schema.
pub const DIAGNOSTICS_SCHEMA_VERSION: u32 = 3;

/// The pose-identity tolerance of the stability contract (design section 3):
/// two directions within this angle are "the same pose". One face changing bin
/// shifts a rest pose by about 0.02 deg on BCO; this is five times that and
/// ten times inside the dedupe radius.
pub const THETA_STAB_DEG: f64 = 0.1;

/// How many of the smallest margins each boundary list keeps.
const SMALLEST_MARGINS: usize = 256;

/// How many rejected rows past the first one each cut keeps.
const NEXT_REJECTED: usize = 8;

/// What to collect for one search.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DiagnosticsConfig {
    /// Measure every commit-admitted pose at production pitch.
    pub prod_all: bool,
    /// Directions to score and measure the same way.
    pub extra: Vec<UpDirection>,
    /// TEST ONLY: feed the diagnostic production measurements into the rerank
    /// (the defect the design forbids), so a test can show the identity
    /// checks would catch it.
    #[cfg(test)]
    pub(crate) sabotage_feed_rerank: bool,
}

/// The parsed `PLINTH_ORIENT_DIAG`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Switches {
    pub off: bool,
    pub digest: bool,
    pub prod_all: bool,
    pub extra: Vec<UpDirection>,
    /// Words that did not parse; the caller prints them.
    pub warnings: Vec<String>,
}

impl Switches {
    /// Reads `PLINTH_ORIENT_DIAG`; unset reads as no switches.
    #[must_use]
    pub fn from_env() -> Self {
        std::env::var("PLINTH_ORIENT_DIAG")
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Parses the switch text (module doc).
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut switches = Self::default();
        let (head, tail) = match text.split_once("extra=") {
            Some((head, tail)) => (head, Some(tail)),
            None => (text, None),
        };
        for word in head
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|word| !word.is_empty())
        {
            match word {
                "off" => switches.off = true,
                "digest" => switches.digest = true,
                "prod-all" => switches.prod_all = true,
                "on" => {}
                other => switches
                    .warnings
                    .push(format!("unknown word {other:?}, ignored")),
            }
        }
        for item in tail
            .into_iter()
            .flat_map(|tail| tail.split(';'))
            .filter(|item| !item.trim().is_empty())
        {
            match parse_direction(item) {
                Ok(direction) => switches.extra.push(direction),
                Err(reason) => switches
                    .warnings
                    .push(format!("extra direction {item:?}: {reason}, ignored")),
            }
        }
        switches
    }

    /// Whether this run asks for extra production sweeps.
    #[must_use]
    pub fn wants_production_sweeps(&self) -> bool {
        self.prod_all || !self.extra.is_empty()
    }

    /// The collection to run: `None` when `off` or when `--perf` is off (there
    /// is nowhere to write it).
    #[must_use]
    pub fn config(&self, perf_on: bool) -> Option<DiagnosticsConfig> {
        if self.off || !perf_on {
            return None;
        }
        Some(DiagnosticsConfig {
            prod_all: self.prod_all,
            extra: self.extra.clone(),
            #[cfg(test)]
            sabotage_feed_rerank: false,
        })
    }
}

fn parse_direction(text: &str) -> Result<UpDirection, String> {
    let parts: Vec<&str> = text.split(',').map(str::trim).collect();
    let [x, y, z] = parts[..] else {
        return Err("want X,Y,Z".to_owned());
    };
    let number = |part: &str| part.parse::<f64>().map_err(|_| format!("{part:?} is not a number"));
    UpDirection::new([number(x)?, number(y)?, number(z)?]).map_err(|error| error.to_string())
}

// ---------------------------------------------------------------- the data

/// Everything the diagnostics hold for one search.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SweepDiagnostics {
    pub schema_version: u32,
    pub theta_stab_deg: f64,
    pub model_fingerprint: String,
    /// The record's own digest, repeated so the two files can be matched.
    pub search_digest: String,
    pub prod_all_requested: bool,
    pub extra_directions_requested: usize,
    /// Candidate id to provenance key and direction.
    pub candidates: Vec<CandidateRow>,
    /// G1, G2 and G4 are read off the table the grid was built from
    /// ([`Self::grid_table_lod`]).
    pub grid_table_lod: String,
    /// Read off the grid's own binning pass (see [`GridObserver`]).
    pub g1: G1Diagnostics,
    pub g2: G2Diagnostics,
    /// G3 for EVERY direction the grid tried, kept or dropped.
    pub g3: Vec<G3Row>,
    pub g4: G4Diagnostics,
    /// `true` when the rest poses rebuilt from the captured G1/G2 data are
    /// the grid's rest poses bit for bit (so G1 and G2 describe the grid that
    /// was searched), and the diagnostic G4 pass found the grid's flat-pose
    /// count.
    pub reproduces_grid: bool,
    pub escort: EscortDiagnostics,
    /// C1 to C4, as many as ran.
    pub cuts: Vec<CutDiagnostics>,
    pub snapshots: Snapshots,
    /// `prod-all`: every commit-admitted pose measured at production pitch by
    /// the separate measurer. `None` when not requested.
    pub production_all: Option<Vec<DiagnosticScore>>,
    /// `extra=`: the given directions, scored and measured the same way.
    pub extras: Vec<DiagnosticScore>,
    pub notes: Vec<String>,
    /// Which ablation words were on for this search (schema 3).
    #[serde(default)]
    pub ablation: AblationRecord,
    /// `exact-shadow`: the proxy cuts' inputs scored on the exact table (schema
    /// 3). `None` when not requested.
    #[serde(default)]
    pub exact_shadow: Option<ExactShadow>,
}

/// The ablation words a search ran with, so an `orient.json` says which
/// search it describes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AblationRecord {
    pub exact_grid: bool,
    /// Every funnel tier scored on the exact table (schema 3).
    #[serde(default)]
    pub exact_score: bool,
    pub exact_shadow: bool,
    /// `"by_area"` (recorded) or `"by_bin_index"` (`exact-grid`).
    pub rest_order: String,
}

/// `exact-shadow` (design Lane A, R6): C1 and C2 repeated on the EXACT table
/// over the SAME input key sets the proxy cuts saw. It scores and ranks with
/// the search's own comparator and decides nothing: the real run's cuts stand.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExactShadow {
    /// C1 on the exact table: the whole grid, as the coarse proxy saw it.
    pub c1: ShadowCut,
    /// C2 on the exact table: the candidates the REAL C1 passed on.
    pub c2: ShadowCut,
}

/// One cut repeated on the exact table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShadowCut {
    /// The exact-table cut, in the shape of the real ones (`C1x`, `C2x`).
    pub cut: CutDiagnostics,
    /// The key set scored: identical to the proxy cut's input.
    pub input: Vec<KeyedId>,
    /// Every scored row on the exact table, best first, full terms.
    pub ranking: Vec<KeyedRanked>,
    /// The top `k` on the exact table.
    pub kept: Vec<KeyedId>,
    /// The top `k` the proxy cut kept from the same input.
    pub proxy_kept: Vec<KeyedId>,
    /// The two kept key sets are equal.
    pub same_kept_set: bool,
    /// C2 only: the commit tier's input after the escort, exact shadow.
    pub admitted: Option<Vec<KeyedId>>,
    /// C2 only: the commit tier's input in the real run.
    pub proxy_admitted: Option<Vec<KeyedId>>,
    /// Candidates the exact table refused (excluded from the ranking).
    pub refused: usize,
}

/// A margin's standing against its envelope (design section 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryEvent {
    /// The margin is above the envelope: a change of that size cannot cross it.
    Clear,
    /// The margin is at or below the envelope (or not a number): a change of
    /// that size can cross it, so the decision is not claimed stable.
    Ambiguous,
}

/// Classifies a boundary margin against the envelope measured for its stage.
/// The envelope is an input; this function owns no number.
#[must_use]
pub fn boundary_event(margin: f64, envelope: f64) -> BoundaryEvent {
    if margin.abs() > envelope {
        BoundaryEvent::Clear
    } else {
        BoundaryEvent::Ambiguous
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CandidateRow {
    pub id: usize,
    pub key: CandidateKey,
    pub block: CandidateBlock,
    pub direction: [f64; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyedId {
    pub id: usize,
    pub key: CandidateKey,
}

/// A ranking row with its provenance key. Full terms.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyedRanked {
    pub rank: usize,
    pub candidate: usize,
    pub key: CandidateKey,
    /// Carried to the commit tier by right (the legacy thirteen and the
    /// flat-face poses), not by promotion.
    pub by_right: bool,
    /// The row's burden was measured at the production raster. The commit
    /// comparator sorts production-measured rows ahead of the rest whatever
    /// their scores, so scores compare only inside one class. `false` for
    /// every ranking that predates the production measurement.
    #[serde(default)]
    pub measured_at_production: bool,
    pub terms: TermsRecord,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct G1Diagnostics {
    pub faces: usize,
    pub binned_faces: usize,
    pub bins: usize,
    /// SHA-256 over the little-endian `u32` bin of every face in face order
    /// (`u32::MAX` for a face the pass skips). Any single reassignment moves it.
    pub face_to_bin_sha256: String,
    /// The faces whose best and second-best bins are closest, smallest gap
    /// first (ties by face index).
    pub smallest_gaps: Vec<G1Face>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct G1Face {
    pub face: usize,
    pub area_mm2: f64,
    pub best: CandidateKey,
    pub best_dot: f64,
    pub second: CandidateKey,
    pub second_dot: f64,
    /// `best_dot - second_dot`, never negative.
    pub gap: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct G2Diagnostics {
    pub occupied_bins: usize,
    pub rest_pose_max: usize,
    /// The kept bins by rank, then the first rejected ones.
    pub ranked: Vec<G2Row>,
    /// Area gap between the last kept bin and the first rejected one.
    pub cut_gap_abs: Option<f64>,
    pub cut_gap_rel: Option<f64>,
    /// The smallest relative gap between adjacent KEPT bins (their order
    /// numbers the rest poses).
    pub min_adjacent_rel_gap: Option<G2Adjacent>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct G2Row {
    pub rank: usize,
    pub key: CandidateKey,
    pub area_mm2: f64,
    pub kept: bool,
    pub gap_to_next_abs: Option<f64>,
    pub gap_to_next_rel: Option<f64>,
    /// The pose, for a kept bin whose faces do not cancel.
    pub direction: Option<[f64; 3]>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct G2Adjacent {
    pub rank: usize,
    pub rel_gap: f64,
}

/// A candidate seen from another direction: how far, and on which side of the
/// dedupe radius.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct G3Ref {
    pub key: CandidateKey,
    pub id: usize,
    pub angle_deg: f64,
    /// `angle - dedupe tolerance`: negative inside the radius.
    pub margin_deg: f64,
    /// `dot - cos(tolerance)`: positive inside the radius (the exact test).
    pub dot_minus_cosine: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct G3Row {
    pub key: CandidateKey,
    pub kept: bool,
    pub id: Option<usize>,
    /// The earliest earlier candidate inside the radius: the one that dropped
    /// this direction. `None` for a kept direction.
    pub first_cover: Option<G3Ref>,
    /// The nearest earlier kept candidate, kept or dropped alike.
    pub nearest_earlier: Option<G3Ref>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct G4Diagnostics {
    pub tolerance_deg: f64,
    pub cosine: f64,
    pub poses: Vec<G4Pose>,
    /// Flat poses the diagnostic pass found; equals the grid's count when
    /// [`SweepDiagnostics::reproduces_grid`].
    pub flat_poses: usize,
    /// The candidate ids carried to the commit tier as flat-face poses.
    pub escorted_ids: Vec<usize>,
    /// (face, rest pose) pairs by `|facing - cos(tolerance)|`: the flat test.
    pub smallest_flat_margins: Vec<G4Face>,
    /// By `|facing - 0|`: whether a face counts toward the side at all.
    pub smallest_side_margins: Vec<G4Face>,
    pub covers: Vec<G4Cover>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct G4Pose {
    pub key: CandidateKey,
    pub flat_area_mm2: f64,
    pub side_area_mm2: f64,
    /// `(flat - (side - flat)) / side`: positive means flat (the strict test
    /// is `flat > side - flat`).
    pub margin_rel: f64,
    pub is_flat: bool,
    pub flat_faces: usize,
    /// SHA-256 over the pose's bin and the ascending indices of its flat faces.
    pub flat_set_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct G4Face {
    pub face: usize,
    pub pose: CandidateKey,
    pub facing: f64,
    /// `facing - threshold` (the cosine for the flat list, zero for the side list).
    pub margin: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct G4Cover {
    pub pose: CandidateKey,
    /// The candidate the escort rides on: the earliest within the radius.
    pub covering: Option<G3Ref>,
    /// The two nearest candidates by angle (the runner-up is the second).
    pub nearest: Vec<G3Ref>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EscortRow {
    pub key: CandidateKey,
    pub id: usize,
    /// `"legacy"` or `"flat_face"`.
    pub source: String,
    pub already_promoted: bool,
    /// This row added the candidate to the commit set.
    pub added: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EscortDiagnostics {
    pub promoted: Vec<KeyedId>,
    pub by_right: Vec<EscortRow>,
    pub added: usize,
    /// The commit tier's candidate set, ascending id.
    pub admitted: Vec<KeyedId>,
}

/// What decided a cut between its last kept row and its first rejected row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CutBasis {
    /// The comparator went to the score: the rows are in one fidelity class
    /// (or the cut has fewer than two rows), and `score_gap` is the margin.
    Score,
    /// The kept row is production-measured and the rejected one is not: the
    /// comparator put the kept row first before it looked at any score.
    ProductionFidelity,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CutDiagnostics {
    /// `"C1"` to `"C4"`.
    pub name: String,
    pub stage: String,
    pub lod: String,
    pub k: usize,
    pub ranked: usize,
    /// What put the last kept row ahead of the first rejected one.
    pub decided_by: CutBasis,
    /// Score of the first rejected row minus the last kept row. `Some` only
    /// when the two rows are in one fidelity class ([`CutBasis::Score`]); when
    /// production precedence decided, a score difference is not the margin and
    /// is not reported.
    pub score_gap: Option<f64>,
    pub last_kept: Option<KeyedRanked>,
    pub first_rejected: Option<KeyedRanked>,
    pub next_rejected: Vec<KeyedRanked>,
    /// Rows below the cut that the escort added to the next stage's input, by
    /// right. Only C2 has an escort; every other cut lists none.
    pub rescued_by_escort: Vec<CandidateKey>,
}

/// The commit tier's ranking at three moments. The in-record ranking is
/// repriced in place, so these are the copies.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshots {
    pub exact_table: Vec<KeyedRanked>,
    pub after_prefilter: Option<Vec<KeyedRanked>>,
    pub after_production: Option<Vec<KeyedRanked>>,
}

/// One pose measured at production pitch by the separate measurer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticScore {
    /// The candidate key, or `extra:N`.
    pub label: String,
    pub key: Option<CandidateKey>,
    pub candidate: Option<usize>,
    pub direction: [f64; 3],
    pub exact_terms: Option<TermsRecord>,
    /// The exact-table terms repriced with the production measurement, the
    /// way the search reprices a finalist.
    pub production_terms: Option<TermsRecord>,
    pub demand: Option<DiagnosticDemand>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticDemand {
    pub support_area_cm2: f64,
    pub struts: usize,
    pub braces: usize,
    pub masts: usize,
    pub total_scar_mm2: f64,
    pub achieved_pitch_mm: f64,
    pub residual_demand_cells: usize,
    pub uncarried_load_mm2: f64,
    pub model_landings: usize,
    pub landing_scar_raw_mm2: Option<f64>,
    pub tip_scar_raw_mm2: Option<f64>,
}

impl From<&MeasuredSupportDemand> for DiagnosticDemand {
    fn from(measured: &MeasuredSupportDemand) -> Self {
        Self {
            support_area_cm2: measured.support_area_cm2,
            struts: measured.struts,
            braces: measured.braces,
            masts: measured.masts,
            total_scar_mm2: measured.total_scar_mm2,
            achieved_pitch_mm: measured.achieved_pitch_mm,
            residual_demand_cells: measured.residual_demand_cells,
            uncarried_load_mm2: measured.uncarried_load_mm2,
            model_landings: measured.model_landings,
            landing_scar_raw_mm2: measured.landing_scar_raw_mm2,
            tip_scar_raw_mm2: measured.tip_scar_raw_mm2,
        }
    }
}

// ------------------------------------------------------------- the inputs

/// What `search` hands over once the record is final.
pub(crate) struct DiagInputs<'a> {
    pub config: &'a DiagnosticsConfig,
    pub mesh: &'a Mesh,
    pub request: &'a SweepRequest,
    pub grid: &'a SweepGrid,
    /// What the observer saw in the grid's own binning pass.
    pub grid_capture: &'a GridCapture,
    pub grid_table: &'a OrientFaceTable,
    pub grid_table_lod: &'a str,
    pub ablation: &'a Ablation,
    pub exact_table: &'a OrientFaceTable,
    pub demand: Option<&'a dyn SupportDemandMeasurer>,
    pub tiers: &'a [TierRecord],
    pub snapshots: &'a RawSnapshots,
    pub record: &'a DenseSweepRecord,
}

/// The commit tier's three copies and the escort's inputs, captured in place.
#[derive(Clone, Debug, Default)]
pub(crate) struct RawSnapshots {
    pub exact_table: Vec<RankedCandidate>,
    pub after_prefilter: Option<Vec<RankedCandidate>>,
    pub after_production: Option<Vec<RankedCandidate>>,
    pub promoted_to_commit: Vec<usize>,
}

// ------------------------------------------------------------ the builder

pub(crate) fn build(inputs: &DiagInputs<'_>) -> SweepDiagnostics {
    let mut notes = Vec::new();
    let grid = inputs.grid;
    let settings = inputs.request.grid;
    let keys = grid.keys();

    let candidates: Vec<CandidateRow> = grid
        .candidates()
        .iter()
        .zip(keys)
        .map(|(candidate, key)| CandidateRow {
            id: candidate.id,
            key: *key,
            block: candidate.block,
            direction: candidate.direction.get(),
        })
        .collect();

    let (g1, g2, rest_reproduced) =
        g1_g2(inputs.grid_table, inputs.grid_capture, settings, grid);
    let g3 = g3_rows(grid.attempts(), settings);
    let g4 = g4_diagnostics(inputs.grid_table, settings, grid);
    let flat_reproduced = g4.flat_poses == grid.flat_face_count();
    if !rest_reproduced {
        notes.push(
            "the rest poses rebuilt from the captured binning pass are not the grid's, bit for bit"
                .to_owned(),
        );
    }
    if !flat_reproduced {
        notes.push("the diagnostic flat-pose count differs from the grid's".to_owned());
    }

    let by_right_ids: BTreeSet<usize> = by_right_ids(grid).into_iter().collect();
    let escort = escort_decisions(
        &inputs.snapshots.promoted_to_commit,
        &legacy_ids(grid),
        grid.flat_face_poses(),
        keys,
    );

    // Before the production measurement no row is production-measured; after
    // it, the record's commit-demand rows say which are.
    let none_at_production = BTreeSet::new();
    let at_production: BTreeSet<usize> = inputs
        .record
        .commit_tier_demand
        .iter()
        .filter(|row| row.measured_at_production())
        .map(|row| row.candidate)
        .collect();
    let before = RowKeys {
        keys,
        by_right: &by_right_ids,
        at_production: &none_at_production,
    };
    let after = RowKeys {
        keys,
        by_right: &by_right_ids,
        at_production: &at_production,
    };
    let keyed = |rows: &RowKeys<'_>, ranking: &[RankedCandidate]| -> Vec<KeyedRanked> {
        ranking.iter().map(|entry| rows.row(entry)).collect()
    };

    // The search escorts only after the refine tier (C2). Every other cut
    // passes its top k and nothing else, so nothing is rescued there.
    let no_rescue = BTreeSet::new();
    let escort_added: BTreeSet<usize> = escort
        .by_right
        .iter()
        .filter(|row| row.added)
        .map(|row| row.id)
        .collect();
    let mut cuts = Vec::new();
    let ladder = inputs.request.ladder;
    if let Some(coarse) = inputs.tiers.first() {
        cuts.push(cut(
            "C1",
            "coarse -> refine",
            &coarse.lod,
            ladder.refine_top_k(),
            &coarse.ranking,
            &before,
            &no_rescue,
        ));
    }
    if let Some(refine) = inputs.tiers.get(1) {
        cuts.push(cut(
            "C2",
            "refine -> commit",
            &refine.lod,
            ladder.commit_top_k(),
            &refine.ranking,
            &before,
            &escort_added,
        ));
    }
    if let Some(after_prefilter) = &inputs.snapshots.after_prefilter {
        let finalists = ladder.commit_top_k().min(after_prefilter.len());
        cuts.push(cut(
            "C3",
            "commit prefilter -> production",
            crate::search::EXACT_LOD,
            finalists,
            after_prefilter,
            &before,
            &no_rescue,
        ));
    }
    if let Some(after_production) = &inputs.snapshots.after_production {
        cuts.push(cut(
            "C4",
            "commit production -> winner",
            crate::search::EXACT_LOD,
            1,
            after_production,
            &after,
            &no_rescue,
        ));
    }

    let snapshots = Snapshots {
        exact_table: keyed(&before, &inputs.snapshots.exact_table),
        after_prefilter: inputs
            .snapshots
            .after_prefilter
            .as_deref()
            .map(|ranking| keyed(&before, ranking)),
        after_production: inputs
            .snapshots
            .after_production
            .as_deref()
            .map(|ranking| keyed(&after, ranking)),
    };

    let (production_all, extras) = production_diagnostics(inputs, &mut notes);
    let exact_shadow = exact_shadow(inputs, &escort, &mut notes);
    let ablation = AblationRecord {
        exact_grid: inputs.ablation.exact_grid,
        exact_score: inputs.ablation.exact_score,
        exact_shadow: inputs.ablation.exact_shadow,
        rest_order: match grid.rest_order() {
            RestOrder::ByArea => "by_area",
            RestOrder::ByBinIndex => "by_bin_index",
        }
        .to_owned(),
    };

    SweepDiagnostics {
        schema_version: DIAGNOSTICS_SCHEMA_VERSION,
        theta_stab_deg: THETA_STAB_DEG,
        model_fingerprint: inputs.record.model_fingerprint.clone(),
        search_digest: inputs.record.search_digest.clone(),
        prod_all_requested: inputs.config.prod_all,
        extra_directions_requested: inputs.config.extra.len(),
        candidates,
        grid_table_lod: inputs.grid_table_lod.to_owned(),
        g1,
        g2,
        g3,
        g4,
        reproduces_grid: rest_reproduced && flat_reproduced,
        escort,
        cuts,
        snapshots,
        production_all,
        extras,
        notes,
        ablation,
        exact_shadow,
    }
}

/// `exact-shadow`: C1 and C2 repeated on the exact table over the proxy cuts'
/// own input key sets. Out of band: it runs after the record is final and
/// nothing it computes is read back.
fn exact_shadow(
    inputs: &DiagInputs<'_>,
    real_escort: &EscortDiagnostics,
    notes: &mut Vec<String>,
) -> Option<ExactShadow> {
    if !inputs.ablation.exact_shadow {
        return None;
    }
    let grid = inputs.grid;
    let keys = grid.keys();
    let ladder = inputs.request.ladder;
    let (Some(coarse), Some(refine)) = (inputs.tiers.first(), inputs.tiers.get(1)) else {
        notes.push("exact-shadow: the record has no C1/C2 tiers; nothing shadowed".to_owned());
        return None;
    };
    let _mark = geom_kernel::perf::stage("orient.diag_shadow");
    let by_right: BTreeSet<usize> = by_right_ids(grid).into_iter().collect();
    let none_at_production = BTreeSet::new();
    let no_rescue = BTreeSet::new();
    let rows = RowKeys {
        keys,
        by_right: &by_right,
        at_production: &none_at_production,
    };
    let keyed_id = |id: &usize| KeyedId {
        id: *id,
        key: keys[*id],
    };
    let key_set = |ids: &[KeyedId]| -> BTreeSet<CandidateKey> { ids.iter().map(|k| k.key).collect() };
    let weights = inputs.request.weights;
    let limits = inputs.request.limits;
    let exact_lod = crate::search::EXACT_LOD;

    let mut shadow = |name: &str,
                      stage: &str,
                      k: usize,
                      input: &[usize],
                      proxy_ranking: &[RankedCandidate],
                      proxy_promoted: usize,
                      rescue_escort: bool|
     -> Option<ShadowCut> {
        let (ranking, refused) = match crate::search::shadow_rank(
            inputs.exact_table,
            exact_lod,
            grid,
            input,
            weights,
            limits,
        ) {
            Ok(done) => done,
            Err(error) => {
                notes.push(format!("exact-shadow {name}: the exact table could not rank: {error}"));
                return None;
            }
        };
        let kept: Vec<KeyedId> = ranking.iter().take(k).map(|r| keyed_id(&r.candidate)).collect();
        let proxy_kept: Vec<KeyedId> = proxy_ranking
            .iter()
            .take(proxy_promoted)
            .map(|r| keyed_id(&r.candidate))
            .collect();
        let (admitted, proxy_admitted) = if rescue_escort {
            let kept_ids: Vec<usize> = ranking.iter().take(k).map(|r| r.candidate).collect();
            let shadow_escort = escort_decisions(
                &kept_ids,
                &legacy_ids(grid),
                grid.flat_face_poses(),
                keys,
            );
            (
                Some(shadow_escort.admitted),
                Some(real_escort.admitted.clone()),
            )
        } else {
            (None, None)
        };
        Some(ShadowCut {
            cut: cut(name, stage, exact_lod, k, &ranking, &rows, &no_rescue),
            input: input.iter().map(keyed_id).collect(),
            ranking: ranking.iter().map(|entry| rows.row(entry)).collect(),
            same_kept_set: key_set(&kept) == key_set(&proxy_kept),
            kept,
            proxy_kept,
            admitted,
            proxy_admitted,
            refused,
        })
    };

    // C1: the whole grid, exactly the set the coarse tier scored.
    let c1_input: Vec<usize> = (0..grid.candidates().len()).collect();
    let c1 = shadow(
        "C1x",
        "coarse -> refine, exact-table shadow",
        ladder.refine_top_k(),
        &c1_input,
        &coarse.ranking,
        coarse.promoted,
        false,
    )?;
    // C2: the candidates the REAL C1 passed on, so the two C2 cuts compare on
    // one input even where the C1 cuts differ.
    let c2_input: Vec<usize> = coarse
        .ranking
        .iter()
        .take(coarse.promoted)
        .map(|r| r.candidate)
        .collect();
    let c2 = shadow(
        "C2x",
        "refine -> commit, exact-table shadow",
        ladder.commit_top_k(),
        &c2_input,
        &refine.ranking,
        refine.promoted,
        true,
    )?;
    Some(ExactShadow { c1, c2 })
}

fn legacy_ids(grid: &SweepGrid) -> Vec<usize> {
    grid.candidates()
        .iter()
        .filter(|candidate| candidate.block == CandidateBlock::Legacy)
        .map(|candidate| candidate.id)
        .collect()
}

/// The ids the search carries to the commit tier by right.
fn by_right_ids(grid: &SweepGrid) -> Vec<usize> {
    let mut ids = legacy_ids(grid);
    ids.extend_from_slice(grid.flat_face_poses());
    ids
}

/// What a ranking row needs to become a [`KeyedRanked`].
struct RowKeys<'a> {
    keys: &'a [CandidateKey],
    by_right: &'a BTreeSet<usize>,
    /// The candidates whose burden was measured at the production raster in
    /// the ranking being read.
    at_production: &'a BTreeSet<usize>,
}

impl RowKeys<'_> {
    fn row(&self, entry: &RankedCandidate) -> KeyedRanked {
        KeyedRanked {
            rank: entry.rank,
            candidate: entry.candidate,
            key: self.keys[entry.candidate],
            by_right: self.by_right.contains(&entry.candidate),
            measured_at_production: self.at_production.contains(&entry.candidate),
            terms: entry.terms,
        }
    }
}

/// What decided between a cut's last kept row and its first rejected one: the
/// commit comparator sorts production-measured rows ahead of the rest before
/// it reads a score, so the score difference is the margin only inside one
/// fidelity class.
fn cut_basis(kept: &KeyedRanked, rejected: &KeyedRanked) -> CutBasis {
    if kept.measured_at_production == rejected.measured_at_production {
        CutBasis::Score
    } else {
        CutBasis::ProductionFidelity
    }
}

/// One cut of the funnel. `rescued` holds the ids this cut's escort added to
/// the next stage's input; it is empty for every cut without an escort.
fn cut(
    name: &str,
    stage: &str,
    lod: &str,
    k: usize,
    ranking: &[RankedCandidate],
    rows: &RowKeys<'_>,
    rescued: &BTreeSet<usize>,
) -> CutDiagnostics {
    let row = |index: usize| ranking.get(index).map(|entry| rows.row(entry));
    let last_kept = k.checked_sub(1).and_then(row);
    let first_rejected = row(k);
    let decided_by = match (&last_kept, &first_rejected) {
        (Some(kept), Some(rejected)) => cut_basis(kept, rejected),
        _ => CutBasis::Score,
    };
    let score_gap = match (&last_kept, &first_rejected, decided_by) {
        (Some(kept), Some(rejected), CutBasis::Score) => {
            Some(rejected.terms.score.get() - kept.terms.score.get())
        }
        _ => None,
    };
    let next_rejected: Vec<KeyedRanked> = (k + 1..k + 1 + NEXT_REJECTED).filter_map(row).collect();
    let rescued_by_escort: Vec<CandidateKey> = ranking
        .iter()
        .skip(k)
        .filter(|entry| rescued.contains(&entry.candidate))
        .map(|entry| rows.keys[entry.candidate])
        .collect();
    CutDiagnostics {
        name: name.to_owned(),
        stage: stage.to_owned(),
        lod: lod.to_owned(),
        k,
        ranked: ranking.len(),
        decided_by,
        score_gap,
        last_kept,
        first_rejected,
        next_rejected,
        rescued_by_escort,
    }
}

/// The escort's decisions, replayed from the same inputs `search::escort`
/// takes: legacy ids first, then the flat-face poses, each inserted into the
/// promoted set in that order.
pub(crate) fn escort_decisions(
    promoted: &[usize],
    legacy: &[usize],
    flat: &[usize],
    keys: &[CandidateKey],
) -> EscortDiagnostics {
    let promoted_set: BTreeSet<usize> = promoted.iter().copied().collect();
    let mut set = promoted_set.clone();
    let mut rows = Vec::with_capacity(legacy.len() + flat.len());
    let mut added = 0_usize;
    for (source, ids) in [("legacy", legacy), ("flat_face", flat)] {
        for id in ids {
            let newly = set.insert(*id);
            added += usize::from(newly);
            rows.push(EscortRow {
                key: keys[*id],
                id: *id,
                source: source.to_owned(),
                already_promoted: promoted_set.contains(id),
                added: newly,
            });
        }
    }
    let keyed_id = |id: &usize| KeyedId {
        id: *id,
        key: keys[*id],
    };
    EscortDiagnostics {
        promoted: promoted.iter().map(keyed_id).collect(),
        by_right: rows,
        added,
        admitted: set.iter().map(keyed_id).collect(),
    }
}

// ------------------------------------------------------------------ G1, G2

fn rest_key(bin: usize) -> CandidateKey {
    CandidateKey::Rest(u32::try_from(bin).unwrap_or(u32::MAX))
}

/// The angle between two unit vectors in degrees, by the chord, which keeps
/// its digits where `acos(dot)` loses them near zero.
fn angle_deg(a: [f64; 3], b: [f64; 3]) -> f64 {
    let chord = vec3::norm(vec3::sub(a, b));
    2.0 * (chord / 2.0).min(1.0).asin().to_degrees()
}

/// One face's best and second-best bins, ordered by gap then face index: the
/// order of the "smallest gaps" list.
struct GapRow {
    gap: f64,
    face: usize,
    best: usize,
    best_dot: f64,
    second: usize,
    second_dot: f64,
}

impl PartialEq for GapRow {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for GapRow {}
impl PartialOrd for GapRow {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for GapRow {
    fn cmp(&self, other: &Self) -> Ordering {
        self.gap
            .total_cmp(&other.gap)
            .then(self.face.cmp(&other.face))
    }
}

/// Watches the grid's own faces x bins pass (G1) and keeps what the G1 lists
/// need: every face's bin and the faces with the smallest best-second gap.
/// The grid runs the pass once; this only reads it, so the accumulated sums
/// G2 reports are the ones the grid built its rest poses from.
#[derive(Default)]
pub(crate) struct GridObserver {
    bins: usize,
    face_bin: Vec<u32>,
    gaps: Smallest<GapRow>,
}

impl BinObserver for GridObserver {
    const TRACKS_SECOND: bool = true;

    fn begin(&mut self, faces: usize, bins: usize) {
        self.bins = bins;
        self.face_bin = vec![u32::MAX; faces];
        self.gaps = Smallest::new(SMALLEST_MARGINS);
    }

    fn face(&mut self, face: usize, best: usize, best_dot: f64, second: usize, second_dot: f64) {
        self.face_bin[face] = u32::try_from(best).unwrap_or(u32::MAX);
        if self.bins >= 2 {
            self.gaps.offer(GapRow {
                gap: best_dot - second_dot,
                face,
                best,
                best_dot,
                second,
                second_dot,
            });
        }
    }
}

impl GridObserver {
    /// Joins what the observer saw with what the pass accumulated.
    pub(crate) fn finish(self, bins: RestBins) -> GridCapture {
        GridCapture {
            bins: self.bins,
            face_bin: self.face_bin,
            gaps: self.gaps.into_sorted(),
            rest: bins,
        }
    }
}

/// Everything G1 and G2 need from the grid's binning pass.
pub(crate) struct GridCapture {
    bins: usize,
    /// Every face's bin (`u32::MAX` for a face the pass skips).
    face_bin: Vec<u32>,
    /// The smallest-gap faces, smallest first.
    gaps: Vec<GapRow>,
    rest: RestBins,
}

/// G1 and G2 off the grid's own binning pass ([`GridObserver`], and the
/// accumulated areas and sums the pass returned). Nothing is rescanned. The
/// rest poses are rebuilt from the captured sums by the grid's own rule and
/// compared with the grid's, so the last value says the data describe the
/// grid that was searched.
fn g1_g2(
    table: &OrientFaceTable,
    capture: &GridCapture,
    settings: GridSettings,
    grid: &SweepGrid,
) -> (G1Diagnostics, G2Diagnostics, bool) {
    let GridCapture {
        bins,
        face_bin,
        gaps,
        rest,
    } = capture;
    let RestBins {
        area,
        accumulated,
        order,
    } = rest;
    let areas = table.areas_mm2();

    let mut hasher = Sha256::new();
    for bin in face_bin {
        hasher.update(bin.to_le_bytes());
    }
    let binned_faces = face_bin.iter().filter(|bin| **bin != u32::MAX).count();
    let smallest_gaps = gaps
        .iter()
        .map(|row| G1Face {
            face: row.face,
            area_mm2: areas[row.face],
            best: rest_key(row.best),
            best_dot: row.best_dot,
            second: rest_key(row.second),
            second_dot: row.second_dot,
            gap: row.gap,
        })
        .collect();

    // G2: the pass already ordered the bins (heaviest first, ties by lowest
    // bin index).
    let occupied_bins = order.len();
    let keep = settings.rest_pose_max();

    // The kept bins, enumerated in the order the grid was built in.
    let mut reproduced_poses = Vec::new();
    for bin in &kept_bins(order, keep, grid.rest_order()) {
        if let Ok(direction) = UpDirection::new(accumulated[*bin]) {
            reproduced_poses.push((*bin, direction));
        }
    }
    let reproduced = reproduced_poses.len() == grid.rest_pose_entries().len()
        && reproduced_poses
            .iter()
            .zip(grid.rest_pose_entries())
            .all(|((bin, direction), pose)| *bin == pose.bin && *direction == pose.direction);

    let shown = keep + NEXT_REJECTED + 1;
    let ranked: Vec<G2Row> = order
        .iter()
        .enumerate()
        .take(shown)
        .map(|(rank, bin)| {
            let next = order.get(rank + 1).map(|next| area[*next]);
            let gap_abs = next.map(|next| area[*bin] - next);
            G2Row {
                rank,
                key: rest_key(*bin),
                area_mm2: area[*bin],
                kept: rank < keep,
                gap_to_next_abs: gap_abs,
                gap_to_next_rel: gap_abs.map(|gap| gap / area[*bin]),
                direction: (rank < keep)
                    .then(|| UpDirection::new(accumulated[*bin]).ok().map(UpDirection::get))
                    .flatten(),
            }
        })
        .collect();
    let (cut_gap_abs, cut_gap_rel) = match keep.checked_sub(1).and_then(|last| ranked.get(last)) {
        Some(last) if ranked.get(keep).is_some() => (last.gap_to_next_abs, last.gap_to_next_rel),
        _ => (None, None),
    };
    let min_adjacent_rel_gap = ranked
        .iter()
        .take(keep.saturating_sub(1))
        .filter_map(|row| {
            row.gap_to_next_rel.map(|rel_gap| G2Adjacent {
                rank: row.rank,
                rel_gap,
            })
        })
        .min_by(|left, right| left.rel_gap.total_cmp(&right.rel_gap));

    (
        G1Diagnostics {
            faces: face_bin.len(),
            binned_faces,
            bins: *bins,
            face_to_bin_sha256: hex(hasher),
            smallest_gaps,
        },
        G2Diagnostics {
            occupied_bins,
            rest_pose_max: keep,
            ranked,
            cut_gap_abs,
            cut_gap_rel,
            min_adjacent_rel_gap,
        },
        reproduced,
    )
}

// --------------------------------------------------------------------- G3

fn g3_ref(
    key: CandidateKey,
    id: usize,
    from: [f64; 3],
    other: [f64; 3],
    settings: GridSettings,
) -> G3Ref {
    let angle = angle_deg(from, other);
    G3Ref {
        key,
        id,
        angle_deg: angle,
        margin_deg: angle - settings.dedupe_tolerance_deg(),
        dot_minus_cosine: vec3::dot(from, other) - settings.dedupe_cosine(),
    }
}

/// For every direction the grid tried (kept or dropped): the earliest earlier
/// candidate inside the radius, and the nearest earlier candidate.
fn g3_rows(attempts: &[GridAttempt], settings: GridSettings) -> Vec<G3Row> {
    let cosine = settings.dedupe_cosine();
    let mut kept: Vec<(usize, CandidateKey, [f64; 3])> = Vec::new();
    let mut rows = Vec::with_capacity(attempts.len());
    for attempt in attempts {
        let u = attempt.direction.get();
        let mut first_cover: Option<usize> = None;
        let mut nearest: Option<(usize, f64)> = None;
        for (index, (_, _, v)) in kept.iter().enumerate() {
            let dot = vec3::dot(u, *v);
            if first_cover.is_none() && dot > cosine {
                first_cover = Some(index);
            }
            if nearest.is_none_or(|(_, best)| dot > best) {
                nearest = Some((index, dot));
            }
        }
        let as_ref = |index: usize| {
            let (id, key, v) = kept[index];
            g3_ref(key, id, u, v, settings)
        };
        rows.push(G3Row {
            key: attempt.key,
            kept: attempt.kept.is_some(),
            id: attempt.kept,
            first_cover: first_cover.map(as_ref),
            nearest_earlier: nearest.map(|(index, _)| as_ref(index)),
        });
        if let Some(id) = attempt.kept {
            kept.push((id, attempt.key, u));
        }
    }
    rows
}

// --------------------------------------------------------------------- G4

/// One pair on a bounded "smallest margin" list, ordered by absolute margin.
struct MarginItem {
    abs: f64,
    face: usize,
    pose: usize,
    facing: f64,
    margin: f64,
}

impl PartialEq for MarginItem {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for MarginItem {}
impl PartialOrd for MarginItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for MarginItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.abs
            .total_cmp(&other.abs)
            .then(self.face.cmp(&other.face))
            .then(self.pose.cmp(&other.pose))
    }
}

/// Keeps the `cap` smallest items offered: a bounded max-heap, so a list of
/// every face or pair is never held.
struct Smallest<T: Ord> {
    heap: BinaryHeap<T>,
    cap: usize,
}

impl<T: Ord> Default for Smallest<T> {
    fn default() -> Self {
        Self::new(SMALLEST_MARGINS)
    }
}

impl<T: Ord> Smallest<T> {
    fn new(cap: usize) -> Self {
        Self {
            heap: BinaryHeap::with_capacity(cap.saturating_add(1)),
            cap,
        }
    }

    fn offer(&mut self, item: T) {
        if self.heap.len() < self.cap {
            self.heap.push(item);
        } else if self.heap.peek().is_some_and(|worst| item < *worst) {
            self.heap.pop();
            self.heap.push(item);
        }
    }

    /// The kept items, smallest first.
    fn into_sorted(self) -> Vec<T> {
        self.heap.into_sorted_vec()
    }
}

fn margin_faces(list: Smallest<MarginItem>, poses: &[CandidateKey]) -> Vec<G4Face> {
    list.into_sorted()
        .into_iter()
        .map(|item| G4Face {
            face: item.face,
            pose: poses[item.pose],
            facing: item.facing,
            margin: item.margin,
        })
        .collect()
}

/// The two candidates nearest a direction, found in the scan that reads every
/// dot product anyway: larger dot first, ties to the lower id. This is the
/// order a full sort by `(dot descending, id ascending)` would give, kept
/// without holding or sorting all candidates.
#[derive(Default)]
struct NearestTwo([Option<(usize, f64)>; 2]);

impl NearestTwo {
    /// `true` when `(id, dot)` sorts before `other` (a larger dot, or an equal
    /// one at a lower id).
    fn before(id: usize, dot: f64, other: (usize, f64)) -> bool {
        dot.total_cmp(&other.1).then(other.0.cmp(&id)) == Ordering::Greater
    }

    fn offer(&mut self, id: usize, dot: f64) {
        let [first, second] = &mut self.0;
        if first.is_none_or(|best| Self::before(id, dot, best)) {
            *second = *first;
            *first = Some((id, dot));
        } else if second.is_none_or(|next| Self::before(id, dot, next)) {
            *second = Some((id, dot));
        }
    }

    fn ids(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().flatten().map(|(id, _)| *id)
    }
}

/// The grid's own comparison: a rest pose is a flat face when its flat area
/// outweighs the rest of its side, STRICTLY (`flat > side - flat`).
fn is_flat_pose(flat: f64, side: f64) -> bool {
    flat > side - flat
}

/// How far the pose is from that comparison, relative to its side:
/// `(flat - (side - flat)) / side`. Positive means flat; zero is the boundary.
fn flat_share_margin(flat: f64, side: f64) -> f64 {
    if side > 0.0 {
        (flat - (side - flat)) / side
    } else {
        0.0
    }
}

/// G4: the flat-face test, per rest pose and per face, replayed with the
/// grid's own comparison (`flat > side - flat`, strict; a face counts toward a
/// side when `facing > 0`, and toward its flat part when `facing > cos(tol)`).
fn g4_diagnostics(table: &OrientFaceTable, settings: GridSettings, grid: &SweepGrid) -> G4Diagnostics {
    let cosine = settings.dedupe_cosine();
    let rest = grid.rest_pose_entries();
    let pose_keys: Vec<CandidateKey> = rest.iter().map(|pose| rest_key(pose.bin)).collect();
    let sides: Vec<[f64; 3]> = rest
        .iter()
        .map(|pose| pose.direction.antipode().get())
        .collect();
    let mut flat = vec![0.0_f64; sides.len()];
    let mut side = vec![0.0_f64; sides.len()];
    let mut flat_faces = vec![0_usize; sides.len()];
    let mut hashers: Vec<Sha256> = rest
        .iter()
        .map(|pose| {
            let mut hasher = Sha256::new();
            hasher.update(u32::try_from(pose.bin).unwrap_or(u32::MAX).to_le_bytes());
            hasher
        })
        .collect();
    let mut flat_list = Smallest::new(SMALLEST_MARGINS);
    let mut side_list = Smallest::new(SMALLEST_MARGINS);
    let areas = table.areas_mm2();
    for (face, normal) in table.normals().iter().enumerate() {
        let face_area = areas[face];
        if face_area <= 0.0 || !face_area.is_finite() {
            continue;
        }
        for (pose, n) in sides.iter().enumerate() {
            let facing = vec3::dot(*normal, *n);
            let flat_margin = facing - cosine;
            flat_list.offer(MarginItem {
                abs: flat_margin.abs(),
                face,
                pose,
                facing,
                margin: flat_margin,
            });
            side_list.offer(MarginItem {
                abs: facing.abs(),
                face,
                pose,
                facing,
                margin: facing,
            });
            if facing <= 0.0 {
                continue;
            }
            let projected = face_area * facing;
            side[pose] += projected;
            if facing > cosine {
                flat[pose] += projected;
                flat_faces[pose] += 1;
                hashers[pose].update(u32::try_from(face).unwrap_or(u32::MAX).to_le_bytes());
            }
        }
    }

    let poses: Vec<G4Pose> = hashers
        .into_iter()
        .enumerate()
        .map(|(pose, hasher)| G4Pose {
            key: pose_keys[pose],
            flat_area_mm2: flat[pose],
            side_area_mm2: side[pose],
            margin_rel: flat_share_margin(flat[pose], side[pose]),
            is_flat: is_flat_pose(flat[pose], side[pose]),
            flat_faces: flat_faces[pose],
            flat_set_sha256: hex(hasher),
        })
        .collect();

    let covers = rest
        .iter()
        .zip(&poses)
        .filter(|(_, pose)| pose.is_flat)
        .map(|(rest_pose, pose)| {
            let u = rest_pose.direction.get();
            let mut covering: Option<usize> = None;
            let mut nearest = NearestTwo::default();
            for candidate in grid.candidates() {
                let dot = vec3::dot(u, candidate.direction.get());
                if covering.is_none() && dot > cosine {
                    covering = Some(candidate.id);
                }
                nearest.offer(candidate.id, dot);
            }
            let as_ref = |id: usize| {
                g3_ref(
                    grid.keys()[id],
                    id,
                    u,
                    grid.candidates()[id].direction.get(),
                    settings,
                )
            };
            G4Cover {
                pose: pose.key,
                covering: covering.map(as_ref),
                nearest: nearest.ids().map(as_ref).collect(),
            }
        })
        .collect();

    G4Diagnostics {
        tolerance_deg: settings.dedupe_tolerance_deg(),
        cosine,
        flat_poses: poses.iter().filter(|pose| pose.is_flat).count(),
        poses,
        escorted_ids: grid.flat_face_poses().to_vec(),
        smallest_flat_margins: margin_faces(flat_list, &pose_keys),
        smallest_side_margins: margin_faces(side_list, &pose_keys),
        covers,
    }
}

// ------------------------------------------------------------- perf phase

/// Restores the perf phase it replaced when dropped, on every path.
struct PhaseGuard(geom_kernel::perf::Phase);

impl Drop for PhaseGuard {
    fn drop(&mut self) {
        geom_kernel::perf::set_phase_quiet(self.0);
    }
}

/// Runs `work` in [`geom_kernel::perf::Phase::Diagnostic`], so the diagnostics'
/// time, and the work counters of any production sweep they run, are not the
/// pipeline's own phase's (the one they replaced, `OrientTable` for the build)
/// and the perf sidecar can say what they cost. The phase is restored
/// afterwards. Inert unless `--perf` is on. The two phase changes write no
/// partial sidecar (`set_phase_quiet`): that write is the largest cost in a
/// bracket this short.
pub(crate) fn in_diagnostic_phase<R>(work: impl FnOnce() -> R) -> R {
    let _restore = PhaseGuard(geom_kernel::perf::set_phase_quiet(
        geom_kernel::perf::Phase::Diagnostic,
    ));
    work()
}

// ------------------------------------------- prod-all and extra= (separate)

/// One pose to measure at production pitch.
pub(crate) struct DiagItem {
    pub label: String,
    pub key: Option<CandidateKey>,
    pub candidate: Option<usize>,
    pub direction: UpDirection,
    pub exact_terms: TermsRecord,
}

/// Prices every item at production pitch through `measurer` in ONE batch and
/// reprices its exact-table terms the way the search reprices a finalist
/// (`TermsRecord::repriced` with the measured burden and both scar halves).
pub(crate) fn measure_items(
    measurer: &dyn SupportDemandMeasurer,
    mesh: &Mesh,
    exact_table: &OrientFaceTable,
    weights: ObjectiveWeights,
    items: Vec<DiagItem>,
) -> Vec<DiagnosticScore> {
    if items.is_empty() {
        return Vec::new();
    }
    let _mark = geom_kernel::perf::stage("orient.diag_production");
    let directions: Vec<UpDirection> = items.iter().map(|item| item.direction).collect();
    let measured = measurer.measure_support_demand(
        mesh,
        exact_table,
        &directions,
        DemandFidelity::Production,
    );
    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let (production_terms, demand, error) = match measured.get(index) {
                Some(Ok(measured)) => (
                    Some(item.exact_terms.repriced(
                        weights,
                        measured.support_area_cm2,
                        item.exact_terms
                            .peel_force_n
                            .low_pressure_low_exponent_n
                            .get(),
                        measured.landing_scar_raw_mm2,
                        measured.tip_scar_raw_mm2,
                    )),
                    Some(DiagnosticDemand::from(measured)),
                    None,
                ),
                Some(Err(error)) => (None, None, Some(error.clone())),
                None => (
                    None,
                    None,
                    Some("the diagnostic measurer returned fewer measurements than poses".to_owned()),
                ),
            };
            DiagnosticScore {
                label: item.label,
                key: item.key,
                candidate: item.candidate,
                direction: item.direction.get(),
                exact_terms: Some(item.exact_terms),
                production_terms,
                demand,
                error,
            }
        })
        .collect()
}

/// The `prod-all` and `extra=` measurements, through the SEPARATE measurer.
/// Runs after the record is final; nothing it returns reaches the ranking.
pub(crate) fn production_diagnostics(
    inputs: &DiagInputs<'_>,
    notes: &mut Vec<String>,
) -> (Option<Vec<DiagnosticScore>>, Vec<DiagnosticScore>) {
    let config = inputs.config;
    if !config.prod_all && config.extra.is_empty() {
        return (None, Vec::new());
    }
    // The extras are scored on the exact table first: that needs no
    // measurer, so a run without one still reports what the table says.
    let (extra_items, extra_notes) = extra_items(inputs);
    notes.extend(extra_notes);
    let Some(measurer) = inputs
        .demand
        .and_then(|demand| demand.diagnostic_measurer())
    else {
        notes.push(
            "prod-all/extra= asked for, but there is no separate non-retaining measurer; \
             no production sweep was run"
                .to_owned(),
        );
        let extras = extra_items.into_iter().map(unmeasured_score).collect();
        return (config.prod_all.then(Vec::new), extras);
    };
    // One batch: the admitted poses first (prod-all), then the extras.
    let mut items = if config.prod_all {
        admitted_items(inputs)
    } else {
        Vec::new()
    };
    let prod_all_count = items.len();
    items.extend(extra_items);
    let mut rows = measure_items(
        measurer.as_ref(),
        inputs.mesh,
        inputs.exact_table,
        inputs.request.weights,
        items,
    );
    let extras = rows.split_off(prod_all_count.min(rows.len()));
    (config.prod_all.then_some(rows), extras)
}

/// Why a pose carries no production measurement when there was no measurer to
/// ask.
const MEASUREMENT_UNAVAILABLE: &str =
    "production measurement unavailable: no separate non-retaining measurer";

/// A pose with its exact-table terms and no production measurement.
fn unmeasured_score(item: DiagItem) -> DiagnosticScore {
    DiagnosticScore {
        label: item.label,
        key: item.key,
        candidate: item.candidate,
        direction: item.direction.get(),
        exact_terms: Some(item.exact_terms),
        production_terms: None,
        demand: None,
        error: Some(MEASUREMENT_UNAVAILABLE.to_owned()),
    }
}

/// Every commit-admitted pose with its exact-table terms.
fn admitted_items(inputs: &DiagInputs<'_>) -> Vec<DiagItem> {
    let keys = inputs.grid.keys();
    inputs
        .snapshots
        .exact_table
        .iter()
        .map(|entry| DiagItem {
            label: keys[entry.candidate].to_string(),
            key: Some(keys[entry.candidate]),
            candidate: Some(entry.candidate),
            direction: inputs.grid.candidates()[entry.candidate].direction,
            exact_terms: entry.terms,
        })
        .collect()
}

/// The `extra=` directions scored on the exact table (one batched call).
fn extra_items(inputs: &DiagInputs<'_>) -> (Vec<DiagItem>, Vec<String>) {
    let extra = &inputs.config.extra;
    if extra.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let mut notes = Vec::new();
    let scored = match score_candidates(
        inputs.exact_table,
        extra,
        inputs.request.weights,
        inputs.request.limits,
    ) {
        Ok(scored) => scored,
        Err(error) => {
            notes.push(format!("extra= directions could not be scored: {error}"));
            return (Vec::new(), notes);
        }
    };
    let mut items = Vec::with_capacity(extra.len());
    for (index, (direction, outcome)) in extra.iter().zip(scored.outcomes()).enumerate() {
        match outcome {
            CandidateOutcome::Scored(terms) => match TermsRecord::new(*terms) {
                Ok(exact_terms) => items.push(DiagItem {
                    label: format!("extra:{index}"),
                    key: None,
                    candidate: None,
                    direction: *direction,
                    exact_terms,
                }),
                Err(error) => notes.push(format!("extra:{index}: {error}")),
            },
            CandidateOutcome::Refused(refusal) => notes.push(format!(
                "extra:{index} was refused: {}",
                crate::search::describe(refusal.reason)
            )),
        }
    }
    (items, notes)
}

// ------------------------------------------------------------------- output

/// What goes in `<model>.orient.json`.
#[derive(Serialize)]
struct OrientJson<'a> {
    record: &'a DenseSweepRecord,
    diagnostics: &'a SweepDiagnostics,
}

/// The `orient.json` bytes: the search record and its diagnostics.
///
/// # Errors
///
/// A serialization failure from `serde_json`.
pub fn orient_json(
    record: &DenseSweepRecord,
    diagnostics: &SweepDiagnostics,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&OrientJson {
        record,
        diagnostics,
    })
}

/// Writes `<perf-dir>/<model>.orient.json` while a perf model is in flight.
/// A failure is a warning: a diagnostic never fails a search.
///
/// Serializing and writing are diagnostic work; the caller runs this inside
/// [`in_diagnostic_phase`].
pub(crate) fn write_orient_json(record: &DenseSweepRecord, diagnostics: &SweepDiagnostics) {
    let bytes = match orient_json(record, diagnostics) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("WARNING: orient.json not written: {error}");
            return;
        }
    };
    if let Err(error) = geom_kernel::perf::write_side_file("orient.json", &bytes) {
        eprintln!("WARNING: orient.json not written: {error}");
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use plinth_mesh::{make_overhang_table, make_recessed_disc, Mesh};

    use super::*;
    use crate::grid::fibonacci_directions;
    use crate::search::{escort, search_core, search_with_diagnostics};

    fn request() -> SweepRequest {
        SweepRequest::registered().expect("the registered sweep configuration")
    }

    // ------------------------------------------------------------ fixtures

    fn cube_from(map: impl Fn([f64; 3]) -> [f64; 3]) -> Mesh {
        let corners = [
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
            [10.0, 0.0, 10.0],
            [10.0, 10.0, 10.0],
            [0.0, 10.0, 10.0],
        ];
        let tris = vec![
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
        Mesh::new(corners.into_iter().map(map).collect(), tris).expect("a cube")
    }

    /// The cube turned so its +Z axis points along `m` (a proper rotation).
    fn cube_pointing(m: [f64; 3]) -> Mesh {
        let helper = if m[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let a = vec3::normalize(vec3::cross(m, helper)).expect("a basis vector");
        let b = vec3::cross(m, a);
        cube_from(move |v| {
            [
                v[0] * a[0] + v[1] * b[0] + v[2] * m[0],
                v[0] * a[1] + v[1] * b[1] + v[2] * m[1],
                v[0] * a[2] + v[1] * b[2] + v[2] * m[2],
            ]
        })
    }

    /// A closed, outward-wound UV sphere with exact poles.
    fn uv_sphere(radius: f64, latitudes: usize, longitudes: usize) -> Mesh {
        let index = |value: usize| u32::try_from(value).expect("a small mesh");
        let mut verts = vec![[0.0, 0.0, radius]];
        for ring in 1..latitudes {
            let phi = std::f64::consts::PI * ring as f64 / latitudes as f64;
            let (sin_phi, cos_phi) = phi.sin_cos();
            for step in 0..longitudes {
                let theta = 2.0 * std::f64::consts::PI * step as f64 / longitudes as f64;
                let (sin_theta, cos_theta) = theta.sin_cos();
                verts.push([
                    radius * sin_phi * cos_theta,
                    radius * sin_phi * sin_theta,
                    radius * cos_phi,
                ]);
            }
        }
        let south = index(verts.len());
        verts.push([0.0, 0.0, -radius]);
        let ring_vertex =
            |ring: usize, step: usize| index(1 + (ring - 1) * longitudes + step % longitudes);
        let mut tris = Vec::new();
        for step in 0..longitudes {
            tris.push([0, ring_vertex(1, step), ring_vertex(1, step + 1)]);
        }
        for ring in 1..latitudes - 1 {
            for step in 0..longitudes {
                let upper = [ring_vertex(ring, step), ring_vertex(ring, step + 1)];
                let lower = [ring_vertex(ring + 1, step), ring_vertex(ring + 1, step + 1)];
                tris.push([upper[0], lower[0], lower[1]]);
                tris.push([upper[0], lower[1], upper[1]]);
            }
        }
        for step in 0..longitudes {
            tris.push([
                south,
                ring_vertex(latitudes - 1, step + 1),
                ring_vertex(latitudes - 1, step),
            ]);
        }
        Mesh::new(verts, tris).expect("a UV sphere")
    }

    fn table_and_grid(mesh: &Mesh) -> (OrientFaceTable, SweepGrid, GridSettings) {
        let settings = GridSettings::registered().expect("registered grid settings");
        let table = OrientFaceTable::build(mesh, "test").expect("a closed mesh");
        let grid = SweepGrid::build(&table, settings).expect("a grid");
        (table, grid, settings)
    }

    /// The grid built with the diagnostics' observer on its binning pass, as
    /// `search` builds it.
    fn observed(mesh: &Mesh) -> (OrientFaceTable, SweepGrid, GridSettings, GridCapture) {
        let settings = GridSettings::registered().expect("registered grid settings");
        let table = OrientFaceTable::build(mesh, "test").expect("a closed mesh");
        let mut observer = GridObserver::default();
        let (grid, bins) =
            SweepGrid::build_observed(&table, settings, RestOrder::ByArea, &mut observer)
                .expect("a grid");
        (table, grid, settings, observer.finish(bins))
    }

    // --------------------------------------------------- switches and keys

    #[test]
    fn the_switch_text_parses_and_warns_on_what_it_cannot_read() {
        assert_eq!(Switches::parse(""), Switches::default());
        let flags = Switches::parse("digest,prod-all");
        assert!(flags.digest && flags.prod_all && !flags.off && flags.warnings.is_empty());
        let both = Switches::parse("prod-all,extra=0,0,2;1,0,0");
        assert!(both.prod_all);
        assert_eq!(both.extra.len(), 2);
        assert_eq!(both.extra[0], UpDirection::MODEL_UP, "directions are normalized");
        assert!(both.warnings.is_empty());
        let bad = Switches::parse("bogus extra=1,2;a,b,c;0,0,0");
        assert_eq!(bad.warnings.len(), 4, "{:?}", bad.warnings);
        assert!(bad.extra.is_empty());

        assert!(Switches::parse("off").config(true).is_none());
        assert!(Switches::default().config(false).is_none(), "no --perf, nowhere to write");
        let on = Switches::parse("prod-all,extra=1,0,0").config(true).expect("on");
        assert!(on.prod_all && on.extra.len() == 1);
        assert!(Switches::default().config(true).is_some(), "--perf alone turns it on");
    }

    #[test]
    fn a_key_survives_text_and_json() {
        for key in [
            CandidateKey::Legacy(12),
            CandidateKey::Rest(2047),
            CandidateKey::Fib(511),
        ] {
            assert_eq!(key.to_string().parse::<CandidateKey>(), Ok(key));
            let json = serde_json::to_string(&key).expect("serialize");
            assert_eq!(json, format!("\"{key}\""));
            assert_eq!(serde_json::from_str::<CandidateKey>(&json).expect("parse"), key);
        }
        assert!("fib:x".parse::<CandidateKey>().is_err());
        assert!("lattice:3".parse::<CandidateKey>().is_err());
    }

    #[test]
    fn every_candidate_key_names_the_direction_it_came_from() {
        let (table, grid, _) = table_and_grid(&uv_sphere(10.0, 20, 40));
        let settings = GridSettings::registered().expect("settings");
        let lattice = fibonacci_directions(settings.fibonacci_count()).expect("lattice");
        let reference = fibonacci_directions(settings.reference_bins()).expect("reference");
        assert_eq!(grid.keys().len(), grid.candidates().len());
        let mut seen = BTreeSet::new();
        for (candidate, key) in grid.candidates().iter().zip(grid.keys()) {
            assert!(seen.insert(*key), "keys are unique: {key}");
            match key {
                CandidateKey::Legacy(k) => {
                    assert_eq!(candidate.block, CandidateBlock::Legacy);
                    assert_eq!(usize::from(*k), candidate.id, "legacy poses are never dropped");
                }
                CandidateKey::Fib(i) => {
                    assert_eq!(candidate.block, CandidateBlock::Fibonacci);
                    assert_eq!(
                        candidate.direction,
                        lattice[usize::try_from(*i).expect("index")],
                        "{key}"
                    );
                }
                CandidateKey::Rest(bin) => {
                    assert_eq!(candidate.block, CandidateBlock::RestPose);
                    let pose = grid
                        .rest_pose_entries()
                        .iter()
                        .find(|pose| pose.bin == usize::try_from(*bin).expect("bin"))
                        .expect("a rest pose with this bin");
                    assert_eq!(pose.direction, candidate.direction);
                    assert!(usize::try_from(*bin).expect("bin") < reference.len());
                }
            }
        }
        // A key is a name, not a position: attempts keep the dropped ones too.
        assert_eq!(
            grid.attempts().len(),
            grid.candidates().len() + grid.duplicates_dropped()
        );
        assert_eq!(table.face_count(), 1520);
    }

    // ---------------------------------------------------------------- G1

    #[test]
    fn g1_records_a_face_that_sits_on_a_bin_boundary() {
        let settings = GridSettings::registered().expect("settings");
        let reference = fibonacci_directions(settings.reference_bins()).expect("reference");
        let (r0, r1) = (reference[0].get(), reference[1].get());
        // A face whose DOWN direction is the bisector of bins 0 and 1 is a tie.
        let bisector = vec3::normalize(vec3::add(r0, r1)).expect("bisector");
        let mesh = cube_pointing([-bisector[0], -bisector[1], -bisector[2]]);
        let (table, grid, settings, capture) = observed(&mesh);
        let (g1, _, reproduced) = g1_g2(&table, &capture, settings, &grid);
        assert!(reproduced, "the captured pass rebuilds the grid's rest poses");
        let tied = &g1.smallest_gaps[0];
        assert!(tied.gap < 1e-12, "a bisector face is within rounding of a tie: {}", tied.gap);
        let pair: BTreeSet<CandidateKey> = [tied.best, tied.second].into_iter().collect();
        assert_eq!(
            pair,
            [CandidateKey::Rest(0), CandidateKey::Rest(1)].into_iter().collect(),
            "the margin names BOTH bins"
        );
        assert!(g1.smallest_gaps.windows(2).all(|w| w[0].gap <= w[1].gap), "ascending");
        assert!(g1.smallest_gaps.iter().all(|row| row.gap >= 0.0));
        assert_eq!(g1.faces, 12);
        assert_eq!(g1.binned_faces, 12);
        assert_eq!(g1.face_to_bin_sha256.len(), 64);
    }

    #[test]
    fn the_g1_hash_moves_when_any_face_changes_bin() {
        let (table_a, grid_a, settings, capture_a) = observed(&cube_pointing([0.0, 0.0, 1.0]));
        let (table_b, grid_b, _, capture_b) =
            observed(&cube_pointing([0.0, 0.3, 0.953_939_201_416_945_7]));
        let (table_c, grid_c, _, capture_c) = observed(&cube_pointing([0.0, 0.0, 1.0]));
        let a = g1_g2(&table_a, &capture_a, settings, &grid_a).0;
        let b = g1_g2(&table_b, &capture_b, settings, &grid_b).0;
        let again = g1_g2(&table_c, &capture_c, settings, &grid_c).0;
        assert_eq!(a.face_to_bin_sha256, again.face_to_bin_sha256, "reproducible");
        assert_ne!(a.face_to_bin_sha256, b.face_to_bin_sha256);
    }

    /// The bounded top-K the observer keeps is the K smallest gaps of the full
    /// list, in the same order, and every face's bin is what a plain scan over
    /// the reference lattice picks (the lowest index on a tie).
    #[test]
    fn g1_is_the_full_scan_truncated_to_the_smallest_gaps() {
        let (table, grid, settings, capture) = observed(&uv_sphere(10.0, 20, 40));
        assert!(table.face_count() > SMALLEST_MARGINS, "the list must actually be cut");
        let (g1, _, reproduced) = g1_g2(&table, &capture, settings, &grid);
        assert!(reproduced);
        let reference = fibonacci_directions(settings.reference_bins()).expect("reference");
        let mut rows = Vec::new();
        let mut hasher = Sha256::new();
        for (face, normal) in table.normals().iter().enumerate() {
            let down = [-normal[0], -normal[1], -normal[2]];
            let mut dots: Vec<(usize, f64)> = reference
                .iter()
                .enumerate()
                .map(|(bin, r)| (bin, vec3::dot(down, r.get())))
                .collect();
            dots.sort_by(|l, r| r.1.total_cmp(&l.1).then(l.0.cmp(&r.0)));
            let ((best, best_dot), (second, second_dot)) = (dots[0], dots[1]);
            hasher.update(u32::try_from(best).expect("bin").to_le_bytes());
            rows.push((best_dot - second_dot, face, best, second));
        }
        assert_eq!(g1.face_to_bin_sha256, hex(hasher), "every face's bin");
        rows.sort_by(|l, r| l.0.total_cmp(&r.0).then(l.1.cmp(&r.1)));
        assert_eq!(g1.smallest_gaps.len(), SMALLEST_MARGINS);
        for (got, want) in g1.smallest_gaps.iter().zip(&rows) {
            assert_eq!(got.gap.to_bits(), want.0.to_bits());
            assert_eq!(got.face, want.1);
            assert_eq!((got.best, got.second), (rest_key(want.2), rest_key(want.3)));
        }
    }

    /// The bounded heap keeps the K smallest in order for any offer order,
    /// ties included.
    #[test]
    fn the_smallest_list_is_a_bounded_sorted_selection() {
        let values: Vec<usize> = (0..200).map(|i| (i * 37 + 11) % 50).collect();
        for cap in [0, 1, 7, 50, 500] {
            let mut list = Smallest::new(cap);
            for (index, value) in values.iter().enumerate() {
                list.offer((*value, index));
                assert!(list.heap.len() <= cap, "never holds more than {cap}");
            }
            let mut want: Vec<(usize, usize)> = values.iter().copied().zip(0..).collect();
            want.sort_unstable();
            want.truncate(cap);
            assert_eq!(list.into_sorted(), want, "cap {cap}");
        }
    }

    // ---------------------------------------------------------------- G2

    #[test]
    fn g2_names_the_bins_either_side_of_the_cut_and_their_area_gap() {
        let (table, grid, settings, capture) = observed(&uv_sphere(10.0, 20, 40));
        let (_, g2, reproduced) = g1_g2(&table, &capture, settings, &grid);
        assert!(reproduced);
        assert_eq!(g2.rest_pose_max, 128);
        assert!(g2.occupied_bins > 137, "the sphere fills more bins than the cut keeps");
        assert_eq!(g2.ranked.len(), 128 + NEXT_REJECTED + 1);
        let (last_kept, first_rejected) = (&g2.ranked[127], &g2.ranked[128]);
        assert!(last_kept.kept && !first_rejected.kept);
        assert!(last_kept.area_mm2 >= first_rejected.area_mm2, "sorted by area");
        let gap = last_kept.area_mm2 - first_rejected.area_mm2;
        assert_eq!(g2.cut_gap_abs.map(f64::to_bits), Some(gap.to_bits()));
        let rel = g2.cut_gap_rel.expect("a relative gap");
        assert!((0.0..=1.0).contains(&rel));
        assert!(g2.ranked.windows(2).all(|w| w[0].area_mm2 >= w[1].area_mm2));
        let adjacent = g2.min_adjacent_rel_gap.expect("an adjacent gap");
        assert!(adjacent.rank < 127 && adjacent.rel_gap >= 0.0);
        assert!(g2.ranked[..128].iter().all(|row| row.direction.is_some()));
        assert!(g2.ranked[128..].iter().all(|row| row.direction.is_none()));
    }

    // ---------------------------------------------------------------- G3

    /// The identity direction, then a second direction whose dot product with
    /// it is EXACTLY `z` (built unnormalized, so no rounding intervenes).
    fn pair_with_dot(z: f64, x: f64) -> Vec<GridAttempt> {
        vec![
            GridAttempt {
                key: CandidateKey::Legacy(0),
                direction: UpDirection::MODEL_UP,
                kept: Some(0),
            },
            GridAttempt {
                key: CandidateKey::Fib(7),
                direction: UpDirection::raw([x, 0.0, z]),
                kept: None,
            },
        ]
    }

    /// The dedupe radius, one ulp each side. The grid drops a direction only
    /// when `dot > cosine` (strict), and the G3 margin must say the same.
    #[test]
    fn g3_reads_the_margin_at_the_dedupe_radius_one_ulp_each_side() {
        let settings = GridSettings::registered().expect("settings");
        let cosine = settings.dedupe_cosine();
        let x = (1.0 - cosine * cosine).sqrt();
        for (z, covered) in [
            (cosine, false),
            (f64::from_bits(cosine.to_bits() + 1), true),
            (f64::from_bits(cosine.to_bits() - 1), false),
        ] {
            let attempts = pair_with_dot(z, x);
            let rows = g3_rows(&attempts, settings);
            assert_eq!(rows.len(), 2);
            assert!(rows[0].nearest_earlier.is_none() && rows[0].first_cover.is_none());
            let probe = &rows[1];
            let nearest = probe.nearest_earlier.as_ref().expect("an earlier candidate");
            assert_eq!(probe.first_cover.is_some(), covered, "z = {z:e}");
            // The exact test is `dot > cosine`: the dot margin is zero ON the boundary.
            let expected = z - cosine;
            assert_eq!(nearest.dot_minus_cosine.to_bits(), expected.to_bits());
            assert_eq!(nearest.dot_minus_cosine > 0.0, covered);
            // The angle margin is the same fact in degrees, read to within rounding.
            assert!(nearest.margin_deg.abs() < 1e-6, "z = {z:e}: {}", nearest.margin_deg);
        }
    }

    #[test]
    fn g3_rows_cover_every_attempt_and_agree_with_the_grid() {
        let cube = cube_pointing([0.0, 0.0, 1.0]);
        let sphere = uv_sphere(10.0, 20, 40);
        for (name, mesh) in [("cube", cube), ("sphere", sphere)] {
            let (_, grid, settings) = table_and_grid(&mesh);
            let rows = g3_rows(grid.attempts(), settings);
            assert_eq!(rows.len(), grid.attempts().len(), "{name}");
            let mut dropped = 0;
            for (row, attempt) in rows.iter().zip(grid.attempts()) {
                assert_eq!(row.key, attempt.key);
                assert_eq!(row.id, attempt.kept);
                assert_eq!(row.kept, attempt.kept.is_some());
                if row.kept {
                    assert!(row.first_cover.is_none());
                    if let Some(nearest) = &row.nearest_earlier {
                        assert!(nearest.dot_minus_cosine <= 0.0, "{name} {row:?}");
                        assert!(nearest.margin_deg >= -1e-9, "{name} {row:?}");
                    }
                } else {
                    dropped += 1;
                    let cover = row.first_cover.as_ref().expect("a dropped direction has a cover");
                    assert!(cover.dot_minus_cosine > 0.0 && cover.margin_deg <= 1e-9, "{name} {row:?}");
                }
            }
            assert_eq!(dropped, grid.duplicates_dropped(), "{name}");
            if name == "cube" {
                // The +Z rest pose of the cube IS the identity pose: dropped, and
                // covered by the legacy candidate that came first.
                let pole = rows
                    .iter()
                    .find(|row| !row.kept && row.first_cover.as_ref().is_some_and(|c| c.id == 0))
                    .expect("the cube's +Z rest pose duplicates identity");
                assert!(matches!(pole.key, CandidateKey::Rest(_)));
            }
        }
    }

    // ---------------------------------------------------------------- G4

    #[test]
    fn the_flat_share_boundary_is_strict() {
        assert!(!is_flat_pose(1.0, 2.0), "50 % flat is not flat");
        assert_eq!(flat_share_margin(1.0, 2.0).to_bits(), 0.0_f64.to_bits());
        let above = f64::from_bits(1.0_f64.to_bits() + 1);
        assert!(is_flat_pose(above, 2.0));
        assert!(flat_share_margin(above, 2.0) > 0.0);
        let below = f64::from_bits(1.0_f64.to_bits() - 1);
        assert!(!is_flat_pose(below, 2.0));
        assert!(flat_share_margin(below, 2.0) < 0.0);
        assert_eq!(flat_share_margin(0.0, 0.0).to_bits(), 0.0_f64.to_bits());
    }

    #[test]
    fn g4_reproduces_the_grids_flat_poses_and_lists_the_nearest_faces() {
        let mesh = make_recessed_disc(24.0, 25.0, 3.5, 2.5, 22.5, 128).expect("a disc");
        let (table, grid, settings) = table_and_grid(&mesh);
        let g4 = g4_diagnostics(&table, settings, &grid);
        assert_eq!(g4.flat_poses, 2, "a disc has one flat face per side");
        assert_eq!(g4.flat_poses, grid.flat_face_count());
        assert_eq!(g4.escorted_ids, grid.flat_face_poses());
        assert_eq!(g4.poses.len(), grid.rest_pose_entries().len());
        assert_eq!(g4.covers.len(), g4.flat_poses);
        for cover in &g4.covers {
            let covering = cover.covering.as_ref().expect("a flat pose is covered");
            assert!(g4.escorted_ids.contains(&covering.id), "{cover:?}");
            assert!(covering.dot_minus_cosine > 0.0);
            assert_eq!(cover.nearest.len(), 2);
            assert!(cover.nearest[0].angle_deg <= cover.nearest[1].angle_deg);
        }
        for pose in g4.poses.iter().filter(|pose| pose.is_flat) {
            assert!(pose.margin_rel > 0.0 && pose.flat_faces > 0, "{pose:?}");
            assert_eq!(pose.flat_set_sha256.len(), 64);
        }
        for list in [&g4.smallest_flat_margins, &g4.smallest_side_margins] {
            assert!(!list.is_empty() && list.len() <= SMALLEST_MARGINS);
            assert!(list.windows(2).all(|w| w[0].margin.abs() <= w[1].margin.abs()));
        }
        for face in &g4.smallest_flat_margins {
            assert_eq!(face.margin.to_bits(), (face.facing - g4.cosine).to_bits());
        }
        for face in &g4.smallest_side_margins {
            assert_eq!(face.margin.to_bits(), face.facing.to_bits());
        }
    }

    // ------------------------------------------------------------ escort

    #[test]
    fn the_escort_replay_agrees_with_the_search_s_own_escort() {
        let keys: Vec<CandidateKey> = (0..10).map(CandidateKey::Fib).collect();
        let promoted = [0_usize, 5, 9];
        let legacy = [0_usize, 1, 2];
        let flat = [2_usize, 7];
        let replay = escort_decisions(&promoted, &legacy, &flat, &keys);
        let by_right: Vec<usize> = legacy.iter().chain(&flat).copied().collect();
        let (set, added) = escort(&promoted, &by_right);
        assert_eq!(replay.added, added);
        assert_eq!(replay.admitted.iter().map(|k| k.id).collect::<Vec<_>>(), set);
        let row = |source: &str, id: usize| {
            replay
                .by_right
                .iter()
                .find(|row| row.source == source && row.id == id)
                .cloned()
                .expect("a row")
        };
        assert!(row("legacy", 0).already_promoted && !row("legacy", 0).added);
        assert!(!row("legacy", 1).already_promoted && row("legacy", 1).added);
        assert!(row("legacy", 2).added);
        assert!(
            !row("flat_face", 2).added && !row("flat_face", 2).already_promoted,
            "a flat pose the legacy block already carried adds nothing"
        );
        assert!(row("flat_face", 7).added);
        assert_eq!(replay.by_right[1].key, CandidateKey::Fib(1));
    }

    // ------------------------------------------------- the whole search

    fn measured(support_area_cm2: f64, fidelity: DemandFidelity) -> MeasuredSupportDemand {
        MeasuredSupportDemand {
            support_area_cm2,
            struts: 1,
            braces: 0,
            masts: 0,
            total_scar_mm2: 0.25,
            achieved_pitch_mm: 0.05,
            layers: 10,
            residual_demand_cells: 0,
            uncarried_load_mm2: 0.0,
            model_landings: 0,
            landing_scar_raw_mm2: None,
            tip_scar_raw_mm2: None,
            fidelity,
        }
    }

    type Calls = Rc<RefCell<Vec<(DemandFidelity, usize)>>>;

    /// The search's own measurer: a burden that depends on the direction, plus
    /// a SEPARATE diagnostic measurer that is poisoned (a huge negative burden),
    /// so anything that leaked from it into the ranking would show.
    struct Primary {
        calls: Calls,
        diagnostic_calls: Calls,
        has_diagnostic: bool,
    }

    impl Primary {
        fn new(has_diagnostic: bool) -> Self {
            Self {
                calls: Rc::default(),
                diagnostic_calls: Rc::default(),
                has_diagnostic,
            }
        }
    }

    fn primary_area(direction: &UpDirection) -> f64 {
        let u = direction.get();
        0.5 + 0.25 * u[0].abs() + 0.125 * u[1].abs()
    }

    fn poisoned_area(direction: &UpDirection) -> f64 {
        -7.0 + direction.get()[2]
    }

    impl SupportDemandMeasurer for Primary {
        fn measure_support_demand(
            &self,
            _mesh: &Mesh,
            _table: &OrientFaceTable,
            candidates: &[UpDirection],
            fidelity: DemandFidelity,
        ) -> Vec<Result<MeasuredSupportDemand, String>> {
            self.calls.borrow_mut().push((fidelity, candidates.len()));
            candidates
                .iter()
                .map(|candidate| Ok(measured(primary_area(candidate), fidelity)))
                .collect()
        }

        fn diagnostic_measurer(&self) -> Option<Box<dyn SupportDemandMeasurer>> {
            self.has_diagnostic.then(|| {
                Box::new(Poisoned {
                    calls: Rc::clone(&self.diagnostic_calls),
                }) as Box<dyn SupportDemandMeasurer>
            })
        }
    }

    struct Poisoned {
        calls: Calls,
    }

    impl SupportDemandMeasurer for Poisoned {
        fn measure_support_demand(
            &self,
            _mesh: &Mesh,
            _table: &OrientFaceTable,
            candidates: &[UpDirection],
            fidelity: DemandFidelity,
        ) -> Vec<Result<MeasuredSupportDemand, String>> {
            self.calls.borrow_mut().push((fidelity, candidates.len()));
            candidates
                .iter()
                .map(|candidate| Ok(measured(poisoned_area(candidate), fidelity)))
                .collect()
        }
    }

    fn config(extra: Vec<UpDirection>) -> DiagnosticsConfig {
        DiagnosticsConfig {
            prod_all: true,
            extra,
            sabotage_feed_rerank: false,
        }
    }

    fn drained(calls: &Calls) -> Vec<(DemandFidelity, usize)> {
        std::mem::take(&mut *calls.borrow_mut())
    }

    #[test]
    fn the_diagnostics_never_change_the_record_the_digest_or_the_primary_measurer_s_calls() {
        let mesh = make_overhang_table().expect("the fixture");
        let request = request();

        let off = Primary::new(true);
        let (baseline, none) =
            search_core(&mesh, &request, Some(&off), None, false, &Ablation::default())
                .expect("search");
        assert!(none.is_none(), "no config, no diagnostics");
        let baseline_calls = drained(&off.calls);
        assert!(drained(&off.diagnostic_calls).is_empty(), "off means off");

        let on = Primary::new(true);
        let extra = UpDirection::new([1.0, 0.0, 1.0]).expect("a direction");
        let (with, diagnostics) =
            search_with_diagnostics(&mesh, &request, Some(&on), &config(vec![extra])).expect("search");

        assert_eq!(with.record(), baseline.record());
        assert_eq!(with.record().search_digest, baseline.record().search_digest);
        assert_eq!(
            serde_json::to_vec(with.record()).expect("json"),
            serde_json::to_vec(baseline.record()).expect("json"),
            "the bytes the digest hashes"
        );
        assert_eq!(with.winner(), baseline.winner());
        assert_eq!(
            drained(&on.calls),
            baseline_calls,
            "the diagnostics added no call to the measurer the search uses"
        );

        // Path-hit witness: the SEPARATE measurer ran, once, at production,
        // over every admitted pose plus the one extra.
        let commit = &baseline.record().tiers[2];
        assert_eq!(
            drained(&on.diagnostic_calls),
            vec![(DemandFidelity::Production, commit.ranking.len() + 1)]
        );
        let rows = diagnostics.production_all.as_ref().expect("prod-all rows");
        assert_eq!(rows.len(), commit.ranking.len());
        for row in rows {
            let demand = row.demand.as_ref().expect("measured");
            let direction = UpDirection::new(row.direction).expect("direction");
            assert_eq!(
                demand.support_area_cm2.to_bits(),
                poisoned_area(&direction).to_bits(),
                "these numbers came from the diagnostic measurer"
            );
            let terms = row.production_terms.as_ref().expect("repriced");
            assert_eq!(terms.support_area_cm2.get().to_bits(), poisoned_area(&direction).to_bits());
        }
        assert_eq!(diagnostics.extras.len(), 1);
        assert_eq!(diagnostics.extras[0].label, "extra:0");
        assert!(diagnostics.extras[0].exact_terms.is_some() && diagnostics.extras[0].production_terms.is_some());
        let winner_row = rows
            .iter()
            .find(|row| row.candidate == Some(baseline.record().winner.candidate))
            .expect("the winner is among the admitted");
        assert_ne!(
            winner_row.production_terms.expect("terms").score.get().to_bits(),
            baseline.record().winner.terms.score.get().to_bits(),
            "the diagnostic price of the winner is not the price that chose it"
        );
    }

    #[test]
    fn sabotage_feeding_a_diagnostic_score_into_the_rerank_is_caught() {
        let mesh = make_overhang_table().expect("the fixture");
        let request = request();
        let clean = Primary::new(true);
        let (baseline, _) =
            search_core(&mesh, &request, Some(&clean), None, false, &Ablation::default())
                .expect("search");

        let leaky = Primary::new(true);
        let mut sabotaged = config(Vec::new());
        sabotaged.sabotage_feed_rerank = true;
        let (leaked, _) =
            search_with_diagnostics(&mesh, &request, Some(&leaky), &sabotaged).expect("search");
        assert_ne!(
            leaked.record().search_digest,
            baseline.record().search_digest,
            "the equality the other test asserts has teeth: a leak moves the digest"
        );
        assert_ne!(leaked.record(), baseline.record());
    }

    #[test]
    fn without_a_separate_measurer_prod_all_says_so_and_runs_nothing() {
        let mesh = make_overhang_table().expect("the fixture");
        let plain = Primary::new(false);
        let (outcome, diagnostics) =
            search_with_diagnostics(&mesh, &request(), Some(&plain), &config(Vec::new())).expect("search");
        assert_eq!(diagnostics.production_all, Some(Vec::new()));
        assert!(diagnostics.extras.is_empty());
        assert!(
            diagnostics.notes.iter().any(|note| note.contains("no separate non-retaining measurer")),
            "{:?}",
            diagnostics.notes
        );
        assert!(drained(&plain.diagnostic_calls).is_empty());
        let (baseline, _) =
            search_core(
                &mesh,
                &request(),
                Some(&Primary::new(false)),
                None,
                false,
                &Ablation::default(),
            )
            .expect("search");
        assert_eq!(outcome.record(), baseline.record());
    }

    /// Production succeeds for the first finalist only, and at a HIGHER burden
    /// than the prefilter gave: the lone production row wins by precedence
    /// while a prefilter-only row below it scores lower.
    struct OneFinalist;

    impl SupportDemandMeasurer for OneFinalist {
        fn measure_support_demand(
            &self,
            _mesh: &Mesh,
            _table: &OrientFaceTable,
            candidates: &[UpDirection],
            fidelity: DemandFidelity,
        ) -> Vec<Result<MeasuredSupportDemand, String>> {
            candidates
                .iter()
                .enumerate()
                .map(|(index, candidate)| match fidelity {
                    DemandFidelity::Prefilter => Ok(measured(primary_area(candidate), fidelity)),
                    DemandFidelity::Production if index == 0 => {
                        Ok(measured(primary_area(candidate) + 5.0, fidelity))
                    }
                    DemandFidelity::Production => Err("no production sweep".to_owned()),
                })
                .collect()
        }
    }

    #[test]
    fn a_cut_that_production_precedence_decides_reports_no_score_margin() {
        let mesh = make_overhang_table().expect("the fixture");
        let (outcome, d) = search_with_diagnostics(
            &mesh,
            &request(),
            Some(&OneFinalist),
            &DiagnosticsConfig::default(),
        )
        .expect("search");
        let c4 = d.cuts.iter().find(|cut| cut.name == "C4").expect("C4");
        let kept = c4.last_kept.as_ref().expect("the winner");
        let rejected = c4.first_rejected.as_ref().expect("a runner-up");
        assert_eq!(kept.candidate, outcome.record().winner.candidate);
        assert!(kept.measured_at_production && !rejected.measured_at_production);
        assert!(
            rejected.terms.score.get() < kept.terms.score.get(),
            "the premise: the prefilter-only row scores LOWER, so a score gap would be negative"
        );
        assert_eq!(c4.decided_by, CutBasis::ProductionFidelity);
        assert_eq!(c4.score_gap, None, "precedence, not score, put the winner first");

        // Every other cut compares rows of one class and keeps its margin.
        for cut in d.cuts.iter().filter(|cut| cut.name != "C4") {
            assert_eq!(cut.decided_by, CutBasis::Score, "{}", cut.name);
            assert!(cut.score_gap.is_some(), "{}", cut.name);
        }
        // The snapshots say which rows were production-measured.
        let after = d.snapshots.after_production.as_ref().expect("after production");
        assert_eq!(after.iter().filter(|row| row.measured_at_production).count(), 1);
        assert!(after[0].measured_at_production);
        let mut before = d
            .snapshots
            .exact_table
            .iter()
            .chain(d.snapshots.after_prefilter.iter().flatten());
        assert!(before.all(|row| !row.measured_at_production));
    }

    #[test]
    fn the_cut_basis_is_the_fidelity_class_not_the_score() {
        let mesh = make_overhang_table().expect("the fixture");
        let (_, d) = search_with_diagnostics(
            &mesh,
            &request(),
            Some(&Primary::new(true)),
            &DiagnosticsConfig::default(),
        )
        .expect("search");
        let c4 = d.cuts.iter().find(|cut| cut.name == "C4").expect("C4");
        // Three production finalists: the winner and the runner-up are one
        // class, so the score margin stands.
        let (kept, rejected) = (
            c4.last_kept.as_ref().expect("winner"),
            c4.first_rejected.as_ref().expect("runner-up"),
        );
        assert!(kept.measured_at_production && rejected.measured_at_production);
        assert_eq!(c4.decided_by, CutBasis::Score);
        assert_eq!(
            c4.score_gap.map(f64::to_bits),
            Some((rejected.terms.score.get() - kept.terms.score.get()).to_bits())
        );
        assert_eq!(cut_basis(rejected, kept), CutBasis::Score, "same class either way");
        let mut prefilter_row = rejected.clone();
        prefilter_row.measured_at_production = false;
        assert_eq!(cut_basis(kept, &prefilter_row), CutBasis::ProductionFidelity);
    }

    #[test]
    fn without_a_measurer_the_extras_keep_their_exact_scores() {
        let mesh = make_overhang_table().expect("the fixture");
        let extra = UpDirection::new([1.0, 0.0, 1.0]).expect("a direction");
        let with_measurer = Primary::new(true);
        let (_, full) =
            search_with_diagnostics(&mesh, &request(), Some(&with_measurer), &config(vec![extra]))
                .expect("search");
        let scored = full.extras[0].exact_terms.expect("the exact-table score");
        assert!(full.extras[0].production_terms.is_some(), "with a measurer it is priced too");

        let plain = Primary::new(false);
        let cases: [Option<&dyn SupportDemandMeasurer>; 2] = [None, Some(&plain)];
        for demand in cases {
            let (_, d) = search_with_diagnostics(&mesh, &request(), demand, &config(vec![extra]))
                .expect("search");
            assert_eq!(d.production_all, Some(Vec::new()));
            assert_eq!(d.extras.len(), 1, "the extra is still reported");
            let row = &d.extras[0];
            assert_eq!(row.label, "extra:0");
            assert_eq!(row.direction.map(f64::to_bits), extra.get().map(f64::to_bits));
            assert_eq!(row.exact_terms, Some(scored), "the same exact-table score");
            assert!(row.production_terms.is_none() && row.demand.is_none());
            assert!(
                row.error.as_deref().is_some_and(|reason| reason.contains("unavailable")),
                "{:?}",
                row.error
            );
            assert!(d.notes.iter().any(|note| note.contains("no separate non-retaining measurer")));
        }
    }

    #[test]
    fn the_two_nearest_are_what_a_full_sort_would_keep() {
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for round in 0..400_usize {
            let count = round % 13;
            // Few distinct values, so ties are the common case.
            let dots: Vec<f64> = (0..count).map(|_| (next() % 5) as f64 / 4.0).collect();
            let mut two = NearestTwo::default();
            for (id, dot) in dots.iter().enumerate() {
                two.offer(id, *dot);
            }
            let mut sorted: Vec<(usize, f64)> = dots.iter().copied().enumerate().collect();
            sorted.sort_by(|left, right| right.1.total_cmp(&left.1).then(left.0.cmp(&right.0)));
            let want: Vec<usize> = sorted.iter().take(2).map(|(id, _)| *id).collect();
            assert_eq!(two.ids().collect::<Vec<_>>(), want, "{dots:?}");
        }
    }

    #[test]
    fn the_diagnostics_describe_the_funnel_the_record_describes() {
        let mesh = make_overhang_table().expect("the fixture");
        let request = request();
        let primary = Primary::new(true);
        let (outcome, d) =
            search_with_diagnostics(&mesh, &request, Some(&primary), &DiagnosticsConfig::default())
                .expect("search");
        let record = outcome.record();
        assert!(d.production_all.is_none() && d.extras.is_empty(), "off unless asked for");
        assert_eq!(d.search_digest, record.search_digest);
        assert!(d.reproduces_grid, "{:?}", d.notes);

        // Provenance map.
        assert_eq!(d.candidates.len(), record.grid.candidates);
        let unique: BTreeSet<CandidateKey> = d.candidates.iter().map(|row| row.key).collect();
        assert_eq!(unique.len(), d.candidates.len());
        assert!(d.candidates.iter().enumerate().all(|(index, row)| row.id == index));
        assert_eq!(d.g3.len(), record.grid.candidates + record.grid.duplicates_dropped);
        assert_eq!(d.g3.iter().filter(|row| row.kept).count(), record.grid.candidates);

        // Escort: the commit tier's candidate set, and how it got there.
        let commit = &record.tiers[2];
        let mut commit_ids: Vec<usize> = commit.ranking.iter().map(|row| row.candidate).collect();
        commit_ids.sort_unstable();
        assert_eq!(d.escort.admitted.iter().map(|k| k.id).collect::<Vec<_>>(), commit_ids);
        assert_eq!(d.escort.added, commit.escorted);
        for row in d.escort.by_right.iter().filter(|row| row.source == "legacy") {
            assert!(commit_ids.contains(&row.id), "every legacy pose reaches the commit tier");
        }

        // The four cuts, with their rows either side.
        let names: Vec<&str> = d.cuts.iter().map(|cut| cut.name.as_str()).collect();
        assert_eq!(names, ["C1", "C2", "C3", "C4"]);
        let ladder = request.ladder;
        for (cut, k) in d
            .cuts
            .iter()
            .zip([ladder.refine_top_k(), ladder.commit_top_k(), ladder.commit_top_k(), 1])
        {
            assert_eq!(cut.k, k, "{}", cut.name);
        }
        let c1 = &d.cuts[0];
        assert_eq!(c1.k, ladder.refine_top_k());
        assert_eq!(c1.lod, record.tiers[0].lod);
        assert_eq!(c1.last_kept.as_ref().map(|row| row.rank), Some(c1.k - 1));
        assert_eq!(c1.first_rejected.as_ref().map(|row| row.rank), Some(c1.k));
        assert!(c1.next_rejected.len() <= NEXT_REJECTED);
        assert!(c1.next_rejected.iter().enumerate().all(|(i, row)| row.rank == c1.k + 1 + i));
        assert!(c1.score_gap.expect("a gap") >= 0.0);
        for cut in &d.cuts {
            for row in cut.last_kept.iter().chain(&cut.first_rejected).chain(&cut.next_rejected) {
                assert_eq!(row.key, d.candidates[row.candidate].key);
            }
        }
        let c4 = &d.cuts[3];
        assert_eq!(c4.last_kept.as_ref().map(|row| row.candidate), Some(record.winner.candidate));
        assert!(
            d.cuts.iter().all(|cut| cut.decided_by == CutBasis::Score && cut.score_gap.is_some()),
            "every cut here compares rows of one fidelity class"
        );

        // Escort rescues exist only where the search escorts (C2), and match
        // the next stage's actual input: the refine rows below the cut that
        // reached the commit tier, and no others.
        for index in [0, 2, 3] {
            assert!(d.cuts[index].rescued_by_escort.is_empty(), "{}", d.cuts[index].name);
        }
        let c2 = &d.cuts[1];
        let below_cut: Vec<usize> = record.tiers[1]
            .ranking
            .iter()
            .skip(c2.k)
            .map(|row| row.candidate)
            .collect();
        let reached_commit: Vec<CandidateKey> = below_cut
            .iter()
            .filter(|id| commit_ids.contains(id))
            .map(|id| d.candidates[*id].key)
            .collect();
        assert_eq!(c2.rescued_by_escort, reached_commit);
        let added_by_right: usize = c2.rescued_by_escort.len();
        assert!(added_by_right <= d.escort.added, "rescues are a subset of the escort's additions");

        // Snapshots are COPIES of the commit ranking at three moments.
        assert_eq!(d.snapshots.exact_table.len(), commit.ranking.len());
        assert!(d.snapshots.exact_table.windows(2).all(|w| w[0].terms.score.get() <= w[1].terms.score.get()));
        let after = d.snapshots.after_production.as_ref().expect("after production");
        assert_eq!(after[0].candidate, record.winner.candidate);
        assert_eq!(after[0].terms, record.winner.terms);
        let proxy = d
            .snapshots
            .exact_table
            .iter()
            .find(|row| row.candidate == record.winner.candidate)
            .expect("the winner was scored on the exact table");
        assert_ne!(
            proxy.terms.support_area_cm2.get().to_bits(),
            record.winner.terms.support_area_cm2.get().to_bits(),
            "the exact-table copy predates the measured burden"
        );
        assert!(d.snapshots.after_prefilter.is_some());
    }

    #[test]
    fn without_a_demand_measurer_only_the_proxy_cuts_exist() {
        let mesh = make_overhang_table().expect("the fixture");
        let (outcome, d) =
            search_with_diagnostics(&mesh, &request(), None, &config(Vec::new())).expect("search");
        let names: Vec<&str> = d.cuts.iter().map(|cut| cut.name.as_str()).collect();
        assert_eq!(names, ["C1", "C2"]);
        assert!(d.snapshots.after_prefilter.is_none() && d.snapshots.after_production.is_none());
        assert_eq!(d.snapshots.exact_table.len(), outcome.record().tiers[2].ranking.len());
    }

    #[test]
    fn orient_json_carries_the_record_and_the_diagnostics_and_reads_back() {
        let mesh = make_overhang_table().expect("the fixture");
        let primary = Primary::new(true);
        let (outcome, d) =
            search_with_diagnostics(&mesh, &request(), Some(&primary), &config(Vec::new())).expect("search");
        let bytes = orient_json(outcome.record(), &d).expect("serialize");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(
            value["record"]["search_digest"],
            value["diagnostics"]["search_digest"],
            "the two halves name the same search"
        );
        // serde_json's default float parser is not guaranteed to round-trip the
        // last bit, so compare the structure and the keys, not every f64.
        let back: SweepDiagnostics =
            serde_json::from_value(value["diagnostics"].clone()).expect("the diagnostics read back");
        assert_eq!(back.search_digest, d.search_digest);
        assert_eq!(back.escort, d.escort);
        assert_eq!(back.g3.len(), d.g3.len());
        assert_eq!(
            back.candidates.iter().map(|row| row.key).collect::<Vec<_>>(),
            d.candidates.iter().map(|row| row.key).collect::<Vec<_>>()
        );
        assert_eq!(
            back.cuts.iter().map(|cut| (&cut.name, cut.k)).collect::<Vec<_>>(),
            d.cuts.iter().map(|cut| (&cut.name, cut.k)).collect::<Vec<_>>()
        );
        assert_eq!(back.snapshots.exact_table.len(), d.snapshots.exact_table.len());
        assert_eq!(back.g1.face_to_bin_sha256, d.g1.face_to_bin_sha256);
    }

    // ------------------------------------------------- Lane A: ablations

    fn ablated(
        mesh: &Mesh,
        ablation: &Ablation,
    ) -> (crate::search::SweepOutcome, SweepDiagnostics) {
        let (outcome, d) = crate::search::search_ablated(
            mesh,
            &request(),
            Some(&Primary::new(true)),
            Some(&DiagnosticsConfig::default()),
            ablation,
        )
        .expect("search");
        (outcome, d.expect("diagnostics were asked for"))
    }

    fn rest_bins(d: &SweepDiagnostics) -> Vec<u32> {
        d.candidates
            .iter()
            .filter_map(|row| match row.key {
                CandidateKey::Rest(bin) => Some(bin),
                _ => None,
            })
            .collect()
    }

    /// With every word off, the search is the recorded one: the ablated entry
    /// point and the plain one return the same record and digest.
    #[test]
    fn with_the_ablation_off_the_search_is_the_recorded_one() {
        let mesh = make_overhang_table().expect("the fixture");
        let (plain, plain_d) = ablated(&mesh, &Ablation::default());
        let (with, _) = search_with_diagnostics(
            &mesh,
            &request(),
            Some(&Primary::new(true)),
            &DiagnosticsConfig::default(),
        )
        .expect("search");
        assert_eq!(plain.record(), with.record());
        assert_eq!(plain.record().search_digest, with.record().search_digest);
        assert_eq!(plain_d.ablation.rest_order, "by_area");
        assert!(!plain_d.ablation.exact_grid && plain_d.exact_shadow.is_none());
    }

    /// `exact-shadow` is out of band: the record, the digest and the winner are
    /// the shadow-off ones, and the shadow scores the SAME input key sets as
    /// the proxy cuts on the exact table.
    #[test]
    fn the_exact_shadow_changes_nothing_and_scores_the_proxy_cuts_own_inputs() {
        let mesh = make_overhang_table().expect("the fixture");
        let (off, off_d) = ablated(&mesh, &Ablation::default());
        let shadow_on = Ablation {
            exact_shadow: true,
            ..Ablation::default()
        };
        let (on, d) = ablated(&mesh, &shadow_on);
        assert_eq!(on.record(), off.record(), "bytes of the record");
        assert_eq!(on.record().search_digest, off.record().search_digest);
        assert_eq!(on.winner(), off.winner());
        assert_eq!(d.candidates, off_d.candidates, "the grid is the same grid");
        assert!(off_d.exact_shadow.is_none());

        let shadow = d.exact_shadow.expect("the shadow was asked for");
        let record = on.record();
        let (c1, c2) = (&shadow.c1, &shadow.c2);
        // C1: the whole grid, the set the coarse tier scored.
        assert_eq!(c1.input.len(), record.grid.candidates);
        assert_eq!(c1.cut.k, request().ladder.refine_top_k());
        assert_eq!(c1.ranking.len() + c1.refused, c1.input.len());
        assert_eq!(c1.kept.len(), c1.cut.k.min(c1.ranking.len()));
        // C2: the real C1's output, on one input for both cuts.
        let real_c1: BTreeSet<CandidateKey> =
            c1.proxy_kept.iter().map(|k| k.key).collect();
        let c2_in: BTreeSet<CandidateKey> = c2.input.iter().map(|k| k.key).collect();
        assert_eq!(real_c1, c2_in);
        assert_eq!(c2.cut.k, request().ladder.commit_top_k());
        let admitted = c2.admitted.as_ref().expect("C2 carries the escort");
        let proxy_admitted = c2.proxy_admitted.as_ref().expect("and the real set");
        assert_eq!(
            proxy_admitted.iter().map(|k| k.id).collect::<Vec<_>>(),
            d.escort.admitted.iter().map(|k| k.id).collect::<Vec<_>>()
        );
        assert!(admitted.len() >= c2.kept.len());
        // The shadow rows ARE the exact-table scores the commit tier computed
        // for the same candidates: same table, same directions, same bits.
        let exact_rows: std::collections::BTreeMap<usize, &KeyedRanked> = d
            .snapshots
            .exact_table
            .iter()
            .map(|row| (row.candidate, row))
            .collect();
        let mut compared = 0;
        for row in &c1.ranking {
            if let Some(commit_row) = exact_rows.get(&row.candidate) {
                assert_eq!(
                    row.terms.score.get().to_bits(),
                    commit_row.terms.score.get().to_bits(),
                    "{}",
                    row.key
                );
                compared += 1;
            }
        }
        assert_eq!(compared, exact_rows.len(), "every commit-tier row is in the C1 shadow");
        assert!(c1.ranking.windows(2).all(|w| w[0].terms.score.get() <= w[1].terms.score.get()));
    }

    /// `exact-grid` reads the grid off the exact table in ascending bin order.
    /// Everything else is the same funnel: the diagnostics rebuild the grid
    /// bit for bit, and the escort still lands the flat poses.
    #[test]
    fn exact_grid_enumerates_the_rest_poses_by_bin_index_off_the_exact_table() {
        let mesh = make_recessed_disc(24.0, 25.0, 3.5, 2.5, 22.5, 128).expect("a disc");
        let exact_grid = Ablation {
            exact_grid: true,
            ..Ablation::default()
        };
        let (_, off) = ablated(&mesh, &Ablation::default());
        let (on_outcome, on) = ablated(&mesh, &exact_grid);
        assert_eq!(on.grid_table_lod, crate::search::EXACT_LOD);
        assert!(on.ablation.exact_grid);
        assert_eq!(on.ablation.rest_order, "by_bin_index");
        assert!(on.reproduces_grid, "{:?}", on.notes);
        let bins = rest_bins(&on);
        assert!(bins.windows(2).all(|w| w[0] < w[1]), "ascending: {bins:?}");
        // A disc's proxy may or may not exist; either way the flat faces are
        // found and escorted, and the commit tier is scored on the exact LOD.
        assert_eq!(on.g4.flat_poses, 2);
        assert_eq!(on_outcome.record().grid.flat_faces, 2);
        // The rest-pose KEY SET is a property of the geometry the grid read.
        // Where the coarse tier fell back to the exact table, both runs read
        // the same table, so only the order differs.
        if off.grid_table_lod == crate::search::EXACT_LOD {
            let mut a = rest_bins(&off);
            a.sort_unstable();
            assert_eq!(a, bins);
        }
    }

    /// `exact-score` retires the proxy tiers (design D-12). Sabotage guard: the
    /// switch-off run on the SAME mesh scores its coarse tier on a proxy LOD with
    /// different scores, so the assertions below can fail; with the switch on,
    /// every tier names the exact LOD and every row carries the exact table's bits.
    #[test]
    fn exact_score_bypasses_the_proxy_tiers_and_scores_every_tier_on_the_exact_table() {
        // Over the 256-face proxy floor, so the off run really builds L1 and L2.
        let mesh = make_recessed_disc(24.0, 25.0, 3.5, 2.5, 22.5, 600).expect("a disc");
        let score_only = Ablation {
            exact_score: true,
            ..Ablation::default()
        };
        let both = Ablation {
            exact_grid: true,
            exact_score: true,
            ..Ablation::default()
        };
        let grid_only = Ablation {
            exact_grid: true,
            ..Ablation::default()
        };
        let (off, off_d) = ablated(&mesh, &Ablation::default());
        let exact_lod = crate::search::EXACT_LOD;
        // The precondition that makes the guard bite: off uses a proxy here.
        assert_ne!(off.record().tiers[0].lod, exact_lod, "fixture has no proxy tier");
        let off_exact: std::collections::BTreeMap<usize, u64> = off_d
            .snapshots
            .exact_table
            .iter()
            .map(|row| (row.candidate, row.terms.score.get().to_bits()))
            .collect();
        assert!(
            off.record().tiers[0]
                .ranking
                .iter()
                .any(|row| off_exact.get(&row.candidate).is_some_and(|bits| *bits != row.terms.score.get().to_bits())),
            "off: the proxy coarse scores differ from the exact ones"
        );

        for (name, ablation) in [("exact-score", &score_only), ("exact-grid+exact-score", &both)] {
            let (on, d) = ablated(&mesh, ablation);
            assert!(d.ablation.exact_score, "{name}");
            let record = on.record();
            assert_eq!(record.tiers.len(), 3, "{name}: the record keeps its three cuts");
            for tier in &record.tiers {
                assert_eq!(tier.lod, exact_lod, "{name}: {:?}", tier.tier);
            }
            let exact_rows: std::collections::BTreeMap<usize, u64> = d
                .snapshots
                .exact_table
                .iter()
                .map(|row| (row.candidate, row.terms.score.get().to_bits()))
                .collect();
            let mut compared = 0;
            // The commit tier is repriced in place by the demand measurer, so
            // only the C1 and C2 tiers still hold the raw table scores.
            for tier in &record.tiers[..2] {
                for row in &tier.ranking {
                    if let Some(bits) = exact_rows.get(&row.candidate) {
                        assert_eq!(*bits, row.terms.score.get().to_bits(), "{name} cand {}", row.candidate);
                        compared += 1;
                    }
                }
            }
            // Every commit-tier candidate is in the C1 ranking (it scored them all).
            assert!(compared >= exact_rows.len(), "{name}: {compared}");
            // One scorer: the later cuts keep the best of the same ranking.
            let c1: Vec<usize> = record.tiers[0].ranking.iter().map(|r| r.candidate).collect();
            let top = request().ladder.refine_top_k().min(c1.len());
            let c2_in: BTreeSet<usize> = record.tiers[1].ranking.iter().map(|r| r.candidate).collect();
            assert_eq!(c2_in, c1[..top].iter().copied().collect::<BTreeSet<_>>(), "{name}");
            // Nothing was built for the tiers, so nothing can have fallen back for
            // them; `exact-score` alone may still record the grid proxy's fallbacks.
            assert_eq!(record.fallbacks.is_empty(), ablation.exact_grid, "{name}: {:?}", record.fallbacks);
        }
        // Proxies are built only when something still reads them.
        let (both_outcome, _) = ablated(&mesh, &both);
        assert_eq!(both_outcome.record().lods.len(), 1, "no L1/L2 built or recorded");
        let (score_outcome, _) = ablated(&mesh, &score_only);
        assert_eq!(score_outcome.record().lods.len(), off.record().lods.len(), "the grid still reads the proxy");

        // The scoring switch does not touch the grid: same candidates as the
        // grid ablation (or off) alone.
        let (_, both_d) = ablated(&mesh, &both);
        let (_, grid_d) = ablated(&mesh, &grid_only);
        assert_eq!(both_d.candidates, grid_d.candidates);
        let (_, score_d) = ablated(&mesh, &score_only);
        assert_eq!(score_d.candidates, off_d.candidates);
    }

    /// The boundary classifier owns no number: a margin at or below the
    /// envelope is AMBIGUOUS, above it is clear, and a NaN is never clear.
    #[test]
    fn a_margin_at_or_below_its_envelope_is_ambiguous() {
        assert_eq!(boundary_event(0.0, 1e-9), BoundaryEvent::Ambiguous);
        assert_eq!(boundary_event(1e-9, 1e-9), BoundaryEvent::Ambiguous);
        assert_eq!(boundary_event(-1e-9, 1e-9), BoundaryEvent::Ambiguous);
        assert_eq!(boundary_event(2e-9, 1e-9), BoundaryEvent::Clear);
        assert_eq!(boundary_event(-2e-9, 1e-9), BoundaryEvent::Clear);
        assert_eq!(boundary_event(f64::NAN, 1e-9), BoundaryEvent::Ambiguous);
    }
}
