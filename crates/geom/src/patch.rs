//! Contact-patch area bounds (lane A; design `step11-construction.md` sections 6.3 and 7.2
//! H1). Step 0 holds the signatures only.
//!
//! The patch of a ball on a mesh is the part of the model surface the ball touches. The
//! batch function returns a lower and an upper bound per ball: exact disc-triangle sums in
//! f64 plus derived rounding slack, never a point-grid estimate.

use crate::bvh::Bvh;
use crate::measure::{Measure, fill_unverified};

/// A whole ball: centre and radius, mm, as the written f32 fields converted to f64.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ball {
    /// Centre.
    pub centre: [f64; 3],
    /// Radius.
    pub radius: f64,
}

/// Bounds on one area, mm². `lo <= hi` whenever the bounds are `Known`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AreaBounds {
    /// Certified lower bound.
    pub lo: f64,
    /// Certified upper bound.
    pub hi: f64,
}

/// Bounds on the connected patch each ball makes on the model, one answer per ball.
///
/// `out` is cleared and refilled with exactly `balls.len()` answers.
pub fn ball_patches(bvh: &Bvh<'_>, balls: &[Ball], out: &mut Vec<Measure<AreaBounds>>) {
    let _ = bvh;
    fill_unverified(out, balls.len(), "ball_patches is not built yet");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Mesh;

    #[test]
    fn stub_refuses_every_ball() {
        let mesh = Mesh::new(
            vec![0.0, 1.0, 0.0],
            vec![0.0, 0.0, 1.0],
            vec![0.0; 3],
            vec![0, 1, 2],
        )
        .unwrap();
        let bvh = Bvh::build(&mesh).unwrap();
        let ball = Ball {
            centre: [0.2, 0.2, 0.1],
            radius: 0.2,
        };
        let mut out = Vec::new();
        ball_patches(&bvh, &[ball, ball], &mut out);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(Measure::is_unverified));
    }
}
