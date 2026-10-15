//! **The winning pose's plan is planned once** (`r1-winner-reuse`, 2026-10-09),
//! through the whole `prepare()` door.
//!
//! The orientation search sweeps its three finalists at production fidelity and
//! used to throw the plans away; the seal then swept the winner again on the
//! same mesh. Now the plan the search kept is emitted, when its key (mesh bits,
//! layer bits, policy, no holes) matches. The bytes must not move: the emitted
//! `.chitubox` is the same file whether the plan was reused or planned again,
//! and `--no-cache` (`PrepareRequest::no_cache`) is the switch.
//!
//! Path hits are asserted, not assumed: the reused run must report
//! `SweepPlanSource::Reused` (a fixture whose winner is not a finalist would
//! pass the byte comparison with the cold path twice, and fails here), and the
//! bypass run must report `Planned(Disabled)`.
//!
//! Fixtures are procedural, written out as real binary `.stl` files, so this
//! runs in a fresh clone with no provisioned corpus.

use std::path::{Path, PathBuf};

use plinth_pipeline::prepare::{prepare_with_reuse, SweepPlanSource, WinnerPlanReuse};
use plinth_pipeline::{
    prepare, registered_decimation_budget, registered_support_request, ModelPreparationOutcome,
    OrientationPolicy, OutputProfile, PrepareReport, PrepareRequest,
};
use plinth_sweep::orient_demand::ReuseMiss;

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "plinth-winner-plan-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("in")).expect("scratch input folder");
        Self(path)
    }

    fn input(&self) -> PathBuf {
        self.0.join("in")
    }

    fn output(&self, run: &str) -> PathBuf {
        self.0.join(run)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Write a procedural mesh as the binary `.stl` a user would hand in, with its
/// coordinates snapped to f32 first because that is what an `.stl` holds.
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
    let snapped = plinth_mesh::Mesh::new(verts, mesh.tris().to_vec())
        .expect("an f32-snapped procedural fixture is still a valid mesh");
    plinth_mesh::write_binary_stl(scratch.input().join(format!("{stem}.stl")), &snapped)
        .expect("write the fixture");
}

/// The Round 25 profile `plate_contact.rs` measures against the real STL.
fn round_25() -> plinth_mesh::Mesh {
    plinth_mesh::make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96).expect("a valid Round 25 profile")
}

/// The request the CLI builds for
/// `plinth prepare --output saturn4-ultra --orient dense --placement sweep`,
/// with `--no-cache` as given.
fn request(scratch: &Scratch, run: &str, no_cache: bool) -> PrepareRequest {
    let printer = OutputProfile::Saturn4Ultra
        .resolve()
        .expect("M0 Saturn profile")
        .clone();
    PrepareRequest {
        input_models_dir: scratch.input(),
        output_dir: scratch.output(run),
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

/// The artifact's bytes and where its sweep plan came from.
fn emitted(report: &PrepareReport, stem: &str) -> (Vec<u8>, Option<SweepPlanSource>) {
    let entry = report
        .models
        .iter()
        .find(|entry| entry.model.as_str() == stem)
        .expect("the model is in the report");
    let ModelPreparationOutcome::Artifact {
        emit,
        sweep_plan_source,
        ..
    } = &entry.outcome
    else {
        panic!("{stem} must reach an artifact; it reported {:?}", entry.outcome);
    };
    let bytes = std::fs::read(Path::new(&emit.artifact_path)).expect("the artifact is on disk");
    (bytes, sweep_plan_source.0)
}

/// **Byte identity, with both paths proved to have run.** Same request twice,
/// `no_cache: false` then `true`: the first reuses the search's plan, the second
/// plans in the seal, and the two files are the same bytes.
#[test]
fn a_reused_plan_and_a_planned_one_emit_the_same_file() {
    let scratch = Scratch::new("round25");
    place(&scratch, &round_25(), "Round_25");

    let reused = prepare(request(&scratch, "reuse", false)).expect("the sweep route emits and warns");
    let planned = prepare(request(&scratch, "bypass", true)).expect("the sweep route emits and warns");
    let (reused_bytes, reused_source) = emitted(&reused, "Round_25");
    let (planned_bytes, planned_source) = emitted(&planned, "Round_25");

    // The path hits come first: equal bytes from two cold runs prove nothing.
    assert_eq!(
        reused_source,
        Some(SweepPlanSource::Reused),
        "the cache-on run must emit the plan the orientation search kept"
    );
    assert_eq!(
        planned_source,
        Some(SweepPlanSource::Planned(ReuseMiss::Disabled)),
        "--no-cache must plan in the seal"
    );
    assert!(
        reused_bytes == planned_bytes,
        "the emitted .chitubox must be the same bytes reused or planned ({} vs {} bytes)",
        reused_bytes.len(),
        planned_bytes.len(),
    );
}

/// **A shell the drill made cannot reuse** (its mesh is not the measured one),
/// and its file is the file it always was. The fixture is a solid large enough
/// that the hollow stage cuts and drills it; the test reads the disposition
/// first, so a fixture that stopped hollowing fails loudly instead of passing
/// on a solid.
///
/// Ignored in the gate: two sweeps of a hollowed 60 mm block take about 12
/// minutes. It passed when written (2026-10-09). The corpus control is the
/// Sphere and the Cylinder through `bench.sh`, which must report
/// `cache.winner_plan.hit` = 0 and the manifest digest.
#[test]
#[ignore = "two sweeps of a drilled 60 mm block, about 12 minutes; run by name at a milestone"]
fn a_drilled_shell_plans_for_itself_and_keeps_its_bytes() {
    let scratch = Scratch::new("shell");
    let block = plinth_mesh::make_box([60.0, 60.0, 60.0], [0.0, 0.0, 0.0]).expect("a block");
    place(&scratch, &block, "block");

    let reused = prepare(request(&scratch, "reuse", false)).expect("the sweep route emits and warns");
    let planned = prepare(request(&scratch, "bypass", true)).expect("the sweep route emits and warns");
    let entry = reused
        .models
        .iter()
        .find(|entry| entry.model.as_str() == "block")
        .expect("the block is in the report");
    let ModelPreparationOutcome::Artifact {
        hollow_disposition, ..
    } = &entry.outcome
    else {
        panic!("the block must reach an artifact; it reported {:?}", entry.outcome);
    };
    assert_eq!(
        *hollow_disposition,
        plinth_hollow::HollowDisposition::HollowedAndDrilled,
        "the fixture must be a drilled shell for this test to say anything"
    );
    let (reused_bytes, reused_source) = emitted(&reused, "block");
    let (planned_bytes, _) = emitted(&planned, "block");
    assert!(
        matches!(
            reused_source,
            Some(SweepPlanSource::Planned(
                ReuseMiss::MeshDiffers | ReuseMiss::HolesPresent
            ))
        ),
        "a drilled shell must fall back to its own sweep, got {reused_source:?}"
    );
    assert!(reused_bytes == planned_bytes, "the fallback keeps the file's bytes");
}

/// **Sabotage:** emit another finalist's plan than the winner's
/// (`WinnerPlanReuse::SabotageOtherPose`, key made to pass). The bytes must
/// move, so the byte comparison above can tell a wrong plan from a right one.
/// The sabotage needs a second finalist; a fixture with one fails at the first
/// assertion (the reuse path was not reached), not silently.
#[test]
fn sabotage_emitting_another_poses_plan_changes_the_file() {
    let scratch = Scratch::new("sabotage");
    place(&scratch, &round_25(), "Round_25");

    let honest = prepare(request(&scratch, "reuse", false)).expect("the sweep route emits and warns");
    let wrong = prepare_with_reuse(
        request(&scratch, "wrong", false),
        Some(WinnerPlanReuse::SabotageOtherPose),
    )
    .expect("the sweep route emits and warns");
    let (honest_bytes, _) = emitted(&honest, "Round_25");
    let (wrong_bytes, wrong_source) = emitted(&wrong, "Round_25");
    assert_eq!(wrong_source, Some(SweepPlanSource::Reused), "the sabotage must reach the reuse path");
    assert!(
        honest_bytes != wrong_bytes,
        "another pose's plan must change the file, or the byte comparison cannot tell a wrong plan"
    );
}
