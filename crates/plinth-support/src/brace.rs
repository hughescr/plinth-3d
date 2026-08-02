//! Frozen pre-P8 bracing policy.
//!
//! The implementation is deliberately an immutable planner: it consumes one
//! accepted transient snapshot, gathers every cross-link and model-rung
//! candidate, evaluates all swept segments in one native batch, and only then
//! scatters the deterministic winners into a cloned forest.  The caller's
//! forest is never changed.

use std::collections::{BTreeMap, BTreeSet};

use geom_kernel::NativeSurface;
use plinth_core::{m0_constants, Finite, PositiveFinite};
use serde::Serialize;
use thiserror::Error;

use crate::{
    anchor::{FOOT_BREACH_CONE_RAYS, FOOT_BREACH_HALF_DEG},
    forest::{
        EmbedEnd, Forest, ForestError, Member, MemberKey, MemberKind, MemberSpec,
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
}

impl BraceConfig {
    fn current() -> Result<Self, BraceError> {
        let constants = m0_constants().map_err(|error| BraceError::Constants(error.to_string()))?;
        let p = constants.placeholders();
        Ok(Self {
            ladder_z_mm: p.brace_ladder_z_mm().diagnostic().get(),
            neighbor_max_xy_mm: p.brace_neighbor_max_xy_mm().diagnostic().get(),
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
    model_foot: Option<([f64; 3], f64)>,
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

/// Port of the live `compute_needs` plus candidate placement, without mutating
/// caller state. The supplied report is checked against a fresh canonical
/// report so stale snapshots cannot authorize geometry.
pub(crate) fn brace_pass_current_parity(
    forest: &Forest,
    surface: &NativeSurface,
    plate_z_mm: Finite,
    report: &TransientReport,
) -> Result<BracePassPlan, BraceError> {
    if &transient_report(forest, plate_z_mm)? != report {
        return Err(BraceError::StaleTransientReport);
    }
    let config = BraceConfig::current()?;
    let mut need_batch = compute_needs(forest, report, &config)?;
    append_clearance_needs(forest, surface, report, &config, &mut need_batch)?;
    let candidates = gather_candidates(forest, surface, &need_batch.needs, &config)?;
    let viable = evaluate_candidates(surface, &candidates, &config)?;
    let mut planned = forest.clone();
    let mut additions = Vec::new();
    let mut ordinals = BTreeMap::<BraceAdditionKind, u32>::new();
    let mut rungs_placed = 0;
    let mut rungs_unplaceable = 0;
    for (need_index, need) in need_batch.needs.iter().enumerate() {
        let mut pending = need.triggers.clone();
        let mut stacked = 0;
        let mut placed_this_need = false;
        let site_count = 1 + need.alternatives.len();
        for site_index in 0..site_count {
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
            let mut still_pending = Vec::new();
            for trigger in pending {
                if !trigger_relieved(&planned, surface, plate_z_mm, &fresh, trigger, &config)? {
                    still_pending.push(trigger);
                }
            }
            pending = still_pending;
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
) -> Result<BraceFixedPointPlan, BraceError> {
    const FROZEN_PASS_CAP: usize = 3;
    let mut working = forest.clone();
    let mut additions = Vec::new();
    let mut saw_unplaceable = false;
    for iteration in 1..=FROZEN_PASS_CAP {
        let report = transient_report(&working, plate_z_mm)?;
        let plan = brace_pass_current_parity(&working, surface, plate_z_mm, &report)?;
        saw_unplaceable |= plan.rungs_unplaceable > 0;
        let changed = !plan.resize.changed.is_empty();
        working = apply_resize(plan.forest_with_braces, &plan.resize)?;
        let added = !plan.additions.is_empty();
        additions.extend(plan.additions);
        let violations = exact_clearance_violations(&working, surface, &BraceConfig::current()?)?;
        if !added && !changed && violations == 0 {
            return Ok(BraceFixedPointPlan {
                forest: working,
                additions,
                iterations: iteration,
                termination: BraceFixedPointTermination::Converged,
            });
        }
        if iteration == FROZEN_PASS_CAP {
            if violations > 0 {
                return Err(BraceError::ClearanceViolation(violations));
            }
            return Ok(BraceFixedPointPlan {
                forest: working,
                additions,
                iterations: iteration,
                termination: if saw_unplaceable {
                    BraceFixedPointTermination::Unplaceable
                } else {
                    BraceFixedPointTermination::IterationLimit
                },
            });
        }
    }
    unreachable!("the frozen positive pass cap always returns")
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
            embed_end: (candidate.kind == BraceAdditionKind::ModelRung)
                .then_some(EmbedEnd::Rootward),
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

fn trigger_relieved(
    forest: &Forest,
    surface: &NativeSurface,
    plate_z_mm: Finite,
    report: &TransientReport,
    trigger: MemberKey,
    config: &BraceConfig,
) -> Result<bool, BraceError> {
    let Some(evidence) = report.member(trigger) else {
        return Ok(true);
    };
    let member = forest
        .member(trigger)
        .ok_or(BraceError::MissingMember(trigger))?;
    let compatibility = LegacyCompatibility::o10_live_current_support_and_raft();
    let transient = transient_radius(
        evidence.worst_span_mm.get(),
        lean_angle(member)?,
        evidence.n_terminals,
        &compatibility,
    )?;
    if evidence.worst_span_mm.get() > compatibility.brace_free_span_mm().diagnostic().get()
        || transient > config.max_unbraced_radius_mm
    {
        return Ok(false);
    }
    // The live path trials the complete resize formula and exact revalidation
    // before declaring a fallback sufficient. This is one state-dependent
    // batch over the whole immutable trial forest, never a scalar query.
    let resize = resize_forest_current_parity(forest, plate_z_mm)?;
    let trial = apply_resize(forest.clone(), &resize)?;
    Ok(exact_clearance_rows(&trial, surface, config)?
        .into_iter()
        .find_map(|(key, clearance)| (key == trigger).then_some(clearance >= 0.0))
        .unwrap_or(true))
}

fn append_clearance_needs(
    forest: &Forest,
    surface: &NativeSurface,
    report: &TransientReport,
    config: &BraceConfig,
    batch: &mut NeedBatch,
) -> Result<(), BraceError> {
    for member in exact_clearance_violated_members(forest, surface, config)? {
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
            let mut cross = cross_link_candidates(forest, host, z, need.rung_index, config, false)?;
            if cross.is_empty() && need.path_mapped {
                cross = cross_link_candidates(forest, host, z, need.rung_index, config, true)?;
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
                    model_foot: None,
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
    if !raw_model.is_empty() {
        let probes = raw_model.iter().map(|row| row.centroid).collect::<Vec<_>>();
        let (_, projected, _, normals) = surface.project(&probes);
        for ((raw, p1), normal) in raw_model.into_iter().zip(projected).zip(normals) {
            if (p1[2] - raw.z_mm).abs() > config.model_rung_z_tolerance_mm + HEIGHT_TOLERANCE_MM {
                continue;
            }
            let member = forest
                .member(raw.host)
                .ok_or(BraceError::MissingMember(raw.host))?;
            let p0 = axis_point_at_z(member, p1[2]);
            let length = norm(sub(p1, p0));
            if length + HEIGHT_TOLERANCE_MM < config.model_rung_min_length_mm
                || length > config.model_brace_max_length_mm + HEIGHT_TOLERANCE_MM
            {
                continue;
            }
            let radius = config.brace_radius_mm;
            let axis = scale(sub(p1, p0), 1.0 / length);
            let cosine = dot(axis, normal).abs().max(0.5);
            let inset1 = (config.contact_depth_mm + radius.get() + config.standoff_mm) / cosine;
            candidates.push(PlacementCandidate {
                need_index: raw.need_index,
                site_index: raw.site_index,
                rank: raw.rank,
                kind: BraceAdditionKind::ModelRung,
                rods: vec![Rod {
                    p0,
                    p1,
                    radius,
                    inset0: 0.0,
                    inset1,
                }],
                model_foot: Some((p1, config.ball_factor_foot * radius.get())),
                // The frozen current path exempts only short, exactly
                // horizontal model_rung members from the cone gate.
                printable: (p0[2] - p1[2]).abs() <= HEIGHT_TOLERANCE_MM,
            });
        }
    }
    candidates
        .sort_by_key(|candidate| (candidate.need_index, candidate.site_index, candidate.rank));
    Ok(candidates)
}

fn evaluate_candidates(
    surface: &NativeSurface,
    candidates: &[PlacementCandidate],
    config: &BraceConfig,
) -> Result<Vec<bool>, BraceError> {
    let mut p0 = Vec::new();
    let mut p1 = Vec::new();
    let mut r0 = Vec::new();
    let mut r1 = Vec::new();
    let mut inset0 = Vec::new();
    let mut inset1 = Vec::new();
    let mut ranges = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let start = p0.len();
        for rod in &candidate.rods {
            p0.push(rod.p0);
            p1.push(rod.p1);
            r0.push(rod.radius.get());
            r1.push(rod.radius.get());
            inset0.push(rod.inset0);
            inset1.push(rod.inset1);
        }
        ranges.push(start..p0.len());
    }
    let clearances = if p0.is_empty() {
        Vec::new()
    } else {
        surface
            .segment_clearance(&p0, &p1, &r0, &r1, config.standoff_mm, &inset0, &inset1)
            .map_err(|error| BraceError::NativeQuery(error.to_string()))?
            .0
    };
    let model_indices = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| candidate.model_foot.map(|foot| (index, foot)))
        .collect::<Vec<_>>();
    let breached = if model_indices.is_empty() {
        Vec::new()
    } else {
        let centers = model_indices
            .iter()
            .map(|(_, (point, _))| *point)
            .collect::<Vec<_>>();
        let radii = model_indices
            .iter()
            .map(|(_, (_, radius))| *radius)
            .collect::<Vec<_>>();
        surface
            .sphere_breach(
                &centers,
                &radii,
                config.ball_breach_epsilon_mm,
                FOOT_BREACH_HALF_DEG,
                FOOT_BREACH_CONE_RAYS,
            )
            .map_err(|error| BraceError::NativeQuery(error.to_string()))?
            .0
    };
    let mut foot_breached = BTreeMap::new();
    for ((candidate_index, _), value) in model_indices.into_iter().zip(breached) {
        foot_breached.insert(candidate_index, value);
    }
    Ok(ranges
        .iter()
        .enumerate()
        .map(|(index, range)| {
            candidates[index].printable
                && clearances[range.clone()].iter().all(|value| *value >= 0.0)
                && !foot_breached.get(&index).copied().unwrap_or(false)
        })
        .collect())
}

fn cross_link_candidates(
    forest: &Forest,
    host: MemberKey,
    z: f64,
    rung_index: usize,
    config: &BraceConfig,
    widened: bool,
) -> Result<Vec<(f64, MemberKey)>, BraceError> {
    let member = forest.member(host).ok_or(BraceError::MissingMember(host))?;
    let (host_lo, host_hi) = z_extent(member);
    if z < host_lo - HEIGHT_TOLERANCE_MM || z > host_hi + HEIGHT_TOLERANCE_MM {
        return Ok(Vec::new());
    }
    let host_point = axis_point_at_z(member, z);
    let cap = config.neighbor_max_xy_mm
        * if widened {
            config
                .reach_max_factor
                .max(1.25)
                .min(config.reach_max_factor)
        } else {
            1.0
        };
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
        let own = compatibility
            .deflection_radius_for_geometry(span.length_mm(), span.lean_angle_deg())?;
        let radius =
            PositiveFinite::new(config.brace_radius_mm.get().max(own.get()).max(if paired {
                raw * CROSS_LINK_DUTY_FACTOR
            } else {
                raw
            }))
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
    rows.truncate(config.model_rung_max_candidates);
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
) -> Result<usize, BraceError> {
    Ok(exact_clearance_rows(forest, surface, config)?
        .into_iter()
        .filter(|(_, clearance)| *clearance < 0.0)
        .count())
}

fn exact_clearance_violated_members(
    forest: &Forest,
    surface: &NativeSurface,
    config: &BraceConfig,
) -> Result<Vec<MemberKey>, BraceError> {
    let braces = forest.braces().into_iter().collect::<BTreeSet<_>>();
    Ok(exact_clearance_rows(forest, surface, config)?
        .into_iter()
        .filter_map(|(key, value)| (value < 0.0 && !braces.contains(&key)).then_some(key))
        .collect())
}

fn exact_clearance_rows(
    forest: &Forest,
    surface: &NativeSurface,
    config: &BraceConfig,
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
    let embedded = rows
        .iter()
        .enumerate()
        .filter_map(|(index, (_, member))| {
            member.embed_end().map(|end| {
                let point = match end {
                    EmbedEnd::Terminalward => member.p0_mm(),
                    EmbedEnd::Rootward => member.p1_mm(),
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
            let cosine = dot(axis, normal).abs().max(0.5);
            let radius = match end {
                EmbedEnd::Terminalward => member.terminal_radius_mm().get().get(),
                EmbedEnd::Rootward => member.rootward_radius_mm().get().get(),
            };
            let inset = (config.contact_depth_mm + radius + config.standoff_mm) / cosine;
            match end {
                EmbedEnd::Terminalward => inset0[index] = inset,
                EmbedEnd::Rootward => inset1[index] = inset,
            }
        }
    }
    let clearance = surface
        .segment_clearance(&p0, &p1, &r0, &r1, config.standoff_mm, &inset0, &inset1)
        .map_err(|error| BraceError::NativeQuery(error.to_string()))?
        .0;
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
    gaps.iter().copied().max_by(|left, right| {
        (left.hi_mm.get() - left.lo_mm.get()).total_cmp(&(right.hi_mm.get() - right.lo_mm.get()))
    })
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
            embed_end: None,
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
        let rows = cross_link_candidates(&forest, a, 10.0, 0, &config, false).unwrap();
        assert_eq!(rows, vec![(2.0, b)]);
        let even = cross_link_rods(&forest, a, b, 10.0, 2.0, 0, &config).unwrap();
        let odd = cross_link_rods(&forest, a, b, 10.0, 2.0, 1, &config).unwrap();
        assert_eq!(even.len(), 1);
        assert_eq!(odd.len(), 1);
        assert_eq!(even[0].p0, [0.0, 0.0, 10.0]);
        assert_eq!(odd[0].p0, [2.0, 0.0, 10.0]);
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
        let plan = brace_pass_current_parity(&forest, &remote_box(), plate, &report).unwrap();
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
            let plan = brace_pass_current_parity(&forest, &remote_box(), plate, &report).unwrap();
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
