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

// ---------------------------------------------------------------------------
// The two batch-product-scaled specialized caps (FU-8b, 2026-08-06).
//
// These scale on the *batch* the stage was handed, not on the face count, so
// their shapes are stated as measured `(rows, columns)` pairs rather than
// derived from a mesh. Every number below is either a measurement of record or
// a derivation from one; none is a copied bound.
// ---------------------------------------------------------------------------

/// Gate batch shapes `(coverage_samples, contacts)` measured on the seven
/// reachable corpus models, 2026-08-06, instrumented release run.
const CORPUS_GATE_BATCHES: [(&str, usize, usize); 7] = [
    ("Sphere", 9_184, 7),
    ("Cylinder", 509_056, 54),
    ("make_box", 12_288, 11),
    ("make_overhang_table", 50_176, 84),
    ("make_toed_slab", 50_688, 73),
    ("make_raking_blade", 35_008, 62),
    ("make_thin_fin", 20_480, 18),
];

/// Load batch shapes `(rows, contacts)` from the same run — **not** the
/// census's `T0_scaled_cap` rows, which answer a different question.
///
/// Provenance corrected 2026-08-06 (record fix F11). This docstring used to say
/// only "from the same run", which reads as "the census row" and is wrong for
/// three of the seven models. Measured at this tip, by instrumenting
/// `census::record_scaled_cap` to print every `specialized_load_operations`
/// checkpoint each model records and segmenting the stream on `census::reset`:
///
/// | model | checkpoints | first — what the census prints | widest | pinned here |
/// |---|---|---|---|---|
/// | `Sphere`              |    5 | (574, 7)  | (574, 7)  | (574, 7)  |
/// | `Cylinder`            |    5 | (61, 54)  | (66, 54)  | (66, 54)  |
/// | `make_box`            |    5 | (5, 11)   | (5, 11)   | (5, 11)   |
/// | `make_overhang_table` |    5 | (10, 84)  | (12, 84)  | (12, 84)  |
/// | `make_toed_slab`      |    5 | (12, 73)  | (12, 73)  | (12, 73)  |
/// | `make_raking_blade`   | 2851 | (13, 62)  | (13, 62)  | (13, 62)  |
/// | `make_thin_fin`       |    5 | (3, 18)   | (3, 18)   | (6, 18)   |
///
/// Every model plans its load stage several times per run, and
/// `record_scaled_cap` keeps exactly one row per cap kind: the one with the
/// **widest materialization**, since that is the bound that decides whether the
/// model is refused. Every corpus model materializes this cap **at the floor**,
/// 2,000,000,000, at every checkpoint, so all of them tie — and on a tie
/// `record_scaled_cap` keeps the incumbent, i.e. the **first** checkpoint
/// rather than the widest. That, and only that, is why the census prints 61 for
/// the Cylinder and 10 for `make_overhang_table` where this table pins 66 and
/// 12. Both are real checkpoints of the same run; neither number is wrong.
///
/// The widest shape is the right one to pin here, because these constants exist
/// to assert that no corpus batch is wide enough to lift the cap off its floor,
/// and that claim has to be tested against the widest batch rather than the
/// earliest one — which is what
/// `the_batch_scaled_caps_are_the_flat_floor_for_every_corpus_batch_shape`
/// does. If a future model ever leaves the floor, the census row and this table
/// converge, because the tie that separates them is gone.
///
/// **`make_thin_fin` is the exception, and is NOT explained by the tie.** Its
/// pinned 6 rows matches neither the census's 3 nor any of the five load
/// checkpoints the instrumented run observed, all of which plan 3 rows. The
/// pinned 18 contacts is right. 6 is conservative for what this table asserts —
/// a wider batch than the model plans still materializes at the floor, so
/// `the_batch_scaled_caps_are_the_flat_floor_for_every_corpus_batch_shape`
/// passes either way and nothing downstream reads it — so it is left standing
/// rather than silently re-pinned, and recorded here as unreconciled. Re-pin it
/// to 3 only together with a run that states where the 6 came from.
const CORPUS_LOAD_BATCHES: [(&str, usize, usize); 7] = [
    ("Sphere", 574, 7),
    ("Cylinder", 66, 54),
    ("make_box", 5, 11),
    ("make_overhang_table", 12, 84),
    ("make_toed_slab", 12, 73),
    ("make_raking_blade", 13, 62),
    ("make_thin_fin", 6, 18),
];

/// The Weeping Treant's proposal-free gate batch and its stated demand.
const TREANT_GATE_SAMPLES: usize = 405_146;
const TREANT_CONTACTS: usize = 1_366;
const TREANT_GATE_OPERATIONS_REQUIRED: usize = 12_714_828_544;
/// Its load partition and stated demand, after FU-8b fix (B) stopped the row
/// *prediction* counting `(owner, face)` pairs: the plan predicted 7,988,078
/// rows and now states the 130,548 the builder had always produced — 16,627
/// generic rows plus the 113,921 distinct faces of 7,971,451 pairs.
///
/// Re-pinned 2026-08-06 by record fix F1, from 134,368 to 130,548. The demand
/// below is unchanged and was never in doubt; the row count paired with it
/// was. One scratch-instrumented treant run printed `resource.rows = 130548`
/// beside `resource.operations = 5402535619` at
/// `finalize_specialized_load_partition`, and the census scaled-cap row prints
/// the same `rows=130548` with `materialized=11421470912`. No 134,368-row load
/// batch is planned anywhere in the corpus; had one been, it would have
/// materialized 11,755,675,072 and taken the census's widest-per-kind slot.
///
/// **Dated addendum 2026-08-07 — FU-8f phase G5. Every sentence above is left
/// standing and the demand below has since moved, downward, by 4,607,175,844.**
/// G5 repriced `load_resource_plan_from_counts`'s scan term from
/// `rows x contacts x (owner_log + 4)` — the model of a dense `contacts x rows`
/// scan that phase G1 had already replaced with a CSR transpose — to the
/// transpose's own `Σ |eligible_owners|`-shaped search term plus a
/// `cells`-shaped fill-and-scan term. On this same 130,548-row batch,
/// re-measured on a **quiet** machine at the G4 tip plus G5, 2026-08-07:
/// **5,402,535,619 → 795,359,775**, a 6.79x fall.
///
/// **`TREANT_LOAD_OPERATIONS_REQUIRED` is deliberately NOT re-pinned to the new
/// figure**, and the reason is a cap, not an oversight. This constant is the
/// *derivation input* of the load batch-product coefficient:
/// `the_batch_product_coefficients_are_the_rule_applied_to_their_own_observations`
/// asserts `next_power_of_two(2 * ceil(demand / units)) == 64`, and the
/// repriced demand divides to 5 rather than 31, which would re-derive that
/// coefficient to **8** — that is, **lower a cap by 8x**. G5's mandate is that
/// no cap is raised, lowered or touched, so the observation of record for that
/// derivation stays the 2026-08-06 one, dated and attributed here. Re-deriving
/// it is a separate decision under FU-23/R4, wants a second specimen rather
/// than the same specimen getting cheaper, and would be a cap *reduction* — the
/// direction that can create refusals.
const TREANT_LOAD_ROWS: usize = 130_548;
const TREANT_LOAD_OPERATIONS_REQUIRED: usize = 5_402_535_619;
/// The same batch's demand under FU-8f phase G5's repriced model, measured
/// 2026-08-07 on a quiet machine, one instrumented release run, the
/// instrumentation reverted by editing before the gates.
///
/// Pinned **beside** its predecessor rather than replacing it, because the two
/// answer different questions: the one above is what the coefficient was
/// derived from, this one is what the treant now asks for. What it is here to
/// assert is a **finding**, and
/// `the_treant_batch_scaled_caps_admit_its_measured_demands_with_headroom`
/// states it: the repriced demand no longer exceeds the flat floor, so the
/// treant no longer needs the batch-product scaling at the load stage at all.
/// The scaling is kept regardless, because removing it would be a cap change.
const TREANT_LOAD_OPERATIONS_REPRICED: usize = 795_359_775;
/// Its 39,223-proposal merge gate, on the *same* batch shape: the gate
/// re-evaluates every contact once per proposal variant, so the plan is
/// quadratic in the batch. FU-8c; this cap must keep refusing it.
const TREANT_MERGE_GATE_OPERATIONS_REQUIRED: usize = 21_755_124_103_146;

fn specialized_production_limits() -> plinth_support::specialized::SpecializedLimits {
    plinth_support::specialized::SpecializedLimits::production(
        KnnBatchLimits {
            max_source_points: 1,
            max_query_points: 1,
            max_neighbors: 1,
            max_logical_peak_bytes: 1,
            max_work: 1,
        },
        plinth_support::specialized::SpecializedBatchWork::registered()
            .expect("registered support batch coefficients"),
    )
}

fn batch_floors_and_coefficients() -> (usize, usize, usize, usize) {
    let policy = m0_constants()
        .expect("M0 constants registry must construct")
        .support_resource_cap_policy();
    (
        usize::try_from(policy.gate_max_operations_floor().diagnostic().get()).unwrap(),
        usize::try_from(*policy.gate_max_work_per_batch_unit().value()).unwrap(),
        usize::try_from(policy.load_max_operations_floor().diagnostic().get()).unwrap(),
        usize::try_from(*policy.load_max_work_per_batch_unit().value()).unwrap(),
    )
}

#[test]
fn the_batch_product_coefficients_are_the_rule_applied_to_their_own_observations() {
    let (_, gate_per_unit, _, load_per_unit) = batch_floors_and_coefficients();
    let gate_units = (TREANT_GATE_SAMPLES + 1) * (TREANT_CONTACTS + 1);
    let gate_observed = TREANT_GATE_OPERATIONS_REQUIRED.div_ceil(gate_units);
    assert_eq!(
        gate_per_unit,
        (2 * gate_observed as u64).next_power_of_two() as usize,
        "the gate coefficient must stay next_power_of_two(2 * observed)"
    );
    let load_units = (TREANT_LOAD_ROWS + 1) * (TREANT_CONTACTS + 1);
    let load_observed = TREANT_LOAD_OPERATIONS_REQUIRED.div_ceil(load_units);
    assert_eq!(
        load_per_unit,
        (2 * load_observed as u64).next_power_of_two() as usize,
        "the load coefficient must stay next_power_of_two(2 * observed)"
    );
}

#[test]
fn the_batch_scaled_caps_are_the_flat_floor_for_every_corpus_batch_shape() {
    let limits = specialized_production_limits();
    let (gate_floor, _, load_floor, _) = batch_floors_and_coefficients();
    for (name, samples, contacts) in CORPUS_GATE_BATCHES {
        assert_eq!(
            limits.gate_operation_limit(samples, contacts),
            gate_floor,
            "{name}: the batch rule must leave the corpus gate cap bit-identical to the floor"
        );
    }
    for (name, rows, contacts) in CORPUS_LOAD_BATCHES {
        assert_eq!(
            limits.load_operation_limit(rows, contacts),
            load_floor,
            "{name}: the batch rule must leave the corpus load cap bit-identical to the floor"
        );
    }
}

#[test]
fn the_treant_batch_scaled_caps_admit_its_measured_demands_with_headroom() {
    let limits = specialized_production_limits();
    let (gate_floor, gate_per_unit, load_floor, load_per_unit) = batch_floors_and_coefficients();

    let gate_cap = limits.gate_operation_limit(TREANT_GATE_SAMPLES, TREANT_CONTACTS);
    assert_eq!(
        gate_cap,
        gate_per_unit * (TREANT_GATE_SAMPLES + 1) * (TREANT_CONTACTS + 1)
    );
    assert!(
        gate_cap > gate_floor,
        "the treant must leave the gate floor"
    );
    assert!(
        TREANT_GATE_OPERATIONS_REQUIRED <= gate_cap,
        "the measured proposal-free gate demand must be admitted"
    );
    assert!(
        TREANT_GATE_OPERATIONS_REQUIRED > gate_floor,
        "and it must be a demand the flat floor really did refuse"
    );

    let load_cap = limits.load_operation_limit(TREANT_LOAD_ROWS, TREANT_CONTACTS);
    assert_eq!(
        load_cap,
        load_per_unit * (TREANT_LOAD_ROWS + 1) * (TREANT_CONTACTS + 1)
    );
    assert!(
        load_cap > load_floor,
        "the treant must leave the load floor"
    );
    assert!(
        TREANT_LOAD_OPERATIONS_REQUIRED <= load_cap,
        "the measured load demand must be admitted"
    );
    assert!(
        TREANT_LOAD_OPERATIONS_REQUIRED > load_floor,
        "and it must be a demand the flat floor really did refuse"
    );

    // FU-8f phase G5 (2026-08-07). The demand above is the 2026-08-06
    // observation the coefficient was derived from; the repriced model states
    // 795,359,775 for the same batch. Two things are asserted about it, and the
    // second is the finding.
    const {
        assert!(
            TREANT_LOAD_OPERATIONS_REPRICED < TREANT_LOAD_OPERATIONS_REQUIRED,
            "the repricing must move the demand DOWNWARD; a rise would be a cap \
             question and G5 raises, lowers and touches no cap"
        );
    }
    assert!(
        TREANT_LOAD_OPERATIONS_REPRICED <= load_cap,
        "the repriced demand must still be admitted"
    );
    assert!(
        TREANT_LOAD_OPERATIONS_REPRICED < load_floor,
        "FINDING: after G5 the treant's load demand no longer exceeds the FLAT \
         floor, so the batch-product scaling this cap adds is no longer load- \
         bearing for the only model that ever needed it. Kept anyway — removing \
         it would be a cap change, which this phase does not make."
    );
}

#[test]
fn the_batch_scaled_gate_cap_still_refuses_the_proposal_quadratic() {
    let limits = specialized_production_limits();
    // Same batch shape as the admitted one above: only the proposal count
    // differs, and the proposal count is deliberately not part of the scale.
    let gate_cap = limits.gate_operation_limit(TREANT_GATE_SAMPLES, TREANT_CONTACTS);
    assert!(
        TREANT_MERGE_GATE_OPERATIONS_REQUIRED > gate_cap,
        "a gate that re-evaluates the whole batch once per merge proposal is \
         superlinear in the batch and must stay refused (FU-8c)"
    );
    // Not marginally: the refused plan is three orders of magnitude over.
    assert!(TREANT_MERGE_GATE_OPERATIONS_REQUIRED / gate_cap > 500);
}

#[test]
fn a_batch_scaled_cap_never_becomes_unbounded_and_never_wraps() {
    let limits = specialized_production_limits();
    let (gate_floor, _, load_floor, _) = batch_floors_and_coefficients();
    // An empty batch is the floor, never zero: a zero cap would refuse every
    // batch including the empty one.
    assert_eq!(limits.gate_operation_limit(0, 0), gate_floor);
    assert_eq!(limits.load_operation_limit(0, 0), load_floor);
    // An absurd batch saturates rather than wrapping to something tiny.
    assert!(limits.gate_operation_limit(usize::MAX, usize::MAX) >= gate_floor);
    assert!(limits.load_operation_limit(usize::MAX, usize::MAX) >= load_floor);
}
