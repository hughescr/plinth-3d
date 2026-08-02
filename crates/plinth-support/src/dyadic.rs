//! Shared deterministic 1-to-4 triangle sampling.
//!
//! Coverage and candidate seats retain different payloads, but they refine
//! through the same bounded leaf traversal. `perceive_contacts` intentionally
//! remains on its accepted centroid path until a later integration explicitly
//! adopts [`build_candidate_payload`].

use std::cmp::Ordering;

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

    /// Crate-private authority material for binding a perception plan to the
    /// exact dyadic request that produced its candidate evidence.
    pub(crate) const fn authority_fields(&self) -> (u32, usize, usize, usize, usize, u64) {
        (
            self.max_refine_depth,
            self.limits.max_virtual_cells,
            self.max_candidates,
            self.max_compact_payload_bytes,
            self.limits.max_working_cells,
            self.cell_edge_limit_mm.to_bits(),
        )
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

/// Shared execution request.  Both payload adapters supply their policy here;
/// the generic builder owns the two passes and all leaf/window mechanics.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DyadicBuildConfig {
    pub limits: DyadicLimits,
    pub cell_edge_limit_mm: f64,
    pub max_refine_depth: Option<u32>,
}

/// Aggregate facts established before any leaf payload is emitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DyadicPlan {
    pub total_cells: usize,
    pub source_faces: usize,
    pub working_cells: usize,
    pub max_refine_depth: u32,
    pub mesh_faces: usize,
    pub mesh_vertices: usize,
}

/// Immutable context for one leaf.  Policies only choose/map rows; they never
/// own traversal or windowing.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DyadicLeaf<'a, M> {
    pub island_id: u32,
    pub face_id: u32,
    pub leaf: [[f64; 3]; 3],
    pub leaf_area_mm2: f64,
    pub face_metadata: &'a M,
}

/// One source face as seen by the policy hook.  Candidate policy elects to
/// derive a normal; coverage deliberately accepts positive-edge collinear
/// faces and retains their zero-area samples.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DyadicFace {
    pub island_id: u32,
    pub face_id: u32,
    pub cell: [[f64; 3]; 3],
}

/// The generic skeleton performs the nomination loop.  Candidate policy can
/// retain its frozen z-only mask; coverage requests the one centroid row.
#[derive(Clone, Copy, Debug)]
pub(crate) enum DyadicNominations {
    Vertices([bool; 3]),
    Centroid,
}

/// Payload-specific decisions permitted around the shared dyadic skeleton.
/// Planning, checked aggregate preflight sequencing, traversal, bounded
/// scatter, and both sort/dedup passes belong to [`build_rows`].
pub(crate) trait DyadicRowPolicy {
    type Row;
    type Metadata;
    type FaceMetadata;
    type Error;

    fn prepare_face(&self, face: DyadicFace) -> Result<Self::FaceMetadata, Self::Error>;
    fn check_aggregate(&self, plan: DyadicPlan) -> Result<(), Self::Error>;
    fn finish_preflight(&self, plan: DyadicPlan) -> Result<Self::Metadata, Self::Error>;
    fn row_capacity(&self, metadata: &Self::Metadata) -> usize;
    fn nominations(&self, leaf: &DyadicLeaf<'_, Self::FaceMetadata>) -> DyadicNominations;
    fn map_row(&self, leaf: &DyadicLeaf<'_, Self::FaceMetadata>, point_mm: [f64; 3]) -> Self::Row;
    fn dedup_order(&self, left: &Self::Row, right: &Self::Row) -> Ordering;
    fn same_row(&self, left: &mut Self::Row, right: &mut Self::Row) -> bool;
    fn output_order(&self, left: &Self::Row, right: &Self::Row) -> Ordering;
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DyadicBuildError<E> {
    InvalidLimits,
    Overflow,
    VirtualCellLimit {
        required: usize,
        limit: usize,
    },
    RefinementDepthLimit {
        face_id: u32,
        required: u32,
        limit: u32,
    },
    FaceOutOfRange {
        island_id: u32,
        face_id: u32,
    },
    DegenerateFace {
        face_id: u32,
    },
    Policy(E),
}

pub(crate) struct DyadicPreflight<M> {
    pub plan: DyadicPlan,
    pub metadata: M,
}

pub(crate) struct DyadicRows<Row, M> {
    pub rows: Vec<Row>,
    pub plan: DyadicPlan,
    pub metadata: M,
    pub high_water: usize,
}

pub(crate) type DyadicRowsResult<P> = Result<
    DyadicRows<<P as DyadicRowPolicy>::Row, <P as DyadicRowPolicy>::Metadata>,
    DyadicBuildError<<P as DyadicRowPolicy>::Error>,
>;

fn checked_aggregate<P: DyadicRowPolicy>(
    plan: DyadicPlan,
    config: DyadicBuildConfig,
    policy: &P,
) -> Result<(), DyadicBuildError<P::Error>> {
    if plan.total_cells > config.limits.max_virtual_cells {
        return Err(DyadicBuildError::VirtualCellLimit {
            required: plan.total_cells,
            limit: config.limits.max_virtual_cells,
        });
    }
    policy
        .check_aggregate(plan)
        .map_err(DyadicBuildError::Policy)
}

fn aggregate_plan(
    source_faces: usize,
    total_cells: usize,
    max_refine_depth: u32,
    mesh: &Mesh,
    config: DyadicBuildConfig,
) -> DyadicPlan {
    DyadicPlan {
        total_cells,
        source_faces,
        working_cells: total_cells.min(config.limits.max_working_cells),
        max_refine_depth,
        mesh_faces: mesh.tris().len(),
        mesh_vertices: mesh.verts().len(),
    }
}

fn source_cell(
    mesh: &Mesh,
    island_id: u32,
    face_id: u32,
) -> Result<DyadicFace, DyadicBuildError<()>> {
    let face_index = usize::try_from(face_id)
        .map_err(|_| DyadicBuildError::FaceOutOfRange { island_id, face_id })?;
    let triangle = mesh
        .tris()
        .get(face_index)
        .ok_or(DyadicBuildError::FaceOutOfRange { island_id, face_id })?;
    let cell = triangle.map(|vertex| {
        mesh.verts()
            .get(vertex as usize)
            .copied()
            .unwrap_or([f64::NAN; 3])
    });
    Ok(DyadicFace {
        island_id,
        face_id,
        cell,
    })
}

fn map_face_error<E>(error: DyadicBuildError<()>) -> DyadicBuildError<E> {
    match error {
        DyadicBuildError::FaceOutOfRange { island_id, face_id } => {
            DyadicBuildError::FaceOutOfRange { island_id, face_id }
        }
        _ => DyadicBuildError::Overflow,
    }
}

/// First pass of the single generic dyadic builder.  It checks a one-cell per
/// face lower bound before allocating any plan storage (there is none), then
/// invokes monotonic aggregate checks as each face contributes its true depth.
pub(crate) fn preflight<P: DyadicRowPolicy>(
    mesh: &Mesh,
    islands: &[SupportIsland],
    config: DyadicBuildConfig,
    policy: &P,
) -> Result<DyadicPreflight<P::Metadata>, DyadicBuildError<P::Error>> {
    if config.limits.max_virtual_cells == 0 || config.limits.max_working_cells == 0 {
        return Err(DyadicBuildError::InvalidLimits);
    }
    let source_faces = islands.iter().try_fold(0_usize, |sum, island| {
        sum.checked_add(island.face_ids.len())
            .ok_or(DyadicBuildError::Overflow)
    })?;
    checked_aggregate(
        aggregate_plan(source_faces, source_faces, 0, mesh, config),
        config,
        policy,
    )?;
    let mut total_cells = 0_usize;
    let mut max_refine_depth = 0_u32;
    for island in islands {
        for &face_id in &island.face_ids {
            let face = source_cell(mesh, island.id, face_id).map_err(map_face_error)?;
            #[cfg(test)]
            PREFLIGHT_FACE_HOOKS.with(|hooks| hooks.set(hooks.get() + 1));
            policy
                .prepare_face(face)
                .map_err(DyadicBuildError::Policy)?;
            let edge = max_edge_mm(face.cell);
            if !edge.is_finite() || edge <= 0.0 {
                return Err(DyadicBuildError::DegenerateFace { face_id });
            }
            let depth = refine_depth(edge, config.cell_edge_limit_mm);
            if let Some(limit) = config.max_refine_depth {
                if depth > limit {
                    return Err(DyadicBuildError::RefinementDepthLimit {
                        face_id,
                        required: depth,
                        limit,
                    });
                }
            }
            let cells = terminal_cells(depth).map_err(|_| DyadicBuildError::Overflow)?;
            total_cells = total_cells
                .checked_add(cells)
                .ok_or(DyadicBuildError::Overflow)?;
            max_refine_depth = max_refine_depth.max(depth);
            checked_aggregate(
                aggregate_plan(source_faces, total_cells, max_refine_depth, mesh, config),
                config,
                policy,
            )?;
        }
    }
    let plan = aggregate_plan(source_faces, total_cells, max_refine_depth, mesh, config);
    let metadata = policy
        .finish_preflight(plan)
        .map_err(DyadicBuildError::Policy)?;
    Ok(DyadicPreflight { plan, metadata })
}

/// Complete second pass of the one generic dyadic builder.  No adapter loops
/// over leaves, windows, or output ordering/deduplication.
pub(crate) fn build_rows<P: DyadicRowPolicy>(
    mesh: &Mesh,
    islands: &[SupportIsland],
    config: DyadicBuildConfig,
    policy: &P,
) -> DyadicRowsResult<P> {
    #[cfg(test)]
    GENERIC_DYADIC_INVOCATIONS.with(|calls| calls.borrow_mut().push(std::any::type_name::<P>()));

    let preflight = preflight(mesh, islands, config, policy)?;
    let mut rows = Vec::with_capacity(policy.row_capacity(&preflight.metadata));
    let mut high_water = 0;
    for island in islands {
        for &face_id in &island.face_ids {
            let face = source_cell(mesh, island.id, face_id).map_err(map_face_error)?;
            let face_metadata = policy
                .prepare_face(face)
                .map_err(DyadicBuildError::Policy)?;
            let edge = max_edge_mm(face.cell);
            if !edge.is_finite() || edge <= 0.0 {
                return Err(DyadicBuildError::DegenerateFace { face_id });
            }
            let depth = refine_depth(edge, config.cell_edge_limit_mm);
            let cells = terminal_cells(depth).map_err(|_| DyadicBuildError::Overflow)?;
            let limits = DyadicLimits {
                max_virtual_cells: cells,
                max_working_cells: config.limits.max_working_cells,
            };
            let seen = for_each_leaf(face.cell, depth, limits, |leaf| {
                let context = DyadicLeaf {
                    island_id: face.island_id,
                    face_id: face.face_id,
                    leaf,
                    leaf_area_mm2: triangle_area_mm2(face.cell) / cells as f64,
                    face_metadata: &face_metadata,
                };
                match policy.nominations(&context) {
                    DyadicNominations::Vertices(mask) => {
                        for (point_mm, nominated) in leaf.into_iter().zip(mask) {
                            if nominated {
                                rows.push(policy.map_row(&context, point_mm));
                            }
                        }
                    }
                    DyadicNominations::Centroid => {
                        let point_mm = [
                            (leaf[0][0] + leaf[1][0] + leaf[2][0]) / 3.0,
                            (leaf[0][1] + leaf[1][1] + leaf[2][1]) / 3.0,
                            (leaf[0][2] + leaf[1][2] + leaf[2][2]) / 3.0,
                        ];
                        rows.push(policy.map_row(&context, point_mm));
                    }
                }
            })
            .map_err(|error| match error {
                DyadicError::InvalidLimits => DyadicBuildError::InvalidLimits,
                DyadicError::Overflow => DyadicBuildError::Overflow,
                DyadicError::VirtualCellLimit { required, limit } => {
                    DyadicBuildError::VirtualCellLimit { required, limit }
                }
                _ => DyadicBuildError::Overflow,
            })?;
            high_water = high_water.max(seen);
        }
    }
    rows.sort_unstable_by(|left, right| policy.dedup_order(left, right));
    rows.dedup_by(|left, right| policy.same_row(left, right));
    rows.sort_unstable_by(|left, right| policy.output_order(left, right));
    Ok(DyadicRows {
        rows,
        plan: preflight.plan,
        metadata: preflight.metadata,
        high_water,
    })
}

#[cfg(test)]
thread_local! {
    static GENERIC_DYADIC_INVOCATIONS: std::cell::RefCell<Vec<&'static str>> = const { std::cell::RefCell::new(Vec::new()) };
    static PREFLIGHT_FACE_HOOKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn clear_generic_dyadic_test_observations() {
    GENERIC_DYADIC_INVOCATIONS.with(|calls| calls.borrow_mut().clear());
    PREFLIGHT_FACE_HOOKS.with(|hooks| hooks.set(0));
}

#[cfg(test)]
pub(crate) fn generic_dyadic_invocation_types() -> Vec<&'static str> {
    GENERIC_DYADIC_INVOCATIONS.with(|calls| calls.borrow().clone())
}

#[cfg(test)]
pub(crate) fn preflight_face_hook_count() -> usize {
    PREFLIGHT_FACE_HOOKS.with(std::cell::Cell::get)
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
    build_candidate_payload_with_high_water(mesh, islands, config).map(|(payload, _)| payload)
}

#[cfg(test)]
pub(crate) fn build_candidate_payload_with_high_water(
    mesh: &Mesh,
    islands: &[SupportIsland],
    config: &CandidatePayloadConfig,
) -> Result<(CandidatePayload, usize), DyadicError> {
    build_candidate_payload_inner(mesh, islands, config)
}

#[cfg(not(test))]
fn build_candidate_payload_with_high_water(
    mesh: &Mesh,
    islands: &[SupportIsland],
    config: &CandidatePayloadConfig,
) -> Result<(CandidatePayload, usize), DyadicError> {
    build_candidate_payload_inner(mesh, islands, config)
}

fn build_candidate_payload_inner(
    mesh: &Mesh,
    islands: &[SupportIsland],
    config: &CandidatePayloadConfig,
) -> Result<(CandidatePayload, usize), DyadicError> {
    let policy = CandidateRowPolicy { config };
    let built = build_rows(
        mesh,
        islands,
        DyadicBuildConfig {
            limits: config.limits,
            cell_edge_limit_mm: config.cell_edge_limit_mm,
            max_refine_depth: Some(config.max_refine_depth),
        },
        &policy,
    )
    .map_err(map_candidate_build_error)?;
    let compact_payload_bytes = candidate_payload_bytes(built.rows.len())?;
    debug_assert!(built.rows.len() <= config.max_candidates);
    debug_assert!(compact_payload_bytes <= config.max_compact_payload_bytes);
    Ok((
        CandidatePayload {
            rows: built.rows,
            virtual_cells: built.plan.total_cells,
            nomination_upper_bound: built.metadata.nominations,
            compact_payload_bytes,
            max_refine_depth: built.plan.max_refine_depth,
        },
        built.high_water,
    ))
}

#[derive(Clone, Copy)]
struct CandidatePreflight {
    nominations: usize,
}

struct CandidateRowPolicy<'a> {
    config: &'a CandidatePayloadConfig,
}

#[cfg(test)]
pub(crate) fn candidate_policy_type_name() -> &'static str {
    std::any::type_name::<CandidateRowPolicy<'static>>()
}

impl DyadicRowPolicy for CandidateRowPolicy<'_> {
    type Row = CandidatePayloadRow;
    type Metadata = CandidatePreflight;
    type FaceMetadata = [f64; 3];
    type Error = DyadicError;

    fn prepare_face(&self, face: DyadicFace) -> Result<Self::FaceMetadata, Self::Error> {
        face_normal(face.cell).ok_or(DyadicError::DegenerateFace {
            face_id: face.face_id,
        })
    }

    fn check_aggregate(&self, plan: DyadicPlan) -> Result<(), Self::Error> {
        let nominations = candidate_nominations(plan.total_cells)?;
        if nominations > self.config.max_candidates {
            return Err(DyadicError::CandidateLimit {
                required: nominations,
                limit: self.config.max_candidates,
            });
        }
        let bytes = candidate_payload_bytes(nominations)?;
        if bytes > self.config.max_compact_payload_bytes {
            return Err(DyadicError::CompactPayloadLimit {
                required: bytes,
                limit: self.config.max_compact_payload_bytes,
            });
        }
        Ok(())
    }

    fn finish_preflight(&self, plan: DyadicPlan) -> Result<Self::Metadata, Self::Error> {
        Ok(CandidatePreflight {
            nominations: candidate_nominations(plan.total_cells)?,
        })
    }

    fn row_capacity(&self, metadata: &Self::Metadata) -> usize {
        metadata.nominations
    }

    fn nominations(&self, leaf: &DyadicLeaf<'_, Self::FaceMetadata>) -> DyadicNominations {
        DyadicNominations::Vertices(co_lowest_mask(leaf.leaf))
    }

    fn map_row(&self, leaf: &DyadicLeaf<'_, Self::FaceMetadata>, point_mm: [f64; 3]) -> Self::Row {
        CandidatePayloadRow {
            island_id: leaf.island_id,
            face_id: leaf.face_id,
            point_mm,
            normal: *leaf.face_metadata,
        }
    }

    fn dedup_order(&self, left: &Self::Row, right: &Self::Row) -> Ordering {
        left.island_id
            .cmp(&right.island_id)
            .then_with(|| left.point_mm[0].total_cmp(&right.point_mm[0]))
            .then_with(|| left.point_mm[1].total_cmp(&right.point_mm[1]))
            .then_with(|| left.point_mm[2].total_cmp(&right.point_mm[2]))
            .then_with(|| left.normal[2].total_cmp(&right.normal[2]))
            .then_with(|| left.face_id.cmp(&right.face_id))
    }

    fn same_row(&self, left: &mut Self::Row, right: &mut Self::Row) -> bool {
        left.island_id == right.island_id && left.point_mm == right.point_mm
    }

    fn output_order(&self, left: &Self::Row, right: &Self::Row) -> Ordering {
        left.island_id
            .cmp(&right.island_id)
            .then_with(|| left.point_mm[2].total_cmp(&right.point_mm[2]))
            .then_with(|| left.point_mm[0].total_cmp(&right.point_mm[0]))
            .then_with(|| left.point_mm[1].total_cmp(&right.point_mm[1]))
            .then_with(|| left.face_id.cmp(&right.face_id))
    }
}

fn map_candidate_build_error(error: DyadicBuildError<DyadicError>) -> DyadicError {
    match error {
        DyadicBuildError::InvalidLimits => DyadicError::InvalidLimits,
        DyadicBuildError::Overflow => DyadicError::Overflow,
        DyadicBuildError::VirtualCellLimit { required, limit } => {
            DyadicError::VirtualCellLimit { required, limit }
        }
        DyadicBuildError::RefinementDepthLimit {
            face_id,
            required,
            limit,
        } => DyadicError::RefinementDepthLimit {
            face_id,
            required,
            limit,
        },
        DyadicBuildError::FaceOutOfRange { island_id, face_id } => {
            DyadicError::FaceOutOfRange { island_id, face_id }
        }
        DyadicBuildError::DegenerateFace { face_id } => DyadicError::DegenerateFace { face_id },
        DyadicBuildError::Policy(error) => error,
    }
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

fn triangle_area_mm2([a, b, c]: [[f64; 3]; 3]) -> f64 {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let cross = [
        u[1].mul_add(v[2], -u[2] * v[1]),
        u[2].mul_add(v[0], -u[0] * v[2]),
        u[0].mul_add(v[1], -u[1] * v[0]),
    ];
    0.5 * dist_sq(cross, [0.0; 3]).sqrt()
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

    fn two_triangles() -> Mesh {
        Mesh::new(
            vec![
                [0.0, 0.0, 0.0],
                [2.0, 0.0, 0.0],
                [0.0, 2.0, 2.0],
                [10.0, 0.0, 0.0],
                [12.0, 0.0, 0.0],
                [10.0, 2.0, 2.0],
            ],
            vec![[0, 1, 2], [3, 4, 5]],
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
    fn co_lowest_tolerance_pins_exact_boundary_and_next_above() {
        let exact = 8.0 * f64::EPSILON;
        assert_eq!(
            co_lowest_mask([[0.0, 0.0, 0.0], [0.0, 0.0, exact], [0.0, 0.0, exact]]),
            [true, true, true]
        );
        assert_eq!(
            co_lowest_mask([
                [0.0, 0.0, 0.0],
                [0.0, 0.0, exact],
                [0.0, 0.0, exact.next_up()],
            ]),
            [true, true, false]
        );
    }

    #[test]
    fn co_lowest_tolerance_pins_scale_two_boundary_and_next_above() {
        // The negative minimum fixes `max(1, abs(z))` at exactly 2.0 while
        // the other values remain inside that scale.  Replacing the frozen
        // scale with a constant 1.0 would reject the middle vertex.
        let tolerance = 8.0 * f64::EPSILON * 2.0;
        let boundary = -2.0 + tolerance;
        assert_eq!(
            co_lowest_mask([[0.0, 0.0, -2.0], [0.0, 0.0, boundary], [0.0, 0.0, boundary],]),
            [true, true, true]
        );
        assert_eq!(
            co_lowest_mask([
                [0.0, 0.0, -2.0],
                [0.0, 0.0, boundary],
                [0.0, 0.0, boundary.next_up()],
            ]),
            [true, true, false]
        );
    }

    #[test]
    fn shared_position_keeps_most_negative_nz_then_lowest_face_id_in_final_order() {
        let mesh = Mesh::new(
            vec![
                [0.0, 0.0, 0.0],
                [2.0, 0.0, 4.0],
                [0.0, 2.0, 1.0],
                [2.0, 0.0, 2.0],
                [0.0, 2.0, 1.0],
                [2.0, 0.0, 1.0],
                [0.0, 2.0, 2.0],
            ],
            // Every face owns [0, 0, 0]. Face 1 and face 2 have identical
            // nz; face 1 must win their face-id tie, ahead of face 0's less
            // negative nz.
            vec![[0, 2, 1], [0, 4, 3], [0, 6, 5]],
        )
        .expect("shared-position fixture");
        let payload = build_candidate_payload(
            &mesh,
            &[SupportIsland {
                id: 9,
                face_ids: vec![2, 0, 1],
            }],
            &config(8, 1_000_000, 3_000_000, 192 * 1024 * 1024, 64),
        )
        .expect("shared-position candidate payload");
        let shared: Vec<_> = payload
            .rows()
            .iter()
            .filter(|row| row.point_mm == [0.0, 0.0, 0.0])
            .collect();
        assert_eq!(shared.len(), 1);
        assert_eq!(shared[0].face_id, 1);
        assert_eq!(shared[0].normal, [2.0 / 3.0, 1.0 / 3.0, -2.0 / 3.0]);
        // Final output is `(island, z, x, y, face)`, so the shared lowest
        // point must be the first retained row as well as the dedup winner.
        assert_eq!(payload.rows()[0], *shared[0]);
    }

    #[test]
    fn multi_face_candidate_vector_pins_order_dedup_and_literal_64_byte_accounting() {
        const FROZEN_ROW_BYTES: usize = 64;
        assert_eq!(
            2 * std::mem::size_of::<[f64; 3]>() + 2 * std::mem::size_of::<u64>(),
            FROZEN_ROW_BYTES
        );
        let payload = build_candidate_payload(
            &two_triangles(),
            &[SupportIsland {
                id: 9,
                // Face zero repeats: generic dedup must retain its lower face
                // identity while preserving the other face's complete rows.
                face_ids: vec![1, 0, 0],
            }],
            &config(1, 12, 36, 36 * FROZEN_ROW_BYTES, 4),
        )
        .expect("multi-face candidate payload");
        let normal = [0.0, -0.707_106_781_186_547_5, 0.707_106_781_186_547_5];
        assert_eq!(
            payload.rows(),
            &[
                CandidatePayloadRow {
                    island_id: 9,
                    face_id: 0,
                    point_mm: [0.0, 0.0, 0.0],
                    normal
                },
                CandidatePayloadRow {
                    island_id: 9,
                    face_id: 0,
                    point_mm: [1.0, 0.0, 0.0],
                    normal
                },
                CandidatePayloadRow {
                    island_id: 9,
                    face_id: 0,
                    point_mm: [2.0, 0.0, 0.0],
                    normal
                },
                CandidatePayloadRow {
                    island_id: 9,
                    face_id: 1,
                    point_mm: [10.0, 0.0, 0.0],
                    normal
                },
                CandidatePayloadRow {
                    island_id: 9,
                    face_id: 1,
                    point_mm: [11.0, 0.0, 0.0],
                    normal
                },
                CandidatePayloadRow {
                    island_id: 9,
                    face_id: 1,
                    point_mm: [12.0, 0.0, 0.0],
                    normal
                },
                CandidatePayloadRow {
                    island_id: 9,
                    face_id: 0,
                    point_mm: [0.0, 1.0, 1.0],
                    normal
                },
                CandidatePayloadRow {
                    island_id: 9,
                    face_id: 0,
                    point_mm: [1.0, 1.0, 1.0],
                    normal
                },
                CandidatePayloadRow {
                    island_id: 9,
                    face_id: 1,
                    point_mm: [10.0, 1.0, 1.0],
                    normal
                },
                CandidatePayloadRow {
                    island_id: 9,
                    face_id: 1,
                    point_mm: [11.0, 1.0, 1.0],
                    normal
                },
            ]
        );
        assert_eq!(payload.compact_payload_bytes, 10 * FROZEN_ROW_BYTES);
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
    fn candidate_working_window_one_is_exact_and_zero_is_refused() {
        let island = [SupportIsland {
            id: 9,
            face_ids: vec![0],
        }];
        let (_, high_water) = build_candidate_payload_with_high_water(
            &triangle(),
            &island,
            &config(1, 4, 12, 12 * 64, 1),
        )
        .expect("one-cell candidate window");
        assert_eq!(high_water, 1);
        assert!(matches!(
            CandidatePayloadConfig::current(1, 4, 12, 12 * 64, 0),
            Err(DyadicError::InvalidLimits)
        ));
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
