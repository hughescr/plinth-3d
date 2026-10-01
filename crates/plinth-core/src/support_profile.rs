//! The support-profile geometry the owner prints with in ChituBox, as data.
//!
//! **UNITS TRAP**: ChituBox and the owner speak DIAMETER; the support engine
//! speaks RADIUS. Every field here is a DIAMETER, named as one, and the two
//! queries that take an engine size ([`TipContactProfiles::ball_factor`],
//! [`RootBossProfiles::boss_factor`]) take a RADIUS and say so.

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
        ratio_by_diameter(
            &self.profiles,
            2.0 * tip_radius_mm,
            TipContactProfile::tip_up_diameter_mm,
            TipContactProfile::ball_factor,
        )
    }
}

/// **The ONE profile-reading rule** both tables share: the ratio at a size of
/// `diameter` mm DIAMETER is linear in diameter between the two profiles whose
/// own sizes bracket it, and the nearest end profile's own ratio outside the
/// owner's range (never extrapolated). A non-finite diameter reads the
/// smallest profile's ratio rather than a NaN.
///
/// `profiles` is non-empty and sorted by `size`, strictly increasing; both
/// table constructors guarantee it.
fn ratio_by_diameter<P>(
    profiles: &[P],
    diameter: f64,
    size: impl Fn(&P) -> f64,
    ratio: impl Fn(&P) -> f64,
) -> f64 {
    let first = &profiles[0];
    let last = &profiles[profiles.len() - 1];
    if !diameter.is_finite() || diameter <= size(first) {
        return ratio(first);
    }
    if diameter >= size(last) {
        return ratio(last);
    }
    let upper = profiles
        .iter()
        .position(|profile| size(profile) >= diameter)
        .unwrap_or(profiles.len() - 1);
    let (below, above) = (&profiles[upper - 1], &profiles[upper]);
    let t = (diameter - size(below)) / (size(above) - size(below));
    ratio(below) + t * (ratio(above) - ratio(below))
}

/// One ChituBox support profile's root boss: the DIAMETER of the shaft the
/// boss stands under (`middle_support.supportmiddlediameter`) and the DIAMETER
/// of the boss's foot on the raft (`support_general_para.diameterofbossbottom`).
///
/// The native bytes show the boss as a cone from the shaft's own section down
/// to the foot: `Chitubox Projects/cylinder-cube-sphere.chitubox` draws
/// type-4 flares 0.75 -> 1.10 (Heavy, shaft 1.5) and 0.30 -> 1.10 (shaft 0.6,
/// Light's) over 1.0 mm.
#[derive(Clone, Debug, PartialEq)]
pub struct RootBossProfile {
    name: String,
    shaft_diameter_mm: f64,
    boss_bottom_diameter_mm: f64,
}

impl RootBossProfile {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `middle_support.supportmiddlediameter`, a DIAMETER in mm.
    #[must_use]
    pub const fn shaft_diameter_mm(&self) -> f64 {
        self.shaft_diameter_mm
    }

    /// `support_general_para.diameterofbossbottom`, a DIAMETER in mm.
    #[must_use]
    pub const fn boss_bottom_diameter_mm(&self) -> f64 {
        self.boss_bottom_diameter_mm
    }

    /// The boss-foot-to-shaft ratio this profile draws: boss bottom diameter
    /// over shaft diameter.
    #[must_use]
    pub fn boss_factor(&self) -> f64 {
        self.boss_bottom_diameter_mm / self.shaft_diameter_mm
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum RootBossProfilesError {
    #[error("a root-boss profile table needs at least one profile")]
    Empty,
    #[error("profile {name}: diameters must be finite and greater than zero")]
    NonPositive { name: String },
    #[error(
        "profile {name}: the boss foot must be wider than the shaft it stands under \
         (a boss factor of 1 or less is no boss)"
    )]
    FootNotWiderThanShaft { name: String },
    #[error("profiles {first} and {second} share a shaft diameter; the table cannot be read by root size")]
    DuplicateShaftSize { first: String, second: String },
    #[error("a profile name must not be empty")]
    EmptyName,
}

/// The owner's ChituBox profiles' root-boss geometry, ordered by shaft size,
/// and the rule that reads a boss factor off them for a root of any size
/// (owner ruling 2026-09-30: the boss WIDTH is scaled to the support's own
/// root by his profiles' boss-to-shaft ratio, chosen by root size like the
/// ball ratio; NOT a fixed 2.2 mm boss under every root).
///
/// **The rule** is [`TipContactProfiles`]'s: the factor at a root is linear in
/// the root's DIAMETER between the two profiles whose shaft diameters bracket
/// it, and held at the nearest profile's own factor outside the owner's range.
#[derive(Clone, Debug, PartialEq)]
pub struct RootBossProfiles {
    profiles: Vec<RootBossProfile>,
}

impl RootBossProfiles {
    /// Build the table from `(name, shaft DIAMETER, boss bottom DIAMETER)`
    /// rows in any order; it is stored sorted by shaft diameter.
    ///
    /// # Errors
    ///
    /// An empty table, a non-finite or non-positive diameter, a boss foot no
    /// wider than its shaft, or two profiles at one shaft size.
    pub fn new<I, S>(rows: I) -> Result<Self, RootBossProfilesError>
    where
        I: IntoIterator<Item = (S, f64, f64)>,
        S: Into<String>,
    {
        let mut profiles = Vec::new();
        for (name, shaft, boss) in rows {
            let name = name.into();
            if name.trim().is_empty() {
                return Err(RootBossProfilesError::EmptyName);
            }
            if !(shaft.is_finite() && shaft > 0.0 && boss.is_finite() && boss > 0.0) {
                return Err(RootBossProfilesError::NonPositive { name });
            }
            if boss <= shaft {
                return Err(RootBossProfilesError::FootNotWiderThanShaft { name });
            }
            profiles.push(RootBossProfile {
                name,
                shaft_diameter_mm: shaft,
                boss_bottom_diameter_mm: boss,
            });
        }
        if profiles.is_empty() {
            return Err(RootBossProfilesError::Empty);
        }
        profiles.sort_by(|a, b| a.shaft_diameter_mm.total_cmp(&b.shaft_diameter_mm));
        if let Some(pair) = profiles
            .windows(2)
            .find(|pair| pair[1].shaft_diameter_mm <= pair[0].shaft_diameter_mm)
        {
            return Err(RootBossProfilesError::DuplicateShaftSize {
                first: pair[0].name.clone(),
                second: pair[1].name.clone(),
            });
        }
        Ok(Self { profiles })
    }

    /// The profiles, narrowest shaft first.
    #[must_use]
    pub fn profiles(&self) -> &[RootBossProfile] {
        &self.profiles
    }

    /// **The boss factor for a root of `root_radius_mm` RADIUS**: the boss
    /// foot's radius is this factor times the root radius. Always greater
    /// than one, because every profile's is.
    #[must_use]
    pub fn boss_factor(&self, root_radius_mm: f64) -> f64 {
        ratio_by_diameter(
            &self.profiles,
            2.0 * root_radius_mm,
            RootBossProfile::shaft_diameter_mm,
            RootBossProfile::boss_factor,
        )
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

    fn bosses() -> RootBossProfiles {
        RootBossProfiles::new([
            ("Light", 0.6, 2.2),
            ("Middle", 1.2, 2.2),
            ("Heavy", 1.5, 2.2),
            ("Ultralight", 0.5, 2.2),
        ])
        .expect("valid fixture")
    }

    #[test]
    fn boss_rows_are_sorted_by_shaft_size_and_a_root_at_a_profile_reads_its_own_ratio() {
        let t = bosses();
        let names: Vec<&str> = t.profiles().iter().map(RootBossProfile::name).collect();
        assert_eq!(names, ["Ultralight", "Light", "Middle", "Heavy"]);
        // RADIUS in: half of each profile's shaft DIAMETER. Each reads 2.2 / d.
        assert!((t.boss_factor(0.25) - 4.4).abs() < 1e-12);
        assert!((t.boss_factor(0.3) - 11.0 / 3.0).abs() < 1e-12);
        assert!((t.boss_factor(0.6) - 11.0 / 6.0).abs() < 1e-12);
        assert!((t.boss_factor(0.75) - 22.0 / 15.0).abs() < 1e-12);
    }

    #[test]
    fn between_shafts_the_boss_ratio_is_linear_in_root_diameter_and_held_outside() {
        let t = bosses();
        // Diameter 0.9: halfway from Light (11/3) to Middle (11/6), 2.75.
        assert!((t.boss_factor(0.45) - 2.75).abs() < 1e-12);
        assert!((t.boss_factor(0.05) - 4.4).abs() < 1e-12);
        assert!((t.boss_factor(2.0) - 22.0 / 15.0).abs() < 1e-12);
        assert!((t.boss_factor(f64::NAN) - 4.4).abs() < 1e-12);
    }

    #[test]
    fn a_boss_no_wider_than_its_shaft_or_a_repeated_shaft_size_is_refused() {
        assert!(matches!(
            RootBossProfiles::new([("flat", 2.2, 2.2)]),
            Err(RootBossProfilesError::FootNotWiderThanShaft { .. })
        ));
        assert!(matches!(
            RootBossProfiles::new([("a", 0.6, 2.2), ("b", 0.6, 2.0)]),
            Err(RootBossProfilesError::DuplicateShaftSize { .. })
        ));
        assert!(matches!(
            RootBossProfiles::new(Vec::<(&str, f64, f64)>::new()),
            Err(RootBossProfilesError::Empty)
        ));
    }
}
