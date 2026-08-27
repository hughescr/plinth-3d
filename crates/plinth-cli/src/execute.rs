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

use plinth_hollow::HollowRequest;
use plinth_orient::PinnedBaseline;
use plinth_pipeline::{
    prepare, registered_decimation_budget, registered_support_request, AssumedPrintSize,
    ModelPreparationOutcome, ModelPreparationReport, OrientationPolicy, PipelineError,
    PipelineExitCode, PreWriteGate, PrepareReport, PrepareRequest, SupportedGeometry,
};

use crate::{CacheMode, OrientSelection, PrepareSelection};

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
}

fn print_model(model: &ModelPreparationReport) {
    println!("MODEL  {}", model.model.as_str());
    println!("  source  {}", model.source_path.display());
    println!(
        "  assumed print size  {}",
        print_size(model.assumed_print_size)
    );
    print_repair(model);
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
            if let Some(repair) = cavity_repair.filter(|repair| repair.repaired_voxels > 0) {
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

    use super::{coverage_lines, coverage_repair_lines, remedies_tried_line, repair_lines};

    fn report(repair: Option<RepairEvidence>) -> ModelPreparationReport {
        ModelPreparationReport {
            model: ModelId::new("fixture"),
            source_path: std::path::PathBuf::from("fixture.stl"),
            assumed_print_size: None,
            repair,
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
        }
    }

    fn with_outcome(outcome: ModelPreparationOutcome) -> ModelPreparationReport {
        ModelPreparationReport {
            model: ModelId::new("fixture"),
            source_path: std::path::PathBuf::from("fixture.stl"),
            assumed_print_size: None,
            repair: None,
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
