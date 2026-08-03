//! Raw, bounded final-member transit evidence.
//!
//! This is intentionally a measurement boundary only.  It neither classifies
//! an interval nor assigns a clearance, gate, report, certificate, or defect.

use geom_kernel::{capsule_rays, Interval, PenetrationBatchLimits, SurfaceTriangleSoupDigest};
use thiserror::Error;

use crate::{
    forest::{EmbedEnd, MemberKey, MemberKind},
    runner::{FinalLiveMemberEvidence, SupportVerificationSnapshot},
};

const OFFSET_RAYS: usize = 8;

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
