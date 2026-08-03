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
    ContactConstructionLimits, SupportPreparationInput, SupportRequest,
};

const GRID_BYTES: usize = 64 * 1024 * 1024;
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
        specialized_limits: SpecializedLimits::production(),
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
