//! The support-profile geometry the owner prints with in ChituBox, as data.
//!
//! **UNITS TRAP**: ChituBox and the owner speak DIAMETER; the support engine
//! speaks RADIUS. Every field here is a DIAMETER, named as one, and the one
//! query that takes an engine size ([`TipContactProfiles::ball_factor`])
//! takes a RADIUS and says so.

use thiserror::Error;

/// One ChituBox support profile's top contact: the diameter of the cone at
/// its upper end (`top_support.supporttipupdiameter`) and the diameter of the
/// contact sphere seated on the model (`top_support.supporttouchtipdiameter`).
///
/// The native bytes of a ChituBox-authored file show which is which: the
/// type-7 contact sphere carries radius `supporttouchtipdiameter / 2` and the
/// type-1 taper that hangs from its centre starts at radius
/// `supporttipupdiameter / 2` (`Chitubox Projects/cylinder-cube-sphere.chitubox`,
/// Heavy profile: sphere 0.40, taper 0.30 -> 0.75).
#[derive(Clone, Debug, PartialEq)]
pub struct TipContactProfile {
    name: String,
    tip_up_diameter_mm: f64,
    touch_diameter_mm: f64,
}

impl TipContactProfile {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `top_support.supporttipupdiameter`, a DIAMETER in mm.
    #[must_use]
    pub const fn tip_up_diameter_mm(&self) -> f64 {
        self.tip_up_diameter_mm
    }

    /// `top_support.supporttouchtipdiameter`, a DIAMETER in mm.
    #[must_use]
    pub const fn touch_diameter_mm(&self) -> f64 {
        self.touch_diameter_mm
    }

    /// The contact-sphere-to-tip ratio this profile draws:
    /// touch diameter over tip-up diameter.
    #[must_use]
    pub fn ball_factor(&self) -> f64 {
        self.touch_diameter_mm / self.tip_up_diameter_mm
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum TipContactProfilesError {
    #[error("a tip-contact profile table needs at least one profile")]
    Empty,
    #[error("profile {name}: diameters must be finite and greater than zero")]
    NonPositive { name: String },
    #[error(
        "profile {name}: the contact sphere must be wider than the tip it sits on \
         (a ball factor of 1 or less has no seat above its own tip)"
    )]
    BallNotWiderThanTip { name: String },
    #[error("profiles {first} and {second} share a tip-up diameter; the table cannot be read by tip size")]
    DuplicateTipSize { first: String, second: String },
    #[error("a profile name must not be empty")]
    EmptyName,
}

/// The owner's ChituBox profiles' contact geometry, ordered by tip size, and
/// the ONE rule that reads a contact-sphere factor off them for a tip of any
/// size.
///
/// **The rule.** The ball factor at a tip is linear in the tip's DIAMETER
/// between the two profiles whose tip-up diameters bracket it, and held at the
/// nearest profile's own factor outside the owner's range. It is never
/// extrapolated: below the smallest and above the largest profile the owner
/// has no practice to extend, so the end profile's own factor stands.
#[derive(Clone, Debug, PartialEq)]
pub struct TipContactProfiles {
    profiles: Vec<TipContactProfile>,
}

impl TipContactProfiles {
    /// Build the table from `(name, tip-up DIAMETER, touch DIAMETER)` rows in
    /// any order; it is stored sorted by tip-up diameter.
    ///
    /// # Errors
    ///
    /// An empty table, a non-finite or non-positive diameter, a contact sphere
    /// no wider than its tip, or two profiles at one tip size.
    pub fn new<I, S>(rows: I) -> Result<Self, TipContactProfilesError>
    where
        I: IntoIterator<Item = (S, f64, f64)>,
        S: Into<String>,
    {
        let mut profiles = Vec::new();
        for (name, tip_up, touch) in rows {
            let name = name.into();
            if name.trim().is_empty() {
                return Err(TipContactProfilesError::EmptyName);
            }
            if !(tip_up.is_finite() && tip_up > 0.0 && touch.is_finite() && touch > 0.0) {
                return Err(TipContactProfilesError::NonPositive { name });
            }
            if touch <= tip_up {
                return Err(TipContactProfilesError::BallNotWiderThanTip { name });
            }
            profiles.push(TipContactProfile {
                name,
                tip_up_diameter_mm: tip_up,
                touch_diameter_mm: touch,
            });
        }
        if profiles.is_empty() {
            return Err(TipContactProfilesError::Empty);
        }
        profiles.sort_by(|a, b| a.tip_up_diameter_mm.total_cmp(&b.tip_up_diameter_mm));
        if let Some(pair) = profiles
            .windows(2)
            // Sorted, so a repeat is a neighbour that is not strictly larger:
            // the interpolation needs strictly increasing tip sizes.
            .find(|pair| pair[1].tip_up_diameter_mm <= pair[0].tip_up_diameter_mm)
        {
            return Err(TipContactProfilesError::DuplicateTipSize {
                first: pair[0].name.clone(),
                second: pair[1].name.clone(),
            });
        }
        Ok(Self { profiles })
    }

    /// The profiles, smallest tip first.
    #[must_use]
    pub fn profiles(&self) -> &[TipContactProfile] {
        &self.profiles
    }

    /// **The contact-sphere factor for a tip of `tip_radius_mm` RADIUS**: the
    /// emitted ball radius is this factor times the tip radius.
    ///
    /// Linear in tip DIAMETER between the bracketing profiles' own factors,
    /// and the end profile's factor outside the table (see the type's rule).
    /// Always greater than one, because every profile's is. A non-finite
    /// radius reads the smallest profile's factor rather than a NaN.
    #[must_use]
    pub fn ball_factor(&self, tip_radius_mm: f64) -> f64 {
        let diameter = 2.0 * tip_radius_mm;
        let first = &self.profiles[0];
        let last = &self.profiles[self.profiles.len() - 1];
        if !diameter.is_finite() || diameter <= first.tip_up_diameter_mm {
            return first.ball_factor();
        }
        if diameter >= last.tip_up_diameter_mm {
            return last.ball_factor();
        }
        let upper = self
            .profiles
            .iter()
            .position(|profile| profile.tip_up_diameter_mm >= diameter)
            .unwrap_or(self.profiles.len() - 1);
        let (below, above) = (&self.profiles[upper - 1], &self.profiles[upper]);
        let t = (diameter - below.tip_up_diameter_mm)
            / (above.tip_up_diameter_mm - below.tip_up_diameter_mm);
        below.ball_factor() + t * (above.ball_factor() - below.ball_factor())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> TipContactProfiles {
        TipContactProfiles::new([
            ("Heavy", 0.6, 0.8),
            ("Light", 0.2, 0.3),
            ("Middle", 0.4, 0.5),
            ("Ultralight", 0.15, 0.2),
        ])
        .expect("valid fixture")
    }

    #[test]
    fn rows_are_sorted_by_tip_size() {
        let table = table();
        let names: Vec<&str> = table
            .profiles()
            .iter()
            .map(TipContactProfile::name)
            .collect();
        assert_eq!(names, ["Ultralight", "Light", "Middle", "Heavy"]);
    }

    #[test]
    fn a_tip_at_a_profile_reads_that_profiles_own_ratio() {
        let t = table();
        // RADIUS in, so half of each profile's tip-up DIAMETER.
        assert!((t.ball_factor(0.075) - 0.2 / 0.15).abs() < 1e-12);
        assert!((t.ball_factor(0.1) - 1.5).abs() < 1e-12);
        assert!((t.ball_factor(0.2) - 1.25).abs() < 1e-12);
        assert!((t.ball_factor(0.3) - 0.8 / 0.6).abs() < 1e-12);
    }

    #[test]
    fn between_profiles_the_ratio_is_linear_in_tip_diameter() {
        // Diameter 0.3 is halfway between Light (0.2, 1.5) and Middle (0.4, 1.25).
        assert!((table().ball_factor(0.15) - 1.375).abs() < 1e-12);
    }

    #[test]
    fn outside_the_owners_range_the_end_profile_holds_and_nothing_extrapolates() {
        let t = table();
        assert!((t.ball_factor(0.05) - 0.2 / 0.15).abs() < 1e-12);
        assert!((t.ball_factor(1.0) - 0.8 / 0.6).abs() < 1e-12);
        assert!((t.ball_factor(f64::NAN) - 0.2 / 0.15).abs() < 1e-12);
    }

    #[test]
    fn a_ball_no_wider_than_its_tip_or_a_repeated_tip_size_is_refused() {
        assert!(matches!(
            TipContactProfiles::new([("flat", 0.3, 0.3)]),
            Err(TipContactProfilesError::BallNotWiderThanTip { .. })
        ));
        assert!(matches!(
            TipContactProfiles::new([("a", 0.3, 0.4), ("b", 0.3, 0.5)]),
            Err(TipContactProfilesError::DuplicateTipSize { .. })
        ));
        assert!(matches!(
            TipContactProfiles::new(Vec::<(&str, f64, f64)>::new()),
            Err(TipContactProfilesError::Empty)
        ));
    }
}
