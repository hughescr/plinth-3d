use crate::{ConstantId, DecisionId, EffectiveArmCount, Finite, LoadSharing, MetricValue, PlateId};
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::{collections::BTreeMap, fmt, marker::PhantomData};
use thiserror::Error;

/// A compact provenance description suitable for a run manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProvenanceRef {
    Measured {
        plate: PlateId,
        date: String,
        note: String,
    },
    Derived {
        from: Vec<String>,
        note: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ProvenanceError {
    #[error("measured numeric value must be finite")]
    NonFiniteMeasuredValue,
    #[error("measured numeric error must be finite")]
    NonFiniteMeasuredError,
    #[error("categorical measured payloads require err to be null")]
    CategoricalErrorBar,
    #[error("conflicting provenance for decision {decision} and constant {constant}")]
    ConflictingRecord {
        decision: DecisionId,
        constant: ConstantId,
    },
    #[error("duplicate provenance decision key {decision}")]
    DuplicateDecisionKey { decision: DecisionId },
    #[error("duplicate provenance constant key {constant}")]
    DuplicateConstantKey { constant: ConstantId },
}

/// The constants that fed each acceptance decision.
///
/// Both map levels are ordered so serialization is deterministic.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ProvenanceManifest(BTreeMap<DecisionId, BTreeMap<ConstantId, ProvenanceRef>>);

impl ProvenanceManifest {
    pub fn new<I, B>(entries: I) -> Result<Self, ProvenanceError>
    where
        I: IntoIterator<Item = (DecisionId, B)>,
        B: IntoIterator<Item = (ConstantId, ProvenanceRef)>,
    {
        let mut manifest = Self::default();
        for (decision, constants) in entries {
            manifest.0.entry(decision.clone()).or_default();
            for (constant, provenance) in constants {
                manifest.record(decision.clone(), constant, provenance)?;
            }
        }
        Ok(manifest)
    }

    /// Records one constant use. Repeating the identical record is idempotent;
    /// changing provenance for the same decision/constant pair is an error.
    pub fn record(
        &mut self,
        decision: DecisionId,
        constant: ConstantId,
        provenance: ProvenanceRef,
    ) -> Result<(), ProvenanceError> {
        let constants = self.0.entry(decision.clone()).or_default();
        match constants.get(&constant) {
            Some(existing) if existing == &provenance => Ok(()),
            Some(_) => Err(ProvenanceError::ConflictingRecord { decision, constant }),
            None => {
                constants.insert(constant, provenance);
                Ok(())
            }
        }
    }

    #[must_use]
    pub fn get(&self, decision: &DecisionId, constant: &ConstantId) -> Option<&ProvenanceRef> {
        self.0.get(decision)?.get(constant)
    }

    #[must_use]
    pub fn iter(
        &self,
    ) -> impl ExactSizeIterator<Item = (&DecisionId, &BTreeMap<ConstantId, ProvenanceRef>)> {
        self.0.iter()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

struct ConstantBatch(BTreeMap<ConstantId, ProvenanceRef>);

impl<'de> Deserialize<'de> for ConstantBatch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BatchVisitor;

        impl<'de> Visitor<'de> for BatchVisitor {
            type Value = ConstantBatch;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a uniquely keyed provenance constant map")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut constants = BTreeMap::new();
                while let Some(constant) = map.next_key::<ConstantId>()? {
                    let provenance = map.next_value::<ProvenanceRef>()?;
                    if constants.contains_key(&constant) {
                        return Err(de::Error::custom(ProvenanceError::DuplicateConstantKey {
                            constant,
                        }));
                    }
                    constants.insert(constant, provenance);
                }
                Ok(ConstantBatch(constants))
            }
        }

        deserializer.deserialize_map(BatchVisitor)
    }
}

impl<'de> Deserialize<'de> for ProvenanceManifest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ManifestVisitor;

        impl<'de> Visitor<'de> for ManifestVisitor {
            type Value = ProvenanceManifest;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a uniquely keyed provenance decision map")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut decisions = BTreeMap::new();
                while let Some(decision) = map.next_key::<DecisionId>()? {
                    let ConstantBatch(constants) = map.next_value::<ConstantBatch>()?;
                    if decisions.contains_key(&decision) {
                        return Err(de::Error::custom(ProvenanceError::DuplicateDecisionKey {
                            decision,
                        }));
                    }
                    decisions.insert(decision, constants);
                }
                Ok(ProvenanceManifest(decisions))
            }
        }

        deserializer.deserialize_map(ManifestVisitor)
    }
}

/// Values that are admissible on an acceptance path.
pub trait AcceptSafe<T: Copy> {
    fn get(&self) -> T;
    fn provenance(&self) -> ProvenanceRef;
}

mod sealed {
    pub trait Sealed {}
}

/// Payload policy for measured constants.
///
/// This trait is sealed: core explicitly decides which payloads have a sound
/// error-bar interpretation.
pub trait MeasuredPayload: sealed::Sealed {
    fn validate_measurement(value: &Self, err: Option<&Self>) -> Result<(), ProvenanceError>;
}

impl sealed::Sealed for f64 {}

impl MeasuredPayload for f64 {
    fn validate_measurement(value: &Self, err: Option<&Self>) -> Result<(), ProvenanceError> {
        if !value.is_finite() {
            return Err(ProvenanceError::NonFiniteMeasuredValue);
        }
        if err.is_some_and(|error| !error.is_finite()) {
            return Err(ProvenanceError::NonFiniteMeasuredError);
        }
        Ok(())
    }
}

macro_rules! finite_payload {
    ($($payload:ty),+ $(,)?) => {
        $(
            impl sealed::Sealed for $payload {}

            impl MeasuredPayload for $payload {
                fn validate_measurement(
                    _value: &Self,
                    _err: Option<&Self>,
                ) -> Result<(), ProvenanceError> {
                    Ok(())
                }
            }
        )+
    };
}

finite_payload!(Finite, EffectiveArmCount, MetricValue);

impl sealed::Sealed for LoadSharing {}

impl MeasuredPayload for LoadSharing {
    fn validate_measurement(_value: &Self, err: Option<&Self>) -> Result<(), ProvenanceError> {
        if err.is_some() {
            Err(ProvenanceError::CategoricalErrorBar)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Measured<T> {
    value: T,
    plate: PlateId,
    /// `None` is intentionally emitted as JSON `null`, never omitted.
    err: Option<T>,
    date: String,
    note: String,
}

impl<T: MeasuredPayload> Measured<T> {
    pub fn new(
        value: T,
        plate: PlateId,
        err: Option<T>,
        date: impl Into<String>,
        note: impl Into<String>,
    ) -> Result<Self, ProvenanceError> {
        T::validate_measurement(&value, err.as_ref())?;
        Ok(Self {
            value,
            plate,
            err,
            date: date.into(),
            note: note.into(),
        })
    }

    #[must_use]
    pub fn value(&self) -> &T {
        &self.value
    }

    #[must_use]
    pub const fn plate(&self) -> &PlateId {
        &self.plate
    }

    #[must_use]
    pub const fn err(&self) -> Option<&T> {
        self.err.as_ref()
    }

    #[must_use]
    pub fn date(&self) -> &str {
        &self.date
    }

    #[must_use]
    pub fn note(&self) -> &str {
        &self.note
    }
}

struct RequiredOption<T>(Option<T>);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for RequiredOption<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<T>::deserialize(deserializer).map(Self)
    }
}

impl<'de, T> Deserialize<'de> for Measured<T>
where
    T: Deserialize<'de> + MeasuredPayload,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MeasuredVisitor<T>(PhantomData<T>);

        impl<'de, T> Visitor<'de> for MeasuredVisitor<T>
        where
            T: Deserialize<'de> + MeasuredPayload,
        {
            type Value = Measured<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a complete measured payload")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                #[derive(Deserialize)]
                #[serde(field_identifier, rename_all = "snake_case")]
                enum Field {
                    Value,
                    Plate,
                    Err,
                    Date,
                    Note,
                }

                let mut value: Option<T> = None;
                let mut plate: Option<PlateId> = None;
                let mut err: Option<Option<T>> = None;
                let mut date: Option<String> = None;
                let mut note: Option<String> = None;
                while let Some(field) = map.next_key::<Field>()? {
                    match field {
                        Field::Value => {
                            if value.is_some() {
                                return Err(de::Error::duplicate_field("value"));
                            }
                            value = Some(map.next_value()?);
                        }
                        Field::Plate => {
                            if plate.is_some() {
                                return Err(de::Error::duplicate_field("plate"));
                            }
                            plate = Some(map.next_value()?);
                        }
                        Field::Err => {
                            if err.is_some() {
                                return Err(de::Error::duplicate_field("err"));
                            }
                            err = Some(map.next_value::<RequiredOption<T>>()?.0);
                        }
                        Field::Date => {
                            if date.is_some() {
                                return Err(de::Error::duplicate_field("date"));
                            }
                            date = Some(map.next_value()?);
                        }
                        Field::Note => {
                            if note.is_some() {
                                return Err(de::Error::duplicate_field("note"));
                            }
                            note = Some(map.next_value()?);
                        }
                    }
                }

                Measured::new(
                    value.ok_or_else(|| de::Error::missing_field("value"))?,
                    plate.ok_or_else(|| de::Error::missing_field("plate"))?,
                    err.ok_or_else(|| de::Error::missing_field("err"))?,
                    date.ok_or_else(|| de::Error::missing_field("date"))?,
                    note.ok_or_else(|| de::Error::missing_field("note"))?,
                )
                .map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_struct(
            "Measured",
            &["value", "plate", "err", "date", "note"],
            MeasuredVisitor(PhantomData),
        )
    }
}

impl<T: Copy + MeasuredPayload> AcceptSafe<T> for Measured<T> {
    fn get(&self) -> T {
        self.value
    }

    fn provenance(&self) -> ProvenanceRef {
        ProvenanceRef::Measured {
            plate: self.plate.clone(),
            date: self.date.clone(),
            note: self.note.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Derived<T> {
    value: T,
    pub from: Vec<String>,
    pub note: String,
}

impl<T> Derived<T> {
    #[must_use]
    pub fn new(
        value: T,
        from: impl IntoIterator<Item = impl Into<String>>,
        note: impl Into<String>,
    ) -> Self {
        Self {
            value,
            from: from.into_iter().map(Into::into).collect(),
            note: note.into(),
        }
    }

    #[must_use]
    pub fn value(&self) -> &T {
        &self.value
    }
}

impl<T: Copy> AcceptSafe<T> for Derived<T> {
    fn get(&self) -> T {
        self.value
    }

    fn provenance(&self) -> ProvenanceRef {
        ProvenanceRef::Derived {
            from: self.from.clone(),
            note: self.note.clone(),
        }
    }
}

/// A value useful for diagnostics or search, but forbidden on acceptance paths.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placeholder<T> {
    value: T,
    pub note: String,
}

impl<T> Placeholder<T> {
    #[must_use]
    pub fn new(value: T, note: impl Into<String>) -> Self {
        Self {
            value,
            note: note.into(),
        }
    }

    #[must_use]
    pub fn diagnostic(&self) -> &T {
        &self.value
    }
}

/// An explicitly unresolved calibration range.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pending<T> {
    pub conservative: T,
    pub optimistic: T,
    pub resolves_on: PlateId,
    pub question: String,
}

impl<T: Copy> Pending<T> {
    #[must_use]
    pub fn new(
        conservative: T,
        optimistic: T,
        resolves_on: PlateId,
        question: impl Into<String>,
    ) -> Self {
        Self {
            conservative,
            optimistic,
            resolves_on,
            question: question.into(),
        }
    }

    /// Resolving uses the conservative branch and returns an acceptance-safe value.
    #[must_use]
    pub fn resolve(&self) -> Derived<T> {
        Derived::new(
            self.conservative,
            [format!("pending plate {}", self.resolves_on)],
            format!("resolved conservatively: {}", self.question),
        )
    }

    /// This is for calibration comparison only; it is still explicitly derived.
    #[must_use]
    pub fn optimistic(&self) -> Derived<T> {
        Derived::new(
            self.optimistic,
            [format!("pending plate {}", self.resolves_on)],
            format!("optimistic calibration branch: {}", self.question),
        )
    }
}
