//! Corpus-fixture wrappers around the production `plinth prepare` chain, plus
//! the independent on-disk audit that re-reads what the chain wrote.
//!
//! This module is deliberately NOT a cargo target. It lives under
//! `examples/shared/` (which cargo's example auto-discovery ignores, because it
//! discovers only `examples/*.rs` and `examples/*/main.rs`) so that the
//! integration test `tests/first_artifact.rs` and the artifact-producing
//! example `examples/first_artifact.rs` run *the same* composition rather than
//! two drifting copies of it.
//!
//! **The composition itself no longer lives here.** It was promoted verbatim
//! into `plinth_pipeline::prepare`, which is what `plinth prepare` executes, so
//! the code this test gates and the code the CLI runs are one implementation
//! (O-5). What remains here is the corpus-fixture scaffolding the production
//! executor has no business knowing about — where the checked-in baselines
//! live, how a procedural mesh is fed in without an STL on disk — and the
//! independent disk audit, which is a second reader of the artifact rather
//! than part of producing it.
//!
//! Nothing in this module plans supports, builds geometry, or edits gate
//! machinery. In particular it is a read-only consumer of `plinth-support`: it
//! never pins a support digest, so concurrent gate work can move member counts
//! without touching this seam.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chitubox_io::{Project, SegmentType};
use plinth_core::{ArtifactId, ModelId, PrinterProfile};
use plinth_mesh::{load_binary_stl, Mesh, WeldPolicy};
use plinth_orient::{run as run_orientation, OrientRequest, PinnedBaseline};
use plinth_pipeline::prepare::{
    registered_decimation_budget, registered_support_request, StagePolicy,
};
pub use plinth_pipeline::prepare::{ChainOutcome, ChainStop, SupportedGeometry};
use plinth_pipeline::{OutputProfile, PrepareResourcePolicy};
use plinth_verify::{verify_artifact, StructuralFormatStatus};

/// The lift every sealed-run gate in this workspace expects.
///
/// This is an *expectation*, not the source of truth: the chain reads the lift
/// off the selected printer profile's registered `LIFT_MM`, and the tests
/// assert the emitted artifact carries this number so that a moved placeholder
/// fails loudly instead of quietly changing every artifact.
pub const LIFT_MM: f64 = 5.0;
/// The frozen 0.05 mm layer pitch, likewise an expectation on the profile's
/// registered `LAYER_HEIGHT_DEFAULT_MM` rather than an input to the chain.
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

/// The one production stage policy, assembled exactly as `prepare` assembles
/// it: registered probe budget, registered wall, registered lift.
pub fn stage_policy() -> StagePolicy {
    let printer = saturn4_ultra().clone();
    StagePolicy {
        // Never read on this path: these fixtures hand `run_from_oriented` an
        // already-oriented model, having applied their own checked-in pinned
        // baseline in `orient` below. The field is the run-level policy
        // `prepare` consults when it does the orienting itself.
        orientation: plinth_pipeline::OrientationPolicy::DenseSweep,
        // LANE 2: the legacy engine, named rather than defaulted.
        placement: plinth_pipeline::Placement::Legacy,
        hollow: plinth_hollow::HollowRequest::with_registered_wall_default(printer.clone())
            .expect("registered hollow wall default"),
        support: registered_support_request(&printer).expect("registered lift"),
        resources: PrepareResourcePolicy::for_saturn4_ultra(
            registered_decimation_budget().expect("registered decimation budget"),
        )
        .expect("production resource policy"),
        // The legacy engine reads no sweep plan, so there is nothing to reuse.
        winner_plan_reuse: plinth_pipeline::prepare::WinnerPlanReuse::Enabled,
        exact_delicacy: false,
        hollow_first: false,
    }
}

/// Load, decimate and orient one model from its checked-in pinned baseline.
///
/// **This path deliberately does not run the automatic repair pre-pass**, which
/// `plinth-pipeline`'s production `prepare_one` runs immediately after the
/// load. Two reasons, and neither is "repair is optional": this composition is
/// the frozen-digest harness, and inserting any stage that *could* move
/// geometry into it would put the frozen digests at the mercy of a repair
/// change; and every corpus model is already clean at this gate, so the
/// pre-pass would be a no-op on all eight and would prove nothing here.
/// **Measured, not assumed** (2026-08-10, `--example repair_probe corpus/models`):
/// all eight — including the 429,120-face Weeping Treant — PASS the
/// closed-solid gate raw *and* decimated, with zero shard faces, zero duplicate
/// groups and zero pinch vertices between them. The
/// pre-pass's own rescue evidence lives in `plinth-mesh`'s and
/// `plinth-hollow`'s repair tests, against fixtures that actually carry the
/// defects.
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
    let budget = registered_decimation_budget().map_err(|error| format!("budget: {error}"))?;
    let decimated = plinth_decimate::run(&mesh, budget)
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

/// Why a fixture run did not reach an artifact.
///
/// The two variants are kept apart on purpose. A `Setup` stop is this
/// scaffolding's own failure — a missing corpus STL, an unreadable pinned
/// baseline — and says nothing about the engine. A `Chain` stop is the
/// production executor's typed verdict, forwarded whole so the gate, the
/// geometry it judged and the exit-code projection all survive.
#[derive(Debug)]
pub enum FixtureStop {
    Setup(String),
    Chain(ChainStop),
}

impl FixtureStop {
    #[must_use]
    pub fn is_gates(&self) -> bool {
        match self {
            Self::Chain(stop) => stop.is_gates(),
            Self::Setup(_) => false,
        }
    }

    #[must_use]
    pub fn gate(&self) -> Option<plinth_pipeline::PreWriteGate> {
        match self {
            Self::Chain(stop) => stop.gate(),
            Self::Setup(_) => None,
        }
    }

    /// Whether **support planning** is what stopped this model. See
    /// `ChainStop::is_support_refusal` (FU-56: a coverage-closure refusal is
    /// excluded).
    #[must_use]
    pub fn is_support_refusal(&self) -> bool {
        match self {
            Self::Chain(stop) => stop.is_support_refusal(),
            Self::Setup(_) => false,
        }
    }

    /// Whether this stop is specifically a coverage-closure refusal (FU-56).
    /// See `ChainStop::is_coverage_closure_refusal`.
    #[must_use]
    pub fn is_coverage_closure_refusal(&self) -> bool {
        match self {
            Self::Chain(stop) => stop.is_coverage_closure_refusal(),
            Self::Setup(_) => false,
        }
    }

    #[must_use]
    pub fn refused_geometry(&self) -> Option<&SupportedGeometry> {
        match self {
            Self::Chain(stop) => stop.refused_geometry(),
            Self::Setup(_) => None,
        }
    }

    #[must_use]
    pub fn detail(&self) -> String {
        match self {
            Self::Chain(stop) => stop.detail(),
            Self::Setup(reason) => reason.clone(),
        }
    }
}

impl std::fmt::Display for FixtureStop {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Chain(stop) => stop.fmt(formatter),
            Self::Setup(reason) => formatter.write_str(reason),
        }
    }
}

/// The whole production chain for one already-oriented model.
pub fn run_from_oriented(
    name: &str,
    source_faces: usize,
    oriented: plinth_orient::OrientedModel,
    target: &Path,
) -> Result<ChainOutcome, FixtureStop> {
    plinth_pipeline::prepare::run_from_oriented(
        &ModelId::new(name),
        source_faces,
        oriented,
        &stage_policy(),
        target,
        None,
    )
    .map(|outcome| outcome.expect("a chain with no stop_after always reaches an outcome"))
    .map_err(FixtureStop::Chain)
}

/// The whole chain for one corpus model, start to written file.
pub fn run_corpus_model(name: &str, target: &Path) -> Result<ChainOutcome, FixtureStop> {
    let (source, oriented) = prepare_from_corpus(name).map_err(FixtureStop::Setup)?;
    run_from_oriented(name, source.tris().len(), oriented, target)
}

/// The whole chain for an in-repo procedural mesh.
pub fn run_procedural_model(
    name: &str,
    mesh: Mesh,
    target: &Path,
) -> Result<ChainOutcome, FixtureStop> {
    let source_faces = mesh.tris().len();
    let (_, oriented) = prepare_from_mesh(mesh, name).map_err(FixtureStop::Setup)?;
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
// GEOM-EXEMPT(independent disk audit -- the module doc at the top of this
// file names it directly: "the independent on-disk audit that re-reads what
// the chain wrote... a second reader of the artifact rather than part of
// producing it." Its `volume > 0.0` result at the call site feeds the
// outward-winding count precisely because it is re-derived from bytes rather
// than trusted; importing geom_kernel::vec3::cross/dot here would make this
// check share arithmetic with the producer it verifies (plinth-emit adopts
// geom_kernel::vec3 under canon C1), so an error common to both would go
// undetected. Same principle as geometry-canon.md §4.1 (plinth-audit) and
// §4.7 (plinth-verify), extended here by analogy since this file is not
// itself named in canon §4.)
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
