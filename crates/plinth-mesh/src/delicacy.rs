//! Canonical, mesh-bound delicacy field contract.
//!
//! This module owns the immutable authority and result shape.  The batched
//! evaluator that fills it lives in [`crate::delicacy_eval`]; the split is
//! deliberate, so the contract can be read without reading the geometry.

use plinth_core::m0_constants;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{mesh_fingerprint, Mesh, TopologyError};

pub const DELICACY_FIELD_SCHEMA: &str = "plinth-mesh/delicacy-field";
pub const DELICACY_FIELD_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DelicacySettings {
    curvature_weight: f64,
    thinness_weight: f64,
    thin_feature_mm: f64,
}

impl DelicacySettings {
    pub fn production() -> Result<Self, DelicacyError> {
        let values = m0_constants()
            .map_err(|e| DelicacyError::Constants(e.to_string()))?
            .placeholders();
        Self::new(
            values.delicacy_curvature_weight().diagnostic().get(),
            values.delicacy_thinness_weight().diagnostic().get(),
            values.thin_feature_mm().diagnostic().get(),
        )
    }
    #[cfg(not(test))]
    fn new(
        curvature_weight: f64,
        thinness_weight: f64,
        thin_feature_mm: f64,
    ) -> Result<Self, DelicacyError> {
        Self::checked(curvature_weight, thinness_weight, thin_feature_mm)
    }
    #[cfg(test)]
    pub(crate) fn new(
        curvature_weight: f64,
        thinness_weight: f64,
        thin_feature_mm: f64,
    ) -> Result<Self, DelicacyError> {
        Self::checked(curvature_weight, thinness_weight, thin_feature_mm)
    }
    fn checked(
        curvature_weight: f64,
        thinness_weight: f64,
        thin_feature_mm: f64,
    ) -> Result<Self, DelicacyError> {
        for (name, value) in [
            ("curvature_weight", curvature_weight),
            ("thinness_weight", thinness_weight),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(DelicacyError::InvalidSetting { name, value });
            }
        }
        if !thin_feature_mm.is_finite() || thin_feature_mm <= 0.0 {
            return Err(DelicacyError::InvalidSetting {
                name: "thin_feature_mm",
                value: thin_feature_mm,
            });
        }
        Ok(Self {
            curvature_weight: canonical_zero(curvature_weight),
            thinness_weight: canonical_zero(thinness_weight),
            thin_feature_mm: canonical_zero(thin_feature_mm),
        })
    }
    #[must_use]
    pub const fn curvature_weight(self) -> f64 {
        self.curvature_weight
    }
    #[must_use]
    pub const fn thinness_weight(self) -> f64 {
        self.thinness_weight
    }
    #[must_use]
    pub const fn thin_feature_mm(self) -> f64 {
        self.thin_feature_mm
    }
}

/// Resource guards for one delicacy evaluation.
///
/// These are limits, not policy: nothing here can change a delicacy value.
/// The evaluator either fits under the guard and computes the field exactly,
/// or it refuses naming both the measured demand and the cap — it never
/// coarsens the evaluation to fit, and it never samples, so there is no seed
/// and no sample count to configure.  That is why these fields are not
/// registry members: everything output-changing in this crate is a registered
/// Placeholder read through [`DelicacySettings::production`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DelicacyBuildConfig {
    /// Ceiling on the shared geometry kernel's conservative peak index memory
    /// for the mesh being scored, charged before any index is allocated.
    pub max_index_bytes: usize,
}

impl DelicacyBuildConfig {
    /// The established guard set, scaled to the mesh it will be asked to
    /// score.
    ///
    /// `max(64 MiB, 2 KiB × faces)` follows the support stage's own
    /// `max(floor, per_face × face_count)` idiom: the floor covers a mini
    /// whose index is dominated by fixed structure, the per-face term covers
    /// a model the size of the treant, and the measured per-face cost of the
    /// kernel's face store plus BVH is several times under 2 KiB, so the
    /// guard refuses only genuinely unreasonable geometry.  Saturating, so an
    /// absurd face count yields `usize::MAX` rather than wrapping to a tiny
    /// cap.
    #[must_use]
    pub const fn production(face_count: usize) -> Self {
        const FLOOR_BYTES: usize = 64 * 1024 * 1024;
        const BYTES_PER_FACE: usize = 2 * 1024;
        let scaled = BYTES_PER_FACE.saturating_mul(face_count);
        Self {
            max_index_bytes: if scaled > FLOOR_BYTES {
                scaled
            } else {
                FLOOR_BYTES
            },
        }
    }

    /// # Errors
    ///
    /// [`DelicacyError::InvalidResourceLimit`] when a guard is zero, which
    /// would refuse every mesh including an empty one.
    pub const fn validate(self) -> Result<(), DelicacyError> {
        if self.max_index_bytes == 0 {
            return Err(DelicacyError::InvalidResourceLimit {
                name: "max_index_bytes",
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaceTable {
    fingerprint: String,
    lod: String,
    face_count: usize,
}
impl FaceTable {
    pub fn bind(mesh: &Mesh, lod: impl Into<String>) -> Result<Self, DelicacyError> {
        let lod = lod.into();
        if lod.is_empty() {
            return Err(DelicacyError::EmptyLod);
        }
        Ok(Self {
            fingerprint: mesh_fingerprint(mesh).map_err(DelicacyError::Topology)?,
            lod,
            face_count: mesh.tris().len(),
        })
    }
    pub fn validate_mesh(&self, mesh: &Mesh) -> Result<(), DelicacyError> {
        let actual = mesh_fingerprint(mesh).map_err(DelicacyError::Topology)?;
        if self.fingerprint != actual {
            return Err(DelicacyError::MeshFingerprintMismatch {
                expected: self.fingerprint.clone(),
                actual,
            });
        }
        if self.face_count != mesh.tris().len() {
            return Err(DelicacyError::FaceOrderMismatch {
                expected: self.face_count,
                actual: mesh.tris().len(),
            });
        }
        Ok(())
    }
    #[must_use]
    pub fn mesh_fingerprint(&self) -> &str {
        &self.fingerprint
    }
    #[must_use]
    pub fn lod(&self) -> &str {
        &self.lod
    }
    #[must_use]
    pub const fn face_count(&self) -> usize {
        self.face_count
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DelicacyFaceValue {
    curvature: f64,
    thinness: f64,
    weight: f64,
}
impl DelicacyFaceValue {
    pub(crate) fn new(curvature: f64, thinness: f64, weight: f64) -> Result<Self, DelicacyError> {
        for (field, value) in [
            ("curvature", curvature),
            ("thinness", thinness),
            ("weight", weight),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(DelicacyError::InvalidFaceValue { field, value });
            }
        }
        Ok(Self {
            curvature: canonical_zero(curvature),
            thinness: canonical_zero(thinness),
            weight: canonical_zero(weight),
        })
    }
    #[must_use]
    pub const fn curvature(self) -> f64 {
        self.curvature
    }
    #[must_use]
    pub const fn thinness(self) -> f64 {
        self.thinness
    }
    #[must_use]
    pub const fn weight(self) -> f64 {
        self.weight
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DelicacyField {
    table: FaceTable,
    settings: DelicacySettings,
    values: Vec<DelicacyFaceValue>,
    digest: String,
}
impl DelicacyField {
    /// Sole evaluator boundary: batch geometry emits one source-order
    /// face value; support only reads the resulting weights.
    pub(crate) fn from_face_values(
        table: FaceTable,
        settings: DelicacySettings,
        values: Vec<DelicacyFaceValue>,
    ) -> Result<Self, DelicacyError> {
        if values.len() != table.face_count {
            return Err(DelicacyError::FaceValueCount {
                expected: table.face_count,
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
    pub fn validate_mesh(&self, mesh: &Mesh) -> Result<(), DelicacyError> {
        self.table.validate_mesh(mesh)
    }
    #[must_use]
    pub fn face_table(&self) -> &FaceTable {
        &self.table
    }
    #[must_use]
    pub const fn settings(&self) -> DelicacySettings {
        self.settings
    }
    #[must_use]
    pub fn face_values(&self) -> &[DelicacyFaceValue] {
        &self.values
    }
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

fn digest(table: &FaceTable, settings: DelicacySettings, values: &[DelicacyFaceValue]) -> String {
    let mut hash = Sha256::new();
    hash.update(DELICACY_FIELD_SCHEMA.as_bytes());
    hash.update(DELICACY_FIELD_SCHEMA_VERSION.to_le_bytes());
    hash.update(table.fingerprint.as_bytes());
    hash.update(table.lod.as_bytes());
    hash.update((table.face_count as u64).to_le_bytes());
    for value in [
        settings.curvature_weight,
        settings.thinness_weight,
        settings.thin_feature_mm,
    ] {
        hash.update(value.to_bits().to_le_bytes());
    }
    for value in values {
        for value in [value.curvature, value.thinness, value.weight] {
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
pub enum DelicacyError {
    #[error("registered delicacy constants are unavailable: {0}")]
    Constants(String),
    #[error("delicacy setting {name} must be finite and in range, got {value}")]
    InvalidSetting { name: &'static str, value: f64 },
    #[error("delicacy resource limit {name} must be positive")]
    InvalidResourceLimit { name: &'static str },
    #[error("delicacy {what} charges {charge} bytes against a {cap}-byte cap")]
    IndexBudgetExceeded {
        charge: usize,
        cap: usize,
        what: &'static str,
    },
    #[error("delicacy geometry kernel refused the mesh or the batch: {0}")]
    Kernel(String),
    #[error("delicacy LOD identity must not be empty")]
    EmptyLod,
    #[error("delicacy face values require {expected} source-order rows, got {actual}")]
    FaceValueCount { expected: usize, actual: usize },
    #[error("delicacy face value {field} must be finite and within [0, 1], got {value}")]
    InvalidFaceValue { field: &'static str, value: f64 },
    #[error("delicacy field mesh fingerprint mismatch: expected {expected}, got {actual}")]
    MeshFingerprintMismatch { expected: String, actual: String },
    #[error("delicacy field face order/count mismatch: expected {expected}, got {actual}")]
    FaceOrderMismatch { expected: usize, actual: usize },
    #[error("delicacy mesh topology error: {0}")]
    Topology(TopologyError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::make_box_default;

    fn field(mesh: &Mesh) -> DelicacyField {
        let table = FaceTable::bind(mesh, "source-lod").unwrap();
        let settings = DelicacySettings::new(0.5, 0.5, 1.5).unwrap();
        let values = vec![DelicacyFaceValue::new(0.0, 0.0, 0.0).unwrap(); table.face_count()];
        DelicacyField::from_face_values(table, settings, values).unwrap()
    }

    #[test]
    fn settings_and_resource_boundaries_are_named() {
        assert!(matches!(
            DelicacySettings::new(f64::NAN, 0.0, 1.0),
            Err(DelicacyError::InvalidSetting { .. })
        ));
        assert!(matches!(
            DelicacySettings::new(0.0, 0.0, 0.0),
            Err(DelicacyError::InvalidSetting { .. })
        ));
        assert!(matches!(
            DelicacyBuildConfig { max_index_bytes: 0 }.validate(),
            Err(DelicacyError::InvalidResourceLimit { .. })
        ));
        assert_eq!(DelicacyBuildConfig::production(1).validate(), Ok(()));
        // The floor governs a mini; the per-face term governs the treant.
        assert_eq!(
            DelicacyBuildConfig::production(1).max_index_bytes,
            64 * 1024 * 1024
        );
        assert_eq!(
            DelicacyBuildConfig::production(429_120).max_index_bytes,
            429_120 * 2 * 1024
        );
        assert_eq!(
            DelicacyBuildConfig::production(usize::MAX).max_index_bytes,
            usize::MAX
        );
        assert!(matches!(
            DelicacyFaceValue::new(0.0, f64::INFINITY, 0.0),
            Err(DelicacyError::InvalidFaceValue { .. })
        ));
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn move_characterization_production_settings_pin_m0_registry_values() {
        let registry = m0_constants()
            .expect("m0 constants registry")
            .placeholders();
        let settings = DelicacySettings::production().expect("production delicacy settings");

        // The production path is now the sole reader of these three registry
        // entries anywhere in the workspace: `plinth-metrics`'s scar.rs used
        // to read them too, and that crate was deleted on 2026-08-26.  Pin
        // both the plumbing (field-for-field) and the registry values
        // themselves so either drifting fails here.
        assert_eq!(
            settings.curvature_weight(),
            registry.delicacy_curvature_weight().diagnostic().get()
        );
        assert_eq!(
            settings.thinness_weight(),
            registry.delicacy_thinness_weight().diagnostic().get()
        );
        assert_eq!(
            settings.thin_feature_mm(),
            registry.thin_feature_mm().diagnostic().get()
        );
        assert_eq!(settings.curvature_weight(), 0.6);
        assert_eq!(settings.thinness_weight(), 0.4);
        assert_eq!(settings.thin_feature_mm(), 1.5);
    }

    #[test]
    fn move_characterization_production_settings_pin_field_digest() {
        let mesh = make_box_default().unwrap();
        let table = FaceTable::bind(&mesh, "source-lod").unwrap();
        let settings = DelicacySettings::production().expect("production delicacy settings");
        let values = vec![DelicacyFaceValue::new(0.25, 0.5, 0.75).unwrap(); table.face_count()];
        let field = DelicacyField::from_face_values(table, settings, values).unwrap();
        assert_eq!(
            field.digest(),
            "17db02b045d06eb71c340ae641bdef819645a087399d013056a3a295fff13c2b"
        );
    }

    #[test]
    fn move_characterization_signed_zero_inputs_digest_as_positive_zero() {
        let mesh = make_box_default().unwrap();

        // `-0.0` clears both validators (`-0.0 < 0.0` is false and
        // `(0.0..=1.0).contains(&-0.0)` is true), so it can reach the digest,
        // which hashes raw `f64::to_bits` and would otherwise diverge.
        let negative = DelicacySettings::new(-0.0, -0.0, 1.5).unwrap();
        let positive = DelicacySettings::new(0.0, 0.0, 1.5).unwrap();
        assert_eq!(negative.curvature_weight().to_bits(), 0.0_f64.to_bits());
        assert_eq!(negative.thinness_weight().to_bits(), 0.0_f64.to_bits());

        let negative_value = DelicacyFaceValue::new(-0.0, -0.0, -0.0).unwrap();
        let positive_value = DelicacyFaceValue::new(0.0, 0.0, 0.0).unwrap();
        assert_eq!(negative_value.curvature().to_bits(), 0.0_f64.to_bits());
        assert_eq!(negative_value.thinness().to_bits(), 0.0_f64.to_bits());
        assert_eq!(negative_value.weight().to_bits(), 0.0_f64.to_bits());

        let build = |settings, value| {
            let table = FaceTable::bind(&mesh, "source-lod").unwrap();
            let values = vec![value; table.face_count()];
            DelicacyField::from_face_values(table, settings, values).unwrap()
        };
        assert_eq!(
            build(negative, negative_value).digest(),
            build(positive, positive_value).digest()
        );
    }

    #[test]
    fn field_is_fingerprint_bound_face_ordered_and_digest_stable() {
        let mesh = make_box_default().unwrap();
        let first = field(&mesh);
        let second = field(&mesh);
        assert_eq!(first.digest(), second.digest());
        assert_eq!(first.face_values().len(), mesh.tris().len());
        assert_eq!(first.validate_mesh(&mesh), Ok(()));
        let other = crate::make_thin_fin().unwrap();
        assert!(matches!(
            first.validate_mesh(&other),
            Err(DelicacyError::MeshFingerprintMismatch { .. })
        ));
        let table = FaceTable::bind(&mesh, "source-lod").unwrap();
        assert!(matches!(
            DelicacyField::from_face_values(table, first.settings(), vec![]),
            Err(DelicacyError::FaceValueCount { .. })
        ));
    }
}
