//! Raw, bounded final-member transit evidence.
//!
//! This is intentionally a measurement boundary only.  It neither classifies
//! an interval nor assigns a clearance, gate, report, certificate, or defect.

use geom_kernel::{capsule_rays, Interval, PenetrationBatchLimits, SurfaceTriangleSoupDigest};
use plinth_core::{m0_constants, ConstantDescriptor, ConstantId, DecisionId, M0Constants};
use thiserror::Error;

use crate::{
    forest::{EmbedEnd, MemberKey, MemberKind},
    runner::{FinalLiveMemberEvidence, SupportVerificationSnapshot},
};

const OFFSET_RAYS: usize = 8;

/// Immutable policy material recorded by the scalar SCAR diagnostic.
///
/// This is deliberately not a gate policy: it copies the canonical registry
/// descriptors so a later owner can inspect exactly which unresolved policy
/// values and measured print floor informed this diagnostic calculation.
#[derive(Clone, Debug, PartialEq)]
struct TransitPolicySnapshot {
    decision_id: DecisionId,
    policy_descriptors: [ConstantDescriptor; 8],
    min_illegal_descriptor: ConstantDescriptor,
    a_abs_mm2: f64,
    kappa: f64,
    l_scar_max_mm: f64,
    eps_cos: f64,
    interval_tol_mm: f64,
    tip_ball_factor: f64,
    foot_ball_factor: f64,
    min_illegal_len_mm: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
enum TransitPolicySnapshotError {
    #[error("SCAR policy registry descriptor is absent: {id}")]
    MissingDescriptor { id: &'static str },
}

impl TransitPolicySnapshot {
    fn from_constants(constants: &M0Constants) -> Result<Self, TransitPolicySnapshotError> {
        let policy = constants.scar_area_cap_policy();
        let ids = policy.constant_ids();
        let policy_descriptors = ids.map(|id| {
            constants
                .registry()
                .get(&ConstantId::from(id))
                .cloned()
                .ok_or(TransitPolicySnapshotError::MissingDescriptor { id })
        });
        let policy_descriptors = policy_descriptors
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let policy_descriptors: [ConstantDescriptor; 8] = policy_descriptors
            .try_into()
            .expect("the fixed SCAR policy ID list always has eight entries");
        let min_id = "MIN_ILLEGAL_LEN_MM";
        let min_illegal_descriptor = constants
            .registry()
            .get(&ConstantId::from(min_id))
            .cloned()
            .ok_or(TransitPolicySnapshotError::MissingDescriptor { id: min_id })?;

        Ok(Self {
            decision_id: policy.decision_id(),
            policy_descriptors,
            min_illegal_descriptor,
            a_abs_mm2: policy.a_abs_mm2().diagnostic().get(),
            kappa: policy.kappa().diagnostic().get(),
            l_scar_max_mm: policy.l_scar_max_mm().diagnostic().get(),
            eps_cos: policy.eps_cos().diagnostic().get(),
            interval_tol_mm: policy.interval_tol_mm().diagnostic().get(),
            tip_ball_factor: policy.tip_ball_factor().diagnostic().get(),
            foot_ball_factor: policy.foot_ball_factor().diagnostic().get(),
            min_illegal_len_mm: constants
                .placeholders()
                .min_illegal_len_mm()
                .diagnostic()
                .get(),
        })
    }
}

/// Checked scalar evidence for a single, already-anchored transit sample.
#[derive(Clone, Debug, PartialEq)]
struct ScarDiagnosticInput {
    t_mm: f64,
    d_mm: f64,
    cosang: Option<f64>,
    raw_anchor_radius_mm: f64,
    tip_r_min_mm: f64,
    root_radius_mm: f64,
    member_length_mm: f64,
    member_kind: MemberKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
enum ScarDiagnosticError {
    #[error("SCAR diagnostic input is non-finite: {field}")]
    NonFinite { field: &'static str },
    #[error("SCAR diagnostic input is negative: {field}")]
    Negative { field: &'static str },
    #[error("SCAR diagnostic member length is collapsed")]
    CollapsedLength,
    #[error("SCAR diagnostic needs cosine evidence when T exceeds tolerance")]
    MissingCosine,
    #[error("SCAR diagnostic arithmetic became non-finite: {operation}")]
    Arithmetic { operation: &'static str },
}

impl ScarDiagnosticInput {
    #[allow(clippy::too_many_arguments)]
    fn new(
        t_mm: f64,
        d_mm: f64,
        cosang: Option<f64>,
        raw_anchor_radius_mm: f64,
        tip_r_min_mm: f64,
        root_radius_mm: f64,
        member_length_mm: f64,
        member_kind: MemberKind,
    ) -> Result<Self, ScarDiagnosticError> {
        for (field, value) in [
            ("T", t_mm),
            ("d", d_mm),
            ("raw anchor radius", raw_anchor_radius_mm),
            ("tip_r_min", tip_r_min_mm),
            ("root radius", root_radius_mm),
            ("member length", member_length_mm),
        ] {
            scalar_input(field, value)?;
        }
        if member_length_mm == 0.0 {
            return Err(ScarDiagnosticError::CollapsedLength);
        }
        if let Some(cosang) = cosang {
            scalar_input("cosang", cosang)?;
        }
        Ok(Self {
            t_mm,
            d_mm,
            cosang,
            raw_anchor_radius_mm,
            tip_r_min_mm,
            root_radius_mm,
            member_length_mm,
            member_kind,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScarDiagnostic {
    r_x_mm: f64,
    ball_radius_mm: f64,
    area_shaft_mm2: f64,
    area_ball_mm2: f64,
    area_patch_mm2: f64,
    area_design_mm2: f64,
    area_max_mm2: f64,
    scar_length_mm: f64,
    anchor_area_mm2: f64,
    neck_disc_area_mm2: f64,
    area_within_recorded_policy: bool,
    length_within_recorded_policy: bool,
    anchor_meets_recorded_policy: bool,
    diagnostic_constraints_hold_under_recorded_policy: bool,
}

fn scalar_input(field: &'static str, value: f64) -> Result<(), ScarDiagnosticError> {
    if !value.is_finite() {
        Err(ScarDiagnosticError::NonFinite { field })
    } else if value < 0.0 {
        Err(ScarDiagnosticError::Negative { field })
    } else {
        Ok(())
    }
}

fn finite(value: f64, operation: &'static str) -> Result<f64, ScarDiagnosticError> {
    value
        .is_finite()
        .then_some(value)
        .ok_or(ScarDiagnosticError::Arithmetic { operation })
}

fn calculate_scar_diagnostic(
    policy: &TransitPolicySnapshot,
    input: &ScarDiagnosticInput,
) -> Result<ScarDiagnostic, ScarDiagnosticError> {
    let r_end = input.raw_anchor_radius_mm.max(input.tip_r_min_mm);
    let taper = finite(input.t_mm / input.member_length_mm, "taper fraction")?.clamp(0.0, 1.0);
    let unconstrained_radius = finite(
        r_end + finite((input.root_radius_mm - r_end) * taper, "taper delta")?,
        "taper radius",
    )?;
    let r_x_mm = unconstrained_radius.clamp(
        r_end.min(input.root_radius_mm),
        r_end.max(input.root_radius_mm),
    );
    let ball_factor = if matches!(&input.member_kind, MemberKind::Stub) {
        policy.tip_ball_factor
    } else {
        policy.foot_ball_factor
    };
    let ball_radius_mm = finite(r_end * ball_factor, "ball radius")?;
    let exceeds_tol = input.t_mm > policy.interval_tol_mm;
    let cosine = if exceeds_tol {
        input.cosang.ok_or(ScarDiagnosticError::MissingCosine)?
    } else {
        0.0
    };
    let cosine_denominator = cosine.max(policy.eps_cos);
    let radius_squared = finite(r_x_mm * r_x_mm, "radius squared")?;
    let area_shaft_mm2 = if exceeds_tol {
        finite(
            (std::f64::consts::PI * radius_squared / cosine_denominator)
                .min(2.0 * r_x_mm * input.t_mm),
            "shaft area",
        )?
    } else {
        0.0
    };
    let ball_squared = finite(ball_radius_mm * ball_radius_mm, "ball radius squared")?;
    let area_ball_mm2 = finite(
        std::f64::consts::PI * (ball_squared - input.d_mm * input.d_mm).max(0.0),
        "ball area",
    )?;
    let area_patch_mm2 = finite(area_shaft_mm2 + area_ball_mm2, "patch area")?;
    let area_design_mm2 = finite(std::f64::consts::PI * ball_squared, "design area")?;
    let area_max_mm2 = policy
        .a_abs_mm2
        .max(finite(policy.kappa * area_design_mm2, "area cap")?);
    let scar_length_mm = if exceeds_tol {
        finite(
            (2.0 * r_x_mm / cosine_denominator).min(input.t_mm + 2.0 * r_x_mm),
            "scar length",
        )?
    } else {
        0.0
    };
    let anchor_area_mm2 = finite(
        area_ball_mm2
            + if exceeds_tol {
                std::f64::consts::PI * radius_squared
            } else {
                0.0
            },
        "anchor area",
    )?;
    let neck_disc_area_mm2 = finite(std::f64::consts::PI * r_end * r_end, "neck disc area")?;
    let (area_within_recorded_policy, length_within_recorded_policy, anchor_meets_recorded_policy) =
        recorded_constraints(
            policy,
            area_patch_mm2,
            scar_length_mm,
            anchor_area_mm2,
            area_design_mm2,
            neck_disc_area_mm2,
        );
    Ok(ScarDiagnostic {
        r_x_mm,
        ball_radius_mm,
        area_shaft_mm2,
        area_ball_mm2,
        area_patch_mm2,
        area_design_mm2,
        area_max_mm2,
        scar_length_mm,
        anchor_area_mm2,
        neck_disc_area_mm2,
        area_within_recorded_policy,
        length_within_recorded_policy,
        anchor_meets_recorded_policy,
        diagnostic_constraints_hold_under_recorded_policy: area_within_recorded_policy
            && length_within_recorded_policy
            && anchor_meets_recorded_policy,
    })
}

fn recorded_constraints(
    policy: &TransitPolicySnapshot,
    area_patch_mm2: f64,
    scar_length_mm: f64,
    anchor_area_mm2: f64,
    area_design_mm2: f64,
    neck_disc_area_mm2: f64,
) -> (bool, bool, bool) {
    (
        area_patch_mm2 <= policy.a_abs_mm2.max(policy.kappa * area_design_mm2),
        scar_length_mm <= policy.l_scar_max_mm,
        anchor_area_mm2 >= neck_disc_area_mm2,
    )
}

#[cfg(test)]
thread_local! {
    static LIVE_CALLS: std::cell::Cell<LiveCalls> = const { std::cell::Cell::new(LiveCalls::ZERO) };
}

/// Test-only, thread-local witnesses for the allocations and kernel entries
/// owned by this module.  They deliberately do not observe Rayon workers or
/// participate in production control flow.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LiveCalls {
    pub(crate) member_allocations: usize,
    pub(crate) partition_allocations: usize,
    pub(crate) evidence_allocations: usize,
    pub(crate) local_row_allocations: usize,
    pub(crate) stub_kernel_calls: usize,
    pub(crate) nonstub_kernel_calls: usize,
}

#[cfg(test)]
impl LiveCalls {
    const ZERO: Self = Self {
        local_row_allocations: 0,
        member_allocations: 0,
        partition_allocations: 0,
        evidence_allocations: 0,
        stub_kernel_calls: 0,
        nonstub_kernel_calls: 0,
    };
}

/// Explicit caller-owned limits and sampling parameters for one raw transit
/// extraction.  No production defaults are supplied by this module.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TransitIntervalLimits {
    max_members: usize,
    max_stub_rows: usize,
    max_nonstub_rows: usize,
    max_retained_intervals: usize,
    max_logical_peak_bytes: usize,
    majority_step_mm: f64,
    cap: usize,
    stub_penetration: PenetrationBatchLimits,
    nonstub_penetration: PenetrationBatchLimits,
}

impl TransitIntervalLimits {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        max_members: usize,
        max_stub_rows: usize,
        max_nonstub_rows: usize,
        max_retained_intervals: usize,
        max_logical_peak_bytes: usize,
        majority_step_mm: f64,
        cap: usize,
        stub_penetration: PenetrationBatchLimits,
        nonstub_penetration: PenetrationBatchLimits,
    ) -> Result<Self, TransitIntervalError> {
        let limits = Self {
            max_members,
            max_stub_rows,
            max_nonstub_rows,
            max_retained_intervals,
            max_logical_peak_bytes,
            majority_step_mm,
            cap,
            stub_penetration,
            nonstub_penetration,
        };
        if max_members == 0
            || max_stub_rows == 0
            || max_nonstub_rows == 0
            || max_retained_intervals == 0
            || max_logical_peak_bytes == 0
            || cap == 0
            || !majority_step_mm.is_finite()
            || majority_step_mm <= 0.0
        {
            return Err(TransitIntervalError::InvalidLimits);
        }
        Ok(limits)
    }

    #[cfg(test)]
    pub(crate) fn with_test_scalar_cap(self, resource: &'static str, cap: usize) -> Self {
        match resource {
            "members" => Self {
                max_members: cap,
                ..self
            },
            "stub rows" => Self {
                max_stub_rows: cap,
                ..self
            },
            "nonstub rows" => Self {
                max_nonstub_rows: cap,
                ..self
            },
            "retained intervals" => Self {
                max_retained_intervals: cap,
                ..self
            },
            "logical peak bytes" => Self {
                max_logical_peak_bytes: cap,
                ..self
            },
            _ => panic!("unknown transit scalar cap: {resource}"),
        }
    }
}

/// Exact scalar resource accounting admitted before gathering query rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TransitIntervalResourcePlan {
    members: usize,
    stub_rows: usize,
    nonstub_rows: usize,
    retained_interval_capacity: usize,
    retained_evidence_bytes: usize,
    logical_peak_bytes: usize,
    stub_penetration: PenetrationBatchLimits,
    nonstub_penetration: PenetrationBatchLimits,
}

impl TransitIntervalResourcePlan {
    pub(crate) const fn members(self) -> usize {
        self.members
    }
    pub(crate) const fn stub_rows(self) -> usize {
        self.stub_rows
    }
    pub(crate) const fn nonstub_rows(self) -> usize {
        self.nonstub_rows
    }
    pub(crate) const fn retained_interval_capacity(self) -> usize {
        self.retained_interval_capacity
    }
    pub(crate) const fn retained_evidence_bytes(self) -> usize {
        self.retained_evidence_bytes
    }
    pub(crate) const fn logical_peak_bytes(self) -> usize {
        self.logical_peak_bytes
    }
    pub(crate) const fn stub_penetration(self) -> PenetrationBatchLimits {
        self.stub_penetration
    }
    pub(crate) const fn nonstub_penetration(self) -> PenetrationBatchLimits {
        self.nonstub_penetration
    }
}

/// One complete raw kernel interval, ordered independently of Rayon timing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TransitRayInterval {
    member: MemberKey,
    designed_end: Option<EmbedEnd>,
    ray: i32,
    s_a_mm: f64,
    s_b_mm: f64,
}

impl TransitRayInterval {
    pub(crate) const fn member(self) -> MemberKey {
        self.member
    }
    pub(crate) const fn designed_end(self) -> Option<EmbedEnd> {
        self.designed_end
    }
    pub(crate) const fn ray(self) -> i32 {
        self.ray
    }
    pub(crate) const fn s_a_mm(self) -> f64 {
        self.s_a_mm
    }
    pub(crate) const fn s_b_mm(self) -> f64 {
        self.s_b_mm
    }
}

/// Immutable raw transit batch bound to one verified snapshot and surface.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransitIntervalBatch {
    snapshot_digest: u64,
    member_digest: u64,
    surface_digest: SurfaceTriangleSoupDigest,
    resource_plan: TransitIntervalResourcePlan,
    intervals: Box<[TransitRayInterval]>,
}

impl TransitIntervalBatch {
    pub(crate) const fn snapshot_digest(&self) -> u64 {
        self.snapshot_digest
    }
    pub(crate) const fn member_digest(&self) -> u64 {
        self.member_digest
    }
    pub(crate) const fn surface_digest(&self) -> SurfaceTriangleSoupDigest {
        self.surface_digest
    }
    pub(crate) const fn resource_plan(&self) -> TransitIntervalResourcePlan {
        self.resource_plan
    }
    pub(crate) fn intervals(&self) -> &[TransitRayInterval] {
        &self.intervals
    }
}

/// Recorded policy material used solely to anchor and census raw intervals.
///
/// These remain separate descriptors even though the present diagnostic values
/// happen to agree: anchoring is governed by `TOL_MM`, while the post-anchor
/// population cutoff is governed by `MIN_ILLEGAL_LEN_MM`.
#[derive(Clone, Debug, PartialEq)]
struct TransitAnchorPolicySnapshot {
    interval_tol_descriptor: ConstantDescriptor,
    min_illegal_descriptor: ConstantDescriptor,
    interval_tol_mm: f64,
    min_illegal_len_mm: f64,
}

impl TransitAnchorPolicySnapshot {
    fn from_constants(constants: &M0Constants) -> Result<Self, TransitIntervalError> {
        let descriptor = |id| {
            constants
                .registry()
                .get(&ConstantId::from(id))
                .cloned()
                .ok_or(TransitIntervalError::PolicyBinding { id })
        };
        let interval_tol_descriptor = descriptor("TOL_MM")?;
        let min_illegal_descriptor = descriptor("MIN_ILLEGAL_LEN_MM")?;
        Ok(Self {
            interval_tol_mm: constants
                .scar_area_cap_policy()
                .interval_tol_mm()
                .diagnostic()
                .get(),
            min_illegal_len_mm: constants
                .placeholders()
                .min_illegal_len_mm()
                .diagnostic()
                .get(),
            interval_tol_descriptor,
            min_illegal_descriptor,
        })
    }
}

/// One immutable normalized member retained by the post-measurement anchor.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AnchoredTransitMember {
    member: MemberKey,
    designed_end: Option<EmbedEnd>,
    rays: Box<[AnchoredTransitRay]>,
}

impl AnchoredTransitMember {
    pub(crate) const fn member(&self) -> MemberKey {
        self.member
    }
    pub(crate) const fn designed_end(&self) -> Option<EmbedEnd> {
        self.designed_end
    }
    pub(crate) fn rays(&self) -> &[AnchoredTransitRay] {
        &self.rays
    }
}

/// One canonical ray and its complete raw crossing population.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AnchoredTransitRay {
    ray: i32,
    length_mm: f64,
    crossings: Box<[AnchoredTransitCrossing]>,
}

impl AnchoredTransitRay {
    pub(crate) const fn ray(&self) -> i32 {
        self.ray
    }
    pub(crate) const fn length_mm(&self) -> f64 {
        self.length_mm
    }
    pub(crate) fn crossings(&self) -> &[AnchoredTransitCrossing] {
        &self.crossings
    }
}

/// One raw interval, with only its local anchor and minimum-length disposition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AnchoredTransitCrossing {
    raw_index: usize,
    s_a_mm: f64,
    s_b_mm: f64,
    chord_mm: f64,
    is_tip_candidate: bool,
    below_minimum_length: bool,
}

impl AnchoredTransitCrossing {
    pub(crate) const fn raw_index(&self) -> usize {
        self.raw_index
    }
    pub(crate) const fn s_a_mm(&self) -> f64 {
        self.s_a_mm
    }
    pub(crate) const fn s_b_mm(&self) -> f64 {
        self.s_b_mm
    }
    pub(crate) const fn chord_mm(&self) -> f64 {
        self.chord_mm
    }
    pub(crate) const fn is_tip_candidate(&self) -> bool {
        self.is_tip_candidate
    }
    pub(crate) const fn below_minimum_length(&self) -> bool {
        self.below_minimum_length
    }
}

/// Immutable, full-resolution anchoring of one exact raw interval batch.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AnchoredTransitBatch {
    snapshot_digest: u64,
    member_digest: u64,
    surface_digest: SurfaceTriangleSoupDigest,
    policy: TransitAnchorPolicySnapshot,
    members: Box<[AnchoredTransitMember]>,
}

impl AnchoredTransitBatch {
    pub(crate) const fn snapshot_digest(&self) -> u64 {
        self.snapshot_digest
    }
    pub(crate) const fn member_digest(&self) -> u64 {
        self.member_digest
    }
    pub(crate) const fn surface_digest(&self) -> SurfaceTriangleSoupDigest {
        self.surface_digest
    }
    pub(crate) fn members(&self) -> &[AnchoredTransitMember] {
        &self.members
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub(crate) enum TransitIntervalError {
    #[error("transit interval limits are invalid")]
    InvalidLimits,
    #[error("transit interval snapshot binding is invalid")]
    SnapshotBinding,
    #[error("transit interval resource limit exceeded: {resource}")]
    ResourceLimit { resource: &'static str },
    #[error("transit interval arithmetic overflow: {resource}")]
    ArithmeticOverflow { resource: &'static str },
    #[error("geometry kernel refused transit intervals")]
    KernelRefusal,
    #[error("geometry kernel returned corrupt transit intervals")]
    CorruptKernelResult,
    #[error("transit anchor policy registry descriptor is absent: {id}")]
    PolicyBinding { id: &'static str },
    #[error("transit anchor batch is not bound to this snapshot")]
    AnchorBatchBinding,
    #[error("transit anchor member or ray evidence is corrupt")]
    AnchorEvidence,
}

#[derive(Clone, Copy)]
struct NormalizedMember<'a> {
    source: &'a FinalLiveMemberEvidence,
    p0: [f64; 3],
    p1: [f64; 3],
    r0: f64,
    r1: f64,
}

pub(crate) fn measure(
    snapshot: &SupportVerificationSnapshot<'_>,
    limits: TransitIntervalLimits,
) -> Result<TransitIntervalBatch, TransitIntervalError> {
    snapshot
        .validate_integrity()
        .map_err(|_| TransitIntervalError::SnapshotBinding)?;
    let mut member_count = 0usize;
    let mut stub_count = 0usize;
    for member in snapshot.final_live_members() {
        member_count =
            member_count
                .checked_add(1)
                .ok_or(TransitIntervalError::ArithmeticOverflow {
                    resource: "members",
                })?;
        if matches!(member.kind(), MemberKind::Stub) {
            stub_count =
                stub_count
                    .checked_add(1)
                    .ok_or(TransitIntervalError::ArithmeticOverflow {
                        resource: "stub rows",
                    })?;
        }
    }
    let plan = preflight(member_count, stub_count, limits)?;
    let mut members = Vec::new();
    members
        .try_reserve_exact(plan.members)
        .map_err(|_| TransitIntervalError::ResourceLimit {
            resource: "member rows allocation",
        })?;
    #[cfg(test)]
    record_live_call(|calls| calls.member_allocations += 1);
    members.extend(snapshot.final_live_members());
    members.sort_unstable_by_key(|member| (member.key(), member.embed_end()));

    let mut stubs = Vec::new();
    let mut nonstubs = Vec::new();
    stubs
        .try_reserve_exact(plan.stub_rows)
        .map_err(|_| TransitIntervalError::ResourceLimit {
            resource: "stub rows allocation",
        })?;
    #[cfg(test)]
    record_live_call(|calls| calls.partition_allocations += 1);
    nonstubs.try_reserve_exact(plan.nonstub_rows).map_err(|_| {
        TransitIntervalError::ResourceLimit {
            resource: "nonstub rows allocation",
        }
    })?;
    #[cfg(test)]
    record_live_call(|calls| calls.partition_allocations += 1);
    for member in members {
        let normalized = normalize(member);
        if matches!(member.kind(), MemberKind::Stub) {
            stubs.push(normalized);
        } else {
            nonstubs.push(normalized);
        }
    }

    let mut evidence = Vec::new();
    evidence
        .try_reserve_exact(plan.retained_interval_capacity)
        .map_err(|_| TransitIntervalError::ResourceLimit {
            resource: "retained intervals allocation",
        })?;
    #[cfg(test)]
    record_live_call(|calls| calls.evidence_allocations += 1);
    collect_batch(
        &mut evidence,
        snapshot.native_surface(),
        &stubs,
        false,
        limits,
    )?;
    collect_batch(
        &mut evidence,
        snapshot.native_surface(),
        &nonstubs,
        true,
        limits,
    )?;
    if evidence.len() > limits.max_retained_intervals {
        return Err(TransitIntervalError::ResourceLimit {
            resource: "retained intervals",
        });
    }
    evidence.sort_unstable_by(|left, right| {
        (left.member, left.designed_end, left.ray)
            .cmp(&(right.member, right.designed_end, right.ray))
            .then_with(|| left.s_a_mm.total_cmp(&right.s_a_mm))
            .then_with(|| left.s_b_mm.total_cmp(&right.s_b_mm))
    });
    Ok(TransitIntervalBatch {
        snapshot_digest: snapshot.digest(),
        member_digest: member_digest(snapshot.final_live_members()),
        surface_digest: snapshot.native_surface().source_triangle_soup_digest(),
        resource_plan: plan,
        intervals: evidence.into_boxed_slice(),
    })
}

/// Anchor and census an already-measured batch without issuing any surface
/// query.  The batch must be bound to this exact sealed snapshot, its final
/// members, its native surface, and its recorded resource plan.
pub(crate) fn anchor(
    snapshot: &SupportVerificationSnapshot<'_>,
    batch: &TransitIntervalBatch,
) -> Result<AnchoredTransitBatch, TransitIntervalError> {
    snapshot
        .validate_integrity()
        .map_err(|_| TransitIntervalError::AnchorBatchBinding)?;
    let member_count = snapshot.final_live_members().len();
    let stub_count = snapshot
        .final_live_members()
        .filter(|member| matches!(member.kind(), MemberKind::Stub))
        .count();
    if batch.snapshot_digest != snapshot.digest()
        || batch.member_digest != member_digest(snapshot.final_live_members())
        || batch.surface_digest != snapshot.native_surface().source_triangle_soup_digest()
        || batch.resource_plan.members != member_count
        || batch.resource_plan.stub_rows != stub_count
        || batch.resource_plan.nonstub_rows != member_count - stub_count
        || batch
            .resource_plan
            .stub_penetration
            .max_total_intervals
            .checked_add(batch.resource_plan.nonstub_penetration.max_total_intervals)
            != Some(batch.resource_plan.retained_interval_capacity)
        || batch.intervals.len() > batch.resource_plan.retained_interval_capacity()
    {
        return Err(TransitIntervalError::AnchorBatchBinding);
    }
    let policy = TransitAnchorPolicySnapshot::from_constants(
        m0_constants().map_err(|_| TransitIntervalError::PolicyBinding { id: "M0" })?,
    )?;
    let mut members = snapshot.final_live_members().collect::<Vec<_>>();
    members.sort_unstable_by_key(|member| (member.key(), member.embed_end()));
    if members
        .windows(2)
        .any(|pair| pair[0].key() == pair[1].key())
    {
        return Err(TransitIntervalError::AnchorEvidence);
    }

    let mut interval_index = 0usize;
    let mut anchored_members = Vec::with_capacity(members.len());
    for member in members {
        let normalized = normalize(member);
        let offsets = !matches!(member.kind(), MemberKind::Stub);
        let mut rays = capsule_rays(
            normalized.p0,
            normalized.p1,
            normalized.r0,
            normalized.r1,
            OFFSET_RAYS,
            offsets,
        );
        let mut anchored_rays = Vec::with_capacity(usize::from(offsets) * OFFSET_RAYS + 1);
        while let Some(ray) = rays.next() {
            let begin = interval_index;
            while let Some(interval) = batch.intervals.get(interval_index) {
                let key = (interval.member, interval.designed_end, interval.ray);
                let expected = (member.key(), member.embed_end(), ray.label());
                if key < expected {
                    return Err(TransitIntervalError::AnchorEvidence);
                }
                if key > expected {
                    break;
                }
                if !valid_transit_batch_interval(interval, ray.length_mm())
                    || (interval_index > begin
                        && (batch.intervals[interval_index - 1].s_b_mm > interval.s_a_mm
                            || batch.intervals[interval_index - 1] == *interval))
                {
                    return Err(TransitIntervalError::AnchorEvidence);
                }
                interval_index += 1;
            }
            let crossings = anchor_crossings(
                member.embed_end(),
                &policy,
                &batch.intervals[begin..interval_index],
            );
            anchored_rays.push(AnchoredTransitRay {
                ray: ray.label(),
                length_mm: ray.length_mm(),
                crossings,
            });
        }
        anchored_members.push(AnchoredTransitMember {
            member: member.key(),
            designed_end: member.embed_end(),
            rays: anchored_rays.into_boxed_slice(),
        });
    }
    if interval_index != batch.intervals.len() {
        return Err(TransitIntervalError::AnchorEvidence);
    }
    Ok(AnchoredTransitBatch {
        snapshot_digest: batch.snapshot_digest,
        member_digest: batch.member_digest,
        surface_digest: batch.surface_digest,
        policy,
        members: anchored_members.into_boxed_slice(),
    })
}

fn anchor_crossings(
    designed_end: Option<EmbedEnd>,
    policy: &TransitAnchorPolicySnapshot,
    intervals: &[TransitRayInterval],
) -> Box<[AnchoredTransitCrossing]> {
    intervals
        .iter()
        .enumerate()
        .map(|(raw_index, interval)| {
            let chord_mm = interval.s_b_mm - interval.s_a_mm;
            // This test intentionally precedes the length cutoff. A short
            // first interval consumes the only tip slot; a later interval can
            // never be promoted into it.
            AnchoredTransitCrossing {
                raw_index,
                s_a_mm: interval.s_a_mm,
                s_b_mm: interval.s_b_mm,
                chord_mm,
                is_tip_candidate: designed_end.is_some()
                    && raw_index == 0
                    && interval.s_a_mm <= policy.interval_tol_mm,
                below_minimum_length: chord_mm <= policy.min_illegal_len_mm,
            }
        })
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

fn valid_transit_batch_interval(interval: &TransitRayInterval, ray_length_mm: f64) -> bool {
    interval.s_a_mm.is_finite()
        && interval.s_b_mm.is_finite()
        && interval.s_a_mm >= 0.0
        && interval.s_a_mm <= interval.s_b_mm
        && interval.s_b_mm <= ray_length_mm
}

fn normalize(member: &FinalLiveMemberEvidence) -> NormalizedMember<'_> {
    if member.embed_end() == Some(EmbedEnd::Rootward) {
        NormalizedMember {
            source: member,
            p0: member.p1_mm(),
            p1: member.p0_mm(),
            r0: member.rootward_radius_mm(),
            r1: member.terminal_radius_mm(),
        }
    } else {
        NormalizedMember {
            source: member,
            p0: member.p0_mm(),
            p1: member.p1_mm(),
            r0: member.terminal_radius_mm(),
            r1: member.rootward_radius_mm(),
        }
    }
}

fn collect_batch(
    evidence: &mut Vec<TransitRayInterval>,
    surface: &geom_kernel::NativeSurface,
    members: &[NormalizedMember<'_>],
    offsets: bool,
    limits: TransitIntervalLimits,
) -> Result<(), TransitIntervalError> {
    if members.is_empty() {
        return Ok(());
    }
    let mut p0 = Vec::new();
    let mut p1 = Vec::new();
    let mut r0 = Vec::new();
    let mut r1 = Vec::new();
    for buffer in [&mut p0, &mut p1] {
        buffer.try_reserve_exact(members.len()).map_err(|_| {
            TransitIntervalError::ResourceLimit {
                resource: "endpoint rows allocation",
            }
        })?;
        #[cfg(test)]
        record_live_call(|calls| calls.local_row_allocations += 1);
    }
    for buffer in [&mut r0, &mut r1] {
        buffer.try_reserve_exact(members.len()).map_err(|_| {
            TransitIntervalError::ResourceLimit {
                resource: "radius rows allocation",
            }
        })?;
        #[cfg(test)]
        record_live_call(|calls| calls.local_row_allocations += 1);
    }
    for member in members {
        p0.push(member.p0);
        p1.push(member.p1);
        r0.push(member.r0);
        r1.push(member.r1);
    }
    #[cfg(test)]
    record_live_call(|calls| {
        if offsets {
            calls.nonstub_kernel_calls += 1;
        } else {
            calls.stub_kernel_calls += 1;
        }
    });
    let rows = surface
        .penetration_intervals_batch_bounded(
            &p0,
            &p1,
            &r0,
            &r1,
            if offsets { OFFSET_RAYS } else { 0 },
            limits.majority_step_mm,
            limits.cap,
            0.0,
            offsets,
            if offsets {
                limits.nonstub_penetration
            } else {
                limits.stub_penetration
            },
        )
        .map_err(|_| TransitIntervalError::KernelRefusal)?;
    collect_rows(evidence, members, offsets, rows)
}

fn collect_rows(
    evidence: &mut Vec<TransitRayInterval>,
    members: &[NormalizedMember<'_>],
    offsets: bool,
    rows: Vec<Vec<Interval>>,
) -> Result<(), TransitIntervalError> {
    if rows.len() != members.len() {
        return Err(TransitIntervalError::CorruptKernelResult);
    }
    for (member, row) in members.iter().zip(rows) {
        let mut rays = capsule_rays(
            member.p0,
            member.p1,
            member.r0,
            member.r1,
            OFFSET_RAYS,
            offsets,
        );
        let mut prior_label = -1;
        let mut prior_s_b = None;
        let mut current = rays.next();
        for interval in row {
            while current.is_some_and(|ray| ray.label() < interval.ray) {
                current = rays.next();
            }
            let Some(ray) = current else {
                return Err(TransitIntervalError::CorruptKernelResult);
            };
            let same_ray = interval.ray == prior_label;
            if interval.ray < prior_label
                || interval.ray != ray.label()
                || !valid_interval(&interval, ray.length_mm())
                // The kernel emits each ray's merged intervals in ascending,
                // disjoint arc order. Exact adjacency is valid; overlap or a
                // backward row would otherwise be hidden by final sorting.
                || (same_ray && prior_s_b.is_some_and(|s_b| interval.s_a < s_b))
            {
                return Err(TransitIntervalError::CorruptKernelResult);
            }
            prior_label = interval.ray;
            prior_s_b = Some(interval.s_b);
            evidence.push(TransitRayInterval {
                member: member.source.key(),
                designed_end: member.source.embed_end(),
                ray: interval.ray,
                s_a_mm: interval.s_a,
                s_b_mm: interval.s_b,
            });
        }
    }
    Ok(())
}

fn valid_interval(interval: &Interval, max_s: f64) -> bool {
    interval.s_a.is_finite()
        && interval.s_b.is_finite()
        && interval.s_a >= 0.0
        && interval.s_a <= interval.s_b
        && interval.s_b <= max_s
}

#[cfg(test)]
pub(crate) fn reset_live_kernel_batch_calls() {
    LIVE_CALLS.with(|calls| calls.set(LiveCalls::ZERO));
}

#[cfg(test)]
pub(crate) fn live_calls() -> LiveCalls {
    LIVE_CALLS.with(std::cell::Cell::get)
}

#[cfg(test)]
fn record_live_call(update: impl FnOnce(&mut LiveCalls)) {
    LIVE_CALLS.with(|calls| {
        let mut value = calls.get();
        update(&mut value);
        calls.set(value);
    });
}

fn preflight(
    member_count: usize,
    stub_rows: usize,
    limits: TransitIntervalLimits,
) -> Result<TransitIntervalResourcePlan, TransitIntervalError> {
    if member_count > limits.max_members {
        return Err(TransitIntervalError::ResourceLimit {
            resource: "members",
        });
    }
    let nonstub_rows =
        member_count
            .checked_sub(stub_rows)
            .ok_or(TransitIntervalError::ArithmeticOverflow {
                resource: "nonstub rows",
            })?;
    if stub_rows > limits.max_stub_rows || stub_rows > limits.stub_penetration.max_rows {
        return Err(TransitIntervalError::ResourceLimit {
            resource: "stub rows",
        });
    }
    if nonstub_rows > limits.max_nonstub_rows || nonstub_rows > limits.nonstub_penetration.max_rows
    {
        return Err(TransitIntervalError::ResourceLimit {
            resource: "nonstub rows",
        });
    }
    let capacity = limits
        .stub_penetration
        .max_total_intervals
        .checked_add(limits.nonstub_penetration.max_total_intervals)
        .ok_or(TransitIntervalError::ArithmeticOverflow {
            resource: "interval capacity",
        })?;
    if capacity > limits.max_retained_intervals {
        return Err(TransitIntervalError::ResourceLimit {
            resource: "retained intervals",
        });
    }
    let evidence_bytes = capacity
        .checked_mul(std::mem::size_of::<TransitRayInterval>())
        .ok_or(TransitIntervalError::ArithmeticOverflow {
            resource: "retained evidence bytes",
        })?;
    let input_bytes = member_count
        .checked_mul(std::mem::size_of::<&FinalLiveMemberEvidence>())
        .and_then(|n| {
            n.checked_add(member_count.checked_mul(std::mem::size_of::<NormalizedMember>())?)
        })
        .and_then(|n| {
            n.checked_add(member_count.checked_mul(
                2 * std::mem::size_of::<[f64; 3]>() + 2 * std::mem::size_of::<f64>(),
            )?)
        })
        .ok_or(TransitIntervalError::ArithmeticOverflow {
            resource: "row bytes",
        })?;
    let kernel_result_bytes = |batch: PenetrationBatchLimits| {
        batch
            .max_total_intervals
            .checked_mul(std::mem::size_of::<Interval>())
            .and_then(|n| {
                n.checked_add(
                    batch
                        .max_rows
                        .checked_mul(std::mem::size_of::<Vec<Interval>>())?,
                )
            })
            .ok_or(TransitIntervalError::ArithmeticOverflow {
                resource: "kernel result bytes",
            })
    };
    let kernel_peak = limits
        .stub_penetration
        .max_scratch_bytes
        .checked_add(kernel_result_bytes(limits.stub_penetration)?)
        .ok_or(TransitIntervalError::ArithmeticOverflow {
            resource: "stub kernel peak",
        })?
        .max(
            limits
                .nonstub_penetration
                .max_scratch_bytes
                .checked_add(kernel_result_bytes(limits.nonstub_penetration)?)
                .ok_or(TransitIntervalError::ArithmeticOverflow {
                    resource: "nonstub kernel peak",
                })?,
        );
    let logical_peak_bytes = input_bytes
        .checked_add(evidence_bytes)
        .and_then(|n| n.checked_add(kernel_peak))
        .ok_or(TransitIntervalError::ArithmeticOverflow {
            resource: "logical peak bytes",
        })?;
    if logical_peak_bytes > limits.max_logical_peak_bytes {
        return Err(TransitIntervalError::ResourceLimit {
            resource: "logical peak bytes",
        });
    }
    Ok(TransitIntervalResourcePlan {
        members: member_count,
        stub_rows,
        nonstub_rows,
        retained_interval_capacity: capacity,
        retained_evidence_bytes: evidence_bytes,
        logical_peak_bytes,
        stub_penetration: limits.stub_penetration,
        nonstub_penetration: limits.nonstub_penetration,
    })
}

fn member_digest<'a>(members: impl Iterator<Item = &'a FinalLiveMemberEvidence>) -> u64 {
    // The sealed snapshot digest is authoritative; this narrower deterministic
    // binding lets a later consumer detect a mismatched member evidence set.
    let mut state = 0xcbf2_9ce4_8422_2325_u64;
    for member in members {
        for word in [
            u64::from(member.key().slot()),
            u64::from(member.key().generation()),
            member.terminal_radius_mm().to_bits(),
            member.rootward_radius_mm().to_bits(),
            match member.embed_end() {
                None => 0,
                Some(EmbedEnd::Terminalward) => 1,
                Some(EmbedEnd::Rootward) => 2,
            },
        ] {
            state ^= word;
            state = state.wrapping_mul(0x0000_0100_0000_01b3);
        }
        for coordinate in member.p0_mm().into_iter().chain(member.p1_mm()) {
            state ^= coordinate.to_bits();
            state = state.wrapping_mul(0x0000_0100_0000_01b3);
        }
        let kind_tag = match member.kind() {
            MemberKind::Stub => 0_u8,
            MemberKind::Shoot => 1,
            MemberKind::Trunk => 2,
            MemberKind::Strut => 3,
            MemberKind::Brace => 4,
            MemberKind::Other(_) => 5,
        };
        state ^= u64::from(kind_tag);
        state = state.wrapping_mul(0x0000_0100_0000_01b3);
        if let MemberKind::Other(label) = member.kind() {
            for byte in label.bytes() {
                state ^= u64::from(byte);
                state = state.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use geom_kernel::{NativeSurface, NativeSurfaceConfig};
    use plinth_core::{m0_constants, RegistryProvenanceClass};

    fn scar_policy() -> TransitPolicySnapshot {
        TransitPolicySnapshot::from_constants(m0_constants().expect("M0 constants"))
            .expect("SCAR registry descriptors")
    }

    fn anchor_policy() -> TransitAnchorPolicySnapshot {
        TransitAnchorPolicySnapshot::from_constants(m0_constants().expect("M0 constants"))
            .expect("anchor registry descriptors")
    }

    fn raw_interval(s_a_mm: f64, s_b_mm: f64) -> TransitRayInterval {
        TransitRayInterval {
            member: MemberKey::transit_test_fixture(1, 0),
            designed_end: Some(EmbedEnd::Terminalward),
            ray: -1,
            s_a_mm,
            s_b_mm,
        }
    }

    #[test]
    fn anchor_keeps_the_first_tip_slot_before_the_post_anchor_length_cutoff() {
        let policy = anchor_policy();
        for chord_mm in [0.005, 0.031] {
            let rows = anchor_crossings(
                Some(EmbedEnd::Terminalward),
                &policy,
                &[raw_interval(0.0, chord_mm), raw_interval(0.2, 0.4)],
            );
            assert!(
                rows[0].is_tip_candidate(),
                "{chord_mm} mm first sliver anchors"
            );
            assert!(rows[0].below_minimum_length());
            assert!(
                !rows[1].is_tip_candidate(),
                "later transit is never promoted"
            );
            assert!(!rows[1].below_minimum_length());
        }
    }

    #[test]
    fn anchor_is_first_only_and_uses_the_recorded_inclusive_minimum_boundary() {
        let policy = anchor_policy();
        let boundary = policy.min_illegal_len_mm;
        let rows = anchor_crossings(
            Some(EmbedEnd::Rootward),
            &policy,
            &[
                raw_interval(policy.interval_tol_mm, policy.interval_tol_mm + boundary),
                raw_interval(policy.interval_tol_mm, policy.interval_tol_mm + 0.2),
                raw_interval(0.4, 0.6),
            ],
        );
        assert!(rows[0].is_tip_candidate());
        assert!(!rows[1].is_tip_candidate());
        assert!(
            rows[0].below_minimum_length(),
            "the <= boundary is excluded from the count"
        );
        let one_ulp_over = f64::from_bits(boundary.to_bits() + 1);
        let rows = anchor_crossings(None, &policy, &[raw_interval(0.0, one_ulp_over)]);
        assert!(
            !rows[0].is_tip_candidate(),
            "an undesigned end has no tip slot"
        );
        assert!(!rows[0].below_minimum_length());
    }

    #[allow(clippy::too_many_arguments)]
    fn scar_input(
        t_mm: f64,
        d_mm: f64,
        cosang: Option<f64>,
        raw_anchor_radius_mm: f64,
        tip_r_min_mm: f64,
        root_radius_mm: f64,
        member_length_mm: f64,
        member_kind: MemberKind,
    ) -> ScarDiagnosticInput {
        ScarDiagnosticInput::new(
            t_mm,
            d_mm,
            cosang,
            raw_anchor_radius_mm,
            tip_r_min_mm,
            root_radius_mm,
            member_length_mm,
            member_kind,
        )
        .expect("checked scalar fixture")
    }

    #[test]
    fn scar_policy_snapshot_retains_canonical_order_and_provenance() {
        let policy = scar_policy();
        assert_eq!(policy.decision_id.as_str(), "SCAR-AREA-CAP");
        assert_eq!(
            policy
                .policy_descriptors
                .iter()
                .map(|descriptor| descriptor.id().as_str())
                .collect::<Vec<_>>(),
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
        );
        assert!(policy
            .policy_descriptors
            .iter()
            .enumerate()
            .all(|(index, descriptor)| {
                descriptor.provenance().class()
                    == if index == 5 {
                        RegistryProvenanceClass::Measured
                    } else {
                        RegistryProvenanceClass::Placeholder
                    }
            }));
        assert_eq!(
            policy.min_illegal_descriptor.id().as_str(),
            "MIN_ILLEGAL_LEN_MM"
        );
        assert_eq!(
            policy.min_illegal_descriptor.provenance().class(),
            RegistryProvenanceClass::Placeholder
        );
        assert_eq!(policy.min_illegal_len_mm, 0.05);
    }

    #[test]
    fn scar_constraint_equalities_pass_and_one_ulp_excesses_fail() {
        let policy = scar_policy();
        let next = |value: f64| f64::from_bits(value.to_bits() + 1);
        let d_for_area = 0.5_f64;
        let ball_area = std::f64::consts::PI * (1.4_f64.powi(2) - d_for_area.powi(2));
        let design_area = std::f64::consts::PI * 1.4_f64.powi(2);
        let t_for_area = (policy.kappa * design_area - ball_area) / 2.0;
        let area_equal = calculate_scar_diagnostic(
            &policy,
            &scar_input(
                t_for_area,
                d_for_area,
                Some(1.0),
                1.0,
                0.5,
                1.0,
                5.0,
                MemberKind::Shoot,
            ),
        )
        .unwrap();
        assert_eq!(area_equal.area_patch_mm2, area_equal.area_max_mm2);
        assert!(area_equal.area_within_recorded_policy);
        let one_ulp_over_area = next(area_equal.area_patch_mm2);
        assert!(one_ulp_over_area > area_equal.area_max_mm2);
        assert!(
            !recorded_constraints(
                &policy,
                one_ulp_over_area,
                0.0,
                0.0,
                area_equal.area_design_mm2,
                0.0,
            )
            .0
        );

        let length_equal = calculate_scar_diagnostic(
            &policy,
            &scar_input(0.5, 0.0, Some(0.5), 1.0, 0.5, 1.0, 5.0, MemberKind::Shoot),
        )
        .unwrap();
        assert_eq!(length_equal.scar_length_mm, policy.l_scar_max_mm);
        assert!(length_equal.length_within_recorded_policy);
        let one_ulp_over_length = next(length_equal.scar_length_mm);
        assert!(one_ulp_over_length > policy.l_scar_max_mm);
        assert!(!recorded_constraints(&policy, 0.0, one_ulp_over_length, 0.0, 0.0, 0.0,).1);

        let anchor_equal = calculate_scar_diagnostic(
            &policy,
            &scar_input(1.0, 1.4, Some(1.0), 1.0, 0.5, 1.0, 1.0, MemberKind::Shoot),
        )
        .unwrap();
        assert_eq!(
            anchor_equal.anchor_area_mm2,
            anchor_equal.neck_disc_area_mm2
        );
        assert!(anchor_equal.anchor_meets_recorded_policy);
        let anchor_under = calculate_scar_diagnostic(
            &policy,
            &scar_input(
                1.0,
                1.4,
                Some(1.0),
                1.0,
                0.5,
                f64::from_bits(1.0_f64.to_bits() - 1),
                1.0,
                MemberKind::Shoot,
            ),
        )
        .unwrap();
        assert!(anchor_under.anchor_area_mm2 < anchor_under.neck_disc_area_mm2);
        assert!(!anchor_under.anchor_meets_recorded_policy);
    }

    #[test]
    fn scar_tolerance_and_eps_cosine_are_exactly_recorded() {
        let policy = scar_policy();
        let at_tolerance = calculate_scar_diagnostic(
            &policy,
            &scar_input(0.05, 0.0, None, 1.0, 0.5, 1.0, 5.0, MemberKind::Shoot),
        )
        .expect("T equals tolerance needs no cosine");
        assert_eq!(at_tolerance.area_shaft_mm2, 0.0);
        assert_eq!(at_tolerance.scar_length_mm, 0.0);
        assert!(matches!(
            calculate_scar_diagnostic(
                &policy,
                &scar_input(
                    0.050_000_000_1,
                    0.0,
                    None,
                    1.0,
                    0.5,
                    1.0,
                    5.0,
                    MemberKind::Shoot
                ),
            ),
            Err(ScarDiagnosticError::MissingCosine)
        ));
        let clamped = calculate_scar_diagnostic(
            &policy,
            &scar_input(0.06, 0.0, Some(0.0), 1.0, 0.5, 1.0, 5.0, MemberKind::Shoot),
        )
        .expect("zero cosine is clamped only by EPS_COS");
        assert_eq!(clamped.area_shaft_mm2, 0.12);
        assert_eq!(clamped.scar_length_mm, 2.06);
    }

    #[test]
    fn scar_uses_t_based_taper_end_based_balls_and_detached_anchor_formula() {
        let policy = scar_policy();
        let stub = calculate_scar_diagnostic(
            &policy,
            &scar_input(0.0, 10.0, None, 2.0, 0.5, 1.0, 1.0, MemberKind::Stub),
        )
        .unwrap();
        assert_eq!(stub.r_x_mm, 2.0, "fixed d does not control taper");
        assert_eq!(stub.ball_radius_mm, 3.0);
        let taper_clamped = calculate_scar_diagnostic(
            &policy,
            &scar_input(2.0, 10.0, Some(1.0), 2.0, 0.5, 1.0, 1.0, MemberKind::Stub),
        )
        .unwrap();
        assert_eq!(
            taper_clamped.r_x_mm, 1.0,
            "T/member length clamps at the root radius"
        );
        assert_eq!(
            taper_clamped.ball_radius_mm, 3.0,
            "ball remains tied to r_end"
        );
        let nonstub = calculate_scar_diagnostic(
            &policy,
            &scar_input(0.06, 0.0, Some(1.0), 0.5, 1.0, 1.0, 5.0, MemberKind::Brace),
        )
        .unwrap();
        assert_eq!(
            nonstub.r_x_mm, 1.0,
            "explicit tip radius floors the raw anchor radius"
        );
        assert_eq!(nonstub.ball_radius_mm, 1.4);
        assert!(nonstub.area_within_recorded_policy);
        assert!(nonstub.anchor_meets_recorded_policy);
        assert!(
            nonstub.area_max_mm2 > policy.a_abs_mm2,
            "KAPPA controls this foot case"
        );
        let detached = calculate_scar_diagnostic(
            &policy,
            &scar_input(0.5, 1.4, Some(1.0), 1.0, 0.5, 0.5, 1.0, MemberKind::Shoot),
        )
        .unwrap();
        assert_eq!(detached.area_ball_mm2, 0.0);
        assert!(!detached.anchor_meets_recorded_policy);
    }

    #[test]
    fn scar_checked_inputs_reject_nonfinite_negative_and_collapsed_lengths() {
        assert!(matches!(
            ScarDiagnosticInput::new(f64::NAN, 0.0, None, 1.0, 1.0, 1.0, 1.0, MemberKind::Stub),
            Err(ScarDiagnosticError::NonFinite { field: "T" })
        ));
        assert!(matches!(
            ScarDiagnosticInput::new(0.0, -1.0, None, 1.0, 1.0, 1.0, 1.0, MemberKind::Stub),
            Err(ScarDiagnosticError::Negative { field: "d" })
        ));
        assert!(matches!(
            ScarDiagnosticInput::new(0.0, 0.0, None, 1.0, 1.0, 1.0, 0.0, MemberKind::Stub),
            Err(ScarDiagnosticError::CollapsedLength)
        ));
    }

    fn fixture_member(
        slot: u32,
        kind: MemberKind,
        embed_end: Option<EmbedEnd>,
        p0: [f64; 3],
        p1: [f64; 3],
        r0: f64,
        r1: f64,
    ) -> FinalLiveMemberEvidence {
        FinalLiveMemberEvidence::transit_test_fixture(
            [slot, 0],
            [p0, p1],
            [r0, r1],
            kind,
            embed_end,
        )
    }

    fn surface() -> NativeSurface {
        let vertices = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ];
        let faces = [
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ];
        NativeSurface::from_indexed(
            &vertices,
            &faces,
            NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: 4 * 1024 * 1024,
            },
        )
        .expect("sealed transit fixture surface")
    }

    fn limits(
        surface: &NativeSurface,
        stub_rows: usize,
        nonstub_rows: usize,
    ) -> TransitIntervalLimits {
        let stub = surface
            .penetration_intervals_batch_worst_case_limits(stub_rows.max(1), 0, 8, false)
            .expect("stub fixture limits");
        let nonstub = surface
            .penetration_intervals_batch_worst_case_limits(
                nonstub_rows.max(1),
                OFFSET_RAYS,
                8,
                true,
            )
            .expect("nonstub fixture limits");
        TransitIntervalLimits::new(
            stub_rows + nonstub_rows + 1,
            stub_rows.max(1),
            nonstub_rows.max(1),
            stub.max_total_intervals + nonstub.max_total_intervals,
            usize::MAX,
            1.0,
            8,
            stub,
            nonstub,
        )
        .expect("fixture limits")
    }

    fn interval(ray: i32, s_a: f64, s_b: f64) -> Interval {
        Interval {
            s_a,
            s_b,
            entered: false,
            exited: false,
            max_depth: 0.0,
            ray,
        }
    }

    #[test]
    fn stub_uses_axis_only_and_tapered_nonstub_uses_canonical_capsule_rays() {
        let surface = surface();
        let stub = fixture_member(
            1,
            MemberKind::Stub,
            None,
            [-1.0, 0.5, 0.5],
            [2.0, 0.5, 0.5],
            0.05,
            0.05,
        );
        let nonstub = fixture_member(
            2,
            MemberKind::Trunk,
            None,
            [-1.0, 0.25, 0.5],
            [2.0, 0.25, 0.5],
            0.05,
            0.15,
        );
        let stubs = [normalize(&stub)];
        let nonstubs = [normalize(&nonstub)];
        let mut evidence = Vec::new();
        let limits = limits(&surface, 1, 1);

        collect_batch(&mut evidence, &surface, &stubs, false, limits).expect("stub batch");
        collect_batch(&mut evidence, &surface, &nonstubs, true, limits).expect("nonstub batch");

        let stub_labels = evidence
            .iter()
            .filter(|row| row.member() == stub.key())
            .map(|row| row.ray())
            .collect::<Vec<_>>();
        assert_eq!(stub_labels, vec![-1]);
        let nonstub_labels = evidence
            .iter()
            .filter(|row| row.member() == nonstub.key())
            .map(|row| row.ray())
            .collect::<Vec<_>>();
        assert_eq!(nonstub_labels, (-1..=7).collect::<Vec<_>>());
    }

    #[test]
    fn embedded_taper_normalization_places_the_designed_end_at_zero() {
        let terminal = fixture_member(
            1,
            MemberKind::Trunk,
            Some(EmbedEnd::Terminalward),
            [1.0, 2.0, 3.0],
            [4.0, 5.0, 6.0],
            1.0,
            3.0,
        );
        let root = fixture_member(
            2,
            MemberKind::Trunk,
            Some(EmbedEnd::Rootward),
            [1.0, 2.0, 3.0],
            [4.0, 5.0, 6.0],
            1.0,
            3.0,
        );
        let terminal = normalize(&terminal);
        let root = normalize(&root);
        assert_eq!(
            (terminal.p0, terminal.p1, terminal.r0, terminal.r1),
            ([1.0, 2.0, 3.0], [4.0, 5.0, 6.0], 1.0, 3.0)
        );
        assert_eq!(
            (root.p0, root.p1, root.r0, root.r1),
            ([4.0, 5.0, 6.0], [1.0, 2.0, 3.0], 3.0, 1.0)
        );
    }

    #[test]
    fn empty_partitions_skip_live_kernel_calls() {
        let surface = surface();
        let stub = fixture_member(
            1,
            MemberKind::Stub,
            None,
            [0.0; 3],
            [0.0, 0.0, 10.0],
            1.0,
            1.0,
        );
        let nonstub = fixture_member(
            2,
            MemberKind::Trunk,
            None,
            [5.0, 0.0, 0.0],
            [5.0, 0.0, 10.0],
            1.0,
            3.0,
        );
        let stub_rows = [normalize(&stub)];
        let nonstub_rows = [normalize(&nonstub)];
        let mut evidence = Vec::new();
        let limits = limits(&surface, 1, 1);

        reset_live_kernel_batch_calls();
        collect_batch(&mut evidence, &surface, &[], false, limits).expect("empty stub partition");
        collect_batch(&mut evidence, &surface, &[], true, limits).expect("empty nonstub partition");
        assert_eq!(live_calls(), LiveCalls::ZERO);

        collect_batch(&mut evidence, &surface, &stub_rows, false, limits)
            .expect("stub-only partition");
        collect_batch(&mut evidence, &surface, &[], true, limits)
            .expect("stub-only empty partition");
        assert_eq!(
            live_calls(),
            LiveCalls {
                local_row_allocations: 4,
                stub_kernel_calls: 1,
                nonstub_kernel_calls: 0,
                ..LiveCalls::ZERO
            }
        );

        reset_live_kernel_batch_calls();
        collect_batch(&mut evidence, &surface, &[], false, limits)
            .expect("nonstub-only empty partition");
        collect_batch(&mut evidence, &surface, &nonstub_rows, true, limits)
            .expect("nonstub-only partition");
        assert_eq!(
            live_calls(),
            LiveCalls {
                local_row_allocations: 4,
                stub_kernel_calls: 0,
                nonstub_kernel_calls: 1,
                ..LiveCalls::ZERO
            }
        );
    }

    #[test]
    fn preflight_admits_each_exact_scalar_cap_and_refuses_one_less_before_local_work() {
        let surface = surface();
        let stub = surface
            .penetration_intervals_batch_worst_case_limits(1, 0, 8, false)
            .expect("stub requirements");
        let nonstub = surface
            .penetration_intervals_batch_worst_case_limits(1, OFFSET_RAYS, 8, true)
            .expect("nonstub requirements");
        let base = TransitIntervalLimits::new(
            2,
            1,
            1,
            stub.max_total_intervals + nonstub.max_total_intervals,
            usize::MAX,
            1.0,
            8,
            stub,
            nonstub,
        )
        .expect("base limits");
        let plan = preflight(2, 1, base).expect("exact caller caps pass");
        assert_eq!(plan.members(), 2);
        assert_eq!((plan.stub_rows(), plan.nonstub_rows()), (1, 1));
        assert_eq!(
            plan.retained_interval_capacity(),
            base.max_retained_intervals
        );
        assert_eq!(
            plan.retained_evidence_bytes(),
            plan.retained_interval_capacity() * std::mem::size_of::<TransitRayInterval>()
        );

        for (resource, member_count, stub_count, limits) in [
            (
                "members",
                2,
                1,
                TransitIntervalLimits {
                    max_members: 1,
                    ..base
                },
            ),
            (
                "stub rows",
                2,
                1,
                TransitIntervalLimits {
                    max_stub_rows: 0,
                    ..base
                },
            ),
            (
                "nonstub rows",
                2,
                1,
                TransitIntervalLimits {
                    max_nonstub_rows: 0,
                    ..base
                },
            ),
            (
                "retained intervals",
                2,
                1,
                TransitIntervalLimits {
                    max_retained_intervals: base.max_retained_intervals - 1,
                    ..base
                },
            ),
        ] {
            reset_live_kernel_batch_calls();
            assert_eq!(
                preflight(member_count, stub_count, limits),
                Err(TransitIntervalError::ResourceLimit { resource })
            );
            assert_eq!(
                live_calls(),
                LiveCalls::ZERO,
                "{resource} refuses before local rows or kernel"
            );
        }

        let exact_peak = plan.logical_peak_bytes();
        assert_eq!(
            preflight(
                2,
                1,
                TransitIntervalLimits {
                    max_logical_peak_bytes: exact_peak,
                    ..base
                }
            )
            .expect("exact logical peak cap passes")
            .logical_peak_bytes(),
            exact_peak
        );
        assert_eq!(
            preflight(
                2,
                1,
                TransitIntervalLimits {
                    max_logical_peak_bytes: exact_peak - 1,
                    ..base
                }
            ),
            Err(TransitIntervalError::ResourceLimit {
                resource: "logical peak bytes"
            })
        );
    }

    #[test]
    fn each_kernel_limit_is_exact_and_one_less_refuses_with_transit_kernel_type() {
        let surface = surface();
        let member = fixture_member(
            1,
            MemberKind::Trunk,
            None,
            [-1.0, 0.5, 0.5],
            [2.0, 0.5, 0.5],
            0.05,
            0.05,
        );
        let members = [normalize(&member)];
        let base = limits(&surface, 0, 1);
        let required = base.nonstub_penetration;
        for (name, lowered) in [
            (
                "rows",
                PenetrationBatchLimits {
                    max_rows: required.max_rows - 1,
                    ..required
                },
            ),
            (
                "work",
                PenetrationBatchLimits {
                    max_total_work: required.max_total_work - 1,
                    ..required
                },
            ),
            (
                "hits",
                PenetrationBatchLimits {
                    max_total_hits: required.max_total_hits - 1,
                    ..required
                },
            ),
            (
                "samples",
                PenetrationBatchLimits {
                    max_total_samples: required.max_total_samples - 1,
                    ..required
                },
            ),
            (
                "intervals",
                PenetrationBatchLimits {
                    max_total_intervals: required.max_total_intervals - 1,
                    ..required
                },
            ),
            (
                "scratch bytes",
                PenetrationBatchLimits {
                    max_scratch_bytes: required.max_scratch_bytes - 1,
                    ..required
                },
            ),
        ] {
            let exact = TransitIntervalLimits {
                nonstub_penetration: required,
                ..base
            };
            collect_batch(&mut Vec::new(), &surface, &members, true, exact)
                .unwrap_or_else(|error| panic!("exact {name} limit must pass: {error:?}"));
            reset_live_kernel_batch_calls();
            let refused = TransitIntervalLimits {
                nonstub_penetration: lowered,
                ..base
            };
            assert_eq!(
                collect_batch(&mut Vec::new(), &surface, &members, true, refused),
                Err(TransitIntervalError::KernelRefusal),
                "{name} must preserve the transit kernel refusal"
            );
            assert_eq!(
                live_calls(),
                LiveCalls {
                    local_row_allocations: 4,
                    stub_kernel_calls: 0,
                    nonstub_kernel_calls: 1,
                    ..LiveCalls::ZERO
                },
                "{name} refusal enters only the nonstub kernel once"
            );
        }
    }

    #[test]
    fn each_stub_kernel_limit_is_exact_and_one_less_refuses_with_transit_kernel_type() {
        let surface = surface();
        let member = fixture_member(
            1,
            MemberKind::Stub,
            None,
            [-1.0, 0.5, 0.5],
            [2.0, 0.5, 0.5],
            0.05,
            0.05,
        );
        let members = [normalize(&member)];
        let base = limits(&surface, 1, 0);
        let required = base.stub_penetration;
        for (name, lowered) in [
            (
                "rows",
                PenetrationBatchLimits {
                    max_rows: required.max_rows - 1,
                    ..required
                },
            ),
            (
                "work",
                PenetrationBatchLimits {
                    max_total_work: required.max_total_work - 1,
                    ..required
                },
            ),
            (
                "hits",
                PenetrationBatchLimits {
                    max_total_hits: required.max_total_hits - 1,
                    ..required
                },
            ),
            (
                "samples",
                PenetrationBatchLimits {
                    max_total_samples: required.max_total_samples - 1,
                    ..required
                },
            ),
            (
                "intervals",
                PenetrationBatchLimits {
                    max_total_intervals: required.max_total_intervals - 1,
                    ..required
                },
            ),
            (
                "scratch bytes",
                PenetrationBatchLimits {
                    max_scratch_bytes: required.max_scratch_bytes - 1,
                    ..required
                },
            ),
        ] {
            collect_batch(
                &mut Vec::new(),
                &surface,
                &members,
                false,
                TransitIntervalLimits {
                    stub_penetration: required,
                    ..base
                },
            )
            .unwrap_or_else(|error| panic!("exact stub {name} limit must pass: {error:?}"));
            reset_live_kernel_batch_calls();
            assert_eq!(
                collect_batch(
                    &mut Vec::new(),
                    &surface,
                    &members,
                    false,
                    TransitIntervalLimits {
                        stub_penetration: lowered,
                        ..base
                    },
                ),
                Err(TransitIntervalError::KernelRefusal),
                "stub {name} must preserve the transit kernel refusal"
            );
            assert_eq!(
                live_calls(),
                LiveCalls {
                    local_row_allocations: 4,
                    stub_kernel_calls: 1,
                    nonstub_kernel_calls: 0,
                    ..LiveCalls::ZERO
                },
                "stub {name} refusal enters only the stub kernel once"
            );
        }
    }

    #[test]
    fn preflight_checked_arithmetic_refuses_overflow() {
        let surface = surface();
        let base = limits(&surface, 1, 1);
        assert_eq!(
            preflight(
                1,
                0,
                TransitIntervalLimits {
                    stub_penetration: PenetrationBatchLimits {
                        max_total_intervals: usize::MAX,
                        ..base.stub_penetration
                    },
                    nonstub_penetration: PenetrationBatchLimits {
                        max_total_intervals: 1,
                        ..base.nonstub_penetration
                    },
                    ..base
                }
            ),
            Err(TransitIntervalError::ArithmeticOverflow {
                resource: "interval capacity"
            })
        );
    }

    #[test]
    fn complete_batch_is_identical_at_one_four_and_sixteen_threads() {
        const TEST: &str = "transit_measurement::tests::complete_batch_is_identical_at_one_four_and_sixteen_threads";
        if std::env::var_os("TRANSIT_THREAD_CHILD").is_some() {
            let surface = surface();
            let stub = fixture_member(
                1,
                MemberKind::Stub,
                None,
                [-1.0, 0.5, 0.5],
                [2.0, 0.5, 0.5],
                0.05,
                0.05,
            );
            let nonstub = fixture_member(
                2,
                MemberKind::Trunk,
                Some(EmbedEnd::Rootward),
                [-1.0, 0.25, 0.5],
                [2.0, 0.25, 0.5],
                0.05,
                0.15,
            );
            let limits = limits(&surface, 1, 1);
            let plan = preflight(2, 1, limits).expect("fixture plan");
            let mut evidence = Vec::new();
            collect_batch(&mut evidence, &surface, &[normalize(&stub)], false, limits)
                .expect("stub batch");
            collect_batch(
                &mut evidence,
                &surface,
                &[normalize(&nonstub)],
                true,
                limits,
            )
            .expect("nonstub batch");
            evidence.sort_unstable_by(|left, right| {
                (left.member, left.designed_end, left.ray)
                    .cmp(&(right.member, right.designed_end, right.ray))
                    .then_with(|| left.s_a_mm.total_cmp(&right.s_a_mm))
                    .then_with(|| left.s_b_mm.total_cmp(&right.s_b_mm))
            });
            let batch = TransitIntervalBatch {
                snapshot_digest: 0xA11C_E001,
                member_digest: member_digest([&stub, &nonstub].into_iter()),
                surface_digest: surface.source_triangle_soup_digest(),
                resource_plan: plan,
                intervals: evidence.into_boxed_slice(),
            };
            assert_eq!(
                geom_kernel::current_threads().to_string(),
                std::env::var("PLINTH_THREADS").expect("child thread authority")
            );
            println!("TRANSIT_SIGNATURE:{batch:?}");
            return;
        }
        let signatures = ["1", "4", "16"].map(|threads| {
            let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
                .arg(TEST)
                .arg("--exact")
                .arg("--nocapture")
                .env("PLINTH_THREADS", threads)
                .env("TRANSIT_THREAD_CHILD", "1")
                .output()
                .expect("isolated thread child");
            assert!(
                output.status.success(),
                "thread child {threads} failed: {output:?}"
            );
            String::from_utf8(output.stdout).expect("test output utf-8")
        });
        assert_eq!(signatures[0], signatures[1]);
        assert_eq!(signatures[0], signatures[2]);
    }

    #[test]
    fn validation_retains_short_intervals_and_rejects_corrupt_rows_per_exact_ray() {
        let member = fixture_member(
            1,
            MemberKind::Trunk,
            None,
            [0.0; 3],
            [0.0, 0.0, 10.0],
            1.0,
            3.0,
        );
        let members = [normalize(&member)];
        let mut evidence = Vec::new();
        collect_rows(
            &mut evidence,
            &members,
            true,
            vec![vec![interval(-1, 0.0, 0.01)]],
        )
        .expect("kernel min filter is zero, so short evidence remains raw");
        assert_eq!(evidence[0].s_b_mm(), 0.01);

        collect_rows(
            &mut Vec::new(),
            &members,
            true,
            vec![vec![interval(-1, 1.0, 2.0), interval(-1, 2.0, 3.0)]],
        )
        .expect("adjacent same-ray intervals retain the kernel's valid ordering");

        for rows in [
            vec![],
            vec![vec![interval(-1, f64::NAN, 1.0)]],
            vec![vec![interval(-1, 2.0, 1.0)]],
            vec![vec![interval(8, 0.0, 1.0)]],
            vec![vec![interval(-1, 0.0, 10.01)]],
            vec![vec![interval(1, 0.0, 1.0), interval(0, 0.0, 1.0)]],
            vec![vec![interval(-1, 5.0, 6.0), interval(-1, 1.0, 2.0)]],
            vec![vec![interval(-1, 1.0, 3.0), interval(-1, 2.0, 4.0)]],
        ] {
            assert_eq!(
                collect_rows(&mut Vec::new(), &members, true, rows),
                Err(TransitIntervalError::CorruptKernelResult)
            );
        }

        let offset_length = capsule_rays([0.0; 3], [0.0, 0.0, 10.0], 1.0, 3.0, OFFSET_RAYS, true)
            .nth(1)
            .expect("canonical offset ray")
            .length_mm();
        collect_rows(
            &mut Vec::new(),
            &members,
            true,
            vec![vec![interval(0, 10.0, offset_length)]],
        )
        .expect("valid tapered offset may extend past the shorter axis");
    }
}
