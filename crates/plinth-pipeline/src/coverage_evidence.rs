//! FU-57: persist evidence on every remaining coverage refusal.
//!
//! **The ruling** (`docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`,
//! §2): a coverage refusal keeps its evidence, in three forms — a sidecar
//! `<model>.coverage.json` beside the would-be artifact path, a CLI card
//! summarizing the worst islands, and deterministic renders of the worst
//! uncovered islands. **All three are evidence only, and never a gate
//! input**: this module is never imported by `gates.rs`, and
//! `judge_pre_write_gates` stays private and unchanged. A failure to write
//! any piece of evidence is recorded as a string and never turns a refusal
//! into a failure or vice versa — every function here that touches disk
//! returns its result inline on [`CoverageRefusalEvidence`], not as a
//! `Result` a caller could let propagate.
//!
//! **Two refusal sources, one schema.** Before FU-56 there was exactly one
//! coverage refusal: the pre-write gate's [`plinth_verify::CoverageReport`].
//! FU-56 added a second: [`plinth_support::CoverageClosureRefusal`], raised
//! by the coverage-closure controller inside `run_support` when it cannot
//! close every measured island within the registered attempt bound. Both are
//! "a coverage refusal" in the ruling's sense, and both are served by
//! [`EvidenceSource`] and [`CoverageRefusalEvidence`] from day one, so the
//! sidecar schema never has to change shape when a source's coverage story
//! changes.
//!
//! **No re-derivation.** The per-island ranking is built from data the
//! refusing source already measured: [`plinth_verify::CoverageReport::defects`]
//! for the gate ([`plinth_core::SiteId::Island`] carries the id, `measure`
//! the summed uncovered area), and [`plinth_support::CoverageClosureIsland`]
//! for the controller. Per-patch face membership and worst-gap, needed only
//! by the renders, are not on `CoverageReport`'s public accessor surface;
//! rather than add one and touch `plinth-verify` (left alone by FU-56 for the
//! same reason), this module round-trips the report through
//! `serde_json::to_value` and reads the same private-field JSON shape the
//! type already serializes -- the shape `CoverageReport`'s own roundtrip test
//! pins. If that shape is ever missing (it never legitimately is: every field
//! read here is written unconditionally by `plinth_verify`), the fallback is
//! an island with a summed area and no face detail, never a panic.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use plinth_core::{ModelId, SiteId};
use plinth_support::{CoverageClosureGap, CoverageClosureRefusal};
use plinth_verify::CoverageReport;
use serde::{Deserialize, Serialize};

use crate::{PreWriteGate, SupportedGeometry};

/// The wire schema tag written into every sidecar. Bumped only if the shape
/// changes in a way an old reader could misparse.
const SIDECAR_SCHEMA: &str = "plinth.coverage-refusal.v1";

/// Whole-mesh face bound above which a render degrades to a bounding-box
/// silhouette plus the highlighted island faces only, rather than drawing
/// every face in the model.
///
/// **A new admitted bound, not a physics constant.** It protects the
/// evidence writer's own output size and wall time; it does not gate
/// emission, is not read anywhere near `judge_pre_write_gates`, and moves no
/// existing cap. The value is this change's own, not owner-derived: bounded
/// generously enough that essentially every corpus mini (a "150k-face mini"
/// is the largest cited in the ledger) still renders full-fidelity per
/// island at typical island sizes, while a pathological island selection
/// cannot blow the sidecar past a few megabytes.
pub const COVERAGE_RENDER_MAX_FACES: usize = 20_000;

/// How many of the worst islands get a render each. Bounds total evidence
/// size on a refusal with dozens of blocking islands (the ledger's own
/// example: `Giant Toad`, 34).
pub const COVERAGE_RENDER_MAX_ISLANDS: usize = 5;

/// Safety bound on how many of one island's own faces get drawn, independent
/// of [`COVERAGE_RENDER_MAX_FACES`]. Real measured islands are tens to a few
/// hundred faces; this exists so a pathological island cannot blow one SVG's
/// size even when the whole mesh is small enough to render full-fidelity.
const MAX_HIGHLIGHT_FACES_PER_ISLAND: usize = 5_000;

/// Which refusal produced this evidence. Serialized so a reader of the
/// sidecar alone -- without the run report -- knows which typed refusal
/// `report`/`closure` embeds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSource {
    /// `GateRefusal::Refused` at the pre-write gate.
    PreWriteGate,
    /// `SupportRunError::CoverageClosure` from the FU-56 controller.
    CoverageClosureController,
}

/// One island's evidence, ranked worst-first in
/// [`CoverageRefusalEvidence::islands`]: uncovered area descending, then
/// island id ascending, so two builds of the same fixture order identically
/// even when two islands tie on area.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UncoveredIslandSummary {
    pub island_id: u32,
    pub uncovered_area_mm2: f64,
    pub worst_gap: CoverageClosureGap,
    /// Sorted, deduplicated face ids belonging to this island's blocking
    /// patches -- exactly what the render highlights, no neighbour bleed.
    pub face_ids: Vec<u32>,
}

impl UncoveredIslandSummary {
    #[must_use]
    pub fn face_count(&self) -> usize {
        self.face_ids.len()
    }
}

/// Everything one coverage refusal's evidence persistence produced.
///
/// **Write failures are data, not control flow.** `render_skipped` and
/// `write_error` record what could not be written; nothing here is a
/// `Result` a caller could propagate into the disposition. See the module
/// doc.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoverageRefusalEvidence {
    pub source: EvidenceSource,
    pub gate: Option<PreWriteGate>,
    pub geometry: Option<SupportedGeometry>,
    /// `Some` only for [`EvidenceSource::CoverageClosureController`]: the
    /// attempt the controller stopped at.
    pub attempts: Option<usize>,
    /// Every blocking island, worst-first, not merely the rendered subset --
    /// this is what the card truncates for display and the sidecar carries
    /// in full.
    pub islands: Vec<UncoveredIslandSummary>,
    pub sidecar_path: PathBuf,
    pub render_paths: Vec<PathBuf>,
    pub render_skipped: Option<String>,
    pub write_error: Option<String>,
    /// `Some` when the refusal is a coverage MEASUREMENT failure rather than
    /// a coverage verdict — the gate's `GateRefusal::Unmeasurable`, or the
    /// controller's `CoverageClosureReason::MeasurementRefused`. There is no
    /// `CoverageReport` to embed in that case and no islands to rank, but
    /// there is still a coverage refusal, and FU-57 says a coverage refusal
    /// keeps its evidence. The string is the measurement error as the
    /// refusing side reported it.
    #[serde(default)]
    pub measurement_error: Option<String>,
}

/// The flat vertex/triangle data a render needs, captured from the prepared
/// mesh before `run_support` consumes the [`plinth_hollow::HollowedModel`]
/// that owns it.
///
/// **Why this exists at all.** `run_support` takes `HollowedModel` by value,
/// and a coverage-closure refusal is only knowable from its `Err`, by which
/// point the mesh is gone. The clone below is therefore paid on every model,
/// not only refused ones -- a real, documented cost, flagged rather than
/// hidden.
///
/// **Two remedies were tried on paper and neither works today (2026-08-12),
/// which is why this is a follow-up row and not a fix.**
///
/// 1. *Return the mesh inside the refusal.* `SupportRunError::CoverageClosure`
///    could carry it, and at the `AttemptsExhausted` / `NoEligibleRepairRows`
///    exits the lowered run is still alive, so the mesh is in hand. It is NOT
///    in hand at the `RepairPlanRefused` exits: the runner's typestate MOVES
///    the context into each stage (`prepared_run_for_plan(context, ...)`,
///    `LoweredSupportRun::into_context`) precisely so the `NativeSurface` grid
///    is never duplicated, so a stage failure consumes the context and the
///    mesh with it. Carrying the mesh there means restructuring every stage
///    to hand the context back on failure — a much larger change than this
///    cost justifies, and one that touches the memory discipline it is trying
///    to protect.
/// 2. *Take the mesh from the sealed snapshot instead.* That works for the
///    controller path but not for the pre-write GATE refusal, which happens
///    inside `judge_then_emit` after the authority has been handed over — the
///    caller has no borrow left to copy from.
///
/// So the copy stays, deliberately, with both dead ends recorded so the
/// follow-up starts from what was already ruled out. For scale: it is
/// `verts × 24 + tris × 12` bytes — a few megabytes on a 150k-face mini,
/// against the `max_grid_bytes` ceiling of 64 MiB the same phase already
/// carries.
#[derive(Clone, Debug)]
pub struct RenderMesh {
    pub verts: Vec<[f64; 3]>,
    pub tris: Vec<[u32; 3]>,
}

impl RenderMesh {
    #[must_use]
    pub fn from_mesh(mesh: &plinth_mesh::Mesh) -> Self {
        Self {
            verts: mesh.verts().to_vec(),
            tris: mesh.tris().to_vec(),
        }
    }
}

/// The sidecar path for one model: `<target-stem>.coverage.json`, beside
/// (never at) the `.chitubox` target.
#[must_use]
pub fn sidecar_path(target: &Path) -> PathBuf {
    target.with_extension("coverage.json")
}

/// One island's render path: `<target-stem>.coverage.island-<id>.svg`.
#[must_use]
pub fn render_path(target: &Path, island_id: u32) -> PathBuf {
    target.with_extension(format!("coverage.island-{island_id}.svg"))
}

/// The filename prefix every render for this target shares, for stale-file
/// clearing by prefix/suffix match rather than shell-style globbing (a model
/// name may contain glob metacharacters).
#[must_use]
pub fn render_file_prefix(target: &Path) -> Option<String> {
    let stem = target.file_stem()?.to_str()?;
    Some(format!("{stem}.coverage.island-"))
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

fn rank_worst_first(islands: &mut [UncoveredIslandSummary]) {
    islands.sort_by(|left, right| {
        right
            .uncovered_area_mm2
            .total_cmp(&left.uncovered_area_mm2)
            .then_with(|| left.island_id.cmp(&right.island_id))
    });
}

// --- Gate-source (`CoverageReport`) island extraction -----------------

/// The private-field JSON shape `plinth_verify::CoverageReport` serializes,
/// read back only for the fields the render needs. See the module doc for
/// why this exists instead of a new `plinth-verify` accessor.
#[derive(Deserialize, Default)]
struct WireReport {
    #[serde(default)]
    request: WireRequest,
    #[serde(default)]
    evidence: WireEvidence,
}

/// The one request field this render needs: the A3 censor the report's own
/// measurement used, so the patch filter below censors at exactly the area the
/// verdict censored at. Defaulting to 0.0 is the FAIL-CLOSED choice — a report
/// whose request could not be read censors nothing, which can only ever admit
/// MORE patch detail for islands the verifier already flagged, never fewer.
/// This path renders evidence for an existing verdict; it never makes one.
#[derive(Deserialize, Default)]
struct WireRequest {
    #[serde(default)]
    patch_area_floor_mm2: f64,
}

#[derive(Deserialize, Default)]
struct WireEvidence {
    #[serde(default)]
    patches: Vec<WirePatch>,
}

#[derive(Deserialize)]
struct WirePatch {
    island_id: u32,
    #[serde(default)]
    sorted_face_ids: Vec<u32>,
    uncovered_area_mm2: f64,
    #[serde(default)]
    cut_ambiguous: bool,
    #[serde(default)]
    assessment_failed: bool,
    worst_gap: WireGap,
}

#[derive(Deserialize)]
#[serde(tag = "state", content = "gap_mm", rename_all = "snake_case")]
enum WireGap {
    Finite(f64),
    NoRoutedContact,
}

impl From<WireGap> for CoverageClosureGap {
    fn from(gap: WireGap) -> Self {
        match gap {
            WireGap::Finite(value) => Self::Finite(value),
            WireGap::NoRoutedContact => Self::NoRoutedContact,
        }
    }
}

/// Rank the gate's blocking islands worst-first.
///
/// The per-island uncovered area comes from `report.defects()` -- the
/// verifier's own authoritative sum -- and is never recomputed. Only the
/// worst gap and face membership, absent from `Defect`, are recovered from
/// the serialized patch evidence.
#[must_use]
pub fn islands_from_gate_report(report: &CoverageReport) -> Vec<UncoveredIslandSummary> {
    let mut areas: BTreeMap<u32, f64> = BTreeMap::new();
    for defect in report.defects() {
        if let SiteId::Island(island_id) = defect.site {
            areas.insert(island_id, defect.measure.get());
        }
    }
    if areas.is_empty() {
        return Vec::new();
    }

    let wire: WireReport = serde_json::to_value(report)
        .ok()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();

    let mut gaps: BTreeMap<u32, CoverageClosureGap> = BTreeMap::new();
    let mut faces: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for patch in &wire.evidence.patches {
        if !areas.contains_key(&patch.island_id) {
            continue;
        }
        if !plinth_support::coverage_patch_blocks_verdict(
            patch.uncovered_area_mm2,
            patch.cut_ambiguous,
            patch.assessment_failed,
            wire.request.patch_area_floor_mm2,
        ) {
            continue;
        }
        let gap: CoverageClosureGap = match patch.worst_gap {
            WireGap::Finite(value) => CoverageClosureGap::Finite(value),
            WireGap::NoRoutedContact => CoverageClosureGap::NoRoutedContact,
        };
        gaps.entry(patch.island_id)
            .and_modify(|existing| *existing = worse_gap(*existing, gap))
            .or_insert(gap);
        faces
            .entry(patch.island_id)
            .or_default()
            .extend(patch.sorted_face_ids.iter().copied());
    }

    let mut islands: Vec<UncoveredIslandSummary> = areas
        .into_iter()
        .map(|(island_id, uncovered_area_mm2)| UncoveredIslandSummary {
            island_id,
            uncovered_area_mm2,
            worst_gap: gaps
                .get(&island_id)
                .copied()
                .unwrap_or(CoverageClosureGap::NoRoutedContact),
            face_ids: faces
                .remove(&island_id)
                .map(|set| set.into_iter().collect())
                .unwrap_or_default(),
        })
        .collect();
    rank_worst_first(&mut islands);
    islands
}

/// Rank the coverage-closure controller's still-blocking islands worst-first,
/// merging [`plinth_support::CoverageClosureIsland`] rows that share an
/// island id across different components -- the same per-island framing the
/// gate side and the card both use.
#[must_use]
pub fn islands_from_closure(refusal: &CoverageClosureRefusal) -> Vec<UncoveredIslandSummary> {
    let mut areas: BTreeMap<u32, f64> = BTreeMap::new();
    let mut gaps: BTreeMap<u32, CoverageClosureGap> = BTreeMap::new();
    let mut faces: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for island in &refusal.islands {
        *areas.entry(island.island_id).or_insert(0.0) += island.uncovered_area_mm2;
        gaps.entry(island.island_id)
            .and_modify(|existing| *existing = worse_gap(*existing, island.worst_gap))
            .or_insert(island.worst_gap);
        faces
            .entry(island.island_id)
            .or_default()
            .extend(island.sorted_face_ids.iter().copied());
    }
    let mut islands: Vec<UncoveredIslandSummary> = areas
        .into_iter()
        .map(|(island_id, uncovered_area_mm2)| UncoveredIslandSummary {
            island_id,
            uncovered_area_mm2,
            worst_gap: gaps
                .remove(&island_id)
                .unwrap_or(CoverageClosureGap::NoRoutedContact),
            face_ids: faces
                .remove(&island_id)
                .map(|set| set.into_iter().collect())
                .unwrap_or_default(),
        })
        .collect();
    rank_worst_first(&mut islands);
    islands
}

// --- Sidecar -----------------------------------------------------------

#[derive(Serialize)]
struct CoverageSidecarBody<'a> {
    schema: &'a str,
    model: &'a str,
    source: EvidenceSource,
    gate: Option<PreWriteGate>,
    geometry: Option<&'a SupportedGeometry>,
    attempts: Option<usize>,
    islands: &'a [UncoveredIslandSummary],
    renders: Vec<String>,
    render_skipped: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    measurement_error: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    report: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    closure: Option<serde_json::Value>,
}

/// Everything [`write_sidecar`] needs, bundled rather than passed as eleven
/// positional arguments.
struct SidecarWrite<'a> {
    model: &'a str,
    source: EvidenceSource,
    gate: Option<PreWriteGate>,
    geometry: Option<&'a SupportedGeometry>,
    attempts: Option<usize>,
    islands: &'a [UncoveredIslandSummary],
    render_paths: &'a [PathBuf],
    render_skipped: Option<&'a str>,
    measurement_error: Option<&'a str>,
    report: Option<&'a CoverageReport>,
    closure: Option<&'a CoverageClosureRefusal>,
}

fn write_sidecar(path: &Path, inputs: SidecarWrite<'_>) -> Result<(), String> {
    let renders = inputs
        .render_paths
        .iter()
        .map(|path| {
            path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            )
        })
        .collect();
    let body = CoverageSidecarBody {
        schema: SIDECAR_SCHEMA,
        model: inputs.model,
        source: inputs.source,
        gate: inputs.gate,
        geometry: inputs.geometry,
        attempts: inputs.attempts,
        islands: inputs.islands,
        renders,
        render_skipped: inputs.render_skipped,
        measurement_error: inputs.measurement_error,
        // Embedded verbatim via `serde_json::to_value`: no re-derivation, no
        // second format, and it deserializes back into the original typed
        // report/closure exactly (both are `Serialize + Deserialize`).
        report: inputs.report.and_then(|report| serde_json::to_value(report).ok()),
        closure: inputs
            .closure
            .and_then(|closure| serde_json::to_value(closure).ok()),
    };
    let bytes = serde_json::to_vec_pretty(&body)
        .map_err(|error| format!("coverage sidecar could not be serialized: {error}"))?;
    std::fs::write(path, bytes).map_err(|error| {
        format!(
            "coverage sidecar could not be written to {}: {error}",
            path.display()
        )
    })
}

// --- Renders -------------------------------------------------------------

const CANVAS_W: f64 = 480.0;
const CANVAS_H: f64 = 360.0;
const PANEL_PAD: f64 = 16.0;

/// One projected, depth-sorted triangle ready to draw.
struct Projected {
    depth: f64,
    points: [(f64, f64); 3],
    highlighted: bool,
}

fn bbox(points: impl Iterator<Item = (f64, f64)>) -> (f64, f64, f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for (x, y) in points {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    if !min_x.is_finite() || !max_x.is_finite() || !min_y.is_finite() || !max_y.is_finite() {
        return (0.0, 1.0, 0.0, 1.0);
    }
    if (max_x - min_x).abs() < f64::EPSILON {
        max_x = min_x + 1.0;
    }
    if (max_y - min_y).abs() < f64::EPSILON {
        max_y = min_y + 1.0;
    }
    (min_x, max_x, min_y, max_y)
}

/// Fixed 3-decimal formatting: deterministic across platforms and compact
/// enough to keep the SVG small.
fn fmt3(value: f64) -> String {
    format!("{value:.3}")
}

/// Project one panel (a 2-D view of `(verts, tris)`, `project` mapping a
/// vertex to `(screen_x, screen_y, depth)`) into one `<g>` of painter's-order
/// triangles, plus the panel's own viewBox transform.
#[allow(clippy::too_many_arguments)]
fn render_panel(
    out: &mut String,
    title: &str,
    origin_x: f64,
    verts: &[[f64; 3]],
    tris: &[[u32; 3]],
    highlighted: &BTreeSet<u32>,
    project: impl Fn(&[f64; 3]) -> (f64, f64, f64),
    degrade: bool,
) {
    let mut projected: Vec<Projected> = Vec::new();
    let mut screen_bbox_points: Vec<(f64, f64)> = Vec::with_capacity(verts.len());
    for vertex in verts {
        let (x, y, _) = project(vertex);
        screen_bbox_points.push((x, y));
    }
    let (min_x, max_x, min_y, max_y) = bbox(screen_bbox_points.into_iter());
    let span_x = (max_x - min_x).max(f64::EPSILON);
    let span_y = (max_y - min_y).max(f64::EPSILON);
    let inner_w = CANVAS_W - 2.0 * PANEL_PAD;
    let inner_h = CANVAS_H - 2.0 * PANEL_PAD - 24.0; // room for the title row
    let scale = (inner_w / span_x).min(inner_h / span_y);

    let to_screen = |x: f64, y: f64| -> (f64, f64) {
        let screen_x = origin_x + PANEL_PAD + (x - min_x) * scale;
        // SVG y grows downward; flip so larger model-space y is higher on
        // the page.
        let screen_y = PANEL_PAD + 24.0 + (max_y - y) * scale;
        (screen_x, screen_y)
    };

    if !degrade {
        for (face_id, tri) in tris.iter().enumerate() {
            let face_id = face_id as u32;
            let mut points = [(0.0, 0.0); 3];
            let mut depth_sum = 0.0;
            for (slot, vertex_index) in tri.iter().enumerate() {
                let Some(vertex) = verts.get(*vertex_index as usize) else {
                    continue;
                };
                let (x, y, depth) = project(vertex);
                points[slot] = to_screen(x, y);
                depth_sum += depth;
            }
            projected.push(Projected {
                depth: depth_sum / 3.0,
                points,
                highlighted: highlighted.contains(&face_id),
            });
        }
    } else {
        // Degraded: a silhouette rectangle for the whole mesh, plus the
        // highlighted island's own faces only.
        let (screen_min_x, screen_min_y) = to_screen(min_x, min_y);
        let (screen_max_x, screen_max_y) = to_screen(max_x, max_y);
        let _ = writeln!(
            out,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" class=\"silhouette\"/>",
            fmt3(screen_min_x.min(screen_max_x)),
            fmt3(screen_min_y.min(screen_max_y)),
            fmt3((screen_max_x - screen_min_x).abs()),
            fmt3((screen_max_y - screen_min_y).abs()),
        );
        for (face_id, tri) in tris.iter().enumerate() {
            let face_id = face_id as u32;
            if !highlighted.contains(&face_id) {
                continue;
            }
            let mut points = [(0.0, 0.0); 3];
            let mut depth_sum = 0.0;
            for (slot, vertex_index) in tri.iter().enumerate() {
                let Some(vertex) = verts.get(*vertex_index as usize) else {
                    continue;
                };
                let (x, y, depth) = project(vertex);
                points[slot] = to_screen(x, y);
                depth_sum += depth;
            }
            projected.push(Projected {
                depth: depth_sum / 3.0,
                points,
                highlighted: true,
            });
        }
    }

    // Painter's order: farthest first, by a total comparator so two runs
    // over the same mesh draw in the same order — but the HIGHLIGHTED faces
    // are drawn after every neutral face regardless of depth.
    //
    // **Why highlighted-last rather than strict depth order.** An uncovered
    // support island is by construction a DOWN-facing surface, so in the
    // plate view (looking down -Z) there is always solid geometry above it:
    // strict painter's order draws the red patch first and then paints the
    // model's top surface over it at identical screen coordinates, and the
    // render shows the owner a picture with no red in it at all. The panel is
    // evidence, not a rendering of what a camera would see; the ruling's
    // stated purpose is that the owner can SEE what could not be covered
    // (2026-08-12 rulings §2), and a hidden patch fails that purpose
    // completely. Depth order is still total within each class, so the
    // neutral shell reads correctly and two runs stay byte-identical.
    projected.sort_by(|left, right| {
        left.highlighted
            .cmp(&right.highlighted)
            .then_with(|| left.depth.total_cmp(&right.depth))
    });

    let _ = writeln!(
        out,
        "<text x=\"{}\" y=\"14\" class=\"title\">{}</text>",
        fmt3(origin_x + PANEL_PAD),
        xml_escape(title)
    );
    for triangle in &projected {
        let class = if triangle.highlighted {
            "uncovered"
        } else {
            "neutral"
        };
        let _ = writeln!(
            out,
            "<polygon points=\"{},{} {},{} {},{}\" class=\"{class}\"/>",
            fmt3(triangle.points[0].0),
            fmt3(triangle.points[0].1),
            fmt3(triangle.points[1].0),
            fmt3(triangle.points[1].1),
            fmt3(triangle.points[2].0),
            fmt3(triangle.points[2].1),
        );
    }
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The view direction that shows the highlighted patch at its largest: the
/// patch's own area-weighted average face normal, as an orthonormal basis
/// `(u, v, n)` with `n` the view axis.
///
/// **Why a third, island-derived projection exists at all.** An uncovered
/// support island is a DOWN-facing surface, and a normal horizontal underside
/// is exactly edge-on in any fixed elevation: it projects to a degenerate,
/// zero-area sliver and the render shows the owner nothing. The two fixed
/// panels answer "where on the model is this", which is worth having; only a
/// projection derived from the patch itself can answer "what shape is the
/// uncovered region", which is the half of the ruling's purpose the fixed
/// panels structurally cannot serve.
///
/// `None` when the highlighted faces carry no usable normal (no highlighted
/// faces, or their area-weighted normals cancel exactly), in which case the
/// caller falls back to the plate axis rather than rendering a basis built
/// from noise.
fn island_view_basis(
    mesh: &RenderMesh,
    highlighted: &BTreeSet<u32>,
) -> Option<([f64; 3], [f64; 3], [f64; 3])> {
    let mut normal = [0.0_f64; 3];
    for &face_id in highlighted {
        let Some(tri) = mesh.tris.get(face_id as usize) else {
            continue;
        };
        let (Some(a), Some(b), Some(c)) = (
            mesh.verts.get(tri[0] as usize),
            mesh.verts.get(tri[1] as usize),
            mesh.verts.get(tri[2] as usize),
        ) else {
            continue;
        };
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        // The cross product's magnitude is twice the triangle's area, so
        // summing raw cross products IS the area weighting -- no per-face
        // normalization, no division, and deterministic in the highlighted
        // set's own (sorted) order.
        normal[0] += ab[1].mul_add(ac[2], -(ab[2] * ac[1]));
        normal[1] += ab[2].mul_add(ac[0], -(ab[0] * ac[2]));
        normal[2] += ab[0].mul_add(ac[1], -(ab[1] * ac[0]));
    }
    let length = normal
        .iter()
        .fold(0.0_f64, |sum, value| value.mul_add(*value, sum))
        .sqrt();
    if !length.is_finite() || length <= f64::EPSILON {
        return None;
    }
    let n = [normal[0] / length, normal[1] / length, normal[2] / length];
    // Any axis not parallel to `n` seeds the basis; picking the smallest
    // component keeps the cross product well conditioned.
    let helper: [f64; 3] = if n[0].abs() <= n[1].abs() && n[0].abs() <= n[2].abs() {
        [1.0, 0.0, 0.0]
    } else if n[1].abs() <= n[2].abs() {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let mut u = [
        helper[1].mul_add(n[2], -(helper[2] * n[1])),
        helper[2].mul_add(n[0], -(helper[0] * n[2])),
        helper[0].mul_add(n[1], -(helper[1] * n[0])),
    ];
    let u_length = u
        .iter()
        .fold(0.0_f64, |sum, value| value.mul_add(*value, sum))
        .sqrt();
    if !u_length.is_finite() || u_length <= f64::EPSILON {
        return None;
    }
    u = [u[0] / u_length, u[1] / u_length, u[2] / u_length];
    let v = [
        n[1].mul_add(u[2], -(n[2] * u[1])),
        n[2].mul_add(u[0], -(n[0] * u[2])),
        n[0].mul_add(u[1], -(n[1] * u[0])),
    ];
    Some((u, v, n))
}

fn dot(left: [f64; 3], right: &[f64; 3]) -> f64 {
    left[0].mul_add(right[0], left[1].mul_add(right[1], left[2] * right[2]))
}

/// Render one island's evidence as a deterministic, self-contained SVG: three
/// orthographic panels — plate view (down -Z), +X elevation, and a third
/// panel projected along the ISLAND'S OWN average normal — a neutral fill for
/// every face and a warning fill for the island's own uncovered-patch faces,
/// painter's-ordered by a total comparator so two runs on the same fixture
/// produce byte-identical output.
///
/// **The uncovered patch is always visible, in every panel.** Two structural
/// facts about uncovered support islands defeat a naive render, and both are
/// answered here rather than left to luck: they are down-facing, so the model
/// itself occludes them in the plate view (answered by drawing the
/// highlighted faces after every neutral face — see `render_panel`); and they
/// are usually near-horizontal, so they project edge-on to zero area in a
/// fixed elevation (answered by the island-normal panel, whose whole purpose
/// is to show the patch at its true shape and size). A render in which the
/// owner cannot see what was uncovered does not serve the ruling at all.
///
/// **Scope, stated rather than hidden.** Routed contact points are not drawn:
/// this evidence layer has face membership and gap distance, not contact
/// coordinates, and adding those would mean capturing a second, larger slice
/// of the sealed run's own geometry at every refusal. The render answers "is
/// this uncovered patch a real overhang or a modelling artifact", which is
/// the ruling's own stated purpose.
#[must_use]
pub fn render_island_svg(
    mesh: &RenderMesh,
    island: &UncoveredIslandSummary,
    degrade: bool,
) -> String {
    let highlighted: BTreeSet<u32> = island
        .face_ids
        .iter()
        .copied()
        .take(MAX_HIGHLIGHT_FACES_PER_ISLAND)
        .collect();

    let mut body = String::new();
    render_panel(
        &mut body,
        "plate view (down -Z)",
        0.0,
        &mesh.verts,
        &mesh.tris,
        &highlighted,
        |vertex| (vertex[0], vertex[1], vertex[2]),
        degrade,
    );
    render_panel(
        &mut body,
        "+X elevation",
        CANVAS_W,
        &mesh.verts,
        &mesh.tris,
        &highlighted,
        |vertex| (vertex[1], vertex[2], vertex[0]),
        degrade,
    );
    match island_view_basis(mesh, &highlighted) {
        Some((u, v, n)) => render_panel(
            &mut body,
            "island face-on (along its own normal)",
            CANVAS_W * 2.0,
            &mesh.verts,
            &mesh.tris,
            &highlighted,
            |vertex| (dot(*vertex, &u), dot(*vertex, &v), dot(*vertex, &n)),
            degrade,
        ),
        // No usable island normal: repeat the plate axis rather than invent
        // one, so the panel is still present, still deterministic, and still
        // labelled honestly.
        None => render_panel(
            &mut body,
            "island face-on (unavailable: no island normal)",
            CANVAS_W * 2.0,
            &mesh.verts,
            &mesh.tris,
            &highlighted,
            |vertex| (vertex[0], vertex[1], vertex[2]),
            degrade,
        ),
    }

    let gap_line = match island.worst_gap {
        CoverageClosureGap::Finite(value) => format!("worst gap {} mm", fmt3(value)),
        CoverageClosureGap::NoRoutedContact => "NO ROUTED CONTACT".to_owned(),
    };
    let legend = format!(
        "island {}  {} mm2 uncovered  {}  {} faces",
        island.island_id,
        fmt3(island.uncovered_area_mm2),
        gap_line,
        island.face_count()
    );

    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
         viewBox=\"0 0 {width} {height}\" font-family=\"monospace\" font-size=\"11\">\n\
         <style>.neutral{{fill:#c9ccd1;stroke:#5b6270;stroke-width:0.4}}\
         .uncovered{{fill:#d9534f;stroke:#7a2b28;stroke-width:0.4}}\
         .silhouette{{fill:#e4e6ea;stroke:#5b6270;stroke-width:0.6}}\
         .title{{fill:#1c1f24}}.legend{{fill:#1c1f24}}</style>\n\
         <rect x=\"0\" y=\"0\" width=\"{width}\" height=\"{height}\" fill=\"#ffffff\"/>\n\
         {body}\
         <text x=\"{pad}\" y=\"{legend_y}\" class=\"legend\">{legend}</text>\n\
         </svg>\n",
        width = fmt3(CANVAS_W * 3.0),
        height = fmt3(CANVAS_H),
        pad = fmt3(PANEL_PAD),
        legend_y = fmt3(CANVAS_H - PANEL_PAD),
        body = body,
        legend = xml_escape(&legend),
    )
}

struct RenderOutcome {
    paths: Vec<PathBuf>,
    skipped: Option<String>,
}

fn render_worst_islands(
    mesh: &RenderMesh,
    islands: &[UncoveredIslandSummary],
    target: &Path,
) -> RenderOutcome {
    let mut paths = Vec::new();
    let mut skipped = None;
    let degrade = mesh.tris.len() > COVERAGE_RENDER_MAX_FACES;
    if degrade {
        skipped = Some(format!(
            "mesh has {} face(s), over COVERAGE_RENDER_MAX_FACES={}; rendered as a silhouette \
             plus the highlighted island faces only",
            mesh.tris.len(),
            COVERAGE_RENDER_MAX_FACES
        ));
    }
    for island in islands.iter().take(COVERAGE_RENDER_MAX_ISLANDS) {
        let svg = render_island_svg(mesh, island, degrade);
        let path = render_path(target, island.island_id);
        match std::fs::write(&path, svg) {
            Ok(()) => paths.push(path),
            Err(error) => {
                let note = format!(
                    "island {} render could not be written to {}: {error}",
                    island.island_id,
                    path.display()
                );
                skipped = Some(match skipped {
                    Some(existing) => format!("{existing}; {note}"),
                    None => note,
                });
            }
        }
    }
    RenderOutcome { paths, skipped }
}

// --- The one entry point ------------------------------------------------

/// Persist one refusal's evidence: rank the islands, write the sidecar, and
/// render the worst of them. Infallible by construction -- every failure a
/// step here can hit is folded into the returned evidence as a string, never
/// propagated, per the module doc.
#[allow(clippy::too_many_arguments)]
fn persist(
    model: &ModelId,
    target: &Path,
    mesh: &RenderMesh,
    source: EvidenceSource,
    gate: Option<PreWriteGate>,
    geometry: Option<&SupportedGeometry>,
    attempts: Option<usize>,
    islands: Vec<UncoveredIslandSummary>,
    measurement_error: Option<&str>,
    report: Option<&CoverageReport>,
    closure: Option<&CoverageClosureRefusal>,
) -> CoverageRefusalEvidence {
    let sidecar = sidecar_path(target);
    let rendered = render_worst_islands(mesh, &islands, target);
    let write_error = write_sidecar(
        &sidecar,
        SidecarWrite {
            model: model.as_str(),
            source,
            gate,
            geometry,
            attempts,
            islands: &islands,
            render_paths: &rendered.paths,
            render_skipped: rendered.skipped.as_deref(),
            measurement_error,
            report,
            closure,
        },
    )
    .err();

    CoverageRefusalEvidence {
        source,
        gate,
        geometry: geometry.cloned(),
        attempts,
        islands,
        sidecar_path: sidecar,
        render_paths: rendered.paths,
        render_skipped: rendered.skipped,
        write_error,
        measurement_error: measurement_error.map(ToOwned::to_owned),
    }
}

/// Persist evidence for a pre-write gate coverage refusal.
#[must_use]
pub fn persist_gate_refusal(
    model: &ModelId,
    target: &Path,
    mesh: &RenderMesh,
    gate: Option<PreWriteGate>,
    geometry: Option<&SupportedGeometry>,
    report: &CoverageReport,
) -> CoverageRefusalEvidence {
    let islands = islands_from_gate_report(report);
    persist(
        model,
        target,
        mesh,
        EvidenceSource::PreWriteGate,
        gate,
        geometry,
        None,
        islands,
        None,
        Some(report),
        None,
    )
}

/// Persist evidence for a pre-write coverage gate that could NOT be measured
/// (`GateRefusal::Unmeasurable`).
///
/// **Why this exists.** An unmeasurable coverage gate is fail-closed: the
/// model is `Refused` with the coverage gate named, and there is no artifact.
/// It has no `CoverageReport` — that is what "unmeasurable" means — but FU-57
/// says evidence is kept on EVERY remaining coverage refusal, and the
/// controller's equivalent failure
/// (`CoverageClosureReason::MeasurementRefused`) has always written a
/// reason-only sidecar. Writing one here removes an asymmetry in which the
/// identical error produced a sidecar inside `run_support` and complete
/// silence one stage later. Islands are empty because none were measured, and
/// the sidecar says so rather than implying there were none.
#[must_use]
pub fn persist_unmeasurable_gate(
    model: &ModelId,
    target: &Path,
    mesh: &RenderMesh,
    gate: Option<PreWriteGate>,
    geometry: Option<&SupportedGeometry>,
    measurement_error: &str,
) -> CoverageRefusalEvidence {
    persist(
        model,
        target,
        mesh,
        EvidenceSource::PreWriteGate,
        gate,
        geometry,
        None,
        Vec::new(),
        Some(measurement_error),
        None,
        None,
    )
}

/// Persist evidence for a coverage-closure controller refusal (FU-56).
#[must_use]
pub fn persist_closure_refusal(
    model: &ModelId,
    target: &Path,
    mesh: &RenderMesh,
    geometry: Option<&SupportedGeometry>,
    refusal: &CoverageClosureRefusal,
) -> CoverageRefusalEvidence {
    let islands = islands_from_closure(refusal);
    let measurement_error = match &refusal.reason {
        plinth_support::CoverageClosureReason::MeasurementRefused(detail) => Some(detail.clone()),
        // EXHAUSTIVE, no wildcard arm, deliberately: this is the one place in
        // the workspace that must decide what a NEW closure reason means for the
        // sidecar, and a wildcard would decide it silently. Only a refused
        // MEASUREMENT is a measurement error; every other reason is the
        // controller reporting a verdict it reached, not an instrument that
        // failed. SPEC-A4 §B.7's three one-winner reasons join the second group
        // for exactly that reason.
        plinth_support::CoverageClosureReason::AttemptsExhausted
        | plinth_support::CoverageClosureReason::NoEligibleRepairRows
        | plinth_support::CoverageClosureReason::RepairPlanRefused { .. }
        | plinth_support::CoverageClosureReason::NothingToTry
        | plinth_support::CoverageClosureReason::NoImprovingRepair { .. }
        | plinth_support::CoverageClosureReason::CommitBudgetExhausted { .. } => None,
    };
    persist(
        model,
        target,
        mesh,
        EvidenceSource::CoverageClosureController,
        None,
        geometry,
        Some(refusal.attempts),
        islands,
        measurement_error.as_deref(),
        None,
        Some(refusal),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_core::{ArtifactId, Defect, Severity, ViolationKind};

    fn fixture_defect(island_id: u32, area: f64) -> Defect {
        Defect::new(
            ViolationKind::UncoveredIsland,
            SiteId::Island(island_id),
            Severity::Blocking,
            area,
            format!("fixture island {island_id}"),
        )
        .expect("finite fixture defect")
    }

    fn fixture_report(defects: Vec<Defect>) -> CoverageReport {
        // `CoverageReport` has no public constructor outside `plinth-verify`
        // (deliberately: only a live measurement can build one). Round-trip
        // through the crate's own pinned wire shape instead of adding one
        // here, exactly as this module's islands-from-report path already
        // does for patch detail -- see the module doc.
        let value = serde_json::json!({
            "artifact": ArtifactId::new("fixture"),
            "snapshot_digest": 0,
            "request": {
                "radius_mm": 6.0,
                "patch_area_floor_mm2": 0.16,
                "max_route_memberships": 1,
                "max_indexed_points": 1,
                "max_query_rows": 1,
                "max_distance_work": 1,
                "max_logical_peak_bytes": 1,
                "max_assessment_work": 1,
            },
            "resources": {
                "route_membership_sum": 0,
                "sequential_route_membership_peak": 0,
                "indexed_point_count": 0,
                "sequential_indexed_point_peak": 0,
                "query_row_count": 0,
                "distance_work": 0,
                "logical_peak_bytes": 0,
                "assessment_work": 0,
                "assessment_scratch_bytes": 0,
                "assessment_evidence_bytes": 0,
                "assessment_phase_peak_bytes": 0,
            },
            "evidence": {
                "islands": [],
                "patches": [
                    {
                        "island_id": 12,
                        "component_id": 0,
                        "representative_face_id": 3,
                        "sorted_face_ids": [3, 4, 5],
                        "uncovered_area_mm2": 3.418,
                        "cut_ambiguous": false,
                        "assessment_failed": false,
                        "worst_gap": {"state": "finite", "gap_mm": 7.94},
                    },
                    {
                        "island_id": 7,
                        "component_id": 0,
                        "representative_face_id": 8,
                        "sorted_face_ids": [8, 9],
                        "uncovered_area_mm2": 1.002,
                        "cut_ambiguous": false,
                        "assessment_failed": false,
                        "worst_gap": {"state": "no_routed_contact"},
                    },
                ],
            },
            "defects": defects,
        });
        serde_json::from_value(value).expect("fixture report parses")
    }

    fn two_island_report() -> CoverageReport {
        fixture_report(vec![fixture_defect(12, 3.418), fixture_defect(7, 1.002)])
    }

    #[test]
    fn islands_are_ranked_worst_first_by_uncovered_area() {
        let report = two_island_report();
        let islands = islands_from_gate_report(&report);
        assert_eq!(
            islands.iter().map(|island| island.island_id).collect::<Vec<_>>(),
            vec![12, 7]
        );
        assert!((islands[0].uncovered_area_mm2 - 3.418).abs() < 1e-9);
        assert_eq!(islands[0].worst_gap, CoverageClosureGap::Finite(7.94));
        assert_eq!(islands[0].face_ids, vec![3, 4, 5]);
        assert_eq!(islands[1].worst_gap, CoverageClosureGap::NoRoutedContact);
        assert_eq!(islands[1].face_ids, vec![8, 9]);
    }

    #[test]
    fn equal_areas_tie_break_on_island_id_ascending_and_are_deterministic() {
        let report = fixture_report(vec![fixture_defect(9, 2.0), fixture_defect(3, 2.0)]);
        // Neither island has patch evidence in this fixture, so both fall
        // back to `NoRoutedContact` / empty faces -- the tie-break is what
        // this test is about.
        let first = islands_from_gate_report(&report);
        let second = islands_from_gate_report(&report);
        assert_eq!(first, second, "two builds of the same fixture must match byte-for-byte");
        assert_eq!(
            first.iter().map(|island| island.island_id).collect::<Vec<_>>(),
            vec![3, 9]
        );
    }

    #[test]
    fn closure_islands_merge_across_components_and_rank_worst_first() {
        let refusal = CoverageClosureRefusal {
            attempts: 8,
            reason: plinth_support::CoverageClosureReason::AttemptsExhausted,
            islands: vec![
                plinth_support::CoverageClosureIsland {
                    island_id: 5,
                    component_id: 0,
                    representative_face_id: 1,
                    sorted_face_ids: vec![1, 2],
                    uncovered_area_mm2: 0.5,
                    worst_gap: CoverageClosureGap::Finite(2.0),
                    patch_count: 1,
                    nominated_repair_rows: 0,
                    eligible_repair_rows: 0,
                    outcome: None,
                },
                plinth_support::CoverageClosureIsland {
                    island_id: 5,
                    component_id: 1,
                    representative_face_id: 6,
                    sorted_face_ids: vec![6],
                    uncovered_area_mm2: 0.7,
                    worst_gap: CoverageClosureGap::NoRoutedContact,
                    patch_count: 1,
                    nominated_repair_rows: 0,
                    eligible_repair_rows: 0,
                    outcome: None,
                },
                plinth_support::CoverageClosureIsland {
                    island_id: 2,
                    component_id: 0,
                    representative_face_id: 9,
                    sorted_face_ids: vec![9],
                    uncovered_area_mm2: 5.0,
                    worst_gap: CoverageClosureGap::Finite(1.0),
                    patch_count: 1,
                    nominated_repair_rows: 0,
                    eligible_repair_rows: 0,
                    outcome: None,
                },
            ],
            history: Vec::new(),
            repair: None,
        };
        let islands = islands_from_closure(&refusal);
        assert_eq!(
            islands.iter().map(|island| island.island_id).collect::<Vec<_>>(),
            vec![2, 5]
        );
        let merged = &islands[1];
        assert!((merged.uncovered_area_mm2 - 1.2).abs() < 1e-9);
        assert_eq!(merged.worst_gap, CoverageClosureGap::NoRoutedContact);
        assert_eq!(merged.face_ids, vec![1, 2, 6]);
    }

    #[test]
    fn the_sidecar_embeds_the_report_verbatim_and_it_deserializes_back() {
        let dir = std::env::temp_dir().join(format!(
            "plinth-coverage-evidence-sidecar-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let target = dir.join("fixture.chitubox");
        let report = two_island_report();
        let mesh = RenderMesh {
            verts: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            tris: vec![[0, 1, 2]],
        };
        let model = ModelId::new("fixture");
        let evidence = persist_gate_refusal(
            &model,
            &target,
            &mesh,
            Some(PreWriteGate::UncoveredIslands),
            None,
            &report,
        );
        assert!(evidence.write_error.is_none(), "{:?}", evidence.write_error);
        let bytes = std::fs::read(&evidence.sidecar_path).expect("sidecar written");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("valid json");
        assert_eq!(value["schema"], SIDECAR_SCHEMA);
        assert_eq!(value["source"], "pre_write_gate");
        let round_tripped: CoverageReport =
            serde_json::from_value(value["report"].clone()).expect("report round-trips");
        assert_eq!(round_tripped, report);
        assert!(value.get("closure").is_none() || value["closure"].is_null());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_sidecar_embeds_the_closure_refusal_verbatim_with_attempts() {
        let dir = std::env::temp_dir().join(format!(
            "plinth-coverage-evidence-closure-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let target = dir.join("fixture.chitubox");
        let refusal = CoverageClosureRefusal {
            attempts: 8,
            reason: plinth_support::CoverageClosureReason::AttemptsExhausted,
            islands: vec![plinth_support::CoverageClosureIsland {
                island_id: 4,
                component_id: 0,
                representative_face_id: 0,
                sorted_face_ids: vec![0, 1],
                uncovered_area_mm2: 0.9,
                worst_gap: CoverageClosureGap::NoRoutedContact,
                patch_count: 1,
                nominated_repair_rows: 3,
                eligible_repair_rows: 0,
                outcome: None,
            }],
            history: Vec::new(),
            repair: None,
        };
        let mesh = RenderMesh {
            verts: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            tris: vec![[0, 1, 2]],
        };
        let model = ModelId::new("fixture");
        let evidence = persist_closure_refusal(&model, &target, &mesh, None, &refusal);
        assert_eq!(evidence.attempts, Some(8));
        let bytes = std::fs::read(&evidence.sidecar_path).expect("sidecar written");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("valid json");
        assert_eq!(value["source"], "coverage_closure_controller");
        assert_eq!(value["attempts"], 8);
        let round_tripped: CoverageClosureRefusal =
            serde_json::from_value(value["closure"].clone()).expect("closure round-trips");
        assert_eq!(round_tripped, refusal);
        assert!(value.get("report").is_none() || value["report"].is_null());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_svg_renderer_is_byte_deterministic_and_highlights_exactly_the_islands_faces() {
        let mesh = RenderMesh {
            verts: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 1.0],
            ],
            tris: vec![[0, 1, 2], [1, 2, 3]],
        };
        let island = UncoveredIslandSummary {
            island_id: 1,
            uncovered_area_mm2: 1.5,
            worst_gap: CoverageClosureGap::Finite(2.25),
            face_ids: vec![1],
        };
        let first = render_island_svg(&mesh, &island, false);
        let second = render_island_svg(&mesh, &island, false);
        assert_eq!(first, second, "two renders of the same fixture must be byte-identical");
        assert_eq!(first.matches("class=\"uncovered\"").count(), 3, "one highlighted triangle per panel");
        assert_eq!(first.matches("class=\"neutral\"").count(), 3, "one neutral triangle per panel");
        assert!(first.contains("island 1"));
        assert!(first.contains("worst gap 2.250 mm"));
    }

    /// Split one render into its per-panel bodies, by the `class="title"`
    /// line each panel opens with.
    fn panels(svg: &str) -> Vec<String> {
        svg.split("class=\"title\"")
            .skip(1)
            .map(std::borrow::ToOwned::to_owned)
            .collect()
    }

    /// The polygons of one class in one panel, as their three screen points.
    fn polygons(panel: &str, class: &str) -> Vec<[(f64, f64); 3]> {
        let needle = format!("\" class=\"{class}\"/>");
        panel
            .lines()
            .filter(|line| line.ends_with(&needle))
            .filter_map(|line| {
                let points = line.strip_prefix("<polygon points=\"")?;
                let points = points.split('"').next()?;
                let mut parsed = [(0.0, 0.0); 3];
                for (slot, pair) in points.split_whitespace().enumerate().take(3) {
                    let (x, y) = pair.split_once(',')?;
                    parsed[slot] = (x.parse().ok()?, y.parse().ok()?);
                }
                Some(parsed)
            })
            .collect()
    }

    fn triangle_area(points: [(f64, f64); 3]) -> f64 {
        ((points[1].0 - points[0].0) * (points[2].1 - points[0].1)
            - (points[2].0 - points[0].0) * (points[1].1 - points[0].1))
            .abs()
            / 2.0
    }

    /// **FU-57's whole purpose: the owner can SEE the uncovered patch.**
    ///
    /// The archetypal uncovered support island is a horizontal underside —
    /// here the bottom face of a unit cube. Two structural facts used to make
    /// that island invisible in every panel of its own render: the model
    /// occludes it from above in the plate view (it was drawn first, then
    /// painted over by the top face at identical screen coordinates), and it
    /// projects edge-on to a zero-area sliver in a fixed elevation. This test
    /// pins both fixes: highlighted faces are drawn after every neutral face
    /// in each panel, and the third panel — projected along the island's own
    /// normal — shows the patch with real area.
    #[test]
    fn a_horizontal_underside_is_visible_in_its_own_render() {
        // Unit cube, 12 triangles, faces 0 and 1 the z = 0 bottom.
        let verts = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ];
        let tris = vec![
            [0, 2, 1],
            [0, 3, 2], // bottom (z = 0), outward normal -Z
            [4, 5, 6],
            [4, 6, 7], // top
            [0, 1, 5],
            [0, 5, 4], // -Y side
            [2, 3, 7],
            [2, 7, 6], // +Y side
            [1, 2, 6],
            [1, 6, 5], // +X side
            [3, 0, 4],
            [3, 4, 7], // -X side
        ];
        let mesh = RenderMesh { verts, tris };
        let island = UncoveredIslandSummary {
            island_id: 0,
            uncovered_area_mm2: 1.0,
            worst_gap: CoverageClosureGap::NoRoutedContact,
            face_ids: vec![0, 1],
        };
        let svg = render_island_svg(&mesh, &island, false);
        let panels = panels(&svg);
        assert_eq!(panels.len(), 3, "plate view, elevation, island face-on");

        for (index, panel) in panels.iter().enumerate() {
            let uncovered_at = panel
                .find("class=\"uncovered\"")
                .unwrap_or_else(|| panic!("panel {index} draws the uncovered patch"));
            let last_neutral = panel
                .rfind("class=\"neutral\"")
                .unwrap_or_else(|| panic!("panel {index} draws the model"));
            assert!(
                uncovered_at > last_neutral,
                "panel {index} paints model geometry over the uncovered patch, which is exactly \
                 the render that shows the owner nothing"
            );
        }

        // The island-normal panel must show the patch at real size: an
        // edge-on projection (what the fixed elevation gives a horizontal
        // underside) is a zero-area sliver and answers nothing.
        let face_on = polygons(&panels[2], "uncovered");
        assert_eq!(face_on.len(), 2, "both bottom triangles are drawn face-on");
        let area: f64 = face_on.iter().copied().map(triangle_area).sum();
        assert!(
            area > 1_000.0,
            "the face-on panel must show the patch at real size, measured {area}"
        );
        // And the fixed elevation is exactly the degenerate case that made
        // the third panel necessary: recorded, not asserted away.
        let elevation: f64 = polygons(&panels[1], "uncovered")
            .iter()
            .copied()
            .map(triangle_area)
            .sum();
        assert!(
            elevation < 1.0,
            "a horizontal underside is edge-on in the fixed elevation; if that ever stops \
             being true this test's premise has changed, measured {elevation}"
        );
    }

    #[test]
    fn over_the_render_face_bound_the_render_degrades_and_records_why() {
        let verts = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let tris: Vec<[u32; 3]> = (0..(COVERAGE_RENDER_MAX_FACES + 1))
            .map(|_| [0, 1, 2])
            .collect();
        let mesh = RenderMesh { verts, tris };
        let island = UncoveredIslandSummary {
            island_id: 1,
            uncovered_area_mm2: 1.0,
            worst_gap: CoverageClosureGap::NoRoutedContact,
            face_ids: vec![0],
        };
        let dir = std::env::temp_dir().join(format!(
            "plinth-coverage-evidence-degrade-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let target = dir.join("fixture.chitubox");
        let outcome = render_worst_islands(&mesh, std::slice::from_ref(&island), &target);
        assert_eq!(outcome.paths.len(), 1, "the render is still written, degraded rather than dropped");
        assert!(
            outcome.skipped.as_deref().is_some_and(|note| note.contains("COVERAGE_RENDER_MAX_FACES")),
            "{:?}",
            outcome.skipped
        );
        let bytes = std::fs::read(&outcome.paths[0]).expect("degraded render written");
        let svg = String::from_utf8(bytes).expect("utf8 svg");
        assert!(svg.contains("class=\"silhouette\""));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn render_and_sidecar_paths_share_the_targets_stem() {
        let target = Path::new("/out/Giant Toad.chitubox");
        assert_eq!(
            sidecar_path(target),
            Path::new("/out/Giant Toad.coverage.json")
        );
        assert_eq!(
            render_path(target, 12),
            Path::new("/out/Giant Toad.coverage.island-12.svg")
        );
        assert_eq!(
            render_file_prefix(target).as_deref(),
            Some("Giant Toad.coverage.island-")
        );
    }

    /// **An unwritable sidecar is recorded, not swallowed and not fatal.**
    ///
    /// The ruling: a failure to write evidence must never turn a refusal into
    /// a failure or vice versa. This module has no disposition of its own to
    /// corrupt -- `persist_gate_refusal` always returns a complete
    /// `CoverageRefusalEvidence` -- so the property under test is that a
    /// write failure is folded into `write_error` as data, the ranked
    /// islands are still correct, and nothing panics.
    #[test]
    fn a_sidecar_write_failure_is_recorded_and_never_panics() {
        let dir = std::env::temp_dir().join(format!(
            "plinth-coverage-evidence-write-failure-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let target = dir.join("fixture.chitubox");
        // Occupy the sidecar's own path with a directory, so the write
        // cannot succeed no matter what it contains.
        std::fs::create_dir_all(sidecar_path(&target)).expect("occupy the sidecar path");

        let report = two_island_report();
        let mesh = RenderMesh {
            verts: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            tris: vec![[0, 1, 2]],
        };
        let model = ModelId::new("fixture");
        let evidence = persist_gate_refusal(&model, &target, &mesh, None, None, &report);

        assert!(evidence.write_error.is_some(), "the write failure must be named");
        assert_eq!(
            evidence.islands.iter().map(|island| island.island_id).collect::<Vec<_>>(),
            vec![12, 7],
            "a write failure must not corrupt the ranked evidence still returned"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
