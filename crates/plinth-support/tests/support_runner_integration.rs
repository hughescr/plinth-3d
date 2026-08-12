use std::{collections::BTreeSet, path::PathBuf};

use plinth_core::{LayerPlan, PositiveFinite};
use plinth_decimate::{run as run_decimation, DecimationBudget, ProbeConfig};
use plinth_hollow::{HollowedModel, SolidDecisionReason};
use plinth_mesh::{make_overhang_table, make_raking_blade, make_toed_slab, Mesh};
use plinth_orient::{run as run_orientation, OrientRequest, PinnedBaseline};
use plinth_support::{
    run_support, CoverageGap, CoverageMeasurementLimits, CoverageMeasurementRequest,
    SupportPreparationInput, SupportPreparationLimits, SupportRequest,
};

type FrozenIslandEvidence = (u32, u32, u32, &'static [u32]);
type FrozenPatchEvidence = (u32, u32, u32, &'static [u32], u64, bool, bool, u64);

fn hollowed_overhang_table() -> HollowedModel {
    let mesh = make_overhang_table().expect("procedural overhang table");
    let budget =
        DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
            .expect("ratified decimation budget");
    let decimated = run_decimation(&mesh, budget)
        .expect("decimate procedural overhang table")
        .into_decimated_model()
        .expect("print-scale procedural overhang table");
    let oriented = run_orientation(
        decimated,
        &OrientRequest {
            pinned: PinnedBaseline {
                pipeline_report: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../corpus/baseline/make_overhang_table.pipeline.json"),
            },
        },
    )
    .expect("orient from checked-in overhang-table baseline");
    let (mesh, orientation) = oriented.into_parts();
    HollowedModel::solid_decision(mesh, orientation, SolidDecisionReason::BelowThreshold)
        .expect("solid decision handoff")
}

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
/// (`docs/engineering-journal/contact-physics-2026-08-04.md`, PR-8 / OD-4).
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
/// print layer by the census attribution
/// (`docs/engineering-journal/slicer-union-design-2026-08-07.md` sec 8.5 and its
/// 2026-08-08 addendum):
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
/// fingerprints were generated!)". See `docs/engineering-journal/CURRENT-STATUS.md`
/// sections `#fu32-j3-2026-08-09` (the attribution) and `#fu32-j4-2026-08-09`
/// (this re-pin).
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
/// (`CURRENT-STATUS.md` FU-56). **Do not respond to this by touching the
/// radius, the floor, or the tolerance** — the FU-56 ruling is explicit that
/// "not one of the 21 [minis] has been shown to be closable ... the first
/// action is what would show it," and this fixture's answer is "not
/// closable from this pool," recorded rather than forced.
#[test]
fn public_runner_seals_toed_slab_and_refuses_raking_blade_via_coverage_closure() {
    let hollowed = hollowed_corpus_model(
        make_toed_slab().expect("procedural toed slab"),
        "make_toed_slab",
    );
    let delicacy = vec![0.0; hollowed.mesh().tris().len()];
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
    let mut identities = BTreeSet::new();
    let mut seats = BTreeSet::new();
    for contact in snapshot.routed_contacts() {
        assert!(identities.insert(contact.physical_contact_id()));
        seats.insert(contact.position_mm().map(f64::to_bits));
    }
    assert_eq!(
        (
            identities.len(),
            snapshot.final_live_members().count(),
            snapshot.digest(),
        ),
        (64_usize, 170_usize, 2_198_459_883_570_957_390_u64),
        "make_toed_slab's digest must not move: it takes zero repair rounds"
    );
    assert_eq!(seats.len(), identities.len());
    authority
        .into_supported_model()
        .expect("consume final support authority after verification");

    let hollowed = hollowed_corpus_model(
        make_raking_blade().expect("procedural raking blade"),
        "make_raking_blade",
    );
    let delicacy = vec![0.0; hollowed.mesh().tris().len()];
    let error = run_support(
        hollowed,
        SupportRequest {
            lift_mm: PositiveFinite::new(5.0).expect("positive lift"),
        },
        support_input(&delicacy, &layers),
    )
    ;
    let error = match error {
        Err(error) => error,
        Ok(_) => panic!(
            "make_raking_blade has two islands the immutable repair pool cannot close at the registered radius"
        ),
    };
    let plinth_support::SupportRunError::CoverageClosure(refusal) = error else {
        panic!("expected a coverage-closure refusal, got a different SupportRunError");
    };
    assert_eq!(refusal.attempts, 8);
    assert_eq!(
        refusal.reason,
        plinth_support::CoverageClosureReason::AttemptsExhausted
    );
    assert_eq!(
        refusal.history.len(),
        9,
        "attempts 0..=8 each measure exactly once"
    );
    let mut islands = refusal.islands.clone();
    islands.sort_by_key(|island| island.island_id);
    assert_eq!(
        islands
            .iter()
            .map(|island| (
                island.island_id,
                island.component_id,
                island.representative_face_id,
                island.worst_gap,
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                3,
                1,
                15,
                plinth_support::CoverageClosureGap::NoRoutedContact
            ),
            (
                5,
                1,
                14,
                plinth_support::CoverageClosureGap::NoRoutedContact
            ),
        ]
    );
}

fn support_input<'a>(
    delicacy_by_face: &'a [f64],
    layers: &'a LayerPlan,
) -> SupportPreparationInput<'a> {
    SupportPreparationLimits::production()
        .expect("production support preparation limits")
        .input(layers, delicacy_by_face, &[])
}

/// **FU-56, 2026-08-12: `make_overhang_table` refuses with a named,
/// genuinely-unclosable coverage-closure island, and that is the correct
/// answer.** Island 0 (component 4, face 51) is 788.05 mm² uncovered at this
/// crate's own small test-radius probe (radius 1.0 mm — see
/// `public_runner_measures_sealed_coverage_with_frozen_resources_and_evidence`,
/// re-pinned off this fixture for the same reason) and still 13.68 mm²
/// uncovered at the registered gate radius (6.0 mm): the larger radius helps
/// a great deal but does not close it. Across all 8 closure attempts the
/// controller nominates 8 DISTINCT repair-pool candidates on face 51
/// (`ContactCandidateId`s 535, 568, 569, 601, 602, 603, 634, 635) and the
/// measured uncovered area does not move by even one bit — the routed-contact
/// projection this island needs is never admitted by the CCO transcript from
/// this fixture's repair pool at this radius, for any of them. This is a real,
/// previously invisible defect in the fixture's baseline placement, exactly
/// the class FU-56 exists to surface (`CURRENT-STATUS.md` FU-56: "not one of
/// the 21 has been shown to be closable ... report the measured before/after
/// count; do not respond to a poor rate by touching the radius, the floor or
/// the tolerance"). Nothing here does.
#[test]
fn public_runner_refuses_overhang_table_with_a_genuinely_unclosable_island() {
    let hollowed = hollowed_overhang_table();
    let delicacy = vec![0.0; hollowed.mesh().tris().len()];
    let layers = LayerPlan::new(0.0, vec![0.05; 1_000]).expect("layer plan");
    let error = run_support(
        hollowed,
        SupportRequest {
            lift_mm: PositiveFinite::new(5.0).expect("positive lift"),
        },
        support_input(&delicacy, &layers),
    )
    ;
    let error = match error {
        Err(error) => error,
        Ok(_) => panic!(
            "make_overhang_table's island 0 has no closing repair-pool candidate at the registered radius"
        ),
    };
    let plinth_support::SupportRunError::CoverageClosure(refusal) = error else {
        panic!("expected a coverage-closure refusal, got a different SupportRunError");
    };
    assert_eq!(refusal.attempts, 8);
    assert_eq!(
        refusal.reason,
        plinth_support::CoverageClosureReason::AttemptsExhausted
    );
    assert_eq!(refusal.islands.len(), 1);
    let island = &refusal.islands[0];
    assert_eq!(
        (
            island.island_id,
            island.component_id,
            island.representative_face_id
        ),
        (0, 4, 51)
    );
    assert!(island.uncovered_area_mm2 > 0.0 && island.uncovered_area_mm2 < 788.05);
    assert!(matches!(
        island.worst_gap,
        plinth_support::CoverageClosureGap::Finite(_)
    ));
    assert_eq!(
        refusal.history.len(),
        9,
        "attempts 0..=8 each measure exactly once"
    );
    // Every attempt measures the same one blocking patch and nominates
    // exactly one candidate, up to the terminal attempt, which measures but
    // does not nominate -- the shape the registered bound itself predicts.
    for (index, attempt) in refusal.history.iter().enumerate() {
        assert_eq!(attempt.attempt, index);
        assert_eq!(attempt.blocking_patch_count, 1);
        assert_eq!(attempt.nominated_rows, usize::from(index < 8));
    }
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
/// rounds; digest unchanged from every prior re-pin, see the sibling test
/// above), so it is the fixture that can still exercise this independent
/// measurement path, which is unrelated to closure and is testing
/// `measure_coverage`'s own resource-charging and evidence-projection
/// arithmetic under a request this test chooses itself. Every frozen number
/// below is freshly hand-counted from the checked-in toed-slab fixture under
/// the SAME explicit request the predecessor test pinned
/// (radius = 1.0 mm, patch-area floor = 1.0 mm²) — not carried over from the
/// old fixture's numbers, and not assembled from the measurement under test.
#[test]
fn public_runner_measures_sealed_coverage_with_frozen_resources_and_evidence() {
    let hollowed = hollowed_corpus_model(
        make_toed_slab().expect("procedural toed slab"),
        "make_toed_slab",
    );
    let delicacy = vec![0.0; hollowed.mesh().tris().len()];
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
        CoverageMeasurementLimits::new(71, 69, 50_688, 1_033_728, 12_624_152, 5_879_808)
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
    assert_eq!(snapshot.digest(), 2_198_459_883_570_957_390);
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
            0.482_931_124_892_749_53_f64.to_bits(),
            false,
            false,
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
