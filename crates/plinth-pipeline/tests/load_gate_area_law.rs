//! **The `PreWriteGate::Load` gate, on a real run's own record, in both
//! directions — and the live defect it found the moment it was wired in.**
//!
//! A gate that cannot refuse is worse than no gate: it reports a safety that is
//! not there. So this file does not test a helper on hand-typed numbers — the
//! unit tests beside the gate do that. It runs the whole chain and shows the
//! gate deciding on the emitted-contact record that run actually wrote to disk.
//!
//! # READ THIS BEFORE CHANGING ANYTHING HERE
//!
//! **`make_toed_slab` does not satisfy the registered support area law, and the
//! ported gate refuses it.** Three of its sixty-four emitted contacts ship tips
//! smaller than `pi * r^2 >= carried_area / X_ALLOWED` demands:
//!
//! | contact | carried area | shipped tip radius | realized X |
//! |---------|--------------|--------------------|------------|
//! | 24      | 450.000 mm²  | 0.050 mm           | 57 296     |
//! | 48      | 450.000 mm²  | 0.050 mm           | 57 296     |
//! | 4       |  60.000 mm²  | 1.000 mm           |     19.1   |
//!
//! against a registered allowance of 12. This is not a disagreement between the
//! gate and the engine: `FrozenTipLoadPolicy::violates_safety_factor`, which
//! `plinth_support::contact::area_sufficient` already delegates to, refuses all
//! three as well. Nothing runs that predicate on the FINAL numbers, which is the
//! hole this gate was ported to close.
//!
//! No tip could have carried those two 450 mm² seats. The largest area any
//! admissible tip may carry is `X_ALLOWED * pi * TIP_RADIUS_MAX_LEGACY_MM^2` =
//! 37.70 mm², so a seat charged 450 mm² is a LOAD PARTITION defect, not a tip
//! sizing one — 900 of this model's 967.7 mm² of carried area landed on two
//! contacts while fifty-eight others were charged exactly zero.
//!
//! # Where it comes from, and it is already written down
//!
//! `plinth_support::perception`'s `finalize_load_rows` charges its partition
//! under `CarriableMaximum::SingleCarrier` — one carrier per face whatever its
//! area — and its own block comment says why: the merge tournament's retained
//! load delta indexes rows POSITIONALLY and cannot express a face carried by
//! several contacts, so splitting there without rebuilding that machine would
//! produce a delta that disagrees with the finalize it is checked against. That
//! comment names the escalation as **task #68**.
//!
//! So the machinery to split an over-maximum face already exists and is already
//! reachable — `carriable_share_count(450.0, AreaLaw(37.70))` answers 12 — it is
//! just not what the production path runs. It is not a model-specific accident
//! either: the corpus model `mini` refuses with the same two areas, 450.000 mm²
//! and 60.000 mm², because they are whole faces of a 30 x 30 mm form.
//!
//! **This gate is the first thing in the codebase that refuses the result.**
//! Until task #68 lands, any model with a downfacing face above 37.70 mm² is
//! refused at the pre-write door instead of shipped.
//!
//! **This file therefore pins a defect, not an approval.** When the load
//! partition is fixed, the assertions below fail and this file is rewritten to
//! assert emission. Do not weaken the gate to make them pass, and do not delete
//! the pin to make the suite quiet.
//!
//! # What is demonstrated, both directions
//!
//! * The gate REFUSES, end to end, inside the production door, before any file
//!   exists — the run stops at `PreWriteGate::Load` and writes no artifact.
//! * The gate CLEARS the same record once exactly those three rows are given
//!   the tip the law demands, so the refusal is those three rows and nothing
//!   else. Without this the refusal would be consistent with a gate that
//!   refuses everything, which is decoration of a different kind.
//! * The gate REFUSES again when one repaired row is put back under the law.
//!
//! **No engine is re-run and no geometry is re-derived below.** Every mutation
//! is one `f64` in one row of a record the pipeline already wrote.

#[path = "../examples/shared/first_artifact_chain.rs"]
mod chain;

use std::path::PathBuf;

use plinth_pipeline::{judge_emitted_load_law, LoadLawFault, PreWriteGate};
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

    fn ledger(&self) -> PathBuf {
        self.0.with_extension("coverage.repair.json")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(self.ledger());
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

/// The largest carried area ANY admissible tip may legally carry: the area law
/// solved for area at the registered maximum tip radius.
fn max_carriable_area_mm2() -> f64 {
    let max_radius_mm = plinth_core::m0_constants()
        .expect("registered constants")
        .placeholders()
        .tip_radius_max_legacy_mm()
        .diagnostic()
        .get();
    x_allowed() * std::f64::consts::PI * max_radius_mm * max_radius_mm
}

#[test]
fn the_load_gate_refuses_the_toed_slabs_own_record_and_clears_it_once_repaired() {
    let scratch = Scratch::new("toed-slab");
    let mesh = plinth_mesh::make_toed_slab().expect("procedural toed slab");
    let stop = chain::run_procedural_model("make_toed_slab", mesh, scratch.path()).expect_err(
        "make_toed_slab ships three contacts that violate the registered area law, so the \
             ported Load gate must refuse it. If this model now emits, the load partition was \
             fixed and this whole file needs rewriting to assert emission -- see the module doc.",
    );

    // ---- DIRECTION 1: the gate REFUSES, end to end, before any file ------
    assert!(stop.is_gates(), "the run must stop at a gate: {stop:?}");
    assert_eq!(
        stop.gate(),
        Some(PreWriteGate::Load),
        "the Load gate must be the gate that refused: {stop:?}"
    );
    assert!(
        !scratch.path().exists(),
        "a gate-refused model must leave no artifact at its target"
    );

    // ---- The record the run left on disk ---------------------------------
    //
    // The ledger is written one frame before the gate speaks, so a refused run
    // still leaves the record the gate refused.
    let body: serde_json::Value = serde_json::from_slice(
        &std::fs::read(scratch.ledger()).expect("the sealed ledger must exist even on a refusal"),
    )
    .expect("the sealed ledger must be valid JSON");
    let recorded: Vec<EmittedContactDisclosure> = serde_json::from_value(body["contacts"].clone())
        .expect("the sealed ledger carries the emitted-contact record");
    assert!(
        !recorded.is_empty(),
        "the run described no emitted contacts, so nothing below is a test of anything"
    );

    // The gate reached its arithmetic at all, which means its record-integrity
    // check already passed: the record describes every contact the sealed
    // snapshot's own independent transcript walk counted as routed.
    let refusal = judge_emitted_load_law(&recorded, recorded.len())
        .expect_err("the record the run wrote must refuse for the same reason the run did");
    let violations = refusal
        .load_law_violations()
        .expect("a load refusal carries the rows it refused")
        .to_vec();
    for violation in &violations {
        println!("LOAD-VIOLATION\t{violation}");
    }

    // ---- The finding, pinned -------------------------------------------
    assert_eq!(
        violations.len(),
        3,
        "the measured size of this model's area-law violation moved; that is a finding, not a \
         number to refresh: {violations:?}"
    );
    for violation in &violations {
        assert_eq!(
            violation.fault,
            LoadLawFault::TipSmallerThanTheAreaLawDemands,
            "every row here is an under-sized tip, not an undescribable record: {violation:?}"
        );
        let carried = violation
            .carried_area_mm2
            .expect("an under-sized-tip row states its load");
        // The sharp half of the finding: these seats are charged more area than
        // ANY registered tip may carry, so no tip sizing could have satisfied
        // the law. The defect is in the load partition.
        assert!(
            carried > max_carriable_area_mm2(),
            "a seat charged {carried} mm^2 is within the {} mm^2 any admissible tip may carry, so \
             this row would be a tip-sizing defect rather than a load-partition one and the \
             module doc above no longer describes what is happening",
            max_carriable_area_mm2()
        );
    }
    let heaviest = violations
        .iter()
        .map(|violation| violation.carried_area_mm2.unwrap_or(0.0))
        .fold(0.0_f64, f64::max);
    assert!(
        (heaviest - 450.0).abs() < 1.0e-9,
        "the heaviest violating seat carries {heaviest} mm^2; it carried 450.000 mm^2 when this \
         finding was recorded"
    );

    // ---- DIRECTION 2: repair exactly those rows, and the gate CLEARS -----
    //
    // A RECORD edit, not a claim that such a tip is printable: 3.4549 mm is far
    // above the registered maximum, which is the finding restated. What it
    // proves is that those three rows are the whole of the refusal.
    let mut repaired = recorded.clone();
    for row in &mut repaired {
        let Some(carried) = row.carried_area_mm2 else {
            continue;
        };
        let demanded = required_radius_mm(carried);
        if row.tip_radius_mm.unwrap_or(0.0) < demanded {
            row.tip_radius_mm = Some(demanded);
            row.tip_diameter_mm = Some(demanded * 2.0);
        }
    }
    judge_emitted_load_law(&repaired, repaired.len()).expect(
        "with every tip sized to the law the gate must clear, or it is refusing something other \
         than the area law",
    );

    // ---- DIRECTION 2b: put one repaired row back under the law -----------
    let worst = violations
        .iter()
        .max_by(|a, b| {
            a.carried_area_mm2
                .unwrap_or(0.0)
                .total_cmp(&b.carried_area_mm2.unwrap_or(0.0))
        })
        .expect("three violations have a worst");
    let index = repaired
        .iter()
        .position(|row| row.contact_id == worst.contact_id)
        .expect("the repaired record still holds the contact");
    let carried = repaired[index]
        .carried_area_mm2
        .expect("the repaired row states its load");
    let starved = required_radius_mm(carried) * 0.999;
    repaired[index].tip_radius_mm = Some(starved);
    repaired[index].tip_diameter_mm = Some(starved * 2.0);

    let refusal = judge_emitted_load_law(&repaired, repaired.len())
        .expect_err("an under-sized tip must refuse emission");
    assert_eq!(refusal.gate(), Some(PreWriteGate::Load));
    let rows = refusal
        .load_law_violations()
        .expect("a load refusal carries its rows");
    assert_eq!(
        rows.len(),
        1,
        "exactly the row that was put back under the law must be refused: {rows:?}"
    );
    assert_eq!(rows[0].contact_id, worst.contact_id);
    assert_eq!(rows[0].emitted_tip_radius_mm, Some(starved));

    // The refusal is read by someone who has not read the code, so the message
    // itself must name the arm, the load and both radii.
    let text = refusal.to_string();
    println!("LOAD-GATE-REFUSAL\t{text}");
    assert!(
        text.contains(&format!("contact {}", worst.contact_id)),
        "{text}"
    );
    assert!(text.contains("X_ALLOWED"), "{text}");
    assert!(
        text.contains(&format!("{:.4} mm", required_radius_mm(carried))),
        "the radius the law demanded must appear: {text}"
    );
}
