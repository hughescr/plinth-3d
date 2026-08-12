//! H1b: one-batch drain/vent drilling and proof of the resulting openings.
//!
//! This stage consumes H1a's exact geometry.  It does not move, resize, or
//! repair a planned hole and it never exposes the undrilled shell as output.

use std::collections::{HashSet, VecDeque};

use geom_kernel::{NativeSurface, NativeSurfaceConfig, V3};
use plinth_core::{m0_constants, Finite};
use plinth_mesh::{
    build_face_topology, certify_closed_components, validate_face_topology, Mesh, WeldPolicy,
};
use plinth_orient::OrientationHistory;
use plinth_solid::{BooleanInputEstimate, BooleanLimits, ManifoldBackend, Mesh as SolidMesh};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::drains::{DrainPlan, HoleGeometry, HoleRole, PlannedShell};
use crate::{
    bounds, from_solid, marching_cavity, orient_boolean_cavity, to_solid, CavityEvidence,
    CavityField, CavityRecord, DrainRecord, HollowDisposition, HollowHistory, HollowReport,
    HollowedModel,
};

// Registered as `HOLLOW_EXTRA_DRAIN_POLICY.cylinder_sides` (currently 24).
// Kept as a compile-time const rather than a live read: it sizes the
// `DEFAULT_MAX_CUTTER_VERTICES`/`DEFAULT_MAX_CUTTER_TRIANGLES` const
// expressions below, which need a `usize` available at const-eval time, not
// a runtime registry lookup. `cylinder_sections_matches_the_registered_
// hollow_extra_drain_policy` pins this value against the registry so a
// registry change is loud here instead of silently diverging.
const CYLINDER_SECTIONS: usize = 24;
const PROOF_EPSILON_MM: f64 = 1.0e-7;
const AUDIT_DIRECTIONS: [V3; 5] = [
    [0.0, 0.0, -1.0],
    [0.5, 0.0, -1.0],
    [-0.5, 0.0, -1.0],
    [0.0, 0.5, -1.0],
    [0.0, -0.5, -1.0],
];
const DEFAULT_MAX_BOOLEAN_PRIMITIVE_UNITS: u64 = 256 * 1024 * 1024;
const DEFAULT_MAX_BOOLEAN_MARSHALLED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const DEFAULT_MAX_BOOLEAN_OUTPUT_VERTICES: u64 = 64 * 1024 * 1024;
const DEFAULT_MAX_BOOLEAN_OUTPUT_TRIANGLES: u64 = 384 * 1024 * 1024;
const DEFAULT_MAX_HOLES: usize = 4096;
const DEFAULT_MAX_CUTTER_VERTICES: usize = 4096 * (2 * CYLINDER_SECTIONS + 2);
const DEFAULT_MAX_CUTTER_TRIANGLES: usize = 4096 * 4 * CYLINDER_SECTIONS;
const DEFAULT_MAX_TOPOLOGY_EDGE_OCCURRENCES: usize = 24 * 1024 * 1024;
const DEFAULT_MAX_CENTERLINE_RESULTS: usize = 24 * 1024 * 1024;
const DEFAULT_MAX_DRAINABILITY_CELLS: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrillingLimits {
    boolean: BooleanLimits,
    max_holes: usize,
    max_cutter_vertices: usize,
    max_cutter_triangles: usize,
    max_topology_edge_occurrences: usize,
    max_centerline_queries: usize,
    max_centerline_results: usize,
    max_drainability_cells: usize,
}

impl DrillingLimits {
    pub fn new(boolean: BooleanLimits) -> Result<Self, DrillingRefusal> {
        if boolean.max_input_primitive_units == 0
            || boolean.max_input_primitive_units == u64::MAX
            || boolean.max_logical_marshaled_input_bytes == 0
            || boolean.max_logical_marshaled_input_bytes == u64::MAX
            || boolean.max_output_vertices == 0
            || boolean.max_output_vertices == u64::MAX
            || boolean.max_output_triangles == 0
            || boolean.max_output_triangles == u64::MAX
        {
            return Err(DrillingRefusal::InvalidLimits);
        }
        Ok(Self {
            boolean,
            max_holes: DEFAULT_MAX_HOLES,
            max_cutter_vertices: DEFAULT_MAX_CUTTER_VERTICES,
            max_cutter_triangles: DEFAULT_MAX_CUTTER_TRIANGLES,
            max_topology_edge_occurrences: DEFAULT_MAX_TOPOLOGY_EDGE_OCCURRENCES,
            max_centerline_queries: DEFAULT_MAX_HOLES,
            max_centerline_results: DEFAULT_MAX_CENTERLINE_RESULTS,
            max_drainability_cells: DEFAULT_MAX_DRAINABILITY_CELLS,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_count_admissions(
        boolean: BooleanLimits,
        max_holes: usize,
        max_cutter_vertices: usize,
        max_cutter_triangles: usize,
        max_topology_edge_occurrences: usize,
        max_centerline_queries: usize,
        max_centerline_results: usize,
        max_drainability_cells: usize,
    ) -> Result<Self, DrillingRefusal> {
        let mut limits = Self::new(boolean)?;
        if [
            max_holes,
            max_cutter_vertices,
            max_cutter_triangles,
            max_topology_edge_occurrences,
            max_centerline_queries,
            max_centerline_results,
            max_drainability_cells,
        ]
        .iter()
        .any(|limit| *limit == 0 || *limit == usize::MAX)
        {
            return Err(DrillingRefusal::InvalidLimits);
        }
        limits.max_holes = max_holes;
        limits.max_cutter_vertices = max_cutter_vertices;
        limits.max_cutter_triangles = max_cutter_triangles;
        limits.max_topology_edge_occurrences = max_topology_edge_occurrences;
        limits.max_centerline_queries = max_centerline_queries;
        limits.max_centerline_results = max_centerline_results;
        limits.max_drainability_cells = max_drainability_cells;
        Ok(limits)
    }

    #[must_use]
    pub const fn production() -> Self {
        Self {
            boolean: BooleanLimits {
                max_input_primitive_units: DEFAULT_MAX_BOOLEAN_PRIMITIVE_UNITS,
                max_logical_marshaled_input_bytes: DEFAULT_MAX_BOOLEAN_MARSHALLED_BYTES,
                max_output_vertices: DEFAULT_MAX_BOOLEAN_OUTPUT_VERTICES,
                max_output_triangles: DEFAULT_MAX_BOOLEAN_OUTPUT_TRIANGLES,
            },
            max_holes: DEFAULT_MAX_HOLES,
            max_cutter_vertices: DEFAULT_MAX_CUTTER_VERTICES,
            max_cutter_triangles: DEFAULT_MAX_CUTTER_TRIANGLES,
            max_topology_edge_occurrences: DEFAULT_MAX_TOPOLOGY_EDGE_OCCURRENCES,
            max_centerline_queries: DEFAULT_MAX_HOLES,
            max_centerline_results: DEFAULT_MAX_CENTERLINE_RESULTS,
            max_drainability_cells: DEFAULT_MAX_DRAINABILITY_CELLS,
        }
    }

    #[must_use]
    pub const fn boolean(self) -> BooleanLimits {
        self.boolean
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrilledHoleEvidence {
    source: DrilledHoleSource,
    role: HoleRole,
    lobe_id: Option<u32>,
    center_mm: [Finite; 3],
    inner_crossing_mm: Finite,
    sampled_exit_mm: Finite,
    exact_outer_exit_mm: [Finite; 3],
    radius_mm: Finite,
    clear_centerline_crossings: usize,
}

impl DrilledHoleEvidence {
    #[must_use]
    pub const fn source(&self) -> DrilledHoleSource {
        self.source
    }
    #[must_use]
    pub const fn role(&self) -> HoleRole {
        self.role
    }
    #[must_use]
    pub const fn lobe_id(&self) -> Option<u32> {
        self.lobe_id
    }
    #[must_use]
    pub const fn center_mm(&self) -> [Finite; 3] {
        self.center_mm
    }
    #[must_use]
    pub const fn inner_crossing_mm(&self) -> Finite {
        self.inner_crossing_mm
    }
    #[must_use]
    pub const fn sampled_exit_mm(&self) -> Finite {
        self.sampled_exit_mm
    }
    #[must_use]
    pub const fn exact_outer_exit_mm(&self) -> [Finite; 3] {
        self.exact_outer_exit_mm
    }
    #[must_use]
    pub const fn radius_mm(&self) -> Finite {
        self.radius_mm
    }
    #[must_use]
    pub const fn clear_centerline_crossings(&self) -> usize {
        self.clear_centerline_crossings
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrilledHoleSource {
    Planned,
    Audit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrillingEvidence {
    holes: Vec<DrilledHoleEvidence>,
    cavity_voxels: usize,
    drainable_voxels: usize,
    audit_added_drains: usize,
    refilled_voxels: usize,
    refilled_mm3: Finite,
    refilled_pockets: Vec<RefilledPocketEvidence>,
    source_material_mm3: Finite,
    drilled_material_mm3: Finite,
    removed_material_mm3: Finite,
    source_bounds_mm: [[Finite; 3]; 2],
    drilled_bounds_mm: [[Finite; 3]; 2],
    bounds_delta_mm: [[Finite; 3]; 2],
    expected_material_mm3: Finite,
    volume_balance_residual_mm3: Finite,
    volume_balance_tolerance_mm3: Finite,
    final_cavity_mm3: Finite,
    boolean_input_primitive_units: u64,
    boolean_marshaled_input_bytes: u64,
}

impl DrillingEvidence {
    #[must_use]
    pub fn holes(&self) -> &[DrilledHoleEvidence] {
        &self.holes
    }
    #[must_use]
    pub const fn cavity_voxels(&self) -> usize {
        self.cavity_voxels
    }
    #[must_use]
    pub const fn drainable_voxels(&self) -> usize {
        self.drainable_voxels
    }
    #[must_use]
    pub const fn audit_added_drains(&self) -> usize {
        self.audit_added_drains
    }
    #[must_use]
    pub const fn refilled_voxels(&self) -> usize {
        self.refilled_voxels
    }
    #[must_use]
    pub const fn refilled_mm3(&self) -> Finite {
        self.refilled_mm3
    }
    #[must_use]
    pub fn refilled_pockets(&self) -> &[RefilledPocketEvidence] {
        &self.refilled_pockets
    }
    #[must_use]
    pub const fn source_material_mm3(&self) -> Finite {
        self.source_material_mm3
    }
    #[must_use]
    pub const fn drilled_material_mm3(&self) -> Finite {
        self.drilled_material_mm3
    }
    #[must_use]
    pub const fn removed_material_mm3(&self) -> Finite {
        self.removed_material_mm3
    }
    #[must_use]
    pub const fn source_bounds_mm(&self) -> [[Finite; 3]; 2] {
        self.source_bounds_mm
    }
    #[must_use]
    pub const fn drilled_bounds_mm(&self) -> [[Finite; 3]; 2] {
        self.drilled_bounds_mm
    }
    #[must_use]
    pub const fn bounds_delta_mm(&self) -> [[Finite; 3]; 2] {
        self.bounds_delta_mm
    }
    #[must_use]
    pub const fn expected_material_mm3(&self) -> Finite {
        self.expected_material_mm3
    }
    #[must_use]
    pub const fn volume_balance_residual_mm3(&self) -> Finite {
        self.volume_balance_residual_mm3
    }
    #[must_use]
    pub const fn volume_balance_tolerance_mm3(&self) -> Finite {
        self.volume_balance_tolerance_mm3
    }
    #[must_use]
    pub const fn final_cavity_mm3(&self) -> Finite {
        self.final_cavity_mm3
    }
    #[must_use]
    pub const fn boolean_input_primitive_units(&self) -> u64 {
        self.boolean_input_primitive_units
    }
    #[must_use]
    pub const fn boolean_marshaled_input_bytes(&self) -> u64 {
        self.boolean_marshaled_input_bytes
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefilledPocketEvidence {
    voxel_count: usize,
    volume_ml: Finite,
}

impl RefilledPocketEvidence {
    #[must_use]
    pub const fn voxel_count(&self) -> usize {
        self.voxel_count
    }
    #[must_use]
    pub const fn volume_ml(&self) -> Finite {
        self.volume_ml
    }
}

struct AuditPlan {
    holes: Vec<HoleGeometry>,
    refill: Vec<bool>,
    retained_voxels: usize,
    refilled_voxels: usize,
    refilled_pockets: Vec<usize>,
}

#[derive(Clone, Copy, Debug)]
struct AuditCandidate {
    component_id: usize,
    direction_order: usize,
    origin: V3,
    direction: V3,
    sampled_mm: f64,
}

#[derive(Debug)]
pub struct HollowedAndDrilled {
    mesh: Mesh,
    cavity: CavityEvidence,
    cavity_field: CavityField,
    drain_plan: DrainPlan,
    evidence: DrillingEvidence,
}

impl HollowedAndDrilled {
    #[must_use]
    pub const fn mesh(&self) -> &Mesh {
        &self.mesh
    }
    #[must_use]
    pub const fn cavity(&self) -> &CavityEvidence {
        &self.cavity
    }
    #[must_use]
    pub const fn cavity_field(&self) -> &CavityField {
        &self.cavity_field
    }
    #[must_use]
    pub const fn drain_plan(&self) -> &DrainPlan {
        &self.drain_plan
    }
    #[must_use]
    pub const fn evidence(&self) -> &DrillingEvidence {
        &self.evidence
    }
    #[cfg(test)]
    fn with_test_diagnostics(mut self, count: usize) -> Self {
        self.drain_plan.append_test_diagnostics(count);
        self
    }
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        Mesh,
        CavityEvidence,
        CavityField,
        DrainPlan,
        DrillingEvidence,
    ) {
        (
            self.mesh,
            self.cavity,
            self.cavity_field,
            self.drain_plan,
            self.evidence,
        )
    }

    /// The only hollow-output handoff accepted by support planning. This
    /// consumes the drilling proof and constructs the matching disposition;
    /// an undrilled `PlannedShell` has no corresponding conversion.
    #[must_use]
    pub fn into_hollowed_model(self, orientation: OrientationHistory) -> HollowedModel {
        let Self {
            mesh,
            cavity,
            cavity_field,
            drain_plan,
            evidence,
        } = self;
        let drains: Vec<DrainRecord> = evidence
            .holes
            .iter()
            .filter(|hole| hole.role == HoleRole::Drain)
            .map(|hole| DrainRecord {
                cavity_id: 1,
                entrance_mm: hole.center_mm,
                exit_mm: hole.exact_outer_exit_mm,
                radius_mm: hole.radius_mm,
            })
            .collect();
        let drain_ids = (0..drains.len()).collect();
        let report = HollowReport {
            disposition: HollowDisposition::HollowedAndDrilled,
            cavities: vec![CavityRecord {
                cavity_id: 1,
                volume_ml: Finite::new(evidence.final_cavity_mm3.get() / 1000.0)
                    .expect("proved finite cavity volume"),
                drain_ids,
            }],
            drains,
            drilling_evidence: Some(evidence),
            drain_plan: Some(drain_plan),
            // The repair that made this cavity extractable, carried into the
            // record of the shell it produced. Nothing downstream could see it
            // before: it lived only in `H0ResourceDiagnostics`, which no
            // emitted artifact reads.
            cavity_repair: Some(cavity.resources().cavity_repair()),
            // FU-37: the wall this shell was built to. `CavityEvidence`
            // carries it in-process only; this is what makes it survive
            // serialization.
            wall: Some(cavity.wall_thickness_mm()),
            // FU-55: the realized minimum wall this shell measured before it
            // was ever allowed to reach drain planning or drilling.
            realized_wall: Some(cavity.realized_wall()),
        };
        HollowedModel::from_verified_hollow(
            mesh,
            HollowHistory::from_reports(orientation, report),
            cavity,
            cavity_field,
        )
    }
}

#[derive(Debug)]
pub struct DrillingFailure {
    planned: Box<PlannedShell>,
    reason: DrillingRefusal,
}

impl DrillingFailure {
    #[must_use]
    pub const fn reason(&self) -> &DrillingRefusal {
        &self.reason
    }
    #[must_use]
    pub fn into_planned(self) -> PlannedShell {
        *self.planned
    }

    #[must_use]
    pub fn into_reason(self) -> DrillingRefusal {
        self.reason
    }
}

#[derive(Clone, Debug, PartialEq, Error)]
pub enum DrillingRefusal {
    #[error("H1b requires finite non-zero Boolean limits")]
    InvalidLimits,
    #[error("planned hole geometry is non-finite or degenerate")]
    InvalidHoleGeometry,
    #[error("planned holes {first} and {second} overlap or are tangent")]
    AmbiguousHolePair { first: usize, second: usize },
    #[error("Boolean input preflight or operation failed")]
    Boolean,
    #[error("drilled mesh failed strict closed, wound, single-body validation")]
    InvalidDrilledTopology,
    #[error("drilling changed the model bounds")]
    BoundsChanged,
    #[error("drilling shrank model bound axis {axis} side {side} by {shrinkage_mm}mm, beyond {max_shrinkage_mm}mm")]
    ExcessiveBoundsShrinkage {
        axis: usize,
        side: usize,
        shrinkage_mm: f64,
        max_shrinkage_mm: f64,
    },
    #[error("drilling did not remove a finite positive material volume")]
    InvalidVolumeDelta,
    #[error("drilled volume balance error {error_mm3}mm3 is not below frozen tolerance {tolerance_mm3}mm3")]
    VolumeBalance { error_mm3: f64, tolerance_mm3: f64 },
    #[error("planned opening {hole} is not a clear cavity-to-exterior passage")]
    BlockedOpening { hole: usize },
    #[error("drilled topology proves a missed, merged, or extra opening")]
    OpeningCountMismatch,
    #[error("retained cavity contains {trapped_voxels} voxels that cannot flow to a drain")]
    TrappedCavity { trapped_voxels: usize },
    #[error("drainability proof requires at least one drain and one vent")]
    MissingRequiredRole,
    #[error("checked H1b arithmetic overflowed")]
    ResourceAccountingOverflow,
    #[error("M0 constants publication failed")]
    Constants,
    #[error("H1b {resource} count {requested} exceeds admitted {cap}")]
    CountAdmission {
        resource: &'static str,
        requested: usize,
        cap: usize,
    },
}

pub fn drill_planned_shell(planned: PlannedShell) -> Result<HollowedAndDrilled, DrillingFailure> {
    drill_planned_shell_with_limits(planned, DrillingLimits::production())
}

pub fn drill_planned_shell_with_limits(
    planned: PlannedShell,
    limits: DrillingLimits,
) -> Result<HollowedAndDrilled, DrillingFailure> {
    match drill(&planned, limits) {
        Ok((mesh, evidence, audit)) => {
            let (shell, cavity, mut cavity_field, drain_plan) = planned.into_parts();
            drop(shell);
            cavity_field
                .refill_excluded(&audit.refill)
                .expect("validated H1b refill mask");
            let cavity = cavity.with_cavity_volume_mm3(evidence.final_cavity_mm3);
            Ok(HollowedAndDrilled {
                mesh,
                cavity,
                cavity_field,
                drain_plan,
                evidence,
            })
        }
        Err(reason) => Err(DrillingFailure {
            planned: Box::new(planned),
            reason,
        }),
    }
}

fn drill(
    planned: &PlannedShell,
    limits: DrillingLimits,
) -> Result<(Mesh, DrillingEvidence, AuditPlan), DrillingRefusal> {
    admit_count("hole", planned.drain_plan().holes().len(), limits.max_holes)?;
    let audit = prepare_audit_plan(planned, limits)?;
    let total_holes = planned
        .drain_plan()
        .holes()
        .len()
        .checked_add(audit.holes.len())
        .ok_or(DrillingRefusal::ResourceAccountingOverflow)?;
    preflight_cutters(total_holes, limits)?;
    let holes: Vec<HoleGeometry> = planned
        .drain_plan()
        .holes()
        .iter()
        .cloned()
        .chain(audit.holes.iter().cloned())
        .collect();
    if !holes.iter().any(|h| h.role == HoleRole::Drain)
        || !holes.iter().any(|h| h.role == HoleRole::Vent)
    {
        return Err(DrillingRefusal::MissingRequiredRole);
    }
    validate_final_holes(&holes)?;
    let reconstructed = reconstructed_source_if_refilled(planned, &audit, limits)?;
    let post_refill_mesh =
        from_solid(reconstructed.clone()).map_err(|_| DrillingRefusal::Boolean)?;
    let post_refill_mesh = if audit.refilled_voxels == 0 {
        post_refill_mesh
    } else {
        orient_boolean_cavity(post_refill_mesh)
            .map_err(|_| DrillingRefusal::InvalidDrilledTopology)?
    };
    let base = if audit.refilled_voxels == 0 {
        reconstructed
    } else {
        to_solid(&post_refill_mesh).map_err(|_| DrillingRefusal::Boolean)?
    };
    let mut operands = Vec::with_capacity(holes.len() + 1);
    operands.push(base);
    let predicted_input = preflight_boolean_with_cutters(&operands, holes.len(), limits)?;
    for hole in &holes {
        operands.push(cylinder(hole)?);
    }
    let input =
        BooleanInputEstimate::for_meshes(&operands).map_err(|_| DrillingRefusal::Boolean)?;
    debug_assert_eq!(input, predicted_input);
    let boolean_limits = limits.boolean();
    if input.operand_primitive_units > boolean_limits.max_input_primitive_units
        || input.logical_marshaled_input_bytes > boolean_limits.max_logical_marshaled_input_bytes
    {
        return Err(DrillingRefusal::Boolean);
    }
    // Exactly one native Boolean invocation subtracts the gathered cutter batch.
    let drilled = ManifoldBackend
        .difference_limited(&operands, boolean_limits)
        .map_err(|_| DrillingRefusal::Boolean)?;
    let mesh = from_solid(drilled).map_err(|_| DrillingRefusal::Boolean)?;
    let (source_volume, source_components, source_chi) =
        topology_evidence(&post_refill_mesh, limits)?;
    let (drilled_volume, drilled_components, drilled_chi) = topology_evidence(&mesh, limits)?;
    if source_components != 2 || drilled_components != 1 {
        return Err(DrillingRefusal::InvalidDrilledTopology);
    }
    let expected_chi = source_chi
        .checked_sub(
            i64::try_from(holes.len()).map_err(|_| DrillingRefusal::ResourceAccountingOverflow)?
                * 2,
        )
        .ok_or(DrillingRefusal::ResourceAccountingOverflow)?;
    if drilled_chi != expected_chi {
        return Err(DrillingRefusal::OpeningCountMismatch);
    }
    let source_bounds = bounds(&post_refill_mesh).map_err(|_| DrillingRefusal::BoundsChanged)?;
    let drilled_bounds = bounds(&mesh).map_err(|_| DrillingRefusal::BoundsChanged)?;
    if !bounds_within_source(source_bounds, drilled_bounds) {
        return Err(DrillingRefusal::BoundsChanged);
    }
    validate_bounds_shrinkage(source_bounds, drilled_bounds, &holes)?;
    let final_cavity_mm3 =
        audit.retained_voxels as f64 * planned.cavity_field().pitch_mm().get().powi(3);
    let (expected_material, volume_balance_residual, volume_balance_tolerance) =
        validate_volume_balance(
            drilled_volume,
            planned.cavity().source_volume_mm3().get(),
            final_cavity_mm3,
        )?;
    let removed = source_volume - drilled_volume;
    if !removed.is_finite() || removed <= 0.0 {
        return Err(DrillingRefusal::InvalidVolumeDelta);
    }
    let crossings = prove_centerlines(
        &mesh,
        &holes,
        planned.cavity_field().pitch_mm().get(),
        limits,
    )?;
    let (members, flags) =
        drainability_map_excluding(planned.cavity_field(), &holes, limits, &audit.refill)?;
    let drainable = members
        .iter()
        .zip(flags)
        .filter(|(_, drains)| *drains)
        .count();
    if drainable != audit.retained_voxels {
        return Err(DrillingRefusal::TrappedCavity {
            trapped_voxels: audit.retained_voxels - drainable,
        });
    }
    let evidence = DrillingEvidence {
        holes: holes
            .iter()
            .zip(crossings)
            .enumerate()
            .map(
                |(index, (hole, clear_centerline_crossings))| DrilledHoleEvidence {
                    source: if index < planned.drain_plan().holes().len() {
                        DrilledHoleSource::Planned
                    } else {
                        DrilledHoleSource::Audit
                    },
                    role: hole.role,
                    lobe_id: hole.lobe_id,
                    center_mm: hole.center_mm,
                    inner_crossing_mm: hole.inner_crossing_mm,
                    sampled_exit_mm: hole.sampled_exit_mm,
                    exact_outer_exit_mm: hole.outer_exit_mm,
                    radius_mm: hole.radius_mm,
                    clear_centerline_crossings,
                },
            )
            .collect(),
        cavity_voxels: audit.retained_voxels,
        drainable_voxels: drainable,
        audit_added_drains: audit.holes.len(),
        refilled_voxels: audit.refilled_voxels,
        refilled_mm3: finite(
            audit.refilled_voxels as f64 * planned.cavity_field().pitch_mm().get().powi(3),
        )?,
        refilled_pockets: audit
            .refilled_pockets
            .iter()
            .map(|voxel_count| {
                Ok(RefilledPocketEvidence {
                    voxel_count: *voxel_count,
                    volume_ml: finite(
                        *voxel_count as f64 * planned.cavity_field().pitch_mm().get().powi(3)
                            / 1000.0,
                    )?,
                })
            })
            .collect::<Result<Vec<_>, DrillingRefusal>>()?,
        source_material_mm3: finite(source_volume)?,
        drilled_material_mm3: finite(drilled_volume)?,
        removed_material_mm3: finite(removed)?,
        source_bounds_mm: finite_bounds(source_bounds)?,
        drilled_bounds_mm: finite_bounds(drilled_bounds)?,
        bounds_delta_mm: finite_bounds((
            sub(drilled_bounds.0, source_bounds.0),
            sub(drilled_bounds.1, source_bounds.1),
        ))?,
        expected_material_mm3: finite(expected_material)?,
        volume_balance_residual_mm3: finite(volume_balance_residual)?,
        volume_balance_tolerance_mm3: finite(volume_balance_tolerance)?,
        final_cavity_mm3: finite(final_cavity_mm3)?,
        boolean_input_primitive_units: input.operand_primitive_units,
        boolean_marshaled_input_bytes: input.logical_marshaled_input_bytes,
    };
    Ok((mesh, evidence, audit))
}

fn reconstructed_source_if_refilled(
    planned: &PlannedShell,
    audit: &AuditPlan,
    limits: DrillingLimits,
) -> Result<SolidMesh, DrillingRefusal> {
    if audit.refilled_voxels == 0 {
        return to_solid(planned.shell()).map_err(|_| DrillingRefusal::Boolean);
    }
    admit_count(
        "drainability cell",
        planned.cavity_field().flat_count(),
        limits.max_drainability_cells,
    )?;
    let retained_mask = planned
        .cavity_field()
        .mask_excluding(&audit.refill)
        .map_err(|_| DrillingRefusal::Boolean)?;
    let retained_inner = marching_cavity(
        &retained_mask,
        planned.cavity_field().origin_mm().map(Finite::get),
        planned.cavity_field().pitch_mm().get(),
    )
    .map_err(|_| DrillingRefusal::Boolean)?;
    let operands = [
        outer_solid(planned.shell(), limits)?,
        to_solid(&retained_inner).map_err(|_| DrillingRefusal::Boolean)?,
    ];
    let input =
        BooleanInputEstimate::for_meshes(&operands).map_err(|_| DrillingRefusal::Boolean)?;
    if input.operand_primitive_units > limits.boolean().max_input_primitive_units
        || input.logical_marshaled_input_bytes > limits.boolean().max_logical_marshaled_input_bytes
    {
        return Err(DrillingRefusal::Boolean);
    }
    // Rebuild the actual post-refill, pre-drill shell in one H0-shaped
    // difference: the retained outer component minus the retained cavity.
    // This object is retained for topology, bounds, and volume evidence.
    ManifoldBackend
        .difference_limited(&operands, limits.boolean())
        .map_err(|_| DrillingRefusal::Boolean)
}

fn outer_solid(shell: &Mesh, limits: DrillingLimits) -> Result<SolidMesh, DrillingRefusal> {
    let edge_occurrences = shell
        .tris()
        .len()
        .checked_mul(3)
        .ok_or(DrillingRefusal::ResourceAccountingOverflow)?;
    admit_count(
        "topology edge occurrence",
        edge_occurrences,
        limits.max_topology_edge_occurrences,
    )?;
    let topology = build_face_topology(shell, WeldPolicy::ExactBits)
        .map_err(|_| DrillingRefusal::InvalidDrilledTopology)?;
    let certificate = certify_closed_components(shell, &topology)
        .map_err(|_| DrillingRefusal::InvalidDrilledTopology)?;
    let mut positive = certificate
        .components()
        .iter()
        .filter(|component| component.signed_volume_mm3() > 0.0);
    let outer = positive
        .next()
        .ok_or(DrillingRefusal::InvalidDrilledTopology)?
        .component_id();
    if positive.next().is_some() {
        return Err(DrillingRefusal::InvalidDrilledTopology);
    }
    let triangles = shell
        .tris()
        .iter()
        .zip(topology.component_ids())
        .filter_map(|(triangle, component)| {
            (*component == outer).then_some(triangle.map(u64::from))
        })
        .collect();
    SolidMesh::new(shell.verts().to_vec(), triangles).map_err(|_| DrillingRefusal::Boolean)
}

fn prepare_audit_plan(
    planned: &PlannedShell,
    limits: DrillingLimits,
) -> Result<AuditPlan, DrillingRefusal> {
    let drain_policy = m0_constants()
        .map_err(|_| DrillingRefusal::Constants)?
        .placeholders()
        .hollow_extra_drain_policy()
        .diagnostic();
    let audit_min_ml = drain_policy.min_pocket_ml();
    let audit_max_drains = usize::try_from(drain_policy.max_extra_drains().get())
        .map_err(|_| DrillingRefusal::ResourceAccountingOverflow)?;
    let audit_max_exit_mm = drain_policy.exit_max_mm();
    let field = planned.cavity_field();
    let planned_holes = planned.drain_plan().holes();
    let (members, flags) = drainability_map(field, planned_holes, limits)?;
    let mut trapped = vec![false; field.flat_count()];
    for (flat, drains) in members.iter().zip(flags) {
        trapped[*flat] = !drains;
    }
    let components = trapped_components(field, &trapped, limits)?;
    let qualifying_count = components
        .iter()
        .filter(|component| {
            qualifies_audit_drain(
                component.len(),
                field.pitch_mm().get(),
                0,
                audit_min_ml,
                audit_max_drains,
            )
        })
        .count();
    let candidate_upper_bound = qualifying_count
        .checked_mul(AUDIT_DIRECTIONS.len())
        .ok_or(DrillingRefusal::ResourceAccountingOverflow)?;
    admit_count(
        "audit centerline query",
        candidate_upper_bound,
        limits.max_centerline_queries,
    )?;
    let mut candidates = Vec::with_capacity(candidate_upper_bound);
    for (component_id, component) in components.iter().enumerate() {
        if qualifies_audit_drain(
            component.len(),
            field.pitch_mm().get(),
            0,
            audit_min_ml,
            audit_max_drains,
        ) {
            let origin = component_low_origin(field, component)?;
            for (direction_order, direction) in AUDIT_DIRECTIONS.into_iter().enumerate() {
                let direction = normalize(direction)?;
                if let Some(sampled_mm) =
                    sampled_exit_along(field, origin, direction, audit_max_exit_mm)
                {
                    if sampled_mm <= audit_max_exit_mm {
                        candidates.push(AuditCandidate {
                            component_id,
                            direction_order,
                            origin,
                            direction,
                            sampled_mm,
                        });
                    }
                }
            }
        }
    }
    let mut audit_holes = Vec::new();
    let mut audit_components = vec![false; components.len()];
    if !candidates.is_empty() {
        let result_upper_bound = candidates
            .len()
            .checked_mul(planned.shell().tris().len())
            .ok_or(DrillingRefusal::ResourceAccountingOverflow)?;
        admit_count(
            "audit centerline result upper bound",
            result_upper_bound,
            limits.max_centerline_results,
        )?;
        let estimate = NativeSurface::estimate_index_memory_from_indexed(
            planned.shell().verts(),
            planned.shell().tris(),
            field.pitch_mm().get(),
        )
        .map_err(|_| DrillingRefusal::InvalidDrilledTopology)?;
        let surface = NativeSurface::from_indexed(
            planned.shell().verts(),
            planned.shell().tris(),
            NativeSurfaceConfig {
                grid_pitch_mm: field.pitch_mm().get(),
                max_grid_bytes: estimate.max_axis_grid_bytes(),
            },
        )
        .map_err(|_| DrillingRefusal::InvalidDrilledTopology)?;
        let origins: Vec<V3> = candidates
            .iter()
            .map(|candidate| candidate.origin)
            .collect();
        let directions: Vec<V3> = candidates
            .iter()
            .map(|candidate| candidate.direction)
            .collect();
        let (offsets, ts, _) = surface
            .cast_rays(&origins, &directions, audit_max_exit_mm)
            .map_err(|_| DrillingRefusal::InvalidDrilledTopology)?;
        for (component_id, component_drilled) in audit_components.iter_mut().enumerate() {
            if audit_holes.len() == audit_max_drains {
                break;
            }
            let mut accepted_holes = planned_holes.to_vec();
            accepted_holes.extend(audit_holes.iter().cloned());
            let Some((_, hole)) = select_exact_audit_candidate(
                &candidates,
                &offsets,
                &ts,
                component_id,
                &accepted_holes,
            )?
            else {
                continue;
            };
            audit_holes.push(hole);
            *component_drilled = true;
        }
    }
    let mut refill = vec![false; field.flat_count()];
    for (component_id, component) in components.iter().enumerate() {
        if !audit_components[component_id] {
            for flat in component {
                refill[*flat] = true;
            }
        }
    }
    let mut all_holes = planned_holes.to_vec();
    all_holes.extend(audit_holes.iter().cloned());
    let (members, flags) = drainability_map_excluding(field, &all_holes, limits, &refill)?;
    for (flat, drains) in members.iter().zip(flags) {
        if !drains {
            refill[*flat] = true;
        }
    }
    let refilled_voxels = refill.iter().filter(|value| **value).count();
    let refilled_pockets = trapped_components(field, &refill, limits)?
        .into_iter()
        .map(|component| component.len())
        .collect();
    let retained_voxels = field
        .member_count()
        .checked_sub(refilled_voxels)
        .ok_or(DrillingRefusal::ResourceAccountingOverflow)?;
    if retained_voxels == 0 {
        return Err(DrillingRefusal::TrappedCavity {
            trapped_voxels: refilled_voxels,
        });
    }
    Ok(AuditPlan {
        holes: audit_holes,
        refill,
        retained_voxels,
        refilled_voxels,
        refilled_pockets,
    })
}

fn select_exact_audit_candidate(
    candidates: &[AuditCandidate],
    offsets: &[i64],
    ts: &[f64],
    component_id: usize,
    accepted_holes: &[HoleGeometry],
) -> Result<Option<(usize, HoleGeometry)>, DrillingRefusal> {
    if offsets.len() != candidates.len() + 1 {
        return Err(DrillingRefusal::ResourceAccountingOverflow);
    }
    let mut ordered: Vec<usize> = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| (candidate.component_id == component_id).then_some(index))
        .collect();
    ordered.sort_by(|left, right| {
        candidates[*left]
            .sampled_mm
            .total_cmp(&candidates[*right].sampled_mm)
            .then(
                candidates[*left]
                    .direction_order
                    .cmp(&candidates[*right].direction_order),
            )
    });
    for index in ordered {
        let begin = usize::try_from(offsets[index])
            .map_err(|_| DrillingRefusal::ResourceAccountingOverflow)?;
        let end = usize::try_from(offsets[index + 1])
            .map_err(|_| DrillingRefusal::ResourceAccountingOverflow)?;
        let Some(hits) = ts.get(begin..end) else {
            return Err(DrillingRefusal::ResourceAccountingOverflow);
        };
        if hits.len() != 2 || hits[0] <= 0.0 || hits[1] <= hits[0] {
            continue;
        }
        let candidate = &candidates[index];
        let Ok(hole) = audit_hole(
            candidate.origin,
            candidate.direction,
            hits[0],
            hits[1],
            candidate.sampled_mm,
        ) else {
            continue;
        };
        let mut final_holes = accepted_holes.to_vec();
        final_holes.push(hole.clone());
        if validate_final_holes(&final_holes).is_ok() {
            return Ok(Some((index, hole)));
        }
    }
    Ok(None)
}

fn qualifies_audit_drain(
    component_voxels: usize,
    pitch_mm: f64,
    added: usize,
    min_pocket_ml: f64,
    max_extra_drains: usize,
) -> bool {
    component_voxels as f64 * pitch_mm.powi(3) / 1000.0 >= min_pocket_ml && added < max_extra_drains
}

fn trapped_components(
    field: &CavityField,
    trapped: &[bool],
    limits: DrillingLimits,
) -> Result<Vec<Vec<usize>>, DrillingRefusal> {
    admit_count(
        "trapped component cell",
        trapped.len(),
        limits.max_drainability_cells,
    )?;
    let dims = field.dimensions();
    let mut seen = vec![false; trapped.len()];
    let mut components = Vec::new();
    for seed in 0..trapped.len() {
        if !trapped[seed] || seen[seed] {
            continue;
        }
        seen[seed] = true;
        let mut queue = VecDeque::new();
        queue.push_back(seed);
        let mut component = Vec::new();
        while let Some(flat) = queue.pop_front() {
            component.push(flat);
            let [x, y, z] = field.flat_to_index(flat);
            for [dx, dy, dz] in [
                [1, 0, 0],
                [-1, 0, 0],
                [0, 1, 0],
                [0, -1, 0],
                [0, 0, 1],
                [0, 0, -1],
            ] {
                if let Some(next) =
                    flat_if_in([x as isize + dx, y as isize + dy, z as isize + dz], dims)
                {
                    if trapped[next] && !seen[next] {
                        seen[next] = true;
                        queue.push_back(next);
                    }
                }
            }
        }
        components.push(component);
    }
    components.sort_by_key(|component| std::cmp::Reverse(component.len()));
    Ok(components)
}

fn component_low_origin(field: &CavityField, component: &[usize]) -> Result<V3, DrillingRefusal> {
    let min_z = component
        .iter()
        .map(|flat| field.world_at_flat(*flat)[2])
        .fold(f64::INFINITY, f64::min);
    let mut sum = [0.0; 3];
    let mut count = 0_usize;
    for flat in component {
        let world = field.world_at_flat(*flat);
        if world[2] < min_z + 1.0 {
            sum = add(sum, world);
            count += 1;
        }
    }
    if count == 0 {
        return Err(DrillingRefusal::InvalidHoleGeometry);
    }
    let mut origin = scale(sum, 1.0 / count as f64);
    origin[2] = min_z;
    Ok(origin)
}

fn sampled_exit_along(field: &CavityField, origin: V3, direction: V3, ceiling: f64) -> Option<f64> {
    let pitch = field.pitch_mm().get();
    let grid_origin = field.origin_mm().map(Finite::get);
    let dims = field.dimensions();
    let samples = (ceiling / pitch).floor() as usize + 1;
    (0..samples).find_map(|sample| {
        let distance = sample as f64 * pitch;
        let point = add(origin, scale(direction, distance));
        let index = grid_index_ties_even(point, grid_origin, pitch);
        let occupied = flat_if_in(index, dims).is_some_and(|flat| field.source_contains_flat(flat));
        (!occupied).then_some(distance)
    })
}

fn audit_hole(
    origin: V3,
    direction: V3,
    inner_mm: f64,
    outer_mm: f64,
    sampled_mm: f64,
) -> Result<HoleGeometry, DrillingRefusal> {
    let finite_point = |point: V3| {
        point
            .map(Finite::new)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| DrillingRefusal::InvalidHoleGeometry)?
            .try_into()
            .map_err(|_| DrillingRefusal::InvalidHoleGeometry)
    };
    Ok(HoleGeometry {
        role: HoleRole::Drain,
        lobe_id: None,
        center_mm: finite_point(origin)?,
        direction: finite_point(direction)?,
        outer_exit_mm: finite_point(add(origin, scale(direction, outer_mm)))?,
        radius_mm: Finite::new(1.5).map_err(|_| DrillingRefusal::InvalidHoleGeometry)?,
        inner_crossing_mm: Finite::new(inner_mm)
            .map_err(|_| DrillingRefusal::InvalidHoleGeometry)?,
        length_mm: Finite::new(outer_mm).map_err(|_| DrillingRefusal::InvalidHoleGeometry)?,
        sampled_exit_mm: Finite::new(sampled_mm)
            .map_err(|_| DrillingRefusal::InvalidHoleGeometry)?,
        keepout_start_mm: finite_point(add(origin, scale(direction, -4.0)))?,
        keepout_end_mm: finite_point(add(origin, scale(direction, sampled_mm + 4.0)))?,
    })
}

fn cylinder(hole: &HoleGeometry) -> Result<SolidMesh, DrillingRefusal> {
    let a = point(hole.keepout_start_mm);
    let b = point(hole.keepout_end_mm);
    let axis = sub(b, a);
    let length = norm(axis);
    let radius = hole.radius_mm.get();
    if !length.is_finite() || length <= 0.0 || !radius.is_finite() || radius <= 0.0 {
        return Err(DrillingRefusal::InvalidHoleGeometry);
    }
    let w = scale(axis, 1.0 / length);
    let reference = if w[0].abs() <= w[1].abs() && w[0].abs() <= w[2].abs() {
        [1.0, 0.0, 0.0]
    } else if w[1].abs() <= w[2].abs() {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let u = normalize(cross(reference, w))?;
    let v = cross(w, u);
    let mut vertices = Vec::with_capacity(2 * CYLINDER_SECTIONS + 2);
    for center in [a, b] {
        for section in 0..CYLINDER_SECTIONS {
            let angle = std::f64::consts::TAU * section as f64 / CYLINDER_SECTIONS as f64;
            vertices.push(add(
                center,
                scale(add(scale(u, angle.cos()), scale(v, angle.sin())), radius),
            ));
        }
    }
    let bottom = vertices.len() as u64;
    vertices.push(a);
    let top = vertices.len() as u64;
    vertices.push(b);
    let mut triangles = Vec::with_capacity(4 * CYLINDER_SECTIONS);
    for section in 0..CYLINDER_SECTIONS {
        let next = (section + 1) % CYLINDER_SECTIONS;
        let i = section as u64;
        let j = next as u64;
        let k = (section + CYLINDER_SECTIONS) as u64;
        let l = (next + CYLINDER_SECTIONS) as u64;
        triangles.push([i, j, l]);
        triangles.push([i, l, k]);
        triangles.push([bottom, j, i]);
        triangles.push([top, k, l]);
    }
    SolidMesh::new(vertices, triangles).map_err(|_| DrillingRefusal::InvalidHoleGeometry)
}

fn preflight_cutters(hole_count: usize, limits: DrillingLimits) -> Result<(), DrillingRefusal> {
    admit_count("hole", hole_count, limits.max_holes)?;
    let vertices = hole_count
        .checked_mul(2 * CYLINDER_SECTIONS + 2)
        .ok_or(DrillingRefusal::ResourceAccountingOverflow)?;
    let triangles = hole_count
        .checked_mul(4 * CYLINDER_SECTIONS)
        .ok_or(DrillingRefusal::ResourceAccountingOverflow)?;
    admit_count("cutter vertex", vertices, limits.max_cutter_vertices)?;
    admit_count("cutter triangle", triangles, limits.max_cutter_triangles)
}

fn preflight_boolean_with_cutters(
    existing_operands: &[SolidMesh],
    hole_count: usize,
    limits: DrillingLimits,
) -> Result<BooleanInputEstimate, DrillingRefusal> {
    let cutter_vertices = hole_count
        .checked_mul(2 * CYLINDER_SECTIONS + 2)
        .ok_or(DrillingRefusal::ResourceAccountingOverflow)?;
    let cutter_triangles = hole_count
        .checked_mul(4 * CYLINDER_SECTIONS)
        .ok_or(DrillingRefusal::ResourceAccountingOverflow)?;
    let vertices = existing_operands
        .iter()
        .try_fold(cutter_vertices, |total, mesh| {
            total.checked_add(mesh.vertices().len())
        })
        .ok_or(DrillingRefusal::ResourceAccountingOverflow)?;
    let triangles = existing_operands
        .iter()
        .try_fold(cutter_triangles, |total, mesh| {
            total.checked_add(mesh.triangles().len())
        })
        .ok_or(DrillingRefusal::ResourceAccountingOverflow)?;
    let operand_count = existing_operands
        .len()
        .checked_add(hole_count)
        .and_then(|count| count.checked_add(1))
        .ok_or(DrillingRefusal::ResourceAccountingOverflow)?;
    let vertex_scalars = u64::try_from(vertices)
        .ok()
        .and_then(|count| count.checked_mul(3))
        .ok_or(DrillingRefusal::ResourceAccountingOverflow)?;
    let triangle_indices = u64::try_from(triangles)
        .ok()
        .and_then(|count| count.checked_mul(3))
        .ok_or(DrillingRefusal::ResourceAccountingOverflow)?;
    let offsets =
        u64::try_from(operand_count).map_err(|_| DrillingRefusal::ResourceAccountingOverflow)?;
    let primitive_units = vertex_scalars
        .checked_add(triangle_indices)
        .and_then(|total| total.checked_add(offsets.checked_mul(2)?))
        .ok_or(DrillingRefusal::ResourceAccountingOverflow)?;
    let logical_marshaled_input_bytes = primitive_units
        .checked_mul(8)
        .ok_or(DrillingRefusal::ResourceAccountingOverflow)?;
    let estimate = BooleanInputEstimate {
        vertex_scalars,
        triangle_indices,
        vertex_offset_units: offsets,
        triangle_offset_units: offsets,
        operand_primitive_units: primitive_units,
        logical_marshaled_input_bytes,
    };
    if primitive_units > limits.boolean().max_input_primitive_units
        || logical_marshaled_input_bytes > limits.boolean().max_logical_marshaled_input_bytes
    {
        return Err(DrillingRefusal::Boolean);
    }
    Ok(estimate)
}

fn admit_count(
    resource: &'static str,
    requested: usize,
    cap: usize,
) -> Result<(), DrillingRefusal> {
    if requested > cap {
        Err(DrillingRefusal::CountAdmission {
            resource,
            requested,
            cap,
        })
    } else {
        Ok(())
    }
}

fn validate_final_holes(holes: &[HoleGeometry]) -> Result<(), DrillingRefusal> {
    for hole in holes {
        validate_frozen_hole_geometry(hole)?;
    }
    for (first, left) in holes.iter().enumerate() {
        for (second, right) in holes.iter().enumerate().skip(first + 1) {
            let distance = segment_distance(
                point(left.keepout_start_mm),
                point(left.keepout_end_mm),
                point(right.keepout_start_mm),
                point(right.keepout_end_mm),
            )?;
            if distance <= left.radius_mm.get() + right.radius_mm.get() + PROOF_EPSILON_MM {
                return Err(DrillingRefusal::AmbiguousHolePair { first, second });
            }
        }
    }
    Ok(())
}

fn validate_frozen_hole_geometry(hole: &HoleGeometry) -> Result<(), DrillingRefusal> {
    let center = point(hole.center_mm);
    let direction = point(hole.direction);
    let start = point(hole.keepout_start_mm);
    let end = point(hole.keepout_end_mm);
    let outer = point(hole.outer_exit_mm);
    let before = dot(sub(center, start), direction);
    let after = dot(sub(end, center), direction);
    let expected_after = hole.sampled_exit_mm.get()
        + match hole.role {
            HoleRole::Drain => 4.0,
            HoleRole::Vent => 5.0,
        };
    let close = |left: f64, right: f64| (left - right).abs() <= PROOF_EPSILON_MM;
    if !close(norm(direction), 1.0)
        || !close(before, 4.0)
        || !close(after, expected_after)
        || norm(cross(sub(center, start), direction)) > PROOF_EPSILON_MM
        || norm(cross(sub(end, center), direction)) > PROOF_EPSILON_MM
        || norm(sub(
            outer,
            add(center, scale(direction, hole.length_mm.get())),
        )) > PROOF_EPSILON_MM
        || hole.inner_crossing_mm.get() <= 0.0
        || hole.inner_crossing_mm.get() >= hole.length_mm.get()
        || hole.length_mm.get() > hole.sampled_exit_mm.get()
        || hole.radius_mm.get() <= 0.0
    {
        return Err(DrillingRefusal::InvalidHoleGeometry);
    }
    Ok(())
}

fn topology_evidence(
    mesh: &Mesh,
    limits: DrillingLimits,
) -> Result<(f64, usize, i64), DrillingRefusal> {
    let edge_occurrences = mesh
        .tris()
        .len()
        .checked_mul(3)
        .ok_or(DrillingRefusal::ResourceAccountingOverflow)?;
    admit_count(
        "topology edge occurrence",
        edge_occurrences,
        limits.max_topology_edge_occurrences,
    )?;
    mesh.audit_strict()
        .map_err(|_| DrillingRefusal::InvalidDrilledTopology)?;
    let topology = build_face_topology(mesh, WeldPolicy::ExactBits)
        .map_err(|_| DrillingRefusal::InvalidDrilledTopology)?;
    validate_face_topology(&topology).map_err(|_| DrillingRefusal::InvalidDrilledTopology)?;
    if topology.cut_flags().iter().any(|flag| *flag != 0) {
        return Err(DrillingRefusal::InvalidDrilledTopology);
    }
    let certificate = certify_closed_components(mesh, &topology)
        .map_err(|_| DrillingRefusal::InvalidDrilledTopology)?;
    if certificate
        .components()
        .iter()
        .any(|component| component.signed_volume_mm3() <= 0.0)
        && certificate.components().len() == 1
    {
        return Err(DrillingRefusal::InvalidDrilledTopology);
    }
    let welded = topology.source_vertex_to_welded();
    let mut edges = HashSet::with_capacity(mesh.tris().len() * 3);
    for triangle in mesh.tris() {
        let ids = triangle.map(|id| welded[id as usize]);
        for (a, b) in [(ids[0], ids[1]), (ids[1], ids[2]), (ids[2], ids[0])] {
            edges.insert(if a < b { (a, b) } else { (b, a) });
        }
    }
    let chi = i64::from(topology.welded_vertex_count())
        - i64::try_from(edges.len()).map_err(|_| DrillingRefusal::ResourceAccountingOverflow)?
        + i64::try_from(mesh.tris().len())
            .map_err(|_| DrillingRefusal::ResourceAccountingOverflow)?;
    Ok((
        certificate.signed_volume_mm3(),
        certificate.components().len(),
        chi,
    ))
}

fn prove_centerlines(
    mesh: &Mesh,
    holes: &[HoleGeometry],
    pitch: f64,
    limits: DrillingLimits,
) -> Result<Vec<usize>, DrillingRefusal> {
    admit_count(
        "centerline query",
        holes.len(),
        limits.max_centerline_queries,
    )?;
    let result_upper_bound = holes
        .len()
        .checked_mul(mesh.tris().len())
        .ok_or(DrillingRefusal::ResourceAccountingOverflow)?;
    admit_count(
        "centerline result upper bound",
        result_upper_bound,
        limits.max_centerline_results,
    )?;
    let estimate =
        NativeSurface::estimate_index_memory_from_indexed(mesh.verts(), mesh.tris(), pitch)
            .map_err(|_| DrillingRefusal::InvalidDrilledTopology)?;
    let surface = NativeSurface::from_indexed(
        mesh.verts(),
        mesh.tris(),
        NativeSurfaceConfig {
            grid_pitch_mm: pitch,
            max_grid_bytes: estimate.max_axis_grid_bytes(),
        },
    )
    .map_err(|_| DrillingRefusal::InvalidDrilledTopology)?;
    let origins: Vec<V3> = holes
        .iter()
        .map(|hole| point(hole.keepout_start_mm))
        .collect();
    let directions: Vec<V3> = holes.iter().map(|hole| point(hole.direction)).collect();
    let ceiling = holes
        .iter()
        .map(|hole| {
            norm(sub(
                point(hole.keepout_end_mm),
                point(hole.keepout_start_mm),
            ))
        })
        .fold(0.0_f64, f64::max)
        + PROOF_EPSILON_MM;
    let (offsets, ts, _) = surface
        .cast_rays(&origins, &directions, ceiling)
        .map_err(|_| DrillingRefusal::InvalidDrilledTopology)?;
    let mut counts = Vec::with_capacity(holes.len());
    for hole in 0..holes.len() {
        let begin = usize::try_from(offsets[hole])
            .map_err(|_| DrillingRefusal::ResourceAccountingOverflow)?;
        let end = usize::try_from(offsets[hole + 1])
            .map_err(|_| DrillingRefusal::ResourceAccountingOverflow)?;
        let intended_length = norm(sub(point(holes[hole].keepout_end_mm), origins[hole]));
        let count = ts[begin..end]
            .iter()
            .filter(|distance| **distance <= intended_length + PROOF_EPSILON_MM)
            .count();
        if count != 0 {
            return Err(DrillingRefusal::BlockedOpening { hole });
        }
        counts.push(count);
    }
    Ok(counts)
}

fn drainability_map(
    field: &CavityField,
    holes: &[HoleGeometry],
    limits: DrillingLimits,
) -> Result<(Vec<usize>, Vec<bool>), DrillingRefusal> {
    admit_count(
        "drainability cell",
        field.flat_count(),
        limits.max_drainability_cells,
    )?;
    drainability_map_admitted(field, holes, None)
}

fn drainability_map_excluding(
    field: &CavityField,
    holes: &[HoleGeometry],
    limits: DrillingLimits,
    excluded: &[bool],
) -> Result<(Vec<usize>, Vec<bool>), DrillingRefusal> {
    admit_count(
        "drainability cell",
        field.flat_count(),
        limits.max_drainability_cells,
    )?;
    if excluded.len() != field.flat_count() {
        return Err(DrillingRefusal::ResourceAccountingOverflow);
    }
    drainability_map_admitted(field, holes, Some(excluded))
}

fn drainability_map_admitted(
    field: &CavityField,
    holes: &[HoleGeometry],
    excluded: Option<&[bool]>,
) -> Result<(Vec<usize>, Vec<bool>), DrillingRefusal> {
    let mut members: Vec<usize> = field
        .member_flats()
        .filter(|flat| excluded.is_none_or(|mask| !mask[*flat]))
        .collect();
    members.sort_by_key(|flat| field.flat_to_index(*flat)[2]);
    let count = members.len();
    let mut ordinal = vec![usize::MAX; field.flat_count()];
    for (id, flat) in members.iter().copied().enumerate() {
        ordinal[flat] = id;
    }
    let mut parent: Vec<usize> = (0..count).collect();
    let mut has_drain = vec![false; count];
    let dims = field.dimensions();
    for hole in holes.iter().filter(|hole| hole.role == HoleRole::Drain) {
        let p = point(hole.center_mm);
        let origin = field.origin_mm().map(Finite::get);
        let pitch = field.pitch_mm().get();
        let center = grid_index_ties_even(p, origin, pitch);
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let q = [center[0] + dx, center[1] + dy, center[2] + dz];
                    if let Some(flat) = flat_if_in(q, dims) {
                        let id = ordinal[flat];
                        if id != usize::MAX {
                            has_drain[id] = true;
                        }
                    }
                }
            }
        }
    }
    let mut drainable = vec![false; count];
    let mut start = 0;
    while start < count {
        let z = field.flat_to_index(members[start])[2];
        let mut end = start + 1;
        while end < count && field.flat_to_index(members[end])[2] == z {
            end += 1;
        }
        for &flat in &members[start..end] {
            let id = ordinal[flat];
            let [x, y, z0] = field.flat_to_index(flat);
            for [dx, dy, dz] in [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, -1]] {
                let q = [x as isize + dx, y as isize + dy, z0 as isize + dz];
                if let Some(other_flat) = flat_if_in(q, dims) {
                    let other = ordinal[other_flat];
                    if other != usize::MAX {
                        union(id, other, &mut parent, &mut has_drain);
                    }
                }
            }
        }
        for (offset, &flat) in members[start..end].iter().enumerate() {
            let root = find(ordinal[flat], &mut parent);
            if has_drain[root] {
                drainable[start + offset] = true;
            }
        }
        start = end;
    }
    Ok((members, drainable))
}

fn find(mut id: usize, parent: &mut [usize]) -> usize {
    while parent[id] != id {
        parent[id] = parent[parent[id]];
        id = parent[id];
    }
    id
}
fn union(a: usize, b: usize, parent: &mut [usize], has: &mut [bool]) {
    let ra = find(a, parent);
    let rb = find(b, parent);
    if ra != rb {
        parent[rb] = ra;
        has[ra] |= has[rb];
    }
}
fn flat_if_in(q: [isize; 3], dims: [usize; 3]) -> Option<usize> {
    if q.iter()
        .enumerate()
        .any(|(axis, value)| *value < 0 || *value as usize >= dims[axis])
    {
        return None;
    }
    Some(q[0] as usize + q[1] as usize * dims[0] + q[2] as usize * dims[0] * dims[1])
}

fn round_ties_even(value: f64) -> f64 {
    let floor = value.floor();
    let fraction = value - floor;
    if fraction < 0.5 {
        floor
    } else if fraction > 0.5 {
        floor + 1.0
    } else if floor % 2.0 == 0.0 {
        floor
    } else {
        floor + 1.0
    }
}

fn grid_index_ties_even(point: V3, origin: V3, pitch: f64) -> [isize; 3] {
    [0, 1, 2].map(|axis| round_ties_even((point[axis] - origin[axis]) / pitch) as isize)
}

fn bounds_within_source(source: ([f64; 3], [f64; 3]), drilled: ([f64; 3], [f64; 3])) -> bool {
    (0..3).all(|axis| {
        drilled.0[axis] + PROOF_EPSILON_MM >= source.0[axis]
            && drilled.1[axis] - PROOF_EPSILON_MM <= source.1[axis]
            && drilled.0[axis] <= drilled.1[axis]
    })
}
fn validate_bounds_shrinkage(
    source: ([f64; 3], [f64; 3]),
    drilled: ([f64; 3], [f64; 3]),
    holes: &[HoleGeometry],
) -> Result<(), DrillingRefusal> {
    let max_shrinkage_mm = holes
        .iter()
        .map(|hole| 2.0 * hole.radius_mm.get())
        .fold(0.0_f64, f64::max);
    for axis in 0..3 {
        for (side, shrinkage_mm) in [
            drilled.0[axis] - source.0[axis],
            source.1[axis] - drilled.1[axis],
        ]
        .into_iter()
        .enumerate()
        {
            if !shrinkage_mm.is_finite() || shrinkage_mm > max_shrinkage_mm + PROOF_EPSILON_MM {
                return Err(DrillingRefusal::ExcessiveBoundsShrinkage {
                    axis,
                    side,
                    shrinkage_mm,
                    max_shrinkage_mm,
                });
            }
        }
    }
    Ok(())
}
fn validate_volume_balance(
    drilled_material_mm3: f64,
    solid_mm3: f64,
    cavity_mm3: f64,
) -> Result<(f64, f64, f64), DrillingRefusal> {
    let expected_material_mm3 = solid_mm3 - cavity_mm3;
    let residual_mm3 = drilled_material_mm3 - expected_material_mm3;
    let tolerance_mm3 = 0.02 * solid_mm3;
    if !expected_material_mm3.is_finite()
        || !residual_mm3.is_finite()
        || !tolerance_mm3.is_finite()
        || residual_mm3.abs() >= tolerance_mm3
    {
        Err(DrillingRefusal::VolumeBalance {
            error_mm3: residual_mm3,
            tolerance_mm3,
        })
    } else {
        Ok((expected_material_mm3, residual_mm3, tolerance_mm3))
    }
}
fn finite(value: f64) -> Result<Finite, DrillingRefusal> {
    Finite::new(value).map_err(|_| DrillingRefusal::InvalidVolumeDelta)
}
fn finite_bounds(value: ([f64; 3], [f64; 3])) -> Result<[[Finite; 3]; 2], DrillingRefusal> {
    Ok([
        value
            .0
            .map(finite)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .unwrap(),
        value
            .1
            .map(finite)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .unwrap(),
    ])
}
fn point(value: [Finite; 3]) -> V3 {
    value.map(Finite::get)
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}
fn normalize(a: V3) -> Result<V3, DrillingRefusal> {
    let n = norm(a);
    if !n.is_finite() || n <= 0.0 {
        Err(DrillingRefusal::InvalidHoleGeometry)
    } else {
        Ok(scale(a, 1.0 / n))
    }
}

fn segment_distance(p1: V3, q1: V3, p2: V3, q2: V3) -> Result<f64, DrillingRefusal> {
    let u = sub(q1, p1);
    let v = sub(q2, p2);
    let w = sub(p1, p2);
    let a = dot(u, u);
    let b = dot(u, v);
    let c = dot(v, v);
    let d = dot(u, w);
    let e = dot(v, w);
    if a <= 0.0 || c <= 0.0 {
        return Err(DrillingRefusal::InvalidHoleGeometry);
    }
    let denom = a * c - b * b;
    let mut s = if denom.abs() < PROOF_EPSILON_MM {
        0.0
    } else {
        ((b * e - c * d) / denom).clamp(0.0, 1.0)
    };
    let mut t = (b * s + e) / c;
    if t < 0.0 {
        t = 0.0;
        s = (-d / a).clamp(0.0, 1.0);
    } else if t > 1.0 {
        t = 1.0;
        s = ((b - d) / a).clamp(0.0, 1.0);
    }
    let distance = norm(sub(add(p1, scale(u, s)), add(p2, scale(v, t))));
    if distance.is_finite() {
        Ok(distance)
    } else {
        Err(DrillingRefusal::InvalidHoleGeometry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{create_shell, drains::plan_drains, registered_settings};
    use plinth_decimate::{run as run_decimation, DecimationBudget, ProbeConfig};
    use plinth_mesh::{load_binary_stl, mesh_fingerprint};
    use plinth_orient::{
        BaselineSelection, OrientationProvenance, OrientationReport, RigidTransform,
    };
    use scirs2_core::ndarray::Array3;
    use std::path::PathBuf;

    fn sphere_plan() -> PlannedShell {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/baseline/Sphere.prepped.oriented-solid.stl");
        let mesh =
            load_binary_stl(&path, WeldPolicy::None).expect("committed analytic Sphere fixture");
        plan_drains(create_shell(mesh, registered_settings()).expect("Sphere shell"))
            .expect("Sphere drain plan")
    }

    fn orientation_history() -> OrientationHistory {
        let mesh = plinth_mesh::make_box([10.0; 3], [0.0; 3]).expect("closed audit fixture");
        let budget =
            DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
                .expect("registered fixture budget");
        let outcome = run_decimation(&mesh, budget).expect("fixture decimation succeeds");
        let (_, audit) = outcome
            .into_decimated_model()
            .expect("fixture scale is in contract")
            .into_parts();
        OrientationHistory::from_reports(
            audit,
            OrientationReport {
                provenance: OrientationProvenance::Pinned {
                    pinned_report: PathBuf::from("baseline/model.pipeline.json"),
                },
                selection: BaselineSelection::Identity,
                transform: RigidTransform::identity(),
            },
        )
    }

    fn frozen_hollow_report() -> serde_json::Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/baseline/Sphere.pipeline.json");
        let bytes = std::fs::read(&path).unwrap_or_else(|error| {
            panic!("required frozen baseline is missing: {path:?}: {error}")
        });
        let root: serde_json::Value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|error| panic!("frozen baseline is invalid JSON: {path:?}: {error}"));
        root["stages"]
            .as_array()
            .and_then(|stages| stages.iter().find(|stage| stage["name"] == "hollow"))
            .and_then(|stage| stage.get("report"))
            .cloned()
            .expect("frozen baseline must contain the hollow report")
    }

    #[test]
    fn frozen_sphere_is_drilled_once_and_every_voxel_drains() {
        let plan = sphere_plan();
        let (source_material_mm3, _, _) =
            topology_evidence(plan.shell(), DrillingLimits::production()).unwrap();
        let drilled = drill_planned_shell(plan).expect("drilled Sphere");
        assert_eq!(drilled.evidence().holes.len(), 2);
        assert!(drilled
            .evidence()
            .holes
            .iter()
            .all(|hole| hole.clear_centerline_crossings == 0));
        assert_eq!(
            drilled.evidence().cavity_voxels,
            drilled.evidence().drainable_voxels
        );
        assert!(drilled.evidence().removed_material_mm3.get() > 0.0);
        assert_eq!(
            drilled.evidence().source_material_mm3.get(),
            source_material_mm3
        );
        assert_eq!(
            drilled.evidence().removed_material_mm3.get(),
            source_material_mm3 - drilled.evidence().drilled_material_mm3.get()
        );
        assert_eq!(
            drilled.evidence().volume_balance_residual_mm3.get(),
            drilled.evidence().drilled_material_mm3.get()
                - drilled.evidence().expected_material_mm3.get()
        );
        // The committed stage report was authoritative for rounded stage
        // metrics through 2026-08-12, even though no provenance proves its
        // exported STL still contains this drilled mesh. FU-53 broke that
        // correspondence on purpose. **The frozen baseline's drain/vent
        // geometry was re-recorded 2026-08-12 by FU-51, gated on FU-61's
        // bore-penetration confirmation** (see `drains.rs`'s
        // `frozen_shell_field_has_an_enclosed_drain_and_vent_plan`) — but
        // that re-record is scoped to drain/vent placement only, exactly what
        // FU-51 named. The Python-era volume fields (`final_ml`, `solid_ml`
        // and the rest of the a5-algorithm report this test does not read)
        // are untouched and still describe the pre-fix engine: no Rust code
        // reads them, and reproducing them would mean re-running the retired
        // Python pipeline, which this project does not do.
        // `drilled_material_mm3` is the shell's own remaining plastic (the
        // live equivalent of `final_ml`); the live engine now selects a
        // smaller, honestly-walled cavity (exact distance plus
        // HOLLOW_CAVITY_MARCH_MARGIN_MM), so it drills away LESS material and
        // MORE remains: 2.1 -> 2.9 mL. That is the direction the fix
        // promises -- a regression that walked this back toward 2.1 would be
        // a thinner, under-walled shell again.
        assert_eq!(
            (drilled.evidence().drilled_material_mm3.get() / 100.0).round() / 10.0,
            2.9
        );
        let report = frozen_hollow_report();
        assert_eq!(report["bodies"], 1);
        assert_eq!(report["watertight"], true);
        assert_eq!(report["winding"], true);
        assert_eq!(report["cavity_breathes"], true);
    }

    #[test]
    fn proved_drilling_uses_the_public_hollowed_model_handoff() {
        let drilled = drill_planned_shell(sphere_plan())
            .expect("drilled Sphere")
            .with_test_diagnostics(8_192);
        let expected = drilled.evidence().clone();
        let expected_plan = drilled.drain_plan().clone();
        let candidate_resources = drilled.cavity().resources();
        let holes_allocation = drilled.evidence().holes().as_ptr();
        let diagnostic_allocation = drilled.drain_plan().unavailable_candidates().as_ptr();
        assert_eq!(drilled.drain_plan().unavailable_candidates().len(), 8_192);
        assert!(expected
            .holes
            .iter()
            .any(|hole| hole.role == HoleRole::Vent));
        let model = drilled.into_hollowed_model(orientation_history());
        assert_eq!(
            model.history().hollow().disposition(),
            HollowDisposition::HollowedAndDrilled
        );
        assert_eq!(model.history().hollow().cavities().len(), 1);
        assert_eq!(model.history().hollow().drains().len(), 1);
        assert_eq!(
            model.history().hollow().drilling_evidence(),
            Some(&expected)
        );
        assert_eq!(model.history().hollow().drain_plan(), Some(&expected_plan));
        // The repair measurement reaches the report on the drilled path. It was
        // reachable only from `H0ResourceDiagnostics` before, which no report,
        // artifact or CLI reads -- so a repair could shrink a model's cavity and
        // leave no trace anywhere a caller could look. The `Sphere` needs no
        // repair, and that is a recorded zero rather than an absence.
        assert_eq!(
            model.history().hollow().cavity_repair(),
            Some(candidate_resources.cavity_repair())
        );
        // FU-37: the requested wall reaches the report on the hollowed path,
        // read off the cavity evidence that built this shell.
        assert_eq!(
            model.history().hollow().wall(),
            Some(registered_settings().wall_thickness_mm())
        );
        assert_eq!(
            model
                .history()
                .hollow()
                .drilling_evidence()
                .expect("retained drilling proof")
                .holes()
                .as_ptr(),
            holes_allocation
        );
        assert_eq!(
            model
                .history()
                .hollow()
                .drain_plan()
                .expect("retained drain plan")
                .unavailable_candidates()
                .as_ptr(),
            diagnostic_allocation
        );
        assert!(model.cavity_evidence().is_some());
        assert!(model.cavity_field().is_some());
        let (mesh, history) = model.into_parts();
        assert!(!mesh.tris().is_empty());
        assert_eq!(
            history.hollow().disposition(),
            HollowDisposition::HollowedAndDrilled
        );
        assert_eq!(history.hollow().drilling_evidence(), Some(&expected));
        assert_eq!(history.hollow().drain_plan(), Some(&expected_plan));
    }

    #[test]
    fn exact_boolean_input_caps_pass_and_cap_minus_one_refuses_untouched() {
        let planned = sphere_plan();
        let mut operands = vec![to_solid(planned.shell()).unwrap()];
        operands.extend(
            planned
                .drain_plan()
                .holes()
                .iter()
                .map(cylinder)
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
        );
        let exact = BooleanInputEstimate::for_meshes(&operands).unwrap();
        let generous = DrillingLimits::new(BooleanLimits {
            max_input_primitive_units: exact.operand_primitive_units,
            max_logical_marshaled_input_bytes: exact.logical_marshaled_input_bytes,
            max_output_vertices: DEFAULT_MAX_BOOLEAN_OUTPUT_VERTICES,
            max_output_triangles: DEFAULT_MAX_BOOLEAN_OUTPUT_TRIANGLES,
        })
        .unwrap();
        drill_planned_shell_with_limits(planned, generous).expect("exact cap accepted");

        let planned = sphere_plan();
        let refused = drill_planned_shell_with_limits(
            planned,
            DrillingLimits::new(BooleanLimits {
                max_input_primitive_units: exact.operand_primitive_units - 1,
                max_logical_marshaled_input_bytes: exact.logical_marshaled_input_bytes,
                max_output_vertices: DEFAULT_MAX_BOOLEAN_OUTPUT_VERTICES,
                max_output_triangles: DEFAULT_MAX_BOOLEAN_OUTPUT_TRIANGLES,
            })
            .unwrap(),
        )
        .expect_err("cap minus one refused");
        assert_eq!(refused.reason(), &DrillingRefusal::Boolean);
        assert_eq!(refused.into_planned().drain_plan().holes().len(), 2);

        let refused = drill_planned_shell_with_limits(
            sphere_plan(),
            DrillingLimits::new(BooleanLimits {
                max_input_primitive_units: exact.operand_primitive_units,
                max_logical_marshaled_input_bytes: exact.logical_marshaled_input_bytes - 1,
                max_output_vertices: DEFAULT_MAX_BOOLEAN_OUTPUT_VERTICES,
                max_output_triangles: DEFAULT_MAX_BOOLEAN_OUTPUT_TRIANGLES,
            })
            .unwrap(),
        )
        .expect_err("marshalled byte cap minus one refused");
        assert_eq!(refused.reason(), &DrillingRefusal::Boolean);
    }

    #[test]
    fn exact_boolean_output_caps_pass_and_cap_minus_one_refuses() {
        let baseline = drill_planned_shell(sphere_plan()).expect("baseline drill");
        let vertices = baseline.mesh().verts().len() as u64;
        let triangles = baseline.mesh().tris().len() as u64;
        let planned = sphere_plan();
        let mut operands = vec![to_solid(planned.shell()).unwrap()];
        operands.extend(
            planned
                .drain_plan()
                .holes()
                .iter()
                .map(cylinder)
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
        );
        let input = BooleanInputEstimate::for_meshes(&operands).unwrap();
        let limits = |max_output_vertices, max_output_triangles| {
            DrillingLimits::new(BooleanLimits {
                max_input_primitive_units: input.operand_primitive_units,
                max_logical_marshaled_input_bytes: input.logical_marshaled_input_bytes,
                max_output_vertices,
                max_output_triangles,
            })
            .unwrap()
        };
        drill_planned_shell_with_limits(planned, limits(vertices, triangles))
            .expect("exact output caps accepted");
        let refused =
            drill_planned_shell_with_limits(sphere_plan(), limits(vertices - 1, triangles))
                .expect_err("vertex cap minus one refused");
        assert_eq!(refused.reason(), &DrillingRefusal::Boolean);
        let refused =
            drill_planned_shell_with_limits(sphere_plan(), limits(vertices, triangles - 1))
                .expect_err("triangle cap minus one refused");
        assert_eq!(refused.reason(), &DrillingRefusal::Boolean);
    }

    #[test]
    fn undrilled_shell_is_a_corrupt_multi_crossing_fixture() {
        let plan = sphere_plan();
        assert!(matches!(
            prove_centerlines(
                plan.shell(),
                plan.drain_plan().holes(),
                plan.cavity_field().pitch_mm().get(),
                DrillingLimits::production()
            ),
            Err(DrillingRefusal::BlockedOpening { .. })
        ));
    }

    #[test]
    fn analytic_extra_opening_is_detected_by_topology_count() {
        let plan = sphere_plan();
        let mut extra = plan.drain_plan().holes()[1].clone();
        extra.center_mm = [
            Finite::new(0.0).unwrap(),
            Finite::new(0.0).unwrap(),
            Finite::new(10.0).unwrap(),
        ];
        extra.direction = [
            Finite::new(0.0).unwrap(),
            Finite::new(1.0).unwrap(),
            Finite::new(0.0).unwrap(),
        ];
        extra.keepout_start_mm = [
            Finite::new(0.0).unwrap(),
            Finite::new(-4.0).unwrap(),
            Finite::new(10.0).unwrap(),
        ];
        extra.keepout_end_mm = [
            Finite::new(0.0).unwrap(),
            Finite::new(25.0).unwrap(),
            Finite::new(10.0).unwrap(),
        ];
        extra.radius_mm = Finite::new(0.75).unwrap();
        let mut operands = vec![to_solid(plan.shell()).unwrap()];
        operands.extend(
            plan.drain_plan()
                .holes()
                .iter()
                .map(cylinder)
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
        );
        operands.push(cylinder(&extra).unwrap());
        let mesh = from_solid(
            ManifoldBackend
                .difference_limited(&operands, DrillingLimits::production().boolean())
                .unwrap(),
        )
        .unwrap();
        let (_, _, source_chi) =
            topology_evidence(plan.shell(), DrillingLimits::production()).unwrap();
        let (_, _, actual_chi) = topology_evidence(&mesh, DrillingLimits::production()).unwrap();
        let expected_two_openings = source_chi - 4;
        assert_ne!(
            actual_chi, expected_two_openings,
            "an unplanned third passage must not masquerade as the two-hole plan"
        );
    }

    #[test]
    fn drilled_digest_child() {
        if std::env::var_os("PLINTH_H1B_DIGEST_CHILD").is_none() {
            return;
        }
        let drilled = drill_planned_shell(sphere_plan()).expect("child drill");
        println!("H1B_DIGEST={}", mesh_fingerprint(drilled.mesh()).unwrap());
    }

    #[test]
    fn drilled_digest_is_subprocess_deterministic_at_one_four_and_sixteen_workers() {
        let executable = std::env::current_exe().expect("current test executable");
        let mut expected = None;
        for workers in [1, 4, 16] {
            let output = std::process::Command::new(&executable)
                .args([
                    "--exact",
                    "drilling::tests::drilled_digest_child",
                    "--nocapture",
                ])
                .env("PLINTH_H1B_DIGEST_CHILD", "1")
                .env("PLINTH_THREADS", workers.to_string())
                .output()
                .expect("spawn isolated determinism child");
            assert!(
                output.status.success(),
                "worker child {workers} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8(output.stdout).expect("UTF-8 child output");
            let digest = stdout
                .lines()
                .find_map(|line| line.strip_prefix("H1B_DIGEST="))
                .expect("child digest marker")
                .to_owned();
            if let Some(expected) = &expected {
                assert_eq!(&digest, expected, "worker count {workers} changed output");
            } else {
                expected = Some(digest);
            }
        }
    }

    #[test]
    fn analytic_overlap_and_tangent_are_refused_before_boolean() {
        let plan = sphere_plan();
        let mut holes = plan.drain_plan().holes().to_vec();
        holes[1] = holes[0].clone();
        assert_eq!(
            validate_final_holes(&holes),
            Err(DrillingRefusal::AmbiguousHolePair {
                first: 0,
                second: 1
            })
        );

        let mut holes = plan.drain_plan().holes().to_vec();
        holes[1] = holes[0].clone();
        let shift = holes[0].radius_mm.get() + holes[1].radius_mm.get();
        let shifted = |mut point: [Finite; 3]| {
            point[0] = Finite::new(point[0].get() + shift).unwrap();
            point
        };
        holes[1].center_mm = shifted(holes[1].center_mm);
        holes[1].outer_exit_mm = shifted(holes[1].outer_exit_mm);
        holes[1].keepout_start_mm = shifted(holes[1].keepout_start_mm);
        holes[1].keepout_end_mm = shifted(holes[1].keepout_end_mm);
        assert!(matches!(
            validate_final_holes(&holes),
            Err(DrillingRefusal::AmbiguousHolePair { .. })
        ));
    }

    #[test]
    fn numpy_grid_rounding_pins_negative_positive_and_averaged_half_cells() {
        assert_eq!(
            [-2.5, -1.5, -0.5, 0.5, 1.5, 2.5].map(round_ties_even),
            [-2.0, -2.0, 0.0, 0.0, 2.0, 2.0]
        );
        let origin = [0.0, 0.0, 0.0];
        let averaged_origins = [
            [-0.25, 0.25, 0.75],
            [0.25, -0.25, -0.75],
            [1.25, -1.25, 0.25],
        ];
        assert_eq!(
            grid_index_ties_even(averaged_origins[0], origin, 0.5),
            [0, 0, 2]
        );
        assert_eq!(
            grid_index_ties_even(averaged_origins[1], origin, 0.5),
            [0, 0, -2]
        );
        assert_eq!(
            grid_index_ties_even(averaged_origins[2], origin, 0.5),
            [2, -2, 0]
        );
    }

    #[test]
    fn frozen_volume_gate_is_strict_at_two_percent_and_bounds_reject_excess_deletion() {
        let solid = 1000.0;
        let cavity = 400.0;
        let expected = 600.0;
        assert!(matches!(
            validate_volume_balance(expected + 20.0, solid, cavity),
            Err(DrillingRefusal::VolumeBalance { .. })
        ));
        let just_inside = f64::from_bits((expected + 20.0).to_bits() - 1);
        assert!(validate_volume_balance(just_inside, solid, cavity).is_ok());
        assert!(matches!(
            validate_volume_balance(expected - 20.0, solid, cavity),
            Err(DrillingRefusal::VolumeBalance {
                error_mm3: -20.0,
                tolerance_mm3: 20.0
            })
        ));
        let (_, negative_residual, _) =
            validate_volume_balance(expected - 19.0, solid, cavity).unwrap();
        assert_eq!(negative_residual, -19.0);

        let holes = sphere_plan().drain_plan().holes().to_vec();
        assert!(matches!(
            validate_bounds_shrinkage(
                ([-10.0; 3], [10.0; 3]),
                ([-6.0, -10.0, -10.0], [10.0; 3]),
                &holes
            ),
            Err(DrillingRefusal::ExcessiveBoundsShrinkage {
                axis: 0,
                side: 0,
                ..
            })
        ));
    }

    #[test]
    fn every_count_admission_has_an_exact_and_cap_minus_one_boundary() {
        let plan = sphere_plan();
        let hole_count = plan.drain_plan().holes().len();
        let cutter_vertices = hole_count * (2 * CYLINDER_SECTIONS + 2);
        let cutter_triangles = hole_count * 4 * CYLINDER_SECTIONS;
        let topology_edges = plan.shell().tris().len() * 3;
        let centerline_results = hole_count * plan.shell().tris().len();
        let cells = plan.cavity_field().flat_count();
        let boolean = DrillingLimits::production().boolean();
        let exact = DrillingLimits::with_count_admissions(
            boolean,
            hole_count,
            cutter_vertices,
            cutter_triangles,
            topology_edges,
            hole_count,
            centerline_results,
            cells,
        )
        .unwrap();
        preflight_cutters(hole_count, exact).unwrap();
        topology_evidence(plan.shell(), exact).unwrap();
        drainability_map(plan.cavity_field(), plan.drain_plan().holes(), exact).unwrap();
        let fields = [
            (
                "hole",
                hole_count - 1,
                cutter_vertices,
                cutter_triangles,
                topology_edges,
                hole_count,
                centerline_results,
                cells,
            ),
            (
                "cutter vertex",
                hole_count,
                cutter_vertices - 1,
                cutter_triangles,
                topology_edges,
                hole_count,
                centerline_results,
                cells,
            ),
            (
                "cutter triangle",
                hole_count,
                cutter_vertices,
                cutter_triangles - 1,
                topology_edges,
                hole_count,
                centerline_results,
                cells,
            ),
        ];
        for (resource, holes, vertices, triangles, edges, queries, results, grid_cells) in fields {
            let capped = DrillingLimits::with_count_admissions(
                boolean, holes, vertices, triangles, edges, queries, results, grid_cells,
            )
            .unwrap();
            assert!(
                matches!(preflight_cutters(hole_count, capped), Err(DrillingRefusal::CountAdmission { resource: actual, .. }) if actual == resource)
            );
        }
        let topology_cap = DrillingLimits::with_count_admissions(
            boolean,
            hole_count,
            cutter_vertices,
            cutter_triangles,
            topology_edges - 1,
            hole_count,
            centerline_results,
            cells,
        )
        .unwrap();
        assert!(matches!(
            topology_evidence(plan.shell(), topology_cap),
            Err(DrillingRefusal::CountAdmission {
                resource: "topology edge occurrence",
                ..
            })
        ));
        let cell_cap = DrillingLimits::with_count_admissions(
            boolean,
            hole_count,
            cutter_vertices,
            cutter_triangles,
            topology_edges,
            hole_count,
            centerline_results,
            cells - 1,
        )
        .unwrap();
        assert!(matches!(
            drainability_map(plan.cavity_field(), plan.drain_plan().holes(), cell_cap),
            Err(DrillingRefusal::CountAdmission {
                resource: "drainability cell",
                ..
            })
        ));

        let drilled = drill_planned_shell(sphere_plan()).unwrap();
        let output_results = hole_count * drilled.mesh().tris().len();
        let output_edges = drilled.mesh().tris().len() * 3;
        let query_exact = DrillingLimits::with_count_admissions(
            boolean,
            hole_count,
            cutter_vertices,
            cutter_triangles,
            output_edges,
            hole_count,
            output_results,
            cells,
        )
        .unwrap();
        prove_centerlines(drilled.mesh(), plan.drain_plan().holes(), 0.5, query_exact).unwrap();
        let query_cap = DrillingLimits::with_count_admissions(
            boolean,
            hole_count,
            cutter_vertices,
            cutter_triangles,
            output_edges,
            hole_count - 1,
            output_results,
            cells,
        )
        .unwrap();
        assert!(matches!(
            prove_centerlines(drilled.mesh(), plan.drain_plan().holes(), 0.5, query_cap),
            Err(DrillingRefusal::CountAdmission {
                resource: "centerline query",
                ..
            })
        ));
        let result_cap = DrillingLimits::with_count_admissions(
            boolean,
            hole_count,
            cutter_vertices,
            cutter_triangles,
            output_edges,
            hole_count,
            output_results - 1,
            cells,
        )
        .unwrap();
        assert!(matches!(
            prove_centerlines(drilled.mesh(), plan.drain_plan().holes(), 0.5, result_cap),
            Err(DrillingRefusal::CountAdmission {
                resource: "centerline result upper bound",
                ..
            })
        ));
    }

    #[test]
    fn frozen_audit_threshold_and_three_drain_cap_are_inclusive_and_ordered() {
        // 4000 half-millimetre voxels are exactly 0.5ml.
        assert!(qualifies_audit_drain(4000, 0.5, 0, 0.5, 3));
        assert!(qualifies_audit_drain(4000, 0.5, 2, 0.5, 3));
        assert!(!qualifies_audit_drain(3999, 0.5, 0, 0.5, 3));
        assert!(!qualifies_audit_drain(4000, 0.5, 3, 0.5, 3));
    }

    #[test]
    fn qualifies_audit_drain_thresholds_match_the_registered_hollow_extra_drain_policy() {
        let policy = plinth_core::m0_constants()
            .expect("registered constants")
            .placeholders()
            .hollow_extra_drain_policy()
            .diagnostic();
        assert_eq!(policy.min_pocket_ml(), 0.5);
        assert_eq!(policy.max_extra_drains().get(), 3);
        assert_eq!(policy.exit_max_mm(), 8.0);
    }

    #[test]
    fn cylinder_sections_matches_the_registered_hollow_extra_drain_policy() {
        let policy = plinth_core::m0_constants()
            .expect("registered constants")
            .placeholders()
            .hollow_extra_drain_policy()
            .diagnostic();
        assert_eq!(
            u64::try_from(CYLINDER_SECTIONS).expect("cylinder section count fits u64"),
            policy.cylinder_sides().get(),
            "CYLINDER_SECTIONS sizes the DEFAULT_MAX_CUTTER_VERTICES/TRIANGLES const \
             expressions above at compile time and so cannot be a live registry read; this \
             pin makes a registry change loud instead of silently divergent.",
        );
    }

    #[test]
    fn frozen_trapped_pocket_policy_refills_a_long_exit() {
        let plan = sphere_plan();
        let mut horizontal_gap = vec![false; plan.cavity_field().flat_count()];
        for flat in plan.cavity_field().member_flats() {
            if plan.cavity_field().flat_to_index(flat)[2] == 20 {
                horizontal_gap[flat] = true;
            }
        }
        let split = plan.with_test_refilled_field(&horizontal_gap);
        let refill = prepare_audit_plan(&split, DrillingLimits::production()).unwrap();
        assert_eq!(refill.holes.len(), 0);
        // Moved 7,880 -> 7,856 on 2026-08-10 with the cavity-field exterior
        // margin: the Sphere's cavity lost the 72 voxels at its six polar
        // tangent points, and the slab this fixture severs is cut from what
        // remains. The refill *policy* is untouched; only the field it counts
        // over is smaller. See the 2026-08-10 journal section.
        //
        // Moved 7,856 -> 7,957 on 2026-08-12 by FU-50's area-proportional
        // conservative rasterizer (decisions/2026-08-12-owner-rulings-fu-
        // batch.md §3): the marked-voxel set is a strict superset of the old
        // one, so the Sphere's cavity gained voxels and the slab this fixture
        // severs is larger. Policy untouched; only the field is bigger.
        //
        // Moved 7,957 -> 4,609 on 2026-08-12 by FU-53: cavity selection now
        // runs on the exact nearest-surface distance plus
        // HOLLOW_CAVITY_MARCH_MARGIN_MM instead of the EDT's conservative
        // overestimate, so the honestly-walled Sphere cavity shrinks and the
        // slab this fixture severs is smaller again. Policy untouched; only
        // the field is smaller.
        assert_eq!(refill.refilled_voxels, 4_609);
        assert_eq!(refill.refilled_pockets, vec![4_609]);
        assert_eq!(refill.refill.iter().position(|value| *value), Some(35_606));
        // Moved 59,842 -> 56,396 in the same 2026-08-12 FU-53 pass.
        assert_eq!(refill.refill.iter().rposition(|value| *value), Some(56_396));
        let reconstructed =
            reconstructed_source_if_refilled(&split, &refill, DrillingLimits::production())
                .unwrap();
        let reconstructed = orient_boolean_cavity(from_solid(reconstructed).unwrap()).unwrap();
        let (post_refill_volume, components, _) =
            topology_evidence(&reconstructed, DrillingLimits::production()).unwrap();
        let (original_volume, _, _) =
            topology_evidence(split.shell(), DrillingLimits::production()).unwrap();
        assert_eq!(components, 2);
        assert!(post_refill_volume > original_volume);
    }

    #[test]
    fn audit_sampling_includes_the_exact_eight_millimetre_boundary() {
        let mut selected = Array3::from_elem((1, 1, 17), false);
        let mut occupied = Array3::from_elem((1, 1, 17), false);
        for x in 0..16 {
            selected[(0, 0, x)] = true;
            occupied[(0, 0, x)] = true;
        }
        let field = CavityField::from_selected_mask_and_occupancy(
            &selected,
            &occupied,
            [0.0, 0.0, 0.0],
            0.5,
        )
        .unwrap();
        assert_eq!(
            sampled_exit_along(&field, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 8.0),
            Some(8.0)
        );
        assert_eq!(
            sampled_exit_along(
                &field,
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                f64::from_bits(8.0_f64.to_bits() - 1),
            ),
            None
        );

        let candidates = vec![
            AuditCandidate {
                component_id: 0,
                direction_order: 0,
                origin: [1.0, 0.0, 0.0],
                direction: [0.0, 0.0, -1.0],
                sampled_mm: 1.0,
            },
            AuditCandidate {
                component_id: 0,
                direction_order: 1,
                origin: [1.0, 0.0, 0.0],
                direction: normalize([0.5, 0.0, -1.0]).unwrap(),
                sampled_mm: 2.0,
            },
            AuditCandidate {
                component_id: 0,
                direction_order: 2,
                origin: [1.0, 0.0, 0.0],
                direction: normalize([-0.5, 0.0, -1.0]).unwrap(),
                sampled_mm: 2.0,
            },
            AuditCandidate {
                component_id: 1,
                direction_order: 0,
                origin: [-1.0, 0.0, 0.0],
                direction: [0.0, 0.0, -1.0],
                sampled_mm: 8.0,
            },
            AuditCandidate {
                component_id: 0,
                direction_order: 3,
                origin: [1.0, 0.0, 0.0],
                direction: normalize([0.0, 0.5, -1.0]).unwrap(),
                sampled_mm: 2.0,
            },
            AuditCandidate {
                component_id: 0,
                direction_order: 4,
                origin: [1.0, 0.0, 0.0],
                direction: normalize([0.0, -0.5, -1.0]).unwrap(),
                sampled_mm: 2.0,
            },
        ];
        assert_eq!(
            candidates
                .iter()
                .filter(|candidate| candidate.component_id == 0)
                .map(|candidate| candidate.direction_order)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4]
        );
        let offsets = [0, 2, 4, 6, 8, 10, 12];
        let ts = [
            0.5, 1.5, // two crossings, but outer > sampled for candidate 0
            0.5, 1.5, // first fully valid ordered fallback
            0.5, 1.5, 0.5, 8.0, // exact 8mm component 1 boundary
            0.5, 1.5, 0.5, 1.5,
        ];
        assert_eq!(
            select_exact_audit_candidate(&candidates, &offsets, &ts, 0, &[])
                .unwrap()
                .map(|(index, _)| index),
            Some(1),
            "a two-crossing ray with outer > sampled must fall through to the first fully valid tie"
        );
        assert_eq!(
            select_exact_audit_candidate(&candidates, &offsets, &ts, 1, &[])
                .unwrap()
                .map(|(index, _)| index),
            Some(3),
            "the exact 8mm candidate remains selectable"
        );
    }

    #[test]
    fn production_drilling_succeeds_with_exact_refill_exclusion_and_public_report() {
        let plan = sphere_plan();
        let mut gap = vec![false; plan.cavity_field().flat_count()];
        for flat in plan.cavity_field().member_flats() {
            if plan.cavity_field().flat_to_index(flat)[0] == 32 {
                gap[flat] = true;
            }
        }
        let split = plan.with_test_refilled_field(&gap);
        let audit = prepare_audit_plan(&split, DrillingLimits::production()).unwrap();
        assert_eq!(audit.holes.len(), 0);
        // Moved 502 -> 490 on 2026-08-10 with the cavity-field exterior
        // margin, for the same reason as the long-exit fixture above: the
        // slab this fixture severs is cut from a cavity that lost its six
        // polar tangent points. The refill policy is untouched.
        //
        // Moved 490 -> 499 on 2026-08-12 by FU-50, for the same reason as the
        // long-exit fixture above: the marked-voxel set only grows under the
        // new area-proportional conservative rasterizer, so the slab this
        // fixture severs is cut from a larger cavity.
        //
        // Moved 499 -> 21 on 2026-08-12 by FU-53, for the same reason as the
        // long-exit fixture above: cavity selection now runs on the exact
        // nearest-surface distance plus HOLLOW_CAVITY_MARCH_MARGIN_MM, so the
        // honestly-walled Sphere cavity is smaller and the slab this fixture
        // severs is cut from less of it.
        assert_eq!(audit.refilled_voxels, 21);
        assert_eq!(audit.refilled_pockets, vec![21]);
        for flat in 0..split.cavity_field().flat_count() {
            let expected = split.cavity_field().contains_flat(flat)
                && split.cavity_field().flat_to_index(flat)[0] > 32;
            assert_eq!(audit.refill[flat], expected, "refill mask cell {flat}");
        }

        let drilled = drill_planned_shell(split).expect("bounded production refill must drill");
        // Moved 499 -> 21 on 2026-08-12 by FU-53, tracking `audit.refilled_voxels` above.
        assert_eq!(drilled.evidence().refilled_voxels, 21);
        assert_eq!(drilled.evidence().refilled_pockets[0].voxel_count, 21);
        assert_eq!(
            drilled.evidence().cavity_voxels,
            drilled.evidence().drainable_voxels
        );
        let expected_cavity_ml = drilled.evidence().final_cavity_mm3.get() / 1000.0;
        let expected_drain = drilled
            .evidence()
            .holes
            .iter()
            .find(|hole| hole.role == HoleRole::Drain)
            .unwrap()
            .clone();
        let model = drilled.into_hollowed_model(orientation_history());
        let report = model.history().hollow();
        // FU-37: the requested wall reaches the report on this production
        // drilling path too, not only the simpler `sphere_plan` fixture above.
        assert_eq!(
            report.wall(),
            Some(registered_settings().wall_thickness_mm())
        );
        // FU-55: and so does the wall the shell actually delivered, with the
        // claim it is making. This is the only path that produces a hollowed
        // artifact, so it is the one that has to carry the realized figure
        // out to `seal_and_emit` for the run's own output to record it.
        let realized = report
            .realized_wall()
            .expect("a drilled shell records what wall it realized");
        assert!(
            realized.wall_mm().get() >= registered_settings().wall_thickness_mm().get(),
            "a shipped shell never realizes less than the requested wall"
        );
        assert!(
            !realized.is_lower_bound(),
            "this shell's cavity vertices are all inside the query bound, so the \
             record is a measurement rather than a bound"
        );
        assert_eq!(report.cavities.len(), 1);
        assert_eq!(report.cavities[0].cavity_id, 1);
        assert_eq!(report.cavities[0].volume_ml.get(), expected_cavity_ml);
        assert_eq!(report.cavities[0].drain_ids, vec![0]);
        assert_eq!(report.drains.len(), 1);
        assert_eq!(report.drains[0].cavity_id, 1);
        assert_eq!(report.drains[0].entrance_mm, expected_drain.center_mm);
        assert_eq!(report.drains[0].exit_mm, expected_drain.exact_outer_exit_mm);
        assert_eq!(report.drains[0].radius_mm, expected_drain.radius_mm);
    }
}
