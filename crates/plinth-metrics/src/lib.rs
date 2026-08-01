//! Independent, deliberately brute-force geometry and topology audit primitives.
//!
//! This crate is an auditor, not a producer: it intentionally has no dependency
//! on `geom_kernel`, `plinth-mesh`, or `plinth-verify`.  Its public query APIs
//! are slice-in/slice-out so callers gather a full batch before asking it to
//! perform one audit pass.
#![forbid(unsafe_code)]

mod geometry;
mod read;
mod topology;

pub use geometry::{
    closest_points, contains_points, ray_intersections, triangle_intersections, ClosestPoint, Ray,
    RayHit,
};
pub use read::{
    load_artifacts, ArtifactDisposition, ArtifactLoad, ArtifactRequest, ArtifactRequestError,
    DiskArtifact, DiskModel, DiskReadError, DiskSupport, ProducerFailure, ProducerStage,
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
            if a == b || b == c || c == a {
                return Err(MetricsError::DegenerateTriangle { triangle });
            }
            match triangle_has_nonzero_area(vertices[a], vertices[b], vertices[c]) {
                Ok(true) => {}
                Ok(false) => return Err(MetricsError::DegenerateTriangle { triangle }),
                Err(()) => {
                    return Err(MetricsError::ArithmeticOverflow {
                        triangle,
                        primitive: GeometryPrimitive::TriangleArea,
                    })
                }
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
    #[error("triangle {triangle} overflows while evaluating {primitive:?}")]
    ArithmeticOverflow {
        triangle: usize,
        primitive: GeometryPrimitive,
    },
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

/// A finite geometry primitive whose intermediate arithmetic may be unrepresentable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeometryPrimitive {
    TriangleArea,
    RayTriangle,
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

/// Returns whether a triangle has nonzero area without squaring its normal.
///
/// Scaling the finite edge vectors before their cross product avoids the former
/// underflow for tiny valid triangles and overflow for large valid triangles.
/// A coordinate subtraction that itself overflows is not an honest geometric
/// answer, so callers turn it into a typed semantic input error.
pub(crate) fn triangle_has_nonzero_area(a: Vec3, b: Vec3, c: Vec3) -> Result<bool, ()> {
    let ab = checked_sub(a, b)?;
    let ac = checked_sub(c, a)?;
    let scale = ab.into_iter().chain(ac).map(f64::abs).fold(0.0, f64::max);
    if scale == 0.0 {
        return Ok(false);
    }
    let normal = cross(
        [ab[0] / scale, ab[1] / scale, ab[2] / scale],
        [ac[0] / scale, ac[1] / scale, ac[2] / scale],
    );
    if normal.iter().any(|value| !value.is_finite()) {
        return Err(());
    }
    Ok(normal.iter().any(|value| *value != 0.0))
}

fn checked_sub(a: Vec3, b: Vec3) -> Result<Vec3, ()> {
    let value = sub(a, b);
    if value.iter().all(|coordinate| coordinate.is_finite()) {
        Ok(value)
    } else {
        Err(())
    }
}
