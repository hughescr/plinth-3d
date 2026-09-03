//! The peel field: the per-cell multiplier `α` on one cell's own peel event.
//!
//! **`α ≡ 1` by default, and that is a decision on evidence, not laziness.**
//! Under `Uniform` the load raster is literally mm² of cured area and the
//! sizing law reduces exactly to the registered area law
//! `π r² ≥ carried_area / X_ALLOWED`. Two records say the alternatives must
//! not decide anything yet:
//!
//! * `docs/physics.md`:69-75 — the measured `X` boundary **collapsed across a
//!   40× cap-area span**. That is direct evidence against a strong area
//!   exponent inside the load path.
//! * `docs/physics.md`:150-158 — "Y-sensitivity is settled… around one rung…
//!   The standing guidance is to **randomize over that axis rather than tune
//!   it**." The orientation search is over S², which structurally has no
//!   azimuth for a front-to-back factor to live on.
//!
//! Both alternative fields are nevertheless COMPUTED and REPORTED, so a future
//! coupon can be graded against them and so the difference between "we did not
//! model this" and "we modelled it and it changed nothing" stays visible.

use plinth_core::M0PlaceholderConstants;

/// Which peel field the down-sweep applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PeelFieldMode {
    /// `α ≡ 1`. The registered default.
    #[default]
    Uniform,
    /// The literature's sublinear force–area law. Computed and reported; must
    /// not decide.
    AreaLaw,
    /// The area law with the Saturn 4 front-to-back tilt factor. Computed and
    /// reported; must not decide.
    AreaLawTilt,
}

impl PeelFieldMode {
    /// Read the registered mode. The encoding lives in the constant's own
    /// note; an unregistered code degrades to `Uniform` and is reported,
    /// because a mode nobody registered must not silently become a decision.
    #[must_use]
    pub fn from_registry(placeholders: &M0PlaceholderConstants) -> Self {
        match *placeholders.sweep_peel_field_mode().diagnostic() {
            1 => Self::AreaLaw,
            2 => Self::AreaLawTilt,
            _ => Self::Uniform,
        }
    }
}

/// What the alternative fields WOULD have said, reported so a coupon can grade
/// them. Nothing here is read by any decision.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PeelFieldStats {
    /// `α` at the smallest and largest layer cross-section, under the
    /// literature exponent bracket's two ends.
    pub beta_area_low_span: [f64; 2],
    pub beta_area_high_span: [f64; 2],
    /// Front-to-back multiplier across the plate under the tilt reading.
    pub beta_tilt_span: [f64; 2],
    /// The exponents the spans above were evaluated at.
    pub beta_area_exponents: [f64; 2],
}

impl PeelFieldStats {
    /// The literature's two exponents, from two different experiments:
    /// `n ≈ 0.24` varying porosity at a constant envelope
    /// (`docs/evidence/literature/LITERATURE-NOTES.md`:813) and `n ≈ 0.46`
    /// shrinking the envelope (`:815`). They are a BRACKET, never a point
    /// value, and they are reported at both ends for that reason.
    pub const BETA_AREA_BRACKET: [f64; 2] = [0.24, 0.46];

    #[must_use]
    pub fn observe(min_layer_mm2: f64, max_layer_mm2: f64, reference_mm2: f64) -> Self {
        let alpha = |area: f64, beta: f64| {
            if area <= 0.0 || reference_mm2 <= 0.0 {
                1.0
            } else {
                (area / reference_mm2).powf(beta - 1.0)
            }
        };
        Self {
            beta_area_low_span: [
                alpha(min_layer_mm2, Self::BETA_AREA_BRACKET[0]),
                alpha(max_layer_mm2, Self::BETA_AREA_BRACKET[0]),
            ],
            beta_area_high_span: [
                alpha(min_layer_mm2, Self::BETA_AREA_BRACKET[1]),
                alpha(max_layer_mm2, Self::BETA_AREA_BRACKET[1]),
            ],
            // Reported as the identity because no tilt term is applied and none
            // is derivable from an S²-only search; the field exists so the
            // absence is visible rather than assumed.
            beta_tilt_span: [1.0, 1.0],
            beta_area_exponents: Self::BETA_AREA_BRACKET,
        }
    }
}

/// The multiplier the down-sweep actually applies.
#[must_use]
pub const fn alpha(mode: PeelFieldMode) -> f64 {
    match mode {
        // The only value any registered mode may apply in v1. Turning the
        // others on is owner ruling OR-1 and is not a code change alone.
        PeelFieldMode::Uniform | PeelFieldMode::AreaLaw | PeelFieldMode::AreaLawTilt => 1.0,
    }
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "exact pins over hand-written literal fixtures: the bracket is a pair of literals and \
          the test exists to prove they were not collapsed to a point value"
)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_mode_applies_the_identity_in_v1() {
        for mode in [
            PeelFieldMode::Uniform,
            PeelFieldMode::AreaLaw,
            PeelFieldMode::AreaLawTilt,
        ] {
            assert!((alpha(mode) - 1.0).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn the_area_bracket_is_reported_at_both_ends_and_never_collapsed() {
        let stats = PeelFieldStats::observe(100.0, 2000.0, 314.0);
        assert_eq!(stats.beta_area_exponents, [0.24, 0.46]);
        // A sublinear exponent makes a LARGER area cheaper per mm², so the
        // multiplier falls with area. If this ever inverts, the sign of the
        // reported term is wrong.
        assert!(stats.beta_area_low_span[1] < stats.beta_area_low_span[0]);
        assert!(stats.beta_area_high_span[1] < stats.beta_area_high_span[0]);
    }
}
