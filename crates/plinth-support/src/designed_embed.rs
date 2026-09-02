//! The designed clearance-mask domain: an optional, construction-validated
//! axial extent that a member end may carry because that end is deliberately
//! embedded in the model.
//!
//! Three properties are load bearing and are the reason this lives in its own
//! crate-private module rather than beside the forest data model:
//!
//! * A mask is *minted*, never asserted. [`attest_members_batch`] is the sole
//!   production entry point, and every extent it mints a mask for has been
//!   proven connected to its endpoint through the actual tapered member.
//! * Observed penetration thickness is separate evidence. Nothing here reads a
//!   penetration interval, a projection distance, or a breach test to *derive*
//!   an extent; the caller supplies a design-time number and the kernel batch
//!   only ever refuses it.
//! * Validation is whole-stage and batched. One native
//!   `penetration_intervals_batch_bounded` call serves every requested endpoint
//!   in the stage, in an order that does not depend on caller order or on the
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
use crate::forest::{mask_binding, ContactEnd, Member};

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
/// The outer `Err` is always a BATCH-level integrity failure — a starved
/// resource admission, a kernel refusal, or a result whose shape does not match
/// the rows that were sent. Nothing can be concluded about any row from it.
/// Geometry verdicts are values in the returned rows, which is what lets PR-8
/// route a refusal to quarantine instead of failing a pipeline (OD-4).
///
/// The INNER `Err` is FU-88's lesson one layer down: a row whose evidence
/// cannot be graded costs itself and nothing else. Grading was whole-batch here
/// until #28 — one ungradable row out of forty took the other thirty-nine down
/// with it, and on the Warhorse and the Giant Bat that is exactly what happened
/// 1803 times, never once because the whole batch was bad.
///
/// **Per-row grading is for CANDIDATE geometry only, and the split is by
/// CALLER, not by entry point.** Both callers below reach this witness:
///
/// * `attest_seats_batch` (brace model rungs, CCO model anchors) grades
///   candidates competing in a tournament. A row it cannot measure leaves the
///   tournament and costs nothing else. This is the case #28 exists for.
/// * `initial.rs`'s `attest_initial_contacts` hands in already-PLACED geometry
///   and collects the inner results back into a TOTAL refusal of the whole
///   initial-contact build. A caller with placed geometry this witness cannot
///   grade has a defect, and the alternative is worse than an error: the placed
///   caller disposes of a refusal by DELETING the member (a stub quarantined),
///   so a per-row verdict there would silently delete a load-carrying member on
///   a measurement nobody made.
///
/// Note that `attest_initial_contacts` reaches this witness through
/// `attest_members_batch`, which `attest_seats_batch` also uses; that shared
/// entry point therefore stays per-row and the totality lives in the placed
/// caller.
fn witness_batch(
    surface: &NativeSurface,
    rows: &[EndpointValidationRow],
    limits: EndpointMaskValidationLimits,
) -> Result<Vec<Result<EndpointEngagement, DesignedMaskError>>, DesignedMaskError> {
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
    // `docs/specs/geometry-adjudication.md` §EXT-1 site 15
    // (`support.embed.witness`).
    let classified = surface
        .penetration_intervals_batch_bounded_classified(
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
    if classified.rows.len() != rows.len() || classified.near_parallel.len() != rows.len() {
        return Err(DesignedMaskError::CorruptKernelResult);
    }
    let near_parallel = classified.near_parallel;
    let interval_rows = classified.rows;
    #[cfg(test)]
    let interval_rows = sabotage_endpoint_intervals(interval_rows);
    // An UNDECIDABLE row's witness is INVALID, so the embed it would have
    // attested is refused by name.
    //
    // This is a connectivity PROOF: `endpoint_engagement` affirms that a
    // declared end really is in the solid, by matching interval evidence
    // against the capsule's rays. A skipped crossing UNDERSTATES the interval
    // set, so an affected row's evidence is a LOWER bound on engagement — the
    // direction that turns an engaged end into an unengaged-looking one AND, at
    // the other end of the same understatement, lets a row's centreline or rim
    // engagement be assembled from evidence that is missing pieces nobody can
    // account for. A proof cannot be completed from an incomplete record, so
    // the row refuses rather than returning an engagement nobody proved.
    // Refusal is candidate-granular: this is an ordinary attestation refusal
    // routed to quarantine (OD-4), not a pipeline failure.
    Ok(rows
        .iter()
        .zip(interval_rows)
        .zip(near_parallel)
        .map(|((row, intervals), near_parallel)| {
            if near_parallel > 0 {
                return Err(DesignedMaskError::NearParallelWitness { near_parallel });
            }
            endpoint_engagement(row, &intervals)
        })
        .collect())
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
    /// FU-88 — this CANDIDATE's own proposed geometry cannot carry the
    /// attestation it asks for, so there is nothing to measure. The
    /// [`DesignedMaskError`] names which scalar check failed; in practice on the
    /// corpus it is [`DesignedMaskError::ExtentOutOfSpan`], a proposed arm
    /// shorter than the one process resolution the attestation claims.
    ///
    /// **The check is unchanged; only who pays for it moved.** These are
    /// exactly the scalar refusals [`prepare_rows`] applies, evaluated per row
    /// instead of over the whole batch. Applying them batch-wide is right for
    /// PLACED members — a caller that hands `attest_members_batch` a bad row
    /// has a defect, and nothing can be concluded about the batch. It is wrong
    /// for CANDIDATES: one unusable candidate out of forty used to abort the
    /// whole model, where the other thirty-nine were fine and one of them was
    /// going to win. So [`attest_seats_batch`] screens each candidate row on its
    /// own, refuses the ones that fail, and sends the survivors to the kernel.
    /// A refused candidate leaves the tournament exactly like a buried seat
    /// does (OD-4/PR-9), and if a spot loses EVERY candidate the terminal goes
    /// unrouted and its area reappears as ordinary uncovered area.
    UnusableGeometry {
        end: ContactEnd,
        cause: DesignedMaskError,
    },
}

impl ContactAttestationRefusal {
    #[must_use]
    pub(crate) const fn end(self) -> ContactEnd {
        match self {
            Self::BuriedSurface { end }
            | Self::NotAttached { end }
            | Self::UnusableGeometry { end, .. } => end,
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
            let refusal = screen_attestation(row.end, is_buried, &engagement);
            let engagement = engagement.unwrap_or(EndpointEngagement::UNGRADABLE);
            let attested_radius_mm = engagement.effective_radius_mm(request.end_radius_mm);
            let verdict = match refusal {
                Some(refusal) => Err(refusal),
                None if engagement.is_total() => Ok(Some(mint_mask(
                    members[row.member],
                    row,
                    engagement.witness,
                    surface_identity,
                ))),
                None => Ok(None),
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

/// The three ways one row can fail to attest, in the one order that is
/// defensible, stated once so no caller can reorder them by accident.
///
/// 1. **Buried (OD-9) first.** The parity probe is INDEPENDENT of the
///    connectivity evidence and it is the screen the connectivity proof passes
///    trivially, so it is reported first or it can be laundered away.
/// 2. **Ungradable next.** Nothing was measured on this row, so there is no
///    finding about attachment to report. Calling it `NotAttached` would be a
///    claim about geometry that was never examined — and it is the claim the
///    zero-valued [`EndpointEngagement::UNGRADABLE`] would otherwise produce,
///    silently, which is precisely the trap this arm exists to close.
/// 3. **Not attached last**, on evidence that was actually graded.
fn screen_attestation(
    end: ContactEnd,
    is_buried: bool,
    engagement: &Result<EndpointEngagement, DesignedMaskError>,
) -> Option<ContactAttestationRefusal> {
    if is_buried {
        return Some(ContactAttestationRefusal::BuriedSurface { end });
    }
    match engagement {
        Err(cause) => Some(ContactAttestationRefusal::UnusableGeometry { end, cause: *cause }),
        Ok(engagement) if !engagement.is_attached() => {
            Some(ContactAttestationRefusal::NotAttached { end })
        }
        Ok(_) => None,
    }
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
        let radius =
            |value: f64| PositiveFinite::new(value).map_err(|_| DesignedMaskError::InvalidExtent);
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
///
/// **FU-88 — a candidate this batch cannot measure costs the CANDIDATE, not the
/// model.** `attest_members_batch` applies every scalar refusal to the whole
/// batch, which is right for PLACED members and wrong here. One unusable
/// candidate — on the corpus, a proposed arm shorter than the one process
/// resolution the attestation claims — used to abort the whole model with the
/// other thirty-nine candidates untested. Each row is now screened on its own by
/// [`screen_seat_row`], which runs exactly the checks [`prepare_rows`] runs and
/// no others; a row that fails leaves the batch as an ordinary
/// [`ContactAttestationRefusal::UnusableGeometry`] verdict, and only the
/// survivors reach the kernel.
///
/// Cost is unchanged: still exactly one screened pass and one delegated pair of
/// native calls for the whole stage, whatever the candidate count.
///
/// The returned rows stay index-aligned with `seats` — [`ContactAttestation::member`]
/// is the caller's seat index, not the index of the compacted survivor set.
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
    // Screen every candidate on its own BEFORE any native work, so an unusable
    // one is discarded at zero kernel cost and the rest still get measured.
    let mut members = Vec::with_capacity(seats.len());
    let mut survivor_seat_index = Vec::with_capacity(seats.len());
    let mut unusable = vec![None; seats.len()];
    for (index, seat) in seats.iter().enumerate() {
        match screen_seat_row(seat, extent) {
            Ok(member) => {
                survivor_seat_index.push(index);
                members.push(member);
            }
            Err(cause) => unusable[index] = Some(cause),
        }
    }
    let borrowed = members.iter().collect::<Vec<_>>();
    let requests = survivor_seat_index
        .iter()
        .enumerate()
        .map(|(row, seat_index)| {
            let seat = &seats[*seat_index];
            ContactAttestationRequest {
                member: row,
                end: seat.end,
                extent,
                exterior_probe: seat.exterior_probe,
                end_radius_mm: seat.end_radius_mm(),
                carried_mm2: seat.carried_mm2,
                // A candidate seat has no tip-sizing clamp to suppress: the clamp
                // clause exists for `initial.rs`'s load-derived tip radii, and
                // neither touching family derives its end radius from a load.
                tip_clamped_at_max: false,
            }
        })
        .collect::<Vec<_>>();
    let mut attested = vec![None; seats.len()];
    if !borrowed.is_empty() {
        let outcomes = attest_members_batch(surface, &borrowed, &requests, limits, policy)?;
        if outcomes.len() != borrowed.len() {
            return Err(DesignedMaskError::CorruptKernelResult);
        }
        for mut outcome in outcomes {
            // Re-key from the compacted survivor row back to the caller's seat
            // index, so a caller that indexes by `member` cannot be handed a
            // verdict about a different candidate.
            let seat_index = *survivor_seat_index
                .get(outcome.member)
                .ok_or(DesignedMaskError::CorruptKernelResult)?;
            outcome.member = seat_index;
            if attested[seat_index].replace(outcome).is_some() {
                return Err(DesignedMaskError::CorruptKernelResult);
            }
        }
    }
    seats
        .iter()
        .enumerate()
        .map(
            |(index, seat)| match (attested[index].take(), unusable[index]) {
                (Some(outcome), None) => Ok(outcome),
                (None, Some(cause)) => Ok(unusable_attestation(index, seat, cause)),
                _ => Err(DesignedMaskError::CorruptKernelResult),
            },
        )
        .collect()
}

/// Every scalar refusal [`prepare_rows`] and [`attest_members_batch`] apply,
/// evaluated for ONE candidate row.
///
/// This is a re-evaluation of the same checks, not a new or stricter one. The
/// cases `prepare_rows` screens that cannot arise here are named rather than
/// silently dropped: a provisional member declares exactly the one end its seat
/// row names, so `EndNotDesigned`, `MissingRequiredExtent`, `DuplicateRequest`,
/// `OverlappingExtents` and `UnknownMember` are all unreachable for a seat row.
fn screen_seat_row(
    seat: &SeatAttestationRow,
    extent: PositiveFiniteMm,
) -> Result<Member, DesignedMaskError> {
    let member = seat.provisional_member()?;
    if !seat.exterior_probe.iter().all(|value| value.is_finite())
        || !seat.end_radius_mm().is_finite()
        || seat.end_radius_mm() <= 0.0
        || !seat.carried_mm2.is_finite()
        || seat.carried_mm2 < 0.0
    {
        return Err(DesignedMaskError::InvalidExtent);
    }
    let length = member_length_mm(&member)?;
    validate_extent(extent, length)?;
    Ok(member)
}

/// The row a screened-out candidate contributes: nothing was measured, so
/// nothing is claimed. Zero engaged rays and a zero attested radius are the
/// honest report for geometry the kernel was never asked about, and PR-4 is not
/// graded on a refused row for the same reason it is not graded on a buried one.
fn unusable_attestation(
    index: usize,
    seat: &SeatAttestationRow,
    cause: DesignedMaskError,
) -> ContactAttestation {
    ContactAttestation {
        member: index,
        end: seat.end,
        engagement: EndpointEngagement::UNGRADABLE,
        attested_radius_mm: 0.0,
        area_deficit: None,
        verdict: Err(ContactAttestationRefusal::UnusableGeometry {
            end: seat.end,
            cause,
        }),
    }
}

// ======================== GROW-TO-ATTEST (PR-8 remedy) ========================
//
// A tip has TWO duties and the engine used to size it for only one.
//
// * **Load.** `pi r^2 >= carried_mm2 / X` — the registered area law, which
//   `FrozenTipLoadPolicy::tip_radius_mm` solves for every contact.
// * **Attachment.** Some part of that same disc must actually be in the solid,
//   which is what PR-8 measures with the centreline plus eight rim rays AT THE
//   TIP RADIUS (see `EndpointValidationRow::terminalward`: the ring radius IS
//   `r0`).
//
// Those duties disagree at a model silhouette edge. A tip seated within a tip
// radius of an edge has its CENTRE on the boundary plane, which parity reads as
// outside, and is held on by the rim alone — `is_attached()` says so out loud.
// Shrink the tip and the ring shrinks with it, so the rim rays that were finding
// material fold in toward the centreline and find air. The tip is then refused
// `NotAttached` for being SMALL, not for being badly placed.
//
// Measured, on this branch: ten seats on thin claws and toes across the corpus
// had centreline-in-air and lived on rim engagement alone. At a law-sized thin
// tip the ring withdrew, all ten were refused and quarantined at the initial
// stage, and the load each carried — 2.98 mm^2 on the Deinonychus — was
// abandoned in silence. No neighbour was told, none was grown, and coverage's
// 6 mm radius cannot see a hole that small.
//
// The remedy is physics, not a threshold: **a tip must be sized for BOTH duties,
// so where the load-sized tip does not attest, grow it until it does and emit
// the grown size.** A tip larger than the load law demands is legal and strictly
// conservative — `violates_safety_factor` is monotone decreasing in radius, so
// growing can never weaken the load refusal — and the growth is capped by the
// registered `tip_radius_max_legacy_mm`. Only when the whole ladder fails may a
// contact be refused, and that refusal is DISCLOSED with the load it abandons.
//
// **What this deliberately does NOT do.** It does not restore the old fat probe
// ring. Every rung probes at the radius that rung would EMIT, so an attestation
// only ever certifies a tip the run is actually going to build. Re-inflating the
// ring while emitting a thin tip is the defect, not the fix.

/// **WHICH registered limit ended a growth ladder.**
///
/// Named rather than inferred from the number, because the disclosure has to
/// say which limit stopped it and a reader cannot tell 0.400 from 1.000 apart
/// by eye once the ladder has been clamped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum GrowthCeilingSource {
    /// `TIP_RADIUS_MAX_GROWN_SCAR_MM` — Craig's measured field ceiling on the
    /// scar a grow-for-attachment rescue may leave, 0.400 mm RADIUS
    /// (0.800 mm DIAMETER).
    GrownScarMaximum,
    /// `TIP_RADIUS_MAX_LEGACY_MM` — the load law's own clamp maximum,
    /// 1.000 mm RADIUS (2.000 mm DIAMETER).
    LegacyTipMaximum,
}

impl GrowthCeilingSource {
    /// The registry id, so a disclosure names the constant and not a number.
    pub(crate) const fn constant_id(self) -> &'static str {
        match self {
            Self::GrownScarMaximum => "TIP_RADIUS_MAX_GROWN_SCAR_MM",
            Self::LegacyTipMaximum => "TIP_RADIUS_MAX_LEGACY_MM",
        }
    }
}

/// **THE SCAR BOUND — how high a rescue may grow a tip, and what it leaves
/// alone.**
///
/// Craig's ruling of 2026-08-20, on his own field data: the heaviest support he
/// has ever needed is a **0.800 mm DIAMETER** sphere, so **0.400 mm RADIUS
/// (0.800 mm DIAMETER)** is the largest scar he accepts on a model. It is
/// registered as `TIP_RADIUS_MAX_GROWN_SCAR_MM`, honestly labelled an EXTERNAL
/// REQUIREMENT — field data, not physics and not a wet-finger estimate.
///
/// **The distinction that matters, and the reason this is a separate limit
/// rather than a smaller `TIP_RADIUS_MAX_LEGACY_MM`.** Two different appetites
/// can enlarge a tip:
///
/// * the **LOAD LAW** — `pi r^2 >= carried_area / X`, which sizes a tip to
///   CARRY WEIGHT and may legitimately demand up to the registered
///   `TIP_RADIUS_MAX_LEGACY_MM` of 1.000 mm radius (2.000 mm diameter) for a
///   genuinely heavy area. **This bound does not touch that.** A load-sized tip
///   above 0.400 mm radius that attests is emitted exactly as before: it is not
///   clamped, not refused, and not mentioned here.
/// * **GROWTH FOR ATTACHMENT** — this module's ladder, which enlarges a tip
///   BEYOND what the load law asked for purely so the attestation ring can
///   reach solid. That excess is a scar in a place the operator did not choose,
///   and it is what this bound limits.
///
/// So the ladder truncates at the SMALLER of the two limits and the refusal
/// path names whichever one it stopped at. A contact whose cure would need a
/// larger tip than the ceiling gets the existing disclosed ABANDONED LOAD line,
/// with the bound named as the stopper — never a silently over-grown scar.
///
/// The truncation weakens no refusal in either direction: lowering a ceiling
/// can only turn a cure into a refusal, never a refusal into a cure.
pub(crate) fn growth_ceiling_radius_mm(
    legacy_max_mm: f64,
    scar_bound_mm: f64,
) -> (f64, GrowthCeilingSource) {
    // SABOTAGE SEAM. Ignoring the scar bound is exactly the engine before this
    // change: the ladder runs to the legacy maximum and a rescue may leave a
    // scar wider than any support Craig has ever needed. It sits here, at the
    // min, because the min IS the truncation — sabotaging the ladder's own
    // arithmetic instead would make the ladder tests disagree with themselves.
    // The bound binds only when it is a real number strictly below the legacy
    // maximum. `is_finite` is stated rather than left to the comparison: a NaN
    // bound must fall back to the legacy limit, and a bare `<` on a NaN is
    // false by accident rather than by decision.
    if !scar_bound_disabled() && scar_bound_mm.is_finite() && scar_bound_mm < legacy_max_mm {
        (scar_bound_mm, GrowthCeilingSource::GrownScarMaximum)
    } else {
        (legacy_max_mm, GrowthCeilingSource::LegacyTipMaximum)
    }
}

fn growth_ceiling(request: &TipGrowthRequest) -> (f64, GrowthCeilingSource) {
    growth_ceiling_radius_mm(request.max_radius_mm, request.scar_bound_radius_mm)
}

/// The radius ratio that DOUBLES a tip's nominal interface area.
///
/// `pi (sqrt(2) r)^2 = 2 pi r^2`. This is an algebraic identity of the disc, not
/// a tunable and not a constant to register: there is no value here anyone could
/// choose differently and still be doubling the area.
const AREA_DOUBLING_RADIUS_RATIO: f64 = std::f64::consts::SQRT_2;

/// The radii, ASCENDING, that a tip refused for attachment may be regrown to.
///
/// **The step is one area doubling.** The tip's registered duty is AREA, so the
/// step that means something is the one that changes the area answer: each rung
/// is `sqrt(2)` times the previous radius and therefore twice its nominal disc
/// area. A finer step would re-ask the same question — the eight rim rays sit
/// `2 pi r / 8 = 0.785 r` apart, so a radius change much smaller than `r` moves
/// the ring by less than the gap between its own samples — and a coarser one
/// would step over the smallest tip that would have worked.
///
/// **The rung COUNT is not chosen.** It is
/// `ceil(log(max / law) / log(sqrt(2)))`, fixed by this row's own law radius and
/// the ceiling [`growth_ceiling`] handed it. Across the widest span the
/// registry admits — `TIP_RADIUS_MAX_LEGACY_MM / TIP_RADIUS_DEFAULT_MM` =
/// 1.0 / 0.05 = 20 mm radius over mm radius (2.0 / 0.1 in diameter) — the
/// deepest possible ladder is `ceil(2 * log2(20))` = 9 rungs. Under the scar
/// bound the span is `TIP_RADIUS_MAX_GROWN_SCAR_MM / TIP_RADIUS_DEFAULT_MM` =
/// 0.4 / 0.05 = 8, i.e. `ceil(2 * log2(8))` = 6 rungs, so the bound makes the
/// ladder SHORTER and never longer. Nothing is registered and no count is
/// hand-typed.
///
/// **The last rung is exactly the ceiling**, so the sweep always terminates on
/// a radius the run is allowed to build and never proposes one above the limit
/// the owner ruled.
///
/// **Ascending, first success wins, and no bisection.** Attestation is NOT
/// monotone in radius: a wider ring can step PAST a thin feature that a
/// narrower one was inside, so "it failed at r, therefore it fails below r" is
/// false and a bisection on it would be unsound. An ordered ascending sweep that
/// keeps the FIRST rung to attest needs no monotonicity assumption and returns
/// the smallest radius on the ladder that satisfies both duties.
fn tip_growth_ladder(law_radius_mm: f64, max_radius_mm: f64) -> Vec<f64> {
    let mut rungs = Vec::new();
    if !law_radius_mm.is_finite()
        || !max_radius_mm.is_finite()
        || law_radius_mm <= 0.0
        || max_radius_mm <= law_radius_mm
    {
        return rungs;
    }
    let mut radius = law_radius_mm;
    loop {
        let next = radius * AREA_DOUBLING_RADIUS_RATIO;
        // The ratio is > 1 and the radius is positive and finite, so this
        // strictly increases and the loop terminates. The guard is here for the
        // one way that could stop being true — a radius large enough that the
        // multiply saturates — rather than trusting it.
        if next <= radius || !next.is_finite() {
            break;
        }
        if next >= max_radius_mm {
            rungs.push(max_radius_mm);
            break;
        }
        rungs.push(next);
        radius = next;
    }
    rungs
}

/// The ladder itself, for the test that asserts its shape against the
/// registered clamps rather than against a hand-typed list of radii.
#[cfg(test)]
pub(crate) fn tip_growth_ladder_for_test(law_radius_mm: f64, max_radius_mm: f64) -> Vec<f64> {
    tip_growth_ladder(law_radius_mm, max_radius_mm)
}

// THE OVERSHOOT THE LADDER LEAVES, and the scan that closes it.
//
// The ladder's step is one AREA DOUBLING, so the rung it stops on can be up to
// one whole doubling above the smallest tip that would have worked — and MORE
// than one when the first success is two or three rungs up, because every rung
// below the last failure was also skipped over in area. MEASURED on this branch
// (the lane judge's item 1): the emitted tips sit 4.6x to 5.7x the LAW area,
// i.e. 2.1x to 2.4x the law RADIUS, which is the two-to-three-rung signature of
// a doubling ladder. Every one of those tips is a scar on the model that is
// bigger than physics asked for.
//
// So: once the ascending ladder has BRACKETED the answer between its last
// failing rung and its first passing one, walk back DOWN inside that bracket and
// keep the smallest radius that still attests.
//
// **No monotonicity is assumed here either.** The ladder is ascending-first-wins
// precisely because attestation is not monotone in radius; the scan inherits
// that. It therefore measures EVERY step in the bracket rather than stopping at
// the first failure on the way down, and the answer is keep-smallest-SUCCESS
// over the whole scanned set — never "the last one before a failure".

/// The radius ratio of ONE refinement step, descending.
///
/// **The step is a fixed fraction of the ring's own sample spacing.** The
/// ladder's step was justified against the ring's angular sampling: the
/// `ENDPOINT_VALIDATION_OFFSET_RAYS` rim rays sit `2 pi r / N` = `0.785 r` apart
/// at radius `r`, so a radius change much smaller than `r` moves the ring by
/// less than the gap between its own samples. The refinement uses the SAME
/// geometry one level down: a step moves the rim by one `N`th of that inter-ray
/// gap,
///
/// ```text
/// delta(r) = (2 pi r / N) / N = 2 pi r / N^2 = 0.0982 r        (N = 8)
/// ```
///
/// `N` is the ring's own ray count and the gap is the ring's own sample spacing,
/// so nothing new is introduced and nothing is hand-typed — change the ray count
/// and the scan step follows it. Because `delta` is proportional to `r` the
/// descent is GEOMETRIC, with this ratio, which is what makes the rung count
/// below independent of the radius the scan starts from.
fn tip_refinement_radius_ratio() -> f64 {
    let rays = ENDPOINT_VALIDATION_OFFSET_RAYS as f64;
    1.0 - std::f64::consts::TAU / (rays * rays)
}

/// The radii, DESCENDING, inside the bracket the ladder closed.
///
/// `cure_radius_mm` is the ladder's first passing rung; `last_failed_radius_mm`
/// is the rung immediately below it, which was MEASURED and failed (or, when the
/// cure is the first rung, the law radius itself, which the ordinary attestation
/// already refused). Every candidate is strictly between them, so the scan can
/// never propose a radius below what the load law asked for and never re-asks a
/// question already answered.
///
/// **The rung COUNT is not chosen — it is bounded by the two ratios.** Candidates
/// are `r_hi q^j`, and they run while `q^j > r_lo / r_hi`. The ladder's own step
/// makes `r_hi / r_lo` at most `sqrt(2)` (less only where the ladder's last rung
/// was clamped to the registered maximum), so
///
/// ```text
/// j < ln(sqrt 2) / ln(1/q) = 0.34657 / 0.10332 = 3.354  =>  at most 3 rungs
/// ```
///
/// **at most THREE extra rounds, for any contact, at any depth of ladder.** With
/// the ladder's own bound of 9 rungs across the whole registered clamp span, one
/// contact costs at most 12 batched rounds end to end — still rounds, never the
/// contact count.
fn tip_refinement_scan(cure_radius_mm: f64, last_failed_radius_mm: f64) -> Vec<f64> {
    let mut rungs = Vec::new();
    if !cure_radius_mm.is_finite()
        || !last_failed_radius_mm.is_finite()
        || last_failed_radius_mm <= 0.0
        || cure_radius_mm <= last_failed_radius_mm
    {
        return rungs;
    }
    let ratio = tip_refinement_radius_ratio();
    let mut radius = cure_radius_mm;
    loop {
        let next = radius * ratio;
        // The ratio is in (0, 1) and the radius is positive and finite, so this
        // strictly decreases and the loop terminates against the positive lower
        // bound. The guard is here for the one way that could stop being true.
        if !next.is_finite() || next >= radius {
            break;
        }
        if next <= last_failed_radius_mm {
            break;
        }
        rungs.push(next);
        radius = next;
    }
    rungs
}

/// The scan itself, for the test that asserts its bound against the ladder's
/// ratio rather than against a hand-typed count.
#[cfg(test)]
pub(crate) fn tip_refinement_scan_for_test(
    cure_radius_mm: f64,
    last_failed_radius_mm: f64,
) -> Vec<f64> {
    tip_refinement_scan(cure_radius_mm, last_failed_radius_mm)
}

/// One contact asking to be regrown until it attests.
///
/// The geometry is the caller's PLACED geometry with the contact-end radius left
/// out: growth proposes that radius, one rung at a time, and everything else —
/// both endpoints, the seat, the OD-9 probe, the routed load — is held fixed.
/// Moving the seat is a different remedy and belongs to whoever owns the seat
/// set (see [`TipGrowthOutcome`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TipGrowthRequest {
    /// The caller's own row index. Verdicts come back keyed by it.
    pub(crate) row: usize,
    pub(crate) p0_mm: V3,
    pub(crate) p1_mm: V3,
    /// Which end meets the model. That end's radius is the one growth proposes.
    pub(crate) end: ContactEnd,
    /// OD-9's probe, unchanged by growth: it is a point on the surface normal,
    /// so no tip radius can move it. A BURIED seat therefore stays buried at
    /// every rung, which is correct — burial is a mesh fault, not a sizing one.
    pub(crate) exterior_probe: V3,
    pub(crate) carried_mm2: f64,
    /// The radius the load law and the registered clamp produced. Rung zero,
    /// already tried by the ordinary attestation before growth is asked for.
    pub(crate) law_radius_mm: f64,
    /// The smallest the OTHER end may be. The mint site keeps a stub from
    /// inverting by holding its rootward end at least as wide as its tip, and
    /// growth keeps that same rule rather than inventing one.
    pub(crate) opposite_end_minimum_radius_mm: f64,
    /// The registered `tip_radius_max_legacy_mm`, in mm RADIUS — the LOAD
    /// LAW's own clamp maximum, carried here so the ladder can say which of
    /// the two limits it actually stopped at.
    pub(crate) max_radius_mm: f64,
    /// The registered `tip_radius_max_grown_scar_mm`, in mm RADIUS — Craig's
    /// measured field ceiling, 0.400 mm RADIUS (0.800 mm DIAMETER). See
    /// [`growth_ceiling`] for what it does and, just as importantly, what it
    /// does not touch.
    pub(crate) scar_bound_radius_mm: f64,
    /// PR-4's non-regression clause, carried through VERBATIM from the caller.
    ///
    /// It is deliberately NOT set by growth reaching the cap. That clause means
    /// "the TIP-SIZING policy wanted a larger radius and the clamp refused",
    /// and it suppresses the area-deficit record so a pre-existing clamp is not
    /// blamed on this package. Growth is a different appetite — the ATTACHMENT
    /// duty wanting more reach — and letting it set the clause would suppress a
    /// real PR-4 deficit on a contact the run is about to emit.
    pub(crate) tip_clamped_at_max: bool,
}

impl TipGrowthRequest {
    /// `(r0, r1)` for a proposed tip radius, contact end first as the row
    /// convention requires.
    fn radii_for(&self, tip_radius_mm: f64) -> (f64, f64) {
        let opposite = self.opposite_end_minimum_radius_mm.max(tip_radius_mm);
        match self.end {
            ContactEnd::Terminalward => (tip_radius_mm, opposite),
            ContactEnd::Rootward => (opposite, tip_radius_mm),
        }
    }

    fn provisional_member(&self, tip_radius_mm: f64) -> Result<Member, DesignedMaskError> {
        let (r0_mm, r1_mm) = self.radii_for(tip_radius_mm);
        SeatAttestationRow {
            p0_mm: self.p0_mm,
            p1_mm: self.p1_mm,
            r0_mm,
            r1_mm,
            end: self.end,
            exterior_probe: self.exterior_probe,
            carried_mm2: self.carried_mm2,
        }
        .provisional_member()
    }
}

/// One rung of the ladder and what it measured.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub(crate) struct TipGrowthAttempt {
    /// The tip RADIUS this rung proposed, in mm. Double it for the diameter the
    /// slicer UI shows.
    pub(crate) radius_mm: f64,
    /// Whether PR-8 attested at that radius.
    pub(crate) attested: bool,
}

/// The rung that cured a contact, and the attestation it earned there.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TipGrowthCure {
    /// The tip RADIUS to emit. It is the SMALLEST radius MEASURED — over the
    /// ascending ladder and the descending refinement scan together — that
    /// satisfies both duties, and it is what the run must build: the ring that
    /// attested and the tip that ships are the same disc.
    pub(crate) radius_mm: f64,
    /// The other end's radius at that tip size, held non-inverting.
    pub(crate) opposite_end_radius_mm: f64,
    pub(crate) attestation: ContactAttestation,
}

/// What growth did for one contact.
///
/// **Re-seating is not attempted here, and that is a statement about this stage
/// rather than an omission.** The remedy set Craig named is "grow, and/or
/// re-seat within the legal seat set". An initial contact has a seat set of
/// exactly ONE: the point perception chose. The tournament machinery that can
/// offer a cheap legal alternative — [`attest_seats_batch`] and its candidate
/// generation — belongs to the two TOUCHING families, which propose several
/// seats and pick between them. Moving an initial contact means asking
/// perception for a different contact, which is a different stage's decision and
/// a different lane's work. So on this path the honest remedy set is growth
/// alone, an exhausted ladder is a genuine refusal, and the disclosure says
/// which remedies were available rather than implying one was skipped.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TipGrowthOutcome {
    pub(crate) row: usize,
    /// Rung zero — what the load law asked for and PR-8 refused.
    pub(crate) law_radius_mm: f64,
    /// The largest tip this ladder was allowed to propose, mm RADIUS — the
    /// SMALLER of the registered `tip_radius_max_grown_scar_mm` and
    /// `tip_radius_max_legacy_mm`. Reported rather than recomputed downstream,
    /// so the disclosure states the ceiling the run actually walked to.
    pub(crate) growth_ceiling_radius_mm: f64,
    /// WHICH registered constant that ceiling came from, so a refusal can name
    /// its stopper instead of quoting a bare number.
    pub(crate) growth_ceiling_source: GrowthCeilingSource,
    /// Every rung tried ABOVE the law radius, in the order they were tried:
    /// the ascending ladder first, then the descending refinement scan inside
    /// the bracket the ladder closed. The order is the order of measurement,
    /// not of size, so a disclosure that prints this list is printing what the
    /// run actually did.
    pub(crate) attempts: Vec<TipGrowthAttempt>,
    /// `Some` when a rung attested. `None` means the ladder ran out at the
    /// registered maximum and the contact is genuinely refused.
    pub(crate) cure: Option<TipGrowthCure>,
}

/// Everything a growth round needs that does NOT change between rounds: the
/// surface, the extent being proven, and the attestation's own sampling
/// settings. Held as one value so the ladder phase and the scan phase provably
/// ask their questions under identical conditions.
#[derive(Clone, Copy)]
struct GrowthRoundContext<'a> {
    surface: &'a NativeSurface,
    extent: PositiveFiniteMm,
    majority_step_mm: PositiveFiniteMm,
    samples_per_subsegment: usize,
    policy: &'a ContactPolicy,
}

/// One batched attestation round: the same `radii[i]` question asked of every
/// row in `live` at once.
///
/// Both phases go through here, so the ladder and the scan cannot drift apart
/// about what "attested" means, and so `grow_tips_to_attest` keeps exactly ONE
/// [`attest_members_batch`] call site however many phases it grows.
fn attest_growth_round(
    context: GrowthRoundContext<'_>,
    requests: &[TipGrowthRequest],
    live: &[usize],
    radii: &[f64],
) -> Result<Vec<ContactAttestation>, DesignedMaskError> {
    let members = live
        .iter()
        .zip(radii)
        .map(|(index, radius)| requests[*index].provisional_member(*radius))
        .collect::<Result<Vec<_>, _>>()?;
    let borrowed = members.iter().collect::<Vec<_>>();
    let attest_requests = live
        .iter()
        .zip(radii)
        .enumerate()
        .map(|(row, (index, radius))| ContactAttestationRequest {
            member: row,
            end: requests[*index].end,
            extent: context.extent,
            exterior_probe: requests[*index].exterior_probe,
            end_radius_mm: *radius,
            carried_mm2: requests[*index].carried_mm2,
            tip_clamped_at_max: requests[*index].tip_clamped_at_max,
        })
        .collect::<Vec<_>>();
    let limits = EndpointMaskValidationLimits::for_batch(
        context.surface,
        attest_requests.len(),
        context.majority_step_mm,
        context.samples_per_subsegment,
    )?;
    let results = attest_members_batch(
        context.surface,
        &borrowed,
        &attest_requests,
        limits,
        context.policy,
    )?;
    if results.len() != live.len() {
        return Err(DesignedMaskError::CorruptKernelResult);
    }
    Ok(results)
}

/// **Grow every refused tip until it attests, in ROUNDS rather than per tip.**
///
/// **Two phases, and the emitted tip is the smallest radius either of them
/// measured as attesting.** The ascending ladder BRACKETS the answer — first
/// success wins, no monotonicity assumed — and the descending refinement scan
/// then walks back down inside that bracket at the ring's own step, keeping the
/// smallest radius that still attests. Without the second phase the emitted tip
/// can be a whole area doubling (measured: 4.6x–5.7x the law area) larger than
/// physics asked for, and every excess millimetre of it is scar on the model.
///
/// Batch-first, and the shape of the batching is the whole point. Round `k`
/// gathers the `k`-th rung of every contact still unresolved and issues ONE
/// delegated [`attest_members_batch`] for all of them. A contact that attests
/// leaves the round set, so the work falls with each round instead of rising:
/// the cost is `rounds x batched calls`, and `rounds` is the ladder depth — at
/// most nine across the whole registered clamp span — NEVER the contact count.
/// The scan rounds batch the same way and add at most THREE more, so the whole
/// remedy is bounded at **12 rounds** end to end. A loop that attested one grown
/// tip at a time would be exactly the per-item native call this module exists to
/// prevent.
///
/// The delegation re-runs OD-9's parity probe each round on the rows still in
/// play. That is redundant — the probe point does not move with the tip radius —
/// and it is kept anyway: one attestation entry point that always runs both
/// screens is worth more than a second, subtly cheaper one that could drift out
/// of agreement with it about what "attested" means.
///
/// The outer `Err` remains a batch-level integrity failure, exactly as it is for
/// the callers this delegates to. Nothing can be concluded about any row from
/// it.
pub(crate) fn grow_tips_to_attest(
    surface: &NativeSurface,
    requests: &[TipGrowthRequest],
    extent: PositiveFiniteMm,
    majority_step_mm: PositiveFiniteMm,
    samples_per_subsegment: usize,
    policy: &ContactPolicy,
) -> Result<Vec<TipGrowthOutcome>, DesignedMaskError> {
    // Fixed for the whole remedy: both phases attest under exactly these
    // conditions, so a difference between them can only ever be the radius.
    let context = GrowthRoundContext {
        surface,
        extent,
        majority_step_mm,
        samples_per_subsegment,
        policy,
    };
    // THE CEILING, decided once per row before any rung is walked. It is the
    // smaller of the registered scar bound and the registered legacy maximum,
    // and it is carried into the outcome so the disclosure states the limit
    // this run actually walked to and names the constant it came from.
    let ceilings = requests.iter().map(growth_ceiling).collect::<Vec<_>>();
    let ladders = requests
        .iter()
        .zip(&ceilings)
        .map(|(request, (ceiling_mm, _))| tip_growth_ladder(request.law_radius_mm, *ceiling_mm))
        .collect::<Vec<_>>();
    let mut outcomes = requests
        .iter()
        .zip(&ceilings)
        .map(|(request, (ceiling_mm, source))| TipGrowthOutcome {
            row: request.row,
            law_radius_mm: request.law_radius_mm,
            growth_ceiling_radius_mm: *ceiling_mm,
            growth_ceiling_source: *source,
            attempts: Vec::new(),
            cure: None,
        })
        .collect::<Vec<_>>();
    // SABOTAGE SEAM. Zero rounds is exactly the engine before grow-to-attest
    // existed: every ladder computed and none of it walked, so a refused tip
    // stays refused and its load is abandoned. It is here, at the loop rather
    // than at [`tip_growth_ladder`], because the ladder's own arithmetic is
    // what the growth tests assert and sabotaging it would make the test
    // disagree with itself.
    let deepest = if growth_rounds_disabled() {
        0
    } else {
        ladders.iter().map(Vec::len).max().unwrap_or(0)
    };

    // Which ladder rung cured each row, so the scan below knows the rung
    // immediately BELOW the cure — the last radius that was measured and failed,
    // and therefore the exclusive lower end of the bracket.
    let mut cure_rung = vec![None; requests.len()];

    for rung in 0..deepest {
        // Only rows still unresolved AND still having a rung left to try.
        let live = (0..requests.len())
            .filter(|index| outcomes[*index].cure.is_none() && rung < ladders[*index].len())
            .collect::<Vec<_>>();
        if live.is_empty() {
            break;
        }
        let radii = live
            .iter()
            .map(|index| ladders[*index][rung])
            .collect::<Vec<_>>();
        let results = attest_growth_round(context, requests, &live, &radii)?;
        for result in results {
            let index = *live
                .get(result.member)
                .ok_or(DesignedMaskError::CorruptKernelResult)?;
            let radius_mm = ladders[index][rung];
            let attested = result.verdict.is_ok();
            outcomes[index].attempts.push(TipGrowthAttempt {
                radius_mm,
                attested,
            });
            if attested {
                cure_rung[index] = Some(rung);
                outcomes[index].cure = Some(cure_for(&requests[index], radius_mm, result));
            }
        }
    }

    // THE DESCENDING REFINEMENT SCAN, inside the bracket the ladder closed.
    //
    // The ladder returns the smallest radius ON THE LADDER, which can be up to a
    // whole area doubling above the smallest radius that would have worked. Now
    // that the answer is bracketed, walk back down through it at the ring's own
    // refinement step and keep the smallest radius that still attests.
    //
    // Same batching shape as the ladder, and the same reason: scan step `j`
    // gathers the `j`-th candidate of every CURED row and issues ONE delegated
    // round. At most three steps per row (see [`tip_refinement_scan`]), so the
    // whole remedy is bounded at 9 + 3 = 12 rounds — rounds, never contacts.
    let scans = (0..requests.len())
        .map(|index| {
            cure_rung[index].map_or_else(Vec::new, |rung| {
                let last_failed_mm = if rung == 0 {
                    // Rung zero's predecessor is the law radius, which the
                    // ordinary attestation measured and refused before growth
                    // was asked for.
                    requests[index].law_radius_mm
                } else {
                    ladders[index][rung - 1]
                };
                tip_refinement_scan(ladders[index][rung], last_failed_mm)
            })
        })
        .collect::<Vec<_>>();
    let deepest_scan = scans.iter().map(Vec::len).max().unwrap_or(0);

    for step in 0..deepest_scan {
        // EVERY candidate is measured. There is no early exit on a failure:
        // attestation is not monotone in radius, so a smaller candidate can
        // attest where a larger one did not, and stopping at the first failure
        // would be the same unsound monotonicity assumption the ladder refuses
        // to make.
        let live = (0..requests.len())
            .filter(|index| step < scans[*index].len())
            .collect::<Vec<_>>();
        if live.is_empty() {
            break;
        }
        let radii = live
            .iter()
            .map(|index| scans[*index][step])
            .collect::<Vec<_>>();
        let results = attest_growth_round(context, requests, &live, &radii)?;
        for result in results {
            let index = *live
                .get(result.member)
                .ok_or(DesignedMaskError::CorruptKernelResult)?;
            let radius_mm = scans[index][step];
            let attested = result.verdict.is_ok();
            outcomes[index].attempts.push(TipGrowthAttempt {
                radius_mm,
                attested,
            });
            // KEEP-SMALLEST-SUCCESS over the scanned set, stated as a
            // comparison rather than relying on the descent's order.
            let smaller = outcomes[index]
                .cure
                .as_ref()
                .is_none_or(|cure| radius_mm < cure.radius_mm);
            if attested && smaller {
                outcomes[index].cure = Some(cure_for(&requests[index], radius_mm, result));
            }
        }
    }
    Ok(outcomes)
}

/// The emitted geometry for one winning radius: the tip, the non-inverting
/// opposite end, and the attestation earned AT that radius.
fn cure_for(
    request: &TipGrowthRequest,
    radius_mm: f64,
    attestation: ContactAttestation,
) -> TipGrowthCure {
    let (r0_mm, r1_mm) = request.radii_for(radius_mm);
    TipGrowthCure {
        radius_mm,
        opposite_end_radius_mm: match request.end {
            ContactEnd::Terminalward => r1_mm,
            ContactEnd::Rootward => r0_mm,
        },
        attestation,
    }
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
            Err(ContactAttestationRefusal::UnusableGeometry { .. }) => 3,
        });
        // FU-88 — a discarded candidate carries WHICH check discarded it into
        // the transcript, so a screen that starts firing for a different reason
        // is visible rather than folded into one tag.
        if let Err(ContactAttestationRefusal::UnusableGeometry { cause, .. }) = &outcome.verdict {
            digest.tag(cause.tag());
        }
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
thread_local! {
    /// `(row, s_a)` — overwrite the first interval of one row's evidence.
    static ENDPOINT_INTERVAL_SABOTAGE: std::cell::Cell<Option<(usize, f64)>> =
        const { std::cell::Cell::new(None) };
    /// Run zero growth rounds — see [`with_growth_rounds_disabled`].
    static TIP_GROWTH_ROUNDS_DISABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Ignore the registered scar bound — see [`with_scar_bound_disabled`].
    static TIP_SCAR_BOUND_DISABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the growth ladder is being walked. Always `true` in production: the
/// switch below exists only in a test build.
fn growth_rounds_disabled() -> bool {
    #[cfg(test)]
    {
        TIP_GROWTH_ROUNDS_DISABLED.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    {
        false
    }
}

/// **Test-only sabotage of grow-to-attest**, so one fixture can be run BOTH
/// ways through the real production path.
///
/// The growth tests need the counterfactual, not just the outcome. Asserting
/// only that a contact survives proves nothing about growth — the contact might
/// have attested at its load-sized tip all along, and the assertion would stay
/// green with the whole ladder deleted. Running the same fixture with the rounds
/// off shows that this contact genuinely depends on growth: without it the seat
/// is refused `NotAttached`, quarantined, and its routed load abandoned.
///
/// Thread-local, and disarmed on the way out, exactly like
/// [`with_sabotaged_interval`].
#[cfg(test)]
pub(crate) fn with_growth_rounds_disabled<T>(body: impl FnOnce() -> T) -> T {
    TIP_GROWTH_ROUNDS_DISABLED.with(|cell| cell.set(true));
    let outcome = body();
    TIP_GROWTH_ROUNDS_DISABLED.with(|cell| cell.set(false));
    outcome
}

/// Whether the registered scar bound is being honoured. Always `false` in
/// production: the switch below exists only in a test build.
fn scar_bound_disabled() -> bool {
    #[cfg(test)]
    {
        TIP_SCAR_BOUND_DISABLED.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    {
        false
    }
}

/// **Test-only sabotage of the scar bound**, so one fixture can be run BOTH
/// ways through the real production path.
///
/// The bound's whole claim is a counterfactual: a seat whose cure would need a
/// tip wider than `TIP_RADIUS_MAX_GROWN_SCAR_MM` must be REFUSED, and the only
/// way to show a refusal is the bound's doing — rather than a seat that was
/// incurable anyway — is to lift the bound and watch the same fixture cure at
/// an over-bound radius. A revert that deletes the truncation therefore turns
/// the guard test red with the over-bound scar it would have built named in
/// the failure.
///
/// Thread-local, and disarmed on the way out, exactly like
/// [`with_growth_rounds_disabled`].
#[cfg(test)]
pub(crate) fn with_scar_bound_disabled<T>(body: impl FnOnce() -> T) -> T {
    TIP_SCAR_BOUND_DISABLED.with(|cell| cell.set(true));
    let outcome = body();
    TIP_SCAR_BOUND_DISABLED.with(|cell| cell.set(false));
    outcome
}

/// Test-only evidence injection, so the two DIVERGENT per-row policies can be
/// driven through the real path on the SAME row.
///
/// `attest_seats_batch` must discard an ungradable CANDIDATE row and grade the
/// rest; the PLACED caller — `initial.rs`'s `attest_initial_contacts` — must
/// refuse totally on it. Getting that divergence wrong is the specific defect
/// #28 is about, and no fixture can make a real kernel emit an out-of-span
/// boundary on demand — the values that actually occur are all a few ULP out,
/// i.e. INSIDE what the fix admits.
#[cfg(test)]
fn sabotage_endpoint_intervals(
    mut interval_rows: Vec<Vec<geom_kernel::Interval>>,
) -> Vec<Vec<geom_kernel::Interval>> {
    if let Some((row, s_a)) = ENDPOINT_INTERVAL_SABOTAGE.with(std::cell::Cell::get) {
        if let Some(interval) = interval_rows.get_mut(row).and_then(|row| row.first_mut()) {
            interval.s_a = s_a;
        }
    }
    interval_rows
}

/// Arm the injection for the duration of `body`, then disarm it. The state is
/// thread-local, so a panicking `body` leaves it armed only on the thread that
/// is already failing.
#[cfg(test)]
pub(crate) fn with_sabotaged_interval<T>(row: usize, s_a: f64, body: impl FnOnce() -> T) -> T {
    ENDPOINT_INTERVAL_SABOTAGE.with(|cell| cell.set(Some((row, s_a))));
    let outcome = body();
    ENDPOINT_INTERVAL_SABOTAGE.with(|cell| cell.set(None));
    outcome
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
///   [`attest_members_batch`] still demands `is_total()`.
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
    /// The honest report for a row nothing was measured on: no ray reached
    /// anything because no ray was graded. Zero engaged of zero sampled also
    /// makes [`Self::effective_radius_mm`] return 0.0 rather than crediting an
    /// interface, and it is deliberately NOT usable as a verdict — a refusal
    /// must accompany it, or `is_attached()` would read as `NotAttached`, which
    /// is a claim about geometry that was never examined.
    pub(crate) const UNGRADABLE: Self = Self {
        centreline: false,
        rim_engaged: 0,
        rim_total: 0,
        witness: 0,
    };

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
            let (s_a, s_b) = admit_endpoint_interval(interval, ray.length_mm()).ok_or(
                DesignedMaskError::EndpointEvidenceOutsideAdmittedSpan {
                    ray: interval.ray,
                    s_a: interval.s_a,
                    s_b: interval.s_b,
                    ray_length_mm: ray.length_mm(),
                },
            )?;
            if prior_s_b.is_some_and(|prior| s_a < prior) {
                return Err(DesignedMaskError::CorruptKernelResult);
            }
            if prior_s_b.is_none() {
                // Contiguity from the endpoint is what makes this evidence of
                // ATTACHMENT rather than of the member merely passing through
                // the model somewhere further along.
                //
                // `s_a == 0.0` is an EXACT test, which is why
                // `admit_endpoint_interval` normalizes rather than merely
                // admitting: a boundary the kernel is entitled to report at
                // -8.5e-16 mm is the start of the ray, and comparing the raw
                // value here would silently downgrade a fully engaged end to
                // `NotAttached` instead of crediting it.
                engaged = s_a == 0.0 && s_b >= target;
            }
            digest.u64(u64::from_ne_bytes(i64::from(interval.ray).to_ne_bytes()));
            digest.f64(s_a);
            digest.f64(s_b);
            prior_s_b = Some(s_b);
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

/// Admit one kernel interval against the PRODUCER's acceptance region, and
/// normalize it into the span the grader compares against.
///
/// The kernel's Möller test accepts a hit for `t in [-EPS, t_max + EPS]`
/// ([`geom_kernel::MOLLER_EPS`], ported verbatim from the oracle), so an
/// arc-length boundary may legitimately land up to `MOLLER_EPS * length`
/// outside `[0, length]`. That is not a rare pathology: a Rootward row's rays
/// start AT the snapped seat point, which lies exactly on a mesh triangle by
/// construction, so the triangle under the origin is hit at a `t` of a couple
/// of ULP either side of zero on essentially every such row.
///
/// This predicate used to demand `s_a >= 0.0 && s_b <= length` exactly — the
/// consumer holding the producer to a contract the producer never offered.
/// Measured on the Warhorse and the Giant Bat before the fix: 14355 rows over
/// 1803 batches were reported as corrupt kernel evidence for `s_a` between
/// -7.1e-13 mm and -5.2e-18 mm and `s_b` up to 2.6e-14 mm past the ray, i.e.
/// seven orders of magnitude INSIDE the slack the kernel is allowed. Every one
/// of them was a fully engaged end, and each one aborted a whole batch of
/// 33-40 candidates that had no fault at all.
///
/// Returns the CLAMPED `(s_a, s_b)`. Clamping is load-bearing, not cosmetic:
/// see the exact `s_a == 0.0` test in [`endpoint_engagement`].
fn admit_endpoint_interval(interval: &geom_kernel::Interval, length: f64) -> Option<(f64, f64)> {
    // `MOLLER_EPS * length` is the producer's own acceptance width in
    // millimetres, not a widening chosen here. The kernel casts each ray with
    // `t_max = 1.0`, so its `t` is parametric over the WHOLE ray, and it
    // converts a hit back to arc length as `t * ray_length`. A hit admitted at
    // the extreme of `t in [-MOLLER_EPS, 1 + MOLLER_EPS]` therefore lands at
    // most `MOLLER_EPS * length` outside `[0, length]`, and this bound is
    // exact rather than generous.
    let slack = geom_kernel::MOLLER_EPS * length;
    if !interval.s_a.is_finite()
        || !interval.s_b.is_finite()
        || interval.s_a < -slack
        || interval.s_a > interval.s_b
        || interval.s_b > length + slack
    {
        return None;
    }
    // Clamping is monotone, so it cannot invert an ordered pair.
    Some((
        interval.s_a.clamp(0.0, length),
        interval.s_b.clamp(0.0, length),
    ))
}

// No `Eq`: `EndpointEvidenceOutsideAdmittedSpan` carries the measured
// millimetres that decide whether the producer or the consumer is at fault, and
// a measurement is worth more here than a total equality that nothing uses.
#[derive(Clone, Copy, Debug, PartialEq, Error)]
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
    /// Genuinely unattributable evidence: an interval left over after every ray
    /// was matched, an out-of-order interval, or a result whose row count does
    /// not match the rows that were sent. Nothing can be said about the row.
    ///
    /// This variant used to cover
    /// [`Self::EndpointEvidenceOutsideAdmittedSpan`] too, which is how a
    /// validator that was stricter than the kernel's own contract came to be
    /// reported as the kernel being broken.
    #[error("geometry kernel returned corrupt endpoint mask evidence")]
    CorruptKernelResult,
    /// The kernel's evidence is well formed but one interval boundary lies
    /// outside the span this validator admits — which is the kernel's own
    /// Möller acceptance region, `[-MOLLER_EPS * length, length * (1 +
    /// MOLLER_EPS)]`, not a stricter one of the validator's own invention.
    ///
    /// It carries the measured numbers because the distinguishing fact about
    /// this refusal is HOW FAR outside, and the answer decides whether the
    /// producer or the consumer is at fault.
    #[error(
        "endpoint interval [{s_a}, {s_b}] mm on ray {ray} lies outside the admitted span of its \
         {ray_length_mm} mm ray, which already allows the geometry kernel's own {} mm of Moller \
         slack at each end",
        .ray_length_mm * geom_kernel::MOLLER_EPS
    )]
    EndpointEvidenceOutsideAdmittedSpan {
        /// -1 for the centreline, `k >= 0` for rim ray `k`.
        ray: i32,
        s_a: f64,
        s_b: f64,
        ray_length_mm: f64,
    },
    #[error("designed embed geometry is not connected to its endpoint: {end:?}")]
    DisconnectedGeometry { end: ContactEnd },
    /// `docs/specs/geometry-adjudication.md` §EXT-1 site 15 — at least one of
    /// this row's rays reached a face whose crossing was UNDECIDABLE
    /// (`RayTriangleClass::NearParallel`: a nonzero determinant inside the
    /// epsilon band, where whether the ray crosses the face cannot be settled
    /// at this scale).
    ///
    /// The refusal is NOT the kernel being broken — that is
    /// [`Self::CorruptKernelResult`], and conflating the two is what the
    /// separate variant prevents. The kernel answered correctly; the answer is
    /// that this row's interval evidence is INCOMPLETE by an unknown amount,
    /// and a connectivity witness assembled from it would affirm an engagement
    /// nobody measured. The count is carried because the distinguishing fact is
    /// how much evidence went missing.
    ///
    /// Appended, per the frozen-tag rule on [`Self::tag`].
    #[error(
        "endpoint mask validation reached {near_parallel} face(s) whose crossing is undecidable at \
         this scale, so the connectivity witness cannot be affirmed"
    )]
    NearParallelWitness { near_parallel: u32 },
}

impl DesignedMaskError {
    /// Stable per-variant byte for the seat-attestation transcript. Values are
    /// frozen: append new variants, never renumber an existing one, or every
    /// recorded digest moves without the geometry moving.
    #[must_use]
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::InvalidExtent => 1,
            Self::ExtentOutOfSpan => 2,
            Self::InvalidMemberSpan => 3,
            Self::OverlappingExtents => 4,
            Self::StaleBinding => 5,
            Self::MissingRequiredExtent { .. } => 6,
            Self::EndNotDesigned { .. } => 7,
            Self::DuplicateRequest { .. } => 8,
            Self::UnknownMember => 9,
            Self::InvalidValidationLimits => 10,
            Self::ResourceLimit => 11,
            Self::KernelRefusal => 12,
            Self::CorruptKernelResult => 13,
            Self::DisconnectedGeometry { .. } => 14,
            Self::EndpointEvidenceOutsideAdmittedSpan { .. } => 15,
            // §EXT-1 site 15. APPENDED, never renumbered: the fifteen above are
            // load-bearing in the recorded seat-attestation transcript and
            // reordering them would move every model's digest with no geometry
            // moving.
            Self::NearParallelWitness { .. } => 16,
        }
    }

    /// **This cause's snake_case NAME, for a histogram an operator reads.**
    ///
    /// `undecidable_geometry` is deliberately the same word
    /// [`crate::runner`]'s `AbandonedContactDisclosure` already prints for
    /// [`Self::NearParallelWitness`], and for the same reason it prints it
    /// there: a run must not have two vocabularies for one refusal. Every
    /// other variant carries its own name rather than being folded into one
    /// `unusable_geometry` bucket, because a screen that removes candidates
    /// has to be able to say WHICH check removed them — a single bucket would
    /// hide a screen that started refusing on `ResourceLimit` behind a count
    /// that reads like geometry.
    #[must_use]
    pub(crate) const fn cause_name(self) -> &'static str {
        match self {
            Self::InvalidExtent => "invalid_extent",
            Self::ExtentOutOfSpan => "extent_out_of_span",
            Self::InvalidMemberSpan => "invalid_member_span",
            Self::OverlappingExtents => "overlapping_extents",
            Self::StaleBinding => "stale_binding",
            Self::MissingRequiredExtent { .. } => "missing_required_extent",
            Self::EndNotDesigned { .. } => "end_not_designed",
            Self::DuplicateRequest { .. } => "duplicate_request",
            Self::UnknownMember => "unknown_member",
            Self::InvalidValidationLimits => "invalid_validation_limits",
            Self::ResourceLimit => "resource_limit",
            Self::KernelRefusal => "kernel_refusal",
            Self::CorruptKernelResult => "corrupt_kernel_result",
            Self::DisconnectedGeometry { .. } => "disconnected_geometry",
            Self::EndpointEvidenceOutsideAdmittedSpan { .. } => {
                "endpoint_evidence_outside_admitted_span"
            }
            Self::NearParallelWitness { .. } => "undecidable_geometry",
        }
    }
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

    // ---------------------------------------------------------------------
    // PR-8 / OD-9
    // ---------------------------------------------------------------------

    fn attestation_policy() -> ContactPolicy {
        ContactPolicy {
            floor_depth_mm: 0.3,
            process_resolution_mm: 0.05,
            standoff_mm: 0.6,
            demanded_gap_mm: 0.6,
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

    /// **FU-88 acceptance — one bad candidate out of forty costs that candidate
    /// and nothing else.**
    ///
    /// The numbers are the Giant Toad's, measured: a batch of 40 model-anchor
    /// seats, one of them proposing a 0.049966772 mm arm against the 0.050 mm
    /// process resolution the attestation claims — 33 NANOMETRES short. Before
    /// this change that single row aborted the whole model with the other 39
    /// candidates never measured, and the model fell back to solid.
    ///
    /// Four things are pinned together, because any one of them alone could be
    /// satisfied by a wrong fix:
    ///
    /// 1. the 39 good candidates still attest;
    /// 2. the bad one is refused, with the check that refused it named;
    /// 3. the 0.050 mm requirement was NOT lowered — an arm 33 nanometres
    ///    LONGER than the extent attests, so the boundary is exactly where it
    ///    was and only the blast radius moved;
    /// 4. the PLACED-member entry point still fails the whole batch on the very
    ///    same row, because a caller that hands `attest_members_batch` an
    ///    unusable member has a defect and nothing can be concluded from it.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn one_unusable_candidate_is_discarded_and_the_other_thirty_nine_still_attest() {
        const DEPTH: f64 = 0.3;
        const RADIUS: f64 = 0.2;
        const EXTENT_MM: f64 = 0.05;
        /// The Giant Toad's measured anchor arm: 33 nanometres under the extent.
        const TOO_SHORT_MM: f64 = 0.049_966_772;
        const BAD_ROW: usize = 17;
        const ROWS: usize = 40;

        let surface = interpenetrating_pair();
        // Every seat sits on B's EXTERIOR +x face, so nothing here is refused
        // for burial and the only thing under test is the length screen.
        let seat_x = 6.0 - DEPTH;
        let stub_row = |y: f64, length_mm: f64| SeatAttestationRow {
            p0_mm: [seat_x + length_mm, y, 1.0],
            p1_mm: [seat_x, y, 1.0],
            r0_mm: RADIUS,
            r1_mm: RADIUS,
            end: ContactEnd::Rootward,
            exterior_probe: [6.05, y, 1.0],
            carried_mm2: 0.1,
        };
        let ys = |row: usize| 0.05 + 1.9 * (row as f64) / (ROWS as f64);
        let seats = (0..ROWS)
            .map(|row| {
                if row == BAD_ROW {
                    stub_row(ys(row), TOO_SHORT_MM)
                } else {
                    touching_seat_row(6.0, ys(row), DEPTH, RADIUS, 6.05, 0.1)
                }
            })
            .collect::<Vec<_>>();

        reset_endpoint_validation_kernel_calls();
        let outcomes = attest_seats_batch(
            &surface,
            &seats,
            extent(EXTENT_MM),
            validation_limits(&surface, seats.len()),
            &attestation_policy(),
        )
        .expect("one unusable candidate must not refuse the batch");

        // Batch-first is preserved: screening the rows individually must not
        // turn one native call into forty.
        assert_eq!(endpoint_validation_kernel_calls(), 1);
        assert_eq!(outcomes.len(), ROWS);
        for (index, outcome) in outcomes.iter().enumerate() {
            assert_eq!(
                outcome.member, index,
                "row {index} must keep the caller's seat index after the \
                 survivors were compacted for the kernel"
            );
        }
        assert_eq!(
            outcomes[BAD_ROW].verdict,
            Err(ContactAttestationRefusal::UnusableGeometry {
                end: ContactEnd::Rootward,
                cause: DesignedMaskError::ExtentOutOfSpan,
            }),
            "the sub-resolution arm is the candidate that must be discarded"
        );
        let attested = outcomes
            .iter()
            .filter(|outcome| outcome.verdict.is_ok())
            .count();
        assert_eq!(
            attested,
            ROWS - 1,
            "the other thirty-nine candidates must still be measured and attest"
        );
        // A discarded candidate claims nothing it did not measure.
        assert_eq!(outcomes[BAD_ROW].attested_radius_mm, 0.0);
        assert_eq!(outcomes[BAD_ROW].engagement.rim_engaged, 0);
        assert!(!outcomes[BAD_ROW].engagement.centreline);
        assert_eq!(outcomes[BAD_ROW].area_deficit, None);

        // (3) THE REQUIREMENT DID NOT MOVE. The same row, 33 nanometres LONGER
        // than the extent instead of 33 shorter, attests. Anything that passed
        // this batch before still passes it.
        let mut just_long_enough = seats.clone();
        just_long_enough[BAD_ROW] = stub_row(ys(BAD_ROW), EXTENT_MM + 33e-9);
        let widened = attest_seats_batch(
            &surface,
            &just_long_enough,
            extent(EXTENT_MM),
            validation_limits(&surface, ROWS),
            &attestation_policy(),
        )
        .expect("a long-enough batch is unaffected");
        assert!(
            widened[BAD_ROW].verdict.is_ok(),
            "an arm 33 nanometres LONGER than the claimed extent must still \
             attest -- the screen is the pre-existing one, not a new minimum: \
             {:?}",
            widened[BAD_ROW].verdict
        );
        assert_eq!(
            widened
                .iter()
                .filter(|outcome| outcome.verdict.is_ok())
                .count(),
            ROWS
        );

        // (4) The PLACED-member entry point is deliberately UNCHANGED: the very
        // same row still refuses the whole batch there, before any kernel call.
        let members = seats
            .iter()
            .map(SeatAttestationRow::provisional_member)
            .collect::<Result<Vec<_>, _>>()
            .expect("every provisional member is well formed on its own");
        let borrowed = members.iter().collect::<Vec<_>>();
        let requests = seats
            .iter()
            .enumerate()
            .map(|(index, seat)| ContactAttestationRequest {
                member: index,
                end: seat.end,
                extent: extent(EXTENT_MM),
                exterior_probe: seat.exterior_probe,
                end_radius_mm: seat.end_radius_mm(),
                carried_mm2: seat.carried_mm2,
                tip_clamped_at_max: false,
            })
            .collect::<Vec<_>>();
        reset_endpoint_validation_kernel_calls();
        assert_eq!(
            attest_members_batch(
                &surface,
                &borrowed,
                &requests,
                validation_limits(&surface, ROWS),
                &attestation_policy(),
            ),
            Err(DesignedMaskError::ExtentOutOfSpan),
            "a PLACED member that cannot carry its extent is still a caller \
             defect and still refuses the whole batch"
        );
        assert_eq!(
            endpoint_validation_kernel_calls(),
            0,
            "and it still costs nothing to find out"
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
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
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
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
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

    // ------------------------------------------------------------------
    // #28 — the endpoint validator was STRICTER than the kernel's own
    // acceptance region, and reported the difference as kernel corruption.
    // ------------------------------------------------------------------

    /// The straight 4.0 mm cylinder every #28 unit test grades.
    ///
    /// Equal end radii make all nine rays exactly 4.0 mm long — the centreline
    /// runs `p0 -> p1`, and each rim ray is that same axis translated sideways
    /// by `r` — so every arc length below is an exact millimetre value a reader
    /// can check by eye rather than a number re-derived from production code.
    const ULP_RAY_LENGTH_MM: f64 = 4.0;
    /// `MOLLER_EPS * 4.0 mm`: the slack the kernel is entitled to on this ray.
    /// Written out, not computed, so widening the tolerance in production
    /// cannot widen the test with it.
    const ULP_ADMITTED_SLACK_MM: f64 = 4e-6;

    /// A 0.2 mm rootward extent claimed on that member (`fraction = 0.05`).
    fn ulp_row() -> EndpointValidationRow {
        EndpointValidationRow {
            member: 0,
            end: ContactEnd::Rootward,
            extent: extent(0.2),
            fraction: 0.05,
            p0: [0.0, 0.0, 0.0],
            p1: [0.0, 0.0, ULP_RAY_LENGTH_MM],
            r0: 0.5,
            r1: 0.5,
        }
    }

    fn ulp_interval(ray: i32, s_a: f64, s_b: f64) -> geom_kernel::Interval {
        geom_kernel::Interval {
            s_a,
            s_b,
            entered: true,
            exited: true,
            max_depth: 0.5,
            ray,
        }
    }

    /// Nine rays engaged end to end, with the CENTRELINE's own boundaries
    /// substituted. This is the shape of every row that failed in production:
    /// fully in the solid from `s = 0` to the end of the ray, with one boundary
    /// a couple of ULP off the exact end.
    fn ulp_fully_engaged(centreline_s_a: f64, centreline_s_b: f64) -> Vec<geom_kernel::Interval> {
        let mut intervals = vec![ulp_interval(-1, centreline_s_a, centreline_s_b)];
        intervals.extend((0..8).map(|k| ulp_interval(k, 0.0, ULP_RAY_LENGTH_MM)));
        intervals
    }

    /// **#28 acceptance — the first failing Warhorse row, verbatim.**
    ///
    /// `s_a = -8.481941181155805e-16 mm`, about 1.5 ULP of negative arc length,
    /// on a ray whose `s_b` is the full ray length: the end is engaged from end
    /// to end. Before the fix this was `CorruptKernelResult`, and it took the
    /// other 32 healthy candidates in its batch down with it. Measured across
    /// two models: 14355 such rows over 1803 batches, and NOT ONE of those
    /// batches was entirely bad.
    #[test]
    fn a_ulp_negative_interval_start_is_a_fully_engaged_end_not_corruption() {
        let engagement = endpoint_engagement(
            &ulp_row(),
            &ulp_fully_engaged(-8.481_941_181_155_805e-16, ULP_RAY_LENGTH_MM),
        )
        .expect("evidence inside the kernel's own Moller slack must be gradable");
        // THE TRAP. Admitting the row without NORMALIZING it leaves the exact
        // `s_a == 0.0` engagement test false, which converts a loud batch abort
        // into a silent `NotAttached` and deletes the best candidates instead
        // of crediting them.
        assert!(
            engagement.centreline,
            "the centreline ray is in the solid from end to end; admitting the \
             row without clamping would report it as not attached"
        );
        assert_eq!((engagement.rim_engaged, engagement.rim_total), (8, 8));
        assert!(engagement.is_total());

        // The same thing at the far end: `s_b` past the ray by 2.6e-14 mm, the
        // magnitude measured on the Giant Bat's 40 rim-ray rows.
        let overshoot =
            endpoint_engagement(&ulp_row(), &ulp_fully_engaged(0.0, 4.000_000_000_000_026))
                .expect("an overshoot inside the kernel's slack must be gradable");
        assert!(overshoot.is_total());
    }

    /// Normalization is real, not an admission that leaves the numbers alone.
    ///
    /// The witness digests every interval boundary the grader used, so a row
    /// clamped into `[0, length]` must produce the SAME witness as the row that
    /// arrived exactly on the boundary. That also pins the other direction: no
    /// currently-passing row's digest can move, because clamping only touches
    /// values that used to be refused outright.
    #[test]
    fn a_ulp_boundary_is_normalized_into_the_span_not_merely_admitted() {
        let exact =
            endpoint_engagement(&ulp_row(), &ulp_fully_engaged(0.0, ULP_RAY_LENGTH_MM)).unwrap();
        let below = endpoint_engagement(
            &ulp_row(),
            &ulp_fully_engaged(-8.481_941_181_155_805e-16, ULP_RAY_LENGTH_MM),
        )
        .unwrap();
        let above = endpoint_engagement(&ulp_row(), &ulp_fully_engaged(0.0, 4.000_000_000_000_026))
            .unwrap();
        assert_eq!(below, exact);
        assert_eq!(above, exact);
        assert_eq!(below.witness, exact.witness);
        assert_eq!(above.witness, exact.witness);
    }

    /// **The sabotage.** The admitted span is the kernel's slack and nothing
    /// wider, and stepping one nanometre past it still refuses — by a name that
    /// says which check fired and what it measured.
    #[test]
    fn the_admitted_span_is_the_kernels_own_slack_and_nothing_wider() {
        let inside = endpoint_engagement(
            &ulp_row(),
            &ulp_fully_engaged(-(ULP_ADMITTED_SLACK_MM - 1e-9), ULP_RAY_LENGTH_MM),
        )
        .expect("a boundary inside the kernel's documented slack is gradable");
        assert!(inside.is_total());

        let past = -(ULP_ADMITTED_SLACK_MM + 1e-9);
        let outside = endpoint_engagement(&ulp_row(), &ulp_fully_engaged(past, ULP_RAY_LENGTH_MM))
            .expect_err("a boundary past the kernel's slack is not the kernel's own");
        assert_eq!(
            outside,
            DesignedMaskError::EndpointEvidenceOutsideAdmittedSpan {
                ray: -1,
                s_a: past,
                s_b: ULP_RAY_LENGTH_MM,
                ray_length_mm: ULP_RAY_LENGTH_MM,
            }
        );
        // Truthful naming (aac205b): the refusal names the real check and
        // carries the measured value, and it does NOT accuse the producer.
        let message = outside.to_string();
        assert!(message.contains("on ray -1"), "{message}");
        assert!(
            message.contains("lies outside the admitted span"),
            "{message}"
        );
        assert!(
            !message.contains("corrupt"),
            "well-formed evidence outside a consumer's admitted span is a \
             disagreement, not corruption: {message}"
        );
    }

    /// The `CorruptKernelResult` name survives for what it was always for:
    /// evidence that cannot be attributed to a ray at all.
    #[test]
    fn genuinely_unattributable_evidence_is_still_named_corrupt() {
        // A tenth rim ray, on a member that has eight.
        let mut orphan = ulp_fully_engaged(0.0, ULP_RAY_LENGTH_MM);
        orphan.push(ulp_interval(9, 0.0, ULP_RAY_LENGTH_MM));
        assert_eq!(
            endpoint_engagement(&ulp_row(), &orphan),
            Err(DesignedMaskError::CorruptKernelResult)
        );

        // Two intervals on one ray, out of order.
        let mut backwards = vec![ulp_interval(-1, 2.0, 3.0), ulp_interval(-1, 0.5, 1.0)];
        backwards.extend((0..8).map(|k| ulp_interval(k, 0.0, ULP_RAY_LENGTH_MM)));
        assert_eq!(
            endpoint_engagement(&ulp_row(), &backwards),
            Err(DesignedMaskError::CorruptKernelResult)
        );
    }

    /// An ungradable row is refused as UNUSABLE GEOMETRY, never as
    /// `NotAttached` — and OD-9 still outranks both.
    ///
    /// The trap this closes: an ungradable row has no engagement, so it carries
    /// [`EndpointEngagement::UNGRADABLE`], and zero engaged rays read exactly
    /// like a detached tip. Deleting the ungradable arm would therefore not
    /// fail to compile and not fail any count-based assertion — it would just
    /// start making a claim about geometry nobody measured.
    #[test]
    fn an_ungradable_row_refuses_as_unusable_geometry_not_as_unattached() {
        let cause = DesignedMaskError::EndpointEvidenceOutsideAdmittedSpan {
            ray: -1,
            s_a: -1.0,
            s_b: ULP_RAY_LENGTH_MM,
            ray_length_mm: ULP_RAY_LENGTH_MM,
        };
        assert_eq!(
            screen_attestation(ContactEnd::Rootward, false, &Err(cause)),
            Some(ContactAttestationRefusal::UnusableGeometry {
                end: ContactEnd::Rootward,
                cause,
            })
        );
        // OD-9 first: the parity probe is independent evidence, and it is the
        // screen the connectivity proof passes trivially.
        assert_eq!(
            screen_attestation(ContactEnd::Rootward, true, &Err(cause)),
            Some(ContactAttestationRefusal::BuriedSurface {
                end: ContactEnd::Rootward
            })
        );
        // The SAME zero-valued engagement, arriving as a GRADED result, is a
        // genuine `NotAttached` — which is the reading the ungradable arm has
        // to intercept.
        assert_eq!(
            screen_attestation(
                ContactEnd::Rootward,
                false,
                &Ok(EndpointEngagement::UNGRADABLE)
            ),
            Some(ContactAttestationRefusal::NotAttached {
                end: ContactEnd::Rootward
            })
        );
        let engaged =
            endpoint_engagement(&ulp_row(), &ulp_fully_engaged(0.0, ULP_RAY_LENGTH_MM)).unwrap();
        assert_eq!(
            screen_attestation(ContactEnd::Rootward, false, &Ok(engaged)),
            None
        );
    }

    /// **#28 acceptance, whole path — one ungradable row out of eight costs
    /// that row and nothing else.**
    ///
    /// This is FU-88's argument one layer down. The matching PLACED-caller half
    /// — the same injected row refusing an entire build — is pinned by
    /// `initial.rs`'s `attest_initial_contacts` test, which drives this same
    /// `with_sabotaged_interval` injection; the two together are what keep a
    /// per-row witness from letting a defective PLACED member through.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn one_ungradable_row_is_discarded_while_the_rest_of_the_batch_attests() {
        const DEPTH: f64 = 0.3;
        const RADIUS: f64 = 0.2;
        const EXTENT_MM: f64 = 0.05;
        const ROWS: usize = 8;
        const BAD_ROW: usize = 3;
        /// Far outside anything `MOLLER_EPS` could excuse — a whole millimetre
        /// of negative arc length, not the 1e-16 the kernel legitimately emits.
        const OUT_OF_SPAN_S_A: f64 = -1.0;

        let surface = interpenetrating_pair();
        // Kept clear of the box's y and z walls by more than one radius, so
        // every rim ray starts inside the solid and the engagement is TOTAL —
        // which is what the placed-member half of this test needs.
        let ys = |row: usize| 0.4 + 1.2 * (row as f64) / (ROWS as f64);
        let seats = (0..ROWS)
            .map(|row| touching_seat_row(6.0, ys(row), DEPTH, RADIUS, 6.05, 0.1))
            .collect::<Vec<_>>();

        // Baseline: with nothing injected every seat attests, so the discard
        // below is caused by the injected evidence and nothing else.
        let clean = attest_seats_batch(
            &surface,
            &seats,
            extent(EXTENT_MM),
            validation_limits(&surface, ROWS),
            &attestation_policy(),
        )
        .expect("the uninjected batch attests");
        assert_eq!(
            clean.iter().filter(|row| row.verdict.is_ok()).count(),
            ROWS,
            "every seat in this fixture attests before anything is injected"
        );

        reset_endpoint_validation_kernel_calls();
        let outcomes = with_sabotaged_interval(BAD_ROW, OUT_OF_SPAN_S_A, || {
            attest_seats_batch(
                &surface,
                &seats,
                extent(EXTENT_MM),
                validation_limits(&surface, ROWS),
                &attestation_policy(),
            )
        })
        .expect("one ungradable row must not refuse the whole batch");

        // Batch-first is untouched: still exactly one native penetration call.
        assert_eq!(endpoint_validation_kernel_calls(), 1);
        let Err(ContactAttestationRefusal::UnusableGeometry { end, cause }) =
            outcomes[BAD_ROW].verdict
        else {
            panic!(
                "row {BAD_ROW} must be discarded on its own: {:?}",
                outcomes[BAD_ROW].verdict
            );
        };
        assert_eq!(end, ContactEnd::Rootward);
        let DesignedMaskError::EndpointEvidenceOutsideAdmittedSpan { ray, s_a, .. } = cause else {
            panic!("the discard must name the span check, not {cause:?}");
        };
        assert_eq!(ray, -1);
        assert_eq!(s_a, OUT_OF_SPAN_S_A);
        assert_eq!(
            outcomes.iter().filter(|row| row.verdict.is_ok()).count(),
            ROWS - 1,
            "the other seven rows must still be measured and attest"
        );
        // A discarded row claims nothing it did not measure.
        assert_eq!(outcomes[BAD_ROW].attested_radius_mm, 0.0);
        assert_eq!(outcomes[BAD_ROW].engagement, EndpointEngagement::UNGRADABLE);
        assert_eq!(outcomes[BAD_ROW].area_deficit, None);
    }

    // ---------------------------------------------------------------------
    // GROW-TO-ATTEST
    // ---------------------------------------------------------------------

    /// A box `[0,4] x [0,2] x [0,2]`, the same shape the OD-9 fixtures use.
    fn edge_seat_box() -> NativeSurface {
        let mut vertices = Vec::new();
        let mut faces = Vec::new();
        append_box(&mut vertices, &mut faces, [0.0, 0.0, 0.0], [4.0, 2.0, 2.0]);
        surface_from(&vertices, &faces, 0.1)
    }

    /// **THE EDGE SEAT, constructed so the numbers are checkable by hand.**
    ///
    /// The stub meets the box's `+x` face at `x = 4`, but its axis is displaced
    /// `EDGE_OFFSET_MM` PAST the box's `+y` silhouette edge at `y = 2`. So:
    ///
    /// * the CENTRELINE starts at `y = 2 + 0.075`, in air, and can never engage
    ///   however deep it is probed — this is the real corpus case, a tip centre
    ///   lying outside the boundary while part of its disc is inside;
    /// * a rim ray at tip radius `r` reaches inward to `y = 2.075 - r` at best,
    ///   so the ring finds solid if and only if `r > 0.075`.
    ///
    /// That makes the outcome arithmetic rather than luck, for BOTH phases.
    ///
    /// **The ladder** from the registered minimum 0.05 mm radius is
    /// 0.05 -> 0.0707 -> 0.1 -> ..., and the first rung whose ring can reach
    /// `y < 2` is 0.1 mm RADIUS (0.2 mm DIAMETER). The two rungs below it must
    /// fail even for a rim ray pointing exactly at `-y`: 2.075 - 0.05 = 2.025
    /// and 2.075 - 0.0707 = 2.0043 are both outside.
    ///
    /// **The scan** then descends from 0.1 through the bracket `(0.0707, 0.1)`
    /// at the ring's refinement ratio `q = 1 - 2 pi / 8^2 = 0.9018252`. Exactly
    /// three candidates fit — 0.0901825, 0.0813289, 0.0733444 mm radius — and
    /// the fourth, 0.0661439, is below the last failing ladder rung and is never
    /// proposed. Against the same `r > 0.075` rule the first two reach solid and
    /// the third does not, so the cure is
    ///
    /// ```text
    /// 0.1 * q^2 = 0.081328874469857680 mm RADIUS
    ///           = 0.162657748939715360 mm DIAMETER
    /// ```
    ///
    /// which is the smallest tip the scan measured as attesting and sits just
    /// above the 0.075 mm the geometry demands. It cuts the interface area this
    /// contact scars the model with from 4.00x the law area to 2.65x.
    const EDGE_OFFSET_MM: f64 = 0.075;
    const EDGE_SEAT_DEPTH_MM: f64 = 0.3;
    const EDGE_SEAT_EXTENT_MM: f64 = 0.05;
    /// The registered tip-sizing clamp minimum, 0.05 mm RADIUS (0.1 mm
    /// DIAMETER), which is what the load law hands a tiny contact.
    const EDGE_SEAT_LAW_RADIUS_MM: f64 = 0.05;
    /// The load this seat carries. Under the frozen policy's area law it needs
    /// only `sqrt(0.05 / (X * pi))` mm of radius, well below the clamp minimum,
    /// so 0.05 mm radius really is what the law asks for here.
    const EDGE_SEAT_CARRIED_MM2: f64 = 0.05;
    /// The cure the two phases together must land on: `0.1 * q^2` with
    /// `q = 1 - 2 pi / 8^2`. Stated here as the number so a reader can check it
    /// against the derivation on [`EDGE_OFFSET_MM`]; the test ALSO asserts it as
    /// arithmetic through [`tip_refinement_radius_ratio`], so a hand-edited
    /// literal cannot pass on its own.
    const EDGE_SEAT_CURE_RADIUS_MM: f64 = 0.081_328_874_469_857_68;

    /// The registered `TIP_RADIUS_MAX_GROWN_SCAR_MM`, read rather than typed,
    /// so a ruling that moves the bound moves these fixtures with it.
    fn registered_scar_bound_mm() -> f64 {
        *plinth_core::m0_constants()
            .expect("the M0 registry constructs")
            .evidence()
            .tip_radius_max_grown_scar_mm()
            .value()
    }

    /// The registered `TIP_RADIUS_MAX_LEGACY_MM` — the load law's own clamp
    /// maximum, which is what the ladder would run to WITHOUT the scar bound.
    fn registered_legacy_tip_max_mm() -> f64 {
        plinth_core::m0_constants()
            .expect("the M0 registry constructs")
            .placeholders()
            .tip_radius_max_legacy_mm()
            .diagnostic()
            .get()
    }

    fn edge_seat_request(seat_y: f64) -> TipGrowthRequest {
        TipGrowthRequest {
            row: 0,
            p0_mm: [4.0 - EDGE_SEAT_DEPTH_MM, seat_y, 1.0],
            p1_mm: [4.0 + 2.5, seat_y, 1.0],
            end: ContactEnd::Terminalward,
            exterior_probe: [4.0 + EDGE_SEAT_EXTENT_MM, seat_y, 1.0],
            carried_mm2: EDGE_SEAT_CARRIED_MM2,
            law_radius_mm: EDGE_SEAT_LAW_RADIUS_MM,
            opposite_end_minimum_radius_mm: 0.35,
            max_radius_mm: registered_legacy_tip_max_mm(),
            scar_bound_radius_mm: registered_scar_bound_mm(),
            tip_clamped_at_max: false,
        }
    }

    fn grow_edge_seat(
        surface: &NativeSurface,
        request: &TipGrowthRequest,
    ) -> Vec<TipGrowthOutcome> {
        grow_tips_to_attest(
            surface,
            std::slice::from_ref(request),
            extent(EDGE_SEAT_EXTENT_MM),
            extent(EDGE_SEAT_EXTENT_MM / 10.0),
            ENDPOINT_VALIDATION_OFFSET_RAYS,
            &attestation_policy(),
        )
        .expect("the growth batch itself is well formed")
    }

    /// **A load-sized tip that cannot attest is grown until it does, and the
    /// tip that is BUILT is the tip that attested.**
    ///
    /// This is the defect in one fixture. PR-8 probes attachment with a ray ring
    /// at the TIP RADIUS, so shrinking a tip to what the load law alone needs
    /// withdraws the very ring the attestation depends on. The seat here is
    /// sound — part of its disc really is over solid — and at 0.05 mm radius
    /// (0.1 mm diameter) it is nevertheless refused `NotAttached`, quarantined
    /// upstream, and its 0.05 mm² abandoned.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn an_edge_seat_refused_at_the_law_radius_attests_at_the_smallest_rung_that_reaches() {
        let surface = edge_seat_box();
        let seat_y = 2.0 + EDGE_OFFSET_MM;
        let scan_ratio = tip_refinement_radius_ratio();

        // NON-VACUITY, stated first, exactly as the OD-9 fixture does it: this
        // is only an edge-seat test if the centreline really is in air, the
        // seat really is exterior (so OD-9 cannot be what refuses it), and the
        // rim at the cure radius really does reach solid.
        assert_eq!(
            surface.inside(&[
                // the centreline's own start point, in air past the edge
                [4.0 - EDGE_SEAT_DEPTH_MM, seat_y, 1.0],
                // the OD-9 probe, which must read EXTERIOR
                [4.0 + EDGE_SEAT_EXTENT_MM, seat_y, 1.0],
                // a rim ray at the law radius, still in air
                [
                    4.0 - EDGE_SEAT_DEPTH_MM,
                    seat_y - EDGE_SEAT_LAW_RADIUS_MM,
                    1.0
                ],
                // a rim ray at the cure radius, inside the solid
                [
                    4.0 - EDGE_SEAT_DEPTH_MM,
                    seat_y - EDGE_SEAT_CURE_RADIUS_MM,
                    1.0
                ],
                // and a rim ray at the scan step BELOW the cure, back in air —
                // the cure really is the smallest step that reaches, not merely
                // one that does.
                [
                    4.0 - EDGE_SEAT_DEPTH_MM,
                    seat_y - EDGE_SEAT_CURE_RADIUS_MM * scan_ratio,
                    1.0
                ],
            ]),
            vec![false, false, false, true, false],
            "the fixture must be an EDGE seat: centreline in air, seat exterior, \
             the law-radius ring missing, the cure ring reaching solid and the \
             next scan step down missing again"
        );

        let outcomes = grow_edge_seat(&surface, &edge_seat_request(seat_y));
        assert_eq!(outcomes.len(), 1);
        let outcome = &outcomes[0];
        let cure = outcome.cure.as_ref().unwrap_or_else(|| {
            panic!(
                "the edge seat must be curable by growth; without it this contact \
                 is refused and its {EDGE_SEAT_CARRIED_MM2} mm² of routed load is \
                 ABANDONED, with no neighbour told and nothing grown in its place. \
                 Rungs tried: {:?}",
                outcome.attempts
            )
        });

        // THE HONEST MINIMUM. Not the first LADDER rung that attests — that is
        // 0.1 mm radius, a whole area doubling of overshoot — but the smallest
        // radius EITHER phase measured as attesting, after the descending scan
        // walked back down inside the bracket the ladder closed.
        let expected_cure_mm = 0.1 * scan_ratio * scan_ratio;
        assert!(
            (expected_cure_mm - EDGE_SEAT_CURE_RADIUS_MM).abs() < 1e-15,
            "the scan step is 2 pi r / 8^2 per the ring's own sampling geometry, \
             so the cure is 0.1 * q^2 = {EDGE_SEAT_CURE_RADIUS_MM} mm radius; the \
             ratio arithmetic now says {expected_cure_mm}"
        );
        assert!(
            (cure.radius_mm - expected_cure_mm).abs() < 1e-12,
            "the ladder brackets the answer at 0.1 mm radius and the scan must \
             then descend to {expected_cure_mm} mm radius \
             ({} mm diameter), the smallest step that still clears the \
             {EDGE_OFFSET_MM} mm the seat stands past the edge; got {} mm radius",
            expected_cure_mm * 2.0,
            cure.radius_mm
        );
        assert!(
            cure.radius_mm > EDGE_OFFSET_MM,
            "a cure at or below {EDGE_OFFSET_MM} mm radius cannot reach the solid \
             at all, so the scan would have kept a tip that does not attest"
        );
        assert_eq!(outcome.law_radius_mm, EDGE_SEAT_LAW_RADIUS_MM);
        // TWO ladder rungs to bracket, then THREE scan steps inside the
        // bracket — the bound this fixture exercises in full.
        assert_eq!(
            outcome.attempts.len(),
            5,
            "two ladder rungs above the law radius reach 0.1 mm, and exactly \
             three scan steps fit in the bracket (0.0707, 0.1): {:?}",
            outcome.attempts
        );
        assert!(
            !outcome.attempts[0].attested,
            "0.0707 mm radius cannot reach the solid and must be recorded as a \
             failed rung, not skipped"
        );
        assert!(outcome.attempts[1].attested);
        // The scan measures EVERY candidate, including the one below the cure.
        // A run that stopped at the first failure on the way down would be
        // assuming the monotonicity the ladder exists to avoid assuming.
        assert!(
            outcome.attempts[2].attested && outcome.attempts[3].attested,
            "the two scan steps above {EDGE_OFFSET_MM} mm must attest: {:?}",
            outcome.attempts
        );
        assert!(
            !outcome.attempts[4].attested,
            "the third scan step falls below {EDGE_OFFSET_MM} mm and must be \
             MEASURED and recorded as failing, not skipped: {:?}",
            outcome.attempts
        );
        assert!(
            outcome.attempts[2].radius_mm > outcome.attempts[3].radius_mm
                && outcome.attempts[3].radius_mm > outcome.attempts[4].radius_mm
                && outcome.attempts[2].radius_mm < outcome.attempts[1].radius_mm,
            "the scan record must be strictly DESCENDING and start below the \
             ladder rung it refines: {:?}",
            outcome.attempts
        );
        assert!(
            outcome.attempts[2..]
                .iter()
                .all(|rung| rung.radius_mm > outcome.attempts[0].radius_mm),
            "no scan step may drop to or below the last FAILING ladder rung at \
             {} mm radius — that radius is already answered, and anything under \
             it is below what the load law asked for: {:?}",
            outcome.attempts[0].radius_mm,
            outcome.attempts
        );
        assert_eq!(
            outcome.attempts[3].radius_mm, cure.radius_mm,
            "the tip that is BUILT must be the tip that ATTESTED — probing at one \
             radius and emitting another is the defect, not the fix"
        );
        // The attestation the cure carries is a real one at that radius: the
        // ring found solid, and it found it with the CENTRELINE still in air,
        // which is the corpus case exactly.
        assert!(cure.attestation.verdict.is_ok());
        assert!(
            !cure.attestation.engagement.centreline,
            "this seat is held on by its RIM; a centreline that engaged would \
             mean the fixture stopped being an edge seat"
        );
        assert!(cure.attestation.engagement.rim_engaged > 0);
        // The shaft below a grown tip is never thinner than the tip, which is
        // the same non-inverting rule the mint site already applies.
        assert!(cure.opposite_end_radius_mm >= cure.radius_mm);

        // THE COUNTERFACTUAL: the same fixture with the growth rounds off is
        // the engine before this change. No cure, no rungs, load abandoned.
        let refused =
            with_growth_rounds_disabled(|| grow_edge_seat(&surface, &edge_seat_request(seat_y)));
        assert!(
            refused[0].cure.is_none() && refused[0].attempts.is_empty(),
            "with the growth rounds off this seat must stay refused, or the test \
             above proves nothing about growth"
        );
    }

    /// **The refinement scan's step and its BOUND, asserted as arithmetic.**
    ///
    /// The step is the ring's own geometry: `N` rim rays sit `2 pi r / N` apart,
    /// and one refinement step moves the rim by one `N`th of that gap, so the
    /// descent ratio is `1 - 2 pi / N^2`. Because the step is proportional to
    /// `r` the descent is geometric, so the number of steps that fit in a
    /// bracket depends only on the bracket's RATIO — and the ladder makes that
    /// ratio at most `sqrt(2)`. Hence at most three extra rounds, for every
    /// contact, at every depth. Nothing here is hand-typed: change the ray count
    /// and both the step and the bound follow it.
    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn the_refinement_scan_steps_by_the_rings_own_sample_spacing_and_is_bounded_at_three() {
        let rays = ENDPOINT_VALIDATION_OFFSET_RAYS as f64;
        let ratio = tip_refinement_radius_ratio();
        let expected_ratio = 1.0 - (std::f64::consts::TAU / rays) / rays;
        assert!(
            (ratio - expected_ratio).abs() < 1e-15,
            "one step must move the rim by one {rays}th of the {}-mm-per-radius \
             inter-ray gap; got ratio {ratio}",
            std::f64::consts::TAU / rays
        );
        assert!(
            ratio > 0.0 && ratio < 1.0,
            "the scan must descend, and to a positive radius: ratio {ratio}"
        );

        // THE BOUND, from the two ratios and nothing else.
        let bound = (AREA_DOUBLING_RADIUS_RATIO.ln() / (1.0 / ratio).ln()).ceil() as usize - 1;
        let steps_per_doubling = AREA_DOUBLING_RADIUS_RATIO.ln() / (1.0 / ratio).ln();
        assert_eq!(
            bound, 3,
            "ln(sqrt2)/ln(1/q) = {steps_per_doubling} steps fit in one area \
             doubling, so the scan adds at most {bound} rounds"
        );

        // A bracket exactly one ladder step wide — the widest the ladder can
        // hand over — yields exactly that many candidates, strictly descending
        // and strictly inside the bracket.
        for lower_mm in [0.05, 0.0707, 0.19, 0.5] {
            let upper_mm = lower_mm * AREA_DOUBLING_RADIUS_RATIO;
            let scan = tip_refinement_scan_for_test(upper_mm, lower_mm);
            assert_eq!(
                scan.len(),
                bound,
                "a full-width bracket ({lower_mm}, {upper_mm}) mm radius must \
                 hold exactly {bound} steps; got {scan:?}"
            );
            let mut previous = upper_mm;
            for radius in &scan {
                assert!(
                    *radius < previous && *radius > lower_mm,
                    "step {radius} must be strictly inside ({lower_mm}, \
                     {previous}) mm radius"
                );
                assert!(
                    (radius - previous * ratio).abs() < 1e-15,
                    "every step is one ratio, never a bisection: {scan:?}"
                );
                previous = *radius;
            }
        }

        // A NARROWER bracket — what the ladder hands over when its last rung was
        // clamped to the registered maximum — holds fewer, and never proposes a
        // radius at or below the rung that already failed.
        let narrow = tip_refinement_scan_for_test(1.0, 0.85);
        assert!(
            !narrow.is_empty()
                && narrow.len() < bound
                && narrow.iter().all(|radius| *radius > 0.85),
            "a clamped final rung leaves a narrow bracket, which must still be \
             scanned and still respect its lower end: {narrow:?}"
        );
        // And a bracket with nothing in it is empty rather than degenerate.
        assert!(tip_refinement_scan_for_test(0.1, 0.1).is_empty());
        assert!(tip_refinement_scan_for_test(0.1, 0.2).is_empty());
        assert!(tip_refinement_scan_for_test(0.1, 0.0).is_empty());
    }

    /// **Growth does not paper over a seat that is genuinely not there.**
    ///
    /// Move the same stub far enough past the edge that no legal tip can reach
    /// — `2.0 + 1.5` mm, beyond even the registered `tip_radius_max_legacy_mm`
    /// of 1.0 mm radius, and so beyond any ceiling growth could ever be given —
    /// and the ladder must be walked to its end and then REFUSE, with every
    /// rung on the record. A remedy that cured this one would be inventing
    /// attachment.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn a_seat_no_legal_tip_can_reach_is_refused_with_the_whole_ladder_on_the_record() {
        let surface = edge_seat_box();
        let seat_y = 2.0 + 1.5;
        let outcomes = grow_edge_seat(&surface, &edge_seat_request(seat_y));
        assert!(
            outcomes[0].cure.is_none(),
            "1.5 mm past the edge is further than the registered maximum 1.0 mm \
             radius can reach; curing it would be inventing attachment"
        );
        // Every rung tried, none attested, and the last one is exactly the
        // ceiling the run walked to — the ladder ends on a radius the run is
        // allowed to build. Read from the outcome rather than typed, so this
        // stays true whichever of the two registered limits binds.
        assert!(!outcomes[0].attempts.is_empty());
        assert!(outcomes[0].attempts.iter().all(|rung| !rung.attested));
        assert_eq!(
            outcomes[0].attempts.last().unwrap().radius_mm,
            outcomes[0].growth_ceiling_radius_mm
        );
    }

    // ---------------------------------------------------------------------
    // THE SCAR BOUND — TIP_RADIUS_MAX_GROWN_SCAR_MM
    // ---------------------------------------------------------------------

    /// **THE OVER-BOUND SIBLING of the edge seat.**
    ///
    /// The same construction as [`EDGE_OFFSET_MM`], moved further out: the seat
    /// stands `1.25 x` the registered scar maximum past the `+y` silhouette
    /// edge. The reach rule is unchanged — a rim ray at tip radius `r` reaches
    /// `y = 2 + offset - r`, so the ring finds solid **iff `r > offset`** — and
    /// with `offset = 1.25 x bound` that means:
    ///
    /// * **every radius the bound allows fails**, because the whole ladder is
    ///   at or below `bound < offset`;
    /// * **a radius above the bound would work**, because the ladder run to the
    ///   legacy maximum contains rungs above `offset`.
    ///
    /// So the seat is curable in geometry and refused by policy, which is
    /// exactly the case the bound exists to decide. The factor is stated as
    /// arithmetic against the registered constant, never as a typed millimetre,
    /// so a ruling that moves the bound moves this fixture with it.
    fn over_bound_edge_offset_mm() -> f64 {
        1.25 * registered_scar_bound_mm()
    }

    /// **A cure that would exceed Craig's scar maximum is REFUSED, and the
    /// refusal names the bound.**
    ///
    /// This is the ruling of 2026-08-20 in one fixture. The seat CAN be reached
    /// — the counterfactual below proves it, by lifting the bound and watching
    /// the same fixture cure — but only by growing the tip past 0.400 mm RADIUS
    /// (0.800 mm DIAMETER), which is the heaviest support Craig has ever needed
    /// and the largest scar he accepts. The engine's answer is therefore the
    /// disclosed ABANDONED LOAD, with the bound NAMED as the stopper, rather
    /// than a support the operator never agreed to.
    ///
    /// **If the truncation is reverted this test goes red**, and the failure
    /// prints the over-bound scar the run would have built — radius and
    /// diameter — instead of a bare "expected None".
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn a_cure_that_would_exceed_the_grown_scar_maximum_is_refused_and_names_the_bound() {
        let surface = edge_seat_box();
        let bound_mm = registered_scar_bound_mm();
        let offset_mm = over_bound_edge_offset_mm();
        let seat_y = 2.0 + offset_mm;
        let outcomes = grow_edge_seat(&surface, &edge_seat_request(seat_y));
        assert_eq!(outcomes.len(), 1);
        let outcome = &outcomes[0];

        if let Some(cure) = outcome.cure.as_ref() {
            panic!(
                "THE SCAR BOUND IS NOT BEING APPLIED. This seat stands \
                 {offset_mm} mm past the edge, so nothing at or below the \
                 registered TIP_RADIUS_MAX_GROWN_SCAR_MM of {bound_mm} mm RADIUS \
                 ({} mm DIAMETER) can reach it — yet growth returned a cure at \
                 {} mm RADIUS ({} mm DIAMETER), an over-bound scar {}x the \
                 maximum Craig ruled. That is a support wider than the heaviest \
                 he has ever needed, cut into a model in a place he did not \
                 choose. Rungs walked: {:?}",
                bound_mm * 2.0,
                cure.radius_mm,
                cure.radius_mm * 2.0,
                cure.radius_mm / bound_mm,
                outcome.attempts
            );
        }

        // THE CEILING IS THE BOUND, and it is named.
        assert_eq!(
            outcome.growth_ceiling_radius_mm,
            bound_mm,
            "the ladder's ceiling must be the registered scar maximum \
             {bound_mm} mm radius ({} mm diameter), not the load law's own \
             clamp",
            bound_mm * 2.0
        );
        assert_eq!(
            outcome.growth_ceiling_source,
            GrowthCeilingSource::GrownScarMaximum
        );
        assert_eq!(
            outcome.growth_ceiling_source.constant_id(),
            "TIP_RADIUS_MAX_GROWN_SCAR_MM",
            "the disclosure names the constant that stopped the rescue; a bare \
             number cannot tell an operator WHICH rule refused their support"
        );

        // THE LADDER WAS WALKED, in full, and it stopped exactly at the bound.
        // A refusal with an unwalked ladder would be the silent quarantine
        // grow-to-attest exists to remove.
        assert!(
            !outcome.attempts.is_empty(),
            "the contact was refused without any larger tip being tried"
        );
        assert!(outcome.attempts.iter().all(|rung| !rung.attested));
        assert_eq!(outcome.attempts.last().unwrap().radius_mm, bound_mm);
        assert!(
            outcome
                .attempts
                .iter()
                .all(|rung| rung.radius_mm <= bound_mm),
            "no rung may PROPOSE a tip above the bound, even to measure it — \
             every rung probes at the radius it would emit: {:?}",
            outcome.attempts
        );
        assert_eq!(outcome.law_radius_mm, EDGE_SEAT_LAW_RADIUS_MM);
    }

    /// **The counterfactual: without the bound, this same seat CURES — at an
    /// over-bound scar.**
    ///
    /// Without this, the refusal above proves nothing about the bound. A seat
    /// that no tip could ever reach would refuse identically, and the guard
    /// would stay green with the whole truncation deleted and the fixture
    /// merely unreachable. Running the SAME fixture through the SAME production
    /// path with the bound lifted separates the two: the cure that appears is
    /// the scar the bound is refusing, and its size is what makes the refusal
    /// worth having.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn without_the_scar_bound_the_same_seat_cures_at_an_over_bound_scar() {
        let surface = edge_seat_box();
        let bound_mm = registered_scar_bound_mm();
        let legacy_max_mm = registered_legacy_tip_max_mm();
        let offset_mm = over_bound_edge_offset_mm();
        let seat_y = 2.0 + offset_mm;

        let outcomes =
            with_scar_bound_disabled(|| grow_edge_seat(&surface, &edge_seat_request(seat_y)));
        let outcome = &outcomes[0];
        assert_eq!(
            outcome.growth_ceiling_radius_mm, legacy_max_mm,
            "with the bound lifted the ceiling falls back to the load law's own \
             clamp maximum"
        );
        assert_eq!(
            outcome.growth_ceiling_source,
            GrowthCeilingSource::LegacyTipMaximum
        );

        let cure = outcome.cure.as_ref().unwrap_or_else(|| {
            panic!(
                "this fixture must be CURABLE once the bound is lifted, or the \
                 refusal it drives says nothing about the bound: a seat nothing \
                 could ever reach would refuse the same way. Rungs: {:?}",
                outcome.attempts
            )
        });
        assert!(
            cure.radius_mm > bound_mm,
            "the cure the bound refuses must genuinely exceed it: got \
             {} mm radius ({} mm diameter) against a bound of {bound_mm} mm \
             radius ({} mm diameter)",
            cure.radius_mm,
            cure.radius_mm * 2.0,
            bound_mm * 2.0
        );
        assert!(
            cure.radius_mm > offset_mm,
            "a tip at or below the {offset_mm} mm the seat stands past the edge \
             cannot reach the solid at all"
        );
        assert!(cure.radius_mm <= legacy_max_mm);
    }

    /// **The truncation itself, as arithmetic against the two registered
    /// limits.**
    ///
    /// The rule is `min`, and it is stated here as the three cases rather than
    /// exercised only through one fixture's geometry, so a ceiling that starts
    /// picking the wrong limit is visible even if no fixture happens to
    /// straddle it. The NAME travels with the number in every case: a
    /// disclosure that quoted the right millimetres under the wrong constant
    /// would still be telling the operator the wrong thing about which rule
    /// refused their support.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn the_growth_ceiling_is_the_smaller_registered_limit_and_carries_its_name() {
        let bound_mm = registered_scar_bound_mm();
        let legacy_max_mm = registered_legacy_tip_max_mm();
        assert!(
            bound_mm < legacy_max_mm,
            "the registered scar maximum {bound_mm} mm radius must sit below \
             the load law's clamp maximum {legacy_max_mm} mm radius, or there \
             is nothing for the truncation to do"
        );

        // The live pairing: the scar bound is the smaller, so it binds.
        assert_eq!(
            growth_ceiling_radius_mm(legacy_max_mm, bound_mm),
            (bound_mm, GrowthCeilingSource::GrownScarMaximum)
        );
        // A bound at or above the legacy maximum does not bind, and the
        // disclosure must then name the limit that actually did.
        assert_eq!(
            growth_ceiling_radius_mm(legacy_max_mm, legacy_max_mm),
            (legacy_max_mm, GrowthCeilingSource::LegacyTipMaximum)
        );
        assert_eq!(
            growth_ceiling_radius_mm(legacy_max_mm, legacy_max_mm * 2.0),
            (legacy_max_mm, GrowthCeilingSource::LegacyTipMaximum)
        );

        // And the ladder truncates there: under the bound it is SHORTER, and
        // every rung it proposes is one the run may actually build.
        let smallest_tip_mm = plinth_core::m0_constants()
            .unwrap()
            .placeholders()
            .tip_radius_default_mm()
            .diagnostic()
            .get();
        let bounded = tip_growth_ladder_for_test(smallest_tip_mm, bound_mm);
        let unbounded = tip_growth_ladder_for_test(smallest_tip_mm, legacy_max_mm);
        assert!(
            bounded.len() < unbounded.len(),
            "the bound must SHORTEN the ladder: {} rungs bounded against {} \
             unbounded",
            bounded.len(),
            unbounded.len()
        );
        assert_eq!(*bounded.last().unwrap(), bound_mm);
        assert!(bounded.iter().all(|radius| *radius <= bound_mm));
        assert!(unbounded.iter().any(|radius| *radius > bound_mm));
    }

    /// **`docs/specs/geometry-adjudication.md` §EXT-1 site 15 — an undecidable
    /// witness row REFUSES the embed, by name.**
    ///
    /// The plant is the §O4 counterexample triangle `(0,0,0) (2,0,0) (0,2,0)`,
    /// for which `e1 = (2,0,0)` and `e2 = (0,2,0)` give `det = -4 * dv_z`
    /// exactly; a row whose axis runs almost in that plane puts `det` at
    /// `-8e-15`, inside the `DET_EPS = 1e-14` band and nonzero — undecidable,
    /// not a genuine prune.
    ///
    /// The input class that changes: a row whose interval evidence is EMPTY
    /// because the crossings that would have filled it could not be decided.
    /// Before this ruling that row was indistinguishable from a row with
    /// nothing to find, and the difference decides whether an embed is
    /// attested.
    #[test]
    fn an_undecidable_witness_row_refuses_the_embed_and_names_the_uncertainty() {
        let surface = geom_kernel::testkit::build_from_tris(vec![
            0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0, 0.0, //
        ]);
        let row = EndpointValidationRow {
            member: 0,
            end: ContactEnd::Terminalward,
            extent: extent(0.01),
            fraction: 0.01,
            p0: [0.4, 0.5, -1.0e-16],
            p1: [1.4, 0.5, 1.9e-15],
            r0: 0.05,
            r1: 0.05,
        };
        let limits = validation_limits(&surface, 1);

        // NON-VACUITY, straight from the kernel: the plant reaches the
        // undecidable band, and the interval evidence it leaves behind is
        // EMPTY — so on the arithmetic alone this row looks like a clean
        // "nothing here" rather than "we could not tell".
        let probe = surface
            .penetration_intervals_batch_bounded_classified(
                &[row.p0],
                &[row.p1],
                &[row.r0],
                &[row.r1],
                ENDPOINT_VALIDATION_OFFSET_RAYS,
                limits.majority_step_mm.get(),
                limits.samples_per_subsegment,
                0.0,
                true,
                limits.penetration,
            )
            .expect("the planted batch runs");
        assert!(
            probe.near_parallel[0] > 0,
            "the plant must fire; tally was {}",
            probe.near_parallel[0]
        );
        assert!(
            probe.rows[0].is_empty(),
            "and it must leave no interval, so only the tally can refuse"
        );

        let engagements = witness_batch(&surface, std::slice::from_ref(&row), limits)
            .expect("the batch itself succeeds; the REFUSAL is per row");
        assert_eq!(engagements.len(), 1);
        match engagements[0] {
            Err(DesignedMaskError::NearParallelWitness { near_parallel }) => {
                assert_eq!(
                    near_parallel, probe.near_parallel[0],
                    "the refusal carries the tally it refused on"
                );
            }
            ref other => panic!(
                "an undecidable row must refuse by name, and never as \
                 CorruptKernelResult or as a measured engagement: {other:?}"
            ),
        }

        // The refusal is DISTINGUISHABLE in the recorded transcript, and its
        // tag is appended rather than colliding with an existing one.
        assert_eq!(
            DesignedMaskError::NearParallelWitness { near_parallel: 1 }.tag(),
            16
        );
    }
}
