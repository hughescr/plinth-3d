//! Current pre-P8 CCO terminal growth.
//!
//! Candidate descriptions are generated serially from one forest snapshot,
//! evaluated independently, flattened into one native clearance batch, and
//! scattered back to their original indices.  Only the deterministic winner
//! is committed to the live forest.
use std::collections::{BTreeMap, BTreeSet};

use geom_kernel::NativeSurface;
use plinth_core::{m0_constants, Finite, PositiveFinite};
use rayon::prelude::*;
use thiserror::Error;

use crate::forest::{
    EmbedEnd, Forest, ForestError, Member, MemberKey, MemberKind, MemberSpec, Owner,
    RootwardEndRadius, TerminalEndRadius,
};
use crate::legacy_compat::{LegacyCompatibility, LegacyCompatibilityError};
use crate::{
    anchor::{
        validate_anchor_feet, AnchorFootOutcome, AnchorFootRefusal, AnchorFootRequest,
        AnchorGeometryAuthority, AnchorSearchConfig, AnchorSearchError, AnchorTerminalBinding,
        QualifiedAnchorSeat, ValidatedModelAnchor,
    },
    BuildFrame, SelectedContact,
};

const MILLIMETRES_PER_MILLILITRE: f64 = 1_000.0;
const DEGENERATE_MM: f64 = 1e-6;
const KDTREE_REBUILD_EVERY: usize = 25;

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct XiaAngles {
    pub(crate) child_one_turn_deg: f64,
    pub(crate) child_two_turn_deg: f64,
    pub(crate) between_children_deg: f64,
}

/// Frozen Xia alpha=1/2 closed form from `cco.py::xia_angles`.
#[cfg(test)]
pub(crate) fn xia_angles(m1: f64, m2: f64) -> XiaAngles {
    let total = m1 + m2;
    if total <= 0.0 {
        return XiaAngles {
            child_one_turn_deg: 45.0,
            child_two_turn_deg: 45.0,
            between_children_deg: 90.0,
        };
    }
    let turn = |load: f64| {
        (load / total)
            .clamp(0.0, 1.0)
            .sqrt()
            .clamp(-1.0, 1.0)
            .acos()
            .to_degrees()
    };
    XiaAngles {
        child_one_turn_deg: turn(m1),
        child_two_turn_deg: turn(m2),
        between_children_deg: 90.0,
    }
}

/// Frozen bounded Weiszfeld polish: exactly `iterations` updates at most.
fn weighted_fermat_point(
    targets: &[([f64; 3], f64)],
    seed: [f64; 3],
    iterations: usize,
) -> [f64; 3] {
    let mut junction = seed;
    for _ in 0..iterations {
        let mut numerator = [0.0; 3];
        let mut denominator = 0.0;
        let mut degenerate = false;
        for &(target, weight) in targets {
            let distance = norm(sub(junction, target));
            if distance < DEGENERATE_MM {
                degenerate = true;
                break;
            }
            for axis in 0..3 {
                numerator[axis] += weight * target[axis] / distance;
            }
            denominator += weight / distance;
        }
        if degenerate || denominator <= 1e-12 {
            break;
        }
        junction = numerator.map(|value| value / denominator);
    }
    junction
}

#[derive(Clone, Debug)]
pub(crate) struct TerminalCcoRequest {
    terminal_id: u32,
    stub: MemberKey,
    stub_snapshot: Member,
    frame: BuildFrame,
}

impl TerminalCcoRequest {
    pub(crate) fn bind(
        forest: &Forest,
        terminal_id: u32,
        frame: BuildFrame,
    ) -> Result<Self, CcoError> {
        let stub = forest
            .terminal(terminal_id)
            .ok_or(CcoError::InvalidTerminalStub {
                terminal_id,
                reason: "terminal map has no live member",
            })?;
        let stub_snapshot = forest
            .member(stub)
            .ok_or(CcoError::InvalidTerminalStub {
                terminal_id,
                reason: "terminal key is stale",
            })?
            .clone();
        validate_terminal_stub(forest, terminal_id, stub, &stub_snapshot)?;
        Ok(Self {
            terminal_id,
            stub,
            stub_snapshot,
            frame,
        })
    }

    fn validate(&self, forest: &Forest) -> Result<(), CcoError> {
        if forest.terminal(self.terminal_id) != Some(self.stub)
            || forest.member(self.stub) != Some(&self.stub_snapshot)
        {
            return Err(CcoError::StaleTerminal(self.terminal_id));
        }
        validate_terminal_stub(forest, self.terminal_id, self.stub, &self.stub_snapshot)
    }

    fn stub_end_mm(&self) -> [Finite; 3] {
        self.stub_snapshot.p1_mm()
    }

    fn carried_mm2(&self) -> Finite {
        self.stub_snapshot.carried_mm2()
    }

    fn tip_radius_mm(&self) -> PositiveFinite {
        self.stub_snapshot.terminal_radius_mm().get()
    }

    fn plate_z_mm(&self) -> Finite {
        self.frame.plate_z_mm()
    }
}

fn validate_terminal_stub(
    forest: &Forest,
    terminal_id: u32,
    stub: MemberKey,
    member: &Member,
) -> Result<(), CcoError> {
    let invalid = |reason| CcoError::InvalidTerminalStub {
        terminal_id,
        reason,
    };
    if !matches!(member.kind(), MemberKind::Stub) {
        return Err(invalid("terminal member is not a stub"));
    }
    if member.owner() != &Owner::Contact(terminal_id) {
        return Err(invalid("stub owner does not match terminal"));
    }
    if member.embed_end() != Some(EmbedEnd::Terminalward) {
        return Err(invalid("stub is not terminalward-embedded"));
    }
    if forest.parent_of(stub).is_some() {
        return Err(invalid("stub is already parented"));
    }
    if norm(sub(
        member.p1_mm().map(Finite::get),
        member.p0_mm().map(Finite::get),
    )) <= DEGENERATE_MM
    {
        return Err(invalid("stub endpoints are degenerate"));
    }
    if member.carried_mm2().get() < 0.0 {
        return Err(invalid("stub carried area is negative"));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CandidateFamily {
    PlateDescent,
    ModelAnchor,
    Bifurcation,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CcoCandidateEvidence {
    pub(crate) family: CandidateFamily,
    pub(crate) generation_index: usize,
    pub(crate) price_ml: f64,
    pub(crate) clearance_rows: usize,
}

#[derive(Debug)]
pub(crate) struct TerminalCcoOutcome {
    pub(crate) winner: Option<CcoCandidateEvidence>,
    pub(crate) candidates_generated: usize,
    pub(crate) candidates_feasible: usize,
    pub(crate) clearance_batches: usize,
    pub(crate) validated_model_anchor: Option<Box<ValidatedModelAnchor>>,
    pub(crate) anchor_refusals: Vec<AnchorFootRefusal>,
}

#[derive(Debug, Error)]
pub(crate) enum CcoError {
    #[error("registered current-support constants unavailable: {0}")]
    Constants(String),
    #[error("terminal {0} is not bound to its requested live stub")]
    StaleTerminal(u32),
    #[error("terminal {terminal_id} has invalid CCO stub state: {reason}")]
    InvalidTerminalStub {
        terminal_id: u32,
        reason: &'static str,
    },
    #[error("anchor route is not the exact binding used for this CCO request")]
    AnchorBindingMismatch,
    #[error("CCO input or derived geometry is not finite and positive where required")]
    InvalidGeometry,
    #[error(transparent)]
    Forest(#[from] ForestError),
    #[error(transparent)]
    Sizing(#[from] LegacyCompatibilityError),
    #[error("native clearance batch failed: {0}")]
    Native(String),
    #[error("serial winner re-application did not reproduce its evaluated forest")]
    NonDeterministicWinner,
    #[error(transparent)]
    Anchor(#[from] AnchorSearchError),
}

#[derive(Clone, Debug)]
enum Descriptor {
    Plate {
        xy: [f64; 2],
        top_z: f64,
    },
    ModelAnchor {
        seat_index: usize,
        point_mm: [f64; 3],
    },
    Bifurcate {
        host: MemberKey,
    },
}

impl Descriptor {
    fn family(&self) -> CandidateFamily {
        match self {
            Self::Plate { .. } => CandidateFamily::PlateDescent,
            Self::ModelAnchor { .. } => CandidateFamily::ModelAnchor,
            Self::Bifurcate { .. } => CandidateFamily::Bifurcation,
        }
    }
}

#[derive(Clone, Debug)]
struct EvaluatedCandidate {
    forest: Forest,
    family: CandidateFamily,
    generation_index: usize,
    price_ml: f64,
    rows: Vec<ClearanceRow>,
    anchor: Option<(usize, PositiveFinite)>,
}

#[derive(Clone, Copy, Debug)]
struct ClearanceRow {
    p0: [f64; 3],
    p1: [f64; 3],
    r0: f64,
    r1: f64,
    inset0: f64,
    inset1: f64,
}

#[derive(Clone, Debug)]
struct Config {
    contact_depth_mm: f64,
    standoff_mm: f64,
    min_shoot_radius_mm: PositiveFinite,
    min_trunk_radius_mm: PositiveFinite,
    branch_angle_deg: f64,
    max_segment_angle_deg: f64,
    cone_tolerance_deg: f64,
    fermat_iterations: usize,
    minimum_split_mm: f64,
    split_start: f64,
    split_end: f64,
    split_step: f64,
    split_steps: usize,
    bifurcation_hosts: usize,
    trunk_directions: usize,
    trunk_radii_mm: Vec<f64>,
    brace_neighbor_max_xy_mm: f64,
    brace_radius_mm: f64,
}

impl Config {
    fn current() -> Result<Self, CcoError> {
        let constants = m0_constants().map_err(|error| CcoError::Constants(error.to_string()))?;
        let p = constants.placeholders();
        let slide = p.cco_split_slide_policy();
        if p.cco_xia_alpha().diagnostic().get() != 0.5 {
            return Err(CcoError::Constants(
                "live CCO port requires registered Xia alpha=0.5".to_owned(),
            ));
        }
        Ok(Self {
            contact_depth_mm: p.default_contact_depth_mm().diagnostic().get(),
            standoff_mm: p.standoff_mm().diagnostic().get(),
            min_shoot_radius_mm: *p.min_shoot_r_mm().diagnostic(),
            min_trunk_radius_mm: *p.min_trunk_r_mm().diagnostic(),
            branch_angle_deg: p.branch_angle_deg().diagnostic().get(),
            max_segment_angle_deg: p.max_segment_angle_deg().diagnostic().get(),
            cone_tolerance_deg: p.cco_cone_tolerance_deg().diagnostic().get(),
            fermat_iterations: p.cco_fermat_max_iterations().diagnostic().get() as usize,
            minimum_split_mm: p.cco_min_split_length_mm().diagnostic().get(),
            split_start: slide.diagnostic().start(),
            split_end: slide.diagnostic().end(),
            split_step: slide.diagnostic().step(),
            split_steps: slide.diagnostic().cap().get() as usize,
            bifurcation_hosts: p.cco_bifurcation_hosts().diagnostic().get() as usize,
            trunk_directions: p.trunk_search_directions().diagnostic().get() as usize,
            trunk_radii_mm: p
                .trunk_search_radii_mm()
                .diagnostic()
                .iter()
                .map(|value| value.get())
                .collect(),
            brace_neighbor_max_xy_mm: p.brace_neighbor_max_xy_mm().diagnostic().get(),
            brace_radius_mm: p.brace_r_mm().diagnostic().get(),
        })
    }
}

struct AnchorValidationContext<'a> {
    contact: &'a SelectedContact,
    terminal: AnchorTerminalBinding,
    config: AnchorSearchConfig,
    geometry: &'a AnchorGeometryAuthority<'a>,
}

pub(crate) struct AnchorTerminalRoute<'a> {
    pub(crate) contact: &'a SelectedContact,
    pub(crate) terminal: AnchorTerminalBinding,
    pub(crate) config: AnchorSearchConfig,
    pub(crate) geometry: &'a AnchorGeometryAuthority<'a>,
    pub(crate) seats: Vec<QualifiedAnchorSeat>,
}

#[derive(Default)]
/// The only integration seam retained for the pending support runner.
pub(crate) struct CcoController {
    hosts: HostCache,
    inserted: usize,
}

#[derive(Default)]
struct HostCache {
    indexed_members: BTreeSet<MemberKey>,
    samples: Vec<([f64; 3], MemberKey)>,
}

impl CcoController {
    pub(crate) fn grow_terminal(
        &mut self,
        forest: &mut Forest,
        request: &TerminalCcoRequest,
        surface: &NativeSurface,
    ) -> Result<TerminalCcoOutcome, CcoError> {
        let config = Config::current()?;
        let hosts = self.hosts.nearest(
            request.stub_end_mm().map(Finite::get),
            config.bifurcation_hosts,
            forest,
        );
        let outcome = grow_terminal_inner(forest, request, surface, Vec::new(), None, Some(hosts))?;
        self.finish_insertion(forest);
        Ok(outcome)
    }

    pub(crate) fn grow_terminal_with_anchor_seats(
        &mut self,
        forest: &mut Forest,
        request: &TerminalCcoRequest,
        route: AnchorTerminalRoute<'_>,
    ) -> Result<TerminalCcoOutcome, CcoError> {
        validate_anchor_route(request, route.terminal)?;
        let config = Config::current()?;
        let hosts = self.hosts.nearest(
            request.stub_end_mm().map(Finite::get),
            config.bifurcation_hosts,
            forest,
        );
        let outcome = grow_terminal_inner(
            forest,
            request,
            route.geometry.surface(),
            route.seats,
            Some(AnchorValidationContext {
                contact: route.contact,
                terminal: route.terminal,
                config: route.config,
                geometry: route.geometry,
            }),
            Some(hosts),
        )?;
        self.finish_insertion(forest);
        Ok(outcome)
    }

    fn finish_insertion(&mut self, forest: &Forest) {
        self.inserted += 1;
        if self.inserted.is_multiple_of(KDTREE_REBUILD_EVERY) {
            self.hosts.rebuild(forest);
        }
    }
}

fn validate_anchor_route(
    request: &TerminalCcoRequest,
    terminal: AnchorTerminalBinding,
) -> Result<(), CcoError> {
    if terminal.terminal_id() != request.terminal_id
        || terminal.stub() != request.stub
        || terminal.stub_end_mm() != request.stub_end_mm()
    {
        return Err(CcoError::AnchorBindingMismatch);
    }
    Ok(())
}

impl HostCache {
    fn rebuild(&mut self, forest: &Forest) {
        self.samples.clear();
        self.indexed_members.clear();
        for key in forest.live_tree_members() {
            let Some(member) = forest.member(key) else {
                continue;
            };
            if matches!(member.kind(), MemberKind::Stub) {
                continue;
            }
            self.indexed_members.insert(key);
            let p0 = member.p0_mm().map(Finite::get);
            let p1 = member.p1_mm().map(Finite::get);
            self.samples.push((p0, key));
            self.samples.push((p1, key));
            self.samples.push((scale(add(p0, p1), 0.5), key));
        }
    }

    fn nearest(&self, point: [f64; 3], count: usize, forest: &Forest) -> Vec<MemberKey> {
        let mut scored = std::collections::BTreeMap::<MemberKey, f64>::new();
        for &(sample, key) in &self.samples {
            if forest.member(key).is_some() {
                let distance = norm(sub(sample, point));
                scored
                    .entry(key)
                    .and_modify(|old| *old = old.min(distance))
                    .or_insert(distance);
            }
        }
        for key in forest.live_tree_members() {
            if self.indexed_members.contains(&key) {
                continue;
            }
            let Some(member) = forest.member(key) else {
                continue;
            };
            if matches!(member.kind(), MemberKind::Stub) {
                continue;
            }
            let midpoint = scale(
                add(
                    member.p0_mm().map(Finite::get),
                    member.p1_mm().map(Finite::get),
                ),
                0.5,
            );
            scored
                .entry(key)
                .or_insert_with(|| norm(sub(midpoint, point)));
        }
        let mut ranked: Vec<(f64, MemberKey)> = scored
            .into_iter()
            .map(|(key, distance)| (distance, key))
            .collect();
        ranked.sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.cmp(&right.1)));
        ranked.into_iter().take(count).map(|(_, key)| key).collect()
    }
}

fn grow_terminal_inner(
    forest: &mut Forest,
    request: &TerminalCcoRequest,
    surface: &NativeSurface,
    mut seats: Vec<QualifiedAnchorSeat>,
    anchor_context: Option<AnchorValidationContext<'_>>,
    cached_hosts: Option<Vec<MemberKey>>,
) -> Result<TerminalCcoOutcome, CcoError> {
    request.validate(forest)?;
    let config = Config::current()?;
    let descriptors = generate_descriptors(forest, request, &config, &seats, cached_hosts);
    let candidates_generated = descriptors.len();
    let snapshot = forest.clone();
    let threads = geom_kernel::current_threads();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .map_err(|error| CcoError::Constants(error.to_string()))?;
    let evaluated: Vec<Result<Option<EvaluatedCandidate>, CcoError>> = pool.install(|| {
        descriptors
            .par_iter()
            .enumerate()
            .map(|(index, descriptor)| evaluate(&snapshot, request, descriptor, index, &config))
            .collect()
    });
    let mut evaluated = evaluated
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let mut validated_anchors = BTreeMap::<usize, Box<ValidatedModelAnchor>>::new();
    let mut anchor_refusals = Vec::new();
    if !seats.is_empty() {
        let context = anchor_context.ok_or(CcoError::InvalidGeometry)?;
        let mut seats: Vec<Option<QualifiedAnchorSeat>> = seats.drain(..).map(Some).collect();
        let mut generation_indices = Vec::new();
        let mut foot_requests = Vec::new();
        for candidate in &evaluated {
            if let Some((seat_index, final_radius)) = candidate.anchor {
                let seat = seats
                    .get_mut(seat_index)
                    .and_then(Option::take)
                    .ok_or(CcoError::InvalidGeometry)?;
                generation_indices.push(candidate.generation_index);
                foot_requests.push(AnchorFootRequest::new(seat, final_radius));
            }
        }
        let validation = validate_anchor_feet(
            foot_requests,
            context.contact,
            context.terminal,
            context.config,
            context.geometry,
        )?;
        let mut valid = BTreeSet::new();
        for (index, outcome) in generation_indices
            .into_iter()
            .zip(validation.into_outcomes())
        {
            match outcome {
                AnchorFootOutcome::Validated(anchor) => {
                    valid.insert(index);
                    validated_anchors.insert(index, anchor);
                }
                AnchorFootOutcome::Refused(refusal) => anchor_refusals.push(refusal),
            }
        }
        evaluated.retain(|candidate| {
            candidate.anchor.is_none() || valid.contains(&candidate.generation_index)
        });
    }
    let candidates_feasible = evaluated.len();
    if evaluated.is_empty() {
        return Ok(TerminalCcoOutcome {
            winner: None,
            candidates_generated,
            candidates_feasible,
            clearance_batches: 0,
            validated_model_anchor: None,
            anchor_refusals,
        });
    }

    let mut p0 = Vec::new();
    let mut p1 = Vec::new();
    let mut r0 = Vec::new();
    let mut r1 = Vec::new();
    let mut inset0 = Vec::new();
    let mut inset1 = Vec::new();
    let mut ranges = Vec::with_capacity(evaluated.len());
    for candidate in &evaluated {
        let start = p0.len();
        for row in &candidate.rows {
            p0.push(row.p0);
            p1.push(row.p1);
            r0.push(row.r0);
            r1.push(row.r1);
            inset0.push(row.inset0);
            inset1.push(row.inset1);
        }
        ranges.push(start..p0.len());
    }
    let (clearances, _) = surface
        .segment_clearance(&p0, &p1, &r0, &r1, config.standoff_mm, &inset0, &inset1)
        .map_err(|error| CcoError::Native(error.to_string()))?;

    let mut survivors: Vec<&EvaluatedCandidate> = evaluated
        .iter()
        .zip(ranges.iter())
        .filter_map(|(candidate, range)| {
            clearances[range.clone()]
                .iter()
                .all(|clearance| *clearance >= 0.0)
                .then_some(candidate)
        })
        .collect();
    survivors.sort_by(|left, right| compare_candidates(left, right));
    let Some(winner) = survivors.first() else {
        return Ok(TerminalCcoOutcome {
            winner: None,
            candidates_generated,
            candidates_feasible,
            clearance_batches: 1,
            validated_model_anchor: None,
            anchor_refusals,
        });
    };
    let winner_index = winner.generation_index;
    let checkpoint = forest.checkpoint();
    match apply_and_size(forest, request, &descriptors[winner_index], &config) {
        Ok(true) if forest.digest() == winner.forest.digest() => forest.commit(checkpoint)?,
        Ok(_) => {
            forest.rollback(checkpoint)?;
            return Err(CcoError::NonDeterministicWinner);
        }
        Err(error) => {
            forest.rollback(checkpoint)?;
            return Err(error);
        }
    }
    Ok(TerminalCcoOutcome {
        winner: Some(CcoCandidateEvidence {
            family: winner.family,
            generation_index: winner.generation_index,
            price_ml: winner.price_ml,
            clearance_rows: winner.rows.len(),
        }),
        candidates_generated,
        candidates_feasible,
        clearance_batches: 1,
        validated_model_anchor: validated_anchors.remove(&winner_index),
        anchor_refusals,
    })
}

fn compare_candidates(left: &EvaluatedCandidate, right: &EvaluatedCandidate) -> std::cmp::Ordering {
    left.price_ml
        .total_cmp(&right.price_ml)
        .then(left.generation_index.cmp(&right.generation_index))
}

fn generate_descriptors(
    forest: &Forest,
    request: &TerminalCcoRequest,
    config: &Config,
    anchor_seats: &[QualifiedAnchorSeat],
    cached_hosts: Option<Vec<MemberKey>>,
) -> Vec<Descriptor> {
    let se = request.stub_end_mm().map(Finite::get);
    let mut out = vec![Descriptor::Plate {
        xy: [se[0], se[1]],
        top_z: se[2],
    }];
    for &radius in &config.trunk_radii_mm {
        if radius == 0.0 {
            continue;
        }
        for direction in 0..config.trunk_directions {
            let angle = std::f64::consts::TAU * direction as f64 / config.trunk_directions as f64;
            let xy = [se[0] + radius * angle.cos(), se[1] + radius * angle.sin()];
            let top_z = se[2] - radius / config.branch_angle_deg.to_radians().tan().max(1e-9);
            if top_z > request.plate_z_mm().get() + 1.0 {
                out.push(Descriptor::Plate { xy, top_z });
            }
        }
    }
    out.extend(
        anchor_seats
            .iter()
            .enumerate()
            .map(|(seat_index, seat)| Descriptor::ModelAnchor {
                seat_index,
                point_mm: seat.point_mm().map(Finite::get),
            }),
    );
    let hosts = cached_hosts
        .unwrap_or_else(|| HostCache::default().nearest(se, config.bifurcation_hosts, forest));
    out.extend(hosts.into_iter().map(|host| Descriptor::Bifurcate { host }));
    out
}

fn evaluate(
    snapshot: &Forest,
    request: &TerminalCcoRequest,
    descriptor: &Descriptor,
    generation_index: usize,
    config: &Config,
) -> Result<Option<EvaluatedCandidate>, CcoError> {
    let before = forest_volume(snapshot)?;
    let mut candidate = snapshot.clone();
    if !apply_and_size(&mut candidate, request, descriptor, config)? {
        return Ok(None);
    }
    candidate.validate()?;
    let after = forest_volume(&candidate)?;
    let price_ml = candidate_price(descriptor, &candidate, request, config, after - before)?;
    if !price_ml.is_finite() {
        return Err(CcoError::InvalidGeometry);
    }
    let rows = clearance_rows(&candidate, snapshot, config);
    let anchor = if let Descriptor::ModelAnchor { seat_index, .. } = descriptor {
        let parent = candidate
            .parent_of(request.stub)
            .and_then(|key| candidate.member(key))
            .ok_or(CcoError::InvalidGeometry)?;
        Some((*seat_index, parent.rootward_radius_mm().get()))
    } else {
        None
    };
    Ok(Some(EvaluatedCandidate {
        forest: candidate,
        family: descriptor.family(),
        generation_index,
        price_ml,
        rows,
        anchor,
    }))
}

fn apply_and_size(
    candidate: &mut Forest,
    request: &TerminalCcoRequest,
    descriptor: &Descriptor,
    config: &Config,
) -> Result<bool, CcoError> {
    let feasible = match descriptor {
        Descriptor::Plate { xy, top_z } => apply_plate(candidate, request, *xy, *top_z, config)?,
        Descriptor::Bifurcate { host } => apply_bifurcation(candidate, request, *host, config)?,
        Descriptor::ModelAnchor { point_mm, .. } => {
            apply_model_anchor(candidate, request, *point_mm, config)?
        }
    };
    if !feasible {
        return Ok(false);
    }
    size_postorder(candidate)?;
    if let Descriptor::Bifurcate { host } = descriptor {
        let upper = candidate
            .parent_of(*host)
            .ok_or(CcoError::InvalidGeometry)?;
        let new_child_radius = candidate
            .parent_of(request.stub)
            .and_then(|key| candidate.member(key))
            .map(Member::rootward_radius_mm)
            .ok_or(CcoError::InvalidGeometry)?;
        uniformly_size_ancestor_path(candidate, upper, new_child_radius)?;
    }
    Ok(true)
}

fn apply_model_anchor(
    forest: &mut Forest,
    request: &TerminalCcoRequest,
    point_mm: [f64; 3],
    config: &Config,
) -> Result<bool, CcoError> {
    let stub_end = request.stub_end_mm().map(Finite::get);
    if norm(sub(point_mm, stub_end)) <= DEGENERATE_MM
        || !within_cone(stub_end, point_mm, config.min_shoot_radius_mm.get(), config)
    {
        return Ok(false);
    }
    let strut = forest.add_member(Member::new(MemberSpec {
        p0_mm: request.stub_end_mm(),
        p1_mm: finite3(point_mm)?,
        r0_mm: TerminalEndRadius::new(cco_terminal_radius(request, config)?),
        r1_mm: RootwardEndRadius::new(config.min_shoot_radius_mm),
        kind: MemberKind::Strut,
        owner: Owner::Contact(request.terminal_id),
        embed_end: Some(EmbedEnd::Rootward),
        carried_mm2: request.carried_mm2(),
    }));
    forest.set_parent(request.stub, Some(strut))?;
    Ok(true)
}

fn apply_plate(
    forest: &mut Forest,
    request: &TerminalCcoRequest,
    xy: [f64; 2],
    top_z: f64,
    config: &Config,
) -> Result<bool, CcoError> {
    let se = request.stub_end_mm().map(Finite::get);
    let top = [xy[0], xy[1], top_z];
    let bottom = [xy[0], xy[1], request.plate_z_mm().get()];
    if top_z - bottom[2] <= DEGENERATE_MM {
        return Ok(false);
    }
    let mut parent = request.stub;
    if norm(sub(top, se)) > DEGENERATE_MM {
        let radius = config.min_trunk_radius_mm;
        if !within_cone(se, top, radius.get(), config) {
            return Ok(false);
        }
        let diagonal = forest.add_member(member(
            se,
            top,
            cco_terminal_radius(request, config)?,
            radius,
            MemberKind::Trunk,
            request,
        )?);
        forest.set_parent(request.stub, Some(diagonal))?;
        parent = diagonal;
    }
    let column = forest.add_member(member(
        top,
        bottom,
        config.min_trunk_radius_mm,
        config.min_trunk_radius_mm,
        MemberKind::Trunk,
        request,
    )?);
    forest.set_parent(parent, Some(column))?;
    Ok(true)
}

fn apply_bifurcation(
    forest: &mut Forest,
    request: &TerminalCcoRequest,
    host_key: MemberKey,
    config: &Config,
) -> Result<bool, CcoError> {
    let Some(host) = forest.member(host_key).cloned() else {
        return Ok(false);
    };
    let p0 = host.p0_mm().map(Finite::get);
    let p1 = host.p1_mm().map(Finite::get);
    let se = request.stub_end_mm().map(Finite::get);
    let direction = sub(p1, p0);
    let length_squared = dot(direction, direction);
    if length_squared < 1e-12 {
        return Ok(false);
    }
    let t0 =
        (dot(sub(se, p0), direction) / length_squared).clamp(config.split_start, config.split_end);
    let seed = add(p0, scale(direction, t0));
    let m1 = host.carried_mm2().get().max(1e-9);
    let m2 = request.carried_mm2().get().max(1e-9);
    let mut junction = weighted_fermat_point(
        &[(p0, m1.sqrt()), (p1, (m1 + m2).sqrt()), (se, m2.sqrt())],
        seed,
        config.fermat_iterations,
    );
    let terminal_radius = cco_terminal_radius(request, config)?;
    if !junction_feasible(junction, se, &host, terminal_radius.get(), config) {
        let mut t = t0;
        let mut found = false;
        for _ in 0..config.split_steps {
            junction = add(p0, scale(direction, t));
            if junction_feasible(junction, se, &host, terminal_radius.get(), config) {
                found = true;
                break;
            }
            t += config.split_step;
            if t > config.split_end {
                break;
            }
        }
        if !found {
            return Ok(false);
        }
    }
    let old_parent = forest.parent_of(host_key);
    let interpolated = radius_along(&host, junction)?;
    let upper = forest.add_member(Member::new(MemberSpec {
        p0_mm: finite3(junction)?,
        p1_mm: host.p1_mm(),
        r0_mm: TerminalEndRadius::new(interpolated),
        r1_mm: host.rootward_radius_mm(),
        kind: host.kind().clone(),
        owner: host.owner().clone(),
        embed_end: host.embed_end(),
        carried_mm2: host.carried_mm2(),
    }));
    forest.set_parent(upper, old_parent)?;
    forest.set_endpoints(host_key, host.p0_mm(), finite3(junction)?)?;
    forest.set_radii(
        host_key,
        host.terminal_radius_mm(),
        RootwardEndRadius::new(interpolated),
    )?;
    if host.embed_end() == Some(EmbedEnd::Rootward) {
        forest.set_embed_end(host_key, None)?;
    }
    forest.set_parent(host_key, Some(upper))?;
    let child = forest.add_member(member(
        se,
        junction,
        terminal_radius,
        config.min_shoot_radius_mm,
        MemberKind::Shoot,
        request,
    )?);
    forest.set_parent(child, Some(upper))?;
    forest.set_parent(request.stub, Some(child))?;
    propagate_carried(forest, upper, request.carried_mm2())?;
    Ok(true)
}

fn size_postorder(forest: &mut Forest) -> Result<(), CcoError> {
    let compatibility = LegacyCompatibility::o10_live_current_support_and_raft();
    for key in forest.postorder()? {
        if forest
            .member(key)
            .is_some_and(|member| matches!(member.kind(), MemberKind::Stub))
        {
            continue;
        }
        let sized = compatibility.size_live_current_support_member(forest, key)?;
        forest.set_radii(key, sized.terminal_continuity(), sized.rootward_radius())?;
    }
    Ok(())
}

fn uniformly_size_ancestor_path(
    forest: &mut Forest,
    mut key: MemberKey,
    mut incoming_radius: RootwardEndRadius,
) -> Result<(), CcoError> {
    let compatibility = LegacyCompatibility::o10_live_current_support_and_raft();
    loop {
        let sized = compatibility.size_live_current_support_member(forest, key)?;
        let entry = forest
            .member(key)
            .ok_or(ForestError::InactiveKey(key))?
            .terminal_radius_mm()
            .get()
            .get();
        let incoming = incoming_radius.get().get();
        let murray = (entry.powi(4) + incoming.powi(4)).powf(0.25);
        let radius = PositiveFinite::new(murray.max(sized.rootward_radius().get().get()))
            .map_err(|_| CcoError::InvalidGeometry)?;
        forest.set_radii(
            key,
            TerminalEndRadius::new(radius),
            RootwardEndRadius::new(radius),
        )?;
        incoming_radius = RootwardEndRadius::new(radius);
        let Some(parent) = forest.parent_of(key) else {
            break;
        };
        key = parent;
    }
    Ok(())
}

fn propagate_carried(
    forest: &mut Forest,
    mut key: MemberKey,
    added: Finite,
) -> Result<(), CcoError> {
    loop {
        let member = forest.member(key).ok_or(ForestError::InactiveKey(key))?;
        let carried = Finite::new(member.carried_mm2().get() + added.get())
            .map_err(|_| CcoError::InvalidGeometry)?;
        forest.set_carried_mm2(key, carried)?;
        let Some(parent) = forest.parent_of(key) else {
            break;
        };
        key = parent;
    }
    Ok(())
}

fn member(
    p0: [f64; 3],
    p1: [f64; 3],
    r0: PositiveFinite,
    r1: PositiveFinite,
    kind: MemberKind,
    request: &TerminalCcoRequest,
) -> Result<Member, CcoError> {
    Ok(Member::new(MemberSpec {
        p0_mm: finite3(p0)?,
        p1_mm: finite3(p1)?,
        r0_mm: TerminalEndRadius::new(r0),
        r1_mm: RootwardEndRadius::new(r1),
        kind,
        owner: Owner::Contact(request.terminal_id),
        embed_end: None,
        carried_mm2: request.carried_mm2(),
    }))
}

fn cco_terminal_radius(
    request: &TerminalCcoRequest,
    config: &Config,
) -> Result<PositiveFinite, CcoError> {
    PositiveFinite::new(
        request
            .tip_radius_mm()
            .get()
            .max(config.min_shoot_radius_mm.get()),
    )
    .map_err(|_| CcoError::InvalidGeometry)
}

fn clearance_rows(forest: &Forest, snapshot: &Forest, config: &Config) -> Vec<ClearanceRow> {
    forest
        .live_tree_members()
        .into_iter()
        .filter_map(|key| {
            let member = forest.member(key)?;
            (snapshot.member(key) != Some(member)).then_some(member)
        })
        .map(|member| ClearanceRow {
            p0: member.p0_mm().map(Finite::get),
            p1: member.p1_mm().map(Finite::get),
            r0: member.terminal_radius_mm().get().get(),
            r1: member.rootward_radius_mm().get().get(),
            inset0: if member.embed_end() == Some(EmbedEnd::Terminalward) {
                config.contact_depth_mm
            } else {
                0.0
            },
            inset1: if member.embed_end() == Some(EmbedEnd::Rootward) {
                config.standoff_mm
            } else {
                0.0
            },
        })
        .collect()
}

fn forest_volume(forest: &Forest) -> Result<f64, CcoError> {
    forest
        .live_tree_members()
        .into_iter()
        .try_fold(0.0, |sum, key| {
            let member = forest.member(key).ok_or(ForestError::InactiveKey(key))?;
            let length = norm(sub(
                member.p1_mm().map(Finite::get),
                member.p0_mm().map(Finite::get),
            ));
            let r0 = member.terminal_radius_mm().get().get();
            let r1 = member.rootward_radius_mm().get().get();
            Ok(sum
                + std::f64::consts::PI / 3.0 * length * (r0 * r0 + r0 * r1 + r1 * r1)
                    / MILLIMETRES_PER_MILLILITRE)
        })
}

fn candidate_price(
    descriptor: &Descriptor,
    candidate: &Forest,
    request: &TerminalCcoRequest,
    config: &Config,
    ordinary_delta_ml: f64,
) -> Result<f64, CcoError> {
    if !matches!(descriptor, Descriptor::Plate { .. }) {
        return Ok(ordinary_delta_ml);
    }
    let first = candidate
        .parent_of(request.stub)
        .ok_or(CcoError::InvalidGeometry)?;
    let (diagonal, column) = if candidate
        .member(first)
        .is_some_and(|member| member.p1_mm()[2].get() == request.plate_z_mm().get())
    {
        (None, first)
    } else {
        (
            Some(first),
            candidate
                .parent_of(first)
                .ok_or(CcoError::InvalidGeometry)?,
        )
    };
    let diagonal_delta = diagonal.map_or(Ok(0.0), |key| {
        let member = candidate.member(key).ok_or(ForestError::InactiveKey(key))?;
        member_volume(member)
    })?;
    let column_member = candidate
        .member(column)
        .ok_or(ForestError::InactiveKey(column))?;
    let column_length = norm(sub(
        column_member.p1_mm().map(Finite::get),
        column_member.p0_mm().map(Finite::get),
    ));
    let compatibility = LegacyCompatibility::o10_live_current_support_and_raft();
    let brace_span = compatibility.brace_free_span_mm().diagnostic().get();
    let priced_span = column_length.min(brace_span);
    let priced_radius = o10_uniform_radius_for_span(
        [0.0, 0.0, 0.0],
        [0.0, 0.0, priced_span],
        MemberKind::Trunk,
        config.min_trunk_radius_mm,
        request.carried_mm2(),
    )?;
    let column_price = std::f64::consts::PI * priced_radius.get().powi(2) * column_length
        / MILLIMETRES_PER_MILLILITRE;
    let rung_count = (column_length / brace_span).floor() as usize;
    let brace_price = if rung_count == 0 {
        0.0
    } else {
        let distance = config.brace_neighbor_max_xy_mm / 2.0;
        let rod_length = distance * 2.0_f64.sqrt();
        let raw = o10_uniform_radius_for_span(
            [0.0, 0.0, 0.0],
            [distance, 0.0, distance],
            MemberKind::Shoot,
            config.min_shoot_radius_mm,
            Finite::new(0.0).map_err(|_| CcoError::InvalidGeometry)?,
        )?;
        let rod_radius = config.brace_radius_mm.max(raw.get() * 0.5_f64.powf(0.25));
        rung_count as f64 * 2.0 * std::f64::consts::PI * rod_radius.powi(2) * rod_length
            / MILLIMETRES_PER_MILLILITRE
    };
    Ok(diagonal_delta + column_price + brace_price)
}

fn o10_uniform_radius_for_span(
    p0: [f64; 3],
    p1: [f64; 3],
    kind: MemberKind,
    floor: PositiveFinite,
    carried_mm2: Finite,
) -> Result<PositiveFinite, CcoError> {
    let mut forest = Forest::new();
    let key = forest.add_member(Member::new(MemberSpec {
        p0_mm: finite3(p0)?,
        p1_mm: finite3(p1)?,
        r0_mm: TerminalEndRadius::new(floor),
        r1_mm: RootwardEndRadius::new(floor),
        kind,
        owner: Owner::Generated {
            family: "cco-price".to_owned(),
            ordinal: 0,
        },
        embed_end: None,
        carried_mm2,
    }));
    let compatibility = LegacyCompatibility::o10_live_current_support_and_raft();
    Ok(compatibility
        .size_live_current_support_member(&forest, key)?
        .rootward_radius()
        .get())
}

fn member_volume(member: &Member) -> Result<f64, CcoError> {
    let length = norm(sub(
        member.p1_mm().map(Finite::get),
        member.p0_mm().map(Finite::get),
    ));
    let r0 = member.terminal_radius_mm().get().get();
    let r1 = member.rootward_radius_mm().get().get();
    let volume = std::f64::consts::PI / 3.0 * length * (r0 * r0 + r0 * r1 + r1 * r1)
        / MILLIMETRES_PER_MILLILITRE;
    volume
        .is_finite()
        .then_some(volume)
        .ok_or(CcoError::InvalidGeometry)
}

fn junction_feasible(
    j: [f64; 3],
    se: [f64; 3],
    host: &Member,
    radius: f64,
    config: &Config,
) -> bool {
    let p0 = host.p0_mm().map(Finite::get);
    let p1 = host.p1_mm().map(Finite::get);
    let direction = sub(p1, p0);
    let length_squared = dot(direction, direction);
    let t = if length_squared < 1e-12 {
        0.0
    } else {
        (dot(sub(j, p0), direction) / length_squared).clamp(0.0, 1.0)
    };
    let host_r0 = host.terminal_radius_mm().get().get();
    let host_r1 = host.rootward_radius_mm().get().get();
    let junction_radius = host_r0 + t * (host_r1 - host_r0);
    j[2] < se[2] - 1e-9
        && norm(sub(j, p0)) >= config.minimum_split_mm
        && norm(sub(j, p1)) >= config.minimum_split_mm
        && within_cone(se, j, radius.max(config.min_shoot_radius_mm.get()), config)
        && within_cone(p0, j, host_r0.max(junction_radius), config)
        && within_cone(j, p1, junction_radius.max(host_r1), config)
}

fn within_cone(p0: [f64; 3], p1: [f64; 3], radius: f64, config: &Config) -> bool {
    let delta = sub(p1, p0);
    let length = norm(delta);
    let lean = if length < 1e-9 {
        0.0
    } else {
        delta[0].hypot(delta[1]).atan2(delta[2].abs()).to_degrees()
    };
    let maximum = if radius >= config.min_shoot_radius_mm.get() {
        config.branch_angle_deg
    } else {
        config.max_segment_angle_deg
    };
    lean <= maximum + config.cone_tolerance_deg
}

fn radius_along(host: &Member, point: [f64; 3]) -> Result<PositiveFinite, CcoError> {
    let p0 = host.p0_mm().map(Finite::get);
    let p1 = host.p1_mm().map(Finite::get);
    let direction = sub(p1, p0);
    let length_squared = dot(direction, direction);
    let t = if length_squared < 1e-12 {
        0.0
    } else {
        (dot(sub(point, p0), direction) / length_squared).clamp(0.0, 1.0)
    };
    let r0 = host.terminal_radius_mm().get().get();
    let r1 = host.rootward_radius_mm().get().get();
    PositiveFinite::new(r0 + t * (r1 - r0)).map_err(|_| CcoError::InvalidGeometry)
}

fn finite3(values: [f64; 3]) -> Result<[Finite; 3], CcoError> {
    values
        .map(Finite::new)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| CcoError::InvalidGeometry)?
        .try_into()
        .map_err(|_| CcoError::InvalidGeometry)
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: [f64; 3]) -> f64 {
    a[0].hypot(a[1]).hypot(a[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_xia_vectors_sum_to_right_angle() {
        let equal = xia_angles(1.0, 1.0);
        assert!((equal.child_one_turn_deg - 45.0).abs() < 1e-12);
        assert!((equal.child_two_turn_deg - 45.0).abs() < 1e-12);
        assert_eq!(equal.between_children_deg, 90.0);
        let split = xia_angles(1.0, 3.0);
        assert!((split.child_one_turn_deg - 60.0).abs() < 1e-12);
        assert!((split.child_two_turn_deg - 30.0).abs() < 1e-12);
        assert!((split.child_one_turn_deg + split.child_two_turn_deg - 90.0).abs() < 1e-12);
        assert_eq!(xia_angles(0.0, 0.0).child_one_turn_deg, 45.0);
    }

    #[test]
    fn brace_price_reads_the_single_o10_placeholder_authority() {
        let compatibility = LegacyCompatibility::o10_live_current_support_and_raft();
        let authority = compatibility.brace_free_span_mm();
        assert_eq!(authority.diagnostic().get(), 18.0);
        assert!(authority.note.contains("O-10 LegacyCompatibility only"));
    }

    #[test]
    fn bounded_weiszfeld_matches_frozen_numeric_vector() {
        let result = weighted_fermat_point(
            &[
                ([0.0, 0.0, 0.0], 1.0),
                ([10.0, 0.0, 0.0], 2.0),
                ([0.0, 10.0, 0.0], 3.0),
            ],
            [2.0, 2.0, 0.0],
            12,
        );
        assert!((result[0] - 1.002_486_160_390_787_5).abs() < 1e-15);
        assert!((result[1] - 8.259_149_868_747_48).abs() < 1e-15);
        assert_eq!(result[2], 0.0);
    }

    #[test]
    fn radius_interpolation_uses_distinct_entry_and_exit_ends() {
        let mut key_source = Forest::new();
        let temporary = Member::new(MemberSpec {
            p0_mm: [Finite::new(0.0).unwrap(); 3],
            p1_mm: [Finite::new(1.0).unwrap(); 3],
            r0_mm: TerminalEndRadius::new(PositiveFinite::new(0.2).unwrap()),
            r1_mm: RootwardEndRadius::new(PositiveFinite::new(0.2).unwrap()),
            kind: MemberKind::Stub,
            owner: Owner::Contact(0),
            embed_end: Some(EmbedEnd::Terminalward),
            carried_mm2: Finite::new(1.0).unwrap(),
        });
        let key = key_source.add_member(temporary);
        key_source.register_terminal(0, key).unwrap();
        let request = TerminalCcoRequest::bind(
            &key_source,
            0,
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
        )
        .unwrap();
        let host = member(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 10.0],
            PositiveFinite::new(0.3).unwrap(),
            PositiveFinite::new(1.2).unwrap(),
            MemberKind::Trunk,
            &request,
        )
        .unwrap();
        let midpoint = radius_along(&host, [0.0, 0.0, 5.0]).unwrap();
        assert!((midpoint.get() - 0.75).abs() < 1e-12);
    }

    #[test]
    fn murray_propagation_uses_entry_radius_and_ignores_dead_children() {
        let finite = |value| Finite::new(value).unwrap();
        let positive = |value| PositiveFinite::new(value).unwrap();
        let make = |r0: f64, r1: f64| {
            Member::new(MemberSpec {
                p0_mm: [finite(0.0), finite(0.0), finite(0.0)],
                p1_mm: [finite(0.0), finite(0.0), finite(1.0)],
                r0_mm: TerminalEndRadius::new(positive(r0)),
                r1_mm: RootwardEndRadius::new(positive(r1)),
                kind: MemberKind::Shoot,
                owner: Owner::Generated {
                    family: "murray-vector".to_owned(),
                    ordinal: 0,
                },
                embed_end: None,
                carried_mm2: finite(0.0),
            })
        };
        let mut forest = Forest::new();
        let parent = forest.add_member(make(0.3, 1.2));
        let child = forest.add_member(make(0.5, 0.5));
        forest.set_parent(child, Some(parent)).unwrap();
        uniformly_size_ancestor_path(&mut forest, parent, RootwardEndRadius::new(positive(0.5)))
            .unwrap();
        let propagated = forest
            .member(parent)
            .unwrap()
            .terminal_radius_mm()
            .get()
            .get();
        assert!((propagated - (0.3_f64.powi(4) + 0.5_f64.powi(4)).powf(0.25)).abs() < 1e-12);

        let mut without_dead = Forest::new();
        let parent = without_dead.add_member(make(0.3, 1.2));
        let dead = without_dead.add_member(make(0.9, 0.9));
        without_dead.set_parent(dead, Some(parent)).unwrap();
        without_dead.retire(dead).unwrap();
        uniformly_size_ancestor_path(
            &mut without_dead,
            parent,
            RootwardEndRadius::new(positive(0.3)),
        )
        .unwrap();
        assert!(
            without_dead
                .member(parent)
                .unwrap()
                .terminal_radius_mm()
                .get()
                .get()
                < 0.9
        );
    }

    #[test]
    fn host_cache_treats_reused_low_slot_as_unindexed_tail() {
        let finite = |value| Finite::new(value).unwrap();
        let positive = |value| PositiveFinite::new(value).unwrap();
        let make_host = |x: f64| {
            Member::new(MemberSpec {
                p0_mm: [finite(x), finite(0.0), finite(0.0)],
                p1_mm: [finite(x), finite(0.0), finite(10.0)],
                r0_mm: TerminalEndRadius::new(positive(0.4)),
                r1_mm: RootwardEndRadius::new(positive(0.8)),
                kind: MemberKind::Trunk,
                owner: Owner::Generated {
                    family: "cache-generation-vector".to_owned(),
                    ordinal: 0,
                },
                embed_end: None,
                carried_mm2: finite(1.0),
            })
        };
        let mut forest = Forest::new();
        let old = forest.add_member(make_host(50.0));
        let mut cache = HostCache::default();
        cache.rebuild(&forest);

        forest.retire(old).unwrap();
        let replacement = forest.add_member(make_host(0.0));
        assert_eq!(replacement.slot(), old.slot());
        assert_ne!(replacement, old);

        assert_eq!(
            cache.nearest([0.0, 0.0, 5.0], 1, &forest),
            vec![replacement]
        );
    }

    #[test]
    fn bound_request_rejects_endpoint_load_radius_and_generation_changes() {
        let (forest, request, _) = growth_fixture();

        let mut changed_endpoint = forest.clone();
        changed_endpoint
            .set_endpoints(
                request.stub,
                request.stub_snapshot.p0_mm(),
                [
                    Finite::new(0.25).unwrap(),
                    request.stub_end_mm()[1],
                    request.stub_end_mm()[2],
                ],
            )
            .unwrap();
        assert!(matches!(
            request.validate(&changed_endpoint),
            Err(CcoError::StaleTerminal(7))
        ));

        let mut changed_load = forest.clone();
        changed_load
            .set_carried_mm2(request.stub, Finite::new(3.0).unwrap())
            .unwrap();
        assert!(matches!(
            request.validate(&changed_load),
            Err(CcoError::StaleTerminal(7))
        ));

        let mut changed_radius = forest.clone();
        changed_radius
            .set_radii(
                request.stub,
                TerminalEndRadius::new(PositiveFinite::new(0.21).unwrap()),
                request.stub_snapshot.rootward_radius_mm(),
            )
            .unwrap();
        assert!(matches!(
            request.validate(&changed_radius),
            Err(CcoError::StaleTerminal(7))
        ));

        let mut reused = forest.clone();
        reused.retire(request.stub).unwrap();
        let replacement = reused.add_member(request.stub_snapshot.clone());
        assert_eq!(replacement.slot(), request.stub.slot());
        assert_ne!(replacement, request.stub);
        assert!(matches!(
            request.validate(&reused),
            Err(CcoError::StaleTerminal(7))
        ));
    }

    #[test]
    fn anchor_route_requires_the_exact_terminal_binding() {
        let (_, request, _) = growth_fixture();
        let different_endpoint = AnchorTerminalBinding::new(
            request.terminal_id,
            request.stub,
            [
                Finite::new(1.0).unwrap(),
                request.stub_end_mm()[1],
                request.stub_end_mm()[2],
            ],
        );
        assert!(matches!(
            validate_anchor_route(&request, different_endpoint),
            Err(CcoError::AnchorBindingMismatch)
        ));

        let exact =
            AnchorTerminalBinding::new(request.terminal_id, request.stub, request.stub_end_mm());
        assert!(validate_anchor_route(&request, exact).is_ok());
    }

    #[test]
    fn request_binding_refuses_non_stub_mismatched_owner_and_parented_state() {
        let frame = BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap());
        let (forest, request, _) = growth_fixture();

        let mut wrong_kind = forest.clone();
        wrong_kind
            .set_kind(request.stub, MemberKind::Shoot)
            .unwrap();
        assert!(matches!(
            TerminalCcoRequest::bind(&wrong_kind, 7, frame),
            Err(CcoError::InvalidTerminalStub { .. })
        ));

        let mut parented = forest.clone();
        let parent = parented.add_member(Member::new(MemberSpec {
            p0_mm: request.stub_end_mm(),
            p1_mm: [
                Finite::new(0.0).unwrap(),
                Finite::new(0.0).unwrap(),
                Finite::new(0.0).unwrap(),
            ],
            r0_mm: TerminalEndRadius::new(PositiveFinite::new(0.35).unwrap()),
            r1_mm: RootwardEndRadius::new(PositiveFinite::new(0.8).unwrap()),
            kind: MemberKind::Trunk,
            owner: Owner::Generated {
                family: "binding-vector".to_owned(),
                ordinal: 0,
            },
            embed_end: None,
            carried_mm2: Finite::new(2.0).unwrap(),
        }));
        parented.set_parent(request.stub, Some(parent)).unwrap();
        assert!(matches!(
            TerminalCcoRequest::bind(&parented, 7, frame),
            Err(CcoError::InvalidTerminalStub { .. })
        ));

        let mut wrong_owner = Forest::new();
        let stub = wrong_owner.add_member(Member::new(MemberSpec {
            p0_mm: request.stub_snapshot.p0_mm(),
            p1_mm: request.stub_snapshot.p1_mm(),
            r0_mm: request.stub_snapshot.terminal_radius_mm(),
            r1_mm: request.stub_snapshot.rootward_radius_mm(),
            kind: MemberKind::Stub,
            owner: Owner::Contact(8),
            embed_end: Some(EmbedEnd::Terminalward),
            carried_mm2: request.stub_snapshot.carried_mm2(),
        }));
        wrong_owner.register_terminal(7, stub).unwrap();
        assert!(matches!(
            TerminalCcoRequest::bind(&wrong_owner, 7, frame),
            Err(CcoError::InvalidTerminalStub { .. })
        ));
    }

    fn growth_fixture() -> (Forest, TerminalCcoRequest, NativeSurface) {
        let finite = |value| Finite::new(value).unwrap();
        let positive = |value| PositiveFinite::new(value).unwrap();
        let mut forest = Forest::new();
        let stub = forest.add_member(Member::new(MemberSpec {
            p0_mm: [finite(0.0), finite(0.0), finite(10.0)],
            p1_mm: [finite(0.0), finite(0.0), finite(12.5)],
            r0_mm: TerminalEndRadius::new(positive(0.2)),
            r1_mm: RootwardEndRadius::new(positive(0.35)),
            kind: MemberKind::Stub,
            owner: Owner::Contact(7),
            embed_end: Some(EmbedEnd::Terminalward),
            carried_mm2: finite(2.0),
        }));
        forest.register_terminal(7, stub).unwrap();
        let request =
            TerminalCcoRequest::bind(&forest, 7, BuildFrame::new(finite(0.0), positive(5.0)))
                .unwrap();
        let vertices = vec![
            [100.0, 100.0, 100.0],
            [101.0, 100.0, 100.0],
            [100.0, 101.0, 100.0],
            [100.0, 100.0, 101.0],
        ];
        let triangles = vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let surface = NativeSurface::from_indexed(
            &vertices,
            &triangles,
            geom_kernel::NativeSurfaceConfig {
                grid_pitch_mm: 0.4,
                max_grid_bytes: 8 * 1024 * 1024,
            },
        )
        .unwrap();
        (forest, request, surface)
    }

    #[test]
    fn candidate_source_ledger_vectors() {
        let (mut forest, request, _) = growth_fixture();
        let finite = |value| Finite::new(value).unwrap();
        let positive = |value| PositiveFinite::new(value).unwrap();
        let host = forest.add_member(Member::new(MemberSpec {
            p0_mm: [finite(1.0), finite(0.0), finite(20.0)],
            p1_mm: [finite(1.0), finite(0.0), finite(0.0)],
            r0_mm: TerminalEndRadius::new(positive(0.45)),
            r1_mm: RootwardEndRadius::new(positive(0.8)),
            kind: MemberKind::Trunk,
            owner: Owner::Generated {
                family: "source-ledger-host".to_owned(),
                ordinal: 0,
            },
            embed_end: None,
            carried_mm2: finite(4.0),
        }));
        let config = Config::current().unwrap();
        let descriptors = [
            Descriptor::Plate {
                xy: [0.0, 0.0],
                top_z: 12.5,
            },
            Descriptor::ModelAnchor {
                seat_index: 0,
                point_mm: [3.0, 0.0, 5.0],
            },
            Descriptor::Bifurcate { host },
        ];
        let candidates = descriptors
            .iter()
            .enumerate()
            .map(|(index, descriptor)| {
                evaluate(&forest, &request, descriptor, index, &config)
                    .unwrap()
                    .unwrap_or_else(|| {
                        panic!("frozen {:?} candidate is feasible", descriptor.family())
                    })
            })
            .collect::<Vec<_>>();
        // Frozen from the live `cco.py` family equations and generation ledger:
        // plate candidates precede model anchors, which precede bifurcations;
        // price wins and generation index is the stable equal-price tiebreak.
        assert_eq!(
            candidates.iter().map(|c| c.price_ml).collect::<Vec<_>>(),
            vec![
                0.025_132_741_228_718_35,
                0.004_674_708_071_732_998,
                0.010_013_605_074_968_252,
            ]
        );
        assert_eq!(
            candidates
                .iter()
                .map(|c| c.forest.digest())
                .collect::<Vec<_>>(),
            vec![
                14_611_506_079_914_898_802,
                12_687_715_610_185_838_542,
                10_584_030_910_390_518_412,
            ]
        );
        let mut by_price = candidates.iter().collect::<Vec<_>>();
        by_price.sort_by(|left, right| compare_candidates(left, right));
        assert_eq!(
            by_price.iter().map(|c| c.family).collect::<Vec<_>>(),
            vec![
                CandidateFamily::ModelAnchor,
                CandidateFamily::Bifurcation,
                CandidateFamily::PlateDescent,
            ]
        );

        let later = evaluate(&forest, &request, &descriptors[0], 9, &config)
            .unwrap()
            .unwrap();
        let earlier = evaluate(&forest, &request, &descriptors[0], 3, &config)
            .unwrap()
            .unwrap();
        assert_eq!(later.price_ml, earlier.price_ml);
        assert_eq!(
            compare_candidates(&earlier, &later),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn terminal_growth_commits_only_the_batched_winner() {
        let (mut forest, request, surface) = growth_fixture();
        let before = forest.digest();
        let outcome = CcoController::default()
            .grow_terminal(&mut forest, &request, &surface)
            .unwrap();
        let winner = outcome.winner.expect("plate descent survives");
        assert_eq!(winner.family, CandidateFamily::PlateDescent);
        assert_eq!(outcome.clearance_batches, 1);
        assert!(outcome.candidates_generated > 1);
        assert!(outcome.candidates_feasible > 0);
        assert_ne!(forest.digest(), before);
        forest.validate().unwrap();
    }

    #[test]
    fn winner_commit_remains_visible_to_an_outer_forest_rollback() {
        let (mut forest, request, surface) = growth_fixture();
        let before = forest.digest();
        let outer = forest.checkpoint();
        CcoController::default()
            .grow_terminal(&mut forest, &request, &surface)
            .unwrap();
        assert_ne!(forest.digest(), before);
        forest.rollback(outer).unwrap();
        assert_eq!(forest.digest(), before);
    }

    #[test]
    fn thread_sweep_is_digest_identical() {
        const CHILD: &str = "PLINTH_CCO_SWEEP_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let (mut forest, request, surface) = growth_fixture();
            let outcome = CcoController::default()
                .grow_terminal(&mut forest, &request, &surface)
                .unwrap();
            println!(
                "CCO_SWEEP_DIGEST={:016x}:{}:{}",
                forest.digest(),
                outcome.winner.unwrap().generation_index,
                outcome.candidates_generated
            );
            return;
        }
        let executable = std::env::current_exe().unwrap();
        let mut digests = Vec::new();
        for threads in [1, 4, 16] {
            let output = std::process::Command::new(&executable)
                .arg("--exact")
                .arg("cco::tests::thread_sweep_is_digest_identical")
                .arg("--nocapture")
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
            let digest = stdout
                .lines()
                .find_map(|line| line.strip_prefix("CCO_SWEEP_DIGEST="))
                .expect("child emitted digest")
                .to_owned();
            digests.push(digest);
        }
        assert!(digests.windows(2).all(|pair| pair[0] == pair[1]));
    }
}
