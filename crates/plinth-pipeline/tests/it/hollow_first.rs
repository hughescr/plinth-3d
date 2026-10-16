//! **Hollow first** through the whole `prepare()` door (lane hf, 2026-10-16;
//! `docs/plans/hollow-first-2026-10-16.md`).
//!
//! With hollow first on, the orientation search measures each production
//! finalist on the drilled shell the hollow stage makes for that pose, so the
//! winner's kept plan is the hollowed plan and the seal emits it instead of
//! sweeping the shell a second time. Two things must hold:
//!
//! 1. A model the hollow stage does not hollow is measured on the solid on
//!    every pose, so its file is the same bytes with the switch on or off.
//! 2. A hollowed model's final placement REUSES the winner's shell plan (the
//!    key: mesh bits, layer bits, policy, drains and vents bit for bit), and
//!    the file is the same bytes as the same run with the plan made in the seal
//!    (`--no-cache`), which proves the reused plan is the plan the seal makes.
//!
//! Path hits are asserted before bytes: the plan source and the hollow
//! disposition, so a fixture that stopped hollowing, or a reuse that silently
//! missed, fails loudly.

use std::path::{Path, PathBuf};

use plinth_pipeline::prepare::{prepare_with_overrides, StageOverrides, SweepPlanSource};
use plinth_pipeline::{
    registered_decimation_budget, registered_support_request, ModelPreparationOutcome,
    OrientationPolicy, OutputProfile, PrepareReport, PrepareRequest,
};
use plinth_sweep::orient_demand::ReuseMiss;

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "plinth-hollow-first-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("in")).expect("scratch input folder");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "test-local f32 snap of a bounded procedural fixture, the narrowing an .stl performs"
)]
fn place(scratch: &Scratch, mesh: &plinth_mesh::Mesh, stem: &str) {
    let verts: Vec<[f64; 3]> = mesh
        .verts()
        .iter()
        .map(|vertex| vertex.map(|value| f64::from(value as f32)))
        .collect();
    let snapped = plinth_mesh::Mesh::new(verts, mesh.tris().to_vec()).expect("a valid mesh");
    plinth_mesh::write_binary_stl(scratch.0.join("in").join(format!("{stem}.stl")), &snapped)
        .expect("write the fixture");
}

/// `plinth prepare --output saturn4-ultra --orient dense --placement sweep`,
/// with `--no-cache` as given.
fn request(scratch: &Scratch, run: &str, no_cache: bool) -> PrepareRequest {
    let printer = OutputProfile::Saturn4Ultra.resolve().expect("M0 Saturn profile").clone();
    PrepareRequest {
        input_models_dir: scratch.0.join("in"),
        output_dir: scratch.0.join(run),
        orientation: OrientationPolicy::DenseSweep,
        no_orient: Default::default(),
        placement: plinth_pipeline::Placement::Sweep,
        output_profile: OutputProfile::Saturn4Ultra,
        decimation_budget: registered_decimation_budget().expect("registered budget"),
        hollow: plinth_hollow::HollowRequest::with_registered_wall_default(printer.clone())
            .expect("registered wall"),
        support: registered_support_request(&printer).expect("registered lift"),
        no_cache,
        exact_delicacy: false,
        stop_after: None,
    }
}

/// The switch's default is on, and only `PLINTH_HOLLOW_FIRST=0` turns it off.
/// (Every run below names its mode, so none of them depends on the default.)
#[test]
fn hollow_first_is_on_unless_the_environment_says_zero() {
    use std::ffi::OsStr;
    use plinth_pipeline::prepare::hollow_first_from_value;
    assert!(hollow_first_from_value(None), "unset: the default is on");
    assert!(hollow_first_from_value(Some(OsStr::new("1"))));
    assert!(hollow_first_from_value(Some(OsStr::new(""))));
    assert!(hollow_first_from_value(Some(OsStr::new("off"))), "only exactly 0 opts out");
    assert!(!hollow_first_from_value(Some(OsStr::new("0"))));
}

/// One run with the mode named: `true` is hollow first, `false` is the solid-first search.
fn run(scratch: &Scratch, label: &str, no_cache: bool, hollow_first: bool) -> PrepareReport {
    prepare_with_overrides(
        request(scratch, label, no_cache),
        StageOverrides {
            reuse: None,
            hollow_first: Some(hollow_first),
        },
    )
    .expect("the sweep route emits and warns")
}

/// The artifact's bytes, where its plan came from, and its hollow disposition.
fn emitted(
    report: &PrepareReport,
    stem: &str,
) -> (Vec<u8>, Option<SweepPlanSource>, plinth_hollow::HollowDisposition) {
    let entry = report
        .models
        .iter()
        .find(|entry| entry.model.as_str() == stem)
        .expect("the model is in the report");
    let ModelPreparationOutcome::Artifact {
        emit,
        sweep_plan_source,
        hollow_disposition,
        ..
    } = &entry.outcome
    else {
        panic!("{stem} must reach an artifact; it reported {:?}", entry.outcome);
    };
    let bytes = std::fs::read(Path::new(&emit.artifact_path)).expect("the artifact is on disk");
    (bytes, sweep_plan_source.0, *hollow_disposition)
}

/// **Flag-off byte identity.** A model the hollow stage does not hollow keeps
/// its bytes with hollow first on or off (`--no-hollow-first` is the old search).
#[test]
fn a_model_that_is_not_hollowed_keeps_its_bytes_with_hollow_first_on() {
    let scratch = Scratch::new("solid");
    let disc = plinth_mesh::make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96).expect("Round 25");
    place(&scratch, &disc, "Round_25");
    let off = run(&scratch, "off", false, false);
    let on = run(&scratch, "on", false, true);
    let (off_bytes, off_source, off_hollow) = emitted(&off, "Round_25");
    let (on_bytes, on_source, on_hollow) = emitted(&on, "Round_25");
    assert!(
        matches!(on_hollow, plinth_hollow::HollowDisposition::Solid { .. }),
        "the fixture must not hollow for this test to say anything, got {on_hollow:?}"
    );
    assert_eq!(off_hollow, on_hollow);
    assert_eq!(off_source, Some(SweepPlanSource::Reused));
    assert_eq!(on_source, Some(SweepPlanSource::Reused), "the solid winner's plan is reused");
    assert!(off_bytes == on_bytes, "the same bytes with hollow first on and off");
}

/// A hollowed model's final placement reuses the winner's SHELL plan, and the
/// file is the file the seal makes when it plans the shell itself.
#[test]
#[ignore = "three dense searches of a drilled block; run by name at a milestone"]
fn a_hollowed_model_reuses_the_winners_shell_plan_and_keeps_the_seals_bytes() {
    let scratch = Scratch::new("shell");
    let block = plinth_mesh::make_box([18.0, 18.0, 18.0], [0.0, 0.0, 0.0]).expect("a block");
    place(&scratch, &block, "block");
    let reused = run(&scratch, "reuse", false, true);
    let planned = run(&scratch, "bypass", true, true);
    let off = run(&scratch, "off", false, false);
    let (reused_bytes, reused_source, hollow) = emitted(&reused, "block");
    let (planned_bytes, planned_source, _) = emitted(&planned, "block");
    let (_, off_source, _) = emitted(&off, "block");
    assert_eq!(
        hollow,
        plinth_hollow::HollowDisposition::HollowedAndDrilled,
        "the fixture must be a drilled shell for this test to say anything"
    );
    assert_eq!(
        reused_source,
        Some(SweepPlanSource::Reused),
        "hollow first: the seal emits the winner's shell plan"
    );
    assert_eq!(planned_source, Some(SweepPlanSource::Planned(ReuseMiss::Disabled)));
    assert_eq!(
        off_source,
        Some(SweepPlanSource::Planned(ReuseMiss::MeshDiffers)),
        "hollow first explicitly off (the solid-first search): the solid's plan cannot serve the shell"
    );
    assert!(
        reused_bytes == planned_bytes,
        "the reused shell plan is the plan the seal makes: same bytes ({} vs {})",
        reused_bytes.len(),
        planned_bytes.len()
    );
}
