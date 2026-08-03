use std::{collections::BTreeSet, path::PathBuf};

use geom_kernel::{KnnBatchLimits, PenetrationBatchLimits};
use plinth_core::{LayerPlan, PositiveFinite};
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
    ContactConstructionLimits, CoverageGap, CoverageMeasurementLimits, CoverageMeasurementRequest,
    SupportPreparationInput, SupportRequest,
};

const GRID_BYTES: usize = 64 * 1024 * 1024;
type FrozenIslandEvidence = (u32, u32, u32, &'static [u32]);
type FrozenPatchEvidence = (u32, u32, u32, &'static [u32], u64, bool, bool, u64);
const TEST_KNN_LIMITS: KnnBatchLimits = KnnBatchLimits {
    max_source_points: 20_000_000,
    max_query_points: 20_000_000,
    max_neighbors: 1,
    max_logical_peak_bytes: 2 * 1024 * 1024 * 1024,
    max_work: 100_000_000,
};

fn hollowed_overhang_table() -> HollowedModel {
    let mesh = make_overhang_table().expect("procedural overhang table");
    let budget = DecimationBudget::ratified_default(ProbeConfig::new(1.0, usize::MAX, 10_000.0))
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

fn support_input<'a>(
    delicacy_by_face: &'a [f64],
    layers: &'a LayerPlan,
) -> SupportPreparationInput<'a> {
    SupportPreparationInput {
        layers,
        delicacy_by_face,
        column_pitch_mm: PositiveFinite::new(1.0).expect("positive column pitch"),
        surface_grid_pitch_mm: PositiveFinite::new(1.0).expect("positive grid pitch"),
        max_grid_bytes: GRID_BYTES,
        candidate_config: CandidatePayloadConfig::current(
            8,
            1_000_000,
            3_000_000,
            256 * 1024 * 1024,
            65_536,
        )
        .expect("candidate payload configuration"),
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
        .expect("contact construction limits"),
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
    }
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
