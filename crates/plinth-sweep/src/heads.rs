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
//!          \  head  K  the joint sphere's centre, K = D + (g + hb + L + r_K)·u
//!           ● K     g  the registered final clearance gap
//!           |       hb the contact ball's height over the plane (2R)
//!           |       L  the bare link, a cylinder of the tip's radius
//!           |  post  K -> base, the rest of the arm
//! ```
//!
//! On a steep face the arm's top is a short straight HEAD along `u`, so the
//! contact arrives square. The head is the contact ball, a bare LINK of length
//! `L` (`link_mm`) and the tip's own radius, then a cone to the joint sphere.
//! The joint sphere stands `g + hb + L` off the seat plane: `g` clear of the
//! link's far end, so the post is never inside the gap. The up-sweep builds a head once,
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
//! * joint, PLANNED: `r_K³ = (r_t²/c)(r_K + 4(g' + r_K))` with `g' = g + hb`
//!   ([`solve_joint`]), at the planning tip. This is the size placement used;
//! * joint, DRAWN: the head law at the measured `L = |DK|`,
//!   `r_K³ = (r_t²/c)(r_K + 4L)` ([`final_joint_radius_mm`]), at the FINAL
//!   tip. `K` and `L` are frozen, so the lever is fixed whatever the ball's
//!   size and the joint follows the final tip down as well as up;
//! * post below `K`, at distance `σ` down a post of length `ℓ`:
//!   `r³ ≥ r_led²·r + (4r_t²/c)·L·(1 − σ/ℓ)` ([`post_section_mm`]), never
//!   narrower than the drawn joint.

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
    /// `K`: the joint sphere's centre, exact mm, `D + (g + hb + r_K)·u`.
    pub joint_mm: [f64; 3],
    /// `floor(K_z / layer height)` over the plate, rounded toward the plate:
    /// the layer the post's grid route starts at.
    pub k_joint: usize,
    /// The cell of `K` at `k_joint`.
    pub joint_cell: u32,
    /// The planned `r_K` ([`solve_joint`]) at the tip radius the head was
    /// planned with: what placement used for the stand-off and the clearance
    /// tests. The DRAWN joint is [`final_joint_radius_mm`] at the final tip
    /// over the frozen `|DK|`, which is smaller than this for a final tip
    /// below the planned one and larger for a fatter one.
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
    /// `hb`: the height the contact ball stands over the seat plane at the
    /// planned tip radius, `2R` for a ball tangent to the plane (a seated ball
    /// stands lower, so this is an upper bound). The joint is planned `g + hb`
    /// off the plane, so the bare neck between the ball's far side and the
    /// joint sphere is at least `g` at the planned radius; without it the
    /// joint stood `g` off the PLANE and the ball, which stands `hb` over the
    /// plane, took the whole neck back (measured: 30 drawn heads, neck min
    /// -0.060 mm, balls overlapping the joint).
    pub ball_allowance_mm: f64,
    /// `L_link`: the registered length of the bare LINK, the cylinder of the
    /// tip's own radius that runs from where it leaves the contact ball to
    /// the cone (Craig 2026-10-08: "neck 0.3-0.6, short is fine"). The joint
    /// stands `g` beyond the link's far end, so `|DK| = g + hb + L_link + r_K`.
    pub link_mm: f64,
    /// Set after the final sizing when the head is given up
    /// ([`downgrade_grown_heads`]): the arm is DRAWN as today's straight tip,
    /// from its seat to its base, while the strut keeps the head, its path and
    /// its cells, because the members that landed on it were placed against
    /// them. The drawing follows ([`crate::adapter`]): members joined to the
    /// post are moved by the displacement between the post's old axis (`K` to
    /// the base) and the straight line now drawn.
    pub drawn_straight: bool,
    /// The law the head was PLANNED with: the direction of the admissible
    /// post that gave the largest joint ([`solve_joint_for_axis`]). Every pass
    /// before the final re-size sizes the head, joint and post on it.
    pub law_planned: HeadLaw,
    /// The law the head is DRAWN with: `c` = cos of the drawn post's lean and
    /// the head-to-post angle `phi'` measured from the exact `K`
    /// ([`HeadLaw::of_post`]). `None` until the final re-size
    /// ([`crate::down_sweep::resize_heads_to_drawn_posts`]) has run.
    pub law_drawn: Option<HeadLaw>,
    /// The area-law ledger radii of the whole strut, one per slot, saved
    /// before the head minimum was applied on the last down-sweep pass. The
    /// final re-size and a downgrade rebuild the strut from it. Empty until
    /// the first down-sweep pass has run.
    pub ledger_radius_mm: Vec<f64>,
    /// **The certified sizing of this head** (Craig 2026-10-11: size the joint
    /// for what the contact CAN pass): the cone knots and the joint radius from
    /// ONE solve ([`crate::joint_cert::certified_profile`]), the post sized
    /// for the loads that reach it ([`crate::joint_size`]). Set by the final
    /// sizing ([`crate::down_sweep::resize_heads_to_drawn_posts`]); the adapter
    /// and the lowering read the knots and the joint radius from here, never
    /// from the old law. `None`: the head is on the old law
    /// ([`Self::old_law`] says why).
    pub certified: Option<CertifiedHead>,
    /// Why this head stayed on the old law, when the final sizing ran and could
    /// not certify it.
    pub old_law: Option<OldLawReason>,
}

/// The certified sizing of one head and what it cost, from
/// [`crate::joint_size::certify`].
#[derive(Clone, Debug, PartialEq)]
pub struct CertifiedHead {
    /// The cone knots `(s, r)` from the link's far end to `K`, and the joint
    /// radius: one profile (R5).
    pub profile: crate::joint_cert::CertifiedProfile,
    /// Post slots sized by the certificate (at or under today's radius),
    /// slots that kept today's radius because the need was above it, and
    /// slots that kept it because a junction hull's ring stands on them (R8).
    pub slots_certified: usize,
    pub slots_kept: usize,
    pub slots_pinned: usize,
    /// The largest certified demand over the drawn head and the drawn post
    /// chords (1 means exactly at capacity; above 1 + 1e-9 is warned).
    pub drawn_max: f64,
    /// Post slots whose drawn radius stays under the certificate's own
    /// computed need for them (a slot that kept today's radius, or a hull's,
    /// although the need is larger: `CHONKY.md` P3). Warned, never refused.
    pub slots_under_need: usize,
    /// The worst such slot: `(need, drawn)` radii in mm, the largest
    /// `need / drawn`. `None` when no slot is under its need.
    pub worst_under_need: Option<(f64, f64)>,
    /// The member's ledger as the old law left it before the certificate
    /// rewrote it, slot by slot: the geometry the junction hulls are planned on
    /// and keep (R8). The adapter builds a reference field from these radii
    /// ([`crate::adapter::hull_reference_arms`]).
    pub today_radius_mm: Vec<f64>,
}

/// Why a head stayed on the old law (R1: when the entry topology cannot be
/// established, leave it on the old law and count it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OldLawReason {
    /// The lowering's seat for the tip could not be derived.
    NoSeat,
    /// Another member loads the head (lands on it, or its drawn solid touches
    /// the certified head): the shared head.
    Shared,
    /// No certified profile exists below the safety cap.
    Infeasible,
}

impl Head {
    /// The law the head is drawn with: the drawn one once the final re-size
    /// has run, the planned one before it.
    #[must_use]
    pub fn law(&self) -> HeadLaw {
        self.law_drawn.unwrap_or(self.law_planned)
    }

    /// The gap the joint law reads: `g + hb + L_link`, so
    /// `|DK| = lever_gap_mm + r_K`.
    #[must_use]
    pub fn lever_gap_mm(&self) -> f64 {
        self.gap_mm + self.ball_allowance_mm + self.link_mm
    }
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

/// **The two angles that size a head, joint and post** (the exact two-force
/// law, plan `heads-v19-fixes-plan.md` section "Physics check"). The post
/// pushes along its own axis with `F/c`, where `c = cos` of the post's lean
/// from vertical. The head sees that force at the angle `phi'` between the
/// head's axis line and the post's axis line: `cos phi'` of it is axial and
/// `sin phi'` of it is transverse, and the transverse part acts through the
/// lever `s`. The head's section is then
///
/// ```text
/// r(s)³ = (r_t²/c) · ( r · |cos phi'| + 4 · s · sin phi' )
/// ```
///
/// and the post's, at a `lever` below the joint over a ledger section `r_led`,
/// `r³ = r_led²·r + (4·r_t²/c) · sin phi' · lever`. Both terms are physics.
/// [`HeadLaw::envelope`] (`|cos| = sin = 1`) is the law's upper bound for
/// every post the lean law admits, and the form the code used before.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadLaw {
    /// `c`: cosine of the post's lean from vertical.
    pub cos_lean: f64,
    /// `|cos phi'|`.
    pub cos_phi_abs: f64,
    /// `sin phi'`.
    pub sin_phi: f64,
}

impl HeadLaw {
    /// The envelope at `c`: `|cos phi'| = sin phi' = 1`.
    #[must_use]
    pub const fn envelope(cos_lean: f64) -> Self {
        Self {
            cos_lean,
            cos_phi_abs: 1.0,
            sin_phi: 1.0,
        }
    }

    /// The law of a post that runs along the unit direction `post_dir` (from
    /// `K` toward the base) under a head whose axis is the unit `head_unit`.
    /// `c` is read off the post (no clamp at the lean law's maximum: a post
    /// that leans further than the law admits grows its joint). `None` where
    /// the post does not descend.
    #[must_use]
    pub fn of_post(head_unit: [f64; 3], post_dir: [f64; 3]) -> Option<Self> {
        let post = vec3::normalize(post_dir)?;
        let unit = vec3::normalize(head_unit)?;
        let cos_lean = -post[2];
        if !(cos_lean.is_finite() && cos_lean > 0.0) {
            return None;
        }
        let along = vec3::dot(unit, post).clamp(-1.0, 1.0);
        Some(Self {
            cos_lean,
            cos_phi_abs: along.abs(),
            sin_phi: (1.0 - along * along).max(0.0).sqrt(),
        })
    }

    /// The larger of each angle term over `laws` (the head and joint are drawn
    /// for the steepest and the most side-loading post segment alike), with
    /// the smallest `c`. `None` for no law.
    #[must_use]
    pub fn enveloped(laws: &[Self]) -> Option<Self> {
        let (first, rest) = laws.split_first()?;
        Some(rest.iter().fold(*first, |acc, law| Self {
            cos_lean: acc.cos_lean.min(law.cos_lean),
            cos_phi_abs: acc.cos_phi_abs.max(law.cos_phi_abs),
            sin_phi: acc.sin_phi.max(law.sin_phi),
        }))
    }
}

/// The head's section at axial distance `s_mm` from the contact centre under
/// the law `law`: the root of `r³ = (r_t²/c)(r·|cos phi'| + 4·s·sin phi')`,
/// never thinner than the contact `r_t` (a head with no axial or transverse
/// load is still as thick as the area-law tip it carries). At the envelope
/// and `s = 0` this is `r_t/√c`, the neck's start.
#[must_use]
pub fn head_section_law_mm(tip_radius_mm: f64, s_mm: f64, law: HeadLaw) -> f64 {
    let root_a = tip_radius_mm / law.cos_lean.sqrt();
    let ledger = root_a * law.cos_phi_abs.sqrt();
    section_radius_mm(ledger, root_a, s_mm * law.sin_phi).max(tip_radius_mm)
}

/// The head's section at axial distance `s_mm` from the contact centre:
/// `r³ = (r_t²/c)(r + 4s)`, the envelope. At `s = 0` this is `r_t/√c`, the
/// neck's start.
#[must_use]
pub fn head_section_mm(tip_radius_mm: f64, s_mm: f64, cos_lean_max: f64) -> f64 {
    head_section_law_mm(tip_radius_mm, s_mm, HeadLaw::envelope(cos_lean_max))
}

/// One candidate post direction's joint: the root of
/// `r³ = a·r·|cos phi'| + 4·a·sin phi'·(g' + r)`, `a = r_t²/c`, i.e. the head
/// law at the lever `|DK| = g' + r`. `P` and `Q` are the cubic `r³ = P·r + Q`.
fn joint_cubic(a: f64, cos_phi_abs: f64, sin_phi: f64, lever_gap_mm: f64) -> (f64, f64) {
    (
        a * (cos_phi_abs + 4.0 * sin_phi),
        4.0 * a * sin_phi * lever_gap_mm,
    )
}

/// The positive root of `r³ = p·r + q` (`p, q >= 0`), by bisection.
fn cubic_root(p: f64, q: f64) -> f64 {
    let (mut low, mut high) = (0.0_f64, p.sqrt() + q.cbrt());
    for _ in 0..BISECTIONS {
        let mid = 0.5 * (low + high);
        if mid * (mid * mid - p) - q >= 0.0 {
            high = mid;
        } else {
            low = mid;
        }
    }
    high
}

/// Lean steps of the placement envelope's direction grid, `0 ..= λmax`.
const ENVELOPE_LEAN_STEPS: usize = 24;
/// Azimuth steps of the grid, `0 ..= π` (the law is symmetric about the head's
/// own vertical plane).
const ENVELOPE_AZIMUTH_STEPS: usize = 64;
/// Shrink rounds of the local refinement around the grid's best direction: a
/// numerical resolution (the maximum is smooth), not a physical number.
const ENVELOPE_REFINE_ROUNDS: usize = 40;

/// **The joint of a head axis `unit` at the placement, as the maximum of the
/// exact law over every admissible post** (a post pointing down with a lean
/// in `[0, λmax]`, `cos λmax = cos_lean_max`, at every azimuth).
///
/// For a post at lean `θ` and azimuth `α` from the head's own vertical plane,
/// `cos phi' = cos φu·cos θ + sin φu·sin θ·cos α`, where `φu` is the head
/// axis's angle from straight down. The joint is the root of the cubic of
/// [`joint_cubic`] at the planned lever `|DK| = g' + r_K`; the largest root is
/// found on a fixed dense grid of `(θ, α)` and polished by a shrinking local
/// search, so it is a numerical maximum, not a threshold. It carries every
/// post the lean law admits, so a later re-route can never overload the joint.
/// Never under the area-law tip `r_t`. `None` where it exceeds
/// `max_radius_mm`. Also returns the law of the winning direction, the one the
/// passes before the final re-size size on.
#[must_use]
pub fn solve_joint_for_axis(
    tip_radius_mm: f64,
    lever_gap_mm: f64,
    unit: [f64; 3],
    cos_lean_max: f64,
    max_radius_mm: f64,
) -> Option<(BendSolution, HeadLaw)> {
    let usable = tip_radius_mm.is_finite()
        && tip_radius_mm > 0.0
        && lever_gap_mm.is_finite()
        && lever_gap_mm >= 0.0
        && cos_lean_max.is_finite()
        && cos_lean_max > 0.0
        && max_radius_mm.is_finite();
    let unit = vec3::normalize(unit).filter(|_| usable)?;
    let cos_up = (-unit[2]).clamp(-1.0, 1.0);
    let (cos_u, sin_u) = (cos_up, (1.0 - cos_up * cos_up).max(0.0).sqrt());
    let lean_max = cos_lean_max.clamp(f64::MIN_POSITIVE, 1.0).acos();
    let tip_squared = tip_radius_mm * tip_radius_mm;
    // The law at a `(lean, azimuth)`.
    let law_at = |lean: f64, azimuth: f64| -> HeadLaw {
        let cos_phi = (cos_u * lean.cos() + sin_u * lean.sin() * azimuth.cos()).clamp(-1.0, 1.0);
        HeadLaw {
            cos_lean: lean.cos().max(f64::MIN_POSITIVE),
            cos_phi_abs: cos_phi.abs(),
            sin_phi: (1.0 - cos_phi * cos_phi).max(0.0).sqrt(),
        }
    };
    let excess = |radius: f64, law: HeadLaw| -> f64 {
        let a = tip_squared / law.cos_lean;
        let (p, q) = joint_cubic(a, law.cos_phi_abs, law.sin_phi, lever_gap_mm);
        radius * (radius * radius - p) - q
    };
    // Running best: the radius, the law that gave it and its `(lean,
    // azimuth)`. The area-law tip is the floor (`|cos phi'| = 0` and `s = 0`
    // give a root of zero); a post exactly along the head carries the contact
    // with no transverse load.
    let mut best = (tip_radius_mm, HeadLaw { cos_lean: 1.0, cos_phi_abs: 1.0, sin_phi: 0.0 });
    let mut winner = (0.0_f64, 0.0_f64);
    // The root is the unique positive zero of a function that is negative
    // below it and positive above, so a candidate beats the best exactly when
    // it is still negative at the best radius.
    let mut consider = |lean: f64, azimuth: f64| {
        let law = law_at(lean, azimuth);
        if excess(best.0, law) < 0.0 {
            let a = tip_squared / law.cos_lean;
            let (p, q) = joint_cubic(a, law.cos_phi_abs, law.sin_phi, lever_gap_mm);
            best = (cubic_root(p, q).max(best.0), law);
            Some((lean, azimuth))
        } else {
            None
        }
    };
    #[expect(
        clippy::cast_precision_loss,
        reason = "grid step counts of at most 64 are exact in f64"
    )]
    let (lean_step, azimuth_step) = (
        lean_max / ENVELOPE_LEAN_STEPS as f64,
        core::f64::consts::PI / ENVELOPE_AZIMUTH_STEPS as f64,
    );
    for i in 0..=ENVELOPE_LEAN_STEPS {
        #[expect(clippy::cast_precision_loss, reason = "a grid index is exact in f64")]
        let lean = lean_step * i as f64;
        for j in 0..=ENVELOPE_AZIMUTH_STEPS {
            #[expect(clippy::cast_precision_loss, reason = "a grid index is exact in f64")]
            let azimuth = azimuth_step * j as f64;
            winner = consider(lean, azimuth).unwrap_or(winner);
        }
    }
    // Polish: a compass search around the grid's winner on the same domain,
    // its step halving each round.
    let (mut step_lean, mut step_azimuth) = (lean_step, azimuth_step);
    for _ in 0..ENVELOPE_REFINE_ROUNDS {
        let centre = winner;
        for dl in [-1.0_f64, 0.0, 1.0] {
            for da in [-1.0_f64, 0.0, 1.0] {
                winner = consider(
                    (centre.0 + dl * step_lean).clamp(0.0, lean_max),
                    (centre.1 + da * step_azimuth).clamp(0.0, core::f64::consts::PI),
                )
                .unwrap_or(winner);
            }
        }
        step_lean *= 0.5;
        step_azimuth *= 0.5;
    }
    let (radius, law) = best;
    if radius > max_radius_mm {
        return None;
    }
    let length_mm = lever_gap_mm + radius;
    Some((
        BendSolution {
            joint_radius_mm: radius,
            length_mm,
            lever_mm: length_mm,
        },
        law,
    ))
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

/// **The joint a FROZEN head must carry at its final tip radius**: the head's
/// own section law ([`head_section_mm`]) at the head's measured length
/// `|DK| = head_length_mm`, the distance from the contact centre `D` to the
/// joint centre `K`. With `a = r_t²/c` it is the root of `r³ = a(r + 4L)`.
///
/// `K` and `|DK|` are frozen at placement, so the lever of the tip's load at
/// `K` is at most `|DK|` whatever the ball's size; the joint at the final tip
/// needs no re-solve and no planned value. It can be smaller than the planned
/// joint (a final tip below the planned one) or larger (a fatter tip), and it
/// equals [`solve_joint`]'s radius when the final tip is the planned one and
/// `|DK|` is that joint's own `g + r_K`. A smaller ball only lengthens the
/// bare stalk, so the clearance cannot get worse; a larger one is caught by
/// [`downgrade_grown_heads`].
#[must_use]
pub fn final_joint_radius_mm(tip_radius_mm: f64, head_length_mm: f64, cos_lean_max: f64) -> f64 {
    head_section_mm(tip_radius_mm, head_length_mm, cos_lean_max)
}

/// [`final_joint_radius_mm`] under the exact law `law`.
#[must_use]
pub fn final_joint_radius_law_mm(tip_radius_mm: f64, head_length_mm: f64, law: HeadLaw) -> f64 {
    head_section_law_mm(tip_radius_mm, head_length_mm, law)
}

/// **The head drawn as a polyline of straight frusta that never goes under
/// its section law** ([`head_section_mm`]): `(s, r)` knots from the contact
/// centre (`s = 0`) to the joint (`s = length_mm`), where `s` is the distance
/// along the head's axis from `D`.
///
/// The law `r³ = a(r + 4s)`, `a = r_t²/c`, is concave in `s` (its slope
/// `dr/ds = 4a/(3r² − a)` falls as `r` grows; it is 2 at `s = 0`), so it lies
/// on or under each of its tangents. The tangents at `s_i = L(i/3)³`,
/// `i = 0..3`, are drawn, and the polyline runs through the two exact ends
/// `(0, r_t/√c)` and `(L, r(L))` and the three points where consecutive
/// tangents meet: at most four frusta, no steps, never narrower than the law,
/// the ends exact (the last equals [`final_joint_radius_mm`], the ball's own
/// radius). The cubic spacing puts the knots where the law bends most, next
/// to the contact. A head of no length is the single point at the contact.
#[must_use]
pub fn head_polyline_mm(tip_radius_mm: f64, length_mm: f64, cos_lean_max: f64) -> Vec<[f64; 2]> {
    head_polyline_law_mm(tip_radius_mm, length_mm, HeadLaw::envelope(cos_lean_max))
}

/// [`head_polyline_mm`] under the exact law `law` (`r³ = P·r + Q·s` with
/// `P = (r_t²/c)|cos phi'|` and `Q = 4(r_t²/c) sin phi'`; the slope is
/// `Q/(3r² − P)`, falling as `r` grows, so the law is concave and the tangents
/// lie over it). Where `P` is under `r_t²` the law's root at the contact is
/// thinner than the tip, and the section is floored at `r_t` until the root
/// passes it at `s₀`; the polyline then holds `r_t` to `s₀` and puts its three
/// tangents on `[s₀, L]`, so it is still at most four frusta and never under
/// the floored law. A head the floor holds to its end is the straight
/// cylinder of radius `r_t`.
#[must_use]
pub fn head_polyline_law_mm(tip_radius_mm: f64, length_mm: f64, law: HeadLaw) -> Vec<[f64; 2]> {
    head_polyline_from_law_mm(tip_radius_mm, 0.0, length_mm, law)
}

/// [`head_polyline_law_mm`] over `[from_mm, length_mm]` instead of
/// `[0, length_mm]`: the first knot is the law at `from_mm`, the last the law
/// at `length_mm`, and the tangents are spaced over the span between them.
/// `from_mm = 0` is [`head_polyline_law_mm`], bit for bit. A span of no length
/// is the single point at `from_mm`.
#[must_use]
pub fn head_polyline_from_law_mm(
    tip_radius_mm: f64,
    from_mm: f64,
    length_mm: f64,
    law: HeadLaw,
) -> Vec<[f64; 2]> {
    let start = head_section_law_mm(tip_radius_mm, from_mm, law);
    if !(length_mm.is_finite() && length_mm > from_mm) {
        return vec![[from_mm, start]];
    }
    let a = tip_radius_mm * tip_radius_mm / law.cos_lean;
    let p = a * law.cos_phi_abs;
    let q = 4.0 * a * law.sin_phi;
    // Where the floor stops holding: the root passes `r_t` at `s₀`.
    let floor_end = if p < tip_radius_mm * tip_radius_mm {
        if q > 0.0 {
            (tip_radius_mm * (tip_radius_mm * tip_radius_mm - p)) / q
        } else {
            f64::INFINITY
        }
    } else {
        0.0
    };
    if floor_end >= length_mm {
        return vec![[from_mm, start], [length_mm, start]];
    }
    let floored = floor_end > from_mm;
    let tangent_count = if floored { 3_usize } else { 4_usize };
    let base = floor_end.max(from_mm);
    let span = length_mm - base;
    // The tangent points `(s, r, slope)`.
    let tangents: Vec<[f64; 3]> = (0..tangent_count)
        .map(|index| {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a tangent index of at most 3 is exact in f64"
            )]
            let s = base + span * (index as f64 / (tangent_count - 1) as f64).powi(3);
            let r = head_section_law_mm(tip_radius_mm, s, law);
            [s, r, q / (3.0 * r * r - p)]
        })
        .collect();
    let mut knots = vec![[from_mm, start]];
    if floored {
        knots.push([floor_end, tip_radius_mm]);
    }
    for pair in tangents.windows(2) {
        let [s_lo, r_lo, m_lo] = pair[0];
        let [s_hi, r_hi, m_hi] = pair[1];
        // The lines `r = r_i + m_i (s - s_i)` meet at `s`; it lies between
        // the two tangent points for a concave law.
        let slope_gap = m_lo - m_hi;
        let s = if slope_gap > 1.0e-12 {
            ((r_hi - r_lo) + m_lo * s_lo - m_hi * s_hi) / slope_gap
        } else {
            0.5 * (s_lo + s_hi)
        }
        .clamp(s_lo, s_hi);
        knots.push([s, r_lo + m_lo * (s - s_lo)]);
    }
    knots.push([length_mm, head_section_law_mm(tip_radius_mm, length_mm, law)]);
    knots
}

/// **The head as it is drawn with its link** (Craig 2026-10-08): the contact
/// ball, a bare cylinder of radius `r_L` and length `L_link` (the LINK), then a
/// cone to the joint sphere.
///
/// All distances `s` run along the head's axis from the contact centre `D`.
/// The ball stands `2R` over `D` at most, so the link's far end is planned at
/// `s_far = 2R + L_link`; `r_L` is the tip's own radius (the contact's area-law
/// need, ruling A: each part is sized for its own requirement), so the link is
/// never wider than the ball.
///
/// A link that thin lies UNDER the bending law at `s_far` (the law there is
/// 1.3 to 3.9 times the ball for a side-loaded head); Craig's ChituBox Light
/// tip does the same. [`section_stress_ratio`] measures how far under, and the
/// run warns per head ([`link_overstress`]). The cone leaves `(s_far, r_L)`
/// along the line of the law's own slope there, and runs on until it meets the
/// law at `s_c`; from `s_c` to the joint the tangent polyline of
/// [`head_polyline_from_law_mm`] keeps the head on or above the law. Where the
/// line does not meet the law before the joint (a short head), the cone is one
/// straight frustum from `(s_far, r_L)` to the joint's own section. A point
/// under a concave law has no tangent to it, which is why the cone starts
/// along the law's slope and is not tangent. At most five frusta, no step.
///
/// Where the link would reach the joint (`s_far` not before `length_mm`) the
/// cone starts a micron before it.
#[derive(Clone, Debug, PartialEq)]
pub struct HeadProfile {
    /// `r_L`: the link's radius.
    pub link_radius_mm: f64,
    /// `s_far`: where the link ends and the cone starts, from `D`.
    pub link_end_mm: f64,
    /// The cone's `(s, r)` knots, from `(s_far, r_L)` to `(length, r_K)`.
    pub knots: Vec<[f64; 2]>,
}

/// See [`HeadProfile`].
#[must_use]
pub fn head_profile_with_link(
    tip_radius_mm: f64,
    ball_radius_mm: f64,
    link_mm: f64,
    length_mm: f64,
    law: HeadLaw,
) -> HeadProfile {
    let link_radius_mm = tip_radius_mm;
    let planned_end = ball_allowance_of(ball_radius_mm) + link_mm;
    let link_end_mm = planned_end.min(length_mm - 1.0e-3).max(0.0);
    let joint = head_section_law_mm(tip_radius_mm, length_mm, law);
    let at_end = head_section_law_mm(tip_radius_mm, link_end_mm, law);
    let finish = |knots: Vec<[f64; 2]>| HeadProfile {
        link_radius_mm,
        link_end_mm,
        knots,
    };
    if !(length_mm.is_finite() && length_mm > link_end_mm) {
        return finish(vec![[link_end_mm, link_radius_mm]]);
    }
    if at_end <= link_radius_mm + 1.0e-12 {
        // The link is on the (floored) law: the tangent polyline from its end.
        let mut knots = head_polyline_from_law_mm(tip_radius_mm, link_end_mm, length_mm, law);
        knots[0] = [link_end_mm, link_radius_mm];
        return finish(knots);
    }
    // The law's slope at `s_far`: `Q / (3 r² − P)`.
    let a = tip_radius_mm * tip_radius_mm / law.cos_lean;
    let (p, q) = (a * law.cos_phi_abs, 4.0 * a * law.sin_phi);
    let slope = q / (3.0 * at_end * at_end - p);
    let line = |s: f64| link_radius_mm + slope * (s - link_end_mm);
    let under = |s: f64| line(s) - head_section_law_mm(tip_radius_mm, s, law);
    if !(slope.is_finite() && under(length_mm) >= 0.0) {
        return finish(vec![[link_end_mm, link_radius_mm], [length_mm, joint]]);
    }
    // `line - law` is convex, negative at `s_far` and not falling: one root.
    let (mut low, mut high) = (link_end_mm, length_mm);
    for _ in 0..BISECTIONS {
        let mid = 0.5 * (low + high);
        if under(mid) >= 0.0 {
            high = mid;
        } else {
            low = mid;
        }
    }
    let meet = high;
    let mut knots = vec![[link_end_mm, link_radius_mm]];
    let rest = head_polyline_from_law_mm(tip_radius_mm, meet, length_mm, law);
    if meet - link_end_mm > 1.0e-9 {
        knots.extend(rest);
    } else {
        knots.extend(rest.into_iter().skip(1));
    }
    finish(knots)
}

/// **How far a section of radius `radius_mm` at axial distance `s_mm` is over
/// its stress limit** under `law`, as a ratio of stress to allowed stress. The
/// law is `r³ = P·r + Q·s`, the section where the ratio is exactly 1, so the
/// ratio of a section of radius `r` there is `P/r² + Q·s/r³` with the law's
/// `P = (r_t²/c)|cos phi'|` and `Q = 4(r_t²/c) sin phi'`. Above 1 is a section
/// under the law.
#[must_use]
pub fn section_stress_ratio(tip_radius_mm: f64, radius_mm: f64, s_mm: f64, law: HeadLaw) -> f64 {
    let a = tip_radius_mm * tip_radius_mm / law.cos_lean;
    let (p, q) = (a * law.cos_phi_abs, 4.0 * a * law.sin_phi);
    p / (radius_mm * radius_mm) + q * s_mm / (radius_mm * radius_mm * radius_mm)
}

/// The warning for a head whose link lies under the bending law (emit and
/// warn, never refuse).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkOverstress {
    /// The member's id.
    pub member: u32,
    /// The link's radius, mm.
    pub link_radius_mm: f64,
    /// The contact ball's radius `R`, mm.
    pub ball_radius_mm: f64,
    /// Stress over allowed stress at the link's far end ([`section_stress_ratio`]).
    pub stress_ratio: f64,
    /// The head is sized by the joint certificate: the joint and the cone are
    /// sized for what this link can pass, so the link is the weakest fuse by
    /// construction and the bending law above it is only the old reference.
    pub certified: bool,
}

impl std::fmt::Display for LinkOverstress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "member {}: link radius {:.3} mm (ball {:.3} mm) carries {:.1} times the allowed stress{}",
            self.member,
            self.link_radius_mm,
            self.ball_radius_mm,
            self.stress_ratio,
            if self.certified {
                " (joint sized for what the link can pass)"
            } else {
                ""
            }
        )
    }
}

/// **One warning per drawn head whose link is over its stress limit**, on the
/// finished field (after the last sizing, so the law and the tip are the
/// drawn ones). The link is a cylinder of the tip's radius whose far end
/// stands `2R + link_mm` from the contact centre. In strut order. One pass.
#[must_use]
pub fn link_overstress(
    struts: &[crate::SweepStrut],
    link_mm: f64,
    ball_factor_at: impl Fn(f64) -> f64,
) -> Vec<LinkOverstress> {
    struts
        .iter()
        .filter_map(|strut| {
            let head = strut.head.as_ref().filter(|head| !head.drawn_straight)?;
            if !crate::slenderness::live(strut) {
                return None;
            }
            let tip = strut.tip_radius_mm();
            let ball = ball_factor_at(tip) * tip;
            // A certified head is read at the link end the file DRAWS (the first
            // knot of its cone); the old law at the planned end.
            let link_end = head.certified.as_ref().map_or_else(
                || ball_allowance_of(ball) + link_mm,
                |cert| cert.profile.knots.first().map_or(0.0, |knot| knot[0]),
            );
            let ratio = section_stress_ratio(tip, tip, link_end, head.law());
            (ratio > 1.0 + 1.0e-9).then_some(LinkOverstress {
                member: strut.id,
                link_radius_mm: tip,
                ball_radius_mm: ball,
                stress_ratio: ratio,
                certified: head.certified.is_some(),
            })
        })
        .collect()
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
    post_section_law_mm(
        ledger_radius_mm,
        tip_radius_mm,
        lever_mm,
        HeadLaw::envelope(cos_lean_max),
    )
}

/// [`post_section_mm`] under the exact law `law`:
/// `r³ = r_led²·r + (4r_t²/c)·sin phi'·lever`.
#[must_use]
pub fn post_section_law_mm(
    ledger_radius_mm: f64,
    tip_radius_mm: f64,
    lever_mm: f64,
    law: HeadLaw,
) -> f64 {
    section_radius_mm(
        ledger_radius_mm,
        tip_radius_mm / law.cos_lean.sqrt(),
        lever_mm * law.sin_phi,
    )
}

/// Why a seat kept today's straight tip instead of a head.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HeadRefusal {
    /// The face is not steep: flat faces and Round discs keep the straight tip.
    NotSteep,
    /// The seat's face does not look down inside the seat-face band (it looks
    /// up, or is steeper than the band admits): there is no seat plane to
    /// stand a head on. Only the graze conversion
    /// ([`plan_graze_head`]) can meet this; placement stops at
    /// [`HeadRefusal::NotSteep`] first.
    NotDownFacing,
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

/// Why placement did NOT ask for a head at a seat: the exits of the plan that
/// are not refusals of a head that was asked for (Craig 2026-10-08: every
/// reason a seat gets no angled head is logged). Logging only; no outcome
/// reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HeadSkip {
    /// The run has no head envelope (heads off), so no seat is asked.
    HeadsOff,
    /// The nearest model face to the seat's axis point does not look down
    /// (its normal's `z` is zero or positive: a ceiling-side face or a
    /// vertical wall), and no down-facing face in the seat-face band was
    /// found to read instead ([`nearest_band_face`]).
    FacetNotDownFacing,
    /// The nearest face looks down but is steeper than the seat-face band
    /// admits (its normal's `z` is at or above the rule's `max_normal_z`),
    /// and no band face was found to read instead.
    FacetSteeperThanBand,
    /// The nearest face is more than the prune margin below the skim angle:
    /// too flat to be worth the footprint query (the footprint normal is not
    /// measured for this exit).
    FacetTooFlat,
    /// The footprint normal was measured and the trigger does not hold on it
    /// ([`HeadRefusal::NotSteep`] from the plan).
    FootprintNotSteep,
}

/// The ids of the model's faces inside the seat-face band: outward normal `z`
/// below `max_normal_z`. Built once per run; [`nearest_band_face`] reads it.
#[must_use]
pub fn band_face_ids(mesh: &Mesh, max_normal_z: f64) -> Vec<u32> {
    let verts = mesh.verts();
    mesh.tris()
        .iter()
        .enumerate()
        .filter_map(|(id, t)| {
            let a = *verts.get(t[0] as usize)?;
            let b = *verts.get(t[1] as usize)?;
            let c = *verts.get(t[2] as usize)?;
            let unit = vec3::normalize(vec3::cross(vec3::sub(b, a), vec3::sub(c, a)))?;
            (unit[2] < max_normal_z).then_some(u32::try_from(id).ok()?)
        })
        .collect()
}

/// **The nearest face inside the seat-face band to `point`**: its closest
/// point, id and unit normal. A seat whose axis point lies on the edge between
/// a down-facing face and a vertical wall has the WALL as its nearest face of
/// any slope, and that reads as no seat at all; the face it actually supports
/// is the nearest one the seat-face rule admits (the rule
/// [`crate::tip_landing::tip_seat_faces`] applies to every finished tip).
/// `None` when `band` is empty or the query names no face.
#[must_use]
pub fn nearest_band_face(
    surface: &NativeSurface,
    mesh: &Mesh,
    band: &[u32],
    point: [f64; 3],
) -> Option<([f64; 3], u32, [f64; 3])> {
    use geom_kernel::{
        AllowedFaceProjectionLimits, AllowedFaceProjectionOutcome, AllowedFaceProjectionPolicy,
    };
    if band.is_empty() {
        return None;
    }
    let limits = AllowedFaceProjectionLimits {
        max_points: usize::MAX,
        max_allowed_faces: usize::MAX,
        max_total_node_visits: usize::MAX,
        max_total_triangle_tests: usize::MAX,
        max_memory_bytes: usize::MAX,
    };
    let outcomes = surface
        .project_allowed_faces_batch(
            band,
            &[point],
            AllowedFaceProjectionPolicy { max_distance_mm: None },
            limits,
        )
        .ok()?;
    let AllowedFaceProjectionOutcome::Projected(found) = outcomes.into_iter().next()? else {
        return None;
    };
    let verts = mesh.verts();
    let t = mesh.tris().get(found.face_id as usize)?;
    let (a, b, c) = (*verts.get(t[0] as usize)?, *verts.get(t[1] as usize)?, *verts.get(t[2] as usize)?);
    let normal = vec3::normalize(vec3::cross(vec3::sub(b, a), vec3::sub(c, a)))?;
    Some((found.point, found.face_id, normal))
}

/// One seat that was not asked for a head, with what the decision read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkippedSeat {
    /// The member id the arm got.
    pub id: u32,
    /// Why.
    pub reason: HeadSkip,
    /// The seat's axis point (lowering frame, plate at `plate_z_mm`), mm.
    pub seat_mm: [f64; 3],
    /// Where the plate stands in that frame, mm.
    pub plate_z_mm: f64,
    /// The unit normal of the nearest model face, when one was found.
    pub facet: Option<[f64; 3]>,
    /// The footprint normal, when it was measured.
    pub footprint: Option<[f64; 3]>,
}

/// What a run did about heads: the path-hit witness for every one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeadsOutcome {
    /// Heads built.
    pub built: usize,
    /// Seats that kept the straight tip, per reason, with the member ids.
    pub refusals: BTreeMap<HeadRefusal, Vec<u32>>,
    /// Seats that were never asked for a head, per reason, with the member
    /// ids (logging only).
    pub skips: BTreeMap<HeadSkip, Vec<u32>>,
    /// The same seats, with the normals each decision read, in member order.
    pub skipped_seats: Vec<SkippedSeat>,
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
    /// Heads KEPT although the final sizing left the joint's stand-off under
    /// `g` or the bare neck under the allowance (emit and warn, never
    /// refuse): one warning per member, in id order. See
    /// [`downgrade_grown_heads`].
    pub kept_short_of_clearance: Vec<HeadClearanceWarning>,
    /// Heads whose sections changed when they were sized AGAIN after the
    /// footprint rule's seat moves, because a host's move tilted the post the
    /// file draws (v19 fix, 2026-10-07): the path-hit witness for the final
    /// sizing. See [`crate::size_heads_for_final_posts`].
    pub resized_after_moves: usize,
    /// Straight tips converted to a head because their shaft grazed the model
    /// (Craig 2026-10-08: a head only where needed): the member ids, in id
    /// order. They are also counted in [`Self::built`]. See [`crate::graze`].
    pub graze_converted: Vec<u32>,
    /// Straight tips whose shaft grazed the model when the conversion ran
    /// (the path-hit witness for the census at that point): zero means the
    /// conversion never saw one.
    pub graze_asked: usize,
    /// **The conversion again, at the final radius** (`MODEL-FEET.md` F3,
    /// 2026-10-12): straight tips whose shaft grazes the model on the field
    /// the final sizing, the ties and the clearance re-route left (a tip
    /// grown past the radius it was first judged at, or an arm moved after
    /// the first conversion), and of those, the ones given a head then. The
    /// converted ids are in [`Self::graze_converted`] too.
    pub graze_asked_late: usize,
    pub graze_converted_late: Vec<u32>,
    /// Why each grazer the conversion looked at and left straight stayed so,
    /// `(member id, reason)`, in id order.
    pub graze_reasons: Vec<(u32, GrazeReason)>,
    /// Straight tips whose shaft still grazes the model on the finished
    /// field, kept straight: the warning for each, in id order. Measured on
    /// the emitted arms by `member_clearance::graze_census`; emit and warn,
    /// never refuse.
    pub graze_kept: Vec<GrazeKept>,
    /// Drawn heads whose link (a cylinder of the tip's radius) lies under the
    /// bending law, one warning each, in strut order (Craig 2026-10-08: the
    /// link is the contact's own size, so the law is not met there and only a
    /// coupon can size the margin). See [`link_overstress`].
    pub link_overstress: Vec<LinkOverstress>,
    /// The joint-sizing certificate on the finished field
    /// ([`crate::joint_size`]): how many heads are sized for what their contact
    /// can pass, how many stayed on the old law and why, and the worst certified
    /// demand over the drawn head and post chords.
    pub joint_certificate: JointCertificate,
}

/// See [`HeadsOutcome::joint_certificate`]. Counts of drawn heads (not
/// downgraded), read off the finished strut set.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JointCertificate {
    pub certified: usize,
    pub old_law_shared: usize,
    pub old_law_no_seat: usize,
    pub old_law_infeasible: usize,
    /// Drawn heads the final sizing never reached (no sizing ran).
    pub not_sized: usize,
    pub slots_certified: usize,
    pub slots_kept: usize,
    pub slots_pinned: usize,
    /// The largest certified demand over any drawn chord (at most 1 + 1e-9).
    pub worst_drawn_max: f64,
    /// Heads whose drawn chords exceed 1 + 1e-9: warned, never refused.
    pub over_one: usize,
    /// Certified heads with at least one post slot drawn under the
    /// certificate's own need for it, and how many such slots in all
    /// ([`CertifiedHead::slots_under_need`]): warned, never refused.
    pub heads_under_need: usize,
    pub slots_under_need: usize,
    /// The worst of them: `(member id, need, drawn)`, radii in mm, the
    /// largest `need / drawn`.
    pub worst_under_need: Option<(u32, f64, f64)>,
}

impl JointCertificate {
    /// Read the counts off the finished strut set. One pass.
    #[must_use]
    pub fn of(struts: &[crate::SweepStrut]) -> Self {
        let mut out = Self::default();
        for (id, head) in struts
            .iter()
            .filter(|strut| crate::slenderness::live(strut))
            .filter_map(|strut| {
                strut.head.as_ref().filter(|head| !head.drawn_straight).map(|head| (strut.id, head))
            })
        {
            match (&head.certified, head.old_law) {
                (Some(c), _) => {
                    out.certified += 1;
                    out.slots_certified += c.slots_certified;
                    out.slots_kept += c.slots_kept;
                    out.slots_pinned += c.slots_pinned;
                    out.worst_drawn_max = out.worst_drawn_max.max(c.drawn_max);
                    if c.drawn_max > 1.0 + 1.0e-9 {
                        out.over_one += 1;
                    }
                    if c.slots_under_need > 0 {
                        out.heads_under_need += 1;
                        out.slots_under_need += c.slots_under_need;
                    }
                    if let Some((need, drawn)) = c.worst_under_need {
                        let worse = out
                            .worst_under_need
                            .is_none_or(|(_, n, d)| need * d > n * drawn);
                        if worse {
                            out.worst_under_need = Some((id, need, drawn));
                        }
                    }
                }
                (None, Some(OldLawReason::Shared)) => out.old_law_shared += 1,
                (None, Some(OldLawReason::NoSeat)) => out.old_law_no_seat += 1,
                (None, Some(OldLawReason::Infeasible)) => out.old_law_infeasible += 1,
                (None, None) => out.not_sized += 1,
            }
        }
        out
    }
}

/// Why a grazing straight tip was left straight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrazeReason {
    /// Another member rests on the arm: moving its post would move theirs.
    CarriesMembers,
    /// Planning or checking the head refused it (the refusal is counted too).
    Refused(HeadRefusal),
    /// No post from the joint lands legally (lean law or path) on the arm's
    /// own host or the plate.
    NoLegalPost,
    /// The post lands only on a host thinner than the joint, so it would
    /// RAISE the host (a lander does not make its host fat), and the plate
    /// under the joint is not free either. The joint is
    /// fat because the face is steep and the post is sized for the full lever
    /// `|DK|` (the sizing ruling left to Craig: a post that falls back to its
    /// ledger section would fit hosts it cannot fit today).
    HostTooThin,
    /// A member below the arm (its host or the host's own) stands within the
    /// final gap of the model: the clearance re-route would have to move it
    /// with the head on it, and it never moves a headed arm.
    HostNearModel,
    /// The straight tip's contact disc hangs off its seat face, so the
    /// footprint rule will slide the tip along the face to put the whole disc
    /// on it ([`crate::footprint`]). A head is frozen and never slides: the
    /// head would keep the disc where the seat cell is, partly off the face,
    /// and the contact would lose the reach the slide gives it (quality lane
    /// Q2, Deinonychus: a 0.18 mm slide put the contact under a 24-pixel
    /// island, the head left it 0.018 mm short).
    SlidesToItsFace,
    /// A brace, a mast or a tie: warned, never converted.
    NotAnArm,
    /// The arm was made or moved after the conversion ran.
    LateSlide,
}

/// One grazing straight tip kept straight: the warning of
/// [`HeadsOutcome::graze_kept`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrazeKept {
    /// The strut's member id.
    pub id: u32,
    /// Degrees between the seat normal and the drawn shaft axis.
    pub angle_deg: f64,
    /// The measured shaft clearance, mm (`f64::INFINITY` when unmeasured).
    pub clearance_mm: f64,
    /// Why it stayed straight.
    pub reason: GrazeReason,
}

/// The warning for a head kept although its drawn clearance is short.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadClearanceWarning {
    /// The member's id.
    pub member: u32,
    /// The joint's stand-off from the seat plane, `(K - D)·u - r_K`, mm.
    pub standoff_mm: f64,
    /// The contact ball's height over the plane at the final tip, `2R`, mm.
    pub ball_height_mm: f64,
    /// The bare neck, `stand-off - ball height`, mm.
    pub neck_mm: f64,
}

impl std::fmt::Display for HeadClearanceWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "member {}: stand-off {:.3} mm, ball height {:.3} mm, neck {:.3} mm",
            self.member, self.standoff_mm, self.ball_height_mm, self.neck_mm
        )
    }
}

/// What [`downgrade_grown_heads`] did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeadDowngrades {
    /// Heads that could not be drawn (no joint slot): given up to the
    /// straight tip, in strut order.
    pub downgraded: Vec<u32>,
    /// Heads kept with a short stand-off or neck, one warning each.
    pub kept_with_warning: Vec<HeadClearanceWarning>,
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
    /// `L_link`: the registered length of the bare link between the contact
    /// ball and the cone ([`Head::link_mm`]). `0` plans a head with no link.
    pub link_mm: f64,
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
    /// The contact ball's radius `R` at that tip radius (the registered
    /// profile's factor times the tip radius): the joint stands `2R` further
    /// out than the clearance alone ([`Head::ball_allowance_mm`]).
    pub ball_radius_mm: f64,
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
/// 3. `D = A − ((A − P)·u)·u`, `K = D + (g + hb + r_K)·u` with `hb = 2R` the
///    ball's height over the plane (exact: never snapped to a cell centre);
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
    plan_head_on_axis(req, rule, grid, unit)
}

/// **Plan one head at a seat whose straight shaft grazes the model**, with no
/// steepness trigger (Craig 2026-10-08: a head only where a straight tip's
/// shaft would graze; the caller measured that). Steps 2 to 6 of
/// [`plan_head`]; the only gate is the seat plane itself.
///
/// # Errors
///
/// [`HeadRefusal::NoFootprint`] for a degenerate normal,
/// [`HeadRefusal::NotDownFacing`] when the axis does not look down inside the
/// seat-face band, then the refusals of [`plan_head`] from step 2.
pub fn plan_graze_head(
    req: &HeadRequest,
    rule: &HeadRule,
    grid: &Grid,
) -> Result<Head, HeadRefusal> {
    let Some(unit) = vec3::normalize(req.unit) else {
        return Err(HeadRefusal::NoFootprint);
    };
    if !(unit[2] < 0.0 && unit[2] < rule.max_normal_z) {
        return Err(HeadRefusal::NotDownFacing);
    }
    plan_head_on_axis(req, rule, grid, unit)
}

/// Steps 2 to 6 of [`plan_head`] for an admitted unit axis.
fn plan_head_on_axis(
    req: &HeadRequest,
    rule: &HeadRule,
    grid: &Grid,
    unit: [f64; 3],
) -> Result<Head, HeadRefusal> {
    // The joint stands `g` beyond the ball's far side, not beyond the plane:
    // the ball stands up to `2R` over the plane, and the lever is `|DK|`.
    let ball_allowance_mm = ball_allowance_of(req.ball_radius_mm);
    // The joint carries EVERY post the lean law admits under THIS head axis
    // (the numerical maximum of the exact law, not the 90 degree envelope).
    let Some((joint, law_planned)) = solve_joint_for_axis(
        req.tip_radius_lb_mm,
        rule.gap_mm + ball_allowance_mm + rule.link_mm,
        unit,
        rule.cos_lean_max,
        rule.max_radius_mm,
    ) else {
        return Err(HeadRefusal::JointOverMax);
    };
    let (a, p) = (req.axis_point_mm, req.face_point_mm);
    let contact = vec3::sub(a, vec3::scale(unit, vec3::dot(vec3::sub(a, p), unit)));
    let k = vec3::add(contact, vec3::scale(unit, joint.length_mm));
    let layers_over_plate = ((k[2] - rule.plate_z_mm) / rule.layer_height_mm).floor();
    if layers_over_plate.is_nan() || layers_over_plate < 1.0 {
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
        ball_allowance_mm,
        link_mm: rule.link_mm,
        drawn_straight: false,
        law_planned,
        law_drawn: None,
        ledger_radius_mm: Vec::new(),
        certified: None,
        old_law: None,
    })
}

/// The height a contact ball of radius `ball_radius_mm` stands over its seat
/// plane at most: its diameter (a ball seated into the model stands lower).
/// Zero for a radius that is not finite and positive.
#[must_use]
pub fn ball_allowance_of(ball_radius_mm: f64) -> f64 {
    if ball_radius_mm.is_finite() && ball_radius_mm > 0.0 {
        2.0 * ball_radius_mm
    } else {
        0.0
    }
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
pub fn planned_tip_radius_mm(
    demand_lb_mm: f64,
    rule: &HeadRule,
    ball_factor_at: impl Fn(f64) -> f64,
) -> f64 {
    let estimate = demand_lb_mm.max(rule.typical_tip_radius_mm);
    if estimate > demand_lb_mm {
        let ball_mm = ball_allowance_of(ball_factor_at(estimate) * estimate);
        if solve_joint(
            estimate,
            rule.gap_mm + ball_mm + rule.link_mm,
            rule.cos_lean_max,
            rule.max_radius_mm,
        )
        .is_none()
        {
            return demand_lb_mm;
        }
    }
    estimate
}

/// **Give up every head that cannot be drawn, and warn on every head whose
/// drawn clearance is short** (orchestrator ruling 2026-10-07, after v19: the
/// downgrade left joint-sized posts under straight tips, a 0.965 mm column on
/// the Cylinder, and the drawn file was worse than either form).
///
/// A head is frozen at placement and only its radii change. It is given up
/// ONLY when the ledger has no joint slot to draw it from. Such a member goes
/// back to today's straight tip: the arm is DRAWN straight from its seat to
/// its base ([`Head::drawn_straight`]), and EVERY slot of the strut is reset
/// to the area-law ledger saved before the head minimum
/// ([`Head::ledger_radius_mm`]), so no joint-sized slot stays under the
/// straight tip. The strut keeps the head, its landing, its base AND its path
/// (it is NOT re-routed): the members that landed on the post were placed
/// against the cells it stood in, and the drawing moves them by the
/// displacement between the old axis and the straight line
/// ([`crate::adapter`]).
///
/// When, at the FINAL tip radius, the joint's stand-off from the seat plane,
/// `(K - D)·u - r_K`, is below the gap `g`, or the bare neck (the stand-off
/// less the ball's height over the plane, `2R`, an upper bound) is under
/// `neck_min_mm`, the head is KEPT, `drawn_straight` stays false, and the
/// member is named in [`HeadDowngrades::kept_with_warning`] with its
/// stand-off, ball height and neck. Emit and warn, never refuse: a straight
/// tip on a steep face is the defect the head exists to cure.
///
/// `ball_factor_at` is the registered contact-sphere factor at a tip radius.
/// One pass over `struts`, after the last sizing and before anything reads the
/// finished field.
pub fn downgrade_grown_heads(
    struts: &mut [crate::SweepStrut],
    neck_min_mm: f64,
    ball_factor_at: impl Fn(f64) -> f64,
) -> HeadDowngrades {
    let mut outcome = HeadDowngrades::default();
    for strut in struts.iter_mut() {
        let Some(head) = strut.head.as_ref() else {
            continue;
        };
        if head.drawn_straight || !crate::slenderness::live(strut) {
            continue;
        }
        let along = vec3::dot(vec3::sub(head.joint_mm, head.contact_mm), head.unit);
        let joint_slot = head.k_joint.checked_sub(strut.k_bottom);
        let joint_radius = joint_slot.and_then(|slot| strut.radius_mm.get(slot).copied());
        let Some(joint_radius) = joint_radius else {
            // The head cannot be drawn: today's straight tip, from the ledger.
            outcome.downgraded.push(strut.id);
            let slots = strut.radius_mm.len();
            let Some(head) = strut.head.as_mut() else {
                continue;
            };
            head.drawn_straight = true;
            if head.ledger_radius_mm.len() == slots {
                strut.radius_mm.clone_from(&head.ledger_radius_mm);
            }
            continue;
        };
        let tip = strut.tip_radius_mm();
        let ball_height = ball_allowance_of(ball_factor_at(tip) * tip);
        let standoff = along - joint_radius;
        if standoff < head.gap_mm - 1e-9 || standoff - ball_height < neck_min_mm - 1e-9 {
            outcome.kept_with_warning.push(HeadClearanceWarning {
                member: strut.id,
                standoff_mm: standoff,
                ball_height_mm: ball_height,
                neck_mm: standoff - ball_height,
            });
        }
    }
    outcome
}

/// **The link is refused at load** where it is shorter than the rasterisation
/// allowance (one layer plus one pixel, `allowance_mm`): such a link would not
/// print as a link. `0` is the switch that draws heads with no link, and is
/// accepted; a negative or non-finite length is refused.
///
/// # Errors
///
/// The reason, as text, for a link under the allowance.
pub fn check_link_mm(link_mm: f64, allowance_mm: f64) -> Result<(), String> {
    if link_mm == 0.0 || (link_mm.is_finite() && link_mm >= allowance_mm) {
        Ok(())
    } else {
        Err(format!(
            "HEAD_LINK_MM {link_mm} mm is shorter than one layer plus one pixel \
             ({allowance_mm} mm): a link that short does not print as a link"
        ))
    }
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
            min_normal_z: f64::NEG_INFINITY,
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
    /// The hidden-cavity map (the run's own, `history.hidden`, when `history`
    /// is the running column history; a shaft index built after the sweep does
    /// not carry it).
    pub hidden: &'a crate::cavity::HiddenCavity,
    /// Members whose stamped shafts are not obstacles (the arm a head is
    /// being added to: its own old shaft stands in the index).
    pub except: &'a [u32],
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
    if ctx.hidden.segment_hidden(raster(d), raster(k), r_k) {
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
        ctx.history.shaft_blocks_excluding(cell, layer, ctx.except)
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
        check_link_mm, final_joint_radius_law_mm, final_joint_radius_mm, head_polyline_law_mm,
        head_polyline_mm, head_profile_with_link, head_section_law_mm, head_section_mm,
        post_section_law_mm, post_section_mm, section_radius_mm, section_stress_ratio, solve_bend,
        solve_joint, solve_joint_for_axis, Head, HeadLaw,
    };
    use crate::{Landing, SeatFace, SweepStrut};

    const GAP_MM: f64 = 0.3;

    fn cos_lean() -> f64 {
        57.5_f64.to_radians().cos()
    }

    /// THE LINK (Craig 2026-10-08): the cone leaves `(s_far, r_t)`, the link's
    /// own radius, where `s_far = 2R + L`; it ends exactly on the law at the
    /// joint; its knots rise strictly; it lies under the law only in a prefix
    /// (from the link to where it meets the law) and on or above it from then
    /// on; and the link is under the law (stress above 1) at its far end while
    /// the law's own section carries exactly 1.
    #[test]
    fn the_cone_leaves_the_link_radius_meets_the_law_and_stays_on_it() {
        let c = cos_lean();
        let link = 0.3;
        for tip in [0.05, 0.115, 0.15] {
            let ball = 1.4 * tip;
            let s_far = 2.0 * ball + link;
            for law in [
                HeadLaw::envelope(c),
                HeadLaw { cos_lean: 0.8, cos_phi_abs: 0.4, sin_phi: 0.9165 },
            ] {
                // A short head (joint just past the link) and a long one.
                for length in [s_far + 0.55, s_far + 3.0] {
                    let profile = head_profile_with_link(tip, ball, link, length, law);
                    assert!((profile.link_radius_mm - tip).abs() < 1e-15, "the link is the tip's own");
                    assert!((profile.link_end_mm - s_far).abs() < 1e-12);
                    let knots = &profile.knots;
                    assert!(knots.len() >= 2 && knots.len() <= 6, "at most five frusta: {law:?}: {knots:?}");
                    let bits = |point: [f64; 2]| point.map(f64::to_bits);
                    assert_eq!(bits(knots[0]), bits([s_far, tip]), "the cone starts at the link radius");
                    assert_eq!(
                        bits(knots[knots.len() - 1]),
                        bits([length, head_section_law_mm(tip, length, law)]),
                        "the cone ends on the law at the joint"
                    );
                    assert!(knots.windows(2).all(|w| w[0][0] < w[1][0]), "{knots:?}");
                    let drawn = |s: f64| {
                        knots
                            .windows(2)
                            .find(|w| s <= w[1][0])
                            .map(|w| w[0][1] + (w[1][1] - w[0][1]) * (s - w[0][0]) / (w[1][0] - w[0][0]))
                            .expect("s lies in the cone")
                    };
                    let mut on_the_law = false;
                    for step in 0..=1000 {
                        let s = s_far + (length - s_far) * f64::from(step) / 1000.0;
                        let under = drawn(s) < head_section_law_mm(tip, s, law) - 1.0e-9;
                        assert!(!(on_the_law && under), "{law:?} tip {tip} s {s}: back under the law");
                        on_the_law |= !under;
                    }
                    // The link is under the law at its far end: the law there is
                    // fatter than the tip, and the ratio is the cube-law's.
                    let at_end = head_section_law_mm(tip, s_far, law);
                    assert!(at_end > tip);
                    assert!(section_stress_ratio(tip, tip, s_far, law) > 1.0);
                    assert!((section_stress_ratio(tip, at_end, s_far, law) - 1.0).abs() < 1e-9);
                }
                // A long head meets the law before the joint: more than one
                // frustum, and the tangent polyline carries it on.
                let long = head_profile_with_link(tip, ball, link, s_far + 8.0, law);
                assert!(long.knots.len() >= 3, "{law:?}: {:?}", long.knots);
            }
        }
        // A link on the (floored) law needs no cone of its own.
        let aligned = HeadLaw { cos_lean: 1.0, cos_phi_abs: 1.0, sin_phi: 0.0 };
        let flat = head_profile_with_link(0.1, 0.14, 0.3, 1.0, aligned);
        assert!(flat.knots.iter().all(|k| (k[1] - 0.1).abs() < 1e-12), "{:?}", flat.knots);
    }

    #[test]
    fn a_link_shorter_than_a_layer_plus_a_pixel_is_refused_at_load() {
        let allowance = 0.05 + 0.019;
        assert!(check_link_mm(0.3, allowance).is_ok());
        assert!(check_link_mm(allowance, allowance).is_ok());
        assert!(check_link_mm(0.0, allowance).is_ok(), "zero switches the link off");
        assert!(check_link_mm(0.05, allowance).is_err());
        assert!(check_link_mm(-0.3, allowance).is_err());
        assert!(check_link_mm(f64::NAN, allowance).is_err());
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

    /// The drawn joint at the FINAL tip over a frozen `|DK|` (plan section A,
    /// v18 Cylinder `|DK|` = 1.344 mm): the sizes of the plan's table, and the
    /// planned joint back when the final tip is the planned one.
    #[test]
    fn the_final_joint_follows_the_final_tip_over_the_frozen_length() {
        let c = cos_lean();
        for (tip, want) in [(0.05, 0.298), (0.06, 0.337), (0.115, 0.526), (0.15, 0.631)] {
            let r = final_joint_radius_mm(tip, 1.344, c);
            assert!((r - want).abs() < 1e-3, "tip {tip}: {r} against {want}");
            let residual = r * r * r - tip * tip / c * (r + 4.0 * 1.344);
            assert!(residual.abs() < 1e-12, "tip {tip}: residual {residual}");
        }
        // The planned joint's own |DK| gives the planned joint back.
        for tip in [0.05, 0.15] {
            let planned = solve_joint(tip, GAP_MM, c, 1.0).expect("solvable");
            let again = final_joint_radius_mm(tip, planned.length_mm, c);
            assert!((again - planned.joint_radius_mm).abs() < 1e-9, "tip {tip}");
        }
        // Monotone in the tip and in the length.
        assert!(final_joint_radius_mm(0.05, 1.0, c) < final_joint_radius_mm(0.06, 1.0, c));
        assert!(final_joint_radius_mm(0.05, 1.0, c) < final_joint_radius_mm(0.05, 1.5, c));
    }

    /// The drawn head (plan section B, L 1.344): the tangent polyline never
    /// goes under the law, its ends are the law's own, and it is the plan's
    /// five points (so four frusta) within 0.5 um.
    #[test]
    fn the_head_polyline_never_goes_under_the_law_and_its_ends_are_exact() {
        let c = cos_lean();
        let length = 1.344;
        for tip in [0.05, 0.06, 0.115, 0.15] {
            let knots = head_polyline_mm(tip, length, c);
            assert_eq!(knots.len(), 5, "tip {tip}: {knots:?}");
            // Exact to the bit: the ends are the law's own values.
            let bits = |point: [f64; 2]| point.map(f64::to_bits);
            assert_eq!(bits(knots[0]), bits([0.0, head_section_mm(tip, 0.0, c)]), "{knots:?}");
            assert_eq!(bits(knots[4]), bits([length, head_section_mm(tip, length, c)]), "{knots:?}");
            assert!(knots.windows(2).all(|w| w[0][0] < w[1][0] && w[0][1] < w[1][1]));
            for step in 0..=1000 {
                let s = length * f64::from(step) / 1000.0;
                let at = knots
                    .windows(2)
                    .find(|w| s <= w[1][0])
                    .map(|w| w[0][1] + (w[1][1] - w[0][1]) * (s - w[0][0]) / (w[1][0] - w[0][0]))
                    .expect("s lies in the polyline");
                let law = head_section_mm(tip, s, c);
                assert!(at >= law - 1.0e-12, "tip {tip}, s {s}: {at} under the law {law}");
            }
        }
        let thin = head_polyline_mm(0.05, length, c);
        for (got, want) in thin.iter().zip([
            [0.0, 0.068],
            [0.012, 0.093],
            [0.139, 0.162],
            [0.720, 0.253],
            [1.344, 0.298],
        ]) {
            assert!(
                (got[0] - want[0]).abs() < 5.0e-4 && (got[1] - want[1]).abs() < 5.0e-4,
                "{got:?} against the plan's {want:?}"
            );
        }
        // No length: only the contact point.
        assert_eq!(head_polyline_mm(0.05, 0.0, c).len(), 1);
        // The exact law (HeadLaw), including the floored cases: never under
        // the law, the ends exact to the bit, the knots strictly rising in s,
        // at most four frusta, and the envelope law is the old polyline.
        for tip in [0.05, 0.115, 0.15] {
            assert_eq!(
                head_polyline_law_mm(tip, length, HeadLaw::envelope(c)),
                head_polyline_mm(tip, length, c),
                "the envelope is the old polyline, bit for bit"
            );
            for (cos_lean, cos_phi_abs, sin_phi) in [
                (c, 1.0, 1.0),
                (1.0, 0.447, 0.894),
                (0.486, 0.217, 0.976),
                (c, 0.3, 0.95),
                (1.0, 0.0, 1.0),
                (1.0, 1.0, 0.0),
                (0.8, 0.2, 0.0),
            ] {
                let law = HeadLaw { cos_lean, cos_phi_abs, sin_phi };
                let knots = head_polyline_law_mm(tip, length, law);
                let bits = |point: [f64; 2]| point.map(f64::to_bits);
                assert!(knots.len() >= 2 && knots.len() <= 5, "{law:?}: {knots:?}");
                assert_eq!(bits(knots[0]), bits([0.0, head_section_law_mm(tip, 0.0, law)]), "{law:?}");
                assert_eq!(
                    bits(knots[knots.len() - 1]),
                    bits([length, head_section_law_mm(tip, length, law)]),
                    "{law:?}"
                );
                assert!(knots.windows(2).all(|w| w[0][0] < w[1][0]), "{law:?}: {knots:?}");
                for step in 0..=1000 {
                    let s = length * f64::from(step) / 1000.0;
                    let at = knots
                        .windows(2)
                        .find(|w| s <= w[1][0])
                        .map(|w| w[0][1] + (w[1][1] - w[0][1]) * (s - w[0][0]) / (w[1][0] - w[0][0]))
                        .expect("s lies in the polyline");
                    let under = head_section_law_mm(tip, s, law);
                    assert!(at >= under - 1.0e-9, "{law:?} tip {tip}, s {s}: {at} under {under}");
                    assert!(under >= tip - 1e-15, "the section is never thinner than the contact");
                }
            }
        }
    }

    /// The exact law is the envelope at `|cos phi'| = sin phi' = 1`; a head
    /// whose post runs along its axis and stands straight (`phi' = 0`,
    /// `c = 1`) carries no transverse load and is the contact's own radius at
    /// every `s`; the post law takes the same angle.
    #[test]
    fn the_exact_law_is_the_envelope_at_its_corner_and_the_tip_when_aligned() {
        let c = cos_lean();
        for tip in [0.05, 0.115] {
            for s in [0.0, 0.3, 1.344] {
                assert_eq!(
                    head_section_law_mm(tip, s, HeadLaw::envelope(c)).to_bits(),
                    head_section_mm(tip, s, c).to_bits()
                );
                let aligned = HeadLaw { cos_lean: 1.0, cos_phi_abs: 1.0, sin_phi: 0.0 };
                assert!((head_section_law_mm(tip, s, aligned) - tip).abs() < 1e-12, "tip {tip}, s {s}");
            }
            assert_eq!(
                post_section_law_mm(0.1, tip, 0.8, HeadLaw::envelope(c)).to_bits(),
                post_section_mm(0.1, tip, 0.8, c).to_bits()
            );
            // The post law reads the same angle: no transverse load, no growth.
            let aligned = HeadLaw { cos_lean: 1.0, cos_phi_abs: 1.0, sin_phi: 0.0 };
            assert!((post_section_law_mm(0.1, tip, 0.8, aligned) - 0.1).abs() < 1e-12);
        }
        // `of_post`: a vertical post under a 45 degree head is phi' = 45.
        let law = HeadLaw::of_post([0.6, 0.0, -0.8], [0.0, 0.0, -1.0]).expect("descends");
        assert!((law.cos_lean - 1.0).abs() < 1e-12);
        assert!((law.cos_phi_abs - 0.8).abs() < 1e-12 && (law.sin_phi - 0.6).abs() < 1e-12);
        // A post that does not descend has no law.
        assert!(HeadLaw::of_post([0.6, 0.0, -0.8], [1.0, 0.0, 0.0]).is_none());
    }

    /// The per-axis placement envelope is the numerical maximum of the exact
    /// law over every admissible post: for 1000 sampled directions (lean up to
    /// 57.5 degrees, any azimuth) and several head axes, the exact law at the
    /// planned `|DK|` never exceeds the envelope joint. Anti-vacuity: the
    /// envelope is strictly under the old worst case for a head along the
    /// vertical, and above the aligned post's radius.
    #[test]
    fn the_placement_envelope_carries_every_admissible_post() {
        let c = cos_lean();
        let lean_max = c.acos();
        let mut seed = 0x9E37_79B9_7F4A_7C15_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1_u64 << 53) as f64
        };
        for unit in [
            [0.66, 0.0, -0.75],
            [0.0, 0.0, -1.0],
            [0.0, 0.95, -0.31],
            [-0.6, 0.6, -0.529_150_262_212_918],
        ] {
            for tip in [0.05, 0.15] {
                let (joint, _) = solve_joint_for_axis(tip, GAP_MM + 0.1, unit, c, 1.0)
                    .expect("fits");
                let length = joint.length_mm;
                let unit = geom_kernel::vec3::normalize(unit).expect("a unit");
                for _ in 0..1000 {
                    let (lean, azimuth) = (next() * lean_max, next() * core::f64::consts::TAU);
                    let post = [lean.sin() * azimuth.cos(), lean.sin() * azimuth.sin(), -lean.cos()];
                    let law = HeadLaw::of_post(unit, post).expect("descends");
                    let carried = final_joint_radius_law_mm(tip, length, law);
                    assert!(
                        carried <= joint.joint_radius_mm + 1e-9,
                        "unit {unit:?} tip {tip}: post {post:?} needs {carried}, envelope {}",
                        joint.joint_radius_mm
                    );
                }
                // Never above the old 90 degree worst case.
                let worst = solve_joint(tip, GAP_MM + 0.1, c, 1.0).expect("fits");
                assert!(joint.joint_radius_mm <= worst.joint_radius_mm + 1e-12);
            }
        }
        // A head along the vertical can only be loaded along its own axis
        // by a vertical post, and tilted by a leaning one: the envelope is
        // under the old worst case by a wide margin.
        let (flat, _) = solve_joint_for_axis(0.05, GAP_MM, [0.0, 0.0, -1.0], c, 1.0).expect("fits");
        let worst = solve_joint(0.05, GAP_MM, c, 1.0).expect("fits");
        assert!(flat.joint_radius_mm < 0.97 * worst.joint_radius_mm, "{flat:?} against {worst:?}");
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
            covered_demand: Vec::new(), held_birth_cells: Vec::new(), twin_host: false,
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
            ball_allowance_mm: 0.0,
            link_mm: 0.0,
            drawn_straight: false,
            law_planned: HeadLaw::envelope(cos_lean()),
            law_drawn: None,
            ledger_radius_mm: Vec::new(),
            certified: None,
            old_law: None,
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
            profile_knots_mm: vec![[0.0, 0.07], [0.52, 0.22]],
            link_mm: 0.0,
            link_radius_mm: 0.0,
            certified: false,
        };
        assert_eq!(payload.profile_knots_mm.len(), 2);
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
        check_head, head_triggers, plan_graze_head, plan_head, placement_normals, skims, Head,
        HeadCapsule, HeadCheckCtx, HeadRefusal, HeadRequest, HeadRule, PlacementBall, PlacementEnv,
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
            link_mm: 0.0,
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
            ball_radius_mm: 0.0,
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
                hidden: &self.history.hidden,
                except: &[],
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
            // The joint carries every post the lean law admits under THIS
            // 41.3 degree axis: 0.2124, under the 90 degree worst case's 0.2204.
            assert!((head.joint_radius_mm - 0.2124).abs() < 1e-4, "{}", head.joint_radius_mm);
            let length = vec3::dist(head.contact_mm, head.joint_mm);
            assert!((length - 0.5124).abs() < 1e-4, "|DK| = {length}");
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
            ball_radius_mm: 0.0,
            seat_cell: 5,
            k_top,
        }
    }

    /// THE LINK moves the joint (Craig 2026-10-08): `K = D + (g + 2R + L + r_K)u`,
    /// so with the link the head is `L` longer than the same head without it
    /// plus the growth of the joint for the longer lever, and the head keeps the
    /// registered link.
    #[test]
    fn the_planned_joint_stands_the_link_beyond_the_ball() {
        let grid = grid();
        let (plain, mut linked) = (rule(0.0), rule(0.0));
        linked.link_mm = 0.3;
        let mut req = request(D, 0.15, 41);
        req.ball_radius_mm = 0.21;
        let without = plan_head(&req, &plain, &grid).expect("a head without a link");
        let with = plan_head(&req, &linked, &grid).expect("a head with a link");
        assert!((with.link_mm - 0.3).abs() < 1e-15 && without.link_mm == 0.0);
        let length = |head: &Head| vec3::dist(head.contact_mm, head.joint_mm);
        for head in [&without, &with] {
            let want = GAP + 2.0 * 0.21 + head.link_mm + head.joint_radius_mm;
            assert!((length(head) - want).abs() < 1e-9, "|DK| = {} against {want}", length(head));
            assert!((head.lever_gap_mm() + head.joint_radius_mm - want).abs() < 1e-9);
        }
        assert!(with.joint_radius_mm > without.joint_radius_mm, "a longer lever grows the joint");
        assert!(length(&with) > length(&without) + 0.3, "the link and the joint's growth");
    }

    #[test]
    fn the_plan_refuses_by_reason() {
        let grid = grid();
        let r = rule(0.0);
        let refusal = |req: HeadRequest| plan_head(&req, &r, &grid).expect_err("a refusal");
        // The per-axis envelope (this head's own 41.3 degree axis) carries more
        // than the 90 degree worst case did, so the threshold moves up from
        // 0.2944: find it by bisection on the envelope itself.
        let fits = |tip: f64| {
            super::solve_joint_for_axis(tip, GAP, unit_at(41.3), r.cos_lean_max, r.max_radius_mm)
                .is_some()
        };
        let (mut fit, mut over_tip) = (0.2943, 0.6);
        assert!(fits(fit) && !fits(over_tip));
        for _ in 0..60 {
            let mid = 0.5 * (fit + over_tip);
            if fits(mid) {
                fit = mid;
            } else {
                over_tip = mid;
            }
        }
        assert!(fit > 0.2944, "the per-axis threshold {fit} is above the worst case's");
        let over = refusal(request(D, over_tip + 1e-6, 41));
        println!("r {over_tip} -> {over:?}");
        assert_eq!(over, HeadRefusal::JointOverMax);
        // Just under the threshold still builds.
        assert!(plan_head(&request(D, fit - 1e-6, 41), &r, &grid).is_ok());
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
        // The graze conversion has no steepness gate: a flat underside and a
        // shallow slope are planned; only a seat plane outside the band is not.
        let flat = plan_graze_head(&straight, &r, &grid).expect("a flat underside is planned");
        assert!((flat.unit[2] + 1.0).abs() < 1e-12);
        assert!(plan_graze_head(&shallow, &r, &grid).is_ok());
        assert_eq!(plan_graze_head(&beyond, &r, &grid).unwrap_err(), HeadRefusal::NotDownFacing);
        let up = HeadRequest { unit: [0.0, 0.0, 1.0], ..request(D, 0.05, 41) };
        assert_eq!(plan_graze_head(&up, &r, &grid).unwrap_err(), HeadRefusal::NotDownFacing);
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
            ball_radius_mm: 0.0,
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
            ball_radius_mm: 0.0,
            seat_cell: 5,
            k_top: 381,
        };
        let surface = surface_of(&mesh);
        let mut history = ColumnHistory::new(&cavity_grid);
        let keep_out = DrainKeepOut::none();
        // With and without the link: the cavity exemption holds for both.
        let mut linked = rule(0.0);
        linked.link_mm = 0.3;
        let heads: Vec<_> = [rule(0.0), linked]
            .iter()
            .map(|rule| plan_head(&req, rule, &cavity_grid).expect("a head in the cavity"))
            .collect();
        let check = |history: &ColumnHistory, head: &Head| {
            let ctx = HeadCheckCtx {
                grid: &cavity_grid,
                history,
                hidden: &history.hidden,
                except: &[],
                keep_out: &keep_out,
                surface: Some(&surface),
                mesh: &mesh,
                plate_z_mm: 0.0,
                layer_height_mm: H,
                separation_mm: SEPARATION,
            };
            check_head(head, 0.05, &ctx, &[])
        };
        for head in &heads {
            assert_eq!(check(&history, head), Ok(()), "control: no map");
        }
        history.hidden = map;
        for head in &heads {
            let verdict = check(&history, head);
            println!("head in a hidden cavity (link {}) -> {verdict:?}", head.link_mm);
            assert_eq!(verdict, Err(HeadRefusal::HiddenCavity));
        }
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
