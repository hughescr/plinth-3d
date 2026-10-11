//! The exact peak cross-section area along one candidate build direction.
//!
//! # Why this is not a sampler
//!
//! The legacy heuristic sampled sections every 4 mm from a 0.1 mm margin, and
//! `03-physics-and-constants.md` §3.10.3 retires that sampler **by name**.  It
//! must not be replaced by another one, and the reason is specific to a *dense*
//! search rather than general distaste for sampling: a stepped sampler's plane
//! phase is anchored to each candidate's own `h_min`, so the peak it reports is
//! a function of the sampler as much as of the model.  On the 20 mm corpus
//! sphere the aliasing bound is ~0.126 cm², about 0.38 score points — the same
//! order as the whole scar term.  Across 13 candidates that is noise; across
//! ~650 the argmin systematically prefers directions whose grid happens to
//! under-report their true peak, so the search would optimize the sampler.
//! Searching harder would make the artifact worse.  So the supremum is computed
//! exactly, which also removes a would-be Placeholder constant (the step)
//! rather than registering one.
//!
//! # The method
//!
//! For a closed, consistently wound mesh the cross-section at height `t` is a
//! union of closed loops, and its area is the shoelace sum over the section
//! polygon.  The shoelace decomposes per edge, and every section edge belongs to
//! exactly one face, so
//!
//! ```text
//! area(t) = Σ_faces ½ · (p_x·q_y − p_y·q_x)
//! ```
//!
//! where `p → q` is that face's chord oriented so the solid lies to its left.
//! The sum is therefore **additive over faces** and needs no loop stitching, no
//! polygon union and no containment test — which is exactly what makes it
//! affordable in the inner loop.  Test `section_matches_the_kernel_slice_oracle`
//! is what licenses that skip: it checks this solver against the geometry
//! kernel's own independent plane-slice batch.
//!
//! Within one face the two crossing edges are fixed, and both crossing points
//! are linear in `t`, so the face's contribution is a **quadratic in `t`** whose
//! breakpoints are exactly the vertex heights.  Each face's sub-interval is
//! charged to the `O(log V)` nodes of a segment tree over the height intervals
//! that tile it, and one top-down pass hands every node's quadratic to its
//! children, which gives every interval's quadratic in `Θ(V log V + F log V)`.
//! The supremum is then the max over interval endpoints and interior
//! stationary points.  No step, no margin, no aliasing, no tolerance.
//!
//! # Why a tree and local heights, not a prefix sum in `t`
//!
//! A face whose corners differ in height by `w` has crossing points that move
//! `L / w` per unit height, so its quadratic's coefficients grow like
//! `(L / w)²`.  A facet that lies horizontal in exact arithmetic (every rest
//! pose has one) has corners a few ulps apart once projected: `w = 1.8e-15` mm
//! and a coefficient of 7e30 on the corpus Cylinder.  The solver used to write
//! every quadratic in the absolute height `t` and prefix-sum the coefficients
//! of all faces into one running polynomial.  One sliver then absorbed every
//! other face's coefficients (7e30 has an ulp of 1e15) and left that rounding
//! behind when it was subtracted, so the section function was wrong for the
//! whole height range above it: 75 to 6.6e6 mm² on 62 congruent 900 mm² poses
//! (Lane 0b, `wave1b/STAGE-JITTER.md`, 2026-10-11).
//!
//! Here no quadratic is ever written in `t` and nothing is ever subtracted.
//! A face is charged to a tree node in that node's **own** height coordinate
//! `r = t − (node's lowest breakpoint)`, and only to nodes inside its own
//! sub-interval, so `r` never exceeds the face's own `w`: each of its terms is
//! bounded by the chord's own `|p|·L`, whatever `w` is.  A node's quadratic is
//! shifted to a child's origin by at most the node's own height, which every
//! face charged to it spans.  So the absolute rounding error of an interval is
//! `O(ε · Σ|p|·L)` over the faces that cross it, the same for a sliver as for
//! a tall face, and a sliver's quadratic never reaches an interval outside it.
//!
//! # The stated precondition
//!
//! The identity measures the **winding-number-weighted** area, so two
//! *interpenetrating* closed shells inside one mesh are counted twice where they
//! overlap.  That is the same convention the geometry kernel's own slice sum
//! carries, and it is exactly right for any consistently wound solid — which is
//! what the orientation stage is handed, since its input has already been
//! triaged and decimated as one production mesh.  It is recorded here as a
//! precondition rather than left as a surprise; disjoint components (a treant on
//! three toes, a slab with separate toes) are unaffected, because disjoint
//! shells never overlap.
//!
//! The closedness half of the precondition is enforced rather than assumed:
//! `OrientFaceTable` refuses a LOD with any boundary edge, so a hole cannot
//! silently produce a plausible wrong area.
//!
//! # Orientation of a chord, without a sign test
//!
//! The chord's direction is `ẑ × n`, so a dot-product sign test against it is
//! ill-conditioned exactly where it matters most: a near-horizontal face has a
//! vanishing in-plane normal but sweeps a large area over a thin band of `t`.
//! The ordering is taken **combinatorially from the triangle's own winding**
//! instead.  On the lower sub-interval the two crossing edges are the two
//! incident to the lowest vertex `p`: the directed edge `p → next(p)` crosses
//! upward and `prev(p) → p` crosses downward; the chord runs from the downward
//! crossing to the upward one.  On the upper sub-interval the pivot is the
//! highest vertex and the two roles swap.  That is a pure integer decision on an
//! outward-wound mesh — no epsilon, no conditioning.

/// Why one candidate could not be scored.  Never clamped, never a sentinel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SectionRefusal {
    /// A coefficient or an evaluated area was not finite.
    NonFinite,
    /// The supremum came out negative, which on a closed outward-wound mesh
    /// means the winding, not the arithmetic, is wrong.
    NegativeArea { peak_mm2: f64 },
}

/// The whole cross-section area function along one candidate direction: a
/// quadratic per height interval, breakpoints at the vertex heights.
pub(crate) struct SectionProfile {
    breakpoints: Vec<f64>,
    /// The segment tree, heap-ordered (node 1 is the root, node `k`'s children
    /// are `2k` and `2k + 1`), over `leaves` = the interval count rounded up
    /// to a power of two.  After [`build_profile`] the leaf of interval `i`,
    /// `tree[leaves + i]`, holds that interval's `(r², r, 1)` coefficients in
    /// its own local height `r = t − breakpoints[i]`; padding leaves stay zero.
    /// Empty when the body has no extent.
    tree: Vec<[f64; 3]>,
    leaves: usize,
}

/// Builds the exact area function.
///
/// `tau` holds each vertex's height along the candidate direction **already
/// shifted so the minimum is zero**; `xy` holds each vertex's coordinates in the
/// candidate's own in-plane basis.  Every quadratic is written in a local
/// height (module docs), so the shift no longer carries the conditioning; it
/// keeps breakpoint differences exact where heights are small.
pub(crate) fn build_profile(
    tris: &[[u32; 3]],
    degenerate: &[bool],
    xy: &[[f64; 2]],
    tau: &[f64],
) -> SectionProfile {
    let breakpoints = distinct_heights(tau);
    let intervals = breakpoints.len().saturating_sub(1);
    if intervals == 0 {
        // Every vertex at one height: the body has no extent along this
        // direction, so every section is empty.
        return SectionProfile {
            breakpoints,
            tree: Vec::new(),
            leaves: 0,
        };
    }

    let leaves = intervals.next_power_of_two();
    let mut tree = vec![[0.0_f64; 3]; 2 * leaves];
    for (face, triangle) in tris.iter().enumerate() {
        if degenerate[face] {
            // A zero-area face bounds nothing; it can only contribute noise.
            continue;
        }
        accumulate_face(*triangle, xy, tau, &breakpoints, leaves, &mut tree);
    }

    // Top-down, serial, ascending node order (determinism law: never a
    // parallel fold over floats): a parent precedes its children, so each
    // node is complete before it is handed down.
    let levels = leaves.trailing_zeros();
    for node in 1..leaves {
        let depth = node.ilog2();
        let span = leaves >> depth;
        let low = (node << (levels - depth)) - leaves;
        if low >= intervals {
            // Padding only: no breakpoint, no face.
            continue;
        }
        let parent = tree[node];
        add_into(&mut tree[2 * node], parent);
        let middle = low + span / 2;
        if middle < intervals {
            let shift = breakpoints[middle] - breakpoints[low];
            add_into(&mut tree[2 * node + 1], shifted(parent, shift));
        }
    }
    SectionProfile {
        breakpoints,
        tree,
        leaves,
    }
}

/// `q(r + shift)` as coefficients in `r`.
fn shifted([square, linear, constant]: [f64; 3], shift: f64) -> [f64; 3] {
    [
        square,
        2.0 * square * shift + linear,
        (square * shift + linear) * shift + constant,
    ]
}

fn add_into(target: &mut [f64; 3], delta: [f64; 3]) {
    for (accumulated, value) in target.iter_mut().zip(delta) {
        *accumulated += value;
    }
}

impl SectionProfile {
    /// Distinct height breakpoints resolved, the metered counterpart of the
    /// vertex-count bound the caller charged.
    pub(crate) fn breakpoint_count(&self) -> usize {
        self.breakpoints.len()
    }

    /// Exact supremum of the cross-section area over the whole height range.
    pub(crate) fn peak_mm2(&self) -> Result<f64, SectionRefusal> {
        let intervals = self.intervals();
        if intervals == 0 {
            return Ok(0.0);
        }
        let mut peak = f64::NEG_INFINITY;
        for interval in 0..intervals {
            let value = interval_supremum(
                self.tree[self.leaves + interval],
                self.breakpoints[interval + 1] - self.breakpoints[interval],
            )?;
            if value > peak {
                peak = value;
            }
        }
        if !peak.is_finite() {
            return Err(SectionRefusal::NonFinite);
        }
        if peak < 0.0 {
            return Err(SectionRefusal::NegativeArea { peak_mm2: peak });
        }
        Ok(canonical_zero(peak))
    }

    /// Exact cross-section area at one shifted height.
    ///
    /// Outside the model's own height range the section is empty, so the answer
    /// is zero.  At an interior breakpoint the interval **above** it is used:
    /// the area function can genuinely step there (a flat face), and the upper
    /// limit is the one a layer at that height would print.
    pub(crate) fn area_at(&self, tau: f64) -> Result<f64, SectionRefusal> {
        let intervals = self.intervals();
        if intervals == 0 {
            return Ok(0.0);
        }
        let last = self.breakpoints.len() - 1;
        if tau < self.breakpoints[0] || tau > self.breakpoints[last] {
            return Ok(0.0);
        }
        let interval = self
            .breakpoints
            .partition_point(|value| *value <= tau)
            .saturating_sub(1)
            .min(intervals - 1);
        let [square, linear, constant] = self.tree[self.leaves + interval];
        let local = tau - self.breakpoints[interval];
        let area = (square * local + linear) * local + constant;
        if area.is_finite() {
            Ok(canonical_zero(area))
        } else {
            Err(SectionRefusal::NonFinite)
        }
    }

    fn intervals(&self) -> usize {
        if self.tree.is_empty() {
            0
        } else {
            self.breakpoints.len() - 1
        }
    }
}

/// Ascending distinct vertex heights: the exact breakpoint set of the piecewise
/// quadratic. Sorted by `total_cmp` then vertex index, so the order is a pure
/// function of the data.
#[expect(
    clippy::cast_possible_truncation,
    reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged as a \
          class, and the one architecture item the class carried (geom_kernel's 32-bit BVH index \
          width) is now a named, enforced limit. TODO(cast-width-audit): retire site by site.  See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn distinct_heights(tau: &[f64]) -> Vec<f64> {
    let mut order: Vec<u32> = (0..tau.len() as u32).collect();
    order.sort_unstable_by(|left, right| {
        tau[*left as usize]
            .total_cmp(&tau[*right as usize])
            .then(left.cmp(right))
    });
    let mut heights = Vec::with_capacity(order.len());
    for index in order {
        let height = tau[index as usize];
        if heights.last().copied() != Some(height) {
            heights.push(height);
        }
    }
    heights
}

/// Charges one face's two sub-interval quadratics to the segment tree.
#[allow(
    clippy::float_cmp,
    reason = "bit-equal lowest and highest corner heights is the definition of \
              a face that crosses no plane interior.  No value is decided by \
              the tightness of this test: every unequal pair falls through to \
              the strict `<` tests below, which handle an arbitrarily thin \
              sub-interval on its own terms"
)]
fn accumulate_face(
    triangle: [u32; 3],
    xy: &[[f64; 2]],
    tau: &[f64],
    breakpoints: &[f64],
    leaves: usize,
    tree: &mut [[f64; 3]],
) {
    // Corner slots sorted by height, ties by vertex index: `slot[k]` is the
    // position *within the triangle's winding cycle* of the k-th lowest corner,
    // which is what makes `next`/`prev` below a winding question rather than a
    // geometric one.
    let mut slot = [0_usize, 1, 2];
    slot.sort_unstable_by(|left, right| {
        tau[triangle[*left] as usize]
            .total_cmp(&tau[triangle[*right] as usize])
            .then(triangle[*left].cmp(&triangle[*right]))
    });
    let height = |corner: usize| tau[triangle[corner] as usize];
    let (low, mid, high) = (slot[0], slot[1], slot[2]);
    if height(low) == height(high) {
        // Horizontal face: it crosses no plane's interior.
        return;
    }

    // Lower sub-interval: pivot is the lowest corner, whose outgoing winding
    // edge climbs (the upward crossing) and whose incoming edge descends.
    if height(low) < height(mid) {
        chord(
            triangle,
            xy,
            tau,
            low,
            (low + 2) % 3, // prev(low): the downward crossing
            (low + 1) % 3, // next(low): the upward crossing
        )
        .charge(height(low), height(mid), breakpoints, leaves, tree);
    }
    // Upper sub-interval: pivot is the highest corner, whose outgoing edge
    // descends (the downward crossing) and whose incoming edge climbs.
    if height(mid) < height(high) {
        chord(
            triangle,
            xy,
            tau,
            high,
            (high + 1) % 3, // next(high): the downward crossing
            (high + 2) % 3, // prev(high): the upward crossing
        )
        .charge(height(mid), height(high), breakpoints, leaves, tree);
    }
}

/// One face's chord over one sub-interval, written from its pivot corner.
///
/// Both crossing points are `p(t) = P + s·b` with `s = t − h_pivot`, so the
/// chord's shoelace term is
/// `½·(p_down × p_up) = ½·(s·X + s²·Y)`, `X = P × (b_up − b_down)`,
/// `Y = b_down × b_up` (`P × P` vanishes).  Nothing here is expanded about
/// `t = 0`; [`Chord::at`] re-expands about a node's own origin.
#[derive(Clone, Copy)]
struct Chord {
    h_pivot: f64,
    linear: f64,
    square: f64,
}

fn chord(
    triangle: [u32; 3],
    xy: &[[f64; 2]],
    tau: &[f64],
    pivot: usize,
    down: usize,
    up: usize,
) -> Chord {
    let point = |corner: usize| xy[triangle[corner] as usize];
    let height = |corner: usize| tau[triangle[corner] as usize];
    let (origin, h_pivot) = (point(pivot), height(pivot));
    let slope = |corner: usize| {
        let target = point(corner);
        let span = height(corner) - h_pivot;
        [
            (target[0] - origin[0]) / span,
            (target[1] - origin[1]) / span,
        ]
    };
    let (b_down, b_up) = (slope(down), slope(up));
    Chord {
        h_pivot,
        linear: origin[0] * (b_up[1] - b_down[1]) - origin[1] * (b_up[0] - b_down[0]),
        square: b_down[0] * b_up[1] - b_down[1] * b_up[0],
    }
}

impl Chord {
    /// The `(r², r, 1)` coefficients of this chord's term in the local height
    /// `r = t − origin`.  With `e = origin − h_pivot`, `s = e + r`.
    fn at(self, origin: f64) -> [f64; 3] {
        let e = origin - self.h_pivot;
        [
            0.5 * self.square,
            0.5 * (2.0 * e * self.square + self.linear),
            0.5 * e * (e * self.square + self.linear),
        ]
    }

    /// Charges this chord to the tree nodes that exactly tile the intervals
    /// inside `[low, high]`, each in its own node's local height.
    ///
    /// Both bounds are vertex heights, so both are members of the breakpoint
    /// list and the lookups are exact rather than nearest.  Every node charged
    /// lies inside `[low, high]`, so its local height never exceeds this
    /// sub-interval's own.
    fn charge(
        self,
        low: f64,
        high: f64,
        breakpoints: &[f64],
        leaves: usize,
        tree: &mut [[f64; 3]],
    ) {
        let start = breakpoints.partition_point(|value| *value < low);
        let end = breakpoints.partition_point(|value| *value < high);
        let (mut left, mut right) = (start + leaves, end + leaves);
        // `level` is the height above the leaves: a node `k` there covers the
        // intervals from `(k << level) - leaves`.
        let mut level = 0_u32;
        while left < right {
            if left & 1 == 1 {
                let origin = breakpoints[(left << level) - leaves];
                add_into(&mut tree[left], self.at(origin));
                left += 1;
            }
            if right & 1 == 1 {
                right -= 1;
                let origin = breakpoints[(right << level) - leaves];
                add_into(&mut tree[right], self.at(origin));
            }
            left >>= 1;
            right >>= 1;
            level += 1;
        }
    }
}

/// Supremum of one interval's quadratic, in its local height, over `[0, width]`.
///
/// The closed range is deliberate: at a breakpoint the open-interval polynomial
/// evaluates to the section's limit from inside, which is the supremum a flat
/// top or bottom face reaches but never attains.
fn interval_supremum(coefficients: [f64; 3], width: f64) -> Result<f64, SectionRefusal> {
    let [square, linear, constant] = coefficients;
    if !(square.is_finite() && linear.is_finite() && constant.is_finite()) {
        return Err(SectionRefusal::NonFinite);
    }
    let evaluate = |r: f64| (square * r + linear) * r + constant;
    let mut best = constant.max(evaluate(width));
    if square < 0.0 {
        // Concave: the interior stationary point is the maximum. A convex
        // interval's stationary point is a minimum and is deliberately skipped.
        let stationary = -linear / (2.0 * square);
        if stationary > 0.0 && stationary < width {
            best = best.max(evaluate(stationary));
        }
    }
    if best.is_finite() {
        Ok(best)
    } else {
        Err(SectionRefusal::NonFinite)
    }
}

const fn canonical_zero(value: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        value
    }
}
