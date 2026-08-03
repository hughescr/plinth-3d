//! Raw, bounded final-member transit evidence.
//!
//! This is intentionally a measurement boundary only.  It neither classifies
//! an interval nor assigns a clearance, gate, report, certificate, or defect.

use geom_kernel::{Interval, PenetrationBatchLimits, SurfaceTriangleSoupDigest};
use thiserror::Error;

use crate::{
    forest::{EmbedEnd, MemberKey, MemberKind},
    runner::{FinalLiveMemberEvidence, SupportVerificationSnapshot},
};

const OFFSET_RAYS: usize = 8;

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
    members.extend(snapshot.final_live_members());
    members.sort_unstable_by_key(|member| (member.key(), member.embed_end()));

    let mut stubs = Vec::new();
    let mut nonstubs = Vec::new();
    stubs
        .try_reserve_exact(plan.stub_rows)
        .map_err(|_| TransitIntervalError::ResourceLimit {
            resource: "stub rows allocation",
        })?;
    nonstubs.try_reserve_exact(plan.nonstub_rows).map_err(|_| {
        TransitIntervalError::ResourceLimit {
            resource: "nonstub rows allocation",
        }
    })?;
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
    collect_batch(&mut evidence, snapshot, &stubs, false, limits)?;
    collect_batch(&mut evidence, snapshot, &nonstubs, true, limits)?;
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
    snapshot: &SupportVerificationSnapshot<'_>,
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
    }
    for buffer in [&mut r0, &mut r1] {
        buffer.try_reserve_exact(members.len()).map_err(|_| {
            TransitIntervalError::ResourceLimit {
                resource: "radius rows allocation",
            }
        })?;
    }
    for member in members {
        p0.push(member.p0);
        p1.push(member.p1);
        r0.push(member.r0);
        r1.push(member.r1);
    }
    let rows = snapshot
        .native_surface()
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
    if rows.len() != members.len() {
        return Err(TransitIntervalError::CorruptKernelResult);
    }
    for (member, row) in members.iter().zip(rows) {
        let max_s = ray_arc_length(member.p0, member.p1, member.r0, member.r1);
        for interval in row {
            if !valid_interval(&interval, offsets, max_s) {
                return Err(TransitIntervalError::CorruptKernelResult);
            }
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

fn valid_interval(interval: &Interval, offsets: bool, max_s: f64) -> bool {
    interval.s_a.is_finite()
        && interval.s_b.is_finite()
        && interval.s_a >= 0.0
        && interval.s_a <= interval.s_b
        && interval.s_b <= max_s
        && if offsets {
            (-1..=7).contains(&interval.ray)
        } else {
            interval.ray == -1
        }
}

fn ray_arc_length(p0: [f64; 3], p1: [f64; 3], r0: f64, r1: f64) -> f64 {
    let axis_sq = p0
        .into_iter()
        .zip(p1)
        .map(|(a, b)| (a - b) * (a - b))
        .sum::<f64>();
    (axis_sq + (r1 - r0) * (r1 - r0)).sqrt()
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
