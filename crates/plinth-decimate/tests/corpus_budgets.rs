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
use plinth_mesh::{load_binary_stl, write_binary_stl, EdgeTopologyCounts, Mesh, WeldPolicy};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const REPORT_SCHEMA: &str = "plinth/m3-decimation-corpus-budget-report/v2";
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
    search_p99_mm: Option<String>,
    search_p99_9_mm: Option<String>,
    final_p99_mm: Option<String>,
    final_p99_9_mm: Option<String>,
    final_p99_limit_mm: Option<String>,
    final_p99_9_limit_mm: Option<String>,
    topology_before: Option<TopologyReport>,
    topology_after: Option<TopologyReport>,
    final_attempts: Vec<FinalAttemptReport>,
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
    passed: bool,
}

#[derive(Serialize, Deserialize)]
struct FinalAttemptReport {
    requested_ratio: String,
    achieved_ratio: String,
    output_faces: usize,
    candidate_fingerprint: String,
    p99_mm: String,
    p99_9_mm: String,
    topology_after: TopologyReport,
    result: String,
    downstream_utility: String,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
struct TopologyReport {
    boundary_edges: usize,
    nonmanifold_edges: usize,
}

impl From<EdgeTopologyCounts> for TopologyReport {
    fn from(counts: EdgeTopologyCounts) -> Self {
        Self {
            boundary_edges: counts.boundary_edges,
            nonmanifold_edges: counts.nonmanifold_edges,
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
                passed: true,
            }],
            search_p99_mm: Some(ratio_text(0.0)),
            search_p99_9_mm: Some(ratio_text(0.0)),
            final_p99_mm: Some(ratio_text(0.0)),
            final_p99_9_mm: Some(ratio_text(0.0)),
            final_p99_limit_mm: Some(ratio_text(0.0095 * 1.1)),
            final_p99_9_limit_mm: Some(ratio_text(0.019 * 1.1)),
            topology_before: Some(TopologyReport {
                boundary_edges: 0,
                nonmanifold_edges: 0,
            }),
            topology_after: Some(TopologyReport {
                boundary_edges: 0,
                nonmanifold_edges: 0,
            }),
            final_attempts: Vec::new(),
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

    let budget = DecimationBudget::ratified_default(ProbeConfig {
        surface: NativeSurfaceConfig {
            grid_pitch_mm: 1.0,
            max_grid_bytes: usize::MAX,
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
            search_p99_mm: None,
            search_p99_9_mm: None,
            final_p99_mm: None,
            final_p99_9_mm: None,
            final_p99_limit_mm: None,
            final_p99_9_limit_mm: None,
            topology_before: None,
            topology_after: None,
            final_attempts: Vec::new(),
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
            search_p99_mm: None,
            search_p99_9_mm: None,
            final_p99_mm: None,
            final_p99_9_mm: None,
            final_p99_limit_mm: None,
            final_p99_9_limit_mm: None,
            topology_before: None,
            topology_after: None,
            final_attempts: Vec::new(),
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
                passed: step.passed,
            })
            .collect(),
        search_p99_mm: trace.steps.last().map(|step| ratio_text(step.probe.p99_mm)),
        search_p99_9_mm: trace
            .steps
            .last()
            .map(|step| ratio_text(step.probe.p99_9_mm)),
        final_p99_mm: Some(ratio_text(final_probe.p99_mm)),
        final_p99_9_mm: Some(ratio_text(final_probe.p99_9_mm)),
        final_p99_limit_mm: Some(ratio_text(trace.budget.p99_mm * 1.1)),
        final_p99_9_limit_mm: Some(ratio_text(trace.budget.p99_9_mm * 1.1)),
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
                p99_mm: ratio_text(attempt.probe.p99_mm),
                p99_9_mm: ratio_text(attempt.probe.p99_9_mm),
                topology_after: attempt.topology_after.into(),
                result: format!("{:?}", attempt.result),
                downstream_utility: format!("{:?}", attempt.downstream_utility),
            })
            .collect(),
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
        return Err(
            "AlreadyPrintScaleUtility: unregistered downstream evidence authorized reduction"
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
    success
        .mesh()
        .audit_strict()
        .map_err(|error| error.to_string())?;
    if expected.name == "Weeping Treant v1.2" {
        write_and_reopen_treant_artifacts(output_dir, original, success.mesh())?;
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
    if trace.steps.len() != 6 {
        return Err(format!(
            "M3SearchStepCount: expected identity plus five bisections, found {}",
            trace.steps.len()
        ));
    }
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
