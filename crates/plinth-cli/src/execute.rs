//! The resolver the parser's own documentation named: [`PrepareSelection`] ->
//! `PrepareRequest` -> `plinth_pipeline::prepare`, plus the run report.
//!
//! Everything in a `PrepareRequest` that is *not* a CLI argument is assembled
//! here from the ratified registry, never invented: the printer profile from
//! the typed output-profile selection, the decimation probe budget and the lift
//! from `plinth_pipeline`'s registry readers, the hollow wall from the operator
//! (defaulted by the parser from `HOLLOW_WALL_DEFAULT_MM`). This module adds no
//! number of its own.
//!
//! The report on stdout is deterministic by construction: models appear in the
//! executor's own sorted order, millimetre quantities are formatted at a fixed
//! precision, and nothing reads a clock. Two runs over the same folder print
//! the same bytes.

use std::collections::BTreeMap;
use std::process::ExitCode;

use geom_kernel::telemetry::{near_parallel_counts, NearParallelCounts, NearParallelSite};
use plinth_hollow::HollowRequest;
use plinth_orient::PinnedBaseline;
use plinth_pipeline::{
    prepare, registered_decimation_budget, registered_support_request, AssumedPrintSize,
    ModelPreparationOutcome, ModelPreparationReport, OrientationPolicy, PipelineError,
    PipelineExitCode, PreWriteGate, PrepareReport, PrepareRequest, SupportedGeometry,
};

use crate::{CacheMode, OrientSelection, PlacementSelection, PrepareSelection};

/// Run one parsed `plinth prepare` invocation to completion.
///
/// Returns the process exit code, which is the pipeline's own exit-code
/// projection and not a second contract: `0` when every model reached an
/// artifact, `3` whenever **any** model in the run was refused by the pre-write
/// physics gates, and otherwise the code of the first model in report order
/// that did not reach one, per [`PipelineExitCode`].
///
/// `3` is sticky because it is the one code a calling script acts on. A folder
/// containing both an unreadable file and a gate refusal must not exit `1`: the
/// unreadable file is a nuisance, the refusal is a safety verdict, and a script
/// that only sees `1` learns nothing about it.
pub fn execute(selection: PrepareSelection) -> ExitCode {
    let request = match resolve(&selection) {
        Ok(request) => request,
        Err(error) => return fail(&error),
    };
    match prepare(request) {
        Ok(report) => {
            print_report(&selection, &report);
            exit_code(report.exit_code())
        }
        Err(error) => fail(&error),
    }
}

/// Assemble the complete execution request from the operator's selection and
/// the ratified authorities.
fn resolve(selection: &PrepareSelection) -> Result<PrepareRequest, PipelineError> {
    let printer = selection
        .output_profile
        .resolve()
        .map_err(|source| PipelineError::ResourcePolicy {
            source: source.into(),
        })?
        .clone();
    let support = registered_support_request(&printer)
        .map_err(|source| PipelineError::ResourcePolicy { source })?;
    let decimation_budget = registered_decimation_budget()
        .map_err(|source| PipelineError::ResourcePolicy { source })?;

    Ok(PrepareRequest {
        input_models_dir: selection.input_models_dir.clone(),
        output_dir: selection.output_dir.clone(),
        orientation: resolve_orientation(&selection.orientation)?,
        placement: selection.placement.into(),
        output_profile: selection.output_profile,
        decimation_budget,
        // The operator's wall, defaulted by the parser from the registry. The
        // hollow stage owns admissibility and has already refused anything it
        // cannot cut, so this threads straight through.
        hollow: HollowRequest {
            printer,
            wall_mm: selection.wall_mm,
        },
        support,
        no_cache: selection.cache.no_cache(),
        // `--stop-after` is not on the ratified command line. A stage stop is a
        // testing outcome; the CLI runs the whole chain or reports why it could
        // not.
        stop_after: None,
    })
}

/// Turn the operator's `--orient` selection into the run's policy.
///
/// This is where the filesystem is read, deliberately and not in the parser.
/// `pinned:DIR` is resolved by scanning `DIR` for `<stem>.pipeline.json` and
/// keying the policy by that stem — **the baseline directory is scanned, not
/// the model directory**, so this does not grow a second, drifting copy of the
/// executor's own model enumeration. A model whose stem the map does not name
/// is refused per model by the pipeline (`OrientationBaselineMissing`), which
/// is the same refusal it has always been: a pinned baseline is one mesh's
/// rotation and is never applied to another model.
///
/// An unreadable directory is a failure of the run, not of one model: every
/// model in it would be refused for the same reason, and reporting that N times
/// would bury the one thing wrong.
fn resolve_orientation(selection: &OrientSelection) -> Result<OrientationPolicy, PipelineError> {
    let directory = match selection {
        OrientSelection::Dense => return Ok(OrientationPolicy::DenseSweep),
        OrientSelection::Pinned(directory) => directory,
    };
    let entries = std::fs::read_dir(directory).map_err(|source| PipelineError::InputFolder {
        path: directory.clone(),
        source,
    })?;
    let mut baselines = BTreeMap::new();
    for entry in entries {
        let entry = entry.map_err(|source| PipelineError::InputFolder {
            path: directory.clone(),
            source,
        })?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(stem) = name.strip_suffix(".pipeline.json") else {
            continue;
        };
        baselines.insert(
            stem.to_owned(),
            PinnedBaseline {
                pipeline_report: path,
            },
        );
    }
    Ok(OrientationPolicy::Pinned(baselines))
}

fn fail(error: &PipelineError) -> ExitCode {
    eprintln!("plinth prepare: {error}");
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        eprintln!("  caused by: {cause}");
        source = cause.source();
    }
    exit_code(error.exit_code())
}

fn exit_code(code: PipelineExitCode) -> ExitCode {
    ExitCode::from(code as u8)
}

/// The run report: one block per model, in the executor's own order.
fn print_report(selection: &PrepareSelection, report: &PrepareReport) {
    println!(
        "plinth prepare: {} model(s) from {}",
        report.models.len(),
        selection.input_models_dir.display()
    );
    println!(
        "  out-dir {}  profile {}  wall {:.3} mm  cache {}",
        selection.output_dir.display(),
        plinth_pipeline::OutputProfile::SATURN4_ULTRA_CLI_VALUE,
        selection.wall_mm.get(),
        match selection.cache {
            // No cache exists yet. Saying "bypassed" would describe machinery
            // that is not there; this states what actually happened.
            CacheMode::Enabled => "no cache implemented (flag absent)",
            CacheMode::Disabled => "no cache implemented (--no-cache is a no-op)",
        }
    );

    // Which orientation authority ran. A searched rotation and a replayed one
    // produce the same kind of artifact, so without this line the two runs are
    // indistinguishable from their output.
    println!(
        "  orientation  {}",
        match &selection.orientation {
            OrientSelection::Dense => "dense sweep (searched per model)".to_owned(),
            OrientSelection::Pinned(directory) => format!(
                "pinned baselines from {} (per model, by stem)",
                directory.display()
            ),
        }
    );

    // Which support engine placed the field. Two runs of the same model under
    // the two engines produce the same KIND of artifact, so without this line
    // a `.chitubox` cannot be told apart from its output — and one of the two
    // ran no pre-write gate at all.
    println!(
        "  placement    {}",
        match selection.placement {
            PlacementSelection::Legacy =>
                "legacy sealed support run (pre-write gates enforced)",
            PlacementSelection::Sweep =>
                "slice-sweep core, EXPERIMENTAL: NO pre-write gate ran on any model in \
                 this run; arms lean and merge, long members are tied by |X| braces, \
                 thickened roots and masts, and braces and masts clear the model and \
                 each other; no joint spheres and no clearance check of an \
                 arm's own shaft. Per-model disclosure is printed above each model's line.",
        }
    );

    let mut emitted = 0usize;
    let mut refused = 0usize;
    let mut failed = 0usize;
    let mut stopped = 0usize;
    for model in &report.models {
        print_model(model);
        match model.outcome {
            ModelPreparationOutcome::Artifact { .. } => emitted += 1,
            ModelPreparationOutcome::Refused { .. } => refused += 1,
            ModelPreparationOutcome::Failed { .. } => failed += 1,
            ModelPreparationOutcome::Stopped { .. } => stopped += 1,
        }
    }

    println!(
        "SUMMARY  emitted={emitted}  gate-refused={refused}  failed={failed}  stopped={stopped}"
    );
    if refused > 0 {
        // Said outright, because a refused model looks like a missing file and
        // the natural next move is to go looking for the switch that turns the
        // gate off. There isn't one.
        println!(
            "  a gate-refused model has NO artifact on disk, by design; the refusal is the finding"
        );
    }
    // Read ONCE, here, after every model in the run has been prepared.
    for line in near_parallel_lines(near_parallel_counts()) {
        println!("{line}");
    }
}

/// The end-of-run near-parallel disclosure, built rather than printed so the
/// decision about *what* gets reported is testable without capturing stdout —
/// the same shape as [`repair_lines`] and [`coverage_lines`].
///
/// # What these numbers are
///
/// `geom_kernel`'s Möller routine classifies a ray/triangle determinant inside
/// `DET_EPS` as `NearParallel`: whether the ray crosses that face is
/// undecidable at this scale (`docs/specs/geometry-adjudication.md` §O4). It
/// used to be reported as a definite miss, which silently dropped real
/// interior crossings out of seat lists and drilling censuses. It is now
/// refused instead, and every refusal is counted here.
///
/// **A nonzero reading is the cost side of that fix, and it is the number the
/// owner asked for**: refusing on uncertainty can withhold an anchor seat, a
/// drain, or a whole model, and this line is where that shows up. Per-model
/// attribution is deliberately NOT claimed here — the counters are
/// process-global and cumulative (the ledger threaded per model in §EXT-4 was
/// dropped by owner ruling as larger than the defect it watched), so a model
/// that stops emitting is identified by comparing corpus dispositions, not by
/// reading this table.
///
/// # Why end of run and not the per-model card
///
/// The obvious home is beside the `safety checks` line inside
/// [`print_model`], which is the other disclosure that says what did *not*
/// get decided. It cannot go there and stay honest: [`execute`] runs
/// `prepare` to completion before this function prints a single character, so
/// at `print_model` time the counters already hold every model's total.
/// Printing them under each model would print the identical run total N times
/// and read as per-model attribution — an unearned claim of exactly the kind
/// the counters exist to stop making.
///
/// # Determinism
///
/// The tally is a sum, so thread scheduling cannot move it; it is otherwise
/// exactly as reproducible as the geometry work the run performs, which the
/// module header already claims. This function adds no clock and no ordering
/// of its own. Measured rather than only argued, 2026-09-01: two `prepare`
/// runs over the same one-model folder printed this block byte-identically
/// (`kernel.walk.collecting 8` both times, on
/// `corpus/baseline/make_toed_slab.prepped.stl`). One model, two runs — enough
/// to show the counters are not a source of nondeterminism, not enough to
/// generalise over the whole corpus.
///
/// # Shape
///
/// The header line is unconditional — a run that measured zero and a run that
/// never measured must not look alike — and one line follows per site that
/// actually fired. `NearParallelSite::ALL` drives the loop, so a site added to
/// the kernel appears here without an edit; a hand-written match arm is how a
/// site goes missing.
fn near_parallel_lines(counts: NearParallelCounts) -> Vec<String> {
    let total = counts.total();
    if total == 0 {
        return vec![format!(
            "  near-parallel geometry  none: all {} kernel call site(s) read zero for this run",
            NearParallelSite::ALL.len()
        )];
    }
    let mut lines = vec![format!(
        "  near-parallel geometry  {total} ray/triangle determinant(s) landed in the undecidable \
         band and were REFUSED rather than guessed; a refusal here can withhold a seat, a drain \
         or a whole model, and is not attributable to one model from this line:"
    )];
    for site in NearParallelSite::ALL {
        let count = counts.get(site);
        if count > 0 {
            lines.push(format!("    {}  {count}", site.label()));
        }
    }
    lines
}

fn print_model(model: &ModelPreparationReport) {
    println!("MODEL  {}", model.model.as_str());
    println!("  source  {}", model.source_path.display());
    println!(
        "  assumed print size  {}",
        print_size(model.assumed_print_size)
    );
    print_repair(model);
    // **How the model was turned, on the line above the result.**
    //
    // The engine flipped `Round 25` — a 25 mm x 3.5 mm flat disc — upside down
    // and tilted it 44.9 deg, and the only way to discover that was to pull the
    // mesh back out of the emitted file and fit its largest plane. Every number
    // here is READ off the orientation record the sweep already sealed; nothing
    // is recomputed from geometry. Printed for every model that reached the
    // stage, identity included: "it was not turned" is a measurement and a line
    // that appears only on surprising models teaches a reader to expect its
    // absence.
    if let Some(orientation) = &model.orientation {
        println!(
            "  orientation  original +Z is {:.1} deg off the build axis{}; rotation committed: {}",
            orientation.original_up_tilt_deg,
            if orientation.inverted {
                " (INVERTED — the model's own up now points DOWN)"
            } else {
                ""
            },
            orientation.selection,
        );
        // **The display-face map, published before it is trusted** (owner
        // ruling, 2026-09-03). The orientation scar term is
        // `Sum_down(area x (fragility + D))`, and D is measured on the
        // model in its AUTHORED frame. An operator must be able to see what
        // the map SAID about this model without re-running anything, so the
        // summary is printed for every searched model, high values or not:
        // "the map found almost nothing" is a measurement too.
        if let Some(display) = &orientation.display {
            println!(
                "  display face map  D area-weighted mean {:.4} (per-face mean {:.4}) over {} \
                 faces / {:.1} mm2; authored-up faces {:.4} vs authored-down faces {:.4}; field \
                 {}",
                display.area_weighted_mean,
                display.mean,
                display.faces,
                display.total_area_mm2,
                display.authored_up_area_weighted_mean,
                display.authored_down_area_weighted_mean,
                &display.digest[..12.min(display.digest.len())],
            );
            println!(
                "    area-weighted histogram of D, ten bins over [0, 1] (% of area):{}",
                display
                    .area_weighted_histogram
                    .iter()
                    .map(|share| format!(" {:.2}", share * 100.0))
                    .collect::<String>()
            );
            for face in &display.top_faces {
                println!(
                    "    highest D  face {:>7}  D {:.4}  area {:.5} mm2  authored n_z {:+.4}",
                    face.face, face.display, face.area_mm2, face.authored_normal_z,
                );
            }
        }
    }
    match &model.outcome {
        ModelPreparationOutcome::Artifact {
            emit,
            geometry,
            hollow_disposition,
            cavity_repair,
            filled_back_chambers_ml,
            wall_mm,
            realized_wall,
            savings_shortfall,
            ownerless_islands,
            contacts,
            pending_gates,
            load_warnings,
            coverage_warning,
            ..
        } => {
            println!(
                "  result  EMITTED  {}  ({} support records, {} bytes)",
                emit.artifact_path.display(),
                emit.support_record_count,
                emit.artifact_bytes
            );
            // Disclosure half of the pre-write gate's #1 review finding: this
            // file was written after only some of the twelve named safety
            // checks actually ran, because most have no Rust implementation
            // yet (see `plinth_pipeline::gates::judge`). Printed unconditionally
            // and right under EMITTED, in plain words, so an operator does not
            // have to read the source to learn that "emitted" does not mean
            // "every safety check passed" -- it means the checks that exist
            // passed, and the rest did not run at all.
            let total_gates = PreWriteGate::ALL.len();
            let ran_gates = total_gates.saturating_sub(pending_gates.len());
            if pending_gates.is_empty() {
                println!("  safety checks  all {total_gates} pre-write gates ran and passed");
            } else {
                println!(
                    "  safety checks  ran {ran_gates} of {total_gates} pre-write gates; \
                     the other {} did NOT run and could not have refused this model: {}",
                    pending_gates.len(),
                    pending_gates
                        .iter()
                        .map(|gate| format!("{gate:?}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            // WARNING MODE (owner ruling, 2026-08-29): `PreWriteGate::Load`
            // no longer refuses a contact that violates the registered
            // support area law, so an emitted model can still carry one.
            // Printed unconditionally when there is evidence to print, right
            // under the safety-checks line, in the same words a hard refusal
            // would have used -- an operator reading this must not have to
            // wonder whether "emitted" means "the area law held".
            if let Some(warnings) = load_warnings {
                println!(
                    "  LOAD LAW WARNING (not blocking)  {} of {} emitted contact(s) violate the \
                     registered support area law pi * tip_radius_mm^2 >= carried_area_mm2 / \
                     X_ALLOWED, X_ALLOWED = {}:",
                    warnings.violations.len(),
                    warnings.judged,
                    warnings.x_allowed
                );
                for violation in &warnings.violations {
                    println!("    {violation}");
                }
            }
            // What is actually in the file. Two of the four geometries are
            // substitutions -- the chain planned a drilled shell, could not
            // seal or could not hollow it, and carried the oriented solid
            // instead -- and without this line a substituted artifact reads
            // exactly like the intended one.
            println!("  geometry  {}", describe(geometry));
            println!("  hollow disposition  {hollow_disposition:?}");
            // FU-37 and FU-55, on the same line: what the run asked the shell
            // to leave, and what the engine measured it actually left. Two
            // runs at different `--wall-mm` used to print identically; this
            // is the line that tells them apart. The realized figure states
            // its own claim ("2.71 mm" vs "at least 6.73 mm") rather than
            // presenting a query bound as a measurement.
            if let Some(wall) = wall_mm {
                match realized_wall {
                    Some(realized) => {
                        println!("  wall  requested {} mm, realized {realized}", wall.get())
                    }
                    None => println!("  wall  requested {} mm, no shell measured", wall.get()),
                }
            }
            // What the well-composedness repair took out of the cavity. Silent
            // when it took nothing, because a line of zeros on every simple
            // model would train the reader to skip it -- and this is the line
            // that says a shell's saving is smaller than its wall implies.
            if let Some(repair) = cavity_repair
                .as_ref()
                .filter(|repair| repair.repaired_voxels > 0)
            {
                println!(
                    "  cavity repair  {} critical cells, {} voxels returned to material \
                     ({} cell visits, {} whole-grid passes)",
                    repair.critical_cells,
                    repair.repaired_voxels,
                    repair.cell_visits,
                    repair.full_grid_passes
                );
            }
            // 2026-08-13 owner ruling: when this model's disposition is a
            // savings refusal, print the refusal's own measurement so it is
            // legible without re-running the model. "saved 0.9 mm3 against
            // a registered 1500.0 mm3 minimum" is useful; "below threshold"
            // is not.
            if let Some(shortfall) = savings_shortfall {
                println!("  savings gate  {shortfall}");
            }
            // Resin the engine gave up on. Each line is a chamber the drilling
            // stage could not reach with a legal drain, so it made it solid
            // again rather than seal liquid resin inside the print. Printed
            // one line per chamber, and silent when there were none, so the
            // reader sees a loss only when there is one to see.
            for volume_ml in filled_back_chambers_ml {
                println!("  chamber filled back: {:.2} mL", volume_ml.get());
            }
            // Task #49 (SPEC-A3-A4 §6). Islands whose every contact went
            // unrouted, so the accepted contact set charges their faces to no
            // support at all. The model still emitted, which is the whole
            // reason this has to be said out loud: the coverage verdict
            // FORGAVE the island under the registered censor, and a
            // forgiveness nobody can see is a forgiveness nobody can audit.
            //
            // Both numbers, because they answer different questions. The
            // ownerless area is what no arm is sized to carry. The measured
            // area is what the routed layout actually leaves uncovered, which
            // is a distance question and not a provenance one. On the four
            // corpus models measured 2026-08-19 the two were EQUAL to six
            // decimals, so the second is printed as its own figure and not as
            // a reassurance about the first.
            //
            // "not measured" is printed, never `0.000000`, when the ownerless
            // faces belong to no named island: the coverage measurement groups
            // its patches by island, so for those faces there is no measured
            // figure at all — and a zero would read as a measurement that was
            // taken and came back clean. Silent when there are no ownerless
            // islands, so the reader sees a loss only when there is one.
            for island in ownerless_islands {
                let measured = island.measured_uncovered_area_mm2().map_or_else(
                    || "not measured".to_owned(),
                    |area| format!("{area:.6} mm²"),
                );
                println!(
                    "  ownerless island  {}  {} face(s), {:.6} mm² charged to no support; \
                     measured uncovered {measured}",
                    island
                        .island_id()
                        .map_or_else(|| "(no island)".to_owned(), |id| id.to_string()),
                    island.ownerless_face_count(),
                    island.ownerless_face_area_mm2(),
                );
            }
            // PR-8 grow-to-attest. A tip is sized for its LOAD, and the same
            // radius is what the attestation probes with, so a load-sized thin
            // tip at a model silhouette edge can find nothing but air. Where
            // that is curable the run GREW the tip until it attested and built
            // the grown one; this is that line, printed because a support the
            // run resized on its own initiative is a decision the operator
            // should be able to see. Radius AND diameter, because the slicer UI
            // speaks diameter and the registry speaks radius.
            //
            // WITH THE SEAT, in the same `(x, y, z)` mm form the ABANDONED LOAD
            // lines use. A grown tip leaves a bigger scar than the load law
            // asked for, and an operator who cannot LOCATE it cannot inspect it,
            // sand it, or tell the run it chose badly. A contact id alone names
            // a decision without naming a place.
            for grown in &contacts.grown_contact_tips {
                println!(
                    "  grew contact tip  contact {} at ({:.3}, {:.3}, {:.3}) mm  \
                     {:.4} -> {:.4} mm radius \
                     ({:.4} -> {:.4} mm diameter) over {} rung(s), keeping {:.4} mm² of load",
                    grown.contact_id(),
                    grown.seat_mm()[0],
                    grown.seat_mm()[1],
                    grown.seat_mm()[2],
                    grown.law_tip_radius_mm(),
                    grown.emitted_tip_radius_mm(),
                    grown.law_tip_radius_mm() * 2.0,
                    grown.emitted_tip_diameter_mm(),
                    grown.growth_rungs().len(),
                    grown.kept_area_mm2(),
                );
            }
            // WARNING MODE (owner ruling, 2026-09-01). The coverage gate used
            // to REFUSE this model and write nothing; it now measures, reports
            // and lets the file through. Printed right beside the load-law
            // warning and in the same shape, because an operator reading
            // "EMITTED" must not have to wonder whether it means "every region
            // is supported".
            //
            // Every island it names is a place to look in the slicer, with the
            // area it leaves bare and the reason nothing reached it, so the gap
            // can be hand-supported or handed back as a defect report.
            if let Some(warning) = coverage_warning {
                for line in coverage_gap_warning_lines(warning) {
                    println!("{line}");
                }
            }
            // The SUMMARY of the same refusals, printed FIRST and grouped by
            // cause (owner ruling, 2026-09-01). The per-contact lines below are
            // the record; this is the line an operator can act on without
            // reading four hundred of them. A run that discards forty
            // placements has to SAY it discarded forty placements, in one
            // place, and say where they were and which stage rejected them —
            // otherwise the emitted file reads as a clean success.
            for line in support_gap_warning_lines(&contacts.abandoned_contacts) {
                println!("{line}");
            }
            // And the refusals growth could NOT cure. Each line is a support
            // that is not there and a routed load nothing carries — the
            // coverage measurement's 6 mm radius cannot see a missing claw tip,
            // so if this line does not say it, nothing does. Silent when there
            // are none.
            for contact in &contacts.abandoned_contacts {
                println!(
                    "  ABANDONED LOAD  contact {} at ({:.3}, {:.3}, {:.3}) mm  {}: \
                     {:.4} mm² carried by nothing",
                    contact.contact_id(),
                    contact.seat_mm()[0],
                    contact.seat_mm()[1],
                    contact.seat_mm()[2],
                    contact.refusal(),
                    contact.abandoned_area_mm2(),
                );
                println!("{}", remedies_tried_line(contact));
            }
        }
        ModelPreparationOutcome::Refused {
            gate,
            geometry,
            violation,
            ..
        } => {
            println!(
                "  result  GATE-REFUSED  {}  (exit {})",
                gate.map_or_else(
                    || "no single gate owns this refusal".to_owned(),
                    |gate| format!("{gate:?}")
                ),
                PipelineExitCode::Gates as u8
            );
            if let Some(geometry) = geometry {
                println!("  refused geometry  {geometry:?}");
            }
            println!("  violation  {violation}");
        }
        ModelPreparationOutcome::Failed {
            exit_code, detail, ..
        } => {
            println!("  result  ERROR  (exit {})", *exit_code as u8);
            println!("  detail  {detail}");
        }
        ModelPreparationOutcome::Stopped { after } => {
            println!("  result  STOPPED after {after:?}");
        }
    }
    for line in coverage_repair_lines(model) {
        println!("{line}");
    }
    print_coverage_evidence(model);
}

/// **SPEC-A4 §A.8's third destination: what the one-winner coverage-repair
/// controller SPENT on this model.** One line, every disposition — emitted,
/// gate-refused, or failed.
///
/// **Printed unconditionally, and deliberately not behind a verbosity
/// switch.** Ruling R10 deleted the failed-tries cap and put visibility in its
/// place; a cost printed only when someone asks for it is a cost that is
/// hidden, and the trade R10 made would not hold. `trials=0 commits=0` is the
/// ordinary model that covered on its baseline layout and paid nothing, and
/// saying so is what makes the expensive model legible beside it.
///
/// **Outside the disposition match, on purpose.** This used to be printed
/// inside the `Artifact` arm, which put it on the sealed path only — and the
/// refused path is the one with no failed-tries cap, the one that runs the
/// candidate pool all the way out, the expensive half. Reading it through
/// [`ModelPreparationOutcome::repair_report`] out here means one call site
/// instead of one per variant, and that accessor's match is exhaustive, so a
/// new disposition is a compile error rather than another silent omission.
///
/// Built rather than printed for the `repair_lines` pattern's reason: what
/// gets reported is testable without capturing stdout.
fn coverage_repair_lines(model: &ModelPreparationReport) -> Vec<String> {
    let Some(repair) = model.outcome.repair_report() else {
        return Vec::new();
    };
    vec![format!("  coverage repair  {}", repair.summary_line())]
}

/// **The `remedies tried` line under one ABANDONED LOAD, in full.**
///
/// A returned `String` rather than an inline `println!` for the reason the
/// repair card's line builder is one: the wording IS the deliverable here, and
/// a wording nothing reads back is a wording nothing checks.
///
/// **The registered ceiling is NAMED, not just quoted as a number.** A refused
/// contact's operator needs to know WHICH rule stopped the rescue — the
/// registered scar maximum, which is Craig's field ceiling on how big a scar a
/// rescue may leave, or the load law's own legacy clamp. Two limits 0.6 mm
/// apart are not distinguishable by eye in a log.
///
/// **...but a limit may be blamed only where it stopped something.** A buried
/// seat never enters the growth ladder at all: the OD-9 probe is a point on the
/// surface normal, so burial is a mesh fault no tip radius can move. One
/// sentence saying the rescue climbed "up to the registered maximum" would let
/// such an operator read Craig's field ceiling as the reason his support is
/// missing when no rescue was ever attempted. Both branches therefore quote the
/// same ceiling, so the number never changes meaning between two lines, and the
/// branch that walked no rung says in words that the ceiling is not its cause.
///
/// Radius AND diameter at every site: this registry speaks radius and the
/// slicer UI speaks diameter, and a number stated in one alone lands 2x wrong.
/// The loud, located, attributed warning for coverage the emitted file does
/// NOT have (owner ruling, 2026-09-01).
///
/// One header line with the totals, then one line per uncovered island — worst
/// first, as the summary ranks them — naming the island, its measured bare
/// area, how many faces it spans and what the closest routed support managed.
/// The search's own give-up reason is printed once, at the end, because it is a
/// statement about the ENGINE rather than about any one island and it is the
/// line that says which code to go and look at.
fn coverage_gap_warning_lines(warning: &plinth_pipeline::CoverageGapWarning) -> Vec<String> {
    let mut lines = vec![format!(
        "  COVERAGE GAP WARNING (not blocking)  {} island(s) are UNSUPPORTED in this file, \
         {:.6} mm² in total. The file was written anyway so the gap can be inspected \
         and hand-supported; it is NOT a covered model.",
        warning.islands.len(),
        warning.total_uncovered_mm2,
    )];
    for island in &warning.islands {
        let gap = match island.worst_gap {
            plinth_pipeline::CoverageClosureGap::NoRoutedContact => {
                "no routed support reached this island at all".to_owned()
            }
            plinth_pipeline::CoverageClosureGap::Finite(distance_mm) => format!(
                "the nearest routed support is {distance_mm:.3} mm away, outside the \
                 registered coverage radius"
            ),
        };
        lines.push(format!(
            "    island {}  {:.6} mm² bare across {} face(s)  — {gap}",
            island.island_id,
            island.uncovered_area_mm2,
            island.face_count(),
        ));
        // §E.1's per-route verdict, indented under its own island. The line
        // below it says why the SEARCH stopped; this says what happened to THIS
        // island, and an operator holding a file with three bare islands needs
        // the second one to know whether to look at the pool, the router or the
        // placement search. Silent when the controller stated none, because a
        // blank is a truer report than a guess.
        if let Some(outcome) = &island.search_outcome {
            lines.push(format!("      route  {outcome}"));
        }
    }
    if let Some(reason) = &warning.closure_reason {
        lines.push(format!(
            "      stage  coverage-repair controller gave up after {} committed step(s): {reason}",
            warning.closure_attempts,
        ));
    }
    lines
}

/// What each refusal string means, in the operator's terms: which stage made
/// the decision and what test it failed. A gap he can LOCATE is worth
/// something; a gap he can locate AND attribute to a named stage is worth much
/// more, because it points at the code to change.
///
/// The strings are the ones `plinth-support`'s `contact_attestation_disclosures`
/// projects. An unrecognised string still prints — with the string itself and
/// no invented explanation — because a disposition that gained a name without
/// gaining a sentence here must not become invisible on that account.
fn refusal_meaning(refusal: &str) -> &'static str {
    match refusal {
        "undecidable_geometry" => {
            "PR-8 contact attestation — a ray/triangle crossing at this seat fell in the \
             undecidable band, so whether the tip reaches solid could not be settled at this \
             scale. This is a property of the SEAT, not of the model: a seat a fraction of a mm \
             away is decidable, so the placement search is what to look at"
        }
        "not_attached" => {
            "PR-8 contact attestation — the tip's probe found no solid at this seat, and the \
             growth ladder could not reach one under the registered scar ceiling"
        }
        "buried_surface" => {
            "OD-9 exterior probe — the seat lies inside the printed material, so there is no \
             outer surface here to attach to. This is a mesh fault, and no tip radius moves it"
        }
        "unusable_geometry" => {
            "PR-8 contact attestation — the evidence for this seat could not be graded at all"
        }
        _ => {
            "no explanation is registered for this refusal string; see \
             `refusal_meaning` in plinth-cli"
        }
    }
}

/// The grouped, counted, located warning for supports the run did NOT build.
///
/// Empty when nothing was abandoned, so a clean run stays clean. Everything it
/// reports is derived from the disclosure list itself — there is no second
/// counter that could drift from the record below it.
fn support_gap_warning_lines(
    abandoned: &[plinth_pipeline::AbandonedContactDisclosure],
) -> Vec<String> {
    if abandoned.is_empty() {
        return Vec::new();
    }
    let total_mm2: f64 = abandoned
        .iter()
        .map(plinth_pipeline::AbandonedContactDisclosure::abandoned_area_mm2)
        .sum();
    let mut lines = vec![format!(
        "  SUPPORT GAP WARNING (not blocking)  {} placement(s) were REJECTED and not built; \
         {total_mm2:.4} mm² of routed load is carried by nothing. The file was still written — \
         these areas are unsupported in it.",
        abandoned.len()
    )];
    // Grouped by cause, worst first by count. `BTreeMap` keeps the grouping
    // deterministic before the sort, so two runs over the same evidence print
    // the same lines.
    let mut by_reason: BTreeMap<&str, Vec<&plinth_pipeline::AbandonedContactDisclosure>> =
        BTreeMap::new();
    for contact in abandoned {
        by_reason.entry(contact.refusal()).or_default().push(contact);
    }
    let mut groups = by_reason.into_iter().collect::<Vec<_>>();
    groups.sort_by(|left, right| right.1.len().cmp(&left.1.len()).then(left.0.cmp(right.0)));
    for (reason, contacts) in groups {
        let count = contacts.len();
        let area: f64 = contacts
            .iter()
            .map(|contact| contact.abandoned_area_mm2())
            .sum();
        // The centroid of the rejected seats and the furthest seat from it.
        // Together they say "look HERE, over about this much of the model",
        // which is what makes the warning usable in a slicer without hunting
        // through the per-contact lines one at a time.
        let scale = 1.0 / count as f64;
        let centre: [f64; 3] = std::array::from_fn(|axis| {
            contacts
                .iter()
                .map(|contact| contact.seat_mm()[axis])
                .sum::<f64>()
                * scale
        });
        let spread = contacts
            .iter()
            .map(|contact| {
                contact
                    .seat_mm()
                    .iter()
                    .zip(centre)
                    .map(|(seat, mid)| (seat - mid) * (seat - mid))
                    .sum::<f64>()
                    .sqrt()
            })
            .fold(0.0_f64, f64::max);
        lines.push(format!(
            "    {reason}  {count} placement(s), {area:.4} mm² unsupported, centred near \
             ({:.3}, {:.3}, {:.3}) mm and spread up to {spread:.3} mm from there",
            centre[0], centre[1], centre[2],
        ));
        lines.push(format!("      cause  {}", refusal_meaning(reason)));
    }
    lines
}

fn remedies_tried_line(contact: &plinth_pipeline::AbandonedContactDisclosure) -> String {
    let law_radius_mm = contact.law_tip_radius_mm();
    let ceiling_radius_mm = contact.growth_ceiling_radius_mm();
    if contact.growth_ladder_entered() {
        format!(
            "    remedies tried  tip {law_radius_mm:.4} mm radius \
             ({:.4} mm diameter) from the load law, then {} growth rung(s) up to the \
             registered {ceiling_radius_mm:.4} mm radius ({:.4} mm diameter), the maximum set \
             by {}; re-seat: {}",
            law_radius_mm * 2.0,
            contact.growth_rungs().len(),
            ceiling_radius_mm * 2.0,
            contact.growth_ceiling_constant(),
            contact.reseat(),
        )
    } else {
        format!(
            "    remedies tried  tip {law_radius_mm:.4} mm radius \
             ({:.4} mm diameter) from the load law; NO growth rung was tried, because no tip \
             radius can move this refusal — the registered {ceiling_radius_mm:.4} mm radius \
             ({:.4} mm diameter) maximum from {} is NOT what stopped this one; re-seat: {}",
            law_radius_mm * 2.0,
            ceiling_radius_mm * 2.0,
            contact.growth_ceiling_constant(),
            contact.reseat(),
        )
    }
}

/// **FU-57.** A coverage refusal -- from either the pre-write gate or the
/// FU-56 coverage-closure controller -- keeps its evidence: the worst
/// islands with their uncovered mm², where the full sidecar and the renders
/// landed, and any write failure along the way. Empty for a model that
/// emitted, stopped, or was refused/failed for a reason that carries no
/// coverage evidence, which is what makes 21 refusals rankable without a
/// rebuild the ruling asks for.
fn print_coverage_evidence(model: &ModelPreparationReport) {
    for line in coverage_lines(model) {
        println!("{line}");
    }
}

/// How many of a refusal's worst islands the card prints before pointing the
/// reader at the sidecar for the rest.
const COVERAGE_CARD_MAX_ISLANDS_SHOWN: usize = 3;

/// The coverage-evidence block's lines, built rather than printed -- the
/// `repair_lines` pattern (see its own doc), so what gets reported is
/// testable without capturing stdout.
fn coverage_lines(model: &ModelPreparationReport) -> Vec<String> {
    let Some(evidence) = model.outcome.coverage_evidence() else {
        return Vec::new();
    };
    let total = evidence.islands.len();
    let shown = total.min(COVERAGE_CARD_MAX_ISLANDS_SHOWN);
    let mut lines = vec![format!(
        "  uncovered islands  {total} island(s), {shown} shown worst-first"
    )];
    // A coverage refusal whose measurement itself failed has no islands to
    // rank, and "0 island(s)" on its own reads as "nothing was uncovered" —
    // the opposite of what happened. Name the failure instead (FU-57).
    if let Some(error) = &evidence.measurement_error {
        lines.push(format!("  coverage unmeasurable  {error}"));
    }
    for island in evidence
        .islands
        .iter()
        .take(COVERAGE_CARD_MAX_ISLANDS_SHOWN)
    {
        let gap = match island.worst_gap {
            plinth_pipeline::CoverageClosureGap::Finite(value) => {
                format!("worst gap {value:.2} mm")
            }
            plinth_pipeline::CoverageClosureGap::NoRoutedContact => "NO ROUTED CONTACT".to_owned(),
        };
        lines.push(format!(
            "    island {}   {:.3} mm2 uncovered   {gap}   {} faces",
            island.island_id,
            island.uncovered_area_mm2,
            island.face_count()
        ));
    }
    if total > shown {
        lines.push(format!("    ... ({} more in the sidecar)", total - shown));
    }
    lines.push(format!("  evidence  {}", evidence.sidecar_path.display()));
    if evidence.render_paths.is_empty() {
        lines.push("  renders   0 image(s)".to_owned());
    } else {
        let paths = evidence
            .render_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "  renders   {} image(s): {paths}",
            evidence.render_paths.len()
        ));
    }
    if let Some(skipped) = &evidence.render_skipped {
        lines.push(format!("  render note  {skipped}"));
    }
    if let Some(error) = &evidence.write_error {
        lines.push(format!("  evidence write error  {error}"));
    }
    lines
}

/// What the automatic repair pre-pass did to this model, printed whenever it
/// did anything at all **or declined to**.
///
/// **An automatic edit to the owner's geometry that nobody is told about is
/// not automation, it is a silent modification.** The 2026-08-10 ruling made
/// repair automatic and the reporting is the other half of that bargain: a
/// model whose input was altered says so, on the same card as its result, with
/// the counts per defect class. A model that needed nothing prints nothing —
/// the pre-pass returned its input bit-identical and there is nothing to
/// report.
///
/// **A refusal is not nothing.** A model whose only findings are
/// refusal-shaped — the shard strip skipped because the raw input triages as
/// pre-hollowed, say — changed no face, so gating this block on "did the pass
/// change anything" would print a blank card and then let the model be
/// refused with no stated reason. Those entries were already in the
/// serialized evidence and are now in the human-readable report too.
fn print_repair(model: &ModelPreparationReport) {
    for line in repair_lines(model) {
        println!("{line}");
    }
}

/// The repair block's lines, built rather than printed, so the decision about
/// *what* gets reported is testable without capturing stdout.
fn repair_lines(model: &ModelPreparationReport) -> Vec<String> {
    let Some(repair) = &model.repair else {
        return Vec::new();
    };
    let has_finding = repair.changed()
        || repair.actions_skipped.is_some()
        || repair.shards.skipped.is_some()
        || repair.refused_to_empty.is_some()
        || repair.duplicates.degenerate_groups_skipped > 0;
    if !has_finding {
        return Vec::new();
    }
    let mut lines = vec![format!("  repaired  {}", repair.summary_line())];
    if let Some(reason) = &repair.actions_skipped {
        // The pass did not run at all. That is not a refusal — the model is
        // going on to every stage below — but it means this model got *less
        // repair* than a smaller one would, so if it is refused downstream the
        // reader needs this line to know why nobody tried (FU-45).
        lines.push(format!("  repair skipped  {reason}"));
    }
    if let Some(refusal) = repair.refused_to_empty {
        // The loudest line the block can print: the pass found a plan that
        // deleted every triangle in the model and declined to apply it. The
        // geometry below is the operator's own, unmodified.
        lines.push(format!(
            "  repair refused  applying this pass would have removed all {} face(s) ({} shard + {} duplicate); nothing was applied and the input is unchanged",
            refusal.source_faces, refusal.shard_faces_planned, refusal.duplicate_faces_planned
        ));
    }
    if let Some(reason) = &repair.shards.skipped {
        lines.push(format!("  repair note  {reason}"));
    }
    if repair.duplicates.degenerate_groups_skipped > 0 {
        lines.push(format!(
            "  repair left in place  {} degenerate duplicate group(s), which belong to the edge-collapse doctrine and not to this pass",
            repair.duplicates.degenerate_groups_skipped
        ));
    }
    if let Some(residual) = repair.residual {
        if !residual.is_clean() {
            lines.push(format!(
                "  repair residual  {residual:?}  (the gate below judges this, unchanged)"
            ));
        }
    }
    lines
}

/// One line naming the geometry the artifact was actually built from, and
/// saying outright when that is not the geometry the pipeline set out to make.
fn describe(geometry: &SupportedGeometry) -> String {
    match geometry {
        SupportedGeometry::DrilledShellSealed => {
            "drilled hollow shell (the intended geometry)".to_owned()
        }
        SupportedGeometry::HollowStageDecidedSolid => {
            "solid, by the hollow stage's own decision".to_owned()
        }
        SupportedGeometry::DrilledShellRefusedBySupport {
            drilled_faces,
            stage,
            refusal,
        } => {
            // The stage as a value when the record has one; a record written
            // before the field existed says so rather than inventing a stage.
            let at = stage.map_or_else(
                || "a support stage this record does not name".to_owned(),
                |stage| stage.to_string(),
            );
            format!(
                "SUBSTITUTED solid -- the {drilled_faces}-face drilled shell was built and \
                 refused at {at}: {refusal}"
            )
        }
        SupportedGeometry::HollowStageRefused { refusal } => {
            format!("SUBSTITUTED solid -- the hollow stage refused this geometry: {refusal}")
        }
    }
}

/// The assumed-print-size line.
///
/// Reported for every model that got as far as being measured, not only for
/// the ones that failed on it: a mini silently processed at a tenth of its
/// intended size produces a plausible artifact and an unusable print, and this
/// line is the only place that is visible.
fn print_size(size: Option<AssumedPrintSize>) -> String {
    size.map_or_else(
        || "unknown (the model was never measured)".to_owned(),
        |size| {
            format!(
                "longest axis {:.3} mm  (bounding box {:.3} x {:.3} x {:.3} mm)",
                size.longest_axis_mm,
                size.dimensions_mm[0],
                size.dimensions_mm[1],
                size.dimensions_mm[2]
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use plinth_core::ModelId;
    use plinth_mesh::RepairEvidence;
    use plinth_pipeline::{
        CoverageClosureGap, CoverageRefusalEvidence, EvidenceSource, ModelPreparationOutcome,
        ModelPreparationReport, RepairRunReport, UncoveredIslandSummary,
    };

    use geom_kernel::telemetry::{NearParallelCounts, NearParallelSite};

    use super::{
        coverage_gap_warning_lines, coverage_lines, coverage_repair_lines, near_parallel_lines,
        remedies_tried_line, repair_lines,
    };

    /// Counts with exactly one site charged.
    ///
    /// The `match` is deliberate: a site added to the kernel breaks THIS
    /// function, and the breakage lands in the one test that walks every site.
    /// A helper written with `..Default::default()` and a field name would let
    /// a new site slip through silently, which is the failure this whole
    /// change is about.
    fn only(site: NearParallelSite, n: u64) -> NearParallelCounts {
        let mut counts = NearParallelCounts::default();
        match site {
            NearParallelSite::CollectingWalk => counts.collecting_walk = n,
            NearParallelSite::CountingWalk => counts.counting_walk = n,
            NearParallelSite::SegTriPierce => counts.seg_tri_pierce = n,
        }
        counts
    }

    /// **A quiet run and an unmeasured run must not look alike.**
    ///
    /// The brief for this block asked for one line per NONZERO site, which on
    /// its own would print nothing at all on any run where no site fires. That
    /// output is indistinguishable from a build where the counters were never
    /// wired up — the exact "check nobody reads" shape the near-parallel work
    /// exists to remove. So the header is unconditional and states the zero.
    ///
    /// Not a hypothetical either direction: measured 2026-09-01, a `prepare`
    /// run over `corpus/baseline/make_toed_slab.prepped.stl` reads
    /// `kernel.walk.collecting 8` and still emits, while other real models
    /// read zero. Both readings have to be legible.
    #[test]
    fn a_run_that_measured_zero_says_so_instead_of_printing_nothing() {
        let lines = near_parallel_lines(NearParallelCounts::default());
        assert_eq!(lines.len(), 1, "the zero case is one stated line: {lines:?}");
        assert!(lines[0].contains("near-parallel geometry"), "{lines:?}");
        assert!(lines[0].contains("none"), "{lines:?}");
        assert!(
            lines[0].contains(&NearParallelSite::ALL.len().to_string()),
            "the zero line must say how many sites read zero: {lines:?}"
        );
    }

    /// Every site the kernel defines can reach the operator's screen.
    ///
    /// Walked from `NearParallelSite::ALL` rather than named one by one: a
    /// site that exists in the kernel and is missing from the report is a
    /// counter that fires into a void, and hand-written arms are how that
    /// happens.
    #[test]
    fn every_kernel_site_reaches_the_report_when_it_fires() {
        for site in NearParallelSite::ALL {
            let lines = near_parallel_lines(only(site, 3));
            assert_eq!(
                lines.len(),
                2,
                "one header plus one row for {}: {lines:?}",
                site.label()
            );
            // The whole row, not a `contains`: the label alone would still
            // pass with the count dropped, and the count is the finding.
            assert_eq!(
                lines[1],
                format!("    {}  3", site.label()),
                "site {} did not reach the report intact: {lines:?}",
                site.label()
            );
        }
    }

    /// A site that did not fire prints no row, and the header totals the ones
    /// that did.
    #[test]
    fn only_the_sites_that_fired_get_a_row_and_the_header_sums_them() {
        let counts = NearParallelCounts {
            collecting_walk: 4,
            counting_walk: 0,
            seg_tri_pierce: 7,
        };
        let lines = near_parallel_lines(counts);
        assert_eq!(lines.len(), 3, "header plus two fired sites: {lines:?}");
        assert!(
            lines[0].contains("11"),
            "the header states the total across sites: {lines:?}"
        );
        assert!(
            lines[0].contains("REFUSED"),
            "the header must say the uncertain rows were refused, not guessed: {lines:?}"
        );
        assert!(
            lines[0].contains("not attributable to one model"),
            "the header must decline the per-model attribution these global counters cannot \
             support: {lines:?}"
        );
        let rows = lines[1..].join("\n");
        assert!(
            rows.contains(NearParallelSite::CollectingWalk.label())
                && rows.contains(NearParallelSite::SegTriPierce.label()),
            "{lines:?}"
        );
        assert!(
            !rows.contains(NearParallelSite::CountingWalk.label()),
            "a site that never fired must not get a row: {lines:?}"
        );
    }

    fn report(repair: Option<RepairEvidence>) -> ModelPreparationReport {
        ModelPreparationReport {
            model: ModelId::new("fixture"),
            source_path: std::path::PathBuf::from("fixture.stl"),
            assumed_print_size: None,
            repair,
            orientation: None,
            outcome: ModelPreparationOutcome::Stopped {
                after: plinth_pipeline::StageStop::Decimate,
            },
        }
    }

    /// **Every scar the run CHOSE must be locatable, not merely counted.**
    ///
    /// The two PR-8 disclosure lines report decisions about a specific place on
    /// the model: a load nothing carries, and a tip the run enlarged past what
    /// the load law asked for. A contact id names the decision; only the seat
    /// names the PLACE, and an operator who cannot find the site can neither
    /// inspect it nor overrule it.
    ///
    /// This is a source-text guard because the two lines are `println!`s inside
    /// the outcome match rather than a returned `*_lines` list. It looks for the
    /// composed call that reads the third coordinate, so dropping the coordinate
    /// arguments from either line fails here even though the struct field
    /// survives.
    ///
    /// **Every needle is composed at run time, and that is not decoration.** A
    /// guard that reads its own file and looks for a literal it CONTAINS is
    /// vacuous: the literal in the test satisfies the search whatever the
    /// production line does. Measured — this test passed with the seat arguments
    /// deleted from the print until the needles were composed instead of
    /// written out.
    #[test]
    fn both_contact_disclosure_lines_state_where_the_scar_is() {
        let source = include_str!("execute.rs");
        let reads_third_coordinate = |binding: &str, field: &str| format!("{binding}.{field}()[2]");
        for (owner, binding) in [
            ("the ABANDONED LOAD line", "contact"),
            ("the grew-contact-tip line", "grown"),
        ] {
            assert!(
                source.contains(&reads_third_coordinate(binding, "seat_mm")),
                "{owner} no longer prints its seat, so the scar it reports \
                 cannot be found on the model"
            );
        }
        // Non-vacuity: the guard can tell a present call from an absent one.
        assert!(!source.contains(&reads_third_coordinate("grown", "seat_of_nothing")));
    }

    /// One refused contact, deserialized exactly as a frozen run report carries
    /// it, because the disclosure's fields are private to `plinth-support`.
    ///
    /// The numbers are the toed slab's own, MEASURED 2026-08-20 through
    /// `plinth-support`'s public runner: contact 25 at (-5.495, -0.938, 3.750)
    /// mm is a buried seat that never entered the ladder, and contact 64 at
    /// (10.928, 0.000, 6.304) mm is the one the scar bound withdraws after six
    /// rungs. Both abandon 0.0000 mm², and only the second was stopped by the
    /// registered maximum.
    fn refused_contact(ladder_entered: bool) -> plinth_pipeline::AbandonedContactDisclosure {
        let (contact_id, seat, refusal, rungs) = if ladder_entered {
            (
                64,
                "[10.92820323027551,0.0,6.303847577293367]",
                "not_attached",
                r#"[{"radius_mm":0.4,"diameter_mm":0.8,"attested":false}]"#,
            )
        } else {
            (
                25,
                "[-5.49519052838329,-0.9375,3.75]",
                "buried_surface",
                "[]",
            )
        };
        serde_json::from_str(&format!(
            r#"{{"contact_id":{contact_id},"seat_mm":{seat},
                "abandoned_area_mm2":0.0,"refusal":"{refusal}",
                "law_tip_radius_mm":0.05,"growth_ceiling_radius_mm":0.4,
                "growth_ceiling_constant":"TIP_RADIUS_MAX_GROWN_SCAR_MM",
                "growth_ladder_entered":{ladder_entered},
                "growth_rungs":{rungs},
                "reseat":"no_alternative_seat_at_this_stage"}}"#
        ))
        .expect("a refused-contact disclosure deserializes as a frozen report carries it")
    }

    /// **A registered limit may be blamed only where it actually stopped
    /// something.**
    ///
    /// Both wordings quote the same registered ceiling, so one number never
    /// changes meaning between two lines. But a buried seat never enters the
    /// growth ladder — burial is a mesh fault no tip radius moves — so a single
    /// sentence blaming the ceiling would invite an operator to read the
    /// owner's field limit as the reason his support is missing when no rescue
    /// was ever attempted.
    ///
    /// Measured on the rendered line, not on the source text: this asserts what
    /// the operator reads.
    #[test]
    fn the_refusal_line_blames_the_registered_ceiling_only_where_it_stopped_a_rescue() {
        let walked = remedies_tried_line(&refused_contact(true));
        assert!(
            walked.contains(
                "then 1 growth rung(s) up to the registered 0.4000 mm radius \
                             (0.8000 mm diameter), the maximum set by \
                             TIP_RADIUS_MAX_GROWN_SCAR_MM"
            ),
            "a rescue the registered maximum really did stop must say so: {walked}"
        );
        let unwalked = remedies_tried_line(&refused_contact(false));
        assert!(
            unwalked.contains("NO growth rung was tried"),
            "a refusal that never entered the ladder must say no rung was tried: {unwalked}"
        );
        assert!(
            unwalked.contains(
                "the registered 0.4000 mm radius (0.8000 mm diameter) maximum from \
                 TIP_RADIUS_MAX_GROWN_SCAR_MM is NOT what stopped this one"
            ),
            "the ceiling must still be stated in both units and named, and disclaimed as the \
             cause: {unwalked}"
        );
        // The wording that would MISLEAD must be absent, not merely outweighed.
        assert!(
            !unwalked.contains("up to the registered"),
            "a rescue that was never attempted must not read as one that climbed to the \
             ceiling: {unwalked}"
        );
        assert!(
            !walked.contains("NOT what stopped this one"),
            "the real bound refusal must not disclaim the bound: {walked}"
        );
        // Both units at every site, on both branches, and the law radius the
        // load law asked for is stated so the reader can size the gap.
        for line in [&walked, &unwalked] {
            assert!(
                line.contains("tip 0.0500 mm radius (0.1000 mm diameter) from the load law"),
                "every branch states the law radius in BOTH units: {line}"
            );
            assert!(
                line.contains("re-seat: no_alternative_seat_at_this_stage"),
                "every branch states why re-seating was not among the remedies: {line}"
            );
        }
    }

    #[test]
    fn a_model_the_pre_pass_had_nothing_to_do_to_prints_nothing() {
        assert!(repair_lines(&report(None)).is_empty());
        assert!(repair_lines(&report(Some(RepairEvidence::default()))).is_empty());
    }

    /// **The refusal-shaped model.** Nothing was changed, so a `changed()`
    /// gate would print an empty card and leave the reader with a gate refusal
    /// and no stated cause. Each of the remaining refusal shapes has to reach
    /// the report on its own.
    #[test]
    fn a_pre_pass_that_only_refused_still_reports_what_it_refused() {
        let mut skipped = RepairEvidence::default();
        skipped.shards.skipped = Some("raw input triages as pre-hollowed".to_owned());
        let lines = repair_lines(&report(Some(skipped)));
        assert!(
            lines.iter().any(|line| line.contains("pre-hollowed")),
            "{lines:?}"
        );

        let mut degenerate = RepairEvidence::default();
        degenerate.duplicates.degenerate_groups_skipped = 2;
        let lines = repair_lines(&report(Some(degenerate)));
        assert!(
            lines.iter().any(|line| line.contains("degenerate")),
            "{lines:?}"
        );

        // The pass declining to run *at all* changes nothing by definition, so
        // a `changed()` gate hides the one line that explains why a model
        // nobody tried to repair was refused downstream (FU-45).
        let skipped_whole = RepairEvidence {
            actions_skipped: Some("the raw input is over the pre-pass's resource bound".to_owned()),
            ..RepairEvidence::default()
        };
        let lines = repair_lines(&report(Some(skipped_whole)));
        assert!(
            lines
                .iter()
                .any(|line| line.contains("repair skipped") && line.contains("resource bound")),
            "{lines:?}"
        );

        // The refuse-to-empty case changed nothing *by construction* — that is
        // the whole point of it — so a `changed()` gate would hide the single
        // most important thing this pass can report.
        let refused = RepairEvidence {
            source_faces: 2,
            refused_to_empty: Some(plinth_mesh::EmptyResultRefusal {
                source_faces: 2,
                shard_faces_planned: 0,
                duplicate_faces_planned: 2,
            }),
            ..RepairEvidence::default()
        };
        let lines = repair_lines(&report(Some(refused)));
        assert!(
            lines
                .iter()
                .any(|line| line.contains("would have removed all 2 face(s)")),
            "{lines:?}"
        );
    }

    /// **SPEC-A4 §A.8's third destination is printed on EVERY disposition, not
    /// only the one that emitted.**
    ///
    /// Ruling R10 removed the failed-tries cap and paid for it with
    /// visibility. The refused path is where the cap used to bite — no cap, so
    /// the candidate pool runs all the way out — and it was the one path whose
    /// spend never reached the owner's summary. The three asserts below are
    /// the sabotage-visible ones: drop the print out of any disposition and
    /// one of them goes red.
    #[test]
    fn the_repair_spend_is_printed_on_the_refused_and_failed_cards_too() {
        let spent = RepairRunReport {
            trials: 45,
            commits: 0,
            candidates_remaining: 0,
            ..RepairRunReport::default()
        };
        let refused = coverage_repair_lines(&with_outcome(ModelPreparationOutcome::Refused {
            gate: Some(plinth_pipeline::PreWriteGate::UncoveredIslands),
            geometry: None,
            violation: "nine uncovered islands".to_owned(),
            evidence: None,
            repair_report: Some(Box::new(spent.clone())),
        }));
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert!(
            refused[0].contains("coverage repair")
                && refused[0].contains("trials=45")
                && refused[0].contains("commits=0"),
            "a gate-refused model must state what the controller spent: {refused:?}"
        );

        let failed = coverage_repair_lines(&with_outcome(ModelPreparationOutcome::Failed {
            exit_code: plinth_pipeline::PipelineExitCode::Supports,
            detail: "coverage closure refused".to_owned(),
            evidence: None,
            repair_report: Some(Box::new(spent)),
        }));
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert!(
            failed[0].contains("trials=45") && failed[0].contains("remaining=0"),
            "a controller-refused model must state what the controller spent: {failed:?}"
        );

        // And silence where there is genuinely no ledger: a model that never
        // reached a sealed run must not print `trials=0`, which would claim
        // the loop ran and found nothing to do.
        let unreadable = coverage_repair_lines(&with_outcome(ModelPreparationOutcome::Failed {
            exit_code: plinth_pipeline::PipelineExitCode::Input,
            detail: "not a binary STL".to_owned(),
            evidence: None,
            repair_report: None,
        }));
        assert!(unreadable.is_empty(), "{unreadable:?}");
        let stopped = coverage_repair_lines(&with_outcome(ModelPreparationOutcome::Stopped {
            after: plinth_pipeline::StageStop::Decimate,
        }));
        assert!(stopped.is_empty(), "{stopped:?}");
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn island(id: u32, area: f64, gap: CoverageClosureGap, faces: usize) -> UncoveredIslandSummary {
        UncoveredIslandSummary {
            island_id: id,
            uncovered_area_mm2: area,
            worst_gap: gap,
            face_ids: (0..faces as u32).collect(),
            // The per-route verdict is exercised by its own test below; the
            // shared helper stays at "not stated" so every other assertion
            // keeps reading the island line it always read.
            search_outcome: None,
        }
    }

    /// **The per-route verdict prints under its own island, and stays silent
    /// when there is none.**
    ///
    /// Measured on `Round 25`: the coverage-repair pre-screen took 45 of that
    /// island's 46 pool rows, and before this line the warning said only "no
    /// routed support reached this island at all" — true, and useless for
    /// deciding whether to look at the pool, the router or the placement
    /// search. The island-level verdict answers that; the run-level
    /// `stage` line below it does not, because it is about the SEARCH.
    #[test]
    fn a_per_route_verdict_prints_under_its_island_and_only_when_stated() {
        let mut stated = island(4, 6.878_205, CoverageClosureGap::NoRoutedContact, 45);
        stated.search_outcome =
            Some("Blocked(PoolScreened { pool_rows: 46, screened: 45 })".to_owned());
        let silent = island(2, 0.656_66, CoverageClosureGap::NoRoutedContact, 3);
        let lines = coverage_gap_warning_lines(&plinth_pipeline::CoverageGapWarning {
            islands: vec![stated, silent],
            total_uncovered_mm2: 7.534_865,
            closure_reason: None,
            closure_attempts: 0,
        });
        let route_lines: Vec<&String> = lines
            .iter()
            .filter(|line| line.trim_start().starts_with("route  "))
            .collect();
        assert_eq!(
            route_lines.len(),
            1,
            "exactly the island that HAS a verdict gets a route line: {lines:?}"
        );
        assert!(
            route_lines[0].contains("PoolScreened { pool_rows: 46, screened: 45 }"),
            "the line must carry how many rows the screen took: {lines:?}"
        );
        let island_index = lines
            .iter()
            .position(|line| line.contains("island 4"))
            .expect("the island line is printed");
        assert_eq!(
            lines[island_index + 1],
            *route_lines[0],
            "the verdict belongs to the island directly above it, not to the block: {lines:?}"
        );
    }

    fn with_outcome(outcome: ModelPreparationOutcome) -> ModelPreparationReport {
        ModelPreparationReport {
            model: ModelId::new("fixture"),
            source_path: std::path::PathBuf::from("fixture.stl"),
            assumed_print_size: None,
            repair: None,
            orientation: None,
            outcome,
        }
    }

    /// **FU-57.** An emitted or non-coverage outcome prints nothing: the
    /// block is evidence for a coverage refusal, not a generic footer.
    #[test]
    fn coverage_lines_is_empty_without_coverage_evidence() {
        assert!(
            coverage_lines(&with_outcome(ModelPreparationOutcome::Stopped {
                after: plinth_pipeline::StageStop::Decimate,
            }))
            .is_empty()
        );
        assert!(
            coverage_lines(&with_outcome(ModelPreparationOutcome::Refused {
                gate: Some(plinth_pipeline::PreWriteGate::Clearance),
                geometry: None,
                violation: "unrelated".to_owned(),
                evidence: None,
                repair_report: None,
            }))
            .is_empty()
        );
        assert!(
            coverage_lines(&with_outcome(ModelPreparationOutcome::Failed {
                exit_code: plinth_pipeline::PipelineExitCode::Supports,
                detail: "unrelated stage failure".to_owned(),
                evidence: None,
                repair_report: None,
            }))
            .is_empty()
        );
    }

    /// **FU-57.** Worst-first, truncated at the shown count with an explicit
    /// "more in the sidecar" line, a finite gap and `NO ROUTED CONTACT`
    /// rendered distinctly, and the sidecar/render paths always printed.
    #[test]
    fn coverage_lines_ranks_worst_first_and_truncates_with_a_named_remainder() {
        let evidence = CoverageRefusalEvidence {
            source: EvidenceSource::PreWriteGate,
            gate: Some(plinth_pipeline::PreWriteGate::UncoveredIslands),
            geometry: None,
            attempts: None,
            islands: vec![
                island(12, 3.418, CoverageClosureGap::Finite(7.94), 431),
                island(7, 1.002, CoverageClosureGap::NoRoutedContact, 88),
                island(3, 0.5, CoverageClosureGap::Finite(1.0), 4),
                island(9, 0.1, CoverageClosureGap::Finite(0.2), 1),
            ],
            sidecar_path: std::path::PathBuf::from("out/Giant Toad.coverage.json"),
            render_paths: vec![
                std::path::PathBuf::from("out/Giant Toad.coverage.island-12.svg"),
                std::path::PathBuf::from("out/Giant Toad.coverage.island-7.svg"),
                std::path::PathBuf::from("out/Giant Toad.coverage.island-3.svg"),
            ],
            render_skipped: None,
            write_error: None,
            measurement_error: None,
        };
        let lines = coverage_lines(&with_outcome(ModelPreparationOutcome::Refused {
            gate: Some(plinth_pipeline::PreWriteGate::UncoveredIslands),
            geometry: None,
            violation: "gate UncoveredIslands refused emission with 4 blocking defect(s)"
                .to_owned(),
            evidence: Some(Box::new(evidence)),
            repair_report: None,
        }));

        assert!(
            lines[0].contains("4 island(s), 3 shown worst-first"),
            "{lines:?}"
        );
        assert!(
            lines.iter().any(|line| line.contains("island 12")
                && line.contains("3.418 mm2 uncovered")
                && line.contains("worst gap 7.94 mm")
                && line.contains("431 faces")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("island 7") && line.contains("NO ROUTED CONTACT")),
            "{lines:?}"
        );
        // The 4th island is not shown, and the remainder is named rather than
        // silently dropped.
        assert!(
            !lines.iter().any(|line| line.contains("island 9")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("(1 more in the sidecar)")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("evidence") && line.contains("Giant Toad.coverage.json")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("renders") && line.contains("3 image(s)")),
            "{lines:?}"
        );
    }

    /// **FU-57.** A `Failed` disposition (the coverage-closure controller's
    /// refusal today, per `ModelPreparationOutcome::coverage_evidence`) is
    /// read exactly like a gate refusal's evidence, and a write failure is
    /// surfaced rather than silently dropped.
    #[test]
    fn coverage_lines_reads_a_failed_dispositions_closure_evidence_and_names_write_errors() {
        let evidence = CoverageRefusalEvidence {
            source: EvidenceSource::CoverageClosureController,
            gate: None,
            geometry: None,
            attempts: Some(8),
            islands: vec![island(0, 2.0, CoverageClosureGap::NoRoutedContact, 12)],
            sidecar_path: std::path::PathBuf::from("out/overhang.coverage.json"),
            render_paths: Vec::new(),
            render_skipped: Some("mesh over COVERAGE_RENDER_MAX_FACES".to_owned()),
            write_error: Some("permission denied".to_owned()),
            measurement_error: None,
        };
        let lines = coverage_lines(&with_outcome(ModelPreparationOutcome::Failed {
            exit_code: plinth_pipeline::PipelineExitCode::Supports,
            detail: "coverage closure refused".to_owned(),
            evidence: Some(Box::new(evidence)),
            repair_report: None,
        }));
        assert!(
            lines[0].contains("1 island(s), 1 shown worst-first"),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("renders") && line.contains("0 image(s)")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("render note")
                    && line.contains("COVERAGE_RENDER_MAX_FACES")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("evidence write error")
                    && line.contains("permission denied")),
            "{lines:?}"
        );
    }
}
