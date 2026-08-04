//! The designed clearance-mask domain: an optional, construction-validated
//! axial extent that a member end may carry because that end is deliberately
//! embedded in the model.
//!
//! Three properties are load bearing and are the reason this lives in its own
//! crate-private module rather than beside the forest data model:
//!
//! * A mask is *minted*, never asserted. [`DesignedMaskTable::validate_forest`]
//!   is the sole production entry point, and every extent it returns has been
//!   proven connected to its endpoint through the actual tapered member.
//! * Observed penetration thickness is separate evidence. Nothing here reads a
//!   penetration interval, a projection distance, or a breach test to *derive*
//!   an extent; the caller supplies a design-time number and the kernel batch
//!   only ever refuses it.
//! * Validation is whole-forest and batched. One native
//!   `penetration_intervals_batch_bounded` call serves every requested endpoint
//!   in the forest, in an order that does not depend on caller order or on the
//!   thread count.
//!
//! Kernel-call census invariant: [`record_endpoint_validation_kernel_call`] runs
//! on the thread that calls the mint, so the census is thread-local. That is
//! only sound because no mint may ever be issued from inside a rayon closure —
//! doing so would both fragment the census and reintroduce the per-item native
//! call this module exists to avoid. Mints belong at stage boundaries.

use std::collections::BTreeMap;

use geom_kernel::{
    capsule_rays, NativeSurface, PenetrationBatchLimits, SurfaceTriangleSoupDigest, V3,
};
use plinth_core::{Finite, PositiveFinite};
use serde::Serialize;
use thiserror::Error;

use crate::forest::{mask_binding, EmbedEnd, Forest, Member, MemberKey};

/// A requested designed clearance extent in millimetres. This is deliberately
/// distinct from observed penetration evidence and cannot be zero or non-finite.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize)]
pub(crate) struct PositiveFiniteMm(PositiveFinite);

impl PositiveFiniteMm {
    pub(crate) fn new(mm: f64) -> Result<Self, DesignedMaskError> {
        PositiveFinite::new(mm)
            .map(Self)
            .map_err(|_| DesignedMaskError::InvalidExtent)
    }

    #[must_use]
    pub(crate) const fn get(self) -> f64 {
        self.0.get()
    }
}

/// Explicit sampling and resource admission for an endpoint-mask construction
/// check. There is deliberately no production default: this is the precise
/// native query budget selected by the construction caller.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EndpointMaskValidationLimits {
    majority_step_mm: PositiveFiniteMm,
    samples_per_subsegment: usize,
    penetration: PenetrationBatchLimits,
}

impl EndpointMaskValidationLimits {
    pub(crate) fn new(
        majority_step_mm: PositiveFiniteMm,
        samples_per_subsegment: usize,
        penetration: PenetrationBatchLimits,
    ) -> Result<Self, DesignedMaskError> {
        if samples_per_subsegment == 0 {
            return Err(DesignedMaskError::InvalidValidationLimits);
        }
        Ok(Self {
            majority_step_mm,
            samples_per_subsegment,
            penetration,
        })
    }

    /// Admission sized for a whole-forest mint: `rows` is the total number of
    /// requested endpoints across every member, never a per-member figure.
    ///
    /// Sampling resolution stays a caller decision precisely because the
    /// connectivity proof's sensitivity is a design question, not a default.
    pub(crate) fn for_batch(
        surface: &NativeSurface,
        rows: usize,
        majority_step_mm: PositiveFiniteMm,
        samples_per_subsegment: usize,
    ) -> Result<Self, DesignedMaskError> {
        let penetration = surface
            .penetration_intervals_batch_worst_case_limits(
                rows,
                ENDPOINT_VALIDATION_OFFSET_RAYS,
                samples_per_subsegment,
                true,
            )
            .map_err(|_| DesignedMaskError::ResourceLimit)?;
        Self::new(majority_step_mm, samples_per_subsegment, penetration)
    }
}

/// Opaque, immutable evidence that one endpoint's exact intended member
/// geometry was connected to that endpoint in the native surface.
#[derive(Clone, PartialEq)]
pub(crate) struct DesignedEndMask {
    requested_extent_mm: PositiveFiniteMm,
    witness: u64,
    validation_binding: u64,
    surface_identity: SurfaceTriangleSoupDigest,
}

impl DesignedEndMask {
    /// Return the extent only when this exact member, end, and immutable
    /// native surface are still the geometry that construction validated.
    pub(crate) fn validated_extent_for(
        &self,
        member: &Member,
        end: EmbedEnd,
        surface: &NativeSurface,
    ) -> Result<PositiveFiniteMm, DesignedMaskError> {
        let surface_identity = surface.source_triangle_soup_digest();
        (self.surface_identity == surface_identity
            && self.validation_binding
                == mask_binding(
                    member,
                    end,
                    self.requested_extent_mm,
                    self.witness,
                    surface_identity,
                ))
        .then_some(self.requested_extent_mm)
        .ok_or(DesignedMaskError::StaleBinding)
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) const fn validation_binding(&self) -> u64 {
        self.validation_binding
    }
}

/// Deliberately opaque: the witness and the binding are evidence, not data, and
/// printing them invites treating a formatted mask as a transportable claim.
impl std::fmt::Debug for DesignedEndMask {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DesignedEndMask")
            .field("requested_extent_mm", &self.requested_extent_mm.get())
            .finish_non_exhaustive()
    }
}

/// Independent end masks. Construction is crate-private so callers cannot inject
/// an unvalidated clearance claim into a member or verifier.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DesignedClearanceMasks {
    terminalward: Option<DesignedEndMask>,
    rootward: Option<DesignedEndMask>,
}

impl DesignedClearanceMasks {
    #[must_use]
    pub(crate) const fn terminalward(&self) -> Option<&DesignedEndMask> {
        self.terminalward.as_ref()
    }

    #[must_use]
    pub(crate) const fn rootward(&self) -> Option<&DesignedEndMask> {
        self.rootward.as_ref()
    }

    #[must_use]
    pub(crate) const fn end(&self, end: EmbedEnd) -> Option<&DesignedEndMask> {
        match end {
            EmbedEnd::Terminalward => self.terminalward(),
            EmbedEnd::Rootward => self.rootward(),
        }
    }

    /// The order is a contract: terminalward before rootward, independent of
    /// construction order or thread count.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (EmbedEnd, &DesignedEndMask)> {
        [
            (EmbedEnd::Terminalward, self.terminalward.as_ref()),
            (EmbedEnd::Rootward, self.rootward.as_ref()),
        ]
        .into_iter()
        .filter_map(|(end, mask)| mask.map(|mask| (end, mask)))
    }
}

/// One design-time extent request for one declared embed end.
///
/// The extent is a policy number chosen by the design family that placed the
/// member. Sizing it from a projection distance, a penetration interval, or a
/// breach test would be inference from observed geometry and is forbidden.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DesignedMaskRequest {
    pub(crate) member: MemberKey,
    pub(crate) end: EmbedEnd,
    pub(crate) extent: PositiveFiniteMm,
}

/// Per-forest-version side table of validated masks.
///
/// Masks deliberately do not live on [`Member`]: `Member` is serialized and
/// digested, so carrying evidence there would perturb `Forest::digest`. The
/// table needs no manual invalidation either — `mask_binding` digests the whole
/// member including its clearance certificate's `geom_version`, so any mutation
/// makes the stored entry fail `validated_extent_for` with `StaleBinding`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DesignedMaskTable {
    by_member: BTreeMap<MemberKey, DesignedClearanceMasks>,
}

impl DesignedMaskTable {
    #[must_use]
    pub(crate) fn masks_for(&self, key: MemberKey) -> Option<&DesignedClearanceMasks> {
        self.by_member.get(&key)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (MemberKey, &DesignedClearanceMasks)> {
        self.by_member.iter().map(|(key, masks)| (*key, masks))
    }

    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.by_member.len()
    }

    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        self.by_member.is_empty()
    }

    /// The extent a path that *requires* a designed embed must consume.
    ///
    /// There is no recovery from observed geometry and no fallback constant: an
    /// absent entry and a stale binding are both hard refusals.
    pub(crate) fn required_extent(
        &self,
        key: MemberKey,
        member: &Member,
        end: EmbedEnd,
        surface: &NativeSurface,
    ) -> Result<PositiveFiniteMm, DesignedMaskError> {
        self.masks_for(key)
            .and_then(|masks| masks.end(end))
            .ok_or(DesignedMaskError::MissingRequiredExtent { end })?
            .validated_extent_for(member, end, surface)
    }

    /// The sole production mint for designed clearance masks.
    ///
    /// Every requested endpoint in the forest is validated by exactly one native
    /// batch call. Validation is total over the forest: a live member that
    /// declares an embed end but carries no request refuses the whole mint, so
    /// the `MissingRequiredExtent` refusal fires here rather than silently
    /// deferring to a consumer.
    pub(crate) fn validate_forest(
        forest: &Forest,
        surface: &NativeSurface,
        requests: &[DesignedMaskRequest],
        limits: EndpointMaskValidationLimits,
    ) -> Result<Self, DesignedMaskError> {
        let keys = forest.live_members();
        let members = keys
            .iter()
            .map(|key| forest.member(*key).ok_or(DesignedMaskError::UnknownMember))
            .collect::<Result<Vec<_>, _>>()?;
        let index_of = keys
            .iter()
            .enumerate()
            .map(|(index, key)| (*key, index))
            .collect::<BTreeMap<_, _>>();
        let indexed = requests
            .iter()
            .map(|request| {
                index_of
                    .get(&request.member)
                    .copied()
                    .map(|index| (index, request.end, request.extent))
                    .ok_or(DesignedMaskError::UnknownMember)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let masks = validate_members_batch(surface, &members, &indexed, limits)?;
        Ok(Self {
            by_member: keys
                .into_iter()
                .zip(masks)
                .filter(|(_, masks)| masks.iter().next().is_some())
                .collect(),
        })
    }
}

/// Whole-batch endpoint-connectivity validation: gather every requested endpoint
/// across every member into one set of rows, issue exactly one native call, then
/// scatter the evidence back index-aligned with `members`.
///
/// Requests are member-indexed rather than keyed so that provisional candidate
/// members can be validated without minting forgeable forest keys.
pub(crate) fn validate_members_batch(
    surface: &NativeSurface,
    members: &[&Member],
    requests: &[(usize, EmbedEnd, PositiveFiniteMm)],
    limits: EndpointMaskValidationLimits,
) -> Result<Vec<DesignedClearanceMasks>, DesignedMaskError> {
    // Row order is a contract: member index, then terminalward before rootward.
    // Sorting here makes the minted table independent of caller order, and the
    // kernel's index-parallel batch makes it independent of thread count.
    let mut ordered = requests.to_vec();
    ordered.sort_by_key(|(index, end, _)| (*index, *end));
    for window in ordered.windows(2) {
        if (window[0].0, window[0].1) == (window[1].0, window[1].1) {
            return Err(DesignedMaskError::DuplicateRequest { end: window[0].1 });
        }
    }
    if ordered.iter().any(|(index, _, _)| *index >= members.len()) {
        return Err(DesignedMaskError::UnknownMember);
    }

    // Every scalar refusal runs for every row before any native work, so one bad
    // row refuses the whole batch at zero kernel cost.
    let mut rows = Vec::with_capacity(ordered.len());
    let mut cursor = 0usize;
    for (index, member) in members.iter().enumerate() {
        let start = cursor;
        while cursor < ordered.len() && ordered[cursor].0 == index {
            cursor += 1;
        }
        let requested = &ordered[start..cursor];
        if requested.is_empty() {
            if let Some(end) = member.embed_end() {
                return Err(DesignedMaskError::MissingRequiredExtent { end });
            }
            continue;
        }
        let length = member_length_mm(member)?;
        let mut terminal = None;
        let mut root = None;
        for (_, end, extent) in requested {
            let extent = validate_extent(*extent, length)?;
            match end {
                EmbedEnd::Terminalward => terminal = Some(extent),
                EmbedEnd::Rootward => root = Some(extent),
            }
        }
        if terminal
            .is_some_and(|value| root.is_some_and(|other| value.get() >= length - other.get()))
        {
            return Err(DesignedMaskError::OverlappingExtents);
        }
        // A mask on an end the member does not declare would be silently
        // ignored by every consumer, which is exactly how designed-mask code
        // went dead before. Refuse it instead.
        let declared = member.embed_end();
        for (_, end, _) in requested {
            if declared != Some(*end) {
                return Err(DesignedMaskError::EndNotDesigned { end: *end });
            }
        }
        if let Some(end) = declared {
            let present = match end {
                EmbedEnd::Terminalward => terminal.is_some(),
                EmbedEnd::Rootward => root.is_some(),
            };
            if !present {
                return Err(DesignedMaskError::MissingRequiredExtent { end });
            }
        }
        if let Some(extent) = terminal {
            rows.push(EndpointValidationRow::terminalward(
                index, member, extent, length,
            ));
        }
        if let Some(extent) = root {
            rows.push(EndpointValidationRow::rootward(
                index, member, extent, length,
            ));
        }
    }

    let mut minted = vec![DesignedClearanceMasks::default(); members.len()];
    if rows.is_empty() {
        return Ok(minted);
    }
    if rows.len() > limits.penetration.max_rows {
        return Err(DesignedMaskError::ResourceLimit);
    }

    let mut p0s = Vec::with_capacity(rows.len());
    let mut p1s = Vec::with_capacity(rows.len());
    let mut r0s = Vec::with_capacity(rows.len());
    let mut r1s = Vec::with_capacity(rows.len());
    for row in &rows {
        p0s.push(row.p0);
        p1s.push(row.p1);
        r0s.push(row.r0);
        r1s.push(row.r1);
    }
    #[cfg(test)]
    record_endpoint_validation_kernel_call();
    let interval_rows = surface
        .penetration_intervals_batch_bounded(
            &p0s,
            &p1s,
            &r0s,
            &r1s,
            ENDPOINT_VALIDATION_OFFSET_RAYS,
            limits.majority_step_mm.get(),
            limits.samples_per_subsegment,
            0.0,
            true,
            limits.penetration,
        )
        .map_err(|_| DesignedMaskError::KernelRefusal)?;
    if interval_rows.len() != rows.len() {
        return Err(DesignedMaskError::CorruptKernelResult);
    }

    let surface_identity = surface.source_triangle_soup_digest();
    for (row, intervals) in rows.into_iter().zip(interval_rows) {
        let member = members[row.member];
        let witness = endpoint_connected_witness(&row, &intervals)?;
        let mask = DesignedEndMask {
            requested_extent_mm: row.extent,
            witness,
            validation_binding: mask_binding(
                member,
                row.end,
                row.extent,
                witness,
                surface_identity,
            ),
            surface_identity,
        };
        match row.end {
            EmbedEnd::Terminalward => minted[row.member].terminalward = Some(mask),
            EmbedEnd::Rootward => minted[row.member].rootward = Some(mask),
        }
    }
    Ok(minted)
}

pub(crate) const ENDPOINT_VALIDATION_OFFSET_RAYS: usize = 8;

#[cfg(test)]
thread_local! {
    static ENDPOINT_VALIDATION_KERNEL_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn record_endpoint_validation_kernel_call() {
    ENDPOINT_VALIDATION_KERNEL_CALLS.with(|calls| calls.set(calls.get() + 1));
}

#[cfg(test)]
fn reset_endpoint_validation_kernel_calls() {
    ENDPOINT_VALIDATION_KERNEL_CALLS.with(|calls| calls.set(0));
}

#[cfg(test)]
fn endpoint_validation_kernel_calls() -> usize {
    ENDPOINT_VALIDATION_KERNEL_CALLS.with(std::cell::Cell::get)
}

#[derive(Clone, Copy)]
struct EndpointValidationRow {
    member: usize,
    end: EmbedEnd,
    extent: PositiveFiniteMm,
    fraction: f64,
    p0: V3,
    p1: V3,
    r0: f64,
    r1: f64,
}

impl EndpointValidationRow {
    fn terminalward(
        member_index: usize,
        member: &Member,
        extent: PositiveFiniteMm,
        length: f64,
    ) -> Self {
        Self {
            member: member_index,
            end: EmbedEnd::Terminalward,
            extent,
            fraction: extent.get() / length,
            p0: member.p0_mm().map(Finite::get),
            p1: member.p1_mm().map(Finite::get),
            r0: member.terminal_radius_mm().get().get(),
            r1: member.rootward_radius_mm().get().get(),
        }
    }

    fn rootward(
        member_index: usize,
        member: &Member,
        extent: PositiveFiniteMm,
        length: f64,
    ) -> Self {
        Self {
            member: member_index,
            end: EmbedEnd::Rootward,
            extent,
            fraction: extent.get() / length,
            p0: member.p1_mm().map(Finite::get),
            p1: member.p0_mm().map(Finite::get),
            r0: member.rootward_radius_mm().get().get(),
            r1: member.terminal_radius_mm().get().get(),
        }
    }
}

fn endpoint_connected_witness(
    row: &EndpointValidationRow,
    intervals: &[geom_kernel::Interval],
) -> Result<u64, DesignedMaskError> {
    let mut rays = capsule_rays(
        row.p0,
        row.p1,
        row.r0,
        row.r1,
        ENDPOINT_VALIDATION_OFFSET_RAYS,
        true,
    );
    let mut interval_index = 0usize;
    let mut digest = crate::forest::DigestWriter::new();
    digest.tag(0x46);
    digest.embed(Some(row.end));
    digest.f64(row.extent.get());
    for ray in rays.by_ref() {
        let target = ray.length_mm() * row.fraction;
        if !target.is_finite() || target <= 0.0 {
            return Err(DesignedMaskError::CorruptKernelResult);
        }
        let first = intervals.get(interval_index);
        let Some(first) = first else {
            return Err(DesignedMaskError::DisconnectedGeometry { end: row.end });
        };
        if first.ray != ray.label()
            || !valid_endpoint_interval(first, ray.length_mm())
            || first.s_a != 0.0
            || first.s_b < target
        {
            return Err(DesignedMaskError::DisconnectedGeometry { end: row.end });
        }
        let mut prior_s_b = None;
        while let Some(interval) = intervals.get(interval_index) {
            if interval.ray != ray.label() {
                break;
            }
            if !valid_endpoint_interval(interval, ray.length_mm())
                || prior_s_b.is_some_and(|s_b| interval.s_a < s_b)
            {
                return Err(DesignedMaskError::CorruptKernelResult);
            }
            digest.u64(u64::from_ne_bytes(i64::from(interval.ray).to_ne_bytes()));
            digest.f64(interval.s_a);
            digest.f64(interval.s_b);
            prior_s_b = Some(interval.s_b);
            interval_index += 1;
        }
    }
    if interval_index != intervals.len() {
        return Err(DesignedMaskError::CorruptKernelResult);
    }
    Ok(digest.finish())
}

fn valid_endpoint_interval(interval: &geom_kernel::Interval, length: f64) -> bool {
    interval.s_a.is_finite()
        && interval.s_b.is_finite()
        && interval.s_a >= 0.0
        && interval.s_a <= interval.s_b
        && interval.s_b <= length
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub(crate) enum DesignedMaskError {
    #[error("designed extent must be positive and finite millimetres")]
    InvalidExtent,
    #[error("designed extent must be shorter than its member")]
    ExtentOutOfSpan,
    #[error("member span must be finite and non-zero")]
    InvalidMemberSpan,
    #[error("terminalward and rootward extents must not overlap or consume a member")]
    OverlappingExtents,
    #[error("designed mask binding is stale for this member, end, or surface")]
    StaleBinding,
    #[error("an embedded member end needs its designed clearance extent")]
    MissingRequiredExtent { end: EmbedEnd },
    #[error("a designed extent was requested for an end the member does not declare: {end:?}")]
    EndNotDesigned { end: EmbedEnd },
    #[error("a designed extent was requested twice for the same member end: {end:?}")]
    DuplicateRequest { end: EmbedEnd },
    #[error("a designed extent was requested for a member that is not in this forest")]
    UnknownMember,
    #[error("endpoint mask validation limits are invalid")]
    InvalidValidationLimits,
    #[error("endpoint mask validation exceeded its admitted resource limit")]
    ResourceLimit,
    #[error("geometry kernel refused endpoint mask validation")]
    KernelRefusal,
    #[error("geometry kernel returned corrupt endpoint mask evidence")]
    CorruptKernelResult,
    #[error("designed embed geometry is not connected to its endpoint: {end:?}")]
    DisconnectedGeometry { end: EmbedEnd },
}

fn validate_extent(
    extent: PositiveFiniteMm,
    length: f64,
) -> Result<PositiveFiniteMm, DesignedMaskError> {
    if extent.get() >= length {
        Err(DesignedMaskError::ExtentOutOfSpan)
    } else {
        Ok(extent)
    }
}

fn member_length_mm(member: &Member) -> Result<f64, DesignedMaskError> {
    let length = capsule_rays(
        member.p0_mm().map(Finite::get),
        member.p1_mm().map(Finite::get),
        member.terminal_radius_mm().get().get(),
        member.rootward_radius_mm().get().get(),
        0,
        false,
    )
    .next()
    .expect("the centerline ray is always present")
    .length_mm();
    (length.is_finite() && length > 0.0)
        .then_some(length)
        .ok_or(DesignedMaskError::InvalidMemberSpan)
}
