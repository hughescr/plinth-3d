//! The structure-of-arrays indexed triangle mesh.
//!
//! Vertices live in three contiguous f64 arrays (`xs`, `ys`, `zs`); triangles are one
//! contiguous `u32` array with three vertex indices per triangle. Meshes built from a
//! triangle soup are welded (equal positions share one vertex) and Morton-reordered, so
//! the result depends only on the input values, never on hash order or thread count.

use crate::aabb::Aabb;
use crate::error::{GeomError, Result};
use crate::morton::morton_keys;
use std::cmp::Ordering;

/// An indexed triangle mesh in structure-of-arrays layout.
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    xs: Vec<f64>,
    ys: Vec<f64>,
    zs: Vec<f64>,
    indices: Vec<u32>,
    bounds: Aabb,
}

fn check_finite(what: &'static str, v: &[f64]) -> Result<()> {
    match v.iter().position(|x| !x.is_finite()) {
        Some(index) => Err(GeomError::NonFinite { what, index }),
        None => Ok(()),
    }
}

fn check_same_len(what: &'static str, expected: usize, actual: usize) -> Result<()> {
    if expected == actual {
        Ok(())
    } else {
        Err(GeomError::LengthMismatch {
            what,
            expected,
            actual,
        })
    }
}

impl Mesh {
    /// Build a mesh from vertex arrays and triangle indices (three per triangle). Checks
    /// array lengths, finiteness and index range; the order is kept as given.
    ///
    /// # Errors
    /// Returns the first problem found.
    pub fn new(xs: Vec<f64>, ys: Vec<f64>, zs: Vec<f64>, indices: Vec<u32>) -> Result<Self> {
        check_same_len("ys", xs.len(), ys.len())?;
        check_same_len("zs", xs.len(), zs.len())?;
        if !indices.len().is_multiple_of(3) {
            return Err(GeomError::LengthMismatch {
                what: "indices (multiple of 3)",
                expected: indices.len() - indices.len() % 3,
                actual: indices.len(),
            });
        }
        if u32::try_from(xs.len()).is_err() {
            return Err(GeomError::TooLarge {
                what: "vertex",
                count: xs.len(),
            });
        }
        if u32::try_from(indices.len() / 3).is_err() {
            return Err(GeomError::TooLarge {
                what: "triangle",
                count: indices.len() / 3,
            });
        }
        check_finite("xs", &xs)?;
        check_finite("ys", &ys)?;
        check_finite("zs", &zs)?;
        if let Some(p) = indices.iter().position(|&i| i as usize >= xs.len()) {
            return Err(GeomError::BadIndex {
                triangle: p / 3,
                index: indices[p],
                vertices: xs.len(),
            });
        }
        let bounds = Aabb::of_points(&xs, &ys, &zs);
        Ok(Self {
            xs,
            ys,
            zs,
            indices,
            bounds,
        })
    }

    /// Build a mesh from a triangle soup: three corners per triangle, corner `k` at
    /// `(sx[k], sy[k], sz[k])`. Equal positions are welded to one vertex (`-0.0` equals
    /// `0.0`), then the mesh is Morton-reordered. Triangle winding is kept.
    ///
    /// # Errors
    /// Returns an error for unequal lengths, a corner count that is not a multiple of 3,
    /// too many corners for 32-bit indices, or a non-finite coordinate.
    pub fn from_soup(sx: &[f64], sy: &[f64], sz: &[f64]) -> Result<Self> {
        check_same_len("soup ys", sx.len(), sy.len())?;
        check_same_len("soup zs", sx.len(), sz.len())?;
        if !sx.len().is_multiple_of(3) {
            return Err(GeomError::LengthMismatch {
                what: "soup corners (multiple of 3)",
                expected: sx.len() - sx.len() % 3,
                actual: sx.len(),
            });
        }
        if u32::try_from(sx.len()).is_err() {
            return Err(GeomError::TooLarge {
                what: "corner",
                count: sx.len(),
            });
        }
        check_finite("soup xs", sx)?;
        check_finite("soup ys", sy)?;
        check_finite("soup zs", sz)?;
        Ok(weld(sx, sy, sz).morton_reordered())
    }

    /// Vertex x coordinates.
    #[must_use]
    pub fn xs(&self) -> &[f64] {
        &self.xs
    }

    /// Vertex y coordinates.
    #[must_use]
    pub fn ys(&self) -> &[f64] {
        &self.ys
    }

    /// Vertex z coordinates.
    #[must_use]
    pub fn zs(&self) -> &[f64] {
        &self.zs
    }

    /// Triangle vertex indices, three per triangle.
    #[must_use]
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    /// Number of vertices.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.xs.len()
    }

    /// Number of triangles.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Bounding box of the vertices (computed once at construction).
    #[must_use]
    pub fn bounds(&self) -> Aabb {
        self.bounds
    }

    /// The three corner positions of triangle `t`. For tests and reference code; geometry
    /// kernels read the arrays directly.
    ///
    /// # Panics
    /// Panics when `t` is not a triangle of this mesh.
    #[must_use]
    pub fn triangle_corners(&self, t: usize) -> [[f64; 3]; 3] {
        let at = |k: usize| {
            let i = self.indices[3 * t + k] as usize;
            [self.xs[i], self.ys[i], self.zs[i]]
        };
        [at(0), at(1), at(2)]
    }

    /// A copy with vertices and then triangles sorted along the Morton curve (triangles by
    /// their centroid). Ties break by the old index, so the order is fully determined.
    /// Corner order inside each triangle is kept.
    #[must_use]
    pub fn morton_reordered(&self) -> Self {
        let nv = self.xs.len();
        let mut keys = vec![0_u64; nv];
        morton_keys(&self.xs, &self.ys, &self.zs, &self.bounds, &mut keys);
        let mut vperm: Vec<u32> = (0..nv as u32).collect();
        vperm.sort_unstable_by_key(|&i| (keys[i as usize], i));
        let mut new_of_old = vec![0_u32; nv];
        for (new, &old) in vperm.iter().enumerate() {
            new_of_old[old as usize] = new as u32;
        }
        let gather = |src: &[f64]| -> Vec<f64> { vperm.iter().map(|&o| src[o as usize]).collect() };
        let (xs, ys, zs) = (gather(&self.xs), gather(&self.ys), gather(&self.zs));
        let remapped: Vec<u32> = self
            .indices
            .iter()
            .map(|&i| new_of_old[i as usize])
            .collect();
        let indices = reorder_triangles(&xs, &ys, &zs, &self.bounds, &remapped);
        Self {
            xs,
            ys,
            zs,
            indices,
            bounds: self.bounds,
        }
    }
}

/// Sort triangles by the Morton key of their centroid; returns the new index array.
fn reorder_triangles(xs: &[f64], ys: &[f64], zs: &[f64], bounds: &Aabb, idx: &[u32]) -> Vec<u32> {
    let nt = idx.len() / 3;
    let mut cx = Vec::with_capacity(nt);
    let mut cy = Vec::with_capacity(nt);
    let mut cz = Vec::with_capacity(nt);
    for t in idx.chunks_exact(3) {
        let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
        cx.push((xs[a] + xs[b] + xs[c]) / 3.0);
        cy.push((ys[a] + ys[b] + ys[c]) / 3.0);
        cz.push((zs[a] + zs[b] + zs[c]) / 3.0);
    }
    let mut keys = vec![0_u64; nt];
    morton_keys(&cx, &cy, &cz, bounds, &mut keys);
    let mut tperm: Vec<u32> = (0..nt as u32).collect();
    tperm.sort_unstable_by_key(|&t| (keys[t as usize], t));
    let mut out = Vec::with_capacity(idx.len());
    for &t in &tperm {
        let t = t as usize;
        out.extend_from_slice(&idx[3 * t..3 * t + 3]);
    }
    out
}

/// Weld equal corner positions into shared vertices. Vertices come out sorted by
/// (x, y, z), so the result is a pure function of the input values.
fn weld(sx: &[f64], sy: &[f64], sz: &[f64]) -> Mesh {
    // Adding 0.0 turns -0.0 into +0.0, so both zeros weld together.
    let nx = |i: usize| sx[i] + 0.0;
    let ny = |i: usize| sy[i] + 0.0;
    let nz = |i: usize| sz[i] + 0.0;
    let n = sx.len();
    let mut order: Vec<u32> = (0..n as u32).collect();
    order.sort_unstable_by(|&a, &b| {
        let (a, b) = (a as usize, b as usize);
        nx(a)
            .total_cmp(&nx(b))
            .then_with(|| ny(a).total_cmp(&ny(b)))
            .then_with(|| nz(a).total_cmp(&nz(b)))
            .then_with(|| a.cmp(&b))
    });
    let mut xs: Vec<f64> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    let mut zs: Vec<f64> = Vec::new();
    let mut indices = vec![0_u32; n];
    let mut prev: Option<usize> = None;
    for &o in &order {
        let k = o as usize;
        let same = prev.is_some_and(|p| {
            nx(p).total_cmp(&nx(k)) == Ordering::Equal
                && ny(p).total_cmp(&ny(k)) == Ordering::Equal
                && nz(p).total_cmp(&nz(k)) == Ordering::Equal
        });
        if !same {
            xs.push(nx(k));
            ys.push(ny(k));
            zs.push(nz(k));
        }
        indices[k] = (xs.len() - 1) as u32;
        prev = Some(k);
    }
    let bounds = Aabb::of_points(&xs, &ys, &zs);
    Mesh {
        xs,
        ys,
        zs,
        indices,
        bounds,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{assert_pt, cube_soup};

    #[test]
    fn soup_cube_welds_to_eight_vertices_and_twelve_triangles() {
        let (x, y, z) = cube_soup();
        let m = Mesh::from_soup(&x, &y, &z).expect("cube");
        assert_eq!(m.vertex_count(), 8);
        assert_eq!(m.triangle_count(), 12);
        assert_pt(m.bounds().min, [0.0; 3]);
        assert_pt(m.bounds().max, [1.0; 3]);
        assert_eq!(m.indices().len(), 36);
        // Every triangle still has the area of half a unit square.
        for t in 0..12 {
            let [a, b, c] = m.triangle_corners(t);
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let w = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                u[1] * w[2] - u[2] * w[1],
                u[2] * w[0] - u[0] * w[2],
                u[0] * w[1] - u[1] * w[0],
            ];
            let area = 0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!((area - 0.5).abs() < 1e-12, "triangle {t} area {area}");
        }
    }

    #[test]
    fn negative_zero_welds_with_positive_zero() {
        let m =
            Mesh::from_soup(&[0.0, -0.0, 1.0], &[0.0, -0.0, 0.0], &[0.0, 0.0, -0.0]).expect("soup");
        assert_eq!(m.vertex_count(), 2);
    }

    #[test]
    fn reorder_is_deterministic_and_keeps_the_triangle_set() {
        let (x, y, z) = cube_soup();
        let m = Mesh::from_soup(&x, &y, &z).expect("cube");
        assert_eq!(
            m.morton_reordered(),
            m,
            "reordering a reordered mesh changes nothing"
        );
        // Reversing the input order of triangles gives the same mesh.
        let mut r = (x.clone(), y.clone(), z.clone());
        for a in [&mut r.0, &mut r.1, &mut r.2] {
            let mut tris: Vec<[f64; 3]> = a.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
            tris.reverse();
            *a = tris.concat();
        }
        let m2 = Mesh::from_soup(&r.0, &r.1, &r.2).expect("reversed cube");
        assert_eq!(m2.vertex_count(), 8);
        assert_eq!(m2.xs(), m.xs());
        assert_eq!(m2.ys(), m.ys());
        assert_eq!(m2.zs(), m.zs());
    }

    #[test]
    fn constructors_reject_bad_input() {
        let ok = vec![0.0, 1.0, 0.0];
        assert!(matches!(
            Mesh::new(ok.clone(), ok.clone(), ok.clone(), vec![0, 1, 3]),
            Err(GeomError::BadIndex {
                triangle: 0,
                index: 3,
                vertices: 3
            })
        ));
        assert!(matches!(
            Mesh::new(ok.clone(), ok.clone(), vec![0.0], vec![0, 1, 2]),
            Err(GeomError::LengthMismatch { .. })
        ));
        assert!(matches!(
            Mesh::new(ok.clone(), ok.clone(), ok.clone(), vec![0, 1]),
            Err(GeomError::LengthMismatch { .. })
        ));
        assert!(matches!(
            Mesh::new(
                vec![0.0, f64::NAN, 0.0],
                ok.clone(),
                ok.clone(),
                vec![0, 1, 2]
            ),
            Err(GeomError::NonFinite {
                what: "xs",
                index: 1
            })
        ));
        assert!(matches!(
            Mesh::from_soup(&[0.0, 1.0], &[0.0, 1.0], &[0.0, 1.0]),
            Err(GeomError::LengthMismatch { .. })
        ));
        assert!(matches!(
            Mesh::from_soup(&[0.0, 1.0, f64::INFINITY], &[0.0; 3], &[0.0; 3]),
            Err(GeomError::NonFinite { .. })
        ));
    }
}
