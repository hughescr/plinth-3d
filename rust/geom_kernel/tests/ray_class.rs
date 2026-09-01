//! The classified ray/triangle primitive: the counterexample it was built for,
//! and the agreement contract that proves it did not drift from the pin.
//!
//! # The defect
//!
//! `ray_triangle` folds its determinant guard into `hit`. For
//! `0 < |det| <= DET_EPS` — a ray NEARLY parallel to the triangle's plane —
//! that reports `hit = false`, which the whole workspace read as "no crossing".
//! The old contract in `moller.rs` called this a PRUNE, on the argument that a
//! parallel ray's crossings register on adjacent faces. That argument holds only
//! for a ray lying IN the plane. A ray nearly parallel to it, originating near
//! it, pierces the face INTERIOR at a real finite `t`, and no adjacent face
//! contains that point.
//!
//! `PRE-FIX BEHAVIOUR, MEASURED` (run against `487e71f` before this change):
//! `ray_triangle` on the literals below returned
//! `RayTriangleHit { hit: false, t: -0.0, u: -0.0, v: -0.0 }` — the `inv = 0`
//! guard's products — with `det = -8.00000000000000062e-15` and a genuine
//! crossing at `t = 0.05`, `u = 0.225`, `v = 0.25`. There was no API through
//! which that uncertainty could be expressed, which is the defect.

use geom_kernel::{ray_triangle, ray_triangle_classified, RayTriangleClass};

/// The §O4 counterexample, in exact f64 arithmetic.
///
/// Every product in the determinant is exact: `e2 = (0, 2, 0)` is a doubling of
/// a stored value and the other factors are `0.0` or a power of two times a
/// stored value, so `pvec = cross(dv, e2) = (-4e-15, 0, 2)` and
/// `det = dot(e1, pvec) = -8e-15` are exact rather than rounded results.
const V0: [f64; 3] = [0.0, 0.0, 0.0];
const E1: [f64; 3] = [2.0, 0.0, 0.0];
const E2: [f64; 3] = [0.0, 2.0, 0.0];
const ORIGIN: [f64; 3] = [0.4, 0.5, -1.0e-16];
const DIRECTION: [f64; 3] = [1.0, 0.0, 2.0e-15];

/// **THE PIN.** The ray really does cross this triangle's interior, the pinned
/// function really does report a miss, and the classified function names the
/// uncertainty instead of hiding it.
///
/// This test cannot pass against the pre-fix tree: `ray_triangle_classified`
/// did not exist there, and no other expression of "undecidable" did either.
#[test]
fn the_near_parallel_counterexample_is_a_miss_on_the_pin_and_undecidable_when_classified() {
    // The crossing is real and deep in the interior. Derived from the geometry,
    // not from either function under test: the ray's z goes from -1e-16 toward
    // the z = 0 plane at 2e-15 per unit t, so it reaches the plane at
    // t = 1e-16 / 2e-15 = 0.05, at the point (0.45, 0.5, 0). Barycentrically
    // that is u = 0.45/2 = 0.225 and v = 0.5/2 = 0.25, with u + v = 0.475 — far
    // inside the u >= 0, v >= 0, u + v <= 1 acceptance region, not on a shell.
    let t_true: f64 = 1.0e-16 / 2.0e-15;
    assert!(
        (t_true - 0.05).abs() < 1.0e-15,
        "the fixture's own arithmetic: the crossing is at t = 0.05"
    );

    let pinned = ray_triangle(ORIGIN, DIRECTION, V0, E1, E2, 1.0);
    assert!(
        !pinned.hit,
        "the pinned function maps the nonzero determinant band to a silent miss \
         (oracle parity). If this ever becomes true, the pin moved: {pinned:?}"
    );

    let classified = ray_triangle_classified(ORIGIN, DIRECTION, V0, E1, E2, 1.0);
    assert_eq!(
        classified,
        RayTriangleClass::NearParallel,
        "a real interior crossing inside the determinant band is UNDECIDABLE, \
         not a miss, and not an invented hit either"
    );

    // The determinant, recomputed from the literals so the band membership is a
    // fact of the fixture rather than a claim about the implementation.
    let pvec = [
        DIRECTION[1] * E2[2] - DIRECTION[2] * E2[1],
        DIRECTION[2] * E2[0] - DIRECTION[0] * E2[2],
        DIRECTION[0] * E2[1] - DIRECTION[1] * E2[0],
    ];
    let det = E1[0] * pvec[0] + E1[1] * pvec[1] + E1[2] * pvec[2];
    assert!(det != 0.0, "nonzero: this is not the exact-parallel prune");
    assert!(
        det.abs() <= 1.0e-14,
        "and inside DET_EPS: det = {det:e} is what puts it in the band"
    );
}

/// `det == 0.0` exactly is a DIFFERENT answer, and the type keeps them apart.
///
/// A ray in the triangle's plane has no interior crossing at all — its
/// crossings of the closed surface sit on the edges, where adjacent faces
/// answer. That is a genuine prune, and collapsing it into `NearParallel` would
/// make every coplanar ray in every mesh uncertain.
#[test]
fn an_exactly_parallel_ray_is_a_prune_and_not_an_uncertainty() {
    // In-plane ray: z = 0 throughout, so det is exactly 0.
    let classified = ray_triangle_classified([0.4, 0.5, 0.0], [1.0, 0.0, 0.0], V0, E1, E2, 1.0);
    assert_eq!(classified, RayTriangleClass::ExactParallel);

    // A degenerate triangle: both edges collinear, so every determinant is 0.
    let degenerate = ray_triangle_classified(
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
        V0,
        E1,
        [4.0, 0.0, 0.0],
        2.0,
    );
    assert_eq!(degenerate, RayTriangleClass::ExactParallel);

    // A zero-length direction: pvec is zero, so det is zero.
    let no_direction = ray_triangle_classified([0.4, 0.5, 1.0], [0.0; 3], V0, E1, E2, 1.0);
    assert_eq!(no_direction, RayTriangleClass::ExactParallel);
}

/// A definite miss stays a definite miss: a well-conditioned determinant with a
/// barycentric outside the acceptance region is `Miss`, not `NearParallel`.
///
/// Without this, a classifier that answered `NearParallel` for everything would
/// pass the two tests above.
#[test]
fn a_well_conditioned_non_hit_is_a_definite_miss() {
    // Straight down, well clear of the triangle in x and y.
    let outside = ray_triangle_classified([5.0, 5.0, 1.0], [0.0, 0.0, -1.0], V0, E1, E2, 2.0);
    assert_eq!(outside, RayTriangleClass::Miss);

    // Inside in x/y, but the crossing is behind t_max.
    let too_far = ray_triangle_classified([0.5, 0.5, 100.0], [0.0, 0.0, -1.0], V0, E1, E2, 1.0);
    assert_eq!(too_far, RayTriangleClass::Miss);

    // And a plain hit is a plain hit, so `Hit` is reachable at all.
    let hit = ray_triangle_classified([0.5, 0.5, 1.0], [0.0, 0.0, -1.0], V0, E1, E2, 2.0);
    let RayTriangleClass::Hit(payload) = hit else {
        panic!("a ray straight down through the interior must hit: {hit:?}");
    };
    assert!(payload.hit);
    assert!((payload.t - 1.0).abs() < 1.0e-15);
}

/// One battery row: `(origin, direction, v0, e1, e2, t_max)`.
type BatteryRow = ([f64; 3], [f64; 3], [f64; 3], [f64; 3], [f64; 3], f64);

/// A deterministic input battery over rays, triangles and scales, hashed into
/// the four classes. Used by both agreement tests below.
///
/// The generator is a plain LCG over a fixed seed: no randomness that varies
/// between runs, and no dependence on either function under test.
fn battery() -> Vec<BatteryRow> {
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let mut next = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // Uniform in [-1, 1), from the high bits.
        (state >> 11) as f64 / (1_u64 << 53) as f64 * 2.0 - 1.0
    };
    let mut out = Vec::new();
    for scale in [1.0e-6_f64, 1.0e-3, 1.0, 1.0e3] {
        for _ in 0..2000 {
            let v0 = [next() * scale, next() * scale, next() * scale];
            let e1 = [next() * scale, next() * scale, next() * scale];
            let e2 = [next() * scale, next() * scale, next() * scale];
            let o = [next() * scale, next() * scale, next() * scale];
            let dv = [next(), next(), next()];
            out.push((o, dv, v0, e1, e2, scale * 4.0));
        }
    }
    // Plus the pinned counterexample and its family at three direction scales,
    // so the battery is guaranteed to contain in-band determinants rather than
    // merely likely to.
    for k in [1.0_f64, 0.5, 2.0] {
        out.push((
            ORIGIN,
            [DIRECTION[0] * k, DIRECTION[1] * k, DIRECTION[2] * k],
            V0,
            E1,
            E2,
            1.0,
        ));
    }
    out
}

/// **THE AGREEMENT CONTRACT.** `ray_triangle(..).hit` is exactly
/// `matches!(classified, Hit(_))`, and a `Hit` carries bit-identical `t`, `u`,
/// `v`.
///
/// The classified function duplicates the pinned arithmetic sequence rather than
/// sharing it — the pin is not edited by this change — so this battery is what
/// holds the two together. A single reassociated term in either body shows up
/// here as a bit difference, not as a rounding-sized discrepancy.
#[test]
fn the_classified_form_agrees_with_the_pin_on_every_hit_bit_for_bit() {
    let mut hits = 0_u32;
    let mut misses = 0_u32;
    for (o, dv, v0, e1, e2, t_max) in battery() {
        let pinned = ray_triangle(o, dv, v0, e1, e2, t_max);
        let classified = ray_triangle_classified(o, dv, v0, e1, e2, t_max);
        match classified {
            RayTriangleClass::Hit(payload) => {
                hits += 1;
                assert!(
                    pinned.hit,
                    "classified Hit where the pin says miss: {o:?} {dv:?}"
                );
                assert_eq!(payload.t.to_bits(), pinned.t.to_bits(), "t drifted");
                assert_eq!(payload.u.to_bits(), pinned.u.to_bits(), "u drifted");
                assert_eq!(payload.v.to_bits(), pinned.v.to_bits(), "v drifted");
                assert!(payload.hit, "a Hit payload carries hit = true");
            }
            RayTriangleClass::Miss
            | RayTriangleClass::NearParallel
            | RayTriangleClass::ExactParallel => {
                misses += 1;
                assert!(
                    !pinned.hit,
                    "the pin found a hit the classified form did not: {o:?} {dv:?}"
                );
            }
        }
    }
    // COUNTED ROWS. A battery that exercised only one arm would pass the loop
    // above having proved nothing about the other.
    assert!(
        hits >= 100,
        "the agreement battery must actually contain hits, got {hits}"
    );
    assert!(misses >= 100, "and non-hits, got {misses}");
}

/// The classification is a PARTITION of the non-hit cases by the determinant,
/// checked against the determinant recomputed in the test.
///
/// This is the sabotage guard for the test above: agreement with the pin says
/// nothing about whether `NearParallel` and `ExactParallel` are being told
/// apart, because the pin cannot tell them apart either. A classifier that
/// returned `Miss` for the whole band would pass the agreement battery and fail
/// here.
#[test]
fn every_non_hit_lands_in_the_class_its_determinant_selects() {
    let mut near_parallel = 0_u32;
    let mut exact_parallel = 0_u32;
    let mut definite_miss = 0_u32;
    for (o, dv, v0, e1, e2, t_max) in battery() {
        let pvec = [
            dv[1] * e2[2] - dv[2] * e2[1],
            dv[2] * e2[0] - dv[0] * e2[2],
            dv[0] * e2[1] - dv[1] * e2[0],
        ];
        let det = e1[0] * pvec[0] + e1[1] * pvec[1] + e1[2] * pvec[2];
        match ray_triangle_classified(o, dv, v0, e1, e2, t_max) {
            RayTriangleClass::Hit(_) => assert!(det.abs() > 1.0e-14),
            RayTriangleClass::Miss => {
                definite_miss += 1;
                assert!(det.abs() > 1.0e-14, "Miss demands a decided determinant");
            }
            RayTriangleClass::NearParallel => {
                near_parallel += 1;
                assert!(det != 0.0, "NearParallel is the NONZERO band");
                assert!(det.abs() <= 1.0e-14, "and it is inside DET_EPS");
            }
            RayTriangleClass::ExactParallel => {
                exact_parallel += 1;
                // Exact bit comparison is the point: `ExactParallel` is defined
                // as `det == 0.0` and nothing near it.
                assert!(
                    det == 0.0,
                    "ExactParallel is exactly zero and nothing else, got {det:e}"
                );
            }
        }
    }
    // COUNTED ROWS again, and the one that matters most: the uncertain arm must
    // have fired. A classifier that can never answer NearParallel is the
    // checks-nothing failure this whole change exists to remove.
    assert!(
        near_parallel >= 3,
        "the battery's planted counterexample family must reach the uncertain \
         arm, got {near_parallel}"
    );
    assert!(
        definite_miss >= 100,
        "and the definite-miss arm must be exercised too, got {definite_miss}"
    );
    // `exact_parallel` is allowed to be zero on this battery: exact zeros need
    // exactly-coplanar inputs, which the random arm does not produce. The
    // dedicated test above covers it, so this is recorded rather than asserted.
    println!("battery classes: near_parallel={near_parallel} exact_parallel={exact_parallel} definite_miss={definite_miss}");
}
