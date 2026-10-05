//! How far a descending arm may lean, and what the lean costs.
//!
//! Until this module existed every arm in this engine descended STRAIGHT DOWN
//! from its seat to the first thing in its own column. That is what the owner
//! saw in ChituBox on 2026-09-03: "all of the supports only descend directly
//! vertically -- none angle down to merge trunks". A field of vertical columns
//! cannot merge two arms into one trunk, cannot step around a drain's escape
//! cylinder, and cannot reach the model surface a fraction of a millimetre to
//! one side; so the load routed down a narrowing body piles onto whatever tiny
//! footprint sits directly beneath it.
//!
//! # The law
//!
//! An arm is itself printed material, so the angle it may lean at is the angle
//! the MODEL is allowed to overhang at, and nothing else is invented here. Per
//! layer of descent the axis may move sideways by
//!
//! ```text
//! s <= h * cot(theta_sat)      theta_sat = centre - transition/2
//! ```
//!
//! where `h` is the layer height, `theta` is measured from HORIZONTAL exactly
//! as `OVERHANG_ANGLE_DEG` is (`crates/plinth-orient/src/score.rs`:73 reads the
//! same convention as `-sin(theta)` on a face normal), `centre` is the
//! registered `OVERHANG_ANGLE_DEG` and `transition` the registered
//! `OVERHANG_WEIGHT_TRANSITION_DEG`.
//!
//! `theta_sat` is not a fourth number: it is the shallow end of the logistic's
//! own 10-to-90 band, the angle below which the weight has saturated and a
//! further degree of lean buys nothing while costing more material. At the
//! registered 45 deg centre and 25 deg width that is 32.5 deg, so one layer of
//! descent may move the axis by at most `1.5697 * h`.
//!
//! # The price is continuous, not a cliff at 45 degrees
//!
//! The lean is not admitted-or-refused at the centre angle. Every leaned layer
//! is charged
//!
//! ```text
//! cost = sqrt(h^2 + s^2) * (1 + w(theta))
//! w(theta) = 1 / (1 + exp(2*ln(9)/transition * (theta - centre)))
//! ```
//!
//! `sqrt(h^2 + s^2)` is the arm's own material: a leaning layer is longer than
//! a vertical one, which is the whole of the geometric cost and is not a
//! weight. `w` is the registered overhang weight evaluated at the angle THIS
//! LAYER of the arm makes with horizontal, and it prices risk: `w = 0` for a
//! near-vertical arm (the factor is exactly 1 and a straight descent is priced
//! at its bare length), `w = 1` for an arm leaning at the saturated angle,
//! where the layer is charged as if its material had to be paid for twice. The
//! `1 +` is a structural statement -- full weight doubles the charge -- and not
//! a tuned coefficient; there is no third number in this file.
//!
//! # What this buys, and how to measure it
//!
//! [`LeanLaw::disabled`] pins the maximum step at zero, which reduces every
//! descent to the straight-down descent this engine had before. That is the
//! sabotage knob: with it set, a merge count must fall to zero and the arms a
//! drain gallery crosses must be deleted again.

/// The overhang angle law, evaluated for one layer height.
///
/// Cloneable and `Copy` so a layer loop can hold it by value; it is three
/// registered numbers and a derived maximum step, and it allocates nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeanLaw {
    layer_height_mm: f64,
    centre_deg: f64,
    transition_deg: f64,
    max_step_mm: f64,
}

impl LeanLaw {
    /// Build the law from the layer height and the two registered angles.
    ///
    /// A non-finite or non-positive input yields the disabled law rather than a
    /// panic: emit and warn, never refuse.
    #[must_use]
    pub fn new(layer_height_mm: f64, centre_deg: f64, transition_deg: f64) -> Self {
        let sane = layer_height_mm.is_finite()
            && layer_height_mm > 0.0
            && centre_deg.is_finite()
            && transition_deg.is_finite()
            && transition_deg > 0.0;
        if !sane {
            return Self::disabled(layer_height_mm.max(0.0));
        }
        let saturated_deg = centre_deg - 0.5 * transition_deg;
        // cot(theta) at the shallow end of the transition band. A saturated
        // angle at or below zero would mean the registry admits a horizontal
        // arm, which no printer does; the law degrades to straight-down rather
        // than producing an infinite step.
        let max_step_mm = if saturated_deg > 0.0 && saturated_deg < 90.0 {
            layer_height_mm / saturated_deg.to_radians().tan()
        } else {
            0.0
        };
        Self {
            layer_height_mm,
            centre_deg,
            transition_deg,
            max_step_mm,
        }
    }

    /// The law with no lean at all: every descent is straight down, exactly as
    /// this engine behaved before 2026-09-04. **Sabotage and comparison only.**
    #[must_use]
    pub const fn disabled(layer_height_mm: f64) -> Self {
        Self {
            layer_height_mm,
            centre_deg: 0.0,
            transition_deg: 0.0,
            max_step_mm: 0.0,
        }
    }

    /// The layer height this law was built for.
    #[must_use]
    pub const fn layer_height_mm(&self) -> f64 {
        self.layer_height_mm
    }

    /// The largest sideways move one layer of descent may make, in mm.
    #[must_use]
    pub const fn max_step_mm(&self) -> f64 {
        self.max_step_mm
    }

    /// Whether this law admits any lean at all.
    #[must_use]
    pub fn leans(&self) -> bool {
        self.max_step_mm > 0.0
    }

    /// `1 / cos(steepest lean from vertical)`: how much longer than the
    /// clearance it stands against a face an arm's own shaft rises off, at the
    /// worst tilt the law admits. A shaft of radius `r` stands `tier` clear of
    /// the plane of the face it seats against `(tier + r) / (n·m)` along its
    /// axis (`member_clearance::model_landing_zones_mm`), and the face an arm
    /// stands against is never tilted past the angle the overhang law lets an
    /// arm lean to, so `n·m ≥ cos(lean)`. Derived from the law, not typed:
    /// `hypot(max_step, h) / h`. `1` for the disabled law (no lean at all).
    #[must_use]
    pub fn face_zone_factor(&self) -> f64 {
        if self.layer_height_mm > 0.0 {
            self.max_step_mm.hypot(self.layer_height_mm) / self.layer_height_mm
        } else {
            1.0
        }
    }

    /// The registered overhang weight at the angle a layer with sideways move
    /// `step_mm` makes with horizontal. `0` for a vertical layer, rising to `1`
    /// as the arm lies over.
    #[must_use]
    pub fn weight_at_step(&self, step_mm: f64) -> f64 {
        if !self.leans() || step_mm <= 0.0 || !step_mm.is_finite() {
            return 0.0;
        }
        // theta from horizontal: a step of zero is vertical (90 deg), a step
        // that dwarfs the layer height lies flat (0 deg).
        let theta_deg = self.layer_height_mm.atan2(step_mm).to_degrees();
        let steepness = 2.0 * 9.0_f64.ln() / self.transition_deg;
        1.0 / (1.0 + (steepness * (theta_deg - self.centre_deg)).exp())
    }

    /// What one layer of descent with sideways move `step_mm` costs, in mm of
    /// weighted arm length.
    ///
    /// This is the arm's own material under the existing objective -- the
    /// section `pi r^2` is the same for every candidate descent of the SAME arm
    /// and cancels out of the comparison, so length is the whole of it -- with
    /// the leaned part of it charged by the registered overhang weight.
    #[must_use]
    pub fn layer_cost_mm(&self, step_mm: f64) -> f64 {
        let step = if step_mm.is_finite() && step_mm > 0.0 {
            step_mm
        } else {
            0.0
        };
        self.layer_height_mm.hypot(step) * (1.0 + self.weight_at_step(step))
    }

    /// The FEWEST layers of descent that can close a lateral gap of
    /// `lateral_mm`, or `0` when this law can never close it.
    ///
    /// This is the law read the other way round, and it is what makes a landing
    /// part-way down a shaft findable at all. The descent search knows the
    /// lateral distance to a column before it knows what stands in that column,
    /// so it asks the law "how far down must I be, at the very least" and then
    /// asks the column what it has at or below that layer. Reading the column
    /// first and the law second is what confined this engine to landing on an
    /// arm's TOP: a top a few layers below a seat that is a whole hex spacing
    /// away is simply not reachable, and the candidate died as "no path"
    /// instead of becoming a landing further down the same trunk.
    ///
    /// A descent of `n` layers moves at most `n * max_step`, so `n` is
    /// `ceil(lateral / max_step)`, and never fewer than one layer. The ceiling
    /// is verified against [`Self::descent_cost_mm`]'s own admissibility test
    /// rather than trusted, because a division and a `ceil` can land a hair on
    /// the wrong side of it and a candidate that the law admits but the pricing
    /// refuses would be dropped in silence.
    #[must_use]
    pub fn layers_to_reach(&self, lateral_mm: f64) -> usize {
        if !lateral_mm.is_finite() || lateral_mm <= 0.0 {
            return 1;
        }
        if !self.leans() {
            return 0;
        }
        let exact = lateral_mm / self.max_step_mm;
        if !exact.is_finite() {
            return 0;
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a finite non-negative ratio of two lengths, rounded up to a layer count"
        )]
        let mut layers = (exact.ceil() as usize).max(1);
        #[expect(
            clippy::cast_precision_loss,
            reason = "a layer count bounded by the plan's own layer count is exact in f64"
        )]
        let mut step = lateral_mm / layers as f64;
        // At most one correction in practice; bounded so a pathological input
        // cannot spin.
        let mut guard = 0;
        while step > self.max_step_mm && guard < 4 {
            layers += 1;
            #[expect(
                clippy::cast_precision_loss,
                reason = "a layer count bounded by the plan's own layer count is exact in f64"
            )]
            let next = lateral_mm / layers as f64;
            step = next;
            guard += 1;
        }
        if step > self.max_step_mm {
            return 0;
        }
        layers
    }

    /// The cost of a whole descent of `layers` layers that moves `lateral_mm`
    /// sideways in total, spreading the move evenly.
    ///
    /// Spreading evenly is the cheapest way to travel a given distance: the
    /// per-layer cost is convex in the step (`hypot` is convex, and the weight
    /// factor is non-decreasing in the step), so Jensen puts the minimum at the
    /// equal-step path. That is why the descent search never has to enumerate
    /// the shape of a path, only its endpoints.
    ///
    /// Returns `None` when the move needs a steeper lean than the law admits.
    #[must_use]
    pub fn descent_cost_mm(&self, layers: usize, lateral_mm: f64) -> Option<f64> {
        if layers == 0 {
            return None;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "a layer count bounded by the plan's own layer count is exact in f64"
        )]
        let count = layers as f64;
        let step = lateral_mm / count;
        if step > self.max_step_mm {
            return None;
        }
        Some(count * self.layer_cost_mm(step))
    }
}

#[cfg(test)]
mod tests {
    use super::LeanLaw;

    const H: f64 = 0.05;

    fn registered() -> LeanLaw {
        LeanLaw::new(H, 45.0, 25.0)
    }

    #[test]
    fn the_maximum_step_is_the_cotangent_of_the_shallow_end_of_the_transition_band() {
        let law = registered();
        let expected = H / 32.5_f64.to_radians().tan();
        assert!(
            (law.max_step_mm() - expected).abs() < 1.0e-12,
            "max step {} should be h*cot(32.5 deg) = {expected}",
            law.max_step_mm()
        );
    }

    #[test]
    fn a_vertical_layer_is_priced_at_its_bare_length() {
        let law = registered();
        assert!((law.layer_cost_mm(0.0) - H).abs() < 1.0e-15);
        assert!((law.weight_at_step(0.0) - 0.0).abs() < 1.0e-15);
    }

    /// The centre of the logistic is the registered `OVERHANG_ANGLE_DEG`, so a
    /// layer leaning at exactly 45 deg from horizontal -- a sideways move of
    /// one layer height -- must read weight 0.5. This is what makes the price
    /// continuous instead of a cliff: 45 deg is not a limit, it is the middle.
    #[test]
    fn the_centre_angle_is_priced_at_half_weight_rather_than_refused() {
        let law = registered();
        let w = law.weight_at_step(H);
        assert!((w - 0.5).abs() < 1.0e-12, "weight at the centre angle was {w}");
        let expected = H.hypot(H) * 1.5;
        assert!((law.layer_cost_mm(H) - expected).abs() < 1.0e-12);
    }

    /// The 10-to-90 band is exactly the registered width: the weight reads 0.9
    /// at the shallow end and 0.1 at the steep end.
    #[test]
    fn the_transition_width_is_the_ten_to_ninety_band() {
        let law = registered();
        let shallow = H / 32.5_f64.to_radians().tan();
        let steep = H / 57.5_f64.to_radians().tan();
        assert!((law.weight_at_step(shallow) - 0.9).abs() < 1.0e-12);
        assert!((law.weight_at_step(steep) - 0.1).abs() < 1.0e-12);
    }

    #[test]
    fn a_move_steeper_than_the_law_admits_has_no_cost_because_it_has_no_path() {
        let law = registered();
        assert!(law.descent_cost_mm(1, 10.0 * H).is_none());
        assert!(law.descent_cost_mm(0, 0.0).is_none());
    }

    /// Straight down is always the cheapest way to cross the same number of
    /// layers. This is the property that keeps a flat disc byte-identical.
    #[test]
    fn straight_down_is_never_beaten_over_the_same_number_of_layers() {
        let law = registered();
        let straight = law.descent_cost_mm(20, 0.0).expect("straight descent");
        for tenths in 1..=10 {
            let lateral = f64::from(tenths) * 0.1 * 20.0 * law.max_step_mm();
            let leaned = law.descent_cost_mm(20, lateral).expect("admissible lean");
            assert!(
                leaned > straight,
                "a lean of {lateral} mm cost {leaned} against {straight} straight down"
            );
        }
    }

    #[test]
    fn the_disabled_law_admits_nothing_and_prices_a_vertical_descent_unchanged() {
        let law = LeanLaw::disabled(H);
        assert!(!law.leans());
        assert!(law.max_step_mm().abs() < f64::EPSILON);
        assert!(law.descent_cost_mm(3, 0.001).is_none());
        assert!((law.descent_cost_mm(3, 0.0).expect("vertical") - 3.0 * H).abs() < 1.0e-15);
    }
}
