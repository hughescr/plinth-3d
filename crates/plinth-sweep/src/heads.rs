//! ARTICULATED SUPPORT HEADS: the data a head is, the law that sizes it, and
//! the report a run keeps about them (Craig, 2026-10-06; plan
//! `field-2026-09-28/articulation-build-plan.md` section 1).
//!
//! # The shape
//!
//! ```text
//!   seat cell (k_top)
//!        ● D        D  the contact centre, on the seat plane
//!         \         u  the footprint normal at placement (outward, z < 0)
//!          \  head  K  the joint sphere's centre, K = D + (g + r_K)·u
//!           ● K     g  the registered final clearance gap
//!           |
//!           |  post  K -> base, the rest of the arm
//! ```
//!
//! On a steep face the arm's top is a short straight HEAD along `u`, so the
//! contact arrives square. The joint sphere stands exactly `g` off the seat
//! plane, so the post is never inside the gap. The up-sweep builds a head once,
//! at placement, and nothing moves it afterwards; where a head cannot be built
//! the arm keeps today's straight tip (emit and warn, never refuse).
//!
//! # The law (no constant)
//!
//! The tip load `F = p·A_tip` acts through the contact centre. A section of
//! radius `r` that carries the axial load area `π·r_led²` and a moment `F·e`
//! has the combined stress `σ = p·[A/(π r²) + 4·A_tip·e/(π r³)]`, and `X_ALLOWED`
//! cancels, leaving `r³ ≥ r_led²·r + 4·r_tip²·e` ([`section_radius_mm`]).
//!
//! **The envelope.** The lever `e` is at most `|DK|` (`sin φ′ ≤ 1`, where `φ′`
//! is the angle between the post's chord and the head), and the axial force is
//! at most `F / cos λmax` for any post the lean law admits (`c = cos λmax`,
//! [`crate::lean::LeanLaw::cos_max_lean`]). So the joint sized at `c` and the
//! full lever carries every post the lean law can route, and a re-route, a
//! merge or a late host slide can never overload it. With `c` applied to both
//! terms the three sections are:
//!
//! * head, at axial distance `s` from `D`: `r(s)³ = (r_t²/c)(r + 4s)`
//!   ([`head_section_mm`]);
//! * joint: `r_K³ = (r_t²/c)(r_K + 4(g + r_K))` ([`solve_joint`]);
//! * post below `K`, at distance `σ` down a post of length `ℓ`:
//!   `r³ ≥ r_led²·r + (4r_t²/c)(g + r_K)(1 − σ/ℓ)` ([`post_section_mm`]).

use std::collections::BTreeMap;

use geom_kernel::{closest_on_tri, vec3, NativeSurface, NearFaceBatchLimits};
use plinth_core::TipContactProfiles;
use plinth_mesh::Mesh;

use crate::drain::DrainKeepOut;
use crate::footprint_normal::{footprint_normals, footprint_pairs, FootprintBall};
use crate::grid::Grid;
use crate::up_sweep::ColumnHistory;

/// Halvings of a bracket: far finer than any radius this engine draws, a
/// numerical resolution and not a physical number.
const BISECTIONS: usize = 100;

/// One articulated head, **frozen at placement**: the up-sweep builds it once
/// and no later pass moves it (a later pass may only grow a radius, and the
/// radii are re-derived from one law). All points are absolute mm, in the same
/// frame as [`crate::SweepStrut::seat_face`]'s `point_mm` (the plate stands at
/// `plate_z_mm`, the lowering's own frame).
#[derive(Clone, Debug, PartialEq)]
pub struct Head {
    /// `D`: the contact centre, on the seat plane.
    pub contact_mm: [f64; 3],
    /// `u`: the footprint normal at placement, unit, outward (`z < 0`).
    pub unit: [f64; 3],
    /// `K`: the joint sphere's centre, exact mm, `D + (g + r_K)·u`.
    pub joint_mm: [f64; 3],
    /// `floor(K_z / layer height)` over the plate, rounded toward the plate:
    /// the layer the post's grid route starts at.
    pub k_joint: usize,
    /// The cell of `K` at `k_joint`.
    pub joint_cell: u32,
    /// The planned `r_K` ([`solve_joint`]) at the tip radius the head was
    /// planned with. The drawn joint is never smaller.
    pub joint_radius_mm: f64,
    /// The tip radius `r_t` the head and joint were planned at.
    pub planned_tip_radius_mm: f64,
    /// `g`: the registered final clearance gap.
    pub gap_mm: f64,
    /// `c = cos λmax` the envelope was planned with
    /// ([`crate::lean::LeanLaw::cos_max_lean`]).
    pub cos_lean_max: f64,
    /// The head's cells for layers `k_joint + 1 ..= k_top`, bottom-up. The
    /// last entry is the seat cell.
    pub cells: Vec<u32>,
}

/// The smallest section radius that carries the tip's bending moment at a
/// lever `lever_mm`: the root `r ≥ ledger_radius_mm` of
/// `r³ = ledger² · r + 4 · tip² · lever`.
///
/// `g(r) = r³ − ledger²·r − 4·tip²·e` is zero at no point below the ledger
/// radius that matters: `g(ledger) = −4·tip²·e ≤ 0` and `g` rises for every
/// `r ≥ ledger`, so the root is unique. A zero lever gives the ledger itself;
/// a non-finite or negative input gives the ledger (nothing is invented).
#[must_use]
pub fn section_radius_mm(ledger_radius_mm: f64, tip_radius_mm: f64, lever_mm: f64) -> f64 {
    let moment_term = 4.0 * tip_radius_mm * tip_radius_mm * lever_mm;
    if !(ledger_radius_mm.is_finite() && moment_term.is_finite() && moment_term > 0.0) {
        return ledger_radius_mm;
    }
    let excess = |r: f64| r * (r * r - ledger_radius_mm * ledger_radius_mm) - moment_term;
    // `(a + b)^3 - a^2 (a + b) >= b^3` with `b^3 = moment_term`, so this
    // bracket's upper end always satisfies the law.
    let mut low = ledger_radius_mm;
    let mut high = ledger_radius_mm + moment_term.cbrt();
    for _ in 0..BISECTIONS {
        let mid = 0.5 * (low + high);
        if excess(mid) >= 0.0 {
            high = mid;
        } else {
            low = mid;
        }
    }
    high
}

/// A solved bend: the joint sphere's radius, the segment's length from the
/// seat plane and the lever of the tip's load at the bend.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BendSolution {
    /// `r_K`, the joint sphere's radius and the post's top section.
    pub joint_radius_mm: f64,
    /// `L = g + r_K`: the bend's distance over the seat plane, along the normal.
    pub length_mm: f64,
    /// `e_K = L · lever_factor`.
    pub lever_mm: f64,
}

/// Solve the bend of a tip of radius `tip_radius_mm` for a `lever_factor`: the
/// root of
///
/// ```text
/// r_K³ = r_tip² · ( r_K + 4 · (g + r_K) · lever_factor )
/// ```
///
/// over `[r_tip, max_radius_mm]`. `None` where even `max_radius_mm` cannot
/// carry the moment (the registered maximum radius refuses that bend, and the
/// arm keeps its straight seat), or where an input is not finite and positive.
#[must_use]
pub fn solve_bend(
    tip_radius_mm: f64,
    lever_factor: f64,
    gap_mm: f64,
    max_radius_mm: f64,
) -> Option<BendSolution> {
    let usable = tip_radius_mm.is_finite()
        && tip_radius_mm > 0.0
        && lever_factor.is_finite()
        && lever_factor >= 0.0
        && gap_mm.is_finite()
        && gap_mm >= 0.0
        && max_radius_mm.is_finite();
    if !usable {
        return None;
    }
    let tip_squared = tip_radius_mm * tip_radius_mm;
    let excess = |r: f64| r * r * r - tip_squared * (r + 4.0 * (gap_mm + r) * lever_factor);
    if excess(max_radius_mm) < 0.0 {
        return None;
    }
    let (mut low, mut high) = (tip_radius_mm, max_radius_mm.max(tip_radius_mm));
    for _ in 0..BISECTIONS {
        let mid = 0.5 * (low + high);
        if excess(mid) >= 0.0 {
            high = mid;
        } else {
            low = mid;
        }
    }
    let joint_radius_mm = high;
    let length_mm = gap_mm + joint_radius_mm;
    Some(BendSolution {
        joint_radius_mm,
        length_mm,
        lever_mm: length_mm * lever_factor,
    })
}

/// The head's section at axial distance `s_mm` from the contact centre:
/// `r³ = (r_t²/c)(r + 4s)`. At `s = 0` this is `r_t/√c`, the neck's start.
#[must_use]
pub fn head_section_mm(tip_radius_mm: f64, s_mm: f64, cos_lean_max: f64) -> f64 {
    let ledger = tip_radius_mm / cos_lean_max.sqrt();
    section_radius_mm(ledger, ledger, s_mm)
}

/// The joint: `r_K³ = (r_t²/c)(r_K + 4(g + r_K))`, solved over
/// `[r_t/√c, max_radius_mm]`. Its `length_mm` is `|DK| = g + r_K`. The lever
/// is at most `|DK|` and the axial force at most `F / cos λmax` for any post
/// the lean law admits, so a re-route can never overload this joint. `None`
/// above the tip radius whose joint would exceed `max_radius_mm`.
#[must_use]
pub fn solve_joint(
    tip_radius_mm: f64,
    gap_mm: f64,
    cos_lean_max: f64,
    max_radius_mm: f64,
) -> Option<BendSolution> {
    solve_bend(tip_radius_mm / cos_lean_max.sqrt(), 1.0, gap_mm, max_radius_mm)
}

/// **The joint a FROZEN head must carry at its final tip radius**, with no
/// placement maximum: the root of the same cubic as [`solve_joint`], over
/// `[r_t/√c, bound]` where `bound` is a radius that always satisfies it.
///
/// Placement may refuse a joint above the registered maximum radius, because
/// no head exists yet and the arm keeps its straight tip. After placement the
/// head is built and frozen; the final tip radius can only grow, and the
/// joint grows with it (never capped, per the grow-never-cap rule). With
/// `a = r_t²/c` the cubic is `r³ = a(5r + 4g)`, and `r = max(√(10a),
/// ∛(8ga))` satisfies it (`r³/2 ≥ 5ar` and `r³/2 ≥ 4ga`). `None` only for a
/// tip radius or a gap that is not finite and positive.
#[must_use]
pub fn grown_joint_radius_mm(tip_radius_mm: f64, gap_mm: f64, cos_lean_max: f64) -> Option<f64> {
    let a = tip_radius_mm * tip_radius_mm / cos_lean_max;
    let bound = (10.0 * a).sqrt().max((8.0 * gap_mm * a).cbrt());
    solve_joint(tip_radius_mm, gap_mm, cos_lean_max, bound).map(|joint| joint.joint_radius_mm)
}

/// The post's section at a `lever_mm` below the joint, over a ledger section
/// `ledger_radius_mm`: `r³ = r_led²·r + (4r_t²/c)·lever`. The lever falls
/// linearly from `g + r_K` at `K` to zero at the post's next hold.
#[must_use]
pub fn post_section_mm(
    ledger_radius_mm: f64,
    tip_radius_mm: f64,
    lever_mm: f64,
    cos_lean_max: f64,
) -> f64 {
    section_radius_mm(
        ledger_radius_mm,
        tip_radius_mm / cos_lean_max.sqrt(),
        lever_mm,
    )
}

/// Why a seat kept today's straight tip instead of a head.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HeadRefusal {
    /// The face is not steep: flat faces and Round discs keep the straight tip.
    NotSteep,
    /// No down-facing face was found for the seat.
    NoSurface,
    /// The footprint normal could not be measured.
    NoFootprint,
    /// The seat is inside a hidden cavity.
    HiddenCavity,
    /// The joint radius would exceed the registered maximum radius.
    JointOverMax,
    /// The head or joint stands inside a drain keep-out.
    InDrain,
    /// The head or joint would stand inside the model.
    HitsModel,
    /// The head or joint would stand inside an accepted member's shaft.
    HitsMember,
    /// The head or joint would stand inside an earlier head.
    HitsHead,
    /// No post route exists from the joint.
    NoPost,
    /// The post from the joint cannot be cleared.
    PostUncleared,
}

/// What a run did about heads: the path-hit witness for every one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeadsOutcome {
    /// Heads built.
    pub built: usize,
    /// Seats that kept the straight tip, per reason, with the member ids.
    pub refusals: BTreeMap<HeadRefusal, Vec<u32>>,
    /// Placement residual, in degrees: the angle between the normal the seat
    /// was placed with and the refined one (max, median).
    pub placement_residual_deg_max: f64,
    pub placement_residual_deg_median: f64,
    /// Final-normal residual, in degrees: the angle between the head's `u`
    /// and the footprint normal measured on the final field (max, median).
    pub final_normal_residual_deg_max: f64,
    pub final_normal_residual_deg_median: f64,
    /// Joints grown past their frozen radius because the final tip radius
    /// asked for more.
    pub joints_grown: usize,
    /// The largest stand-off deficit below `g`, in mm, left after a joint
    /// grew: a WARNING quantity, zero when every joint stands `g` off.
    pub max_standoff_deficit_mm: f64,
    /// Heads built at placement and DOWNGRADED to today's straight tip after
    /// the final sizing, because the final tip radius grew the joint across
    /// the clearance (stand-off below `g`): the member ids. See
    /// [`downgrade_grown_heads`].
    pub downgraded_after_sizing: Vec<u32>,
}

// ---------------------------------------------------------------------------
// The constructor: trigger, placement normal, plan, placement tests.
//
// Pure functions. Nothing calls them yet (a later lane wires them into
// `commit_seat`). Every point is absolute mm in the lowering's frame (the
// plate stands at `plate_z_mm`); the raster frame (plate at z = 0) is derived
// from it where a layer index or the hidden-cavity map is asked.
// ---------------------------------------------------------------------------

/// `Skim(x)`: a shaft at angle `x` (cosine `cos_x`) to the normal stays inside
/// the final gap for more than the rasterisation allowance longer than a normal
/// approach does: `(g + r)(sec x − 1) > allowance`. A non-positive cosine is a
/// grazing approach: it skims. `allowance_mm` is the seat law's layer + pixel
/// (`plinth_support`'s `rasterisation_allowance_mm`).
#[must_use]
pub fn skims(tip_radius_mm: f64, cos_x: f64, gap_mm: f64, allowance_mm: f64) -> bool {
    if cos_x.is_nan() || cos_x <= 0.0 {
        return true;
    }
    (gap_mm + tip_radius_mm) * (1.0 / cos_x - 1.0) > allowance_mm
}

/// The corpus p90 of a steep tip's final radius, mm: the planning prior behind
/// [`HeadRule::typical_tip_radius_mm`] (measured on the v17 corpus,
/// `articulation-build-plan.md`). DEBT: a measured prior, not a derived limit;
/// it prunes the plan and decides no outcome, because the check after the final
/// sizing ([`downgrade_grown_heads`]) does.
pub const TYPICAL_TIP_RADIUS_MM: f64 = 0.15;

/// The registered numbers the constructor reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadRule {
    /// `g`: the registered final clearance gap.
    pub gap_mm: f64,
    /// `layer + pixel`: the seat law's rasterisation allowance.
    pub allowance_mm: f64,
    /// `c = cos λmax` ([`crate::lean::LeanLaw::cos_max_lean`]).
    pub cos_lean_max: f64,
    /// The registered maximum tip radius: the joint may not exceed it.
    pub max_radius_mm: f64,
    /// The tip radius a seat is planned at when its own demand asks for less:
    /// the corpus p90 of a steep tip's final radius (about 0.15 mm,
    /// `articulation-build-plan.md`). A PLANNING ESTIMATE that prunes and
    /// never decides an outcome: the check after the final sizing
    /// ([`downgrade_grown_heads`]) decides whether a head is kept.
    pub typical_tip_radius_mm: f64,
    /// A face is admitted when its outward normal's `z` is below this
    /// ([`crate::tip_landing::SeatFaceRule::max_normal_z`]).
    pub max_normal_z: f64,
    /// The layer height `h`.
    pub layer_height_mm: f64,
    /// Where the plate stands, in the frame of every point below.
    pub plate_z_mm: f64,
}

/// Whether a face with outward unit normal `z` component `unit_z` is steep
/// enough for a head at tip radius `tip_radius_lb_mm`: the face is admitted
/// (faces down, below the rule's `max_normal_z`) and a vertical post would
/// skim it ([`skims`] at `cos x = −unit_z`).
#[must_use]
pub fn head_triggers(unit_z: f64, tip_radius_lb_mm: f64, rule: &HeadRule) -> bool {
    unit_z < 0.0
        && unit_z < rule.max_normal_z
        && skims(tip_radius_lb_mm, -unit_z, rule.gap_mm, rule.allowance_mm)
}

/// One seat's question to [`plan_head`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadRequest {
    /// `A`: the seat's axis point.
    pub axis_point_mm: [f64; 3],
    /// `P`: the seat face's point (the seat plane passes through it).
    pub face_point_mm: [f64; 3],
    /// `u`: the footprint normal at placement, outward.
    pub unit: [f64; 3],
    /// The tip radius lower bound the trigger and the joint are planned at.
    pub tip_radius_lb_mm: f64,
    /// The seat's cell at `k_top`.
    pub seat_cell: u32,
    /// The seat's layer.
    pub k_top: usize,
}

/// The cell of the grid holding `(x, y)`, or `None` outside it.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a floored coordinate tested non-negative and below the grid's own dimensions first"
)]
fn cell_holding(grid: &Grid, x_mm: f64, y_mm: f64) -> Option<usize> {
    let col = grid.col_coord(x_mm).floor();
    let row = grid.row_coord(y_mm).floor();
    if !(col >= 0.0 && row >= 0.0 && col < grid.cols() as f64 && row < grid.rows() as f64) {
        return None;
    }
    Some(grid.index(row as usize, col as usize))
}

/// **Plan one head**, once, at placement. Steps, in order, each a refusal:
///
/// 1. [`HeadRefusal::NotSteep`] unless the trigger holds ([`head_triggers`]);
/// 2. [`HeadRefusal::JointOverMax`] if [`solve_joint`] has no root;
/// 3. `D = A − ((A − P)·u)·u`, `K = D + (g + r_K)·u` (exact: never snapped to
///    a cell centre);
/// 4. `k_joint = floor(K_z / h)` over the plate, toward the plate;
/// 5. the joint's cell, and for `k_joint < k < k_top` the cell of the `D→K`
///    axis at `z = k·h` (clamped to the segment), then the seat cell at
///    `k_top`;
/// 6. [`HeadRefusal::NoPost`] if `k_joint` is 0 (the joint is below layer 1)
///    or not below `k_top`, or if the axis leaves the grid.
///
/// # Errors
///
/// The [`HeadRefusal`] above; the arm keeps today's straight tip.
pub fn plan_head(req: &HeadRequest, rule: &HeadRule, grid: &Grid) -> Result<Head, HeadRefusal> {
    let Some(unit) = vec3::normalize(req.unit) else {
        return Err(HeadRefusal::NoFootprint);
    };
    if !head_triggers(unit[2], req.tip_radius_lb_mm, rule) {
        return Err(HeadRefusal::NotSteep);
    }
    let Some(joint) = solve_joint(
        req.tip_radius_lb_mm,
        rule.gap_mm,
        rule.cos_lean_max,
        rule.max_radius_mm,
    ) else {
        return Err(HeadRefusal::JointOverMax);
    };
    let (a, p) = (req.axis_point_mm, req.face_point_mm);
    let contact = vec3::sub(a, vec3::scale(unit, vec3::dot(vec3::sub(a, p), unit)));
    let k = vec3::add(contact, vec3::scale(unit, joint.length_mm));
    let layers_over_plate = ((k[2] - rule.plate_z_mm) / rule.layer_height_mm).floor();
    if !(layers_over_plate >= 1.0) {
        return Err(HeadRefusal::NoPost);
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a floored layer count tested at least one above"
    )]
    let k_joint = layers_over_plate as usize;
    if k_joint >= req.k_top {
        return Err(HeadRefusal::NoPost);
    }
    let cell_of = |at: [f64; 3]| {
        cell_holding(grid, at[0], at[1])
            .and_then(|cell| u32::try_from(cell).ok())
            .ok_or(HeadRefusal::NoPost)
    };
    let joint_cell = cell_of(k)?;
    let drop_mm = (contact[2] - k[2]).max(f64::MIN_POSITIVE);
    let mut cells = Vec::with_capacity(req.k_top - k_joint);
    for layer in k_joint + 1..req.k_top {
        #[expect(clippy::cast_precision_loss, reason = "a layer index is exact in f64")]
        let z = rule.plate_z_mm + layer as f64 * rule.layer_height_mm;
        let t = ((contact[2] - z) / drop_mm).clamp(0.0, 1.0);
        cells.push(cell_of(vec3::add(contact, vec3::scale(vec3::sub(k, contact), t)))?);
    }
    cells.push(req.seat_cell);
    Ok(Head {
        contact_mm: contact,
        unit,
        joint_mm: k,
        k_joint,
        joint_cell,
        joint_radius_mm: joint.joint_radius_mm,
        planned_tip_radius_mm: req.tip_radius_lb_mm,
        gap_mm: rule.gap_mm,
        cos_lean_max: rule.cos_lean_max,
        cells,
    })
}

/// **The tip radius a head is planned at** (orchestrator ruling 2026-10-06,
/// after the Cylinder watch: 9 of 28 joints planned at the 0.05 mm minimum grew
/// across the seat plane when the final sizing gave the tip up to 0.28 mm).
///
/// `demand_lb_mm` is the lower bound the seat's own demand gives through the
/// area law (the only per-seat figure placement has: later landers and peel
/// events add load afterwards, so the final radius is never below it). The
/// estimate is the larger of that and the corpus p90 of a steep tip's final
/// radius ([`HeadRule::typical_tip_radius_mm`]), so a seat that covers little is
/// still planned for the tip it usually ends with. Where the joint for the
/// estimate would exceed the registered maximum radius, the lower bound is
/// used (the head is still planned, and the check after the final sizing
/// decides). A planning figure only: it never decides an outcome.
#[must_use]
pub fn planned_tip_radius_mm(demand_lb_mm: f64, rule: &HeadRule) -> f64 {
    let estimate = demand_lb_mm.max(rule.typical_tip_radius_mm);
    if estimate > demand_lb_mm
        && solve_joint(estimate, rule.gap_mm, rule.cos_lean_max, rule.max_radius_mm).is_none()
    {
        return demand_lb_mm;
    }
    estimate
}

/// **Downgrade every head the final sizing grew across the clearance**
/// (orchestrator ruling 2026-10-06). A head is frozen at placement and only
/// its radii grow; when the final tip radius gives a joint whose stand-off
/// from the seat plane, `(K - D)·u - r_K`, is below the gap `g`, the sphere
/// stands inside the clearance and the file would draw a head that cuts the
/// model. Such a member goes back to today's straight tip: the head is dropped,
/// the post keeps its landing, its base AND its path (it is NOT re-routed: the
/// members that landed on it, and the landing ledger, still read the cells it
/// stood in), and the arm is drawn straight from its seat to that base. Its
/// section from the joint up falls back to the area law at the tip (the head
/// minimum is gone with the head). A head the ledger cannot draw (no joint
/// slot) is dropped the same way.
///
/// One pass over `struts`, after the last sizing and before anything reads the
/// finished field. Returns the downgraded members' ids, in id order.
pub fn downgrade_grown_heads(struts: &mut [crate::SweepStrut]) -> Vec<u32> {
    let mut downgraded = Vec::new();
    for strut in struts.iter_mut() {
        let Some(head) = strut.head.as_ref() else {
            continue;
        };
        if !crate::slenderness::live(strut) {
            continue;
        }
        let along = vec3::dot(vec3::sub(head.joint_mm, head.contact_mm), head.unit);
        let joint_slot = head.k_joint.checked_sub(strut.k_bottom);
        let joint_radius = joint_slot.and_then(|slot| strut.radius_mm.get(slot).copied());
        let grown = joint_radius.is_none_or(|radius| along - radius < head.gap_mm - 1e-9);
        if !grown {
            continue;
        }
        downgraded.push(strut.id);
        strut.head = None;
        let slots = strut.radius_mm.len();
        let (Some(joint_slot), Some(&tip)) = (joint_slot, strut.radius_mm.last()) else {
            continue;
        };
        for radius in strut.radius_mm.iter_mut().take(slots).skip(joint_slot) {
            *radius = tip;
        }
    }
    downgraded
}

/// One tip's question to [`placement_normals`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacementBall {
    /// `A`: the seat's axis point, the first pass's ball centre.
    pub axis_point_mm: [f64; 3],
    /// The seat triangle's mesh face id (the same nearest-face primitive
    /// [`crate::tip_landing::tip_seat_faces_counted_with_ids`] uses).
    pub seat_face: u32,
    /// The seat triangle's own outward unit normal.
    pub facet_normal: [f64; 3],
    /// The tip radius lower bound.
    pub tip_radius_lb_mm: f64,
    /// The contact-sphere factor at that radius, read off the registry's
    /// [`TipContactProfiles::ball_factor`]; never a literal.
    pub ball_factor: f64,
}

impl PlacementBall {
    /// A ball whose factor is read off the registered profile table.
    #[must_use]
    pub fn from_profiles(
        axis_point_mm: [f64; 3],
        seat_face: u32,
        facet_normal: [f64; 3],
        tip_radius_lb_mm: f64,
        profiles: &TipContactProfiles,
    ) -> Self {
        Self {
            axis_point_mm,
            seat_face,
            facet_normal,
            tip_radius_lb_mm,
            ball_factor: profiles.ball_factor(tip_radius_lb_mm),
        }
    }

    fn radius_mm(&self) -> f64 {
        self.ball_factor * self.tip_radius_lb_mm
    }
}

/// What [`placement_normals`] reads.
pub struct PlacementEnv<'a> {
    pub mesh: &'a Mesh,
    /// The model's surface index, where one was built.
    pub surface: Option<&'a NativeSurface>,
    /// [`crate::tip_landing::SeatFaceRule::max_normal_z`].
    pub max_normal_z: f64,
}

/// One batched pass of the footprint kernel: the normal under each ball.
fn footprint_pass(
    env: &PlacementEnv<'_>,
    balls: &[PlacementBall],
    centres: &[[f64; 3]],
) -> Vec<Option<[f64; 3]>> {
    let radii: Vec<f64> = balls.iter().map(PlacementBall::radius_mm).collect();
    let Some(pairs) = footprint_pairs(env.mesh, env.surface, centres, &radii) else {
        return vec![None; balls.len()];
    };
    let questions: Vec<FootprintBall> = balls
        .iter()
        .zip(centres.iter().zip(&radii))
        .map(|(ball, (&centre_mm, &radius_mm))| FootprintBall {
            centre_mm,
            radius_mm,
            seat_face: ball.seat_face,
            facet_normal: ball.facet_normal,
            max_normal_z: env.max_normal_z,
        })
        .collect();
    footprint_normals(env.mesh, &pairs, &questions)
        .into_iter()
        .map(|found| found.map(|f| f.normal))
        .collect()
}

/// **The placement normal of every ball, in two batched passes** (the kernel
/// of [`crate::footprint_normal`] over the whole slice at once).
///
/// Pass 1 centres each ball (radius `R = ball_factor · r_lb`) at `A`, giving
/// `u1`. Pass 2 centres it at `D(u1)`: `A` projected onto the plane through
/// the seat triangle's closest point to `A` with normal `u1`, giving `u2`.
/// The answer is `u2` and the angle between `u1` and `u2`, in degrees.
///
/// Each answer depends on its own ball alone, so one call over three balls
/// equals three calls over one, bit for bit.
///
/// # Errors (per ball)
///
/// [`HeadRefusal::NoFootprint`] where either pass finds no admitted, connected
/// footprint under the ball.
#[must_use]
pub fn placement_normals(
    env: &PlacementEnv<'_>,
    balls: &[PlacementBall],
) -> Vec<Result<([f64; 3], f64), HeadRefusal>> {
    let first_centres: Vec<[f64; 3]> = balls.iter().map(|b| b.axis_point_mm).collect();
    let first = footprint_pass(env, balls, &first_centres);
    // Pass 2 runs only for the balls pass 1 measured.
    let live: Vec<usize> = (0..balls.len()).filter(|&i| first[i].is_some()).collect();
    let verts = env.mesh.verts();
    let tris = env.mesh.tris();
    let mut second_balls = Vec::with_capacity(live.len());
    let mut second_centres = Vec::with_capacity(live.len());
    let mut kept = Vec::with_capacity(live.len());
    for &i in &live {
        let (ball, Some(u1)) = (&balls[i], first[i]) else { continue };
        let corners = tris.get(ball.seat_face as usize).and_then(|t| {
            Some([
                *verts.get(t[0] as usize)?,
                *verts.get(t[1] as usize)?,
                *verts.get(t[2] as usize)?,
            ])
        });
        let Some([va, vb, vc]) = corners else { continue };
        let a = ball.axis_point_mm;
        let face_point = closest_on_tri(a, va, vb, vc);
        let centre = vec3::sub(a, vec3::scale(u1, vec3::dot(vec3::sub(a, face_point), u1)));
        second_balls.push(*ball);
        second_centres.push(centre);
        kept.push(i);
    }
    let second = footprint_pass(env, &second_balls, &second_centres);
    let mut out = vec![Err(HeadRefusal::NoFootprint); balls.len()];
    for (&i, u2) in kept.iter().zip(second) {
        let (Some(u1), Some(u2)) = (first[i], u2) else { continue };
        let residual = vec3::dot(u1, u2).clamp(-1.0, 1.0).acos().to_degrees();
        out[i] = Ok((u2, residual));
    }
    out
}

/// A planned head as the placement tests see another head: its axis and its
/// joint's radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadCapsule {
    /// `D`.
    pub contact_mm: [f64; 3],
    /// `K`.
    pub joint_mm: [f64; 3],
    /// `r_K`.
    pub joint_radius_mm: f64,
}

impl HeadCapsule {
    #[must_use]
    pub fn of(head: &Head) -> Self {
        Self {
            contact_mm: head.contact_mm,
            joint_mm: head.joint_mm,
            joint_radius_mm: head.joint_radius_mm,
        }
    }
}

/// What [`check_head`] reads. `mesh` is the triangle store the `surface`
/// index was built from (the index answers WHICH faces are near, not where
/// their corners are).
pub struct HeadCheckCtx<'a> {
    pub grid: &'a Grid,
    pub history: &'a ColumnHistory,
    pub keep_out: &'a DrainKeepOut,
    pub surface: Option<&'a NativeSurface>,
    pub mesh: &'a Mesh,
    pub plate_z_mm: f64,
    pub layer_height_mm: f64,
    pub separation_mm: f64,
}

/// Walk the head's layers `k_joint .. k_top` (the seat layer is the seat's
/// own business): at each, every cell whose centre stands within
/// `r(s) / cos(tilt) + extra_mm` of the axis (the horizontal reach of the
/// inclined section), and the cell holding the axis itself, is offered to
/// `refuses`. `true` as soon as one is refused. No allocation.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "cell ranges floored and clamped into the grid; a layer index is exact in f64"
)]
fn walk_axis(
    head: &Head,
    tip_radius_mm: f64,
    ctx: &HeadCheckCtx<'_>,
    extra_mm: f64,
    mut refuses: impl FnMut(usize, usize) -> bool,
) -> bool {
    let (d, k) = (head.contact_mm, head.joint_mm);
    let length = vec3::dist(d, k);
    let drop_mm = (d[2] - k[2]).max(f64::MIN_POSITIVE);
    let cos_tilt = (-head.unit[2]).max(f64::MIN_POSITIVE);
    let grid = ctx.grid;
    let top = head.k_joint + head.cells.len();
    for layer in head.k_joint..top {
        let z = ctx.plate_z_mm + layer as f64 * ctx.layer_height_mm;
        let t = ((d[2] - z) / drop_mm).clamp(0.0, 1.0);
        let at = vec3::add(d, vec3::scale(vec3::sub(k, d), t));
        let mut radius = head_section_mm(tip_radius_mm, t * length, head.cos_lean_max);
        if t >= 1.0 {
            radius = radius.max(head.joint_radius_mm);
        }
        let reach = radius / cos_tilt + extra_mm;
        let own = cell_holding(grid, at[0], at[1]);
        if own.is_some_and(|cell| refuses(cell, layer)) {
            return true;
        }
        let max_col = grid.cols().saturating_sub(1);
        let max_row = grid.rows().saturating_sub(1);
        let lo_col = grid.col_coord(at[0] - reach).floor().max(0.0) as usize;
        let hi_col = (grid.col_coord(at[0] + reach).floor().max(0.0) as usize).min(max_col);
        let lo_row = grid.row_coord(at[1] - reach).floor().max(0.0) as usize;
        let hi_row = (grid.row_coord(at[1] + reach).floor().max(0.0) as usize).min(max_row);
        for row in lo_row..=hi_row {
            let dy = grid.center_y_mm(row) - at[1];
            for col in lo_col..=hi_col {
                let dx = grid.center_x_mm(col) - at[0];
                let cell = grid.index(row, col);
                if dx * dx + dy * dy <= reach * reach
                    && own != Some(cell)
                    && refuses(cell, layer)
                {
                    return true;
                }
            }
        }
    }
    false
}

/// A triangle clipped to the half-space `(Q − origin)·normal ≥ offset`: a
/// convex polygon of up to four corners, in a fixed array.
fn clip_above(
    triangle: [[f64; 3]; 3],
    origin: [f64; 3],
    normal: [f64; 3],
    offset_mm: f64,
    out: &mut [[f64; 3]; 4],
) -> usize {
    let height = |q: [f64; 3]| vec3::dot(vec3::sub(q, origin), normal) - offset_mm;
    let mut n = 0;
    for i in 0..3 {
        let (cur, nxt) = (triangle[i], triangle[(i + 1) % 3]);
        let (hc, hn) = (height(cur), height(nxt));
        if hc >= 0.0 {
            out[n] = cur;
            n += 1;
        }
        if (hc >= 0.0) != (hn >= 0.0) {
            let t = hc / (hc - hn);
            out[n] = vec3::add(cur, vec3::scale(vec3::sub(nxt, cur), t));
            n += 1;
        }
    }
    n
}

/// The distance from the segment `a`–`b` to the convex polygon `poly`
/// (three or four corners, planar): the distance from a point to a convex set
/// is convex along a segment, so a ternary search over the segment finds the
/// minimum, and each probe is the nearest point of the polygon's fan
/// triangles ([`closest_on_tri`]). A segment through the polygon gives zero.
fn segment_polygon_distance(a: [f64; 3], b: [f64; 3], poly: &[[f64; 3]]) -> f64 {
    let probe = |t: f64| {
        let p = vec3::add(a, vec3::scale(vec3::sub(b, a), t));
        (1..poly.len() - 1)
            .map(|i| vec3::dist(p, closest_on_tri(p, poly[0], poly[i], poly[i + 1])))
            .fold(f64::INFINITY, f64::min)
    };
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for _ in 0..48 {
        let third = (hi - lo) / 3.0;
        let (m1, m2) = (lo + third, hi - third);
        if probe(m1) <= probe(m2) {
            hi = m2;
        } else {
            lo = m1;
        }
    }
    probe(0.5 * (lo + hi))
}

/// **The placement tests of one head**, against everything accepted before it.
/// In order, each a refusal:
///
/// * (a) [`HeadRefusal::HiddenCavity`]: the whole `D→K` solid lies in the
///   hidden-cavity map (`history.hidden`, not a strut's `interior` flag);
/// * (b) [`HeadRefusal::InDrain`]: some cell within `r(s)` of the axis, at a
///   layer `k_joint .. k_top − 1`, is refused by the drain keep-out;
/// * (c) [`HeadRefusal::HitsMember`]: a stamped shaft stands within
///   `r(s) + separation` of the axis at those layers;
/// * (d) [`HeadRefusal::HitsHead`]: the segment distance to an accepted head
///   is under `r_K + r_K,other + separation`;
/// * (e) [`HeadRefusal::HitsModel`]: ONE batched surface query for the
///   segment `D→K` at `r_K + g` finds the candidate faces; each is clipped to
///   the half-space `(Q − D)·u ≥ g` (below it is the contact zone, so the seat
///   surface is exempt) and the clipped part's distance to the axis from
///   `D + g·u` to `K` must stay at least `r_K + g`. A concave wall near the
///   axis above height `g` is refused; the seat face and convex surroundings
///   are not;
/// * (f) [`HeadRefusal::NoSurface`] where there is no surface index (or it
///   refused the query): the model cannot be tested, so no head is built.
///
/// # Errors
///
/// The first failing check's [`HeadRefusal`].
pub fn check_head(
    head: &Head,
    tip_radius_mm: f64,
    ctx: &HeadCheckCtx<'_>,
    accepted: &[HeadCapsule],
) -> Result<(), HeadRefusal> {
    let (d, k, r_k) = (head.contact_mm, head.joint_mm, head.joint_radius_mm);
    // (a) The hidden-cavity map speaks the raster frame: the plate at z = 0.
    let raster = |p: [f64; 3]| [p[0], p[1], p[2] - ctx.plate_z_mm];
    if ctx.history.hidden.segment_hidden(raster(d), raster(k), r_k) {
        return Err(HeadRefusal::HiddenCavity);
    }
    // (b)
    if walk_axis(head, tip_radius_mm, ctx, 0.0, |cell, layer| {
        !ctx.keep_out.admits(cell, layer)
    }) {
        return Err(HeadRefusal::InDrain);
    }
    // (c)
    if walk_axis(head, tip_radius_mm, ctx, ctx.separation_mm, |cell, layer| {
        ctx.history.shaft_blocks(cell, layer, None)
    }) {
        return Err(HeadRefusal::HitsMember);
    }
    // (d)
    for other in accepted {
        let (distance, _, _) =
            geom_kernel::segment_segment_distance(d, k, other.contact_mm, other.joint_mm);
        if distance < r_k + other.joint_radius_mm + ctx.separation_mm {
            return Err(HeadRefusal::HitsHead);
        }
    }
    // (e), (f)
    let Some(surface) = ctx.surface else {
        return Err(HeadRefusal::NoSurface);
    };
    let reach_mm = r_k + head.gap_mm;
    let limits = NearFaceBatchLimits {
        max_rows: 1,
        max_faces_per_row: surface.nfaces(),
        max_total_faces: usize::MAX,
    };
    let Ok(found) = surface.faces_near_segments_batch_classified(&[d], &[k], &[reach_mm], limits)
    else {
        return Err(HeadRefusal::NoSurface);
    };
    let from = vec3::add(d, vec3::scale(head.unit, head.gap_mm));
    let (verts, tris) = (ctx.mesh.verts(), ctx.mesh.tris());
    let mut polygon = [[0.0; 3]; 4];
    for &face in found.rows.first().map_or(&[][..], Vec::as_slice) {
        let Some(&[ia, ib, ic]) = tris.get(face as usize) else { continue };
        let (Some(&va), Some(&vb), Some(&vc)) = (
            verts.get(ia as usize),
            verts.get(ib as usize),
            verts.get(ic as usize),
        ) else {
            continue;
        };
        let corners = clip_above(
            [va, vb, vc],
            d,
            head.unit,
            head.gap_mm,
            &mut polygon,
        );
        if corners >= 3 && segment_polygon_distance(from, k, &polygon[..corners]) < reach_mm {
            return Err(HeadRefusal::HitsModel);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        head_section_mm, post_section_mm, section_radius_mm, solve_bend, solve_joint, Head,
    };
    use crate::{Landing, SeatFace, SweepStrut};

    const GAP_MM: f64 = 0.3;

    fn cos_lean() -> f64 {
        57.5_f64.to_radians().cos()
    }

    #[test]
    fn a_zero_lever_needs_no_more_than_the_ledger() {
        assert!((section_radius_mm(0.2, 0.05, 0.0) - 0.2).abs() < 1e-15);
        assert!((section_radius_mm(0.2, 0.05, f64::NAN) - 0.2).abs() < 1e-15);
    }

    #[test]
    fn the_section_satisfies_its_own_law_with_equality() {
        for (ledger, tip, lever) in [(0.05, 0.05, 0.297), (0.4, 0.15, 0.9), (0.1, 0.3, 0.05)] {
            let r = section_radius_mm(ledger, tip, lever);
            let residual = r * r * r - ledger * ledger * r - 4.0 * tip * tip * lever;
            assert!(residual.abs() < 1e-12, "ledger {ledger}: residual {residual}");
            assert!(r >= ledger);
        }
    }

    #[test]
    fn the_bend_of_the_design_example_is_the_published_one() {
        // r_tip 0.05, phi 41.3 deg, lean 0, g 0.3 gives r_K 0.149, L 0.449,
        // e_K 0.297.
        let factor = 41.3_f64.to_radians().sin();
        let bend = solve_bend(0.05, factor, 0.3, 1.0).expect("within the maximum radius");
        assert!((bend.joint_radius_mm - 0.149).abs() < 1e-3, "{bend:?}");
        assert!((bend.length_mm - 0.449).abs() < 1e-3, "{bend:?}");
        assert!((bend.lever_mm - 0.297).abs() < 1e-3, "{bend:?}");
    }

    #[test]
    fn a_moment_the_maximum_radius_cannot_carry_is_refused() {
        let factor = 80.0_f64.to_radians().sin();
        assert!(solve_bend(0.9, factor, 0.3, 1.0).is_none());
    }

    /// The law with the lean envelope: the worked values of the build plan.
    /// Dropping the `/c` (a secant of 1) makes every number here fail.
    #[test]
    fn the_head_law_gives_the_plan_values() {
        let c = cos_lean();
        assert!((c - 0.5373).abs() < 1e-4, "c = {c}");
        let thin = solve_joint(0.05, GAP_MM, c, 1.0).expect("a thin tip");
        assert!((thin.joint_radius_mm - 0.2204).abs() < 1e-4, "{thin:?}");
        assert!((thin.length_mm - 0.5204).abs() < 1e-4, "{thin:?}");
        let fat = solve_joint(0.15, GAP_MM, c, 1.0).expect("a 0.15 tip");
        assert!((fat.joint_radius_mm - 0.5486).abs() < 1e-4, "{fat:?}");
        assert!((fat.length_mm - 0.8486).abs() < 1e-4, "{fat:?}");
        // The threshold is 0.2944 (0.29438): above it the joint exceeds the
        // 1.0 mm maximum.
        assert!(solve_joint(0.2943, GAP_MM, c, 1.0).is_some());
        assert!(solve_joint(0.2945, GAP_MM, c, 1.0).is_none());
        assert!(solve_joint(0.30, GAP_MM, c, 1.0).is_none());
        // The neck starts at r_t / sqrt(c).
        let neck = head_section_mm(0.05, 0.0, c);
        assert!((neck - 0.0682).abs() < 1e-4, "neck {neck}");
        // The joint's cubic holds with equality at the solution, and the head
        // section at |DK| is the joint's own radius.
        for tip in [0.05, 0.15] {
            let joint = solve_joint(tip, GAP_MM, c, 1.0).expect("solvable");
            let r = joint.joint_radius_mm;
            let residual = r * r * r - tip * tip / c * (r + 4.0 * (GAP_MM + r));
            assert!(residual.abs() < 1e-12, "tip {tip}: residual {residual}");
            let at_joint = head_section_mm(tip, joint.length_mm, c);
            assert!((at_joint - r).abs() < 1e-9, "tip {tip}: {at_joint} vs {r}");
        }
        // The post at K, with a ledger under it, starts no thinner than the joint.
        let r_k = thin.joint_radius_mm;
        let post = post_section_mm(r_k, 0.05, thin.length_mm, c);
        assert!(post >= r_k);
    }

    fn strut(id: u32, k_top: usize, k_bottom: usize) -> SweepStrut {
        SweepStrut {
            id,
            cell: 7,
            center_mm: [1.5, 2.5],
            kind: crate::StrutKind::Arm,
            min_radius_mm: 0.0,
            thickened_mm: Vec::new(),
            seat_on_strut: None,
            k_top,
            k_bottom,
            landing: Landing::Plate,
            interior: false,
            radius_mm: vec![0.2, 0.15, 0.1, 0.05],
            carried_mm2: vec![4.0, 3.0, 2.0, 1.0],
            covered_demand: Vec::new(),
            inserted_by_load: false,
            path: Vec::new(),
            seat_face: SeatFace::default(),
            seat_normal: [0.0; 3],
            straight_fallback: None,
            fallback_path: Vec::new(),
            head: None,
        }
    }

    /// A head over layers 5..=13: the joint at layer 5 in cell 9, the head's
    /// own cells for layers 6..=13 ending in the seat cell 7.
    fn head() -> Head {
        Head {
            contact_mm: [1.5, 2.5, 0.65],
            unit: [0.66, 0.0, -0.75],
            joint_mm: [1.8, 2.5, 0.25],
            k_joint: 5,
            joint_cell: 9,
            joint_radius_mm: 0.2204,
            planned_tip_radius_mm: 0.05,
            gap_mm: GAP_MM,
            cos_lean_max: cos_lean(),
            cells: vec![9, 9, 8, 8, 8, 7, 7, 7],
        }
    }

    fn headed(post_path: &[u32]) -> SweepStrut {
        let mut s = strut(3, 13, 2);
        s.head = Some(head());
        s.set_post(Landing::Plate, 2, post_path.to_vec());
        s
    }

    #[test]
    fn set_post_keeps_the_head_and_a_plain_path_rewrite_breaks_it() {
        // Layers 2..=5 are the post (4 entries), then the 8 head cells.
        let mut s = headed(&[4, 4, 9, 9]);
        assert_eq!(s.path.len(), 12);
        assert_eq!(s.k_top, 13);
        assert_eq!(s.k_bottom, 2);
        assert!(s.head_intact());
        assert_eq!(s.post_top_layer(), 5);
        // A re-route keeps k_top and the head slots.
        s.set_post(Landing::Plate, 1, vec![3, 4, 4, 9, 9]);
        assert_eq!(s.k_top, 13);
        assert_eq!(s.k_bottom, 1);
        assert_eq!(s.path.len(), 13);
        assert_eq!(&s.path[5..], head().cells.as_slice());
        assert!(s.head_intact());
        // An empty post means straight down in the joint cell.
        let mut s = headed(&[]);
        assert_eq!(&s.path[..4], &[9, 9, 9, 9]);
        assert!(s.head_intact());
        // A plain rewrite of the path (what every other mover does) is caught.
        s.path = vec![4; 12];
        assert!(!s.head_intact());
        // A rewrite that keeps the length but moves the joint is caught too.
        let mut s = headed(&[4, 4, 9, 9]);
        s.path[3] = 8;
        assert!(!s.head_intact());
    }

    #[test]
    fn a_straight_arm_set_post_is_the_three_assignments() {
        let mut s = strut(1, 13, 10);
        s.set_post(Landing::Model { layer: 4 }, 11, vec![5, 5, 6]);
        assert_eq!(s.landing, Landing::Model { layer: 4 });
        assert_eq!(s.k_bottom, 11);
        assert_eq!(s.path, vec![5, 5, 6]);
        assert_eq!(s.post_top_layer(), 13);
        assert!(s.joinable_at(13, 0.05));
    }

    #[test]
    fn a_headed_host_is_joinable_only_below_its_joint() {
        let s = headed(&[4, 4, 9, 9]);
        // r_K 0.2204 over 0.05 mm layers: ceil(4.408) = 5 layers.
        assert!(s.joinable_at(0, 0.05));
        assert!(!s.joinable_at(1, 0.05));
        assert!(!s.joinable_at(5, 0.05));
    }

    /// The digest of a field with no head is what it was before heads existed:
    /// pinned against the value the code computed at the start of the lane.
    #[test]
    fn a_strut_with_no_head_digests_exactly_as_before() {
        let mut leaned = strut(2, 13, 10);
        leaned.cell = 8;
        leaned.path = vec![9, 9, 8, 8];
        leaned.landing = Landing::Strut { id: 1, layer: 11 };
        let mut straight = strut(1, 13, 10);
        straight.cell = 7;
        let none = crate::digest(&[straight.clone(), leaned.clone()]);
        assert_eq!(none, PINNED_DIGEST, "a headless digest moved");
        let mut with_head = leaned;
        with_head.head = Some(head());
        assert_ne!(crate::digest(&[straight, with_head]), none);
    }

    /// The adapter lives in this crate and must build the drawing payload
    /// through the public `plinth_support` door, not through `lower`.
    #[test]
    fn the_drawing_payload_is_constructible_across_the_crate_boundary() {
        let payload = plinth_support::SweepHead {
            contact_mm: [0.0; 3],
            unit: [0.0, 0.0, -1.0],
            joint_mm: [0.0, 0.0, -0.52],
            joint_radius_mm: 0.22,
            gap_mm: 0.3,
            profile_mm: vec![0.07, 0.06],
        };
        assert_eq!(payload.profile_mm.len(), 2);
    }

    /// Computed by the code of `dffb6f3`, before heads existed, for the two
    /// struts above.
    const PINNED_DIGEST: &str = "f72fd0a62fafab0e";
}

/// The constructor's own tests: the trigger table, the worked head, one
/// fixture per refusal reason, and the batched placement normal.
#[cfg(test)]
mod constructor_tests {
    use super::{
        check_head, head_triggers, plan_head, placement_normals, skims, Head, HeadCapsule,
        HeadCheckCtx, HeadRefusal, HeadRequest, HeadRule, PlacementBall, PlacementEnv,
    };
    use crate::cavity::fixtures as cavity;
    use crate::drain::{DrainKeepOut, DrainSpec};
    use crate::grid::{lay_out_at, Grid};
    use crate::up_sweep::ColumnHistory;
    use geom_kernel::{closest_on_tri, vec3, NativeSurface, NativeSurfaceConfig};
    use plinth_core::m0_constants;
    use plinth_mesh::Mesh;

    const H: f64 = 0.05;
    const GAP: f64 = 0.3;
    const PIXEL: f64 = 0.0192;
    const SEPARATION: f64 = 0.1;

    fn allowance() -> f64 {
        H + PIXEL
    }

    fn cos_lean() -> f64 {
        57.5_f64.to_radians().cos()
    }

    /// Faces admitted up to 70 degrees from straight down.
    fn rule(plate_z_mm: f64) -> HeadRule {
        HeadRule {
            gap_mm: GAP,
            allowance_mm: allowance(),
            cos_lean_max: cos_lean(),
            max_radius_mm: 1.0,
            typical_tip_radius_mm: 0.15,
            max_normal_z: -70.0_f64.to_radians().cos(),
            layer_height_mm: H,
            plate_z_mm,
        }
    }

    /// The outward normal of a face `deg` from straight down, leaning to +x.
    fn unit_at(deg: f64) -> [f64; 3] {
        [deg.to_radians().sin(), 0.0, -deg.to_radians().cos()]
    }

    fn grid() -> Grid {
        lay_out_at(-2.0, -2.0, 0.1, 100, 100)
    }

    fn surface_of(mesh: &Mesh) -> NativeSurface {
        NativeSurface::from_indexed(
            mesh.verts(),
            mesh.tris(),
            NativeSurfaceConfig { grid_pitch_mm: 0.25, max_grid_bytes: 64 << 20 },
        )
        .expect("a surface")
    }

    /// `(e1, e2)`: a basis of the plane whose normal is `u`.
    fn basis(u: [f64; 3]) -> ([f64; 3], [f64; 3]) {
        let e2 = [0.0, 1.0, 0.0];
        (vec3::normalize(vec3::cross(e2, u)).expect("a basis"), e2)
    }

    /// `origin + a·e + b·f` for the four corners of `[-ha, ha] x [lo_b, hi_b]`.
    fn rect(origin: [f64; 3], e: [f64; 3], f: [f64; 3], a: (f64, f64), b: (f64, f64)) -> [[f64; 3]; 4] {
        let at = |s: f64, t: f64| vec3::add(origin, vec3::add(vec3::scale(e, s), vec3::scale(f, t)));
        [at(a.0, b.0), at(a.1, b.0), at(a.1, b.1), at(a.0, b.1)]
    }

    fn quads_mesh(quads: &[[[f64; 3]; 4]]) -> Mesh {
        let mut verts = Vec::new();
        let mut tris = Vec::new();
        for quad in quads {
            let base = u32::try_from(verts.len()).expect("small");
            verts.extend_from_slice(quad);
            tris.push([base, base + 1, base + 2]);
            tris.push([base, base + 2, base + 3]);
        }
        Mesh::new(verts, tris).expect("quads")
    }

    /// The head of the worked example, D on a face `41.3` degrees from down.
    fn worked_head(contact: [f64; 3], plate_z: f64) -> Head {
        let u = unit_at(41.3);
        let top = ((contact[2] - plate_z) / H).ceil();
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a small positive layer")]
        let k_top = top as usize;
        let req = HeadRequest {
            axis_point_mm: vec3::add(contact, vec3::scale(u, 0.2)),
            face_point_mm: contact,
            unit: u,
            tip_radius_lb_mm: 0.05,
            seat_cell: 5,
            k_top,
        };
        plan_head(&req, &rule(plate_z), &grid()).expect("the worked head")
    }

    struct Scene {
        mesh: Mesh,
        surface: NativeSurface,
        grid: Grid,
        history: ColumnHistory,
        keep_out: DrainKeepOut,
    }

    impl Scene {
        fn of(mesh: Mesh) -> Self {
            let grid = grid();
            Self {
                surface: surface_of(&mesh),
                history: ColumnHistory::new(&grid),
                keep_out: DrainKeepOut::none(),
                grid,
                mesh,
            }
        }

        fn ctx(&self) -> HeadCheckCtx<'_> {
            HeadCheckCtx {
                grid: &self.grid,
                history: &self.history,
                keep_out: &self.keep_out,
                surface: Some(&self.surface),
                mesh: &self.mesh,
                plate_z_mm: 0.0,
                layer_height_mm: H,
                separation_mm: SEPARATION,
            }
        }
    }

    const D: [f64; 3] = [1.5, 1.0, 2.03];

    /// A planar seat face through `D` with the head's own normal, and a
    /// `wall` quad (if any) standing on it `lateral` mm from the axis.
    fn planar_scene(wall_lateral: Option<f64>) -> Scene {
        let u = unit_at(41.3);
        let (e1, e2) = basis(u);
        let mut quads = vec![rect(D, e1, e2, (-1.5, 1.5), (-1.5, 1.5))];
        if let Some(lateral) = wall_lateral {
            quads.push(rect(
                vec3::add(D, vec3::scale(e1, lateral)),
                u,
                e2,
                (0.0, 3.0),
                (-1.5, 1.5),
            ));
        }
        Scene::of(quads_mesh(&quads))
    }

    // ---- (1) the trigger ---------------------------------------------------

    #[test]
    fn the_trigger_table() {
        let r = rule(0.0);
        let cases = [
            (0.05, 32.0, false),
            (0.05, 35.0, true),
            (0.15, 31.0, true),
            (0.05, 0.0, false),
            // Beyond the admitted band: 75 degrees from down, past 70.
            (0.05, 75.0, false),
        ];
        for (radius, deg, expected) in cases {
            let got = head_triggers(unit_at(deg)[2], radius, &r);
            println!("trigger r {radius} at {deg} deg -> {got}");
            assert_eq!(got, expected, "r {radius} at {deg} deg");
        }
        assert!((allowance() - 0.0692).abs() < 1e-12);
        // The bare law: a grazing (non-positive) cosine skims, a square one does not.
        assert!(skims(0.05, 0.0, GAP, allowance()));
        assert!(skims(0.05, -0.2, GAP, allowance()));
        assert!(skims(0.05, f64::NAN, GAP, allowance()));
        assert!(!skims(0.05, 1.0, GAP, allowance()));
        // A face that points up is never steep.
        assert!(!head_triggers(0.5, 0.05, &r));
    }

    // ---- (2) the worked head -----------------------------------------------

    #[test]
    fn the_worked_head_has_the_plan_numbers() {
        // Shifted by a plate height the answer is the same one.
        for plate_z in [0.0, 3.0] {
            let contact = [D[0], D[1], D[2] + plate_z];
            let head = worked_head(contact, plate_z);
            let u = unit_at(41.3);
            assert!((head.joint_radius_mm - 0.2204).abs() < 1e-4, "{}", head.joint_radius_mm);
            let length = vec3::dist(head.contact_mm, head.joint_mm);
            assert!((length - 0.5204).abs() < 1e-4, "|DK| = {length}");
            for axis in 0..3 {
                assert!((head.contact_mm[axis] - contact[axis]).abs() < 1e-12, "D");
                let want = contact[axis] + (GAP + head.joint_radius_mm) * u[axis];
                assert!((head.joint_mm[axis] - want).abs() < 1e-9, "K exact, axis {axis}");
            }
            // K_z over the plate is 1.639 mm: layer 32 at 0.05 mm, rounded toward the plate.
            let expected = ((head.joint_mm[2] - plate_z) / H).floor();
            assert!((expected - 32.0).abs() < f64::EPSILON);
            assert_eq!(head.k_joint, 32, "floor(K_z / h)");
            let k_top = 41;
            assert_eq!(head.cells.len(), k_top - head.k_joint);
            assert_eq!(*head.cells.last().expect("cells"), 5, "the last cell is the seat cell");
            // Every cell lies within half a cell of the axis at its layer.
            let grid = grid();
            let drop_mm = head.contact_mm[2] - head.joint_mm[2];
            for (offset, &cell) in head.cells[..head.cells.len() - 1].iter().enumerate() {
                let layer = head.k_joint + 1 + offset;
                #[expect(clippy::cast_precision_loss, reason = "small layer index")]
                let z = plate_z + layer as f64 * H;
                let t = ((head.contact_mm[2] - z) / drop_mm).clamp(0.0, 1.0);
                let x = head.contact_mm[0] + t * (head.joint_mm[0] - head.contact_mm[0]);
                let y = head.contact_mm[1] + t * (head.joint_mm[1] - head.contact_mm[1]);
                let (col, row) = (grid.col_of(cell as usize), grid.row_of(cell as usize));
                assert!((grid.center_x_mm(col) - x).abs() <= 0.5 * grid.pitch_mm() + 1e-9, "layer {layer} x");
                assert!((grid.center_y_mm(row) - y).abs() <= 0.5 * grid.pitch_mm() + 1e-9, "layer {layer} y");
            }
            let joint_col = grid.col_of(head.joint_cell as usize);
            assert!((grid.center_x_mm(joint_col) - head.joint_mm[0]).abs() <= 0.5 * grid.pitch_mm() + 1e-9);
            println!(
                "worked head (plate {plate_z}): r_K {:.4} |DK| {length:.4} k_joint {} cells {}",
                head.joint_radius_mm,
                head.k_joint,
                head.cells.len()
            );
        }
    }

    // ---- (3) the refusals of the plan -----------------------------------------

    fn request(contact: [f64; 3], radius: f64, k_top: usize) -> HeadRequest {
        let u = unit_at(41.3);
        HeadRequest {
            axis_point_mm: contact,
            face_point_mm: contact,
            unit: u,
            tip_radius_lb_mm: radius,
            seat_cell: 5,
            k_top,
        }
    }

    #[test]
    fn the_plan_refuses_by_reason() {
        let grid = grid();
        let r = rule(0.0);
        let refusal = |req: HeadRequest| plan_head(&req, &r, &grid).expect_err("a refusal");
        let over = refusal(request(D, 0.30, 41));
        println!("r 0.30 -> {over:?}");
        assert_eq!(over, HeadRefusal::JointOverMax);
        // Just under the threshold of 0.2944 still builds.
        assert!(plan_head(&request(D, 0.2943, 41), &r, &grid).is_ok());
        // K below layer 1: D at 0.3 puts K under the plate.
        let low = refusal(request([1.5, 1.0, 0.3], 0.05, 41));
        println!("K under the plate -> {low:?}");
        assert_eq!(low, HeadRefusal::NoPost);
        // K in layer 0 (0.029 mm over the plate).
        assert_eq!(refusal(request([1.5, 1.0, 0.42], 0.05, 41)), HeadRefusal::NoPost);
        // K at or above the seat's own layer.
        let flat_top = refusal(request(D, 0.05, 32));
        println!("k_joint = k_top -> {flat_top:?}");
        assert_eq!(flat_top, HeadRefusal::NoPost);
        // The axis leaves the raster.
        assert_eq!(refusal(request([7.9, 1.0, 2.03], 0.05, 41)), HeadRefusal::NoPost);
        // Not steep: straight down, and a face beyond the admitted band.
        let straight = HeadRequest { unit: unit_at(0.0), ..request(D, 0.05, 41) };
        assert_eq!(refusal(straight), HeadRefusal::NotSteep);
        let beyond = HeadRequest { unit: unit_at(75.0), ..request(D, 0.05, 41) };
        assert_eq!(refusal(beyond), HeadRefusal::NotSteep);
        let shallow = HeadRequest { unit: unit_at(32.0), ..request(D, 0.05, 41) };
        assert_eq!(refusal(shallow), HeadRefusal::NotSteep);
    }

    // ---- (4) check_head ----------------------------------------------------------

    #[test]
    fn a_planar_seat_face_passes() {
        let scene = planar_scene(None);
        let head = worked_head(D, 0.0);
        let verdict = check_head(&head, 0.05, &scene.ctx(), &[]);
        println!("planar seat face -> {verdict:?}");
        assert_eq!(verdict, Ok(()));
    }

    #[test]
    fn a_concave_wall_beside_the_axis_is_refused() {
        let head = worked_head(D, 0.0);
        // 0.2 mm from the axis, rising from the seat plane to 3 mm over it.
        let near = planar_scene(Some(0.2));
        let verdict = check_head(&head, 0.05, &near.ctx(), &[]);
        println!("concave wall at 0.2 mm -> {verdict:?}");
        assert_eq!(verdict, Err(HeadRefusal::HitsModel));
        // The same wall far away (1.2 mm) clears the 0.52 mm sphere + gap.
        let far = planar_scene(Some(1.2));
        assert_eq!(check_head(&head, 0.05, &far.ctx(), &[]), Ok(()));
    }

    /// A lower cap of a sphere of radius 3 about (2, 2, 10), outward normals,
    /// rings every 1.5 degrees to 60 degrees from straight down.
    fn sphere_cap() -> Mesh {
        let (centre, radius) = ([2.0, 2.0, 10.0], 3.0);
        let (rings, segments) = (40_usize, 240_usize);
        let point = |ring: usize, seg: usize| {
            #[expect(clippy::cast_precision_loss, reason = "small indices")]
            let (theta, phi) = (
                (60.0_f64 * ring as f64 / rings as f64).to_radians(),
                (360.0_f64 * seg as f64 / segments as f64).to_radians(),
            );
            [
                centre[0] + radius * theta.sin() * phi.cos(),
                centre[1] + radius * theta.sin() * phi.sin(),
                centre[2] - radius * theta.cos(),
            ]
        };
        let mut verts = vec![point(0, 0)];
        for ring in 1..=rings {
            for seg in 0..segments {
                verts.push(point(ring, seg));
            }
        }
        let at = |ring: usize, seg: usize| -> u32 {
            u32::try_from(if ring == 0 { 0 } else { 1 + (ring - 1) * segments + seg % segments })
                .expect("small")
        };
        let mut tris = Vec::new();
        for ring in 0..rings {
            for seg in 0..segments {
                let (a, b, c, d) = (at(ring, seg), at(ring, seg + 1), at(ring + 1, seg + 1), at(ring + 1, seg));
                if ring == 0 {
                    tris.push([a, c, d]);
                } else {
                    tris.push([a, b, c]);
                    tris.push([a, c, d]);
                }
            }
        }
        // Wind every triangle outward (away from the sphere's centre).
        for tri in &mut tris {
            let [a, b, c] = tri.map(|i| verts[i as usize]);
            let normal = vec3::cross(vec3::sub(b, a), vec3::sub(c, a));
            let mid = vec3::scale(vec3::add(vec3::add(a, b), c), 1.0 / 3.0);
            if vec3::dot(normal, vec3::sub(mid, centre)) < 0.0 {
                tri.swap(1, 2);
            }
        }
        Mesh::new(verts, tris).expect("a sphere cap")
    }

    /// A seat ball just outside the cap, `deg` from straight down, azimuth 0.
    fn sphere_ball(mesh: &Mesh, deg: f64, radius: f64) -> (PlacementBall, [f64; 3]) {
        let radial = unit_at(deg);
        let on = [2.0 + 3.0 * radial[0], 2.0, 10.0 + 3.0 * radial[2]];
        let a = vec3::add(on, vec3::scale(radial, 0.02));
        let (verts, tris) = (mesh.verts(), mesh.tris());
        let mut best = (f64::INFINITY, 0_u32);
        for (id, t) in tris.iter().enumerate() {
            let near = closest_on_tri(a, verts[t[0] as usize], verts[t[1] as usize], verts[t[2] as usize]);
            let distance = vec3::dist(a, near);
            if distance < best.0 {
                best = (distance, u32::try_from(id).expect("small"));
            }
        }
        let t = tris[best.1 as usize];
        let [va, vb, vc] = [verts[t[0] as usize], verts[t[1] as usize], verts[t[2] as usize]];
        let facet = vec3::normalize(vec3::cross(vec3::sub(vb, va), vec3::sub(vc, va))).expect("a facet");
        let constants = m0_constants().expect("registry");
        let profiles = constants.evidence().chitubox_tip_contact_profiles().value();
        (PlacementBall::from_profiles(a, best.1, facet, radius, profiles), on)
    }

    #[test]
    fn a_convex_sphere_underside_passes_and_the_normal_is_radial() {
        let mesh = sphere_cap();
        let (ball, on) = sphere_ball(&mesh, 41.3, 0.05);
        let env = PlacementEnv { mesh: &mesh, surface: None, max_normal_z: rule(0.0).max_normal_z };
        let found = placement_normals(&env, &[ball]);
        let (u2, residual) = found[0].expect("a footprint");
        let radial = unit_at(41.3);
        let off = vec3::dot(u2, radial).clamp(-1.0, 1.0).acos().to_degrees();
        println!("sphere underside: normal {off:.3} deg off radial, residual {residual:.3} deg");
        assert!(off < 1.0, "within 1 degree of the radial normal: {off}");
        assert!(residual.is_finite() && (0.0..1.0).contains(&residual), "residual {residual}");
        // The batched surface route gives the same bits as the scan route.
        let surface = surface_of(&mesh);
        let env_surface = PlacementEnv { surface: Some(&surface), ..env };
        assert_eq!(placement_normals(&env_surface, &[ball]), found);
        // A head on that normal.
        let (verts, tris) = (mesh.verts(), mesh.tris());
        let t = tris[ball.seat_face as usize];
        let face_point = closest_on_tri(
            ball.axis_point_mm,
            verts[t[0] as usize],
            verts[t[1] as usize],
            verts[t[2] as usize],
        );
        let top = (face_point[2] / H).ceil();
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a small positive layer")]
        let k_top = top as usize;
        let req = HeadRequest {
            axis_point_mm: ball.axis_point_mm,
            face_point_mm: face_point,
            unit: u2,
            tip_radius_lb_mm: 0.05,
            seat_cell: 5,
            k_top,
        };
        let head = plan_head(&req, &rule(0.0), &grid()).expect("a head on the sphere");
        let scene = Scene::of(mesh);
        let verdict = check_head(&head, 0.05, &scene.ctx(), &[]);
        println!("convex sphere underside (contact at {on:?}) -> {verdict:?}");
        assert_eq!(verdict, Ok(()));
    }

    #[test]
    fn a_drain_keep_out_across_the_head_refuses_it() {
        let mut scene = planar_scene(None);
        let head = worked_head(D, 0.0);
        let mid = vec3::scale(vec3::add(head.contact_mm, head.joint_mm), 0.5);
        assert_eq!(check_head(&head, 0.05, &scene.ctx(), &[]), Ok(()), "control: no drain");
        let drain = DrainSpec { outlet_mm: mid, outward_axis: [0.0, 1.0, 0.0], radius_mm: 0.3 };
        scene.keep_out = DrainKeepOut::build(&scene.grid, &[drain], 0.1, 0.5, 400, 0.0, H);
        let verdict = check_head(&head, 0.05, &scene.ctx(), &[]);
        println!("drain keep-out across the head -> {verdict:?}");
        assert_eq!(verdict, Err(HeadRefusal::InDrain));
    }

    #[test]
    fn a_shaft_beside_the_head_refuses_it() {
        let mut scene = planar_scene(None);
        let head = worked_head(D, 0.0);
        let mid = vec3::scale(vec3::add(head.contact_mm, head.joint_mm), 0.5);
        // 0.2 mm beside the axis: inside r(s)/cos + separation (about 0.33).
        let beside = super::cell_holding(&scene.grid, mid[0] + 0.2, mid[1]).expect("in the raster");
        scene.history.stamp_shaft(9, 0, 100, beside, &[]);
        let verdict = check_head(&head, 0.05, &scene.ctx(), &[]);
        println!("shaft stamped 0.2 mm beside the head -> {verdict:?}");
        assert_eq!(verdict, Err(HeadRefusal::HitsMember));
        // A shaft 1 mm away does not.
        let mut far = planar_scene(None);
        let far_cell = super::cell_holding(&far.grid, mid[0] + 1.0, mid[1]).expect("in the raster");
        far.history.stamp_shaft(9, 0, 100, far_cell, &[]);
        assert_eq!(check_head(&head, 0.05, &far.ctx(), &[]), Ok(()));
    }

    #[test]
    fn a_second_head_close_to_the_first_is_refused() {
        let scene = planar_scene(None);
        let head = worked_head(D, 0.0);
        let mut other = HeadCapsule::of(&head);
        other.contact_mm[1] += 0.3;
        other.joint_mm[1] += 0.3;
        let verdict = check_head(&head, 0.05, &scene.ctx(), &[other]);
        println!("second head 0.3 mm away -> {verdict:?}");
        assert_eq!(verdict, Err(HeadRefusal::HitsHead));
        // 1.0 mm away clears r_K + r_K + separation = 0.54.
        other.contact_mm[1] += 0.7;
        other.joint_mm[1] += 0.7;
        assert_eq!(check_head(&head, 0.05, &scene.ctx(), &[other]), Ok(()));
    }

    #[test]
    fn a_head_inside_a_hidden_cavity_is_refused() {
        let mesh = cavity::cavity_box(false, false);
        let (map, cavity_grid) = cavity::map_of(&mesh, &[]);
        // Under the cavity's ceiling (z = 21), well inside it.
        let contact = [5.0, 5.0, 19.02];
        let u = unit_at(41.3);
        let req = HeadRequest {
            axis_point_mm: contact,
            face_point_mm: contact,
            unit: u,
            tip_radius_lb_mm: 0.05,
            seat_cell: 5,
            k_top: 381,
        };
        let head = plan_head(&req, &rule(0.0), &cavity_grid).expect("a head in the cavity");
        let surface = surface_of(&mesh);
        let mut history = ColumnHistory::new(&cavity_grid);
        let keep_out = DrainKeepOut::none();
        let check = |history: &ColumnHistory| {
            let ctx = HeadCheckCtx {
                grid: &cavity_grid,
                history,
                keep_out: &keep_out,
                surface: Some(&surface),
                mesh: &mesh,
                plate_z_mm: 0.0,
                layer_height_mm: H,
                separation_mm: SEPARATION,
            };
            check_head(&head, 0.05, &ctx, &[])
        };
        assert_eq!(check(&history), Ok(()), "control: no map");
        history.hidden = map;
        let verdict = check(&history);
        println!("head in a hidden cavity -> {verdict:?}");
        assert_eq!(verdict, Err(HeadRefusal::HiddenCavity));
    }

    #[test]
    fn no_surface_index_is_a_refusal() {
        let scene = planar_scene(None);
        let head = worked_head(D, 0.0);
        let mut ctx = scene.ctx();
        ctx.surface = None;
        let verdict = check_head(&head, 0.05, &ctx, &[]);
        println!("no surface -> {verdict:?}");
        assert_eq!(verdict, Err(HeadRefusal::NoSurface));
    }

    // ---- (5) the batched placement normal -------------------------------------------

    #[test]
    fn three_balls_in_one_call_equal_three_calls_bit_for_bit() {
        let mesh = sphere_cap();
        let surface = surface_of(&mesh);
        let env = PlacementEnv { mesh: &mesh, surface: Some(&surface), max_normal_z: rule(0.0).max_normal_z };
        let (low, _) = sphere_ball(&mesh, 20.0, 0.05);
        let (mid, _) = sphere_ball(&mesh, 41.3, 0.15);
        // A ball in the air over nothing: no footprint.
        let air = PlacementBall { axis_point_mm: [2.0, 2.0, 50.0], ..low };
        let balls = [low, mid, air];
        let together = placement_normals(&env, &balls);
        let apart: Vec<_> = balls.iter().map(|b| placement_normals(&env, &[*b])[0]).collect();
        assert_eq!(together, apart);
        assert!(together[0].is_ok() && together[1].is_ok());
        assert_eq!(together[2], Err(HeadRefusal::NoFootprint));
        println!("3-ball call: {together:?}");
        // The same without the surface index (the scan route).
        let scan = PlacementEnv { surface: None, ..env };
        assert_eq!(placement_normals(&scan, &balls), together);
    }
}
