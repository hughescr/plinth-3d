//! The support-profile geometry the owner prints with in ChituBox, as data.
//!
//! **UNITS TRAP**: ChituBox and the owner speak DIAMETER; the support engine
//! speaks RADIUS. Every field here is a DIAMETER, named as one, and the query
//! that takes an engine size ([`TipContactProfiles::ball_factor`]) takes a
//! RADIUS and says so. [`RootBossProfile::boss_factor`] is a pure ratio of two
//! diameters, the same for every root.

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

/// **The profile-reading rule** of the tip-contact table: the ratio at a size
/// of `diameter` mm DIAMETER is linear in diameter between the two profiles
/// whose own sizes bracket it, and the nearest end profile's own ratio outside
/// the owner's range (never extrapolated). A non-finite diameter reads the
/// smallest profile's ratio rather than a NaN.
///
/// `profiles` is non-empty and sorted by `size`, strictly increasing; the
/// table constructor guarantees it. (The root boss read its ratio by this
/// rule too for part of 2026-09-30; the owner's final ruling of that day
/// replaced it with his Heavy profile's one ratio, [`RootBossProfile`].)
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

/// The owner's four profiles' foot contact, `(name, middle_support.
/// supportmiddlediameter, bottom_support.supporttouchtaildiameter)` in mm
/// DIAMETER, transcribed from `docs/evidence/chitubox-support-profiles-
/// 2026-09-30.cfgx` (a test reads the export and compares).
pub const CHITUBOX_TAIL_TOUCH_ROWS: [(&str, f64, f64); 4] = [
    ("Light", 0.6, 0.35),
    ("Middle", 1.2, 0.5),
    ("Heavy", 1.5, 0.35),
    ("Ultralight", 0.5, 0.25),
];

/// The owner's foot-contact table ([`CHITUBOX_TAIL_TOUCH_ROWS`]).
///
/// # Panics
///
/// Never for the committed rows (a test builds the table).
#[must_use]
pub fn chitubox_tail_touch_profiles() -> &'static TailContactProfiles {
    static TABLE: std::sync::OnceLock<TailContactProfiles> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        TailContactProfiles::new(CHITUBOX_TAIL_TOUCH_ROWS)
            .expect("the owner's committed tail rows are valid")
    })
}

/// One ChituBox support profile's FOOT contact: the DIAMETER of the shaft the
/// profile draws (`middle_support.supportmiddlediameter`) and the DIAMETER of
/// the contact sphere its tail touches the model with
/// (`bottom_support.supporttouchtaildiameter`).
#[derive(Clone, Debug, PartialEq)]
pub struct TailContactProfile {
    name: String,
    shaft_diameter_mm: f64,
    tail_touch_diameter_mm: f64,
}

impl TailContactProfile {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `middle_support.supportmiddlediameter`, a DIAMETER in mm.
    #[must_use]
    pub const fn shaft_diameter_mm(&self) -> f64 {
        self.shaft_diameter_mm
    }

    /// `bottom_support.supporttouchtaildiameter`, a DIAMETER in mm.
    #[must_use]
    pub const fn tail_touch_diameter_mm(&self) -> f64 {
        self.tail_touch_diameter_mm
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum TailContactProfilesError {
    #[error("a tail-contact profile table needs at least one profile")]
    Empty,
    #[error("profile {name}: diameters must be finite and greater than zero")]
    NonPositive { name: String },
    #[error("profiles {first} and {second} share a shaft diameter; the table cannot be read by body size")]
    DuplicateShaftSize { first: String, second: String },
    #[error("a profile name must not be empty")]
    EmptyName,
}

/// **The owner's foot contact, read by body size** (round 6, 2026-10-14).
///
/// A post that stands on the model touches it with the sphere of its tail:
/// the owner's profile for a shaft of that size draws a contact of
/// `supporttouchtaildiameter`. The diameter at a body of any size is linear in
/// the body's DIAMETER between the two profiles whose own shaft diameters
/// bracket it, and held at the end profile's own value outside the table (the
/// same rule as [`TipContactProfiles`]; never extrapolated). The table is taken
/// as the owner wrote it: it is not monotone (Heavy draws a smaller tail
/// contact than Middle).
///
/// **UNITS.** Everything the owner wrote is a DIAMETER; the engine's neck is a
/// RADIUS. A contact sphere of diameter `d` has radius `d / 2`, and the neck it
/// stands on is `r_n = (d / 2) / f(r_n)` with `f` the tip table's ball factor
/// read at the neck's OWN diameter `2 r_n` ([`Self::neck_radius_mm`]).
#[derive(Clone, Debug, PartialEq)]
pub struct TailContactProfiles {
    profiles: Vec<TailContactProfile>,
}

impl TailContactProfiles {
    /// Build the table from `(name, shaft DIAMETER, tail touch DIAMETER)` rows
    /// in any order; it is stored sorted by shaft diameter.
    ///
    /// # Errors
    ///
    /// An empty table, a non-finite or non-positive diameter, an empty name,
    /// or two profiles at one shaft size.
    pub fn new<I, S>(rows: I) -> Result<Self, TailContactProfilesError>
    where
        I: IntoIterator<Item = (S, f64, f64)>,
        S: Into<String>,
    {
        let mut profiles = Vec::new();
        for (name, shaft, tail) in rows {
            let name = name.into();
            if name.trim().is_empty() {
                return Err(TailContactProfilesError::EmptyName);
            }
            if !(shaft.is_finite() && shaft > 0.0 && tail.is_finite() && tail > 0.0) {
                return Err(TailContactProfilesError::NonPositive { name });
            }
            profiles.push(TailContactProfile {
                name,
                shaft_diameter_mm: shaft,
                tail_touch_diameter_mm: tail,
            });
        }
        if profiles.is_empty() {
            return Err(TailContactProfilesError::Empty);
        }
        profiles.sort_by(|a, b| a.shaft_diameter_mm.total_cmp(&b.shaft_diameter_mm));
        if let Some(pair) = profiles
            .windows(2)
            .find(|pair| pair[1].shaft_diameter_mm <= pair[0].shaft_diameter_mm)
        {
            return Err(TailContactProfilesError::DuplicateShaftSize {
                first: pair[0].name.clone(),
                second: pair[1].name.clone(),
            });
        }
        Ok(Self { profiles })
    }

    /// The profiles, smallest shaft first.
    #[must_use]
    pub fn profiles(&self) -> &[TailContactProfile] {
        &self.profiles
    }

    /// The tail contact sphere's DIAMETER for a body of `body_diameter_mm`
    /// DIAMETER.
    #[must_use]
    pub fn tail_touch_diameter_mm(&self, body_diameter_mm: f64) -> f64 {
        ratio_by_diameter(
            &self.profiles,
            body_diameter_mm,
            TailContactProfile::shaft_diameter_mm,
            TailContactProfile::tail_touch_diameter_mm,
        )
    }

    /// **The neck RADIUS under the foot contact of a body of `body_radius_mm`
    /// RADIUS**: the radius `r` with `r * f(r) = d / 2`, `d` the tail touch
    /// DIAMETER read at the body's diameter and `f` the tip table's ball factor
    /// at the neck's own diameter. `r * f(r)` rises with `r` over the owner's
    /// table (its slope is `f + r f'`, and `f` never falls faster than `1 / r`
    /// does), so the root is found by bisection on the bracket the factor's
    /// own range gives.
    #[must_use]
    pub fn neck_radius_mm(&self, body_radius_mm: f64, tips: &TipContactProfiles) -> f64 {
        let half_touch = 0.5 * self.tail_touch_diameter_mm(2.0 * body_radius_mm);
        let (f_min, f_max) = tips
            .profiles()
            .iter()
            .map(TipContactProfile::ball_factor)
            .fold((f64::INFINITY, 0.0_f64), |(lo, hi), f| (lo.min(f), hi.max(f)));
        // r = half_touch / f: the widest neck at the smallest factor.
        let (mut low, mut high) = (half_touch / f_max, half_touch / f_min);
        for _ in 0..60 {
            let middle = 0.5 * (low + high);
            if middle * tips.ball_factor(middle) < half_touch {
                low = middle;
            } else {
                high = middle;
            }
        }
        0.5 * (low + high)
    }
}

/// One ChituBox support profile's root boss: the DIAMETER of the shaft the
/// boss stands under (`middle_support.supportmiddlediameter`) and the DIAMETER
/// of the boss's foot on the raft (`support_general_para.diameterofbossbottom`).
///
/// The native bytes show the boss as a cone from the shaft's own section down
/// to the foot: `Chitubox Projects/cylinder-cube-sphere.chitubox` draws
/// type-4 flares 0.75 -> 1.10 (Heavy, shaft 1.5) and 0.30 -> 1.10 (shaft 0.6,
/// Light's) over 1.0 mm.
///
/// **One profile, one ratio, every root** (Craig ruling 2026-09-30: Heavy
/// profile ratio). The registered boss is the owner's profile 2 "Heavy", and
/// every sweep plate root's foot is its root times [`Self::boss_factor`],
/// whatever the root's size. It is not a table read by root size: that
/// reading (linear between the bracketing profiles' ratios, 1.47 to 4.4) sat
/// above 2.2 / d for every root inside the owner's 0.5-1.5 mm shafts and
/// RAISED resin on every lane model.
#[derive(Clone, Debug, PartialEq)]
pub struct RootBossProfile {
    name: String,
    shaft_diameter_mm: f64,
    boss_bottom_diameter_mm: f64,
}

impl RootBossProfile {
    /// One profile's `(name, shaft DIAMETER, boss bottom DIAMETER)`.
    ///
    /// # Errors
    ///
    /// An empty name, a non-finite or non-positive diameter, or a boss foot
    /// no wider than its shaft (a factor of 1 or less is no boss).
    pub fn new(
        name: impl Into<String>,
        shaft_diameter_mm: f64,
        boss_bottom_diameter_mm: f64,
    ) -> Result<Self, RootBossProfileError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(RootBossProfileError::EmptyName);
        }
        if !(shaft_diameter_mm.is_finite()
            && shaft_diameter_mm > 0.0
            && boss_bottom_diameter_mm.is_finite()
            && boss_bottom_diameter_mm > 0.0)
        {
            return Err(RootBossProfileError::NonPositive { name });
        }
        if boss_bottom_diameter_mm <= shaft_diameter_mm {
            return Err(RootBossProfileError::FootNotWiderThanShaft { name });
        }
        Ok(Self {
            name,
            shaft_diameter_mm,
            boss_bottom_diameter_mm,
        })
    }

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

    /// **The boss-foot-to-root ratio this profile draws**: boss bottom
    /// diameter over shaft diameter. A ratio of two diameters, so it is also
    /// the ratio of the foot RADIUS to the root RADIUS. Always greater than
    /// one ([`Self::new`] refuses anything else).
    #[must_use]
    pub fn boss_factor(&self) -> f64 {
        self.boss_bottom_diameter_mm / self.shaft_diameter_mm
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum RootBossProfileError {
    #[error("profile {name}: diameters must be finite and greater than zero")]
    NonPositive { name: String },
    #[error(
        "profile {name}: the boss foot must be wider than the shaft it stands under \
         (a boss factor of 1 or less is no boss)"
    )]
    FootNotWiderThanShaft { name: String },
    #[error("a profile name must not be empty")]
    EmptyName,
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

    #[test]
    fn the_heavy_boss_reads_its_own_ratio_as_diameter_over_diameter() {
        let heavy = RootBossProfile::new("Heavy", 1.5, 2.2).expect("valid fixture");
        assert_eq!(heavy.name(), "Heavy");
        assert!((heavy.boss_factor() - 22.0 / 15.0).abs() < 1e-12);
    }

    fn tail_table() -> &'static TailContactProfiles {
        chitubox_tail_touch_profiles()
    }

    /// GOLDEN, all four tail rows: the owner's tail touch is a DIAMETER `d`; the
    /// sphere's radius is `d / 2` and the neck under it `r_n = (d / 2) / f(r_n)`,
    /// `f` read at the neck's own diameter. Not `d / f` (a diameter where a radius
    /// belongs: 0.2625 for the Light row) and not `d / (2 * 1.3333)` (the factor
    /// held where it is not: 0.1313).
    #[test]
    fn the_four_tail_rows_read_their_neck_radius_with_the_diameter_converted() {
        let tips = TipContactProfiles::new(CHITUBOX_TIP_CONTACT_ROWS_FOR_TESTS).unwrap();
        // (name, body DIAMETER = the row's shaft, tail touch DIAMETER, neck RADIUS)
        let golden = [
            ("Ultralight", 0.5, 0.25, 0.088_020),
            ("Light", 0.6, 0.35, 0.120_871),
            ("Middle", 1.2, 0.5, 0.200_000),
            ("Heavy", 1.5, 0.35, 0.120_871),
        ];
        for (name, body_diameter, touch, neck) in golden {
            let row = tail_table()
                .profiles()
                .iter()
                .find(|row| row.name() == name)
                .unwrap();
            assert!((row.shaft_diameter_mm() - body_diameter).abs() < 1e-12, "{name}");
            assert!((row.tail_touch_diameter_mm() - touch).abs() < 1e-12, "{name}");
            let read = tail_table().tail_touch_diameter_mm(body_diameter);
            assert!((read - touch).abs() < 1e-12, "{name}: tail touch {read}");
            let r_n = tail_table().neck_radius_mm(0.5 * body_diameter, &tips);
            assert!((r_n - neck).abs() < 2e-6, "{name}: neck {r_n} against {neck}");
            // The defining identity, in radii: the ball's radius is the neck times
            // the factor at the neck's own diameter, and is half the touch diameter.
            assert!(
                (r_n * tips.ball_factor(r_n) - 0.5 * touch).abs() < 1e-9,
                "{name}: ball radius {} is not half of {touch}",
                r_n * tips.ball_factor(r_n)
            );
            assert!((r_n - touch / tips.ball_factor(r_n)).abs() > 0.05, "{name}: d / f is the wrong conversion");
        }
    }

    #[test]
    fn the_tail_touch_is_linear_in_body_diameter_and_held_outside_the_table() {
        let t = tail_table();
        // Halfway between Light (0.6, 0.35) and Middle (1.2, 0.50).
        assert!((t.tail_touch_diameter_mm(0.9) - 0.425).abs() < 1e-12);
        assert!((t.tail_touch_diameter_mm(0.1) - 0.25).abs() < 1e-12);
        assert!((t.tail_touch_diameter_mm(5.0) - 0.35).abs() < 1e-12);
        assert!((t.tail_touch_diameter_mm(f64::NAN) - 0.25).abs() < 1e-12);
    }

    #[test]
    fn a_tail_table_with_no_rows_a_bad_size_or_a_repeated_shaft_is_refused() {
        assert!(matches!(
            TailContactProfiles::new(Vec::<(&str, f64, f64)>::new()),
            Err(TailContactProfilesError::Empty)
        ));
        assert!(matches!(
            TailContactProfiles::new([("a", 0.0, 0.3)]),
            Err(TailContactProfilesError::NonPositive { .. })
        ));
        assert!(matches!(
            TailContactProfiles::new([("a", 0.6, 0.3), ("b", 0.6, 0.4)]),
            Err(TailContactProfilesError::DuplicateShaftSize { .. })
        ));
        assert!(matches!(
            TailContactProfiles::new([(" ", 0.6, 0.3)]),
            Err(TailContactProfilesError::EmptyName)
        ));
    }

    /// The rows are the owner's, read off the committed export by name, so a
    /// re-export with other numbers turns this red until the rows are re-read.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "the rows transcribe the export's decimal literals exactly; equality is the claim"
    )]
    fn the_tail_rows_equal_the_committed_profile_export() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("docs/evidence/chitubox-support-profiles-2026-09-30.cfgx");
        let raw = std::fs::read_to_string(&path).expect("the committed export is readable");
        let export: serde_json::Value = serde_json::from_str(&raw).expect("the export is JSON");
        let profiles = export["support"].as_array().expect("a support array");
        assert_eq!(profiles.len(), CHITUBOX_TAIL_TOUCH_ROWS.len());
        for profile in profiles {
            let name = profile["name"].as_str().expect("a name");
            let row = tail_table()
                .profiles()
                .iter()
                .find(|row| row.name() == name)
                .unwrap_or_else(|| panic!("{name} is missing from the tail table"));
            let field = |section: &str, key: &str| {
                profile[section][key]["currentvalue"]
                    .as_f64()
                    .unwrap_or_else(|| panic!("{name} has no {section}.{key}"))
            };
            assert_eq!(
                row.shaft_diameter_mm(),
                field("middle_support", "supportmiddlediameter"),
                "{name}: supportmiddlediameter"
            );
            assert_eq!(
                row.tail_touch_diameter_mm(),
                field("bottom_support", "supporttouchtaildiameter"),
                "{name}: supporttouchtaildiameter"
            );
        }
    }

    const CHITUBOX_TIP_CONTACT_ROWS_FOR_TESTS: [(&str, f64, f64); 4] = [
        ("Light", 0.2, 0.3),
        ("Middle", 0.4, 0.5),
        ("Heavy", 0.6, 0.8),
        ("Ultralight", 0.15, 0.2),
    ];

    #[test]
    fn a_boss_no_wider_than_its_shaft_a_non_positive_size_or_no_name_is_refused() {
        assert!(matches!(
            RootBossProfile::new("flat", 2.2, 2.2),
            Err(RootBossProfileError::FootNotWiderThanShaft { .. })
        ));
        assert!(matches!(
            RootBossProfile::new("nan", f64::NAN, 2.2),
            Err(RootBossProfileError::NonPositive { .. })
        ));
        assert!(matches!(
            RootBossProfile::new("zero", 0.0, 2.2),
            Err(RootBossProfileError::NonPositive { .. })
        ));
        assert!(matches!(
            RootBossProfile::new(" ", 1.5, 2.2),
            Err(RootBossProfileError::EmptyName)
        ));
    }
}
