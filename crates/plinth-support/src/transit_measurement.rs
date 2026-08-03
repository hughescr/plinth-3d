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
    static LIVE_KERNEL_BATCH_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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
    #[cfg(test)]
    LIVE_KERNEL_BATCH_CALLS.with(|calls| calls.set(calls.get() + 1));
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
        let mut current = rays.next();
        for interval in row {
            while current.is_some_and(|ray| ray.label() < interval.ray) {
                current = rays.next();
            }
            let Some(ray) = current else {
                return Err(TransitIntervalError::CorruptKernelResult);
            };
            if interval.ray < prior_label
                || interval.ray != ray.label()
                || !valid_interval(&interval, ray.length_mm())
            {
                return Err(TransitIntervalError::CorruptKernelResult);
            }
            prior_label = interval.ray;
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
fn reset_live_kernel_batch_calls() {
    LIVE_KERNEL_BATCH_CALLS.with(|calls| calls.set(0));
}

#[cfg(test)]
fn live_kernel_batch_calls() -> usize {
    LIVE_KERNEL_BATCH_CALLS.with(std::cell::Cell::get)
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
        assert_eq!(live_kernel_batch_calls(), 0);

        collect_batch(&mut evidence, &surface, &stub_rows, false, limits)
            .expect("stub-only partition");
        collect_batch(&mut evidence, &surface, &[], true, limits)
            .expect("stub-only empty partition");
        assert_eq!(live_kernel_batch_calls(), 1);

        reset_live_kernel_batch_calls();
        collect_batch(&mut evidence, &surface, &[], false, limits)
            .expect("nonstub-only empty partition");
        collect_batch(&mut evidence, &surface, &nonstub_rows, true, limits)
            .expect("nonstub-only partition");
        assert_eq!(live_kernel_batch_calls(), 1);
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

        for rows in [
            vec![],
            vec![vec![interval(-1, f64::NAN, 1.0)]],
            vec![vec![interval(-1, 2.0, 1.0)]],
            vec![vec![interval(8, 0.0, 1.0)]],
            vec![vec![interval(-1, 0.0, 10.01)]],
            vec![vec![interval(1, 0.0, 1.0), interval(0, 0.0, 1.0)]],
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
