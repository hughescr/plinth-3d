use std::{collections::BTreeSet, path::PathBuf};

use plinth_core::{LayerPlan, PositiveFinite};
use plinth_decimate::{run as run_decimation, DecimationBudget, ProbeConfig};
use plinth_hollow::{HollowedModel, SolidDecisionReason};
use plinth_mesh::{
    evaluate_delicacy, make_overhang_table, make_toed_slab, make_uncovered_rim_table,
    DelicacyBuildConfig, DelicacySettings, Mesh,
};
use plinth_orient::{
    run as run_orientation, BaselineSelection, OrientRequest, OrientationHistory,
    OrientationProvenance, OrientationReport, PinnedBaseline, RigidTransform,
};
use plinth_support::{
    run_support, CoverageGap, CoverageMeasurementLimits, CoverageMeasurementRequest,
    SupportPreparationInput, SupportPreparationLimits, SupportRequest,
};

type FrozenIslandEvidence = (u32, u32, u32, &'static [u32]);
type FrozenPatchEvidence = (u32, u32, u32, &'static [u32], u64, bool, bool, u64);

fn hollowed_corpus_model(mesh: Mesh, baseline: &str) -> HollowedModel {
    let budget =
        DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
            .expect("ratified decimation budget");
    let decimated = run_decimation(&mesh, budget)
        .expect("decimate procedural corpus model")
        .into_decimated_model()
        .expect("print-scale procedural corpus model");
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

/// FU-38 (2026-08-12): the real producer-side delicacy field, computed
/// exactly as `plinth_pipeline::prepare::prepared_delicacy_field` computes it
/// in production -- same LOD name, same registered `DelicacySettings`, same
/// scaled `DelicacyBuildConfig`. Inlined rather than imported because
/// `plinth-support` cannot depend on `plinth-pipeline` (the dependency runs
/// the other way). Every call site below fed `vec![0.0; faces]` until this
/// row; see
/// `docs/architecture.md` and
/// `docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`
/// §5 for why the all-zeros feed was not a neutral default.
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

/// Both models nominate one dyadic cell vertex from two different islands
/// under their own ratified corpus orientation. Before the terminal
/// exactly-coincident regular seat coalescing pass, that co-located pair
/// survived to canonicalization and failed the whole sealed run closed with
/// `DuplicatePhysicalSeat`, so neither model could reach brace, CCO, or
/// rewiring at all.
///
/// **The tuples were re-pinned on 2026-08-05, and the whole movement is one
/// rule.** They were first frozen by the corpus-fixes work, which unblocked
/// these two models BEFORE the contact-physics package existed, so that
/// package's zero-delta attestation never covered them. Post-merge:
/// `make_toed_slab` 65/184 -> 64/179 and `make_raking_blade` 35/93 -> 32/89.
///
/// Every unit of that delta is PR-8 `NotAttached` quarantining FOUR stubs whose
/// model contact has no renderable interface
/// (`docs/physics.md`, PR-8 / OD-4).
/// Measured against the run's 0.05 mm process resolution by the attestation
/// witness (centreline false, 0 of 8 rim rays) and independently by a parity
/// sweep along the contact normal at 0.0025 mm:
///
/// | model | contact | routed load | material behind the seat |
/// | --- | --- | ---: | --- |
/// | toed_slab | 68 | 0.000 mm^2 | none in [-0.01, +0.39] mm; tip 0.265 mm clear |
/// | raking_blade | 7 | 0.060 mm^2 | 0.0275 mm skin; tip 0.152 mm clear beyond it |
/// | raking_blade | 12 | 0.000 mm^2 | 0.0275 mm skin; body starts 0.03 mm past the tip |
/// | raking_blade | 13 | 0.000 mm^2 | tip enters the body 0.025 mm, half a layer |
///
/// The other seven attestation refusals on these models (five OD-9
/// `BuriedSurface` on `toed_slab`, two more `NotAttached` on `raking_blade`)
/// contribute ZERO, because the pre-existing initial-overlap cutoff already
/// condemned those same stubs. PR-3, the model-rung seat witness, PR-9 and the
/// CCO model-anchor family fire exactly zero times on either model. Restoring
/// only those four stubs reproduces the old tuples bit-for-bit, digests
/// included, so nothing else in the package moved these models. The member
/// counts move by more than the four stubs because CCO re-solves its tournament
/// over the smaller contact set.
///
/// **The tuples were re-pinned a second time on 2026-08-08, by owner ruling,
/// for FU-10 phase C** (the per-layer polygon-union cutover). The paragraphs
/// above stand unamended as the record of the 2026-08-05 movement; this one
/// records an independent, later one. Post-cutover: `make_toed_slab`
/// 64/179 -> 64/170 and `make_raking_blade` 32/89 -> 27/72. Every unit of that
/// delta is the removal of **24 phantom island births**, each traced to a named
/// print layer by the census attribution:
///
/// - `make_raking_blade`: 20 births at layers 90..=109 — one per layer, the
///   same rake tooth re-born on every layer once it is fused into the blade
///   body (a real in-plane overlap of 2.5718e-3 mm^2, 0.029886 mm deep, NOT an
///   abutment). Its 8 genuine births at layers 82..=89, while the tooth really
///   was a free-standing solid, survive bit-identically. Cost: 5 routed
///   contacts, 17 live members.
/// - `make_toed_slab`: 4 births at layers 49, 52, 226, 228 — the layer
///   immediately after each of the four runs where a boundary-degenerate parity
///   probe had labelled the solid toe a void. Routed contacts are unchanged at
///   64; cost 9 live members.
///
/// No material is lost anywhere in the corpus, and 87.9727 mm^2 of real toe
/// material — exactly the sum of the seven false hole areas — is restored on
/// `make_toed_slab`. The owner accepted the `make_raking_blade` tooth-body
/// merge on the ground that a 0.029886 mm-deep fusion prints as fused at 0.05 mm
/// layers, so the merged solid is the one that matches physical reality
/// (PR-1(a) precedent: sub-layer features are not resolvable).
///
/// **The digests were re-pinned a third time on 2026-08-09, by owner ruling,
/// for FU-32 phase J2** (the state-pure incremental `CcoController::digest`).
/// Both paragraphs above stand unamended as the record of their own dates.
/// `make_toed_slab` `13_346_779_153_602_682_749 -> 2_198_459_883_570_957_390`
/// and `make_raking_blade` `8_660_653_339_934_460_766 -> 74_030_269_957_898_085`.
/// **The counts did not move and must not**: `make_toed_slab` stays 64/170 and
/// `make_raking_blade` stays 27/72, which is the discriminator separating this
/// movement from a behavioural one. The movement is DECISION-NEUTRAL and was
/// attributed before it was re-pinned, twice over: from source (all 30
/// `controller_digest_before` occurrences audited — zero comparisons outside
/// `#[cfg(test)]`) and by parent-vs-tip routing-output dumps on all seven corpus
/// models (275 transcript rows and 687,288 content lines byte-identical; not one
/// contact, membership, live member, coverage sample or island line differs).
/// The owner ruled: "The fingerprints aren't sacred  if things change slightly
/// that's to be expected (because the algorithm wasn't right when the
/// fingerprints were generated!)".
/// **FU-56 addendum, 2026-08-12: `make_raking_blade` now REFUSES, by a typed
/// coverage-closure refusal, and that is a real finding rather than a
/// regression.** `run_support` is now the coverage-closure controller
/// (FU-56): it measures ROUTED coverage under the registered gate radius
/// (`COVERAGE_RADIUS_FACTOR × TARGET_SPACING_MM` = 6.0 mm) and only ever
/// returns a sealed authority once every island is covered or the registered
/// 8-attempt bound is exhausted. `make_toed_slab` still closes on its very
/// first attempt — zero repair rounds, so its digest is UNCHANGED from every
/// prior re-pin recorded above, which is exactly the "costs nothing on a
/// model that already passes" property FU-56 requires. `make_raking_blade`
/// does not close: islands 3 and 5 each carry a residual patch whose
/// `worst_gap` is `NoRoutedContact` on every one of the 8 attempts — the CCO
/// transcript never admits ANY of the repair-pool candidates the controller
/// nominates for either island, at this radius, from this fixture's pool.
/// This was always true of the fixture; nothing before FU-56 ever measured
/// it, because production `run_support` had no coverage-closure step at all
/// (FU-56 ruling). **Do not respond to this by touching the
/// radius, the floor, or the tolerance** — the FU-56 ruling is explicit that
/// "not one of the 21 [minis] has been shown to be closable ... the first
/// action is what would show it," and this fixture's answer is "not
/// closable from this pool," recorded rather than forced.
///
/// **The digests were re-pinned a fourth time on 2026-08-12, by owner ruling,
/// for FU-38** — the ruling that moves all four decision-relevant instruments
/// off the all-zeros delicacy feed and onto the real producer-side field
/// (`docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`
/// §5). Every paragraph above stands unamended as the record of its own date;
/// the sentence in the FU-56 addendum saying `make_toed_slab`'s digest is
/// "UNCHANGED from every prior re-pin recorded above" was true on its date and
/// is superseded HERE, by this movement, and not by anything FU-56 did.
///
/// - `make_toed_slab`: `2_198_459_883_570_957_390 -> 15_556_275_517_430_246_244`.
///   **The counts did not move and must not**: 64 physical contact identities,
///   170 final live members, 69 distinct seats — the same discriminator the
///   third re-pin used, and it separates an input change that reorders exact-Z
///   ties from a behavioural one. `coverage_repair_rounds()` is still **0**, so
///   the model still closes on its baseline layout and the controller still
///   costs it nothing.
/// - `make_raking_blade`: **the deferred measurement, landed as part of this
///   re-pin rather than ahead of it, exactly as the ruling requires — and it
///   moved NOTHING.** Under the real field the blade still refuses, with a
///   byte-identical typed refusal: `attempts` 8, reason `AttemptsExhausted`,
///   `history.len()` 9, and the same two islands (3 → component 1, face 15;
///   5 → component 1, face 14) each with `worst_gap = NoRoutedContact` and a
///   non-zero `nominated_repair_rows`. There is therefore **no J4 snapshot
///   digest left to re-pin on this fixture**: `74_030_269_957_898_085` was
///   retired by the FU-56 coverage-closure landing (a refusal has no sealed
///   authority and so no digest), not by FU-38, and the row's "measure it, then
///   re-pin" is discharged by the measurement above.
/// - **Item 4 of the ruling — this same geometry's structural asserts and
///   hand-counted coverage resource plan
///   (`public_runner_measures_sealed_coverage_with_its_islands_patches_and_resource_plan`)
///   — was re-derived under the real field and is UNCHANGED**, every number of
///   it: the 11-tuple resource plan, all 9 frozen islands, all 9 frozen patches
///   with their `uncovered_area_mm2`/`cut_ambiguous`/`assessment_failed`/
///   `worst_gap` bits. Only the `snapshot_digest` assertion in that test moved,
///   to the same value pinned here. That is recorded as a measured result, not
///   assumed: re-deriving it was the work, and finding it still true is the
///   finding.
///
/// **Nothing else moved.** No cap, gate, floor, radius or tolerance is touched
/// by this re-pin — the coverage radius is still the registered
/// `COVERAGE_RADIUS_FACTOR × TARGET_SPACING_MM` = 6.0 mm, the patch-area floor
/// is still `VOXEL_PITCH_MM²`, the uncovered-area tolerance is still zero, and
/// the 8-attempt closure bound is still 8. **No digest is claimed wrong:** each
/// value on the left of an arrow above remains exactly reproducible by the
/// harness that produced it, against the zero-field engine that was the engine
/// at the time. The full from→to record, including the OD-8 census septet and
/// the coverage-gate golden, was kept under the FU-38 row's dated section; that
/// record's own document has since been retired (git history only).
///
/// **Amended 2026-08-14, when the written 0.16 mm² A3 censor was honoured**
/// (docs/specs/coverage-repair.md §4; `plinth_support::coverage_patch_blocks_verdict`).
/// The two-island record above is now a record of its own date: the blade's
/// islands are **0.0600 mm² (island 3)** and **5.1350 mm² (island 5)**, and the
/// censor is `voxel_pitch_mm²` = 0.16 mm², so island 3 falls below one voxel
/// footprint and is censored A3 uncertainty. This fixture therefore refuses on
/// ONE island now, not two. Nothing about the blade or the pool changed —
/// `attempts` is still 8, the reason is still `AttemptsExhausted`,
/// `history.len()` is still 9, island 5 is byte-identical, and no digest is
/// affected because a refusal has no sealed authority to digest. What changed
/// is that a patch the measurement had ALWAYS stamped `assessment_failed =
/// false` on stopped being re-failed one function later. **No cap, gate,
/// radius, bound or tolerance was raised**: the comparison moved onto the
/// number the spec already wrote down, and the code had been stricter than the
/// written rule. See the assertion block below, which now pins both sides of
/// the censor on this one model.
///
/// **Re-pinned a fifth time on 2026-08-19, for task #49** — SPEC-A3-A4
/// §6's unrouted-regular removal, which had never been wired: the sole
/// caller of `remove_demand_free_regular` was a test. Every paragraph above
/// stands unamended as the record of its own date.
///
/// - `make_toed_slab`: `15_556_275_517_430_246_244 -> 9_355_852_079_468_421_123`.
///   **The counts did not move and must not**: still 64 physical contact
///   identities, 170 final live members, 69 distinct seats, and
///   `coverage_repair_rounds()` still **0**. The same discriminator the third
///   and fourth re-pins used therefore says the same thing here: the model
///   still closes on its baseline layout, routes the same 64 contacts, and
///   lowers the same 170 members.
/// - **The cause, measured on this fixture.** Of the toed slab's 68 accepted
///   contacts, **three regular contacts never route** (contact 25 quarantined,
///   contacts 32 and 54 with no CCO winner) and one demand-bearing contact is
///   quarantined. Contacts 32 and 54 were each charged **450.000 mm²** of
///   carried area — **900 of the model's 967.689 mm², 93.0 %, was charged to
///   contacts with no support under them.** Removing them repartitions that
///   area onto the survivors, which moves `plan_digest` and `load_digest`,
///   which are two of `verification_snapshot_digest`'s inputs. The digest
///   movement is that repartition and nothing else.
/// - **Only the digest moves.** The routed contact identities, the seat set,
///   the final live members and the closure history are all unchanged, because
///   the snapshot has always projected the transcript's ROUTED rows — the
///   removed contacts were never in the projection. What changes is which plan
///   the projection is taken over.
/// - **The frozen coverage-gate golden moved with it**, to the same value:
///   `crates/plinth-verify/tests/coverage_gate_toed_slab.json`,
///   `snapshot_digest 15556275517430246244 -> 9355852079468421123`,
///   regenerated through that test's own `THREAD_CHILD` producer path and
///   diffed leaf by leaf afterwards — **169 scalar leaves, EXACTLY 1 moved**,
///   no evidence lost, still 9 islands, 9 patches and 7 blocking defects, and
///   all 11 resource counters identical.
/// - **No cap, gate, bound, tolerance or registered constant was touched**, and
///   no new one was added.
///
/// **Ported at SPEC-A4 design §G.1 stage 4, when `run_support` BECAME the
/// one-winner controller — and split in two, which is the one thing about it
/// that changed.**
///
/// The toed-slab half below is UNCHANGED, and unchanged is the finding: this
/// model closes on its baseline layout, so it never enters a transaction, so no
/// rule in §A, §B, §C, §D or §E can reach it. Every number here — 64 routed
/// identities, 170 final live members, digest `12_136_443_226_163_403_608`
/// (`4_190_462_360_230_124_963` before the seal-semantics lane's AUTHORIZED
/// one-time re-freeze of 2026-08-23 — the note on
/// `public_runner_measures_sealed_coverage_with_its_islands_patches_and_resource_plan`
/// records what moved and why the counts did not),
/// `coverage_repair_rounds() == 0`, both abandoned contacts and their ladders —
/// is bit-identical under the new door. That is §G.3's central claim measured on
/// the model it was made about.
///
/// **The `make_raking_blade` half MOVED, it was not dropped.** It now lives in
/// `plinth-support/tests/a4_transactional_controller.rs::the_raking_blade_refuses_with_a_measured_class_per_route`,
/// which already asserted every substantive thing this half asserted, and more:
/// the censor BOTH ways (island 3's 0.060 mm² censored and out of the refusal,
/// island 5's 5.135 mm² above the censor and refusing, both re-checked against
/// the registered censor), the exact island tuple, and — the part this half
/// could not state — WHY the route is still uncovered, as
/// `Blocked(PoolExhausted { pool_rows: 33, occupied: 2, ledgered: 31 })`.
/// That last assertion is the successor to this half's
/// `nominated_repair_rows > 0` ("the evidence must say whether it was ever
/// attempted"), and it is strictly stronger: it says how many rows the route
/// had, how many were spent, and how many were left.
///
/// **Why moved rather than duplicated.** Both files now call the same
/// `run_support`, so keeping both would run the blade's 45 full pipeline trials
/// TWICE per gate for one set of facts. Gate cost is a real cost (task #44) and
/// a duplicated expensive fixture is the wrong place to spend it.
// The `#[expect(clippy::float_cmp)]` that stood here is gone with the exact
// `abandoned_mm2 == 0.0` pin it existed for: this function no longer compares a
// float for equality anywhere. Clippy caught its own staleness under
// `-D unfulfilled-lint-expectations`, which is the lint working as intended.
#[test]
fn public_runner_seals_toed_slab_and_discloses_its_abandoned_load() {
    let hollowed = hollowed_corpus_model(
        make_toed_slab().expect("procedural toed slab"),
        "make_toed_slab",
    );
    // FU-38 (2026-08-12): real delicacy, not the zero field -- see
    // `real_delicacy`'s doc comment for what production this now matches.
    let delicacy = real_delicacy(&hollowed);
    let layers = LayerPlan::new(0.0, vec![0.05; 1_000]).expect("layer plan");
    let authority = run_support(
        hollowed,
        SupportRequest {
            lift_mm: PositiveFinite::new(5.0).expect("positive lift"),
        },
        support_input(&delicacy, &layers),
    )
    .expect("make_toed_slab must close coverage on its first closure attempt");
    assert_eq!(
        authority.coverage_repair_rounds(),
        0,
        "make_toed_slab must cover on its baseline layout: the controller costs nothing on a \
         model that already passes"
    );
    let snapshot = authority
        .verification_snapshot()
        .expect("borrow sealed verification snapshot");
    snapshot.validate_integrity().expect("snapshot integrity");
    // **Physical-contact-identity uniqueness — an INVARIANT, asserted with no
    // literal in it.** Two physical contacts cannot collide on one id on any
    // acceptable plan, whatever placement does. It used to feed the frozen
    // count below it; that count is retired (see the block that follows), so
    // this now stands on its own, which is where it always belonged.
    //
    // Pairwise-distinct SEAT positions -- "no two routed contacts occupy the
    // identical physical point" -- is a different property and is checked
    // placement-agnostically by
    // `toed_slab_contacts_stay_pairwise_distinct_and_under_the_scar_ceiling_regardless_of_the_digest`
    // at the bottom of this file.
    let mut identities = BTreeSet::new();
    for contact in snapshot.routed_contacts() {
        assert!(identities.insert(contact.physical_contact_id()));
    }
    // ===================================================================
    // RETIRED 2026-08-29 — the (contacts, members, digest) triple that stood
    // here, `(64, 170, 13_198_418_135_389_558_006)`.
    //
    // It was a FINGERPRINT, and the doc-comment chain above this function is
    // its own indictment: seven recorded re-derivations, each naming a
    // legitimate change that moved it, several of which moved the counts too
    // (65 -> 64 contacts, 184 -> 170 members, 179 -> 170). A number that has
    // to be rewritten every time the engine legitimately improves is not
    // guarding the engine; it is charging a toll on improving it. Under the
    // adopted doctrine (`docs/development.md`, "Testing doctrine: three kinds
    // of check") a whole-plan digest is an EQUIVALENCE check: run at the time
    // of a specific change and recorded in that change's commit message, never
    // stored as a standing assertion.
    //
    // MEASURED at the retirement, on the legal placement perturbation the
    // doctrine was built for (reversing the terminal tie-break in
    // `initial.rs::routing_order`): of every value this test and its sibling
    // coverage test observe, EXACTLY TWO move — `final_live_members`
    // 170 -> 174 and `digest` 13198418135389558006 -> 9878193565529525115.
    // The routed-contact count, the abandoned set, all eleven coverage
    // resource-plan terms, all nine islands and all nine patches are
    // bit-identical across it. So this triple was the entire reason a legal
    // improvement landed as a red suite, and nothing else here was protecting
    // anything the perturbation could break.
    //
    // What the triple was standing in for now lives in checks that survive a
    // placement change:
    //   * identity uniqueness — asserted directly above, no literal;
    //   * seat pairwise-distinctness and the scar ceiling —
    //     `toed_slab_contacts_stay_pairwise_distinct_and_under_the_scar_ceiling_regardless_of_the_digest`;
    //   * refusal-disclosure consistency —
    //     `abandoned_refusals_disclose_a_consistent_ladder_and_reason_whichever_contacts_are_refused`;
    //   * plan SIZE as a quality number, which is what a member count was
    //     really proxying for — `ratchet_harness.rs` metric 3, support resin
    //     volume, a physical quantity that cannot be improved by deleting
    //     supports until the model falls off the plate.
    // ===================================================================
    //
    // The one thing the counts said that is true of ANY acceptable plan, and
    // is therefore kept: a model that seals must actually be held by
    // something. Stated as a bound, not as the number 64.
    assert!(
        !identities.is_empty(),
        "a sealed plan must route at least one contact"
    );
    assert!(
        snapshot.final_live_members().count() >= identities.len(),
        "every routed contact needs at least one live member reaching it: {} members for {} \
         contacts",
        snapshot.final_live_members().count(),
        identities.len()
    );
    // COUNTS AND DIGEST re-derived 2026-08-20 for the SCAR BOUND, Craig's
    // ruling of the same day: a grow-for-attachment rescue may not exceed
    // `TIP_RADIUS_MAX_GROWN_SCAR_MM` = 0.400 mm RADIUS (0.800 mm DIAMETER),
    // his measured field ceiling — the heaviest support he has ever needed.
    // 65 -> 64 routed contacts, 184 -> 170 final live members,
    // 14_374_106_896_419_012_910 -> 4_190_462_360_230_124_963.
    //
    // **MEASURED, on this fixture, through this producer path.** Exactly one
    // contact moved: contact 64, the single seat this model was growing. Its
    // load law asked for 0.050 mm RADIUS (0.100 mm DIAMETER) and the cure the
    // scan landed on was **0.650630996 mm RADIUS = 1.301261991 mm DIAMETER**
    // — 1.63x Craig's field ceiling, and a scar wider than any support he has
    // ever printed. It is now refused, disclosed as an ABANDONED LOAD naming
    // `TIP_RADIUS_MAX_GROWN_SCAR_MM` as the stopper, and its 14 dependent
    // members go with it.
    //
    // **The load abandoned by that refusal is 0.000 mm².** That is measured,
    // not assumed: this contact carried no routed area at all, so the bound
    // costs this model a 1.3 mm-diameter scar and buys back nothing that was
    // holding the model up. It is the cheapest possible demonstration of the
    // ruling and it is why the counts moving here is the right answer rather
    // than a regression.
    //
    // The resulting digest is bit-identical to the pre-grow-to-attest state
    // (4_190_462_360_230_124_963, recorded below for the 2026-08-19 tip
    // minimum): withdrawing the one over-bound cure returns this fixture
    // exactly to the layout it had before growth existed, which is the
    // strongest available evidence that ONE thing changed and nothing else
    // drifted.
    //
    // Superseded record — digest re-derived 2026-08-20 for the DESCENDING
    // REFINEMENT SCAN on
    // grow-to-attest: 15_045_390_433_036_889_889 ->
    // 14_374_106_896_419_012_910.  The counts are again deliberately
    // unchanged, and the leaf diff through this same producer path is the
    // narrowest one this corpus has produced: of the 184 final live members,
    // ONE changed and 183 are bit-identical.
    //   * slot 61, the model contact this model grows (contact 64), drops its
    //     terminal radius (`r0`) 0.800000000 -> 0.650630996 mm RADIUS
    //     (1.600000000 -> 1.301261991 mm DIAMETER).  That is 0.8 * q^2 with
    //     `q = 1 - 2 pi / 8^2` — two refinement steps below the ladder rung
    //     that used to be emitted — and it cuts the interface area this scar
    //     leaves on the model by 33.9% (0.6614x).
    //   * The ladder record shows why: rungs 0.0707 through 0.5657 all fail,
    //     0.8000 is the first to attest, then the scan measures 0.7215
    //     (attests), 0.6506 (attests) and 0.5868 (FAILS).  The smallest
    //     attesting radius is the one built.
    // Nothing else moved: no rootward radius, no endpoint, no kind, no
    // connectivity, and all 65 routed contact positions were bit-identical AT
    // THAT TIP — the scar bound above has since withdrawn one of those 65.
    // The coverage measurement's own numbers are bit-identical too — see the
    // toed-slab coverage golden, where the ONLY field that changed is
    // `snapshot_digest`.
    //
    // Digest previously re-derived 2026-08-19 for the tip-minimum ruling
    // (`TIP_RADIUS_DEFAULT_MM` 0.2 -> 0.05 mm RADIUS, 0.4 -> 0.1 mm
    // DIAMETER): 9_355_852_079_468_421_123 -> 4_190_462_360_230_124_963.
    // The COUNTS above are deliberately unchanged, and that is the point.
    // Leaf-diffed through this same producer path: of the 170 final live
    // members, 65 changed and 105 are bit-identical.  The 65 are three
    // disjoint groups, 60 + 2 + 3:
    //   * 60 stubs move their terminal radius (`r0`) 0.200 -> 0.050 mm
    //     RADIUS (0.400 -> 0.100 mm DIAMETER).  These carry so little area
    //     that the minimum still binds; it is simply lower now.
    //   * 2 stubs (slots 56, 59) drop `r0` 0.200 -> 0.155284 mm RADIUS
    //     (0.400 -> 0.310568 mm DIAMETER).  The OLD minimum was clamping
    //     these ABOVE their own law-required radius; the law sizes them
    //     now.  This is the band the ruling actually freed.
    //   * 3 members (slots 84, 90, 91) keep every radius bit-identical and
    //     move an ENDPOINT instead.  They share one interior junction,
    //     which shifted 0.272 mm: `rewire.rs::junction_geometry` slides the
    //     junction to minimise frustum volume and `frustum_raw` reads the
    //     child's terminal radius, so thinner tips move that optimum.  The
    //     junction radius is `max(required_radius, terminal_radius)`, so a
    //     thinner terminal can only lower it, and the unconditional
    //     `MIN_SHOOT_R_MM` = 0.35 mm radius (0.7 mm diameter) bounds it
    //     from below regardless.
    // Counting these 3 as unchanged is the mistake to avoid: their radii
    // are identical but their lines are not.  No rootward radius (`r1`)
    // moved anywhere, all 64 routed contact positions are bit-identical,
    // and member count, kinds and connectivity are unchanged.
    //
    // **THE ABANDONED LOAD, ASSERTED — not left in prose.**
    //
    // Accepting 65 -> 64 routed contacts turns entirely on the claim above that
    // the contact the scar bound withdraws carries **0.000 mm²**: the bound
    // costs this model a 1.3 mm-diameter scar and buys back nothing that was
    // holding it up. A comment cannot fail, so the claim is measured here. If a
    // future change ever makes the bound drop a contact that IS carrying load,
    // this fixture must go red rather than keep quoting a number nobody checks.
    // RECLASSIFIED 2026-08-29 to a RATCHET. The number is worth keeping and
    // the equality was the wrong shape for it: 0.0 is the value this fixture
    // happens to have today, not a law, and a legal placement change that
    // moved it to 0.02 mm² would have read as a defect. It is now
    // `ratchet_harness.rs` metric 7 (`abandoned_load_mm2`), recorded per model
    // against the corpus STL digest, where it covers all six measurable corpus
    // models instead of this one fixture and where a worsening must be argued
    // rather than merely re-pinned. Asserted here only as the bound that is
    // true of any plan: a refusal may not abandon MORE load than the plan ever
    // routed, and the disclosure may not invent a negative or non-finite area.
    let abandoned = authority.abandoned_contacts();
    let abandoned_mm2 = abandoned
        .iter()
        .map(plinth_support::AbandonedContactDisclosure::abandoned_area_mm2)
        .sum::<f64>();
    assert!(
        abandoned_mm2.is_finite() && abandoned_mm2 >= 0.0,
        "abandoned load must be a finite, non-negative area; got {abandoned_mm2} mm² across \
         {abandoned:?}"
    );
    // **TWO refusals, of two DIFFERENT kinds, and the fixture pins which is
    // which.** Measured 2026-08-20, and the pair is the reason this assertion
    // is worth writing at all:
    //
    //   * contact 25, seat (-5.495, -0.938, 3.750) mm — `buried_surface`. This
    //     one predates the scar bound entirely and has nothing to do with it:
    //     the OD-9 probe is a point on the surface normal, so NO tip radius
    //     moves a buried seat. The ladder was never walked
    //     (`growth_ladder_entered == false`, zero rungs) and the run report
    //     must not let Craig's field ceiling read as the reason it is missing.
    //   * contact 64, seat (10.928, 0.000, 6.304) mm — `not_attached`, ladder
    //     walked, six rungs from 0.0707 to 0.4000 mm RADIUS (0.1414 to 0.8000
    //     mm DIAMETER) and every one refused. THIS is the contact the bound
    //     withdraws, and here naming `TIP_RADIUS_MAX_GROWN_SCAR_MM` as the
    //     stopper is exactly true.
    //
    // RETIRED 2026-08-29 — the refusal-count equality (`abandoned.len() == 2`)
    // and the literal class vector
    // `[(25, "buried_surface", false, 0), (64, "not_attached", true, 6)]`.
    //
    // Contact IDs are placement-order artifacts, so both were wrong by
    // construction the moment placement moved; and both sat after this
    // function's digest equality, so neither could fail while that digest was
    // stale, which was every time placement moved. The prose above records what
    // they were for: a buried seat must not read as though the scar ceiling
    // refused it. That claim is not about contacts 25 and 64 — it is a
    // consistency rule between the fields of ANY refusal disclosure, and it is
    // asserted that way, on whichever contacts a placement change happens to
    // refuse, by
    // `abandoned_refusals_disclose_a_consistent_ladder_and_reason_whichever_contacts_are_refused`
    // at the bottom of this file. That test is strictly stronger here: it also
    // catches an abandoned contact owning an ATTESTING rung, which the literal
    // vector could never see because it only counted rungs.
    // That every abandoned contact quotes the SAME registered
    // `TIP_RADIUS_MAX_GROWN_SCAR_MM` ceiling, and that no growth rung it
    // offered exceeds that ceiling, is checked independently and
    // placement-agnostically -- on whichever contacts a placement change
    // happens to abandon, not just contacts 25 and 64 -- by
    // `toed_slab_contacts_stay_pairwise_distinct_and_under_the_scar_ceiling_regardless_of_the_digest`
    // below, so it is not repeated here gated behind this function's own
    // digest assertion.
    authority
        .into_supported_model()
        .expect("consume final support authority after verification");
}

fn support_input<'a>(
    delicacy_by_face: &'a [f64],
    layers: &'a LayerPlan,
) -> SupportPreparationInput<'a> {
    SupportPreparationLimits::production()
        .expect("production support preparation limits")
        .input(layers, delicacy_by_face, &[])
}

/// **FU-56's happy path, demonstrated end to end: `make_uncovered_rim_table`
/// CLOSES, and only because the controller appended repair contacts.**
///
/// **The fixture was re-pinned on 2026-08-26, and not one assertion below
/// moved with it.** This test read `make_overhang_table` until `3a6526f`
/// converged the clearance mask and seated the model anchor, which let the
/// model-seated anchor family route for the first time; the 30 mm table's
/// baseline layout then covered every island, `coverage_repair_rounds()` fell
/// to 0, and the panic this test printed named an EMPTY blocking list. That is
/// a fixture that stopped posing the question, not a runner that stopped
/// answering it, so the fixture was replaced rather than the expectation
/// edited (OD-8). `make_uncovered_rim_table` is the same generator at
/// 25 x 25 x 4 mm on 2 x 2 x 20 mm legs, and it measures 0.553 mm² uncovered
/// on island 8 at attempt 0 with a 6.901 mm worst gap against the 6.0 mm
/// registered radius — above the censor, so it BLOCKS — while one regular
/// contact on that same route is retired `NoWinner` in the run's own §6
/// removal ledger. The controller closes the hole with one committed contact
/// on its second trial.
///
/// **Nothing was loosened to get here.** The coverage radius is still the
/// registered `COVERAGE_RADIUS_FACTOR × TARGET_SPACING_MM` = 6.0 mm, the
/// patch-area floor is still `VOXEL_PITCH_MM²`, the uncovered-area tolerance
/// is still zero, and closure is still achieved only by appending immutable
/// repair-pool contacts. Every assertion is on the SHAPE of a committed step —
/// that attempt 0 measured a blocking patch, that the sealing step measures
/// clean, that each commit strictly reduced uncensored uncovered area — so no
/// number frozen to the retired fixture survives here to be re-pinned.
#[test]
fn public_runner_closes_the_uncovered_rim_table_only_after_appending_repair_contacts() {
    let hollowed = hollowed_corpus_model(
        make_uncovered_rim_table().expect("procedural uncovered-rim table"),
        "make_uncovered_rim_table",
    );
    // FU-38 (2026-08-12): real delicacy, not the zero field -- see
    // `real_delicacy`'s doc comment for what production this now matches.
    let delicacy = real_delicacy(&hollowed);
    let layers = LayerPlan::new(0.0, vec![0.05; 1_000]).expect("layer plan");
    let authority = run_support(
        hollowed,
        SupportRequest {
            lift_mm: PositiveFinite::new(5.0).expect("positive lift"),
        },
        support_input(&delicacy, &layers),
    )
    .expect("make_uncovered_rim_table closes coverage from its own repair pool");

    let steps = authority.repair_steps();
    let committed: Vec<_> = steps.iter().filter(|step| step.commit.is_some()).collect();
    assert!(
        authority.coverage_repair_rounds() >= 1,
        "this fixture must NOT close on its baseline layout -- that is what makes it the \
         success-after-repair demonstration: {steps:?}"
    );
    assert_eq!(
        committed.len(),
        authority.coverage_repair_rounds(),
        "a repair round IS a committed step now; if these disagree the counter is measuring \
         something other than the contacts the plan kept"
    );

    // Attempt 0 measured a blocking patch, and the record says so in its own
    // numbers: the FIRST trial's before-state is attempt 0's measurement, and a
    // model with no failing patch there would never have entered a transaction.
    let first = committed.first().expect("at least one committed step");
    assert!(
        first.objective_before.failing_patch_count > 0,
        "the first commit's before-state IS attempt 0's measurement, and it must show the \
         blocking patch that put this model into repair: {steps:?}"
    );
    // The closing step measures clean. That is the same claim the retired
    // `history`'s "the closing attempt measures clean and nominates nothing"
    // made, read off the step that actually did it rather than off an
    // aggregate that could not say which one that was.
    let last = committed.last().expect("at least one committed step");
    assert_eq!(
        last.objective_after
            .expect("a committed step was measured on both sides")
            .failing_patch_count,
        0,
        "this run sealed, so its last committed step must have measured zero failing patches: \
         {steps:?}"
    );

    // **The monotone pair, and what replaced it.** The retired version asserted
    // that the blocking patch count was non-increasing across attempts and fell
    // at least once — a PROXY for "a repair worked", because a batch of 266
    // appended rows could not say which row moved it. Under one-winner
    // transactions the property is a guarantee by construction and is asserted
    // as one: every committed step strictly reduced UNCENSORED uncovered area
    // (clause 3), which is coverage by definition, and no committed step
    // worsened the censored objective. A step that covered nothing cannot be
    // in this list at all.
    for step in &committed {
        let before = step.uncensored_before_mm2;
        let after = step
            .uncensored_after_mm2
            .expect("a committed step was measured on both sides");
        assert!(
            after < before,
            "trial {} committed without covering one sample ({before} -> {after} mm²): {steps:?}",
            step.trial
        );
        assert!(
            step.objective_after
                .expect("a committed step was measured on both sides")
                .failing_uncovered_area_mm2
                <= step.objective_before.failing_uncovered_area_mm2,
            "trial {} committed while the censored failing area grew: {steps:?}",
            step.trial
        );
    }
    // The accepted state moves ONLY inside a transaction: each commit's
    // before-state is the previous commit's after-state.
    for pair in committed.windows(2) {
        assert_eq!(
            pair[0].objective_after.expect("measured"),
            pair[1].objective_before,
            "something moved the accepted state between commits: {steps:?}"
        );
    }

    let snapshot = authority
        .verification_snapshot()
        .expect("borrow sealed verification snapshot");
    snapshot.validate_integrity().expect("snapshot integrity");
    authority
        .into_supported_model()
        .expect("consume final support authority after verification");
}

/// **Re-pinned off `make_overhang_table` onto `make_toed_slab` (FU-56,
/// 2026-08-12).** `run_support` is now the coverage-closure controller and
/// unconditionally measures ROUTED coverage at the REGISTERED gate radius
/// before ever returning a sealed authority
/// (`public_runner_seals_toed_slab_and_refuses_raking_blade_via_coverage_closure`,
/// `public_runner_refuses_overhang_table_with_a_genuinely_unclosable_island`).
/// `make_overhang_table` refuses at that radius, so it can no longer reach
/// THIS test's own, independently-chosen small-radius probe (radius 1.0 mm)
/// at all — there is no `FinalSupportAuthority` to call `measure_coverage`
/// on. `make_toed_slab` closes coverage on its first attempt (zero repair
/// rounds; its digest is re-pinned by FU-38 in lockstep with the sibling test
/// above), so it is the fixture that can still exercise this independent
/// measurement path, which is unrelated to closure and is testing
/// `measure_coverage`'s own resource-charging and evidence-projection
/// arithmetic under a request this test chooses itself. Every frozen number
/// below is freshly hand-counted from the checked-in toed-slab fixture under
/// the SAME explicit request the predecessor test pinned
/// (radius = 1.0 mm, patch-area floor = 1.0 mm²) — not carried over from the
/// old fixture's numbers, and not assembled from the measurement under test.
///
/// **FU-38, 2026-08-12 — this test IS item 4 of the ruling** (the toed-slab
/// geometry's structural asserts and hand-counted coverage resource plan), and
/// it was re-derived under the real producer-side delicacy field. **Exactly one
/// number below moved: `snapshot_digest`
/// `2_198_459_883_570_957_390 -> 15_556_275_517_430_246_244`**, the same value
/// the sibling test pins and the same value the frozen coverage-gate golden in
/// `plinth-verify` now carries. The 11-tuple resource plan, all 9 frozen
/// islands and all 9 frozen patches are UNCHANGED — measured, not assumed —
/// which is what says the delicacy field reordered exact-Z tie-breaks inside
/// routing without moving a single seat, sample, island or square millimetre
/// this measurement can see. No cap, ceiling, radius or floor in this test was
/// touched: the explicit request is still (1.0 mm, 1.0 mm²) against the same
/// six frozen ceilings.
///
/// **Task #49, 2026-08-19 — the same one number moved again, and again only
/// that one.** SPEC-A3-A4 §6's unrouted-regular removal is now wired, so the
/// snapshot is taken over the ACCEPTED contact set rather than the routed
/// plan: `snapshot_digest 15_556_275_517_430_246_244 ->
/// 9_355_852_079_468_421_123`. The 11-tuple resource plan, all 9 frozen
/// islands and all 9 frozen patches are UNCHANGED — re-derived at this tip,
/// not assumed. The paragraph above stands as the record of its own date.
///
/// **The seal-semantics lane, 2026-08-23 — one number again, and this time it
/// moved ON PURPOSE.** Craig's authorization item 5 keeps `candidates_feasible`
/// meaning exactly what it means today and adds a SECOND counter for ruled-out
/// arms (`FinalizedCcoTranscriptRow::candidates_ruled_out`). That counter is
/// folded into the sealed CCO transcript UNCONDITIONALLY, so the fingerprint
/// re-freezes once for every model in the corpus, deliberately, with the reason
/// written down: `snapshot_digest 4_190_462_360_230_124_963 ->
/// 12_136_443_226_163_403_608`. The counter is ZERO on every path today —
/// nothing rules an arm out until Stage B's steering channel lands — so this is
/// a stream-shape change and nothing else. MEASURED at the re-freeze: the
/// 11-tuple resource plan, all 9 islands, all 9 patches, all 7 defects and the
/// 64/170 counts are byte-identical; the only field that differs anywhere in
/// the frozen JSON is `snapshot_digest`. Ledgered in
/// `docs/evidence/emission-canary.md`.
#[test]
fn public_runner_measures_sealed_coverage_with_its_islands_patches_and_resource_plan() {
    let hollowed = hollowed_corpus_model(
        make_toed_slab().expect("procedural toed slab"),
        "make_toed_slab",
    );
    // FU-38 (2026-08-12): real delicacy, not the zero field -- see
    // `real_delicacy`'s doc comment for what production this now matches.
    let delicacy = real_delicacy(&hollowed);
    let layers = LayerPlan::new(0.0, vec![0.05; 1_000]).expect("layer plan");
    let authority = run_support(
        hollowed,
        SupportRequest {
            lift_mm: PositiveFinite::new(5.0).expect("positive lift"),
        },
        support_input(&delicacy, &layers),
    )
    .expect("public support runner");
    // Hand-counted from the checked-in toed-slab fixture, not by the
    // verifier's private preflight: 71 logical route rows across nine
    // islands, 69 physical seats, and 50,688 coverage rows.  The remaining
    // values are the frozen allocation arithmetic for that fixed fixture.
    let request = CoverageMeasurementRequest::from_f64(
        1.0,
        1.0,
        CoverageMeasurementLimits::new(74, 71, 50_688, 1_034_240, 12_624_272, 5_879_808)
            .expect("explicit frozen test ceilings"),
    )
    .expect("explicit finite coverage request");
    let snapshot = authority
        .verification_snapshot()
        .expect("borrow sealed verification snapshot");
    let measurement = snapshot
        .measure_coverage(request)
        .expect("public measurement");
    assert_eq!(measurement.snapshot_digest(), snapshot.digest());
    // ===================================================================
    // RETIRED 2026-08-29 — `assert_eq!(snapshot.digest(),
    // 13_198_418_135_389_558_006)`.
    //
    // It was never an independent measurement: it was the SAME fingerprint as
    // the closure test above, on the same fixture, re-derived in lockstep with
    // it every time it moved. Two copies of one fingerprint cost two re-pins
    // per change and catch nothing the first does not. Its movement history,
    // kept here because it is the evidence for retiring it rather than a value
    // anyone must maintain — six re-derivations in ten days, each for a change
    // that was correct:
    //   * 2026-08-19, tip minimum 0.2 -> 0.05 mm radius:
    //     9_355_852_079_468_421_123 -> 4_190_462_360_230_124_963
    //   * 2026-08-20, descending refinement scan on grow-to-attest:
    //     15_045_390_433_036_889_889 -> 14_374_106_896_419_012_910
    //   * 2026-08-20, the scar bound (`TIP_RADIUS_MAX_GROWN_SCAR_MM`):
    //     14_374_106_896_419_012_910 -> 4_190_462_360_230_124_963
    //   * 2026-08-23, the seal-semantics ruled-out-arm counter:
    //     4_190_462_360_230_124_963 -> 12_136_443_226_163_403_608
    //   * 2026-08-27/28, the raft pad/bridge dispatch split (`ad335d9`):
    //     12_136_443_226_163_403_608 -> 13_198_418_135_389_558_006
    // On that last one this copy and its sibling drifted apart — one golden was
    // refreshed and this pair was not — and nobody saw it, because the binary
    // holding them never finishes under a plain `cargo test`. A number three
    // files must re-pin in lockstep, in a binary that cannot report, is not a
    // guard.
    //
    // It was also the most expensive line in this file. EIGHT of the nine
    // checks in this function sat after it, and `assert_eq!` aborts a test at
    // its first mismatch — so the eleven-term resource plan, all nine islands
    // and all nine patches below stopped running entirely whenever this digest
    // was stale. MEASURED under the legal placement perturbation (reversing
    // the terminal tie-break in `initial.rs::routing_order`): every one of
    // those eight checks is bit-identical across it, and only this digest
    // moved (13198418135389558006 -> 9878193565529525115). This one line was
    // the whole cost of a legal placement change to this test, and it was
    // protecting nothing the perturbation could break.
    //
    // The digest's real content — "the plan did not change" — is an
    // EQUIVALENCE claim under the adopted doctrine: run at the time of a
    // specific change and recorded in that change's commit message, not stored.
    // ===================================================================
    //
    // KEPT, and no longer stranded behind that literal: the measurement must
    // carry the digest of the snapshot it was actually taken from. That is a
    // self-consistency invariant with no magic number in it, and it is
    // asserted above, before anything that can move.
    assert_eq!(measurement.request(), request);
    let plan = measurement.resource_plan();
    // **SPLIT 2026-08-29 into the two lifetimes it was conflating.** One
    // `assert_eq!` over eleven terms hid the fact that four of them are a
    // function of the ROUTED CONTACT COUNT and seven are not — the comment
    // that used to sit here said so in prose while the assertion treated all
    // eleven identically, so a placement change that moved the contact count
    // re-pinned all eleven and buried which four actually moved.
    //
    // GROUP 1 — functions of the fixed fixture mesh and this test's EXPLICIT
    // request, and of nothing else. Placement cannot move these: the mesh is
    // the checked-in toed slab and the request is written out three lines
    // above. Literals are correct here, and they are the assertion that would
    // catch a real change in how coverage is sampled or assessed.
    //
    // Confirmed, not assumed: bit-identical under the legal placement
    // perturbation (reversing the terminal tie-break in
    // `initial.rs::routing_order`), measured 2026-08-29.
    assert_eq!(
        (
            plan.query_row_count(),
            plan.assessment_work(),
            plan.assessment_scratch_bytes(),
            plan.assessment_evidence_bytes(),
            plan.assessment_phase_peak_bytes(),
        ),
        (50_688, 5_879_808, 3_902_976, 8_312_832, 12_215_808),
        "the coverage sampling and assessment phases are functions of the mesh and the request, \
         neither of which a placement change touches"
    );
    // GROUP 2 — functions of the routed contact count. These moved the last
    // time a contact was withdrawn (route_membership_sum 74 -> 71,
    // indexed_point_count 71 -> 69, distance_work 1_034_240 -> 1_033_728,
    // logical_peak_bytes 12_624_272 -> 12_624_152) and they will move again
    // whenever placement changes how many contacts survive. Pinning today's
    // four numbers would re-create exactly the toll this lane is removing, so
    // they are asserted as the RELATIONS that hold for any measurement instead.
    //
    // The strongest of these is the limits check: `measure_coverage` returned
    // `Ok`, and a measurement that succeeded must have stayed inside every
    // ceiling its own request handed it. That is a real guarantee about the
    // allocator, it is re-derived from the request rather than copied from
    // history, and it tracks placement instead of pinning one placement's
    // answer.
    let limits = request.limits();
    assert!(
        plan.route_membership_sum() <= limits.max_route_memberships(),
        "route memberships {} exceed the requested ceiling {}",
        plan.route_membership_sum(),
        limits.max_route_memberships()
    );
    assert!(
        plan.indexed_point_count() <= limits.max_indexed_points(),
        "indexed points {} exceed the requested ceiling {}",
        plan.indexed_point_count(),
        limits.max_indexed_points()
    );
    assert!(
        plan.query_row_count() <= limits.max_query_rows(),
        "query rows {} exceed the requested ceiling {}",
        plan.query_row_count(),
        limits.max_query_rows()
    );
    assert!(
        plan.distance_work() <= limits.max_distance_work(),
        "distance work {} exceeds the requested ceiling {}",
        plan.distance_work(),
        limits.max_distance_work()
    );
    assert!(
        plan.logical_peak_bytes() <= limits.max_logical_peak_bytes(),
        "logical peak bytes {} exceed the requested ceiling {}",
        plan.logical_peak_bytes(),
        limits.max_logical_peak_bytes()
    );
    assert!(
        plan.assessment_work() <= limits.max_assessment_work(),
        "assessment work {} exceeds the requested ceiling {}",
        plan.assessment_work(),
        limits.max_assessment_work()
    );
    // A SEQUENTIAL PEAK is a high-water mark taken while accumulating its own
    // total, so it can never exceed that total and can never be zero on a plan
    // that routed anything. Both directions are structural, and both hold
    // whatever placement does.
    assert!(
        plan.sequential_route_membership_peak() <= plan.route_membership_sum(),
        "the route-membership peak {} cannot exceed its own sum {}",
        plan.sequential_route_membership_peak(),
        plan.route_membership_sum()
    );
    assert!(
        plan.sequential_indexed_point_peak() <= plan.indexed_point_count(),
        "the indexed-point peak {} cannot exceed its own count {}",
        plan.sequential_indexed_point_peak(),
        plan.indexed_point_count()
    );
    assert!(
        plan.sequential_route_membership_peak() > 0 && plan.sequential_indexed_point_peak() > 0,
        "a measurement over a plan that routed contacts must show a non-zero peak: {plan:?}"
    );
    // These rows are frozen from corpus/baseline/make_toed_slab.* under the
    // explicit request above.  They are literal fixture authority, not
    // assembled from the snapshot or the measurement under test, so a change
    // in partitioning, sorting, or evidence projection cannot self-validate.
    let expected_islands: &[FrozenIslandEvidence] = &[
        (0, 0, 8, &[8]),
        (1, 1, 37, &[37]),
        (2, 1, 51, &[51]),
        (3, 0, 0, &[0]),
        (4, 1, 13, &[13]),
        (5, 2, 77, &[77]),
        (6, 0, 3, &[3]),
        (7, 1, 45, &[45]),
        (8, 2, 109, &[109]),
    ];
    assert_eq!(
        measurement.evidence().islands().len(),
        expected_islands.len()
    );
    for (island, &(island_id, component_id, representative_face_id, face_ids)) in measurement
        .evidence()
        .islands()
        .iter()
        .zip(expected_islands)
    {
        assert_eq!(
            (
                island.island_id(),
                island.component_id(),
                island.representative_face_id(),
                island.sorted_face_ids(),
            ),
            (island_id, component_id, representative_face_id, face_ids)
        );
    }
    let expected_patches: &[FrozenPatchEvidence] = &[
        (
            0,
            0,
            8,
            &[8],
            393.283_081_054_687_5_f64.to_bits(),
            false,
            true,
            5.137_869_702_330_431_f64.to_bits(),
        ),
        (
            1,
            1,
            37,
            &[37],
            1.416_113_088_335_631_4_f64.to_bits(),
            false,
            true,
            4.895_128_061_442_59_f64.to_bits(),
        ),
        (
            2,
            1,
            51,
            &[51],
            1.416_113_088_335_632_f64.to_bits(),
            false,
            true,
            4.895_128_061_442_59_f64.to_bits(),
        ),
        (
            3,
            0,
            0,
            &[0],
            50.577_392_578_125_01_f64.to_bits(),
            false,
            true,
            4.044_796_495_404_449_f64.to_bits(),
        ),
        (
            4,
            1,
            13,
            &[13],
            1.416_113_088_335_631_2_f64.to_bits(),
            false,
            true,
            4.895_128_061_442_59_f64.to_bits(),
        ),
        (
            5,
            2,
            77,
            &[77],
            // Re-derived 2026-08-20 with the scar bound: 0.461_625_340 ->
            // 0.482_931_125 mm² uncovered, +0.021_305_785 mm². Islands 5 and 8
            // are the two toe islands the withdrawn over-bound contact stood
            // between, they move together and by the same amount, and this is
            // the ENTIRE coverage cost of refusing that 1.301 mm-DIAMETER scar:
            // twenty-one thousandths of a square millimetre each, on islands
            // the gate still passes (`assessment_failed` stays false). The
            // other seven patches are bit-identical.
            0.482_931_124_892_749_53_f64.to_bits(),
            false,
            false,
            // The worst gap on this island moves with the same withdrawal:
            // 2.195_345_800 -> 4.380_949_641 mm, because the contact that was
            // standing nearest these faces is the one the scar bound refused.
            // The island's verdict is unchanged (`assessment_failed` false):
            // the gate rules on uncovered AREA, and 0.483 mm² still passes.
            4.380_949_640_913_765_f64.to_bits(),
        ),
        (
            6,
            0,
            3,
            &[3],
            378.698_730_468_75_f64.to_bits(),
            false,
            true,
            4.297_585_168_585_958_f64.to_bits(),
        ),
        (
            7,
            1,
            45,
            &[45],
            1.416_113_088_335_631_4_f64.to_bits(),
            false,
            true,
            4.895_128_061_442_591_f64.to_bits(),
        ),
        (
            8,
            2,
            109,
            &[109],
            // Island 8, the mirror of island 5 above, moves identically and one
            // ULP apart: 0.461_625_340 -> 0.482_931_125 mm² uncovered, worst
            // gap 2.195_345_800 -> 4.380_949_641 mm, verdict unchanged.
            0.482_931_124_892_749_64_f64.to_bits(),
            false,
            false,
            4.380_949_640_913_764_f64.to_bits(),
        ),
    ];
    assert_eq!(
        measurement.evidence().patches().len(),
        expected_patches.len()
    );
    for (
        patch,
        &(
            island_id,
            component_id,
            representative_face_id,
            face_ids,
            area_bits,
            cut_ambiguous,
            assessment_failed,
            gap_bits,
        ),
    ) in measurement
        .evidence()
        .patches()
        .iter()
        .zip(expected_patches)
    {
        // **The VERDICT is re-derived BEFORE it is compared.** `assessment_failed`
        // appears in the frozen tuple below beside the area it belongs to, which
        // means that table can only ever agree with itself: a bug that flipped
        // every verdict would be re-frozen as "the new answer" without anyone
        // seeing that the RULE had changed. The rule is stated here, from the
        // request's own patch-area minimum rather than a second hand-written
        // column, and it runs FIRST so a flipped verdict is reported as the rule
        // violation it is rather than as one more number to re-pin.
        //
        // Added 2026-08-29 with the pin retirement, and it holds on all nine
        // patches: areas 393.283, 50.577, 378.699 and the four at 1.416 exceed
        // the 1.0 mm² minimum and fail; the two toe patches at 0.483 do not and
        // pass. Unlike the frozen column, it keeps holding whatever a placement
        // change does to those areas.
        assert_eq!(
            patch.assessment_failed(),
            patch.uncovered_area_mm2().get() > request.patch_area_floor_mm2().get(),
            "a patch fails assessment exactly when its uncovered area exceeds the requested \
             minimum; patch on island {} reports {} at {} mm² against a {} mm² minimum",
            patch.island_id(),
            patch.assessment_failed(),
            patch.uncovered_area_mm2().get(),
            request.patch_area_floor_mm2().get()
        );
        assert_eq!(
            (
                patch.island_id(),
                patch.component_id(),
                patch.representative_face_id(),
                patch.sorted_face_ids(),
                patch.uncovered_area_mm2().get().to_bits(),
                patch.cut_ambiguous(),
                patch.assessment_failed(),
            ),
            (
                island_id,
                component_id,
                representative_face_id,
                face_ids,
                area_bits,
                cut_ambiguous,
                assessment_failed
            )
        );
        match patch.worst_gap() {
            CoverageGap::Finite(gap) => assert_eq!(gap.get().to_bits(), gap_bits),
            CoverageGap::NoRoutedContact => panic!("fixture has a finite routed gap"),
        }
    }
}

/// **The repair worklist is UNCENSORED, and this fixture is why** (co-owner
/// ruling, 2026-08-14).
///
/// The 0.16 mm² A3 censor (`docs/specs/coverage-repair.md` §4, `voxel_pitch_mm²`)
/// decides **verdicts**: sub-censor area is "censored A3 uncertainty …
/// diagnostic only … **not permission for another hard gate to fail**". When
/// the same censor was also applied to the coverage-closure controller's
/// repair worklist, three models that had emitted the day before started
/// refusing with REAL, above-censor uncovered area — `Skulker Zombie`
/// 0.35 mm², `Tavey Badger` 0.21 mm², `zombie-3-a` 12.65 mm² — because the
/// dust patches the censored worklist dropped had been nominating contacts
/// that incidentally closed those holes. A contact covers everything inside
/// the 6.0 mm coverage radius; it does not care which patch nominated it.
///
/// **The fixture reproduces that mechanism in one procedural mesh.** A tall
/// overhang table (legs 7 × 7 × 24 mm, top 42 × 26 × 4 mm) whose underside
/// keeps ONE residual patch of **0.0999… mm²** — below the censor, so the
/// verdict ignores it, and on an island with ~1,000 eligible repair rows —
/// plus a 0.6 × 0.6 × 5 mm stub hanging from that underside at (10.0, 7.5).
/// The stub's whole 0.36 mm² bottom face is uncovered: **2.25× the censor**,
/// so it is a real hole and no censor can excuse it, and its island has
/// **ZERO eligible repair rows of its own**. The only contact that can close
/// it is one nominated for the dust patch beside it.
///
/// **Measured both ways, on this exact mesh:**
///
/// | repair worklist | attempts (blocking / worklist / rows) | outcome |
/// | --- | --- | --- |
/// | uncensored (ruled) | `0: 4/4/4`, `1: 3/4/3`, `2: 1/2/1`, `3: 0/1/0` | **seals**, residual is one 0.0999 mm² censored patch |
/// | censored (the regression) | `0: 4/4/4`, `1: 3/4/2`, `2: 1/2/0` | **refuses** `NoEligibleRepairRows`, island 5 left **0.36 mm² uncovered** |
///
/// The censored column is not a hypothetical: it is what this fixture does
/// when `run_support` nominates against the verdict set instead of the
/// worklist, which is what the code did before this ruling.
///
/// **Nothing is loosened by passing.** The model seals with its every
/// remaining patch below the registered censor, which the pre-write gate
/// re-measures for itself, and the assertions below check exactly that rather
/// than trusting the controller's own word.
///
/// ---
///
/// # FINDING 1 — this fixture is RED ON PURPOSE, and here is what it caught
///
/// **Under the one-winner controller this model no longer seals.** It refuses
/// with `NoImprovingRepair { candidates_tried: 190, best_remaining_score_mm2:
/// 0.0 }` after **190 trials and 2 commits**, leaving island 4 (component 4)
/// at 0.2666015625 mm² and island 5 (component 5) at **0.36 mm² — the stub's
/// hole, untouched**. The red is NOT "the expectation needs re-pinning": the
/// expectation below is what the engine is supposed to do, and it stays until
/// the engine does it.
///
/// ## Amended 2026-08-21 — task #68 contract (1) has landed, and this fixture
/// still refuses. What moved, and what did not.
///
/// The append no longer replays the load partition
/// (`perception::append_to_specialized_load_partition`): every surviving
/// contact's carried area is bit-identical across a trial append, so no trial
/// can hand a 546.0 mm² block to whichever contact the repartition names. That
/// is the whole of contract (1) and it is delivered.
///
/// **The model does not seal, and it now measures WORSE on island 4 by a
/// constant.** Both runs below were taken at this commit, on this fixture,
/// with only `append_regular_repair_rows`' derivation switched — replay
/// against incremental — so the comparison is like for like:
///
/// | | replay (the defect) | incremental (contract 1) |
/// | --- | --- | --- |
/// | trials / commits | 190 / 2 | 190 / 2 |
/// | island 4 residual | 0.2666015625 mm² | 0.2666015625 mm² |
/// | island 5 residual | 0.36 mm² | 0.36 mm² |
/// | rejections | `no_improvement: 187, stage_refusal: 1` | `no_improvement: 181, stage_refusal: 7` |
/// | island-4 area a trial measures | **≤ 0.9664306640625 mm²** | **≤ 2.2994384765625 mm²** |
/// | `load_violations`, every state | **7** | **7** |
/// | wall / median per trial | 368 s / 1953 ms | 367 s / 1912 ms |
///
/// **The shift is EXACTLY 1.3330078125 mm², in every bucket.** The trial
/// distribution keeps its shape to the bit — the own-route re-buy offsets are
/// unchanged (`0.9664306640625 - 0.7998046875` and
/// `2.2994384765625 - 2.1328125` are both 0.166626 mm²) — and the whole
/// distribution simply rises by that one constant.
///
/// **That constant is phantom credit, and losing it is the fix working.** The
/// replay bought it by moving un-carriable blocks off incumbent contacts:
/// a contact relieved of a 546.0 mm² block gets a smaller tip, clears, routes,
/// and pays island 4. Nothing was supported by that shuffle — the block did
/// not get lighter, it got someone else's name on it — which is the
/// toed-slab phantom-credit pattern task #55 named. Contract (1) stops the
/// shuffle, so the credit stops arriving, and what is left is the true state
/// of a layout whose contacts are overloaded.
///
/// **What is still missing is contract (2) AT THE REPLAY SEAM, and the number
/// that says so is `load_violations: 7`.** It is 7 in every measured state of
/// both runs. Seven contacts carry more than the area law's single-contact
/// maximum of 37.699 mm², and until those blocks are SPLIT where they are
/// created rather than passed around, this fixture cannot route them however
/// the trials are ordered. Contract (2) is delivered at the append seam
/// (`append_to_specialized_load_partition` passes
/// `CarriableMaximum::AreaLaw`), and is BLOCKED at the replay seam by the
/// merge tournament's retained load delta, which indexes its arrays by
/// eligibility row and carries one winner per row — the escalation recorded on
/// task #68 and stated in full at `finalize_load_rows`' charge.
///
/// **The two commits are not in question.** Commit 1 is bit-identical across
/// both runs (trial 1, island 10 / component 5, objective_after
/// 1.3264306640625005). Commit 2 lands on a different equally-valued island-4
/// seat — candidate 1077 at (7.875, 8.9375, 24.0) under the replay, candidate
/// 1075 at (7.875, 7.3125, 24.0) under the incremental — and both leave island
/// 4 at 0.2666015625 mm².
///
/// **This fixture stays RED, and for exactly one reason: neither of the two
/// defects below has been resolved.** Item 1 (cross-route credit) is task #68's
/// own "Defect 1", which it records as separate and needing a design framing.
/// Item 2's identity survives with its base moved, as the table above shows.
/// Re-pinning the `expect` to a refusal would turn this fixture green over the
/// defect it exists to report.
///
/// ---
///
/// **The step stream says exactly why, and it is neither reachability nor
/// objective granularity.** Measured over all 190 trials, 2026-08-20:
///
/// 1. **The cross-route credit is scored but never delivered.** 44 of the 190
///    trials were credited an island-5 share in `scored_from` — commit 1
///    (0.36 of 2.22), commit 2 (0.36 of 1.0598) and 42 rejections. **Not one
///    route delta in the whole run names island 5**, and its worst gap reads
///    `NoRoutedContact` from first measurement to last. The two sides disagree
///    by construction: `coverage_closure::score_candidates_globally` scores a
///    seat against **every** uncovered sample within the 6.0 mm radius and
///    deliberately does **not** filter by `(island_id, component_id)`, while
///    the coverage measurement matches a sample only against routed contacts
///    of **its own** `(island_id, component_id)` route
///    (`fill_route_points_nearest`; the rule is written down at
///    `runner.rs`'s `OwnerlessIslandDisclosure`). A contact seated on island
///    4 can therefore be *paid* for island 5's hole and can never *close* it.
///
/// 2. **Every trial from the accepted state is arithmetic, not search.** From
///    the post-commit-2 state, for **182 of 182** measured trials:
///
///    ```text
///    island4_after = 0.9664306640625 - (own-route island-4 share, if the trial's contact routed)
///    ```
///
///    **RE-PINNED 2026-08-21, task #68 contract (1).** The identity holds
///    unchanged in shape; only its base moved, by exactly 1.3330078125 mm²:
///
///    ```text
///    island4_after = 2.2994384765625 - (own-route island-4 share, if the trial's contact routed)
///    ```
///
///    …over 179 of the 190 trials (100 at 2.2994384765625, 30 at 2.1328125,
///    27 at 2.199462890625, 5 at 2.26611328125, 2 at 2.2327880859375, 1 at
///    2.1661376953125, and 14 still at 0.9664306640625). The paragraph below
///    reads against the 0.9664306640625 base it was written for; the sentence
///    it makes — that this is arithmetic and not search — is what survived the
///    re-pin, and the amendment above says why the base moved.
///
///    That is: appending ANY repair row first destroys exactly the
///    0.6998291015625 mm² that commit 2 had bought — whether the row routes,
///    is quarantined, or is refused a winner, and wherever it sits, including
///    a seat 30 mm away — and a row that routes then re-buys exactly its own
///    -route share and nothing else. The best own-route share on the ranking
///    IS 0.6998291015625, so the best attainable result is break-even (trials
///    2, 4, 5 and 6 measure bit-identical objectives) and clause 1, which is
///    strict, can never be satisfied again. The search was decided at trial 8:
///    the loop spent 366 s over 190 trials at a measured 1.917 s median, so
///    roughly 351 s of it went on proving an identity.
///
/// 3. **No accept clause other than clause 1 ever fired.** The rejection
///    histogram is `{no_improvement: 187, stage_refusal: 1}` — clause 2
///    (non-regression) and clause 3 (uncensored area) rejected **nothing**, so
///    this is not an accept-clause calibration problem.
///
///    **RE-PINNED 2026-08-21:** `{no_improvement: 181, stage_refusal: 7}`.
///    Six trials moved from "measured and not better" to "the stage refused
///    them", which is the incremental append declining to place a row rather
///    than placing it and measuring nothing. Clauses 2 and 3 still reject
///    **nothing**, so the sentence this item makes is unchanged.
///
/// **Island 5's own pool cannot rescue it either.** It reports
/// `Blocked(PoolExhausted { pool_rows: 4, occupied: 3, ledgered: 1 })`: the
/// stub's bottom face is 0.6 mm × 0.6 mm, narrower than the candidate seat
/// pitch (0.8125 mm on this mesh), so the pool is exactly the face's four
/// corner seats and nothing interior. Both corners that ever became eligible
/// were tried and both failed before any measurement — trial 3 at
/// (10.3, 7.2, 19.0) took `unrouted_no_winner` 3 -> 4, and trial 28 at
/// (10.3, 7.8, 19.0) killed the whole chain with
/// `Lowering(RootRunTooShort { .. })`.
#[test]
fn closure_repairs_censored_dust_because_that_repair_closes_a_real_hole() {
    let hollowed = identity_hollowed(shelf_with_dust_beside_a_row_less_hole());
    let delicacy = real_delicacy(&hollowed);
    let layers = LayerPlan::new(0.0, vec![0.05; 1_000]).expect("layer plan");
    let authority = run_support(
        hollowed,
        SupportRequest {
            lift_mm: PositiveFinite::new(5.0).expect("positive lift"),
        },
        support_input(&delicacy, &layers),
    )
    // **FINDING 1's red starts here** — see the block above the test. Today
    // this `expect` is what fails, and the message states the finding rather
    // than the retired suspicion it replaced: the run reaches the two commits
    // it is supposed to reach and then cannot place a third, because the
    // island-5 share it keeps being credited for is scored across routes and
    // measured within one. Blaming the censored worklist here would be a claim
    // the step stream refutes — the worklist is uncensored on this path and the
    // model still refuses.
    .expect(
        "FINDING 1: the stub's 0.36 mm² hole must CLOSE from a contact nominated for the \
         censored dust patch beside it. Measured 2026-08-20 and again 2026-08-21 with task #68 \
         contract (1) landed: the run commits twice, never moves island 5 by a single sample, \
         and refuses with NoImprovingRepair after 190 trials. Seven contacts still carry more \
         than the area law allows one contact to carry; see the amendment above the test",
    );

    let steps = authority.repair_steps();
    let committed: Vec<_> = steps.iter().filter(|step| step.commit.is_some()).collect();
    // MEASURED, not aspirational: this run earns two commits — trial 1 closes
    // island 10's 1.86 mm² and trial 7 buys back island 4's 0.6998 mm². Pinning
    // it at 0 would have been a second, independent falsehood in a test that is
    // already red for one reason; a red fixture has to be red for exactly one.
    assert_eq!(
        authority.coverage_repair_rounds(),
        2,
        "this fixture must close in the measured number of committed steps: {steps:?}"
    );

    // **§E.5 case 4's mechanism, asserted on the real loop instead of through a
    // proxy — and this is the whole reason the fixture exists.**
    //
    // The stub's island has ZERO eligible repair rows of its own. The only
    // contact that can close its 0.36 mm² hole is one drawn from the DUST
    // route beside it — the route whose own residual is 0.0999 mm², below the
    // registered censor, which a censored worklist would have dropped. Under
    // the retired batch loop the closest this test could get was "some attempt
    // had a worklist wider than its verdict set". Under one-winner transactions
    // the step stream names the row, its route, and every route its GLOBAL
    // score was credited for, so the sentence §D.2 promised is a read rather
    // than an inference.
    let closer = committed
        .iter()
        .find(|step| {
            step.route_deltas.iter().any(|delta| {
                delta.failing_area_before_mm2 > 0.0 && delta.failing_area_after_mm2 == 0.0
            })
        })
        .unwrap_or_else(|| panic!("some committed step must close a FAILING patch: {steps:?}"));
    let closed = closer
        .route_deltas
        .iter()
        .find(|delta| delta.failing_area_before_mm2 > 0.0 && delta.failing_area_after_mm2 == 0.0)
        .expect("checked just above");
    let closed_route = (closed.island_id, closed.component_id);
    let winner_route = (closer.source_island_id, closer.source_component_id);
    assert_ne!(
        winner_route, closed_route,
        "the closing contact must come from a DIFFERENT route than the hole it closed; if it \
         came from the hole's own route this fixture is no longer testing the cross-route \
         effect it was built for: {steps:?}"
    );
    assert!(
        closer
            .scored_from
            .iter()
            .any(
                |share| (share.island_id, share.component_id) == closed_route
                    && share.area_mm2 > 0.0
            ),
        "the winner's GLOBAL score must be credited, by name and by area, for the route whose \
         hole it closed — that share IS the uncensored worklist's justification: {steps:?}"
    );
    assert!(
        (closed.failing_area_before_mm2 - 0.36).abs() < 1e-6,
        "the hole this fixture is about measures 0.36 mm²; if it moved, the mesh moved: {} mm²",
        closed.failing_area_before_mm2
    );

    // The closing step measures clean, which is what sealing means.
    //
    // **FINDING 1 keeps this assertion exactly as it stands.** It is the one
    // that legitimately fails while the model refuses, and it is the sentence
    // the fix has to make true: the last committed step measures
    // `failing_patch_count: 2` today — island 4 at 0.2666015625 mm² and island
    // 5 at 0.36 mm² — because commit 2 bought back its own route's area and was
    // never paid the island-5 share its GLOBAL score was credited for. Relaxing
    // this to "2" would turn the fixture green over the defect it exists to
    // report.
    let last = committed.last().expect("at least one committed step");
    assert_eq!(
        last.objective_after
            .expect("a committed step was measured on both sides")
            .failing_patch_count,
        0,
        "the closing step must measure no blocking patch: {steps:?}"
    );

    // And the sealed model is judged by the gate's own rule, re-measured here
    // rather than taken on the controller's word: every remaining patch is
    // censored, and at least one remains — a fixture with nothing left would
    // pass this test without exercising the censor at all.
    let censor = plinth_support::registered_coverage_censor_area_mm2()
        .expect("registered A3 coverage censor");
    let request = plinth_support::registered_coverage_request().expect("registered request");
    let snapshot = authority
        .verification_snapshot()
        .expect("borrow sealed verification snapshot");
    snapshot.validate_integrity().expect("snapshot integrity");
    let measurement = snapshot
        .measure_coverage(request)
        .expect("measure the sealed layout at the registered gate radius");
    let patches = measurement.evidence().patches();
    assert!(
        !patches.is_empty(),
        "the sealed model must still carry censored dust, or this fixture proves nothing"
    );
    for patch in patches {
        assert!(
            !plinth_support::coverage_patch_blocks_verdict(
                patch.uncovered_area_mm2().get(),
                patch.cut_ambiguous(),
                patch.assessment_failed(),
                censor,
            ),
            "island {} still blocks the verdict with {} mm²",
            patch.island_id(),
            patch.uncovered_area_mm2().get()
        );
        assert!(
            plinth_support::coverage_patch_needs_repair(
                patch.uncovered_area_mm2().get(),
                patch.cut_ambiguous(),
                patch.assessment_failed(),
            ),
            "a patch with no measured uncovered area would not be evidence of anything"
        );
    }
    authority
        .into_supported_model()
        .expect("consume final support authority after verification");
}

/// The mesh behind the test above: an overhang table that keeps one sub-censor
/// residual patch on its underside, with a thin stub hanging from that same
/// underside whose bottom face is a real 0.36 mm² hole its own island has no
/// repair row for.
///
/// Built here rather than in `plinth-mesh`'s fixture set because the geometry
/// is chosen for what the COVERAGE CONTROLLER does with it, not for any mesh
/// property; the two boxes are plain `plinth_mesh` primitives.
fn shelf_with_dust_beside_a_row_less_hole() -> Mesh {
    let table = plinth_mesh::make_overhang_table_with([7.0, 7.0, 24.0], [42.0, 26.0, 4.0], 0.0)
        .expect("procedural overhang table");
    let stub_size = [0.6, 0.6, 5.0];
    let stub = plinth_mesh::make_box(stub_size, [0.0, 0.0, 0.0]).expect("procedural stub");
    let mut verts: Vec<[f64; 3]> = table.verts().to_vec();
    let mut tris: Vec<[u32; 3]> = table.tris().to_vec();
    let base = u32::try_from(verts.len()).expect("vertex count fits u32");
    // The stub hangs from the table's underside (legs 24 mm tall), so its
    // bottom face sits 5 mm below it at (10.0, 7.5).
    let lift = 24.0 - stub_size[2];
    verts.extend(
        stub.verts()
            .iter()
            .map(|vertex| [vertex[0] + 10.0, vertex[1] + 7.5, vertex[2] + lift]),
    );
    tris.extend(
        stub.tris()
            .iter()
            .map(|triangle| [triangle[0] + base, triangle[1] + base, triangle[2] + base]),
    );
    Mesh::new(verts, tris).expect("table plus stub is one mesh")
}

/// A hollow hand-off for a mesh that has no checked-in corpus baseline: the
/// same identity-orientation path `plinth-support`'s own unit tests use
/// (`runner.rs`, `hollowed_fixture`). The fixture above is composed at test
/// time, so there is no `corpus/baseline/*.pipeline.json` to pin it to, and
/// the coverage question it asks does not depend on the orientation search.
fn identity_hollowed(mesh: Mesh) -> HollowedModel {
    let budget =
        DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
            .expect("ratified decimation budget");
    let decimated = run_decimation(&mesh, budget)
        .expect("decimate the composed fixture")
        .into_decimated_model()
        .expect("print-scale composed fixture");
    let (mesh, decimation) = decimated.into_parts();
    let orientation = OrientationHistory::from_reports(
        decimation,
        OrientationReport {
            provenance: OrientationProvenance::Pinned {
                pinned_report: PathBuf::from("composed-coverage-fixture.pipeline.json"),
            },
            selection: BaselineSelection::Identity,
            transform: RigidTransform::identity(),
        },
    );
    HollowedModel::solid_decision(mesh, orientation, SolidDecisionReason::BelowThreshold)
        .expect("solid decision handoff")
}

/// A model half buried under a blocker, composed at test time: the buried
/// half's terminals are all quarantined, so SPEC-A3-A4 §6's removal takes
/// every contact of the buried islands out of the accepted contact set and
/// the carried-load repartition is left with faces it cannot place.
fn half_buried_table() -> Mesh {
    let buried = make_overhang_table().expect("buried table fixture");
    let blocker =
        plinth_mesh::make_box([30.0, 30.0, 1.0], [0.0, 0.0, 16.56]).expect("burial blocker");
    let live = make_overhang_table().expect("live table fixture");
    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    // The live copy is translated clear of the blocker so the fixture keeps a
    // routing half as well as a buried one — the point is a PARTIAL loss of
    // owners, not an empty plan.
    for (part, shift) in [
        (&buried, [0.0, 0.0, 0.0]),
        (&blocker, [0.0, 0.0, 0.0]),
        (&live, [100.0, 0.0, 0.0]),
    ] {
        let base = u32::try_from(verts.len()).expect("vertex namespace fits u32");
        verts.extend(part.verts().iter().map(|vertex| {
            [
                vertex[0] + shift[0],
                vertex[1] + shift[1],
                vertex[2] + shift[2],
            ]
        }));
        tris.extend(
            part.tris()
                .iter()
                .map(|triangle| triangle.map(|vertex| vertex + base)),
        );
    }
    Mesh::new(verts, tris).expect("the composed half-buried fixture is one mesh")
}

/// **SPEC-A3-A4 §6's removal never pre-empts the measurement it feeds — it
/// DISCLOSES what it could not place and lets the coverage judgment rule.**
///
/// End to end through `run_support`, on a fixture built to make the condition
/// happen: half the model is buried under a blocker, every one of that half's
/// terminals is quarantined, and §6's removal therefore takes every contact of
/// the buried islands out of the accepted contact set. The carried-load
/// repartition is then left with faces no owner can carry.
///
/// **Revert-proof, and it guards the whole ruling.** The intermediate commit
/// `9a9b459` refused here, and the assertions below fail on that tree: the
/// `Ok` arm cannot be taken at all, and the `MeasurementRefused` detail it
/// produced contains "no eligible carrier", which the `Err` arm rejects by
/// name.
///
/// **Why refusing was wrong, in one sentence.** Load eligibility is a
/// provenance relation, not a reach test, so "every eligible owner lost its
/// contacts" is not a measurement that these faces are unsupported — an arm
/// belonging to a DIFFERENT OWNER on the same island can stand a millimetre
/// away and hold them — and the routed-coverage measurement §6 sequences AFTER
/// the removal is the thing that can tell. Refusing ahead of it decided a
/// support-adequacy question from a set-membership tally.
///
/// Corrected 2026-08-19: this used to say "a neighbouring ISLAND's contact",
/// which the measurement does not support. `fill_route_points_nearest` matches
/// a coverage sample only against routed contacts of its own
/// `(island_id, component_id)` route, so another island's arm is never counted
/// as coverage however close it stands. One island can carry several load
/// owners, and that is where the eligibility/reach gap actually lives.
///
/// The measured cost of that refusal was three corpus models' artifacts
/// (Deinonychus, Skeleton_2, Tavey), each one whose ownerless island's true
/// uncovered area was dust the registered A3 censor already forgives.
#[test]
fn a_spec6_removal_discloses_its_ownerless_islands_and_lets_the_measurement_rule() {
    let hollowed = identity_hollowed(half_buried_table());
    let delicacy = vec![0.0; hollowed.mesh().tris().len()];
    let layers = LayerPlan::new(0.0, vec![0.05; 1_000]).expect("layer plan");
    let outcome = run_support(
        hollowed,
        SupportRequest {
            lift_mm: PositiveFinite::new(5.0).expect("positive lift"),
        },
        support_input(&delicacy, &layers),
    );
    match outcome {
        Ok(authority) => {
            // The whole point of the split landing. Routing completed, so
            // whatever verdict this fixture reaches is reached by the coverage
            // measurement over the routed layout — and the islands the
            // repartition could not place are named, counted and quantified.
            let disclosed = authority.ownerless_islands();
            assert!(
                !disclosed.is_empty(),
                "the buried half's islands lost every contact; a run that seals over them \
                 MUST say so"
            );
            for island in disclosed {
                assert!(
                    island.ownerless_face_count() > 0
                        && island.ownerless_face_area_mm2().is_finite()
                        && island.ownerless_face_area_mm2() > 0.0,
                    "a disclosure with no faces or no area discloses nothing: {island:?}"
                );
                assert!(
                    island
                        .measured_uncovered_area_mm2()
                        .is_none_or(|area| area.is_finite() && area >= 0.0),
                    "when the disclosure states a measured area it must be a real one, and \
                     when it has none it must say so rather than report a zero: {island:?}"
                );
            }
        }
        Err(plinth_support::SupportRunError::CoverageClosure(refusal)) => {
            // The other honest ending: the coverage measurement RAN over the
            // routed layout and refused on what it found. That is a verdict
            // from the measurement, which is exactly what the ruling asks for.
            // What it must never again be is `MeasurementRefused` carrying a
            // §6 removal message, because that was the removal pre-empting the
            // measurement instead of feeding it.
            if let plinth_support::CoverageClosureReason::MeasurementRefused(detail) =
                &refusal.reason
            {
                assert!(
                    !detail.contains("no eligible carrier"),
                    "the §6 removal must not refuse ahead of the measurement it is supposed \
                     to feed: {detail}"
                );
            }
        }
        Err(other) => {
            panic!("a buried half must reach the coverage judgment, not a stage failure: {other:?}")
        }
    }
}

// =============================================================================
// THE THREE-KIND TESTING DOCTRINE (docs/development.md) -- INVARIANT AND
// DETERMINISM REPLACEMENTS
//
// The checks below assert properties the file's frozen-fingerprint and
// hand-counted-golden tests above were also, incidentally, standing in for.
// They are ADDITIONS: nothing above is removed, weakened, or reordered. Each
// one is written to keep telling the truth about the property it names
// whether or not a placement improvement has moved a sibling fingerprint that
// nobody has re-pinned yet.
// =============================================================================

/// **INVARIANT, independent of placement and of the fingerprint above it.**
///
/// `public_runner_seals_toed_slab_and_discloses_its_abandoned_load`
/// asserts this same fixture's routed seats are pairwise distinct and that no
/// grown tip exceeds the registered scar ceiling -- but both assertions sit
/// AFTER that test's `snapshot.digest()` equality, and `assert_eq!` aborts a
/// test function on its first mismatch. Under the adopted doctrine a moved
/// digest is an EXPECTED, non-blocking consequence of a placement
/// improvement, not a defect -- but until someone re-pins it, every assertion
/// written after it in that function, including these two structural
/// guarantees, silently stops running.
///
/// This test re-derives the same toed-slab plan and checks ONLY these two
/// properties, with no digest, no contact-ID literal, and no member count in
/// sight, so they keep running -- and keep failing loudly on a real
/// violation -- across a placement change whether or not the sibling test's
/// fingerprint has been re-pinned yet.
#[test]
fn toed_slab_contacts_stay_pairwise_distinct_and_under_the_scar_ceiling_regardless_of_the_digest() {
    let hollowed = hollowed_corpus_model(
        make_toed_slab().expect("procedural toed slab"),
        "make_toed_slab",
    );
    let delicacy = real_delicacy(&hollowed);
    let layers = LayerPlan::new(0.0, vec![0.05; 1_000]).expect("layer plan");
    let authority = run_support(
        hollowed,
        SupportRequest {
            lift_mm: PositiveFinite::new(5.0).expect("positive lift"),
        },
        support_input(&delicacy, &layers),
    )
    .expect("make_toed_slab must close coverage on its first closure attempt");
    let snapshot = authority
        .verification_snapshot()
        .expect("borrow sealed verification snapshot");
    snapshot.validate_integrity().expect("snapshot integrity");

    // No two routed contacts may occupy the identical physical seat, on ANY
    // accepted plan, regardless of where placement put them.
    let mut identities = BTreeSet::new();
    let mut seats = BTreeSet::new();
    for contact in snapshot.routed_contacts() {
        assert!(identities.insert(contact.physical_contact_id()));
        seats.insert(contact.position_mm().map(f64::to_bits));
    }
    assert_eq!(
        seats.len(),
        identities.len(),
        "two distinct routed contact identities must never share one physical seat position"
    );

    // No growth rung offered during a rescue may exceed the registered
    // TIP_RADIUS_MAX_GROWN_SCAR_MM ceiling, and every refusal must quote that
    // same registered limit by name -- on ANY plan, regardless of which
    // contacts a placement change happens to abandon.
    for contact in authority.abandoned_contacts() {
        assert_eq!(
            contact.growth_ceiling_constant(),
            "TIP_RADIUS_MAX_GROWN_SCAR_MM",
            "every abandoned contact must quote the SAME registered scar ceiling; got {contact:?}"
        );
        assert!(
            contact
                .growth_rungs()
                .iter()
                .all(|rung| rung.radius_mm() <= contact.growth_ceiling_radius_mm()),
            "no growth rung may propose a tip above the registered ceiling; got {contact:?}"
        );
    }
}

/// **DETERMINISM, not a frozen literal.**
///
/// `closure_repairs_censored_dust_because_that_repair_closes_a_real_hole` pins
/// the cross-route stub hole's measured area at exactly 0.36 mm² with the
/// comment "if it moved, the mesh moved" -- a hand-verified constant that
/// stands in for a claim about REPRODUCIBILITY: build the same procedural
/// fixture and measure it twice, and the answer must not depend on which of
/// the two runs you happen to be looking at. That claim is worth checking
/// directly, without hard-coding the number it happens to be today, so a
/// legitimate change to a fixture generator or to the area law never has to
/// touch a test that only wants to know "is this reproducible".
///
/// **This uses `make_uncovered_rim_table`, not the cross-route stub, so the
/// check runs in about a second rather than the ~190-trial repair search the
/// stub needs to reach its own known defect.** The property under test --
/// measuring the same procedural fixture twice gives the bit-identical
/// blocking area both times -- does not depend on which corpus fixture
/// demonstrates it, and `make_uncovered_rim_table` measures a real blocking
/// patch (`attempt 0`, island 8) on only its first commit.
#[test]
fn uncovered_rim_tables_attempt_zero_route_area_is_the_same_on_a_second_independent_run() {
    fn attempt_zero_blocking_route_area_mm2() -> f64 {
        let hollowed = hollowed_corpus_model(
            make_uncovered_rim_table().expect("procedural uncovered-rim table"),
            "make_uncovered_rim_table",
        );
        let delicacy = real_delicacy(&hollowed);
        let layers = LayerPlan::new(0.0, vec![0.05; 1_000]).expect("layer plan");
        let authority = run_support(
            hollowed,
            SupportRequest {
                lift_mm: PositiveFinite::new(5.0).expect("positive lift"),
            },
            support_input(&delicacy, &layers),
        )
        .expect("make_uncovered_rim_table closes coverage from its own repair pool");
        let steps = authority.repair_steps();
        let first = steps
            .iter()
            .find(|step| step.commit.is_some())
            .expect("at least one committed step");
        assert!(
            first.objective_before.failing_patch_count > 0,
            "attempt 0 must show the blocking patch that put this model into repair: {steps:?}"
        );
        first
            .route_deltas
            .iter()
            .find(|delta| delta.failing_area_before_mm2 > 0.0)
            .expect("attempt 0's blocking patch must show up as a per-route area too")
            .failing_area_before_mm2
    }

    let first = attempt_zero_blocking_route_area_mm2();
    let second = attempt_zero_blocking_route_area_mm2();
    assert_eq!(
        first.to_bits(),
        second.to_bits(),
        "measuring the same procedurally-generated fixture twice in one process must give the \
         bit-identical blocking route area both times: {first} vs {second}"
    );
}

/// **INVARIANT, on whichever contacts placement refuses — not on contacts 25
/// and 64.**
///
/// `public_runner_seals_toed_slab_and_discloses_its_abandoned_load` used to
/// pin the refusal set as a literal vector,
/// `[(25, "buried_surface", false, 0), (64, "not_attached", true, 6)]`,
/// sitting after that test's own `snapshot.digest()` equality. Two separate
/// problems: contact IDs are placement-order artifacts, so the vector is
/// wrong the moment placement moves; and it could not fail at all while the
/// digest above it was stale, which is every time placement moves. The
/// literal therefore guarded nothing precisely when it was needed.
///
/// What that vector was really standing in for is a set of consistency rules
/// between the fields of a refusal disclosure, and those hold for ANY refused
/// contact on ANY model. They are stated directly here, with no contact ID,
/// no count, and no digest, so they keep biting across a placement change.
///
/// The rules, and where each comes from:
///
/// * The refusal reason is one of the three
///   `ContactAttestationRefusal` variants `runner.rs` can project
///   (`buried_surface`, `not_attached`, `unusable_geometry`). A fourth string
///   means a variant was added without its disclosure.
/// * `growth_ladder_entered == false` means "no rung was ever proposed", per
///   `AbandonedContactDisclosure::growth_ladder_entered`'s own doc, so the
///   rung list MUST be empty. The reverse is NOT asserted: `true` with an
///   empty list is a documented, distinct case (the load law had already
///   sized the tip at or above the ceiling).
/// * **Every rung of an ABANDONED contact must read `attested == false`.**
///   This is the sharp one. A contact whose ladder found an attesting rung is
///   a CURE — it lands in `grown()` and is built — so an attesting rung
///   inside the abandoned list means the run threw away a support it had
///   already proved it could stand up.
/// * Rungs ascend in radius: the ladder is documented as "every larger tip
///   that was tried, ascending".
/// * A ceiling NAME maps to exactly one ceiling VALUE within a run. This
///   catches the drift the growth-ceiling disclosure exists to prevent — the
///   same registered constant quoting two different numbers on two contacts —
///   without pinning either number, which belongs to the registry and not to
///   this test.
/// * Refused contact IDs are unique, and every abandoned area is finite and
///   non-negative. The abandoned AREA is tracked as a quality number by
///   `ratchet_harness.rs`'s metric 7, not asserted as a literal here.
#[test]
fn abandoned_refusals_disclose_a_consistent_ladder_and_reason_whichever_contacts_are_refused() {
    let hollowed = hollowed_corpus_model(
        make_toed_slab().expect("procedural toed slab"),
        "make_toed_slab",
    );
    let delicacy = real_delicacy(&hollowed);
    let layers = LayerPlan::new(0.0, vec![0.05; 1_000]).expect("layer plan");
    let authority = run_support(
        hollowed,
        SupportRequest {
            lift_mm: PositiveFinite::new(5.0).expect("positive lift"),
        },
        support_input(&delicacy, &layers),
    )
    .expect("make_toed_slab must close coverage on its first closure attempt");

    const REGISTERED_REFUSALS: [&str; 3] = ["buried_surface", "not_attached", "unusable_geometry"];

    let mut seen_ids = BTreeSet::new();
    let mut ceiling_by_constant: std::collections::BTreeMap<String, u64> =
        std::collections::BTreeMap::new();

    for contact in authority.abandoned_contacts() {
        assert!(
            seen_ids.insert(contact.contact_id()),
            "a contact may be refused at most once; {} appears twice",
            contact.contact_id()
        );
        assert!(
            REGISTERED_REFUSALS.contains(&contact.refusal()),
            "refusal '{}' is not one of the registered reasons {REGISTERED_REFUSALS:?}; a new \
             ContactAttestationRefusal variant needs a disclosure string: {contact:?}",
            contact.refusal()
        );
        assert!(
            contact.abandoned_area_mm2().is_finite() && contact.abandoned_area_mm2() >= 0.0,
            "abandoned load must be a finite, non-negative area; got {}: {contact:?}",
            contact.abandoned_area_mm2()
        );
        assert!(
            contact.growth_ceiling_radius_mm().is_finite()
                && contact.growth_ceiling_radius_mm() > 0.0,
            "a refusal must quote a finite, positive growth ceiling; got {}: {contact:?}",
            contact.growth_ceiling_radius_mm()
        );
        if !contact.growth_ladder_entered() {
            assert!(
                contact.growth_rungs().is_empty(),
                "a refusal that never entered the ladder cannot have proposed a rung: {contact:?}"
            );
        }
        for rung in contact.growth_rungs() {
            assert!(
                !rung.attested(),
                "an ABANDONED contact cannot own an attesting rung -- a ladder that attested is \
                 a cure and belongs in the grown list, not the refused one: {contact:?}"
            );
        }
        for pair in contact.growth_rungs().windows(2) {
            assert!(
                pair[0].radius_mm() < pair[1].radius_mm(),
                "the growth ladder is documented as ascending; got {} then {}: {contact:?}",
                pair[0].radius_mm(),
                pair[1].radius_mm()
            );
        }
        let recorded = ceiling_by_constant
            .entry(contact.growth_ceiling_constant().to_owned())
            .or_insert_with(|| contact.growth_ceiling_radius_mm().to_bits());
        assert_eq!(
            *recorded,
            contact.growth_ceiling_radius_mm().to_bits(),
            "the registered constant '{}' must name ONE value within a run; it reported {} here \
             and {} on an earlier contact",
            contact.growth_ceiling_constant(),
            contact.growth_ceiling_radius_mm(),
            f64::from_bits(*recorded)
        );
    }
}
