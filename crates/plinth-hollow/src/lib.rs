//! Native H0 shell creation, before drain placement.
//!
//! The later stage entry point owns drain and vent placement:
//!
//! `plinth_hollow::run(OrientedModel, &HollowRequest) -> Result<HollowedModel, HollowError>`.
//!
//! H0 deliberately does not claim that entry point: a shell without drilled
//! drains must not become a `HollowedAndDrilled` production model. Instead
//! [`create_shell`] returns one closed shell mesh and the cavity evidence H1
//! needs to place drains. It keeps exactly one mesh; the source solid never
//! travels with the candidate.
#![forbid(unsafe_code)]

mod cavity_field;

pub use cavity_field::CavityField;

use fast_surface_nets::ndshape::RuntimeShape;
use fast_surface_nets::{surface_nets, SurfaceNetsBuffer};
use plinth_core::{m0_constants, Finite, M0ConstantsError, PrinterProfile};
use plinth_mesh::{
    build_face_topology, certify_closed_components, triage_components, validate_face_topology,
    Mesh, StrictMeshAuditError, TopologyError, TopologyValidationError, TriageError, WeldPolicy,
};
use plinth_orient::OrientationHistory;
use plinth_solid::{ManifoldBackend, Mesh as SolidMesh, SolidBoolean, SolidError};
use scirs2_core::ndarray::Array3;
use scirs2_ndimage::{distance_transform_edt, morphology3d::label3d};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Hollowing policy is selected from the printer profile and registered constants.
#[derive(Clone, Debug, PartialEq)]
pub struct HollowRequest {
    pub printer: PrinterProfile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolidFallbackReason {
    IneligibleForHollowing,
    CavityNotDrainable,
    InternalSupportRefused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HollowDisposition {
    HollowedAndDrilled,
    SolidFallback { reason: SolidFallbackReason },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrainRecord {
    pub cavity_id: u32,
    pub entrance_mm: [Finite; 3],
    pub exit_mm: [Finite; 3],
    pub radius_mm: Finite,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CavityRecord {
    pub cavity_id: u32,
    pub volume_ml: Finite,
    pub drain_ids: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HollowReport {
    pub disposition: HollowDisposition,
    pub cavities: Vec<CavityRecord>,
    pub drains: Vec<DrainRecord>,
}

/// Accumulated report state after hollowing; it contains no historical mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct HollowHistory {
    orientation: OrientationHistory,
    hollow: HollowReport,
}

impl HollowHistory {
    /// Builds passive accumulated report state; this does not construct a hollowed model.
    #[must_use]
    pub fn from_reports(orientation: OrientationHistory, hollow: HollowReport) -> Self {
        Self {
            orientation,
            hollow,
        }
    }

    #[must_use]
    pub const fn orientation(&self) -> &OrientationHistory {
        &self.orientation
    }

    #[must_use]
    pub const fn hollow(&self) -> &HollowReport {
        &self.hollow
    }

    #[must_use]
    pub fn into_reports(self) -> (OrientationHistory, HollowReport) {
        (self.orientation, self.hollow)
    }
}

/// Move-only hollowed/drilled model or explicitly retained oriented solid.
#[derive(Debug)]
pub struct HollowedModel {
    mesh: Mesh,
    history: HollowHistory,
}

impl HollowedModel {
    #[must_use]
    pub fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    #[must_use]
    pub const fn history(&self) -> &HollowHistory {
        &self.history
    }

    /// Moves the sole downstream mesh and report history into support planning.
    #[must_use]
    pub fn into_parts(self) -> (Mesh, HollowHistory) {
        (self.mesh, self.history)
    }
}

/// H0's explicit extraction settings. Both values come from the one typed M0
/// publication; they remain diagnostic `Placeholder`s, never an acceptance
/// claim. The pitch is the frozen voxel-oracle resolution used for the EDT,
/// component labels, and cavity eligibility; Manifold only subtracts that
/// selected voxel-surface cavity from the source solid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HollowSettings {
    wall_thickness_mm: Finite,
    voxel_pitch_mm: Finite,
    bulk_min_mm: Finite,
    save_min_mm3: Finite,
    save_min_fraction: Finite,
    volume_balance_rel_tol: Finite,
}

impl HollowSettings {
    pub fn from_registered_constants() -> Result<Self, HollowError> {
        let constants = m0_constants().map_err(HollowError::Constants)?;
        let placeholders = constants.placeholders();
        finite_settings(
            placeholders.hollow_wall_default_mm().diagnostic().get(),
            placeholders.hollow_pitch_mm().diagnostic().get(),
            placeholders.hollow_bulk_min_mm().diagnostic().get(),
            placeholders.hollow_save_min_ml().diagnostic().get() * 1000.0,
            placeholders.hollow_save_min_frac().diagnostic().get(),
            placeholders
                .hollow_volume_balance_rel_tol()
                .diagnostic()
                .get(),
        )
    }

    #[must_use]
    pub const fn wall_thickness_mm(self) -> Finite {
        self.wall_thickness_mm
    }

    #[must_use]
    pub const fn voxel_pitch_mm(self) -> Finite {
        self.voxel_pitch_mm
    }

    #[must_use]
    pub const fn bulk_min_mm(self) -> Finite {
        self.bulk_min_mm
    }

    #[must_use]
    pub const fn save_min_mm3(self) -> Finite {
        self.save_min_mm3
    }

    #[must_use]
    pub const fn save_min_fraction(self) -> Finite {
        self.save_min_fraction
    }

    #[must_use]
    pub const fn volume_balance_rel_tol(self) -> Finite {
        self.volume_balance_rel_tol
    }
}

fn finite_settings(
    wall_thickness_mm: f64,
    voxel_pitch_mm: f64,
    bulk_min_mm: f64,
    save_min_mm3: f64,
    save_min_fraction: f64,
    volume_balance_rel_tol: f64,
) -> Result<HollowSettings, HollowError> {
    let finite = |value| Finite::new(value).map_err(|_| HollowError::InvalidArithmetic);
    Ok(HollowSettings {
        wall_thickness_mm: finite(wall_thickness_mm)?,
        voxel_pitch_mm: finite(voxel_pitch_mm)?,
        bulk_min_mm: finite(bulk_min_mm)?,
        save_min_mm3: finite(save_min_mm3)?,
        save_min_fraction: finite(save_min_fraction)?,
        volume_balance_rel_tol: finite(volume_balance_rel_tol)?,
    })
}

const DEFAULT_HOLLOW_PEAK_BYTES: usize = 1024 * 1024 * 1024;
const DEFAULT_QUERY_WORKING_BYTES: usize = 8 * 1024 * 1024;
const QUERY_BYTES_PER_POINT: usize = std::mem::size_of::<[f64; 3]>()
    + std::mem::size_of::<bool>()
    + std::mem::size_of::<f64>()
    + std::mem::size_of::<i64>();

/// Caller-selected resource envelope for H0. `max_peak_bytes` covers the
/// complete conservative phase peak; `query_working_bytes` bounds each
/// deterministic gather/batch/scatter native query chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HollowResourceLimits {
    max_peak_bytes: usize,
    query_working_bytes: usize,
}

impl HollowResourceLimits {
    pub fn new(max_peak_bytes: usize, query_working_bytes: usize) -> Result<Self, HollowError> {
        if max_peak_bytes == 0 || query_working_bytes < QUERY_BYTES_PER_POINT {
            return Err(HollowError::InvalidResourceLimits);
        }
        Ok(Self {
            max_peak_bytes,
            query_working_bytes,
        })
    }

    #[must_use]
    pub const fn production() -> Self {
        Self {
            max_peak_bytes: DEFAULT_HOLLOW_PEAK_BYTES,
            query_working_bytes: DEFAULT_QUERY_WORKING_BYTES,
        }
    }

    #[must_use]
    pub const fn max_peak_bytes(self) -> usize {
        self.max_peak_bytes
    }

    #[must_use]
    pub const fn query_working_bytes(self) -> usize {
        self.query_working_bytes
    }
}

/// Conservative checked allocation accounting for one H0 invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HollowResourceEstimate {
    voxel_count: usize,
    query_batch_points: usize,
    grid_coordinate_bytes: usize,
    occupancy_bytes: usize,
    edt_peak_bytes: usize,
    label_peak_bytes: usize,
    surface_peak_bytes: usize,
    rasterization_peak_bytes: usize,
    retained_cavity_field_bytes: usize,
    output_upper_bound_bytes: usize,
    peak_bytes: usize,
}

impl HollowResourceEstimate {
    #[must_use]
    pub const fn voxel_count(self) -> usize {
        self.voxel_count
    }
    #[must_use]
    pub const fn query_batch_points(self) -> usize {
        self.query_batch_points
    }
    #[must_use]
    pub const fn grid_coordinate_bytes(self) -> usize {
        self.grid_coordinate_bytes
    }
    #[must_use]
    pub const fn occupancy_bytes(self) -> usize {
        self.occupancy_bytes
    }
    #[must_use]
    pub const fn edt_peak_bytes(self) -> usize {
        self.edt_peak_bytes
    }
    #[must_use]
    pub const fn label_peak_bytes(self) -> usize {
        self.label_peak_bytes
    }
    #[must_use]
    pub const fn surface_peak_bytes(self) -> usize {
        self.surface_peak_bytes
    }
    #[must_use]
    pub const fn rasterization_peak_bytes(self) -> usize {
        self.rasterization_peak_bytes
    }
    #[must_use]
    pub const fn retained_cavity_field_bytes(self) -> usize {
        self.retained_cavity_field_bytes
    }
    #[must_use]
    pub const fn output_upper_bound_bytes(self) -> usize {
        self.output_upper_bound_bytes
    }
    #[must_use]
    pub const fn peak_bytes(self) -> usize {
        self.peak_bytes
    }
}

/// Evidence that describes the sole inner cavity retained by H0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CavityEvidence {
    wall_thickness_mm: Finite,
    frozen_voxel_pitch_mm: Finite,
    cavity_volume_mm3: Finite,
    source_volume_mm3: Finite,
    resources: HollowResourceEstimate,
    resource_cap_bytes: usize,
}

impl CavityEvidence {
    #[must_use]
    pub const fn wall_thickness_mm(self) -> Finite {
        self.wall_thickness_mm
    }

    #[must_use]
    pub const fn frozen_voxel_pitch_mm(self) -> Finite {
        self.frozen_voxel_pitch_mm
    }

    #[must_use]
    pub const fn cavity_volume_mm3(self) -> Finite {
        self.cavity_volume_mm3
    }

    #[must_use]
    pub const fn source_volume_mm3(self) -> Finite {
        self.source_volume_mm3
    }

    #[must_use]
    pub const fn resources(self) -> HollowResourceEstimate {
        self.resources
    }

    #[must_use]
    pub const fn resource_cap_bytes(self) -> usize {
        self.resource_cap_bytes
    }
}

/// The H0 hand-off to H1. It owns only the closed shell mesh and its one
/// retained cavity; it cannot be mistaken for drilled, printable output.
#[derive(Debug)]
pub struct ShellCandidate {
    mesh: Mesh,
    cavity: CavityEvidence,
    cavity_field: CavityField,
}

impl ShellCandidate {
    #[must_use]
    pub const fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    #[must_use]
    pub const fn cavity(&self) -> &CavityEvidence {
        &self.cavity
    }

    /// Opaque H0-selected voxel membership retained for H1 planning. It is
    /// not reconstructed from the shell and admits no raw mutable buffer.
    #[must_use]
    pub const fn cavity_field(&self) -> &CavityField {
        &self.cavity_field
    }

    #[must_use]
    pub fn into_parts(self) -> (Mesh, CavityEvidence, CavityField) {
        (self.mesh, self.cavity, self.cavity_field)
    }
}

/// Convert a triaged, strict oriented solid into a single shell
/// mesh. H0 does not add drains and therefore does not expose `run`.
pub fn create_shell(input: Mesh) -> Result<ShellCandidate, HollowError> {
    let settings = HollowSettings::from_registered_constants()?;
    create_shell_with_resources(input, settings, HollowResourceLimits::production())
}

/// Testable H0 implementation with explicit settings. Production callers use
/// [`create_shell`], which is the only route that reads the registry.
pub fn create_shell_with_settings(
    input: Mesh,
    settings: HollowSettings,
) -> Result<ShellCandidate, HollowError> {
    create_shell_with_resources(input, settings, HollowResourceLimits::production())
}

/// H0 with an explicit caller-owned resource envelope.
pub fn create_shell_with_resources(
    input: Mesh,
    settings: HollowSettings,
    limits: HollowResourceLimits,
) -> Result<ShellCandidate, HollowError> {
    let triaged = triage_components(&input, WeldPolicy::ExactBits)?;
    let source = triaged.into_mesh();
    validate_closed_mesh(&source)?;
    validate_printable_bounds(&source)?;
    let source_volume_mm3 = source_volume_mm3(&source)?;
    let spec = grid_spec(&source, settings.voxel_pitch_mm.get())?;
    let resources = resource_estimate(&source, spec, limits)?;
    let grid = voxelize_filled(&source, spec)?;
    let VoxelGrid {
        origin: grid_origin,
        pitch: grid_pitch,
        material,
    } = grid;
    let edt = frozen_distance_transform(&material, grid_pitch)?;
    drop(material);
    let bulk_mm = 2.0 * edt.iter().copied().fold(0.0_f64, f64::max);
    if !bulk_mm.is_finite() {
        return Err(HollowError::InvalidArithmetic);
    }
    if bulk_mm < settings.bulk_min_mm.get() {
        return Err(HollowError::IneligibleBelowBulk);
    }
    let cavity_mask = edt.mapv(|distance_mm| distance_mm > settings.wall_thickness_mm.get());
    drop(edt);
    let (labels, label_count) =
        label3d(cavity_mask.view(), 6).map_err(|_| HollowError::VoxelStack)?;
    drop(cavity_mask);
    if label_count == 0 {
        return Err(HollowError::IneligibleNoCavity);
    }
    let largest = largest_label(&labels, label_count)?;
    let main = labels.mapv(|label| label == largest);
    let cavity_voxels = main.iter().filter(|value| **value).count();
    let cavity_volume_mm3 = (cavity_voxels as f64) * grid_pitch.powi(3);
    if !cavity_volume_mm3.is_finite() {
        return Err(HollowError::InvalidArithmetic);
    }
    let required_fraction_mm3 = source_volume_mm3 * settings.save_min_fraction.get();
    if !required_fraction_mm3.is_finite() {
        return Err(HollowError::InvalidArithmetic);
    }
    if cavity_volume_mm3 < settings.save_min_mm3.get() && cavity_volume_mm3 < required_fraction_mm3
    {
        return Err(HollowError::IneligibleSavings);
    }

    let cavity_field = CavityField::from_selected_mask(&main, grid_origin, grid_pitch)?;
    let inner = marching_cavity(&main, grid_origin, grid_pitch)?;
    drop(main);
    let shell = ManifoldBackend
        .difference(&[to_solid(&source)?, to_solid(&inner)?])
        .map_err(HollowError::Solid)?;
    let mesh = orient_boolean_cavity(from_solid(shell)?)?;
    validate_shell_topology(
        &mesh,
        source_volume_mm3,
        cavity_volume_mm3,
        settings.volume_balance_rel_tol.get(),
    )?;
    validate_printable_bounds(&mesh)?;
    let cavity = CavityEvidence {
        wall_thickness_mm: settings.wall_thickness_mm,
        frozen_voxel_pitch_mm: settings.voxel_pitch_mm,
        cavity_volume_mm3: finite(cavity_volume_mm3)?,
        source_volume_mm3: finite(source_volume_mm3)?,
        resources,
        resource_cap_bytes: limits.max_peak_bytes,
    };
    Ok(ShellCandidate {
        mesh,
        cavity,
        cavity_field,
    })
}

const PRINTABLE_MAX_EXTENT_MM: f64 = 300.0;

fn validate_closed_mesh(mesh: &Mesh) -> Result<(), HollowError> {
    mesh.audit_strict().map_err(HollowError::StrictAudit)?;
    let topology =
        build_face_topology(mesh, WeldPolicy::ExactBits).map_err(HollowError::Topology)?;
    validate_face_topology(&topology).map_err(HollowError::TopologyValidation)?;
    if topology.cut_flags().iter().any(|flags| *flags != 0) {
        return Err(HollowError::NotClosedOrOriented);
    }
    Ok(())
}

fn validate_shell_topology(
    mesh: &Mesh,
    source_volume_mm3: f64,
    cavity_volume_mm3: f64,
    relative_tolerance: f64,
) -> Result<(), HollowError> {
    validate_closed_mesh(mesh)?;
    let topology =
        build_face_topology(mesh, WeldPolicy::ExactBits).map_err(HollowError::Topology)?;
    let certificate = certify_closed_components(mesh, &topology).map_err(HollowError::Topology)?;
    let negative = certificate
        .components()
        .iter()
        .filter(|component| component.signed_volume_mm3() < 0.0)
        .count();
    if negative != 1
        || certificate.components().len() < 2
        || certificate
            .components()
            .iter()
            .any(|component| component.signed_volume_mm3() == 0.0)
    {
        return Err(HollowError::InvalidShellTopology);
    }
    match triage_components(mesh, WeldPolicy::ExactBits) {
        Err(TriageError::PreHollowed(refusal)) if refusal.cavity_components().len() == 1 => {}
        _ => return Err(HollowError::InvalidShellTopology),
    }
    let actual_material_mm3 = certificate.signed_volume_mm3();
    let expected_material_mm3 = source_volume_mm3 - cavity_volume_mm3;
    let tolerance_mm3 = source_volume_mm3 * relative_tolerance;
    if !actual_material_mm3.is_finite()
        || !expected_material_mm3.is_finite()
        || !tolerance_mm3.is_finite()
        || (actual_material_mm3 - expected_material_mm3).abs() > tolerance_mm3
    {
        return Err(HollowError::BooleanVolumeMismatch {
            actual_material_mm3,
            expected_material_mm3,
            tolerance_mm3,
        });
    }
    Ok(())
}

fn orient_boolean_cavity(mesh: Mesh) -> Result<Mesh, HollowError> {
    let topology =
        build_face_topology(&mesh, WeldPolicy::ExactBits).map_err(HollowError::Topology)?;
    validate_face_topology(&topology).map_err(HollowError::TopologyValidation)?;
    let cavity_component = match triage_components(&mesh, WeldPolicy::ExactBits) {
        Err(TriageError::PreHollowed(refusal)) if refusal.cavity_components().len() == 1 => {
            u32::try_from(refusal.cavity_components()[0].component_id())
                .map_err(|_| HollowError::InvalidShellTopology)?
        }
        _ => return Err(HollowError::InvalidShellTopology),
    };
    let triangles = mesh
        .tris()
        .iter()
        .zip(topology.component_ids())
        .map(|(triangle, component)| {
            if *component == cavity_component {
                [triangle[0], triangle[2], triangle[1]]
            } else {
                *triangle
            }
        })
        .collect();
    Mesh::new(mesh.verts().to_vec(), triangles).map_err(HollowError::OutputMesh)
}

fn source_volume_mm3(mesh: &Mesh) -> Result<f64, HollowError> {
    let topology =
        build_face_topology(mesh, WeldPolicy::ExactBits).map_err(HollowError::Topology)?;
    let certificate = certify_closed_components(mesh, &topology).map_err(HollowError::Topology)?;
    if certificate
        .components()
        .iter()
        .all(|component| component.signed_volume_mm3() > 0.0)
    {
        let volume = certificate.signed_volume_mm3();
        if volume.is_finite() && volume > 0.0 {
            return Ok(volume);
        }
    }
    Err(HollowError::NotClosedOrOriented)
}

struct VoxelGrid {
    origin: [f64; 3],
    pitch: f64,
    material: Array3<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct GridSpec {
    origin: [f64; 3],
    dims: [usize; 3],
    pitch: f64,
    count: usize,
}

fn grid_spec(mesh: &Mesh, pitch: f64) -> Result<GridSpec, HollowError> {
    if !pitch.is_finite() || pitch <= 0.0 {
        return Err(HollowError::InvalidArithmetic);
    }
    let (low, high) = bounds(mesh)?;
    let mut origin = [0.0; 3];
    let mut dims = [0_usize; 3];
    for axis in 0..3 {
        let low_index = low[axis] / pitch;
        let high_index = high[axis] / pitch;
        if !low_index.is_finite() || !high_index.is_finite() {
            return Err(HollowError::VoxelGridTooLarge);
        }
        origin[axis] = low_index.round() * pitch;
        let end = high_index.round() * pitch;
        let span = end - origin[axis];
        if !origin[axis].is_finite() || !end.is_finite() || !span.is_finite() || span < 0.0 {
            return Err(HollowError::InvalidArithmetic);
        }
        let axis_samples_f64 = (span / pitch).round();
        if !axis_samples_f64.is_finite()
            || axis_samples_f64 < 0.0
            || axis_samples_f64 > (u32::MAX - 1) as f64
        {
            return Err(HollowError::VoxelGridTooLarge);
        }
        let axis_samples = axis_samples_f64 as usize;
        dims[axis] = axis_samples
            .checked_add(1)
            .ok_or(HollowError::VoxelGridTooLarge)?;
        if dims[axis] < 3 {
            return Err(HollowError::InvalidArithmetic);
        }
    }
    let count = dims[0]
        .checked_mul(dims[1])
        .and_then(|value| value.checked_mul(dims[2]))
        .ok_or(HollowError::VoxelGridTooLarge)?;
    Ok(GridSpec {
        origin,
        dims,
        pitch,
        count,
    })
}

fn checked_scale(count: usize, bytes: usize) -> Result<usize, HollowError> {
    count
        .checked_mul(bytes)
        .ok_or(HollowError::ResourceAccountingOverflow)
}

fn checked_sum(values: &[usize]) -> Result<usize, HollowError> {
    values.iter().try_fold(0_usize, |total, value| {
        total
            .checked_add(*value)
            .ok_or(HollowError::ResourceAccountingOverflow)
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LabelMemoryEstimate {
    requested_live_bytes: usize,
    conservative_peak_bytes: usize,
}

fn allocator_allocation_bound(
    count: usize,
    element_size: usize,
    element_align: usize,
) -> Result<usize, HollowError> {
    let requested = checked_scale(count, element_size)?;
    let allocation_align = element_align.max(2 * std::mem::size_of::<usize>());
    let rounded = requested
        .checked_add(allocation_align - 1)
        .map(|value| value / allocation_align * allocation_align)
        .ok_or(HollowError::ResourceAccountingOverflow)?;
    // These exact-size Vec allocations do not intentionally reserve spare
    // capacity. Doubling the aligned request plus two words conservatively
    // covers allocator size classes, alignment padding, and block metadata.
    rounded
        .checked_mul(2)
        .and_then(|value| value.checked_add(2 * std::mem::size_of::<usize>()))
        .ok_or(HollowError::ResourceAccountingOverflow)
}

fn label_phase_memory(voxel_count: usize) -> Result<LabelMemoryEstimate, HollowError> {
    let union_find_count = voxel_count
        .checked_add(1)
        .ok_or(HollowError::ResourceAccountingOverflow)?;
    let usize_size = std::mem::size_of::<usize>();
    let usize_align = std::mem::align_of::<usize>();
    let tuple_size = std::mem::size_of::<(isize, isize, isize)>();
    let tuple_align = std::mem::align_of::<(isize, isize, isize)>();

    // scirs2-ndimage 0.6.5 label3d first pass: input mask, provisional
    // labels, UnionFind parent/rank, three fixed 6-neighbor offsets, and one
    // per-voxel neighbor Vec (capacity conservatively bounded at eight).
    let first_requested = checked_sum(&[
        checked_scale(voxel_count, std::mem::size_of::<bool>())?,
        checked_scale(voxel_count, usize_size)?,
        checked_scale(union_find_count, usize_size)?,
        checked_scale(union_find_count, usize_size)?,
        checked_scale(3, tuple_size)?,
        checked_scale(8, usize_size)?,
    ])?;
    let first_conservative = checked_sum(&[
        allocator_allocation_bound(
            voxel_count,
            std::mem::size_of::<bool>(),
            std::mem::align_of::<bool>(),
        )?,
        allocator_allocation_bound(voxel_count, usize_size, usize_align)?,
        allocator_allocation_bound(union_find_count, usize_size, usize_align)?,
        allocator_allocation_bound(union_find_count, usize_size, usize_align)?,
        allocator_allocation_bound(3, tuple_size, tuple_align)?,
        allocator_allocation_bound(8, usize_size, usize_align)?,
    ])?;

    // Second pass retains the mask, provisional labels, both UnionFind
    // vectors and offsets while adding root_to_final and the i32 Array3.
    let second_requested = checked_sum(&[
        checked_scale(voxel_count, std::mem::size_of::<bool>())?,
        checked_scale(voxel_count, usize_size)?,
        checked_scale(union_find_count, usize_size)?,
        checked_scale(union_find_count, usize_size)?,
        checked_scale(3, tuple_size)?,
        checked_scale(union_find_count, usize_size)?,
        checked_scale(voxel_count, std::mem::size_of::<i32>())?,
    ])?;
    let second_conservative = checked_sum(&[
        allocator_allocation_bound(
            voxel_count,
            std::mem::size_of::<bool>(),
            std::mem::align_of::<bool>(),
        )?,
        allocator_allocation_bound(voxel_count, usize_size, usize_align)?,
        allocator_allocation_bound(union_find_count, usize_size, usize_align)?,
        allocator_allocation_bound(union_find_count, usize_size, usize_align)?,
        allocator_allocation_bound(3, tuple_size, tuple_align)?,
        allocator_allocation_bound(union_find_count, usize_size, usize_align)?,
        allocator_allocation_bound(
            voxel_count,
            std::mem::size_of::<i32>(),
            std::mem::align_of::<i32>(),
        )?,
    ])?;
    Ok(LabelMemoryEstimate {
        requested_live_bytes: first_requested.max(second_requested),
        conservative_peak_bytes: first_conservative.max(second_conservative),
    })
}

fn resource_estimate(
    mesh: &Mesh,
    spec: GridSpec,
    limits: HollowResourceLimits,
) -> Result<HollowResourceEstimate, HollowError> {
    let query_batch_points = spec
        .count
        .min(limits.query_working_bytes / QUERY_BYTES_PER_POINT)
        .max(1);
    let grid_coordinate_bytes = checked_scale(query_batch_points, std::mem::size_of::<[f64; 3]>())?;
    let occupancy_bytes = checked_scale(spec.count, std::mem::size_of::<bool>())?;
    // `voxelize_subdivide` emits its leaf vertices directly into the grid;
    // it retains no tessellated mesh. Hole filling keeps the surface grid, a
    // 6-connected exterior mask, and a worst-case frontier containing every
    // voxel. The fixed-depth recursive splitter is stack-only.
    let rasterization_peak_bytes = checked_sum(&[
        occupancy_bytes,
        occupancy_bytes,
        checked_scale(spec.count, std::mem::size_of::<usize>())?,
    ])?;

    // Source occupancy + EDT input clone + squared and final f64 distance
    // arrays, plus a conservative line-transform scratch upper bound.
    let edt_arrays = checked_scale(
        spec.count,
        2 * std::mem::size_of::<bool>() + 2 * std::mem::size_of::<f64>(),
    )?;
    let edt_line = checked_scale(
        *spec.dims.iter().max().unwrap_or(&0),
        4 * std::mem::size_of::<usize>().max(std::mem::size_of::<f64>()),
    )?;
    let edt_peak_bytes = checked_sum(&[edt_arrays, edt_line])?;

    let label_memory = label_phase_memory(spec.count)?;
    debug_assert!(label_memory.conservative_peak_bytes >= label_memory.requested_live_bytes);
    let retained_cavity_field_bytes = cavity_field::packed_bytes_for_voxels(spec.count)?;
    // Packing the selected mask occurs while labels and the selected bool mask
    // are still live. The compact field remains in the returned candidate.
    let label_peak_bytes = checked_sum(&[
        label_memory.conservative_peak_bytes,
        retained_cavity_field_bytes,
    ])?;

    // Surface Nets worst case: one surface vertex per cell, up to three quads
    // (six triangles) per vertex, all library scratch/output buffers, the f32
    // SDF, the bool mask, and simultaneous f64/u32 Mesh conversion buffers.
    let surface_lattice_count = spec.dims.iter().try_fold(1_usize, |total, dimension| {
        dimension
            .checked_add(2)
            .and_then(|padded| total.checked_mul(padded))
            .ok_or(HollowError::ResourceAccountingOverflow)
    })?;
    let surface_peak_bytes = checked_scale(surface_lattice_count, 224)?;
    let source_mesh_bytes = checked_sum(&[
        checked_scale(mesh.verts().len(), std::mem::size_of::<[f64; 3]>())?,
        checked_scale(mesh.tris().len(), std::mem::size_of::<[u32; 3]>())?,
    ])?;
    let inner_mesh_upper = checked_scale(
        surface_lattice_count,
        std::mem::size_of::<[f64; 3]>() + 6 * std::mem::size_of::<[u32; 3]>(),
    )?;
    let output_upper_bound_bytes = checked_sum(&[source_mesh_bytes, inner_mesh_upper])?;
    // Rust source/inner meshes, FFI conversion, native Boolean result, and
    // returned Mesh can coexist at the boundary.
    let boolean_peak = checked_sum(&[
        checked_scale(output_upper_bound_bytes, 3)?,
        retained_cavity_field_bytes,
    ])?;
    let peak_bytes = [
        rasterization_peak_bytes,
        edt_peak_bytes,
        label_peak_bytes,
        checked_sum(&[surface_peak_bytes, retained_cavity_field_bytes])?,
        boolean_peak,
    ]
    .into_iter()
    .max()
    .ok_or(HollowError::ResourceAccountingOverflow)?;
    let estimate = HollowResourceEstimate {
        voxel_count: spec.count,
        query_batch_points,
        grid_coordinate_bytes,
        occupancy_bytes,
        edt_peak_bytes,
        label_peak_bytes,
        surface_peak_bytes,
        rasterization_peak_bytes,
        retained_cavity_field_bytes,
        output_upper_bound_bytes,
        peak_bytes,
    };
    if estimate.peak_bytes > limits.max_peak_bytes {
        Err(HollowError::ResourceLimitExceeded {
            estimate,
            cap_bytes: limits.max_peak_bytes,
        })
    } else {
        Ok(estimate)
    }
}

fn voxelize_filled(
    mesh: &Mesh,
    spec: GridSpec,
) -> Result<VoxelGrid, HollowError> {
    // Frozen source: `trimesh.voxelized(pitch).fill()`. Its default
    // `subdivide` voxelizer recursively quarters each triangle until every
    // edge is at most pitch/2, rounds each resulting vertex to a grid point,
    // then `binary_fill_holes` fills the 6-connected exterior complement.
    // Point-in-mesh and closest-surface tests are a different voxelizer: their
    // boundary bands are not the source field and can put an H1 candidate
    // outside the Boolean cavity.
    let mut occupancy = vec![false; spec.count];
    for triangle in mesh.tris() {
        let [a, b, c] = *triangle;
        let a = *mesh
            .verts()
            .get(usize::try_from(a).map_err(|_| HollowError::VoxelGridTooLarge)?)
            .ok_or(HollowError::VoxelGridTooLarge)?;
        let b = *mesh
            .verts()
            .get(usize::try_from(b).map_err(|_| HollowError::VoxelGridTooLarge)?)
            .ok_or(HollowError::VoxelGridTooLarge)?;
        let c = *mesh
            .verts()
            .get(usize::try_from(c).map_err(|_| HollowError::VoxelGridTooLarge)?)
            .ok_or(HollowError::VoxelGridTooLarge)?;
        rasterize_subdivided_triangle(a, b, c, spec, &mut occupancy, 0)?;
    }
    fill_voxel_holes(&mut occupancy, spec.dims)?;
    let material = Array3::from_shape_vec((spec.dims[2], spec.dims[1], spec.dims[0]), occupancy)
        .map_err(|_| HollowError::VoxelGridTooLarge)?;
    Ok(VoxelGrid {
        origin: spec.origin,
        pitch: spec.pitch,
        material,
    })
}

fn rasterize_subdivided_triangle(
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
    spec: GridSpec,
    occupancy: &mut [bool],
    iteration: u8,
) -> Result<(), HollowError> {
    let max_edge = spec.pitch / 2.0;
    let edge_too_long = |left: [f64; 3], right: [f64; 3]| {
        left.iter()
            .zip(right)
            .map(|(left, right)| (left - right).powi(2))
            .sum::<f64>()
            .sqrt()
            > max_edge
    };
    if !edge_too_long(a, b) && !edge_too_long(b, c) && !edge_too_long(c, a) {
        mark_voxel_vertex(a, spec, occupancy)?;
        mark_voxel_vertex(b, spec, occupancy)?;
        return mark_voxel_vertex(c, spec, occupancy);
    }
    if iteration >= 10 {
        // Matches trimesh.remesh.subdivide_to_size(max_iter=10).
        return Err(HollowError::VoxelGridTooLarge);
    }
    let midpoint = |left: [f64; 3], right: [f64; 3]| {
        [
            (left[0] + right[0]) / 2.0,
            (left[1] + right[1]) / 2.0,
            (left[2] + right[2]) / 2.0,
        ]
    };
    let ab = midpoint(a, b);
    let bc = midpoint(b, c);
    let ca = midpoint(c, a);
    let next = iteration + 1;
    rasterize_subdivided_triangle(a, ab, ca, spec, occupancy, next)?;
    rasterize_subdivided_triangle(ab, b, bc, spec, occupancy, next)?;
    rasterize_subdivided_triangle(ca, bc, c, spec, occupancy, next)?;
    rasterize_subdivided_triangle(ab, bc, ca, spec, occupancy, next)
}

fn mark_voxel_vertex(
    point: [f64; 3],
    spec: GridSpec,
    occupancy: &mut [bool],
) -> Result<(), HollowError> {
    let mut index = [0_usize; 3];
    for axis in 0..3 {
        let grid = round_ties_even(point[axis] / spec.pitch);
        let origin = round_ties_even(spec.origin[axis] / spec.pitch);
        let relative = grid - origin;
        if !relative.is_finite()
            || relative < 0.0
            || relative >= spec.dims[axis] as f64
            || relative.fract() != 0.0
        {
            return Err(HollowError::VoxelGridTooLarge);
        }
        index[axis] = relative as usize;
    }
    let plane = spec.dims[0]
        .checked_mul(spec.dims[1])
        .ok_or(HollowError::VoxelGridTooLarge)?;
    let flat = index[0]
        .checked_add(index[1].checked_mul(spec.dims[0]).ok_or(HollowError::VoxelGridTooLarge)?)
        .and_then(|value| value.checked_add(index[2].checked_mul(plane)?))
        .ok_or(HollowError::VoxelGridTooLarge)?;
    *occupancy
        .get_mut(flat)
        .ok_or(HollowError::VoxelGridTooLarge)? = true;
    Ok(())
}

fn round_ties_even(value: f64) -> f64 {
    let lower = value.floor();
    let fractional = value - lower;
    if fractional < 0.5 {
        lower
    } else if fractional > 0.5 {
        lower + 1.0
    } else if lower.rem_euclid(2.0) == 0.0 {
        lower
    } else {
        lower + 1.0
    }
}

fn fill_voxel_holes(occupancy: &mut [bool], dims: [usize; 3]) -> Result<(), HollowError> {
    let [nx, ny, nz] = dims;
    let plane = nx.checked_mul(ny).ok_or(HollowError::VoxelGridTooLarge)?;
    let mut exterior = vec![false; occupancy.len()];
    let mut queue = std::collections::VecDeque::new();
    let mark_exterior = |x: usize,
                         y: usize,
                         z: usize,
                         exterior: &mut [bool],
                         queue: &mut std::collections::VecDeque<usize>| {
        let flat = x + y * nx + z * plane;
        if !occupancy[flat] && !exterior[flat] {
            exterior[flat] = true;
            queue.push_back(flat);
        }
    };
    for z in 0..nz {
        for y in 0..ny {
            mark_exterior(0, y, z, &mut exterior, &mut queue);
            mark_exterior(nx - 1, y, z, &mut exterior, &mut queue);
        }
    }
    for z in 0..nz {
        for x in 0..nx {
            mark_exterior(x, 0, z, &mut exterior, &mut queue);
            mark_exterior(x, ny - 1, z, &mut exterior, &mut queue);
        }
    }
    for y in 0..ny {
        for x in 0..nx {
            mark_exterior(x, y, 0, &mut exterior, &mut queue);
            mark_exterior(x, y, nz - 1, &mut exterior, &mut queue);
        }
    }
    while let Some(flat) = queue.pop_front() {
        let z = flat / plane;
        let remaining = flat % plane;
        let y = remaining / nx;
        let x = remaining % nx;
        if x > 0 {
            mark_exterior(x - 1, y, z, &mut exterior, &mut queue);
        }
        if x + 1 < nx {
            mark_exterior(x + 1, y, z, &mut exterior, &mut queue);
        }
        if y > 0 {
            mark_exterior(x, y - 1, z, &mut exterior, &mut queue);
        }
        if y + 1 < ny {
            mark_exterior(x, y + 1, z, &mut exterior, &mut queue);
        }
        if z > 0 {
            mark_exterior(x, y, z - 1, &mut exterior, &mut queue);
        }
        if z + 1 < nz {
            mark_exterior(x, y, z + 1, &mut exterior, &mut queue);
        }
    }
    for (filled, exterior) in occupancy.iter_mut().zip(exterior) {
        if !exterior {
            *filled = true;
        }
    }
    Ok(())
}

fn largest_label(labels: &Array3<i32>, label_count: usize) -> Result<i32, HollowError> {
    let mut counts = vec![
        0_usize;
        label_count
            .checked_add(1)
            .ok_or(HollowError::VoxelGridTooLarge)?
    ];
    for label in labels {
        let label = usize::try_from(*label).map_err(|_| HollowError::VoxelStack)?;
        if label > label_count {
            return Err(HollowError::VoxelStack);
        }
        counts[label] = counts[label]
            .checked_add(1)
            .ok_or(HollowError::VoxelGridTooLarge)?;
    }
    let (label, count) = counts
        .iter()
        .enumerate()
        .skip(1)
        .max_by_key(|(_, count)| **count)
        .ok_or(HollowError::VoxelStack)?;
    if *count == 0 {
        Err(HollowError::VoxelStack)
    } else {
        i32::try_from(label).map_err(|_| HollowError::VoxelGridTooLarge)
    }
}

fn frozen_distance_transform(
    material: &Array3<bool>,
    pitch: f64,
) -> Result<Array3<f64>, HollowError> {
    if material.iter().all(|filled| *filled) {
        // SciPy's frozen all-foreground behavior measures from the implicit
        // background sample at (-1, 0, 0); scirs2 otherwise returns infinity.
        return Ok(Array3::from_shape_fn(material.raw_dim(), |(z, y, x)| {
            let dz = z as f64 + 1.0;
            let dy = y as f64;
            let dx = x as f64;
            pitch * dz.hypot(dy).hypot(dx)
        }));
    }
    let (distances, _) =
        distance_transform_edt(&material.clone().into_dyn(), Some(&[pitch; 3]), true, false)
            .map_err(|_| HollowError::VoxelStack)?;
    distances
        .ok_or(HollowError::VoxelStack)?
        .into_dimensionality()
        .map_err(|_| HollowError::VoxelStack)
}

fn marching_cavity(mask: &Array3<bool>, origin: [f64; 3], pitch: f64) -> Result<Mesh, HollowError> {
    let [nz, ny, nx]: [usize; 3] = mask
        .shape()
        .try_into()
        .map_err(|_| HollowError::VoxelStack)?;
    let padded = [
        nx.checked_add(2).ok_or(HollowError::VoxelGridTooLarge)?,
        ny.checked_add(2).ok_or(HollowError::VoxelGridTooLarge)?,
        nz.checked_add(2).ok_or(HollowError::VoxelGridTooLarge)?,
    ];
    let dimensions = [
        u32::try_from(padded[0]).map_err(|_| HollowError::VoxelGridTooLarge)?,
        u32::try_from(padded[1]).map_err(|_| HollowError::VoxelGridTooLarge)?,
        u32::try_from(padded[2]).map_err(|_| HollowError::VoxelGridTooLarge)?,
    ];
    let shape = RuntimeShape::<u32, 3>::new(dimensions);
    let max = dimensions.map(|value| value - 1);
    let padded_count = padded[0]
        .checked_mul(padded[1])
        .and_then(|value| value.checked_mul(padded[2]))
        .ok_or(HollowError::VoxelGridTooLarge)?;
    let mut sdf = vec![1.0_f32; padded_count];
    for z in 0..nz {
        for y in 0..ny {
            for x in 0..nx {
                if mask[[z, y, x]] {
                    let index = (z + 1) * padded[1] * padded[0] + (y + 1) * padded[0] + x + 1;
                    sdf[index] = -1.0;
                }
            }
        }
    }
    let mut surface = SurfaceNetsBuffer::default();
    surface_nets(&sdf, &shape, [0; 3], max, &mut surface);
    let vertices = surface
        .positions
        .into_iter()
        .map(|[x, y, z]| {
            [
                origin[0] + (f64::from(x) - 1.0) * pitch,
                origin[1] + (f64::from(y) - 1.0) * pitch,
                origin[2] + (f64::from(z) - 1.0) * pitch,
            ]
        })
        .collect();
    if surface.indices.len() % 3 != 0 {
        return Err(HollowError::VoxelStack);
    }
    let triangles = surface
        .indices
        .chunks_exact(3)
        .map(|indices| {
            let a = indices[0];
            let b = indices[1];
            let c = indices[2];
            // `fast_surface_nets` orients this negative-inside level set toward
            // the filled cavity. Manifold operands must use outward winding.
            Ok::<[u32; 3], HollowError>([a, c, b])
        })
        .collect::<Result<Vec<_>, _>>()?;
    Mesh::new(vertices, triangles).map_err(HollowError::OutputMesh)
}

fn bounds(mesh: &Mesh) -> Result<([f64; 3], [f64; 3]), HollowError> {
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for point in mesh.verts() {
        for axis in 0..3 {
            low[axis] = low[axis].min(point[axis]);
            high[axis] = high[axis].max(point[axis]);
        }
    }
    if low.iter().chain(high.iter()).all(|value| value.is_finite()) {
        Ok((low, high))
    } else {
        Err(HollowError::InvalidArithmetic)
    }
}

fn validate_printable_bounds(mesh: &Mesh) -> Result<(), HollowError> {
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for point in mesh.verts() {
        for axis in 0..3 {
            low[axis] = low[axis].min(point[axis]);
            high[axis] = high[axis].max(point[axis]);
        }
    }
    for axis in 0..3 {
        let extent = high[axis] - low[axis];
        if !extent.is_finite() {
            return Err(HollowError::InvalidArithmetic);
        }
        if extent > PRINTABLE_MAX_EXTENT_MM {
            return Err(HollowError::OutsidePrintableEnvelope { axis });
        }
    }
    Ok(())
}

fn to_solid(mesh: &Mesh) -> Result<SolidMesh, HollowError> {
    let triangles = mesh
        .tris()
        .iter()
        .map(|triangle| triangle.map(u64::from))
        .collect();
    SolidMesh::new(mesh.verts().to_vec(), triangles).map_err(HollowError::SolidMesh)
}

fn from_solid(mesh: SolidMesh) -> Result<Mesh, HollowError> {
    let triangles = mesh
        .triangles()
        .iter()
        .enumerate()
        .map(|(triangle, indices)| {
            Ok::<[u32; 3], HollowError>([
                u32::try_from(indices[0])
                    .map_err(|_| HollowError::OutputIndexOverflow { triangle })?,
                u32::try_from(indices[1])
                    .map_err(|_| HollowError::OutputIndexOverflow { triangle })?,
                u32::try_from(indices[2])
                    .map_err(|_| HollowError::OutputIndexOverflow { triangle })?,
            ])
        })
        .collect::<Result<Vec<_>, _>>()?;
    Mesh::new(mesh.vertices().to_vec(), triangles).map_err(HollowError::OutputMesh)
}

fn finite(value: f64) -> Result<Finite, HollowError> {
    Finite::new(value).map_err(|_| HollowError::InvalidArithmetic)
}

#[derive(Debug, Error)]
pub enum HollowError {
    #[error("M0 constants publication failed: {0}")]
    Constants(&'static M0ConstantsError),
    #[error(transparent)]
    Triage(#[from] TriageError),
    #[error("strict mesh audit failed: {0}")]
    StrictAudit(StrictMeshAuditError),
    #[error("topology construction failed: {0}")]
    Topology(TopologyError),
    #[error("topology validation failed: {0}")]
    TopologyValidation(TopologyValidationError),
    #[error("input is not one closed, consistently oriented solid")]
    NotClosedOrOriented,
    #[error("geometry exceeds H0's printable 300mm extent on axis {axis}")]
    OutsidePrintableEnvelope { axis: usize },
    #[error("non-finite or unrepresentable H0 arithmetic")]
    InvalidArithmetic,
    #[error("the printable voxel grid exceeds addressable storage")]
    VoxelGridTooLarge,
    #[error("resource limits require a positive peak and at least one query point")]
    InvalidResourceLimits,
    #[error("checked H0 resource accounting overflowed")]
    ResourceAccountingOverflow,
    #[error("H0 resource estimate exceeds caller cap {cap_bytes}")]
    ResourceLimitExceeded {
        estimate: HollowResourceEstimate,
        cap_bytes: usize,
    },
    #[error("geom-kernel could not construct the batched voxelization surface")]
    Kernel,
    #[error("the established EDT/label/marching-surface adapter rejected the voxel field")]
    VoxelStack,
    #[error("the model has no interior beyond the 10mm bulk threshold")]
    IneligibleBelowBulk,
    #[error("the wall offset leaves no cavity")]
    IneligibleNoCavity,
    #[error("the largest cavity does not meet the registered resin-saving threshold")]
    IneligibleSavings,
    #[error("plinth-solid mesh conversion failed: {0}")]
    SolidMesh(plinth_solid::MeshValidationError),
    #[error("plinth-solid operation failed: {0}")]
    Solid(SolidError),
    #[error("plinth-solid output index does not fit plinth-mesh at triangle {triangle}")]
    OutputIndexOverflow { triangle: usize },
    #[error("plinth-solid output cannot become a plinth-mesh: {0}")]
    OutputMesh(plinth_mesh::MeshError),
    #[error(
        "native shell did not contain one negative cavity enclosed by positive outer material"
    )]
    InvalidShellTopology,
    #[error("Boolean material volume {actual_material_mm3} differs from expected {expected_material_mm3} beyond {tolerance_mm3} mm3")]
    BooleanVolumeMismatch {
        actual_material_mm3: f64,
        expected_material_mm3: f64,
        tolerance_mm3: f64,
    },
    #[error("cavity extraction failed")]
    CavityExtractionFailed,
    #[error("drain geometry could not be constructed")]
    DrainGeometryFailed,
    #[error("the hollowed or retained-solid production mesh is invalid")]
    InvalidProductionMesh,
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_core::{ContentDigest, NonNegativeFinite, UnitFraction};
    use plinth_decimate::DecimationReport;
    use plinth_mesh::{load_binary_stl, make_box, mesh_fingerprint};
    use plinth_orient::{BaselineSelection, OrientationReport, RigidTransform};
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    fn frozen_mesh(file: &str) -> Mesh {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/baseline")
            .join(file);
        load_binary_stl(path, WeldPolicy::None).expect("committed frozen STL")
    }

    #[derive(Debug)]
    struct Characterization {
        spec: GridSpec,
        bulk_mm: f64,
        label_count: usize,
        cavity_voxels: usize,
    }

    fn characterize(input: Mesh) -> (usize, Characterization) {
        let triaged = triage_components(&input, WeldPolicy::ExactBits).expect("frozen triage");
        let components = triaged.summary().component_count();
        let source = triaged.into_mesh();
        let settings = HollowSettings::from_registered_constants().expect("M0 settings");
        let spec = grid_spec(&source, settings.voxel_pitch_mm().get()).expect("grid spec");
        resource_estimate(&source, spec, HollowResourceLimits::production())
            .expect("resource preflight");
        let grid = voxelize_filled(&source, spec).expect("bounded voxelization");
        let edt = frozen_distance_transform(&grid.material, grid.pitch).expect("EDT");
        let bulk_mm = 2.0 * edt.iter().copied().fold(0.0_f64, f64::max);
        let cavity = edt.mapv(|distance| distance > settings.wall_thickness_mm().get());
        let (labels, label_count) = label3d(cavity.view(), 6).expect("6-connected labels");
        let largest =
            (label_count > 0).then(|| largest_label(&labels, label_count).expect("largest label"));
        let cavity_voxels = largest.map_or(0, |label| {
            labels
                .iter()
                .filter(|candidate| **candidate == label)
                .count()
        });
        (
            components,
            Characterization {
                spec,
                bulk_mm,
                label_count,
                cavity_voxels,
            },
        )
    }

    #[test]
    fn frozen_oracle_characterizes_transforms_boundaries_eligibility_and_extra_body() {
        // Record of record: corpus/baseline/Sphere.pipeline.json.
        let (sphere_components, sphere) =
            characterize(frozen_mesh("Sphere.prepped.oriented-solid.stl"));
        assert_eq!(sphere_components, 1);
        assert_eq!(sphere.spec.dims, [41, 41, 41]);
        assert_eq!(sphere.spec.origin, [-10.0, -10.0, 0.0]);
        assert!((sphere.bulk_mm - 20.6).abs() <= 0.1);
        assert_eq!(sphere.label_count, 1);
        assert_eq!(sphere.cavity_voxels, 16_563);

        // Record of record: make_box is the frozen no-background boundary case.
        let (box_components, box_case) =
            characterize(frozen_mesh("make_box.prepped.oriented-solid.stl"));
        assert_eq!(box_components, 1);
        assert_eq!(box_case.spec.dims, [21, 21, 21]);
        assert_eq!(box_case.spec.origin, [-5.0, -5.0, 0.0]);
        assert!((box_case.bulk_mm - 35.2).abs() <= 0.1, "{box_case:?}");
        assert_eq!(box_case.label_count, 1);
        assert!((box_case.cavity_voxels as f64 * 0.125 / 1000.0 - 1.1).abs() <= 0.1);

        // Record of record: make_overhang_table.pipeline.json preserves four
        // authored extra bodies. Its 6.1mm source raster bulk is well below
        // the registered 10mm eligibility threshold.
        let (table_components, table) =
            characterize(frozen_mesh("make_overhang_table.prepped.stl"));
        assert_eq!(table_components, 5);
        assert!((table.bulk_mm - 6.1).abs() <= 0.1, "{table:?}");
        assert!(table.bulk_mm < 10.0);
    }

    #[test]
    fn frozen_voxel_rounding_and_exterior_fill_rules_are_independently_pinned() {
        // NumPy's `round` is nearest with half-to-even, including negatives.
        assert_eq!(round_ties_even(-2.5), -2.0);
        assert_eq!(round_ties_even(-1.5), -2.0);
        assert_eq!(round_ties_even(-0.5), 0.0);
        assert_eq!(round_ties_even(0.5), 0.0);
        assert_eq!(round_ties_even(1.5), 2.0);
        assert_eq!(round_ties_even(2.5), 2.0);

        // The complement flood uses SciPy's default face connectivity. The
        // central empty cell is only diagonally connected to the boundary,
        // so it is filled; a 26-connected flood would leave it empty.
        let mut material = vec![true; 27];
        material[0] = false;
        material[13] = false;
        fill_voxel_holes(&mut material, [3, 3, 3]).expect("bounded 6-neighbor fill");
        assert!(!material[0]);
        assert!(material[13]);
    }

    #[test]
    fn frozen_subdivision_work_is_capped_at_ten_rounds() {
        let spec = GridSpec {
            origin: [0.0, 0.0, 0.0],
            dims: [3, 3, 3],
            pitch: 0.5,
            count: 27,
        };
        let mut material = vec![false; spec.count];
        assert!(matches!(
            rasterize_subdivided_triangle(
                [0.0, 0.0, 0.0],
                [300.0, 0.0, 0.0],
                [0.0, 300.0, 0.0],
                spec,
                &mut material,
                0,
            ),
            Err(HollowError::VoxelGridTooLarge)
        ));
    }

    #[test]
    fn solid_fallback_moves_one_mesh_and_is_visible_in_history() {
        let mesh = Mesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .expect("the test triangle is a valid mesh");
        let allocation = mesh.verts().as_ptr();
        let orientation = OrientationHistory::from_reports(
            DecimationReport {
                budget_id: ContentDigest(11),
                input_faces: 1,
                output_faces: 1,
                chosen_ratio: UnitFraction::new(1.0).expect("unit fixture ratio"),
                p99_mm: NonNegativeFinite::new(0.0).expect("nonnegative fixture error"),
                p999_mm: NonNegativeFinite::new(0.0).expect("nonnegative fixture error"),
                boundary_edges_before: 3,
                boundary_edges_after: 3,
                nonmanifold_edges_before: 0,
                nonmanifold_edges_after: 0,
            },
            OrientationReport {
                pinned_report: PathBuf::from("baseline/model.pipeline.json"),
                selection: BaselineSelection::Identity,
                transform: RigidTransform::identity(),
            },
        );
        let report = HollowReport {
            disposition: HollowDisposition::SolidFallback {
                reason: SolidFallbackReason::CavityNotDrainable,
            },
            cavities: Vec::new(),
            drains: Vec::new(),
        };
        let model = HollowedModel {
            mesh,
            history: HollowHistory::from_reports(orientation, report),
        };
        let (mesh, history) = model.into_parts();
        assert_eq!(mesh.verts().as_ptr(), allocation);
        assert_eq!(
            history.hollow().disposition,
            HollowDisposition::SolidFallback {
                reason: SolidFallbackReason::CavityNotDrainable
            }
        );
    }

    fn bounds(mesh: &Mesh) -> ([f64; 3], [f64; 3]) {
        let mut low = [f64::INFINITY; 3];
        let mut high = [f64::NEG_INFINITY; 3];
        for point in mesh.verts() {
            for axis in 0..3 {
                low[axis] = low[axis].min(point[axis]);
                high[axis] = high[axis].max(point[axis]);
            }
        }
        (low, high)
    }

    fn join(left: &Mesh, right: &Mesh) -> Mesh {
        let offset = u32::try_from(left.verts().len()).expect("fixture vertex count fits u32");
        let mut vertices = left.verts().to_vec();
        vertices.extend_from_slice(right.verts());
        let mut triangles = left.tris().to_vec();
        triangles.extend(right.tris().iter().map(|triangle| {
            triangle.map(|index| {
                index
                    .checked_add(offset)
                    .expect("fixture index cannot overflow")
            })
        }));
        Mesh::new(vertices, triangles).expect("joined fixture is structurally valid")
    }

    #[test]
    fn registered_shell_is_closed_with_one_retained_2_5mm_cavity() {
        let source = frozen_mesh("Sphere.prepped.oriented-solid.stl");
        let source_bounds = bounds(&source);
        let candidate = create_shell(source).expect("frozen sphere is hollow-eligible");
        let output = candidate.mesh();
        assert_eq!(
            bounds(output),
            source_bounds,
            "outer surface must not drift"
        );
        let topology = build_face_topology(output, WeldPolicy::ExactBits).expect("topology");
        let certificate = certify_closed_components(output, &topology).expect("certificate");
        assert_eq!(
            certificate
                .components()
                .iter()
                .filter(|component| component.signed_volume_mm3() < 0.0)
                .count(),
            1,
            "the inner surface must be a negative, contained cavity"
        );
        assert_eq!(candidate.cavity().wall_thickness_mm().get(), 2.5);
        assert_eq!(candidate.cavity().frozen_voxel_pitch_mm().get(), 0.5);
        assert!(
            (1_900.0..=2_300.0).contains(&candidate.cavity().cavity_volume_mm3().get()),
            "the frozen 0.5mm EDT shell has the expected 2.5mm-wall cavity range"
        );
        assert!(
            candidate.cavity().cavity_volume_mm3().get()
                < candidate.cavity().source_volume_mm3().get()
        );
    }

    #[test]
    fn open_prehollowed_and_ineligible_meshes_refuse_without_repair() {
        let solid = make_box([50.0, 50.0, 50.0], [0.0, 0.0, 0.0]).expect("box fixture");
        let open =
            Mesh::new(solid.verts().to_vec(), solid.tris()[1..].to_vec()).expect("open fixture");
        assert!(matches!(
            create_shell(open),
            Err(HollowError::NotClosedOrOriented)
        ));

        let outer = make_box([50.0, 50.0, 50.0], [0.0, 0.0, 0.0]).expect("outer fixture");
        let inner = make_box([20.0, 20.0, 20.0], [0.0, 0.0, 15.0]).expect("inner fixture");
        assert!(matches!(
            create_shell(join(&outer, &inner)),
            Err(HollowError::Triage(TriageError::PreHollowed(_)))
        ));

        let thin = frozen_mesh("make_overhang_table.prepped.stl");
        assert!(matches!(
            create_shell(thin),
            Err(HollowError::IneligibleBelowBulk)
        ));
    }

    #[test]
    fn preserves_authored_extra_bodies_while_hollowing_only_the_largest_cavity() {
        let main = frozen_mesh("Sphere.prepped.oriented-solid.stl");
        let detail = make_box([2.0, 2.0, 2.0], [14.0, 0.0, 1.0]).expect("detail fixture");
        let candidate =
            create_shell(join(&main, &detail)).expect("extra authored body is retained");
        let topology =
            build_face_topology(candidate.mesh(), WeldPolicy::ExactBits).expect("output topology");
        let certificate =
            certify_closed_components(candidate.mesh(), &topology).expect("output certificate");
        assert_eq!(
            certificate.components().len(),
            3,
            "outer body, authored detail, and one cavity remain"
        );
    }

    #[test]
    fn resource_preflight_has_exact_cap_boundary_and_refuses_300mm_before_voxelization() {
        let huge = make_box([300.0, 300.0, 300.0], [0.0, 0.0, 0.0]).expect("300mm box");
        let error = create_shell(huge).expect_err("300mm at 0.5mm must preflight-refuse");
        match error {
            HollowError::ResourceLimitExceeded {
                estimate,
                cap_bytes,
            } => {
                assert_eq!(estimate.voxel_count(), 601_usize.pow(3));
                assert!(estimate.peak_bytes() > cap_bytes);
                assert!(estimate.grid_coordinate_bytes() <= DEFAULT_QUERY_WORKING_BYTES);
                assert!(estimate.rasterization_peak_bytes() > 0);
                assert!(estimate.output_upper_bound_bytes() > 0);
            }
            other => panic!("unexpected 300mm refusal: {other}"),
        }

        let input = frozen_mesh("Sphere.prepped.oriented-solid.stl");
        let source = triage_components(&input, WeldPolicy::ExactBits)
            .expect("sphere triage")
            .into_mesh();
        let settings = HollowSettings::from_registered_constants().expect("M0 settings");
        let spec = grid_spec(&source, settings.voxel_pitch_mm().get()).expect("grid spec");
        let query_bytes = QUERY_BYTES_PER_POINT * 127;
        let unconstrained = resource_estimate(
            &source,
            spec,
            HollowResourceLimits::new(usize::MAX, query_bytes).expect("unbounded test cap"),
        )
        .expect("estimate");
        assert_eq!(unconstrained.query_batch_points(), 127);
        assert_eq!(
            resource_estimate(
                &source,
                spec,
                HollowResourceLimits::new(unconstrained.peak_bytes(), query_bytes)
                    .expect("exact cap"),
            )
            .expect("exact boundary"),
            unconstrained
        );
        assert!(matches!(
            resource_estimate(
                &source,
                spec,
                HollowResourceLimits::new(unconstrained.peak_bytes() - 1, query_bytes)
                    .expect("below cap"),
            ),
            Err(HollowError::ResourceLimitExceeded { .. })
        ));
        assert!(matches!(
            grid_spec(&source, f64::MIN_POSITIVE),
            Err(HollowError::VoxelGridTooLarge)
        ));
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn label_phase_bound_matches_hand_derived_live_allocations() {
        // N=8 second-pass live requests: mask 8, labels 64, three N+1
        // usize arrays 216, offsets 72, and output 32 = 392 bytes.
        let estimate = label_phase_memory(8).expect("label bound");
        assert_eq!(estimate.requested_live_bytes, 392);
        // Each of those seven live allocations is rounded to 16 bytes,
        // doubled for allocator size-class slack, then charged 16 metadata
        // bytes: 48 + 144 + 3*176 + 176 + 80 = 976.
        assert_eq!(estimate.conservative_peak_bytes, 976);
        assert!(estimate.conservative_peak_bytes >= estimate.requested_live_bytes);
        assert!(matches!(
            label_phase_memory(usize::MAX),
            Err(HollowError::ResourceAccountingOverflow)
        ));
    }

    #[test]
    fn boolean_volume_invariant_accepts_shell_and_rejects_wrong_cavity_evidence() {
        let candidate = create_shell(frozen_mesh("Sphere.prepped.oriented-solid.stl"))
            .expect("frozen sphere shell");
        let evidence = *candidate.cavity();
        validate_shell_topology(
            candidate.mesh(),
            evidence.source_volume_mm3().get(),
            evidence.cavity_volume_mm3().get(),
            0.02,
        )
        .expect("measured Boolean balance");
        assert!(matches!(
            validate_shell_topology(
                candidate.mesh(),
                evidence.source_volume_mm3().get(),
                0.0,
                0.02,
            ),
            Err(HollowError::BooleanVolumeMismatch { .. })
        ));
    }

    fn thread_guard() -> &'static Mutex<()> {
        static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
        GUARD.get_or_init(|| Mutex::new(()))
    }

    struct RestoreThreads(Option<std::ffi::OsString>);

    impl Drop for RestoreThreads {
        fn drop(&mut self) {
            if let Some(value) = self.0.take() {
                std::env::set_var("PLINTH_THREADS", value);
            } else {
                std::env::remove_var("PLINTH_THREADS");
            }
        }
    }

    #[test]
    fn shell_digest_is_bit_identical_across_one_four_and_sixteen_kernel_workers() {
        let _guard = thread_guard().lock().expect("thread environment lock");
        let _restore = RestoreThreads(std::env::var_os("PLINTH_THREADS"));
        let source = frozen_mesh("Sphere.prepped.oriented-solid.stl");
        let settings = HollowSettings::from_registered_constants().expect("M0 settings");
        let mut baseline: Option<String> = None;
        for workers in [1, 4, 16] {
            std::env::set_var("PLINTH_THREADS", workers.to_string());
            assert_eq!(geom_kernel::current_threads(), workers);
            let candidate = create_shell_with_resources(
                source.clone(),
                settings,
                HollowResourceLimits::new(DEFAULT_HOLLOW_PEAK_BYTES, QUERY_BYTES_PER_POINT * 1024)
                    .expect("chunked limits"),
            )
            .expect("thread-count shell");
            let digest = mesh_fingerprint(candidate.mesh()).expect("shell fingerprint");
            if let Some(ref expected) = baseline {
                assert_eq!(
                    digest.as_str(),
                    expected.as_str(),
                    "worker count {workers} changed the shell"
                );
            } else {
                baseline = Some(digest);
            }
        }
        std::env::set_var("PLINTH_THREADS", "1");
        for query_points in [127, 4096] {
            let candidate = create_shell_with_resources(
                source.clone(),
                settings,
                HollowResourceLimits::new(
                    DEFAULT_HOLLOW_PEAK_BYTES,
                    QUERY_BYTES_PER_POINT * query_points,
                )
                .expect("chunk-size limits"),
            )
            .expect("chunk-size shell");
            let digest = mesh_fingerprint(candidate.mesh()).expect("chunk fingerprint");
            assert_eq!(digest, *baseline.as_ref().expect("worker baseline"));
            assert_eq!(
                candidate.cavity().resources().query_batch_points(),
                query_points
            );
        }
    }
}
