//! THROWAWAY INSTRUMENTATION -- census harness.
//!
//! Requires the `census` cargo feature (declared as `required-features` in this
//! crate's manifest), so it is invisible to an ordinary build:
//! `cargo run --release --offline --features census --example
//! embed_incidence_census`.
//!
//! Runs the exact public support-preparation path (`run_support`) over the
//! locally provisioned eight-model corpus and prints the census the
//! embed-incidence ruling needs.  Without `PLINTH_CENSUS=1` this binary is a
//! pure baseline: it prints only the sealed verification digest and the
//! candidate/surviving counts, so an un-instrumented run can be diffed against
//! an instrumented one.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use plinth_core::{LayerPlan, PositiveFinite};
use plinth_decimate::{run as run_decimation, DecimationBudget, ProbeConfig};
use plinth_hollow::{HollowedModel, SolidDecisionReason};
use plinth_mesh::{load_binary_stl, WeldPolicy};
use plinth_orient::{run as run_orientation, OrientRequest, PinnedBaseline};
use plinth_support::{run_support, SupportPreparationLimits, SupportRequest};

const MODELS: [&str; 8] = [
    "Sphere",
    "Cylinder",
    "make_box",
    "make_overhang_table",
    "make_toed_slab",
    "make_raking_blade",
    "make_thin_fin",
    "Weeping Treant v1.2",
];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn main() -> ExitCode {
    let root = workspace_root();
    let only = std::env::var("PLINTH_CENSUS_ONLY").ok();
    let mut failures = 0usize;
    for name in MODELS {
        if let Some(filter) = only.as_deref() {
            if filter != name {
                continue;
            }
        }
        plinth_support::census::reset();
        match run_model(&root, name) {
            Ok(summary) => {
                println!("MODEL\t{name}\tOK\t{summary}");
            }
            Err(reason) => {
                failures += 1;
                println!("MODEL\t{name}\tUNREACHABLE\t{reason}");
                plinth_support::census::report_failure(name);
            }
        }
    }
    if failures == MODELS.len() {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Models whose own pinned pipeline report carries no usable orientation
/// record (their legacy hollow stage failed, so `plinth-orient` refuses it).
/// With `PLINTH_CENSUS_SUBSTITUTE_ORIENT=1` the census borrows another
/// member's ratified axis/degrees so the geometry still reaches the support
/// engine. Every such row is reported as `orientation=substituted`.
const SUBSTITUTE_ORIENT: [(&str, &str); 2] =
    [("Cylinder", "Sphere"), ("make_box", "make_overhang_table")];

fn run_model(root: &Path, name: &str) -> Result<String, String> {
    let stl = root.join("corpus/models").join(format!("{name}.stl"));
    let mesh = load_binary_stl(&stl, WeldPolicy::None).map_err(|error| format!("load: {error}"))?;
    let budget =
        DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
            .map_err(|error| format!("budget: {error}"))?;
    let decimated = run_decimation(&mesh, budget)
        .map_err(|error| format!("decimate: {error}"))?
        .into_decimated_model()
        .map_err(|scale| format!("decimate-scale-skip: {scale:?}"))?;
    let substitute = (std::env::var("PLINTH_CENSUS_SUBSTITUTE_ORIENT").as_deref() == Ok("1"))
        .then(|| {
            SUBSTITUTE_ORIENT
                .iter()
                .find(|(model, _)| *model == name)
                .map(|(_, donor)| *donor)
        })
        .flatten();
    let baseline_name = substitute.unwrap_or(name);
    let oriented = run_orientation(
        decimated,
        &OrientRequest {
            pinned: PinnedBaseline {
                pipeline_report: root
                    .join("corpus/baseline")
                    .join(format!("{baseline_name}.pipeline.json")),
            },
        },
    )
    .map_err(|error| format!("orient: {error}"))?;
    let (mesh, orientation) = oriented.into_parts();
    let hollowed =
        HollowedModel::solid_decision(mesh, orientation, SolidDecisionReason::BelowThreshold)
            .map_err(|error| format!("hollow: {error}"))?;

    let max_z = hollowed
        .mesh()
        .verts()
        .iter()
        .map(|vertex| vertex[2])
        .fold(f64::NEG_INFINITY, f64::max);
    let min_z = hollowed
        .mesh()
        .verts()
        .iter()
        .map(|vertex| vertex[2])
        .fold(f64::INFINITY, f64::min);
    if !max_z.is_finite() || !min_z.is_finite() {
        return Err("degenerate z extent".to_owned());
    }
    // Deterministic layer plan: the frozen 0.05 mm pitch of the existing
    // gates, extended to cover the lifted model with a fixed 1000-layer floor.
    let pitch_mm = std::env::var("PLINTH_CENSUS_LAYER_PITCH_MM")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.05);
    let span_mm = (max_z - min_z) + 5.0 + 1.0;
    let layer_count = ((span_mm / pitch_mm).ceil() as usize).max(1_000);
    let layers = LayerPlan::new(0.0, vec![pitch_mm; layer_count])
        .map_err(|error| format!("layers: {error}"))?;
    let delicacy = vec![0.0; hollowed.mesh().tris().len()];
    let faces = hollowed.mesh().tris().len();

    let authority = run_support(
        hollowed,
        SupportRequest {
            lift_mm: PositiveFinite::new(5.0).map_err(|_| "lift".to_owned())?,
        },
        SupportPreparationLimits::production()
            .map_err(|error| format!("limits: {error}"))?
            .input(&layers, &delicacy, &[]),
    )
    .map_err(|error| format!("support: {error}"))?;
    let snapshot = authority
        .verification_snapshot()
        .map_err(|error| format!("snapshot: {error}"))?;
    let routed = snapshot.routed_contacts().count();
    let live = snapshot.final_live_members().count();
    let digest = snapshot.digest();

    plinth_support::census::finish(name, &snapshot);

    let orientation_note = if substitute.is_some() {
        "substituted"
    } else {
        "own_pinned"
    };
    Ok(format!(
        "faces={faces}\torientation={orientation_note}\tpitch_mm={pitch_mm}\tlayers={layer_count}\trouted_contacts={routed}\tfinal_live_members={live}\tsnapshot_digest={digest}"
    ))
}
