//! Produce `.chitubox` artifacts from native sealed support runs, for the
//! owner's ChituBox GUI acceptance gate (design phase E3 / corpus sweep).
//!
//! `cargo run --release --offline --example first_artifact -p plinth-pipeline -- Sphere`
//! `cargo run --release --offline --example first_artifact -p plinth-pipeline -- --all`
//!
//! With a model name, writes that one model's artifact (defaulting to
//! `Sphere` when no argument is given). With `--all`, writes one artifact for
//! every frozen-digest corpus model except the Weeping Treant, which stays out
//! of the sweep for the reasons on `CORPUS_MODELS` — but is fully reachable by
//! name, and that is how its checkpoint artifact was produced:
//!
//! `cargo run --release --offline --example first_artifact -p plinth-pipeline -- "Weeping Treant v1.2"`
//!
//! Writes `exports/<model-slug>-first-artifact.chitubox` at the workspace
//! root. `exports/` is gitignored: this is a run output, not source. The
//! composition itself lives in `examples/shared/first_artifact_chain.rs` and
//! is shared verbatim with `tests/first_artifact.rs`, so every artifact
//! handed to the GUI is produced by exactly the code the test gates.
//!
//! `--all` keeps going past a per-model failure: an unreachable model is a
//! finding to report, not a reason to withhold the artifacts that did seal.
//! The final exit code reflects whether every attempted model succeeded.
//!
//! **Several models now report `GATES` instead of an artifact, and that is
//! correct.** The pre-write coverage gate judges the sealed plan before
//! anything is written, so a model whose plan leaves uncovered islands
//! produces no `.chitubox` at all rather than a plausible-looking one. As of
//! 2026-08-09 that is `Sphere`, `make_overhang_table` and
//! `make_raking_blade`; `make_box`, `make_toed_slab` and `make_thin_fin`
//! still seal and emit. Do not "fix" this by relaxing the gate — the gate has
//! no override, by design (`03-physics-and-constants.md`, the retirement of
//! `PERF_GATE_EMIT_ON_VERIFY`). The uncovered islands are the finding.

#[path = "shared/first_artifact_chain.rs"]
mod chain;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The seven frozen-digest corpus models this sweep covers. The Weeping Treant
/// is deliberately excluded, and **the reason has changed** (2026-08-09).
///
/// The old reason — "its support tournament cannot yet complete" — is dead:
/// FU-32 phase J6 sealed the treant twice, at 429,120 faces, in 470.8 s and
/// 470.6 s with identical counters, so it completes. Two reasons stand in its
/// place, and neither is "it does not work":
///
/// 1. **It is not a frozen-digest model.** This constant is the frozen-digest
///    set and the treant has never been in it (§J3.9, §J4.1); a sweep over that
///    set is the wrong place to add it.
/// 2. **It costs minutes, not seconds.** The other seven finish this sweep in
///    well under a minute combined; the treant alone is ~8 minutes of seal plus
///    emit, which would turn a quick sweep into a long one for every caller.
///
/// It is reachable by name and that is how its checkpoint artifact is made:
/// `-- "Weeping Treant v1.2"`. **Its provenance changed on 2026-08-11 and this
/// note is corrected with it.** It used to reach the artifact through the
/// hollow-stage fallback (`SupportedGeometry::HollowStageRefused`) because
/// `plinth_hollow::run` refused its shell — FU-34, then FU-43. The
/// well-composedness repair closed that refusal, so the treant now hollows,
/// drains and drills, and a checkpoint artifact made today is built from a
/// **drilled shell**, not from the oriented-solid reserve. Anyone comparing a
/// checkpoint artifact against one made before that date is comparing two
/// different geometries.
const CORPUS_MODELS: [&str; 7] = [
    "Sphere",
    "Cylinder",
    "make_box",
    "make_overhang_table",
    "make_toed_slab",
    "make_raking_blade",
    "make_thin_fin",
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let exports = chain::workspace_root().join("exports");
    if let Err(error) = std::fs::create_dir_all(&exports) {
        eprintln!("cannot create {}: {error}", exports.display());
        return ExitCode::FAILURE;
    }

    if args.first().map(String::as_str) == Some("--all") {
        let mut failures = Vec::new();
        for model in CORPUS_MODELS {
            if let Err(reason) = run_one(model, &exports) {
                eprintln!("FINDING\t{model}\t{reason}");
                failures.push((model, reason));
            }
        }
        if failures.is_empty() {
            ExitCode::SUCCESS
        } else {
            eprintln!(
                "SUMMARY\t{} of {} models failed: {}",
                failures.len(),
                CORPUS_MODELS.len(),
                failures
                    .iter()
                    .map(|(model, _)| *model)
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            ExitCode::FAILURE
        }
    } else {
        let model = args.first().cloned().unwrap_or_else(|| "Sphere".to_owned());
        match run_one(&model, &exports) {
            Ok(()) => ExitCode::SUCCESS,
            Err(reason) => {
                eprintln!("FINDING\t{model}\t{reason}");
                ExitCode::FAILURE
            }
        }
    }
}

/// Run the whole chain for one model, write its artifact, audit the bytes on
/// disk, and print the same diagnostics the original single-model runner
/// printed. Returns `Err` with the reason on any failure, without touching
/// the filesystem outside the one target path.
fn run_one(model: &str, exports: &Path) -> Result<(), String> {
    let target: PathBuf = exports.join(format!(
        "{}-first-artifact.chitubox",
        model.to_lowercase().replace(' ', "-")
    ));

    let outcome = chain::run_corpus_model(model, &target).map_err(|reason| {
        if reason.is_gates() {
            format!("GATES\tREFUSED\t{reason}")
        } else {
            format!("CHAIN\tUNREACHABLE\t{reason}")
        }
    })?;
    let audit = chain::audit_disk_artifact(&target, model)
        .map_err(|reason| format!("AUDIT\tFAILED\t{reason}"))?;

    println!("ARTIFACT\t{}", target.display());
    println!(
        "CHAIN\t{}\tsource_faces={}\tprepared_faces={}\thollow={:?}",
        outcome.model, outcome.source_faces, outcome.prepared_faces, outcome.hollow_disposition,
    );
    println!("GEOMETRY\t{:?}", outcome.supported_geometry);
    println!(
        "SEAL\trouted_contacts={}\tfinal_live_members={}\tsnapshot_digest={}\tprimitives={}\tlift_mm={}\tplate_z_mm={}",
        outcome.routed_contacts,
        outcome.final_live_members,
        outcome.snapshot_digest,
        outcome.primitive_count,
        outcome.lift_mm,
        outcome.plate_z_mm,
    );
    println!(
        "GATES\tpassed=uncovered_islands\tnot_yet_ported={:?}",
        outcome.pending_gates,
    );
    println!(
        "EMIT\trecords={}\tbytes={}",
        outcome.emit.support_record_count, outcome.emit.artifact_bytes,
    );
    println!(
        "DISK\tmodel={}\tmodel_triangles={}\trecords={}\tby_type={:?}",
        audit.model_name,
        audit.model_triangle_count,
        audit.support_record_count,
        audit.records_by_segment_type,
    );
    println!(
        "WINDING\toutward={}/{}\tmin_record_volume_mm3={}\tmodel_volume_mm3={}",
        audit.outward_wound_records,
        audit.support_record_count,
        audit.min_record_signed_volume_mm3,
        audit.model_signed_volume_mm3,
    );
    println!(
        "PREVIEW\tbytes={}\tnative_fallback={}",
        audit.preview_bytes, audit.preview_is_native_fallback,
    );
    println!(
        "GATE\tformat_gate_passed={}\tfile_bytes={}",
        audit.format_gate_passed, audit.file_bytes,
    );

    if audit.format_gate_passed && audit.outward_wound_records == audit.support_record_count {
        Ok(())
    } else {
        Err(format!(
            "GATE\tformat_gate_passed={}\toutward={}/{}",
            audit.format_gate_passed, audit.outward_wound_records, audit.support_record_count
        ))
    }
}
