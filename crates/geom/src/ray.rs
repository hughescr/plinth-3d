//! Batched, two-sided first hits on the frozen mesh. Stable triangle order breaks ties.
//! The scalar reference visits every triangle; production enumerates a conservative
//! ray/box BVH superset before the shared triangle narrow phase.

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
    hit_triangles(mesh, ray, 0..mesh.triangle_count())
}

fn ray_box(ray: Ray, lo: [f64; 3], hi: [f64; 3]) -> bool {
    let mut near = 0.0_f64;
    let mut far = f64::INFINITY;
    for k in 0..3 {
        // Covers the barycentric tolerance as well as f64 slab arithmetic.
        let slack = 1e-10 * (1.0 + lo[k].abs().max(hi[k].abs()));
        let a = lo[k] - slack - ray.origin[k];
        let b = hi[k] + slack - ray.origin[k];
        if ray.dir[k] == 0.0 {
            if a > 0.0 || b < 0.0 {
                return false;
            }
        } else {
            let x = a / ray.dir[k];
            let y = b / ray.dir[k];
            near = near.max(x.min(y).next_down());
            far = far.min(x.max(y).next_up());
        }
    }
    near <= far
}

fn hit_triangles(
    mesh: &Mesh,
    ray: Ray,
    triangles: impl IntoIterator<Item = usize>,
) -> Measure<RayHit> {
    if ray.origin.iter().chain(&ray.dir).any(|v| !v.is_finite()) || dot(ray.dir, ray.dir) < 1e-24 {
        return Measure::Unverified("non-finite or zero ray");
    }
    let mut best = f64::INFINITY;
    let mut index = 0;
    for i in triangles {
        let [a, b, c] = triangle(mesh, i);
        let lo = std::array::from_fn(|k| a[k].min(b[k]).min(c[k]));
        let hi = std::array::from_fn(|k| a[k].max(b[k]).max(c[k]));
        if !ray_box(ray, lo, hi) {
            continue;
        }
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

/// Explicit bounded-worker BVH traversal; never calls the brute-force reference.
pub fn first_hits_with_workers(
    bvh: &Bvh<'_>,
    rays: &[Ray],
    out: &mut Vec<Measure<RayHit>>,
    workers: usize,
) {
    crate::counters::record(crate::counters::Op::FirstHits, rays.len());
    crate::patch::map_batch(rays, out, workers, |&ray| {
        let mut candidates = Vec::new();
        bvh.visit_bounds(|lo, hi| ray_box(ray, lo, hi), |t| candidates.push(t));
        candidates.sort_unstable();
        hit_triangles(bvh.mesh(), ray, candidates.into_iter().map(|t| t as usize))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn randomized_bvh_rays_match_brute_force() {
        let mesh = crate::bvh::query_test_mesh();
        let bvh = Bvh::build(&mesh).unwrap();
        let mut seed = 987_u64;
        let rays: Vec<_> = (0..256)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let i = (seed >> 32) as usize % 100;
                let offset = (seed & 65535) as f64 / 65535.0;
                Ray {
                    origin: [
                        (i % 10) as f64 * 4.0 + offset,
                        (i / 10) as f64 * 4.0 + 0.5,
                        -1.0,
                    ],
                    dir: [offset * 0.05, 0.0, 1.0],
                }
            })
            .collect();
        let mut out = Vec::new();
        first_hits(&bvh, &rays, &mut out);
        for (&ray, actual) in rays.iter().zip(out) {
            assert_eq!(actual, first_hit_reference(&mesh, ray));
        }
    }

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
