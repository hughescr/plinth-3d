//! The single final ChituBox emission boundary.
//!
//! This crate is deliberately only a conversion boundary.  It turns the f64
//! [`plinth_support::SupportedModel`] into `chitubox_io`'s f32 wire records,
//! writes one fresh project, then audits a *newly opened* copy of that file.
//! It neither plans supports nor uses a model-bearing template.
#![forbid(unsafe_code)]

use chitubox_io::{
    ModelGeometry, ModelSource, Placement, PlacementUpdate, Project, SegmentType, SupportBatch,
    SupportMode, SupportRecord, TriangleSoup,
};
use geom_kernel::SoupTriangle;
use plinth_core::PrinterProfile;
use plinth_mesh::Mesh;
use plinth_support::{FrustumRingNormal, SupportPrimitive, SupportSegmentKind, SupportedModel};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

const MODEL_NAME: &str = "plinth-model";
const FRUSTUM_SIDES: usize = 24;
const SPHERE_LATITUDES: usize = 8;
const SPHERE_LONGITUDES: usize = 16;
const RAFT_SIDES: usize = 20;

#[derive(Clone, Debug, PartialEq)]
pub struct EmitRequest {
    pub target_path: PathBuf,
    pub printer: PrinterProfile,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmitReport {
    pub artifact_path: PathBuf,
    pub support_record_count: usize,
    pub artifact_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum EmitError {
    #[error("cannot create ChituBox target {path}")]
    TargetUnavailable { path: PathBuf },
    #[error("ChituBox record encoding rejected the supported scene")]
    RecordEncodingRejected,
    #[error("the freshly reopened ChituBox artifact failed its disk audit")]
    DiskAuditRejected,
}

/// Write one newly-created project and derive the report from a fresh disk read.
///
/// `SupportedModel` is the only source of model and support geometry.  Its retained lift is
/// assigned to the ChituBox model placement exactly once; support vertices remain in their
/// model-local frame, where that placement applies to the complete assembly.
pub fn write(model: &SupportedModel, request: &EmitRequest) -> Result<EmitReport, EmitError> {
    validate_target(&request.target_path)?;

    let center = mesh_bbox_center(model.mesh()).ok_or(EmitError::RecordEncodingRejected)?;
    let geometry = model_triangle_soup(model.mesh(), center)?;
    let lift = model.lift_mm().get();
    let lift = f32_of(lift)?;
    let records = build_records(model.primitives(), center)?;

    let mut project = Project::new_empty();
    let index = project
        .add_model(ModelSource {
            name: MODEL_NAME.to_owned(),
            geometry: ModelGeometry::Triangles(geometry.clone()),
            placement: Placement::default(),
        })
        .map_err(|_| EmitError::RecordEncodingRejected)?;
    debug_assert_eq!(index, 0);

    // `attach_supports` is the only format-side operation which applies a lift to a model with
    // records.  For the empty support case, use the analogous placement update explicitly.
    if records.is_empty() {
        project
            .set_placements(&[PlacementUpdate {
                model_index: index,
                placement: Placement {
                    x: 0.0,
                    y: 0.0,
                    z_lift: lift,
                },
            }])
            .map_err(|_| EmitError::RecordEncodingRejected)?;
    } else {
        project
            .attach_supports(&[SupportBatch {
                model_index: index,
                records: records.clone(),
                mode: SupportMode::Replace,
                lift_mm: Some(lift),
            }])
            .map_err(|_| EmitError::RecordEncodingRejected)?;
    }
    project
        .save(&request.target_path)
        .map_err(|_| EmitError::TargetUnavailable {
            path: request.target_path.clone(),
        })?;

    audit_disk(
        &request.target_path,
        &request.printer,
        AuditExpected {
            lift,
            plate_z: f32_of(model.plate_z_mm().get() - center[2])?,
            model: &geometry,
            records: &records,
            primitives: model.primitives(),
            model_center: center,
        },
    )
}

fn validate_target(path: &Path) -> Result<(), EmitError> {
    if path.is_dir() || !path.parent().unwrap_or_else(|| Path::new(".")).is_dir() {
        return Err(EmitError::TargetUnavailable {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

struct AuditExpected<'a> {
    lift: f32,
    plate_z: f32,
    model: &'a TriangleSoup,
    records: &'a [SupportRecord],
    primitives: &'a [SupportPrimitive],
    model_center: [f64; 3],
}

fn audit_disk(
    path: &Path,
    printer: &PrinterProfile,
    expected: AuditExpected<'_>,
) -> Result<EmitReport, EmitError> {
    let project = Project::open(path).map_err(|_| EmitError::DiskAuditRejected)?;
    if project.model_count() != 1 {
        return Err(EmitError::DiskAuditRejected);
    }
    let info = project.model(0).map_err(|_| EmitError::DiskAuditRejected)?;
    if info.name != MODEL_NAME
        || !same_placement_bits(
            info.placement,
            Placement {
                x: 0.0,
                y: 0.0,
                z_lift: expected.lift,
            },
        )
    {
        return Err(EmitError::DiskAuditRejected);
    }
    let model_triangles = project
        .model_triangles(0)
        .map_err(|_| EmitError::DiskAuditRejected)?;
    if !same_f32_slice_bits(&model_triangles, &expected.model.xyz) {
        return Err(EmitError::DiskAuditRejected);
    }
    let records = project
        .supports(0)
        .map_err(|_| EmitError::DiskAuditRejected)?;
    if !same_support_records_bits(&records, expected.records) {
        return Err(EmitError::DiskAuditRejected);
    }
    if records.len() != expected.primitives.len() {
        return Err(EmitError::DiskAuditRejected);
    }
    for (record, primitive) in records.iter().zip(expected.primitives) {
        audit_sphere_record_fields(record, primitive, expected.model_center)?;
    }

    let model_local_min_z = min_z(&model_triangles)?;
    // This is the physical frame invariant frozen by the Python emitter:
    // centring moves the model and the planned plate together, while the sole
    // placement lift separates their local Z values by exactly `lift_mm`.
    let reopened_plate_z = model_local_min_z - info.placement.z_lift;
    if !same_f32_frame_value(reopened_plate_z, expected.plate_z)
        || !same_f32_frame_value(model_local_min_z - reopened_plate_z, info.placement.z_lift)
    {
        return Err(EmitError::DiskAuditRejected);
    }

    let mut bounds = WorldBounds::empty();
    bounds.extend_f32(&model_triangles, info.placement, reopened_plate_z)?;
    for record in &records {
        if !record.r_start.is_finite()
            || !record.r_end.is_finite()
            || !record
                .start
                .iter()
                .chain(record.end.iter())
                .all(|value| value.is_finite())
        {
            return Err(EmitError::DiskAuditRejected);
        }
        bounds.extend_f32(&record.mesh.xyz, info.placement, reopened_plate_z)?;
        bounds.extend_point(record.start, info.placement, reopened_plate_z)?;
        bounds.extend_point(record.end, info.placement, reopened_plate_z)?;
    }
    bounds.validate_against(printer)?;

    let artifact_bytes = std::fs::metadata(path)
        .map_err(|_| EmitError::DiskAuditRejected)?
        .len();
    Ok(EmitReport {
        artifact_path: path.to_path_buf(),
        support_record_count: records.len(),
        artifact_bytes,
    })
}

/// Independent verifier for the native sphere-record field projection.  Keep
/// this separate from the producer's `SphereGeometry::canonical_poles`: it
/// derives the expected binary32 fields from immutable semantic authority
/// after reopening the file.
fn audit_sphere_record_fields(
    record: &SupportRecord,
    primitive: &SupportPrimitive,
    model_center: [f64; 3],
) -> Result<(), EmitError> {
    let Some(sphere) = &primitive.sphere_geometry else {
        return Ok(());
    };
    let radius = primitive.start_radius_mm.get();
    let (start, end) = if let Some((center_mm, outward_axis_unit)) = sphere.tip_authority() {
        if primitive.segment_kind != SupportSegmentKind::ContactSphere {
            return Err(EmitError::DiskAuditRejected);
        }
        let center = finite_point(center_mm);
        let axis = finite_point(outward_axis_unit);
        (
            std::array::from_fn(|index| center[index] - radius * axis[index]),
            std::array::from_fn(|index| center[index] + radius * axis[index]),
        )
    } else if let Some(center_mm) = sphere.joint_center() {
        if primitive.segment_kind != SupportSegmentKind::JointSphere {
            return Err(EmitError::DiskAuditRejected);
        }
        let center = finite_point(center_mm);
        (
            [center[0], center[1], center[2] + radius],
            [center[0], center[1], center[2] - radius],
        )
    } else {
        return Err(EmitError::DiskAuditRejected);
    };
    let expected_start = point_f32(sub_center(start, model_center))?;
    let expected_end = point_f32(sub_center(end, model_center))?;
    let expected_radius = f32_of(radius)?;
    if !same_f32_slice_bits(&record.start, &expected_start)
        || !same_f32_slice_bits(&record.end, &expected_end)
        || !same_f32_bits(record.r_start, expected_radius)
        || !same_f32_bits(record.r_end, expected_radius)
    {
        return Err(EmitError::DiskAuditRejected);
    }
    Ok(())
}

/// Compare format fields, never numeric values: `PartialEq` treats +0.0 and
/// -0.0 as identical and treats all NaN payloads as unequal.  The disk audit
/// is a wire-fidelity check, so binary32 payloads are the authority.
fn same_f32_bits(actual: f32, expected: f32) -> bool {
    actual.to_bits() == expected.to_bits()
}

fn same_f32_slice_bits(actual: &[f32], expected: &[f32]) -> bool {
    actual.len() == expected.len()
        && actual
            .iter()
            .zip(expected)
            .all(|(actual, expected)| same_f32_bits(*actual, *expected))
}

fn same_placement_bits(actual: Placement, expected: Placement) -> bool {
    same_f32_bits(actual.x, expected.x)
        && same_f32_bits(actual.y, expected.y)
        && same_f32_bits(actual.z_lift, expected.z_lift)
}

fn same_support_records_bits(actual: &[SupportRecord], expected: &[SupportRecord]) -> bool {
    actual.len() == expected.len()
        && actual.iter().zip(expected).all(|(actual, expected)| {
            actual.segment_type == expected.segment_type
                && same_f32_slice_bits(&actual.start, &expected.start)
                && same_f32_slice_bits(&actual.end, &expected.end)
                && same_f32_bits(actual.r_start, expected.r_start)
                && same_f32_bits(actual.r_end, expected.r_end)
                && same_f32_slice_bits(&actual.mesh.xyz, &expected.mesh.xyz)
                // This covers type-1's depth and every other f32-bearing
                // trailing field without assuming unwritten bytes are zero.
                && actual.trailing == expected.trailing
        })
}

#[derive(Clone, Copy)]
struct Bounds {
    min: [f64; 3],
    max: [f64; 3],
}

fn min_z(values: &[f32]) -> Result<f32, EmitError> {
    if values.is_empty() || !values.len().is_multiple_of(3) {
        return Err(EmitError::DiskAuditRejected);
    }
    values
        .chunks_exact(3)
        .map(|point| point[2])
        .filter(|value| value.is_finite())
        .min_by(f32::total_cmp)
        .ok_or(EmitError::DiskAuditRejected)
}

/// Bounds in printer coordinates, never merely local mesh coordinates.
#[derive(Clone, Copy)]
struct WorldBounds(Bounds);

impl WorldBounds {
    fn empty() -> Self {
        Self(Bounds::empty())
    }

    fn extend_f32(
        &mut self,
        values: &[f32],
        placement: Placement,
        plate_z: f32,
    ) -> Result<(), EmitError> {
        if values.is_empty() || !values.len().is_multiple_of(3) {
            return Err(EmitError::DiskAuditRejected);
        }
        for point in values.chunks_exact(3) {
            self.extend_point([point[0], point[1], point[2]], placement, plate_z)?;
        }
        Ok(())
    }

    fn extend_point(
        &mut self,
        point: [f32; 3],
        placement: Placement,
        plate_z: f32,
    ) -> Result<(), EmitError> {
        let world = [
            point[0] + placement.x,
            point[1] + placement.y,
            point[2] - plate_z,
        ];
        self.0.extend_f32(&world)
    }

    fn validate_against(self, printer: &PrinterProfile) -> Result<(), EmitError> {
        self.0.validate_against(printer)
    }
}

impl Bounds {
    fn empty() -> Self {
        Self {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        }
    }

    fn extend_f32(&mut self, values: &[f32]) -> Result<(), EmitError> {
        if values.is_empty() || !values.len().is_multiple_of(3) {
            return Err(EmitError::DiskAuditRejected);
        }
        for point in values.chunks_exact(3) {
            for (axis, value) in point.iter().enumerate() {
                if !value.is_finite() {
                    return Err(EmitError::DiskAuditRejected);
                }
                let value = f64::from(*value);
                self.min[axis] = self.min[axis].min(value);
                self.max[axis] = self.max[axis].max(value);
            }
        }
        Ok(())
    }

    fn validate_against(self, printer: &PrinterProfile) -> Result<(), EmitError> {
        let volume = printer.build_volume_mm().value();
        let span = [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ];
        if !span.iter().all(|value| value.is_finite())
            || span[0] > volume.x()
            || span[1] > volume.y()
            || span[2] > volume.z()
            || self.min[0] < -volume.x() * 0.5
            || self.max[0] > volume.x() * 0.5
            || self.min[1] < -volume.y() * 0.5
            || self.max[1] > volume.y() * 0.5
            || self.min[2] < 0.0
            || self.max[2] > volume.z()
        {
            return Err(EmitError::DiskAuditRejected);
        }
        Ok(())
    }
}

fn mesh_bbox_center(mesh: &Mesh) -> Option<[f64; 3]> {
    let mut bounds = Bounds::empty();
    for vertex in mesh.verts() {
        for (axis, value) in vertex.iter().enumerate() {
            if !value.is_finite() {
                return None;
            }
            bounds.min[axis] = bounds.min[axis].min(*value);
            bounds.max[axis] = bounds.max[axis].max(*value);
        }
    }
    Some([
        (bounds.min[0] + bounds.max[0]) * 0.5,
        (bounds.min[1] + bounds.max[1]) * 0.5,
        (bounds.min[2] + bounds.max[2]) * 0.5,
    ])
}

fn model_triangle_soup(mesh: &Mesh, center: [f64; 3]) -> Result<TriangleSoup, EmitError> {
    let mut triangles = Vec::with_capacity(mesh.tris().len());
    for indices in mesh.tris() {
        let mut triangle = [[0.0; 3]; 3];
        for (corner, index) in indices.iter().enumerate() {
            let source = mesh
                .verts()
                .get(*index as usize)
                .ok_or(EmitError::RecordEncodingRejected)?;
            triangle[corner] = sub_center(*source, center);
        }
        triangles.push(triangle);
    }
    let mut soups = [triangles];
    wind_outward(&mut soups)?;
    soup_from_triangles(&soups[0])
}

/// Build every support record for a model in one pass.
///
/// Batch-first (D-37): gather every primitive's frame and raw soup, hand the
/// whole forest to one kernel winding call, then scatter into wire records.
/// There is deliberately no per-primitive winding call in this path.
fn build_records(
    primitives: &[SupportPrimitive],
    center: [f64; 3],
) -> Result<Vec<SupportRecord>, EmitError> {
    let mut frames = Vec::with_capacity(primitives.len());
    let mut soups = Vec::with_capacity(primitives.len());
    for primitive in primitives {
        let (frame, triangles) = record_frame_and_soup(primitive, center)?;
        frames.push(frame);
        soups.push(triangles);
    }
    wind_outward(&mut soups)?;
    frames
        .iter()
        .zip(&soups)
        .map(|(frame, triangles)| encode_record(frame, triangles))
        .collect()
}

/// Everything a wire record needs besides its wound triangle soup.
#[derive(Clone, Copy, Debug)]
struct RecordFrame {
    segment_kind: SupportSegmentKind,
    start: [f64; 3],
    end: [f64; 3],
    r_start: f64,
    r_end: f64,
    contact_depth_mm: Option<f64>,
}

fn record_frame_and_soup(
    primitive: &SupportPrimitive,
    center: [f64; 3],
) -> Result<(RecordFrame, Vec<SoupTriangle>), EmitError> {
    let r_start = primitive.start_radius_mm.get();
    let r_end = primitive.end_radius_mm.get();
    let (source_start, source_end) = match primitive.segment_kind {
        SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere => primitive
            .sphere_geometry
            .as_ref()
            .ok_or(EmitError::RecordEncodingRejected)?
            .canonical_poles(primitive.start_radius_mm)
            .map_err(|_| EmitError::RecordEncodingRejected)?,
        _ => (primitive.start_mm, primitive.end_mm),
    };
    let start = sub_center(finite_point(source_start), center);
    let end = sub_center(finite_point(source_end), center);
    let triangles = match primitive.segment_kind {
        // Frozen semantics: type 7 owns the contact sphere; type 9 owns a
        // standalone joint sphere; types 1 and 3 are frusta.
        SupportSegmentKind::ContactSphere => sphere_triangles(midpoint(start, end), r_start)?,
        SupportSegmentKind::JointSphere => sphere_triangles(midpoint(start, end), r_start)?,
        SupportSegmentKind::RaftPadOrBridge if same_point(start, end) => {
            raft_pad_triangles(start, r_start, r_end)?
        }
        SupportSegmentKind::RaftPadOrBridge => raft_bridge_triangles(start, end, r_start, r_end)?,
        SupportSegmentKind::ContactTaper
        | SupportSegmentKind::Member
        | SupportSegmentKind::RootFlare => frustum_triangles(
            start,
            end,
            r_start,
            r_end,
            FRUSTUM_SIDES,
            primitive.ring_normal,
        )?,
    };
    let contact_depth_mm = match primitive.segment_kind {
        SupportSegmentKind::ContactTaper => Some(
            primitive
                .contact_depth_mm
                .ok_or(EmitError::RecordEncodingRejected)?
                .get(),
        ),
        _ => None,
    };
    Ok((
        RecordFrame {
            segment_kind: primitive.segment_kind,
            start,
            end,
            r_start,
            r_end,
            contact_depth_mm,
        },
        triangles,
    ))
}

fn encode_record(
    frame: &RecordFrame,
    triangles: &[SoupTriangle],
) -> Result<SupportRecord, EmitError> {
    let mut record = SupportRecord::new(
        segment_type(frame.segment_kind),
        point_f32(frame.start)?,
        point_f32(frame.end)?,
        f32_of(frame.r_start)?,
        f32_of(frame.r_end)?,
        soup_from_triangles(triangles)?,
    );
    if let Some(depth) = frame.contact_depth_mm {
        record = record.with_contact_depth(f32_of(depth)?);
    }
    if frame.segment_kind == SupportSegmentKind::RaftPadOrBridge {
        let is_bridge = !same_point(frame.start, frame.end);
        record.trailing[0..4]
            .copy_from_slice(&(if is_bridge { 1.0_f32 } else { 0.0_f32 }).to_le_bytes());
        record.trailing[4..8].copy_from_slice(&4.0_f32.to_le_bytes());
    }
    Ok(record)
}

/// The sole exhaustive semantic-to-wire mapping.
const fn segment_type(kind: SupportSegmentKind) -> SegmentType {
    match kind {
        SupportSegmentKind::ContactSphere => SegmentType::TIP,
        SupportSegmentKind::ContactTaper => SegmentType::TAPER,
        SupportSegmentKind::JointSphere => SegmentType::SHAFT,
        SupportSegmentKind::Member => SegmentType::JOINER,
        SupportSegmentKind::RootFlare => SegmentType::FLARE,
        SupportSegmentKind::RaftPadOrBridge => SegmentType::PAD_OR_BRIDGE,
    }
}

fn finite_point(point: [plinth_core::Finite; 3]) -> [f64; 3] {
    point.map(plinth_core::Finite::get)
}

fn sub_center(point: [f64; 3], center: [f64; 3]) -> [f64; 3] {
    [
        point[0] - center[0],
        point[1] - center[1],
        point[2] - center[2],
    ]
}

fn same_point(a: [f64; 3], b: [f64; 3]) -> bool {
    a == b
}

fn midpoint(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        (a[0] + b[0]) * 0.5,
        (a[1] + b[1]) * 0.5,
        (a[2] + b[2]) * 0.5,
    ]
}

/// The disk format stores every frame coordinate as binary32.  This permits
/// only the bounded roundoff introduced by separately rounded subtraction,
/// not a changed plate or a second lift.
fn same_f32_frame_value(actual: f32, expected: f32) -> bool {
    let scale = actual.abs().max(expected.abs()).max(1.0);
    (actual - expected).abs() <= 32.0 * f32::EPSILON * scale
}

fn f32_of(value: f64) -> Result<f32, EmitError> {
    let value = value as f32;
    value
        .is_finite()
        .then_some(value)
        .ok_or(EmitError::RecordEncodingRejected)
}

fn point_f32(point: [f64; 3]) -> Result<[f32; 3], EmitError> {
    Ok([f32_of(point[0])?, f32_of(point[1])?, f32_of(point[2])?])
}

fn soup_from_triangles(triangles: &[SoupTriangle]) -> Result<TriangleSoup, EmitError> {
    let mut xyz = Vec::with_capacity(triangles.len() * 9);
    for triangle in triangles {
        for point in triangle {
            xyz.extend(point_f32(*point)?);
        }
    }
    Ok(TriangleSoup::new(xyz))
}

fn frustum_triangles(
    start: [f64; 3],
    end: [f64; 3],
    r_start: f64,
    r_end: f64,
    sides: usize,
    ring_normal: FrustumRingNormal,
) -> Result<Vec<SoupTriangle>, EmitError> {
    if sides < 3 || !r_start.is_finite() || !r_end.is_finite() {
        return Err(EmitError::RecordEncodingRejected);
    }
    let axis = sub(end, start);
    let axis_length = length(axis);
    if !axis_length.is_finite() || axis_length <= f64::EPSILON {
        return Err(EmitError::RecordEncodingRejected);
    }
    let normal = match ring_normal {
        FrustumRingNormal::SegmentAxis => scale(axis, 1.0 / axis_length),
        FrustumRingNormal::PlateZ => [0.0, 0.0, 1.0],
    };
    let (u, v) = perpendicular_basis(normal)?;
    let mut ring_start = Vec::with_capacity(sides);
    let mut ring_end = Vec::with_capacity(sides);
    for index in 0..sides {
        let angle = std::f64::consts::TAU * index as f64 / sides as f64;
        let radial = add(scale(u, angle.cos()), scale(v, angle.sin()));
        ring_start.push(add(start, scale(radial, r_start)));
        ring_end.push(add(end, scale(radial, r_end)));
    }
    let mut triangles = Vec::with_capacity(sides * 4);
    for index in 0..sides {
        let next = (index + 1) % sides;
        triangles.push([ring_start[index], ring_end[next], ring_end[index]]);
        triangles.push([ring_start[index], ring_start[next], ring_end[next]]);
        triangles.push([start, ring_start[next], ring_start[index]]);
        triangles.push([end, ring_end[index], ring_end[next]]);
    }
    Ok(triangles)
}

fn sphere_triangles(center: [f64; 3], radius: f64) -> Result<Vec<SoupTriangle>, EmitError> {
    if !radius.is_finite() || radius <= 0.0 {
        return Err(EmitError::RecordEncodingRejected);
    }
    let mut triangles = Vec::with_capacity(2 * SPHERE_LONGITUDES * (SPHERE_LATITUDES - 1));
    for latitude in 0..SPHERE_LATITUDES {
        let a0 = std::f64::consts::PI * latitude as f64 / SPHERE_LATITUDES as f64;
        let a1 = std::f64::consts::PI * (latitude + 1) as f64 / SPHERE_LATITUDES as f64;
        for longitude in 0..SPHERE_LONGITUDES {
            let l0 = std::f64::consts::TAU * longitude as f64 / SPHERE_LONGITUDES as f64;
            let l1 = std::f64::consts::TAU * ((longitude + 1) % SPHERE_LONGITUDES) as f64
                / SPHERE_LONGITUDES as f64;
            let p00 = sphere_point(center, radius, a0, l0);
            let p01 = sphere_point(center, radius, a0, l1);
            let p10 = sphere_point(center, radius, a1, l0);
            let p11 = sphere_point(center, radius, a1, l1);
            if latitude + 1 < SPHERE_LATITUDES {
                triangles.push([p00, p10, p11]);
            }
            if latitude > 0 {
                triangles.push([p00, p11, p01]);
            }
        }
    }
    Ok(triangles)
}

fn sphere_point(center: [f64; 3], radius: f64, latitude: f64, longitude: f64) -> [f64; 3] {
    [
        center[0] + radius * latitude.sin() * longitude.cos(),
        center[1] + radius * latitude.sin() * longitude.sin(),
        center[2] + radius * latitude.cos(),
    ]
}

fn raft_pad_triangles(
    start: [f64; 3],
    diameter: f64,
    height: f64,
) -> Result<Vec<SoupTriangle>, EmitError> {
    if !diameter.is_finite() || !height.is_finite() || height <= 0.0 {
        return Err(EmitError::RecordEncodingRejected);
    }
    frustum_triangles(
        start,
        add(start, [0.0, 0.0, height]),
        diameter * 0.5,
        diameter * 0.5,
        RAFT_SIDES,
        FrustumRingNormal::SegmentAxis,
    )
}

fn raft_bridge_triangles(
    start: [f64; 3],
    end: [f64; 3],
    width: f64,
    height: f64,
) -> Result<Vec<SoupTriangle>, EmitError> {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(EmitError::RecordEncodingRejected);
    }
    let axis = [end[0] - start[0], end[1] - start[1], 0.0];
    let length = length(axis);
    if !length.is_finite() || length <= f64::EPSILON {
        return Err(EmitError::RecordEncodingRejected);
    }
    let across = [
        -axis[1] / length * width * 0.5,
        axis[0] / length * width * 0.5,
        0.0,
    ];
    let lo = [
        sub(start, across),
        add(start, across),
        add(end, across),
        sub(end, across),
    ];
    let hi = lo.map(|point| add(point, [0.0, 0.0, height]));
    let vertices = [lo[0], lo[1], lo[2], lo[3], hi[0], hi[1], hi[2], hi[3]];
    const FACES: [[usize; 3]; 12] = [
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ];
    Ok(FACES
        .map(|face| [vertices[face[0]], vertices[face[1]], vertices[face[2]]])
        .to_vec())
}

fn perpendicular_basis(normal: [f64; 3]) -> Result<([f64; 3], [f64; 3]), EmitError> {
    let reference = if normal[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let u = cross(normal, reference);
    let length = length(u);
    if !length.is_finite() || length <= f64::EPSILON {
        return Err(EmitError::RecordEncodingRejected);
    }
    let u = scale(u, 1.0 / length);
    Ok((u, cross(normal, u)))
}

/// Wind a whole gathered batch of soups outward in one kernel call.
///
/// The f64 tetrahedron sum itself lives in `geom_kernel` (O-5: one
/// implementation per capability; the 05-chitubox-and-corpus.md ruling that
/// replaced Python's trimesh `mass_properties` winding check).  This crate
/// keeps only the boundary mapping onto its own error enum, because a soup a
/// signed volume cannot be stated for is exactly a record that cannot be
/// encoded.
fn wind_outward(soups: &mut [Vec<SoupTriangle>]) -> Result<(), EmitError> {
    geom_kernel::wind_soups_outward(soups)
        .map(|_| ())
        .map_err(|_| EmitError::RecordEncodingRejected)
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: [f64; 3], scalar: f64) -> [f64; 3] {
    [a[0] * scalar, a[1] * scalar, a[2] * scalar]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn length(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_core::{m0_constants, Finite, PositiveFinite};
    use plinth_decimate::{run as run_decimation, DecimationAudit, DecimationBudget, ProbeConfig};
    use plinth_hollow::{HollowHistory, HollowReport, SolidDecisionReason};
    use plinth_orient::{BaselineSelection, OrientationHistory, OrientationReport, RigidTransform};
    use plinth_support::{
        BuildFrame, FrustumRingNormal, SphereGeometry, SupportHistory, SupportReport,
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Single-primitive convenience over the batched builder.  Only tests hold
    /// exactly one primitive; production always hands over a whole forest.
    fn build_record(
        primitive: &SupportPrimitive,
        center: [f64; 3],
    ) -> Result<SupportRecord, EmitError> {
        build_records(std::slice::from_ref(primitive), center)?
            .into_iter()
            .next()
            .ok_or(EmitError::RecordEncodingRejected)
    }

    /// Triangles as f64, read back out of an already-encoded wire soup.
    fn wire_triangles(xyz: &[f32]) -> Vec<SoupTriangle> {
        xyz.chunks_exact(9)
            .map(|t| {
                [
                    [t[0] as f64, t[1] as f64, t[2] as f64],
                    [t[3] as f64, t[4] as f64, t[5] as f64],
                    [t[6] as f64, t[7] as f64, t[8] as f64],
                ]
            })
            .collect()
    }

    /// Independent orientation oracle.  Every primitive this crate synthesizes
    /// — frustum, sphere, raft pad, raft bridge — is a convex body, and every
    /// face of a convex body wound outward has its geometric normal pointing
    /// away from the body's own centroid.  This decides orientation from local
    /// face normals alone and never forms a tetrahedron sum, so it does not
    /// restate the algorithm under test.
    ///
    /// Returns `(faces_checked, faces_pointing_outward)`; degenerate faces
    /// carry no orientation and are excluded from both counts.
    fn faces_pointing_away_from_centroid(triangles: &[SoupTriangle]) -> (usize, usize) {
        let mut centroid = [0.0_f64; 3];
        let mut corners = 0.0;
        for triangle in triangles {
            for point in triangle {
                for (axis, value) in point.iter().enumerate() {
                    centroid[axis] += value;
                }
                corners += 1.0;
            }
        }
        if corners == 0.0 {
            return (0, 0);
        }
        for value in &mut centroid {
            *value /= corners;
        }
        let mut checked = 0;
        let mut outward = 0;
        for triangle in triangles {
            let normal = cross(sub(triangle[1], triangle[0]), sub(triangle[2], triangle[0]));
            if length(normal) <= 0.0 {
                continue;
            }
            checked += 1;
            let face_centroid = scale(add(add(triangle[0], triangle[1]), triangle[2]), 1.0 / 3.0);
            if dot(normal, sub(face_centroid, centroid)) > 0.0 {
                outward += 1;
            }
        }
        (checked, outward)
    }

    fn assert_convex_soup_is_wound_outward(triangles: &[SoupTriangle], what: &str) {
        let (checked, outward) = faces_pointing_away_from_centroid(triangles);
        assert!(checked > 0, "{what} carries no non-degenerate face");
        assert_eq!(
            checked,
            outward,
            "{what} has {} of {checked} faces wound inward",
            checked - outward
        );
    }

    fn fixture_path(name: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "plinth-emit-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn printer() -> PrinterProfile {
        m0_constants()
            .expect("fixture constants")
            .saturn4_ultra()
            .clone()
    }

    fn fixture_mesh() -> Mesh {
        Mesh::new(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        )
        .expect("fixture mesh")
    }

    fn fixture_decimation_audit() -> DecimationAudit {
        let mesh = plinth_mesh::make_box([10.0; 3], [0.0; 3]).expect("closed audit fixture");
        let budget =
            DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
                .expect("registered fixture budget");
        let outcome = run_decimation(&mesh, budget).expect("fixture decimation succeeds");
        let (_, audit) = outcome
            .into_decimated_model()
            .expect("fixture scale is in contract")
            .into_parts();
        audit
    }

    fn fixture_history(primitives: &[SupportPrimitive], lift_mm: f64) -> SupportHistory {
        let decimation = fixture_decimation_audit();
        let orientation = OrientationHistory::from_reports(
            decimation,
            OrientationReport {
                pinned_report: PathBuf::from("fixture.pipeline.json"),
                selection: BaselineSelection::Identity,
                transform: RigidTransform::identity(),
            },
        );
        let hollow = HollowHistory::from_reports(
            orientation,
            HollowReport::solid_decision(SolidDecisionReason::BelowThreshold),
        );
        let raft_count = primitives
            .iter()
            .filter(|primitive| primitive.segment_kind == SupportSegmentKind::RaftPadOrBridge)
            .count();
        let frame = BuildFrame::new(
            // Fixture model minimum is 0: its sole lift establishes the
            // planned plate in the same un-centred support frame.
            Finite::new(-lift_mm).unwrap(),
            PositiveFinite::new(lift_mm).unwrap(),
        );
        SupportHistory::from_reports(
            hollow,
            SupportReport {
                support_primitive_count: primitives.len() - raft_count,
                raft_primitive_count: raft_count,
            },
            frame,
        )
    }

    fn fixture_model(primitives: Vec<SupportPrimitive>, lift_mm: f64) -> SupportedModel {
        let history = fixture_history(&primitives, lift_mm);
        SupportedModel::from_parts(fixture_mesh(), primitives, history).expect("fixture scene")
    }

    fn primitive(
        segment_kind: SupportSegmentKind,
        start: [f64; 3],
        end: [f64; 3],
    ) -> SupportPrimitive {
        let start_radius_mm = PositiveFinite::new(2.2).unwrap();
        let sphere_geometry = match segment_kind {
            SupportSegmentKind::ContactSphere => {
                let midpoint = midpoint(start, end);
                Some(
                    SphereGeometry::tip(
                        midpoint.map(|value| Finite::new(value).unwrap()),
                        sub(end, start).map(|value| Finite::new(value).unwrap()),
                    )
                    .unwrap(),
                )
            }
            SupportSegmentKind::JointSphere => Some(SphereGeometry::joint(
                midpoint(start, end).map(|value| Finite::new(value).unwrap()),
            )),
            _ => None,
        };
        let (start_mm, end_mm) = sphere_geometry.as_ref().map_or_else(
            || {
                (
                    start.map(|value| Finite::new(value).unwrap()),
                    end.map(|value| Finite::new(value).unwrap()),
                )
            },
            |geometry| {
                geometry
                    .canonical_poles(start_radius_mm)
                    .expect("fixture sphere geometry remains finite")
            },
        );
        SupportPrimitive {
            segment_kind,
            start_mm,
            end_mm,
            start_radius_mm,
            end_radius_mm: PositiveFinite::new(
                if matches!(
                    segment_kind,
                    SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere
                ) {
                    2.2
                } else {
                    0.2
                },
            )
            .unwrap(),
            contact_depth_mm: (segment_kind == SupportSegmentKind::ContactTaper).then(|| {
                m0_constants()
                    .unwrap()
                    .placeholders()
                    .default_contact_depth_mm()
                    .diagnostic()
                    .to_owned()
            }),
            ring_normal: FrustumRingNormal::SegmentAxis,
            sphere_geometry,
        }
    }

    fn primitive_with_radii(
        segment_kind: SupportSegmentKind,
        start: [f64; 3],
        end: [f64; 3],
        start_radius: f64,
        end_radius: f64,
    ) -> SupportPrimitive {
        let mut primitive = primitive(segment_kind, start, end);
        primitive.start_radius_mm = PositiveFinite::new(start_radius).unwrap();
        primitive.end_radius_mm = PositiveFinite::new(end_radius).unwrap();
        if let Some(geometry) = &primitive.sphere_geometry {
            (primitive.start_mm, primitive.end_mm) = geometry
                .canonical_poles(primitive.start_radius_mm)
                .expect("fixture sphere geometry remains finite");
        }
        primitive
    }

    fn record_bounds(record: &SupportRecord) -> Bounds {
        let mut bounds = Bounds::empty();
        bounds.extend_f32(&record.mesh.xyz).unwrap();
        bounds
    }

    #[test]
    fn frozen_contact_joiner_and_joint_ball_records_keep_their_distinct_roles() {
        // This is the frozen `member_chain_specs` vocabulary: a type-7
        // contact sphere, adjacent type-1 taper carrying configured depth,
        // nonzero type-3 member frustum, and a standalone type-9 joint ball.
        let depth = m0_constants()
            .unwrap()
            .placeholders()
            .default_contact_depth_mm()
            .diagnostic()
            .get();
        let records = build_records(
            &[
                primitive_with_radii(
                    SupportSegmentKind::ContactSphere,
                    [1.0, 2.0, 3.0],
                    [1.0, 2.0, 5.0],
                    1.0,
                    1.0,
                ),
                primitive_with_radii(
                    SupportSegmentKind::ContactTaper,
                    [1.0, 2.0, 4.0],
                    [1.0, 2.0, 8.0],
                    0.8,
                    0.3,
                ),
                primitive_with_radii(
                    SupportSegmentKind::Member,
                    [1.0, 2.0, 8.0],
                    [1.0, 2.0, 11.0],
                    0.6,
                    0.4,
                ),
                primitive_with_radii(
                    SupportSegmentKind::JointSphere,
                    [1.0, 2.0, 14.0],
                    [1.0, 2.0, 10.0],
                    0.75,
                    0.75,
                ),
            ],
            [0.0; 3],
        )
        .unwrap();
        assert_eq!(records[0].segment_type, SegmentType::TIP);
        assert_eq!(records[0].start, [1.0, 2.0, 3.0]);
        assert_eq!(records[0].end, [1.0, 2.0, 5.0]);
        let tip_bounds = record_bounds(&records[0]);
        assert_eq!((tip_bounds.min[2], tip_bounds.max[2]), (3.0, 5.0));

        assert_eq!(records[1].segment_type, SegmentType::TAPER);
        assert_eq!(records[1].start, [1.0, 2.0, 4.0]);
        assert_eq!(records[1].end, [1.0, 2.0, 8.0]);
        assert_eq!(
            f32::from_le_bytes(records[1].trailing[0..4].try_into().unwrap()),
            depth as f32
        );
        let taper_bounds = record_bounds(&records[1]);
        assert_eq!((taper_bounds.min[2], taper_bounds.max[2]), (4.0, 8.0));

        assert_eq!(records[2].segment_type, SegmentType::JOINER);
        assert_eq!(records[2].start, [1.0, 2.0, 8.0]);
        assert_eq!(records[2].end, [1.0, 2.0, 11.0]);
        let joiner_bounds = record_bounds(&records[2]);
        assert_eq!((joiner_bounds.min[2], joiner_bounds.max[2]), (8.0, 11.0));

        assert_eq!(records[3].segment_type, SegmentType::SHAFT);
        assert_eq!(records[3].start, [1.0, 2.0, 12.75]);
        assert_eq!(records[3].end, [1.0, 2.0, 11.25]);
        let shaft_bounds = record_bounds(&records[3]);
        assert_eq!((shaft_bounds.min[2], shaft_bounds.max[2]), (11.25, 12.75));
    }

    #[test]
    fn plate_root_joiner_uses_frozen_horizontal_ring_normal() {
        let mut root = primitive_with_radii(
            SupportSegmentKind::Member,
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 2.0],
            0.6,
            0.4,
        );
        root.ring_normal = FrustumRingNormal::PlateZ;
        let record = build_record(&root, [0.0; 3]).unwrap();
        assert_eq!(record.start, [0.0, 0.0, 0.0]);
        assert_eq!(record.end, [1.0, 0.0, 2.0]);
        assert!(record.mesh.xyz.chunks_exact(3).all(|point| {
            point[2].to_bits() == 0.0_f32.to_bits() || point[2].to_bits() == 2.0_f32.to_bits()
        }));

        let axis_record = build_record(
            &primitive_with_radii(
                SupportSegmentKind::Member,
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 2.0],
                0.6,
                0.4,
            ),
            [0.0; 3],
        )
        .unwrap();
        assert!(axis_record
            .mesh
            .xyz
            .chunks_exact(3)
            .any(|point| point[2] != 0.0 && point[2] != 2.0));
    }

    #[test]
    fn model_winding_is_fixed_in_f64_before_wire_cast() {
        let mesh = Mesh::new(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        )
        .unwrap();
        // Same tetrahedron, every face reversed.  Emitting both pins that the
        // correction happens, not merely that one lucky input needed none.
        let reversed = Mesh::new(
            mesh.verts().to_vec(),
            mesh.tris()
                .iter()
                .map(|face| [face[0], face[2], face[1]])
                .collect(),
        )
        .unwrap();
        for (source, label) in [
            (&mesh, "the outward tetrahedron"),
            (&reversed, "the reversed one"),
        ] {
            let soup = model_triangle_soup(source, mesh_bbox_center(source).unwrap()).unwrap();
            assert_convex_soup_is_wound_outward(&wire_triangles(&soup.xyz), label);
        }
    }

    #[test]
    fn record_builder_covers_current_segment_vocabulary_in_order() {
        let primitives = vec![
            primitive(
                SupportSegmentKind::ContactSphere,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 3.0],
            ),
            primitive(
                SupportSegmentKind::ContactTaper,
                [0.0, 0.0, 3.0],
                [0.0, 0.0, 6.0],
            ),
            primitive(
                SupportSegmentKind::JointSphere,
                [0.0, 0.0, 6.0],
                [0.0, 0.0, 9.0],
            ),
            primitive(
                SupportSegmentKind::Member,
                [0.0, 0.0, 9.0],
                [0.0, 0.0, 11.0],
            ),
            primitive(
                SupportSegmentKind::RootFlare,
                [0.0, 0.0, 11.0],
                [0.0, 0.0, 12.0],
            ),
            primitive(
                SupportSegmentKind::RaftPadOrBridge,
                [0.0, 0.0, 0.0],
                [3.0, 0.0, 0.0],
            ),
            primitive(
                SupportSegmentKind::RaftPadOrBridge,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
            ),
        ];
        let records = build_records(&primitives, [0.0; 3]).unwrap();
        assert_eq!(
            records
                .iter()
                .map(|record| record.segment_type)
                .collect::<Vec<_>>(),
            primitives
                .iter()
                .map(|primitive| segment_type(primitive.segment_kind))
                .collect::<Vec<_>>()
        );
        for (index, record) in records.iter().enumerate() {
            assert!(!record.mesh.xyz.is_empty());
            assert!(record.mesh.xyz.iter().all(|value| value.is_finite()));
            assert_convex_soup_is_wound_outward(
                &wire_triangles(&record.mesh.xyz),
                &format!("record {index} ({:?})", record.segment_type),
            );
        }
    }

    /// Unmatched surface of a soup, as a fraction of its total area.
    ///
    /// The total outward area vector `Σ (v1-v0)×(v2-v0)` cancels to zero over a
    /// closed surface and equals the missing patch over an open one, so its
    /// norm over the soup's total area is a scale-free openness measure.  It is
    /// also *exactly* the term that makes the tetrahedron sum frame-dependent:
    /// translating a soup by `t` shifts the sum by `t · Σ(v1-v0)×(v2-v0) / 6`.
    /// Measuring it needs no vertex welding, which §2 of 06-conventions.md
    /// forbids as a production habit for good reason.
    fn relative_closure_defect(triangles: &[SoupTriangle]) -> f64 {
        let mut total = [0.0_f64; 3];
        let mut area = 0.0;
        for triangle in triangles {
            let normal = cross(sub(triangle[1], triangle[0]), sub(triangle[2], triangle[0]));
            for (axis, value) in normal.iter().enumerate() {
                total[axis] += value;
            }
            area += length(normal);
        }
        if area == 0.0 {
            return 0.0;
        }
        length(total) / area
    }

    /// The precondition the whole winding decision rests on.
    ///
    /// `Σ v0 · (v1 × v2) / 6` is only translation-invariant — and therefore
    /// only sign-stable — for a CLOSED soup.  Native
    /// `morel_mudskipper_supports.chitubox` proves the negative case: its
    /// merged, open records read positive or negative purely by where they sit
    /// (see `tests/native_winding_oracle.rs`).  Every soup this crate emits is
    /// one closed convex primitive, and that is what this test pins.
    ///
    /// The bound is 1e-12 rather than exactly zero for one measured reason, and
    /// it is a recorded defect rather than a slackened test: `sphere_triangles`
    /// closes its north cap on `sin(0) == 0` but its south cap on
    /// `sin(PI) == 1.2246e-16`, so the south "pole" is really a 16-gon of that
    /// radius and the sphere carries a seam of 16 sliver triangles.  The gap is
    /// ~1e-16 of the sphere's area — six orders below anything f32 emission can
    /// carry — so it changes no emitted byte and no winding decision.  It is
    /// NOT repaired here: repairing it would move emitted geometry, which this
    /// relocation is not permitted to do.
    #[test]
    fn every_emitted_soup_is_closed_which_the_winding_decision_requires() {
        let primitives = vec![
            primitive(
                SupportSegmentKind::ContactSphere,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 3.0],
            ),
            primitive_with_radii(
                SupportSegmentKind::ContactTaper,
                [0.0, 0.0, 3.0],
                [0.0, 0.0, 6.0],
                0.35,
                0.8,
            ),
            primitive(
                SupportSegmentKind::JointSphere,
                [0.0, 0.0, 6.0],
                [0.0, 0.0, 9.0],
            ),
            primitive_with_radii(
                SupportSegmentKind::Member,
                [0.0, 0.0, 9.0],
                [1.5, -2.0, 11.0],
                0.8,
                0.6,
            ),
            primitive(
                SupportSegmentKind::RootFlare,
                [0.0, 0.0, 11.0],
                [0.0, 0.0, 12.0],
            ),
            primitive(
                SupportSegmentKind::RaftPadOrBridge,
                [0.0, 0.0, 0.0],
                [3.0, 0.0, 0.0],
            ),
            primitive(
                SupportSegmentKind::RaftPadOrBridge,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
            ),
        ];
        // Off-origin on every axis: a closed soup's winding decision is
        // unchanged by the frame, an open one's is not, so a leak shows up
        // here as a disagreement between the two centers.
        let origin = build_records(&primitives, [0.0; 3]).unwrap();
        let displaced = build_records(&primitives, [-37.5, 91.25, -6.0]).unwrap();
        for (index, (near, far)) in origin.iter().zip(&displaced).enumerate() {
            for (record, what) in [(near, "origin frame"), (far, "displaced frame")] {
                let triangles = wire_triangles(&record.mesh.xyz);
                let defect = relative_closure_defect(&triangles);
                assert!(
                    defect <= 1e-12,
                    "record {index} ({:?}) in the {what} leaks {defect:e} of its area",
                    record.segment_type
                );
                assert_convex_soup_is_wound_outward(
                    &triangles,
                    &format!("record {index} in the {what}"),
                );
            }
        }
    }

    /// The batched builder must reach the same wire bytes it reached when each
    /// record was wound on its own, and must do so whatever else shares the
    /// batch.  This is the bit-identity guard on moving the sum into the
    /// kernel and the winding call out of the per-record path.
    #[test]
    fn batched_winding_matches_each_record_built_alone() {
        let primitives = vec![
            primitive(
                SupportSegmentKind::ContactSphere,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 3.0],
            ),
            primitive_with_radii(
                SupportSegmentKind::ContactTaper,
                [0.0, 0.0, 3.0],
                [0.0, 0.0, 6.0],
                0.35,
                0.8,
            ),
            primitive_with_radii(
                SupportSegmentKind::Member,
                [0.0, 0.0, 9.0],
                [1.5, -2.0, 11.0],
                0.8,
                0.6,
            ),
            primitive(
                SupportSegmentKind::RaftPadOrBridge,
                [0.0, 0.0, 0.0],
                [3.0, 0.0, 0.0],
            ),
            primitive(
                SupportSegmentKind::RaftPadOrBridge,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
            ),
        ];
        let batched = build_records(&primitives, [0.25, -0.5, 1.0]).unwrap();
        for (index, primitive) in primitives.iter().enumerate() {
            let alone = build_records(std::slice::from_ref(primitive), [0.25, -0.5, 1.0]).unwrap();
            assert!(
                same_support_records_bits(&alone, &batched[index..=index]),
                "record {index} differs between the batched and solitary builds"
            );
        }
    }

    #[test]
    fn wire_audit_rejects_signed_zero_and_nan_payload_changes() {
        // Both values can be represented by an f32 parser, but neither is the
        // same serialized field.  This is intentionally a bit test rather
        // than `assert_ne!`: normal f32 equality would miss the first case.
        assert!(!same_f32_slice_bits(&[0.0], &[f32::from_bits(0x8000_0000)]));
        assert!(!same_f32_slice_bits(
            &[f32::from_bits(0x7fc0_0001)],
            &[f32::from_bits(0x7fc0_0002)],
        ));
    }

    #[test]
    fn malformed_prewrite_geometry_is_rejected() {
        assert_eq!(
            frustum_triangles(
                [0.0; 3],
                [0.0; 3],
                1.0,
                1.0,
                FRUSTUM_SIDES,
                FrustumRingNormal::SegmentAxis,
            ),
            Err(EmitError::RecordEncodingRejected)
        );
        assert_eq!(
            raft_bridge_triangles([0.0; 3], [0.0; 3], 2.2, 0.2),
            Err(EmitError::RecordEncodingRejected)
        );
    }

    #[test]
    fn emitter_handoff_accepts_coincident_raft_pad_only_after_semantic_validation() {
        for kind in [
            SupportSegmentKind::ContactTaper,
            SupportSegmentKind::Member,
            SupportSegmentKind::RootFlare,
        ] {
            let primitive = primitive(kind, [0.0; 3], [0.0; 3]);
            let history = fixture_history(std::slice::from_ref(&primitive), 3.0);
            assert!(matches!(
                SupportedModel::from_parts(fixture_mesh(), vec![primitive], history),
                Err(plinth_support::SupportError::InvalidPrimitive)
            ));
        }

        let model = fixture_model(
            vec![primitive(
                SupportSegmentKind::RaftPadOrBridge,
                [0.0; 3],
                [0.0; 3],
            )],
            3.0,
        );
        let records = build_records(model.primitives(), [0.0; 3])
            .expect("validated coincident raft primitive emits a pad record");
        assert_eq!(records[0].segment_type, SegmentType::PAD_OR_BRIDGE);
    }

    #[test]
    fn semantic_segment_mapping_is_exhaustive_and_pins_native_ids() {
        let expected = [
            (SupportSegmentKind::ContactSphere, 7),
            (SupportSegmentKind::ContactTaper, 1),
            (SupportSegmentKind::JointSphere, 9),
            (SupportSegmentKind::Member, 3),
            (SupportSegmentKind::RootFlare, 4),
            (SupportSegmentKind::RaftPadOrBridge, 6),
        ];
        for (kind, id) in expected {
            assert_eq!(segment_type(kind), SegmentType(id));
        }
    }

    #[test]
    fn native_chitubox_fixture_pins_real_support_record_semantics() {
        const BASIC: &[u8] = include_bytes!(
            "../../../scripts/tests/fixtures/chitubox/basic-cube-supports-raft.chitubox"
        );
        const CYLINDER: &[u8] = include_bytes!(
            "../../../scripts/tests/fixtures/chitubox/cylinder-supported-bottom-connected.chitubox"
        );
        for bytes in [BASIC, CYLINDER] {
            let project = Project::from_bytes(bytes.to_vec()).expect("native required fixture");
            assert!(project
                .supports(0)
                .unwrap()
                .iter()
                .any(|record| record.segment_type == SegmentType::PAD_OR_BRIDGE));
        }
        const TWO_SUPPORTS: &[u8] =
            include_bytes!("../../../scripts/tests/fixtures/chitubox/two-supports.chitubox");
        let project = Project::from_bytes(TWO_SUPPORTS.to_vec()).expect("native two-support donor");
        let records = project.supports(0).expect("native donor supports");
        // These are independently transcribed raw binary32 values from the
        // authoritative UI-authored donor.  They deliberately do not call
        // our emitter or sphere projection code: this fixture is the native
        // semantic oracle for type 7/1/9/3/4 records.
        type NativeRecord = (u32, [u32; 3], [u32; 3], [u32; 2], usize, Option<u32>);
        const EXPECTED: &[NativeRecord] = &[
            (
                7,
                [0x3fe6_ff88, 0xc0c0_b36b, 0xc104_2b50],
                [0x3fef_1cf0, 0xc0c8_45aa, 0xc106_f0c3],
                [0x3e19_9999, 0x3e19_9999],
                4320,
                None,
            ),
            (
                1,
                [0x3feb_0e3c, 0xc0c4_7c8a, 0xc105_8e0a],
                [0x4010_93cf, 0xc0f6_f631, 0xc118_07b1],
                [0x3dcc_cccd, 0x3e99_999a],
                540,
                Some(0x3dcc_cccd),
            ),
            (
                9,
                [0x4010_93cf, 0xc0f6_f631, 0xc113_09bd],
                [0x4010_93cf, 0xc0f6_f631, 0xc11d_05a5],
                [0x3e9f_be80, 0x3e9f_be80],
                4320,
                None,
            ),
            (
                7,
                [0x40e6_b696, 0xc08f_aaea, 0xc106_a1b7],
                [0x40e6_b696, 0xc08f_aaea, 0xc126_a1b7],
                [0x3f80_0000, 0x3f80_0000],
                4320,
                None,
            ),
            (
                3,
                [0x4010_93cf, 0xc0f6_f631, 0xc118_07b1],
                [0x4010_93cf, 0xc0f6_f631, 0xc19c_fcf1],
                [0x3e99_999a, 0x3e99_999a],
                540,
                None,
            ),
            (
                4,
                [0x4010_93cf, 0xc0f6_f631, 0xc19c_fae5],
                [0x4010_93cf, 0xc0f6_f631, 0xc1a0_fae5],
                [0x3e99_999a, 0x3f40_0000],
                540,
                None,
            ),
            (
                3,
                [0x40f8_add7, 0xc0d0_6c7d, 0xc12e_a643],
                [0x40f8_add7, 0xc0d0_6c7d, 0xc198_fefe],
                [0x3f40_0000, 0x3f40_0000],
                540,
                None,
            ),
            (
                4,
                [0x40f8_add7, 0xc0d0_6c7d, 0xc198_fae5],
                [0x40f8_add7, 0xc0d0_6c7d, 0xc1a0_fae5],
                [0x3f40_0000, 0x3f8c_cccd],
                540,
                None,
            ),
            (
                9,
                [0x40f8_add7, 0xc0d0_6c7d, 0xc122_2b62],
                [0x40f8_add7, 0xc0d0_6c7d, 0xc13b_2124],
                [0x3f47_ae10, 0x3f47_ae10],
                4320,
                None,
            ),
            (
                1,
                [0x40e6_b696, 0xc08f_aaea, 0xc116_a1b7],
                [0x40f8_add7, 0xc0d0_6c7d, 0xc12e_a643],
                [0x3e99_999a, 0x3f40_0000],
                540,
                Some(0x3f00_0000),
            ),
        ];
        let semantic_records: Vec<_> = records
            .iter()
            .filter(|record| record.segment_type != SegmentType::PAD_OR_BRIDGE)
            .collect();
        assert_eq!(semantic_records.len(), EXPECTED.len());
        for (record, &(segment_type, start, end, radii, mesh_len, contact_depth)) in
            semantic_records.into_iter().zip(EXPECTED)
        {
            assert_eq!(record.segment_type, SegmentType(segment_type));
            assert_eq!(record.start.map(f32::to_bits), start);
            assert_eq!(record.end.map(f32::to_bits), end);
            assert_eq!([record.r_start.to_bits(), record.r_end.to_bits()], radii);
            assert_eq!(record.mesh.xyz.len(), mesh_len);
            assert_eq!(
                contact_depth.map(
                    |_| f32::from_le_bytes(record.trailing[..4].try_into().unwrap()).to_bits()
                ),
                contact_depth
            );
        }
    }

    #[test]
    fn writes_and_reopens_one_fresh_supported_artifact_in_exact_record_order() {
        let primitives = vec![
            primitive(
                SupportSegmentKind::ContactSphere,
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 3.0],
            ),
            primitive(
                SupportSegmentKind::ContactTaper,
                [0.0, 0.0, 3.0],
                [0.0, 0.0, 5.0],
            ),
            primitive(
                SupportSegmentKind::JointSphere,
                [0.0, 0.0, 5.0],
                [0.0, 0.0, 8.0],
            ),
            primitive(SupportSegmentKind::Member, [0.0, 0.0, 8.0], [0.0, 0.0, 9.0]),
            primitive(
                SupportSegmentKind::RootFlare,
                [0.0, 0.0, 9.0],
                [0.0, 0.0, 10.0],
            ),
            primitive(
                SupportSegmentKind::RaftPadOrBridge,
                [0.0, 0.0, 0.0],
                [3.0, 0.0, 0.0],
            ),
            primitive(
                SupportSegmentKind::RaftPadOrBridge,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
            ),
        ];
        let expected_types: Vec<_> = primitives
            .iter()
            .map(|primitive| segment_type(primitive.segment_kind))
            .collect();
        let model = fixture_model(primitives, 3.0);
        let target = fixture_path("all-types.chitubox");
        let report = write(
            &model,
            &EmitRequest {
                target_path: target.clone(),
                printer: printer(),
            },
        )
        .expect("fresh write and disk audit");
        assert_eq!(report.artifact_path, target);
        assert_eq!(report.support_record_count, expected_types.len());
        assert!(report.artifact_bytes > 0);

        let reopened = Project::open(&target).expect("reopen emitted file");
        assert_eq!(reopened.model_count(), 1);
        assert_eq!(reopened.model(0).unwrap().placement.z_lift, 3.0);
        let records = reopened.supports(0).unwrap();
        assert_eq!(
            records
                .iter()
                .map(|record| record.segment_type)
                .collect::<Vec<_>>(),
            expected_types
        );
        assert!(records
            .iter()
            .flat_map(|record| record.mesh.xyz.iter())
            .all(|value| value.is_finite()));
        std::fs::remove_file(target).expect("remove unique fixture artifact");
    }

    #[test]
    fn writes_zero_support_scene_and_applies_retained_lift_once() {
        let model = fixture_model(Vec::new(), 3.0);
        let target = fixture_path("zero-supports.chitubox");
        let report = write(
            &model,
            &EmitRequest {
                target_path: target.clone(),
                printer: printer(),
            },
        )
        .expect("zero-support write");
        assert_eq!(report.support_record_count, 0);
        let reopened = Project::open(&target).unwrap();
        assert_eq!(reopened.supports(0).unwrap(), Vec::new());
        assert_eq!(reopened.model(0).unwrap().placement.z_lift, 3.0);
        std::fs::remove_file(target).expect("remove unique fixture artifact");
    }

    #[test]
    fn deterministic_emission_is_byte_identical() {
        let model = fixture_model(
            vec![primitive(
                SupportSegmentKind::JointSphere,
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 4.0],
            )],
            3.0,
        );
        let first = fixture_path("determinism-a.chitubox");
        let second = fixture_path("determinism-b.chitubox");
        for target_path in [&first, &second] {
            write(
                &model,
                &EmitRequest {
                    target_path: target_path.clone(),
                    printer: printer(),
                },
            )
            .expect("deterministic fixture write");
        }
        assert_eq!(
            std::fs::read(&first).unwrap(),
            std::fs::read(&second).unwrap()
        );
        std::fs::remove_file(first).expect("remove first deterministic fixture");
        std::fs::remove_file(second).expect("remove second deterministic fixture");
    }

    #[test]
    fn target_directory_is_rejected_before_any_write() {
        let model = fixture_model(Vec::new(), 3.0);
        let target = fixture_path("target-dir");
        std::fs::create_dir(&target).unwrap();
        let result = write(
            &model,
            &EmitRequest {
                target_path: target.clone(),
                printer: printer(),
            },
        );
        assert_eq!(
            result,
            Err(EmitError::TargetUnavailable {
                path: target.clone()
            })
        );
        std::fs::remove_dir(target).expect("remove unique fixture directory");
    }

    #[test]
    fn fresh_disk_corruption_fails_without_repairing_or_rewriting() {
        let model = fixture_model(Vec::new(), 3.0);
        let target = fixture_path("corruption.chitubox");
        write(
            &model,
            &EmitRequest {
                target_path: target.clone(),
                printer: printer(),
            },
        )
        .unwrap();
        let corrupt = vec![0_u8; 9];
        std::fs::write(&target, &corrupt).unwrap();
        let mesh = fixture_mesh();
        let expected_model = model_triangle_soup(&mesh, mesh_bbox_center(&mesh).unwrap()).unwrap();
        assert_eq!(
            audit_disk(
                &target,
                &printer(),
                AuditExpected {
                    lift: 3.0,
                    plate_z: -3.5,
                    model: &expected_model,
                    records: &[],
                    primitives: &[],
                    model_center: [0.0; 3],
                },
            ),
            Err(EmitError::DiskAuditRejected)
        );
        assert_eq!(
            std::fs::read(&target).unwrap(),
            corrupt,
            "audit must not repair or rewrite disk bytes"
        );
        std::fs::remove_file(target).expect("remove unique fixture artifact");
    }

    #[test]
    fn disk_audit_rejects_support_geometry_below_the_planned_plate() {
        let model = fixture_model(
            vec![primitive(
                SupportSegmentKind::Member,
                [0.0, 0.0, -4.0],
                [0.0, 0.0, -3.1],
            )],
            3.0,
        );
        let target = fixture_path("below-plate.chitubox");
        assert_eq!(
            write(
                &model,
                &EmitRequest {
                    target_path: target.clone(),
                    printer: printer()
                }
            ),
            Err(EmitError::DiskAuditRejected)
        );
        std::fs::remove_file(target).unwrap();
    }

    #[test]
    fn disk_audit_rejects_support_geometry_above_printer_height() {
        let model = fixture_model(
            vec![primitive(
                SupportSegmentKind::Member,
                [0.0, 0.0, 221.0],
                [0.0, 0.0, 222.0],
            )],
            3.0,
        );
        let target = fixture_path("above-height.chitubox");
        assert_eq!(
            write(
                &model,
                &EmitRequest {
                    target_path: target.clone(),
                    printer: printer()
                }
            ),
            Err(EmitError::DiskAuditRejected)
        );
        std::fs::remove_file(target).unwrap();
    }

    #[test]
    fn disk_audit_rejects_a_reopened_lift_or_plate_frame_mismatch() {
        let model = fixture_model(Vec::new(), 3.0);
        let target = fixture_path("lift-mismatch.chitubox");
        write(
            &model,
            &EmitRequest {
                target_path: target.clone(),
                printer: printer(),
            },
        )
        .unwrap();
        let mesh = fixture_mesh();
        let expected_model = model_triangle_soup(&mesh, mesh_bbox_center(&mesh).unwrap()).unwrap();
        assert_eq!(
            audit_disk(
                &target,
                &printer(),
                AuditExpected {
                    lift: 3.0,
                    plate_z: -3.4,
                    model: &expected_model,
                    records: &[],
                    primitives: &[],
                    model_center: [0.0; 3],
                },
            ),
            Err(EmitError::DiskAuditRejected)
        );
        std::fs::remove_file(target).unwrap();
    }
}
