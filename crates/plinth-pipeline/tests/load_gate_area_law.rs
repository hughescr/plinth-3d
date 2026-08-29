//! **The `PreWriteGate::Load` gate, on a real run's own record, in both
//! directions.**
//!
//! A gate that cannot refuse is worse than no gate: it reports a safety that is
//! not there. So this file does not test a helper on hand-typed numbers — the
//! unit tests beside the gate do that. It runs the whole chain, takes the
//! emitted-contact record that run actually wrote to disk, and shows the gate
//!
//! * CLEARS that record unmodified, and
//! * REFUSES the same record with one field changed, naming the contact.
//!
//! The record is not constructed here and is not read back out of the gate's
//! own memory: it is deserialized from `<model>.coverage.repair.json`, the
//! sidecar `prepare::seal_and_emit` writes beside the artifact from
//! `FinalSupportAuthority::emitted_contacts()`. So the rows the sabotage
//! mutates are the rows a shipped run leaves on an operator's disk.
//!
//! **No engine is re-run and no geometry is re-derived anywhere below.** The
//! sabotage is one `f64` in one row of a JSON file the pipeline already wrote.

#[path = "../examples/shared/first_artifact_chain.rs"]
mod chain;

use std::path::PathBuf;

use plinth_pipeline::{judge_emitted_load_law, GateRefusal, LoadLawFault, PreWriteGate};
use plinth_support::EmittedContactDisclosure;

/// A per-test scratch path that is removed on the way in and on the way out,
/// together with the evidence sidecars the run writes beside it.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "plinth-load-gate-{label}-{}.chitubox",
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
        let _ = std::fs::remove_file(self.0.with_extension("coverage.repair.json"));
    }
}

/// The registered allowance, read from the ratified registry exactly as the
/// gate reads it. Never a hand-typed twin.
fn x_allowed() -> f64 {
    *plinth_core::m0_constants()
        .expect("registered constants")
        .evidence()
        .x_allowed()
        .value()
}

/// The smallest tip radius the area law admits for a carried area.
fn required_radius_mm(carried_area_mm2: f64) -> f64 {
    (carried_area_mm2 / (std::f64::consts::PI * x_allowed())).sqrt()
}

#[test]
fn the_load_gate_clears_a_real_record_and_refuses_the_same_record_sabotaged() {
    let scratch = Scratch::new("toed-slab");
    let mesh = plinth_mesh::make_toed_slab().expect("procedural toed slab");
    let outcome = chain::run_procedural_model("make_toed_slab", mesh, scratch.path())
        .expect("make_toed_slab reaches the pre-write gates and emits");

    // ---- The gate ran, end to end, inside the production door -----------
    //
    // `pending_gates` is filled from the arm `judge` actually took, so this is
    // a measurement of the real run and not a restatement of the match.
    assert!(
        !outcome.pending_gates.contains(&PreWriteGate::Load),
        "the Load gate did not reach a verdict on a run that emitted: {:?}",
        outcome.pending_gates
    );

    // ---- The record the run left on disk --------------------------------
    let ledger = outcome
        .repair_ledger
        .as_ref()
        .expect("an emitting model records where its sealed ledger was written");
    assert_eq!(ledger.write_error, None, "the ledger must reach disk");
    let body: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&ledger.path).expect("the sealed ledger must exist on disk"),
    )
    .expect("the sealed ledger must be valid JSON");
    let recorded: Vec<EmittedContactDisclosure> = serde_json::from_value(body["contacts"].clone())
        .expect("the sealed ledger carries the emitted-contact record");

    // A vacuous record would make every assertion below pass without checking
    // anything, which is the exact failure this file exists to rule out.
    assert!(
        !recorded.is_empty(),
        "the run described no emitted contacts, so nothing below is a test of anything"
    );
    assert_eq!(
        recorded.len(),
        outcome.routed_contacts,
        "the record on disk must describe every contact the sealed run routed"
    );

    // Print the real margins, so the acceptance below is readable as a
    // measurement rather than as an absence of failure.
    let mut tightest = f64::INFINITY;
    for contact in &recorded {
        let (Some(area), Some(radius)) = (contact.carried_area_mm2, contact.tip_radius_mm) else {
            panic!("a shipped contact the run could not describe: {contact:?}");
        };
        let margin = (std::f64::consts::PI * radius * radius) / (area / x_allowed());
        tightest = tightest.min(margin);
    }
    println!(
        "LOAD-GATE\tcontacts={}\tx_allowed={}\ttightest_margin_over_the_law={tightest:.3}x",
        recorded.len(),
        x_allowed(),
    );

    // ---- DIRECTION 1: the gate ACCEPTS the record as the run wrote it ----
    judge_emitted_load_law(&recorded, outcome.routed_contacts)
        .expect("the real run's own record satisfies the registered area law");

    // ---- DIRECTION 2: one under-sized tip, and the gate REFUSES ----------
    //
    // The sabotage is the defect the law exists to catch: the heaviest-loaded
    // contact keeps its carried area and ships a tip one part in a thousand
    // below the radius the law demands for it. Nothing else in the record
    // changes, and nothing is re-planned.
    let heaviest = recorded
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| {
            a.carried_area_mm2
                .unwrap_or(f64::NEG_INFINITY)
                .total_cmp(&b.carried_area_mm2.unwrap_or(f64::NEG_INFINITY))
        })
        .map(|(index, _)| index)
        .expect("a non-empty record has a heaviest row");
    let mut sabotaged = recorded.clone();
    let carried = sabotaged[heaviest]
        .carried_area_mm2
        .expect("the heaviest row states its load");
    let starved = required_radius_mm(carried) * 0.999;
    sabotaged[heaviest].tip_radius_mm = Some(starved);
    sabotaged[heaviest].tip_diameter_mm = Some(starved * 2.0);

    let refusal = judge_emitted_load_law(&sabotaged, outcome.routed_contacts)
        .expect_err("an under-sized tip must refuse emission");
    assert_eq!(refusal.gate(), Some(PreWriteGate::Load));
    let violations = refusal
        .load_law_violations()
        .expect("a load refusal carries the rows it refused");
    assert_eq!(
        violations.len(),
        1,
        "exactly the sabotaged row must be refused: {violations:?}"
    );
    assert_eq!(violations[0].contact_id, recorded[heaviest].contact_id);
    assert_eq!(
        violations[0].fault,
        LoadLawFault::TipSmallerThanTheAreaLawDemands
    );
    assert_eq!(violations[0].emitted_tip_radius_mm, Some(starved));

    // The refusal is read by someone who has not read the code, so the message
    // itself must name the arm and both radii.
    let text = refusal.to_string();
    println!("LOAD-GATE-REFUSAL\t{text}");
    assert!(
        text.contains(&format!("contact {}", recorded[heaviest].contact_id)),
        "{text}"
    );
    assert!(text.contains("X_ALLOWED"), "{text}");
    assert!(
        text.contains(&format!("{:.4} mm", required_radius_mm(carried))),
        "the radius the law demanded must appear: {text}"
    );

    // ---- DIRECTION 2b: a shipped contact the record cannot describe ------
    let mut undescribed = recorded.clone();
    undescribed[heaviest].carried_area_mm2 = None;
    let refusal = judge_emitted_load_law(&undescribed, outcome.routed_contacts)
        .expect_err("a routed contact whose load the run cannot state must refuse");
    assert_eq!(
        refusal.load_law_violations().expect("rows")[0].fault,
        LoadLawFault::CarriedAreaNotDescribed
    );

    // ---- DIRECTION 2c: a record that quietly covers fewer contacts -------
    let mut dropped = recorded.clone();
    dropped.remove(heaviest);
    assert!(
        matches!(
            judge_emitted_load_law(&dropped, outcome.routed_contacts)
                .expect_err("a record short of the routed set must refuse"),
            GateRefusal::LoadRecordIncomplete { routed, described }
                if routed == outcome.routed_contacts && described == outcome.routed_contacts - 1
        ),
        "dropping a shipped contact from the record must be caught, not absorbed"
    );

    // ---- The control: the unmodified record still clears -----------------
    //
    // Without this the refusals above would be consistent with a gate that
    // refuses everything, which is decoration of a different kind.
    judge_emitted_load_law(&recorded, outcome.routed_contacts)
        .expect("the unsabotaged record still clears, so the refusals above are the sabotage");
}
