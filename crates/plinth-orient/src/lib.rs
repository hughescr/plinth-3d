//! Orientation: the dense sweep, the pinned baseline it can be replayed as, and
//! the operator's "use it as drawn".
//!
//! Three entry points, and the difference between them is where the rotation
//! came from, never what is done with it:
//!
//! - [`search_and_orient`] runs the recorded three-tier dense sweep ([`search`])
//!   over the candidate grid ([`grid`]), scoring `03-physics-and-constants.md`
//!   §3.8's four terms in one batched call per tier ([`score`]) with the exact
//!   peak cross-section solver ([`section`]), and commits the winner at the
//!   production mesh.
//! - [`run`] takes the selected legacy orientation out of a required pipeline
//!   JSON and applies that rigid rotation, unchanged.
//! - [`orient_as_drawn`] applies identity because the OPERATOR said so
//!   (`--orient none`, a `plinth-no-rotate` tag, or a `<stem>.asdrawn` marker;
//!   see [`as_drawn`]), and records which of those it was.
//!
//! All three then recreate the legacy frame — the transformed bounding box is
//! centered in XY and rests at Z=0 — through the one `transform_mesh`, so a
//! searched orientation, a pinned one and an as-drawn one differ in nothing
//! downstream; the lift off the plate is applied later, by placement, to all
//! three alike. The output replaces the production mesh and retains only
//! reports, never an untransformed second mesh.
//!
//! **Orientation is still never inferred.** The sweep does not guess: it
//! measures every candidate and records what it measured, including when the
//! answer it commits to is identity. Identity is a rotation like any other and
//! is chosen by the search only by winning, never by default, and never as a
//! fallback when the search fails. The as-drawn path is not a default either:
//! it runs only on an explicit operator mark, and its provenance names the mark.
#![forbid(unsafe_code)]

pub mod ablate;
pub mod as_drawn;
#[cfg(test)]
mod certified_stop_tests;
pub mod demand;
pub mod diag;
pub mod grid;
mod score;
pub mod search;
mod section;

pub use score::{
    score_candidates, section_area_mm2_at, CandidateOutcome, CandidateRefusal,
    CandidateRefusalReason, CandidateTerms, DisplayFaceSummary, DisplayFieldSummary,
    ObjectiveWeights, OrientFaceTable, OrientSearchLimits, ScoreError, ScoreMeter, ScorePlan,
    ScoredCandidates, UpDirection, DISPLAY_DETAIL_TIP_MULTIPLE_ID, DISPLAY_VIEW_DIRECTIONS_ID,
    ORIENT_SCAR_WEIGHT_ID, ORIENT_WEIGHT_HEIGHT_MM_ID, ORIENT_WEIGHT_PEAK_XSEC_CM2_ID,
    ORIENT_WEIGHT_SUPPORT_AREA_CM2_ID, OVERHANG_ANGLE_DEG_ID,
};
pub use search::{DenseSweepRecord, LodRecord, SearchError, SweepOutcome, SweepRequest, EXACT_LOD};

use plinth_core::Finite;
use plinth_decimate::{DecimatedModel, DecimationAudit};
use plinth_mesh::Mesh;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

/// The only orientation policy required on the elimination path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedBaseline {
    pub pipeline_report: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrientRequest {
    pub pinned: PinnedBaseline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaselineAxis {
    X,
    Y,
}

/// The orientation a run selected, however it selected it.
///
/// The first two variants are what a legacy pinned baseline can encode, and the
/// legacy reader that produces them is deliberately untouched by the dense
/// sweep: `orientation_rederivation.rs` is the regression fence for this enum
/// and must keep passing unmodified.
///
/// [`BaselineSelection::UpDirection`] is what the sweep actually chose — the
/// build-up direction in model frame, which is the whole search space (S², not
/// SO(3); see [`score`]). A dense winner drawn from the legacy 13 emits
/// [`BaselineSelection::AxisDegrees`] through the same [`rotation`] builder the
/// pinned path uses, so a searched −30° about X and a pinned −30° about X are
/// the same rotation bit for bit rather than merely equal to within a
/// normalization.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BaselineSelection {
    Identity,
    AxisDegrees {
        axis: BaselineAxis,
        degrees: Finite,
    },
    /// A searched direction. The transform is derived from it by the pinned
    /// shortest-arc convention of [`UpDirection::rotation_to_build_frame`],
    /// whose single antipode is resolved as 180° about `+X`.
    UpDirection {
        up_model_frame: [Finite; 3],
    },
}

/// The chosen rigid transform, including the required post-rotation re-centering translation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RigidTransform {
    pub rotation: [[Finite; 3]; 3],
    pub translation_mm: [Finite; 3],
}

impl RigidTransform {
    pub fn identity() -> Self {
        let zero = Finite::new(0.0).expect("zero is finite");
        let one = Finite::new(1.0).expect("one is finite");
        Self {
            rotation: [[one, zero, zero], [zero, one, zero], [zero, zero, one]],
            translation_mm: [zero; 3],
        }
    }
}

/// Applies the frozen legacy orientation choice and frame restoration.
///
/// The pinned report must contain exactly one hollow stage that ran, plus its authoritative
/// `report.orientation` record. "Ran" means `ok` or `failed`: the legacy writer selects and
/// applies the orientation before the hollow stage self-verifies, so a hollow failure leaves the
/// orientation final and already baked into the committed oriented-solid artifact. A `skipped`
/// hollow stage skipped the orientation search with it and is refused. Identity must be
/// explicitly encoded as a valid zero-degree orientation; missing or skipped stages never
/// silently choose it. Identity still receives frame restoration, matching the legacy
/// implementation.
pub fn run(model: DecimatedModel, request: &OrientRequest) -> Result<OrientedModel, OrientError> {
    let pinned_report = request.pinned.pipeline_report.clone();
    let selection = read_selection(&pinned_report)?;
    let transform = transform_for(selection)?;
    let (mesh, decimation) = model.into_parts();
    let (mesh, transform) = transform_mesh(mesh, transform)?;
    let orientation = OrientationReport {
        provenance: OrientationProvenance::Pinned { pinned_report },
        selection,
        transform,
    };

    Ok(OrientedModel {
        mesh,
        history: OrientationHistory::from_reports(decimation, orientation),
    })
}

/// Searches for this model's build orientation and applies the winner.
///
/// The sweep runs the recorded D-12 funnel over the production mesh it is
/// handed: the whole candidate grid on the ~2 % proxy, the top
/// `ORIENT_REFINE_TOP_K` on the ~10 % proxy, and the top `ORIENT_COMMIT_TOP_K`
/// at `Fidelity::Exact` — the production mesh itself, where the committed score
/// is computed and only there. The winner's full evidence travels in the report
/// as [`OrientationProvenance::DenseSweep`], so the answer is auditable and
/// re-pinnable rather than a bare matrix.
///
/// The result is bit-stable: the grid is a pure function of `(mesh, registered
/// constants)`, every reduction is a serial fold in index order, rayon is used
/// across candidates only as index-parallel map-collect, and the comparator is a
/// first-index scan with no epsilon. Nothing here reads a clock or a seed.
///
/// # Errors
///
/// [`OrientError::Search`] when the production mesh cannot be scored at all —
/// deliberately a refusal, because falling back to identity would be inferring
/// an orientation; [`OrientError::InvalidTransformedMesh`] when the winning
/// rotation does not produce a finite mesh.
pub fn search_and_orient(
    model: DecimatedModel,
    request: &SweepRequest,
    demand: Option<&dyn demand::SupportDemandMeasurer>,
) -> Result<OrientedModel, OrientError> {
    let (oriented, pending) = search_and_orient_deferred(model, request, demand)?;
    pending.publish();
    Ok(oriented)
}

/// [`search_and_orient`] that publishes nothing: the `orient.json` and digest
/// line the search would have written come back as a [`diag::PendingOutput`].
/// The caller publishes it once nothing else can fail the run any more; a run
/// that stops first drops it and leaves no orientation output behind.
///
/// # Errors
///
/// As [`search_and_orient`]; a failure after the search (a non-finite
/// transformed mesh) drops the pending output with it.
pub fn search_and_orient_deferred(
    model: DecimatedModel,
    request: &SweepRequest,
    demand: Option<&dyn demand::SupportDemandMeasurer>,
) -> Result<(OrientedModel, diag::PendingOutput), OrientError> {
    let (mesh, decimation) = model.into_parts();
    let (outcome, pending) =
        search::search_deferred(&mesh, request, demand).map_err(OrientError::Search)?;
    let selection = outcome.winner().selection;
    let transform = transform_for(selection)?;
    let transform_mark = geom_kernel::perf::stage("orient.transform_mesh");
    let (mesh, transform) = transform_mesh(mesh, transform)?;
    drop(transform_mark);
    let orientation = OrientationReport {
        provenance: OrientationProvenance::DenseSweep(Box::new(outcome.into_record())),
        selection,
        transform,
    };
    Ok((
        OrientedModel {
            mesh,
            history: OrientationHistory::from_reports(decimation, orientation),
        },
        pending,
    ))
}

/// Uses the model exactly as drawn: identity rotation, then the same frame
/// restoration every orientation gets (XY-centred, resting at Z=0; placement
/// lifts it off the plate later, as it does every model).
///
/// Nothing is searched and nothing is read from disk: the caller has already
/// decided, from an operator mark, that this model stands as drawn, and
/// `reason` records which mark ([`as_drawn::AsDrawnReason`]). The orientation
/// report says [`BaselineSelection::Identity`] with
/// [`OrientationProvenance::AsDrawn`], so the run report can tell "used as
/// drawn" apart from "searched, and identity won".
///
/// # Errors
///
/// [`OrientError::InvalidTransformedMesh`] when the mesh is not finite after
/// re-centring.
pub fn orient_as_drawn(
    model: DecimatedModel,
    reason: as_drawn::AsDrawnReason,
) -> Result<OrientedModel, OrientError> {
    let selection = BaselineSelection::Identity;
    let transform = transform_for(selection)?;
    let (mesh, decimation) = model.into_parts();
    let (mesh, transform) = transform_mesh(mesh, transform)?;
    let orientation = OrientationReport {
        provenance: OrientationProvenance::AsDrawn { reason },
        selection,
        transform,
    };
    Ok(OrientedModel {
        mesh,
        history: OrientationHistory::from_reports(decimation, orientation),
    })
}

/// The one production sweep configuration, read from the M0 registry.
///
/// Every number the sweep uses is registered, so the pipeline default and the
/// stage default cannot be two different numbers — the `registered_wall`
/// discipline, applied to orientation.
///
/// # Errors
///
/// [`OrientError::Search`] when a registered constant is unavailable or is not
/// the shape the sweep requires.
pub fn registered_sweep_request() -> Result<SweepRequest, OrientError> {
    SweepRequest::registered().map_err(|error| OrientError::Search(SearchError::Score(error)))
}

fn read_selection(path: &Path) -> Result<BaselineSelection, OrientError> {
    let bytes = fs::read(path).map_err(|_| OrientError::PinnedBaselineUnreadable {
        path: path.to_path_buf(),
    })?;
    decode_selection(&bytes, path)
}

fn decode_selection(bytes: &[u8], path: &Path) -> Result<BaselineSelection, OrientError> {
    let report =
        JsonParser::new(bytes)
            .parse()
            .map_err(|_| OrientError::PinnedBaselineInvalid {
                path: path.to_path_buf(),
            })?;
    selection_from_report(&report).ok_or_else(|| OrientError::PinnedBaselineInvalid {
        path: path.to_path_buf(),
    })
}

/// Returns `None` for any malformed required pipeline/stage/orientation record.
#[allow(
    clippy::float_cmp,
    reason = "decoding a stored report's axis triple against the unit-axis \
              literals; 0.0 and 1.0 are exactly representable and anything \
              else is not one of the two baseline axes"
)]
fn selection_from_report(report: &JsonValue) -> Option<BaselineSelection> {
    let stages = report.object_field("stages")?.as_array()?;
    let mut hollow = None;
    for stage in stages {
        let stage = stage.as_object()?;
        let name = object_field(stage, "name")?.as_str()?;
        if name == "hollow" && hollow.replace(stage).is_some() {
            return None;
        }
    }

    let hollow = hollow?;
    // The legacy writer chooses and applies the orientation before the hollow stage's own
    // self-verification runs, so a hollow stage that later `failed` still carries the final,
    // ratified orientation it printed into the oriented-solid artifact. `skipped` is different in
    // kind: skipping the hollow stage skips the orientation search with it, so nothing was chosen
    // and nothing may be inferred.
    let status = object_field(hollow, "status")?.as_str()?;
    if status != "ok" && status != "failed" {
        return None;
    }
    if hollow.iter().any(|(key, _)| key == "log") {
        return None;
    }
    let stage_report = object_field(hollow, "report")?.as_object()?;
    // Belt and braces for the widened status: the legacy report states outright whether the
    // orientation was applied. Where the claim is present it must be `true`, and it must be a
    // boolean; only its total absence is tolerated, for reports predating the field.
    if let Some(oriented) = object_field(stage_report, "oriented") {
        if !oriented.as_bool()? {
            return None;
        }
    }
    let orientation = object_field(stage_report, "orientation")?;
    let orientation = orientation.as_object()?;
    // A searched pose has no axis-and-degrees form. A run that wants to replay one writes
    // `"up": [x, y, z]` — the build-up direction in model frame, exactly as the run's log
    // prints it under `rotation committed` — and the replay is bit for bit the searched pose.
    if let Some(up) = object_field(orientation, "up") {
        return match up.as_array()? {
            [JsonValue::Number(x), JsonValue::Number(y), JsonValue::Number(z)] => {
                Some(BaselineSelection::UpDirection {
                    up_model_frame: [
                        Finite::new(*x).ok()?,
                        Finite::new(*y).ok()?,
                        Finite::new(*z).ok()?,
                    ],
                })
            }
            _ => None,
        };
    }
    let axis = object_field(orientation, "axis")?.as_array()?;
    let axis = match axis {
        [JsonValue::Number(x), JsonValue::Number(y), JsonValue::Number(z)]
            if *x == 1.0 && *y == 0.0 && *z == 0.0 =>
        {
            BaselineAxis::X
        }
        [JsonValue::Number(x), JsonValue::Number(y), JsonValue::Number(z)]
            if *x == 0.0 && *y == 1.0 && *z == 0.0 =>
        {
            BaselineAxis::Y
        }
        _ => return None,
    };
    let degrees = Finite::new(*object_field(orientation, "deg")?.as_number()?).ok()?;
    if degrees.get() == 0.0 {
        Some(BaselineSelection::Identity)
    } else {
        Some(BaselineSelection::AxisDegrees { axis, degrees })
    }
}

pub(crate) fn transform_for(selection: BaselineSelection) -> Result<RigidTransform, OrientError> {
    let rotation = match selection {
        BaselineSelection::Identity => identity_rotation(),
        BaselineSelection::AxisDegrees { axis, degrees } => rotation(axis, degrees.get())?,
        BaselineSelection::UpDirection { up_model_frame } => UpDirection::new([
            up_model_frame[0].get(),
            up_model_frame[1].get(),
            up_model_frame[2].get(),
        ])
        .map_err(|_| OrientError::InvalidTransformedMesh)?
        .rotation_to_build_frame(),
    };
    Ok(RigidTransform {
        rotation: finite_matrix(rotation)?,
        // The true translation depends on the transformed mesh bounds and is filled in by
        // `transform_mesh`; keeping this initialized avoids a second transform representation.
        translation_mm: finite_vector([0.0; 3])?,
    })
}

fn rotation(axis: BaselineAxis, degrees: f64) -> Result<[[f64; 3]; 3], OrientError> {
    let radians = degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    let matrix = match axis {
        BaselineAxis::X => [[1.0, 0.0, 0.0], [0.0, cos, -sin], [0.0, sin, cos]],
        BaselineAxis::Y => [[cos, 0.0, sin], [0.0, 1.0, 0.0], [-sin, 0.0, cos]],
    };
    for row in matrix {
        for value in row {
            if !value.is_finite() {
                return Err(OrientError::InvalidTransformedMesh);
            }
        }
    }
    Ok(matrix)
}

fn identity_rotation() -> [[f64; 3]; 3] {
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
}

/// Orients and recenters a candidate direction exactly as the eventual
/// winner is oriented and recentered at emit — the same [`transform_for`] /
/// [`transform_mesh`] pair `run` and `search_and_orient` apply, behind one
/// narrow public function rather than exposing either directly.
///
/// This exists for [`demand::SupportDemandMeasurer`] implementations (LANE 3,
/// `docs/plans/slice-sweep-core-2026-09-02.md` §12): a candidate's real
/// sweep-measured support demand is only meaningful if it is measured in the
/// SAME frame the candidate would actually be built in. `transform_mesh`
/// itself stays crate-private: it is also the function search proxies are
/// forbidden to reach (`score::search`'s own module doc, "Search proxies can
/// never become production geometry"), and this wrapper returns an owned
/// `Mesh` with no path back to `SearchProxy`, `DecimatedModel` or
/// `OrientedModel`, so that invariant is unaffected by this crate's new
/// public surface.
///
/// # Errors
///
/// [`OrientError::InvalidTransformedMesh`] when the candidate's rotation does
/// not produce a finite mesh.
pub fn oriented_recentered_mesh(
    mesh: &Mesh,
    candidate: score::UpDirection,
) -> Result<Mesh, OrientError> {
    let up_model_frame = finite_vector(candidate.get())?;
    let transform = transform_for(BaselineSelection::UpDirection { up_model_frame })?;
    let (transformed, _transform) = transform_mesh(mesh.clone(), transform)?;
    Ok(transformed)
}

pub(crate) fn transform_mesh(
    mesh: Mesh,
    mut transform: RigidTransform,
) -> Result<(Mesh, RigidTransform), OrientError> {
    let mut transformed = Vec::with_capacity(mesh.verts().len());
    for vertex in mesh.verts() {
        transformed.push(rotate_vertex(transform.rotation, *vertex)?);
    }
    let (min, max) = bounds(&transformed)?;
    let translation = [-(min[0] + max[0]) / 2.0, -(min[1] + max[1]) / 2.0, -min[2]];
    transform.translation_mm = finite_vector(translation)?;
    for vertex in &mut transformed {
        for (coordinate, offset) in vertex.iter_mut().zip(transform.translation_mm) {
            *coordinate = checked_add(*coordinate, offset.get())?;
        }
    }
    let mesh = Mesh::new(transformed, mesh.tris().to_vec())
        .map_err(|_| OrientError::InvalidTransformedMesh)?;
    Ok((mesh, transform))
}

fn rotate_vertex(rotation: [[Finite; 3]; 3], vertex: [f64; 3]) -> Result<[f64; 3], OrientError> {
    let mut result = [0.0; 3];
    for (row, value) in rotation.iter().zip(&mut result) {
        let mut sum = 0.0;
        for (coefficient, coordinate) in row.iter().zip(vertex) {
            let product = coefficient.get() * coordinate;
            if !product.is_finite() {
                return Err(OrientError::InvalidTransformedMesh);
            }
            sum = checked_add(sum, product)?;
        }
        *value = sum;
    }
    Ok(result)
}

fn bounds(vertices: &[[f64; 3]]) -> Result<([f64; 3], [f64; 3]), OrientError> {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for vertex in vertices {
        for axis in 0..3 {
            min[axis] = min[axis].min(vertex[axis]);
            max[axis] = max[axis].max(vertex[axis]);
        }
    }
    if min.into_iter().chain(max).all(f64::is_finite) {
        Ok((min, max))
    } else {
        Err(OrientError::InvalidTransformedMesh)
    }
}

fn checked_add(left: f64, right: f64) -> Result<f64, OrientError> {
    let sum = left + right;
    if sum.is_finite() {
        Ok(sum)
    } else {
        Err(OrientError::InvalidTransformedMesh)
    }
}

fn finite_matrix(matrix: [[f64; 3]; 3]) -> Result<[[Finite; 3]; 3], OrientError> {
    Ok([
        finite_vector(matrix[0])?,
        finite_vector(matrix[1])?,
        finite_vector(matrix[2])?,
    ])
}

fn finite_vector(vector: [f64; 3]) -> Result<[Finite; 3], OrientError> {
    Ok([
        Finite::new(vector[0]).map_err(|_| OrientError::InvalidTransformedMesh)?,
        Finite::new(vector[1]).map_err(|_| OrientError::InvalidTransformedMesh)?,
        Finite::new(vector[2]).map_err(|_| OrientError::InvalidTransformedMesh)?,
    ])
}

/// A deliberately narrow, dependency-free JSON reader for immutable pipeline reports.
///
/// The workspace lock is frozen for this vertical slice. Keeping this reader local avoids a
/// lockfile change merely to decode the four fields this stage owns, while still parsing the
/// entire JSON document rather than searching its text for lookalike keys.
#[derive(Clone, Debug, PartialEq)]
enum JsonValue {
    Object(Vec<(String, JsonValue)>),
    Array(Vec<JsonValue>),
    String(String),
    Number(f64),
    Bool(bool),
    Null,
}

impl JsonValue {
    fn as_object(&self) -> Option<&[(String, JsonValue)]> {
        match self {
            Self::Object(value) => Some(value),
            _ => None,
        }
    }

    fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            Self::Array(value) => Some(value),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    fn as_number(&self) -> Option<&f64> {
        match self {
            Self::Number(value) => Some(value),
            _ => None,
        }
    }

    fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    fn object_field(&self, name: &str) -> Option<&JsonValue> {
        object_field(self.as_object()?, name)
    }
}

fn object_field<'a>(object: &'a [(String, JsonValue)], name: &str) -> Option<&'a JsonValue> {
    let mut found = None;
    for (key, value) in object {
        if key == name && found.replace(value).is_some() {
            return None;
        }
    }
    found
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> JsonParser<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn parse(mut self) -> Result<JsonValue, ()> {
        let value = self.value()?;
        self.whitespace();
        if self.offset == self.bytes.len() {
            Ok(value)
        } else {
            Err(())
        }
    }

    fn value(&mut self) -> Result<JsonValue, ()> {
        self.whitespace();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'\"') => self.string().map(JsonValue::String),
            Some(b'-' | b'0'..=b'9') => self.number().map(JsonValue::Number),
            Some(b't') => self.literal(b"true", JsonValue::Bool(true)),
            Some(b'f') => self.literal(b"false", JsonValue::Bool(false)),
            Some(b'n') => self.literal(b"null", JsonValue::Null),
            _ => Err(()),
        }
    }

    fn object(&mut self) -> Result<JsonValue, ()> {
        self.expect(b'{')?;
        self.whitespace();
        let mut values = Vec::new();
        if self.consume(b'}') {
            return Ok(JsonValue::Object(values));
        }
        loop {
            self.whitespace();
            let key = self.string()?;
            self.whitespace();
            self.expect(b':')?;
            let value = self.value()?;
            values.push((key, value));
            self.whitespace();
            if self.consume(b'}') {
                return Ok(JsonValue::Object(values));
            }
            self.expect(b',')?;
        }
    }

    fn array(&mut self) -> Result<JsonValue, ()> {
        self.expect(b'[')?;
        self.whitespace();
        let mut values = Vec::new();
        if self.consume(b']') {
            return Ok(JsonValue::Array(values));
        }
        loop {
            values.push(self.value()?);
            self.whitespace();
            if self.consume(b']') {
                return Ok(JsonValue::Array(values));
            }
            self.expect(b',')?;
        }
    }

    fn string(&mut self) -> Result<String, ()> {
        self.expect(b'\"')?;
        let mut value = String::new();
        loop {
            let byte = self.advance().ok_or(())?;
            match byte {
                b'\"' => return Ok(value),
                b'\\' => match self.advance().ok_or(())? {
                    b'\"' => value.push('\"'),
                    b'\\' => value.push('\\'),
                    b'/' => value.push('/'),
                    b'b' => value.push('\u{0008}'),
                    b'f' => value.push('\u{000C}'),
                    b'n' => value.push('\n'),
                    b'r' => value.push('\r'),
                    b't' => value.push('\t'),
                    b'u' => value.push(self.unicode_escape()?),
                    _ => return Err(()),
                },
                0..=0x1f => return Err(()),
                _ => {
                    let start = self.offset - 1;
                    let width = utf8_width(byte).ok_or(())?;
                    for _ in 1..width {
                        if !matches!(self.advance(), Some(0x80..=0xbf)) {
                            return Err(());
                        }
                    }
                    value.push_str(
                        std::str::from_utf8(&self.bytes[start..self.offset]).map_err(|_| ())?,
                    );
                }
            }
        }
    }

    fn unicode_escape(&mut self) -> Result<char, ()> {
        let mut code = 0_u32;
        for _ in 0..4 {
            code = code
                .checked_mul(16)
                .and_then(|value| hex(self.advance()?).map(|digit| value + digit))
                .ok_or(())?;
        }
        char::from_u32(code).ok_or(())
    }

    fn number(&mut self) -> Result<f64, ()> {
        let start = self.offset;
        self.consume(b'-');
        if self.consume(b'0') {
            if matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(());
            }
        } else {
            self.digits_one_or_more()?;
        }
        if self.consume(b'.') {
            self.digits_one_or_more()?;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.offset += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.offset += 1;
            }
            self.digits_one_or_more()?;
        }
        std::str::from_utf8(&self.bytes[start..self.offset])
            .map_err(|_| ())?
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .ok_or(())
    }

    fn literal(&mut self, literal: &[u8], value: JsonValue) -> Result<JsonValue, ()> {
        if self.bytes.get(self.offset..self.offset + literal.len()) == Some(literal) {
            self.offset += literal.len();
            Ok(value)
        } else {
            Err(())
        }
    }

    fn digits_one_or_more(&mut self) -> Result<(), ()> {
        let start = self.offset;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.offset += 1;
        }
        if self.offset == start {
            Err(())
        } else {
            Ok(())
        }
    }

    fn whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.offset += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), ()> {
        if self.consume(byte) {
            Ok(())
        } else {
            Err(())
        }
    }

    fn consume(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.offset += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.offset).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.offset += 1;
        Some(byte)
    }
}

fn utf8_width(byte: u8) -> Option<usize> {
    match byte {
        0x00..=0x7f => Some(1),
        0xc2..=0xdf => Some(2),
        0xe0..=0xef => Some(3),
        0xf0..=0xf4 => Some(4),
        _ => None,
    }
}

fn hex(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some(u32::from(byte - b'0')),
        b'a'..=b'f' => Some(u32::from(byte - b'a' + 10)),
        b'A'..=b'F' => Some(u32::from(byte - b'A' + 10)),
        _ => None,
    }
}

/// Where a run's orientation came from.
///
/// A report that only carried the rotation could not distinguish a searched
/// answer from a replayed one, and a searched winner that cannot be told apart
/// from a pin cannot be pinned later either. So the provenance is part of the
/// report rather than something a reader infers from which fields are present.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OrientationProvenance {
    /// The rotation was read out of a frozen baseline report and applied
    /// verbatim.
    Pinned { pinned_report: PathBuf },
    /// The rotation was searched. The record carries the grid, the ladder, the
    /// full ranking at every tier, the fidelity ledger and the `search_digest`
    /// — everything needed to re-derive the same answer or to pin it.
    ///
    /// Boxed because it is two orders of magnitude larger than the pinned arm,
    /// and an enum is as large as its largest variant.
    DenseSweep(Box<DenseSweepRecord>),
    /// Nothing was searched or replayed: the operator marked the model to be
    /// used as drawn, and `reason` says how ([`orient_as_drawn`]).
    AsDrawn { reason: as_drawn::AsDrawnReason },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrientationReport {
    pub provenance: OrientationProvenance,
    pub selection: BaselineSelection,
    pub transform: RigidTransform,
}

/// Accumulated report state after orientation; it contains no historical mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct OrientationHistory {
    decimation: DecimationAudit,
    orientation: OrientationReport,
}

impl OrientationHistory {
    /// Builds passive accumulated report state; this does not construct an oriented model.
    #[must_use]
    pub fn from_reports(decimation: DecimationAudit, orientation: OrientationReport) -> Self {
        Self {
            decimation,
            orientation,
        }
    }

    #[must_use]
    pub const fn decimation(&self) -> &DecimationAudit {
        &self.decimation
    }

    #[must_use]
    pub const fn orientation(&self) -> &OrientationReport {
        &self.orientation
    }

    #[must_use]
    pub fn into_reports(self) -> (DecimationAudit, OrientationReport) {
        (self.decimation, self.orientation)
    }
}

/// Move-only oriented production model.
#[derive(Debug)]
pub struct OrientedModel {
    mesh: Mesh,
    history: OrientationHistory,
}

impl OrientedModel {
    #[must_use]
    pub fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    #[must_use]
    pub const fn history(&self) -> &OrientationHistory {
        &self.history
    }

    /// Moves the transformed sole production mesh and report history into hollowing.
    #[must_use]
    pub fn into_parts(self) -> (Mesh, OrientationHistory) {
        (self.mesh, self.history)
    }
}

#[derive(Clone, Debug, PartialEq, Error)]
pub enum OrientError {
    #[error("cannot read pinned baseline report {path}")]
    PinnedBaselineUnreadable { path: PathBuf },
    #[error("pinned baseline report {path} has an invalid orientation record")]
    PinnedBaselineInvalid { path: PathBuf },
    #[error("the rigid transform produced an invalid production mesh")]
    InvalidTransformedMesh,
    /// The dense sweep could not measure this model. Never a fallback to
    /// identity: identity is a rotation like any other, and choosing it because
    /// the search failed would be inferring an orientation.
    #[error("the dense orientation sweep could not choose an orientation: {0}")]
    Search(SearchError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_decimate::{
        run as run_decimation, DecimationAudit, DecimationBudget, DecimationDisposition,
        DownstreamUtilityEvidence, FinalVerificationResult, ProbeConfig, ReductionRefusalReason,
    };
    use plinth_mesh::{load_binary_stl, WeldPolicy};

    fn baseline(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/baseline")
            .join(name)
    }

    fn parse(input: &str) -> Option<BaselineSelection> {
        selection_from_report(
            &JsonParser::new(input.as_bytes())
                .parse()
                .expect("valid JSON"),
        )
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1e-12,
            "expected {expected}, got {actual}"
        );
    }

    fn fixture_decimation_audit() -> DecimationAudit {
        let mesh = plinth_mesh::make_box([10.0; 3], [0.0; 3]).expect("closed audit fixture");
        let budget =
            DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
                .expect("registered fixture budget");
        let outcome = run_decimation(&mesh, budget).expect("fixture decimation succeeds");
        let (_, audit) = outcome
            .into_decimated_model()
            .expect("fixture scale is in contract")
            .into_parts();
        audit
    }

    #[test]
    fn a_pinned_report_can_replay_a_searched_up_direction_bit_for_bit() {
        let report = br#"{"stages":[{"name":"hollow","status":"ok","report":{"oriented":true,"orientation":{"up":[-0.9949444714882141,-0.08012224773914275,-0.060546875]}}}]}"#;
        let selection = decode_selection(report, Path::new("up.pipeline.json"))
            .expect("an up-direction report decodes");
        assert_eq!(
            selection,
            BaselineSelection::UpDirection {
                up_model_frame: [
                    Finite::new(-0.994_944_471_488_214_1).expect("finite"),
                    Finite::new(-0.080_122_247_739_142_75).expect("finite"),
                    Finite::new(-0.060_546_875).expect("finite"),
                ],
            }
        );
        let short = br#"{"stages":[{"name":"hollow","status":"ok","report":{"orientation":{"up":[0.0,1.0]}}}]}"#;
        assert!(decode_selection(short, Path::new("short.pipeline.json")).is_err());
    }

    #[test]
    fn sphere_baseline_selects_the_frozen_x_rotation_and_real_artifact_is_reframed() {
        let pinned = read_selection(&baseline("Sphere.pipeline.json"))
            .expect("the committed Sphere pipeline report is valid");
        assert_eq!(
            pinned,
            BaselineSelection::AxisDegrees {
                axis: BaselineAxis::X,
                degrees: Finite::new(-20.0).expect("fixture degree is finite"),
            }
        );

        let artifact = load_binary_stl(
            baseline("Sphere.prepped.oriented-solid.stl"),
            WeldPolicy::None,
        )
        .expect("the committed Sphere orientation artifact loads");
        let (min, max) = bounds(artifact.verts()).expect("the artifact is finite");
        assert!((min[0] + max[0]).abs() < 1e-6);
        assert!((min[1] + max[1]).abs() < 1e-6);
        assert!(min[2].abs() < 1e-6);
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn complete_already_print_scale_audit_survives_orientation_handoff() {
        let source = baseline("../models/Sphere.stl");
        let mesh = load_binary_stl(&source, WeldPolicy::None)
            .expect("required real Sphere source fixture must be provisioned");
        let budget =
            DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
                .expect("registered corpus budget");
        let outcome = run_decimation(&mesh, budget).expect("Sphere decimation succeeds");
        let plinth_decimate::BudgetSearchOutcome::AlreadyPrintScale(success) = &outcome else {
            // Sphere is 4,760 faces, below DECIMATION_UTILITY_MIN_INPUT_FACES
            // (32,768), so the registered witness refuses it on its first
            // clause and its frozen census digest cannot move.
            panic!("the Sphere is below the registered face threshold and may not reduce");
        };
        let expected_trace = success.trace().clone();
        let expected_final_probe = success.final_probe().clone();
        let expected_topology_before = success.topology_before();
        let expected_topology_after = success.topology_after();
        let decimated = outcome
            .into_decimated_model()
            .expect("Sphere scale is in contract");
        let oriented = run(
            decimated,
            &OrientRequest {
                pinned: PinnedBaseline {
                    pipeline_report: baseline("Sphere.pipeline.json"),
                },
            },
        )
        .expect("strict pinned orientation succeeds");
        let audit = oriented.history().decimation();

        assert_eq!(
            audit.disposition(),
            DecimationDisposition::AlreadyPrintScale
        );
        assert_eq!(audit.trace(), &expected_trace);
        assert_eq!(audit.final_probe(), &expected_final_probe);
        assert_eq!(audit.topology_before(), expected_topology_before);
        assert_eq!(audit.topology_after(), expected_topology_after);
        assert_eq!(audit.report().chosen_ratio(), 1.0);
        assert_eq!(audit.report().input_faces(), mesh.tris().len());
        assert_eq!(audit.report().output_faces(), mesh.tris().len());
        assert_eq!(audit.report().p99_mm(), 0.0);
        assert_eq!(audit.report().p99_9_mm(), 0.0);

        let trace = audit.trace();
        assert!(!trace.steps.is_empty());
        assert!(trace
            .steps
            .iter()
            .all(|step| !step.candidate_fingerprint.is_empty()));
        assert!(trace
            .final_attempts
            .iter()
            .all(|attempt| !attempt.candidate_fingerprint.is_empty()));
        assert!(trace.final_attempts.iter().all(|attempt| {
            attempt.result != FinalVerificationResult::GeometricallyEligible
                || attempt.downstream_utility
                    == DownstreamUtilityEvidence::NoDownstreamUtilityEvidence
        }));
        if trace
            .final_attempts
            .iter()
            .any(|attempt| attempt.result == FinalVerificationResult::GeometricallyEligible)
        {
            assert!(trace.reduction_refusals.iter().any(|reason| matches!(
                reason,
                ReductionRefusalReason::NoDownstreamUtilityEvidence { .. }
            )));
        }
        assert_eq!(
            trace.adaptive_exploration.complete,
            trace.adaptive_exploration.remaining_intervals.is_empty()
        );
        assert!(trace
            .adaptive_exploration
            .probes
            .iter()
            .all(|probe| !probe.candidate_fingerprint.is_empty()));
    }

    #[test]
    fn missing_skipped_or_orientationless_hollow_stages_are_invalid() {
        assert_eq!(parse(r#"{"stages":[]}"#), None);
        assert_eq!(
            parse(r#"{"stages":[{"name":"hollow","status":"skipped","report":{}}]}"#),
            None
        );
        assert_eq!(
            parse(r#"{"stages":[{"name":"hollow","status":"ok","report":{}}]}"#),
            None
        );
        // A skipped stage never chose an orientation, so even a record it somehow carries is
        // refused; failing later is what widens the gate, skipping is not.
        assert_eq!(
            parse(
                r#"{"stages":[{"name":"hollow","status":"skipped","report":{"oriented":true,"orientation":{"axis":[1,0,0],"deg":-20}}}]}"#
            ),
            None
        );
        assert_eq!(
            parse(r#"{"stages":[{"name":"hollow","status":"failed","report":{}}]}"#),
            None
        );
    }

    #[test]
    fn a_failed_hollow_stage_still_carries_its_ratified_orientation() {
        // The orientation is chosen and applied before the hollow stage self-verifies, so its
        // later failure does not unmake the choice; `oriented: true` is the report saying so.
        let identity = r#"{"stages":[{"name":"hollow","status":"failed","report":{"oriented":true,"orientation":{"axis":[1,0,0],"deg":0}}}]}"#;
        assert_eq!(parse(identity), Some(BaselineSelection::Identity));

        let rotated = r#"{"stages":[{"name":"hollow","status":"failed","report":{"oriented":true,"orientation":{"axis":[0,1,0],"deg":-30}}}]}"#;
        assert_eq!(
            parse(rotated),
            Some(BaselineSelection::AxisDegrees {
                axis: BaselineAxis::Y,
                degrees: Finite::new(-30.0).expect("fixture degree is finite"),
            })
        );
    }

    #[test]
    fn an_unapplied_or_non_boolean_oriented_claim_is_refused() {
        let unapplied = r#"{"stages":[{"name":"hollow","status":"failed","report":{"oriented":false,"orientation":{"axis":[1,0,0],"deg":0}}}]}"#;
        let non_boolean = r#"{"stages":[{"name":"hollow","status":"ok","report":{"oriented":"true","orientation":{"axis":[1,0,0],"deg":0}}}]}"#;
        assert_eq!(parse(unapplied), None);
        assert_eq!(parse(non_boolean), None);
    }

    #[test]
    fn only_the_frozen_report_location_selects_the_pinned_rotation() {
        let log = r#"{"stages":[{"name":"hollow","status":"ok","log":{"orientation":{"axis":[0,1,0],"deg":-45}}}]}"#;
        let report = r#"{"stages":[{"name":"hollow","status":"ok","report":{"orientation":{"axis":[0,1,0],"deg":-45}}}]}"#;
        let expected = Some(BaselineSelection::AxisDegrees {
            axis: BaselineAxis::Y,
            degrees: Finite::new(-45.0).expect("fixture degree is finite"),
        });
        assert_eq!(parse(log), None);
        assert_eq!(parse(report), expected);
    }

    #[test]
    fn explicit_valid_zero_rotation_selects_identity() {
        let report = r#"{"stages":[{"name":"hollow","status":"ok","report":{"orientation":{"axis":[1,0,0],"deg":0,"score":1.0}}}]}"#;
        assert_eq!(parse(report), Some(BaselineSelection::Identity));
    }

    #[test]
    fn only_the_frozen_axes_and_finite_degrees_are_accepted() {
        let invalid_axis = r#"{"stages":[{"name":"hollow","status":"ok","report":{"orientation":{"axis":[0,0,1],"deg":-30}}}]}"#;
        let invalid_degrees = r#"{"stages":[{"name":"hollow","status":"ok","report":{"orientation":{"axis":[1,0,0],"deg":"-45"}}}]}"#;
        assert_eq!(parse(invalid_axis), None);
        assert_eq!(parse(invalid_degrees), None);
    }

    #[test]
    fn conflicting_log_and_report_wrappers_are_invalid() {
        let report = r#"{"stages":[{"name":"hollow","status":"ok","log":{},"report":{}}]}"#;
        assert_eq!(parse(report), None);
    }

    #[test]
    fn malformed_or_unreadable_reports_are_typed_failures() {
        let missing = PathBuf::from("this-pinned-baseline-does-not-exist.pipeline.json");
        assert_eq!(
            read_selection(&missing),
            Err(OrientError::PinnedBaselineUnreadable { path: missing })
        );

        let path = Path::new("baseline/invalid.pipeline.json");
        let invalid_reports: &[&[u8]] = &[
            br#"{"stages":[}"#,
            br#"{"stages":{"name":"hollow"}}"#,
            br#"{"stages":[]}"#,
            br#"{"stages":[{"name":"hollow","status":"skipped","report":{}}]}"#,
            br#"{"stages":[{"name":"hollow","status":"skipped","report":{"oriented":true,"orientation":{"axis":[1,0,0],"deg":0}}}]}"#,
            br#"{"stages":[{"name":"hollow","status":"ok","report":{}}]}"#,
            br#"{"stages":[{"name":"hollow","status":"failed","report":{}}]}"#,
            br#"{"stages":[{"name":"hollow","status":"failed","report":{"oriented":false,"orientation":{"axis":[1,0,0],"deg":0}}}]}"#,
            br#"{"stages":[{"name":"hollow","status":"ok","log":{"orientation":{"axis":[1,0,0],"deg":0}}}]}"#,
            br#"{"stages":[{"name":"hollow","status":"ok","log":{},"report":{"orientation":{"axis":[1,0,0],"deg":0}}}]}"#,
            br#"{"stages":[{"name":"hollow","status":"ok","report":{"orientation":{"axis":[0,0,1],"deg":0}}}]}"#,
            br#"{"stages":[{"name":"hollow","status":"ok","report":{"orientation":{"axis":[1,0,0],"deg":"0"}}}]}"#,
            br#"{"stages":[{"name":"hollow","status":"ok","report":{"orientation":{"axis":[1,0,0],"deg":0}}},{"name":"hollow","status":"ok","report":{"orientation":{"axis":[1,0,0],"deg":0}}}]}"#,
        ];
        for report in invalid_reports {
            assert_eq!(
                decode_selection(report, path),
                Err(OrientError::PinnedBaselineInvalid {
                    path: path.to_path_buf(),
                })
            );
        }
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn rotation_and_reframing_record_the_exact_matrix_and_translation() {
        let mesh = Mesh::new(
            vec![[3.0, 5.0, -2.0], [7.0, 5.0, -2.0], [3.0, 9.0, 4.0]],
            vec![[0, 1, 2]],
        )
        .expect("fixture mesh is valid");
        let transform = transform_for(BaselineSelection::Identity).expect("identity is valid");
        let (mesh, transform) = transform_mesh(mesh, transform).expect("fixture transforms");
        assert_eq!(transform.translation_mm.map(Finite::get), [-5.0, -7.0, 2.0]);
        assert_eq!(mesh.verts()[0], [-2.0, -2.0, 0.0]);
        assert_eq!(mesh.verts()[1], [2.0, -2.0, 0.0]);
        assert_eq!(mesh.verts()[2], [-2.0, 2.0, 6.0]);

        let selection = BaselineSelection::AxisDegrees {
            axis: BaselineAxis::Y,
            degrees: Finite::new(-30.0).expect("fixture degree is finite"),
        };
        let transform = transform_for(selection).expect("rotation is valid");
        assert_close(transform.rotation[0][0].get(), 3.0_f64.sqrt() / 2.0);
        assert_close(transform.rotation[0][2].get(), -0.5);
        assert_close(transform.rotation[2][0].get(), 0.5);
    }

    #[test]
    fn consuming_transition_keeps_only_the_transformed_mesh() {
        let mesh = Mesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .expect("the test triangle is a valid mesh");
        let allocation = mesh.verts().as_ptr();
        let decimation = fixture_decimation_audit();
        let orientation = OrientationReport {
            provenance: OrientationProvenance::Pinned {
                pinned_report: PathBuf::from("baseline/model.pipeline.json"),
            },
            selection: BaselineSelection::Identity,
            transform: RigidTransform::identity(),
        };
        let model = OrientedModel {
            mesh,
            history: OrientationHistory::from_reports(decimation, orientation),
        };
        let (mesh, history) = model.into_parts();
        assert_eq!(mesh.verts().as_ptr(), allocation);
        assert_eq!(
            history.decimation().disposition(),
            DecimationDisposition::AlreadyPrintScale
        );
        assert_eq!(history.orientation().selection, BaselineSelection::Identity);
    }
}
