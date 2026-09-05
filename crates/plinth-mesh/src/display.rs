//! Canonical, mesh-bound **display-face** field contract.
//!
//! This module owns the immutable authority and result shape; the batched
//! evaluator that fills it lives in [`crate::display_eval`].  The split is the
//! same one [`crate::delicacy`] makes, for the same reason: the contract can be
//! read without reading the geometry.
//!
//! # What a display face is
//!
//! Owner rulings, 2026-09-03.  A **display face** is a face carrying detail we
//! would prefer not to scar: the top of a base (the bottom sits on the table),
//! a character's face, ornate carving, surface texture.  It is NOT the same
//! quantity as delicacy: delicacy asks *would a support break this*, display
//! asks *would a support spoil the look of this*.  The two travel side by side
//! and are published side by side; neither is expressed in terms of the other.
//!
//! # Two signals, one confound, and the frame they are measured in
//!
//! * **Visibility from the authored upper hemisphere.**  The owner's second
//!   named signal is that "top" matters more than "bottom" along the model's
//!   AUTHORED Z axis.  Here that is measured, not assumed: the cosine-weighted
//!   fraction of the upper hemisphere from which the face is unoccluded.  A
//!   face whose authored normal points straight down scores exactly 0 by
//!   construction, with no cutoff to choose.
//! * **Detail density.**  The owner's first named signal is local low cosine
//!   similarity among neighbouring triangle normals — faces that articulate a
//!   lot carry denser detail.  Measured over a neighbourhood of radius
//!   `R_detail` on the surface.
//! * **The tip confound.**  The tips of pointy protrusions (rivet nubs, sword
//!   tips, hilt bottoms, fingertips) also have sharply varying normals and are
//!   the BEST places to land supports.  They are separated geometrically, not
//!   by taste, and by an IDENTITY rather than a comparison: the TOTAL
//!   Gaussian curvature of the neighbourhood — the sum of the welded angle
//!   defects [`crate::delicacy_eval`] already measures — is `2π` for a
//!   neighbourhood that has closed over into a tip and `≈ 0` for a
//!   near-planar patch however violently it articulates, because Gauss-Bonnet
//!   makes a texture's convex crests and concave troughs cancel.  Carving,
//!   grooves and texture are therefore not discounted at all, and a tip is
//!   discounted in proportion to how far it has closed over.
//!
//! **The frame is the authored one, and that is structural.**  The field is
//! measured on the mesh as the source file supplied it, and the orientation
//! search never rotates a mesh (`plinth-orient`'s D-4: a candidate is a build
//! direction `u` in model frame, and the scoring table is orientation
//! invariant).  So the mesh an [`crate::display::DisplayField`] is bound to IS
//! the authored frame, and there is no code path that could recompute this
//! field in a posed frame — the same structural argument that makes the
//! delicacy field safe, reused rather than re-argued.
//!
//! # What this field is NOT
//!
//! It is not a measure of importance, beauty, or owner intent.  It is a
//! geometric proxy with two named inputs and one named confound, and its whole
//! output is published per model before it steers anything.

use plinth_core::m0_constants;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::delicacy::{DelicacyError, FaceTable};
use crate::{Mesh, TopologyError};

pub const DISPLAY_FIELD_SCHEMA: &str = "plinth-mesh/display-field";
pub const DISPLAY_FIELD_SCHEMA_VERSION: u32 = 1;

/// The registered quantities one display evaluation is measured against.
///
/// Three numbers, and every one of them is read from the constants registry:
/// two registered `Placeholder`s and one length inherited from a `Measured`
/// entry.  Nothing here is a literal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplaySettings {
    view_directions: usize,
    detail_radius_mm: f64,
}

impl DisplaySettings {
    /// The production settings, read from the M0 registry.
    ///
    /// * `view_directions` = `DISPLAY_VIEW_DIRECTIONS` (Placeholder, a
    ///   quadrature resolution).
    /// * `detail_radius_mm` = `DISPLAY_DETAIL_TIP_MULTIPLE ×
    ///   TIP_RADIUS_MAX_GROWN_SCAR_MM` (Placeholder × Measured): `R_detail`,
    ///   the neighbourhood radius both the detail density and the tip
    ///   discount are measured over.  The LENGTH is inherited from a Measured
    ///   owner field datum — the radius of the largest scar one support tip
    ///   may leave — so `R_detail` is a stated multiple of the worst scar,
    ///   not a number chosen for this field.
    ///
    /// # Errors
    ///
    /// [`DisplayError::Constants`] when the registry is unavailable, and
    /// [`DisplayError::InvalidSetting`] when a registered value cannot make a
    /// usable setting (a zero direction count, a non-finite radius).
    pub fn production() -> Result<Self, DisplayError> {
        let constants = m0_constants().map_err(|error| DisplayError::Constants(error.to_string()))?;
        let multiple = constants
            .placeholders()
            .display_detail_tip_multiple()
            .diagnostic()
            .get();
        let directions = constants
            .placeholders()
            .display_view_directions()
            .diagnostic()
            .get();
        let scar_radius_mm = *constants.evidence().tip_radius_max_grown_scar_mm().value();
        Self::new(
            usize::try_from(directions).unwrap_or(usize::MAX),
            multiple * scar_radius_mm,
        )
    }

    /// # Errors
    ///
    /// [`DisplayError::InvalidSetting`] when a value is not usable: a zero
    /// direction count would leave visibility undefined, and a non-positive or
    /// non-finite radius would leave the neighbourhood undefined.
    pub fn new(view_directions: usize, detail_radius_mm: f64) -> Result<Self, DisplayError> {
        if view_directions == 0 {
            return Err(DisplayError::InvalidSetting {
                name: "view_directions",
                value: 0.0,
            });
        }
        if !detail_radius_mm.is_finite() || detail_radius_mm <= 0.0 {
            return Err(DisplayError::InvalidSetting {
                name: "detail_radius_mm",
                value: detail_radius_mm,
            });
        }
        Ok(Self {
            view_directions,
            detail_radius_mm,
        })
    }

    #[must_use]
    pub const fn view_directions(self) -> usize {
        self.view_directions
    }
    #[must_use]
    pub const fn detail_radius_mm(self) -> f64 {
        self.detail_radius_mm
    }
}

/// Resource guards for one display evaluation.
///
/// These are limits, not policy: **nothing here can change a display value.**
/// The evaluator either fits under the guard and computes the field exactly, or
/// it refuses naming both the measured demand and the cap.
///
/// `index_grid_pitch_mm` is the shared geometry kernel's parity-grid pitch, and
/// it is here rather than in [`DisplaySettings`] precisely because it is not a
/// display quantity: every visibility answer comes from the kernel's BVH walk,
/// and only the parity grid reads the pitch, so it cannot move a value.  It is
/// `THIN_FEATURE_MM`, the one registered length already proven affordable as a
/// pitch across this corpus by the delicacy evaluator, rather than a second
/// number to keep aligned with the first — and deliberately NOT `R_detail`,
/// which is roughly half of it and would raise the grid cost eightfold for a
/// buffer no display value reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayBuildConfig {
    pub max_index_bytes: usize,
    pub index_grid_pitch_mm: f64,
}

impl DisplayBuildConfig {
    /// The established guard set, scaled to the mesh it will be asked to
    /// score: `max(64 MiB, 2 KiB × faces)`, the same rule
    /// [`crate::delicacy::DelicacyBuildConfig::production`] states and for the
    /// same reason — this evaluator builds the same kernel index.
    ///
    /// # Errors
    ///
    /// [`DisplayError::Constants`] when the registry is unavailable.
    pub fn production(face_count: usize) -> Result<Self, DisplayError> {
        const MINIMUM_BYTES: usize = 64 * 1024 * 1024;
        const BYTES_PER_FACE: usize = 2 * 1024;
        let scaled = BYTES_PER_FACE.saturating_mul(face_count);
        let pitch = m0_constants()
            .map_err(|error| DisplayError::Constants(error.to_string()))?
            .placeholders()
            .thin_feature_mm()
            .diagnostic()
            .get();
        Ok(Self {
            max_index_bytes: if scaled > MINIMUM_BYTES {
                scaled
            } else {
                MINIMUM_BYTES
            },
            index_grid_pitch_mm: pitch,
        })
    }

    /// # Errors
    ///
    /// [`DisplayError::InvalidResourceLimit`] when a guard is zero or a pitch
    /// is not a usable length, either of which would refuse every mesh.
    pub fn validate(self) -> Result<(), DisplayError> {
        if self.max_index_bytes == 0 {
            return Err(DisplayError::InvalidResourceLimit {
                name: "max_index_bytes",
            });
        }
        if !self.index_grid_pitch_mm.is_finite() || self.index_grid_pitch_mm <= 0.0 {
            return Err(DisplayError::InvalidResourceLimit {
                name: "index_grid_pitch_mm",
            });
        }
        Ok(())
    }
}

/// One face's display measurement, with each input published beside the
/// product so a reader can see WHICH signal carried a face.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayFaceValue {
    visibility: f64,
    detail: f64,
    tip_factor: f64,
    weight: f64,
}

impl DisplayFaceValue {
    pub(crate) fn new(
        visibility: f64,
        detail: f64,
        tip_factor: f64,
        weight: f64,
    ) -> Result<Self, DisplayError> {
        for (field, value) in [
            ("visibility", visibility),
            ("detail", detail),
            ("tip_factor", tip_factor),
            ("weight", weight),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(DisplayError::InvalidFaceValue { field, value });
            }
        }
        Ok(Self {
            visibility: canonical_zero(visibility),
            detail: canonical_zero(detail),
            tip_factor: canonical_zero(tip_factor),
            weight: canonical_zero(weight),
        })
    }

    /// Cosine-weighted unoccluded fraction of the AUTHORED upper hemisphere.
    #[must_use]
    pub const fn visibility(self) -> f64 {
        self.visibility
    }
    /// Mean `1 − cos` against neighbouring face normals over `R_detail`.
    #[must_use]
    pub const fn detail(self) -> f64 {
        self.detail
    }
    /// `1 − min(1, K/2π)`, where `K` is the TOTAL Gaussian curvature of the
    /// `R_detail` neighbourhood: `1` on carving, grooves and texture (whose
    /// crests and troughs cancel to `K ≈ 0`) and `0` on a neighbourhood that
    /// has closed all the way over into a protrusion tip.
    #[must_use]
    pub const fn tip_factor(self) -> f64 {
        self.tip_factor
    }
    /// `visibility × detail × tip_factor`, the value D that the orientation
    /// objective reads.
    #[must_use]
    pub const fn weight(self) -> f64 {
        self.weight
    }
}

/// The display field for one `(mesh, LOD)` pair, bound to that pair exactly as
/// a [`crate::delicacy::DelicacyField`] is.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayField {
    table: FaceTable,
    settings: DisplaySettings,
    values: Vec<DisplayFaceValue>,
    digest: String,
}

impl DisplayField {
    /// Sole evaluator boundary: batch geometry emits one source-order face
    /// value; consumers only ever read the resulting weights.
    pub(crate) fn from_face_values(
        table: FaceTable,
        settings: DisplaySettings,
        values: Vec<DisplayFaceValue>,
    ) -> Result<Self, DisplayError> {
        if values.len() != table.face_count() {
            return Err(DisplayError::FaceValueCount {
                expected: table.face_count(),
                actual: values.len(),
            });
        }
        let digest = digest(&table, settings, &values);
        Ok(Self {
            table,
            settings,
            values,
            digest,
        })
    }

    /// # Errors
    ///
    /// [`DisplayError::Delicacy`] when `mesh` is not the mesh this field was
    /// measured on — the `FaceTable` binding is shared with the delicacy
    /// field, so its refusal is reported verbatim rather than restated.
    pub fn validate_mesh(&self, mesh: &Mesh) -> Result<(), DisplayError> {
        self.table.validate_mesh(mesh).map_err(DisplayError::Delicacy)
    }
    #[must_use]
    pub fn face_table(&self) -> &FaceTable {
        &self.table
    }
    #[must_use]
    pub const fn settings(&self) -> DisplaySettings {
        self.settings
    }
    #[must_use]
    pub fn face_values(&self) -> &[DisplayFaceValue] {
        &self.values
    }
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

fn digest(table: &FaceTable, settings: DisplaySettings, values: &[DisplayFaceValue]) -> String {
    let mut hash = Sha256::new();
    hash.update(DISPLAY_FIELD_SCHEMA.as_bytes());
    hash.update(DISPLAY_FIELD_SCHEMA_VERSION.to_le_bytes());
    hash.update(table.mesh_fingerprint().as_bytes());
    hash.update(table.lod().as_bytes());
    hash.update((table.face_count() as u64).to_le_bytes());
    hash.update((settings.view_directions as u64).to_le_bytes());
    hash.update(settings.detail_radius_mm.to_bits().to_le_bytes());
    for value in values {
        for value in [
            value.visibility,
            value.detail,
            value.tip_factor,
            value.weight,
        ] {
            hash.update(value.to_bits().to_le_bytes());
        }
    }
    format!("{:x}", hash.finalize())
}

const fn canonical_zero(value: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        value
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum DisplayError {
    #[error("registered display constants are unavailable: {0}")]
    Constants(String),
    #[error("display setting {name} is unusable, got {value}")]
    InvalidSetting { name: &'static str, value: f64 },
    #[error("display resource limit {name} must be positive")]
    InvalidResourceLimit { name: &'static str },
    #[error("display {what} charges {charge} bytes against a {cap}-byte cap")]
    IndexBudgetExceeded {
        charge: usize,
        cap: usize,
        what: &'static str,
    },
    #[error("display geometry kernel refused the mesh or the batch: {0}")]
    Kernel(String),
    #[error("display face values require {expected} source-order rows, got {actual}")]
    FaceValueCount { expected: usize, actual: usize },
    #[error("display face value {field} must be finite and within [0, 1], got {value}")]
    InvalidFaceValue { field: &'static str, value: f64 },
    #[error("display field binding: {0}")]
    Delicacy(DelicacyError),
    #[error("display mesh topology error: {0}")]
    Topology(TopologyError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::make_box_default;

    #[test]
    fn settings_and_resource_boundaries_are_named() {
        assert!(matches!(
            DisplaySettings::new(0, 0.8),
            Err(DisplayError::InvalidSetting {
                name: "view_directions",
                ..
            })
        ));
        assert!(matches!(
            DisplaySettings::new(8, f64::NAN),
            Err(DisplayError::InvalidSetting { .. })
        ));
        assert!(matches!(
            DisplaySettings::new(8, 0.0),
            Err(DisplayError::InvalidSetting { .. })
        ));
        assert!(DisplaySettings::new(8, 0.8).is_ok());
        assert!(matches!(
            DisplayBuildConfig {
                max_index_bytes: 0,
                index_grid_pitch_mm: 1.5,
            }
            .validate(),
            Err(DisplayError::InvalidResourceLimit { .. })
        ));
        assert!(matches!(
            DisplayBuildConfig {
                max_index_bytes: 1,
                index_grid_pitch_mm: 0.0,
            }
            .validate(),
            Err(DisplayError::InvalidResourceLimit { .. })
        ));
        assert!(matches!(
            DisplayFaceValue::new(0.0, 1.5, 1.0, 0.0),
            Err(DisplayError::InvalidFaceValue { .. })
        ));
    }

    /// The production settings are the registry's, arithmetically, and the
    /// arithmetic is stated here so a registry drift is visible as a failure
    /// rather than as a silently different field.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "exact pins over registered constants; a drift must fail, not round away"
    )]
    fn production_settings_are_the_registered_values() {
        let settings = DisplaySettings::production().expect("production display settings");
        let constants = plinth_core::m0_constants().expect("registry");
        let scar = *constants.evidence().tip_radius_max_grown_scar_mm().value();
        let multiple = constants
            .placeholders()
            .display_detail_tip_multiple()
            .diagnostic()
            .get();
        assert_eq!(settings.detail_radius_mm(), multiple * scar);
        assert_eq!(scar, 0.400);
        assert_eq!(settings.detail_radius_mm(), 0.800);
        assert_eq!(settings.view_directions(), 64);
    }

    #[test]
    fn field_is_face_ordered_fingerprint_bound_and_digest_stable() {
        let mesh = make_box_default().unwrap();
        let build = || {
            let table = FaceTable::bind(&mesh, "source-lod").unwrap();
            let settings = DisplaySettings::new(8, 0.8).unwrap();
            let values =
                vec![DisplayFaceValue::new(0.5, 0.25, 1.0, 0.125).unwrap(); table.face_count()];
            DisplayField::from_face_values(table, settings, values).unwrap()
        };
        let first = build();
        assert_eq!(first.digest(), build().digest());
        assert_eq!(first.face_values().len(), mesh.tris().len());
        assert_eq!(first.validate_mesh(&mesh), Ok(()));
        let other = crate::make_thin_fin().unwrap();
        assert!(matches!(
            first.validate_mesh(&other),
            Err(DisplayError::Delicacy(_))
        ));
        let table = FaceTable::bind(&mesh, "source-lod").unwrap();
        assert!(matches!(
            DisplayField::from_face_values(table, first.settings(), vec![]),
            Err(DisplayError::FaceValueCount { .. })
        ));
    }
}
