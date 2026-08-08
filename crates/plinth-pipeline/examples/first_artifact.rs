//! Produce the first `.chitubox` artifact ever emitted from a native sealed
//! support run, for the owner's ChituBox GUI acceptance gate (design phase E3).
//!
//! `cargo run --release --offline --example first_artifact -p plinth-pipeline`
//!
//! Writes `exports/<model>-first-artifact.chitubox` at the workspace root.
//! `exports/` is gitignored: this is a run output, not source. The composition
//! itself lives in `examples/shared/first_artifact_chain.rs` and is shared
//! verbatim with `tests/first_artifact.rs`, so the artifact handed to the GUI
//! is produced by exactly the code the test gates.

#[path = "shared/first_artifact_chain.rs"]
mod chain;

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let model = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Sphere".to_owned());
    let exports = chain::workspace_root().join("exports");
    if let Err(error) = std::fs::create_dir_all(&exports) {
        eprintln!("cannot create {}: {error}", exports.display());
        return ExitCode::FAILURE;
    }
    let target: PathBuf = exports.join(format!(
        "{}-first-artifact.chitubox",
        model.to_lowercase().replace(' ', "-")
    ));

    let outcome = match chain::run_corpus_model(&model, &target) {
        Ok(value) => value,
        Err(reason) => {
            eprintln!("CHAIN\t{model}\tUNREACHABLE\t{reason}");
            return ExitCode::FAILURE;
        }
    };
    let audit = match chain::audit_disk_artifact(&target, &model) {
        Ok(audit) => audit,
        Err(reason) => {
            eprintln!("AUDIT\t{model}\tFAILED\t{reason}");
            return ExitCode::FAILURE;
        }
    };

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
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
