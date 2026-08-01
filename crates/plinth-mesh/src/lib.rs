//! The f64 mesh substrate for Plinth.
//!
//! Meshes are deliberately retained as their source supplied them.  In
//! particular, binary STL is an indexed *triangle soup* on disk: loading it
//! creates three vertices per facet and never performs an implicit weld,
//! reorder, repair, or f32 in-memory conversion.
#![forbid(unsafe_code)]

mod fixtures;
mod io;
mod topology;
mod triage;

pub use fixtures::{
    make_box, make_box_default, make_overhang_table, make_overhang_table_with, make_raking_blade,
    make_raking_blade_with, make_thin_fin, make_thin_fin_with, make_toed_slab, make_toed_slab_with,
};
pub use io::{load_binary_stl, load_binary_stls, write_binary_stl, write_binary_stls, MeshIoError};
pub use topology::{
    build_face_topology, mesh_fingerprint, validate_face_topology, FaceTopology, TopologyDigest,
    TopologyError, TopologySchema, TopologyValidationError, FACE_TOPOLOGY_SCHEMA,
    FACE_TOPOLOGY_SCHEMA_VERSION, TOPOLOGY_CUT_BOUNDARY, TOPOLOGY_CUT_DEGENERATE,
    TOPOLOGY_CUT_DUPLICATE, TOPOLOGY_CUT_NONMANIFOLD, TOPOLOGY_CUT_POINT_ONLY,
    TOPOLOGY_CUT_SAME_DIRECTION,
};
pub use triage::{
    triage_components, triage_settings, ComponentDescription, PreHollowed, TriageError,
    TriageSettings, TriageSummary, TriagedMesh,
};

use thiserror::Error;

/// An owned f64 triangle mesh.  Vertex and triangle ordering are source data.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    verts: Vec<[f64; 3]>,
    tris: Vec<[u32; 3]>,
    weld_policy: WeldPolicy,
}

impl Mesh {
    /// Builds a finite, indexed mesh without altering its representation.
    pub fn new(verts: Vec<[f64; 3]>, tris: Vec<[u32; 3]>) -> Result<Self, MeshError> {
        Self::new_unwelded(verts, tris)
    }

    pub(crate) fn new_unwelded(
        verts: Vec<[f64; 3]>,
        tris: Vec<[u32; 3]>,
    ) -> Result<Self, MeshError> {
        Self::validate_parts(&verts, &tris)?;
        Ok(Self {
            verts,
            tris,
            weld_policy: WeldPolicy::None,
        })
    }

    /// Builds a mesh whose arrays were already explicitly welded by the
    /// caller.  This is crate-private so an IO path cannot falsely advertise
    /// a welding policy without changing the arrays it returns.
    pub(crate) fn new_welded(
        verts: Vec<[f64; 3]>,
        tris: Vec<[u32; 3]>,
        weld_policy: WeldPolicy,
    ) -> Result<Self, MeshError> {
        Self::validate_parts(&verts, &tris)?;
        debug_assert!(!matches!(weld_policy, WeldPolicy::None));
        Ok(Self {
            verts,
            tris,
            weld_policy,
        })
    }

    /// Borrow source-order vertices without permitting mutation.
    #[must_use]
    pub fn verts(&self) -> &[[f64; 3]] {
        &self.verts
    }

    /// Borrow source-order triangle indices without permitting mutation.
    #[must_use]
    pub fn tris(&self) -> &[[u32; 3]] {
        &self.tris
    }

    pub(crate) fn validate(&self) -> Result<(), MeshError> {
        Self::validate_parts(&self.verts, &self.tris)
    }

    fn validate_parts(verts: &[[f64; 3]], tris: &[[u32; 3]]) -> Result<(), MeshError> {
        if verts.is_empty() {
            return Err(MeshError::NoVertices);
        }
        if tris.is_empty() {
            return Err(MeshError::NoTriangles);
        }
        for (vertex, value) in verts.iter().enumerate() {
            for (axis, coordinate) in value.iter().enumerate() {
                if !coordinate.is_finite() {
                    return Err(MeshError::NonFiniteVertex { vertex, axis });
                }
            }
        }
        for (triangle, indices) in tris.iter().enumerate() {
            for (corner, index) in indices.iter().enumerate() {
                if (*index as usize) >= verts.len() {
                    return Err(MeshError::IndexOutOfBounds {
                        triangle,
                        corner,
                        index: *index,
                        vertex_count: verts.len(),
                    });
                }
            }
        }
        Ok(())
    }

    /// The policy explicitly selected for this mesh's construction or load.
    #[must_use]
    pub const fn weld_policy(&self) -> WeldPolicy {
        self.weld_policy
    }

    /// The minimum and maximum Z coordinates in source vertex order.
    #[must_use]
    pub fn bounds_z(&self) -> (f64, f64) {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for vertex in &self.verts {
            min = min.min(vertex[2]);
            max = max.max(vertex[2]);
        }
        (min, max)
    }

    /// Unit-face-normal Z components in triangle order.
    pub fn face_normals_z(&self) -> Result<Vec<f64>, MeshError> {
        self.tris
            .iter()
            .enumerate()
            .map(|(triangle, indices)| {
                let a = self.verts[indices[0] as usize];
                let b = self.verts[indices[1] as usize];
                let c = self.verts[indices[2] as usize];
                let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                let normal = [
                    ab[1] * ac[2] - ab[2] * ac[1],
                    ab[2] * ac[0] - ab[0] * ac[2],
                    ab[0] * ac[1] - ab[1] * ac[0],
                ];
                let length_squared = normal.iter().map(|value| value * value).sum::<f64>();
                if !length_squared.is_finite() {
                    return Err(MeshError::NormalArithmeticOverflow { triangle });
                }
                if length_squared == 0.0 {
                    return Err(MeshError::DegenerateTriangle { triangle });
                }
                let length = length_squared.sqrt();
                let z = normal[2] / length;
                if !z.is_finite() {
                    return Err(MeshError::NormalArithmeticOverflow { triangle });
                }
                Ok(z)
            })
            .collect()
    }
}

/// A deliberately explicit vertex welding choice.
///
/// M2's IO applies only [`WeldPolicy::None`].  The other variants remain
/// vocabulary for the later topology/triage operation; requesting either at
/// this boundary returns a typed unsupported-policy error rather than merely
/// recording a false label.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WeldPolicy {
    /// Preserve the source triangle soup exactly (the STL default).
    None,
    /// Weld only coordinates with identical IEEE f64 bit patterns.
    ExactBits,
    /// Weld coordinates within this positive, finite distance.
    Epsilon(f64),
}

/// Semantic failures at a mesh API boundary.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum MeshError {
    #[error("mesh has no vertices")]
    NoVertices,
    #[error("mesh has no triangles")]
    NoTriangles,
    #[error("vertex {vertex} coordinate {axis} is not finite")]
    NonFiniteVertex { vertex: usize, axis: usize },
    #[error("triangle {triangle} corner {corner} references vertex {index}, but only {vertex_count} vertices exist")]
    IndexOutOfBounds {
        triangle: usize,
        corner: usize,
        index: u32,
        vertex_count: usize,
    },
    #[error("triangle {triangle} has zero area")]
    DegenerateTriangle { triangle: usize },
    #[error("triangle {triangle} overflowed while calculating its normal")]
    NormalArithmeticOverflow { triangle: usize },
    #[error("fixture parameter {parameter} must be finite and greater than zero, got {value}")]
    InvalidPositiveFixtureParameter { parameter: &'static str, value: f64 },
    #[error("fixture parameter {parameter} must be finite, got {value}")]
    InvalidFiniteFixtureParameter { parameter: &'static str, value: f64 },
    #[error("fixture has too many vertices for u32 triangle indices")]
    FixtureIndexOverflow,
}
