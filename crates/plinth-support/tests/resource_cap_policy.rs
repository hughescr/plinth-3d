//! The support pipeline's two model-scaled execution caps.
//!
//! Each cap gets two tests: one pinning the bound the policy produces, and one
//! proving the typed refusal still fires one unit past it. The policy changes
//! the bound; it must not soften the refusal, and it must never convert a
//! refusal into an unbounded run.
//!
//! Every expected number here is written as its derivation
//! (`floor.max(per_face * faces)`, `next_power_of_two(2 * observed)`), not as a
//! copied literal, so a constant that moves in the registry without its rule
//! moving is a failure rather than a silent re-pin.

use geom_kernel::{
    KnnBatchLimits, KnnBatchPlan, KnnBatchPlanError, NativeSurface, NativeSurfaceConfig,
    SurfaceBuildError,
};
use plinth_core::{m0_constants, LayerPlan};
use plinth_support::SupportPreparationLimits;

/// The Weeping Treant v1.2 support mesh, as the census reports it.
const TREANT_FACES: usize = 429_120;
/// Its measured minima-kNN work: `429_120 * 696`, the pre-policy refusal's own
/// `required` figure.
const TREANT_KNN_WORK_REQUIRED: usize = 298_667_520;
/// Its measured parity-grid projected peak: `81 * 429_120 + 36 * 1_214_544 + 4`.
const TREANT_GRID_BYTES_REQUIRED: usize = 78_482_308;
/// The largest model the corpus can currently run end to end.
const LARGEST_REACHABLE_CORPUS_FACES: usize = 4_760;

fn floors_and_coefficients() -> (usize, usize, usize, usize) {
    let policy = m0_constants()
        .expect("M0 constants registry must construct")
        .support_resource_cap_policy();
    (
        usize::try_from(policy.knn_max_work_floor().diagnostic().get()).unwrap(),
        usize::try_from(*policy.knn_max_work_per_face().value()).unwrap(),
        usize::try_from(policy.parity_grid_max_bytes_floor().diagnostic().get()).unwrap(),
        usize::try_from(*policy.parity_grid_max_bytes_per_face().value()).unwrap(),
    )
}

fn caps_for(face_count: usize) -> (usize, usize) {
    let layers = LayerPlan::new(0.0, vec![0.05; 8]).expect("layer plan");
    let delicacy = vec![0.0_f64; face_count];
    let input = SupportPreparationLimits::production()
        .expect("production limits")
        .input(&layers, &delicacy, &[]);
    (input.minima_knn_max_work(), input.max_grid_bytes())
}

fn coverage_route_knn_cap_for(face_count: usize) -> usize {
    let layers = LayerPlan::new(0.0, vec![0.05; 8]).expect("layer plan");
    let delicacy = vec![0.0_f64; face_count];
    SupportPreparationLimits::production()
        .expect("production limits")
        .input(&layers, &delicacy, &[])
        .coverage_route_knn_max_work()
}

/// The scaled family has exactly two members, and this is the fence.
///
/// The specialized coverage-route kNN batch shares `KnnBatchLimits` with the
/// minima batch, so scaling it costs one line and discloses nothing. It is not
/// scaled: `SUPPORT_KNN_WORK_PER_FACE_OBSERVED` = 696 was measured on the
/// minima-to-face batch, and the coverage-route batch has no measurement of
/// record. It stays at the flat floor for every model, treant included, until
/// one exists.
#[test]
fn the_coverage_route_knn_cap_is_the_flat_floor_at_every_face_count() {
    let (knn_floor, knn_per_face, _, _) = floors_and_coefficients();
    for faces in [
        0,
        1,
        12,
        24,
        LARGEST_REACHABLE_CORPUS_FACES,
        32_768,
        TREANT_FACES,
        100_000_000,
    ] {
        assert_eq!(
            coverage_route_knn_cap_for(faces),
            knn_floor,
            "the coverage-route kNN cap must stay at its unscaled floor at {faces} faces"
        );
    }

    // And it really is unscaled, not coincidentally equal: at the treant's face
    // count the scaled rule would have produced a cap 8.8x larger.
    assert!(knn_per_face * TREANT_FACES > 8 * knn_floor);
}

#[test]
fn both_scaled_caps_are_exactly_floor_max_per_face_times_faces() {
    let (knn_floor, knn_per_face, grid_floor, grid_per_face) = floors_and_coefficients();

    // The rule that produced the two coefficients, restated as arithmetic.
    assert_eq!(knn_per_face, (2 * 696_usize).next_power_of_two());
    assert_eq!(grid_per_face, (2 * 183_usize).next_power_of_two());

    for faces in [
        0,
        1,
        12,
        24,
        LARGEST_REACHABLE_CORPUS_FACES,
        32_768,
        TREANT_FACES,
    ] {
        let (knn, grid) = caps_for(faces);
        assert_eq!(
            knn,
            knn_floor.max(knn_per_face * faces),
            "kNN work cap drifted from the rule at {faces} faces"
        );
        assert_eq!(
            grid,
            grid_floor.max(grid_per_face * faces),
            "parity grid byte cap drifted from the rule at {faces} faces"
        );
    }
}

#[test]
fn every_reachable_corpus_model_keeps_the_pre_policy_caps_unchanged() {
    // OD-8 in a test: the floor governs for every model the corpus can run, so
    // the policy cannot move a single corpus digest. Sphere is the largest at
    // 4,760 faces and still needs only 2,437,120 grid bytes / 9,748,480 kNN
    // work under the coefficients — both far under their floors.
    let (knn_floor, knn_per_face, grid_floor, grid_per_face) = floors_and_coefficients();
    for faces in [12, 24, 60, 140, 244, LARGEST_REACHABLE_CORPUS_FACES] {
        assert!(knn_per_face * faces < knn_floor);
        assert!(grid_per_face * faces < grid_floor);
        assert_eq!(caps_for(faces), (knn_floor, grid_floor));
    }
}

#[test]
fn the_treant_caps_admit_its_measured_requirement_with_headroom() {
    let (_, knn_per_face, _, grid_per_face) = floors_and_coefficients();
    let (knn, grid) = caps_for(TREANT_FACES);

    assert_eq!(knn, knn_per_face * TREANT_FACES);
    assert_eq!(grid, grid_per_face * TREANT_FACES);
    assert!(
        knn > TREANT_KNN_WORK_REQUIRED,
        "the kNN cap must admit the measured {TREANT_KNN_WORK_REQUIRED}, got {knn}"
    );
    assert!(
        grid > TREANT_GRID_BYTES_REQUIRED,
        "the grid cap must admit the measured {TREANT_GRID_BYTES_REQUIRED}, got {grid}"
    );

    // The bound stays a bound. All-pairs kNN over this mesh is 1.84e11 work and
    // is still refused, by more than two orders of magnitude.
    assert!(TREANT_FACES * TREANT_FACES > 200 * knn);
    // And the grid cap is memory-denominated and small: 209 MiB for a 429k-face
    // mesh, three orders of magnitude under the 100 GB machine ceiling.
    assert!(grid < 256 * 1024 * 1024);
}

#[test]
fn knn_work_refusal_still_fires_one_unit_past_the_scaled_bound() {
    let (knn_cap, _) = caps_for(TREANT_FACES);
    let limits = KnnBatchLimits {
        max_source_points: 20_000_000,
        max_query_points: 20_000_000,
        max_neighbors: 1,
        max_logical_peak_bytes: 2 * 1024 * 1024 * 1024,
        max_work: knn_cap,
    };
    let per_face = knn_cap / TREANT_FACES;
    assert_eq!(per_face * TREANT_FACES, knn_cap, "cap must factor exactly");

    // Exactly at the bound: admitted, and the plan reports the bound as work.
    let at = KnnBatchPlan::new(TREANT_FACES, per_face, 1, limits).expect("work == cap is admitted");
    assert_eq!(at.work(), knn_cap);

    // One query past the bound: refused, typed, with both figures reported.
    assert_eq!(
        KnnBatchPlan::new(TREANT_FACES, per_face + 1, 1, limits),
        Err(KnnBatchPlanError::WorkLimit {
            required: TREANT_FACES * (per_face + 1),
            limit: knn_cap,
        })
    );

    // An all-pairs batch over the same mesh is refused too, so raising the cap
    // to admit the treant did not admit the quadratic the cap exists to catch.
    assert_eq!(
        KnnBatchPlan::new(TREANT_FACES, TREANT_FACES, 1, limits),
        Err(KnnBatchPlanError::WorkLimit {
            required: TREANT_FACES * TREANT_FACES,
            limit: knn_cap,
        })
    );
}

/// A closed unit tetrahedron: enough geometry for one real parity axis grid.
fn tetra() -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
    (
        vec![
            [0.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [0.0, 4.0, 0.0],
            [0.0, 0.0, 4.0],
        ],
        vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
    )
}

#[test]
fn parity_grid_byte_refusal_still_fires_one_byte_past_the_bound() {
    let (verts, tris) = tetra();
    let projected = match NativeSurface::from_indexed(
        &verts,
        &tris,
        NativeSurfaceConfig {
            grid_pitch_mm: 1.0,
            max_grid_bytes: 1,
        },
    ) {
        Err(SurfaceBuildError::GridBudget {
            projected_bytes, ..
        }) => projected_bytes,
        Err(other) => panic!("a 1-byte budget must refuse with GridBudget, got {other}"),
        Ok(_) => panic!("a 1-byte grid budget must never be admitted"),
    };

    // The exact projection this policy's coefficient was derived through:
    // 81 * faces + 36 * references + 4 (geom_kernel parity axis_memory_bounds).
    // Inverting it here proves the derivation reads the same formula the engine
    // charges against.
    let references = (projected - 81 * tris.len() - 4) / 36;
    assert_eq!(81 * tris.len() + 36 * references + 4, projected);

    // Exactly at the projection: admitted.
    NativeSurface::from_indexed(
        &verts,
        &tris,
        NativeSurfaceConfig {
            grid_pitch_mm: 1.0,
            max_grid_bytes: projected,
        },
    )
    .expect("projected == budget is admitted");

    // One byte short: refused, typed, reporting both figures. The refusal is a
    // clean typed error, never an OOM or an unbounded build.
    assert!(matches!(
        NativeSurface::from_indexed(
            &verts,
            &tris,
            NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: projected - 1,
            },
        ),
        Err(SurfaceBuildError::GridBudget {
            projected_bytes,
            limit_bytes,
            ..
        }) if projected_bytes == projected && limit_bytes == projected - 1
    ));
}

#[test]
fn a_scaled_cap_never_becomes_unbounded_and_never_wraps() {
    // usize::MAX is refused by SupportPreparationInput::validate, so the
    // saturating multiply must not be the thing that produces it in practice,
    // and it must never wrap to a smaller cap than the floor.
    let (knn_floor, _, grid_floor, _) = floors_and_coefficients();
    for faces in [0, 1, TREANT_FACES, 100_000_000] {
        let (knn, grid) = caps_for(faces);
        assert!(knn >= knn_floor && knn != usize::MAX);
        assert!(grid >= grid_floor && grid != usize::MAX);
    }
}
