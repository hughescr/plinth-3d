//! **A stranded contact is a tip, not a new root** (2026-10-02).
//!
//! # The defect
//!
//! The owner opened the round-11 Cylinder in ChituBox and found the posts by
//! its drain overlapping (`plinth-evidence/slice-sweep-2026-09-02/
//! field-2026-09-28/v11-feedback-diagnosis.md`, item 1). The cause is
//! creep-45's stranded seating ([`crate::up_sweep`]'s `stranded_seat_for`):
//! demand inside the drain keep-out that no demand seat can reach is seated on
//! a rooted underside cell on the keep-out's rim, and the arm it starts drops
//! STRAIGHT to the plate. On the Cylinder's bottom the keep-out's demand comes
//! back on layers 100-103, each layer's stranded seat lands on the one-cell
//! rim beside the previous layer's, and every one stands a new plate post
//! 0.54-1.1 mm from the last: 14 of the 16 overlapping root pairs, 129 cells
//! under three or more roots.
//!
//! # The rule
//!
//! When a member the stranded rule seated stands on the plate with a root disc
//! that laps a standing root (the congestion census's own test,
//! [`crate::congestion::lapped_roots`], which asks
//! [`crate::congestion::clear_of_placed`]'s question of every disc), it lands
//! on a lapped root's MEMBER as a branch instead: anywhere on that member's
//! shaft (owner ruling 2026-09-05, "merges land anywhere on a shaft"), at the
//! highest layer the lean law reaches and the path test
//! ([`crate::slenderness::segment_path_sized`]: model, other members,
//! drain keep-out) admits, with its base disc above the raft top
//! ([`crate::head_join::stays_above_raft`]). No new number: the lap test, the
//! lean law, the path test and the raft floor are the ones in force.
//!
//! The one thing the path test is asked differently: the drain keep-out is
//! `bore + arm radius + standoff`, built with the MAXIMUM arm radius because
//! nothing is sized when it is built. Here the member is sized, so the
//! keep-out's own capsule is asked at the member's drawn radius
//! ([`crate::drain::DrainKeepOut::blocks_member`]). Without that, no branch
//! between two posts on the keep-out's rim is ever admitted: their chord dips
//! inside the circle (measured on the Cylinder: 1,679 of 1,679 path refusals
//! were the keep-out).
//!
//! # Why here, on the sized field, and not in the up-sweep
//!
//! "Laps a standing root" is a question about SIZED roots: two roots 0.54 mm
//! apart lap at the 0.57 mm radii the area law gives the ring posts and not at
//! the 0.05 mm minimum radius, which is the only radius the up-sweep could
//! assume. So this runs where the slenderness merges run: after a completed
//! down-sweep has sized every member, followed by the same down-sweep again.
//!
//! # Why after the congestion lane
//!
//! The congestion lane leans a stacked root out to clear plate, but only a
//! leaf: a member something lands on is never moved
//! (`congestion::lean_congested_roots`). A branch makes its trunk a carrier,
//! so this rule run FIRST took the leaf moves away from that lane. Measured on
//! the laid tube at develop `469d482`: run first, the rule took the
//! overlapping pairs from 38 to 20 at the lane's entry, the lane then leaned 11
//! roots instead of 22, and the finished field held 9 pairs against 8 without
//! the rule. Run after the lane, it branches only the stranded members the
//! lane could not clear: 6 pairs against 8 on the finished field, and the
//! Cylinder's 16 still come down to 4 (the lane leans none of its ring posts).
//!
//! # Overload: the next legal seat, never an overloaded trunk
//!
//! A branch hands its trunk its whole load, and the ring posts already carry
//! most of the cavity ceiling's load (~150 mm² across the ring). After every
//! round the field is sized again, and a branch whose trunk chain (the trunk
//! and everything below the junction) came out at the maximum radius is put
//! back exactly as it was and the pair is refused; the next round tries the
//! next lapped root. The test is `unmerge_overgrown`'s own: the drawn radius
//! at or above `TIP_RADIUS_MAX`. A member that still has no legal landing
//! stays a plate post and is counted ([`StrandedBranchOutcome::lapping_after`]):
//! emit and warn, never refuse.

use std::collections::HashSet;

use crate::congestion::RootDisc;
use crate::slenderness::{segment_path_sized, PathBlock, SlendernessContext};
use crate::up_sweep::ColumnHistory;
use crate::{Landing, StrutKind, SweepStrut};

/// What the rule found and did, published on the run report.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StrandedBranchOutcome {
    /// Whether the rule ran (the sabotage knob is
    /// [`crate::SweepPolicy::without_stranded_branch`]).
    pub enabled: bool,
    /// Members the stranded rule seated that are still in the sized field.
    pub stranded_members: usize,
    /// Of those, the ones standing on the plate with a root disc that laps
    /// another root when the rule began, and their ids. The defect's count.
    pub lapping_before: usize,
    pub lapping_ids_before: Vec<u32>,
    /// Branches made, over every round (a branch put back and made again onto
    /// another root counts twice).
    pub branches_made: usize,
    /// Branches standing when the rule finished.
    pub branches_kept: usize,
    /// Those branches as `(member, the member it landed on, junction layer)`,
    /// in the order they were made.
    pub branches: Vec<(u32, u32, usize)>,
    /// Branches put back because their trunk chain came out at the maximum
    /// radius once sized.
    pub put_back_overloaded: usize,
    /// Lapping members not moved because a member resting on them cannot
    /// move with them: a tie (a brace seated on it), or a resting member that
    /// carries another in turn.
    pub refused_carrying: usize,
    /// Branches left alone because the child is HEADED, or because a member
    /// resting on it is (the branch would re-land it). A head is frozen at
    /// placement and nothing moves a headed arm.
    pub headed_kept: usize,
    /// Members resting on a branched member that were re-landed on its new
    /// axis with it.
    pub resting_moved_with_branch: usize,
    /// Lapping members (per round) for which no lapped root offered a legal
    /// landing.
    pub refused_no_landing: usize,
    /// Landings priced on lapped members' shafts, and how many of them the
    /// path test refused. Path-hit witnesses.
    pub landings_priced: u64,
    pub paths_blocked: u64,
    /// The path refusals by kind: the drain's escape cylinder, model
    /// material, another member.
    pub paths_blocked_drain: u64,
    pub paths_blocked_model: u64,
    pub paths_blocked_member: u64,
    /// Landings refused because the member's base disc would reach under the
    /// raft top.
    pub refused_raft: u64,
    /// Rounds that made at least one branch.
    pub rounds: usize,
    /// Every judgement of a lapping member, in order:
    /// `(round, member, what rests on it, the roots it laps, the outcome)`,
    /// the outcome being `"branch"` (then `landed` names the trunk),
    /// `"carrying"` or `"no landing"`. What the counters summarise, member by
    /// member.
    pub decisions: Vec<StrandedDecision>,
    /// Stranded members still on the plate with a lapping root disc when the
    /// rule finished, and their ids. Zero is the rule's goal; anything else is
    /// a WARNING on an emitted file.
    pub lapping_after: usize,
    pub lapping_ids_after: Vec<u32>,
    /// The same census after each later lane that edits landings, in the
    /// order they ran: `(lane, count)`. Says WHICH lane put a stranded member
    /// back on a lapped root, when the finished count is above `lapping_after`.
    pub lapping_by_lane: Vec<(&'static str, usize)>,
    /// The same census on the FINISHED field, after every later lane.
    pub lapping_on_finished_field: usize,
    pub lapping_ids_on_finished_field: Vec<u32>,
}

/// One judgement of a lapping stranded member ([`StrandedBranchOutcome::decisions`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrandedDecision {
    pub round: usize,
    pub member: u32,
    /// Members that land on it or are tied to it when it was judged.
    pub resting: Vec<u32>,
    /// The roots its disc laps, nearest first.
    pub laps: Vec<u32>,
    pub outcome: &'static str,
    pub landed: Option<(u32, usize)>,
}

/// One branch made this round, with everything needed to put it back.
#[derive(Clone, Debug)]
pub(crate) struct Branch {
    child: u32,
    parent: u32,
    layer: usize,
    /// Every member this branch moved, the child first and then each member
    /// resting on it that was re-landed on its new axis, as it was before.
    saved: Vec<Saved>,
}

/// One member's descent as it was before a branch moved it.
#[derive(Clone, Debug)]
struct Saved {
    id: u32,
    landing: Landing,
    k_bottom: usize,
    path: Vec<u32>,
    straight_fallback: Option<(Landing, usize)>,
    fallback_path: Vec<u32>,
}

impl Saved {
    fn of(strut: &SweepStrut) -> Self {
        Self {
            id: strut.id,
            landing: strut.landing,
            k_bottom: strut.k_bottom,
            path: strut.path.clone(),
            straight_fallback: strut.straight_fallback,
            fallback_path: strut.fallback_path.clone(),
        }
    }

    fn restore(&self, strut: &mut SweepStrut) {
        strut.landing = self.landing;
        strut.k_bottom = self.k_bottom;
        strut.path = self.path.clone();
        strut.straight_fallback = self.straight_fallback;
        strut.fallback_path = self.fallback_path.clone();
    }
}

/// A landing found for a lapping member: the junction layer on the trunk,
/// the member's new path, and the new path of each member resting on it
/// (`(id, path)`), re-landed where its junction layer crosses the new axis.
type Found = (usize, Vec<u32>, Vec<(u32, Vec<u32>)>);

/// The ids of the members the stranded rule seated, found by their seat cell
/// and seat layer (which the removal pass's renumbering does not change).
/// One pass over the members against a set of the seats.
pub(crate) fn stranded_ids(struts: &[SweepStrut], seats: &[(usize, usize)]) -> Vec<u32> {
    if seats.is_empty() {
        return Vec::new();
    }
    let wanted: HashSet<(usize, usize)> = seats.iter().copied().collect();
    struts
        .iter()
        .filter(|strut| {
            strut.kind == StrutKind::Arm
                && !strut.inserted_by_load
                && strut.k_bottom != usize::MAX
                && wanted.contains(&(strut.cell, strut.k_top))
        })
        .map(|strut| strut.id)
        .collect()
}

/// Every stranded member standing on the plate whose root disc laps another
/// root, with the roots it laps (nearest first). `placed` is the field's root
/// discs ([`crate::congestion::root_discs_drawn`]).
fn lapping(
    context: &SlendernessContext<'_>,
    struts: &[SweepStrut],
    stranded: &[u32],
    placed: &[RootDisc],
) -> Vec<(u32, Vec<(u32, f64)>)> {
    let mut out = Vec::new();
    for &id in stranded {
        let Some(strut) = struts.get(id as usize) else {
            continue;
        };
        if !matches!(strut.landing, Landing::Plate) {
            continue;
        }
        let Some(slot) = crate::congestion::disc_slot(placed, id) else {
            continue;
        };
        let Some(radius) = crate::congestion::disc_radius_mm(placed, slot) else {
            continue;
        };
        let laps =
            crate::congestion::lapped_roots(context.grid, strut.base_cell(), radius, placed, Some(slot));
        if !laps.is_empty() {
            out.push((id, laps));
        }
    }
    out
}

/// The census: stranded members on the plate whose root disc laps another
/// root, by id. Read before and after the rule and on the finished field.
pub(crate) fn census(context: &SlendernessContext<'_>, struts: &[SweepStrut], stranded: &[u32]) -> Vec<u32> {
    let placed = crate::congestion::root_discs(
        context.grid,
        context.history,
        struts,
        context.flare_factor,
    );
    lapping(context, struts, stranded, &placed)
        .into_iter()
        .map(|(id, _)| id)
        .collect()
}

/// One round of the rule on a SIZED field, in place. Returns the branches it
/// made; the caller sizes the field again and then calls
/// [`put_back_overloaded`].
///
/// Members are taken LATEST seat first (highest seat layer, then highest id):
/// the post a later layer's seat laps was standing when that seat was placed,
/// so the later seat is the branch and the earlier post the trunk. Taken the
/// other way round, the earlier post landed on the later one and froze it,
/// and the later one then stood lapping its other neighbours (measured on the
/// laid-tube fixture: 4 of the 9 members left lapping were trunks this rule
/// had made of later seats). A lapped root is tried nearest first.
pub(crate) fn branch_round(
    context: &SlendernessContext<'_>,
    plate_z_mm: f64,
    struts: &mut [SweepStrut],
    stranded: &[u32],
    refused: &HashSet<(u32, u32)>,
    outcome: &mut StrandedBranchOutcome,
) -> Vec<Branch> {
    let grid = context.grid;
    let (profiles, _) = crate::adapter::drawn_profiles(struts, context.load_step_taper);
    let mut placed = crate::congestion::root_discs_drawn(
        grid,
        context.history,
        struts,
        &profiles,
        context.flare_factor,
    );
    let mut index = ColumnHistory::new(grid);
    crate::slenderness::stamp_all(&mut index, struts, context.layer_height_mm);
    // Who rests on whom (lands on, or is tied to), kept up to date as this
    // round makes branches: what must move with a member, or pins it.
    let mut resting: Vec<Vec<u32>> = vec![Vec::new(); struts.len()];
    for strut in struts.iter() {
        let host = match strut.landing {
            Landing::Strut { id, .. } => Some(id),
            Landing::Plate | Landing::Model { .. } => None,
        };
        for parent in host.into_iter().chain(strut.seat_on_strut) {
            if let Some(list) = resting.get_mut(parent as usize) {
                list.push(strut.id);
            }
        }
    }
    let floor_z_mm = crate::head_join::join_floor_z_mm(plate_z_mm, context.layer_height_mm);

    let mut order: Vec<u32> = stranded.to_vec();
    order.sort_unstable_by_key(|&id| {
        core::cmp::Reverse((
            struts.get(id as usize).map_or(0, |strut| strut.k_top),
            id,
        ))
    });
    let mut made = Vec::new();
    for &child in &order {
        let Some(strut) = struts.get(child as usize) else {
            continue;
        };
        if !matches!(strut.landing, Landing::Plate) || strut.k_bottom != 0 || strut.k_top == 0 {
            continue;
        }
        let Some(slot) = crate::congestion::disc_slot(&placed, child) else {
            continue;
        };
        let Some(radius) = crate::congestion::disc_radius_mm(&placed, slot) else {
            continue;
        };
        let laps =
            crate::congestion::lapped_roots(grid, strut.base_cell(), radius, &placed, Some(slot));
        if laps.is_empty() {
            continue;
        }
        // A HEADED child is never branched: its post would be re-routed and
        // its head is frozen at placement.
        if strut.head.is_some() {
            outcome.headed_kept += 1;
            continue;
        }
        let mut decision = StrandedDecision {
            round: outcome.rounds + 1,
            member: child,
            resting: resting.get(child as usize).cloned().unwrap_or_default(),
            laps: laps.iter().map(|(id, _)| *id).collect(),
            outcome: "carrying",
            landed: None,
        };
        // WHAT RESTS ON IT MOVES WITH IT. A member another member lands on
        // can still branch, provided every such member is an arm landing on
        // its shaft that nothing rests on in turn: the junction then goes
        // below the lowest of them, and each is re-landed where its own
        // junction layer crosses the member's new axis, under the lean law
        // and the path test (`landing_on`). A tie (a brace seated on it), or
        // a resting member that carries another, pins it where it stands.
        // Measured on the laid-tube fixture before this: 9 of the 11 lapping
        // members left standing carried exactly one up-sweep arm or one of
        // this rule's own branches.
        let mut dependants: Vec<(u32, usize)> = Vec::new();
        let mut pinned = false;
        let mut headed_dependant = false;
        for &id in resting.get(child as usize).map_or(&[][..], Vec::as_slice) {
            let Some(rest) = struts.get(id as usize) else {
                pinned = true;
                break;
            };
            // A dependant is RE-LANDED with the branch; a headed one is never
            // moved, so the branch is refused.
            if rest.head.is_some() {
                headed_dependant = true;
                break;
            }
            match rest.landing {
                Landing::Strut { id: host, layer }
                    if host == child
                        && rest.seat_on_strut.is_none()
                        && rest.kind == StrutKind::Arm
                        && resting.get(id as usize).is_none_or(Vec::is_empty) =>
                {
                    dependants.push((id, layer));
                }
                _ => {
                    pinned = true;
                    break;
                }
            }
        }
        if headed_dependant {
            outcome.headed_kept += 1;
            decision.outcome = "headed dependant";
            outcome.decisions.push(decision);
            continue;
        }
        if pinned {
            outcome.refused_carrying += 1;
            outcome.decisions.push(decision);
            continue;
        }
        let widest = profiles
            .get(child as usize)
            .map_or(0.0, |profile| profile.iter().copied().fold(0.0_f64, f64::max));
        let mut found: Option<(u32, Found)> = None;
        for &(parent, _) in &laps {
            if parent == child || refused.contains(&(child, parent)) {
                continue;
            }
            if let Some(landing) = landing_on(
                context,
                &index,
                struts,
                &profiles,
                child,
                parent,
                &dependants,
                (plate_z_mm, floor_z_mm),
                widest,
                outcome,
            ) {
                found = Some((parent, landing));
                break;
            }
        }
        let Some((parent, (layer, path, moved))) = found else {
            outcome.refused_no_landing += 1;
            decision.outcome = "no landing";
            outcome.decisions.push(decision);
            continue;
        };
        decision.outcome = "branch";
        decision.landed = Some((parent, layer));
        outcome.decisions.push(decision);
        if let Some(list) = resting.get_mut(parent as usize) {
            list.push(child);
        }
        let mut saved = vec![Saved::of(&struts[child as usize])];
        let strut = &mut struts[child as usize];
        // WHERE THIS ARM WAS, so every later un-merge pass can put it back
        // (`unmerge_overgrown` reads exactly these two fields).
        strut.straight_fallback = Some((strut.landing, strut.k_bottom));
        strut.fallback_path = core::mem::take(&mut strut.path);
        strut.landing = Landing::Strut { id: parent, layer };
        strut.k_bottom = layer;
        strut.path = path;
        index.stamp_strut(strut, context.layer_height_mm);
        for (id, new_path) in moved {
            let rest = &mut struts[id as usize];
            saved.push(Saved::of(rest));
            rest.path = new_path;
            index.stamp_strut(rest, context.layer_height_mm);
            outcome.resting_moved_with_branch += 1;
        }
        made.push(Branch {
            child,
            parent,
            layer,
            saved,
        });
        placed.remove(slot);
        outcome.branches_made += 1;
        outcome.branches.push((child, parent, layer));
    }
    made
}

/// The highest legal landing for `child` on `parent`'s shaft, with the new
/// paths of the members resting on `child` ([`Found`]).
///
/// Walks the parent's shaft from the highest layer below the child's seat
/// (and below the lowest junction a resting member has on the child)
/// downward. A layer the lean law cannot reach from the seat is skipped (a
/// lower one may be reachable: the law closes more lateral distance the
/// further down the landing is); a reachable one is priced, its base disc held
/// above the raft top, and its straight segment path-tested; then every member
/// resting on the child is re-landed at its own junction layer where the new
/// segment crosses it, under the same lean law and path test. The first layer
/// where all of that passes is taken. The path test may refuse a high landing
/// whose segment clips the model's own lowest layer beside the seat and admit
/// a lower, steeper one, which is why this does not stop at the first
/// reachable layer.
#[expect(
    clippy::too_many_arguments,
    reason = "the search reads the context, the shaft index, the field and its drawn profiles, \
          the pair, what rests on the child, the plate and raft floor, the member's own width and \
          the outcome it counts into"
)]
fn landing_on(
    context: &SlendernessContext<'_>,
    index: &ColumnHistory,
    struts: &[SweepStrut],
    profiles: &[Vec<f64>],
    child: u32,
    parent: u32,
    dependants: &[(u32, usize)],
    (plate_z_mm, floor_z_mm): (f64, f64),
    widest_mm: f64,
    outcome: &mut StrandedBranchOutcome,
) -> Option<Found> {
    let grid = context.grid;
    let law = context.lean;
    if !law.leans() {
        return None;
    }
    let seat = struts.get(child as usize)?;
    let host = struts.get(parent as usize)?;
    if host.k_bottom == usize::MAX || host.kind == StrutKind::Brace {
        return None;
    }
    let k_top = seat.k_top;
    let lo = host.k_bottom + 1;
    // Strictly below every resting member's junction on the child, so each
    // still meets the child's shaft after the move.
    let below = dependants
        .iter()
        .map(|(_, layer)| *layer)
        .min()
        .unwrap_or(k_top);
    // A headed trunk is joined only below its joint (`joinable_top_layer`;
    // `k_top` for a straight one).
    let host_top = host.joinable_top_layer(context.layer_height_mm)?;
    let hi = host_top.min(k_top.checked_sub(1)?).min(below.checked_sub(1)?);
    if lo > hi {
        return None;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "layer indices bounded by the plan's own layer count are exact in f64"
    )]
    let z_at = |layer: usize| plate_z_mm + layer as f64 * context.layer_height_mm;
    let seat_point = [
        grid.center_x_mm(grid.col_of(seat.cell)),
        grid.center_y_mm(grid.row_of(seat.cell)),
        z_at(k_top),
    ];
    let except = [parent, child];
    for layer in (lo..=hi).rev() {
        let base = host.cell_at(layer);
        let lateral_mm = crate::cell_distance_mm(grid, seat.cell, base);
        let layers = k_top - layer;
        let needed = law.layers_to_reach(lateral_mm);
        if needed == 0 {
            // The law can never close this gap from any height.
            return None;
        }
        if layers < needed {
            continue;
        }
        if law.descent_cost_mm(layers, lateral_mm).is_none() {
            continue;
        }
        outcome.landings_priced += 1;
        let base_point = [
            grid.center_x_mm(grid.col_of(base)),
            grid.center_y_mm(grid.row_of(base)),
            z_at(layer),
        ];
        if !crate::head_join::stays_above_raft(seat_point, base_point, widest_mm, floor_z_mm) {
            // Lower only sinks the base disc further: nothing below passes.
            outcome.refused_raft += 1;
            return None;
        }
        // The member is SIZED here, so the drain keep-out is asked at its own
        // drawn radius, not at the maximum arm radius the keep-out assumed
        // before sizing (`DrainKeepOut::blocks_member`). Two posts on the
        // keep-out's rim are joined by a chord, and every chord of a circle
        // dips inside it: measured on the Cylinder, the maximum-radius test
        // refused all 1,679 such paths.
        match segment_path_sized(
            context,
            index,
            seat.cell,
            k_top,
            base,
            layer,
            &except,
            Some(widest_mm),
        ) {
            Ok(path) => {
                if let Some(moved) = reland_resting(
                    context, index, struts, profiles, child, parent, layer, &path, dependants,
                    outcome,
                ) {
                    return Some((layer, path, moved));
                }
            }
            Err(why) => count_block(outcome, why),
        }
    }
    None
}

fn count_block(outcome: &mut StrandedBranchOutcome, why: PathBlock) {
    outcome.paths_blocked += 1;
    match why {
        PathBlock::Drain => outcome.paths_blocked_drain += 1,
        PathBlock::Model => outcome.paths_blocked_model += 1,
        PathBlock::Member => outcome.paths_blocked_member += 1,
        PathBlock::NoRise => {}
    }
}

/// The new path of every member resting on `child` once the child's axis is
/// `path` from `layer` up: each keeps its seat and its junction layer, and its
/// base moves to where the child's new axis stands at that layer. `None` when
/// any of them would break the lean law or the path test.
#[expect(
    clippy::too_many_arguments,
    reason = "the child's new axis, the field and its profiles, the index, the trunk and what \
          rests on the child, and the outcome it counts into"
)]
fn reland_resting(
    context: &SlendernessContext<'_>,
    index: &ColumnHistory,
    struts: &[SweepStrut],
    profiles: &[Vec<f64>],
    child: u32,
    parent: u32,
    layer: usize,
    path: &[u32],
    dependants: &[(u32, usize)],
    outcome: &mut StrandedBranchOutcome,
) -> Option<Vec<(u32, Vec<u32>)>> {
    let law = context.lean;
    let mut moved = Vec::with_capacity(dependants.len());
    for &(id, junction) in dependants {
        let rest = struts.get(id as usize)?;
        let base = *path.get(junction.checked_sub(layer)?)? as usize;
        let lateral_mm = crate::cell_distance_mm(context.grid, rest.cell, base);
        let layers = rest.k_top.checked_sub(junction)?;
        let needed = law.layers_to_reach(lateral_mm);
        if needed == 0 || layers < needed || law.descent_cost_mm(layers, lateral_mm).is_none() {
            return None;
        }
        let widest = profiles
            .get(id as usize)
            .map_or(0.0, |profile| profile.iter().copied().fold(0.0_f64, f64::max));
        match segment_path_sized(
            context,
            index,
            rest.cell,
            rest.k_top,
            base,
            junction,
            &[child, id, parent],
            Some(widest),
        ) {
            Ok(new_path) => moved.push((id, new_path)),
            Err(why) => {
                count_block(outcome, why);
                return None;
            }
        }
    }
    Some(moved)
}

/// Put back every branch of `made` whose trunk chain came out at the maximum
/// radius once the field was sized: the trunk at and below the junction, and
/// everything the trunk stands on in turn. The pair is refused, so the next
/// round tries the next lapped root. Returns how many were put back.
pub(crate) fn put_back_overloaded(
    struts: &mut [SweepStrut],
    made: &[Branch],
    tip_radius_max_mm: f64,
    refused: &mut HashSet<(u32, u32)>,
    outcome: &mut StrandedBranchOutcome,
) -> usize {
    let overloaded: Vec<bool> = made
        .iter()
        .map(|branch| chain_at_maximum(struts, branch.parent, branch.layer, tip_radius_max_mm))
        .collect();
    let mut undone = 0_usize;
    // LATEST FIRST. A branch made later may have moved an earlier one with it
    // (the earlier child rests on the later-moved member), and its saved state
    // holds that earlier child as it stood on the member's OLD axis; undoing
    // the later branch first and the earlier one second leaves each member as
    // it was before either.
    for (branch, over) in made.iter().zip(overloaded).rev() {
        if !over {
            continue;
        }
        let Some(strut) = struts.get(branch.child as usize) else {
            continue;
        };
        // Only a branch still standing where this rule put it is ours to undo.
        if strut.landing
            != (Landing::Strut {
                id: branch.parent,
                layer: branch.layer,
            })
        {
            continue;
        }
        for saved in &branch.saved {
            if let Some(member) = struts.get_mut(saved.id as usize) {
                saved.restore(member);
            }
        }
        refused.insert((branch.child, branch.parent));
        outcome
            .branches
            .retain(|&kept| kept != (branch.child, branch.parent, branch.layer));
        undone += 1;
    }
    outcome.put_back_overloaded += undone;
    undone
}

/// Whether the member `id` at or below `layer`, or anything it stands on in
/// turn at or below its own landing, is drawn at `tip_radius_max_mm` or more.
fn chain_at_maximum(struts: &[SweepStrut], id: u32, layer: usize, tip_radius_max_mm: f64) -> bool {
    let mut at = Landing::Strut { id, layer };
    for _ in 0..struts.len() {
        let Landing::Strut { id, layer } = at else {
            return false;
        };
        let Some(trunk) = struts.get(id as usize) else {
            return false;
        };
        if trunk.k_bottom == usize::MAX || layer < trunk.k_bottom {
            return false;
        }
        let top = (layer - trunk.k_bottom + 1).min(trunk.radius_mm.len());
        if trunk.radius_mm[..top]
            .iter()
            .any(|radius| *radius >= tip_radius_max_mm)
        {
            return true;
        }
        at = trunk.landing;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::chain_at_maximum;
    use crate::{Landing, StrutKind, SweepStrut};

    fn member(id: u32, k_bottom: usize, k_top: usize, landing: Landing, radius: &[f64]) -> SweepStrut {
        SweepStrut {
            id,
            cell: 0,
            center_mm: [0.0, 0.0],
            kind: StrutKind::Arm,
            min_radius_mm: 0.0,
            thickened_mm: Vec::new(),
            seat_on_strut: None,
            k_top,
            k_bottom,
            landing,
            interior: false,
            radius_mm: radius.to_vec(),
            carried_mm2: vec![0.0; radius.len()],
            covered_demand: Vec::new(), twin_host: false,
            inserted_by_load: false,
            seat_face: crate::SeatFace::default(),
            seat_normal: [0.0; 3],
            fallback_path: Vec::new(),
            head: None,
            straight_fallback: None,
            path: Vec::new(),
        }
    }

    /// The overload test reads the trunk at and below the junction, then what
    /// the trunk stands on, and nothing above the junction.
    #[test]
    fn the_chain_is_read_at_and_below_the_junction() {
        // Trunk 0 on the plate, layers 0..=4, at the maximum only at layer 4.
        let trunk = member(0, 0, 4, Landing::Plate, &[0.5, 0.5, 0.5, 0.5, 1.0]);
        // Trunk 1 lands on trunk 0 at layer 2, layers 2..=6, under the maximum.
        let upper = member(1, 2, 6, Landing::Strut { id: 0, layer: 2 }, &[0.6; 5]);
        let field = [trunk, upper];
        assert!(!chain_at_maximum(&field, 0, 3, 1.0), "layer 4 is above a junction at 3");
        assert!(chain_at_maximum(&field, 0, 4, 1.0), "a junction at 4 meets the maximum");
        assert!(!chain_at_maximum(&field, 1, 5, 1.0), "trunk 0 below layer 2 is under it");
        let mut heavy = field.clone();
        heavy[0].radius_mm[1] = 1.0;
        assert!(chain_at_maximum(&heavy, 1, 5, 1.0), "the chain reaches trunk 0's layer 1");
    }
}
