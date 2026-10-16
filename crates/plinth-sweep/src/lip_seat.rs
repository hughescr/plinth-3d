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
//!    cheapest descent that passed every test runs longer than `S` times the
//!    thickest radius at which the arm still keeps the final gap (the spare
//!    gap measured for it, up to the registered maximum radius), and the arm
//!    as it stands is a support (longer than one layer, its solid not inside
//!    the model; q12 regressions, 2026-10-12), the arm keeps its seat and
//!    falls to item 3. Seat-bite triggers only; the clearance re-route is not
//!    asked.
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
    ///
    /// `foot_sizing` sizes a model landing's foot for the load it passes on
    /// the SIZED field (`size_arm(carried at the landing)`, the law
    /// [`crate::model_feet`] draws it by), so a candidate onto the model must
    /// fit that foot, not the tip's minimum (fix lane A, 2026-10-12).
    Clearance {
        violators: &'a [bool],
        foot_sizing: Option<crate::down_sweep::SizingPolicy>,
    },
}

impl Trigger<'_> {
    /// The law a model landing's foot is sized by on this trigger: the
    /// clearance re-route runs on the sized field and reads it; the lip-seat
    /// rule runs before and keeps the tip's minimum.
    const fn foot_sizing(self) -> Option<crate::down_sweep::SizingPolicy> {
        match self {
            Trigger::Clearance { foot_sizing, .. } => foot_sizing,
            Trigger::SeatBite => None,
        }
    }

    /// Whether member `index` is sent to be re-landed, given its measured bite.
    fn sends(self, index: usize, bite: Option<&ShaftSeatBite>) -> bool {
        match self {
            Trigger::SeatBite => bite.is_some_and(over_limit),
            Trigger::Clearance { violators, .. } => violators.get(index).copied().unwrap_or(false),
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
    foot_sizing: Option<crate::down_sweep::SizingPolicy>,
) -> (Vec<bool>, usize, usize) {
    let _perf_span = crate::perf::span(crate::perf::Span::LipViolators);
    let mut flags = vec![false; struts.len()];
    let Some(surface) = joins.surface() else {
        return (flags, 0, 0);
    };
    // A MODEL LANDING WHOSE FOOT NO LONGER FITS (fix lane A, 2026-10-12): the
    // descent checked the foot at the tip's minimum, and the sized field draws
    // it for the load it passes (`model_feet`), which on Deinonychus left the
    // foot under the wrist of the long-fingered hand 38 % on the model. Such
    // an arm is sent to be re-landed, onto a candidate whose foot fits at the
    // sized radius (`price`); one none fits stays, and the census warns.
    let misfits: Vec<u32> = foot_sizing.map_or_else(Vec::new, |sizing| {
        struts
            .iter()
            .filter(|strut| {
                matches!(strut.landing, crate::Landing::Model { .. })
                    && strut.k_bottom != usize::MAX
                    && strut.kind == StrutKind::Arm
            })
            .filter(|strut| {
                let carried = strut.carried_mm2.first().copied().unwrap_or(0.0).max(0.0);
                let radius = crate::down_sweep::size_arm(carried, sizing);
                let (on, total) = crate::up_sweep::foot_on_model(
                    joins.slender.grid,
                    joins.slender.history,
                    strut.base_cell(),
                    strut.k_bottom,
                    radius,
                );
                on < total
            })
            .map(|strut| strut.id)
            .collect()
    });
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
    // An arm whose drawn solid holds another member's tip or head (F4,
    // `MODEL-FEET.md`) is sent too: moving it off the tip is the remedy, since
    // a tip never moves here.
    let buried =
        crate::member_clearance::tips_inside_members(struts, arms, joins.layer_plan, joins.plate_z_mm);
    let named_rows = census
        .within_gap
        .iter()
        .map(|row| (row.member, row.kind))
        .chain(buried.iter().map(|row| (row.host_member, row.host_kind)))
        .chain(misfits.iter().map(|&id| (id, StrutKind::Arm)));
    let mut named = 0;
    for (member, kind) in named_rows {
        if kind != StrutKind::Arm || exclude.contains(&member) {
            continue;
        }
        let slot = match struts.get(member as usize) {
            Some(strut) if strut.id == member => Some(member as usize),
            _ => struts.iter().position(|strut| strut.id == member),
        };
        if let Some(slot) = slot {
            if !flags[slot] {
                flags[slot] = true;
                named += 1;
            }
        }
    }
    (flags, named, misfits.len())
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
    let n = arms.len();
    let frame = solid_frame(surface, arms, on_model, layer_plan, plate_z_mm, tier);
    surface
        .segment_clearance_classified(
            &frame.seats,
            &frame.bases,
            &frame.radii,
            &frame.radii,
            0.0,
            &frame.insets,
            &frame.landing_zones,
        )
        .map_or_else(
            |_| vec![None; n],
            |rows| {
                rows.into_iter()
                    .map(SegmentClearanceClassified::clearance_mm)
                    .collect()
            },
        )
}

/// What [`solid_clearances`] and [`drawn_solid_clearances`] read per member:
/// the drawn segment's two ends, its widest drawn radius, and the lengths at
/// each end the kernel leaves untested (its own seat zone, its own landing
/// zone).
struct SolidFrame {
    seats: Vec<[f64; 3]>,
    bases: Vec<[f64; 3]>,
    radii: Vec<f64>,
    insets: Vec<f64>,
    landing_zones: Vec<f64>,
}

fn solid_frame(
    surface: &NativeSurface,
    arms: &[SweepArm],
    on_model: &[bool],
    layer_plan: &LayerPlan,
    plate_z_mm: f64,
    tier: f64,
) -> SolidFrame {
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
    let radii: Vec<f64> = arms.iter().map(drawn_max_radius_mm).collect();
    let insets: Vec<f64> = arms
        .iter()
        .zip(&discs)
        .zip(&radii)
        .zip(&bases)
        .map(|(((arm, disc), &radius), &base)| {
            let Some(disc) = disc else {
                return f64::INFINITY;
            };
            let Some(axis) =
                geom_kernel::vec3::normalize(geom_kernel::vec3::sub(base, arm.seat_mm))
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
    let landing_zones = crate::member_clearance::model_landing_zones_mm(
        surface, &seats, &bases, &radii, on_model, tier,
    );
    SolidFrame {
        seats,
        bases,
        radii,
        insets,
        landing_zones,
    }
}

/// **The same clearance read on the solid the file draws**: each member as the
/// straight spans the lowering gives it ([`plinth_support::drawn_spans`], the
/// one rule: chords on or above the area law's section at every height),
/// every span read as the frustum it is, in ONE batched kernel call, with the
/// same seat and landing zones left untested as [`solid_clearances`] leaves.
/// [`solid_clearances`] reads the axis at the member's WIDEST radius all along,
/// a lower bound on the clearance of the drawn solid: a tapered or stepped
/// member can read inside the model there while its thinner shaft stands
/// clear. The
/// least clearance over a member's spans; `None` where the kernel did not
/// decide, or nothing is left to test after the zones.
fn drawn_solid_clearances(
    surface: &NativeSurface,
    arms: &[SweepArm],
    on_model: &[bool],
    layer_plan: &LayerPlan,
    plate_z_mm: f64,
    tier: f64,
) -> Vec<Option<f64>> {
    let n = arms.len();
    let frame = solid_frame(surface, arms, on_model, layer_plan, plate_z_mm, tier);
    let mut owner: Vec<usize> = Vec::new();
    let mut p0s: Vec<[f64; 3]> = Vec::new();
    let mut p1s: Vec<[f64; 3]> = Vec::new();
    let mut top_radii: Vec<f64> = Vec::new();
    let mut bottom_radii: Vec<f64> = Vec::new();
    let mut inset0s: Vec<f64> = Vec::new();
    let mut inset1s: Vec<f64> = Vec::new();
    for (index, arm) in arms.iter().enumerate() {
        let (seat, base) = (frame.seats[index], frame.bases[index]);
        let rise = seat[2] - base[2];
        let length = geom_kernel::vec3::norm(geom_kernel::vec3::sub(seat, base));
        if rise.partial_cmp(&0.0) != Some(core::cmp::Ordering::Greater) {
            continue;
        }
        let root = plinth_support::drawn_root_radius_mm(&arm.radius_profile_mm, arm.tip_radius_mm);
        let spans = plinth_support::drawn_spans(
            &arm.radius_profile_mm,
            arm.base_mm[2],
            arm.seat_mm[2],
            seat[2],
            base[2],
            arm.tip_radius_mm,
            root,
        );
        // The point of the axis at height `z`, and its distance from the seat.
        let at = |z: f64| -> ([f64; 3], f64) {
            let t = ((seat[2] - z) / rise).clamp(0.0, 1.0);
            (
                std::array::from_fn(|axis| seat[axis] + t * (base[axis] - seat[axis])),
                t * length,
            )
        };
        for span in spans {
            let (from, a0) = at(span.top_z_mm);
            let (to, a1) = at(span.bottom_z_mm);
            let inset0 = (frame.insets[index] - a0).max(0.0);
            let inset1 = (frame.landing_zones[index] - (length - a1)).max(0.0);
            if a1 <= a0 || inset0 + inset1 >= a1 - a0 {
                continue;
            }
            owner.push(index);
            p0s.push(from);
            p1s.push(to);
            top_radii.push(span.top_radius_mm.max(0.0));
            bottom_radii.push(span.bottom_radius_mm.max(0.0));
            inset0s.push(inset0);
            inset1s.push(inset1);
        }
    }
    let mut out: Vec<Option<f64>> = vec![None; n];
    if owner.is_empty() {
        return out;
    }
    let Ok(rows) = surface.segment_frustum_clearance_classified(
        &p0s,
        &p1s,
        &top_radii,
        &bottom_radii,
        0.0,
        &inset0s,
        &inset1s,
    ) else {
        return out;
    };
    for (row, &index) in rows.into_iter().zip(&owner) {
        if let Some(clearance) = row.clearance_mm() {
            if out[index].is_none_or(|least| clearance < least) {
                out[index] = Some(clearance);
            }
        }
    }
    out
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

/// **Whether a proposed lean is longer than the slenderness law holds at the
/// thickest radius its measured clearance leaves room for** (Q1): the lean
/// `arm` (drawn as the trial draws it, measured `now` clear of the model in
/// the pass's one batched clearance read) for `strut`, landing as `candidate`
/// proposes. The run is the chord the ladder measures; the radius is the drawn
/// radius grown by the spare gap ([`thickest_radius_keeping_gap_mm`]), up to
/// the registered maximum. Arithmetic only: no kernel read of its own.
///
/// **Not the thicken rung's taper** (q12 regressions, 2026-10-12, review).
/// A lean leaving its seat face reads its minimum clearance at the gap itself
/// (its seat zone ends where the solid first stands the gap clear of the seat
/// plane), so this reading leaves it no room to grow anywhere, although the
/// rung's taper (tip kept, root raised away from the seat face) would hold
/// many such leans clear of the model. Admitting those (`ff2e64c`, the taper
/// drawn as a frustum and read against the model) took ten more leans on
/// Deinonychus that Q12 kept as standing arms, two of them the long slanted
/// runs Q12 removed (8.4 mm at 48 deg and 12.7 mm at 30 deg, each about 6 mm
/// sideways; `q12/REGRESSIONS.md`), while the arms it was meant for are
/// taken by the kept-arm test in [`apply`] instead: a lean refused here is
/// refused only while the arm as it stands is a support. Whether a held lean
/// should replace a standing support is the owner's call, open.
fn lean_too_slender(
    context: &SlendernessContext<'_>,
    strut: &SweepStrut,
    candidate: &Candidate,
    arm: &SweepArm,
    now: f64,
    tier: f64,
) -> bool {
    let law = context.law;
    #[expect(
        clippy::cast_precision_loss,
        reason = "a layer count bounded by the plan's own layer count is exact in f64"
    )]
    let drop_mm = strut.k_top.saturating_sub(candidate.k_bottom) as f64 * context.layer_height_mm;
    let run_mm = drop_mm.hypot(crate::cell_distance_mm(context.grid, strut.cell, candidate.cell));
    let fit_mm =
        thickest_radius_keeping_gap_mm(drawn_max_radius_mm(arm), now, tier, law.max_radius_mm());
    run_mm > law.max_unbraced_mm(fit_mm)
}

/// Whether the slenderness refusal keeps the arm as it stands in place of a
/// lean it cannot hold (see the comment at its call in [`apply`]).
///
/// The arm as it stands must be physically there to keep: clear of the model
/// (`clear_of_model`) and standing on its ground (`rooted`). Then the lean the
/// law cannot hold is taken in one case only: the lean's tip really seats on
/// its face (`lean_seats`) and the standing arm's does not (`stands_seats`).
/// Seating is the real contact the emitted file would have, read by the
/// same measure that grows the balls ([`crate::contact_growth`]); a SHAPE of
/// the standing arm (a one-layer stub on the model, say) says nothing about it
/// (Bat_Cat_Owl arm 20, joint round 4: a stub on the claw's 0.018 mm edge with
/// 0.0000 mm2 of contact, replaced by a 40 degree lean that touched nothing
/// either and ran 3.4 times its limit).
fn keeps_standing_arm(
    clear_of_model: bool,
    rooted: bool,
    stands_seats: bool,
    lean_seats: bool,
) -> bool {
    clear_of_model && rooted && !(lean_seats && !stands_seats)
}

/// What one tip really touches: whether it seats (see [`tips_seat`]) and the
/// real contact it makes, `None` where the tip has no seat face or no ball to
/// measure.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TipContact {
    pub seats: bool,
    pub real_mm2: Option<f64>,
    /// One pixel of area, mm2: the contact below which the printer cannot
    /// tell a touch from none (the same tolerance [`tips_seat`] seats by).
    pub pixel_mm2: f64,
}

impl TipContact {
    /// A tip measured to make less than one pixel of contact with the model.
    /// An unmeasured tip is not proof of that. (Bat_Cat_Owl arm 20: 0.0000 mm2
    /// for the stub, 0.00003 mm2 for the lean, against a pixel of 0.0025.)
    fn touches_nothing(self) -> bool {
        self.real_mm2.is_some_and(|real| real < self.pixel_mm2)
    }
}

/// **An arm that holds nothing is not kept** (round 6, 2026-10-14,
/// `NUB-CLAW.md` part 1). The kept-arm test keeps the standing arm when neither
/// it nor the lean seats. Where the standing arm's tip makes under one pixel
/// of real contact with the model (the tolerance [`tips_seat`] seats by) and
/// the lean's does not either, the arm is a nub: Bat_Cat_Owl arm 20 was a
/// 0.156 mm stub in an antler groove whose tip ball cleared the antler by
/// 0.002 mm (0.0000 mm2; its lean 0.00003 mm2), put there for a birth of ONE
/// printer pixel (0.0003 mm2) of a fin narrower than a pixel, the antler two
/// pixels away. It holds nothing and the owner sees it as a stray lump.
/// Dropped, with the birth's pixel area as a WARNING, and only where nothing
/// rests on it. A tip with a pixel or more of contact, however short of its
/// plan, is a support ("better than no support") and is kept as before.
fn holds_nothing(stands: TipContact, lean: TipContact, carries_someone: bool) -> bool {
    !carries_someone && stands.touches_nothing() && lean.touches_nothing()
}

/// An arm dropped because it held nothing ([`holds_nothing`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnheldArm {
    /// The arm's id when it was dropped (ids are renumbered by the drop).
    pub arm: u32,
    /// The load it was seated for, mm2 (the birth's own pixel area).
    pub load_mm2: f64,
    /// Where its tip stood, mm in the build frame.
    pub seat_mm: [f64; 3],
}

/// For each `(standing arm, lean)` pair, whether its tip really seats on its
/// face: `(the standing arm's, the lean's)`. Seating is the real contact (the
/// model surface inside the drawn ball, measured exactly) reaching the planned
/// contact to within one pixel of area, the test the ball growth uses, and
/// above zero. Both arms of a pair seat on the same face (a lean moves the
/// base, never the seat), found from the standing arm's seat-face point. One
/// batched read for all the pairs ([`crate::contact_growth::measure`]); a tip
/// with no seat face or no ball does not seat.
///
/// **The standing arm's half is read once per pass** (perf lane A1,
/// 2026-10-14). The standing arm is `arms[index]`, fixed for the whole
/// [`apply`] pass, and its seat site (the nearest admitted face to its seat-face
/// point) and its contact at that site are pure functions of that arm, the
/// mesh, the layer plan and the surface, none of which changes during the
/// pass. The round loop asked the same question again each round: 539 017
/// brute-force face scans over 117 199 faces on Deinonychus (62 % of the run).
/// [`StandingSeats`] holds the answers by arm index; only the lean (which
/// differs for every candidate) is read each round.
pub(crate) fn tips_seat(
    joins: &HeadJoinContext<'_>,
    layer_plan: &LayerPlan,
    standing: &mut StandingSeats,
    pairs: &[(usize, &SweepArm, &SweepArm)],
) -> Vec<(TipContact, TipContact)> {
    let context = joins.slender;
    let pixel_area_mm2 = context.grid.pitch_mm() * context.grid.pitch_mm();
    let contact = |row: Option<&crate::contact_growth::Measured>| TipContact {
        seats: row.is_some_and(|m| m.real_mm2 > 0.0 && m.real_mm2 + pixel_area_mm2 >= m.planned_mm2),
        real_mm2: row.map(|m| m.real_mm2),
        pixel_mm2: pixel_area_mm2,
    };
    let no_ball = |arm: &SweepArm| SweepArm {
        contact_ball_radius_mm: None,
        ..arm.clone()
    };
    // The standing arms not read yet, once each, in one batch.
    let mut fresh: Vec<(usize, &SweepArm)> = pairs
        .iter()
        .filter(|(index, _, _)| !standing.reads.contains_key(index))
        .map(|&(index, stands, _)| (index, stands))
        .collect();
    fresh.sort_unstable_by_key(|&(index, _)| index);
    fresh.dedup_by_key(|&mut (index, _)| index);
    if !fresh.is_empty() {
        let table = standing
            .table
            .get_or_insert_with(|| crate::footprint_normal::FaceTable::of(joins.mesh));
        let points: Vec<Option<[f64; 3]>> = fresh.iter().map(|(_, arm)| arm.seat_face_point_mm).collect();
        let sites = crate::footprint_normal::sites_at(table, &points, context.seat_faces);
        let rows: Vec<SweepArm> = fresh.iter().map(|(_, arm)| no_ball(arm)).collect();
        let read = joins.measure_contacts(layer_plan, &rows, &sites);
        for (((index, _), site), measured) in fresh.iter().zip(sites).zip(read) {
            standing.reads.insert(
                *index,
                StandingSeat {
                    site,
                    contact: contact(measured.as_ref()),
                },
            );
        }
    }
    // The leans, on the site their standing arm seats on.
    let rows: Vec<SweepArm> = pairs.iter().map(|(_, _, lean)| no_ball(lean)).collect();
    let row_sites: Vec<Option<crate::footprint_normal::TipSite>> = pairs
        .iter()
        .map(|(index, _, _)| standing.reads.get(index).and_then(|read| read.site))
        .collect();
    let leans = joins.measure_contacts(layer_plan, &rows, &row_sites);
    pairs
        .iter()
        .zip(&leans)
        .map(|((index, _, _), lean)| {
            let stands = standing.reads.get(index).map_or_else(|| contact(None), |read| read.contact);
            (stands, contact(lean.as_ref()))
        })
        .collect()
}

/// One standing arm's seat site and the contact it makes there, as
/// [`tips_seat`] reads them.
#[derive(Clone, Copy)]
struct StandingSeat {
    site: Option<crate::footprint_normal::TipSite>,
    contact: TipContact,
}

/// **The standing arms of one [`apply`] pass, seat-read once each**, by arm
/// index (`arms` is fixed for the pass, so the index names the same arm in
/// every round), and the mesh's face table the reads share. A new pass starts
/// with a new value: nothing is kept between passes, so a changed field, mesh
/// or layer plan can never meet a stale read.
#[derive(Default)]
pub(crate) struct StandingSeats {
    table: Option<crate::footprint_normal::FaceTable>,
    reads: std::collections::HashMap<usize, StandingSeat>,
}

#[cfg(test)]
impl StandingSeats {
    /// How many standing arms have been read.
    pub(crate) fn arms_read(&self) -> usize {
        self.reads.len()
    }
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
    /// Model landings not offered because the arm's foot would not lie
    /// wholly on cured model there (lane 2, 2026-10-12, `MODEL-FEET.md` F2;
    /// [`crate::up_sweep::foot_on_model`]).
    pub candidates_refused_foot: usize,
    /// Model landings the clearance re-route sent because their foot, sized
    /// for the load it passes on the sized field, does not lie wholly on the
    /// model (fix lane A, 2026-10-12), at the first pass and the last.
    pub foot_misfits_first: usize,
    pub foot_misfits_last: usize,
    pub candidates_refused_path: usize,
    pub candidates_refused_plate: usize,
    pub candidates_refused_measured: usize,
    /// Refused because a moved member's drawn solid would come nearer the
    /// model than the final gap, or than it already stood.
    pub candidates_refused_clearance: usize,
    /// Refused because the lean's straight run is longer than `S` times the
    /// thickest radius at which the arm keeps the final gap, while the arm as
    /// it stands is a support (Q1, 2026-10-10; q12 regressions, 2026-10-12).
    pub candidates_refused_slender: usize,
    /// Of the arms kept with no admissible descent, those that had a lean
    /// refused for its slenderness (they keep the seat-bite WARNING).
    pub kept_for_slenderness: usize,
    /// The arms that had a lean refused for its slenderness (each once), the
    /// last pass's list.
    pub kept_for_slenderness_ids: Vec<u32>,
    /// Arms found to hold nothing in this run, not yet dropped: their tip
    /// touches the model nowhere and neither does their lean, and nothing
    /// rests on them ([`holds_nothing`]). The caller drops them once the
    /// passes are done and moves them to [`Self::unheld`].
    pub unheld_pending: Vec<UnheldArm>,
    /// The arms dropped for holding nothing, each with the load (the
    /// birth's pixel area) it was seated for: a WARNING each on the emitted
    /// file. That load is reported as not carried.
    pub unheld: Vec<UnheldArm>,
    /// Leans the slenderness test refused that were TAKEN anyway, because the
    /// arm as it stands is not there to keep (its solid is inside the model, or
    /// it floats above its ground), or because the lean's tip really seats on
    /// its face and the standing arm's does not (q12 regressions, 2026-10-12;
    /// joint round 5, 2026-10-14). The path hit of that test. Summed over
    /// every pass, like [`Self::candidates_refused_slender`].
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
    /// **The mover's own solid clearance, read ahead** ([`read_movers`]): by
    /// candidate index, the path the read was drawn on and the read.
    ahead: std::collections::HashMap<usize, Ahead>,
    /// **The path tests of candidates read ahead** ([`Tested`]), by candidate
    /// index: a refused candidate as well as an admitted one, each valid for
    /// the field generation it was asked at ([`StampedIndex::generation`]) and
    /// no other. The round loop takes its candidate from here instead of
    /// asking [`propose`] again.
    tested: std::collections::HashMap<usize, Tested>,
    /// How many candidates the next read-ahead of this arm takes, from the
    /// one a round proposes (doubled at each read-ahead, up to
    /// [`READ_AHEAD_MAX`]).
    window: usize,
    /// The bytes of `tested` and `ahead` entries charged to the pass's
    /// [`Ledger`] for this arm.
    retained: usize,
    /// What the deep scan settled past the cursor ([`scan_open_jobs`]).
    scan: ScanStore,
}

/// A path test that passed: the mover's new path and each child's
/// ([`propose`]). Shared by the arm's `tested` and `ahead` entries for one
/// candidate (one copy of the descendants' paths, not two).
type Found = (Vec<u32>, Vec<Follow>);

/// One candidate's path test, as [`propose`] answered it against the field at
/// `generation` (`None` in `found`: refused).
struct Tested {
    generation: u64,
    found: Option<std::rc::Rc<Found>>,
    /// The bytes charged for this entry.
    charged: usize,
}

/// **The bytes the read-ahead may hold** for one [`apply`] pass.
///
/// `READ_AHEAD_MAX` bounds the candidates an arm reads ahead, not their bytes:
/// a trunk with hundreds of thousands of long descendants would keep every
/// descendant's path for each of 64 candidates. The ledger counts the bytes of
/// the descendant paths (and trial solids) a read-ahead retains, across all
/// arms; a candidate that would take it past `limit` is not read ahead, and
/// the round loop asks it when it gets there, as it did before the read-ahead
/// existed. Whether the limit trips changes no answer: every stored result is
/// a pure function of the candidate and the field, the same as the one
/// asked on demand.
struct Ledger {
    retained: usize,
    limit: usize,
}

impl Ledger {
    fn new() -> Self {
        Self {
            retained: 0,
            limit: read_ahead_limit_bytes(),
        }
    }

    /// Whether `bytes` more may be retained.
    const fn has_room(&self, bytes: usize) -> bool {
        self.retained.saturating_add(bytes) <= self.limit
    }
}

/// The bytes the read-ahead may retain in one pass; the working set of one
/// read-ahead call (trial solids, groups) is held to a quarter of it.
const READ_AHEAD_BYTES: usize = 256 << 20;

/// [`READ_AHEAD_BYTES`], or `PLINTH_SWEEP_LIPSEAT_READ_AHEAD_BYTES` where set
/// (a diagnostic: the output does not depend on it). Read once.
fn read_ahead_limit_bytes() -> usize {
    #[cfg(test)]
    if let Some(limit) = TEST_BUDGET.with(std::cell::Cell::get) {
        return limit;
    }
    static LIMIT: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *LIMIT.get_or_init(|| {
        std::env::var("PLINTH_SWEEP_LIPSEAT_READ_AHEAD_BYTES")
            .ok()
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(READ_AHEAD_BYTES)
    })
}

/// The bytes a stored path test holds: its entry, and the descendants' paths.
fn found_bytes(found: Option<&Found>) -> usize {
    const ENTRY: usize = 64;
    found.map_or(ENTRY, |(path, children)| {
        ENTRY
            + std::mem::size_of_val(path.as_slice())
            + children
                .iter()
                .map(|(_, _, path)| std::mem::size_of::<Follow>() + std::mem::size_of_val(path.as_slice()))
                .sum::<usize>()
    })
}

impl Pending {
    /// Store a path test for candidate `at`, charging the ledger.
    fn store_tested(&mut self, ledger: &mut Ledger, at: usize, generation: u64, found: Option<std::rc::Rc<Found>>) {
        let charged = found_bytes(found.as_deref());
        if let Some(old) = self.tested.insert(at, Tested { generation, found, charged }) {
            self.credit(ledger, old.charged);
        }
        self.charge(ledger, charged);
    }

    /// Take the stored path test of candidate `at`, crediting the ledger.
    fn take_tested(&mut self, ledger: &mut Ledger, at: usize) -> Option<Tested> {
        let tested = self.tested.remove(&at)?;
        self.credit(ledger, tested.charged);
        Some(tested)
    }

    /// Store a read-ahead of candidate `at`, charging the ledger `charged`.
    fn store_ahead(&mut self, ledger: &mut Ledger, at: usize, ahead: Ahead) {
        let charged = ahead.charged;
        if let Some(old) = self.ahead.insert(at, ahead) {
            self.credit(ledger, old.charged);
        }
        self.charge(ledger, charged);
    }

    /// Take the read-ahead of candidate `at`, crediting the ledger.
    fn take_ahead(&mut self, ledger: &mut Ledger, at: usize) -> Option<Ahead> {
        let ahead = self.ahead.remove(&at)?;
        self.credit(ledger, ahead.charged);
        Some(ahead)
    }

    /// The arm's job has ended: nothing it read ahead will be asked again.
    fn release(&mut self, ledger: &mut Ledger) {
        self.tested = std::collections::HashMap::new();
        self.ahead = std::collections::HashMap::new();
        // The scan's verdicts are charged into `retained` too.
        self.scan.clear();
        self.scan.verdicts = std::collections::VecDeque::new();
        ledger.retained = ledger.retained.saturating_sub(self.retained);
        self.retained = 0;
    }

    /// Drop what was stored for candidates `from..to`, which a deep scan
    /// settled ([`scan_open_jobs`]): nothing will ask them again.
    fn drop_settled(&mut self, ledger: &mut Ledger, from: usize, to: usize) {
        if from >= to {
            return;
        }
        let mut freed = 0_usize;
        self.tested.retain(|at, tested| {
            let keep = !(from..to).contains(at);
            if !keep {
                freed += tested.charged;
            }
            keep
        });
        self.ahead.retain(|at, ahead| {
            let keep = !(from..to).contains(at);
            if !keep {
                freed += ahead.charged;
            }
            keep
        });
        self.credit(ledger, freed);
    }

    fn charge(&mut self, ledger: &mut Ledger, bytes: usize) {
        self.retained += bytes;
        ledger.retained += bytes;
        #[cfg(test)]
        TEST_PEAK_RETAINED.with(|cell| cell.set(cell.get().max(ledger.retained)));
    }

    fn credit(&mut self, ledger: &mut Ledger, bytes: usize) {
        self.retained = self.retained.saturating_sub(bytes);
        ledger.retained = ledger.retained.saturating_sub(bytes);
    }
}

/// **The column index of one [`apply`] pass, with the generation of the field
/// it stands for.**
///
/// [`propose`] is a pure function of its arguments: the context (fixed for
/// the pass), the candidate and the mover's children (fixed per job), the
/// column index, and the struts it reads (the mover, its children, the member
/// the candidate lands on, and the hosts a transitive follow walks). During a
/// pass the index and those struts change in ONE place: the commit of a
/// re-landing (`apply`'s round loop), which rewrites a member's landing,
/// base layer or path and stamps it into the index. The generation counts
/// the stamps, and [`StampedIndex::stamp_strut`] is the only way to stamp, so
/// every mutation that can change a path test's answer bumps it in the
/// mutator itself (it is bumped before the stamp, and a re-landing rewrites
/// the struts only to stamp them next). A stored path test is reused only at
/// the generation it was asked at; `PLINTH_SWEEP_VERIFY_LIPSEAT=1` recomputes
/// every reuse and panics on a difference ([`verify_lip_seat`]).
struct StampedIndex {
    index: ColumnHistory,
    generation: u64,
    /// S0 (measurement only, `perf` on): the generation at which each grid
    /// column and each member last changed, so a stale stored path test can
    /// be graded against a per-cell / per-member read set. Never read by the
    /// pass itself.
    s0: Option<S0Gens>,
}

/// [`StampedIndex::s0`]: per column and per member, the generation of the
/// last stamp that touched it.
struct S0Gens {
    cell: Vec<u64>,
    strut: Vec<u64>,
}

impl StampedIndex {
    const fn new(index: ColumnHistory) -> Self {
        Self {
            index,
            generation: 0,
            s0: None,
        }
    }

    const fn generation(&self) -> u64 {
        self.generation
    }

    /// Stamp `strut` as it stands now; every stored path test is stale after.
    fn stamp_strut(&mut self, strut: &SweepStrut, layer_height_mm: f64) {
        #[cfg(not(test))]
        let skip = false;
        // The sabotage drill skips the bump; the shadow check must catch it.
        #[cfg(test)]
        let skip = TEST_SKIP_BUMP.with(std::cell::Cell::get);
        if !skip {
            self.generation += 1;
        }
        if let Some(gens) = self.s0.as_mut() {
            let generation = self.generation;
            for cell in strut.path.iter().map(|&cell| cell as usize).chain([strut.cell]) {
                if let Some(slot) = gens.cell.get_mut(cell) {
                    *slot = generation;
                }
            }
            if let Some(slot) = gens.strut.get_mut(strut.id as usize) {
                *slot = generation;
            }
        }
        self.index.stamp_strut(strut, layer_height_mm);
    }

    /// S0: mark member `member` changed at the current generation (a host a
    /// commit landed on; its lock flag changed, which S2's read set tags).
    fn s0_touch(&mut self, member: usize) {
        let generation = self.generation;
        if let Some(slot) = self.s0.as_mut().and_then(|gens| gens.strut.get_mut(member)) {
            *slot = generation;
        }
    }

    /// S0: whether a path test stored at `generation` for `job`'s candidate
    /// `candidate` with answer `found` is stale under S2's read set: a column
    /// of the mover's or a child's path, or the mover, a child or the host,
    /// changed after it was asked.
    fn s0_stale_fine(&self, generation: u64, job: &Pending, candidate: Candidate, found: Option<&Found>) -> bool {
        let Some(gens) = self.s0.as_ref() else {
            return true;
        };
        let after = |slot: Option<&u64>| slot.is_some_and(|&at| at > generation);
        let host = match candidate.landing {
            Landing::Strut { id, .. } => Some(id as usize),
            Landing::Plate | Landing::Model { .. } => None,
        };
        let members = std::iter::once(job.index).chain(job.children.iter().copied()).chain(host);
        if members.into_iter().any(|member| after(gens.strut.get(member))) {
            return true;
        }
        found.is_some_and(|(path, children)| {
            path.iter()
                .chain(children.iter().flat_map(|(_, _, child_path)| child_path.iter()))
                .any(|&cell| after(gens.cell.get(cell as usize)))
        })
    }
}

/// S0 (p2design, 2026-10-15): what one [`apply`] call's path tests and
/// read-ahead add up on this thread, written to the `s0.lip.*` rows when the
/// call ends. Measurement only: nothing in the pass reads it. A nested call
/// on the same thread (a stolen pose) saves and restores its caller's tally.
#[derive(Clone, Copy)]
struct S0Tally {
    propose_calls: u64,
    propose_ns: u64,
    window_ns: u64,
    stale_today: u64,
    stale_fine: u64,
    stored_valid: u64,
    ahead_rows: u64,
    /// S2's exact-key memo probe ([`s0_memo_exact`]).
    probe_rows: u64,
    probe_call: u64,
    probe_sweep: u64,
}

impl S0Tally {
    const ZERO: Self = Self {
        propose_calls: 0,
        propose_ns: 0,
        window_ns: 0,
        stale_today: 0,
        stale_fine: 0,
        stored_valid: 0,
        ahead_rows: 0,
        probe_rows: 0,
        probe_call: 0,
        probe_sweep: 0,
    };
}

/// `PLINTH_S2_MEMO_PROBE=1` with `perf` on: count every mover read the pass
/// sends to the kernel by its EXACT key ([`s0_memo_exact`]). Read once.
fn s2_memo_probe() -> bool {
    static PROBE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    crate::perf::enabled()
        && *PROBE.get_or_init(|| std::env::var_os("PLINTH_S2_MEMO_PROBE").is_some_and(|value| value == "1"))
}

/// The exact key of one mover read: the surface, every kernel input of the
/// row (seat, base, radius, seat inset, landing zone) and the bound, as bits.
type ProbeKey = [u64; 11];

thread_local! {
    /// S2's exact memo probe: the sweep the keys belong to (its column
    /// history's address), the keys read in that sweep, and the keys read in
    /// the current `apply` call.
    static S2_PROBE: std::cell::RefCell<(usize, std::collections::HashSet<ProbeKey>, std::collections::HashSet<ProbeKey>)> =
        std::cell::RefCell::new((0, std::collections::HashSet::new(), std::collections::HashSet::new()));
}

/// **S2's exact-key memo probe** (measurement only, [`s2_memo_probe`]): each
/// row of a mover read batch keyed by everything the kernel's answer is a
/// function of (the surface, the row's five kernel inputs as
/// [`solid_frame`] builds them, and the bound), counted, and counted again if
/// the same key was read earlier in this `apply` call or in this sweep. An
/// exact memo could answer exactly the repeats. Rebuilds the frame (probe
/// runs only).
fn s0_memo_exact(
    frame: &ReadFrame<'_>,
    context: &SlendernessContext<'_>,
    solids: &[SweepArm],
    on_model: &[bool],
    bounds: &[f64],
) {
    if !s2_memo_probe() || solids.is_empty() {
        return;
    }
    let rows = solid_frame(
        frame.surface,
        solids,
        on_model,
        frame.layer_plan,
        frame.plate_z_mm,
        frame.tier,
    );
    let surface = std::ptr::from_ref(frame.surface) as u64;
    let sweep = std::ptr::from_ref(context.history) as usize;
    let (mut call, mut swept) = (0_u64, 0_u64);
    S2_PROBE.with(|probe| {
        let mut probe = probe.borrow_mut();
        if probe.0 != sweep {
            probe.0 = sweep;
            probe.1.clear();
        }
        for (at, &bound) in bounds.iter().enumerate() {
            let [s0, s1, s2] = rows.seats[at];
            let [b0, b1, b2] = rows.bases[at];
            let key: ProbeKey = [
                surface,
                s0.to_bits(),
                s1.to_bits(),
                s2.to_bits(),
                b0.to_bits(),
                b1.to_bits(),
                b2.to_bits(),
                rows.radii[at].to_bits(),
                rows.insets[at].to_bits(),
                rows.landing_zones[at].to_bits(),
                bound.to_bits(),
            ];
            call += u64::from(!probe.2.insert(key));
            swept += u64::from(!probe.1.insert(key));
        }
    });
    let sent = solids.len() as u64;
    s0_tally(|tally| {
        tally.probe_rows += sent;
        tally.probe_call += call;
        tally.probe_sweep += swept;
    });
}

thread_local! {
    static S0_TALLY: std::cell::Cell<S0Tally> = const { std::cell::Cell::new(S0Tally::ZERO) };
}

/// Add to this thread's [`S0Tally`]. Call only with `perf` on.
fn s0_tally(add: impl FnOnce(&mut S0Tally)) {
    S0_TALLY.with(|cell| {
        let mut tally = cell.get();
        add(&mut tally);
        cell.set(tally);
    });
}

/// Nanoseconds since `since`.
fn s0_ns(since: std::time::Instant) -> u64 {
    u64::try_from(since.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// S0: the driver-thread parts of one [`apply`] call (`s0.lip.apply` rows).
#[derive(Clone, Copy)]
enum S0Part {
    Price,
    B1,
    Trial,
    Standing,
    Ahead,
    Follow,
    Bites,
    Seats,
    B2,
}

/// S0: one [`apply`] call's own counts and part times, and the clock. Every
/// method is one branch when `perf` is off.
struct S0Apply {
    on: bool,
    started: Option<std::time::Instant>,
    mark: Option<std::time::Instant>,
    ns: [u64; 9],
    saved: S0Tally,
    rounds: u64,
    proposals: u64,
    b1_refused: u64,
    longest_slot: usize,
    longest_cands: u64,
    longest_accept_plus1: u64,
    cand_total: u64,
    b2_lock: u64,
    b2_reask: u64,
    b2_reask_refused: u64,
    seat_pairs: u64,
    bite_arms: u64,
    bite_known: u64,
    nubs: u64,
    by_open_rounds: [u64; 4],
    by_open_ns: [u64; 4],
    round_open: usize,
    round_started: Option<std::time::Instant>,
    refused_before: [usize; 5],
    /// S2: what the open jobs' deep scans did in this call.
    scan: ScanTally,
    /// S2's memo probe: the caller's per-call key set, given back at the end.
    saved_probe: Option<std::collections::HashSet<ProbeKey>>,
}

/// S2 (`s2.lip.scan` rows): the deep scans of one [`apply`] call.
#[derive(Default)]
struct ScanTally {
    scans: u64,
    rounds: u64,
    chunks: u64,
    cands: u64,
    passes: u64,
    child_rows: u64,
    ref_path: u64,
    ref_clear: u64,
    stops: u64,
    ns: u64,
    read_ns: u64,
}

impl S0Apply {
    fn start(outcome: &LipSeatOutcome) -> Self {
        let on = crate::perf::enabled();
        let now = on.then(std::time::Instant::now);
        let saved = if on {
            S0_TALLY.with(|cell| cell.replace(S0Tally::ZERO))
        } else {
            S0Tally::ZERO
        };
        Self {
            on,
            started: now,
            mark: now,
            ns: [0; 9],
            saved,
            rounds: 0,
            proposals: 0,
            b1_refused: 0,
            longest_slot: usize::MAX,
            longest_cands: 0,
            longest_accept_plus1: 0,
            cand_total: 0,
            b2_lock: 0,
            b2_reask: 0,
            b2_reask_refused: 0,
            seat_pairs: 0,
            bite_arms: 0,
            bite_known: 0,
            nubs: 0,
            by_open_rounds: [0; 4],
            by_open_ns: [0; 4],
            round_open: 0,
            round_started: None,
            refused_before: Self::refused(outcome),
            scan: ScanTally::default(),
            saved_probe: s2_memo_probe()
                .then(|| S2_PROBE.with(|probe| std::mem::take(&mut probe.borrow_mut().2))),
        }
    }

    const fn refused(outcome: &LipSeatOutcome) -> [usize; 5] {
        [
            outcome.candidates_refused_clearance,
            outcome.candidates_refused_plate,
            outcome.candidates_refused_roots,
            outcome.candidates_refused_measured,
            outcome.candidates_refused_slender,
        ]
    }

    /// Charge the time since the last mark to `part`.
    fn lap(&mut self, part: S0Part) {
        if let Some(mark) = self.mark {
            let now = std::time::Instant::now();
            self.ns[part as usize] += u64::try_from(now.duration_since(mark).as_nanos()).unwrap_or(u64::MAX);
            self.mark = Some(now);
        }
    }

    fn jobs(&mut self, pending: &[Pending]) {
        if !self.on {
            return;
        }
        for (slot, job) in pending.iter().enumerate() {
            let len = job.candidates.len() as u64;
            self.cand_total += len;
            if len > self.longest_cands {
                self.longest_cands = len;
                self.longest_slot = slot;
            }
        }
    }

    fn round_start(&mut self, open: usize) {
        if self.on {
            self.round_open = open;
            self.round_started = Some(std::time::Instant::now());
            self.rounds += 1;
        }
    }

    fn round_end(&mut self) {
        if let Some(started) = self.round_started.take() {
            let bucket = match self.round_open {
                0 | 1 => 0,
                2 | 3 => 1,
                4..=11 => 2,
                _ => 3,
            };
            self.by_open_rounds[bucket] += 1;
            self.by_open_ns[bucket] += s0_ns(started);
        }
    }

    /// Write the rows and give the caller's tally back.
    fn finish(self, clearance: bool, jobs: usize, commits: usize, outcome: &LipSeatOutcome) {
        if !self.on {
            return;
        }
        let tally = S0_TALLY.with(|cell| cell.replace(self.saved));
        let wall = self.started.map_or(0, s0_ns);
        let count = crate::perf::count;
        let after = Self::refused(outcome);
        let delta = |at: usize| count(after[at].saturating_sub(self.refused_before[at]));
        crate::perf::row(
            crate::perf::Rows::LipApply,
            &[
                u64::from(clearance),
                count(jobs),
                self.rounds,
                self.proposals,
                count(commits),
                self.b1_refused,
                self.cand_total,
                self.longest_cands,
                self.longest_accept_plus1,
                tally.propose_calls,
                tally.propose_ns,
                wall,
                self.ns[S0Part::Price as usize],
                self.ns[S0Part::B1 as usize],
                self.ns[S0Part::Trial as usize],
                self.ns[S0Part::B2 as usize],
            ],
        );
        crate::perf::row(
            crate::perf::Rows::LipApplyB,
            &[
                self.ns[S0Part::Standing as usize],
                self.ns[S0Part::Ahead as usize],
                tally.window_ns,
                self.ns[S0Part::Follow as usize],
                self.ns[S0Part::Bites as usize],
                self.ns[S0Part::Seats as usize],
                tally.stale_today,
                tally.stale_fine,
                tally.stored_valid,
                self.b2_lock,
                self.b2_reask,
                self.b2_reask_refused,
                self.seat_pairs,
                self.bite_arms,
                self.bite_known,
                tally.ahead_rows,
            ],
        );
        crate::perf::row(
            crate::perf::Rows::LipApplyOpen,
            &[
                self.by_open_rounds[0],
                self.by_open_rounds[1],
                self.by_open_rounds[2],
                self.by_open_rounds[3],
                self.by_open_ns[0],
                self.by_open_ns[1],
                self.by_open_ns[2],
                self.by_open_ns[3],
                delta(0),
                delta(1),
                delta(2),
                delta(3),
                delta(4),
                self.nubs,
            ],
        );
        crate::perf::row(
            crate::perf::Rows::LipScan,
            &[
                u64::from(clearance),
                self.scan.scans,
                self.scan.rounds,
                self.scan.chunks,
                self.scan.cands,
                self.scan.passes,
                self.scan.child_rows,
                self.scan.ref_path,
                self.scan.ref_clear,
                self.scan.stops,
                self.scan.ns,
                self.scan.read_ns,
                tally.probe_rows,
                tally.probe_call,
                tally.probe_sweep,
            ],
        );
        if let Some(saved) = self.saved_probe {
            S2_PROBE.with(|probe| probe.borrow_mut().2 = saved);
        }
    }
}

impl std::ops::Deref for StampedIndex {
    type Target = ColumnHistory;

    fn deref(&self) -> &ColumnHistory {
        &self.index
    }
}

#[cfg(test)]
thread_local! {
    /// Tests: skip the generation bump (sabotage) or force the shadow check on.
    static TEST_SKIP_BUMP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static TEST_VERIFY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Tests: the read-ahead's byte limit, and how many window candidates it
    /// refused for want of room.
    static TEST_BUDGET: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
    static TEST_REFUSED_FOR_ROOM: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Tests: turn the deep scan off ([`lip_seat_scan`]), and the
    /// candidates the scans settled as refused.
    static TEST_NO_SCAN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static TEST_SCANNED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Tests: candidates the scan's clearance test refused at a child.
    static TEST_CHILD_REFUSED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Tests: the most any ledger held, whether a scan chunk's working set
    /// passed its share, and how often the scan fell back for want of room.
    static TEST_PEAK_RETAINED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static TEST_CHUNK_OVER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static TEST_FALLBACKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Tests: what [`with_scan`]'s work held. `peak`: the most a ledger held;
/// `chunk_over`: a scan chunk's working set passed its share; `fallbacks`:
/// times the scan found no room and took only what was proven.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct ScanMemory {
    pub(crate) peak: usize,
    pub(crate) chunk_over: bool,
    pub(crate) fallbacks: usize,
}

/// Tests: run `work` with the read-ahead's (and the scan's) byte limit set to
/// `limit`; returns its answer and what it held ([`ScanMemory`]).
#[cfg(test)]
pub(crate) fn with_budget<R>(limit: Option<usize>, work: impl FnOnce() -> R) -> (R, ScanMemory) {
    TEST_BUDGET.with(|cell| cell.set(limit));
    TEST_PEAK_RETAINED.with(|cell| cell.set(0));
    TEST_CHUNK_OVER.with(|cell| cell.set(false));
    TEST_FALLBACKS.with(|cell| cell.set(0));
    let answer = work();
    TEST_BUDGET.with(|cell| cell.set(None));
    let memory = ScanMemory {
        peak: TEST_PEAK_RETAINED.with(std::cell::Cell::get),
        chunk_over: TEST_CHUNK_OVER.with(std::cell::Cell::get),
        fallbacks: TEST_FALLBACKS.with(std::cell::Cell::get),
    };
    (answer, memory)
}

/// Tests: run `work` with the deep scan on or off and the shadow
/// check (`PLINTH_SWEEP_VERIFY_LIPSEAT`) as `verify`; returns its answer, how
/// many candidates the scans settled as refused, and how many verdicts
/// (scan's or shadow's) a child's read refused.
#[cfg(test)]
pub(crate) fn with_scan<R>(scan: bool, verify: bool, work: impl FnOnce() -> R) -> (R, usize, usize) {
    TEST_NO_SCAN.with(|cell| cell.set(!scan));
    TEST_VERIFY.with(|cell| cell.set(verify));
    TEST_SCANNED.with(|cell| cell.set(0));
    TEST_CHILD_REFUSED.with(|cell| cell.set(0));
    let answer = work();
    TEST_NO_SCAN.with(|cell| cell.set(false));
    TEST_VERIFY.with(|cell| cell.set(false));
    (
        answer,
        TEST_SCANNED.with(std::cell::Cell::get),
        TEST_CHILD_REFUSED.with(std::cell::Cell::get),
    )
}

/// `PLINTH_SWEEP_VERIFY_LIPSEAT=1`: every stored path test the pass reuses is
/// also asked of [`propose`] afresh and compared; any difference panics. For
/// gate runs, not for production. Read once.
fn verify_lip_seat() -> bool {
    #[cfg(test)]
    if TEST_VERIFY.with(std::cell::Cell::get) {
        return true;
    }
    static VERIFY: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *VERIFY.get_or_init(|| {
        std::env::var_os("PLINTH_SWEEP_VERIFY_LIPSEAT").is_some_and(|value| value == "1")
    })
}

/// The path test of `job`'s candidate `at` for the round loop: the one the
/// read-ahead stored at the field's current generation, else asked now.
/// Either way the stored entry is spent (a stale one is dropped).
fn take_path_test(
    context: &SlendernessContext<'_>,
    index: &StampedIndex,
    struts: &[SweepStrut],
    job: &mut Pending,
    ledger: &mut Ledger,
    at: usize,
    clearance: bool,
) -> Option<Found> {
    let candidate = job.candidates[at];
    let ask = |job: &Pending| {
        propose(
            context,
            index,
            struts,
            job.index,
            candidate,
            &job.children,
            clearance,
        )
    };
    match job.take_tested(ledger, at) {
        Some(tested) if tested.generation == index.generation() => {
            if crate::perf::enabled() {
                s0_tally(|tally| tally.stored_valid += 1);
            }
            if verify_lip_seat() {
                verify_lip_seat_reuse("round", tested.found.as_deref(), ask(job).as_ref());
            }
            // The read-ahead may still hold the same path test.
            tested
                .found
                .map(|found| std::rc::Rc::try_unwrap(found).unwrap_or_else(|shared| (*shared).clone()))
        }
        Some(stale) => {
            if crate::perf::enabled() {
                let fine = index.s0_stale_fine(stale.generation, job, candidate, stale.found.as_deref());
                s0_tally(|tally| {
                    tally.stale_today += 1;
                    tally.stale_fine += u64::from(fine);
                });
            }
            ask(job)
        }
        None => ask(job),
    }
}

/// What the read-ahead made of one window candidate's path test.
enum Window {
    /// The candidate is refused by the path test.
    Refused,
    /// The candidate passes; the answer is stored for the round loop.
    Passed(std::rc::Rc<Found>),
    /// The ledger has no room: the candidate (and the rest of the window) is
    /// left for the round loop to ask when it gets there.
    Full,
}

/// **The path test of window candidate `at`** for the read-ahead: the one
/// stored at the field's current generation (a refusal too, so a refused
/// candidate is asked once per generation, not once per overlapping window),
/// else asked now and stored; or [`Window::Full`] where the byte limit
/// ([`Ledger`]) leaves no room for it. Either way the round loop's answer
/// for the candidate is the one the on-demand ask gives.
fn window_path_test(
    context: &SlendernessContext<'_>,
    index: &StampedIndex,
    struts: &[SweepStrut],
    job: &mut Pending,
    ledger: &mut Ledger,
    at: usize,
    clearance: bool,
) -> Window {
    // With no room for even an empty entry the candidate is left to the
    // round loop.
    if !ledger.has_room(2 * found_bytes(None)) {
        refused_for_room();
        return Window::Full;
    }
    let generation = index.generation();
    let candidate = job.candidates[at];
    let ask = |job: &Pending| {
        propose(
            context,
            index,
            struts,
            job.index,
            candidate,
            &job.children,
            clearance,
        )
    };
    let stored = job
        .tested
        .get(&at)
        .filter(|tested| tested.generation == generation)
        .map(|tested| tested.found.clone());
    let found = if let Some(found) = stored {
        if verify_lip_seat() {
            verify_lip_seat_reuse("read-ahead", found.as_deref(), ask(job).as_ref());
        }
        found
    } else {
        let found = ask(job).map(std::rc::Rc::new);
        // A refusal is an empty entry; a pass keeps the descendants' paths,
        // and is stored only where the ledger has room for it (twice: here
        // and in the read-ahead entry).
        if found
            .as_deref()
            .is_some_and(|found| !ledger.has_room(2 * found_bytes(Some(found))))
        {
            refused_for_room();
            return Window::Full;
        }
        job.store_tested(ledger, at, generation, found.clone());
        found
    };
    found.map_or(Window::Refused, Window::Passed)
}

/// The shadow check of one reused path test: `stored` against `fresh`.
fn verify_lip_seat_reuse(what: &str, stored: Option<&Found>, fresh: Option<&Found>) {
    assert!(
        stored == fresh,
        "PLINTH_SWEEP_VERIFY_LIPSEAT: a reused path test ({what}) differs from the fresh one\n stored {stored:?}\n fresh  {fresh:?}"
    );
}

/// The most candidates one arm's read-ahead takes in one batch. A batch
/// size only: every read is the same pure function of the candidate and its
/// path whichever batch carries it, so the field does not depend on it.
const READ_AHEAD_MAX: usize = 64;

/// A child following its moved host: `(child index, the cell it now lands
/// in, its new path)`.
type Follow = (usize, usize, Vec<u32>);

/// One round's proposal for one pending arm: the candidate, the arm's new
/// path, and each child's new path onto the moved axis.
struct Proposal {
    slot: usize,
    /// The candidate's index in its arm's list.
    at: usize,
    candidate: Candidate,
    path: Vec<u32>,
    children: Vec<Follow>,
    /// The field generation the path test answered at.
    generation: u64,
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

/// What every solid read of one [`apply`] pass is taken against.
struct ReadFrame<'a> {
    surface: &'a NativeSurface,
    layer_plan: &'a LayerPlan,
    plate_z_mm: f64,
    tier: f64,
}

impl ReadFrame<'_> {
    /// [`solid_clearances`] of `arms`, one batch.
    fn solid(&self, arms: &[SweepArm], on_model: &[bool]) -> Vec<Option<f64>> {
        solid_clearances(
            self.surface,
            arms,
            on_model,
            self.layer_plan,
            self.plate_z_mm,
            self.tier,
        )
    }

    /// [`solid_clearances`] of `arms`, one batch, where only a clearance at
    /// or above `at_least[i]` is wanted: a row the kernel proves below its
    /// bound reads `None`, as an undecided row does, and every other row is
    /// the full reading ([`NativeSurface::segment_clearance_classified_at_least`]).
    fn solid_at_least(&self, arms: &[SweepArm], on_model: &[bool], at_least: &[f64]) -> Vec<Option<f64>> {
        let n = arms.len();
        let frame = solid_frame(
            self.surface,
            arms,
            on_model,
            self.layer_plan,
            self.plate_z_mm,
            self.tier,
        );
        self.surface
            .segment_clearance_classified_at_least(
                &frame.seats,
                &frame.bases,
                &frame.radii,
                &frame.radii,
                0.0,
                &frame.insets,
                &frame.landing_zones,
                at_least,
            )
            .map_or_else(
                |_| vec![None; n],
                |rows| {
                    rows.into_iter()
                        .map(|row| row.and_then(SegmentClearanceClassified::clearance_mm))
                        .collect()
                },
            )
    }
}

/// The least clearance a moved member's solid must read to be admitted
/// ([`apply`]): the final gap outright for a mover onto a new root, or any
/// mover of the clearance re-route; else no nearer than it stood
/// (`min(gap, max(was, 0))`), less the comparison's slack.
fn admitted_at_least(outright: bool, was: Option<f64>, tier: f64) -> f64 {
    let minimum = if outright {
        tier
    } else {
        tier.min(was.unwrap_or(tier).max(0.0))
    };
    minimum - CLEARANCE_SLACK_MM
}

/// **The members of one [`apply`] pass as they stand**, read once each.
///
/// A member's standing solid is `arms[member]` (fixed for the pass), read
/// with its landing on the model or not as `struts[member]` stands; an arm
/// that moves leaves the pending list and a child that moves keeps its
/// landing, so a read keyed by the member and that flag is the read every
/// round would have made. `drawn` holds, for a mover the widest-radius
/// reading puts inside the model, its re-read on the solid the file draws
/// ([`drawn_solid_clearances`]; the kept-arm test asks whether the arm as it
/// stands enters the model, and a tapered or stepped arm reads inside there
/// while its thinner shaft clears).
#[derive(Default)]
struct StandingReads {
    before: std::collections::HashMap<(usize, bool), Option<f64>>,
    drawn: std::collections::HashMap<(usize, bool), Option<f64>>,
}

impl StandingReads {
    fn key(member: usize, struts: &[SweepStrut]) -> (usize, bool) {
        (member, matches!(struts[member].landing, Landing::Model { .. }))
    }

    /// Read every member of `members` not read yet, in one batch; with
    /// `movers`, the doubtful ones again on the drawn solid, in one more.
    fn read(
        &mut self,
        frame: &ReadFrame<'_>,
        arms: &[SweepArm],
        struts: &[SweepStrut],
        members: &[usize],
        movers: bool,
    ) {
        let mut keys: Vec<(usize, bool)> = members
            .iter()
            .map(|&member| Self::key(member, struts))
            .filter(|key| !self.before.contains_key(key))
            .collect();
        keys.sort_unstable();
        keys.dedup();
        if !keys.is_empty() {
            let standing: Vec<SweepArm> = keys.iter().map(|&(member, _)| arms[member].clone()).collect();
            let on_model: Vec<bool> = keys.iter().map(|&(_, on)| on).collect();
            let _site = crate::perf::clear_site(crate::perf::ClearSite::LipBefore);
            for (key, value) in keys.into_iter().zip(frame.solid(&standing, &on_model)) {
                self.before.insert(key, value);
            }
        }
        if !movers {
            return;
        }
        let mut doubtful: Vec<(usize, bool)> = members
            .iter()
            .map(|&member| Self::key(member, struts))
            .filter(|key| {
                !self.drawn.contains_key(key)
                    && self
                        .before
                        .get(key)
                        .copied()
                        .flatten()
                        .is_some_and(|was| was < -CLEARANCE_SLACK_MM)
            })
            .collect();
        doubtful.sort_unstable();
        doubtful.dedup();
        if doubtful.is_empty() {
            return;
        }
        let doubtful_arms: Vec<SweepArm> =
            doubtful.iter().map(|&(member, _)| arms[member].clone()).collect();
        let doubtful_on_model: Vec<bool> = doubtful.iter().map(|&(_, on)| on).collect();
        let _site = crate::perf::clear_site(crate::perf::ClearSite::LipDrawn);
        let read = drawn_solid_clearances(
            frame.surface,
            &doubtful_arms,
            &doubtful_on_model,
            frame.layer_plan,
            frame.plate_z_mm,
            frame.tier,
        );
        for (key, clearance) in doubtful.into_iter().zip(read) {
            // Undecided on the drawn solid: the widest-radius reading stands.
            let was = self.before.get(&key).copied().flatten();
            self.drawn.insert(key, clearance.or(was));
        }
    }

    /// The member's widest-radius reading as it stands (`None` unread).
    fn before(&self, member: usize, struts: &[SweepStrut]) -> Option<f64> {
        self.before.get(&Self::key(member, struts)).copied().flatten()
    }

    /// The mover's standing reading on the solid the file draws where the
    /// widest-radius reading puts it inside the model, else that reading.
    fn drawn(&self, member: usize, struts: &[SweepStrut]) -> Option<f64> {
        let key = Self::key(member, struts);
        self.drawn
            .get(&key)
            .copied()
            .unwrap_or_else(|| self.before.get(&key).copied().flatten())
    }
}

/// **Each proposal's mover read on its trial solid**, into `after` at its
/// first slot, through the arm's read-ahead store (`Pending::ahead`).
///
/// A mover's trial solid is a pure function of its candidate and the path it
/// would take ([`trial_arm`]); the round loop walks an arm's candidates one
/// per round, so an arm refused for its clearance (Deinonychus: some 340 000
/// such candidates) used to cost a kernel call of one or two rows per round.
/// Where a round's proposal has no stored read on its own path, the arm's
/// next `window` candidates are proposed against the index as it stands
/// ([`propose`]) and read with it, every such arm in ONE batch, and the
/// window doubles (up to [`READ_AHEAD_MAX`]). A stored read is used only
/// when the round proposes that candidate on the very path it was read on,
/// so the field is the one the one-at-a-time reads made.
///
/// Each read asks only whether the mover clears what the test in [`apply`]
/// demands of it ([`admitted_at_least`]: `outright` for the clearance
/// re-route, or per candidate a new root; else no nearer than `was(member)`,
/// its standing read): a read proven below that is `None`, which that test
/// refuses exactly as it refuses a reading below it, and every other read is
/// the full one. The value of a refused mover's read is used nowhere else
/// (the slenderness test reads only a mover that cleared; a mover in a
/// hidden cavity is admitted without it).
#[expect(
    clippy::too_many_arguments,
    reason = "the read frame, the field and its index, the pending arms and this round's proposals"
)]
fn read_movers(
    frame: &ReadFrame<'_>,
    context: &SlendernessContext<'_>,
    index: &StampedIndex,
    struts: &[SweepStrut],
    arms: &[SweepArm],
    pending: &mut [Pending],
    proposals: &[Proposal],
    trial: &[SweepArm],
    first_of: &[usize],
    clearance: bool,
    standing_reads: &mut StandingReads,
    ledger: &mut Ledger,
    after: &mut [Option<f64>],
) -> Vec<Option<Prefetched>> {
    struct Row {
        slot: usize,
        at: usize,
        found: std::rc::Rc<Found>,
        /// The first slot of the proposal's own trial members, for a row
        /// that is this round's proposal (its members are drawn already).
        own: Option<usize>,
        solid: SweepArm,
        on_model: bool,
        at_least: f64,
    }
    // The working set of this call (the rows' trial solids, the groups and the
    // follower solids cloned from them) is held to a quarter of the limit.
    let call_limit = ledger.limit / 4;
    let mut call_bytes = 0_usize;
    let plate_z_mm = frame.plate_z_mm;
    let at_least = |standing_reads: &StandingReads, member: usize, landing: Landing| {
        admitted_at_least(
            clearance || !matches!(landing, Landing::Strut { .. }),
            standing_reads.before(member, struts),
            frame.tier,
        )
    };
    let mut rows: Vec<Row> = Vec::new();
    let mut ahead_hits = 0_usize;
    // S0 measurement gate (write-only; nothing below branches on it but the
    // measurement itself).
    let s0_on = crate::perf::enabled();
    for (proposal, &first) in proposals.iter().zip(first_of) {
        let job = &mut pending[proposal.slot];
        if job
            .ahead
            .get(&proposal.at)
            .is_some_and(|ahead| {
                ahead.found.0 == proposal.path && ahead.found.1 == proposal.children
            })
        {
            ahead_hits += 1;
            continue;
        }
        call_bytes += trial_bytes(&trial[first]);
        rows.push(Row {
            slot: proposal.slot,
            at: proposal.at,
            found: std::rc::Rc::new((proposal.path.clone(), proposal.children.clone())),
            own: Some(first),
            solid: trial[first].clone(),
            on_model: matches!(proposal.candidate.landing, Landing::Model { .. }),
            at_least: at_least(standing_reads, job.index, proposal.candidate.landing),
        });
        let end = proposal.at.saturating_add(job.window).min(job.candidates.len());
        let window_started = s0_on.then(std::time::Instant::now);
        for at in proposal.at + 1..end {
            if job.ahead.contains_key(&at) {
                continue;
            }
            let candidate = job.candidates[at];
            let found = match window_path_test(context, index, struts, job, ledger, at, clearance) {
                Window::Full => break,
                Window::Refused => continue,
                Window::Passed(found) => found,
            };
            let solid = trial_arm(
                context,
                plate_z_mm,
                struts,
                arms,
                job.index,
                candidate.landing,
                candidate.k_bottom,
                &found.0,
            );
            // The working set of the call: this row's solid, and the solids of
            // the group the read of its children and bites will draw.
            let row_bytes = trial_bytes(&solid)
                + found
                    .1
                    .iter()
                    .map(|(child, _, _)| {
                        let strut = &struts[*child];
                        8 * (strut.k_top.saturating_sub(strut.k_bottom) + 1) + TRIAL_ARM_BYTES
                    })
                    .sum::<usize>();
            if call_bytes + row_bytes > call_limit {
                refused_for_room();
                break;
            }
            call_bytes += row_bytes;
            rows.push(Row {
                slot: proposal.slot,
                at,
                found,
                own: None,
                solid,
                on_model: matches!(candidate.landing, Landing::Model { .. }),
                at_least: at_least(standing_reads, job.index, candidate.landing),
            });
        }
        if let Some(started) = window_started {
            let ns = s0_ns(started);
            s0_tally(|tally| tally.window_ns += ns);
        }
        let job = &mut pending[proposal.slot];
        job.window = job.window.saturating_mul(2).min(READ_AHEAD_MAX);
    }
    if s0_on {
        let sent = crate::perf::count(rows.len());
        s0_tally(|tally| tally.ahead_rows += sent);
    }
    crate::perf::add(crate::PerfWork::LipSeatMoverReads, crate::perf::count(proposals.len()));
    crate::perf::add(crate::PerfWork::LipSeatMoverReadsAhead, crate::perf::count(ahead_hits));
    if !rows.is_empty() {
        let mut solids: Vec<SweepArm> = Vec::with_capacity(rows.len());
        let mut on_model: Vec<bool> = Vec::with_capacity(rows.len());
        let mut bounds: Vec<f64> = Vec::with_capacity(rows.len());
        for row in &rows {
            solids.push(row.solid.clone());
            on_model.push(row.on_model);
            bounds.push(row.at_least);
        }
        let reads = {
            let _site = crate::perf::clear_site(crate::perf::ClearSite::LipReadAhead);
            frame.solid_at_least(&solids, &on_model, &bounds)
        };
        s0_memo_exact(frame, context, &solids, &on_model, &bounds);
        // **What the round asks next of a mover that clears**, read here for
        // every row that clears, on the pool, so the round finds it done:
        // each child's trial solid against what it stood at, and the seat
        // bites of the mover and its children ([`Prefetched`]).
        let clearing: Vec<usize> = (0..rows.len())
            .filter(|&at| reads[at].is_some_and(|now| now >= bounds[at]))
            .collect();
        let mut prefetched: std::collections::HashMap<usize, Prefetched> =
            std::collections::HashMap::with_capacity(clearing.len());
        if !clearing.is_empty() {
            // Each clearing row's group: the mover's trial solid, then its
            // children's, in the order the round lays them out.
            let groups: Vec<Vec<SweepArm>> = clearing
                .iter()
                .map(|&at| {
                    let row = &rows[at];
                    let mut group = Vec::with_capacity(1 + row.found.1.len());
                    group.push(solids[at].clone());
                    for (offset, (child, _, path)) in row.found.1.iter().enumerate() {
                        group.push(row.own.map_or_else(
                            || {
                                let strut = &struts[*child];
                                trial_arm(
                                    context,
                                    plate_z_mm,
                                    struts,
                                    arms,
                                    *child,
                                    strut.landing,
                                    strut.k_bottom,
                                    path,
                                )
                            },
                            |first| trial[first + 1 + offset].clone(),
                        ));
                    }
                    group
                })
                .collect();
            // The children's solids against what each stood at, one kernel batch.
            let members: Vec<usize> = clearing
                .iter()
                .flat_map(|&at| rows[at].found.1.iter().map(|(child, _, _)| *child))
                .collect();
            standing_reads.read(frame, arms, struts, &members, false);
            let follower_arms: Vec<SweepArm> = groups
                .iter()
                .flat_map(|group| group[1..].iter().cloned())
                .collect();
            let follower_bounds: Vec<f64> = members
                .iter()
                .map(|&member| {
                    admitted_at_least(false, standing_reads.before(member, struts), frame.tier)
                })
                .collect();
            let follower_reads = if follower_arms.is_empty() {
                Vec::new()
            } else {
                let _site = crate::perf::clear_site(crate::perf::ClearSite::LipFollowers);
                frame.solid_at_least(
                    &follower_arms,
                    &vec![false; follower_arms.len()],
                    &follower_bounds,
                )
            };
            // Every group's seat bites, chunked across the pool.
            let slices: Vec<&[SweepArm]> = groups.iter().map(Vec::as_slice).collect();
            let bites = plinth_support::tip_shaft_seat_bite_groups(&slices, frame.layer_plan).ok();
            let mut follower_reads = follower_reads.into_iter();
            let mut bites = bites.map(IntoIterator::into_iter);
            for (&at, group) in clearing.iter().zip(&groups) {
                prefetched.insert(
                    at,
                    Prefetched {
                        followers: follower_reads.by_ref().take(group.len() - 1).collect(),
                        bites: bites.as_mut().and_then(Iterator::next),
                    },
                );
            }
        }
        for (at, (row, read)) in rows.into_iter().zip(reads).enumerate() {
            let prefetched = prefetched.remove(&at);
            // A row that is this round's own proposal is spent before the
            // call returns, so it is not charged; a candidate read ahead is.
            let charged = if row.own.is_some() {
                0
            } else {
                found_bytes(Some(&row.found)) + AHEAD_ENTRY_BYTES * (1 + row.found.1.len())
            };
            pending[row.slot].store_ahead(
                ledger,
                row.at,
                Ahead {
                    found: row.found,
                    read,
                    prefetched,
                    charged,
                },
            );
        }
    }
    proposals
        .iter()
        .zip(first_of)
        .map(|(proposal, &first)| {
            let ahead = pending[proposal.slot].take_ahead(ledger, proposal.at);
            after[first] = ahead.as_ref().and_then(|ahead| ahead.read);
            // The read-ahead's follower reads and bites stand for the round
            // only for the children it drew them for.
            ahead
                .filter(|ahead| ahead.found.1 == proposal.children)
                .and_then(|ahead| ahead.prefetched)
        })
        .collect()
}

/// **What the read-ahead measured of a candidate** (`Pending::ahead`), for the
/// path it was drawn on and the children that follow it: the mover's trial
/// read, and where that read clears, [`Prefetched`].
struct Ahead {
    found: std::rc::Rc<Found>,
    read: Option<f64>,
    prefetched: Option<Prefetched>,
    /// The bytes charged for this entry.
    charged: usize,
}

/// What an entry's follower reads and bites hold per member, in bytes.
const AHEAD_ENTRY_BYTES: usize = 128;

/// What a trial arm holds besides its radius profile, in bytes.
const TRIAL_ARM_BYTES: usize = 256;

/// The bytes a trial solid holds.
fn trial_bytes(arm: &SweepArm) -> usize {
    8 * arm.radius_profile_mm.len() + TRIAL_ARM_BYTES
}

/// Count a window candidate left for the round loop for want of room (tests).
fn refused_for_room() {
    #[cfg(test)]
    TEST_REFUSED_FOR_ROOM.with(|cell| cell.set(cell.get() + 1));
}

/// What the round asks of a mover that clears, read ahead for every member of
/// its group (the mover first, then its children as the proposal lays them
/// out); each read is a pure function of the member's trial solid, so it
/// stands for whichever batch the round would have carried it in, except the
/// seat bites, which depend on the batch through the lander heights
/// ([`plinth_support::tip_shaft_seat_bites_with`] checks them).
struct Prefetched {
    /// Each child's trial solid read against what it stood at
    /// ([`admitted_at_least`]): one read per child, in order.
    followers: Vec<Option<f64>>,
    /// The group's seat bites, each with the lander height it was read with;
    /// `None` when the lowering refused the request.
    bites: Option<Vec<plinth_support::SeatBiteRead>>,
}

/// **The open jobs' deep scan** (lane S2, 2026-10-15).
///
/// Measured on Deinonychus at 17b543d (S0): 82 % of the rounds of
/// [`apply`] have ONE open job, 99.97 % of rounds commit nothing, and one arm
/// walks a 138,302-candidate list in which some 90,000 candidates pass the
/// path test and every one is refused by its clearance read, one candidate
/// per round, in four `apply` calls of every sweep.
///
/// # Why rounds that commit nothing can be settled ahead
///
/// A round that does not commit changes nothing any later round reads: the
/// struts, the column index and its generation, the plate roots and the lock
/// flags change only in a commit, and the caches a round fills (the standing
/// reads, the stored path tests and reads, the ledger) hold pure functions of
/// that unchanged field. So until some job's proposal gets past boundary 2's
/// first test (the clearance test), every job walks against a FROZEN field,
/// one proposal per round in round order, and each candidate's fate in that
/// walk is a pure function of the candidate ([`Verdict`]):
///
/// * the path test ([`propose`]) refuses it: boundary 1 counts
///   `candidates_refused_path` and takes the next candidate in the same
///   round;
/// * it passes, and the clearance test refuses it: its mover's trial solid
///   (read as [`read_movers`] reads it, bounded by [`admitted_at_least`]),
///   or, where the mover is hidden or clears, a child's trial solid (read as
///   the round reads its followers), does not reach its bound, after any
///   members the test counts as hidden. Boundary 2 counts
///   `candidates_refused_clearance` and those `candidates_hidden`, and
///   nothing else of the round reaches the field: the seat bites and the
///   tip-contact reads a mover that clears is also given are pure reads
///   (the contact read fills memos of pure values), consumed only by the
///   tests after the clearance test.
///
/// The scan evaluates those pure functions for a chunk of candidates at a
/// time, in parallel (the path tests and trial solids on the pool, the
/// reads in kernel batches, each row's answer independent of the batch that
/// carries it, as the read-ahead already relies on). With several jobs open,
/// round `r` asks each job its `r`-th proposal, so the rounds that commit
/// nothing are the first `R`, `R` the fewest proposals any job has refused
/// before its first that passes ([`scan_open_jobs`]); the order in which the
/// rounds would have interleaved the jobs' refusals reaches only the counts,
/// which are sums. Each job's walk is then taken in candidate order exactly
/// as the rounds would take it: the same three counts, and the cursor left
/// on the job's proposal in round `R + 1`, the round that runs as before.
/// That proposal's path test and mover's read are stored at the current
/// generation (so the round takes them as it takes the read-ahead's). With
/// no job ever passing, every cursor is left at its
/// list's end, as the rounds would leave it. The verdicts past the cursors
/// are kept ([`ScanStore`]) for the next scan and taken only at the same
/// generation, from the very candidate the rounds left each cursor on.
///
/// **Bytes.** A stored refusal keeps its hidden count and its mover's read,
/// no paths (a round that proposes it asks its path test again); a stored
/// pass keeps its paths. Every stored verdict is charged to the pass's
/// [`Ledger`] (`PLINTH_SWEEP_LIPSEAT_READ_AHEAD_BYTES`), with the read-ahead's
/// entries, and a verdict that would take it past its limit is not stored.
/// A chunk's working set (trial solids, every descendant's path and solid,
/// frame rows) is held to a share of a quarter of the limit at an upper bound
/// per candidate ([`scan_candidate_bytes`]). When nothing more fits, the
/// scan takes the rounds its stores already prove, which frees what they
/// held, and settles on; with none proven, the rounds go on from the cursors
/// as they did before the scan existed. A job's stores go when its job ends.
///
/// Nothing else a skipped round does
/// reaches the field: its perf counters, the read-ahead's window and stores
/// (which only ever save asking again) and the S0 tallies. Chunking, the
/// chunk size and the thread count decide only how much is evaluated ahead,
/// never what is counted or stored.
///
/// `PLINTH_SWEEP_VERIFY_LIPSEAT=1` asks every candidate the scan settles
/// again, when the walk takes its verdict, one at a time ([`verdict_alone`]:
/// [`propose`] and one-row reads at the field as it then stands), and panics
/// on a difference. `PLINTH_SWEEP_LIPSEAT_SCAN=0` turns the scan off (a
/// diagnostic: the output does not depend on it).
fn lip_seat_scan() -> bool {
    #[cfg(test)]
    if TEST_NO_SCAN.with(std::cell::Cell::get) {
        return false;
    }
    static SCAN: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SCAN.get_or_init(|| std::env::var_os("PLINTH_SWEEP_LIPSEAT_SCAN").is_none_or(|value| value != "0"))
}

/// The candidates of a job's first scan chunk (and its first after a commit):
/// a walk that stops early evaluates at most this many past its stop, and a
/// long walk doubles from here ([`ScanStore::next_len`]).
pub(crate) const SCAN_FIRST: usize = 16;

/// The kernel reads (movers and children) one scan chunk aims at. S0 item 2
/// measured 17.7-20.5 us of thread time per lip-seat read row in a traced
/// run (an upper bound; ~140 us of fixed cost per batch, latency-bound), so
/// a chunk of this many rows is a few milliseconds over 12 threads: the
/// fixed cost is a few per cent of it, and a commit throws little away. Rows,
/// not time, so the work a run does (its `Work` perf counters, and the reads
/// the scan makes) is the same at every thread count and on every run.
const SCAN_CHUNK_ROWS: f64 = 4096.0;

impl ScanStore {
    /// **How many candidates the job's next scan chunk takes.** A chunk at
    /// most doubles the one before (so the work a commit throws away is at
    /// most the work before it, plus [`SCAN_FIRST`]), aims at
    /// [`SCAN_CHUNK_ROWS`] kernel rows at the rate the job's chunks measured,
    /// and holds its working set to `working` bytes at `per_candidate`, an
    /// upper bound on what one candidate's evaluation holds
    /// ([`scan_candidate_bytes`]), the first chunk included. `0`: not even
    /// one candidate fits. A schedule only: the scan's answers do not depend
    /// on it.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a chunk length from a measured rate: positive, finite, far below 2^52"
    )]
    fn next_len(&self, working: usize, per_candidate: usize) -> usize {
        let by_bytes = working / per_candidate.max(1);
        let Some(previous) = self.chunk else {
            return SCAN_FIRST.min(by_bytes);
        };
        let doubled = previous.saturating_mul(2);
        let by_rows = self
            .rows_per_candidate
            .map_or(doubled, |rate| (SCAN_CHUNK_ROWS / rate.max(1.0e-3)) as usize)
            .max(SCAN_FIRST);
        doubled.min(by_rows).min(by_bytes)
    }

    #[expect(clippy::cast_precision_loss, reason = "a rate from counts far below 2^52")]
    fn measured(&mut self, candidates: usize, rows: usize) {
        if candidates > 0 {
            self.rows_per_candidate = Some(rows as f64 / candidates as f64);
        }
    }
}

/// The bytes one frame row of a kernel read holds besides the solid (seat,
/// base, radius, zones; `solid_frame`).
const FRAME_ROW_BYTES: usize = 128;

/// **An upper bound on the bytes one candidate's scan evaluation holds**
/// ([`scan_chunk`]): the mover's trial solid (a radius per layer up to its
/// top), its path and every descendant's (a cell per layer), each
/// descendant's trial solid, a frame row per kernel read, and the verdict.
/// Each length is bounded by the member's own top layer, so the bound holds
/// for every candidate of the job.
fn scan_candidate_bytes(job: &Pending, struts: &[SweepStrut]) -> usize {
    let layers = |member: usize| struts[member].k_top.saturating_add(1);
    let member = |member: usize| {
        (8 + 4) * layers(member) + TRIAL_ARM_BYTES + FRAME_ROW_BYTES + std::mem::size_of::<Follow>()
    };
    64 + std::mem::size_of::<Verdict>()
        + member(job.index)
        + job.children.iter().map(|&child| member(child)).sum::<usize>()
}

/// **One candidate's fate in a job's walk**, as the rounds settle it
/// against the frozen field ([`lip_seat_scan`]).
#[derive(Debug)]
enum Verdict {
    /// The path test refuses it: `candidates_refused_path`.
    Path,
    /// The clearance test refuses it: `candidates_refused_clearance`, after
    /// `hidden` members the test counted as standing in a hidden cavity
    /// (`candidates_hidden`; the test stops at the first member that fails).
    /// It keeps its mover's read (for a round that proposes it) and none of
    /// its paths: a round that proposes it asks its path test again.
    Clearance { hidden: usize, read: Option<f64> },
    /// It gets past the clearance test: the walk stops here, and the round
    /// takes it with this path test and this read of its mover.
    Clears { found: Box<Found>, read: Option<f64> },
}

impl Verdict {
    /// Equal, the reads compared bit for bit (the shadow check).
    fn same(&self, other: &Self) -> bool {
        let read_bits = |read: &Option<f64>| read.map(f64::to_bits);
        match (self, other) {
            (Self::Path, Self::Path) => true,
            (Self::Clearance { hidden: a, read: ra }, Self::Clearance { hidden: b, read: rb }) => {
                a == b && read_bits(ra) == read_bits(rb)
            }
            (Self::Clears { found: a, read: ra }, Self::Clears { found: b, read: rb }) => {
                a == b && read_bits(ra) == read_bits(rb)
            }
            _ => false,
        }
    }

    /// The bytes the verdict holds in a [`ScanStore`].
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Clears { found, .. } => found_bytes(Some(found)),
                Self::Path | Self::Clearance { .. } => 0,
            }
    }
}

/// **The verdicts a job's scan holds past its cursor**: those of
/// candidates `from..`, settled against the field at `generation` and valid
/// at that generation only (a commit bumps it, [`StampedIndex::stamp_strut`];
/// nothing else a round does changes what a verdict reads). `chunk` is the
/// length of the job's last chunk, and the rates the next chunk is sized
/// from ([`ScanStore::next_len`]). The counts a round start
/// asks of it ([`Pending::stored_walk`]) are kept as verdicts come and go, so
/// asking costs nothing however long the store is. `bytes` is what the held
/// verdicts hold ([`Verdict::bytes`]); the job charges it to the pass's
/// [`Ledger`] ([`Pending::scan_push`]), so the store never takes the ledger
/// past its limit.
#[derive(Default)]
struct ScanStore {
    generation: u64,
    from: usize,
    verdicts: std::collections::VecDeque<Verdict>,
    chunk: Option<usize>,
    rows_per_candidate: Option<f64>,
    /// Clearance verdicts pushed and popped since the store was last cleared.
    pushed_refused: usize,
    popped_refused: usize,
    /// For each `Clears` verdict held, the clearance verdicts pushed before it.
    clears: std::collections::VecDeque<usize>,
    bytes: usize,
}

impl ScanStore {
    /// Drop every verdict; returns the bytes they held.
    fn clear(&mut self) -> usize {
        self.verdicts.clear();
        self.clears.clear();
        self.pushed_refused = 0;
        self.popped_refused = 0;
        std::mem::take(&mut self.bytes)
    }

    fn push(&mut self, verdict: Verdict) {
        match verdict {
            Verdict::Path => {}
            Verdict::Clearance { .. } => self.pushed_refused += 1,
            Verdict::Clears { .. } => self.clears.push_back(self.pushed_refused),
        }
        self.bytes += verdict.bytes();
        self.verdicts.push_back(verdict);
    }

    /// The verdict of candidate `from`, taken; `from` moves on.
    fn pop(&mut self) -> Option<Verdict> {
        let verdict = self.verdicts.pop_front()?;
        match verdict {
            Verdict::Path => {}
            Verdict::Clearance { .. } => self.popped_refused += 1,
            Verdict::Clears { .. } => {
                self.clears.pop_front();
            }
        }
        self.bytes = self.bytes.saturating_sub(verdict.bytes());
        self.from += 1;
        Some(verdict)
    }

    /// The candidate after the last verdict held.
    fn end(&self) -> usize {
        self.from + self.verdicts.len()
    }
}

/// The clearance test of one candidate that passed the path test, as
/// boundary 2's first test runs it (`apply`, "THE SOLID"): the mover first,
/// then each child, a member in a hidden cavity counted and passed, the first
/// member whose read does not reach its bound refusing the candidate. `None`
/// in `children` (the mover is refused): the children are never asked.
fn clearance_verdict(
    found: Found,
    mover_hidden: bool,
    mover_read: Option<f64>,
    mover_bound: f64,
    (child_hidden, reads, bounds): (&[bool], &[Option<f64>], &[f64]),
) -> Verdict {
    if !mover_hidden && !mover_read.is_some_and(|now| now >= mover_bound) {
        return Verdict::Clearance {
            hidden: 0,
            read: mover_read,
        };
    }
    debug_assert_eq!(child_hidden.len(), found.1.len(), "every child of a mover that passes is asked");
    let mut hidden = usize::from(mover_hidden);
    for ((&child_hidden, read), &bound) in child_hidden.iter().zip(reads).zip(bounds) {
        if child_hidden {
            hidden += 1;
        } else if !read.is_some_and(|now| now >= bound) {
            #[cfg(test)]
            TEST_CHILD_REFUSED.with(|cell| cell.set(cell.get() + 1));
            return Verdict::Clearance {
                hidden,
                read: mover_read,
            };
        }
    }
    Verdict::Clears {
        found: Box::new(found),
        read: mover_read,
    }
}

/// What a scan chunk made of one candidate that passed the path test (its
/// trial solid goes to the read batch).
struct ScanPass {
    found: Found,
    /// Whether the mover's proposed axis stands wholly in a hidden cavity.
    hidden: bool,
    /// The trial solid's radius, as the hidden test reads it.
    radius_mm: f64,
}

/// One child's trial solid and hidden flag for a candidate whose mover the
/// clearance test passes: the children's trial solids on the moved axis, and
/// each child's hidden flag ([`proposal_hidden`]: a child only where the
/// mover is hidden too).
fn children_trial(
    context: &SlendernessContext<'_>,
    plate_z_mm: f64,
    struts: &[SweepStrut],
    arms: &[SweepArm],
    mover: usize,
    candidate: Candidate,
    pass: &ScanPass,
) -> (Vec<SweepArm>, Vec<bool>) {
    let solids: Vec<SweepArm> = pass
        .found
        .1
        .iter()
        .map(|(child, _, path)| {
            let strut = &struts[*child];
            trial_arm(context, plate_z_mm, struts, arms, *child, strut.landing, strut.k_bottom, path)
        })
        .collect();
    let hidden = if pass.hidden {
        proposal_hidden(
            &context.history.hidden,
            ProposedAxis {
                k_bottom: candidate.k_bottom,
                k_top: struts[mover].k_top,
                base_cell: Some(candidate.cell),
                seat_cell: struts[mover].cell,
                path: &pass.found.0,
                radius_mm: pass.radius_mm,
            },
            pass.found.1.iter().zip(&solids).map(|((child, _, path), solid)| ProposedAxis {
                k_bottom: struts[*child].k_bottom,
                k_top: struts[*child].k_top,
                base_cell: None,
                seat_cell: struts[*child].cell,
                path,
                radius_mm: crate::cavity::arm_radius_mm(solid),
            }),
        )
        .split_off(1)
    } else {
        vec![false; solids.len()]
    };
    (solids, hidden)
}

/// The mover's path test, trial solid and hidden flag for one candidate.
#[expect(
    clippy::too_many_arguments,
    reason = "the field and its index, the mover, its children and the candidate"
)]
fn scan_pass(
    context: &SlendernessContext<'_>,
    plate_z_mm: f64,
    index: &StampedIndex,
    struts: &[SweepStrut],
    arms: &[SweepArm],
    mover: usize,
    children: &[usize],
    candidate: Candidate,
    clearance: bool,
) -> Option<(ScanPass, SweepArm)> {
    let found = propose_now(context, index, struts, mover, candidate, children, clearance)?;
    let solid = trial_arm(
        context,
        plate_z_mm,
        struts,
        arms,
        mover,
        candidate.landing,
        candidate.k_bottom,
        &found.0,
    );
    let radius_mm = crate::cavity::arm_radius_mm(&solid);
    let hidden = proposal_hidden(
        &context.history.hidden,
        ProposedAxis {
            k_bottom: candidate.k_bottom,
            k_top: struts[mover].k_top,
            base_cell: Some(candidate.cell),
            seat_cell: struts[mover].cell,
            path: &found.0,
            radius_mm,
        },
        std::iter::empty(),
    )[0];
    Some((ScanPass { found, hidden, radius_mm }, solid))
}

/// The bound a mover's read must reach ([`read_movers`]'s, and boundary 2's
/// first-member test's), for a candidate landing on `landing`.
fn mover_bound(
    frame: &ReadFrame<'_>,
    standing_reads: &StandingReads,
    struts: &[SweepStrut],
    mover: usize,
    landing: Landing,
    clearance: bool,
) -> f64 {
    admitted_at_least(
        clearance || !matches!(landing, Landing::Strut { .. }),
        standing_reads.before(mover, struts),
        frame.tier,
    )
}

/// **One scan chunk**: the verdicts of `candidates`, in order, against the
/// frozen field. The path tests, trial solids and hidden tests run on the
/// pool; the movers' reads are one kernel batch, and the children of every
/// mover that passes are read in one more (each row's answer independent of
/// the batch that carries it). Returns the verdicts and the bytes the chunk
/// held (its trial solids and paths).
#[expect(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "the read frame, the field and its index, the one open job and the pass's caches"
)]
fn scan_chunk(
    frame: &ReadFrame<'_>,
    context: &SlendernessContext<'_>,
    index: &StampedIndex,
    struts: &[SweepStrut],
    arms: &[SweepArm],
    mover: usize,
    children: &[usize],
    candidates: &[Candidate],
    clearance: bool,
    standing_reads: &mut StandingReads,
    s0: &mut S0Apply,
) -> (Vec<Verdict>, usize, usize) {
    use rayon::prelude::*;
    let plate_z_mm = frame.plate_z_mm;
    let judged: Vec<Option<(ScanPass, SweepArm)>> = candidates
        .par_iter()
        .map(|&candidate| scan_pass(context, plate_z_mm, index, struts, arms, mover, children, candidate, clearance))
        .collect();
    // The movers' reads, one batch; the mover as it stands bounds each, read
    // once per pass as the round reads it ([`StandingReads`]).
    let mut passes: Vec<Option<ScanPass>> = Vec::with_capacity(judged.len());
    let mut rows: Vec<usize> = Vec::new();
    let mut solids: Vec<SweepArm> = Vec::new();
    let mut on_model: Vec<bool> = Vec::new();
    let mut bytes = 0_usize;
    for (at, entry) in judged.into_iter().enumerate() {
        bytes += std::mem::size_of::<Verdict>();
        passes.push(entry.map(|(pass, solid)| {
            bytes += trial_bytes(&solid) + found_bytes(Some(&pass.found)) + FRAME_ROW_BYTES;
            rows.push(at);
            on_model.push(matches!(candidates[at].landing, Landing::Model { .. }));
            solids.push(solid);
            pass
        }));
    }
    if rows.is_empty() {
        return (passes.iter().map(|_| Verdict::Path).collect(), 0, bytes);
    }
    standing_reads.read(frame, arms, struts, &[mover], true);
    let bounds: Vec<f64> = rows
        .iter()
        .map(|&at| mover_bound(frame, standing_reads, struts, mover, candidates[at].landing, clearance))
        .collect();
    let read_started = s0.on.then(std::time::Instant::now);
    let reads = {
        let _site = crate::perf::clear_site(crate::perf::ClearSite::LipScan);
        frame.solid_at_least(&solids, &on_model, &bounds)
    };
    if let Some(read_started) = read_started {
        s0.scan.read_ns += s0_ns(read_started);
    }
    s0_memo_exact(frame, context, &solids, &on_model, &bounds);
    drop(solids);
    // The children of every mover that passes (hidden, or its read reaches
    // its bound), drawn on the moved axis and read in one batch, each bounded
    // by what it stood at, as the round reads its followers.
    let mover_passes: Vec<bool> = rows
        .iter()
        .enumerate()
        .map(|(row, &at)| {
            passes[at].as_ref().is_some_and(|pass| pass.hidden) || reads[row].is_some_and(|now| now >= bounds[row])
        })
        .collect();
    let asked: Vec<usize> = (0..rows.len())
        .filter(|&row| mover_passes[row] && passes[rows[row]].as_ref().is_some_and(|pass| !pass.found.1.is_empty()))
        .collect();
    let mut trials: Vec<(Vec<SweepArm>, Vec<bool>)> = asked
        .par_iter()
        .map(|&row| {
            let at = rows[row];
            let pass = passes[at].as_ref().expect("a row is a pass");
            children_trial(context, plate_z_mm, struts, arms, mover, candidates[at], pass)
        })
        .collect();
    bytes += trials
        .iter()
        .flat_map(|(solids, _)| solids.iter())
        .map(|solid| trial_bytes(solid) + FRAME_ROW_BYTES)
        .sum::<usize>();
    let followers: Vec<usize> = asked
        .iter()
        .flat_map(|&row| {
            passes[rows[row]]
                .as_ref()
                .map(|pass| pass.found.1.iter().map(|(child, _, _)| *child).collect::<Vec<_>>())
                .unwrap_or_default()
        })
        .collect();
    let (child_reads, child_bounds) = if followers.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        standing_reads.read(frame, arms, struts, &followers, false);
        let child_bounds: Vec<f64> = followers
            .iter()
            .map(|&child| admitted_at_least(false, standing_reads.before(child, struts), frame.tier))
            .collect();
        // Moved, not copied: only the hidden flags are read after this.
        let child_solids: Vec<SweepArm> = trials.iter_mut().flat_map(|(solids, _)| solids.drain(..)).collect();
        let read_started = s0.on.then(std::time::Instant::now);
        let child_reads = {
            let _site = crate::perf::clear_site(crate::perf::ClearSite::LipScan);
            frame.solid_at_least(&child_solids, &vec![false; child_solids.len()], &child_bounds)
        };
        if let Some(read_started) = read_started {
            s0.scan.read_ns += s0_ns(read_started);
        }
        (child_reads, child_bounds)
    };
    let read_rows = rows.len() + followers.len();
    if s0.on {
        s0.scan.passes += rows.len() as u64;
        s0.scan.child_rows += followers.len() as u64;
    }
    // Each candidate's verdict, in candidate order.
    let mut row_of = rows.iter().copied().enumerate().map(|(row, at)| (at, row)).peekable();
    let mut asked_of = asked.iter().copied().zip(&trials).peekable();
    let mut child_at = 0_usize;
    let mut verdicts: Vec<Verdict> = Vec::with_capacity(passes.len());
    for (at, pass) in passes.into_iter().enumerate() {
        let Some(pass) = pass else {
            verdicts.push(Verdict::Path);
            continue;
        };
        let (_, row) = row_of.next().filter(|(of, _)| *of == at).expect("rows in candidate order");
        let children_read = match asked_of.peek() {
            Some(&(asked_row, (_, hidden))) if asked_row == row => {
                asked_of.next();
                let n = hidden.len();
                let span = child_at..child_at + n;
                child_at += n;
                Some((hidden.as_slice(), &child_reads[span.clone()], &child_bounds[span]))
            }
            _ => None,
        };
        verdicts.push(clearance_verdict(
            pass.found,
            pass.hidden,
            reads[row],
            bounds[row],
            // A mover that is refused, or passes with no child: no child read.
            children_read.unwrap_or((&[], &[], &[])),
        ));
    }
    (verdicts, read_rows, bytes)
}

/// **One candidate's verdict, asked alone** (the shadow check of
/// [`scan_chunk`]): its path test, its mover's read and its children's reads,
/// each in a batch of one, at the field as it stands.
#[expect(
    clippy::too_many_arguments,
    reason = "the read frame, the field and its index, the one open job and the pass's caches"
)]
fn verdict_alone(
    frame: &ReadFrame<'_>,
    context: &SlendernessContext<'_>,
    index: &StampedIndex,
    struts: &[SweepStrut],
    arms: &[SweepArm],
    mover: usize,
    children: &[usize],
    candidate: Candidate,
    clearance: bool,
    standing_reads: &mut StandingReads,
) -> Verdict {
    let Some((pass, solid)) =
        scan_pass(context, frame.plate_z_mm, index, struts, arms, mover, children, candidate, clearance)
    else {
        return Verdict::Path;
    };
    standing_reads.read(frame, arms, struts, &[mover], true);
    let bound = mover_bound(frame, standing_reads, struts, mover, candidate.landing, clearance);
    let on_model = matches!(candidate.landing, Landing::Model { .. });
    let read = frame.solid_at_least(std::slice::from_ref(&solid), &[on_model], &[bound])[0];
    if !pass.hidden && !read.is_some_and(|now| now >= bound) {
        return clearance_verdict(pass.found, false, read, bound, (&[], &[], &[]));
    }
    let (solids, hidden) = children_trial(context, frame.plate_z_mm, struts, arms, mover, candidate, &pass);
    let followers: Vec<usize> = pass.found.1.iter().map(|(child, _, _)| *child).collect();
    standing_reads.read(frame, arms, struts, &followers, false);
    let bounds: Vec<f64> = followers
        .iter()
        .map(|&child| admitted_at_least(false, standing_reads.before(child, struts), frame.tier))
        .collect();
    let reads: Vec<Option<f64>> = solids
        .iter()
        .zip(&bounds)
        .map(|(solid, &bound)| frame.solid_at_least(std::slice::from_ref(solid), &[false], &[bound])[0])
        .collect();
    clearance_verdict(pass.found, pass.hidden, read, bound, (&hidden, &reads, &bounds))
}

/// What a job's stored verdicts say of the rounds ahead: how many of its
/// proposals the clearance test refuses before the first one that passes it
/// (`clears`), or, with none stored, how many it refuses in what is stored
/// (`refused`) and whether that reaches the end of its list (`ends`).
struct StoredWalk {
    refused: usize,
    clears: Option<usize>,
    ends: bool,
}

impl Pending {
    /// Line the job's [`ScanStore`] up with its cursor: verdicts of another
    /// generation are dropped (and the chunk length starts again), and so are
    /// those of candidates the rounds have walked since the store was filled.
    fn align_scan(&mut self, ledger: &mut Ledger, generation: u64) {
        let held = self.scan.bytes;
        let store = &mut self.scan;
        if store.generation != generation {
            store.clear();
            store.chunk = None;
            store.generation = generation;
            store.from = self.next;
        } else if self.next < store.from || self.next > store.end() {
            store.clear();
            store.from = self.next;
        } else {
            while store.from < self.next {
                store.pop();
            }
        }
        let freed = held.saturating_sub(self.scan.bytes);
        self.credit(ledger, freed);
    }

    /// Store `verdict` at the end of the job's scan, charged to the ledger;
    /// `false` (the verdict dropped) where the ledger has no room for it. A
    /// dropped verdict is only work to do again: the store simply ends
    /// before it.
    fn scan_push(&mut self, ledger: &mut Ledger, verdict: Verdict) -> bool {
        let bytes = verdict.bytes();
        if !ledger.has_room(bytes) {
            return false;
        }
        self.scan.push(verdict);
        self.charge(ledger, bytes);
        true
    }

    /// Take the verdict at the scan's cursor, crediting the ledger.
    fn scan_pop(&mut self, ledger: &mut Ledger) -> Option<Verdict> {
        let verdict = self.scan.pop()?;
        self.credit(ledger, verdict.bytes());
        Some(verdict)
    }

    fn stored_walk(&self) -> StoredWalk {
        let store = &self.scan;
        let clears = store.clears.front().map(|&before| before - store.popped_refused);
        StoredWalk {
            refused: clears.unwrap_or(store.pushed_refused - store.popped_refused),
            clears,
            ends: clears.is_none() && store.end() >= self.candidates.len(),
        }
    }
}

/// **The open jobs' walks, settled ahead** (see [`lip_seat_scan`]).
///
/// `slots` are the jobs that can propose this round (open, not locked, with
/// candidates left). Each job's verdicts are settled from its cursor, in
/// chunks, until the number of rounds that commit nothing is known: `R`, the
/// fewest proposals any job has refused before its first proposal that
/// passes the clearance test (rounds while every proposal of every job is
/// refused there). For each job, its first `R` proposals and every path
/// refusal up to its next proposal are counted as the rounds count them, and
/// its cursor is left on that proposal: the round that proposes it runs as
/// before, taking the proposal's path test and mover's read from what the
/// scan stored for it, as it takes the read-ahead's. With no job ever passing, every
/// list is walked to its end. Verdicts past the cursors stay in the jobs'
/// [`ScanStore`]s, at this generation only. Returns `false` if the pose was
/// cancelled between chunks (the caller returns at once, as the round loop
/// does).
#[expect(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "the read frame, the field and its index, the open jobs, the pass's caches and \
              counters; one walk per job in chunks"
)]
fn scan_open_jobs(
    frame: &ReadFrame<'_>,
    context: &SlendernessContext<'_>,
    index: &StampedIndex,
    struts: &[SweepStrut],
    arms: &[SweepArm],
    pending: &mut [Pending],
    slots: &[usize],
    clearance: bool,
    standing_reads: &mut StandingReads,
    ledger: &mut Ledger,
    outcome: &mut LipSeatOutcome,
    s0: &mut S0Apply,
) -> bool {
    let started = s0.on.then(std::time::Instant::now);
    let generation = index.generation();
    let look = ScanLook {
        frame,
        context,
        index,
        struts,
        arms,
        clearance,
        verify: verify_lip_seat(),
        generation,
    };
    for &slot in slots {
        pending[slot].align_scan(ledger, generation);
    }
    // One chunk's working set is held to a share of a read-ahead call's
    // bytes; what the stores hold, to the ledger's limit.
    let working = ledger.limit / 4 / slots.len().max(1);
    let mut taken = Settled::default();
    // Settle more of every job that has refused fewer proposals than the
    // least `R` known so far, until `R` is known (`None`: no job ever passes,
    // and every list is walked to its end).
    let settled_rounds: Option<Option<usize>> = loop {
        let walks: Vec<StoredWalk> = slots.iter().map(|&slot| pending[slot].stored_walk()).collect();
        let bound = walks.iter().filter_map(|walk| walk.clears).min();
        let short: Vec<usize> = slots
            .iter()
            .zip(&walks)
            // A job's proposal in round `R + 1` is settled too, so the round
            // takes it from the store.
            .filter(|(_, walk)| walk.clears.is_none() && !walk.ends && bound.is_none_or(|bound| walk.refused <= bound))
            .map(|(&slot, _)| slot)
            .collect();
        if short.is_empty() {
            break Some(bound);
        }
        if crate::cancel::cancelled() {
            return false;
        }
        let mut stored = false;
        for slot in short {
            let job = &mut pending[slot];
            let from = job.scan.end();
            // Held to the working-set share at the job's own upper bound per
            // candidate, and to what the ledger can still hold of verdicts.
            let room = ledger.limit.saturating_sub(ledger.retained) / std::mem::size_of::<Verdict>();
            let take = job.scan.next_len(working, scan_candidate_bytes(job, struts)).min(room);
            if take == 0 {
                continue;
            }
            let end = from.saturating_add(take).min(job.candidates.len());
            let (verdicts, chunk_rows, chunk_bytes) = scan_chunk(
                frame,
                context,
                index,
                struts,
                arms,
                job.index,
                &job.children,
                &job.candidates[from..end],
                clearance,
                standing_reads,
                s0,
            );
            #[cfg(test)]
            if chunk_bytes > working {
                TEST_CHUNK_OVER.with(|cell| cell.set(true));
            }
            let _ = chunk_bytes;
            job.scan.measured(end - from, chunk_rows);
            job.scan.chunk = Some(take);
            if s0.on {
                s0.scan.chunks += 1;
            }
            for verdict in verdicts {
                if !job.scan_push(ledger, verdict) {
                    break;
                }
                stored = true;
            }
        }
        if !stored {
            // **The ledger is full.** Take the rounds the stores already prove
            // (every job's proposals up to the fewest any job has settled, or
            // its first that passes), which frees what they held, and settle
            // on. With none proven, the rounds go on from the cursors as they
            // did before the scan existed.
            #[cfg(test)]
            TEST_FALLBACKS.with(|cell| cell.set(cell.get() + 1));
            let proven = walks
                .iter()
                .map(|walk| walk.clears.unwrap_or(if walk.ends { usize::MAX } else { walk.refused }))
                .min()
                .unwrap_or(0);
            let proven = if proven == usize::MAX { 0 } else { proven };
            for &slot in slots {
                take_settled(&look, &mut pending[slot], ledger, standing_reads, Some(proven), false, &mut taken, s0);
            }
            if proven == 0 {
                break None;
            }
        }
    };
    // `None`: the fallback ended the scan with nothing proven to take.
    if let Some(rounds) = settled_rounds {
        for &slot in slots {
            take_settled(&look, &mut pending[slot], ledger, standing_reads, rounds, true, &mut taken, s0);
        }
    }
    outcome.candidates_refused_path += taken.path;
    outcome.candidates_refused_clearance += taken.clearance;
    outcome.candidates_hidden += taken.hidden;
    #[cfg(test)]
    TEST_SCANNED.with(|cell| cell.set(cell.get() + taken.path + taken.clearance));
    if let Some(started) = started {
        s0.scan.scans += 1;
        s0.scan.rounds += settled_rounds.flatten().map_or(0, |rounds| rounds as u64);
        s0.scan.ref_path += taken.path as u64;
        s0.scan.ref_clear += taken.clearance as u64;
        s0.scan.ns += s0_ns(started);
    }
    true
}

/// What [`scan_open_jobs`] reads the frozen field through.
struct ScanLook<'a, 'f> {
    frame: &'a ReadFrame<'f>,
    context: &'a SlendernessContext<'f>,
    index: &'a StampedIndex,
    struts: &'a [SweepStrut],
    arms: &'a [SweepArm],
    clearance: bool,
    verify: bool,
    generation: u64,
}

/// The refusals [`take_settled`] counted, as the rounds count them.
#[derive(Default)]
struct Settled {
    path: usize,
    clearance: usize,
    hidden: usize,
}

/// **Take one job's settled rounds**: its first `rounds` proposals (each
/// refused at the clearance test) and every path refusal up to its next
/// proposal are counted, and its cursor moves past them (`None`: the whole
/// list, which then holds no proposal that passes). With `hand`, the next
/// proposal is taken too and left for the round that runs, with its path
/// test and its mover's read stored at the current generation (as the
/// read-ahead stores them); the round counts it. Without `hand` the cursor
/// stops on it.
#[expect(
    clippy::too_many_arguments,
    reason = "the field, one job, the pass's caches and counters, and what to take"
)]
fn take_settled(
    look: &ScanLook<'_, '_>,
    job: &mut Pending,
    ledger: &mut Ledger,
    standing_reads: &mut StandingReads,
    rounds: Option<usize>,
    hand: bool,
    settled: &mut Settled,
    s0: &mut S0Apply,
) {
    let start = job.next;
    let mut taken = 0_usize;
    // The job's proposal in round `R + 1`: its index, path test (where the
    // verdict kept it) and mover's read, and whether it passes.
    let mut handed: Option<(usize, Option<Found>, Option<f64>, bool)> = None;
    while let Some(front) = job.scan.verdicts.front() {
        let next_round = rounds.is_some_and(|rounds| taken >= rounds);
        let proposal = matches!(front, Verdict::Clears { .. })
            || matches!(front, Verdict::Clearance { .. }) && next_round;
        if proposal && !hand {
            break;
        }
        let at = job.scan.from;
        let Some(verdict) = job.scan_pop(ledger) else {
            break;
        };
        if look.verify {
            let fresh = verdict_alone(
                look.frame,
                look.context,
                look.index,
                look.struts,
                look.arms,
                job.index,
                &job.children,
                job.candidates[at],
                look.clearance,
                standing_reads,
            );
            assert!(
                verdict.same(&fresh),
                "PLINTH_SWEEP_VERIFY_LIPSEAT: a scan verdict differs from the candidate asked alone\n candidate {at}: scan {verdict:?}\n alone {fresh:?}"
            );
        }
        match verdict {
            Verdict::Path => settled.path += 1,
            Verdict::Clearance { read, .. } if next_round => {
                handed = Some((at, None, read, false));
                break;
            }
            Verdict::Clearance { hidden, .. } => {
                settled.clearance += 1;
                settled.hidden += hidden;
                taken += 1;
            }
            Verdict::Clears { found, read } => {
                debug_assert_eq!(rounds, Some(taken), "a job passes only in round R + 1");
                handed = Some((at, Some(*found), read, true));
                break;
            }
        }
    }
    let next = handed.as_ref().map_or(job.scan.from, |(at, _, _, _)| *at);
    // What the read-ahead stored for the candidates the walk settled is never
    // asked again.
    job.drop_settled(ledger, start, next);
    job.next = next;
    if s0.on {
        s0.scan.cands += (next - start) as u64;
        s0.scan.stops += u64::from(handed.as_ref().is_some_and(|(_, _, _, clears)| *clears));
    }
    let Some((at, found, read, _)) = handed else {
        return;
    };
    // A refused proposal kept no paths: its path test is asked again, at the
    // generation its verdict was settled at (it passed then, so it passes).
    let found = found.or_else(|| {
        propose_now(
            look.context,
            look.index,
            look.struts,
            job.index,
            job.candidates[at],
            &job.children,
            look.clearance,
        )
    });
    let Some(found) = found else {
        debug_assert!(false, "a settled proposal passes its path test");
        return;
    };
    let found = std::rc::Rc::new(found);
    let charged = found_bytes(Some(&found)) + AHEAD_ENTRY_BYTES * (1 + found.1.len());
    if ledger.has_room(charged + found_bytes(Some(&found))) {
        job.store_tested(ledger, at, look.generation, Some(std::rc::Rc::clone(&found)));
        job.store_ahead(
            ledger,
            at,
            Ahead {
                found,
                read,
                prefetched: None,
                charged,
            },
        );
    }
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
    let mut s0 = S0Apply::start(outcome);
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
    let mut index = StampedIndex::new(index);
    if s0.on {
        index.s0 = Some(S0Gens {
            cell: vec![0; context.grid.cells()],
            strut: vec![0; struts.len()],
        });
    }
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

    // Which members stand on the plate or the model through a chain of
    // landings that holds, as the field stands as this pass begins (one
    // `O(struts)` walk, [`crate::landing_check::check_landings`]): the
    // kept-arm test below reads it, since an arm floating above its ground
    // (or resting on one that does) is no support to keep.
    let rooted: Vec<bool> = {
        let check = crate::landing_check::check_landings(struts, context.layer_height_mm);
        let mut rooted = vec![true; struts.len()];
        for id in &check.unrooted {
            if let Some(flag) = rooted.get_mut(*id as usize) {
                *flag = false;
            }
        }
        rooted
    };

    // ---- stage 1: price every candidate descent of every arm sent.
    let mut pending: Vec<Pending> = Vec::new();
    for (index_of, (strut, bite)) in struts.iter().zip(bites).enumerate() {
        if !trigger.sends(index_of, bite.as_ref())
            || strut.kind != StrutKind::Arm
            || strut.k_bottom == usize::MAX
        {
            continue;
        }
        // A cancelled pose stops between arms; see the round loop below.
        if crate::cancel::cancelled() {
            return Vec::new();
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
            trigger.foot_sizing(),
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
            ahead: std::collections::HashMap::new(),
            tested: std::collections::HashMap::new(),
            window: 1,
            retained: 0,
            scan: ScanStore::default(),
        });
    }

    s0.jobs(&pending);
    s0.lap(S0Part::Price);

    // ---- stages 2 and 3, in rounds: each pending arm's next candidate whose
    // path clears, then all of them measured by the lowering in one call.
    let mut moved: Vec<Relanding> = Vec::new();
    let mut standing_reads = StandingReads::default();
    let mut standing_seats = StandingSeats::default();
    let mut ledger = Ledger::new();
    let mut open: Vec<usize> = (0..pending.len()).collect();
    let mut done = vec![false; pending.len()];
    let scan = lip_seat_scan();
    while !open.is_empty() {
        // A cancelled pose stops between rounds. The answer is read by nothing:
        // the caller (`LipSeatRun::run`) checks the token right after this call
        // and ends the pose, so the moves taken so far are never sized or kept.
        if crate::cancel::cancelled() {
            return moved;
        }
        // **The rounds that commit nothing are settled ahead**
        // ([`scan_open_jobs`]): until some job's proposal passes the
        // clearance test, no round commits and the field is frozen.
        if let Some(surface) = surface.filter(|_| scan) {
            let can_propose: Vec<usize> = open
                .iter()
                .copied()
                .filter(|&slot| !locked[pending[slot].index] && pending[slot].next < pending[slot].candidates.len())
                .collect();
            if !can_propose.is_empty() {
                let frame = ReadFrame {
                    surface,
                    layer_plan,
                    plate_z_mm,
                    tier,
                };
                let lap = s0.mark;
                if !scan_open_jobs(
                    &frame,
                    context,
                    &index,
                    struts,
                    arms,
                    &mut pending,
                    &can_propose,
                    trigger.is_clearance(),
                    &mut standing_reads,
                    &mut ledger,
                    outcome,
                    &mut s0,
                ) {
                    return moved;
                }
                // The scan's time is its own row, not boundary 1's.
                if lap.is_some() {
                    s0.mark = Some(std::time::Instant::now());
                }
            }
        }
        s0.round_start(open.len());
        let mut proposals: Vec<Proposal> = Vec::new();
        for &slot in &open {
            let job = &mut pending[slot];
            if locked[job.index] {
                // Something landed on it in an earlier round.
                continue;
            }
            while job.next < job.candidates.len() {
                let at = job.next;
                let candidate = job.candidates[at];
                job.next += 1;
                // The path test the read-ahead stored at this very field
                // generation, else asked now (a stale one is dropped).
                match take_path_test(
                    context,
                    &index,
                    struts,
                    job,
                    &mut ledger,
                    at,
                    trigger.is_clearance(),
                ) {
                    Some((path, children)) => {
                        proposals.push(Proposal {
                            slot,
                            at,
                            candidate,
                            path,
                            children,
                            generation: index.generation(),
                        });
                        break;
                    }
                    None => {
                        outcome.candidates_refused_path += 1;
                        s0.b1_refused += 1;
                    }
                }
            }
        }
        s0.proposals += proposals.len() as u64;
        s0.lap(S0Part::B1);
        if proposals.is_empty() {
            s0.round_end();
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
        // Whether each proposal's members stand wholly in a hidden cavity
        // (read once per proposal; see the loop below for the rule).
        let hidden_of: Vec<Vec<bool>> = proposals
            .iter()
            .zip(&first_of)
            .map(|(proposal, &first)| {
                let index_of = pending[proposal.slot].index;
                proposal_hidden(
                    &context.history.hidden,
                    ProposedAxis {
                        k_bottom: proposal.candidate.k_bottom,
                        k_top: struts[index_of].k_top,
                        base_cell: Some(proposal.candidate.cell),
                        seat_cell: struts[index_of].cell,
                        path: &proposal.path,
                        radius_mm: crate::cavity::arm_radius_mm(&trial[first]),
                    },
                    proposal.children.iter().enumerate().map(|(offset, (child, _, path))| {
                        ProposedAxis {
                            k_bottom: struts[*child].k_bottom,
                            k_top: struts[*child].k_top,
                            base_cell: None,
                            seat_cell: struts[*child].cell,
                            path,
                            radius_mm: crate::cavity::arm_radius_mm(&trial[first + 1 + offset]),
                        }
                    }),
                )
            })
            .collect();
        s0.lap(S0Part::Trial);
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
        //
        // **Read once, in batches, and only what is asked** (speed lane B,
        // 2026-10-13). Every read here is a pure function of the member's
        // drawn solid and of whether it lands on the model, so the value does
        // not depend on the batch that carries it:
        //
        // * a member AS IT STANDS (`before`, and the drawn-solid re-read of a
        //   standing mover the widest-radius reading puts inside the model)
        //   does not change while this pass runs (`arms` is fixed, and an arm
        //   that moves leaves the pending list), so it is read once per pass
        //   ([`StandingReads`]) rather than once per round;
        // * a mover's own trial solid depends only on its candidate and the
        //   path it would take, so it is read AHEAD for the candidates that
        //   follow it in its list, against the index as it stands now, in
        //   one batch with every other arm's; a round uses a read only when
        //   its path is the one the round proposes ([`read_movers`]);
        // * a child's solid, and every member's measured seat bite, are asked
        //   only where the mover's own solid clears (the test below reads the
        //   mover first and stops at the first member that fails).
        //
        // Measured on Deinonychus (lane A, c941e69): the round loop read every
        // mover and every follower twice per round (trial and standing), one
        // or two rows per kernel call, over some 340 000 refused candidates;
        // 300 s of the 335 s run was this loop on one pose.
        let rows = trial.len();
        let mut after: Vec<Option<f64>> = vec![None; rows];
        let mut before: Vec<Option<f64>> = vec![None; rows];
        // What the read-ahead measured of each proposal's followers and bites
        // ([`Prefetched`]), where it did.
        let mut prefetched: Vec<Option<Prefetched>> = (0..proposals.len()).map(|_| None).collect();
        let mover_clears: Vec<bool> = match surface {
            None => vec![true; proposals.len()],
            Some(surface) => {
                let frame = ReadFrame {
                    surface,
                    layer_plan,
                    plate_z_mm,
                    tier,
                };
                // The movers as they stand first: what each must not come
                // nearer than bounds its trial read.
                let movers: Vec<usize> = proposals
                    .iter()
                    .map(|proposal| pending[proposal.slot].index)
                    .collect();
                standing_reads.read(&frame, arms, struts, &movers, true);
                for (&member, &first) in movers.iter().zip(&first_of) {
                    before[first] = standing_reads.before(member, struts);
                }
                s0.lap(S0Part::Standing);
                prefetched = read_movers(
                    &frame,
                    context,
                    &index,
                    struts,
                    arms,
                    &mut pending,
                    &proposals,
                    &trial,
                    &first_of,
                    trigger.is_clearance(),
                    &mut standing_reads,
                    &mut ledger,
                    &mut after,
                );
                s0.lap(S0Part::Ahead);
                let clears: Vec<bool> = proposals
                    .iter()
                    .zip(&first_of)
                    .zip(&hidden_of)
                    .map(|((proposal, &first), hidden)| {
                        hidden[0] || {
                            let new_root = !matches!(proposal.candidate.landing, Landing::Strut { .. });
                            let at_least =
                                admitted_at_least(new_root || trigger.is_clearance(), before[first], tier);
                            after[first].is_some_and(|now| now >= at_least)
                        }
                    })
                    .collect();
                // The children of the movers that clear, as they stand, then
                // their trial solids in one batch, each asked only whether it
                // comes no nearer than it stood.
                let follower_slots: Vec<usize> = proposals
                    .iter()
                    .zip(&first_of)
                    .zip(&clears)
                    .filter(|(_, &clear)| clear)
                    .flat_map(|((proposal, &first), _)| first + 1..=first + proposal.children.len())
                    .collect();
                if !follower_slots.is_empty() {
                    let followers: Vec<usize> = proposals
                        .iter()
                        .zip(&clears)
                        .filter(|(_, &clear)| clear)
                        .flat_map(|(proposal, _)| proposal.children.iter().map(|(child, _, _)| *child))
                        .collect();
                    standing_reads.read(&frame, arms, struts, &followers, false);
                    for (&slot, &member) in follower_slots.iter().zip(&followers) {
                        before[slot] = standing_reads.before(member, struts);
                    }
                    // The followers' solids, read in one batch: those the
                    // read-ahead already read for these very children are
                    // taken from it (a read is a pure function of the
                    // member's trial solid and its bound), the rest here.
                    let mut served = vec![false; rows];
                    for (((proposal, &first), pre), &clear) in proposals
                        .iter()
                        .zip(&first_of)
                        .zip(&prefetched)
                        .zip(&clears)
                    {
                        let Some(pre) = pre.as_ref().filter(|pre| clear && pre.followers.len() == proposal.children.len())
                        else {
                            continue;
                        };
                        for (offset, value) in pre.followers.iter().enumerate() {
                            after[first + 1 + offset] = *value;
                            served[first + 1 + offset] = true;
                        }
                    }
                    let read_slots = |slots: &[usize]| -> Vec<Option<f64>> {
                        if slots.is_empty() {
                            return Vec::new();
                        }
                        let follower_arms: Vec<SweepArm> =
                            slots.iter().map(|&slot| trial[slot].clone()).collect();
                        let bounds: Vec<f64> = slots
                            .iter()
                            .map(|&slot| admitted_at_least(false, before[slot], tier))
                            .collect();
                        let _site = crate::perf::clear_site(crate::perf::ClearSite::LipFollowers);
                        frame.solid_at_least(&follower_arms, &vec![false; follower_arms.len()], &bounds)
                    };
                    let todo: Vec<usize> = follower_slots
                        .iter()
                        .copied()
                        .filter(|&slot| !served[slot])
                        .collect();
                    let read = read_slots(&todo);
                    if verify_lip_seat() {
                        // The shadow check: every served read, read the
                        // round's own way.
                        let taken: Vec<usize> = follower_slots
                            .iter()
                            .copied()
                            .filter(|&slot| served[slot])
                            .collect();
                        let fresh = read_slots(&taken);
                        for (&slot, fresh) in taken.iter().zip(fresh) {
                            assert!(
                                after[slot].map(f64::to_bits) == fresh.map(f64::to_bits),
                                "PLINTH_SWEEP_VERIFY_LIPSEAT: a follower read ahead differs from the round's own\n slot {slot}: ahead {:?}, fresh {fresh:?}",
                                after[slot]
                            );
                        }
                    }
                    for (&slot, value) in todo.iter().zip(read) {
                        after[slot] = value;
                    }
                }
                clears
            }
        };
        s0.lap(S0Part::Follow);
        // Every member of a proposal whose mover clears, as the lowering
        // measures its seat bite: one call for the round.
        let mut measured: Vec<Option<ShaftSeatBite>> = vec![None; rows];
        {
            let slots: Vec<usize> = proposals
                .iter()
                .zip(&first_of)
                .zip(&mover_clears)
                .filter(|(_, &clear)| clear)
                .flat_map(|((proposal, &first), _)| first..=first + proposal.children.len())
                .collect();
            if !slots.is_empty() {
                let arms_read: Vec<SweepArm> = slots.iter().map(|&slot| trial[slot].clone()).collect();
                // The bites the read-ahead measured for these very groups
                // stand where this batch gives each arm the lander height
                // they were read with; the rest are read here.
                let known: Vec<Option<plinth_support::SeatBiteRead>> = proposals
                    .iter()
                    .zip(&prefetched)
                    .zip(&mover_clears)
                    .filter(|(_, &clear)| clear)
                    .flat_map(|((proposal, pre), _)| {
                        let group = pre.as_ref().and_then(|pre| pre.bites.as_ref());
                        (0..=proposal.children.len())
                            .map(move |offset| group.and_then(|group| group.get(offset)).copied())
                    })
                    .collect();
                if s0.on {
                    s0.bite_arms += arms_read.len() as u64;
                    s0.bite_known += known.iter().filter(|read| read.is_some()).count() as u64;
                }
                let bites = plinth_support::tip_shaft_seat_bites_with(&arms_read, &known, layer_plan)
                    .unwrap_or_else(|_| vec![None; arms_read.len()]);
                if verify_lip_seat() && known.iter().any(Option::is_some) {
                    // The shadow check: the round's own call, compared.
                    let fresh = plinth_support::tip_shaft_seat_bites(&arms_read, layer_plan)
                        .unwrap_or_else(|_| vec![None; arms_read.len()]);
                    assert!(
                        bites == fresh,
                        "PLINTH_SWEEP_VERIFY_LIPSEAT: seat bites taken from the read-ahead differ from the round's own call\n ahead {bites:?}\n fresh {fresh:?}"
                    );
                }
                for (&slot, bite) in slots.iter().zip(bites) {
                    measured[slot] = bite;
                }
            }
        }
        s0.lap(S0Part::Bites);
        // **Whether the standing arm's tip and the lean's tip seat**, for
        // each proposal the slenderness law refuses (the test in the loop
        // below, evaluated here on the same inputs): one batched contact read
        // for the round, as the seat bites above are one call.
        let refused_lean: Vec<bool> = proposals
            .iter()
            .zip(&first_of)
            .zip(&hidden_of)
            .zip(&mover_clears)
            .map(|(((proposal, &first), hidden_slots), &clears)| {
                clears
                    && !trigger.is_clearance()
                    && !hidden_slots[0]
                    && after[first].is_some_and(|now| {
                        lean_too_slender(
                            context,
                            &struts[pending[proposal.slot].index],
                            &proposal.candidate,
                            &trial[first],
                            now,
                            tier,
                        )
                    })
            })
            .collect();
        let mut seating: Vec<Option<(TipContact, TipContact)>> = vec![None; proposals.len()];
        {
            let asked: Vec<usize> = (0..proposals.len()).filter(|&at| refused_lean[at]).collect();
            s0.seat_pairs += asked.len() as u64;
            if !asked.is_empty() {
                let pairs: Vec<(usize, &SweepArm, &SweepArm)> = asked
                    .iter()
                    .map(|&at| {
                        let index = pending[proposals[at].slot].index;
                        (index, &arms[index], &trial[first_of[at]])
                    })
                    .collect();
                for (&at, read) in asked
                    .iter()
                    .zip(tips_seat(joins, layer_plan, &mut standing_seats, &pairs))
                {
                    seating[at] = Some(read);
                }
            }
        }
        s0.lap(S0Part::Seats);
        for (at_proposal, ((proposal, &first), hidden_slots)) in
            proposals.into_iter().zip(&first_of).zip(hidden_of).enumerate()
        {
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
            // at (round 2). Read for every proposal above (`hidden_of`).
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
                s0.b2_lock += 1;
                continue;
            }
            // The index and the field know the moves already taken this
            // round: a parent re-landed from under the proposal, or a path
            // that now crosses a member moved this round, sends the arm back
            // for its next candidate.
            let job = &pending[proposal.slot];
            // With no move committed since the proposal was made (the field
            // generation is the one it was asked at), the answer is the
            // proposal itself.
            let unchanged = proposal.generation == index.generation();
            let again = if unchanged {
                if verify_lip_seat() {
                    let fresh = propose(
                        context,
                        &index,
                        struts,
                        index_of,
                        proposal.candidate,
                        &job.children,
                        trigger.is_clearance(),
                    );
                    verify_lip_seat_reuse(
                        "re-ask",
                        Some(&(proposal.path.clone(), proposal.children.clone())),
                        fresh.as_ref(),
                    );
                }
                Some((proposal.path.clone(), proposal.children.clone()))
            } else {
                s0.b2_reask += 1;
                propose(
                    context,
                    &index,
                    struts,
                    index_of,
                    proposal.candidate,
                    &job.children,
                    trigger.is_clearance(),
                )
            };
            let Some((path, children)) = again.filter(|(path, children)| {
                *path == proposal.path && *children == proposal.children
            }) else {
                outcome.candidates_refused_path += 1;
                s0.b2_reask_refused += 1;
                continue;
            };
            // **A LEAN THE SLENDERNESS LAW CANNOT HOLD IS NOT TAKEN** (Q1,
            // 2026-10-10, `DEINO-DIAGNOSIS.md` fix 1). The re-landing leans the
            // WHOLE arm along one straight axis. Where that axis runs within
            // the final gap of the model, the slenderness ladder cannot
            // thicken it (the thicker solid would enter the gap) and the arm
            // ends as an `unbraced_run_over_limit` warning on a long thin rod
            // (Deinonychus arms 215 and 220: runs 3.3x and 5.3x their limit).
            // The thickest radius that still keeps the gap follows from the
            // clearance just measured: the spare gap, `clearance - gap`, is
            // the radius it can grow by, up to the registered maximum. The
            // run is the chord the ladder measures (`slenderness::run_length`:
            // the drop and the lateral move between the seat and base cells)
            // and its limit is the existing `S * r` ([`lean_too_slender`]:
            // arithmetic on this pass's batched clearance read, no kernel
            // call of its own; why not the thicken rung's taper, there).
            //
            // **AND ONLY WHERE THE LEAN IS NO BETTER SEATED THAN THE ARM AS IT
            // STANDS** (q12 regressions, 2026-10-12; joint round 5,
            // 2026-10-14; [`keeps_standing_arm`]). The refusal trades the lean
            // for the arm already there, so that arm must be there to keep:
            // standing on its ground (`rooted`: the landing walk, so not
            // floating above the plate or the model, nor resting on a member
            // that is), whose solid does not enter the model
            // (`standing_drawn`: the widest-radius reading of `before`, and
            // where that reads inside the model, the solid the file draws,
            // since a tapered arm's thinner shaft can clear a wall its widest
            // radius would enter; unmeasured is not proof of entry). The lean
            // the law cannot hold is taken only where its tip really seats on
            // its face and the standing arm's tip does not (`seating`: the
            // real contact of both, one batched read for the round). A SHAPE
            // test of the standing arm (a one-layer stub on the model is "no
            // support") took Bat_Cat_Owl arm 20's 40 degree lean, a 0.10 mm
            // rod 3.4 times over its limit, in place of a stub whose tip
            // touched the claw's 0.018 mm edge: neither seated (0.0000 mm2),
            // and the lean added the rod. The Morel base's arm 44 stood as a
            // stub one layer long onto the model wall beside its seat, its
            // tip off its face (Q12 kept it in place of its 7.5 mm plate
            // lean); where the plate lean seats, it is taken as before Q1 and
            // left to the ladder.
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
            //
            // The test itself ran with the round's batched reads (`refused_lean`
            // above, on these same inputs); `seating` holds the contact reads
            // of the tips it asked for.
            if refused_lean[at_proposal] {
                let unread = TipContact {
                    seats: false,
                    real_mm2: None,
                    pixel_mm2: 0.0,
                };
                let (stands, lean) = seating[at_proposal].unwrap_or((unread, unread));
                let keeps_arm = keeps_standing_arm(
                    standing_reads
                        .drawn(index_of, struts)
                        .is_none_or(|was| was >= -CLEARANCE_SLACK_MM),
                    rooted[index_of],
                    stands.seats,
                    lean.seats,
                );
                if keeps_arm
                    && holds_nothing(
                        stands,
                        lean,
                        carries[index_of] || struts[index_of].twin_host,
                    )
                {
                    // A NUB: the arm holds nothing and neither does its lean
                    // (see [`holds_nothing`]). It is dropped once the pass has
                    // run (the caller owns the member list), with a WARNING
                    // naming the birth it was seated for.
                    let id = struts[index_of].id;
                    if !outcome.unheld_pending.iter().any(|unheld| unheld.arm == id) {
                        outcome.unheld_pending.push(UnheldArm {
                            arm: id,
                            load_mm2: struts[index_of].tip_load_mm2(),
                            seat_mm: arms[index_of].seat_mm,
                        });
                    }
                    let job = &mut pending[proposal.slot];
                    job.next = job.candidates.len();
                    s0.nubs += 1;
                    continue;
                }
                if keeps_arm {
                    outcome.candidates_refused_slender += 1;
                    // The cheapest admissible descent cannot be held: the arm
                    // keeps its seat and does not walk the rest of its
                    // candidates.
                    let job = &mut pending[proposal.slot];
                    job.refused_slender = true;
                    job.next = job.candidates.len();
                    // Once per arm: its job ends here (`next` is spent), and
                    // the list is cleared at the start of the pass.
                    outcome.kept_for_slenderness_ids.push(struts[index_of].id);
                    continue;
                }
                // The path hit of the kept-arm test: a lean the law cannot
                // hold, taken because the arm as it stands is not there to
                // keep, or because the lean seats where it does not.
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
                index.s0_touch(host as usize);
            }
            if proposal.slot == s0.longest_slot {
                s0.longest_accept_plus1 = proposal.at as u64 + 1;
            }
            outcome.relanded_ids.push(id);
            outcome.arms_relanded += 1;
            if matches!(proposal.candidate.landing, Landing::Model { .. }) {
                outcome.relanded_on_model += 1;
            }
            moved.push(record);
            done[proposal.slot] = true;
        }
        // An arm whose job has ended asks nothing more: what was read ahead
        // for it is released.
        for &slot in &open {
            if done[slot] || pending[slot].next >= pending[slot].candidates.len() {
                pending[slot].release(&mut ledger);
            }
        }
        open.retain(|slot| !done[*slot] && pending[*slot].next < pending[*slot].candidates.len());
        s0.lap(S0Part::B2);
        s0.round_end();
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
    s0.finish(trigger.is_clearance(), pending.len(), moved.len(), outcome);
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
    // S0: every path test is counted and timed on the thread that asks it.
    let started = crate::perf::enabled().then(std::time::Instant::now);
    let answer = propose_now(context, index, struts, mover, candidate, children, transitive);
    if let Some(started) = started {
        let ns = s0_ns(started);
        s0_tally(|tally| {
            tally.propose_calls += 1;
            tally.propose_ns += ns;
        });
    }
    answer
}

/// [`propose`], unmeasured.
fn propose_now(
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
    foot_sizing: Option<crate::down_sweep::SizingPolicy>,
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
    // The least contact the arm's foot must deliver: its narrowest ledger
    // section (the tip's, sized for the tip's own load; the foot passes more).
    let foot_radius_mm = strut
        .radius_mm
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    let foot_radius_mm = if foot_radius_mm.is_finite() {
        foot_radius_mm
    } else {
        0.0
    };
    // ON THE SIZED FIELD (the clearance re-route; fix lane A, 2026-10-12)
    // the foot is drawn for the load it passes, `size_arm(carried at the
    // landing)` ([`crate::model_feet`]), and that is the foot a model landing
    // must fit. Deinonychus arms 103, 154, 159, 184 and 200 were landed at
    // the tip's minimum and their feet grew past their faces afterwards.
    let foot_radius_mm = foot_sizing.map_or(foot_radius_mm, |sizing| {
        let carried = strut.carried_mm2.first().copied().unwrap_or(0.0).max(0.0);
        crate::down_sweep::size_arm(carried, sizing).max(foot_radius_mm)
    });
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
    // One row of the search box: its candidates, in column order, and the
    // counts the row adds (priced, refused for the foot). A pure function of
    // the row; the rows run on the pool where the box is large (lane S2,
    // 2026-10-15: the walker's box holds some 150,000 cells), and the list is
    // sorted below on a total key (cost, then cell, one candidate per cell),
    // so the order the rows come back in cannot reach it.
    let price_row = |row: usize| -> (Vec<Candidate>, usize, usize) {
        let mut out: Vec<Candidate> = Vec::new();
        let (mut priced, mut refused_foot) = (0_usize, 0_usize);
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
            // A MODEL LANDING TAKES ITS WHOLE FOOT (lane 2, 2026-10-12): the
            // same predicate the up-sweep's descent reads, at a lower bound on
            // the foot's contact (the arm's own tip section: the foot passes
            // at least its tip's load).
            if matches!(landing, Landing::Model { .. }) {
                let (on, total) = crate::up_sweep::foot_on_model(
                    grid,
                    context.history,
                    cell,
                    k_bottom,
                    foot_radius_mm,
                );
                if on < total {
                    refused_foot += 1;
                    continue;
                }
            }
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
            priced += 1;
            out.push(Candidate {
                cost_mm,
                cell,
                landing,
                k_bottom,
            });
        }
        (out, priced, refused_foot)
    };
    let box_cells = (row_hi + 1 - row_lo) * (col_hi + 1 - col_lo);
    let rows: Vec<(Vec<Candidate>, usize, usize)> = if box_cells >= PRICE_PARALLEL_MIN_CELLS {
        use rayon::prelude::*;
        (row_lo..=row_hi).into_par_iter().map(price_row).collect()
    } else {
        (row_lo..=row_hi).map(price_row).collect()
    };
    out.reserve(rows.iter().map(|(row, _, _)| row.len()).sum());
    for (row, priced, refused_foot) in rows {
        out.extend(row);
        outcome.candidates_priced += priced;
        outcome.candidates_refused_foot += refused_foot;
    }
    let order = |a: &Candidate, b: &Candidate| {
        a.cost_mm
            .total_cmp(&b.cost_mm)
            .then_with(|| a.cell.cmp(&b.cell))
    };
    if out.len() >= PRICE_PARALLEL_MIN_CELLS {
        use rayon::prelude::*;
        out.par_sort_unstable_by(order);
    } else {
        out.sort_unstable_by(order);
    }
    out
}

/// The search boxes (and candidate lists) [`price`] prices and sorts on the
/// pool: a schedule only (the list is sorted on a total key either way).
const PRICE_PARALLEL_MIN_CELLS: usize = 4096;

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
            thickened_mm: Vec::new(), foot_radius_mm: None,
            seat_on_strut,
            k_top: 100,
            k_bottom: 0,
            landing,
            interior: false,
            radius_mm: Vec::new(),
            carried_mm2: Vec::new(),
            covered_demand: Vec::new(), held_births: Vec::new(), twin_host: false,
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
            thickened_mm: Vec::new(), foot_radius_mm: None,
            seat_on_strut: None,
            k_top,
            k_bottom: 0,
            landing: Landing::Plate,
            interior: false,
            radius_mm: Vec::new(),
            carried_mm2: Vec::new(),
            covered_demand: Vec::new(), held_births: Vec::new(), twin_host: false,
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
            thickened_mm: Vec::new(), foot_radius_mm: None,
            seat_on_strut: None,
            k_top,
            k_bottom,
            landing,
            interior: false,
            radius_mm: Vec::new(),
            carried_mm2: Vec::new(),
            covered_demand: Vec::new(), held_births: Vec::new(), twin_host: false,
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
            thickened_mm: Vec::new(), foot_radius_mm: None,
            seat_on_strut: None,
            k_top: SEAT_LAYER,
            k_bottom,
            landing,
            interior: false,
            radius_mm: vec![0.1; SEAT_LAYER - k_bottom + 1],
            carried_mm2: Vec::new(),
            covered_demand: Vec::new(), held_births: Vec::new(), twin_host: false,
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
            None,
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

    /// **A stored path test is reused only at the generation it was asked
    /// at** (read-ahead reuse, 2026-10-15), and the shadow switch catches a
    /// mutator that forgets to bump it.
    ///
    /// The mover's lean to the plate at column 66 is open; the read-ahead
    /// stores it. A member then stands in column 63, across the lean: the
    /// commit stamps it ([`StampedIndex::stamp_strut`], which bumps the
    /// generation), and the round must ask afresh and find the lean blocked.
    /// SABOTAGE INSIDE: with the bump skipped the stale `Some` is served
    /// (visible with the shadow off) and the shadow panics with it on.
    #[test]
    fn a_stored_path_test_is_reused_only_at_its_generation_and_the_shadow_catches_a_missed_bump() {
        use super::{
            propose, take_path_test, Candidate, Ledger, Pending, StampedIndex, TEST_SKIP_BUMP,
            TEST_VERIFY,
        };
        let grid = grid();
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
        let law = crate::slenderness::SlendernessLaw::new(30.0, 0.05, 1.0, 32.5);
        let bare = history(|_, _| false);
        let context = crate::slenderness::SlendernessContext {
            grid: &grid,
            history: &bare,
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
        let candidate = Candidate {
            cost_mm: 1.0,
            cell: 60 * SIDE + 66,
            landing: Landing::Plate,
            k_bottom: 0,
        };
        // Runs the scene: returns (stored-and-served answer after the commit,
        // fresh answer after the commit), both `is_some`.
        let scene = |skip_bump: bool, verify: bool| {
            let mut struts = vec![
                arm(Landing::Plate, 0),
                member_in_column(1, StrutKind::Arm, 40, SEAT_LAYER),
            ];
            let mut raw = ColumnHistory::new(&grid);
            crate::congestion::stamp_all(&mut raw, &struts, H);
            let mut index = StampedIndex::new(raw);
            let mut job = Pending {
                index: 0,
                candidates: vec![candidate],
                next: 0,
                children: Vec::new(),
                refused_slender: false,
                ahead: std::collections::HashMap::new(),
                tested: std::collections::HashMap::new(),
                window: 1,
                retained: 0,
                scan: super::ScanStore::default(),
            };
            let mut ledger = Ledger::new();
            let ask = |index: &StampedIndex, struts: &[SweepStrut]| {
                propose(&context, index, struts, 0, candidate, &[], false)
            };
            let before = ask(&index, &struts);
            assert!(before.is_some(), "the lean is open before the commit");
            job.store_tested(
                &mut ledger,
                0,
                index.generation(),
                before.map(std::rc::Rc::new),
            );
            // The commit: member 1 comes to stand across the lean.
            TEST_SKIP_BUMP.with(|cell| cell.set(skip_bump));
            TEST_VERIFY.with(|cell| cell.set(verify));
            struts[1].cell = 60 * SIDE + 63;
            index.stamp_strut(&struts[1], H);
            let fresh = ask(&index, &struts);
            let served = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                take_path_test(&context, &index, &struts, &mut job, &mut ledger, 0, false)
            }));
            TEST_SKIP_BUMP.with(|cell| cell.set(false));
            TEST_VERIFY.with(|cell| cell.set(false));
            (served, fresh)
        };
        // With the bump: the stale answer is dropped and the lean reads blocked.
        let (served, fresh) = scene(false, true);
        assert!(fresh.is_none(), "the member across the lean blocks it");
        assert_eq!(served.expect("no panic with the bump"), fresh);
        // Sabotage, shadow off: the stale `Some` is served (the drill bites).
        let (served, fresh) = scene(true, false);
        assert!(fresh.is_none());
        assert!(served.expect("no shadow, no panic").is_some(), "the skipped bump serves the stale answer");
        // Sabotage, shadow on: the shadow panics.
        let (served, _) = scene(true, true);
        let payload = served.expect_err("the shadow check must catch the missed bump");
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|text| (*text).to_owned()))
            .unwrap_or_default();
        assert!(message.contains("PLINTH_SWEEP_VERIFY_LIPSEAT"), "{message}");
    }

    /// **The read-ahead's byte limit changes no answer** (2026-10-15). One
    /// mover with forty plate descents to the right, ten open and thirty
    /// across a member's column: the window is filled, then the round loop
    /// takes every candidate in order. With the limit at its default every
    /// candidate is read ahead; with a limit of two kilobytes the ledger
    /// refuses most of the window, never holds more than the limit, and the
    /// round loop asks the rest on demand. Both give, candidate by candidate,
    /// the answer of the bare [`propose`]; the ledger reads zero once every
    /// entry is spent, and a released job holds nothing.
    #[test]
    fn a_tiny_read_ahead_limit_gives_the_answers_of_no_limit() {
        use super::{
            propose, take_path_test, window_path_test, Candidate, Ledger, Pending, StampedIndex,
            Window, TEST_BUDGET, TEST_REFUSED_FOR_ROOM,
        };
        let grid = grid();
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
        let law = crate::slenderness::SlendernessLaw::new(30.0, 0.05, 1.0, 32.5);
        let bare = history(|_, _| false);
        let context = crate::slenderness::SlendernessContext {
            grid: &grid,
            history: &bare,
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
        let candidates: Vec<Candidate> = (61..101)
            .map(|col| Candidate {
                cost_mm: f64::from(u32::try_from(col).expect("small")),
                cell: 60 * SIDE + col,
                landing: Landing::Plate,
                k_bottom: 0,
            })
            .collect();
        let struts = vec![
            arm(Landing::Plate, 0),
            member_in_column(1, StrutKind::Arm, 71, SEAT_LAYER),
        ];
        let mut raw = ColumnHistory::new(&grid);
        crate::congestion::stamp_all(&mut raw, &struts, H);
        let index = StampedIndex::new(raw);
        let run = |limit: Option<usize>| {
            TEST_BUDGET.with(|cell| cell.set(limit));
            TEST_REFUSED_FOR_ROOM.with(|cell| cell.set(0));
            let mut ledger = Ledger::new();
            let mut job = Pending {
                index: 0,
                candidates: candidates.clone(),
                next: 0,
                children: Vec::new(),
                refused_slender: false,
                ahead: std::collections::HashMap::new(),
                tested: std::collections::HashMap::new(),
                window: 1,
                retained: 0,
                scan: super::ScanStore::default(),
            };
            let mut read_ahead = 0_usize;
            for at in 0..candidates.len() {
                match window_path_test(&context, &index, &struts, &mut job, &mut ledger, at, false) {
                    Window::Full => break,
                    Window::Refused | Window::Passed(_) => read_ahead += 1,
                }
            }
            let held = ledger.retained;
            let limit_held = ledger.limit;
            let answers: Vec<_> = (0..candidates.len())
                .map(|at| take_path_test(&context, &index, &struts, &mut job, &mut ledger, at, false))
                .collect();
            let spent = ledger.retained;
            job.release(&mut ledger);
            let refused = TEST_REFUSED_FOR_ROOM.with(std::cell::Cell::get);
            TEST_BUDGET.with(|cell| cell.set(None));
            (answers, read_ahead, held, limit_held, spent, refused, ledger.retained)
        };
        let bare_answers: Vec<_> = candidates
            .iter()
            .map(|&candidate| propose(&context, &index, &struts, 0, candidate, &[], false))
            .collect();
        let passes = bare_answers.iter().filter(|answer| answer.is_some()).count();
        assert!(
            passes > 0 && passes < candidates.len(),
            "anti-vacuity: some descents are open and some blocked ({passes} of {})",
            candidates.len()
        );
        let (free, free_ahead, _, _, free_spent, free_refused, free_end) = run(None);
        assert_eq!(free, bare_answers, "no limit: the on-demand answers");
        assert_eq!(free_ahead, candidates.len(), "no limit: the whole window is read ahead");
        assert_eq!(free_refused, 0);
        assert_eq!((free_spent, free_end), (0, 0), "every entry is spent and credited");
        let (tiny, tiny_ahead, tiny_held, tiny_limit, tiny_spent, tiny_refused, tiny_end) = run(Some(2000));
        assert_eq!(tiny, bare_answers, "a tiny limit: the same answers");
        assert!(tiny_ahead < candidates.len() && tiny_refused > 0, "the limit tripped: {tiny_ahead} read ahead");
        assert!(tiny_held <= tiny_limit, "the ledger never passes its limit: {tiny_held} > {tiny_limit}");
        assert_eq!((tiny_spent, tiny_end), (0, 0));
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

    /// **THE STANDING ARM IS KEPT UNLESS THE LEAN SEATS WHERE IT DOES NOT**
    /// (joint round 5, 2026-10-14; Bat_Cat_Owl arm 20). The arm must be there
    /// to keep (clear of the model, rooted), and a lean the law cannot hold
    /// replaces it only when the lean's tip seats and the standing arm's does
    /// not. No SHAPE of the standing arm enters: a one-layer stub on the model
    /// whose lean seats no better is kept. SABOTAGE (by hand, the shape test
    /// back: a one-layer stub on the model is "no support"): the first row
    /// fails, as it did on arm 20.
    #[test]
    fn the_standing_arm_is_kept_unless_the_lean_seats_where_it_does_not() {
        use super::keeps_standing_arm;
        // (clear, rooted, stands seats, lean seats) -> kept
        assert!(keeps_standing_arm(true, true, false, false), "neither seats: the stub is kept");
        assert!(keeps_standing_arm(true, true, true, true), "both seat: the arm is kept");
        assert!(keeps_standing_arm(true, true, true, false), "only the arm seats: kept");
        assert!(!keeps_standing_arm(true, true, false, true), "only the lean seats: the lean is taken");
        assert!(!keeps_standing_arm(false, true, true, false), "inside the model: nothing to keep");
        assert!(!keeps_standing_arm(true, false, true, false), "off its ground: nothing to keep");
    }

    /// **An arm that holds nothing is dropped** (round 6, `NUB-CLAW.md` part
    /// 1; Bat_Cat_Owl arm 20, a 0.156 mm stub with 0.0000 mm2 of contact and a
    /// lean with 0.00003 mm2, against a pixel of 0.0025). Only a tip MEASURED
    /// to make under one pixel of contact, whose lean makes under one pixel,
    /// with nothing resting on it. A tip with a pixel or more of contact,
    /// however short of its plan, is a support (arm 12 of the same file: 0.0061
    /// and 0.0094 mm2, kept); an unmeasured tip is not proof of nothing.
    /// SABOTAGE (by hand: the nub rule off, the stub kept as in round 5): the
    /// first row fails.
    #[test]
    fn an_arm_is_dropped_only_where_it_and_its_lean_touch_nothing() {
        use super::{holds_nothing, TipContact};
        let touches = |real: Option<f64>| TipContact {
            seats: false,
            real_mm2: real,
            pixel_mm2: 0.0025,
        };
        let nothing = touches(Some(0.0));
        assert!(holds_nothing(nothing, nothing, false), "neither touches: a nub");
        assert!(
            holds_nothing(nothing, touches(Some(0.000_031)), false),
            "a lean under one pixel of contact is none (arm 20)"
        );
        assert!(!holds_nothing(touches(Some(0.0061)), touches(Some(0.0094)), false), "arm 12");
        assert!(!holds_nothing(touches(Some(0.01)), nothing, false), "the arm has some contact");
        assert!(!holds_nothing(nothing, touches(Some(0.01)), false), "the lean has some contact");
        assert!(!holds_nothing(touches(None), nothing, false), "an unmeasured tip is not proof");
        assert!(!holds_nothing(nothing, touches(None), false), "an unmeasured lean is not proof");
        assert!(!holds_nothing(nothing, nothing, true), "something rests on it");
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
