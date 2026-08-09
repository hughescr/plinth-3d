//! FU-8g H5 — the census-vs-execution equivalence proof and its non-vacuity
//! (design: `docs/rust-migration/fu8g-design-2026-08-08.md`, H5 row).
//!
//! The whole correctness claim of the FU-8g repricing (H1/H2) is that the
//! counting walk and the filling walk are THE SAME traversal — same visit
//! order, same predicates, same intervals — so executing under the counted
//! (census-derived) budgets changes NOTHING about the geometry the batch
//! returns. This harness states that claim as executable assertions over
//! real fixture geometry and a synthetic dense-crossing fixture:
//!
//! 1. `penetration_intervals_batch_bounded` under census-derived limits is
//!    `to_bits`-identical, field by field (`s_a`, `s_b`, `entered`, `exited`,
//!    `max_depth`, `ray`), to the same call under worst-case-derived limits.
//! 2. For EVERY row, the census's `crossings` equals the hits the fill walk
//!    actually pushed and its `tests` equals the traversal effort the fill
//!    walk actually executed (proven per row through single-row metered
//!    calls, and for the batch through the aggregate meter).
//! 3. Every census-derived limit is admitted by the worst-case-derived limit
//!    for the same batch shape (the census never out-charges the ceiling).
//! 4. An over-demand batch is refused by the TYPED preflight
//!    (`ResourceLimitExceeded`) — never by an allocation failure and never by
//!    a mid-execution drift refusal — i.e. the refusal happens before any row
//!    executes. The caps-level sibling of this witness, at the UNCHANGED
//!    `ContactConstructionLimits::production()` values, is
//!    `initial::tests::honest_over_demand_still_refuses_at_the_unchanged_production_caps`
//!    (landed in H2); this file proves the same discipline at the kernel
//!    boundary, resource by resource.
//!
//! Non-vacuity is measured, not argued: the dense fixture genuinely
//! exercises more than 100 crossings on one ray (the treant's tail regime,
//! design §M2), the overhang-table fixture has a genuine hole (two disjoint
//! penetration intervals on one row), and every batch is built with row 0 as
//! strictly the least-demanding row — asserted in the harness — so the
//! budget-broadcast mutation (triad (c) below) cannot be vacuously green.
//!
//! **Mutation triad (house standard), each applied to
//! `rust/geom_kernel/src/queries.rs`, observed failing exactly as recorded
//! below, and reverted (2026-08-09):**
//!
//! (a) **Census under-counts one row by 1** — `census_one` given
//!     `census.max_ray_crossings = census.max_ray_crossings.saturating_sub(1);`
//!     immediately before its return. All four tests in this file fail with
//!     `CensusDrift { row: 1, resource: "hits", counted, executed }` where
//!     `executed == counted + 1` — raking blade `counted: 1, executed: 2`;
//!     overhang table `counted: 7, executed: 8`; dense stack and the
//!     refusal test `counted: 119, executed: 120`. The refusal fires at the
//!     fill walk's own budget check, one row and one buffer at a time,
//!     exactly as the `CensusDrift` doc comment promises. geom_kernel's own
//!     suite also catches this mutation broadly (11 of `native_api`'s 32
//!     tests fail, `penetration_census_severs_work_from_hits_and_prices_
//!     the_counted_row` and `penetration_meter_receipt_matches_the_counted_
//!     charge` among them).
//!
//! (b) **The F10 closed-form sample bound drops its `3 x` term** —
//!     `census_row_sample_budget`'s
//!     `checked_penetration_mul(subsegments, 3, ..)` arm replaced by plain
//!     `subsegments`. The dense-stack harness fails with
//!     `CensusDrift { row: 1, resource: "samples", counted: 140, executed: 365 }`
//!     — the 60-slab column's axis ray has 121 subsegments of 0.01 mm each,
//!     so the `max(3, ..)` per-subsegment floor binds (121 x 3 + 2 extra on
//!     the long end runs = 365 executed) while the mutated bound only funds
//!     `subsegments + ceil(seg_mm / step)` = 121 + 19 = 140 — and
//!     geom_kernel's `penetration_census_sample_budget_is_the_closed_form_
//!     bound` fails its pinned equality at `left: 30, right: 40`
//!     (`rust/geom_kernel/tests/native_api.rs:894`). The raking-blade and
//!     overhang-table tests stay GREEN under this mutation — their long
//!     subsegments are funded by the step term alone — which is precisely
//!     why the H5 row requires the dense fixture: without it the triad's
//!     (b) arm would be caught by only the one numeric pin.
//!
//! (c) **The per-row budget is replaced by row 0's budget for all rows** —
//!     in `penetration_requirements_from_census`, after the derivation loop:
//!     `if let Some(&first) = row_budgets.first() { for budget in &mut
//!     row_budgets { *budget = first; } }`. All four tests in this file
//!     fail with `CensusDrift { row: 1, resource: "hits", counted: 0,
//!     executed: .. }` (executed 2 / 8 / 120 / 120 by fixture) — row 0 is
//!     the least-demanding row by construction, so its broadcast budget
//!     cannot fund any later row's real crossings. geom_kernel's
//!     `native_api` suite passes 32 of 32 under this mutation: THIS HARNESS
//!     IS THE ONLY WITNESS, which is exactly the vacuity the design's H5
//!     row exists to rule out.

use geom_kernel::{
    Interval, NativeSurface, NativeSurfaceConfig, PenetrationBatchError, PenetrationBatchLimits,
    PenetrationRowCensus,
};
use plinth_mesh::{make_overhang_table, make_raking_blade, Mesh};

fn surface_of(mesh: &Mesh) -> NativeSurface {
    NativeSurface::from_indexed(
        mesh.verts(),
        mesh.tris(),
        NativeSurfaceConfig {
            grid_pitch_mm: 1.0,
            max_grid_bytes: 64 * 1024 * 1024,
        },
    )
    .expect("fixture surface")
}

/// A stack of `slabs` thin closed boxes sharing one 1 mm x 1 mm footprint,
/// 0.01 mm thick on a 0.02 mm pitch. A ray down the column crosses the top
/// and bottom of every slab, so one ray's crossing count is `2 x slabs` —
/// the synthetic dense-crossing fixture the design's H5 row requires for
/// non-vacuity in the treant's tail regime (§M2: real rays with hundreds of
/// crossings must not be averaged away).
fn dense_lamella_stack(slabs: usize) -> NativeSurface {
    let mut vertices = Vec::with_capacity(slabs * 8);
    let mut faces = Vec::with_capacity(slabs * 12);
    for slab in 0..slabs {
        let z0 = slab as f64 * 0.02;
        let z1 = z0 + 0.01;
        let base = u32::try_from(vertices.len()).expect("fixture vertex count fits u32");
        for z in [z0, z1] {
            for (x, y) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
                vertices.push([x, y, z]);
            }
        }
        for corners in [
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ] {
            faces.push(corners.map(|corner| base + corner));
        }
    }
    NativeSurface::from_indexed(
        &vertices,
        &faces,
        NativeSurfaceConfig {
            grid_pitch_mm: 1.0,
            max_grid_bytes: 64 * 1024 * 1024,
        },
    )
    .expect("dense stack surface")
}

/// One batch of capsule rows. Row 0 MUST be the least-demanding row (the
/// harness asserts it): mutation (c) — broadcasting row 0's budget over the
/// batch — is only catchable when some later row genuinely demands more.
#[derive(Default)]
struct Rows {
    p0s: Vec<[f64; 3]>,
    p1s: Vec<[f64; 3]>,
    r0s: Vec<f64>,
    r1s: Vec<f64>,
}

impl Rows {
    fn push(&mut self, p0: [f64; 3], p1: [f64; 3], r0: f64, r1: f64) {
        self.p0s.push(p0);
        self.p1s.push(p1);
        self.r0s.push(r0);
        self.r1s.push(r1);
    }
}

/// The batch shape shared by both sides of the comparison. The values mirror
/// the production seed loop's shape (`n_offset_rays = 8`) and the geom_kernel
/// test suite's sampling constants.
#[derive(Clone, Copy)]
struct Shape {
    n_offset_rays: usize,
    majority_step_mm: f64,
    cap: usize,
    min_illegal_len_mm: f64,
    rays_all: bool,
}

const PRODUCTION_SHAPE: Shape = Shape {
    n_offset_rays: 8,
    majority_step_mm: 0.12,
    cap: 8,
    min_illegal_len_mm: 0.05,
    rays_all: false,
};

const ALL_RAYS_SHAPE: Shape = Shape {
    rays_all: true,
    ..PRODUCTION_SHAPE
};

/// The H5 equivalence harness. For one surface and one batch it proves, in
/// order: the census-derived limits are admitted by the worst-case-derived
/// limits; execution under census limits equals execution under worst-case
/// limits `to_bits`, field by field; the batch receipt equals the census's
/// counted `work`/`hits` exactly (the two walks measure the same quantity);
/// and every row's counted `crossings`/`tests` equal that row's executed
/// `hits`/`work` (single-row metered calls, which must also reproduce the
/// batch row's intervals bit for bit).
///
/// Returns the census and the intervals so fixture tests can add their own
/// non-vacuity assertions on top.
fn assert_census_matches_execution(
    surface: &NativeSurface,
    rows: &Rows,
    shape: Shape,
) -> (Vec<PenetrationRowCensus>, Vec<Vec<Interval>>) {
    let census = surface
        .penetration_census_batch(
            &rows.p0s,
            &rows.p1s,
            &rows.r0s,
            &rows.r1s,
            shape.n_offset_rays,
            shape.rays_all,
        )
        .expect("census pass");
    // Non-vacuity for mutation (c): row 0 must be strictly the
    // least-demanding row, or broadcasting its budget could be a no-op.
    let worst_row_crossings = census
        .iter()
        .map(|row| row.max_ray_crossings)
        .max()
        .expect("non-empty batch");
    assert!(
        census[0].max_ray_crossings < worst_row_crossings,
        "harness precondition: row 0 must demand strictly less than the worst row \
         (row 0: {}, worst: {worst_row_crossings})",
        census[0].max_ray_crossings,
    );

    let census_limits = surface
        .penetration_intervals_batch_census_limits(
            &census,
            shape.n_offset_rays,
            shape.cap,
            shape.majority_step_mm,
            shape.rays_all,
        )
        .expect("census limits");
    let worst_limits = surface
        .penetration_intervals_batch_worst_case_limits(
            rows.p0s.len(),
            shape.n_offset_rays,
            shape.cap,
            shape.rays_all,
        )
        .expect("worst-case limits");
    // The counted charge never exceeds the conservative ceiling: a caller
    // holding a worst-case-derived limit always admits the counted demand.
    for (resource, counted, ceiling) in [
        ("rows", census_limits.max_rows, worst_limits.max_rows),
        (
            "work",
            census_limits.max_total_work,
            worst_limits.max_total_work,
        ),
        (
            "hits",
            census_limits.max_total_hits,
            worst_limits.max_total_hits,
        ),
        (
            "samples",
            census_limits.max_total_samples,
            worst_limits.max_total_samples,
        ),
        (
            "intervals",
            census_limits.max_total_intervals,
            worst_limits.max_total_intervals,
        ),
        (
            "scratch bytes",
            census_limits.max_scratch_bytes,
            worst_limits.max_scratch_bytes,
        ),
    ] {
        assert!(
            counted <= ceiling,
            "counted {resource} charge {counted} exceeds the worst-case ceiling {ceiling}"
        );
    }

    let (under_census, meter) = surface
        .penetration_intervals_batch_bounded_metered(
            &rows.p0s,
            &rows.p1s,
            &rows.r0s,
            &rows.r1s,
            shape.n_offset_rays,
            shape.majority_step_mm,
            shape.cap,
            shape.min_illegal_len_mm,
            shape.rays_all,
            census_limits,
        )
        .expect("execution under the counted budgets");
    let under_worst_case = surface
        .penetration_intervals_batch_bounded(
            &rows.p0s,
            &rows.p1s,
            &rows.r0s,
            &rows.r1s,
            shape.n_offset_rays,
            shape.majority_step_mm,
            shape.cap,
            shape.min_illegal_len_mm,
            shape.rays_all,
            worst_limits,
        )
        .expect("execution under the worst-case ceiling");
    assert_interval_rows_bit_identical(&under_census, &under_worst_case, "worst-case limits");

    // The batch receipt: `work` and `hits` are the SAME quantities the census
    // counted, measured by the other walk, so they are EQUAL — not merely
    // bounded. `samples` and `intervals` are bounded quantities.
    let counted_tests: usize = census.iter().map(|row| row.tests).sum();
    let counted_crossings: usize = census.iter().map(|row| row.crossings).sum();
    assert_eq!(meter.rows, rows.p0s.len());
    assert_eq!(
        meter.work, counted_tests,
        "executed traversal effort must equal the census's counted tests"
    );
    assert_eq!(
        meter.hits, counted_crossings,
        "executed crossings must equal the census's counted crossings"
    );
    assert!(meter.samples <= census_limits.max_total_samples);
    assert!(meter.intervals <= census_limits.max_total_intervals);

    // Per row: the census row's `crossings`/`tests` are that row's executed
    // `hits`/`work`. Single-row metered calls prove it row by row (this loop
    // is verification shape, not production shape: production stays batched)
    // and must reproduce the batch row's intervals bit for bit.
    for index in 0..rows.p0s.len() {
        let row_range = index..=index;
        let row_census = &census[row_range.clone()];
        let row_limits = surface
            .penetration_intervals_batch_census_limits(
                row_census,
                shape.n_offset_rays,
                shape.cap,
                shape.majority_step_mm,
                shape.rays_all,
            )
            .expect("single-row census limits");
        let (row_intervals, row_meter) = surface
            .penetration_intervals_batch_bounded_metered(
                &rows.p0s[row_range.clone()],
                &rows.p1s[row_range.clone()],
                &rows.r0s[row_range.clone()],
                &rows.r1s[row_range],
                shape.n_offset_rays,
                shape.majority_step_mm,
                shape.cap,
                shape.min_illegal_len_mm,
                shape.rays_all,
                row_limits,
            )
            .expect("single-row execution");
        assert_eq!(
            row_meter.hits, census[index].crossings,
            "row {index}: executed hits must equal the census's counted crossings"
        );
        assert_eq!(
            row_meter.work, census[index].tests,
            "row {index}: executed work must equal the census's counted tests"
        );
        assert_interval_rows_bit_identical(
            &row_intervals,
            &under_census[index..=index],
            "single-row execution",
        );
    }

    (census, under_census)
}

/// Field-by-field `to_bits` identity over ragged interval rows — the exact
/// field list the design's H5 row names. `to_bits` (not `==`) so that a
/// `-0.0` vs `0.0` or NaN divergence between the two executions cannot pass.
fn assert_interval_rows_bit_identical(left: &[Vec<Interval>], right: &[Vec<Interval>], what: &str) {
    assert_eq!(left.len(), right.len(), "row count vs {what}");
    for (row, (a_row, b_row)) in left.iter().zip(right).enumerate() {
        assert_eq!(
            a_row.len(),
            b_row.len(),
            "row {row} interval count vs {what}"
        );
        for (slot, (a, b)) in a_row.iter().zip(b_row).enumerate() {
            assert_eq!(
                a.s_a.to_bits(),
                b.s_a.to_bits(),
                "row {row} interval {slot} s_a vs {what}"
            );
            assert_eq!(
                a.s_b.to_bits(),
                b.s_b.to_bits(),
                "row {row} interval {slot} s_b vs {what}"
            );
            assert_eq!(
                a.entered, b.entered,
                "row {row} interval {slot} entered vs {what}"
            );
            assert_eq!(
                a.exited, b.exited,
                "row {row} interval {slot} exited vs {what}"
            );
            assert_eq!(
                a.max_depth.to_bits(),
                b.max_depth.to_bits(),
                "row {row} interval {slot} max_depth vs {what}"
            );
            assert_eq!(a.ray, b.ray, "row {row} interval {slot} ray vs {what}");
        }
    }
}

/// `make_raking_blade` (a real corpus fixture, OD-8 digest
/// 9567737006650358297): a 20 x 20 x 4 slab at z in [10, 14] plus a
/// 0.03 mm-thin blade raked 85 degrees below it. Row 1 crosses only the
/// slab; row 2 crosses the blade AND the slab — the blade's ~0.03 mm
/// penetration is a real counted crossing whose interval is dropped by
/// `min_illegal_len_mm`, so counted crossings and emitted intervals are
/// exercised as DIFFERENT numbers; row 3 runs 30 mm horizontally through
/// the slab.
///
/// Mutation transcript (2026-08-09, applied and reverted): under (a) this
/// test fails at `execution under the counted budgets:
/// CensusDrift { row: 1, resource: "hits", counted: 1, executed: 2 }`; under
/// (c) it fails at the same call with
/// `CensusDrift { row: 1, resource: "hits", counted: 0, executed: 2 }` —
/// row 0 (a miss) counted zero crossings, and its broadcast budget cannot
/// fund row 1's two. Under (b) this test stays green: its subsegments are
/// long enough for the step term to fund them, which is why the dense-stack
/// test exists.
///
/// **Digest supersession, 2026-08-09 (FU-32 phase J4).** The OD-8 digest quoted
/// in the first paragraph is the value of ITS date and appears here as a MODEL
/// IDENTIFIER ONLY — this test pins nothing about it and asserts nothing on it.
/// `make_raking_blade`'s OD-8 census digest moved to `18388631460456455184`
/// under the owner ruling for FU-32 J2 (the state-pure incremental
/// `CcoController::digest`). The fixture geometry, the rows, the mutation
/// transcript and every assertion below are unaffected: the movement is
/// decision-neutral and this model's routing output is byte-identical across
/// it. See `docs/rust-migration/CURRENT-STATUS.md` `#fu32-j4-2026-08-09`.
#[test]
fn fu8g_h5_raking_blade_census_equals_execution_bit_for_bit() {
    let surface = surface_of(&make_raking_blade().expect("raking blade fixture"));
    let mut rows = Rows::default();
    // Row 0: a deliberate miss, far from the model — the least-demanding row.
    rows.push([50.0, 50.0, 0.0], [50.0, 50.0, 1.0], 0.05, 0.05);
    // Row 1: vertical column through the slab only (2 crossings, 1 interval).
    rows.push([3.3, 2.7, 5.0], [3.3, 2.7, 20.0], 0.1, 0.2);
    // Row 2: vertical column through the raked blade AND the slab. The blade
    // crossing pair is ~0.03 mm apart: counted, then dropped as an interval.
    rows.push([-2.6, 0.3, 5.0], [-2.6, 0.3, 20.0], 0.1, 0.2);
    // Row 3: horizontal run through the slab's full 20 mm width.
    rows.push([-15.0, 1.3, 12.4], [15.0, 1.3, 12.4], 0.05, 0.15);

    for shape in [PRODUCTION_SHAPE, ALL_RAYS_SHAPE] {
        let (census, intervals) = assert_census_matches_execution(&surface, &rows, shape);
        // Non-vacuity: the batch genuinely crosses the model...
        assert!(census.iter().map(|row| row.crossings).sum::<usize>() > 0);
        assert_eq!(census[0].crossings, 0, "row 0 must miss");
        // ...and row 2 counts MORE crossings than row 1 (the blade pair)
        // while emitting the same single slab interval on its axis ray: the
        // count-vs-interval severance is real in this batch, not incidental.
        assert!(census[2].crossings > census[1].crossings);
        assert!(!intervals[1].is_empty());
        assert!(!intervals[2].is_empty());
    }
}

/// `make_overhang_table` (a real corpus fixture with holes, OD-8 digest
/// 1875674282253663076): four 4 x 4 x 20 legs under a 30 x 30 x 4 top. Rows
/// 1 and 4 run horizontally at leg height and cross TWO legs each — two
/// disjoint penetration intervals with a genuine hole between them on one
/// row, which is what makes this fixture the "model with holes" witness.
///
/// Mutation transcript (2026-08-09, applied and reverted): under (a) this
/// test fails at `execution under the counted budgets:
/// CensusDrift { row: 1, resource: "hits", counted: 7, executed: 8 }`; under
/// (c), `CensusDrift { row: 1, resource: "hits", counted: 0, executed: 8 }`.
/// (Row 1's executed crossings are 8, not 4: the hole ray runs along the
/// leg-wall quad centerlines, so each of its four wall crossings lands on a
/// triangle diagonal and reports a Möller pair — counted identically by
/// both walks, deduplicated identically by both executions.) Under (b) this
/// test stays green; see the dense-stack test.
///
/// **Digest supersession, 2026-08-09 (FU-32 phase J4).** The OD-8 digest quoted
/// in the first paragraph is the value of ITS date and appears here as a MODEL
/// IDENTIFIER ONLY — this test pins nothing about it and asserts nothing on it.
/// `make_overhang_table`'s OD-8 census digest moved to `16396858890798947253`
/// under the owner ruling for FU-32 J2 (the state-pure incremental
/// `CcoController::digest`). The fixture geometry, the rows, the mutation
/// transcript and every assertion below are unaffected: the movement is
/// decision-neutral and this model's routing output is byte-identical across
/// it. See `docs/rust-migration/CURRENT-STATUS.md` `#fu32-j4-2026-08-09`.
#[test]
fn fu8g_h5_overhang_table_hole_row_census_equals_execution_bit_for_bit() {
    let surface = surface_of(&make_overhang_table().expect("overhang table fixture"));
    let mut rows = Rows::default();
    // Row 0: a deliberate miss — the least-demanding row.
    rows.push([40.0, 40.0, 5.0], [40.0, 40.0, 6.0], 0.05, 0.05);
    // Row 1: horizontal at leg height through both x-side legs — 4 crossings,
    // 2 disjoint intervals, one hole.
    rows.push([-20.0, -10.0, 10.0], [20.0, -10.0, 10.0], 0.05, 0.1);
    // Row 2: vertical through one leg and the top (coincident faces at
    // z = 20 exercise the dedup path identically on both sides).
    rows.push([-9.3, -10.4, -5.0], [-9.3, -10.4, 30.0], 0.1, 0.2);
    // Row 3: vertical through the top only, between the legs.
    rows.push([0.7, 0.3, 15.0], [0.7, 0.3, 30.0], 0.1, 0.2);
    // Row 4: horizontal along y at x of the +x leg pair — the second
    // hole row.
    rows.push([10.2, -20.0, 9.7], [10.2, 20.0, 9.7], 0.05, 0.1);

    for shape in [PRODUCTION_SHAPE, ALL_RAYS_SHAPE] {
        let (census, intervals) = assert_census_matches_execution(&surface, &rows, shape);
        assert_eq!(census[0].crossings, 0, "row 0 must miss");
        // The hole is real: row 1's axis ray yields two disjoint intervals.
        let row1_axis: Vec<&Interval> = intervals[1]
            .iter()
            .filter(|interval| interval.ray == -1)
            .collect();
        assert_eq!(
            row1_axis.len(),
            2,
            "row 1 must penetrate two legs with a hole between them"
        );
        assert!(
            row1_axis[0].s_b < row1_axis[1].s_a,
            "the two leg intervals must be disjoint"
        );
    }
}

/// The synthetic dense-crossing fixture: a 60-slab lamella stack whose full
/// column crosses 120 faces on ONE ray — the treant's-tail regime (design
/// §M2) where per-ray crossing counts in the hundreds must be counted, not
/// averaged. Non-vacuity is asserted (`max_ray_crossings > 100`), and the
/// 0.01 mm subsegments make the F10 bound's `3 x` per-subsegment floor the
/// binding term, so mutation (b) cannot slip past this batch.
///
/// Mutation transcript (2026-08-09, applied and reverted): under (a) this
/// test fails at `execution under the counted budgets:
/// CensusDrift { row: 1, resource: "hits", counted: 119, executed: 120 }`;
/// under (b) — the F10 bound without its `3 x` term — it fails at the same
/// call with `CensusDrift { row: 1, resource: "samples", counted: 140,
/// executed: 365 }` (row 1's axis ray: 121 subsegments x 3 interior
/// samples each plus 2 extra on the long entry/exit runs = 365 executed,
/// against a mutated bound of `subsegments + ceil(seg_mm / step)` =
/// 121 + 19 = 140); under (c),
/// `CensusDrift { row: 1, resource: "hits", counted: 0, executed: 120 }`.
/// This is the ONLY test in the file that catches mutation (b) — the other
/// fixtures' subsegments are funded by the step term alone.
#[test]
fn fu8g_h5_dense_stack_exceeds_one_hundred_crossings_on_one_ray_and_matches() {
    let surface = dense_lamella_stack(60);
    let top = 60.0 * 0.02;
    let mut rows = Rows::default();
    // Row 0: a deliberate miss beside the stack — the least-demanding row.
    rows.push([5.0, 5.0, 0.0], [5.0, 5.0, 0.5], 0.02, 0.02);
    // Row 1: the full column — 120 crossings on the axis ray alone.
    rows.push([0.37, 0.53, -0.5], [0.37, 0.53, top + 0.5], 0.02, 0.05);
    // Row 2: a partial column through the lower half of the stack.
    rows.push([0.61, 0.29, -0.5], [0.61, 0.29, top / 2.0], 0.02, 0.05);
    // Row 3: horizontal through one slab's side walls.
    rows.push([-0.5, 0.43, 0.605], [1.5, 0.43, 0.605], 0.02, 0.02);

    for shape in [PRODUCTION_SHAPE, ALL_RAYS_SHAPE] {
        let (census, _intervals) = assert_census_matches_execution(&surface, &rows, shape);
        // Non-vacuity for the treant's tail: one ray genuinely carries more
        // than 100 crossings, and the census counted every one of them.
        assert!(
            census[1].max_ray_crossings > 100,
            "the dense fixture must exercise > 100 crossings on one ray \
             (got {})",
            census[1].max_ray_crossings
        );
        assert_eq!(census[0].crossings, 0, "row 0 must miss");
    }
}

/// The refusal fixture at the kernel boundary: when the batch's COUNTED
/// demand exceeds a limit, the refusal is the TYPED preflight
/// (`ResourceLimitExceeded`, naming the resource and both numbers) — never
/// an `AllocationFailure` and never a mid-execution `CensusDrift` — because
/// `check_penetration_limits` runs before any row executes and before any
/// interval-sized buffer exists. Count, THEN charge, THEN allocate.
///
/// Probed resource by resource: for each of the six limit fields, granting
/// exactly the counted demand minus one must refuse naming THAT resource
/// with `required` equal to the counted demand. The caps-level sibling —
/// the same discipline at the UNCHANGED
/// `ContactConstructionLimits::production()` values (4,000,000 work items,
/// 512 MiB scratch, byte-identical to develop @ e3168c5) over an 8,000-slab
/// over-demand mesh — is
/// `initial::tests::honest_over_demand_still_refuses_at_the_unchanged_production_caps`,
/// landed in H2 and green at this tip.
///
/// Mutation transcript (2026-08-09, applied and reverted): this test fails
/// under all three triad mutations at its final exact-demand admission —
/// under (a) with `CensusDrift { row: 1, resource: "hits", counted: 119,
/// executed: 120 }`, under (b) with `CensusDrift { row: 1, resource:
/// "samples", counted: 140, executed: 365 }`, under (c) with
/// `CensusDrift { row: 1, resource: "hits", counted: 0, executed: 120 }` —
/// so "granting exactly the counted demand admits the batch" is itself a
/// non-vacuous claim about the count being honest.
#[test]
fn fu8g_h5_over_demand_is_refused_typed_at_preflight_before_any_row_executes() {
    let surface = dense_lamella_stack(60);
    let top = 60.0 * 0.02;
    let mut rows = Rows::default();
    rows.push([5.0, 5.0, 0.0], [5.0, 5.0, 0.5], 0.02, 0.02);
    rows.push([0.37, 0.53, -0.5], [0.37, 0.53, top + 0.5], 0.02, 0.05);
    let shape = PRODUCTION_SHAPE;
    let census = surface
        .penetration_census_batch(
            &rows.p0s,
            &rows.p1s,
            &rows.r0s,
            &rows.r1s,
            shape.n_offset_rays,
            shape.rays_all,
        )
        .expect("census pass");
    let counted = surface
        .penetration_intervals_batch_census_limits(
            &census,
            shape.n_offset_rays,
            shape.cap,
            shape.majority_step_mm,
            shape.rays_all,
        )
        .expect("census limits");

    let probes: [(&str, PenetrationBatchLimits, usize); 6] = [
        (
            "rows",
            PenetrationBatchLimits {
                max_rows: counted.max_rows - 1,
                ..counted
            },
            counted.max_rows,
        ),
        (
            "work",
            PenetrationBatchLimits {
                max_total_work: counted.max_total_work - 1,
                ..counted
            },
            counted.max_total_work,
        ),
        (
            "hits",
            PenetrationBatchLimits {
                max_total_hits: counted.max_total_hits - 1,
                ..counted
            },
            counted.max_total_hits,
        ),
        (
            "samples",
            PenetrationBatchLimits {
                max_total_samples: counted.max_total_samples - 1,
                ..counted
            },
            counted.max_total_samples,
        ),
        (
            "intervals",
            PenetrationBatchLimits {
                max_total_intervals: counted.max_total_intervals - 1,
                ..counted
            },
            counted.max_total_intervals,
        ),
        (
            "scratch bytes",
            PenetrationBatchLimits {
                max_scratch_bytes: counted.max_scratch_bytes - 1,
                ..counted
            },
            counted.max_scratch_bytes,
        ),
    ];
    for (resource, limits, counted_demand) in probes {
        let refusal = surface.penetration_intervals_batch_bounded(
            &rows.p0s,
            &rows.p1s,
            &rows.r0s,
            &rows.r1s,
            shape.n_offset_rays,
            shape.majority_step_mm,
            shape.cap,
            shape.min_illegal_len_mm,
            shape.rays_all,
            limits,
        );
        let error = refusal
            .err()
            .unwrap_or_else(|| panic!("{resource}: one under the counted demand must refuse"));
        assert!(
            matches!(
                error,
                PenetrationBatchError::ResourceLimitExceeded {
                    resource: named,
                    required,
                    limit,
                } if named == resource
                    && required == counted_demand
                    && limit == counted_demand - 1
            ),
            "{resource}: one under the counted demand must refuse as the typed \
             preflight naming that resource, got {error:?}"
        );
    }
    // And granting exactly the counted demand admits the batch: the charge
    // is a boundary, not a margin.
    surface
        .penetration_intervals_batch_bounded(
            &rows.p0s,
            &rows.p1s,
            &rows.r0s,
            &rows.r1s,
            shape.n_offset_rays,
            shape.majority_step_mm,
            shape.cap,
            shape.min_illegal_len_mm,
            shape.rays_all,
            counted,
        )
        .expect("the exact counted demand must be admitted");
}
