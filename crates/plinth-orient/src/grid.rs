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
    duplicates_dropped: usize,
    digest: String,
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
        let mut candidates: Vec<Candidate> = Vec::new();
        let mut duplicates_dropped = 0_usize;
        let cosine = settings.dedupe_cosine();

        // Block 0. Built through the pinned path's own rotation builder, so a
        // dense winner at one of these angles produces the same bits the pinned
        // path would.
        for entry in LEGACY_THIRTEEN {
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
            push_unique(
                &mut candidates,
                &mut duplicates_dropped,
                CandidateBlock::Legacy,
                direction,
                Some(selection),
                cosine,
            );
        }
        let legacy_count = candidates.len();

        // Block 1.
        for direction in rest_poses(table, settings)? {
            push_unique(
                &mut candidates,
                &mut duplicates_dropped,
                CandidateBlock::RestPose,
                direction,
                None,
                cosine,
            );
        }
        let rest_pose_count = candidates.len() - legacy_count;

        // Block 2.
        for direction in fibonacci_directions(settings.fibonacci_count)? {
            push_unique(
                &mut candidates,
                &mut duplicates_dropped,
                CandidateBlock::Fibonacci,
                direction,
                None,
                cosine,
            );
        }
        let fibonacci_count = candidates.len() - legacy_count - rest_pose_count;

        let digest = direction_digest(&candidates);
        Ok(Self {
            candidates,
            settings,
            legacy_count,
            rest_pose_count,
            fibonacci_count,
            duplicates_dropped,
            digest,
        })
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

/// Appends a direction unless an EARLIER candidate already covers it.
fn push_unique(
    candidates: &mut Vec<Candidate>,
    duplicates_dropped: &mut usize,
    block: CandidateBlock,
    direction: UpDirection,
    selection: Option<BaselineSelection>,
    cosine: f64,
) {
    let u = direction.get();
    for existing in candidates.iter() {
        let v = existing.direction.get();
        if vec3::dot(u, v) > cosine {
            *duplicates_dropped += 1;
            return;
        }
    }
    let id = candidates.len();
    candidates.push(Candidate {
        id,
        block,
        direction,
        selection: selection.unwrap_or_else(|| up_selection(direction)),
    });
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
fn rest_poses(
    table: &OrientFaceTable,
    settings: GridSettings,
) -> Result<Vec<UpDirection>, ScoreError> {
    let reference = fibonacci_directions(settings.reference_bins)?;
    let bins = reference.len();
    let mut area = vec![0.0_f64; bins];
    let mut accumulated = vec![[0.0_f64; 3]; bins];

    // One dense faces × bins pass, faces outer so the reference lattice streams
    // once. The fold is serial in face order, so the accumulated sums are
    // bit-identical at any thread count.
    let areas = table.areas_mm2();
    let normals = table.normals();
    for (face, normal) in normals.iter().enumerate() {
        let face_area = areas[face];
        if face_area <= 0.0 || !face_area.is_finite() {
            continue;
        }
        // The direction that puts this face DOWN.
        let down = [-normal[0], -normal[1], -normal[2]];
        let mut best = 0_usize;
        let mut best_dot = f64::NEG_INFINITY;
        for (bin, candidate) in reference.iter().enumerate() {
            let v = candidate.get();
            let dot = vec3::dot(down, v);
            // Strict `>`: the lowest bin index wins a tie.
            if dot > best_dot {
                best_dot = dot;
                best = bin;
            }
        }
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
    order.truncate(settings.rest_pose_max);

    let mut poses = Vec::with_capacity(order.len());
    for bin in order {
        // A bin whose faces cancel exactly has no pose; it is skipped rather
        // than normalized into a direction nothing supports.
        if let Ok(direction) = UpDirection::new(accumulated[bin]) {
            poses.push(direction);
        }
    }
    Ok(poses)
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

fn hex(hasher: Sha256) -> String {
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

    /// A cube's six face-down poses are the six axis directions, and the
    /// substitute must find all six before anything else.
    #[test]
    fn a_cube_yields_its_six_axis_rest_poses() {
        let settings = GridSettings::registered().unwrap();
        let poses = rest_poses(&table(), settings).unwrap();
        assert_eq!(poses.len(), 6, "a cube has exactly six face-down poses");
        for pose in &poses {
            let u = pose.get();
            let axes = u.iter().filter(|value| value.abs() > 0.5).count();
            assert_eq!(axes, 1, "a cube rest pose is an axis direction: {u:?}");
        }
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
