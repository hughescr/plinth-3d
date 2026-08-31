//! The invariant registry.
//!
//! **This file is the ONLY shared module list, and it is complete.** Every
//! invariant below is already declared and already wired into the registry, so
//! an implementing agent edits exactly one file -- its own -- and never this
//! one. Adding a module here after the fact would put nine agents on one line
//! of source, which is the merge conflict this layout exists to prevent.
//!
//! Every stub returns [`crate::check::CheckOutcome::not_implemented`], which is
//! a **FAILURE** with `checked == indeterminate == scope`. "Not implemented"
//! never maps to pass (`audit-invariants.md` §0 rule 3).
//!
//! ## What an implementing agent does
//!
//! 1. Read `docs/specs/audit-invariants.md` for your invariant's section and
//!    `docs/specs/audit-contract.md` for what the bytes can answer.
//! 2. Replace the body of `run` in your own file. Keep `id`, `title` and the
//!    struct name: they are the JSON schema key and the summary label.
//! 3. Fill `outcome.measured` with the quantities your failure lines name,
//!    **for every item checked and not only for failures** -- a passing
//!    invariant that reports no numbers is not evidence (§5 rule 3).
//! 4. Set `measured["headline"]` to the short string the human summary prints
//!    after the counts.
//! 5. Report anything you cannot establish with `outcome.indeterminate(..)`,
//!    never by omitting it.

use crate::check::Check;

pub mod contact_area;
pub mod dead_geometry;
pub mod intersection;
pub mod island_coverage;
pub mod load_ownership;
pub mod load_path;
pub mod narrowest_section;
pub mod report_metrics;
pub mod seat_depth;

/// The eight gating invariants, in schema order. The verdict
/// `print_candidate` is true only when all eight are `"pass"`.
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
pub const GATING_IDS: [&str; 8] = [
    "i1_seat_depth",
    "i2_contact_area",
    "i3_min_section",
    "i4_load_ownership",
    "i5_closed_load_path",
    "i6_no_intersection",
    "i7_island_coverage",
    "i8_no_dead_geometry",
];

pub const GATING_TITLES: [&str; 8] = [
    "I1 seat depth",
    "I2 contact area",
    "I3 min section",
    "I4 load ownership",
    "I5 closed load path",
    "I6 no intersection",
    "I7 island coverage",
    "I8 no dead geometry",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_matches_the_schema_keys_exactly() {
        let g = gating();
        assert_eq!(g.len(), 8);
        for (chk, id) in g.iter().zip(GATING_IDS.iter()) {
            assert_eq!(&chk.id(), id, "registry order must match GATING_IDS");
        }
        for (chk, t) in g.iter().zip(GATING_TITLES.iter()) {
            assert_eq!(&chk.title(), t);
        }
    }
}
