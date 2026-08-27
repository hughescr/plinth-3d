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
use plinth_mesh::{
    evaluate_delicacy, load_binary_stl, DelicacyBuildConfig, DelicacySettings, WeldPolicy,
};
use plinth_orient::{run as run_orientation, OrientRequest, PinnedBaseline};
use plinth_support::{run_support, SupportPreparationLimits, SupportRequest};

/// FU-32 J0 — the signal-triggered half of the flush (§D4). A `SIGTERM`
/// (the watchdog's new first signal, ahead of `SIGKILL`) or `SIGINT` (a
/// manual Ctrl-C) requests an immediate flush on the census module's own
/// background thread; this handler does nothing else, on purpose.
///
/// Declared via a raw `extern "C"` prototype for POSIX `signal(3)` rather
/// than a new crate dependency: `libc` is already linked into every Unix
/// Rust binary through `std`, so this adds no new dependency and no new
/// entry to `Cargo.toml`/`Cargo.lock`.
#[cfg(unix)]
mod termination_signal {
    use std::os::raw::c_int;

    const SIGINT: c_int = 2;
    const SIGTERM: c_int = 15;

    extern "C" {
        fn signal(signum: c_int, handler: extern "C" fn(c_int)) -> usize;
    }

    /// Must stay async-signal-safe: [`plinth_support::census::request_flush_on_next_entry`]
    /// is documented as exactly that (a single relaxed atomic store, no
    /// allocation, no lock, no I/O) -- nothing else may be added here.
    extern "C" fn handle(_signum: c_int) {
        plinth_support::census::request_flush_on_next_entry();
    }

    /// Install the handler for both signals. Best-effort: if `signal(3)`
    /// fails there is no async-signal-safe way to report it, and the
    /// periodic (time-based) flush still runs regardless.
    pub fn install() {
        unsafe {
            signal(SIGINT, handle);
            signal(SIGTERM, handle);
        }
    }
}

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
    // FU-32 J0. Best-effort and harmless when `PLINTH_CENSUS` is unset: the
    // handler only ever stores a flag that nothing polls unless recording is
    // enabled.
    #[cfg(unix)]
    termination_signal::install();

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
        // FU-32 J0. Tags the periodic/signal-triggered flush's rows without
        // any routing code carrying a model string through the whole
        // preparation/routing stack -- see `census::current_model`'s doc
        // comment.
        plinth_support::census::set_current_model(name);
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

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
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
    let oriented = run_orientation(
        decimated,
        &OrientRequest {
            pinned: PinnedBaseline {
                pipeline_report: root
                    .join("corpus/baseline")
                    .join(format!("{name}.pipeline.json")),
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
    // FU-38 (2026-08-12): re-pinned from the `vec![0.0; faces]` all-zeros
    // feed to the real producer-side delicacy field. Computed exactly as
    // `plinth_pipeline::prepare::prepared_delicacy_field` computes it in
    // production -- same LOD name, same registered `DelicacySettings`, same
    // scaled `DelicacyBuildConfig` -- inlined rather than imported because
    // this crate cannot depend on `plinth-pipeline` (the dependency runs the
    // other way: `plinth-pipeline` depends on `plinth-support`). See
    // `docs/architecture.md`
    // and `docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`
    // §5.
    //
    // The OD-8 census digests this harness prints move with this change, and
    // the FULL from->to table -- both columns re-measured at this tip by
    // building this harness twice, once with the `vec![0.0; faces]` feed
    // restored verbatim from `44b8df1` and once with the real field -- was
    // recorded under the FU-38 row and its dated section
    // (`#fu38-real-delicacy-repin-2026-08-12`; git history only, that record's
    // own document has since been retired). In summary: SIX models seal and
    // all six digests move (`Sphere` 2297157889062365577 -> 686822247060573262,
    // `Cylinder` 6853793710942395608 -> 12410021645461158056, `make_box`
    // 8709076999423814412 -> 5356317665317762409, `make_overhang_table`
    // 11652461737312715963 -> 14600685200423759332, `make_toed_slab`
    // 133000971864625681 -> 16855162491340010017, `make_thin_fin`
    // 14656364680512457190 -> 14718557305548010128); `make_raking_blade`
    // refuses coverage closure BYTE-IDENTICALLY under both fields, and
    // `Weeping Treant v1.2` fails `Failed(Preparation)` under both, so neither
    // is attributable to this change. Unlike the FU-32 J4 movement, counts DO
    // move here -- `Sphere` 7/15 -> 9/21 and `Cylinder` 54/119 -> 45/101 --
    // because delicacy breaks exact-Z ties in contact selection, which is the
    // divergence FU-38 was opened against.
    //
    // The "from" column is deliberately NOT the J4 septet: the coverage-closure
    // lane (`1d407a0`/`6ddf051`, merged `b9decd5`) had already moved
    // `make_overhang_table` and taken `make_raking_blade` off the sealed list
    // before this change touched anything, and charging FU-38 for another
    // lane's movement would be a false attribution.
    let delicacy_field = evaluate_delicacy(
        hollowed.mesh(),
        "prepared",
        DelicacySettings::production().map_err(|error| format!("delicacy settings: {error}"))?,
        DelicacyBuildConfig::production(hollowed.mesh().tris().len()),
    )
    .map_err(|error| format!("delicacy: {error}"))?;
    let delicacy = delicacy_field
        .face_values()
        .iter()
        .map(|value| value.weight())
        .collect::<Vec<_>>();
    let faces = hollowed.mesh().tris().len();

    let input = SupportPreparationLimits::production()
        .map_err(|error| format!("limits: {error}"))?
        .input(&layers, &delicacy, &[]);
    // Report the model-scaled caps this model actually got, and its face count,
    // on the failure path too: a refusal without the bound it was measured
    // against is not a measurement.
    let scaled_caps = format!(
        "faces={faces}\tmax_grid_bytes={}\tknn_max_work={}",
        input.max_grid_bytes(),
        input.minima_knn_max_work(),
    );

    let authority = run_support(
        hollowed,
        SupportRequest {
            lift_mm: PositiveFinite::new(5.0).map_err(|_| "lift".to_owned())?,
        },
        input,
    )
    .map_err(|error| format!("{scaled_caps}\tsupport: {error}"))?;
    let snapshot = authority
        .verification_snapshot()
        .map_err(|error| format!("snapshot: {error}"))?;
    let routed = snapshot.routed_contacts().count();
    let live = snapshot.final_live_members().count();
    let digest = snapshot.digest();

    plinth_support::census::finish(name, &snapshot);

    // Every corpus member now orients from its own repaired baseline; there is
    // no donor-substitution path left.
    Ok(format!(
        "{scaled_caps}\torientation=own_pinned\tpitch_mm={pitch_mm}\tlayers={layer_count}\trouted_contacts={routed}\tfinal_live_members={live}\tsnapshot_digest={digest}"
    ))
}
