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
//!    landing on another arm's shaft, on the MODEL below (fix/morel-long-posts,
//!    2026-10-02), or on the plate, within the lean law, on a straight segment
//!    that clears model material, every other member and the drain keep-out
//!    (`slenderness::segment_path`, the test remedy-1 merges and head joins
//!    use) — admitted only where the approach angle is within the arm's own
//!    derived limit, cheapest by the up-sweep's own price first (the lean
//!    law's weighted length, plus a model landing's height still owed to the
//!    plate), whose drawn solid (read outside its own seat and landing zones)
//!    clears the model by the final gap where the descent stands on a new
//!    root, the plate or the model, and otherwise never enters the model or
//!    comes nearer it than the final gap unless it already stood nearer, and
//!    each one VERIFIED by the lowering's own
//!    measurement before it is taken. An arm something else rests on moves only with its leaf
//!    children. **The sized field then decides** ([`blame`], [`put_back`]):
//!    the down-sweep sizes the pass, and a re-landing that ADDED load to a
//!    member of its new host chain that comes out overloaded, or whose NEW
//!    foot on the plate comes to lap a root it did not, is put back and its
//!    descent refused from then on; where several moves broke the same
//!    member, the longest goes back first, alone, and the field is sized
//!    again before the next (fix/lip-seat-blame, 2026-10-01). **A lean the
//!    slenderness law cannot hold is not taken** (Q1, 2026-10-10): when the
//!    cheapest descent that passed every test runs longer than the slenderness
//!    ladder's thicken rung can hold (its taper, tip kept, root raised up to
//!    the registered maximum radius) while the thickened solid keeps the final
//!    gap, the arm keeps its seat and falls to item 3. Seat-bite triggers
//!    only; the clearance re-route is not asked.
//! 2. **A seat further in on the same face.** The footprint rule
//!    (`footprint.rs`) already slides a seat along its face; a tip whose disc
//!    is on its face but whose shaft the trim still bites joins its search,
//!    and a slide is taken only where the moved arm keeps its section too
//!    (and its disc stays on the face). A tip whose disc is OFF its face is
//!    slid for contact area alone, whatever the slide does to its shaft.
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

/// Float noise on "no nearer the model than the gap, or than it was": the
/// kernel's distance on the same segment read twice, and nothing more.
const CLEARANCE_SLACK_MM: f64 = 1.0e-9;

/// What sends an arm to be re-landed by [`apply`].
#[derive(Clone, Copy)]
pub(crate) enum Trigger<'a> {
    /// The seat trim cuts the arm's shaft outside its ball (this module's own
    /// remedy 1).
    SeatBite,
    /// **The clearance re-route** (fix/drawn-solid-clearance, R2.2 of
    /// `clearance-census-diagnosis.md`): the arm's drawn solid, as the sized
    /// field draws it, stands within the final gap of the model. `violators`
    /// has one flag per member. The same candidates, path tests, lowering
    /// measurements and sized-field settle apply, with two differences that
    /// follow from the trigger: the mover must clear the final gap OUTRIGHT
    /// wherever it lands (it was sent for the gap, so "no nearer than it
    /// stood" would license it to stay inside), and an arm that has no seat
    /// bite to compare is not refused for having none.
    Clearance { violators: &'a [bool] },
}

impl Trigger<'_> {
    /// Whether member `index` is sent to be re-landed, given its measured bite.
    fn sends(self, index: usize, bite: Option<&ShaftSeatBite>) -> bool {
        match self {
            Trigger::SeatBite => bite.is_some_and(over_limit),
            Trigger::Clearance { violators } => violators.get(index).copied().unwrap_or(false),
        }
    }

    const fn is_clearance(self) -> bool {
        matches!(self, Trigger::Clearance { .. })
    }
}

/// **The members the finished-field census names as within the final gap of the
/// model**, as one flag per strut (batch-first: one census over the field),
/// and how many it names. Only arms are re-landed; `exclude` holds the arms the
/// lip-seat rule itself re-landed: Craig's ruling of 2026-10-04 keeps that
/// rule's gate ("no nearer than it stood"), so what it admitted stays a
/// WARNING and is never sent back here.
pub(crate) fn clearance_violators(
    joins: &HeadJoinContext<'_>,
    struts: &[SweepStrut],
    arms: &[SweepArm],
    exclude: &[u32],
) -> (Vec<bool>, usize) {
    let _perf_span = crate::perf::span(crate::perf::Span::LipViolators);
    let mut flags = vec![false; struts.len()];
    let Some(surface) = joins.surface() else {
        return (flags, 0);
    };
    let census = crate::member_clearance::model_clearance_census(
        Some(surface),
        struts,
        arms,
        joins.layer_plan,
        joins.plate_z_mm,
        joins.clearance_mm,
        joins.slender.lean.face_zone_factor(),
        &joins.slender.history.hidden,
    );
    let mut named = 0;
    for row in &census.within_gap {
        if row.kind != StrutKind::Arm || exclude.contains(&row.member) {
            continue;
        }
        let slot = match struts.get(row.member as usize) {
            Some(strut) if strut.id == row.member => Some(row.member as usize),
            _ => struts.iter().position(|strut| strut.id == row.member),
        };
        if let Some(slot) = slot {
            flags[slot] = true;
            named += 1;
        }
    }
    (flags, named)
}

/// The kernel's solid clearance from the model of each member's drawn
/// segment, from its drawn seat to its base at its widest drawn radius, with
/// its own seat zone untested — its tip ball, or where further, the point
/// where its solid first stands `tier` clear of its own seat plane (the
/// head-join rule's test 2, `head_join::own_seat_zone_mm`). `None` where the
/// kernel did not decide or nothing is left to test.
///
/// A member that LANDS ON THE MODEL has its landing zone untested too
/// ([`crate::member_clearance::model_landing_zones_mm`], fix/morel-long-posts
/// 2026-10-02): until then its own strike read as an intrusion, so every
/// model re-landing was refused at about −0.05 mm, and a member that already
/// stood on the model read the same −0.05 mm and so licensed a move to any
/// path no deeper than that (the Morel base's arm 127, for-craig-v10: 0.185 mm
/// into the base rim, admitted as "no nearer than it was").
fn solid_clearances(
    surface: &NativeSurface,
    arms: &[SweepArm],
    on_model: &[bool],
    layer_plan: &LayerPlan,
    plate_z_mm: f64,
    tier: f64,
) -> Vec<Option<f64>> {
    let radii: Vec<f64> = arms.iter().map(drawn_max_radius_mm).collect();
    solid_clearances_tapered(surface, arms, on_model, layer_plan, plate_z_mm, tier, &radii, &radii)
}

/// [`solid_clearances`] with each solid a FRUSTUM: radius `top_radii[i]` at
/// the seat end and `bottom_radii[i]` at the base. The seat zone ends where
/// that frustum first stands `tier` clear of the seat plane: at distance `s`
/// along the axis the plane is `s * along` away and the radius has grown by
/// `s * k`, `k = (bottom - top) / length`, so `s = (tier + top) / (along - k)`
/// (with equal radii, exactly the cylinder's `(tier + r) / along`). A frustum
/// that never leaves its seat plane's gap (`along <= k`) has nothing outside
/// its seat zone and reads `None`.
#[expect(clippy::too_many_arguments, reason = "the batched SoA shape of the kernel call it wraps")]
fn solid_clearances_tapered(
    surface: &NativeSurface,
    arms: &[SweepArm],
    on_model: &[bool],
    layer_plan: &LayerPlan,
    plate_z_mm: f64,
    tier: f64,
    top_radii: &[f64],
    bottom_radii: &[f64],
) -> Vec<Option<f64>> {
    let discs = plinth_support::tip_contact_discs(arms, layer_plan)
        .unwrap_or_else(|_| vec![None; arms.len()]);
    let n = arms.len();
    crate::perf::add(crate::PerfWork::LipSeatClearanceRows, crate::perf::count(n));
    let seats: Vec<[f64; 3]> = arms.iter().map(|arm| arm.seat_mm).collect();
    // A plate member's DRAWN shaft ends at its boss top over its foot, not on
    // the plate under the planned line (fix/morel-long-posts, 2026-10-02:
    // read on the planned line, the Morel's rim arm 120 cleared the base rim
    // here and entered it by 0.092 mm in the file).
    let flare = layer_plan
        .heights_mm()
        .first()
        .and_then(|&height| plinth_support::SweepRootFlare::registered(height).ok());
    let bases: Vec<[f64; 3]> = arms
        .iter()
        .map(|arm| {
            flare
                .as_ref()
                .and_then(|flare| flare.plate_shaft_of(arm, plate_z_mm))
                .map_or(arm.base_mm, |(_, bottom)| bottom)
        })
        .collect();
    let insets: Vec<f64> = arms
        .iter()
        .zip(&discs)
        .zip(top_radii.iter().zip(bottom_radii))
        .zip(&bases)
        .map(|(((arm, disc), (&top, &bottom)), &base)| {
            let Some(disc) = disc else {
                return f64::INFINITY;
            };
            let offset = geom_kernel::vec3::sub(base, arm.seat_mm);
            let Some(axis) = geom_kernel::vec3::normalize(offset) else {
                return f64::INFINITY;
            };
            let reach =
                geom_kernel::vec3::norm(geom_kernel::vec3::sub(disc.ball_centre_mm, arm.seat_mm))
                    + disc.ball_radius_mm;
            let along = geom_kernel::vec3::dot(disc.outward_normal, axis);
            // The radius grows by `k` per millimetre of axis; zero for a
            // cylinder, so every caller that passes equal radii reads as it did.
            let k = if bottom > top {
                (bottom - top) / geom_kernel::vec3::norm(offset)
            } else {
                0.0
            };
            if along <= k {
                return f64::INFINITY;
            }
            reach.max((tier + top) / (along - k))
        })
        .collect();
    let landing_zones = crate::member_clearance::model_landing_zones_mm(
        surface, &seats, &bases, bottom_radii, on_model, tier,
    );
    surface
        .segment_clearance_classified(&seats, &bases, top_radii, bottom_radii, 0.0, &insets, &landing_zones)
        .map_or_else(
            |_| vec![None; n],
            |rows| {
                rows.into_iter()
                    .map(SegmentClearanceClassified::clearance_mm)
                    .collect()
            },
        )
}

/// The widest radius the arm is drawn at: the solid that [`solid_clearances`]
/// reads against the model is the arm's axis at this one radius.
fn drawn_max_radius_mm(arm: &SweepArm) -> f64 {
    arm.radius_profile_mm
        .iter()
        .copied()
        .fold(arm.tip_radius_mm, f64::max)
}

/// **The thickest radius at which an arm keeps the final gap**, from the
/// clearance this module already measured for it: `clearance_mm` is the gap
/// between the model and the arm's axis solid drawn at `drawn_mm`, so growing
/// the radius by `clearance_mm - gap_mm` spends exactly the spare gap. It never
/// exceeds `max_mm`, the registered maximum radius, and is never below the
/// drawn radius (an arm already inside the gap cannot be thickened at all).
fn thickest_radius_keeping_gap_mm(
    drawn_mm: f64,
    clearance_mm: f64,
    gap_mm: f64,
    max_mm: f64,
) -> f64 {
    (drawn_mm + (clearance_mm - gap_mm).max(0.0)).min(max_mm.max(drawn_mm))
}

/// **Whether the slenderness ladder's thicken rung cannot hold a proposed
/// lean** (Q1): the lean `arm` (drawn as the trial draws it, measured `now`
/// clear of the model) for `strut`, landing as `candidate` proposes.
///
/// 1. Arithmetic: the run (the chord the ladder measures) against `S` times
///    the thickest UNIFORM radius the measured clearance leaves room for
///    ([`thickest_radius_keeping_gap_mm`]). A run within it is held.
/// 2. Otherwise the rung's own remedy: the root its taper needs with the tip
///    kept ([`crate::slenderness::taper_root_to_hold_mm`]; none at all below
///    the maximum radius means it cannot hold), drawn as that frustum and read
///    against the model. It cannot hold where that frustum comes nearer the
///    model than the gap, or than the lean as drawn already stands.
///
/// Stage 2 exists because the clearance of a lean leaving its seat face is
/// about the gap itself: the seat zone ends where the solid first stands the
/// gap clear of the seat plane, so the minimum is read there, at the TIP end,
/// where the rung never adds radius. Stage 1 alone reads such a lean as
/// "cannot grow at all" wherever it stands (`make_brace_pair` arm 3 and the
/// Morel base's arm 44 read 0.3000 and 0.3002 mm; q12 regressions,
/// 2026-10-12).
///
/// One kernel read of one row, at most once per arm per pass: it is asked only
/// of the cheapest descent that passed every other test, and either answer
/// ends that arm's search (a refusal keeps the seat, an admission commits the
/// move). A lean with no measured frustum is not refused: nothing proves it
/// cannot be held.
fn lean_too_slender(
    context: &SlendernessContext<'_>,
    surface: Option<&NativeSurface>,
    strut: &SweepStrut,
    candidate: &Candidate,
    arm: &SweepArm,
    now: f64,
    (layer_plan, plate_z_mm, tier): (&LayerPlan, f64, f64),
) -> bool {
    let law = context.law;
    #[expect(
        clippy::cast_precision_loss,
        reason = "a layer count bounded by the plan's own layer count is exact in f64"
    )]
    let drop_mm = strut.k_top.saturating_sub(candidate.k_bottom) as f64 * context.layer_height_mm;
    let run_mm = drop_mm.hypot(crate::cell_distance_mm(context.grid, strut.cell, candidate.cell));
    let drawn = drawn_max_radius_mm(arm);
    if run_mm <= law.max_unbraced_mm(thickest_radius_keeping_gap_mm(drawn, now, tier, law.max_radius_mm())) {
        return false;
    }
    let top = arm.radius_profile_mm.last().copied().unwrap_or(arm.tip_radius_mm);
    let Some(root) = crate::slenderness::taper_root_to_hold_mm(law, &arm.radius_profile_mm, top, run_mm) else {
        return true;
    };
    let Some(surface) = surface else {
        return false;
    };
    let on_model = [matches!(candidate.landing, Landing::Model { .. })];
    solid_clearances_tapered(
        surface,
        std::slice::from_ref(arm),
        &on_model,
        layer_plan,
        plate_z_mm,
        tier,
        &[top],
        &[root.max(drawn)],
    )
    .first()
    .copied()
    .flatten()
    .is_some_and(|clear| clear < tier.min(now) - CLEARANCE_SLACK_MM)
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

/// Why the clearance re-route kept an arm it was sent: the arm carries part
/// of a frame the slenderness ladder solved.
///
/// Moving such an arm moves a junction of that frame. The frame's band
/// layers and count, its window and its two-post ceiling were solved for the
/// pair as it stands; carried along unsolved, the frame could leave a post
/// over its slenderness limit while the report counts it braced, and taken
/// down and solved again it could leave the post unbraced. Either trades a
/// member that stands clear of the model but inside the final gap for a
/// post that may sway or snap mid-print, so the re-route keeps the arm and
/// warns (emit and warn).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameLock {
    /// A cross is seated on the arm itself.
    CrossTied,
    /// A cross is seated on a member that rests on it, directly or not.
    CrossTiedBelow,
    /// A member that is not an arm (a brace, a mast, a model tie) rests on
    /// it, directly or not.
    NonArmResting,
}

/// One flag per member: whether a cross (a brace's top) is seated on it.
pub(crate) fn crosses_seated(struts: &[SweepStrut]) -> Vec<bool> {
    let mut tied = vec![false; struts.len()];
    for strut in struts {
        if let Some(host) = strut.seat_on_strut {
            if let Some(flag) = tied.get_mut(host as usize) {
                *flag = true;
            }
        }
    }
    tied
}

/// Why the clearance re-route keeps `member` for a frame, given which
/// members have a cross seated on them (`tied`) and every member resting on
/// it, directly or not (`followers`); `None` where it may move with them.
pub(crate) fn frame_lock(
    struts: &[SweepStrut],
    tied: &[bool],
    member: usize,
    followers: &[usize],
) -> Option<FrameLock> {
    if tied.get(member).copied().unwrap_or(false) {
        Some(FrameLock::CrossTied)
    } else if followers.iter().any(|&follower| tied.get(follower).copied().unwrap_or(false)) {
        Some(FrameLock::CrossTiedBelow)
    } else if followers
        .iter()
        .any(|&follower| struts.get(follower).is_some_and(|strut| strut.kind != StrutKind::Arm))
    {
        Some(FrameLock::NonArmResting)
    } else {
        None
    }
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
    /// **The clearance re-route's survey** ([`Trigger::Clearance`], filled
    /// only by that run): arms the census named within the final gap on the
    /// first pass of the run (the lip-seat rule's own re-landings excluded),
    /// and on the last pass's field.
    pub clearance_violators_first: usize,
    pub clearance_violators_last: usize,
    /// Of the arms `kept_carrying` kept (on the last pass): those with a
    /// cross tied to them, those with a member that is not an arm (a brace, a
    /// mast, a tie) resting on them or on one of their descendants, and those
    /// held back by a member resting on them that was sent itself.
    pub kept_locked_by_tie: usize,
    pub kept_locked_by_non_arm_below: usize,
    pub kept_behind_a_follower_also_sent: usize,
    /// **Named, with the reason** (challenger round 3, 2026-10-05): every arm
    /// the clearance re-route was sent and kept where it stands (on the last
    /// pass) because a cross, a brace, a mast or a tie belongs to the frame
    /// it carries ([`FrameLock`]). Each is a WARNING on the emitted file: the
    /// re-route does not move the junction of a frame the slenderness ladder
    /// solved and does not re-plan that frame.
    pub kept_for_a_frame: Vec<(u32, FrameLock)>,
    /// Remedy 1: arms re-landed so they meet their face within the limit.
    pub arms_relanded: usize,
    /// Of those, the ones re-landed on the MODEL below them rather than on a
    /// member or the plate (fix/morel-long-posts, 2026-10-02): the path hits
    /// of the model-landing candidate.
    pub relanded_on_model: usize,
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
    /// Refused because the lean's straight run is longer than the slenderness
    /// ladder's thicken rung can hold with the thickened solid keeping the
    /// final gap, while the arm as it stands is a support (Q1, 2026-10-10;
    /// q12 regressions, 2026-10-12).
    pub candidates_refused_slender: usize,
    /// Of the arms kept with no admissible descent, those that had a lean
    /// refused for its slenderness (they keep the seat-bite WARNING).
    pub kept_for_slenderness: usize,
    /// The arms that had a lean refused for its slenderness (each once), the
    /// last pass's list.
    pub kept_for_slenderness_ids: Vec<u32>,
    /// Leans the thicken rung cannot hold that were TAKEN anyway, because the
    /// arm as it stands is no support (a one-layer stub, or a solid inside the
    /// model): nothing better to keep (q12 regressions, 2026-10-12). The path
    /// hit of that test.
    pub slender_taken_kept_not_clear: usize,
    /// Moved members (a mover, a follower) whose solid was NOT read against
    /// the final gap because it stands in a hidden cavity (Craig ruling
    /// 2026-10-05): the path hit of that ruling in the landing search.
    pub candidates_hidden: usize,
    /// Refused because a new plate root's flared disc would touch another
    /// root's (the congestion lane's own clearance rule).
    pub candidates_refused_roots: usize,
    /// Whether the model surface could not be built, so the solid clearance
    /// went unmeasured (a WARNING; the rule still ran on the centreline test).
    pub clearance_unmeasured: bool,
    pub candidates_verified: usize,
    /// Re-landings put back once the field was SIZED ([`settle`]): the host
    /// chain that took the arm's load came out overloaded where it was not
    /// (a member past `X_ALLOWED` at the maximum radius), or the plate root
    /// at the foot of that chain came to overlap a root it did not overlap.
    /// Each descent put back is not offered again.
    pub moves_put_back_host_load: usize,
    pub moves_put_back_roots: usize,
    /// Arms the load insertion of a sizing that put a re-landing back had
    /// added, withdrawn with it ([`put_back_all`]); and of those, the ones
    /// that had landed on a mover put back or on a member that moved with it
    /// (each would have stood on air). The path hits of the withdrawal.
    pub inserted_arms_withdrawn: usize,
    pub inserted_on_a_put_back_move: usize,
    /// The path hits of [`blame`]'s attribution (fix/lip-seat-blame,
    /// 2026-10-01), summed over the sized checks that put a move back: a
    /// member newly overloaded (or a root newly lapping) that a move's new
    /// chain reaches but its old chain already reached, so the move added
    /// no load to it and is not blamed for it (R1); and moves that did add
    /// to a broken member but were left standing because a longer move that
    /// broke it was put back first (R2).
    pub blame_spared_already_carried: usize,
    pub blame_held_behind_longer: usize,
    /// Sized-field checks run (one per re-sizing): the path hits of
    /// [`settle`].
    pub sizing_checks: usize,
    /// On the last check of the last pass: members newly overloaded, and
    /// root pairs newly overlapping, against the field before the run's first
    /// move, that no re-landing still standing fed (a WARNING quantity; the
    /// rule cannot name a cause it did not make). Until 2026-10-02 this read
    /// "no re-landing of that pass", against the field the previous pass left.
    pub new_overloads_unattributed: usize,
    pub new_root_pairs_unattributed: usize,
    /// **Across passes** (fix/creep-45 round 11, 2026-10-02): every sized
    /// check judges every move still standing, from every pass of the run,
    /// against the field before the run's first move. A move an EARLIER pass
    /// took cannot be put back in place (later moves, and the arms later
    /// sizings inserted, may rest on its moved shaft), so its descent is
    /// refused and the run starts again from that field. The moves so
    /// refused, the re-runs, and the runs withdrawn whole because the re-runs
    /// reached the registered pass count (a WARNING: the field is left as it
    /// stood before that run). Kept across re-runs; every other count
    /// describes the run that stands.
    pub moves_refused_across_passes: usize,
    pub reruns: usize,
    pub runs_withdrawn: usize,
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
    /// Arms the trigger sent (bite or clearance) that stay where they are
    /// because they are HEADED, or because a member that would have to move
    /// with them is. A head is frozen at placement; on the last pass.
    pub headed_kept: usize,
    /// HEADED arms whose shaft the emitted field's seat trim still cuts. A
    /// head's post starts at the joint, clear of the seat plane: must read 0.
    pub headed_bites: usize,
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
    let arms = live_arms(mesh, context, layer_plan, plate_z_mm, struts);
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
    /// The cheapest descent that passed every other test was refused because
    /// the lean cannot be held ([`apply`]); the arm keeps its seat.
    refused_slender: bool,
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
#[expect(
    clippy::too_many_arguments,
    reason = "the field and its index, the mover's new axis, and the members that follow it"
)]
fn children_follow(
    context: &SlendernessContext<'_>,
    index: &ColumnHistory,
    struts: &[SweepStrut],
    mover: usize,
    k_bottom: usize,
    path: &[u32],
    children: &[usize],
    transitive: bool,
) -> Option<Vec<Follow>> {
    // Each host that moved, with the base layer and path its axis now has:
    // the mover first. With `transitive` the list holds every descendant of
    // the mover, parents before children, and a descendant lands on its OWN
    // host's moved axis (the clearance re-route moves a whole subtree).
    let mut moved: Vec<(usize, usize, Vec<u32>)> = vec![(mover, k_bottom, path.to_vec())];
    let mut out = Vec::with_capacity(children.len());
    for &child in children {
        let strut = &struts[child];
        let Landing::Strut { id: host_id, layer } = strut.landing else {
            return None;
        };
        let (host, host_k_bottom, host_path) = moved
            .iter()
            .find(|(member, _, _)| *member == host_id as usize)
            .map(|(member, k_bottom, path)| (&struts[*member], *k_bottom, path.as_slice()))?;
        if layer < host_k_bottom || layer > host.k_top {
            return None;
        }
        let cell = host_path
            .get(layer - host_k_bottom)
            .map_or(host.cell, |cell| *cell as usize);
        let lateral_mm = crate::cell_distance_mm(context.grid, strut.cell, cell);
        context
            .lean
            .descent_cost_mm(strut.k_top.checked_sub(layer)?, lateral_mm)?;
        // The child may pass through the host it lands on and through its own
        // old shaft, which the index still holds (and, moving a subtree,
        // through the shafts of the members that move with it).
        let mut except: Vec<u32> = vec![host.id, strut.id];
        if transitive {
            except.extend(children.iter().map(|&other| struts[other].id));
        }
        let new_path = crate::slenderness::segment_path_excluding(
            context,
            index,
            strut.cell,
            strut.k_top,
            cell,
            layer,
            &except,
        )?;
        if transitive {
            moved.push((child, strut.k_bottom, new_path.clone()));
        }
        out.push((child, cell, new_path));
    }
    Some(out)
}

/// One axis a landing-search proposal would draw: from `k_bottom` to `k_top`,
/// standing in `path[layer - k_bottom]` (the seat's cell past the path's
/// end, as [`SweepStrut::cell_at`] reads it), the foot in `base_cell` where
/// given, with a solid of `radius_mm` about it.
#[derive(Clone, Copy, Debug)]
struct ProposedAxis<'a> {
    k_bottom: usize,
    k_top: usize,
    base_cell: Option<usize>,
    seat_cell: usize,
    path: &'a [u32],
    radius_mm: f64,
}

/// Which members of one proposal the final gap does not apply to, the mover
/// first, then its children in order: a member whose PROPOSED axis, with the
/// solid it would be drawn at, stands wholly in hidden voxels; a child only
/// when the mover it follows does too. Each is asked with the geometry the
/// trial draws, never the one it stands on now (challenger round 3,
/// 2026-10-05). The children are asked only when the mover is hidden.
fn proposal_hidden<'a>(
    hidden: &crate::cavity::HiddenCavity,
    mover: ProposedAxis<'a>,
    children: impl Iterator<Item = ProposedAxis<'a>>,
) -> Vec<bool> {
    let on = |axis: &ProposedAxis<'_>| {
        hidden.span_hidden(
            axis.k_bottom,
            axis.k_top,
            |layer| match axis.base_cell {
                Some(base) if layer == axis.k_bottom => base,
                _ => axis
                    .path
                    .get(layer.wrapping_sub(axis.k_bottom))
                    .map_or(axis.seat_cell, |cell| *cell as usize),
            },
            |_| axis.radius_mm,
        )
    };
    let mover_hidden = on(&mover);
    std::iter::once(mover_hidden)
        .chain(children.map(|child| mover_hidden && on(&child)))
        .collect()
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
/// measures as keeping the shaft's section. A descent in `refused` (an arm
/// id and a base cell the sized field already refused, [`settle`]) is not
/// offered again. Returns the moves taken, so the caller can re-size the
/// field, [`settle`] it, and call again.
#[expect(
    clippy::too_many_lines,
    clippy::too_many_arguments,
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
    trigger: Trigger<'_>,
    refused: &[(u32, usize)],
    outcome: &mut LipSeatOutcome,
) -> Vec<Relanding> {
    let _perf_span = crate::perf::span(crate::perf::Span::LipApply);
    let context = joins.slender;
    let surface = joins.surface();
    let tier = joins.clearance_mm;
    // The raft top a re-landing on a member keeps its base disc above, as the
    // head-join rule's joins do (`head_join::stays_above_raft`).
    let floor_z_mm = crate::head_join::join_floor_z_mm(plate_z_mm, context.layer_height_mm);
    if surface.is_none() {
        outcome.clearance_unmeasured = true;
    }
    let mut index = ColumnHistory::new(context.grid);
    crate::congestion::stamp_all(&mut index, struts, context.layer_height_mm);
    // The roots on the plate, as the congestion census sizes them, on the
    // profiles the survey drew.
    let flare_factor = if context.flare_factor.is_finite() && context.flare_factor > 0.0 {
        context.flare_factor
    } else {
        1.0
    };
    let profiles: Vec<Vec<f64>> = arms
        .iter()
        .map(|arm| arm.radius_profile_mm.clone())
        .collect();
    let mut roots = crate::congestion::root_discs_drawn(
        context.grid,
        context.history,
        struts,
        &profiles,
        flare_factor,
    );
    outcome.kept_carrying = 0;
    outcome.headed_kept = 0;
    outcome.kept_no_candidate = 0;
    outcome.kept_for_slenderness = 0;
    outcome.kept_for_slenderness_ids.clear();
    outcome.slender_taken_kept_not_clear = 0;
    outcome.kept_locked_by_tie = 0;
    outcome.kept_locked_by_non_arm_below = 0;
    outcome.kept_behind_a_follower_also_sent = 0;
    outcome.kept_for_a_frame.clear();

    // ---- who may move. An arm another member rests on moves only WITH its
    // children, and only when each child is a support arm nothing rests on and
    // no cross is tied to the arm (the follow is then one re-landing per child
    // on the moved axis, at the child's own landing layer). Anything else is
    // locked for the pass: moving it would move the ground under members this
    // pass cannot re-land.
    let carries = crate::landing_check::carries_someone(struts);
    let mut children_of: Vec<Vec<usize>> = vec![Vec::new(); struts.len()];
    for (child, strut) in struts.iter().enumerate() {
        if let Landing::Strut { id, .. } = strut.landing {
            if let Some(list) = children_of.get_mut(id as usize) {
                list.push(child);
            }
        }
    }
    let tied = crosses_seated(struts);
    // **The clearance re-route moves a whole SUBTREE.** An arm sent for its
    // clearance that carries members which carry others (the overhang
    // table's post 26: three branches, one with two branches of its own) is
    // the ordinary case of a thick, heavily loaded trunk, and it is the one
    // the area law grew into the model. It moves with every arm that rests on
    // it, directly or not (each lands on its own host's moved axis, parents
    // before children); it stays locked only where a member that is not an
    // arm (a brace, a mast) rests on it somewhere below, or a cross is tied
    // to it or to one of them, since this pass cannot re-land those.
    let followers_of: Vec<Vec<usize>> = if trigger.is_clearance() {
        (0..struts.len())
            .map(|member| {
                let mut out: Vec<usize> = Vec::new();
                let mut stack: Vec<usize> = children_of[member].iter().rev().copied().collect();
                while let Some(next) = stack.pop() {
                    if out.len() > struts.len() {
                        break;
                    }
                    out.push(next);
                    stack.extend(children_of[next].iter().rev().copied());
                }
                out
            })
            .collect()
    } else {
        children_of.clone()
    };
    //
    // **The arm a twin seat was merged into is never re-landed here**
    // (`SweepStrut::twin_host`, quality lane Q2, 2026-10-10). This pass leans
    // a WHOLE arm along one axis to the plate when its tip meets a steep face
    // (`DEINO-DIAGNOSIS.md`, cause 3): that is the 30-46 degree post the merge
    // exists to remove. The merged arm keeps the straight post (and head) the
    // merge gave it, and a bite it cannot lose is a seat-trim WARNING, as for
    // every arm this pass leaves. Measured on the Deinonychus: with the host
    // free to lean, four ridge posts that stood vertical before the merge
    // leaned 33-46 degrees over a free vertical landing; locked, none does.
    let mut locked: Vec<bool> = (0..struts.len())
        .map(|member| {
            struts[member].twin_host
                || carries[member]
                && (tied[member]
                    || if trigger.is_clearance() {
                        followers_of[member].iter().any(|&follower| {
                            tied[follower]
                                || struts[follower].kind != StrutKind::Arm
                                || struts[follower].head.is_some()
                        })
                    } else {
                        children_of[member].iter().any(|&child| {
                            carries[child]
                                || struts[child].kind != StrutKind::Arm
                                || struts[child].head.is_some()
                        })
                    })
        })
        .collect();
    let to_move: Vec<bool> = struts
        .iter()
        .zip(bites)
        .enumerate()
        .map(|(index_of, (strut, bite))| {
            trigger.sends(index_of, bite.as_ref())
                && strut.kind == StrutKind::Arm
                // A HEADED arm is never re-landed (its head is frozen).
                && strut.head.is_none()
                && strut.k_bottom != usize::MAX
                && !locked[index_of]
        })
        .collect();

    // ---- stage 1: price every candidate descent of every arm sent.
    let mut pending: Vec<Pending> = Vec::new();
    for (index_of, (strut, bite)) in struts.iter().zip(bites).enumerate() {
        if !trigger.sends(index_of, bite.as_ref())
            || strut.kind != StrutKind::Arm
            || strut.k_bottom == usize::MAX
        {
            continue;
        }
        // A HEADED arm is sent but never re-landed: its head is frozen at
        // placement. Counted, and left where it is.
        if strut.head.is_some() {
            outcome.headed_kept += 1;
            continue;
        }
        // A child that is itself over its limit is re-landed first; the
        // parent is taken on the next pass, with whatever still rests on it.
        let children = followers_of[index_of].clone();
        if locked[index_of] || children.iter().any(|&child| to_move[child]) {
            outcome.kept_carrying += 1;
            // A member that would have to move with it is HEADED: never moved.
            if children.iter().any(|&child| struts[child].head.is_some()) {
                outcome.headed_kept += 1;
            }
            // Why, for the report: a cross tied to it, a member that is not an
            // arm (a brace, a mast) resting on it or below it, an arm that
            // carries others (the lip-seat rule's own lock), or a member that
            // rests on it being sent itself (it goes first, this arm next pass).
            if tied[index_of] {
                outcome.kept_locked_by_tie += 1;
            } else if children.iter().any(|&child| tied[child] || struts[child].kind != StrutKind::Arm) {
                outcome.kept_locked_by_non_arm_below += 1;
            } else if !locked[index_of] {
                outcome.kept_behind_a_follower_also_sent += 1;
            }
            // Named, with its reason: kept for a frame is a WARNING.
            if trigger.is_clearance() && locked[index_of] {
                if let Some(lock) = frame_lock(struts, &tied, index_of, &children) {
                    outcome.kept_for_a_frame.push((strut.id, lock));
                }
            }
            continue;
        }
        let Some(arm) = arms.get(index_of) else {
            continue;
        };
        let Some(normal) = geom_kernel::vec3::normalize(arm.seat_normal_mm) else {
            outcome.kept_no_candidate += 1;
            continue;
        };
        let mut candidates = price(
            context,
            &index,
            struts,
            index_of,
            arm.seat_mm,
            normal,
            // An arm sent for clearance that has no measured bite (its seat
            // trim is not an issue) keeps the whole half space: the angle the
            // lowering measures below is what decides.
            bite.as_ref().map_or(90.0, |bite| bite.limit_deg),
            &to_move,
            &children,
            trigger.is_clearance(),
            outcome,
        );
        let id = strut.id;
        if refused.iter().any(|(arm, _)| *arm == id) {
            candidates.retain(|candidate| !refused.contains(&(id, candidate.cell)));
        }
        pending.push(Pending {
            index: index_of,
            candidates,
            next: 0,
            children,
            refused_slender: false,
        });
    }

    // ---- stages 2 and 3, in rounds: each pending arm's next candidate whose
    // path clears, then all of them measured by the lowering in one call.
    let mut moved: Vec<Relanding> = Vec::new();
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
                match propose(
                    context,
                    &index,
                    struts,
                    job.index,
                    candidate,
                    &job.children,
                    trigger.is_clearance(),
                ) {
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
        // lander (the kernel's clearance, its own seat zone untested, and its
        // own landing zone when it lands on the model), fix/morel-long-posts
        // 2026-10-02:
        //
        // * A mover that comes to stand on a NEW ROOT, the plate or the
        //   model, clears the final gap outright: its whole drawn shaft is
        //   its own, read on the line the file draws (a plate member's to its
        //   boss top over its foot), and the slenderness lane may yet thicken
        //   it. Read only as "no nearer than it stood", the Morel's rim arm
        //   120 went to the plate and ended 0.21 mm INTO the base rim: it had
        //   stood with its shaft cut along the cap's underside (a cut the
        //   seat trim removes in the file), so its "stood" was already
        //   negative.
        // * A mover joined to a member, and every child that follows it,
        //   never enters the model and comes no nearer it than the final gap
        //   unless it already stood nearer: `now ≥ min(gap, max(was, 0))`. A
        //   junction under a host's head stands near the host's own seat by
        //   construction (the head-join rule warns it); demanding the full
        //   gap there sent the stilt plate's arm 18 9.6 mm sideways past its
        //   near post (a 13.5 mm branch, the owner's 2026-10-01 finding).
        // * Both readings exempt the member's own landing zone. Until then a
        //   member standing on the model read its own strike, about
        //   −0.05 mm, which licensed a path 0.185 mm INTO the Morel base's
        //   rim (arm 127, for-craig-v10), while every model landing a move
        //   proposed read the same strike and was refused.
        //
        // One batch for the round, and one for the same members as they
        // stand.
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
                // Which of them land on the model: the mover as proposed (its
                // children land on it), and each as it stands now.
                let trial_on_model: Vec<bool> = proposals
                    .iter()
                    .flat_map(|proposal| {
                        std::iter::once(matches!(
                            proposal.candidate.landing,
                            Landing::Model { .. }
                        ))
                        .chain(proposal.children.iter().map(|_| false))
                    })
                    .collect();
                let standing_on_model: Vec<bool> = movers
                    .iter()
                    .map(|&member| matches!(struts[member].landing, Landing::Model { .. }))
                    .collect();
                (
                    solid_clearances(
                        surface,
                        &trial,
                        &trial_on_model,
                        layer_plan,
                        plate_z_mm,
                        tier,
                    ),
                    solid_clearances(
                        surface,
                        &standing,
                        &standing_on_model,
                        layer_plan,
                        plate_z_mm,
                        tier,
                    ),
                )
            }
            None => (vec![None; trial.len()], vec![None; trial.len()]),
        };
        for (proposal, &first) in proposals.into_iter().zip(&first_of) {
            let index_of = pending[proposal.slot].index;
            // A mover that stands on a NEW root (the plate or the model)
            // must clear the gap outright; a mover joined to a member, and
            // every child (each joined to the mover), no nearer than it
            // stood, and never inside.
            let new_root = !matches!(proposal.candidate.landing, Landing::Strut { .. });
            // The final gap does not apply inside a hidden cavity (Craig
            // ruling 2026-10-05): a mover whose PROPOSED axis, from the
            // landing it would make up to its own seat, stands wholly in
            // enclosed air ([`crate::cavity`]; whatever it would land on),
            // is not read against the gap, and nor is a child whose proposed
            // axis would stand in one and follows a mover that does. A proposal
            // outside a cavity is read as before, so a recess open to the
            // outside is never exempt.
            //
            // Every member is asked with the geometry the trial draws: the
            // mover on its proposed landing and path, each child on its
            // PROPOSED path onto the moved axis, not the path it stands on
            // (challenger round 3, 2026-10-05: a child moved beside another
            // wall can carry its solid out of the hidden voxels while its old
            // axis stays inside), each with the SOLID its trial arm is drawn
            // at (round 2).
            let hidden_slots = proposal_hidden(
                &context.history.hidden,
                ProposedAxis {
                    k_bottom: proposal.candidate.k_bottom,
                    k_top: struts[index_of].k_top,
                    base_cell: Some(proposal.candidate.cell),
                    seat_cell: struts[index_of].cell,
                    path: &proposal.path,
                    radius_mm: crate::cavity::arm_radius_mm(&trial[first]),
                },
                proposal.children.iter().enumerate().map(|(offset, (child, _, path))| ProposedAxis {
                    k_bottom: struts[*child].k_bottom,
                    k_top: struts[*child].k_top,
                    base_cell: None,
                    seat_cell: struts[*child].cell,
                    path,
                    radius_mm: crate::cavity::arm_radius_mm(&trial[first + 1 + offset]),
                }),
            );
            let hidden = |slot: usize| hidden_slots[slot - first];
            let clear = surface.is_none()
                || (first..=first + proposal.children.len()).all(|slot| {
                    if hidden(slot) {
                        outcome.candidates_hidden += 1;
                        return true;
                    }
                    let minimum = if slot == first && (new_root || trigger.is_clearance()) {
                        tier
                    } else {
                        tier.min(before[slot].unwrap_or(tier).max(0.0))
                    };
                    after[slot].is_some_and(|now| now >= minimum - CLEARANCE_SLACK_MM)
                });
            if !clear {
                outcome.candidates_refused_clearance += 1;
                continue;
            }
            // The mover keeps its section; each child is no worse than it was.
            let mover_keeps = measured[first].as_ref().map_or(trigger.is_clearance(), |bite| !over_limit(bite));
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
                        measured[first + 1 + offset].as_ref().map_or(trigger.is_clearance(), |bite| {
                            bite.bite_mm <= before.max(0.0) + BITE_SLACK_MM
                        })
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
                    crate::head_join::stays_above_raft(arm.seat_mm, arm.base_mm, root, floor_z_mm)
                }
            });
            if !above_plate {
                outcome.candidates_refused_plate += 1;
                continue;
            }
            // A NEW PLATE ROOT keeps clear of every root already on the
            // plate: its flared disc, sized as the congestion census sizes
            // one (`congestion::root_discs_drawn`), may not touch another's.
            let flared = matches!(proposal.candidate.landing, Landing::Plate).then(|| {
                let arm = &trial[first];
                let bare =
                    plinth_support::drawn_root_radius_mm(&arm.radius_profile_mm, arm.tip_radius_mm);
                (bare * flare_factor, bare)
            });
            if let Some((radius, _)) = flared {
                let own = struts[index_of].id;
                let skip = roots.iter().position(|disc| disc.id() == own);
                if !crate::congestion::clear_of_placed(
                    context.grid,
                    proposal.candidate.cell,
                    radius,
                    &roots,
                    skip,
                ) {
                    outcome.candidates_refused_roots += 1;
                    continue;
                }
            }
            if !mover_keeps || !children_hold {
                outcome.candidates_refused_measured += 1;
                continue;
            }
            // A child that something landed on this pass is no longer a leaf,
            // so its host may not take it along: the member resting on it would
            // be left standing where the child used to be (measured on the
            // Cylinder at fix/creep-45 `03428ce`: arm 182 re-landed on arm 198's
            // shaft at layer 629, then arm 196 moved and took its child 198 with
            // it, and 182 stood on air). `locked` is set on every new host and
            // every child already moved, so one test covers both.
            if locked[index_of] || proposal.children.iter().any(|(child, _, _)| locked[*child]) {
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
                trigger.is_clearance(),
            );
            let Some((path, children)) = again.filter(|(path, children)| {
                *path == proposal.path && *children == proposal.children
            }) else {
                outcome.candidates_refused_path += 1;
                continue;
            };
            // **A LEAN THE SLENDERNESS LAW CANNOT HOLD IS NOT TAKEN** (Q1,
            // 2026-10-10, `DEINO-DIAGNOSIS.md` fix 1). The re-landing leans the
            // WHOLE arm along one straight axis. Where that axis runs within
            // the final gap of the model, the slenderness ladder cannot
            // thicken it (the thicker solid would enter the gap) and the arm
            // ends as an `unbraced_run_over_limit` warning on a long thin rod
            // (Deinonychus arms 215 and 220: runs 3.3x and 5.3x their limit).
            // The test is the thicken rung's own: the root that rung's taper
            // (tip kept) needs to hold the run, and whether that frustum
            // keeps the gap ([`lean_too_slender`]). The run is the chord the
            // ladder measures (`slenderness::run_length`: the drop and the
            // lateral move between the seat and base cells) and its limit is
            // the existing `S * r`.
            //
            // **AND ONLY WHERE THE ARM AS IT STANDS IS A SUPPORT** (q12
            // regressions, 2026-10-12). The refusal trades the lean for the arm
            // already there, so that arm must be one: a shaft longer than one
            // layer, whose solid does not enter the model (`before[first]`,
            // measured above in the same batch; unmeasured is not proof of
            // entry). The Morel base's arm 44 stood as a stub one layer long
            // onto the model wall beside its seat, its tip off its face; the
            // brace pair's arm 3 stood 0.149 mm INSIDE the model, and the
            // clearance re-route took the very lean refused here after the
            // ladder had planned the field without it, so its two masts and
            // both thickened post roots were lost. There the lean is taken as
            // before Q1 and left to the ladder.
            //
            // This is the LAST test, after the plate, root, measured-section,
            // lock and live-path tests above: a descent one of those refuses
            // is not admissible and never ends the search. The cheapest
            // ADMISSIBLE descent is the lean the rule would take; if its run
            // exceeds the limit at that radius the rule takes none, and the
            // arm keeps its seat and today's seat-bite WARNING. (Walking on to
            // dearer admissible descents would cost one kernel clearance read
            // each over lists tens of thousands long: 8 times slower on
            // Deinonychus.) No new constant: `S`, the gap and the maximum
            // radius are the law's own. The clearance re-route is not asked
            // (it was sent for the gap itself), and a mover in a hidden
            // cavity, where the gap does not apply, is not asked either.
            let too_slender = !trigger.is_clearance()
                && !hidden(first)
                && after[first].is_some_and(|now| {
                    lean_too_slender(
                        context,
                        surface,
                        &struts[index_of],
                        &proposal.candidate,
                        &trial[first],
                        now,
                        (layer_plan, plate_z_mm, tier),
                    )
                });
            if too_slender {
                let standing = &struts[index_of];
                let kept_is_a_support = before[first].is_none_or(|was| was >= -CLEARANCE_SLACK_MM)
                    && standing.k_top > standing.k_bottom.saturating_add(1);
                if kept_is_a_support {
                    outcome.candidates_refused_slender += 1;
                    // The cheapest admissible descent cannot be held: the arm
                    // keeps its seat and does not walk the rest of its
                    // candidates.
                    let job = &mut pending[proposal.slot];
                    job.refused_slender = true;
                    job.next = job.candidates.len();
                    let id = struts[index_of].id;
                    if !outcome.kept_for_slenderness_ids.contains(&id) {
                        outcome.kept_for_slenderness_ids.push(id);
                    }
                    continue;
                }
                // The path hit of the kept-arm test: a lean the law cannot
                // hold, taken because the arm as it stands is no better.
                outcome.slender_taken_kept_not_clear += 1;
            }
            outcome.candidates_verified += 1;
            let strut = &mut struts[index_of];
            let was = (
                strut.landing,
                strut.k_bottom,
                std::mem::take(&mut strut.path),
            );
            strut.landing = proposal.candidate.landing;
            strut.k_bottom = proposal.candidate.k_bottom;
            strut.path = path;
            let id = strut.id;
            let mut record = Relanding {
                index: index_of,
                id,
                cell: proposal.candidate.cell,
                cost_mm: proposal.candidate.cost_mm,
                was,
                children_were: Vec::with_capacity(children.len()),
            };
            index.stamp_strut(strut, context.layer_height_mm);
            roots.retain(|disc| disc.id() != id);
            if let Some((radius, bare)) = flared {
                let cell = proposal.candidate.cell;
                let under = context
                    .history
                    .column_max_model
                    .get(cell)
                    .is_some_and(|top| *top != 0);
                crate::congestion::push_disc(
                    &mut roots,
                    context.grid,
                    id,
                    cell,
                    radius,
                    bare,
                    under,
                );
            }
            for (child, _, child_path) in children {
                let strut = &mut struts[child];
                record
                    .children_were
                    .push((child, std::mem::replace(&mut strut.path, child_path)));
                index.stamp_strut(strut, context.layer_height_mm);
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
            if matches!(proposal.candidate.landing, Landing::Model { .. }) {
                outcome.relanded_on_model += 1;
            }
            moved.push(record);
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
            if job.refused_slender {
                outcome.kept_for_slenderness += 1;
            }
        }
    }
    moved
}

/// One re-landing [`apply`] took, with what it replaced, so [`settle`] can
/// put it back.
pub(crate) struct Relanding {
    index: usize,
    id: u32,
    /// The base cell of the descent taken (what a put-back refuses again).
    cell: usize,
    /// The lean law's price of the descent taken (`LeanLaw::descent_cost_mm`,
    /// its weighted length): the measure that chose it as the cheapest, and
    /// the order in which [`blame`] puts offenders back, longest first.
    cost_mm: f64,
    /// The arm's landing, base layer and path before the move.
    was: (Landing, usize, Vec<u32>),
    /// Each child moved with it, and the child's path before.
    children_were: Vec<(usize, Vec<u32>)>,
}

/// What a SIZED field measures that a re-landing can break: each overloaded
/// member's largest carried load (`X_ALLOWED` at the maximum radius exceeded,
/// the down-sweep's own test, `down_sweep::DownSweepOutcome::overloaded`),
/// and every overlapping pair of plate roots as drawn (the congestion
/// census's own discs and sweep line).
pub(crate) struct SizedField {
    overloaded: Vec<(u32, f64)>,
    root_pairs: Vec<(u32, u32)>,
}

/// [`SizedField`] of `struts`, just sized by the down-sweep that returned
/// `sized`. Batch-first: one pass over the overload list, one root census.
pub(crate) fn measure_sized(
    context: &SlendernessContext<'_>,
    struts: &[SweepStrut],
    sized: &crate::down_sweep::DownSweepOutcome,
) -> SizedField {
    let _perf_span = crate::perf::span(crate::perf::Span::LipMeasure);
    let mut overloaded: Vec<(u32, f64)> = Vec::new();
    for row in &sized.overloaded {
        match overloaded.iter_mut().find(|(id, _)| *id == row.strut_id) {
            Some((_, carried)) => *carried = carried.max(row.carried_mm2),
            None => overloaded.push((row.strut_id, row.carried_mm2)),
        }
    }
    overloaded.sort_unstable_by_key(|(id, _)| *id);
    let discs =
        crate::congestion::root_discs(context.grid, context.history, struts, context.flare_factor);
    SizedField {
        overloaded,
        root_pairs: crate::congestion::overlap_pair_ids(&discs),
    }
}

/// Float noise on "carries no more than it did", mm²: the down-sweep's own
/// sum over the same ledger read twice, and nothing more.
const LOAD_SLACK_MM2: f64 = 1.0e-9;

/// Why [`settle`] put a re-landing back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PutBack {
    HostLoad,
    Roots,
}

/// What [`blame`] decided on one sized check: the moves to put back now (by
/// slot in `moves`, ascending, each with why), and the path hits of the
/// attribution, which [`put_back_all`] books on the outcome.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Blamed {
    moves: Vec<(usize, PutBack)>,
    /// (move, broken member or root pair) pairs the move's new chain reaches
    /// but its old chain already reached: not blamed (R1).
    spared_already_carried: usize,
    /// Moves that added to a broken member or root pair, left standing
    /// because a longer move that broke the same ones goes back first (R2).
    held_behind_longer: usize,
}

impl Blamed {
    /// Whether nothing is to be put back.
    pub(crate) fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    /// Whether a move to put back stands at a slot below `pass_from`: the
    /// standing list holds every pass's moves in order, so such a move was
    /// taken by an earlier pass than the check's own.
    pub(crate) fn reaches_before(&self, pass_from: usize) -> bool {
        self.moves.iter().any(|(slot, _)| *slot < pass_from)
    }
}

/// **Refuse every blamed descent for a re-run of the whole run** (fix/creep-45
/// round 11, 2026-10-02). Called when [`Blamed::reaches_before`] holds: a move
/// of an earlier pass offends a break, and only starting again from the field
/// before the run's first move can take it back without leaving a later
/// member on air. Every blamed move's descent (arm id, base cell) is refused
/// from now on, as a put-back refuses it. Returns how many of them an earlier
/// pass took.
pub(crate) fn refuse_for_rerun(
    moves: &[Relanding],
    blamed: &Blamed,
    pass_from: usize,
    refused: &mut Vec<(u32, usize)>,
) -> usize {
    let mut earlier = 0;
    for &(slot, _) in &blamed.moves {
        let Some(relanding) = moves.get(slot) else {
            continue;
        };
        refused.push((relanding.id, relanding.cell));
        if slot < pass_from {
            earlier += 1;
        }
    }
    earlier
}

/// The members `start` hands its load to, `start` first, down its landings
/// to its foot; and whether that foot stands on the plate. Bounded by the
/// strut count, so a malformed cycle cannot spin.
fn chain_of(struts: &[SweepStrut], start: usize, chain: &mut Vec<u32>) -> bool {
    chain.clear();
    let mut at = start;
    for _ in 0..=struts.len() {
        let Some(member) = struts.get(at) else { break };
        chain.push(member.id);
        match member.landing {
            Landing::Strut { id, .. } => at = id as usize,
            Landing::Plate => return true,
            Landing::Model { .. } => break,
        }
    }
    false
}

/// **The sized field decides** (challenger, 2026-09-29), **and a move answers
/// only for the load it ADDED** (fix/lip-seat-blame, 2026-10-01).
///
/// A re-landing hands the arm's whole load to a new host chain, or makes the
/// arm a new plate root, and only the down-sweep that follows knows how thick
/// that makes the chain and its root. So after it, the members overloaded
/// now and not before (or carrying more past their limit than before), and
/// the plate-root pairs overlapping now and not before, are the field's
/// breaks, and a move is an offender of a break only where its move put load
/// there:
///
/// * **R1, attribution.** A move offends a broken member only when the member
///   is in its NEW chain (the arm, its host, down to the foot) and was not in
///   the chain under its `was` landing, read on the field as it stands (the
///   chain this move alone, put back, would hand the load to). A move DOWN
///   the host it already stood on added nothing to that host or anything
///   under it. The mover itself is in both chains, and still answers for its
///   own overload: whatever lands on it now lands on its moved shaft. A move
///   offends a new root pair only when the root at its new foot is one of the
///   pair and is a NEW foot for it: a different member from the old chain's
///   foot, or the mover itself standing on the plate (its root is where it
///   moved it).
/// * **R2, smallest put-back.** Where several moves offend a break, the
///   LONGEST offender (the lean law's price, `Relanding::cost_mm`, the
///   measure that chose each move) goes back first, alone; the others stand
///   until the field is sized again, and go back one by one, longest first,
///   only while the break remains. One put-back per break per check: a move
///   is put back when it offends a break no longer move of this check is
///   already being put back for.
///
/// No constant: the attribution is the chain, the order is the lean law's.
/// Before R1 and R2 (for-craig-v8, `247b8d5`) every move whose new chain held
/// an overloaded member went back, including two moves on the stilt plate
/// that only slid down their own near post; refused that descent for good,
/// they ended 9.6 mm away on a far post as 13.5 mm branches
/// (`stilt-and-brace-pair-diagnosis.md` item 1).
///
/// Returns what to put back now, plus the breaks no move added to (overloaded
/// members, root pairs): a WARNING quantity, the rule cannot put back a
/// cause it did not make.
pub(crate) fn blame(
    struts: &[SweepStrut],
    moves: &[Relanding],
    before: &SizedField,
    after: &SizedField,
) -> (Blamed, usize, usize) {
    let _perf_span = crate::perf::span(crate::perf::Span::LipBlame);
    let was = |id: u32| -> f64 {
        before
            .overloaded
            .binary_search_by_key(&id, |(member, _)| *member)
            .map_or(0.0, |slot| before.overloaded[slot].1)
    };
    let new_over: Vec<u32> = after
        .overloaded
        .iter()
        .filter(|(id, carried)| *carried > was(*id) + LOAD_SLACK_MM2)
        .map(|(id, _)| *id)
        .collect();
    let new_pairs: Vec<(u32, u32)> = after
        .root_pairs
        .iter()
        .filter(|pair| before.root_pairs.binary_search(pair).is_err())
        .copied()
        .collect();
    let mut verdict = Blamed::default();
    if new_over.is_empty() && new_pairs.is_empty() {
        return (verdict, 0, 0);
    }
    // Breaks are numbered: the new overloads first, then the new root pairs.
    let breaks = new_over.len() + new_pairs.len();
    let mut added = vec![false; breaks];
    // Per move: the breaks its move added to (R1).
    let mut offends: Vec<Vec<usize>> = vec![Vec::new(); moves.len()];
    let mut chain: Vec<u32> = Vec::new();
    let mut old: Vec<u32> = Vec::new();
    for (slot, relanding) in moves.iter().enumerate() {
        let foot_on_plate = chain_of(struts, relanding.index, &mut chain);
        // The chain under the `was` landing, on the field as it stands.
        let (old_on_plate, old_foot) = match relanding.was.0 {
            Landing::Strut { id, .. } => {
                let on_plate = chain_of(struts, id as usize, &mut old);
                (on_plate, old.last().copied())
            }
            Landing::Plate => {
                old.clear();
                (true, Some(relanding.id))
            }
            Landing::Model { .. } => {
                old.clear();
                (false, None)
            }
        };
        for (number, id) in new_over.iter().enumerate() {
            if !chain.contains(id) {
                continue;
            }
            if *id == relanding.id || !old.contains(id) {
                offends[slot].push(number);
            } else {
                verdict.spared_already_carried += 1;
            }
        }
        if let (true, Some(&foot)) = (foot_on_plate, chain.last()) {
            let new_foot = foot == relanding.id || !(old_on_plate && old_foot == Some(foot));
            for (offset, (a, b)) in new_pairs.iter().enumerate() {
                if *a != foot && *b != foot {
                    continue;
                }
                if new_foot {
                    offends[slot].push(new_over.len() + offset);
                } else {
                    verdict.spared_already_carried += 1;
                }
            }
        }
        for &number in &offends[slot] {
            added[number] = true;
        }
    }
    // R2: longest offender first; a move goes back only for a break no
    // longer move of this check already goes back for.
    let mut order: Vec<usize> = (0..moves.len())
        .filter(|slot| !offends[*slot].is_empty())
        .collect();
    order.sort_unstable_by(|a, b| {
        moves[*b]
            .cost_mm
            .total_cmp(&moves[*a].cost_mm)
            .then_with(|| b.cmp(a))
    });
    let mut relieved = vec![false; breaks];
    for slot in order {
        let list = &offends[slot];
        if list.iter().all(|number| relieved[*number]) {
            verdict.held_behind_longer += 1;
            continue;
        }
        let cause = if list.iter().any(|number| *number < new_over.len()) {
            PutBack::HostLoad
        } else {
            PutBack::Roots
        };
        for &number in list {
            relieved[number] = true;
        }
        verdict.moves.push((slot, cause));
    }
    // `put_back_all` removes from the highest slot down.
    verdict.moves.sort_unstable_by_key(|(slot, _)| *slot);
    let unexplained_over = added[..new_over.len()]
        .iter()
        .filter(|flag| !**flag)
        .count();
    let unexplained_pairs = added[new_over.len()..]
        .iter()
        .filter(|flag| !**flag)
        .count();
    (verdict, unexplained_over, unexplained_pairs)
}

/// **Put the blamed re-landings back, and withdraw the sizing that judged
/// them** (round5-review-fixes, 2026-09-29).
///
/// The field [`blame`] reads was sized by a down-sweep with load insertion
/// on, and that down-sweep can add an arm that grows down and lands on a
/// moved shaft (`down_sweep`, a growing arm's `Landing::Strut` on the member
/// standing in its column). A re-landing records only what IT replaced, so
/// putting the mover back left such an arm with a fixed landing where the
/// moved shaft used to be: standing on air, and the next down-sweep still
/// handed its load to the mover at that layer. The down-sweep never changes
/// the geometry of a member it did not add (it rewrites every ledger, and
/// appends what it inserts), so the whole of its side effect is the members
/// past `sized_from`, the strut count the moves left before it ran. They are
/// withdrawn with the moves, and the caller sizes again: insertion then
/// places what the field that actually stands needs.
pub(crate) fn put_back_all(
    struts: &mut Vec<SweepStrut>,
    sized_from: usize,
    moves: &mut Vec<Relanding>,
    blamed: &Blamed,
    refused: &mut Vec<(u32, usize)>,
    outcome: &mut LipSeatOutcome,
) {
    if blamed.is_empty() {
        return;
    }
    outcome.blame_spared_already_carried += blamed.spared_already_carried;
    outcome.blame_held_behind_longer += blamed.held_behind_longer;
    let blamed = blamed.moves.as_slice();
    if struts.len() > sized_from {
        // The members a put-back moves: each mover and the children that
        // moved with it. One flag vector, one pass over the added arms.
        let mut moved_back = vec![false; sized_from];
        for &(slot, _) in blamed {
            let Some(relanding) = moves.get(slot) else {
                continue;
            };
            for index in std::iter::once(relanding.index)
                .chain(relanding.children_were.iter().map(|(child, _)| *child))
            {
                if let Some(flag) = moved_back.get_mut(index) {
                    *flag = true;
                }
            }
        }
        for added in &struts[sized_from..] {
            outcome.inserted_arms_withdrawn += 1;
            if let Landing::Strut { id, .. } = added.landing {
                if moved_back.get(id as usize).copied().unwrap_or(false) {
                    outcome.inserted_on_a_put_back_move += 1;
                }
            }
        }
        struts.truncate(sized_from);
    }
    // Highest slot first, so each removal leaves the lower slots where
    // `blame` found them.
    for &(slot, cause) in blamed.iter().rev() {
        let relanding = moves.remove(slot);
        put_back(struts, relanding, cause, refused, outcome);
    }
}

/// Put one re-landing back as [`blame`] found it, refuse its descent from
/// now on, and correct the counts.
pub(crate) fn put_back(
    struts: &mut [SweepStrut],
    relanding: Relanding,
    cause: PutBack,
    refused: &mut Vec<(u32, usize)>,
    outcome: &mut LipSeatOutcome,
) {
    let Relanding {
        index,
        id,
        cell,
        cost_mm: _,
        was: (landing, k_bottom, path),
        children_were,
    } = relanding;
    let strut = &mut struts[index];
    if matches!(strut.landing, Landing::Model { .. }) {
        outcome.relanded_on_model = outcome.relanded_on_model.saturating_sub(1);
    }
    strut.landing = landing;
    strut.k_bottom = k_bottom;
    strut.path = path;
    for (child, path) in children_were {
        struts[child].path = path;
        outcome.children_moved_with_host = outcome.children_moved_with_host.saturating_sub(1);
    }
    refused.push((id, cell));
    if let Some(at) = outcome
        .relanded_ids
        .iter()
        .rposition(|member| *member == id)
    {
        outcome.relanded_ids.remove(at);
    }
    outcome.arms_relanded = outcome.arms_relanded.saturating_sub(1);
    match cause {
        PutBack::HostLoad => outcome.moves_put_back_host_load += 1,
        PutBack::Roots => outcome.moves_put_back_roots += 1,
    }
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
    transitive: bool,
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
        transitive,
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
    arms_only: bool,
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
    // The arm's own shaft and its children's are in the index; none of them
    // is anything this arm could land on.
    let mut own: Vec<u32> = Vec::with_capacity(children.len() + 1);
    own.push(strut.id);
    own.extend(children.iter().map(|&child| struts[child].id));
    let straight_layers = index
        .shaft_landing_excluding(strut.cell, k_top.saturating_sub(1), &own)
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
            // The column's highest landing at or below the ceiling, the arm's
            // own shaft and its children's aside (those rest on it: a cycle
            // in the load path). Its own column is the straight descent.
            //
            // THE MODEL IS A LANDING TOO (fix/morel-long-posts, 2026-10-02),
            // exactly as the up-sweep's own descent offers it
            // (`up_sweep::descend_leaning`): the highest of the column's
            // shaft and its cured run's top wins, an arm on a tie. Until
            // then only the shaft index was asked, so a column whose landing
            // was model material read as EMPTY and was offered as the plate:
            // on the Morel base (for-craig-v10) four arms under the mushroom
            // caps that had landed straight down on the base's top face were
            // re-landed 9.5-10.5 mm to the plate, leaning out past the rim.
            //
            // A column whose cured run stands THROUGH the ceiling offers
            // nothing: everything at or below the ceiling is inside it, and
            // the path to it would pass through the model above (the
            // up-sweep's own "occupied, but not at any layer this descent
            // can reach"). Until then such a column offered the plate, which
            // the path test then refused.
            if context.history.model_blocks(cell, ceiling)
                && context.history.model_blocks(cell, ceiling + 1)
            {
                continue;
            }
            let shaft = index.shaft_landing_excluding(cell, ceiling, &own);
            let model = context.history.model_landing_at_or_below(cell, ceiling);
            let (landing, k_bottom) = match (shaft, model) {
                (Some((_, at, _)), Some(model_at)) if model_at > at => {
                    (Landing::Model { layer: model_at }, model_at)
                }
                // **A RE-ROUTE LANDS ONLY ON AN ARM** (lane cylinder-bracing,
                // rule E, 2026-10-05). The column history holds the braces
                // too, so the shaft search can return a cross diagonal; a
                // diagonal is sized for drag (`r_b = l / S`), not for a load
                // path, and a branch re-landed on one (the Cylinder's 169 on
                // brace 362) made a new run over its limit. The head-join
                // rule already joins only to a host that is an arm (the host
                // test in `head_join`). The clearance re-route only:
                // the lip-seat rule's own search is unchanged.
                (Some((id, _, _)), _)
                    if arms_only
                        && struts
                            .get(id as usize)
                            .is_some_and(|host| host.kind != StrutKind::Arm) =>
                {
                    continue
                }
                // Not onto an arm this pass is about to re-land: landing on
                // it would freeze its own cut in place.
                (Some((id, at, _)), _) if !to_move.get(id as usize).copied().unwrap_or(false) => {
                    (Landing::Strut { id, layer: at }, at)
                }
                (Some(_), _) => continue,
                (None, Some(model_at)) => (Landing::Model { layer: model_at }, model_at),
                (None, None) => (Landing::Plate, 0),
            };
            #[expect(
                clippy::cast_precision_loss,
                reason = "a layer count bounded by the plan's own layer count is exact in f64"
            )]
            let drop_mm = (k_top - k_bottom) as f64 * height;
            if !approach_holds(cell, drop_mm) {
                continue;
            }
            let Some(length_mm) = law.descent_cost_mm(k_top - k_bottom, lateral_mm) else {
                continue;
            };
            // Priced as the up-sweep prices it: a model landing still owes
            // the path from its strike face to the plate, at least its own
            // height (`up_sweep::deferred_path_mm`, no tunable part). So a
            // model landing beats the plate exactly when its weighted
            // length plus its height is the shorter, which any reachable
            // landing under a leaning arm is: the same lean over fewer
            // layers costs less than over all of them.
            let cost_mm =
                length_mm + crate::up_sweep::deferred_path_mm(landing, k_bottom, height);
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
    layer_plan: &LayerPlan,
    plate_z_mm: f64,
    struts: &[SweepStrut],
) -> Vec<SweepArm> {
    let grid = context.grid;
    let height = context.layer_height_mm;
    let (mut faces, seat_triangles) = crate::tip_landing::tip_seat_faces_with_ids(
        mesh,
        grid,
        struts,
        plate_z_mm,
        height,
        context.seat_faces,
    );
    // The seat normal is the footprint's average, here as in the finished plan
    // (`footprint_normal`, Craig ruling 2026-10-05), so the bite this pass
    // measures is the bite the file gets.
    crate::footprint_normal::refine(
        &crate::footprint_normal::FootprintEnv {
            mesh,
            surface: context.model_surface,
            grid,
            plate_z_mm,
            layer_height_mm: height,
            layer_plan,
            rule: context.seat_faces,
        },
        struts,
        &mut faces,
        &seat_triangles,
    );
    let (mut profiles, _) = crate::adapter::drawn_profiles(struts, context.load_step_taper);
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

/// **A carrier kept for a frame is NAMED, with why** (challenger round 3,
/// 2026-10-05, finding 3): the Cylinder's arms 171, 195 and 203 stood 0.20 to
/// 0.24 mm from the model, each with a cross seated on it, and the re-route
/// only counted them (`kept_locked_by_tie 3`). Each is now a WARNING naming
/// the arm and the part of a frame that holds it.
#[cfg(test)]
mod frame_lock_tests {
    use super::{crosses_seated, frame_lock, FrameLock};
    use crate::{Landing, SeatFace, StrutKind, SweepStrut};

    fn member(id: u32, kind: StrutKind, landing: Landing, seat_on_strut: Option<u32>) -> SweepStrut {
        SweepStrut {
            id,
            cell: 0,
            center_mm: [0.0, 0.0],
            kind,
            min_radius_mm: 0.0,
            thickened_mm: Vec::new(),
            seat_on_strut,
            k_top: 100,
            k_bottom: 0,
            landing,
            interior: false,
            radius_mm: Vec::new(),
            carried_mm2: Vec::new(),
            covered_demand: Vec::new(), twin_host: false,
            inserted_by_load: false,
            path: Vec::new(),
            straight_fallback: None,
            fallback_path: Vec::new(),
            head: None,
            seat_face: SeatFace::default(),
            seat_normal: [0.0; 3],
        }
    }

    /// Carrier 0 on the plate, arm 1 resting on it, partner post 2, and
    /// member 3: a cross diagonal seated on `seated_on` and landing on the
    /// partner (or, with `seated_on` `None`, an arm on the partner instead).
    fn field(seated_on: Option<u32>, third: StrutKind) -> Vec<SweepStrut> {
        vec![
            member(0, StrutKind::Arm, Landing::Plate, None),
            member(1, StrutKind::Arm, Landing::Strut { id: 0, layer: 40 }, None),
            member(2, StrutKind::Arm, Landing::Plate, None),
            member(3, third, Landing::Strut { id: 2, layer: 30 }, seated_on),
        ]
    }

    #[test]
    fn a_carrier_held_by_a_frame_is_named_with_the_part_that_holds_it() {
        // A cross seated on the carrier itself.
        let struts = field(Some(0), StrutKind::Brace);
        let tied = crosses_seated(&struts);
        assert_eq!(tied, vec![true, false, false, false]);
        assert_eq!(frame_lock(&struts, &tied, 0, &[1]), Some(FrameLock::CrossTied));
        // A cross seated on the arm resting on it.
        let struts = field(Some(1), StrutKind::Brace);
        let tied = crosses_seated(&struts);
        assert_eq!(frame_lock(&struts, &tied, 0, &[1]), Some(FrameLock::CrossTiedBelow));
        // A mast resting on it.
        let mut struts = field(None, StrutKind::Arm);
        struts[1].kind = StrutKind::Mast;
        let tied = crosses_seated(&struts);
        assert_eq!(frame_lock(&struts, &tied, 0, &[1]), Some(FrameLock::NonArmResting));
        // CONTROL: no frame anywhere under it, it moves with its follower.
        let struts = field(None, StrutKind::Arm);
        let tied = crosses_seated(&struts);
        assert_eq!(tied, vec![false; 4]);
        assert_eq!(frame_lock(&struts, &tied, 0, &[1]), None);
    }
}

#[cfg(test)]
mod put_back_tests {
    use super::{put_back_all, Blamed, LipSeatOutcome, PutBack, Relanding};
    use crate::{Landing, SeatFace, StrutKind, SweepStrut};

    fn member(id: u32, cell: usize, k_top: usize) -> SweepStrut {
        SweepStrut {
            id,
            cell,
            center_mm: [0.0, 0.0],
            kind: StrutKind::Arm,
            min_radius_mm: 0.0,
            thickened_mm: Vec::new(),
            seat_on_strut: None,
            k_top,
            k_bottom: 0,
            landing: Landing::Plate,
            interior: false,
            radius_mm: Vec::new(),
            carried_mm2: Vec::new(),
            covered_demand: Vec::new(), twin_host: false,
            inserted_by_load: false,
            path: Vec::new(),
            straight_fallback: None,
            fallback_path: Vec::new(),
            head: None,
            seat_face: SeatFace::default(),
            seat_normal: [0.0; 3],
        }
    }

    /// **The reviewer's case** (round5-review-fixes 2026-09-29): mover M was
    /// re-landed from a straight drop in its seat column 10 to a lean whose
    /// shaft stands in column 20 at layer 40. The sizing that judged the
    /// move inserted arm I, which grew down and landed on M's MOVED shaft at
    /// layer 40. The sized field then blamed M, and M is put back. I must not
    /// be left standing where M's shaft used to be.
    #[test]
    fn putting_a_relanding_back_leaves_no_inserted_arm_on_air() {
        let mover_seat = 10_u32;
        let moved_base = 20_u32;
        // Layers 0..=100: column 20 up to layer 50, then column 10.
        let moved_path: Vec<u32> = (0..=100)
            .map(|layer| if layer <= 50 { moved_base } else { mover_seat })
            .collect();
        let mover = SweepStrut {
            path: moved_path,
            ..member(0, mover_seat as usize, 100)
        };
        // I, appended by the sizing: seat in column 30 at layer 60, down to
        // M's moved shaft in column 20 at layer 40.
        let inserted = SweepStrut {
            k_bottom: 40,
            landing: Landing::Strut { id: 0, layer: 40 },
            inserted_by_load: true,
            path: (40..=60)
                .map(|layer| if layer == 40 { moved_base } else { 30 })
                .collect(),
            ..member(1, 30, 60)
        };
        let mut struts = vec![mover, inserted];
        let sized_from = 1;
        assert!(
            crate::landing_check::check_landings(&struts, 0.05).clean(),
            "the premise: I stands on M's moved shaft"
        );
        let mut moves = vec![Relanding {
            index: 0,
            id: 0,
            cell: moved_base as usize,
            cost_mm: 1.0,
            was: (Landing::Plate, 0, Vec::new()),
            children_were: Vec::new(),
        }];
        let mut refused = Vec::new();
        let mut outcome = LipSeatOutcome {
            arms_relanded: 1,
            relanded_ids: vec![0],
            ..LipSeatOutcome::default()
        };
        put_back_all(
            &mut struts,
            sized_from,
            &mut moves,
            &Blamed {
                moves: vec![(0, PutBack::Roots)],
                ..Blamed::default()
            },
            &mut refused,
            &mut outcome,
        );
        let check = crate::landing_check::check_landings(&struts, 0.05);
        println!(
            "put back: {} members left, {} inserted arms withdrawn ({} on a put-back move); \
             landing check: {} strut landings, absent {:?}, elsewhere {:?}, unrooted {:?}",
            struts.len(),
            outcome.inserted_arms_withdrawn,
            outcome.inserted_on_a_put_back_move,
            check.strut_landings,
            check.parent_absent,
            check.parent_elsewhere,
            check.unrooted
        );
        assert!(
            check.clean() && check.invalid_landings() == 0,
            "an arm the judging sizing inserted on the moved shaft stands on air after the put \
             back: elsewhere {:?}, absent {:?}",
            check.parent_elsewhere,
            check.parent_absent
        );
        assert!(struts[0].path.is_empty(), "M is back on its straight drop");
        assert_eq!(struts.len(), sized_from);
        assert_eq!(outcome.inserted_arms_withdrawn, 1);
        assert_eq!(outcome.inserted_on_a_put_back_move, 1);
        assert_eq!(refused, vec![(0, moved_base as usize)]);
        assert_eq!(outcome.arms_relanded, 0);
        assert_eq!(outcome.moves_put_back_roots, 1);
        assert!(moves.is_empty());
    }
}

#[cfg(test)]
mod blame_attribution_tests {
    use super::{blame, refuse_for_rerun, PutBack, Relanding, SizedField};
    use crate::{Landing, SeatFace, StrutKind, SweepStrut};

    fn member(id: u32, cell: usize, k_top: usize, k_bottom: usize, landing: Landing) -> SweepStrut {
        SweepStrut {
            id,
            cell,
            center_mm: [0.0, 0.0],
            kind: StrutKind::Arm,
            min_radius_mm: 0.0,
            thickened_mm: Vec::new(),
            seat_on_strut: None,
            k_top,
            k_bottom,
            landing,
            interior: false,
            radius_mm: Vec::new(),
            carried_mm2: Vec::new(),
            covered_demand: Vec::new(), twin_host: false,
            inserted_by_load: false,
            path: Vec::new(),
            straight_fallback: None,
            fallback_path: Vec::new(),
            head: None,
            seat_face: SeatFace::default(),
            seat_normal: [0.0; 3],
        }
    }

    /// The stilt plate's arm 78 (for-craig-v8, `247b8d5`): it already stood
    /// on post 71 when the lip-seat pass moved it DOWN the same post to a
    /// smaller approach angle. In the same pass other arms were moved ONTO
    /// post 71 from elsewhere, and the sized field read 71 overloaded
    /// (61.405 mm² > 56.549). Only the moves that ADDED load to 71 can have
    /// overloaded it; arm 78's load was on 71 before its move. Blaming it
    /// struck the near post off its list for the rest of the rule, and it
    /// re-landed 9.6 mm away (a 13.55 mm branch).
    ///
    /// A's move is the LONGER of the two here, so R2 alone (longest first)
    /// would put A back: this grades R1, the attribution.
    #[test]
    fn a_move_down_the_host_it_already_stood_on_is_not_blamed_for_that_hosts_overload() {
        let post = member(0, 100, 700, 0, Landing::Plate);
        // A: was on the post at 653, moved down the same post to 627.
        let a = member(1, 200, 700, 627, Landing::Strut { id: 0, layer: 627 });
        // B: was on the plate (its own column), moved onto the post at 628.
        let b = member(2, 300, 700, 628, Landing::Strut { id: 0, layer: 628 });
        let struts = vec![post, a, b];
        let moves = vec![
            relanding(1, 9.0, Landing::Strut { id: 0, layer: 653 }),
            relanding(2, 5.0, Landing::Plate),
        ];
        let (blamed, unexplained, _) = blame(&struts, &moves, &calm(), &over(&[0]));
        assert_eq!(unexplained, 0);
        assert_eq!(
            blamed.moves,
            vec![(1, PutBack::HostLoad)],
            "only the move that added load to the overloaded post may be put back"
        );
        assert_eq!(blamed.spared_already_carried, 1, "the R1 path hit");
    }

    fn relanding(index: usize, cost_mm: f64, was: Landing) -> Relanding {
        Relanding {
            index,
            id: u32::try_from(index).expect("a small test index"),
            cell: 100,
            cost_mm,
            was: (was, 0, Vec::new()),
            children_were: Vec::new(),
        }
    }

    fn calm() -> SizedField {
        SizedField {
            overloaded: Vec::new(),
            root_pairs: Vec::new(),
        }
    }

    fn over(members: &[u32]) -> SizedField {
        SizedField {
            overloaded: members.iter().map(|id| (*id, 61.405)).collect(),
            root_pairs: Vec::new(),
        }
    }

    /// **R2.** Three moves each added load to the same post (all three came
    /// from the plate). Only the longest goes back on this check; the field is
    /// sized again, and only if the post is still over its limit does the
    /// next longest go back.
    #[test]
    fn where_several_moves_overload_a_post_the_longest_goes_back_alone_first() {
        let post = member(0, 100, 700, 0, Landing::Plate);
        let on_post = |id: u32| {
            member(
                id,
                200 + id as usize,
                700,
                620,
                Landing::Strut { id: 0, layer: 620 },
            )
        };
        let struts = vec![post, on_post(1), on_post(2), on_post(3)];
        let mut moves = vec![
            relanding(1, 6.0, Landing::Plate),
            relanding(2, 20.45, Landing::Plate),
            relanding(3, 13.26, Landing::Plate),
        ];
        let (blamed, unexplained, _) = blame(&struts, &moves, &calm(), &over(&[0]));
        assert_eq!(unexplained, 0);
        assert_eq!(
            blamed.moves,
            vec![(1, PutBack::HostLoad)],
            "the 20.45 mm move alone"
        );
        assert_eq!(blamed.held_behind_longer, 2, "the R2 path hit");
        // Sized again with that move put back: the post still over its limit.
        moves.remove(1);
        let (blamed, _, _) = blame(&struts, &moves, &calm(), &over(&[0]));
        assert_eq!(
            blamed.moves,
            vec![(1, PutBack::HostLoad)],
            "then the 13.26 mm move"
        );
        // And once it is within its limit, nothing more goes back.
        moves.remove(1);
        let (blamed, _, _) = blame(&struts, &moves, &calm(), &calm());
        assert!(blamed.is_empty());
    }

    /// **R2 across breaks.** The longest move overloaded two posts' chain
    /// (it stands on post 1, which stands on post 0); a shorter move added
    /// only to post 0. Putting the longest back relieves both, so the
    /// shorter one stands until the field is sized again. A third move that
    /// is the only offender of another post goes back on the same check.
    #[test]
    fn a_move_goes_back_only_for_a_break_no_longer_move_already_goes_back_for() {
        let post = member(0, 100, 700, 0, Landing::Plate);
        let middle = member(1, 110, 650, 600, Landing::Strut { id: 0, layer: 600 });
        let far_post = member(2, 120, 700, 0, Landing::Plate);
        let long = member(3, 130, 700, 640, Landing::Strut { id: 1, layer: 640 });
        let short = member(4, 140, 700, 610, Landing::Strut { id: 0, layer: 610 });
        let lone = member(5, 150, 700, 600, Landing::Strut { id: 2, layer: 600 });
        let struts = vec![post, middle, far_post, long, short, lone];
        let moves = vec![
            relanding(3, 15.0, Landing::Plate),
            relanding(4, 7.0, Landing::Plate),
            relanding(5, 3.0, Landing::Plate),
        ];
        let (blamed, unexplained, _) = blame(&struts, &moves, &calm(), &over(&[0, 1, 2]));
        assert_eq!(unexplained, 0);
        assert_eq!(
            blamed.moves,
            vec![(0, PutBack::HostLoad), (2, PutBack::HostLoad)],
            "the longest (posts 1 and 0) and the lone offender of post 2, not the short one"
        );
        assert_eq!(blamed.held_behind_longer, 1);
    }

    /// **R1 on plate roots.** A root pair that laps now and did not: the move
    /// down the post it already stood on keeps that post's root as its foot
    /// and is not blamed for it; an arm moved off its host onto the plate is
    /// a new root there and is.
    #[test]
    fn a_move_whose_foot_was_already_its_foot_is_not_blamed_for_that_roots_new_pair() {
        let post = member(0, 100, 700, 0, Landing::Plate);
        let host = member(3, 400, 700, 0, Landing::Plate);
        // A: moved down post 0 (it stood on it before).
        let a = member(1, 200, 700, 627, Landing::Strut { id: 0, layer: 627 });
        // B: was on post 3; now its own root on the plate beside post 0.
        let b = member(2, 300, 700, 0, Landing::Plate);
        let struts = vec![post, a, b, host];
        let moves = vec![
            relanding(1, 9.0, Landing::Strut { id: 0, layer: 653 }),
            relanding(2, 5.0, Landing::Strut { id: 3, layer: 500 }),
        ];
        let after = SizedField {
            overloaded: Vec::new(),
            root_pairs: vec![(0, 2)],
        };
        let (blamed, _, unexplained) = blame(&struts, &moves, &calm(), &after);
        assert_eq!(unexplained, 0);
        assert_eq!(blamed.moves, vec![(1, PutBack::Roots)]);
        assert_eq!(blamed.spared_already_carried, 1);
    }

    /// The mover still answers for its OWN overload: whatever loads it now
    /// stands on its moved shaft. And an overload no move added to is
    /// reported, not pinned on a move that did not make it.
    #[test]
    fn the_mover_answers_for_its_own_overload_and_an_overload_no_move_added_is_reported() {
        let post = member(0, 100, 700, 0, Landing::Plate);
        let a = member(1, 200, 700, 627, Landing::Strut { id: 0, layer: 627 });
        let struts = vec![post, a];
        let moves = vec![relanding(1, 9.0, Landing::Strut { id: 0, layer: 653 })];
        let (blamed, unexplained, _) = blame(&struts, &moves, &calm(), &over(&[1]));
        assert_eq!(blamed.moves, vec![(0, PutBack::HostLoad)]);
        assert_eq!(unexplained, 0);
        let (blamed, unexplained, _) = blame(&struts, &moves, &calm(), &over(&[0]));
        assert!(
            blamed.is_empty(),
            "the post's load from A was there before A moved"
        );
        assert_eq!(unexplained, 1);
    }

    /// **Across passes** (fix/creep-45 round 11, 2026-10-02), the laid tube's
    /// case in miniature. Pass 0 moved A off the plate onto post 0, and its
    /// check found the post under its limit. Pass 1 moved B onto another post,
    /// and its check finds post 0 over. Judged pass by pass (B alone, against
    /// the field pass 0 left) no move feeds the post: "unattributed", left
    /// standing. Judged over every standing move against the field before the
    /// first move, A is named, and A is an earlier pass's move, so its descent
    /// is refused for a re-run.
    #[test]
    fn a_host_that_the_moves_of_two_passes_overload_names_the_earlier_pass_move() {
        let post = member(0, 100, 700, 0, Landing::Plate);
        let a = member(1, 200, 700, 95, Landing::Strut { id: 0, layer: 95 });
        let b = member(2, 300, 700, 400, Landing::Strut { id: 3, layer: 400 });
        let other_post = member(3, 400, 700, 0, Landing::Plate);
        let struts = vec![post, a, b, other_post];
        let standing = vec![
            relanding(1, 4.0, Landing::Plate),
            relanding(2, 6.0, Landing::Plate),
        ];
        let pass_from = 1;
        // The rule as it was: this pass's moves, against the previous pass's field.
        let (blamed, unexplained, _) = blame(&struts, &standing[pass_from..], &calm(), &over(&[0]));
        assert!(blamed.is_empty());
        assert_eq!(unexplained, 1, "the old blind spot");
        // The rule now: every standing move, against the field before the first.
        let (blamed, unexplained, _) = blame(&struts, &standing, &calm(), &over(&[0]));
        assert_eq!(unexplained, 0);
        assert_eq!(blamed.moves, vec![(0, PutBack::HostLoad)]);
        assert!(blamed.reaches_before(pass_from));
        let mut refused = Vec::new();
        let earlier = refuse_for_rerun(&standing, &blamed, pass_from, &mut refused);
        assert_eq!(earlier, 1);
        assert_eq!(refused, vec![(1, 100)]);
    }
}

/// **The model below is a landing** (fix/morel-long-posts, 2026-10-02): the
/// re-landing search offers the model's strike face where the column has one,
/// priced as the up-sweep prices it, and the plate only where it does not.
#[cfg(test)]
mod model_landing_tests {
    use super::{price, LipSeatOutcome};
    use crate::up_sweep::{ColumnHistory, ShaftSpan};
    use crate::{Landing, SeatFace, StrutKind, SweepStrut};

    const H: f64 = 0.05;
    const SIDE: usize = 120;
    const SEAT_LAYER: usize = 100;
    const SLAB_TOP: usize = 40;

    fn grid() -> crate::grid::Grid {
        crate::grid::lay_out_at(0.0, 0.0, H, SIDE, SIDE)
    }

    /// Cured model over layers `0..=SLAB_TOP` in every column where `cured`
    /// says so: the base's top face under a mushroom cap, as the up-sweep's
    /// model spans record it.
    fn history(cured: impl Fn(usize, usize) -> bool) -> ColumnHistory {
        let grid = grid();
        let mut history = ColumnHistory::new(&grid);
        for row in 0..SIDE {
            for col in 0..SIDE {
                if !cured(row, col) {
                    continue;
                }
                let cell = row * SIDE + col;
                let slot = u32::try_from(history.model_spans.len()).expect("small");
                history.model_spans.push(ShaftSpan {
                    k_bottom: 0,
                    k_top: u32::try_from(SLAB_TOP).expect("small"),
                    id: 0,
                    next: history.model_head[cell],
                    landable_end: u32::MAX,
                });
                history.model_head[cell] = slot;
            }
        }
        history
    }

    fn arm(landing: Landing, k_bottom: usize) -> SweepStrut {
        let grid = grid();
        let cell = 60 * SIDE + 60;
        SweepStrut {
            id: 0,
            cell,
            center_mm: [grid.center_x_mm(60), grid.center_y_mm(60)],
            kind: StrutKind::Arm,
            min_radius_mm: 0.0,
            thickened_mm: Vec::new(),
            seat_on_strut: None,
            k_top: SEAT_LAYER,
            k_bottom,
            landing,
            interior: false,
            radius_mm: vec![0.1; SEAT_LAYER - k_bottom + 1],
            carried_mm2: Vec::new(),
            covered_demand: Vec::new(), twin_host: false,
            inserted_by_load: false,
            path: Vec::new(),
            straight_fallback: None,
            fallback_path: Vec::new(),
            head: None,
            seat_face: SeatFace::default(),
            seat_normal: [0.0; 3],
        }
    }

    /// Every candidate `price` offers the arm under a cap face tilted 30° out
    /// toward −X with a 20° limit (so it must lean out), over `history`.
    fn candidates(history: &ColumnHistory, strut: &SweepStrut) -> Vec<(Landing, f64)> {
        candidates_among(history, vec![strut.clone()], false)
    }

    /// [`candidates`] for the arm `struts[0]` with the other members of
    /// `struts` standing in the column index too, and `arms_only` as the
    /// clearance re-route sets it.
    fn candidates_among(
        history: &ColumnHistory,
        struts: Vec<SweepStrut>,
        arms_only: bool,
    ) -> Vec<(Landing, f64)> {
        let strut = struts[0].clone();
        let grid = grid();
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
        let law = crate::slenderness::SlendernessLaw::new(30.0, 0.05, 1.0, 32.5);
        let context = crate::slenderness::SlendernessContext {
            grid: &grid,
            history,
            keep_out: &keep_out,
            lean: &lean,
            law: &law,
            layer_height_mm: H,
            layers: 200,
            flare_factor: 1.6,
            mast_root_clearance: true,
            final_clearance_mm: 0.3,
            member_clearance: true,
            model_surface: None,
            plate_z_mm: 0.0,
            shaft_landings: true,
            merge: true,
            brace: true,
            thicken: true,
            mast: true,
            max_struts: 1_000,
            x_allowed: 18.0,
            seat_faces: crate::tip_landing::SeatFaceRule::from_overhang_band(45.0, 25.0),
            load_step_taper: crate::adapter::LoadStepTaper::of(&lean),
            brace_cap: true,
            brace_window: true,
            frame_bands: true,
            frame_ledger: true,
            model_tie: true,
        };
        let mut index = ColumnHistory::new(&grid);
        crate::congestion::stamp_all(&mut index, &struts, H);
        let (sin, cos) = 30.0_f64.to_radians().sin_cos();
        #[expect(
            clippy::cast_precision_loss,
            reason = "a small layer index is exact in f64"
        )]
        let seat = [strut.center_mm[0], strut.center_mm[1], SEAT_LAYER as f64 * H];
        let mut outcome = LipSeatOutcome::default();
        price(
            &context,
            &index,
            &struts,
            0,
            seat,
            [-sin, 0.0, -cos],
            20.0,
            &[true],
            &[],
            arms_only,
            &mut outcome,
        )
        .into_iter()
        .map(|candidate| (candidate.landing, candidate.cost_mm))
        .collect()
    }

    /// A member standing in column `(60, col)` over layers `0..=top`, on the
    /// plate, of the given kind and id.
    fn member_in_column(id: u32, kind: StrutKind, col: usize, top: usize) -> SweepStrut {
        let grid = grid();
        SweepStrut {
            id,
            cell: 60 * SIDE + col,
            center_mm: [grid.center_x_mm(col), grid.center_y_mm(60)],
            kind,
            k_top: top,
            k_bottom: 0,
            landing: Landing::Plate,
            radius_mm: vec![0.1; top + 1],
            ..arm(Landing::Plate, 0)
        }
    }

    /// **A RE-ROUTE LANDS ONLY ON AN ARM** (lane cylinder-bracing, rule E,
    /// 2026-10-05; `cylinder-v15-regression.md`). The column history holds the
    /// braces too, so the shaft search returned a cross diagonal as a landing,
    /// and the Cylinder's branch 169 was re-landed on brace 362: a diagonal
    /// sized for drag (`r_b = l / S`), not for a load path, which made a new
    /// run over its limit (8.53 mm against 4.85 mm).
    ///
    /// The arm of the fixtures above, under the cap face that makes it lean
    /// −X, with a member standing in a column inside its reach, 60 layers
    /// tall (below the arm's lean ceiling). As a BRACE it may not be landed on
    /// by the re-route (`arms_only`): no candidate lands on member 1. SABOTAGE
    /// INSIDE: the same field priced for the lip-seat rule (`arms_only` off)
    /// offers it, so the refusal comes from the rule. CONTROL: the same member
    /// as an ARM is still offered to the re-route.
    #[test]
    fn a_rerouted_arm_never_lands_on_a_brace() {
        let bare = history(|_, _| false);
        let on_member = |found: &[(Landing, f64)]| {
            found
                .iter()
                .filter(|(landing, _)| matches!(landing, Landing::Strut { id: 1, .. }))
                .count()
        };
        let field = |kind: StrutKind| {
            vec![
                arm(Landing::Plate, 0),
                member_in_column(1, kind, 50, 60),
            ]
        };
        let reroute = candidates_among(&bare, field(StrutKind::Brace), true);
        let lip_seat = candidates_among(&bare, field(StrutKind::Brace), false);
        assert!(
            on_member(&lip_seat) > 0,
            "anti-vacuity: the lip-seat search offers a brace as a landing ({} candidates)",
            lip_seat.len()
        );
        assert_eq!(
            on_member(&reroute),
            0,
            "the re-route offered a landing on a brace: {:?}",
            reroute
                .iter()
                .filter(|(landing, _)| matches!(landing, Landing::Strut { id: 1, .. }))
                .collect::<Vec<_>>()
        );
        assert!(!reroute.is_empty(), "the re-route still offers the plate and the model");
        let as_an_arm = candidates_among(&bare, field(StrutKind::Arm), true);
        assert!(
            on_member(&as_an_arm) > 0,
            "control: an arm is still a landing for the re-route"
        );
    }

    /// The Morel base's cap arms (for-craig-v10): a slab under the seat, and
    /// the search offered only the plate. Now every column over the slab
    /// offers the slab's top, and the cheapest descent lands on it.
    #[test]
    fn over_the_base_the_search_offers_the_model_and_lands_on_it() {
        let slab = history(|_, _| true);
        let found = candidates(&slab, &arm(Landing::Model { layer: SLAB_TOP }, SLAB_TOP));
        assert!(!found.is_empty(), "anti-vacuity: no candidate at all");
        let on_plate = found
            .iter()
            .filter(|(landing, _)| *landing == Landing::Plate)
            .count();
        assert_eq!(
            on_plate, 0,
            "{on_plate} of {} candidates offered the plate through a slab every column holds",
            found.len()
        );
        assert_eq!(
            found[0].0,
            Landing::Model { layer: SLAB_TOP },
            "the cheapest descent is {:?}",
            found[0]
        );
        // Priced as the up-sweep prices a model landing: at least its height
        // over the slab, plus the height it still owes to the plate, which
        // is the seat's own height; a plate descent of the same lean costs
        // more than that.
        #[expect(
            clippy::cast_precision_loss,
            reason = "a small layer index is exact in f64"
        )]
        let seat_height = SEAT_LAYER as f64 * H;
        assert!(
            found[0].1 >= seat_height,
            "a model landing priced under the seat's own height: {:?}",
            found[0]
        );
    }

    /// The model landing is offered where the model stands, not invented:
    /// with the slab only on the side the lean cannot take (+X; the face
    /// looks −X, so the arm must lean −X) every candidate is the plate; with
    /// the slab only far out on the −X side, the columns over it offer the
    /// slab and the columns short of it the plate.
    #[test]
    fn beside_the_base_the_plate_is_still_offered() {
        let count = |found: &[(Landing, f64)]| {
            let on_plate = found
                .iter()
                .filter(|(landing, _)| *landing == Landing::Plate)
                .count();
            (on_plate, found.len() - on_plate)
        };
        let behind = history(|_, col| col > 60);
        let (on_plate, on_model) =
            count(&candidates(&behind, &arm(Landing::Model { layer: SLAB_TOP }, SLAB_TOP)));
        assert!(on_plate > 0 && on_model == 0, "behind: {on_plate} plate, {on_model} model");
        let ahead = history(|_, col| col < 40);
        let (on_plate, on_model) =
            count(&candidates(&ahead, &arm(Landing::Plate, 0)));
        assert!(on_plate > 0 && on_model > 0, "ahead: {on_plate} plate, {on_model} model");
        // With no model at all, the plate is all there is (the search as it
        // was everywhere before this rule).
        let bare = history(|_, _| false);
        let found = candidates(&bare, &arm(Landing::Plate, 0));
        assert!(
            found.iter().all(|(landing, _)| *landing == Landing::Plate),
            "a model landing offered with no model"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{min_drop_mm, thickest_radius_keeping_gap_mm, top_aligned};

    /// The spare gap is the radius the arm can grow by, up to the maximum, and
    /// an arm already inside the gap cannot grow at all.
    #[test]
    fn the_thickest_radius_spends_the_spare_gap_up_to_the_maximum() {
        // 0.058 mm drawn, 0.298 mm clear of the model, a 0.3 mm final gap: inside
        // the gap already, so no growth (Deinonychus arm 215).
        assert!((thickest_radius_keeping_gap_mm(0.058, 0.298, 0.3, 1.0) - 0.058).abs() < 1e-12);
        // 0.5 mm clear leaves 0.2 mm to grow by.
        assert!((thickest_radius_keeping_gap_mm(0.058, 0.5, 0.3, 1.0) - 0.258).abs() < 1e-12);
        // Open space is capped by the registered maximum radius.
        assert!((thickest_radius_keeping_gap_mm(0.058, f64::INFINITY, 0.3, 1.0) - 1.0).abs() < 1e-12);
        // A drawn radius past the maximum is not shrunk.
        assert!((thickest_radius_keeping_gap_mm(1.2, 5.0, 0.3, 1.0) - 1.2).abs() < 1e-12);
    }

    /// With S = 40 a 12.347 mm lean at 0.092 mm needs 0.309 mm: refused where
    /// the gap allows 0.092 mm, admitted where it allows the maximum.
    #[test]
    fn a_lean_past_s_times_the_thickest_fit_is_over_its_limit() {
        let law = crate::slenderness::SlendernessLaw::new(40.0, 0.05, 1.0, 40.0);
        let tight = thickest_radius_keeping_gap_mm(0.092, 0.30, 0.3, law.max_radius_mm());
        assert!(12.347 > law.max_unbraced_mm(tight));
        let open = thickest_radius_keeping_gap_mm(0.092, 2.0, 0.3, law.max_radius_mm());
        assert!(12.347 <= law.max_unbraced_mm(open));
        // A disabled law constrains nothing.
        let off = crate::slenderness::SlendernessLaw::disabled();
        assert!(12.347 <= off.max_unbraced_mm(tight));
    }

    /// **A MOVED CHILD IS ASKED ON ITS PROPOSED PATH** (challenger round 3,
    /// 2026-10-05). The landing search exempted a follower from the final gap
    /// when its OLD axis was hidden, although the trial draws it on its new
    /// path onto the moved axis. A sealed 9 mm box, 1.5 mm walls, the +x
    /// wall's inner face at 8.0 mm (hidden voxels reach x = 8.5): the mover
    /// re-lands straight in the cell centred at x = 7.45 (a 0.3 mm solid,
    /// hidden); its child stands straight at x = 6.0 (a 1.2 mm solid, hidden
    /// as it stands) and follows the mover by a lean down to x = 7.45, where
    /// its 1.2 mm solid (plus the quarter-voxel sampling margin, to 8.775)
    /// reaches the wall's outer voxels. Its centreline stays in the cavity
    /// all the way.
    ///
    /// The child as proposed is NOT exempt (it is read against the gap, as
    /// any member outside a cavity is). CONTROLS: the same child asked on its
    /// standing path is exempt (the old answer, and the anti-vacuity: the
    /// child WAS hidden); the same lean at a 0.3 mm solid stays hidden (it is
    /// the solid that leaves, not the centreline); in an open-sided pocket
    /// nothing is exempt. SABOTAGE RUN (by hand): `proposal_hidden` reading
    /// each child with an empty path (its seat's cell all the way down, the
    /// straight axis it stands on) exempts the lean child, and the first
    /// assertion goes red (`[true, true]`).
    #[test]
    fn a_moved_child_is_asked_on_its_proposed_path_not_the_one_it_stands_on() {
        use super::{proposal_hidden, ProposedAxis};
        use crate::cavity::fixtures::{cavity_box_sized, cell, layer, map_of, CENTRE};
        let (sealed, grid) = map_of(&cavity_box_sized(false, false, 0.0, 9.0, 1.5), &[]);
        let (pocket, _) = map_of(&cavity_box_sized(true, false, 0.0, 9.0, 1.5), &[]);
        assert!(sealed.census().enclosed_voxels > 0, "{:?}", sealed.census());
        let at = |x: f64| cell(&grid, x, CENTRE);
        let mover = ProposedAxis {
            k_bottom: layer(0.2),
            k_top: layer(6.0),
            base_cell: Some(at(7.45)),
            seat_cell: at(7.45),
            path: &[],
            radius_mm: 0.3,
        };
        let (k_bottom, k_top) = (layer(2.0), layer(5.0));
        #[expect(clippy::cast_precision_loss, reason = "a small layer count")]
        let lean: Vec<u32> = (k_bottom..=k_top)
            .map(|k| {
                let t = (k - k_bottom) as f64 / (k_top - k_bottom) as f64;
                u32::try_from(at(t.mul_add(6.0 - 7.45, 7.45))).expect("a small grid")
            })
            .collect();
        let seat = at(6.0);
        // A child standing at `seat`, on `path`, with a solid of `radius_mm`.
        let child = |path, radius_mm| ProposedAxis { k_bottom, k_top, base_cell: None, seat_cell: seat, path, radius_mm };
        let (lean, standing): (&[u32], &[u32]) = (&lean, &[]);
        assert_eq!(
            proposal_hidden(&sealed, mover, std::iter::once(child(lean, 1.2))),
            vec![true, false],
            "the moved child's solid leaves the hidden voxels"
        );
        assert_eq!(
            proposal_hidden(&sealed, mover, std::iter::once(child(standing, 1.2))),
            vec![true, true],
            "control: as it stands, the child is hidden"
        );
        assert_eq!(
            proposal_hidden(&sealed, mover, std::iter::once(child(lean, 0.3))),
            vec![true, true],
            "control: a thin solid on the same lean stays hidden"
        );
        assert_eq!(
            proposal_hidden(&pocket, mover, std::iter::once(child(standing, 1.2))),
            vec![false, false],
            "control: an open pocket exempts nothing"
        );
    }

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
}
