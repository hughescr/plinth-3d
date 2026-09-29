//! The corpus-scale instrument for the zero-volume repair, over local inputs.
//!
//! The census (2026-08-10) established that seven models are **clean at RAW** —
//! watertight, consistently oriented, no non-manifold edges, no duplicate faces
//! — and come out of *decimation* carrying manufactured duplicate faces: `Cat
//! (wild shape - Cat Lord)` gains 2 duplicate faces and 31 point-only-cut faces
//! going 82,564 -> 53,150; `zombie-9-b` gains 8 going 373,748 -> 51,974. That is
//! the defect `remove_zero_volume_face_pairs` exists to stop, and this is the
//! test that measures whether it did.
//!
//! **What it reads, and why that had to change.** The first version of this
//! harness read `TopologyCounts` alone — four numbers, none of which is a
//! cut-flag count — and so it was structurally blind to five of the six cut
//! classes. It passed while `Tavey Base.stl` emitted a *verified* 73,582 ->
//! 29,890 candidate carrying 10 faces flagged `TOPOLOGY_CUT_POINT_ONLY` from a
//! source clean in every class, and `validate_closed_mesh` refuses any nonzero
//! cut flag, so that model stayed pipeline-refused end to end with this harness
//! green. It now counts **every** cut class per stage, straight off
//! `FaceTopology::cut_flags()`, and fails on any class the decimation
//! manufactured — not duplicate faces alone.
//!
//! **Why it is `#[ignore]`d.** Its inputs are 4–20 MB binary STLs that are not
//! in git and never will be (see `docs/development.md`, "Corpus provisioning").
//! A checkout without them must not fail `cargo test --workspace`, and a
//! checkout *with* them must be able to reproduce the claim exactly. So the
//! harness is committed, always compiled, and run on demand:
//!
//! ```sh
//! cargo test -p plinth-decimate --test it local_minis_repair:: -- --ignored --nocapture
//! ```
//!
//! It reads the STLs where they lie and writes nothing but its own report into
//! the crate's target directory: the models are read-only verification
//! material, never modified and never copied into the repository.
//!
//! Point the harness at a different directory with `PLINTH_MINIS_DIR`; it
//! defaults to `minis-to-process` beside the workspace.

use std::fs;
use std::path::{Path, PathBuf};

use geom_kernel::NativeSurfaceConfig;
use plinth_decimate::{run, BudgetSearchOutcome, DecimationBudget, ProbeConfig};
use plinth_mesh::{
    build_face_topology, load_binary_stl, Mesh, TopologyCounts, WeldPolicy, TOPOLOGY_CUT_BOUNDARY,
    TOPOLOGY_CUT_DEGENERATE, TOPOLOGY_CUT_DUPLICATE, TOPOLOGY_CUT_NONMANIFOLD,
    TOPOLOGY_CUT_POINT_ONLY, TOPOLOGY_CUT_SAME_DIRECTION,
};

/// Every class `validate_closed_mesh` refuses, in flag order. A stage that
/// gains faces in any of them has manufactured a defect, whatever the four
/// `TopologyCounts` numbers say.
const CUT_CLASSES: [(&str, u8); 6] = [
    ("boundary", TOPOLOGY_CUT_BOUNDARY),
    ("nonmanifold", TOPOLOGY_CUT_NONMANIFOLD),
    ("same_direction", TOPOLOGY_CUT_SAME_DIRECTION),
    ("duplicate", TOPOLOGY_CUT_DUPLICATE),
    ("point_only", TOPOLOGY_CUT_POINT_ONLY),
    ("degenerate", TOPOLOGY_CUT_DEGENERATE),
];

/// The seven the census named clean-raw-but-damaged-by-decimation. These are
/// the models the claim is about, so these are the models the harness runs by
/// default; set `PLINTH_MINIS_ALL=1` to sweep every STL in the directory
/// instead.
const CENSUS_CLEAN_RAW: [&str; 7] = [
    "Cat (wild shape - Cat Lord).stl",
    "Morel base.stl",
    "Rotting Warhorse.stl",
    "Tavey Base.stl",
    "Thorny Vegepygmy.stl",
    "zombie-8-a.stl",
    "zombie-9-b.stl",
];

const REPORT_NAME: &str = "local-minis-zero-volume-repair.txt";

#[test]
#[ignore = "reads local, uncommitted minis-to-process STLs"]
fn local_minis_zero_volume_repair() {
    let directory = minis_directory();
    let models = models_to_run(&directory);
    assert!(
        !models.is_empty(),
        "LocalMinisMissing: no STL to run in {}. Point PLINTH_MINIS_DIR at the read-only \
         minis directory.",
        directory.display()
    );

    let mut report = String::from("plinth/local-minis-zero-volume-repair/v2\n");
    let mut manufactured = Vec::new();
    for path in &models {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("<unnamed>")
            .to_owned();
        eprintln!("local minis repair: running {name}");
        let observation = observe(path);
        report.push_str(&observation.line(&name));
        report.push('\n');
        eprintln!("  {}", observation.line(&name));
        // The refused ratios are the record of the gate doing its work, so the
        // artifact carries them in full even though the summary line counts
        // them: this is where "Tavey Base verifies at a *different* ratio" is
        // legible as a decision rather than as a changed number.
        for refusal in &observation.topology_refusals {
            report.push_str("\t\t");
            report.push_str(refusal);
            report.push('\n');
        }
        if !observation.manufactured_cut_classes().is_empty()
            || observation.refused_by_defect.is_some()
        {
            manufactured.push(observation.line(&name));
        }
    }

    let report_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(REPORT_NAME);
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).expect("create the harness report directory");
    }
    fs::write(&report_path, &report).expect("write the harness report");
    eprintln!("local minis repair report: {}", report_path.display());

    assert!(
        manufactured.is_empty(),
        "LocalMinisManufacturedCut: decimation must not add a cut face of ANY class to a model \
         that did not have one, and must not refuse a model over coincidence it manufactured:\n{}",
        manufactured.join("\n")
    );
}

/// One model's before/after, and the two ways the repair can fail it: emitting
/// coincidence, or refusing every ratio because of coincidence.
struct Observation {
    input_faces: usize,
    output_faces: usize,
    status: &'static str,
    before: Stage,
    after: Stage,
    /// `CoincidentFacesRemain` or `ZeroVolumePairOpensBoundary` reaching the
    /// operator means the repair met something it cannot handle; the census
    /// says it should meet neither on these models.
    refused_by_defect: Option<String>,
    /// Ratios the emitted-candidate certification refused because a canonical
    /// count rose. Unlike the two above these are not failures: they are the
    /// gate declining one ratio and the search stepping on, and a model that
    /// verifies with some of these verified at a ratio that manufactured
    /// nothing.
    topology_refusals: Vec<String>,
}

/// One stage's canonical topology: the four counts, and the face count of each
/// of the six cut classes. `TopologyCounts` covers three of the six on its own,
/// so the classes are read from `cut_flags()` directly and all six are reported
/// the same way — the report says what the gate downstream reads, with nothing
/// inferred.
struct Stage {
    counts: TopologyCounts,
    cut_faces: [usize; CUT_CLASSES.len()],
}

impl Observation {
    /// Every class whose face count rose across decimation, with both numbers.
    fn manufactured_cut_classes(&self) -> Vec<(&'static str, usize, usize)> {
        CUT_CLASSES
            .iter()
            .enumerate()
            .filter(|&(class, _)| self.after.cut_faces[class] > self.before.cut_faces[class])
            .map(|(class, &(name, _))| {
                (
                    name,
                    self.before.cut_faces[class],
                    self.after.cut_faces[class],
                )
            })
            .collect()
    }

    fn line(&self, name: &str) -> String {
        let mut line = format!(
            "{name}\tstatus={}\tfaces={}->{}",
            self.status, self.input_faces, self.output_faces
        );
        for (class, &(class_name, _)) in CUT_CLASSES.iter().enumerate() {
            line.push_str(&format!(
                "\tcut_{class_name}={}->{}",
                self.before.cut_faces[class], self.after.cut_faces[class]
            ));
        }
        line.push_str(&format!(
            "\tboundary_edges={}->{}\tnonmanifold_edges={}->{}\tduplicate_faces={}->{}\t\
             duplicate_groups={}->{}",
            self.before.counts.boundary_edges,
            self.after.counts.boundary_edges,
            self.before.counts.nonmanifold_edges,
            self.after.counts.nonmanifold_edges,
            self.before.counts.duplicate_faces,
            self.after.counts.duplicate_faces,
            self.before.counts.duplicate_face_groups,
            self.after.counts.duplicate_face_groups,
        ));
        let manufactured = self.manufactured_cut_classes();
        line.push_str(&format!(
            "\ttopology_refusals={}",
            self.topology_refusals.len()
        ));
        line.push_str(&format!(
            "\tmanufactured={}\tdefect_refusal={}",
            if manufactured.is_empty() {
                "none".to_owned()
            } else {
                manufactured
                    .iter()
                    .map(|(class, before, after)| format!("{class}:{before}->{after}"))
                    .collect::<Vec<_>>()
                    .join(",")
            },
            self.refused_by_defect.as_deref().unwrap_or("none"),
        ));
        line
    }
}

fn observe(path: &Path) -> Observation {
    let original = load_binary_stl(path, WeldPolicy::None).unwrap_or_else(|error| {
        panic!(
            "LocalMinisUnreadable: {} could not be loaded without repair: {error}",
            path.display()
        )
    });
    let before = stage(&original);
    let outcome = run(&original, harness_budget()).unwrap_or_else(|error| {
        panic!(
            "LocalMinisExecutionFailure: {} ended the run: {error}",
            path.display()
        )
    });
    let (status, mesh, refusals) = match &outcome {
        BudgetSearchOutcome::Verified(success) => (
            "verified",
            success.mesh().clone(),
            refusal_text(success.trace()),
        ),
        BudgetSearchOutcome::AlreadyPrintScale(success) => (
            "already_print_scale",
            success.mesh().clone(),
            refusal_text(success.trace()),
        ),
        BudgetSearchOutcome::SkippedScaleSuspicious { .. } => {
            ("skipped_scale_suspicious", original.clone(), Vec::new())
        }
    };
    let defect = refusals
        .iter()
        .find(|refusal| {
            refusal.contains("CoincidentFacesRemain") || refusal.contains("ZeroVolumePairOpens")
        })
        .cloned();
    let topology_refusals = refusals
        .iter()
        .filter(|refusal| refusal.starts_with("TopologyIncreased"))
        .cloned()
        .collect();
    Observation {
        input_faces: original.tris().len(),
        output_faces: mesh.tris().len(),
        status,
        before,
        after: stage(&mesh),
        refused_by_defect: defect,
        topology_refusals,
    }
}

fn refusal_text(trace: &plinth_decimate::BudgetSearchTrace) -> Vec<String> {
    trace
        .reduction_refusals
        .iter()
        .map(|reason| format!("{reason:?}"))
        .collect()
}

/// `WeldPolicy::ExactBits` is the policy `validate_closed_mesh` uses, so these
/// counts are the ones the pipeline gate reads.
///
/// One topology build, one scan over its flags, six classes scattered out of
/// that single pass — never one pass per class.
fn stage(mesh: &Mesh) -> Stage {
    let topology = build_face_topology(mesh, WeldPolicy::ExactBits)
        .expect("build the exact-bits face topology the pipeline gate reads");
    let mut cut_faces = [0_usize; CUT_CLASSES.len()];
    for flags in topology.cut_flags() {
        for (class, &(_, flag)) in CUT_CLASSES.iter().enumerate() {
            if flags & flag != 0 {
                cut_faces[class] += 1;
            }
        }
    }
    Stage {
        counts: topology.counts(),
        cut_faces,
    }
}

fn harness_budget() -> DecimationBudget {
    DecimationBudget::ratified_default(ProbeConfig {
        surface: NativeSurfaceConfig {
            grid_pitch_mm: 1.0,
            max_grid_bytes: 64 * 1024 * 1024,
        },
        nearest_upper_bound_mm: 10_000.0,
    })
    .expect("construct the ratified 19um M3 budget")
}

fn minis_directory() -> PathBuf {
    match std::env::var_os("PLINTH_MINIS_DIR") {
        Some(path) => PathBuf::from(path),
        None => workspace_root()
            .parent()
            .expect("the workspace must have a parent directory")
            .join("minis-to-process"),
    }
}

fn models_to_run(directory: &Path) -> Vec<PathBuf> {
    if std::env::var_os("PLINTH_MINIS_ALL").is_some() {
        let mut all = fs::read_dir(directory)
            .unwrap_or_else(|error| {
                panic!(
                    "LocalMinisDirectoryUnreadable: {}: {error}",
                    directory.display()
                )
            })
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("stl"))
            })
            .collect::<Vec<_>>();
        // Deterministic order: a directory walk is not.
        all.sort();
        return all;
    }
    CENSUS_CLEAN_RAW
        .iter()
        .map(|name| directory.join(name))
        .filter(|path| path.is_file())
        .collect()
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("plinth-decimate must remain in the workspace crates directory")
        .to_path_buf()
}
