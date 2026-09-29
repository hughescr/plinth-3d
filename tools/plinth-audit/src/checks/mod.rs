//! The invariant registry.
//!
//! **This file is the ONLY shared module list, and it is complete.** Every
//! invariant below is already declared and already wired into the registry, so
//! an implementing agent edits exactly one file -- its own -- and never this
//! one. Adding a module here after the fact would put eleven agents on one
//! line of source, which is the merge conflict this layout exists to prevent.
//!
//! Every stub returns [`crate::check::CheckOutcome::not_implemented`], which is
//! a **FAILURE** with `checked == indeterminate == scope`. "Not implemented"
//! never maps to pass (`audit-invariants.md` §0 rule 3).
//!
//! I9 (`unsupported_span`, `docs/specs/audit-span-criterion.md`) and I10
//! (`arrangement_spread`, `docs/specs/audit-spread-criterion.md`) are the two
//! newest invariants. They were registered here as stubs first, so their
//! implementations had a fixed schema key, scope convention and declared
//! inputs to code against ([`crate::context::DeclaredInputs::span_l_survived_max_mm`],
//! `span_condition_shift_max`, `span_overhang_angle_legacy_deg`; and
//! [`crate::context::AuditOptions::spread_budget`]). **Both are implemented
//! as of 2026-08-31** and no gating invariant is a stub any more; see their
//! module docs for the specifications they satisfy, and
//! `tests/fixtures.rs::every_unimplemented_check_reports_failure_never_pass`,
//! which now asserts the stub count is zero rather than merely printing it.
//!
//! ## What an implementing agent does
//!
//! 1. Read `docs/specs/audit-invariants.md` for your invariant's section and
//!    `docs/specs/audit-contract.md` for what the bytes can answer (I9 and I10
//!    instead read their own specification documents, named above).
//! 2. Replace the body of `run` in your own file. Keep `id`, `title` and the
//!    struct name: they are the JSON schema key and the summary label.
//! 3. Fill `outcome.measured` with the quantities your failure lines name,
//!    **for every item checked and not only for failures** -- a passing
//!    invariant that reports no numbers is not evidence (§5 rule 3).
//! 4. Set `measured["headline"]` to the short string the human summary prints
//!    after the counts.
//! 5. Report anything you cannot establish with `outcome.indeterminate(..)`,
//!    never by omitting it.

//! ## The one shared helper
//!
//! [`interval`] is NOT an invariant. It is the single implementation of the
//! three-outcome threshold comparison of
//! `docs/specs/audit-interval-policy.md` §3, shared by every check that
//! grades a measured quantity against a requirement or a limit. It lives
//! here rather than one copy per check because Finding 3 of the cross-family
//! review found the same optimistic comparison copied into four files, and a
//! copied predicate carries its bug into every copy.

use crate::check::Check;

pub mod arrangement_spread;
pub mod contact_area;
pub mod dead_geometry;
pub mod intersection;
pub mod interval;
pub mod island_coverage;
pub mod load_ownership;
pub mod load_path;
pub mod narrowest_section;
pub mod report_metrics;
pub mod seat_depth;
/// I5 clause (e), "no support island" — a helper of `load_path`, not an
/// invariant of its own; it adds no schema key.
pub mod support_below;
pub mod unsupported_span;

/// The ten gating invariants, in schema order. The verdict
/// `support_invariants_pass` is true only when all ten are `"pass"`. That
/// name is deliberate: it means the ten support invariants this audit
/// currently checks all passed, and nothing stronger -- it does NOT mean the
/// print is safe. See `docs/specs/audit-span-criterion.md` §8 and
/// `docs/specs/audit-spread-criterion.md` §6 for what even a full pass still
/// cannot see.
#[must_use]
pub fn gating() -> Vec<Box<dyn Check>> {
    vec![
        Box::new(seat_depth::SeatDepth),
        Box::new(contact_area::ContactArea),
        Box::new(narrowest_section::NarrowestSection),
        Box::new(load_ownership::LoadOwnership),
        Box::new(load_path::LoadPath),
        Box::new(intersection::Intersection),
        Box::new(island_coverage::IslandCoverage),
        Box::new(dead_geometry::DeadGeometry),
        Box::new(unsupported_span::UnsupportedSpan),
        Box::new(arrangement_spread::ArrangementSpread),
    ]
}

/// The non-gating report of §4. Its verdict field is literally `"disclosed"`,
/// and `"disclosed"` is not a passing value anywhere in the schema. It never
/// contributes to `print_candidate`.
#[must_use]
pub fn report() -> report_metrics::ReportMetrics {
    report_metrics::ReportMetrics
}

/// The schema keys, in order. Used by the file-level abort path of §5 rule 6,
/// which must still write every invariant as `"fail"`.
pub const GATING_IDS: [&str; 10] = [
    "i1_seat_depth",
    "i2_contact_area",
    "i3_min_section",
    "i4_load_ownership",
    "i5_closed_load_path",
    "i6_no_intersection",
    "i7_island_coverage",
    "i8_no_dead_geometry",
    "i9_unsupported_span",
    "i10_arrangement_spread",
];

pub const GATING_TITLES: [&str; 10] = [
    "I1 seat depth",
    "I2 contact area",
    "I3 min section",
    "I4 load ownership",
    "I5 closed load path",
    "I6 no intersection",
    "I7 island coverage",
    "I8 no dead geometry",
    "I9 unsupported span",
    "I10 arrangement spread",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_matches_the_schema_keys_exactly() {
        let g = gating();
        assert_eq!(g.len(), 10);
        for (chk, id) in g.iter().zip(GATING_IDS.iter()) {
            assert_eq!(&chk.id(), id, "registry order must match GATING_IDS");
        }
        for (chk, t) in g.iter().zip(GATING_TITLES.iter()) {
            assert_eq!(&chk.title(), t);
        }
    }

    /// The never-pass rule at the unit level: `not_implemented` maps to
    /// `Fail`, for any check id, so a stub can never certify anything.
    ///
    /// **What this test does NOT establish, stated because its previous name
    /// ("the two newest invariants are registered as failing stubs") claimed
    /// it did:** it builds a `CheckOutcome` by hand and never calls
    /// `Check::run`, so it says nothing about whether I9 or I10 is actually
    /// implemented. That property is asserted where it can be — against a
    /// real artifact, in
    /// `tests/fixtures.rs::every_unimplemented_check_reports_failure_never_pass`,
    /// which requires the stub count to be zero.
    #[test]
    fn a_not_implemented_outcome_is_a_failure_for_every_registered_check() {
        use crate::check::{Status, NOT_IMPLEMENTED_CODE};
        for chk in gating() {
            let outcome = crate::check::CheckOutcome::not_implemented(chk.id(), "f.chitubox", 0, 1);
            assert_eq!(
                outcome.status(),
                Status::Fail,
                "{} must never pass",
                chk.id()
            );
            assert_eq!(outcome.failures[0].code, NOT_IMPLEMENTED_CODE);
        }
    }
}
