//! The candidate grid: which build-up directions the dense sweep considers, in
//! which order, and why that order is a contract.
//!
//! # Enumeration order IS the tie-break key
//!
//! [`score::score_candidates`](crate::score) computes; this module decides what
//! it is handed.  The comparator in [`search`](crate::search) scans candidate
//! ids ascending with strict `<`, so **the first id attaining the minimum
//! wins** — which makes this file's ordering part of the answer, not a
//! presentation detail.  Two consequences are deliberate:
//!
//! 1. **Identity is `CandidateId` 0.** The legacy writer seeded its best pose
//!    with `np.eye(4)` and emitted no orientation key when identity survived, so
//!    identity won every tie it took part in.  Two committed baselines
//!    (`Cylinder`, `make_box`) record exactly that outcome.  Putting identity
//!    first is how that semantics is reproduced without a second comparator key.
//! 2. **Deduplication keeps the EARLIER index.** D-12 keeps the legacy 13
//!    permanently "so the sweep is never worse by construction"; that claim is
//!    only structural if a near-duplicate Fibonacci direction cannot displace
//!    the legacy candidate it duplicates.  Keeping the earlier index is what
//!    makes it structural.
//!
//! # The three blocks
//!
//! | Block | Members | Count |
//! |---|---|---|
//! | 0 | the legacy 13 — identity, then ∓10/20/30° about X, then about Y | 13 |
//! | 1 | face-aligned rest poses (§1.3's hull substitute) | ≤ `ORIENT_REST_POSE_MAX` |
//! | 2 | the Fibonacci lattice on S² | `ORIENT_FIBONACCI_COUNT` |
//!
//! # Flat faces rest on the plate by right (2026-09-28)
//!
//! A pose that rests one of the part's large flat faces on the plate is a
//! rest pose (block 1), and the grid names the ones that are FLAT FACES
//! ([`SweepGrid::flat_face_poses`]); the search carries them to the commit
//! tier by right, exactly as it carries the legacy 13. A disc has two: its
//! plain glue face down (identity) and its stiffener side down (the 180°
//! flip, glue face UP — the owner's ruling for all three `Round_*` bases).
//! The recorded funnel lost exactly that second pose on Round 50 and
//! Round 100: the coarse proxy ranked it 76th and 482nd and promoted 24
//! (`plinth-evidence/slice-sweep-2026-09-02/field-2026-09-28/round50.md` Q2).
//!
//! **Only resting poses, not their face-up twins — measured.** A face-up
//! twin (`u = +n` for a flat face `n`) is a flat-face pose in its own right
//! only when the part's opposite side is itself flat, and then it is already
//! that side's resting pose. Adding the twin for a part whose opposite side
//! is NOT flat was built and measured on 2026-09-28 and rejected: on the
//! Morel base it adds the exact 180° flip (textured top DOWN), which the
//! commit tier then prefers by 0.049 points on fragility, against the
//! owner's ruling that the Morel base prints textured top up
//! (`plinth-evidence/.../lanes/round-pose.md`). That preference is a finding
//! about the objective's display pricing, recorded there as an open question,
//! not something a candidate-set rule should decide either way.
//!
//! **Which rest poses are flat faces — a comparison, not a constant.** A
//! rest pose `u` names the side of the part that faces `n = −u`. Its FLAT
//! area is the projected area of the faces whose normals lie within the
//! grid's own dedupe tolerance of `n` (the angle at which the search itself
//! calls two directions one); its SIDE area is the projected area of
//! everything facing that way, `Σ area·max(0, n_f·n)`. The rest pose is a
//! flat face when the flat area outweighs the rest of that side: `flat >
//! side − flat`. A disc's glue face (1809 of 1962 mm² on Round 50) and its
//! stiffener side (1868 of 1964) both qualify; a sphere's bins never do; a
//! cube's six faces all do. No number is chosen: the tolerance is the grid's
//! and the comparison is between two areas of the same part. The escort it
//! drives only adds candidates to a measured tier, so it can prune nothing
//! and decides no outcome — the measured commit tier does.
//!
//! Block 0 is enumerated **first**, where D-12's prose lists it last, for
//! reason (1) above.  The legacy 13 are built through the very same
//! `rotation(axis, degrees)` builder the pinned path uses, so a dense winner
//! drawn from block 0 is bit-identical to the pinned rotation for the same
//! angle rather than merely equal to within a normalization.
//!
//! # Rest poses without a convex hull
//!
//! D-12 names "convex-hull face-aligned" poses.  This workspace has no 3-D hull
//! kernel and the lockfile is frozen, so the recorded intent — poses in which a
//! large, flat region of the model lies against the plate — is captured at O(F)
//! by binning each face's **down direction** `−n_f` into a fixed
//! `ORIENT_REST_POSE_REFERENCE_BINS`-point reference lattice, accumulating area,
//! and emitting the area-weighted mean direction of the heaviest bins.  What is
//! **not** covered is stated rather than implied: a genuine three-point rest
//! pose (a treant balanced on three toes) has no single large facet and this
//! substitute will not propose it.  The loss is bounded — any such pose lies
//! within one grid spacing (~9°) of a block-2 direction — and a real hull kernel
//! is a filed follow-up.
//!
//! # Batch-first and determinism
//!
//! One dense faces × bins pass with faces outer so the reference lattice
//! streams once; one serial area fold in face order; one O(n²) dedupe over at
//! most 653 candidates.  No spatial index, no allocation per face, no RNG, no
//! clock.  Every tie — nearest bin, heaviest bin — resolves to the lowest index,
//! so the grid is a pure function of `(mesh, registered constants)`.

use geom_kernel::vec3;
use plinth_core::{m0_constants, Finite};
use sha2::{Digest, Sha256};

use crate::score::{OrientFaceTable, ScoreError, UpDirection};
use crate::{rotation, BaselineAxis, BaselineSelection};

/// Registered `ConstantId`s of the grid, carried so a report can stamp the
/// grid it searched rather than restating its shape.
pub const ORIENT_FIBONACCI_COUNT_ID: &str = "ORIENT_FIBONACCI_COUNT";
pub const ORIENT_REST_POSE_MAX_ID: &str = "ORIENT_REST_POSE_MAX";
pub const ORIENT_REST_POSE_REFERENCE_BINS_ID: &str = "ORIENT_REST_POSE_REFERENCE_BINS";
pub const ORIENT_DEDUPE_TOLERANCE_DEG_ID: &str = "ORIENT_DEDUPE_TOLERANCE_DEG";

/// The legacy enumeration, verbatim: `hollow.orient_search`'s 13 poses.
///
/// `None` is identity; the pairs are `(axis, degrees)` in the order the Python
/// listed them. This table is the whole of block 0 and is deliberately a
/// literal rather than a generated sweep — it is a historical artifact being
/// reproduced, not a parameterization being chosen.
const LEGACY_THIRTEEN: [Option<(BaselineAxis, f64)>; 13] = [
    None,
    Some((BaselineAxis::X, -10.0)),
    Some((BaselineAxis::X, 10.0)),
    Some((BaselineAxis::X, -20.0)),
    Some((BaselineAxis::X, 20.0)),
    Some((BaselineAxis::X, -30.0)),
    Some((BaselineAxis::X, 30.0)),
    Some((BaselineAxis::Y, -10.0)),
    Some((BaselineAxis::Y, 10.0)),
    Some((BaselineAxis::Y, -20.0)),
    Some((BaselineAxis::Y, 20.0)),
    Some((BaselineAxis::Y, -30.0)),
    Some((BaselineAxis::Y, 30.0)),
];

/// Which recorded block a candidate came from.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum CandidateBlock {
    /// The 13 poses of the retired heuristic, kept permanently so the dense
    /// sweep cannot be worse than the search it replaces (D-12).
    Legacy,
    /// Face-aligned rest poses, the hull substitute.
    RestPose,
    /// The Fibonacci lattice on S².
    Fibonacci,
}

/// A candidate's provenance key: where its direction came from, named by
/// something that survives re-numbering.
///
/// A [`Candidate::id`] is a position in the deduplicated vector, so one
/// dropped direction renumbers every later id (pose-stability design, D2). The
/// key does not move: a legacy direction and a lattice point are fixed bits,
/// and a rest pose is named by the reference bin whose faces it averages.
/// Diagnostics only: nothing in the search reads a key, and no recorded
/// structure carries one, so the search record and its digest are unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CandidateKey {
    /// Index into the legacy thirteen, `0..13`.
    Legacy(u8),
    /// Index of the reference bin (`0..ORIENT_REST_POSE_REFERENCE_BINS`).
    Rest(u32),
    /// Index into the Fibonacci lattice, `0..ORIENT_FIBONACCI_COUNT`.
    Fib(u32),
}

impl std::fmt::Display for CandidateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Legacy(k) => write!(f, "legacy:{k}"),
            Self::Rest(b) => write!(f, "rest:{b}"),
            Self::Fib(i) => write!(f, "fib:{i}"),
        }
    }
}

impl std::str::FromStr for CandidateKey {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bad = || format!("{text:?} is not a candidate key (legacy:K, rest:B or fib:I)");
        let (kind, number) = text.split_once(':').ok_or_else(bad)?;
        match kind {
            "legacy" => number.parse().map(Self::Legacy).map_err(|_| bad()),
            "rest" => number.parse().map(Self::Rest).map_err(|_| bad()),
            "fib" => number.parse().map(Self::Fib).map_err(|_| bad()),
            _ => Err(bad()),
        }
    }
}

impl serde::Serialize for CandidateKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for CandidateKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// Watches the rest-pose binning pass (the G1 decision of the pose-stability
/// design) from the inside, so the diagnostics read the production scan
/// instead of repeating it. It sees each binned face once, in face order,
/// after the face's bin is fixed; it cannot change the choice.
///
/// [`Unobserved`] is the production observer. Its `TRACKS_SECOND` is `false`,
/// so the production instantiation of the scan carries no second-best
/// bookkeeping and no call.
pub(crate) trait BinObserver {
    /// Whether the scan also tracks each face's second-best bin.
    const TRACKS_SECOND: bool;
    /// Called once before the scan: the table's face count and the bin count.
    fn begin(&mut self, faces: usize, bins: usize);
    /// A face the scan binned. `second` and `second_dot` hold the runner-up
    /// bin only when [`Self::TRACKS_SECOND`].
    fn face(&mut self, face: usize, best: usize, best_dot: f64, second: usize, second_dot: f64);
}

/// The observer that observes nothing.
pub(crate) struct Unobserved;

impl BinObserver for Unobserved {
    const TRACKS_SECOND: bool = false;
    fn begin(&mut self, _faces: usize, _bins: usize) {}
    fn face(&mut self, _face: usize, _best: usize, _best_dot: f64, _second: usize, _second_dot: f64) {}
}

/// What the binning pass accumulated, before the rest-pose cut.
pub(crate) struct RestBins {
    /// Summed face area per reference bin.
    pub area: Vec<f64>,
    /// Area-weighted sum of the down normals per reference bin.
    pub accumulated: Vec<[f64; 3]>,
    /// The occupied bins, heaviest first, ties by lowest bin index. The grid
    /// keeps the first `rest_pose_max` of them.
    pub order: Vec<usize>,
}

/// One direction the grid TRIED to admit, in the order it tried, with the
/// id it received (`None` when an earlier candidate covered it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridAttempt {
    pub key: CandidateKey,
    pub direction: UpDirection,
    pub kept: Option<usize>,
}

/// A rest pose and the reference bin it was averaged from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RestPose {
    pub bin: usize,
    pub direction: UpDirection,
}

/// One candidate: its id, where it came from, the direction itself, and the
/// selection a report would record for it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub id: usize,
    pub block: CandidateBlock,
    pub direction: UpDirection,
    pub selection: BaselineSelection,
}

/// The registered shape of the grid. Every number is read from the one M0
/// registry; nothing here is a literal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridSettings {
    fibonacci_count: usize,
    rest_pose_max: usize,
    reference_bins: usize,
    /// Stored as raw bits so the settings type stays `Eq` and a report cannot
    /// print one tolerance while the dedupe used another.
    dedupe_tolerance_deg_bits: u64,
}

impl GridSettings {
    /// Reads all four numbers from the one M0 registry.
    ///
    /// # Errors
    ///
    /// [`ScoreError::Constants`] when the registry cannot be constructed or a
    /// count constant is not a whole positive number of candidates.
    pub fn registered() -> Result<Self, ScoreError> {
        let values = m0_constants()
            .map_err(|error| ScoreError::Constants(error.to_string()))?
            .placeholders();
        Ok(Self {
            fibonacci_count: count(
                values.orient_fibonacci_count().diagnostic().get(),
                ORIENT_FIBONACCI_COUNT_ID,
            )?,
            rest_pose_max: count(
                values.orient_rest_pose_max().diagnostic().get(),
                ORIENT_REST_POSE_MAX_ID,
            )?,
            reference_bins: count(
                values.orient_rest_pose_reference_bins().diagnostic().get(),
                ORIENT_REST_POSE_REFERENCE_BINS_ID,
            )?,
            dedupe_tolerance_deg_bits: values
                .orient_dedupe_tolerance_deg()
                .diagnostic()
                .get()
                .to_bits(),
        })
    }

    #[must_use]
    pub const fn fibonacci_count(self) -> usize {
        self.fibonacci_count
    }
    #[must_use]
    pub const fn rest_pose_max(self) -> usize {
        self.rest_pose_max
    }
    #[must_use]
    pub const fn reference_bins(self) -> usize {
        self.reference_bins
    }
    #[must_use]
    pub fn dedupe_tolerance_deg(self) -> f64 {
        f64::from_bits(self.dedupe_tolerance_deg_bits)
    }

    /// `cos(tolerance)`: two directions closer than the tolerance have a dot
    /// product above this.
    #[must_use]
    pub fn dedupe_cosine(self) -> f64 {
        self.dedupe_tolerance_deg().to_radians().cos()
    }
}

/// A registered count constant, refused rather than rounded if it is not a
/// whole number of candidates.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "float-to-integer casts on lattice or step arithmetic: Rust's `as` saturates at the integer \
          bounds and maps NaN to 0, and every site in this function is range-checked before the cast \
          or clamped into range after it (read site by site, 2026-08-26). See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn count(value: f64, id: &'static str) -> Result<usize, ScoreError> {
    if !value.is_finite() || value < 1.0 || value.fract() != 0.0 || value > 1_048_576.0 {
        return Err(ScoreError::Constants(format!(
            "{id} = {value} is not a whole candidate count"
        )));
    }
    Ok(value as usize)
}

/// The deduplicated candidate list, in the order that is the tie-break key.
#[derive(Clone, Debug, PartialEq)]
pub struct SweepGrid {
    candidates: Vec<Candidate>,
    settings: GridSettings,
    legacy_count: usize,
    rest_pose_count: usize,
    fibonacci_count: usize,
    flat_face_count: usize,
    flat_face_poses: Vec<usize>,
    duplicates_dropped: usize,
    digest: String,
    /// Provenance, for diagnostics only (see [`CandidateKey`]); parallel to
    /// `candidates`.
    keys: Vec<CandidateKey>,
    /// Every direction the build tried to admit, kept or dropped.
    attempts: Vec<GridAttempt>,
    /// The rest poses before dedupe, with their reference bins.
    rest: Vec<RestPose>,
}

impl SweepGrid {
    /// Builds the grid for one model.
    ///
    /// The rest-pose block is derived from `table`, which must be the **coarse
    /// tier's own** table: rest poses are a property of the geometry being
    /// searched, and reading them off a different LOD than the one that scores
    /// them is the same class of mistake D-4 outlaws for delicacy.
    ///
    /// # Errors
    ///
    /// [`ScoreError::InvalidDirection`] if an accumulated rest-pose direction
    /// degenerates, which it can only do if a bin's faces cancel exactly.
    pub fn build(table: &OrientFaceTable, settings: GridSettings) -> Result<Self, ScoreError> {
        Self::build_observed(table, settings, &mut Unobserved).map(|(grid, _)| grid)
    }

    /// [`Self::build`] with an observer on the rest-pose binning pass, also
    /// returning what that pass accumulated. The grid is the same grid for any
    /// observer: the observer only reads.
    ///
    /// # Errors
    ///
    /// As [`Self::build`].
    pub(crate) fn build_observed<O: BinObserver>(
        table: &OrientFaceTable,
        settings: GridSettings,
        observer: &mut O,
    ) -> Result<(Self, RestBins), ScoreError> {
        let mut admitted = Admitted::default();
        let cosine = settings.dedupe_cosine();

        // Block 0. Built through the pinned path's own rotation builder, so a
        // dense winner at one of these angles produces the same bits the pinned
        // path would.
        for (legacy_index, entry) in LEGACY_THIRTEEN.into_iter().enumerate() {
            let (selection, matrix) = match entry {
                None => (BaselineSelection::Identity, None),
                Some((axis, degrees)) => (
                    BaselineSelection::AxisDegrees {
                        axis,
                        degrees: Finite::new(degrees).map_err(|_| {
                            ScoreError::InvalidDirection {
                                direction: [0.0, 0.0, 0.0],
                            }
                        })?,
                    },
                    Some(
                        rotation(axis, degrees).map_err(|_| ScoreError::InvalidDirection {
                            direction: [0.0, 0.0, degrees],
                        })?,
                    ),
                ),
            };
            // `u = Rᵀẑ` is row 2 of R: for any normal n, `(R n)_z = n·u`.
            let direction = match matrix {
                None => UpDirection::MODEL_UP,
                Some(matrix) => UpDirection::new(matrix[2])?,
            };
            admitted.push(
                CandidateBlock::Legacy,
                CandidateKey::Legacy(u8::try_from(legacy_index).unwrap_or(u8::MAX)),
                direction,
                Some(selection),
                cosine,
            );
        }
        let legacy_count = admitted.candidates.len();

        // Block 1.
        let (rest, bins) = rest_poses(table, settings, observer)?;
        for pose in &rest {
            admitted.push(
                CandidateBlock::RestPose,
                CandidateKey::Rest(u32::try_from(pose.bin).unwrap_or(u32::MAX)),
                pose.direction,
                None,
                cosine,
            );
        }
        let rest_pose_count = admitted.candidates.len() - legacy_count;

        // Block 2.
        for (lattice_index, direction) in fibonacci_directions(settings.fibonacci_count)?
            .into_iter()
            .enumerate()
        {
            admitted.push(
                CandidateBlock::Fibonacci,
                CandidateKey::Fib(u32::try_from(lattice_index).unwrap_or(u32::MAX)),
                direction,
                None,
                cosine,
            );
        }
        let fibonacci_count = admitted.candidates.len() - legacy_count - rest_pose_count;
        let Admitted {
            candidates,
            keys,
            attempts,
            duplicates_dropped,
        } = admitted;

        // The flat faces, and the pose that rests each one on the plate,
        // resolved to the candidate that covers it — the EARLIEST within the
        // dedupe tolerance, which is the one dedupe kept (a flat face resting
        // at +Z is identity, the legacy candidate 0) — ascending, once each.
        // No candidate is added: every such pose is already a rest pose.
        let rest_directions: Vec<UpDirection> = rest.iter().map(|pose| pose.direction).collect();
        let flat = flat_faces(table, &rest_directions, cosine);
        let mut poses: Vec<usize> = flat
            .iter()
            .filter_map(|face_down| covering(&candidates, *face_down, cosine))
            .collect();
        poses.sort_unstable();
        poses.dedup();

        let digest = direction_digest(&candidates);
        let grid = Self {
            candidates,
            settings,
            legacy_count,
            rest_pose_count,
            fibonacci_count,
            flat_face_count: flat.len(),
            flat_face_poses: poses,
            duplicates_dropped,
            digest,
            keys,
            attempts,
            rest,
        };
        Ok((grid, bins))
    }

    /// The provenance key of every candidate, parallel to
    /// [`Self::candidates`]. Diagnostics only.
    #[must_use]
    pub fn keys(&self) -> &[CandidateKey] {
        &self.keys
    }

    /// Every direction the build tried to admit, in the order it tried, kept
    /// or dropped. Diagnostics only.
    #[must_use]
    pub fn attempts(&self) -> &[GridAttempt] {
        &self.attempts
    }

    /// The rest poses before dedupe, each with its reference bin.
    /// Diagnostics only.
    #[must_use]
    pub fn rest_pose_entries(&self) -> &[RestPose] {
        &self.rest
    }

    #[must_use]
    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }

    /// The directions alone, in candidate order — what the batched scorer takes.
    #[must_use]
    pub fn directions(&self) -> Vec<UpDirection> {
        self.candidates
            .iter()
            .map(|candidate| candidate.direction)
            .collect()
    }

    #[must_use]
    pub const fn settings(&self) -> GridSettings {
        self.settings
    }
    #[must_use]
    pub const fn legacy_count(&self) -> usize {
        self.legacy_count
    }
    #[must_use]
    pub const fn rest_pose_count(&self) -> usize {
        self.rest_pose_count
    }
    #[must_use]
    pub const fn fibonacci_count(&self) -> usize {
        self.fibonacci_count
    }
    /// How many rest poses qualified as flat faces (module doc).
    #[must_use]
    pub const fn flat_face_count(&self) -> usize {
        self.flat_face_count
    }
    /// The candidate ids that rest a flat face on the plate, ascending, once
    /// each. These reach the commit tier by right; see `search::escort`.
    #[must_use]
    pub fn flat_face_poses(&self) -> &[usize] {
        &self.flat_face_poses
    }
    #[must_use]
    pub const fn duplicates_dropped(&self) -> usize {
        self.duplicates_dropped
    }
    /// SHA-256 over the raw bits of every admitted direction, in order. One
    /// string answers "did the grid move".
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

/// The candidates admitted so far, with the provenance diagnostics read off
/// the same pass.
#[derive(Default)]
struct Admitted {
    candidates: Vec<Candidate>,
    keys: Vec<CandidateKey>,
    attempts: Vec<GridAttempt>,
    duplicates_dropped: usize,
}

impl Admitted {
    /// Appends a direction unless an EARLIER candidate already covers it.
    fn push(
        &mut self,
        block: CandidateBlock,
        key: CandidateKey,
        direction: UpDirection,
        selection: Option<BaselineSelection>,
        cosine: f64,
    ) {
        let u = direction.get();
        for existing in &self.candidates {
            let v = existing.direction.get();
            if vec3::dot(u, v) > cosine {
                self.duplicates_dropped += 1;
                self.attempts.push(GridAttempt {
                    key,
                    direction,
                    kept: None,
                });
                return;
            }
        }
        let id = self.candidates.len();
        self.candidates.push(Candidate {
            id,
            block,
            direction,
            selection: selection.unwrap_or_else(|| up_selection(direction)),
        });
        self.keys.push(key);
        self.attempts.push(GridAttempt {
            key,
            direction,
            kept: Some(id),
        });
    }
}

/// The earliest candidate within the dedupe tolerance of `direction` — the one
/// [`Admitted::push`] kept for it — or `None` when nothing covers it.
fn covering(candidates: &[Candidate], direction: UpDirection, cosine: f64) -> Option<usize> {
    let u = direction.get();
    candidates
        .iter()
        .find(|candidate| vec3::dot(u, candidate.direction.get()) > cosine)
        .map(|candidate| candidate.id)
}

/// The rest poses that are flat faces (module doc, "Flat faces"), in
/// rest-pose order: those whose within-tolerance area outweighs the rest of
/// the side they face.
///
/// One dense faces × rest-poses pass, faces outer so the pose list streams
/// once, accumulating both areas for every pose at once; the fold is serial in
/// face order, so the result is bit-identical at any thread count.
fn flat_faces(table: &OrientFaceTable, rest: &[UpDirection], cosine: f64) -> Vec<UpDirection> {
    // The side a rest pose `u` lays on the plate faces `n = −u`.
    let sides: Vec<[f64; 3]> = rest.iter().map(|pose| pose.antipode().get()).collect();
    let mut flat = vec![0.0_f64; sides.len()];
    let mut side = vec![0.0_f64; sides.len()];
    let areas = table.areas_mm2();
    for (face, normal) in table.normals().iter().enumerate() {
        let face_area = areas[face];
        if face_area <= 0.0 || !face_area.is_finite() {
            continue;
        }
        for (pose, n) in sides.iter().enumerate() {
            let facing = vec3::dot(*normal, *n);
            if facing <= 0.0 {
                continue;
            }
            let projected = face_area * facing;
            side[pose] += projected;
            if facing > cosine {
                flat[pose] += projected;
            }
        }
    }
    rest.iter()
        .enumerate()
        .filter(|(pose, _)| flat[*pose] > side[*pose] - flat[*pose])
        .map(|(_, direction)| *direction)
        .collect()
}

/// The selection a searched direction records: the direction itself, which is
/// the object the sweep actually chose.
fn up_selection(direction: UpDirection) -> BaselineSelection {
    let u = direction.get();
    BaselineSelection::UpDirection {
        up_model_frame: [
            Finite::new(u[0]).expect("a unit direction component is finite"),
            Finite::new(u[1]).expect("a unit direction component is finite"),
            Finite::new(u[2]).expect("a unit direction component is finite"),
        ],
    }
}

/// The Fibonacci lattice on S², in exactly the recorded expression order.
///
/// `z_i = 1 − (2i+1)/N`, `φ_i = i·π(3−√5)`. The expression order is pinned for
/// the same reason the objective's association order is: a re-association is a
/// different set of rounding decisions, and at ~9° spacing two directions that
/// differ in the last bit can still rank differently.
pub fn fibonacci_directions(count: usize) -> Result<Vec<UpDirection>, ScoreError> {
    let n = count as f64;
    let golden = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
    let mut directions = Vec::with_capacity(count);
    for index in 0..count {
        let i = index as f64;
        let z = 1.0 - (2.0 * i + 1.0) / n;
        let phi = i * golden;
        let radius = (1.0 - z * z).max(0.0).sqrt();
        directions.push(UpDirection::new([
            radius * phi.cos(),
            radius * phi.sin(),
            z,
        ])?);
    }
    Ok(directions)
}

/// Face-aligned rest poses: the hull substitute of the module doc.
fn rest_poses<O: BinObserver>(
    table: &OrientFaceTable,
    settings: GridSettings,
    observer: &mut O,
) -> Result<(Vec<RestPose>, RestBins), ScoreError> {
    let reference = fibonacci_directions(settings.reference_bins)?;
    let bins = reference.len();
    let mut area = vec![0.0_f64; bins];
    let mut accumulated = vec![[0.0_f64; 3]; bins];

    // One dense faces × bins pass, faces outer so the reference lattice streams
    // once. The fold is serial in face order, so the accumulated sums are
    // bit-identical at any thread count.
    let areas = table.areas_mm2();
    let normals = table.normals();
    observer.begin(normals.len(), bins);
    for (face, normal) in normals.iter().enumerate() {
        let face_area = areas[face];
        if face_area <= 0.0 || !face_area.is_finite() {
            continue;
        }
        // The direction that puts this face DOWN.
        let down = [-normal[0], -normal[1], -normal[2]];
        let (best, best_dot, second, second_dot) = if O::TRACKS_SECOND {
            nearest_bins::<true>(down, &reference)
        } else {
            nearest_bins::<false>(down, &reference)
        };
        observer.face(face, best, best_dot, second, second_dot);
        area[best] += face_area;
        accumulated[best][0] += face_area * down[0];
        accumulated[best][1] += face_area * down[1];
        accumulated[best][2] += face_area * down[2];
    }

    // Heaviest bins first, ties by lowest bin index. `sort_by` is stable, so the
    // index tie-break is the input order and needs no second key.
    let mut order: Vec<usize> = (0..bins).filter(|bin| area[*bin] > 0.0).collect();
    order.sort_by(|left, right| {
        area[*right]
            .partial_cmp(&area[*left])
            .expect("accumulated areas are finite")
    });

    let mut poses = Vec::with_capacity(settings.rest_pose_max.min(order.len()));
    for bin in order.iter().copied().take(settings.rest_pose_max) {
        // A bin whose faces cancel exactly has no pose; it is skipped rather
        // than normalized into a direction nothing supports.
        if let Ok(direction) = UpDirection::new(accumulated[bin]) {
            poses.push(RestPose { bin, direction });
        }
    }
    Ok((
        poses,
        RestBins {
            area,
            accumulated,
            order,
        },
    ))
}

/// The reference bin nearest `down` (and, when `SECOND`, the runner-up): the
/// grid's one scan over the bins. `best` is chosen by strict `>`, so the
/// lowest bin index wins a tie, and the choice does not depend on `SECOND`;
/// the production instantiation (`SECOND = false`) is the scan with the
/// second-best bookkeeping compiled out.
#[inline]
fn nearest_bins<const SECOND: bool>(
    down: [f64; 3],
    reference: &[UpDirection],
) -> (usize, f64, usize, f64) {
    let mut best = 0_usize;
    let mut best_dot = f64::NEG_INFINITY;
    let mut second = 0_usize;
    let mut second_dot = f64::NEG_INFINITY;
    for (bin, candidate) in reference.iter().enumerate() {
        let v = candidate.get();
        let dot = vec3::dot(down, v);
        // Strict `>`: the lowest bin index wins a tie.
        if dot > best_dot {
            if SECOND {
                second = best;
                second_dot = best_dot;
            }
            best_dot = dot;
            best = bin;
        } else if SECOND && dot > second_dot {
            second = bin;
            second_dot = dot;
        }
    }
    (best, best_dot, second, second_dot)
}

/// SHA-256 over the raw f64 bits of every admitted direction, in order.
fn direction_digest(candidates: &[Candidate]) -> String {
    let mut hasher = Sha256::new();
    hasher.update((candidates.len() as u64).to_le_bytes());
    for candidate in candidates {
        hasher.update([candidate.block as u8]);
        for component in candidate.direction.get() {
            hasher.update(component.to_bits().to_le_bytes());
        }
    }
    hex(hasher)
}

/// SHA-256 over a bare direction list, block-free.
///
/// This is what freezes the generator itself: the 512-point lattice is a pure
/// function of one integer, so a golden over it fails the moment the expression
/// order, the golden-angle constant or the `z` formula changes — independently
/// of any mesh, any dedupe and any block assignment.
#[must_use]
pub fn direction_set_digest(directions: &[UpDirection]) -> String {
    let mut hasher = Sha256::new();
    hasher.update((directions.len() as u64).to_le_bytes());
    for direction in directions {
        for component in direction.get() {
            hasher.update(component.to_bits().to_le_bytes());
        }
    }
    hex(hasher)
}

/// The lower-case hex of a finished hash.
pub(crate) fn hex(hasher: Sha256) -> String {
    let digest = hasher.finalize();
    let mut text = String::with_capacity(digest.len() * 2);
    for byte in digest {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_mesh::Mesh;

    fn cube() -> Mesh {
        let verts = vec![
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
        Mesh::new(verts, tris).expect("cube")
    }

    fn table() -> OrientFaceTable {
        OrientFaceTable::build(&cube(), "test").expect("cube table")
    }

    #[test]
    fn identity_is_candidate_zero_and_the_legacy_thirteen_lead() {
        let grid = SweepGrid::build(&table(), GridSettings::registered().unwrap()).unwrap();
        assert_eq!(grid.candidates()[0].id, 0);
        assert_eq!(
            grid.candidates()[0].selection,
            BaselineSelection::Identity,
            "identity must be CandidateId 0: two committed baselines depend on identity winning ties"
        );
        assert_eq!(grid.candidates()[0].direction, UpDirection::MODEL_UP);
        assert_eq!(grid.legacy_count(), 13);
        assert!(grid.candidates()[..13]
            .iter()
            .all(|candidate| candidate.block == CandidateBlock::Legacy));
    }

    /// The legacy block's directions are the pinned path's own rotation rows,
    /// not a re-derivation: a dense winner at −30° about X and a pinned −30°
    /// about X must be the same rotation, bit for bit.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn legacy_directions_come_from_the_pinned_rotation_builder() {
        let grid = SweepGrid::build(&table(), GridSettings::registered().unwrap()).unwrap();
        let matrix = rotation(BaselineAxis::X, -30.0).unwrap();
        let expected = UpDirection::new(matrix[2]).unwrap();
        let found = grid
            .candidates()
            .iter()
            .find(|candidate| {
                candidate.selection
                    == BaselineSelection::AxisDegrees {
                        axis: BaselineAxis::X,
                        degrees: Finite::new(-30.0).unwrap(),
                    }
            })
            .expect("the legacy X −30 pose is in the grid");
        assert_eq!(found.direction.get(), expected.get());
    }

    #[test]
    fn every_pair_of_admitted_directions_is_at_least_the_dedupe_tolerance_apart() {
        let settings = GridSettings::registered().unwrap();
        let grid = SweepGrid::build(&table(), settings).unwrap();
        let cosine = settings.dedupe_cosine();
        for (index, left) in grid.candidates().iter().enumerate() {
            for right in &grid.candidates()[index + 1..] {
                let a = left.direction.get();
                let b = right.direction.get();
                assert!(
                    a[0] * b[0] + a[1] * b[1] + a[2] * b[2] <= cosine,
                    "candidates {} and {} are closer than the dedupe tolerance",
                    left.id,
                    right.id
                );
            }
        }
        assert_eq!(
            grid.candidates().len(),
            13 + grid.rest_pose_count() + grid.fibonacci_count()
        );
    }

    /// The dedupe test is `dot > cosine`, STRICT: a direction exactly on the
    /// radius is kept, one ulp inside it is dropped. The provenance record
    /// (`attempts`, `keys`) tells the two apart and agrees with the decision.
    #[test]
    fn the_dedupe_radius_is_strict_and_the_attempt_log_records_each_decision() {
        let cosine = GridSettings::registered().unwrap().dedupe_cosine();
        let x = (1.0 - cosine * cosine).sqrt();
        for (z, kept) in [
            (cosine, true),
            (f64::from_bits(cosine.to_bits() + 1), false),
            (f64::from_bits(cosine.to_bits() - 1), true),
        ] {
            let mut admitted = Admitted::default();
            admitted.push(
                CandidateBlock::Legacy,
                CandidateKey::Legacy(0),
                UpDirection::MODEL_UP,
                Some(BaselineSelection::Identity),
                cosine,
            );
            admitted.push(
                CandidateBlock::Fibonacci,
                CandidateKey::Fib(9),
                UpDirection::raw([x, 0.0, z]),
                None,
                cosine,
            );
            assert_eq!(admitted.candidates.len(), if kept { 2 } else { 1 }, "z = {z:e}");
            assert_eq!(admitted.duplicates_dropped, usize::from(!kept));
            assert_eq!(admitted.attempts.len(), 2, "the dropped direction is still logged");
            assert_eq!(admitted.attempts[1].key, CandidateKey::Fib(9));
            assert_eq!(admitted.attempts[1].kept, kept.then_some(1));
            assert_eq!(admitted.keys.len(), admitted.candidates.len());
        }
    }

    /// A cube's six faces are all flat faces, and all six resting poses are
    /// named for the escort; the one that rests the authored bottom down is
    /// identity, the legacy candidate that already covers it.
    #[test]
    fn a_cube_names_six_flat_faces() {
        let grid = SweepGrid::build(&table(), GridSettings::registered().unwrap()).unwrap();
        assert_eq!(grid.flat_face_count(), 6);
        assert_eq!(grid.flat_face_poses().len(), 6);
        assert!(grid.flat_face_poses().contains(&0), "identity rests the cube's bottom face down");
    }

    /// A cube's six face-down poses are the six axis directions, and the
    /// substitute must find all six before anything else.
    #[test]
    fn a_cube_yields_its_six_axis_rest_poses() {
        let settings = GridSettings::registered().unwrap();
        let (poses, _) = rest_poses(&table(), settings, &mut Unobserved).unwrap();
        assert_eq!(poses.len(), 6, "a cube has exactly six face-down poses");
        for pose in &poses {
            let u = pose.direction.get();
            let axes = u.iter().filter(|value| value.abs() > 0.5).count();
            assert_eq!(axes, 1, "a cube rest pose is an axis direction: {u:?}");
        }
    }

    /// Records every call and tracks the runner-up bin.
    #[derive(Default)]
    struct Recorder {
        begun: Option<(usize, usize)>,
        seen: Vec<(usize, usize, usize)>,
    }

    impl BinObserver for Recorder {
        const TRACKS_SECOND: bool = true;
        fn begin(&mut self, faces: usize, bins: usize) {
            self.begun = Some((faces, bins));
        }
        fn face(&mut self, face: usize, best: usize, best_dot: f64, second: usize, second_dot: f64) {
            assert!(best_dot >= second_dot);
            self.seen.push((face, best, second));
        }
    }

    /// An observer reads the scan and changes nothing: the grid, its rest-pose
    /// sums and its bin order are the unobserved ones, bit for bit, and the
    /// observer saw each binned face once, in face order.
    #[test]
    fn an_observer_does_not_change_the_grid_or_the_bins() {
        let settings = GridSettings::registered().unwrap();
        let table = table();
        let (plain, plain_bins) =
            SweepGrid::build_observed(&table, settings, &mut Unobserved).unwrap();
        let mut recorder = Recorder::default();
        let (watched, watched_bins) =
            SweepGrid::build_observed(&table, settings, &mut recorder).unwrap();
        assert_eq!(plain.digest(), watched.digest());
        assert_eq!(plain.candidates(), watched.candidates());
        assert_eq!(plain.rest_pose_entries(), watched.rest_pose_entries());
        assert_eq!(plain.digest(), SweepGrid::build(&table, settings).unwrap().digest());
        assert_eq!(plain_bins.order, watched_bins.order);
        let bits = |sums: &[[f64; 3]]| -> Vec<[u64; 3]> {
            sums.iter().map(|v| v.map(f64::to_bits)).collect()
        };
        assert_eq!(bits(&plain_bins.accumulated), bits(&watched_bins.accumulated));
        assert_eq!(
            plain_bins.area.iter().map(|a| a.to_bits()).collect::<Vec<_>>(),
            watched_bins.area.iter().map(|a| a.to_bits()).collect::<Vec<_>>()
        );
        assert_eq!(recorder.begun, Some((table.face_count(), settings.reference_bins())));
        assert_eq!(recorder.seen.len(), table.face_count());
        assert!(recorder.seen.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert!(recorder.seen.iter().all(|(_, best, second)| best != second));
    }

    /// The grid is a pure function of `(mesh, registered constants)`, and the
    /// digest is what says so in one string.
    #[test]
    fn the_grid_is_reproducible_bit_for_bit() {
        let settings = GridSettings::registered().unwrap();
        let first = SweepGrid::build(&table(), settings).unwrap();
        let second = SweepGrid::build(&table(), settings).unwrap();
        assert_eq!(first.digest(), second.digest());
        assert_eq!(first.candidates(), second.candidates());
        assert_eq!(first.digest().len(), 64);
    }

    #[test]
    fn the_fibonacci_lattice_is_unit_length_and_spans_the_whole_sphere() {
        let directions = fibonacci_directions(512).unwrap();
        assert_eq!(directions.len(), 512);
        let mut lowest = f64::INFINITY;
        let mut highest = f64::NEG_INFINITY;
        for direction in &directions {
            let u = direction.get();
            let norm = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
            assert!((norm - 1.0).abs() < 1e-12);
            lowest = lowest.min(u[2]);
            highest = highest.max(u[2]);
        }
        // The objective is NOT symmetric under u → −u, so the whole sphere is
        // searched and no hemisphere shortcut exists.
        assert!(lowest < -0.99 && highest > 0.99);
    }
}
