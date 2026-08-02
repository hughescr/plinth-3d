//! Shared deterministic 1-to-4 triangle sampling.
//!
//! Coverage and candidate seats retain different payloads, but they refine
//! through the same bounded leaf traversal. `perceive_contacts` intentionally
//! remains on its accepted centroid path until a later integration explicitly
//! adopts [`build_candidate_payload`].

use plinth_core::m0_constants;
use plinth_mesh::Mesh;
use thiserror::Error;

use crate::SupportIsland;

const NOMINATIONS_PER_CELL: usize = 3;
const CANDIDATE_PAYLOAD_ROW_BYTES: usize =
    2 * std::mem::size_of::<[f64; 3]>() + 2 * std::mem::size_of::<u64>();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DyadicLimits {
    pub max_virtual_cells: usize,
    pub max_working_cells: usize,
}

/// Explicit resource request plus registered result-changing geometry.
#[derive(Clone, Copy, Debug)]
pub struct CandidatePayloadConfig {
    limits: DyadicLimits,
    max_refine_depth: u32,
    max_candidates: usize,
    max_compact_payload_bytes: usize,
    cell_edge_limit_mm: f64,
}

impl CandidatePayloadConfig {
    /// Binds the cell-edge law to M0 while keeping implementation caps as
    /// explicit caller-owned requests, matching the existing coverage API.
    pub fn current(
        max_refine_depth: u32,
        max_virtual_cells: usize,
        max_candidates: usize,
        max_compact_payload_bytes: usize,
        max_working_cells: usize,
    ) -> Result<Self, DyadicError> {
        if max_virtual_cells == 0
            || max_candidates == 0
            || max_compact_payload_bytes == 0
            || max_working_cells == 0
        {
            return Err(DyadicError::InvalidLimits);
        }
        let constants = m0_constants()
            .map_err(|error| DyadicError::RegisteredConstantsUnavailable(error.to_string()))?;
        let policy = constants.placeholders();
        let target = policy.target_spacing_mm().diagnostic().get();
        let coverage_factor = policy.coverage_radius_factor().diagnostic().get();
        let cell_edge_limit_mm = target * (coverage_factor - 1.0);
        if !(cell_edge_limit_mm.is_finite() && cell_edge_limit_mm > 0.0) {
            return Err(DyadicError::InvalidRegisteredGeometry);
        }
        Ok(Self {
            limits: DyadicLimits {
                max_virtual_cells,
                max_working_cells,
            },
            max_refine_depth,
            max_candidates,
            max_compact_payload_bytes,
            cell_edge_limit_mm,
        })
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum DyadicError {
    #[error("dyadic resource request must be positive")]
    InvalidLimits,
    #[error("registered M0 constants are unavailable: {0}")]
    RegisteredConstantsUnavailable(String),
    #[error("registered candidate cell-edge geometry is invalid")]
    InvalidRegisteredGeometry,
    #[error("dyadic resource arithmetic overflowed")]
    Overflow,
    #[error("dyadic sampling requires {required} virtual cells; limit is {limit}")]
    VirtualCellLimit { required: usize, limit: usize },
    #[error("candidate face {face_id} requires refinement depth {required}; limit is {limit}")]
    RefinementDepthLimit {
        face_id: u32,
        required: u32,
        limit: u32,
    },
    #[error("candidate payload requires {required} nomination rows; limit is {limit}")]
    CandidateLimit { required: usize, limit: usize },
    #[error("candidate compact payload requires {required} bytes; limit is {limit}")]
    CompactPayloadLimit { required: usize, limit: usize },
    #[error("candidate island {island_id} names face {face_id} outside the mesh")]
    FaceOutOfRange { island_id: u32, face_id: u32 },
    #[error("candidate source face {face_id} is degenerate or arithmetically invalid")]
    DegenerateFace { face_id: u32 },
}

/// Frozen 64-byte SOA row projected into a typed Rust value. Refinement depth
/// is plan metadata, not retained row payload in Python.
#[derive(Clone, Debug, PartialEq)]
pub struct CandidatePayloadRow {
    pub island_id: u32,
    pub face_id: u32,
    pub point_mm: [f64; 3],
    pub normal: [f64; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct CandidatePayload {
    rows: Vec<CandidatePayloadRow>,
    pub virtual_cells: usize,
    pub nomination_upper_bound: usize,
    pub compact_payload_bytes: usize,
    pub max_refine_depth: u32,
}

impl CandidatePayload {
    #[must_use]
    pub fn rows(&self) -> &[CandidatePayloadRow] {
        &self.rows
    }
}

/// Frozen `placement._refine_depths` scalar rule, including the one-ULP edge
/// correction. Both typed wrappers call this one implementation.
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

/// Streams leaves in frozen midpoint order in bounded windows. Callers gather
/// payload inputs, invoke this for each admitted face, then scatter rows. No
/// native query is made per point.
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
        let end = start.saturating_add(limits.max_working_cells).min(total);
        for index in start..end {
            emit(cell_at_index(cell, depth, index));
        }
        start = end;
    }
    Ok(total.min(limits.max_working_cells))
}

/// Additive, non-authoritative candidate-pool-shaped wrapper from frozen
/// `placement.candidate_pool`. Global upper bounds are checked before any
/// nomination payload is built. [`crate::perceive_contacts`] does not consume
/// this payload; it remains on its accepted centroid path until a later
/// integration explicitly adopts this wrapper.
pub fn build_candidate_payload(
    mesh: &Mesh,
    islands: &[SupportIsland],
    config: &CandidatePayloadConfig,
) -> Result<CandidatePayload, DyadicError> {
    let source_faces = islands.iter().try_fold(0_usize, |sum, island| {
        sum.checked_add(island.face_ids.len())
            .ok_or(DyadicError::Overflow)
    })?;
    preflight_candidate_resources(source_faces, config)?;

    let mut planned = Vec::with_capacity(source_faces);
    let mut total_cells = 0_usize;
    let mut max_depth = 0_u32;
    for island in islands {
        for &face_id in &island.face_ids {
            let face_index = usize::try_from(face_id).map_err(|_| DyadicError::FaceOutOfRange {
                island_id: island.id,
                face_id,
            })?;
            let face = mesh
                .tris()
                .get(face_index)
                .ok_or(DyadicError::FaceOutOfRange {
                    island_id: island.id,
                    face_id,
                })?;
            let cell = [
                mesh.verts()[face[0] as usize],
                mesh.verts()[face[1] as usize],
                mesh.verts()[face[2] as usize],
            ];
            face_normal(cell).ok_or(DyadicError::DegenerateFace { face_id })?;
            let edge = max_edge_mm(cell);
            if !edge.is_finite() || edge <= 0.0 {
                return Err(DyadicError::DegenerateFace { face_id });
            }
            let depth = refine_depth(edge, config.cell_edge_limit_mm);
            if depth > config.max_refine_depth {
                return Err(DyadicError::RefinementDepthLimit {
                    face_id,
                    required: depth,
                    limit: config.max_refine_depth,
                });
            }
            let cells = terminal_cells(depth)?;
            total_cells = total_cells
                .checked_add(cells)
                .ok_or(DyadicError::Overflow)?;
            preflight_candidate_resources(total_cells, config)?;
            max_depth = max_depth.max(depth);
            planned.push((island.id, face_id, cell, depth));
        }
    }

    let nomination_upper_bound = candidate_nominations(total_cells)?;
    let mut nominated = Vec::with_capacity(nomination_upper_bound);
    for (island_id, face_id, cell, depth) in planned {
        let normal = face_normal(cell).ok_or(DyadicError::DegenerateFace { face_id })?;
        let local_limits = DyadicLimits {
            max_virtual_cells: terminal_cells(depth)?,
            ..config.limits
        };
        for_each_leaf(cell, depth, local_limits, |leaf| {
            let nominate = co_lowest_mask(leaf);
            for (point, should_emit) in leaf.into_iter().zip(nominate) {
                if !should_emit {
                    continue;
                }
                nominated.push(CandidatePayloadRow {
                    island_id,
                    face_id,
                    point_mm: point,
                    normal,
                });
            }
        })?;
    }
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
    let compact_payload_bytes = candidate_payload_bytes(nominated.len())?;
    debug_assert!(nominated.len() <= config.max_candidates);
    debug_assert!(compact_payload_bytes <= config.max_compact_payload_bytes);
    Ok(CandidatePayload {
        rows: nominated,
        virtual_cells: total_cells,
        nomination_upper_bound,
        compact_payload_bytes,
        max_refine_depth: max_depth,
    })
}

fn preflight_candidate_resources(
    cells: usize,
    config: &CandidatePayloadConfig,
) -> Result<(), DyadicError> {
    if cells > config.limits.max_virtual_cells {
        return Err(DyadicError::VirtualCellLimit {
            required: cells,
            limit: config.limits.max_virtual_cells,
        });
    }
    let nominations = candidate_nominations(cells)?;
    if nominations > config.max_candidates {
        return Err(DyadicError::CandidateLimit {
            required: nominations,
            limit: config.max_candidates,
        });
    }
    let bytes = candidate_payload_bytes(nominations)?;
    if bytes > config.max_compact_payload_bytes {
        return Err(DyadicError::CompactPayloadLimit {
            required: bytes,
            limit: config.max_compact_payload_bytes,
        });
    }
    Ok(())
}

fn candidate_nominations(cells: usize) -> Result<usize, DyadicError> {
    cells
        .checked_mul(NOMINATIONS_PER_CELL)
        .ok_or(DyadicError::Overflow)
}

fn candidate_payload_bytes(rows: usize) -> Result<usize, DyadicError> {
    rows.checked_mul(CANDIDATE_PAYLOAD_ROW_BYTES)
        .ok_or(DyadicError::Overflow)
}

fn co_lowest_mask(leaf: [[f64; 3]; 3]) -> [bool; 3] {
    let z = [leaf[0][2], leaf[1][2], leaf[2][2]];
    let min_z = z.into_iter().min_by(f64::total_cmp).unwrap_or(0.0);
    let scale = z.into_iter().map(f64::abs).fold(1.0_f64, f64::max);
    let tolerance = 8.0 * f64::EPSILON * scale;
    z.map(|value| (value - min_z).abs() <= tolerance)
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

/// Shared exact edge-length reduction for all dyadic planning paths.
#[must_use]
pub(crate) fn max_edge_mm([a, b, c]: [[f64; 3]; 3]) -> f64 {
    dist_sq(a, b).max(dist_sq(b, c)).max(dist_sq(c, a)).sqrt()
}

fn dist_sq(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).mul_add(
        a[0] - b[0],
        (a[1] - b[1]).mul_add(a[1] - b[1], (a[2] - b[2]) * (a[2] - b[2])),
    )
}

fn face_normal([a, b, c]: [[f64; 3]; 3]) -> Option<[f64; 3]> {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        u[1].mul_add(v[2], -u[2] * v[1]),
        u[2].mul_add(v[0], -u[0] * v[2]),
        u[0].mul_add(v[1], -u[1] * v[0]),
    ];
    let length = dist_sq(n, [0.0; 3]).sqrt();
    (length.is_finite() && length > 0.0).then(|| [n[0] / length, n[1] / length, n[2] / length])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle() -> Mesh {
        Mesh::new(
            vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 2.0]],
            vec![[0, 1, 2]],
        )
        .expect("finite fixture")
    }

    fn config(
        depth: u32,
        cells: usize,
        candidates: usize,
        bytes: usize,
        work: usize,
    ) -> CandidatePayloadConfig {
        CandidatePayloadConfig::current(depth, cells, candidates, bytes, work).expect("M0 fixture")
    }

    #[test]
    fn depth_one_candidate_payload_matches_full_frozen_row_order() {
        let payload = build_candidate_payload(
            &triangle(),
            &[SupportIsland {
                id: 9,
                face_ids: vec![0],
            }],
            &config(1, 4, 12, 12 * CANDIDATE_PAYLOAD_ROW_BYTES, 4),
        )
        .expect("depth-one payload");
        assert_eq!(payload.virtual_cells, 4);
        assert_eq!(payload.nomination_upper_bound, 12);
        assert_eq!(payload.max_refine_depth, 1);
        assert_eq!(
            payload.compact_payload_bytes,
            5 * CANDIDATE_PAYLOAD_ROW_BYTES
        );
        let points: Vec<_> = payload.rows().iter().map(|row| row.point_mm).collect();
        assert_eq!(
            points,
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [2.0, 0.0, 0.0],
                [0.0, 1.0, 1.0],
                [1.0, 1.0, 1.0],
            ]
        );
        assert!(payload
            .rows()
            .iter()
            .all(|row| row.island_id == 9 && row.face_id == 0));
        assert!(payload
            .rows()
            .iter()
            .all(|row| row.normal == [0.0, -0.707_106_781_186_547_5, 0.707_106_781_186_547_5]));
    }

    #[test]
    fn co_lowest_tolerance_uses_only_z_scale() {
        let leaf = [
            [1.0e16, 0.0, 0.0],
            [1.0e16 + 2.0, 0.0, 1.0e-12],
            [1.0e16, 2.0, 2.0e-12],
        ];
        assert_eq!(co_lowest_mask(leaf), [true, false, false]);
    }

    #[test]
    fn every_candidate_preflight_cap_has_exact_and_cap_minus_one_refusal() {
        let island = [SupportIsland {
            id: 9,
            face_ids: vec![0],
        }];
        let upper_bytes = 12 * CANDIDATE_PAYLOAD_ROW_BYTES;
        assert!(
            build_candidate_payload(&triangle(), &island, &config(1, 4, 12, upper_bytes, 1))
                .is_ok()
        );
        assert_eq!(
            build_candidate_payload(&triangle(), &island, &config(0, 4, 12, upper_bytes, 1)),
            Err(DyadicError::RefinementDepthLimit {
                face_id: 0,
                required: 1,
                limit: 0
            })
        );
        assert_eq!(
            build_candidate_payload(&triangle(), &island, &config(1, 3, 12, upper_bytes, 1)),
            Err(DyadicError::VirtualCellLimit {
                required: 4,
                limit: 3
            })
        );
        assert_eq!(
            build_candidate_payload(&triangle(), &island, &config(1, 4, 11, upper_bytes, 1)),
            Err(DyadicError::CandidateLimit {
                required: 12,
                limit: 11
            })
        );
        assert_eq!(
            build_candidate_payload(&triangle(), &island, &config(1, 4, 12, upper_bytes - 1, 1)),
            Err(DyadicError::CompactPayloadLimit {
                required: upper_bytes,
                limit: upper_bytes - 1
            })
        );
    }

    #[test]
    fn serial_window_size_is_deterministic_without_claiming_thread_parallelism() {
        let island = [SupportIsland {
            id: 9,
            face_ids: vec![0],
        }];
        let signatures: Vec<_> = [1, 2, 4]
            .into_iter()
            .map(|work| {
                build_candidate_payload(
                    &triangle(),
                    &island,
                    &config(1, 4, 12, 12 * CANDIDATE_PAYLOAD_ROW_BYTES, work),
                )
                .unwrap()
                .rows
            })
            .collect();
        assert_eq!(signatures[0], signatures[1]);
        assert_eq!(signatures[0], signatures[2]);
    }

    #[test]
    fn degenerate_and_resource_overflow_are_typed() {
        let mesh = Mesh::new(vec![[0.0; 3]], vec![[0, 0, 0]]).expect("raw soup admits degeneracy");
        assert_eq!(
            build_candidate_payload(
                &mesh,
                &[SupportIsland {
                    id: 0,
                    face_ids: vec![0]
                }],
                &config(0, 1, 3, 3 * CANDIDATE_PAYLOAD_ROW_BYTES, 1)
            ),
            Err(DyadicError::DegenerateFace { face_id: 0 })
        );
        assert_eq!(terminal_cells(u32::MAX), Err(DyadicError::Overflow));
        assert_eq!(
            candidate_nominations(usize::MAX),
            Err(DyadicError::Overflow)
        );
        assert_eq!(
            candidate_payload_bytes(usize::MAX),
            Err(DyadicError::Overflow)
        );
    }
}
