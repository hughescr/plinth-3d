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
    forest::{
        ContactEnd, Forest, ForestError, Member, MemberKey, MemberKind, MemberSpec,
        RootwardEndRadius, TerminalEndRadius,
    },
    legacy_compat::{LegacyCompatibility, LegacyCompatibilityError},
    member_sizing::{
        diagnostic_within_printability_cone, DiagnosticMemberSizingPolicy, LeanAngleDeg,
        MemberSizingError, MemberSpan,
    },
    transient::{
        resize_forest_current_parity, transient_report, TransientError, TransientReport,
        TransientResizePlan, ZInterval,
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
}

impl BraceConfig {
    fn current() -> Result<Self, BraceError> {
        let constants = m0_constants().map_err(|error| BraceError::Constants(error.to_string()))?;
        let p = constants.placeholders();
        Ok(Self {
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
}

/// Bounded fixed-point result used by the pending support runner.
#[derive(Clone, Debug)]
pub(crate) struct BraceFixedPointPlan {
    pub(crate) forest: Forest,
    pub(crate) additions: Vec<BraceAddition>,
    pub(crate) iterations: usize,
    pub(crate) termination: BraceFixedPointTermination,
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

#[derive(Clone, Debug)]
struct Rod {
    p0: [f64; 3],
    p1: [f64; 3],
    radius: PositiveFinite,
    inset0: f64,
    inset1: f64,
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
) -> Result<BracePassPlan, BraceError> {
    if &transient_report(forest, plate_z_mm)? != report {
        return Err(BraceError::StaleTransientReport);
    }
    let config = BraceConfig::current()?;
    let mut need_batch = compute_needs(forest, report, &config)?;
    append_clearance_needs(
        forest,
        surface,
        report,
        &config,
        penetration_limits,
        &mut need_batch,
    )?;
    let candidates = gather_candidates(forest, surface, &need_batch.needs, &config)?;
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
    })
}

/// Frozen three-pass bracing/resize controller. It returns a new forest and a
/// named non-convergence outcome; it never asserts or silently falls through.
pub(crate) fn brace_fixed_point_current_parity(
    forest: &Forest,
    surface: &NativeSurface,
    plate_z_mm: Finite,
    penetration_limits: PenetrationBatchLimits,
) -> Result<BraceFixedPointPlan, BraceError> {
    const FROZEN_PASS_CAP: usize = 3;
    let mut working = forest.clone();
    let mut additions = Vec::new();
    let mut saw_unplaceable = false;
    for iteration in 1..=FROZEN_PASS_CAP {
        let report = transient_report(&working, plate_z_mm)?;
        let plan =
            brace_pass_current_parity(&working, surface, plate_z_mm, &report, penetration_limits)?;
        saw_unplaceable |= plan.rungs_unplaceable > 0;
        let changed = !plan.resize.changed.is_empty();
        working = apply_resize(plan.forest_with_braces, &plan.resize)?;
        let added = !plan.additions.is_empty();
        additions.extend(plan.additions);
        let violations = exact_clearance_violations(
            &working,
            surface,
            &BraceConfig::current()?,
            penetration_limits,
        )?;
        if !added && !changed && violations == 0 {
            return Ok(BraceFixedPointPlan {
                forest: working,
                additions,
                iterations: iteration,
                termination: if saw_unplaceable {
                    BraceFixedPointTermination::Unplaceable
                } else {
                    BraceFixedPointTermination::Converged
                },
            });
        }
        if iteration == FROZEN_PASS_CAP {
            let termination = pass_three_termination(saw_unplaceable, added, changed, violations)?
                .expect("the prior convergence return handles a stable clean third pass");
            return Ok(BraceFixedPointPlan {
                forest: working,
                additions,
                iterations: iteration,
                termination,
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
    violations: usize,
) -> Result<Option<BraceFixedPointTermination>, BraceError> {
    if violations > 0 {
        return Err(BraceError::ClearanceViolation(violations));
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
    crate::census::record_append(match candidate.kind {
        BraceAdditionKind::CrossLink => "brace_append_crosslink",
        BraceAdditionKind::ModelRung => "brace_append_model_rung",
    });
    for rod in &candidate.rods {
        let spec = MemberSpec {
            p0_mm: finite3(rod.p0)?,
            p1_mm: finite3(rod.p1)?,
            r0_mm: TerminalEndRadius::new(rod.radius),
            r1_mm: RootwardEndRadius::new(rod.radius),
            kind: match candidate.kind {
                BraceAdditionKind::CrossLink => MemberKind::Brace,
                BraceAdditionKind::ModelRung => MemberKind::Other("model_rung".to_owned()),
            },
            owner: crate::forest::Owner::Generated {
                family: match candidate.kind {
                    BraceAdditionKind::CrossLink => "brace",
                    BraceAdditionKind::ModelRung => "model_rung",
                }
                .to_owned(),
                ordinal: *ordinal,
            },
            model_contact_end: (candidate.kind == BraceAdditionKind::ModelRung)
                .then_some(ContactEnd::Rootward),
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
        let Some(gap) = largest_gap(&evidence.free_gaps_mm) else {
            continue;
        };
        let z = (gap.lo_mm.get() + gap.hi_mm.get()) / 2.0;
        if batch
            .needs
            .iter()
            .any(|need| need.host == member && (need.z_mm - z).abs() <= config.ladder_z_mm / 2.0)
        {
            continue;
        }
        batch.requested += 1;
        batch.needs.push(Need {
            host: member,
            z_mm: z,
            rung_index: 0,
            path_mapped: false,
            triggers: Vec::new(),
            alternatives: Vec::new(),
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

fn gather_candidates(
    forest: &Forest,
    surface: &NativeSurface,
    needs: &[Need],
    config: &BraceConfig,
) -> Result<Vec<PlacementCandidate>, BraceError> {
    let mut candidates = Vec::new();
    let mut raw_model = Vec::new();
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
        let cosine = abs_cos_raw.max(0.5);
        let rung_inset1 = embed_inset_mm(
            config.contact_depth_mm,
            radius.get(),
            config.standoff_mm,
            cosine,
        );
        crate::census::record_rung(abs_cos_raw, cosine, length, radius.get(), rung_inset1);
        candidates.push(PlacementCandidate {
            need_index: projected.raw.need_index,
            site_index: projected.raw.site_index,
            rank: projected.raw.rank,
            kind: BraceAdditionKind::ModelRung,
            rods: vec![Rod {
                p0,
                p1: projected.point,
                radius,
                inset0: 0.0,
                inset1: rung_inset1,
            }],
            printable: (p0[2] - projected.point[2]).abs() <= HEIGHT_TOLERANCE_MM,
        });
    }
    candidates
        .sort_by_key(|candidate| (candidate.need_index, candidate.site_index, candidate.rank));
    Ok(candidates)
}

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
            // Model rungs are the only candidate kind carrying a
            // cosine-derived embed inset; recover the cosine that built it.
            census_cos_class.push(if candidate.kind == BraceAdditionKind::ModelRung {
                let cosine = (rod.radius.get() + config.standoff_mm)
                    / (rod.inset1 - config.contact_depth_mm);
                if cosine <= 0.5 + 1e-9 {
                    1
                } else {
                    2
                }
            } else {
                0
            });
        }
        ranges.push(start..p0.len());
    }
    crate::census::record_segment_batch(
        "brace_candidate_clearance",
        &p0,
        &p1,
        &inset0,
        &inset1,
        &census_cos_class,
    );
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
    if crate::census::enabled() {
        let mut outcomes = Vec::new();
        for ((candidate, range), is_viable) in candidates.iter().zip(&ranges).zip(&viable) {
            if candidate.kind != BraceAdditionKind::ModelRung {
                continue;
            }
            for (rod, index) in candidate.rods.iter().zip(range.clone()) {
                let length = norm(sub(rod.p1, rod.p0));
                let cosine = (rod.radius.get() + config.standoff_mm)
                    / (rod.inset1 - config.contact_depth_mm);
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
    }
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

fn exact_clearance_violations(
    forest: &Forest,
    surface: &NativeSurface,
    config: &BraceConfig,
    penetration_limits: PenetrationBatchLimits,
) -> Result<usize, BraceError> {
    Ok(
        exact_clearance_rows(forest, surface, config, penetration_limits)?
            .into_iter()
            .filter(|(_, clearance)| *clearance < 0.0)
            .count(),
    )
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
            let cosine = abs_cos_raw.max(0.5);
            let radius = match end {
                ContactEnd::Terminalward => member.terminal_radius_mm().get().get(),
                ContactEnd::Rootward => member.rootward_radius_mm().get().get(),
            };
            let inset = embed_inset_mm(config.contact_depth_mm, radius, config.standoff_mm, cosine);
            crate::census::record_embed(
                abs_cos_raw,
                cosine,
                radius,
                inset,
                norm(axis_raw),
                member.kind(),
                end,
            );
            census_cos_class[index] = if abs_cos_raw < 0.5 { 1 } else { 2 };
            match end {
                ContactEnd::Terminalward => inset0[index] = inset,
                ContactEnd::Rootward => inset1[index] = inset,
            }
        }
    }
    crate::census::record_segment_batch(
        "brace_exact_member_clearance",
        &p0,
        &p1,
        &inset0,
        &inset1,
        &census_cos_class,
    );
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

fn embed_inset_mm(contact_depth_mm: f64, radius_mm: f64, standoff_mm: f64, cosine: f64) -> f64 {
    contact_depth_mm + (radius_mm + standoff_mm) / cosine
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
        let config = BraceConfig::current().unwrap();
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
        let config = BraceConfig::current().unwrap();
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
        let config = BraceConfig::current().unwrap();
        let rods = cross_link_rods(&forest, host, neighbor, 2.0, 2.0, 0, &config).unwrap();
        let compatibility = LegacyCompatibility::o10_live_current_support_and_raft();
        let expected = config
            .brace_radius_mm
            .get()
            .max(deflection_radius(2.0 * 2.0_f64.sqrt(), 45.0, &compatibility).unwrap());
        assert_eq!(rods.len(), 1);
        assert!((rods[0].radius.get() - expected).abs() <= 1e-12);
    }

    #[test]
    fn angled_embed_inset_leaves_contact_depth_unscaled() {
        let inset = embed_inset_mm(0.3, 0.5, 0.2, 0.5);
        assert!((inset - 1.7).abs() <= 1e-12);
        assert_ne!(inset, (0.3 + 0.5 + 0.2) / 0.5);
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
            &BraceConfig::current().unwrap(),
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
        let config = BraceConfig::current().unwrap();
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
        )
        .unwrap();
        assert_eq!(plan.termination, BraceFixedPointTermination::Unplaceable);
    }

    #[test]
    fn third_pass_termination_truth_table_preserves_frozen_outcomes() {
        for (added, changed) in [(true, false), (false, true), (true, true)] {
            assert_eq!(
                pass_three_termination(false, added, changed, 0).unwrap(),
                Some(BraceFixedPointTermination::IterationLimit)
            );
            assert_eq!(
                pass_three_termination(true, added, changed, 0).unwrap(),
                Some(BraceFixedPointTermination::Unplaceable)
            );
        }
        assert_eq!(
            pass_three_termination(false, false, false, 0).unwrap(),
            None
        );
        assert!(matches!(
            pass_three_termination(false, true, false, 1),
            Err(BraceError::ClearanceViolation(1))
        ));
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
        let batch = compute_needs(&forest, &report, &BraceConfig::current().unwrap()).unwrap();
        assert_eq!(batch.needs.len(), 1);
        assert!(batch.needs[0].path_mapped);
        assert_eq!(batch.needs[0].triggers, vec![lower]);
        assert_eq!(batch.needs[0].z_mm, 10.0);
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
        let config = BraceConfig::current().unwrap();
        let candidates = gather_candidates(&forest, &surface, &needs, &config).unwrap();
        let model = candidates
            .iter()
            .find(|row| row.kind == BraceAdditionKind::ModelRung)
            .expect("the real wall offers a bounded horizontal model rung");
        assert_eq!(model.rods.len(), 1);
        assert!((model.rods[0].p0[2] - model.rods[0].p1[2]).abs() <= HEIGHT_TOLERANCE_MM);
        assert!(model.rods[0].p1[0] >= 4.0);
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
