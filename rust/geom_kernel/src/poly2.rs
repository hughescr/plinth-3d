//! 2D polygon and segment primitives on `[f64; 2]` points, in millimetres.
//!
//! # Why this module exists
//!
//! The kernel had no 2D module at all, so the four primitives below were
//! hand-copied into `plinth-support::perception`, `plinth-support::specialized`,
//! `plinth-layers`, and `plinth-layers`'s test helper. Three of those copies are
//! byte-identical text; the fourth,
//! [`point_segment_distance`], is where they silently disagreed — three
//! different degeneracy rules on the SAME slice data, differing by a factor of
//! 3.16e7 in where they decide an edge has no length
//! (`docs/specs/geometry-canon.md` §1 OP-3, OP-8; plan items C2 and C6).
//!
//! `plinth-layers/src/lib.rs` already recorded the whole problem in a doc
//! comment, correctly including the part people get wrong: opening
//! `geom_kernel::vec3` did not and could not close the 2D gap, because `vec3`
//! is 3-vector arithmetic on `[f64; 3]`, not 2D polygon primitives. This module
//! is that home.
//!
//! # Provenance of each body
//!
//! Every function here is the incumbent text, moved rather than rewritten, so
//! the move is byte-neutral and the ONE behaviour change (the degeneracy rule
//! in [`point_segment_distance`]) is separately attributable. Each item names
//! the source it was taken from.
//!
//! # Module-wide contract
//!
//! * Coordinates are millimetres; returned distances and areas are mm and mm².
//! * Nothing here allocates, panics, or has a fallible path. Every function is
//!   total over finite input.
//! * **Non-finite input is not guarded** and propagates, except where an item
//!   says otherwise. A caller that must refuse non-finite input does so at its
//!   own call site, in its own error vocabulary.
//! * These are the SLICE-LATTICE primitives. [`point_segment_distance`]'s
//!   degeneracy threshold is derived from the slice quantum
//!   ([`SLICE_QUANTUM_MM`]); data on a different lattice needs its own wrapper
//!   with its own derivation, which is exactly why
//!   `plinth-support::joints::segment_distance_2d` — `(axial, radial)`
//!   frustum-profile data, not slice data — stays where it is
//!   (canon §4 item 5).

/// Mirror of `plinth_layers::SLICE_UNION_QUANTUM_MM`, in mm.
///
/// The dependency points the other way (`plinth-layers` depends on
/// `geom_kernel`, not vice versa), so this crate cannot import the original. A
/// cross-crate pin test in `plinth-layers` asserts
/// `geom_kernel::poly2::SLICE_QUANTUM_MM == SLICE_UNION_QUANTUM_MM` so the pair
/// cannot drift silently — the same pattern that already ties that quantum to
/// perception's stitch key `LOOP_MERGE_MM`.
///
/// All slice-loop coordinates reaching this module are `round(value / 1e-8)`
/// i64 lattice points dequantized back to `f64`.
pub const SLICE_QUANTUM_MM: f64 = 1.0e-8;

/// Squared length below which a 2D slice edge is treated as a point, in mm².
///
/// An edge shorter than HALF a lattice quantum cannot have come from two
/// distinct lattice points; it is a point to the data that produced it.
///
/// # The justification, rewritten (§O3 of `docs/specs/geometry-adjudication.md`)
///
/// **The lattice premise this constant used to be derived from is false, and
/// it was tested, not argued.** The old text claimed every input is a
/// dequantised lattice point, so two distinct inputs are at least one quantum
/// apart and "no representable input sits near" the threshold. That is not
/// true of the PASS-THROUGH path: the stitcher welds by quantised KEY but
/// stores the first ORIGINAL coordinate per key
/// (`plinth-support::perception`, `points.entry(ka).or_insert(a)` — slicer
/// output, not lattice values), and the normalizer's pass-through arm emits
/// those originals verbatim. Non-lattice coordinates are the NORM on clean
/// layers, not an upstream defect. Measured witness (§O3, and
/// `plinth-layers/tests/passthrough_verbatim.rs`): two vertices in adjacent
/// rounding cells `2.0000000000006135e-12` mm apart survive normalization
/// bit-identical and form a real edge whose `l2 = 4.000000000002454e-24` fires
/// this branch. Pass-through edges can sit ANYWHERE in `(0, one quantum)`,
/// including at the threshold.
///
/// **The behaviour nevertheless stays, on a bound rather than on
/// representability.** Two facts justify it, and both are properties of the
/// branch itself:
///
/// 1. **Bounded error.** Taking the point fallback answers `dist(p, a)` where
///    the truth is `dist(p, segment ab)`, and those differ by at most `|ab|`.
///    Every edge that can reach this branch has `|ab| < 5e-9` mm — six orders
///    of magnitude below the 14 µm pixel, at the scale of ordinary `f64`
///    rounding in the consumers. Measured on the witness edge: predicate
///    answer `0.49999999500100001` vs true minimum `0.49999999499900000`,
///    error `2.00001e-12` mm, bounded by `|ab| = 2.00000e-12` mm plus
///    ULP-scale rounding of the two distances.
/// 2. **No division occurs on the branch**, so no `inf` and no `NaN` can
///    escape it. On any non-lattice input a subnormal `l2` would survive an
///    `== 0.0` test and then `t = dot / l2` could produce one; a `NaN`
///    distance is silently dropped by the `f64::min` reach-fold — a value
///    meaning "measured nothing" read as "nothing nearby", the exact failure
///    class this project keeps reproducing. That robustness property is real;
///    it just holds for a different reason than the one written down.
///
/// So the threshold is a KNIFE EDGE, and that is harmless rather than absent:
/// both branches agree to within `5e-9` mm wherever an input lands near it.
/// The value is a **chosen behavior pin** at `(Q/2)² = 2.5e-17` — half a
/// quantum squared, four times below the smallest LATTICE edge, which is why
/// a lattice-only producer never reaches the branch at all. What is derived is
/// the error BOUND, not the number.
///
/// For context on the three incumbent rules this replaced — `< 1.0e-15`
/// (perception, i.e. `|ab| < 3.162e-8` mm), `== 0.0` (specialized), and
/// `< 1e-18` (joints): no printer-physics scale can choose between them, since
/// the finest feature the machine resolves is the 14 µm pixel / 50 µm layer,
/// six or more orders of magnitude above all three. Perception's `3.162e-8`
/// threshold called edges the producer asserts exist "points", so two stages
/// disagreed about whether a thing exists; at `2.5e-17` a 1-quantum edge is a
/// real edge and is projected onto.
pub const DEGENERATE_EDGE_L2_MM2: f64 = 0.25 * SLICE_QUANTUM_MM * SLICE_QUANTUM_MM;

/// Distance in mm from `point` to the segment `a`–`b`, by project-and-clamp.
///
/// **Canonical for slice-loop 2D distance** (`docs/specs/geometry-canon.md` §1
/// OP-3, plan item C6). Replaces `plinth-support::perception`'s
/// `point_segment_distance` and `plinth-support::specialized`'s
/// `segment_distance`, which walk the same `SliceSolid` loops and both answer
/// "is this point within reach of that boundary" — so they are required to
/// agree, and did not.
///
/// Provenance: the body is the text all three incumbents already shared
/// (project, clamp to `[0, 1]`, `hypot` for the final 2D distance). The ONLY
/// change is the degeneracy test, which is now [`DEGENERATE_EDGE_L2_MM2`];
/// read that constant's documentation for the derivation.
///
/// # Contract
///
/// mm in, mm out. Always `>= 0.0` and always finite for finite input.
///
/// # Degenerate-input contract
///
/// * **Zero-length or sub-half-quantum segment** (`|ab|² < 2.5e-17` mm², i.e.
///   `|ab| < 5e-9` mm): returns the distance to `a`. No division is performed,
///   so no `inf` and no `NaN` can escape this branch.
/// * **A real 1-quantum edge** (`|ab|² ≈ 1e-16` mm²) is NOT degenerate; it is
///   projected onto, because the union normalizer that emitted it asserts it
///   exists.
/// * **`point` coincident with `a` or `b`**: returns exactly `0.0`.
/// * **Non-finite input is not guarded.** A non-finite coordinate propagates to
///   a non-finite result; the degeneracy branch cannot convert one into a
///   plausible finite number.
/// * Never panics, never allocates, never returns an error.
///
/// # Lattice scope
///
/// [`DEGENERATE_EDGE_L2_MM2`] is derived from the SLICE lattice. Data on a
/// different lattice — `joints`' `(axial, radial)` frustum profile is the live
/// example — needs its own wrapper with its own derivation, not this function
/// and not a tolerance parameter bolted onto it (canon §4 item 5).
#[inline]
#[must_use]
pub fn point_segment_distance(point: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let l2 = ab[0] * ab[0] + ab[1] * ab[1];
    if l2 < DEGENERATE_EDGE_L2_MM2 {
        return (point[0] - a[0]).hypot(point[1] - a[1]);
    }
    let t = (((point[0] - a[0]) * ab[0] + (point[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0);
    (point[0] - (a[0] + t * ab[0])).hypot(point[1] - (a[1] + t * ab[1]))
}

/// Shoelace signed area of a closed polygon, in mm². Positive is
/// counter-clockwise.
///
/// **Canonical for the workspace** (`docs/specs/geometry-canon.md` §1 OP-8,
/// plan item C2). Provenance: byte-for-byte the shared text of
/// `plinth-support::perception`'s `signed_area` and `plinth-layers`'s
/// `signed_area` — `diff` between those two was empty.
///
/// The polygon is implicitly closed: the edge from the last point back to the
/// first is included.
///
/// # Degenerate-input contract
///
/// * **Fewer than 3 points** (empty, one point, or two): returns exactly `0.0`.
///   This is load-bearing, not a convenience — winding-derivation callers read
///   the SIGN of this value, and a 2-point "loop" must be sign-free rather than
///   arbitrarily wound.
/// * **A collinear or self-cancelling polygon** returns `0.0` with the same
///   meaning: no signed area, therefore no winding.
/// * Non-finite input propagates. Never panics, never allocates.
#[inline]
#[must_use]
pub fn signed_area(points: &[[f64; 2]]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    (0..points.len())
        .map(|i| {
            let next = (i + 1) % points.len();
            points[i][0] * points[next][1] - points[next][0] * points[i][1]
        })
        .sum::<f64>()
        * 0.5
}

/// Even-odd crossing test: is `point` inside `polygon`?
///
/// **Canonical for the workspace** (`docs/specs/geometry-canon.md` §1 OP-8,
/// plan item C2). Provenance: byte-for-byte the shared text of
/// `plinth-support::perception`'s and `plinth-support::specialized`'s
/// `point_in_polygon` — `diff` between them was empty modulo the loop-variable
/// name — and of `plinth-layers/tests/common/mod.rs`'s copy.
///
/// The polygon is implicitly closed. Winding is irrelevant: this is even-odd,
/// so a clockwise and a counter-clockwise spelling of the same ring answer
/// identically.
///
/// # Degenerate-input contract
///
/// * **Empty polygon, or one or two points**: `false`.
/// * **A point exactly ON the boundary: the result is UNSPECIFIED-BUT-PINNED.**
///   It may report either side, and which side it reports is an artifact of the
///   incumbent text, deliberately preserved rather than improved. **Do not use
///   this function alone for a verdict on points that can sit on the
///   boundary.** Every production caller already pairs it with a distance
///   tolerance ([`point_segment_distance`] via `point_in_solid` /
///   `solid_contains`); that pairing is the contract, and a new caller must
///   reproduce it.
/// * A horizontal polygon edge (`a[1] == b[1]`) contributes no crossing: the
///   `(a[1] > point[1]) != (b[1] > point[1])` guard is false before the
///   division, so no division by zero occurs.
/// * Non-finite coordinates propagate into the comparisons, which then read
///   `false`; the result is a `bool` either way and carries no signal that
///   anything was wrong. Check finiteness upstream if that matters.
#[inline]
#[must_use]
pub fn point_in_polygon(point: [f64; 2], polygon: &[[f64; 2]]) -> bool {
    let mut inside = false;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        if (a[1] > point[1]) != (b[1] > point[1])
            && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
    }
    inside
}

/// Do the segments `a`–`b` and `c`–`d` cross?
///
/// **Canonical for the workspace** (`docs/specs/geometry-canon.md` §1 OP-8,
/// plan item C2). Provenance: the strict-CCW form from
/// `plinth-support::perception`'s `segments_intersect`, its only copy.
///
/// # Degenerate-input contract
///
/// The orientation test uses a STRICT `>`, so a collinear triple counts as "not
/// counter-clockwise" — indistinguishable from clockwise. Everything below
/// follows from that, and all of it is PINNED rather than improved: the
/// incumbent's semantics are what its callers were written against. **These
/// outcomes are measured, not inferred:**
///
/// * **Collinear segments report `false`** — overlapping, identical, or
///   disjoint alike. Two segments lying on one line never "intersect" here.
/// * **Segments sharing an endpoint report `false`.**
/// * **A zero-length segment reports `false`**, even when the degenerate point
///   lies exactly on the other segment.
/// * **A T-junction reports `true`.** When one segment's endpoint lies strictly
///   inside the other and the two are not collinear, the strict form calls it a
///   crossing. So this is *not* "touching contacts are excluded" in general:
///   endpoint-to-endpoint and collinear contacts are excluded, endpoint-to-
///   interior contact is not. A caller that needs proper-crossing-only, or
///   any-contact, must add its own test rather than assume either.
/// * Non-finite coordinates make every strict comparison `false`, which makes
///   both `!=` tests `false`, so the result is `false`. Never panics, never
///   allocates, no division anywhere.
#[inline]
#[must_use]
pub fn segments_intersect(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let ccw = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        (r[1] - p[1]) * (q[0] - p[0]) > (q[1] - p[1]) * (r[0] - p[0])
    };
    ccw(a, c, d) != ccw(b, c, d) && ccw(a, b, c) != ccw(a, b, d)
}

/// 2-D dot product, `a[0]*b[0] + a[1]*b[1]` — IN EXACTLY THAT OPERAND AND TERM
/// ORDER.
///
/// **The order is the contract.** Call sites consolidated onto this function
/// were measured bit-identical only under this order; regrouping or swapping
/// operands is a spec violation here even when algebraically neutral, exactly
/// as it is in `closest.rs` (Amendments A6/A21).
///
/// This closes the "missing 2-D home" open item (`docs/specs/
/// geometry-adjudication.md` §O6, evidence doc §8.1): the verdict-feeding 2-D
/// algebra in `plinth-support::perception` and `plinth-orient::section` had no
/// kernel home, and under the ratified precedence rule a verdict-feeding site
/// with no kernel home gets a kernel home BUILT, not an exemption marker.
///
/// mm in, mm² out (or whatever the operands' units square to — this is
/// arithmetic, not a length).
#[inline]
#[must_use]
pub fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

/// 2-D perp-dot (scalar cross), `a[0]*b[1] - a[1]*b[0]` — same operand and term
/// order contract as [`dot`], for the same reason.
///
/// Sign convention: positive when `b` is counter-clockwise from `a`. Twice the
/// signed area of the triangle `(0, a, b)`.
#[inline]
#[must_use]
pub fn perp_dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
