//! **SPEC-A4 design §G.1 stage 4 — what a production emission SPENT reaches
//! the run report, and cutting that wire turns this file red.**
//!
//! Stage 4 has landed in full: `plinth_support::run_support` IS the one-winner
//! transactional controller and the batch loop is deleted, so there is no
//! longer a second door to swap back to. What remains reversible in one line —
//! and what these tests exist for — is the WIRING: `seal_and_emit` reads the
//! §A.8 ledger off the sealed authority one frame before the authority is
//! consumed, and carries it onto the disposition. Delete that read, or move it
//! one frame later into `judged_emission`, and every digest, every gate verdict
//! and every island count stays identical while the cost ruling R10 traded the
//! failed-tries cap for silently stops being reported.
//!
//! So the proof is the MARKER: [`RepairRunReport`] present, on both an emitting
//! model that needed repair and one that needed none. `None` here means the
//! wire is cut.
//!
//! **Read through the public pipeline API, not through `plinth-support`.** The
//! assertions below travel `plinth_pipeline::prepare::run_from_oriented` ->
//! `seal_and_emit` -> `ChainOutcome`, which is the same path production takes.
//! A test that called `run_support` itself would prove the controller works and
//! prove nothing at all about what the pipeline does with what it produced,
//! which is the entire content of these two tests.
//!
//! Both fixtures are procedural — built from `plinth_mesh::make_*` against a
//! checked-in `corpus/baseline/*.pipeline.json` — so this file runs on a clean
//! worktree with no `corpus/models/` present (task #16's known hole).

use crate::chain;

use std::path::PathBuf;

use plinth_mesh::{make_toed_slab, make_uncovered_rim_table};

/// A per-test scratch path that is removed on the way in and on the way out.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "plinth-a4-production-door-{label}-{}.chitubox",
            std::process::id()
        ));
        let scratch = Self(path);
        scratch.clear();
        scratch
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }

    /// The artifact AND the evidence the chain writes beside it — the FU-57
    /// coverage sidecar and SPEC-A4 §D.2's sealed repair ledger. Leaving either
    /// behind would let one test read another run's attribution.
    fn clear(&self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(self.0.with_extension("coverage.json"));
        let _ = std::fs::remove_file(self.0.with_extension("coverage.repair.json"));
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        self.clear();
    }
}

/// **The revert-proof, on a model that NEEDS repair.**
///
/// `make_uncovered_rim_table` does not cover on its baseline layout: it closes
/// only after the controller commits repair rows. Under the transactional loop
/// that costs measurable trials and commits, and the §A.8 report carries them
/// out to the run report. Cut the read in `seal_and_emit` and there is no report to
/// carry.
///
/// The assertions are on the SHAPE of what the loop spent, not on frozen
/// counts, because the count is a property of the geometry and the ranking and
/// will move when either moves; what may not move is that a production
/// emission's cost is stated at all. The three that must hold:
///
/// * a report exists — the marker itself;
/// * at least one step COMMITTED — this model reached its artifact through
///   repair, so a report claiming zero commits would be describing a different
///   run than the one that just emitted;
/// * every commit was a trial — the two counters come from the same loop and
///   `commits > trials` would mean the accounting is invented.
#[test]
fn the_production_door_reports_what_the_one_winner_controller_spent() {
    let scratch = Scratch::new("uncovered-rim-table");
    let outcome = chain::run_procedural_model(
        "make_uncovered_rim_table",
        make_uncovered_rim_table().expect("procedural uncovered-rim table"),
        scratch.path(),
    )
    .expect("make_uncovered_rim_table emits through coverage repair");

    let repair = outcome.repair_report.as_ref().expect(
        "SPEC-A4 §G.1 stage 4: the controller reports on every path, and `seal_and_emit` \
         carries that report onto the disposition. `None` here means the read was deleted or \
         moved past the frame the authority lives in — the revert this test exists to catch",
    );

    assert!(
        repair.commits >= 1,
        "this fixture reaches its artifact THROUGH repair, so the run report must say at least \
         one repair step earned its place: {repair:?}"
    );
    assert!(
        repair.trials >= repair.commits,
        "every commit is a trial, so an accounting with more commits than trials is not a \
         measurement of anything: {repair:?}"
    );
    assert_eq!(
        repair.trials - repair.commits,
        repair.rejections_by_class.values().sum::<usize>(),
        "every trial that did not commit must appear in the rejection histogram by class, or \
         the cost R10 traded the failed-tries cap for is not fully stated: {repair:?}"
    );

    // The operator-visible line R10's trade depends on, reached through the
    // same value the CLI prints. A report that exists but summarises to
    // nothing is a report nobody can read.
    let line = repair.summary_line();
    for field in ["trials=", "commits=", "remaining=", "wall_total="] {
        assert!(
            line.contains(field),
            "the run report's per-model repair line must carry {field}: {line}"
        );
    }
}

/// **The same marker on a model that needs NO repair, which is the harder
/// half.**
///
/// `make_toed_slab` covers on its baseline layout and never enters a
/// transaction (design §E.5 case 6), so its emitted artifact is bit-identical
/// to the pre-controller engine — there is no digest, no island count and no
/// gate verdict on this model that can tell whether the controller ran at all.
/// The report is the only thing that can, and it is present and reads zero.
///
/// That is why `trials=0 commits=0` is asserted as a VALUE rather than allowed
/// to be `None`: "this model paid nothing" and "nobody was counting" are
/// different facts, and a cut wire produces the second one.
#[test]
fn a_model_that_needs_no_repair_still_reports_that_it_spent_nothing() {
    let scratch = Scratch::new("toed-slab");
    let outcome = chain::run_procedural_model(
        "make_toed_slab",
        make_toed_slab().expect("procedural toed slab"),
        scratch.path(),
    )
    .expect("make_toed_slab emits on its baseline layout");

    let repair = outcome.repair_report.as_ref().expect(
        "a sealing model reports too: `None` here is a cut wire, not a model that paid nothing",
    );
    assert_eq!(
        (repair.trials, repair.commits),
        (0, 0),
        "the toed slab covers on its baseline layout, so the controller must have cost it \
         nothing — a non-zero count here means a model that never needed a transaction entered \
         one, and its frozen digest is no longer safe: {repair:?}"
    );
    assert!(
        repair.rejections_by_class.is_empty(),
        "no trial means no rejection to classify: {repair:?}"
    );
    assert_eq!(
        repair.wall_time_total_ms, 0,
        "no trial means no trial time to total: {repair:?}"
    );
}

/// **SPEC-A4 §D.2 — the exact attribution is readable from the FILE, for a
/// model that emitted.**
///
/// §D's claim is that a one-winner step changes the plan by exactly one contact,
/// so the difference between its before- and after-measurement IS that
/// contact's contribution: "commit 3, row (island 3, component 0, candidate
/// 118) at (10.4, 7.6, 19.0), scored 0.46 mm² of which 0.36 mm² came from
/// island 5 … closed island 5's 0.36 mm² patch". Until this landed that
/// sentence could be produced only for a REFUSED model — a sealing model's step
/// stream lived on the authority and died with it one frame after
/// `seal_and_emit` read the report — which is the wrong way round, because the
/// sealing model is the one whose plan gets printed.
///
/// **The revert this guards** is the one-line kind: delete the
/// `persist_sealed_repair_ledger` call, or write only the commits, or summarise
/// the stream on its way out, and every digest, gate verdict and artifact byte
/// stays identical while the attribution stops being answerable. So the
/// assertions are on the FILE, re-read from disk by a second reader, and on the
/// three things §D.2 says it must be able to state: which row, what it was
/// credited for, and what it closed.
///
/// `make_uncovered_rim_table` is used because it COMMITS — the toed slab's
/// honestly empty ledger is pinned in `prepare_folder.rs`, and an empty stream
/// cannot prove that a populated one survives the trip.
///
/// **It replaced `make_overhang_table` here on 2026-08-26**, when that model
/// stopped needing repair at `3a6526f` and left both tests in this file
/// reading a ledger with nothing in it. The fixture changed; no assertion did.
#[test]
fn the_sealed_ledger_on_disk_names_the_row_that_closed_the_hole() {
    let scratch = Scratch::new("uncovered-rim-table-ledger");
    let outcome = chain::run_procedural_model(
        "make_uncovered_rim_table",
        make_uncovered_rim_table().expect("procedural uncovered-rim table"),
        scratch.path(),
    )
    .expect("make_uncovered_rim_table emits through coverage repair");

    let ledger = outcome
        .repair_ledger
        .as_ref()
        .expect("an emitting model must record where its repair ledger was written");
    assert_eq!(
        ledger.write_error, None,
        "the ledger must reach disk: {ledger:?}"
    );
    assert!(
        ledger.commits >= 1,
        "this fixture reaches its artifact THROUGH repair, so its ledger must hold at least one \
         committed step: {ledger:?}"
    );

    let bytes = std::fs::read(&ledger.path).expect("the sealed repair ledger must exist on disk");
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).expect("the sealed repair ledger must be valid JSON");
    assert_eq!(
        json["schema"], "plinth.coverage-repair.v1",
        "the ledger carries its own versioned schema tag"
    );
    assert_eq!(json["model"], "make_uncovered_rim_table");
    assert_eq!(
        json["commits"], ledger.commits,
        "the report's count and the file's count are the same fact"
    );

    let steps = json["steps"]
        .as_array()
        .expect("the stream is an array of steps");
    assert_eq!(
        steps.len(),
        ledger.steps,
        "every trial must reach the file, committed or not"
    );
    let committed: Vec<&serde_json::Value> = steps
        .iter()
        .filter(|step| !step["commit"].is_null())
        .collect();
    assert_eq!(
        committed.len(),
        ledger.commits,
        "the file's committed rows and its commit count must agree"
    );

    for step in &committed {
        // **Which row** — the pool identity, by name, not an index into
        // something the reader does not have.
        let row = &step["row_id"];
        assert!(
            row["candidate_id"].is_number()
                && row["owner_island_id"].is_number()
                && row["component_id"].is_number()
                && row["face_id"].is_number(),
            "a committed step must name its repair-pool row on the wire: {step}"
        );
        assert!(
            row["source_kind"].is_string(),
            "the row's provenance must be readable, not a bare number: {step}"
        );
        // **Where it sits** — three finite millimetre coordinates.
        let seat = step["seat_mm"]
            .as_array()
            .expect("the seat is three coordinates");
        assert_eq!(seat.len(), 3);
        assert!(
            seat.iter()
                .all(|value| value.as_f64().is_some_and(f64::is_finite)),
            "a seat must be three finite millimetre coordinates: {step}"
        );

        // **What it was credited for** — §D's `scored_from`, per ROUTE, with
        // each route's share. This is the field the cross-route sentence is
        // made of, and a serializer that dropped it would leave the record
        // saying only "0.46 mm²" with no way to ask where that came from.
        let shares = step["scored_from"]
            .as_array()
            .expect("scored_from is an array");
        assert!(
            !shares.is_empty(),
            "a committed step covered area, so at least one route was credited: {step}"
        );
        let credited: f64 = shares
            .iter()
            .map(|share| share["area_mm2"].as_f64().expect("a share is an area"))
            .sum();
        let scored = step["marginal_area_mm2"]
            .as_f64()
            .expect("the global score is an area");
        assert!(
            (credited - scored).abs() < 1e-9,
            "the per-route shares must sum to the GLOBAL score they explain; {credited} mm² \
             against {scored} mm²: {step}"
        );

        // **What it closed** — both sides of the transaction, and the per-route
        // deltas that say which route moved. §D.1: the before/after difference
        // IS this contact's contribution.
        let before = step["uncensored_before_mm2"]
            .as_f64()
            .expect("clause 3's before-area");
        let after = step["uncensored_after_mm2"]
            .as_f64()
            .expect("a committed step was measured on both sides");
        assert!(
            after < before,
            "a committed step covered real area, and the file must say so: {before} -> {after} \
             mm²: {step}"
        );
        assert!(
            step["route_deltas"]
                .as_array()
                .expect("route_deltas is an array")
                .iter()
                .any(|delta| {
                    delta["uncensored_area_before_mm2"].as_f64()
                        != delta["uncensored_area_after_mm2"].as_f64()
                        || delta["failing_area_before_mm2"].as_f64()
                            != delta["failing_area_after_mm2"].as_f64()
                }),
            "a committed step moved at least one route's evidence, and the record names which: \
             {step}"
        );
        assert_eq!(
            step["outcome"], "committed",
            "a row carrying a commit number must be recorded as committed: {step}"
        );
    }

    // The stream is the run's, not a story about it: the last committed step's
    // own recorded objective must show the zero failing patches that sealing
    // means.
    let last = committed.last().expect("at least one commit");
    assert_eq!(
        last["objective_after"]["failing_patch_count"], 0,
        "this run sealed, so the file's last committed step must record zero failing patches: \
         {last}"
    );
}
