//! Shared, side-effect-free diagnostic support-member sizing primitives.
//!
//! This module ports the frozen scalar mechanics leaves once for planning and
//! compatibility reporting. It is deliberately not a production sizing
//! authority: the material/serviceability values remain M0 [`Placeholder`]
//! values, and no evidence-backed production lateral-load derivation exists
//! yet. Consequently, every radius influenced by those values retains an
//! explicit `Diagnostic` type and cannot be converted here into forest end
//! radii. Pure geometry such as frustum volume stays outside that tainted path.

use plinth_core::{m0_constants, Finite, NonNegativeFinite, Placeholder, PositiveFinite};
use thiserror::Error;

use crate::forest::{MemberKind, RootwardEndRadius, TerminalEndRadius};

const MILLIMETRES_PER_METRE: f64 = 1_000.0;
const SQUARE_METRES_PER_SQUARE_MILLIMETRE: f64 = 1e-6;
const PASCALS_PER_MEGAPASCAL: f64 = 1e6;
const PASCALS_PER_GIGAPASCAL: f64 = 1e9;
const FROZEN_LEAN_BRANCH_DEG: f64 = 1e-9;

/// Failure to construct or evaluate a finite diagnostic sizing primitive.
#[derive(Clone, Debug, PartialEq, Error)]
pub enum MemberSizingError {
    #[error("registered M0 constants are unavailable: {0}")]
    RegisteredConstantsUnavailable(String),
    #[error("member span endpoints must be distinct")]
    DegenerateSpan,
    #[error("lean angle must be between zero and 90 degrees inclusive")]
    LeanAngleOutOfRange,
    #[error("sizing arithmetic produced a non-finite result")]
    NonFiniteResult,
}

/// The complete Placeholder-tainted policy used by frozen diagnostic sizing.
///
/// The wrappers and their notes remain present in this value. There is no
/// conversion to an acceptance-safe policy.
#[derive(Clone, Debug, PartialEq)]
pub struct DiagnosticMemberSizingPolicy {
    peel_pressure_pa: Placeholder<PositiveFinite>,
    tensile_green_mpa: Placeholder<PositiveFinite>,
    tip_safety_factor: Placeholder<PositiveFinite>,
    modulus_green_gpa: Placeholder<PositiveFinite>,
    max_deflection_mm: Placeholder<PositiveFinite>,
    min_shaft_radius_mm: Placeholder<PositiveFinite>,
    min_shoot_radius_mm: Placeholder<PositiveFinite>,
    min_trunk_radius_mm: Placeholder<PositiveFinite>,
    branch_angle_deg: Placeholder<PositiveFinite>,
    max_segment_angle_deg: Placeholder<PositiveFinite>,
    cone_tolerance_deg: Placeholder<PositiveFinite>,
}

impl DiagnosticMemberSizingPolicy {
    /// Clones the registered Placeholder wrappers without promoting them.
    pub fn current() -> Result<Self, MemberSizingError> {
        let constants = m0_constants().map_err(|error| {
            MemberSizingError::RegisteredConstantsUnavailable(error.to_string())
        })?;
        let policy = constants.placeholders();
        Ok(Self {
            peel_pressure_pa: policy.peel_pressure_pa().clone(),
            tensile_green_mpa: policy.tensile_green_mpa().clone(),
            tip_safety_factor: policy.tip_safety_factor().clone(),
            modulus_green_gpa: policy.modulus_green_gpa().clone(),
            max_deflection_mm: policy.max_deflection_mm().clone(),
            min_shaft_radius_mm: policy.min_shaft_r_mm().clone(),
            min_shoot_radius_mm: policy.min_shoot_r_mm().clone(),
            min_trunk_radius_mm: policy.min_trunk_r_mm().clone(),
            branch_angle_deg: policy.branch_angle_deg().clone(),
            max_segment_angle_deg: policy.max_segment_angle_deg().clone(),
            cone_tolerance_deg: policy.cco_cone_tolerance_deg().clone(),
        })
    }
}

/// An actual side load with evidence owned by the future support-load module.
///
/// There is intentionally no public constructor, scalar accessor, `Default`,
/// or serde implementation. Public callers cannot recreate the retired
/// constant `shaft_drag_force_n = 0.01 N`.
///
/// ```compile_fail
/// use plinth_core::NonNegativeFinite;
/// use plinth_support::member_sizing::ActualLateralLoad;
/// let force = NonNegativeFinite::new(0.01).unwrap();
/// let _ = ActualLateralLoad::from_tip_load_path_and_moment_arm(force);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct ActualLateralLoad {
    force_n: NonNegativeFinite,
    evidence: ActualLateralLoadEvidence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ActualLateralLoadEvidence {
    TipLoadPathAndMomentArm,
    /// O-10's sole compatibility exception.  This is intentionally
    /// crate-private so the frozen value cannot become a general load input.
    LegacyCompatibilityO10,
    #[cfg(test)]
    LegacyFrozenCompatibilityVector,
}

impl ActualLateralLoad {
    /// Future crate-internal entry point for the load module after it derives
    /// side force from the actual tip-load path and moment arm.
    ///
    /// No such production derivation exists yet, so this remains unused and
    /// cannot be called by downstream crates.
    #[allow(dead_code)]
    pub(crate) fn from_tip_load_path_and_moment_arm(force_n: NonNegativeFinite) -> Self {
        Self {
            force_n,
            evidence: ActualLateralLoadEvidence::TipLoadPathAndMomentArm,
        }
    }

    /// The frozen 0.01 N shaft-drag value admitted only by the O-10
    /// compatibility boundary.  It is not an actual measured lateral load,
    /// and this crate-private constructor prevents new algorithms or
    /// downstream callers from selecting it as a general-purpose input.
    pub(crate) fn legacy_compatibility_o10_frozen_shaft_drag(
        shaft_drag_force_n: &Placeholder<NonNegativeFinite>,
    ) -> Self {
        Self {
            force_n: *shaft_drag_force_n.diagnostic(),
            evidence: ActualLateralLoadEvidence::LegacyCompatibilityO10,
        }
    }

    #[cfg(test)]
    fn legacy_frozen_001n_fixture() -> Self {
        Self {
            force_n: NonNegativeFinite::new(0.01).expect("finite legacy vector"),
            evidence: ActualLateralLoadEvidence::LegacyFrozenCompatibilityVector,
        }
    }
}

/// A validated chord lean produced by member geometry.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct LeanAngleDeg(Finite);

impl LeanAngleDeg {
    pub fn new(value: Finite) -> Result<Self, MemberSizingError> {
        if (0.0..=90.0).contains(&value.get()) {
            Ok(Self(value))
        } else {
            Err(MemberSizingError::LeanAngleOutOfRange)
        }
    }

    #[must_use]
    pub const fn get(self) -> Finite {
        self.0
    }
}

/// A nondegenerate straight member represented by finite endpoints.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MemberSpan {
    start_mm: [Finite; 3],
    end_mm: [Finite; 3],
    length_mm: PositiveFinite,
    lean_angle_deg: LeanAngleDeg,
}

impl MemberSpan {
    /// Derives true 3-D length and the frozen chord lean from endpoints.
    pub fn new(start_mm: [Finite; 3], end_mm: [Finite; 3]) -> Result<Self, MemberSizingError> {
        let dx = end_mm[0].get() - start_mm[0].get();
        let dy = end_mm[1].get() - start_mm[1].get();
        let dz = end_mm[2].get() - start_mm[2].get();
        let horizontal_mm = dx.hypot(dy);
        let length_mm = horizontal_mm.hypot(dz);
        if !length_mm.is_finite() {
            return Err(MemberSizingError::NonFiniteResult);
        }
        let length_mm =
            PositiveFinite::new(length_mm).map_err(|_| MemberSizingError::DegenerateSpan)?;
        let lean = if length_mm.get() < FROZEN_LEAN_BRANCH_DEG {
            0.0
        } else {
            horizontal_mm.atan2(dz.abs()).to_degrees()
        };
        let lean = Finite::new(lean).map_err(|_| MemberSizingError::NonFiniteResult)?;
        Ok(Self {
            start_mm,
            end_mm,
            length_mm,
            lean_angle_deg: LeanAngleDeg::new(lean)?,
        })
    }

    #[must_use]
    pub const fn start_mm(self) -> [Finite; 3] {
        self.start_mm
    }

    #[must_use]
    pub const fn end_mm(self) -> [Finite; 3] {
        self.end_mm
    }

    #[must_use]
    pub const fn length_mm(self) -> PositiveFinite {
        self.length_mm
    }

    #[must_use]
    pub const fn lean_angle_deg(self) -> LeanAngleDeg {
        self.lean_angle_deg
    }
}

/// Placeholder-tainted, unclamped tension radius for accumulated carried area.
#[derive(Clone, Debug, PartialEq)]
pub struct DiagnosticPeelTensionRadiusMm(Placeholder<NonNegativeFinite>);

impl DiagnosticPeelTensionRadiusMm {
    #[must_use]
    pub fn diagnostic(&self) -> NonNegativeFinite {
        *self.0.diagnostic()
    }

    #[must_use]
    pub const fn provenance(&self) -> &Placeholder<NonNegativeFinite> {
        &self.0
    }
}

/// Pure fourth-power fan-in radius from already-realized child geometry.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct MurrayRadiusMm(NonNegativeFinite);

impl MurrayRadiusMm {
    #[must_use]
    pub const fn get(self) -> NonNegativeFinite {
        self.0
    }
}

/// Placeholder-tainted deflection-required radius.
#[derive(Clone, Debug, PartialEq)]
pub struct DiagnosticDeflectionRadiusMm(Placeholder<PositiveFinite>);

impl DiagnosticDeflectionRadiusMm {
    #[must_use]
    pub fn diagnostic(&self) -> PositiveFinite {
        *self.0.diagnostic()
    }

    #[must_use]
    pub const fn provenance(&self) -> &Placeholder<PositiveFinite> {
        &self.0
    }
}

/// Placeholder-tainted kind radius floor.
#[derive(Clone, Debug, PartialEq)]
pub struct DiagnosticMemberKindFloorMm(Placeholder<PositiveFinite>);

impl DiagnosticMemberKindFloorMm {
    #[must_use]
    pub fn diagnostic(&self) -> PositiveFinite {
        *self.0.diagnostic()
    }
}

/// Diagnostic rootward radius; no conversion to `RootwardEndRadius` exists.
#[derive(Clone, Debug, PartialEq)]
pub struct DiagnosticRootwardEndRadiusMm(Placeholder<PositiveFinite>);

impl DiagnosticRootwardEndRadiusMm {
    #[must_use]
    pub fn diagnostic(&self) -> PositiveFinite {
        *self.0.diagnostic()
    }

    #[must_use]
    pub const fn provenance(&self) -> &Placeholder<PositiveFinite> {
        &self.0
    }
}

/// Diagnostic terminal radius; no conversion to `TerminalEndRadius` exists.
#[derive(Clone, Debug, PartialEq)]
pub struct DiagnosticTerminalEndRadiusMm(Placeholder<PositiveFinite>);

impl DiagnosticTerminalEndRadiusMm {
    #[must_use]
    pub fn diagnostic(&self) -> PositiveFinite {
        *self.0.diagnostic()
    }
}

/// A one-sided tapered result. The terminal end remains the caller's exact
/// tip-continuity radius; only the rootward end is diagnostically enlarged.
#[derive(Clone, Debug, PartialEq)]
pub struct DiagnosticTaperedMemberEndRadii {
    terminal_continuity: TerminalEndRadius,
    rootward: DiagnosticRootwardEndRadiusMm,
}

impl DiagnosticTaperedMemberEndRadii {
    #[must_use]
    pub const fn terminal_continuity(&self) -> TerminalEndRadius {
        self.terminal_continuity
    }

    #[must_use]
    pub const fn diagnostic_rootward(&self) -> &DiagnosticRootwardEndRadiusMm {
        &self.rootward
    }
}

/// Uniform diagnostic result for propagated ancestors only.
#[derive(Clone, Debug, PartialEq)]
pub struct DiagnosticUniformMemberEndRadii {
    terminal: DiagnosticTerminalEndRadiusMm,
    rootward: DiagnosticRootwardEndRadiusMm,
}

impl DiagnosticUniformMemberEndRadii {
    #[must_use]
    pub const fn diagnostic_terminal(&self) -> &DiagnosticTerminalEndRadiusMm {
        &self.terminal
    }

    #[must_use]
    pub const fn diagnostic_rootward(&self) -> &DiagnosticRootwardEndRadiusMm {
        &self.rootward
    }
}

/// Exact nonnegative frustum volume in millilitres.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct FrustumVolumeMl(NonNegativeFinite);

impl FrustumVolumeMl {
    #[must_use]
    pub const fn get(self) -> NonNegativeFinite {
        self.0
    }
}

/// Placeholder-tainted result of the frozen printability-cone policy.
#[derive(Clone, Debug, PartialEq)]
pub struct DiagnosticPrintabilityConeResult(Placeholder<bool>);

impl DiagnosticPrintabilityConeResult {
    #[must_use]
    pub fn diagnostic(&self) -> bool {
        *self.0.diagnostic()
    }

    #[must_use]
    pub const fn provenance(&self) -> &Placeholder<bool> {
        &self.0
    }
}

/// Direct diagnostic port of `cco._r_tension_mm`, without the tip-removal
/// clamp. The frozen `max(carried_area_mm2, 0)` remains exact.
pub fn diagnostic_peel_tension_radius_mm(
    carried_area_mm2: Finite,
    policy: &DiagnosticMemberSizingPolicy,
) -> Result<DiagnosticPeelTensionRadiusMm, MemberSizingError> {
    let area_m2 = carried_area_mm2.get().max(0.0) * SQUARE_METRES_PER_SQUARE_MILLIMETRE;
    let force_n = area_m2 * policy.peel_pressure_pa.diagnostic().get();
    let tensile_pa = policy.tensile_green_mpa.diagnostic().get() * PASCALS_PER_MEGAPASCAL;
    let radius_m = (force_n * policy.tip_safety_factor.diagnostic().get()
        / (std::f64::consts::PI * tensile_pa))
        .sqrt();
    let radius = nonnegative(radius_m * MILLIMETRES_PER_METRE)?;
    Ok(DiagnosticPeelTensionRadiusMm(Placeholder::new(
        radius,
        "Diagnostic frozen tension radius; depends on Placeholder peel pressure, tensile split, and safety factor.",
    )))
}

/// Computes the pure frozen fourth-power fan-in radius from child rootward ends.
pub fn murray_radius_mm(
    child_rootward_radii: &[RootwardEndRadius],
) -> Result<MurrayRadiusMm, MemberSizingError> {
    let sum = child_rootward_radii.iter().try_fold(0.0, |sum, radius| {
        let value = radius.get().get();
        let next = sum + value.powi(4);
        if next.is_finite() {
            Ok(next)
        } else {
            Err(MemberSizingError::NonFiniteResult)
        }
    })?;
    nonnegative(sum.powf(0.25)).map(MurrayRadiusMm)
}

/// Computes the frozen angle-aware cantilever radius for diagnostic use.
///
/// The exact `_shaft_r_for_L` branch is retained: six fixed-point updates are
/// performed only when `lean_deg > 1e-9`. The opaque load must come from the
/// future evidence-owning load module; public callers cannot manufacture it.
pub fn diagnostic_deflection_radius_mm(
    span: MemberSpan,
    lateral_load: &ActualLateralLoad,
    policy: &DiagnosticMemberSizingPolicy,
) -> Result<DiagnosticDeflectionRadiusMm, MemberSizingError> {
    diagnostic_deflection_radius_for_geometry(
        span.length_mm(),
        span.lean_angle_deg(),
        lateral_load,
        policy,
    )
}

fn diagnostic_deflection_radius_for_geometry(
    length_3d_mm: PositiveFinite,
    lean_angle_deg: LeanAngleDeg,
    lateral_load: &ActualLateralLoad,
    policy: &DiagnosticMemberSizingPolicy,
) -> Result<DiagnosticDeflectionRadiusMm, MemberSizingError> {
    let length_m = length_3d_mm.get() / MILLIMETRES_PER_METRE;
    let modulus_pa = policy.modulus_green_gpa.diagnostic().get() * PASCALS_PER_GIGAPASCAL;
    let max_deflection_m = policy.max_deflection_mm.diagnostic().get() / MILLIMETRES_PER_METRE;
    let geometry =
        4.0 * length_m.powi(3) / (3.0 * modulus_pa * std::f64::consts::PI * max_deflection_m);
    let mut radius_m = (geometry * lateral_load.force_n.get()).powf(0.25);
    if !radius_m.is_finite() || !geometry.is_finite() {
        return Err(MemberSizingError::NonFiniteResult);
    }

    // Frozen `_peel_tan` branches on degrees, not on the tangent's value.
    if lean_angle_deg.get().get() > FROZEN_LEAN_BRANCH_DEG {
        let tangent = lean_angle_deg.get().get().to_radians().tan();
        if !tangent.is_finite() {
            return Err(MemberSizingError::NonFiniteResult);
        }
        for _ in 0..6 {
            let force_n = lateral_load.force_n.get()
                + policy.peel_pressure_pa.diagnostic().get()
                    * std::f64::consts::PI
                    * radius_m.powi(2)
                    * tangent;
            radius_m = (geometry * force_n).powf(0.25);
            if !radius_m.is_finite() {
                return Err(MemberSizingError::NonFiniteResult);
            }
        }
    }

    let radius_mm =
        (radius_m * MILLIMETRES_PER_METRE).max(policy.min_shaft_radius_mm.diagnostic().get());
    let radius = positive(radius_mm)?;
    Ok(DiagnosticDeflectionRadiusMm(Placeholder::new(
        radius,
        "Diagnostic frozen deflection radius; depends on Placeholder modulus, deflection limit, shaft floor, and peel-pressure split.",
    )))
}

/// Returns the Placeholder-tainted frozen kind floor.
#[must_use]
pub fn diagnostic_member_kind_floor_mm(
    kind: &MemberKind,
    policy: &DiagnosticMemberSizingPolicy,
) -> DiagnosticMemberKindFloorMm {
    let source = if matches!(kind, MemberKind::Trunk) {
        &policy.min_trunk_radius_mm
    } else {
        &policy.min_shoot_radius_mm
    };
    DiagnosticMemberKindFloorMm(Placeholder::new(*source.diagnostic(), source.note.clone()))
}

/// One-sided tapered sizing: preserve p0 exactly and diagnostically compute
/// `p1 = max(max(Murray, tension, deflection, kind floor), p0)`.
pub fn diagnostic_required_tapered_member_end_radii(
    terminal_continuity: TerminalEndRadius,
    kind: &MemberKind,
    child_rootward_radii: &[RootwardEndRadius],
    carried_area_mm2: Finite,
    span: MemberSpan,
    lateral_load: &ActualLateralLoad,
    policy: &DiagnosticMemberSizingPolicy,
) -> Result<DiagnosticTaperedMemberEndRadii, MemberSizingError> {
    let required = diagnostic_required_radius(
        kind,
        child_rootward_radii,
        carried_area_mm2,
        span,
        lateral_load,
        policy,
    )?;
    let rootward = required.get().max(terminal_continuity.get().get());
    Ok(DiagnosticTaperedMemberEndRadii {
        terminal_continuity,
        rootward: diagnostic_rootward(positive(rootward)?),
    })
}

/// Uniform diagnostic sizing for propagated ancestors whose two ends must be
/// replaced together. This must not be used for a one-sided tip edge.
pub fn diagnostic_required_uniform_ancestor_end_radii(
    kind: &MemberKind,
    child_rootward_radii: &[RootwardEndRadius],
    carried_area_mm2: Finite,
    span: MemberSpan,
    lateral_load: &ActualLateralLoad,
    policy: &DiagnosticMemberSizingPolicy,
) -> Result<DiagnosticUniformMemberEndRadii, MemberSizingError> {
    let required = diagnostic_required_radius(
        kind,
        child_rootward_radii,
        carried_area_mm2,
        span,
        lateral_load,
        policy,
    )?;
    Ok(DiagnosticUniformMemberEndRadii {
        terminal: DiagnosticTerminalEndRadiusMm(Placeholder::new(
            required,
            "Diagnostic uniform ancestor radius; Placeholder-tainted and not a production terminal end.",
        )),
        rootward: diagnostic_rootward(required),
    })
}

fn diagnostic_required_radius(
    kind: &MemberKind,
    child_rootward_radii: &[RootwardEndRadius],
    carried_area_mm2: Finite,
    span: MemberSpan,
    lateral_load: &ActualLateralLoad,
    policy: &DiagnosticMemberSizingPolicy,
) -> Result<PositiveFinite, MemberSizingError> {
    let murray = murray_radius_mm(child_rootward_radii)?.get().get();
    let tension = diagnostic_peel_tension_radius_mm(carried_area_mm2, policy)?
        .diagnostic()
        .get();
    let deflection = diagnostic_deflection_radius_mm(span, lateral_load, policy)?
        .diagnostic()
        .get();
    let kind_floor = diagnostic_member_kind_floor_mm(kind, policy)
        .diagnostic()
        .get();
    // Preserve the frozen left-to-right max order.
    positive(murray.max(tension).max(deflection).max(kind_floor))
}

fn diagnostic_rootward(value: PositiveFinite) -> DiagnosticRootwardEndRadiusMm {
    DiagnosticRootwardEndRadiusMm(Placeholder::new(
        value,
        "Diagnostic rootward sizing result; Placeholder-tainted and not a production rootward end.",
    ))
}

/// Computes exact tapered-member volume from already-realized geometry.
pub fn frustum_volume_ml(
    span: MemberSpan,
    terminal_radius: TerminalEndRadius,
    rootward_radius: RootwardEndRadius,
) -> Result<FrustumVolumeMl, MemberSizingError> {
    let r0 = terminal_radius.get().get();
    let r1 = rootward_radius.get().get();
    let volume =
        std::f64::consts::PI / 3.0 * span.length_mm().get() * (r0.powi(2) + r0 * r1 + r1.powi(2))
            / MILLIMETRES_PER_METRE;
    nonnegative(volume).map(FrustumVolumeMl)
}

/// Evaluates the frozen Placeholder cone policy without creating an accept
/// verdict. The returned bool remains inside a `Diagnostic` result.
pub fn diagnostic_within_printability_cone(
    span: MemberSpan,
    worst_case_radius: PositiveFinite,
    policy: &DiagnosticMemberSizingPolicy,
) -> DiagnosticPrintabilityConeResult {
    let theta_max = if worst_case_radius.get() >= policy.min_shoot_radius_mm.diagnostic().get() {
        policy.branch_angle_deg.diagnostic().get()
    } else {
        policy.max_segment_angle_deg.diagnostic().get()
    };
    let within = span.lean_angle_deg().get().get()
        <= theta_max + policy.cone_tolerance_deg.diagnostic().get();
    DiagnosticPrintabilityConeResult(Placeholder::new(
        within,
        "Diagnostic frozen cone comparison; depends on Placeholder angle tiers, radius threshold, and tolerance.",
    ))
}

fn positive(value: f64) -> Result<PositiveFinite, MemberSizingError> {
    PositiveFinite::new(value).map_err(|_| MemberSizingError::NonFiniteResult)
}

fn nonnegative(value: f64) -> Result<NonNegativeFinite, MemberSizingError> {
    NonNegativeFinite::new(value).map_err(|_| MemberSizingError::NonFiniteResult)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn finite(value: f64) -> Finite {
        Finite::new(value).unwrap()
    }

    fn positive(value: f64) -> PositiveFinite {
        PositiveFinite::new(value).unwrap()
    }

    fn span(end: [f64; 3]) -> MemberSpan {
        MemberSpan::new([finite(0.0); 3], end.map(finite)).unwrap()
    }

    fn policy() -> DiagnosticMemberSizingPolicy {
        DiagnosticMemberSizingPolicy::current().unwrap()
    }

    fn legacy_load() -> ActualLateralLoad {
        ActualLateralLoad::legacy_frozen_001n_fixture()
    }

    fn close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= expected.abs().max(1.0) * 1e-12,
            "actual {actual:.17}, expected {expected:.17}"
        );
    }

    fn threshold_bits(angle_deg: f64) -> u64 {
        diagnostic_deflection_radius_for_geometry(
            positive(10.0),
            LeanAngleDeg::new(finite(angle_deg)).unwrap(),
            &legacy_load(),
            &policy(),
        )
        .unwrap()
        .diagnostic()
        .get()
        .to_bits()
    }

    #[test]
    fn frozen_lean_branch_is_bit_exact_at_zero_below_equal_and_above_threshold() {
        assert_eq!(threshold_bits(0.0), 0x3fdf_3703_56aa_60fb);
        assert_eq!(threshold_bits(0.5e-9), 0x3fdf_3703_56aa_60fb);
        assert_eq!(threshold_bits(1e-9), 0x3fdf_3703_56aa_60fb);
        assert_eq!(threshold_bits(1.000_001e-9), 0x3fdf_3703_56aa_828d);
    }

    #[test]
    fn impossible_lean_and_load_inputs_are_rejected_before_sizing() {
        assert_eq!(
            LeanAngleDeg::new(finite(-f64::EPSILON)),
            Err(MemberSizingError::LeanAngleOutOfRange)
        );
        assert_eq!(
            LeanAngleDeg::new(finite(90.0 + f64::EPSILON * 128.0)),
            Err(MemberSizingError::LeanAngleOutOfRange)
        );
        assert!(Finite::new(f64::NAN).is_err());
        assert!(NonNegativeFinite::new(-0.01).is_err());
    }

    #[test]
    fn frozen_vectors_cover_tension_deflection_and_one_sided_kind_floors() {
        let policy = policy();
        close(
            diagnostic_peel_tension_radius_mm(finite(30.0), &policy)
                .unwrap()
                .diagnostic()
                .get(),
            0.892_062_058_076_385_5,
        );
        assert_eq!(
            diagnostic_peel_tension_radius_mm(finite(-7.0), &policy)
                .unwrap()
                .diagnostic()
                .get()
                .to_bits(),
            0.0_f64.to_bits()
        );

        let vertical = span([0.0, 0.0, 10.0]);
        close(
            diagnostic_deflection_radius_mm(vertical, &legacy_load(), &policy)
                .unwrap()
                .diagnostic()
                .get(),
            0.487_732_729_563_063_3,
        );
        close(
            diagnostic_deflection_radius_mm(span([10.0, 0.0, 10.0]), &legacy_load(), &policy)
                .unwrap()
                .diagnostic()
                .get(),
            0.694_652_122_240_129_7,
        );

        let child = RootwardEndRadius::new(positive(0.5));
        let tapered = diagnostic_required_tapered_member_end_radii(
            TerminalEndRadius::new(positive(0.2)),
            &MemberKind::Shoot,
            &[child],
            finite(30.0),
            vertical,
            &legacy_load(),
            &policy,
        )
        .unwrap();
        close(tapered.terminal_continuity().get().get(), 0.2);
        close(
            tapered.diagnostic_rootward().diagnostic().get(),
            0.892_062_058_076_385_5,
        );

        let uniform = diagnostic_required_uniform_ancestor_end_radii(
            &MemberKind::Shoot,
            &[],
            finite(0.0),
            span([50.0, 0.0, 50.0]),
            &legacy_load(),
            &policy,
        )
        .unwrap();
        close(
            uniform.diagnostic_terminal().diagnostic().get(),
            4.422_534_369_337_705,
        );
        close(
            uniform.diagnostic_rootward().diagnostic().get(),
            4.422_534_369_337_705,
        );

        let trunk_floor = diagnostic_required_tapered_member_end_radii(
            TerminalEndRadius::new(positive(0.2)),
            &MemberKind::Trunk,
            &[],
            finite(0.0),
            vertical,
            &ActualLateralLoad {
                force_n: NonNegativeFinite::new(0.0).unwrap(),
                evidence: ActualLateralLoadEvidence::LegacyFrozenCompatibilityVector,
            },
            &policy,
        )
        .unwrap();
        close(trunk_floor.terminal_continuity().get().get(), 0.2);
        close(trunk_floor.diagnostic_rootward().diagnostic().get(), 0.8);
    }

    #[test]
    fn frozen_vectors_cover_volume_cone_boundary_and_toed_slab_length() {
        let policy = policy();
        let frustum = frustum_volume_ml(
            span([3.0, 4.0, 12.0]),
            TerminalEndRadius::new(positive(0.2)),
            RootwardEndRadius::new(positive(0.8)),
        )
        .unwrap();
        close(frustum.get().get(), 0.011_435_397_259_066_848);

        let boundary = span([1.0, 0.0, 1.0]);
        assert!(diagnostic_within_printability_cone(boundary, positive(0.3), &policy).diagnostic());
        assert!(
            diagnostic_within_printability_cone(boundary, positive(0.35), &policy).diagnostic()
        );
        assert!(!diagnostic_within_printability_cone(
            span([1.0, 0.0, 0.999_999]),
            positive(0.3),
            &policy,
        )
        .diagnostic());

        let toed = span([30.0, 0.0, 40.0]);
        close(toed.length_mm().get(), 50.0);
        close(toed.lean_angle_deg().get().get(), 36.869_897_645_844_02);
        close(
            diagnostic_deflection_radius_mm(toed, &legacy_load(), &policy)
                .unwrap()
                .diagnostic()
                .get(),
            2.475_902_583_196_808_7,
        );
    }

    #[test]
    fn rejects_degenerate_span_without_a_micro_length_clamp() {
        assert_eq!(
            MemberSpan::new([finite(0.0); 3], [finite(0.0); 3]),
            Err(MemberSizingError::DegenerateSpan)
        );
    }

    #[test]
    fn scalar_vectors_are_identical_in_real_thread_setting_subprocesses() {
        if std::env::var_os("PLINTH_MEMBER_SIZING_SUBPROCESS").is_some() {
            let policy = policy();
            let vector = [
                diagnostic_peel_tension_radius_mm(finite(30.0), &policy)
                    .unwrap()
                    .diagnostic()
                    .get(),
                diagnostic_deflection_radius_mm(span([50.0, 0.0, 50.0]), &legacy_load(), &policy)
                    .unwrap()
                    .diagnostic()
                    .get(),
            ];
            println!(
                "MEMBER_SIZING_BITS={:016x},{:016x}",
                vector[0].to_bits(),
                vector[1].to_bits()
            );
            return;
        }

        let test_name = "member_sizing::tests::scalar_vectors_are_identical_in_real_thread_setting_subprocesses";
        let mut outputs = Vec::new();
        for thread_count in ["1", "4", "16"] {
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", test_name, "--nocapture"])
                .env("PLINTH_MEMBER_SIZING_SUBPROCESS", "1")
                .env("PLINTH_THREADS", thread_count)
                .output()
                .unwrap();
            assert!(output.status.success());
            let stdout = String::from_utf8(output.stdout).unwrap();
            outputs.push(
                stdout
                    .lines()
                    .find(|line| line.starts_with("MEMBER_SIZING_BITS="))
                    .unwrap()
                    .to_owned(),
            );
        }
        assert!(outputs.windows(2).all(|pair| pair[0] == pair[1]));
    }
}
