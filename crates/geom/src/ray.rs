//! Batched ray casts against the frozen model (lane A; design `step11-construction.md`
//! sections 5.2 and 5.9). Step 0 holds the signatures only.
//!
//! Site class (hidden underside, partly visible rim, decorated top) comes from many rays per
//! sample point in the resting pose. The caller builds every ray of a stage and passes them
//! in one call.

use crate::bvh::Bvh;
use crate::measure::{Measure, fill_unverified};

/// One ray: a start point and a direction (not necessarily unit length).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    /// Start point.
    pub origin: [f64; 3],
    /// Direction.
    pub dir: [f64; 3],
}

/// What a ray found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RayHit {
    /// The ray leaves the mesh's bounds without touching it.
    Miss,
    /// The nearest triangle hit.
    Hit {
        /// Distance along `dir`, in units of `dir`'s length.
        t: f64,
        /// Index of the triangle hit.
        triangle: u32,
    },
}

/// The nearest hit of each ray, one answer per ray.
///
/// `out` is cleared and refilled with exactly `rays.len()` answers.
pub fn first_hits(bvh: &Bvh<'_>, rays: &[Ray], out: &mut Vec<Measure<RayHit>>) {
    let _ = bvh;
    fill_unverified(out, rays.len(), "first_hits is not built yet");
}
