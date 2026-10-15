//! The repair pre-pass **as a pipeline stage**, not as an algorithm.
//!
//! `plinth-mesh/tests/repair.rs` proves the three actions and
//! `plinth-hollow/tests/repair_gate.rs` proves the rescue at the real gate.
//! Neither proves the thing an operator actually depends on: that
//! `prepare_one` calls the pre-pass at all, hands the *repaired* mesh to the
//! stages below it, and carries the evidence into the run report the CLI
//! prints and the caller serializes. A stage that is written, tested and never
//! wired in is indistinguishable from one that does not exist.
//!
//! Every fixture here is procedural and in-repo, written out as a real binary
//! STL, so this runs in a fresh clone with nothing provisioned.

use std::path::PathBuf;

use plinth_mesh::{make_box_default, make_duplicated_face_box, Mesh};
use plinth_pipeline::{
    prepare, registered_decimation_budget, registered_support_request, ModelPreparationOutcome,
    ModelPreparationReport, OrientationPolicy, OutputProfile, PrepareReport, PrepareRequest,
    StageStop,
};

/// A per-test scratch folder, removed on the way in and on the way out.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "plinth-repair-prepass-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("in")).expect("scratch input folder");
        Self(path)
    }

    fn input(&self) -> PathBuf {
        self.0.join("in")
    }

    fn output(&self) -> PathBuf {
        self.0.join("out")
    }

    fn write_mesh(&self, stem: &str, mesh: &Mesh) {
        plinth_mesh::write_binary_stl(self.input().join(format!("{stem}.stl")), mesh)
            .expect("write procedural fixture as binary STL");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The dense-sweep request every test here uses. Nothing is pinned: the
/// fixtures are procedural, so there is no baseline that belongs to them.
fn request(scratch: &Scratch, stop_after: Option<StageStop>) -> PrepareRequest {
    let printer = OutputProfile::Saturn4Ultra
        .resolve()
        .expect("M0 Saturn profile")
        .clone();
    PrepareRequest {
        input_models_dir: scratch.input(),
        output_dir: scratch.output(),
        orientation: OrientationPolicy::DenseSweep,
        // LANE 2: this suite pins the LEGACY engine. The sweep selector is a
        // runtime enum whose default is Legacy, named here so a future default
        // flip cannot silently re-point these fixtures at another engine.
        no_orient: Default::default(),
        placement: plinth_pipeline::Placement::Legacy,
        output_profile: OutputProfile::Saturn4Ultra,
        decimation_budget: registered_decimation_budget().expect("registered budget"),
        hollow: plinth_hollow::HollowRequest::with_registered_wall_default(printer.clone())
            .expect("registered wall"),
        support: registered_support_request(&printer).expect("registered lift"),
        no_cache: false,
        exact_delicacy: false,
        stop_after,
    }
}

fn model<'a>(report: &'a PrepareReport, stem: &str) -> &'a ModelPreparationReport {
    report
        .models
        .iter()
        .find(|entry| entry.model.as_str() == stem)
        .unwrap_or_else(|| panic!("model {stem} is missing from the report"))
}

/// **The wiring claim.** A model carrying a census defect goes into `prepare`
/// and comes out with that defect repaired, the repair recorded per class, and
/// the repaired geometry — not the source geometry — measured by the stage
/// below.
///
/// The duplicated-face box is the fixture because its defect is the one the
/// census found the *decimator* manufacturing, it is unambiguous at report
/// level — one group, one face removed — and the closed-solid gate refuses it
/// before the pre-pass runs.
#[test]
fn prepare_runs_the_pre_pass_and_puts_its_evidence_in_the_report() {
    let scratch = Scratch::new("wired");
    scratch.write_mesh(
        "duplicated",
        &make_duplicated_face_box().expect("duplicate fixture"),
    );

    let report = prepare(request(&scratch, Some(StageStop::Decimate))).expect("the run succeeds");
    let entry = model(&report, "duplicated");
    let repair = entry
        .repair
        .as_ref()
        .expect("the pre-pass ran, so the report says what it did");

    assert!(repair.changed(), "{}", repair.summary_line());
    assert_eq!(
        repair.duplicates.groups.len(),
        1,
        "one duplicate group: {}",
        repair.summary_line()
    );
    assert_eq!(repair.duplicates.faces_removed(), 1);
    assert_eq!(repair.source_faces, 13);
    assert_eq!(repair.repaired_faces, 12, "a plain box comes out");
    assert!(
        repair
            .residual
            .expect("a changed mesh is re-measured")
            .is_clean(),
        "{:?}",
        repair.residual
    );

    // The stage below saw the repaired mesh: decimation measured it and
    // reached the stop, which means it did not choke on the geometry the
    // pre-pass handed it.
    assert!(
        matches!(
            entry.outcome,
            ModelPreparationOutcome::Stopped {
                after: StageStop::Decimate
            }
        ),
        "{:?}",
        entry.outcome
    );
    assert!(
        entry.assumed_print_size.is_some(),
        "the repaired mesh was measured"
    );
}

/// The pre-pass's byte-identity contract, asserted at the *pipeline* boundary:
/// a clean model still gets a report row saying the pass looked and found
/// nothing, which is a different statement from `None` (never ran).
#[test]
fn a_clean_model_is_reported_as_looked_at_and_left_alone() {
    let scratch = Scratch::new("clean");
    scratch.write_mesh("clean", &make_box_default().expect("box"));

    let report = prepare(request(&scratch, Some(StageStop::Decimate))).expect("the run succeeds");
    let repair = model(&report, "clean")
        .repair
        .as_ref()
        .expect("the pre-pass ran on a clean model too");
    assert!(!repair.changed(), "{}", repair.summary_line());
    assert_eq!(repair.source_faces, repair.repaired_faces);
    assert_eq!(repair.source_vertices, repair.repaired_vertices);
    assert!(
        repair.residual.is_none(),
        "an untouched mesh is not re-measured"
    );
}

/// The evidence has to survive the trip the report actually takes: the run
/// report is serialized, and a `repair` field that dropped or renamed itself on
/// the way through would take the whole record of what was edited with it.
#[test]
fn the_repair_evidence_survives_the_report_serde_round_trip() {
    let scratch = Scratch::new("serde");
    scratch.write_mesh(
        "duplicated",
        &make_duplicated_face_box().expect("duplicate fixture"),
    );

    let report = prepare(request(&scratch, Some(StageStop::Decimate))).expect("the run succeeds");
    let json = serde_json::to_string(&report).expect("the report serializes");
    assert!(
        json.contains("\"repair\""),
        "the field is present in the wire form"
    );
    let restored: PrepareReport = serde_json::from_str(&json).expect("and deserializes");
    assert_eq!(restored, report);
    assert_eq!(
        model(&restored, "duplicated").repair,
        model(&report, "duplicated").repair
    );
}

/// **The pre-pass has no failure disposition, and that is the fix.**
///
/// It used to have one: a `PipelineError::Repair` carrying a `RepairError`,
/// which projected to the input exit code and failed the model at a stage added
/// to help it. Any internal failure — the topology, the triage, the constants
/// registry, the resource bound — became a hard refusal of the operator's file,
/// which is the exact inversion the 2026-08-10 ruling forbids. Now
/// `repair_mesh` returns a mesh: an input it cannot work on comes back
/// bit-identical with the reason in `actions_skipped`, and the model reaches
/// every stage below exactly as it did before the pre-pass existed.
///
/// The type system carries most of this claim — there is no `Result` to
/// mishandle — so what is left to assert is that the *disposition* is a
/// pass-through and not a refusal.
#[test]
fn the_pre_pass_has_no_failure_disposition_at_all() {
    let scratch = Scratch::new("no-failure");
    let mesh = make_duplicated_face_box().expect("duplicate fixture");
    scratch.write_mesh("duplicated", &mesh);

    let report = prepare(request(&scratch, Some(StageStop::Decimate))).expect("the run succeeds");
    let repair = model(&report, "duplicated")
        .repair
        .clone()
        .expect("the pre-pass ran");
    assert!(
        repair.actions_skipped.is_none(),
        "a fixture this small is nowhere near the resource bound: {repair:?}"
    );
    assert!(matches!(
        model(&report, "duplicated").outcome,
        plinth_pipeline::ModelPreparationOutcome::Stopped { .. }
    ));
}
