//! Gap lower bounds between a support solid and the model (lane A; design
//! `step11-construction.md` sections 5.5 and 7.2 H6). Step 0 holds the signatures only.
//!
//! The gap is the distance between the actual finite convex solid, caps included, and the
//! model surface. A radius-varying axis distance is a broad-phase rejection only, never a
//! bound.

use crate::bvh::Bvh;
use crate::measure::{Measure, fill_unverified};
use crate::patch::Ball;

/// A support solid, as the written fields give it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Solid {
    /// A whole ball.
    Ball(Ball),
    /// A frustum from `start` (radius `r_start`) to `end` (radius `r_end`), flat caps.
    Frustum {
        /// Start point.
        start: [f64; 3],
        /// End point.
        end: [f64; 3],
        /// Radius at `start`.
        r_start: f64,
        /// Radius at `end`.
        r_end: f64,
    },
}

/// Bounds on one gap, mm. `lo <= hi` whenever the bounds are `Known`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GapBounds {
    /// Certified lower bound.
    pub lo: f64,
    /// Certified upper bound.
    pub hi: f64,
}

/// Bounds on the gap from each solid to the model, one answer per solid.
///
/// `out` is cleared and refilled with exactly `solids.len()` answers.
pub fn solid_gaps(bvh: &Bvh<'_>, solids: &[Solid], out: &mut Vec<Measure<GapBounds>>) {
    let _ = bvh;
    fill_unverified(out, solids.len(), "solid_gaps is not built yet");
}
