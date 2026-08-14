use std::{path::PathBuf, process::Command};

use plinth_core::{ArtifactId, LayerPlan, PositiveFinite};
use plinth_decimate::{run as run_decimation, DecimationBudget, ProbeConfig};
use plinth_hollow::{HollowedModel, SolidDecisionReason};
use plinth_mesh::{evaluate_delicacy, make_toed_slab, DelicacyBuildConfig, DelicacySettings};
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
// `run_support` digest was, at that rename, 2_198_459_883_570_957_390 — see
// the FU-38 refresh below, which moved it) while STILL carrying real blocking
// patches at this test's small radius (9 of 9 islands have positive uncovered
// area there), so it is the fixture that can still exercise this
// refused-report path. The golden below was regenerated through THIS FILE'S
// OWN PRODUCER PATH (the `THREAD_CHILD` arm of
// `public_coverage_report_is_frozen_without_evidence_loss`), never
// hand-edited, exactly as every prior refresh recorded above was.
//
// **Refreshed a fifth time on 2026-08-12, by owner ruling, for FU-38** — the
// ruling that moved all four decision-relevant instruments off the all-zeros
// delicacy feed and onto the real producer-side field
// (`docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`
// §5). `snapshot_digest` **2198459883570957390 -> 15556275517430246244**.
// The from->to spans the coverage lane's own rename of this golden
// (`coverage_gate_overhang_table.json` -> `coverage_gate_toed_slab.json`,
// merged in `b9decd5`): the retired overhang-table file is NOT what moved —
// the value above is the toed-slab golden's zero-field digest moving to its
// real-field one, and it is the same number this file's own producer path now
// emits. The golden was REGENERATED THROUGH THAT PRODUCER PATH, never
// hand-edited, exactly as every refresh above was. The two JSON documents were
// then diffed leaf by leaf: **181 scalar leaves, EXACTLY 1 moved**
// (`snapshot_digest`). Every `request` bound, all 11 `resources` counters, all
// 9 islands with their face ids, all 9 patches
// (`uncovered_area_mm2`, `cut_ambiguous`, `assessment_failed`, `worst_gap`)
// and all 9 defects are byte-identical, so **no evidence was lost and no
// blocking verdict changed** — the report still refuses with 9 defects. No cap,
// gate, floor, radius or tolerance was touched: this test's own 1.0 mm probe
// radius and 1.0 mm² patch-area floor are unchanged, and so are the registered
// gate values it deliberately does not use. The from->to septet for the OD-8
// census, and the sibling `run_support` digest pin in
// `plinth-support/tests/support_runner_integration.rs`, are recorded together
// with this one in `docs/engineering-journal/CURRENT-STATUS.md` under the
// FU-38 row and its dated section.
//
// **Refreshed a sixth time on 2026-08-14, by owner ruling, to honour the
// WRITTEN 0.16 mm² A3 coverage censor** (SPEC-A3-A4-COVERAGE.md §4: "A patch
// fails if and only if either: 1. its uncovered area is greater than
// `voxel_pitch_mm**2` … Area at or below `voxel_pitch_mm**2` is **censored A3
// uncertainty**"). `plinth_support::coverage_patch_blocks_verdict` used to block
// on `uncovered_area_mm2 > 0.0` — any dust at all — and now blocks on
// `> censor_area_mm2`, the censor taken off the live measurement request.
//
// **This golden is the change's own witness on a real measurement.** Its
// `request.patch_area_floor_mm2` is 1.0 mm² (this file's own deliberately
// coarse probe, NOT the registered production 0.16 mm²), and two of the toed
// slab's nine patches — islands 5 and 8, each 0.48293112489274953… mm² —
// measure below it. The measurement had ALWAYS stamped
// `assessment_failed = false` on both; only the verdict disagreed. So exactly
// two leaves changed here and both are removals: the `defects` array drops
// from 9 entries to 7, losing island 5 and island 8. NOTHING ELSE MOVED —
// `snapshot_digest` is unchanged at 15556275517430246244, every `request`
// bound, all 11 `resources` counters, all 9 islands with their face ids, and
// all 9 patches (`uncovered_area_mm2`, `cut_ambiguous`, `assessment_failed`,
// `worst_gap`) are byte-identical. No evidence was lost: the two censored
// patches are still present, in full, in `evidence.patches`; they simply no
// longer raise a blocking defect. The report still REFUSES, with 7 defects.
//
// No cap, gate, radius or tolerance was raised. The one authorized limit change
// in this job is this censor, and it moves a comparison onto the number the
// spec already wrote down (registered `VOXEL_PITCH_MM` = 0.4 mm, squared) —
// the code was stricter than the written rule, not the other way round. This
// test's 1.0 mm probe radius and 1.0 mm² probe censor are themselves unchanged.
// The golden was REGENERATED THROUGH THIS FILE'S OWN PRODUCER PATH (the
// `THREAD_CHILD` arm of `public_coverage_report_is_frozen_without_evidence_loss`),
// never hand-edited, exactly as every refresh above was.
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
    // FU-38 (2026-08-12): re-pinned from the all-zeros feed to the real
    // producer-side delicacy field, computed exactly as
    // `plinth_pipeline::prepare::prepared_delicacy_field` computes it in
    // production (same LOD name, registered `DelicacySettings`, scaled
    // `DelicacyBuildConfig`); inlined because `plinth-verify` cannot depend
    // on `plinth-pipeline`. See
    // `docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`
    // §5 and the frozen golden's own from->to comment above.
    let delicacy_field = evaluate_delicacy(
        hollowed.mesh(),
        "prepared",
        DelicacySettings::production().expect("production delicacy settings"),
        DelicacyBuildConfig::production(hollowed.mesh().tris().len()),
    )
    .expect("delicacy field");
    let delicacy = delicacy_field
        .face_values()
        .iter()
        .map(|value| value.weight())
        .collect::<Vec<_>>();
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
    // Nine patches, SEVEN blocking islands: two of the nine patches measure
    // 0.48293 mm² against this test's 1.0 mm² censor and are censored A3
    // uncertainty (SPEC-A3-A4-COVERAGE.md §4). The measurement always stamped
    // `assessment_failed = false` on them; the verdict now agrees.
    assert_eq!(report.defects().len(), 7);
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

/// FU-56 guard (design test #12): the same predicate, on a real measurement.
///
/// **Scope, stated honestly (2026-08-12).** `defects_for_patches` now CALLS
/// `plinth_support::coverage_patch_blocks_verdict` rather than restating it, so
/// this is no longer the thing standing between the two definitions — the
/// per-axis rule is pinned by `defects_for_patches`'s own unit test
/// (`a_patch_blocks_on_each_axis_independently_and_only_blocking_area_is_summed`),
/// which can set the three axes independently. This test's remaining, real
/// value is that it walks a FROZEN REAL MEASUREMENT end to end through this
/// crate's public `Serialize` output and shows the shared predicate names
/// exactly the islands the report's own defects name.
///
/// It cannot do more than that, and the earlier version of this comment
/// claimed it could: all 9 patches in the frozen fixture are blocking on
/// positive area, and every one has `cut_ambiguous = false`, so a predicate
/// that dropped either of the other two terms would still pass here.
#[test]
fn shared_blocking_predicate_reproduces_the_frozen_reports_own_defect_set() {
    let parsed: serde_json::Value =
        serde_json::from_str(FROZEN_REPORT_JSON.trim_end()).expect("frozen report parses");
    let patches = parsed["evidence"]["patches"]
        .as_array()
        .expect("frozen report has a patches array");
    assert_eq!(patches.len(), 9, "frozen fixture has nine real patches");

    // The censor this report's own measurement used, read off the frozen
    // report's request rather than written down here. On this fixture it is
    // this test's deliberately coarse 1.0 mm² probe value, not the registered
    // production 0.16 mm² — which is exactly why the fixture can show the
    // censor doing real work on a real measurement.
    let censor = parsed["request"]["patch_area_floor_mm2"]
        .as_f64()
        .expect("frozen report carries its own censor");

    let mut expected_islands: Vec<u32> = Vec::new();
    let mut expected_measure = std::collections::BTreeMap::<u32, f64>::new();
    let mut censored = 0usize;
    for patch in patches {
        let island_id = patch["island_id"].as_u64().unwrap() as u32;
        let area = patch["uncovered_area_mm2"].as_f64().unwrap();
        let cut_ambiguous = patch["cut_ambiguous"].as_bool().unwrap();
        let assessment_failed = patch["assessment_failed"].as_bool().unwrap();
        // `defects_for_patches` sums area over an island's BLOCKING patches
        // only (a censored or otherwise non-blocking patch contributes
        // nothing), so the reconstruction mirrors that in one pass.
        if plinth_support::coverage_patch_blocks_verdict(
            area,
            cut_ambiguous,
            assessment_failed,
            censor,
        ) {
            expected_islands.push(island_id);
            *expected_measure.entry(island_id).or_insert(0.0) += area;
        } else {
            censored += 1;
            assert!(
                area > 0.0 && area <= censor,
                "island {island_id} is non-blocking for some reason other than the censor"
            );
        }
    }
    // **The censor, witnessed on a real measurement, not a hand-built record.**
    // Two of the toed slab's nine patches measure 0.48293 mm² of uncovered
    // area against this test's 1.0 mm² probe censor. Before the censor was
    // honoured they raised blocking defects on `area > 0.0` even though the
    // measurement had already stamped `assessment_failed = false` on both, and
    // the frozen golden recorded 9 defects. It now records 7.
    assert_eq!(
        censored, 2,
        "the toed slab has exactly two sub-censor patches; if this moves, the fixture moved"
    );
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
