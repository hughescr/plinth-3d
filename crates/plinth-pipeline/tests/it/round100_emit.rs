//! **Round_100 at its committed pose emits a file that passes the disk audit,
//! and nothing in it reaches below the build plate** (fix/round100-emit,
//! 2026-09-29).
//!
//! # The defect this pins
//!
//! From the head-join lane (`15e594d`) until this fix, `plinth prepare` wrote
//! NO file for Round_100: the post-write disk audit refused it ("the freshly
//! reopened ChituBox artifact failed its disk audit", exit 4) and deleted the
//! bytes. `plinth-evidence/.../field-2026-09-28/for-craig-v3/README.md`, "Read
//! this first" item 1. With the audit made to name its check
//! (`lanes/round100-emit.md`), the failure read: "check BuildVolume, record
//! 1656: minimum z above the plate -0.233532 mm". Record 1656 was a leaned
//! member, lander 536, that the head-join rule had re-landed on plate arm 40
//! AT LAYER 0 — on the plate itself. Its drawn end disc, normal to its leaned
//! axis, dipped 0.2335 mm under the plate.
//!
//! # The rule
//!
//! A join is on the host's solid, above the build plate: a member re-landed
//! on a support arm keeps its whole drawn solid at or above the plate. Its
//! base disc, normal to its axis, reaches `r·√(1 − a_z²)` under its base
//! point, so a join at height `z` over the plate is admissible only when
//! `z ≥ r·√(1 − a_z²)` (`plinth_sweep` `head_join`).
//!
//! # What is graded, and how
//!
//! 1. The production `prepare` (the CLI's own route: dense orientation off,
//!    the committed pose pinned) reaches an ARTIFACT.
//! 2. The emitter's own post-write audit found nothing
//!    (`EmitReport::audit_findings` is empty).
//! 3. The grader's own reading of the bytes: every vertex of every record's
//!    mesh, and every non-raft record's end points, stand at or above the
//!    plate the file itself implies (the model's lowest vertex minus the
//!    placement lift), in the file's own binary32.
//!
//! The fixture is the PROVISIONED `corpus/models/Round_100.stl` (gitignored,
//! as `corpus_budgets.rs` and `orientation_rederivation.rs` use it), pinned at
//! its committed pose: the dense search commits up-direction (0, 0, −1), the
//! flip, which the pin reproduces as 180° about X.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use plinth_pipeline::{
    prepare, registered_decimation_budget, registered_support_request, ModelPreparationOutcome,
    OrientationPolicy, OutputProfile, PrepareRequest,
};

const STEM: &str = "Round_100";
const FLIP_PIN: &str = r#"{"stages":[{"name":"hollow","status":"ok","report":{"oriented":true,"orientation":{"axis":[1,0,0],"deg":180}}}]}"#;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("plinth-round100-emit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("in")).expect("scratch input folder");
        std::fs::create_dir_all(path.join("pins")).expect("scratch pin folder");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/plinth-pipeline sits two levels under the repo root")
        .to_path_buf()
}

fn request(scratch: &Scratch) -> PrepareRequest {
    let printer = OutputProfile::Saturn4Ultra
        .resolve()
        .expect("M0 Saturn profile")
        .clone();
    PrepareRequest {
        input_models_dir: scratch.0.join("in"),
        output_dir: scratch.0.join("out"),
        orientation: OrientationPolicy::pinned_for(
            STEM,
            plinth_orient::PinnedBaseline {
                pipeline_report: scratch.0.join("pins").join(format!("{STEM}.pipeline.json")),
            },
        ),
        placement: plinth_pipeline::Placement::Sweep,
        output_profile: OutputProfile::Saturn4Ultra,
        decimation_budget: registered_decimation_budget().expect("registered budget"),
        hollow: plinth_hollow::HollowRequest::with_registered_wall_default(printer.clone())
            .expect("registered wall"),
        support: registered_support_request(&printer).expect("registered lift"),
        no_cache: true,
        stop_after: None,
    }
}

/// What the run produced: the model's outcome, and the file's bytes read
/// back when there is one.
struct Emitted {
    outcome: ModelPreparationOutcome,
    /// `(record index, lowest z above the plate, in mm)` of the lowest vertex
    /// or end point of each record that reaches below the plate.
    below_plate: Vec<(usize, f64)>,
    records: usize,
}

fn emitted() -> &'static Emitted {
    static EMITTED: OnceLock<Emitted> = OnceLock::new();
    EMITTED.get_or_init(|| {
        let source = repo_root()
            .join("corpus/models")
            .join(format!("{STEM}.stl"));
        assert!(
            source.is_file(),
            "RequiredCorpusModelMissing: {} is not provisioned (corpus/models is gitignored)",
            source.display()
        );
        let scratch = Scratch::new();
        std::fs::copy(&source, scratch.0.join("in").join(format!("{STEM}.stl")))
            .expect("stage Round_100");
        std::fs::write(
            scratch.0.join("pins").join(format!("{STEM}.pipeline.json")),
            FLIP_PIN,
        )
        .expect("write the flip pin");
        let report = prepare(request(&scratch)).expect("the sweep route emits and warns");
        let entry = report
            .models
            .iter()
            .find(|entry| entry.model.as_str() == STEM)
            .expect("Round_100 is in the report");
        let outcome = entry.outcome.clone();
        let mut below_plate = Vec::new();
        let mut records = 0;
        if let ModelPreparationOutcome::Artifact { emit, .. } = &outcome {
            let project =
                chitubox_io::Project::open(&emit.artifact_path).expect("the artifact must reopen");
            let placement = project.model(0).expect("the one model").placement;
            let model = project.model_triangles(0).expect("model triangles");
            let model_min = model
                .chunks_exact(3)
                .map(|point| point[2])
                .fold(f32::INFINITY, f32::min);
            // The plate the file implies: the placement lifts the model's
            // lowest point `z_lift` over the plate.
            let plate = f64::from(model_min) - f64::from(placement.z_lift);
            let supports = project.supports(0).expect("support records");
            records = supports.len();
            for (index, record) in supports.iter().enumerate() {
                let mut lowest = record
                    .mesh
                    .xyz
                    .chunks_exact(3)
                    .map(|point| f64::from(point[2]))
                    .fold(f64::INFINITY, f64::min);
                // A raft record's end points are in the native raft frame
                // (z = 0), not model heights; only its mesh is geometry.
                if record.segment_type != chitubox_io::SegmentType::PAD_OR_BRIDGE {
                    lowest = lowest
                        .min(f64::from(record.start[2]))
                        .min(f64::from(record.end[2]));
                }
                // One binary32 ulp at this magnitude is the file's own
                // resolution: a vertex written exactly on the plate reads back
                // within it.
                let ulp = f64::from(f32::EPSILON) * plate.abs().max(1.0);
                if lowest < plate - ulp {
                    below_plate.push((index, lowest - plate));
                }
            }
        }
        Emitted {
            outcome,
            below_plate,
            records,
        }
    })
}

#[test]
fn round_100_at_its_committed_pose_emits_a_file_that_passes_the_disk_audit() {
    let emitted = emitted();
    let ModelPreparationOutcome::Artifact { emit, .. } = &emitted.outcome else {
        panic!(
            "Round_100 must reach an artifact; it reported {:?}",
            emitted.outcome
        );
    };
    assert!(
        emitted.records > 1_000,
        "a real Round_100 field was graded: {} records",
        emitted.records
    );
    assert_eq!(
        emit.audit_findings,
        Vec::new(),
        "the emitter's own post-write audit must find nothing on Round_100"
    );
}

#[test]
fn no_record_of_round_100_reaches_below_the_build_plate() {
    let emitted = emitted();
    assert!(
        matches!(emitted.outcome, ModelPreparationOutcome::Artifact { .. }),
        "Round_100 must reach an artifact; it reported {:?}",
        emitted.outcome
    );
    assert!(
        emitted.below_plate.is_empty(),
        "records reaching below the plate, (record, mm above the plate): {:?}",
        emitted.below_plate
    );
}
