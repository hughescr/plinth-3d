//! **THE REAL CONTACT, and the ball grown until it reaches plan** (Craig
//! 2026-10-12: "cut using the actual model mesh"; orchestrator ruling of the
//! same date: keep the seat depth, area governs, grow the ball).
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
//! # The rule
//!
//! * **The plan** is the disc the seat law sized: the UNGROWN ball's section in
//!   the seat plane ([`plinth_support::TipContactDisc::radius_mm`]), area
//!   `π a²`.
//! * **The real contact** is the area of model surface inside the DRAWN ball,
//!   measured exactly ([`crate::footprint_normal::triangle_disc_area`]) over the
//!   footprint the seat normal is averaged on: the admitted (down-facing) faces
//!   connected to the seat triangle through edges inside the ball
//!   ([`crate::footprint_normal::footprint_normals`]). The far side of a thin
//!   wall is not admitted and not connected, exactly the piece the emitter drops.
//! * **Short of plan, the ball grows.** The seat depth stays (the 0.1 mm depth
//!   limit of 2026-08-30 is kept): the grown ball keeps the ungrown ball's
//!   deepest point along the seat normal and contains it
//!   ([`plinth_support::SweepArm::contact_ball_radius_mm`]), so it reaches no
//!   deeper into a flat face and takes more of a curved one. Every ball that
//!   grows contains the one before it, so the real contact never falls as it
//!   grows.
//! * **A limit stops it, with a WARNING** (emit and warn, never refuse): the
//!   owner's growth-for-attachment ceiling (the profile ball of a tip of
//!   `TIP_RADIUS_MAX_GROWN_SCAR_MM`, 0.400 mm radius: the largest scar he
//!   accepts for a tip enlarged beyond what the load law asked; a ball the
//!   load law already drew larger is not grown), or a ball whose growth found
//!   no more surface (the patch ends: a knife edge, a thin feature).
//! * **A grown ball must print from its own shaft** (joint review P1,
//!   2026-10-12). The only part of a ball that starts a new layer island is
//!   its lowest point; it prints when it lies inside the shaft the ball stands
//!   on. The grown ball moves out along its seat normal, and on an oblique
//!   shaft its lowest point leaves the shaft: the file then cures a patch over
//!   nothing (Bat_Cat_Owl: a 0.270 mm ball on a 0.097 mm taper, a new
//!   97-pixel island at 10.425 mm). Each tip's growth is therefore also
//!   bounded by the largest ball whose lowest point stays inside its shaft
//!   ([`plinth_support::carried_ball_radii_mm`], the lowering's own
//!   derivation), and a tip it stops is warned with [`ShortReason::Overhang`].
//!
//! # Tolerance and the search
//!
//! "Reaches plan" is to within one pixel of area (the printer's own area
//! resolution, `pitch²`), not a new number. The radius is found by the secant
//! rule on `area(R)` (a proportional first step; never more than doubling in
//! a round, a search step), each round ONE batched
//! [`plinth_support::tip_contact_discs`] call, ONE
//! [`crate::footprint_normal::footprint_pairs`] native query and one parallel
//! [`crate::footprint_normal::footprint_normals`] pass over the tips still
//! short. [`MAX_ROUNDS`] bounds the search (a disclosed Placeholder that prunes
//! the search; a tip it stops is named in the warnings, never refused).

use plinth_support::SweepArm;

use crate::footprint_normal::{
    footprint_normals, footprint_pairs, FootprintBall, MeasureEnv, TipSite,
};

/// Rounds of the radius search. A Placeholder that only bounds the search: a
/// tip still short of plan when it runs out is warned with
/// [`ShortReason::SearchRounds`], never refused. The secant rule reaches one
/// pixel of area in two to four rounds on the measured models.
pub const MAX_ROUNDS: usize = 8;

/// Why a tip's real contact is still short of plan after the growth.
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
        }
    }
}

/// One tip still short of plan.
#[derive(Clone, Debug, PartialEq)]
pub struct ContactShort {
    /// The strut index.
    pub strut: usize,
    /// The real contact, mm², at the drawn ball.
    pub real_mm2: f64,
    /// The planned contact, mm².
    pub planned_mm2: f64,
    /// The drawn ball's radius, and the ungrown one's, mm.
    pub ball_radius_mm: f64,
    pub planned_ball_radius_mm: f64,
    pub reason: ShortReason,
}

/// What the contact measurement and growth did on one field.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContactOutcome {
    /// Whether it ran (it needs the footprint sites and the registry).
    pub enabled: bool,
    /// Seated tips measured.
    pub tips_measured: usize,
    /// Sum over those tips of the planned contact, mm².
    pub planned_mm2: f64,
    /// Sum of the real contact at the ungrown ball, and at the drawn ball.
    pub real_ungrown_mm2: f64,
    pub real_drawn_mm2: f64,
    /// Tips short of plan at the ungrown ball.
    pub short_ungrown: usize,
    /// Tips whose ball was grown, and the largest drawn/ungrown radius ratio.
    pub grown: usize,
    pub max_growth_ratio: f64,
    /// Rounds the search ran.
    pub rounds: usize,
    /// The largest ball the registry draws, mm (the growth limit).
    pub limit_ball_radius_mm: f64,
    /// Tips short of plan whose growth is bounded below that limit because a
    /// larger ball's lowest point would leave its shaft (the path hits of the
    /// carried bound), and of those, the ones it stopped short of plan.
    pub carried_bound: usize,
    pub stopped_by_overhang: usize,
    /// Tips still short after the growth, with why.
    pub short: Vec<ContactShort>,
    /// Per strut: `(real mm², planned mm²)` at the drawn ball, `None` for a
    /// tip that was not measured.
    pub per_strut: Vec<Option<(f64, f64)>>,
}

/// The real contact of every row of `arms` at its drawn ball: one batched
/// disc call, one pairs query, one parallel footprint pass. `(planned mm²,
/// real mm², drawn ball radius mm)`, `None` where the arm has no disc or no
/// site.
fn measure(
    env: &MeasureEnv<'_>,
    arms: &[SweepArm],
    sites: &[Option<TipSite>],
) -> Vec<Option<(f64, f64, f64)>> {
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
        let planned = std::f64::consts::PI * disc.radius_mm * disc.radius_mm;
        // No connected, admitted area in the ball is no contact.
        let real = answer.map_or(0.0, |found| found.contact_area_mm2);
        out[row] = Some((planned, real, radius));
    }
    out
}

/// **Measure every seated tip's real contact and grow the short ones' balls**
/// (module doc). `arms` are the field exactly as the lowering gets it, with
/// NO grown ball (`contact_ball_radius_mm: None`), in strut order; `sites` the
/// footprint sites (the seat triangles, re-derived for slid tips). Returns the
/// outcome; `outcome.per_strut` and the radii to draw are in strut order:
/// `grown[i]` is the radius strut `i`'s ball is drawn at, `None` for the
/// ungrown ball.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "one measurement and one secant search over the tips still short, in batched rounds"
)]
pub fn grow(
    env: &MeasureEnv<'_>,
    arms: &[SweepArm],
    sites: &[Option<TipSite>],
    limit_ball_radius_mm: f64,
    area_tolerance_mm2: f64,
) -> (ContactOutcome, Vec<Option<f64>>) {
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
    // Per tip: planned, ungrown radius, and the search state.
    struct Search {
        tip: usize,
        planned: f64,
        ungrown_radius: f64,
        radius: f64,
        real: f64,
        previous: Option<(f64, f64)>,
        reason: Option<ShortReason>,
        /// This tip's growth limit, and whether it is the carried bound
        /// (below the registry's).
        limit: f64,
        overhang: bool,
    }
    impl Search {
        /// Why a tip stopped at its limit.
        const fn at_limit(&self) -> ShortReason {
            if self.overhang {
                ShortReason::Overhang
            } else {
                ShortReason::BallLimit
            }
        }
    }
    let mut searches: Vec<Search> = Vec::new();
    for (position, answer) in first.iter().enumerate() {
        let Some((planned, real, radius)) = *answer else {
            continue;
        };
        let strut = tips[position];
        outcome.tips_measured += 1;
        outcome.planned_mm2 += planned;
        outcome.real_ungrown_mm2 += real;
        outcome.per_strut[strut] = Some((real, planned));
        if real + area_tolerance_mm2 >= planned {
            outcome.real_drawn_mm2 += real;
            continue;
        }
        outcome.short_ungrown += 1;
        searches.push(Search {
            tip: position,
            planned,
            ungrown_radius: radius,
            radius,
            real,
            previous: None,
            reason: None,
            limit: limit_ball_radius_mm,
            overhang: false,
        });
    }
    // THE CARRIED BOUND (joint review P1): the largest ball each short tip's
    // shaft carries, in one call. A tip it cannot be read for is not grown
    // (the side that keeps the file printable).
    let searched: Vec<SweepArm> = searches
        .iter()
        .map(|search| working[search.tip].clone())
        .collect();
    let ceilings = vec![limit_ball_radius_mm; searched.len()];
    let carried = plinth_support::carried_ball_radii_mm(&searched, &ceilings, env.layer_plan)
        .unwrap_or_else(|_| vec![None; searched.len()]);
    for (search, carried) in searches.iter_mut().zip(carried) {
        let bound = carried.unwrap_or(search.ungrown_radius);
        if bound < limit_ball_radius_mm {
            search.limit = bound.max(search.ungrown_radius);
            search.overhang = true;
            outcome.carried_bound += 1;
        }
    }
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
                        + (search.planned - search.real) * (search.radius - radius)
                            / (search.real - real)
                }
                // No surface yet: double the ball (a search step only).
                _ if search.real <= 0.0 => 2.0 * search.radius,
                _ => search.radius * search.planned / search.real,
            };
            // At most double per round: a tip with almost no contact would
            // otherwise jump straight to the limit, and a patch that ends
            // between the two would be misnamed a limit. A search step only.
            let next = next
                .min(2.0 * search.radius)
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
            let real = answer.map_or(search.real, |(_, real, _)| real);
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
            search.previous = Some((search.radius, search.real));
            search.radius = radius;
            search.real = real;
            if search.real + area_tolerance_mm2 >= search.planned {
                continue;
            }
            still.push(slot);
        }
        active = still;
    }
    for search in &searches {
        let strut = tips[search.tip];
        outcome.real_drawn_mm2 += search.real;
        outcome.per_strut[strut] = Some((search.real, search.planned));
        // A ball that found no area at all (its seat triangle never entered
        // it) is not grown: the growth bought nothing.
        let bought = search.radius > search.ungrown_radius
            && search.real > first[search.tip].map_or(0.0, |(_, real, _)| real);
        if bought {
            grown[strut] = Some(search.radius);
            outcome.grown += 1;
            outcome.max_growth_ratio = outcome
                .max_growth_ratio
                .max(search.radius / search.ungrown_radius);
        }
        if search.real + area_tolerance_mm2 < search.planned {
            if search.reason == Some(ShortReason::Overhang) {
                outcome.stopped_by_overhang += 1;
            }
            outcome.short.push(ContactShort {
                strut,
                real_mm2: search.real,
                planned_mm2: search.planned,
                ball_radius_mm: if bought { search.radius } else { search.ungrown_radius },
                planned_ball_radius_mm: search.ungrown_radius,
                reason: search.reason.unwrap_or(ShortReason::SearchRounds),
            });
        }
    }
    (outcome, grown)
}
