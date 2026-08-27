//! Build-time transient-span reporting and immutable current-parity sizing.
//!
//! This is a direct Rust port of the live pre-P8 `transient.py` path.  It
//! gathers forest geometry, evaluates the complete root-to-terminal batch,
//! then scatters one deterministic report and a new radius plan.  The input
//! forest is never mutated; applying and verifying a plan belongs to the
//! controller that owns the transaction.

use std::collections::{BTreeMap, BTreeSet};

use plinth_core::{m0_constants, Finite, PositiveFinite};
use serde::Serialize;
use thiserror::Error;

use crate::{
    forest::{
        ContactEnd, Forest, Member, MemberKey, MemberKind, RootwardEndRadius, TerminalEndRadius,
    },
    legacy_compat::{LegacyCompatibility, LegacyCompatibilityError},
    member_sizing::{MemberSizingError, MemberSpan},
};

const BRACE_ENDPOINT_BATCH_SIZE: usize = 256;
const TRANSIENT_TIE_TOLERANCE_MM: f64 = 1e-9;
const MIN_GEOMETRY_LENGTH_MM: f64 = 1e-6;
const MIN_Z_EXTENT_MM: f64 = 1e-9;

/// A restraint grips the path at `point_z_mm`, but exists only once the build
/// front reaches `active_z_mm`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct RestraintEvent {
    pub point_z_mm: Finite,
    pub active_z_mm: Finite,
}

/// Closed Z interval. Boundary contact is an overlap throughout this module.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct ZInterval {
    pub lo_mm: Finite,
    pub hi_mm: Finite,
}

/// One live brace endpoint geometrically attached to a host member.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct BraceAttachment {
    pub brace: MemberKey,
    pub z_mm: Finite,
}

/// Frozen build-time evidence for one live, non-stub, non-brace member.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MemberTransientEvidence {
    pub member: MemberKey,
    pub worst_span_mm: Finite,
    pub restraint_zs_mm: Vec<Finite>,
    pub free_gaps_mm: Vec<ZInterval>,
    pub n_terminals: usize,
    pub worst_span_interval_mm: ZInterval,
    pub worst_span_path: Vec<MemberKey>,
}

/// Deterministically keyed report for every eligible member.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TransientReport {
    pub members: Vec<MemberTransientEvidence>,
}

impl TransientReport {
    #[must_use]
    pub fn member(&self, key: MemberKey) -> Option<&MemberTransientEvidence> {
        self.members
            .binary_search_by_key(&key, |row| row.member)
            .ok()
            .map(|index| &self.members[index])
    }
}

/// New radii proposed for one member. Terminalward and rootward ends remain
/// distinct, preventing a taper from being accidentally flattened.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct PlannedMemberRadii {
    pub member: MemberKey,
    pub terminalward: TerminalEndRadius,
    pub rootward: RootwardEndRadius,
}

/// Audit row emitted only when the proposed endpoints differ from the input.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct RadiusChange {
    pub member: MemberKey,
    pub old_max_mm: PositiveFinite,
    pub new_max_mm: PositiveFinite,
}

/// Immutable resize result. `radii` is a new complete array for eligible
/// members; it never aliases or mutates verifier input.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TransientResizePlan {
    pub radii: Vec<PlannedMemberRadii>,
    pub changed: Vec<RadiusChange>,
    pub report: TransientReport,
}

impl TransientResizePlan {
    #[must_use]
    pub fn member_radii(&self, key: MemberKey) -> Option<PlannedMemberRadii> {
        self.radii
            .binary_search_by_key(&key, |row| row.member)
            .ok()
            .map(|index| self.radii[index])
    }
}

/// Named failure from report or sizing construction.
#[derive(Clone, Debug, PartialEq, Error)]
pub enum TransientError {
    #[error("transient interval has base above top")]
    ReversedBuildInterval,
    #[error("live forest member disappeared while gathering: {0:?}")]
    MissingLiveMember(MemberKey),
    #[error("member has no finite positive physical span: {0:?}")]
    InvalidMemberSpan(MemberKey),
    #[error("member has no free Z gap to size: {0:?}")]
    MissingFreeGap(MemberKey),
    #[error("registered M0 constants are unavailable: {0}")]
    RegisteredConstantsUnavailable(String),
    #[error("transient arithmetic produced a non-finite result")]
    NonFiniteResult,
    #[error(transparent)]
    PointSegmentBatch(#[from] geom_kernel::PointSegmentBatchError),
    #[error(transparent)]
    Compatibility(#[from] LegacyCompatibilityError),
    #[error(transparent)]
    Sizing(#[from] MemberSizingError),
}

/// The frozen active-height sweep. Events are stably sorted by `active_z_mm`,
/// never by their grip point, and every event is clamped rather than dropped.
pub fn transient_worst_span(
    restraints: &[RestraintEvent],
    base_z_mm: Finite,
    z_top_mm: Finite,
) -> Result<Finite, TransientError> {
    let intervals = span_intervals(restraints, base_z_mm, z_top_mm)?;
    finite(
        intervals
            .iter()
            .map(|interval| interval.hi_mm.get() - interval.lo_mm.get())
            .fold(0.0, f64::max),
    )
}

fn span_intervals(
    restraints: &[RestraintEvent],
    base_z_mm: Finite,
    z_top_mm: Finite,
) -> Result<Vec<ZInterval>, TransientError> {
    let base = base_z_mm.get();
    let top = z_top_mm.get();
    if base > top {
        return Err(TransientError::ReversedBuildInterval);
    }
    let mut events = restraints.to_vec();
    events.sort_by(|left, right| left.active_z_mm.get().total_cmp(&right.active_z_mm.get()));
    let mut highest_active = base;
    let mut intervals = Vec::with_capacity(events.len() + 1);
    for event in events {
        let active = event.active_z_mm.get().clamp(base, top);
        intervals.push(interval(highest_active, active)?);
        highest_active = highest_active.max(event.point_z_mm.get().min(top));
    }
    intervals.push(interval(highest_active, top)?);
    Ok(intervals)
}

/// Gather all brace endpoints, evaluate them against all host axes, and then
/// scatter the complete attachment relation. A joint endpoint may attach to
/// multiple hosts; a model-embedded endpoint attaches to none.
pub fn derive_brace_attachments(
    forest: &Forest,
) -> Result<BTreeMap<MemberKey, Vec<BraceAttachment>>, TransientError> {
    let brace_attachment_tolerance_mm = m0_constants()
        .map_err(|error| TransientError::RegisteredConstantsUnavailable(error.to_string()))?
        .placeholders()
        .connectivity_axis_attach_tol_mm()
        .diagnostic()
        .get();
    let braces = forest.braces();
    let brace_set = braces.iter().copied().collect::<BTreeSet<_>>();
    let hosts = forest
        .live_members()
        .into_iter()
        .filter(|key| !brace_set.contains(key))
        .collect::<Vec<_>>();
    let host_geometry = hosts
        .iter()
        .map(|key| {
            forest
                .member(*key)
                .map(|member| (*key, points(member)))
                .ok_or(TransientError::MissingLiveMember(*key))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut endpoints = Vec::new();
    for brace in braces {
        let member = forest
            .member(brace)
            .ok_or(TransientError::MissingLiveMember(brace))?;
        let (p0, p1) = points(member);
        if member.model_contact_end() != Some(ContactEnd::Terminalward) {
            endpoints.push((brace, p0));
        }
        if member.model_contact_end() != Some(ContactEnd::Rootward) {
            endpoints.push((brace, p1));
        }
    }

    let segment_starts = host_geometry
        .iter()
        .map(|(_, (start, _))| *start)
        .collect::<Vec<_>>();
    let segment_ends = host_geometry
        .iter()
        .map(|(_, (_, end))| *end)
        .collect::<Vec<_>>();
    let batch_limits = geom_kernel::PointSegmentBatchLimits::for_dimensions(
        BRACE_ENDPOINT_BATCH_SIZE,
        host_geometry.len(),
    )?;
    let mut out = BTreeMap::<MemberKey, Vec<BraceAttachment>>::new();
    for endpoint_window in endpoints.chunks(BRACE_ENDPOINT_BATCH_SIZE) {
        let points = endpoint_window
            .iter()
            .map(|(_, point)| *point)
            .collect::<Vec<_>>();
        let distances = geom_kernel::point_segment_distances_batch(
            &points,
            &segment_starts,
            &segment_ends,
            batch_limits,
        )?;
        debug_assert_eq!(
            distances.effective_threads(),
            geom_kernel::current_threads()
        );
        for (endpoint_index, (brace, endpoint)) in endpoint_window.iter().enumerate() {
            let row = distances
                .row(endpoint_index)
                .expect("batch retains one result row per gathered endpoint");
            for (host_index, distance) in row.iter().enumerate() {
                if *distance <= brace_attachment_tolerance_mm {
                    let host = host_geometry[host_index].0;
                    out.entry(host).or_default().push(BraceAttachment {
                        brace: *brace,
                        z_mm: finite(endpoint[2])?,
                    });
                }
            }
        }
    }
    for attachments in out.values_mut() {
        attachments.sort_by(|left, right| {
            left.z_mm
                .get()
                .total_cmp(&right.z_mm.get())
                .then_with(|| left.brace.cmp(&right.brace))
        });
    }
    Ok(out)
}

/// Build the complete frozen transient report without changing the forest.
pub fn transient_report(
    forest: &Forest,
    _plate_z_mm: Finite,
) -> Result<TransientReport, TransientError> {
    let brace_attachments = derive_brace_attachments(forest)?;
    let live_tree = forest.live_tree_members();
    let eligible = live_tree
        .iter()
        .copied()
        .filter(|key| {
            forest
                .member(*key)
                .is_some_and(|member| !matches!(member.kind(), MemberKind::Stub))
        })
        .collect::<Vec<_>>();
    let eligible_set = eligible.iter().copied().collect::<BTreeSet<_>>();
    let terminals = live_tree
        .iter()
        .copied()
        .filter(|key| {
            forest
                .member(*key)
                .is_some_and(|member| matches!(member.kind(), MemberKind::Stub))
                && forest.parent_of(*key).is_some()
        })
        .collect::<Vec<_>>();

    #[derive(Clone)]
    struct Best {
        span_mm: f64,
        interval: Option<(f64, f64)>,
        path: Option<Vec<MemberKey>>,
    }
    let mut best = eligible
        .iter()
        .map(|key| {
            (
                *key,
                Best {
                    span_mm: 0.0,
                    interval: None,
                    path: None,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut completion_cache = BTreeMap::new();

    for terminal in terminals {
        let path = root_to_terminal_path(forest, terminal);
        let root = *path
            .first()
            .ok_or(TransientError::MissingLiveMember(terminal))?;
        let root_member = forest
            .member(root)
            .ok_or(TransientError::MissingLiveMember(root))?;
        let base_z = root_member.p1_mm()[2];
        let z_top = finite(path.iter().try_fold(f64::NEG_INFINITY, |top, key| {
            let member = forest
                .member(*key)
                .ok_or(TransientError::MissingLiveMember(*key))?;
            Ok::<_, TransientError>(
                top.max(member.p0_mm()[2].get())
                    .max(member.p1_mm()[2].get()),
            )
        })?)?;
        let events = path_events(forest, &path, &brace_attachments, &mut completion_cache)?;
        let intervals = span_intervals(&events, base_z, z_top)?;

        for key in path
            .iter()
            .copied()
            .filter(|key| eligible_set.contains(key))
        {
            let member = forest
                .member(key)
                .ok_or(TransientError::MissingLiveMember(key))?;
            let (zlo, zhi) = z_extent(member);
            let (best_here, best_interval) = best_member_interval(&intervals, zlo, zhi);
            if best_here <= 0.0 {
                continue;
            }
            let current = best.get_mut(&key).expect("eligible key was gathered");
            let wins = best_here > current.span_mm + TRANSIENT_TIE_TOLERANCE_MM;
            let ties = (best_here - current.span_mm).abs() <= TRANSIENT_TIE_TOLERANCE_MM;
            if wins
                || (ties
                    && current
                        .path
                        .as_ref()
                        .is_none_or(|current_path| path.as_slice() < current_path.as_slice()))
            {
                current.span_mm = best_here;
                current.interval = best_interval;
                current.path = Some(path.clone());
            }
        }
    }

    let mut terminal_count_cache = BTreeMap::new();
    let mut members = Vec::with_capacity(eligible.len());
    for key in eligible {
        let member = forest
            .member(key)
            .ok_or(TransientError::MissingLiveMember(key))?;
        let (zlo, zhi) = z_extent(member);
        let restraint_zs = static_restraint_zs(forest, &brace_attachments, key)?;
        let free_gaps = free_gaps(zlo, zhi, &restraint_zs)?;
        let n_terminals = subtree_terminal_count(forest, key, &mut terminal_count_cache)?;
        let winner = best.remove(&key).expect("eligible key was gathered");
        let (winning_interval, winning_path) = if let Some((lo, hi)) = winner.interval {
            let winning_path = winner
                .path
                .unwrap_or_default()
                .into_iter()
                .filter(|path_key| {
                    forest.member(*path_key).is_some_and(|path_member| {
                        !matches!(path_member.kind(), MemberKind::Stub)
                            && extent_intersects(path_member, lo, hi)
                    })
                })
                .collect();
            (interval(lo, hi)?, winning_path)
        } else {
            (interval(zlo, zlo)?, Vec::new())
        };
        members.push(MemberTransientEvidence {
            member: key,
            worst_span_mm: finite(winner.span_mm)?,
            restraint_zs_mm: restraint_zs,
            free_gaps_mm: free_gaps,
            n_terminals,
            worst_span_interval_mm: winning_interval,
            worst_span_path: winning_path,
        });
    }
    Ok(TransientReport { members })
}

/// Compute a new complete radius array in child-before-parent order. The
/// O-10 bridge supplies the sole frozen 0.01 N compatibility load; this module
/// neither calibrates it nor promotes it to acceptance evidence.
pub(crate) fn resize_forest_current_parity(
    forest: &Forest,
    plate_z_mm: Finite,
) -> Result<TransientResizePlan, TransientError> {
    let report = transient_report(forest, plate_z_mm)?;
    let order = postorder(forest);
    let policy = LegacyCompatibility::o10_live_current_support_and_raft();
    let exponent = m0_constants()
        .map_err(|error| TransientError::RegisteredConstantsUnavailable(error.to_string()))?
        .placeholders()
        .transient_shoot_count_exponent()
        .diagnostic()
        .get();
    let live_tree = forest
        .live_tree_members()
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut planned_by_member = BTreeMap::new();
    let mut changed = Vec::new();

    for key in order {
        let member = forest
            .member(key)
            .ok_or(TransientError::MissingLiveMember(key))?;
        let evidence = report
            .member(key)
            .ok_or(TransientError::MissingLiveMember(key))?;
        let children = forest
            .children_of(key)
            .into_iter()
            .filter(|child| live_tree.contains(child))
            .collect::<Vec<_>>();
        let child_rootward = children
            .iter()
            .map(|child| {
                planned_by_member.get(child).map_or_else(
                    || {
                        forest
                            .member(*child)
                            .map(Member::rootward_radius_mm)
                            .ok_or(TransientError::MissingLiveMember(*child))
                    },
                    |planned: &PlannedMemberRadii| Ok(planned.rootward),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let span = MemberSpan::new(member.p0_mm(), member.p1_mm())
            .map_err(|_| TransientError::InvalidMemberSpan(key))?;
        let worst_gap_z = evidence
            .free_gaps_mm
            .iter()
            .map(|gap| gap.hi_mm.get() - gap.lo_mm.get())
            .reduce(f64::max)
            .ok_or(TransientError::MissingFreeGap(key))?;
        let z_extent = (member.p0_mm()[2].get() - member.p1_mm()[2].get())
            .abs()
            .max(MIN_Z_EXTENT_MM);
        let worst_gap_3d = equivalent_free_length_mm(worst_gap_z, span.length_mm().get(), z_extent);
        let base_required = policy.required_radius_for_geometry(
            member.kind(),
            &child_rootward,
            member.carried_mm2(),
            positive(worst_gap_3d.max(MIN_GEOMETRY_LENGTH_MM))?,
            span.lean_angle_deg(),
        )?;
        let transient_base = policy.deflection_radius_for_geometry(
            positive(evidence.worst_span_mm.get().max(MIN_GEOMETRY_LENGTH_MM))?,
            span.lean_angle_deg(),
        )?;
        let shoot_factor = (evidence.n_terminals.max(1) as f64).powf(exponent);
        let transient_required = transient_base.get() * shoot_factor;
        if !transient_required.is_finite() {
            return Err(TransientError::NonFiniteResult);
        }
        let required = positive(base_required.get().max(transient_required))?;
        let old_terminal = member.terminal_radius_mm();
        let old_rootward = member.rootward_radius_mm();
        let uniform_or_branch = (old_terminal.get().get() - old_rootward.get().get()).abs()
            < TRANSIENT_TIE_TOLERANCE_MM
            || children.len() >= 2;
        let planned = if uniform_or_branch {
            PlannedMemberRadii {
                member: key,
                terminalward: TerminalEndRadius::new(required),
                rootward: RootwardEndRadius::new(required),
            }
        } else {
            PlannedMemberRadii {
                member: key,
                terminalward: old_terminal,
                rootward: RootwardEndRadius::new(positive(
                    required.get().max(old_terminal.get().get()),
                )?),
            }
        };
        let endpoint_changed = (planned.terminalward.get().get() - old_terminal.get().get()).abs()
            > TRANSIENT_TIE_TOLERANCE_MM
            || (planned.rootward.get().get() - old_rootward.get().get()).abs()
                > TRANSIENT_TIE_TOLERANCE_MM;
        if endpoint_changed {
            changed.push(RadiusChange {
                member: key,
                old_max_mm: positive(old_terminal.get().get().max(old_rootward.get().get()))?,
                new_max_mm: positive(
                    planned
                        .terminalward
                        .get()
                        .get()
                        .max(planned.rootward.get().get()),
                )?,
            });
        }
        planned_by_member.insert(key, planned);
    }
    Ok(TransientResizePlan {
        radii: planned_by_member.into_values().collect(),
        changed,
        report,
    })
}

fn root_to_terminal_path(forest: &Forest, terminal: MemberKey) -> Vec<MemberKey> {
    let mut path = Vec::new();
    let mut current = Some(terminal);
    while let Some(key) = current {
        path.push(key);
        current = forest.parent_of(key);
    }
    path.reverse();
    path
}

fn path_events(
    forest: &Forest,
    path: &[MemberKey],
    brace_attachments: &BTreeMap<MemberKey, Vec<BraceAttachment>>,
    completion_cache: &mut BTreeMap<MemberKey, f64>,
) -> Result<Vec<RestraintEvent>, TransientError> {
    let mut events = Vec::new();
    for (position, key) in path.iter().copied().enumerate() {
        for attachment in brace_attachments.get(&key).into_iter().flatten() {
            let brace = forest
                .member(attachment.brace)
                .ok_or(TransientError::MissingLiveMember(attachment.brace))?;
            events.push(RestraintEvent {
                point_z_mm: attachment.z_mm,
                active_z_mm: finite(brace.p0_mm()[2].get().max(brace.p1_mm()[2].get()))?,
            });
        }
        if let Some(next) = path.get(position + 1) {
            let siblings = forest
                .children_of(key)
                .into_iter()
                .filter(|child| child != next)
                .collect::<Vec<_>>();
            if !siblings.is_empty() {
                let mut active_z = f64::INFINITY;
                for sibling in siblings {
                    active_z = active_z.min(subtree_completion(forest, sibling, completion_cache)?);
                }
                let member = forest
                    .member(key)
                    .ok_or(TransientError::MissingLiveMember(key))?;
                events.push(RestraintEvent {
                    point_z_mm: member.p0_mm()[2],
                    active_z_mm: finite(active_z)?,
                });
            }
        }
    }
    Ok(events)
}

fn subtree_completion(
    forest: &Forest,
    key: MemberKey,
    cache: &mut BTreeMap<MemberKey, f64>,
) -> Result<f64, TransientError> {
    if let Some(value) = cache.get(&key) {
        return Ok(*value);
    }
    let member = forest
        .member(key)
        .ok_or(TransientError::MissingLiveMember(key))?;
    let mut top = member.p0_mm()[2].get().max(member.p1_mm()[2].get());
    for child in forest.children_of(key) {
        top = top.max(subtree_completion(forest, child, cache)?);
    }
    cache.insert(key, top);
    Ok(top)
}

fn subtree_terminal_count(
    forest: &Forest,
    key: MemberKey,
    cache: &mut BTreeMap<MemberKey, usize>,
) -> Result<usize, TransientError> {
    if let Some(value) = cache.get(&key) {
        return Ok(*value);
    }
    let member = forest
        .member(key)
        .ok_or(TransientError::MissingLiveMember(key))?;
    let mut count = usize::from(matches!(member.kind(), MemberKind::Stub));
    for child in forest.children_of(key) {
        count = count
            .checked_add(subtree_terminal_count(forest, child, cache)?)
            .ok_or(TransientError::NonFiniteResult)?;
    }
    cache.insert(key, count);
    Ok(count)
}

fn static_restraint_zs(
    forest: &Forest,
    brace_attachments: &BTreeMap<MemberKey, Vec<BraceAttachment>>,
    key: MemberKey,
) -> Result<Vec<Finite>, TransientError> {
    let member = forest
        .member(key)
        .ok_or(TransientError::MissingLiveMember(key))?;
    let mut zs = brace_attachments
        .get(&key)
        .into_iter()
        .flatten()
        .map(|attachment| attachment.z_mm)
        .collect::<Vec<_>>();
    if forest.children_of(key).len() >= 2 {
        zs.push(member.p0_mm()[2]);
    }
    if let Some(parent) = forest.parent_of(key) {
        if forest.children_of(parent).len() >= 2 {
            zs.push(member.p1_mm()[2]);
        }
    } else {
        zs.push(member.p1_mm()[2]);
    }
    zs.sort_by(|left, right| left.get().total_cmp(&right.get()));
    Ok(zs)
}

#[allow(
    clippy::float_cmp,
    reason = "collapsing bit-equal bounds after a `total_cmp` sort so \
              `windows(2)` cannot emit a zero-length interval; two restraints \
              that differ at all bound a real, if thin, gap"
)]
fn free_gaps(zlo: f64, zhi: f64, restraints: &[Finite]) -> Result<Vec<ZInterval>, TransientError> {
    let mut bounds = vec![zlo, zhi];
    bounds.extend(
        restraints
            .iter()
            .map(|value| value.get())
            .filter(|value| *value >= zlo && *value <= zhi),
    );
    bounds.sort_by(f64::total_cmp);
    bounds.dedup_by(|left, right| *left == *right);
    bounds
        .windows(2)
        .map(|pair| interval(pair[0], pair[1]))
        .collect()
}

fn postorder(forest: &Forest) -> Vec<MemberKey> {
    fn visit(
        forest: &Forest,
        key: MemberKey,
        visited: &mut BTreeSet<MemberKey>,
        order: &mut Vec<MemberKey>,
    ) {
        if !visited.insert(key) {
            return;
        }
        for child in forest.children_of(key) {
            visit(forest, child, visited, order);
        }
        if forest
            .member(key)
            .is_some_and(|member| !matches!(member.kind(), MemberKind::Stub))
        {
            order.push(key);
        }
    }
    let mut visited = BTreeSet::new();
    let mut order = Vec::new();
    for key in forest.live_tree_members() {
        visit(forest, key, &mut visited, &mut order);
    }
    order
}

fn points(member: &Member) -> ([f64; 3], [f64; 3]) {
    (
        member.p0_mm().map(Finite::get),
        member.p1_mm().map(Finite::get),
    )
}

fn z_extent(member: &Member) -> (f64, f64) {
    let a = member.p0_mm()[2].get();
    let b = member.p1_mm()[2].get();
    (a.min(b), a.max(b))
}

fn extent_intersects(member: &Member, lo: f64, hi: f64) -> bool {
    let (zlo, zhi) = z_extent(member);
    zhi >= lo && zlo <= hi
}

fn best_member_interval(
    intervals: &[ZInterval],
    member_zlo: f64,
    member_zhi: f64,
) -> (f64, Option<(f64, f64)>) {
    let mut best = 0.0;
    let mut winner = None;
    for candidate in intervals {
        let lo = candidate.lo_mm.get();
        let hi = candidate.hi_mm.get();
        if hi >= member_zlo && lo <= member_zhi {
            let arm_lo = member_zlo.max(lo);
            let contribution = hi - arm_lo;
            if contribution > best {
                best = contribution;
                winner = Some((arm_lo, hi));
            }
        }
    }
    (best, winner)
}

fn equivalent_free_length_mm(worst_gap_z_mm: f64, own_length_mm: f64, z_extent_mm: f64) -> f64 {
    worst_gap_z_mm * own_length_mm / z_extent_mm.max(MIN_Z_EXTENT_MM)
}

fn interval(lo: f64, hi: f64) -> Result<ZInterval, TransientError> {
    Ok(ZInterval {
        lo_mm: finite(lo)?,
        hi_mm: finite(hi)?,
    })
}

fn finite(value: f64) -> Result<Finite, TransientError> {
    Finite::new(value).map_err(|_| TransientError::NonFiniteResult)
}

fn positive(value: f64) -> Result<PositiveFinite, TransientError> {
    PositiveFinite::new(value).map_err(|_| TransientError::NonFiniteResult)
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;
    use crate::forest::{MemberSpec, Owner};

    fn f(value: f64) -> Finite {
        Finite::new(value).unwrap()
    }

    fn p(value: f64) -> PositiveFinite {
        PositiveFinite::new(value).unwrap()
    }

    fn member(
        ordinal: u32,
        p0: [f64; 3],
        p1: [f64; 3],
        kind: MemberKind,
        r0: f64,
        r1: f64,
    ) -> Member {
        Member::new(MemberSpec {
            p0_mm: p0.map(f),
            p1_mm: p1.map(f),
            r0_mm: TerminalEndRadius::new(p(r0)),
            r1_mm: RootwardEndRadius::new(p(r1)),
            kind,
            owner: Owner::Generated {
                family: "transient-test".to_owned(),
                ordinal,
            },
            model_contact_end: None,
            carried_mm2: f(0.0),
        })
    }

    fn branched_fixture() -> (Forest, MemberKey, MemberKey, MemberKey, MemberKey) {
        let mut forest = Forest::new();
        let root = forest.add_member(member(
            0,
            [0.0, 0.0, 10.0],
            [0.0, 0.0, 0.0],
            MemberKind::Trunk,
            0.8,
            0.8,
        ));
        let shoot_a = forest.add_member(member(
            1,
            [0.0, 0.0, 50.0],
            [0.0, 0.0, 10.0],
            MemberKind::Shoot,
            0.2,
            0.3,
        ));
        forest.set_parent(shoot_a, Some(root)).unwrap();
        let shoot_b = forest.add_member(member(
            2,
            [10.0, 0.0, 50.0],
            [0.0, 0.0, 10.0],
            MemberKind::Shoot,
            0.2,
            0.3,
        ));
        forest.set_parent(shoot_b, Some(root)).unwrap();
        let stub_a = forest.add_member(member(
            3,
            [0.0, 0.0, 55.0],
            [0.0, 0.0, 50.0],
            MemberKind::Stub,
            0.2,
            0.2,
        ));
        forest.set_parent(stub_a, Some(shoot_a)).unwrap();
        let stub_b = forest.add_member(member(
            4,
            [10.0, 0.0, 55.0],
            [10.0, 0.0, 50.0],
            MemberKind::Stub,
            0.2,
            0.2,
        ));
        forest.set_parent(stub_b, Some(shoot_b)).unwrap();
        (forest, root, shoot_a, shoot_b, stub_a)
    }

    fn leaning_fixture(lean_deg: f64) -> (Forest, MemberKey) {
        let mut forest = Forest::new();
        let horizontal = 10.0 * lean_deg.to_radians().tan();
        let shoot = forest.add_member(member(
            0,
            [horizontal, 0.0, 10.0],
            [0.0, 0.0, 0.0],
            MemberKind::Shoot,
            0.2,
            0.3,
        ));
        let stub = forest.add_member(member(
            1,
            [horizontal, 0.0, 12.0],
            [horizontal, 0.0, 10.0],
            MemberKind::Stub,
            0.2,
            0.2,
        ));
        forest.set_parent(stub, Some(shoot)).unwrap();
        (forest, shoot)
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn active_z_sort_clamps_events_and_zero_events_use_the_trailing_span() {
        let events = [
            RestraintEvent {
                point_z_mm: f(50.0),
                active_z_mm: f(100.0),
            },
            RestraintEvent {
                point_z_mm: f(10.0),
                active_z_mm: f(20.0),
            },
        ];
        assert_eq!(
            transient_worst_span(&events, f(0.0), f(80.0))
                .unwrap()
                .get(),
            70.0
        );
        assert_eq!(
            transient_worst_span(&[], f(3.0), f(83.0)).unwrap().get(),
            80.0
        );
    }

    #[test]
    fn overlap_is_closed_but_later_disjoint_restraints_do_not_load_member() {
        let touching = [interval(10.0, 20.0).unwrap()];
        assert_eq!(
            best_member_interval(&touching, 0.0, 10.0),
            (10.0, Some((10.0, 20.0)))
        );

        let disjoint = [interval(15.0, 100.0).unwrap()];
        assert_eq!(best_member_interval(&disjoint, 0.0, 10.0), (0.0, None));

        let arm = [interval(10.0, 50.0).unwrap()];
        assert_eq!(
            best_member_interval(&arm, 20.0, 40.0),
            (30.0, Some((20.0, 50.0)))
        );
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn branch_report_counts_live_terminals_and_uses_lexicographically_first_path() {
        let (mut forest, root, shoot_a, shoot_b, _stub_a) = branched_fixture();
        let dead = forest.add_member(member(
            99,
            [0.0, 0.0, 2.0],
            [0.0, 0.0, 1.0],
            MemberKind::Shoot,
            0.4,
            0.4,
        ));
        forest.retire(dead).unwrap();

        let report = transient_report(&forest, f(0.0)).unwrap();
        assert!(report.member(dead).is_none());
        let root_evidence = report.member(root).unwrap();
        assert_eq!(root_evidence.n_terminals, 2);
        assert_eq!(root_evidence.worst_span_mm.get(), 55.0);
        assert_eq!(root_evidence.worst_span_path, vec![root, shoot_a]);
        assert_eq!(
            root_evidence
                .restraint_zs_mm
                .iter()
                .map(|z| z.get())
                .collect::<Vec<_>>(),
            vec![0.0, 10.0]
        );
        assert_eq!(report.member(shoot_a).unwrap().worst_span_mm.get(), 45.0);
        assert_eq!(report.member(shoot_b).unwrap().worst_span_mm.get(), 45.0);
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn brace_endpoints_are_gathered_geometrically_and_model_embed_is_excluded() {
        let mut forest = Forest::new();
        let host = forest.add_member(member(
            0,
            [0.0, 0.0, 20.0],
            [0.0, 0.0, 0.0],
            MemberKind::Trunk,
            0.8,
            0.8,
        ));
        let stub = forest.add_member(member(
            1,
            [0.0, 0.0, 22.0],
            [0.0, 0.0, 20.0],
            MemberKind::Stub,
            0.2,
            0.2,
        ));
        forest.set_parent(stub, Some(host)).unwrap();
        let brace = forest.add_brace(member(
            2,
            [0.1, 0.0, 5.0],
            [5.0, 0.0, 5.0],
            MemberKind::Brace,
            0.3,
            0.3,
        ));

        let attached = derive_brace_attachments(&forest).unwrap();
        assert_eq!(
            attached[&host],
            vec![BraceAttachment {
                brace,
                z_mm: f(5.0)
            }]
        );
        let report = transient_report(&forest, f(0.0)).unwrap();
        assert_eq!(report.member(host).unwrap().worst_span_mm.get(), 17.0);
        assert_eq!(
            report
                .member(host)
                .unwrap()
                .free_gaps_mm
                .iter()
                .map(|gap| (gap.lo_mm.get(), gap.hi_mm.get()))
                .collect::<Vec<_>>(),
            vec![(0.0, 5.0), (5.0, 20.0)]
        );

        forest
            .set_model_contact_end(brace, Some(ContactEnd::Terminalward))
            .unwrap();
        assert!(!derive_brace_attachments(&forest)
            .unwrap()
            .contains_key(&host));
    }

    #[test]
    fn brace_endpoint_batch_crosses_the_frozen_256_row_window_without_reordering() {
        let mut forest = Forest::new();
        let host = forest.add_member(member(
            0,
            [0.0, 0.0, 20.0],
            [0.0, 0.0, 0.0],
            MemberKind::Trunk,
            0.8,
            0.8,
        ));
        let mut expected = Vec::new();
        for ordinal in 1..=129 {
            let z = f64::from(ordinal) / 10.0;
            let brace = forest.add_brace(member(
                ordinal,
                [0.1, 0.0, z],
                [0.1, 0.0, z],
                MemberKind::Brace,
                0.3,
                0.3,
            ));
            expected.extend([brace, brace]);
        }
        let attachments = derive_brace_attachments(&forest).unwrap();
        assert_eq!(attachments[&host].len(), 258);
        assert_eq!(
            attachments[&host]
                .iter()
                .map(|attachment| attachment.brace)
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn leaning_free_gap_is_converted_back_to_true_3d_length() {
        let own_length = 20.0;
        let z_extent = 10.0;
        assert_eq!(equivalent_free_length_mm(5.0, own_length, z_extent), 10.0);
    }

    #[test]
    fn resize_is_immutable_keeps_taper_ends_distinct_and_pins_toed_vectors() {
        let mut bits = Vec::new();
        for lean in [56.0, 58.0, 60.0] {
            let (forest, shoot) = leaning_fixture(lean);
            let before = forest.digest();
            let plan = resize_forest_current_parity(&forest, f(0.0)).unwrap();
            assert_eq!(forest.digest(), before);
            let radii = plan.member_radii(shoot).unwrap();
            assert_eq!(radii.terminalward.get().get().to_bits(), 0.2f64.to_bits());
            assert!(radii.rootward.get().get() > radii.terminalward.get().get());
            bits.push(radii.rootward.get().get().to_bits());
        }
        assert_eq!(
            bits,
            [
                0x3fed_4bd4_1e8f_6ec8,
                0x3fef_7612_1173_dd86,
                0x3ff1_0ea3_edfa_5ea4,
            ]
        );
    }

    #[test]
    fn plan_is_identical_at_one_four_and_sixteen_workspace_threads() {
        const MARKER: &str = "PLINTH_TRANSIENT_THREAD_CHILD";
        if std::env::var_os(MARKER).is_some() {
            let expected_threads = std::env::var("PLINTH_THREADS")
                .unwrap()
                .parse::<usize>()
                .unwrap();
            let pool_probe = geom_kernel::point_segment_distances_batch(
                &[[0.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                &[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
                &[[1.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
                geom_kernel::PointSegmentBatchLimits::new(4, 32),
            )
            .unwrap();
            assert_eq!(pool_probe.effective_threads(), expected_threads);
            assert_eq!(geom_kernel::current_threads(), expected_threads);

            let (mut forest, root, ..) = branched_fixture();
            let brace = forest.add_brace(member(
                100,
                [0.1, 0.0, 5.0],
                [4.0, 0.0, 5.0],
                MemberKind::Brace,
                0.3,
                0.3,
            ));
            let attachments = derive_brace_attachments(&forest).unwrap();
            assert_eq!(attachments[&root][0].brace, brace);
            let plan = resize_forest_current_parity(&forest, f(0.0)).unwrap();
            assert!(!plan.radii.is_empty());
            assert!(!plan.report.members.is_empty());
            println!("TRANSIENT_PLAN={}", serde_json::to_string(&plan).unwrap());
            return;
        }
        let test_name =
            "transient::tests::plan_is_identical_at_one_four_and_sixteen_workspace_threads";
        let outputs = ["1", "4", "16"].map(|threads| {
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", test_name, "--nocapture"])
                .env(MARKER, "1")
                .env("PLINTH_THREADS", threads)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .find(|line| line.starts_with("TRANSIENT_PLAN="))
                .unwrap()
                .to_owned()
        });
        assert!(outputs.windows(2).all(|pair| pair[0] == pair[1]));
    }
}
