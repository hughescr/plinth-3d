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
/// (`docs/rust-migration/contact-physics-2026-08-04.md`, PR-8 / OD-4).
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
/// (`docs/rust-migration/slicer-union-design-2026-08-07.md` sec 8.5 and its
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
#[test]
fn public_runner_seals_toed_slab_and_raking_blade_after_seat_coalescing() {
    let mut failures = Vec::new();
    for (name, mesh, expected_routed, expected_live, expected_digest) in [
        (
            "make_toed_slab",
            make_toed_slab().expect("procedural toed slab"),
            64_usize,
            170_usize,
            13_346_779_153_602_682_749_u64,
        ),
        (
            "make_raking_blade",
            make_raking_blade().expect("procedural raking blade"),
            27,
            72,
            8_660_653_339_934_460_766,
        ),
    ] {
        let hollowed = hollowed_corpus_model(mesh, name);
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = LayerPlan::new(0.0, vec![0.05; 1_000]).expect("layer plan");
        let authority = run_support(
            hollowed,
            SupportRequest {
                lift_mm: PositiveFinite::new(5.0).expect("positive lift"),
            },
            support_input(&delicacy, &layers),
        )
        .unwrap_or_else(|error| panic!("{name} must reach a sealed support run: {error}"));
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
        let actual = (
            identities.len(),
            snapshot.final_live_members().count(),
            snapshot.digest(),
        );
        if actual != (expected_routed, expected_live, expected_digest)
            || seats.len() != identities.len()
        {
            failures.push(format!(
                "{name}: actual={actual:?} expected={:?}; distinct_seats={} of {}",
                (expected_routed, expected_live, expected_digest),
                seats.len(),
                identities.len(),
            ));
        }
        authority
            .into_supported_model()
            .expect("consume final support authority after verification");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn support_input<'a>(
    delicacy_by_face: &'a [f64],
    layers: &'a LayerPlan,
) -> SupportPreparationInput<'a> {
    SupportPreparationLimits::production()
        .expect("production support preparation limits")
        .input(layers, delicacy_by_face, &[])
}

#[test]
fn public_runner_seals_and_verifies_overhang_table() {
    let hollowed = hollowed_overhang_table();
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

    {
        let snapshot = authority
            .verification_snapshot()
            .expect("borrow sealed verification snapshot");
        snapshot.validate_integrity().expect("snapshot integrity");

        let mut identities = BTreeSet::new();
        for contact in snapshot.routed_contacts() {
            assert!(identities.insert(contact.physical_contact_id()));
            assert!(contact.position_mm().iter().all(|value| value.is_finite()));
            assert!(contact.normal().iter().all(|value| value.is_finite()));
            assert!(contact.carried_area_mm2().is_finite());
            assert!(contact.carried_area_mm2() >= 0.0);
        }
    }

    authority
        .into_supported_model()
        .expect("consume final support authority after verification");
}

#[test]
fn public_runner_measures_sealed_coverage_with_frozen_resources_and_evidence() {
    let hollowed = hollowed_overhang_table();
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
    // Hand-counted from the checked-in overhang-table fixture, not by the
    // verifier's private preflight: 81 logical route rows across nine
    // islands, 77 physical seats, and 50,176 coverage rows.  The remaining
    // values are the frozen allocation arithmetic for that fixed fixture.
    let request = CoverageMeasurementRequest::from_f64(
        1.0,
        1.0,
        CoverageMeasurementLimits::new(81, 77, 50_176, 1_574_912, 12_497_064, 5_820_416)
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
            81, 45, 77, 45, 50_176, 1_574_912, 12_497_064, 5_820_416, 3_863_552, 8_228_864,
            12_092_416
        )
    );
    // These rows are frozen from corpus/baseline/make_overhang_table.* under
    // the explicit request above.  They are literal fixture authority, not
    // assembled from the snapshot or the measurement under test, so a change
    // in partitioning, sorting, or evidence projection cannot self-validate.
    let expected_islands: &[FrozenIslandEvidence] = &[
        (0, 4, 51, &[51, 56]),
        (1, 1, 14, &[14]),
        (2, 3, 38, &[38]),
        (3, 0, 2, &[2]),
        (4, 2, 26, &[26]),
        (5, 1, 20, &[20]),
        (6, 3, 44, &[44]),
        (7, 2, 32, &[32]),
        (8, 0, 8, &[8]),
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
            4,
            51,
            &[51, 56],
            788.049_316_406_25_f64.to_bits(),
            false,
            true,
            4_620_851_132_428_015_356,
        ),
        (
            1,
            1,
            14,
            &[14],
            30.312_5_f64.to_bits(),
            false,
            true,
            4_613_848_121_590_755_819,
        ),
        (
            2,
            3,
            38,
            &[38],
            30.312_5_f64.to_bits(),
            false,
            true,
            4_613_848_121_590_755_819,
        ),
        (
            3,
            0,
            2,
            &[2],
            30.312_5_f64.to_bits(),
            false,
            true,
            4_613_848_121_590_755_831,
        ),
        (
            4,
            2,
            26,
            &[26],
            30.312_5_f64.to_bits(),
            false,
            true,
            4_613_848_121_590_755_831,
        ),
        (
            5,
            1,
            20,
            &[20],
            6.843_749_999_999_999_f64.to_bits(),
            false,
            true,
            4_615_816_357_518_629_816,
        ),
        (
            6,
            3,
            44,
            &[44],
            6.843_749_999_999_999_f64.to_bits(),
            false,
            true,
            4_615_816_357_518_629_816,
        ),
        (
            7,
            2,
            32,
            &[32],
            6.843_749_999_999_999_f64.to_bits(),
            false,
            true,
            4_615_816_357_518_629_823,
        ),
        (
            8,
            0,
            8,
            &[8],
            6.843_749_999_999_999_f64.to_bits(),
            false,
            true,
            4_615_816_357_518_629_823,
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
