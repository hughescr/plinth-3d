//! Deterministic batched horizontal-plane intersections over an immutable mesh.
//!
//! This module deliberately stops at face-local intersection events.  In
//! particular, it does not weld coincident edges or stitch loops: those are
//! policy decisions for the consumer of a slice, not geometry-kernel work.

use rayon::prelude::*;

use crate::par;
use crate::vec3::V3;
use crate::NativeSurface;

/// Explicit resource limits for [`NativeSurface::slice_z_batch`].
///
/// All caps are checked from a worst-case bound before any allocation whose
/// size grows with the input batch.  `max_output_bytes` accounts for the flat
/// event buffer and the plane-offset table, but makes no claim about allocator
/// internals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaneSliceLimits {
    pub max_planes: usize,
    pub max_triangle_plane_tests: usize,
    pub max_intersections: usize,
    pub max_output_bytes: usize,
}

/// A checked batch-slicing limit failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneSliceError {
    NonFinitePlane { index: usize },
    PlaneCountCap { requested: usize, cap: usize },
    TrianglePlaneWorkCap { requested: usize, cap: usize },
    IntersectionCap { requested: usize, cap: usize },
    OutputBytesCap { requested: usize, cap: usize },
    ArithmeticOverflow { section: &'static str },
}

impl std::fmt::Display for PlaneSliceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFinitePlane { index } => write!(f, "planes[{index}] is not finite"),
            Self::PlaneCountCap { requested, cap } => {
                write!(f, "plane count {requested} exceeds cap {cap}")
            }
            Self::TrianglePlaneWorkCap { requested, cap } => {
                write!(f, "triangle-plane work {requested} exceeds cap {cap}")
            }
            Self::IntersectionCap { requested, cap } => {
                write!(f, "slice intersections {requested} exceeds cap {cap}")
            }
            Self::OutputBytesCap { requested, cap } => {
                write!(f, "slice output bytes {requested} exceeds cap {cap}")
            }
            Self::ArithmeticOverflow { section } => {
                write!(f, "slice resource arithmetic overflow in {section}")
            }
        }
    }
}

impl std::error::Error for PlaneSliceError {}

/// The exact relation between one selected triangle and one requested plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneIntersectionKind {
    /// A plane crosses two non-coplanar triangle edges.
    Transverse,
    /// A plane passes through exactly one vertex.  A tangent has no segment;
    /// a crossing has a segment from that vertex to the opposite edge.
    Vertex,
    /// A plane contains one triangle edge.  The segment is that source edge.
    Edge,
    /// A plane contains all three vertices.  No arbitrary triangle edge is
    /// selected as a segment, avoiding a hidden loop-stitching policy.
    Coplanar,
}

/// A face-local event in one horizontal-plane query.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaneIntersection {
    /// Immutable global face ID, in ascending selected-face order.
    pub face_id: u32,
    pub kind: PlaneIntersectionKind,
    /// The exact segment when this relation has one.  `None` records a
    /// vertex tangent or fully coplanar triangle without inventing geometry.
    pub segment: Option<[V3; 2]>,
}

/// Flattened batch result. `plane_offsets` has `planes.len() + 1` entries;
/// events for plane `i` occupy `intersections[offsets[i]..offsets[i + 1]]`.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaneSliceBatch {
    pub plane_offsets: Vec<usize>,
    pub intersections: Vec<PlaneIntersection>,
}

impl PlaneSliceBatch {
    pub fn plane(&self, index: usize) -> &[PlaneIntersection] {
        &self.intersections[self.plane_offsets[index]..self.plane_offsets[index + 1]]
    }
}

impl NativeSurface {
    /// Intersect all selected faces against all horizontal `z` planes in one
    /// batch.  Planes retain caller order; within each plane events retain
    /// ascending global face ID.  Comparisons to a plane are exact `f64 ==`,
    /// so the classifications make every boundary convention visible.
    pub fn slice_z_batch(
        &self,
        planes: &[f64],
        limits: PlaneSliceLimits,
    ) -> Result<PlaneSliceBatch, PlaneSliceError> {
        if let Some((index, _)) = planes.iter().enumerate().find(|(_, z)| !z.is_finite()) {
            return Err(PlaneSliceError::NonFinitePlane { index });
        }
        if planes.len() > limits.max_planes {
            return Err(PlaneSliceError::PlaneCountCap {
                requested: planes.len(),
                cap: limits.max_planes,
            });
        }
        let work = planes.len().checked_mul(self.mesh.nfaces()).ok_or(
            PlaneSliceError::ArithmeticOverflow {
                section: "triangle-plane work",
            },
        )?;
        if work > limits.max_triangle_plane_tests {
            return Err(PlaneSliceError::TrianglePlaneWorkCap {
                requested: work,
                cap: limits.max_triangle_plane_tests,
            });
        }
        // Each face-plane test produces at most one event.
        if work > limits.max_intersections {
            return Err(PlaneSliceError::IntersectionCap {
                requested: work,
                cap: limits.max_intersections,
            });
        }
        let offset_bytes = planes
            .len()
            .checked_add(1)
            .and_then(|n| n.checked_mul(std::mem::size_of::<usize>()))
            .ok_or(PlaneSliceError::ArithmeticOverflow {
                section: "offset bytes",
            })?;
        let event_bytes = work
            .checked_mul(std::mem::size_of::<PlaneIntersection>())
            .ok_or(PlaneSliceError::ArithmeticOverflow {
                section: "event bytes",
            })?;
        let output_bytes =
            offset_bytes
                .checked_add(event_bytes)
                .ok_or(PlaneSliceError::ArithmeticOverflow {
                    section: "output bytes",
                })?;
        if output_bytes > limits.max_output_bytes {
            return Err(PlaneSliceError::OutputBytesCap {
                requested: output_bytes,
                cap: limits.max_output_bytes,
            });
        }

        let per_plane: Vec<Vec<PlaneIntersection>> =
            par::run(|| planes.par_iter().map(|&z| self.slice_one_z(z)).collect());
        let mut plane_offsets = Vec::with_capacity(planes.len() + 1);
        let mut intersections = Vec::with_capacity(work);
        plane_offsets.push(0);
        for events in per_plane {
            intersections.extend(events);
            plane_offsets.push(intersections.len());
        }
        Ok(PlaneSliceBatch {
            plane_offsets,
            intersections,
        })
    }

    fn slice_one_z(&self, z: f64) -> Vec<PlaneIntersection> {
        let mut events = Vec::new();
        for &face_id in self.mesh.face_ids() {
            let i = face_id as usize;
            let vertices = [
                self.mesh.v0(i),
                self.mesh.vertex_b(i),
                self.mesh.vertex_c(i),
            ];
            if let Some(event) = intersect_triangle_z(face_id, vertices, z) {
                events.push(event);
            }
        }
        events
    }
}

fn intersect_triangle_z(face_id: u32, v: [V3; 3], z: f64) -> Option<PlaneIntersection> {
    let d = [v[0][2] - z, v[1][2] - z, v[2][2] - z];
    let mut zero = [0usize; 3];
    let mut zero_count = 0;
    for (i, &distance) in d.iter().enumerate() {
        if distance == 0.0 {
            zero[zero_count] = i;
            zero_count += 1;
        }
    }
    match zero_count {
        3 => Some(PlaneIntersection {
            face_id,
            kind: PlaneIntersectionKind::Coplanar,
            segment: None,
        }),
        2 => Some(PlaneIntersection {
            face_id,
            kind: PlaneIntersectionKind::Edge,
            segment: Some([v[zero[0]], v[zero[1]]]),
        }),
        1 => {
            let vertex = zero[0];
            let other_a = (vertex + 1) % 3;
            let other_b = (vertex + 2) % 3;
            if same_side(d[other_a], d[other_b]) {
                Some(PlaneIntersection {
                    face_id,
                    kind: PlaneIntersectionKind::Vertex,
                    segment: None,
                })
            } else {
                Some(PlaneIntersection {
                    face_id,
                    kind: PlaneIntersectionKind::Vertex,
                    segment: Some([v[vertex], interpolate_z(v[other_a], v[other_b], z)]),
                })
            }
        }
        0 if !same_side(d[0], d[1]) || !same_side(d[1], d[2]) || !same_side(d[2], d[0]) => {
            let mut points = [[0.0; 3]; 2];
            let mut point_count = 0;
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                if !same_side(d[a], d[b]) {
                    points[point_count] = interpolate_z(v[a], v[b], z);
                    point_count += 1;
                }
            }
            debug_assert_eq!(point_count, 2);
            Some(PlaneIntersection {
                face_id,
                kind: PlaneIntersectionKind::Transverse,
                segment: Some([points[0], points[1]]),
            })
        }
        _ => None,
    }
}

#[inline]
fn same_side(a: f64, b: f64) -> bool {
    (a < 0.0 && b < 0.0) || (a > 0.0 && b > 0.0)
}

#[inline]
fn interpolate_z(a: V3, b: V3, z: f64) -> V3 {
    let t = (z - a[2]) / (b[2] - a[2]);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, z]
}
