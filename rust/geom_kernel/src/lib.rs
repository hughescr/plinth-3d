//! Native-first, batch-oriented triangle-soup geometry kernel.
//!
//! DETERMINISM REVIEW CHECKLIST (spec sec 4.2, R6). Any edit must preserve:
//!  * Parallelism is index-parallel map-collect ONLY (`par.rs`). No
//!    rayon sum/reduce/fold over floats anywhere.
//!  * No HashMap/HashSet iteration feeds any result (parity grids are CSR;
//!    the pool cache is a Vec).
//!  * No wall-clock reads in the kernel.
//!  * Mutable state is confined to result-inert caches and telemetry atomics.

mod bvh;
mod closest;
mod consts;
mod mesh;
mod moller;
mod par;
mod parity;
mod point_index;
mod queries;
mod segment_batch;
mod slicing;
mod vec3;
mod winding;

pub mod testkit;

use std::borrow::Cow;

use rayon::prelude::*;
use sha2::{Digest, Sha256};

use mesh::Mesh;
pub use mesh::{FaceBounds, FaceGeometry};
use parity::ParityBuildError;
pub use point_index::{
    knn_single_source_planned_into, KnnBatchLimits, KnnBatchPlan, KnnBatchPlanError, NeighborCsr,
    PointCloudIndex, PointIndexBuildError, PointQueryError, RadiusQueryLimits,
};
pub use queries::capsule_rays;
pub use queries::{
    AllowedFaceProjection, AllowedFaceProjectionError, AllowedFaceProjectionLimits,
    AllowedFaceProjectionNoWinner, AllowedFaceProjectionOutcome, AllowedFaceProjectionPolicy,
    BatchInputError, CapsuleRay, CapsuleRays, CastRaysBatch, Interval, NativeSurface,
    PenetrationBatchError, PenetrationBatchLimits, PenetrationBatchMeter, PenetrationRowCensus,
    SphereBreachBatch,
};
pub use segment_batch::{
    point_segment_distances_batch, point_segment_taper_batch, FrozenJointTaperBatch,
    PointSegmentBatchError, PointSegmentBatchLimits, PointSegmentDistanceBatch,
    PointSegmentTaperBatch,
};
pub use slicing::{
    PlaneIntersection, PlaneIntersectionKind, PlaneSliceBatch, PlaneSliceError, PlaneSliceLimits,
    PLANE_SLICE_MERGE_TOLERANCE_MM,
};
pub use vec3::V3;
pub use winding::{
    signed_soup_volumes, wind_soups_outward, SignedSoupVolumeBatch, SignedVolumeError, SoupTriangle,
};

pub const KERNEL_API_VERSION: u32 = 1;

/// Möller barycentric and ray-parameter acceptance slack used by the native
/// kernel. This is specifically the `t in [-slack, t_max + slack]` boundary;
/// consumers must not treat it as a general geometry tolerance.
pub const MOLLER_PARAMETER_SLACK: f64 = consts::EPS;

/// Collision-resistant content identity for an exact, ordered triangle soup.
///
/// This is an opaque value, not an unforgeable capability. It has no unchecked
/// raw-byte constructor, but callers can reproducibly compute the same value
/// through [`triangle_soup_digest`] or [`indexed_triangle_soup_digest`]. A
/// caller can compare or inspect digest values; it cannot overwrite the digest
/// retained inside an already constructed [`NativeSurface`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceTriangleSoupDigest([u8; 32]);

impl SurfaceTriangleSoupDigest {
    /// Borrow the canonical digest bytes for comparison or serialization.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

const TRIANGLE_SOUP_DIGEST_DOMAIN: &[u8] = b"plinth.native-surface.triangle-soup.sha256.v1\0";

/// Reproducibly compute the canonical content identity for a logical triangle
/// soup in face, vertex, and coordinate order.
///
/// Components are encoded as their raw IEEE-754 bits in little-endian order,
/// so signed zero remains distinct. `NativeSurface` construction rejects
/// non-finite coordinates before retaining this identity.
#[must_use]
pub fn triangle_soup_digest(triangles: &[[[f64; 3]; 3]]) -> SurfaceTriangleSoupDigest {
    triangle_soup_digest_from_iter(triangles.iter().copied())
}

pub(crate) fn triangle_soup_digest_from_iter<I>(triangles: I) -> SurfaceTriangleSoupDigest
where
    I: ExactSizeIterator<Item = [[f64; 3]; 3]>,
{
    let mut hasher = Sha256::new();
    hasher.update(TRIANGLE_SOUP_DIGEST_DOMAIN);
    hasher.update(
        u64::try_from(triangles.len())
            .expect("triangle soup length always fits in u64")
            .to_le_bytes(),
    );
    for triangle in triangles {
        for vertex in triangle {
            for component in vertex {
                hasher.update(component.to_bits().to_le_bytes());
            }
        }
    }
    SurfaceTriangleSoupDigest(hasher.finalize().into())
}

/// Reproducibly compute the canonical content identity for an indexed mesh as
/// its ordered, expanded triangle soup.
///
/// Returns `None` when a face index is outside `vertices`. This helper does
/// not perform `NativeSurface`'s finite-input validation; use it only after a
/// mesh boundary has established valid source coordinates.
#[must_use]
pub fn indexed_triangle_soup_digest(
    vertices: &[[f64; 3]],
    faces: &[[u32; 3]],
) -> Option<SurfaceTriangleSoupDigest> {
    let mut hasher = Sha256::new();
    hasher.update(TRIANGLE_SOUP_DIGEST_DOMAIN);
    hasher.update(
        u64::try_from(faces.len())
            .expect("face count always fits in u64")
            .to_le_bytes(),
    );
    for indices in faces {
        for &index in indices {
            let vertex = vertices.get(index as usize)?;
            for component in vertex {
                hasher.update(component.to_bits().to_le_bytes());
            }
        }
    }
    Some(SurfaceTriangleSoupDigest(hasher.finalize().into()))
}

/// Explicit parity-grid construction settings. There is deliberately no
/// default: callers must select their production memory/precision budget.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeSurfaceConfig {
    pub grid_pitch_mm: f64,
    pub max_grid_bytes: usize,
}

/// Allocation-free conservative memory bound for an indexed native surface.
/// It includes the immutable face store, BVH construction/retention, and all
/// three parity-grid transient and retained buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeSurfaceMemoryEstimate {
    peak_bytes: usize,
    retained_bytes: usize,
    max_axis_grid_bytes: usize,
    axis_references: [usize; 3],
}

impl NativeSurfaceMemoryEstimate {
    #[must_use]
    pub const fn peak_bytes(self) -> usize {
        self.peak_bytes
    }

    #[must_use]
    pub const fn retained_bytes(self) -> usize {
        self.retained_bytes
    }

    /// The exact conservative cap to pass as
    /// [`NativeSurfaceConfig::max_grid_bytes`] for this mesh and pitch.
    #[must_use]
    pub const fn max_axis_grid_bytes(self) -> usize {
        self.max_axis_grid_bytes
    }

    #[must_use]
    pub const fn axis_references(self) -> [usize; 3] {
        self.axis_references
    }
}

/// Typed triangle-soup inputs accepted by the native construction boundary.
///
/// `borrowed` avoids copying at the API boundary; `owned` lets callers transfer
/// ownership. Construction builds one immutable face store directly from this
/// source metadata; it does not flatten or recompute supplied metadata.
#[derive(Clone, Debug)]
pub struct TriangleSoup<'a> {
    triangles: Cow<'a, [[[f64; 3]; 3]]>,
    normals: Cow<'a, [[f64; 3]]>,
    areas: Cow<'a, [f64]>,
    bounds: [[f64; 3]; 2],
}

impl<'a> TriangleSoup<'a> {
    pub fn borrowed(
        triangles: &'a [[[f64; 3]; 3]],
        normals: &'a [[f64; 3]],
        areas: &'a [f64],
        bounds: [[f64; 3]; 2],
    ) -> Self {
        Self {
            triangles: Cow::Borrowed(triangles),
            normals: Cow::Borrowed(normals),
            areas: Cow::Borrowed(areas),
            bounds,
        }
    }

    pub fn triangles(&self) -> &[[[f64; 3]; 3]] {
        &self.triangles
    }

    pub fn normals(&self) -> &[[f64; 3]] {
        &self.normals
    }

    pub fn areas(&self) -> &[f64] {
        &self.areas
    }

    pub fn bounds(&self) -> [[f64; 3]; 2] {
        self.bounds
    }
}

impl TriangleSoup<'static> {
    pub fn owned(
        triangles: Vec<[[f64; 3]; 3]>,
        normals: Vec<[f64; 3]>,
        areas: Vec<f64>,
        bounds: [[f64; 3]; 2],
    ) -> Self {
        Self {
            triangles: Cow::Owned(triangles),
            normals: Cow::Owned(normals),
            areas: Cow::Owned(areas),
            bounds,
        }
    }
}

/// Named failures from the public native construction boundary.
#[derive(Debug, PartialEq)]
pub enum SurfaceBuildError {
    EmptyTriangleSoup,
    EmptyVertices,
    EmptyFaces,
    NonFiniteVertex {
        vertex: usize,
        axis: usize,
    },
    FaceIndexOutOfBounds {
        face: usize,
        corner: usize,
        index: u32,
        vertex_count: usize,
    },
    IndexedDerivedArithmetic {
        face: usize,
        operation: &'static str,
    },
    MemoryEstimateOverflow {
        section: &'static str,
    },
    LengthMismatch {
        field: &'static str,
        expected: usize,
        actual: usize,
    },
    NonFinite {
        field: &'static str,
        index: usize,
    },
    MalformedBounds {
        axis: usize,
        min: f64,
        max: f64,
    },
    InvalidGridPitch(f64),
    GridBudget {
        axis: usize,
        projected_bytes: usize,
        limit_bytes: usize,
    },
    Kernel(String),
}

impl std::fmt::Display for SurfaceBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyTriangleSoup => {
                write!(formatter, "triangle soup must contain at least one face")
            }
            Self::EmptyVertices => write!(
                formatter,
                "indexed surface must contain at least one vertex"
            ),
            Self::EmptyFaces => write!(formatter, "indexed surface must contain at least one face"),
            Self::NonFiniteVertex { vertex, axis } => {
                write!(formatter, "vertices[{vertex}][{axis}] is not finite")
            }
            Self::FaceIndexOutOfBounds {
                face,
                corner,
                index,
                vertex_count,
            } => write!(
                formatter,
                "faces[{face}][{corner}] index {index} is outside {vertex_count} vertices"
            ),
            Self::IndexedDerivedArithmetic { face, operation } => write!(
                formatter,
                "indexed face {face} produced non-finite {operation}"
            ),
            Self::MemoryEstimateOverflow { section } => {
                write!(formatter, "native memory estimate overflow in {section}")
            }
            Self::LengthMismatch {
                field,
                expected,
                actual,
            } => write!(formatter, "{field} has {actual} rows; expected {expected}"),
            Self::NonFinite { field, index } => {
                write!(formatter, "{field}[{index}] is not finite")
            }
            Self::MalformedBounds { axis, min, max } => write!(
                formatter,
                "bounds axis {axis} is malformed: minimum {min} exceeds maximum {max}"
            ),
            Self::InvalidGridPitch(value) => write!(
                formatter,
                "grid_pitch_mm must be finite and greater than zero, got {value}"
            ),
            Self::GridBudget {
                axis,
                projected_bytes,
                limit_bytes,
            } => write!(
                formatter,
                "parity grid axis {axis} exceeds memory budget: {projected_bytes} bytes projected > {limit_bytes} budget"
            ),
            Self::Kernel(message) => write!(formatter, "kernel construction failed: {message}"),
        }
    }
}

impl std::error::Error for SurfaceBuildError {}

/// Native index whose construction failed while creating a surface subset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurfaceIndexKind {
    Bvh,
    Parity,
}

/// Named failures from narrowing an existing native surface by global face ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SurfaceSubsetError {
    EmptySelection,
    DuplicateFaceId {
        face_id: u32,
    },
    FaceIdOutOfBounds {
        position: usize,
        face_id: u32,
        global_face_count: usize,
    },
    FaceIdNotSelected {
        position: usize,
        face_id: u32,
    },
    IndexBuild {
        index: SurfaceIndexKind,
    },
}

impl std::fmt::Display for SurfaceSubsetError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptySelection => write!(formatter, "surface subset must select at least one face"),
            Self::DuplicateFaceId { face_id } => {
                write!(formatter, "surface subset repeats global face ID {face_id}")
            }
            Self::FaceIdOutOfBounds {
                position,
                face_id,
                global_face_count,
            } => write!(
                formatter,
                "surface subset face_ids[{position}] is {face_id}, outside {global_face_count} global faces"
            ),
            Self::FaceIdNotSelected { position, face_id } => write!(
                formatter,
                "surface subset face_ids[{position}] global face ID {face_id} is not selected by the parent surface"
            ),
            Self::IndexBuild { index } => {
                write!(formatter, "surface subset {index:?} index construction failed")
            }
        }
    }
}

impl std::error::Error for SurfaceSubsetError {}

fn map_surface_kernel_error(error: ParityBuildError) -> SurfaceBuildError {
    match error {
        ParityBuildError::GridBudget {
            axis,
            projected_bytes,
            limit_bytes,
        } => SurfaceBuildError::GridBudget {
            axis,
            projected_bytes,
            limit_bytes,
        },
        ParityBuildError::Other(message) => SurfaceBuildError::Kernel(message),
    }
}

impl NativeSurface {
    /// Compute a checked conservative peak before constructing any native
    /// face, BVH, or parity-grid allocation. The estimate deliberately uses
    /// upper bounds for BVH nodes, recursive partition scratch, and unique CSR
    /// keys; it may reject early but never understates those native buffers.
    pub fn estimate_index_memory_from_indexed(
        vertices: &[[f64; 3]],
        faces: &[[u32; 3]],
        grid_pitch_mm: f64,
    ) -> Result<NativeSurfaceMemoryEstimate, SurfaceBuildError> {
        if !grid_pitch_mm.is_finite() || grid_pitch_mm <= 0.0 {
            return Err(SurfaceBuildError::InvalidGridPitch(grid_pitch_mm));
        }
        let bounds = Mesh::validate_indexed_bounds(vertices, faces)?;
        let parity = parity::estimate_indexed_memory(vertices, faces, bounds, grid_pitch_mm)
            .map_err(|_| SurfaceBuildError::MemoryEstimateOverflow {
                section: "parity grid",
            })?;
        let faces_len = faces.len();
        let face_store_bytes = faces_len
            .checked_mul(
                7 * std::mem::size_of::<[f64; 3]>()
                    + std::mem::size_of::<f64>()
                    + std::mem::size_of::<mesh::FaceBounds>(),
            )
            .and_then(|bytes| {
                faces_len
                    .checked_mul(std::mem::size_of::<u32>())
                    .and_then(|face_ids| bytes.checked_add(face_ids))
            })
            .ok_or(SurfaceBuildError::MemoryEstimateOverflow {
                section: "face store",
            })?;
        let bvh_node_count = faces_len
            .checked_mul(2)
            .and_then(|value| value.checked_add(1))
            .ok_or(SurfaceBuildError::MemoryEstimateOverflow {
                section: "BVH nodes",
            })?;
        let bvh_retained_bytes = bvh_node_count
            .checked_mul(std::mem::size_of::<bvh::Node>())
            .and_then(|bytes| {
                faces_len
                    .checked_mul(std::mem::size_of::<u32>())
                    .and_then(|order| bytes.checked_add(order))
            })
            .ok_or(SurfaceBuildError::MemoryEstimateOverflow {
                section: "BVH retained",
            })?;
        // `tri_pad` is one f64 per face. Recursive `bin_of` vectors can remain
        // live down a branch; twice the root length is a geometric-series
        // upper bound for their total usize capacity.
        let bvh_build_scratch = faces_len
            .checked_mul(std::mem::size_of::<f64>())
            .and_then(|bytes| {
                faces_len
                    .checked_mul(2 * std::mem::size_of::<usize>())
                    .and_then(|bins| bytes.checked_add(bins))
            })
            .ok_or(SurfaceBuildError::MemoryEstimateOverflow {
                section: "BVH build scratch",
            })?;
        let retained_without_parity = face_store_bytes.checked_add(bvh_retained_bytes).ok_or(
            SurfaceBuildError::MemoryEstimateOverflow {
                section: "native retained",
            },
        )?;
        let bvh_peak = retained_without_parity
            .checked_add(bvh_build_scratch)
            .ok_or(SurfaceBuildError::MemoryEstimateOverflow {
                section: "BVH peak",
            })?;
        let parity_peak = retained_without_parity
            .checked_add(parity.peak_bytes)
            .ok_or(SurfaceBuildError::MemoryEstimateOverflow {
                section: "parity peak",
            })?;
        let retained_bytes = retained_without_parity
            .checked_add(parity.retained_bytes)
            .ok_or(SurfaceBuildError::MemoryEstimateOverflow {
                section: "native retained",
            })?;
        Ok(NativeSurfaceMemoryEstimate {
            peak_bytes: bvh_peak.max(parity_peak).max(retained_bytes),
            retained_bytes,
            max_axis_grid_bytes: parity.max_axis_build_bytes,
            axis_references: parity.axis_references,
        })
    }

    /// Construct the existing query engine from typed owned or borrowed data.
    ///
    /// This validates then constructs one immutable face store before building
    /// the BVH and parity-grid indexes.
    pub fn from_triangle_soup(
        soup: TriangleSoup<'_>,
        grid_pitch_mm: f64,
        max_grid_bytes: usize,
    ) -> Result<Self, SurfaceBuildError> {
        let nfaces = soup.triangles.len();
        if nfaces == 0 {
            return Err(SurfaceBuildError::EmptyTriangleSoup);
        }
        if soup.normals.len() != nfaces {
            return Err(SurfaceBuildError::LengthMismatch {
                field: "normals",
                expected: nfaces,
                actual: soup.normals.len(),
            });
        }
        if soup.areas.len() != nfaces {
            return Err(SurfaceBuildError::LengthMismatch {
                field: "areas",
                expected: nfaces,
                actual: soup.areas.len(),
            });
        }
        if !grid_pitch_mm.is_finite() || grid_pitch_mm <= 0.0 {
            return Err(SurfaceBuildError::InvalidGridPitch(grid_pitch_mm));
        }

        let flat_bounds = [
            soup.bounds[0][0],
            soup.bounds[0][1],
            soup.bounds[0][2],
            soup.bounds[1][0],
            soup.bounds[1][1],
            soup.bounds[1][2],
        ];

        for (face, triangle) in soup.triangles.iter().enumerate() {
            for (corner, vertex) in triangle.iter().enumerate() {
                for (axis, value) in vertex.iter().enumerate() {
                    if !value.is_finite() {
                        return Err(SurfaceBuildError::NonFinite {
                            field: "triangles",
                            index: face * 9 + corner * 3 + axis,
                        });
                    }
                }
            }
        }
        for (face, normal) in soup.normals.iter().enumerate() {
            for (axis, value) in normal.iter().enumerate() {
                if !value.is_finite() {
                    return Err(SurfaceBuildError::NonFinite {
                        field: "normals",
                        index: face * 3 + axis,
                    });
                }
            }
        }
        validate_finite("areas", &soup.areas)?;
        validate_finite("bounds", &flat_bounds)?;
        for axis in 0..3 {
            if soup.bounds[0][axis] > soup.bounds[1][axis] {
                return Err(SurfaceBuildError::MalformedBounds {
                    axis,
                    min: soup.bounds[0][axis],
                    max: soup.bounds[1][axis],
                });
            }
        }

        let mesh = Mesh::from_soup(&soup.triangles, &soup.normals, &soup.areas, soup.bounds);
        Self::new(mesh, grid_pitch_mm, max_grid_bytes).map_err(map_surface_kernel_error)
    }

    /// Construct a surface from indexed vertices and faces, deriving face
    /// metadata exactly once. Unlike `from_triangle_soup`, this path owns the
    /// normal/area/bounds derivation because there is no source metadata.
    pub fn from_indexed(
        vertices: &[[f64; 3]],
        faces: &[[u32; 3]],
        config: NativeSurfaceConfig,
    ) -> Result<Self, SurfaceBuildError> {
        if !config.grid_pitch_mm.is_finite() || config.grid_pitch_mm <= 0.0 {
            return Err(SurfaceBuildError::InvalidGridPitch(config.grid_pitch_mm));
        }
        let mesh = Mesh::from_indexed(vertices, faces)?;
        Self::new(mesh, config.grid_pitch_mm, config.max_grid_bytes)
            .map_err(map_surface_kernel_error)
    }

    /// Number of faces selected by this immutable surface view.
    pub fn nfaces(&self) -> usize {
        self.mesh.nfaces()
    }

    /// Selected global face IDs in deterministic ascending order.
    pub fn face_ids(&self) -> &[u32] {
        self.mesh.face_ids()
    }

    /// Query-index bounds for this surface view. A full triangle-soup surface
    /// retains its validated source-supplied bounds; proper subsets tightly
    /// enclose only their selected faces.
    pub fn bounds(&self) -> FaceBounds {
        self.mesh.bounds()
    }

    /// Exact immutable construction settings retained by this surface.
    #[must_use]
    pub const fn config(&self) -> NativeSurfaceConfig {
        NativeSurfaceConfig {
            grid_pitch_mm: self.grid_pitch_mm,
            max_grid_bytes: self.max_grid_bytes,
        }
    }

    /// Immutable global face metadata shared with every subset. Selected IDs
    /// are exposed separately by [`Self::face_ids`].
    pub fn geometry(&self) -> &FaceGeometry {
        self.mesh.geometry()
    }

    /// Read the exact source triangle-soup content identity retained at
    /// construction.
    ///
    /// Callers can independently reproduce this value with the canonical
    /// digest helpers, but cannot replace the immutable value stored by this
    /// surface.
    #[must_use]
    pub const fn source_triangle_soup_digest(&self) -> SurfaceTriangleSoupDigest {
        self.source_triangle_soup_digest
    }
}

/// Effective thread count the next batch operation will use.
///
/// Each operation re-reads `PLINTH_THREADS`, so changes take effect without
/// rebuilding the surface or process-global Rayon configuration.
pub fn current_threads() -> usize {
    par::current_threads()
}

/// Preflight limits for the only generic workspace-parallel operation:
/// ordered, independent index mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrderedMapLimits {
    max_items: usize,
    max_output_bytes: usize,
}

impl OrderedMapLimits {
    #[must_use]
    pub const fn new(max_items: usize, max_output_bytes: usize) -> Self {
        Self {
            max_items,
            max_output_bytes,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrderedMapError {
    ItemLimitExceeded { required: usize, maximum: usize },
    OutputSizeOverflow { items: usize, item_bytes: usize },
    OutputByteLimitExceeded { required: usize, maximum: usize },
}

impl std::fmt::Display for OrderedMapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ItemLimitExceeded { required, maximum } => {
                write!(
                    formatter,
                    "ordered map needs {required} items, limit is {maximum}"
                )
            }
            Self::OutputSizeOverflow { items, item_bytes } => write!(
                formatter,
                "ordered map output size overflows: {items} items of {item_bytes} bytes"
            ),
            Self::OutputByteLimitExceeded { required, maximum } => write!(
                formatter,
                "ordered map needs {required} output bytes, limit is {maximum}"
            ),
        }
    }
}

impl std::error::Error for OrderedMapError {}

/// Result of one bounded ordered map. Values retain exact input-index order.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderedMap<T> {
    values: Vec<T>,
    effective_threads: usize,
}

impl<T> OrderedMap<T> {
    #[must_use]
    pub const fn effective_threads(&self) -> usize {
        self.effective_threads
    }

    #[must_use]
    pub fn values(&self) -> &[T] {
        &self.values
    }

    #[must_use]
    pub fn into_values(self) -> Vec<T> {
        self.values
    }
}

/// Bounded index-parallel map/collect under the sole workspace thread pool.
/// There is deliberately no reduce/fold API: every result occupies its input
/// index and collection preserves that order.
pub fn ordered_index_map<I, O, F>(
    inputs: &[I],
    limits: OrderedMapLimits,
    operation: F,
) -> Result<OrderedMap<O>, OrderedMapError>
where
    I: Sync,
    O: Send,
    F: Fn(usize, &I) -> O + Sync + Send,
{
    if inputs.len() > limits.max_items {
        return Err(OrderedMapError::ItemLimitExceeded {
            required: inputs.len(),
            maximum: limits.max_items,
        });
    }
    let item_bytes = std::mem::size_of::<O>();
    let required =
        inputs
            .len()
            .checked_mul(item_bytes)
            .ok_or(OrderedMapError::OutputSizeOverflow {
                items: inputs.len(),
                item_bytes,
            })?;
    if required > limits.max_output_bytes {
        return Err(OrderedMapError::OutputByteLimitExceeded {
            required,
            maximum: limits.max_output_bytes,
        });
    }
    let (values, effective_threads) = par::run(|| {
        let effective_threads = rayon::current_num_threads();
        let values = inputs
            .par_iter()
            .enumerate()
            .map(|(index, input)| operation(index, input))
            .collect();
        (values, effective_threads)
    });
    Ok(OrderedMap {
        values,
        effective_threads,
    })
}

fn validate_finite(field: &'static str, values: &[f64]) -> Result<(), SurfaceBuildError> {
    if let Some((index, _)) = values
        .iter()
        .enumerate()
        .find(|(_, value)| !value.is_finite())
    {
        return Err(SurfaceBuildError::NonFinite { field, index });
    }
    Ok(())
}

#[cfg(test)]
mod thread_tests {
    use super::*;

    const TEST_UNBOUNDED_GRID_BYTES: usize = usize::MAX;
    const THREAD_SWEEP_CHILD: &str = "GEOM_KERNEL_THREAD_SWEEP_CHILD";
    const THREAD_SWEEP_SIGNATURE: &str = "GEOM_KERNEL_THREAD_SWEEP_SIGNATURE=";

    fn sweep_threads(test_name: &str) -> Option<Vec<String>> {
        if std::env::var(THREAD_SWEEP_CHILD).as_deref() == Ok("1") {
            return None;
        }
        Some(
            [1, 4, 16]
                .into_iter()
                .map(|threads| {
                    let output = std::process::Command::new(
                        std::env::current_exe().expect("current test executable"),
                    )
                    .args(["--exact", test_name, "--nocapture"])
                    .env("PLINTH_THREADS", threads.to_string())
                    .env(THREAD_SWEEP_CHILD, "1")
                    .output()
                    .expect("spawn isolated thread-sweep test");
                    assert!(
                        output.status.success(),
                        "thread sweep child {threads} failed:\n{}\n{}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr),
                    );
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let sentinel = format!("\n{THREAD_SWEEP_SIGNATURE}");
                    assert_eq!(
                        stdout.matches(&sentinel).count(),
                        1,
                        "thread sweep child must emit exactly one signature sentinel:\n{stdout}"
                    );
                    stdout
                        .split_once(&sentinel)
                        .and_then(|(_, signature)| signature.lines().next())
                        .map(str::to_owned)
                        .expect("thread sweep child must report its deterministic signature")
                })
                .collect(),
        )
    }

    #[test]
    fn indexed_memory_estimator_bills_mesh_face_ids_and_bvh_order_separately() {
        let vertices = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        let faces = [[0, 1, 2], [0, 3, 1]];
        let pitch = 0.5;
        let bounds = Mesh::validate_indexed_bounds(&vertices, &faces).unwrap();
        let parity = parity::estimate_indexed_memory(&vertices, &faces, bounds, pitch).unwrap();
        let face_count = faces.len();
        let geometry_arrays = face_count
            * (7 * std::mem::size_of::<[f64; 3]>()
                + std::mem::size_of::<f64>()
                + std::mem::size_of::<mesh::FaceBounds>());
        let mesh_face_ids = face_count * std::mem::size_of::<u32>();
        let bvh_nodes = (2 * face_count + 1) * std::mem::size_of::<bvh::Node>();
        let bvh_order = face_count * std::mem::size_of::<u32>();
        let expected_retained =
            geometry_arrays + mesh_face_ids + bvh_nodes + bvh_order + parity.retained_bytes;

        let estimate =
            NativeSurface::estimate_index_memory_from_indexed(&vertices, &faces, pitch).unwrap();
        assert_eq!(estimate.retained_bytes(), expected_retained);
        assert_eq!(mesh_face_ids, bvh_order);
        assert_eq!(
            mesh_face_ids + bvh_order,
            2 * face_count * std::mem::size_of::<u32>()
        );
    }

    #[test]
    fn ordered_index_map_preflights_caps_and_preserves_input_order() {
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let rejected = ordered_index_map(
            &[3_u32, 1, 2],
            OrderedMapLimits::new(2, 3 * std::mem::size_of::<u64>()),
            |index, value| {
                calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                (index as u64) << 32 | u64::from(*value)
            },
        );
        assert_eq!(
            rejected,
            Err(OrderedMapError::ItemLimitExceeded {
                required: 3,
                maximum: 2,
            })
        );
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 0);

        let mapped = ordered_index_map(
            &[3_u32, 1, 2],
            OrderedMapLimits::new(3, 3 * std::mem::size_of::<u64>()),
            |index, value| (index as u64) << 32 | u64::from(*value),
        )
        .unwrap();
        assert_eq!(mapped.values(), &[3, (1_u64 << 32) | 1, (2_u64 << 32) | 2]);
        assert_eq!(
            ordered_index_map(
                &[3_u32, 1, 2],
                OrderedMapLimits::new(3, 3 * std::mem::size_of::<u64>() - 1),
                |_, value| u64::from(*value),
            ),
            Err(OrderedMapError::OutputByteLimitExceeded {
                required: 3 * std::mem::size_of::<u64>(),
                maximum: 3 * std::mem::size_of::<u64>() - 1,
            })
        );
    }

    #[test]
    fn ordered_index_map_is_identical_at_one_four_and_sixteen_threads() {
        if let Some(signatures) = sweep_threads(
            "thread_tests::ordered_index_map_is_identical_at_one_four_and_sixteen_threads",
        ) {
            assert!(signatures.windows(2).all(|pair| pair[0] == pair[1]));
            return;
        }
        let expected = std::env::var("PLINTH_THREADS")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        let inputs: Vec<_> = (0_u64..4096).collect();
        let mapped = ordered_index_map(
            &inputs,
            OrderedMapLimits::new(inputs.len(), inputs.len() * std::mem::size_of::<u64>()),
            |index, value| value.rotate_left((index % 63) as u32),
        )
        .unwrap();
        assert_eq!(mapped.effective_threads(), expected);
        assert_eq!(mapped.values()[0], 0);
        assert_eq!(
            mapped.values()[4095],
            4095_u64.rotate_left((4095 % 63) as u32)
        );
        println!("{THREAD_SWEEP_SIGNATURE}{:?}", mapped.values());
    }

    #[test]
    fn batch_queries_reread_live_thread_pool_and_remain_identical() {
        if let Some(signatures) = sweep_threads(
            "thread_tests::batch_queries_reread_live_thread_pool_and_remain_identical",
        ) {
            assert!(signatures.windows(2).all(|pair| pair[0] == pair[1]));
            return;
        }
        let expected = std::env::var("PLINTH_THREADS")
            .expect("isolated sweep config")
            .parse::<usize>()
            .expect("positive isolated thread count");
        assert_eq!(current_threads(), expected);
        assert_eq!(par::run(rayon::current_num_threads), expected);
        let triangles = vec![[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]];
        let surface = NativeSurface::from_triangle_soup(
            TriangleSoup::owned(
                triangles,
                vec![[0.0, 0.0, 1.0]],
                vec![0.5],
                [[0.0, 0.0, 0.0], [1.0, 1.0, 0.0]],
            ),
            2.0,
            TEST_UNBOUNDED_GRID_BYTES,
        )
        .unwrap();
        let subset = surface.subset(&[0]).unwrap();
        let points = [[0.25, 0.25, 1.0], [2.0, 2.0, 0.0]];
        let result = (
            surface.query(&points, f64::INFINITY),
            subset.query(&points, f64::INFINITY),
            subset.inside(&points),
            surface.cast_rays(
                &[[0.25, 0.25, 1.0], [2.0, 2.0, 1.0]],
                &[[0.0, 0.0, -1.0], [0.0, 0.0, -1.0]],
                2.0,
            ),
        );
        println!("\n{THREAD_SWEEP_SIGNATURE}{result:?}");
    }
}
