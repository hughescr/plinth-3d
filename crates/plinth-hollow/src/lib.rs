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

use fast_surface_nets::ndshape::RuntimeShape;
use fast_surface_nets::{surface_nets, SurfaceNetsBuffer};
use geom_kernel::{NativeSurface, NativeSurfaceConfig};
use plinth_core::{m0_constants, Finite, M0ConstantsError, PrinterProfile};
use plinth_mesh::{
    build_face_topology, triage_components, validate_face_topology, Mesh, StrictMeshAuditError,
    TopologyError, TopologyValidationError, TriageError, WeldPolicy,
};
use plinth_metrics::{analyze_topology, Mesh as AuditMesh, ShellWinding};
use plinth_orient::OrientationHistory;
use plinth_solid::{ManifoldBackend, Mesh as SolidMesh, SolidBoolean, SolidError};
use scirs2_core::ndarray::Array3;
use scirs2_ndimage::{distance_transform_edt, label, Connectivity};
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
}

fn finite_settings(
    wall_thickness_mm: f64,
    voxel_pitch_mm: f64,
    bulk_min_mm: f64,
    save_min_mm3: f64,
    save_min_fraction: f64,
) -> Result<HollowSettings, HollowError> {
    let finite = |value| Finite::new(value).map_err(|_| HollowError::InvalidArithmetic);
    Ok(HollowSettings {
        wall_thickness_mm: finite(wall_thickness_mm)?,
        voxel_pitch_mm: finite(voxel_pitch_mm)?,
        bulk_min_mm: finite(bulk_min_mm)?,
        save_min_mm3: finite(save_min_mm3)?,
        save_min_fraction: finite(save_min_fraction)?,
    })
}

/// Evidence that describes the sole inner cavity retained by H0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CavityEvidence {
    wall_thickness_mm: Finite,
    frozen_voxel_pitch_mm: Finite,
    cavity_volume_mm3: Finite,
    source_volume_mm3: Finite,
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
}

/// The H0 hand-off to H1. It owns only the closed shell mesh and its one
/// retained cavity; it cannot be mistaken for drilled, printable output.
#[derive(Debug)]
pub struct ShellCandidate {
    mesh: Mesh,
    cavity: CavityEvidence,
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

    #[must_use]
    pub fn into_parts(self) -> (Mesh, CavityEvidence) {
        (self.mesh, self.cavity)
    }
}

/// Convert a triaged, strict oriented solid into a single shell
/// mesh. H0 does not add drains and therefore does not expose `run`.
pub fn create_shell(input: Mesh) -> Result<ShellCandidate, HollowError> {
    let settings = HollowSettings::from_registered_constants()?;
    create_shell_with_settings(input, settings)
}

/// Testable H0 implementation with explicit settings. Production callers use
/// [`create_shell`], which is the only route that reads the registry.
pub fn create_shell_with_settings(
    input: Mesh,
    settings: HollowSettings,
) -> Result<ShellCandidate, HollowError> {
    let triaged = triage_components(&input, WeldPolicy::ExactBits)?;
    let source = triaged.into_mesh();
    validate_closed_mesh(&source)?;
    validate_printable_bounds(&source)?;
    let source_volume_mm3 = source_volume_mm3(&source)?;
    let grid = voxelize_filled(&source, settings.voxel_pitch_mm.get())?;
    let (edt, _) = distance_transform_edt(
        &grid.material.clone().into_dyn(),
        Some(&[grid.pitch; 3]),
        true,
        false,
    )
    .map_err(|_| HollowError::VoxelStack)?;
    let edt: Array3<f64> = edt
        .ok_or(HollowError::VoxelStack)?
        .into_dimensionality()
        .map_err(|_| HollowError::VoxelStack)?;
    let bulk_mm = 2.0 * edt.iter().copied().fold(0.0_f64, f64::max);
    if !bulk_mm.is_finite() {
        return Err(HollowError::InvalidArithmetic);
    }
    if bulk_mm < settings.bulk_min_mm.get() {
        return Err(HollowError::IneligibleBelowBulk);
    }
    let cavity_mask = edt.mapv(|distance_mm| distance_mm > settings.wall_thickness_mm.get());
    let (labels, label_count) = label(&cavity_mask, None, Some(Connectivity::Face), None)
        .map_err(|_| HollowError::VoxelStack)?;
    if label_count == 0 {
        return Err(HollowError::IneligibleNoCavity);
    }
    let largest = largest_label(&labels, label_count)?;
    let main = labels.mapv(|label| label == largest);
    let cavity_voxels = main.iter().filter(|value| **value).count();
    let cavity_volume_mm3 = (cavity_voxels as f64) * grid.pitch.powi(3);
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

    let inner = marching_cavity(&main, grid.origin, grid.pitch)?;
    let shell = ManifoldBackend
        .difference(&[to_solid(&source)?, to_solid(&inner)?])
        .map_err(HollowError::Solid)?;
    let mesh = from_solid(shell)?;
    validate_shell_topology(&mesh)?;
    validate_printable_bounds(&mesh)?;
    let cavity = CavityEvidence {
        wall_thickness_mm: settings.wall_thickness_mm,
        frozen_voxel_pitch_mm: settings.voxel_pitch_mm,
        cavity_volume_mm3: finite(cavity_volume_mm3)?,
        source_volume_mm3: finite(source_volume_mm3)?,
    };
    Ok(ShellCandidate { mesh, cavity })
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

fn validate_shell_topology(mesh: &Mesh) -> Result<(), HollowError> {
    validate_closed_mesh(mesh)?;
    let audit = audit_mesh(mesh)?;
    let analysis = analyze_topology(&audit).map_err(|_| HollowError::InvalidShellTopology)?;
    let negative = analysis
        .shells
        .iter()
        .filter(|shell| shell.signed_volume < 0.0)
        .count();
    if !analysis.boundary_edges.is_empty()
        || !analysis.nonmanifold_edges.is_empty()
        || !analysis.inconsistent_winding_faces.is_empty()
        || analysis.cavities != 1
        || negative != 1
        || analysis.shells.len() < 2
    {
        return Err(HollowError::InvalidShellTopology);
    }
    Ok(())
}

fn source_volume_mm3(mesh: &Mesh) -> Result<f64, HollowError> {
    let analysis =
        analyze_topology(&audit_mesh(mesh)?).map_err(|_| HollowError::NotClosedOrOriented)?;
    if analysis.boundary_edges.is_empty()
        && analysis.nonmanifold_edges.is_empty()
        && analysis.inconsistent_winding_faces.is_empty()
        && analysis.shells.iter().all(|shell| {
            shell.signed_volume > 0.0
                && matches!(shell.winding, ShellWinding::Outward { ref faces_requiring_flip } if faces_requiring_flip.is_empty())
        })
    {
        let volume = analysis.shells.iter().map(|shell| shell.signed_volume).sum::<f64>();
        if volume.is_finite() && volume > 0.0 {
            return Ok(volume);
        }
    }
    Err(HollowError::NotClosedOrOriented)
}

fn audit_mesh(mesh: &Mesh) -> Result<AuditMesh, HollowError> {
    let triangles = mesh
        .tris()
        .iter()
        .map(|triangle| triangle.map(|index| index as usize))
        .collect();
    AuditMesh::new(mesh.verts().to_vec(), triangles).map_err(|_| HollowError::InvalidShellTopology)
}

struct VoxelGrid {
    origin: [f64; 3],
    pitch: f64,
    material: Array3<bool>,
}

fn voxelize_filled(mesh: &Mesh, pitch: f64) -> Result<VoxelGrid, HollowError> {
    if !pitch.is_finite() || pitch <= 0.0 {
        return Err(HollowError::InvalidArithmetic);
    }
    let (low, high) = bounds(mesh)?;
    let mut origin = [0.0; 3];
    let mut dims = [0_usize; 3];
    for axis in 0..3 {
        origin[axis] = (low[axis] / pitch).floor() * pitch - pitch;
        let span = high[axis] - origin[axis] + pitch;
        if !origin[axis].is_finite() || !span.is_finite() || span <= 0.0 {
            return Err(HollowError::InvalidArithmetic);
        }
        let axis_samples = (span / pitch).ceil() as usize;
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
    let mut points = Vec::with_capacity(count);
    for z in 0..dims[2] {
        for y in 0..dims[1] {
            for x in 0..dims[0] {
                points.push([
                    origin[0] + x as f64 * pitch,
                    origin[1] + y as f64 * pitch,
                    origin[2] + z as f64 * pitch,
                ]);
            }
        }
    }
    let surface = NativeSurface::from_indexed(
        mesh.verts(),
        mesh.tris(),
        NativeSurfaceConfig {
            grid_pitch_mm: pitch,
            max_grid_bytes: usize::MAX,
        },
    )
    .map_err(|_| HollowError::Kernel)?;
    let inside = surface.inside(&points);
    let material = Array3::from_shape_vec((dims[2], dims[1], dims[0]), inside)
        .map_err(|_| HollowError::VoxelGridTooLarge)?;
    Ok(VoxelGrid {
        origin,
        pitch,
        material,
    })
}

fn largest_label(labels: &Array3<usize>, label_count: usize) -> Result<usize, HollowError> {
    let mut counts = vec![
        0_usize;
        label_count
            .checked_add(1)
            .ok_or(HollowError::VoxelGridTooLarge)?
    ];
    for label in labels {
        if *label > label_count {
            return Err(HollowError::VoxelStack);
        }
        counts[*label] = counts[*label]
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
        Ok(label)
    }
}

fn marching_cavity(mask: &Array3<bool>, origin: [f64; 3], pitch: f64) -> Result<Mesh, HollowError> {
    let [nz, ny, nx]: [usize; 3] = mask
        .shape()
        .try_into()
        .map_err(|_| HollowError::VoxelStack)?;
    let dimensions = [
        u32::try_from(nx).map_err(|_| HollowError::VoxelGridTooLarge)?,
        u32::try_from(ny).map_err(|_| HollowError::VoxelGridTooLarge)?,
        u32::try_from(nz).map_err(|_| HollowError::VoxelGridTooLarge)?,
    ];
    let shape = RuntimeShape::<u32, 3>::new(dimensions);
    let max = dimensions.map(|value| value - 1);
    let sdf = mask
        .iter()
        .map(|inside| if *inside { -1.0_f32 } else { 1.0_f32 })
        .collect::<Vec<_>>();
    let mut surface = SurfaceNetsBuffer::default();
    surface_nets(&sdf, &shape, [0; 3], max, &mut surface);
    let vertices = surface
        .positions
        .into_iter()
        .map(|[x, y, z]| {
            [
                origin[0] + f64::from(x) * pitch,
                origin[1] + f64::from(y) * pitch,
                origin[2] + f64::from(z) * pitch,
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
            Ok::<[u32; 3], HollowError>([a, b, c])
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
    use plinth_mesh::{make_box, mesh_fingerprint};
    use plinth_orient::{BaselineSelection, OrientationReport, RigidTransform};
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

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
        let source = make_box([50.0, 50.0, 50.0], [0.0, 0.0, 0.0]).expect("box fixture");
        let source_bounds = bounds(&source);
        let candidate = create_shell(source).expect("50mm solid is hollow-eligible");
        let output = candidate.mesh();
        assert_eq!(
            bounds(output),
            source_bounds,
            "outer surface must not drift"
        );
        validate_shell_topology(output).expect("closed, signed, enclosed cavity");
        let analysis =
            analyze_topology(&audit_mesh(output).expect("audit mesh")).expect("topology analysis");
        assert_eq!(analysis.cavities, 1);
        assert_eq!(
            analysis
                .shells
                .iter()
                .filter(|shell| shell.signed_volume < 0.0)
                .count(),
            1,
            "the inner surface must be a negative, contained cavity"
        );
        assert_eq!(candidate.cavity().wall_thickness_mm().get(), 2.5);
        assert_eq!(candidate.cavity().frozen_voxel_pitch_mm().get(), 0.5);
        assert!(
            (80_000.0..=100_000.0).contains(&candidate.cavity().cavity_volume_mm3().get()),
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

        let thin = make_box([8.0, 8.0, 8.0], [0.0, 0.0, 0.0]).expect("thin fixture");
        assert!(matches!(
            create_shell(thin),
            Err(HollowError::IneligibleBelowBulk)
        ));
    }

    #[test]
    fn preserves_authored_extra_bodies_while_hollowing_only_the_largest_cavity() {
        let main = make_box([50.0, 50.0, 50.0], [0.0, 0.0, 0.0]).expect("main fixture");
        let detail = make_box([5.0, 5.0, 5.0], [35.0, 0.0, 0.0]).expect("detail fixture");
        let candidate =
            create_shell(join(&main, &detail)).expect("extra authored body is retained");
        let analysis = analyze_topology(&audit_mesh(candidate.mesh()).expect("audit mesh"))
            .expect("topology analysis");
        assert_eq!(
            analysis.cavities, 1,
            "only H0's largest eligible cavity is hollowed"
        );
        assert_eq!(
            analysis.shells.len(),
            3,
            "outer body, authored detail, and one cavity remain"
        );
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
        let source = make_box([50.0, 50.0, 50.0], [0.0, 0.0, 0.0]).expect("box fixture");
        let mut baseline: Option<String> = None;
        for workers in [1, 4, 16] {
            std::env::set_var("PLINTH_THREADS", workers.to_string());
            assert_eq!(geom_kernel::current_threads(), workers);
            let candidate = create_shell(source.clone()).expect("thread-count shell");
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
    }
}
