use std::{path::PathBuf, process::Command};

use geom_kernel::{KnnBatchLimits, PenetrationBatchLimits};
use plinth_core::{ArtifactId, LayerPlan, PositiveFinite};
use plinth_decimate::{run as run_decimation, DecimationBudget, ProbeConfig};
use plinth_hollow::{HollowedModel, SolidDecisionReason};
use plinth_mesh::make_overhang_table;
use plinth_orient::{run as run_orientation, OrientRequest, PinnedBaseline};
use plinth_support::{
    dyadic::CandidatePayloadConfig,
    perception::{GenericPerceptionLimits, SpecializedOwnerReservation},
    raft::RaftLimits,
    run_support,
    specialized::SpecializedLimits,
    ContactConstructionLimits, CoverageMeasurementLimits, CoverageMeasurementRequest,
    SupportPreparationInput, SupportRequest,
};
use plinth_verify::{verify_coverage, CoverageVerificationError};

const GRID_BYTES: usize = 64 * 1024 * 1024;
const THREAD_CHILD: &str = "PLINTH_VERIFY_COVERAGE_THREAD_CHILD";
const THREAD_LINE: &str = "PLINTH_VERIFY_COVERAGE_JSON=";
const FROZEN_REPORT_JSON: &str = include_str!("coverage_gate_overhang_table.json");
const TEST_KNN_LIMITS: KnnBatchLimits = KnnBatchLimits {
    max_source_points: 20_000_000,
    max_query_points: 20_000_000,
    max_neighbors: 1,
    max_logical_peak_bytes: 2 * 1024 * 1024 * 1024,
    max_work: 100_000_000,
};

fn verify_json(max_distance_work: usize) -> Result<String, CoverageVerificationError> {
    let mesh = make_overhang_table().expect("procedural overhang table");
    let budget = DecimationBudget::ratified_default(ProbeConfig::new(1.0, usize::MAX, 10_000.0))
        .expect("ratified decimation budget");
    let decimated = run_decimation(&mesh, budget)
        .expect("decimate procedural table")
        .into_decimated_model()
        .expect("print-scale table");
    let oriented = run_orientation(
        decimated,
        &OrientRequest {
            pinned: PinnedBaseline {
                pipeline_report: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../corpus/baseline/make_overhang_table.pipeline.json"),
            },
        },
    )
    .expect("pinned orientation");
    let (mesh, orientation) = oriented.into_parts();
    let hollowed =
        HollowedModel::solid_decision(mesh, orientation, SolidDecisionReason::BelowThreshold)
            .expect("solid decision");
    let delicacy = vec![0.0; hollowed.mesh().tris().len()];
    let layers = LayerPlan::new(0.0, vec![0.05; 1_000]).expect("layer plan");
    let authority = run_support(
        hollowed,
        SupportRequest {
            lift_mm: PositiveFinite::new(5.0).expect("positive lift"),
        },
        SupportPreparationInput {
            layers: &layers,
            delicacy_by_face: &delicacy,
            column_pitch_mm: PositiveFinite::new(1.0).expect("positive pitch"),
            surface_grid_pitch_mm: PositiveFinite::new(1.0).expect("positive grid pitch"),
            max_grid_bytes: GRID_BYTES,
            candidate_config: CandidatePayloadConfig::current(
                8,
                1_000_000,
                3_000_000,
                256 * 1024 * 1024,
                65_536,
            )
            .expect("candidate configuration"),
            reservations: &[] as &[SpecializedOwnerReservation],
            perception_limits: GenericPerceptionLimits::production(TEST_KNN_LIMITS),
            coverage_samples_max_virtual_cells: 2_000_000,
            coverage_score_matrix_max_entries: 65_536,
            specialized_limits: SpecializedLimits::production(TEST_KNN_LIMITS),
            construction_limits: ContactConstructionLimits::new(
                1_000_000,
                10_000_000,
                4_000_000,
                512 * 1024 * 1024,
            )
            .expect("contact limits"),
            brace_penetration_limits: PenetrationBatchLimits {
                max_rows: 100_000,
                max_total_work: 1_000_000_000,
                max_total_hits: 1_000_000_000,
                max_total_samples: 10_000_000_000,
                max_total_intervals: 1_000_000_000,
                max_scratch_bytes: 100_000_000_000,
            },
            raft_limits: RaftLimits::try_new(100_000, 100_000, 200_000, 128 * 1024 * 1024)
                .expect("raft limits"),
            max_primitives: 200_000,
            max_primitive_bytes: 128 * 1024 * 1024,
        },
    )
    .expect("explicit support run");
    let snapshot = authority.verification_snapshot().expect("sealed snapshot");
    let report = verify_coverage(
        ArtifactId::from("coverage-gate-overhang-table"),
        &snapshot,
        request(max_distance_work),
    )?;
    assert!(!report.accepted());
    assert_eq!(report.defects().len(), 9);
    Ok(serde_json::to_string(&report).expect("serialize full coverage report"))
}

fn request(max_distance_work: usize) -> CoverageMeasurementRequest {
    CoverageMeasurementRequest::from_f64(
        1.0,
        1.0,
        CoverageMeasurementLimits::new(81, 77, 50_176, max_distance_work, 12_497_064, 5_820_416)
            .expect("nonzero frozen limits"),
    )
    .expect("finite request")
}

fn report_json() -> String {
    verify_json(1_574_912).expect("coverage report")
}

#[test]
fn public_coverage_report_is_frozen_without_evidence_loss() {
    if std::env::var_os(THREAD_CHILD).is_some() {
        assert_eq!(
            geom_kernel::current_threads().to_string(),
            std::env::var("PLINTH_THREADS").unwrap()
        );
        println!("{THREAD_LINE}{}", report_json());
        return;
    }
    let json = report_json();
    assert_eq!(json, FROZEN_REPORT_JSON.trim_end());
    assert!(json.contains("\"islands\""));
    assert!(json.contains("\"patches\""));
}

#[test]
fn cap_minus_one_returns_a_typed_error_without_a_report() {
    assert_eq!(
        verify_json(1_574_911),
        Err(CoverageVerificationError::ResourceLimit {
            resource: "distance work".to_owned()
        })
    );
}

#[test]
fn complete_coverage_report_json_is_identical_at_one_four_and_sixteen_threads() {
    if std::env::var_os(THREAD_CHILD).is_some() {
        return;
    }
    let executable = std::env::current_exe().expect("coverage gate test executable");
    let outputs = ["1", "4", "16"].map(|threads| {
        let output = Command::new(&executable)
            .args([
                "--exact",
                "public_coverage_report_is_frozen_without_evidence_loss",
                "--nocapture",
            ])
            .env(THREAD_CHILD, "1")
            .env("PLINTH_THREADS", threads)
            .output()
            .expect("coverage thread child starts");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("child output utf8")
            .lines()
            .find(|line| line.starts_with(THREAD_LINE))
            .expect("child coverage json")
            .to_owned()
    });
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0], outputs[2]);
}
