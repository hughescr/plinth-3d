//! **THE REAL CONTACT, and the ball grown toward the load's need** (Craig
//! 2026-10-12: "cut using the actual model mesh"; orchestrator ruling of the
//! same date: keep the seat depth, area governs, grow the ball; round 5,
//! 2026-10-14: the growth is bounded by the load and by the feature).
//!
//! # What changed, and why
//!
//! The emitter cuts a tip ball to the model MESH (`plinth_emit`'s seat trim):
//! the file keeps `ball ∖ model`, so the contact is exactly the model surface
//! inside the drawn ball. Until 2026-10-12 a plane through one seat point cut
//! the ball too, and the engine REPORTED the disc that plane left,
//! `π (R² − d²)`, as the contact. On a curved or tilted underside the plane is
//! not the surface: the file got 32 % of that disc on Bat_Cat_Owl and 30 % on
//! Deinonychus (`plinth-evidence/.../quality-2026-10-10/joint/
//! TIP-TRUNCATION.md`). With the plane gone, a convex underside still gives
//! less than plan: the surface curves away from the ball's pole.
//!
//! The first growth (2026-10-12) targeted the profile ball's disc and stopped
//! only at zero gain. Owner field report on Bat_Cat_Owl: the tip balls are
//! "much bigger than the support column or the feature that is being
//! supported" (`BALLS-FLARE.md`: six balls over 1.5 times their shaft, the
//! worst 1.067 mm on a 0.36 mm shaft under a 0.21 mm edge; Deinonychus 72). A
//! bigger ball always catches a little more flank, so a short tip on a thin
//! edge ran to the ceiling. The rules below are the fix at that cause.
//!
//! # The rule
//!
//! * **The target** is what the tip's LOAD needs, `π r_load²` with `r_load`
//!   the area law before its minimums and clamps ([`TipBounds::need_mm2`]),
//!   and never more than the disc the seat law sized for the profile ball:
//!   the UNGROWN ball's section in the seat plane
//!   ([`plinth_support::TipContactDisc::radius_mm`]), area `π a²`. A tip at
//!   the 0.05 mm minimum needs far less than its ball gives and is not grown.
//! * **The real contact** is the area of model surface inside the DRAWN ball,
//!   measured exactly ([`crate::footprint_normal::triangle_disc_area`]) over the
//!   footprint the seat normal is averaged on: the admitted (down-facing) faces
//!   connected to the seat triangle through edges inside the ball
//!   ([`crate::footprint_normal::footprint_normals`]). The far side of a thin
//!   wall is not admitted and not connected, exactly the piece the emitter drops.
//! * **Short of the target, the ball grows.** The seat depth stays (the 0.1 mm
//!   depth limit of 2026-08-30 is kept): the grown ball keeps the ungrown
//!   ball's deepest point along the seat normal and contains it
//!   ([`plinth_support::SweepArm::contact_ball_radius_mm`]), so it reaches no
//!   deeper into a flat face and takes more of a curved one.
//! * **A step must pay for itself.** A face that is flat at the seat depth
//!   `δ` gains `2 π δ` of area per unit of ball radius. A step that gains
//!   less than that wraps the flanks of the feature and buys no contact a
//!   flat face would not give: it is not drawn ([`ShortReason::FlankOnly`]).
//!   The doc of the first growth named this stop ("the patch ends: a knife
//!   edge, a thin feature") and did not test it.
//! * **Every ball has a ceiling** (emit and warn, never refuse):
//!   1. the owner's growth-for-attachment ceiling (the profile ball of a tip of
//!      `TIP_RADIUS_MAX_GROWN_SCAR_MM`, 0.400 mm radius; a ball the load law
//!      already drew larger is not grown);
//!   2. the top of the owner's ball-to-shaft ratio range (the largest ball
//!      factor in his profile table, 1.5) times the shaft radius at the seat;
//!   3. the FEATURE: a contact disc no wider than the narrowest cured chord of
//!      the model section through the seat at the strike layer
//!      ([`crate::feature_chord`]). The cured joint is only as strong as its
//!      narrowest section; a ball wider than the feature wraps the edge, the
//!      break moves into the model and the peel area only goes up;
//!   4. the shaft's own carry: a grown ball must print from its own shaft
//!      (joint review P1, 2026-10-12): the only part of a ball that starts a new
//!      layer island is its lowest point, and it prints when it lies inside the
//!      shaft the ball stands on ([`plinth_support::carried_ball_radii_mm`]).
//!
//!   A tip a ceiling stops short of its target is WARNED with the reason and
//!   the shortfall ([`ShortReason`]). Where the feature is narrower than the
//!   contact its load needs ([`ShortReason::FeatureWidth`]) the second contact
//!   along the feature belongs to the placement, which has run by the time the
//!   field is final; the warning is how it is reported.
//!
//! The profile balls themselves (factor × tip radius, a table of 1.25 to 1.5)
//! are the owner's ChituBox balls for the tip size and are drawn as they are:
//! the lowering ignores a requested ball at or below them. A profile ball that
//! is wider than its feature is counted ([`ContactOutcome::ungrown_over_feature`]).
//!
//! # Tolerance and the search
//!
//! "Reaches the target" is to within one pixel of area (the printer's own area
//! resolution, `pitch²`), not a new number. The radius is found by the secant
//! rule on `area(R)` (a proportional first step; never more than doubling in
//! a round, a search step), each round ONE batched
//! [`plinth_support::tip_contact_discs`] call, ONE
//! [`crate::footprint_normal::footprint_pairs`] native query and one parallel
//! [`crate::footprint_normal::footprint_normals`] pass over the tips still
//! short. A step the flat-gain rule refuses is halved, down to one pixel of
//! radius. [`MAX_ROUNDS`] bounds the search (a disclosed Placeholder that
//! prunes the search; a tip it stops is named in the warnings, never refused).

use plinth_support::SweepArm;

use crate::footprint_normal::{
    footprint_normals, footprint_pairs, FootprintBall, MeasureEnv, TipSite,
};

/// Rounds of the radius search. A Placeholder that only bounds the search: a
/// tip still short of its target when it runs out is warned with
/// [`ShortReason::SearchRounds`], never refused. The secant rule reaches one
/// pixel of area in two to four rounds on the measured models.
pub const MAX_ROUNDS: usize = 8;

/// Which of the round-5 rules are on. [`Self::REGISTERED`] is production;
/// [`Self::PLAN_TARGET`] is the growth of 2026-10-12 (target the profile
/// ball's disc, stop at zero gain, no ceiling but the registry's and the
/// shaft's carry), kept for sabotage and comparison only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrowthRule {
    /// Target `min(plan, load need)` rather than the plan.
    pub load_need_target: bool,
    /// Ceiling at the feature's narrowest cured chord.
    pub feature_ceiling: bool,
    /// Ceiling at the top of the owner's ball-to-shaft ratio range.
    pub shaft_ratio_ceiling: bool,
    /// A step that gains less than a flat face would is not drawn.
    pub flat_gain_stop: bool,
}

impl GrowthRule {
    /// Production.
    pub const REGISTERED: Self = Self {
        load_need_target: true,
        feature_ceiling: true,
        shaft_ratio_ceiling: true,
        flat_gain_stop: true,
    };
    /// The growth as it was on 2026-10-12. **Sabotage and comparison only**;
    /// nothing in the pipeline sets it.
    pub const PLAN_TARGET: Self = Self {
        load_need_target: false,
        feature_ceiling: false,
        shaft_ratio_ceiling: false,
        flat_gain_stop: false,
    };
}

/// What bounds the growth of every ball on the field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrowthLimits {
    /// The largest ball growth for attachment may draw, mm radius (the
    /// registry's: the profile ball of a 0.400 mm-radius tip).
    pub ball_radius_mm: f64,
    /// The top of the owner's ball-to-shaft ratio range: the largest ball
    /// factor of his profile table.
    pub max_ball_ratio: f64,
    /// Which rules are on.
    pub rule: GrowthRule,
}

/// What the field knows about one tip beyond its arm: its load and its
/// feature. `None` for either reads as "unknown": the plan is the target and
/// the feature is no ceiling.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TipBounds {
    /// The contact area the tip's load needs, mm²: the down-sweep's carried
    /// load at the contact over `X_ALLOWED`, before the radius clamps.
    pub need_mm2: Option<f64>,
    /// The narrowest cured chord of the model section through the seat at the
    /// strike layer, mm ([`crate::feature_chord`]).
    pub feature_chord_mm: Option<f64>,
}

/// Why a tip's real contact is still short of its target after the growth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortReason {
    /// The ball is at the growth-for-attachment ceiling, or the load law drew
    /// it larger already.
    BallLimit,
    /// The ball grew and found no more model surface: the patch ends.
    NoMoreSurface,
    /// A larger ball's lowest point would stand outside the shaft it stands
    /// on and print as a new island.
    Overhang,
    /// [`MAX_ROUNDS`] ran out.
    SearchRounds,
    /// The ball is at the top of the owner's ball-to-shaft ratio range.
    ShaftRatio,
    /// The feature is narrower than the contact the load needs.
    FeatureWidth,
    /// A larger ball gained less contact than a flat face would: it only
    /// wraps the flanks.
    FlankOnly,
}

impl ShortReason {
    /// The reason in words, for the log.
    #[must_use]
    pub const fn words(self) -> &'static str {
        match self {
            Self::BallLimit => {
                "the ball is at the owner's growth-for-attachment ceiling (a 0.4 mm-radius tip's \
                 ball), or the load law already drew it larger"
            }
            Self::NoMoreSurface => "a larger ball found no more model surface (the patch ends)",
            Self::Overhang => {
                "a larger ball's lowest point would stand outside its shaft and print as a new \
                 island"
            }
            Self::SearchRounds => "the radius search ran out of rounds",
            Self::ShaftRatio => {
                "the ball is at the top of the owner's ball-to-shaft ratio range (a larger ball \
                 would be a lump on its shaft)"
            }
            Self::FeatureWidth => {
                "feature narrower than its load's contact (a ball wider than the feature only \
                 wraps its flanks; the second contact along it is not placed at this stage)"
            }
            Self::FlankOnly => {
                "a larger ball gained less contact than a flat face would (it only wraps the \
                 flanks of the feature)"
            }
        }
    }
}

/// One tip still short of its target.
#[derive(Clone, Debug, PartialEq)]
pub struct ContactShort {
    /// The strut index.
    pub strut: usize,
    /// The real contact, mm², at the drawn ball.
    pub real_mm2: f64,
    /// The target contact, mm²: the load's need, no more than the plan.
    pub planned_mm2: f64,
    /// The disc the profile ball plans, mm².
    pub profile_disc_mm2: f64,
    /// The drawn ball's radius, and the ungrown one's, mm.
    pub ball_radius_mm: f64,
    pub planned_ball_radius_mm: f64,
    pub reason: ShortReason,
}

/// One profile ball drawn past a ceiling it cannot be brought under (the
/// lowering never draws a ball below the owner's profile ball).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProfileOver {
    /// The strut index.
    pub strut: usize,
    /// The profile ball's radius, mm.
    pub ball_radius_mm: f64,
    /// The width of the contact disc that ball plans, mm.
    pub disc_width_mm: f64,
    /// The feature's narrowest cured chord at the strike layer, mm.
    pub chord_mm: f64,
}

/// What the contact measurement and growth did on one field.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContactOutcome {
    /// Whether it ran (it needs the footprint sites and the registry).
    pub enabled: bool,
    /// Seated tips measured.
    pub tips_measured: usize,
    /// Sum over those tips of the target contact, mm² (the load's need, no
    /// more than the plan).
    pub planned_mm2: f64,
    /// Sum over those tips of the disc the profile ball plans, mm².
    pub profile_disc_mm2: f64,
    /// Sum of the real contact at the ungrown ball, and at the drawn ball.
    pub real_ungrown_mm2: f64,
    pub real_drawn_mm2: f64,
    /// Tips short of the target at the ungrown ball.
    pub short_ungrown: usize,
    /// Tips whose ball was grown, and the largest drawn/ungrown radius ratio.
    pub grown: usize,
    pub max_growth_ratio: f64,
    /// Rounds the search ran.
    pub rounds: usize,
    /// The largest ball the registry draws, mm (the growth limit).
    pub limit_ball_radius_mm: f64,
    /// Tips short of the target whose growth is bounded below that limit
    /// because a larger ball's lowest point would leave its shaft (the path
    /// hits of the carried bound), and of those, the ones it stopped short.
    pub carried_bound: usize,
    pub stopped_by_overhang: usize,
    /// Tips short of the target whose ceiling is the ball-to-shaft ratio, and
    /// whose ceiling is the feature's chord (the path hits of those rules).
    pub ratio_bound: usize,
    pub feature_bound: usize,
    /// Profile balls (never grown, never shrunk) already past a ceiling: wider
    /// than their feature's chord, and past the ball-to-shaft ratio.
    pub ungrown_over_feature: usize,
    pub ungrown_over_ratio: usize,
    /// The profile balls counted in `ungrown_over_feature`, one row each.
    pub over_feature: Vec<ProfileOver>,
    /// Tips still short after the growth, with why.
    pub short: Vec<ContactShort>,
    /// Per strut: `(real mm², target mm²)` at the drawn ball, `None` for a
    /// tip that was not measured.
    pub per_strut: Vec<Option<(f64, f64)>>,
}

/// One tip's measurement at one ball.
#[derive(Clone, Copy)]
pub(crate) struct Measured {
    /// The disc the UNGROWN ball plans, mm².
    pub(crate) planned_mm2: f64,
    /// The real contact at the drawn ball, mm².
    pub(crate) real_mm2: f64,
    /// The drawn ball's radius, mm.
    drawn_radius_mm: f64,
    /// The ungrown ball's radius and its planned disc's radius, mm.
    planned_ball_mm: f64,
    planned_disc_mm: f64,
}

/// The real contact of every row of `arms` at its drawn ball: one batched
/// disc call, one pairs query, one parallel footprint pass. `None` where the
/// arm has no disc or no site.
pub(crate) fn measure(
    env: &MeasureEnv<'_>,
    arms: &[SweepArm],
    sites: &[Option<TipSite>],
) -> Vec<Option<Measured>> {
    let Ok(discs) = plinth_support::tip_contact_discs(arms, env.layer_plan) else {
        return vec![None; arms.len()];
    };
    let mut rows: Vec<usize> = Vec::with_capacity(arms.len());
    let mut centres: Vec<[f64; 3]> = Vec::with_capacity(arms.len());
    let mut radii: Vec<f64> = Vec::with_capacity(arms.len());
    let mut balls: Vec<FootprintBall> = Vec::with_capacity(arms.len());
    for (row, (disc, site)) in discs.iter().zip(sites).enumerate() {
        let (Some(disc), Some(site)) = (disc, site) else {
            continue;
        };
        rows.push(row);
        centres.push(disc.ball_centre_mm);
        radii.push(disc.ball_radius_mm);
        balls.push(FootprintBall {
            centre_mm: disc.ball_centre_mm,
            radius_mm: disc.ball_radius_mm,
            seat_face: site.face,
            facet_normal: site.facet_normal,
            max_normal_z: site.max_normal_z,
            min_normal_z: f64::NEG_INFINITY,
        });
    }
    let mut out = vec![None; arms.len()];
    let Some(pairs) = footprint_pairs(env.mesh, env.surface, &centres, &radii) else {
        return out;
    };
    let found = footprint_normals(env.mesh, &pairs, &balls);
    for ((&row, answer), &radius) in rows.iter().zip(&found).zip(&radii) {
        let Some(disc) = discs[row] else { continue };
        // No connected, admitted area in the ball is no contact.
        let real = answer.map_or(0.0, |found| found.contact_area_mm2);
        out[row] = Some(Measured {
            planned_mm2: std::f64::consts::PI * disc.radius_mm * disc.radius_mm,
            real_mm2: real,
            drawn_radius_mm: radius,
            planned_ball_mm: disc.planned_ball_radius_mm,
            planned_disc_mm: disc.radius_mm,
        });
    }
    out
}

/// **The seat depth of the planned ball**, mm: how far the ungrown ball's
/// pole stands past the seat plane, `R₀ − √(R₀² − a₀²)` for the planned disc
/// of radius `a₀`. The depth every grown ball keeps.
fn seat_depth_mm(ball_radius_mm: f64, disc_radius_mm: f64) -> f64 {
    let under = (ball_radius_mm * ball_radius_mm - disc_radius_mm * disc_radius_mm).max(0.0);
    (ball_radius_mm - under.sqrt()).max(0.0)
}

/// **The largest ball whose contact disc is no wider than `chord_mm`**, at the
/// kept seat depth `delta_mm`: a ball of radius `R` that keeps its pole at
/// depth `δ` cuts the seat plane in a disc of radius `a`, `a² = 2Rδ − δ²`, so
/// `R = (a² + δ²) / 2δ` for `a = chord / 2`. Unbounded where there is no
/// depth (no disc at all).
fn ball_radius_for_chord_mm(chord_mm: f64, delta_mm: f64) -> f64 {
    if !(delta_mm > 1e-9 && chord_mm.is_finite() && chord_mm > 0.0) {
        return f64::INFINITY;
    }
    let half = 0.5 * chord_mm;
    (half * half + delta_mm * delta_mm) / (2.0 * delta_mm)
}

/// **Measure every seated tip's real contact and grow the short ones' balls**
/// (module doc). `arms` are the field exactly as the lowering gets it, with
/// NO grown ball (`contact_ball_radius_mm: None`), in strut order; `sites` the
/// footprint sites (the seat triangles, re-derived for slid tips); `bounds`
/// each tip's load and feature, in strut order (a missing entry is unknown).
/// Returns the outcome; `outcome.per_strut` and the radii to draw are in strut
/// order: `grown[i]` is the radius strut `i`'s ball is drawn at, `None` for
/// the ungrown ball.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "one measurement and one secant search over the tips still short, in batched rounds"
)]
pub fn grow(
    env: &MeasureEnv<'_>,
    arms: &[SweepArm],
    sites: &[Option<TipSite>],
    bounds: &[TipBounds],
    limits: GrowthLimits,
    area_tolerance_mm2: f64,
) -> (ContactOutcome, Vec<Option<f64>>) {
    let rule = limits.rule;
    let limit_ball_radius_mm = limits.ball_radius_mm;
    let mut outcome = ContactOutcome {
        enabled: true,
        limit_ball_radius_mm,
        per_strut: vec![None; arms.len()],
        ..ContactOutcome::default()
    };
    let mut grown: Vec<Option<f64>> = vec![None; arms.len()];
    let tips: Vec<usize> = (0..arms.len().min(sites.len()))
        .filter(|&index| sites[index].is_some() && arms[index].seats_into_model)
        .collect();
    if tips.is_empty() {
        return (outcome, grown);
    }
    let working: Vec<SweepArm> = tips
        .iter()
        .map(|&index| SweepArm {
            contact_ball_radius_mm: None,
            ..arms[index].clone()
        })
        .collect();
    let tip_sites: Vec<Option<TipSite>> = tips.iter().map(|&index| sites[index]).collect();
    let first = measure(env, &working, &tip_sites);
    // Per tip: target, ungrown radius, and the search state.
    struct Search {
        tip: usize,
        /// The contact the load needs, no more than the profile disc.
        target: f64,
        /// The profile disc, mm².
        profile_disc: f64,
        ungrown_radius: f64,
        radius: f64,
        real: f64,
        previous: Option<(f64, f64)>,
        reason: Option<ShortReason>,
        /// The kept seat depth, mm: a flat face gains `2 π δ` per mm of radius.
        depth: f64,
        /// The largest step the next proposal may take, after a refused one.
        step_cap: f64,
        /// This tip's growth limit, and why it is that (the lowest of the
        /// registry's, the shaft's carry, the ratio's and the feature's).
        limit: f64,
        limit_reason: ShortReason,
    }
    impl Search {
        /// Why a tip stopped at its limit.
        const fn at_limit(&self) -> ShortReason {
            self.limit_reason
        }
    }
    let mut searches: Vec<Search> = Vec::new();
    for (position, answer) in first.iter().enumerate() {
        let Some(measured) = *answer else {
            continue;
        };
        let strut = tips[position];
        let tip_bounds = bounds.get(strut).copied().unwrap_or_default();
        let plan = measured.planned_mm2;
        let need = tip_bounds.need_mm2.filter(|need| need.is_finite() && *need >= 0.0);
        let target = match need {
            Some(need) if rule.load_need_target => plan.min(need),
            _ => plan,
        };
        let depth = seat_depth_mm(measured.planned_ball_mm, measured.planned_disc_mm);
        // The ceilings that are the rules of this round. A profile ball that
        // stands past one is counted and left (the lowering draws nothing
        // below the owner's ball for the tip size).
        let ratio_cap = if rule.shaft_ratio_ceiling {
            limits.max_ball_ratio * arms[strut].tip_radius_mm
        } else {
            f64::INFINITY
        };
        let feature_cap = match tip_bounds.feature_chord_mm {
            Some(chord) if rule.feature_ceiling => ball_radius_for_chord_mm(chord, depth),
            _ => f64::INFINITY,
        };
        if measured.planned_ball_mm > ratio_cap * (1.0 + 1e-9) {
            outcome.ungrown_over_ratio += 1;
        }
        if measured.planned_ball_mm > feature_cap * (1.0 + 1e-9) {
            outcome.ungrown_over_feature += 1;
            if let Some(chord) = tip_bounds.feature_chord_mm {
                outcome.over_feature.push(ProfileOver {
                    strut,
                    ball_radius_mm: measured.planned_ball_mm,
                    disc_width_mm: 2.0 * measured.planned_disc_mm,
                    chord_mm: chord,
                });
            }
        }
        outcome.tips_measured += 1;
        outcome.planned_mm2 += target;
        outcome.profile_disc_mm2 += plan;
        outcome.real_ungrown_mm2 += measured.real_mm2;
        outcome.per_strut[strut] = Some((measured.real_mm2, target));
        if measured.real_mm2 + area_tolerance_mm2 >= target {
            outcome.real_drawn_mm2 += measured.real_mm2;
            continue;
        }
        outcome.short_ungrown += 1;
        let mut limit = limit_ball_radius_mm;
        let mut limit_reason = ShortReason::BallLimit;
        for (cap, reason) in [
            (feature_cap, ShortReason::FeatureWidth),
            (ratio_cap, ShortReason::ShaftRatio),
        ] {
            if cap < limit {
                limit = cap;
                limit_reason = reason;
            }
        }
        searches.push(Search {
            tip: position,
            target,
            profile_disc: plan,
            ungrown_radius: measured.drawn_radius_mm,
            radius: measured.drawn_radius_mm,
            real: measured.real_mm2,
            previous: None,
            reason: None,
            depth,
            step_cap: f64::INFINITY,
            limit: limit.max(measured.drawn_radius_mm),
            limit_reason,
        });
    }
    // THE CARRIED BOUND (joint review P1): the largest ball each short tip's
    // shaft carries, in one call. A tip it cannot be read for is not grown
    // (the side that keeps the file printable).
    let searched: Vec<SweepArm> = searches
        .iter()
        .map(|search| working[search.tip].clone())
        .collect();
    let ceilings: Vec<f64> = searches.iter().map(|search| search.limit).collect();
    let carried = plinth_support::carried_ball_radii_mm(&searched, &ceilings, env.layer_plan)
        .unwrap_or_else(|_| vec![None; searched.len()]);
    for (search, carried) in searches.iter_mut().zip(carried) {
        let bound = carried.unwrap_or(search.ungrown_radius);
        if bound < search.limit {
            search.limit = bound.max(search.ungrown_radius);
            search.limit_reason = ShortReason::Overhang;
            outcome.carried_bound += 1;
        }
    }
    // The pixel of radius under which a refused step is not halved again.
    let step_floor = area_tolerance_mm2.max(0.0).sqrt();
    let mut active: Vec<usize> = (0..searches.len()).collect();
    while !active.is_empty() && outcome.rounds < MAX_ROUNDS {
        outcome.rounds += 1;
        // The next radius of every active tip: the secant through the last two
        // measurements, a proportional step from the first. Never smaller than
        // the current one, never past the limit.
        let mut proposed: Vec<(usize, f64)> = Vec::with_capacity(active.len());
        for &slot in &active {
            let search = &mut searches[slot];
            if search.radius >= search.limit {
                search.reason = Some(search.at_limit());
                continue;
            }
            let next = match search.previous {
                Some((radius, real)) if search.real > real && search.radius > radius => {
                    search.radius
                        + (search.target - search.real) * (search.radius - radius)
                            / (search.real - real)
                }
                // No surface yet: double the ball (a search step only).
                _ if search.real <= 0.0 => 2.0 * search.radius,
                _ => search.radius * search.target / search.real,
            };
            // At most double per round: a tip with almost no contact would
            // otherwise jump straight to the limit, and a patch that ends
            // between the two would be misnamed a limit. A search step only.
            let next = next
                .min(2.0 * search.radius)
                .min(search.radius + search.step_cap)
                .max(search.radius)
                .min(search.limit);
            if !(next.is_finite() && next > search.radius) {
                search.reason = Some(ShortReason::NoMoreSurface);
                continue;
            }
            proposed.push((slot, next));
        }
        if proposed.is_empty() {
            break;
        }
        let round_arms: Vec<SweepArm> = proposed
            .iter()
            .map(|&(slot, radius)| SweepArm {
                contact_ball_radius_mm: Some(radius),
                ..working[searches[slot].tip].clone()
            })
            .collect();
        let round_sites: Vec<Option<TipSite>> = proposed
            .iter()
            .map(|&(slot, _)| tip_sites[searches[slot].tip])
            .collect();
        let answers = measure(env, &round_arms, &round_sites);
        let mut still: Vec<usize> = Vec::with_capacity(proposed.len());
        for (&(slot, radius), answer) in proposed.iter().zip(&answers) {
            let search = &mut searches[slot];
            let real = answer.map_or(search.real, |measured| measured.real_mm2);
            // A larger ball that took no more surface is not drawn: the
            // patch ends, and the ball stays at the last radius that gained.
            if real <= search.real {
                search.reason = Some(if radius >= search.limit {
                    search.at_limit()
                } else {
                    ShortReason::NoMoreSurface
                });
                continue;
            }
            // A step must pay for itself: a flat face at the kept seat depth
            // gains 2 pi delta per mm of radius. Less than that is flank. A
            // refused step is halved and tried again, down to one pixel.
            let flat_gain = 2.0 * std::f64::consts::PI * search.depth * (radius - search.radius);
            if rule.flat_gain_stop && real - search.real < flat_gain {
                let half = 0.5 * (radius - search.radius);
                if half > step_floor {
                    search.step_cap = half;
                    still.push(slot);
                } else {
                    search.reason = Some(ShortReason::FlankOnly);
                }
                continue;
            }
            search.previous = Some((search.radius, search.real));
            search.radius = radius;
            search.real = real;
            if search.real + area_tolerance_mm2 >= search.target {
                continue;
            }
            still.push(slot);
        }
        active = still;
    }
    for search in &searches {
        let strut = tips[search.tip];
        outcome.real_drawn_mm2 += search.real;
        outcome.per_strut[strut] = Some((search.real, search.target));
        // A ball that found no area at all (its seat triangle never entered
        // it) is not grown: the growth bought nothing.
        let bought = search.radius > search.ungrown_radius
            && search.real > first[search.tip].map_or(0.0, |measured| measured.real_mm2);
        if bought {
            grown[strut] = Some(search.radius);
            outcome.grown += 1;
            outcome.max_growth_ratio = outcome
                .max_growth_ratio
                .max(search.radius / search.ungrown_radius);
        }
        if search.real + area_tolerance_mm2 < search.target {
            // A tip that never got a round (its ceiling is its own ball) is
            // named by that ceiling.
            let reason = search.reason.unwrap_or(if search.radius >= search.limit {
                search.at_limit()
            } else {
                ShortReason::SearchRounds
            });
            match reason {
                ShortReason::Overhang => outcome.stopped_by_overhang += 1,
                ShortReason::ShaftRatio => outcome.ratio_bound += 1,
                ShortReason::FeatureWidth => outcome.feature_bound += 1,
                _ => {}
            }
            outcome.short.push(ContactShort {
                strut,
                real_mm2: search.real,
                planned_mm2: search.target,
                profile_disc_mm2: search.profile_disc,
                ball_radius_mm: if bought { search.radius } else { search.ungrown_radius },
                planned_ball_radius_mm: search.ungrown_radius,
                reason,
            });
        }
    }
    (outcome, grown)
}
