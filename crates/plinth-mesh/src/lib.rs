//! The f64 mesh substrate for Plinth.
//!
//! Meshes are deliberately retained as their source supplied them.  In
//! particular, binary STL is an indexed *triangle soup* on disk: loading it
//! creates three vertices per facet and never performs an implicit weld,
//! reorder, repair, or f32 in-memory conversion.
#![forbid(unsafe_code)]

mod fixtures;
mod io;

pub use fixtures::{
    make_box, make_box_default, make_overhang_table, make_overhang_table_with, make_raking_blade,
    make_raking_blade_with, make_thin_fin, make_thin_fin_with, make_toed_slab, make_toed_slab_with,
};
pub use io::{load_binary_stl, load_binary_stls, write_binary_stl, write_binary_stls, MeshIoError};

use thiserror::Error;

/// An owned f64 triangle mesh.  Vertex and triangle ordering are source data.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub verts: Vec<[f64; 3]>,
    pub tris: Vec<[u32; 3]>,
    weld_policy: WeldPolicy,
}

impl Mesh {
    /// Builds a finite, indexed mesh without altering its representation.
    pub fn new(verts: Vec<[f64; 3]>, tris: Vec<[u32; 3]>) -> Result<Self, MeshError> {
        Self::with_weld_policy(verts, tris, WeldPolicy::None)
    }

    /// Builds a mesh and records the explicit welding policy which produced it.
    ///
    /// This constructor intentionally does not weld.  Welding is a future,
    /// named topology operation; recording a policy here prevents a load path
    /// from silently choosing one.
    pub fn with_weld_policy(
        verts: Vec<[f64; 3]>,
        tris: Vec<[u32; 3]>,
        weld_policy: WeldPolicy,
    ) -> Result<Self, MeshError> {
        validate_weld_policy(weld_policy)?;
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
        Ok(Self {
            verts,
            tris,
            weld_policy,
        })
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
    #[error("epsilon weld policy must be finite and greater than zero, got {epsilon}")]
    InvalidWeldEpsilon { epsilon: f64 },
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

fn validate_weld_policy(policy: WeldPolicy) -> Result<(), MeshError> {
    if let WeldPolicy::Epsilon(epsilon) = policy {
        if !epsilon.is_finite() || epsilon <= 0.0 {
            return Err(MeshError::InvalidWeldEpsilon { epsilon });
        }
    }
    Ok(())
}
