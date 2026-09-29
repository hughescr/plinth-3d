//! The complete segment-vs-segment minimisation: the counterexample it was
//! built for, and a dense-sweep oracle that is not the code under test.
//!
//! # The defect
//!
//! `segment_segment_distance` is parity-pinned term for term. When
//! `denom = |u|^2|v|^2 - (u.v)^2 <= 1e-14` it takes `s = 0` and recovers with
//! ONE coordinate-descent step. For an exactly parallel pair that step reaches
//! the minimum. For a merely NEAR-parallel pair it does not, and the error is
//! one-sided HIGH: the function returns the distance between two points that
//! really do lie on the segments, so it can only ever OVERestimate. An
//! overestimated gap turns a refusal into an acceptance.
//!
//! `PRE-FIX BEHAVIOUR, MEASURED` (run against `487e71f` before this change, on
//! the pair pinned below): the pinned function returned
//! `9.87189037462355423e-2` mm where the complete minimisation gives
//! `9.87180933769562400e-2` mm — an excess of `8.10369279305445112e-7` mm,
//! **8.1x the `PROOF_EPSILON_MM = 1e-7` slack** of the drilling comparison that
//! value fed. The assertion below fails against that answer and passes against
//! the complete one.
//!
//! # Why the oracle here is a dense sweep
//!
//! A test must not re-implement the production logic it checks. The candidate
//! enumeration under test is a closed-form argument about where a convex
//! quadratic attains its minimum on a box; the oracle is a brute-force scan of
//! the box on a lattice. They share no reasoning, so agreement between them is
//! evidence rather than tautology. A sweep is a strict UPPER bound on the true
//! minimum (every sample is an achievable distance), so the direction of the
//! comparison matters and is stated at each use.

// Imported THROUGH THE CRATE ROOT on purpose: this also proves the export
// exists, which is what a previous revision of the specification omitted.
use geom_kernel::{segment_segment_distance, segment_segment_distance_min};

type V3 = [f64; 3];

/// The reviewer's counterexample, re-measured in the adjudication.
const P1: V3 = [0.0, 0.0, 0.0];
const Q1: V3 = [0.107_809_924_939_717_08, 0.0, 0.0];
const P2: V3 = [
    0.017_559_309_679_091_19,
    -0.098_053_488_880_395_89,
    -0.011_442_695_278_674_364,
];
const Q2: V3 = [
    0.118_371_877_206_833_15,
    -0.098_052_577_535_768_63,
    -0.011_442_695_278_674_364,
];

/// The complete minimum for that pair, at `s = 1`, `t = 0.8952405822914677`.
const TRUE_MINIMUM_MM: f64 = 0.098_718_093_376_956_24;

/// What the pinned one-step recovery returns on the same pair.
const RETIRED_ONE_STEP_MM: f64 = 0.098_718_903_746_235_54;

fn dist(a: V3, b: V3) -> f64 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    d[0].mul_add(d[0], d[1].mul_add(d[1], d[2] * d[2])).sqrt()
}

fn lerp(a: V3, b: V3, t: f64) -> V3 {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Brute-force minimum over an `n x n` lattice of the `(s, t)` unit square.
///
/// Structurally independent of the code under test: no denominators, no
/// candidate enumeration, no convexity argument — just samples. Every sample is
/// a real point pair, so the result is an UPPER bound on the true minimum.
fn dense_sweep(p1: V3, q1: V3, p2: V3, q2: V3, n: usize) -> f64 {
    let mut best = f64::INFINITY;
    for i in 0..=n {
        let s = i as f64 / n as f64;
        let a = lerp(p1, q1, s);
        for j in 0..=n {
            let t = j as f64 / n as f64;
            let d = dist(a, lerp(p2, q2, t));
            if d < best {
                best = d;
            }
        }
    }
    best
}

/// **THE PIN.** The complete minimisation reaches the true minimum on the pair
/// that defeats the one-step recovery.
///
/// The second assertion is the regression guard with teeth: it fails if the
/// one-step branch ever comes back, with margin to spare against the 8.10e-7 mm
/// excess it produces.
#[test]
fn the_near_parallel_counterexample_is_measured_by_complete_minimisation() {
    let (measured, c1, c2) = segment_segment_distance_min(P1, Q1, P2, Q2);
    assert!(
        (measured - TRUE_MINIMUM_MM).abs() <= 1.0e-12,
        "complete minimisation must reach {TRUE_MINIMUM_MM:.17e}, got {measured:.17e}"
    );
    assert!(
        measured < RETIRED_ONE_STEP_MM - 5.0e-7,
        "the retired one-step answer was {RETIRED_ONE_STEP_MM:.17e}, 8.1037e-7 mm \
         above the true minimum and past the 1e-7 mm slack of the comparison it \
         fed; got {measured:.17e}"
    );

    // The return contract: the distance IS `dist` of the two returned points, so
    // a caller never recomputes it and can never disagree with it.
    assert!(
        (dist(c1, c2) - measured).abs() <= 1.0e-15 * measured.max(1.0),
        "returned distance must be the distance between the returned points"
    );

    // And the pinned function still answers the way the parity path needs it
    // to. If this changes, the pin moved and every parity assertion in the
    // suite is suspect.
    let (pinned, _, _) = segment_segment_distance(P1, Q1, P2, Q2);
    assert!(
        (pinned - RETIRED_ONE_STEP_MM).abs() <= 1.0e-15,
        "the parity pin is not edited by this change: expected \
         {RETIRED_ONE_STEP_MM:.17e}, got {pinned:.17e}"
    );
}

/// The same pair against the independent oracle, at the resolution the
/// adjudication used.
#[test]
fn the_counterexample_agrees_with_a_dense_sweep() {
    let (measured, _, _) = segment_segment_distance_min(P1, Q1, P2, Q2);
    let swept = dense_sweep(P1, Q1, P2, Q2, 4001);
    // The sweep is an upper bound, so the minimisation must not EXCEED it...
    assert!(
        measured <= swept + 1.0e-15,
        "the minimisation ({measured:.17e}) exceeded a lattice sample ({swept:.17e}), \
         which means it missed a point that really exists on the segments"
    );
    // ...and must land within a lattice step of it, or it is not the minimum. A
    // 4001x4001 sweep resolves the (s, t) square to 2.5e-4, and the adjudication
    // measured this sweep at 9.871809338152179e-2 — 4.6e-11 above the true
    // minimum. The 1e-9 bound below is therefore three orders of magnitude
    // tighter than the 8.1e-7 mm defect and two orders looser than sweep
    // resolution: it can see the defect and cannot fail on lattice coarseness.
    assert!(
        swept - measured < 1.0e-9,
        "the sweep found {swept:.17e} and the minimisation {measured:.17e}"
    );
}

/// A deterministic near-parallel battery at drain-keepout scale, against the
/// sweep oracle, with the pinned function's excess measured alongside.
///
/// Counted rows: the battery must actually PRODUCE near-parallel pairs (the
/// ones that fire the pinned function's `s = 0` branch), or it proves nothing
/// about the branch it exists to retire.
#[test]
fn a_near_parallel_battery_matches_a_dense_sweep() {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut next = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 11) as f64 / (1_u64 << 53) as f64
    };

    let mut near_parallel_rows = 0_u32;
    let mut worst_min_vs_sweep = 0.0_f64;
    let mut worst_pinned_excess = 0.0_f64;

    for _ in 0..300 {
        // A base direction, and a second segment whose direction is the same one
        // perturbed by ~1e-7 — near parallel by construction, not by luck.
        let len = 0.005 + next() * 0.045;
        let dir = {
            let raw = [next() * 2.0 - 1.0, next() * 2.0 - 1.0, next() * 2.0 - 1.0];
            let n = dist([0.0; 3], raw).max(1.0e-12);
            [raw[0] / n, raw[1] / n, raw[2] / n]
        };
        let wobble = 1.0e-7;
        let dir2 = {
            let raw = [
                dir[0] + (next() * 2.0 - 1.0) * wobble,
                dir[1] + (next() * 2.0 - 1.0) * wobble,
                dir[2] + (next() * 2.0 - 1.0) * wobble,
            ];
            let n = dist([0.0; 3], raw).max(1.0e-12);
            [raw[0] / n, raw[1] / n, raw[2] / n]
        };
        let p1 = [next() * 0.1, next() * 0.1, next() * 0.1];
        let q1 = [
            p1[0] + dir[0] * len,
            p1[1] + dir[1] * len,
            p1[2] + dir[2] * len,
        ];
        let offset = 0.02 + next() * 0.08;
        let p2 = [
            p1[0] + dir[1] * offset + next() * 0.02,
            p1[1] - dir[0] * offset + next() * 0.02,
            p1[2] + next() * 0.02,
        ];
        let len2 = 0.005 + next() * 0.045;
        let q2 = [
            p2[0] + dir2[0] * len2,
            p2[1] + dir2[1] * len2,
            p2[2] + dir2[2] * len2,
        ];

        // Does this pair fire the pinned function's parallel branch? Computed
        // from the pair's own geometry, not read out of either function.
        let d1 = [q1[0] - p1[0], q1[1] - p1[1], q1[2] - p1[2]];
        let d2 = [q2[0] - p2[0], q2[1] - p2[1], q2[2] - p2[2]];
        let dot = |a: V3, b: V3| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let denom = dot(d1, d1) * dot(d2, d2) - dot(d1, d2) * dot(d1, d2);
        if denom <= 1.0e-14 {
            near_parallel_rows += 1;
        }

        let (measured, c1, c2) = segment_segment_distance_min(p1, q1, p2, q2);
        assert!(
            (dist(c1, c2) - measured).abs() <= 1.0e-12 * measured.max(1.0),
            "return contract broken on a battery row"
        );

        let swept = dense_sweep(p1, q1, p2, q2, 601);
        assert!(
            measured <= swept + 1.0e-15,
            "the minimisation exceeded an achievable lattice sample: \
             {measured:.17e} > {swept:.17e}"
        );
        worst_min_vs_sweep = worst_min_vs_sweep.max(swept - measured);

        let (pinned, _, _) = segment_segment_distance(p1, q1, p2, q2);
        worst_pinned_excess = worst_pinned_excess.max(pinned - measured);
    }

    // COUNTED ROW. Without this the battery could be all well-conditioned pairs
    // and the whole test would be checking nothing about the near-parallel case.
    assert!(
        near_parallel_rows >= 100,
        "the battery must exercise the pinned function's parallel branch; only \
         {near_parallel_rows} of 300 rows fired it"
    );
    // The sweep is a coarse upper bound, so a gap of a lattice step is expected;
    // what must not happen is the minimisation landing ABOVE it, asserted above.
    assert!(
        worst_min_vs_sweep < 1.0e-4,
        "the minimisation should track a 601x601 sweep closely, worst gap \
         {worst_min_vs_sweep:.6e}"
    );
    println!(
        "battery: near_parallel_rows={near_parallel_rows}/300 \
         worst(sweep-min)={worst_min_vs_sweep:.6e} \
         worst(pinned-min)={worst_pinned_excess:.6e}"
    );
}

/// Degenerate inputs, where the exact `a > 0.0` / `e > 0.0` guards replace the
/// pinned function's mm^2 epsilon.
///
/// The last case is the one the name split (§O2) is about: a genuinely nonzero
/// segment of length 1e-7 mm has `a = 9.999999999999998e-15`, which FAILS the
/// pinned function's `a > 1e-14` test and is treated as a point. The complete
/// minimisation projects onto it, because its guard is exact.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "exact pins over hand-written fixture values: a point-vs-point pair \
              must return its own two input points bit for bit, and \"close to \
              the input\" would not be the property under test"
)]
fn degenerate_and_tiny_segments_are_handled_by_exact_guards() {
    // Both segments are points.
    let (d, c1, c2) =
        segment_segment_distance_min([0.0; 3], [0.0; 3], [3.0, 4.0, 0.0], [3.0, 4.0, 0.0]);
    assert!(
        (d - 5.0).abs() <= 1.0e-15,
        "point vs point is just the distance"
    );
    // Exact: a point-vs-point pair must return its own two input points, not
    // something a rounding away from them.
    assert!(
        c1 == [0.0; 3],
        "returned point drifted off the input: {c1:?}"
    );
    assert!(
        c2 == [3.0, 4.0, 0.0],
        "returned point drifted off the input: {c2:?}"
    );

    // First segment is a point, second is real: point-vs-segment.
    let (d, _, _) =
        segment_segment_distance_min([0.5, 1.0, 0.0], [0.5, 1.0, 0.0], [0.0; 3], [1.0, 0.0, 0.0]);
    assert!(
        (d - 1.0).abs() <= 1.0e-15,
        "perpendicular drop onto the segment"
    );

    // Crossing segments: the minimum is 0 and lies in the interior, which is the
    // candidate the boundary edges alone cannot find.
    let (d, _, _) = segment_segment_distance_min(
        [-1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 1.0, 0.0],
    );
    assert!(d <= 1.0e-15, "crossing segments meet, got {d:e}");

    // A 1e-7 mm segment is REAL, and gets projected onto rather than collapsed.
    // The query point sits 1 mm away, perpendicular to the segment's midpoint,
    // so the true answer is 1.0 and the collapsed answer would be
    // sqrt(1 + (5e-8)^2), which differs from 1.0 by ~1.25e-15 — below f64
    // resolution at this magnitude. The observable difference is therefore the
    // returned POINT, which must lie strictly inside the tiny segment.
    let tiny_p = [0.0, 0.0, 0.0];
    let tiny_q = [1.0e-7, 0.0, 0.0];
    let (_, _, on_tiny) =
        segment_segment_distance_min([5.0e-8, 1.0, 0.0], [5.0e-8, 1.0, 0.0], tiny_p, tiny_q);
    assert!(
        on_tiny[0] > 0.0 && on_tiny[0] < 1.0e-7,
        "a 1e-7 mm segment is a real segment and must be projected onto, not \
         treated as its own start point; got {on_tiny:?}"
    );
}
