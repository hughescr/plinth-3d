//! Immutable face-local geometry shared by surface-local query indexes.

use std::sync::Arc;

use crate::vec3::{add, V3};
use crate::SurfaceBuildError;

pub type FaceBounds = [V3; 2];

/// Why an indexed face has no normal.
///
/// A triangle whose three corners span no area has no plane and therefore no
/// normal. There is nothing to store for it and nothing a consumer can do with
/// it, so [`crate::SurfaceBuildError::DegenerateFace`] carries one of these
/// instead of a fabricated direction. The three cases are separated because
/// they say different things about the CALLER's mesh: the first is an index
/// bug, the second a weld/precision bug, the third a genuine sliver collapsed
/// to a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DegenerateFaceCause {
    /// Two of the face's three corners name the SAME vertex index. The face is
    /// a line segment by construction; no coordinate value can rescue it.
    RepeatedVertexIndex {
        /// The two corner slots (0, 1 or 2) that carry the repeated index.
        corners: (usize, usize),
        /// The vertex index they both name.
        index: u32,
    },
    /// The corners are three DISTINCT indices, but two of them sit at exactly
    /// the same coordinates -- a mesh that was never welded, or one welded with
    /// a tolerance coarser than its own feature size.
    CoincidentVertices {
        /// The two corner slots (0, 1 or 2) whose coordinates are equal.
        corners: (usize, usize),
    },
    /// Three distinct, non-coincident corners that lie on one line, so the edge
    /// cross product is exactly zero. This is a real triangle that has
    /// collapsed, not a bookkeeping mistake.
    CollinearVertices,
}

impl DegenerateFaceCause {
    /// Name the reason a face's edge cross product came out exactly zero.
    ///
    /// Classification only: the CALLER has already established that the face is
    /// degenerate. This never decides degeneracy itself, so it cannot disagree
    /// with the predicate that did.
    fn classify(indices: [u32; 3], points: [V3; 3]) -> Self {
        for (left, right) in [(0_usize, 1_usize), (0, 2), (1, 2)] {
            if indices[left] == indices[right] {
                return Self::RepeatedVertexIndex {
                    corners: (left, right),
                    index: indices[left],
                };
            }
        }
        for (left, right) in [(0_usize, 1_usize), (0, 2), (1, 2)] {
            if points[left] == points[right] {
                return Self::CoincidentVertices {
                    corners: (left, right),
                };
            }
        }
        Self::CollinearVertices
    }
}

impl std::fmt::Display for DegenerateFaceCause {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RepeatedVertexIndex {
                corners: (left, right),
                index,
            } => write!(
                formatter,
                "corners {left} and {right} both name vertex {index}"
            ),
            Self::CoincidentVertices {
                corners: (left, right),
            } => write!(
                formatter,
                "corners {left} and {right} are distinct indices at identical coordinates"
            ),
            Self::CollinearVertices => {
                write!(formatter, "the three corners are collinear")
            }
        }
    }
}

/// Immutable global face data. Query indexes and caches deliberately do not
/// live here: a future subset can share this store but own independent indexes.
pub struct FaceGeometry {
    v0: Vec<V3>,
    /// Exact source coordinates retained independently of edge arithmetic.
    /// Plane slicing needs these bits at tolerance boundaries.
    v1: Vec<V3>,
    v2: Vec<V3>,
    e1: Vec<V3>,
    e2: Vec<V3>,
    normals: Vec<V3>,
    areas: Vec<f64>,
    face_bounds: Vec<FaceBounds>,
    centroid: Vec<V3>,
    bounds: FaceBounds,
}

impl FaceGeometry {
    pub fn normals(&self) -> &[[f64; 3]] {
        &self.normals
    }
    pub fn areas(&self) -> &[f64] {
        &self.areas
    }
    pub fn centroids(&self) -> &[[f64; 3]] {
        &self.centroid
    }
    pub fn face_bounds(&self) -> &[FaceBounds] {
        &self.face_bounds
    }
    pub fn bounds(&self) -> FaceBounds {
        self.bounds
    }
    pub fn nfaces(&self) -> usize {
        self.v0.len()
    }
}

pub struct Mesh {
    geometry: Arc<FaceGeometry>,
    /// Selected global face IDs, always strictly ascending.
    face_ids: Vec<u32>,
    /// Query bounds for this view. Full soup views retain source bounds;
    /// proper subsets tightly enclose selected faces.
    bounds: FaceBounds,
}

impl Mesh {
    pub(crate) fn validate_indexed_bounds(
        vertices: &[[f64; 3]],
        faces: &[[u32; 3]],
    ) -> Result<FaceBounds, SurfaceBuildError> {
        if vertices.is_empty() {
            return Err(SurfaceBuildError::EmptyVertices);
        }
        if faces.is_empty() {
            return Err(SurfaceBuildError::EmptyFaces);
        }
        for (vertex, point) in vertices.iter().enumerate() {
            for (axis, value) in point.iter().enumerate() {
                if !value.is_finite() {
                    return Err(SurfaceBuildError::NonFiniteVertex { vertex, axis });
                }
            }
        }
        let mut bounds = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
        for (face, indices) in faces.iter().enumerate() {
            for (corner, &index) in indices.iter().enumerate() {
                let Some(point) = vertices.get(index as usize) else {
                    return Err(SurfaceBuildError::FaceIndexOutOfBounds {
                        face,
                        corner,
                        index,
                        vertex_count: vertices.len(),
                    });
                };
                for axis in 0..3 {
                    bounds[0][axis] = bounds[0][axis].min(point[axis]);
                    bounds[1][axis] = bounds[1][axis].max(point[axis]);
                }
            }
        }
        Ok(bounds)
    }

    #[allow(clippy::too_many_arguments)] // exact parallel face arrays are the immutable storage layout
    fn from_parts(
        v0: Vec<V3>,
        v1: Vec<V3>,
        v2: Vec<V3>,
        e1: Vec<V3>,
        e2: Vec<V3>,
        normals: Vec<V3>,
        areas: Vec<f64>,
        face_bounds: Vec<FaceBounds>,
        centroid: Vec<V3>,
        bounds: FaceBounds,
    ) -> Self {
        let n = v0.len();
        debug_assert!(n > 0);
        debug_assert_eq!(v1.len(), n);
        debug_assert_eq!(v2.len(), n);
        debug_assert_eq!(e1.len(), n);
        debug_assert_eq!(e2.len(), n);
        debug_assert_eq!(normals.len(), n);
        debug_assert_eq!(areas.len(), n);
        debug_assert_eq!(face_bounds.len(), n);
        debug_assert_eq!(centroid.len(), n);
        let geometry = Arc::new(FaceGeometry {
            v0,
            v1,
            v2,
            e1,
            e2,
            normals,
            areas,
            face_bounds,
            centroid,
            bounds,
        });
        Self {
            geometry,
            face_ids: (0..n as u32).collect(),
            bounds,
        }
    }

    /// Build once from source-supplied soup metadata. The caller has already
    /// validated shape/finite/bounds; normals, areas and overall bounds are
    /// copied verbatim and never recomputed.
    pub(crate) fn from_soup(
        triangles: &[[[f64; 3]; 3]],
        normals: &[[f64; 3]],
        areas: &[f64],
        bounds: FaceBounds,
    ) -> Self {
        let n = triangles.len();
        let mut v0 = Vec::with_capacity(n);
        let mut v1 = Vec::with_capacity(n);
        let mut v2 = Vec::with_capacity(n);
        let mut e1 = Vec::with_capacity(n);
        let mut e2 = Vec::with_capacity(n);
        let mut face_bounds = Vec::with_capacity(n);
        let mut centroid = Vec::with_capacity(n);
        for triangle in triangles {
            let [a, b, c] = *triangle;
            v0.push(a);
            v1.push(b);
            v2.push(c);
            e1.push([b[0] - a[0], b[1] - a[1], b[2] - a[2]]);
            e2.push([c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
            face_bounds.push([
                [
                    a[0].min(b[0]).min(c[0]),
                    a[1].min(b[1]).min(c[1]),
                    a[2].min(b[2]).min(c[2]),
                ],
                [
                    a[0].max(b[0]).max(c[0]),
                    a[1].max(b[1]).max(c[1]),
                    a[2].max(b[2]).max(c[2]),
                ],
            ]);
            centroid.push([
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ]);
        }
        Self::from_parts(
            v0,
            v1,
            v2,
            e1,
            e2,
            normals.to_vec(),
            areas.to_vec(),
            face_bounds,
            centroid,
            bounds,
        )
    }

    /// Validate and derive the complete immutable indexed face store in one
    /// pass. No temporary triangle soup or second metadata derivation exists.
    pub(crate) fn from_indexed(
        vertices: &[[f64; 3]],
        faces: &[[u32; 3]],
    ) -> Result<Self, SurfaceBuildError> {
        let bounds = Self::validate_indexed_bounds(vertices, faces)?;
        let n = faces.len();
        let mut v0 = Vec::with_capacity(n);
        let mut v1 = Vec::with_capacity(n);
        let mut v2 = Vec::with_capacity(n);
        let mut e1s = Vec::with_capacity(n);
        let mut e2s = Vec::with_capacity(n);
        let mut normals = Vec::with_capacity(n);
        let mut areas = Vec::with_capacity(n);
        let mut face_bounds = Vec::with_capacity(n);
        let mut centroids = Vec::with_capacity(n);
        for (face, indices) in faces.iter().enumerate() {
            let mut points = [[0.0; 3]; 3];
            for (corner, &index) in indices.iter().enumerate() {
                let point = &vertices[index as usize];
                points[corner] = *point;
            }
            let [a, b, c] = points;
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            if !e1.iter().chain(&e2).all(|v| v.is_finite()) {
                return Err(SurfaceBuildError::IndexedDerivedArithmetic {
                    face,
                    operation: "edge subtraction",
                });
            }
            let cross = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            if !cross.iter().all(|v| v.is_finite()) {
                return Err(SurfaceBuildError::IndexedDerivedArithmetic {
                    face,
                    operation: "normal cross product",
                });
            }
            let length = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
            if !length.is_finite() {
                return Err(SurfaceBuildError::IndexedDerivedArithmetic {
                    face,
                    operation: "normal length",
                });
            }
            // A face with no area has no normal. The previous code divided by
            // `length.max(1e-30)`, which turns `length == 0` into a stored
            // "normal" of [0, 0, 0] -- a vector every consumer downstream reads
            // as a DIRECTION (inward = -normal in the breach predicate,
            // dot(normal, ...) in the exterior mask and the contact tests).
            // Zero is not a direction, and it is not a detectable one either:
            // it is finite, so the `is_finite` guard below waves it through.
            //
            // The predicate is exact and carries no tolerance. `length > 0.0`
            // is precisely the condition under which the three divisions
            // produce a unit vector: `|cross[k]| <= length` for every k, so no
            // component can overflow, and only `length == 0.0` can make the
            // quotient meaningless. There is nothing here for a magic number to
            // decide (PHYSICS-FIRST CONSTANTS).
            if length <= 0.0 {
                return Err(SurfaceBuildError::DegenerateFace {
                    face,
                    cause: DegenerateFaceCause::classify(*indices, [a, b, c]),
                });
            }
            let normal = [cross[0] / length, cross[1] / length, cross[2] / length];
            if !normal.iter().all(|v| v.is_finite()) {
                return Err(SurfaceBuildError::IndexedDerivedArithmetic {
                    face,
                    operation: "normalization",
                });
            }
            let area = 0.5 * length;
            if !area.is_finite() {
                return Err(SurfaceBuildError::IndexedDerivedArithmetic {
                    face,
                    operation: "area",
                });
            }
            let centroid = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            if !centroid.iter().all(|v| v.is_finite()) {
                return Err(SurfaceBuildError::IndexedDerivedArithmetic {
                    face,
                    operation: "centroid",
                });
            }
            v0.push(a);
            v1.push(b);
            v2.push(c);
            e1s.push(e1);
            e2s.push(e2);
            normals.push(normal);
            areas.push(area);
            face_bounds.push([
                [
                    a[0].min(b[0]).min(c[0]),
                    a[1].min(b[1]).min(c[1]),
                    a[2].min(b[2]).min(c[2]),
                ],
                [
                    a[0].max(b[0]).max(c[0]),
                    a[1].max(b[1]).max(c[1]),
                    a[2].max(b[2]).max(c[2]),
                ],
            ]);
            centroids.push(centroid);
        }
        Ok(Self::from_parts(
            v0,
            v1,
            v2,
            e1s,
            e2s,
            normals,
            areas,
            face_bounds,
            centroids,
            bounds,
        ))
    }

    // Existing test-only flat construction compatibility.
    pub fn new(
        tris: &[f64],
        normals: &[f64],
        areas: &[f64],
        bounds: &[f64],
    ) -> Result<Self, String> {
        if !tris.len().is_multiple_of(9)
            || tris.is_empty()
            || normals.len() != tris.len() / 3
            || areas.len() != tris.len() / 9
            || bounds.len() != 6
        {
            return Err("invalid mesh buffers".into());
        }
        let triangles: Vec<[[f64; 3]; 3]> = tris
            .chunks_exact(9)
            .map(|t| [[t[0], t[1], t[2]], [t[3], t[4], t[5]], [t[6], t[7], t[8]]])
            .collect();
        let nrms: Vec<V3> = normals
            .chunks_exact(3)
            .map(|n| [n[0], n[1], n[2]])
            .collect();
        Ok(Self::from_soup(
            &triangles,
            &nrms,
            areas,
            [
                [bounds[0], bounds[1], bounds[2]],
                [bounds[3], bounds[4], bounds[5]],
            ],
        ))
    }
    pub(crate) fn geometry(&self) -> &FaceGeometry {
        &self.geometry
    }
    pub(crate) fn triangle_soup_digest(&self) -> crate::SurfaceTriangleSoupDigest {
        crate::triangle_soup_digest_from_iter(self.face_ids.iter().map(|&face_id| {
            let face = face_id as usize;
            [
                self.geometry.v0[face],
                self.geometry.v1[face],
                self.geometry.v2[face],
            ]
        }))
    }
    pub(crate) fn subset(&self, face_ids: Vec<u32>) -> Self {
        debug_assert!(!face_ids.is_empty());
        debug_assert!(face_ids.windows(2).all(|pair| pair[0] < pair[1]));
        debug_assert!(face_ids
            .iter()
            .all(|face_id| (*face_id as usize) < self.geometry.nfaces()));

        // An identity view preserves the parent's validated query bounds. In
        // particular, triangle-soup callers own the full surface bounds and
        // identity subsets must remain bit-equivalent. Proper subsets use the
        // tight union of only their selected faces.
        let bounds = if face_ids == self.face_ids {
            self.bounds
        } else {
            let mut bounds = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
            for &face_id in &face_ids {
                let face_bounds = self.geometry.face_bounds[face_id as usize];
                for axis in 0..3 {
                    bounds[0][axis] = bounds[0][axis].min(face_bounds[0][axis]);
                    bounds[1][axis] = bounds[1][axis].max(face_bounds[1][axis]);
                }
            }
            bounds
        };
        Self {
            geometry: Arc::clone(&self.geometry),
            face_ids,
            bounds,
        }
    }
    pub(crate) fn shares_geometry(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.geometry, &other.geometry)
    }
    #[inline]
    pub(crate) fn face_ids(&self) -> &[u32] {
        &self.face_ids
    }
    #[inline]
    pub(crate) fn contains_face(&self, face_id: u32) -> bool {
        self.face_ids.binary_search(&face_id).is_ok()
    }
    #[inline]
    pub(crate) fn face_slot(&self, face_id: u32) -> Option<usize> {
        self.face_ids.binary_search(&face_id).ok()
    }
    #[inline]
    pub(crate) fn nfaces(&self) -> usize {
        self.face_ids.len()
    }
    #[inline]
    pub(crate) fn global_nfaces(&self) -> usize {
        self.geometry.nfaces()
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
        self.bounds
    }
    #[inline]
    pub(crate) fn vertex_b(&self, i: usize) -> V3 {
        add(self.v0(i), self.e1(i))
    }
    #[inline]
    pub(crate) fn vertex_c(&self, i: usize) -> V3 {
        add(self.v0(i), self.e2(i))
    }
    #[inline]
    pub(crate) fn source_vertex_b(&self, i: usize) -> V3 {
        self.geometry.v1[i]
    }
    #[inline]
    pub(crate) fn source_vertex_c(&self, i: usize) -> V3 {
        self.geometry.v2[i]
    }
}
