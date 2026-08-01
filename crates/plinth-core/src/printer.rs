use crate::{LayerPlan, LayerPlanError, Measured, Placeholder};
use serde::{de, ser::SerializeStruct, Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum BuildVolumeError {
    #[error("build-volume {axis} axis must be finite")]
    NonFiniteAxis { axis: &'static str },
    #[error("build-volume {axis} axis must be greater than zero")]
    NonPositiveAxis { axis: &'static str },
}

/// A printer build volume in project X/Y/Z order, measured in millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildVolumeMm {
    x: f64,
    y: f64,
    z: f64,
}

impl BuildVolumeMm {
    pub fn new(x: f64, y: f64, z: f64) -> Result<Self, BuildVolumeError> {
        validate_positive_axis(x, "x")?;
        validate_positive_axis(y, "y")?;
        validate_positive_axis(z, "z")?;
        Ok(Self { x, y, z })
    }

    #[must_use]
    pub const fn x(&self) -> f64 {
        self.x
    }

    #[must_use]
    pub const fn y(&self) -> f64 {
        self.y
    }

    #[must_use]
    pub const fn z(&self) -> f64 {
        self.z
    }
}

fn validate_positive_axis(value: f64, axis: &'static str) -> Result<(), BuildVolumeError> {
    if !value.is_finite() {
        Err(BuildVolumeError::NonFiniteAxis { axis })
    } else if value <= 0.0 {
        Err(BuildVolumeError::NonPositiveAxis { axis })
    } else {
        Ok(())
    }
}

impl Serialize for BuildVolumeMm {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("BuildVolumeMm", 3)?;
        state.serialize_field("x", &self.x)?;
        state.serialize_field("y", &self.y)?;
        state.serialize_field("z", &self.z)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for BuildVolumeMm {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            x: f64,
            y: f64,
            z: f64,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.x, wire.y, wire.z).map_err(de::Error::custom)
    }
}

/// A Y-axis direction used by the two distinct release-front profile fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum YDirection {
    NegativeY,
    PositiveY,
}

impl YDirection {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NegativeY => "negative_y",
            Self::PositiveY => "positive_y",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PrinterProfileError {
    #[error("invalid printer profile slug {value}")]
    InvalidId { value: String },
    #[error("printer profile field {field} must be finite")]
    NonFiniteScalar { field: &'static str },
    #[error("printer profile field {field} must be greater than zero")]
    NonPositiveScalar { field: &'static str },
    #[error("leading release edge and release-front sweep direction must oppose each other")]
    NonOpposingReleaseDirections,
}

/// A validated lowercase ASCII slug naming one printer profile.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct PrinterProfileId(String);

impl PrinterProfileId {
    pub fn new(value: impl Into<String>) -> Result<Self, PrinterProfileError> {
        let value = value.into();
        let bytes = value.as_bytes();
        let valid = !bytes.is_empty()
            && bytes.first() != Some(&b'-')
            && bytes.last() != Some(&b'-')
            && !bytes.windows(2).any(|pair| pair == b"--")
            && bytes
                .iter()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-');
        if valid {
            Ok(Self(value))
        } else {
            Err(PrinterProfileError::InvalidId { value })
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for PrinterProfileId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

/// Passive printer facts with their provenance wrappers retained at the API boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct PrinterProfile {
    id: PrinterProfileId,
    build_volume_mm: Measured<BuildVolumeMm>,
    pixel_pitch_x_um: Measured<f64>,
    pixel_pitch_y_um: Measured<f64>,
    default_layer_height_mm: Measured<f64>,
    lift_default_mm: Placeholder<f64>,
    leading_release_edge: Measured<YDirection>,
    release_front_sweep_direction: Measured<YDirection>,
    release_film_thickness_mm: Measured<f64>,
}

impl PrinterProfile {
    #[allow(
        clippy::too_many_arguments,
        reason = "the exact passive wire fields are intentional"
    )]
    pub fn new(
        id: impl Into<String>,
        build_volume_mm: Measured<BuildVolumeMm>,
        pixel_pitch_x_um: Measured<f64>,
        pixel_pitch_y_um: Measured<f64>,
        default_layer_height_mm: Measured<f64>,
        lift_default_mm: Placeholder<f64>,
        leading_release_edge: Measured<YDirection>,
        release_front_sweep_direction: Measured<YDirection>,
        release_film_thickness_mm: Measured<f64>,
    ) -> Result<Self, PrinterProfileError> {
        let profile = Self {
            id: PrinterProfileId::new(id)?,
            build_volume_mm,
            pixel_pitch_x_um,
            pixel_pitch_y_um,
            default_layer_height_mm,
            lift_default_mm,
            leading_release_edge,
            release_front_sweep_direction,
            release_film_thickness_mm,
        };
        profile.validate()?;
        Ok(profile)
    }

    fn validate(&self) -> Result<(), PrinterProfileError> {
        validate_positive_scalar(*self.pixel_pitch_x_um.value(), "pixel_pitch_x_um")?;
        validate_positive_scalar(*self.pixel_pitch_y_um.value(), "pixel_pitch_y_um")?;
        validate_positive_scalar(
            *self.default_layer_height_mm.value(),
            "default_layer_height_mm",
        )?;
        validate_positive_scalar(*self.lift_default_mm.diagnostic(), "lift_default_mm")?;
        validate_positive_scalar(
            *self.release_film_thickness_mm.value(),
            "release_film_thickness_mm",
        )?;
        if self.leading_release_edge.value() == self.release_front_sweep_direction.value() {
            return Err(PrinterProfileError::NonOpposingReleaseDirections);
        }
        Ok(())
    }

    #[must_use]
    pub const fn id(&self) -> &PrinterProfileId {
        &self.id
    }

    #[must_use]
    pub const fn build_volume_mm(&self) -> &Measured<BuildVolumeMm> {
        &self.build_volume_mm
    }

    #[must_use]
    pub const fn pixel_pitch_x_um(&self) -> &Measured<f64> {
        &self.pixel_pitch_x_um
    }

    #[must_use]
    pub const fn pixel_pitch_y_um(&self) -> &Measured<f64> {
        &self.pixel_pitch_y_um
    }

    #[must_use]
    pub const fn default_layer_height_mm(&self) -> &Measured<f64> {
        &self.default_layer_height_mm
    }

    #[must_use]
    pub const fn lift_default_mm(&self) -> &Placeholder<f64> {
        &self.lift_default_mm
    }

    #[must_use]
    pub const fn leading_release_edge(&self) -> &Measured<YDirection> {
        &self.leading_release_edge
    }

    #[must_use]
    pub const fn release_front_sweep_direction(&self) -> &Measured<YDirection> {
        &self.release_front_sweep_direction
    }

    #[must_use]
    pub const fn release_film_thickness_mm(&self) -> &Measured<f64> {
        &self.release_film_thickness_mm
    }

    pub fn layer_plan(&self, count: usize) -> Result<LayerPlan, LayerPlanError> {
        LayerPlan::constant(&self.default_layer_height_mm, count)
    }
}

fn validate_positive_scalar(value: f64, field: &'static str) -> Result<(), PrinterProfileError> {
    if !value.is_finite() {
        Err(PrinterProfileError::NonFiniteScalar { field })
    } else if value <= 0.0 {
        Err(PrinterProfileError::NonPositiveScalar { field })
    } else {
        Ok(())
    }
}

impl Serialize for PrinterProfile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("PrinterProfile", 9)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("build_volume_mm", &self.build_volume_mm)?;
        state.serialize_field("pixel_pitch_x_um", &self.pixel_pitch_x_um)?;
        state.serialize_field("pixel_pitch_y_um", &self.pixel_pitch_y_um)?;
        state.serialize_field("default_layer_height_mm", &self.default_layer_height_mm)?;
        state.serialize_field("lift_default_mm", &self.lift_default_mm)?;
        state.serialize_field("leading_release_edge", &self.leading_release_edge)?;
        state.serialize_field(
            "release_front_sweep_direction",
            &self.release_front_sweep_direction,
        )?;
        state.serialize_field("release_film_thickness_mm", &self.release_film_thickness_mm)?;
        state.end()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LiftDefaultWire {
    value: f64,
    note: String,
}

impl<'de> Deserialize<'de> for PrinterProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            id: String,
            build_volume_mm: Measured<BuildVolumeMm>,
            pixel_pitch_x_um: Measured<f64>,
            pixel_pitch_y_um: Measured<f64>,
            default_layer_height_mm: Measured<f64>,
            lift_default_mm: LiftDefaultWire,
            leading_release_edge: Measured<YDirection>,
            release_front_sweep_direction: Measured<YDirection>,
            release_film_thickness_mm: Measured<f64>,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.id,
            wire.build_volume_mm,
            wire.pixel_pitch_x_um,
            wire.pixel_pitch_y_um,
            wire.default_layer_height_mm,
            Placeholder::new(wire.lift_default_mm.value, wire.lift_default_mm.note),
            wire.leading_release_edge,
            wire.release_front_sweep_direction,
            wire.release_film_thickness_mm,
        )
        .map_err(de::Error::custom)
    }
}
