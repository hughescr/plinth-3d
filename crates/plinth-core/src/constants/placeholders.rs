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

            pub fn descriptors(&self) -> Result<[ConstantDescriptor; 187], RegistryError> {
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
    decimation_ratio_search: DecimationRatioSearchPolicy = DecimationRatioSearchPolicy::new(0.05, 0.50, 5)?;
    "DECIMATION_RATIO_SEARCH", "1", "record{bisections,lower,upper}", "Structured bounded ratio-search policy.";
    decimation_recheck_slack: PositiveFinite = PositiveFinite::new(1.1)?;
    "DECIMATION_RECHECK_SLACK", "1", "scalar", "Selected full-population recheck slack, not measured authority.";
    decimation_scale_acceptance_mm: FiniteInterval = FiniteInterval::new(10.0, 300.0, crate::BoundKind::Inclusive, crate::BoundKind::Inclusive)?;
    "DECIMATION_SCALE_ACCEPTANCE_MM", "mm", "interval", "Inclusive scale-sanity acceptance interval.";
    decimation_seed_error_mm: PositiveFinite = PositiveFinite::new(0.050)?;
    "DECIMATION_SEED_ERROR_MM", "mm", "scalar", "Feature seed error threshold.";
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
    hollow_vent_r_mm: PositiveFinite = PositiveFinite::new(1.25)?;
    "HOLLOW_VENT_R_MM", "mm", "scalar", "Vent-hole radius policy.";
    hollow_volume_balance_rel_tol: UnitFraction = UnitFraction::new(0.02)?;
    "HOLLOW_VOLUME_BALANCE_REL_TOL", "1", "scalar", "Final volume-balance tolerance relative to solid volume.";
    hollow_wall_default_mm: PositiveFinite = PositiveFinite::new(2.5)?;
    "HOLLOW_WALL_DEFAULT_MM", "mm", "scalar", "Hollow generation and tool wall default.";
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
