//! **SPEC-A4 §A–§E, proved on real geometry: the one-winner transactional
//! coverage-repair controller.**
//!
//! Every fixture here is procedural — built in code from
//! `plinth_mesh::make_*` against a checked-in `corpus/baseline/*.pipeline.json`
//! — so this file runs on a clean worktree with no `corpus/models/` present
//! (task #16's known hole). Nothing below reads a mini.
//!
//! What each test is FOR, so a reader knows what breaks if it fails:
//!
//! | test | the claim it holds |
//! | --- | --- |
//! | `toed_slab…zero_trials…` | design §E.5 case 6 and §G.3: a model that closes at attempt 0 never enters a transaction, so its digest CANNOT move under this change. The regression pin for the whole controller. |
//! | `overhang_table…three_clauses…` | design §G.2's strengthened assertion: every committed step proved all three accept clauses, and the proof is re-run through the production predicate rather than restated here. |
//! | `overhang_table…attribution…` | design §D.1: one step, one row, one delta. The batch loop could not produce this at all. |
//! | `raking_blade…measured_class…` | design §E.1 and rulings R7/R14: the refusal names a MEASURED defect class per route, never a distance claim, and it names the same island at the same area the batch controller does. |
//! | `every_run_reports…` | design §A.8 and ruling R10: no failed-tries cap, so the trial count and the wall time are LOUD on every path. |

use std::path::PathBuf;

use plinth_core::{LayerPlan, PositiveFinite};
use plinth_decimate::{run as run_decimation, DecimationBudget, ProbeConfig};
use plinth_hollow::{HollowedModel, SolidDecisionReason};
use plinth_mesh::{
    evaluate_delicacy, make_overhang_table, make_raking_blade, make_toed_slab, DelicacyBuildConfig,
    DelicacySettings, Mesh,
};
use plinth_orient::{run as run_orientation, OrientRequest, PinnedBaseline};
use plinth_support::{
    repair_step_outcome, run_support_transactional, BlockedCause, CoverageClosureReason,
    FinalSupportAuthority, RepairGateVector, RepairStepOutcome, RouteRepairOutcome,
    SupportPreparationInput, SupportPreparationLimits, SupportRequest, SupportRunError,
};

fn hollowed(mesh: Mesh, baseline: &str) -> HollowedModel {
    let budget =
        DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
            .expect("ratified decimation budget");
    let decimated = run_decimation(&mesh, budget)
        .expect("decimate procedural fixture")
        .into_decimated_model()
        .expect("print-scale procedural fixture");
    let oriented = run_orientation(
        decimated,
        &OrientRequest {
            pinned: PinnedBaseline {
                pipeline_report: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../corpus/baseline")
                    .join(format!("{baseline}.pipeline.json")),
            },
        },
    )
    .expect("orient from checked-in corpus baseline");
    let (mesh, orientation) = oriented.into_parts();
    HollowedModel::solid_decision(mesh, orientation, SolidDecisionReason::BelowThreshold)
        .expect("solid decision handoff")
}

/// The real producer-side delicacy field, exactly as
/// `plinth_pipeline::prepare::prepared_delicacy_field` computes it. Feeding
/// zeros instead is not a neutral default — see the same helper's doc comment
/// in `support_runner_integration.rs`.
fn real_delicacy(hollowed: &HollowedModel) -> Vec<f64> {
    let field = evaluate_delicacy(
        hollowed.mesh(),
        "prepared",
        DelicacySettings::production().expect("production delicacy settings"),
        DelicacyBuildConfig::production(hollowed.mesh().tris().len()),
    )
    .expect("delicacy field");
    field
        .face_values()
        .iter()
        .map(|value| value.weight())
        .collect()
}

fn support_input<'a>(
    delicacy_by_face: &'a [f64],
    layers: &'a LayerPlan,
) -> SupportPreparationInput<'a> {
    SupportPreparationLimits::production()
        .expect("production support preparation limits")
        .input(layers, delicacy_by_face, &[])
}

fn layer_plan() -> LayerPlan {
    LayerPlan::new(0.0, vec![0.05; 1_000]).expect("layer plan")
}

fn run(mesh: Mesh, baseline: &str) -> Result<FinalSupportAuthority<'static>, SupportRunError> {
    // The delicacy field and the layer plan must outlive the authority's
    // borrow, so both are leaked for the duration of the test process. This is
    // a test harness, not a production path.
    let model = hollowed(mesh, baseline);
    let delicacy: &'static [f64] = Box::leak(real_delicacy(&model).into_boxed_slice());
    let layers: &'static LayerPlan = Box::leak(Box::new(layer_plan()));
    run_support_transactional(
        model,
        SupportRequest {
            lift_mm: PositiveFinite::new(5.0).expect("positive lift"),
        },
        support_input(delicacy, layers),
    )
}

/// **Design §E.5 case 6, and §G.3's whole blast-radius claim, on the model that
/// proves it: `make_toed_slab` seals with ZERO trials and its frozen digest
/// does not move.**
///
/// The claim under test is not "the toed slab still works". It is that a model
/// whose baseline layout already covers **never enters a transaction at all**,
/// so no rule in §A, §B, §C, §D or §E can reach it and its digest is
/// bit-identical by construction rather than by luck. If a future change makes
/// the controller touch a sealing model's plan, this is where it shows up —
/// the digest is the same
/// `4_190_462_360_230_124_963` the batch controller produces, taken through the
/// same producer path, and the counts are the same 64 routed identities and 170
/// final live members.
#[test]
fn toed_slab_seals_with_zero_trials_and_the_frozen_digest_does_not_move() {
    let authority = run(
        make_toed_slab().expect("procedural toed slab"),
        "make_toed_slab",
    )
    .expect("make_toed_slab closes on its baseline layout");

    assert_eq!(
        authority.coverage_repair_rounds(),
        0,
        "the toed slab covers on its baseline layout, so the controller must cost it nothing"
    );
    let report = authority
        .repair_report()
        .expect("the one-winner controller reports on every path");
    assert_eq!(
        (report.trials, report.commits),
        (0, 0),
        "a model that seals at attempt 0 must not spend a single transaction"
    );
    assert!(
        authority.repair_steps().is_empty(),
        "zero trials means zero rows in the trial stream"
    );

    let snapshot = authority
        .verification_snapshot()
        .expect("borrow sealed verification snapshot");
    snapshot.validate_integrity().expect("snapshot integrity");
    let identities = snapshot
        .routed_contacts()
        .map(|contact| contact.physical_contact_id())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        (
            identities.len(),
            snapshot.final_live_members().count(),
            snapshot.digest(),
        ),
        (64_usize, 170_usize, 4_190_462_360_230_124_963_u64),
        "the toed slab's sealed digest is the batch controller's, unmoved: a model that never \
         transacts cannot have its plan changed by a controller that only changes plans through \
         transactions"
    );
}

/// **Design §G.2's strengthened overhang-table assertion.** The batch
/// controller's version of this test asserts that the blocking count is
/// non-increasing and falls at least once, which is a PROXY for "a repair
/// worked". Under one-winner transactions the property is a guarantee by
/// construction, and it is asserted directly:
///
/// * every committed step strictly improved the censored coverage objective
///   (clause 1);
/// * no committed step worsened any `NoncoverageVector` component (clause 2);
/// * every committed step strictly reduced total UNCENSORED uncovered sample
///   area (clause 3, the anti-gaming guard).
///
/// The proof is re-run through the production predicate `repair_step_outcome`
/// rather than restated here. A test that re-implements the rule it is checking
/// proves only that two copies of the same mistake agree.
#[test]
fn every_committed_step_on_the_overhang_table_proved_all_three_accept_clauses() {
    let authority = run(
        make_overhang_table().expect("procedural overhang table"),
        "make_overhang_table",
    )
    .expect("make_overhang_table closes through repair");

    let report = authority.repair_report().expect("a §A.8 report");
    assert!(
        report.commits >= 1,
        "this fixture is here BECAUSE it needs repair; if it stopped needing any, the test \
         has stopped testing the repair path: {report:?}"
    );
    assert!(
        report.trials >= report.commits,
        "every commit is a trial: {report:?}"
    );

    let committed: Vec<_> = authority
        .repair_steps()
        .iter()
        .filter(|step| step.commit.is_some())
        .collect();
    assert_eq!(
        committed.len(),
        report.commits,
        "the trial stream and the report must agree about how many steps committed"
    );

    for step in &committed {
        let after = RepairGateVector {
            coverage: step
                .objective_after
                .expect("a committed step was measured on both sides"),
            noncoverage: step.gates_after.expect("a committed step has after-gates"),
            uncensored_uncovered_mm2: step
                .uncensored_after_mm2
                .expect("a committed step has an after-area"),
        };
        let before = RepairGateVector {
            coverage: step.objective_before,
            noncoverage: step.gates_before,
            uncensored_uncovered_mm2: step.uncensored_before_mm2,
        };
        assert_eq!(
            repair_step_outcome(&after, &before),
            RepairStepOutcome::Committed,
            "trial {} committed, so re-running the accept test on its own recorded before/after \
             must still say Committed — all three clauses, through the production predicate",
            step.trial
        );
        // And the same three clauses, spelled out, so a failure says WHICH one.
        assert!(
            after.uncensored_uncovered_mm2 < before.uncensored_uncovered_mm2,
            "clause 3: trial {} committed without covering a single sample ({} -> {} mm²)",
            step.trial,
            before.uncensored_uncovered_mm2,
            after.uncensored_uncovered_mm2
        );
        assert!(
            after
                .noncoverage
                .first_regression(&before.noncoverage)
                .is_none(),
            "clause 2: trial {} committed while worsening a noncoverage count",
            step.trial
        );
    }

    // The censored objective is monotone across the whole accepted chain, not
    // just per step: the accepted state only ever moves forward.
    for pair in committed.windows(2) {
        let earlier = pair[0].objective_after.expect("measured");
        let later = pair[1].objective_before;
        assert_eq!(
            earlier, later,
            "each committed step's before-state must be the previous commit's after-state; if \
             these differ, something moved the accepted state outside a transaction"
        );
    }
}

/// **Design §D.1 — one step, one row, one delta.** Because a step changes the
/// plan by exactly one contact, the difference between its before- and
/// after-measurement IS that contact's contribution. There is no inference and
/// no apportionment, and the record says which contact, where it sits, and
/// which routes' uncovered samples it was credited for.
///
/// The batch controller cannot produce any of this: an attempt that appends 266
/// rows and moves the area by 0.4 mm² cannot say which row did it.
#[test]
fn a_committed_step_names_the_row_that_closed_the_hole_and_the_routes_it_covered() {
    let authority = run(
        make_overhang_table().expect("procedural overhang table"),
        "make_overhang_table",
    )
    .expect("make_overhang_table closes through repair");

    let committing = authority
        .repair_steps()
        .iter()
        .find(|step| step.commit.is_some())
        .expect("at least one step committed");

    assert!(
        committing.marginal_area_mm2 > 0.0,
        "a committed row must have been scored on real uncovered area; the zero-score guard \
         (§B.3) exists so a zero-scoring row never even gets a trial"
    );
    assert!(
        !committing.scored_from.is_empty(),
        "§D.1: the record must name the routes whose uncovered samples produced the score, \
         because the row's own route is often not the largest contributor"
    );
    let credited: f64 = committing
        .scored_from
        .iter()
        .map(|share| share.area_mm2)
        .sum();
    assert!(
        (credited - committing.marginal_area_mm2).abs() < 1e-9,
        "the per-route shares must add up to the global score, or the attribution is a story: \
         {credited} against {}",
        committing.marginal_area_mm2
    );
    assert!(
        !committing.route_deltas.is_empty(),
        "a committed step moved the measurement, so at least one route's evidence must have moved"
    );
    // Ownerless faces travel as EVIDENCE and decide nothing (§C.1). What is
    // asserted is only that they are recorded on both sides.
    assert!(
        committing.ownerless_faces_after.is_some(),
        "task #49's ownerless-face count must be recorded on both sides of a measured step"
    );
}

/// **Design §E.1 and rulings R7 + R14: the refusal names a MEASURED defect
/// class per route, never a distance claim.**
///
/// `make_raking_blade` refuses under both controllers, and it must refuse for
/// the same physical reason: island 5, one patch, 5.135 mm² — 32× the
/// registered A3 censor — with no routed contact reaching it. What changes is
/// the vocabulary: the batch controller says `AttemptsExhausted`, which is a
/// counter's name for "I gave up"; this one says which measured class stopped
/// the route.
///
/// The design's v1 assigned the Warhorse's blocked islands "no row within
/// radius", inferred from a worst gap against the coverage radius. The island
/// diagnosis showed that inference was false — 220 of 300 pool rows sit INSIDE
/// the radius, nearest seat 0.0972 mm from the hole. This test pins that no
/// route outcome is a distance claim.
#[test]
fn the_raking_blade_refuses_with_a_measured_class_per_route() {
    let error = run(
        make_raking_blade().expect("procedural raking blade"),
        "make_raking_blade",
    )
    .err()
    .expect("make_raking_blade has a 5.135 mm² island the frozen repair pool cannot close");

    let SupportRunError::CoverageClosure(refusal) = error else {
        panic!("expected a coverage-closure refusal, got a different SupportRunError");
    };

    // Ruling R6: the field named for the registered bound now carries what that
    // bound bounds — COMMITTED steps.
    assert!(
        refusal.attempts <= 8,
        "the registered bound counts committed steps and its value is unmoved at 8: {}",
        refusal.attempts
    );
    assert!(
        matches!(
            refusal.reason,
            CoverageClosureReason::NoImprovingRepair { .. }
                | CoverageClosureReason::NothingToTry
                | CoverageClosureReason::CommitBudgetExhausted { .. }
        ),
        "the stop reason must be one of §B.7's measured ends, not a retired counter name: {:?}",
        refusal.reason
    );

    let censor = plinth_support::registered_coverage_censor_area_mm2()
        .expect("registered A3 coverage censor");
    let mut islands = refusal.islands.clone();
    islands.sort_by_key(|island| island.island_id);
    assert_eq!(
        islands
            .iter()
            .map(|island| (island.island_id, island.component_id))
            .collect::<Vec<_>>(),
        vec![(5, 1)],
        "only the island genuinely above the censor may refuse this model — island 3's \
         0.060 mm² is censored A3 uncertainty and must stay out of the refusal"
    );
    assert!(
        islands[0].uncovered_area_mm2 > censor,
        "the refusing island must be above the censor: {} mm² against {censor} mm²",
        islands[0].uncovered_area_mm2
    );
    assert!(
        (islands[0].uncovered_area_mm2 - 5.135).abs() < 1e-9,
        "island 5 measures 5.135 mm²; the one-winner controller must refuse the SAME physical \
         hole the batch controller refuses: {} mm²",
        islands[0].uncovered_area_mm2
    );

    let outcome = islands[0]
        .outcome
        .clone()
        .expect("§E.1: every still-uncovered route reports its own measured outcome");
    match &outcome {
        RouteRepairOutcome::Blocked(cause) => match cause {
            // Each of these IS a counter the run kept. `NoCandidateCoversAnything`
            // carries a distance, and that is allowed and deliberate: the class
            // is decided by the SCORE being 0.00 mm², and the distance rides
            // along as context. A distance in a measured class is not a
            // distance claim.
            BlockedCause::PoolExhausted { .. }
            | BlockedCause::NoCandidateCoversAnything { .. }
            | BlockedCause::NoPoolRows
            | BlockedCause::ClearanceMaskUnsatisfiable { .. }
            | BlockedCause::StubReentry { .. } => {}
        },
        RouteRepairOutcome::TriedNoImprovement { rows_tried, .. } => {
            assert!(
                *rows_tried > 0,
                "'tried and none improved' must have actually tried something"
            );
        }
        RouteRepairOutcome::NotReached { .. } => panic!(
            "under ruling R10 there is no failed-tries cap, so this route cannot end the run \
             un-reached unless the commit bound stopped the search first: {outcome:?}"
        ),
    }

    let report = refusal
        .repair
        .as_ref()
        .expect("§A.8: the refused path reports too");
    assert!(
        report.trials > 0,
        "the controller must have actually transacted before refusing: {report:?}"
    );
    assert_eq!(
        report.trials - report.commits,
        report.rejections_by_class.values().sum::<usize>(),
        "every trial that did not commit must appear in the rejection histogram, by class: \
         {report:?}"
    );
}

/// **Design §A.8, ruling R10 — the price of having no failed-tries cap is that
/// the cost is LOUD.**
///
/// R10 removed the cap and put visibility in its place, and that trade only
/// holds if the visibility is real. This asserts the report exists on the
/// SEALED path (the refused path is asserted above), that the wall-time
/// quantiles are ordered, and that the diversity instrument is present — the
/// pair `diversity_demotions` / `diversity_demotions_tried` is what will say
/// whether ruling R13 did any work on a given model, and whether it kept its
/// completeness promise.
///
/// **These counters REPORT; none of them DECIDES.** No branch of the loop reads
/// any of them except `committed` against the registered bound.
#[test]
fn every_run_reports_its_trials_and_its_wall_time_on_the_sealed_path() {
    let authority = run(
        make_overhang_table().expect("procedural overhang table"),
        "make_overhang_table",
    )
    .expect("make_overhang_table closes through repair");
    let report = authority.repair_report().expect("a §A.8 report");

    assert!(report.wall_time_per_trial_min_ms <= report.wall_time_per_trial_median_ms);
    assert!(report.wall_time_per_trial_median_ms <= report.wall_time_per_trial_max_ms);
    assert!(
        report.diversity_demotions_tried <= report.trials,
        "a demoted candidate that was tried anyway is still a trial: {report:?}"
    );
    let line = report.summary_line();
    for field in [
        "trials=",
        "commits=",
        "remaining=",
        "wall_total=",
        "diversity_demotions=",
    ] {
        assert!(
            line.contains(field),
            "the one operator-visible line must carry {field}: {line}"
        );
    }
}
