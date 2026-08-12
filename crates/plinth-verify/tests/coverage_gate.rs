use std::{path::PathBuf, process::Command};

use plinth_core::{ArtifactId, LayerPlan, PositiveFinite};
use plinth_decimate::{run as run_decimation, DecimationBudget, ProbeConfig};
use plinth_hollow::{HollowedModel, SolidDecisionReason};
use plinth_mesh::make_toed_slab;
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
// **Re-pinned off `make_overhang_table` onto `make_toed_slab` (FU-56,
// 2026-08-12).** `run_support` is now the coverage-closure controller: it
// measures ROUTED coverage at the REGISTERED gate radius
// (`COVERAGE_RADIUS_FACTOR × TARGET_SPACING_MM` = 6.0 mm) and only returns a
// sealed authority once every island closes or the registered 8-attempt bound
// is exhausted. `make_overhang_table` has a genuinely unclosable island at
// that radius (`plinth-support/tests/support_runner_integration.rs`,
// `public_runner_refuses_overhang_table_with_a_genuinely_unclosable_island`),
// so `run_support` on it now refuses before this file's OWN, independently
// chosen small-radius probe (1.0 mm) could ever run — there is no sealed
// authority left to hand `verify_coverage`. `make_toed_slab` closes at the
// registered radius on its first attempt (zero repair rounds; its own
// `run_support` digest is unchanged, 2_198_459_883_570_957_390) while STILL
// carrying real blocking patches at this test's small radius (9 of 9 islands
// have positive uncovered area there), so it is the fixture that can still
// exercise this refused-report path. The golden below was regenerated through
// THIS FILE'S OWN PRODUCER PATH (the `THREAD_CHILD` arm of
// `public_coverage_report_is_frozen_without_evidence_loss`), never
// hand-edited, exactly as every prior refresh recorded above was.
const FROZEN_REPORT_JSON: &str = include_str!("coverage_gate_toed_slab.json");

fn verify_json(max_distance_work: usize) -> Result<String, CoverageVerificationError> {
    let mesh = make_toed_slab().expect("procedural toed slab");
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
                    .join("../../corpus/baseline/make_toed_slab.pipeline.json"),
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
        ArtifactId::from("coverage-gate-toed-slab"),
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
        CoverageMeasurementLimits::new(71, 69, 50_688, max_distance_work, 12_624_152, 5_879_808)
            .expect("nonzero frozen limits"),
    )
    .expect("finite request")
}

fn report_json() -> String {
    verify_json(1_033_728).expect("coverage report")
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
        verify_json(1_033_727),
        Err(CoverageVerificationError::ResourceLimit {
            resource: "distance work".to_owned()
        })
    );
}

/// FU-56 guard (design test #12): `plinth_support::coverage_patch_is_blocking`
/// must never independently drift from `defects_for_patches`'s own per-patch
/// blocking predicate — that predicate is private to this crate, so this
/// walks the SAME frozen report's own serialized evidence (already proven
/// byte-identical above) and re-derives the per-island defect set from it
/// with the shared predicate, entirely through this crate's public
/// `Serialize` output rather than by importing anything private. Agreement
/// here on all 9 real islands, spanning finite positive area with and without
/// `assessment_failed`, is the additive drift alarm: if the two predicates
/// ever disagreed, this test would fail without either crate's gate logic
/// having to change.
#[test]
fn shared_blocking_predicate_reproduces_the_frozen_reports_own_defect_set() {
    let parsed: serde_json::Value =
        serde_json::from_str(FROZEN_REPORT_JSON.trim_end()).expect("frozen report parses");
    let patches = parsed["evidence"]["patches"]
        .as_array()
        .expect("frozen report has a patches array");
    assert_eq!(patches.len(), 9, "frozen fixture has nine real patches");

    let mut expected_islands: Vec<u32> = Vec::new();
    let mut expected_measure = std::collections::BTreeMap::<u32, f64>::new();
    for patch in patches {
        let island_id = patch["island_id"].as_u64().unwrap() as u32;
        let area = patch["uncovered_area_mm2"].as_f64().unwrap();
        let cut_ambiguous = patch["cut_ambiguous"].as_bool().unwrap();
        let assessment_failed = patch["assessment_failed"].as_bool().unwrap();
        if plinth_support::coverage_patch_is_blocking(area, cut_ambiguous, assessment_failed) {
            expected_islands.push(island_id);
        }
        // `defects_for_patches` sums area across every patch of an island
        // regardless of whether that one patch is individually blocking, so
        // the reconstruction mirrors that rather than only summing blocking
        // patches.
        *expected_measure.entry(island_id).or_insert(0.0) += area;
    }
    expected_islands.sort_unstable();
    expected_islands.dedup();

    let defects = parsed["defects"].as_array().expect("frozen report has defects");
    let mut actual_islands: Vec<u32> = defects
        .iter()
        .map(|defect| defect["site"]["id"].as_u64().unwrap() as u32)
        .collect();
    actual_islands.sort_unstable();
    assert_eq!(
        actual_islands, expected_islands,
        "the shared predicate must name exactly the islands the frozen report's own defects name"
    );
    for defect in defects {
        let island_id = defect["site"]["id"].as_u64().unwrap() as u32;
        let measure = defect["measure"].as_f64().unwrap();
        assert_eq!(
            measure, expected_measure[&island_id],
            "island {island_id}'s defect measure must match the shared predicate's area sum"
        );
    }
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
