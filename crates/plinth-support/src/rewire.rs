//! Deterministic current-parity topology rewiring.
//!
//! CCO owns candidate geometry, anchor proof, clearance batching, and replay.
//! This module owns only sweep ordering, the three local topology moves, the
//! degenerate-retirement repair, convergence accounting, and serial commit.
#![allow(
    dead_code,
    reason = "the crate-private support runner integration follows this completed parity pass"
)]

use std::collections::{BTreeSet, VecDeque};

use plinth_core::{m0_constants, Finite, PositiveFinite, CUBIC_MILLIMETRES_PER_MILLILITRE};
use serde::Serialize;
use thiserror::Error;

use crate::{
    anchor::{AnchorGeometryAuthority, AnchorSearchConfig, SurfaceAnchorSample},
    cco::{CcoController, CcoError, RewireAnchorRoute, RewireCcoBinding, RewireSpliceFacts},
    forest::{
        Forest, ForestError, Member, MemberKey, MemberKind, RootwardEndRadius, TerminalEndRadius,
    },
    legacy_compat::{LegacyCompatibility, LegacyCompatibilityError},
    member_sizing::{frustum_volume_ml, LeanAngleDeg, MemberSizingError, MemberSpan},
    transient::{derive_brace_attachments, TransientError},
    BuildFrame,
};

const VOLUME_EPS_ML: f64 = 1e-9;
const SLIDE_MIN_STEP_MM: f64 = 0.02;

pub(crate) struct RewireRequest<'a> {
    pub(crate) frame: BuildFrame,
    pub(crate) anchor_samples: &'a [SurfaceAnchorSample],
    pub(crate) delicacy_by_face: Option<&'a [f64]>,
    pub(crate) anchor_config: AnchorSearchConfig,
    pub(crate) geometry: &'a AnchorGeometryAuthority<'a>,
    /// F3 — the run's own process resolution, forwarded to the CCO
    /// model-anchor seat attestation.
    pub(crate) process_resolution_mm: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RewireTermination {
    FullSweepNoAcceptance,
    RollingSavingsConverged,
    DeterministicWorkBudget,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct RewireReport {
    pub(crate) initial_volume_ml: f64,
    pub(crate) final_volume_ml: f64,
    pub(crate) curve: Vec<(u64, f64)>,
    pub(crate) accepted: u64,
    pub(crate) rejected: u64,
    pub(crate) sweeps: u64,
    pub(crate) move_evaluations: u64,
    pub(crate) cco_candidate_batches: u64,
    /// How many seated MEMBERS this optimization re-cut, over every move
    /// evaluation (Craig's ruling of 2026-08-23; second review, C5/C6).
    ///
    /// `validate_changed_geometry` runs once per candidate move and returns the
    /// count `refreeze_resized_seat_zones` gives it. That count used to be
    /// discarded on the spot, so the ruling's most-travelled site — rewire
    /// re-cuts on EVERY sweep, not once per pass — reported nothing at all.
    ///
    /// The pair with `move_evaluations` is what makes it readable: a run whose
    /// forest holds seated members and whose sizing moves radii should show a
    /// non-zero count here, and a zero against a non-zero
    /// `move_evaluations` says the re-size stopped firing without any verdict
    /// having to change first.
    pub(crate) seat_zone_members_refrozen: u64,
    pub(crate) termination: RewireTermination,
    /// PR-8/OD-9 (F3) — the model-anchor seat-attestation CHAIN over every CCO
    /// candidate batch this optimization evaluated.
    ///
    /// Rewire is the second place the model-anchor family proposes a seat, and
    /// it is the one that runs on EVERY sweep, so the transcript has to be
    /// chained here rather than only at the initial routing pass.
    pub(crate) contact_attestation_digest: u64,
}

/// F3 — fold one rewire CCO batch's transcript into the running chain, keyed by
/// the batch ordinal so two sweeps producing identical transcripts in a
/// different order are not the same chain.
fn extend_rewire_contact_attestation(head: u64, batch: u64, stage: u64) -> u64 {
    let mut digest = crate::forest::DigestWriter::new();
    digest.tag(0x8f);
    digest.u64(head);
    digest.u64(batch);
    digest.u64(stage);
    digest.finish()
}

#[derive(Debug, Error)]
pub(crate) enum RewireError {
    #[error("registered rewire constants unavailable: {0}")]
    Constants(String),
    #[error("registered rewire constant {name} cannot shape a search: {value} — {why}")]
    ConstantUnusable {
        name: &'static str,
        value: String,
        why: &'static str,
    },
    #[error(transparent)]
    Forest(#[from] ForestError),
    #[error(transparent)]
    Cco(#[from] CcoError),
    #[error(transparent)]
    Sizing(#[from] MemberSizingError),
    #[error(transparent)]
    Legacy(#[from] LegacyCompatibilityError),
    #[error(transparent)]
    Transient(#[from] TransientError),
    #[error("native rewire clearance batch failed: {0}")]
    Native(String),
    #[error("rewire arithmetic produced invalid geometry")]
    InvalidGeometry,
    #[error("proof-bound CCO replay did not match the selected move")]
    ReplayMismatch,
    #[error("accepted rewire move left a stale brace endpoint")]
    StaleBrace,
    #[error("re-sizing a seated member's contact exemption failed: {0}")]
    SeatZone(String),
    #[error("a rewire disclosure count is not representable on this platform")]
    CountNotRepresentable,
}

#[derive(Clone, Debug)]
struct Config {
    twig_mm: f64,
    degenerate_mm: f64,
    max_slide_evals: usize,
    roll_window: usize,
    roll_eps: f64,
    work_budget: Option<u64>,
    standoff_mm: f64,
    /// The run's own process resolution — the PR-1(a) seat minimum and the PR-2
    /// mask MARGIN. It is a property of THIS RUN, not a registry constant, so
    /// it arrives as an argument.
    process_resolution_mm: f64,
}

impl Config {
    fn current(process_resolution_mm: f64) -> Result<Self, RewireError> {
        let constants =
            m0_constants().map_err(|error| RewireError::Constants(error.to_string()))?;
        let p = constants.placeholders();
        let config = Self {
            twig_mm: p.rewire_twig_merge_len_mm().diagnostic().get(),
            degenerate_mm: p.rewire_degenerate_len_mm().diagnostic().get(),
            max_slide_evals: usize::try_from(p.rewire_max_slide_evals().diagnostic().get())
                .map_err(|_| RewireError::InvalidGeometry)?,
            roll_window: usize::try_from(p.rewire_roll_window().diagnostic().get())
                .map_err(|_| RewireError::InvalidGeometry)?,
            roll_eps: p.rewire_roll_eps().diagnostic().get(),
            work_budget: p
                .rewire_work_budget()
                .diagnostic()
                .enabled_value()
                .map(|value| value.get()),
            standoff_mm: p.standoff_mm().diagnostic().get(),
            process_resolution_mm,
        };
        config.check_usable()?;
        Ok(config)
    }

    /// **WELL-FORMEDNESS, NOT A FREEZE.**
    ///
    /// This gate used to read each registered value and then compare it,
    /// bit-for-bit, against a hard-coded literal in this file — `0.5`, `1e-4`,
    /// `0.0025`, `50`, `20`. That made the registry the SOURCE of these numbers
    /// and a literal here their AUTHORITY, so retuning the registry could only
    /// ever break this stage, never move it: every value but the one already
    /// written down was refused, and the registry entry was decorative.
    ///
    /// What a search parameter actually has to satisfy is that the search it
    /// shapes still means something. Each condition below names one way the
    /// sweep would stop being a search, and none of them decides an
    /// accept/reject outcome for any candidate move (physics-first constants
    /// doctrine): they only reject a configuration under which the sweep has no
    /// definable behaviour at all.
    fn check_usable(&self) -> Result<(), RewireError> {
        for (value, name, why) in [
            (
                self.twig_mm,
                "REWIRE_TWIG_MERGE_LEN_MM",
                "the twig-merge length is the span below which a single-child member is a twig; \
                 a non-positive or non-finite length puts no member on either side of it",
            ),
            (
                self.degenerate_mm,
                "REWIRE_DEGENERATE_LEN_MM",
                "the degenerate length is the span below which a member is a point and must be \
                 retired; a non-positive or non-finite length retires nothing, ever",
            ),
            (
                self.standoff_mm,
                "STANDOFF_MM",
                "the standoff is the gap this stage demands of every graded row and the band \
                 radius its seat exemptions are cut at; both need a finite, positive distance",
            ),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(RewireError::ConstantUnusable {
                    name,
                    value: value.to_string(),
                    why,
                });
            }
        }
        if !self.roll_eps.is_finite() || self.roll_eps < 0.0 {
            return Err(RewireError::ConstantUnusable {
                name: "REWIRE_ROLL_EPS",
                value: self.roll_eps.to_string(),
                why:
                    "the rolling-convergence fraction multiplies a volume to make the threshold a \
                      window of savings is compared against; a negative fraction can never be met \
                      and a non-finite one is met by every window",
            });
        }
        if self.max_slide_evals == 0 {
            return Err(RewireError::ConstantUnusable {
                name: "REWIRE_MAX_SLIDE_EVALS",
                value: self.max_slide_evals.to_string(),
                why: "the slide move walks at most this many candidates; a budget of zero deletes \
                      the move rather than bounding it",
            });
        }
        if self.roll_window == 0 {
            return Err(RewireError::ConstantUnusable {
                name: "REWIRE_ROLL_WINDOW",
                value: self.roll_window.to_string(),
                why: "the convergence window is full at this many accepted savings; a window of \
                      zero is full before the first move and converges the sweep immediately",
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum MoveKind {
    Prune,
    Reattach,
    Retire,
    Slide,
}

#[derive(Clone, Copy, Debug)]
struct MoveDescriptor {
    saving_key_ml: f64,
    kind: MoveKind,
    member: MemberKey,
}

/// What one attempted move did to the forest, and what the sweep is entitled to
/// conclude from its volume delta.
///
/// **The two accepted arms are not the same answer, which is why the sweep no
/// longer reads a bare `Option<f64>`.**
///
/// [`Self::Saved`] is an OPTIMIZATION result. Reattach, slide and prune each
/// refuse their own candidate unless it lowers the forest's volume, so their
/// delta is a genuine saving and is exactly the evidence
/// [`RewireTermination::RollingSavingsConverged`] grades.
///
/// [`Self::Repaired`] is the degenerate-retirement outcome, and its delta is
/// INCIDENTAL. `try_retire` fires on a member shorter than
/// `REWIRE_DEGENERATE_LEN_MM`, which lowering refuses outright, so the move is
/// taken because the member must not survive — not because it pays. Re-anchoring
/// the orphaned children onto the retiring member's plate-side end and widening
/// the ancestors they now feed can legitimately RAISE the total, and both of
/// this module's degenerate fixtures do exactly that.
///
/// Collapsing the two used to hide that twice over: a repair that grew the
/// forest was reported as `Some(0.0)`, so the sweep's running volume — and with
/// it the disclosed `curve` — drifted one repair further from the
/// `final_volume_ml` recomputed at the end, and twenty such zeroes filled the
/// convergence window with savings that had never happened and stopped the
/// sweep with the rest of the forest unswept.
#[derive(Clone, Copy, Debug, PartialEq)]
enum MoveOutcome {
    /// The candidate was refused; the forest is untouched.
    Declined,
    /// Committed as an improvement. `volume_delta_ml` is `before - after` and
    /// is positive by the move's own acceptance test.
    Saved { volume_delta_ml: f64 },
    /// Committed as a repair. `volume_delta_ml` is `before - after`, SIGNED:
    /// negative means this repair grew the forest, which is a real, disclosable
    /// outcome and not a saving of zero.
    Repaired { volume_delta_ml: f64 },
}

impl MoveOutcome {
    /// `before - after` for a committed move, `None` when nothing was
    /// committed. Positive means the forest got smaller.
    const fn volume_delta_ml(self) -> Option<f64> {
        match self {
            Self::Declined => None,
            Self::Saved { volume_delta_ml } | Self::Repaired { volume_delta_ml } => {
                Some(volume_delta_ml)
            }
        }
    }

    /// The delta the rolling-convergence window may grade. A repair has none:
    /// it was not looking for a saving and its delta says nothing about whether
    /// the search still has savings left to find.
    const fn saving_ml(self) -> Option<f64> {
        match self {
            Self::Saved { volume_delta_ml } => Some(volume_delta_ml),
            Self::Declined | Self::Repaired { .. } => None,
        }
    }

    /// Lift one of the three optimization moves' `Option<f64>` into the shared
    /// outcome. Those moves have no repair arm to choose between.
    const fn from_saving(saving_ml: Option<f64>) -> Self {
        match saving_ml {
            Some(volume_delta_ml) => Self::Saved { volume_delta_ml },
            None => Self::Declined,
        }
    }
}

pub(crate) fn optimize(
    forest: &mut Forest,
    request: RewireRequest<'_>,
) -> Result<RewireReport, RewireError> {
    let config = Config::current(request.process_resolution_mm)?;
    forest.validate()?;
    let initial_volume_ml = total_volume_ml(forest)?;
    let mut current_volume_ml = initial_volume_ml;
    let mut report = RewireReport {
        initial_volume_ml,
        final_volume_ml: initial_volume_ml,
        curve: vec![(0, initial_volume_ml)],
        accepted: 0,
        rejected: 0,
        sweeps: 0,
        move_evaluations: 0,
        cco_candidate_batches: 0,
        seat_zone_members_refrozen: 0,
        termination: RewireTermination::FullSweepNoAcceptance,
        // F3 -- a distinguished non-zero seed, so "no CCO batch ran" and "a
        // batch ran and attested nothing" are different heads.
        contact_attestation_digest: 0x8f00_0000_0000_0001,
    };
    let mut recent = VecDeque::with_capacity(config.roll_window);
    let cco = CcoController::default();

    'sweeps: loop {
        report.sweeps += 1;
        let braced_hosts = derive_brace_attachments(forest)?
            .into_iter()
            .filter_map(|(key, rows)| (!rows.is_empty()).then_some(key))
            .collect::<BTreeSet<_>>();
        let order = sweep_order(forest, &config)?;
        let mut any_accepted = false;
        for descriptor in order {
            if config
                .work_budget
                .is_some_and(|budget| report.move_evaluations >= budget)
            {
                report.termination = RewireTermination::DeterministicWorkBudget;
                break 'sweeps;
            }
            report.move_evaluations += 1;
            let outcome = match descriptor.kind {
                MoveKind::Reattach => MoveOutcome::from_saving(try_reattach(
                    forest,
                    descriptor.member,
                    &request,
                    &cco,
                    &config,
                    &mut report,
                )?),
                MoveKind::Slide => MoveOutcome::from_saving(try_slide(
                    forest,
                    descriptor.member,
                    request.geometry,
                    &braced_hosts,
                    &config,
                    &mut report.seat_zone_members_refrozen,
                )?),
                MoveKind::Prune => MoveOutcome::from_saving(try_prune(
                    forest,
                    descriptor.member,
                    request.geometry,
                    &braced_hosts,
                    &config,
                    &mut report.seat_zone_members_refrozen,
                )?),
                MoveKind::Retire => try_retire(
                    forest,
                    descriptor.member,
                    request.geometry,
                    &braced_hosts,
                    &config,
                    &mut report.seat_zone_members_refrozen,
                )?,
            };
            if let Some(delta_ml) = outcome.volume_delta_ml() {
                any_accepted = true;
                report.accepted += 1;
                // THE TRUE, SIGNED DELTA, whichever arm produced it. A repair
                // that grew the forest moves this the other way, which is what
                // keeps the disclosed `curve` reconcilable against the
                // `final_volume_ml` recomputed once the sweep ends.
                current_volume_ml -= delta_ml;
                report.curve.push((report.accepted, current_volume_ml));
                // ONLY A SAVING MAY CONVERGE THE SWEEP. `saving_ml` is `None`
                // for a degenerate retirement, so a run of repairs neither
                // fills this window with savings that never happened nor —
                // now that the delta is signed — poisons it with a negative
                // that would satisfy the test outright. Repairs cannot run
                // forever: each one permanently retires one member and none
                // creates a new degenerate one, so the sweep still terminates.
                if let Some(saved) = outcome.saving_ml() {
                    recent.push_back(saved);
                    if recent.len() > config.roll_window {
                        recent.pop_front();
                    }
                    if recent.len() == config.roll_window
                        && recent.iter().sum::<f64>()
                            < config.roll_eps * current_volume_ml.max(VOLUME_EPS_ML)
                    {
                        report.termination = RewireTermination::RollingSavingsConverged;
                        break 'sweeps;
                    }
                }
            } else {
                report.rejected += 1;
            }
        }
        if !any_accepted {
            report.termination = RewireTermination::FullSweepNoAcceptance;
            break;
        }
    }
    assert_no_stale_braces(forest)?;
    report.final_volume_ml = total_volume_ml(forest)?;
    Ok(report)
}

fn sweep_order(forest: &Forest, config: &Config) -> Result<Vec<MoveDescriptor>, RewireError> {
    let braces = forest.braces().into_iter().collect::<BTreeSet<_>>();
    let mut moves = Vec::new();
    for key in forest.live_tree_members() {
        let member = forest.member(key).ok_or(ForestError::InactiveKey(key))?;
        if matches!(member.kind(), MemberKind::Stub) || braces.contains(&key) {
            continue;
        }
        let volume = member_volume_ml(member)?;
        let parent = forest.parent_of(key);
        if parent.is_none() || parent.is_some_and(|p| forest.children_of(p).len() >= 2) {
            moves.push(MoveDescriptor {
                saving_key_ml: volume,
                kind: MoveKind::Reattach,
                member: key,
            });
        }
        let children = forest.children_of(key);
        let single_stub = children.len() == 1
            && forest
                .member(children[0])
                .is_some_and(|m| matches!(m.kind(), MemberKind::Stub));
        if children.len() >= 2 {
            moves.push(MoveDescriptor {
                saving_key_ml: volume,
                kind: MoveKind::Slide,
                member: key,
            });
        } else if children.len() == 1 && member_length(member) < config.twig_mm {
            moves.push(MoveDescriptor {
                saving_key_ml: volume,
                kind: MoveKind::Prune,
                member: key,
            });
        }
        if (children.len() != 1 || single_stub) && member_length(member) < config.degenerate_mm {
            moves.push(MoveDescriptor {
                saving_key_ml: if single_stub { f64::INFINITY } else { volume },
                kind: MoveKind::Retire,
                member: key,
            });
        }
    }
    moves.sort_by(|left, right| {
        right
            .saving_key_ml
            .total_cmp(&left.saving_key_ml)
            .then(left.kind.cmp(&right.kind))
            .then(left.member.cmp(&right.member))
    });
    Ok(moves)
}

fn try_reattach(
    forest: &mut Forest,
    key: MemberKey,
    request: &RewireRequest<'_>,
    cco: &CcoController,
    config: &Config,
    report: &mut RewireReport,
) -> Result<Option<f64>, RewireError> {
    let Some(member) = forest.member(key) else {
        return Ok(None);
    };
    if matches!(member.kind(), MemberKind::Stub) {
        return Ok(None);
    }
    let binding = if forest.parent_of(key).is_some() {
        match RewireCcoBinding::bind_detached_subtree(forest, key, request.frame) {
            Ok(binding) => binding,
            Err(CcoError::InvalidRewireRoot { .. }) => return Ok(None),
            Err(error) => return Err(error.into()),
        }
    } else {
        match RewireCcoBinding::bind_root_reanchor(forest, key, request.frame) {
            Ok(binding) => binding,
            Err(CcoError::InvalidRewireRoot { .. }) => return Ok(None),
            Err(error) => return Err(error.into()),
        }
    };
    let before = total_volume_ml(forest)?;
    let mut batch = cco.evaluate_rewire_candidates(
        &binding,
        RewireAnchorRoute {
            samples: request.anchor_samples,
            delicacy_by_face: request.delicacy_by_face,
            config: request.anchor_config,
            geometry: request.geometry,
            process_resolution_mm: request.process_resolution_mm,
        },
    )?;
    report.cco_candidate_batches += 1;
    // C5/C6 -- the CCO batch's own re-size, folded into the same counter
    // `validate_changed_geometry` feeds. Counted BEFORE the early returns
    // below, for the same reason the attestation chain is: a batch that
    // produced no ranked candidate still did the re-cut, and dropping the
    // count would make a live re-size look idle on exactly the sweeps where a
    // regression would be easiest to miss.
    report.seat_zone_members_refrozen = report.seat_zone_members_refrozen.saturating_add(
        u64::try_from(batch.seat_zone_members_refrozen())
            .map_err(|_| RewireError::CountNotRepresentable)?,
    );
    // F3 -- chain this batch's model-anchor seat-attestation transcript. It is
    // chained BEFORE the early returns below, so a batch that produced no
    // ranked candidate still cannot drop its attestation evidence.
    report.contact_attestation_digest = extend_rewire_contact_attestation(
        report.contact_attestation_digest,
        report.cco_candidate_batches,
        batch.seat_attestation_digest(),
    );
    // TRIPWIRE -- the winner is elected through the batch's own commit gate, so
    // a model anchor whose seat witness said `NotAttached` is refused here
    // rather than replayed and spliced into the live forest.
    let Some(generation_index) = batch.winning_generation_index() else {
        return Ok(None);
    };
    let replay = batch.replay_candidate(&binding, forest, generation_index, request.geometry)?;
    let (mut candidate, splice, evidence, _anchor) = replay.into_parts();
    if evidence.generation_index != generation_index {
        return Err(RewireError::ReplayMismatch);
    }
    if let RewireSpliceFacts::RootReanchor {
        retiring_root,
        own_children,
        ..
    } = splice
    {
        let new_parent = candidate
            .parent_of(retiring_root)
            .ok_or(RewireError::ReplayMismatch)?;
        candidate.set_parent(retiring_root, None)?;
        for child in own_children {
            candidate.set_parent(child, Some(new_parent))?;
        }
        candidate.retire(retiring_root)?;
        reconcile_ancestors(&mut candidate, Some(new_parent))?;
    }
    candidate.validate()?;
    // Root-reanchor's splice and Murray reconciliation occur after CCO's
    // proof-bound replay. They can enlarge the connector or its ancestor
    // path, so the final geometry must be proved as written, in one batch,
    // before volume comparison and publication.
    if !validate_changed_geometry(
        forest,
        &mut candidate,
        request.geometry,
        config,
        &mut report.seat_zone_members_refrozen,
    )? {
        return Ok(None);
    }
    let after = total_volume_ml(&candidate)?;
    if after >= before - VOLUME_EPS_ML {
        return Ok(None);
    }
    *forest = candidate;
    Ok(Some(before - after))
}

fn try_slide(
    forest: &mut Forest,
    key: MemberKey,
    geometry: &AnchorGeometryAuthority<'_>,
    braced: &BTreeSet<MemberKey>,
    config: &Config,
    refrozen: &mut u64,
) -> Result<Option<f64>, RewireError> {
    if braced.contains(&key) {
        return Ok(None);
    }
    let Some(member) = forest.member(key).cloned() else {
        return Ok(None);
    };
    let children = forest.children_of(key);
    if children.len() < 2 || children.iter().any(|child| braced.contains(child)) {
        return Ok(None);
    }
    let child_members = children
        .iter()
        .map(|child| {
            forest
                .member(*child)
                .cloned()
                .ok_or(ForestError::InactiveKey(*child))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let Some(search) = ordered_slide_search(&member, &child_members, config)? else {
        return Ok(None);
    };
    if search.best_volume_ml >= search.original_volume_ml - VOLUME_EPS_ML {
        return Ok(None);
    }
    let before = total_volume_ml(forest)?;
    let mut candidate = forest.clone();
    let point = finite3(search.point_mm)?;
    candidate.set_endpoints(key, point, member.p1_mm())?;
    candidate.set_radii(
        key,
        TerminalEndRadius::new(search.radii.member),
        RootwardEndRadius::new(search.radii.member),
    )?;
    for ((child, old), radius) in children
        .iter()
        .zip(&child_members)
        .zip(search.radii.children)
    {
        candidate.set_endpoints(*child, old.p0_mm(), point)?;
        candidate.set_radii(
            *child,
            old.terminal_radius_mm(),
            RootwardEndRadius::new(radius),
        )?;
    }
    let parent = candidate.parent_of(key);
    reconcile_ancestors(&mut candidate, parent)?;
    if !validate_changed_geometry(forest, &mut candidate, geometry, config, refrozen)? {
        return Ok(None);
    }
    let after = total_volume_ml(&candidate)?;
    if after >= before - VOLUME_EPS_ML {
        return Ok(None);
    }
    *forest = candidate;
    Ok(Some(before - after))
}

struct SlideSearch {
    point_mm: [f64; 3],
    radii: JunctionRadii,
    original_volume_ml: f64,
    best_volume_ml: f64,
}

fn ordered_slide_search(
    member: &Member,
    child_members: &[Member],
    config: &Config,
) -> Result<Option<SlideSearch>, RewireError> {
    let start = member.p0_mm().map(Finite::get);
    let Some((_, original_local)) = junction_geometry(start, member, child_members, config)? else {
        return Ok(None);
    };
    let directions = [
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ];
    let mut best = start;
    let mut best_volume = original_local;
    let mut step = 1.0;
    let mut evals = 0;
    while evals < config.max_slide_evals && step > SLIDE_MIN_STEP_MM {
        let mut improved = false;
        // Frozen Python semantics are an ordered coordinate walk, not a six-
        // point stencil: an accepted +X move changes the base point used by
        // the immediately following -X evaluation. This dependency is pure
        // arithmetic and intentionally serial. Native clearance remains one
        // batch after the final point is chosen.
        for direction in directions {
            if evals >= config.max_slide_evals {
                break;
            }
            let point = add(best, scale(direction, step));
            let result = junction_geometry(point, member, child_members, config)?;
            evals += 1;
            if let Some((_, volume)) = result {
                if volume < best_volume - VOLUME_EPS_ML {
                    best = point;
                    best_volume = volume;
                    improved = true;
                }
            }
        }
        if !improved {
            step *= 0.5;
        }
    }
    let Some((radii, _)) = junction_geometry(best, member, child_members, config)? else {
        return Ok(None);
    };
    Ok(Some(SlideSearch {
        point_mm: best,
        radii,
        original_volume_ml: original_local,
        best_volume_ml: best_volume,
    }))
}

struct JunctionRadii {
    children: Vec<PositiveFinite>,
    member: PositiveFinite,
}

fn junction_geometry(
    point: [f64; 3],
    member: &Member,
    children: &[Member],
    config: &Config,
) -> Result<Option<(JunctionRadii, f64)>, RewireError> {
    if distance(point, member.p1_mm().map(Finite::get)) < config.degenerate_mm {
        return Ok(None);
    }
    let compatibility = LegacyCompatibility::o10_live_current_support_and_raft();
    let mut child_radii = Vec::with_capacity(children.len());
    let mut volume = 0.0;
    for child in children {
        let p0 = child.p0_mm().map(Finite::get);
        if distance(p0, point) < config.degenerate_mm {
            return Ok(None);
        }
        let radius = required_radius(&compatibility, child, p0, point, &[])?
            .get()
            .max(child.terminal_radius_mm().get().get());
        let radius = PositiveFinite::new(radius).map_err(|_| RewireError::InvalidGeometry)?;
        if !within_cone(p0, point, radius.get())? {
            return Ok(None);
        }
        volume += frustum_raw(
            distance(p0, point),
            child.terminal_radius_mm().get().get(),
            radius.get(),
        );
        child_radii.push(radius);
    }
    let fanin = child_radii
        .iter()
        .map(|r| r.get().powi(4))
        .sum::<f64>()
        .powf(0.25);
    let p1 = member.p1_mm().map(Finite::get);
    let own = required_radius(
        &compatibility,
        member,
        point,
        p1,
        &child_radii
            .iter()
            .copied()
            .map(RootwardEndRadius::new)
            .collect::<Vec<_>>(),
    )?;
    let member_radius =
        PositiveFinite::new(own.get().max(fanin)).map_err(|_| RewireError::InvalidGeometry)?;
    if !within_cone(point, p1, member_radius.get())? {
        return Ok(None);
    }
    volume += frustum_raw(
        distance(point, p1),
        member_radius.get(),
        member_radius.get(),
    );
    Ok(Some((
        JunctionRadii {
            children: child_radii,
            member: member_radius,
        },
        volume,
    )))
}

fn try_prune(
    forest: &mut Forest,
    key: MemberKey,
    geometry: &AnchorGeometryAuthority<'_>,
    braced: &BTreeSet<MemberKey>,
    config: &Config,
    refrozen: &mut u64,
) -> Result<Option<f64>, RewireError> {
    if braced.contains(&key) {
        return Ok(None);
    }
    let Some(member) = forest.member(key).cloned() else {
        return Ok(None);
    };
    if matches!(member.kind(), MemberKind::Stub) || member_length(&member) >= config.twig_mm {
        return Ok(None);
    }
    let children = forest.children_of(key);
    if children.len() != 1 || braced.contains(&children[0]) {
        return Ok(None);
    }
    let child_key = children[0];
    let child = forest
        .member(child_key)
        .cloned()
        .ok_or(ForestError::InactiveKey(child_key))?;
    if matches!(child.kind(), MemberKind::Stub) {
        return Ok(None);
    }
    let new_p1 = member.p1_mm();
    let compatibility = LegacyCompatibility::o10_live_current_support_and_raft();
    let required = required_radius(
        &compatibility,
        &child,
        child.p0_mm().map(Finite::get),
        new_p1.map(Finite::get),
        &[],
    )?;
    let new_r1 = PositiveFinite::new(
        required
            .get()
            .max(child.rootward_radius_mm().get().get())
            .max(member.rootward_radius_mm().get().get()),
    )
    .map_err(|_| RewireError::InvalidGeometry)?;
    let local_before = member_volume_ml(&member)? + member_volume_ml(&child)?;
    let local_after = frustum_raw(
        distance(child.p0_mm().map(Finite::get), new_p1.map(Finite::get)),
        child.terminal_radius_mm().get().get(),
        new_r1.get(),
    );
    if local_after >= local_before - VOLUME_EPS_ML {
        return Ok(None);
    }
    let before = total_volume_ml(forest)?;
    let mut candidate = forest.clone();
    let parent = candidate.parent_of(key);
    candidate.set_endpoints(child_key, child.p0_mm(), new_p1)?;
    candidate.set_radii(
        child_key,
        child.terminal_radius_mm(),
        RootwardEndRadius::new(new_r1),
    )?;
    candidate.set_parent(child_key, parent)?;
    candidate.retire(key)?;
    reconcile_ancestors(&mut candidate, parent)?;
    if !validate_changed_geometry(forest, &mut candidate, geometry, config, refrozen)? {
        return Ok(None);
    }
    let after = total_volume_ml(&candidate)?;
    if after >= before - VOLUME_EPS_ML {
        return Ok(None);
    }
    *forest = candidate;
    Ok(Some(before - after))
}

/// **THE DEGENERATE-RETIREMENT REPAIR — not an optimization.**
///
/// Every other move in this module is taken because it pays, and refuses its own
/// candidate when it does not. This one is taken because a member shorter than
/// `REWIRE_DEGENERATE_LEN_MM` is a point: as a root it is a plate root whose run
/// to the plate is shorter than `root_boss_top` allows and lowering refuses it
/// outright (`degenerate_root_retire_grounds_its_children_and_the_forest_still_lowers`).
/// Re-anchoring the orphaned children and widening the ancestors they now feed
/// routinely costs more volume than the retired point returns — both of this
/// module's degenerate fixtures grow — so a volume test here would decline the
/// repair in exactly the cases that need it. It commits on legality, and returns
/// [`MoveOutcome::Repaired`] so the sweep books the delta without reading it as
/// a saving.
fn try_retire(
    forest: &mut Forest,
    key: MemberKey,
    geometry: &AnchorGeometryAuthority<'_>,
    braced: &BTreeSet<MemberKey>,
    config: &Config,
    refrozen: &mut u64,
) -> Result<MoveOutcome, RewireError> {
    if braced.contains(&key) {
        return Ok(MoveOutcome::Declined);
    }
    let Some(member) = forest.member(key).cloned() else {
        return Ok(MoveOutcome::Declined);
    };
    if matches!(member.kind(), MemberKind::Stub) || member_length(&member) >= config.degenerate_mm {
        return Ok(MoveOutcome::Declined);
    }
    let children = forest.children_of(key);
    if children.len() == 1
        && forest
            .member(children[0])
            .is_some_and(|m| !matches!(m.kind(), MemberKind::Stub))
    {
        return Ok(MoveOutcome::Declined);
    }
    if children.iter().any(|child| braced.contains(child)) {
        return Ok(MoveOutcome::Declined);
    }
    let before = total_volume_ml(forest)?;
    let mut candidate = forest.clone();
    let parent = candidate.parent_of(key);
    match parent {
        // The ordinary case: every child inherits this member's parent, and
        // with it the route to ground. The member is degenerate, so a child's
        // rootward end already sits within `degenerate_mm` of the junction it
        // moves to and no geometry has to follow.
        Some(_) => {
            for child in children {
                candidate.set_parent(child, parent)?;
            }
        }
        // A root has no parent to inherit, so re-anchoring by reparenting alone
        // would promote each child to a root standing where the retiring
        // member's terminal-ward end was rather than on its plate-side one --
        // the stranding `Forest::retire` now refuses. Splice each child's
        // rootward end down onto the retiring root's own instead, exactly as
        // `try_prune` re-anchors its single child, and the promoted roots are
        // grounded by construction rather than by proximity. Declining the move
        // is not the safe alternative: a degenerate root left standing is a
        // plate root whose run to the plate is shorter than `root_boss_top`
        // allows, which fails lowering outright.
        None => {
            let new_p1 = member.p1_mm();
            let compatibility = LegacyCompatibility::o10_live_current_support_and_raft();
            for child_key in children {
                let child = candidate
                    .member(child_key)
                    .cloned()
                    .ok_or(ForestError::InactiveKey(child_key))?;
                let required = required_radius(
                    &compatibility,
                    &child,
                    child.p0_mm().map(Finite::get),
                    new_p1.map(Finite::get),
                    &[],
                )?;
                // The child's own fan-in is already carried by its current
                // rootward radius, and the retiring root's radius is the floor
                // the plate end had; the splice may only widen.
                let new_r1 = PositiveFinite::new(
                    required
                        .get()
                        .max(child.rootward_radius_mm().get().get())
                        .max(member.rootward_radius_mm().get().get()),
                )
                .map_err(|_| RewireError::InvalidGeometry)?;
                candidate.set_endpoints(child_key, child.p0_mm(), new_p1)?;
                candidate.set_radii(
                    child_key,
                    child.terminal_radius_mm(),
                    RootwardEndRadius::new(new_r1),
                )?;
                candidate.set_parent(child_key, None)?;
            }
        }
    }
    candidate.retire(key)?;
    if parent.is_some() {
        reconcile_ancestors(&mut candidate, parent)?;
    }
    if !validate_changed_geometry(forest, &mut candidate, geometry, config, refrozen)? {
        return Ok(MoveOutcome::Declined);
    }
    let after = total_volume_ml(&candidate)?;
    *forest = candidate;
    // THE SIGNED DELTA. This used to be `(before - after).max(0.0)`, which
    // reported a repair that GREW the forest as a saving of exactly zero. The
    // sweep subtracts what it is told from its running volume, so every clamped
    // zero left the disclosed `curve` one repair further from the
    // `final_volume_ml` recomputed at the end, and twenty of them filled the
    // convergence window with savings that had never happened.
    Ok(MoveOutcome::Repaired {
        volume_delta_ml: before - after,
    })
}

fn reconcile_ancestors(forest: &mut Forest, mut key: Option<MemberKey>) -> Result<(), RewireError> {
    let compatibility = LegacyCompatibility::o10_live_current_support_and_raft();
    while let Some(current) = key {
        let member = forest
            .member(current)
            .cloned()
            .ok_or(ForestError::InactiveKey(current))?;
        let children = forest
            .children_of(current)
            .into_iter()
            .filter_map(|child| forest.member(child).map(|m| m.rootward_radius_mm()))
            .collect::<Vec<_>>();
        let radius = required_radius(
            &compatibility,
            &member,
            member.p0_mm().map(Finite::get),
            member.p1_mm().map(Finite::get),
            &children,
        )?;
        let old = member
            .terminal_radius_mm()
            .get()
            .get()
            .max(member.rootward_radius_mm().get().get());
        if radius.get() <= old + VOLUME_EPS_ML {
            break;
        }
        forest.set_radii(
            current,
            TerminalEndRadius::new(radius),
            RootwardEndRadius::new(radius),
        )?;
        key = forest.parent_of(current);
    }
    Ok(())
}

fn required_radius(
    policy: &LegacyCompatibility,
    member: &Member,
    p0: [f64; 3],
    p1: [f64; 3],
    children: &[RootwardEndRadius],
) -> Result<PositiveFinite, RewireError> {
    let length = PositiveFinite::new(distance(p0, p1).max(1e-6))
        .map_err(|_| RewireError::InvalidGeometry)?;
    let lean = LeanAngleDeg::new(
        Finite::new(lean_deg(p0, p1)).map_err(|_| RewireError::InvalidGeometry)?,
    )?;
    Ok(policy.required_radius_for_geometry(
        member.kind(),
        children,
        member.carried_mm2(),
        length,
        lean,
    )?)
}

/// `refrozen` accumulates the number of seated members the re-size below
/// re-cut, over every candidate this function screens (second review, C5/C6).
///
/// It accumulates whether or not the candidate is then ACCEPTED, because the
/// re-cut is work this call really did on the candidate it really graded — a
/// count that only followed accepted moves would read zero on a sweep that
/// rejected everything and would say nothing about whether the re-size ran.
fn validate_changed_geometry(
    before: &Forest,
    after: &mut Forest,
    geometry: &AnchorGeometryAuthority<'_>,
    config: &Config,
    refrozen: &mut u64,
) -> Result<bool, RewireError> {
    after.validate()?;
    // **THE RE-SIZE — Craig's ruling of 2026-08-23, at rewire's post-sizing
    // point.**
    //
    // Every caller of this function has just rewritten radii on the candidate —
    // `try_slide` sizes a junction and its children, `reconcile_ancestors` walks
    // the path above a splice — and a seated member among them is carrying a
    // band cut at a radius it no longer has. This is the last moment before the
    // PLACEMENT grading below reads those bands, and it is the first moment at
    // which every radius on the candidate is final. ONE batched kernel call.
    //
    // `after` is `&mut` for exactly this and nothing else: the grading itself
    // reads only. Re-cutting inside the grading loop instead would make the
    // exemption a function of the radius being graded, which is the defect the
    // frozen-`Z` rule retired.
    *refrozen = refrozen.saturating_add(
        u64::try_from(
            crate::seat_zone::refreeze_resized_seat_zones(
                geometry.surface(),
                geometry.topology(),
                config.standoff_mm,
                std::iter::once(&mut *after),
            )
            .map_err(|error| RewireError::SeatZone(error.to_string()))?,
        )
        .map_err(|_| RewireError::CountNotRepresentable)?,
    );
    let after: &Forest = after;
    let mut p0 = Vec::new();
    let mut p1 = Vec::new();
    let mut r0 = Vec::new();
    let mut r1 = Vec::new();
    // #36/#51 — the contact-declaring rows, resolved into PR-2 masks below in
    // ONE batched projection rather than each taking the flat inset pair this
    // site used to build. See `contact::placed_mask_inset_mm`.
    let mut contacts: Vec<(
        usize,
        crate::forest::ContactEnd,
        crate::contact::ContactSeating,
        f64,
    )> = Vec::new();
    // **#36 step 5 — one frozen exclusion set per row, in row order.**
    let mut excluded: Vec<&[u32]> = Vec::new();
    const EMPTY: &[u32] = &[];
    for key in after.live_tree_members() {
        let member = after.member(key).ok_or(ForestError::InactiveKey(key))?;
        if before.member(key) == Some(member) {
            continue;
        }
        if !within_cone(
            member.p0_mm().map(Finite::get),
            member.p1_mm().map(Finite::get),
            member
                .terminal_radius_mm()
                .get()
                .get()
                .max(member.rootward_radius_mm().get().get()),
        )? {
            return Ok(false);
        }
        if let Some(end) = member.model_contact_end() {
            contacts.push((
                p0.len(),
                end,
                crate::contact::seating_of(member.kind(), end),
                match end {
                    crate::forest::ContactEnd::Terminalward => {
                        member.terminal_radius_mm().get().get()
                    }
                    crate::forest::ContactEnd::Rootward => member.rootward_radius_mm().get().get(),
                },
            ));
        }
        // A member with no contact forgives nothing; a contact-declaring
        // member with no frozen zone is graded against its own designed embed
        // and refused, which is the strict direction.
        excluded.push(
            member
                .model_contact_end()
                .and_then(|_| after.seat_zone(key))
                .map_or(EMPTY, |zone| zone.as_ref()),
        );
        p0.push(member.p0_mm().map(Finite::get));
        p1.push(member.p1_mm().map(Finite::get));
        r0.push(member.terminal_radius_mm().get().get());
        r1.push(member.rootward_radius_mm().get().get());
    }
    if p0.is_empty() {
        return Ok(true);
    }
    // **#36 step 5 — the PR-2 mask survives here for PR-3 ONLY** (design §5,
    // reader 1). It no longer reaches the kernel's distance path; `emergence_ok`
    // still needs the claimed mask, and under the honest rule PR-3 is more
    // necessary rather than less, because a zone that swallows the whole
    // neighbourhood would otherwise pass by erasure.
    let mut mask_i0 = vec![0.0; p0.len()];
    let mut mask_i1 = vec![0.0; p0.len()];
    // Hoisted out of the contact branch below because the DEMANDED GAP is
    // read for every row, contact-declaring or not: a changed set with no
    // contacts still grades at this stage's registered demand.
    let policy = contact_policy(config)?;
    if !contacts.is_empty() {
        let points = contacts
            .iter()
            .map(|(row, end, _, _)| match end {
                crate::forest::ContactEnd::Terminalward => p0[*row],
                crate::forest::ContactEnd::Rootward => p1[*row],
            })
            .collect::<Vec<_>>();
        // ONE projection for the whole changed set, alongside the one
        // `segment_clearance` that was already here.
        let (_, _, _, normals) = geometry.surface().project(&points);
        let mut claimed = vec![0.0_f64; p0.len()];
        for ((row, end, seating, end_radius_mm), normal) in contacts.iter().zip(normals) {
            let axis = sub(p1[*row], p0[*row]);
            let length = norm(axis);
            if length <= 0.0 {
                return Ok(false);
            }
            let abs_cos = dot(scale(axis, 1.0 / length), normal).abs();
            let r_max = r0[*row].max(r1[*row]);
            // PR-2. A degenerate incidence has no finite mask; this pass
            // VALIDATES an already-accepted move rather than screening a
            // tournament, so it refuses the move rather than exempting the row.
            let Some(mask) = crate::contact::placed_mask_inset_mm(
                *seating,
                abs_cos,
                *end_radius_mm,
                r_max,
                &policy,
            ) else {
                return Ok(false);
            };
            claimed[*row] += mask;
            match end {
                crate::forest::ContactEnd::Terminalward => mask_i0[*row] = mask,
                crate::forest::ContactEnd::Rootward => mask_i1[*row] = mask,
            }
            // PR-3, at the same site as the mask that makes it necessary. A
            // mask that consumes the segment makes `seg_clear_one_excluding` return
            // `INFINITY`; validating a move on that number would be validating
            // nothing.
            if !crate::contact::emerges(claimed[*row], length, &policy) {
                return Ok(false);
            }
        }
    }
    let zero = vec![0.0; p0.len()];
    crate::census_record!(crate::census::record_segment_batch(
        "rewire_clearance",
        &p0,
        &p1,
        // The insets the batch ACTUALLY receives, which are now zero.
        &zero,
        &zero,
        &[]
    ));
    // PLACEMENT. `policy.demanded_gap_mm` is this stage's registered demand,
    // read through the SAME shared predicate the final pass uses; only the
    // number differs (Craig's two-tier ruling, 2026-08-22).
    // #36 step 5 — the FULL axis against surface-minus-Z.
    let clearances = geometry
        .surface()
        .segment_clearance_excluding_typed(
            &p0,
            &p1,
            &r0,
            &r1,
            policy.demanded_gap_mm,
            &zero,
            &zero,
            &excluded,
        )
        .map_err(|e| RewireError::Native(e.to_string()))?;
    // **AN UNTESTED ROW REFUSES.** `clears_or` has no default because the
    // verdict for a row nothing was measured on is a property of the STAGE, and
    // this stage validates a move it is about to commit: an untested row has
    // earned nothing, so `false`.
    //
    // The row that makes this load-bearing is
    // `UnmeasuredSegment::AllFacesExcluded`. The seat exemption assembled into
    // `excluded` above can cover every face the distance walk can reach — a
    // small mesh under a fat arm's band is enough — and the walk then has no
    // candidate to measure against. Through the float entry point this call
    // used to make, that row came back as `f64::INFINITY`, and the
    // `value >= 0.0` this line used to read graded it as the roomiest clearance
    // in the forest: erasure by exemption laundered into a pass, which is
    // precisely what naming the exempt FACES instead of a skipped LENGTH was
    // introduced to prevent. A support was admitted on a check that never ran.
    Ok(clearances.into_iter().all(|row| row.clears_or(false)))
}

/// The PR-1..PR-4 policy this stage validates under — every number the
/// registered one, plus THIS RUN's process resolution.
fn contact_policy(config: &Config) -> Result<crate::contact::ContactPolicy, RewireError> {
    let constants = m0_constants().map_err(|e| RewireError::Constants(e.to_string()))?;
    let p = constants.placeholders();
    Ok(crate::contact::ContactPolicy {
        floor_depth_mm: p.default_contact_depth_mm().diagnostic().get(),
        process_resolution_mm: config.process_resolution_mm,
        standoff_mm: p.standoff_mm().diagnostic().get(),
        // PLACEMENT. `validate_changed_geometry` screens a changed set inside a
        // search: the demand PRUNES and refuses nothing final, so it reads
        // `STANDOFF_MM` and not the refusing `FINAL_CLEARANCE_GAP_MM`.
        demanded_gap_mm: p.standoff_mm().diagnostic().get(),
        min_free_span_mm: p.min_illegal_len_mm().diagnostic().get(),
        ball_factor_stub: p.ball_factor_stub().diagnostic().get(),
        tip_load: crate::perception::FrozenTipLoadPolicy::from_placeholders(p),
    })
}

fn within_cone(p0: [f64; 3], p1: [f64; 3], radius: f64) -> Result<bool, RewireError> {
    let p = m0_constants()
        .map_err(|e| RewireError::Constants(e.to_string()))?
        .placeholders();
    let max = if radius >= p.min_shoot_r_mm().diagnostic().get() {
        p.branch_angle_deg().diagnostic().get()
    } else {
        p.max_segment_angle_deg().diagnostic().get()
    };
    Ok(lean_deg(p0, p1) <= max + p.cco_cone_tolerance_deg().diagnostic().get())
}

fn assert_no_stale_braces(forest: &Forest) -> Result<(), RewireError> {
    let brace_host_tol_mm = m0_constants()
        .map_err(|error| RewireError::Constants(error.to_string()))?
        .placeholders()
        .connectivity_axis_attach_tol_mm()
        .diagnostic()
        .get();
    // ONE BATCH for the whole forest: every brace endpoint that still has to
    // find a host, against every live tree member, in a single kernel call.
    // What this replaces was a scalar fold per endpoint inside a loop over
    // every brace, and it reached the kernel's own point-to-segment arithmetic
    // through a private copy of it in this file — a copy that carried its own
    // `1e-18` degeneracy epsilon. Two copies of one predicate drift, and the
    // copy in the caller is the one nobody tests.
    let hosts = forest.live_tree_members();
    let mut host_starts: Vec<[f64; 3]> = Vec::with_capacity(hosts.len());
    let mut host_ends: Vec<[f64; 3]> = Vec::with_capacity(hosts.len());
    for host in hosts.iter().filter_map(|key| forest.member(*key)) {
        host_starts.push(host.p0_mm().map(Finite::get));
        host_ends.push(host.p1_mm().map(Finite::get));
    }
    let mut endpoints: Vec<[f64; 3]> = Vec::new();
    for brace_key in forest.braces() {
        let brace = forest
            .member(brace_key)
            .ok_or(ForestError::InactiveKey(brace_key))?;
        for (end, point) in [
            (crate::forest::ContactEnd::Terminalward, brace.p0_mm()),
            (crate::forest::ContactEnd::Rootward, brace.p1_mm()),
        ] {
            if brace.model_contact_end() == Some(end) {
                continue;
            }
            endpoints.push(point.map(Finite::get));
        }
    }
    if endpoints.is_empty() {
        return Ok(());
    }
    // Charged over the notional dense window because that is what bounds the
    // WORK; the output this call allocates is one row per ENDPOINT.
    let limits =
        geom_kernel::PointSegmentBatchLimits::for_dimensions(endpoints.len(), host_starts.len())
            .map_err(|error| RewireError::Native(error.to_string()))?;
    let nearest =
        geom_kernel::point_nearest_segment_batch(&endpoints, &host_starts, &host_ends, limits)
            .map_err(|error| RewireError::Native(error.to_string()))?;
    for row in 0..endpoints.len() {
        // AN EMPTY HOST SET HAS NO NEAREST HOST, and that is not "far away".
        // The fold this replaces seeded with `f64::INFINITY` and so reached the
        // same verdict — every brace stale — through a sentinel that happened
        // to point the right way. `None` reaches it on purpose.
        let Some(found) = nearest.nearest(row) else {
            return Err(RewireError::StaleBrace);
        };
        if found.distance_mm() > brace_host_tol_mm {
            return Err(RewireError::StaleBrace);
        }
    }
    Ok(())
}

fn total_volume_ml(forest: &Forest) -> Result<f64, RewireError> {
    forest.live_members().into_iter().try_fold(0.0, |sum, key| {
        let member = forest.member(key).ok_or(ForestError::InactiveKey(key))?;
        if matches!(member.kind(), MemberKind::Stub) && forest.parent_of(key).is_none() {
            Ok(sum)
        } else {
            Ok(sum + member_volume_ml(member)?)
        }
    })
}
fn member_volume_ml(member: &Member) -> Result<f64, RewireError> {
    Ok(frustum_volume_ml(
        MemberSpan::new(member.p0_mm(), member.p1_mm())?,
        member.terminal_radius_mm(),
        member.rootward_radius_mm(),
    )?
    .get()
    .get())
}
fn member_length(member: &Member) -> f64 {
    distance(
        member.p0_mm().map(Finite::get),
        member.p1_mm().map(Finite::get),
    )
}
#[allow(
    clippy::suspicious_operation_groupings,
    reason = "`r0 * r0 + r0 * r1 + r1 * r1` is the exact conical-frustum \
              volume factor; the mixed term is meant to be `r0 * r1`"
)]
fn frustum_raw(length: f64, r0: f64, r1: f64) -> f64 {
    std::f64::consts::PI / 3.0 * length * (r0 * r0 + r0 * r1 + r1 * r1)
        / CUBIC_MILLIMETRES_PER_MILLILITRE
}
fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    norm(sub(a, b))
}
fn lean_deg(p0: [f64; 3], p1: [f64; 3]) -> f64 {
    let d = sub(p1, p0);
    let l = norm(d);
    if l <= 1e-12 {
        0.0
    } else {
        (d[2].abs() / l).clamp(-1.0, 1.0).acos().to_degrees()
    }
}
fn finite3(v: [f64; 3]) -> Result<[Finite; 3], RewireError> {
    v.map(|x| Finite::new(x).map_err(|_| RewireError::InvalidGeometry))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| RewireError::InvalidGeometry)
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        forest::{MemberSpec, Owner},
        joints::{plan_joint_balls_filtered, FilteredLiveView, JointStagingLimits, LiveViewLimits},
        lower::{lower_current, LoweringConfig, LoweringError},
        raft::{plan_bottom_connected, RaftLimits},
    };
    use geom_kernel::{NativeSurface, NativeSurfaceConfig};
    use plinth_core::LayerPlan;
    use plinth_mesh::{build_face_topology, Mesh, WeldPolicy};

    static FAR_DELICACY: [f64; 1] = [0.0];

    fn f(v: f64) -> Finite {
        Finite::new(v).unwrap()
    }
    fn p(v: f64) -> PositiveFinite {
        PositiveFinite::new(v).unwrap()
    }
    fn member(p0: [f64; 3], p1: [f64; 3], kind: MemberKind, n: u32) -> Member {
        Member::new(MemberSpec {
            p0_mm: p0.map(f),
            p1_mm: p1.map(f),
            r0_mm: TerminalEndRadius::new(p(0.4)),
            r1_mm: RootwardEndRadius::new(p(0.4)),
            kind,
            owner: Owner::Generated {
                family: "rewire-test".into(),
                ordinal: n,
            },
            model_contact_end: None,
            carried_mm2: f(1.0),
        })
    }

    fn with_far_authority<T>(run: impl FnOnce(&AnchorGeometryAuthority<'_>) -> T) -> T {
        let mesh = Mesh::new(
            vec![
                [100.0, 100.0, 100.0],
                [101.0, 100.0, 100.0],
                [100.0, 101.0, 100.0],
            ],
            vec![[0, 1, 2]],
        )
        .unwrap();
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let surface = NativeSurface::from_indexed(
            mesh.verts(),
            mesh.tris(),
            NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: 8 * 1024 * 1024,
            },
        )
        .unwrap();
        let authority = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        run(&authority)
    }

    /// Lowers a forest exactly as the runner does, so a topology move can be
    /// asserted to leave a forest the emitter still accepts.
    fn lower_forest(forest: &Forest) -> Result<(), LoweringError> {
        let frame = BuildFrame::new(f(0.0), p(5.0));
        let layers = LayerPlan::new(0.0, vec![0.05; 200]).unwrap();
        let config = LoweringConfig::current(
            frame,
            &layers,
            1_000,
            1_000 * std::mem::size_of::<crate::SupportPrimitive>(),
        )?;
        let view =
            FilteredLiveView::current(forest, &[], LiveViewLimits::new(100, 10, 100_000)).unwrap();
        let joints = plan_joint_balls_filtered(
            forest,
            &view,
            config.joint_config(),
            JointStagingLimits::from_joint_config(config.joint_config()),
        )
        .unwrap();
        let raft = plan_bottom_connected(
            forest,
            &view,
            frame.plate_z_mm(),
            RaftLimits::new(100, 100, 200, 100_000),
        )
        .unwrap();
        lower_current(forest, &view, &joints, &raft, config).map(|_| ())
    }

    fn rewire_request<'a>(geometry: &'a AnchorGeometryAuthority<'a>) -> RewireRequest<'a> {
        RewireRequest {
            frame: BuildFrame::new(f(0.0), p(5.0)),
            anchor_samples: &[],
            delicacy_by_face: Some(&FAR_DELICACY),
            anchor_config: AnchorSearchConfig::current().unwrap(),
            geometry,
            process_resolution_mm: 0.05,
        }
    }

    #[test]
    fn frozen_order_prioritizes_single_stub_degenerate_and_uses_python_ties() {
        let cfg = Config::current(0.05).unwrap();
        let mut forest = Forest::new();
        let root = forest.add_member(member(
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            MemberKind::Trunk,
            0,
        ));
        let deg = forest.add_member(member(
            [0.0, 0.0, 2.0],
            [0.0, 0.0, 2.0 + 5e-5],
            MemberKind::Shoot,
            1,
        ));
        let stub = forest.add_member(member(
            [0.0, 0.0, 3.0],
            [0.0, 0.0, 2.0],
            MemberKind::Stub,
            2,
        ));
        forest.set_parent(deg, Some(root)).unwrap();
        forest.set_parent(stub, Some(deg)).unwrap();
        let order = sweep_order(&forest, &cfg).unwrap();
        assert_eq!((order[0].kind, order[0].member), (MoveKind::Retire, deg));
        assert!(order
            .windows(2)
            .all(|w| w[0].saving_key_ml > w[1].saving_key_ml
                || w[0].saving_key_ml.to_bits() == w[1].saving_key_ml.to_bits()
                    && (w[0].kind, w[0].member) <= (w[1].kind, w[1].member)));
    }

    #[test]
    fn degenerate_retire_splices_children_and_dead_member_never_contributes() {
        let cfg = Config::current(0.05).unwrap();
        let mut forest = Forest::new();
        let parent = forest.add_member(member(
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            MemberKind::Trunk,
            0,
        ));
        let deg = forest.add_member(member(
            [0.0, 0.0, 2.0],
            [0.0, 0.0, 2.0 + 5e-5],
            MemberKind::Shoot,
            1,
        ));
        let a = forest.add_member(member(
            [1.0, 0.0, 3.0],
            [0.0, 0.0, 2.0],
            MemberKind::Shoot,
            2,
        ));
        let b = forest.add_member(member(
            [-1.0, 0.0, 3.0],
            [0.0, 0.0, 2.0],
            MemberKind::Shoot,
            3,
        ));
        forest.set_parent(deg, Some(parent)).unwrap();
        forest.set_parent(a, Some(deg)).unwrap();
        forest.set_parent(b, Some(deg)).unwrap();
        let before = forest.live_tree_members().len();
        let mut candidate = forest.clone();
        with_far_authority(|authority| {
            let outcome = try_retire(
                &mut candidate,
                deg,
                authority,
                &BTreeSet::new(),
                &cfg,
                &mut 0,
            )
            .unwrap();
            assert!(matches!(outcome, MoveOutcome::Repaired { .. }));
            assert!(outcome.volume_delta_ml().is_some());
        });
        assert_eq!(candidate.live_tree_members().len(), before - 1);
        assert_eq!(candidate.parent_of(a), Some(parent));
        assert_eq!(candidate.parent_of(b), Some(parent));
        assert!(candidate.member(deg).is_none());
        let committed_digest = candidate.digest();
        let checkpoint = candidate.checkpoint();
        candidate.retire(a).unwrap();
        assert_ne!(candidate.digest(), committed_digest);
        candidate.rollback(checkpoint).unwrap();
        assert_eq!(candidate.digest(), committed_digest);
        assert_eq!(candidate.parent_of(a), Some(parent));
        assert!(
            member_length(&member(
                [0.0; 3],
                [0.0, 0.0, cfg.degenerate_mm / 2.0],
                MemberKind::Shoot,
                9
            )) < cfg.degenerate_mm
        );
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn degenerate_root_retire_grounds_its_children_and_the_forest_still_lowers() {
        let cfg = Config::current(0.05).unwrap();
        let mut forest = Forest::new();
        // The spliced case above with `deg` on the plate and no parent: its
        // rootward end is the grounded one and its children hang off the
        // terminal-ward end `5e-5 mm` up, well inside `degenerate_mm`.
        let deg = forest.add_member(member(
            [0.0, 0.0, 5e-5],
            [0.0, 0.0, 0.0],
            MemberKind::Shoot,
            0,
        ));
        let a = forest.add_member(member(
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 5e-5],
            MemberKind::Shoot,
            1,
        ));
        let b = forest.add_member(member(
            [-1.0, 0.0, 1.0],
            [0.0, 0.0, 5e-5],
            MemberKind::Shoot,
            2,
        ));
        forest.set_parent(a, Some(deg)).unwrap();
        forest.set_parent(b, Some(deg)).unwrap();
        assert!(forest.parent_of(deg).is_none());

        // Leaving the degenerate root standing is not the conservative
        // direction: it is a plate root whose run to the plate is `5e-5 mm`,
        // and lowering refuses it outright.
        assert!(matches!(
            lower_forest(&forest),
            Err(LoweringError::RootRunTooShort { member }) if member == deg
        ));

        let mut candidate = forest.clone();
        with_far_authority(|authority| {
            let outcome = try_retire(
                &mut candidate,
                deg,
                authority,
                &BTreeSet::new(),
                &cfg,
                &mut 0,
            )
            .unwrap();
            assert!(
                matches!(outcome, MoveOutcome::Repaired { .. }),
                "a grounded degenerate root's children can inherit its plate end"
            );
        });
        assert!(candidate.member(deg).is_none());
        assert_eq!(candidate.roots(), vec![a, b]);
        for child in [a, b] {
            let member = candidate.member(child).unwrap();
            assert!(candidate.parent_of(child).is_none());
            // Grounded by construction: the promoted root inherited the
            // retired root's plate-side end, not the junction it hung from.
            assert_eq!(
                member.p1_mm().map(Finite::get),
                forest.member(deg).unwrap().p1_mm().map(Finite::get)
            );
            // The splice may only widen the rootward end.
            assert!(
                member.rootward_radius_mm().get().get()
                    >= forest
                        .member(child)
                        .unwrap()
                        .rootward_radius_mm()
                        .get()
                        .get()
            );
        }
        candidate.validate().unwrap();
        lower_forest(&candidate).expect("the post-move forest lowers");
    }

    #[test]
    fn constants_and_disabled_budget_pin_exact_search_contract() {
        let cfg = Config::current(0.05).unwrap();
        assert_eq!(cfg.twig_mm.to_bits(), 0.5f64.to_bits());
        assert_eq!(cfg.degenerate_mm.to_bits(), 1e-4f64.to_bits());
        assert_eq!(cfg.max_slide_evals, 50);
        assert_eq!(cfg.roll_window, 20);
        assert_eq!(cfg.roll_eps.to_bits(), 0.0025f64.to_bits());
        assert_eq!(cfg.work_budget, None);
    }

    #[test]
    fn post_splice_radius_growth_rechecks_clearance_before_publication() {
        let mesh = Mesh::new(
            vec![[1.0, -10.0, 0.0], [1.0, 10.0, 0.0], [1.0, 0.0, 10.0]],
            vec![[0, 1, 2]],
        )
        .unwrap();
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let surface = NativeSurface::from_indexed(
            mesh.verts(),
            mesh.tris(),
            NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: 8 * 1024 * 1024,
            },
        )
        .unwrap();
        let authority = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let mut before = Forest::new();
        let key = before.add_member(member(
            [0.0, 0.0, 9.0],
            [0.0, 0.0, 1.0],
            MemberKind::Trunk,
            40,
        ));
        before
            .set_radii(
                key,
                TerminalEndRadius::new(p(0.1)),
                RootwardEndRadius::new(p(0.1)),
            )
            .unwrap();
        let mut grown = before.clone();
        grown
            .set_radii(
                key,
                TerminalEndRadius::new(p(0.5)),
                RootwardEndRadius::new(p(0.5)),
            )
            .unwrap();
        assert!(!validate_changed_geometry(
            &before,
            &mut grown,
            &authority,
            &Config::current(0.05).unwrap(),
            &mut 0,
        )
        .unwrap());
        assert_eq!(
            before.member(key).unwrap().rootward_radius_mm().get(),
            p(0.1)
        );
    }

    /// **C5/C6 — the count `validate_changed_geometry` used to discard now
    /// reaches the report, and it counts a REFUSED candidate too.**
    ///
    /// The second review's disclosure finding at rewire's site: the re-size
    /// here runs on EVERY move evaluation, which makes it the ruling's
    /// most-travelled site, and it reported nothing at all.
    ///
    /// The screened candidate below is REFUSED — this fixture's seated arm is
    /// grown from 0.1 mm to 0.5 mm against a wall it cannot clear, which is
    /// what `post_splice_radius_growth_rechecks_clearance_before_publication`
    /// above measures. That is the case worth pinning: a counter that only
    /// followed accepted moves would read zero across a sweep that rejected
    /// everything and could not distinguish it from a re-size that had stopped
    /// running.
    #[test]
    fn the_re_size_count_reaches_the_report_even_when_the_candidate_is_refused() {
        // The same near wall at `x = 1` that
        // `post_splice_radius_growth_rechecks_clearance_before_publication`
        // builds, so the two measure one geometry.
        let mesh = Mesh::new(
            vec![[1.0, -10.0, 0.0], [1.0, 10.0, 0.0], [1.0, 0.0, 10.0]],
            vec![[0, 1, 2]],
        )
        .unwrap();
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let surface = NativeSurface::from_indexed(
            mesh.verts(),
            mesh.tris(),
            NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: 8 * 1024 * 1024,
            },
        )
        .unwrap();
        let authority = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let config = Config::current(0.05).unwrap();

        let p0_mm = [0.9, 0.0, 4.0];
        let p1_mm = [1.0, 0.0, 3.0];
        let mut before = Forest::new();
        let key = before.add_member(Member::new(MemberSpec {
            p0_mm: p0_mm.map(f),
            p1_mm: p1_mm.map(f),
            r0_mm: TerminalEndRadius::new(p(0.1)),
            r1_mm: RootwardEndRadius::new(p(0.1)),
            kind: MemberKind::Trunk,
            owner: Owner::Generated {
                family: "c5-count".into(),
                ordinal: 0,
            },
            model_contact_end: Some(crate::forest::ContactEnd::Rootward),
            carried_mm2: f(1.0),
        }));
        let (_, _, face_ids, _) = surface.project(&[p1_mm]);
        let record = crate::seat_zone::freeze_production_seat_zones(
            &surface,
            &topology,
            &[crate::seat_zone::SeatZoneRequest {
                p0_mm,
                p1_mm,
                seat_face_id: u32::try_from(face_ids[0]).unwrap(),
                band_radius_mm: 0.1 + config.standoff_mm,
            }],
        )
        .unwrap()
        .remove(0);
        before.set_seat_zone(key, record);

        let mut grown = before.clone();
        grown
            .set_radii(
                key,
                TerminalEndRadius::new(p(0.5)),
                RootwardEndRadius::new(p(0.5)),
            )
            .unwrap();

        let mut refrozen = 0_u64;
        let admitted =
            validate_changed_geometry(&before, &mut grown, &authority, &config, &mut refrozen)
                .unwrap();
        assert_eq!(
            refrozen, 1,
            "the grown seated member must be re-cut, and the count must say so \
             — this is the number the site used to drop on the floor"
        );
        assert!(
            !admitted,
            "and it is counted whether or not the candidate survives: this one \
             does not"
        );
    }

    #[test]
    fn live_prune_root_reanchor_and_optimizer_paths_are_transactional() {
        with_far_authority(|authority| {
            let cfg = Config::current(0.05).unwrap();

            let mut prune = Forest::new();
            let short = prune.add_member(member(
                [0.3, 0.0, 1.3],
                [0.0, 0.0, 1.0],
                MemberKind::Trunk,
                0,
            ));
            let child = prune.add_member(member(
                [0.0, 0.0, 2.0],
                [0.3, 0.0, 1.3],
                MemberKind::Shoot,
                1,
            ));
            prune
                .set_radii(
                    short,
                    TerminalEndRadius::new(p(5.0)),
                    RootwardEndRadius::new(p(5.0)),
                )
                .unwrap();
            prune
                .set_radii(
                    child,
                    TerminalEndRadius::new(p(5.0)),
                    RootwardEndRadius::new(p(5.0)),
                )
                .unwrap();
            prune.set_parent(child, Some(short)).unwrap();
            assert!(
                try_prune(&mut prune, short, authority, &BTreeSet::new(), &cfg, &mut 0,)
                    .unwrap()
                    .is_some()
            );
            assert!(prune.member(short).is_none());

            let mut roots = Forest::new();
            let moving = roots.add_member(member(
                [0.0, 0.0, 10.0],
                [0.0, 0.0, 0.0],
                MemberKind::Trunk,
                10,
            ));
            let host = roots.add_member(member(
                [1.0, 0.0, 10.0],
                [1.0, 0.0, 0.0],
                MemberKind::Trunk,
                11,
            ));
            let mut evidence = RewireReport {
                initial_volume_ml: total_volume_ml(&roots).unwrap(),
                final_volume_ml: total_volume_ml(&roots).unwrap(),
                curve: Vec::new(),
                accepted: 0,
                rejected: 0,
                sweeps: 0,
                move_evaluations: 0,
                cco_candidate_batches: 0,
                seat_zone_members_refrozen: 0,
                termination: RewireTermination::FullSweepNoAcceptance,
                contact_attestation_digest: 0x8f00_0000_0000_0001,
            };
            let roots_before = roots.digest();
            assert!(try_reattach(
                &mut roots,
                moving,
                &rewire_request(authority),
                &CcoController::default(),
                &cfg,
                &mut evidence,
            )
            .unwrap()
            .is_none());
            assert_eq!(roots.digest(), roots_before);
            assert!(roots.member(moving).is_some());
            assert!(roots.member(host).is_some());
            assert_eq!(evidence.cco_candidate_batches, 1);

            let mut detached = Forest::new();
            let old_parent = detached.add_member(member(
                [5.0, 0.0, 5.0],
                [5.0, 0.0, 0.0],
                MemberKind::Trunk,
                12,
            ));
            let subtree = detached.add_member(member(
                [5.0, 0.0, 10.0],
                [5.0, 0.0, 5.0],
                MemberKind::Shoot,
                13,
            ));
            let sibling = detached.add_member(member(
                [6.0, 0.0, 10.0],
                [5.0, 0.0, 5.0],
                MemberKind::Shoot,
                14,
            ));
            detached.set_parent(subtree, Some(old_parent)).unwrap();
            detached.set_parent(sibling, Some(old_parent)).unwrap();
            let detached_before = detached.digest();
            let mut detached_evidence = evidence.clone();
            detached_evidence.cco_candidate_batches = 0;
            let detached_result = try_reattach(
                &mut detached,
                subtree,
                &rewire_request(authority),
                &CcoController::default(),
                &cfg,
                &mut detached_evidence,
            )
            .unwrap();
            assert_eq!(detached_evidence.cco_candidate_batches, 1);
            if detached_result.is_none() {
                assert_eq!(detached.digest(), detached_before);
            } else {
                assert_ne!(detached.digest(), detached_before);
            }

            let mut optimize_forest = Forest::new();
            let parent = optimize_forest.add_member(member(
                [20.0, 0.0, 5.0],
                [20.0, 0.0, 0.0],
                MemberKind::Trunk,
                20,
            ));
            let degenerate = optimize_forest.add_member(member(
                [20.0, 0.0, 6.0],
                [20.0, 0.0, 6.0 + 5e-5],
                MemberKind::Shoot,
                21,
            ));
            optimize_forest
                .set_parent(degenerate, Some(parent))
                .unwrap();
            let report = optimize(&mut optimize_forest, rewire_request(authority)).unwrap();
            assert!(report.accepted >= 1);
            assert!(optimize_forest.member(degenerate).is_none());
            assert_eq!(report.termination, RewireTermination::FullSweepNoAcceptance);
        });
    }

    /// FINDING D — terminal consumption makes a stub mutable, and `try_slide`
    /// can then move the endpoint its seat-zone record was cut for.
    ///
    /// The record starts on the old axis with the final radius already in place,
    /// so only the axis comparison can make `validate_changed_geometry` re-cut
    /// it. A radius-only gate leaves the record's stored endpoint at the old
    /// junction and reports zero re-cuts.
    ///
    /// **The seat face is near both axes and the stub's zone exempts it, so a
    /// clearance refusal cannot be what this test measures — but the surface
    /// carries a SECOND face, far from everything, so that something is still
    /// left to grade.** This fixture used to be one triangle, and the stub's
    /// zone therefore exempted every face on the mesh; the row came back
    /// unmeasured, the float entry point encoded that as `f64::INFINITY`, and
    /// the admissibility rule read it as the roomiest clearance in the forest.
    /// The slide was admitted on a check that never ran. The honest rule refuses
    /// an unmeasured row, so the surface has to be one a seat band cannot
    /// swallow whole — which is what a real model is.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn try_slide_re_cuts_a_consumed_stubs_zone_to_follow_its_new_endpoint() {
        let mesh = Mesh::new(
            vec![
                // The seat face, under the stub's plate-side end.
                [-0.2, -0.2, 0.0],
                [0.2, -0.2, 0.0],
                [0.0, 0.2, 0.0],
                // A second, unconnected face 20 mm away in +x: too far for the
                // seat band to reach and too far to touch either axis, so the
                // exemption cannot cover the whole surface and the graded rows
                // still have something to be measured against.
                [20.0, -0.2, 0.0],
                [20.0, 0.2, 0.0],
                [20.0, 0.0, 0.4],
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        )
        .unwrap();
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let surface = NativeSurface::from_indexed(
            mesh.verts(),
            mesh.tris(),
            NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: 8 * 1024 * 1024,
            },
        )
        .unwrap();
        let authority = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let config = Config::current(0.05).unwrap();
        let old_p0 = [5.0, 0.0, 5.0];
        let p1 = [0.0, 0.0, 0.0];
        let left_p0 = [-1.0, 0.0, 10.0];
        let right_p0 = [1.0, 0.0, 10.0];

        // Compute the exact radius the accepted slide will write, and create the
        // stub at that radius. This makes the move axis-only: a radius-only
        // change gate cannot fire by accident.
        let geometry_stub = Member::new(MemberSpec {
            p0_mm: old_p0.map(f),
            p1_mm: p1.map(f),
            r0_mm: TerminalEndRadius::new(p(0.4)),
            r1_mm: RootwardEndRadius::new(p(0.4)),
            kind: MemberKind::Stub,
            owner: Owner::Contact(7),
            model_contact_end: Some(crate::forest::ContactEnd::Terminalward),
            carried_mm2: f(1.0),
        });
        let geometry_children = vec![
            member(left_p0, old_p0, MemberKind::Shoot, 1),
            member(right_p0, old_p0, MemberKind::Shoot, 2),
        ];
        let search = ordered_slide_search(&geometry_stub, &geometry_children, &config)
            .unwrap()
            .unwrap();
        let final_radius = search.radii.member;

        let mut forest = Forest::new();
        let stub = forest.add_member(Member::new(MemberSpec {
            p0_mm: old_p0.map(f),
            p1_mm: p1.map(f),
            r0_mm: TerminalEndRadius::new(final_radius),
            r1_mm: RootwardEndRadius::new(final_radius),
            kind: MemberKind::Stub,
            owner: Owner::Contact(7),
            model_contact_end: Some(crate::forest::ContactEnd::Terminalward),
            carried_mm2: f(1.0),
        }));
        let left = forest.add_member(member(left_p0, old_p0, MemberKind::Shoot, 1));
        let right = forest.add_member(member(right_p0, old_p0, MemberKind::Shoot, 2));
        forest.set_parent(left, Some(stub)).unwrap();
        forest.set_parent(right, Some(stub)).unwrap();
        forest.register_terminal(7, stub).unwrap();
        forest.consume_terminal(7, stub).unwrap();

        let created = crate::seat_zone::freeze_production_seat_zones(
            &surface,
            &topology,
            &[crate::seat_zone::SeatZoneRequest {
                p0_mm: old_p0,
                p1_mm: p1,
                seat_face_id: 0,
                band_radius_mm: final_radius.get() + config.standoff_mm,
            }],
        )
        .unwrap()
        .remove(0);
        assert_eq!(created.zone().as_ref(), &[0], "fixture control");
        forest.set_seat_zone(stub, created);

        let mut refrozen = 0_u64;
        let saved = try_slide(
            &mut forest,
            stub,
            &authority,
            &BTreeSet::new(),
            &config,
            &mut refrozen,
        )
        .unwrap();
        assert!(saved.is_some(), "the consumed stub must take the slide");
        assert_eq!(
            refrozen, 1,
            "the endpoint-only move must trigger one re-cut"
        );
        let moved_p0 = forest.member(stub).unwrap().p0_mm().map(Finite::get);
        assert_ne!(moved_p0, old_p0, "try_slide must really move the stub");
        let record = forest.seat_zone_record(stub).unwrap();
        assert_eq!(
            record.p0_mm(),
            moved_p0,
            "the zone must follow the new endpoint"
        );
        assert_eq!(record.p1_mm(), p1);
        assert_eq!(record.zone().as_ref(), &[0]);
    }

    #[test]
    fn ordered_slide_walk_pins_frozen_path_and_is_thread_setting_independent() {
        const CHILD: &str = "PLINTH_REWIRE_SLIDE_WALK_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let mesh = Mesh::new(
                vec![
                    [100.0, 100.0, 100.0],
                    [101.0, 100.0, 100.0],
                    [100.0, 101.0, 100.0],
                ],
                vec![[0, 1, 2]],
            )
            .unwrap();
            let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
            let surface = NativeSurface::from_indexed(
                mesh.verts(),
                mesh.tris(),
                NativeSurfaceConfig {
                    grid_pitch_mm: 1.0,
                    max_grid_bytes: 8 * 1024 * 1024,
                },
            )
            .unwrap();
            let authority = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
            let mut forest = Forest::new();
            let junction = forest.add_member(member(
                [5.0, 0.0, 5.0],
                [0.0, 0.0, 0.0],
                MemberKind::Trunk,
                0,
            ));
            let left = forest.add_member(member(
                [-1.0, 0.0, 10.0],
                [5.0, 0.0, 5.0],
                MemberKind::Shoot,
                1,
            ));
            let right = forest.add_member(member(
                [1.0, 0.0, 10.0],
                [5.0, 0.0, 5.0],
                MemberKind::Shoot,
                2,
            ));
            forest.set_parent(left, Some(junction)).unwrap();
            forest.set_parent(right, Some(junction)).unwrap();
            let junction_member = forest.member(junction).unwrap().clone();
            let child_members = vec![
                forest.member(left).unwrap().clone(),
                forest.member(right).unwrap().clone(),
            ];
            let walk = ordered_slide_search(
                &junction_member,
                &child_members,
                &Config::current(0.05).unwrap(),
            )
            .unwrap()
            .unwrap();
            assert!(walk.best_volume_ml < walk.original_volume_ml);
            let saved = try_slide(
                &mut forest,
                junction,
                &authority,
                &BTreeSet::new(),
                &Config::current(0.05).unwrap(),
                &mut 0,
            )
            .unwrap();
            println!(
                "REWIRE_SLIDE_WALK={}:{}:{:016x}:{:016x}:{:016x}:{:016x}",
                geom_kernel::current_threads(),
                forest.digest(),
                saved.unwrap_or_default().to_bits(),
                walk.point_mm[0].to_bits(),
                walk.point_mm[1].to_bits(),
                walk.point_mm[2].to_bits()
            );
            return;
        }
        let executable = std::env::current_exe().unwrap();
        let mut stable_payload = Vec::new();
        for threads in [1, 4, 16] {
            let output = std::process::Command::new(&executable)
                .arg("--exact")
                .arg("rewire::tests::ordered_slide_walk_pins_frozen_path_and_is_thread_setting_independent")
                .arg("--nocapture")
                .env(CHILD, "1")
                .env("PLINTH_THREADS", threads.to_string())
                .output()
                .unwrap();
            assert!(output.status.success());
            let stdout = String::from_utf8(output.stdout).unwrap();
            let row = stdout
                .lines()
                .find_map(|line| line.strip_prefix("REWIRE_SLIDE_WALK="))
                .unwrap();
            let (observed, payload) = row.split_once(':').unwrap();
            assert_eq!(observed, threads.to_string());
            stable_payload.push(payload.to_owned());
        }
        assert!(stable_payload.windows(2).all(|pair| pair[0] == pair[1]));
        assert_eq!(
            stable_payload[0],
            "5336204845989297568:0000000000000000:0000000000000000:0000000000000000:3ff8000000000000"
        );
    }
    /// **REGRESSION — the clearance verdict for a row NOTHING WAS MEASURED ON.**
    ///
    /// `validate_changed_geometry` hands the kernel one exemption set per row:
    /// the faces a seated member's own seat forgives, so a contact-declaring arm
    /// does not refuse itself for being inside the material it is seated in.
    /// When that set covers EVERY face the distance walk can reach, the walk has
    /// no candidate and the row carries no measurement at all
    /// (`UnmeasuredSegment::AllFacesExcluded`). Through the float entry point
    /// this site used to call, that row came back as `f64::INFINITY`, and the
    /// `value >= 0.0` verdict read the absence of a measurement as the roomiest
    /// clearance in the forest. Erasure by exemption laundered into a pass is
    /// exactly what naming the exempt FACES rather than a skipped LENGTH was
    /// introduced to prevent.
    ///
    /// **The control is the whole test.** Both halves below use the same member,
    /// the same contact, the same seat face and the same frozen exemption — the
    /// zone is `[0]` in both, and neither half re-cuts it. The only difference is
    /// whether the surface carries a second face 20 mm away that the exemption
    /// does not name. With something left to grade, the move is admitted; with
    /// nothing left, it must be refused. Any refusal shared by both halves —
    /// the cone test, the PR-2 mask, PR-3 emergence — is cancelled out by
    /// construction, so what this measures is the unmeasured row and nothing
    /// else.
    #[test]
    fn a_seat_exemption_that_covers_every_face_refuses_the_move_it_cannot_grade() {
        // A seated arm: vertical, 4 mm long, rootward end on the seat face at
        // the origin. `after` widens only the TERMINAL radius, so the row is a
        // changed row while the band radius (cut from the contact end's radius)
        // and both endpoints are untouched -- which is what keeps
        // `refreeze_resized_seat_zones` from re-cutting the zone under the test.
        let p0_mm = [0.0, 0.0, 4.0];
        let p1_mm = [0.0, 0.0, 0.0];
        let seat_radius_mm = 0.1;

        let graded = |with_a_face_left_over: bool| {
            let mut vertices = vec![[-0.2, -0.2, 0.0], [0.2, -0.2, 0.0], [0.0, 0.2, 0.0]];
            let mut faces = vec![[0_u32, 1, 2]];
            if with_a_face_left_over {
                vertices.extend([[20.0, -0.2, 0.0], [20.0, 0.2, 0.0], [20.0, 0.0, 0.4]]);
                faces.push([3, 4, 5]);
            }
            let mesh = Mesh::new(vertices, faces).unwrap();
            let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
            let surface = NativeSurface::from_indexed(
                mesh.verts(),
                mesh.tris(),
                NativeSurfaceConfig {
                    grid_pitch_mm: 1.0,
                    max_grid_bytes: 8 * 1024 * 1024,
                },
            )
            .unwrap();
            let authority = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
            let config = Config::current(0.05).unwrap();

            let mut before = Forest::new();
            let key = before.add_member(Member::new(MemberSpec {
                p0_mm: p0_mm.map(f),
                p1_mm: p1_mm.map(f),
                r0_mm: TerminalEndRadius::new(p(seat_radius_mm)),
                r1_mm: RootwardEndRadius::new(p(seat_radius_mm)),
                kind: MemberKind::Trunk,
                owner: Owner::Generated {
                    family: "all-faces-excluded".into(),
                    ordinal: 0,
                },
                model_contact_end: Some(crate::forest::ContactEnd::Rootward),
                carried_mm2: f(1.0),
            }));
            let record = crate::seat_zone::freeze_production_seat_zones(
                &surface,
                &topology,
                &[crate::seat_zone::SeatZoneRequest {
                    p0_mm,
                    p1_mm,
                    seat_face_id: 0,
                    band_radius_mm: seat_radius_mm + config.standoff_mm,
                }],
            )
            .unwrap()
            .remove(0);
            let zone = record.zone().as_ref().to_vec();
            before.set_seat_zone(key, record);

            let mut after = before.clone();
            after
                .set_radii(
                    key,
                    TerminalEndRadius::new(p(0.5)),
                    RootwardEndRadius::new(p(seat_radius_mm)),
                )
                .unwrap();

            let mut refrozen = 0_u64;
            let admitted =
                validate_changed_geometry(&before, &mut after, &authority, &config, &mut refrozen)
                    .unwrap();
            (admitted, zone, refrozen, topology.n_faces())
        };

        let (admitted_with_a_face_left, zone_left, refrozen_left, faces_left) = graded(true);
        let (admitted_with_nothing_left, zone_none, refrozen_none, faces_none) = graded(false);

        // Same exemption on both sides, and neither side re-cut it: the halves
        // differ only in what the exemption leaves behind.
        assert_eq!(zone_left, vec![0_u32], "the arm forgives its own seat face");
        assert_eq!(zone_none, vec![0_u32], "and exactly the same one here");
        assert_eq!((refrozen_left, refrozen_none), (0, 0));
        assert_eq!(faces_left, 2, "one face is left outside the exemption");
        assert_eq!(
            faces_none, 1,
            "the exemption covers every face the walk can reach"
        );

        assert!(
            admitted_with_a_face_left,
            "CONTROL: with a face left to measure against, this move is clean and \
             must be admitted -- so nothing else in this pass refuses it"
        );
        assert!(
            !admitted_with_nothing_left,
            "the exemption removed every face the distance walk could reach, so \
             NOTHING was measured. That row is not a large clearance: reading it \
             as one admits a support on a check that never ran"
        );
    }

    /// **REGRESSION — a degenerate retirement is a REPAIR, and a repair that
    /// grew the forest reports the increase.**
    ///
    /// `try_retire` used to close with `Ok(Some((before - after).max(0.0)))`.
    /// The clamp is not cosmetic: the sweep subtracts what it is told from its
    /// running volume, so a repair that cost volume was booked as a saving of
    /// exactly zero and left the disclosed `curve` one repair further from the
    /// `final_volume_ml` recomputed at the end. Twenty of them in a row also
    /// filled the rolling window with savings that had never happened and
    /// satisfied `RollingSavingsConverged`, stopping the sweep with the rest of
    /// the forest unswept.
    ///
    /// The measured fact this pins: retiring a degenerate junction re-anchors
    /// its children onto the retiring member's own rootward end and widens the
    /// ancestors they now feed, and on this fixture that costs about 78% of the
    /// forest's volume. A volume test would therefore DECLINE this repair, which
    /// is why `try_retire` commits on legality instead and hands the sweep a
    /// `Repaired` outcome rather than a saving.
    #[test]
    fn a_degenerate_retirement_that_costs_volume_reports_the_cost_not_a_zero() {
        let cfg = Config::current(0.05).unwrap();
        let mut forest = Forest::new();
        let parent = forest.add_member(member(
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            MemberKind::Trunk,
            0,
        ));
        let deg = forest.add_member(member(
            [0.0, 0.0, 2.0],
            [0.0, 0.0, 2.0 + 5e-5],
            MemberKind::Shoot,
            1,
        ));
        let a = forest.add_member(member(
            [1.0, 0.0, 3.0],
            [0.0, 0.0, 2.0],
            MemberKind::Shoot,
            2,
        ));
        let b = forest.add_member(member(
            [-1.0, 0.0, 3.0],
            [0.0, 0.0, 2.0],
            MemberKind::Shoot,
            3,
        ));
        forest.set_parent(deg, Some(parent)).unwrap();
        forest.set_parent(a, Some(deg)).unwrap();
        forest.set_parent(b, Some(deg)).unwrap();

        let before_ml = total_volume_ml(&forest).unwrap();
        let mut candidate = forest.clone();
        let outcome = with_far_authority(|authority| {
            try_retire(
                &mut candidate,
                deg,
                authority,
                &BTreeSet::new(),
                &cfg,
                &mut 0,
            )
            .unwrap()
        });
        let after_ml = total_volume_ml(&candidate).unwrap();

        assert!(
            candidate.member(deg).is_none(),
            "the repair commits: a degenerate member must not survive into lowering"
        );
        assert!(
            after_ml > before_ml,
            "fixture control: this repair really does cost volume \
             ({before_ml} ml -> {after_ml} ml)"
        );
        let delta_ml = outcome
            .volume_delta_ml()
            .expect("a committed move books a delta");
        assert!(
            delta_ml < 0.0,
            "the sweep must be told the forest GREW, not that it saved zero: \
             {delta_ml} ml"
        );
        assert!(
            (delta_ml - (before_ml - after_ml)).abs() <= f64::EPSILON,
            "and the number it is told must be the real one"
        );
        assert_eq!(
            outcome.saving_ml(),
            None,
            "a repair is not a saving, and the rolling-convergence window must \
             not be able to read it as one"
        );
    }

    /// **REGRESSION — the disclosed curve reconciles against the volume
    /// recomputed once the sweep ends.**
    ///
    /// `curve` is the running volume after each accepted move and
    /// `final_volume_ml` is recomputed from the forest itself, so the last point
    /// on the curve and the final volume are two routes to one number. They stop
    /// agreeing the moment an accepted move reports a delta that is not the one
    /// it caused -- which is what the retirement clamp did, once per repair.
    #[test]
    fn the_disclosed_curve_reconciles_against_the_recomputed_final_volume() {
        with_far_authority(|authority| {
            let mut forest = Forest::new();
            let parent = forest.add_member(member(
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 0.0],
                MemberKind::Trunk,
                0,
            ));
            let deg = forest.add_member(member(
                [0.0, 0.0, 2.0],
                [0.0, 0.0, 2.0 + 5e-5],
                MemberKind::Shoot,
                1,
            ));
            let a = forest.add_member(member(
                [1.0, 0.0, 3.0],
                [0.0, 0.0, 2.0],
                MemberKind::Shoot,
                2,
            ));
            let b = forest.add_member(member(
                [-1.0, 0.0, 3.0],
                [0.0, 0.0, 2.0],
                MemberKind::Shoot,
                3,
            ));
            forest.set_parent(deg, Some(parent)).unwrap();
            forest.set_parent(a, Some(deg)).unwrap();
            forest.set_parent(b, Some(deg)).unwrap();

            let report = optimize(&mut forest, rewire_request(authority)).unwrap();
            assert!(
                forest.member(deg).is_none(),
                "fixture control: the sweep really does take the degenerate repair"
            );
            assert!(
                report.final_volume_ml > report.initial_volume_ml,
                "fixture control: and that repair costs volume ({} ml -> {} ml)",
                report.initial_volume_ml,
                report.final_volume_ml
            );
            let (_, last_ml) = *report
                .curve
                .last()
                .expect("the curve is seeded with the initial volume");
            assert!(
                (last_ml - report.final_volume_ml).abs() <= 1e-12,
                "the disclosed curve ends at {last_ml} ml but the forest \
                 recomputes to {} ml",
                report.final_volume_ml
            );
        });
    }

    /// **REGRESSION — the registry may MOVE this stage's search parameters, and
    /// is refused only where a value leaves the sweep with no definable
    /// behaviour.**
    ///
    /// The gate this replaces read five registered values and then compared each
    /// against a hard-coded literal in this file, so the registry was the source
    /// of the numbers and a literal here was their authority: every value but
    /// the one already written down was refused. Retuning the registry could
    /// break the stage and never move it.
    #[test]
    fn the_constant_gate_admits_a_retuned_registry_and_refuses_only_an_unusable_one() {
        let registered = Config::current(0.05).unwrap();
        registered
            .check_usable()
            .expect("the registry's own values are usable");

        // A registry that MOVED every one of these is a legal registry. Under
        // the literal gate each line below was a `ConstantMismatch`.
        let retuned = Config {
            twig_mm: 0.7,
            degenerate_mm: 5e-4,
            max_slide_evals: 80,
            roll_window: 12,
            roll_eps: 0.01,
            ..registered.clone()
        };
        retuned
            .check_usable()
            .expect("a retuned registry must be usable, not refused for differing");

        for (broken, name) in [
            (
                Config {
                    twig_mm: 0.0,
                    ..registered.clone()
                },
                "a twig length of zero puts no member on either side of it",
            ),
            (
                Config {
                    degenerate_mm: f64::NAN,
                    ..registered.clone()
                },
                "a non-finite degenerate length retires nothing, ever",
            ),
            (
                Config {
                    standoff_mm: -1.0,
                    ..registered.clone()
                },
                "a negative standoff is not a gap",
            ),
            (
                Config {
                    roll_eps: -1.0,
                    ..registered.clone()
                },
                "a negative convergence fraction can never be met",
            ),
            (
                Config {
                    max_slide_evals: 0,
                    ..registered.clone()
                },
                "a slide budget of zero deletes the move rather than bounding it",
            ),
            (
                Config {
                    roll_window: 0,
                    ..registered.clone()
                },
                "a window of zero is full before the first move",
            ),
        ] {
            assert!(
                matches!(
                    broken.check_usable(),
                    Err(RewireError::ConstantUnusable { .. })
                ),
                "{name}"
            );
        }
    }

    /// **REGRESSION — a brace endpoint with no live host is STALE, not
    /// infinitely far away.**
    ///
    /// The scalar fold this check replaced seeded its minimum with
    /// `f64::INFINITY`, so a forest with no live hosts reported every brace as
    /// stale through a sentinel that happened to point the right way. The batch
    /// answers `None` for a point with no nearest segment, and `None` is not a
    /// large distance: reading it as one would pass every brace in a forest that
    /// has nothing left to hold them.
    ///
    /// The fold also carried a private copy of the kernel's own
    /// point-to-segment degeneracy epsilon into this file. Two copies of one
    /// predicate drift, and the copy in the caller is the one nobody tests.
    #[test]
    fn a_brace_endpoint_with_no_live_host_is_stale_rather_than_infinitely_far() {
        let mut forest = Forest::new();
        let host = forest.add_member(member(
            [0.0, 0.0, 2.0],
            [0.0, 0.0, 0.0],
            MemberKind::Trunk,
            0,
        ));
        let brace = forest.add_brace(member(
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.5],
            MemberKind::Brace,
            1,
        ));
        assert!(
            assert_no_stale_braces(&forest).is_ok(),
            "both ends sit on the host axis"
        );

        // The same brace, moved 10 mm off every host it could attach to.
        forest
            .set_endpoints(brace, [10.0, 0.0, 1.0].map(f), [10.0, 0.0, 1.5].map(f))
            .unwrap();
        assert!(matches!(
            assert_no_stale_braces(&forest),
            Err(RewireError::StaleBrace)
        ));

        // And the case the sentinel used to answer: nothing to attach TO.
        forest.retire(host).unwrap();
        assert!(
            forest.live_tree_members().is_empty(),
            "fixture control: the brace is the only member left"
        );
        assert!(
            matches!(
                assert_no_stale_braces(&forest),
                Err(RewireError::StaleBrace)
            ),
            "an unanswerable question is not a comfortable pass"
        );
    }
}
