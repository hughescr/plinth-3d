//! The first end-to-end export composition: sealed support run -> supported
//! scene -> `.chitubox` on disk -> re-parse and audit the written bytes.
//!
//! This module is deliberately NOT a cargo target. It lives under
//! `examples/shared/` (which cargo's example auto-discovery ignores, because it
//! discovers only `examples/*.rs` and `examples/*/main.rs`) so that the
//! integration test `tests/first_artifact.rs` and the artifact-producing
//! example `examples/first_artifact.rs` run *the same* composition rather than
//! two drifting copies of it.
//!
//! What is new here is only the composition. Every stage is existing, frozen
//! code called through its public API; nothing in this module plans supports,
//! builds geometry, or edits gate machinery. In particular it is a read-only
//! consumer of `plinth-support`: it never pins a support digest, so concurrent
//! gate work can move member counts without touching this seam.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chitubox_io::{Project, SegmentType};
use plinth_core::{ArtifactId, LayerPlan, PositiveFinite, PrinterProfile};
use plinth_decimate::{run as run_decimation, DecimationBudget, ProbeConfig};
use plinth_emit::{write as emit_write, EmitReport, EmitRequest};
use plinth_hollow::{
    HollowDisposition, HollowRequest, HollowStageOutput, HollowedModel, SolidDecisionReason,
};
use plinth_mesh::{load_binary_stl, Mesh, WeldPolicy};
use plinth_orient::{run as run_orientation, OrientRequest, PinnedBaseline};
use plinth_pipeline::{hollow_stage, OutputProfile};
use plinth_support::{run_support, SupportPreparationLimits, SupportRequest};
use plinth_verify::{verify_artifact, StructuralFormatStatus};

/// The lift used by every sealed-run gate in this workspace.
pub const LIFT_MM: f64 = 5.0;
/// The frozen 0.05 mm layer pitch of the existing sealed-run gates.
pub const LAYER_PITCH_MM: f64 = 0.05;
/// The `plinth-emit` model-record name; asserted on the reopened file.
pub const EMITTED_MODEL_NAME: &str = "plinth-model";
/// The E1 native preview asset, embedded on first-model-add by `chitubox_io`.
const NATIVE_PREVIEW: &[u8] =
    include_bytes!("../../../../rust/chitubox_io/assets/two-supports-preview.bin");

pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

pub fn corpus_stl(name: &str) -> PathBuf {
    workspace_root()
        .join("corpus/models")
        .join(format!("{name}.stl"))
}

pub fn pinned_baseline(name: &str) -> PinnedBaseline {
    PinnedBaseline {
        pipeline_report: workspace_root()
            .join("corpus/baseline")
            .join(format!("{name}.pipeline.json")),
    }
}

/// Which geometry actually reached the sealed support run, and why.
///
/// This is measured, never assumed. The design record's OQ-1 asked whether
/// `plinth_hollow::run` completes on corpus models; it does, and the answer
/// turned out to matter for a reason OQ-1 did not anticipate, so the state is
/// carried out to the caller rather than buried in a comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SupportedGeometry {
    /// The hollow stage decided solid on its own terms and support planning
    /// consumed exactly what it produced. Nothing here is a fallback.
    HollowStageDecidedSolid,
    /// Hollowed, drained and drilled, and the sealed support run accepted that
    /// shell. This is the path the pipeline is ultimately meant to take.
    DrilledShellSealed,
    /// Hollowed, drained and drilled -- and then refused by the sealed support
    /// run, so the oriented solid carried the model instead.
    ///
    /// The refusal string is the runner's own message plus the model-scaled
    /// caps it was measured against, because a refusal without its bound is
    /// not a measurement.
    DrilledShellRefusedBySupport {
        drilled_faces: usize,
        refusal: String,
    },
}

/// Everything the composition observed, reported rather than asserted so both
/// callers can decide what to do with it.
#[derive(Clone, Debug)]
pub struct ChainOutcome {
    pub model: String,
    pub source_faces: usize,
    pub prepared_faces: usize,
    pub hollow_disposition: HollowDisposition,
    pub supported_geometry: SupportedGeometry,
    pub routed_contacts: usize,
    pub final_live_members: usize,
    pub snapshot_digest: u64,
    pub primitive_count: usize,
    pub lift_mm: f64,
    pub plate_z_mm: f64,
    pub emit: EmitReport,
}

/// Per-artifact evidence recovered by *re-reading* the file that was written.
#[derive(Clone, Debug)]
pub struct DiskAudit {
    pub model_name: String,
    pub model_triangle_count: usize,
    pub support_record_count: usize,
    /// Wire segment type -> record count, from the reopened file.
    pub records_by_segment_type: BTreeMap<u32, usize>,
    /// Records whose embedded mesh had strictly positive signed volume, i.e.
    /// consistent outward winding, computed from the binary32 disk vertices.
    pub outward_wound_records: usize,
    pub min_record_signed_volume_mm3: f64,
    pub model_signed_volume_mm3: f64,
    pub preview_bytes: usize,
    pub preview_is_native_fallback: bool,
    pub format_gate_passed: bool,
    pub file_bytes: u64,
}

impl DiskAudit {
    /// The three mesh families `plinth-emit` synthesises, by wire type.
    #[must_use]
    pub fn frustum_records(&self) -> usize {
        self.count(SegmentType::TAPER)
            + self.count(SegmentType::JOINER)
            + self.count(SegmentType::FLARE)
    }

    #[must_use]
    pub fn sphere_records(&self) -> usize {
        self.count(SegmentType::TIP) + self.count(SegmentType::SHAFT)
    }

    #[must_use]
    pub fn raft_records(&self) -> usize {
        self.count(SegmentType::PAD_OR_BRIDGE)
    }

    #[must_use]
    pub fn count(&self, segment_type: SegmentType) -> usize {
        self.records_by_segment_type
            .get(&segment_type.0)
            .copied()
            .unwrap_or(0)
    }
}

/// The M0 Saturn 4 Ultra authority, resolved through the pipeline's own
/// typed output-profile selection rather than reached for directly.
pub fn saturn4_ultra() -> &'static PrinterProfile {
    OutputProfile::Saturn4Ultra
        .resolve()
        .expect("the M0 Saturn 4 Ultra profile is a registered constant")
}

/// Load, decimate and orient one model from its checked-in pinned baseline.
pub fn prepare_from_corpus(name: &str) -> Result<(Mesh, plinth_orient::OrientedModel), String> {
    let mesh = load_binary_stl(corpus_stl(name), WeldPolicy::None)
        .map_err(|error| format!("load {name}: {error}"))?;
    let oriented = orient(mesh.clone(), name)?;
    Ok((mesh, oriented))
}

/// Same preparation for an in-repo procedural mesh, which needs no STL.
pub fn prepare_from_mesh(
    mesh: Mesh,
    baseline: &str,
) -> Result<(Mesh, plinth_orient::OrientedModel), String> {
    let oriented = orient(mesh.clone(), baseline)?;
    Ok((mesh, oriented))
}

fn orient(mesh: Mesh, baseline: &str) -> Result<plinth_orient::OrientedModel, String> {
    let budget =
        DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
            .map_err(|error| format!("budget: {error}"))?;
    let decimated = run_decimation(&mesh, budget)
        .map_err(|error| format!("decimate: {error}"))?
        .into_decimated_model()
        .map_err(|scale| format!("decimate-scale-skip: {scale:?}"))?;
    run_orientation(
        decimated,
        &OrientRequest {
            pinned: pinned_baseline(baseline),
        },
    )
    .map_err(|error| format!("orient: {error}"))
}

/// Run the whole chain from an oriented model to a written artifact, taking the
/// real hollow stage and letting the sealed support run decide what it can
/// carry.
///
/// **OQ-1 is answered, and not the way the design record guessed.** It expected
/// a corpus model might *refuse* to hollow, and recommended the typed
/// solid-decision fallback for that case. `plinth_hollow::run` does not refuse
/// the corpus Sphere at all: it hollows, plans drains, drills, and hands back
/// `AwaitingInternalSupport` — a proved drilled shell paired with the untouched
/// oriented solid. The refusal lands one stage later. The sealed support run
/// rejects that 14,526-face drilled shell during Preparation, so the drilled
/// hollow is not supportable by today's engine even though it is producible.
///
/// So this seam tries the real thing and records what happened:
///
/// 1. `Solid` — support planning consumes the hollow stage's own output.
/// 2. `AwaitingInternalSupport` — `approve_hollow` yields the drilled shell
///    with its true `HollowedAndDrilled` disposition (the two alternatives,
///    `into_solid_fallback` and `solid_decision`, would both stamp a named
///    verdict nobody reached). Note that A5 internal-support planning does not
///    exist in this workspace, so this shell carries no internal supports.
/// 3. If the sealed run refuses that shell, fall back to the oriented-solid
///    reserve through the same `solid_decision(BelowThreshold)` handoff that
///    every sealed-run gate and the whole OD-8 census already use — the
///    design record's F14 caveat, inherited deliberately and reported in
///    `SupportedGeometry`, not hidden.
pub fn run_from_oriented(
    name: &str,
    source_faces: usize,
    oriented: plinth_orient::OrientedModel,
    target: &Path,
) -> Result<ChainOutcome, String> {
    let printer = saturn4_ultra().clone();
    let output = hollow_stage(oriented, &HollowRequest { printer })
        .map_err(|error| format!("hollow: {error}"))?;

    let candidate = match output {
        HollowStageOutput::Solid(model) => {
            return seal_and_emit(
                name,
                source_faces,
                *model,
                SupportedGeometry::HollowStageDecidedSolid,
                target,
            );
        }
        HollowStageOutput::AwaitingInternalSupport(candidate) => candidate,
    };

    // Keep the reserve before consuming the pair, so a support refusal on the
    // drilled shell still has the oriented solid to fall back to.
    let reserve_mesh = candidate.reserve_mesh().clone();
    let reserve_history = candidate.reserve_history().clone();
    let drilled = (*candidate).approve_hollow();
    let drilled_faces = drilled.mesh().tris().len();

    match seal_and_emit(
        name,
        source_faces,
        drilled,
        SupportedGeometry::DrilledShellSealed,
        target,
    ) {
        Ok(outcome) => Ok(outcome),
        Err(refusal) => {
            let solid = HollowedModel::solid_decision(
                reserve_mesh,
                reserve_history,
                SolidDecisionReason::BelowThreshold,
            )
            .map_err(|error| format!("{name}: oriented-solid handoff: {error}"))?;
            seal_and_emit(
                name,
                source_faces,
                solid,
                SupportedGeometry::DrilledShellRefusedBySupport {
                    drilled_faces,
                    refusal,
                },
                target,
            )
        }
    }
}

/// Seal supports, lower to a `SupportedModel`, and emit one `.chitubox`.
///
/// This is the composition the design record calls the load-bearing gap: three
/// existing arrows joined for the first time.
pub fn seal_and_emit(
    name: &str,
    source_faces: usize,
    hollowed: HollowedModel,
    supported_geometry: SupportedGeometry,
    target: &Path,
) -> Result<ChainOutcome, String> {
    let prepared_faces = hollowed.mesh().tris().len();
    let hollow_disposition = hollowed.history().hollow().disposition();

    let (min_z, max_z) = hollowed
        .mesh()
        .verts()
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), vertex| {
            (lo.min(vertex[2]), hi.max(vertex[2]))
        });
    if !min_z.is_finite() || !max_z.is_finite() {
        return Err(format!("{name}: degenerate z extent"));
    }
    let span_mm = (max_z - min_z) + LIFT_MM + 1.0;
    let layer_count = ((span_mm / LAYER_PITCH_MM).ceil() as usize).max(1_000);
    let layers = LayerPlan::new(0.0, vec![LAYER_PITCH_MM; layer_count])
        .map_err(|error| format!("layers: {error}"))?;
    let delicacy = vec![0.0; prepared_faces];

    let input = SupportPreparationLimits::production()
        .map_err(|error| format!("limits: {error}"))?
        .input(&layers, &delicacy, &[]);
    // A refusal without the bounds it was measured against is not a
    // measurement; carry the model-scaled caps on the failure path, as the
    // census harness does.
    let scaled_caps = format!(
        "faces={prepared_faces} max_grid_bytes={} knn_max_work={}",
        input.max_grid_bytes(),
        input.minima_knn_max_work(),
    );

    let authority = run_support(
        hollowed,
        SupportRequest {
            lift_mm: PositiveFinite::new(LIFT_MM).map_err(|_| "lift".to_owned())?,
        },
        input,
    )
    .map_err(|error| format!("{name}: support: {error} [{scaled_caps}]"))?;

    let (routed_contacts, final_live_members, snapshot_digest) = {
        let snapshot = authority
            .verification_snapshot()
            .map_err(|error| format!("snapshot: {error}"))?;
        snapshot
            .validate_integrity()
            .map_err(|error| format!("snapshot integrity: {error}"))?;
        (
            snapshot.routed_contacts().count(),
            snapshot.final_live_members().count(),
            snapshot.digest(),
        )
    };

    let supported = authority
        .into_supported_model()
        .map_err(|error| format!("{name}: lowering: {error}"))?;

    let emit = emit_write(
        &supported,
        &EmitRequest {
            target_path: target.to_path_buf(),
            printer: saturn4_ultra().clone(),
        },
    )
    .map_err(|error| format!("{name}: emit: {error}"))?;

    Ok(ChainOutcome {
        model: name.to_owned(),
        source_faces,
        prepared_faces,
        hollow_disposition,
        supported_geometry,
        routed_contacts,
        final_live_members,
        snapshot_digest,
        primitive_count: supported.primitives().len(),
        lift_mm: supported.lift_mm().get(),
        plate_z_mm: supported.plate_z_mm().get(),
        emit,
    })
}

/// The whole chain for one corpus model, start to written file.
pub fn run_corpus_model(name: &str, target: &Path) -> Result<ChainOutcome, String> {
    let (source, oriented) = prepare_from_corpus(name)?;
    run_from_oriented(name, source.tris().len(), oriented, target)
}

/// The whole chain for an in-repo procedural mesh.
pub fn run_procedural_model(name: &str, mesh: Mesh, target: &Path) -> Result<ChainOutcome, String> {
    let source_faces = mesh.tris().len();
    let (_, oriented) = prepare_from_mesh(mesh, name)?;
    run_from_oriented(name, source_faces, oriented, target)
}

/// Re-parse the file that was just written and judge only those bytes.
///
/// `plinth-emit` already audits a freshly reopened copy against its own
/// producer state. This is the independent second read the crate's round-trip
/// discipline asks for: it re-derives winding from the binary32 vertices on
/// disk, inspects the opaque preview span through the raw header, and runs the
/// `plinth-verify` format gate, none of which consult producer state.
pub fn audit_disk_artifact(path: &Path, artifact: &str) -> Result<DiskAudit, String> {
    let disk_bytes = std::fs::read(path).map_err(|error| format!("read artifact: {error}"))?;
    let project = Project::open(path).map_err(|error| format!("reopen artifact: {error:?}"))?;
    if project.model_count() != 1 {
        return Err(format!(
            "reopened artifact has {} models, expected 1",
            project.model_count()
        ));
    }
    let info = project
        .model(0)
        .map_err(|error| format!("reopened model record: {error:?}"))?;
    let model_triangles = project
        .model_triangles(0)
        .map_err(|error| format!("reopened model mesh: {error:?}"))?;
    let records = project
        .supports(0)
        .map_err(|error| format!("reopened support section: {error:?}"))?;

    let mut records_by_segment_type: BTreeMap<u32, usize> = BTreeMap::new();
    let mut outward_wound_records = 0usize;
    let mut min_record_signed_volume_mm3 = f64::INFINITY;
    for record in &records {
        *records_by_segment_type
            .entry(record.segment_type.0)
            .or_default() += 1;
        let volume = signed_volume_f32(&record.mesh.xyz)
            .ok_or_else(|| format!("record mesh is not a whole triangle soup in {artifact}"))?;
        if volume > 0.0 {
            outward_wound_records += 1;
        }
        min_record_signed_volume_mm3 = min_record_signed_volume_mm3.min(volume);
    }
    if records.is_empty() {
        min_record_signed_volume_mm3 = 0.0;
    }

    // The preview is the opaque span between the 420-byte header+settings block
    // and the model-record table, whose offset the header carries at byte 8.
    let preview_end = u32::from_le_bytes(
        disk_bytes
            .get(8..12)
            .ok_or_else(|| "artifact is shorter than its header".to_owned())?
            .try_into()
            .expect("four bytes"),
    ) as usize;
    let preview = disk_bytes
        .get(420..preview_end)
        .ok_or_else(|| "artifact preview span is out of bounds".to_owned())?;

    let report = verify_artifact(ArtifactId::new(artifact), path);

    Ok(DiskAudit {
        model_name: info.name.clone(),
        model_triangle_count: model_triangles.len() / 9,
        support_record_count: records.len(),
        records_by_segment_type,
        outward_wound_records,
        min_record_signed_volume_mm3,
        model_signed_volume_mm3: signed_volume_f32(&model_triangles)
            .ok_or_else(|| format!("model mesh is not a whole triangle soup in {artifact}"))?,
        preview_bytes: preview.len(),
        preview_is_native_fallback: preview == NATIVE_PREVIEW,
        format_gate_passed: report.accepted()
            && matches!(report.structural_status(), StructuralFormatStatus::Passed),
        file_bytes: std::fs::metadata(path)
            .map_err(|error| format!("stat artifact: {error}"))?
            .len(),
    })
}

/// Signed volume of a closed triangle soup, in f64, from binary32 disk
/// vertices. For a closed surface this is translation invariant, so a strictly
/// positive value is exactly the outward-winding claim `plinth-emit` enforces
/// on the producer side -- re-derived here from the bytes rather than trusted.
fn signed_volume_f32(xyz: &[f32]) -> Option<f64> {
    if xyz.is_empty() || !xyz.len().is_multiple_of(9) {
        return None;
    }
    let mut sum = 0.0f64;
    for triangle in xyz.chunks_exact(9) {
        let point = |offset: usize| {
            [
                f64::from(triangle[offset]),
                f64::from(triangle[offset + 1]),
                f64::from(triangle[offset + 2]),
            ]
        };
        let (a, b, c) = (point(0), point(3), point(6));
        let cross = [
            b[1] * c[2] - b[2] * c[1],
            b[2] * c[0] - b[0] * c[2],
            b[0] * c[1] - b[1] * c[0],
        ];
        sum += (a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2]) / 6.0;
    }
    sum.is_finite().then_some(sum)
}
