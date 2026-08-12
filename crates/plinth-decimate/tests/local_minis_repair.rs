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
//! **Why it is `#[ignore]`d.** Its inputs are 4–20 MB binary STLs that are not
//! in git and never will be (see `docs/development.md`, "Corpus provisioning").
//! A checkout without them must not fail `cargo test --workspace`, and a
//! checkout *with* them must be able to reproduce the claim exactly. So the
//! harness is committed, always compiled, and run on demand:
//!
//! ```sh
//! cargo test -p plinth-decimate --test local_minis_repair -- --ignored --nocapture
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
use plinth_mesh::{build_face_topology, load_binary_stl, Mesh, TopologyCounts, WeldPolicy};

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

    let mut report = String::from("plinth/local-minis-zero-volume-repair/v1\n");
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
        if observation.manufactured_duplicate_faces() > 0 || observation.refused_by_defect.is_some()
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
        "LocalMinisManufacturedCoincidence: decimation must not add duplicate faces to a model \
         that did not have them, and must not refuse a model over coincidence it manufactured:\n{}",
        manufactured.join("\n")
    );
}

/// One model's before/after, and the two ways the repair can fail it: emitting
/// coincidence, or refusing every ratio because of coincidence.
struct Observation {
    input_faces: usize,
    output_faces: usize,
    status: &'static str,
    before: TopologyCounts,
    after: TopologyCounts,
    /// `CoincidentFacesRemain` or `ZeroVolumePairOpensBoundary` reaching the
    /// operator means the repair met something it cannot handle; the census
    /// says it should meet neither on these models.
    refused_by_defect: Option<String>,
}

impl Observation {
    fn manufactured_duplicate_faces(&self) -> usize {
        self.after
            .duplicate_faces
            .saturating_sub(self.before.duplicate_faces)
    }

    fn line(&self, name: &str) -> String {
        format!(
            "{name}\tstatus={}\tfaces={}->{}\tduplicate_faces={}->{}\tduplicate_groups={}->{}\t\
             boundary={}->{}\tnonmanifold={}->{}\tmanufactured={}\tdefect_refusal={}",
            self.status,
            self.input_faces,
            self.output_faces,
            self.before.duplicate_faces,
            self.after.duplicate_faces,
            self.before.duplicate_face_groups,
            self.after.duplicate_face_groups,
            self.before.boundary_edges,
            self.after.boundary_edges,
            self.before.nonmanifold_edges,
            self.after.nonmanifold_edges,
            self.manufactured_duplicate_faces(),
            self.refused_by_defect.as_deref().unwrap_or("none"),
        )
    }
}

fn observe(path: &Path) -> Observation {
    let original = load_binary_stl(path, WeldPolicy::None).unwrap_or_else(|error| {
        panic!(
            "LocalMinisUnreadable: {} could not be loaded without repair: {error}",
            path.display()
        )
    });
    let before = counts(&original);
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
    Observation {
        input_faces: original.tris().len(),
        output_faces: mesh.tris().len(),
        status,
        before,
        after: counts(&mesh),
        refused_by_defect: defect,
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
fn counts(mesh: &Mesh) -> TopologyCounts {
    build_face_topology(mesh, WeldPolicy::ExactBits)
        .expect("build the exact-bits face topology the pipeline gate reads")
        .counts()
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
