//! Batch-shaped, f64 boolean-solid operations.
//!
//! M0 intentionally exposes only a narrow spike behind [`SolidBoolean`]. The
//! full artifact audit belongs to M4; tests here independently inspect the
//! returned triangle mesh to prove the selected backend is viable.

mod ffi;

use std::fmt;

use thiserror::Error;

/// A validated, owned f64 triangle mesh with u64 indices.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    vertices: Vec<[f64; 3]>,
    triangles: Vec<[u64; 3]>,
}

impl Mesh {
    pub fn new(
        vertices: Vec<[f64; 3]>,
        triangles: Vec<[u64; 3]>,
    ) -> Result<Self, MeshValidationError> {
        validate_mesh(&vertices, &triangles)?;
        Ok(Self {
            vertices,
            triangles,
        })
    }

    pub fn vertices(&self) -> &[[f64; 3]] {
        &self.vertices
    }

    pub fn triangles(&self) -> &[[u64; 3]] {
        &self.triangles
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum MeshValidationError {
    #[error("mesh has no vertices")]
    NoVertices,
    #[error("mesh has no triangles")]
    NoTriangles,
    #[error("vertex {vertex} coordinate {axis} is not finite")]
    NonFiniteVertex { vertex: usize, axis: usize },
    #[error("triangle {triangle} index {index} is outside the {vertex_count}-vertex mesh")]
    IndexOutOfBounds {
        triangle: usize,
        index: u64,
        vertex_count: usize,
    },
    #[error("triangle {triangle} repeats a vertex index")]
    DegenerateTriangle { triangle: usize },
    #[error("flattened vertex buffer has {scalar_count} scalars, not a multiple of three")]
    VertexBufferLength { scalar_count: usize },
    #[error("flattened triangle buffer has {index_count} indices, not a multiple of three")]
    TriangleBufferLength { index_count: usize },
}

#[derive(Debug, Error)]
pub enum SolidError {
    #[error("union requires at least one mesh")]
    EmptyUnion,
    #[error("difference requires a minuend and at least one subtrahend")]
    DifferenceRequiresSubtrahend,
    #[error("flattened mesh batch exceeds addressable storage")]
    BatchTooLarge,
    #[error("Manifold backend failed: {0}")]
    Backend(String),
    #[error("Manifold returned an invalid mesh: {0}")]
    InvalidOutput(#[source] MeshValidationError),
}

/// Boolean operations are batch-shaped: each operation crosses the native
/// boundary once for the complete input slice.
pub trait SolidBoolean {
    fn union(&self, meshes: &[Mesh]) -> Result<Mesh, SolidError>;

    /// Subtracts the batch-union of `meshes[1..]` from `meshes[0]` in one
    /// native call.
    fn difference(&self, meshes: &[Mesh]) -> Result<Mesh, SolidError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ManifoldBackend;

impl SolidBoolean for ManifoldBackend {
    fn union(&self, meshes: &[Mesh]) -> Result<Mesh, SolidError> {
        if meshes.is_empty() {
            return Err(SolidError::EmptyUnion);
        }
        run_batch(meshes, ffi::manifold_union_batch)
    }

    fn difference(&self, meshes: &[Mesh]) -> Result<Mesh, SolidError> {
        if meshes.len() < 2 {
            return Err(SolidError::DifferenceRequiresSubtrahend);
        }
        run_batch(meshes, ffi::manifold_difference_batch)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackendConfiguration {
    pub version: (u32, u32, u32),
    pub parallel_mode: i32,
    pub scalar_bytes: u32,
    pub index_bytes: u32,
    pub cross_section_enabled: bool,
}

pub fn backend_configuration() -> BackendConfiguration {
    let config = ffi::manifold_config();
    BackendConfiguration {
        version: (
            config.version_major,
            config.version_minor,
            config.version_patch,
        ),
        parallel_mode: config.parallel_mode,
        scalar_bytes: config.scalar_bytes,
        index_bytes: config.index_bytes,
        cross_section_enabled: config.cross_section_enabled,
    }
}

type NativeOperation = fn(&[f64], &[u64], &[u64], &[u64]) -> Result<ffi::FfiMesh, cxx::Exception>;

fn run_batch(meshes: &[Mesh], operation: NativeOperation) -> Result<Mesh, SolidError> {
    let batch = FlatBatch::gather(meshes)?;
    let output = operation(
        &batch.vertices,
        &batch.triangles,
        &batch.vertex_offsets,
        &batch.triangle_offsets,
    )
    .map_err(|error| SolidError::Backend(error.to_string()))?;
    unflatten(output).map_err(SolidError::InvalidOutput)
}

struct FlatBatch {
    vertices: Vec<f64>,
    triangles: Vec<u64>,
    vertex_offsets: Vec<u64>,
    triangle_offsets: Vec<u64>,
}

impl FlatBatch {
    fn gather(meshes: &[Mesh]) -> Result<Self, SolidError> {
        let vertex_scalars = checked_scalar_count(meshes.iter().map(|mesh| mesh.vertices.len()))?;
        let triangle_indices =
            checked_scalar_count(meshes.iter().map(|mesh| mesh.triangles.len()))?;
        let offset_count = meshes
            .len()
            .checked_add(1)
            .ok_or(SolidError::BatchTooLarge)?;

        let mut batch = Self {
            vertices: Vec::with_capacity(vertex_scalars),
            triangles: Vec::with_capacity(triangle_indices),
            vertex_offsets: Vec::with_capacity(offset_count),
            triangle_offsets: Vec::with_capacity(offset_count),
        };
        batch.vertex_offsets.push(0);
        batch.triangle_offsets.push(0);

        for mesh in meshes {
            batch.vertices.extend(
                mesh.vertices
                    .iter()
                    .flat_map(|vertex| vertex.iter())
                    .copied(),
            );
            batch.triangles.extend(
                mesh.triangles
                    .iter()
                    .flat_map(|triangle| triangle.iter())
                    .copied(),
            );
            batch
                .vertex_offsets
                .push(u64::try_from(batch.vertices.len()).map_err(|_| SolidError::BatchTooLarge)?);
            batch
                .triangle_offsets
                .push(u64::try_from(batch.triangles.len()).map_err(|_| SolidError::BatchTooLarge)?);
        }
        Ok(batch)
    }
}

fn checked_scalar_count(mut lengths: impl Iterator<Item = usize>) -> Result<usize, SolidError> {
    lengths.try_fold(0usize, |total, length| {
        length
            .checked_mul(3)
            .and_then(|scalars| total.checked_add(scalars))
            .ok_or(SolidError::BatchTooLarge)
    })
}

fn unflatten(output: ffi::FfiMesh) -> Result<Mesh, MeshValidationError> {
    if !output.vertices.len().is_multiple_of(3) {
        return Err(MeshValidationError::VertexBufferLength {
            scalar_count: output.vertices.len(),
        });
    }
    if !output.triangles.len().is_multiple_of(3) {
        return Err(MeshValidationError::TriangleBufferLength {
            index_count: output.triangles.len(),
        });
    }
    let vertices = output
        .vertices
        .chunks_exact(3)
        .map(|values| [values[0], values[1], values[2]])
        .collect();
    let triangles = output
        .triangles
        .chunks_exact(3)
        .map(|values| [values[0], values[1], values[2]])
        .collect();
    Mesh::new(vertices, triangles)
}

fn validate_mesh(vertices: &[[f64; 3]], triangles: &[[u64; 3]]) -> Result<(), MeshValidationError> {
    if vertices.is_empty() {
        return Err(MeshValidationError::NoVertices);
    }
    if triangles.is_empty() {
        return Err(MeshValidationError::NoTriangles);
    }
    for (vertex_index, vertex) in vertices.iter().enumerate() {
        for (axis, coordinate) in vertex.iter().enumerate() {
            if !coordinate.is_finite() {
                return Err(MeshValidationError::NonFiniteVertex {
                    vertex: vertex_index,
                    axis,
                });
            }
        }
    }
    for (triangle_index, triangle) in triangles.iter().enumerate() {
        if triangle[0] == triangle[1] || triangle[1] == triangle[2] || triangle[2] == triangle[0] {
            return Err(MeshValidationError::DegenerateTriangle {
                triangle: triangle_index,
            });
        }
        for index in triangle {
            if usize::try_from(*index).map_or(true, |value| value >= vertices.len()) {
                return Err(MeshValidationError::IndexOutOfBounds {
                    triangle: triangle_index,
                    index: *index,
                    vertex_count: vertices.len(),
                });
            }
        }
    }
    Ok(())
}

impl fmt::Display for BackendConfiguration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Manifold {}.{}.{} (parallel={}, f{} / u{}, cross-section={})",
            self.version.0,
            self.version.1,
            self.version.2,
            self.parallel_mode,
            self.scalar_bytes * 8,
            self.index_bytes * 8,
            self.cross_section_enabled
        )
    }
}
