//! The simple scalar reference for the batched closest-point query: every query against
//! every triangle, no acceleration structure, no threads. Tests compare the batched
//! kernels against it (design section 6, T0 and T0p). It uses the same point-to-triangle
//! kernel and the same tie-break (lowest triangle number), so the comparison is exact.

use crate::error::{GeomError, Result};
use crate::mesh::Mesh;
use crate::query::{ClosestHits, Points};
use crate::tri::{closest_on_triangle, dist2};

/// Closest point on `mesh` for every query, by brute force.
///
/// # Errors
/// Returns [`GeomError::EmptyMesh`] for a mesh with no triangles.
pub fn closest_points_reference(mesh: &Mesh, queries: &Points) -> Result<ClosestHits> {
    if mesh.triangle_count() == 0 {
        return Err(GeomError::EmptyMesh);
    }
    let mut out = ClosestHits::with_len(queries.len());
    for i in 0..queries.len() {
        let p = [queries.xs()[i], queries.ys()[i], queries.zs()[i]];
        let mut best = (f64::INFINITY, u32::MAX, p);
        for t in 0..mesh.triangle_count() {
            let [a, b, c] = mesh.triangle_corners(t);
            let q = closest_on_triangle(p, a, b, c);
            let d2 = dist2(p, q);
            if d2 < best.0 {
                best = (d2, t as u32, q);
            }
        }
        out.tri[i] = best.1;
        out.x[i] = best.2[0];
        out.y[i] = best.2[1];
        out.z[i] = best.2[2];
        out.dist2[i] = best.0;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bvh::Bvh;
    use crate::query::closest_points;
    use crate::testutil::cube_soup;

    #[test]
    fn reference_equals_batched_on_a_cube_grid() {
        let (x, y, z) = cube_soup();
        let m = Mesh::from_soup(&x, &y, &z).expect("cube");
        let b = Bvh::build(&m).expect("bvh");
        let (mut qx, mut qy, mut qz) = (Vec::new(), Vec::new(), Vec::new());
        for i in 0..9 {
            for j in 0..9 {
                for k in 0..9 {
                    qx.push(f64::from(i) * 0.25 - 0.5);
                    qy.push(f64::from(j) * 0.25 - 0.5);
                    qz.push(f64::from(k) * 0.25 - 0.5);
                }
            }
        }
        let q = Points::new(qx, qy, qz).expect("points");
        let want = closest_points_reference(&m, &q).expect("reference");
        let mut got = ClosestHits::with_len(q.len());
        closest_points(&b, &q, &mut got).expect("batched");
        assert_eq!(got, want);
    }
}
