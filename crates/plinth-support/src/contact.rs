//! Contact physics — the single home of the seat/mask/admissibility rules.
//!
//! This module is **pure and kernel-free**: every function here is scalar
//! arithmetic over already-resolved geometry.  That is deliberate.  The
//! clearance and candidate-gathering loops call these functions per member, so
//! anything that reached for the kernel here would turn a batched stage into a
//! per-item native call.  The batching lives at the call sites (one
//! `NativeSurface::project` for a whole row set, one
//! `penetration_intervals_batch_bounded` for a whole forest); this module is
//! the scalar kernel-free core they scatter over.
//!
//! The rules implemented here are PR-1 through PR-4 of the Phase B-2 contact
//! physics design (`docs/rust-migration/contact-physics-2026-08-04.md`):
//!
//! * **PR-1 `CONTACT_SEAT_DEPTH`** — every model contact is seated, none
//!   kisses.  [`seat_depth_mm`], [`seat_endpoint`].
//! * **PR-2 `CONTACT_MASK_EXTENT`** — the mask that solves the kernel's actual
//!   clearance criterion.  [`mask_extent_mm`].
//! * **PR-3 `CONTACT_EMERGENCE_ADMISSIBILITY`** — a member that cannot leave
//!   its own contact is not a member.  [`admit`].
//! * **PR-4 `CONTACT_AREA_SUFFICIENCY`** — the H2 area-ratio law, enforced
//!   through the engine's own tolerant predicate.  [`area_sufficient`].
//!
//! Nothing in this module invents a constant.  Every number comes from the M0
//! registry via [`ContactPolicy`], or from the run's own [`LayerPlan`].

use plinth_core::LayerPlan;

use crate::forest::ContactEnd;
use crate::perception::FrozenTipLoadPolicy;

/// `sqrt(1 - c^2)`, clamped so float noise at `|c| = 1` cannot produce a NaN.
pub(crate) fn sin_from_cos(abs_cos: f64) -> f64 {
    (1.0 - abs_cos * abs_cos).max(0.0).sqrt()
}

/// The ball-tip seat factor `f - sqrt(f^2 - 1)` (PR-1 clause (c)).
///
/// DERIVED, never hardcoded.  A stub is emitted as a ball of radius
/// `f * r_tip` whose south pole sits at `p0` (`lower.rs`), so a plane at depth
/// `h` cuts it in a disc of radius `a` with `a^2 = 2*f*r_tip*h - h^2`.
/// Requiring that cut disc to be no narrower than the shaft above it
/// (`a >= r_tip`) gives `h >= r_tip * (f - sqrt(f^2 - 1))`.
///
/// At `f = ball_factor_stub = 1.5` this is `0.3819660112501051`, pinned by
/// unit test rather than written down.
pub(crate) fn ball_seat_factor(ball_factor: f64) -> f64 {
    ball_factor - (ball_factor * ball_factor - 1.0).max(0.0).sqrt()
}

/// The radius of the disc a plane at depth `h` cuts through the emitted tip
/// ball — PR-4's `r_eff` for a ball-tipped stub end.
///
/// This is the ACTUAL minimal separating cut at the seat plane, which is what
/// the area-ratio law must be evaluated on.  It is not `r_tip`.
pub(crate) fn ball_cut_radius_mm(r_tip_mm: f64, seat_depth_mm: f64, ball_factor: f64) -> f64 {
    (2.0 * ball_factor * r_tip_mm * seat_depth_mm - seat_depth_mm * seat_depth_mm)
        .max(0.0)
        .sqrt()
}

/// Registry-sourced policy for the contact rules.
///
/// Every field is either an M0 registered constant or a quantity the run
/// already carries (`process_resolution_mm`).  No value is invented here.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ContactPolicy {
    /// `default_contact_depth_mm` — the floor seat depth (0.3 mm).
    pub(crate) floor_depth_mm: f64,
    /// `max(LayerPlan::heights_mm())` — the run's own process resolution.
    ///
    /// This is BOTH the PR-1(a) slicer-resolution seat floor and the PR-2
    /// mask MARGIN.  It is emphatically NOT `THRESHOLD_BAND_MM`: that constant
    /// is result-inert telemetry whose purpose is to flag knife-edge clearance
    /// decisions, and sizing every production contact to sit one ULP outside
    /// it would destroy the instrument while leaving no real margin.
    pub(crate) process_resolution_mm: f64,
    /// `standoff_mm` (0.6 mm) — the kernel's clearance threshold addend.
    pub(crate) standoff_mm: f64,
    /// `min_illegal_len_mm` (0.05 mm) — the engine's own resolution for
    /// "is this free span long enough to return a verdict at all".
    pub(crate) min_free_span_mm: f64,
    /// `ball_factor_stub` (1.5).
    pub(crate) ball_factor_stub: f64,
    /// The engine's existing tip-load policy.  PR-4 delegates to it rather
    /// than duplicating the predicate, so the 1e-6 safety-factor tolerance and
    /// the `[0.2, 1.0]` clamp-boundary agreement are inherited, not restated.
    pub(crate) tip_load: FrozenTipLoadPolicy,
}

impl ContactPolicy {
    /// `max` over the run's own layer heights.  A fused region thinner than one
    /// layer is not resolvable by the printer, so this is the smallest
    /// physically meaningful seat floor and clearance margin.
    pub(crate) fn process_resolution_of(layers: &LayerPlan) -> f64 {
        layers.heights_mm().iter().copied().fold(0.0_f64, f64::max)
    }
}

/// One member end that declares a model contact.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ContactSite {
    pub(crate) end: ContactEnd,
    /// The member's radius AT THE CONTACT END — drives the PR-1(b) flank term.
    pub(crate) end_radius_mm: f64,
    /// `max(r0_mm, r1_mm)` — a sound, non-circular upper bound for the
    /// kernel's interpolated `r_lo.max(r_hi)` (PR-2).
    pub(crate) member_r_max_mm: f64,
    /// `|cos|` between the member axis and the surface normal, UNSEATED.
    pub(crate) abs_cos_incidence: f64,
    /// Model area routed onto this contact, for PR-4.
    pub(crate) carried_mm2: f64,
    /// The member's UNSEATED length.
    pub(crate) member_length_mm: f64,
}

/// Why a contact was refused.  Every variant routes to an ordinary candidate
/// rejection or quarantine (OD-4) — never a hard pipeline failure.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ContactRefusal {
    /// The member cannot emerge from its own contact mask (PR-3).
    GrazingIncidence { mask_mm: f64, length_mm: f64 },
    /// The minimal separating cut cannot carry the routed load (PR-4).
    ContactAreaInsufficient {
        available_mm2: f64,
        required_mm2: f64,
    },
    /// `|cos|` is numerically zero; the mask diverges (PR-2).
    DegenerateIncidence,
}

/// The seated geometry plus the extents the kernel path consumes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ContactExtents {
    /// PR-1 seat depth `h`, measured along the inward normal.
    pub(crate) seat_depth_mm: f64,
    /// PR-2 mask extent `u*`, measured along the member axis.
    pub(crate) mask_extent_mm: f64,
    /// `|cos|` AFTER seating.  By the PR-1 monotonicity lemma this is `>=` the
    /// unseated value.
    pub(crate) seated_abs_cos: f64,
    /// Member length AFTER seating.  Likewise `>=` the unseated value.
    pub(crate) seated_length_mm: f64,
    /// PR-4's non-regression clause: the tip radius was already at the
    /// registered `tip_radius_max_legacy_mm` clamp, so any area deficiency is
    /// the pre-existing clamp rather than a new contact defect.  Recorded as
    /// evidence; NOT refused.  See FU-CONTACT-1.
    pub(crate) clamp_limited: bool,
}

/// PR-1 — the designed seat depth `h` for one contact end.
///
/// `h = max(floor_depth, process_resolution + r_end*sin(theta), ball_seat)`.
///
/// Three independent requirements, each binding in a different regime:
/// (a) the fused region must be at least one layer deep or the slicer cannot
/// render it; (b) a frustum end at incidence `theta` has its end disc tilted,
/// and only `h >= r_end*sin(theta)` engages the ENTIRE cross-section rather
/// than smearing a partial bond; (c) for a ball-tipped stub the cut disc at
/// the seat plane must not be the narrowest section of the load path.
pub(crate) fn seat_depth_mm(
    abs_cos: f64,
    end_radius_mm: f64,
    policy: &ContactPolicy,
    ball_tipped: bool,
) -> f64 {
    let flank = policy.process_resolution_mm + end_radius_mm * sin_from_cos(abs_cos);
    let ball = if ball_tipped {
        ball_seat_factor(policy.ball_factor_stub) * end_radius_mm
    } else {
        0.0
    };
    policy.floor_depth_mm.max(flank).max(ball)
}

/// PR-1 step (2) — push the surface point `p_surface` inward along `normal` by
/// `seat_depth_mm`, and report the geometry the kernel will actually see.
///
/// This is the ONLY place seating happens.  Returns
/// `(p_contact, seated_abs_cos, seated_length_mm)`.
///
/// MONOTONICITY LEMMA (PR-1(d)): with `v0 = p_surface - other_end` and
/// `d = v0 . n < 0` (the member approaches from outside), seating gives
/// `|v1|^2 = |v0|^2 + 2h|d| + h^2 >= |v0|^2` and
/// `c1 >= c0` because `L0^2(|d|+h)^2 - |d|^2(L0^2 + 2h|d| + h^2)
/// = (L0^2 - |d|^2)(2h|d| + h^2) >= 0`.  Both quantities move in the
/// admitting direction, which is exactly why ONE pass suffices and nothing
/// iterates.
pub(crate) fn seat_endpoint(
    p_surface: [f64; 3],
    normal: [f64; 3],
    other_end: [f64; 3],
    seat_depth_mm: f64,
) -> ([f64; 3], f64, f64) {
    debug_assert!(
        (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2] - 1.0).abs() < 1e-6,
        "seat_endpoint requires a unit normal"
    );
    let p_contact = [
        p_surface[0] - normal[0] * seat_depth_mm,
        p_surface[1] - normal[1] * seat_depth_mm,
        p_surface[2] - normal[2] * seat_depth_mm,
    ];
    let v1 = [
        p_contact[0] - other_end[0],
        p_contact[1] - other_end[1],
        p_contact[2] - other_end[2],
    ];
    let len = (v1[0] * v1[0] + v1[1] * v1[1] + v1[2] * v1[2]).sqrt();
    let abs_cos = if len > 0.0 {
        ((v1[0] * normal[0] + v1[1] * normal[1] + v1[2] * normal[2]) / len).abs()
    } else {
        0.0
    };
    (p_contact, abs_cos, len)
}

/// PR-2 — the clearance-mask extent that solves the kernel's ACTUAL criterion.
///
/// `u* = (h + r_max + standoff + MARGIN) / |cos(theta)|`
///
/// The kernel's `seg_clear_one` builds AXIS points at the mask stations,
/// interpolates the radii there, and tests
/// `seg_surface_min(a, b) >= r_lo.max(r_hi) + standoff`.  It is a sphere-swept
/// capsule criterion, NOT a tilted-end-disc criterion — so there is no
/// `r_end*sin(theta)` term in the mask.  (That term belongs in the PR-1 seat
/// depth, where it IS correct.)
///
/// The axis point at arc distance `u` from a contact endpoint seated `h` below
/// the tangent plane stands `u*|cos| - h` above that plane, and on a planar
/// seat that height IS its distance to the surface.  Requiring
/// `u*|cos| - h >= r_max + standoff + MARGIN` gives the formula.
///
/// GUARANTEE: on a locally planar seat the kernel returns `clearance == MARGIN`
/// exactly, by construction.  That round trip is pinned by regression test.
///
/// There is no cosine floor, no clamp, and no arbitrary maximum.  As
/// `|cos| -> 0` the mask genuinely diverges, because a nearly tangential member
/// really does stay within the standoff band for an unbounded run; the correct
/// disposition is refusal (PR-3), not a floor that silently substitutes a
/// 2.50 mm mask for a 290 mm one and calls the difference checked.
pub(crate) fn mask_extent_mm(
    seat_depth_mm: f64,
    member_r_max_mm: f64,
    abs_cos: f64,
    policy: &ContactPolicy,
) -> Option<f64> {
    // NaN-safe by construction: a non-finite incidence refuses rather than
    // propagating into the mask. Written as an explicit finiteness test plus a
    // positive comparison so the NaN case cannot be lost to a negated operator.
    if !abs_cos.is_finite() || abs_cos <= f64::EPSILON {
        return None;
    }
    Some(
        (seat_depth_mm + member_r_max_mm + policy.standoff_mm + policy.process_resolution_mm)
            / abs_cos,
    )
}

/// PR-4 — the H2 area-ratio law, evaluated by the engine's EXISTING predicate.
///
/// `sufficient iff !FrozenTipLoadPolicy::violates_safety_factor(carried, r_eff)`,
/// which is algebraically `pi*r_eff^2 >= carried/X` with
/// `X = tensile_green_pa / (peel_pressure_pa * tip_safety_factor) = 12`,
/// carrying the engine's 1e-6 tolerance on the realized safety factor.
///
/// THE ALLOWANCE ANCHOR IS THE REGISTERED `X_ALLOWED`.  The H2 area-ratio
/// allowance is already in the M0 registry as the Derived evidence constant
/// `X_ALLOWED = floor(lower(X_STAR_CONE)/2) = 12`, chained from the measured
/// plate interval (`constants/evidence.rs`).  This module deliberately does NOT
/// re-declare that number as a second, hand-typed placeholder: a duplicate
/// would let a plate recalibration move `X_STAR_CONE` while the contact
/// threshold silently stayed at the stale hand-typed value.  The one-sided
/// engine-vs-allowance relationship is asserted against `X_ALLOWED` in
/// `crates/plinth-core/tests/constants_placeholders.rs`
/// (`engine_implied_tip_area_ratio_does_not_exceed_the_h2_allowance`).
///
/// Delegation is deliberate (not a duplicated inequality): a strict
/// reimplementation would refuse a contact one ULP low and vanish a support
/// that prints fine today, and it would have to re-derive the `[0.2, 1.0]`
/// clamp-boundary agreement by inspection rather than by construction.
///
/// PRECONDITION ON `A_contact`.  The area this predicate credits is
/// `pi*r_eff^2`, which is WITNESSED — not proven in general — by the PR-5
/// endpoint witness: 8 capsule rim rays plus the centreline all penetrating
/// from `s = 0` means the whole end cross-section is engaged.  That inference
/// is EXACT for locally convex or planar seats and is a SAMPLING BOUND
/// otherwise.  The concave regime is unmeasured on the reachable corpus.
pub(crate) fn area_sufficient(carried_mm2: f64, r_eff_mm: f64, policy: &ContactPolicy) -> bool {
    match plinth_core::PositiveFinite::new(r_eff_mm) {
        Ok(radius) => !policy.tip_load.violates_safety_factor(carried_mm2, radius),
        Err(_) => false,
    }
}

/// The area the H2 allowance requires for a given routed load, for reporting a
/// refusal.  `required = carried / X`, with `X` the engine's implied ratio.
fn required_area_mm2(carried_mm2: f64, policy: &ContactPolicy) -> f64 {
    let ratio = policy.tip_load.implied_area_ratio();
    if ratio > 0.0 {
        carried_mm2 / ratio
    } else {
        f64::INFINITY
    }
}

/// PR-1 step (3) + PR-3 + PR-4, in the normative order.
///
/// Seats the contact at the UNSEATED incidence, derives the seated geometry
/// once (the monotonicity lemma licenses the single pass — over-seating is
/// safe, under-seating is impossible), then evaluates the mask, the emergence
/// admissibility and the area sufficiency ON THE SEATED GEOMETRY, which is
/// what the kernel will actually see.
///
/// `other_end_mask_mm` is the mask already claimed by the member's OTHER end
/// (0.0 when that end carries no model contact).  `tip_clamped_at_max`
/// suppresses an area refusal per PR-4's non-regression clause.
pub(crate) fn admit(
    site: &ContactSite,
    policy: &ContactPolicy,
    seated_abs_cos: f64,
    seated_length_mm: f64,
    other_end_mask_mm: f64,
    ball_tipped: bool,
    tip_clamped_at_max: bool,
) -> Result<ContactExtents, ContactRefusal> {
    let seat = seat_depth_mm(
        site.abs_cos_incidence,
        site.end_radius_mm,
        policy,
        ball_tipped,
    );
    let mask = mask_extent_mm(seat, site.member_r_max_mm, seated_abs_cos, policy)
        .ok_or(ContactRefusal::DegenerateIncidence)?;

    // PR-3 — the member must EMERGE, and the distance check must be able to
    // return a verdict at all.  `min_free_span_mm` is the engine's own
    // registered resolution for the latter (`axis_illegal_rows` cannot flag a
    // free span shorter than it), so no new constant is invented.
    if mask + other_end_mask_mm > seated_length_mm - policy.min_free_span_mm {
        return Err(ContactRefusal::GrazingIncidence {
            mask_mm: mask,
            length_mm: seated_length_mm,
        });
    }

    // PR-4 — evaluated on the MINIMAL SEPARATING CUT at this contact.
    let r_eff = if ball_tipped {
        ball_cut_radius_mm(site.end_radius_mm, seat, policy.ball_factor_stub)
    } else {
        site.end_radius_mm
    };
    let mut clamp_limited = false;
    if !area_sufficient(site.carried_mm2, r_eff, policy) {
        if tip_clamped_at_max {
            // The policy itself wanted a larger tip and the registered
            // `tip_radius_max_legacy_mm` clamp denied it.  That deficiency is
            // the pre-existing clamp, not a new contact defect, so it is
            // recorded as evidence rather than refused (FU-CONTACT-1).
            clamp_limited = true;
        } else {
            return Err(ContactRefusal::ContactAreaInsufficient {
                available_mm2: std::f64::consts::PI * r_eff * r_eff,
                required_mm2: required_area_mm2(site.carried_mm2, policy),
            });
        }
    }

    Ok(ContactExtents {
        seat_depth_mm: seat,
        mask_extent_mm: mask,
        seated_abs_cos,
        seated_length_mm,
        clamp_limited,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BALL_FACTOR: f64 = 1.5;

    fn policy() -> ContactPolicy {
        ContactPolicy {
            floor_depth_mm: 0.3,
            process_resolution_mm: 0.05,
            standoff_mm: 0.6,
            min_free_span_mm: 0.05,
            ball_factor_stub: BALL_FACTOR,
            tip_load: FrozenTipLoadPolicy::for_contact_tests(),
        }
    }

    #[test]
    fn ball_seat_factor_is_derived_not_hardcoded() {
        assert!((ball_seat_factor(BALL_FACTOR) - 0.381_966_011_250_105_1).abs() < 1e-15);
    }

    #[test]
    fn break_even_tip_radius_is_where_cut_disc_equals_the_shaft() {
        // h = 0.3 (the registered floor) equals the ball seat requirement at
        // r_tip = 0.3 / 0.381966..., and there the cut disc equals r_tip.
        let r_break = 0.3 / ball_seat_factor(BALL_FACTOR);
        assert!((r_break - 0.785_410_196_624_968).abs() < 1e-12);
        let a = ball_cut_radius_mm(r_break, 0.3, BALL_FACTOR);
        assert!(
            (a - r_break).abs() < 1e-12,
            "cut disc {a} vs shaft {r_break}"
        );
    }

    #[test]
    fn ball_dominated_threshold_is_where_no_shaft_remains() {
        // The ball spans 3*r_tip against a total stub length h + 2.5 with
        // h = 0.381966*r_tip; above this radius there is no shaft to compare
        // against and PR-1(c) is an interface-area maximiser instead.
        let r = 2.5 / (3.0 - ball_seat_factor(BALL_FACTOR));
        assert!((r - 0.954_915_028_125_262_7).abs() < 1e-12);
    }

    #[test]
    fn seat_depth_takes_the_binding_regime() {
        let p = policy();
        // Normal incidence, small tip: the registered floor binds.
        assert!((seat_depth_mm(1.0, 0.2, &p, false) - 0.3).abs() < 1e-12);
        // Normal incidence, ball-tipped at r = 1.0: the ball cap binds.
        let h = seat_depth_mm(1.0, 1.0, &p, true);
        assert!((h - ball_seat_factor(BALL_FACTOR)).abs() < 1e-12);
        // Grazing incidence: the flank term binds.
        let h = seat_depth_mm(0.0, 0.5, &p, false);
        assert!((h - (0.05 + 0.5)).abs() < 1e-12);
    }

    #[test]
    fn at_normal_incidence_mask_is_seat_plus_r_max_plus_standoff_plus_margin() {
        // REWRITTEN per challenge RC-2.  At theta = 0 this IS the
        // kernel-correct answer; the original design asserted it was NOT, which
        // would have made the F1 error permanent.
        let p = policy();
        let mask = mask_extent_mm(0.3, 0.5, 1.0, &p).expect("finite mask");
        assert!((mask - (0.3 + 0.5 + 0.6 + 0.05)).abs() < 1e-12, "{mask}");
    }

    #[test]
    fn mask_has_no_sin_term_and_diverges_as_incidence_grazes() {
        let p = policy();
        let mut previous = 0.0;
        for &c in &[0.9_f64, 0.7, 0.5, 0.3, 0.1, 0.01] {
            let mask = mask_extent_mm(0.3, 0.5, c, &p).expect("finite mask");
            assert!(mask > previous, "mask must grow as |cos| falls");
            previous = mask;
            // Pure 1/cos scaling — no sin term anywhere.
            assert!((mask - (0.3 + 0.5 + 0.6 + 0.05) / c).abs() < 1e-12);
        }
        assert_eq!(mask_extent_mm(0.3, 0.5, 0.0, &p), None);
    }

    #[test]
    fn mask_is_monotone_in_seat_depth_and_radius() {
        let p = policy();
        let base = mask_extent_mm(0.3, 0.5, 0.8, &p).expect("mask");
        assert!(mask_extent_mm(0.4, 0.5, 0.8, &p).expect("mask") > base);
        assert!(mask_extent_mm(0.3, 0.6, 0.8, &p).expect("mask") > base);
    }

    #[test]
    fn monotonicity_lemma_holds_over_a_sweep() {
        // PR-1(d): seating never reduces |cos| and never shortens the member,
        // so the seat computed at the UNSEATED incidence remains a valid seat
        // at the seated incidence.  Over-seating is safe; under-seating cannot
        // happen.  This is what licenses the single pass.
        let p = policy();
        let normal = [0.0, 0.0, 1.0];
        for &l0 in &[1.0_f64, 3.0, 5.0, 12.0] {
            for &c0 in &[0.02_f64, 0.1, 0.3, 0.5, 0.8, 0.999] {
                let sin0 = sin_from_cos(c0);
                // Surface point at the origin; the other end sits outside
                // (below, along -n) at incidence c0.
                let p_surface = [0.0, 0.0, 0.0];
                let other = [-l0 * sin0, 0.0, l0 * c0];
                let h = seat_depth_mm(c0, 0.5, &p, false);
                let (_, c1, l1) = seat_endpoint(p_surface, normal, other, h);
                assert!(c1 >= c0 - 1e-12, "c1 {c1} < c0 {c0}");
                assert!(l1 >= l0 - 1e-12, "l1 {l1} < l0 {l0}");
                // The over-seat is still a valid seat at the seated incidence.
                assert!(h >= p.process_resolution_mm + 0.5 * sin_from_cos(c1) - 1e-12);
            }
        }
    }

    #[test]
    fn derived_rung_floor_is_1_50_mm_and_the_registered_band_is_unreachable() {
        // RC-15 reconciliation.  With brace_r_mm = 0.5 at normal incidence the
        // mask is 1.45 mm and PR-3 needs 0.05 mm of free span, so the shortest
        // admissible rung at ANY incidence is 1.50 mm.  The registered
        // `bracing_model_rung_min_length_mm` = 1.0 is a candidate-GENERATION
        // floor, deliberately not raised; the band [1.0, 1.50) is simply
        // unreachable, which is what this test pins.
        let p = policy();
        let mask = mask_extent_mm(0.3, 0.5, 1.0, &p).expect("mask");
        assert!((mask - 1.45).abs() < 1e-12);
        assert!((mask + p.min_free_span_mm - 1.50).abs() < 1e-12);

        for length in [1.0_f64, 1.1, 1.25, 1.4, 1.49, 1.499_999] {
            let site = ContactSite {
                end: ContactEnd::Rootward,
                end_radius_mm: 0.5,
                member_r_max_mm: 0.5,
                abs_cos_incidence: 1.0,
                carried_mm2: 0.0,
                member_length_mm: length,
            };
            let verdict = admit(&site, &p, 1.0, length, 0.0, false, false);
            assert!(
                matches!(verdict, Err(ContactRefusal::GrazingIncidence { .. })),
                "L = {length} must be inadmissible, got {verdict:?}"
            );
        }
        // Just above the floor it admits.
        let site = ContactSite {
            end: ContactEnd::Rootward,
            end_radius_mm: 0.5,
            member_r_max_mm: 0.5,
            abs_cos_incidence: 1.0,
            carried_mm2: 0.0,
            member_length_mm: 1.5001,
        };
        assert!(admit(&site, &p, 1.0, 1.5001, 0.0, false, false).is_ok());
    }

    #[test]
    fn degenerate_incidence_is_refused_not_floored() {
        let p = policy();
        let site = ContactSite {
            end: ContactEnd::Rootward,
            end_radius_mm: 0.5,
            member_r_max_mm: 0.5,
            abs_cos_incidence: 0.0,
            carried_mm2: 0.0,
            member_length_mm: 10.0,
        };
        assert_eq!(
            admit(&site, &p, 0.0, 10.0, 0.0, false, false),
            Err(ContactRefusal::DegenerateIncidence)
        );
    }

    /// THE 8-ROW CENSUS GOLDEN TABLE, recomputed with the corrected PR-2
    /// formula on the SEATED geometry (challenge RC-17).
    ///
    /// Inputs are the census's raw `|cos|` and unseated `L`; `r_end = r_max =
    /// brace_r_mm = 0.5`, `standoff = 0.6`, `process_resolution = 0.05`,
    /// `floor_depth = 0.3`.  Verdicts are 2 ADMIT / 6 REFUSE and are identical
    /// on the unseated screen and the seated evaluation.
    #[test]
    fn census_eight_rung_rows_reproduce_the_golden_table() {
        // MEASURED DIVERGENCE FROM THE DESIGN TABLE, recorded here per the
        // design's own instruction ("reproduce this table ... and record any
        // divergence").  Two independent causes, both in the DESIGN RECORD
        // rather than in this implementation:
        //
        // 1. INPUT ROUNDING (rows 0,1,3,4,5,6,7).  The table reports raw |cos|
        //    and L0 rounded to 6 dp while its derived columns came from the
        //    census's full-precision values.  Re-deriving from the rounded
        //    inputs cannot reproduce them to 1e-6 because d(c1)/d(c0) > 1.
        //    Measured |delta| on the seated columns: c1 <= 4.2e-5,
        //    L1 <= 2.3e-5, mask <= 4.7e-4.  Row 5 agrees to 3e-6 throughout.
        //
        // 2. AN INTERNAL SLIP IN ROW 2's MASK COLUMN.  Row 2's own published
        //    h (0.549996) and c1 (0.178818) give (0.549996 + 0.5 + 0.6 + 0.05)
        //    / 0.178818 = 9.50685, not the 9.508010 printed in the mask
        //    column — the table is inconsistent with itself by 1.16e-3 there,
        //    independently of anything this code does.  Row 2 is the extreme
        //    grazing row (|cos| = 0.003789, four significant figures), so it
        //    is also the row where the mask is most sensitive.
        //
        // Neither cause touches a DECISION.  The verdicts are 2 ADMIT / 6
        // REFUSE on both the seated and unseated evaluations, and the nearest
        // row to its boundary (row 6) misses admission by 0.398 mm — nearly
        // three orders of magnitude clear of the worst divergence above.  The
        // tolerance below is therefore set to cover the recorded table noise,
        // while the verdicts are asserted exactly.
        const TABLE_TOL: f64 = 2.0e-3;
        let p = policy();
        /// raw `|cos|`, L0, expected h, expected seated `|cos|`, expected L1,
        /// expected mask, expected free span, expected admit.
        type GoldenRow = (f64, f64, f64, f64, f64, f64, f64, bool);
        let rows: [GoldenRow; 8] = [
            (
                0.431_225, 4.630_548, 0.501_122, 0.513_128, 4.867_665, 3.217_772, 1.649_893, true,
            ),
            (
                0.108_199, 4.803_552, 0.547_065, 0.218_012, 4.893_061, 7.784_290, -2.891_229, false,
            ),
            (
                0.003_789, 3.090_579, 0.549_996, 0.178_818, 3.141_187, 9.508_010, -6.366_823, false,
            ),
            (
                0.127_310, 4.607_781, 0.545_932, 0.240_533, 4.708_517, 7.050_730, -2.342_213, false,
            ),
            (
                0.108_199, 4.803_552, 0.547_065, 0.218_012, 4.893_061, 7.784_290, -2.891_229, false,
            ),
            (
                0.862_506, 4.099_993, 0.303_023, 0.879_760, 4.364_052, 1.651_620, 2.712_432, true,
            ),
            (
                0.257_358, 3.884_180, 0.533_158, 0.378_025, 4.054_245, 4.452_480, -0.398_235, false,
            ),
            (
                0.089_100, 4.739_332, 0.548_011, 0.201_338, 4.819_172, 8.433_360, -3.614_028, false,
            ),
        ];
        let normal = [0.0, 0.0, 1.0];
        for (i, &(c0, l0, want_h, want_c1, want_l1, want_mask, want_span, want_admit)) in
            rows.iter().enumerate()
        {
            let h = seat_depth_mm(c0, 0.5, &p, false);
            assert!((h - want_h).abs() < 1e-6, "row {i} h {h} vs {want_h}");

            let sin0 = sin_from_cos(c0);
            let other = [-l0 * sin0, 0.0, l0 * c0];
            let (_, c1, l1) = seat_endpoint([0.0, 0.0, 0.0], normal, other, h);
            let mask = mask_extent_mm(h, 0.5, c1, &p).expect("finite mask");
            println!(
                "row {i}: c1 {c1:.6} (d {:+.2e})  l1 {l1:.6} (d {:+.2e})  \
                 mask {mask:.6} (d {:+.2e})  span {:.6} (d {:+.2e})",
                c1 - want_c1,
                l1 - want_l1,
                mask - want_mask,
                l1 - mask,
                (l1 - mask) - want_span
            );
            assert!(
                (c1 - want_c1).abs() < TABLE_TOL,
                "row {i} c1 {c1} vs {want_c1}"
            );
            assert!(
                (l1 - want_l1).abs() < TABLE_TOL,
                "row {i} l1 {l1} vs {want_l1}"
            );
            assert!(
                (mask - want_mask).abs() < TABLE_TOL,
                "row {i} mask {mask} vs {want_mask}"
            );
            assert!(
                ((l1 - mask) - want_span).abs() < TABLE_TOL,
                "row {i} span {} vs {want_span}",
                l1 - mask
            );

            let site = ContactSite {
                end: ContactEnd::Rootward,
                end_radius_mm: 0.5,
                member_r_max_mm: 0.5,
                abs_cos_incidence: c0,
                carried_mm2: 0.0,
                member_length_mm: l0,
            };
            let verdict = admit(&site, &p, c1, l1, 0.0, false, false);
            assert_eq!(
                verdict.is_ok(),
                want_admit,
                "row {i} verdict {verdict:?} expected admit = {want_admit}"
            );
        }
        let admits = rows.iter().filter(|r| r.7).count();
        assert_eq!(admits, 2, "the golden table is 2 ADMIT / 6 REFUSE");
    }
}
