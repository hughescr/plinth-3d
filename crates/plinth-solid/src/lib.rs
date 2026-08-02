//! Batch-shaped, f64 boolean-solid operations.
//!
//! M0 intentionally exposes only a narrow spike behind [`SolidBoolean`]. The
//! full artifact audit belongs to M4; tests here independently inspect the
//! returned triangle mesh to prove the selected backend is viable. The M0
//! [`Mesh`] represents non-empty solids only, so a valid empty boolean is
//! reported as [`SolidError::EmptyResult`]. Revisit that policy with M2's mesh
//! data model instead of speculating about its public result shape here.

mod ffi;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManifoldStatus {
    NoError,
    NonFiniteVertex,
    NotManifold,
    VertexOutOfBounds,
    PropertiesWrongLength,
    MissingPositionProperties,
    MergeVectorsDifferentLengths,
    MergeIndexOutOfBounds,
    TransformWrongLength,
    RunIndexWrongLength,
    FaceIdWrongLength,
    InvalidConstruction,
    ResultTooLarge,
    InvalidTangents,
    Cancelled,
    Unknown(i32),
}

impl ManifoldStatus {
    fn from_code(code: i32) -> Self {
        match code {
            0 => Self::NoError,
            1 => Self::NonFiniteVertex,
            2 => Self::NotManifold,
            3 => Self::VertexOutOfBounds,
            4 => Self::PropertiesWrongLength,
            5 => Self::MissingPositionProperties,
            6 => Self::MergeVectorsDifferentLengths,
            7 => Self::MergeIndexOutOfBounds,
            8 => Self::TransformWrongLength,
            9 => Self::RunIndexWrongLength,
            10 => Self::FaceIdWrongLength,
            11 => Self::InvalidConstruction,
            12 => Self::ResultTooLarge,
            13 => Self::InvalidTangents,
            14 => Self::Cancelled,
            unknown => Self::Unknown(unknown),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeValidationError {
    OffsetArrayShape,
    OffsetSpan,
    OffsetOrderOrAlignment,
    DifferenceArity,
    OutputPropertyStride,
    OutputCountMismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManifoldInputLimit {
    VertexCount,
    TriangleCount,
    TriangleIndex {
        triangle: usize,
        corner: usize,
        index: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeExceptionKind {
    InvalidArgument,
    Overflow,
    Allocation,
    Standard,
    NonStandard,
}

/// Exact primitive counts and logical payload bytes for one complete boolean
/// batch. This is derived before flattening the meshes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BooleanInputEstimate {
    pub vertex_scalars: u64,
    pub triangle_indices: u64,
    pub vertex_offset_units: u64,
    pub triangle_offset_units: u64,
    pub operand_primitive_units: u64,
    pub logical_marshaled_input_bytes: u64,
}

impl BooleanInputEstimate {
    pub fn for_meshes(meshes: &[Mesh]) -> Result<Self, SolidError> {
        let mesh_count = u64::try_from(meshes.len()).map_err(|_| SolidError::BatchTooLarge)?;
        let vertex_scalars =
            checked_u64_scalar_count(meshes.iter().map(|mesh| mesh.vertices.len()))?;
        let triangle_indices =
            checked_u64_scalar_count(meshes.iter().map(|mesh| mesh.triangles.len()))?;
        calculate_input_estimate(mesh_count, vertex_scalars, triangle_indices)
    }
}

fn calculate_input_estimate(
    mesh_count: u64,
    vertex_scalars: u64,
    triangle_indices: u64,
) -> Result<BooleanInputEstimate, SolidError> {
    let offset_units = mesh_count.checked_add(1).ok_or(SolidError::BatchTooLarge)?;
    let operand_primitive_units = vertex_scalars
        .checked_add(triangle_indices)
        .and_then(|total| total.checked_add(offset_units))
        .and_then(|total| total.checked_add(offset_units))
        .ok_or(SolidError::BatchTooLarge)?;
    // The CXX ABI contains four primitive slices, whose element types are
    // each eight bytes. This intentionally excludes Manifold private heap.
    let logical_marshaled_input_bytes = operand_primitive_units
        .checked_mul(8)
        .ok_or(SolidError::BatchTooLarge)?;
    Ok(BooleanInputEstimate {
        vertex_scalars,
        triangle_indices,
        vertex_offset_units: offset_units,
        triangle_offset_units: offset_units,
        operand_primitive_units,
        logical_marshaled_input_bytes,
    })
}

/// Limits for the public boolean ABI, not estimates of Manifold internals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BooleanLimits {
    pub max_input_primitive_units: u64,
    pub max_logical_marshaled_input_bytes: u64,
    pub max_output_vertices: u64,
    pub max_output_triangles: u64,
}

impl BooleanLimits {
    pub const UNLIMITED: Self = Self {
        max_input_primitive_units: u64::MAX,
        max_logical_marshaled_input_bytes: u64::MAX,
        max_output_vertices: u64::MAX,
        max_output_triangles: u64::MAX,
    };
}

impl Default for BooleanLimits {
    fn default() -> Self {
        Self::UNLIMITED
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanInputLimit {
    PrimitiveUnits,
    LogicalMarshalledInputBytes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanOutputLimit {
    Vertices,
    Triangles,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SolidError {
    #[error("union requires at least one mesh")]
    EmptyUnion,
    #[error("difference requires a minuend and at least one subtrahend")]
    DifferenceRequiresSubtrahend,
    #[error("flattened mesh batch exceeds addressable storage")]
    BatchTooLarge,
    #[error("boolean input exceeds {limit:?}: actual {actual}, limit {maximum}")]
    InputLimitExceeded {
        limit: BooleanInputLimit,
        actual: u64,
        maximum: u64,
    },
    #[error("boolean output exceeds {limit:?}: actual {actual}, limit {maximum}")]
    OutputLimitExceeded {
        limit: BooleanOutputLimit,
        actual: u64,
        maximum: u64,
    },
    #[error("mesh {mesh_index} exceeds Manifold's u32 import domain: {limit:?}")]
    ManifoldInputLimitExceeded {
        mesh_index: u64,
        limit: ManifoldInputLimit,
    },
    #[error("native mesh-batch validation failed: {0:?}")]
    NativeValidation(NativeValidationError),
    #[error("native mesh output exceeded addressable storage")]
    NativeOverflow,
    #[error("Manifold rejected input mesh {mesh_index}: {status:?}")]
    ManifoldInput {
        mesh_index: u64,
        status: ManifoldStatus,
    },
    #[error("Manifold boolean result failed: {status:?}")]
    ManifoldResult { status: ManifoldStatus },
    #[error("boolean operation produced an empty solid")]
    EmptyResult,
    #[error("unexpected native exception: {0:?}")]
    UnexpectedNativeException(NativeExceptionKind),
    #[error("native response violated its typed protocol")]
    InvalidNativeResponse,
    #[error("Manifold returned an invalid mesh: {0}")]
    InvalidOutput(#[source] MeshValidationError),
}

/// Boolean operations are batch-shaped: each operation crosses the native
/// boundary once for the complete input slice.
pub trait SolidBoolean {
    fn union(&self, meshes: &[Mesh]) -> Result<Mesh, SolidError>;

    /// Compatibility path that subtracts the batch-union of `meshes[1..]`
    /// from `meshes[0]` in one native call without caller-selected caps.
    /// Resource-bounded callers should use [`ManifoldBackend::difference_limited`].
    fn difference(&self, meshes: &[Mesh]) -> Result<Mesh, SolidError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ManifoldBackend;

impl ManifoldBackend {
    /// Runs one batch-shaped difference while enforcing explicit ABI limits.
    /// Input limits are checked before gathering or crossing the native boundary.
    pub fn difference_limited(
        &self,
        meshes: &[Mesh],
        limits: BooleanLimits,
    ) -> Result<Mesh, SolidError> {
        if meshes.len() < 2 {
            return Err(SolidError::DifferenceRequiresSubtrahend);
        }
        run_batch_limited(meshes, limits, ffi::manifold_difference_batch)
    }
}

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

type NativeOperation = fn(&[f64], &[u64], &[u64], &[u64], u64, u64) -> ffi::FfiResponse;

fn run_batch(meshes: &[Mesh], operation: NativeOperation) -> Result<Mesh, SolidError> {
    run_batch_limited(meshes, BooleanLimits::UNLIMITED, operation)
}

fn run_batch_limited(
    meshes: &[Mesh],
    limits: BooleanLimits,
    operation: NativeOperation,
) -> Result<Mesh, SolidError> {
    let estimate = BooleanInputEstimate::for_meshes(meshes)?;
    validate_input_limits(estimate, limits)?;
    validate_manifold_input_representability(meshes)?;
    let batch = FlatBatch::gather(meshes)?;
    let response = operation(
        &batch.vertices,
        &batch.triangles,
        &batch.vertex_offsets,
        &batch.triangle_offsets,
        limits.max_output_vertices,
        limits.max_output_triangles,
    );
    map_native_response_with_limits(response, limits)
}

fn validate_manifold_input_representability(meshes: &[Mesh]) -> Result<(), SolidError> {
    for (mesh_index, mesh) in meshes.iter().enumerate() {
        let mesh_index = u64::try_from(mesh_index).map_err(|_| SolidError::BatchTooLarge)?;
        validate_manifold_input_counts(mesh_index, mesh.vertices.len(), mesh.triangles.len())?;
        for (triangle, indices) in mesh.triangles.iter().enumerate() {
            for (corner, index) in indices.iter().copied().enumerate() {
                validate_manifold_triangle_index(mesh_index, triangle, corner, index)?;
            }
        }
    }
    Ok(())
}

fn validate_manifold_input_counts(
    mesh_index: u64,
    vertex_count: usize,
    triangle_count: usize,
) -> Result<(), SolidError> {
    if u64::try_from(vertex_count).map_or(true, |count| count > u64::from(u32::MAX)) {
        return Err(SolidError::ManifoldInputLimitExceeded {
            mesh_index,
            limit: ManifoldInputLimit::VertexCount,
        });
    }
    if u64::try_from(triangle_count).map_or(true, |count| count > u64::from(u32::MAX)) {
        return Err(SolidError::ManifoldInputLimitExceeded {
            mesh_index,
            limit: ManifoldInputLimit::TriangleCount,
        });
    }
    Ok(())
}

fn validate_manifold_triangle_index(
    mesh_index: u64,
    triangle: usize,
    corner: usize,
    index: u64,
) -> Result<(), SolidError> {
    if index > u64::from(u32::MAX) {
        return Err(SolidError::ManifoldInputLimitExceeded {
            mesh_index,
            limit: ManifoldInputLimit::TriangleIndex {
                triangle,
                corner,
                index,
            },
        });
    }
    Ok(())
}

fn validate_input_limits(
    estimate: BooleanInputEstimate,
    limits: BooleanLimits,
) -> Result<(), SolidError> {
    if estimate.operand_primitive_units > limits.max_input_primitive_units {
        return Err(SolidError::InputLimitExceeded {
            limit: BooleanInputLimit::PrimitiveUnits,
            actual: estimate.operand_primitive_units,
            maximum: limits.max_input_primitive_units,
        });
    }
    if estimate.logical_marshaled_input_bytes > limits.max_logical_marshaled_input_bytes {
        return Err(SolidError::InputLimitExceeded {
            limit: BooleanInputLimit::LogicalMarshalledInputBytes,
            actual: estimate.logical_marshaled_input_bytes,
            maximum: limits.max_logical_marshaled_input_bytes,
        });
    }
    Ok(())
}

#[cfg(test)]
fn map_native_response(response: ffi::FfiResponse) -> Result<Mesh, SolidError> {
    map_native_response_with_limits(response, BooleanLimits::UNLIMITED)
}

fn map_native_response_with_limits(
    response: ffi::FfiResponse,
    limits: BooleanLimits,
) -> Result<Mesh, SolidError> {
    use ffi::FfiErrorCode as Code;

    validate_native_envelope(&response)?;
    match response.code {
        Code::Success => unflatten(response.mesh).map_err(SolidError::InvalidOutput),
        Code::OffsetArrayShape => Err(SolidError::NativeValidation(
            NativeValidationError::OffsetArrayShape,
        )),
        Code::OffsetSpan => Err(SolidError::NativeValidation(
            NativeValidationError::OffsetSpan,
        )),
        Code::OffsetOrderOrAlignment => Err(SolidError::NativeValidation(
            NativeValidationError::OffsetOrderOrAlignment,
        )),
        Code::DifferenceArity => Err(SolidError::NativeValidation(
            NativeValidationError::DifferenceArity,
        )),
        Code::OutputPropertyStride => Err(SolidError::NativeValidation(
            NativeValidationError::OutputPropertyStride,
        )),
        Code::OutputCountMismatch => Err(SolidError::NativeValidation(
            NativeValidationError::OutputCountMismatch,
        )),
        Code::OutputOverflow => Err(SolidError::NativeOverflow),
        Code::OutputLimitExceeded => {
            let (limit, maximum) = match response.output_limit {
                ffi::FfiOutputLimit::Vertices => {
                    (BooleanOutputLimit::Vertices, limits.max_output_vertices)
                }
                ffi::FfiOutputLimit::Triangles => {
                    (BooleanOutputLimit::Triangles, limits.max_output_triangles)
                }
                _ => return Err(SolidError::InvalidNativeResponse),
            };
            if response.output_count <= maximum {
                return Err(SolidError::InvalidNativeResponse);
            }
            Err(SolidError::OutputLimitExceeded {
                limit,
                actual: response.output_count,
                maximum,
            })
        }
        Code::InputManifoldStatus => Err(SolidError::ManifoldInput {
            mesh_index: response
                .has_mesh_index
                .then_some(response.mesh_index)
                .ok_or(SolidError::InvalidNativeResponse)?,
            status: manifold_error_status(response.manifold_status)?,
        }),
        Code::ResultManifoldStatus => Err(SolidError::ManifoldResult {
            status: manifold_error_status(response.manifold_status)?,
        }),
        Code::EmptyResult => Err(SolidError::EmptyResult),
        Code::UnexpectedInvalidArgument => Err(SolidError::UnexpectedNativeException(
            NativeExceptionKind::InvalidArgument,
        )),
        Code::UnexpectedOverflow => Err(SolidError::UnexpectedNativeException(
            NativeExceptionKind::Overflow,
        )),
        Code::UnexpectedAllocation => Err(SolidError::UnexpectedNativeException(
            NativeExceptionKind::Allocation,
        )),
        Code::UnexpectedStandardException => Err(SolidError::UnexpectedNativeException(
            NativeExceptionKind::Standard,
        )),
        Code::UnexpectedNonStandardException => Err(SolidError::UnexpectedNativeException(
            NativeExceptionKind::NonStandard,
        )),
        _ => Err(SolidError::InvalidNativeResponse),
    }
}

#[derive(Clone, Copy)]
enum StatusField {
    Zero,
    NonZero,
}

#[derive(Clone, Copy)]
enum MeshIndexField {
    Absent,
    Required,
}

#[derive(Clone, Copy)]
enum PayloadField {
    Empty,
    Required,
}

#[derive(Clone, Copy)]
struct ResponseSchema {
    status: StatusField,
    mesh_index: MeshIndexField,
    payload: PayloadField,
}

fn response_schema(code: ffi::FfiErrorCode) -> Option<ResponseSchema> {
    use ffi::FfiErrorCode as Code;

    let schema = match code {
        Code::Success => ResponseSchema {
            status: StatusField::Zero,
            mesh_index: MeshIndexField::Absent,
            payload: PayloadField::Required,
        },
        Code::InputManifoldStatus => ResponseSchema {
            status: StatusField::NonZero,
            mesh_index: MeshIndexField::Required,
            payload: PayloadField::Empty,
        },
        Code::ResultManifoldStatus => ResponseSchema {
            status: StatusField::NonZero,
            mesh_index: MeshIndexField::Absent,
            payload: PayloadField::Empty,
        },
        Code::OffsetArrayShape
        | Code::OffsetSpan
        | Code::OffsetOrderOrAlignment
        | Code::DifferenceArity
        | Code::OutputPropertyStride
        | Code::OutputCountMismatch
        | Code::OutputOverflow
        | Code::OutputLimitExceeded
        | Code::EmptyResult
        | Code::UnexpectedInvalidArgument
        | Code::UnexpectedOverflow
        | Code::UnexpectedAllocation
        | Code::UnexpectedStandardException
        | Code::UnexpectedNonStandardException => ResponseSchema {
            status: StatusField::Zero,
            mesh_index: MeshIndexField::Absent,
            payload: PayloadField::Empty,
        },
        _ => return None,
    };
    Some(schema)
}

fn validate_native_envelope(response: &ffi::FfiResponse) -> Result<(), SolidError> {
    let schema = response_schema(response.code).ok_or(SolidError::InvalidNativeResponse)?;
    let status_valid = match schema.status {
        StatusField::Zero => response.manifold_status == 0,
        StatusField::NonZero => response.manifold_status != 0,
    };
    let mesh_index_valid = match schema.mesh_index {
        MeshIndexField::Absent => !response.has_mesh_index && response.mesh_index == 0,
        MeshIndexField::Required => response.has_mesh_index,
    };
    let payload_valid = match schema.payload {
        PayloadField::Empty => {
            response.mesh.vertices.is_empty() && response.mesh.triangles.is_empty()
        }
        PayloadField::Required => {
            !response.mesh.vertices.is_empty() && !response.mesh.triangles.is_empty()
        }
    };
    let output_limit_valid = match response.code {
        ffi::FfiErrorCode::OutputLimitExceeded => response.output_count > 0,
        _ => response.output_limit == ffi::FfiOutputLimit::Vertices && response.output_count == 0,
    };
    if status_valid && mesh_index_valid && payload_valid && output_limit_valid {
        Ok(())
    } else {
        Err(SolidError::InvalidNativeResponse)
    }
}

fn manifold_error_status(code: i32) -> Result<ManifoldStatus, SolidError> {
    match ManifoldStatus::from_code(code) {
        ManifoldStatus::NoError => Err(SolidError::InvalidNativeResponse),
        status => Ok(status),
    }
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

fn checked_u64_scalar_count(mut lengths: impl Iterator<Item = usize>) -> Result<u64, SolidError> {
    lengths.try_fold(0u64, |total, length| {
        u64::try_from(length)
            .ok()
            .and_then(|count| count.checked_mul(3))
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{
        calculate_input_estimate, ffi, map_native_response, run_batch_limited,
        validate_input_limits, validate_manifold_input_counts, validate_manifold_triangle_index,
        validate_native_envelope, BooleanInputEstimate, BooleanInputLimit, BooleanLimits,
        BooleanOutputLimit, ManifoldBackend, ManifoldInputLimit, ManifoldStatus, Mesh,
        NativeExceptionKind, NativeValidationError, SolidBoolean, SolidError,
    };

    static NATIVE_CALLS: AtomicUsize = AtomicUsize::new(0);

    fn counted_operation(
        _: &[f64],
        _: &[u64],
        _: &[u64],
        _: &[u64],
        _: u64,
        _: u64,
    ) -> ffi::FfiResponse {
        NATIVE_CALLS.fetch_add(1, Ordering::SeqCst);
        unreachable!("an input-limited batch must not cross FFI")
    }

    fn tetrahedron() -> Mesh {
        Mesh::new(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]],
        )
        .expect("fixture is structurally valid")
    }

    fn cuboid(min: [f64; 3], max: [f64; 3]) -> Mesh {
        Mesh::new(
            vec![
                [min[0], min[1], min[2]],
                [max[0], min[1], min[2]],
                [max[0], max[1], min[2]],
                [min[0], max[1], min[2]],
                [min[0], min[1], max[2]],
                [max[0], min[1], max[2]],
                [max[0], max[1], max[2]],
                [min[0], max[1], max[2]],
            ],
            vec![
                [0, 2, 1],
                [0, 3, 2],
                [4, 5, 6],
                [4, 6, 7],
                [0, 1, 5],
                [0, 5, 4],
                [1, 2, 6],
                [1, 6, 5],
                [2, 3, 7],
                [2, 7, 6],
                [3, 0, 4],
                [3, 4, 7],
            ],
        )
        .expect("fixture is structurally valid")
    }

    #[derive(Clone, Copy)]
    enum TestPayload {
        Empty,
        Present,
    }

    #[derive(Clone, Copy)]
    struct EnvelopeCase {
        code: ffi::FfiErrorCode,
        status: i32,
        mesh_index: Option<u64>,
        payload: TestPayload,
    }

    impl EnvelopeCase {
        const fn new(
            code: ffi::FfiErrorCode,
            status: i32,
            mesh_index: Option<u64>,
            payload: TestPayload,
        ) -> Self {
            Self {
                code,
                status,
                mesh_index,
                payload,
            }
        }
    }

    use ffi::FfiErrorCode as Code;

    const ENVELOPES: [EnvelopeCase; 17] = [
        EnvelopeCase::new(Code::Success, 0, None, TestPayload::Present),
        EnvelopeCase::new(Code::OffsetArrayShape, 0, None, TestPayload::Empty),
        EnvelopeCase::new(Code::OffsetSpan, 0, None, TestPayload::Empty),
        EnvelopeCase::new(Code::OffsetOrderOrAlignment, 0, None, TestPayload::Empty),
        EnvelopeCase::new(Code::DifferenceArity, 0, None, TestPayload::Empty),
        EnvelopeCase::new(Code::OutputPropertyStride, 0, None, TestPayload::Empty),
        EnvelopeCase::new(Code::OutputOverflow, 0, None, TestPayload::Empty),
        EnvelopeCase::new(Code::OutputLimitExceeded, 0, None, TestPayload::Empty),
        EnvelopeCase::new(Code::OutputCountMismatch, 0, None, TestPayload::Empty),
        EnvelopeCase::new(Code::InputManifoldStatus, 2, Some(7), TestPayload::Empty),
        EnvelopeCase::new(Code::ResultManifoldStatus, 2, None, TestPayload::Empty),
        EnvelopeCase::new(Code::EmptyResult, 0, None, TestPayload::Empty),
        EnvelopeCase::new(Code::UnexpectedInvalidArgument, 0, None, TestPayload::Empty),
        EnvelopeCase::new(Code::UnexpectedOverflow, 0, None, TestPayload::Empty),
        EnvelopeCase::new(Code::UnexpectedAllocation, 0, None, TestPayload::Empty),
        EnvelopeCase::new(
            Code::UnexpectedStandardException,
            0,
            None,
            TestPayload::Empty,
        ),
        EnvelopeCase::new(
            Code::UnexpectedNonStandardException,
            0,
            None,
            TestPayload::Empty,
        ),
    ];

    fn response(case: EnvelopeCase) -> ffi::FfiResponse {
        let mesh = match case.payload {
            TestPayload::Empty => ffi::FfiMesh {
                vertices: Vec::new(),
                triangles: Vec::new(),
            },
            TestPayload::Present => ffi::FfiMesh {
                vertices: vec![0.0],
                triangles: vec![0],
            },
        };
        let (output_limit, output_count) = if case.code == Code::OutputLimitExceeded {
            (ffi::FfiOutputLimit::Triangles, 1)
        } else {
            (ffi::FfiOutputLimit::Vertices, 0)
        };
        ffi::FfiResponse {
            code: case.code,
            manifold_status: case.status,
            has_mesh_index: case.mesh_index.is_some(),
            mesh_index: case.mesh_index.unwrap_or(0),
            output_limit,
            output_count,
            mesh,
        }
    }

    fn case(code: ffi::FfiErrorCode) -> EnvelopeCase {
        ENVELOPES
            .iter()
            .copied()
            .find(|case| case.code == code)
            .expect("every tested response code has one independent canonical envelope")
    }

    #[test]
    fn every_native_response_code_has_one_strict_canonical_envelope() {
        for case in ENVELOPES {
            assert_eq!(validate_native_envelope(&response(case)), Ok(()));
        }
    }

    #[test]
    fn every_envelope_rejects_status_index_and_payload_contradictions() {
        for canonical in ENVELOPES {
            let mut bad_status = response(canonical);
            bad_status.manifold_status = if canonical.status == 0 { 1 } else { 0 };
            assert_eq!(
                validate_native_envelope(&bad_status),
                Err(SolidError::InvalidNativeResponse)
            );

            let mut bad_index_presence = response(canonical);
            bad_index_presence.has_mesh_index = canonical.mesh_index.is_none();
            assert_eq!(
                validate_native_envelope(&bad_index_presence),
                Err(SolidError::InvalidNativeResponse)
            );

            if canonical.mesh_index.is_none() {
                let mut hidden_index = response(canonical);
                hidden_index.mesh_index = 9;
                assert_eq!(
                    validate_native_envelope(&hidden_index),
                    Err(SolidError::InvalidNativeResponse)
                );
            }

            let mut bad_payload = response(canonical);
            match canonical.payload {
                TestPayload::Empty => bad_payload.mesh.vertices.push(1.0),
                TestPayload::Present => bad_payload.mesh.triangles.clear(),
            }
            assert_eq!(
                validate_native_envelope(&bad_payload),
                Err(SolidError::InvalidNativeResponse)
            );
        }
    }

    #[test]
    fn native_response_codes_retain_semantic_rust_errors() {
        assert_eq!(
            map_native_response(response(case(ffi::FfiErrorCode::OffsetSpan))),
            Err(SolidError::NativeValidation(
                NativeValidationError::OffsetSpan
            ))
        );
        assert_eq!(
            map_native_response(response(case(ffi::FfiErrorCode::OutputOverflow))),
            Err(SolidError::NativeOverflow)
        );
        assert_eq!(
            map_native_response(response(case(
                ffi::FfiErrorCode::UnexpectedStandardException
            ))),
            Err(SolidError::UnexpectedNativeException(
                NativeExceptionKind::Standard
            ))
        );
        assert_eq!(
            map_native_response(response(case(ffi::FfiErrorCode::InputManifoldStatus))),
            Err(SolidError::ManifoldInput {
                mesh_index: 7,
                status: ManifoldStatus::NotManifold,
            })
        );
        assert_eq!(
            map_native_response(response(EnvelopeCase {
                status: 99,
                ..case(ffi::FfiErrorCode::ResultManifoldStatus)
            })),
            Err(SolidError::ManifoldResult {
                status: ManifoldStatus::Unknown(99),
            })
        );
        assert_eq!(
            map_native_response(response(EnvelopeCase {
                status: 0,
                ..case(ffi::FfiErrorCode::ResultManifoldStatus)
            })),
            Err(SolidError::InvalidNativeResponse)
        );
    }

    #[test]
    fn input_estimate_counts_every_marshaled_primitive_exactly() {
        let meshes = [tetrahedron(), tetrahedron()];
        let estimate = BooleanInputEstimate::for_meshes(&meshes).unwrap();
        assert_eq!(estimate.vertex_scalars, 24);
        assert_eq!(estimate.triangle_indices, 24);
        assert_eq!(estimate.vertex_offset_units, 3);
        assert_eq!(estimate.triangle_offset_units, 3);
        assert_eq!(estimate.operand_primitive_units, 54);
        assert_eq!(estimate.logical_marshaled_input_bytes, 432);

        let exact = BooleanLimits {
            max_input_primitive_units: 54,
            max_logical_marshaled_input_bytes: 432,
            ..BooleanLimits::UNLIMITED
        };
        assert_eq!(validate_input_limits(estimate, exact), Ok(()));
        assert_eq!(
            validate_input_limits(
                estimate,
                BooleanLimits {
                    max_input_primitive_units: 53,
                    ..exact
                },
            ),
            Err(SolidError::InputLimitExceeded {
                limit: BooleanInputLimit::PrimitiveUnits,
                actual: 54,
                maximum: 53,
            })
        );
        assert_eq!(
            validate_input_limits(
                estimate,
                BooleanLimits {
                    max_logical_marshaled_input_bytes: 431,
                    ..exact
                },
            ),
            Err(SolidError::InputLimitExceeded {
                limit: BooleanInputLimit::LogicalMarshalledInputBytes,
                actual: 432,
                maximum: 431,
            })
        );
    }

    #[test]
    fn input_limit_rejection_does_not_gather_or_call_native() {
        let meshes = [tetrahedron(), tetrahedron()];
        NATIVE_CALLS.store(0, Ordering::SeqCst);
        let error = run_batch_limited(
            &meshes,
            BooleanLimits {
                max_input_primitive_units: 53,
                ..BooleanLimits::UNLIMITED
            },
            counted_operation,
        )
        .expect_err("cap minus one must reject before FFI");
        assert_eq!(NATIVE_CALLS.load(Ordering::SeqCst), 0);
        assert_eq!(
            error,
            SolidError::InputLimitExceeded {
                limit: BooleanInputLimit::PrimitiveUnits,
                actual: 54,
                maximum: 53,
            }
        );
    }

    #[test]
    fn native_output_limit_is_typed_and_must_exceed_the_requested_cap() {
        let response = ffi::FfiResponse {
            code: ffi::FfiErrorCode::OutputLimitExceeded,
            manifold_status: 0,
            has_mesh_index: false,
            mesh_index: 0,
            output_limit: ffi::FfiOutputLimit::Triangles,
            output_count: 13,
            mesh: ffi::FfiMesh {
                vertices: Vec::new(),
                triangles: Vec::new(),
            },
        };
        let limits = BooleanLimits {
            max_output_triangles: 12,
            ..BooleanLimits::UNLIMITED
        };
        assert_eq!(
            super::map_native_response_with_limits(response, limits),
            Err(SolidError::OutputLimitExceeded {
                limit: BooleanOutputLimit::Triangles,
                actual: 13,
                maximum: 12,
            })
        );
    }

    #[test]
    fn input_estimate_checked_arithmetic_rejects_synthetic_overflow() {
        assert_eq!(
            calculate_input_estimate(u64::MAX, 0, 0),
            Err(SolidError::BatchTooLarge)
        );
        assert_eq!(
            calculate_input_estimate(0, u64::MAX, 0),
            Err(SolidError::BatchTooLarge)
        );
        assert_eq!(
            calculate_input_estimate(0, u64::MAX / 8, 0),
            Err(SolidError::BatchTooLarge)
        );
    }

    #[test]
    fn manifold_u32_import_preflight_has_exact_synthetic_boundaries() {
        assert_eq!(
            validate_manifold_input_counts(7, u32::MAX as usize, u32::MAX as usize),
            Ok(())
        );
        assert_eq!(
            validate_manifold_input_counts(7, (u32::MAX as usize) + 1, 1),
            Err(SolidError::ManifoldInputLimitExceeded {
                mesh_index: 7,
                limit: ManifoldInputLimit::VertexCount,
            })
        );
        assert_eq!(
            validate_manifold_input_counts(7, 1, (u32::MAX as usize) + 1),
            Err(SolidError::ManifoldInputLimitExceeded {
                mesh_index: 7,
                limit: ManifoldInputLimit::TriangleCount,
            })
        );
        assert_eq!(
            validate_manifold_triangle_index(7, 3, 2, u64::from(u32::MAX)),
            Ok(())
        );
        assert_eq!(
            validate_manifold_triangle_index(7, 3, 2, u64::from(u32::MAX) + 1),
            Err(SolidError::ManifoldInputLimitExceeded {
                mesh_index: 7,
                limit: ManifoldInputLimit::TriangleIndex {
                    triangle: 3,
                    corner: 2,
                    index: u64::from(u32::MAX) + 1,
                },
            })
        );
    }

    #[test]
    fn output_limits_are_checked_before_get_mesh_gl64() {
        let inputs = [
            cuboid([0.0, 0.0, 0.0], [3.0, 3.0, 3.0]),
            cuboid([0.5, 0.5, -0.5], [1.0, 1.0, 3.5]),
            cuboid([2.0, 2.0, -0.5], [2.5, 2.5, 3.5]),
        ];
        let backend = ManifoldBackend;
        let ordinary = backend.difference(&inputs).unwrap();
        let limits = BooleanLimits {
            max_output_vertices: ordinary.vertices().len() as u64,
            max_output_triangles: ordinary.triangles().len() as u64,
            ..BooleanLimits::UNLIMITED
        };

        ffi::reset_export_mesh_calls();
        assert_eq!(
            backend.difference_limited(&inputs, limits).unwrap(),
            ordinary
        );
        assert_eq!(ffi::export_mesh_calls(), 1);

        ffi::reset_export_mesh_calls();
        let vertex_error = backend
            .difference_limited(
                &inputs,
                BooleanLimits {
                    max_output_vertices: limits.max_output_vertices - 1,
                    ..limits
                },
            )
            .unwrap_err();
        assert_eq!(ffi::export_mesh_calls(), 0);
        assert_eq!(
            vertex_error,
            SolidError::OutputLimitExceeded {
                limit: BooleanOutputLimit::Vertices,
                actual: limits.max_output_vertices,
                maximum: limits.max_output_vertices - 1
            }
        );

        ffi::reset_export_mesh_calls();
        let triangle_error = backend
            .difference_limited(
                &inputs,
                BooleanLimits {
                    max_output_triangles: limits.max_output_triangles - 1,
                    ..limits
                },
            )
            .unwrap_err();
        assert_eq!(ffi::export_mesh_calls(), 0);
        assert_eq!(
            triangle_error,
            SolidError::OutputLimitExceeded {
                limit: BooleanOutputLimit::Triangles,
                actual: limits.max_output_triangles,
                maximum: limits.max_output_triangles - 1
            }
        );
    }
}
