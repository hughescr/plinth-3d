//! Shared, deterministic 1-to-4 triangle sampling.
//!
//! This is deliberately payload-agnostic: coverage and the future legal-seat
//! pool differ only in the rows they retain.  `perceive_contacts` remains on
//! its accepted centroid path until the later perception-integration task
//! elects to consume [`build_candidate_payload`].

use plinth_mesh::Mesh;
use thiserror::Error;

use crate::SupportIsland;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DyadicLimits {
    pub max_virtual_cells: usize,
    pub max_working_cells: usize,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum DyadicError {
    #[error("dyadic resource request must be positive")]
    InvalidLimits,
    #[error("dyadic arithmetic overflowed")]
    Overflow,
    #[error("dyadic sampling requires {required} virtual cells; limit is {limit}")]
    VirtualCellLimit { required: usize, limit: usize },
}

/// A future S0 legal-seat payload.  It is intentionally separate from the
/// current `ContactCandidatePool`: no current selection behavior changes by
/// constructing it.
#[derive(Clone, Debug, PartialEq)]
pub struct CandidatePayloadRow {
    pub island_id: u32,
    pub face_id: u32,
    pub point_mm: [f64; 3],
    pub normal: [f64; 3],
    pub refine_depth: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CandidatePayload {
    rows: Vec<CandidatePayloadRow>,
    pub virtual_cells: usize,
}

impl CandidatePayload {
    #[must_use]
    pub fn rows(&self) -> &[CandidatePayloadRow] {
        &self.rows
    }
}

/// Frozen `placement._refine_depths` scalar rule, including the one-ULP edge
/// correction.  Both typed wrappers call this one implementation.
#[must_use]
pub fn refine_depth(max_edge_mm: f64, cell_edge_limit_mm: f64) -> u32 {
    if max_edge_mm <= cell_edge_limit_mm {
        return 0;
    }
    let mut depth = (max_edge_mm / cell_edge_limit_mm).log2().ceil() as u32;
    if depth > 0 {
        let parent = cell_edge_limit_mm * 2_f64.powi((depth - 1) as i32);
        if max_edge_mm <= parent.next_up() {
            depth -= 1;
        }
    }
    let bound = cell_edge_limit_mm * 2_f64.powi(depth as i32);
    if max_edge_mm > bound.next_up() {
        depth += 1;
    }
    depth
}

pub(crate) fn terminal_cells(depth: u32) -> Result<usize, DyadicError> {
    4_usize.checked_pow(depth).ok_or(DyadicError::Overflow)
}

/// Streams leaves in the frozen midpoint order in bounded windows.  Callers
/// gather their payload inputs, invoke this once per planned face, and scatter
/// rows; no native/kernel query is performed per point.
pub(crate) fn for_each_leaf(
    cell: [[f64; 3]; 3],
    depth: u32,
    limits: DyadicLimits,
    mut emit: impl FnMut([[f64; 3]; 3]),
) -> Result<usize, DyadicError> {
    if limits.max_working_cells == 0 {
        return Err(DyadicError::InvalidLimits);
    }
    let total = terminal_cells(depth)?;
    if total > limits.max_virtual_cells {
        return Err(DyadicError::VirtualCellLimit {
            required: total,
            limit: limits.max_virtual_cells,
        });
    }
    let mut start = 0;
    while start < total {
        let end = (start + limits.max_working_cells).min(total);
        for index in start..end {
            emit(cell_at_index(cell, depth, index));
        }
        start = end;
    }
    Ok(total.min(limits.max_working_cells))
}

/// Candidate-pool-shaped wrapper from frozen `placement.candidate_pool`.
/// It is additive until perception explicitly adopts it.
pub fn build_candidate_payload(
    mesh: &Mesh,
    islands: &[SupportIsland],
    cell_edge_limit_mm: f64,
    limits: DyadicLimits,
) -> Result<CandidatePayload, DyadicError> {
    if !(cell_edge_limit_mm.is_finite() && cell_edge_limit_mm > 0.0)
        || limits.max_virtual_cells == 0
        || limits.max_working_cells == 0
    {
        return Err(DyadicError::InvalidLimits);
    }
    let mut planned = Vec::new();
    let mut total = 0usize;
    for island in islands {
        for &face_id in &island.face_ids {
            let face = mesh.tris()[face_id as usize];
            let cell = [
                mesh.verts()[face[0] as usize],
                mesh.verts()[face[1] as usize],
                mesh.verts()[face[2] as usize],
            ];
            let depth = refine_depth(max_edge(cell), cell_edge_limit_mm);
            let cells = terminal_cells(depth)?;
            total = total.checked_add(cells).ok_or(DyadicError::Overflow)?;
            if total > limits.max_virtual_cells {
                return Err(DyadicError::VirtualCellLimit {
                    required: total,
                    limit: limits.max_virtual_cells,
                });
            }
            planned.push((island.id, face_id, cell, depth));
        }
    }
    let mut nominated = Vec::new();
    for (island_id, face_id, cell, depth) in planned {
        let normal = face_normal(cell);
        let local_limits = DyadicLimits {
            max_virtual_cells: terminal_cells(depth)?,
            ..limits
        };
        for_each_leaf(cell, depth, local_limits, |leaf| {
            let min_z = leaf
                .iter()
                .map(|p| p[2])
                .min_by(f64::total_cmp)
                .unwrap_or(0.0);
            let scale = leaf
                .iter()
                .flat_map(|p| p.iter())
                .map(|x| x.abs())
                .fold(1.0_f64, f64::max);
            let tolerance = 8.0 * f64::EPSILON * scale;
            for point in leaf
                .into_iter()
                .filter(|p| (p[2] - min_z).abs() <= tolerance)
            {
                nominated.push(CandidatePayloadRow {
                    island_id,
                    face_id,
                    point_mm: point,
                    normal,
                    refine_depth: depth,
                });
            }
        })?;
    }
    // Python ownership: most-negative normal-z, then source identity.  The
    // public result uses its frozen z/x/y/face canonical order.
    nominated.sort_unstable_by(|a, b| {
        a.island_id
            .cmp(&b.island_id)
            .then_with(|| a.point_mm[0].total_cmp(&b.point_mm[0]))
            .then_with(|| a.point_mm[1].total_cmp(&b.point_mm[1]))
            .then_with(|| a.point_mm[2].total_cmp(&b.point_mm[2]))
            .then_with(|| a.normal[2].total_cmp(&b.normal[2]))
            .then_with(|| a.face_id.cmp(&b.face_id))
    });
    nominated.dedup_by(|a, b| a.island_id == b.island_id && a.point_mm == b.point_mm);
    nominated.sort_unstable_by(|a, b| {
        a.island_id
            .cmp(&b.island_id)
            .then_with(|| a.point_mm[2].total_cmp(&b.point_mm[2]))
            .then_with(|| a.point_mm[0].total_cmp(&b.point_mm[0]))
            .then_with(|| a.point_mm[1].total_cmp(&b.point_mm[1]))
            .then_with(|| a.face_id.cmp(&b.face_id))
    });
    Ok(CandidatePayload {
        rows: nominated,
        virtual_cells: total,
    })
}

pub(crate) fn cell_at_index(mut cell: [[f64; 3]; 3], depth: u32, index: usize) -> [[f64; 3]; 3] {
    for level in (0..depth).rev() {
        let [a, b, c] = cell;
        let ab = midpoint(a, b);
        let bc = midpoint(b, c);
        let ca = midpoint(c, a);
        cell = match (index >> (2 * level)) & 3 {
            0 => [a, ab, ca],
            1 => [ab, b, bc],
            2 => [ca, bc, c],
            _ => [ab, bc, ca],
        };
    }
    cell
}
fn midpoint(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        (a[0] + b[0]) * 0.5,
        (a[1] + b[1]) * 0.5,
        (a[2] + b[2]) * 0.5,
    ]
}
fn max_edge([a, b, c]: [[f64; 3]; 3]) -> f64 {
    dist(a, b).max(dist(b, c)).max(dist(c, a)).sqrt()
}
fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).mul_add(
        a[0] - b[0],
        (a[1] - b[1]).mul_add(a[1] - b[1], (a[2] - b[2]) * (a[2] - b[2])),
    )
}
fn face_normal([a, b, c]: [[f64; 3]; 3]) -> [f64; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let l = dist(n, [0.0; 3]).sqrt();
    if l == 0.0 {
        [0.0; 3]
    } else {
        [n[0] / l, n[1] / l, n[2] / l]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle() -> Mesh {
        Mesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 1.0]],
            vec![[0, 1, 2]],
        )
        .expect("finite fixture")
    }

    #[test]
    fn candidate_payload_matches_the_frozen_small_triangle_vector() {
        let rows = build_candidate_payload(
            &triangle(),
            &[SupportIsland {
                id: 9,
                face_ids: vec![0],
            }],
            2.0,
            DyadicLimits {
                max_virtual_cells: 1,
                max_working_cells: 1,
            },
        )
        .expect("depth-zero payload");
        assert_eq!(rows.virtual_cells, 1);
        assert_eq!(rows.rows().len(), 2);
        assert_eq!(rows.rows()[0].island_id, 9);
        assert_eq!(rows.rows()[0].face_id, 0);
        assert_eq!(rows.rows()[0].point_mm, [0.0, 0.0, 0.0]);
        assert_eq!(
            rows.rows()[0].normal,
            [0.0, -0.707_106_781_186_547_5, 0.707_106_781_186_547_5]
        );
    }

    #[test]
    fn exact_cap_and_cap_minus_one_are_named_and_window_invariant() {
        let island = [SupportIsland {
            id: 9,
            face_ids: vec![0],
        }];
        for effective_threads in [1, 4, 16] {
            let payload = build_candidate_payload(
                &triangle(),
                &island,
                1.0,
                DyadicLimits {
                    max_virtual_cells: 4,
                    max_working_cells: effective_threads,
                },
            )
            .expect("exact cap");
            assert_eq!(payload.virtual_cells, 4);
            assert_eq!(payload.rows().len(), 5);
        }
        assert_eq!(
            build_candidate_payload(
                &triangle(),
                &island,
                1.0,
                DyadicLimits {
                    max_virtual_cells: 3,
                    max_working_cells: 1
                }
            ),
            Err(DyadicError::VirtualCellLimit {
                required: 4,
                limit: 3
            }),
        );
    }

    #[test]
    fn overflow_and_degenerate_inputs_refuse_without_allocation() {
        assert_eq!(terminal_cells(u32::MAX), Err(DyadicError::Overflow));
        assert_eq!(
            build_candidate_payload(
                &triangle(),
                &[],
                0.0,
                DyadicLimits {
                    max_virtual_cells: 1,
                    max_working_cells: 1
                }
            ),
            Err(DyadicError::InvalidLimits),
        );
    }
}
