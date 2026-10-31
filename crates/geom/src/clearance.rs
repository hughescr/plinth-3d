//! Conservative finite-solid/surface distance enclosures, including caps.
//! Outward-rounded plane projections provide certified lower bounds. A containing
//! capsule and triangle box give a second lower bound; actual vertex witnesses give
//! upper bounds. These admission intervals need not have the checker's 0.005 mm width.

use crate::patch::{dot, frustum_plane_gap, plane_distance_bounds, sub, triangle};
use crate::{Bvh, Mesh, measure::Measure, patch::Ball};

/// Native analytic enclosing solids.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Solid {
    /// Whole sphere.
    Ball(Ball),
    /// Closed finite frustum (caps included).
    Frustum {
        /// Start centre.
        start: [f64; 3],
        /// End centre.
        end: [f64; 3],
        /// Start radius.
        r_start: f64,
        /// End radius.
        r_end: f64,
    },
}
/// Enclosure of the surface distance, mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GapBounds {
    /// Lower bound (may be conservative).
    pub lo: f64,
    /// Upper bound (a geometric witness).
    pub hi: f64,
}

// At coordinates <= 1000 mm the following box and witness expressions use at most
// 32 scalar operations at magnitude <= 1.2e7. Their absolute distance rounding error
// is enclosed by 1e-8 mm. No ill-conditioned point/triangle solve enters a lower bound.
const ARITHMETIC_MM: f64 = 1e-8;
fn box_gap(p: [f64; 3], t: [[f64; 3]; 3]) -> f64 {
    let mut squared = 0.0;
    for k in 0..3 {
        let lo = t.iter().map(|v| v[k]).fold(f64::INFINITY, f64::min);
        let hi = t.iter().map(|v| v[k]).fold(f64::NEG_INFINITY, f64::max);
        let d = (lo - p[k]).max(p[k] - hi).max(0.0);
        squared += d * d;
    }
    (squared.sqrt() - ARITHMETIC_MM).max(0.0)
}
fn vertex_witness(p: [f64; 3], t: [[f64; 3]; 3]) -> f64 {
    t.into_iter()
        .map(|v| {
            let d = sub(v, p);
            dot(d, d).sqrt() + ARITHMETIC_MM
        })
        .fold(f64::INFINITY, f64::min)
}

fn frustum_triangle(t: [[f64; 3]; 3], a: [f64; 3], b: [f64; 3], ra: f64, rb: f64) -> GapBounds {
    let d = sub(b, a);
    let length = dot(d, d).sqrt();
    let plane = frustum_plane_gap(t, a, b, ra, rb);
    // A sub-frustum is inside the radius balls swept along its finite axis.
    // Distance is 1-Lipschitz: box-distance(mid) - half-axis-length - max-radius
    // bounds the entire sub-frustum. This is NOT an exact finite-cap distance.
    let mut lower = f64::INFINITY;
    let mut upper = f64::INFINITY;
    const PIECES: usize = 16;
    for i in 0..PIECES {
        let u = i as f64 / PIECES as f64;
        let v = (i + 1) as f64 / PIECES as f64;
        let mid = (u + v) * 0.5;
        let p = std::array::from_fn(|k| a[k] + mid * d[k]);
        let radius = (ra + (rb - ra) * u).max(ra + (rb - ra) * v);
        lower = lower.min(box_gap(p, t) - length / (2.0 * PIECES as f64) - radius - ARITHMETIC_MM);
        upper = upper.min(vertex_witness(p, t) + ARITHMETIC_MM);
    }
    GapBounds {
        lo: plane.max(lower).max(0.0),
        hi: upper,
    }
}

/// Scalar reference. Out-of-domain and degenerate solids fail closed. An interval
/// can be wide; a caller may use its lower bound for admission but not claim precision.
#[must_use]
pub fn solid_gap_reference(mesh: &Mesh, solid: Solid) -> Measure<GapBounds> {
    if mesh
        .bounds()
        .min
        .iter()
        .chain(&mesh.bounds().max)
        .any(|v| !v.is_finite() || v.abs() > 1000.0)
    {
        return Measure::Unverified("model outside clearance domain");
    }
    let mut bound = GapBounds {
        lo: f64::INFINITY,
        hi: f64::INFINITY,
    };
    match solid {
        Solid::Ball(b) => {
            if !valid_point(b.centre) || !b.radius.is_finite() || b.radius <= 0.0 || b.radius > 10.0
            {
                return Measure::Unverified("invalid ball");
            }
            for i in 0..mesh.triangle_count() {
                let t = triangle(mesh, i);
                let plane = plane_distance_bounds(b.centre, t).map_or(0.0, |d| d.lo);
                let lo = (plane.max(box_gap(b.centre, t)) - b.radius).max(0.0);
                let hi = (vertex_witness(b.centre, t) - b.radius).max(0.0);
                bound.lo = bound.lo.min(lo);
                bound.hi = bound.hi.min(hi);
            }
        }
        Solid::Frustum {
            start,
            end,
            r_start,
            r_end,
        } => {
            if !valid_point(start)
                || !valid_point(end)
                || !r_start.is_finite()
                || !r_end.is_finite()
                || r_start <= 0.0
                || r_end <= 0.0
                || r_start.max(r_end) > 10.0
                || dot(sub(start, end), sub(start, end)) < 1e-20
            {
                return Measure::Unverified("invalid frustum");
            }
            for i in 0..mesh.triangle_count() {
                let d = frustum_triangle(triangle(mesh, i), start, end, r_start, r_end);
                bound.lo = bound.lo.min(d.lo);
                bound.hi = bound.hi.min(d.hi);
            }
        }
    }
    if !bound.hi.is_finite() {
        return Measure::Unverified("empty mesh");
    }
    Measure::Known(bound)
}
fn valid_point(p: [f64; 3]) -> bool {
    p.iter().all(|x| x.is_finite() && x.abs() <= 1000.0)
}
/// One ordered answer per input solid; one worker by default.
pub fn solid_gaps(bvh: &Bvh<'_>, solids: &[Solid], out: &mut Vec<Measure<GapBounds>>) {
    solid_gaps_with_workers(bvh, solids, out, 1);
}
/// Bounded-worker stage-level probe; allocation comes from the caller.
pub fn solid_gaps_with_workers(
    bvh: &Bvh<'_>,
    solids: &[Solid],
    out: &mut Vec<Measure<GapBounds>>,
    workers: usize,
) {
    crate::counters::record(crate::counters::Op::SolidGaps, solids.len());
    crate::patch::map_batch(solids, out, workers, |&s| {
        solid_gap_reference(bvh.mesh(), s)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lateral_cap_edge_encloses_analytic_gap_between_axis_samples() {
        // The triangle lies in the start-cap plane, outside its rim. Its closest
        // point is (0, 0.35, 0); the cylinder rim is (0, 0.25, 0): gap = 0.10.
        // The first axis sample is 0.625 mm away. Dropping the half-length term
        // incorrectly reports lo > 0.46. Infinite-plane distance is zero here,
        // so this witness specifically exercises the finite box bound.
        let m = Mesh::new(
            vec![-0.01, 0.01, 0.0],
            vec![0.35, 0.35, 0.36],
            vec![0.0; 3],
            vec![0, 1, 2],
        )
        .unwrap();
        let solids = [Solid::Frustum {
            start: [0.0; 3],
            end: [0.0, 0.0, 20.0],
            r_start: 0.25,
            r_end: 0.25,
        }];
        let mut out = Vec::new();
        solid_gaps(&Bvh::build(&m).unwrap(), &solids, &mut out);
        let bound = out[0].known().unwrap();
        let truth = 0.35 - 0.25;
        assert!(
            bound.lo <= truth && truth <= bound.hi,
            "{bound:?}, truth={truth}"
        );
        assert_eq!(out[0], solid_gap_reference(&m, solids[0]));
        eprintln!("finite cap-edge analytic truth={truth} mm, interval={bound:?}");
    }

    #[test]
    fn finite_cap_plane_and_parallel_plane() {
        let m = Mesh::new(
            vec![-5., 5., 0.],
            vec![-5., -5., 5.],
            vec![0.; 3],
            vec![0, 1, 2],
        )
        .unwrap();
        let solids = [
            Solid::Frustum {
                start: [0., 0., 0.33],
                end: [0., 0., 2.],
                r_start: 0.25,
                r_end: 0.25,
            },
            Solid::Frustum {
                start: [-1., 0., 0.58],
                end: [1., 0., 0.58],
                r_start: 0.25,
                r_end: 0.25,
            },
        ];
        let mut out = Vec::new();
        solid_gaps(&Bvh::build(&m).unwrap(), &solids, &mut out);
        for (s, r) in solids.into_iter().zip(out) {
            assert_eq!(r, solid_gap_reference(&m, s));
            let b = r.known().unwrap();
            assert!(b.lo > 0.32999 && b.lo <= 0.33);
        }
    }
}
