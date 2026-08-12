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
    PipelineExitCode, PrepareReport, PrepareRequest, SupportedGeometry,
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
            wall_mm,
            realized_wall,
        } => {
            println!(
                "  result  EMITTED  {}  ({} support records, {} bytes)",
                emit.artifact_path.display(),
                emit.support_record_count,
                emit.artifact_bytes
            );
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
                    Some(realized) => println!(
                        "  wall  requested {} mm, realized {realized}",
                        wall.get()
                    ),
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
    print_coverage_evidence(model);
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
    for island in evidence.islands.iter().take(COVERAGE_CARD_MAX_ISLANDS_SHOWN) {
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
            refusal,
        } => format!(
            "SUBSTITUTED solid -- support refused the {drilled_faces}-face drilled shell: {refusal}"
        ),
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
        ModelPreparationReport, UncoveredIslandSummary,
    };

    use super::{coverage_lines, repair_lines};

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
        assert!(coverage_lines(&with_outcome(ModelPreparationOutcome::Stopped {
            after: plinth_pipeline::StageStop::Decimate,
        }))
        .is_empty());
        assert!(coverage_lines(&with_outcome(ModelPreparationOutcome::Refused {
            gate: Some(plinth_pipeline::PreWriteGate::Clearance),
            geometry: None,
            violation: "unrelated".to_owned(),
            evidence: None,
        }))
        .is_empty());
        assert!(coverage_lines(&with_outcome(ModelPreparationOutcome::Failed {
            exit_code: plinth_pipeline::PipelineExitCode::Supports,
            detail: "unrelated stage failure".to_owned(),
            evidence: None,
        }))
        .is_empty());
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
            lines.iter().any(|line| line.contains("(1 more in the sidecar)")),
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
        }));
        assert!(lines[0].contains("1 island(s), 1 shown worst-first"), "{lines:?}");
        assert!(
            lines.iter().any(|line| line.contains("renders") && line.contains("0 image(s)")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("render note") && line.contains("COVERAGE_RENDER_MAX_FACES")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("evidence write error") && line.contains("permission denied")),
            "{lines:?}"
        );
    }
}
