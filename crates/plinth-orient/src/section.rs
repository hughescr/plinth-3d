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
//! breakpoints are exactly the vertex heights.  Accumulating each face's three
//! coefficients into a per-interval difference array and prefix-summing gives
//! every interval's quadratic in `Θ(V log V + F)`, after which the supremum is
//! the max over interval endpoints and interior stationary points.  No step, no
//! margin, no aliasing, no tolerance.
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
    /// One absolute `(t², t, 1)` triple per interval, already prefix-summed.
    coefficients: Vec<[f64; 3]>,
}

/// Builds the exact area function.
///
/// `tau` holds each vertex's height along the candidate direction **already
/// shifted so the minimum is zero**; `xy` holds each vertex's coordinates in the
/// candidate's own in-plane basis.  The shift is not cosmetic: each face's
/// quadratic is stored as `V_p + b·(t − h_p)` expanded into `a + b·t`, and `|a|`
/// grows with `|h_p|`, so a model far from the origin would lose precision on
/// thin faces for no reason.  Shifting costs one subtraction per vertex.
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
            coefficients: Vec::new(),
        };
    }

    // Difference array over intervals; one extra slot so a face ending at the
    // last interval needs no bounds special case.
    let mut coefficients = vec![[0.0_f64; 3]; intervals + 1];
    for (face, triangle) in tris.iter().enumerate() {
        if degenerate[face] {
            // A zero-area face bounds nothing; it can only contribute noise.
            continue;
        }
        accumulate_face(*triangle, xy, tau, &breakpoints, &mut coefficients);
    }

    // Serial prefix sum in ascending interval order (determinism law: never a
    // parallel fold over floats).
    let mut running = [0.0_f64; 3];
    for interval in coefficients.iter_mut().take(intervals) {
        for (accumulated, delta) in running.iter_mut().zip(*interval) {
            *accumulated += delta;
        }
        *interval = running;
    }
    coefficients.truncate(intervals);
    SectionProfile {
        breakpoints,
        coefficients,
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
        if self.coefficients.is_empty() {
            return Ok(0.0);
        }
        let mut peak = f64::NEG_INFINITY;
        for (interval, coefficients) in self.coefficients.iter().enumerate() {
            let value = interval_supremum(
                *coefficients,
                self.breakpoints[interval],
                self.breakpoints[interval + 1],
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
        if self.coefficients.is_empty() {
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
            .min(self.coefficients.len() - 1);
        let [square, linear, constant] = self.coefficients[interval];
        let area = (square * tau + linear) * tau + constant;
        if area.is_finite() {
            Ok(canonical_zero(area))
        } else {
            Err(SectionRefusal::NonFinite)
        }
    }
}

/// Ascending distinct vertex heights: the exact breakpoint set of the piecewise
/// quadratic. Sorted by `total_cmp` then vertex index, so the order is a pure
/// function of the data.
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

/// Adds one face's two sub-interval quadratics into the difference array.
fn accumulate_face(
    triangle: [u32; 3],
    xy: &[[f64; 2]],
    tau: &[f64],
    breakpoints: &[f64],
    coefficients: &mut [[f64; 3]],
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
        add_range(
            quadratic(
                triangle,
                xy,
                tau,
                low,
                (low + 2) % 3, // prev(low): the downward crossing
                (low + 1) % 3, // next(low): the upward crossing
            ),
            height(low),
            height(mid),
            breakpoints,
            coefficients,
        );
    }
    // Upper sub-interval: pivot is the highest corner, whose outgoing edge
    // descends (the downward crossing) and whose incoming edge climbs.
    if height(mid) < height(high) {
        add_range(
            quadratic(
                triangle,
                xy,
                tau,
                high,
                (high + 1) % 3, // next(high): the downward crossing
                (high + 2) % 3, // prev(high): the upward crossing
            ),
            height(mid),
            height(high),
            breakpoints,
            coefficients,
        );
    }
}

/// The `(t², t, 1)` coefficients of `½·(p_down × p_up)` for one sub-interval,
/// where both crossing points are parametrized from the shared pivot corner.
fn quadratic(
    triangle: [u32; 3],
    xy: &[[f64; 2]],
    tau: &[f64],
    pivot: usize,
    down: usize,
    up: usize,
) -> [f64; 3] {
    let point = |corner: usize| xy[triangle[corner] as usize];
    let height = |corner: usize| tau[triangle[corner] as usize];
    let (origin, h_pivot) = (point(pivot), height(pivot));

    // p(t) = origin + slope·(t − h_pivot) = a + b·t
    let line = |corner: usize| {
        let target = point(corner);
        let span = height(corner) - h_pivot;
        let slope = [
            (target[0] - origin[0]) / span,
            (target[1] - origin[1]) / span,
        ];
        (
            [
                origin[0] - slope[0] * h_pivot,
                origin[1] - slope[1] * h_pivot,
            ],
            slope,
        )
    };
    let (a_down, b_down) = line(down);
    let (a_up, b_up) = line(up);

    [
        0.5 * (b_down[0] * b_up[1] - b_down[1] * b_up[0]),
        0.5 * (a_down[0] * b_up[1] + b_down[0] * a_up[1]
            - a_down[1] * b_up[0]
            - b_down[1] * a_up[0]),
        0.5 * (a_down[0] * a_up[1] - a_down[1] * a_up[0]),
    ]
}

/// Charges one quadratic to every interval inside `[low, high]`.
///
/// Both bounds are vertex heights, so both are members of the breakpoint list
/// and the lookups are exact rather than nearest.
fn add_range(
    quadratic: [f64; 3],
    low: f64,
    high: f64,
    breakpoints: &[f64],
    coefficients: &mut [[f64; 3]],
) {
    let start = breakpoints.partition_point(|value| *value < low);
    let end = breakpoints.partition_point(|value| *value < high);
    for (accumulated, delta) in coefficients[start].iter_mut().zip(quadratic) {
        *accumulated += delta;
    }
    for (accumulated, delta) in coefficients[end].iter_mut().zip(quadratic) {
        *accumulated -= delta;
    }
}

/// Supremum of one interval's quadratic over its closed range.
///
/// The closed range is deliberate: at a breakpoint the open-interval polynomial
/// evaluates to the section's limit from inside, which is the supremum a flat
/// top or bottom face reaches but never attains.
fn interval_supremum(coefficients: [f64; 3], low: f64, high: f64) -> Result<f64, SectionRefusal> {
    let [square, linear, constant] = coefficients;
    if !(square.is_finite() && linear.is_finite() && constant.is_finite()) {
        return Err(SectionRefusal::NonFinite);
    }
    let evaluate = |t: f64| (square * t + linear) * t + constant;
    let mut best = evaluate(low).max(evaluate(high));
    if square < 0.0 {
        // Concave: the interior stationary point is the maximum. A convex
        // interval's stationary point is a minimum and is deliberately skipped.
        let stationary = -linear / (2.0 * square);
        if stationary > low && stationary < high {
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
