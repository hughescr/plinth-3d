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
        if !config.grid_pitch_mm.is_finite() || config.grid_pitch_mm <= 0.0 {
            return Err(SurfaceBuildError::InvalidGridPitch(config.grid_pitch_mm));
        }
        let mesh = Mesh::from_indexed(vertices, faces)?;
        Self::new(mesh, config.grid_pitch_mm, config.max_grid_bytes)
            .map_err(SurfaceBuildError::Kernel)
    }

    /// Number of faces in the immutable surface.
    pub fn nfaces(&self) -> usize {
        self.mesh.nfaces()
    }

    /// Immutable global face metadata. This view has no mutable access and
    /// does not expose the shared ownership mechanism.
    pub fn geometry(&self) -> &FaceGeometry {
        self.mesh.geometry()
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
