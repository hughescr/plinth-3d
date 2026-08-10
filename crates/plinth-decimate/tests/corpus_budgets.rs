//! M3's provisioned, real-corpus acceptance gate.
//!
//! This is intentionally an integration test rather than a CLI test.  The
//! source STLs are ignored local inputs, so their absence is a named failure
//! of this explicit gate, never an accidental pass or a skip.  Successful
//! invocations leave a human-inspectable Treant before/after pair plus a
//! deterministic, relative-path-free report in the selected artifact directory.

use std::fs;
use std::path::{Path, PathBuf};

use geom_kernel::NativeSurfaceConfig;
use plinth_decimate::{
    run, BudgetSearchOutcome, BudgetSearchSuccess, DecimationBudget, ProbeConfig,
};
use plinth_mesh::{load_binary_stl, write_binary_stl, Mesh, TopologyCounts, WeldPolicy};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// v8 (2026-08-11) adds `search_candidate_free_ratios`. A bisection midpoint
/// that yields no usable candidate publishes no `SearchStepReport`, so a
/// step-count audit that reads `search_steps.len()` alone would reject a model
/// that verified perfectly well after stepping past one unusable ratio. The
/// count is now recorded and the audit checks `steps + candidate_free_ratios`.
///
/// v7 (2026-08-10) turns v6's single `zero_volume_pairs_removed` into the full
/// `zero_volume_repair` record — `pairs_removed`, `vertices_dropped`,
/// `locked_vertices_dropped` — and records it on **every** candidate the
/// search describes rather than only on the winner: each bisection step, each
/// adaptive probe, each final attempt, plus the mandatory ratio-0.20 feature
/// probe that decides the entire protected set. Every fingerprint in this
/// report is now self-describing: a reader can tell from the record alone
/// whether the mesh behind it is meshoptimizer's own result or a repaired one.
/// `locked_vertices_dropped` is the one that had no witness at all: locks are
/// checked before the removal, so a protected vertex whose whole surviving
/// incidence sat inside removed pairs was dropped with no error and no trace
/// entry. It is a deliberate ordering (the other order refuses every Treant
/// candidate at every ratio) and it is now audited rather than silent.
/// `candidate_free_probes` and the `CandidateFree` refusals are also new: a
/// ratio that yields no candidate is recorded and stepped past instead of
/// ending the run.
///
/// v6 (2026-08-10) adds `zero_volume_pairs_removed` to every final attempt.
/// meshoptimizer's collapse is not link-condition checked, so it manufactures
/// opposite-winding coincident face pairs — zero-volume artifacts of a collapse
/// that already happened — and `compact_result` now deletes them before the
/// candidate becomes a mesh. That deletion is the only edit this crate makes to
/// a meshoptimizer result, so it is serialized beside the counts it explains:
/// with it, the v5 duplicate counters read zero on candidates that previously
/// carried hundreds (1,296 duplicate faces on the Weeping Treant v1.2 at
/// r = 0.525, and the 30,724 cut faces they dragged with them fall to 3,828).
///
/// v5 (2026-08-09) adds `duplicate_faces` and `duplicate_face_groups` to every
/// topology report. Duplicate faces are excluded from `nonmanifold_edges` by
/// design and, before v5, were counted nowhere at all, so a reduction could
/// fuse hundreds of coincident faces and the gate's report said nothing — 664
/// duplicate-face groups at r = 0.50 on the Weeping Treant v1.2 were a scratch
/// measurement precisely because no committed instrument counted them (FU-26).
///
/// v4 (2026-08-07) adds the worst-case deviation `max_mm` to every probe the
/// search records — sampled bisection steps, final full-population attempts,
/// and both the sampled and full adaptive probes — plus the model-level
/// `final_max_mm`, the `DECIMATION_MAX_ABSOLUTE_DEVIATION_MM` bound it is
/// judged against, and the search's `native_surface_builds`. `max_mm` is the
/// quantity U3 exists for and the one the ratified p99/p99.9 budget is blind
/// to; before v4 it was measured on every probe and serialized nowhere, so the
/// design record's §8 max-deviation figures could not be reproduced from a
/// clean checkout without re-adding scratch instrumentation. Per-phase
/// wall-clock, which §6 Phase 1 also lists, is deliberately NOT serialized —
/// see the record's §9.8.
const REPORT_SCHEMA: &str = "plinth/m3-decimation-corpus-budget-report/v8";
const DEFAULT_OUTPUT_DIR: &str = "target/plinth-decimate-corpus-budgets";
const EXPECTED: [CorpusModel; 8] = [
    CorpusModel::already_print_scale(
        "Sphere",
        "corpus/models/Sphere.stl",
        "48c27bb596a265900a614171d7ffe3a7a4ea80d95f1c620cf3ab00f67fe0a1d5",
    ),
    CorpusModel::already_print_scale(
        "Cylinder",
        "corpus/models/Cylinder.stl",
        "7bd60bd7fb83882f2bdc9c977e4f15e55f5cf0342c05c31584c586782505f741",
    ),
    CorpusModel::already_print_scale(
        "make_box",
        "corpus/models/make_box.stl",
        "aada5b79e87461c6f74afe4e16a5f651aed47fd6638a50a225e35760a8010c3b",
    ),
    CorpusModel::already_print_scale(
        "make_overhang_table",
        "corpus/models/make_overhang_table.stl",
        "a776132a2cc4a43298d9ca646a4265a7ba60ed7d37de7618147ac754dfe8c996",
    ),
    CorpusModel::already_print_scale(
        "make_toed_slab",
        "corpus/models/make_toed_slab.stl",
        "ceb70f2b549fb131b0f177033401c50879a8fbc6d575b27459c0f9574d9f1169",
    ),
    CorpusModel::already_print_scale(
        "make_raking_blade",
        "corpus/models/make_raking_blade.stl",
        "0979f5812c26b3bd88f6e36bb792573fbdc9c023656c46544f2bffdecddefb97",
    ),
    CorpusModel::already_print_scale(
        "make_thin_fin",
        "corpus/models/make_thin_fin.stl",
        "09bec5c6ff8ff169f91438c209b12c3e6d9eb0dbb7e2fd7611e98d1884516c4c",
    ),
    CorpusModel::already_print_scale(
        "Weeping Treant v1.2",
        "corpus/models/Weeping Treant v1.2.stl",
        "4c027b7849019ab9598c85f285b9df7d480aacfd1733123a32a8a81239525552",
    ),
];

#[derive(Clone, Copy)]
struct CorpusModel {
    name: &'static str,
    stl: &'static str,
    sha256: &'static str,
    expected_status: ExpectedStatus,
}

impl CorpusModel {
    const fn already_print_scale(
        name: &'static str,
        stl: &'static str,
        sha256: &'static str,
    ) -> Self {
        Self {
            name,
            stl,
            sha256,
            expected_status: ExpectedStatus::AlreadyPrintScale,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ExpectedStatus {
    Verified,
    AlreadyPrintScale,
    SkippedScaleSuspicious,
}

#[derive(Serialize, Deserialize)]
struct GateReport {
    schema: String,
    /// No cache exists in plinth-decimate: the only persistent data is the
    /// caller-owned artifact written below.  Thus cache-on/off equivalence is
    /// structurally inapplicable at this library stage.
    cache: CacheStatus,
    models: Vec<ModelReport>,
}

#[derive(Serialize, Deserialize)]
struct CacheStatus {
    exists: bool,
    equivalence: String,
}

#[derive(Serialize, Deserialize)]
struct ModelReport {
    name: String,
    expected_status: ExpectedStatus,
    actual_status: String,
    input_faces: Option<usize>,
    output_faces: Option<usize>,
    /// Decimal is presentation only; exact input/output face counts establish
    /// the achieved ratio without a JSON f64 lexical round-trip.
    chosen_ratio: Option<String>,
    search_steps: Vec<SearchStepReport>,
    /// Bisection midpoints that produced no usable candidate, so they have no
    /// `SearchStepReport` to publish. `search_steps.len() +
    /// search_candidate_free_ratios` is what the step-count audit checks; each
    /// one is also named in `reduction_refusals`.
    search_candidate_free_ratios: Option<usize>,
    search_p99_mm: Option<String>,
    search_p99_9_mm: Option<String>,
    final_p99_mm: Option<String>,
    final_p99_9_mm: Option<String>,
    /// The worst-case two-sided deviation of the returned mesh. This is the
    /// quantity U3 is the only clause to see: on the Weeping Treant v1.2 it
    /// reaches 0.0566 mm while `p99_9` is still 0.000073 mm.
    final_max_mm: Option<String>,
    final_p99_limit_mm: Option<String>,
    final_p99_9_limit_mm: Option<String>,
    /// `DECIMATION_MAX_ABSOLUTE_DEVIATION_MM`, the registered `Derived` bound
    /// `final_max_mm` is judged against. Unlike the two percentile limits it
    /// carries no ×1.1 recheck slack.
    final_max_limit_mm: Option<String>,
    /// Two per `probe_two_sided` call the search made, plus one more built by
    /// `run` for the feature-protection backward error map, which the search
    /// never sees.
    search_native_surface_builds: Option<usize>,
    topology_before: Option<TopologyReport>,
    topology_after: Option<TopologyReport>,
    final_attempts: Vec<FinalAttemptReport>,
    adaptive_exploration: Option<AdaptiveExplorationReport>,
    reduction_refusals: Vec<String>,
    error: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct SearchStepReport {
    ratio: String,
    output_faces: usize,
    reduced: bool,
    p99_mm: String,
    p99_9_mm: String,
    max_mm: Option<String>,
    zero_volume_repair: ZeroVolumeRepairReport,
    passed: bool,
}

/// The one mesh edit `plinth-decimate` makes to a meshoptimizer result,
/// serialized beside every fingerprint it applies to.
#[derive(Clone, Copy, Serialize, Deserialize)]
struct ZeroVolumeRepairReport {
    pairs_removed: usize,
    vertices_dropped: usize,
    /// Protected vertices surrendered to the removal. Locks are checked
    /// against meshoptimizer's own result *before* the pairs are deleted, so
    /// this can be non-zero without a `LockedVertexRemoved` error; that is the
    /// point of serializing it.
    locked_vertices_dropped: usize,
}

impl From<plinth_decimate::ZeroVolumeRepairRecord> for ZeroVolumeRepairReport {
    fn from(record: plinth_decimate::ZeroVolumeRepairRecord) -> Self {
        Self {
            pairs_removed: record.pairs_removed,
            vertices_dropped: record.vertices_dropped,
            locked_vertices_dropped: record.locked_vertices_dropped,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct AdaptiveExplorationReport {
    probe_limit: usize,
    probes: Vec<AdaptiveProbeReport>,
    /// Integer targets that consumed a probe from `probe_limit` and produced
    /// no candidate at all, so they have no `AdaptiveProbeReport`. Each is
    /// named in `reduction_refusals` as a `CandidateFree` refusal.
    candidate_free_probes: usize,
    remaining_intervals: Vec<[usize; 2]>,
    complete: bool,
}

#[derive(Serialize, Deserialize)]
struct AdaptiveProbeReport {
    target_faces: usize,
    output_faces: usize,
    candidate_fingerprint: String,
    plateau_of_fingerprint: Option<String>,
    sampled_p99_mm: String,
    sampled_p99_9_mm: String,
    sampled_max_mm: Option<String>,
    zero_volume_repair: ZeroVolumeRepairReport,
    topology_after: Option<TopologyReport>,
    full_p99_mm: Option<String>,
    full_p99_9_mm: Option<String>,
    full_max_mm: Option<String>,
    result: String,
}

#[derive(Serialize, Deserialize)]
struct FinalAttemptReport {
    requested_ratio: String,
    achieved_ratio: String,
    output_faces: usize,
    candidate_fingerprint: String,
    p99_mm: Option<String>,
    p99_9_mm: Option<String>,
    max_mm: Option<String>,
    topology_after: TopologyReport,
    zero_volume_repair: ZeroVolumeRepairReport,
    result: String,
    downstream_utility: String,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
struct TopologyReport {
    boundary_edges: usize,
    nonmanifold_edges: usize,
    duplicate_faces: usize,
    duplicate_face_groups: usize,
}

impl From<TopologyCounts> for TopologyReport {
    fn from(counts: TopologyCounts) -> Self {
        Self {
            boundary_edges: counts.boundary_edges,
            nonmanifold_edges: counts.nonmanifold_edges,
            duplicate_faces: counts.duplicate_faces,
            duplicate_face_groups: counts.duplicate_face_groups,
        }
    }
}

#[test]
fn corpus_budgets() {
    let root = workspace_root();
    let output_dir = output_directory(&root);
    let report = match execute_gate(&root, &output_dir) {
        Ok(report) => report,
        Err(reason) => panic!("M3 corpus budget gate failed: {reason}"),
    };
    let report_path = output_dir.join("corpus-budget-report.json");
    write_and_reopen_report(&report_path, &report).unwrap_or_else(|reason| {
        panic!("M3 corpus budget gate could not persist/reopen its report: {reason}")
    });
    let failures = report
        .models
        .iter()
        .filter(|model| {
            model.actual_status != expected_status_name(model.expected_status)
                || model.error.is_some()
        })
        .map(|model| {
            format!(
                "{}: {}",
                model.name,
                model.error.as_deref().unwrap_or(&model.actual_status)
            )
        })
        .collect::<Vec<_>>();
    assert!(
        failures.is_empty(),
        "M3 corpus budget gate report {} records failures: {}",
        report_path.display(),
        failures.join("; ")
    );
    eprintln!(
        "M3 corpus budget report: {}; Treant before/after: {} / {}",
        report_path.display(),
        output_dir.join("Weeping Treant v1.2.before.stl").display(),
        output_dir.join("Weeping Treant v1.2.after.stl").display()
    );
}

/// The gate's own arithmetic, pinned away from the eight corpus STLs.
///
/// The end-to-end shape this guards — a model that verifies *through* a
/// candidate-free midpoint — cannot be manufactured here: reaching it would
/// mean engineering a mesh on which a vendored heuristic produces one exact
/// unusable answer at one exact ratio and safe answers at the rest, which is
/// not a thing a regression test may depend on. What can be pinned exactly is
/// the conservation law the gate reads, and `plinth-decimate`'s own
/// `every_bisection_being_candidate_free_refuses_by_name_instead_of_ending_the_run`
/// pins the producing half: a search whose midpoints are all refused reports
/// one step and five candidate-free ratios, and they still sum to six.
#[test]
fn the_step_count_audit_counts_a_candidate_free_midpoint_as_a_bisection() {
    // Every midpoint produced a candidate: the only shape the audit used to
    // accept.
    assert!(audit_search_step_count(6, 0).is_ok());
    // One midpoint was unusable, was named as a `CandidateFree` refusal and was
    // stepped past. The model can still verify, and before this fix the gate
    // rejected it with `M3SearchStepCount` — a complaint about the search's
    // shape for something that is a fact about one ratio. Latent only because
    // all eight corpus members are `already_print_scale`.
    assert!(audit_search_step_count(5, 1).is_ok());
    assert!(audit_search_step_count(2, 4).is_ok());
    assert!(audit_search_step_count(1, 5).is_ok());
    // A genuinely short search is still a failure: refusals must be *counted*,
    // not merely tolerated.
    assert!(audit_search_step_count(5, 0).is_err());
    assert!(audit_search_step_count(4, 1).is_err());
    // And a long one: the bisection budget is a cap, never a target to exceed.
    assert!(audit_search_step_count(6, 1).is_err());
    assert!(audit_search_step_count(7, 0).is_err());
    // The canonical ratio-1 identity step is mandatory and is never
    // candidate-free, so it can never be one of the counted refusals.
    assert!(audit_search_step_count(0, 6).is_err());
}

#[test]
fn report_writer_canonicalizes_arithmetic_floats_before_the_only_disk_write() {
    let report = GateReport {
        schema: REPORT_SCHEMA.to_owned(),
        cache: CacheStatus {
            exists: false,
            equivalence: "not_applicable_no_library_cache".to_owned(),
        },
        models: vec![ModelReport {
            name: "arithmetic-f64-regression".to_owned(),
            expected_status: ExpectedStatus::Verified,
            actual_status: "verified".to_owned(),
            input_faces: Some(2),
            output_faces: Some(1),
            chosen_ratio: Some("0.50000000000000000".to_owned()),
            search_steps: vec![SearchStepReport {
                ratio: "0.50000000000000000".to_owned(),
                output_faces: 1,
                reduced: true,
                p99_mm: ratio_text(0.0),
                p99_9_mm: ratio_text(0.0),
                max_mm: Some(ratio_text(0.0)),
                zero_volume_repair: ZeroVolumeRepairReport {
                    pairs_removed: 0,
                    vertices_dropped: 0,
                    locked_vertices_dropped: 0,
                },
                passed: true,
            }],
            search_candidate_free_ratios: Some(0),
            search_p99_mm: Some(ratio_text(0.0)),
            search_p99_9_mm: Some(ratio_text(0.0)),
            final_p99_mm: Some(ratio_text(0.0)),
            final_p99_9_mm: Some(ratio_text(0.0)),
            final_max_mm: Some(ratio_text(0.0)),
            final_p99_limit_mm: Some(ratio_text(0.0095 * 1.1)),
            final_p99_9_limit_mm: Some(ratio_text(0.019 * 1.1)),
            final_max_limit_mm: Some(ratio_text(0.08)),
            search_native_surface_builds: Some(2),
            topology_before: Some(TopologyReport {
                boundary_edges: 0,
                nonmanifold_edges: 0,
                duplicate_faces: 0,
                duplicate_face_groups: 0,
            }),
            topology_after: Some(TopologyReport {
                boundary_edges: 0,
                nonmanifold_edges: 0,
                duplicate_faces: 0,
                duplicate_face_groups: 0,
            }),
            final_attempts: Vec::new(),
            adaptive_exploration: None,
            reduction_refusals: Vec::new(),
            error: None,
        }],
    };
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/report-f64-canonicalization-regression.json");
    fs::create_dir_all(path.parent().expect("target parent")).expect("create target directory");
    write_and_reopen_report(&path, &report).expect("canonical report audit");
}

fn execute_gate(root: &Path, output_dir: &Path) -> Result<GateReport, String> {
    let models_dir = root.join("corpus/models");
    let metadata = fs::metadata(&models_dir).map_err(|error| {
        format!(
            "RequiredCorpusDirectoryMissing: required corpus/models directory {} is missing or unreadable: {error}",
            models_dir.display()
        )
    })?;
    if !metadata.is_dir() {
        return Err(format!(
            "RequiredCorpusDirectoryMissing: {} exists but is not a directory",
            models_dir.display()
        ));
    }
    fs::create_dir_all(output_dir).map_err(|error| {
        format!(
            "could not create M3 corpus output directory {}: {error}",
            output_dir.display()
        )
    })?;

    // An explicit finite construction cap, not a default: `usize::MAX` is now
    // refused by `DecimationBudget::validate_for_execution` as no budget at
    // all.  Only `grid_pitch_mm` is fingerprinted into the native surface, so
    // this cap is result-inert once a surface exists; it can only refuse
    // construction, which the expected per-model statuses below would catch.
    let budget = DecimationBudget::ratified_default(ProbeConfig {
        surface: NativeSurfaceConfig {
            grid_pitch_mm: 1.0,
            max_grid_bytes: 64 * 1024 * 1024,
        },
        nearest_upper_bound_mm: 10_000.0,
    })
    .map_err(|error| format!("could not construct default 19um M3 budget: {error}"))?;
    let mut models = Vec::with_capacity(EXPECTED.len());
    for expected in EXPECTED {
        eprintln!("M3 corpus budget gate: running {}", expected.name);
        let input = required_input(root, expected)?;
        let original = load_binary_stl(&input, WeldPolicy::None).map_err(|error| {
            format!(
                "RequiredCorpusModelUnreadable: {} at {} could not be loaded without repair: {error}",
                expected.name,
                input.display()
            )
        })?;
        models.push(run_one(expected, &original, budget, output_dir)?);
    }
    Ok(GateReport {
        schema: REPORT_SCHEMA.to_owned(),
        cache: CacheStatus {
            exists: false,
            equivalence: "not_applicable_no_library_cache".to_owned(),
        },
        models,
    })
}

fn required_input(root: &Path, expected: CorpusModel) -> Result<PathBuf, String> {
    let path = root.join(expected.stl);
    let bytes = fs::read(&path).map_err(|error| {
        format!(
            "RequiredCorpusModelMissing: {} at {} is missing or unreadable: {error}",
            expected.name,
            path.display()
        )
    })?;
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual != expected.sha256 {
        return Err(format!(
            "RequiredCorpusModelDigestMismatch: {} at {} expected {}, found {}",
            expected.name,
            path.display(),
            expected.sha256,
            actual
        ));
    }
    Ok(path)
}

fn run_one(
    expected: CorpusModel,
    original: &Mesh,
    budget: DecimationBudget,
    output_dir: &Path,
) -> Result<ModelReport, String> {
    match run(original, budget) {
        Ok(BudgetSearchOutcome::SkippedScaleSuspicious { scale }) => Ok(ModelReport {
            name: expected.name.to_owned(),
            expected_status: expected.expected_status,
            actual_status: expected_status_name(ExpectedStatus::SkippedScaleSuspicious).to_owned(),
            input_faces: Some(original.tris().len()),
            output_faces: None,
            chosen_ratio: None,
            search_steps: Vec::new(),
            search_candidate_free_ratios: None,
            search_p99_mm: None,
            search_p99_9_mm: None,
            final_p99_mm: None,
            final_p99_9_mm: None,
            final_max_mm: None,
            final_p99_limit_mm: None,
            final_p99_9_limit_mm: None,
            final_max_limit_mm: None,
            search_native_surface_builds: None,
            topology_before: None,
            topology_after: None,
            final_attempts: Vec::new(),
            adaptive_exploration: None,
            reduction_refusals: Vec::new(),
            error: Some(format!(
                "ScaleSuspicious: longest axis {} mm is outside [10, 300] mm; no candidate exported",
                scale.longest_axis_mm
            )),
        }),
        Ok(BudgetSearchOutcome::Verified(success)) => Ok(success_report(
            expected,
            original,
            success.as_ref(),
            ExpectedStatus::Verified,
            output_dir,
        )),
        Ok(BudgetSearchOutcome::AlreadyPrintScale(success)) => Ok(success_report(
            expected,
            original,
            success.as_ref(),
            ExpectedStatus::AlreadyPrintScale,
            output_dir,
        )),
        Err(error) => Ok(ModelReport {
            name: expected.name.to_owned(),
            expected_status: expected.expected_status,
            actual_status: "gate_execution_failure".to_owned(),
            input_faces: Some(original.tris().len()),
            output_faces: None,
            chosen_ratio: None,
            search_steps: Vec::new(),
            search_candidate_free_ratios: None,
            search_p99_mm: None,
            search_p99_9_mm: None,
            final_p99_mm: None,
            final_p99_9_mm: None,
            final_max_mm: None,
            final_p99_limit_mm: None,
            final_p99_9_limit_mm: None,
            final_max_limit_mm: None,
            search_native_surface_builds: None,
            topology_before: None,
            topology_after: None,
            final_attempts: Vec::new(),
            adaptive_exploration: None,
            reduction_refusals: Vec::new(),
            error: Some(error.to_string()),
        }),
    }
}

fn success_report(
    expected: CorpusModel,
    original: &Mesh,
    success: &BudgetSearchSuccess,
    actual_status: ExpectedStatus,
    output_dir: &Path,
) -> ModelReport {
    let trace = success.trace();
    let final_probe = success.final_probe();
    let contract_error = match actual_status {
        ExpectedStatus::Verified => {
            independently_audit_verified(expected, original, success, output_dir).err()
        }
        ExpectedStatus::AlreadyPrintScale => {
            independently_audit_already_print_scale(expected, original, success, output_dir).err()
        }
        ExpectedStatus::SkippedScaleSuspicious => unreachable!("success cannot be a scale skip"),
    };
    ModelReport {
        name: expected.name.to_owned(),
        expected_status: expected.expected_status,
        actual_status: expected_status_name(actual_status).to_owned(),
        input_faces: Some(original.tris().len()),
        output_faces: Some(success.mesh().tris().len()),
        chosen_ratio: Some(ratio_text(success.chosen_ratio())),
        search_steps: trace
            .steps
            .iter()
            .map(|step| SearchStepReport {
                ratio: ratio_text(step.ratio),
                output_faces: step.output_face_count,
                reduced: step.reduced,
                p99_mm: ratio_text(step.probe.p99_mm),
                p99_9_mm: ratio_text(step.probe.p99_9_mm),
                max_mm: step.probe.max_mm.map(ratio_text),
                zero_volume_repair: step.zero_volume_repair.into(),
                passed: step.passed,
            })
            .collect(),
        search_candidate_free_ratios: Some(trace.candidate_free_ratios),
        search_p99_mm: trace.steps.last().map(|step| ratio_text(step.probe.p99_mm)),
        search_p99_9_mm: trace
            .steps
            .last()
            .map(|step| ratio_text(step.probe.p99_9_mm)),
        final_p99_mm: Some(ratio_text(final_probe.p99_mm)),
        final_p99_9_mm: Some(ratio_text(final_probe.p99_9_mm)),
        final_max_mm: final_probe.max_mm.map(ratio_text),
        final_p99_limit_mm: Some(ratio_text(trace.budget.p99_mm * 1.1)),
        final_p99_9_limit_mm: Some(ratio_text(trace.budget.p99_9_mm * 1.1)),
        final_max_limit_mm: Some(ratio_text(registered_max_absolute_deviation_mm())),
        search_native_surface_builds: Some(trace.native_surface_builds),
        topology_before: Some(success.topology_before().into()),
        topology_after: Some(success.topology_after().into()),
        final_attempts: trace
            .final_attempts
            .iter()
            .map(|attempt| FinalAttemptReport {
                requested_ratio: ratio_text(attempt.requested_ratio),
                achieved_ratio: ratio_text(attempt.achieved_ratio),
                output_faces: attempt.output_face_count,
                candidate_fingerprint: attempt.candidate_fingerprint.clone(),
                p99_mm: attempt.probe.as_ref().map(|probe| ratio_text(probe.p99_mm)),
                p99_9_mm: attempt
                    .probe
                    .as_ref()
                    .map(|probe| ratio_text(probe.p99_9_mm)),
                max_mm: attempt
                    .probe
                    .as_ref()
                    .and_then(|probe| probe.max_mm)
                    .map(ratio_text),
                topology_after: attempt.topology_after.into(),
                zero_volume_repair: attempt.zero_volume_repair.into(),
                result: format!("{:?}", attempt.result),
                downstream_utility: format!("{:?}", attempt.downstream_utility),
            })
            .collect(),
        adaptive_exploration: Some(AdaptiveExplorationReport {
            probe_limit: trace.adaptive_exploration.probe_limit,
            candidate_free_probes: trace.adaptive_exploration.candidate_free_probes,
            probes: trace
                .adaptive_exploration
                .probes
                .iter()
                .map(|probe| AdaptiveProbeReport {
                    target_faces: probe.target_face_count,
                    output_faces: probe.output_face_count,
                    candidate_fingerprint: probe.candidate_fingerprint.clone(),
                    plateau_of_fingerprint: probe.plateau_of_fingerprint.clone(),
                    sampled_p99_mm: ratio_text(probe.sampled_probe.p99_mm),
                    sampled_p99_9_mm: ratio_text(probe.sampled_probe.p99_9_mm),
                    sampled_max_mm: probe.sampled_probe.max_mm.map(ratio_text),
                    zero_volume_repair: probe.zero_volume_repair.into(),
                    topology_after: probe.topology_after.map(Into::into),
                    full_p99_mm: probe
                        .full_probe
                        .as_ref()
                        .map(|full| ratio_text(full.p99_mm)),
                    full_p99_9_mm: probe
                        .full_probe
                        .as_ref()
                        .map(|full| ratio_text(full.p99_9_mm)),
                    full_max_mm: probe
                        .full_probe
                        .as_ref()
                        .and_then(|full| full.max_mm)
                        .map(ratio_text),
                    result: format!("{:?}", probe.result),
                })
                .collect(),
            remaining_intervals: trace
                .adaptive_exploration
                .remaining_intervals
                .iter()
                .map(|interval| [interval.first_target_faces, interval.last_target_faces])
                .collect(),
            complete: trace.adaptive_exploration.complete,
        }),
        reduction_refusals: trace
            .reduction_refusals
            .iter()
            .map(|reason| format!("{reason:?}"))
            .collect(),
        error: contract_error,
    }
}

fn independently_audit_already_print_scale(
    expected: CorpusModel,
    original: &Mesh,
    success: &BudgetSearchSuccess,
    output_dir: &Path,
) -> Result<(), String> {
    if success.chosen_ratio() != 1.0 || success.mesh() != original {
        return Err(
            "AlreadyPrintScaleIdentity: expected unchanged canonical mesh at ratio 1.0".to_owned(),
        );
    }
    if success.final_probe().p99_mm != 0.0
        || success.final_probe().p99_9_mm != 0.0
        || success.final_probe().max_mm != Some(0.0)
    {
        return Err(
            "AlreadyPrintScaleIdentityError: identity evidence was not exactly zero".to_owned(),
        );
    }
    if success.topology_before() != success.topology_after()
        || success.trace().reduction_refusals.is_empty()
    {
        return Err("AlreadyPrintScaleEvidence: topology or refusal evidence missing".to_owned());
    }
    if success.trace().final_attempts.iter().any(|attempt| {
        attempt.result == plinth_decimate::FinalVerificationResult::GeometricallyEligible
            && attempt.downstream_utility
                != plinth_decimate::DownstreamUtilityEvidence::NoDownstreamUtilityEvidence
    }) {
        // A granted witness on a geometrically eligible attempt would have
        // produced `Verified`, not this outcome, so seeing one here means the
        // search returned the original after minting downstream evidence.
        return Err(
            "AlreadyPrintScaleUtility: downstream evidence was minted but no reduction was returned"
                .to_owned(),
        );
    }
    if !success.trace().reduction_refusals.iter().any(|reason| {
        matches!(
            reason,
            plinth_decimate::ReductionRefusalReason::NoDownstreamUtilityEvidence { .. }
        )
    }) && success.trace().final_attempts.iter().any(|attempt| {
        attempt.result == plinth_decimate::FinalVerificationResult::GeometricallyEligible
    }) {
        return Err("AlreadyPrintScaleUtility: missing named utility refusal".to_owned());
    }
    audit_no_lock_was_surrendered(success.trace())?;
    success
        .mesh()
        .audit_strict()
        .map_err(|error| error.to_string())?;
    if expected.name == "Weeping Treant v1.2" {
        write_and_reopen_treant_artifacts(output_dir, original, success.mesh())?;
    }
    Ok(())
}

/// The search runs one canonical identity step and then exactly `BISECTIONS`
/// midpoints, and every midpoint accounts for itself: either it produced a
/// candidate and pushed a `BudgetSearchStep`, or it produced none, was named as
/// a `CandidateFree` refusal, and was counted.
///
/// Reading `steps.len()` alone — which this audit did — silently required every
/// midpoint to succeed. Any model that hit one unusable ratio and still
/// verified would have failed the gate with `M3SearchStepCount`, a message
/// about the search's shape for something that is a fact about one ratio. The
/// eight current corpus members are all `already_print_scale`, which is the only
/// reason that was latent rather than a live failure.
fn audit_search_step_count(steps: usize, candidate_free_ratios: usize) -> Result<(), String> {
    const BISECTIONS: usize = 5;
    if steps + candidate_free_ratios != BISECTIONS + 1 {
        return Err(format!(
            "M3SearchStepCount: expected identity plus {BISECTIONS} bisections, found {steps} \
             steps and {candidate_free_ratios} candidate-free ratios"
        ));
    }
    if steps == 0 {
        return Err(
            "M3SearchStepCount: the canonical identity step is mandatory and never candidate-free"
                .to_owned(),
        );
    }
    Ok(())
}

/// `ZeroVolumeRepairRecord::locked_vertices_dropped` documents itself as
/// structurally zero on every emitted candidate — `compact_result` re-checks the
/// hard lock against the *repaired* face list and refuses the candidate rather
/// than emitting a mesh that gave a protected vertex up. The doc claims this
/// gate's independent audit requires it to read zero everywhere; before this
/// function that audit did not exist, and the field was a constant masquerading
/// as a measurement. This is the standing witness it names.
fn audit_no_lock_was_surrendered(trace: &plinth_decimate::BudgetSearchTrace) -> Result<(), String> {
    let total = trace.zero_volume_repair_total();
    if total.locked_vertices_dropped != 0 {
        return Err(format!(
            "M3LockSurrendered: {} hard-locked vertices were dropped by the zero-volume repair; \
             the emitted candidate must refuse instead",
            total.locked_vertices_dropped
        ));
    }
    // The mandatory ratio-0.20 feature probe is not one of the trace's
    // candidates and so is not in the total above, but it decides the entire
    // protected set and is the one record that carried no witness at all before
    // the repair record was plumbed onto it.
    for reason in &trace.reduction_refusals {
        if let plinth_decimate::ReductionRefusalReason::ProtectionCap { trace } = reason {
            if trace.probe_zero_volume_repair.locked_vertices_dropped != 0 {
                return Err(format!(
                    "M3LockSurrendered: the ratio-{} feature probe dropped {} hard-locked vertices",
                    trace.probe_ratio, trace.probe_zero_volume_repair.locked_vertices_dropped
                ));
            }
        }
    }
    Ok(())
}

fn independently_audit_verified(
    expected: CorpusModel,
    original: &Mesh,
    success: &BudgetSearchSuccess,
    output_dir: &Path,
) -> Result<(), String> {
    let trace = success.trace();
    let search_p99_limit = trace.budget.p99_mm;
    let search_p99_9_limit = trace.budget.p99_9_mm;
    let final_probe = success.final_probe();
    let topology_before = success.topology_before();
    let topology_after = success.topology_after();
    if success.mesh().tris().len() >= original.tris().len() {
        return Err(format!(
            "M3NoReduction: requested production decimation retained {} of {} faces",
            success.mesh().tris().len(),
            original.tris().len()
        ));
    }
    audit_search_step_count(trace.steps.len(), trace.candidate_free_ratios)?;
    audit_no_lock_was_surrendered(trace)?;
    for step in &trace.steps {
        if step.passed
            && (step.probe.p99_mm > search_p99_limit || step.probe.p99_9_mm > search_p99_9_limit)
        {
            return Err(format!(
                "M3SearchBudgetExceeded: ratio {} has p99={} (limit {}) and p99.9={} (limit {})",
                step.ratio,
                step.probe.p99_mm,
                search_p99_limit,
                step.probe.p99_9_mm,
                search_p99_9_limit
            ));
        }
    }
    if final_probe.p99_mm > search_p99_limit * 1.1
        || final_probe.p99_9_mm > search_p99_9_limit * 1.1
    {
        return Err(format!(
            "M3FinalFullPopulationBudgetExceeded: p99={} (limit {}) p99.9={} (limit {})",
            final_probe.p99_mm,
            search_p99_limit * 1.1,
            final_probe.p99_9_mm,
            search_p99_9_limit * 1.1
        ));
    }
    if topology_after.boundary_edges > topology_before.boundary_edges
        || topology_after.nonmanifold_edges > topology_before.nonmanifold_edges
    {
        return Err(format!(
            "M3TopologyIncreased: before boundary={} nonmanifold={}; after boundary={} nonmanifold={}",
            topology_before.boundary_edges,
            topology_before.nonmanifold_edges,
            topology_after.boundary_edges,
            topology_after.nonmanifold_edges
        ));
    }
    if expected.name == "Weeping Treant v1.2" {
        write_and_reopen_treant_artifacts(output_dir, original, success.mesh())?;
    }
    Ok(())
}

fn write_and_reopen_treant_artifacts(
    output_dir: &Path,
    original: &Mesh,
    candidate: &Mesh,
) -> Result<(), String> {
    let before = output_dir.join("Weeping Treant v1.2.before.stl");
    let after = output_dir.join("Weeping Treant v1.2.after.stl");
    write_binary_stl(&before, original).map_err(|error| {
        format!(
            "could not write Treant before artifact {}: {error}",
            before.display()
        )
    })?;
    write_binary_stl(&after, candidate).map_err(|error| {
        format!(
            "could not write Treant after artifact {}: {error}",
            after.display()
        )
    })?;
    let reread_before = load_binary_stl(&before, WeldPolicy::None).map_err(|error| {
        format!(
            "could not reopen Treant before artifact {}: {error}",
            before.display()
        )
    })?;
    let reread_after = load_binary_stl(&after, WeldPolicy::None).map_err(|error| {
        format!(
            "could not reopen Treant after artifact {}: {error}",
            after.display()
        )
    })?;
    reread_before.audit_strict().map_err(|error| {
        format!("Treant before artifact strict audit failed without repair: {error}")
    })?;
    reread_after.audit_strict().map_err(|error| {
        format!("Treant after artifact strict audit failed without repair: {error}")
    })?;
    assert_stl_surface_bits_equal(original, &reread_before, "Treant before artifact")?;
    assert_stl_surface_bits_equal(candidate, &reread_after, "Treant after artifact")?;
    Ok(())
}

fn assert_stl_surface_bits_equal(
    expected: &Mesh,
    actual: &Mesh,
    label: &str,
) -> Result<(), String> {
    if expected.tris().len() != actual.tris().len() {
        return Err(format!("{label} changed triangle count after disk reopen"));
    }
    for (triangle, (left_indices, right_indices)) in
        expected.tris().iter().zip(actual.tris()).enumerate()
    {
        for corner in 0..3 {
            let left = expected.verts()[left_indices[corner] as usize];
            let right = actual.verts()[right_indices[corner] as usize];
            for axis in 0..3 {
                if left[axis].to_bits() != right[axis].to_bits() {
                    return Err(format!(
                        "{label} changed triangle {triangle} corner {corner} axis {axis} after disk reopen"
                    ));
                }
            }
        }
    }
    if actual.verts().len() != actual.tris().len() * 3 {
        return Err(format!(
            "{label} reload did not retain binary-STL triangle-soup representation"
        ));
    }
    Ok(())
}

fn expected_status_name(status: ExpectedStatus) -> &'static str {
    match status {
        ExpectedStatus::Verified => "verified",
        ExpectedStatus::AlreadyPrintScale => "already_print_scale",
        ExpectedStatus::SkippedScaleSuspicious => "skipped_scale_suspicious",
    }
}

fn ratio_text(value: f64) -> String {
    format!("{value:.17}")
}

/// `DECIMATION_MAX_ABSOLUTE_DEVIATION_MM`, read from the one M0 registry rather
/// than restated here, so the report's stated U3 bound cannot drift from the
/// bound the witness actually applies.
fn registered_max_absolute_deviation_mm() -> f64 {
    *plinth_core::m0_constants()
        .expect("M0 constants registry must construct")
        .evidence()
        .decimation_max_absolute_deviation_mm()
        .value()
}

fn write_and_reopen_report(path: &Path, report: &GateReport) -> Result<(), String> {
    // Report floats are fixed-decimal strings so their printed evidence is
    // stable across JSON parser implementations. Canonicalize the typed report
    // before the only disk write, then require byte-identical re-encoding.
    let provisional = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("could not encode provisional typed report: {error}"))?;
    let canonical: GateReport = serde_json::from_slice(&provisional)
        .map_err(|error| format!("could not canonicalize typed report numbers: {error}"))?;
    let bytes = serde_json::to_vec_pretty(&canonical)
        .map_err(|error| format!("could not encode deterministic report: {error}"))?;
    fs::write(path, &bytes)
        .map_err(|error| format!("could not write report {}: {error}", path.display()))?;
    let reread = fs::read(path)
        .map_err(|error| format!("could not reopen report {}: {error}", path.display()))?;
    let parsed: GateReport = serde_json::from_slice(&reread)
        .map_err(|error| format!("report {} is not valid typed JSON: {error}", path.display()))?;
    let recoded = serde_json::to_vec_pretty(&parsed)
        .map_err(|error| format!("could not re-encode reopened report: {error}"))?;
    if bytes != recoded {
        return Err(format!(
            "report {} did not round-trip to identical deterministic bytes",
            path.display()
        ));
    }
    Ok(())
}

fn output_directory(root: &Path) -> PathBuf {
    match std::env::var_os("PLINTH_DECIMATE_CORPUS_OUTPUT_DIR") {
        Some(path) => PathBuf::from(path),
        None => root.join(DEFAULT_OUTPUT_DIR),
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("plinth-decimate must remain in the workspace crates directory")
        .to_path_buf()
}
