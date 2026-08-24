//! Frozen pre-P8 bracing policy.
//!
//! The implementation is deliberately an immutable planner: it consumes one
//! accepted transient snapshot, gathers every cross-link and model-rung
//! candidate, evaluates all swept segments in one native batch, and only then
//! scatters the deterministic winners into a cloned forest.  The caller's
//! forest is never changed.

use std::collections::{BTreeMap, BTreeSet};

use geom_kernel::{NativeSurface, PenetrationBatchError, PenetrationBatchLimits};
use plinth_core::{m0_constants, Finite, PositiveFinite};
use plinth_mesh::FaceTopology;
use serde::Serialize;
use thiserror::Error;

use crate::{
    anchor::{FOOT_BREACH_CONE_RAYS, FOOT_BREACH_HALF_DEG},
    cco::FinalizedDispositionKind,
    contact::{self, ContactPolicy},
    designed_embed,
    forest::{
        ContactEnd, Forest, ForestError, Member, MemberKey, MemberKind, MemberSpec, Owner,
        RootwardEndRadius, TerminalEndRadius,
    },
    legacy_compat::{LegacyCompatibility, LegacyCompatibilityError},
    member_sizing::{
        diagnostic_within_printability_cone, DiagnosticMemberSizingPolicy, LeanAngleDeg,
        MemberSizingError, MemberSpan,
    },
    perception::FrozenTipLoadPolicy,
    transient::{
        resize_forest_current_parity, transient_report, MemberTransientEvidence, TransientError,
        TransientReport, TransientResizePlan, ZInterval,
    },
};

const HEIGHT_TOLERANCE_MM: f64 = 1e-9;
const CROSS_LINK_DUTY_FACTOR: f64 = 0.840_896_415_253_714_5; // 0.5^(1/4)

#[derive(Clone, Debug)]
struct BraceConfig {
    ladder_z_mm: f64,
    neighbor_max_xy_mm: f64,
    reach_tier_base_factor: f64,
    reach_max_factor: f64,
    brace_radius_mm: PositiveFinite,
    fallback_fractions: Vec<f64>,
    max_stacked: usize,
    min_cross_link_xy_mm: f64,
    model_rung_max_candidates: usize,
    model_rung_min_length_mm: f64,
    model_rung_z_tolerance_mm: f64,
    model_brace_max_length_mm: f64,
    twin_single_xy_mm: f64,
    max_unbraced_radius_mm: f64,
    contact_depth_mm: f64,
    /// `STANDOFF_MM` — the PR-2 mask's standoff term, and the PLACEMENT-stage
    /// demanded gap.
    standoff_mm: f64,
    /// `FINAL_CLEARANCE_GAP_MM` — the FINAL-stage demanded gap, and the only
    /// refusing clearance number in the engine (Craig's ruling, 2026-08-22).
    final_clearance_gap_mm: f64,
    ball_factor_foot: f64,
    ball_breach_epsilon_mm: f64,
    min_illegal_len_mm: f64,
    /// `max(LayerPlan::heights_mm())` for the run being braced — the PR-1
    /// slicer-resolution seat floor and the PR-2 mask margin.  Carried on the
    /// config because it is a property of the RUN, not of the registry.
    process_resolution_mm: f64,
    ball_factor_stub: f64,
    tip_load: FrozenTipLoadPolicy,
}

impl BraceConfig {
    /// The PR-1..PR-4 policy this brace pass evaluates contacts under, at the
    /// demanded gap of ONE named stage.
    ///
    /// The brace pass is the only stage that reads BOTH demands, which is why
    /// the stage is an argument here and a constant everywhere else:
    ///
    /// * `evaluate_candidates` grades a per-`(need, site)` tournament and
    ///   refuses no model, so it is PLACEMENT (`STANDOFF_MM`, 0.600 mm). Ruling
    ///   R-1495 of 2026-08-22: prune-harder-than-accept is the intended
    ///   asymmetry, and `Unplaceable` is an accepted termination.
    /// * `exact_clearance_rows` is the verdict that condemns an arm, so it is
    ///   FINAL (`FINAL_CLEARANCE_GAP_MM`, 0.300 mm).
    ///
    /// Sites that attest or admit a contact rather than grade its distance
    /// (`attest_seats_batch`, `contact::admit`) read no demanded gap at all;
    /// they take the FINAL policy because the contact they are admitting will
    /// be graded by the final pass.
    fn contact_policy(&self, stage: contact::ClearanceStage) -> ContactPolicy {
        ContactPolicy {
            floor_depth_mm: self.contact_depth_mm,
            process_resolution_mm: self.process_resolution_mm,
            standoff_mm: self.standoff_mm,
            demanded_gap_mm: match stage {
                contact::ClearanceStage::Placement => self.standoff_mm,
                contact::ClearanceStage::Final => self.final_clearance_gap_mm,
            },
            min_free_span_mm: self.min_illegal_len_mm,
            ball_factor_stub: self.ball_factor_stub,
            tip_load: self.tip_load,
        }
    }

    fn current(process_resolution_mm: f64) -> Result<Self, BraceError> {
        let constants = m0_constants().map_err(|error| BraceError::Constants(error.to_string()))?;
        let p = constants.placeholders();
        Ok(Self {
            process_resolution_mm,
            ball_factor_stub: p.ball_factor_stub().diagnostic().get(),
            tip_load: FrozenTipLoadPolicy::from_placeholders(p),
            ladder_z_mm: p.brace_ladder_z_mm().diagnostic().get(),
            neighbor_max_xy_mm: p.brace_neighbor_max_xy_mm().diagnostic().get(),
            reach_tier_base_factor: p.bracing_reach_tier_base_factor().diagnostic().get(),
            reach_max_factor: p.brace_reach_max_factor_x().diagnostic().get(),
            brace_radius_mm: *p.brace_r_mm().diagnostic(),
            fallback_fractions: p
                .bracing_fallback_fractions()
                .diagnostic()
                .iter()
                .map(|fraction| fraction.get())
                .collect(),
            max_stacked: usize::try_from(p.bracing_max_stacked_fallbacks().diagnostic().get())
                .map_err(|_| BraceError::CountNotRepresentable)?,
            min_cross_link_xy_mm: p.bracing_min_cross_link_xy_mm().diagnostic().get(),
            model_rung_max_candidates: usize::try_from(
                p.bracing_model_rung_max_candidates().diagnostic().get(),
            )
            .map_err(|_| BraceError::CountNotRepresentable)?,
            model_rung_min_length_mm: p.bracing_model_rung_min_length_mm().diagnostic().get(),
            model_rung_z_tolerance_mm: p.bracing_model_rung_z_tol_mm().diagnostic().get(),
            model_brace_max_length_mm: p.model_brace_max_len_mm().diagnostic().get(),
            twin_single_xy_mm: p.twin_single_brace_xy_mm().diagnostic().get(),
            max_unbraced_radius_mm: p.max_unbraced_shaft_r_mm().diagnostic().get(),
            contact_depth_mm: p.default_contact_depth_mm().diagnostic().get(),
            standoff_mm: p.standoff_mm().diagnostic().get(),
            final_clearance_gap_mm: *constants.evidence().final_clearance_gap_mm().value(),
            ball_factor_foot: p.ball_factor_foot().diagnostic().get(),
            ball_breach_epsilon_mm: p.ball_breach_eps_mm().diagnostic().get(),
            min_illegal_len_mm: p.min_illegal_len_mm().diagnostic().get(),
        })
    }
}

/// Why a bounded bracing controller stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum BraceFixedPointTermination {
    /// No brace was added, no radius changed, and exact clearance is clean.
    Converged,
    /// The frozen three-pass compatibility bound was consumed.
    IterationLimit,
    /// Requests remained but every bounded candidate was geometrically invalid.
    Unplaceable,
    /// **PR-9.** The third pass ended with negative exact clearance, and every
    /// offending member DECLARES A MODEL CONTACT.
    ///
    /// That is a refused contact, not a broken pipeline, and the distinction is
    /// OD-4's: a seated contact whose geometry cannot clear the model is an
    /// ordinary candidate rejection and must route to quarantine, whereas a
    /// member carrying NO model contact reading negative clearance means the
    /// bracing controller put geometry inside the mini with nothing to reject —
    /// that remains `BraceError::ClearanceViolation`.
    ///
    /// The refused members are quarantined in the returned forest, so this
    /// termination never means "shipped anyway".
    ContactRefused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) enum BraceAdditionKind {
    CrossLink,
    ModelRung,
}

/// Immutable description of one member added by the bracing pass.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct BraceAddition {
    pub(crate) key: MemberKey,
    pub(crate) kind: BraceAdditionKind,
    pub(crate) member: MemberSpec,
}

/// One immutable propose/resize result.
#[derive(Clone, Debug)]
pub(crate) struct BracePassPlan {
    pub(crate) forest_with_braces: Forest,
    pub(crate) additions: Vec<BraceAddition>,
    pub(crate) resize: TransientResizePlan,
    pub(crate) rungs_requested: usize,
    pub(crate) rungs_placed: usize,
    pub(crate) rungs_unplaceable: usize,
    /// PR-8/OD-9 (F3) — this pass's model-rung seat-attestation transcript.
    pub(crate) contact_attestation_digest: u64,
}

/// Bounded fixed-point result used by the pending support runner.
#[derive(Clone, Debug)]
pub(crate) struct BraceFixedPointPlan {
    pub(crate) forest: Forest,
    pub(crate) additions: Vec<BraceAddition>,
    pub(crate) iterations: usize,
    pub(crate) termination: BraceFixedPointTermination,
    /// PR-9 — contact-declaring members quarantined at the third-pass cap for
    /// negative exact clearance. Ascending, so the evidence is order-stable.
    pub(crate) contact_refusals: Vec<MemberKey>,
    /// PR-8/OD-9 (F3) — the model-rung seat-attestation transcript, chained
    /// over every pass this controller ran.
    ///
    /// Carried separately from the forest digest for the same reason the stub
    /// family's transcript is: an attested rung and an unattested one are the
    /// same geometry, and a rung that was REFUSED leaves no trace in the forest
    /// at all, so deleting the screen would move nothing an existing gate can
    /// see.
    pub(crate) contact_attestation_digest: u64,
    /// **Refusal accounting** — the counts and the true gaps in millimetres
    /// the FINAL pass measured (Craig's rider, 2026-08-23).
    ///
    /// Deliberately NOT folded into `brace_fixed_point_plan_digest`, and the
    /// reason is not convenience. Every row here is a pure function of the
    /// forest and the surface that digest already covers — the refused keys
    /// are digested one field up, and the gaps are what
    /// `exact_clearance_rows` reads off that same geometry. Folding them would
    /// move every sealed brace-plan digest in the corpus to disclose a number
    /// no verdict reads, and this lane is authorized to move exactly one
    /// fingerprint, the CCO transcript's.
    pub(crate) final_pass_refusals: FinalPassRefusalAccounting,
}

/// The empty head of a seat-attestation chain.
///
/// A distinguished non-zero seed so "no pass ran" is distinguishable from "a
/// pass ran and attested nothing", which is exactly the confusion a zero
/// initial value would create.
const fn contact_attestation_chain_seed() -> u64 {
    0x8c00_0000_0000_0001
}

/// Fold one pass's transcript into the running chain.
///
/// The pass index is mixed in so that two passes producing the same transcript
/// in a different order are not the same chain.
fn extend_contact_attestation_chain(head: u64, pass: usize, stage: u64) -> u64 {
    let mut digest = crate::forest::DigestWriter::new();
    digest.tag(0x8d);
    digest.u64(head);
    digest.u64(pass as u64);
    digest.u64(stage);
    digest.finish()
}

#[derive(Clone, Debug, PartialEq, Error)]
pub(crate) enum BraceError {
    #[error("registered M0 constants are unavailable: {0}")]
    Constants(String),
    #[error("registered brace count is not representable on this platform")]
    CountNotRepresentable,
    #[error("transient report is stale or not canonical for this forest")]
    StaleTransientReport,
    #[error("brace candidate references a missing live member: {0:?}")]
    MissingMember(MemberKey),
    #[error("brace geometry is degenerate or non-finite")]
    InvalidGeometry,
    #[error("native brace query failed: {0}")]
    NativeQuery(String),
    #[error(transparent)]
    Penetration(#[from] PenetrationBatchError),
    #[error("bracing fixed point ended with {0} exact-clearance violation(s)")]
    ClearanceViolation(usize),
    #[error(transparent)]
    Forest(#[from] ForestError),
    #[error(transparent)]
    Transient(#[from] TransientError),
    #[error(transparent)]
    Compatibility(#[from] LegacyCompatibilityError),
    #[error(transparent)]
    Sizing(#[from] MemberSizingError),
    /// #36. A frozen seat zone could not be computed. Like every other
    /// batch-level failure here this is a statement about the BATCH — a starved
    /// band admission or a mesh/topology disagreement — never about any one
    /// rung, so it fails the pass rather than dropping a candidate.
    #[error("brace seat-zone freeze failed: {0}")]
    SeatZone(String),
    /// **Stage A — the pass is abandoned rather than minting the illegal pair.**
    ///
    /// The final pass measured a sealed-`Routed` terminal's OWN chosen arm
    /// inside the model. Quarantining it would flip that terminal's live state
    /// to `Quarantined` under a sealed record that still says `Routed` — a pair
    /// the accounting rightly refuses two stages later, where it reads as a
    /// broken engine rather than as the physics it is. Nothing is quarantined
    /// and the refusal carries the whole class.
    ///
    /// It is an `Err` rather than a named termination on purpose: the forest
    /// this pass holds still contains the illegal member, so no caller may be
    /// able to reach it by ignoring a field.
    #[error(
        "the bracing pass is abandoned: it measured a sealed-routed terminal's own chosen arm \
         inside the model, and quarantining it would mint the terminal pair the accounting \
         forbids — {0}"
    )]
    SealedRoutedTerminalCondemned(Box<SealedCondemnationClass>),
    /// **Stage A — the pass that learns nothing (design §3's T6).**
    ///
    /// The sweep would disturb sealed terminals, but no sealed-`Routed`
    /// terminal's OWN chosen arm is among the condemned members: every one of
    /// them is a cascade victim of somebody else's arm, or the disturbed
    /// terminals were sealed as something other than `Routed`. There is no
    /// choice to steer, so re-planning would reproduce this pass exactly.
    /// It refuses here, by name, with the physics.
    #[error(
        "the bracing pass has nothing to steer with: its condemned members disturb sealed \
         terminals, but no sealed-routed terminal's own chosen arm is among them — {0}"
    )]
    SealedSweepLearnedNothing(Box<SealedCondemnationClass>),
}

#[derive(Clone, Debug)]
struct Need {
    host: MemberKey,
    z_mm: f64,
    rung_index: usize,
    path_mapped: bool,
    triggers: Vec<MemberKey>,
    alternatives: Vec<(MemberKey, f64)>,
}

#[derive(Clone, Debug)]
struct NeedBatch {
    requested: usize,
    needs: Vec<Need>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TriggerReliefResult {
    pending: Vec<MemberKey>,
    resize_batches: usize,
    exact_clearance_batches: usize,
    /// How many batched seat-zone re-size calls the trial spent (Craig's ruling,
    /// 2026-08-23). Counted for the same reason the two above are: the re-size
    /// is batch-first by design, and a regression to one call per seated member
    /// has to be VISIBLE rather than merely slow.
    seat_zone_refreeze_batches: usize,
}

/// The seated contact geometry carried alongside a rod that touches the model.
///
/// CENSUS-ONLY, and gated on the `census` feature accordingly (MAJOR-8): the
/// production clearance path reads `inset1` directly and never needs this.
///
/// Carried EXPLICITLY rather than reconstructed.  Two census consumers used to
/// invert the retired `embed_inset_mm` to recover the incidence
/// (`(r + standoff) / (inset1 - contact_depth)`); under the PR-2 mask that
/// inverse is meaningless, and since the census is the OD-8 delta instrument,
/// a meaningless inverse would invalidate the measurement itself.
#[cfg(feature = "census")]
#[derive(Clone, Copy, Debug)]
struct RodContact {
    /// `|cos|` between the member axis and the surface normal, AFTER seating.
    abs_cos_incidence: f64,
}

#[derive(Clone, Debug)]
struct Rod {
    p0: [f64; 3],
    p1: [f64; 3],
    radius: PositiveFinite,
    inset0: f64,
    inset1: f64,
    /// `Some` exactly when this rod's `p1` end is a seated model contact.
    /// Census-only; see [`RodContact`].
    #[cfg(feature = "census")]
    contact: Option<RodContact>,
}

#[derive(Clone, Debug)]
struct PlacementCandidate {
    need_index: usize,
    site_index: usize,
    rank: usize,
    kind: BraceAdditionKind,
    rods: Vec<Rod>,
    printable: bool,
    /// **#36 — the frozen seat zone `Z`, for a candidate that declares a model
    /// contact.** `None` for a cross-link, which has no seat and must be graded
    /// against the whole surface.
    ///
    /// It is frozen in `gather_candidates`, at the ONE moment this family has a
    /// creation radius and a seat face together, and it travels on the
    /// candidate so that `evaluate_candidates` (the PLACEMENT grading site) and
    /// `append_candidate` (the mint) read the SAME face set. A zone recomputed
    /// between those two would be the retired skip-length's inversion rebuilt
    /// one level up (design §3).
    ///
    /// The whole RECORD travels, not the bare face list, because
    /// `append_candidate` installs it into the forest and the forest's side
    /// table is what a later re-size reads.
    seat_zone: Option<crate::seat_zone::SeatZoneRecord>,
}

#[derive(Clone, Debug)]
struct RawModelCandidate {
    need_index: usize,
    site_index: usize,
    rank: usize,
    host: MemberKey,
    z_mm: f64,
    centroid: [f64; 3],
}

#[derive(Clone, Debug)]
struct ProjectedModelCandidate {
    raw: RawModelCandidate,
    point: [f64; 3],
    normal: [f64; 3],
    /// **#36 — the global face the seat projected onto.** The projection that
    /// resolves `point` and `normal` already computes it and used to discard
    /// it; capturing it is what lets the seat zone be frozen without a second
    /// native call. `None` only when the projection landed off the mesh, which
    /// leaves the rung unfrozen and therefore graded against everything — the
    /// strict direction.
    seat_face_id: Option<u32>,
}

/// Port of the live `compute_needs` plus candidate placement, without mutating
/// caller state. The supplied report is checked against a fresh canonical
/// report so stale snapshots cannot authorize geometry.
pub(crate) fn brace_pass_current_parity(
    forest: &Forest,
    surface: &NativeSurface,
    topology: &FaceTopology,
    plate_z_mm: Finite,
    report: &TransientReport,
    penetration_limits: PenetrationBatchLimits,
    process_resolution_mm: f64,
) -> Result<BracePassPlan, BraceError> {
    if &transient_report(forest, plate_z_mm)? != report {
        return Err(BraceError::StaleTransientReport);
    }
    let config = BraceConfig::current(process_resolution_mm)?;
    let mut need_batch = compute_needs(forest, report, &config)?;
    append_clearance_needs(
        forest,
        surface,
        report,
        &config,
        penetration_limits,
        &mut need_batch,
    )?;
    let CandidateGather {
        candidates,
        attestation_digest: contact_attestation_digest,
    } = gather_candidates(forest, surface, topology, &need_batch.needs, &config)?;
    let viable = evaluate_candidates(surface, &candidates, &config, penetration_limits)?;
    let mut planned = forest.clone();
    let mut additions = Vec::new();
    let mut ordinals = BTreeMap::<BraceAdditionKind, u32>::new();
    let mut tried_zs = BTreeMap::<MemberKey, Vec<f64>>::new();
    let mut rungs_placed = 0;
    let mut rungs_unplaceable = 0;
    for (need_index, need) in need_batch.needs.iter().enumerate() {
        let mut pending = need.triggers.clone();
        let mut stacked = 0;
        let mut placed_this_need = false;
        let site_count = 1 + need.alternatives.len();
        for site_index in 0..site_count {
            let (site_host, site_z) = if site_index == 0 {
                (need.host, need.z_mm)
            } else {
                need.alternatives[site_index - 1]
            };
            if site_was_attempted(&tried_zs, site_host, site_z, config.ladder_z_mm) {
                continue;
            }
            // Frozen Python parity records the attempted host/height before
            // trying any of its primary or fallback geometry candidates.
            tried_zs.entry(site_host).or_default().push(site_z);
            let winner = candidates
                .iter()
                .enumerate()
                .find(|(candidate_index, candidate)| {
                    candidate.need_index == need_index
                        && candidate.site_index == site_index
                        && viable[*candidate_index]
                });
            let Some((_, candidate)) = winner else {
                continue;
            };
            append_candidate(&mut planned, candidate, &mut ordinals, &mut additions)?;
            placed_this_need = true;
            stacked += 1;
            if pending.is_empty() || stacked >= config.max_stacked {
                break;
            }
            let fresh = transient_report(&planned, plate_z_mm)?;
            pending = pending_triggers_after_trial(
                &planned,
                surface,
                topology,
                plate_z_mm,
                &fresh,
                pending,
                &config,
                penetration_limits,
            )?
            .pending;
            if pending.is_empty() {
                break;
            }
        }
        if placed_this_need {
            rungs_placed += 1;
        } else {
            rungs_unplaceable += 1;
        }
    }

    let resize = resize_forest_current_parity(&planned, plate_z_mm)?;
    Ok(BracePassPlan {
        forest_with_braces: planned,
        additions,
        resize,
        rungs_requested: need_batch.requested,
        rungs_placed,
        rungs_unplaceable,
        contact_attestation_digest,
    })
}

/// Frozen three-pass bracing/resize controller. It returns a new forest and a
/// named non-convergence outcome; it never asserts or silently falls through.
///
/// `sealed` is the routing seal's own record — every terminal it named, with
/// the disposition it named. **Stage A** of the seal-semantics design uses it
/// to decide whether the third-pass sweep may run at all: a sweep that would
/// flip a sealed terminal's live state abandons the pass instead of writing on
/// top of a record it cannot change. Pass an empty slice to get today's
/// behaviour exactly, which is what every unit fixture without a sealed record
/// does.
pub(crate) fn brace_fixed_point_current_parity(
    forest: &Forest,
    surface: &NativeSurface,
    topology: &FaceTopology,
    plate_z_mm: Finite,
    penetration_limits: PenetrationBatchLimits,
    process_resolution_mm: f64,
    sealed: &[SealedTerminalDisposition],
) -> Result<BraceFixedPointPlan, BraceError> {
    const FROZEN_PASS_CAP: usize = 3;
    let mut working = forest.clone();
    let mut additions = Vec::new();
    let mut saw_unplaceable = false;
    // F3 -- the model-rung seat-attestation chain across the bounded passes.
    // Chained rather than summed or replaced, so a pass that stopped attesting
    // moves the head even if a later pass attests the same rows.
    let mut contact_attestation_digest = contact_attestation_chain_seed();
    for iteration in 1..=FROZEN_PASS_CAP {
        let report = transient_report(&working, plate_z_mm)?;
        let plan = brace_pass_current_parity(
            &working,
            surface,
            topology,
            plate_z_mm,
            &report,
            penetration_limits,
            process_resolution_mm,
        )?;
        saw_unplaceable |= plan.rungs_unplaceable > 0;
        contact_attestation_digest = extend_contact_attestation_chain(
            contact_attestation_digest,
            iteration,
            plan.contact_attestation_digest,
        );
        let changed = !plan.resize.changed.is_empty();
        working = apply_resize(plan.forest_with_braces, &plan.resize)?;
        // **THE RE-SIZE — Craig's ruling of 2026-08-23, at the brace
        // controller's post-sizing point.**
        //
        // `apply_resize` has just rewritten radii across the live tree, and a
        // seated member it fattened is carrying a band cut at a radius it no
        // longer has. The exemption must equal the arm's true footprint with no
        // slack, so the bands are re-cut here — after the sizing, before
        // `exact_clearance_negatives` grades anything at FINAL, in ONE batched
        // kernel call for the whole forest.
        //
        // It runs once per controller iteration because there is one sizing pass
        // per iteration. That is not continuous tracking: each call sits at a
        // moment when no grading is in flight, which is the whole of what the
        // monotonicity theorem needs (design §3).
        crate::seat_zone::refreeze_resized_seat_zones(
            surface,
            topology,
            BraceConfig::current(process_resolution_mm)?.standoff_mm,
            std::iter::once(&mut working),
        )
        .map_err(|error| BraceError::SeatZone(error.to_string()))?;
        let added = !plan.additions.is_empty();
        additions.extend(plan.additions);
        let negative = exact_clearance_negatives(
            &working,
            surface,
            &BraceConfig::current(process_resolution_mm)?,
            penetration_limits,
        )?;
        if !added && !changed && negative.is_clean() {
            let final_pass_refusals = FinalPassRefusalAccounting::read(&working, &negative);
            final_pass_refusals.disclose(iteration);
            return Ok(BraceFixedPointPlan {
                forest: working,
                additions,
                iterations: iteration,
                termination: if saw_unplaceable {
                    BraceFixedPointTermination::Unplaceable
                } else {
                    BraceFixedPointTermination::Converged
                },
                contact_refusals: Vec::new(),
                contact_attestation_digest,
                final_pass_refusals,
            });
        }
        if iteration == FROZEN_PASS_CAP {
            // Refusal accounting is read and streamed BEFORE the truth table
            // runs, so an unrefusable violation — which returns `Err` below and
            // never reaches a plan — still discloses its measured magnitude.
            let final_pass_refusals = FinalPassRefusalAccounting::read(&working, &negative);
            final_pass_refusals.disclose(iteration);
            let termination = pass_three_termination(
                saw_unplaceable,
                added,
                changed,
                negative.pipeline_violations,
                !negative.contact_refusals.is_empty(),
            )?
            .expect("the prior convergence return handles a stable clean third pass");
            // PR-9. A refused contact is quarantined, not shipped and not
            // hard-errored. Quarantining here rather than at pass 1 or 2 is
            // deliberate: the earlier passes are still iterating and a member
            // that is inside the model at pass 1 may be resized out of it at
            // pass 2, so removing it early would delete a support the
            // controller was about to fix.
            //
            // OD-4, and the Giant Bat's hard failure. A refused member may be a
            // CARRIER, not a leaf: `cco::apply_model_anchor` mints the seated
            // arm as a root and re-parents the contact stub onto it, so every
            // placed model-seated arm has a child by construction. A per-member
            // `quarantine` therefore refused it — `Forest(QuarantineHasChildren)`
            // — and that refusal travelled out of here as a hard pipeline
            // failure, which is exactly what OD-4 forbids: "a refused
            // genuine-contact stub routes to the existing quarantine, never a
            // hard pipeline failure".
            //
            // The sweep is the honest reading of the verdict rather than a way
            // around the arena's rule. A condemned carrier takes its dependants'
            // only route to ground with it: the arm is a root, so its stub has
            // no grandparent to be spliced onto, and detaching the stub is the
            // promote-to-root move `Forest::retire` names as the one reachable
            // path to shipping a floating member. Every member below a condemned
            // one has no closed load path afterwards, so the same verdict
            // applies to it, and `quarantine_subtrees` records each one with an
            // ordinary quarantine record — which is what SPEC §6's removal and
            // the `quarantined` noncoverage component already read.
            //
            // Batched, and deliberately so: the sweep resolves overlapping
            // subtrees once (a refused stub that is itself the child of a
            // refused arm is condemned once, not twice, and does not become an
            // `InactiveKey` error on the second visit).
            //
            // **Stage A stands here.** Before the sweep runs, the condemnation
            // set is planned READ-ONLY by the planner half of this very
            // mutation and intersected with the sealed record. An empty
            // intersection applies the sweep exactly as it always did — the
            // refutation's own carve-out, and the behaviour of every model
            // whose brace refusals disturb no contact accounting. A non-empty
            // one abandons the pass instead of minting a terminal pair the
            // accounting forbids.
            if let Some(class) =
                classify_sealed_condemnation(&working, sealed, &negative, &final_pass_refusals)?
            {
                class.disclose();
                let class = Box::new(class);
                return Err(if class.has_steerable_sealed_route() {
                    BraceError::SealedRoutedTerminalCondemned(class)
                } else {
                    BraceError::SealedSweepLearnedNothing(class)
                });
            }
            working.quarantine_subtrees(&negative.contact_refusals)?;
            return Ok(BraceFixedPointPlan {
                forest: working,
                additions,
                iterations: iteration,
                termination,
                contact_refusals: negative.contact_refusals,
                contact_attestation_digest,
                final_pass_refusals,
            });
        }
    }
    unreachable!("the frozen positive pass cap always returns")
}

/// Exact frozen decision at the third-pass cap.  A stable clean state has
/// already returned as converged above; this helper names the remaining
/// outcomes so tests can pin the bounded-controller truth table without
/// manufacturing an otherwise unreachable production geometry.
fn pass_three_termination(
    saw_unplaceable: bool,
    added: bool,
    changed: bool,
    pipeline_violations: usize,
    contact_refused: bool,
) -> Result<Option<BraceFixedPointTermination>, BraceError> {
    // PR-9. Only a negative-clearance member that declares NO model contact is
    // a pipeline violation. A contact-declaring one is an ordinary candidate
    // refusal, and OD-4 forbids turning that into a hard pipeline failure —
    // "a refused genuine-contact stub routes to the existing quarantine, never
    // a hard pipeline failure". A real violation still wins: if both are
    // present the run fails, because the un-refusable one is unfixable here.
    if pipeline_violations > 0 {
        return Err(BraceError::ClearanceViolation(pipeline_violations));
    }
    if contact_refused {
        return Ok(Some(BraceFixedPointTermination::ContactRefused));
    }
    if !added && !changed {
        return Ok(None);
    }
    Ok(Some(if saw_unplaceable {
        BraceFixedPointTermination::Unplaceable
    } else {
        BraceFixedPointTermination::IterationLimit
    }))
}

fn apply_resize(mut forest: Forest, plan: &TransientResizePlan) -> Result<Forest, BraceError> {
    for row in &plan.radii {
        forest.set_radii(row.member, row.terminalward, row.rootward)?;
    }
    Ok(forest)
}

fn append_candidate(
    forest: &mut Forest,
    candidate: &PlacementCandidate,
    ordinals: &mut BTreeMap<BraceAdditionKind, u32>,
    additions: &mut Vec<BraceAddition>,
) -> Result<(), BraceError> {
    let ordinal = ordinals.entry(candidate.kind).or_default();
    crate::census_record!(crate::census::record_append(match candidate.kind {
        BraceAdditionKind::CrossLink => "brace_append_crosslink",
        BraceAdditionKind::ModelRung => "brace_append_model_rung",
    }));
    for rod in &candidate.rods {
        let spec = MemberSpec {
            p0_mm: finite3(rod.p0)?,
            p1_mm: finite3(rod.p1)?,
            r0_mm: TerminalEndRadius::new(rod.radius),
            r1_mm: RootwardEndRadius::new(rod.radius),
            kind: match candidate.kind {
                BraceAdditionKind::CrossLink => MemberKind::Brace,
                BraceAdditionKind::ModelRung => {
                    MemberKind::Other(contact::MODEL_RUNG_FAMILY.to_owned())
                }
            },
            owner: crate::forest::Owner::Generated {
                family: match candidate.kind {
                    BraceAdditionKind::CrossLink => "brace",
                    BraceAdditionKind::ModelRung => contact::MODEL_RUNG_FAMILY,
                }
                .to_owned(),
                ordinal: *ordinal,
            },
            model_contact_end: (candidate.kind == BraceAdditionKind::ModelRung)
                .then_some(ContactEnd::Rootward),
            // MAJOR-7 / F2. Zero is the CORRECT routed load for both brace
            // kinds, not an unwired input: `add_brace` below puts this member
            // in the forest's `braces` set, and `set_parent` refuses a brace as
            // either child or parent (`ForestError::BraceInTree`), so no load
            // path can traverse it. The full argument, including the contrast
            // with `cco::apply_model_anchor`'s real
            // `carried_mm2: request.carried_mm2()`, is at the
            // `contact::admit` call site in `gather_candidates`.
            carried_mm2: finite(0.0)?,
        };
        *ordinal = ordinal
            .checked_add(1)
            .ok_or(BraceError::CountNotRepresentable)?;
        let key = forest.add_brace(Member::new(spec.clone()));
        // **#36 — the mint half of creation site 2.** The zone was frozen in
        // `gather_candidates` from this rung's creation radius and its own seat
        // face; here it is bound to the key the grading sites will look it up
        // by.
        //
        // For a model rung the creation band is also the FINAL band, because a
        // rung is never re-sized: `add_brace` above puts it in the forest's
        // `braces` set, `live_tree_members` filters that set out
        // (`forest.rs`), and `transient::postorder` walks only
        // `live_tree_members`. The re-size pass therefore finds nothing to do
        // for a rung — it is covered by the same uniform scan as everything
        // else, and that scan is what would notice if this ever stopped being
        // true.
        if let Some(record) = candidate.seat_zone.as_ref() {
            forest.set_seat_zone(key, record.clone());
        }
        additions.push(BraceAddition {
            key,
            kind: candidate.kind,
            member: spec,
        });
    }
    Ok(())
}

#[allow(
    clippy::too_many_arguments,
    reason = "the eighth argument is the FACE TOPOLOGY, and it is here because the trial forest \
              must have its seated exemptions re-sized before it is graded (Craig's ruling, \
              2026-08-23). Every other argument is already the same set \
              `brace_pass_current_parity` holds; bundling surface+topology into a pair type \
              would be a crate-wide refactor this lane is not taking"
)]
fn pending_triggers_after_trial(
    forest: &Forest,
    surface: &NativeSurface,
    topology: &FaceTopology,
    plate_z_mm: Finite,
    report: &TransientReport,
    pending: Vec<MemberKey>,
    config: &BraceConfig,
    penetration_limits: PenetrationBatchLimits,
) -> Result<TriggerReliefResult, BraceError> {
    let compatibility = LegacyCompatibility::o10_live_current_support_and_raft();
    let mut threshold_relieved = BTreeSet::new();
    for trigger in &pending {
        let Some(evidence) = report.member(*trigger) else {
            threshold_relieved.insert(*trigger);
            continue;
        };
        let member = forest
            .member(*trigger)
            .ok_or(BraceError::MissingMember(*trigger))?;
        let transient = transient_radius(
            evidence.worst_span_mm.get(),
            lean_angle(member)?,
            evidence.n_terminals,
            &compatibility,
        )?;
        if evidence.worst_span_mm.get() <= compatibility.brace_free_span_mm().diagnostic().get()
            && transient <= config.max_unbraced_radius_mm
        {
            threshold_relieved.insert(*trigger);
        }
    }

    if threshold_relieved.is_empty() {
        return Ok(TriggerReliefResult {
            pending,
            resize_batches: 0,
            exact_clearance_batches: 0,
            seat_zone_refreeze_batches: 0,
        });
    }
    // Shared triggers use one complete resize and one exact-clearance batch.
    // The immutable trial is common to every trigger because the forest does
    // not change while their relief verdicts are scattered.
    let resize = resize_forest_current_parity(forest, plate_z_mm)?;
    let mut trial = apply_resize(forest.clone(), &resize)?;
    // **THE RE-SIZE — Craig's ruling of 2026-08-23, on the TRIAL forest.**
    //
    // Easy to miss and it changes verdicts. The trial exists to answer "is this
    // trigger relieved once everything is sized?", and it answers by grading at
    // FINAL. Grading the trial with bands cut at pre-resize radii would read a
    // seated arm worse than the geometry it is about to have, leave the trigger
    // unrelieved, and place a rung nobody needed. One batched call, on the same
    // batch boundary this function already declares.
    crate::seat_zone::refreeze_resized_seat_zones(
        surface,
        topology,
        config.standoff_mm,
        std::iter::once(&mut trial),
    )
    .map_err(|error| BraceError::SeatZone(error.to_string()))?;
    let clearance = exact_clearance_rows(&trial, surface, config, penetration_limits)?
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    Ok(TriggerReliefResult {
        pending: pending
            .into_iter()
            .filter(|trigger| {
                !threshold_relieved.contains(trigger)
                    || clearance.get(trigger).is_some_and(|value| *value < 0.0)
            })
            .collect(),
        resize_batches: 1,
        exact_clearance_batches: 1,
        seat_zone_refreeze_batches: 1,
    })
}

fn append_clearance_needs(
    forest: &Forest,
    surface: &NativeSurface,
    report: &TransientReport,
    config: &BraceConfig,
    penetration_limits: PenetrationBatchLimits,
    batch: &mut NeedBatch,
) -> Result<(), BraceError> {
    for member in exact_clearance_violated_members(forest, surface, config, penetration_limits)? {
        let Some(evidence) = report.member(member) else {
            continue;
        };
        let Some(site) = clearance_relief_site(forest, evidence, config) else {
            continue;
        };
        if batch.needs.iter().any(|need| {
            need.host == site.host && (need.z_mm - site.z_mm).abs() <= config.ladder_z_mm / 2.0
        }) {
            continue;
        }
        batch.requested += 1;
        batch.needs.push(Need {
            host: site.host,
            z_mm: site.z_mm,
            rung_index: 0,
            path_mapped: site.path_mapped,
            // The violated member is this need's TRIGGER, not merely its host.
            // An empty trigger list made the controller stop after the first
            // rung it managed to place, whatever that rung did to the span; the
            // trigger makes `pending_triggers_after_trial` re-measure the exact
            // clearance and keep working the remaining sites until the member is
            // actually back outside the standoff shell.
            triggers: vec![member],
            alternatives: site.alternatives,
        });
    }
    batch.needs.sort_by(|left, right| {
        left.host
            .cmp(&right.host)
            .then_with(|| left.z_mm.total_cmp(&right.z_mm))
            .then_with(|| left.triggers.cmp(&right.triggers))
    });
    Ok(())
}

/// Where to ask for the rung that is supposed to relieve one negative-clearance
/// member.
struct ClearanceReliefSite {
    host: MemberKey,
    z_mm: f64,
    alternatives: Vec<(MemberKey, f64)>,
    path_mapped: bool,
}

/// Aim a clearance-triggered brace request at the span that actually sets the
/// violated member's radius.
///
/// The radius that pushed the member inside the standoff shell is not a
/// property of the member alone. `resize_forest_current_parity` sizes every
/// member from `worst_span_mm`, which is the worst FREE SPAN along that
/// member's root-to-terminal path, so the span to shorten generally lives on
/// OTHER members of that path. Asking for a rung on the violated member is
/// therefore asking the wrong member to change: on the Giant Bat, member 466's
/// own largest free gap is 1.667 mm tall while the span driving its radius is
/// 7.893 mm and lies elsewhere on its path, so no rung inside that gap could
/// move the sizing driver at all.
///
/// This aims the request the same way `compute_needs` aims its
/// activation-triggered ones: map the midpoint of the worst-span interval onto
/// the path member that spans that height, and offer the rest of the interval
/// as fallback sites. A path-mapped need is also the only kind
/// `gather_candidates` will widen to the reach tiers for, which is the second
/// half of why clearance needs used to gather nothing.
///
/// When the report carries no usable path span the old behaviour stands: aim at
/// the middle of the member's own largest free gap, unmapped.
fn clearance_relief_site(
    forest: &Forest,
    evidence: &MemberTransientEvidence,
    config: &BraceConfig,
) -> Option<ClearanceReliefSite> {
    let path = &evidence.worst_span_path;
    let lo = evidence.worst_span_interval_mm.lo_mm.get();
    let hi = evidence.worst_span_interval_mm.hi_mm.get();
    if path.len() > 1 && hi > lo {
        let z = (lo + hi) / 2.0;
        if let Some(host) = map_height_to_path_member(forest, path, z) {
            return Some(ClearanceReliefSite {
                host,
                z_mm: z,
                alternatives: path_fallbacks(forest, path, host, z, lo, hi, config),
                path_mapped: true,
            });
        }
    }
    let gap = largest_gap(&evidence.free_gaps_mm)?;
    Some(ClearanceReliefSite {
        host: evidence.member,
        z_mm: (gap.lo_mm.get() + gap.hi_mm.get()) / 2.0,
        alternatives: Vec::new(),
        path_mapped: false,
    })
}

fn compute_needs(
    forest: &Forest,
    report: &TransientReport,
    config: &BraceConfig,
) -> Result<NeedBatch, BraceError> {
    #[derive(Clone, Debug)]
    struct StaticBand {
        lo: f64,
        hi: f64,
        rungs: usize,
    }
    #[derive(Clone, Debug)]
    struct PathEntry {
        host: MemberKey,
        z: f64,
        alternatives: Vec<(MemberKey, f64)>,
        triggers: Vec<MemberKey>,
    }

    let compatibility = LegacyCompatibility::o10_live_current_support_and_raft();
    let free_span = compatibility.brace_free_span_mm().diagnostic().get();
    let mut static_bands = BTreeMap::<MemberKey, Vec<StaticBand>>::new();
    let mut claimed = BTreeMap::<MemberKey, Vec<(f64, Option<usize>)>>::new();
    let mut path_entries = Vec::<PathEntry>::new();

    // Pass one: static free gaps, in stable member/gap order.
    for evidence in &report.members {
        let member = forest
            .member(evidence.member)
            .ok_or(BraceError::MissingMember(evidence.member))?;
        let lean = lean_angle(member)?;
        for gap in &evidence.free_gaps_mm {
            let span = gap.hi_mm.get() - gap.lo_mm.get();
            if span <= 0.0 {
                continue;
            }
            let n_slender = slender_rungs(span, lean, config, &compatibility)?;
            let n_span = (span / free_span) as usize;
            let n = n_slender.max(n_span);
            if n == 0 {
                continue;
            }
            static_bands
                .entry(evidence.member)
                .or_default()
                .push(StaticBand {
                    lo: gap.lo_mm.get(),
                    hi: gap.hi_mm.get(),
                    rungs: n,
                });
            for k in 0..n {
                let z = gap.lo_mm.get() + (k + 1) as f64 * span / (n + 1) as f64;
                claimed.entry(evidence.member).or_default().push((z, None));
            }
        }
    }

    // Pass two: activation-aware full-path interval trigger.
    for evidence in &report.members {
        if evidence.worst_span_mm.get() <= 0.0 || evidence.free_gaps_mm.is_empty() {
            continue;
        }
        let member = forest
            .member(evidence.member)
            .ok_or(BraceError::MissingMember(evidence.member))?;
        let lean = lean_angle(member)?;
        let transient_radius = transient_radius(
            evidence.worst_span_mm.get(),
            lean,
            evidence.n_terminals,
            &compatibility,
        )?;
        if transient_radius <= config.max_unbraced_radius_mm
            && evidence.worst_span_mm.get() <= free_span
        {
            continue;
        }
        let n = slender_rungs(evidence.worst_span_mm.get(), lean, config, &compatibility)?
            .max((evidence.worst_span_mm.get() / free_span) as usize);
        if n == 0 {
            continue;
        }
        let path = &evidence.worst_span_path;
        let interval = evidence.worst_span_interval_mm;
        if path.len() <= 1 || interval.hi_mm.get() <= interval.lo_mm.get() {
            let Some(gap) = largest_gap(&evidence.free_gaps_mm) else {
                continue;
            };
            let fit = rungs_that_fit(gap.hi_mm.get() - gap.lo_mm.get(), n);
            if fit == 0 {
                continue;
            }
            let bands = static_bands.entry(evidence.member).or_default();
            if let Some(existing) = bands.iter_mut().find(|band| {
                band.lo.to_bits() == gap.lo_mm.get().to_bits()
                    && band.hi.to_bits() == gap.hi_mm.get().to_bits()
            }) {
                existing.rungs = existing.rungs.max(fit);
            } else {
                bands.push(StaticBand {
                    lo: gap.lo_mm.get(),
                    hi: gap.hi_mm.get(),
                    rungs: fit,
                });
            }
            continue;
        }
        let lo = interval.lo_mm.get();
        let hi = interval.hi_mm.get();
        let fit = rungs_that_fit(hi - lo, n);
        for k in 0..fit {
            let z = lo + (k + 1) as f64 * (hi - lo) / (fit + 1) as f64;
            let Some(host) = map_height_to_path_member(forest, path, z) else {
                continue;
            };
            let colliding = claimed
                .get(&host)
                .into_iter()
                .flatten()
                .copied()
                .filter(|(other_z, _)| {
                    (z - other_z).abs() <= config.ladder_z_mm / 2.0
                        && lo <= *other_z
                        && *other_z <= hi
                })
                .collect::<Vec<_>>();
            let mut mergeable = colliding
                .iter()
                .filter_map(|(other_z, entry)| entry.map(|index| (*other_z, index)))
                .collect::<Vec<_>>();
            if !mergeable.is_empty() {
                mergeable.sort_by(|left, right| {
                    left.0
                        .total_cmp(&right.0)
                        .then_with(|| left.1.cmp(&right.1))
                });
                let entry = &mut path_entries[mergeable[0].1];
                if !entry.triggers.contains(&evidence.member) {
                    entry.triggers.push(evidence.member);
                    entry.triggers.sort();
                }
                let mut additions = path_fallbacks(forest, path, host, z, lo, hi, config);
                additions.push((host, z));
                merge_alternatives(&mut entry.alternatives, additions);
                continue;
            }
            if !colliding.is_empty() {
                // A static claim becomes a real restraint in this pass; the
                // outer fixed point rechecks any residual activation need.
                continue;
            }
            let entry_index = path_entries.len();
            claimed
                .entry(host)
                .or_default()
                .push((z, Some(entry_index)));
            path_entries.push(PathEntry {
                host,
                alternatives: path_fallbacks(forest, path, host, z, lo, hi, config),
                z,
                triggers: vec![evidence.member],
            });
        }
    }
    let mut requested = 0;
    let mut needs = path_entries
        .into_iter()
        .map(|entry| {
            requested += 1;
            Need {
                host: entry.host,
                z_mm: entry.z,
                rung_index: 0,
                path_mapped: true,
                triggers: entry.triggers,
                alternatives: entry.alternatives,
            }
        })
        .collect::<Vec<_>>();
    for evidence in &report.members {
        for band in static_bands.remove(&evidence.member).unwrap_or_default() {
            requested += band.rungs;
            for k in 0..band.rungs {
                let z = band.lo + (k + 1) as f64 * (band.hi - band.lo) / (band.rungs + 1) as f64;
                if evidence
                    .restraint_zs_mm
                    .iter()
                    .any(|restraint| (z - restraint.get()).abs() <= config.ladder_z_mm / 2.0)
                {
                    continue;
                }
                needs.push(Need {
                    host: evidence.member,
                    z_mm: z,
                    rung_index: k,
                    path_mapped: false,
                    triggers: Vec::new(),
                    alternatives: Vec::new(),
                });
            }
        }
    }
    needs.sort_by(|left, right| {
        left.host
            .cmp(&right.host)
            .then_with(|| left.z_mm.total_cmp(&right.z_mm))
            .then_with(|| left.triggers.cmp(&right.triggers))
    });
    Ok(NeedBatch { requested, needs })
}

fn merge_alternatives(existing: &mut Vec<(MemberKey, f64)>, additions: Vec<(MemberKey, f64)>) {
    existing.extend(additions);
    existing.sort_by(|left, right| {
        left.1
            .total_cmp(&right.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    existing.dedup_by(|left, right| left.0 == right.0 && left.1.to_bits() == right.1.to_bits());
}

fn site_was_attempted(
    tried_zs: &BTreeMap<MemberKey, Vec<f64>>,
    host: MemberKey,
    z_mm: f64,
    ladder_z_mm: f64,
) -> bool {
    tried_zs.get(&host).is_some_and(|heights| {
        heights
            .iter()
            .any(|height| (z_mm - height).abs() <= ladder_z_mm / 2.0)
    })
}

/// One pass's candidate set plus the PR-8 seat-attestation transcript for the
/// model-rung family.
///
/// The transcript is returned rather than folded into the candidate list
/// because it must survive into the digest chain even when it refuses nothing:
/// an attested rung and an unattested one are the same geometry, so no forest
/// digest can see the attestation stop happening.
struct CandidateGather {
    candidates: Vec<PlacementCandidate>,
    /// `seat_attestation_digest(SEAT_TAG_BRACE_MODEL_RUNG, ..)` over every
    /// model-rung candidate this pass proposed, refused rows included.
    attestation_digest: u64,
}

fn gather_candidates(
    forest: &Forest,
    surface: &NativeSurface,
    topology: &FaceTopology,
    needs: &[Need],
    config: &BraceConfig,
) -> Result<CandidateGather, BraceError> {
    let mut candidates = Vec::new();
    let mut raw_model = Vec::new();
    // Model-rung candidates are held aside from the cross-link ones until the
    // whole-pass seat attestation has run, so a refused rung never reaches the
    // tournament. They rejoin `candidates` before the deterministic sort, so
    // candidate ORDER is unchanged for every rung that attests.
    let mut model_candidates = Vec::new();
    let mut seat_rows = Vec::new();
    // #36 — one seat-zone request per model-rung candidate, row-aligned with
    // `model_candidates`. `None` where the seat's projection landed off the
    // mesh; see `ProjectedModelCandidate::seat_face_id`.
    let mut seat_zone_rows: Vec<Option<crate::seat_zone::SeatZoneRequest>> = Vec::new();
    for (need_index, need) in needs.iter().enumerate() {
        let mut sites = vec![(need.host, need.z_mm)];
        sites.extend(need.alternatives.iter().copied());
        let mut rank = 0;
        for (site_index, (host, z)) in sites.into_iter().enumerate() {
            let standard = cross_link_candidates(
                forest,
                host,
                z,
                need.rung_index,
                config,
                config.neighbor_max_xy_mm,
            )?;
            let mut cross = standard.clone();
            if standard.is_empty() && need.path_mapped {
                let mut tried = BTreeSet::new();
                for cap in reach_tiers_mm(config) {
                    for row in cross_link_candidates(forest, host, z, need.rung_index, config, cap)?
                    {
                        if tried.insert(row.1) {
                            cross.push(row);
                        }
                    }
                }
            }
            for (distance, neighbor) in cross {
                let rods =
                    cross_link_rods(forest, host, neighbor, z, distance, need.rung_index, config)?;
                candidates.push(PlacementCandidate {
                    need_index,
                    site_index,
                    rank,
                    kind: BraceAdditionKind::CrossLink,
                    printable: rods_printable(&rods)?,
                    rods,
                    // A cross-link declares no model contact, so it forgives
                    // nothing and is graded against the whole surface.
                    seat_zone: None,
                });
                rank += 1;
            }
            for centroid in model_rung_anchor_centroids(surface, forest, host, z, config)? {
                raw_model.push(RawModelCandidate {
                    need_index,
                    site_index,
                    rank,
                    host,
                    z_mm: z,
                    centroid,
                });
                rank += 1;
            }
        }
    }
    let mut projected_model = Vec::new();
    if !raw_model.is_empty() {
        let probes = raw_model.iter().map(|row| row.centroid).collect::<Vec<_>>();
        // #36 — the third return is the projected FACE, which this call has
        // always computed and always thrown away. It is the seat's own face,
        // and the seat zone is the connected component of the band that
        // contains it.
        let (_, projected, face_ids, normals) = surface.project(&probes);
        for (((raw, p1), face_id), normal) in raw_model
            .into_iter()
            .zip(projected)
            .zip(face_ids)
            .zip(normals)
        {
            if (p1[2] - raw.z_mm).abs() > config.model_rung_z_tolerance_mm + HEIGHT_TOLERANCE_MM {
                continue;
            }
            projected_model.push(ProjectedModelCandidate {
                raw,
                point: p1,
                normal,
                seat_face_id: u32::try_from(face_id).ok(),
            });
        }
    }
    // The six-anchor cap applies after exact projection and foot-breach
    // filtering. Gather every surviving projected foot into one sphere batch,
    // then scatter the first six per deterministic need/site order.
    let breached = if projected_model.is_empty() {
        Vec::new()
    } else {
        surface
            .sphere_breach(
                &projected_model
                    .iter()
                    .map(|candidate| candidate.point)
                    .collect::<Vec<_>>(),
                &projected_model
                    .iter()
                    .map(|_| config.ball_factor_foot * config.brace_radius_mm.get())
                    .collect::<Vec<_>>(),
                config.ball_breach_epsilon_mm,
                FOOT_BREACH_HALF_DEG,
                FOOT_BREACH_CONE_RAYS,
            )
            .map_err(|error| BraceError::NativeQuery(error.to_string()))?
            .0
    };
    retain_projected_model_survivors(
        &mut projected_model,
        &breached,
        config.model_rung_max_candidates,
    );
    for projected in projected_model {
        let member = forest
            .member(projected.raw.host)
            .ok_or(BraceError::MissingMember(projected.raw.host))?;
        let p0 = axis_point_at_z(member, projected.point[2]);
        let length = norm(sub(projected.point, p0));
        if length + HEIGHT_TOLERANCE_MM < config.model_rung_min_length_mm
            || length > config.model_brace_max_length_mm + HEIGHT_TOLERANCE_MM
        {
            continue;
        }
        let radius = config.brace_radius_mm;
        let axis = scale(sub(projected.point, p0), 1.0 / length);
        let abs_cos_raw = dot(axis, projected.normal).abs();

        // PR-1: SEAT the rung rather than kissing the surface. Today's code
        // sets p1 to the projected surface point exactly, producing a
        // measure-zero tangential contact whose fusion under union is decided
        // by voxel rounding -- precisely the "properly attaches the tips for
        // all cases" failure the owner named.
        let policy = config.contact_policy(contact::ClearanceStage::Final);
        let seat = contact::seat_depth_mm(abs_cos_raw, radius.get(), &policy, false);
        let (seated_point, seated_cos, seated_length) =
            contact::seat_endpoint(projected.point, projected.normal, p0, seat);

        // PR-2/PR-3 evaluated on the SEATED geometry -- what the kernel will
        // actually see. Exactly one pass: the PR-1 monotonicity lemma proves
        // seating raises both |cos| and length, so the seat computed at the
        // unseated incidence remains valid at the seated one. Nothing iterates.
        // MAJOR-7 / F2 -- WHY `carried_mm2` IS ZERO HERE, AND WHY THAT IS THE
        // CORRECT VALUE RATHER THAN AN UNWIRED ONE.
        //
        // `carried_mm2` is ROUTED DEMAND: the model area whose peel force this
        // member must hold. It has exactly one authority,
        // `perception::FinalizedContactLoads`, which partitions every eligible
        // model face's area over the PHYSICAL CONTACT ids -- each face's
        // `area_mm2` is charged to its one nearest eligible contact and summed
        // per contact slot. That partition is complete and exclusive, and its
        // only consumer that mints geometry is `initial.rs`, which copies
        // `loads.carried_area_mm2()[slot]` into each contact stub. Every unit
        // of demand on the corpus is therefore already charged to a stub before
        // brace ever runs.
        //
        // A brace model rung is a BRACING member, not a demand route, and the
        // forest enforces that structurally rather than by convention:
        // `append_candidate` mints it through `Forest::add_brace`, which removes
        // the key from `roots` and inserts it into `braces`; `set_parent` then
        // returns `ForestError::BraceInTree` if a brace is offered as either
        // child or parent, and `live_tree_members` filters braces out of the
        // load tree entirely. A brace cannot appear on any path from a contact
        // to the plate, so no load can route through one -- charging it a
        // nonzero `carried_mm2` would DOUBLE-COUNT area already charged to the
        // stub that owns that face.
        //
        // The contrast inside this same package is the proof that zero is a
        // decision and not an omission: `cco::apply_model_anchor` mints its
        // strut with `carried_mm2: request.carried_mm2()`, a real routed load,
        // because a CCO model anchor IS the terminal's route to ground. Demand
        // route -> real load; bracing member -> zero. Both are attested; only
        // the route is graded by PR-4's area law, because only the route has an
        // area demand to grade.
        //
        // Consequence for PR-4 here: with `carried_mm2 = 0.0` the area law is
        // satisfied by any interface, which is correct -- a member carrying no
        // routed load has no area requirement. PR-8's attachment proof and
        // OD-9's exterior screen still apply in full, and they are what this
        // family is actually gated on.
        let site = contact::ContactSite {
            end: ContactEnd::Rootward,
            end_radius_mm: radius.get(),
            member_r_max_mm: radius.get(),
            abs_cos_incidence: abs_cos_raw,
            // Zero routed load, by the argument above -- not an unwired input.
            carried_mm2: 0.0,
            member_length_mm: length,
        };
        let extents =
            match contact::admit(&site, &policy, seated_cos, seated_length, 0.0, false, false) {
                Ok(extents) => extents,
                // An inadmissible rung is DROPPED, exactly like the length and
                // breach filters above -- an ordinary candidate rejection, never a
                // hard failure (OD-4). Under the retired floored formula these rows
                // were instead admitted unconditionally: the floor made the mask
                // smaller and the check stricter, and any rung whose insets
                // over-masked the segment took `seg_clear_one_excluding`'s INFINITY
                // trivially-clear branch.
                Err(_) => continue,
            };
        crate::census_record!(crate::census::record_rung(
            abs_cos_raw,
            extents.seated_abs_cos,
            extents.seated_length_mm,
            radius.get(),
            extents.mask_extent_mm,
        ));
        // PR-8/OD-9 at the SEAT (F3). One row per surviving rung candidate,
        // gathered here and handed to ONE batched attestation below. The seat
        // is `seated_point`, the contact end is `Rootward`, and the OD-9 probe
        // steps one process resolution OUTWARD from the UNSEATED surface point
        // along the same normal `seat_endpoint` used to push the seat inward --
        // `p_contact = p_surface - normal * h` there, so `+ normal * res` here
        // is the outward sense by the same construction.
        seat_rows.push(designed_embed::SeatAttestationRow {
            p0_mm: p0,
            p1_mm: seated_point,
            r0_mm: radius.get(),
            r1_mm: radius.get(),
            end: ContactEnd::Rootward,
            exterior_probe: std::array::from_fn(|axis| {
                projected.point[axis] + projected.normal[axis] * config.process_resolution_mm
            }),
            // Zero routed load -- the F2 argument above. PR-4 therefore grades
            // nothing here, which is correct for a member with no area demand;
            // PR-8 and OD-9 are what this family is gated on.
            carried_mm2: 0.0,
        });
        // #36 — this rung's seat-zone request, gathered here and frozen in ONE
        // batch below, on the same stage boundary as the seat-attestation batch
        // it rides beside. The band radius is FROZEN at this rung's creation
        // radius: `config.brace_radius_mm` is both `r0` and `r1` for this
        // family, so the contact end's radius and `r_max` are the same number
        // today — and the freeze is what keeps that true if they ever diverge.
        seat_zone_rows.push(projected.seat_face_id.map(|seat_face_id| {
            crate::seat_zone::SeatZoneRequest {
                p0_mm: p0,
                p1_mm: seated_point,
                seat_face_id,
                band_radius_mm: radius.get() + config.standoff_mm,
            }
        }));
        model_candidates.push(PlacementCandidate {
            need_index: projected.raw.need_index,
            site_index: projected.raw.site_index,
            rank: projected.raw.rank,
            kind: BraceAdditionKind::ModelRung,
            // Filled in below, once the whole pass's zones are frozen together.
            seat_zone: None,
            rods: vec![Rod {
                p0,
                p1: seated_point,
                radius,
                inset0: 0.0,
                inset1: extents.mask_extent_mm,
                #[cfg(feature = "census")]
                contact: Some(RodContact {
                    abs_cos_incidence: extents.seated_abs_cos,
                }),
            }],
            // BLOCKER-3.  Printability is evaluated at the UNSEATED projection
            // point, deliberately.
            //
            // `p0 = axis_point_at_z(member, projected.point[2])` returns
            // `projected.point[2]` BIT-EXACTLY as its own z, so this predicate
            // has been identically `true` for every model rung since the family
            // was written -- it constrained nothing.  Comparing against
            // `seated_point[2]` instead would turn that historic tautology into
            // `|normal_z| * h <= HEIGHT_TOLERANCE_MM`, i.e. a brand-new rule
            // deleting every model rung on a non-horizontal face, arriving as an
            // ACCIDENT of PR-1 seating rather than as a designed, measured
            // physics rule.  Under OD-8 a family-killing restriction may not
            // arrive that way: PR-3 is the explicit, physics-derived incidence
            // gate for this family, and it already refuses the rungs that cannot
            // emerge from their own contact mask.
            //
            // The comparison is retained rather than replaced by `true` so the
            // structure survives if `p0`'s derivation ever stops pinning z.
            printable: (p0[2] - projected.point[2]).abs() <= HEIGHT_TOLERANCE_MM,
        });
    }

    // F3 -- the model-rung family's PR-8 attestation and OD-9 exterior screen,
    // in ONE batch for the whole pass, riding the same stage boundary as the
    // `project` and `sphere_breach` batches above. Cost is a fixed pair of
    // native calls per brace pass, independent of the candidate count; a
    // per-candidate call here would be the batch-first defect.
    //
    // A refused rung is DROPPED, which is this family's form of quarantine
    // (OD-4/PR-9): the candidate never entered the forest, so there is no
    // placed member to quarantine and nothing to fail. It loses its tournament
    // entry and the pass continues.
    let attestation = attest_model_rung_seats(surface, &seat_rows, config)?;
    // **#36 creation site 2 of 4 — freeze this pass's rung seat zones.**
    //
    // ONE band call plus a parallel flood fill for the whole pass, on the same
    // stage boundary as the attestation batch above. The rows are frozen for
    // every candidate, including the ones the attestation is about to drop:
    // splitting the batch on the attestation verdict would price a second
    // native call to save a few tens of faces, and the dropped candidates are
    // discarded one line later anyway.
    let zone_requests = seat_zone_rows.iter().flatten().copied().collect::<Vec<_>>();
    let mut frozen =
        crate::seat_zone::freeze_production_seat_zones(surface, topology, &zone_requests)
            .map_err(|error| BraceError::SeatZone(error.to_string()))?
            .into_iter();
    for ((mut candidate, attested), request) in model_candidates
        .into_iter()
        .zip(&attestation.attested_by_row)
        .zip(&seat_zone_rows)
    {
        // `flatten()` above kept the requests in row order, so the same walk in
        // the same order re-pairs each zone with the candidate that asked for
        // it. A row whose projection landed off the mesh asked for nothing and
        // stays unfrozen — graded against the whole surface.
        if request.is_some() {
            candidate.seat_zone = frozen.next();
        }
        if *attested {
            candidates.push(candidate);
        }
    }
    debug_assert!(
        frozen.next().is_none(),
        "every frozen seat zone must be claimed by the candidate that requested it"
    );
    candidates
        .sort_by_key(|candidate| (candidate.need_index, candidate.site_index, candidate.rank));
    Ok(CandidateGather {
        candidates,
        attestation_digest: attestation.digest,
    })
}

/// The model-rung family's whole-pass seat attestation.
struct ModelRungAttestation {
    /// Row-aligned with the `seats` slice: `true` where the seat attested.
    attested_by_row: Vec<bool>,
    digest: u64,
}

/// PR-8/OD-9 for every model-rung candidate in one pass, in exactly two batched
/// native calls (delegated to `attest_seats_batch`).
///
/// The claimed extent is ONE PROCESS RESOLUTION, the same choice PR-8 made for
/// the stub family and for the same reason: attestation asks whether a
/// renderable interface EXISTS, and the resolvability floor is one layer.
/// Demanding the full PR-1 seat depth of every rim ray would conflate
/// attestation with the mask-validity proof, which `validate_members_batch`
/// enforces separately and TOTALLY.
///
/// A batch-level integrity failure (starved admission, kernel refusal,
/// malformed row set) is an error, because nothing can be concluded about any
/// rung from it. Per-row geometry verdicts are values, never errors -- OD-4.
fn attest_model_rung_seats(
    surface: &NativeSurface,
    seats: &[designed_embed::SeatAttestationRow],
    config: &BraceConfig,
) -> Result<ModelRungAttestation, BraceError> {
    let extent = designed_embed::PositiveFiniteMm::new(config.process_resolution_mm)
        .map_err(|_| BraceError::InvalidGeometry)?;
    if seats.is_empty() {
        return Ok(ModelRungAttestation {
            attested_by_row: Vec::new(),
            digest: designed_embed::seat_attestation_digest(
                designed_embed::SEAT_TAG_BRACE_MODEL_RUNG,
                extent,
                seats,
                &[],
            ),
        });
    }
    // Sampling resolution is tied to the extent being proven, exactly as
    // `initial.rs` does it: the majority step must be able to RESOLVE the OD-4
    // margin, so it is one tenth of the claimed extent.
    let majority_step = designed_embed::PositiveFiniteMm::new(extent.get() / 10.0)
        .map_err(|_| BraceError::InvalidGeometry)?;
    let limits = designed_embed::EndpointMaskValidationLimits::for_batch(
        surface,
        seats.len(),
        majority_step,
        MODEL_RUNG_ATTESTATION_SAMPLES_PER_SUBSEGMENT,
    )
    .map_err(|error| BraceError::NativeQuery(error.to_string()))?;
    let policy = config.contact_policy(contact::ClearanceStage::Final);
    let outcomes = designed_embed::attest_seats_batch(surface, seats, extent, limits, &policy)
        .map_err(|error| BraceError::NativeQuery(error.to_string()))?;
    let mut attested_by_row = vec![false; seats.len()];
    for outcome in &outcomes {
        if outcome.member >= seats.len() {
            return Err(BraceError::InvalidGeometry);
        }
        attested_by_row[outcome.member] = outcome.verdict.is_ok();
        crate::census_record!(crate::census::record_seat_attestation(
            "brace_model_rung",
            outcome.engagement.centreline,
            outcome.engagement.rim_engaged,
            outcome.engagement.rim_total,
            outcome.attested_radius_mm,
            outcome.verdict.is_err(),
            outcome.area_deficit.is_some(),
        ));
    }
    Ok(ModelRungAttestation {
        attested_by_row,
        digest: designed_embed::seat_attestation_digest(
            designed_embed::SEAT_TAG_BRACE_MODEL_RUNG,
            extent,
            seats,
            &outcomes,
        ),
    })
}

/// Matches `designed_embed::ENDPOINT_VALIDATION_OFFSET_RAYS`, for the same
/// reason `initial.rs` matches it: the witness needs the eight rim rays and the
/// centreline resolved at the density it samples them.
const MODEL_RUNG_ATTESTATION_SAMPLES_PER_SUBSEGMENT: usize = 8;

fn retain_projected_model_survivors(
    candidates: &mut Vec<ProjectedModelCandidate>,
    breached: &[bool],
    cap: usize,
) {
    let mut surviving_counts = BTreeMap::<(usize, usize), usize>::new();
    let mut index = 0;
    candidates.retain(|candidate| {
        let keep = if breached.get(index).copied().unwrap_or(false) {
            false
        } else {
            let count = surviving_counts
                .entry((candidate.raw.need_index, candidate.raw.site_index))
                .or_default();
            let within_cap = *count < cap;
            *count += 1;
            within_cap
        };
        index += 1;
        keep
    });
}

fn evaluate_candidates(
    surface: &NativeSurface,
    candidates: &[PlacementCandidate],
    config: &BraceConfig,
    penetration_limits: PenetrationBatchLimits,
) -> Result<Vec<bool>, BraceError> {
    let mut p0 = Vec::new();
    let mut p1 = Vec::new();
    let mut r0 = Vec::new();
    let mut r1 = Vec::new();
    let mut ranges = Vec::with_capacity(candidates.len());
    // **#36 step 5 — one frozen exclusion set per ROD, in row order.**
    //
    // A model rung's zone was frozen in `gather_candidates` from that rung's own
    // creation radius and seat face; a cross-link declares no contact and
    // forgives nothing. The row set the tournament grades and the row set the
    // mint installs are therefore the same face sets, by construction rather
    // than by re-derivation.
    let mut excluded: Vec<&[u32]> = Vec::new();
    const EMPTY: &[u32] = &[];
    #[cfg(feature = "census")]
    let mut census_cos_class = Vec::new();
    for candidate in candidates {
        let start = p0.len();
        for rod in &candidate.rods {
            p0.push(rod.p0);
            p1.push(rod.p1);
            r0.push(rod.radius.get());
            r1.push(rod.radius.get());
            excluded.push(
                candidate
                    .seat_zone
                    .as_ref()
                    .map_or(EMPTY, |record| record.zone().as_ref()),
            );
            // Read the incidence DIRECTLY off the rod. It used to be
            // reconstructed by inverting `embed_inset_mm`; that inverse does
            // not exist for the PR-2 mask, and since this census is the OD-8
            // delta instrument, inverting a retired formula would have
            // invalidated the very measurement the ruling requires.
            crate::census_record!(census_cos_class.push(match rod.contact {
                Some(contact) =>
                    if contact.abs_cos_incidence <= 0.5 + 1e-9 {
                        1
                    } else {
                        2
                    },
                None => 0,
            }));
        }
        ranges.push(start..p0.len());
    }
    // #36 step 5 — the FULL axis against surface-minus-Z. `rod.inset0/inset1`
    // stay populated with the PR-2 mask because `candidate_revalidation` hands
    // them to `axis_illegal_rows`, which is the one screen where a length
    // measured from the seat is still the right shape (design §5, finding D).
    let zero = vec![0.0; p0.len()];
    // The insets the batch ACTUALLY receives, which are now zero — the SAME
    // vector the call below is handed, so the census cannot drift from it.
    // `SegRow` is documented as one row handed to `segment_clearance`;
    // recording `inset0`/`inset1` here would describe a call this site no
    // longer makes, which is the deviation §11.4 forbids.
    crate::census_record!(crate::census::record_segment_batch(
        "brace_candidate_clearance",
        &p0,
        &p1,
        &zero,
        &zero,
        &census_cos_class,
    ));
    // PLACEMENT (ruling R-1495, 2026-08-22). This grades a per-`(need, site)`
    // candidate TOURNAMENT: a row it drops loses a tournament entry, and no
    // model is refused here — `Unplaceable` is an accepted termination
    // (`runner.rs`). So the demand is the pruning `STANDOFF_MM`, and
    // prune-harder-than-accept is the intended two-tier asymmetry, not a
    // disagreement with the final pass.
    let placement_gap_mm = config
        .contact_policy(contact::ClearanceStage::Placement)
        .demanded_gap_mm;
    let clearances = if p0.is_empty() {
        Vec::new()
    } else {
        surface
            .segment_clearance_excluding(
                &p0,
                &p1,
                &r0,
                &r1,
                placement_gap_mm,
                &zero,
                &zero,
                &excluded,
            )
            .map_err(|error| BraceError::NativeQuery(error.to_string()))?
            .0
    };
    let viable = candidate_revalidation(
        surface,
        candidates,
        &ranges,
        &clearances,
        config,
        penetration_limits,
    )?;
    crate::census_record!(if crate::census::enabled() {
        let mut outcomes = Vec::new();
        for ((candidate, range), is_viable) in candidates.iter().zip(&ranges).zip(&viable) {
            if candidate.kind != BraceAdditionKind::ModelRung {
                continue;
            }
            for (rod, index) in candidate.rods.iter().zip(range.clone()) {
                let length = norm(sub(rod.p1, rod.p0));
                // Direct read, not an inverse of the retired formula.
                let cosine = rod.contact.map_or(f64::NAN, |c| c.abs_cos_incidence);
                outcomes.push(crate::census::RungOutcome {
                    cos_used: cosine,
                    length_mm: length,
                    inset1_mm: rod.inset1,
                    clearance_mm: clearances.get(index).copied().unwrap_or(f64::NAN),
                    over_masked: length >= 1e-9 && (length - rod.inset1) - rod.inset0 <= 1e-9,
                    viable: *is_viable,
                });
            }
        }
        crate::census::record_rung_outcomes(outcomes.into_iter());
    });
    Ok(viable)
}

fn candidate_revalidation(
    surface: &NativeSurface,
    candidates: &[PlacementCandidate],
    ranges: &[std::ops::Range<usize>],
    clearances: &[f64],
    config: &BraceConfig,
    penetration_limits: PenetrationBatchLimits,
) -> Result<Vec<bool>, BraceError> {
    let rods = candidates
        .iter()
        .flat_map(|candidate| candidate.rods.iter())
        .collect::<Vec<_>>();
    let p0 = rods.iter().map(|rod| rod.p0).collect::<Vec<_>>();
    let p1 = rods.iter().map(|rod| rod.p1).collect::<Vec<_>>();
    let radii = rods.iter().map(|rod| rod.radius.get()).collect::<Vec<_>>();
    let inset0 = rods.iter().map(|rod| rod.inset0).collect::<Vec<_>>();
    let inset1 = rods.iter().map(|rod| rod.inset1).collect::<Vec<_>>();
    let illegal_rods = axis_illegal_rows(
        surface,
        &p0,
        &p1,
        &radii,
        &radii,
        &inset0,
        &inset1,
        clearances,
        config,
        penetration_limits,
    )?;
    Ok(ranges
        .iter()
        .enumerate()
        .map(|(candidate_index, range)| {
            candidates[candidate_index].printable
                && range.clone().all(|rod_index| {
                    clearances[rod_index] >= 0.0 && !illegal_rods.contains(&rod_index)
                })
        })
        .collect())
}

#[allow(clippy::too_many_arguments)]
fn axis_illegal_rows(
    surface: &NativeSurface,
    p0: &[[f64; 3]],
    p1: &[[f64; 3]],
    r0: &[f64],
    r1: &[f64],
    inset0: &[f64],
    inset1: &[f64],
    clearances: &[f64],
    config: &BraceConfig,
    penetration_limits: PenetrationBatchLimits,
) -> Result<BTreeSet<usize>, BraceError> {
    let midpoint_rows = p0
        .iter()
        .zip(p1)
        .enumerate()
        .filter(|(index, _)| clearances[*index] >= 0.1)
        .map(|(index, (start, end))| {
            (
                index,
                std::array::from_fn(|axis| 0.5 * (start[axis] + end[axis])),
            )
        })
        .collect::<Vec<_>>();
    let midpoint_inside = surface.inside(
        &midpoint_rows
            .iter()
            .map(|(_, point)| *point)
            .collect::<Vec<_>>(),
    );
    let inside_by_rod = midpoint_rows
        .into_iter()
        .zip(midpoint_inside)
        .map(|((index, _), inside)| (index, inside))
        .collect::<BTreeMap<_, _>>();
    let interval_indices = (0..p0.len())
        .filter(|index| {
            clearances[*index] >= 0.0
                && (clearances[*index] < 0.1 || inside_by_rod.get(index).copied().unwrap_or(false))
        })
        .collect::<Vec<_>>();
    if interval_indices.is_empty() {
        return Ok(BTreeSet::new());
    }
    let intervals = surface.penetration_intervals_batch_bounded(
        &interval_indices
            .iter()
            .map(|index| p0[*index])
            .collect::<Vec<_>>(),
        &interval_indices
            .iter()
            .map(|index| p1[*index])
            .collect::<Vec<_>>(),
        &interval_indices
            .iter()
            .map(|index| r0[*index])
            .collect::<Vec<_>>(),
        &interval_indices
            .iter()
            .map(|index| r1[*index])
            .collect::<Vec<_>>(),
        8,
        0.12,
        80,
        config.min_illegal_len_mm,
        false,
        penetration_limits,
    )?;
    Ok(interval_indices
        .into_iter()
        .zip(intervals)
        .filter_map(|(index, intervals)| {
            let length = norm(sub(p1[index], p0[index]));
            let lo = inset0[index];
            let hi = length - inset1[index];
            (hi - lo > HEIGHT_TOLERANCE_MM
                && intervals.iter().any(|interval| {
                    interval.s_b.min(hi) - interval.s_a.max(lo) > config.min_illegal_len_mm
                }))
            .then_some(index)
        })
        .collect())
}

fn cross_link_candidates(
    forest: &Forest,
    host: MemberKey,
    z: f64,
    rung_index: usize,
    config: &BraceConfig,
    cap: f64,
) -> Result<Vec<(f64, MemberKey)>, BraceError> {
    let member = forest.member(host).ok_or(BraceError::MissingMember(host))?;
    let (host_lo, host_hi) = z_extent(member);
    if z < host_lo - HEIGHT_TOLERANCE_MM || z > host_hi + HEIGHT_TOLERANCE_MM {
        return Ok(Vec::new());
    }
    let host_point = axis_point_at_z(member, z);
    let braces = forest.braces().into_iter().collect::<BTreeSet<_>>();
    let mut out = Vec::new();
    for neighbor in forest.live_members() {
        if neighbor == host || braces.contains(&neighbor) {
            continue;
        }
        let other = forest
            .member(neighbor)
            .ok_or(BraceError::MissingMember(neighbor))?;
        if matches!(other.kind(), MemberKind::Stub) {
            continue;
        }
        let (lo, hi) = z_extent(other);
        if z < lo - HEIGHT_TOLERANCE_MM || z > hi + HEIGHT_TOLERANCE_MM {
            continue;
        }
        let other_point = axis_point_at_z(other, z);
        let dx = other_point[0] - host_point[0];
        let dy = other_point[1] - host_point[1];
        let distance = dx.hypot(dy);
        if distance <= config.min_cross_link_xy_mm || distance > cap {
            continue;
        }
        if z + distance > hi + HEIGHT_TOLERANCE_MM {
            continue;
        }
        let twin = distance <= config.twin_single_xy_mm;
        if (!twin || !rung_index.is_multiple_of(2)) && z + distance > host_hi + HEIGHT_TOLERANCE_MM
        {
            continue;
        }
        out.push((distance, neighbor));
    }
    out.sort_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
    });
    Ok(out)
}

fn reach_tiers_mm(config: &BraceConfig) -> Vec<f64> {
    let mut factors = [config.reach_tier_base_factor, config.reach_max_factor]
        .into_iter()
        .filter(|factor| {
            *factor > 1.0 + HEIGHT_TOLERANCE_MM
                && *factor <= config.reach_max_factor + HEIGHT_TOLERANCE_MM
        })
        .collect::<Vec<_>>();
    factors.sort_by(f64::total_cmp);
    factors.dedup_by(|left, right| left.to_bits() == right.to_bits());
    factors
        .into_iter()
        .map(|factor| config.neighbor_max_xy_mm * factor)
        .collect()
}

fn cross_link_rods(
    forest: &Forest,
    host: MemberKey,
    neighbor: MemberKey,
    z: f64,
    distance: f64,
    rung_index: usize,
    config: &BraceConfig,
) -> Result<Vec<Rod>, BraceError> {
    let host_member = forest.member(host).ok_or(BraceError::MissingMember(host))?;
    let neighbor_member = forest
        .member(neighbor)
        .ok_or(BraceError::MissingMember(neighbor))?;
    let host_lo = axis_point_at_z(host_member, z);
    let neighbor_lo = axis_point_at_z(neighbor_member, z);
    let host_hi = axis_point_at_z(host_member, z + distance);
    let neighbor_hi = axis_point_at_z(neighbor_member, z + distance);
    let compatibility = LegacyCompatibility::o10_live_current_support_and_raft();
    let raw = deflection_radius(distance * 2.0_f64.sqrt(), 45.0, &compatibility)?;
    let make = |p0, p1, paired: bool| -> Result<Rod, BraceError> {
        let span = MemberSpan::new(finite3(p0)?, finite3(p1)?)?;
        let radius = if paired {
            let own = compatibility
                .deflection_radius_for_geometry(span.length_mm(), span.lean_angle_deg())?;
            PositiveFinite::new(
                config
                    .brace_radius_mm
                    .get()
                    .max(own.get())
                    .max(raw * CROSS_LINK_DUTY_FACTOR),
            )
        } else {
            // Frozen twin parity deliberately uses the 45-degree construction
            // estimate only; the real-endpoint own floor belongs to X pairs.
            PositiveFinite::new(config.brace_radius_mm.get().max(raw))
        }
        .map_err(|_| BraceError::InvalidGeometry)?;
        Ok(Rod {
            p0,
            p1,
            radius,
            inset0: 0.0,
            inset1: 0.0,
            // Cross-links join two supports; neither end touches the model.
            #[cfg(feature = "census")]
            contact: None,
        })
    };
    if distance <= config.twin_single_xy_mm {
        return if rung_index.is_multiple_of(2) {
            Ok(vec![make(host_lo, neighbor_hi, false)?])
        } else {
            Ok(vec![make(neighbor_lo, host_hi, false)?])
        };
    }
    Ok(vec![
        make(host_lo, neighbor_hi, true)?,
        make(neighbor_lo, host_hi, true)?,
    ])
}

fn model_rung_anchor_centroids(
    surface: &NativeSurface,
    forest: &Forest,
    host: MemberKey,
    z: f64,
    config: &BraceConfig,
) -> Result<Vec<[f64; 3]>, BraceError> {
    let member = forest.member(host).ok_or(BraceError::MissingMember(host))?;
    let axis = axis_point_at_z(member, z);
    let exterior = surface.exterior_face_mask();
    let mut rows = surface
        .face_ids()
        .iter()
        .enumerate()
        .filter(|(selected, _)| exterior.get(*selected).copied().unwrap_or(false))
        .filter_map(|(_, global)| {
            let point = surface
                .geometry()
                .centroids()
                .get(*global as usize)
                .copied()?;
            let distance = norm(sub(point, axis));
            (distance <= config.model_brace_max_length_mm + HEIGHT_TOLERANCE_MM
                && (point[2] - z).abs() <= config.model_rung_z_tolerance_mm + HEIGHT_TOLERANCE_MM)
                .then_some((distance, *global, point))
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
    });
    Ok(rows.into_iter().map(|(_, _, point)| point).collect())
}

fn rods_printable(rods: &[Rod]) -> Result<bool, BraceError> {
    let policy = DiagnosticMemberSizingPolicy::current()?;
    for rod in rods {
        let span = MemberSpan::new(finite3(rod.p0)?, finite3(rod.p1)?)?;
        if !diagnostic_within_printability_cone(span, rod.radius, &policy).diagnostic() {
            return Ok(false);
        }
    }
    Ok(true)
}

/// PR-9 — the negative-clearance rows, split by whether they are refusable.
#[derive(Clone, Debug, Default)]
struct ExactClearanceNegatives {
    /// Members reading negative clearance that declare NO model contact. There
    /// is no candidate to reject here: the controller placed geometry inside
    /// the mini, so this stays `BraceError::ClearanceViolation`.
    pipeline_violations: usize,
    /// Members reading negative clearance that DO declare a model contact.
    /// These are refused candidates and route to quarantine (OD-4).
    contact_refusals: Vec<MemberKey>,
    /// **Refusal accounting (Craig's rider on the seal-semantics lane,
    /// 2026-08-23).** Every negative row with the PROVENANCE of its number,
    /// keyed by member — both classes above.
    ///
    /// `exact_clearance_rows` already computes a real per-arm clearance in mm
    /// and, until this field existed, only the SIGN of it survived: a run
    /// could say "one arm was refused" and never say whether that arm was
    /// 0.04 mm short of the demanded gap or 4 mm inside the model. Those are
    /// different physical situations and the sub-0.3 mm residue is the one
    /// Craig's seal decision waited on.
    ///
    /// The rows carry the axis-transit flag as well as the distance, because
    /// the two refusals are not the same fact and a magnitude read off a
    /// transit sentinel is not a measurement — see [`RefusalGapRow`]. Read by
    /// no branch.
    negative_rows: BTreeMap<MemberKey, ExactClearanceRow>,
}

impl ExactClearanceNegatives {
    const fn is_clean(&self) -> bool {
        self.pipeline_violations == 0 && self.contact_refusals.is_empty()
    }
}

/// One negative exact-clearance row, as the FINAL pass measured it.
///
/// Disclosure only. Nothing branches on any field here; the verdict is taken
/// from the sign of `gap_mm`, exactly as it was before this record existed.
///
/// **The two refusals are different physical facts and this record keeps them
/// apart.** `exact_clearance_rows` produces one signed number per member, and
/// it is written by TWO paths:
///
/// 1. the DISTANCE path — `clr = best_axis_to_surface_distance − (r_max +
///    FINAL_CLEARANCE_GAP_MM)`, so a negative value is a real shortfall in
///    millimetres against the 0.300 mm demand;
/// 2. the AXIS-TRANSIT path — a row the distance test found LEGAL
///    (`clearance >= 0.0`) whose axis nonetheless passes THROUGH material away
///    from its seat is stamped `min(clearance, −min_illegal_len_mm)`, i.e.
///    exactly `−0.050 mm`, so that a single sign carries the verdict.
///
/// A reader who takes a transit row's `−0.050` for a gap reads a sentinel as a
/// measurement. So `axis_transit_illegal` says which path wrote the row, and
/// `distance_clearance_mm` carries the distance path's own value BEFORE the
/// stamp — for a transit row that number is ≥ 0, which is the honest statement
/// that this arm did not fail on distance at all.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RefusalGapRow {
    pub(crate) member: MemberKey,
    /// The verdict number the engine acts on. Negative condemns.
    pub(crate) gap_mm: f64,
    /// MEASURED, and the distance path's own value before any transit stamp:
    /// `distance − (r_max + FINAL_CLEARANCE_GAP_MM)` in millimetres. Negative
    /// means the member is that far short of the 0.300 mm demanded gap; a
    /// value ≥ 0 on a refused row means the refusal was NOT about distance.
    pub(crate) distance_clearance_mm: f64,
    /// `true` when the axis-transit screen wrote this row's verdict — the
    /// member's axis passes through material away from its seat. `gap_mm` is
    /// then the `−min_illegal_len_mm` sentinel and carries no distance.
    pub(crate) axis_transit_illegal: bool,
    /// The radii the resize assigned to this member, in millimetres — the
    /// deciding input the refutation proved cannot be predicted at candidacy.
    pub(crate) terminal_radius_mm: f64,
    pub(crate) rootward_radius_mm: f64,
    /// `true` when the member DECLARES a model contact, so its refusal is an
    /// ordinary candidate rejection (OD-4); `false` when it is an unrefusable
    /// pipeline violation.
    pub(crate) refusable: bool,
}

impl RefusalGapRow {
    /// One row, rendered so the two refusal kinds cannot be confused.
    ///
    /// A distance refusal prints the shortfall it measured. An axis-transit
    /// refusal prints `axis-transit` and the distance clearance it PASSED,
    /// because printing its `−0.050` verdict as a gap would be presenting a
    /// sentinel as a measurement. A `!` marks an unrefusable violation.
    fn render(&self) -> String {
        let key = format!(
            "slot{}g{}{}",
            self.member.slot(),
            self.member.generation(),
            if self.refusable { "" } else { "!" },
        );
        if self.axis_transit_illegal {
            format!(
                "{key}=axis-transit(distance_clearance={:+.6}mm)@r{:.6}/{:.6}mm",
                self.distance_clearance_mm, self.terminal_radius_mm, self.rootward_radius_mm,
            )
        } else {
            format!(
                "{key}=distance{:+.6}mm@r{:.6}/{:.6}mm",
                self.gap_mm, self.terminal_radius_mm, self.rootward_radius_mm,
            )
        }
    }
}

/// **The refusal-accounting rider.** What the final bracing pass measured, in
/// counts AND in millimetres, so a run can be read for the size of its
/// clearance failures rather than only for their existence.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FinalPassRefusalAccounting {
    /// How many negative rows DECLARE a model contact — the refused arms.
    pub(crate) refused_arms: usize,
    /// How many negative rows declare none — unrefusable violations.
    pub(crate) pipeline_violations: usize,
    /// Every negative row, ascending by member key.
    pub(crate) rows: Vec<RefusalGapRow>,
}

impl FinalPassRefusalAccounting {
    /// Read the accounting off one pass's negatives. Batched: the gaps were
    /// measured by the single `exact_clearance_rows` call this pass already
    /// made, and nothing here re-enters the kernel.
    fn read(forest: &Forest, negatives: &ExactClearanceNegatives) -> Self {
        let refused = negatives
            .contact_refusals
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let rows = negatives
            .negative_rows
            .values()
            .map(|row| {
                let (terminal_radius_mm, rootward_radius_mm) =
                    forest
                        .member(row.member)
                        .map_or((f64::NAN, f64::NAN), |member| {
                            (
                                member.terminal_radius_mm().get().get(),
                                member.rootward_radius_mm().get().get(),
                            )
                        });
                RefusalGapRow {
                    member: row.member,
                    gap_mm: row.verdict_mm,
                    distance_clearance_mm: row.distance_clearance_mm,
                    axis_transit_illegal: row.axis_transit_illegal,
                    terminal_radius_mm,
                    rootward_radius_mm,
                    refusable: refused.contains(&row.member),
                }
            })
            .collect::<Vec<_>>();
        Self {
            refused_arms: negatives.contact_refusals.len(),
            pipeline_violations: negatives.pipeline_violations,
            rows,
        }
    }

    /// Stream the accounting while the run is happening.
    ///
    /// Emitted only when the final pass measured at least one negative row.
    /// A pass with nothing negative has no refusal to hide, and the brace
    /// fixed point runs once per repair trial as well as at attempt 0 — 92
    /// trials MEASURED on `zombie-3-a` — so a line per clean pass would bury
    /// the lines that carry a verdict.
    fn disclose(&self, iteration: usize) {
        if self.rows.is_empty() {
            return;
        }
        let transit = self
            .rows
            .iter()
            .filter(|row| row.axis_transit_illegal)
            .count();
        let rows = self
            .rows
            .iter()
            .map(RefusalGapRow::render)
            .collect::<Vec<_>>()
            .join(" ");
        crate::coverage_repair::emit_run_line(&format!(
            "brace: final-pass refusals pass={iteration} refused_arms={} \
             pipeline_violations={} distance={} axis_transit={} rows: {rows}",
            self.refused_arms,
            self.pipeline_violations,
            self.rows.len() - transit,
            transit,
        ));
    }
}

/// One terminal the ROUTING SEAL named, with the disposition it named.
///
/// Stage A is handed every sealed disposition rather than the `Routed` ones
/// alone. The tripwire it stands in front of —
/// `runner::finalized_unrouted_terminals` — refuses every pair outside the
/// three it admits plus a ledger cardinality mismatch, so classifying on the
/// whole record is the right shape and costs nothing (design §4 Stage A(a)).
/// It is NOT claimed to retire an error that fires today: `children_of`
/// filters to live members, so a cascade cannot reach an inactive one, and a
/// directly named inactive key still refuses inside the planner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SealedTerminalDisposition {
    pub(crate) terminal_id: u32,
    pub(crate) kind: FinalizedDispositionKind,
}

/// One sealed terminal the third-pass sweep would have disturbed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisturbedSealedTerminal {
    pub(crate) terminal_id: u32,
    pub(crate) sealed_kind: FinalizedDispositionKind,
    /// The terminal's registered contact member, taken from the one map that
    /// binds a member to a terminal.
    pub(crate) terminal_member: MemberKey,
    /// Measured-illegal members on this terminal's OWN route that the terminal
    /// itself owns — the arms ITS tournament chose. A no-good is only ever
    /// recorded for one of these (design §1.3).
    pub(crate) own_chosen_arms: Vec<MemberKey>,
    /// Measured-illegal members on its route that it does NOT own: somebody
    /// else's arm, with this terminal's stub hanging off it. **A cascade
    /// victim steers nothing.** Punishing it would cost a legal arm per pass
    /// for another member's failure, and retire it in the end with no arm of
    /// its own ever measured illegal.
    pub(crate) cascade_carriers: Vec<MemberKey>,
}

impl DisturbedSealedTerminal {
    const fn own_arm_condemned(&self) -> bool {
        !self.own_chosen_arms.is_empty()
    }
}

/// **The WHOLE mismatch class one abandoned pass names.**
///
/// The measurement lane found `finalized_unrouted_terminals` returns on the
/// FIRST bad row, so a run learned about one terminal at a time. Stage A does
/// not repeat that: every sealed terminal the sweep would have disturbed is
/// carried here, each with its own arms and its cascade carriers, together
/// with every member the final pass measured illegal and by how much.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SealedCondemnationClass {
    /// Every member the final pass measured illegal, with its true gap in
    /// millimetres and the radii the resize assigned.
    pub(crate) measured_refusals: Vec<RefusalGapRow>,
    /// Every disturbed sealed terminal, ascending by terminal id.
    pub(crate) disturbed: Vec<DisturbedSealedTerminal>,
}

impl SealedCondemnationClass {
    /// Does the class contain a sealed-`Routed` terminal whose OWN chosen arm
    /// was measured illegal? That is the steerable case; anything else is a
    /// pass that has learned nothing it can steer with.
    fn has_steerable_sealed_route(&self) -> bool {
        self.disturbed.iter().any(|terminal| {
            terminal.sealed_kind == FinalizedDispositionKind::Routed && terminal.own_arm_condemned()
        })
    }

    fn disclose(&self) {
        crate::coverage_repair::emit_run_line(&format!("brace: {self}"));
    }
}

impl std::fmt::Display for SealedCondemnationClass {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let key = |member: &MemberKey| format!("slot{}g{}", member.slot(), member.generation());
        let members = |keys: &[MemberKey]| {
            if keys.is_empty() {
                "none".to_owned()
            } else {
                keys.iter().map(key).collect::<Vec<_>>().join(",")
            }
        };
        write!(
            formatter,
            "sealed terminals disturbed={} measured refusals=[{}] class=[{}]",
            self.disturbed.len(),
            self.measured_refusals
                .iter()
                .map(RefusalGapRow::render)
                .collect::<Vec<_>>()
                .join(" "),
            self.disturbed
                .iter()
                .map(|terminal| format!(
                    "terminal {} sealed={:?} stub={} own_arms={} cascade_carriers={}",
                    terminal.terminal_id,
                    terminal.sealed_kind,
                    key(&terminal.terminal_member),
                    members(&terminal.own_chosen_arms),
                    members(&terminal.cascade_carriers),
                ))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }
}

/// **Stage A.** Plan the condemnation set read-only, intersect it with the
/// sealed record, and say whether the sweep may run.
///
/// `Ok(None)` means the intersection is empty: no terminal the sealed record
/// names is disturbed, so the sweep applies exactly as it does today. This is
/// the refutation's own carve-out — brace rungs and model-anchor arms whose
/// condemnation disturbs no contact accounting. **Their behaviour does not
/// move.**
///
/// `Ok(Some(class))` means the sweep would have flipped a sealed terminal's
/// live state under a record that cannot be rewritten. Quarantining then
/// mints the `Routed`/`Quarantined` pair the accounting forbids, and the run
/// dies two stages later with `TerminalEvidenceMismatch` — an error that reads
/// as a broken engine rather than as the physics it is. The caller abandons
/// instead, and the class says which terminals, which arms, and how many
/// millimetres inside the model.
///
/// **Attribution, and why it is not a key comparison.** A member belongs to a
/// terminal's own route when it carries that terminal's provenance:
/// `Forest::register_terminal` refuses a stub whose owner is not
/// `Owner::Contact(terminal)`, and `cco` mints every member of a terminal's
/// route with that same `connector_owner`. A bifurcation's UPPER half inherits
/// the HOST's owner instead, which is precisely the cascade-victim case — a
/// stub re-parented onto somebody else's arm.
/// Does this member's subtree carry `terminal` and no other terminal?
///
/// The physical reading of *"somebody else's arm"*: an arm is somebody else's
/// when somebody else hangs off it. One walk of the member's own subtree,
/// checking each key against the stub→terminal map the caller already built —
/// no second authority, and no per-member native call.
///
/// Called only for a member the owner test already rejected, and only on a pass
/// that is refusing anyway, so it is off every successful path.
fn carries_only(
    forest: &Forest,
    terminals_by_member: &BTreeMap<MemberKey, u32>,
    member: MemberKey,
    terminal: u32,
) -> bool {
    let mut frontier = vec![member];
    let mut carries_this = false;
    while let Some(key) = frontier.pop() {
        if let Some(id) = terminals_by_member.get(&key) {
            if *id == terminal {
                carries_this = true;
            } else {
                return false;
            }
        }
        frontier.extend(forest.children_of(key));
    }
    carries_this
}

fn classify_sealed_condemnation(
    forest: &Forest,
    sealed: &[SealedTerminalDisposition],
    negatives: &ExactClearanceNegatives,
    accounting: &FinalPassRefusalAccounting,
) -> Result<Option<SealedCondemnationClass>, BraceError> {
    if sealed.is_empty() || negatives.contact_refusals.is_empty() {
        return Ok(None);
    }
    // (b) The query is an exact PREFIX of the mutation, not a second walk:
    // `quarantine_subtrees` is this call plus a `quarantine` per planned key.
    let order = forest.plan_quarantine_subtrees(&negatives.contact_refusals)?;
    let condemned = order.iter().copied().collect::<BTreeSet<_>>();
    let refused = negatives
        .contact_refusals
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    // (c) Invert the one existing map, once, for the whole batch.
    let terminals_by_member = forest.terminals_by_member();
    let sealed_by_id = sealed
        .iter()
        .map(|row| (row.terminal_id, row.kind))
        .collect::<BTreeMap<_, _>>();

    let mut disturbed = Vec::new();
    for member_key in &order {
        let Some(terminal_id) = terminals_by_member.get(member_key).copied() else {
            continue;
        };
        let Some(sealed_kind) = sealed_by_id.get(&terminal_id).copied() else {
            continue;
        };
        // Walk this terminal's own route rootward through the condemned set.
        // The sweep condemns whole subtrees, so every member between the stub
        // and the refusal key that took it down is in the set; the walk stops
        // at the first member that is not.
        let mut own_chosen_arms = Vec::new();
        let mut cascade_carriers = Vec::new();
        let mut cursor = Some(*member_key);
        while let Some(key) = cursor {
            if !condemned.contains(&key) {
                break;
            }
            if refused.contains(&key) {
                let owned = forest
                    .member(key)
                    .is_some_and(|member| member.owner() == &Owner::Contact(terminal_id));
                // **The owner field is not sufficient, and one measured run
                // proved it.** `cco`'s rewire mints a connector owning
                // `Owner::Contact(root.slot())` — a MEMBER slot in the
                // terminal-id namespace — so a member rewire minted for THIS
                // terminal's own route wears an owner that is not
                // `Contact(terminal)`. MEASURED on the Deinonychus: after one
                // steering pass, terminal 113's route ended at `slot501g0`
                // owned by `Contact(401)`, where 401 is a member slot and no
                // sealed terminal. The owner test called it somebody else's arm
                // and the pass refused with nothing to steer.
                //
                // So the second test is §1.3's own definition, applied to the
                // thing it is actually about: *"its stub re-parented onto
                // SOMEBODY ELSE'S arm"*. An arm is somebody else's when it
                // also carries somebody else. A member whose subtree holds this
                // terminal's stub and no other terminal's cannot be another
                // terminal's arm, whatever its owner field says — it exists to
                // carry this terminal.
                //
                // The victim rule is untouched where it matters. A shared spine
                // carrying `{this, other}` is still a carrier and still steers
                // nothing, which is the case §1.3 spends three paragraphs
                // forbidding the punishment of — and so is every member whose
                // owner honestly names somebody: another sealed terminal, a
                // generated brace rung, a model anchor.
                // **BOTH conditions, and the second one alone is not enough.**
                // An `Owner::Generated` brace rung, or an `Owner::ModelAnchor`,
                // that happens to carry only this terminal is NOT this
                // terminal's chosen arm — its owner is honest provenance and
                // says so. The reclassification below is for exactly one
                // defect: an owner that names no terminal at all.
                let rewire_root_owner = forest.member(key).is_some_and(|member| {
                    matches!(member.owner(), Owner::Contact(id) if !sealed_by_id.contains_key(id))
                });
                let solo = !owned
                    && rewire_root_owner
                    && carries_only(forest, &terminals_by_member, key, terminal_id);
                if owned || solo {
                    own_chosen_arms.push(key);
                } else {
                    cascade_carriers.push(key);
                }
            }
            cursor = forest.parent_of(key);
        }
        disturbed.push(DisturbedSealedTerminal {
            terminal_id,
            sealed_kind,
            terminal_member: *member_key,
            own_chosen_arms,
            cascade_carriers,
        });
    }
    if disturbed.is_empty() {
        return Ok(None);
    }
    disturbed.sort_by_key(|terminal| terminal.terminal_id);
    Ok(Some(SealedCondemnationClass {
        measured_refusals: accounting
            .rows
            .iter()
            .filter(|row| row.refusable)
            .copied()
            .collect(),
        disturbed,
    }))
}

fn exact_clearance_negatives(
    forest: &Forest,
    surface: &NativeSurface,
    config: &BraceConfig,
    penetration_limits: PenetrationBatchLimits,
) -> Result<ExactClearanceNegatives, BraceError> {
    let mut negatives = ExactClearanceNegatives::default();
    for row in exact_clearance_rows_detailed(forest, surface, config, penetration_limits)? {
        if row.verdict_mm >= 0.0 {
            continue;
        }
        negatives.negative_rows.insert(row.member, row);
        let declares_contact = forest
            .member(row.member)
            .is_some_and(|member| member.model_contact_end().is_some());
        if declares_contact {
            negatives.contact_refusals.push(row.member);
        } else {
            negatives.pipeline_violations += 1;
        }
    }
    // `exact_clearance_rows` walks `live_members`, which is already ascending,
    // but sorting is stated rather than inherited so the evidence order cannot
    // drift with an upstream change.
    negatives.contact_refusals.sort_unstable();
    Ok(negatives)
}

fn exact_clearance_violated_members(
    forest: &Forest,
    surface: &NativeSurface,
    config: &BraceConfig,
    penetration_limits: PenetrationBatchLimits,
) -> Result<Vec<MemberKey>, BraceError> {
    let braces = forest.braces().into_iter().collect::<BTreeSet<_>>();
    Ok(
        exact_clearance_rows(forest, surface, config, penetration_limits)?
            .into_iter()
            .filter_map(|(key, value)| (value < 0.0 && !braces.contains(&key)).then_some(key))
            .collect(),
    )
}

fn exact_clearance_rows_detailed(
    forest: &Forest,
    surface: &NativeSurface,
    config: &BraceConfig,
    penetration_limits: PenetrationBatchLimits,
) -> Result<Vec<ExactClearanceRow>, BraceError> {
    let rows = forest
        .live_members()
        .into_iter()
        .filter_map(|key| forest.member(key).map(|member| (key, member)))
        .filter(|(_, member)| !matches!(member.kind(), MemberKind::Stub))
        .collect::<Vec<_>>();
    let p0 = rows
        .iter()
        .map(|(_, member)| member.p0_mm().map(Finite::get))
        .collect::<Vec<_>>();
    let p1 = rows
        .iter()
        .map(|(_, member)| member.p1_mm().map(Finite::get))
        .collect::<Vec<_>>();
    let r0 = rows
        .iter()
        .map(|(_, member)| member.terminal_radius_mm().get().get())
        .collect::<Vec<_>>();
    let r1 = rows
        .iter()
        .map(|(_, member)| member.rootward_radius_mm().get().get())
        .collect::<Vec<_>>();
    // **#36 step 5 — these are the AXIS-TRANSIT masks, not clearance insets.**
    //
    // The PR-2 mask stays computed and stays handed to `axis_illegal_rows`
    // (design §5, reader 2): that screen asks "is the arm passing THROUGH
    // material away from its seat", and there a length measured from the seat
    // is the right shape, because the seat embed IS a length along the axis.
    // Zeroing it there would condemn every seated contact for its own designed
    // embed — challenge finding D, and the prior challenge proved the seated-arm
    // case must stay legal.
    //
    // It no longer reaches the clearance DISTANCE path. That path now tests the
    // full axis and exempts a frozen face SET instead (`excluded` below).
    let mut mask_inset0 = vec![0.0; rows.len()];
    let mut mask_inset1 = vec![0.0; rows.len()];
    #[cfg(feature = "census")]
    let mut census_cos_class = vec![0u8; rows.len()];
    let embedded = rows
        .iter()
        .enumerate()
        .filter_map(|(index, (_, member))| {
            member.model_contact_end().map(|end| {
                let point = match end {
                    ContactEnd::Terminalward => member.p0_mm(),
                    ContactEnd::Rootward => member.p1_mm(),
                }
                .map(Finite::get);
                (index, end, point)
            })
        })
        .collect::<Vec<_>>();
    if !embedded.is_empty() {
        let points = embedded
            .iter()
            .map(|(_, _, point)| *point)
            .collect::<Vec<_>>();
        let (_, _, _, normals) = surface.project(&points);
        for ((index, end, _), normal) in embedded.into_iter().zip(normals) {
            let member = rows[index].1;
            let axis_raw = sub(
                member.p1_mm().map(Finite::get),
                member.p0_mm().map(Finite::get),
            );
            let axis = scale(axis_raw, 1.0 / norm(axis_raw));
            let abs_cos_raw = dot(axis, normal).abs();
            let radius = match end {
                ContactEnd::Terminalward => member.terminal_radius_mm().get().get(),
                ContactEnd::Rootward => member.rootward_radius_mm().get().get(),
            };
            // PR-2 on the geometry the kernel will actually see. These members
            // are already placed, so the observed incidence IS the one the mask
            // must use. The seat depth is RECOMPUTED from the design family
            // plus policy rather than stored on `Member` (OD-3), which is why
            // no digest moves.
            //
            // MAJOR-6. The mask's depth term must reflect this member's ACTUAL
            // seating state, not the seating state of the one family PR-1
            // happens to seat. `contact::seating_of` is the single place that
            // decision is recorded and `contact::placed_mask_inset_mm` is the
            // single statement of the mask, so this site, `cco::clearance_rows`
            // and `rewire::validate_changed_geometry` cannot drift again (#36).
            // PR-7 seats the CCO model anchor's `Rootward` end, and this site
            // picked that up for free when `seating_of` learned the family.
            let policy = config.contact_policy(contact::ClearanceStage::Final);
            let r_max = member
                .terminal_radius_mm()
                .get()
                .get()
                .max(member.rootward_radius_mm().get().get());
            // A degenerate incidence yields no mask. Masking nothing is the
            // conservative disposition here: the row is then fully distance
            // checked rather than exempted, and PR-3/PR-9 refuse it upstream.
            let inset = contact::placed_mask_inset_mm(
                contact::seating_of(member.kind(), end),
                abs_cos_raw,
                radius,
                r_max,
                &policy,
            )
            .unwrap_or(0.0);
            crate::census_record!({
                crate::census::record_embed(
                    abs_cos_raw,
                    abs_cos_raw,
                    radius,
                    inset,
                    norm(axis_raw),
                    member.kind(),
                    end,
                );
                census_cos_class[index] = if abs_cos_raw < 0.5 { 1 } else { 2 };
            });
            match end {
                ContactEnd::Terminalward => mask_inset0[index] = inset,
                ContactEnd::Rootward => mask_inset1[index] = inset,
            }
        }
    }
    // **#36 step 5 — EXEMPT A TARGET, NEVER A LENGTH.**
    //
    // The distance test now runs over the FULL axis (`inset0 = inset1 = 0`)
    // against the whole surface EXCEPT each row's own frozen seat zone. The
    // zone is looked up only for a row that DECLARES a contact: a member with
    // no seat forgives nothing, and reading a stale entry for one would exempt
    // faces it has no right to.
    //
    // A contact-declaring row with no frozen zone gets an empty exclusion and is
    // graded against everything, including its own designed embed. That is the
    // STRICT direction and it is deliberate — a missed creation site shows up as
    // a refusal, never as a silent pass.
    let zero = vec![0.0; rows.len()];
    let empty: &[u32] = &[];
    let excluded = rows
        .iter()
        .map(|(key, member)| {
            member
                .model_contact_end()
                .and_then(|_| forest.seat_zone(*key))
                .map_or(empty, |zone| zone.as_ref())
        })
        .collect::<Vec<_>>();
    crate::census_record!(crate::census::record_segment_batch(
        "brace_exact_member_clearance",
        &p0,
        &p1,
        // The insets the batch ACTUALLY receives, which are now zero. Recording
        // the mask here instead would describe a call this site no longer makes.
        &zero,
        &zero,
        &census_cos_class,
    ));
    // FINAL. This is the verdict that condemns an arm — over contact-declaring
    // members and non-contact members alike — so it grades against the only
    // refusing number in the engine, `FINAL_CLEARANCE_GAP_MM` (Craig's ruling,
    // 2026-08-22). The placement family screens at the larger `STANDOFF_MM`;
    // the refusing demand is deliberately the SMALLER of the two, or a search
    // would admit exactly what this pass then refuses.
    let final_gap_mm = config
        .contact_policy(contact::ClearanceStage::Final)
        .demanded_gap_mm;
    let mut clearance = surface
        .segment_clearance_excluding(&p0, &p1, &r0, &r1, final_gap_mm, &zero, &zero, &excluded)
        .map_err(|error| BraceError::NativeQuery(error.to_string()))?
        .0;
    // The DISTANCE path's own answer, kept before the transit screen can stamp
    // over it. Nothing branches on this copy; it exists so a disclosure cannot
    // present the transit sentinel as a measured shortfall (`RefusalGapRow`).
    let distance_clearance = clearance.clone();
    let mut transit_illegal = BTreeSet::new();
    for index in axis_illegal_rows(
        surface,
        &p0,
        &p1,
        &r0,
        &r1,
        // Finding D: the transit screen KEEPS the mask. Zeroing both would
        // condemn every seated arm for its own embed.
        &mask_inset0,
        &mask_inset1,
        &clearance,
        config,
        penetration_limits,
    )? {
        // Exact-clearance consumers need a single sign verdict. Preserve any
        // harder distance violation and encode belt-only transit as negative.
        clearance[index] = clearance[index].min(-config.min_illegal_len_mm);
        transit_illegal.insert(rows[index].0);
    }
    Ok(rows
        .into_iter()
        .zip(distance_clearance)
        .zip(clearance)
        .map(
            |(((member, _), distance_clearance_mm), verdict_mm)| ExactClearanceRow {
                member,
                verdict_mm,
                distance_clearance_mm,
                axis_transit_illegal: transit_illegal.contains(&member),
            },
        )
        .collect())
}

/// One member's FINAL-pass clearance verdict, with the provenance of the number.
///
/// The engine acts on `verdict_mm` alone and always has. The other two fields
/// exist so a DISCLOSURE cannot present the axis-transit sentinel as a measured
/// distance — see [`RefusalGapRow`].
#[derive(Clone, Copy, Debug, PartialEq)]
struct ExactClearanceRow {
    member: MemberKey,
    verdict_mm: f64,
    distance_clearance_mm: f64,
    axis_transit_illegal: bool,
}

/// The verdict column only — byte-identical to what this function returned
/// before the provenance split, for every caller that only needs the sign.
fn exact_clearance_rows(
    forest: &Forest,
    surface: &NativeSurface,
    config: &BraceConfig,
    penetration_limits: PenetrationBatchLimits,
) -> Result<Vec<(MemberKey, f64)>, BraceError> {
    Ok(
        exact_clearance_rows_detailed(forest, surface, config, penetration_limits)?
            .into_iter()
            .map(|row| (row.member, row.verdict_mm))
            .collect(),
    )
}

fn slender_rungs(
    length_mm: f64,
    lean: f64,
    config: &BraceConfig,
    compatibility: &LegacyCompatibility,
) -> Result<usize, BraceError> {
    for count in 0..=50 {
        let radius = deflection_radius(length_mm / (count + 1) as f64, lean, compatibility)?;
        if radius <= config.max_unbraced_radius_mm {
            return Ok(count);
        }
    }
    Ok(50)
}

fn transient_radius(
    span_mm: f64,
    lean: f64,
    terminals: usize,
    compatibility: &LegacyCompatibility,
) -> Result<f64, BraceError> {
    let exponent = m0_constants()
        .map_err(|error| BraceError::Constants(error.to_string()))?
        .placeholders()
        .transient_shoot_count_exponent()
        .diagnostic()
        .get();
    Ok(deflection_radius(span_mm, lean, compatibility)? * (terminals.max(1) as f64).powf(exponent))
}

fn deflection_radius(
    length_mm: f64,
    lean: f64,
    compatibility: &LegacyCompatibility,
) -> Result<f64, BraceError> {
    let length =
        PositiveFinite::new(length_mm.max(1e-6)).map_err(|_| BraceError::InvalidGeometry)?;
    let lean = LeanAngleDeg::new(Finite::new(lean).map_err(|_| BraceError::InvalidGeometry)?)?;
    Ok(compatibility
        .deflection_radius_for_geometry(length, lean)?
        .get())
}

fn lean_angle(member: &Member) -> Result<f64, BraceError> {
    Ok(MemberSpan::new(member.p0_mm(), member.p1_mm())?
        .lean_angle_deg()
        .get()
        .get())
}

fn largest_gap(gaps: &[ZInterval]) -> Option<ZInterval> {
    let first = gaps.first().copied()?;
    Some(gaps[1..].iter().copied().fold(first, |best, candidate| {
        let best_span = best.hi_mm.get() - best.lo_mm.get();
        let candidate_span = candidate.hi_mm.get() - candidate.lo_mm.get();
        if candidate_span > best_span {
            candidate
        } else {
            best
        }
    }))
}

fn rungs_that_fit(span: f64, requested: usize) -> usize {
    if requested == 0 {
        return 0;
    }
    requested.min((span - 1.0 + HEIGHT_TOLERANCE_MM).floor().max(0.0) as usize)
}

fn map_height_to_path_member(forest: &Forest, path: &[MemberKey], z: f64) -> Option<MemberKey> {
    path.iter()
        .copied()
        .filter(|key| {
            forest.member(*key).is_some_and(|member| {
                let (lo, hi) = z_extent(member);
                lo - HEIGHT_TOLERANCE_MM <= z && z <= hi + HEIGHT_TOLERANCE_MM
            })
        })
        .min()
}

fn path_fallbacks(
    forest: &Forest,
    path: &[MemberKey],
    host: MemberKey,
    ideal_z: f64,
    lo: f64,
    hi: f64,
    config: &BraceConfig,
) -> Vec<(MemberKey, f64)> {
    let mut out = Vec::new();
    for key in path {
        let Some(member) = forest.member(*key) else {
            continue;
        };
        let (member_lo, member_hi) = z_extent(member);
        let window_lo = lo.max(member_lo);
        let window_hi = hi.min(member_hi);
        if window_hi <= window_lo {
            continue;
        }
        for fraction in &config.fallback_fractions {
            let z = window_lo + fraction * (window_hi - window_lo);
            if *key == host && (z - ideal_z).abs() <= HEIGHT_TOLERANCE_MM {
                continue;
            }
            out.push((*key, z));
        }
    }
    out.sort_by(|left, right| {
        left.1
            .total_cmp(&right.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    out.dedup();
    out.truncate(config.max_stacked.saturating_mul(path.len().max(1)));
    out
}

fn z_extent(member: &Member) -> (f64, f64) {
    let z0 = member.p0_mm()[2].get();
    let z1 = member.p1_mm()[2].get();
    (z0.min(z1), z0.max(z1))
}

fn axis_point_at_z(member: &Member, z: f64) -> [f64; 3] {
    let p0 = member.p0_mm().map(Finite::get);
    let p1 = member.p1_mm().map(Finite::get);
    let dz = p1[2] - p0[2];
    if dz.abs() < HEIGHT_TOLERANCE_MM {
        return [p0[0], p0[1], z];
    }
    let t = (z - p0[2]) / dz;
    [p0[0] + t * (p1[0] - p0[0]), p0[1] + t * (p1[1] - p0[1]), z]
}

fn finite(value: f64) -> Result<Finite, BraceError> {
    Finite::new(value).map_err(|_| BraceError::InvalidGeometry)
}

fn finite3(values: [f64; 3]) -> Result<[Finite; 3], BraceError> {
    Ok([finite(values[0])?, finite(values[1])?, finite(values[2])?])
}

fn sub(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn scale(value: [f64; 3], factor: f64) -> [f64; 3] {
    [value[0] * factor, value[1] * factor, value[2] * factor]
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn norm(value: [f64; 3]) -> f64 {
    dot(value, value).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forest::Owner;
    use geom_kernel::NativeSurfaceConfig;

    /// #36 — the retained-topology half of a fixture.
    ///
    /// Built with `WeldPolicy::ExactBits`, the same policy
    /// `plinth_support::validate_perception_input` uses in production, so the
    /// face IDs `NativeSurface::project` returns for this mesh and the
    /// adjacency `topology.neighbors` walks address the SAME faces. A fixture
    /// that welded differently would silently fragment the seat zone.
    fn fixture_topology(vertices: Vec<[f64; 3]>, faces: Vec<[u32; 3]>) -> FaceTopology {
        let mesh = plinth_mesh::Mesh::new(vertices, faces).expect("fixture mesh");
        plinth_mesh::build_face_topology(&mesh, plinth_mesh::WeldPolicy::ExactBits)
            .expect("fixture topology")
    }

    fn test_penetration_limits() -> PenetrationBatchLimits {
        PenetrationBatchLimits {
            max_rows: usize::MAX,
            max_total_work: usize::MAX,
            max_total_hits: usize::MAX,
            max_total_samples: usize::MAX,
            max_total_intervals: usize::MAX,
            max_scratch_bytes: usize::MAX,
        }
    }

    fn positive(value: f64) -> PositiveFinite {
        PositiveFinite::new(value).unwrap()
    }

    fn member(p0: [f64; 3], p1: [f64; 3], kind: MemberKind, ordinal: u32) -> Member {
        Member::new(MemberSpec {
            p0_mm: finite3(p0).unwrap(),
            p1_mm: finite3(p1).unwrap(),
            r0_mm: TerminalEndRadius::new(positive(0.5)),
            r1_mm: RootwardEndRadius::new(positive(0.5)),
            kind,
            owner: Owner::Generated {
                family: "brace-test".into(),
                ordinal,
            },
            model_contact_end: None,
            carried_mm2: finite(0.0).unwrap(),
        })
    }

    fn contact_member(
        p0: [f64; 3],
        p1: [f64; 3],
        kind: MemberKind,
        end: ContactEnd,
        ordinal: u32,
    ) -> Member {
        Member::new(MemberSpec {
            p0_mm: finite3(p0).unwrap(),
            p1_mm: finite3(p1).unwrap(),
            r0_mm: TerminalEndRadius::new(positive(0.5)),
            r1_mm: RootwardEndRadius::new(positive(0.5)),
            kind,
            owner: Owner::Generated {
                family: "brace-test".into(),
                ordinal,
            },
            model_contact_end: Some(end),
            carried_mm2: finite(0.0).unwrap(),
        })
    }

    /// A slab whose top face is exactly the plane `z = 0`.
    fn flat_table_parts() -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        const H: f64 = 20.0;
        let vertices = vec![
            [-H, -H, -10.0],
            [H, -H, -10.0],
            [H, H, -10.0],
            [-H, H, -10.0],
            [-H, -H, 0.0],
            [H, -H, 0.0],
            [H, H, 0.0],
            [-H, H, 0.0],
        ];
        let faces = vec![
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
        (vertices, faces)
    }

    fn flat_table() -> NativeSurface {
        let (vertices, faces) = flat_table_parts();
        NativeSurface::from_indexed(
            &vertices,
            &faces,
            NativeSurfaceConfig {
                grid_pitch_mm: 0.5,
                max_grid_bytes: 16 * 1024 * 1024,
            },
        )
        .unwrap()
    }

    /// #36 — the face adjacency for the SAME indexed mesh `flat_table` builds its
    /// surface from. The seat-zone flood fill walks it, and it must be the same
    /// mesh or the face IDs `project` returns address nothing.
    fn flat_table_topology() -> FaceTopology {
        let (vertices, faces) = flat_table_parts();
        fixture_topology(vertices, faces)
    }

    /// **A slab whose top face is FINELY TESSELLATED** — `pitch_mm` triangles
    /// over an 8 mm square, with the four walls and the underside left coarse.
    ///
    /// `flat_table` cannot price a seat exemption. Its top face is TWO triangles
    /// 40 mm across, so a seat zone is the WHOLE tabletop and the nearest
    /// non-exempt face is the 10 mm-away underside: any seated arm reads metres
    /// of headroom there (MEASURED by the round-5 challenger: `+8.75 mm` at
    /// `r_max = 3.0 mm`). That is what hid challenge finding F1. On this slab the
    /// zone is a disc of the band's own size and the nearest non-exempt face
    /// sits one tessellation step outside it, which is the geometry a real mini
    /// presents.
    ///
    /// Watertight by construction: each wall is a fan from the bottom corner
    /// adjacent to its first top vertex, over the SAME top-edge vertices the
    /// grid emitted, closed by one triangle to the other bottom corner. The four
    /// edge traversals are each the previous one rotated 90° about `z`, so all
    /// four walls inherit the top face's outward winding.
    fn fine_seat_slab_parts(pitch_mm: f64) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        const HALF_MM: f64 = 4.0;
        const DEPTH_MM: f64 = 10.0;
        let span_mm = 2.0 * HALF_MM;
        let steps = (span_mm / pitch_mm).round() as usize;
        assert!(steps >= 4, "the top face must actually be tessellated");
        let mut vertices = Vec::with_capacity((steps + 1) * (steps + 1) + 4);
        for i in 0..=steps {
            for j in 0..=steps {
                vertices.push([
                    -HALF_MM + span_mm * (i as f64) / (steps as f64),
                    -HALF_MM + span_mm * (j as f64) / (steps as f64),
                    0.0,
                ]);
            }
        }
        let top = |i: usize, j: usize| (i * (steps + 1) + j) as u32;
        let mut faces = Vec::with_capacity(2 * steps * steps + 4 * (steps + 1) + 2);
        for i in 0..steps {
            for j in 0..steps {
                faces.push([top(i, j), top(i + 1, j), top(i + 1, j + 1)]);
                faces.push([top(i, j), top(i + 1, j + 1), top(i, j + 1)]);
            }
        }
        let base = vertices.len() as u32;
        for corner in [
            [-HALF_MM, -HALF_MM],
            [HALF_MM, -HALF_MM],
            [HALF_MM, HALF_MM],
            [-HALF_MM, HALF_MM],
        ] {
            vertices.push([corner[0], corner[1], -DEPTH_MM]);
        }
        faces.push([base, base + 2, base + 1]);
        faces.push([base, base + 3, base + 2]);
        let walls = [
            (
                (0..=steps).map(|k| top(k, 0)).collect::<Vec<_>>(),
                base,
                base + 1,
            ),
            (
                (0..=steps).map(|k| top(steps, k)).collect::<Vec<_>>(),
                base + 1,
                base + 2,
            ),
            (
                (0..=steps)
                    .map(|k| top(steps - k, steps))
                    .collect::<Vec<_>>(),
                base + 2,
                base + 3,
            ),
            (
                (0..=steps).map(|k| top(0, steps - k)).collect::<Vec<_>>(),
                base + 3,
                base,
            ),
        ];
        for (edge, first_corner, last_corner) in walls {
            for k in 0..steps {
                faces.push([first_corner, edge[k + 1], edge[k]]);
            }
            faces.push([first_corner, last_corner, edge[steps]]);
        }
        (vertices, faces)
    }

    /// The surface and the face adjacency for ONE `fine_seat_slab_parts` mesh,
    /// welded the way `validate_perception_input` welds production's.
    fn fine_seat_slab(pitch_mm: f64) -> (NativeSurface, FaceTopology) {
        let (vertices, faces) = fine_seat_slab_parts(pitch_mm);
        let surface = NativeSurface::from_indexed(
            &vertices,
            &faces,
            NativeSurfaceConfig {
                grid_pitch_mm: 0.5,
                max_grid_bytes: 16 * 1024 * 1024,
            },
        )
        .unwrap();
        let topology = fixture_topology(vertices, faces);
        (surface, topology)
    }

    /// The two-tier ruling, read through the REGISTRY rather than restated.
    ///
    /// A wiring test, not a values test: the values themselves are pinned in
    /// `plinth-core` by `the_two_tier_demanded_gaps_are_a_pruning_0_6_and_a_
    /// refusing_0_3`. What this asserts is that the brace stage actually READS
    /// each of them, at the stage that owns it — the failure mode the two-tier
    /// split invites is a hand-typed 0.3 somewhere, which would go stale the
    /// day a coupon measurement supersedes the owner decision.
    #[test]
    fn each_stage_reads_its_own_demanded_gap_from_the_registry() {
        let constants = m0_constants().unwrap();
        let registered_placement = constants.placeholders().standoff_mm().diagnostic().get();
        let registered_final = *constants.evidence().final_clearance_gap_mm().value();
        let config = BraceConfig::current(0.05).unwrap();

        assert_eq!(
            config
                .contact_policy(contact::ClearanceStage::Placement)
                .demanded_gap_mm,
            registered_placement,
            "the PLACEMENT demand is STANDOFF_MM, read from the registry"
        );
        assert_eq!(
            config
                .contact_policy(contact::ClearanceStage::Final)
                .demanded_gap_mm,
            registered_final,
            "the FINAL demand is FINAL_CLEARANCE_GAP_MM, read from the registry"
        );
        assert!(
            registered_final < registered_placement,
            "the REFUSING demand must be strictly below the PRUNING one \
             ({registered_final} vs {registered_placement}), or the placement \
             search would admit exactly what the final pass then refuses"
        );
        // The PR-2 mask's own standoff term is a DIFFERENT quantity from the
        // demand and must not have followed it down.
        assert_eq!(
            config
                .contact_policy(contact::ClearanceStage::Final)
                .standoff_mm,
            registered_placement,
            "the mask's standoff term is the mask's input, not the demand"
        );
    }

    /// TWO DEMANDS, ONE PREDICATE — on real geometry, through the real kernel.
    ///
    /// Three arms parallel to a flat table, each `r + gap` above it, so the TRUE
    /// surface-to-surface gap is exactly `gap`:
    ///
    /// | true gap | at PLACEMENT 0.600 | at FINAL 0.300 |
    /// |---|---|---|
    /// | 0.45 mm | pruned  | LEGAL |
    /// | 0.20 mm | pruned  | refused |
    /// | 0.80 mm | legal   | legal |
    ///
    /// The middle row is what keeps the two-tier split from being a blanket
    /// loosening: the final demand is smaller, not absent, and a gap below it is
    /// refused exactly as before. The bottom row keeps the test from passing by
    /// refusing everything.
    ///
    /// Both columns are the SAME kernel entry point with the same insets on the
    /// same rows; only the demanded number differs. That is the "one predicate"
    /// half of the ruling, asserted rather than asserted-about.
    #[test]
    fn a_0_45mm_true_gap_is_pruned_at_placement_legal_at_final_and_a_0_20mm_gap_is_still_refused() {
        let surface = flat_table();
        let config = BraceConfig::current(0.05).unwrap();
        let placement = config
            .contact_policy(contact::ClearanceStage::Placement)
            .demanded_gap_mm;
        let demanded_final = config
            .contact_policy(contact::ClearanceStage::Final)
            .demanded_gap_mm;

        const RADIUS: f64 = 0.5;
        let gaps = [0.45, 0.20, 0.80];
        let p0 = gaps
            .iter()
            .map(|gap| [-2.0, 0.0, RADIUS + gap])
            .collect::<Vec<_>>();
        let p1 = gaps
            .iter()
            .map(|gap| [2.0, 0.0, RADIUS + gap])
            .collect::<Vec<_>>();
        let radii = vec![RADIUS; gaps.len()];
        let zero = vec![0.0; gaps.len()];

        let graded = |demand: f64| {
            surface
                .segment_clearance(&p0, &p1, &radii, &radii, demand, &zero, &zero)
                .unwrap()
                .0
        };
        let pruning = graded(placement);
        let refusing = graded(demanded_final);

        // The kernel reads a TRUE gap: distance minus the arm radius minus the
        // demand. Stated as an equality so a change in what the kernel measures
        // fails here rather than silently re-interpreting both demands.
        for (row, gap) in gaps.iter().enumerate() {
            assert!(
                (pruning[row] - (gap - placement)).abs() < 1e-9,
                "row {row}: placement clearance must be (true gap - demand)"
            );
            assert!(
                (refusing[row] - (gap - demanded_final)).abs() < 1e-9,
                "row {row}: final clearance must be (true gap - demand)"
            );
        }

        assert!(
            pruning[0] < 0.0 && refusing[0] >= 0.0,
            "a 0.45 mm true gap is pruned by the search margin and LEGAL under \
             the refusing demand — that is what the two-tier split buys"
        );
        assert!(
            pruning[1] < 0.0 && refusing[1] < 0.0,
            "a 0.20 mm true gap is still refused: the final demand is smaller, \
             not absent"
        );
        assert!(
            pruning[2] >= 0.0 && refusing[2] >= 0.0,
            "a 0.80 mm true gap clears both demands, so this test is not \
             passing by refusing everything"
        );
    }

    /// The caller's penetration cap REFUSES rather than quietly skipping the
    /// axis-transit screen — on a fixture that CONSTRUCTS the admitted row.
    ///
    /// This replaces the runner-level `max_rows = 1` form, which depended on
    /// the required corpus happening to leave a row in `axis_illegal_rows`'
    /// admission window and went vacuous the moment the FINAL demand moved
    /// (`runner.rs`, `the_callers_brace_penetration_authority_is_the_one_the_
    /// brace_stage_spends`). Here the row is built to order: the window admits
    /// a clearance in `[0, 0.1)`, and a member parked
    /// `r + FINAL_CLEARANCE_GAP_MM + 0.05` above a flat table reads exactly
    /// 0.05 by construction, derived from the registered demand rather than
    /// from a number typed in.
    #[test]
    fn a_capped_penetration_batch_refuses_rather_than_skipping_the_transit_screen() {
        let config = BraceConfig::current(0.05).unwrap();
        let demanded = config
            .contact_policy(contact::ClearanceStage::Final)
            .demanded_gap_mm;
        const RADIUS: f64 = 0.5;
        // Clearance = true gap - demand = 0.05, inside the admission window.
        let height = RADIUS + demanded + 0.05;
        let mut forest = Forest::new();
        forest.add_member(member(
            [-2.0, 0.0, height],
            [2.0, 0.0, height],
            MemberKind::Strut,
            0,
        ));
        let surface = flat_table();

        let generous =
            exact_clearance_rows(&forest, &surface, &config, test_penetration_limits()).unwrap();
        assert_eq!(generous.len(), 1);
        assert!(
            (generous[0].1 - 0.05).abs() < 1e-9,
            "the fixture must land inside the admission window, or the cap has \
             nothing to refuse; read {}",
            generous[0].1
        );

        let capped = exact_clearance_rows(
            &forest,
            &surface,
            &config,
            PenetrationBatchLimits {
                max_rows: 0,
                ..test_penetration_limits()
            },
        );
        assert!(
            matches!(
                capped,
                Err(BraceError::Penetration(
                    geom_kernel::PenetrationBatchError::ResourceLimitExceeded {
                        resource: "rows",
                        required: 1,
                        limit: 0,
                    }
                ))
            ),
            "a batch the caller's authority cannot pay for must REFUSE; \
             skipping it would silently retire the transit screen"
        );
    }

    fn remote_box_parts() -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        let vertices = vec![
            [100.0, 100.0, 100.0],
            [110.0, 100.0, 100.0],
            [110.0, 110.0, 100.0],
            [100.0, 110.0, 100.0],
            [100.0, 100.0, 110.0],
            [110.0, 100.0, 110.0],
            [110.0, 110.0, 110.0],
            [100.0, 110.0, 110.0],
        ];
        let faces = vec![
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
        (vertices, faces)
    }

    fn remote_box() -> NativeSurface {
        let (vertices, faces) = remote_box_parts();
        NativeSurface::from_indexed(
            &vertices,
            &faces,
            NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: 4 * 1024 * 1024,
            },
        )
        .unwrap()
    }

    /// #36 — the face adjacency for the SAME indexed mesh `remote_box` builds its
    /// surface from. The seat-zone flood fill walks it, and it must be the same
    /// mesh or the face IDs `project` returns address nothing.
    fn remote_box_topology() -> FaceTopology {
        let (vertices, faces) = remote_box_parts();
        fixture_topology(vertices, faces)
    }

    fn near_wall_box_parts() -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        let vertices = vec![
            [4.0, -1.0, 8.0],
            [5.0, -1.0, 8.0],
            [5.0, 1.0, 8.0],
            [4.0, 1.0, 8.0],
            [4.0, -1.0, 12.0],
            [5.0, -1.0, 12.0],
            [5.0, 1.0, 12.0],
            [4.0, 1.0, 12.0],
        ];
        let faces = vec![
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
        (vertices, faces)
    }

    fn near_wall_box() -> NativeSurface {
        let (vertices, faces) = near_wall_box_parts();
        NativeSurface::from_indexed(
            &vertices,
            &faces,
            NativeSurfaceConfig {
                grid_pitch_mm: 0.5,
                max_grid_bytes: 4 * 1024 * 1024,
            },
        )
        .unwrap()
    }

    /// #36 — the face adjacency for the SAME indexed mesh `near_wall_box` builds its
    /// surface from. The seat-zone flood fill walks it, and it must be the same
    /// mesh or the face IDs `project` returns address nothing.
    fn near_wall_box_topology() -> FaceTopology {
        let (vertices, faces) = near_wall_box_parts();
        fixture_topology(vertices, faces)
    }

    /// `near_wall_box` with its model-facing wall RAKED, so the surface normal
    /// there has a non-zero z component.
    ///
    /// `near_wall_box` cannot exercise BLOCKER-3: its near face is exactly
    /// vertical, so `normal_z == 0` and seating moves the contact point purely
    /// in xy.  This fixture is the same solid with the near wall's two top
    /// vertices pulled from `x = 4.0` to `x = 3.5`, giving a wall normal
    /// proportional to `(-8, 0, -1)` (`|normal_z| ~ 0.124`).  The PR-1 seat then
    /// displaces the contact point in z by `|normal_z| * h ~ 0.037 mm` — seven
    /// orders of magnitude above `HEIGHT_TOLERANCE_MM` = 1e-9.  Everything else
    /// (extent, grid pitch, distance from the host axis) matches the fixture
    /// that already yields a model rung, so the only variable is the rake.
    fn raked_wall_box_parts() -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        let vertices = vec![
            [4.0, -1.0, 8.0],
            [5.0, -1.0, 8.0],
            [5.0, 1.0, 8.0],
            [4.0, 1.0, 8.0],
            [3.5, -1.0, 12.0],
            [5.0, -1.0, 12.0],
            [5.0, 1.0, 12.0],
            [3.5, 1.0, 12.0],
        ];
        let faces = vec![
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
        (vertices, faces)
    }

    fn raked_wall_box() -> NativeSurface {
        let (vertices, faces) = raked_wall_box_parts();
        NativeSurface::from_indexed(
            &vertices,
            &faces,
            NativeSurfaceConfig {
                grid_pitch_mm: 0.5,
                max_grid_bytes: 4 * 1024 * 1024,
            },
        )
        .unwrap()
    }

    /// #36 — the face adjacency for the SAME indexed mesh `raked_wall_box` builds its
    /// surface from. The seat-zone flood fill walks it, and it must be the same
    /// mesh or the face IDs `project` returns address nothing.
    fn raked_wall_box_topology() -> FaceTopology {
        let (vertices, faces) = raked_wall_box_parts();
        fixture_topology(vertices, faces)
    }

    fn enclosing_box_parts() -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        let vertices = vec![
            [0.0, 0.0, 0.0],
            [20.0, 0.0, 0.0],
            [20.0, 20.0, 0.0],
            [0.0, 20.0, 0.0],
            [0.0, 0.0, 20.0],
            [20.0, 0.0, 20.0],
            [20.0, 20.0, 20.0],
            [0.0, 20.0, 20.0],
        ];
        let faces = vec![
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
        (vertices, faces)
    }

    fn enclosing_box() -> NativeSurface {
        let (vertices, faces) = enclosing_box_parts();
        NativeSurface::from_indexed(
            &vertices,
            &faces,
            NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: 4 * 1024 * 1024,
            },
        )
        .unwrap()
    }

    /// #36 — the face adjacency for the SAME indexed mesh `enclosing_box` builds its
    /// surface from. The seat-zone flood fill walks it, and it must be the same
    /// mesh or the face IDs `project` returns address nothing.
    fn enclosing_box_topology() -> FaceTopology {
        let (vertices, faces) = enclosing_box_parts();
        fixture_topology(vertices, faces)
    }

    #[test]
    fn frozen_cross_link_thresholds_and_twin_parity_match_source_ledger() {
        let mut forest = Forest::new();
        let a = forest.add_member(member(
            [0.0, 0.0, 30.0],
            [0.0, 0.0, 0.0],
            MemberKind::Trunk,
            0,
        ));
        let b = forest.add_member(member(
            [2.0, 0.0, 30.0],
            [2.0, 0.0, 0.0],
            MemberKind::Trunk,
            1,
        ));
        let config = BraceConfig::current(0.05).unwrap();
        let rows =
            cross_link_candidates(&forest, a, 10.0, 0, &config, config.neighbor_max_xy_mm).unwrap();
        assert_eq!(rows, vec![(2.0, b)]);
        let even = cross_link_rods(&forest, a, b, 10.0, 2.0, 0, &config).unwrap();
        let odd = cross_link_rods(&forest, a, b, 10.0, 2.0, 1, &config).unwrap();
        assert_eq!(even.len(), 1);
        assert_eq!(odd.len(), 1);
        assert_eq!(even[0].p0, [0.0, 0.0, 10.0]);
        assert_eq!(odd[0].p0, [2.0, 0.0, 10.0]);
    }

    #[test]
    fn registered_reach_tiers_preserve_the_intermediate_cap() {
        let config = BraceConfig::current(0.05).unwrap();
        assert_eq!(
            reach_tiers_mm(&config),
            vec![
                config.neighbor_max_xy_mm * config.reach_tier_base_factor,
                config.neighbor_max_xy_mm * config.reach_max_factor,
            ]
        );
    }

    #[test]
    fn twin_sizing_does_not_apply_the_x_pair_real_endpoint_floor() {
        let mut forest = Forest::new();
        let host = forest.add_member(member(
            [15.0, 0.0, 30.0],
            [0.0, 0.0, 0.0],
            MemberKind::Trunk,
            0,
        ));
        let neighbor = forest.add_member(member(
            [2.0, 0.0, 30.0],
            [2.0, 0.0, 0.0],
            MemberKind::Trunk,
            1,
        ));
        let config = BraceConfig::current(0.05).unwrap();
        let rods = cross_link_rods(&forest, host, neighbor, 2.0, 2.0, 0, &config).unwrap();
        let compatibility = LegacyCompatibility::o10_live_current_support_and_raft();
        let expected = config
            .brace_radius_mm
            .get()
            .max(deflection_radius(2.0 * 2.0_f64.sqrt(), 45.0, &compatibility).unwrap());
        assert_eq!(rods.len(), 1);
        assert!((rods[0].radius.get() - expected).abs() <= 1e-12);
    }

    /// REPLACES `angled_embed_inset_leaves_contact_depth_unscaled`, which
    /// pinned the retired `embed_inset_mm` -- and pinned it as CORRECT.
    ///
    /// That test asserted the seat depth is NOT divided by `|cos|`. It is: the
    /// depth is measured along the normal while the mask is an arc length
    /// along the axis, so the 1/|cos| conversion applies to every term. The old
    /// formula therefore UNDER-masked by `h*(1/|cos| - 1)`, making the
    /// clearance check stricter than the geometry warrants -- and the
    /// `.max(0.5)` floor that fed it made the mask smaller still, which is why
    /// it was the conservative branch rather than the grazing rejector it was
    /// long believed to be.
    #[test]
    fn contact_mask_divides_every_term_by_the_incidence() {
        let config = BraceConfig::current(0.05).unwrap();
        let policy = config.contact_policy(contact::ClearanceStage::Final);
        let seat = 0.3;
        let mask = contact::mask_extent_mm(seat, 0.5, 0.5, &policy).unwrap();

        // Every term divided, including the seat depth: 1.45 / 0.5 = 2.90 mm.
        assert!((mask - 2.9).abs() <= 1e-12, "mask {mask}");
        assert!((mask - (0.3 + 0.5 + 0.6 + 0.05) / 0.5).abs() <= 1e-12);

        // Explicitly NOT the retired arithmetic, which left the depth
        // unscaled: 0.3 + 1.1/0.5 = 2.50 mm.
        let retired: f64 = 0.3 + (0.5 + 0.6) / 0.5;
        assert!((retired - 2.5).abs() <= 1e-12);

        // The gap decomposes into exactly TWO independent corrections, and
        // conflating them is how the original design talked itself into a
        // wrong mask. At |cos| = 0.5 with a 0.05 mm layer:
        //   (i)  the undivided-depth fix, h*(1/|cos| - 1) = 0.30 mm; and
        //   (ii) the engineering margin the retired formula had none of,
        //        MARGIN/|cos| = 0.10 mm.
        let undivided_depth_fix = seat * (1.0 / 0.5 - 1.0);
        let margin_term = policy.process_resolution_mm / 0.5;
        assert!((undivided_depth_fix - 0.3).abs() <= 1e-12);
        assert!((margin_term - 0.1).abs() <= 1e-12);
        assert!(
            (mask - retired - undivided_depth_fix - margin_term).abs() <= 1e-12,
            "mask {mask} must exceed the retired {retired} by exactly the \
             undivided-depth fix {undivided_depth_fix} plus the margin term \
             {margin_term}"
        );
    }

    #[test]
    fn equal_largest_gap_keeps_first_source_row() {
        let gaps = vec![
            ZInterval {
                lo_mm: finite(1.0).unwrap(),
                hi_mm: finite(4.0).unwrap(),
            },
            ZInterval {
                lo_mm: finite(10.0).unwrap(),
                hi_mm: finite(13.0).unwrap(),
            },
        ];
        assert_eq!(largest_gap(&gaps), Some(gaps[0]));
    }

    #[test]
    fn attempted_host_height_is_suppressed_across_primary_and_fallback_sites() {
        let mut forest = Forest::new();
        let host = forest.add_member(member(
            [0.0, 0.0, 10.0],
            [0.0, 0.0, 0.0],
            MemberKind::Trunk,
            0,
        ));
        let tried = BTreeMap::from([(host, vec![5.0])]);
        assert!(site_was_attempted(&tried, host, 5.49, 1.0));
        assert!(!site_was_attempted(&tried, host, 5.51, 1.0));
    }

    #[test]
    fn shared_trigger_relief_uses_one_resize_and_one_clearance_batch() {
        let mut forest = Forest::new();
        let first = forest.add_member(member(
            [0.0, 0.0, 2.0],
            [0.0, 0.0, 0.0],
            MemberKind::Trunk,
            0,
        ));
        let second = forest.add_member(member(
            [2.0, 0.0, 2.0],
            [2.0, 0.0, 0.0],
            MemberKind::Trunk,
            1,
        ));
        let plate = finite(0.0).unwrap();
        let report = transient_report(&forest, plate).unwrap();
        let result = pending_triggers_after_trial(
            &forest,
            &remote_box(),
            &remote_box_topology(),
            plate,
            &report,
            vec![first, second],
            &BraceConfig::current(0.05).unwrap(),
            test_penetration_limits(),
        )
        .unwrap();
        assert!(result.pending.is_empty());
        assert_eq!(result.resize_batches, 1);
        assert_eq!(result.exact_clearance_batches, 1);
        assert_eq!(
            result.seat_zone_refreeze_batches, 1,
            "the trial re-sizes seated exemptions in ONE batched call, on the \
             same batch boundary as the resize and the clearance pass. A count \
             above 1 is a regression to one native call per seated member."
        );
    }

    #[test]
    fn model_anchor_cap_counts_only_post_breach_survivors() {
        let mut forest = Forest::new();
        let host = forest.add_member(member(
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            MemberKind::Trunk,
            0,
        ));
        let mut candidates = (0..8)
            .map(|rank| ProjectedModelCandidate {
                raw: RawModelCandidate {
                    need_index: 0,
                    site_index: 0,
                    rank,
                    host,
                    z_mm: 0.0,
                    centroid: [rank as f64, 0.0, 0.0],
                },
                // This fixture exercises the six-anchor retention cap, which
                // never reaches a seat-zone freeze.
                seat_face_id: None,
                point: [rank as f64, 0.0, 0.0],
                normal: [1.0, 0.0, 0.0],
            })
            .collect::<Vec<_>>();
        retain_projected_model_survivors(
            &mut candidates,
            &[true, true, false, false, false, false, false, false],
            6,
        );
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.raw.rank)
                .collect::<Vec<_>>(),
            vec![2, 3, 4, 5, 6, 7]
        );
    }

    #[test]
    fn axis_penetration_belt_rejects_inside_rods_in_one_batch() {
        let surface = enclosing_box();
        let config = BraceConfig::current(0.05).unwrap();
        let candidates = vec![
            PlacementCandidate {
                need_index: 0,
                site_index: 0,
                rank: 0,
                kind: BraceAdditionKind::CrossLink,
                rods: vec![Rod {
                    p0: [5.0, 8.0, 10.0],
                    p1: [15.0, 8.0, 10.0],
                    radius: positive(0.1),
                    inset0: 0.0,
                    inset1: 0.0,
                    #[cfg(feature = "census")]
                    contact: None,
                }],
                printable: true,
                seat_zone: None,
            },
            PlacementCandidate {
                need_index: 1,
                site_index: 0,
                rank: 0,
                kind: BraceAdditionKind::CrossLink,
                rods: vec![Rod {
                    p0: [5.0, 12.0, 10.0],
                    p1: [15.0, 12.0, 10.0],
                    radius: positive(0.1),
                    inset0: 0.0,
                    inset1: 0.0,
                    #[cfg(feature = "census")]
                    contact: None,
                }],
                printable: true,
                seat_zone: None,
            },
        ];
        let viable =
            evaluate_candidates(&surface, &candidates, &config, test_penetration_limits()).unwrap();
        assert_eq!(viable, vec![false, false]);
        let refusal = evaluate_candidates(
            &surface,
            &candidates,
            &config,
            PenetrationBatchLimits {
                max_rows: 0,
                ..test_penetration_limits()
            },
        )
        .unwrap_err();
        assert!(matches!(
            refusal,
            BraceError::Penetration(PenetrationBatchError::ResourceLimitExceeded {
                resource: "rows",
                required: 2,
                limit: 0,
            })
        ));
    }

    /// The flat table PLUS one small foreign panel, welded into ONE surface, so
    /// a grazing arm has something to be graded against that is not the surface
    /// it is seated in.
    fn table_with_foreign_panel_parts(
        lo: [f64; 3],
        hi: [f64; 3],
    ) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        const H: f64 = 40.0;
        let box_faces = [
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
        let mut vertices = vec![
            [-H, -H, -10.0],
            [H, -H, -10.0],
            [H, H, -10.0],
            [-H, H, -10.0],
            [-H, -H, 0.0],
            [H, -H, 0.0],
            [H, H, 0.0],
            [-H, H, 0.0],
        ];
        let mut faces = box_faces.to_vec();
        let base = vertices.len() as u32;
        for corner in [
            [lo[0], lo[1], lo[2]],
            [hi[0], lo[1], lo[2]],
            [hi[0], hi[1], lo[2]],
            [lo[0], hi[1], lo[2]],
            [lo[0], lo[1], hi[2]],
            [hi[0], lo[1], hi[2]],
            [hi[0], hi[1], hi[2]],
            [lo[0], hi[1], hi[2]],
        ] {
            vertices.push(corner);
        }
        for face in box_faces {
            faces.push([base + face[0], base + face[1], base + face[2]]);
        }
        (vertices, faces)
    }

    fn table_with_foreign_panel(lo: [f64; 3], hi: [f64; 3]) -> NativeSurface {
        let (vertices, faces) = table_with_foreign_panel_parts(lo, hi);
        NativeSurface::from_indexed(
            &vertices,
            &faces,
            NativeSurfaceConfig {
                grid_pitch_mm: 0.5,
                max_grid_bytes: 64 * 1024 * 1024,
            },
        )
        .unwrap()
    }

    /// #36 — the face adjacency for the SAME indexed mesh
    /// `table_with_foreign_panel` builds its surface from.
    fn table_with_foreign_panel_topology(lo: [f64; 3], hi: [f64; 3]) -> FaceTopology {
        let (vertices, faces) = table_with_foreign_panel_parts(lo, hi);
        fixture_topology(vertices, faces)
    }

    /// **MONOTONICITY, THROUGH THE PRODUCTION FINAL PASS — the successor to the
    /// parked-state trip-wire** (#36 §8 step 6, §10.7 item 1).
    ///
    /// It replaces `the_final_pass_still_grades_a_thinner_arm_worse_until_the_zones_are_wired`,
    /// which MEASURED the interim inversion and was documented to go red the
    /// moment steps 3-5 landed. Same fixture, same axis, same surface; only the
    /// expected sign changed, exactly as its own message said it would.
    ///
    /// **What the rule now guarantees, and why it is a theorem rather than a
    /// measurement.** Fix an axis and a FROZEN `Z`. The graded clearance is
    ///
    /// ```text
    /// clearance = min over the WHOLE axis of dist(x, surface \ Z) - (r_max + gap)
    ///           = C - r_max - gap
    /// ```
    ///
    /// `C` does not mention the radius: the tested STATION set is the whole axis
    /// either way, and the tested FACE set is `surface \ Z`, which is frozen. So
    /// clearance is monotone decreasing in `r_max`, unconditionally, on any
    /// geometry — and a thinner arm can never grade worse than a fatter one.
    ///
    /// The retired rule could not say that. Its exemption was a LENGTH
    /// proportional to the radius, so a fatter arm was tested over a SHORTER
    /// stretch of its own axis, and on this same fixture the two rows read
    /// `thin = -0.45 REFUSED` against `fat = +0.05 LEGAL` — the inversion
    /// challenge finding B2 measured.
    ///
    /// **ONE frozen zone, two radii.** That is the production case, not a
    /// contrivance: a contact is minted once and `brace::apply_resize` may
    /// fatten the member afterwards. Re-freezing from the later radius is what
    /// the rule forbids, and `seat_zone.rs`'s
    /// `the_zone_does_not_move_when_the_radius_does_because_it_is_never_recomputed`
    /// is the sabotage that reproduces why: a wider band swallows more surface,
    /// so the fat arm would be graded against strictly LESS of it.
    #[test]
    fn a_thinner_arm_never_grades_worse_than_a_fatter_one_through_the_production_final_pass() {
        let config = BraceConfig::current(0.05).unwrap();
        let policy = config.contact_policy(contact::ClearanceStage::Final);
        let seating = contact::seating_of(&MemberKind::Strut, ContactEnd::Terminalward);

        const COS: f64 = 0.2;
        const THIN_MM: f64 = 0.20;
        const FAT_MM: f64 = 0.90;
        let dir = [(1.0_f64 - COS * COS).sqrt(), 0.0, COS];
        let p0 = [0.0, 0.0, -0.3];
        let point_at = |u: f64| std::array::from_fn(|axis| p0[axis] + dir[axis] * u);

        // NON-VACUITY 1 — the RETIRED masks are still well separated at these
        // two radii, so the fixture is still the one that produced the
        // inversion. If they ever converge, this test stops discriminating and
        // says so instead of passing quietly.
        let mask = |r: f64| contact::placed_mask_inset_mm(seating, COS, r, r, &policy).unwrap();
        let (mask_thin, mask_fat) = (mask(THIN_MM), mask(FAT_MM));
        assert!(
            mask_fat > mask_thin + 3.0,
            "the fixture needs the two retired masks well separated ({mask_thin} vs \
             {mask_fat}); it is the separation that made the skip-length invert"
        );

        // The foreign panel hugs the stretch the THIN arm tested and the FAT
        // arm's own mask used to excuse — geometry unchanged from the trip-wire.
        let a: [f64; 3] = point_at(mask_thin + 0.02);
        let b: [f64; 3] = point_at(mask_thin + 1.0);
        let lo = [a[0], 0.05, a[2] - 0.05];
        let hi = [b[0], 0.25, b[2] + 0.05];
        let surface = table_with_foreign_panel(lo, hi);
        let topology = table_with_foreign_panel_topology(lo, hi);
        let p1: [f64; 3] = point_at(mask_fat + 5.0);

        // THE FREEZE, ONCE, at the THIN arm's creation radius. Both gradings
        // below read this same face set — that is the hypothesis of the theorem
        // and the thing production guarantees by never recomputing.
        let (_, _, face_ids, _) = surface.project(&[p0]);
        let seat_face_id = u32::try_from(face_ids[0]).expect("the seat projects onto a face");
        let zone = crate::seat_zone::freeze_production_seat_zones(
            &surface,
            &topology,
            &[crate::seat_zone::SeatZoneRequest {
                p0_mm: p0,
                p1_mm: p1,
                seat_face_id,
                band_radius_mm: THIN_MM + config.standoff_mm,
            }],
        )
        .expect("the seat zone freezes")
        .remove(0);

        // NON-VACUITY 2 — the exemption is real. An empty zone would make this
        // test a statement about a rule that is not running.
        assert!(
            !zone.zone().is_empty(),
            "the seated arm must forgive its own seat, or the fixture is grading \
             a member with no exemption at all"
        );

        let graded = |radius_mm: f64, config: &BraceConfig| {
            let mut forest = Forest::new();
            let key = forest.add_member(Member::new(MemberSpec {
                p0_mm: finite3(p0).unwrap(),
                p1_mm: finite3(p1).unwrap(),
                r0_mm: TerminalEndRadius::new(positive(radius_mm)),
                r1_mm: RootwardEndRadius::new(positive(radius_mm)),
                kind: MemberKind::Strut,
                owner: Owner::Generated {
                    family: "monotonicity".into(),
                    ordinal: 0,
                },
                model_contact_end: Some(ContactEnd::Terminalward),
                carried_mm2: finite(0.0).unwrap(),
            }));
            forest.set_seat_zone(key, zone.clone());
            let rows =
                exact_clearance_rows(&forest, &surface, config, test_penetration_limits()).unwrap();
            assert_eq!(rows.len(), 1, "one member, one graded row");
            rows[0].1
        };
        let thin = graded(THIN_MM, &config);
        let fat = graded(FAT_MM, &config);

        assert!(
            thin >= fat,
            "MONOTONICITY IS A THEOREM (#36 §3): with the seat zone frozen and \
             the whole axis tested, clearance is `C - r_max - gap` and a thinner \
             arm can NEVER grade worse than a fatter one on the same axis. \
             Kernel said thin(r = {THIN_MM}) = {thin}, fat(r = {FAT_MM}) = {fat}. \
             If this failed, something recomputed the zone from a later radius \
             — which is the retired skip-length's inversion rebuilt one level up."
        );

        // The two verdicts now AGREE, where the retired rule split them. Which
        // way they agree is a fact about this fixture and is reported rather
        // than engineered: the assertion above is the rule, this one only
        // forbids the inversion from surviving in disguise.
        assert_eq!(
            thin >= 0.0,
            fat >= 0.0,
            "the pair that the retired mask split (thin REFUSED, fat LEGAL) must \
             now share a verdict: thin = {thin}, fat = {fat}"
        );

        // The theorem does not mention the demand, so it must also hold at the
        // pre-ruling single demand. A monotonicity that only held at one number
        // would be an accident of that number.
        let pre_ruling = BraceConfig {
            final_clearance_gap_mm: config.standoff_mm,
            ..config.clone()
        };
        let (thin_at_600, fat_at_600) = (graded(THIN_MM, &pre_ruling), graded(FAT_MM, &pre_ruling));
        assert!(
            thin_at_600 >= fat_at_600,
            "monotonicity is independent of the demanded gap, which enters as a \
             constant offset; at {} mm the kernel said thin = {thin_at_600}, \
             fat = {fat_at_600}",
            config.standoff_mm
        );

        println!(
            "HONEST-EXEMPTION-MONOTONICITY at FINAL {}: thin(r={THIN_MM})={thin}  \
             fat(r={FAT_MM})={fat}  zone={} faces  |  at the pre-ruling {}: \
             thin={thin_at_600}  fat={fat_at_600}",
            config.final_clearance_gap_mm,
            zone.zone().len(),
            config.standoff_mm
        );
    }

    /// **THE BLIND SPOT, CLOSED — a foreign feature the retired mask hid is
    /// graded now that the exemption names FACES** (#36 §8 step 6).
    ///
    /// This is the defect the whole change exists for, measured through the
    /// production final pass. On the Giant Bat both refusals were FOREIGN
    /// feature proximity — the opposite wall of a crevice, 133-151 degrees away
    /// from the seat's own closest point — and the radius-proportional skip
    /// excused it: at `r = 1.712694` an arm PASSED with `clearance = +0.336`
    /// while standing 0.242160 mm INSIDE the model, because its own mask
    /// covered the station that would have said so.
    ///
    /// The fixture is that geometry in miniature: a table the arm is seated in,
    /// and a foreign panel hugging the stretch of axis the fat arm's mask used
    /// to cover. The frozen zone is the seat's own faces and NOTHING else, so
    /// the panel is tested and the arm is refused.
    ///
    /// **The named sabotage: `a_zone_that_swallowed_the_foreign_panel`.** Widen
    /// the exemption to include the panel's faces — which is what a length
    /// measured from the seat did in effect — and the same arm passes. The test
    /// performs that sabotage itself rather than describing it, so the
    /// difference between the two rules is a number in this file.
    #[test]
    fn the_foreign_wall_inside_the_retired_mask_is_graded_now_that_the_exemption_names_faces() {
        let config = BraceConfig::current(0.05).unwrap();
        let policy = config.contact_policy(contact::ClearanceStage::Final);
        let seating = contact::seating_of(&MemberKind::Strut, ContactEnd::Terminalward);

        const COS: f64 = 0.2;
        const FAT_MM: f64 = 0.90;
        let dir = [(1.0_f64 - COS * COS).sqrt(), 0.0, COS];
        let p0 = [0.0, 0.0, -0.3];
        let point_at = |u: f64| std::array::from_fn(|axis| p0[axis] + dir[axis] * u);

        let mask = |r: f64| contact::placed_mask_inset_mm(seating, COS, r, r, &policy).unwrap();
        let (mask_thin, mask_fat) = (mask(0.20), mask(FAT_MM));
        let a: [f64; 3] = point_at(mask_thin + 0.02);
        let b: [f64; 3] = point_at(mask_thin + 1.0);
        let lo = [a[0], 0.05, a[2] - 0.05];
        let hi = [b[0], 0.25, b[2] + 0.05];
        let (_, fixture_faces) = table_with_foreign_panel_parts(lo, hi);
        let surface = table_with_foreign_panel(lo, hi);
        let topology = table_with_foreign_panel_topology(lo, hi);
        let p1: [f64; 3] = point_at(mask_fat + 5.0);

        // NON-VACUITY — the panel really does lie inside the stretch the FAT
        // arm's retired mask would have skipped. Without this the test could
        // pass on a panel the old rule graded too.
        let panel_u = mask_thin + 0.5;
        assert!(
            panel_u < mask_fat,
            "the foreign panel must sit at u = {panel_u} mm, INSIDE the fat arm's \
             retired mask of {mask_fat} mm, or this fixture is not the blind spot"
        );

        let (_, _, face_ids, _) = surface.project(&[p0]);
        let seat_face_id = u32::try_from(face_ids[0]).expect("the seat projects onto a face");
        let honest = crate::seat_zone::freeze_production_seat_zones(
            &surface,
            &topology,
            &[crate::seat_zone::SeatZoneRequest {
                p0_mm: p0,
                p1_mm: p1,
                seat_face_id,
                band_radius_mm: FAT_MM + config.standoff_mm,
            }],
        )
        .expect("the seat zone freezes")
        .remove(0);

        // The panel is the SECOND box in the fixture: `table_with_foreign_panel`
        // appends its twelve faces after the table's twelve.
        let panel_faces = (12_u32..fixture_faces.len() as u32).collect::<Vec<_>>();
        assert!(!panel_faces.is_empty(), "the fixture must carry a panel");
        assert!(
            panel_faces.iter().all(|face| !honest.zone().contains(face)),
            "the honest zone must forgive the SEAT and nothing else; it must not \
             contain a single foreign-panel face"
        );

        let graded = |zone: crate::seat_zone::SeatZoneRecord| {
            let mut forest = Forest::new();
            let key = forest.add_member(Member::new(MemberSpec {
                p0_mm: finite3(p0).unwrap(),
                p1_mm: finite3(p1).unwrap(),
                r0_mm: TerminalEndRadius::new(positive(FAT_MM)),
                r1_mm: RootwardEndRadius::new(positive(FAT_MM)),
                kind: MemberKind::Strut,
                owner: Owner::Generated {
                    family: "blind-spot".into(),
                    ordinal: 0,
                },
                model_contact_end: Some(ContactEnd::Terminalward),
                carried_mm2: finite(0.0).unwrap(),
            }));
            forest.set_seat_zone(key, zone);
            exact_clearance_rows(&forest, &surface, &config, test_penetration_limits()).unwrap()[0]
                .1
        };

        let honest_clearance = graded(honest.clone());

        // THE SABOTAGE, performed: `a_zone_that_swallowed_the_foreign_panel`.
        let mut swallowed = honest.zone().to_vec();
        swallowed.extend(panel_faces);
        swallowed.sort_unstable();
        swallowed.dedup();
        let swallowed_clearance = graded(honest.with_zone(std::sync::Arc::from(swallowed)));

        assert!(
            honest_clearance < 0.0,
            "the arm stands close to a FOREIGN feature that is not its own seat, \
             so the honest rule must refuse it. Kernel said {honest_clearance}. \
             This is the verdict the retired skip-length inverted."
        );
        assert!(
            swallowed_clearance > honest_clearance,
            "the sabotage must be LOAD-BEARING: a zone that swallows the foreign \
             panel has to change the verdict, or this test is not measuring the \
             exemption at all. honest = {honest_clearance}, swallowed = \
             {swallowed_clearance}"
        );
        println!(
            "BLIND-SPOT-CLOSED: honest={honest_clearance} (zone {} faces)  \
             swallowed={swallowed_clearance}",
            honest.zone().len()
        );
    }

    /// **A SEATED ARM IS STILL LEGAL — the exemption has to exempt** (#36 §8
    /// step 6).
    ///
    /// The other half of the blind-spot test, and the one that stops the change
    /// from being a blanket tightening. A member seated into the model is
    /// legitimately inside the material it is seated in; if the honest rule
    /// refused it, every seated arm in the engine would refuse itself and three
    /// families would die at once.
    ///
    /// **The named sabotage: `a_seat_that_forgot_to_freeze_its_zone`.** Grade
    /// the same member with no frozen zone — which is precisely what a MISSED
    /// creation site produces — and it is refused by its own designed embed.
    /// That is why §9.4's inventory had to be complete before step 5 landed, and
    /// it is the failure direction a missed site takes: a refusal, loudly, never
    /// a silent pass.
    ///
    /// **WHAT THIS PIN CANNOT SEE — challenge finding F1, 2026-08-22.** Its
    /// fixture is `flat_table`, whose top face is two triangles 40 mm across, so
    /// the frozen zone is the WHOLE tabletop and the nearest non-exempt face is
    /// the 10 mm-away underside. The seated arm therefore reads metres of
    /// headroom here (`+8.75 mm` even at `r_max = 3.0 mm`) and this test proves
    /// only that the exemption EXEMPTS. What it costs is measured on a real
    /// seat by
    /// `a_seated_arm_that_grows_after_its_freeze_spends_its_headroom_one_for_one`.
    #[test]
    fn a_seated_arm_stays_legal_against_its_own_designed_embed_and_refuses_without_its_zone() {
        let config = BraceConfig::current(0.05).unwrap();
        let surface = flat_table();
        let topology = flat_table_topology();
        const RADIUS_MM: f64 = 0.4;
        // Seated one designed contact depth into the table, whose top face is
        // exactly `z = 0`, and standing straight up out of it.
        let p0 = [0.0, 0.0, -config.contact_depth_mm];
        let p1 = [0.0, 0.0, 5.0];

        let (_, _, face_ids, _) = surface.project(&[p0]);
        let seat_face_id = u32::try_from(face_ids[0]).expect("the seat projects onto a face");
        let zone = crate::seat_zone::freeze_production_seat_zones(
            &surface,
            &topology,
            &[crate::seat_zone::SeatZoneRequest {
                p0_mm: p0,
                p1_mm: p1,
                seat_face_id,
                band_radius_mm: RADIUS_MM + config.standoff_mm,
            }],
        )
        .expect("the seat zone freezes")
        .remove(0);
        assert!(
            !zone.zone().is_empty(),
            "a seated arm must forgive its own seat face"
        );

        let graded = |zone: Option<crate::seat_zone::SeatZoneRecord>| {
            let mut forest = Forest::new();
            let key = forest.add_member(Member::new(MemberSpec {
                p0_mm: finite3(p0).unwrap(),
                p1_mm: finite3(p1).unwrap(),
                r0_mm: TerminalEndRadius::new(positive(RADIUS_MM)),
                r1_mm: RootwardEndRadius::new(positive(RADIUS_MM)),
                kind: MemberKind::Strut,
                owner: Owner::Generated {
                    family: "seated-legality".into(),
                    ordinal: 0,
                },
                model_contact_end: Some(ContactEnd::Terminalward),
                carried_mm2: finite(0.0).unwrap(),
            }));
            if let Some(zone) = zone {
                forest.set_seat_zone(key, zone);
            }
            exact_clearance_rows(&forest, &surface, &config, test_penetration_limits()).unwrap()[0]
                .1
        };

        let with_zone = graded(Some(zone));
        // THE SABOTAGE, performed: `a_seat_that_forgot_to_freeze_its_zone`.
        let without_zone = graded(None);

        assert!(
            with_zone >= 0.0,
            "a member seated its designed {} mm into the model must stay LEGAL — \
             it is inside the material it is seated in on purpose. Kernel said \
             {with_zone}.",
            config.contact_depth_mm
        );
        assert!(
            without_zone < 0.0,
            "and the exemption must be what makes it legal: with no frozen zone \
             the same member is graded over its own embed and refused. Kernel \
             said {without_zone}. If this ever passes, the test has stopped \
             measuring the exemption."
        );
        println!("SEATED-LEGALITY: with_zone={with_zone}  without_zone={without_zone}");
    }

    /// The six post-freeze growths challenge finding F1 swept by hand, in
    /// millimetres. GROWTHS rather than absolute radii, because the freeze
    /// radius is read from the registry rather than typed.
    const F1_GROWTH_MM: [f64; 6] = [0.00, 0.05, 0.15, 0.30, 0.45, 0.65];

    /// One `fine_seat_slab` fixture with a seated arm per rung of the F1
    /// ladder, each frozen at the CREATION radius — the state the sizing pass
    /// leaves behind, before anything re-sizes it.
    ///
    /// Shared by the ruling's pin and its sabotage so the two cannot drift into
    /// measuring different geometry. It returns the keys in ladder order.
    fn f1_ladder_forest(
        surface: &NativeSurface,
        topology: &FaceTopology,
        p0: [f64; 3],
        p1: [f64; 3],
        seat_radius_mm: f64,
        standoff_mm: f64,
        ladder: &[f64],
    ) -> (Forest, Vec<MemberKey>) {
        let (_, _, face_ids, _) = surface.project(&[p0]);
        let seat_face_id = u32::try_from(face_ids[0]).expect("the seat projects onto a face");
        // ONE batched freeze for the whole ladder, at the CREATION radius —
        // every rung starts from the same band, exactly as production does.
        let creation = crate::seat_zone::freeze_production_seat_zones(
            surface,
            topology,
            &vec![
                crate::seat_zone::SeatZoneRequest {
                    p0_mm: p0,
                    p1_mm: p1,
                    seat_face_id,
                    band_radius_mm: seat_radius_mm + standoff_mm,
                };
                ladder.len()
            ],
        )
        .expect("the seat zones freeze");

        let mut forest = Forest::new();
        let keys = ladder
            .iter()
            .zip(creation)
            .enumerate()
            .map(|(ordinal, (&radius_mm, record))| {
                let key = forest.add_member(Member::new(MemberSpec {
                    p0_mm: finite3(p0).unwrap(),
                    p1_mm: finite3(p1).unwrap(),
                    r0_mm: TerminalEndRadius::new(positive(radius_mm)),
                    r1_mm: RootwardEndRadius::new(positive(radius_mm)),
                    kind: MemberKind::Strut,
                    owner: Owner::Generated {
                        family: "f1-footprint".into(),
                        ordinal: u32::try_from(ordinal).unwrap(),
                    },
                    model_contact_end: Some(ContactEnd::Terminalward),
                    carried_mm2: finite(0.0).unwrap(),
                }));
                forest.set_seat_zone(key, record);
                key
            })
            .collect::<Vec<_>>();
        (forest, keys)
    }

    /// The PLACEMENT tier's verdict for each rung, through production
    /// `evaluate_candidates`, reading each member's zone out of the forest.
    ///
    /// One batch for the whole ladder. `evaluate_candidates` grades a candidate
    /// slice, so this is one production call, never one per rung.
    fn f1_placement_legal(
        surface: &NativeSurface,
        forest: &Forest,
        keys: &[MemberKey],
        p0: [f64; 3],
        p1: [f64; 3],
        config: &BraceConfig,
    ) -> Vec<bool> {
        let candidates = keys
            .iter()
            .enumerate()
            .map(|(index, key)| {
                let member = forest.member(*key).expect("a live rung");
                PlacementCandidate {
                    need_index: 0,
                    site_index: index,
                    rank: index,
                    kind: BraceAdditionKind::ModelRung,
                    rods: vec![Rod {
                        p0,
                        p1,
                        radius: member.terminal_radius_mm().get(),
                        // The PR-2 mask, set generously enough to cover the
                        // whole designed embed, so the TRANSIT screen is not
                        // what decides here and the demand is.
                        inset0: config.contact_depth_mm + config.min_illegal_len_mm,
                        inset1: 0.0,
                        #[cfg(feature = "census")]
                        contact: None,
                    }],
                    printable: true,
                    seat_zone: forest.seat_zone_record(*key).cloned(),
                }
            })
            .collect::<Vec<_>>();
        evaluate_candidates(surface, &candidates, config, test_penetration_limits()).unwrap()
    }

    /// The FINAL tier's clearance for each rung, through production
    /// `exact_clearance_rows`. One batch for the whole ladder; the rows come
    /// back keyed, so nothing here depends on the forest's walk order.
    fn f1_final_clearances(
        surface: &NativeSurface,
        forest: &Forest,
        keys: &[MemberKey],
        config: &BraceConfig,
    ) -> Vec<f64> {
        let graded = exact_clearance_rows(forest, surface, config, test_penetration_limits())
            .unwrap()
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        keys.iter().map(|key| graded[key]).collect()
    }

    /// **THE EXEMPTION EQUALS THE ARM'S TRUE FOOTPRINT — Craig's ruling of
    /// 2026-08-23, MEASURED through both production tiers (design §11.9).**
    ///
    /// The ruling, verbatim: *"the exemption always equals the arm's true
    /// footprint, no slack."* When the sizing pass thickens a seated arm past
    /// its creation radius, the frozen exemption is RE-SIZED to the final
    /// radius. Padding the band at creation was rejected (slack in the band);
    /// accepting the cost was rejected (a silent coupling between two passes).
    ///
    /// This test REPLACES
    /// `a_seated_arm_that_grows_after_its_freeze_spends_its_headroom_one_for_one`,
    /// which measured the pre-ruling behaviour and whose own note said it would
    /// go red when the ruling landed. Same fixture, same six-rung ladder, same
    /// registry-read seat radius; the sabotage below keeps its numbers.
    ///
    /// **What the ruling buys, exactly.** Every production sizer leaves the
    /// contact end at least as fat as the other end
    /// (`legacy_compat::size_live_current_support_member` clamps
    /// `rootward = max(required, terminal)`; `transient`'s non-uniform branch
    /// does the same; `cco::uniformly_size_ancestor_path` writes both ends
    /// equal). So `r_contact = r_max` for a seated production member, and after
    /// a re-size the graded clearance is
    ///
    /// ```text
    /// clearance = (r_final + STANDOFF + s) − (r_max + demand)
    ///           = STANDOFF − demand + s
    /// ```
    ///
    /// independent of the radius. `s` is tessellation slack and is strictly
    /// positive because a face straddling the band boundary is admitted to the
    /// band WHOLE. So the FINAL tier keeps at least `STANDOFF − FINAL_GAP` and
    /// the PLACEMENT tier keeps exactly `s`.
    ///
    /// **BE PLAIN ABOUT THE PLACEMENT MARGIN.** It is tessellation slack and
    /// nothing else, and it is much smaller than the first rung suggests.
    /// MEASURED across this ladder: 0.050, 0.031, 0.018, 0.025, 0.014 and
    /// **0.0008** mm — eight ten-thousandths of a millimetre at the top rung. It
    /// wobbles rather than falling smoothly, because it is mesh aliasing rather
    /// than a quantity. Quoting the 0.050 mm first rung alone would be the
    /// favourable reading. "No self-refusal at any rung" is a statement about
    /// the SIGN, which is structural; the SIZE guarantees nothing.
    ///
    /// The FINAL tier is the one with real margin, and it is guaranteed rather
    /// than measured: `STANDOFF − FINAL_GAP` = 0.300 mm, asserted below.
    ///
    /// The refusals that survive are the ones that should: genuine foreign
    /// geometry. Craig confirmed that case explicitly and it is untouched —
    /// `the_blind_spot_is_closed_because_the_exemption_names_faces` measures it,
    /// and an arm placed with 0.6 mm to a foreign feature and thickened to
    /// 0.35 mm of remaining gap still refuses at FINAL. This slab is clean, so
    /// nothing here refuses.
    #[test]
    fn a_seated_arm_that_grows_is_re_sized_to_its_final_footprint_and_stays_legal() {
        let config = BraceConfig::current(0.05).unwrap();
        let constants = m0_constants().unwrap();
        let seat_radius_mm = constants.placeholders().min_shoot_r_mm().diagnostic().get();
        const PITCH_MM: f64 = 0.25;
        let (surface, topology) = fine_seat_slab(PITCH_MM);
        let p0 = [0.0, 0.0, -config.contact_depth_mm];
        let p1 = [0.0, 0.0, 3.0];
        let ladder = F1_GROWTH_MM
            .iter()
            .map(|growth| seat_radius_mm + growth)
            .collect::<Vec<_>>();

        let (mut forest, keys) = f1_ladder_forest(
            &surface,
            &topology,
            p0,
            p1,
            seat_radius_mm,
            config.standoff_mm,
            &ladder,
        );
        let created_faces = keys
            .iter()
            .map(|key| forest.seat_zone_record(*key).unwrap().zone().len())
            .collect::<Vec<_>>();
        // FIXTURE NON-VACUITY — the zone must be a DISC, not the whole top face,
        // or this test degenerates into `..._stays_legal_against_its_own_designed_embed`.
        assert!(
            created_faces[0] * 4 < topology.n_faces(),
            "the fine slab must give the seat a zone that is a small fraction of \
             the mesh, or the fixture cannot price the footprint: zone {} faces \
             of {} total",
            created_faces[0],
            topology.n_faces()
        );

        // **THE RULING, through the production helper.** Not a hand-rolled
        // re-freeze: this is the same call `brace_fixed_point_current_parity`,
        // `pending_triggers_after_trial`, `cco::grow_terminal_inner` and
        // `rewire::validate_changed_geometry` make after their sizing passes. If
        // the helper stops re-sizing, this test is what says so.
        let refrozen = crate::seat_zone::refreeze_resized_seat_zones(
            &surface,
            &topology,
            config.standoff_mm,
            std::iter::once(&mut forest),
        )
        .unwrap();
        assert_eq!(
            refrozen,
            F1_GROWTH_MM.len() - 1,
            "every rung that GREW must be re-sized, and the rung that did not \
             must be left alone: {} of {} rungs re-cut",
            refrozen,
            F1_GROWTH_MM.len()
        );

        let final_faces = keys
            .iter()
            .map(|key| forest.seat_zone_record(*key).unwrap().zone().len())
            .collect::<Vec<_>>();
        // NON-VACUITY OF THE RE-SIZE — the band followed the radius. This
        // replaces the retired pin's "the ladder must span LEGAL to REFUSED"
        // guard, which was ITS non-vacuity check: without a replacement this
        // test would pass against a build whose re-size silently did nothing.
        assert!(
            final_faces[0] == created_faces[0]
                && final_faces.windows(2).all(|pair| pair[1] > pair[0]),
            "a fatter arm must claim a strictly larger patch of its own seat, and \
             the un-grown rung must keep exactly the patch it was created with: \
             created {created_faces:?} became {final_faces:?}"
        );

        let ladder_final = f1_final_clearances(&surface, &forest, &keys, &config);
        let ladder_placement = f1_placement_legal(&surface, &forest, &keys, p0, p1, &config);

        // THE RULING'S LAW, measured through the production FINAL pass across
        // the whole ladder. The clearance no longer falls with the growth,
        // because the tested face set grew with it — and it keeps at least the
        // two tiers' difference, which is what "no slack" means arithmetically.
        let structural_minimum_mm = config.standoff_mm - config.final_clearance_gap_mm;
        assert!(
            ladder_final
                .iter()
                .all(|graded| *graded >= structural_minimum_mm),
            "a re-sized exemption equals the arm's own footprint, so every rung \
             must grade at least STANDOFF − FINAL_GAP = {structural_minimum_mm} mm \
             at FINAL, whatever it grew to. Kernel said {ladder_final:?} for \
             growths {F1_GROWTH_MM:?}"
        );
        // AND THE HEADLINE: no rung refuses ITSELF, on either tier. On this
        // clean slab there is no foreign geometry anywhere, so a refusal could
        // only ever have been the arm's own seat.
        assert!(
            ladder_final.iter().all(|graded| *graded > 0.0),
            "no rung may be refused at FINAL: {ladder_final:?}"
        );
        assert!(
            ladder_placement.iter().all(|legal| *legal),
            "and none at PLACEMENT either — this is the crossing challenge \
             finding F1 measured at r_max = 0.400 mm, which the ruling removes: \
             {ladder_placement:?}"
        );

        // THE PLACEMENT MARGIN, STATED AS A NUMBER rather than as a boolean.
        // `evaluate_candidates` returns verdicts, so the millimetres are
        // INFERRED from the FINAL column: the two tiers grade the identical
        // frozen face set on the identical axis and differ only by the demanded
        // gap, so PLACEMENT = FINAL − tier_delta exactly. MEASURED is the
        // boolean above; this is the arithmetic that explains it.
        let tier_delta_mm = config
            .contact_policy(contact::ClearanceStage::Placement)
            .demanded_gap_mm
            - config
                .contact_policy(contact::ClearanceStage::Final)
                .demanded_gap_mm;
        let placement_margin_mm = ladder_final
            .iter()
            .map(|graded| graded - tier_delta_mm)
            .collect::<Vec<_>>();
        assert!(
            placement_margin_mm.iter().all(|margin| *margin > 0.0),
            "the INFERRED placement margins must agree in sign with the MEASURED \
             verdicts, or the two tiers are not grading what this test thinks: \
             {placement_margin_mm:?} against {ladder_placement:?}"
        );
        println!(
            "F1-FOOTPRINT: seat r={seat_radius_mm} mm, pitch={PITCH_MM} mm, mesh \
             {} faces, tier gap={tier_delta_mm} mm, re-cut {refrozen} rungs\n\
             F1-FOOTPRINT:      growth_mm={F1_GROWTH_MM:?}\n\
             F1-FOOTPRINT:   zone_faces={final_faces:?} (created {created_faces:?})\n\
             F1-FOOTPRINT:  final_mm=[MEASURED] {ladder_final:?}\n\
             F1-FOOTPRINT: placement=[MEASURED] {ladder_placement:?}\n\
             F1-FOOTPRINT: placement_margin_mm=[INFERRED] {placement_margin_mm:?}",
            topology.n_faces()
        );
    }

    /// **THE SABOTAGE: an exemption that was NOT re-sized after the growth —
    /// and the two crossings challenge finding F1 measured come straight back.**
    ///
    /// Named sabotage: `a_sizing_pass_whose_seat_zones_were_never_re_cut`.
    /// Delete the `refreeze_resized_seat_zones` call from any of its four
    /// production sites — `brace_fixed_point_current_parity`,
    /// `pending_triggers_after_trial`, `cco::grow_terminal_inner`,
    /// `rewire::validate_changed_geometry` — and this is the behaviour that
    /// returns.
    ///
    /// **This is where F1's original measurement is KEPT** rather than deleted,
    /// so the before-picture stays checkable. Same slab, same ladder, same
    /// registry seat radius; the only difference from the pin above is that the
    /// re-size never runs.
    ///
    /// ```text
    /// r_max mm | 0.35  0.40  0.50  0.65  0.80  1.00
    /// FINAL mm |+0.35 +0.30 +0.20 +0.05 -0.10 -0.30
    /// PLACEMENT| LEGAL LEGAL  REF   REF   REF   REF
    /// ```
    ///
    /// **PLACEMENT refuses above `r_max` = 0.400 mm. FINAL refuses above
    /// `r_max` = 0.700 mm.** Both crossings are probed a thousandth either side,
    /// so they are kernel verdicts and not arithmetic.
    #[test]
    fn a_seat_zone_that_was_not_re_sized_after_the_growth_refuses_the_arm_again() {
        let config = BraceConfig::current(0.05).unwrap();
        let constants = m0_constants().unwrap();
        let seat_radius_mm = constants.placeholders().min_shoot_r_mm().diagnostic().get();
        const PITCH_MM: f64 = 0.25;
        let (surface, topology) = fine_seat_slab(PITCH_MM);
        let p0 = [0.0, 0.0, -config.contact_depth_mm];
        let p1 = [0.0, 0.0, 3.0];

        // THE SABOTAGE: build the ladder and DO NOT call
        // `refreeze_resized_seat_zones`. Every rung keeps the creation band.
        let ladder = |radii: &[f64]| {
            f1_ladder_forest(
                &surface,
                &topology,
                p0,
                p1,
                seat_radius_mm,
                config.standoff_mm,
                radii,
            )
        };

        let growths = F1_GROWTH_MM
            .iter()
            .map(|growth| seat_radius_mm + growth)
            .collect::<Vec<_>>();
        let (forest, keys) = ladder(&growths);
        let ladder_final = f1_final_clearances(&surface, &forest, &keys, &config);
        let ladder_placement = f1_placement_legal(&surface, &forest, &keys, p0, p1, &config);

        // THE PRE-RULING LAW: growth is spent one for one, because the tested
        // face set never moved. The differences cancel the tessellation slack,
        // so this holds to the kernel's own precision rather than to a tolerance
        // chosen to make it pass.
        let headroom_final_mm = ladder_final[0];
        assert!(
            F1_GROWTH_MM
                .iter()
                .zip(&ladder_final)
                .all(|(growth, graded)| ((headroom_final_mm - graded) - growth).abs() < 1.0e-9),
            "without the re-size a seated arm's clearance falls exactly as fast \
             as its radius rises: growths {F1_GROWTH_MM:?} graded {ladder_final:?}"
        );
        assert!(
            headroom_final_mm > 0.0,
            "the arm must be legal at its creation radius, or the fixture is \
             measuring something else: {headroom_final_mm}"
        );

        // THE TWO CROSSINGS, PINNED BY MEASUREMENT rather than by arithmetic.
        let tier_delta_mm = config
            .contact_policy(contact::ClearanceStage::Placement)
            .demanded_gap_mm
            - config
                .contact_policy(contact::ClearanceStage::Final)
                .demanded_gap_mm;
        assert!(
            tier_delta_mm > 0.0,
            "the two-tier ruling must leave PLACEMENT the stricter demand: \
             {tier_delta_mm}"
        );
        const EPS_MM: f64 = 1.0e-3;
        let placement_crossing_mm = seat_radius_mm + headroom_final_mm - tier_delta_mm;
        let final_crossing_mm = seat_radius_mm + headroom_final_mm;
        let probes = vec![
            placement_crossing_mm - EPS_MM,
            placement_crossing_mm + EPS_MM,
            final_crossing_mm - EPS_MM,
            final_crossing_mm + EPS_MM,
        ];
        let (probe_forest, probe_keys) = ladder(&probes);
        let probe_final = f1_final_clearances(&surface, &probe_forest, &probe_keys, &config);
        let probe_placement =
            f1_placement_legal(&surface, &probe_forest, &probe_keys, p0, p1, &config);
        assert!(
            probe_placement[0] && !probe_placement[1],
            "un-re-sized, the PLACEMENT screen flips from LEGAL to REFUSED across \
             r_max = {placement_crossing_mm} mm — this IS challenge finding F1 as \
             it was measured on 2026-08-22. Kernel said {probe_placement:?} at \
             {probes:?}"
        );
        assert!(
            probe_final[2] > 0.0 && probe_final[3] < 0.0,
            "and the FINAL pass flips across r_max = {final_crossing_mm} mm. \
             Kernel said {probe_final:?}"
        );
        assert!(
            probe_final[1] > 0.0,
            "…while the SAME arm is still legal at FINAL: {}. The two tiers must \
             disagree across this band or the fixture is not on the knife edge.",
            probe_final[1]
        );
        // NON-VACUITY OF THE LADDER — it must straddle both crossings, or the
        // printed table is not the challenger's table.
        assert!(
            ladder_placement[0]
                && !ladder_placement[F1_GROWTH_MM.len() - 1]
                && ladder_final[F1_GROWTH_MM.len() - 1] < 0.0,
            "the ladder must span LEGAL to REFUSED on BOTH tiers: \
             final {ladder_final:?} placement {ladder_placement:?}"
        );
        println!(
            "F1-NOT-RE-SIZED: growth_mm={F1_GROWTH_MM:?}\n\
             F1-NOT-RE-SIZED:  final_mm={ladder_final:?}\n\
             F1-NOT-RE-SIZED: placement_legal={ladder_placement:?}\n\
             F1-NOT-RE-SIZED: PLACEMENT refuses above r_max={placement_crossing_mm} mm, \
             FINAL refuses above r_max={final_crossing_mm} mm"
        );
    }

    /// **THE TRANSIT SCREEN'S ADMISSION WINDOW STILL CATCHES A BURIED ARM —
    /// challenge finding F2, 2026-08-22, MEASURED rather than inferred.**
    ///
    /// Design §5 said this change "must not move" the axis-transit screen. That
    /// is true of the MASKS the screen receives, which step 5 left alone. It was
    /// not true as written of the screen's ADMISSION rule: a row is screened
    /// when its clearance is in `[0, 0.1)` or when it is `≥ 0.1` with its
    /// midpoint inside — and that clearance is now the zero-inset,
    /// zone-excluded number, which moved on every contact-declaring row. The
    /// challenger's finding was that nothing measured the consequence.
    ///
    /// This measures it on the branch `the_exact_pass_hands_the_transit_screen_…`
    /// cannot reach: the midpoint-inside branch, which is the one that catches
    /// an arm buried in material far from every face — the `#52` family. Under
    /// the honest rule such a row's clearance goes UP, not down, because the
    /// seat zone forgives exactly the faces the arm is nearest to; if the
    /// admission rule had been weakened, this is where an arm would escape.
    ///
    /// The fixture: a seated arm that runs DOWN into the slab instead of
    /// standing out of it. Its seat zone is exempt, every remaining face is
    /// 7 mm away, and the distance path therefore reads it WIDE OPEN (asserted
    /// as a witness). Only the screen can refuse it, and the verdict proves the
    /// screen is what did — a distance refusal cannot produce the transit
    /// stamp's exact `−min_illegal_len_mm`.
    ///
    /// **The named sabotage: `an_exact_pass_that_skipped_the_transit_screen`.**
    /// Delete the `axis_illegal_rows` loop in `exact_clearance_rows` and this
    /// buried arm is stamped legal at the full witness clearance.
    #[test]
    fn a_seated_arm_that_keeps_running_inside_the_model_is_still_condemned_by_the_transit_screen() {
        let config = BraceConfig::current(0.05).unwrap();
        let surface = flat_table();
        let topology = flat_table_topology();
        const RADIUS_MM: f64 = 0.4;
        // Seated at the top face and continuing straight DOWN through the slab —
        // 2.7 mm of material past the designed embed the mask covers.
        let p0 = [0.0, 0.0, -config.contact_depth_mm];
        let p1 = [0.0, 0.0, -3.0];
        let (_, _, face_ids, _) = surface.project(&[p0]);
        let seat_face_id = u32::try_from(face_ids[0]).expect("the seat projects onto a face");
        let zone = crate::seat_zone::freeze_production_seat_zones(
            &surface,
            &topology,
            &[crate::seat_zone::SeatZoneRequest {
                p0_mm: p0,
                p1_mm: p1,
                seat_face_id,
                band_radius_mm: RADIUS_MM + config.standoff_mm,
            }],
        )
        .expect("the seat zone freezes")
        .remove(0);

        let mut forest = Forest::new();
        let key = forest.add_member(Member::new(MemberSpec {
            p0_mm: finite3(p0).unwrap(),
            p1_mm: finite3(p1).unwrap(),
            r0_mm: TerminalEndRadius::new(positive(RADIUS_MM)),
            r1_mm: RootwardEndRadius::new(positive(RADIUS_MM)),
            kind: MemberKind::Strut,
            owner: Owner::Generated {
                family: "f2-buried".into(),
                ordinal: 0,
            },
            model_contact_end: Some(ContactEnd::Terminalward),
            carried_mm2: finite(0.0).unwrap(),
        }));
        forest.set_seat_zone(key, zone.clone());

        // THE WITNESS, from the same kernel call the production distance path
        // makes: with its seat zone exempt this arm is nowhere near a face it is
        // graded against, so the distance path cannot be what refuses it, and
        // the row reaches the screen through the midpoint-inside branch rather
        // than the `[0, 0.1)` window.
        let final_gap_mm = config
            .contact_policy(contact::ClearanceStage::Final)
            .demanded_gap_mm;
        let zero = [0.0];
        let witness = surface
            .segment_clearance_excluding(
                &[p0],
                &[p1],
                &[RADIUS_MM],
                &[RADIUS_MM],
                final_gap_mm,
                &zero,
                &zero,
                &[zone.zone().as_ref()],
            )
            .unwrap()
            .0[0];
        assert!(
            witness >= 0.1,
            "NON-VACUITY: the buried arm must read WIDE OPEN on the distance \
             path, or this test is not measuring the screen at all. Kernel said \
             {witness}."
        );

        let verdict = exact_clearance_rows(&forest, &surface, &config, test_penetration_limits())
            .unwrap()[0]
            .1;
        assert!(
            (verdict + config.min_illegal_len_mm).abs() < 1.0e-12,
            "the transit screen must condemn an arm that keeps running inside \
             the model past its designed embed, and the stamp must be the \
             screen's own `-min_illegal_len_mm`. Kernel said {verdict} against \
             a distance-path witness of {witness}; a value equal to the witness \
             means the screen was skipped, and any other negative means \
             something else refused it."
        );
        println!(
            "F2-ADMISSION: witness={witness}  verdict={verdict}  zone={} faces",
            zone.zone().len()
        );
    }

    /// **FINDING D — the axis-transit screen KEEPS the PR-2 mask** (#36 §5
    /// reader 2, §8 step 6).
    ///
    /// Step 5 zeroed the insets on the clearance DISTANCE path and only there.
    /// `axis_illegal_rows` asks a different question — "is this arm passing
    /// THROUGH material away from its seat" — and there a length measured from
    /// the seat is the right shape, because the seat embed IS a length along the
    /// axis. Zeroing both would condemn every seated contact for its own design,
    /// which the prior challenge proved must stay legal.
    ///
    /// **The named sabotage: `a_transit_screen_with_its_seat_mask_zeroed`.** The
    /// test calls the production screen twice on the SAME row — once with the
    /// production masks and once with zeros — and the second condemns.
    #[test]
    fn the_transit_screen_keeps_the_seat_mask_or_it_condemns_the_designed_embed() {
        let config = BraceConfig::current(0.05).unwrap();
        let surface = flat_table();
        const RADIUS_MM: f64 = 0.4;
        // Deliberately DEEP inside the slab, so the row is admitted to the
        // interval batch by the midpoint-inside branch and the penetration
        // interval it finds is the seat embed itself.
        let p0 = [[0.0, 0.0, -3.0]];
        let p1 = [[0.0, 0.0, 1.0]];
        let radii = [RADIUS_MM];
        // The mask must cover the embedded stretch: this row's whole embed is
        // one contiguous interval from the contact end.
        let mask0 = [3.0 + config.min_illegal_len_mm];
        let zero = [0.0];
        // The clearance the distance path now returns for this row is
        // irrelevant to the screen's admission rule beyond putting it in the
        // batch; state it explicitly so the fixture is not reading a number
        // from somewhere else.
        let clearances = [0.05];

        let masked = axis_illegal_rows(
            &surface,
            &p0,
            &p1,
            &radii,
            &radii,
            &mask0,
            &zero,
            &clearances,
            &config,
            test_penetration_limits(),
        )
        .unwrap();
        // THE SABOTAGE, performed: `a_transit_screen_with_its_seat_mask_zeroed`.
        let unmasked = axis_illegal_rows(
            &surface,
            &p0,
            &p1,
            &radii,
            &radii,
            &zero,
            &zero,
            &clearances,
            &config,
            test_penetration_limits(),
        )
        .unwrap();

        assert!(
            unmasked.contains(&0),
            "NON-VACUITY: with the seat mask zeroed the transit screen must \
             condemn this row — it really is inside the model along its own \
             embed. If it does not, the fixture is not exercising the screen."
        );
        assert!(
            !masked.contains(&0),
            "FINDING D: with the PR-2 mask supplied the SAME row must be admitted \
             — the stretch inside the model is the arm's own designed embed, \
             measured as a length from its seat, and condemning it would kill \
             every seated contact in the engine. Step 5 zeroed the insets on the \
             DISTANCE path only."
        );
    }

    /// **FINDING D, THROUGH PRODUCTION — the pin that the direct-call test
    /// above could not provide.**
    ///
    /// MEASURED while red-running the step-6 sabotages: forking
    /// `exact_clearance_rows` to hand ZEROS to `axis_illegal_rows` left the
    /// entire brace suite GREEN. The direct-call test calls the screen itself,
    /// so a production fork is invisible to it — the same shape of hole
    /// challenge finding B3 named for the two unpinned tier reads. This closes
    /// it: the assertion runs through `exact_clearance_rows`, so the fork is
    /// loud.
    ///
    /// The fixture puts a seated arm in the transit screen's own admission
    /// window. The screen admits a row whose clearance sits in `[0, 0.1)`, so a
    /// foreign panel is placed to land the clearance there; the arm is then
    /// examined for penetration intervals, and the only material it passes
    /// through is its own designed embed, which its PR-2 mask covers.
    ///
    /// **The named sabotage: `an_exact_pass_that_zeroed_its_transit_masks`.**
    /// With zeros the embed interval is no longer excluded, the row is
    /// condemned, and `exact_clearance_rows` stamps the verdict negative.
    #[test]
    fn the_exact_pass_hands_the_transit_screen_its_masks_and_not_the_zeroed_insets() {
        let config = BraceConfig::current(0.05).unwrap();
        const RADIUS_MM: f64 = 0.4;
        let final_gap = config
            .contact_policy(contact::ClearanceStage::Final)
            .demanded_gap_mm;
        // A panel this far from the axis lands the clearance inside the
        // screen's `[0, 0.1)` admission window — derived from the demand and
        // the radius, not picked.
        let panel_x = RADIUS_MM + final_gap + 0.02;
        let lo = [panel_x, -0.5, 0.2];
        let hi = [panel_x + 0.2, 0.5, 1.5];
        let surface = table_with_foreign_panel(lo, hi);
        let topology = table_with_foreign_panel_topology(lo, hi);

        // Seated its designed depth into the table (top face at `z = 0`) and
        // standing 2 mm out of it.
        let p0 = [0.0, 0.0, -config.contact_depth_mm];
        let p1 = [0.0, 0.0, 2.0 - config.contact_depth_mm];
        let (_, _, face_ids, _) = surface.project(&[p0]);
        let seat_face_id = u32::try_from(face_ids[0]).expect("the seat projects onto a face");
        let zone = crate::seat_zone::freeze_production_seat_zones(
            &surface,
            &topology,
            &[crate::seat_zone::SeatZoneRequest {
                p0_mm: p0,
                p1_mm: p1,
                seat_face_id,
                band_radius_mm: RADIUS_MM + config.standoff_mm,
            }],
        )
        .expect("the seat zone freezes")
        .remove(0);

        let mut forest = Forest::new();
        let key = forest.add_member(Member::new(MemberSpec {
            p0_mm: finite3(p0).unwrap(),
            p1_mm: finite3(p1).unwrap(),
            r0_mm: TerminalEndRadius::new(positive(RADIUS_MM)),
            r1_mm: RootwardEndRadius::new(positive(RADIUS_MM)),
            kind: MemberKind::Strut,
            owner: Owner::Generated {
                family: "finding-d".into(),
                ordinal: 0,
            },
            model_contact_end: Some(ContactEnd::Terminalward),
            carried_mm2: finite(0.0).unwrap(),
        }));
        forest.set_seat_zone(key, zone);
        let clearance = exact_clearance_rows(&forest, &surface, &config, test_penetration_limits())
            .unwrap()[0]
            .1;

        // THE CLAIM FIRST, THE FIXTURE CHECK SECOND — challenge finding F4,
        // 2026-08-22. Asserting the admission window first made the SABOTAGE
        // read "the fixture must land the clearance in [0, 0.1); kernel said
        // −0.05": true, but it names the fixture when the defect is the
        // production fork. The condemnation is now what fails first, and it
        // says so.
        assert!(
            clearance >= 0.0,
            "FINDING D: the production final pass CONDEMNED a seated arm for its \
             OWN designed embed — kernel said {clearance}. That is what the \
             sabotage `an_exact_pass_that_zeroed_its_transit_masks` does: with \
             zeros in place of the PR-2 masks the transit screen reads the \
             embed as a penetration and `exact_clearance_rows` stamps the row \
             negative. (A distance refusal would land some other number; this \
             fixture's distance path is checked immediately below.)"
        );
        // NON-VACUITY — the row really is in the screen's admission window, so
        // the screen really did run on it. Without this the test would pass on
        // a row the screen never looked at, which is precisely how the hole
        // this test closes stayed invisible.
        assert!(
            clearance < 0.1,
            "NON-VACUITY: the fixture must land the clearance in the transit \
             screen's admission window [0, 0.1); kernel said {clearance}, which \
             is above it, so the screen may never have looked at this row. \
             Retune the panel rather than deleting the assertion."
        );
        println!("FINDING-D-THROUGH-PRODUCTION: clearance={clearance}");
    }

    /// **`evaluate_candidates` spends the PLACEMENT demand — ruling R-1495, as
    /// a test that goes RED on a sabotaged read** (challenge finding B3,
    /// 2026-08-22).
    ///
    /// Before this test, `brace.rs:1495` was one of two ruled tier reads with
    /// no test that could tell: the challenger switched
    /// `ClearanceStage::Placement` to `Final` at this site and the whole scoped
    /// estate stayed GREEN. The ruling is only real if a violation is loud.
    ///
    /// The fixture is the two-tier case §8 step 6 names, built so the answer
    /// lives strictly BETWEEN the two registered demands and cannot be reached
    /// by either one by accident:
    ///
    /// * a horizontal rod of radius 0.45 mm at `z = +0.90 mm` over a table
    ///   whose top face is `z = 0`, so the kernel's TRUE gap is
    ///   `0.90 - 0.45 = 0.45 mm` — asserted from the kernel at demand 0, not
    ///   restated arithmetic;
    /// * `0.300 < 0.45 < 0.600` is asserted from the REGISTRY, so the fixture
    ///   is provably a two-tier discriminator rather than a hand-picked number;
    /// * a control rod 2.0 mm up is viable at BOTH tiers, so a uniformly
    ///   refusing fixture cannot pass this test by accident.
    ///
    /// The pin: production must call the middle rod NOT viable. A read that
    /// switched to `FINAL_CLEARANCE_GAP_MM` would call it viable, and this
    /// assertion is what says so.
    #[test]
    fn evaluate_candidates_prunes_at_the_placement_demand_not_the_final_one() {
        let surface = flat_table();
        let config = BraceConfig::current(0.05).unwrap();
        let constants = m0_constants().unwrap();
        let placement_mm = constants.placeholders().standoff_mm().diagnostic().get();
        let final_mm = *constants.evidence().final_clearance_gap_mm().value();

        let radius_mm = 0.45_f64;
        let rod_at = |height_mm: f64| PlacementCandidate {
            need_index: 0,
            site_index: 0,
            rank: 0,
            kind: BraceAdditionKind::CrossLink,
            rods: vec![Rod {
                p0: [-2.0, 0.0, height_mm],
                p1: [2.0, 0.0, height_mm],
                radius: positive(radius_mm),
                inset0: 0.0,
                inset1: 0.0,
                #[cfg(feature = "census")]
                contact: None,
            }],
            printable: true,
            seat_zone: None,
        };

        // NON-VACUITY, from the kernel: at a demand of zero the number IS the
        // true gap, and it sits strictly between the two registered demands.
        let (true_gap, _) = surface
            .segment_clearance(
                &[[-2.0, 0.0, 0.9]],
                &[[2.0, 0.0, 0.9]],
                &[radius_mm],
                &[radius_mm],
                0.0,
                &[0.0],
                &[0.0],
            )
            .unwrap();
        assert!(
            (true_gap[0] - 0.45).abs() < 1e-9,
            "the fixture's TRUE gap must be 0.45 mm; the kernel said {}",
            true_gap[0]
        );
        assert!(
            final_mm < true_gap[0] && true_gap[0] < placement_mm,
            "the fixture must discriminate the two tiers: FINAL {final_mm} < \
             true gap {} < PLACEMENT {placement_mm}",
            true_gap[0]
        );

        let viable = evaluate_candidates(
            &surface,
            &[rod_at(0.9), rod_at(2.0)],
            &config,
            test_penetration_limits(),
        )
        .unwrap();
        assert_eq!(
            viable,
            vec![false, true],
            "R-1495: this tournament prunes at PLACEMENT ({placement_mm} mm), so \
             a 0.45 mm true gap is dropped here and a 1.55 mm one is kept. If \
             this site were switched to read FINAL ({final_mm} mm) the first \
             candidate would survive, which is exactly the sabotage this \
             assertion exists to catch."
        );
    }

    #[test]
    fn stable_unplaceable_fixed_point_is_not_reported_as_converged() {
        let mut forest = Forest::new();
        forest.add_member(member(
            [0.0, 0.0, 30.0],
            [0.0, 0.0, 0.0],
            MemberKind::Trunk,
            0,
        ));
        let plan = brace_fixed_point_current_parity(
            &forest,
            &remote_box(),
            &remote_box_topology(),
            finite(0.0).unwrap(),
            test_penetration_limits(),
            0.05,
            &[],
        )
        .unwrap();
        assert_eq!(plan.termination, BraceFixedPointTermination::Unplaceable);
    }

    #[test]
    fn third_pass_termination_truth_table_preserves_frozen_outcomes() {
        for (added, changed) in [(true, false), (false, true), (true, true)] {
            assert_eq!(
                pass_three_termination(false, added, changed, 0, false).unwrap(),
                Some(BraceFixedPointTermination::IterationLimit)
            );
            assert_eq!(
                pass_three_termination(true, added, changed, 0, false).unwrap(),
                Some(BraceFixedPointTermination::Unplaceable)
            );
        }
        assert_eq!(
            pass_three_termination(false, false, false, 0, false).unwrap(),
            None
        );
        assert!(matches!(
            pass_three_termination(false, true, false, 1, false),
            Err(BraceError::ClearanceViolation(1))
        ));
    }

    /// PR-9 — the refusable/unrefusable split, exhaustively.
    #[test]
    fn a_contact_clearance_refusal_is_not_a_pipeline_violation() {
        // A contact-declaring member reading negative clearance is a refusal
        // in every combination of the other flags, and it OUTRANKS the
        // iteration-limit and unplaceable outcomes, because those describe how
        // the controller stopped while this describes a candidate that must be
        // quarantined.
        for saw_unplaceable in [false, true] {
            for (added, changed) in [(false, false), (true, false), (false, true), (true, true)] {
                assert_eq!(
                    pass_three_termination(saw_unplaceable, added, changed, 0, true).unwrap(),
                    Some(BraceFixedPointTermination::ContactRefused),
                    "saw_unplaceable={saw_unplaceable} added={added} changed={changed}"
                );
            }
        }
        // But a genuine violation still wins: a member with no model contact
        // inside the mini cannot be refused away, so the run must fail even
        // when a refusable one is present alongside it.
        assert!(matches!(
            pass_three_termination(false, false, false, 2, true),
            Err(BraceError::ClearanceViolation(2))
        ));
    }

    /// PR-9 — the classifier, against a real forest.
    #[test]
    fn negative_clearance_is_split_by_whether_the_member_declares_a_contact() {
        // Two identical members buried in the same box, differing ONLY in
        // whether they declare a model contact. That is the whole rule, so the
        // fixture isolates exactly it.
        let surface = remote_box();
        let config = BraceConfig::current(0.05).unwrap();
        let mut forest = Forest::new();
        // Buried in the middle of `remote_box`, which spans 100..110 on every
        // axis, so both members read negative exact clearance.
        let spec = |contact| MemberSpec {
            p0_mm: [
                finite(105.0).unwrap(),
                finite(105.0).unwrap(),
                finite(102.0).unwrap(),
            ],
            p1_mm: [
                finite(105.0).unwrap(),
                finite(105.0).unwrap(),
                finite(108.0).unwrap(),
            ],
            r0_mm: TerminalEndRadius::new(PositiveFinite::new(0.5).unwrap()),
            r1_mm: RootwardEndRadius::new(PositiveFinite::new(0.5).unwrap()),
            kind: MemberKind::Shoot,
            owner: Owner::Generated {
                family: "pr9".into(),
                ordinal: 0,
            },
            model_contact_end: contact,
            carried_mm2: finite(0.0).unwrap(),
        };
        let plain = forest.add_member(Member::new(spec(None)));
        let declaring = forest.add_member(Member::new(spec(Some(ContactEnd::Terminalward))));

        let negatives =
            exact_clearance_negatives(&forest, &surface, &config, test_penetration_limits())
                .unwrap();
        // Non-vacuity: both members really are in violation, so the split is
        // being tested rather than an empty set being partitioned.
        assert!(
            !negatives.is_clean(),
            "the fixture must actually produce negative clearance"
        );
        assert_eq!(negatives.pipeline_violations, 1, "the plain member");
        assert_eq!(
            negatives.contact_refusals,
            vec![declaring],
            "the contact-declaring member"
        );
        assert!(!negatives.contact_refusals.contains(&plain));
    }

    /// **OD-4, and the Giant Bat's hard failure, in the shape that caused it.**
    ///
    /// MEASURED at `e9e160f`, release binary, dense orient: `Giant Bat.stl`
    /// died at `Brace(Forest(QuarantineHasChildren))` on BOTH the drilled shell
    /// and the substituted oriented solid, 75.77 s wall, `emitted=0 failed=1`.
    /// The topology behind it is the one built below, and it is not an unusual
    /// one: `cco::apply_model_anchor` mints the seated arm as a ROOT and
    /// re-parents the contact stub onto it, so **every** placed model-seated
    /// arm has a child by construction. Refusing that arm's contact therefore
    /// hit `Forest::quarantine`'s childless rule every single time.
    ///
    /// The fixture is deliberately built from the two ends the production path
    /// builds: a `Strut` declaring a `Rootward` contact whose rootward end sits
    /// deep inside the model (so its exact clearance is genuinely negative),
    /// and a registered, consumed contact `Stub` parented onto it and sitting
    /// clear of the model (so it is not itself a refusal and the cascade is
    /// what condemns it).
    #[test]
    fn a_refused_model_seated_arm_condemns_its_stub_instead_of_failing_the_run() {
        let surface = remote_box();
        let topology = remote_box_topology();
        let mut forest = Forest::new();

        // The stub: clear of the box (which spans 100..110), so it contributes
        // no negative clearance of its own. Terminal 7, registered and then
        // consumed, exactly as growth leaves it before the CCO re-parents it.
        let stub_key = forest.add_member(Member::new(MemberSpec {
            p0_mm: finite3([105.0, 105.0, 114.0]).unwrap(),
            p1_mm: finite3([105.0, 105.0, 112.0]).unwrap(),
            r0_mm: TerminalEndRadius::new(positive(0.5)),
            r1_mm: RootwardEndRadius::new(positive(0.5)),
            kind: MemberKind::Stub,
            owner: Owner::Contact(7),
            model_contact_end: Some(ContactEnd::Terminalward),
            carried_mm2: finite(0.0).unwrap(),
        }));
        forest.register_terminal(7, stub_key).unwrap();
        forest.consume_terminal(7, stub_key).unwrap();

        // The seated arm: terminalward end at the stub's rootward end, rootward
        // end buried at the box centre. This is the member the exact pass
        // refuses.
        let arm_key = forest.add_member(Member::new(MemberSpec {
            p0_mm: finite3([105.0, 105.0, 112.0]).unwrap(),
            p1_mm: finite3([105.0, 105.0, 105.0]).unwrap(),
            r0_mm: TerminalEndRadius::new(positive(0.5)),
            r1_mm: RootwardEndRadius::new(positive(0.5)),
            kind: MemberKind::Strut,
            owner: Owner::Generated {
                family: "brace-test".into(),
                ordinal: 1,
            },
            model_contact_end: Some(ContactEnd::Rootward),
            carried_mm2: finite(0.0).unwrap(),
        }));
        forest.set_parent(stub_key, Some(arm_key)).unwrap();

        // NON-VACUITY 1 — the arm really is the refused member, and the stub
        // really is not. Without this the test could pass on an empty set.
        let config = BraceConfig::current(0.05).unwrap();
        let negatives =
            exact_clearance_negatives(&forest, &surface, &config, test_penetration_limits())
                .unwrap();
        assert_eq!(
            negatives.contact_refusals,
            vec![arm_key],
            "the seated arm must be the contact refusal this fixture is about"
        );
        assert_eq!(
            negatives.pipeline_violations, 0,
            "nothing here may be an unrefusable violation, or the run fails for a different reason"
        );

        // NON-VACUITY 2 — the arm has the child that used to make this fatal.
        assert_eq!(
            forest.children_of(arm_key),
            vec![stub_key],
            "the fixture must reproduce the parent/child pair, not a lone arm"
        );

        // REVERT PROOF — the arena's childless rule is UNCHANGED. If a future
        // edit "fixes" the Bat by deleting that rule instead of cascading, this
        // goes red, because the fix would then be a weakening of the invariant
        // that stops a promoted child shipping as a floating support member.
        assert_eq!(
            forest.clone().quarantine(arm_key),
            Err(ForestError::QuarantineHasChildren),
            "the per-member quarantine must still refuse a carrier"
        );

        // THE BAR — the controller returns a named outcome, not an error.
        let plan = brace_fixed_point_current_parity(
            &forest,
            &surface,
            &topology,
            finite(0.0).unwrap(),
            test_penetration_limits(),
            0.05,
            &[],
        )
        .expect(
            "OD-4: a refused genuine-contact member routes to quarantine, never a hard failure",
        );
        assert_eq!(plan.termination, BraceFixedPointTermination::ContactRefused);
        assert_eq!(plan.contact_refusals, vec![arm_key]);

        // The verdict reached BOTH members, because a condemned carrier takes
        // its dependant's only route to ground with it.
        assert!(
            plan.forest.member(arm_key).is_none() && plan.forest.member(stub_key).is_none(),
            "neither the refused arm nor the stub it carried may remain live"
        );
        assert!(
            plan.forest.quarantine_record(arm_key).is_some(),
            "the refused arm is condemned, not merely retired"
        );
        let stub_record = plan
            .forest
            .quarantine_record(stub_key)
            .expect("the cascaded stub carries an ordinary quarantine record");
        assert_eq!(
            stub_record.terminal_contacts,
            vec![7],
            "the record must name the terminal, which is how SPEC §6's removal finds it"
        );
        assert!(
            plan.forest.terminal_quarantine(7).is_some(),
            "the terminal's own lookup must resolve, or the `quarantined` noncoverage \
             component would report this contact as merely unrouted"
        );
    }

    /// The Stage A fixture: one registered, consumed terminal whose stub hangs
    /// off a model-seated arm buried in the box, built at a chosen `x`/`y` so
    /// several can share one forest.
    ///
    /// `arm_owner` is the whole point of the helper. Production mints every
    /// member of a terminal's own route with that terminal's provenance
    /// (`cco::apply_model_anchor` copies `request.connector_owner`, which
    /// `Forest::register_terminal` has already pinned to
    /// `Owner::Contact(terminal)`), while a bifurcation's upper half inherits
    /// the HOST's owner instead. Passing `Owner::Contact(terminal)` therefore
    /// builds "the terminal's own chosen arm"; passing anything else builds
    /// "somebody else's arm, with this stub hanging off it".
    fn seated_terminal_fixture(
        forest: &mut Forest,
        terminal: u32,
        xy: [f64; 2],
        arm_owner: Owner,
    ) -> (MemberKey, MemberKey) {
        let stub_key = forest.add_member(Member::new(MemberSpec {
            p0_mm: finite3([xy[0], xy[1], 114.0]).unwrap(),
            p1_mm: finite3([xy[0], xy[1], 112.0]).unwrap(),
            r0_mm: TerminalEndRadius::new(positive(0.5)),
            r1_mm: RootwardEndRadius::new(positive(0.5)),
            kind: MemberKind::Stub,
            owner: Owner::Contact(terminal),
            model_contact_end: Some(ContactEnd::Terminalward),
            carried_mm2: finite(0.0).unwrap(),
        }));
        forest.register_terminal(terminal, stub_key).unwrap();
        forest.consume_terminal(terminal, stub_key).unwrap();
        let arm_key = forest.add_member(Member::new(MemberSpec {
            p0_mm: finite3([xy[0], xy[1], 112.0]).unwrap(),
            p1_mm: finite3([xy[0], xy[1], 105.0]).unwrap(),
            r0_mm: TerminalEndRadius::new(positive(0.5)),
            r1_mm: RootwardEndRadius::new(positive(0.5)),
            kind: MemberKind::Strut,
            owner: arm_owner,
            model_contact_end: Some(ContactEnd::Rootward),
            carried_mm2: finite(0.0).unwrap(),
        }));
        forest.set_parent(stub_key, Some(arm_key)).unwrap();
        (stub_key, arm_key)
    }

    fn sealed_routed(terminal: u32) -> SealedTerminalDisposition {
        SealedTerminalDisposition {
            terminal_id: terminal,
            kind: FinalizedDispositionKind::Routed,
        }
    }

    /// **Stage A, the headline.** A sealed-`Routed` terminal whose OWN chosen
    /// arm is measured inside the model abandons the pass. Nothing is
    /// quarantined, the illegal terminal pair is never minted, and the refusal
    /// names the terminal, the arm, the millimetres and the radius.
    ///
    /// This is the Giant Bat's class. Before Stage A it swept, flipped the
    /// terminal's live state to `Quarantined` under a record still saying
    /// `Routed`, and died two stages later with `TerminalEvidenceMismatch` —
    /// an error that reads as a broken engine rather than as the physics.
    #[test]
    fn a_sealed_routed_terminals_own_arm_abandons_the_pass_instead_of_quarantining() {
        let surface = remote_box();
        let topology = remote_box_topology();
        let mut forest = Forest::new();
        let (stub_key, arm_key) =
            seated_terminal_fixture(&mut forest, 7, [105.0, 105.0], Owner::Contact(7));
        let before = forest.digest();

        // NON-VACUITY — with an EMPTY sealed set this exact forest sweeps, so
        // the difference below is Stage A and not the fixture.
        let swept = brace_fixed_point_current_parity(
            &forest,
            &surface,
            &topology,
            finite(0.0).unwrap(),
            test_penetration_limits(),
            0.05,
            &[],
        )
        .expect("with no sealed record the sweep applies exactly as it always did");
        assert_eq!(
            swept.termination,
            BraceFixedPointTermination::ContactRefused
        );
        assert!(swept.forest.member(stub_key).is_none());

        let capture = crate::coverage_repair::capture::RunLogCapture::install();
        let refusal = brace_fixed_point_current_parity(
            &forest,
            &surface,
            &topology,
            finite(0.0).unwrap(),
            test_penetration_limits(),
            0.05,
            &[sealed_routed(7)],
        )
        .expect_err("a sealed-routed terminal's own condemned arm abandons the pass");
        let lines = capture.lines();

        let BraceError::SealedRoutedTerminalCondemned(class) = refusal else {
            panic!("the refusal must be the steerable one, not: {refusal}");
        };
        assert_eq!(
            class.disturbed.len(),
            1,
            "one sealed terminal is disturbed by this sweep"
        );
        let disturbed = &class.disturbed[0];
        assert_eq!(disturbed.terminal_id, 7);
        assert_eq!(disturbed.sealed_kind, FinalizedDispositionKind::Routed);
        assert_eq!(disturbed.terminal_member, stub_key);
        assert_eq!(
            disturbed.own_chosen_arms,
            vec![arm_key],
            "the arm the terminal's own tournament chose is what was measured illegal"
        );
        assert!(
            disturbed.cascade_carriers.is_empty(),
            "nothing else took this terminal down"
        );

        // The physics travels with the refusal, in millimetres.
        let row = class
            .measured_refusals
            .iter()
            .find(|row| row.member == arm_key)
            .expect("the refused arm's measured gap must travel with the class");
        assert!(
            row.gap_mm < 0.0 && row.gap_mm.is_finite(),
            "the refusal names how far inside the model the arm is"
        );
        assert_eq!((row.terminal_radius_mm, row.rootward_radius_mm), (0.5, 0.5));

        // NOTHING WAS TOUCHED. The input forest is borrowed, so this also
        // pins that the classifier's read-only query stayed read-only.
        assert_eq!(
            forest.digest(),
            before,
            "an abandoned pass must leave the forest exactly as it found it"
        );
        assert!(
            forest.member(arm_key).is_some() && forest.member(stub_key).is_some(),
            "neither member may be condemned by an abandoned pass"
        );
        assert!(
            forest.quarantine_record(arm_key).is_none(),
            "no quarantine record may be written by an abandoned pass"
        );

        assert!(
            lines
                .iter()
                .any(|line| line.contains("terminal 7") && line.contains("own_arms=")),
            "the class must be streamed while the run is happening: {lines:?}"
        );
    }

    /// **The cascade-victim rule, and it is a correction rather than a nicety.**
    ///
    /// A terminal can be disturbed purely by cascade: its stub re-parented onto
    /// somebody else's arm, and that arm condemned. Its own arm was never
    /// measured illegal, so it has nothing to steer with and must NOT be given
    /// a no-good. Without this rule the scheme punishes the innocent — a
    /// victim would lose one legal arm per pass for another member's failure
    /// and retire in the end with the ledger arithmetically perfect.
    ///
    /// The pass therefore refuses as "nothing learned" rather than as the
    /// steerable class, and the victim is reported as a victim.
    #[test]
    fn a_terminal_taken_down_by_someone_elses_arm_is_reported_as_a_cascade_victim() {
        let surface = remote_box();
        let topology = remote_box_topology();
        let mut forest = Forest::new();
        let (stub_key, arm_key) = seated_terminal_fixture(
            &mut forest,
            7,
            [105.0, 105.0],
            Owner::Generated {
                family: "brace-test".into(),
                ordinal: 1,
            },
        );

        let refusal = brace_fixed_point_current_parity(
            &forest,
            &surface,
            &topology,
            finite(0.0).unwrap(),
            test_penetration_limits(),
            0.05,
            &[sealed_routed(7)],
        )
        .expect_err("a disturbed sealed terminal still abandons the pass");

        let BraceError::SealedSweepLearnedNothing(class) = refusal else {
            panic!(
                "no sealed-routed terminal's OWN arm was condemned here, so nothing is steerable"
            );
        };
        assert_eq!(class.disturbed.len(), 1);
        let disturbed = &class.disturbed[0];
        assert_eq!(disturbed.terminal_id, 7);
        assert_eq!(disturbed.terminal_member, stub_key);
        assert!(
            disturbed.own_chosen_arms.is_empty(),
            "the victim's OWN arm was never measured illegal, so it gets no no-good"
        );
        assert_eq!(
            disturbed.cascade_carriers,
            vec![arm_key],
            "the arm that took it down is named as a carrier, not as its own choice"
        );
    }

    /// **A rewire connector that carries only this terminal IS this terminal's
    /// own arm, whatever its owner field says.**
    ///
    /// # Why this test has to exist
    ///
    /// `cco`'s rewire mints a connector owning `Owner::Contact(root.slot())` —
    /// a MEMBER slot in the terminal-id namespace. So a member rewire minted
    /// for this terminal's own route wears an owner that names no terminal at
    /// all, and the owner test alone calls it somebody else's arm.
    ///
    /// **MEASURED, and it blocked a real model.** On the Deinonychus, after one
    /// steering pass, terminal 113's route ended at `slot501g0` owned by
    /// `Contact(401)` — 401 being a member slot and no sealed terminal. The
    /// carrier's subtree held exactly one terminal stub: 113's own. The owner
    /// test classified it as a cascade victim, so the pass learned nothing, and
    /// the run refused `SealedSweepLearnedNothing` — a refusal that reads
    /// exactly like a model whose physics cannot be satisfied, produced by a
    /// namespace collision.
    ///
    /// The test is §1.3's own definition applied physically: *"its stub
    /// re-parented onto SOMEBODY ELSE'S arm"*. An arm is somebody else's when
    /// somebody else hangs off it.
    #[test]
    fn a_rewire_connector_carrying_only_this_terminal_is_its_own_arm_not_a_carrier() {
        let surface = remote_box();
        let topology = remote_box_topology();
        let mut forest = Forest::new();
        // Owner names slot 401 — a MEMBER slot, and no terminal the record
        // seals. This is exactly what `cco.rs`'s rewire mints.
        let (stub_key, arm_key) =
            seated_terminal_fixture(&mut forest, 7, [105.0, 105.0], Owner::Contact(401));

        let refusal = brace_fixed_point_current_parity(
            &forest,
            &surface,
            &topology,
            finite(0.0).unwrap(),
            test_penetration_limits(),
            0.05,
            &[sealed_routed(7)],
        )
        .expect_err("a sealed-routed terminal's own condemned arm abandons the pass");

        let BraceError::SealedRoutedTerminalCondemned(class) = refusal else {
            panic!(
                "the arm carries terminal 7 and nobody else, so it is terminal 7's OWN arm and \
                 the refusal must be the steerable one, not: {refusal}"
            );
        };
        let disturbed = &class.disturbed[0];
        assert_eq!(disturbed.terminal_member, stub_key);
        assert_eq!(
            disturbed.own_chosen_arms,
            vec![arm_key],
            "a connector whose subtree holds only this terminal's stub cannot be another \
             terminal's arm, whatever its owner field says"
        );
        assert!(disturbed.cascade_carriers.is_empty());
    }

    /// **The other side of the same rule, and it is the one that keeps the
    /// victim protection intact.** A member whose owner honestly names
    /// somebody — a generated brace rung, a model anchor, another sealed
    /// terminal — stays a carrier even when it carries only this terminal. The
    /// reclassification above is for exactly one defect: an owner that names no
    /// terminal at all.
    ///
    /// Widen it to "any owner that is not `Contact(this)`" and every generated
    /// rung becomes some terminal's own arm, which is the punish-the-innocent
    /// harm §1.3 spends three paragraphs forbidding. This test stands on that
    /// widening, and
    /// `a_terminal_taken_down_by_someone_elses_arm_is_reported_as_a_cascade_victim`
    /// is its sibling — that one went red the first time the rule was written
    /// without this condition.
    #[test]
    fn a_generated_rung_carrying_only_this_terminal_is_still_a_carrier() {
        let surface = remote_box();
        let topology = remote_box_topology();
        let mut forest = Forest::new();
        let (_stub, arm_key) =
            seated_terminal_fixture(&mut forest, 7, [105.0, 105.0], Owner::ModelAnchor(9));

        let refusal = brace_fixed_point_current_parity(
            &forest,
            &surface,
            &topology,
            finite(0.0).unwrap(),
            test_penetration_limits(),
            0.05,
            &[sealed_routed(7)],
        )
        .expect_err("a disturbed sealed terminal still abandons the pass");

        let BraceError::SealedSweepLearnedNothing(class) = refusal else {
            panic!("an owner that honestly names somebody is not this terminal's own arm");
        };
        assert_eq!(class.disturbed[0].cascade_carriers, vec![arm_key]);
        assert!(class.disturbed[0].own_chosen_arms.is_empty());
    }

    /// **The refutation's carve-out, unchanged.** A refused member that
    /// disturbs no terminal the sealed record names sweeps exactly as it always
    /// did, even with a sealed record present. This is the property every model
    /// whose brace refusals touch no contact accounting depends on, and it is
    /// what makes Stage A's blast radius a claim a corpus run can prove.
    #[test]
    fn a_refusal_that_disturbs_no_sealed_terminal_sweeps_exactly_as_it_always_did() {
        let surface = remote_box();
        let topology = remote_box_topology();
        let mut forest = Forest::new();

        // A sealed terminal that the sweep never reaches: its stub is clear of
        // the box and hangs off nothing.
        let untouched_stub = forest.add_member(Member::new(MemberSpec {
            p0_mm: finite3([102.0, 102.0, 118.0]).unwrap(),
            p1_mm: finite3([102.0, 102.0, 116.0]).unwrap(),
            r0_mm: TerminalEndRadius::new(positive(0.5)),
            r1_mm: RootwardEndRadius::new(positive(0.5)),
            kind: MemberKind::Stub,
            owner: Owner::Contact(7),
            model_contact_end: Some(ContactEnd::Terminalward),
            carried_mm2: finite(0.0).unwrap(),
        }));
        forest.register_terminal(7, untouched_stub).unwrap();
        forest.consume_terminal(7, untouched_stub).unwrap();

        // A childless model-seated arm buried in the box: refused, carrying no
        // terminal of its own.
        let lone_arm = forest.add_member(Member::new(MemberSpec {
            p0_mm: finite3([105.0, 105.0, 112.0]).unwrap(),
            p1_mm: finite3([105.0, 105.0, 105.0]).unwrap(),
            r0_mm: TerminalEndRadius::new(positive(0.5)),
            r1_mm: RootwardEndRadius::new(positive(0.5)),
            kind: MemberKind::Strut,
            owner: Owner::Generated {
                family: "brace-test".into(),
                ordinal: 2,
            },
            model_contact_end: Some(ContactEnd::Rootward),
            carried_mm2: finite(0.0).unwrap(),
        }));

        let plan = brace_fixed_point_current_parity(
            &forest,
            &surface,
            &topology,
            finite(0.0).unwrap(),
            test_penetration_limits(),
            0.05,
            &[sealed_routed(7)],
        )
        .expect("an empty intersection applies the sweep, sealed record or not");
        assert_eq!(plan.termination, BraceFixedPointTermination::ContactRefused);
        assert_eq!(plan.contact_refusals, vec![lone_arm]);
        assert!(
            plan.forest.member(lone_arm).is_none(),
            "the refused arm is condemned exactly as it always was"
        );
        assert!(
            plan.forest.member(untouched_stub).is_some(),
            "the sealed terminal the sweep never reached must survive it"
        );
    }

    /// **The measurement-lane gap, closed.** `finalized_unrouted_terminals`
    /// returns on the FIRST bad row, so a run learned about one terminal at a
    /// time and the class could never be counted. Stage A's classification must
    /// report the WHOLE class.
    ///
    /// Sabotage-shaped: a classifier that stops at the first disturbed terminal
    /// — the shape the tripwire it replaces already has — goes red here.
    #[test]
    fn the_refusal_names_the_whole_mismatch_class_not_only_its_first_member() {
        let surface = remote_box();
        let topology = remote_box_topology();
        let mut forest = Forest::new();
        let (_, first_arm) =
            seated_terminal_fixture(&mut forest, 7, [105.0, 105.0], Owner::Contact(7));
        let (_, second_arm) =
            seated_terminal_fixture(&mut forest, 9, [103.0, 103.0], Owner::Contact(9));

        // NON-VACUITY — both arms really are refused by the exact pass.
        let config = BraceConfig::current(0.05).unwrap();
        let negatives =
            exact_clearance_negatives(&forest, &surface, &config, test_penetration_limits())
                .unwrap();
        assert_eq!(
            negatives.contact_refusals,
            vec![first_arm, second_arm],
            "both seated arms must be refusals, or the class cannot have two members"
        );

        let refusal = brace_fixed_point_current_parity(
            &forest,
            &surface,
            &topology,
            finite(0.0).unwrap(),
            test_penetration_limits(),
            0.05,
            &[sealed_routed(7), sealed_routed(9)],
        )
        .expect_err("two sealed-routed terminals lose their own arms in one sweep");

        let BraceError::SealedRoutedTerminalCondemned(class) = refusal else {
            panic!("both terminals lost their OWN arms, so this is the steerable class");
        };
        assert_eq!(
            class
                .disturbed
                .iter()
                .map(|terminal| terminal.terminal_id)
                .collect::<Vec<_>>(),
            vec![7, 9],
            "every disturbed terminal must be named, not only the first"
        );
        assert_eq!(class.disturbed[0].own_chosen_arms, vec![first_arm]);
        assert_eq!(class.disturbed[1].own_chosen_arms, vec![second_arm]);
        assert_eq!(
            class.measured_refusals.len(),
            2,
            "every measured refusal travels with the class, with its own millimetres"
        );

        // The rendered refusal names both, so an operator reading one line
        // learns the size of the class rather than one member of it.
        let rendered = class.to_string();
        assert!(
            rendered.contains("terminal 7") && rendered.contains("terminal 9"),
            "the rendered class must name every terminal: {rendered}"
        );
    }

    /// **The refusal-accounting rider (Craig, 2026-08-23).**
    ///
    /// `exact_clearance_rows` computes a real per-arm clearance in millimetres
    /// and, before this landed, only the SIGN of it survived a run. A run could
    /// say "one arm was refused" and never say whether that arm was 0.04 mm
    /// inside the model or 4 mm inside it — and the sub-0.3 mm residue is the
    /// one the seal decision waited on.
    ///
    /// This is sabotage-shaped on purpose. An implementation that reported a
    /// count and a placeholder magnitude, or that reported the sign as `-1`,
    /// passes a test that only asks "is it negative". So the assertion is
    /// EQUALITY with the number `exact_clearance_rows` measured for this exact
    /// member, taken independently in this test.
    #[test]
    fn the_final_pass_discloses_its_refusal_count_and_the_true_gaps_in_millimetres() {
        let surface = remote_box();
        let topology = remote_box_topology();
        let mut forest = Forest::new();

        let stub_key = forest.add_member(Member::new(MemberSpec {
            p0_mm: finite3([105.0, 105.0, 114.0]).unwrap(),
            p1_mm: finite3([105.0, 105.0, 112.0]).unwrap(),
            r0_mm: TerminalEndRadius::new(positive(0.5)),
            r1_mm: RootwardEndRadius::new(positive(0.5)),
            kind: MemberKind::Stub,
            owner: Owner::Contact(7),
            model_contact_end: Some(ContactEnd::Terminalward),
            carried_mm2: finite(0.0).unwrap(),
        }));
        forest.register_terminal(7, stub_key).unwrap();
        forest.consume_terminal(7, stub_key).unwrap();
        let arm_key = forest.add_member(Member::new(MemberSpec {
            p0_mm: finite3([105.0, 105.0, 112.0]).unwrap(),
            p1_mm: finite3([105.0, 105.0, 105.0]).unwrap(),
            r0_mm: TerminalEndRadius::new(positive(0.5)),
            r1_mm: RootwardEndRadius::new(positive(0.5)),
            kind: MemberKind::Strut,
            owner: Owner::Generated {
                family: "brace-test".into(),
                ordinal: 1,
            },
            model_contact_end: Some(ContactEnd::Rootward),
            carried_mm2: finite(0.0).unwrap(),
        }));
        forest.set_parent(stub_key, Some(arm_key)).unwrap();

        let capture = crate::coverage_repair::capture::RunLogCapture::install();
        let plan = brace_fixed_point_current_parity(
            &forest,
            &surface,
            &topology,
            finite(0.0).unwrap(),
            test_penetration_limits(),
            0.05,
            &[],
        )
        .expect("the refused contact routes to quarantine");
        let lines = capture.lines();

        assert_eq!(
            plan.final_pass_refusals.refused_arms, 1,
            "one contact-declaring member was measured inside the model"
        );
        assert_eq!(
            plan.final_pass_refusals.pipeline_violations, 0,
            "nothing here is an unrefusable violation"
        );
        let row = plan
            .final_pass_refusals
            .rows
            .iter()
            .find(|row| row.member == arm_key)
            .expect("the refused arm must carry a row of its own");
        assert!(row.refusable, "a contact-declaring refusal is refusable");
        assert!(
            !row.axis_transit_illegal,
            "this fixture's arm is buried in the box, so the DISTANCE path must write its \
             verdict — if the transit screen wrote it, the number below is a sentinel and the \
             test would be pinning the wrong thing"
        );
        assert_eq!(
            row.distance_clearance_mm, row.gap_mm,
            "on a distance refusal the two numbers are the same number"
        );

        // The independent measurement. `plan.forest` has already condemned the
        // arm, so the gap is re-read from the forest the controller graded:
        // the input forest, resized exactly as the final pass resized it.
        let config = BraceConfig::current(0.05).unwrap();
        let measured = exact_clearance_rows(&forest, &surface, &config, test_penetration_limits())
            .unwrap()
            .into_iter()
            .find(|(key, _)| *key == arm_key)
            .expect("the arm is graded by the exact pass")
            .1;
        assert_eq!(
            row.gap_mm, measured,
            "the disclosed gap must BE the measured clearance, not a sign or a placeholder"
        );
        assert!(
            row.gap_mm < 0.0 && row.gap_mm.is_finite(),
            "a refusal's true gap is a finite negative number of millimetres"
        );
        assert_eq!(
            (row.terminal_radius_mm, row.rootward_radius_mm),
            (0.5, 0.5),
            "the radii the resize assigned travel with the gap"
        );

        // LOUD, and streamed while the run is happening rather than only
        // returned to a caller that may never print it.
        let disclosure = lines
            .iter()
            .find(|line| line.starts_with("brace: final-pass refusals"))
            .expect("the final pass must stream its refusal accounting");
        assert!(
            disclosure.contains("refused_arms=1") && disclosure.contains("pipeline_violations=0"),
            "the streamed line must carry the counts: {disclosure}"
        );
        assert!(
            disclosure.contains("distance=1") && disclosure.contains("axis_transit=0"),
            "the streamed line must say WHICH screen refused, or a reader cannot tell a \
             measured shortfall from a transit sentinel: {disclosure}"
        );
        assert!(
            disclosure.contains(&format!("distance{measured:+.6}mm")),
            "the streamed line must carry the measured magnitude in millimetres: {disclosure}"
        );
    }

    /// **The correction that matters most in this rider, and it is
    /// sabotage-shaped.**
    ///
    /// `exact_clearance_rows` writes one signed number per member from TWO
    /// paths. The DISTANCE path writes a real shortfall against the 0.300 mm
    /// demand. The AXIS-TRANSIT screen takes a row the distance test found
    /// LEGAL — `axis_illegal_rows` filters to `clearance >= 0.0` — and stamps
    /// `min(clearance, -min_illegal_len_mm)`, i.e. exactly `-0.050 mm`, so one
    /// sign can carry the verdict.
    ///
    /// A disclosure that printed that `-0.050` as "the gap" would report a
    /// SENTINEL as a MEASUREMENT, and 0.050 mm is close enough to a plausible
    /// sub-0.3 mm residue that nobody would question it. That is exactly the
    /// mistake this lane made in its first ledger entry, on the Deinonychus,
    /// and this test is the guard against making it again.
    ///
    /// **The row here is constructed, and that is deliberate.** The transit
    /// path only fires for a member whose own seat faces are excluded from the
    /// distance query, so reproducing it geometrically needs a seated arm on a
    /// non-convex surface — the Deinonychus's terminal 113 is the real
    /// witness, recorded in `EMISSION-CANARY-LEDGER.md`. What was defective
    /// was the DISCLOSURE, and the disclosure is what this pins.
    ///
    /// **The coverage gap this leaves, stated rather than left for a reader to
    /// find.** `transit_illegal.insert(...)` in `exact_clearance_rows_detailed`
    /// has no unit witness for its TRUE branch. An always-true defect goes red
    /// in `the_final_pass_discloses_…`, which asserts its own fixture is a
    /// distance refusal; an always-FALSE defect — the flag never set, every
    /// transit sentinel printed as a gap again — is caught only by a real model
    /// run, because no fixture on a convex box can reach that path. Closing it
    /// needs a seated arm on a non-convex fixture, and that is worth doing.
    #[test]
    fn an_axis_transit_refusal_is_disclosed_as_a_sentinel_and_never_as_a_measured_gap() {
        let member = MemberKey::transit_test_fixture(401, 0);
        let transit = RefusalGapRow {
            member,
            // The verdict is the sentinel, `-min_illegal_len_mm`.
            gap_mm: -0.05,
            // The distance test PASSED this row by 0.32 mm. Reporting the
            // verdict as a gap would invert the physics.
            distance_clearance_mm: 0.32,
            axis_transit_illegal: true,
            terminal_radius_mm: 0.372_135_701_026_434_6,
            rootward_radius_mm: 0.372_135_701_026_434_6,
            refusable: true,
        };
        let distance = RefusalGapRow {
            member: MemberKey::transit_test_fixture(377, 2),
            gap_mm: -0.040_503,
            distance_clearance_mm: -0.040_503,
            axis_transit_illegal: false,
            terminal_radius_mm: 1.409_608,
            rootward_radius_mm: 1.409_608,
            refusable: true,
        };

        let rendered = transit.render();
        assert!(
            rendered.contains("axis-transit(distance_clearance=+0.320000mm)"),
            "a transit row must name its screen and carry the distance it PASSED: {rendered}"
        );
        assert!(
            !rendered.contains("-0.050000"),
            "the sentinel must never be printed as a magnitude: {rendered}"
        );

        let rendered_distance = distance.render();
        assert!(
            rendered_distance.contains("distance-0.040503mm"),
            "a distance row prints the shortfall it measured: {rendered_distance}"
        );

        // And the counts separate the two kinds, so a reader can size each
        // class without parsing the rows.
        let accounting = FinalPassRefusalAccounting {
            refused_arms: 2,
            pipeline_violations: 0,
            rows: vec![transit, distance],
        };
        let capture = crate::coverage_repair::capture::RunLogCapture::install();
        accounting.disclose(3);
        let line = capture
            .lines()
            .into_iter()
            .find(|line| line.starts_with("brace: final-pass refusals"))
            .expect("the accounting streams");
        assert!(
            line.contains("distance=1") && line.contains("axis_transit=1"),
            "the streamed counts must split the two refusal kinds: {line}"
        );
    }

    /// A pass with nothing negative streams nothing, and its accounting is
    /// empty rather than absent. The brace fixed point runs once per repair
    /// trial as well as at attempt 0 — 92 trials MEASURED on `zombie-3-a` — so
    /// a line per clean pass would bury the lines that carry a verdict.
    #[test]
    fn a_clean_final_pass_discloses_no_refusals_and_streams_no_line() {
        let surface = remote_box();
        let topology = remote_box_topology();
        let mut forest = Forest::new();
        forest.add_member(member(
            [105.0, 105.0, 130.0],
            [105.0, 105.0, 120.0],
            MemberKind::Trunk,
            0,
        ));

        let capture = crate::coverage_repair::capture::RunLogCapture::install();
        let plan = brace_fixed_point_current_parity(
            &forest,
            &surface,
            &topology,
            finite(0.0).unwrap(),
            test_penetration_limits(),
            0.05,
            &[],
        )
        .expect("a member clear of the model braces cleanly");
        assert_eq!(
            plan.final_pass_refusals,
            FinalPassRefusalAccounting::default(),
            "a clean pass has nothing to account for"
        );
        assert!(
            !capture
                .lines()
                .iter()
                .any(|line| line.starts_with("brace: final-pass refusals")),
            "a clean pass must not stream a refusal line"
        );
    }

    #[test]
    fn static_long_gap_activates_and_plans_cross_link_without_mutating_input() {
        let mut forest = Forest::new();
        let a = forest.add_member(member(
            [0.0, 0.0, 30.0],
            [0.0, 0.0, 0.0],
            MemberKind::Trunk,
            0,
        ));
        let _b = forest.add_member(member(
            [2.0, 0.0, 30.0],
            [2.0, 0.0, 0.0],
            MemberKind::Trunk,
            1,
        ));
        let before = forest.digest();
        let plate = finite(0.0).unwrap();
        let report = transient_report(&forest, plate).unwrap();
        assert!(report.member(a).unwrap().free_gaps_mm[0].hi_mm.get() >= 30.0);
        let plan = brace_pass_current_parity(
            &forest,
            &remote_box(),
            &remote_box_topology(),
            plate,
            &report,
            test_penetration_limits(),
            0.05,
        )
        .unwrap();
        assert_eq!(forest.digest(), before);
        assert!(plan.rungs_requested >= 1);
        assert!(plan.rungs_placed >= 1);
        assert!(plan
            .additions
            .iter()
            .any(|row| row.kind == BraceAdditionKind::CrossLink));
    }

    #[test]
    fn activation_span_maps_a_rung_across_the_winning_path() {
        let mut forest = Forest::new();
        let lower = forest.add_member(member(
            [0.0, 0.0, 10.0],
            [0.0, 0.0, 0.0],
            MemberKind::Trunk,
            0,
        ));
        let upper = forest.add_member(member(
            [0.0, 0.0, 20.0],
            [0.0, 0.0, 10.0],
            MemberKind::Shoot,
            1,
        ));
        forest.set_parent(upper, Some(lower)).unwrap();
        let report = TransientReport {
            members: vec![crate::transient::MemberTransientEvidence {
                member: lower,
                worst_span_mm: finite(20.0).unwrap(),
                restraint_zs_mm: vec![finite(0.0).unwrap()],
                free_gaps_mm: vec![ZInterval {
                    lo_mm: finite(0.0).unwrap(),
                    hi_mm: finite(4.0).unwrap(),
                }],
                n_terminals: 1,
                worst_span_interval_mm: ZInterval {
                    lo_mm: finite(0.0).unwrap(),
                    hi_mm: finite(20.0).unwrap(),
                },
                worst_span_path: vec![lower, upper],
            }],
        };
        let batch = compute_needs(&forest, &report, &BraceConfig::current(0.05).unwrap()).unwrap();
        assert_eq!(batch.needs.len(), 1);
        assert!(batch.needs[0].path_mapped);
        assert_eq!(batch.needs[0].triggers, vec![lower]);
        assert_eq!(batch.needs[0].z_mm, 10.0);
    }

    /// Task #25 — a clearance-triggered brace request must be aimed at the span
    /// that sets the violated member's radius, not at the violated member.
    ///
    /// `resize_forest_current_parity` sizes a member from `worst_span_mm`, a
    /// property of that member's whole root-to-terminal PATH. The span to
    /// shorten therefore usually lies on some other member of the path, so a
    /// rung placed inside the violated member's own free gap cannot move the
    /// sizing driver — which is how the Giant Bat's member 466 asked three
    /// times for relief and got a request no candidate could ever serve.
    ///
    /// The fixture separates the two answers by construction: the violated
    /// member `upper` owns the free gap, and the worst-span interval lies
    /// wholly inside `lower`. The old code hosted the need on `upper`; the
    /// driver is on `lower`.
    #[test]
    fn a_clearance_need_is_hosted_on_the_span_that_sets_the_radius() {
        // `remote_box` spans 100..110 on every axis. `lower` sits below it with
        // room to spare, `upper` runs up into it, so exactly one member reads
        // negative exact clearance.
        let surface = remote_box();
        let config = BraceConfig::current(0.05).unwrap();
        let mut forest = Forest::new();
        let lower = forest.add_member(member(
            [105.0, 105.0, 98.5],
            [105.0, 105.0, 95.0],
            MemberKind::Trunk,
            0,
        ));
        let upper = forest.add_member(member(
            [105.0, 105.0, 105.0],
            [105.0, 105.0, 98.5],
            MemberKind::Shoot,
            1,
        ));
        forest.set_parent(upper, Some(lower)).unwrap();

        // Non-vacuity: the fixture really does violate, and only on `upper`.
        assert_eq!(
            exact_clearance_violated_members(&forest, &surface, &config, test_penetration_limits())
                .unwrap(),
            vec![upper],
            "the fixture must put exactly the upper member inside the model"
        );

        let report = TransientReport {
            members: vec![crate::transient::MemberTransientEvidence {
                member: upper,
                worst_span_mm: finite(10.0).unwrap(),
                restraint_zs_mm: Vec::new(),
                // The violated member's own free gap — the site the old code
                // asked for, whose midpoint is 101.5.
                free_gaps_mm: vec![ZInterval {
                    lo_mm: finite(99.0).unwrap(),
                    hi_mm: finite(104.0).unwrap(),
                }],
                n_terminals: 1,
                // The span that actually sets the radius, lying wholly inside
                // `lower`; its midpoint is 96.5.
                worst_span_interval_mm: ZInterval {
                    lo_mm: finite(95.0).unwrap(),
                    hi_mm: finite(98.0).unwrap(),
                },
                worst_span_path: vec![lower, upper],
            }],
        };

        let mut batch = NeedBatch {
            requested: 0,
            needs: Vec::new(),
        };
        append_clearance_needs(
            &forest,
            &surface,
            &report,
            &config,
            test_penetration_limits(),
            &mut batch,
        )
        .unwrap();

        assert_eq!(batch.requested, 1);
        assert_eq!(batch.needs.len(), 1);
        let need = &batch.needs[0];
        assert_eq!(
            need.host, lower,
            "the rung must be asked for on the member carrying the radius-driving span"
        );
        assert_eq!(need.z_mm, 96.5, "the midpoint of the worst-span interval");
        assert!(
            need.path_mapped,
            "a path-mapped need is the only kind gather_candidates widens to the reach tiers for"
        );
        assert_eq!(
            need.triggers,
            vec![upper],
            "the violated member is the trigger, so the controller keeps working \
             sites until its clearance is non-negative again"
        );
        assert!(
            !need.alternatives.is_empty(),
            "the rest of the interval must be offered as fallback sites"
        );
        assert!(
            need.alternatives
                .iter()
                .all(|(key, _)| *key == lower || *key == upper),
            "every fallback site must lie on the worst-span path"
        );
    }

    #[test]
    fn model_rung_fallback_uses_real_surface_candidates() {
        let mut forest = Forest::new();
        let host = forest.add_member(member(
            [0.0, 0.0, 20.0],
            [0.0, 0.0, 0.0],
            MemberKind::Trunk,
            0,
        ));
        let needs = vec![Need {
            host,
            z_mm: 10.0,
            rung_index: 0,
            path_mapped: false,
            triggers: Vec::new(),
            alternatives: Vec::new(),
        }];
        let surface = near_wall_box();
        let topology = near_wall_box_topology();
        let config = BraceConfig::current(0.05).unwrap();
        let candidates = gather_candidates(&forest, &surface, &topology, &needs, &config)
            .unwrap()
            .candidates;
        let model = candidates
            .iter()
            .find(|row| row.kind == BraceAdditionKind::ModelRung)
            .expect("the real wall offers a bounded horizontal model rung");
        assert_eq!(model.rods.len(), 1);
        assert!((model.rods[0].p0[2] - model.rods[0].p1[2]).abs() <= HEIGHT_TOLERANCE_MM);
        assert!(model.rods[0].p1[0] >= 4.0);
    }

    /// **MAJOR-6, RE-AIMED BY #36 STEP 5 — the seating distinction is real, and
    /// the exact pass is now INDEPENDENT of it.**
    ///
    /// This test used to assert an artifact of the retired rule: that a correct
    /// PR-2 mask placed the exact pass's first tested station so the kernel read
    /// exactly `MARGIN`, and that including the `h` term for an unseated end
    /// pushed it 0.3 mm further out. That number existed only because the mask
    /// was handed to the kernel as an INSET. Step 5 stopped doing that — the
    /// exact pass tests the FULL axis and exempts a frozen face SET — so the
    /// station the mask used to place is no longer a thing the exact pass has.
    ///
    /// MAJOR-6's actual rule is untouched and is asserted here at the reader it
    /// still has: `contact::seating_of` records each family's seating state, and
    /// `placed_mask_inset_mm` carries the depth term for a seated end and not
    /// for an unseated one. That mask still feeds PR-3 emergence and
    /// `axis_illegal_rows` (design §5), so getting it wrong still matters.
    ///
    /// What step 5 adds is the second half, and it is the stronger statement:
    /// **the same geometry, the same frozen zone, graded once as an unseated
    /// family and once as a seated one, now reads the SAME clearance.** Under
    /// the retired rule those two rows differed by `h/|cos|`, because the mask
    /// moved the tested stations. Under the honest rule the tested stations are
    /// the whole axis for both and the tested faces are `surface \ Z` for both,
    /// so nothing about seating can move the distance verdict.
    #[test]
    fn the_exact_pass_no_longer_reads_the_seat_depth_that_the_mask_still_carries() {
        let config = BraceConfig::current(0.05).unwrap();
        let policy = config.contact_policy(contact::ClearanceStage::Final);

        // HALF ONE — the mask distinction survives, at the reader it still has.
        let unseated = contact::placed_mask_inset_mm(
            contact::seating_of(
                &MemberKind::Other("cco_upper".to_owned()),
                ContactEnd::Rootward,
            ),
            1.0,
            0.65,
            0.65,
            &policy,
        )
        .unwrap();
        let seated = contact::placed_mask_inset_mm(
            contact::seating_of(&MemberKind::Strut, ContactEnd::Rootward),
            1.0,
            0.65,
            0.65,
            &policy,
        )
        .unwrap();
        assert!(
            (seated - unseated - config.contact_depth_mm).abs() < 1.0e-9,
            "MAJOR-6: at |cos| = 1 a SEATED end's mask must carry exactly one \
             seat depth ({}) more than an unseated one's — the mask reflects the \
             member's actual seating state. unseated = {unseated}, seated = \
             {seated}",
            config.contact_depth_mm
        );

        // HALF TWO — and the exact pass no longer reads that difference.
        //
        // One geometry, one frozen zone, two families. `cco_upper` is genuinely
        // unseated (a bifurcation upper half); `Strut` + `Rootward` is the
        // PR-7-seated CCO model anchor. `seating_of` separates them, so under
        // the retired rule the two rows' masks — and therefore their tested
        // station sets, and therefore their clearances — differed.
        let surface = flat_table();
        let topology = flat_table_topology();
        let p0 = [0.0, 0.0, 6.0];
        let p1 = [0.0, 0.0, 0.0];
        let (_, _, face_ids, _) = surface.project(&[p1]);
        let seat_face_id = u32::try_from(face_ids[0]).expect("the seat projects onto a face");
        let zone = crate::seat_zone::freeze_production_seat_zones(
            &surface,
            &topology,
            &[crate::seat_zone::SeatZoneRequest {
                p0_mm: p0,
                p1_mm: p1,
                seat_face_id,
                band_radius_mm: 0.5 + config.standoff_mm,
            }],
        )
        .expect("the seat zone freezes")
        .remove(0);

        let graded = |kind: MemberKind| {
            let mut forest = Forest::new();
            let key = forest.add_member(contact_member(p0, p1, kind, ContactEnd::Rootward, 0));
            forest.set_seat_zone(key, zone.clone());
            let rows = exact_clearance_rows(&forest, &surface, &config, test_penetration_limits())
                .unwrap();
            rows.iter()
                .copied()
                .find(|(row_key, _)| *row_key == key)
                .expect("the member is a live non-stub row")
                .1
        };
        let unseated_clearance = graded(MemberKind::Other("cco_upper".to_owned()));
        let seated_clearance = graded(MemberKind::Strut);

        assert_eq!(
            unseated_clearance, seated_clearance,
            "#36 step 5: the exact pass tests the WHOLE axis against \
             `surface \\ Z` for both rows, so the seating state — which used to \
             move the mask, and with it the first tested station — cannot move \
             the distance verdict any more. unseated = {unseated_clearance}, \
             seated = {seated_clearance}"
        );
        assert!(
            seated_clearance >= 0.0,
            "NON-VACUITY: with its own seat forgiven the arm must be LEGAL, or \
             this test is comparing two refusals and would pass on a rule that \
             refused everything. Kernel said {seated_clearance}"
        );
        println!(
            "SEATING-INDEPENDENT-EXACT-PASS: unseated={unseated_clearance}  \
             seated={seated_clearance}  mask_unseated={unseated}  mask_seated={seated}"
        );
    }

    /// The seating classifier is keyed on the DESIGN FAMILY plus the declaring
    /// END (OD-3), recomputed per stage and never stored.
    ///
    /// **THIS TEST MOVED WITH PR-7, AND ITS MOVEMENT IS THE CHANGE'S
    /// SIGNATURE.** It used to assert that `MemberKind::Strut` and
    /// `Other("cco_upper")` are `OnSurface` — that was the single most explicit
    /// pin of "PR-7 is not landed". `cco::apply_model_anchor` now seats the
    /// strut's rootward end with `contact::seat_endpoint`, so a `Rootward`
    /// `Strut` contact is `Seated`.
    ///
    /// Everything else in the table is UNCHANGED, and each row is load-bearing:
    ///
    /// * a `Terminalward` `Strut` contact stays `OnSurface`. #52 (the initial
    ///   contact stub's designed embed) is a different rule and is not in this
    ///   commit; classifying that end `Seated` would add an `h` term to a mask
    ///   for a depth no `seat_endpoint` produced;
    /// * `Other("cco_upper")` stays `OnSurface` at both ends. A bifurcation
    ///   upper half inherits its HOST's kind, so a bifurcated model anchor is
    ///   still a `Strut` and still classified `Seated` by the `Strut` row —
    ///   `cco_upper` is not the family that carries a model seat;
    /// * every non-strut kind stays `OnSurface`, because no other family calls
    ///   `seat_endpoint`.
    #[test]
    fn seating_follows_the_design_family() {
        use crate::contact::{seating_of, ContactSeating, MODEL_RUNG_FAMILY};
        // The brace model rung is seated at either end (PR-1).
        for end in [ContactEnd::Rootward, ContactEnd::Terminalward] {
            assert_eq!(
                seating_of(&MemberKind::Other(MODEL_RUNG_FAMILY.to_owned()), end),
                ContactSeating::Seated
            );
        }
        // PR-7 — the CCO model anchor, at the end that carries its seat.
        assert_eq!(
            seating_of(&MemberKind::Strut, ContactEnd::Rootward),
            ContactSeating::Seated,
            "PR-7 seats the CCO model anchor's rootward end"
        );
        // ... and only at that end.
        assert_eq!(
            seating_of(&MemberKind::Strut, ContactEnd::Terminalward),
            ContactSeating::OnSurface,
            "a terminalward strut contact is the #52 designed embed, not a PR-1 seat"
        );
        for kind in [
            MemberKind::Brace,
            MemberKind::Shoot,
            MemberKind::Trunk,
            MemberKind::Stub,
            MemberKind::Other("cco_upper".to_owned()),
        ] {
            for end in [ContactEnd::Rootward, ContactEnd::Terminalward] {
                assert_eq!(
                    seating_of(&kind, end),
                    ContactSeating::OnSurface,
                    "{kind:?} at {end:?} is not seated by PR-1"
                );
            }
        }
        // The depth term is present for a seated member and absent otherwise.
        assert!((ContactSeating::Seated.mask_seat_depth_mm(0.3) - 0.3).abs() < 1e-15);
        assert!(ContactSeating::OnSurface.mask_seat_depth_mm(0.3).abs() < 1e-15);
    }

    /// BLOCKER-3 regression: PR-1 seating must not silently delete the whole
    /// model-rung family on non-horizontal faces.
    ///
    /// Printability is evaluated at the UNSEATED projection point.  The
    /// historic predicate compared `p0[2]` against `projected.point[2]`, which
    /// `axis_point_at_z` sets bit-identically, so it constrained NOTHING.
    /// Re-pointing it at the seated endpoint would have converted that
    /// tautology into `|normal_z| * h <= 1e-9` — a new, family-killing rule
    /// arriving as an accident of seating rather than as measured physics.
    /// PR-3 is the explicit incidence gate for this family.
    ///
    /// The second assertion is what makes this test discriminating: the seat
    /// really does displace the contact point in z here, so the rejected
    /// alternative would have failed.
    #[test]
    fn a_seated_rung_on_a_raked_wall_stays_printable() {
        let mut forest = Forest::new();
        let host = forest.add_member(member(
            [0.0, 0.0, 20.0],
            [0.0, 0.0, 0.0],
            MemberKind::Trunk,
            0,
        ));
        let needs = vec![Need {
            host,
            z_mm: 10.0,
            rung_index: 0,
            path_mapped: false,
            triggers: Vec::new(),
            alternatives: Vec::new(),
        }];
        let surface = raked_wall_box();
        let topology = raked_wall_box_topology();
        let config = BraceConfig::current(0.05).unwrap();
        let candidates = gather_candidates(&forest, &surface, &topology, &needs, &config)
            .unwrap()
            .candidates;
        let model = candidates
            .iter()
            .find(|row| row.kind == BraceAdditionKind::ModelRung)
            .expect("the raked wall offers an admissible model rung");
        assert!(
            model.printable,
            "a seated rung on a raked wall must stay printable; \
             printability is evaluated at the unseated projection point"
        );
        let rod = &model.rods[0];
        assert!(
            (rod.p0[2] - rod.p1[2]).abs() > 1.0e-3,
            "fixture is not discriminating: the seat displaced z by only {} mm",
            (rod.p0[2] - rod.p1[2]).abs()
        );
    }

    #[test]
    fn thread_sweep_is_nonvacuous_and_digest_identical() {
        const CHILD: &str = "PLINTH_BRACE_SWEEP_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let expected = std::env::var("PLINTH_THREADS")
                .unwrap()
                .parse::<usize>()
                .unwrap();
            let probe = geom_kernel::point_segment_distances_batch(
                &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
                &[[0.0, 0.0, 0.0]],
                &[[0.0, 0.0, 1.0]],
                geom_kernel::PointSegmentBatchLimits::new(4, 16),
            )
            .unwrap();
            assert_eq!(probe.effective_threads(), expected);
            let mut forest = Forest::new();
            forest.add_member(member(
                [0.0, 0.0, 30.0],
                [0.0, 0.0, 0.0],
                MemberKind::Trunk,
                0,
            ));
            forest.add_member(member(
                [2.0, 0.0, 30.0],
                [2.0, 0.0, 0.0],
                MemberKind::Trunk,
                1,
            ));
            let plate = finite(0.0).unwrap();
            let report = transient_report(&forest, plate).unwrap();
            let plan = brace_pass_current_parity(
                &forest,
                &remote_box(),
                &remote_box_topology(),
                plate,
                &report,
                test_penetration_limits(),
                0.05,
            )
            .unwrap();
            assert!(!plan.additions.is_empty());
            println!(
                "BRACE_SWEEP={:016x}:{}",
                plan.forest_with_braces.digest(),
                expected
            );
            return;
        }
        let test_name = "brace::tests::thread_sweep_is_nonvacuous_and_digest_identical";
        let mut digests = Vec::new();
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
                .find_map(|line| line.strip_prefix("BRACE_SWEEP="))
                .expect("child emitted a nonvacuous brace digest");
            let (digest, effective) = row.split_once(':').unwrap();
            assert_eq!(effective.parse::<usize>().unwrap(), threads);
            digests.push(digest.to_owned());
        }
        assert!(digests.windows(2).all(|pair| pair[0] == pair[1]));
    }
}
