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
use serde::Serialize;
use thiserror::Error;

use crate::{
    anchor::{FOOT_BREACH_CONE_RAYS, FOOT_BREACH_HALF_DEG},
    contact::{self, ContactPolicy},
    designed_embed,
    forest::{
        ContactEnd, Forest, ForestError, Member, MemberKey, MemberKind, MemberSpec,
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
    standoff_mm: f64,
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
    /// The PR-1..PR-4 policy this brace pass evaluates contacts under.
    fn contact_policy(&self) -> ContactPolicy {
        ContactPolicy {
            floor_depth_mm: self.contact_depth_mm,
            process_resolution_mm: self.process_resolution_mm,
            standoff_mm: self.standoff_mm,
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
}

/// Port of the live `compute_needs` plus candidate placement, without mutating
/// caller state. The supplied report is checked against a fresh canonical
/// report so stale snapshots cannot authorize geometry.
pub(crate) fn brace_pass_current_parity(
    forest: &Forest,
    surface: &NativeSurface,
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
    } = gather_candidates(forest, surface, &need_batch.needs, &config)?;
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
pub(crate) fn brace_fixed_point_current_parity(
    forest: &Forest,
    surface: &NativeSurface,
    plate_z_mm: Finite,
    penetration_limits: PenetrationBatchLimits,
    process_resolution_mm: f64,
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
        let added = !plan.additions.is_empty();
        additions.extend(plan.additions);
        let negative = exact_clearance_negatives(
            &working,
            surface,
            &BraceConfig::current(process_resolution_mm)?,
            penetration_limits,
        )?;
        if !added && !changed && negative.is_clean() {
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
            });
        }
        if iteration == FROZEN_PASS_CAP {
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
            for key in &negative.contact_refusals {
                working.quarantine(*key)?;
            }
            return Ok(BraceFixedPointPlan {
                forest: working,
                additions,
                iterations: iteration,
                termination,
                contact_refusals: negative.contact_refusals,
                contact_attestation_digest,
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
        additions.push(BraceAddition {
            key,
            kind: candidate.kind,
            member: spec,
        });
    }
    Ok(())
}

fn pending_triggers_after_trial(
    forest: &Forest,
    surface: &NativeSurface,
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
        });
    }
    // Shared triggers use one complete resize and one exact-clearance batch.
    // The immutable trial is common to every trigger because the forest does
    // not change while their relief verdicts are scattered.
    let resize = resize_forest_current_parity(forest, plate_z_mm)?;
    let trial = apply_resize(forest.clone(), &resize)?;
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
        let (_, projected, _, normals) = surface.project(&probes);
        for ((raw, p1), normal) in raw_model.into_iter().zip(projected).zip(normals) {
            if (p1[2] - raw.z_mm).abs() > config.model_rung_z_tolerance_mm + HEIGHT_TOLERANCE_MM {
                continue;
            }
            projected_model.push(ProjectedModelCandidate {
                raw,
                point: p1,
                normal,
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
        let policy = config.contact_policy();
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
                // over-masked the segment took `seg_clear_one`'s INFINITY
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
        model_candidates.push(PlacementCandidate {
            need_index: projected.raw.need_index,
            site_index: projected.raw.site_index,
            rank: projected.raw.rank,
            kind: BraceAdditionKind::ModelRung,
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
    for (candidate, attested) in model_candidates
        .into_iter()
        .zip(&attestation.attested_by_row)
    {
        if *attested {
            candidates.push(candidate);
        }
    }
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
    let policy = config.contact_policy();
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
    let mut inset0 = Vec::new();
    let mut inset1 = Vec::new();
    let mut ranges = Vec::with_capacity(candidates.len());
    #[cfg(feature = "census")]
    let mut census_cos_class = Vec::new();
    for candidate in candidates {
        let start = p0.len();
        for rod in &candidate.rods {
            p0.push(rod.p0);
            p1.push(rod.p1);
            r0.push(rod.radius.get());
            r1.push(rod.radius.get());
            inset0.push(rod.inset0);
            inset1.push(rod.inset1);
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
    crate::census_record!(crate::census::record_segment_batch(
        "brace_candidate_clearance",
        &p0,
        &p1,
        &inset0,
        &inset1,
        &census_cos_class,
    ));
    let clearances = if p0.is_empty() {
        Vec::new()
    } else {
        surface
            .segment_clearance(&p0, &p1, &r0, &r1, config.standoff_mm, &inset0, &inset1)
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
}

impl ExactClearanceNegatives {
    const fn is_clean(&self) -> bool {
        self.pipeline_violations == 0 && self.contact_refusals.is_empty()
    }
}

fn exact_clearance_negatives(
    forest: &Forest,
    surface: &NativeSurface,
    config: &BraceConfig,
    penetration_limits: PenetrationBatchLimits,
) -> Result<ExactClearanceNegatives, BraceError> {
    let mut negatives = ExactClearanceNegatives::default();
    for (key, clearance) in exact_clearance_rows(forest, surface, config, penetration_limits)? {
        if clearance >= 0.0 {
            continue;
        }
        let declares_contact = forest
            .member(key)
            .is_some_and(|member| member.model_contact_end().is_some());
        if declares_contact {
            negatives.contact_refusals.push(key);
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

fn exact_clearance_rows(
    forest: &Forest,
    surface: &NativeSurface,
    config: &BraceConfig,
    penetration_limits: PenetrationBatchLimits,
) -> Result<Vec<(MemberKey, f64)>, BraceError> {
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
    let mut inset0 = vec![0.0; rows.len()];
    let mut inset1 = vec![0.0; rows.len()];
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
            let policy = config.contact_policy();
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
                ContactEnd::Terminalward => inset0[index] = inset,
                ContactEnd::Rootward => inset1[index] = inset,
            }
        }
    }
    crate::census_record!(crate::census::record_segment_batch(
        "brace_exact_member_clearance",
        &p0,
        &p1,
        &inset0,
        &inset1,
        &census_cos_class,
    ));
    let mut clearance = surface
        .segment_clearance(&p0, &p1, &r0, &r1, config.standoff_mm, &inset0, &inset1)
        .map_err(|error| BraceError::NativeQuery(error.to_string()))?
        .0;
    for index in axis_illegal_rows(
        surface,
        &p0,
        &p1,
        &r0,
        &r1,
        &inset0,
        &inset1,
        &clearance,
        config,
        penetration_limits,
    )? {
        // Exact-clearance consumers need a single sign verdict. Preserve any
        // harder distance violation and encode belt-only transit as negative.
        clearance[index] = clearance[index].min(-config.min_illegal_len_mm);
    }
    Ok(rows
        .into_iter()
        .zip(clearance)
        .map(|((key, _), value)| (key, value))
        .collect())
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
    fn flat_table() -> NativeSurface {
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

    fn remote_box() -> NativeSurface {
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

    fn near_wall_box() -> NativeSurface {
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
    fn raked_wall_box() -> NativeSurface {
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

    fn enclosing_box() -> NativeSurface {
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
        let policy = config.contact_policy();
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
            finite(0.0).unwrap(),
            test_penetration_limits(),
            0.05,
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
        let config = BraceConfig::current(0.05).unwrap();
        let candidates = gather_candidates(&forest, &surface, &needs, &config)
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

    /// MAJOR-6 regression: the exact-clearance mask must reflect each member's
    /// ACTUAL seating state.
    ///
    /// An endpoint that sits ON the projected surface point has its axis
    /// station at arc distance `u*` standing `u*|cos|` above the plane, not
    /// `u*|cos| - h`, so the mask carries no `h` term for it. The check is
    /// end-to-end through the real kernel: on a planar seat the correct mask
    /// makes `segment_clearance` read exactly `MARGIN` (= process resolution,
    /// 0.05 mm at a 0.05 mm layer). Including `h` would push the first checked
    /// station 0.3 mm further out and the row would read 0.35 mm — 0.3 mm of
    /// the member silently exempted from the distance test.
    ///
    /// **THE SUBJECT MOVED WITH PR-7.** This test used `MemberKind::Strut` with
    /// a `Rootward` contact, because that WAS the unseated family: a CCO
    /// model-anchor strut's `p1` was the raw seat point. `apply_model_anchor`
    /// now seats it, so that pair is `Seated` and carrying the `h` term for it
    /// is correct. The unseated case is taken from a family that genuinely is
    /// one — a bifurcation `cco_upper` half — and the complementary half, that
    /// the SEATED family reads MARGIN too when its endpoint really is seated,
    /// is asserted alongside it. Between them they pin both directions of
    /// `contact::seating_of`, which is the only place the decision is recorded.
    #[test]
    fn an_unseated_contact_is_not_over_masked_by_the_seat_depth() {
        let mut forest = Forest::new();
        let strut = forest.add_member(contact_member(
            [0.0, 0.0, 6.0],
            [0.0, 0.0, 0.0],
            MemberKind::Other("cco_upper".to_owned()),
            ContactEnd::Rootward,
            0,
        ));
        let surface = flat_table();
        let config = BraceConfig::current(0.05).unwrap();
        let rows =
            exact_clearance_rows(&forest, &surface, &config, test_penetration_limits()).unwrap();
        let (key, clearance) = rows
            .iter()
            .copied()
            .find(|(key, _)| *key == strut)
            .expect("the strut is a live non-stub row");
        assert_eq!(key, strut);
        // MARGIN is the run's own process resolution.
        let margin = config.contact_policy().process_resolution_mm;
        assert!(
            (clearance - margin).abs() < 1.0e-9,
            "unseated strut must read exactly MARGIN = {margin}, got {clearance}"
        );
        // Discrimination: the rejected alternative (h included) reads
        // MARGIN + default_contact_depth_mm = 0.35, an order of magnitude away.
        assert!(clearance < margin + config.contact_depth_mm / 2.0);

        // The complementary half. A PR-7-seated CCO model anchor — the same
        // geometry with its endpoint pushed inward by the PR-1 depth, which is
        // what `apply_model_anchor` now mints — reads exactly MARGIN too,
        // BECAUSE the mask carries the `h` term for it. Take the `h` term away
        // and this row under-masks by `h/|cos|` and reads 0.05 - 0.3 = -0.25.
        let policy = config.contact_policy();
        let mut seated_forest = Forest::new();
        let seat = contact::seat_depth_mm(1.0, 0.65, &policy, false);
        let anchor = seated_forest.add_member(contact_member(
            [0.0, 0.0, 6.0],
            [0.0, 0.0, -seat],
            MemberKind::Strut,
            ContactEnd::Rootward,
            0,
        ));
        let seated_rows =
            exact_clearance_rows(&seated_forest, &surface, &config, test_penetration_limits())
                .unwrap();
        let (_, seated_clearance) = seated_rows
            .iter()
            .copied()
            .find(|(key, _)| *key == anchor)
            .expect("the seated anchor is a live non-stub row");
        assert!(
            (seated_clearance - margin).abs() < 1.0e-9,
            "a PR-7-seated anchor must read exactly MARGIN = {margin}, got {seated_clearance}"
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
        let config = BraceConfig::current(0.05).unwrap();
        let candidates = gather_candidates(&forest, &surface, &needs, &config)
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
