//! **THE NECK OF A MODEL FOOT, READ INTO THE ONE-PIXEL LAW** (round 5, flare
//! lane, 2026-10-14; `round5-2026-10-14/FLARE-DESIGN.md`, section 0 and 2).
//!
//! # What was wrong
//!
//! A post that stands on the model ends in a ball seated into the landed face,
//! and its shaft narrows to that ball in a cone (`plinth_support::lower`,
//! `lower_one_sweep_arm`, the foot cone at the lean law's slope). The run
//! measurement of [`crate::slenderness`] read such a post as a cantilever
//! clamped at the shaft's root, so the neck (one cured layer at the contact
//! radius) and the cone above it, the thinnest and most compliant part of the
//! run, were never read. The thicken rung then raised the root to make the run
//! hold and drew a carrot: a wide shaft on a hairline contact. The root did not
//! help, because the neck, not the shaft, sets the sway of the top.
//!
//! # What this module reads
//!
//! The ACTUAL coordinates of the foot, not an equivalent length:
//!
//! * a neck of one layer (one slot of the run along its axis) at the contact
//!   radius `r_n`. **This is an estimate**: the lowering draws no cylinder there
//!   (the cone starts at the ball centre), and the first cured layer of a ball
//!   seated into the face has a section that depends on the seat. It stays an
//!   estimate until it is checked against the emitted ball, cone and model union;
//! * the drawn cone from `r_n` to the shaft's root radius at the cone slope
//!   (`cot theta_sat`), `(root - r_n) / slope` long;
//! * the shaft, slot by slot, above that.
//!
//! The foot REPLACES the shaft slots it covers. Compliance of the run is the full
//! weighted integral `I = int_0^l (l - x)^3 / r(x)^4 dx` (the one-pixel kernel of
//! `slenderness.rs`), with the cone integrated in closed form so a finite cone
//! and the axial extent are both exact. The run holds when `(4 / S^4) I <= 1`;
//! the **neck share** `N = (4 / S^4) I_foot` is what the neck and cone alone use
//! of that one pixel. `N >= 1` means no shaft width can hold the run.
//!
//! The guarantee is the print-time growing cantilever under the S-law's
//! distributed drag, and nothing else (FLARE-DESIGN.md section 0, item 1).

/// The foot of a model-landed post, in mm along the post's axis from the
/// contact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FootGeom {
    /// The contact radius `r_n`, never wider than the root.
    pub neck_radius_mm: f64,
    /// The neck: one layer along the axis, at `neck_radius_mm` (an estimate).
    pub neck_mm: f64,
    /// The shaft's radius where the cone ends, the first drawn radius.
    pub root_mm: f64,
    /// The cone from `neck_radius_mm` to `root_mm`: `(root - r_n) / slope`.
    pub cone_mm: f64,
    /// The radius the cone gains per mm of height: the lean law's `cot theta_sat`.
    pub slope: f64,
}

impl FootGeom {
    /// The foot of a post whose first drawn radius is `root_mm`, whose contact
    /// radius is `neck_radius_mm`, with `layer_mm` of axis per layer and the
    /// cone `slope`. `None` for anything that is not a positive finite number.
    #[must_use]
    pub fn new(neck_radius_mm: f64, root_mm: f64, layer_mm: f64, slope: f64) -> Option<Self> {
        let sane = |v: f64| v.is_finite() && v > 0.0;
        if !(sane(neck_radius_mm) && sane(root_mm) && sane(layer_mm) && sane(slope)) {
            return None;
        }
        let neck = neck_radius_mm.min(root_mm);
        Some(Self {
            neck_radius_mm: neck,
            neck_mm: layer_mm,
            root_mm,
            cone_mm: (root_mm - neck) / slope,
            slope,
        })
    }

    /// The foot's axial extent: the neck and the cone.
    #[must_use]
    pub fn extent_mm(&self) -> f64 {
        self.neck_mm + self.cone_mm
    }

    /// The foot cut at `x_mm` (the run is shorter than the foot): the same
    /// foot, its extent reading only up to there.
    fn radius_at(&self, x_mm: f64) -> f64 {
        if x_mm <= self.neck_mm {
            self.neck_radius_mm
        } else {
            (self.neck_radius_mm + self.slope * (x_mm - self.neck_mm)).min(self.root_mm)
        }
    }

    /// The foot as a short list of `(top height mm, radius mm)` slices for a
    /// beam model: the neck, then the cone cut into `CONE_SLICES` pieces of equal
    /// radius gain, each at the radius whose bending flexibility equals the
    /// piece's (`(r_a^-3 - r_b^-3) / (3 (r_b - r_a))` per mm, the exact
    /// `int dx / r^4` of a linear taper). Discretisation of a beam model, not
    /// physics.
    #[must_use]
    pub fn slices(&self) -> Vec<(f64, f64)> {
        let mut out = vec![(self.neck_mm, self.neck_radius_mm)];
        if self.cone_mm <= 0.0 {
            return out;
        }
        let gain = (self.root_mm - self.neck_radius_mm) / f64::from(CONE_SLICES);
        for i in 0..CONE_SLICES {
            let (a, b) = (
                self.neck_radius_mm + gain * f64::from(i),
                self.neck_radius_mm + gain * f64::from(i + 1),
            );
            let flexibility = (a.powi(-3) - b.powi(-3)) / (3.0 * gain);
            let top = self.neck_mm + (b - self.neck_radius_mm) / self.slope;
            out.push((top, flexibility.powf(-0.25)));
        }
        out
    }
}

/// How many pieces the beam model cuts the cone into.
const CONE_SLICES: u32 = 8;

/// What one run reads once the foot is in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    /// The uniform radius whose cantilever sways as far as the run does, foot
    /// included: the run holds when `l <= S * radius_mm`.
    pub radius_mm: f64,
    /// The same without the foot (the shaft as a cantilever clamped at the
    /// contact): what a beam model that builds the foot explicitly takes for its
    /// posts, so the foot is counted once.
    pub shaft_radius_mm: f64,
    /// `(4 / S^4) * I_foot`: the share of the one pixel the neck and cone use.
    pub neck_share: f64,
    /// The foot that was read.
    pub foot: Option<FootGeom>,
}

/// `int_{x0}^{x1} (l - x)^3 / r^4 dx` for a constant radius.
fn constant(l: f64, x0: f64, x1: f64, r: f64) -> f64 {
    ((l - x0).powi(4) - (l - x1).powi(4)) / (4.0 * r.powi(4))
}

/// `int_{x0}^{x1} (l - x)^3 / r(x)^4 dx` for `r(x) = r0 + b (x - x0)`, in closed
/// form: with `R = r0 + b (l - x0)`, `(l - x) = (R - r) / b`, so the integrand is
/// `(R - r)^3 / (b^4 r^4) dr`.
fn linear(l: f64, x0: f64, x1: f64, r0: f64, b: f64) -> f64 {
    let r1 = r0 + b * (x1 - x0);
    let big = r0 + b * (l - x0);
    let anti = |r: f64| {
        -big.powi(3) / (3.0 * r.powi(3)) + 3.0 * big * big / (2.0 * r * r) - 3.0 * big / r - r.ln()
    };
    (anti(r1) - anti(r0)) / b.powi(4)
}

/// The shaft-only equivalent radius of `n` equal slots, bottom first: the
/// weighted power mean of [`crate::slenderness`]'s kernel. Slots with no radius
/// are left out and the weights re-normalised.
#[must_use]
pub fn shaft_equivalent(n: usize, radius_at: &dyn Fn(usize) -> Option<f64>) -> Option<f64> {
    if n == 0 {
        return None;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "layer counts bounded by the plan's own layer count are exact in f64"
    )]
    let n_f = n as f64;
    let (mut compliance, mut weight) = (0.0, 0.0);
    let (mut narrowest, mut widest) = (f64::INFINITY, 0.0_f64);
    for j in 0..n {
        let Some(radius) = radius_at(j) else {
            continue;
        };
        narrowest = narrowest.min(radius);
        widest = widest.max(radius);
        #[expect(
            clippy::cast_precision_loss,
            reason = "a slot index bounded by the layer count is exact in f64"
        )]
        let (upper, lower) = (((n - j) as f64) / n_f, ((n - j - 1) as f64) / n_f);
        let w = upper.powi(4) - lower.powi(4);
        weight += w;
        compliance += w / radius.powi(4);
    }
    (compliance > 0.0 && weight > 0.0)
        .then(|| (weight / compliance).powf(0.25).clamp(narrowest, widest))
}

/// Read a run of `n` equal slots over `l_mm` of axis, bottom first, whose slot
/// `j` has radius `radius_at(j)`. With `foot` = `Some((r_n, slope))` the run
/// stands on a model foot: its first slot's radius is the root, the foot is built
/// from the coordinates ([`FootGeom`]) and replaces the slots it covers. `ratio`
/// is `S`. `None` where the run has no slot or a slot has no radius under a foot.
#[must_use]
pub fn read(
    l_mm: f64,
    n: usize,
    ratio: f64,
    radius_at: &dyn Fn(usize) -> Option<f64>,
    foot: Option<(f64, f64)>,
) -> Option<Reading> {
    let shaft = shaft_equivalent(n, radius_at)?;
    let plain = Reading {
        radius_mm: shaft,
        shaft_radius_mm: shaft,
        neck_share: 0.0,
        foot: None,
    };
    let Some((neck_radius, slope)) = foot else {
        return Some(plain);
    };
    if !(l_mm.is_finite() && l_mm > 0.0 && ratio.is_finite() && ratio > 0.0) {
        return Some(plain);
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "layer counts bounded by the plan's own layer count are exact in f64"
    )]
    let d = l_mm / n as f64;
    let Some(root) = radius_at(0) else {
        return Some(plain);
    };
    let Some(geom) = FootGeom::new(neck_radius, root, d, slope) else {
        return Some(plain);
    };
    let extent = geom.extent_mm().min(l_mm);
    let mut foot_part = constant(l_mm, 0.0, geom.neck_mm.min(l_mm), geom.neck_radius_mm);
    if extent > geom.neck_mm {
        foot_part += linear(l_mm, geom.neck_mm, extent, geom.neck_radius_mm, geom.slope);
    }
    let mut total = foot_part;
    for j in 0..n {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a slot index bounded by the layer count is exact in f64"
        )]
        let (x0, x1) = (d * j as f64, d * (j + 1) as f64);
        if x1 <= extent {
            continue;
        }
        let Some(radius) = radius_at(j) else {
            return Some(plain);
        };
        total += constant(l_mm, x0.max(extent), x1, radius);
    }
    if !(total > 0.0 && total.is_finite()) {
        return Some(plain);
    }
    let four_over_s4 = 4.0 / ratio.powi(4);
    // `I = (l^4 / 4) / r_eq^4`.
    let radius_mm = (l_mm.powi(4) / (4.0 * total)).powf(0.25);
    Some(Reading {
        radius_mm,
        shaft_radius_mm: shaft,
        neck_share: four_over_s4 * foot_part,
        foot: Some(geom),
    })
}

/// The foot's radius `x_mm` above the contact, for the tests and the beam model.
#[must_use]
pub fn foot_radius_at(foot: &FootGeom, x_mm: f64) -> f64 {
    foot.radius_at(x_mm)
}

#[cfg(test)]
mod tests {
    use super::{constant, linear, read, shaft_equivalent, FootGeom};

    /// The closed form of the cone is the integral, by a fine midpoint sum.
    #[test]
    fn the_cone_integral_is_the_integral() {
        let (l, x0, x1, r0, b) = (11.3, 0.05, 0.28, 0.05, 1.5697);
        let steps = 200_000;
        let dx = (x1 - x0) / f64::from(steps);
        let mut sum = 0.0;
        for i in 0..steps {
            let x = x0 + dx * (f64::from(i) + 0.5);
            sum += (l - x).powi(3) / (r0 + b * (x - x0)).powi(4) * dx;
        }
        let closed = linear(l, x0, x1, r0, b);
        assert!((closed - sum).abs() / sum < 1e-6, "{closed} against {sum}");
    }

    /// A uniform post with no foot reads as itself, and the plain reading is the
    /// old equivalent radius.
    #[test]
    fn no_foot_is_the_plain_equivalent_radius() {
        let radius = |_: usize| Some(0.4);
        let plain = read(8.0, 160, 40.0, &radius, None).expect("a reading");
        assert!((plain.radius_mm - 0.4).abs() < 1e-12);
        assert!(plain.foot.is_none() && plain.neck_share == 0.0);
        let shaped = |j: usize| Some(0.4 - 0.002 * j as f64);
        let a = read(8.0, 160, 40.0, &shaped, None).expect("a reading").radius_mm;
        let b = shaft_equivalent(160, &shaped).expect("a radius");
        assert!((a - b).abs() < 1e-15);
    }

    /// The design's numbers from coordinates: a 11.3 mm post, tip 0.05, root
    /// 0.404 on a 0.05 mm contact. The neck and cone alone use more than the one
    /// pixel (the carrot hides it), and a foot of the same root on a wide contact
    /// uses little.
    #[test]
    fn a_hairline_contact_under_a_wide_root_uses_the_pixel() {
        let l = 11.32;
        let n = 226;
        let taper = |j: usize| Some(0.404 + (0.05 - 0.404) * (j as f64) / 225.0);
        let thin = read(l, n, 40.0, &taper, Some((0.05, 1.5697))).expect("a reading");
        assert!(thin.neck_share > 1.0, "N = {}", thin.neck_share);
        assert!(l > 40.0 * thin.radius_mm, "the run is over its limit with the foot");
        assert!(l <= 40.0 * thin.shaft_radius_mm * 1.05, "and held by its shaft alone");
        let wide = read(l, n, 40.0, &taper, Some((0.404, 1.5697))).expect("a reading");
        assert!((wide.radius_mm - wide.shaft_radius_mm).abs() < 1e-9, "no foot, no change");
    }

    /// The finite-cone identity of the challenger: for an unweighted `r^-4` read,
    /// `int dx / r^4` over a cone is `r_n / (3 slope) (1 - (r_n/R)^3) / r_n^4`.
    #[test]
    fn the_finite_cone_is_not_the_semi_infinite_one() {
        let (rn, big, slope) = (0.05_f64, 0.4_f64, 1.5697_f64);
        let semi = 1.0 / (3.0 * slope * rn.powi(3));
        let finite = semi * (1.0 - (rn / big).powi(3));
        let l = 1.0e-3; // (l - x)^3 ~ l^3 over a vanishing run, so the weighted form is the unweighted one
        let weighted = linear(l + 0.23, 0.0, 0.23, rn, slope);
        let direct = {
            let cone = 0.23;
            let steps = 100_000;
            let dx = cone / f64::from(steps);
            (0..steps)
                .map(|i| (l + 0.23 - dx * (f64::from(i) + 0.5)).powi(3) / (rn + slope * dx * (f64::from(i) + 0.5)).powi(4) * dx)
                .sum::<f64>()
        };
        assert!((weighted - direct).abs() / direct < 1e-6);
        assert!(finite < semi && finite > 0.0);
        let _ = constant(1.0, 0.0, 1.0, 1.0);
    }

    /// A foot longer than the run is cut at the run.
    #[test]
    fn a_foot_longer_than_the_run_is_cut_at_the_run() {
        let radius = |_: usize| Some(0.4);
        let r = read(0.1, 2, 40.0, &radius, Some((0.05, 1.5697))).expect("a reading");
        assert!(r.radius_mm.is_finite() && r.radius_mm > 0.0 && r.radius_mm <= 0.4);
        let g = FootGeom::new(0.05, 0.4, 0.05, 1.5697).expect("a foot");
        assert!(g.slices().len() == 9);
    }
}
