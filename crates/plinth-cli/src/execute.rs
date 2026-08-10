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

use std::process::ExitCode;

use plinth_hollow::HollowRequest;
use plinth_pipeline::{
    prepare, registered_decimation_budget, registered_support_request, AssumedPrintSize,
    ModelPreparationOutcome, ModelPreparationReport, PipelineError, PipelineExitCode,
    PrepareReport, PrepareRequest,
};

use crate::{CacheMode, PrepareSelection};

/// Run one parsed `plinth prepare` invocation to completion.
///
/// Returns the process exit code, which is the pipeline's own exit-code
/// projection and not a second contract: `0` when every model reached an
/// artifact, and otherwise the code of the first model in report order that did
/// not — `3` for a pre-write gate refusal, per [`PipelineExitCode`].
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
        orientation: selection.orientation.clone(),
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
    match &model.outcome {
        ModelPreparationOutcome::Artifact { emit } => println!(
            "  result  EMITTED  {}  ({} support records, {} bytes)",
            emit.artifact_path.display(),
            emit.support_record_count,
            emit.artifact_bytes
        ),
        ModelPreparationOutcome::Refused {
            gate,
            geometry,
            violation,
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
        ModelPreparationOutcome::Failed { exit_code, detail } => {
            println!("  result  ERROR  (exit {})", *exit_code as u8);
            println!("  detail  {detail}");
        }
        ModelPreparationOutcome::Stopped { after } => {
            println!("  result  STOPPED after {after:?}");
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
