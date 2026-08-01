//! Immutable face-local geometry shared by a surface and its future indexes.
//!
//! Soup construction deliberately preserves caller-supplied normals, areas and
//! bounds. Indexed construction derives the same face-local representation.

use std::sync::Arc;

use crate::vec3::{add, V3};

/// Read-only per-face bounds, represented as `[minimum, maximum]`.
pub type FaceBounds = [V3; 2];

/// Immutable, face-local geometry. Its data is never mutated after surface
/// construction; indexes and result-inert caches live on `NativeSurface`.
pub struct FaceGeometry {
    pub(crate) nfaces: usize,
    pub(crate) v0: Vec<V3>,
    pub(crate) e1: Vec<V3>,
    pub(crate) e2: Vec<V3>,
    pub(crate) normals: Vec<V3>,
    pub(crate) areas: Vec<f64>,
    pub(crate) face_bounds: Vec<FaceBounds>,
    pub(crate) centroid: Vec<V3>,
    pub(crate) bounds: FaceBounds,
}

impl FaceGeometry {
    /// Global face normals in input/indexed face order.
    pub fn normals(&self) -> &[[f64; 3]] {
        &self.normals
    }
    /// Global face areas in input/indexed face order.
    pub fn areas(&self) -> &[f64] {
        &self.areas
    }
    /// Global face centroids in input/indexed face order.
    pub fn centroids(&self) -> &[[f64; 3]] {
        &self.centroid
    }
    /// Per-face `[minimum, maximum]` bounds in global face order.
    pub fn face_bounds(&self) -> &[FaceBounds] {
        &self.face_bounds
    }
    /// Overall `[minimum, maximum]` bounds supplied or derived at construction.
    pub fn bounds(&self) -> FaceBounds {
        self.bounds
    }
    /// Number of global faces.
    pub fn nfaces(&self) -> usize {
        self.nfaces
    }
}

/// Internal query view. The Arc ensures future subset surfaces can share one
/// immutable face store while retaining independent BVH/parity caches.
pub struct Mesh {
    pub(crate) geometry: Arc<FaceGeometry>,
    pub(crate) nfaces: usize,
}

impl Mesh {
    pub fn new(
        tris: &[f64],
        normals: &[f64],
        areas: &[f64],
        bounds: &[f64],
    ) -> Result<Self, String> {
        if !tris.len().is_multiple_of(9) {
            return Err(format!("tris length {} not a multiple of 9", tris.len()));
        }
        let nfaces = tris.len() / 9;
        if nfaces == 0 {
            return Err("empty mesh (0 faces) — the oracle cannot build one either".into());
        }
        if normals.len() != nfaces * 3 {
            return Err(format!(
                "normals length {} != nfaces*3 ({})",
                normals.len(),
                nfaces * 3
            ));
        }
        if areas.len() != nfaces {
            return Err(format!(
                "areas length {} != nfaces ({})",
                areas.len(),
                nfaces
            ));
        }
        if bounds.len() != 6 {
            return Err(format!("bounds length {} != 6", bounds.len()));
        }

        let mut v0 = Vec::with_capacity(nfaces);
        let mut e1 = Vec::with_capacity(nfaces);
        let mut e2 = Vec::with_capacity(nfaces);
        let mut nrm = Vec::with_capacity(nfaces);
        let mut face_bounds = Vec::with_capacity(nfaces);
        let mut centroid = Vec::with_capacity(nfaces);
        for f in 0..nfaces {
            let base = f * 9;
            let a: V3 = [tris[base], tris[base + 1], tris[base + 2]];
            let b: V3 = [tris[base + 3], tris[base + 4], tris[base + 5]];
            let c: V3 = [tris[base + 6], tris[base + 7], tris[base + 8]];
            let ee1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ee2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let mn = [
                a[0].min(b[0]).min(c[0]),
                a[1].min(b[1]).min(c[1]),
                a[2].min(b[2]).min(c[2]),
            ];
            let mx = [
                a[0].max(b[0]).max(c[0]),
                a[1].max(b[1]).max(c[1]),
                a[2].max(b[2]).max(c[2]),
            ];
            let ctr = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            v0.push(a);
            e1.push(ee1);
            e2.push(ee2);
            nrm.push([normals[f * 3], normals[f * 3 + 1], normals[f * 3 + 2]]);
            face_bounds.push([mn, mx]);
            centroid.push(ctr);
        }
        Ok(Self {
            geometry: Arc::new(FaceGeometry {
                nfaces,
                v0,
                e1,
                e2,
                normals: nrm,
                areas: areas.to_vec(),
                face_bounds,
                centroid,
                bounds: [
                    [bounds[0], bounds[1], bounds[2]],
                    [bounds[3], bounds[4], bounds[5]],
                ],
            }),
            nfaces,
        })
    }
    #[inline]
    pub(crate) fn nfaces(&self) -> usize {
        self.nfaces
    }
    #[inline]
    pub(crate) fn v0(&self, i: usize) -> V3 {
        self.geometry.v0[i]
    }
    #[inline]
    pub(crate) fn e1(&self, i: usize) -> V3 {
        self.geometry.e1[i]
    }
    #[inline]
    pub(crate) fn e2(&self, i: usize) -> V3 {
        self.geometry.e2[i]
    }
    #[inline]
    pub(crate) fn normal(&self, i: usize) -> V3 {
        self.geometry.normals[i]
    }
    #[inline]
    pub(crate) fn tri_min(&self, i: usize) -> V3 {
        self.geometry.face_bounds[i][0]
    }
    #[inline]
    pub(crate) fn tri_max(&self, i: usize) -> V3 {
        self.geometry.face_bounds[i][1]
    }
    #[inline]
    pub(crate) fn centroid(&self, i: usize) -> V3 {
        self.geometry.centroid[i]
    }
    #[inline]
    pub(crate) fn bounds(&self) -> FaceBounds {
        self.geometry.bounds
    }
    #[inline]
    pub(crate) fn vertex_b(&self, i: usize) -> V3 {
        add(self.v0(i), self.e1(i))
    }
    #[inline]
    pub(crate) fn vertex_c(&self, i: usize) -> V3 {
        add(self.v0(i), self.e2(i))
    }
}
