use crate::{
    BoundKind, BuildVolumeMm, ConstantId, Derived, EffectiveArmCount, EvidenceDate, EvidenceSource,
    EvidenceSourceView, Finite, FiniteInterval, LoadSharing, Measured, MeasuredPayload,
    MetricValue, MicronGranularity, Pending, Placeholder, PlateId, PrinterProfile,
    PrinterProfileError, YDirection,
};
use std::collections::BTreeMap;
use std::sync::LazyLock;
use thiserror::Error;

mod evidence;
mod placeholders;

pub use evidence::{M0EvidenceConstants, M0EvidenceConstantsError};
pub use placeholders::{
    AnchorCandidateCap, BeamWidth, BridgeSagLimit, CcoSplitSlidePolicy, DecimationAdaptationPolicy,
    DecimationRatioSearchPolicy, DecimationSamplingPolicy, DrainDirection, DrainDirections,
    FixBraceIterationCap, HollowExtraDrainPolicy, M0PlaceholderConstants,
    M0PlaceholderConstantsError, NonNegativeFinite, PlaceholderValueError, PlacementXyMm,
    PositiveFinite, SeedSpacingFactor, SphereTessellationPolicy, TrialSetting, UnitFraction,
    VentDirections,
};

/// Errors while atomically constructing the complete M0 constants registry.
#[derive(Clone, Debug, PartialEq, Error)]
pub enum M0ConstantsError {
    #[error(transparent)]
    Evidence(#[from] M0EvidenceConstantsError),
    #[error(transparent)]
    Placeholder(#[from] M0PlaceholderConstantsError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    PrinterProfile(#[from] PrinterProfileError),
}

/// The one complete, immutable M0 constants publication.
///
/// Construction gathers both declaration components, validates their combined
/// batch, constructs the typed Saturn profile, and only then publishes this
/// value through [`m0_constants`]. There is no incremental global registry.
#[derive(Clone, Debug, PartialEq)]
pub struct M0Constants {
    registry: ConstantsRegistry,
    evidence: M0EvidenceConstants,
    placeholders: M0PlaceholderConstants,
    saturn4_ultra: PrinterProfile,
}

/// Immutable typed authority for the SCAR-AREA-CAP and PRINT-FLOOR inputs.
///
/// This is deliberately a view over the one M0 registry, rather than a second
/// declaration set. The policy literals remain owned by their canonical
/// registry entries: seven are `Placeholder` policy values and
/// `PRINT_FLOOR_CHORD_MM` is an owner-decision `Measured` value. See
/// `docs/specs/SPEC-EMBED-POLICY.md` "Normative constants" and
/// "PRINT-FLOOR amendment", and
/// `docs/rust-migration/03-physics-and-constants.md` §3.6.
///
/// The later measurement/gate implementation must preserve the specification's
/// strict boundaries: a scar limit is legal at equality (`<=`), while an
/// unanchored transit is moot only when its chord is strictly below the print
/// floor (`<`); equality is a firm transit. This type supplies no geometry,
/// predicate, request limit, or report behavior.
#[derive(Clone, Copy, Debug)]
pub struct ScarAreaCapPolicy<'a> {
    a_abs_mm2: &'a Placeholder<PositiveFinite>,
    kappa: &'a Placeholder<PositiveFinite>,
    l_scar_max_mm: &'a Placeholder<PositiveFinite>,
    eps_cos: &'a Placeholder<PositiveFinite>,
    interval_tol_mm: &'a Placeholder<PositiveFinite>,
    print_floor_chord_mm: &'a Measured<f64>,
    tip_ball_factor: &'a Placeholder<PositiveFinite>,
    foot_ball_factor: &'a Placeholder<PositiveFinite>,
}

impl<'a> ScarAreaCapPolicy<'a> {
    /// Stable decision identity for provenance recording once the gate exists.
    #[must_use]
    pub fn decision_id(&self) -> crate::DecisionId {
        crate::DecisionId::from("SCAR-AREA-CAP")
    }

    /// Canonical registry IDs, in the policy specification's order.
    #[must_use]
    pub const fn constant_ids(&self) -> [&'static str; 8] {
        [
            "A_ABS_MM2",
            "KAPPA",
            "L_SCAR_MAX_MM",
            "EPS_COS",
            "TOL_MM",
            "PRINT_FLOOR_CHORD_MM",
            "BALL_FACTOR_STUB",
            "BALL_FACTOR_FOOT",
        ]
    }

    #[must_use]
    pub const fn a_abs_mm2(&self) -> &'a Placeholder<PositiveFinite> {
        self.a_abs_mm2
    }

    #[must_use]
    pub const fn kappa(&self) -> &'a Placeholder<PositiveFinite> {
        self.kappa
    }

    #[must_use]
    pub const fn l_scar_max_mm(&self) -> &'a Placeholder<PositiveFinite> {
        self.l_scar_max_mm
    }

    #[must_use]
    pub const fn eps_cos(&self) -> &'a Placeholder<PositiveFinite> {
        self.eps_cos
    }

    #[must_use]
    pub const fn interval_tol_mm(&self) -> &'a Placeholder<PositiveFinite> {
        self.interval_tol_mm
    }

    #[must_use]
    pub const fn print_floor_chord_mm(&self) -> &'a Measured<f64> {
        self.print_floor_chord_mm
    }

    #[must_use]
    pub const fn tip_ball_factor(&self) -> &'a Placeholder<PositiveFinite> {
        self.tip_ball_factor
    }

    #[must_use]
    pub const fn foot_ball_factor(&self) -> &'a Placeholder<PositiveFinite> {
        self.foot_ball_factor
    }
}

/// Immutable typed authority for the support pipeline's four scaled execution
/// caps.
///
/// Like [`ScarAreaCapPolicy`] this is a view over the one M0 registry, not a
/// second declaration set. All four caps take the same shape,
/// `max(floor, coefficient × scale)`, so that a cap refuses work that is
/// *superlinear in its own input* rather than refusing large models. The floors
/// are the inherited unmeasured constants and are `Placeholder`; the
/// coefficients are `Derived` from one instrumented Weeping Treant run by one
/// stated rule, `next_power_of_two(2 × observed)`.
///
/// They differ only in what `scale` is, and that difference is itself measured:
///
/// * kNN work and parity-grid bytes scale on the **support mesh face count**,
///   because both batches are built face by face.
/// * Specialized gate and load operations scale on their own **batch product**
///   — `(coverage_samples + 1) × (contacts + 1)` for the gate,
///   `(rows + 1) × (contacts + 1)` for the load — because gate and load work
///   does *not* track face count. 24-face `make_raking_blade` charges
///   612,887,212 gate operations against 429,120-face Weeping Treant's
///   12,714,828,544: a 20× spread over an 18,000× face spread. Scaling those
///   two on faces would have refused the small model and admitted anything on
///   the large one.
///
/// The 12,714,828,544 above is a dated FU-8b-era charge and is quoted here only
/// for that spread. FU-8d phase F3's repricing plausibly supersedes it as an
/// input and no re-measurement exists at this tip; the ruling note on
/// `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` (`constants/evidence.rs`, and
/// `docs/rust-migration/support-resource-caps-2026-08-05.md` §7.1) is the
/// primary record. The spread this paragraph draws from it is unaffected.
///
/// These are execution-authority bounds only. They admit or refuse; they never
/// alter a plan that is admitted, so they are result-inert and excluded from
/// every semantic and result digest.
#[derive(Clone, Copy, Debug)]
pub struct SupportResourceCapPolicy<'a> {
    knn_max_work_floor: &'a Placeholder<std::num::NonZeroU64>,
    knn_max_work_per_face: &'a Derived<u64>,
    parity_grid_max_bytes_floor: &'a Placeholder<std::num::NonZeroU64>,
    parity_grid_max_bytes_per_face: &'a Derived<u64>,
    gate_max_operations_floor: &'a Placeholder<std::num::NonZeroU64>,
    gate_max_work_per_batch_unit: &'a Derived<u64>,
    load_max_operations_floor: &'a Placeholder<std::num::NonZeroU64>,
    load_max_work_per_batch_unit: &'a Derived<u64>,
}

impl<'a> SupportResourceCapPolicy<'a> {
    /// Stable decision identity for provenance recording.
    #[must_use]
    pub fn decision_id(&self) -> crate::DecisionId {
        crate::DecisionId::from("SUPPORT-RESOURCE-CAP")
    }

    /// Canonical registry IDs, floor then coefficient, kNN then parity grid
    /// then specialized gate then specialized load.
    #[must_use]
    pub const fn constant_ids(&self) -> [&'static str; 8] {
        [
            "SUPPORT_KNN_MAX_WORK_FLOOR",
            "SUPPORT_KNN_MAX_WORK_PER_FACE",
            "SUPPORT_PARITY_GRID_MAX_BYTES_FLOOR",
            "SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE",
            "SUPPORT_SPECIALIZED_MAX_GATE_OPERATIONS",
            "SUPPORT_GATE_MAX_WORK_PER_BATCH_UNIT",
            "SUPPORT_SPECIALIZED_MAX_LOAD_OPERATIONS",
            "SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT",
        ]
    }

    #[must_use]
    pub const fn knn_max_work_floor(&self) -> &'a Placeholder<std::num::NonZeroU64> {
        self.knn_max_work_floor
    }

    #[must_use]
    pub const fn knn_max_work_per_face(&self) -> &'a Derived<u64> {
        self.knn_max_work_per_face
    }

    #[must_use]
    pub const fn parity_grid_max_bytes_floor(&self) -> &'a Placeholder<std::num::NonZeroU64> {
        self.parity_grid_max_bytes_floor
    }

    #[must_use]
    pub const fn parity_grid_max_bytes_per_face(&self) -> &'a Derived<u64> {
        self.parity_grid_max_bytes_per_face
    }

    /// The specialized gate operation floor: the whole cap for every model
    /// whose batch product is small enough that the coefficient does not reach
    /// it, which is every corpus model.
    #[must_use]
    pub const fn gate_max_operations_floor(&self) -> &'a Placeholder<std::num::NonZeroU64> {
        self.gate_max_operations_floor
    }

    #[must_use]
    pub const fn gate_max_work_per_batch_unit(&self) -> &'a Derived<u64> {
        self.gate_max_work_per_batch_unit
    }

    /// The specialized load operation floor, on the same terms as
    /// [`Self::gate_max_operations_floor`].
    #[must_use]
    pub const fn load_max_operations_floor(&self) -> &'a Placeholder<std::num::NonZeroU64> {
        self.load_max_operations_floor
    }

    #[must_use]
    pub const fn load_max_work_per_batch_unit(&self) -> &'a Derived<u64> {
        self.load_max_work_per_batch_unit
    }
}

impl M0Constants {
    fn build() -> Result<Self, M0ConstantsError> {
        let evidence = M0EvidenceConstants::new()?;
        let placeholders = M0PlaceholderConstants::new()?;

        // Gather every declaration before one structural batch validation and
        // publication. Do not register constants incrementally.
        let mut declarations = Vec::with_capacity(264);
        declarations.extend(evidence.descriptors()?);
        declarations.extend(placeholders.descriptors()?);
        let registry = ConstantsRegistry::build_batch(declarations)?;

        let saturn4_ultra = PrinterProfile::new(
            "saturn4-ultra",
            evidence.saturn_build_volume_mm().clone(),
            evidence.pixel_pitch_x_um().clone(),
            evidence.pixel_pitch_y_um().clone(),
            evidence.layer_height_default_mm().clone(),
            Placeholder::new(
                placeholders.lift_mm().diagnostic().get(),
                placeholders.lift_mm().note.clone(),
            ),
            evidence.saturn_leading_release_edge().clone(),
            evidence.saturn_release_front_sweep_direction().clone(),
            evidence.saturn_release_film_thickness_mm().clone(),
        )?;

        Ok(Self {
            registry,
            evidence,
            placeholders,
            saturn4_ultra,
        })
    }

    #[must_use]
    pub const fn registry(&self) -> &ConstantsRegistry {
        &self.registry
    }

    /// The immutable, typed Placeholder component that backs this atomic M0
    /// publication. Consumers may read diagnostic/search settings from it,
    /// but its wrapper still structurally bars those settings from acceptance.
    #[must_use]
    pub const fn placeholders(&self) -> &M0PlaceholderConstants {
        &self.placeholders
    }

    #[must_use]
    pub const fn saturn4_ultra(&self) -> &PrinterProfile {
        &self.saturn4_ultra
    }

    /// Returns the SCAR-AREA-CAP / PRINT-FLOOR authority without copying its
    /// registry entries or inventing defaults for caller-supplied geometry.
    #[must_use]
    pub const fn scar_area_cap_policy(&self) -> ScarAreaCapPolicy<'_> {
        ScarAreaCapPolicy {
            a_abs_mm2: self.placeholders.a_abs_mm2(),
            kappa: self.placeholders.kappa(),
            l_scar_max_mm: self.placeholders.l_scar_max_mm(),
            eps_cos: self.placeholders.eps_cos(),
            interval_tol_mm: self.placeholders.tol_mm(),
            print_floor_chord_mm: self.evidence.print_floor_chord_mm(),
            tip_ball_factor: self.placeholders.ball_factor_stub(),
            foot_ball_factor: self.placeholders.ball_factor_foot(),
        }
    }

    /// Returns the SUPPORT-RESOURCE-CAP authority without copying its registry
    /// entries or inventing a second numeric owner in the support crate.
    #[must_use]
    pub const fn support_resource_cap_policy(&self) -> SupportResourceCapPolicy<'_> {
        SupportResourceCapPolicy {
            knn_max_work_floor: self.placeholders.support_knn_max_work_floor(),
            knn_max_work_per_face: self.evidence.support_knn_max_work_per_face(),
            parity_grid_max_bytes_floor: self.placeholders.support_parity_grid_max_bytes_floor(),
            parity_grid_max_bytes_per_face: self.evidence.support_parity_grid_max_bytes_per_face(),
            gate_max_operations_floor: self.placeholders.support_specialized_max_gate_operations(),
            gate_max_work_per_batch_unit: self.evidence.support_gate_max_work_per_batch_unit(),
            load_max_operations_floor: self.placeholders.support_specialized_max_load_operations(),
            load_max_work_per_batch_unit: self.evidence.support_load_max_work_per_batch_unit(),
        }
    }
}

/// The sole workspace-wide global constants initialization point.
///
/// Its fallible result is itself published atomically: callers observe either
/// the complete 264-entry registry and profile or the same construction error.
static M0_CONSTANTS: LazyLock<Result<M0Constants, M0ConstantsError>> =
    LazyLock::new(M0Constants::build);

/// Returns the complete M0 constants registry and printer profile.
pub fn m0_constants() -> Result<&'static M0Constants, &'static M0ConstantsError> {
    M0_CONSTANTS.as_ref()
}

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
constant_value_conversion!(MicronGranularity, |value: &MicronGranularity| {
    Ok(ConstantValue::unsigned_integer(u64::from(value.get())))
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
/// The current M0 publication is one `LazyLock<Result<M0Constants,
/// M0ConstantsError>>`: it gathers all declarations, calls `build_batch` once,
/// and then exposes the complete registry or one construction error. This
/// structural type deliberately has no `Default`, incremental registration, or
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
