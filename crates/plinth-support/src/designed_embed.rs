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

use crate::contact::ContactPolicy;
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

/// Gather every requested endpoint across every member into one ordered row set,
/// applying every scalar refusal before any native work is admitted.
///
/// Row order is a contract: member index, then terminalward before rootward.
/// Sorting here makes the minted table independent of caller order, and the
/// kernel's index-parallel batch makes it independent of thread count.
fn prepare_rows(
    members: &[&Member],
    requests: &[(usize, ContactEnd, PositiveFiniteMm)],
) -> Result<Vec<EndpointValidationRow>, DesignedMaskError> {
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
    Ok(rows)
}

/// Exactly one native `penetration_intervals_batch_bounded` for the whole row
/// set, scattered back to a PER-ROW engagement measurement.
///
/// `Err` here is always a BATCH-level integrity failure — a starved resource
/// admission, a kernel refusal, or a result whose shape does not match the rows
/// that were sent. Nothing can be concluded about any row from it. Geometry
/// verdicts are values in the returned rows, which is what lets PR-8 route a
/// refusal to quarantine instead of failing a pipeline (OD-4).
fn witness_batch(
    surface: &NativeSurface,
    rows: &[EndpointValidationRow],
    limits: EndpointMaskValidationLimits,
) -> Result<Vec<EndpointEngagement>, DesignedMaskError> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    if rows.len() > limits.penetration.max_rows {
        return Err(DesignedMaskError::ResourceLimit);
    }

    let mut p0s = Vec::with_capacity(rows.len());
    let mut p1s = Vec::with_capacity(rows.len());
    let mut r0s = Vec::with_capacity(rows.len());
    let mut r1s = Vec::with_capacity(rows.len());
    for row in rows {
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
    rows.iter()
        .zip(interval_rows)
        .map(|(row, intervals)| endpoint_engagement(row, &intervals))
        .collect()
}

fn mint_mask(
    member: &Member,
    row: &EndpointValidationRow,
    witness: u64,
    surface_identity: SurfaceTriangleSoupDigest,
) -> DesignedEndMask {
    DesignedEndMask {
        requested_extent_mm: row.extent,
        witness,
        validation_binding: mask_binding(member, row.end, row.extent, witness, surface_identity),
        surface_identity,
    }
}

/// Whole-batch endpoint-connectivity validation: gather every requested endpoint
/// across every member into one set of rows, issue exactly one native call, then
/// scatter the evidence back index-aligned with `members`.
///
/// Requests are member-indexed rather than keyed so that provisional candidate
/// members can be validated without minting forgeable forest keys.
///
/// This entry point is TOTAL: one refused row refuses the whole mint. Callers
/// that must survive an individual refusal — every PR-8 attestation caller,
/// because OD-4 forbids a hard pipeline failure — use
/// [`attest_members_batch`] instead.
pub(crate) fn validate_members_batch(
    surface: &NativeSurface,
    members: &[&Member],
    requests: &[(usize, ContactEnd, PositiveFiniteMm)],
    limits: EndpointMaskValidationLimits,
) -> Result<Vec<DesignedClearanceMasks>, DesignedMaskError> {
    let rows = prepare_rows(members, requests)?;
    let mut minted = vec![DesignedClearanceMasks::default(); members.len()];
    let engagements = witness_batch(surface, &rows, limits)?;
    let surface_identity = surface.source_triangle_soup_digest();
    for (row, engagement) in rows.iter().zip(engagements) {
        // TOTALITY, not mere attachment. A mask tells the distance check to
        // ignore this extent of the member; any ray that leaves the solid
        // inside it is geometry that would then go unchecked.
        if !engagement.is_total() {
            return Err(DesignedMaskError::DisconnectedGeometry { end: row.end });
        }
        let mask = mint_mask(
            members[row.member],
            row,
            engagement.witness,
            surface_identity,
        );
        match row.end {
            ContactEnd::Terminalward => minted[row.member].terminalward = Some(mask),
            ContactEnd::Rootward => minted[row.member].rootward = Some(mask),
        }
    }
    Ok(minted)
}

/// One PR-8 attestation request: a declared model-contact end, the design-time
/// extent that end claims, and the OD-9 exterior-ness probe for its attachment
/// point.
///
/// The probe is supplied by the caller rather than derived here because the
/// caller is the only party that knows the OUTWARD sense of the contact normal.
/// A probe pointing the wrong way would invert the screen silently, so the
/// contract is stated once, here, and pinned by fixture: `probe` must be the
/// attachment point displaced OUTWARD along the surface normal by the run's own
/// process resolution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ContactAttestationRequest {
    pub(crate) member: usize,
    pub(crate) end: ContactEnd,
    pub(crate) extent: PositiveFiniteMm,
    pub(crate) exterior_probe: V3,
    /// The member's radius AT this end — the nominal interface disc PR-4 would
    /// have credited without a measurement.
    pub(crate) end_radius_mm: f64,
    /// The model area routed onto this tip. This is MAJOR-7: PR-4 was inert
    /// while `contact::admit` was called with `carried_mm2 = 0.0`, because with
    /// zero load every interface is sufficient.
    pub(crate) carried_mm2: f64,
    /// PR-4's non-regression clause. The tip-sizing policy asked for a larger
    /// radius and the registered `tip_radius_max_legacy_mm` clamp denied it, so
    /// any area deficiency at this tip is that pre-existing clamp rather than a
    /// contact defect this package introduced. Recorded as evidence, never
    /// refused — see FU-CONTACT-1, and note that it is load-bearing: on
    /// `make_box` two clamped 1.0 mm tips carry 50 mm^2 each against a
    /// `pi r^2 = 3.14 mm^2` interface, so without this clause a fully engaged,
    /// today-printing contact would be deleted by the area law.
    pub(crate) tip_clamped_at_max: bool,
}

/// Why one proposed attachment failed to attest. Every variant is an ordinary
/// candidate refusal routed to quarantine (OD-4), never a pipeline failure.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ContactAttestationRefusal {
    /// OD-9 — the offset probe tested INSIDE the model, so the surface this
    /// contact proposes to attach to is buried: it lies within the printed
    /// solid and will not exist after the per-layer union. Note that this is
    /// exactly the case naive verification passes: the foot really is "on the
    /// surface", and an embedded tip's whole neighbourhood really is interior,
    /// so the endpoint-connectivity proof succeeds TRIVIALLY there.
    BuriedSurface { end: ContactEnd },
    /// PR-8 — NO sampled part of this end is in the solid over even one
    /// process resolution. There is no interface here at all, so there is
    /// nothing for the area law to grade.
    NotAttached { end: ContactEnd },
}

impl ContactAttestationRefusal {
    #[must_use]
    pub(crate) const fn end(self) -> ContactEnd {
        match self {
            Self::BuriedSurface { end } | Self::NotAttached { end } => end,
        }
    }
}

/// PR-4 evaluated on the ATTESTED interface rather than the nominal one: this
/// tip IS attached, but over measurably less area than the tip-sizing policy
/// assumed, and the H2 area-ratio allowance does not cover its routed load on
/// what is actually there.
///
/// **This is recorded, not refused, and the disposition is deliberate.**
///
/// The measurement is real and it is the owner's governing requirement made
/// quantitative — a tip must attach "all along the area of the mesh that
/// requires the contact", and here it does not. But the remedy for a contact
/// placed within a tip radius of a model silhouette edge is to move or shrink
/// the contact, which belongs to the perception/sizing stages. Deleting it
/// leaves ZERO support where a partial one stood, which is the worse print
/// outcome of the two and is precisely what "never let supports silently
/// vanish" forbids. Every affected contact on the reachable corpus prints
/// today.
///
/// So the deficit is surfaced as evidence, carried in the attestation digest so
/// a regression in it cannot hide, and the refuse-or-resize disposition is
/// referred to the owner as a print-consequence tradeoff with no
/// physics-preferred answer (OD-8's own carve-out).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ContactAreaDeficit {
    pub(crate) end: ContactEnd,
    /// `r_end` — what PR-4 would have credited without a measurement.
    pub(crate) nominal_radius_mm: f64,
    /// `r_end * sqrt(rim_engaged / rim_total)` — what is actually engaged.
    pub(crate) attested_radius_mm: f64,
    pub(crate) carried_mm2: f64,
}

/// Per-row outcome of [`attest_members_batch`], in ROW order (member index,
/// then terminalward before rootward) rather than caller order. Callers read
/// [`ContactAttestation::member`] rather than relying on their own ordering.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContactAttestation {
    pub(crate) member: usize,
    pub(crate) end: ContactEnd,
    /// What the kernel measured, refused or not — so a caller can report how
    /// close a refusal was, and a census can watch an attested contact drift
    /// toward its boundary before it crosses.
    pub(crate) engagement: EndpointEngagement,
    /// The attested effective interface radius.
    pub(crate) attested_radius_mm: f64,
    /// PR-4 on the attested interface: `Some` when the engaged area cannot
    /// carry the routed load. Evidence, not a refusal — see
    /// [`ContactAreaDeficit`].
    pub(crate) area_deficit: Option<ContactAreaDeficit>,
    /// `Ok` carries the minted mask only when the engagement was TOTAL; a
    /// partial but sufficient contact attests with `Ok(None)`, because it has
    /// no validated extent to mask with.
    pub(crate) verdict: Result<Option<DesignedEndMask>, ContactAttestationRefusal>,
}

/// **PR-8 `AFFIRMATIVE_CONTACT_ATTESTATION`** — the production entry point.
///
/// Every proposed model contact in a stage is attested in ONE pass with exactly
/// TWO batched native calls, whatever the row count:
///
/// 1. one `inside` batch carrying every OD-9 exterior-ness probe, and
/// 2. one `penetration_intervals_batch_bounded` carrying every endpoint
///    connectivity row (issued by [`witness_batch`]).
///
/// They cannot be folded into a single kernel entry point — a parity query and a
/// capsule-ray penetration sweep are different kernels — but neither scales with
/// the row count, which is what the batch-first rule actually requires. A
/// per-contact `inside` call here would be the defect.
///
/// The two screens are INDEPENDENT and neither subsumes the other, which is the
/// whole point of OD-9: a touching foot seated on a buried patch is genuinely
/// "on the surface", and an embedded tip inside a buried region passes the
/// connectivity proof TRIVIALLY because its entire neighbourhood is interior.
/// The exterior screen is therefore evaluated FIRST and reported as its own
/// refusal, so a buried contact can never be laundered into an attested one by
/// the connectivity evidence it trivially satisfies.
///
/// The outer `Err` remains a batch-level integrity failure (starved admission,
/// kernel refusal, malformed request set). Per-row geometry verdicts are inner
/// values, so a single refused contact costs that contact and nothing else.
pub(crate) fn attest_members_batch(
    surface: &NativeSurface,
    members: &[&Member],
    requests: &[ContactAttestationRequest],
    limits: EndpointMaskValidationLimits,
    policy: &ContactPolicy,
) -> Result<Vec<ContactAttestation>, DesignedMaskError> {
    let extents = requests
        .iter()
        .map(|request| (request.member, request.end, request.extent))
        .collect::<Vec<_>>();
    let rows = prepare_rows(members, &extents)?;

    // Row order is `prepare_rows`'s contract, not the caller's order, so the
    // per-row inputs are re-keyed through it rather than zipped positionally.
    let by_end = requests
        .iter()
        .map(|request| ((request.member, request.end), *request))
        .collect::<BTreeMap<_, _>>();
    let keyed = rows
        .iter()
        .map(|row| {
            by_end
                .get(&(row.member, row.end))
                .copied()
                .ok_or(DesignedMaskError::UnknownMember)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if keyed.iter().any(|request| {
        !request.exterior_probe.iter().all(|value| value.is_finite())
            || !request.end_radius_mm.is_finite()
            || request.end_radius_mm <= 0.0
            || !request.carried_mm2.is_finite()
            || request.carried_mm2 < 0.0
    }) {
        return Err(DesignedMaskError::InvalidExtent);
    }

    // ONE parity batch for every probe in the stage (OD-9 layer (b)).
    let probes = keyed
        .iter()
        .map(|request| request.exterior_probe)
        .collect::<Vec<_>>();
    let buried = if probes.is_empty() {
        Vec::new()
    } else {
        surface.inside(&probes)
    };
    if buried.len() != rows.len() {
        return Err(DesignedMaskError::CorruptKernelResult);
    }

    let engagements = witness_batch(surface, &rows, limits)?;
    let surface_identity = surface.source_triangle_soup_digest();
    Ok(rows
        .iter()
        .zip(engagements)
        .zip(buried)
        .zip(keyed)
        .map(|(((row, engagement), is_buried), request)| {
            let attested_radius_mm = engagement.effective_radius_mm(request.end_radius_mm);
            let verdict = if is_buried {
                // OD-9 is evaluated FIRST and reported as its own refusal, so a
                // buried contact can never be laundered into an attested one by
                // the connectivity evidence it trivially satisfies.
                Err(ContactAttestationRefusal::BuriedSurface { end: row.end })
            } else if !engagement.is_attached() {
                Err(ContactAttestationRefusal::NotAttached { end: row.end })
            } else if engagement.is_total() {
                Ok(Some(mint_mask(
                    members[row.member],
                    row,
                    engagement.witness,
                    surface_identity,
                )))
            } else {
                Ok(None)
            };
            // PR-4 is graded on every ATTESTED row. It is not evaluated on a
            // refused one: a buried or unattached contact has no interface, and
            // reporting an area deficit for it would be reporting a number
            // about geometry that is not there.
            let area_deficit = (verdict.is_ok()
                && !request.tip_clamped_at_max
                && !crate::contact::area_sufficient(
                    request.carried_mm2,
                    attested_radius_mm,
                    policy,
                ))
            .then_some(ContactAreaDeficit {
                end: row.end,
                nominal_radius_mm: request.end_radius_mm,
                attested_radius_mm,
                carried_mm2: request.carried_mm2,
            });
            ContactAttestation {
                member: row.member,
                end: row.end,
                engagement,
                attested_radius_mm,
                area_deficit,
                verdict,
            }
        })
        .collect())
}

/// One proposed model contact belonging to a family that attests CANDIDATE
/// geometry rather than a minted forest member.
///
/// The stub family attests members that already exist in the forest, because
/// `initial.rs` mints first and quarantines the refusals. The two TOUCHING
/// families cannot: a brace model rung and a CCO model anchor are both decided
/// by a tournament, so the only honest moment to attest them is at the SEAT,
/// while they are still candidates. `attest_members_batch` was written
/// member-INDEXED rather than key-indexed precisely so provisional geometry can
/// be validated without minting forgeable forest keys; this row set is that
/// contract made explicit.
///
/// Endpoint convention is the mint site's, verbatim: `p0`/`r0` are the
/// terminalward end and `p1`/`r1` the rootward one, and `end` names the end
/// that meets the model. A row whose `end` does not match the geometry it
/// describes is a caller defect, and it fails closed in `prepare_rows`
/// (`EndNotDesigned`) rather than attesting the wrong end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SeatAttestationRow {
    pub(crate) p0_mm: V3,
    pub(crate) p1_mm: V3,
    pub(crate) r0_mm: f64,
    pub(crate) r1_mm: f64,
    pub(crate) end: ContactEnd,
    /// The attachment point displaced OUTWARD along the surface normal by one
    /// process resolution — OD-9's probe. Supplied by the caller for the same
    /// reason as [`ContactAttestationRequest::exterior_probe`]: only the mint
    /// site knows the outward sense.
    pub(crate) exterior_probe: V3,
    /// Routed demand on this seat. Zero for a bracing member (see the F2
    /// argument at `brace.rs`'s `contact::admit` call site); the terminal's
    /// real `carried_mm2` for a CCO model anchor, which IS a demand route.
    pub(crate) carried_mm2: f64,
}

impl SeatAttestationRow {
    /// The provisional member this row describes.
    ///
    /// `Owner::Generated { family: "seat_attestation" }` is deliberately a
    /// namespace that no mint site uses: this member exists only for the
    /// duration of the batch and must never be mistakable for placed geometry
    /// if it ever escapes into a debug dump.
    fn provisional_member(&self) -> Result<Member, DesignedMaskError> {
        let finite3 = |value: V3| -> Result<[Finite; 3], DesignedMaskError> {
            let mut out = [Finite::new(0.0).expect("zero is finite"); 3];
            for (slot, component) in out.iter_mut().zip(value) {
                *slot = Finite::new(component).map_err(|_| DesignedMaskError::InvalidExtent)?;
            }
            Ok(out)
        };
        let radius = |value: f64| {
            PositiveFinite::new(value).map_err(|_| DesignedMaskError::InvalidExtent)
        };
        Ok(Member::new(crate::forest::MemberSpec {
            p0_mm: finite3(self.p0_mm)?,
            p1_mm: finite3(self.p1_mm)?,
            r0_mm: crate::forest::TerminalEndRadius::new(radius(self.r0_mm)?),
            r1_mm: crate::forest::RootwardEndRadius::new(radius(self.r1_mm)?),
            kind: crate::forest::MemberKind::Other(SEAT_ATTESTATION_FAMILY.to_owned()),
            owner: crate::forest::Owner::Generated {
                family: SEAT_ATTESTATION_FAMILY.to_owned(),
                ordinal: 0,
            },
            model_contact_end: Some(self.end),
            carried_mm2: Finite::new(self.carried_mm2)
                .map_err(|_| DesignedMaskError::InvalidExtent)?,
        }))
    }

    fn end_radius_mm(&self) -> f64 {
        match self.end {
            ContactEnd::Terminalward => self.r0_mm,
            ContactEnd::Rootward => self.r1_mm,
        }
    }
}

const SEAT_ATTESTATION_FAMILY: &str = "seat_attestation_provisional";

/// **PR-8 / OD-9 at a candidate SEAT** — the touching families' entry point.
///
/// Identical physics to [`attest_members_batch`], and literally the same two
/// batched native calls, because it delegates to it: one `inside` batch
/// carrying every OD-9 probe in the stage, one
/// `penetration_intervals_batch_bounded` carrying every connectivity row. The
/// only difference is that the members are provisional, so a refusal costs the
/// CANDIDATE rather than requiring a quarantine transition on placed geometry.
///
/// Callers must issue this ONCE per stage over the whole candidate set. Calling
/// it per candidate would restore exactly the per-item native call the batched
/// verifier exists to prevent, and the structural gate counts the call sites.
pub(crate) fn attest_seats_batch(
    surface: &NativeSurface,
    seats: &[SeatAttestationRow],
    extent: PositiveFiniteMm,
    limits: EndpointMaskValidationLimits,
    policy: &ContactPolicy,
) -> Result<Vec<ContactAttestation>, DesignedMaskError> {
    if seats.is_empty() {
        return Ok(Vec::new());
    }
    let members = seats
        .iter()
        .map(SeatAttestationRow::provisional_member)
        .collect::<Result<Vec<_>, _>>()?;
    let borrowed = members.iter().collect::<Vec<_>>();
    let requests = seats
        .iter()
        .enumerate()
        .map(|(index, seat)| ContactAttestationRequest {
            member: index,
            end: seat.end,
            extent,
            exterior_probe: seat.exterior_probe,
            end_radius_mm: seat.end_radius_mm(),
            carried_mm2: seat.carried_mm2,
            // A candidate seat has no tip-sizing clamp to suppress: the clamp
            // clause exists for `initial.rs`'s load-derived tip radii, and
            // neither touching family derives its end radius from a load.
            tip_clamped_at_max: false,
        })
        .collect::<Vec<_>>();
    let outcomes = attest_members_batch(surface, &borrowed, &requests, limits, policy)?;
    if outcomes.len() != seats.len() {
        return Err(DesignedMaskError::CorruptKernelResult);
    }
    Ok(outcomes)
}

/// One stage's seat-attestation transcript, folded to a digest.
///
/// Same totality requirement as `initial.rs`'s `attestation_digest`, and for
/// the same reason: it covers ATTESTED rows too, because a refusal that stopped
/// firing leaves no geometric trace at all. `tag` names the family so two
/// stages cannot produce a colliding transcript.
pub(crate) fn seat_attestation_digest(
    tag: u8,
    extent: PositiveFiniteMm,
    seats: &[SeatAttestationRow],
    outcomes: &[ContactAttestation],
) -> u64 {
    let mut digest = crate::forest::DigestWriter::new();
    digest.tag(0x8c);
    digest.tag(tag);
    digest.f64(extent.get());
    digest.u64(seats.len() as u64);
    for outcome in outcomes {
        digest.u64(outcome.member as u64);
        digest.tag(match outcome.end {
            ContactEnd::Terminalward => 0,
            ContactEnd::Rootward => 1,
        });
        digest.f64(outcome.attested_radius_mm);
        digest.u64(outcome.engagement.rim_engaged as u64);
        digest.u64(outcome.engagement.rim_total as u64);
        digest.tag(u8::from(outcome.engagement.centreline));
        // The witness digests every interval the kernel returned, so a seat
        // that drifts inside its own verdict is still visible.
        digest.u64(outcome.engagement.witness);
        digest.tag(match &outcome.verdict {
            Ok(_) => 0,
            Err(ContactAttestationRefusal::BuriedSurface { .. }) => 1,
            Err(ContactAttestationRefusal::NotAttached { .. }) => 2,
        });
        match outcome.area_deficit {
            None => digest.tag(0),
            Some(deficit) => {
                digest.tag(1);
                digest.f64(deficit.attested_radius_mm);
                digest.f64(deficit.carried_mm2);
            }
        }
    }
    digest.finish()
}

/// Family tags for [`seat_attestation_digest`].
pub(crate) const SEAT_TAG_BRACE_MODEL_RUNG: u8 = 0x01;
pub(crate) const SEAT_TAG_CCO_MODEL_ANCHOR: u8 = 0x02;

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

/// How much of one member end's cross-section is actually fused to the model.
///
/// The rays are the same nine the mask proof uses — the centreline plus eight
/// capsule rim rays — but the outcome is graded rather than binary, which is the
/// difference between the two questions being asked:
///
/// * **Is masking this axial extent from the distance check safe?** That needs
///   TOTALITY: any ray that leaves the solid inside the mask is a part of the
///   member the check was told to ignore and should not have been. Hence
///   [`validate_members_batch`] still demands `is_total()`.
/// * **Is the tip attached, and over how much area?** That is a MEASUREMENT.
///   A contact placed within a tip radius of a model edge genuinely engages
///   only part of its end disc — it is not disconnected, it is smaller. Deleting
///   it would vanish a support that prints today; crediting it the full disc
///   would let the H2 area law gate against area that is not there. PR-8 does
///   neither: it measures the engaged fraction and hands it to PR-4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EndpointEngagement {
    /// The centreline penetrated contiguously from `s = 0` to the claimed
    /// extent. False means the endpoint is not in the solid at all.
    pub(crate) centreline: bool,
    /// How many of the offset rim rays did the same.
    pub(crate) rim_engaged: usize,
    /// How many were sampled. Always [`ENDPOINT_VALIDATION_OFFSET_RAYS`] for a
    /// non-degenerate member.
    pub(crate) rim_total: usize,
    /// Digest of every interval the kernel returned, in ray order.
    pub(crate) witness: u64,
}

impl EndpointEngagement {
    /// Every sampled ray reached the claimed extent — the totality the mask
    /// proof requires.
    #[must_use]
    pub(crate) const fn is_total(self) -> bool {
        self.centreline && self.rim_engaged == self.rim_total
    }

    /// Any part of the end at all is in the solid.
    ///
    /// The centreline deliberately does NOT get a veto here. A contact placed
    /// within a tip radius of a model silhouette edge has its tip CENTRE lying
    /// exactly on the boundary plane, which parity classifies as outside, while
    /// three or four rim rays are genuinely inside — measured on
    /// `make_overhang_table`, where eight such contacts exist and all eight
    /// print today. Treating the centre as decisive would delete them.
    #[must_use]
    pub(crate) const fn is_attached(self) -> bool {
        self.centreline || self.rim_engaged > 0
    }

    /// The radius of the concentric disc whose area equals the SAMPLED engaged
    /// area of the end disc: `r_eff = r_end * sqrt(rim_engaged / rim_total)`.
    ///
    /// This is the number PR-4's area law must be evaluated on, and its
    /// epistemic status is exactly the one the design record already assigns
    /// `A_contact`: a SAMPLING BOUND at eight rim samples, exact at the ends
    /// (`k = 0` and `k = rim_total`) and an estimate between them. For the
    /// dominant partial case — a straight edge cutting the disc — the sampled
    /// rim fraction equals the true area fraction exactly when the cut passes
    /// through the centre; off-centre cuts are UNDER-credited, which is the
    /// conservative direction for a load check.
    #[must_use]
    pub(crate) fn effective_radius_mm(self, end_radius_mm: f64) -> f64 {
        if self.rim_total == 0 {
            return 0.0;
        }
        end_radius_mm * ((self.rim_engaged as f64) / (self.rim_total as f64)).sqrt()
    }
}

/// Walk the kernel's interval rows in ray order and grade each ray.
///
/// The kernel emits nothing at all for a ray that never enters the solid, so
/// absence of a label is data, not corruption. Only an out-of-order, malformed,
/// or unattributable interval is a `CorruptKernelResult`.
fn endpoint_engagement(
    row: &EndpointValidationRow,
    intervals: &[geom_kernel::Interval],
) -> Result<EndpointEngagement, DesignedMaskError> {
    let rays = capsule_rays(
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
    let mut centreline = false;
    let mut rim_engaged = 0usize;
    let mut rim_total = 0usize;
    for ray in rays {
        let target = ray.length_mm() * row.fraction;
        if !target.is_finite() || target <= 0.0 {
            return Err(DesignedMaskError::CorruptKernelResult);
        }
        let is_centreline = ray.label() < 0;
        if !is_centreline {
            rim_total += 1;
        }
        let mut engaged = false;
        let mut prior_s_b: Option<f64> = None;
        while let Some(interval) = intervals.get(interval_index) {
            if interval.ray != ray.label() {
                break;
            }
            if !valid_endpoint_interval(interval, ray.length_mm())
                || prior_s_b.is_some_and(|s_b| interval.s_a < s_b)
            {
                return Err(DesignedMaskError::CorruptKernelResult);
            }
            if prior_s_b.is_none() {
                // Contiguity from the endpoint is what makes this evidence of
                // ATTACHMENT rather than of the member merely passing through
                // the model somewhere further along.
                engaged = interval.s_a == 0.0 && interval.s_b >= target;
            }
            digest.u64(u64::from_ne_bytes(i64::from(interval.ray).to_ne_bytes()));
            digest.f64(interval.s_a);
            digest.f64(interval.s_b);
            prior_s_b = Some(interval.s_b);
            interval_index += 1;
        }
        if is_centreline {
            centreline = engaged;
        } else if engaged {
            rim_engaged += 1;
        }
    }
    if interval_index != intervals.len() {
        return Err(DesignedMaskError::CorruptKernelResult);
    }
    Ok(EndpointEngagement {
        centreline,
        rim_engaged,
        rim_total,
        witness: digest.finish(),
    })
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

    // ---------------------------------------------------------------------
    // PR-8 / OD-9
    // ---------------------------------------------------------------------

    fn attestation_policy() -> ContactPolicy {
        ContactPolicy {
            floor_depth_mm: 0.3,
            process_resolution_mm: 0.05,
            standoff_mm: 0.6,
            min_free_span_mm: 0.05,
            ball_factor_stub: 1.5,
            tip_load: crate::perception::FrozenTipLoadPolicy::for_contact_tests(),
        }
    }

    /// A box spanning `[x0,x1] x [y0,y1] x [z0,z1]`, appended to a soup.
    fn append_box(
        vertices: &mut Vec<[f64; 3]>,
        faces: &mut Vec<[u32; 3]>,
        lo: [f64; 3],
        hi: [f64; 3],
    ) {
        let base = u32::try_from(vertices.len()).unwrap();
        vertices.extend([
            [lo[0], lo[1], lo[2]],
            [hi[0], lo[1], lo[2]],
            [hi[0], hi[1], lo[2]],
            [lo[0], hi[1], lo[2]],
            [lo[0], lo[1], hi[2]],
            [hi[0], lo[1], hi[2]],
            [hi[0], hi[1], hi[2]],
            [lo[0], hi[1], hi[2]],
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

    /// **The OD-9 acceptance vehicle.**
    ///
    /// Two overlapping boxes, each individually closed and correctly wound —
    /// the synthetic stand-in for the real failure mode, since no reachable
    /// corpus model self-intersects and the Weeping Treant does not yet reach
    /// the support stages. `A` spans `x in [0, 4]`, `B` spans `x in [2, 6]`,
    /// and they share `y, z in [0, 2]`.
    ///
    /// The plane `x = 4` is `A`'s `+x` face. Over `x in [2, 4]` that face is
    /// BURIED: it lies strictly inside `B`, so after the per-layer union it
    /// does not exist in the print. `B`'s `+x` face at `x = 6` is exterior.
    /// Both look identical to every check the engine had before OD-9.
    fn interpenetrating_pair() -> NativeSurface {
        let mut vertices = Vec::new();
        let mut faces = Vec::new();
        append_box(&mut vertices, &mut faces, [0.0, 0.0, 0.0], [4.0, 2.0, 2.0]);
        append_box(&mut vertices, &mut faces, [2.0, 0.0, 0.0], [6.0, 2.0, 2.0]);
        surface_from(&vertices, &faces, 0.1)
    }

    /// A stub that seats `depth` into the plane `x = face_x` from the `+x`
    /// side, exactly the way `initial.rs` mints one: axis along the contact
    /// normal, `p0` inside, `p1` out in free space.
    fn seated_stub(face_x: f64, y: f64, depth: f64, radius: f64) -> Member {
        Member::new(spec(
            [face_x - depth, y, 1.0],
            [face_x + 2.5, y, 1.0],
            radius,
            Some(ContactEnd::Terminalward),
            0,
        ))
    }

    #[test]
    fn od9_a_buried_face_is_refused_while_the_exterior_face_attests() {
        let surface = interpenetrating_pair();
        const DEPTH: f64 = 0.3;
        const EPS: f64 = 0.05;
        const RADIUS: f64 = 0.2;

        // NON-VACUITY, stated first: this fixture is only an OD-9 test if the
        // buried face really is buried and the exterior face really is not.
        // Assert the parity facts the screen rests on, so a fixture that
        // silently stopped self-intersecting fails here rather than passing
        // the test below for the wrong reason.
        assert_eq!(
            surface.inside(&[[4.0 + EPS, 1.0, 1.0], [6.0 + EPS, 1.0, 1.0]]),
            vec![true, false],
            "A's +x face at x=4 must be buried inside B; B's at x=6 must be exterior"
        );

        let buried = seated_stub(4.0, 1.0, DEPTH, RADIUS);
        let exterior = seated_stub(6.0, 1.0, DEPTH, RADIUS);
        let members = [&buried, &exterior];
        let requests = [
            ContactAttestationRequest {
                member: 0,
                end: ContactEnd::Terminalward,
                extent: extent(EPS),
                exterior_probe: [4.0 + EPS, 1.0, 1.0],
                end_radius_mm: RADIUS,
                carried_mm2: 0.1,
                tip_clamped_at_max: false,
            },
            ContactAttestationRequest {
                member: 1,
                end: ContactEnd::Terminalward,
                extent: extent(EPS),
                exterior_probe: [6.0 + EPS, 1.0, 1.0],
                end_radius_mm: RADIUS,
                carried_mm2: 0.1,
                tip_clamped_at_max: false,
            },
        ];
        reset_endpoint_validation_kernel_calls();
        let outcomes = attest_members_batch(
            &surface,
            &members,
            &requests,
            validation_limits(&surface, 2),
            &attestation_policy(),
        )
        .expect("the batch itself is well formed");

        // Batch-first: two contacts, ONE penetration batch.
        assert_eq!(endpoint_validation_kernel_calls(), 1);
        assert_eq!(outcomes.len(), 2);

        assert_eq!(
            outcomes[0].verdict,
            Err(ContactAttestationRefusal::BuriedSurface {
                end: ContactEnd::Terminalward,
            }),
            "a tip seated into buried surface must be refused"
        );
        assert!(
            outcomes[1].verdict.is_ok(),
            "the exterior face must still attest: {:?}",
            outcomes[1].verdict
        );

        // THE TRAP OD-9 NAMES, pinned. The buried tip's connectivity evidence
        // is not merely adequate, it is PERFECT — its whole neighbourhood is
        // interior, so every ray penetrates. Nothing but the exterior-ness
        // screen distinguishes it from the good contact.
        assert!(
            outcomes[0].engagement.is_total(),
            "the buried tip passes the connectivity proof trivially, which is \
             exactly why a second, independent screen is required"
        );
        assert_eq!(
            (
                outcomes[0].engagement.centreline,
                outcomes[0].engagement.rim_engaged,
                outcomes[0].engagement.rim_total,
            ),
            (
                outcomes[1].engagement.centreline,
                outcomes[1].engagement.rim_engaged,
                outcomes[1].engagement.rim_total,
            ),
            "the two contacts are indistinguishable to every pre-OD-9 check"
        );
    }

    /// A TOUCHING-family candidate seated `depth` into the plane `x = face_x`
    /// from the `-x` side, with the member running out in `+x`.
    ///
    /// This is the shape both touching families mint: the model contact is at
    /// the ROOTWARD end (`brace.rs` seats `p1` into the surface, and
    /// `cco::apply_model_anchor` puts its seat at `p1`), the mirror of the stub
    /// family's terminalward contact. Both families' rows are built from this
    /// one helper on purpose: they are geometrically indistinguishable at the
    /// seat, so anything that separates them can only be the screen under test.
    fn touching_seat_row(
        face_x: f64,
        y: f64,
        depth: f64,
        radius: f64,
        probe_x: f64,
        carried_mm2: f64,
    ) -> SeatAttestationRow {
        SeatAttestationRow {
            p0_mm: [face_x + 2.5, y, 1.0],
            p1_mm: [face_x - depth, y, 1.0],
            r0_mm: radius,
            r1_mm: radius,
            end: ContactEnd::Rootward,
            exterior_probe: [probe_x, y, 1.0],
            carried_mm2,
        }
    }

    /// **F3 acceptance — BOTH touching families over the overlap region.**
    ///
    /// The ruling is that the brace model rung and the CCO model anchor deliver
    /// OD-9 at their seats, not only the stub family. This is that requirement
    /// made executable on the SAME interpenetrating pair the stub family uses,
    /// so all three families are held to one fixture and one standard.
    ///
    /// Row 0 is a brace model rung seated on `A`'s `+x` face at `x = 4`, which
    /// over `x in [2, 4]` is buried inside `B`. Row 1 is a CCO model anchor
    /// seated on `B`'s exterior `+x` face at `x = 6`, carrying a real routed
    /// load the way that family does. The families are then SWAPPED, because a
    /// screen that only worked for whichever family happened to be buried in
    /// the fixture would pass a one-sided version of this.
    #[test]
    fn f3_a_buried_seat_is_refused_and_an_exterior_seat_attests_for_both_touching_families() {
        let surface = interpenetrating_pair();
        const DEPTH: f64 = 0.3;
        const RADIUS: f64 = 0.2;

        // The fixture's non-vacuity, asserted BEFORE the screen runs: if these
        // parity facts ever stop holding, the fixture has stopped
        // self-intersecting and this test must fail rather than pass for the
        // wrong reason.
        assert_eq!(
            surface.inside(&[[4.05, 1.0, 1.0], [6.05, 1.0, 1.0]]),
            vec![true, false],
            "the fixture must keep x = 4 buried inside B and x = 6 exterior"
        );

        // Brace model rung on the BURIED face; CCO model anchor on the
        // EXTERIOR one. A rung carries zero routed load (F2) and an anchor
        // carries the terminal's real load (MAJOR-7), so the two rows differ in
        // exactly the way production differs.
        let seats = [
            touching_seat_row(4.0, 1.0, DEPTH, RADIUS, 4.05, 0.0),
            touching_seat_row(6.0, 1.0, DEPTH, RADIUS, 6.05, 0.1),
        ];
        let outcomes = attest_seats_batch(
            &surface,
            &seats,
            extent(0.05),
            validation_limits(&surface, seats.len()),
            &attestation_policy(),
        )
        .unwrap();
        assert_eq!(outcomes.len(), 2);
        assert_eq!(
            outcomes[0].verdict,
            Err(ContactAttestationRefusal::BuriedSurface {
                end: ContactEnd::Rootward
            }),
            "a rung seated on buried surface must be refused"
        );
        assert!(
            outcomes[1].verdict.is_ok(),
            "an anchor seated on exterior surface must attest"
        );

        // The trap OD-9 exists for: the BURIED seat's connectivity evidence is
        // not merely adequate, it is PERFECT, because its whole neighbourhood
        // is interior. Every pre-OD-9 check ranks it at least as highly as the
        // exterior seat.
        assert!(
            outcomes[0].engagement.is_total(),
            "the buried seat's connectivity witness is total -- which is why \
             nothing but the parity screen can catch it"
        );
        assert!(outcomes[0].engagement.centreline);
        assert_eq!(
            outcomes[0].engagement.rim_engaged,
            outcomes[0].engagement.rim_total
        );

        // Swap the families across the two seats. The verdicts must follow the
        // GEOMETRY, not the family, or the screen is family-specific by
        // accident.
        let swapped = [
            touching_seat_row(6.0, 1.0, DEPTH, RADIUS, 6.05, 0.0),
            touching_seat_row(4.0, 1.0, DEPTH, RADIUS, 4.05, 0.1),
        ];
        let swapped_outcomes = attest_seats_batch(
            &surface,
            &swapped,
            extent(0.05),
            validation_limits(&surface, swapped.len()),
            &attestation_policy(),
        )
        .unwrap();
        assert!(
            swapped_outcomes[0].verdict.is_ok(),
            "an exterior seat attests whichever family proposed it"
        );
        assert_eq!(
            swapped_outcomes[1].verdict,
            Err(ContactAttestationRefusal::BuriedSurface {
                end: ContactEnd::Rootward
            }),
            "a buried seat is refused whichever family proposed it"
        );
    }

    /// The transcript must be able to SEE the refusal, or the digest chain that
    /// carries it is decorative.
    #[test]
    fn f3_the_seat_transcript_digest_separates_a_refusal_from_an_attestation() {
        let surface = interpenetrating_pair();
        let buried = [touching_seat_row(4.0, 1.0, 0.3, 0.2, 4.05, 0.0)];
        let exterior = [touching_seat_row(6.0, 1.0, 0.3, 0.2, 6.05, 0.0)];
        let attest = |seats: &[SeatAttestationRow]| {
            attest_seats_batch(
                &surface,
                seats,
                extent(0.05),
                validation_limits(&surface, seats.len()),
                &attestation_policy(),
            )
            .unwrap()
        };
        let buried_outcomes = attest(&buried);
        let exterior_outcomes = attest(&exterior);
        let digest_of = |tag, seats: &[SeatAttestationRow], outcomes: &[ContactAttestation]| {
            seat_attestation_digest(tag, extent(0.05), seats, outcomes)
        };
        assert_ne!(
            digest_of(SEAT_TAG_BRACE_MODEL_RUNG, &buried, &buried_outcomes),
            digest_of(SEAT_TAG_BRACE_MODEL_RUNG, &exterior, &exterior_outcomes),
            "a refused seat and an attested one must not share a transcript"
        );
        // The family tag is load bearing: one transcript from two families is
        // two different chain links.
        assert_ne!(
            digest_of(SEAT_TAG_BRACE_MODEL_RUNG, &buried, &buried_outcomes),
            digest_of(SEAT_TAG_CCO_MODEL_ANCHOR, &buried, &buried_outcomes),
        );
        // An EMPTY batch is a real value, so "this family proposed nothing" and
        // "the attestation stopped running" stay distinguishable.
        assert_ne!(digest_of(SEAT_TAG_BRACE_MODEL_RUNG, &[], &[]), 0);
    }

    /// Batch-first, pinned rather than asserted in prose: the whole row set
    /// costs ONE connectivity kernel call however many seats it carries.
    #[test]
    fn f3_the_seat_attestation_issues_one_kernel_call_for_the_whole_batch() {
        let surface = interpenetrating_pair();
        let seats = (0..6)
            .map(|index| {
                touching_seat_row(6.0, 0.25 + 0.25 * f64::from(index), 0.3, 0.2, 6.05, 0.0)
            })
            .collect::<Vec<_>>();
        reset_endpoint_validation_kernel_calls();
        let outcomes = attest_seats_batch(
            &surface,
            &seats,
            extent(0.05),
            validation_limits(&surface, seats.len()),
            &attestation_policy(),
        )
        .unwrap();
        assert_eq!(outcomes.len(), 6);
        assert_eq!(
            endpoint_validation_kernel_calls(),
            1,
            "six seats must cost one connectivity batch, not six"
        );
    }

    #[test]
    fn od9_the_screen_is_the_only_thing_that_separates_them() {
        // The complement of the test above: with the probes swapped to points
        // that are both exterior, the buried contact attests. That is what
        // proves the refusal came from the parity screen and not from some
        // incidental property of the buried member's geometry.
        let surface = interpenetrating_pair();
        let buried = seated_stub(4.0, 1.0, 0.3, 0.2);
        let members = [&buried];
        let requests = [ContactAttestationRequest {
            member: 0,
            end: ContactEnd::Terminalward,
            extent: extent(0.05),
            // Deliberately mis-sited probe, out beyond both boxes.
            exterior_probe: [7.0, 1.0, 1.0],
            end_radius_mm: 0.2,
            carried_mm2: 0.1,
            tip_clamped_at_max: false,
        }];
        let outcomes = attest_members_batch(
            &surface,
            &members,
            &requests,
            validation_limits(&surface, 1),
            &attestation_policy(),
        )
        .unwrap();
        assert!(outcomes[0].verdict.is_ok());
    }

    #[test]
    fn a_partly_engaged_tip_is_measured_rather_than_deleted() {
        // A contact placed within a tip radius of a model edge. Its centre may
        // sit exactly on the boundary plane and several rim rays hang in air —
        // measured on `make_overhang_table`, where eight such contacts exist
        // and all eight print today. PR-8 must call this ATTACHED with reduced
        // area, not absent.
        let surface = surface_with_cubes(&[(0.0, 1.0)]);
        // The cube spans y in [0, 1]; seat the tip at y = 1.0 exactly, so half
        // the end disc hangs outside the +y face.
        let member = Member::new(spec(
            [0.7, 1.0, 0.5],
            [-1.8, 1.0, 0.5],
            0.2,
            Some(ContactEnd::Terminalward),
            0,
        ));
        let members = [&member];
        let requests = [ContactAttestationRequest {
            member: 0,
            end: ContactEnd::Terminalward,
            extent: extent(0.05),
            exterior_probe: [0.7, 1.05, 0.5],
            end_radius_mm: 0.2,
            carried_mm2: 0.0,
            tip_clamped_at_max: false,
        }];
        let outcomes = attest_members_batch(
            &surface,
            &members,
            &requests,
            validation_limits(&surface, 1),
            &attestation_policy(),
        )
        .unwrap();
        let outcome = &outcomes[0];
        assert!(
            outcome.verdict.is_ok(),
            "a half-engaged tip is attached, not absent: {outcome:?}"
        );
        assert!(
            !outcome.engagement.is_total(),
            "and it must NOT be credited a full disc"
        );
        assert!(
            outcome.attested_radius_mm < 0.2 && outcome.attested_radius_mm > 0.0,
            "r_eff {} must sit strictly between zero and the nominal radius",
            outcome.attested_radius_mm
        );
        // Partial engagement mints no mask: there is no extent whose totality
        // was proven, so nothing may be exempted from the distance check.
        assert_eq!(outcome.verdict, Ok(None));
        // The same geometry, asked for a MASK rather than an attestation, is
        // refused — the two questions have different answers by design.
        assert_eq!(
            validate_members_batch(
                &surface,
                &members,
                &[(0, ContactEnd::Terminalward, extent(0.05))],
                validation_limits(&surface, 1),
            ),
            Err(DesignedMaskError::DisconnectedGeometry {
                end: ContactEnd::Terminalward,
            })
        );
    }

    #[test]
    fn a_tip_entirely_outside_the_solid_is_refused_as_unattached() {
        let surface = surface_with_cubes(&[(0.0, 1.0)]);
        // Seated nowhere: the whole member sits clear of the cube in y.
        let member = Member::new(spec(
            [0.5, 3.0, 0.5],
            [0.5, 6.0, 0.5],
            0.05,
            Some(ContactEnd::Terminalward),
            0,
        ));
        let members = [&member];
        let requests = [ContactAttestationRequest {
            member: 0,
            end: ContactEnd::Terminalward,
            extent: extent(0.05),
            exterior_probe: [0.5, 2.95, 0.5],
            end_radius_mm: 0.05,
            carried_mm2: 0.0,
            tip_clamped_at_max: false,
        }];
        let outcomes = attest_members_batch(
            &surface,
            &members,
            &requests,
            validation_limits(&surface, 1),
            &attestation_policy(),
        )
        .unwrap();
        assert_eq!(
            outcomes[0].verdict,
            Err(ContactAttestationRefusal::NotAttached {
                end: ContactEnd::Terminalward,
            })
        );
        assert_eq!(outcomes[0].attested_radius_mm, 0.0);
    }

    /// MAJOR-7 — PR-4 is no longer inert, and the FU-CONTACT-1 clamp clause is
    /// load-bearing rather than decorative.
    #[test]
    fn the_area_law_grades_the_attested_interface_and_the_clamp_suppresses_it() {
        let surface = surface_with_cubes(&[(0.0, 1.0)]);
        let policy = attestation_policy();
        // A stub seated 0.1 mm into the cube's x = 1.0 face, well clear of its
        // edges, so the interface is the FULL disc and the only thing under
        // test is the area law.
        let member = Member::new(spec(
            [0.9, 0.5, 0.5],
            [3.0, 0.5, 0.5],
            0.05,
            Some(ContactEnd::Terminalward),
            0,
        ));
        let members = [&member];
        let request = |carried_mm2: f64, tip_clamped_at_max: bool| ContactAttestationRequest {
            member: 0,
            end: ContactEnd::Terminalward,
            extent: extent(0.05),
            exterior_probe: [1.05, 0.5, 0.5],
            end_radius_mm: 0.05,
            carried_mm2,
            tip_clamped_at_max,
        };
        let attest = |carried_mm2: f64, clamped: bool| {
            attest_members_batch(
                &surface,
                &members,
                &[request(carried_mm2, clamped)],
                validation_limits(&surface, 1),
                &policy,
            )
            .unwrap()
            .remove(0)
        };

        // pi * 0.05^2 = 7.854e-3 mm^2 of interface. At X = 12 that carries up
        // to 9.42e-2 mm^2 of model area.
        let carried_ok = 0.09_f64;
        let carried_over = 0.2_f64;

        let under = attest(carried_ok, false);
        assert!(under.verdict.is_ok());
        assert_eq!(under.area_deficit, None);

        let over = attest(carried_over, false);
        assert!(
            over.verdict.is_ok(),
            "an area deficit is evidence, never a refusal"
        );
        let deficit = over.area_deficit.expect("the deficit must be recorded");
        assert_eq!(deficit.carried_mm2, carried_over);
        assert!((deficit.nominal_radius_mm - 0.05).abs() < 1e-12);

        // MAJOR-7 non-vacuity: the SAME row with zero routed load records no
        // deficit, which is precisely why PR-4 gated nothing while
        // `contact::admit`'s only production caller passed `carried_mm2 = 0.0`.
        assert_eq!(attest(0.0, false).area_deficit, None);

        // FU-CONTACT-1: a tip already at the registered clamp cannot be
        // charged for a deficiency the clamp caused.
        assert_eq!(attest(carried_over, true).area_deficit, None);
    }
}
