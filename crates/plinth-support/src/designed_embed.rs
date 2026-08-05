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

use crate::forest::{mask_binding, ContactEnd, Forest, Member, MemberKey};

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
        end: ContactEnd,
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
    pub(crate) const fn end(&self, end: ContactEnd) -> Option<&DesignedEndMask> {
        match end {
            ContactEnd::Terminalward => self.terminalward(),
            ContactEnd::Rootward => self.rootward(),
        }
    }

    /// The order is a contract: terminalward before rootward, independent of
    /// construction order or thread count.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (ContactEnd, &DesignedEndMask)> {
        [
            (ContactEnd::Terminalward, self.terminalward.as_ref()),
            (ContactEnd::Rootward, self.rootward.as_ref()),
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
    pub(crate) end: ContactEnd,
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
        end: ContactEnd,
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
    requests: &[(usize, ContactEnd, PositiveFiniteMm)],
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
            if let Some(end) = member.model_contact_end() {
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
                ContactEnd::Terminalward => terminal = Some(extent),
                ContactEnd::Rootward => root = Some(extent),
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
        let declared = member.model_contact_end();
        for (_, end, _) in requested {
            if declared != Some(*end) {
                return Err(DesignedMaskError::EndNotDesigned { end: *end });
            }
        }
        if let Some(end) = declared {
            let present = match end {
                ContactEnd::Terminalward => terminal.is_some(),
                ContactEnd::Rootward => root.is_some(),
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
            ContactEnd::Terminalward => minted[row.member].terminalward = Some(mask),
            ContactEnd::Rootward => minted[row.member].rootward = Some(mask),
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
    end: ContactEnd,
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
            end: ContactEnd::Terminalward,
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
            end: ContactEnd::Rootward,
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
    digest.contact_end(Some(row.end));
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
    MissingRequiredExtent { end: ContactEnd },
    #[error("a designed extent was requested for an end the member does not declare: {end:?}")]
    EndNotDesigned { end: ContactEnd },
    #[error("a designed extent was requested twice for the same member end: {end:?}")]
    DuplicateRequest { end: ContactEnd },
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
    DisconnectedGeometry { end: ContactEnd },
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forest::{MemberKind, MemberSpec, Owner, RootwardEndRadius, TerminalEndRadius};

    fn finite(value: f64) -> Finite {
        Finite::new(value).unwrap()
    }

    fn radius(value: f64) -> PositiveFinite {
        PositiveFinite::new(value).unwrap()
    }

    fn extent(value: f64) -> PositiveFiniteMm {
        PositiveFiniteMm::new(value).unwrap()
    }

    fn append_cube(vertices: &mut Vec<[f64; 3]>, faces: &mut Vec<[u32; 3]>, x0: f64, x1: f64) {
        let base = u32::try_from(vertices.len()).unwrap();
        vertices.extend([
            [x0, 0.0, 0.0],
            [x1, 0.0, 0.0],
            [x1, 1.0, 0.0],
            [x0, 1.0, 0.0],
            [x0, 0.0, 1.0],
            [x1, 0.0, 1.0],
            [x1, 1.0, 1.0],
            [x0, 1.0, 1.0],
        ]);
        for [a, b, c] in [
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
        ] {
            faces.push([base + a, base + b, base + c]);
        }
    }

    fn surface_from(vertices: &[[f64; 3]], faces: &[[u32; 3]], pitch: f64) -> NativeSurface {
        NativeSurface::from_indexed(
            vertices,
            faces,
            geom_kernel::NativeSurfaceConfig {
                grid_pitch_mm: pitch,
                max_grid_bytes: 32 * 1024 * 1024,
            },
        )
        .unwrap()
    }

    fn surface_with_cubes(cubes: &[(f64, f64)]) -> NativeSurface {
        let mut vertices = Vec::new();
        let mut faces = Vec::new();
        for &(x0, x1) in cubes {
            append_cube(&mut vertices, &mut faces, x0, x1);
        }
        surface_from(&vertices, &faces, 0.25)
    }

    /// A tessellated sphere: the fixture the suite lacked. Every existing
    /// fixture is a flat-faced box, so nothing measured what convex curvature
    /// does to a rim ray that starts inside the solid.
    fn sphere_surface(radius_mm: f64, segments: usize, rings: usize) -> NativeSurface {
        let mut vertices = vec![[0.0, 0.0, radius_mm]];
        for ring in 1..rings {
            let phi = std::f64::consts::PI * (ring as f64) / (rings as f64);
            for segment in 0..segments {
                let theta = 2.0 * std::f64::consts::PI * (segment as f64) / (segments as f64);
                vertices.push([
                    radius_mm * phi.sin() * theta.cos(),
                    radius_mm * phi.sin() * theta.sin(),
                    radius_mm * phi.cos(),
                ]);
            }
        }
        vertices.push([0.0, 0.0, -radius_mm]);
        let south = u32::try_from(vertices.len() - 1).unwrap();
        let ring_base = |ring: usize| u32::try_from(1 + (ring - 1) * segments).unwrap();
        let mut faces = Vec::new();
        for segment in 0..segments {
            let a = ring_base(1) + u32::try_from(segment).unwrap();
            let b = ring_base(1) + u32::try_from((segment + 1) % segments).unwrap();
            faces.push([0, a, b]);
        }
        for ring in 1..rings - 1 {
            for segment in 0..segments {
                let next = (segment + 1) % segments;
                let a = ring_base(ring) + u32::try_from(segment).unwrap();
                let b = ring_base(ring) + u32::try_from(next).unwrap();
                let c = ring_base(ring + 1) + u32::try_from(next).unwrap();
                let d = ring_base(ring + 1) + u32::try_from(segment).unwrap();
                faces.push([a, d, c]);
                faces.push([a, c, b]);
            }
        }
        for segment in 0..segments {
            let a = ring_base(rings - 1) + u32::try_from(segment).unwrap();
            let b = ring_base(rings - 1) + u32::try_from((segment + 1) % segments).unwrap();
            faces.push([a, south, b]);
        }
        surface_from(&vertices, &faces, 0.5)
    }

    fn validation_limits(surface: &NativeSurface, rows: usize) -> EndpointMaskValidationLimits {
        EndpointMaskValidationLimits::for_batch(surface, rows, extent(0.01), 8).unwrap()
    }

    fn spec(
        p0: [f64; 3],
        p1: [f64; 3],
        r: f64,
        model_contact_end: Option<ContactEnd>,
        ordinal: u32,
    ) -> MemberSpec {
        MemberSpec {
            p0_mm: p0.map(finite),
            p1_mm: p1.map(finite),
            r0_mm: TerminalEndRadius::new(radius(r)),
            r1_mm: RootwardEndRadius::new(radius(r)),
            kind: MemberKind::Shoot,
            owner: Owner::Generated {
                family: "endpoint-mask".into(),
                ordinal,
            },
            model_contact_end,
            carried_mm2: finite(1.0),
        }
    }

    fn embedded_member(p0: f64, p1: f64, model_contact_end: Option<ContactEnd>) -> Member {
        Member::new(spec(
            [p0, 0.5, 0.5],
            [p1, 0.5, 0.5],
            0.05,
            model_contact_end,
            0,
        ))
    }

    /// Members are spread along y so each one is a distinct row, and the ends
    /// alternate so the batch exercises both row shapes at once.
    fn spread_forest(count: usize) -> (Forest, Vec<MemberKey>) {
        let mut forest = Forest::new();
        let keys = (0..count)
            .map(|index| {
                let y = 0.2 + 0.15 * (index as f64);
                let end = if index % 2 == 0 {
                    ContactEnd::Terminalward
                } else {
                    ContactEnd::Rootward
                };
                let (p0, p1) = match end {
                    ContactEnd::Terminalward => ([0.2, y, 0.5], [0.8, y, 0.5]),
                    ContactEnd::Rootward => ([0.8, y, 0.5], [0.2, y, 0.5]),
                };
                forest.add_member(Member::new(spec(
                    p0,
                    p1,
                    0.05,
                    Some(end),
                    u32::try_from(index).unwrap(),
                )))
            })
            .collect::<Vec<_>>();
        (forest, keys)
    }

    fn spread_requests(forest: &Forest, keys: &[MemberKey]) -> Vec<DesignedMaskRequest> {
        keys.iter()
            .map(|key| DesignedMaskRequest {
                member: *key,
                end: forest.member(*key).unwrap().model_contact_end().unwrap(),
                extent: extent(0.1),
            })
            .collect()
    }

    #[test]
    fn whole_forest_validation_issues_exactly_one_kernel_call() {
        let surface = surface_with_cubes(&[(0.0, 1.0)]);
        let (forest, keys) = spread_forest(5);
        let requests = spread_requests(&forest, &keys);
        assert_eq!(requests.len(), 5);
        reset_endpoint_validation_kernel_calls();
        assert_eq!(endpoint_validation_kernel_calls(), 0);
        let table = DesignedMaskTable::validate_forest(
            &forest,
            &surface,
            &requests,
            validation_limits(&surface, requests.len()),
        )
        .unwrap();
        // N members must cost one native call, not N.
        assert_eq!(endpoint_validation_kernel_calls(), 1);
        assert_eq!(table.len(), keys.len());
        for key in &keys {
            let member = forest.member(*key).unwrap();
            let end = member.model_contact_end().unwrap();
            assert_eq!(
                table.required_extent(*key, member, end, &surface),
                Ok(extent(0.1))
            );
        }
        assert_eq!(
            table
                .iter()
                .flat_map(|(_, masks)| masks.iter().map(|(end, _)| end))
                .count(),
            5
        );
    }

    #[test]
    fn a_single_bad_row_refuses_the_whole_batch_before_any_kernel_call() {
        for value in [0.0, -0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                PositiveFiniteMm::new(value),
                Err(DesignedMaskError::InvalidExtent)
            );
        }
        let surface = surface_with_cubes(&[(0.0, 1.0)]);
        let (forest, keys) = spread_forest(4);
        let good = spread_requests(&forest, &keys);
        let limits = validation_limits(&surface, 8);

        let refuse = |mutate: &dyn Fn(&mut Vec<DesignedMaskRequest>)| {
            let mut requests = good.clone();
            mutate(&mut requests);
            reset_endpoint_validation_kernel_calls();
            let outcome = DesignedMaskTable::validate_forest(&forest, &surface, &requests, limits);
            assert_eq!(endpoint_validation_kernel_calls(), 0);
            outcome.expect_err("a single bad row refuses the batch")
        };

        assert_eq!(
            refuse(&|requests| requests[2].extent = extent(0.61)),
            DesignedMaskError::ExtentOutOfSpan
        );
        assert_eq!(
            refuse(&|requests| {
                let mut both = requests[1];
                both.end = ContactEnd::Terminalward;
                both.extent = extent(0.31);
                requests[1].extent = extent(0.31);
                requests.push(both);
            }),
            DesignedMaskError::OverlappingExtents
        );
        assert_eq!(
            refuse(&|requests| {
                requests.remove(3);
            }),
            DesignedMaskError::MissingRequiredExtent {
                end: ContactEnd::Rootward,
            }
        );
        assert_eq!(
            refuse(&|requests| requests[0].end = ContactEnd::Rootward),
            DesignedMaskError::EndNotDesigned {
                end: ContactEnd::Rootward,
            }
        );
        assert_eq!(
            refuse(&|requests| {
                let duplicate = requests[2];
                requests.push(duplicate);
            }),
            DesignedMaskError::DuplicateRequest {
                end: ContactEnd::Terminalward,
            }
        );
        assert_eq!(
            refuse(&|requests| {
                requests[0].member = MemberKey::transit_test_fixture(999, 0);
            }),
            DesignedMaskError::UnknownMember
        );

        reset_endpoint_validation_kernel_calls();
        let starved = EndpointMaskValidationLimits::new(
            extent(0.01),
            8,
            PenetrationBatchLimits {
                max_rows: 3,
                ..limits.penetration
            },
        )
        .unwrap();
        assert_eq!(
            DesignedMaskTable::validate_forest(&forest, &surface, &good, starved),
            Err(DesignedMaskError::ResourceLimit)
        );
        assert_eq!(endpoint_validation_kernel_calls(), 0);
    }

    #[test]
    fn degenerate_member_spans_refuse_before_any_kernel_call() {
        let surface = surface_with_cubes(&[(0.0, 1.0)]);
        reset_endpoint_validation_kernel_calls();
        let limits = validation_limits(&surface, 2);
        for (p0, p1, extent_mm) in [
            ([0.0, 0.0, 0.0], [1e200, 0.0, 0.0], 1e300),
            ([-1e308, 0.0, 0.0], [1e308, 0.0, 0.0], 1.0),
            ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
        ] {
            let member = Member::new(spec(p0, p1, 0.2, Some(ContactEnd::Terminalward), 0));
            assert_eq!(
                validate_members_batch(
                    &surface,
                    &[&member],
                    &[(0, ContactEnd::Terminalward, extent(extent_mm))],
                    limits,
                ),
                Err(DesignedMaskError::InvalidMemberSpan)
            );
        }
        assert_eq!(endpoint_validation_kernel_calls(), 0);
    }

    #[test]
    fn row_order_is_member_then_terminal_then_root_regardless_of_request_order() {
        let surface = surface_with_cubes(&[(0.0, 1.0)]);
        let (forest, keys) = spread_forest(4);
        let requests = spread_requests(&forest, &keys);
        let bindings = |requests: &[DesignedMaskRequest]| {
            DesignedMaskTable::validate_forest(
                &forest,
                &surface,
                requests,
                validation_limits(&surface, requests.len()),
            )
            .unwrap()
            .iter()
            .map(|(key, masks)| {
                (
                    key,
                    masks
                        .iter()
                        .map(|(end, mask)| (end, mask.validation_binding()))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
        };
        let baseline = bindings(&requests);
        assert_eq!(baseline.len(), 4);
        let mut reversed = requests.clone();
        reversed.reverse();
        assert_eq!(bindings(&reversed), baseline);
        let mut rotated = requests.clone();
        rotated.rotate_left(3);
        assert_eq!(bindings(&rotated), baseline);

        // The terminal-before-root iteration contract holds even when a single
        // member carries both ends.
        let mut forest = Forest::new();
        let key = forest.add_member(embedded_member(0.2, 0.8, None));
        let member = forest.member(key).unwrap();
        let masks = validate_members_batch(
            &surface,
            &[member],
            &[
                (0, ContactEnd::Rootward, extent(0.1)),
                (0, ContactEnd::Terminalward, extent(0.1)),
            ],
            validation_limits(&surface, 2),
        );
        assert_eq!(
            masks.unwrap_err(),
            DesignedMaskError::EndNotDesigned {
                end: ContactEnd::Terminalward,
            }
        );
    }

    #[test]
    fn rejects_a_request_for_an_end_the_member_does_not_declare() {
        let surface = surface_with_cubes(&[(0.0, 1.0)]);
        let member = embedded_member(0.2, 0.8, Some(ContactEnd::Terminalward));
        assert_eq!(
            validate_members_batch(
                &surface,
                &[&member],
                &[(0, ContactEnd::Rootward, extent(0.1))],
                validation_limits(&surface, 1),
            ),
            Err(DesignedMaskError::EndNotDesigned {
                end: ContactEnd::Rootward,
            })
        );
        let unembedded = embedded_member(0.2, 0.8, None);
        assert_eq!(
            validate_members_batch(
                &surface,
                &[&unembedded],
                &[(0, ContactEnd::Terminalward, extent(0.1))],
                validation_limits(&surface, 1),
            ),
            Err(DesignedMaskError::EndNotDesigned {
                end: ContactEnd::Terminalward,
            })
        );
    }

    #[test]
    fn a_member_without_a_declared_contact_end_needs_no_designed_extent() {
        let surface = surface_with_cubes(&[(0.0, 1.0)]);
        let mut forest = Forest::new();
        let key = forest.add_member(embedded_member(0.2, 0.8, None));
        reset_endpoint_validation_kernel_calls();
        let table = DesignedMaskTable::validate_forest(
            &forest,
            &surface,
            &[],
            validation_limits(&surface, 1),
        )
        .unwrap();
        assert!(table.is_empty());
        assert_eq!(endpoint_validation_kernel_calls(), 0);
        assert_eq!(
            table.required_extent(
                key,
                forest.member(key).unwrap(),
                ContactEnd::Terminalward,
                &surface,
            ),
            Err(DesignedMaskError::MissingRequiredExtent {
                end: ContactEnd::Terminalward,
            })
        );
    }

    #[test]
    fn disconnected_geometry_is_rejected_and_the_binding_covers_member_end_and_surface() {
        let disconnected = surface_with_cubes(&[(0.0, 0.35), (0.55, 1.0)]);
        let member = embedded_member(0.1, 0.9, Some(ContactEnd::Terminalward));
        assert_eq!(
            validate_members_batch(
                &disconnected,
                &[&member],
                &[(0, ContactEnd::Terminalward, extent(0.45))],
                validation_limits(&disconnected, 1),
            ),
            Err(DesignedMaskError::DisconnectedGeometry {
                end: ContactEnd::Terminalward,
            })
        );

        let surface = surface_with_cubes(&[(0.0, 1.0)]);
        let mut forest = Forest::new();
        let key = forest.add_member(embedded_member(0.2, 0.8, Some(ContactEnd::Terminalward)));
        let requests = [DesignedMaskRequest {
            member: key,
            end: ContactEnd::Terminalward,
            extent: extent(0.1),
        }];
        let table = DesignedMaskTable::validate_forest(
            &forest,
            &surface,
            &requests,
            validation_limits(&surface, 1),
        )
        .unwrap();
        assert_eq!(
            table.required_extent(
                key,
                forest.member(key).unwrap(),
                ContactEnd::Terminalward,
                &surface,
            ),
            Ok(extent(0.1))
        );
        let rebound = surface_with_cubes(&[(0.0, 1.1)]);
        assert_eq!(
            table.required_extent(
                key,
                forest.member(key).unwrap(),
                ContactEnd::Terminalward,
                &rebound,
            ),
            Err(DesignedMaskError::StaleBinding)
        );
    }

    /// Any mutation that could change the member invalidates prior validation.
    /// One case per mutator, so a future mutator that forgets to bump
    /// `geom_version` fails here rather than silently reusing stale evidence.
    #[test]
    fn every_member_mutation_invalidates_a_prior_validation() {
        let surface = surface_with_cubes(&[(0.0, 1.0)]);
        type Mutator = &'static dyn Fn(&mut Forest, MemberKey);
        let mutators: [(&str, Mutator); 4] = [
            ("set_endpoints", &|forest: &mut Forest, key: MemberKey| {
                forest
                    .set_endpoints(
                        key,
                        [finite(0.25), finite(0.5), finite(0.5)],
                        [finite(0.8), finite(0.5), finite(0.5)],
                    )
                    .unwrap();
            }),
            ("set_radii", &|forest: &mut Forest, key: MemberKey| {
                forest
                    .set_radii(
                        key,
                        TerminalEndRadius::new(radius(0.06)),
                        RootwardEndRadius::new(radius(0.06)),
                    )
                    .unwrap();
            }),
            ("set_kind", &|forest: &mut Forest, key: MemberKey| {
                forest.set_kind(key, MemberKind::Trunk).unwrap();
            }),
            (
                "set_model_contact_end",
                &|forest: &mut Forest, key: MemberKey| {
                    forest
                        .set_model_contact_end(key, Some(ContactEnd::Rootward))
                        .unwrap();
                },
            ),
        ];
        for (name, mutate) in mutators {
            let mut forest = Forest::new();
            let key = forest.add_member(embedded_member(0.2, 0.8, Some(ContactEnd::Terminalward)));
            let table = DesignedMaskTable::validate_forest(
                &forest,
                &surface,
                &[DesignedMaskRequest {
                    member: key,
                    end: ContactEnd::Terminalward,
                    extent: extent(0.1),
                }],
                validation_limits(&surface, 1),
            )
            .unwrap();
            assert_eq!(
                table.required_extent(
                    key,
                    forest.member(key).unwrap(),
                    ContactEnd::Terminalward,
                    &surface,
                ),
                Ok(extent(0.1)),
                "{name} baseline"
            );
            mutate(&mut forest, key);
            assert_eq!(
                table.required_extent(
                    key,
                    forest.member(key).unwrap(),
                    ContactEnd::Terminalward,
                    &surface,
                ),
                Err(DesignedMaskError::StaleBinding),
                "{name} must invalidate prior validation"
            );
        }
    }

    /// The connectivity proof caps a requested extent at the true embedded
    /// depth: it can never justify an inset that reaches outside the solid.
    #[test]
    fn validation_refuses_an_extent_deeper_than_the_solid() {
        let surface = surface_with_cubes(&[(0.0, 1.0)]);
        // p0 sits 0.15 mm inside the x = 1.0 face and the body leaves the solid.
        let member = Member::new(spec(
            [0.85, 0.5, 0.5],
            [2.0, 0.5, 0.5],
            0.05,
            Some(ContactEnd::Terminalward),
            0,
        ));
        let limits = validation_limits(&surface, 1);
        assert!(validate_members_batch(
            &surface,
            &[&member],
            &[(0, ContactEnd::Terminalward, extent(0.09))],
            limits,
        )
        .is_ok());
        assert_eq!(
            validate_members_batch(
                &surface,
                &[&member],
                &[(0, ContactEnd::Terminalward, extent(0.225))],
                limits,
            ),
            Err(DesignedMaskError::DisconnectedGeometry {
                end: ContactEnd::Terminalward,
            })
        );
    }

    /// Quantifies the margin question a contact stub faces on a real model: the
    /// rim rays start at `p0 + unit * r0`, so on a convex surface they leave the
    /// solid *before* the centerline does. Requesting exactly the placement
    /// depth is therefore refused on any curved model, and the margin needed
    /// scales as `r0^2 / (2 R)`.
    #[test]
    fn a_convex_surface_refuses_an_extent_equal_to_the_placement_depth() {
        let sphere_radius_mm = 5.0;
        let surface = sphere_surface(sphere_radius_mm, 96, 48);
        let depth_mm = 0.3;
        let stub_radius_mm = 0.35;
        // A contact stub placed exactly the way `initial.rs` places one: the
        // axis is the contact normal, so there is no incidence angle at all.
        let member = Member::new(spec(
            [sphere_radius_mm - depth_mm, 0.0, 0.0],
            [sphere_radius_mm + 3.0, 0.0, 0.0],
            stub_radius_mm,
            Some(ContactEnd::Terminalward),
            0,
        ));
        let limits =
            EndpointMaskValidationLimits::for_batch(&surface, 1, extent(0.002), 8).unwrap();
        assert_eq!(
            validate_members_batch(
                &surface,
                &[&member],
                &[(0, ContactEnd::Terminalward, extent(depth_mm))],
                limits,
            ),
            Err(DesignedMaskError::DisconnectedGeometry {
                end: ContactEnd::Terminalward,
            }),
            "an extent equal to the placement depth must be refused on a convex surface"
        );
        // r0^2 / (2 R) = 0.35^2 / 10 = 0.01225 mm, so a 0.02 mm margin clears it.
        assert!(
            validate_members_batch(
                &surface,
                &[&member],
                &[(0, ContactEnd::Terminalward, extent(depth_mm - 0.02))],
                limits,
            )
            .is_ok(),
            "a margin larger than r0^2 / (2 R) must validate"
        );
    }

    #[test]
    fn endpoint_validator_handles_reversed_root_coordinates() {
        let surface = surface_with_cubes(&[(0.0, 1.0)]);
        let member = embedded_member(-0.2, 0.8, Some(ContactEnd::Rootward));
        let masks = validate_members_batch(
            &surface,
            &[&member],
            &[(0, ContactEnd::Rootward, extent(0.1))],
            validation_limits(&surface, 1),
        )
        .unwrap();
        assert_eq!(
            masks[0].iter().map(|(end, _)| end).collect::<Vec<_>>(),
            vec![ContactEnd::Rootward]
        );
        assert_eq!(
            masks[0].rootward().unwrap().validated_extent_for(
                &member,
                ContactEnd::Rootward,
                &surface
            ),
            Ok(extent(0.1))
        );
        // The rootward row swaps p0/p1 and r0/r1, so the proof walks outward
        // from p1 even though the member's stored p0 lies outside the solid.
        assert_eq!(
            masks[0].terminalward().map(|_| ()),
            None,
            "only the declared end is minted"
        );
    }

    /// A real 1/4/16 thread sweep: the kernel re-reads `PLINTH_THREADS` on every
    /// batch call, so the sweep only means anything if the child process
    /// actually runs under the requested count.
    #[test]
    fn endpoint_validation_is_thread_setting_independent() {
        const CHILD: &str = "PLINTH_EMBED_MASK_SWEEP_CHILD";
        const LINE: &str = "EMBED_MASK_SWEEP=";
        if std::env::var(CHILD).is_ok() {
            let surface = surface_with_cubes(&[(0.0, 1.0)]);
            let (forest, keys) = spread_forest(4);
            let requests = spread_requests(&forest, &keys);
            let table = DesignedMaskTable::validate_forest(
                &forest,
                &surface,
                &requests,
                validation_limits(&surface, requests.len()),
            )
            .unwrap();
            let payload = table
                .iter()
                .flat_map(|(_, masks)| {
                    masks
                        .iter()
                        .map(|(_, mask)| format!("{:016x}", mask.validation_binding()))
                })
                .collect::<Vec<_>>();
            println!(
                "{LINE}{}:{}:{}",
                geom_kernel::current_threads(),
                payload.len(),
                payload.join(":")
            );
            return;
        }
        let test_name = "designed_embed::tests::endpoint_validation_is_thread_setting_independent";
        let mut payloads = Vec::new();
        for threads in [1, 4, 16] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", test_name, "--nocapture"])
                .env(CHILD, "1")
                .env("PLINTH_THREADS", threads.to_string())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8(output.stdout).unwrap();
            let row = stdout
                .lines()
                .find_map(|line| line.strip_prefix(LINE))
                .expect("child emitted a sweep payload");
            let mut fields = row.splitn(3, ':');
            let effective = fields.next().unwrap();
            let count = fields.next().unwrap();
            let payload = fields.next().unwrap();
            // Non-vacuity: the env var reached the kernel, and the payload is
            // not an empty table.
            assert_eq!(effective.parse::<usize>().unwrap(), threads);
            assert!(count.parse::<usize>().unwrap() >= 2);
            payloads.push(payload.to_owned());
        }
        assert!(payloads.windows(2).all(|pair| pair[0] == pair[1]));
    }
}
