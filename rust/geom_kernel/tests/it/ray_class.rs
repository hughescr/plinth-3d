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

/// The classification is a PARTITION of the non-hit cases by the determinant
/// AND, inside the band, by whether the ray reaches the triangle at all,
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
    let mut cleared_miss = 0_u32;
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
                if det.abs() <= 1.0e-14 {
                    // The proximity arm: a ray inside the band that provably cannot
                    // reach this triangle at all. Pinned by the two dedicated
                    // tests below, which hold the determinant FIXED and move
                    // only the origin — this battery cannot check the predicate
                    // here without re-implementing it, which would prove
                    // nothing.
                    cleared_miss += 1;
                }
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
        definite_miss - cleared_miss >= 100,
        "and the well-conditioned miss arm must be exercised too, got \
         {definite_miss} miss(es), {cleared_miss} of them decided by proximity"
    );
    // `exact_parallel` is allowed to be zero on this battery: exact zeros need
    // exactly-coplanar inputs, which the random arm does not produce. The
    // dedicated test above covers it, so this is recorded rather than asserted.
    println!("battery classes: near_parallel={near_parallel} exact_parallel={exact_parallel} definite_miss={definite_miss} cleared_miss={cleared_miss}");
}

/// **THE PROXIMITY PAIR.** Same triangle, same direction, therefore the SAME
/// determinant — and two different classifications, decided by geometry rather
/// than by a tolerance.
///
/// The determinant is a function of `dv`, `e1` and `e2` alone; the ray's origin
/// does not enter it. So translating the counterexample's origin sideways
/// leaves it at exactly `-8e-15`, inside `DET_EPS`, while moving the ray to
/// where it demonstrably cannot touch the triangle. Before the proximity
/// qualification both rows answered `NearParallel`, and the second one had
/// nothing to be uncertain about: it passes 49 mm from a triangle 2 mm across.
///
/// This is the pair the corpus behaviour turns on. A vertical support ray
/// running alongside a vertical wall is parallel to every facet of that wall,
/// including the facets on the far side of the model, and answering
/// `NearParallel` for all of them is what made a refusal out of empty space.
#[test]
fn the_determinant_alone_does_not_decide_a_ray_that_never_reaches_the_triangle() {
    // Held fixed across both rows, computed here rather than taken from the
    // function under test.
    let pvec = [
        DIRECTION[1] * E2[2] - DIRECTION[2] * E2[1],
        DIRECTION[2] * E2[0] - DIRECTION[0] * E2[2],
        DIRECTION[0] * E2[1] - DIRECTION[1] * E2[0],
    ];
    let det = E1[0] * pvec[0] + E1[1] * pvec[1] + E1[2] * pvec[2];
    assert!(
        det != 0.0 && det.abs() <= 1.0e-14,
        "both rows must sit INSIDE the undecidable band, got {det:e}"
    );

    // Row 1: the counterexample itself — through the interior. Undecidable.
    assert_eq!(
        ray_triangle_classified(ORIGIN, DIRECTION, V0, E1, E2, 1.0),
        RayTriangleClass::NearParallel,
        "a ray that pierces the interior is genuinely undecidable in this band"
    );

    // Row 2: the same ray, started 50 mm away across the triangle's own plane.
    // The triangle spans y in [0, 2]; this ray holds y = 50.5 for its whole
    // length, so no rounding of any determinant can bring the two together.
    let far = [ORIGIN[0], ORIGIN[1] + 50.0, ORIGIN[2]];
    assert_eq!(
        ray_triangle_classified(far, DIRECTION, V0, E1, E2, 1.0),
        RayTriangleClass::Miss,
        "a ray 49 mm clear of a 2 mm triangle is a MISS, whatever its \
         determinant is doing"
    );
}

/// The proximity qualification must not reach past the triangle's own extent.
///
/// A ray inside the band that clears the bounding sphere by a hair is still a
/// miss, but one that enters it is still undecidable — the predicate is allowed
/// to be conservative and is not allowed to be generous. Without this, widening
/// the bound until the corpus went quiet would pass every other test in this
/// file.
#[test]
fn the_proximity_qualification_is_conservative_at_its_own_boundary() {
    // The bounding sphere of V0/E1/E2 is centred on the centroid (2/3, 2/3, 0)
    // with radius |E1 - centroid| = sqrt((4/3)^2 + (2/3)^2) = 1.4907...
    let radius = ((4.0_f64 / 3.0).powi(2) + (2.0_f64 / 3.0).powi(2)).sqrt();

    // Just inside the sphere in y, running along x: undecidable, because the
    // ray really does pass through the region the triangle occupies.
    let inside = [0.4, 2.0 / 3.0 + radius * 0.99, -1.0e-16];
    assert_eq!(
        ray_triangle_classified(inside, DIRECTION, V0, E1, E2, 1.0),
        RayTriangleClass::NearParallel,
        "inside the triangle's own reach, the band stays undecidable"
    );

    // Comfortably outside it: a decided miss.
    let outside = [0.4, 2.0 / 3.0 + radius * 4.0, -1.0e-16];
    assert_eq!(
        ray_triangle_classified(outside, DIRECTION, V0, E1, E2, 1.0),
        RayTriangleClass::Miss,
        "well outside it, the band is decided"
    );
}

/// **THE PREDICATE MUST DECLINE WHEN ITS OWN ARITHMETIC IS UNSOUND.**
///
/// Four counterexamples found by adversarial review of the first version of
/// the proximity qualification, which asserted in its own doc comment that it
/// could never manufacture a miss. Each is a GENUINE intersection that the
/// first version reported as a definite `Miss`, and each has the same shape:
/// the closest-approach parameter `t` could not be computed, came back as
/// `0.0`, and the ray's distant origin was then read as clearance.
///
/// The magnitudes are absurd and no miniature produces them. That is not the
/// point. A predicate whose only job is to refine an uncertainty into a
/// certainty has exactly one way to be dangerous, and this is it.
#[test]
fn the_proximity_qualification_declines_rather_than_guesses() {
    // 1. `dot(dv, dv)` UNDERFLOWS to zero although the direction is not zero.
    //    The triangle lies in the x = 0 plane; the ray starts at x = -2 and
    //    crosses at t = 2 / 1e-200 = 2e200, inside an infinite t_max — which is
    //    what `plinth-mesh`'s triage actually passes.
    let underflow = ray_triangle_classified(
        [-2.0, 0.25, 0.25],
        [1.0e-200, 0.0, 0.0],
        V0,
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        f64::INFINITY,
    );
    assert_eq!(
        underflow,
        RayTriangleClass::NearParallel,
        "a direction whose squared norm underflows leaves the closest approach \
         uncomputable, and uncomputable is NOT clearance"
    );

    // 2. `dot(dv, dv)` OVERFLOWS to infinity, so `t = finite / INFINITY = 0`.
    //    A triangle scaled to 1e-100 in the same plane, crossed at t = 1e-155.
    let overflow = ray_triangle_classified(
        [-1.0, 2.5e-101, 2.5e-101],
        [1.0e155, 0.0, 0.0],
        V0,
        [0.0, 1.0e-100, 0.0],
        [0.0, 0.0, 1.0e-100],
        1.0,
    );
    assert_eq!(
        overflow,
        RayTriangleClass::NearParallel,
        "and neither is an overflowing squared norm"
    );

    // 3. A NaN `t_max`. `f64::max` returns its non-NaN operand, so the old
    //    `t_max.max(0.0)` reported an EMPTY parameter range where the truth is
    //    an unusable one, and `t` clamped to a perfectly finite 0.0 that passed
    //    every other check. The control below is the same ray with a usable
    //    bound: identical determinant, identical geometry, and a real answer.
    let far = [ORIGIN[0], ORIGIN[1] + 50.0, ORIGIN[2]];
    assert_eq!(
        ray_triangle_classified(far, DIRECTION, V0, E1, E2, 1.0),
        RayTriangleClass::Miss,
        "control: with a usable bound this ray is a decided miss"
    );
    assert_eq!(
        ray_triangle_classified(far, DIRECTION, V0, E1, E2, f64::NAN),
        RayTriangleClass::NearParallel,
        "an unusable bound is not an empty range, and must not decide anything"
    );

    // 4. `dot(dv, dv)` is SUBNORMAL: positive and finite, and therefore past a
    //    guard that only checked those two things, but carrying almost no
    //    significant bits. `dv = (2e-162, 0, 0)` gives 5e-324, the smallest
    //    subnormal there is. The interior crossing is at t = 5e163.
    let subnormal = ray_triangle_classified(
        [-100.0, 0.25, 0.25],
        [2.0e-162, 0.0, 0.0],
        V0,
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        f64::INFINITY,
    );
    assert_eq!(
        subnormal,
        RayTriangleClass::NearParallel,
        "a subnormal squared norm has no significant bits to divide by, so the \
         closest approach it yields is not one"
    );
}

// ---- the plane-side qualification (2026-10-13, Cylinder vent) -------------
//
// The bounding-sphere test cannot clear a face that is LONGER than its distance
// from the ray: the sphere through the face's own vertices then contains the
// ray. A drilled bore's wall is that face, seen from the bore's own axis. The
// plane-side test asks the other question — does the ray's whole accepted span
// stay strictly on one side of the face's plane — under a derived forward error
// bound. These tests pin it.
//
// SABOTAGE (recorded by hand, 2026-10-13): with the `span_clears_plane` arm
// removed from `ray_triangle_classified`, three tests below fail
// (`NearParallel` where `Miss` is asserted): the two bore-wall tests and the
// climbing-away half of the crossing test. The crossing half and the coplanar
// test still pass, as they must: neither may be decided by this arm.

/// The determinant the kernel computes, recomputed here from the literals so
/// band membership is a fact of the fixture, not a claim of the code.
fn det_of(dv: [f64; 3], e1: [f64; 3], e2: [f64; 3]) -> f64 {
    let pvec = [
        dv[1] * e2[2] - dv[2] * e2[1],
        dv[2] * e2[0] - dv[0] * e2[2],
        dv[0] * e2[1] - dv[1] * e2[0],
    ];
    e1[0] * pvec[0] + e1[1] * pvec[1] + e1[2] * pvec[2]
}

/// The distance from the ray's LINE to the centre of the triangle's bounding
/// sphere, minus that sphere's radius — negative when the sphere contains the
/// line, so the bounding-sphere test cannot clear the face. The witness that
/// a row really needed the new arm.
fn sphere_reach(o: [f64; 3], dv: [f64; 3], v0: [f64; 3], e1: [f64; 3], e2: [f64; 3]) -> f64 {
    use geom_kernel::vec3::{add, dot, norm, scale, sub};
    let c = add(v0, scale(add(e1, e2), 1.0 / 3.0));
    let r = [v0, add(v0, e1), add(v0, e2)]
        .iter()
        .map(|vertex| norm(sub(*vertex, c)))
        .fold(0.0_f64, f64::max);
    let to_c = sub(c, o);
    let along = dot(to_c, dv) / dot(dv, dv);
    norm(sub(to_c, scale(dv, along))) - r
}

/// A NEARLY parallel bore-wall face 1.24 mm from the axis is a decided miss.
///
/// The face is 5 mm long (longer than the 2.5 mm wall it lines) and tilted by
/// 1e-15 in y over its length, so the determinant is exactly
/// `-0.32 · 1e-15 = -3.2e-16`: inside the band, nonzero. Its bounding sphere
/// contains the axis, so the old proximity test could not clear it.
#[test]
fn a_nearly_parallel_bore_wall_beside_the_axis_is_a_miss() {
    let o = [0.0, 0.0, 0.0];
    let dv = [1.0, 0.0, 0.0];
    let v0 = [-1.0, 1.24, -0.16];
    let e1 = [5.0, 1.0e-15, 0.0];
    let e2 = [0.0, 0.0, 0.32];
    let det = det_of(dv, e1, e2);
    assert!(det != 0.0 && det.abs() <= 1.0e-14, "in the band: {det:e}");
    assert!(
        sphere_reach(o, dv, v0, e1, e2) < 0.0,
        "the face's bounding sphere contains the axis, so only the plane test can decide it"
    );
    assert_eq!(
        ray_triangle_classified(o, dv, v0, e1, e2, 8.0),
        RayTriangleClass::Miss,
        "the axis never comes within 1.24 mm of the face's plane"
    );
}

/// A whole 24-sided bore drilled along an OBLIQUE axis, cast along that axis:
/// every wall face is EXACTLY parallel to the axis in real arithmetic, and the
/// rounded vertices put some of them in the determinant band. Every one must
/// come back a non-hit and none undecidable.
///
/// This is the Cylinder vent's shape: r 1.25 mm, 24 sides (facet midlines
/// 1.25·cos 7.5° = 1.239 mm from the axis), 2.5 mm long.
#[test]
fn every_wall_face_of_an_oblique_bore_is_decided_from_its_own_axis() {
    use geom_kernel::vec3::{add, cross, normalize, scale, sub};
    let d = normalize([1.0, 2.0, 3.0]).expect("axis");
    let u = normalize(cross(d, [1.0, 0.0, 0.0])).expect("u");
    let w = cross(d, u);
    let c = [10.3, -4.7, 21.9];
    let (radius, half) = (1.25_f64, 1.25_f64);
    let ring = |k: usize| {
        let theta = std::f64::consts::TAU * (k % 24) as f64 / 24.0;
        add(scale(u, radius * theta.cos()), scale(w, radius * theta.sin()))
    };
    let o = sub(c, scale(d, 4.0));
    let (mut band, mut sphere_blind, mut faces) = (0_u32, 0_u32, 0_u32);
    for k in 0..24 {
        let a = add(sub(c, scale(d, half)), ring(k));
        let b = add(add(c, scale(d, half)), ring(k));
        let a2 = add(sub(c, scale(d, half)), ring(k + 1));
        let b2 = add(add(c, scale(d, half)), ring(k + 1));
        for (v0, p1, p2) in [(a, b, b2), (a, b2, a2)] {
            let (e1, e2) = (sub(p1, v0), sub(p2, v0));
            let det = det_of(d, e1, e2);
            let class = ray_triangle_classified(o, d, v0, e1, e2, 8.0);
            faces += 1;
            if det != 0.0 && det.abs() <= 1.0e-14 {
                band += 1;
                if sphere_reach(o, d, v0, e1, e2) < 0.0 {
                    sphere_blind += 1;
                }
            }
            assert!(
                matches!(class, RayTriangleClass::Miss | RayTriangleClass::ExactParallel),
                "face {k}: a bore wall seen from its own axis is a non-hit, decided: {class:?} (det {det:e})"
            );
        }
    }
    // COUNTED ROWS: the fixture must actually reach the band, with faces the
    // sphere cannot clear, or it tests nothing.
    assert_eq!(faces, 48);
    assert!(band >= 4, "the rounded wall must land faces in the band, got {band}");
    assert!(
        sphere_blind >= 4,
        "and some of those must be faces whose bounding sphere holds the axis, got {sphere_blind}"
    );
    println!("oblique bore: {faces} faces, {band} in the band, {sphere_blind} the sphere cannot clear");
}

/// A near-parallel face the span REALLY crosses stays undecidable.
///
/// Bore-scale version of the §O4 counterexample: a 5 mm face in the z = 0
/// plane, and a ray that climbs through that plane at t = 0.05 at the
/// interior point (0.05, 0, 0) (u = 0.21, v = 0.5). The plane test sees the
/// two ends of the span on opposite sides and must decline.
#[test]
fn a_near_parallel_face_the_span_really_crosses_stays_undecidable() {
    let o = [0.0, 0.0, -1.0e-16];
    let dv = [1.0, 0.0, 2.0e-15];
    let v0 = [-1.0, -0.16, 0.0];
    let e1 = [5.0, 0.0, 0.0];
    let e2 = [0.0, 0.32, 0.0];
    let det = det_of(dv, e1, e2);
    assert!(det != 0.0 && det.abs() <= 1.0e-14, "in the band: {det:e}");
    assert_eq!(
        ray_triangle_classified(o, dv, v0, e1, e2, 8.0),
        RayTriangleClass::NearParallel,
        "a real interior crossing must never be decided as a miss"
    );
    // The same face, the same direction, with the ray started 1e-16 on the
    // OTHER side and climbing away: it never reaches the plane. The bound is
    // RELATIVE to the terms that make up the side value (here 1e-16 x 1.6),
    // not an absolute closeness, so even this offset is decided: a miss.
    let above = [0.0, 0.0, 1.0e-16];
    assert_eq!(
        ray_triangle_classified(above, dv, v0, e1, e2, 8.0),
        RayTriangleClass::Miss,
        "a ray that starts above the plane and climbs away never meets it"
    );
}

/// A ray lying IN an oblique triangle's plane is never a miss by the plane
/// test: its side value is zero up to rounding, inside the bound.
#[test]
fn a_coplanar_ray_is_never_decided_by_the_plane_test() {
    use geom_kernel::vec3::{add, normalize, scale, sub};
    let v0 = [0.1, 0.2, 0.3];
    let e1 = [1.1, 0.7, -0.3];
    let e2 = [0.2, 1.3, 0.9];
    let mut undecided = 0_u32;
    for step in 1..=40 {
        let f = f64::from(step) / 41.0;
        let o = add(v0, add(scale(e1, 0.2 * f), scale(e2, 0.2)));
        let dv = normalize(sub(scale(e1, 1.0 + f), scale(e2, 0.5))).expect("in-plane direction");
        let det = det_of(dv, e1, e2);
        let class = ray_triangle_classified(o, dv, v0, e1, e2, 0.05);
        if det != 0.0 && det.abs() <= 1.0e-14 {
            undecided += 1;
            assert_eq!(
                class,
                RayTriangleClass::NearParallel,
                "row {step}: an in-plane ray in the band is undecidable, never a miss"
            );
        } else {
            assert_eq!(class, RayTriangleClass::ExactParallel, "row {step}: det {det:e}");
        }
    }
    assert!(undecided >= 5, "the battery must reach the band, got {undecided}");
}
