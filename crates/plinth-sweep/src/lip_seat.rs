//! **A seated arm meets its seat face within the angle at which the seat trim
//! keeps the arm's own section** (fix/lip-seat, 2026-09-29).
//!
//! # The defect
//!
//! The seat trim keeps only the support side of a tip's seat plane
//! (`plinth_support::SeatTrim`, fix/tip-trim). The plane is the face's own,
//! and it is infinite. Inside the tip ball that removes the ball's cap region
//! and nothing the ball does not already cover. But an arm that meets its face
//! nearly side-on runs its SHAFT along the plane, and the trim removes the
//! arm's own section there, outside the ball, even where no model stands.
//! On Round_25 at its committed flip (owner, ChituBox, 2026-09-29, for-craig-v3)
//! a few fat side-branches seat on the drafted inner wall of the ring lip (a
//! face 75° from horizontal) and lean ~57° along it to merge with a neighbour:
//! the trim removed up to 70 % of such a shaft (arm at record 147 in
//! `for-craig-v4/Round_25`: 1.05 mm³ of shaft outside its ball, in air).
//! "Probably working OK, but looks a little off because a very fat
//! side-branch is then almost completely shaved off." The resin is drawn and
//! then trimmed, and the arm that prints is a sliver.
//!
//! # The rule
//!
//! **A seated arm must meet its seat face within the angle at which the seat
//! trim keeps the arm's own section**: the trim may remove only the tip
//! ball's cap region, never the shaft outside the ball. Measured on the arm as
//! the lowering draws it ([`plinth_support::tip_shaft_seat_bites`]): the
//! deepest the seat plane reaches into the shaft section at any station whose
//! ring is not wholly inside the ball. The limit angle is DERIVED, not typed
//! (`ShaftSeatBite::limit_deg`): from the taper's radius `r`, the ball's
//! radius `R` and the seat law's own depth `δ(φ)`, it is the root of
//! `(R cos φ − δ) + √(R² − r²) cos φ − r sin φ = 0` — the ball centre's height
//! over the plane, plus the rise along the axis to where the shaft leaves the
//! ball, minus the tilted ring's drop to the plane. Contact area stays the only
//! seat criterion: the seat law, the ball and the trim are unchanged.
//!
//! # What placement does about a seat that breaks it
//!
//! In the brief's order:
//!
//! 1. **A smaller lean to the same seat.** The seat stays; the arm is re-landed
//!    (its base moves) so its axis meets the face within the limit. The
//!    candidates are the descents the up-sweep itself would consider — a
//!    landing on another arm's shaft or on the plate, within the lean law, on
//!    a straight segment that clears model material, every other member and
//!    the drain keep-out (`slenderness::segment_path`, the test remedy-1
//!    merges and head joins use) — admitted only where the approach angle is
//!    within the arm's own derived limit, cheapest by the lean law's own price
//!    first, and each one VERIFIED by the lowering's own measurement before it
//!    is taken. An arm something else rests on is not moved (moving it would
//!    move the ground under its dependants).
//! 2. **A seat further in on the same face.** The footprint rule
//!    (`footprint.rs`) already slides a seat along its face; a tip whose shaft
//!    the trim still bites joins its search, and a slide is taken only where
//!    the moved arm keeps its section too (and its disc stays on the face).
//! 3. **Otherwise the seat is kept and a WARNING names the arm**, its approach
//!    angle and its limit. Emit and warn, never refuse.
//!
//! No new constant. The limit is the lowering's geometry; the lean law, the
//! path refusals and the plate rule are the ones the other re-landing passes
//! use.
//!
//! # Batching
//!
//! One survey per pass: every seat face in one closest-face query
//! (`tip_landing::tip_seat_faces`), every arm's bite in one lowering call.
//! Candidates are priced by arithmetic; the lowering verifies one candidate
//! per pending arm per round, all pending arms in one call.

use geom_kernel::{NativeSurface, SegmentClearanceClassified};
use plinth_core::LayerPlan;
use plinth_mesh::Mesh;
use plinth_support::{ShaftSeatBite, SweepArm};

use crate::head_join::HeadJoinContext;
use crate::slenderness::SlendernessContext;
use crate::up_sweep::ColumnHistory;
use crate::{Landing, StrutKind, SweepStrut};

/// Float noise on "the plane does not reach the shaft": the lowering's own
/// arithmetic on an arm drawn exactly at the limit, and nothing more.
const BITE_SLACK_MM: f64 = 1.0e-9;

/// Float noise on "no nearer the model than it was": the kernel's distance
/// on the same segment read twice, and nothing more.
const CLEARANCE_SLACK_MM: f64 = 1.0e-9;

/// The kernel's solid clearance from the model of each member's drawn
/// segment, from its drawn seat to its base at its widest drawn radius, with
/// its own seat zone untested — its tip ball, or where further, the point
/// where its solid first stands `tier` clear of its own seat plane (the
/// head-join rule's test 2, `head_join::own_seat_zone_mm`). `None` where the
/// kernel did not decide or nothing is left to test.
fn solid_clearances(
    surface: &NativeSurface,
    arms: &[SweepArm],
    layer_plan: &LayerPlan,
    tier: f64,
) -> Vec<Option<f64>> {
    let discs = plinth_support::tip_contact_discs(arms, layer_plan)
        .unwrap_or_else(|_| vec![None; arms.len()]);
    let n = arms.len();
    let seats: Vec<[f64; 3]> = arms.iter().map(|arm| arm.seat_mm).collect();
    let bases: Vec<[f64; 3]> = arms.iter().map(|arm| arm.base_mm).collect();
    let radii: Vec<f64> = arms
        .iter()
        .map(|arm| {
            arm.radius_profile_mm
                .iter()
                .copied()
                .fold(arm.tip_radius_mm, f64::max)
        })
        .collect();
    let insets: Vec<f64> = arms
        .iter()
        .zip(&discs)
        .zip(&radii)
        .map(|((arm, disc), &radius)| {
            let Some(disc) = disc else {
                return f64::INFINITY;
            };
            let Some(axis) =
                geom_kernel::vec3::normalize(geom_kernel::vec3::sub(arm.base_mm, arm.seat_mm))
            else {
                return f64::INFINITY;
            };
            let reach =
                geom_kernel::vec3::norm(geom_kernel::vec3::sub(disc.ball_centre_mm, arm.seat_mm))
                    + disc.ball_radius_mm;
            let along = geom_kernel::vec3::dot(disc.outward_normal, axis);
            if along <= 0.0 {
                return f64::INFINITY;
            }
            reach.max((tier + radius) / along)
        })
        .collect();
    let zero = vec![0.0; n];
    surface
        .segment_clearance_classified(&seats, &bases, &radii, &radii, 0.0, &insets, &zero)
        .map_or_else(
            |_| vec![None; n],
            |rows| {
                rows.into_iter()
                    .map(SegmentClearanceClassified::clearance_mm)
                    .collect()
            },
        )
}

/// One arm the finished field still has with its shaft cut by its seat trim.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LipSeatWarning {
    /// The arm's id.
    pub arm: u32,
    /// The angle between its axis and its seat face's outward normal.
    pub approach_deg: f64,
    /// Its derived limit angle.
    pub limit_deg: f64,
    /// How deep the seat plane reaches into its shaft outside the ball, mm.
    pub bite_mm: f64,
    /// Whether this rule re-landed it (and a later pass cut it again).
    pub relanded: bool,
    /// Whether another member rests on it or is tied to it, which is what
    /// kept remedy 1 from moving it.
    pub carries_someone: bool,
    /// Whether it stands on the plate (its shaft is drawn with horizontal
    /// rings down to its boss).
    pub on_plate: bool,
}

/// What the rule did to one field.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LipSeatOutcome {
    /// Whether remedy 1 ran (the survey and the census always do).
    pub enabled: bool,
    /// Seated arms measured by the first survey.
    pub arms_graded: usize,
    /// Of those, arms whose shaft the seat trim cut outside the ball.
    pub arms_over_limit_before: usize,
    /// The deepest such cut, mm, and the widest approach angle among them.
    pub max_bite_before_mm: f64,
    pub max_approach_before_deg: f64,
    /// Remedy 1: arms re-landed so they meet their face within the limit.
    pub arms_relanded: usize,
    /// Their ids, in the order they were moved.
    pub relanded_ids: Vec<u32>,
    /// Members that rest on a re-landed arm and were re-landed with it, on
    /// its moved axis at their own landing layer.
    pub children_moved_with_host: usize,
    /// Candidate descents priced, refused by the path test (model, another
    /// member, the drain keep-out), refused by the plate rule, refused by the
    /// lowering's own measurement (the arithmetic admitted them), and verified.
    pub candidates_priced: usize,
    pub candidates_refused_path: usize,
    pub candidates_refused_plate: usize,
    pub candidates_refused_measured: usize,
    /// Refused because a moved member's drawn solid would come nearer the
    /// model than the final gap, or than it already stood.
    pub candidates_refused_clearance: usize,
    /// Whether the model surface could not be built, so the solid clearance
    /// went unmeasured (a WARNING; the rule still ran on the centreline test).
    pub clearance_unmeasured: bool,
    pub candidates_verified: usize,
    /// Arms over the limit that remedy 1 left alone: something rests on them,
    /// or no candidate held.
    pub kept_carrying: usize,
    pub kept_no_candidate: usize,
    /// Passes remedy 1 ran (each followed by the down-sweep).
    pub passes: usize,
    /// The EMITTED field's census (after the footprint rule): every seated arm
    /// graded, and those whose shaft the trim still cuts, warned.
    pub arms_graded_after: usize,
    pub over_limit_after: Vec<LipSeatWarning>,
}

/// Every seated arm of `struts`, as the lowering will draw it on the live
/// field (seat faces measured now, profiles drawn now), with its bite.
/// `None` where the arm does not seat into the model.
pub(crate) fn survey(
    mesh: &Mesh,
    context: &SlendernessContext<'_>,
    layer_plan: &LayerPlan,
    plate_z_mm: f64,
    struts: &[SweepStrut],
) -> (Vec<SweepArm>, Vec<Option<ShaftSeatBite>>) {
    let arms = live_arms(mesh, context, plate_z_mm, struts);
    let bites = plinth_support::tip_shaft_seat_bites(&arms, layer_plan)
        .unwrap_or_else(|_| vec![None; arms.len()]);
    (arms, bites)
}

/// One admissible descent for an arm over its limit: priced, not yet
/// path-tested or measured.
#[derive(Clone, Copy, Debug)]
struct Candidate {
    cost_mm: f64,
    cell: usize,
    landing: Landing,
    k_bottom: usize,
}

/// One arm remedy 1 is working on, with the members that rest on it (each a
/// support arm nothing else rests on), which move with it.
struct Pending {
    index: usize,
    candidates: Vec<Candidate>,
    next: usize,
    children: Vec<usize>,
}

/// A child following its moved host: `(child index, the cell it now lands
/// in, its new path)`.
type Follow = (usize, usize, Vec<u32>);

/// One round's proposal for one pending arm: the candidate, the arm's new
/// path, and each child's new path onto the moved axis.
struct Proposal {
    slot: usize,
    candidate: Candidate,
    path: Vec<u32>,
    children: Vec<Follow>,
}

/// Where each child of a mover lands on its new axis, and the child's new
/// path there, or `None` when one of them cannot follow: its landing layer is
/// no longer on the moved shaft, its straight segment is blocked, or the lean
/// law cannot close the new gap.
fn children_follow(
    context: &SlendernessContext<'_>,
    index: &ColumnHistory,
    struts: &[SweepStrut],
    mover: usize,
    k_bottom: usize,
    path: &[u32],
    children: &[usize],
) -> Option<Vec<Follow>> {
    let host = &struts[mover];
    let mut out = Vec::with_capacity(children.len());
    for &child in children {
        let strut = &struts[child];
        let Landing::Strut { layer, .. } = strut.landing else {
            return None;
        };
        if layer < k_bottom || layer > host.k_top {
            return None;
        }
        let cell = path
            .get(layer - k_bottom)
            .map_or(host.cell, |cell| *cell as usize);
        let lateral_mm = crate::cell_distance_mm(context.grid, strut.cell, cell);
        context
            .lean
            .descent_cost_mm(strut.k_top.checked_sub(layer)?, lateral_mm)?;
        // The child may pass through the host it lands on and through its own
        // old shaft, which the index still holds.
        let new_path = crate::slenderness::segment_path_excluding(
            context,
            index,
            strut.cell,
            strut.k_top,
            cell,
            layer,
            &[host.id, strut.id],
        )?;
        out.push((child, cell, new_path));
    }
    Some(out)
}

/// A member drawn as the lowering will draw it, from a strut clone with its
/// landing, base layer and path replaced and the survey's measured face.
#[expect(
    clippy::too_many_arguments,
    reason = "the field's frame, the live struts and arms, and the one member's new descent"
)]
fn trial_arm(
    context: &SlendernessContext<'_>,
    plate_z_mm: f64,
    struts: &[SweepStrut],
    arms: &[SweepArm],
    index_of: usize,
    landing: Landing,
    k_bottom: usize,
    path: &[u32],
) -> SweepArm {
    let arm = &arms[index_of];
    let mut strut = struts[index_of].clone();
    strut.landing = landing;
    strut.k_bottom = k_bottom;
    strut.path = path.to_vec();
    // The face the survey measured; the live strut records its own only once
    // the field is final.
    strut.seat_normal = arm.seat_normal_mm;
    let (seat, base) =
        crate::adapter::drawn_ends(context.grid, plate_z_mm, context.layer_height_mm, &strut);
    crate::adapter::sweep_arm(
        &strut,
        seat,
        base,
        arm.seat_face_point_mm,
        top_aligned(&arm.radius_profile_mm, strut.k_top - strut.k_bottom),
    )
}

/// **Remedy 1: a smaller angle to the same seat.** Re-land every arm over its
/// limit that nothing rests on, on the cheapest descent (by the lean law's own
/// price) whose axis meets the seat face within the arm's own limit angle,
/// whose straight segment clears model, members and the drain keep-out, whose
/// base disc stays above the plate, and whose drawn geometry the lowering
/// measures as keeping the shaft's section. Returns how many arms moved; the
/// caller re-sizes the field and calls again.
#[expect(
    clippy::too_many_lines,
    reason = "one pass in three batched stages (price, path, measure); splitting it would thread \
              the same field, index and pending list through three signatures"
)]
pub(crate) fn apply(
    joins: &HeadJoinContext<'_>,
    layer_plan: &LayerPlan,
    plate_z_mm: f64,
    struts: &mut [SweepStrut],
    arms: &[SweepArm],
    bites: &[Option<ShaftSeatBite>],
    outcome: &mut LipSeatOutcome,
) -> usize {
    let context = joins.slender;
    let surface = joins.surface();
    let tier = joins.clearance_mm;
    if surface.is_none() {
        outcome.clearance_unmeasured = true;
    }
    let mut index = ColumnHistory::new(context.grid);
    crate::congestion::stamp_all(&mut index, struts);
    outcome.kept_carrying = 0;
    outcome.kept_no_candidate = 0;

    // ---- who may move. An arm another member rests on moves only WITH its
    // children, and only when each child is a support arm nothing rests on and
    // no cross is tied to the arm (the follow is then one re-landing per child
    // on the moved axis, at the child's own landing layer). Anything else is
    // locked for the pass: moving it would move the ground under members this
    // pass cannot re-land.
    let carries = crate::landing_check::carries_someone(struts);
    let mut children_of: Vec<Vec<usize>> = vec![Vec::new(); struts.len()];
    let mut tied = vec![false; struts.len()];
    for (child, strut) in struts.iter().enumerate() {
        if let Landing::Strut { id, .. } = strut.landing {
            if let Some(list) = children_of.get_mut(id as usize) {
                list.push(child);
            }
        }
        if let Some(host) = strut.seat_on_strut {
            if let Some(flag) = tied.get_mut(host as usize) {
                *flag = true;
            }
        }
    }
    let mut locked: Vec<bool> = (0..struts.len())
        .map(|member| {
            carries[member]
                && (tied[member]
                    || children_of[member]
                        .iter()
                        .any(|&child| carries[child] || struts[child].kind != StrutKind::Arm))
        })
        .collect();
    let to_move: Vec<bool> = struts
        .iter()
        .zip(bites)
        .enumerate()
        .map(|(index_of, (strut, bite))| {
            bite.as_ref().is_some_and(over_limit)
                && strut.kind == StrutKind::Arm
                && strut.k_bottom != usize::MAX
                && !locked[index_of]
        })
        .collect();

    // ---- stage 1: price every candidate descent of every arm over its limit.
    let mut pending: Vec<Pending> = Vec::new();
    for (index_of, (strut, bite)) in struts.iter().zip(bites).enumerate() {
        let Some(bite) = bite else { continue };
        if !over_limit(bite) || strut.kind != StrutKind::Arm || strut.k_bottom == usize::MAX {
            continue;
        }
        // A child that is itself over its limit is re-landed first; the
        // parent is taken on the next pass, with whatever still rests on it.
        let children = children_of[index_of].clone();
        if locked[index_of] || children.iter().any(|&child| to_move[child]) {
            outcome.kept_carrying += 1;
            continue;
        }
        let Some(arm) = arms.get(index_of) else {
            continue;
        };
        let Some(normal) = geom_kernel::vec3::normalize(arm.seat_normal_mm) else {
            outcome.kept_no_candidate += 1;
            continue;
        };
        let candidates = price(
            context,
            &index,
            struts,
            index_of,
            arm.seat_mm,
            normal,
            bite.limit_deg,
            &to_move,
            &children,
            outcome,
        );
        pending.push(Pending {
            index: index_of,
            candidates,
            next: 0,
            children,
        });
    }

    // ---- stages 2 and 3, in rounds: each pending arm's next candidate whose
    // path clears, then all of them measured by the lowering in one call.
    let mut moved = 0;
    let mut open: Vec<usize> = (0..pending.len()).collect();
    let mut done = vec![false; pending.len()];
    while !open.is_empty() {
        let mut proposals: Vec<Proposal> = Vec::new();
        for &slot in &open {
            let job = &mut pending[slot];
            if locked[job.index] {
                // Something landed on it in an earlier round.
                continue;
            }
            while job.next < job.candidates.len() {
                let candidate = job.candidates[job.next];
                job.next += 1;
                match propose(context, &index, struts, job.index, candidate, &job.children) {
                    Some((path, children)) => {
                        proposals.push(Proposal {
                            slot,
                            candidate,
                            path,
                            children,
                        });
                        break;
                    }
                    None => outcome.candidates_refused_path += 1,
                }
            }
        }
        if proposals.is_empty() {
            break;
        }
        // Every proposal drawn as the lowering will draw it: the arm with its
        // new base, and each child with its new path onto the moved axis. One
        // measurement call for the round.
        let mut trial: Vec<SweepArm> = Vec::new();
        let mut first_of: Vec<usize> = Vec::with_capacity(proposals.len());
        for proposal in &proposals {
            first_of.push(trial.len());
            let index_of = pending[proposal.slot].index;
            trial.push(trial_arm(
                context,
                plate_z_mm,
                struts,
                arms,
                index_of,
                proposal.candidate.landing,
                proposal.candidate.k_bottom,
                &proposal.path,
            ));
            for (child, _, path) in &proposal.children {
                let strut = &struts[*child];
                trial.push(trial_arm(
                    context,
                    plate_z_mm,
                    struts,
                    arms,
                    *child,
                    strut.landing,
                    strut.k_bottom,
                    path,
                ));
            }
        }
        let measured = plinth_support::tip_shaft_seat_bites(&trial, layer_plan)
            .unwrap_or_else(|_| vec![None; trial.len()]);
        // THE SOLID, not only the centreline: each moved member's drawn
        // segment against the model, as the head-join rule measures a moved
        // lander (the kernel's clearance, its own seat zone untested). A move
        // may not bring it nearer the model than the final gap, or than it
        // already was. One batch for the round, and one for the same members
        // as they stand.
        let (after, before) = match surface {
            Some(surface) => {
                let movers: Vec<usize> = proposals
                    .iter()
                    .flat_map(|proposal| {
                        std::iter::once(pending[proposal.slot].index)
                            .chain(proposal.children.iter().map(|(child, _, _)| *child))
                    })
                    .collect();
                let standing: Vec<SweepArm> =
                    movers.iter().map(|&member| arms[member].clone()).collect();
                (
                    solid_clearances(surface, &trial, layer_plan, tier),
                    solid_clearances(surface, &standing, layer_plan, tier),
                )
            }
            None => (vec![None; trial.len()], vec![None; trial.len()]),
        };
        for (proposal, &first) in proposals.into_iter().zip(&first_of) {
            let index_of = pending[proposal.slot].index;
            let clear = surface.is_none()
                || (first..=first + proposal.children.len()).all(|slot| {
                    match (after[slot], before[slot]) {
                        (Some(now), was) => {
                            now >= tier.min(was.unwrap_or(tier)) - CLEARANCE_SLACK_MM
                        }
                        (None, _) => false,
                    }
                });
            if !clear {
                outcome.candidates_refused_clearance += 1;
                continue;
            }
            // The mover keeps its section; each child is no worse than it was.
            let mover_keeps = measured[first]
                .as_ref()
                .is_some_and(|bite| !over_limit(bite));
            let children_hold =
                proposal
                    .children
                    .iter()
                    .enumerate()
                    .all(|(offset, (child, _, _))| {
                        let before = bites
                            .get(*child)
                            .copied()
                            .flatten()
                            .map_or(0.0, |bite| bite.bite_mm);
                        measured[first + 1 + offset]
                            .as_ref()
                            .is_some_and(|bite| bite.bite_mm <= before.max(0.0) + BITE_SLACK_MM)
                    });
            let above_plate = (0..=proposal.children.len()).all(|offset| {
                let arm = &trial[first + offset];
                let lands_on_a_member = if offset == 0 {
                    matches!(proposal.candidate.landing, Landing::Strut { .. })
                } else {
                    true
                };
                !lands_on_a_member || {
                    let root = arm
                        .radius_profile_mm
                        .first()
                        .copied()
                        .unwrap_or(arm.tip_radius_mm);
                    crate::head_join::stays_above_plate(arm.seat_mm, arm.base_mm, root, plate_z_mm)
                }
            });
            if !above_plate {
                outcome.candidates_refused_plate += 1;
                continue;
            }
            if !mover_keeps || !children_hold {
                outcome.candidates_refused_measured += 1;
                continue;
            }
            if locked[index_of] {
                continue;
            }
            // The index and the field know the moves already taken this
            // round: a parent re-landed from under the proposal, or a path
            // that now crosses a member moved this round, sends the arm back
            // for its next candidate.
            let job = &pending[proposal.slot];
            let again = propose(
                context,
                &index,
                struts,
                index_of,
                proposal.candidate,
                &job.children,
            );
            let Some((path, children)) = again.filter(|(path, children)| {
                *path == proposal.path && *children == proposal.children
            }) else {
                outcome.candidates_refused_path += 1;
                continue;
            };
            outcome.candidates_verified += 1;
            let strut = &mut struts[index_of];
            strut.landing = proposal.candidate.landing;
            strut.k_bottom = proposal.candidate.k_bottom;
            strut.path = path;
            let id = strut.id;
            index.stamp_shaft(id, strut.k_bottom, strut.k_top, strut.cell, &strut.path);
            for (child, _, child_path) in children {
                let strut = &mut struts[child];
                strut.path = child_path;
                index.stamp_shaft(
                    strut.id,
                    strut.k_bottom,
                    strut.k_top,
                    strut.cell,
                    &strut.path,
                );
                locked[child] = true;
                outcome.children_moved_with_host += 1;
            }
            if let Landing::Strut { id: host, .. } = proposal.candidate.landing {
                if let Some(flag) = locked.get_mut(host as usize) {
                    *flag = true;
                }
            }
            outcome.relanded_ids.push(id);
            outcome.arms_relanded += 1;
            moved += 1;
            done[proposal.slot] = true;
        }
        open.retain(|slot| !done[*slot] && pending[*slot].next < pending[*slot].candidates.len());
    }
    for (slot, job) in pending.iter().enumerate() {
        if done[slot] {
            continue;
        }
        if locked[job.index] {
            outcome.kept_carrying += 1;
        } else {
            outcome.kept_no_candidate += 1;
        }
    }
    moved
}

/// One candidate's new path for the mover and for each of its children, or
/// `None` when the parent no longer stands where the candidate found it or a
/// path is blocked.
fn propose(
    context: &SlendernessContext<'_>,
    index: &ColumnHistory,
    struts: &[SweepStrut],
    mover: usize,
    candidate: Candidate,
    children: &[usize],
) -> Option<(Vec<u32>, Vec<Follow>)> {
    // THE PARENT MUST STAND HERE NOW: another arm of this pass may have been
    // re-landed from under the index's picture of it.
    let strut = &struts[mover];
    // The path may pass through the member it lands on (the junction), its
    // own old shaft and its children's, which the index still holds.
    let mut except: Vec<u32> = Vec::with_capacity(children.len() + 2);
    except.push(strut.id);
    except.extend(children.iter().map(|&child| struts[child].id));
    if let Landing::Strut { id, layer } = candidate.landing {
        let parent = struts.get(id as usize)?;
        if !crate::landing_check::stands_at(parent, candidate.cell, layer) {
            return None;
        }
        except.push(id);
    }
    let path = crate::slenderness::segment_path_excluding(
        context,
        index,
        strut.cell,
        strut.k_top,
        candidate.cell,
        candidate.k_bottom,
        &except,
    )?;
    let followed = children_follow(
        context,
        index,
        struts,
        mover,
        candidate.k_bottom,
        &path,
        children,
    )?;
    Some((path, followed))
}

/// `profile` (index 0 at the landing, last under the seat) resampled onto
/// `len` layers, aligned at the TOP: the section under the seat is kept, and a
/// longer arm repeats its root below.
fn top_aligned(profile: &[f64], len: usize) -> Vec<f64> {
    if profile.is_empty() || len == 0 {
        return Vec::new();
    }
    let old = profile.len();
    (0..len)
        .map(|slot| {
            let from_top = len - 1 - slot;
            profile[old.saturating_sub(1 + from_top)]
        })
        .collect()
}

/// Every candidate descent of one arm over its limit, cheapest first.
///
/// For each column within the search box, the lean law gives the fewest
/// layers that close the lateral gap, and the arm's own limit angle gives
/// the fewest that bring its axis within the limit of the face normal
/// ([`min_drop_mm`]); the column is asked for the highest landing at or below
/// the deeper of the two, and the axis to it is checked against the limit
/// again (a lower landing can leave the admissible band on a steep face).
/// A column with nothing at or below that ceiling offers the plate.
#[expect(
    clippy::too_many_arguments,
    reason = "the field, its index, the arm, its seat and face, its limit, who may not be landed \
              on, and the counters"
)]
fn price(
    context: &SlendernessContext<'_>,
    index: &ColumnHistory,
    struts: &[SweepStrut],
    index_of: usize,
    seat_mm: [f64; 3],
    normal: [f64; 3],
    limit_deg: f64,
    to_move: &[bool],
    children: &[usize],
    outcome: &mut LipSeatOutcome,
) -> Vec<Candidate> {
    let grid = context.grid;
    let height = context.layer_height_mm;
    let law = context.lean;
    let strut = &struts[index_of];
    let k_top = strut.k_top;
    let mut out = Vec::new();
    if k_top == 0 || height <= 0.0 {
        return out;
    }
    let cos_limit = limit_deg.to_radians().cos();
    // The search box: no descent travels further sideways than the lean law
    // lets it over the arm's whole height, nor further than the straight
    // descent below its own seat is long (the up-sweep's own bound: a
    // re-landing is a descent the up-sweep could have chosen).
    let straight_layers = index
        .shaft_landing(strut.cell, k_top.saturating_sub(1))
        .filter(|(id, _, _)| *id != strut.id)
        .map_or(k_top, |(_, at, _)| k_top - at);
    #[expect(
        clippy::cast_precision_loss,
        reason = "layer counts bounded by the plan's own layer count are exact in f64"
    )]
    let radius_mm = (k_top as f64 * law.max_step_mm()).min(straight_layers as f64 * height);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a finite non-negative length over the grid pitch, floored to a loop bound"
    )]
    let radius_cells = (radius_mm / grid.pitch_mm()).floor().max(0.0) as usize;
    let cols = grid.cols();
    let seat_row = grid.row_of(strut.cell);
    let seat_col = grid.col_of(strut.cell);
    let row_lo = seat_row.saturating_sub(radius_cells);
    let row_hi = (seat_row + radius_cells).min(grid.rows() - 1);
    let col_lo = seat_col.saturating_sub(radius_cells);
    let col_hi = (seat_col + radius_cells).min(cols - 1);
    let approach_holds = |cell: usize, drop_mm: f64| -> bool {
        let dx = grid.center_x_mm(grid.col_of(cell)) - seat_mm[0];
        let dy = grid.center_y_mm(grid.row_of(cell)) - seat_mm[1];
        let length = (dx * dx + dy * dy + drop_mm * drop_mm).sqrt();
        length > 0.0
            && (dx * normal[0] + dy * normal[1] - drop_mm * normal[2]) / length >= cos_limit
    };
    for row in row_lo..=row_hi {
        for col in col_lo..=col_hi {
            let cell = row * cols + col;
            let dx = grid.center_x_mm(col) - seat_mm[0];
            let dy = grid.center_y_mm(row) - seat_mm[1];
            let lateral_mm = crate::cell_distance_mm(grid, strut.cell, cell);
            if lateral_mm > radius_mm {
                continue;
            }
            let law_layers = law.layers_to_reach(lateral_mm);
            if law_layers == 0 {
                continue;
            }
            let Some(angle_mm) = min_drop_mm(dx, dy, normal, cos_limit) else {
                continue;
            };
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a finite non-negative drop over the layer height, rounded up to a count"
            )]
            let angle_layers = (angle_mm / height).ceil().max(1.0) as usize;
            let drop = law_layers.max(angle_layers);
            let Some(ceiling) = k_top.checked_sub(drop) else {
                continue;
            };
            let (landing, k_bottom) = match index.shaft_landing(cell, ceiling) {
                // Not onto an arm this pass is about to re-land: landing on
                // it would freeze its own cut in place.
                // Nor onto one of its own children, which rest on it (a
                // cycle in the load path).
                Some((id, at, _))
                    if id != strut.id
                        && !to_move.get(id as usize).copied().unwrap_or(false)
                        && !children.contains(&(id as usize)) =>
                {
                    (Landing::Strut { id, layer: at }, at)
                }
                Some(_) => continue,
                None => (Landing::Plate, 0),
            };
            #[expect(
                clippy::cast_precision_loss,
                reason = "a layer count bounded by the plan's own layer count is exact in f64"
            )]
            let drop_mm = (k_top - k_bottom) as f64 * height;
            if !approach_holds(cell, drop_mm) {
                continue;
            }
            let Some(cost_mm) = law.descent_cost_mm(k_top - k_bottom, lateral_mm) else {
                continue;
            };
            outcome.candidates_priced += 1;
            out.push(Candidate {
                cost_mm,
                cell,
                landing,
                k_bottom,
            });
        }
    }
    out.sort_unstable_by(|a, b| {
        a.cost_mm
            .total_cmp(&b.cost_mm)
            .then_with(|| a.cell.cmp(&b.cell))
    });
    out
}

/// The smallest drop `u ≥ 0` (mm) at which an axis from the seat to a base
/// offset `(dx, dy)` sideways and `u` down meets the face of OUTWARD normal
/// `n` within the limit: `(dx·nx + dy·ny − u·nz) / √(dx² + dy² + u²) ≥ c`,
/// `c = cos(limit)`. With `A = dx·nx + dy·ny`, `q = −nz` (> 0 under a
/// down-facing face) and `D² = dx² + dy²`, this is `A + q·u ≥ c·√(D² + u²)`,
/// which squares to the quadratic `(q² − c²)·u² + 2Aq·u + (A² − c²D²) ≥ 0`
/// on the branch `A + q·u ≥ 0`. `None` when no drop admits it.
fn min_drop_mm(dx: f64, dy: f64, normal: [f64; 3], c: f64) -> Option<f64> {
    let a = dx * normal[0] + dy * normal[1];
    let q = -normal[2];
    let d2 = dx * dx + dy * dy;
    let holds = |u: f64| -> bool {
        let length = (d2 + u * u).sqrt();
        length > 0.0 && (a + q * u) >= c * length - 1.0e-12
    };
    if holds(0.0) && d2 > 0.0 {
        return Some(0.0);
    }
    let qa = q * q - c * c;
    let qb = 2.0 * a * q;
    let qc = a * a - c * c * d2;
    let mut roots: Vec<f64> = Vec::with_capacity(2);
    if qa.abs() < 1.0e-15 {
        if qb.abs() > 0.0 {
            roots.push(-qc / qb);
        }
    } else {
        let disc = qb * qb - 4.0 * qa * qc;
        if disc >= 0.0 {
            let root = disc.sqrt();
            roots.push((-qb - root) / (2.0 * qa));
            roots.push((-qb + root) / (2.0 * qa));
        }
    }
    roots.sort_by(f64::total_cmp);
    roots
        .into_iter()
        .filter(|u| u.is_finite() && *u >= 0.0)
        // The boundary itself, nudged into the admissible side by a
        // nanometre so the check reads it as held.
        .map(|u| u + 1.0e-9)
        .find(|u| holds(*u))
}

/// The live field as the lowering's arms, with each tip's seat face measured
/// now. Same construction as the adapter's (`adapter::sweep_arm`, drawn ends,
/// drawn profiles), before any footprint move.
fn live_arms(
    mesh: &Mesh,
    context: &SlendernessContext<'_>,
    plate_z_mm: f64,
    struts: &[SweepStrut],
) -> Vec<SweepArm> {
    let grid = context.grid;
    let height = context.layer_height_mm;
    let faces = crate::tip_landing::tip_seat_faces(mesh, grid, struts, plate_z_mm, height);
    let (mut profiles, _) = crate::adapter::drawn_profiles(struts);
    struts
        .iter()
        .zip(&faces)
        .zip(profiles.iter_mut())
        .map(|((strut, face), profile)| {
            let mut tip = strut.clone();
            tip.seat_normal = face.map_or([0.0; 3], |(normal, _)| normal);
            let (seat, base) = crate::adapter::drawn_ends(grid, plate_z_mm, height, &tip);
            crate::adapter::sweep_arm(
                &tip,
                seat,
                base,
                face.map(|(_, point)| point),
                std::mem::take(profile),
            )
        })
        .collect()
}

/// Whether a measured arm's shaft is cut by its seat trim outside the ball.
pub(crate) fn over_limit(bite: &ShaftSeatBite) -> bool {
    bite.bite_mm > BITE_SLACK_MM
}

/// [`over_limit`] for an arm that may not have been measured (`None`: it
/// seats into nothing, and nothing of it is cut).
pub(crate) fn cuts(bite: Option<&ShaftSeatBite>) -> bool {
    bite.is_some_and(over_limit)
}

/// Whether a move takes a shaft cut `before` mm deep to one cut deeper.
pub(crate) fn cuts_deeper(before_mm: f64, after_mm: f64) -> bool {
    after_mm > before_mm.max(0.0) + BITE_SLACK_MM
}

/// The emitted field's census, on the arms exactly as they go to the lowering
/// (`adapter::arms_of`, footprint moves included): `(graded, warnings)`.
pub(crate) fn census(
    struts: &[SweepStrut],
    arms: &[SweepArm],
    layer_plan: &LayerPlan,
    relanded: &[u32],
) -> (usize, Vec<LipSeatWarning>) {
    let Ok(bites) = plinth_support::tip_shaft_seat_bites(arms, layer_plan) else {
        return (0, Vec::new());
    };
    let carries = crate::landing_check::carries_someone(struts);
    let mut graded = 0;
    let mut over = Vec::new();
    for (index_of, (strut, bite)) in struts.iter().zip(&bites).enumerate() {
        let Some(bite) = bite else { continue };
        graded += 1;
        if over_limit(bite) {
            over.push(LipSeatWarning {
                arm: strut.id,
                approach_deg: bite.approach_deg,
                limit_deg: bite.limit_deg,
                bite_mm: bite.bite_mm,
                relanded: relanded.contains(&strut.id),
                carries_someone: carries.get(index_of).copied().unwrap_or(false),
                on_plate: matches!(strut.landing, Landing::Plate),
            });
        }
    }
    (graded, over)
}

/// Record the first survey's numbers in `outcome`.
pub(crate) fn record_before(outcome: &mut LipSeatOutcome, bites: &[Option<ShaftSeatBite>]) {
    for bite in bites.iter().flatten() {
        outcome.arms_graded += 1;
        if over_limit(bite) {
            outcome.arms_over_limit_before += 1;
            outcome.max_bite_before_mm = outcome.max_bite_before_mm.max(bite.bite_mm);
            outcome.max_approach_before_deg =
                outcome.max_approach_before_deg.max(bite.approach_deg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{cuts_deeper, min_drop_mm, top_aligned};

    /// Under a flat face (outward normal straight down) an axis offset `D`
    /// sideways meets the face within a limit `L` once it drops `D·cot L`:
    /// the angle from the normal is `atan(D / u)`.
    #[test]
    fn under_a_flat_face_the_drop_is_the_offset_times_the_cotangent_of_the_limit() {
        let limit = 50.0_f64.to_radians();
        let drop = min_drop_mm(1.0, 0.0, [0.0, 0.0, -1.0], limit.cos()).expect("admissible");
        let expected = 1.0 / limit.tan();
        assert!(
            (drop - expected).abs() < 1e-6,
            "drop {drop} against {expected}"
        );
    }

    /// Under a face 75° from horizontal, facing -X: an offset towards -X (out
    /// of the face) is admissible with no drop at all, and an offset towards
    /// +X (into the wall's side) never is, however far down the base goes.
    #[test]
    fn under_a_steep_face_only_a_lean_out_of_it_is_admissible() {
        let (sin, cos) = 75.0_f64.to_radians().sin_cos();
        let wall = [-sin, 0.0, -cos];
        let limit = 50.0_f64.to_radians().cos();
        assert_eq!(min_drop_mm(-1.0, 0.0, wall, limit), Some(0.0));
        assert_eq!(min_drop_mm(1.0, 0.0, wall, limit), None);
        // Straight down is 75° from this normal, past a 50° limit.
        assert_eq!(min_drop_mm(0.0, 0.0, wall, limit), None);
    }

    /// The section under the seat is kept; a longer arm repeats its root.
    #[test]
    fn a_profile_is_resampled_from_the_top() {
        assert_eq!(
            top_aligned(&[1.0, 2.0, 3.0], 5),
            vec![1.0, 1.0, 1.0, 2.0, 3.0]
        );
        assert_eq!(top_aligned(&[1.0, 2.0, 3.0, 4.0, 5.0], 2), vec![4.0, 5.0]);
        assert!(top_aligned(&[], 3).is_empty());
    }

    /// A shaft already cut may stay cut as deep, never deeper; one not cut
    /// may not be cut at all.
    #[test]
    fn a_move_may_not_cut_a_shaft_deeper() {
        assert!(!cuts_deeper(0.0, 0.0));
        assert!(cuts_deeper(0.0, 1e-6));
        assert!(!cuts_deeper(0.2, 0.2));
        assert!(cuts_deeper(0.2, 0.21));
    }
}
