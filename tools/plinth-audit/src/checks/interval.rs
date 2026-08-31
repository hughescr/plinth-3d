//! Conservative interval semantics — the one place the three-outcome
//! comparison lives (`docs/specs/audit-interval-policy.md` §3).
//!
//! **The defect this module removes.** Several checks were written as
//! `delivered >= required - tol`. That form compares the measured value's
//! **upper** bound against the requirement, so a value genuinely short of the
//! requirement passes whenever the shortfall is smaller than the measurement
//! uncertainty. The evidence has not established that the requirement is met;
//! the check said it had. `docs/specs/audit-contract.md`'s governing rule is
//! the opposite: **anything not established is a hard failure.**
//!
//! **Direction is the whole content of the fix, and it is where a mechanical
//! sweep goes wrong.** "Every seat must reach `h_required`" and "no unowned
//! area may exceed one pixel" take *opposite* conservative bounds. Applying
//! one rule to both inverts half the checks. So this module offers three
//! named entry points, one per relation, and every call site names which
//! relation it is:
//!
//! - [`min_type`] — "delivered must REACH OR EXCEED a requirement".
//!   Conservative bound of `V` is its LOWER bound: `V - tol >= R` to pass.
//! - [`max_type`] — "delivered must NOT EXCEED a limit".
//!   Conservative bound of `V` is its UPPER bound: `V + tol <= L` to pass.
//! - [`bracket`] — a two-sided `lo <= V <= hi` books check, which is
//!   [`min_type`] against `lo` **and** [`max_type`] against `hi`
//!   simultaneously, never one rule applied twice with the same sign
//!   (policy §3.3).
//!
//! **What this module is NOT for.** Equality-band predicates
//! (`|a - b| <= combined_tol`, two independent reconstructions of the same
//! physical quantity) are a different category, not a third direction: there
//! is no safety margin under test, so there is no "which side is
//! conservative" question and no third outcome to add. `|a - b| <= tol` is
//! already the correct, symmetric, already-conservative form (policy §3.4).
//! Do not route those through here.
//!
//! Quantities that are exact integer counts scaled by a printer quantum — a
//! voxel volume in I6, a pixel area in I7 — carry no continuous measurement
//! uncertainty and have no interval to widen (policy §3.4). Do not route
//! those through here either.

/// The three outcomes of one interval comparison. There is no fourth value.
///
/// [`Verdict::Indeterminate`] is a **failure** for gating
/// (`audit-invariants.md` §0 rule 3: `indeterminate > 0` forces `"fail"`),
/// and it stays distinguishable from [`Verdict::Fail`] in the output because
/// the distinction tells a reader whether the file is bad or the instrument
/// is blunt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    Indeterminate,
}

impl Verdict {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Indeterminate => "indeterminate",
        }
    }
    /// True only for [`Verdict::Pass`]. Indeterminate is never a pass.
    #[must_use]
    pub const fn is_pass(self) -> bool {
        matches!(self, Self::Pass)
    }
}

/// The reason token every straddling-band line carries
/// (`audit-interval-policy.md` §5). Distinct from the "could not compute at
/// all" reasons already in each check's vocabulary: this one means the
/// quantity WAS computed and the instrument cannot resolve it against its
/// threshold.
pub const REASON_WITHIN_TOLERANCE_BAND: &str = "within_tolerance_band";

/// The reason token for the two-sided books bracket of I4 clause (d).
pub const REASON_BOOKS_BRACKET_STRADDLED: &str = "books_bracket_straddled";

/// **Minimum-type**: the delivered quantity `v` must reach or exceed the
/// requirement `r` (`audit-interval-policy.md` §3.2).
///
/// The conservative bound of `v` is its LOWER bound, because passing must
/// mean "even in the worst case the evidence admits, this still clears the
/// bar":
///
/// ```text
/// PASS           v - tol >= r
/// FAIL           v + tol <  r
/// INDETERMINATE  otherwise (the interval straddles r)
/// ```
///
/// The retired form `v >= r - tol` is algebraically `v + tol >= r`, i.e. it
/// used `v`'s UPPER bound — backwards for this direction.
///
/// A non-finite input is [`Verdict::Indeterminate`]: an unmeasurable
/// quantity never maps to a pass. A negative `tol` is treated as its
/// magnitude, so a caller cannot narrow the band by passing a sign error.
#[must_use]
pub fn min_type(v: f64, r: f64, tol: f64) -> Verdict {
    if !v.is_finite() || !r.is_finite() || !tol.is_finite() {
        return Verdict::Indeterminate;
    }
    let tol = tol.abs();
    if v - tol >= r {
        Verdict::Pass
    } else if v + tol < r {
        Verdict::Fail
    } else {
        Verdict::Indeterminate
    }
}

/// **Maximum-type**: the delivered quantity `v` must not exceed the limit `l`
/// (`audit-interval-policy.md` §3.3).
///
/// The conservative bound of `v` is its UPPER bound, because passing must
/// mean "even in the worst case, this still does not exceed the limit":
///
/// ```text
/// PASS           v + tol <= l
/// FAIL           v - tol >  l
/// INDETERMINATE  otherwise
/// ```
///
/// Same non-finite and negative-`tol` handling as [`min_type`].
#[must_use]
pub fn max_type(v: f64, l: f64, tol: f64) -> Verdict {
    if !v.is_finite() || !l.is_finite() || !tol.is_finite() {
        return Verdict::Indeterminate;
    }
    let tol = tol.abs();
    if v + tol <= l {
        Verdict::Pass
    } else if v - tol > l {
        Verdict::Fail
    } else {
        Verdict::Indeterminate
    }
}

/// **Two-sided bracket**: `lo <= v <= hi`, needing both rules at once
/// (`audit-interval-policy.md` §3.3).
///
/// `v` must clear `lo` from above (a minimum-type relation to `lo`) AND clear
/// `hi` from below (a maximum-type relation to `hi`):
///
/// ```text
/// PASS           v - tol >= lo  AND  v + tol <= hi
/// FAIL           v + tol <  lo  OR   v - tol >  hi
/// INDETERMINATE  otherwise (the interval overlaps lo or hi without clearing it)
/// ```
///
/// A malformed bracket (`hi < lo`) is [`Verdict::Indeterminate`]: the
/// instrument, not the plan, is then the thing at fault.
#[must_use]
pub fn bracket(v: f64, lo: f64, hi: f64, tol: f64) -> Verdict {
    if !v.is_finite() || !lo.is_finite() || !hi.is_finite() || !tol.is_finite() {
        return Verdict::Indeterminate;
    }
    if hi < lo {
        return Verdict::Indeterminate;
    }
    let low = min_type(v, lo, tol);
    let high = max_type(v, hi, tol);
    match (low, high) {
        (Verdict::Pass, Verdict::Pass) => Verdict::Pass,
        // Either end CLEARLY out of the bracket is a measured defect in the
        // plan, not an unresolved measurement.
        (Verdict::Fail, _) | (_, Verdict::Fail) => Verdict::Fail,
        _ => Verdict::Indeterminate,
    }
}

/// The two edges of the interval `[v - tol, v + tol]`, for the failure and
/// indeterminate lines. `audit-invariants.md` §5 pinned-rule 3: an
/// indeterminate that reports only "indeterminate", with no interval a reader
/// can judge for themselves, is not evidence either.
#[must_use]
pub fn edges(v: f64, tol: f64) -> (f64, f64) {
    let tol = tol.abs();
    (v - tol, v + tol)
}

/// `[lo,hi]` formatted for a `key=value` failure-line field.
#[must_use]
pub fn fmt_interval(v: f64, tol: f64) -> String {
    let (lo, hi) = edges(v, tol);
    format!("[{lo},{hi}]")
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- min_type: the direction rows 1-4, 8, 10, 16, 17, 18 take ---------

    /// Boundary values are powers of two so `v - tol` and `v + tol` are
    /// exact in `binary64`. Testing the boundary with decimal literals
    /// asserts a rounding accident, not the predicate.
    #[test]
    fn min_type_passes_only_when_the_lower_bound_clears_the_requirement() {
        let (r, tol) = (1.0, 0.25);
        // Clear of the band on the high side.
        assert_eq!(min_type(1.5, r, tol), Verdict::Pass);
        // Exactly at the lower edge: v - tol == r, passes.
        assert_eq!(min_type(1.25, r, tol), Verdict::Pass);
        // One ULP inside the edge no longer passes.
        assert_eq!(
            min_type(f64::from_bits(1.25f64.to_bits() - 1), r, tol),
            Verdict::Indeterminate
        );
        // Clear of the band on the low side.
        assert_eq!(min_type(0.5, r, tol), Verdict::Fail);
        // Exactly at the upper edge: v + tol == r, still a fail (the whole
        // interval sits at or below the requirement).
        assert_eq!(min_type(0.75, r, tol), Verdict::Indeterminate);
    }

    /// The regression this whole module exists for. `v == r - tol/2` is a
    /// value genuinely short of the requirement, inside the uncertainty band.
    /// The retired form `v >= r - tol` PASSED it.
    #[test]
    fn min_type_refuses_the_shortfall_the_retired_form_passed() {
        let (r, tol) = (1.0, 1e-3);
        let v = r - tol / 2.0;
        assert!(v >= r - tol, "the retired form did pass this value");
        assert_eq!(min_type(v, r, tol), Verdict::Indeterminate);
        assert!(!min_type(v, r, tol).is_pass());
    }

    /// Exactly AT the requirement is now indeterminate, not a pass: the
    /// evidence cannot tell `r` from `r - tol`.
    #[test]
    fn min_type_at_the_bare_threshold_is_indeterminate() {
        assert_eq!(min_type(1.0, 1.0, 1e-3), Verdict::Indeterminate);
    }

    // --- max_type: the OPPOSITE bound, rows 9 and 11 ----------------------

    #[test]
    fn max_type_passes_only_when_the_upper_bound_stays_under_the_limit() {
        let (l, tol) = (1.0, 0.25);
        assert_eq!(max_type(0.5, l, tol), Verdict::Pass);
        assert_eq!(max_type(0.75, l, tol), Verdict::Pass);
        assert_eq!(max_type(1.5, l, tol), Verdict::Fail);
        assert_eq!(max_type(1.0, l, tol), Verdict::Indeterminate);
    }

    /// The sabotage this module's own doc warns about: applying the
    /// minimum rule to a maximum-type relation inverts it. At the same
    /// inputs the two functions must disagree, in both directions.
    #[test]
    fn the_two_directions_are_genuinely_opposite_not_the_same_rule_twice() {
        let (thr, tol) = (1.0, 0.25);
        let just_above = thr + tol / 2.0;
        let just_below = thr - tol / 2.0;
        // Just above the threshold: reaching a minimum is unresolved,
        // and staying under a maximum is also unresolved -- both refuse.
        assert_eq!(min_type(just_above, thr, tol), Verdict::Indeterminate);
        assert_eq!(max_type(just_above, thr, tol), Verdict::Indeterminate);
        // Well above: a minimum is met, a maximum is violated.
        let well_above = thr + 10.0 * tol;
        assert_eq!(min_type(well_above, thr, tol), Verdict::Pass);
        assert_eq!(max_type(well_above, thr, tol), Verdict::Fail);
        // Well below: a minimum is violated, a maximum is met.
        let well_below = thr - 10.0 * tol;
        assert_eq!(min_type(well_below, thr, tol), Verdict::Fail);
        assert_eq!(max_type(well_below, thr, tol), Verdict::Pass);
        let _ = just_below;
    }

    // --- bracket: row 14, both rules at once ------------------------------

    #[test]
    fn bracket_needs_both_ends_cleared_with_room() {
        let (lo, hi, tol) = (10.0, 20.0, 0.1);
        assert_eq!(bracket(15.0, lo, hi, tol), Verdict::Pass);
        // Straddling the low end.
        assert_eq!(bracket(10.05, lo, hi, tol), Verdict::Indeterminate);
        // Straddling the high end.
        assert_eq!(bracket(19.95, lo, hi, tol), Verdict::Indeterminate);
        // Clearly outside, both ends.
        assert_eq!(bracket(9.0, lo, hi, tol), Verdict::Fail);
        assert_eq!(bracket(21.0, lo, hi, tol), Verdict::Fail);
        // Malformed bracket.
        assert_eq!(bracket(15.0, 20.0, 10.0, tol), Verdict::Indeterminate);
    }

    /// The retired form widened BOTH ends simultaneously. Both of those
    /// widenings must now be refused.
    #[test]
    fn bracket_refuses_the_two_shortfalls_the_retired_form_passed() {
        let (lo, hi, tol) = (10.0, 20.0, 0.1);
        for v in [lo - tol / 2.0, hi + tol / 2.0] {
            assert!(
                v >= lo - tol && v <= hi + tol,
                "the retired form did pass {v}"
            );
            assert_eq!(bracket(v, lo, hi, tol), Verdict::Indeterminate);
        }
    }

    // --- unmeasurable inputs ---------------------------------------------

    #[test]
    fn a_nonfinite_input_is_never_a_pass_in_any_direction() {
        for f in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(min_type(f, 1.0, 1e-3), Verdict::Indeterminate);
            assert_eq!(min_type(1.0, f, 1e-3), Verdict::Indeterminate);
            assert_eq!(min_type(1.0, 1.0, f), Verdict::Indeterminate);
            assert_eq!(max_type(f, 1.0, 1e-3), Verdict::Indeterminate);
            assert_eq!(max_type(1.0, f, 1e-3), Verdict::Indeterminate);
            assert_eq!(bracket(f, 0.0, 2.0, 1e-3), Verdict::Indeterminate);
        }
    }

    #[test]
    fn a_negative_tolerance_cannot_narrow_the_band() {
        assert_eq!(min_type(1.0, 1.0, -1e-3), Verdict::Indeterminate);
        assert_eq!(max_type(1.0, 1.0, -1e-3), Verdict::Indeterminate);
    }

    #[test]
    fn the_interval_edges_are_reported_for_the_reader() {
        let (lo, hi) = edges(2.0, 0.25);
        assert!((lo - 1.75).abs() < 1e-15 && (hi - 2.25).abs() < 1e-15);
        assert_eq!(fmt_interval(2.0, 0.25), "[1.75,2.25]");
    }
}
