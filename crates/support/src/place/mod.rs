//! The planner: where contacts go and how big they are (lane A; design
//! `step11-construction.md` sections 5.2 to 5.6). Step 0 holds the signatures only.
//!
//! The planner works in a private proposal and calls the load model
//! ([`crate::load::analyse`]) once per round. It never moves or grows an admitted contact.

use crate::construct::ContactCandidate;
use crate::load::{LayerStack, LoadReport};

/// Planner settings (starting values from the design, **[S]**).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanSettings {
    /// Most planner rounds.
    pub max_rounds: u32,
    /// Spacing of the hexagonal sample over the needs regions is `coverage_reach_mm` times
    /// the square root of 3, mm.
    pub coverage_reach_mm: f64,
    /// Inner and outer radius of the ring of added contacts around a capped tip, mm.
    pub ring_mm: [f64; 2],
}

impl Default for PlanSettings {
    fn default() -> Self {
        Self {
            max_rounds: 8,
            coverage_reach_mm: 3.0,
            ring_mm: [1.0, 2.5],
        }
    }
}

/// How the search ended (design section 5.6 step 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Termination {
    /// Every ratio is within the design ratio and every island is anchored.
    Satisfied,
    /// The rounds or samples ran out; nothing is proved.
    SearchBudgetExhausted,
    /// No legal anchor exists within the stated search domain.
    ProvedNoLegalAnchor,
    /// The planner is not built yet (step 0 stub).
    NotBuilt,
}

/// The planner's answer.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    /// How the search ended.
    pub termination: Termination,
    /// The candidates to probe and commit, in a stable order.
    pub candidates: Vec<ContactCandidate>,
    /// The load model's last report on the proposal.
    pub load: LoadReport,
}

/// Plan the contacts for a sliced model.
#[must_use]
pub fn plan_contacts(stack: &LayerStack, settings: &PlanSettings) -> Proposal {
    let _ = (stack, settings);
    Proposal {
        termination: Termination::NotBuilt,
        candidates: Vec::new(),
        load: LoadReport::unverified(crate::load::DESIGN_RATIO, "the planner is not built yet"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stub_proposes_nothing_and_says_so() {
        let p = plan_contacts(&LayerStack::default(), &PlanSettings::default());
        assert_eq!(p.termination, Termination::NotBuilt);
        assert!(p.candidates.is_empty());
        assert!(!p.load.is_computed());
    }
}
