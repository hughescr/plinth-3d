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
mod queries;
mod vec3;

pub mod testkit;

use std::borrow::Cow;

use mesh::Mesh;
pub use mesh::{FaceBounds, FaceGeometry};
pub use queries::{BatchInputError, CastRaysBatch, Interval, NativeSurface, SphereBreachBatch};
pub use vec3::V3;

pub const KERNEL_API_VERSION: u32 = 1;

/// Explicit parity-grid construction settings. There is deliberately no
/// default: callers must select their production memory/precision budget.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeSurfaceConfig {
    pub grid_pitch_mm: f64,
    pub max_grid_bytes: usize,
}

/// Typed triangle-soup inputs accepted by the native construction boundary.
///
/// `borrowed` avoids copying at the API boundary; `owned` lets callers transfer
/// ownership. Construction still flattens once for the unchanged `Mesh::new`
/// implementation.
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
            Self::Kernel(message) => write!(formatter, "kernel construction failed: {message}"),
        }
    }
}

impl std::error::Error for SurfaceBuildError {}

impl NativeSurface {
    /// Construct the existing query engine from typed owned or borrowed data.
    ///
    /// This validates the complete input before invoking the unchanged
    /// `Mesh::new`, BVH, and parity-grid algorithms.
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

        let mut flat_tris = Vec::with_capacity(nfaces * 9);
        for triangle in soup.triangles.iter() {
            for vertex in triangle {
                flat_tris.extend_from_slice(vertex);
            }
        }
        let mut flat_normals = Vec::with_capacity(nfaces * 3);
        for normal in soup.normals.iter() {
            flat_normals.extend_from_slice(normal);
        }
        let flat_bounds = [
            soup.bounds[0][0],
            soup.bounds[0][1],
            soup.bounds[0][2],
            soup.bounds[1][0],
            soup.bounds[1][1],
            soup.bounds[1][2],
        ];

        validate_finite("triangles", &flat_tris)?;
        validate_finite("normals", &flat_normals)?;
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

        let mesh = Mesh::new(&flat_tris, &flat_normals, &soup.areas, &flat_bounds)
            .map_err(SurfaceBuildError::Kernel)?;
        Self::new(mesh, grid_pitch_mm, max_grid_bytes).map_err(SurfaceBuildError::Kernel)
    }

    /// Construct a surface from indexed vertices and faces, deriving face
    /// metadata exactly once. Unlike `from_triangle_soup`, this path owns the
    /// normal/area/bounds derivation because there is no source metadata.
    pub fn from_indexed(
        vertices: &[[f64; 3]],
        faces: &[[u32; 3]],
        config: NativeSurfaceConfig,
    ) -> Result<Self, SurfaceBuildError> {
        if vertices.is_empty() {
            return Err(SurfaceBuildError::EmptyVertices);
        }
        if faces.is_empty() {
            return Err(SurfaceBuildError::EmptyFaces);
        }
        if !config.grid_pitch_mm.is_finite() || config.grid_pitch_mm <= 0.0 {
            return Err(SurfaceBuildError::InvalidGridPitch(config.grid_pitch_mm));
        }
        for (vertex, point) in vertices.iter().enumerate() {
            for (axis, value) in point.iter().enumerate() {
                if !value.is_finite() {
                    return Err(SurfaceBuildError::NonFiniteVertex { vertex, axis });
                }
            }
        }
        let mut triangles = Vec::with_capacity(faces.len());
        let mut normals = Vec::with_capacity(faces.len());
        let mut areas = Vec::with_capacity(faces.len());
        let mut bounds = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
        for (face, indices) in faces.iter().enumerate() {
            let mut triangle = [[0.0; 3]; 3];
            for (corner, &index) in indices.iter().enumerate() {
                let Some(point) = vertices.get(index as usize) else {
                    return Err(SurfaceBuildError::FaceIndexOutOfBounds {
                        face,
                        corner,
                        index,
                        vertex_count: vertices.len(),
                    });
                };
                triangle[corner] = *point;
                for axis in 0..3 {
                    bounds[0][axis] = bounds[0][axis].min(point[axis]);
                    bounds[1][axis] = bounds[1][axis].max(point[axis]);
                }
            }
            let e1 = [
                triangle[1][0] - triangle[0][0],
                triangle[1][1] - triangle[0][1],
                triangle[1][2] - triangle[0][2],
            ];
            let e2 = [
                triangle[2][0] - triangle[0][0],
                triangle[2][1] - triangle[0][1],
                triangle[2][2] - triangle[0][2],
            ];
            if !e1.iter().chain(&e2).all(|value| value.is_finite()) {
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
            if !cross.iter().all(|value| value.is_finite()) {
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
            let denominator = length.max(1e-30);
            let normal = [
                cross[0] / denominator,
                cross[1] / denominator,
                cross[2] / denominator,
            ];
            if !normal.iter().all(|value| value.is_finite()) {
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
                (triangle[0][0] + triangle[1][0] + triangle[2][0]) / 3.0,
                (triangle[0][1] + triangle[1][1] + triangle[2][1]) / 3.0,
                (triangle[0][2] + triangle[1][2] + triangle[2][2]) / 3.0,
            ];
            if !centroid.iter().all(|value| value.is_finite()) {
                return Err(SurfaceBuildError::IndexedDerivedArithmetic {
                    face,
                    operation: "centroid",
                });
            }
            triangles.push(triangle);
            normals.push(normal);
            areas.push(area);
        }
        Self::from_triangle_soup(
            TriangleSoup::owned(triangles, normals, areas, bounds),
            config.grid_pitch_mm,
            config.max_grid_bytes,
        )
    }

    /// Number of faces in the immutable surface.
    pub fn nfaces(&self) -> usize {
        self.mesh.nfaces()
    }

    /// Immutable global face metadata. This view has no mutable access and
    /// does not expose the shared ownership mechanism.
    pub fn geometry(&self) -> &FaceGeometry {
        &self.geometry
    }
}

/// Effective thread count the next batch operation will use.
///
/// Each operation re-reads `PLINTH_THREADS`, so changes take effect without
/// rebuilding the surface or process-global Rayon configuration.
pub fn current_threads() -> usize {
    par::current_threads()
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

    struct RestoreVar(Option<std::ffi::OsString>);

    impl Drop for RestoreVar {
        fn drop(&mut self) {
            if let Some(value) = self.0.take() {
                std::env::set_var("PLINTH_THREADS", value);
            } else {
                std::env::remove_var("PLINTH_THREADS");
            }
        }
    }

    #[test]
    fn batch_queries_reread_live_thread_pool_and_remain_identical() {
        let _restore = RestoreVar(std::env::var_os("PLINTH_THREADS"));
        let triangles = vec![[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]];
        let surface = NativeSurface::from_triangle_soup(
            TriangleSoup::owned(
                triangles,
                vec![[0.0, 0.0, 1.0]],
                vec![0.5],
                [[0.0, 0.0, 0.0], [1.0, 1.0, 0.0]],
            ),
            2.0,
            usize::MAX,
        )
        .unwrap();
        let points = [[0.25, 0.25, 1.0], [2.0, 2.0, 0.0]];
        let mut baseline = None;

        for expected in [1, 4, 16] {
            std::env::set_var("PLINTH_THREADS", expected.to_string());
            assert_eq!(current_threads(), expected);
            assert_eq!(par::run(rayon::current_num_threads), expected);

            let result = surface.query(&points, f64::INFINITY);
            if let Some(previous) = &baseline {
                assert_eq!(&result, previous);
            } else {
                baseline = Some(result);
            }
        }
    }
}
