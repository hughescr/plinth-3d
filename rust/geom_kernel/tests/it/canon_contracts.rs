//! Degenerate-boundary contract tests for the geometry canon
//! (`docs/specs/geometry-canon.md`).
//!
//! These assert that the SURVIVING implementation is right at each boundary its
//! public doc comment names. They are deliberately NOT characterisation tests of
//! the retired copies: the owner ruling is *"we know the fix"*, so nothing here
//! re-measures how wrong `drilling::segment_distance`,
//! `triage::ray_triangle_distance`, `perception::point_segment_distance` or
//! `joints::normalize` were. Every assertion below is a statement about the
//! contract a crate agent is about to call.

#![allow(
    clippy::float_cmp,
    reason = "these tests EXIST to pin exact float outcomes (exact 0.0 at a \
              degeneracy, bit-identity between two spellings). An approximate \
              comparison would not test the contract."
)]

use geom_kernel::poly2::{
    self, point_in_polygon, point_segment_distance, segments_intersect, signed_area,
    DEGENERATE_EDGE_L2_MM2, SLICE_QUANTUM_MM,
};
use geom_kernel::testkit::kern_moller;
use geom_kernel::vec3::{cross, dist, dot, norm, normalize, sub, V3};
use geom_kernel::{ray_triangle, segment_segment_distance};

/// One `segments_intersect` fixture: label, expected answer, and the four points.
type IntersectCase = (&'static str, bool, [f64; 2], [f64; 2], [f64; 2], [f64; 2]);

/// Deterministic 64-bit LCG. Tests must not depend on a system RNG.
struct Lcg(u64);

impl Lcg {
    fn next_f64(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // top 53 bits -> [0, 1)
        ((self.0 >> 11) as f64) * (1.0 / 9_007_199_254_740_992.0)
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }
}

// ---------------------------------------------------------------------------
// vec3 — OP-6, OP-7, OP-9
// ---------------------------------------------------------------------------

#[test]
fn norm_zero_vector_is_exactly_zero() {
    assert_eq!(norm([0.0, 0.0, 0.0]), 0.0);
    assert_eq!(norm([-0.0, -0.0, -0.0]), 0.0);
}

#[test]
fn norm_overflow_and_underflow_boundaries_are_where_the_doc_says() {
    // Overflow: the SUM of the squares is what overflows, not each square.
    assert!(norm([1e200, 1e200, 1e200]).is_infinite());
    assert!(norm([1e154, 1e154, 1e154]).is_infinite());
    assert!(norm([1e153, 1e153, 1e153]).is_finite());

    // Underflow: each square rounds to zero before the sum.
    assert_eq!(norm([1e-200, 1e-200, 1e-200]), 0.0);
    assert_eq!(norm([1e-162, 1e-162, 1e-162]), 0.0);
    assert!(norm([2e-162, 2e-162, 2e-162]) > 0.0);
}

#[test]
fn norm_propagates_non_finite_rather_than_refusing() {
    assert!(norm([f64::NAN, 0.0, 0.0]).is_nan());
    assert!(norm([f64::INFINITY, 0.0, 0.0]).is_infinite());
}

#[test]
fn norm_is_bit_identical_to_the_expanded_spelling() {
    // The canon consolidates `(x*x + y*y + z*z).sqrt()` sites into `norm` on the
    // grounds that the two spellings are the same implementation. A crate agent
    // relies on that being byte-neutral, so pin it here rather than in prose.
    let mut rng = Lcg(0x5EED_1234_ABCD_0001);
    let mut checked = 0_u32;
    let mut divergent = 0_u32;
    for _ in 0..200_000 {
        let a: V3 = [
            rng.range(-60.0, 60.0),
            rng.range(-60.0, 60.0),
            rng.range(-60.0, 60.0),
        ];
        let expanded = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
        if norm(a).to_bits() != expanded.to_bits() {
            divergent += 1;
        }
        checked += 1;
    }
    // Gate on the work actually done, not on reaching the end of the loop.
    assert_eq!(checked, 200_000, "sweep did not run");
    assert_eq!(divergent, 0, "norm diverged from the expanded spelling");
}

#[test]
fn dist_of_a_point_with_itself_is_exactly_zero() {
    let a = [12.5, -3.25, 88.125];
    assert_eq!(dist(a, a), 0.0);
    // The overflow boundary applies to the DIFFERENCE, not to the operands.
    assert_eq!(dist([1e200, 0.0, 0.0], [1e200 - 1.0, 0.0, 0.0]), 0.0);
}

#[test]
fn cross_of_parallel_or_zero_vectors_is_exactly_the_zero_vector() {
    assert_eq!(cross([0.0, 0.0, 0.0], [1.0, 2.0, 3.0]), [0.0, 0.0, 0.0]);
    assert_eq!(cross([1.0, 2.0, 3.0], [2.0, 4.0, 6.0]), [0.0, 0.0, 0.0]);
    assert_eq!(cross([1.0, 2.0, 3.0], [-1.0, -2.0, -3.0]), [0.0, 0.0, 0.0]);
    // And that zero vector is NOT a direction: normalize must refuse it.
    assert_eq!(normalize(cross([1.0, 2.0, 3.0], [2.0, 4.0, 6.0])), None);
}

#[test]
fn normalize_refuses_every_input_without_a_direction() {
    // The zero vector, both signs of zero.
    assert_eq!(normalize([0.0, 0.0, 0.0]), None);
    assert_eq!(normalize([-0.0, -0.0, -0.0]), None);
    // Non-finite components.
    assert_eq!(normalize([f64::NAN, 0.0, 0.0]), None);
    assert_eq!(normalize([f64::INFINITY, 1.0, 1.0]), None);
    assert_eq!(normalize([f64::NEG_INFINITY, 1.0, 1.0]), None);
    // Squared-norm overflow.
    assert_eq!(normalize([1e200, 1e200, 1e200]), None);
    // Squared-norm underflow (the measured `[inf, inf, inf]` case in the
    // retired joints copy).
    assert_eq!(normalize([1e-200, 1e-200, 1e-200]), None);
    assert_eq!(normalize([1e-162, 1e-162, 1e-162]), None);
}

#[test]
fn normalize_answers_with_a_finite_unit_vector_where_a_direction_exists() {
    // Pinned exactly, because the reciprocal-multiply spelling is part of the
    // contract: `scale(a, 1.0/norm(a))`, not three divisions. Componentwise
    // division would give [0.6, 0.8, 0.0] — one ULP away in x.
    let u = normalize([3.0, 4.0, 0.0]).expect("3-4-5 vector has a direction");
    assert_eq!(u, [0.600_000_000_000_000_1, 0.8, 0.0]);
    assert!(
        u[0] != 3.0 / 5.0,
        "spelling drifted to componentwise division"
    );

    // Every accepted answer is finite and of unit length to within a few ULP.
    let mut rng = Lcg(0x5EED_1234_ABCD_0002);
    let mut accepted = 0_u32;
    for _ in 0..100_000 {
        let a: V3 = [
            rng.range(-60.0, 60.0),
            rng.range(-60.0, 60.0),
            rng.range(-60.0, 60.0),
        ];
        let u = normalize(a).expect("a 60 mm-cube vector has a direction");
        assert!(u.iter().all(|c| c.is_finite()));
        assert!((norm(u) - 1.0).abs() < 1e-15);
        accepted += 1;
    }
    assert_eq!(accepted, 100_000, "sweep did not run");
}

#[test]
fn normalize_some_is_only_a_unit_vector_above_the_subnormal_band() {
    // The gap between the canon's ruled FORMULA and the canon's PROSE. The
    // prose says everything below ~1e-154 refuses; the formula refuses only
    // once the squared norm reaches exactly zero, ~1.6e-162. In between, the
    // squared norm is subnormal and `Some` comes back finite but NOT unit.
    // Pinned here so it is a known, stated fact rather than a surprise, and
    // flagged for an owner ruling rather than silently patched.
    let bad = normalize([2e-162, 2e-162, 2e-162]).expect("2e-162 is above the zero boundary");
    assert!(bad.iter().all(|c| c.is_finite()));
    assert!(
        (norm(bad) - 1.0).abs() > 1e-3,
        "subnormal-band behaviour changed; the canon discrepancy may have been \
         closed without amending the spec"
    );
    assert!((norm(bad) - 8.997_827_589_086_393e-1).abs() < 1e-15);

    // Above the subnormal band the guarantee holds exactly.
    let good = normalize([1e-154, 1e-154, 1e-154]).expect("1e-154 has a direction");
    assert_eq!(norm(good), 1.0);
}

#[test]
fn normalize_never_returns_a_sentinel_that_reads_as_a_pass_downstream() {
    // The failure this replaces: a NaN direction compares `false` against every
    // `<` and `>`, so a downstream guard reads "not violated". `None` cannot.
    for a in [
        [0.0, 0.0, 0.0],
        [f64::NAN, f64::NAN, f64::NAN],
        [1e-200, 1e-200, 1e-200],
        [1e200, 1e200, 1e200],
    ] {
        match normalize(a) {
            None => {}
            Some(u) => panic!("normalize({a:?}) answered {u:?} instead of refusing"),
        }
    }
}

// ---------------------------------------------------------------------------
// segment_segment_distance — OP-1
// ---------------------------------------------------------------------------

/// Assert a returned closest point genuinely lies on its segment.
///
/// This is the LOWER-bound half of the correctness argument and it needs no
/// sampling: if `c1` is on segment 1 and `c2` is on segment 2, then
/// `dist(c1, c2)` is achieved by a real pair of points, so it cannot be below
/// the true minimum. Paired with the sampled upper bound below, that brackets
/// the answer from both sides without ever depending on the grid resolution.
fn assert_on_segment(c: V3, p: V3, q: V3, which: &str) {
    let d = sub(q, p);
    let len2 = dot(d, d);
    if len2 == 0.0 {
        assert_eq!(c, p, "{which}: degenerate segment, closest point must be p");
        return;
    }
    let s = dot(sub(c, p), d) / len2;
    assert!(
        (-1e-12..=1.0 + 1e-12).contains(&s),
        "{which}: parameter {s} is off the segment"
    );
    let foot = [p[0] + d[0] * s, p[1] + d[1] * s, p[2] + d[2] * s];
    let off = dist(c, foot);
    assert!(
        off <= 1e-15 * len2.sqrt().max(1.0),
        "{which}: point sits {off} off the segment line"
    );
}

/// Brute-force minimum over a sampled grid of the two segments. The sampled
/// minimum is an UPPER bound on the true minimum, so a correct closed form must
/// land at or below it. This direction is the one the retired drilling copy
/// failed: its answers were too LARGE (2.55x, on the census's worst case).
fn brute_force_min(p1: V3, q1: V3, p2: V3, q2: V3, n: usize) -> f64 {
    let mut best = f64::INFINITY;
    let d1 = sub(q1, p1);
    let d2 = sub(q2, p2);
    for i in 0..=n {
        #[allow(
            clippy::cast_precision_loss,
            reason = "loop counter <= 2000, exactly representable in f64"
        )]
        let s = i as f64 / n as f64;
        let a = [p1[0] + d1[0] * s, p1[1] + d1[1] * s, p1[2] + d1[2] * s];
        for j in 0..=n {
            #[allow(
                clippy::cast_precision_loss,
                reason = "loop counter <= 2000, exactly representable in f64"
            )]
            let t = j as f64 / n as f64;
            let b = [p2[0] + d2[0] * t, p2[1] + d2[1] * t, p2[2] + d2[2] * t];
            let d = dist(a, b);
            if d < best {
                best = d;
            }
        }
    }
    best
}

#[test]
fn segseg_returns_the_distance_between_the_points_it_returns() {
    let (d, c1, c2) = segment_segment_distance(
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.5, 1.0, 0.0],
        [0.5, 2.0, 0.0],
    );
    assert_eq!(d, dist(c1, c2));
    assert_eq!(d, 1.0);
}

#[test]
fn segseg_answers_for_zero_length_segments_instead_of_refusing() {
    // Both degenerate: point vs point.
    let (d, c1, c2) = segment_segment_distance(
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        [3.0, 4.0, 0.0],
        [3.0, 4.0, 0.0],
    );
    assert_eq!(d, 5.0);
    assert_eq!(c1, [0.0, 0.0, 0.0]);
    assert_eq!(c2, [3.0, 4.0, 0.0]);

    // First degenerate: point vs segment. The census's measured case.
    let (d, _, _) = segment_segment_distance(
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
    );
    assert_eq!(d, 1.0);

    // Second degenerate: segment vs point, and the point projects INSIDE the
    // first segment — the case a naive endpoint fallback gets wrong.
    let (d, c1, _) = segment_segment_distance(
        [0.0, 0.0, 0.0],
        [10.0, 0.0, 0.0],
        [4.0, 2.0, 0.0],
        [4.0, 2.0, 0.0],
    );
    assert_eq!(d, 2.0);
    assert_eq!(c1, [4.0, 0.0, 0.0]);
}

/// The kernel's parallel-branch discriminant, computed from the INPUTS only.
///
/// This classifies which branch a fixture exercises; it does not compute the
/// answer, and no assertion below reads its value as a result. `SEGSEG_EPS` is
/// `1e-14`; `denom <= SEGSEG_EPS` takes the `s = 0` branch.
fn segseg_denominator(p1: V3, q1: V3, p2: V3, q2: V3) -> f64 {
    let u = sub(q1, p1);
    let v = sub(q2, p2);
    dot(u, u) * dot(v, v) - dot(u, v) * dot(u, v)
}

const SEGSEG_EPS: f64 = 1e-14;

/// Measured worst-case excess of the near-parallel branch over the true minimum,
/// at drain-keepout scale (0.005-0.05 mm segments).
///
/// The A21 re-clamp is one coordinate-descent step, so on NEAR-parallel pairs
/// (as opposed to exactly parallel ones) the answer can sit slightly above the
/// true minimum. Measured at 5.145e-6 mm over 20,000 such pairs; pinned here at
/// 1e-5 mm so a regression that widens it fails, while still catching the
/// retired drilling copy's 4.494e-2 mm error 4,500x over.
const NEAR_PARALLEL_EXCESS_BOUND_MM: f64 = 1.0e-5;

#[test]
fn segseg_recovers_the_true_minimum_on_the_parallel_branch() {
    // Exactly parallel: denom = 0, so the `s = 0` branch is taken and only the
    // A21 re-clamp can produce the right answer.
    let p1 = [0.0, 0.0, 0.0];
    let q1 = [1.0, 0.0, 0.0];
    let p2 = [5.0, 3.0, 0.0];
    let q2 = [6.0, 3.0, 0.0];
    assert_eq!(
        segseg_denominator(p1, q1, p2, q2),
        0.0,
        "not the branch under test"
    );

    let (d, c1, c2) = segment_segment_distance(p1, q1, p2, q2);
    // Closest pair is (1,0,0) and (5,3,0): sqrt(16 + 9) = 5.
    assert_eq!(d, 5.0);
    // And this is the assertion the re-clamp earns: the branch STARTS at s = 0,
    // i.e. at `p1`. Only the A21 re-computation of `s` from `t` moves it to the
    // far end. A copy that dropped the re-clamp would answer c1 = p1 and
    // d = 5.831, which is exactly the shape of the retired drilling defect.
    assert_eq!(c1, q1);
    assert_eq!(c2, p2);
    assert!(c1 != p1);
}

#[test]
fn segseg_is_correct_at_drain_keepout_scale_where_the_retired_copy_was_not() {
    // Two 0.01 mm segments at 1e-4 rad — drain-keepout scale.
    //   denom = L1^2 L2^2 sin^2(theta) = 1e-4 * 1e-4 * 1e-8 = 1e-16,
    // below SEGSEG_EPS = 1e-14, so this takes the near-parallel branch: the
    // branch the canon claims still recovers the true minimum. Asserted, not
    // assumed. Verified against brute force rather than against prose.
    let len = 0.01_f64;
    let theta = 1.0e-4_f64;
    let p1 = [0.0, 0.0, 0.0];
    let q1 = [len, 0.0, 0.0];
    let p2 = [0.02, 0.002, 0.0];
    let q2 = [0.02 + len * theta.cos(), 0.002 + len * theta.sin(), 0.0];
    let denom = segseg_denominator(p1, q1, p2, q2);
    assert!(
        denom <= SEGSEG_EPS,
        "fixture denom {denom:e} is above SEGSEG_EPS, so it does NOT exercise \
         the near-parallel branch this test exists to check"
    );

    let (d, c1, c2) = segment_segment_distance(p1, q1, p2, q2);
    // The re-clamp moved `s` off its 0 start, so a copy without it would differ.
    assert!(
        c1 != p1,
        "the A21 re-clamp did not fire; fixture is not discriminating"
    );
    assert!(d.is_finite());
    assert_eq!(d, dist(c1, c2));
    // Lower bound: the answer is achieved by real points on the segments.
    assert_on_segment(c1, p1, q1, "c1");
    assert_on_segment(c2, p2, q2, "c2");

    // Upper bound: within the pinned near-parallel excess of every sampled
    // configuration. This still catches the retired copy, whose error at this
    // very scale reached 4.494e-2 mm — 4,500x the bound.
    let brute = brute_force_min(p1, q1, p2, q2, 2000);
    assert!(
        d <= brute + NEAR_PARALLEL_EXCESS_BOUND_MM,
        "closed form {d:.17e} exceeded sampled minimum {brute:.17e} by more \
         than the pinned near-parallel bound"
    );
    // And the sampled grid is fine enough that the bracket is tight.
    assert!(
        brute - d <= 1e-6,
        "closed form {d:.17e} sits {:.17e} below the sampled minimum {brute:.17e}",
        brute - d
    );
}

#[test]
fn segseg_matches_brute_force_across_a_near_parallel_sweep() {
    // The near-parallel regime, swept: every pair must land at or below its own
    // sampled minimum. Gated on the case count, not on the loop finishing.
    let mut rng = Lcg(0x5EED_1234_ABCD_0003);
    let mut cases = 0_u32;
    let mut on_parallel_branch = 0_u32;
    let mut reclamp_moved_s = 0_u32;
    for _ in 0..40 {
        let len = rng.range(0.005, 0.05);
        // Choose the angle so the mm^4 determinant lands an order of magnitude
        // BELOW SEGSEG_EPS: denom = len^4 sin^2(theta) <= 1e-15 requires
        // |theta| <= sqrt(1e-15) / len^2. Picking theta blindly puts most cases
        // on the ordinary branch and the sweep then tests nothing it claims to.
        let theta_max = 1e-15_f64.sqrt() / (len * len);
        let theta = rng.range(-theta_max, theta_max);
        let p1 = [0.0, 0.0, 0.0];
        let q1 = [len, 0.0, 0.0];
        let off = [rng.range(-0.02, 0.02), rng.range(0.001, 0.02), 0.0];
        let p2 = off;
        let q2 = [off[0] + len * theta.cos(), off[1] + len * theta.sin(), 0.0];
        if segseg_denominator(p1, q1, p2, q2) <= SEGSEG_EPS {
            on_parallel_branch += 1;
        }
        let (d, c1, c2) = segment_segment_distance(p1, q1, p2, q2);
        if c1 != p1 {
            reclamp_moved_s += 1;
        }
        assert!(d.is_finite());
        // Lower bound, resolution-free: the answer is realised on the segments.
        assert_on_segment(c1, p1, q1, "c1");
        assert_on_segment(c2, p2, q2, "c2");
        // Upper bound: no sampled configuration beats it. This is the direction
        // the retired copy failed, by up to 4.494e-2 mm at this very scale.
        let brute = brute_force_min(p1, q1, p2, q2, 400);
        assert!(
            d <= brute + NEAR_PARALLEL_EXCESS_BOUND_MM,
            "closed form {d:.17e} exceeded sampled minimum {brute:.17e} by more \
             than the pinned near-parallel bound"
        );
        cases += 1;
    }
    assert_eq!(cases, 40, "sweep did not run");
    // Gate on the PATH having been exercised, not on the loop finishing.
    assert_eq!(
        on_parallel_branch, 40,
        "only {on_parallel_branch}/40 cases reached the near-parallel branch"
    );
    assert!(
        reclamp_moved_s >= 20,
        "the A21 re-clamp moved `s` off its 0 start in only \
         {reclamp_moved_s}/40 cases, so this sweep barely distinguishes the \
         kernel from a copy that drops the re-clamp"
    );
}

#[test]
fn segseg_propagates_non_finite_input_rather_than_inventing_a_number() {
    let (d, _, _) = segment_segment_distance(
        [f64::NAN, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
    );
    assert!(!d.is_finite(), "non-finite input produced a finite {d}");
}

// ---------------------------------------------------------------------------
// ray_triangle — OP-2
// ---------------------------------------------------------------------------

#[test]
fn ray_triangle_reports_the_barycentrics_of_a_hit() {
    let v0 = [0.0, 0.0, 0.0];
    let e1 = [1.0, 0.0, 0.0];
    let e2 = [0.0, 1.0, 0.0];
    let h = ray_triangle([0.25, 0.5, 1.0], [0.0, 0.0, -1.0], v0, e1, e2, 2.0);
    assert!(h.hit);
    assert_eq!(h.t, 1.0);
    assert_eq!(h.u, 0.25);
    assert_eq!(h.v, 0.5);
}

#[test]
fn ray_triangle_projection_is_bit_identical_to_the_kernel_hot_path() {
    // `moller` must add no arithmetic over `ray_triangle`; a crate agent moving
    // to the wrapper must not move a byte.
    let mut rng = Lcg(0x5EED_1234_ABCD_0004);
    let mut checked = 0_u32;
    for _ in 0..100_000 {
        let v0: V3 = [
            rng.range(-20.0, 20.0),
            rng.range(-20.0, 20.0),
            rng.range(-20.0, 20.0),
        ];
        let e1: V3 = [
            rng.range(-5.0, 5.0),
            rng.range(-5.0, 5.0),
            rng.range(-5.0, 5.0),
        ];
        let e2: V3 = [
            rng.range(-5.0, 5.0),
            rng.range(-5.0, 5.0),
            rng.range(-5.0, 5.0),
        ];
        let o: V3 = [
            rng.range(-20.0, 20.0),
            rng.range(-20.0, 20.0),
            rng.range(-20.0, 20.0),
        ];
        let dv: V3 = [
            rng.range(-1.0, 1.0),
            rng.range(-1.0, 1.0),
            rng.range(-1.0, 1.0),
        ];
        let h = ray_triangle(o, dv, v0, e1, e2, 50.0);
        let (hit, t) = kern_moller(o, dv, v0, e1, e2, 50.0);
        assert_eq!(h.hit, hit);
        assert_eq!(h.t.to_bits(), t.to_bits());
        checked += 1;
    }
    assert_eq!(checked, 100_000, "sweep did not run");
}

#[test]
fn ray_triangle_misses_a_degenerate_triangle_with_all_outputs_exactly_zero() {
    let v0 = [0.0, 0.0, 0.0];
    // e1 parallel to e2: zero-area triangle.
    let h = ray_triangle(
        [0.25, 0.25, 1.0],
        [0.0, 0.0, -1.0],
        v0,
        [1.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        2.0,
    );
    assert!(!h.hit);
    assert_eq!(h.t, 0.0);
    assert_eq!(h.u, 0.0);
    assert_eq!(h.v, 0.0);

    // Collapsed triangle: e2 is the zero vector.
    let h = ray_triangle(
        [0.25, 0.25, 1.0],
        [0.0, 0.0, -1.0],
        v0,
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        2.0,
    );
    assert!(!h.hit);
    assert_eq!((h.t, h.u, h.v), (0.0, 0.0, 0.0));
}

#[test]
fn ray_triangle_misses_a_ray_parallel_to_the_face_plane() {
    let h = ray_triangle(
        [-1.0, 0.25, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        10.0,
    );
    assert!(!h.hit);
    assert_eq!((h.t, h.u, h.v), (0.0, 0.0, 0.0));
}

#[test]
fn ray_triangle_misses_a_zero_length_direction_without_dividing_by_zero() {
    let h = ray_triangle(
        [0.25, 0.25, 1.0],
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        2.0,
    );
    assert!(!h.hit);
    assert_eq!((h.t, h.u, h.v), (0.0, 0.0, 0.0));
}

#[test]
fn ray_triangle_registers_a_crossing_that_lands_exactly_on_a_shared_edge() {
    // The C5 property, stated about the SURVIVOR: a ray passing exactly through
    // an edge shared by two faces must register on at least one of them. A
    // zero-slack acceptance test can miss BOTH, which silently flips parity for
    // every point beyond the crossing — that, not the band arithmetic, is the
    // defect the canon's EPS = 1e-6 removes.
    //
    // Fixture: a random shared edge P->Q with opposite corners R and S, so
    // face A = (P, Q, R) and face B = (P, S, Q). The query ray is aimed from a
    // random origin at a point computed ON the shared edge, so every crossing
    // is an exact-edge crossing evaluated through inexact arithmetic. Nice
    // axis-aligned geometry will NOT exercise this: it makes the barycentric
    // land on exactly 0.0, which even a zero-slack rule accepts.
    let mut rng = Lcg(0x5EED_1234_ABCD_0005);
    let mut checked = 0_u32;
    let mut missed_both = 0_u32;
    // Cases where the crossing is formally OUTSIDE both triangles and only the
    // slack rescues it. Counting these is what proves the fixture can see the
    // defect at all, rather than passing because nothing was exercised.
    let mut outside_both_yet_registered = 0_u32;
    for _ in 0..50_000 {
        let p: V3 = [
            rng.range(-20.0, 20.0),
            rng.range(-20.0, 20.0),
            rng.range(-20.0, 20.0),
        ];
        let q: V3 = [
            rng.range(-20.0, 20.0),
            rng.range(-20.0, 20.0),
            rng.range(-20.0, 20.0),
        ];
        let r: V3 = [
            rng.range(-20.0, 20.0),
            rng.range(-20.0, 20.0),
            rng.range(-20.0, 20.0),
        ];
        let s: V3 = [
            rng.range(-20.0, 20.0),
            rng.range(-20.0, 20.0),
            rng.range(-20.0, 20.0),
        ];
        // Reconstructing edges from vertices is the documented caller pattern:
        // vec3::sub, the same subtraction the kernel's own surface build uses.
        let a_e1 = sub(q, p);
        let a_e2 = sub(r, p);
        let b_e1 = sub(s, p);
        let b_e2 = sub(q, p);

        let k = rng.range(0.05, 0.95);
        let edge_point = [
            p[0] + (q[0] - p[0]) * k,
            p[1] + (q[1] - p[1]) * k,
            p[2] + (q[2] - p[2]) * k,
        ];
        let o: V3 = [
            rng.range(-40.0, 40.0),
            rng.range(-40.0, 40.0),
            rng.range(-40.0, 40.0),
        ];
        // Non-unit `dv`: the crossing is at t = 1 in |dv| units, which also
        // exercises the documented "t is in units of |dv|" contract.
        let dv = sub(edge_point, o);

        let ha = ray_triangle(o, dv, p, a_e1, a_e2, 2.0);
        let hb = ray_triangle(o, dv, p, b_e1, b_e2, 2.0);
        if !ha.hit && !hb.hit {
            missed_both += 1;
        }
        if ha.hit && hb.hit {
            let a_in = ha.u.min(ha.v).min(1.0 - ha.u - ha.v);
            let b_in = hb.u.min(hb.v).min(1.0 - hb.u - hb.v);
            if a_in < 0.0 && b_in < 0.0 {
                outside_both_yet_registered += 1;
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 50_000, "sweep did not run");
    assert_eq!(
        missed_both, 0,
        "{missed_both} shared-edge crossings registered on NEITHER face"
    );
    assert!(
        outside_both_yet_registered > 1_000,
        "only {outside_both_yet_registered} crossings landed formally outside \
         both faces, so this fixture cannot tell the slack from no slack — a \
         zero-slack rule would pass it too. Fix the fixture, not the threshold."
    );
}

#[test]
fn ray_triangle_honours_the_t_max_bound_with_its_documented_slack() {
    let v0 = [0.0, 0.0, 0.0];
    let e1 = [1.0, 0.0, 0.0];
    let e2 = [0.0, 1.0, 0.0];
    let o = [0.25, 0.25, 1.0];
    let dv = [0.0, 0.0, -1.0];
    // The plane is hit at t = 1.0.
    assert!(ray_triangle(o, dv, v0, e1, e2, 1.0).hit);
    // Just inside the +EPS slack.
    assert!(ray_triangle(o, dv, v0, e1, e2, 1.0 - 5.0e-7).hit);
    // Clear of the slack: a miss.
    assert!(!ray_triangle(o, dv, v0, e1, e2, 0.999).hit);
}

#[test]
fn ray_triangle_misses_on_non_finite_input_rather_than_claiming_a_hit() {
    let h = ray_triangle(
        [f64::NAN, 0.25, 1.0],
        [0.0, 0.0, -1.0],
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        2.0,
    );
    assert!(!h.hit);
}

// ---------------------------------------------------------------------------
// poly2::point_segment_distance — OP-3 / C6
// ---------------------------------------------------------------------------

#[test]
fn degenerate_edge_threshold_is_half_a_quantum_squared() {
    assert_eq!(SLICE_QUANTUM_MM, 1.0e-8);
    assert_eq!(
        DEGENERATE_EDGE_L2_MM2,
        0.25 * SLICE_QUANTUM_MM * SLICE_QUANTUM_MM
    );
    // The knife-edge property the canon requires: no representable input sits
    // near the threshold. The smallest REAL edge is one quantum, whose squared
    // length is 4x the threshold; the only degenerate input is exactly 0.
    let one_quantum_l2 = SLICE_QUANTUM_MM * SLICE_QUANTUM_MM;
    assert!(one_quantum_l2 >= 4.0 * DEGENERATE_EDGE_L2_MM2 * (1.0 - 1.0e-12));
    const { assert!(DEGENERATE_EDGE_L2_MM2 > 0.0) };
}

#[test]
fn point_segment_distance_treats_a_coincident_pair_as_a_point() {
    let d = point_segment_distance([1.0, 0.0], [0.0, 0.0], [0.0, 0.0]);
    assert_eq!(d, 1.0);
    assert!(d.is_finite(), "a zero-length edge must not produce NaN/inf");
}

#[test]
fn point_segment_distance_treats_a_sub_half_quantum_edge_as_a_point() {
    // 0.4 quantum: below the threshold, so the answer is the distance to `a`.
    let b = [0.4 * SLICE_QUANTUM_MM, 0.0];
    let d = point_segment_distance([1.0, 0.0], [0.0, 0.0], b);
    assert_eq!(d, 1.0);
}

#[test]
fn point_segment_distance_projects_onto_a_real_one_quantum_edge() {
    // A 1-quantum edge is a REAL edge to the union normalizer that emitted it,
    // so the consumer must project onto it rather than call it a point.
    let b = [SLICE_QUANTUM_MM, 0.0];
    let d = point_segment_distance([1.0, 0.0], [0.0, 0.0], b);
    assert!(d < 1.0, "a real 1-quantum edge was treated as a point");
    assert!((d - (1.0 - SLICE_QUANTUM_MM)).abs() < 1.0e-16);
}

#[test]
fn point_segment_distance_projects_onto_the_censuss_three_quantum_edge() {
    // The worked case in the census: a 3-quantum edge queried from (1, 0).
    // The correct answer is the projection, 0.99999997, not the endpoint
    // distance 1.0.
    let b = [3.0 * SLICE_QUANTUM_MM, 0.0];
    let d = point_segment_distance([1.0, 0.0], [0.0, 0.0], b);
    assert!(d != 1.0);
    assert!(
        (d - 9.999_999_7e-1).abs() < 1.0e-16,
        "3-quantum projection measured {d:.17e}"
    );
}

#[test]
fn point_segment_distance_is_zero_at_the_endpoints_and_never_negative() {
    assert_eq!(
        point_segment_distance([0.0, 0.0], [0.0, 0.0], [1.0, 0.0]),
        0.0
    );
    assert_eq!(
        point_segment_distance([1.0, 0.0], [0.0, 0.0], [1.0, 0.0]),
        0.0
    );

    let mut rng = Lcg(0x5EED_1234_ABCD_0006);
    let mut checked = 0_u32;
    for _ in 0..200_000 {
        let p = [rng.range(-60.0, 60.0), rng.range(-60.0, 60.0)];
        let a = [rng.range(-60.0, 60.0), rng.range(-60.0, 60.0)];
        // Include near-degenerate edges: log-uniform lengths down to 1e-12 mm.
        let len = 10.0_f64.powf(rng.range(-12.0, 1.0));
        let ang = rng.range(0.0, std::f64::consts::TAU);
        let b = [a[0] + len * ang.cos(), a[1] + len * ang.sin()];
        let d = poly2::point_segment_distance(p, a, b);
        assert!(
            d >= 0.0 && d.is_finite(),
            "d = {d} for p={p:?} a={a:?} b={b:?}"
        );
        checked += 1;
    }
    assert_eq!(checked, 200_000, "sweep did not run");
}

// ---------------------------------------------------------------------------
// poly2 polygon primitives — OP-8 / C2
// ---------------------------------------------------------------------------

#[test]
fn signed_area_of_a_sub_triangle_loop_is_exactly_zero_and_therefore_sign_free() {
    assert_eq!(signed_area(&[]), 0.0);
    assert_eq!(signed_area(&[[1.0, 1.0]]), 0.0);
    assert_eq!(signed_area(&[[0.0, 0.0], [1.0, 0.0]]), 0.0);
    // Collinear triple: an area, but no winding.
    assert_eq!(signed_area(&[[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]]), 0.0);
}

#[test]
fn signed_area_sign_follows_winding() {
    let ccw = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mut cw = ccw;
    cw.reverse();
    assert_eq!(signed_area(&ccw), 1.0);
    assert_eq!(signed_area(&cw), -1.0);
}

#[test]
fn point_in_polygon_is_false_for_a_polygon_that_cannot_enclose_anything() {
    assert!(!point_in_polygon([0.5, 0.5], &[]));
    assert!(!point_in_polygon([0.5, 0.5], &[[0.0, 0.0]]));
    assert!(!point_in_polygon([0.5, 0.5], &[[0.0, 0.0], [1.0, 1.0]]));
}

#[test]
fn point_in_polygon_separates_interior_from_exterior_and_ignores_winding() {
    let ccw = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mut cw = ccw;
    cw.reverse();
    assert!(point_in_polygon([0.5, 0.5], &ccw));
    assert!(point_in_polygon([0.5, 0.5], &cw));
    assert!(!point_in_polygon([1.5, 0.5], &ccw));
    assert!(!point_in_polygon([-0.5, 0.5], &ccw));
    assert!(!point_in_polygon([0.5, 1.5], &ccw));
}

#[test]
fn point_in_polygon_on_boundary_is_unspecified_but_pinned_to_these_answers() {
    // The canon calls on-boundary behaviour unspecified-but-pinned, and warns
    // that a caller must not read a verdict off this function alone for points
    // that can sit on the boundary. "Unspecified" is not "untested": the exact
    // incumbent answers are recorded here so a future edit that changes them is
    // visible, and so a crate agent can see for itself that the two sides of
    // the same square disagree — which is the reason for the warning.
    //
    // The horizontal edges also exercise the no-division path: the
    // `(a[1] > y) != (b[1] > y)` guard is false before the divide, so a query
    // at exactly the edge's y cannot divide by zero.
    let square = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let pinned: [([f64; 2], bool); 6] = [
        ([0.5, 0.0], true),  // on the bottom (horizontal) edge
        ([0.5, 1.0], false), // on the top (horizontal) edge
        ([0.0, 0.5], true),  // on the left edge
        ([1.0, 0.5], false), // on the right edge
        ([0.0, 0.0], true),  // on a corner
        ([1.0, 1.0], false), // on the opposite corner
    ];
    let mut checked = 0_u32;
    for (p, expected) in pinned {
        assert_eq!(
            point_in_polygon(p, &square),
            expected,
            "on-boundary answer moved for {p:?}"
        );
        checked += 1;
    }
    assert_eq!(checked, 6, "case table did not run");

    // The interior answer is unaffected by any of it.
    assert!(point_in_polygon([0.5, 0.25], &square));
}

#[test]
fn segments_intersect_pins_every_contact_case_the_strict_form_decides() {
    // Each row is (label, expected, a, b, c, d). The strict `>` makes a
    // collinear triple read as "not counter-clockwise", and these are the
    // consequences a caller must know — in particular that a T-junction is
    // `true` while a shared endpoint is `false`, so "touching is excluded" is
    // NOT a correct summary of this function.
    let cases: [IntersectCase; 12] = [
        (
            "proper crossing",
            true,
            [0.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
            [1.0, 0.0],
        ),
        (
            "proper crossing, roles swapped",
            true,
            [0.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0],
            [1.0, 1.0],
        ),
        (
            "T: c strictly inside ab",
            true,
            [0.0, 0.0],
            [2.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
        ),
        (
            "T: d strictly inside ab",
            true,
            [0.0, 0.0],
            [2.0, 0.0],
            [1.0, 1.0],
            [1.0, 0.0],
        ),
        (
            "T: a strictly inside cd",
            true,
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 0.0],
            [2.0, 0.0],
        ),
        (
            "shared endpoint b == c",
            false,
            [0.0, 0.0],
            [1.0, 1.0],
            [1.0, 1.0],
            [2.0, 0.0],
        ),
        (
            "shared endpoint a == c",
            false,
            [0.0, 0.0],
            [1.0, 1.0],
            [0.0, 0.0],
            [2.0, 0.0],
        ),
        (
            "collinear overlap",
            false,
            [0.0, 0.0],
            [2.0, 0.0],
            [1.0, 0.0],
            [3.0, 0.0],
        ),
        (
            "collinear identical",
            false,
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 0.0],
            [1.0, 0.0],
        ),
        (
            "collinear disjoint",
            false,
            [0.0, 0.0],
            [1.0, 0.0],
            [2.0, 0.0],
            [3.0, 0.0],
        ),
        (
            "zero-length ON the other segment",
            false,
            [1.0, 0.5],
            [1.0, 0.5],
            [0.0, 0.0],
            [2.0, 1.0],
        ),
        (
            "parallel disjoint",
            false,
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 1.0],
            [1.0, 1.0],
        ),
    ];
    let mut checked = 0_u32;
    for (label, expected, a, b, c, d) in cases {
        assert_eq!(
            segments_intersect(a, b, c, d),
            expected,
            "segments_intersect disagreed with the pinned contract for {label}"
        );
        checked += 1;
    }
    assert_eq!(checked, 12, "case table did not run");
}

#[test]
fn segments_intersect_is_false_on_non_finite_input() {
    assert!(!segments_intersect(
        [f64::NAN, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
        [1.0, 0.0]
    ));
}
