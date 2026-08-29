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
///   (`public_runner_measures_sealed_coverage_with_frozen_resources_and_evidence`)
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
/// `public_runner_measures_sealed_coverage_with_frozen_resources_and_evidence`
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
#[test]
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn public_runner_seals_toed_slab_with_its_frozen_digest_and_abandoned_load() {
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
    let mut identities = BTreeSet::new();
    let mut seats = BTreeSet::new();
    for contact in snapshot.routed_contacts() {
        assert!(identities.insert(contact.physical_contact_id()));
        seats.insert(contact.position_mm().map(f64::to_bits));
    }
    // Re-derived 2026-08-28 for the RAFT PAD/BRIDGE DISPATCH SPLIT (`ad335d9`):
    // 12_136_443_226_163_403_608 -> 13_198_418_135_389_558_006.  The counts do
    // NOT move — 64 contacts, 170 members, before and after — only the
    // classification of raft primitives inside them.  Before `ad335d9`, pad and
    // bridge were told apart by comparing a segment's two endpoints for exact
    // `f64` equality; that read the right answer only because one producer
    // happened to pass the same point twice, so a zero-length bridge silently
    // fell through to pad geometry.  The producer now states `RaftPad` or
    // `RaftBridge` and a zero-length bridge is an error.  The digest moves
    // because the plan now records the kind that was chosen instead of one
    // re-inferred from coordinates.
    //
    // This pair went unrefreshed when `ad335d9` landed, while its sibling
    // golden (`coverage_gate_toed_slab.json`, `15e4a6f`) was refreshed.  They
    // were found only because this binary was run under `gtimeout`, letting its
    // early tests report before the unbounded coverage-repair search was killed:
    // under a plain `cargo test` this binary never finishes, so nothing in it
    // reports and it has been unable to guard anything it contains.
    assert_eq!(
        (
            identities.len(),
            snapshot.final_live_members().count(),
            snapshot.digest(),
        ),
        (64_usize, 170_usize, 13_198_418_135_389_558_006_u64),
        "make_toed_slab's counts must not move: it takes zero repair rounds"
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
    assert_eq!(seats.len(), identities.len());
    // **THE ABANDONED LOAD, ASSERTED — not left in prose.**
    //
    // Accepting 65 -> 64 routed contacts turns entirely on the claim above that
    // the contact the scar bound withdraws carries **0.000 mm²**: the bound
    // costs this model a 1.3 mm-diameter scar and buys back nothing that was
    // holding it up. A comment cannot fail, so the claim is measured here. If a
    // future change ever makes the bound drop a contact that IS carrying load,
    // this fixture must go red rather than keep quoting a number nobody checks.
    let abandoned = authority.abandoned_contacts();
    let abandoned_mm2 = abandoned
        .iter()
        .map(plinth_support::AbandonedContactDisclosure::abandoned_area_mm2)
        .sum::<f64>();
    assert_eq!(
        abandoned_mm2, 0.0,
        "the scar bound must cost this fixture no routed load; got {abandoned_mm2} mm² across \
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
    // Both abandon 0.000 mm², which the sum above already states.
    assert_eq!(
        abandoned.len(),
        2,
        "this fixture refuses exactly two contacts — one buried seat and one the scar bound \
         withdraws; got {abandoned:?}"
    );
    let classes = abandoned
        .iter()
        .map(|contact| {
            (
                contact.contact_id(),
                contact.refusal(),
                contact.growth_ladder_entered(),
                contact.growth_rungs().len(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        classes,
        vec![
            (25_u32, "buried_surface", false, 0_usize),
            (64_u32, "not_attached", true, 6_usize),
        ],
        "a buried seat must be recorded as never having entered the ladder, and only the \
         ladder-walked refusal may read as the bound's doing"
    );
    for contact in abandoned {
        assert_eq!(
            contact.growth_ceiling_constant(),
            "TIP_RADIUS_MAX_GROWN_SCAR_MM",
            "both paths must quote the SAME registered limit, so the number never changes \
             meaning between lines; got {contact:?}"
        );
        assert!(
            contact
                .growth_rungs()
                .iter()
                .all(|rung| rung.radius_mm() <= contact.growth_ceiling_radius_mm()),
            "no rung may propose a tip above the ceiling; got {contact:?}"
        );
    }
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
fn public_runner_measures_sealed_coverage_with_frozen_resources_and_evidence() {
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
    // Same snapshot digest as the toed-slab closure test above, re-derived
    // 2026-08-20 for the SCAR BOUND (`TIP_RADIUS_MAX_GROWN_SCAR_MM` = 0.400 mm
    // RADIUS / 0.800 mm DIAMETER, Craig's field ceiling):
    // 14_374_106_896_419_012_910 -> 4_190_462_360_230_124_963.  ONE contact
    // moved — contact 64's cure needed 0.650630996 mm RADIUS
    // (1.301261991 mm DIAMETER), 1.63x the ceiling, so it is refused and
    // disclosed instead of built.  The full measurement, including the
    // 0.000 mm² of load that refusal abandons, is recorded at the closure test
    // above.  The frozen resource arithmetic below MOVES with the count, and
    // is re-derived here rather than inherited.
    //
    // Superseded records: re-derived 2026-08-20 for the descending refinement
    // scan on grow-to-attest (15_045_390_433_036_889_889 ->
    // 14_374_106_896_419_012_910), and 2026-08-19 for the tip-minimum ruling,
    // 0.2 -> 0.05 mm RADIUS / 0.4 -> 0.1 mm DIAMETER
    // (9_355_852_079_468_421_123 -> 4_190_462_360_230_124_963).
    //
    // Re-derived 2026-08-28 for the raft pad/bridge dispatch split (`ad335d9`):
    // 12_136_443_226_163_403_608 -> 13_198_418_135_389_558_006.  Same value and
    // same reason as the closure test above, where the argument is written out.
    assert_eq!(snapshot.digest(), 13_198_418_135_389_558_006);
    assert_eq!(measurement.request(), request);
    let plan = measurement.resource_plan();
    assert_eq!(
        (
            plan.route_membership_sum(),
            plan.sequential_route_membership_peak(),
            plan.indexed_point_count(),
            plan.sequential_indexed_point_peak(),
            plan.query_row_count(),
            plan.distance_work(),
            plan.logical_peak_bytes(),
            plan.assessment_work(),
            plan.assessment_scratch_bytes(),
            plan.assessment_evidence_bytes(),
            plan.assessment_phase_peak_bytes(),
        ),
        // Re-derived 2026-08-20 with the scar bound. Only the three terms that
        // are a function of the ROUTED CONTACT COUNT moved, which is exactly
        // what withdrawing one over-bound contact must do and nothing more:
        //   * route_membership_sum   74 -> 71  (the contact left three routes)
        //   * indexed_point_count    71 -> 69
        //   * distance_work     1_034_240 -> 1_033_728  (= query rows x points)
        //   * logical_peak_bytes 12_624_272 -> 12_624_152
        // The sequential peaks (30, 30), the query row count and the whole
        // assessment phase are UNMOVED: they are functions of the mesh and the
        // request, not of how many contacts survived.
        (
            71, 30, 69, 30, 50_688, 1_033_728, 12_624_152, 5_879_808, 3_902_976, 8_312_832,
            12_215_808
        )
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
/// `public_runner_seals_toed_slab_with_its_frozen_digest_and_abandoned_load`
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
