use crate::{
    BoundKind, BuildVolumeMm, ConstantId, Derived, EffectiveArmCount, EvidenceDate, EvidenceSource,
    EvidenceSourceView, Finite, FiniteInterval, LoadSharing, Measured, MeasuredPayload,
    MetricValue, Pending, Placeholder, PlateId, YDirection,
};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum RegistryError {
    #[error("{field} must not be empty or whitespace")]
    EmptyText { field: &'static str },
    #[error("{field} must not have leading or trailing whitespace")]
    SurroundingWhitespace { field: &'static str },
    #[error("{field} must not contain control characters")]
    ControlCharacter { field: &'static str },
    #[error("constant scalar values must be finite")]
    NonFiniteScalar,
    #[error("constant record contains duplicate field {field}")]
    DuplicateRecordField { field: String },
    #[error("derived provenance must name at least one source")]
    EmptyDerivedSources,
    #[error("a constants-registry batch must not be empty")]
    EmptyBatch,
    #[error("duplicate constant id {id}")]
    DuplicateConstantId { id: ConstantId },
}

fn validated_text(value: impl Into<String>, field: &'static str) -> Result<String, RegistryError> {
    let value = value.into();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(RegistryError::EmptyText { field })
    } else if trimmed != value {
        Err(RegistryError::SurroundingWhitespace { field })
    } else {
        Ok(value)
    }
}

fn validated_registry_text(
    value: impl Into<String>,
    field: &'static str,
) -> Result<String, RegistryError> {
    let value = validated_text(value, field)?;
    if value.chars().any(char::is_control) {
        Err(RegistryError::ControlCharacter { field })
    } else {
        Ok(value)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ConstantValueData {
    Bool(bool),
    SignedInteger(i64),
    UnsignedInteger(u64),
    FiniteScalar(Finite),
    Text(String),
    FiniteInterval(FiniteInterval),
    LoadSharing(LoadSharing),
    List(Vec<ConstantValue>),
    Record(BTreeMap<String, ConstantValue>),
}

/// A validated, dependency-light value tree for heterogeneous constants.
///
/// Construction is fallible where text, finite-number, or record-key
/// invariants apply. The private representation prevents invalid recursive
/// values from being assembled with struct literals.
#[derive(Clone, Debug, PartialEq)]
pub struct ConstantValue(ConstantValueData);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConstantValueView<'a> {
    Bool(bool),
    SignedInteger(i64),
    UnsignedInteger(u64),
    FiniteScalar(f64),
    Text(&'a str),
    FiniteInterval(&'a FiniteInterval),
    LoadSharing(&'a LoadSharing),
    List(&'a [ConstantValue]),
    Record(&'a BTreeMap<String, ConstantValue>),
}

impl ConstantValue {
    #[must_use]
    pub const fn boolean(value: bool) -> Self {
        Self(ConstantValueData::Bool(value))
    }

    #[must_use]
    pub const fn signed_integer(value: i64) -> Self {
        Self(ConstantValueData::SignedInteger(value))
    }

    #[must_use]
    pub const fn unsigned_integer(value: u64) -> Self {
        Self(ConstantValueData::UnsignedInteger(value))
    }

    pub fn finite_scalar(value: f64) -> Result<Self, RegistryError> {
        let value = Finite::new(value).map_err(|_| RegistryError::NonFiniteScalar)?;
        Ok(Self(ConstantValueData::FiniteScalar(value)))
    }

    fn core_text(value: impl Into<String>) -> Result<Self, RegistryError> {
        Ok(Self(ConstantValueData::Text(validated_registry_text(
            value,
            "text value",
        )?)))
    }

    #[must_use]
    pub const fn finite_interval(value: FiniteInterval) -> Self {
        Self(ConstantValueData::FiniteInterval(value))
    }

    #[must_use]
    pub const fn load_sharing(value: LoadSharing) -> Self {
        Self(ConstantValueData::LoadSharing(value))
    }

    pub fn list<I, V>(values: I) -> Result<Self, RegistryError>
    where
        I: IntoIterator<Item = V>,
        V: ToConstantValue,
    {
        let values = values
            .into_iter()
            .map(|value| value.to_constant_value())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self(ConstantValueData::List(values)))
    }

    pub fn record<I, K, V>(fields: I) -> Result<Self, RegistryError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: ToConstantValue,
    {
        let mut record = BTreeMap::new();
        for (key, value) in fields {
            let key = validated_registry_text(key, "record field")?;
            if record.contains_key(&key) {
                return Err(RegistryError::DuplicateRecordField { field: key });
            }
            record.insert(key, value.to_constant_value()?);
        }
        Ok(Self(ConstantValueData::Record(record)))
    }

    #[must_use]
    pub fn view(&self) -> ConstantValueView<'_> {
        match &self.0 {
            ConstantValueData::Bool(value) => ConstantValueView::Bool(*value),
            ConstantValueData::SignedInteger(value) => ConstantValueView::SignedInteger(*value),
            ConstantValueData::UnsignedInteger(value) => ConstantValueView::UnsignedInteger(*value),
            ConstantValueData::FiniteScalar(value) => ConstantValueView::FiniteScalar(value.get()),
            ConstantValueData::Text(value) => ConstantValueView::Text(value),
            ConstantValueData::FiniteInterval(value) => ConstantValueView::FiniteInterval(value),
            ConstantValueData::LoadSharing(value) => ConstantValueView::LoadSharing(value),
            ConstantValueData::List(value) => ConstantValueView::List(value),
            ConstantValueData::Record(value) => ConstantValueView::Record(value),
        }
    }

    fn validate(&self) -> Result<(), RegistryError> {
        match &self.0 {
            ConstantValueData::FiniteScalar(value) if !value.get().is_finite() => {
                Err(RegistryError::NonFiniteScalar)
            }
            ConstantValueData::Text(value) => {
                validated_registry_text(value, "text value").map(|_| ())
            }
            ConstantValueData::List(values) => values.iter().try_for_each(Self::validate),
            ConstantValueData::Record(fields) => {
                for (key, value) in fields {
                    validated_registry_text(key, "record field")?;
                    value.validate()?;
                }
                Ok(())
            }
            ConstantValueData::Bool(_)
            | ConstantValueData::SignedInteger(_)
            | ConstantValueData::UnsignedInteger(_)
            | ConstantValueData::FiniteScalar(_)
            | ConstantValueData::FiniteInterval(_)
            | ConstantValueData::LoadSharing(_) => Ok(()),
        }
    }
}

mod sealed {
    pub trait ConstantValueSource {}
}

/// Core-controlled conversion into the registry's heterogeneous value tree.
///
/// The trait is sealed so unsupported domain values cannot silently degrade to
/// arbitrary `Debug` strings. New domain types require an explicit core-owned
/// conversion policy.
///
/// Generic strings are deliberately unsupported:
///
/// ```compile_fail
/// use plinth_core::ToConstantValue;
///
/// let _ = String::from("not a typed domain value").to_constant_value();
/// ```
///
/// ```compile_fail
/// use plinth_core::ToConstantValue;
///
/// let _ = "not a typed domain value".to_constant_value();
/// ```
pub trait ToConstantValue: sealed::ConstantValueSource {
    fn to_constant_value(&self) -> Result<ConstantValue, RegistryError>;
}

impl sealed::ConstantValueSource for ConstantValue {}
impl ToConstantValue for ConstantValue {
    fn to_constant_value(&self) -> Result<ConstantValue, RegistryError> {
        self.validate()?;
        Ok(self.clone())
    }
}

impl<T: ToConstantValue + ?Sized> sealed::ConstantValueSource for &T {}
impl<T: ToConstantValue + ?Sized> ToConstantValue for &T {
    fn to_constant_value(&self) -> Result<ConstantValue, RegistryError> {
        (*self).to_constant_value()
    }
}

macro_rules! constant_value_conversion {
    ($type:ty, $body:expr) => {
        impl sealed::ConstantValueSource for $type {}
        impl ToConstantValue for $type {
            fn to_constant_value(&self) -> Result<ConstantValue, RegistryError> {
                $body(self)
            }
        }
    };
}

constant_value_conversion!(bool, |value: &bool| Ok(ConstantValue::boolean(*value)));
constant_value_conversion!(i8, |value: &i8| Ok(ConstantValue::signed_integer(
    i64::from(*value)
)));
constant_value_conversion!(i16, |value: &i16| Ok(ConstantValue::signed_integer(
    i64::from(*value)
)));
constant_value_conversion!(i32, |value: &i32| Ok(ConstantValue::signed_integer(
    i64::from(*value)
)));
constant_value_conversion!(i64, |value: &i64| Ok(ConstantValue::signed_integer(*value)));
constant_value_conversion!(isize, |value: &isize| Ok(ConstantValue::signed_integer(
    *value as i64
)));
constant_value_conversion!(u8, |value: &u8| Ok(ConstantValue::unsigned_integer(
    u64::from(*value)
)));
constant_value_conversion!(u16, |value: &u16| Ok(ConstantValue::unsigned_integer(
    u64::from(*value)
)));
constant_value_conversion!(u32, |value: &u32| Ok(ConstantValue::unsigned_integer(
    u64::from(*value)
)));
constant_value_conversion!(u64, |value: &u64| Ok(ConstantValue::unsigned_integer(
    *value
)));
constant_value_conversion!(usize, |value: &usize| Ok(ConstantValue::unsigned_integer(
    *value as u64
)));
constant_value_conversion!(f64, |value: &f64| ConstantValue::finite_scalar(*value));
constant_value_conversion!(Finite, |value: &Finite| ConstantValue::finite_scalar(
    value.get()
));
constant_value_conversion!(EffectiveArmCount, |value: &EffectiveArmCount| {
    ConstantValue::finite_scalar(value.get())
});
constant_value_conversion!(MetricValue, |value: &MetricValue| {
    ConstantValue::finite_scalar(value.get())
});
constant_value_conversion!(FiniteInterval, |value: &FiniteInterval| {
    Ok(ConstantValue::finite_interval(*value))
});
constant_value_conversion!(LoadSharing, |value: &LoadSharing| {
    Ok(ConstantValue::load_sharing(*value))
});
constant_value_conversion!(BuildVolumeMm, |value: &BuildVolumeMm| {
    ConstantValue::record([("x", value.x()), ("y", value.y()), ("z", value.z())])
});
constant_value_conversion!(YDirection, |value: &YDirection| {
    ConstantValue::core_text(value.as_str())
});

impl<T: ToConstantValue> sealed::ConstantValueSource for Vec<T> {}
impl<T: ToConstantValue> ToConstantValue for Vec<T> {
    fn to_constant_value(&self) -> Result<ConstantValue, RegistryError> {
        ConstantValue::list(self)
    }
}

impl<T: ToConstantValue, const N: usize> sealed::ConstantValueSource for [T; N] {}
impl<T: ToConstantValue, const N: usize> ToConstantValue for [T; N] {
    fn to_constant_value(&self) -> Result<ConstantValue, RegistryError> {
        ConstantValue::list(self)
    }
}

impl<T: ToConstantValue> sealed::ConstantValueSource for BTreeMap<String, T> {}
impl<T: ToConstantValue> ToConstantValue for BTreeMap<String, T> {
    fn to_constant_value(&self) -> Result<ConstantValue, RegistryError> {
        ConstantValue::record(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegistryProvenanceClass {
    Measured,
    Derived,
    Placeholder,
    Pending,
}

#[derive(Clone, Debug, PartialEq)]
enum RegistryProvenanceData {
    Measured {
        source: EvidenceSource,
        err: Option<ConstantValue>,
        note: String,
    },
    Derived {
        from: Vec<String>,
        note: String,
    },
    Placeholder {
        note: String,
    },
    Pending {
        conservative: ConstantValue,
        optimistic: ConstantValue,
        resolves_on: PlateId,
        question: String,
    },
}

/// Registry metadata extracted from one of the four actual provenance wrappers.
///
/// There is intentionally no public constructor: descriptor construction takes
/// a wrapper and extracts its value and metadata together.
#[derive(Clone, Debug, PartialEq)]
pub struct RegistryProvenance(RegistryProvenanceData);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RegistryProvenanceView<'a> {
    Measured {
        source: &'a EvidenceSource,
        err: Option<&'a ConstantValue>,
        note: &'a str,
    },
    Derived {
        from: &'a [String],
        note: &'a str,
    },
    Placeholder {
        note: &'a str,
    },
    Pending {
        conservative: &'a ConstantValue,
        optimistic: &'a ConstantValue,
        resolves_on: &'a PlateId,
        question: &'a str,
    },
}

impl RegistryProvenance {
    fn from_measured<T>(measured: &Measured<T>) -> Result<Self, RegistryError>
    where
        T: MeasuredPayload + ToConstantValue,
    {
        validate_evidence_source(measured.source())?;
        let note = validated_text(measured.note(), "provenance note")?;
        let err = measured
            .err()
            .map(ToConstantValue::to_constant_value)
            .transpose()?;
        Ok(Self(RegistryProvenanceData::Measured {
            source: measured.source().clone(),
            err,
            note,
        }))
    }

    fn from_derived<T>(derived: &Derived<T>) -> Result<Self, RegistryError> {
        if derived.from.is_empty() {
            return Err(RegistryError::EmptyDerivedSources);
        }
        let from = derived
            .from
            .iter()
            .map(|source| validated_text(source, "derived source"))
            .collect::<Result<Vec<_>, _>>()?;
        let note = validated_text(&derived.note, "provenance note")?;
        Ok(Self(RegistryProvenanceData::Derived { from, note }))
    }

    fn from_placeholder<T>(placeholder: &Placeholder<T>) -> Result<Self, RegistryError> {
        Ok(Self(RegistryProvenanceData::Placeholder {
            note: validated_text(&placeholder.note, "provenance note")?,
        }))
    }

    fn from_pending<T>(pending: &Pending<T>) -> Result<Self, RegistryError>
    where
        T: ToConstantValue,
    {
        validated_text(pending.resolves_on.as_str(), "pending source plate")?;
        Ok(Self(RegistryProvenanceData::Pending {
            conservative: pending.conservative.to_constant_value()?,
            optimistic: pending.optimistic.to_constant_value()?,
            resolves_on: pending.resolves_on.clone(),
            question: validated_text(&pending.question, "pending question")?,
        }))
    }

    #[must_use]
    pub const fn class(&self) -> RegistryProvenanceClass {
        match self.0 {
            RegistryProvenanceData::Measured { .. } => RegistryProvenanceClass::Measured,
            RegistryProvenanceData::Derived { .. } => RegistryProvenanceClass::Derived,
            RegistryProvenanceData::Placeholder { .. } => RegistryProvenanceClass::Placeholder,
            RegistryProvenanceData::Pending { .. } => RegistryProvenanceClass::Pending,
        }
    }

    #[must_use]
    pub fn view(&self) -> RegistryProvenanceView<'_> {
        match &self.0 {
            RegistryProvenanceData::Measured { source, err, note } => {
                RegistryProvenanceView::Measured {
                    source,
                    err: err.as_ref(),
                    note,
                }
            }
            RegistryProvenanceData::Derived { from, note } => {
                RegistryProvenanceView::Derived { from, note }
            }
            RegistryProvenanceData::Placeholder { note } => {
                RegistryProvenanceView::Placeholder { note }
            }
            RegistryProvenanceData::Pending {
                conservative,
                optimistic,
                resolves_on,
                question,
            } => RegistryProvenanceView::Pending {
                conservative,
                optimistic,
                resolves_on,
                question,
            },
        }
    }

    fn validate(&self) -> Result<(), RegistryError> {
        match &self.0 {
            RegistryProvenanceData::Measured { source, err, note } => {
                validate_evidence_source(source)?;
                if let Some(err) = err {
                    err.validate()?;
                }
                validated_text(note, "provenance note").map(|_| ())
            }
            RegistryProvenanceData::Derived { from, note } => {
                if from.is_empty() {
                    return Err(RegistryError::EmptyDerivedSources);
                }
                for source in from {
                    validated_text(source, "derived source")?;
                }
                validated_text(note, "provenance note").map(|_| ())
            }
            RegistryProvenanceData::Placeholder { note } => {
                validated_text(note, "provenance note").map(|_| ())
            }
            RegistryProvenanceData::Pending {
                conservative,
                optimistic,
                resolves_on,
                question,
            } => {
                conservative.validate()?;
                optimistic.validate()?;
                validated_text(resolves_on.as_str(), "pending source plate")?;
                validated_text(question, "pending question").map(|_| ())
            }
        }
    }
}

fn validate_evidence_source(source: &EvidenceSource) -> Result<(), RegistryError> {
    match source.view() {
        EvidenceSourceView::CalibrationPlate {
            plate,
            record,
            locator,
            ..
        } => {
            validated_text(plate.as_str(), "evidence source plate")?;
            validated_text(record, "evidence record")?;
            validated_text(locator, "evidence locator")?;
        }
        EvidenceSourceView::VendorDocument {
            id,
            record,
            locator,
            ..
        }
        | EvidenceSourceView::InstalledHardware {
            id,
            record,
            locator,
            ..
        }
        | EvidenceSourceView::CorpusStudy {
            id,
            record,
            locator,
            ..
        }
        | EvidenceSourceView::ProjectRecord {
            id,
            record,
            locator,
            ..
        }
        | EvidenceSourceView::OwnerDecision {
            id,
            record,
            locator,
            ..
        } => {
            validated_text(id.as_str(), "evidence source id")?;
            validated_text(record, "evidence record")?;
            validated_text(locator, "evidence locator")?;
        }
    }
    Ok(())
}

/// One validated constant and its wrapper-derived registry metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct ConstantDescriptor {
    id: ConstantId,
    value: ConstantValue,
    unit: String,
    shape: String,
    provenance: RegistryProvenance,
}

impl ConstantDescriptor {
    pub fn from_measured<T>(
        id: ConstantId,
        unit: impl Into<String>,
        shape: impl Into<String>,
        measured: &Measured<T>,
    ) -> Result<Self, RegistryError>
    where
        T: MeasuredPayload + ToConstantValue,
    {
        Self::new(
            id,
            measured.value().to_constant_value()?,
            unit,
            shape,
            RegistryProvenance::from_measured(measured)?,
        )
    }

    pub fn from_derived<T>(
        id: ConstantId,
        unit: impl Into<String>,
        shape: impl Into<String>,
        derived: &Derived<T>,
    ) -> Result<Self, RegistryError>
    where
        T: ToConstantValue,
    {
        Self::new(
            id,
            derived.value().to_constant_value()?,
            unit,
            shape,
            RegistryProvenance::from_derived(derived)?,
        )
    }

    pub fn from_placeholder<T>(
        id: ConstantId,
        unit: impl Into<String>,
        shape: impl Into<String>,
        placeholder: &Placeholder<T>,
    ) -> Result<Self, RegistryError>
    where
        T: ToConstantValue,
    {
        Self::new(
            id,
            placeholder.diagnostic().to_constant_value()?,
            unit,
            shape,
            RegistryProvenance::from_placeholder(placeholder)?,
        )
    }

    pub fn from_pending<T>(
        id: ConstantId,
        unit: impl Into<String>,
        shape: impl Into<String>,
        pending: &Pending<T>,
    ) -> Result<Self, RegistryError>
    where
        T: ToConstantValue,
    {
        let conservative = pending.conservative.to_constant_value()?;
        let optimistic = pending.optimistic.to_constant_value()?;
        let value =
            ConstantValue::record([("conservative", conservative), ("optimistic", optimistic)])?;
        Self::new(
            id,
            value,
            unit,
            shape,
            RegistryProvenance::from_pending(pending)?,
        )
    }

    fn new(
        id: ConstantId,
        value: ConstantValue,
        unit: impl Into<String>,
        shape: impl Into<String>,
        provenance: RegistryProvenance,
    ) -> Result<Self, RegistryError> {
        let descriptor = Self {
            id,
            value,
            unit: validated_registry_text(unit, "constant unit")?,
            shape: validated_registry_text(shape, "constant shape")?,
            provenance,
        };
        descriptor.validate()?;
        Ok(descriptor)
    }

    fn validate(&self) -> Result<(), RegistryError> {
        validated_registry_text(self.id.as_str(), "constant id")?;
        validated_registry_text(&self.unit, "constant unit")?;
        validated_registry_text(&self.shape, "constant shape")?;
        self.value.validate()?;
        self.provenance.validate()
    }

    #[must_use]
    pub const fn id(&self) -> &ConstantId {
        &self.id
    }

    #[must_use]
    pub const fn value(&self) -> &ConstantValue {
        &self.value
    }

    #[must_use]
    pub fn unit(&self) -> &str {
        &self.unit
    }

    #[must_use]
    pub fn shape(&self) -> &str {
        &self.shape
    }

    #[must_use]
    pub const fn provenance(&self) -> &RegistryProvenance {
        &self.provenance
    }
}

/// An immutable registry published only after a declaration batch validates.
///
/// This is the future global hook: one `LazyLock<Result<ConstantsRegistry,
/// RegistryError>>` may gather a declaration batch and call `build_batch` once.
/// This structural builder deliberately does not claim that the batch contains
/// every expected constant; the future all-declarations task owns that check.
/// The framework provides no global, `Default`, incremental registration, or
/// mutation API.
///
/// Incremental publication is intentionally not part of the public surface:
///
/// ```compile_fail
/// use plinth_core::{ConstantDescriptor, ConstantsRegistry};
///
/// fn register_one(registry: &mut ConstantsRegistry, descriptor: ConstantDescriptor) {
///     registry.register(descriptor);
/// }
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct ConstantsRegistry {
    entries: BTreeMap<ConstantId, ConstantDescriptor>,
}

impl ConstantsRegistry {
    pub fn build_batch(
        descriptors: impl IntoIterator<Item = ConstantDescriptor>,
    ) -> Result<Self, RegistryError> {
        let mut descriptors = descriptors.into_iter().collect::<Vec<_>>();
        if descriptors.is_empty() {
            return Err(RegistryError::EmptyBatch);
        }
        descriptors.sort_by(|left, right| left.id.cmp(&right.id));
        for descriptor in &descriptors {
            descriptor.validate()?;
        }
        for pair in descriptors.windows(2) {
            if pair[0].id == pair[1].id {
                return Err(RegistryError::DuplicateConstantId {
                    id: pair[0].id.clone(),
                });
            }
        }
        let entries = descriptors
            .into_iter()
            .map(|descriptor| (descriptor.id.clone(), descriptor))
            .collect();
        Ok(Self { entries })
    }

    #[must_use]
    pub fn get(&self, id: &ConstantId) -> Option<&ConstantDescriptor> {
        self.entries.get(id)
    }

    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &ConstantDescriptor> {
        self.entries.values()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Renders a deterministic checked view without consulting the source ledger.
    #[must_use]
    pub fn render_markdown(&self) -> String {
        let mut output = String::from(
            "# Constants Registry\n\n| ID | Value | Unit | Shape | Provenance |\n| --- | --- | --- | --- | --- |\n",
        );
        for descriptor in self.iter() {
            output.push_str("| ");
            output.push_str(&escape_markdown_cell(descriptor.id.as_str()));
            output.push_str(" | ");
            output.push_str(&escape_markdown_cell(&format_constant_value(
                &descriptor.value,
            )));
            output.push_str(" | ");
            output.push_str(&escape_markdown_cell(&descriptor.unit));
            output.push_str(" | ");
            output.push_str(&escape_markdown_cell(&descriptor.shape));
            output.push_str(" | ");
            output.push_str(&escape_markdown_cell(&format_registry_provenance(
                &descriptor.provenance,
            )));
            output.push_str(" |\n");
        }
        output
    }
}

fn format_constant_value(value: &ConstantValue) -> String {
    match value.view() {
        ConstantValueView::Bool(value) => value.to_string(),
        ConstantValueView::SignedInteger(value) => value.to_string(),
        ConstantValueView::UnsignedInteger(value) => value.to_string(),
        ConstantValueView::FiniteScalar(value) => format_finite(value),
        ConstantValueView::Text(value) => quoted(value),
        ConstantValueView::FiniteInterval(value) => format!(
            "interval(lower={},lower_bound={},upper={},upper_bound={})",
            format_finite(value.lower()),
            format_bound(value.lower_bound()),
            format_finite(value.upper()),
            format_bound(value.upper_bound())
        ),
        ConstantValueView::LoadSharing(LoadSharing::Zipper) => "zipper".to_owned(),
        ConstantValueView::LoadSharing(LoadSharing::Additive { n_eff_cap }) => {
            format!("additive(n_eff_cap={})", format_finite(n_eff_cap.get()))
        }
        ConstantValueView::List(values) => {
            let values = values
                .iter()
                .map(format_constant_value)
                .collect::<Vec<_>>()
                .join(",");
            format!("[{values}]")
        }
        ConstantValueView::Record(fields) => {
            let fields = fields
                .iter()
                .map(|(key, value)| format!("{}:{}", quoted(key), format_constant_value(value)))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{fields}}}")
        }
    }
}

fn format_registry_provenance(provenance: &RegistryProvenance) -> String {
    match provenance.view() {
        RegistryProvenanceView::Measured { source, err, note } => {
            let err = err.map_or_else(|| "null".to_owned(), format_constant_value);
            format!(
                "measured(source={},err={},note={})",
                format_evidence_source(source),
                err,
                quoted(note)
            )
        }
        RegistryProvenanceView::Derived { from, note } => {
            let from = from.iter().map(|value| quoted(value)).collect::<Vec<_>>();
            format!("derived(from=[{}],note={})", from.join(","), quoted(note))
        }
        RegistryProvenanceView::Placeholder { note } => {
            format!("placeholder(note={})", quoted(note))
        }
        RegistryProvenanceView::Pending {
            conservative,
            optimistic,
            resolves_on,
            question,
        } => format!(
            "pending(conservative={},optimistic={},resolves_on={},question={})",
            format_constant_value(conservative),
            format_constant_value(optimistic),
            quoted(resolves_on.as_str()),
            quoted(question)
        ),
    }
}

fn format_evidence_source(source: &EvidenceSource) -> String {
    match source.view() {
        EvidenceSourceView::CalibrationPlate {
            plate,
            record,
            locator,
            date,
        } => format!(
            "calibration_plate(plate={},record={},locator={},date={})",
            quoted(plate.as_str()),
            quoted(record),
            quoted(locator),
            format_evidence_date(date)
        ),
        EvidenceSourceView::VendorDocument {
            id,
            record,
            locator,
            date,
        } => format_identified_source("vendor_document", id.as_str(), record, locator, date),
        EvidenceSourceView::InstalledHardware {
            id,
            record,
            locator,
            date,
        } => format_identified_source("installed_hardware", id.as_str(), record, locator, date),
        EvidenceSourceView::CorpusStudy {
            id,
            record,
            locator,
            date,
        } => format_identified_source("corpus_study", id.as_str(), record, locator, date),
        EvidenceSourceView::ProjectRecord {
            id,
            record,
            locator,
            date,
        } => format_identified_source("project_record", id.as_str(), record, locator, date),
        EvidenceSourceView::OwnerDecision {
            id,
            record,
            locator,
            date,
        } => format_identified_source("owner_decision", id.as_str(), record, locator, date),
    }
}

fn format_identified_source(
    kind: &str,
    id: &str,
    record: &str,
    locator: &str,
    date: &EvidenceDate,
) -> String {
    format!(
        "{kind}(id={},record={},locator={},date={})",
        quoted(id),
        quoted(record),
        quoted(locator),
        format_evidence_date(date)
    )
}

fn format_evidence_date(date: &EvidenceDate) -> String {
    match date {
        EvidenceDate::Exact(date) => format!("exact({})", quoted(date.as_str())),
        EvidenceDate::OnOrBefore(date) => {
            format!("on_or_before({})", quoted(date.as_str()))
        }
        EvidenceDate::Unknown => "unknown".to_owned(),
    }
}

const fn format_bound(bound: BoundKind) -> &'static str {
    match bound {
        BoundKind::Inclusive => "inclusive",
        BoundKind::Exclusive => "exclusive",
    }
}

fn format_finite(value: f64) -> String {
    let mut formatted = value.to_string();
    if !formatted.contains(['.', 'e', 'E']) {
        formatted.push_str(".0");
    }
    formatted
}

fn quoted(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => output.extend(character.escape_default()),
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn escape_markdown_cell(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '|' => output.push_str("&#124;"),
            '\\' => output.push_str("&#92;"),
            '`' => output.push_str("&#96;"),
            '*' => output.push_str("&#42;"),
            '_' => output.push_str("&#95;"),
            '[' => output.push_str("&#91;"),
            ']' => output.push_str("&#93;"),
            '~' => output.push_str("&#126;"),
            character if character.is_control() => {
                output.push_str("&#");
                output.push_str(&(character as u32).to_string());
                output.push(';');
            }
            character => output.push(character),
        }
    }
    output
}
