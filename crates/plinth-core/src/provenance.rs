use crate::{
    BuildVolumeMm, ConstantId, DecisionId, EffectiveArmCount, Finite, LoadSharing, MetricValue,
    MicronGranularity, PlateId, RootBossProfile, TipContactProfiles, YDirection,
};
use serde::{
    de::{self, MapAccess, Visitor},
    ser::SerializeStruct,
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{
    collections::BTreeMap,
    fmt,
    marker::PhantomData,
    path::{Component, Path},
};
use thiserror::Error;

fn require_text(value: impl Into<String>, field: &'static str) -> Result<String, ProvenanceError> {
    let value = value.into();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(ProvenanceError::EmptyField { field })
    } else if trimmed != value {
        Err(ProvenanceError::SurroundingWhitespace { field })
    } else {
        Ok(value)
    }
}

/// An evidence identifier whose text is meaningful rather than a placeholder.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EvidenceId(String);

impl EvidenceId {
    pub fn new(value: impl Into<String>) -> Result<Self, ProvenanceError> {
        Ok(Self(require_text(value, "evidence id")?))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for EvidenceId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for EvidenceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

/// A validated proleptic Gregorian calendar date serialized as `YYYY-MM-DD`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CalendarDate(String);

impl CalendarDate {
    pub fn new(value: impl Into<String>) -> Result<Self, ProvenanceError> {
        let value = value.into();
        let bytes = value.as_bytes();
        let valid_shape = bytes.len() == 10
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit());
        if !valid_shape {
            return Err(ProvenanceError::InvalidCalendarDate { value });
        }
        let parse = |start| {
            std::str::from_utf8(&bytes[start..start + 2])
                .ok()
                .and_then(|part| part.parse::<u32>().ok())
        };
        let year = std::str::from_utf8(&bytes[..4])
            .ok()
            .and_then(|part| part.parse::<u32>().ok());
        let (Some(year), Some(month), Some(day)) = (year, parse(5), parse(8)) else {
            return Err(ProvenanceError::InvalidCalendarDate { value });
        };
        if year == 0 {
            return Err(ProvenanceError::InvalidCalendarDate { value });
        }
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let days = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => 0,
        };
        if day == 0 || day > days {
            return Err(ProvenanceError::InvalidCalendarDate { value });
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for CalendarDate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CalendarDate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

/// The precision with which an evidence date is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvidenceDate {
    Exact(CalendarDate),
    OnOrBefore(CalendarDate),
    Unknown,
}

impl EvidenceDate {
    #[must_use]
    pub fn exact(date: CalendarDate) -> Self {
        Self::Exact(date)
    }

    #[must_use]
    pub fn on_or_before(date: CalendarDate) -> Self {
        Self::OnOrBefore(date)
    }
}

impl Serialize for EvidenceDate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Exact(date) => {
                let mut state = serializer.serialize_struct("EvidenceDate", 2)?;
                state.serialize_field("precision", "exact")?;
                state.serialize_field("date", date)?;
                state.end()
            }
            Self::OnOrBefore(date) => {
                let mut state = serializer.serialize_struct("EvidenceDate", 2)?;
                state.serialize_field("precision", "on_or_before")?;
                state.serialize_field("date", date)?;
                state.end()
            }
            Self::Unknown => {
                let mut state = serializer.serialize_struct("EvidenceDate", 1)?;
                state.serialize_field("precision", "unknown")?;
                state.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for EvidenceDate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Precision,
            Date,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Precision {
            Exact,
            OnOrBefore,
            Unknown,
        }
        struct DateVisitor;
        impl<'de> Visitor<'de> for DateVisitor {
            type Value = EvidenceDate;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a strict evidence date")
            }
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut precision = None;
                let mut date = None;
                while let Some(field) = map.next_key::<Field>()? {
                    match field {
                        Field::Precision => {
                            if precision.is_some() {
                                return Err(de::Error::duplicate_field("precision"));
                            }
                            precision = Some(map.next_value()?);
                        }
                        Field::Date => {
                            if date.is_some() {
                                return Err(de::Error::duplicate_field("date"));
                            }
                            date = Some(map.next_value::<CalendarDate>()?);
                        }
                    }
                }
                match precision.ok_or_else(|| de::Error::missing_field("precision"))? {
                    Precision::Exact => Ok(EvidenceDate::Exact(
                        date.ok_or_else(|| de::Error::missing_field("date"))?,
                    )),
                    Precision::OnOrBefore => Ok(EvidenceDate::OnOrBefore(
                        date.ok_or_else(|| de::Error::missing_field("date"))?,
                    )),
                    Precision::Unknown => {
                        if date.is_some() {
                            Err(de::Error::custom("unknown evidence date forbids date"))
                        } else {
                            Ok(EvidenceDate::Unknown)
                        }
                    }
                }
            }
        }
        deserializer.deserialize_struct("EvidenceDate", &["precision", "date"], DateVisitor)
    }
}

fn has_uri_scheme(value: &str) -> bool {
    let Some((scheme, _)) = value.split_once(':') else {
        return false;
    };
    !scheme.is_empty()
        && scheme.as_bytes()[0].is_ascii_alphabetic()
        && scheme
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

fn validate_record(value: impl Into<String>) -> Result<String, ProvenanceError> {
    let value = require_text(value, "record")?;
    let normalized = value.replace('\\', "/");
    let drive_rooted = normalized.len() >= 2
        && normalized.as_bytes()[0].is_ascii_alphabetic()
        && normalized.as_bytes()[1] == b':';
    let path = Path::new(&normalized);
    if has_uri_scheme(&value)
        || normalized.starts_with('/')
        || drive_rooted
        || path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        || normalized.split('/').any(|component| component == "..")
    {
        return Err(ProvenanceError::InvalidRecordPath { value });
    }
    Ok(value)
}

fn validate_locator(value: impl Into<String>) -> Result<String, ProvenanceError> {
    let value = require_text(value, "locator")?;
    let normalized = value.replace('\\', "/");
    if has_uri_scheme(&value) || normalized.starts_with("//") {
        Err(ProvenanceError::ExternalLocator { value })
    } else {
        let drive_rooted = normalized.len() >= 2
            && normalized.as_bytes()[0].is_ascii_alphabetic()
            && normalized.as_bytes()[1] == b':';
        let has_parent = Path::new(&normalized)
            .components()
            .any(|component| matches!(component, Component::ParentDir))
            || normalized.split('/').any(|component| component == "..");
        if drive_rooted || value.starts_with('\\') || has_parent {
            Err(ProvenanceError::InvalidLocator { value })
        } else {
            Ok(value)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum EvidenceSourceData {
    CalibrationPlate {
        plate: PlateId,
        record: String,
        locator: String,
        date: EvidenceDate,
    },
    Identified {
        kind: EvidenceKind,
        id: EvidenceId,
        record: String,
        locator: String,
        date: EvidenceDate,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EvidenceKind {
    VendorDocument,
    InstalledHardware,
    CorpusStudy,
    ProjectRecord,
    OwnerDecision,
}

impl EvidenceKind {
    const fn wire_name(self) -> &'static str {
        match self {
            Self::VendorDocument => "vendor_document",
            Self::InstalledHardware => "installed_hardware",
            Self::CorpusStudy => "corpus_study",
            Self::ProjectRecord => "project_record",
            Self::OwnerDecision => "owner_decision",
        }
    }
}

/// A strict, repository-locatable source for a measurement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceSource(EvidenceSourceData);

/// An exhaustive borrowed view of an [`EvidenceSource`].
///
/// The view exposes validated source metadata without permitting mutation or
/// reconstructing invalid combinations of variant-specific fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceSourceView<'a> {
    CalibrationPlate {
        plate: &'a PlateId,
        record: &'a str,
        locator: &'a str,
        date: &'a EvidenceDate,
    },
    VendorDocument {
        id: &'a EvidenceId,
        record: &'a str,
        locator: &'a str,
        date: &'a EvidenceDate,
    },
    InstalledHardware {
        id: &'a EvidenceId,
        record: &'a str,
        locator: &'a str,
        date: &'a EvidenceDate,
    },
    CorpusStudy {
        id: &'a EvidenceId,
        record: &'a str,
        locator: &'a str,
        date: &'a EvidenceDate,
    },
    ProjectRecord {
        id: &'a EvidenceId,
        record: &'a str,
        locator: &'a str,
        date: &'a EvidenceDate,
    },
    OwnerDecision {
        id: &'a EvidenceId,
        record: &'a str,
        locator: &'a str,
        date: &'a EvidenceDate,
    },
}

impl EvidenceSource {
    #[must_use]
    pub fn view(&self) -> EvidenceSourceView<'_> {
        match &self.0 {
            EvidenceSourceData::CalibrationPlate {
                plate,
                record,
                locator,
                date,
            } => EvidenceSourceView::CalibrationPlate {
                plate,
                record,
                locator,
                date,
            },
            EvidenceSourceData::Identified {
                kind,
                id,
                record,
                locator,
                date,
            } => match kind {
                EvidenceKind::VendorDocument => EvidenceSourceView::VendorDocument {
                    id,
                    record,
                    locator,
                    date,
                },
                EvidenceKind::InstalledHardware => EvidenceSourceView::InstalledHardware {
                    id,
                    record,
                    locator,
                    date,
                },
                EvidenceKind::CorpusStudy => EvidenceSourceView::CorpusStudy {
                    id,
                    record,
                    locator,
                    date,
                },
                EvidenceKind::ProjectRecord => EvidenceSourceView::ProjectRecord {
                    id,
                    record,
                    locator,
                    date,
                },
                EvidenceKind::OwnerDecision => EvidenceSourceView::OwnerDecision {
                    id,
                    record,
                    locator,
                    date,
                },
            },
        }
    }

    pub fn calibration_plate(
        plate: PlateId,
        record: impl Into<String>,
        locator: impl Into<String>,
        date: EvidenceDate,
    ) -> Result<Self, ProvenanceError> {
        Ok(Self(EvidenceSourceData::CalibrationPlate {
            plate,
            record: validate_record(record)?,
            locator: validate_locator(locator)?,
            date,
        }))
    }

    fn identified(
        kind: EvidenceKind,
        id: EvidenceId,
        record: impl Into<String>,
        locator: impl Into<String>,
        date: EvidenceDate,
    ) -> Result<Self, ProvenanceError> {
        Ok(Self(EvidenceSourceData::Identified {
            kind,
            id,
            record: validate_record(record)?,
            locator: validate_locator(locator)?,
            date,
        }))
    }

    pub fn vendor_document(
        id: EvidenceId,
        record: impl Into<String>,
        locator: impl Into<String>,
        date: EvidenceDate,
    ) -> Result<Self, ProvenanceError> {
        Self::identified(EvidenceKind::VendorDocument, id, record, locator, date)
    }
    pub fn installed_hardware(
        id: EvidenceId,
        record: impl Into<String>,
        locator: impl Into<String>,
        date: EvidenceDate,
    ) -> Result<Self, ProvenanceError> {
        Self::identified(EvidenceKind::InstalledHardware, id, record, locator, date)
    }
    pub fn corpus_study(
        id: EvidenceId,
        record: impl Into<String>,
        locator: impl Into<String>,
        date: EvidenceDate,
    ) -> Result<Self, ProvenanceError> {
        Self::identified(EvidenceKind::CorpusStudy, id, record, locator, date)
    }
    pub fn project_record(
        id: EvidenceId,
        record: impl Into<String>,
        locator: impl Into<String>,
        date: EvidenceDate,
    ) -> Result<Self, ProvenanceError> {
        Self::identified(EvidenceKind::ProjectRecord, id, record, locator, date)
    }
    pub fn owner_decision(
        id: EvidenceId,
        record: impl Into<String>,
        locator: impl Into<String>,
        date: EvidenceDate,
    ) -> Result<Self, ProvenanceError> {
        Self::identified(EvidenceKind::OwnerDecision, id, record, locator, date)
    }
}

impl Serialize for EvidenceSource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self.0 {
            EvidenceSourceData::CalibrationPlate {
                plate,
                record,
                locator,
                date,
            } => {
                let mut state = serializer.serialize_struct("EvidenceSource", 5)?;
                state.serialize_field("kind", "calibration_plate")?;
                state.serialize_field("plate", plate)?;
                state.serialize_field("record", record)?;
                state.serialize_field("locator", locator)?;
                state.serialize_field("date", date)?;
                state.end()
            }
            EvidenceSourceData::Identified {
                kind,
                id,
                record,
                locator,
                date,
            } => {
                let mut state = serializer.serialize_struct("EvidenceSource", 5)?;
                state.serialize_field("kind", kind.wire_name())?;
                state.serialize_field("id", id)?;
                state.serialize_field("record", record)?;
                state.serialize_field("locator", locator)?;
                state.serialize_field("date", date)?;
                state.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for EvidenceSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Kind,
            Plate,
            Id,
            Record,
            Locator,
            Date,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Kind {
            CalibrationPlate,
            VendorDocument,
            InstalledHardware,
            CorpusStudy,
            ProjectRecord,
            OwnerDecision,
        }
        struct SourceVisitor;
        impl<'de> Visitor<'de> for SourceVisitor {
            type Value = EvidenceSource;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a strict evidence source")
            }
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut kind = None;
                let mut plate: Option<PlateId> = None;
                let mut id: Option<EvidenceId> = None;
                let mut record: Option<String> = None;
                let mut locator: Option<String> = None;
                let mut date: Option<EvidenceDate> = None;
                while let Some(field) = map.next_key::<Field>()? {
                    match field {
                        Field::Kind => {
                            if kind.is_some() {
                                return Err(de::Error::duplicate_field("kind"));
                            }
                            kind = Some(map.next_value()?);
                        }
                        Field::Plate => {
                            if plate.is_some() {
                                return Err(de::Error::duplicate_field("plate"));
                            }
                            plate = Some(map.next_value()?);
                        }
                        Field::Id => {
                            if id.is_some() {
                                return Err(de::Error::duplicate_field("id"));
                            }
                            id = Some(map.next_value()?);
                        }
                        Field::Record => {
                            if record.is_some() {
                                return Err(de::Error::duplicate_field("record"));
                            }
                            record = Some(map.next_value()?);
                        }
                        Field::Locator => {
                            if locator.is_some() {
                                return Err(de::Error::duplicate_field("locator"));
                            }
                            locator = Some(map.next_value()?);
                        }
                        Field::Date => {
                            if date.is_some() {
                                return Err(de::Error::duplicate_field("date"));
                            }
                            date = Some(map.next_value()?);
                        }
                    }
                }
                let record = record.ok_or_else(|| de::Error::missing_field("record"))?;
                let locator = locator.ok_or_else(|| de::Error::missing_field("locator"))?;
                let date = date.ok_or_else(|| de::Error::missing_field("date"))?;
                match kind.ok_or_else(|| de::Error::missing_field("kind"))? {
                    Kind::CalibrationPlate => {
                        if id.is_some() {
                            return Err(de::Error::custom("calibration_plate forbids id"));
                        }
                        EvidenceSource::calibration_plate(
                            plate.ok_or_else(|| de::Error::missing_field("plate"))?,
                            record,
                            locator,
                            date,
                        )
                    }
                    kind => {
                        if plate.is_some() {
                            return Err(de::Error::custom(
                                "identified evidence source forbids plate",
                            ));
                        }
                        let kind = match kind {
                            Kind::VendorDocument => EvidenceKind::VendorDocument,
                            Kind::InstalledHardware => EvidenceKind::InstalledHardware,
                            Kind::CorpusStudy => EvidenceKind::CorpusStudy,
                            Kind::ProjectRecord => EvidenceKind::ProjectRecord,
                            Kind::OwnerDecision => EvidenceKind::OwnerDecision,
                            Kind::CalibrationPlate => unreachable!(),
                        };
                        EvidenceSource::identified(
                            kind,
                            id.ok_or_else(|| de::Error::missing_field("id"))?,
                            record,
                            locator,
                            date,
                        )
                    }
                }
                .map_err(de::Error::custom)
            }
        }
        deserializer.deserialize_struct(
            "EvidenceSource",
            &["kind", "plate", "id", "record", "locator", "date"],
            SourceVisitor,
        )
    }
}

/// A compact provenance description suitable for a run manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProvenanceRef {
    Measured {
        source: EvidenceSource,
        note: String,
    },
    Derived {
        from: Vec<String>,
        note: String,
    },
}

impl Serialize for ProvenanceRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Measured { source, note } => {
                let mut state = serializer.serialize_struct("ProvenanceRef", 3)?;
                state.serialize_field("kind", "measured")?;
                state.serialize_field("source", source)?;
                state.serialize_field("note", note)?;
                state.end()
            }
            Self::Derived { from, note } => {
                let mut state = serializer.serialize_struct("ProvenanceRef", 3)?;
                state.serialize_field("kind", "derived")?;
                state.serialize_field("from", from)?;
                state.serialize_field("note", note)?;
                state.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for ProvenanceRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Kind,
            Source,
            From,
            Note,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Kind {
            Measured,
            Derived,
        }
        struct RefVisitor;
        impl<'de> Visitor<'de> for RefVisitor {
            type Value = ProvenanceRef;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a strict provenance reference")
            }
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut kind = None;
                let mut source = None;
                let mut from = None;
                let mut note = None;
                while let Some(field) = map.next_key::<Field>()? {
                    match field {
                        Field::Kind => {
                            if kind.is_some() {
                                return Err(de::Error::duplicate_field("kind"));
                            }
                            kind = Some(map.next_value()?);
                        }
                        Field::Source => {
                            if source.is_some() {
                                return Err(de::Error::duplicate_field("source"));
                            }
                            source = Some(map.next_value()?);
                        }
                        Field::From => {
                            if from.is_some() {
                                return Err(de::Error::duplicate_field("from"));
                            }
                            from = Some(map.next_value()?);
                        }
                        Field::Note => {
                            if note.is_some() {
                                return Err(de::Error::duplicate_field("note"));
                            }
                            note = Some(map.next_value()?);
                        }
                    }
                }
                let note = note.ok_or_else(|| de::Error::missing_field("note"))?;
                match kind.ok_or_else(|| de::Error::missing_field("kind"))? {
                    Kind::Measured => {
                        if from.is_some() {
                            return Err(de::Error::custom("measured provenance forbids from"));
                        }
                        Ok(ProvenanceRef::Measured {
                            source: source.ok_or_else(|| de::Error::missing_field("source"))?,
                            note,
                        })
                    }
                    Kind::Derived => {
                        if source.is_some() {
                            return Err(de::Error::custom("derived provenance forbids source"));
                        }
                        Ok(ProvenanceRef::Derived {
                            from: from.ok_or_else(|| de::Error::missing_field("from"))?,
                            note,
                        })
                    }
                }
            }
        }
        deserializer.deserialize_struct(
            "ProvenanceRef",
            &["kind", "source", "from", "note"],
            RefVisitor,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ProvenanceError {
    #[error("{field} must not be empty or whitespace")]
    EmptyField { field: &'static str },
    #[error("{field} must not have leading or trailing whitespace")]
    SurroundingWhitespace { field: &'static str },
    #[error("invalid calendar date {value}")]
    InvalidCalendarDate { value: String },
    #[error("record must be repository-relative and cannot traverse parents: {value}")]
    InvalidRecordPath { value: String },
    #[error("locator must not be an external URI: {value}")]
    ExternalLocator { value: String },
    #[error("locator must be a stable internal reference: {value}")]
    InvalidLocator { value: String },
    #[error("measured numeric value must be finite")]
    NonFiniteMeasuredValue,
    #[error("measured numeric error must be finite")]
    NonFiniteMeasuredError,
    #[error("categorical or structured measured payloads require err to be null")]
    CategoricalErrorBar,
    #[error("finite intervals require err to be null")]
    IntervalErrorBar,
    #[error("finite interval bounds must be finite")]
    NonFiniteIntervalBound,
    #[error("finite interval lower bound must be less than upper bound")]
    InvalidIntervalBounds,
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
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a uniquely keyed provenance constant map")
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
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a uniquely keyed provenance decision map")
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
pub trait AcceptSafe<T: Copy>: sealed::AcceptSafe {
    fn get(&self) -> T;
    fn provenance(&self) -> ProvenanceRef;
}
mod sealed {
    pub trait Sealed {}
    pub trait AcceptSafe {}
}
/// Payload policy for measured constants. This trait is sealed by core.
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
macro_rules! finite_payload { ($($payload:ty),+ $(,)?) => { $( impl sealed::Sealed for $payload {} impl MeasuredPayload for $payload { fn validate_measurement(_value: &Self, _err: Option<&Self>) -> Result<(), ProvenanceError> { Ok(()) } } )+ }; }
finite_payload!(Finite, EffectiveArmCount, MetricValue);
macro_rules! categorical_payload {
    ($($payload:ty),+ $(,)?) => {
        $(
            impl sealed::Sealed for $payload {}
            impl MeasuredPayload for $payload {
                fn validate_measurement(
                    _value: &Self,
                    err: Option<&Self>,
                ) -> Result<(), ProvenanceError> {
                    if err.is_some() {
                        Err(ProvenanceError::CategoricalErrorBar)
                    } else {
                        Ok(())
                    }
                }
            }
        )+
    };
}
// `u64` is categorical here on purpose: the only `Measured<u64>` declarations
// are exact integer counts read off one instrumented run (no dispersion to
// report), so an error bar on them would be a fiction, not a missing field.
categorical_payload!(
    bool,
    u64,
    BuildVolumeMm,
    YDirection,
    LoadSharing,
    MicronGranularity,
    // A table (and one profile) read verbatim off the owner's exported
    // profiles: exact values, no dispersion, so no error bar.
    TipContactProfiles,
    RootBossProfile
);

/// Endpoint inclusion semantics for a finite interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundKind {
    Inclusive,
    Exclusive,
}

/// A strictly ordered interval with explicitly specified endpoint inclusion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FiniteInterval {
    lower: f64,
    upper: f64,
    lower_bound: BoundKind,
    upper_bound: BoundKind,
}
impl FiniteInterval {
    pub fn new(
        lower: f64,
        upper: f64,
        lower_bound: BoundKind,
        upper_bound: BoundKind,
    ) -> Result<Self, ProvenanceError> {
        if !lower.is_finite() || !upper.is_finite() {
            return Err(ProvenanceError::NonFiniteIntervalBound);
        }
        if lower >= upper {
            return Err(ProvenanceError::InvalidIntervalBounds);
        }
        Ok(Self {
            lower,
            upper,
            lower_bound,
            upper_bound,
        })
    }
    #[must_use]
    pub const fn lower(&self) -> f64 {
        self.lower
    }
    #[must_use]
    pub const fn upper(&self) -> f64 {
        self.upper
    }
    #[must_use]
    pub const fn lower_bound(&self) -> BoundKind {
        self.lower_bound
    }
    #[must_use]
    pub const fn upper_bound(&self) -> BoundKind {
        self.upper_bound
    }
    #[must_use]
    pub fn contains(&self, value: f64) -> bool {
        value.is_finite()
            && (if self.lower_bound == BoundKind::Inclusive {
                value >= self.lower
            } else {
                value > self.lower
            })
            && (if self.upper_bound == BoundKind::Inclusive {
                value <= self.upper
            } else {
                value < self.upper
            })
    }
}
impl Serialize for FiniteInterval {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("FiniteInterval", 4)?;
        state.serialize_field("lower", &self.lower)?;
        state.serialize_field("upper", &self.upper)?;
        state.serialize_field("lower_bound", &self.lower_bound)?;
        state.serialize_field("upper_bound", &self.upper_bound)?;
        state.end()
    }
}
impl<'de> Deserialize<'de> for FiniteInterval {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Lower,
            Upper,
            LowerBound,
            UpperBound,
        }
        struct IntervalVisitor;
        impl<'de> Visitor<'de> for IntervalVisitor {
            type Value = FiniteInterval;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a strict finite interval")
            }
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut lower = None;
                let mut upper = None;
                let mut lower_bound = None;
                let mut upper_bound = None;
                while let Some(field) = map.next_key::<Field>()? {
                    match field {
                        Field::Lower => {
                            if lower.is_some() {
                                return Err(de::Error::duplicate_field("lower"));
                            }
                            lower = Some(map.next_value()?);
                        }
                        Field::Upper => {
                            if upper.is_some() {
                                return Err(de::Error::duplicate_field("upper"));
                            }
                            upper = Some(map.next_value()?);
                        }
                        Field::LowerBound => {
                            if lower_bound.is_some() {
                                return Err(de::Error::duplicate_field("lower_bound"));
                            }
                            lower_bound = Some(map.next_value()?);
                        }
                        Field::UpperBound => {
                            if upper_bound.is_some() {
                                return Err(de::Error::duplicate_field("upper_bound"));
                            }
                            upper_bound = Some(map.next_value()?);
                        }
                    }
                }
                FiniteInterval::new(
                    lower.ok_or_else(|| de::Error::missing_field("lower"))?,
                    upper.ok_or_else(|| de::Error::missing_field("upper"))?,
                    lower_bound.ok_or_else(|| de::Error::missing_field("lower_bound"))?,
                    upper_bound.ok_or_else(|| de::Error::missing_field("upper_bound"))?,
                )
                .map_err(de::Error::custom)
            }
        }
        deserializer.deserialize_struct(
            "FiniteInterval",
            &["lower", "upper", "lower_bound", "upper_bound"],
            IntervalVisitor,
        )
    }
}
impl sealed::Sealed for FiniteInterval {}
impl MeasuredPayload for FiniteInterval {
    fn validate_measurement(_value: &Self, err: Option<&Self>) -> Result<(), ProvenanceError> {
        if err.is_some() {
            Err(ProvenanceError::IntervalErrorBar)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Measured<T> {
    value: T,
    source: EvidenceSource,
    err: Option<T>,
    note: String,
}
impl<T: MeasuredPayload> Measured<T> {
    pub fn new(
        value: T,
        source: EvidenceSource,
        err: Option<T>,
        note: impl Into<String>,
    ) -> Result<Self, ProvenanceError> {
        T::validate_measurement(&value, err.as_ref())?;
        Ok(Self {
            value,
            source,
            err,
            note: require_text(note, "note")?,
        })
    }
    #[must_use]
    pub fn value(&self) -> &T {
        &self.value
    }
    #[must_use]
    pub const fn source(&self) -> &EvidenceSource {
        &self.source
    }
    #[must_use]
    pub const fn err(&self) -> Option<&T> {
        self.err.as_ref()
    }
    #[must_use]
    pub fn note(&self) -> &str {
        &self.note
    }
}
impl<T: Serialize> Serialize for Measured<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Measured", 4)?;
        state.serialize_field("value", &self.value)?;
        state.serialize_field("source", &self.source)?;
        state.serialize_field("err", &self.err)?;
        state.serialize_field("note", &self.note)?;
        state.end()
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
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Value,
            Source,
            Err,
            Note,
        }
        struct MeasuredVisitor<T>(PhantomData<T>);
        impl<'de, T> Visitor<'de> for MeasuredVisitor<T>
        where
            T: Deserialize<'de> + MeasuredPayload,
        {
            type Value = Measured<T>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a complete measured payload")
            }
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut value: Option<T> = None;
                let mut source: Option<EvidenceSource> = None;
                let mut err: Option<Option<T>> = None;
                let mut note: Option<String> = None;
                while let Some(field) = map.next_key::<Field>()? {
                    match field {
                        Field::Value => {
                            if value.is_some() {
                                return Err(de::Error::duplicate_field("value"));
                            }
                            value = Some(map.next_value()?);
                        }
                        Field::Source => {
                            if source.is_some() {
                                return Err(de::Error::duplicate_field("source"));
                            }
                            source = Some(map.next_value()?);
                        }
                        Field::Err => {
                            if err.is_some() {
                                return Err(de::Error::duplicate_field("err"));
                            }
                            err = Some(map.next_value::<RequiredOption<T>>()?.0);
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
                    source.ok_or_else(|| de::Error::missing_field("source"))?,
                    err.ok_or_else(|| de::Error::missing_field("err"))?,
                    note.ok_or_else(|| de::Error::missing_field("note"))?,
                )
                .map_err(de::Error::custom)
            }
        }
        deserializer.deserialize_struct(
            "Measured",
            &["value", "source", "err", "note"],
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
            source: self.source.clone(),
            note: self.note.clone(),
        }
    }
}
impl<T: Copy + MeasuredPayload> sealed::AcceptSafe for Measured<T> {}

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
impl<T: Copy> sealed::AcceptSafe for Derived<T> {}
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
///
/// **NO REGISTERED CONSTANT CURRENTLY USES THIS, AND THAT IS BY OWNER RULING,
/// NOT BY DECAY.** It is scaffolding, not vestige, and the distinction was
/// checked before this note was written rather than assumed. The ruling is
/// recorded in `docs/engineering-journal/03-physics-and-constants.md`
/// (the paragraph closing the B1–B5 table): "`Pending<T>` remains unchanged:
/// it requires two concrete executable branches and a real `PlateId`. Do not
/// generalize `ResolutionTarget` now; a corpus-resolved Pending design is
/// deferred until an actual candidate exists."
///
/// So the empty occupancy is the ruling working as intended: the type demands
/// two genuinely executable branches and a real calibration plate that will
/// decide between them, and no constant in the registry can honestly supply
/// those yet. The bar is deliberately high — inventing a `PlateId` or a
/// null/zero sentinel to give this type an occupant is expressly forbidden by
/// the same paragraph. Registry integration is complete and exercised
/// (`RegistryProvenanceClass::Pending`, `RegistryProvenanceView::Pending`,
/// `from_pending`), and `constants_registry.rs` / `core_contracts.rs` hold it
/// to its contract, so it is ready for its first real occupant.
///
/// Do not delete this as dead code. A dead-code census will keep flagging it,
/// correctly on the mechanical evidence and wrongly on the intent; this note
/// is here so the next census reads the ruling before acting on the count.
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
    #[must_use]
    pub fn resolve(&self) -> Derived<T> {
        Derived::new(
            self.conservative,
            [format!("pending plate {}", self.resolves_on)],
            format!("resolved conservatively: {}", self.question),
        )
    }
    #[must_use]
    pub fn optimistic(&self) -> Derived<T> {
        Derived::new(
            self.optimistic,
            [format!("pending plate {}", self.resolves_on)],
            format!("optimistic calibration branch: {}", self.question),
        )
    }
}
