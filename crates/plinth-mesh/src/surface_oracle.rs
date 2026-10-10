//! Serial-oracle comparison of the two parallel surface walks, for the witness
//! example (`examples/surface_field_witness.rs`). Compiled only with the
//! `serial-oracle` feature; never part of a production build.
//!
//! The oracle bodies are the walks as they stood before the parallel spread
//! (2026-10-09), kept in `delicacy_eval` and `display_eval`. This module runs
//! both versions on one mesh and reports, per field, whether the bits agree.

use sha2::{Digest, Sha256};

use crate::delicacy::DelicacySettings;
use crate::delicacy_eval::{curvature_by_face_serial, curvature_parallel, FaceGeometryPass};
use crate::display_eval::compare_neighbourhood_passes;
use crate::Mesh;

/// One field compared parallel against serial.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldComparison {
    /// Which field.
    pub name: &'static str,
    /// SHA-256 of the field's `f64::to_bits` values, little-endian, face order
    /// (first 12 hex digits).
    pub parallel_digest: String,
    /// The same for the serial oracle.
    pub serial_digest: String,
    /// Index of the first face whose bits differ, if any.
    pub first_difference: Option<usize>,
}

impl FieldComparison {
    fn new(name: &'static str, parallel: &[f64], serial: &[f64]) -> Self {
        let first_difference = if parallel.len() == serial.len() {
            parallel
                .iter()
                .zip(serial)
                .position(|(a, b)| a.to_bits() != b.to_bits())
        } else {
            Some(parallel.len().min(serial.len()))
        };
        Self {
            name,
            parallel_digest: digest(parallel),
            serial_digest: digest(serial),
            first_difference,
        }
    }

    /// True when the two fields are equal bit for bit.
    #[must_use]
    pub fn identical(&self) -> bool {
        self.first_difference.is_none()
    }
}

fn digest(values: &[f64]) -> String {
    let mut hasher = Sha256::new();
    for value in values {
        hasher.update(value.to_bits().to_le_bytes());
    }
    let mut text = String::with_capacity(12);
    for byte in hasher.finalize().iter().take(6) {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// Every comparison and counter of one mesh.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceFieldReport {
    /// Face count of the mesh.
    pub faces: usize,
    /// Delicacy curvature per face.
    pub curvature: FieldComparison,
    /// Display detail per face.
    pub display_detail: FieldComparison,
    /// Display neighbourhood curvature per face.
    pub display_curvature: FieldComparison,
    /// Welded vertices spread (parallel == serial is checked by the caller).
    pub vertices_spread: usize,
    /// Serial oracle's `vertices_spread`.
    pub serial_vertices_spread: usize,
    /// `(vertex, face)` pairs measured.
    pub pairs_measured: usize,
    /// Serial oracle's `pairs_measured`.
    pub serial_pairs_measured: usize,
    /// Pairs charged.
    pub pairs_charged: usize,
    /// Serial oracle's `pairs_charged`.
    pub serial_pairs_charged: usize,
    /// Faces with at least two curvature addends.
    pub faces_multiply_charged: usize,
    /// Curvature blocks (remainders included).
    pub blocks: usize,
    /// Curvature waves.
    pub waves: usize,
    /// Curvature pair-cap overflows.
    pub overflow_splits: usize,
    /// Non-degenerate faces.
    pub non_degenerate_faces: usize,
    /// Display walks run.
    pub faces_walked: usize,
    /// Faces found inside their walk's neighbourhood, over all walks.
    pub members_total: usize,
}

impl SurfaceFieldReport {
    /// True when all three fields agree bit for bit and the work counts match.
    #[must_use]
    pub fn all_identical(&self) -> bool {
        self.curvature.identical()
            && self.display_detail.identical()
            && self.display_curvature.identical()
            && self.vertices_spread == self.serial_vertices_spread
            && self.pairs_measured == self.serial_pairs_measured
            && self.pairs_charged == self.serial_pairs_charged
    }
}

/// Runs the parallel walks (in the kernel pool at the current `PLINTH_THREADS`)
/// and the serial oracles on `mesh`.
///
/// # Errors
///
/// The text of the first evaluator error.
pub fn compare_surface_fields(mesh: &Mesh) -> Result<SurfaceFieldReport, String> {
    let settings = DelicacySettings::production().map_err(|error| error.to_string())?;
    let geometry = FaceGeometryPass::run(mesh);
    let radius = settings.thin_feature_mm();
    let (parallel_curvature, outcome) =
        curvature_parallel(mesh, &geometry, radius).map_err(|error| error.to_string())?;
    let (serial_curvature, serial_counts) =
        curvature_by_face_serial(mesh, &geometry, radius).map_err(|error| error.to_string())?;
    let display = compare_neighbourhood_passes(mesh).map_err(|error| error.to_string())?;
    Ok(SurfaceFieldReport {
        faces: mesh.tris().len(),
        curvature: FieldComparison::new("curvature", &parallel_curvature, &serial_curvature),
        display_detail: FieldComparison::new(
            "display_detail",
            &display.parallel_detail,
            &display.serial_detail,
        ),
        display_curvature: FieldComparison::new(
            "display_curvature",
            &display.parallel_curvature,
            &display.serial_curvature,
        ),
        vertices_spread: outcome.counts.vertices_spread,
        serial_vertices_spread: serial_counts.vertices_spread,
        pairs_measured: outcome.counts.pairs_measured,
        serial_pairs_measured: serial_counts.pairs_measured,
        pairs_charged: outcome.counts.pairs_charged,
        serial_pairs_charged: serial_counts.pairs_charged,
        faces_multiply_charged: outcome.faces_multiply_charged,
        blocks: outcome.waves.blocks,
        waves: outcome.waves.waves,
        overflow_splits: outcome.waves.overflow_splits,
        non_degenerate_faces: display.non_degenerate_faces,
        faces_walked: display.faces_walked,
        members_total: display.members_total,
    })
}
