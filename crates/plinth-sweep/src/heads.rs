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
