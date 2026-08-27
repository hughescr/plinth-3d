//! Deterministic batched horizontal-plane intersections over an immutable mesh.
//!
//! The emitted segment stream reproduces the frozen Trimesh `mesh_plane`
//! cases used by `mesh_multiplane`: basic crossings first, then vertex
//! crossings, then positive-side on-plane edges. Coplanar faces, vertex
//! tangencies, and negative-side owners of a shared on-plane edge emit no
//! stitchable segment.

use std::sync::atomic::{AtomicU64, Ordering};

use rayon::prelude::*;

use crate::par;
use crate::vec3::V3;
use crate::NativeSurface;

/// Frozen `trimesh.constants.ToleranceMesh.merge` used by the Python oracle.
pub const PLANE_SLICE_MERGE_TOLERANCE_MM: f64 = 1.0e-8;

/// Explicit resource limits for [`NativeSurface::slice_z_batch`].
///
/// Every cap is checked from deterministic BVH candidate counts before
/// candidate or result buffers are allocated. Candidate counting uses one
/// reusable depth-bounded BVH traversal stack. `max_output_bytes` describes
/// only returned offset and worst-case segment storage, not allocator or
/// temporary-heap behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaneSliceLimits {
    pub max_planes: usize,
    pub max_triangle_plane_tests: usize,
    pub max_intersections: usize,
    pub max_output_bytes: usize,
}

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
                write!(
                    f,
                    "triangle-plane candidate work {requested} exceeds cap {cap}"
                )
            }
            Self::IntersectionCap { requested, cap } => {
                write!(f, "worst-case slice segments {requested} exceeds cap {cap}")
            }
            Self::OutputBytesCap { requested, cap } => {
                write!(
                    f,
                    "worst-case slice output bytes {requested} exceeds cap {cap}"
                )
            }
            Self::ArithmeticOverflow { section } => {
                write!(f, "slice resource arithmetic overflow in {section}")
            }
        }
    }
}

impl std::error::Error for PlaneSliceError {}

/// Frozen Trimesh stitchable triangle-plane cases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneIntersectionKind {
    Basic,
    Vertex,
    PositiveEdge,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaneIntersection {
    pub face_id: u32,
    pub kind: PlaneIntersectionKind,
    pub segment: [V3; 2],
}

/// Flattened deterministic segment batch. Planes retain caller order. Within
/// each plane the order is Basic, Vertex, PositiveEdge and then ascending
/// global face ID, matching frozen Trimesh's handler concatenation.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaneSliceBatch {
    pub plane_offsets: Vec<usize>,
    pub intersections: Vec<PlaneIntersection>,
    /// Result-inert execution telemetry for thread-sweep verification.
    pub effective_threads: usize,
    pub participating_threads: usize,
    /// Exact number of candidate faces classified after BVH z-slab culling.
    pub candidate_work: usize,
}

impl PlaneSliceBatch {
    pub fn plane(&self, index: usize) -> &[PlaneIntersection] {
        &self.intersections[self.plane_offsets[index]..self.plane_offsets[index + 1]]
    }
}

impl NativeSurface {
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

        // Candidate preflight reuses one small BVH-depth stack. Each candidate
        // can emit at most one segment, so the count is an output upper bound.
        let mut candidate_work = 0usize;
        let mut traversal_stack = Vec::with_capacity(self.bvh.depth + 1);
        for &z in planes {
            candidate_work = candidate_work
                .checked_add(self.count_z_candidates(z, &mut traversal_stack))
                .ok_or(PlaneSliceError::ArithmeticOverflow {
                    section: "candidate work",
                })?;
            if candidate_work > limits.max_triangle_plane_tests {
                return Err(PlaneSliceError::TrianglePlaneWorkCap {
                    requested: candidate_work,
                    cap: limits.max_triangle_plane_tests,
                });
            }
        }
        if candidate_work > limits.max_intersections {
            return Err(PlaneSliceError::IntersectionCap {
                requested: candidate_work,
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
        let event_bytes = candidate_work
            .checked_mul(std::mem::size_of::<PlaneIntersection>())
            .ok_or(PlaneSliceError::ArithmeticOverflow {
                section: "segment bytes",
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

        let effective_threads = par::configured_pool_threads();
        let worker_mask = AtomicU64::new(0);
        let per_plane: Vec<Vec<PlaneIntersection>> = par::run(|| {
            planes
                .par_iter()
                .map(|&z| {
                    if let Some(index) = rayon::current_thread_index() {
                        if index < 64 {
                            worker_mask.fetch_or(1u64 << index, Ordering::Relaxed);
                        }
                    }
                    self.slice_one_z(z)
                })
                .collect()
        });

        let mut plane_offsets = Vec::with_capacity(planes.len() + 1);
        let mut intersections = Vec::with_capacity(candidate_work);
        plane_offsets.push(0);
        for events in per_plane {
            intersections.extend(events);
            plane_offsets.push(intersections.len());
        }
        Ok(PlaneSliceBatch {
            plane_offsets,
            intersections,
            effective_threads,
            participating_threads: worker_mask.load(Ordering::Relaxed).count_ones() as usize,
            candidate_work,
        })
    }

    fn count_z_candidates(&self, z: f64, traversal_stack: &mut Vec<i32>) -> usize {
        let mut count = 0usize;
        self.visit_z_candidates(z, traversal_stack, |_| count += 1);
        count
    }

    fn z_candidates(&self, z: f64) -> Vec<u32> {
        let mut candidates = Vec::new();
        let mut traversal_stack = Vec::with_capacity(self.bvh.depth + 1);
        self.visit_z_candidates(z, &mut traversal_stack, |face_id| candidates.push(face_id));
        candidates.sort_unstable();
        candidates
    }

    #[expect(
        clippy::cast_sign_loss,
        reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged \
              as a class, and the one architecture item the class carried (geom_kernel's 32-bit BVH \
              index width) is now a named, enforced limit. TODO(cast-width-audit): retire site by \
              site.  See docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn visit_z_candidates(
        &self,
        z: f64,
        traversal_stack: &mut Vec<i32>,
        mut visit: impl FnMut(u32),
    ) {
        let low = z - PLANE_SLICE_MERGE_TOLERANCE_MM;
        let high = z + PLANE_SLICE_MERGE_TOLERANCE_MM;
        traversal_stack.clear();
        traversal_stack.push(0);
        while let Some(node_id) = traversal_stack.pop() {
            let node = &self.bvh.nodes[node_id as usize];
            if node.bmin[2] > high || node.bmax[2] < low {
                continue;
            }
            if node.leaf {
                let start = node.a as usize;
                let count = node.b as usize;
                for &face_id in &self.bvh.order[start..start + count] {
                    let i = face_id as usize;
                    if self.mesh.tri_min(i)[2] <= high && self.mesh.tri_max(i)[2] >= low {
                        visit(face_id);
                    }
                }
            } else {
                traversal_stack.push(node.a);
                traversal_stack.push(node.b);
            }
        }
    }

    fn slice_one_z(&self, z: f64) -> Vec<PlaneIntersection> {
        let candidates = self.z_candidates(z);
        let mut basic = Vec::new();
        let mut vertex = Vec::new();
        let mut edge = Vec::new();
        for face_id in candidates {
            let i = face_id as usize;
            let vertices = [
                self.mesh.v0(i),
                self.mesh.source_vertex_b(i),
                self.mesh.source_vertex_c(i),
            ];
            if let Some(event) = intersect_triangle_z(face_id, vertices, z) {
                match event.kind {
                    PlaneIntersectionKind::Basic => basic.push(event),
                    PlaneIntersectionKind::Vertex => vertex.push(event),
                    PlaneIntersectionKind::PositiveEdge => edge.push(event),
                }
            }
        }
        basic.extend(vertex);
        basic.extend(edge);
        basic
    }
}

fn sign(distance: f64) -> i8 {
    if distance < -PLANE_SLICE_MERGE_TOLERANCE_MM {
        -1
    } else if distance > PLANE_SLICE_MERGE_TOLERANCE_MM {
        1
    } else {
        0
    }
}

fn intersect_triangle_z(face_id: u32, vertices: [V3; 3], z: f64) -> Option<PlaneIntersection> {
    let signs = vertices.map(|vertex| sign(vertex[2] - z));
    let negatives = signs.iter().filter(|&&value| value == -1).count();
    let zeros = signs.iter().filter(|&&value| value == 0).count();
    let positives = 3 - negatives - zeros;

    if zeros == 0 && negatives > 0 && positives > 0 {
        let unique_sign = if negatives == 1 { -1 } else { 1 };
        let unique = signs
            .iter()
            .position(|&value| value == unique_sign)
            .unwrap();
        let next = (unique + 1) % 3;
        let previous = (unique + 2) % 3;
        return Some(PlaneIntersection {
            face_id,
            kind: PlaneIntersectionKind::Basic,
            segment: [
                trimesh_line_plane(vertices[unique], vertices[next], z),
                trimesh_line_plane(vertices[unique], vertices[previous], z),
            ],
        });
    }
    if zeros == 1 && negatives == 1 && positives == 1 {
        let on = signs.iter().position(|&value| value == 0).unwrap();
        let mut off = [0usize; 2];
        let mut next = 0;
        for (index, &value) in signs.iter().enumerate() {
            if value != 0 {
                off[next] = index;
                next += 1;
            }
        }
        return Some(PlaneIntersection {
            face_id,
            kind: PlaneIntersectionKind::Vertex,
            segment: [
                vertices[on],
                trimesh_line_plane(vertices[off[0]], vertices[off[1]], z),
            ],
        });
    }
    if zeros == 2 && positives == 1 {
        let mut on = [0usize; 2];
        let mut next = 0;
        for (index, &value) in signs.iter().enumerate() {
            if value == 0 {
                on[next] = index;
                next += 1;
            }
        }
        return Some(PlaneIntersection {
            face_id,
            kind: PlaneIntersectionKind::PositiveEdge,
            segment: [vertices[on[0]], vertices[on[1]]],
        });
    }
    None
}

/// Preserve the frozen `plane_lines(..., line_segments=False)` arithmetic:
/// unitize the line, divide plane distance by its unit-Z denominator, then
/// advance from the first endpoint along the unit direction.
fn trimesh_line_plane(a: V3, b: V3, z: f64) -> V3 {
    let direction = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let length =
        (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2])
            .sqrt();
    let unit = [
        direction[0] / length,
        direction[1] / length,
        direction[2] / length,
    ];
    let distance = (z - a[2]) / unit[2];
    [
        a[0] + distance * unit[0],
        a[1] + distance * unit[1],
        a[2] + distance * unit[2],
    ]
}
