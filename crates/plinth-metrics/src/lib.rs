//! Independent, deliberately brute-force geometry and topology audit primitives.
//!
//! This crate is an auditor, not a producer: it intentionally has no dependency
//! on `geom_kernel`, `plinth-mesh`, or `plinth-verify`.  Its public query APIs
//! are slice-in/slice-out so callers gather a full batch before asking it to
//! perform one audit pass.
#![forbid(unsafe_code)]

mod geometry;
mod topology;

pub use geometry::{
    closest_points, contains_points, ray_intersections, triangle_intersections, ClosestPoint, Ray,
    RayHit,
};
pub use topology::{
    analyze_topology, boundary_edges, signed_shell_volumes, Edge, EdgeIncidence, EdgeUse, Shell,
    TopologyAnalysis,
};

use thiserror::Error;

/// An f64 Cartesian point or vector.  Values are always validated at the API boundary.
pub type Vec3 = [f64; 3];

/// A triangle references three entries in [`Mesh::vertices`].
pub type Triangle = [usize; 3];

/// A finite, non-degenerate triangle soup retained without coordinate rewriting.
///
/// In particular, `-0.0` is not canonicalized: its IEEE representation is input
/// data and generic audit inputs must retain it.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    vertices: Vec<Vec3>,
    triangles: Vec<Triangle>,
}

impl Mesh {
    /// Builds a validated soup.  Empty and degenerate soups cannot support the
    /// audit primitives and are rejected as semantic input errors.
    pub fn new(vertices: Vec<Vec3>, triangles: Vec<Triangle>) -> Result<Self, MetricsError> {
        if triangles.is_empty() {
            return Err(MetricsError::EmptyMesh);
        }
        for (vertex, value) in vertices.iter().enumerate() {
            validate_vec3(*value, CoordinateKind::Vertex { vertex })?;
        }
        for (triangle, indices) in triangles.iter().enumerate() {
            for (corner, &vertex) in indices.iter().enumerate() {
                if vertex >= vertices.len() {
                    return Err(MetricsError::IndexOutOfBounds {
                        triangle,
                        corner,
                        vertex,
                        vertex_count: vertices.len(),
                    });
                }
            }
            let [a, b, c] = *indices;
            if a == b
                || b == c
                || c == a
                || triangle_double_area(vertices[a], vertices[b], vertices[c]) == 0.0
            {
                return Err(MetricsError::DegenerateTriangle { triangle });
            }
        }
        Ok(Self {
            vertices,
            triangles,
        })
    }

    #[must_use]
    pub fn vertices(&self) -> &[Vec3] {
        &self.vertices
    }

    #[must_use]
    pub fn triangles(&self) -> &[Triangle] {
        &self.triangles
    }

    pub(crate) fn triangle_points(&self, index: usize) -> (Vec3, Vec3, Vec3) {
        let [a, b, c] = self.triangles[index];
        (self.vertices[a], self.vertices[b], self.vertices[c])
    }
}

/// The validated input category that contained a non-finite coordinate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoordinateKind {
    Vertex { vertex: usize },
    Point { point: usize },
    RayOrigin { ray: usize },
    RayDirection { ray: usize },
}

/// Typed input and numerical failures for independent audit operations.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum MetricsError {
    #[error("a mesh must contain at least one triangle")]
    EmptyMesh,
    #[error("triangle {triangle} corner {corner} references vertex {vertex}, but only {vertex_count} vertices exist")]
    IndexOutOfBounds {
        triangle: usize,
        corner: usize,
        vertex: usize,
        vertex_count: usize,
    },
    #[error("triangle {triangle} has zero area or repeated vertices")]
    DegenerateTriangle { triangle: usize },
    #[error("{kind:?} coordinate {component} is not finite")]
    NonFiniteCoordinate {
        kind: CoordinateKind,
        component: usize,
    },
    #[error("ray {ray} has a zero-length direction")]
    ZeroRayDirection { ray: usize },
    #[error("triangle query {query} {side:?} index {triangle} is outside the mesh's {triangle_count} triangles")]
    QueryTriangleIndexOutOfBounds {
        query: usize,
        side: TriangleSide,
        triangle: usize,
        triangle_count: usize,
    },
}

/// Which side of a triangle-pair query addressed an invalid triangle index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TriangleSide {
    Left,
    Right,
}

pub(crate) fn validate_vec3(value: Vec3, kind: CoordinateKind) -> Result<(), MetricsError> {
    for (component, coordinate) in value.iter().enumerate() {
        if !coordinate.is_finite() {
            return Err(MetricsError::NonFiniteCoordinate { kind, component });
        }
    }
    Ok(())
}

pub(crate) fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn scale(a: Vec3, factor: f64) -> Vec3 {
    [a[0] * factor, a[1] * factor, a[2] * factor]
}

pub(crate) fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn norm_squared(value: Vec3) -> f64 {
    dot(value, value)
}

pub(crate) fn triangle_double_area(a: Vec3, b: Vec3, c: Vec3) -> f64 {
    norm_squared(cross(sub(b, a), sub(c, a)))
}
