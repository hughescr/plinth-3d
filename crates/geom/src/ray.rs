//! Batched, two-sided first hits on the frozen mesh. Stable triangle order breaks ties.
//! The scalar reference deliberately visits every triangle. The batch currently uses the
//! same kernel; BVH traversal is a performance opportunity, not a different answer.

use crate::patch::{cross, dot, sub, triangle};
use crate::{Bvh, Mesh, measure::Measure};

/// Ray in model coordinates. Direction need not be normalized.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    /// Origin, mm.
    pub origin: [f64; 3],
    /// Direction.
    pub dir: [f64; 3],
}
/// First intersection at nonnegative ray parameter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RayHit {
    /// No intersection.
    Miss,
    /// Nearest intersection. Ties use the lowest triangle index.
    Hit {
        /// Parameter in `origin + t * dir`, not Euclidean distance unless dir is unit.
        t: f64,
        /// Triangle index in the frozen mesh.
        triangle: u32,
    },
}

/// Scalar reference for the batched kernel. Degenerate rays are unverified.
#[must_use]
pub fn first_hit_reference(mesh: &Mesh, ray: Ray) -> Measure<RayHit> {
    if ray.origin.iter().chain(&ray.dir).any(|v| !v.is_finite()) || dot(ray.dir, ray.dir) < 1e-24 {
        return Measure::Unverified("non-finite or zero ray");
    }
    let mut best = f64::INFINITY;
    let mut index = 0;
    for i in 0..mesh.triangle_count() {
        let [a, b, c] = triangle(mesh, i);
        let e = sub(b, a);
        let f = sub(c, a);
        let h = cross(ray.dir, f);
        let det = dot(e, h);
        // Parallel triangles cannot cross the ray. Coplanar rays are not assigned a
        // favourable miss: their first surface interval is not a single intersection.
        let scale = dot(e, e).sqrt() * dot(f, f).sqrt() * dot(ray.dir, ray.dir).sqrt();
        if det.abs() <= 1e-14 * scale {
            let n = cross(e, f);
            if dot(n, n) > 1e-24 && dot(sub(ray.origin, a), n).abs() <= 1e-12 * dot(n, n).sqrt() {
                return Measure::Unverified("coplanar ray");
            }
            continue;
        }
        let s = sub(ray.origin, a);
        let u = dot(s, h) / det;
        let q = cross(s, e);
        let v = dot(ray.dir, q) / det;
        let t = dot(f, q) / det;
        if u >= -1e-12 && v >= -1e-12 && u + v <= 1.0 + 1e-12 && t >= 0.0 && t < best {
            best = t;
            index = i as u32;
        }
    }
    Measure::Known(if best.is_finite() {
        RayHit::Hit {
            t: best,
            triangle: index,
        }
    } else {
        RayHit::Miss
    })
}

/// One call for the whole stage, exactly one answer per ray, in input order.
pub fn first_hits(bvh: &Bvh<'_>, rays: &[Ray], out: &mut Vec<Measure<RayHit>>) {
    first_hits_with_workers(bvh, rays, out, 1);
}

/// Explicit bounded-worker version of the same reference kernel.
pub fn first_hits_with_workers(
    bvh: &Bvh<'_>,
    rays: &[Ray],
    out: &mut Vec<Measure<RayHit>>,
    workers: usize,
) {
    crate::counters::record(crate::counters::Op::FirstHits, rays.len());
    crate::patch::map_batch(rays, out, workers, |&ray| {
        first_hit_reference(bvh.mesh(), ray)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rays_match_reference_and_report_parameters_and_ties() {
        let mesh = Mesh::new(
            vec![0., 2., 0.],
            vec![0., 0., 2.],
            vec![1.; 3],
            vec![0, 1, 2, 0, 1, 2],
        )
        .unwrap();
        let rays = [
            Ray {
                origin: [0.25, 0.25, 0.],
                dir: [0., 0., 2.],
            },
            Ray {
                origin: [3., 3., 0.],
                dir: [0., 0., 1.],
            },
            Ray {
                origin: [0.; 3],
                dir: [0.; 3],
            },
        ];
        let mut out = Vec::new();
        first_hits(&Bvh::build(&mesh).unwrap(), &rays, &mut out);
        assert_eq!(
            out,
            rays.iter()
                .map(|&r| first_hit_reference(&mesh, r))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            out[0],
            Measure::Known(RayHit::Hit {
                t: 0.5,
                triangle: 0
            })
        );
        assert_eq!(out[1], Measure::Known(RayHit::Miss));
        assert!(out[2].is_unverified());
    }
}
