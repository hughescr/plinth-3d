use std::{path::PathBuf, process::Command};

use plinth_core::{ArtifactId, LayerPlan, PositiveFinite};
use plinth_decimate::{run as run_decimation, DecimationBudget, ProbeConfig};
use plinth_hollow::{HollowedModel, SolidDecisionReason};
use plinth_mesh::make_overhang_table;
use plinth_orient::{run as run_orientation, OrientRequest, PinnedBaseline};
use plinth_support::{
    run_support, CoverageMeasurementLimits, CoverageMeasurementRequest, SupportPreparationLimits,
    SupportRequest,
};
use plinth_verify::{verify_coverage, CoverageVerificationError};

const THREAD_CHILD: &str = "PLINTH_VERIFY_COVERAGE_THREAD_CHILD";
const THREAD_LINE: &str = "PLINTH_VERIFY_COVERAGE_JSON=";
// The golden's `snapshot_digest` was refreshed (8022575652269967193 ->
// 5304257786453935365) after ancestor commit 908d6df widened the
// `verification_snapshot_digest` input set with `final_live_members_digest`
// and the final live member count; every other field of the frozen report was
// verified byte-identical. Any future widening must first flip the pinned
// input-list test `verification_snapshot_digest_input_list_is_pinned` in
// plinth-support's runner tests, forcing a deliberate refresh here.
//
// Refreshed a second time on 2026-08-08, by owner ruling, for the FU-10 phase-C
// per-layer polygon-union cutover: `snapshot_digest` 5304257786453935365 ->
// 5470664714994646368, moving in lockstep with `make_overhang_table`'s OD-8
// census digest (6686599816688114065 -> 1875674282253663076). The two JSON
// documents were diffed field by field before and after the refresh and
// EXACTLY ONE field differs: `snapshot_digest`. Every `request` and `resources`
// counter, all 9 islands, all 9 patches (`uncovered_area_mm2`, `cut_ambiguous`,
// `assessment_failed`, `worst_gap`) and all 9 defects are byte-identical, so no
// evidence was lost. `make_overhang_table` produces byte-identical support
// (73 routed contacts, 261 live members, 5 island births, 4 local minima); its
// digest moved only because the plan digest hashes slice-layer loop vertices
// directly, and 80 of its 686 print layers were requantized by 5 picometres.
// See `docs/engineering-journal/slicer-union-design-2026-08-07.md` sec 8.5 and its
// 2026-08-08 addendum.
//
// Refreshed a third time on 2026-08-09, by owner ruling, for FU-32 phase J2 (the
// state-pure incremental `CcoController::digest`): `snapshot_digest`
// 5470664714994646368 -> 3706765241747593242, moving in lockstep with
// `make_overhang_table`'s OD-8 census digest (1875674282253663076 ->
// 16396858890798947253). Both records above stand unamended. The golden was
// REGENERATED THROUGH THIS FILE'S OWN PRODUCER PATH — the `THREAD_CHILD` arm of
// `public_coverage_report_is_frozen_without_evidence_loss`, whose stdout
// `PLINTH_VERIFY_COVERAGE_JSON=` line was captured verbatim — never hand-edited.
// The two JSON documents were then diffed field by field and EXACTLY ONE field
// differs: `snapshot_digest`. 185 scalar leaves, 1 moved: every `request` bound,
// all 11 `resources` counters, all 9 islands with their face ids, all 9 patches
// and all 9 defects are byte-identical, so no evidence was lost.
// `make_overhang_table` still produces byte-identical support (73 routed
// contacts, unchanged live members); the digest moved because exactly 1 of
// `verification_snapshot_digest`'s 17 inputs (`transcript_digest`) folds the
// moved controller digest. The movement is DECISION-NEUTRAL and was attributed
// before it was re-pinned; the owner ruled: "The fingerprints aren't sacred  if
// things change slightly that's to be expected (because the algorithm wasn't
// right when the fingerprints were generated!)". The pinned input-list test
// `verification_snapshot_digest_input_list_is_pinned` stayed GREEN across this
// movement, which is the independent confirmation that the digest's input SHAPE
// did not widen — only one input's value moved. See
// `docs/engineering-journal/CURRENT-STATUS.md` sections `#fu32-j3-2026-08-09` and
// `#fu32-j4-2026-08-09`.
const FROZEN_REPORT_JSON: &str = include_str!("coverage_gate_overhang_table.json");

fn verify_json(max_distance_work: usize) -> Result<String, CoverageVerificationError> {
    let mesh = make_overhang_table().expect("procedural overhang table");
    let budget =
        DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
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
        SupportPreparationLimits::production()
            .expect("production support preparation limits")
            .input(&layers, &delicacy, &[]),
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
