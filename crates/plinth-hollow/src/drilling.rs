//! H1b: one-batch drain/vent drilling and proof of the resulting openings.
//!
//! This stage consumes H1a's exact geometry.  It does not move, resize, or
//! repair a planned hole and it never exposes the undrilled shell as output.

use std::collections::HashSet;

use geom_kernel::{NativeSurface, NativeSurfaceConfig, V3};
use plinth_core::Finite;
use plinth_mesh::{
    build_face_topology, certify_closed_components, validate_face_topology, Mesh, WeldPolicy,
};
use plinth_solid::{BooleanInputEstimate, BooleanLimits, ManifoldBackend, Mesh as SolidMesh};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::drains::{DrainPlan, HoleGeometry, HoleRole, PlannedShell};
use crate::{bounds, from_solid, to_solid, CavityEvidence, CavityField};

const CYLINDER_SECTIONS: usize = 24;
const PROOF_EPSILON_MM: f64 = 1.0e-7;
const DEFAULT_MAX_BOOLEAN_PRIMITIVE_UNITS: u64 = 256 * 1024 * 1024;
const DEFAULT_MAX_BOOLEAN_MARSHALLED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const DEFAULT_MAX_BOOLEAN_OUTPUT_VERTICES: u64 = 64 * 1024 * 1024;
const DEFAULT_MAX_BOOLEAN_OUTPUT_TRIANGLES: u64 = 384 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrillingLimits {
    boolean: BooleanLimits,
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
        Ok(Self { boolean })
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
    pub role: HoleRole,
    pub lobe_id: Option<u32>,
    pub inner_crossing_mm: Finite,
    pub sampled_exit_mm: Finite,
    pub exact_outer_exit_mm: [Finite; 3],
    pub radius_mm: Finite,
    pub clear_centerline_crossings: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrillingEvidence {
    pub holes: Vec<DrilledHoleEvidence>,
    pub cavity_voxels: usize,
    pub drainable_voxels: usize,
    pub source_material_mm3: Finite,
    pub drilled_material_mm3: Finite,
    pub removed_material_mm3: Finite,
    pub source_bounds_mm: [[Finite; 3]; 2],
    pub drilled_bounds_mm: [[Finite; 3]; 2],
    pub bounds_delta_mm: [[Finite; 3]; 2],
    pub boolean_input_primitive_units: u64,
    pub boolean_marshaled_input_bytes: u64,
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
    #[error("drilling did not remove a finite positive material volume")]
    InvalidVolumeDelta,
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
}

pub fn drill_planned_shell(planned: PlannedShell) -> Result<HollowedAndDrilled, DrillingFailure> {
    drill_planned_shell_with_limits(planned, DrillingLimits::production())
}

pub fn drill_planned_shell_with_limits(
    planned: PlannedShell,
    limits: DrillingLimits,
) -> Result<HollowedAndDrilled, DrillingFailure> {
    match drill(&planned, limits) {
        Ok((mesh, evidence)) => {
            let (shell, cavity, cavity_field, drain_plan) = planned.into_parts();
            drop(shell);
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
) -> Result<(Mesh, DrillingEvidence), DrillingRefusal> {
    let holes = planned.drain_plan().holes();
    if !holes.iter().any(|h| h.role == HoleRole::Drain)
        || !holes.iter().any(|h| h.role == HoleRole::Vent)
    {
        return Err(DrillingRefusal::MissingRequiredRole);
    }
    validate_separate_holes(holes)?;
    let mut operands = Vec::with_capacity(holes.len() + 1);
    operands.push(to_solid(planned.shell()).map_err(|_| DrillingRefusal::Boolean)?);
    for hole in holes {
        operands.push(cylinder(hole)?);
    }
    let input =
        BooleanInputEstimate::for_meshes(&operands).map_err(|_| DrillingRefusal::Boolean)?;
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
    let (source_volume, source_components, source_chi) = topology_evidence(planned.shell())?;
    let (drilled_volume, drilled_components, drilled_chi) = topology_evidence(&mesh)?;
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
    let source_bounds = bounds(planned.shell()).map_err(|_| DrillingRefusal::BoundsChanged)?;
    let drilled_bounds = bounds(&mesh).map_err(|_| DrillingRefusal::BoundsChanged)?;
    if !bounds_within_source(source_bounds, drilled_bounds) {
        return Err(DrillingRefusal::BoundsChanged);
    }
    let removed = source_volume - drilled_volume;
    if !removed.is_finite() || removed <= 0.0 {
        return Err(DrillingRefusal::InvalidVolumeDelta);
    }
    let crossings = prove_centerlines(&mesh, holes, planned.cavity_field().pitch_mm().get())?;
    let drainable = drainable_voxels(planned.cavity_field(), holes)?;
    if drainable != planned.cavity_field().member_count() {
        return Err(DrillingRefusal::TrappedCavity {
            trapped_voxels: planned.cavity_field().member_count() - drainable,
        });
    }
    let evidence = DrillingEvidence {
        holes: holes
            .iter()
            .zip(crossings)
            .map(|(hole, clear_centerline_crossings)| DrilledHoleEvidence {
                role: hole.role,
                lobe_id: hole.lobe_id,
                inner_crossing_mm: hole.inner_crossing_mm,
                sampled_exit_mm: hole.sampled_exit_mm,
                exact_outer_exit_mm: hole.outer_exit_mm,
                radius_mm: hole.radius_mm,
                clear_centerline_crossings,
            })
            .collect(),
        cavity_voxels: planned.cavity_field().member_count(),
        drainable_voxels: drainable,
        source_material_mm3: finite(source_volume)?,
        drilled_material_mm3: finite(drilled_volume)?,
        removed_material_mm3: finite(removed)?,
        source_bounds_mm: finite_bounds(source_bounds)?,
        drilled_bounds_mm: finite_bounds(drilled_bounds)?,
        bounds_delta_mm: finite_bounds((
            sub(drilled_bounds.0, source_bounds.0),
            sub(drilled_bounds.1, source_bounds.1),
        ))?,
        boolean_input_primitive_units: input.operand_primitive_units,
        boolean_marshaled_input_bytes: input.logical_marshaled_input_bytes,
    };
    Ok((mesh, evidence))
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

fn validate_separate_holes(holes: &[HoleGeometry]) -> Result<(), DrillingRefusal> {
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

fn topology_evidence(mesh: &Mesh) -> Result<(f64, usize, i64), DrillingRefusal> {
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
) -> Result<Vec<usize>, DrillingRefusal> {
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

fn drainable_voxels(field: &CavityField, holes: &[HoleGeometry]) -> Result<usize, DrillingRefusal> {
    let mut members: Vec<usize> = field.member_flats().collect();
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
        let center = [0, 1, 2].map(|axis| ((p[axis] - origin[axis]) / pitch).round() as isize);
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
    let mut drainable = 0;
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
        for &flat in &members[start..end] {
            let root = find(ordinal[flat], &mut parent);
            if has_drain[root] {
                drainable += 1;
            }
        }
        start = end;
    }
    Ok(drainable)
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

fn bounds_within_source(source: ([f64; 3], [f64; 3]), drilled: ([f64; 3], [f64; 3])) -> bool {
    (0..3).all(|axis| {
        drilled.0[axis] + PROOF_EPSILON_MM >= source.0[axis]
            && drilled.1[axis] - PROOF_EPSILON_MM <= source.1[axis]
            && drilled.0[axis] <= drilled.1[axis]
    })
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
    use crate::{create_shell, drains::plan_drains};
    use plinth_mesh::{load_binary_stl, mesh_fingerprint};

    fn sphere_plan() -> PlannedShell {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/baseline/Sphere.prepped.oriented-solid.stl");
        let mesh =
            load_binary_stl(&path, WeldPolicy::None).expect("committed analytic Sphere fixture");
        plan_drains(create_shell(mesh).expect("Sphere shell")).expect("Sphere drain plan")
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
        let drilled = drill_planned_shell(sphere_plan()).expect("drilled Sphere");
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
        // The committed stage report is authoritative for rounded stage
        // metrics even though no provenance proves its exported STL still
        // contains this drilled mesh.
        let report = frozen_hollow_report();
        assert_eq!(
            (drilled.evidence().drilled_material_mm3.get() / 100.0).round() / 10.0,
            report["final_ml"].as_f64().expect("numeric final_ml")
        );
        assert_eq!(report["bodies"], 1);
        assert_eq!(report["watertight"], true);
        assert_eq!(report["winding"], true);
        assert_eq!(report["cavity_breathes"], true);
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
                plan.cavity_field().pitch_mm().get()
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
        let (_, _, source_chi) = topology_evidence(plan.shell()).unwrap();
        let (_, _, actual_chi) = topology_evidence(&mesh).unwrap();
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
            validate_separate_holes(&holes),
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
            validate_separate_holes(&holes),
            Err(DrillingRefusal::AmbiguousHolePair { .. })
        ));
    }
}
