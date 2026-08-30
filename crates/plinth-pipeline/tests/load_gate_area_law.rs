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
//! ported gate finds exactly that.** Three of its sixty-four emitted contacts
//! ship tips smaller than `pi * r^2 >= carried_area / X_ALLOWED` demands:
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
//! either: the corpus model `mini` finds the same two areas, 450.000 mm² and
//! 60.000 mm², because they are whole faces of a 30 x 30 mm form.
//!
//! **WARNING MODE (owner ruling, 2026-08-29).** The gate used to refuse this
//! result outright. It no longer does: it now records every violation with its
//! full evidence and lets emission proceed, so the corpus can be measured while
//! task #68 (the load-partition defect above) is fixed, and flips back to hard
//! refusal once it is. Until task #68 lands, any model with a downfacing face
//! above 37.70 mm² still SHIPS with a documented, non-blocking WARNING attached
//! rather than being silently accepted.
//!
//! **This file therefore pins a defect, not an approval.** When the load
//! partition is fixed, the assertion that `load_warnings` is non-empty fails
//! and this file is rewritten to assert `None`. Do not weaken the gate to make
//! it pass, and do not delete the pin to make the suite quiet.
//!
//! # What is demonstrated, both directions
//!
//! * The gate WARNS, end to end, inside the production door, and the model
//!   still reaches a written file — WARNING MODE's whole point is that
//!   emission is not refused for this.
//! * [`judge_emitted_load_law`] itself is UNCHANGED and still hard-refuses on
//!   these same rows when called directly (it is what warning mode's dispatch
//!   downgrades, not what it weakens) — see `crates/plinth-pipeline/src/gates.rs`'s
//!   `judge` function. This file calls it directly on the run's own recorded
//!   evidence to demonstrate that.
//! * The gate CLEARS the same record once exactly those three rows are given
//!   the tip the law demands, so the finding is those three rows and nothing
//!   else. Without this the finding would be consistent with a gate that
//!   flags everything, which is decoration of a different kind.
//! * The gate REFUSES again when one repaired row is put back under the law.
//!
//! # And the DISPATCH, which is a different layer
//!
//! The four points above are about [`judge_emitted_load_law`], which answers
//! the physics question. Whether that answer refuses, warns or clears is
//! decided one layer up, in `judge_load_gate` — and that layer is where the
//! 2026-08-29 review found warning mode downgrading faults it must never
//! downgrade. `the_gate_dispatch_refuses_a_real_record_it_cannot_read_or_that_names_a_contact_twice`
//! exercises that layer, on a real `make_box` run's own record:
//!
//! * every record-integrity fault, injected into one real row, REFUSES;
//! * a real record that names one contact twice in place of another REFUSES,
//!   where the old count-only completeness check cleared it;
//! * a MEASURED area-law violation still WARNS, carrying every row.
//!
//! **No engine is re-run and no geometry is re-derived below.** Every mutation
//! is one `f64` (or one `None`) in one row of a record the pipeline already
//! wrote.

#[path = "../examples/shared/first_artifact_chain.rs"]
mod chain;

use std::path::PathBuf;

use plinth_pipeline::{
    judge_emitted_load_law, judge_load_gate, GateRefusal, LoadGateDisposition, LoadLawFault,
    LoadLawFaultClass, PreWriteGate,
};
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

/// The routed-contact identity list the gate cross-checks a record against,
/// taken from the record itself.
///
/// **Deliberately circular, and only where the circularity is harmless.** The
/// tests below are about the AREA LAW, not about record completeness: they
/// mutate one `f64` in a row and ask what the law says. Feeding the record's
/// own ids means the completeness check passes and the arithmetic is what is
/// being measured. The completeness check is exercised where it is the
/// subject — against a routed list that disagrees with the record — in
/// `the_gate_dispatch_refuses_a_real_record_it_cannot_read_or_that_names_a_contact_twice`
/// below, and in the unit tests beside the gate.
fn described_ids(record: &[EmittedContactDisclosure]) -> Vec<u64> {
    record.iter().map(|row| row.contact_id).collect()
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
fn the_load_gate_warns_on_the_toed_slabs_own_record_and_clears_it_once_repaired() {
    let scratch = Scratch::new("toed-slab");
    let mesh = plinth_mesh::make_toed_slab().expect("procedural toed slab");
    let outcome = chain::run_procedural_model("make_toed_slab", mesh, scratch.path()).expect(
        "WARNING MODE (owner ruling, 2026-08-29): make_toed_slab ships three contacts that \
             violate the registered area law, but the Load gate no longer refuses emission for \
             it -- it records the violation and lets the model reach disk. If this expectation \
             now fails, either the load partition was fixed (rewrite this file to assert no \
             warning) or warning mode itself regressed back to a hard refusal -- see the module \
             doc.",
    );

    // ---- DIRECTION 1: the gate WARNS, end to end, and still emits --------
    assert!(
        scratch.path().exists(),
        "WARNING MODE must still write the artifact -- a warning is not a refusal"
    );
    let warnings = outcome.load_warnings.as_ref().expect(
        "the run's real record has three area-law violations; WARNING MODE must report them",
    );

    // ---- The record the run left on disk ---------------------------------
    //
    // Independent corroboration: the sealed ledger holds the same record the
    // gate judged, so the warning evidence is not something this test invented
    // by calling the gate a second time on different input.
    let body: serde_json::Value = serde_json::from_slice(
        &std::fs::read(scratch.ledger()).expect("the sealed ledger must exist on an emitted run"),
    )
    .expect("the sealed ledger must be valid JSON");
    let recorded: Vec<EmittedContactDisclosure> = serde_json::from_value(body["contacts"].clone())
        .expect("the sealed ledger carries the emitted-contact record");
    assert!(
        !recorded.is_empty(),
        "the run described no emitted contacts, so nothing below is a test of anything"
    );

    // `judge_emitted_load_law` ITSELF is unchanged and still hard-refuses on
    // these rows when called directly -- warning mode downgrades what the
    // dispatch in `gates::judge` does with an `Err`, it does not touch this
    // function's own arithmetic or its own `Result`. Demonstrated here so a
    // future edit that quietly softens the arithmetic (rather than only the
    // dispatch) is caught by this file, not just by the unit tests beside the
    // gate.
    let refusal = judge_emitted_load_law(&recorded, &described_ids(&recorded)).expect_err(
        "judge_emitted_load_law must still refuse on these rows directly -- warning mode is a \
         dispatch decision, not a weakened predicate",
    );
    let violations = refusal
        .load_law_violations()
        .expect("a load refusal carries the rows it refused")
        .to_vec();
    for violation in &violations {
        println!("LOAD-VIOLATION\t{violation}");
    }
    // And the pipeline's own WARNING MODE evidence must name exactly the same
    // rows the direct call does -- the dispatch is a relabelling, not a
    // resummarizing.
    assert_eq!(
        warnings.violations, violations,
        "the run report's warning evidence must be exactly what a hard refusal would have \
         carried, unweakened"
    );
    assert_eq!(warnings.judged, recorded.len());

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
    judge_emitted_load_law(&repaired, &described_ids(&repaired)).expect(
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

    let refusal = judge_emitted_load_law(&repaired, &described_ids(&repaired))
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

/// **The gate DISPATCH, on a real run's own record — the layer the previous
/// tests never reached.**
///
/// Everything above calls [`judge_emitted_load_law`], which answers the physics
/// question and has always been correct. The WARNING MODE decision is not
/// there: it is in [`judge_load_gate`], which `gates::judge`'s
/// `PreWriteGate::Load` arm calls and which decides whether an answer refuses,
/// warns, or clears. That decision shipped downgrading EVERY `LoadLawViolated`
/// to a warning, record-integrity faults included, and three sabotage lanes
/// plus a cross-family challenger missed it — because breaking what a check
/// looks at cannot detect a check that looks at the wrong thing, and because
/// every test in this suite stopped one layer below the decision.
///
/// So this test exercises the decision itself, and it does so on a record a
/// real chain actually produced rather than on hand-typed rows: it runs
/// `make_box` to a written artifact, reads the emitted-contact record back off
/// the sealed ledger the run left on disk, and puts each fault into ONE row of
/// that real record.
///
/// **The measured consequence this pins.** With one contact's tip radius made
/// undescribable, `plinth prepare` printed the gate's own warning text and then
/// `result EMITTED ... gate-refused=0`: the unreadable support shipped. The
/// ratchet then rewarded it — `ratchet_harness.rs` drops rows with missing
/// operands through a `filter_map`, so the vanished contact IMPROVED the total
/// scar area by exactly its own cross-section (0.0078540 mm² on both
/// `make_toed_slab` and `make_overhang_table`). Every assertion below is that
/// this cannot happen again.
#[test]
fn the_gate_dispatch_refuses_a_real_record_it_cannot_read_or_that_names_a_contact_twice() {
    let scratch = Scratch::new("dispatch");
    let mesh = plinth_mesh::make_box_default().expect("procedural box");
    let outcome = chain::run_procedural_model("make_box", mesh, scratch.path())
        .expect("make_box seals and emits, which is the premise of everything below");
    assert!(
        scratch.path().exists(),
        "the run must have emitted, or the record below is not a shipped run's record"
    );

    let body: serde_json::Value = serde_json::from_slice(
        &std::fs::read(scratch.ledger()).expect("the sealed ledger must exist on an emitted run"),
    )
    .expect("the sealed ledger must be valid JSON");
    let recorded: Vec<EmittedContactDisclosure> = serde_json::from_value(body["contacts"].clone())
        .expect("the sealed ledger carries the emitted-contact record");
    assert!(
        recorded.len() >= 2,
        "this run described {} emitted contact(s); the substitution case below needs at least \
         two, so nothing here would be a test of anything",
        recorded.len()
    );
    assert_eq!(
        recorded.len(),
        outcome.routed_contacts,
        "the record the run wrote must already cover every contact its sealed snapshot says it \
         routed, or the injections below are not the only thing being measured"
    );
    let routed = described_ids(&recorded);

    // The baseline: the dispatch does NOT refuse the record this run actually
    // shipped. Without this every refusal below would be consistent with a
    // dispatch that refuses everything.
    let baseline = judge_load_gate(&recorded, &routed)
        .expect("the run emitted, so the gate dispatch must not refuse the record it emitted on");
    println!("DISPATCH-BASELINE\t{baseline:?}");

    // ---- Every record-integrity fault, one at a time, in a REAL row -----
    type Injection = (
        &'static str,
        fn(&mut EmittedContactDisclosure),
        LoadLawFault,
    );
    let injections: [Injection; 6] = [
        (
            "carried area missing",
            |row| row.carried_area_mm2 = None,
            LoadLawFault::CarriedAreaNotDescribed,
        ),
        (
            "carried area negative",
            |row| row.carried_area_mm2 = Some(-1.0),
            LoadLawFault::CarriedAreaUnusable,
        ),
        (
            "carried area non-finite",
            |row| row.carried_area_mm2 = Some(f64::NAN),
            LoadLawFault::CarriedAreaUnusable,
        ),
        (
            "tip radius missing",
            |row| {
                row.tip_radius_mm = None;
                row.tip_diameter_mm = None;
            },
            LoadLawFault::TipRadiusNotDescribed,
        ),
        (
            "tip radius zero",
            |row| {
                row.tip_radius_mm = Some(0.0);
                row.tip_diameter_mm = Some(0.0);
            },
            LoadLawFault::TipRadiusUnusable,
        ),
        (
            "tip radius non-finite",
            |row| {
                row.tip_radius_mm = Some(f64::INFINITY);
                row.tip_diameter_mm = Some(f64::INFINITY);
            },
            LoadLawFault::TipRadiusUnusable,
        ),
    ];

    for (label, inject, expected_fault) in injections {
        let mut record = recorded.clone();
        inject(&mut record[0]);
        let subject = record[0].contact_id;

        let refusal = match judge_load_gate(&record, &routed) {
            Ok(disposition) => panic!(
                "{label}: the gate dispatch answered {disposition:?} on a record with one \
                 undescribable row. This is the shipped defect: warning mode downgraded a \
                 record-integrity fault, `plinth prepare` printed the warning and emitted \
                 anyway, and the ratchet then dropped the row and read the omission as an \
                 improvement"
            ),
            Err(refusal) => refusal,
        };
        assert_eq!(refusal.gate(), Some(PreWriteGate::Load), "{label}");

        let violations = refusal
            .load_law_violations()
            .unwrap_or_else(|| panic!("{label}: the refusal must carry the rows behind it"));
        let row = violations
            .iter()
            .find(|violation| violation.contact_id == subject)
            .unwrap_or_else(|| {
                panic!("{label}: the refusal must name the contact that was made undescribable")
            });
        assert_eq!(row.fault, expected_fault, "{label}");
        assert_eq!(
            row.fault.class(),
            LoadLawFaultClass::RunCannotShowItIsSafe,
            "{label}"
        );
        assert_eq!(
            refusal.unreadable_load_rows().len(),
            1,
            "{label}: exactly the injected row is unreadable"
        );
        let text = refusal.to_string();
        println!("DISPATCH-REFUSAL\t{label}\t{text}");
        assert!(
            text.contains("1 of them the run could not describe"),
            "{label}: the refusal must say the record could not be read: {text}"
        );
    }

    // ---- A duplicated row, where counting rows cleared it ---------------
    //
    // Name the first contact twice and drop the second. The record then has
    // exactly the length it had, so the count check the gate used to run
    // compares equal and clears, while a contact this run SHIPPED is never
    // judged by the area law at all.
    let mut duplicated = recorded.clone();
    let kept = duplicated[0].contact_id;
    let vanished = duplicated[1].contact_id;
    assert_ne!(kept, vanished, "the record's own ids must be distinct");
    duplicated[1] = duplicated[0];
    assert_eq!(
        duplicated.len(),
        routed.len(),
        "THE PREMISE: the old count-only completeness check compares these two lengths and \
         clears this record"
    );

    let refusal = judge_load_gate(&duplicated, &routed).expect_err(
        "a record that names one contact twice in place of another leaves a shipped contact \
         unjudged and must refuse",
    );
    assert!(
        matches!(refusal, GateRefusal::LoadRecordMismatch { .. }),
        "{refusal:?}"
    );
    let mismatches = refusal
        .load_record_mismatches()
        .expect("a record mismatch carries the ids the two walks disagree about");
    let text = refusal.to_string();
    println!("DISPATCH-REFUSAL\tduplicated row\t{text}");
    assert_eq!(mismatches.len(), 2, "{mismatches:?}");
    let duplicate = mismatches
        .iter()
        .find(|mismatch| mismatch.contact_id == kept)
        .expect("the duplicated id must be named");
    assert!(duplicate.is_duplicated(), "{duplicate:?}");
    assert_eq!((duplicate.routed, duplicate.described), (1, 2));
    let missing = mismatches
        .iter()
        .find(|mismatch| mismatch.contact_id == vanished)
        .expect("the id that vanished from the record must be named");
    assert!(missing.is_missing(), "{missing:?}");
    assert_eq!((missing.routed, missing.described), (1, 0));
    assert!(text.contains("DUPLICATED"), "{text}");
    assert!(text.contains("MISSING"), "{text}");

    // ---- And the other half of the ruling still holds -------------------
    //
    // A MEASURED area-law violation on the same real record still WARNS. The
    // fix must not have turned warning mode back into a hard refusal.
    let mut starved = recorded.clone();
    let carried = starved[0].carried_area_mm2.unwrap_or(1.0).max(1.0);
    let under = required_radius_mm(carried) * 0.5;
    starved[0].carried_area_mm2 = Some(carried);
    starved[0].tip_radius_mm = Some(under);
    starved[0].tip_diameter_mm = Some(under * 2.0);
    let disposition = judge_load_gate(&starved, &routed)
        .expect("a MEASURED area-law violation warns; warning mode is not withdrawn");
    let LoadGateDisposition::Warned(warnings) = disposition else {
        panic!("an under-sized tip must WARN, not clear: {disposition:?}");
    };
    assert!(
        warnings
            .violations
            .iter()
            .any(|violation| violation.contact_id == kept
                && violation.fault == LoadLawFault::TipSmallerThanTheAreaLawDemands),
        "the warning must carry the row that was starved, unweakened: {warnings:?}"
    );
    assert_eq!(warnings.judged, recorded.len());
    assert!(
        warnings
            .violations
            .iter()
            .all(|violation| violation.fault.class() == LoadLawFaultClass::RunShowsItIsUnsafe),
        "warning mode may only ever carry measured violations: {warnings:?}"
    );
}
