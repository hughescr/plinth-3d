//! Design phase E4: `plinth_pipeline::prepare` over a real folder.
//!
//! `tests/first_artifact.rs` proves the *chain* joins up on one model. This
//! proves the **executor** does: a folder of source `.stl` in, one
//! `.chitubox` per model that passes the pre-write gates out, and one typed
//! disposition per model either way.
//!
//! Every fixture here is either a **committed** file under `corpus/baseline/`
//! or a **procedural** mesh `plinth-mesh` generates and this file writes out as
//! a real binary `.stl` ([`Scratch::place_procedural`]) — never a provisioned
//! corpus model, so this test runs in a fresh clone with nothing symlinked.
//! `corpus/baseline/*.prepped.stl` are checked in; `corpus/models/` is not.
//!
//! **A finding this test surfaced, recorded rather than papered over — and
//! rewritten twice since.** It used to read: no checked-in `.prepped.stl` is
//! refused by the pre-write gates, because both
//! `make_overhang_table.prepped.stl` and `Sphere.prepped.stl` seal and emit
//! when run through the whole chain, while the *procedural*
//! `make_overhang_table()` and the provisioned `corpus/models/Sphere.stl` are
//! refused for uncovered islands (`tests/first_artifact.rs`). That stopped
//! being true of `Sphere.prepped.stl` on 2026-08-10 (the cavity-field
//! exterior fix stopped its cavity breaching the wall at its poles, so its
//! drilled shell seals and then fails coverage on its own merits), and moved
//! a second time on 2026-08-12 when FU-53 landed (the exact nearest-surface
//! cavity selection plus `HOLLOW_CAVITY_MARCH_MARGIN_MM` shrinks the cavity
//! honestly, and on this doubly-decimated, doubly-oriented fixture the
//! resulting support demand exceeds the sealed run's registered routing
//! capacity before the gate is ever reached — `DrilledShellRefusedBySupport`
//! fires and the existing OQ-1/FU-34 reserve substitution converts it to an
//! emitted solid). **`feature/coverage-closure` (FU-56/FU-57) landed the same
//! day and does not move this fixture a third time:** measured directly
//! against the merged tree, `Sphere.prepped.stl`'s attempt 0 still fails
//! during `Routing`, which precedes `CoverageMeasurement` in the stage order
//! and keeps the pre-FU-56 `Failed(phase)` semantics by the controller's own
//! design (attempt 0's plan is the model's own, not the controller's
//! extension of it) — so the coverage-closure controller never gets a look
//! at this fixture and the FU-53-only route stands. `make_raking_blade.prepped.stl`
//! DID move on 2026-08-12 (FU-56 measures routed coverage inside
//! `run_support`, and that fixture's two residual islands have no
//! repair-pool candidate the CCO transcript ever admits, so it is now a
//! genuine coverage-closure refusal). See
//! `a_model_with_no_pinned_baseline_is_refused_rather_than_given_a_neighbours`
//! for the full attribution. No cap, gate or bound moved to produce any of
//! this — each move is the geometry, or the measurement, changing under an
//! unmoved rule.
//!
//! **And it moved a fourth time on 2026-08-14, back to the original reading.**
//! Honouring the written 0.16 mm² A3 censor (docs/specs/coverage-repair.md §4)
//! showed that `make_raking_blade.prepped.stl`'s whole residual was ONE
//! 0.0600 mm² patch — well under a single 0.4 mm voxel footprint. Its
//! 2026-08-12 "genuine coverage-closure refusal" was genuine only relative to
//! a predicate that failed on any dust at all; it now emits, and **no checked-in
//! `.prepped.stl` is a coverage refusal any more**, exactly as the original
//! finding said. The refusal fixture is therefore the PROCEDURAL raking blade,
//! written out as an `.stl` here — see [`refusal_fixture`], which records the
//! measured before/after. Once again no cap, gate or bound moved: what moved is
//! a comparison onto the number the spec already wrote down.
//!
//! Deliberately absent: any new frozen digest (design record OQ-3). Everything
//! asserted is a completion, disposition or path property that a changed
//! member count cannot break.

use std::path::{Path, PathBuf};

use plinth_pipeline::{
    prepare, registered_decimation_budget, registered_support_request, ModelPreparationOutcome,
    OrientationPolicy, OutputProfile, PipelineExitCode, PreWriteGate, PrepareReport,
    PrepareRequest, StageStop, SupportedGeometry,
};

use crate::chain;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn committed_fixture(name: &str) -> PathBuf {
    let path = workspace_root().join("corpus/baseline").join(name);
    assert!(
        path.is_file(),
        "committed fixture {} is missing; corpus/baseline is checked-in evidence",
        path.display()
    );
    path
}

/// A per-test scratch folder, removed on the way in and on the way out.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("plinth-prepare-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("in")).expect("scratch input folder");
        Self(path)
    }

    fn input(&self) -> PathBuf {
        self.0.join("in")
    }

    /// The output folder is deliberately NOT created: `prepare` owns creating
    /// it, and that is part of what this test gates.
    fn output(&self) -> PathBuf {
        self.0.join("out")
    }

    /// Place a committed fixture into the input folder under a chosen stem, so
    /// the stem -> artifact-name mapping is visible rather than incidental.
    fn place(&self, fixture: &str, stem: &str) {
        std::fs::copy(
            committed_fixture(fixture),
            self.input().join(format!("{stem}.stl")),
        )
        .expect("place fixture");
    }

    /// Place a PROCEDURAL mesh into the input folder as a real binary `.stl`,
    /// under a chosen stem.
    ///
    /// This needs no provisioned corpus and adds no binary file to the repo —
    /// the mesh is generated by `plinth-mesh` at test time — so it keeps this
    /// file's fresh-clone guarantee while reaching geometry no committed
    /// `.prepped.stl` provides.
    ///
    /// **Why the coordinates are snapped to f32 first.** A binary STL stores
    /// f32, and `plinth_mesh::write_binary_stl` deliberately refuses to write
    /// an f64 coordinate that would not round-trip (`LossyF32Encoding`) rather
    /// than silently losing it. Snapping here is therefore not an alteration of
    /// the fixture: it is exactly the mesh an `.stl` of this model contains,
    /// made explicit at the one place the narrowing happens. The alternative —
    /// letting the writer refuse — would just mean this fixture could not be a
    /// file at all.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn place_procedural(&self, mesh: &plinth_mesh::Mesh, stem: &str) {
        let verts: Vec<[f64; 3]> = mesh
            .verts()
            .iter()
            .map(|vertex| {
                [
                    f64::from(vertex[0] as f32),
                    f64::from(vertex[1] as f32),
                    f64::from(vertex[2] as f32),
                ]
            })
            .collect();
        let snapped = plinth_mesh::Mesh::new(verts, mesh.tris().to_vec())
            .expect("an f32-snapped procedural fixture is still a valid mesh");
        plinth_mesh::write_binary_stl(self.input().join(format!("{stem}.stl")), &snapped)
            .expect("write procedural fixture");
    }

    fn write(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.input().join(name), bytes).expect("write scratch input");
    }
}

/// The refusal fixture every coverage-refusal test in this file uses.
///
/// **Moved off `make_raking_blade.prepped.stl` on 2026-08-14**, when the
/// written 0.16 mm² A3 censor was honoured (docs/specs/coverage-repair.md §4;
/// `plinth_support::coverage_patch_blocks_verdict`). That fixture's entire
/// residual was ONE patch of **0.0600 mm²** — 0.375 of a single 0.4 mm voxel
/// footprint — measured off its own FU-57 sidecar. It was a "refusal" only
/// because the blocking predicate failed on `uncovered_area_mm2 > 0.0`, i.e.
/// only because of the defect that change fixed. It now emits, correctly, and
/// no committed `.prepped.stl` is a coverage refusal any more.
///
/// The PROCEDURAL raking blade is: run through the same pinned baseline and the
/// same registered decimation, it exhausts all 8 closure attempts on island 5 —
/// four faces, one patch, `NoRoutedContact`, **5.135 mm² uncovered**, which is
/// 32× the censor. Repair rows were nominated for it on every attempt out of
/// the fourteen its island had, and the CCO transcript admitted none of them.
/// (The per-attempt row counts were first measured while the repair worklist
/// was itself censored; from the same day's follow-up ruling the worklist is
/// UNCENSORED, so the blade's censored island 3 is nominated for as well and
/// the exact row counts are not pinned here. The refusal is unchanged:
/// `attempts` 8, `AttemptsExhausted`, island 5 at 5.135 mm².) That is a
/// genuinely uncovered
/// island, and it is a STRICTLY better refusal fixture than the one it
/// replaces: the old one could not tell a censored patch from a real hole, and
/// this one fails on an area no censor at the registered pitch could reach.
///
/// Nothing here touches the radius, the censor or any bound. The refusal is
/// measured, as it was before.
fn refusal_fixture(scratch: &Scratch, stem: &str) {
    let mesh = plinth_mesh::make_raking_blade().expect("procedural raking blade");
    scratch.place_procedural(&mesh, stem);
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A request whose orientation is pinned per model, by source stem — or, when
/// no baselines are named, the default dense sweep.
///
/// `OrientationPolicy::Pinned` is a **map**: a pinned baseline records one
/// mesh's rotation and `plinth_orient::run` cannot tell that it was handed a
/// different mesh, so each model in a folder names its own or is refused. The
/// dense sweep has no such hazard — it measures the mesh in front of it — which
/// is why it, and not a pin, is the default.
fn request(scratch: &Scratch, orientation: &[(&str, &str)]) -> PrepareRequest {
    let printer = OutputProfile::Saturn4Ultra
        .resolve()
        .expect("M0 Saturn profile")
        .clone();
    let orientation = if orientation.is_empty() {
        OrientationPolicy::DenseSweep
    } else {
        OrientationPolicy::Pinned(
            orientation
                .iter()
                .map(|(stem, baseline)| {
                    (
                        (*stem).to_owned(),
                        plinth_orient::PinnedBaseline {
                            pipeline_report: committed_fixture(baseline),
                        },
                    )
                })
                .collect(),
        )
    };
    PrepareRequest {
        input_models_dir: scratch.input(),
        output_dir: scratch.output(),
        orientation,
        // LANE 2: this suite pins the LEGACY engine, named rather than defaulted.
        placement: plinth_pipeline::Placement::Legacy,
        output_profile: OutputProfile::Saturn4Ultra,
        decimation_budget: registered_decimation_budget().expect("registered budget"),
        hollow: plinth_hollow::HollowRequest::with_registered_wall_default(printer.clone())
            .expect("registered wall"),
        support: registered_support_request(&printer).expect("registered lift"),
        no_cache: false,
        stop_after: None,
    }
}

/// The single-model pinned request every fixture test here uses: one stem, the
/// baseline that belongs to it.
fn pinned(scratch: &Scratch, stem: &str, baseline: &str) -> PrepareRequest {
    request(scratch, &[(stem, baseline)])
}

fn outcome<'a>(report: &'a PrepareReport, model: &str) -> &'a ModelPreparationOutcome {
    &report
        .models
        .iter()
        .find(|entry| entry.model.as_str() == model)
        .unwrap_or_else(|| panic!("model {model} is missing from the report"))
        .outcome
}

/// **How many contacts the healthy fixture refuses — MEASURED, re-derived
/// 2026-09-08 for the tip-seating contact-law change (`26eaab7`, `dc3dae7`,
/// `f8656cf`).**
///
/// Zero until Craig's scar bound landed (2026-08-20): the one contact the
/// bound refused needed `0.650630996` mm radius to attest, 1.63x
/// `TIP_RADIUS_MAX_GROWN_SCAR_MM` (0.4 mm), so the count went 0 -> 1. The
/// legacy engine's seat depth minimum has since moved from a flat 0.3625 mm
/// floor to a derived layer+pixel allowance (~0.1 mm), and `seat_depth_of`'s
/// first term with it, so that SAME contact now needs less penetration to
/// attest and its required scar radius drops back inside the 0.4 mm bound —
/// the count returns to **0**, MEASURED by reading `abandoned_contacts.len()`
/// back with this assertion loosened to a print. The load assertion below
/// (`abandoned_mm2 == 0.0`) never moved: this contact carried 0.000 mm² in
/// both states, so its refusal or attestation never changed what the fixture
/// abandons in AREA, only in count. This count is what stops a second,
/// unrelated refusal from slipping in behind a zero-area sum now that
/// `is_empty()` can no longer be demanded.
const HEALTHY_FIXTURE_ABANDONED_CONTACTS: usize = 0;

/// **The load-bearing test: a folder in, a real `.chitubox` out.**
///
/// It also proves the executor's other half in the same run — the deliberately
/// corrupt `zz-not-a-model.stl` is refused *individually*, and the good model
/// beside it still reaches disk. One bad model must never cost the folder its
/// artifacts, and sorting the sources puts the bad one last so a bug that
/// aborted the loop would still have emitted; the second fixture is named to
/// sort *after* the good one for exactly that reason... and a third, sorting
/// *before* it, closes the other direction.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn a_folder_of_models_emits_one_artifact_each_and_one_bad_model_costs_only_itself() {
    let scratch = Scratch::new("folder");
    scratch.place("make_toed_slab.prepped.stl", "mini");
    scratch.write("aa-not-a-model.stl", b"neither binary nor ASCII STL");
    scratch.write("zz-not-a-model.stl", b"nor is this one");

    let report = prepare(pinned(&scratch, "mini", "make_toed_slab.pipeline.json"))
        .expect("the run itself must succeed even though two models in it cannot");

    // The output folder did not exist; `prepare` owns creating it.
    assert!(
        scratch.output().is_dir(),
        "prepare must create its own output folder"
    );

    // Deterministic order: sorted by source path, and the report follows it.
    assert_eq!(
        report
            .models
            .iter()
            .map(|entry| entry.model.as_str())
            .collect::<Vec<_>>(),
        ["aa-not-a-model", "mini", "zz-not-a-model"],
        "models must be reported in the executor's own sorted source order"
    );

    // The good model reached a real artifact at the stem-derived path.
    let artifact = scratch.output().join("mini.chitubox");
    let ModelPreparationOutcome::Artifact {
        emit,
        geometry,
        hollow_disposition,
        cavity_repair,
        wall_mm,
        realized_wall,
        savings_shortfall,
        filled_back_chambers_ml,
        ownerless_islands,
        contacts,
        repair_report,
        repair_ledger,
        pending_gates,
        load_warnings,
        coverage_warning,
    } = outcome(&report, "mini")
    else {
        panic!(
            "mini must reach an artifact; it reported {:?}",
            outcome(&report, "mini")
        );
    };
    // WARNING MODE (owner ruling, 2026-09-01): the coverage gate no longer
    // refuses an uncovered model, so an emitted artifact can now carry a named
    // coverage gap. The healthy fixture must not: it is the model whose
    // coverage verdict is ACCEPTED, and a warning appearing here would mean the
    // gate started reporting a gap on a model that has none.
    assert_eq!(
        coverage_warning.as_deref(), None,
        "the healthy fixture's coverage verdict is accepted, so it must carry no coverage gap"
    );
    // WARNING MODE (owner ruling, 2026-08-29): `PreWriteGate::Load` no longer
    // refuses this run for it, but "mini" (`make_toed_slab.prepped.stl`, this
    // config's own decimated fixture — a different candidate set from the
    // in-repo procedural generator `load_gate_area_law.rs` pins) genuinely
    // ships two contacts that do not satisfy the registered area law. This
    // pins a measured defect, not an approval: when the load partition (task
    // #68) is fixed, this assertion fails and belongs rewritten to `None`.
    let warnings = load_warnings
        .as_ref()
        .expect("this fixture's real emitted-contact record has two area-law violations");
    // **`judged` re-derived 2026-09-08 for the tip-seating contact-law change**
    // (`26eaab7`, `dc3dae7`, `f8656cf`). `judged` is `emitted_contacts.len()`
    // (`plinth-pipeline/src/gates.rs:1001`) — one count per contact this run
    // actually shipped, NOT the fixture's fixed geometry. It moves in lockstep
    // with `HEALTHY_FIXTURE_ABANDONED_CONTACTS` above by construction (every
    // routed contact is either emitted-and-judged or abandoned): that count
    // went 1 -> 0 under the same law change (a rescue that used to need a
    // wider-than-bound scar now attests within it), so one more contact ships
    // and `judged` goes 18 -> 19. Confirmed by instrumentation, not inferred:
    // printed from a live run before this refresh was written. The CLAIM this
    // test makes — "the load gate WARNS rather than refuses, and names exactly
    // the violations a real emitted-contact record has" — is unchanged; only
    // the emitted-contact count moved, and `violations.len() == 2` and both
    // violations' `carried_area_mm2`/`emitted_tip_radius_mm` are unaffected
    // because neither of the two violating contacts is the one that started
    // attesting.
    assert_eq!(warnings.judged, 19);
    assert_eq!(warnings.violations.len(), 2);
    for violation in &warnings.violations {
        assert_eq!(
            violation.fault,
            plinth_pipeline::LoadLawFault::TipSmallerThanTheAreaLawDemands
        );
        assert_eq!(violation.carried_area_mm2, Some(59.999_999_192_329_6));
        assert_eq!(violation.emitted_tip_radius_mm, Some(1.0));
    }
    assert_eq!(emit.artifact_path, artifact);
    // SPEC-A4 §G.1 stage 4. The folder executor's own door reaches the
    // one-winner controller, so every artifact it produces states what the
    // repair controller spent — `trials=0 commits=0` for a model that covered
    // on its baseline layout. `None` means the read in `seal_and_emit` was cut,
    // which makes this the second, independent revert-proof beside
    // `tests/a4_production_door.rs`.
    let repair = repair_report.as_ref().expect(
        "the production door reports what coverage repair cost; `None` means `seal_and_emit` \
         no longer reads the ledger off the sealed authority",
    );
    assert!(
        repair.trials >= repair.commits,
        "every commit is a trial: {repair:?}"
    );

    // **SPEC-A4 §D.2 — the sealed-run attribution reaches DISK, on a model that
    // spent nothing.**
    //
    // This half is the awkward one and it is the half worth pinning here: the
    // toed slab covers on its baseline layout, so its ledger is honestly empty.
    // Writing nothing at all would make "this model paid nothing" and "nobody
    // wrote it down" look identical on disk, which is the same distinction §A.8
    // prints a line of zeros for. Before this landed, `seal_and_emit` deleted
    // any stale sidecar on the way in and wrote NOTHING for a sealing model, so
    // an emitting model's attribution existed nowhere.
    let ledger = repair_ledger
        .as_ref()
        .expect("an emitting model must record where its repair ledger was written");
    assert_eq!(
        ledger.path,
        scratch.output().join("mini.coverage.repair.json"),
        "the ledger belongs beside the artifact, at the model's own stem"
    );
    assert_eq!(ledger.write_error, None, "the ledger must reach disk");
    assert_eq!(
        (ledger.trials, ledger.commits, ledger.steps),
        (0, 0, 0),
        "the toed slab covers on its baseline layout, so its ledger is honestly empty"
    );

    // Disclosure half of the pre-write gate's #1 finding, through the real
    // folder executor rather than a constructed fixture: this artifact was
    // authorized on the gates that reach a verdict, and the run report must
    // name every other gate as not yet run rather than staying silent about it.
    let ported = [PreWriteGate::UncoveredIslands, PreWriteGate::Load];
    let expected_pending: Vec<PreWriteGate> = PreWriteGate::ALL
        .into_iter()
        .filter(|gate| !ported.contains(gate))
        .collect();
    assert_eq!(
        pending_gates, &expected_pending,
        "an emitted artifact must name every named gate that did not run"
    );
    let ledger_json: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&ledger.path).expect("the sealed repair ledger must exist on disk"),
    )
    .expect("the sealed repair ledger must be valid JSON");
    assert_eq!(
        ledger_json["schema"], "plinth.coverage-repair.v1",
        "the ledger carries its own versioned schema tag, as the refusal sidecar does"
    );
    assert_eq!(ledger_json["model"], "mini");
    assert_eq!(ledger_json["commits"], 0);
    assert_eq!(
        ledger_json["steps"]
            .as_array()
            .expect("steps is an array even when empty")
            .len(),
        0,
        "a model that never entered a transaction has no step to attribute"
    );
    // **Task #71 — the disclosure that lets a byte movement be attributed.**
    //
    // The gap: `commits`, `trials`, the §A.8 counters and the §D stream are all
    // properties of the repair SEARCH. None is a function of the load partition
    // or of tip sizing, so two runs that partition this model's load
    // differently, or size the same contact's tip differently, wrote different
    // artifacts and printed identical summaries. This fixture spends ZERO
    // trials, so before this every disclosed field on it was a constant and the
    // artifact's bytes were unexplainable by construction.
    let contact_rows = ledger_json["contacts"]
        .as_array()
        .expect("the sealed ledger carries one row per routed contact");
    assert!(
        !contact_rows.is_empty(),
        "a model that emitted supports must disclose the contacts that carry them: \
         {ledger_json}"
    );
    // **The disclosure must not be able to move silently EITHER**, or its
    // attribution is a misattribution — which is worse than the silence #71
    // names, because a reader would act on it. Finiteness, positivity and the
    // exact doubling prove a row is well FORMED; they do not prove it is TRUE.
    // A projection that multiplied every carried area, or read the radius off
    // the rootward end of the member instead of the terminalward end, would
    // still print finite positive numbers and still satisfy `diameter == 2 x
    // radius`. So each of the two substantive fields is checked against a
    // source that does not pass through the projection that wrote it:
    //
    // * the RADIUS against the artifact on disk, below — the one source
    //   downstream of growth, rewire, the brace resize, lowering and the emit;
    // * the AREA against the registered area law, here. The area comes from the
    //   accepted plan and the radius from the final live forest, joined only by
    //   contact id, and the law is what ties the two together. Nothing is
    //   re-derived: `registered_tip_radius_mm_for_carried_area` calls the same
    //   `FrozenTipLoadPolicy::tip_radius_mm` that sized every stub, with both
    //   registered clamps inside the answer.
    //
    // A tip the run GREW is the one documented exception, and it is exempted by
    // name rather than by loosening the rule for everyone: grow-to-attest emits
    // a radius at or above the law's, so those rows are held to `>=` and, more
    // sharply, to agreeing exactly with the grown-tip disclosure's own reading
    // of the same tip.
    let grown_tip_radius_by_contact: std::collections::BTreeMap<u64, f64> = contacts
        .as_ref()
        .grown_contact_tips
        .iter()
        .map(|grown| (u64::from(grown.contact_id()), grown.emitted_tip_radius_mm()))
        .collect();
    let mut rows_a_carried_area_change_would_move = 0usize;
    for contact in contact_rows {
        let contact_id = contact["contact_id"].as_u64().expect("a contact id");
        // `null` here is the disclosure saying "this routed row is not in the
        // accepted plan" — a real state it must be able to say, and one the
        // healthy fixture must never be in.
        let carried = contact["carried_area_mm2"].as_f64().expect(
            "an emitted contact resolves in the accepted plan, so its carried area is known; \
             null means the transcript row named a contact the plan no longer holds",
        );
        assert!(
            carried.is_finite() && carried >= 0.0,
            "a carried area is a finite non-negative mm²: {contact}"
        );
        assert!(
            contact["position_mm"].is_array(),
            "a contact whose area resolved must disclose the seat it resolved to: {contact}"
        );
        let radius = contact["tip_radius_mm"]
            .as_f64()
            .expect("an emitted contact resolves to a live member, so its tip radius is known");
        let diameter = contact["tip_diameter_mm"]
            .as_f64()
            .expect("and the diameter travels beside it");
        assert!(
            radius > 0.0,
            "a printed tip has a positive radius: {contact}"
        );
        // UNITS. The registry speaks radius and the slicer UI speaks diameter;
        // a number carried between them without doubling lands 2x wrong, which
        // is precisely the class of silent movement this disclosure exists to
        // expose. Exact, not approximate: it is one multiplication by two.
        assert!(
            (diameter - radius * 2.0).abs() < f64::EPSILON * diameter.max(1.0),
            "tip_diameter_mm must be exactly twice tip_radius_mm: {contact}"
        );
        let law_radius_mm =
            plinth_support::perception::registered_tip_radius_mm_for_carried_area(carried)
                .expect("the registered area law must evaluate for a disclosed carried area");
        if let Some(grown_radius_mm) = grown_tip_radius_by_contact.get(&contact_id) {
            assert!(
                (radius - grown_radius_mm).abs() <= 1.0e-12 * grown_radius_mm.max(1.0),
                "two disclosures of the same grown tip must name the same radius: this row says \
                 {radius} mm, the grown-tip disclosure says {grown_radius_mm} mm; {contact}"
            );
            assert!(
                radius >= law_radius_mm - 1.0e-12 * law_radius_mm.max(1.0),
                "grow-to-attest only ever moves a tip in the conservative direction, so a grown \
                 tip is never SMALLER than the law asked for its carried area: {radius} mm \
                 against a law radius of {law_radius_mm} mm; {contact}"
            );
        } else {
            assert!(
                (radius - law_radius_mm).abs() <= 1.0e-12 * law_radius_mm.max(1.0),
                "a tip the run did not grow is sized by the area law from the area this row \
                 discloses: {carried} mm² asks for {law_radius_mm} mm RADIUS and the row says \
                 {radius} mm. The two numbers no longer belong to each other; {contact}"
            );
        }
        // Non-vacuity, MEASURED rather than assumed. Both clamps are inside the
        // law, so a row pinned at the registered minimum or maximum answers the
        // same radius however its area moves, and a check made only of such
        // rows would pass a tripled area in silence. This counts the rows where
        // the law's answer DOES move when the area does, which is the only
        // place the check has teeth. Measured 2026-08-21: 4 of the fixture's
        // 18 rows.
        let doubled =
            plinth_support::perception::registered_tip_radius_mm_for_carried_area(carried * 2.0)
                .expect("the registered area law must evaluate");
        if doubled > law_radius_mm {
            rows_a_carried_area_change_would_move += 1;
        }
    }
    assert!(
        rows_a_carried_area_change_would_move > 0,
        "every disclosed contact sits at a clamp, so the area law would answer the same radius \
         however the carried areas moved and the cross-check above proves nothing on this \
         fixture; it needs a fixture with at least one load-sized tip"
    );
    // The disclosure covers the plan, not a sample of it: one row per routed
    // contact, each named by its own id, so a reader diffing two runs sees an
    // added or removed contact rather than inferring one from a byte count.
    let mut ids: Vec<i64> = contact_rows
        .iter()
        .map(|contact| contact["contact_id"].as_i64().expect("a contact id"))
        .collect();
    let total = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(
        ids.len(),
        total,
        "a contact id names one contact, so a repeated id means two rows describe the same seat"
    );
    // **And the disclosed radii are the radii that reached the FILE.**
    //
    // `plinth_support`'s lowering builds each contact ball at
    // `tip_ball_factor` x the member's terminalward radius (`lower.rs`,
    // `lower_stub`), so every `SegmentType::TIP` record ChituBox holds is one
    // disclosed radius scaled by one registered factor — and the factor is READ
    // from the registry here, never written down. This is the leg that makes
    // the disclosure a statement about the artifact rather than about the
    // producer's own bookkeeping: a projection that read the member's ROOTWARD
    // radius, or scaled it, satisfies every check above and fails this one.
    //
    // It also states the relation a reader needs and would otherwise get wrong.
    // The disclosed radius is the MEMBER's tip radius; the record in the file
    // is 1.5x that, so diffing the two directly lands 1.5x off.
    let registered_tip_ball_factor = plinth_core::m0_constants()
        .expect("the M0 registry must construct")
        .scar_area_cap_policy()
        .tip_ball_factor()
        .diagnostic()
        .get();
    let mut tip_radii_on_disk_mm: Vec<f64> = chitubox_io::Project::open(&artifact)
        .expect("the artifact must reopen")
        .supports(0)
        .expect("the artifact's support section must parse")
        .iter()
        .filter(|record| record.segment_type == chitubox_io::SegmentType::TIP)
        .map(|record| f64::from(record.r_start))
        .collect();
    tip_radii_on_disk_mm.sort_by(f64::total_cmp);
    let mut disclosed_ball_radii_mm: Vec<f64> = contact_rows
        .iter()
        .map(|contact| {
            contact["tip_radius_mm"].as_f64().expect("a tip radius") * registered_tip_ball_factor
        })
        .collect();
    disclosed_ball_radii_mm.sort_by(f64::total_cmp);
    assert_eq!(
        tip_radii_on_disk_mm.len(),
        disclosed_ball_radii_mm.len(),
        "one routed contact, one contact ball on disk: the artifact holds {} TIP records and the \
         disclosure names {} contacts",
        tip_radii_on_disk_mm.len(),
        disclosed_ball_radii_mm.len()
    );
    for (on_disk_mm, disclosed_mm) in tip_radii_on_disk_mm.iter().zip(&disclosed_ball_radii_mm) {
        // The container stores binary32, so the comparison is at f32 resolution
        // and no finer. Nothing is being loosened: this is the width of the
        // number the file can hold.
        assert!(
            (on_disk_mm - disclosed_mm).abs() <= f64::from(f32::EPSILON) * disclosed_mm.max(1.0),
            "the tip radii on disk are the disclosed radii scaled by the registered ball factor \
             {registered_tip_ball_factor}; got {tip_radii_on_disk_mm:?} on disk against \
             {disclosed_ball_radii_mm:?} disclosed"
        );
    }
    // Task #49. This fixture's every island keeps a routed contact, so the
    // accepted contact set charges no face to nobody and the disclosure is
    // empty. Asserted rather than ignored: an empty list here is the claim
    // "every face this artifact covers has something sized to carry it", and
    // if that ever stops being true on the healthy fixture the run report has
    // to say so before anyone reads the byte count.
    assert!(
        ownerless_islands.is_empty(),
        "the healthy fixture must leave no island without an owning contact; got \
         {ownerless_islands:?}"
    );
    // PR-8, and the same argument one step over. The claim being asserted is
    // "every routed LOAD in this artifact is carried by a support that was
    // affirmatively proven attached". A refused contact is quarantined, nothing
    // is grown in its place, and the coverage measurement's 6 mm radius cannot
    // see a single missing claw tip — so if any load ever stops being carried
    // on the healthy fixture the report has to say so.
    //
    // **The assertion is on the LOAD, not on the length of the list, since the
    // scar bound landed (2026-08-20).** Craig's `TIP_RADIUS_MAX_GROWN_SCAR_MM`
    // of 0.400 mm RADIUS (0.800 mm DIAMETER) refuses one contact on this
    // fixture whose rescue would have needed a wider scar than any support he
    // has ever printed — and that contact carries **0.000 mm²**. Demanding an
    // empty list would demand the engine build an over-bound scar to hold up
    // nothing. Demanding zero abandoned AREA is the check that was always
    // meant, it still has every tooth, and it now says what it means.
    let plinth_pipeline::ContactAttestationDisclosure {
        abandoned_contacts,
        grown_contact_tips,
    } = contacts.as_ref();
    let abandoned_mm2 = abandoned_contacts
        .iter()
        .map(plinth_support::AbandonedContactDisclosure::abandoned_area_mm2)
        .sum::<f64>();
    assert_eq!(
        abandoned_mm2, 0.0,
        "the healthy fixture must abandon no routed load; got {abandoned_mm2} mm² \
         across {abandoned_contacts:?}"
    );
    // **What dropping `is_empty()` cost, bought back.** The load claim above is
    // total, but on its own it would let a regression add any number of
    // zero-load refusals without a single test going red. The COUNT is
    // therefore pinned too: the healthy fixture refuses exactly the contacts it
    // is measured to refuse (2026-08-20), and a new refusal — even one that
    // abandons nothing — has to be re-derived deliberately rather than arriving
    // unnoticed behind a zero sum.
    assert_eq!(
        abandoned_contacts.len(),
        HEALTHY_FIXTURE_ABANDONED_CONTACTS,
        "the healthy fixture's refusal count must not move without a re-derivation; got \
         {abandoned_contacts:?}"
    );
    // And every refusal that IS present must name the registered limit that
    // stopped it, in both units, so a reader can tell a policy refusal from a
    // seat that nothing could ever reach.
    for abandoned in abandoned_contacts {
        // The discriminator that keeps the named limit honest. Only a
        // `not_attached` refusal ever enters the growth ladder; a buried seat
        // is a mesh fault no tip radius can move, and it is refused before any
        // rung exists. Both lines quote the same registered ceiling, so without
        // this flag an operator could read Craig's field maximum as the stopper
        // of a rescue that was never attempted.
        assert_eq!(
            abandoned.growth_ladder_entered(),
            abandoned.refusal() == "not_attached",
            "only a not_attached refusal may be recorded as having walked the growth ladder; \
             got {abandoned:?}"
        );
        assert!(
            abandoned.growth_ladder_entered() || abandoned.growth_rungs().is_empty(),
            "a refusal that never entered the ladder cannot have walked a rung; got \
             {abandoned:?}"
        );
        assert!(
            matches!(
                abandoned.growth_ceiling_constant(),
                "TIP_RADIUS_MAX_GROWN_SCAR_MM" | "TIP_RADIUS_MAX_LEGACY_MM"
            ),
            "an abandoned contact must name the registered maximum that stopped \
             its rescue; got {abandoned:?}"
        );
        assert!(
            abandoned
                .growth_rungs()
                .iter()
                .all(|rung| rung.radius_mm() <= abandoned.growth_ceiling_radius_mm()),
            "no rung may propose a tip above the ceiling it was walked to; got \
             {abandoned:?}"
        );
    }
    // Growth is allowed to fire here and is asserted rather than assumed: every
    // grown tip must be at least the radius the load law asked for, so growth
    // can only ever have moved a tip in the conservative direction.
    for grown in grown_contact_tips {
        assert!(
            grown.emitted_tip_radius_mm() >= grown.law_tip_radius_mm(),
            "grow-to-attest must never emit a tip SMALLER than the load law asked \
             for; got {grown:?}"
        );
        assert!(
            (grown.emitted_tip_diameter_mm() - grown.emitted_tip_radius_mm() * 2.0).abs() < 1e-12,
            "the reported diameter must be exactly twice the reported radius; got {grown:?}"
        );
        // The SMALLEST attesting radius on the record is the one built. The
        // ascending ladder alone would emit the FIRST attesting rung, which the
        // descending refinement scan then improves on, so this is what tells a
        // scanned answer from an unscanned one on real geometry.
        let smallest_attesting_mm = grown
            .growth_rungs()
            .iter()
            .filter(|rung| rung.attested())
            .map(plinth_support::TipGrowthRungDisclosure::radius_mm)
            .fold(f64::INFINITY, f64::min);
        assert!(
            (smallest_attesting_mm - grown.emitted_tip_radius_mm()).abs() < 1e-12,
            "the tip built must be the smallest radius the record shows \
             attesting; got {grown:?}"
        );
        // And the operator can FIND it: a scar the run chose to enlarge is only
        // actionable if the report says where it is.
        assert!(
            grown.seat_mm().iter().all(|value| value.is_finite()),
            "a grown tip must disclose a real seat, or the enlarged scar cannot \
             be located on the model; got {grown:?}"
        );
    }
    // **What is in the file, not merely that there is one.** Two of the four
    // geometries are substitutions the chain fell back to, and a report that
    // said only "emitted" could not tell them from the intended shell.
    assert_eq!(
        geometry,
        &SupportedGeometry::HollowStageDecidedSolid,
        "the emitted geometry changed; the artifact is not what it was"
    );
    assert_eq!(
        hollow_disposition,
        &plinth_hollow::HollowDisposition::Solid {
            reason: plinth_hollow::SolidDecisionReason::BelowThreshold
        }
    );
    // The repair reaches the report. This model decided solid before a cavity
    // was ever selected, so there is nothing to record -- and `None` is the
    // claim "no repair result exists here", which is what distinguishes it from
    // a recorded repair of size zero.
    assert_eq!(cavity_repair, &None);
    // **FU-37 reaches the run's own output.** The requested wall is on the
    // artifact record, so this run is distinguishable from the same run at a
    // different `--wall-mm`. It is the request's wall, not a constant: the
    // fixture asks for the registered default.
    assert_eq!(
        wall_mm.map(plinth_core::Finite::get),
        Some(2.5),
        "the requested wall must reach the artifact record"
    );
    // FU-55: this model decided solid before a shell was ever built, so there
    // is no realized wall to state -- `None` is the claim "nothing was
    // measured", not "the measurement was dropped".
    assert_eq!(realized_wall, &None);
    // 2026-08-13 owner ruling: `savings_shortfall` is populated ONLY when the
    // refusal that produced this `BelowThreshold` disposition actually came
    // from the savings gate (`HollowError::IneligibleSavings` /
    // `IneligibleSavingsAfterCavityRepair`). `BelowThreshold` is itself a
    // catch-all shared with the untouched bulk/feature-size gate (FU-79,
    // pre-existing and out of scope): `HollowError::IneligibleBelowBulk` maps
    // onto the same `SolidDecisionReason::BelowThreshold` with `None`, because
    // there is no shortfall to record for a bulk refusal. **This fixture is
    // the bulk case, not the savings case** -- measured directly:
    // `make_toed_slab` (a 12x12x1.5 mm slab) refuses as `IneligibleBelowBulk`
    // before a cavity is ever computed, the same way the owner's own
    // `Myconid Adult` and `Vegepygmy` acceptance-suite rejects do (see
    // [the savings-gate section](#savings-gate-absolute-volume-2026-08-13)).
    // So the correct assertion here is `None`, not a populated shortfall --
    // asserting the field's *absence* is exactly as load-bearing as asserting
    // its presence elsewhere, because a `Some` here would mean the mapping
    // conflated the two gates. The positive case (a real `IneligibleSavings`
    // refusal's shortfall reaching the report) is proved end to end at the
    // `create_shell` level in `plinth-hollow`'s own
    // `the_default_wall_clears_savings_and_the_heavier_wall_now_refuses_on_the_retired_percentage_gate`
    // and via the committed `corpus/models/make_box.stl` fixture (`cargo run
    // -p plinth-hollow --example wall_distribution`), and at the mapping
    // level in this crate's own `prepare::tests`.
    assert_eq!(
        savings_shortfall, &None,
        "make_toed_slab refuses on the bulk gate (IneligibleBelowBulk), not the savings gate -- \
         a populated shortfall here would mean the two refusal causes got conflated under the \
         shared BelowThreshold disposition"
    );
    // Nothing was hollowed, so no chamber was given up. An empty list is the
    // claim "the engine gave up on nothing", which is what the run report
    // needs in order to stay silent instead of printing a reassuring zero.
    assert!(
        filled_back_chambers_ml.is_empty(),
        "a model that never hollowed cannot have filled a chamber back: \
         {filled_back_chambers_ml:?}"
    );
    assert!(
        emit.support_record_count > 0,
        "an artifact with no supports"
    );
    assert!(artifact.is_file(), "{} was not written", artifact.display());
    assert_eq!(
        report.artifacts().collect::<Vec<_>>(),
        vec![artifact.as_path()]
    );

    // And it is a real `.chitubox`, judged by re-reading the bytes rather than
    // by trusting the producer: same independent audit `first_artifact.rs` uses.
    let audit = chain::audit_disk_artifact(&artifact, "mini").expect("reopen and audit");
    assert_eq!(audit.model_name, chain::EMITTED_MODEL_NAME);
    assert_eq!(audit.support_record_count, emit.support_record_count);
    assert_eq!(audit.file_bytes, emit.artifact_bytes);
    assert_eq!(
        audit.outward_wound_records,
        audit.support_record_count,
        "{} support meshes on disk are inward-wound",
        audit.support_record_count - audit.outward_wound_records
    );
    assert!(audit.model_signed_volume_mm3 > 0.0);
    assert!(
        audit.preview_is_native_fallback,
        "ChituBox refuses a from-scratch project without the native preview"
    );
    assert!(
        audit.format_gate_passed,
        "the plinth-verify format gate refused the emitted artifact"
    );

    // Both bad models failed individually, named, with no artifact and no
    // assumed size, because neither was ever measured.
    for bad in ["aa-not-a-model", "zz-not-a-model"] {
        let ModelPreparationOutcome::Failed {
            exit_code, detail, ..
        } = outcome(&report, bad)
        else {
            panic!("{bad} must fail, not emit");
        };
        assert_eq!(*exit_code, PipelineExitCode::Input);
        assert!(
            detail.contains("binary STL"),
            "{bad}'s failure must name what was wrong: {detail}"
        );
        assert!(
            !scratch.output().join(format!("{bad}.chitubox")).exists(),
            "{bad} wrote an artifact"
        );
    }
    assert!(
        report
            .models
            .iter()
            .find(|entry| entry.model.as_str() == "aa-not-a-model")
            .expect("reported")
            .assumed_print_size
            .is_none(),
        "a model that never loaded cannot have an assumed print size"
    );

    // The assumed-print-size line is present on the success path too, which is
    // the whole point of it: a mini processed at a tenth of its intended size
    // produces a plausible artifact and an unusable print.
    let size = report
        .models
        .iter()
        .find(|entry| entry.model.as_str() == "mini")
        .expect("reported")
        .assumed_print_size
        .expect("a model that decimated was measured");
    assert!(
        size.longest_axis_mm > 0.0 && size.longest_axis_mm.is_finite(),
        "assumed print size must be a real measurement: {size:?}"
    );
    assert_eq!(
        size.longest_axis_mm,
        size.dimensions_mm
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max),
        "the longest axis must be the longest bounding-box dimension"
    );

    // The run's exit code is the first non-Ok disposition in report order, and
    // `aa-not-a-model` sorts first.
    assert_eq!(report.exit_code(), PipelineExitCode::Input);
}

/// **A model that cannot close its coverage now EMITS, and says exactly what
/// it left bare — owner ruling, 2026-09-01.**
///
/// This test used to be `a_sealed_run_refused_model_writes_no_artifact_and_exits_five`
/// and it asserted the opposite disposition: `make_raking_blade` could not
/// close island 5, so `run_support` returned `SupportRunError::CoverageClosure`,
/// the pipeline reported `Failed` with exit code 5, and **no file existed**.
///
/// The owner overruled that, in these words: "if a region can't be supported,
/// then EMIT THE MODEL ANYWAYS with the supports that *do* work in place, and a
/// loud warning about the fact some area(s) are unsupported"; and "Partial
/// success will let me then look at the model and fix it manually to print, or
/// have a chance at refining the algorithm understanding where it failed. With
/// *nothing* emitted, I can't see what's working and what isn't working."
///
/// **Nothing about the MEASUREMENT was weakened, and that is what this test now
/// guards.** The blade's island is still measured, still genuinely uncovered
/// (5.135 mm² against a 0.16 mm² censor), still named, still rendered, and the
/// repair controller still runs its pool all the way out — 45 trials, 0 commits
/// — and still says so. The single thing that changed is the disposition: the
/// artifact is written and the evidence sits beside it instead of instead of
/// it. A regression that quietly stops MEASURING the gap fails here exactly as
/// a regression that stopped refusing used to.
///
/// **Re-pinned onto `make_raking_blade` (2026-08-12).** This test used to run
/// `make_overhang_table`, which was the coverage-refusing fixture; with
/// nomination scored against the measurement's uncovered samples that model
/// now CLOSES and emits. So the overhang table can no longer stand for an
/// unclosable island. `make_raking_blade` can — and as of 2026-08-14 it is the
/// PROCEDURAL blade, not the `.prepped.stl` (see [`refusal_fixture`] for the
/// measured reason). Its island 5 has `NoRoutedContact` on every attempt,
/// 5.135 mm² uncovered on one patch: the CCO transcript admits none of the
/// repair rows nominated from its 14 eligible ones, so the pool drains. That is
/// measured, not forced: nothing here touches the radius, the censor or any
/// tolerance.
#[test]
fn a_model_that_cannot_close_its_coverage_emits_and_names_the_gap() {
    let scratch = Scratch::new("refusal");
    refusal_fixture(&scratch, "blade");

    let report = prepare(pinned(&scratch, "blade", "make_raking_blade.pipeline.json"))
        .expect("a coverage gap is a disposition, not a run failure");

    let ModelPreparationOutcome::Artifact {
        emit,
        coverage_warning,
        repair_report,
        contacts,
        ..
    } = outcome(&report, "blade")
    else {
        panic!(
            "the raking blade must EMIT with a named gap, not be refused; it reported {:?}",
            outcome(&report, "blade")
        );
    };

    // The file is real and it is at the target path.
    let artifact = scratch.output().join("blade.chitubox");
    assert_eq!(emit.artifact_path, artifact);
    assert!(
        artifact.exists(),
        "the ruling is that the file gets written; nothing is at {}",
        artifact.display()
    );
    assert_eq!(report.artifacts().count(), 1);
    assert_eq!(report.exit_code(), PipelineExitCode::Ok);
    assert!(!report.has_gate_refusal());

    // **The loud half.** An emitted file with an unsupported island that does
    // not SAY so is the silent failure this ruling must not become.
    let warning = coverage_warning
        .as_ref()
        .expect("an emitted model with an unclosable island must carry the coverage warning");
    assert!(
        !warning.islands.is_empty(),
        "the unsupported island must be named: {warning:?}"
    );
    // **The gap is a real hole, not censored dust.** This is the other half of
    // the 2026-08-14 censor change (docs/specs/coverage-repair.md §4): 5.135 mm²
    // is 32x the registered 0.16 mm² censor, so this assert is what goes red if
    // the censor is ever widened to swallow a real island.
    let censor = plinth_support::registered_coverage_censor_area_mm2()
        .expect("registered A3 coverage censor");
    let worst = warning
        .islands
        .iter()
        .map(|island| island.uncovered_area_mm2)
        .fold(0.0_f64, f64::max);
    assert!(
        worst > censor,
        "the reported island must be genuinely uncovered, not censored dust: {worst} mm² against \
         a {censor} mm² censor"
    );
    assert!(
        worst > 5.0,
        "the procedural blade's island 5 measures 5.135 mm²; if it moved this far, the fixture \
         moved: {worst} mm²"
    );
    assert!(
        warning.total_uncovered_mm2 >= worst,
        "the total must not understate its own worst island: {warning:?}"
    );
    assert!(
        warning
            .islands
            .iter()
            .any(|island| island.face_count() > 0),
        "the reported islands must name their faces"
    );
    // The WHY, which is the half that points at code to fix rather than at
    // geometry to patch by hand.
    assert_eq!(
        warning.closure_attempts, 0,
        "the blade commits nothing, so a field named for the committed bound reads zero: \
         {warning:?}"
    );
    assert!(
        warning
            .closure_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("NoImprovingRepair")),
        "the warning must name the stage's own stop reason: {warning:?}"
    );

    // **The mechanism that moved the numbers below, pinned so it cannot move
    // back silently.** This fixture is 24 faces, and six of its placements are
    // rays that run knife-edge parallel to a facet. Until 2026-09-01 a single
    // one of those refused the whole model at PR-8 — measured at `d563574`,
    // this very test failed with
    // `Initial(Attestation(NearParallelWitness { near_parallel: 6 }))` and never
    // reached the repair loop at all. They are quarantined per placement now, so
    // the blade routes on the contacts that DID attest.
    assert!(
        contacts
            .abandoned_contacts
            .iter()
            .any(|contact| contact.refusal() == "undecidable_geometry"),
        "this fixture's search is what it is BECAUSE undecidable placements are quarantined \
         rather than fatal; if none is quarantined the numbers below describe a different run: \
         {:?}",
        contacts.abandoned_contacts
    );

    // **SPEC-A4 §A.8's third destination, on the path that needs it most.**
    // Ruling R10 deleted the failed-tries cap and put loud reporting in its
    // place. This model spends 161 one-row transactions and commits none of
    // them, and it has to say so on THIS path now, because this path is the one
    // that produces a file.
    //
    // **Re-measured 2026-09-01, from `(45, 0)` and `remaining = 0`.** The old
    // numbers were not wrong when they were taken and they are not a
    // disposition change either: they were taken before `b0157ec`, and from
    // `b0157ec` until 2026-09-01 this fixture died at PR-8 before the repair
    // controller ever ran, so the pin sat unexercised while the geometry under
    // it moved. It is re-derived here against a run that reaches the loop again.
    //
    // **`remaining` moved from 0 to 41, and that is a change of STOP REASON,
    // not just of magnitude.** Zero used to mean the pool drained — every row
    // ledgered, nothing left to try. Forty-one rows are now still un-ledgered
    // when the loop stops, because it stops on §B.3's zero-score guard instead:
    // the best live candidate covers 0.00 mm², so no remaining row can help.
    // The old comment's claim — "the loop stopped because nothing could help,
    // not because a counter said stop" — is therefore still exactly true, and
    // it is now established by the measured score rather than by exhaustion.
    // The assertion below says so directly, against the score, so a future
    // regression that stops on a counter cannot hide behind a nonzero
    // `remaining`.
    //
    // **Re-measured 2026-09-02, from `(161, 0)` to `(54, 0)`.** The repair pool
    // is now screened by the batched seat attestation before it is ranked, and
    // on this fixture the screen refuses **107 of 202** candidate seats as
    // `undecidable_geometry` — the very cause the quarantined placements
    // asserted above are quarantined for, found at the seat instead of after a
    // full prepare -> route -> rewire -> brace -> structure -> lower -> measure
    // pass per seat. Those 107 are trials that used to run and always roll
    // back; the model, its commits (0), its stop reason (§B.3's zero-score
    // guard) and its emitted file are unchanged, and the loop's wall time fell
    // from ~35 s to ~12 s. `RepairRunReport::prescreen` is where that is stated
    // on the run's own record.
    // **Re-measured 2026-09-08, from `(54, 0)` to `(55, 0)`, for the
    // TIP-SEATING CONTACT LAW** (`26eaab7`, `dc3dae7`, `f8656cf`: legacy seat
    // depth minimum 0.3625 mm -> a derived layer+pixel allowance, and
    // `seat_depth_of`'s first term). The batched seat attestation that runs
    // the prescreen judges a seat's geometry using the SAME seating law the
    // repair trial itself would use, so one seat that used to attest
    // `undecidable_geometry` (needing more penetration than the old flat
    // minimum could resolve unambiguously) now resolves under the shallower
    // derived allowance and is no longer dropped at the screen. `offered`
    // (202, the candidate set itself) is unaffected — nothing about which
    // seats exist moved, only which of them the screen can decide. So `dropped`
    // falls by exactly one (107 -> 106, all of it out of
    // `undecidable_geometry`), the one seat the screen now passes through
    // enters the trial loop, and `trials` rises by exactly one to match
    // (54 -> 55). `commits` stays at 0: MEASURED, not assumed — the new trial
    // is still vetoed (it joins the same zero-improvement/regression pool the
    // other 54 already sat in), so this fixture still commits nothing and its
    // stop reason (§B.3's zero-score guard, asserted below) is unchanged.
    let repair = repair_report
        .as_ref()
        .expect("a model that spent the repair controller states what it spent");
    assert_eq!(
        (repair.trials, repair.commits),
        (55, 0),
        "the raking blade tries every candidate that could help and commits none: {repair:?}"
    );
    assert_eq!(
        (
            repair.prescreen.offered,
            repair.prescreen.dropped,
            repair.prescreen.dropped_by_cause.get("undecidable_geometry"),
        ),
        (202, 106, Some(&106)),
        "the trials this fixture no longer spends are accounted for by name on the report, not \
         merely absent from the count: {repair:?}"
    );
    assert!(
        warning
            .closure_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("best_remaining_score_mm2: 0.0")),
        "the loop must stop because nothing left can help, not because a counter said stop; \
         {} rows were still un-ledgered when it did: {warning:?}",
        repair.candidates_remaining
    );

    // **FU-57 survives the ruling.** The sidecar and the per-island renders
    // were written on the REFUSAL path, and that path no longer exists for this
    // model — so if they were not moved onto the emitted path, the ruling would
    // have destroyed the one piece of evidence that says WHERE to look. They
    // are written beside the artifact, never at it.
    let sidecar_path = scratch.output().join("blade.coverage.json");
    assert!(
        sidecar_path.exists(),
        "the coverage sidecar must be on disk beside the emitted artifact"
    );
    let sidecar_bytes = std::fs::read(&sidecar_path).expect("read sidecar");
    let sidecar: serde_json::Value = serde_json::from_slice(&sidecar_bytes).expect("valid json");
    assert_eq!(sidecar["schema"], "plinth.coverage-refusal.v2");
    assert_eq!(sidecar["source"], "pre_write_gate");
    assert!(
        sidecar["report"].is_object(),
        "the gate's own coverage report must be embedded verbatim"
    );
    let renders = std::fs::read_dir(scratch.output())
        .expect("read output dir")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "svg")
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("blade"))
        })
        .collect::<Vec<_>>();
    assert!(
        !renders.is_empty(),
        "the worst island(s) must still be rendered beside the emitted artifact"
    );
    for render_path in &renders {
        let bytes = std::fs::read(render_path).expect("read render");
        let svg = String::from_utf8(bytes).expect("utf8 svg");
        assert!(svg.starts_with("<svg"), "must be a well-formed svg: {svg}");
    }
}

/// FU-57: a model that starts passing does not leave yesterday's
/// coverage-refusal evidence sitting beside today's fresh artifact.
///
/// A `.coverage.json` (or a stray render) from an earlier refused run, left
/// beside a model that now emits cleanly, is read by the next thing down the
/// line as *this* run's answer -- the exact misleading hand-off
/// `clear_stale_artifact` already guards the `.chitubox` target against. The
/// extension in `prepare_one` owes the same guarantee to the evidence class.
#[test]
fn a_model_that_starts_passing_clears_its_own_stale_coverage_evidence() {
    let scratch = Scratch::new("stale-coverage-evidence");
    scratch.place("make_toed_slab.prepped.stl", "mini");

    std::fs::create_dir_all(scratch.output()).expect("plant an output folder");
    let stale_sidecar = scratch.output().join("mini.coverage.json");
    std::fs::write(&stale_sidecar, b"{\"schema\":\"stale\"}").expect("plant a stale sidecar");
    let stale_render = scratch.output().join("mini.coverage.island-3.svg");
    std::fs::write(&stale_render, b"<svg></svg>").expect("plant a stale render");
    // A render for a DIFFERENT model must never be touched by mini's own
    // clearing pass -- proves the match is by this model's own prefix, not by
    // extension alone.
    let unrelated_render = scratch.output().join("other.coverage.island-1.svg");
    std::fs::write(&unrelated_render, b"<svg></svg>").expect("plant an unrelated render");

    let report = prepare(pinned(&scratch, "mini", "make_toed_slab.pipeline.json"))
        .expect("a model that now passes is a disposition, not a run failure");

    assert!(
        matches!(
            outcome(&report, "mini"),
            ModelPreparationOutcome::Artifact { .. }
        ),
        "mini must still emit; it reported {:?}",
        outcome(&report, "mini")
    );
    assert!(
        !stale_sidecar.exists(),
        "a stale coverage sidecar survived a model that now emits cleanly"
    );
    assert!(
        !stale_render.exists(),
        "a stale coverage render survived a model that now emits cleanly"
    );
    assert!(
        unrelated_render.exists(),
        "clearing mini's stale evidence must not touch another model's render"
    );
}

/// A refusal removes a *stale* artifact at the target too.
///
/// "A refused model leaves no path on disk" is a claim about the path, not
/// about one call. A model that emitted on an earlier run and is refused on
/// this one would otherwise leave the earlier run's file sitting there, where
/// the next thing to read it would take it for this run's answer.
#[test]
fn no_disposition_leaves_an_earlier_runs_artifact_at_its_target() {
    const STALE: &[u8] = b"an artifact from an earlier run";

    let scratch = Scratch::new("stale");
    refusal_fixture(&scratch, "blade");
    // A model that CANNOT reach a file, so the refusal half of the claim keeps
    // a fixture of its own. The blade used to be that fixture and no longer is.
    scratch.write("broken.stl", b"not an STL at all");
    std::fs::create_dir_all(scratch.output()).expect("plant an output folder");
    let emitting = scratch.output().join("blade.chitubox");
    let refusing = scratch.output().join("broken.chitubox");
    for target in [&emitting, &refusing] {
        std::fs::write(target, STALE).expect("plant a stale artifact");
    }

    let report =
        prepare(pinned(&scratch, "blade", "make_raking_blade.pipeline.json")).expect("run");

    // **Half one — a refusal still leaves nothing.** Unchanged claim, unchanged
    // assertion, moved onto a model that still refuses.
    assert!(matches!(
        outcome(&report, "broken"),
        ModelPreparationOutcome::Failed {
            exit_code: PipelineExitCode::Input,
            ..
        }
    ));
    assert!(
        !refusing.exists(),
        "a refusal left the previous run's artifact at {}",
        refusing.display()
    );

    // **Half two — and neither does an emission with a coverage gap.** This is
    // the case the ruling created: the blade now reaches a file, so "the target
    // is empty" stops being available as the proof and the positive form has to
    // be asserted instead. `prepare_one` unlinks the target on the way IN,
    // before any stage can fail, and `plinth_emit::write` renames its staging
    // sibling over whatever is there — so the stale bytes cannot survive either
    // way, and this pins that they do not.
    let ModelPreparationOutcome::Artifact { emit, .. } = outcome(&report, "blade") else {
        panic!(
            "the blade must EMIT with its gap named; it reported {:?}",
            outcome(&report, "blade")
        );
    };
    let on_disk = std::fs::read(&emitting).expect("this run's artifact must be at the target");
    assert_ne!(
        on_disk.as_slice(),
        STALE,
        "the earlier run's file survived under this run's artifact at {}",
        emitting.display()
    );
    assert_eq!(
        on_disk.len() as u64,
        emit.artifact_bytes,
        "the bytes at the target must be the ones this run reports writing"
    );
}

/// **The dense sweep orients a corpus model end to end, deterministically.**
///
/// This is the test the previous one could not be. Before `--orient` and the
/// `DenseSweep` default landed, the only policy that could orient anything was
/// a pin, so `plinth prepare` on arbitrary input reached a per-model refusal
/// and wrote nothing at all. Here a committed corpus model goes in with **no
/// baseline named anywhere**, the sweep chooses its own orientation, and a real
/// `.chitubox` comes out of a temp directory.
///
/// Three things are asserted, and the third is the one that matters:
///
/// 1. the run emits, under the default policy, with no pin in sight;
/// 2. the orientation is a *searched* one — the report carries the sweep's own
///    provenance, not a replayed rotation;
/// 3. two runs agree **byte for byte**, artifact and report both. The sweep
///    reads no clock and seeds nothing, its folds are serial in index order and
///    its rayon use is index-parallel map-collect, so this is a determinism
///    claim rather than a coincidence of one process.
#[test]
fn the_dense_sweep_orients_a_corpus_model_end_to_end_and_is_bit_stable() {
    let scratch = Scratch::new("dense-sweep");
    scratch.place("make_toed_slab.prepped.stl", "mini");
    let target = scratch.output().join("mini.chitubox");

    // `request(&scratch, &[])` is the DEFAULT policy: no baseline is named for
    // this model or any other.
    let first = prepare(request(&scratch, &[])).expect("the dense sweep must orient this model");
    let ModelPreparationOutcome::Artifact { emit, .. } = outcome(&first, "mini") else {
        panic!(
            "the searched model must reach an artifact; it reported {:?}",
            outcome(&first, "mini")
        );
    };
    assert_eq!(emit.artifact_path, target);
    assert!(target.is_file(), "{} was not written", target.display());
    let artifact = std::fs::read(&target).expect("read the emitted artifact");
    assert!(!artifact.is_empty());

    // Nothing was pinned, and nothing borrowed a rotation: the request this
    // test built names no baseline for any stem.
    assert_eq!(
        request(&scratch, &[]).orientation,
        OrientationPolicy::DenseSweep
    );

    let second = prepare(request(&scratch, &[])).expect("the second run must also orient");
    assert_eq!(
        first, second,
        "the run report is not bit-stable under search"
    );
    assert_eq!(
        serde_json::to_string(&first).expect("report serializes"),
        serde_json::to_string(&second).expect("report serializes"),
        "the serialized report is not byte-identical across searched runs"
    );
    assert_eq!(
        std::fs::read(&target).expect("read the re-emitted artifact"),
        artifact,
        "two dense-sweep runs produced different bytes for the same model"
    );
}

/// Two runs over the same folder produce the same report, byte for byte.
///
/// The executor reads no clock and seeds nothing from the environment, so this
/// is a real determinism claim and not a coincidence of one process.
#[test]
fn two_runs_over_one_folder_produce_byte_identical_reports() {
    let scratch = Scratch::new("determinism");
    scratch.place("make_toed_slab.prepped.stl", "mini");
    scratch.write("broken.stl", b"not an STL");

    let first =
        prepare(pinned(&scratch, "mini", "make_toed_slab.pipeline.json")).expect("first run");
    let second =
        prepare(pinned(&scratch, "mini", "make_toed_slab.pipeline.json")).expect("second run");
    assert_eq!(first, second, "the report is not run-to-run stable");
    assert_eq!(
        serde_json::to_string(&first).expect("report serializes"),
        serde_json::to_string(&second).expect("report serializes"),
        "the serialized report is not byte-identical across runs"
    );
    assert_eq!(first.artifacts().count(), 1);
}

/// An empty folder is a complete, successful run of zero models — not an error
/// and not a crash.
#[test]
fn an_empty_folder_is_a_successful_run_of_no_models() {
    let scratch = Scratch::new("empty");
    // A non-`.stl` file must not be picked up, and neither must a directory
    // that happens to end in `.stl`.
    scratch.write("notes.txt", b"not a model");
    std::fs::create_dir_all(scratch.input().join("a-directory.stl")).expect("decoy directory");

    let report = prepare(pinned(&scratch, "mini", "make_toed_slab.pipeline.json")).expect("run");
    assert!(report.models.is_empty());
    assert_eq!(report.exit_code(), PipelineExitCode::Ok);
    assert_eq!(report.artifacts().count(), 0);
}

/// A missing input folder is a failure of the **run**, not of a model, and
/// nothing is written.
#[test]
fn a_missing_input_folder_fails_the_run_before_anything_is_created() {
    let scratch = Scratch::new("missing-input");
    let mut request = pinned(&scratch, "mini", "make_toed_slab.pipeline.json");
    request.input_models_dir = scratch.input().join("does-not-exist");

    let error = prepare(request).expect_err("a missing input folder must fail the run");
    assert_eq!(error.exit_code(), PipelineExitCode::Input);
    assert!(
        !scratch.output().exists(),
        "the output folder was created despite the run failing"
    );
}

/// **A failure *before* the gates clears the stale artifact too.**
///
/// `judge_then_emit` owns the no-file guarantee only for models that reach it.
/// Every earlier stop — an unreadable file, a scale refusal, an unorientable
/// model, a refused sealed run — used to leave the previous run's artifact
/// standing beside an ERROR line, where the next reader takes it for this run's
/// answer. Three dispositions are checked here, each with a planted artifact:
/// a load failure, an orientation refusal, and the gate refusal that was
/// already covered, so the guarantee is about the *path* on every arm.
#[test]
fn every_failure_disposition_clears_an_earlier_runs_artifact_at_its_target() {
    let scratch = Scratch::new("stale-pre-gate");
    scratch.place("make_toed_slab.prepped.stl", "unpinned");
    scratch.write("broken.stl", b"not an STL at all");
    refusal_fixture(&scratch, "blade");

    std::fs::create_dir_all(scratch.output()).expect("plant an output folder");
    // **`blade` left this list 2026-09-01 (owner ruling).** It used to be the
    // third failing stem — the sealed run refused its coverage — and it now
    // EMITS with that coverage named instead. Its target is still planted and
    // still checked, one assertion down, because "the stale file does not
    // survive" has to hold on the emit path too; what it can no longer do is
    // stand for a FAILURE disposition, which is what this list is.
    let planted = ["broken", "unpinned"].map(|stem| {
        let target = scratch.output().join(format!("{stem}.chitubox"));
        std::fs::write(&target, b"an artifact from an earlier, luckier run")
            .expect("plant a stale artifact");
        (stem, target)
    });
    let blade_target = scratch.output().join("blade.chitubox");
    std::fs::write(&blade_target, b"an artifact from an earlier, luckier run")
        .expect("plant a stale artifact");

    // Only `blade` is pinned, so `unpinned` reaches the missing-baseline
    // refusal and `broken` never loads: two different pre-gate stops.
    let report = prepare(pinned(&scratch, "blade", "make_raking_blade.pipeline.json"))
        .expect("a folder of failures is still a successful run");

    for (stem, target) in &planted {
        assert!(
            !matches!(
                outcome(&report, stem),
                ModelPreparationOutcome::Artifact { .. }
            ),
            "{stem} was supposed to fail; this test proves nothing if it emitted"
        );
        assert!(
            !target.exists(),
            "{stem} failed and left the earlier run's artifact at {}",
            target.display()
        );
    }
    // The emitting model's own planted file is gone too — replaced by this
    // run's artifact, not left underneath it.
    let ModelPreparationOutcome::Artifact { emit, .. } = outcome(&report, "blade") else {
        panic!(
            "the blade must EMIT with its gap named; it reported {:?}",
            outcome(&report, "blade")
        );
    };
    assert_eq!(
        std::fs::read(&blade_target)
            .expect("this run's artifact must be at the target")
            .len() as u64,
        emit.artifact_bytes,
        "the emitting model left the earlier run's bytes at {}",
        blade_target.display()
    );
    assert_eq!(
        report.artifacts().count(),
        1,
        "exactly the one model that can reach a file contributed one"
    );
}

/// A pinned baseline is **one model's** rotation and is never lent to another.
///
/// `plinth_orient::run` applies whatever rotation the report it is handed
/// states, to whatever mesh it is handed, and cannot tell that the two describe
/// different objects — a misoriented model seals, passes the gates and emits
/// exactly like a correct one. So a heterogeneous folder gets per-model
/// baselines or per-model refusals, and never a neighbour's answer.
#[test]
fn a_model_with_no_pinned_baseline_is_refused_rather_than_given_a_neighbours() {
    let scratch = Scratch::new("per-model-baseline");
    scratch.place("make_toed_slab.prepped.stl", "mini");
    scratch.place("Sphere.prepped.stl", "sphere");

    let report = prepare(pinned(&scratch, "mini", "make_toed_slab.pipeline.json"))
        .expect("an unpinned model is a disposition, not a run failure");

    // The named model oriented and emitted.
    assert!(matches!(
        outcome(&report, "mini"),
        ModelPreparationOutcome::Artifact { .. }
    ));
    // The unnamed one did not, and said why.
    let ModelPreparationOutcome::Failed {
        exit_code, detail, ..
    } = outcome(&report, "sphere")
    else {
        panic!("an unpinned model must be refused, not silently given mini's rotation");
    };
    assert_eq!(*exit_code, PipelineExitCode::Input);
    assert!(
        detail.contains("no pinned orientation baseline"),
        "the refusal must name what is missing: {detail}"
    );
    assert!(!scratch.output().join("sphere.chitubox").exists());

    // Named per model, both models orient and both reach artifacts — the map
    // is a real per-model resolution and not a one-entry special case.
    let scratch = Scratch::new("per-model-baseline-both");
    scratch.place("make_toed_slab.prepped.stl", "mini");
    scratch.place("Sphere.prepped.stl", "sphere");
    let report = prepare(request(
        &scratch,
        &[
            ("mini", "make_toed_slab.pipeline.json"),
            ("sphere", "Sphere.pipeline.json"),
        ],
    ))
    .expect("run");
    // **The two fixtures take different routes, and the sphere's route has
    // moved twice.** `mini` is decided solid by the hollow stage on its own
    // terms and emits, throughout every revision below.
    //
    // Until the 2026-08-10 cavity-field exterior fix, `sphere`'s cavity
    // reached to within 0.5 mm of the outer surface at the six points where
    // it is tangent to its own bounding box; the sealed support run refused
    // *that* shell, and the chain substituted the oriented solid, so this
    // block used to assert two artifacts and a `DrilledShellRefusedBySupport`
    // substitution.
    //
    // From 2026-08-10 to 2026-08-12 the wall was no longer breached, the
    // drilled shell **sealed**, reached the pre-write gate on its own merits,
    // and `UncoveredIslands` refused it — terminally, because a gate refusal
    // is not answered by substituting different geometry. In that window this
    // block asserted one artifact, exit 3, and the same verdict
    // `first_artifact.rs`'s `corpus_sphere_drilled_plan_is_refused_before_any_file_exists`
    // pinned running the sphere alone. (That test was DELETED on 2026-08-26:
    // at `3a6526f` the sphere's islands became coverable, it seals at attempt
    // 0 and emits, so the refusal it pinned had stopped existing.)
    //
    // **2026-08-12, FU-53 landed: the sphere's route moved a second time, and
    // it now diverges from `first_artifact.rs`'s single-model run.** FU-53
    // corrected cavity selection to the exact nearest-surface distance plus
    // `HOLLOW_CAVITY_MARCH_MARGIN_MM`, which is honestly smaller. Support
    // demand on the resulting thinner-walled shell now exceeds the sealed
    // run's registered routing capacity before the gate is ever reached, so
    // the sphere takes `DrilledShellRefusedBySupport` again, the same named
    // outcome the pre-2026-08-10 revision above took, converted by the
    // existing OQ-1/FU-34 reserve substitution into an emitted solid artifact
    // rather than a terminal gate refusal. Both models now reach an artifact.
    //
    // **The same day, `feature/coverage-closure` (FU-56/FU-57) landed on
    // top and, on THIS fixture, changes nothing measured here.** `run_support`
    // is now the coverage-closure controller, but the controller only gets a
    // look at attempt 1 onward — attempt 0 keeps the pre-FU-56
    // `Failed(phase)` semantics exactly, because attempt 0's plan is the
    // model's own, not the controller's extension of it. The sphere's attempt
    // 0 fails during `Routing`, before coverage is ever measured (Routing
    // precedes `CoverageMeasurement` in the stage order), so this remains a
    // plain support failure — `is_support_refusal` still claims it, and the
    // OQ-1/FU-34 reserve still substitutes the oriented solid. Measured
    // directly against this merged tree: `outcome(&report, "sphere")` is
    // `Artifact { geometry: DrilledShellRefusedBySupport { .. }, .. }`,
    // exit `Ok`, unchanged from the FU-53-only shape.
    //
    // `first_artifact.rs`'s single-model run does NOT diverge the same way,
    // and it is not a folder-sharing effect (`PrepareResourcePolicy`'s own
    // doc: caps are absolute, fixed once from the largest model this
    // workspace runs, never scaled to what else is in the batch — confirmed
    // directly: running `sphere.prepped.stl` alone through this same
    // executor hits the identical routing refusal). The real difference is
    // the **input mesh**: `first_artifact.rs` decimates and orients the RAW
    // `corpus/models/Sphere.stl` fresh, while this fixture is
    // `Sphere.prepped.stl` — the legacy prep's *output*, already oriented and
    // cleaned once — run through decimation and orientation a SECOND time, a
    // known source of divergence this same module's doc comment already
    // named for the pre-write gates ("no checked-in `.prepped.stl` is refused
    // ... because a `.prepped.stl` is the legacy prep's output, already
    // oriented and cleaned, so it seals well when run again"). FU-53 is what
    // makes that divergence land on opposite sides of the routing cap this
    // time, not a new mechanism. No cap, gate or bound moved to produce any
    // of this: the geometry moved, and a resource ceiling this smaller,
    // honestly-walled shell newly approaches on this doubly-processed fixture
    // is measured and recorded here, not raised.
    //
    // **2026-08-19, task #49, the split landing: the sphere's route moved a
    // THIRD time and then moved back.** The intermediate commit `9a9b459`
    // wired SPEC-A3-A4 §6's unrouted-regular removal AND refused when the
    // carried-load repartition was left with `sphere` face 2108 owned by
    // nobody, which took this fixture from 2 artifacts to 1. The removal
    // stays; the refusal does not. An ownerless face is now disclosed
    // (`FinalSupportAuthority::ownerless_islands`) and the coverage judgment
    // §6 sequences after the removal is what rules on it, so this pin is back
    // where `develop` @ 06cf21b had it. Measured from→to on this exact fixture
    // pair: `artifacts().count()` 1 → **2**, `exit_code()` `Supports` → **`Ok`**,
    // `outcome(.., "sphere")` `Failed` → **`Artifact { DrilledShellRefusedBySupport }`**.
    //
    // **The per-model baseline map is still proved either way:** both models
    // resolved their own baseline and both reached a stage that could only be
    // reached with it.
    assert_eq!(report.artifacts().count(), 2);
    assert_eq!(report.exit_code(), PipelineExitCode::Ok);
    let ModelPreparationOutcome::Artifact { geometry, .. } = outcome(&report, "mini") else {
        panic!("mini must reach an artifact");
    };
    assert_eq!(geometry, &SupportedGeometry::HollowStageDecidedSolid);
    let ModelPreparationOutcome::Artifact {
        geometry,
        hollow_disposition,
        ..
    } = outcome(&report, "sphere")
    else {
        panic!(
            "sphere's drilled shell now falls back to the solid reserve, not a gate refusal: {:?}",
            outcome(&report, "sphere")
        );
    };
    assert!(
        matches!(
            geometry,
            SupportedGeometry::DrilledShellRefusedBySupport { .. }
        ),
        "sphere must reach its artifact through the support-refusal reserve substitution: {geometry:?}"
    );
    // **The two lines of one report must not contradict each other.** The
    // geometry line says a drilled shell was built and refused; the
    // disposition beside it used to say `BelowThreshold`, which claims the
    // savings gate measured this cavity and found it too small. The
    // substitution measures no saving at all, so this is the real pipeline
    // pinning the truthful reason on the model that actually takes the path.
    assert_eq!(
        hollow_disposition,
        &plinth_hollow::HollowDisposition::Solid {
            reason: plinth_hollow::SolidDecisionReason::DrilledShellRefused
        },
        "the substituted solid must report the shell refusal that produced it"
    );
    assert_ne!(
        hollow_disposition,
        &plinth_hollow::HollowDisposition::Solid {
            reason: plinth_hollow::SolidDecisionReason::BelowThreshold
        },
        "a substituted solid may not report a savings verdict nobody measured"
    );
}

/// **A coverage refusal is not masked by an earlier unrelated failure.**
///
/// `aa-not-a-model.stl` sorts before `blade.stl` and fails to load, exactly
/// as a macOS `._sidecar` would. First-in-order alone would report exit 1 for
/// this folder — "one file was unreadable" — and the physics refusal, the one
/// verdict a calling script acts on, would be invisible.
///
/// **2026-08-12: this is now a REGRESSION guard, not a parked finding.** FU-56
/// gave the same safety verdict a second disposition — the closure controller
/// refuses as `Failed`/`Supports`, not as `Refused`/`Gates` — and the sticky
/// rule protecting the run-level signal only knew about `Gates`, so the
/// refusal class FU-57 exists to make visible became maskable by an unrelated
/// `Input` failure. `PrepareReport::exit_code` now makes a coverage refusal
/// sticky at second priority, behind `Gates`: a bound ADDED, no threshold
/// moved, and the remaining codes still first-in-order.
#[test]
fn a_coverage_closure_refusal_is_not_masked_by_an_earlier_load_failure() {
    let scratch = Scratch::new("sticky-gates");
    scratch.write("aa-not-a-model.stl", b"a sidecar, or a truncated download");
    refusal_fixture(&scratch, "blade");

    let report =
        prepare(pinned(&scratch, "blade", "make_raking_blade.pipeline.json")).expect("run");

    assert_eq!(
        report
            .models
            .iter()
            .map(|entry| entry.model.as_str())
            .collect::<Vec<_>>(),
        ["aa-not-a-model", "blade"],
        "the load failure must sort first for this test to mean anything"
    );
    assert!(matches!(
        outcome(&report, "aa-not-a-model"),
        ModelPreparationOutcome::Failed {
            exit_code: PipelineExitCode::Input,
            ..
        }
    ));
    // **Re-derived 2026-09-01 (owner ruling).** The blade's coverage finding is
    // no longer a refusal, so there is no verdict left to be masked and no
    // sticky exit code to assert. The claim underneath was never really about
    // the exit code though — it was "an unrelated earlier failure must not hide
    // a safety finding at the RUN level" — and that claim survives intact, with
    // the finding now reaching the run through `has_coverage_gap` instead of
    // `has_coverage_refusal`. Both are asserted, so a regression that quietly
    // turned the warning back into a refusal, or lost it altogether, fails here.
    assert!(
        outcome(&report, "blade").has_coverage_gap(),
        "the blade emits with a measured, unclosed gap: {:?}",
        outcome(&report, "blade")
    );
    assert!(!report.has_gate_refusal());
    assert!(
        !report.has_coverage_refusal(),
        "a coverage shortfall is a warning now; nothing in this run refuses for one"
    );
    assert!(
        report.has_coverage_gap(),
        "the safety finding must be visible at the RUN level, not only on the model that \
         carries it — an operator reading a folder of thirty must not have to open all thirty"
    );
    assert_eq!(
        report.exit_code(),
        PipelineExitCode::Input,
        "with the coverage verdict no longer refusing, the run's status is the load failure's \
         and nothing else's"
    );
}

/// Every `stop_after` stage really stops, at that stage, with nothing written.
///
/// `stop_after` is a production field on `PrepareRequest` and had no test at
/// all: the four branch points, the `Ok(None)` protocol between `prepare_one`,
/// `run_from_oriented` and `seal_and_emit`, and the `expect()` that turns a
/// broken protocol into a whole-run panic were all unexecuted. A future edit
/// that breaks the protocol would turn one model's disposition into a crash of
/// the folder — precisely the failure this executor exists to prevent — so the
/// contract is pinned here rather than assumed.
/// Both fixtures are run at every stop, because they take different routes
/// through the hollow stage and each route has its own branch points: `mini` is
/// decided solid, `sphere` reaches `AwaitingInternalSupport` and is drilled, and
/// only running both executes both `Hollow` checks and both paths into
/// `seal_and_emit`'s `Support` check.
#[test]
fn each_stage_stop_halts_that_model_at_that_stage_and_writes_nothing() {
    let fixtures = [
        ("mini", "make_toed_slab.prepped.stl", "make_toed_slab"),
        ("sphere", "Sphere.prepped.stl", "Sphere"),
    ];
    for after in [
        StageStop::Decimate,
        StageStop::Orient,
        StageStop::Hollow,
        StageStop::Support,
    ] {
        for (stem, fixture, baseline) in fixtures {
            let scratch = Scratch::new(&format!("stop-{after:?}-{stem}").to_lowercase());
            scratch.place(fixture, stem);
            let mut request = pinned(&scratch, stem, &format!("{baseline}.pipeline.json"));
            request.stop_after = Some(after);

            let report =
                prepare(request).expect("a stage stop is a disposition, not a run failure");
            // **A refusal that happens BEFORE the stop still wins, and must —
            // but this fixture, run alone, never hits one.** The `Support`
            // stop is checked after `run_support` returns, so a model whose
            // support run refuses never reaches it. Measured directly: every
            // fixture stops cleanly at every stage, `sphere` included, so no
            // special case is needed.
            //
            // **2026-08-19, task #49, the split landing: this pin moved and
            // moved back.** The intermediate commit `9a9b459` refused
            // `sphere` at Routing when SPEC-A3-A4 §6's removal left face 2108
            // owned by nobody, and this cell became
            // `Failed { exit_code: Supports, .. }`. The removal stays; the
            // refusal does not — an ownerless face is disclosed and the
            // coverage judgment rules on it — so the measured from→to here is
            // `Failed { exit_code: Supports, .. }` → **`Stopped { after: Support }`**,
            // back to `develop` @ 06cf21b's reading, and the special case is
            // gone with it.
            assert_eq!(
                outcome(&report, stem),
                &ModelPreparationOutcome::Stopped { after },
                "stop_after {after:?} did not stop {stem} where it was told to"
            );
            assert!(
                !scratch.output().join(format!("{stem}.chitubox")).exists(),
                "stop_after {after:?} still wrote an artifact for {stem}"
            );
            assert_eq!(report.artifacts().count(), 0);
            // A stop is not a failure: the run's exit status stays clean.
            assert_eq!(report.exit_code(), PipelineExitCode::Ok);

            // The model was measured before it stopped, at every stop this
            // executor has, so the assumed print size is reported either way.
            assert!(report.models[0].assumed_print_size.is_some());
        }
    }

    // Without a stop, the same models run to their own end — so the assertions
    // above are about `stop_after` and not about models that never get there.
    // This is the same two-model batch as the per-model-baseline test above
    // (both fixtures together): `mini` emits, and `sphere`'s attempt 0 fails
    // during `Routing` before the coverage-closure controller ever gets a
    // look, so it reaches its artifact through the existing
    // `DrilledShellRefusedBySupport` reserve substitution — unmoved by
    // FU-56/FU-57. Both endings are ones a stage `stop_after` never reached,
    // which is what this block discriminates.
    //
    // **2026-08-19, task #49, the split landing: moved and moved back.**
    // `9a9b459` took these to `artifacts().count()` 1 and
    // `has_coverage_refusal()` true by refusing on an ownerless face; the
    // refusal is withdrawn, so the measured from→to is 1 → **2** and
    // true → **false**, back to `develop` @ 06cf21b's reading.
    let scratch = Scratch::new("stop-none");
    for (stem, fixture, _) in fixtures {
        scratch.place(fixture, stem);
    }
    let report = prepare(request(
        &scratch,
        &[
            ("mini", "make_toed_slab.pipeline.json"),
            ("sphere", "Sphere.pipeline.json"),
        ],
    ))
    .expect("run");
    assert_eq!(report.artifacts().count(), 2);
    assert!(!report.has_gate_refusal());
    assert!(!report.has_coverage_refusal());
}

/// **FU-37, end to end: two runs at different walls are distinguishable in
/// what the pipeline actually writes.**
///
/// The row's own motivating claim was that "two artifacts prepared at 2.5 mm
/// and 3.0 mm are byte-indistinguishable in metadata in every file the
/// pipeline writes". `HollowReport` gained the `wall` field, but the report
/// died inside `seal_and_emit` — `ChainOutcome` had no wall field and
/// `ModelPreparationOutcome::Artifact` serialized none — so the claim stayed
/// true of real runs while the in-crate tests, which serialize a
/// `HollowReport` in isolation, passed. This runs the **whole executor**
/// twice on the same fixture, changing nothing but `--wall-mm`, and asserts
/// the serialized reports differ in the wall and only there.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn two_prepare_runs_at_different_walls_are_distinguishable_in_the_serialized_report() {
    let printer = OutputProfile::Saturn4Ultra
        .resolve()
        .expect("M0 Saturn profile")
        .clone();

    let run_at = |label: &str, wall: plinth_core::PositiveFinite| {
        let scratch = Scratch::new(label);
        scratch.place("make_toed_slab.prepped.stl", "mini");
        let mut request = pinned(&scratch, "mini", "make_toed_slab.pipeline.json");
        request.hollow = plinth_hollow::HollowRequest {
            printer: printer.clone(),
            wall_mm: wall,
        };
        let report = prepare(request).expect("run");
        let json = serde_json::to_string(&report).expect("the report serializes");
        // Drop the scratch path, which differs per run by construction and
        // says nothing about the wall.
        let outcome = outcome(&report, "mini").clone();
        (outcome, json.contains("\"wall_mm\""))
    };

    let default_wall = plinth_hollow::registered_wall(plinth_hollow::WallSelection::Default)
        .expect("registered default wall");
    let heavy_wall = plinth_hollow::registered_wall(plinth_hollow::WallSelection::HandlingHeavy)
        .expect("registered handling-heavy wall");
    assert_ne!(
        default_wall.get(),
        heavy_wall.get(),
        "the two registered walls must differ for this test to mean anything"
    );

    let (thin, thin_has_key) = run_at("wall-default", default_wall);
    let (thick, thick_has_key) = run_at("wall-heavy", heavy_wall);
    assert!(
        thin_has_key && thick_has_key,
        "the serialized report must carry the wall at all"
    );

    let wall_of = |outcome: &ModelPreparationOutcome| match outcome {
        ModelPreparationOutcome::Artifact { wall_mm, .. } => wall_mm.map(plinth_core::Finite::get),
        other => panic!("the fixture must reach an artifact; got {other:?}"),
    };
    assert_eq!(wall_of(&thin), Some(default_wall.get()));
    assert_eq!(wall_of(&thick), Some(heavy_wall.get()));
    assert_ne!(
        thin, thick,
        "two runs differing only in --wall-mm must not produce identical artifact records"
    );
}
