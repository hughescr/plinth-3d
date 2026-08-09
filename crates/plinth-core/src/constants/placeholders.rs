use super::{sealed, ConstantDescriptor, ConstantValue, RegistryError, ToConstantValue};
use crate::{ConstantId, Finite, FiniteInterval, Placeholder, ProvenanceError};
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{fmt, marker::PhantomData, num::NonZeroU64};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum PlaceholderValueError {
    #[error("value must be finite")]
    NonFinite,
    #[error("value must be greater than zero")]
    NonPositive,
    #[error("value must be non-negative")]
    Negative,
    #[error("fraction must be between zero and one inclusive")]
    OutsideUnitInterval,
    #[error("factor must be finite and greater than one")]
    NotGreaterThanOne,
    #[error("count must be greater than zero")]
    ZeroCount,
    #[error("{field} lower bound must be less than its upper bound")]
    InvalidOrder { field: &'static str },
    #[error("{field} step must fit strictly inside its range")]
    InvalidStep { field: &'static str },
    #[error("sphere triangle count does not match its latitude/longitude policy")]
    SphereTriangleCount,
    #[error("drain directions must contain exactly five downward nonzero vectors")]
    InvalidDrainDirections,
    #[error("vent directions must contain exactly four horizontal axis vectors")]
    InvalidVentDirections,
    #[error("extension ladder must be nonempty and strictly increasing")]
    InvalidExtensionLadder,
}

#[derive(Clone, Debug, PartialEq, Error)]
pub enum M0PlaceholderConstantsError {
    #[error(transparent)]
    Value(#[from] PlaceholderValueError),
    #[error(transparent)]
    Interval(#[from] ProvenanceError),
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct PositiveFinite(Finite);

impl PositiveFinite {
    pub fn new(value: f64) -> Result<Self, PlaceholderValueError> {
        if !value.is_finite() {
            Err(PlaceholderValueError::NonFinite)
        } else if value <= 0.0 {
            Err(PlaceholderValueError::NonPositive)
        } else {
            Ok(Self(
                Finite::new(value).map_err(|_| PlaceholderValueError::NonFinite)?,
            ))
        }
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct NonNegativeFinite(Finite);

impl NonNegativeFinite {
    pub fn new(value: f64) -> Result<Self, PlaceholderValueError> {
        if !value.is_finite() {
            Err(PlaceholderValueError::NonFinite)
        } else if value < 0.0 {
            Err(PlaceholderValueError::Negative)
        } else {
            Ok(Self(
                Finite::new(value).map_err(|_| PlaceholderValueError::NonFinite)?,
            ))
        }
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct UnitFraction(Finite);

impl UnitFraction {
    pub fn new(value: f64) -> Result<Self, PlaceholderValueError> {
        if !value.is_finite() {
            Err(PlaceholderValueError::NonFinite)
        } else if !(0.0..=1.0).contains(&value) {
            Err(PlaceholderValueError::OutsideUnitInterval)
        } else {
            Ok(Self(
                Finite::new(value).map_err(|_| PlaceholderValueError::NonFinite)?,
            ))
        }
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0.get()
    }
}

macro_rules! finite_wire {
    ($type:ty) => {
        impl Serialize for $type {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_f64(self.get())
            }
        }

        impl<'de> Deserialize<'de> for $type {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                Self::new(f64::deserialize(deserializer)?).map_err(de::Error::custom)
            }
        }

        impl sealed::ConstantValueSource for $type {}
        impl ToConstantValue for $type {
            fn to_constant_value(&self) -> Result<ConstantValue, RegistryError> {
                ConstantValue::finite_scalar(self.get())
            }
        }
    };
}

finite_wire!(PositiveFinite);
finite_wire!(NonNegativeFinite);
finite_wire!(UnitFraction);

impl sealed::ConstantValueSource for NonZeroU64 {}
impl ToConstantValue for NonZeroU64 {
    fn to_constant_value(&self) -> Result<ConstantValue, RegistryError> {
        Ok(ConstantValue::unsigned_integer(self.get()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeSagLimit {
    span_mm: PositiveFinite,
    section_mm: PositiveFinite,
}

impl BridgeSagLimit {
    pub fn new(span_mm: f64, section_mm: f64) -> Result<Self, PlaceholderValueError> {
        Ok(Self {
            span_mm: PositiveFinite::new(span_mm)?,
            section_mm: PositiveFinite::new(section_mm)?,
        })
    }

    #[must_use]
    pub const fn span_mm(self) -> f64 {
        self.span_mm.get()
    }

    #[must_use]
    pub const fn section_mm(self) -> f64 {
        self.section_mm.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct CcoSplitSlidePolicy {
    start: UnitFraction,
    end: UnitFraction,
    step: UnitFraction,
    cap: NonZeroU64,
}

impl CcoSplitSlidePolicy {
    pub fn new(start: f64, end: f64, step: f64, cap: u64) -> Result<Self, PlaceholderValueError> {
        let start = UnitFraction::new(start)?;
        let end = UnitFraction::new(end)?;
        let step = UnitFraction::new(step)?;
        if start.get() >= end.get() {
            return Err(PlaceholderValueError::InvalidOrder {
                field: "CCO split-slide",
            });
        }
        if step.get() <= 0.0 || step.get() >= end.get() - start.get() {
            return Err(PlaceholderValueError::InvalidStep {
                field: "CCO split-slide",
            });
        }
        Ok(Self {
            start,
            end,
            step,
            cap: nonzero(cap)?,
        })
    }

    #[must_use]
    pub const fn start(self) -> f64 {
        self.start.get()
    }

    #[must_use]
    pub const fn end(self) -> f64 {
        self.end.get()
    }

    #[must_use]
    pub const fn step(self) -> f64 {
        self.step.get()
    }

    #[must_use]
    pub const fn cap(self) -> NonZeroU64 {
        self.cap
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CcoSplitSlideWire {
    start: f64,
    end: f64,
    step: f64,
    cap: u64,
}

impl<'de> Deserialize<'de> for CcoSplitSlidePolicy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CcoSplitSlideWire::deserialize(deserializer)?;
        Self::new(wire.start, wire.end, wire.step, wire.cap).map_err(de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementXyMm {
    x: Finite,
    y: Finite,
}

impl PlacementXyMm {
    pub fn new(x: f64, y: f64) -> Result<Self, PlaceholderValueError> {
        Ok(Self {
            x: Finite::new(x).map_err(|_| PlaceholderValueError::NonFinite)?,
            y: Finite::new(y).map_err(|_| PlaceholderValueError::NonFinite)?,
        })
    }

    #[must_use]
    pub const fn x(self) -> f64 {
        self.x.get()
    }

    #[must_use]
    pub const fn y(self) -> f64 {
        self.y.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SphereTessellationPolicy {
    n_lat: NonZeroU64,
    n_lon: NonZeroU64,
    triangles: NonZeroU64,
}

impl SphereTessellationPolicy {
    pub fn new(n_lat: u64, n_lon: u64, triangles: u64) -> Result<Self, PlaceholderValueError> {
        let n_lat = nonzero(n_lat)?;
        let n_lon = nonzero(n_lon)?;
        let triangles = nonzero(triangles)?;
        let expected = 2_u64
            .checked_mul(n_lon.get())
            .and_then(|value| value.checked_mul(n_lat.get().saturating_sub(1)));
        if expected != Some(triangles.get()) {
            return Err(PlaceholderValueError::SphereTriangleCount);
        }
        Ok(Self {
            n_lat,
            n_lon,
            triangles,
        })
    }

    #[must_use]
    pub const fn n_lat(self) -> NonZeroU64 {
        self.n_lat
    }

    #[must_use]
    pub const fn n_lon(self) -> NonZeroU64 {
        self.n_lon
    }

    #[must_use]
    pub const fn triangles(self) -> NonZeroU64 {
        self.triangles
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SphereTessellationWire {
    n_lat: u64,
    n_lon: u64,
    triangles: u64,
}

impl<'de> Deserialize<'de> for SphereTessellationPolicy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = SphereTessellationWire::deserialize(deserializer)?;
        Self::new(wire.n_lat, wire.n_lon, wire.triangles).map_err(de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecimationAdaptationPolicy {
    adaptations: NonZeroU64,
    feature_multiplier: PositiveFinite,
    error_multiplier: UnitFraction,
}

impl DecimationAdaptationPolicy {
    pub fn new(
        adaptations: u64,
        feature_multiplier: f64,
        error_multiplier: f64,
    ) -> Result<Self, PlaceholderValueError> {
        Ok(Self {
            adaptations: nonzero(adaptations)?,
            feature_multiplier: PositiveFinite::new(feature_multiplier)?,
            error_multiplier: UnitFraction::new(error_multiplier)?,
        })
    }

    #[must_use]
    pub const fn adaptations(self) -> NonZeroU64 {
        self.adaptations
    }

    #[must_use]
    pub const fn feature_multiplier(self) -> f64 {
        self.feature_multiplier.get()
    }

    #[must_use]
    pub const fn error_multiplier(self) -> f64 {
        self.error_multiplier.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecimationSamplingPolicy {
    seed: u64,
    cap: NonZeroU64,
}

impl DecimationSamplingPolicy {
    pub fn new(seed: u64, cap: u64) -> Result<Self, PlaceholderValueError> {
        Ok(Self {
            seed,
            cap: nonzero(cap)?,
        })
    }

    #[must_use]
    pub const fn seed(self) -> u64 {
        self.seed
    }

    #[must_use]
    pub const fn cap(self) -> NonZeroU64 {
        self.cap
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct DecimationRatioSearchPolicy {
    lower: UnitFraction,
    upper: UnitFraction,
    bisections: NonZeroU64,
}

impl DecimationRatioSearchPolicy {
    pub fn new(lower: f64, upper: f64, bisections: u64) -> Result<Self, PlaceholderValueError> {
        let lower = UnitFraction::new(lower)?;
        let upper = UnitFraction::new(upper)?;
        if lower.get() >= upper.get() {
            return Err(PlaceholderValueError::InvalidOrder {
                field: "decimation ratio search",
            });
        }
        Ok(Self {
            lower,
            upper,
            bisections: nonzero(bisections)?,
        })
    }

    #[must_use]
    pub const fn lower(self) -> f64 {
        self.lower.get()
    }

    #[must_use]
    pub const fn upper(self) -> f64 {
        self.upper.get()
    }

    #[must_use]
    pub const fn bisections(self) -> NonZeroU64 {
        self.bisections
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RatioSearchWire {
    lower: f64,
    upper: f64,
    bisections: u64,
}

impl<'de> Deserialize<'de> for DecimationRatioSearchPolicy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RatioSearchWire::deserialize(deserializer)?;
        Self::new(wire.lower, wire.upper, wire.bisections).map_err(de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrainDirection {
    x: Finite,
    y: Finite,
    z: Finite,
}

impl DrainDirection {
    pub fn new(x: f64, y: f64, z: f64) -> Result<Self, PlaceholderValueError> {
        let direction = Self {
            x: Finite::new(x).map_err(|_| PlaceholderValueError::NonFinite)?,
            y: Finite::new(y).map_err(|_| PlaceholderValueError::NonFinite)?,
            z: Finite::new(z).map_err(|_| PlaceholderValueError::NonFinite)?,
        };
        if direction.x.get() == 0.0 && direction.y.get() == 0.0 && direction.z.get() == 0.0 {
            Err(PlaceholderValueError::InvalidDrainDirections)
        } else {
            Ok(direction)
        }
    }

    #[must_use]
    pub const fn x(self) -> f64 {
        self.x.get()
    }

    #[must_use]
    pub const fn y(self) -> f64 {
        self.y.get()
    }

    #[must_use]
    pub const fn z(self) -> f64 {
        self.z.get()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct DrainDirections([DrainDirection; 5]);

impl DrainDirections {
    pub fn new(directions: [DrainDirection; 5]) -> Result<Self, PlaceholderValueError> {
        if directions.iter().all(|direction| direction.z() < 0.0) {
            Ok(Self(directions))
        } else {
            Err(PlaceholderValueError::InvalidDrainDirections)
        }
    }

    #[must_use]
    pub const fn directions(&self) -> &[DrainDirection; 5] {
        &self.0
    }
}

impl<'de> Deserialize<'de> for DrainDirections {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(<[DrainDirection; 5]>::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct VentDirections([DrainDirection; 4]);

impl VentDirections {
    pub fn new(directions: [DrainDirection; 4]) -> Result<Self, PlaceholderValueError> {
        let expected = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)];
        if directions.iter().zip(expected).all(|(direction, (x, y))| {
            direction.x() == x && direction.y() == y && direction.z() == 0.0
        }) {
            Ok(Self(directions))
        } else {
            Err(PlaceholderValueError::InvalidVentDirections)
        }
    }

    #[must_use]
    pub const fn directions(&self) -> &[DrainDirection; 4] {
        &self.0
    }
}

impl<'de> Deserialize<'de> for VentDirections {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(<[DrainDirection; 4]>::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HollowExtraDrainPolicy {
    min_pocket_ml: PositiveFinite,
    max_extra_drains: NonZeroU64,
    exit_max_mm: PositiveFinite,
    extensions_mm: Vec<PositiveFinite>,
    cylinder_sides: NonZeroU64,
}

impl HollowExtraDrainPolicy {
    pub fn new(
        min_pocket_ml: f64,
        max_extra_drains: u64,
        exit_max_mm: f64,
        extensions_mm: &[f64],
        cylinder_sides: u64,
    ) -> Result<Self, PlaceholderValueError> {
        let extensions_mm = positive_values(extensions_mm)?;
        if extensions_mm.is_empty()
            || extensions_mm
                .windows(2)
                .any(|pair| pair[0].get() >= pair[1].get())
        {
            return Err(PlaceholderValueError::InvalidExtensionLadder);
        }
        Ok(Self {
            min_pocket_ml: PositiveFinite::new(min_pocket_ml)?,
            max_extra_drains: nonzero(max_extra_drains)?,
            exit_max_mm: PositiveFinite::new(exit_max_mm)?,
            extensions_mm,
            cylinder_sides: nonzero(cylinder_sides)?,
        })
    }

    #[must_use]
    pub const fn min_pocket_ml(&self) -> f64 {
        self.min_pocket_ml.get()
    }

    #[must_use]
    pub const fn max_extra_drains(&self) -> NonZeroU64 {
        self.max_extra_drains
    }

    #[must_use]
    pub const fn exit_max_mm(&self) -> f64 {
        self.exit_max_mm.get()
    }

    #[must_use]
    pub fn extensions_mm(&self) -> impl ExactSizeIterator<Item = f64> + '_ {
        self.extensions_mm.iter().map(|value| value.get())
    }

    #[must_use]
    pub const fn cylinder_sides(&self) -> NonZeroU64 {
        self.cylinder_sides
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtraDrainWire {
    min_pocket_ml: f64,
    max_extra_drains: u64,
    exit_max_mm: f64,
    extensions_mm: Vec<f64>,
    cylinder_sides: u64,
}

impl<'de> Deserialize<'de> for HollowExtraDrainPolicy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ExtraDrainWire::deserialize(deserializer)?;
        Self::new(
            wire.min_pocket_ml,
            wire.max_extra_drains,
            wire.exit_max_mm,
            &wire.extensions_mm,
            wire.cylinder_sides,
        )
        .map_err(de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BeamWidth(NonZeroU64);

impl BeamWidth {
    pub fn new(value: u64) -> Result<Self, PlaceholderValueError> {
        Ok(Self(nonzero(value)?))
    }

    #[must_use]
    pub const fn get(self) -> NonZeroU64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AnchorCandidateCap(NonZeroU64);

impl AnchorCandidateCap {
    pub fn new(value: u64) -> Result<Self, PlaceholderValueError> {
        Ok(Self(nonzero(value)?))
    }

    #[must_use]
    pub const fn get(self) -> NonZeroU64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SeedSpacingFactor(PositiveFinite);

impl SeedSpacingFactor {
    pub fn new(value: f64) -> Result<Self, PlaceholderValueError> {
        if !value.is_finite() || value <= 1.0 {
            Err(PlaceholderValueError::NotGreaterThanOne)
        } else {
            Ok(Self(PositiveFinite::new(value)?))
        }
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0.get()
    }
}

impl<'de> Deserialize<'de> for SeedSpacingFactor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(f64::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FixBraceIterationCap(NonZeroU64);

impl FixBraceIterationCap {
    pub fn new(value: u64) -> Result<Self, PlaceholderValueError> {
        Ok(Self(nonzero(value)?))
    }

    #[must_use]
    pub const fn get(self) -> NonZeroU64 {
        self.0
    }
}

macro_rules! enabled_scalar_conversion {
    ($type:ty, $body:expr) => {
        impl sealed::ConstantValueSource for $type {}
        impl ToConstantValue for $type {
            fn to_constant_value(&self) -> Result<ConstantValue, RegistryError> {
                $body(self)
            }
        }
    };
}

enabled_scalar_conversion!(BeamWidth, |value: &BeamWidth| Ok(
    ConstantValue::unsigned_integer(value.get().get())
));
enabled_scalar_conversion!(AnchorCandidateCap, |value: &AnchorCandidateCap| Ok(
    ConstantValue::unsigned_integer(value.get().get())
));
enabled_scalar_conversion!(SeedSpacingFactor, |value: &SeedSpacingFactor| {
    ConstantValue::finite_scalar(value.get())
});
enabled_scalar_conversion!(FixBraceIterationCap, |value: &FixBraceIterationCap| Ok(
    ConstantValue::unsigned_integer(value.get().get())
));

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TrialSetting<T> {
    Disabled,
    Enabled { value: T },
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for TrialSetting<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            State,
            Value,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum State {
            Disabled,
            Enabled,
        }
        struct TrialVisitor<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for TrialVisitor<T> {
            type Value = TrialSetting<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a strict disabled or enabled trial setting")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut state = None;
                let mut value = None;
                while let Some(field) = map.next_key::<Field>()? {
                    match field {
                        Field::State => {
                            if state.is_some() {
                                return Err(de::Error::duplicate_field("state"));
                            }
                            state = Some(map.next_value()?);
                        }
                        Field::Value => {
                            if value.is_some() {
                                return Err(de::Error::duplicate_field("value"));
                            }
                            value = Some(map.next_value()?);
                        }
                    }
                }
                match state.ok_or_else(|| de::Error::missing_field("state"))? {
                    State::Disabled if value.is_none() => Ok(TrialSetting::Disabled),
                    State::Disabled => Err(de::Error::custom("disabled trial forbids value")),
                    State::Enabled => Ok(TrialSetting::Enabled {
                        value: value.ok_or_else(|| de::Error::missing_field("value"))?,
                    }),
                }
            }
        }
        deserializer.deserialize_struct(
            "TrialSetting",
            &["state", "value"],
            TrialVisitor(PhantomData),
        )
    }
}

impl<T> TrialSetting<T> {
    #[must_use]
    pub const fn disabled() -> Self {
        Self::Disabled
    }

    #[must_use]
    pub const fn enabled(value: T) -> Self {
        Self::Enabled { value }
    }

    #[must_use]
    pub const fn is_disabled(&self) -> bool {
        matches!(self, Self::Disabled)
    }

    #[must_use]
    pub const fn enabled_value(&self) -> Option<&T> {
        match self {
            Self::Disabled => None,
            Self::Enabled { value } => Some(value),
        }
    }
}

impl<T: ToConstantValue> sealed::ConstantValueSource for TrialSetting<T> {}
impl<T: ToConstantValue> ToConstantValue for TrialSetting<T> {
    fn to_constant_value(&self) -> Result<ConstantValue, RegistryError> {
        match self {
            Self::Disabled => {
                ConstantValue::record([("state", ConstantValue::core_text("disabled")?)])
            }
            Self::Enabled { value } => ConstantValue::record([
                ("state", ConstantValue::core_text("enabled")?),
                ("value", value.to_constant_value()?),
            ]),
        }
    }
}

macro_rules! record_conversion {
    ($type:ty, [$($field:literal => $value:expr),+ $(,)?]) => {
        impl sealed::ConstantValueSource for $type {}
        impl ToConstantValue for $type {
            fn to_constant_value(&self) -> Result<ConstantValue, RegistryError> {
                ConstantValue::record([
                    $(($field, ($value)(self).to_constant_value()?),)+
                ])
            }
        }
    };
}

record_conversion!(BridgeSagLimit, [
    "section_mm" => |value: &BridgeSagLimit| value.section_mm,
    "span_mm" => |value: &BridgeSagLimit| value.span_mm,
]);
record_conversion!(CcoSplitSlidePolicy, [
    "cap" => |value: &CcoSplitSlidePolicy| value.cap,
    "end" => |value: &CcoSplitSlidePolicy| value.end,
    "start" => |value: &CcoSplitSlidePolicy| value.start,
    "step" => |value: &CcoSplitSlidePolicy| value.step,
]);
record_conversion!(PlacementXyMm, [
    "x" => |value: &PlacementXyMm| value.x,
    "y" => |value: &PlacementXyMm| value.y,
]);
record_conversion!(SphereTessellationPolicy, [
    "n_lat" => |value: &SphereTessellationPolicy| value.n_lat,
    "n_lon" => |value: &SphereTessellationPolicy| value.n_lon,
    "triangles" => |value: &SphereTessellationPolicy| value.triangles,
]);
record_conversion!(DecimationAdaptationPolicy, [
    "adaptations" => |value: &DecimationAdaptationPolicy| value.adaptations,
    "error_multiplier" => |value: &DecimationAdaptationPolicy| value.error_multiplier,
    "feature_multiplier" => |value: &DecimationAdaptationPolicy| value.feature_multiplier,
]);
record_conversion!(DecimationSamplingPolicy, [
    "cap" => |value: &DecimationSamplingPolicy| value.cap,
    "seed" => |value: &DecimationSamplingPolicy| value.seed,
]);
record_conversion!(DecimationRatioSearchPolicy, [
    "bisections" => |value: &DecimationRatioSearchPolicy| value.bisections,
    "lower" => |value: &DecimationRatioSearchPolicy| value.lower,
    "upper" => |value: &DecimationRatioSearchPolicy| value.upper,
]);
record_conversion!(DrainDirection, [
    "x" => |value: &DrainDirection| value.x,
    "y" => |value: &DrainDirection| value.y,
    "z" => |value: &DrainDirection| value.z,
]);

impl sealed::ConstantValueSource for DrainDirections {}
impl ToConstantValue for DrainDirections {
    fn to_constant_value(&self) -> Result<ConstantValue, RegistryError> {
        ConstantValue::list(self.0.iter())
    }
}

impl sealed::ConstantValueSource for VentDirections {}
impl ToConstantValue for VentDirections {
    fn to_constant_value(&self) -> Result<ConstantValue, RegistryError> {
        ConstantValue::list(self.0.iter())
    }
}

impl sealed::ConstantValueSource for HollowExtraDrainPolicy {}
impl ToConstantValue for HollowExtraDrainPolicy {
    fn to_constant_value(&self) -> Result<ConstantValue, RegistryError> {
        ConstantValue::record([
            ("cylinder_sides", self.cylinder_sides.to_constant_value()?),
            ("exit_max_mm", self.exit_max_mm.to_constant_value()?),
            ("extensions_mm", self.extensions_mm.to_constant_value()?),
            (
                "max_extra_drains",
                self.max_extra_drains.to_constant_value()?,
            ),
            ("min_pocket_ml", self.min_pocket_ml.to_constant_value()?),
        ])
    }
}

fn nonzero(value: u64) -> Result<NonZeroU64, PlaceholderValueError> {
    NonZeroU64::new(value).ok_or(PlaceholderValueError::ZeroCount)
}

fn positive_values(values: &[f64]) -> Result<Vec<PositiveFinite>, PlaceholderValueError> {
    values.iter().copied().map(PositiveFinite::new).collect()
}

fn nonnegative_values(values: &[f64]) -> Result<Vec<NonNegativeFinite>, PlaceholderValueError> {
    values.iter().copied().map(NonNegativeFinite::new).collect()
}

fn fraction_values(values: &[f64]) -> Result<Vec<UnitFraction>, PlaceholderValueError> {
    values.iter().copied().map(UnitFraction::new).collect()
}

// The declaration table follows. It is intentionally handwritten from the
// closed M0 authority; runtime code never parses the source ledger.

macro_rules! placeholder_declarations {
    ($(
        $field:ident : $type:ty = $value:expr;
        $id:literal, $unit:literal, $shape:literal, $note:literal;
    )+) => {
        #[derive(Clone, Debug, PartialEq)]
        pub struct M0PlaceholderConstants {
            $($field: Placeholder<$type>,)+
        }

        impl M0PlaceholderConstants {
            pub fn new() -> Result<Self, M0PlaceholderConstantsError> {
                Ok(Self {
                    $($field: Placeholder::new(
                        $value,
                        concat!("Placeholder; non-AcceptSafe. ", $note),
                    ),)+
                })
            }

            $(
                #[must_use]
                pub const fn $field(&self) -> &Placeholder<$type> {
                    &self.$field
                }
            )+

            pub fn descriptors(&self) -> Result<[ConstantDescriptor; 255], RegistryError> {
                Ok([
                    $(ConstantDescriptor::from_placeholder(
                        ConstantId::from($id),
                        $unit,
                        $shape,
                        &self.$field,
                    )?,)+
                ])
            }
        }
    };
}

placeholder_declarations! {
    a_abs_mm2: PositiveFinite = PositiveFinite::new(2.0)?;
    "A_ABS_MM2", "mm²", "scalar", "Owner-ratified scar-area gate with no literature measurement.";
    ball_breach_eps_mm: PositiveFinite = PositiveFinite::new(0.05)?;
    "BALL_BREACH_EPS_MM", "mm", "scalar", "Legacy ball-breach gate tolerance.";
    ball_factor_foot: PositiveFinite = PositiveFinite::new(1.4)?;
    "BALL_FACTOR_FOOT", "1", "scalar", "Model-foot joint-ball geometry factor.";
    ball_factor_stub: PositiveFinite = PositiveFinite::new(1.5)?;
    "BALL_FACTOR_STUB", "1", "scalar", "Tip stub ball-radius factor.";
    brace_ladder_z_mm: PositiveFinite = PositiveFinite::new(8.0)?;
    "BRACE_LADDER_Z_MM", "mm", "scalar", "Brace ladder vertical spacing tuning.";
    brace_neighbor_max_xy_mm: PositiveFinite = PositiveFinite::new(15.0)?;
    "BRACE_NEIGHBOR_MAX_XY_MM", "mm", "scalar", "Brace-neighbor search radius.";
    brace_reach_max_factor_x: PositiveFinite = PositiveFinite::new(1.5)?;
    "BRACE_REACH_MAX_FACTOR_X", "1", "scalar", "Hard widened cross-link reach factor.";
    brace_r_mm: PositiveFinite = PositiveFinite::new(0.5)?;
    "BRACE_R_MM", "mm", "scalar", "Brace member radius tuning.";
    bracing_fallback_fractions: Vec<UnitFraction> = fraction_values(&[0.5, 0.25, 0.75, 0.1, 0.9])?;
    "BRACING_FALLBACK_FRACTIONS", "1", "list[5]", "Ordered deterministic fallback-fraction policy.";
    bracing_max_stacked_fallbacks: NonZeroU64 = nonzero(6)?;
    "BRACING_MAX_STACKED_FALLBACKS", "count", "scalar", "Result-changing stacked-fallback search cap.";
    bracing_min_cross_link_xy_mm: PositiveFinite = PositiveFinite::new(std::f64::consts::FRAC_1_SQRT_2)?;
    "BRACING_MIN_CROSS_LINK_XY_MM", "mm", "scalar", "Minimum cross-link XY distance, exactly one over square-root two.";
    bracing_model_rung_max_candidates: NonZeroU64 = nonzero(6)?;
    "BRACING_MODEL_RUNG_MAX_CANDIDATES", "count", "scalar", "Model-rung shortlist cap.";
    bracing_model_rung_min_length_mm: PositiveFinite = PositiveFinite::new(1.0)?;
    "BRACING_MODEL_RUNG_MIN_LENGTH_MM", "mm", "scalar", "Model-rung length floor.";
    bracing_model_rung_z_tol_mm: PositiveFinite = PositiveFinite::new(0.75)?;
    "BRACING_MODEL_RUNG_Z_TOL_MM", "mm", "scalar", "Model-rung candidate Z band.";
    bracing_reach_tier_base_factor: PositiveFinite = PositiveFinite::new(1.25)?;
    "BRACING_REACH_TIER_BASE_FACTOR", "1", "scalar", "Base factor for deterministic brace reach tiers.";
    branch_angle_deg: PositiveFinite = PositiveFinite::new(60.0)?;
    "BRANCH_ANGLE_DEG", "°", "scalar", "Widened branch cone is valid only with the angle-aware load term.";
    bridge_free_gap_mm: PositiveFinite = PositiveFinite::new(1.0)?;
    "BRIDGE_FREE_GAP_MM", "mm", "scalar", "Local unsupported-gap perception default, distinct from sag span.";
    bridge_sag_span_approx_mm: BridgeSagLimit = BridgeSagLimit::new(3.0, 0.300)?;
    "BRIDGE_SAG_SPAN_APPROX_MM", "mm", "record{section_mm,span_mm}", "Approximate sag span inseparably conditioned on a 0.300 mm section; distinct from free gap.";
    cco_bifurcation_hosts: NonZeroU64 = nonzero(5)?;
    "CCO_BIFURCATION_HOSTS", "count", "scalar", "Bifurcation host shortlist.";
    cco_cone_tolerance_deg: PositiveFinite = PositiveFinite::new(1e-6)?;
    "CCO_CONE_TOLERANCE_DEG", "°", "scalar", "Cone comparison tolerance, not a physics floor.";
    cco_fermat_max_iterations: NonZeroU64 = nonzero(12)?;
    "CCO_FERMAT_MAX_ITERATIONS", "count", "scalar", "Bounded Weiszfeld iteration cap.";
    cco_min_split_length_mm: PositiveFinite = PositiveFinite::new(0.5)?;
    "CCO_MIN_SPLIT_LENGTH_MM", "mm", "scalar", "Bifurcation split-piece length floor.";
    cco_split_slide_policy: CcoSplitSlidePolicy = CcoSplitSlidePolicy::new(0.03, 0.97, 0.03, 40)?;
    "CCO_SPLIT_SLIDE_POLICY", "1", "record{cap,end,start,step}", "Structured deterministic split-slide grid.";
    cco_xia_alpha: UnitFraction = UnitFraction::new(0.5)?;
    "CCO_XIA_ALPHA", "1", "scalar", "Legacy topology-search heuristic whose conditional equations do not derive alpha.";
    chitubox_default_placement_xy_mm: PlacementXyMm = PlacementXyMm::new(0.0, 0.0)?;
    "CHITUBOX_DEFAULT_PLACEMENT_XY_MM", "mm", "record{x,y}", "Project placement default with signed-zero-preserving coordinates.";
    chitubox_emit_n_sides: NonZeroU64 = nonzero(24)?;
    "CHITUBOX_EMIT_N_SIDES", "count", "scalar", "ChituBox emission geometry contract.";
    chitubox_pad_diameter_mm: PositiveFinite = PositiveFinite::new(2.2)?;
    "CHITUBOX_PAD_DIAMETER_MM", "mm", "scalar", "Generated support-pad diameter.";
    chitubox_pad_n_sides: NonZeroU64 = nonzero(20)?;
    "CHITUBOX_PAD_N_SIDES", "count", "scalar", "Generated support-pad tessellation count.";
    chitubox_plate_mesh_tol_mm: PositiveFinite = PositiveFinite::new(1e-5)?;
    "CHITUBOX_PLATE_MESH_TOL_MM", "mm", "scalar", "Plate mesh geometry tolerance, separate from binary offsets.";
    chitubox_raft_height_mm: PositiveFinite = PositiveFinite::new(0.2)?;
    "CHITUBOX_RAFT_HEIGHT_MM", "mm", "scalar", "Generated raft height.";
    chitubox_root_height_mm: PositiveFinite = PositiveFinite::new(0.5)?;
    "CHITUBOX_ROOT_HEIGHT_MM", "mm", "scalar", "Root no-bend height.";
    chitubox_root_layers: NonZeroU64 = nonzero(10)?;
    "CHITUBOX_ROOT_LAYERS", "count", "scalar", "Minimum vertical root layers.";
    chitubox_sphere_mesh_tessellation_policy: SphereTessellationPolicy = SphereTessellationPolicy::new(8, 16, 224)?;
    "CHITUBOX_SPHERE_MESH_TESSELLATION_POLICY", "count", "record{n_lat,n_lon,triangles}", "Default UV-sphere geometry policy for emitted tip, contact, and joint spheres.";
    clearance_sample_step_mm: PositiveFinite = PositiveFinite::new(0.3)?;
    "CLEARANCE_SAMPLE_STEP_MM", "mm", "scalar", "Legacy sampled-path diagnostic only; production clearance is exact.";
    coincide_tol_mm: PositiveFinite = PositiveFinite::new(0.15)?;
    "COINCIDE_TOL_MM", "mm", "scalar", "Joint coincidence tolerance.";
    colinear_angle_deg: PositiveFinite = PositiveFinite::new(10.0)?;
    "COLINEAR_ANGLE_DEG", "°", "scalar", "Joint colinearity angle tolerance.";
    colinear_radius_frac: UnitFraction = UnitFraction::new(0.20)?;
    "COLINEAR_RADIUS_FRAC", "1", "scalar", "Joint colinearity radius fraction.";
    connectivity_axis_attach_tol_mm: PositiveFinite = PositiveFinite::new(0.25)?;
    "CONNECTIVITY_AXIS_ATTACH_TOL_MM", "mm", "scalar", "Connectivity axis-attachment gate tolerance.";
    connectivity_embed_pad_mm: PositiveFinite = PositiveFinite::new(0.45)?;
    "CONNECTIVITY_EMBED_PAD_MM", "mm", "scalar", "Connectivity embed pad added to contact depth.";
    coverage_radius_factor: PositiveFinite = PositiveFinite::new(1.5)?;
    "COVERAGE_RADIUS_FACTOR", "1", "scalar", "Coverage gate radius factor selected without measurement.";
    coverage_repair_max_attempts: NonZeroU64 = nonzero(8)?;
    "COVERAGE_REPAIR_MAX_ATTEMPTS", "count", "scalar", "Result-changing coverage-repair cap, not a resource exclusion.";
    decimation_adaptation_policy: DecimationAdaptationPolicy = DecimationAdaptationPolicy::new(3, 2.0, 0.6)?;
    "DECIMATION_ADAPTATION_POLICY", "1", "record{adaptations,error_multiplier,feature_multiplier}", "Structured feature-protection adaptation policy.";
    decimation_feature_halo_mm: PositiveFinite = PositiveFinite::new(0.4)?;
    "DECIMATION_FEATURE_HALO_MM", "mm", "scalar", "Feature-protection halo.";
    decimation_feature_probe_ratio: UnitFraction = UnitFraction::new(0.2)?;
    "DECIMATION_FEATURE_PROBE_RATIO", "1", "scalar", "Aggressive feature-probe ratio.";
    decimation_p999_factor: UnitFraction = UnitFraction::new(1.0 / 1000.0)?;
    "DECIMATION_P999_FACTOR", "1", "scalar", "Selected pixel-pitch divisor fraction; arithmetic does not establish authority.";
    decimation_p999_sample_policy: DecimationSamplingPolicy = DecimationSamplingPolicy::new(1, 150_000)?;
    "DECIMATION_P999_SAMPLE_POLICY", "count", "record{cap,seed}", "Forward percentile sampling seed and cap.";
    decimation_p99_factor: UnitFraction = UnitFraction::new(1.0 / 2000.0)?;
    "DECIMATION_P99_FACTOR", "1", "scalar", "Selected pixel-pitch divisor fraction; arithmetic does not establish authority.";
    decimation_p99_sample_policy: DecimationSamplingPolicy = DecimationSamplingPolicy::new(42, 120_000)?;
    "DECIMATION_P99_SAMPLE_POLICY", "count", "record{cap,seed}", "Backward percentile sampling seed and cap.";
    decimation_protected_cap_fraction: UnitFraction = UnitFraction::new(0.12)?;
    "DECIMATION_PROTECTED_CAP_FRACTION", "1", "scalar", "Protected-vertex fraction cap.";
    decimation_ratio_search: DecimationRatioSearchPolicy = DecimationRatioSearchPolicy::new(0.05, 1.0, 5)?;
    "DECIMATION_RATIO_SEARCH", "1", "record{bisections,lower,upper}", "Structured bounded ratio-search policy.";
    decimation_recheck_slack: PositiveFinite = PositiveFinite::new(1.1)?;
    "DECIMATION_RECHECK_SLACK", "1", "scalar", "Selected full-population recheck slack, not measured authority.";
    decimation_scale_acceptance_mm: FiniteInterval = FiniteInterval::new(10.0, 300.0, crate::BoundKind::Inclusive, crate::BoundKind::Inclusive)?;
    "DECIMATION_SCALE_ACCEPTANCE_MM", "mm", "interval", "Inclusive scale-sanity acceptance interval.";
    decimation_seed_error_mm: PositiveFinite = PositiveFinite::new(0.050)?;
    "DECIMATION_SEED_ERROR_MM", "mm", "scalar", "Feature seed error threshold.";
    decimation_utility_min_input_faces: NonZeroU64 = nonzero(32_768)?;
    "DECIMATION_UTILITY_MIN_INPUT_FACES", "count", "scalar", "Input-face floor below which the downstream-utility witness refuses without examining geometry; corpus-shaped policy, not printer physics, and it decides only whether to attempt, never whether a result is safe. It is deliberately NOT derivable: gating on 'finer than the printer can resolve' (area / (pixel x layer)) puts a mini-sized model's resolvable face count near 1e6, which would forbid decimating the 429,120-face Weeping Treant too. The honest statement is that downstream cost is superlinear in face count while the geometric price is bounded by the Derived deviation limits. Value: the smallest power of two strictly above 4x the largest reachable corpus model (Sphere, 4,760 faces), chosen over 65,536 because it sits below the 65,024-face thread_sweep UV sphere and so gives the reduced path cross-thread determinism coverage at no frozen-digest cost. Single-specimen basis (R4-shaped standing trigger): n = 1 large specimen; a second corpus model above ~4,760 faces triggers re-derivation, never silent inheritance.";
    decimation_vertex_group_factor: PositiveFinite = PositiveFinite::new(10.0)?;
    "DECIMATION_VERTEX_GROUP_FACTOR", "1", "scalar", "Protected vertex-group sizing factor.";
    default_contact_depth_mm: PositiveFinite = PositiveFinite::new(0.3)?;
    "DEFAULT_CONTACT_DEPTH_MM", "mm", "scalar", "Generation/profile default, not ChituBox format authority.";
    delicacy_curvature_weight: UnitFraction = UnitFraction::new(0.6)?;
    "DELICACY_CURVATURE_WEIGHT", "1", "scalar", "Curvature proxy blend weight.";
    delicacy_thinness_weight: UnitFraction = UnitFraction::new(0.4)?;
    "DELICACY_THINNESS_WEIGHT", "1", "scalar", "Thinness proxy blend weight.";
    emit_require_passed_gates_default: bool = true;
    "EMIT_REQUIRE_PASSED_GATES_DEFAULT", "1", "scalar", "Emission authorization default; blocking defects always refuse emission.";
    enclosure_proof_tol_mm: PositiveFinite = PositiveFinite::new(0.018)?;
    "ENCLOSURE_PROOF_TOL_MM", "mm", "scalar", "Arithmetic from enclosure tolerance does not improve Placeholder authority.";
    enclosure_tol_mm: PositiveFinite = PositiveFinite::new(0.02)?;
    "ENCLOSURE_TOL_MM", "mm", "scalar", "Enclosure tolerance whose old pixel derivation is rejected.";
    engine_surface_tree_max_points: NonZeroU64 = nonzero(400_000)?;
    "ENGINE_SURFACE_TREE_MAX_POINTS", "count", "scalar", "Live engine sampled-surface override.";
    engine_surface_tree_pitch_mm: PositiveFinite = PositiveFinite::new(0.3)?;
    "ENGINE_SURFACE_TREE_PITCH_MM", "mm", "scalar", "Live engine sampled-surface pitch override.";
    eps_cos: PositiveFinite = PositiveFinite::new(1e-9)?;
    "EPS_COS", "1", "scalar", "Selected divide-by-zero guard, explicitly not a physics floor.";
    expected_chitubox_api_version: NonZeroU64 = nonzero(1)?;
    "EXPECTED_CHITUBOX_API_VERSION", "count", "scalar", "Native writer compatibility version checked before emission.";
    flare_len_mm: PositiveFinite = PositiveFinite::new(1.5)?;
    "FLARE_LEN_MM", "mm", "scalar", "Conical flare length, not the missing fast-flare term.";
    flare_r_factor: PositiveFinite = PositiveFinite::new(1.6)?;
    "FLARE_R_FACTOR", "1", "scalar", "Conical flare radius factor.";
    gate_on_ball_breach_default: bool = true;
    "GATE_ON_BALL_BREACH_DEFAULT", "1", "scalar", "Independent ball-breach verification default.";
    gate_on_removal_default: bool = false;
    "GATE_ON_REMOVAL_DEFAULT", "1", "scalar", "Removal metric remains opt-in by default.";
    grip_clear_mm: PositiveFinite = PositiveFinite::new(5.0)?;
    "GRIP_CLEAR_MM", "mm", "scalar", "Uncalibrated opt-in removal clearance.";
    grip_handle_len_mm: PositiveFinite = PositiveFinite::new(10.0)?;
    "GRIP_HANDLE_LEN_MM", "mm", "scalar", "Uncalibrated opt-in graspable length.";
    grip_handle_r_mm: PositiveFinite = PositiveFinite::new(1.0)?;
    "GRIP_HANDLE_R_MM", "mm", "scalar", "Uncalibrated opt-in graspable radius.";
    hollow_allow_output_default: bool = false;
    "HOLLOW_ALLOW_OUTPUT_DEFAULT", "1", "scalar", "Hollow output remains disabled without a dedicated verifier.";
    hollow_breathing_voxel_pitch_mm: PositiveFinite = PositiveFinite::new(0.4)?;
    "HOLLOW_BREATHING_VOXEL_PITCH_MM", "mm", "scalar", "Final cavity-breathing verification voxel pitch.";
    hollow_bulk_min_mm: PositiveFinite = PositiveFinite::new(10.0)?;
    "HOLLOW_BULK_MIN_MM", "mm", "scalar", "Hollowing bulk-size policy.";
    hollow_containment_min_fraction: UnitFraction = UnitFraction::new(0.9)?;
    "HOLLOW_CONTAINMENT_MIN_FRACTION", "1", "scalar", "Internal-component containment threshold.";
    hollow_containment_samples: NonZeroU64 = nonzero(32)?;
    "HOLLOW_CONTAINMENT_SAMPLES", "count", "scalar", "Internal-component containment sample count.";
    hollow_drain_directions: DrainDirections = DrainDirections::new([
        DrainDirection::new(0.0, 0.0, -1.0)?,
        DrainDirection::new(0.5, 0.0, -1.0)?,
        DrainDirection::new(-0.5, 0.0, -1.0)?,
        DrainDirection::new(0.0, 0.5, -1.0)?,
        DrainDirection::new(0.0, -0.5, -1.0)?,
    ])?;
    "HOLLOW_DRAIN_DIRECTIONS", "1", "list[5]<record{x,y,z}>", "One ordered structured set of five downward drain-search directions.";
    hollow_drain_r_mm: PositiveFinite = PositiveFinite::new(1.5)?;
    "HOLLOW_DRAIN_R_MM", "mm", "scalar", "Drain-hole radius policy.";
    hollow_exit_ray_max_mm: PositiveFinite = PositiveFinite::new(45.0)?;
    "HOLLOW_EXIT_RAY_MAX_MM", "mm", "scalar", "Drain and vent exit-search ceiling.";
    hollow_extra_drain_policy: HollowExtraDrainPolicy = HollowExtraDrainPolicy::new(0.5, 3, 8.0, &[4.0, 8.0, 9.0], 24)?;
    "HOLLOW_EXTRA_DRAIN_POLICY", "mixed", "record{cylinder_sides,exit_max_mm,extensions_mm,max_extra_drains,min_pocket_ml}", "Structured trapped-pocket repair policy; all conditions travel together.";
    hollow_lobe_band_mm: PositiveFinite = PositiveFinite::new(15.0)?;
    "HOLLOW_LOBE_BAND_MM", "mm", "scalar", "Hollow lobe spatial band.";
    hollow_lobe_min_ml: PositiveFinite = PositiveFinite::new(0.5)?;
    "HOLLOW_LOBE_MIN_ML", "mL", "scalar", "Minimum hollow lobe volume.";
    hollow_orient_enable_default: bool = true;
    "HOLLOW_ORIENT_ENABLE_DEFAULT", "1", "scalar", "Independent hollow-orientation enable default.";
    hollow_pitch_mm: PositiveFinite = PositiveFinite::new(0.5)?;
    "HOLLOW_PITCH_MM", "mm", "scalar", "Hollowing pitch policy.";
    hollow_save_min_frac: UnitFraction = UnitFraction::new(0.25)?;
    "HOLLOW_SAVE_MIN_FRAC", "1", "scalar", "Minimum fractional resin saving.";
    hollow_save_min_ml: PositiveFinite = PositiveFinite::new(15.0)?;
    "HOLLOW_SAVE_MIN_ML", "mL", "scalar", "Minimum absolute resin saving.";
    hollow_shard_max_dim_mm: PositiveFinite = PositiveFinite::new(1.0)?;
    "HOLLOW_SHARD_MAX_DIM_MM", "mm", "scalar", "Component-triage dimension ceiling.";
    hollow_shard_max_faces: NonZeroU64 = nonzero(64)?;
    "HOLLOW_SHARD_MAX_FACES", "count", "scalar", "Component-triage face ceiling.";
    hollow_shard_max_volume_mm3: PositiveFinite = PositiveFinite::new(0.01)?;
    "HOLLOW_SHARD_MAX_VOLUME_MM3", "mm³", "scalar", "Component-triage volume ceiling.";
    hollow_vent_directions: VentDirections = VentDirections::new([
        DrainDirection::new(1.0, 0.0, 0.0)?,
        DrainDirection::new(-1.0, 0.0, 0.0)?,
        DrainDirection::new(0.0, 1.0, 0.0)?,
        DrainDirection::new(0.0, -1.0, 0.0)?,
    ])?;
    "HOLLOW_VENT_DIRECTIONS", "1", "list[4]<record{x,y,z}>", "Ordered horizontal vent-search directions.";
    hollow_vent_r_mm: PositiveFinite = PositiveFinite::new(1.25)?;
    "HOLLOW_VENT_R_MM", "mm", "scalar", "Vent-hole radius policy.";
    hollow_vent_sample_divisor: NonZeroU64 = nonzero(40)?;
    "HOLLOW_VENT_SAMPLE_DIVISOR", "count", "scalar", "Top-band vent-site deterministic sampling divisor.";
    hollow_vent_top_band_mm: PositiveFinite = PositiveFinite::new(2.0)?;
    "HOLLOW_VENT_TOP_BAND_MM", "mm", "scalar", "Cavity-top band searched for a horizontal vent site.";
    hollow_volume_balance_rel_tol: UnitFraction = UnitFraction::new(0.02)?;
    "HOLLOW_VOLUME_BALANCE_REL_TOL", "1", "scalar", "Final volume-balance tolerance relative to solid volume.";
    hollow_wall_default_mm: PositiveFinite = PositiveFinite::new(2.5)?;
    "HOLLOW_WALL_DEFAULT_MM", "mm", "scalar", "Hollow generation and tool wall default.";
    initial_axis_majority_cap: NonZeroU64 = nonzero(80)?;
    "INITIAL_AXIS_MAJORITY_CAP", "count", "scalar", "O-10 legacy compatibility interval majority cap; docs/engineering-journal/04-migration-plan.md §O-10, not safety evidence.";
    initial_axis_majority_step_mm: PositiveFinite = PositiveFinite::new(0.12)?;
    "INITIAL_AXIS_MAJORITY_STEP_MM", "mm", "scalar", "O-10 legacy compatibility interval majority step; docs/engineering-journal/04-migration-plan.md §O-10, not safety evidence.";
    island_attach_reach_mm: PositiveFinite = PositiveFinite::new(0.2)?;
    "ISLAND_ATTACH_REACH_MM", "mm", "scalar", "Tuning tied to the legacy Placeholder tip default.";
    island_birth_link_r_mm: PositiveFinite = PositiveFinite::new(0.6)?;
    "ISLAND_BIRTH_LINK_R_MM", "mm", "scalar", "Tuned link radius invalidated by dynamic layer height.";
    island_merge_radius_mm: PositiveFinite = PositiveFinite::new(2.0)?;
    "ISLAND_MERGE_RADIUS_MM", "mm", "scalar", "Geometric closing radius, not physics.";
    joint_ball_factor: PositiveFinite = PositiveFinite::new(1.0)?;
    "JOINT_BALL_FACTOR", "1", "scalar", "Joint-ball geometry factor.";
    kappa: PositiveFinite = PositiveFinite::new(1.25)?;
    "KAPPA", "1", "scalar", "Owner-ratified crossing-disc headroom multiplier.";
    lift_mm: PositiveFinite = PositiveFinite::new(5.0)?;
    "LIFT_MM", "mm", "scalar", "Canonical local printer-profile default; retired 3.0 pipeline overrides are not aliases.";
    lift_tol_mm: PositiveFinite = PositiveFinite::new(1e-6)?;
    "LIFT_TOL_MM", "mm", "scalar", "Output frame-invariant tolerance for centered Z minus plate Z.";
    l_scar_max_mm: PositiveFinite = PositiveFinite::new(2.5)?;
    "L_SCAR_MAX_MM", "mm", "scalar", "Owner-ratified maximum scar-patch dimension.";
    material_cost_per_mm: NonNegativeFinite = NonNegativeFinite::new(0.05)?;
    "MATERIAL_COST_PER_MM", "per-mm", "scalar", "Strut-selection material cost weight.";
    max_deflection_mm: PositiveFinite = PositiveFinite::new(0.15)?;
    "MAX_DEFLECTION_MM", "mm", "scalar", "Subjective visual-ribbing serviceability threshold.";
    max_segment_angle_deg: PositiveFinite = PositiveFinite::new(45.0)?;
    "MAX_SEGMENT_ANGLE_DEG", "°", "scalar", "Qualitative printability policy, not measured authority.";
    max_trunk_r_mm: PositiveFinite = PositiveFinite::new(2.4)?;
    "MAX_TRUNK_R_MM", "mm", "scalar", "Live trunk-radius ceiling.";
    max_unbraced_shaft_r_mm: PositiveFinite = PositiveFinite::new(0.9)?;
    "MAX_UNBRACED_SHAFT_R_MM", "mm", "scalar", "Corpus rationale does not derive the unbraced-shaft ceiling.";
    minima_birth_match_layers: PositiveFinite = PositiveFinite::new(2.0)?;
    "MINIMA_BIRTH_MATCH_LAYERS", "layers", "scalar", "Corpus rule with selected headroom, invalidated by per-layer heights.";
    minima_connect_layers: PositiveFinite = PositiveFinite::new(2.0)?;
    "MINIMA_CONNECT_LAYERS", "layers", "scalar", "Corpus rule with selected headroom, invalidated by per-layer heights.";
    minima_contact_radius_mm: PositiveFinite = PositiveFinite::new(1.0)?;
    "MINIMA_CONTACT_RADIUS_MM", "mm", "scalar", "Minima contact discretization slack, not coverage radius.";
    minima_eps_mm: PositiveFinite = PositiveFinite::new(0.02)?;
    "MINIMA_EPS_MM", "mm", "scalar", "Minima-stage sub-layer tie tolerance.";
    minima_support_area_tips: PositiveFinite = PositiveFinite::new(1.0)?;
    "MINIMA_SUPPORT_AREA_TIPS", "1", "scalar", "Selected tuning in a flat sensitivity region, not a measured boundary.";
    min_contacts_per_island: NonZeroU64 = nonzero(1)?;
    "MIN_CONTACTS_PER_ISLAND", "count", "scalar", "Minimum placement contacts per island.";
    min_illegal_len_mm: PositiveFinite = PositiveFinite::new(0.05)?;
    "MIN_ILLEGAL_LEN_MM", "mm", "scalar", "Post-anchoring crossing sliver cutoff.";
    min_shaft_r_mm: PositiveFinite = PositiveFinite::new(0.3)?;
    "MIN_SHAFT_R_MM", "mm", "scalar", "Minimum shaft radius.";
    min_shoot_r_mm: PositiveFinite = PositiveFinite::new(0.35)?;
    "MIN_SHOOT_R_MM", "mm", "scalar", "Minimum routed shoot/member radius, distinct from tip diameter.";
    min_trunk_r_mm: PositiveFinite = PositiveFinite::new(0.8)?;
    "MIN_TRUNK_R_MM", "mm", "scalar", "Minimum trunk radius.";
    model_brace_max_len_mm: PositiveFinite = PositiveFinite::new(5.0)?;
    "MODEL_BRACE_MAX_LEN_MM", "mm", "scalar", "Maximum model-brace length tuning.";
    modulus_green_gpa: PositiveFinite = PositiveFinite::new(0.5)?;
    "MODULUS_GREEN_GPA", "GPa", "scalar", "Legacy diagnostic modulus with no plate-5 measurement.";
    murray_tolerance: PositiveFinite = PositiveFinite::new(0.05)?;
    "MURRAY_TOLERANCE", "1", "scalar", "Murray gate tolerance.";
    near_plate_mm: PositiveFinite = PositiveFinite::new(1.5)?;
    "NEAR_PLATE_MM", "mm", "scalar", "Near-plate perception band.";
    on_axis_tol_mm: PositiveFinite = PositiveFinite::new(0.2)?;
    "ON_AXIS_TOL_MM", "mm", "scalar", "Joint on-axis tolerance.";
    orient_scar_weight: NonNegativeFinite = NonNegativeFinite::new(0.4)?;
    "ORIENT_SCAR_WEIGHT", "1", "scalar", "Opt-in additive scar weight; zero reproduces the pre-scar objective.";
    overhang_angle_deg: PositiveFinite = PositiveFinite::new(45.0)?;
    "OVERHANG_ANGLE_DEG", "°", "scalar", "Legacy tool convention lacking the required geometric-extent criterion.";
    p8_ball_slide_max_mm: PositiveFinite = PositiveFinite::new(2.0)?;
    "P8_BALL_SLIDE_MAX_MM", "mm", "scalar", "Phase-8 ball-slide search reach.";
    p8_ball_slide_step_mm: PositiveFinite = PositiveFinite::new(0.25)?;
    "P8_BALL_SLIDE_STEP_MM", "mm", "scalar", "Phase-8 ball-slide search step.";
    p8_junction_reseat_enable: bool = true;
    "P8_JUNCTION_RESEAT_ENABLE", "1", "scalar", "Independent junction reseat enable.";
    p8_junction_reseat_max_mm: PositiveFinite = PositiveFinite::new(1.0)?;
    "P8_JUNCTION_RESEAT_MAX_MM", "mm", "scalar", "Junction reseat reach.";
    p8_junction_reseat_step_mm: PositiveFinite = PositiveFinite::new(0.25)?;
    "P8_JUNCTION_RESEAT_STEP_MM", "mm", "scalar", "Junction reseat grid step.";
    p8_max_relocate_rounds: NonZeroU64 = nonzero(4)?;
    "P8_MAX_RELOCATE_ROUNDS", "count", "scalar", "Relocation backstop whose live fallbacks agree.";
    p8_max_relocate_sweeps: NonZeroU64 = nonzero(8)?;
    "P8_MAX_RELOCATE_SWEEPS", "count", "scalar", "Relocation sweep backstop whose live fallbacks agree.";
    p8_max_unresolved: NonZeroU64 = nonzero(10)?;
    "P8_MAX_UNRESOLVED", "count", "scalar", "Honest-failure unresolved-site bound.";
    p8_max_unresolved_weight: PositiveFinite = PositiveFinite::new(3.0)?;
    "P8_MAX_UNRESOLVED_WEIGHT", "1", "scalar", "Honest-failure unresolved-weight bound.";
    p8_pocket_corridor_pad_mm: PositiveFinite = PositiveFinite::new(0.5)?;
    "P8_POCKET_CORRIDOR_PAD_MM", "mm", "scalar", "Pocket detection halo and density-penalty radius.";
    p8_pocket_min_members: NonZeroU64 = nonzero(8)?;
    "P8_POCKET_MIN_MEMBERS", "count", "scalar", "Minimum defect members in a pocket component.";
    p8_reaim_azimuth_deg: Vec<u64> = vec![0, 45, 90, 135, 180, 225, 270, 315];
    "P8_REAIM_AZIMUTH_DEG", "°", "list[8]", "Pinned deterministic azimuth enumeration order.";
    p8_reaim_length_factors: Vec<UnitFraction> = fraction_values(&[1.0, 0.75, 0.5])?;
    "P8_REAIM_LENGTH_FACTORS", "1", "list[3]", "Pinned shortening-length ladder.";
    p8_reaim_polar_deg: Vec<u64> = vec![0, 10, 20, 30];
    "P8_REAIM_POLAR_DEG", "°", "list[4]", "Pinned polar cone grid.";
    p8_reaim_route_growth_max_factor: PositiveFinite = PositiveFinite::new(1.50)?;
    "P8_REAIM_ROUTE_GROWTH_MAX_FACTOR", "1", "scalar", "Phase-8 route-growth honest-failure bound.";
    p8_reaim_volume_delta_max_ml: PositiveFinite = PositiveFinite::new(0.02)?;
    "P8_REAIM_VOLUME_DELTA_MAX_ML", "mL", "scalar", "Phase-8 volume-delta honest-failure bound.";
    p8_relocate_enable: bool = true;
    "P8_RELOCATE_ENABLE", "1", "scalar", "Canonical migration target true; conflicting false compatibility fallback is not ported.";
    p8_reseat_enable: bool = true;
    "P8_RESEAT_ENABLE", "1", "scalar", "Surface reseat enable with consistent live true fallbacks.";
    p8_reseat_max_seeds: NonZeroU64 = nonzero(24)?;
    "P8_RESEAT_MAX_SEEDS", "count", "scalar", "Surface reseat candidate cap.";
    p8_reseat_radius_mm: PositiveFinite = PositiveFinite::new(2.0)?;
    "P8_RESEAT_RADIUS_MM", "mm", "scalar", "Surface reseat search radius.";
    p8_resin_delta_max: UnitFraction = UnitFraction::new(0.03)?;
    "P8_RESIN_DELTA_MAX", "1", "scalar", "Phase-8 resin-delta honest-failure bound.";
    p8_resize_fixed_point_max_rounds: NonZeroU64 = nonzero(4)?;
    "P8_RESIZE_FIXED_POINT_MAX_ROUNDS", "count", "scalar", "Resize fixed-point backstop whose live fallback agrees.";
    p8_site_commit_budget: NonZeroU64 = nonzero(2)?;
    "P8_SITE_COMMIT_BUDGET", "count", "scalar", "Per-site Phase-8 commit budget.";
    p8_strut_growth_max_factor: PositiveFinite = PositiveFinite::new(1.25)?;
    "P8_STRUT_GROWTH_MAX_FACTOR", "1", "scalar", "Phase-8 strut-growth honest-failure bound.";
    peel_elevations_deg: Vec<u64> = vec![0, 25, 45, 65, 90];
    "PEEL_ELEVATIONS_DEG", "°", "list[5]", "Uncalibrated ordered elevations for the opt-in removal metric.";
    peel_pressure_pa: PositiveFinite = PositiveFinite::new(3000.0)?;
    "PEEL_PRESSURE_PA", "Pa", "scalar", "Legacy diagnostic split, potentially about four times low.";
    perf_anchor_cap: TrialSetting<AnchorCandidateCap> = TrialSetting::disabled();
    "PERF_ANCHOR_CAP", "count", "trial_setting<anchor_candidate_cap>", "Experimental B2 remains Disabled; no enabled candidate is selected.";
    perf_beam_k: TrialSetting<BeamWidth> = TrialSetting::disabled();
    "PERF_BEAM_K", "count", "trial_setting<beam_width>", "Experimental B1 remains Disabled; no enabled candidate is selected.";
    perf_fix_iter_caps: TrialSetting<FixBraceIterationCap> = TrialSetting::disabled();
    "PERF_FIX_ITER_CAPS", "count", "trial_setting<brace_iteration_cap>", "Experimental B4 models only the consumed brace-iteration cap and remains Disabled.";
    perf_seed_spacing_factor: TrialSetting<SeedSpacingFactor> = TrialSetting::disabled();
    "PERF_SEED_SPACING_FACTOR", "1", "trial_setting<seed_spacing_factor>", "Experimental B3 remains Disabled and cannot loosen the coverage gate.";
    pipeline_hollow_enable_default: bool = true;
    "PIPELINE_HOLLOW_ENABLE_DEFAULT", "1", "scalar", "Pipeline hollow-stage interface default.";
    pipeline_p8_relocate_default: bool = false;
    "PIPELINE_P8_RELOCATE_DEFAULT", "1", "scalar", "CLI pipeline default distinct from the canonical Phase-8 config enable.";
    plate_band_mm: PositiveFinite = PositiveFinite::new(0.2)?;
    "PLATE_BAND_MM", "mm", "scalar", "One owner for joint and connectivity plate-band tolerance.";
    pull_dist_mm: PositiveFinite = PositiveFinite::new(15.0)?;
    "PULL_DIST_MM", "mm", "scalar", "Uncalibrated opt-in removal translation distance.";
    rewire_degenerate_len_mm: PositiveFinite = PositiveFinite::new(1e-4)?;
    "REWIRE_DEGENERATE_LEN_MM", "mm", "scalar", "Result-changing degenerate-span threshold.";
    rewire_max_slide_evals: NonZeroU64 = nonzero(50)?;
    "REWIRE_MAX_SLIDE_EVALS", "count", "scalar", "Deterministic slide-evaluation cap.";
    rewire_roll_eps: UnitFraction = UnitFraction::new(0.0025)?;
    "REWIRE_ROLL_EPS", "1", "scalar", "Rolling-savings convergence fraction.";
    rewire_roll_window: NonZeroU64 = nonzero(20)?;
    "REWIRE_ROLL_WINDOW", "count", "scalar", "Rolling-savings convergence window.";
    rewire_twig_merge_len_mm: PositiveFinite = PositiveFinite::new(0.5)?;
    "REWIRE_TWIG_MERGE_LEN_MM", "mm", "scalar", "Result-changing twig-merge geometry threshold.";
    rewire_work_budget: TrialSetting<NonZeroU64> = TrialSetting::disabled();
    "REWIRE_WORK_BUDGET", "count", "trial_setting<nonzero_u64>", "B5 exact-search baseline is Disabled with no wall-clock fallback and no selected enabled budget.";
    scar_cost_weight: NonNegativeFinite = NonNegativeFinite::new(1.0)?;
    "SCAR_COST_WEIGHT", "1", "scalar", "Strut-selection scar cost weight.";
    slice_union_max_abs_mm: PositiveFinite = PositiveFinite::new(1.0e4)?;
    "SLICE_UNION_MAX_ABS_MM", "mm", "scalar", "FU-10 layer-normalizer coordinate domain half-width; keeps quantized magnitudes at or below 1e12 so i128 wide arithmetic in the pinned integer overlay engine cannot overflow. A refusal boundary, not printable reach.";
    slice_union_quantum_mm: PositiveFinite = PositiveFinite::new(1.0e-8)?;
    "SLICE_UNION_QUANTUM_MM", "mm", "scalar", "FU-10 layer-normalizer quantization step; a determinism contract, not a tunable. Equals the perception stitch key LOOP_MERGE_MM so stitched vertices quantize losslessly (design OQ-1 i64 branch).";
    spacing_floor_mm: PositiveFinite = PositiveFinite::new(2.5)?;
    "SPACING_FLOOR_MM", "mm", "scalar", "Removal-ergonomics spacing floor selected without measurement.";
    standoff_mm: PositiveFinite = PositiveFinite::new(0.6)?;
    "STANDOFF_MM", "mm", "scalar", "Legacy clearance policy whose corpus use does not derive the distance.";
    strut_fallback_max_angle_deg: PositiveFinite = PositiveFinite::new(60.0)?;
    "STRUT_FALLBACK_MAX_ANGLE_DEG", "°", "scalar", "Fallback strut routing angle.";
    strut_max_length_mm: PositiveFinite = PositiveFinite::new(20.0)?;
    "STRUT_MAX_LENGTH_MM", "mm", "scalar", "Corpus outcomes do not establish a safe strut maximum.";
    strut_min_length_saved_mm: PositiveFinite = PositiveFinite::new(8.0)?;
    "STRUT_MIN_LENGTH_SAVED_MM", "mm", "scalar", "Minimum material saving that justifies a second scar.";
    strut_model_anchor_max_candidates: NonZeroU64 = nonzero(40)?;
    "STRUT_MODEL_ANCHOR_MAX_CANDIDATES", "count", "scalar", "Model-anchor candidate-selection cap.";
    strut_model_anchor_max_delicacy: UnitFraction = UnitFraction::new(0.5)?;
    "STRUT_MODEL_ANCHOR_MAX_DELICACY", "1", "scalar", "Model-anchor delicacy ceiling.";
    strut_model_anchor_min_len_mm: PositiveFinite = PositiveFinite::new(5.0)?;
    "STRUT_MODEL_ANCHOR_MIN_LEN_MM", "mm", "scalar", "Model-anchor candidate length floor.";
    stub_length_mm: PositiveFinite = PositiveFinite::new(2.5)?;
    "STUB_LENGTH_MM", "mm", "scalar", "Support stub length.";
    support_brace_penetration_max_rows: NonZeroU64 = nonzero(100_000)?;
    "SUPPORT_BRACE_PENETRATION_MAX_ROWS", "count", "scalar", "Inherited unmeasured cap on brace clearance-rod penetration-batch rows (SupportPreparationLimits::production's brace_penetration_limits, PenetrationBatchLimits.max_rows). Codex-era 73e7c13; registered by FU-8g phase H3 (2026-08-09), which records the inherited number without ratifying it. The brace stage is unexercised on every corpus model and on the Weeping Treant v1.2 at this tip -- no run has ever reached it (support-resource-caps-2026-08-05.md section 17.7) -- so no measurement of record exists for its demand. No cap was raised, lowered or touched by this registration.";
    support_brace_penetration_max_scratch_bytes: NonZeroU64 = nonzero(100_000_000_000)?;
    "SUPPORT_BRACE_PENETRATION_MAX_SCRATCH_BYTES", "B", "scalar", "Inherited unmeasured cap on brace clearance-rod penetration-batch scratch bytes (SupportPreparationLimits::production's brace_penetration_limits, PenetrationBatchLimits.max_scratch_bytes). Codex-era 73e7c13; registered by FU-8g phase H3 (2026-08-09), which records the inherited number without ratifying it. The brace stage is unexercised on every corpus model and on the Weeping Treant v1.2 at this tip -- no measurement of record exists for its demand. The design's F7 finding (single specimen, n = 1, per R4/FU-23) is a DERIVED ceiling from the worst-case sizing formula, not a measured demand: at rays_all=false, cap=80, T=429,120 faces (the Weeping Treant v1.2), the per-row worst-case charge is scratch_bytes 1,171,071,249 B, so this cap alone admits at most 85 rows before any other brace cap binds -- and is simultaneously ABOVE the 90 GB watchdog line and the 100 GB machine ceiling (memory machine-memory-ceiling; support-resource-caps-2026-08-05.md section 17.4/A1), a standing hazard this registration discloses and does not resolve. No cap was raised, lowered or touched.";
    support_brace_penetration_max_total_hits: NonZeroU64 = nonzero(1_000_000_000)?;
    "SUPPORT_BRACE_PENETRATION_MAX_TOTAL_HITS", "count", "scalar", "Inherited unmeasured cap on brace clearance-rod penetration-batch total hits (SupportPreparationLimits::production's brace_penetration_limits, PenetrationBatchLimits.max_total_hits). Codex-era 73e7c13; registered by FU-8g phase H3 (2026-08-09), which records the inherited number without ratifying it. The brace stage is unexercised on every corpus model and on the Weeping Treant v1.2 at this tip -- no measurement of record exists for its demand. Design finding F9 (fu8g-design-2026-08-08.md) shows hits and work are the same expression at the worst-case sizing this cap is checked against, so the design's F7 derivation for max_total_work (per-row charge 429,120, admitting at most 2,330 rows before this cap alone) applies here too. No cap was raised, lowered or touched.";
    support_brace_penetration_max_total_intervals: NonZeroU64 = nonzero(1_000_000_000)?;
    "SUPPORT_BRACE_PENETRATION_MAX_TOTAL_INTERVALS", "count", "scalar", "Inherited unmeasured cap on brace clearance-rod penetration-batch total intervals (SupportPreparationLimits::production's brace_penetration_limits, PenetrationBatchLimits.max_total_intervals). Codex-era 73e7c13; registered by FU-8g phase H3 (2026-08-09), which records the inherited number without ratifying it. The brace stage is unexercised on every corpus model and on the Weeping Treant v1.2 at this tip -- no measurement of record exists for its demand. The design's F7 finding (single specimen, n = 1, per R4/FU-23) is a DERIVED ceiling from the worst-case sizing formula, not a measured demand: at rays_all=false, cap=80, T=429,120 faces (the Weeping Treant v1.2), the per-row worst-case charge is intervals 429,121, so this cap alone admits at most 2,330 rows before any other brace cap binds. No cap was raised, lowered or touched.";
    support_brace_penetration_max_total_samples: NonZeroU64 = nonzero(10_000_000_000)?;
    "SUPPORT_BRACE_PENETRATION_MAX_TOTAL_SAMPLES", "count", "scalar", "Inherited unmeasured cap on brace clearance-rod penetration-batch total samples (SupportPreparationLimits::production's brace_penetration_limits, PenetrationBatchLimits.max_total_samples). Codex-era 73e7c13; registered by FU-8g phase H3 (2026-08-09), which records the inherited number without ratifying it. The brace stage is unexercised on every corpus model and on the Weeping Treant v1.2 at this tip -- no measurement of record exists for its demand. The design's F7 finding (single specimen, n = 1, per R4/FU-23) is a DERIVED ceiling from the worst-case sizing formula, not a measured demand: at rays_all=false, cap=80, T=429,120 faces (the Weeping Treant v1.2), the per-row worst-case charge is samples 34,329,680, so this cap alone admits at most 291 rows before any other brace cap binds. No cap was raised, lowered or touched.";
    support_brace_penetration_max_total_work: NonZeroU64 = nonzero(1_000_000_000)?;
    "SUPPORT_BRACE_PENETRATION_MAX_TOTAL_WORK", "count", "scalar", "Inherited unmeasured cap on brace clearance-rod penetration-batch total work (SupportPreparationLimits::production's brace_penetration_limits, PenetrationBatchLimits.max_total_work). Codex-era 73e7c13; registered by FU-8g phase H3 (2026-08-09), which records the inherited number without ratifying it. The brace stage is unexercised on every corpus model and on the Weeping Treant v1.2 at this tip -- no measurement of record exists for its demand. The design's F7 finding (single specimen, n = 1, per R4/FU-23) is a DERIVED ceiling from the worst-case sizing formula, not a measured demand (fu8g-design-2026-08-08.md F7): at rays_all=false, cap=80, T=429,120 faces (the Weeping Treant v1.2), the per-row worst-case charge is work 429,120, so this cap alone admits at most 2,330 rows before any other brace cap binds -- production admits at most 85 clearance rods on this mesh once max_scratch_bytes is accounted for. No cap was raised, lowered or touched.";
    support_construction_max_contacts: NonZeroU64 = nonzero(1_000_000)?;
    "SUPPORT_CONSTRUCTION_MAX_CONTACTS", "count", "scalar", "Inherited unmeasured cap on physical contacts admitted before initial-contact construction allocates (ContactConstructionLimits::new/::production, crates/plinth-support/src/initial.rs, and SupportPreparationLimits::production's construction_limits, crates/plinth-support/src/runner.rs -- the same four numbers are hard-coded at both sites, fu8g-design-2026-08-08.md F1). Codex-era 73e7c13; registered by FU-8g phase H3 (2026-08-09), which records the inherited number without ratifying it. Single-specimen observation (n = 1, per R4/FU-23): the Weeping Treant v1.2's FU-31-fixed acceptance run charges 553 contacts, 0.055 % of this cap; every corpus model's contact count is smaller still. No cap was raised, lowered or touched.";
    support_construction_max_load_rows: NonZeroU64 = nonzero(10_000_000)?;
    "SUPPORT_CONSTRUCTION_MAX_LOAD_ROWS", "count", "scalar", "Inherited unmeasured cap on load-partition rows checked before initial-contact construction allocates (ContactConstructionLimits::new/::production, crates/plinth-support/src/initial.rs, and SupportPreparationLimits::production's construction_limits, crates/plinth-support/src/runner.rs -- the same four numbers are hard-coded at both sites, fu8g-design-2026-08-08.md F1). Codex-era 73e7c13; registered by FU-8g phase H3 (2026-08-09), which records the inherited number without ratifying it. No measurement of record exists for this quantity on any corpus model or the Weeping Treant v1.2 at this tip: FU-8g's H0/H2 measurements priced the work-items charge and the contact count, not load rows. No cap was raised, lowered or touched.";
    support_construction_max_scratch_bytes: NonZeroU64 = nonzero(536_870_912)?;
    "SUPPORT_CONSTRUCTION_MAX_SCRATCH_BYTES", "B", "scalar", "Inherited unmeasured cap on initial-contact construction's penetration scratch bytes (ContactConstructionLimits::new/::production, crates/plinth-support/src/initial.rs, and SupportPreparationLimits::production's construction_limits, crates/plinth-support/src/runner.rs -- the same four numbers are hard-coded at both sites, fu8g-design-2026-08-08.md F1). Codex-era 73e7c13; registered by FU-8g phase H3 (2026-08-09), which records the inherited number without ratifying it. FU-8g's H0 pre-fix estimate for the Weeping Treant v1.2 (553 contacts, honest crossing-count scratch) was 149,188,000 B (approximately 142 MiB, 27.8 % of this cap); H2 (2026-08-09) explicitly declined to claim a post-H1 per-model byte figure as the measurement of record, leaving that to H6's in-process run. No cap was raised, lowered or touched.";
    support_construction_max_work_items: NonZeroU64 = nonzero(4_000_000)?;
    "SUPPORT_CONSTRUCTION_MAX_WORK_ITEMS", "count", "scalar", "Inherited unmeasured cap on the initial-contact preflight's work-items charge (ContactConstructionLimits::new/::production, crates/plinth-support/src/initial.rs, and SupportPreparationLimits::production's construction_limits, crates/plinth-support/src/runner.rs -- the same four numbers are hard-coded at both sites, fu8g-design-2026-08-08.md F1). Codex-era 73e7c13; registered by FU-8g phase H3 (2026-08-09), which records the inherited number without ratifying it. This is the cap FU-8g exists to make honestly chargeable, not to raise: at develop@e3168c5 the flat contact_count x (4 + nfaces) bound charged the Weeping Treant v1.2 232,585,208 (58.1x over, FU-8f phase G6d, 2026-08-08); FU-8g phases H1/H2 (2026-08-09) replaced that with the census pass's counted work-items total, and the SAME treant run (553 contacts) now charges 108,378 -- 2.71 % of this UNCHANGED cap (single specimen, n = 1, per R4/FU-23). No cap was raised, lowered or touched by H1, H2 or this registration; see support-resource-caps-2026-08-05.md sections 17.5/18 and CURRENT-STATUS.md FU-8g phase H2 landed 2026-08-09.";
    support_knn_max_logical_peak_bytes: NonZeroU64 = nonzero(2147483648)?;
    "SUPPORT_KNN_MAX_LOGICAL_PEAK_BYTES", "B", "scalar", "Inherited unmeasured kNN batch logical-peak byte cap; it refuses a batch whose projected working set is too large before any allocation happens. Codex-era 2bca0c8; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_knn_max_neighbors: NonZeroU64 = nonzero(1)?;
    "SUPPORT_KNN_MAX_NEIGHBORS", "count", "scalar", "Inherited unmeasured kNN neighbor-count cap; the support pipeline only ever asks for the single nearest neighbor, so this is a one-neighbor ceiling rather than a tuning lever. Codex-era 2bca0c8; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it. AMENDED 2026-08-06 (FU-8d phase F1, ruling R-b): the 'only ever asks for the single nearest neighbor' clause was true when written and is no longer true of the whole pipeline. The value 1 registered here is unchanged and still describes SupportPreparationLimits::production's own KnnBatchLimits, which the generic perception limits carry verbatim; but SpecializedLimits::production now restates coverage_route_knn.max_neighbors as 3, because the merge gate's baseline coverage sweep asks for min(3, source_count) neighbours per sample so it can retain the witnesses a merge-proposal delta reads. That raise is a KnnBatchLimits shape field and NOT a cap: KnnBatchPlan's charged work is source_count x query_count, k-independent, so no operation charge moves; the bytes it admits are charged per route inside KnnBatchPlan::logical_peak_bytes and per batch as GatePeakUsage::nearest_witness_bytes. No cap was raised anywhere in FU-8d F1, and all seven frozen OD-8 digests and the Weeping Treant's refusal line are byte-identical across it.";
    support_knn_max_query_points: NonZeroU64 = nonzero(20000000)?;
    "SUPPORT_KNN_MAX_QUERY_POINTS", "count", "scalar", "Inherited unmeasured kNN query-point cap; it bounds one dimension of the work product that SUPPORT_KNN_MAX_WORK_FLOOR bounds as a whole. Codex-era 2bca0c8; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_knn_max_source_points: NonZeroU64 = nonzero(20000000)?;
    "SUPPORT_KNN_MAX_SOURCE_POINTS", "count", "scalar", "Inherited unmeasured kNN source-point cap; it bounds the other dimension of the work product that SUPPORT_KNN_MAX_WORK_FLOOR bounds as a whole. Codex-era 2bca0c8; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_knn_max_work_floor: NonZeroU64 = nonzero(100_000_000)?;
    "SUPPORT_KNN_MAX_WORK_FLOOR", "count", "scalar", "Inherited unmeasured kNN work floor, Codex-era 73e7c13/a8acfb9 with no measurement of record; it is only the constant term under the measured per-face rule and is never the whole cap.";
    support_parity_grid_max_bytes_floor: NonZeroU64 = nonzero(67_108_864)?;
    "SUPPORT_PARITY_GRID_MAX_BYTES_FLOOR", "B", "scalar", "Inherited unmeasured parity-grid byte floor, Codex-era 73e7c13 with no measurement of record; it governs every corpus model coarse enough that references, not faces, dominate the projection.";
    support_perception_max_island_neighbor_bytes: NonZeroU64 = nonzero(2147483648)?;
    "SUPPORT_PERCEPTION_MAX_ISLAND_NEIGHBOR_BYTES", "B", "scalar", "Inherited unmeasured perception cap on island-neighbor pair storage. Codex-era 4f140f0; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_perception_max_island_neighbor_pairs: NonZeroU64 = nonzero(100000000)?;
    "SUPPORT_PERCEPTION_MAX_ISLAND_NEIGHBOR_PAIRS", "count", "scalar", "Inherited unmeasured perception cap on island-neighbor pair count. Codex-era 4f140f0; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_perception_max_loop_vertices: NonZeroU64 = nonzero(100000000)?;
    "SUPPORT_PERCEPTION_MAX_LOOP_VERTICES", "count", "scalar", "Inherited unmeasured perception cap on total slice-loop vertices. Codex-era fc7162d; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_perception_max_minima_vertices: NonZeroU64 = nonzero(20000000)?;
    "SUPPORT_PERCEPTION_MAX_MINIMA_VERTICES", "count", "scalar", "Inherited unmeasured perception cap on detected minima vertices; it bounds the source side of the minima-to-face kNN batch. Codex-era fc7162d; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_perception_max_overlap_bytes: NonZeroU64 = nonzero(2147483648)?;
    "SUPPORT_PERCEPTION_MAX_OVERLAP_BYTES", "B", "scalar", "Inherited unmeasured perception cap on overlap-test scratch bytes. Codex-era 6458c9b; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_perception_max_overlap_edge_tests: NonZeroU64 = nonzero(100000000)?;
    "SUPPORT_PERCEPTION_MAX_OVERLAP_EDGE_TESTS", "count", "scalar", "Inherited unmeasured perception cap on overlap edge tests. Codex-era 4f140f0; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_perception_max_overlap_pairs: NonZeroU64 = nonzero(100000000)?;
    "SUPPORT_PERCEPTION_MAX_OVERLAP_PAIRS", "count", "scalar", "Inherited unmeasured perception cap on overlap candidate pairs. Codex-era 6458c9b; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_perception_max_overlap_vertex_tests: NonZeroU64 = nonzero(100000000)?;
    "SUPPORT_PERCEPTION_MAX_OVERLAP_VERTEX_TESTS", "count", "scalar", "Inherited unmeasured perception cap on overlap vertex tests. Codex-era 4f140f0; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_perception_max_planes: NonZeroU64 = nonzero(20000)?;
    "SUPPORT_PERCEPTION_MAX_PLANES", "count", "scalar", "Inherited unmeasured perception cap on slice planes per run; the only cap in this family denominated in layers rather than in work or bytes. Codex-era cb982ac; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_perception_max_slice_intersections: NonZeroU64 = nonzero(100000000)?;
    "SUPPORT_PERCEPTION_MAX_SLICE_INTERSECTIONS", "count", "scalar", "Inherited unmeasured perception cap on triangle-plane intersection results. Codex-era fc7162d; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_perception_max_slice_output_bytes: NonZeroU64 = nonzero(2147483648)?;
    "SUPPORT_PERCEPTION_MAX_SLICE_OUTPUT_BYTES", "B", "scalar", "Inherited unmeasured perception cap on slice output bytes. Codex-era fc7162d; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_perception_max_triangle_plane_tests: NonZeroU64 = nonzero(100000000)?;
    "SUPPORT_PERCEPTION_MAX_TRIANGLE_PLANE_TESTS", "count", "scalar", "Inherited unmeasured perception cap on triangle-against-plane tests; it is the faces x planes product, so it is the cap most directly exposed to a large mesh. Codex-era cb982ac; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_preparation_coverage_samples_max_virtual_cells: NonZeroU64 = nonzero(2000000)?;
    "SUPPORT_PREPARATION_COVERAGE_SAMPLES_MAX_VIRTUAL_CELLS", "count", "scalar", "Inherited unmeasured cap on virtual coverage-sample cells. Codex-era 19aa4fd; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_preparation_coverage_score_matrix_max_entries: NonZeroU64 = nonzero(65536)?;
    "SUPPORT_PREPARATION_COVERAGE_SCORE_MATRIX_MAX_ENTRIES", "count", "scalar", "Inherited unmeasured cap on coverage score-matrix entries. Codex-era 19aa4fd; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_preparation_max_primitives: NonZeroU64 = nonzero(200000)?;
    "SUPPORT_PREPARATION_MAX_PRIMITIVES", "count", "scalar", "Inherited unmeasured cap on emitted support primitives per run. Codex-era 73611c3; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_preparation_max_primitive_bytes: NonZeroU64 = nonzero(134217728)?;
    "SUPPORT_PREPARATION_MAX_PRIMITIVE_BYTES", "B", "scalar", "Inherited unmeasured cap on emitted support primitive bytes per run. Codex-era 73611c3; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_raft_max_bytes: NonZeroU64 = nonzero(134_217_728)?;
    "SUPPORT_RAFT_MAX_BYTES", "B", "scalar", "Inherited unmeasured cap on raft-plate construction bytes (SupportPreparationLimits::production's raft_limits, RaftLimits::try_new max_bytes, crates/plinth-support/src/raft.rs). Codex-era 73e7c13; registered by FU-8g phase H3 (2026-08-09), which records the inherited number without ratifying it. The raft stage is unexercised on every corpus model and on the Weeping Treant v1.2 at this tip -- the pipeline has never reached it, no treant seal exists (support-resource-caps-2026-08-05.md section 17.7) -- so no measurement of record exists for its demand. No cap was raised, lowered or touched.";
    support_raft_max_candidates: NonZeroU64 = nonzero(100_000)?;
    "SUPPORT_RAFT_MAX_CANDIDATES", "count", "scalar", "Inherited unmeasured cap on raft-plate candidate count (SupportPreparationLimits::production's raft_limits, RaftLimits::try_new max_candidates, crates/plinth-support/src/raft.rs). Codex-era 73e7c13; registered by FU-8g phase H3 (2026-08-09), which records the inherited number without ratifying it. The raft stage is unexercised on every corpus model and on the Weeping Treant v1.2 at this tip -- no measurement of record exists for its demand. No cap was raised, lowered or touched.";
    support_raft_max_primitives: NonZeroU64 = nonzero(200_000)?;
    "SUPPORT_RAFT_MAX_PRIMITIVES", "count", "scalar", "Inherited unmeasured cap on raft-plate emitted primitives (SupportPreparationLimits::production's raft_limits, RaftLimits::try_new max_primitives, crates/plinth-support/src/raft.rs). Codex-era 73e7c13; registered by FU-8g phase H3 (2026-08-09), which records the inherited number without ratifying it. The raft stage is unexercised on every corpus model and on the Weeping Treant v1.2 at this tip -- no measurement of record exists for its demand. No cap was raised, lowered or touched.";
    support_raft_max_roots: NonZeroU64 = nonzero(100_000)?;
    "SUPPORT_RAFT_MAX_ROOTS", "count", "scalar", "Inherited unmeasured cap on raft-plate root count (SupportPreparationLimits::production's raft_limits, RaftLimits::try_new max_roots, crates/plinth-support/src/raft.rs). Codex-era 73e7c13; registered by FU-8g phase H3 (2026-08-09), which records the inherited number without ratifying it. The raft stage is unexercised on every corpus model and on the Weeping Treant v1.2 at this tip -- no measurement of record exists for its demand. No cap was raised, lowered or touched.";
    support_route_cco_max_seat_evaluations: NonZeroU64 = nonzero(536_870_912)?;
    "SUPPORT_ROUTE_CCO_MAX_SEAT_EVALUATIONS", "count", "scalar", "Registered by FU-32 phase J5 (2026-08-09) as a NEW charge on a stage that had none. The sealed initial-terminal CCO routing cadence (route_initial_cco, crates/plinth-support/src/runner.rs) was ENTIRELY uncharged before this tip -- no Limit, no required/limit pair, no counter and no clock anywhere in cco.rs or routing's half of runner.rs (fu32-design-2026-08-09.md F8), which is the pre-FU-8b naked-hang shape: a 600 s wall inside a loop that priced nothing. This cap bounds the cadence's real Theta(N x P) work, the seat search: every grown terminal's discover_finalized_anchor_seats walks the WHOLE exterior anchor pool exactly once and classifies every row (crates/plinth-support/src/anchor.rs discover_anchor_seats_bound), so the honest worst-case demand is cadence_len x pool_rows. It is charged once before the first terminal and metered beside the charge against the counted per-terminal walk -- executed <= charged, typed refusal SupportRoutingError::ChargeDrift -- which is the G6c charge-beside-meter idiom, never a meter instead of a charge. The controller and immutable-authority digests are deliberately NOT charged: after FU-32 J1 and J2 they are O(1) and O(delta), and pricing them would price an implementation detail (design D3, the same reason G3 declined to charge the gate prefix). Measured provenance, single specimen (n = 1, per R4/FU-23): the Weeping Treant v1.2 measured at bfa5b3b has cadence 553 and pool_rows 364,223 (fu32-design-2026-08-09.md M1), so its charge is 553 x 364,223 = 201,415,319 seat evaluations. The number registered here is next_power_of_two(2 x 201,415,319) = 536,870,912 -- one stated rule, 2.67x headroom over the only specimen ever measured. MEASURED AT THIS TIP, 2026-08-09 (J5's own corpus run, PLINTH_CENSUS=1, PLINTH_THREADS=4, census table T0_route_cco_charge): the treant's charge reproduces at exactly 201,415,319 and its EXECUTED spend is 164,993,019 seat evaluations -- 81.9 % of the charge, the shortfall being the quarantined cadence entries a preflight cannot know about in advance -- which is 30.7 % of this cap. Every corpus model is orders of magnitude smaller: charged 33,320 (Sphere, the largest), 13,176 (Cylinder), 5,168 (make_toed_slab), 4,368 (make_overhang_table), 759 (make_raking_blade), 396 (make_thin_fin), 132 (make_box); the largest of the seven is 0.0062 % of this cap. No existing cap was raised, lowered or touched by this registration: the charge is an ADDITION to a previously uncharged stage, and design D3's sequencing is not negotiable -- it was added only AFTER J1 and J2 removed the redundant hashing, so it prices the loop that actually runs rather than enshrining the defect as a budget (support-resource-caps-2026-08-05.md, FU-8b's lesson that a cap is not the remedy).";
    support_route_cco_max_seat_refusal_rows: NonZeroU64 = nonzero(536_870_912)?;
    "SUPPORT_ROUTE_CCO_MAX_SEAT_REFUSAL_ROWS", "count", "scalar", "Registered by FU-32 phase J5 (2026-08-09) beside SUPPORT_ROUTE_CCO_MAX_SEAT_EVALUATIONS, on the same previously uncharged stage (fu32-design-2026-08-09.md F8) and by the same stated rule. This cap bounds the OTHER thing the routing cadence spends: the per-terminal AnchorSeatRefusal evidence rows the run RETAINS (crates/plinth-support/src/cco.rs, TerminalCcoOutcome::anchor_seat_refusals and the NoWinnerEvidence rows behind it). A sample can enter one terminal's refusal list at most once -- it either refuses at qualification or it qualifies, and a qualified sample refuses at most once more at the ray stage -- so retained rows per terminal are bounded by pool_rows and the run-wide worst case is the same cadence_len x pool_rows product the evaluation charge uses. Charged before the first terminal, metered beside the charge against the counted retained rows (executed <= charged, typed SupportRoutingError::ChargeDrift). Measured provenance, single specimen (n = 1, per R4/FU-23): the Weeping Treant v1.2 at bfa5b3b had retained 50,216,402 refusal rows by cadence entry 396 and grew them at approximately 126,800 per entry (fu32-design-2026-08-09.md M2), projecting to approximately 70.1 M over its full 553-entry cadence against a worst case of 201,415,319; the number registered here is next_power_of_two(2 x 201,415,319) = 536,870,912, derived from the worst case rather than the projection. MEASURED AT THIS TIP, 2026-08-09 (J5's own corpus run, census table T0_route_cco_charge): the treant completed its whole cadence for the first time and RETAINED 162,719,910 refusal rows -- 80.8 % of the charge and 30.3 % of this cap, and 2.3x the approximately 70.1 M the M2 growth law projected, which is recorded here as a correction to that projection rather than to the cap the worst case derived. The corpus models retain 33,320 (Sphere), 13,095 (Cylinder), 5,016 (make_toed_slab), 3,578 (make_overhang_table), 667 (make_raking_blade), 396 (make_thin_fin) and 125 (make_box). DISCLOSED, NOT RATIFIED: at the 30.3 B per retained refusal measured in M3, this cap admits approximately 16.3 GB of retained refusal evidence -- far above the 1,452 MiB the treant actually grew, and below, but not comfortably below, the 90 GB watchdog arm and the ~100 GB machine ceiling. Design open question 4 records the underlying question this registration does not answer (is evidence proportional to the anchor pool, or to the decision?) and deliberately leaves the refusal list itself untouched, because summarising it would move every transcript digest. No existing cap was raised, lowered or touched.";
    support_specialized_max_coverage_operations: NonZeroU64 = nonzero(2000000000)?;
    "SUPPORT_SPECIALIZED_MAX_COVERAGE_OPERATIONS", "count", "scalar", "Inherited unmeasured specialized cap on charged coverage-assessment operations. Codex-era 7ef66f6; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_specialized_max_coverage_scratch_bytes: NonZeroU64 = nonzero(536870912)?;
    "SUPPORT_SPECIALIZED_MAX_COVERAGE_SCRATCH_BYTES", "B", "scalar", "Inherited unmeasured specialized cap on coverage-assessment scratch bytes. Codex-era 7ef66f6; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_specialized_max_gate_contact_matrix_bytes: NonZeroU64 = nonzero(536870912)?;
    "SUPPORT_SPECIALIZED_MAX_GATE_CONTACT_MATRIX_BYTES", "B", "scalar", "Inherited unmeasured specialized cap on the dense gate contact-matrix allocation. Codex-era c2b3b98; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_specialized_max_gate_operations: NonZeroU64 = nonzero(2000000000)?;
    "SUPPORT_SPECIALIZED_MAX_GATE_OPERATIONS", "count", "scalar", "Inherited unmeasured specialized cap on charged gate operations, now the FLOOR of the measured batch-product rule max(floor, SUPPORT_GATE_MAX_WORK_PER_BATCH_UNIT x (coverage_samples + 1) x (contacts + 1)) and the whole cap for every corpus model. Measured 2026-08-05 to be refusing an algorithm defect, not a large model: the Weeping Treant tripped it at limit+1 through a selection sort in unique_island_face_count that charged 16,795,425,332 operations where a set with identical output charges 142,646. FU-8b fixes (a), (b) and (c) landed 2026-08-06 and the treant refusal became a stated demand, GateOperationLimit { required: 12714828544, limit: 2000000000 } (single specimen, n=1, per R4/FU-23), which the rule now admits. The number itself was NOT raised, and it still refuses the treant 39,223-proposal merge gate at 21,755,124,103,146 operations (FU-8c). Correction 2026-08-06 (FU-8c closed at a3308ec): the verdict in the sentence above STANDS -- the treant merge gate is still refused, at this same gate -- but its number and its follow-up pointer have both moved, and the sentence is left standing rather than rewritten. The per-proposal re-evaluation the number came from is gone (GateBatchIndex plus per-proposal route-key deltas) and gate_resource_plan is repriced, so the stated demand fell 108x, from 21,755,124,103,146 to 201,480,535,329, against the same materialized 35,445,500,736 -- still over, by 5.68x. No cap, floor or coefficient was changed by that fix. Read the standing refusal as 201,480,535,329 (FU-8d), not 21,755,124,103,146 (FU-8c): FU-8c is closed on the mechanism it named, and the residual 196,264,386,588 -- 97.4% of the demand -- is per-proposal sources x samples work on the perturbed route key, opened as FU-8d. AMENDED 2026-08-06 (FU-8d phase F2): every sentence above stands as written, and one more number has moved. F2 answers each merge proposal's nearest from the baseline sweep's retained witnesses plus ONE merged-seat kNN batch, so the per-proposal nearest term fell 94.2x, from 196,264,386,588 to 2,083,167,818 (nearest_work 1,041,544,686 + touched_rows 1,041,544,686 + key_visits 78,446), and gate_resource_plan's stated demand fell 27.6x, from 201,480,535,329 to 7,299,316,559 against the SAME materialized 35,445,500,736 -- 20.6% of it, so the plan check now PASSES. No cap, floor or coefficient was changed by that fix either. The treant still refuses at this same gate, one site later: in the exact_operations fold, where variant_specialized_owner_face_count's descriptor budget crosses this limit at variant 275 of 39,224 and prints required 35,445,618,202 against limit 35,445,500,736 (single specimen, n=1, per R4/FU-23). Read the standing refusal as 35,445,618,202 from the per-variant owner-face descriptor scan (FU-8d part (2), phase F4), not as 201,480,535,329 from the nearest term (FU-8d part (1), landed). AMENDED 2026-08-06 (FU-8d phases F3 and F4): every sentence above stands as written, and the standing refusal this note has carried since FU-8b is GONE -- there is no refusal at this cap any more. F3 finalizes the load partition once per batch rather than once per variant, deletes the per-variant window matrices, collapses the four per-proposal planning passes into one behind a count-derived pre-check, and prices the load phase from specialized_load_resource_plan itself instead of the gate's variants x faces model. The descriptor scan that crossed at variant 275 falls to 136,791,451 charged once; unique_face_finalizer_work (~4.33e14 extrapolated) ceases to exist as a term; and the honest load figure 6,592,813,742 is 1.43x the 4,618,272,984 model it replaced, so the stated demand ROSE to 9,273,857,317 -- 26.2% of the SAME materialized 35,445,500,736 -- with exact_operations 10,457,910,996 (29.5%). Both checks PASS and the batch EXECUTES: 39,224 variants evaluated, a merge accepted, the next batch opened. No cap, floor or coefficient was changed by F3 or F4. Do NOT read any refusal figure here as current; read 9,273,857,317 as a demand this cap ADMITS (single specimen, n=1, per R4/FU-23). The treant still has no seal, but what stops it is now wall-clock rather than this limit: F4's clean run at the F3 tip is an EXCEEDS-TIMEBOX at 600 s with peak RSS ~2.5 GiB and the 90 GB watchdog never firing, so ruling R-f is still NOT satisfied and SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED must still NOT be re-derived. The successor defect, sampled directly rather than inferred, is opened as FU-8e. AMENDED 2026-08-07 (FU-8e phases E1-E4): every sentence above stands as written, and this amendment records that NO number in it moved. FU-8e removed the merge gate's per-proposal kd-tree construction and its per-variant assess_coverage; plan_proposal_nearest_delta was deliberately left untouched, so work() is still 1 x touched_rows, the treant's touched_rows re-measured at 1,041,544,686 to the row, and the demand this cap ADMITS is still 9,273,857,317 against the same 35,445,500,736 (single specimen, n=1, per R4/FU-23). No cap, floor or coefficient was changed by E1, E2, E3 or E4. Two mechanism claims are corrected rather than rewritten: the one-point kd-trees were free at 83.3 ns each (about 3.27 ms for all 39,223 in a batch) and the cost was the query loop behind them, about 1.04e9 query iterations and 2.08e9 heap allocations per batch; and assess_coverage's cost was two unconditional full scans of the nearest array per variant, about 254 GB of reads per batch, not allocation churn. Measured 2026-08-07 under ruling R-g: merge round 63.83 s to 14.68 s (4.35x), coverage per variant 17.05 s to 0.001 s, zero coverage fallbacks across 33 rounds, peak RSS 2.446 GiB, watchdog armed at 90 GB and never fired, no typed refusal at any cap. The treant still has no seal and the stopping mode is still wall-clock: EXCEEDS-TIMEBOX at 601 s having completed 33 merge rounds of an estimated 460 (the tournament commits exactly one merge per round; at most 1,365 rounds by the contact bound), so the gate stage is roughly 1.8 to 5.5 hours and ruling R-f is still NOT satisfied — SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED must still NOT be re-derived. The successor is opened as FU-8f and is a RECORDED FINDING AWAITING OWNER RULING, deliberately not built. Arithmetic corrected 2026-08-07 (records fix): the kd-tree construction total in the amendment above was first written as 16.7 microseconds, an arithmetic slip in the design pass rather than a second measurement -- 39,223 x 83.3 ns = 3.267 ms, and 16.7 microseconds would have implied 0.426 ns per build. The per-build 83.3 ns is RETAINED as measured; only the product is corrected, and the conclusion is unchanged, since 3.27 ms is 0.009% of the 37.33 s fill_proposal_nearest_delta frame -- construction stays negligible and the fix still targets the query loop. No number this note prices moves. AMENDED 2026-08-07 (FU-8f phase G3): every sentence above stands as written, and this amendment records that NO number in it moved -- the demand this cap ADMITS is still 9,273,857,317 against the same materialized 35,445,500,736 (single specimen, n=1, per R4/FU-23), because gate_resource_plan still prices plan.variants = proposals + 1, the exact_operations fold still folds all 39,224, and the refusal check still runs BEFORE the variant loop. What G3 changes is EXECUTION, not charge, and two claims above have stopped being true as a result; they are corrected here rather than rewritten. First, 'Both checks PASS and the batch EXECUTES: 39,224 variants evaluated' now reads: the batch is CHARGED for 39,224 variants and EXECUTES a PREFIX of them. The merge tournament stops at the first variant that is gate_not_worse than the batch baseline, which was proposal 0 in all 28 rounds ever observed across two models, so one proposal variant is evaluated per round rather than 39,223. The executed count is metered as GateBatchOutcome::executed_variant_count and reported as SpecializedGateEvidence::evaluated_variant_count, beside gathered_pair_count -- renamed from evaluated_pair_count, which had always counted proposals GATHERED rather than evaluated. Second, 'The successor is opened as FU-8f and is a RECORDED FINDING AWAITING OWNER RULING, deliberately not built' is past tense: the ruling came (Craig, 2026-08-07) and phase G3 built part (1). GateRoutePlanMismatch is re-founded a SECOND time, prefix-wise -- it attests that the batch executed a prefix of the work it planned, the first executed_variant_count variants, in plan order, each exactly once, at the peak the plan stated. Charged and executed are severed and BOTH are stated; charging the prefix instead was considered and refused, because a bound not knowable until after the work that determines it bounds nothing. No cap, floor, coefficient or rule was changed by G3, and all seven frozen OD-8 digests are byte-identical at its tip. Ruling R-f is STILL NOT satisfied and SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED must STILL NOT be re-derived: a treant run at the G3 tip, on a QUIET machine with no concurrent workflow, is an EXCEEDS-TIMEBOX at 600 s in both permitted attempts (peak RSS 3,479,600 KB, watchdog armed at 90 GB and never fired), so no run has yet COMPLETED the gate. See docs/engineering-journal/support-resource-caps-2026-08-05.md section 13. ADDENDUM 2026-08-07 (FU-8f phase G5): every sentence above stands, THIS CAP IS UNCHANGED at 35,445,500,736 for the treant's batch shape, and the demand it is handed has MOVED -- downward, by 4,748,065,084. G5 repriced load_resource_plan_from_counts's scan term from rows x contacts x (owner_log + 4) -- the model of the dense contacts x rows scan that phase G1 had ALREADY replaced with a CSR transpose, and which G1 deliberately left standing and over-stating -- to what the transpose really spends: Sigma |eligible_owners| x (log(contacts) + 2) x 2 for its two eligibility traversals, charged from the counts, plus cells x (LOAD_WITNESS_SLOTS + 5) for the fill and the scan, charged by load_resource_plan from the exact cell count. Measured on the Weeping Treant v1.2's round-1 merge-gate batch (39,224 variants, 1,366 contacts, 405,146 samples, 1,095 demands), 2026-08-07, on a QUIET machine with no concurrent workflow, one instrumented release run whose instrumentation was reverted by editing before the gates: the load baseline falls 5,543,831,115 -> 795,766,031, so the gate's load term falls 6,592,813,742 -> 1,844,748,658, gate_resource_plan's stated demand falls 9,273,857,317 -> 4,525,792,233 (12.8 % of this cap where it was 26.2 %) and exact_operations falls 10,457,910,996 -> 5,709,845,912 (16.1 % where it was 29.5 %). The fall is 4,748,065,084 at BOTH sites, to the unit, and it is ENTIRELY the load term: baseline_rows 554,926,572, demand_rows 42,949,185 and proposal work 2,083,167,818 are byte-identical. A falling demand can create no refusal, and no cap, floor or coefficient was raised, lowered or touched by G5. All seven frozen OD-8 digests are byte-identical at this tip -- repricing changes charges, never geometry. Phase G4's transcript was deliberately recorded in the OLD numbers and is left that way. Ruling R-f is STILL NOT satisfied and SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED is STILL NOT re-derived: G5 ran no completion attempt at all, and its one measurement run was stopped at 260 s having reached the gate, which is not a completed tournament. See docs/engineering-journal/support-resource-caps-2026-08-05.md section 15. Codex-era c2b3b98; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_specialized_max_gate_peak_bytes: NonZeroU64 = nonzero(1073741824)?;
    "SUPPORT_SPECIALIZED_MAX_GATE_PEAK_BYTES", "B", "scalar", "Inherited unmeasured specialized cap on peak gate-stage bytes. Measured 2026-08-05 to be correctly refusing an 8.0 M-row load-eligibility explosion on a 429,120-face mesh; tracked as FU-8b algorithm work and must NOT be raised. Codex-era 7ef66f6; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it. ADDENDUM 2026-08-08 (FU-8f phase G6c): every sentence above stands, this cap is UNCHANGED at 1,073,741,824, and one NEW term now joins gate_load_state_bytes -- the charge this cap governs -- whenever the merge gate's cross-round load-transpose retention (phases G6a/G6b) is live. RetainedLoadTranspose::retained_bytes charges the carrier's own held bytes: its append-only arena at record_commit's own 2x-compaction CEILING (2 x cells x 4 B, never the arena's live length, because compaction runs only once dead payload exceeds live cells), plus the real (checked) sizes of its per-contact bucket refs, its (owner, point_mm) contact witness and its merge-invariant row invariants (face_id/area/centroid). This is NOT the same term as the 32,848,040 B transpose-scratch figure the ADDENDUM on SUPPORT_SPECIALIZED_MAX_LOAD_SCRATCH_BYTES describes: that term is charged inside load_resource_plan, reached only from SpecializedLoadBaseline::build, and a retained round never reaches it -- so on a retained round THAT term does not run at all, while THIS term (the retained carrier's own bytes) is charged here, at THIS cap, precisely because retention extends that state's lifetime across rounds instead of rebuilding it every round. No cap, floor or coefficient is raised, lowered or touched by this addition; it states a new charge against an unchanged limit. See docs/engineering-journal/support-resource-caps-2026-08-05.md section 16.";
    support_specialized_max_gate_variants: NonZeroU64 = nonzero(20000001)?;
    "SUPPORT_SPECIALIZED_MAX_GATE_VARIANTS", "count", "scalar", "Inherited unmeasured specialized cap on enumerated gate variants; the lone off-round value in the family, which is itself unexplained by any record. Codex-era c2b3b98; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_specialized_max_gate_vector_bytes: NonZeroU64 = nonzero(536870912)?;
    "SUPPORT_SPECIALIZED_MAX_GATE_VECTOR_BYTES", "B", "scalar", "Inherited unmeasured specialized cap on gate vector bytes. Codex-era c2b3b98; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_specialized_max_gate_window_variants: NonZeroU64 = nonzero(1024)?;
    "SUPPORT_SPECIALIZED_MAX_GATE_WINDOW_VARIANTS", "count", "scalar", "Inherited unmeasured specialized cap on gate window variants. Codex-era c2b3b98; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_specialized_max_load_operations: NonZeroU64 = nonzero(2000000000)?;
    "SUPPORT_SPECIALIZED_MAX_LOAD_OPERATIONS", "count", "scalar", "Inherited unmeasured specialized cap on charged load-partition operations, now the FLOOR of the measured batch-product rule max(floor, SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT x (rows + 1) x (contacts + 1)) and the whole cap for every corpus model. Measured 2026-08-05 to be correctly refusing a real defect: the treant's load plan PREDICTED 7,988,078 load-eligibility rows on a 429,120-face mesh, and a dense contact x row scan priced over that prediction is ~2.95e11 charged operations. FU-8b fix (B) landed 2026-08-06 and corrected the prediction to the 130,548 rows the builder had always produced, leaving an honest 5,402,535,619 which the rule now admits (single specimen, n=1, per R4/FU-23). The number itself was NOT raised. Correction 2026-08-06 (record fixes F1 and F3): this note previously said the treant BUILT 7,988,078 rows (18.6 per face) and that fix (B) collapsed them to 134,368 distinct faces. Neither is so. build_load_eligibility was already a pure per-face BTreeMap collapse at eb36358 and emitted one row per distinct face then as now, so no row set was ever built at that size and none collapsed; 7,988,078 was specialized_load_resource_plan's predicted row_count with the (owner, face) pair count standing in for the face count (16,627 generic rows + 7,971,451 pairs). The corrected post-fix row count is 130,548 (16,627 + the 113,921 distinct faces of those pairs), which is what the plan states and what the census scaled-cap row prints; 134,368 belongs to no batch this build plans. What the cap refused was an inflated PLAN, which is what a resource plan exists to catch, so the cap's verdict stands unchanged. ADDENDUM 2026-08-07 (FU-8f phase G5): every sentence above stands, this floor is UNCHANGED at 2,000,000,000, and the honest 5,402,535,619 the paragraph above quotes has been repriced DOWNWARD to 795,359,775 on the same 130,548-row batch. G5 replaced load_resource_plan_from_counts's rows x contacts x (owner_log + 4) scan term -- a model of the dense contacts x rows scan that phase G1 had already replaced with a CSR transpose -- with the transpose's own terms: Sigma |eligible_owners| x (log(contacts) + 2) x 2 for its two eligibility traversals, plus cells x (LOAD_WITNESS_SLOTS + 5) for the fill and the scan, the latter charged by load_resource_plan from the exact cell count that no preflight can state. Measured 2026-08-07 on a QUIET machine, one instrumented release run, instrumentation reverted by editing before the gates (single specimen, n=1, per R4/FU-23). THE FINDING, stated plainly because it changes what this cap is for: 795,359,775 is BELOW this 2,000,000,000 floor, so after G5 the Weeping Treant -- the only model that ever left the floor at this cap -- no longer does. The batch-product scaling above it is no longer load-bearing for any model in the corpus. It is KEPT anyway and nothing here is touched: removing the scaling, or re-deriving SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT from the repriced observation, would LOWER a cap, and a cap reduction is the direction that creates refusals. See the dated addendum on SUPPORT_LOAD_WORK_PER_BATCH_UNIT_OBSERVED for why the coefficient's derivation input stays the 2026-08-06 figure, and docs/engineering-journal/support-resource-caps-2026-08-05.md section 15. Codex-era 7ef66f6; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_specialized_max_load_scratch_bytes: NonZeroU64 = nonzero(536870912)?;
    "SUPPORT_SPECIALIZED_MAX_LOAD_SCRATCH_BYTES", "B", "scalar", "Inherited unmeasured specialized cap on load-partition scratch bytes. Measured 2026-08-05 to be correctly refusing the same load-eligibility explosion; tracked as FU-8b algorithm work and must NOT be raised. Codex-era 7ef66f6; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it. ADDENDUM 2026-08-07 (FU-8f phase G1): every sentence above stands, this cap is UNCHANGED at 536,870,912, and what has changed is that one NEW term is charged against it. finalize_load_rows no longer visits every (contact, row) cell and skips the ineligible ones; it builds a contact-major CSR transpose of the eligibility and runs the same loop body over the eligible cells only, in the same order. On the Weeping Treant v1.2's merge-gate batch measured at 7aa6b73 that is 8,210,643 eligible cells of 160,834,206 visited -- 94.9% of the visits were the skip. The transpose is charged, as it must be, at cells x 4 B + (contacts + 1) x 4 B = 8,210,643 x 4 + 1,367 x 4 = 32,848,040 B, which is 6.1% of this cap and 3.1% of the 1 GiB gate peak. That is an ADDITION to LoadResourcePlan::scratch_bytes and nothing else moved: no cap raised, lowered or touched, and no charged operation count moved either -- load_resource_plan_from_counts deliberately still prices the scan as rows x contacts x (owner_log + 4), over-stating the executed work by roughly 4.34e9 against ~1e8, on the FU-8e Q5 precedent that a plan admitting the general path a fortiori admits its specialisation. Repricing that term is phase G5 by design and is NOT to be done as a side effect of anything else. Stated so it is not discovered later: the term is added in load_resource_plan, the finalizer's own exact plan, and NOT in specialized_load_resource_plan, the preflight, because the term is the exact eligible-cell count and no preflight can state it ahead of the eligibility without over-stating by two or three orders of magnitude; the preflight therefore under-states this finalizer's scratch by the transpose term, which costs a later refusal, never an unafforded allocation. Bit-identity of the transposed scan against the scan it replaces -- including the (distance, contact_id) tie-break -- was measured in-process on the treant's real batch (nearest_bit_identical = true, buckets_already_sorted = true) and is pinned by fu8f_transposed_finalize_reproduces_the_scan_bit_for_bit against the retained #[cfg(test)] contacts x rows reference. Single specimen for the 32,848,040: the Weeping Treant v1.2 alone, 2026-08-07 (n = 1, per R4/FU-23). ADDENDUM 2026-08-07 (FU-8f phase G5): every sentence above stands, THIS CAP IS STILL UNCHANGED at 536,870,912, the 32,848,040 B transpose term is unchanged to the byte, and the two sentences above that promised a later repricing have come due. G5 retired rows x contacts x (owner_log + 4) from load_resource_plan_from_counts and replaced it with the transpose's own terms -- Sigma |eligible_owners| x (log(contacts) + 2) x 2 for the count and fill traversals, plus cells x (LOAD_WITNESS_SLOTS + 5) for the fill's stores and the scan, the latter added by load_resource_plan from the exact cell count for exactly the reason the BYTE term is added there and not in the preflight. So the sentence 'no charged operation count moved either' is now history: on the treant's gate batch that term goes 4,342,523,562 -> 207,690,028 + 65,685,144, and the finalizer's whole exact plan goes 4,714,113,843 -> 644,970,917. The disposition the paragraph above describes for BYTES now also holds for OPERATIONS, and is stated once here so it is not discovered later: specialized_load_resource_plan, the preflight, under-states this finalizer's operations by the cells term, because cells is knowable only after LoadTransposeCounts::count has walked the eligibility. That costs a LATER refusal, never unafforded execution -- the fill and the scan run only after finalize_load_rows's own exact check, and that check is the one carrying the cells term. No cap raised, lowered or touched; all seven frozen OD-8 digests byte-identical. ADDENDUM 2026-08-08 (FU-8f phase G6c): every sentence above stands, this cap is STILL UNCHANGED at 536,870,912, and the 32,848,040 B transpose term above describes a charge that no longer runs on EVERY round. Phase G6b (2026-08-07) added a retained fast path, RetainedLoadTranspose::finalize_retained_round, that a gate round takes instead of SpecializedLoadBaseline::build whenever its four guards all pass; that function -- and therefore load_resource_plan, and therefore this 32,848,040 B term -- is called only on the tournament's first round or a counted guard-miss rebuild, never on a retained round. This is stated here, at G6c, rather than left implicit: it does not change what the term above prices when it DOES run (nothing here moved, to the byte), it changes how OFTEN it runs. The retained carrier's OWN held bytes are a separate, NEW term, charged at a DIFFERENT cap (SUPPORT_SPECIALIZED_MAX_GATE_PEAK_BYTES, via gate_load_state_bytes) precisely because retention extends that state's lifetime across rounds instead of rebuilding it every round -- see that constant's own ADDENDUM. No cap, floor or coefficient raised, lowered or touched. See docs/engineering-journal/support-resource-caps-2026-08-05.md section 16.";
    support_specialized_max_merge_commit_bytes: NonZeroU64 = nonzero(536870912)?;
    "SUPPORT_SPECIALIZED_MAX_MERGE_COMMIT_BYTES", "B", "scalar", "Inherited unmeasured specialized cap on merge-commit bytes. Codex-era abea502; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_specialized_max_merge_descriptor_operations: NonZeroU64 = nonzero(2000000000)?;
    "SUPPORT_SPECIALIZED_MAX_MERGE_DESCRIPTOR_OPERATIONS", "count", "scalar", "Inherited unmeasured specialized cap on charged merge-descriptor operations. Codex-era 7ef66f6; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_specialized_max_merge_iterations: NonZeroU64 = nonzero(100000)?;
    "SUPPORT_SPECIALIZED_MAX_MERGE_ITERATIONS", "count", "scalar", "Inherited unmeasured specialized cap on merge fixed-point iterations. Codex-era abea502; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_specialized_max_merge_pairs: NonZeroU64 = nonzero(20000000)?;
    "SUPPORT_SPECIALIZED_MAX_MERGE_PAIRS", "count", "scalar", "Inherited unmeasured specialized cap on merge candidate pairs. Codex-era 6a66245; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_specialized_max_merge_proposals: NonZeroU64 = nonzero(20000000)?;
    "SUPPORT_SPECIALIZED_MAX_MERGE_PROPOSALS", "count", "scalar", "Inherited unmeasured specialized cap on merge proposals in one iteration. Codex-era c2b3b98; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_specialized_max_merge_proposal_bytes: NonZeroU64 = nonzero(536870912)?;
    "SUPPORT_SPECIALIZED_MAX_MERGE_PROPOSAL_BYTES", "B", "scalar", "Inherited unmeasured specialized cap on merge-proposal bytes. Codex-era 7ef66f6; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_specialized_max_merge_total_proposals: NonZeroU64 = nonzero(100000000)?;
    "SUPPORT_SPECIALIZED_MAX_MERGE_TOTAL_PROPOSALS", "count", "scalar", "Inherited unmeasured specialized cap on merge proposals across all iterations. Codex-era abea502; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_specialized_max_satisfaction_neighbor_bytes: NonZeroU64 = nonzero(2147483648)?;
    "SUPPORT_SPECIALIZED_MAX_SATISFACTION_NEIGHBOR_BYTES", "B", "scalar", "Inherited unmeasured specialized cap on satisfaction-neighbor storage bytes. Codex-era 6a66245; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_specialized_max_satisfaction_neighbor_pairs: NonZeroU64 = nonzero(100000000)?;
    "SUPPORT_SPECIALIZED_MAX_SATISFACTION_NEIGHBOR_PAIRS", "count", "scalar", "Inherited unmeasured specialized cap on satisfaction-neighbor pairs. Codex-era 6a66245; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_specialized_max_satisfaction_tests: NonZeroU64 = nonzero(100000000)?;
    "SUPPORT_SPECIALIZED_MAX_SATISFACTION_TESTS", "count", "scalar", "Inherited unmeasured specialized cap on demand-satisfaction tests. Codex-era 6a66245; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_specialized_max_synthetic_neighbor_bytes: NonZeroU64 = nonzero(536870912)?;
    "SUPPORT_SPECIALIZED_MAX_SYNTHETIC_NEIGHBOR_BYTES", "B", "scalar", "Inherited unmeasured specialized cap on synthetic-neighbor storage bytes. Codex-era 6a66245; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    support_specialized_max_synthetic_neighbor_pairs: NonZeroU64 = nonzero(20000000)?;
    "SUPPORT_SPECIALIZED_MAX_SYNTHETIC_NEIGHBOR_PAIRS", "count", "scalar", "Inherited unmeasured specialized cap on synthetic-neighbor pairs. Codex-era 6a66245; registered by owner ruling R5 (2026-08-05), which records the inherited number without ratifying it.";
    surface_tree_max_points: NonZeroU64 = nonzero(150_000)?;
    "SURFACE_TREE_MAX_POINTS", "count", "scalar", "Result-changing sampled-surface cap, not a resource exclusion.";
    surface_tree_pitch_mm: PositiveFinite = PositiveFinite::new(0.4)?;
    "SURFACE_TREE_PITCH_MM", "mm", "scalar", "Result-changing sampled-surface pitch.";
    surface_tree_seed: u64 = 0;
    "SURFACE_TREE_SEED", "count", "scalar", "Deterministic sampled-surface seed; zero is a real seed, not a sentinel.";
    target_spacing_mm: PositiveFinite = PositiveFinite::new(4.0)?;
    "TARGET_SPACING_MM", "mm", "scalar", "Observed sufficiency does not derive an optimum spacing boundary.";
    tensile_green_mpa: PositiveFinite = PositiveFinite::new(0.288)?;
    "TENSILE_GREEN_MPA", "MPa", "scalar", "Algebraic legacy diagnostic split, not a measured material property.";
    thin_feature_mm: PositiveFinite = PositiveFinite::new(1.5)?;
    "THIN_FEATURE_MM", "mm", "scalar", "Delicacy thinness saturation threshold.";
    tip_radius_default_mm: PositiveFinite = PositiveFinite::new(0.200)?;
    "TIP_RADIUS_DEFAULT_MM", "mm", "scalar", "Generation starting radius, distinct from the measured validator minimum.";
    tip_radius_max_legacy_mm: PositiveFinite = PositiveFinite::new(1.0)?;
    "TIP_RADIUS_MAX_LEGACY_MM", "mm", "scalar", "Live output-affecting placement clamp retained as legacy policy.";
    tip_safety_factor: PositiveFinite = PositiveFinite::new(8.0)?;
    "TIP_SAFETY_FACTOR", "1", "scalar", "Legacy diagnostic load multiplier selected as policy.";
    tol_mm: PositiveFinite = PositiveFinite::new(0.05)?;
    "TOL_MM", "mm", "scalar", "Interval anchoring and detection tolerance, distinct from the sliver cutoff.";
    transient_shoot_count_exponent: UnitFraction = UnitFraction::new(0.125)?;
    "TRANSIENT_SHOOT_COUNT_EXPONENT", "1", "scalar", "Inherited N-to-the-one-eighth heuristic without physical authority.";
    trunk_search_directions: NonZeroU64 = nonzero(12)?;
    "TRUNK_SEARCH_DIRECTIONS", "count", "scalar", "Trunk search direction count.";
    trunk_search_radii_mm: Vec<NonNegativeFinite> = nonnegative_values(&[0.0, 2.0, 4.0, 7.0, 10.0, 14.0, 19.0, 25.0])?;
    "TRUNK_SEARCH_RADII_MM", "mm", "list[8]", "Ordered trunk-search radius ladder including the real zero-radius candidate.";
    twin_single_brace_xy_mm: PositiveFinite = PositiveFinite::new(3.0)?;
    "TWIN_SINGLE_BRACE_XY_MM", "mm", "scalar", "Twin-to-single brace XY threshold.";
    voxel_pitch_mm: PositiveFinite = PositiveFinite::new(0.4)?;
    "VOXEL_PITCH_MM", "mm", "scalar", "Legacy perception working scale inferred from one corpus run.";
}
