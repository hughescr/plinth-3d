//! **SPEC-A4 §C.3 — the clause-2 forgetting-proof, at the layer where it can
//! exist.**
//!
//! docs/specs/coverage-repair.md §6 clause 2 says a committed repair step is accepted
//! only when "every noncoverage hard-gate count is componentwise no worse".
//! `plinth_support::NoncoverageVector` is what compares them — and it can only
//! compare counts that exist. Most of the twelve pre-write gates return
//! `NotYetPorted` and produce no count to compare, and no ported gate produces
//! one either: a pre-write gate REFUSES a sealed run, it does not hand the A4
//! controller a per-trial count. So clause 2 is satisfied **in part, by proxy**
//! today, and this file is the alarm that fires the day a gate moves off
//! `NotYetPorted` without somewhere for its quantity to go.
//!
//! **Why here and not in `plinth-support`.** `PreWriteGate` is defined in
//! `plinth-pipeline`, and `plinth-pipeline` depends on `plinth-support`, not
//! the other way round. The A4 controller lives in `plinth-support`, so it
//! cannot match on `PreWriteGate` and **must not be made able to** — inverting
//! the crate graph to buy a test would be a worse defect than the one the test
//! catches. This crate depends on both vocabularies, so this is the one place
//! they may meet. That is Craig's ruling R4.
//!
//! **What breaks, and when.** [`a4_coverage_claim`] matches `PreWriteGate`
//! **exhaustively with no wildcard arm**, so a thirteenth gate is a compile
//! error in this file. And the day a gate moves off `NotYetPorted`, the test
//! below fails until someone decides what clause 2 compares for it. That is a
//! real obligation instead of a promise, and it is the hand-off task #42 needs.

use crate::chain;

use std::collections::BTreeSet;
use std::path::PathBuf;

use plinth_mesh::make_toed_slab;
use plinth_pipeline::PreWriteGate;
use plinth_support::NoncoverageComponent;

/// A per-test scratch path that is removed on the way in and on the way out.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "plinth-a4-gate-drift-{label}-{}.chitubox",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// What the A4 non-regression vector claims about each pre-write gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum A4Claim {
    /// This gate's quantity IS clause 1's, not clause 2's.
    IsClauseOne,
    /// Covered indirectly: a violation refuses the trial's stage outright
    /// (`RepairStepOutcome::StageRefusal`) rather than being compared as a
    /// count. Honest, and not the same thing as a componentwise comparison.
    ByStageRefusal,
    /// Covered by a named `NoncoverageVector` component — a HOUSE count, not
    /// the gate's own, because the gate produces none.
    ByComponent(NoncoverageComponent),
    /// Nothing compares this gate. It has no count to compare because it is not
    /// ported; the day it is, the test below fails.
    NotCovered,
}

/// **EXHAUSTIVE over `PreWriteGate`, no wildcard arm.** Adding a thirteenth
/// gate is a compile error here.
const fn a4_coverage_claim(gate: PreWriteGate) -> A4Claim {
    match gate {
        PreWriteGate::UncoveredIslands => A4Claim::IsClauseOne,
        PreWriteGate::Clearance => A4Claim::ByStageRefusal,
        PreWriteGate::Load => A4Claim::ByComponent(NoncoverageComponent::LoadViolations),
        PreWriteGate::UnsupportedMinima => {
            A4Claim::ByComponent(NoncoverageComponent::UnsupportedSpecialized)
        }
        PreWriteGate::UnroutedSpecializedContacts => {
            A4Claim::ByComponent(NoncoverageComponent::UnroutedSpecialized)
        }
        PreWriteGate::IslandBirths
        | PreWriteGate::RegularContactInvariants
        | PreWriteGate::Connectivity
        | PreWriteGate::Printability
        | PreWriteGate::Murray
        | PreWriteGate::BallBreach
        | PreWriteGate::PhysicalContactInvariant => A4Claim::NotCovered,
    }
}

/// Which gates are ported, **measured** from a real pipeline run rather than
/// mirrored from `judge`'s own match.
///
/// A second copy of the ported/not-ported list would drift from the first
/// exactly as quietly as the hole this file exists to catch. So the list is
/// read out of `ArtifactRecord::pending_gates`, which `judge` fills at run time
/// from the arm it actually took.
fn ported_gates() -> BTreeSet<PreWriteGate> {
    let scratch = Scratch::new("ported");
    let mesh = make_toed_slab().expect("procedural toed slab");
    let outcome = chain::run_procedural_model("make_toed_slab", mesh, scratch.path())
        .expect("make_toed_slab reaches the pre-write gates and emits");
    let pending: BTreeSet<PreWriteGate> = outcome.pending_gates.iter().copied().collect();
    PreWriteGate::ALL
        .into_iter()
        .filter(|gate| !pending.contains(gate))
        .collect()
}

/// **The alarm.** A gate that reaches a verdict must have somewhere in the A4
/// non-regression vector for its count to go, or SPEC §6 clause 2 has a hole
/// nobody decided to leave.
#[test]
fn a_ported_gate_must_have_an_a4_component() {
    let ported = ported_gates();
    assert!(
        !ported.is_empty(),
        "if nothing is ported the fixture stopped reaching the gates and this test proves nothing"
    );
    for gate in PreWriteGate::ALL {
        if !ported.contains(&gate) {
            continue;
        }
        assert_ne!(
            a4_coverage_claim(gate),
            A4Claim::NotCovered,
            "{gate:?} moved off NotYetPorted but the A4 non-regression vector still has no \
             component for it — SPEC §6 clause 2 says every noncoverage hard-gate count must be \
             componentwise no worse, and nothing compares this one. See task #42."
        );
    }
}

/// The honest state of clause 2 today, pinned so it cannot quietly worsen and
/// so a reader is never told it is complete.
///
/// **Seven of the eleven clause-2 gates have no component at all.** Of the four
/// that have anything, three are matched by a HOUSE count rather than by the
/// gate's own — a pre-write gate refuses a sealed run and hands the A4
/// controller no per-trial count, whether it is ported or not — and the fourth,
/// `Clearance`, is covered only indirectly, by the trial's stage refusing. If
/// this count moves DOWN, someone closed part of #42 and should say so here; if
/// it moves UP, a component was deleted.
#[test]
fn the_measured_size_of_the_clause_two_gap_is_seven_of_eleven() {
    let uncovered = PreWriteGate::ALL
        .into_iter()
        .filter(|&gate| a4_coverage_claim(gate) == A4Claim::NotCovered)
        .count();
    assert_eq!(
        uncovered, 7,
        "SPEC §6 clause 2 is satisfied in part, by proxy; the size of the remaining gap is \
         task #42's and this design does not claim it"
    );
    let by_component = PreWriteGate::ALL
        .into_iter()
        .filter(|&gate| matches!(a4_coverage_claim(gate), A4Claim::ByComponent(_)))
        .count();
    assert_eq!(by_component, 3, "three gates are matched by a house proxy");
}

/// Every component the vector declares is claimed by some gate, and every
/// component a gate claims exists in the vector.
///
/// This is the other direction of the same drift: a component added to
/// `NoncoverageVector` that no gate maps to is a count nobody asked for, and a
/// claim naming a component that was deleted would not compile.
#[test]
fn the_two_vocabularies_do_not_drift_apart() {
    let claimed: BTreeSet<&'static str> = PreWriteGate::ALL
        .into_iter()
        .filter_map(|gate| match a4_coverage_claim(gate) {
            A4Claim::ByComponent(component) => Some(component.name()),
            A4Claim::IsClauseOne | A4Claim::ByStageRefusal | A4Claim::NotCovered => None,
        })
        .collect();
    for component in claimed {
        assert!(
            NoncoverageComponent::ALL
                .into_iter()
                .any(|declared| declared.name() == component),
            "{component} is claimed by a gate but is not a NoncoverageVector component"
        );
    }
    // The two components no gate maps to are the CCO transcript's own
    // dispositions, which are not pre-write gates at all: they are measured
    // inside the transaction, from the run's own cadence.
    let unmapped: Vec<&'static str> = NoncoverageComponent::ALL
        .into_iter()
        .map(NoncoverageComponent::name)
        .filter(|name| {
            !PreWriteGate::ALL.into_iter().any(|gate| {
                matches!(a4_coverage_claim(gate), A4Claim::ByComponent(component)
                    if component.name() == *name)
            })
        })
        .collect();
    assert_eq!(
        unmapped,
        vec!["unrouted_no_winner", "quarantined"],
        "these two are transcript dispositions, not gates; any OTHER unmapped component is a \
         count with no owner"
    );
}
