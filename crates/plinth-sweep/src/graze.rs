//! **A head only where a straight shaft would graze the model** (Craig ruling
//! 2026-10-08, replacing "a head on every off-normal contact").
//!
//! # The rule
//!
//! A leaning straight arm whose shaft stays at least the final clearance (0.3
//! mm) clear of the model stays STRAIGHT. A contact gets an angled head only
//! where a straight tip's shaft would graze or clip the model, closer than the
//! final gap beyond its contact ball. Steep-face heads planned at placement
//! ([`crate::heads::head_triggers`]) stay as they were; hidden cavities are
//! exempt (no heads, no clearance rule there).
//!
//! # Why it runs here
//!
//! A head puts the joint `K` about `g + 2R + r_K` (roughly 1.8 mm) below the
//! seat, and the post then runs from `K` to the arm's UNCHANGED base. Measured
//! on the v19 grazers, that post leans past the 57.5 degree lean law on most of
//! them (Round 25 11 of 11, Round 50 31 of 31): the post must be re-landed, lower
//! on its own host. That is a layout move, so it runs while layout moves are
//! still allowed: after the descents, merges and lip seats have set every
//! landing, and BEFORE the head joins, the ties and the last down-sweep.
//! Nothing after the footprint rule re-plans a brace.
//!
//! # What it does
//!
//! 1. [`crate::member_clearance::graze_census`] names the grazers (the audit's
//!    report 4, in the engine), in ONE clearance call.
//! 2. For each, in id order: refuse (keep straight, warn) if another member
//!    rests on the arm; plan the head at the arm's seat with no steepness gate
//!    ([`crate::heads::plan_graze_head`]) and check it against the live field
//!    ([`crate::heads::check_head`]); then re-land the post from `K` on the
//!    arm's own host, from the highest layer its seat zone leaves free down. A
//!    candidate must pass the lean law, keep the seat plane's free side, keep
//!    its base above the raft, have a clear path
//!    ([`crate::slenderness::segment_path_sized`]) and clear the model by the
//!    final gap. The first that does is taken; none, and the arm keeps its
//!    straight tip with the reason recorded. Emit and warn, never refuse.
//! 3. **The joint is then sized for its own post.** The head is first planned
//!    for the steepest lean its joint can carry (the registered 57.5 degrees,
//!    or less where the joint would pass the maximum radius), which makes the
//!    joint fat. The post the arm actually gets leans less, so the head is
//!    planned a second time for that lean, and kept if its post still lands.
//!
//! A plate arm is not converted: its post would stand on the plate under a new
//! joint and lap the roots around it (measured: the laid tube's root laps, the
//! flat slab's stranded seats), and a grazer that cannot be converted is warned.
//!
//! The caller re-runs the down-sweep, which sizes the joint and post at the
//! post the file draws.

use geom_kernel::vec3;

use crate::head_join::{self, HeadJoinContext};
use crate::heads::{
    check_head, plan_graze_head, planned_tip_radius_mm, GrazeReason, Head, HeadCapsule,
    HeadCheckCtx, HeadRefusal, HeadRequest, HeadRule, HeadsOutcome,
};
use crate::up_sweep::{ColumnHistory, PostPlane};
use crate::{Landing, StrutKind, SweepStrut};

/// What the conversion reads besides the field.
pub(crate) struct GrazeEnv<'a> {
    pub joins: &'a HeadJoinContext<'a>,
    pub profiles: &'a plinth_core::TipContactProfiles,
    pub rule: HeadRule,
}

/// One place the post from the joint may land.
#[derive(Clone, Copy, Debug)]
struct Candidate {
    landing: Landing,
    k_bottom: usize,
    base_cell: usize,
    /// The host's id for a landing on a member (the path may end on its axis).
    host: Option<u32>,
}

/// How many path-clear candidates are measured against the model in one
/// batch. A packing size for the kernel call and nothing else: the first
/// candidate in order that clears wins whatever the size.
const CLEARANCE_BATCH: usize = 8;

/// The live field one grazer is converted against.
struct Field<'a> {
    env: &'a GrazeEnv<'a>,
    struts: &'a [SweepStrut],
    arms: &'a [plinth_support::SweepArm],
    slot_of: &'a [Option<usize>],
    index: &'a ColumnHistory,
    accepted: &'a [HeadCapsule],
    surface: &'a geom_kernel::NativeSurface,
    floor_z_mm: f64,
    margin_mm: f64,
}

/// **Convert the grazing straight tips of `struts` to heads** where a legal
/// head and post exist. Returns how many were converted; the caller re-runs
/// the down-sweep when that is not zero. Every grazer left straight is
/// recorded in `outcome.graze_reasons`; a head the planner or the checks
/// refused is also counted in `outcome.refusals`.
pub(crate) fn convert(
    env: &GrazeEnv<'_>,
    struts: &mut [SweepStrut],
    outcome: &mut HeadsOutcome,
) -> usize {
    let joins = env.joins;
    let slender = joins.slender;
    let grid = slender.grid;
    let height = slender.layer_height_mm;
    let Some(surface) = joins.surface() else {
        return 0;
    };
    let rule = env.rule;
    // The arms the lowering will get for every live straight support arm.
    let members: Vec<usize> = (0..struts.len())
        .filter(|&index| {
            let strut = &struts[index];
            strut.kind == StrutKind::Arm
                && crate::slenderness::live(strut)
                && strut.k_bottom != usize::MAX
                && strut.head.as_ref().is_none_or(|head| head.drawn_straight)
        })
        .collect();
    if members.is_empty() {
        return 0;
    }
    let (profiles, _) = crate::adapter::drawn_profiles(struts, slender.load_step_taper);
    let arms = head_join::measured_arms(joins, struts, &profiles, &members);
    let rows = crate::member_clearance::graze_census(
        Some(surface),
        struts,
        &members,
        &arms,
        joins.layer_plan,
        joins.plate_z_mm,
        rule.gap_mm,
        rule.allowance_mm,
        &slender.history.hidden,
    );
    outcome.graze_asked = rows.len();
    if rows.is_empty() {
        return 0;
    }
    let mut slot_of: Vec<Option<usize>> = vec![None; struts.len()];
    for (slot, &member) in members.iter().enumerate() {
        slot_of[member] = Some(slot);
    }
    // The field's shafts, every grazer's old one included: each grazer is
    // excepted from its own checks, and a converted arm's new shaft is
    // stamped beside the old, so a later grazer sees both (conservative).
    let mut index = ColumnHistory::new(grid);
    crate::congestion::stamp_all(&mut index, struts, height);
    let carries = crate::landing_check::carries_someone(struts);
    let mut accepted: Vec<HeadCapsule> = struts
        .iter()
        .filter_map(|strut| strut.head.as_ref())
        .filter(|head| !head.drawn_straight)
        .map(HeadCapsule::of)
        .collect();
    let floor_z_mm = head_join::join_floor_z_mm(joins.plate_z_mm, height);
    let margin_mm = height + crate::member_clearance::raster_half_diagonal_mm(grid);
    let mut converted = 0;
    for row in rows {
        let g = row.index;
        let id = struts[g].id;
        let Some(slot) = slot_of[g] else { continue };
        if carries[g] {
            outcome.graze_reasons.push((id, GrazeReason::CarriesMembers));
            continue;
        }
        let arm = &arms[slot];
        let (Some(unit), Some(face_point)) = (
            vec3::normalize(arm.seat_normal_mm),
            arm.seat_face_point_mm.or(struts[g].seat_face.point_mm),
        ) else {
            outcome.refusals.entry(HeadRefusal::NoSurface).or_default().push(id);
            outcome.graze_reasons.push((id, GrazeReason::Refused(HeadRefusal::NoSurface)));
            continue;
        };
        let tip_lb = planned_tip_radius_mm(struts[g].tip_radius_mm(), &rule, |radius| {
            env.profiles.ball_factor(radius)
        });
        let request = HeadRequest {
            axis_point_mm: arm.seat_mm,
            face_point_mm: face_point,
            unit,
            tip_radius_lb_mm: tip_lb,
            ball_radius_mm: env.profiles.ball_factor(tip_lb) * tip_lb,
            seat_cell: u32::try_from(struts[g].cell).unwrap_or(u32::MAX),
            k_top: struts[g].k_top,
        };
        let field = Field {
            env,
            struts,
            arms: &arms,
            slot_of: &slot_of,
            index: &index,
            accepted: &accepted,
            surface,
            floor_z_mm,
            margin_mm,
        };
        let found = match convert_one(&field, g, &request, tip_lb) {
            Ok(found) => found,
            Err(reason) => {
                if let GrazeReason::Refused(refusal) = reason {
                    outcome.refusals.entry(refusal).or_default().push(id);
                }
                outcome.graze_reasons.push((id, reason));
                continue;
            }
        };
        // ---- apply: the head, then the post below it.
        let (head, candidate, mut post_path) = found;
        post_path.push(head.joint_cell);
        accepted.push(HeadCapsule::of(&head));
        let strut = &mut struts[g];
        strut.head = Some(head);
        strut.straight_fallback = None;
        strut.fallback_path.clear();
        strut.set_post(candidate.landing, candidate.k_bottom, post_path);
        index.stamp_strut(strut, height);
        outcome.graze_converted.push(id);
        outcome.built += 1;
        converted += 1;
    }
    converted
}

/// A converted arm: its head, where its post lands and the post's cells
/// (layers `k_bottom` up to the one under the joint).
type Converted = (Head, Candidate, Vec<u32>);

/// Plan, check and land one grazer's head: the head for the steepest lean its
/// joint can carry, the post it gets, then the head again for that post's own
/// lean. `Err` names why the arm stays straight: the planner's or the checks'
/// refusal, no post that lands, or a host too thin to take the post without
/// being raised.
fn convert_one(
    field: &Field<'_>,
    g: usize,
    request: &HeadRequest,
    tip_lb_mm: f64,
) -> Result<Converted, GrazeReason> {
    let (head, cos_cap) = plan_checked(field, g, request, tip_lb_mm, None)
        .map_err(GrazeReason::Refused)?;
    // The first plan's joint is the fat one (the steepest lean it can carry),
    // so its post is only FOUND here, to learn the lean the arm really gets.
    let Some((candidate, _)) = choose_post(field, g, &head, cos_cap, false) else {
        return Err(GrazeReason::NoLegalPost);
    };
    // The joint is sized for ITS post: the lean the post really has, which is
    // at most the cap the first plan used. Planned again for that lean the
    // joint is smaller, K stands a little higher, and the same base is a
    // longer, gentler post. The post must then not raise its host.
    let cos_post = lean_cos(field, &head, &candidate);
    if cos_post > cos_cap + 1.0e-6 && cos_post <= 1.0 {
        if let Ok((tight, cos_tight)) = plan_checked(field, g, request, tip_lb_mm, Some(cos_post))
        {
            if let Some((tight_candidate, tight_post)) =
                choose_post(field, g, &tight, cos_tight, true)
            {
                return Ok((tight, tight_candidate, tight_post));
            }
        }
    }
    // No gentler plan: the first one stands if its post does not raise its host.
    choose_post(field, g, &head, cos_cap, true)
        .map(|(candidate, post)| (head, candidate, post))
        .ok_or(GrazeReason::HostTooThin)
}

/// The cosine of the lean from vertical of the post from `head`'s joint to
/// `candidate`'s base.
fn lean_cos(field: &Field<'_>, head: &Head, candidate: &Candidate) -> f64 {
    let slender = field.env.joins.slender;
    let lateral_mm = crate::cell_distance_mm(slender.grid, head.joint_cell as usize, candidate.base_cell);
    #[expect(clippy::cast_precision_loss, reason = "a layer count is exact in f64")]
    let drop_mm = head.k_joint.saturating_sub(candidate.k_bottom) as f64 * slender.layer_height_mm;
    if drop_mm <= 0.0 {
        return 0.0;
    }
    drop_mm / lateral_mm.hypot(drop_mm)
}

/// Plan the head (for the registered lean, lowered where its joint would pass
/// the maximum radius, or for `cos_lean_max` where given) and run the placement
/// tests against the live field.
fn plan_checked(
    field: &Field<'_>,
    g: usize,
    request: &HeadRequest,
    tip_lb_mm: f64,
    cos_lean_max: Option<f64>,
) -> Result<(Head, f64), HeadRefusal> {
    let env = field.env;
    let joins = env.joins;
    let slender = joins.slender;
    let rule = env.rule;
    let (head, cos_cap) = match cos_lean_max {
        None => plan_restricted(request, &rule, slender.grid)?,
        Some(cos_lean_max) => (
            plan_graze_head(request, &HeadRule { cos_lean_max, ..rule }, slender.grid)?,
            cos_lean_max,
        ),
    };
    let ctx = HeadCheckCtx {
        grid: slender.grid,
        history: field.index,
        hidden: &slender.history.hidden,
        except: &[field.struts[g].id],
        keep_out: slender.keep_out,
        surface: Some(field.surface),
        mesh: joins.mesh,
        plate_z_mm: joins.plate_z_mm,
        layer_height_mm: slender.layer_height_mm,
        separation_mm: rule.gap_mm,
    };
    check_head(&head, tip_lb_mm, &ctx, field.accepted).map(|()| (head, cos_cap))
}

/// The first post from `head`'s joint that lands: the candidates in order, the
/// cheap tests first, the model clearance in batches.
fn choose_post(
    field: &Field<'_>,
    g: usize,
    head: &Head,
    cos_cap: f64,
    not_raising: bool,
) -> Option<(Candidate, Vec<u32>)> {
    let env = field.env;
    let joins = env.joins;
    let slender = joins.slender;
    let rule = env.rule;
    let id = field.struts[g].id;
    let joint_cell = head.joint_cell as usize;
    let plane = PostPlane {
        joint_mm: head.joint_mm,
        contact_mm: head.contact_mm,
        unit: head.unit,
        radius_mm: rule.max_radius_mm.max(head.joint_radius_mm),
        plate_z_mm: joins.plate_z_mm,
        margin_mm: field.margin_mm,
    };
    let candidates = candidates_for(field, g, head, joint_cell, cos_cap, not_raising);
    let widest = head.joint_radius_mm.max(field.struts[g].tip_radius_mm());
    let mut batch: Vec<(Candidate, Vec<u32>)> = Vec::new();
    let mut candidates = candidates.into_iter().peekable();
    while candidates.peek().is_some() {
        while batch.len() < CLEARANCE_BATCH {
            let Some(candidate) = candidates.next() else { break };
            let base = head_join::base_point(joins, candidate.base_cell, candidate.k_bottom);
            let raster_base = [base[0], base[1], base[2] - joins.plate_z_mm];
            if !plane.admits(raster_base) {
                continue;
            }
            if candidate.host.is_some()
                && !head_join::stays_above_raft(head.joint_mm, base, widest, field.floor_z_mm)
            {
                continue;
            }
            let excepted: Vec<u32> = std::iter::once(id).chain(candidate.host).collect();
            let Ok(path) = crate::slenderness::segment_path_sized(
                slender,
                field.index,
                joint_cell,
                head.k_joint,
                candidate.base_cell,
                candidate.k_bottom,
                &excepted,
                Some(widest),
            ) else {
                continue;
            };
            batch.push((candidate, path));
        }
        if batch.is_empty() {
            break;
        }
        let starts = vec![head.joint_mm; batch.len()];
        let ends: Vec<[f64; 3]> = batch
            .iter()
            .map(|(candidate, _)| {
                head_join::base_point(joins, candidate.base_cell, candidate.k_bottom)
            })
            .collect();
        let radii = vec![head.joint_radius_mm; batch.len()];
        let zero = vec![0.0; batch.len()];
        let found = field
            .surface
            .segment_clearance_classified(&starts, &ends, &radii, &radii, 0.0, &zero, &zero)
            .map_or_else(
                |_| vec![None; batch.len()],
                |rows| {
                    rows.into_iter()
                        .map(geom_kernel::SegmentClearanceClassified::clearance_mm)
                        .collect()
                },
            );
        match found.iter().position(|clearance| clearance.is_some_and(|c| c >= rule.gap_mm)) {
            Some(position) => return Some(batch.swap_remove(position)),
            None => batch.clear(),
        }
    }
    None
}

/// **Plan the head for the steepest lean its joint can carry.** The joint of
/// a head is planned to carry every post the lean law admits under the head
/// axis (`heads::solve_joint_for_axis`), and on a face tilted 40 degrees or
/// more that joint exceeds the registered maximum radius: [`HeadRefusal::JointOverMax`].
/// At placement that refuses the head. Here the post is chosen afterwards, so
/// the cap is lowered instead: the largest lean at which the joint still fits
/// is found by bisection on `cos λmax` (a numerical resolution, not a
/// physical number), and only posts inside it are offered. Returns the head
/// and the `cos λmax` it was planned at; the registered law's own value when it
/// fits whole. Still `JointOverMax` when even a vertical post does not fit.
fn plan_restricted(
    request: &HeadRequest,
    rule: &HeadRule,
    grid: &crate::grid::Grid,
) -> Result<(Head, f64), HeadRefusal> {
    match plan_graze_head(request, rule, grid) {
        Ok(head) => Ok((head, rule.cos_lean_max)),
        Err(HeadRefusal::JointOverMax) => {
            let at = |cos_lean_max: f64| {
                plan_graze_head(request, &HeadRule { cos_lean_max, ..*rule }, grid)
            };
            // A vertical post only: the least the joint can be asked to carry.
            at(1.0)?;
            let (mut fails, mut fits) = (rule.cos_lean_max, 1.0_f64);
            for _ in 0..24 {
                let middle = 0.5 * (fails + fits);
                if at(middle).is_ok() {
                    fits = middle;
                } else {
                    fails = middle;
                }
            }
            at(fits).map(|head| (head, fits))
        }
        Err(other) => Err(other),
    }
}

/// Where the post from `head`'s joint may land, in the order they are tried:
/// the arm's own host from the highest layer its seat zone leaves free down to
/// the host's lowest. Only candidates the lean law admits are listed
/// (arithmetic only); the rest of the tests are [`choose_post`]'s.
fn candidates_for(
    field: &Field<'_>,
    g: usize,
    head: &Head,
    joint_cell: usize,
    cos_cap: f64,
    not_raising: bool,
) -> Vec<Candidate> {
    let struts = field.struts;
    let slender = field.env.joins.slender;
    let grid = slender.grid;
    let height = slender.layer_height_mm;
    let k_joint = head.k_joint;
    // The steepest lean the joint was planned for: `tan` of the cap angle is
    // the most sideways travel per layer, in layer heights.
    let tan_cap = (1.0 - cos_cap * cos_cap).max(0.0).sqrt() / cos_cap.max(f64::MIN_POSITIVE);
    let admitted = |base_cell: usize, k_bottom: usize| -> bool {
        let Some(layers) = k_joint.checked_sub(k_bottom).filter(|&n| n > 0) else {
            return false;
        };
        let lateral_mm = crate::cell_distance_mm(grid, joint_cell, base_cell);
        #[expect(clippy::cast_precision_loss, reason = "a layer count is exact in f64")]
        let within_cap = lateral_mm <= layers as f64 * height * tan_cap + 1.0e-9;
        within_cap
            && (lateral_mm <= 0.0 || slender.lean.descent_cost_mm(layers, lateral_mm).is_some())
    };
    let mut out = Vec::new();
    // Only an arm that rests on a member: a plate arm or a model arm would
    // need a new root (see the module comment).
    let Landing::Strut { id, .. } = struts[g].landing else {
        return out;
    };
    let host_index = id as usize;
    let Some(host) = struts.get(host_index).filter(|host| {
        host.id == id && host.kind == StrutKind::Arm && host.k_bottom != usize::MAX
    }) else {
        return out;
    };
    let zone = host_zone_layers(field, host_index, head.joint_radius_mm);
    let cap = host
        .joinable_top_layer(height)
        .map(|top| top.saturating_sub(zone))
        .filter(|&top| top >= host.k_bottom);
    if let Some(cap) = cap {
        let highest = cap.min(k_joint.saturating_sub(1));
        for layer in (host.k_bottom..=highest).rev() {
            let base_cell = host.cell_at(layer);
            // The post is drawn at the joint's section or more; a host thinner
            // than that is RAISED to meet it down its whole length (measured:
            // the laid tube's root laps, the flat slab's lapped root, an
            // overloaded host). A lander does not make its host fat.
            let host_radius = host.radius_mm.get(layer - host.k_bottom).copied().unwrap_or(0.0);
            if not_raising && host_radius + 1.0e-9 < head.joint_radius_mm {
                continue;
            }
            if admitted(base_cell, layer) {
                out.push(Candidate {
                    landing: Landing::Strut { id, layer },
                    k_bottom: layer,
                    base_cell,
                    host: Some(id),
                });
            }
        }
    }
    out
}

/// How many layers under a straight host's own contact a joining post must
/// stay, so that its section clears the host's seat zone and the head-join
/// rule has nothing to move (a headed lander is never moved): the stretch of
/// the host's axis over which a section of the joiner's radius is still within
/// the final gap of the host's seat plane, `(g + r_K + r_host) / (n . m)`, at
/// least the host's own contact ball. Zero for a host with no measured seat
/// (a headed host already caps its joinable layers at its joint).
fn host_zone_layers(field: &Field<'_>, host_index: usize, joiner_radius_mm: f64) -> usize {
    let host = &field.struts[host_index];
    let Some(arm) = field.slot_of[host_index].and_then(|slot| field.arms.get(slot)) else {
        return 0;
    };
    let (Some(normal), Some(axis)) = (
        vec3::normalize(arm.seat_normal_mm),
        vec3::normalize(vec3::sub(arm.base_mm, arm.seat_mm)),
    ) else {
        return 0;
    };
    let rule = field.env.rule;
    let tip = host.tip_radius_mm();
    let ball = field.env.profiles.ball_factor(tip) * tip;
    let along = vec3::dot(normal, axis).max(0.1);
    let zone_mm = ball.max((rule.gap_mm + joiner_radius_mm + tip) / along);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a finite non-negative length over the layer height, ceiled to a layer count"
    )]
    let layers = (zone_mm / field.env.joins.slender.layer_height_mm).ceil().max(0.0) as usize;
    layers
}
