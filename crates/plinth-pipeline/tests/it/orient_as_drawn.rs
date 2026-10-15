//! **"Use this model as drawn"** through the real executor (GitHub issue #1,
//! owner spec 2026-10-01): `plinth_pipeline::prepare` over a folder, stopped
//! after the Orientation stage, one disposition per model.
//!
//! Each switch is exercised on its own model beside an unmarked one, so a
//! switch that leaks to a neighbour, or one that never fires, both redden:
//!
//! - `<stem>.asdrawn` marker (every platform);
//! - macOS Finder tag, on a `/bin/cp -p` copy of the owner's tagged
//!   `dnd-campaigns/cube.stl` (macOS only; the original is only read), both
//!   as a file in the input folder and as a SYMLINK into a library folder
//!   that holds the tagged file (the tag is read through the link);
//! - Linux `user.xdg.tags` (Linux only);
//! - `OrientationPolicy::AsDrawn` (`--orient none`): every model;
//! - a mark under a PINNED run: the marked model is used as drawn and is not
//!   refused for lacking a baseline; the unmarked, unpinned model still is;
//! - `--no-orient=<list>` (owner spec 2026-10-01): an exact name, a name
//!   without `.stl`, a glob, a name that matches nothing (reported, the run
//!   goes on), a listed model that ALSO has a marker, under the dense, pinned
//!   and `--orient none` policies.
//!
//! An unmarked model under the default policy is still SEARCHED, and says so.
//!
//! Fixtures: the committed `corpus/baseline/make_toed_slab.prepped.stl`
//! (searched in about a second) placed under several stems, so this file runs
//! in a fresh clone; the cube copy only on macOS.

use std::path::{Path, PathBuf};

use plinth_orient::as_drawn::{AsDrawnReason, NoOrientList};
use plinth_pipeline::{
    prepare, registered_decimation_budget, registered_support_request, ModelPreparationOutcome,
    OrientationPolicy, OrientationSource, OutputProfile, PipelineExitCode, PrepareReport,
    PrepareRequest, StageStop,
};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// A per-test scratch folder under cargo's per-target temporary directory
/// (a tmpfs `/tmp` may refuse `user.*` attributes on older Linux kernels).
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("plinth-orient-as-drawn-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("in")).expect("scratch input folder");
        Self(path)
    }

    fn input(&self) -> PathBuf {
        self.0.join("in")
    }

    /// The committed toed slab under `stem`. Written from its bytes, not
    /// copied, so the scratch file carries no extended attribute of the
    /// checkout's.
    fn place(&self, stem: &str) -> PathBuf {
        let bytes = std::fs::read(workspace_root().join("corpus/baseline/make_toed_slab.prepped.stl"))
            .expect("the committed toed slab");
        let path = self.input().join(format!("{stem}.stl"));
        std::fs::write(&path, bytes).expect("place fixture");
        path
    }

    fn mark(&self, stem: &str) {
        std::fs::write(self.input().join(format!("{stem}.asdrawn")), b"").expect("write marker");
    }

    fn request(&self, orientation: OrientationPolicy) -> PrepareRequest {
        let printer = OutputProfile::Saturn4Ultra
            .resolve()
            .expect("M0 Saturn profile")
            .clone();
        PrepareRequest {
            input_models_dir: self.input(),
            output_dir: self.0.join("out"),
            orientation,
            no_orient: NoOrientList::default(),
            placement: plinth_pipeline::Placement::Sweep,
            output_profile: OutputProfile::Saturn4Ultra,
            decimation_budget: registered_decimation_budget().expect("registered budget"),
            hollow: plinth_hollow::HollowRequest::with_registered_wall_default(printer.clone())
                .expect("registered wall"),
            support: registered_support_request(&printer).expect("registered lift"),
            no_cache: true,
            exact_delicacy: false,
            stop_after: Some(StageStop::Orient),
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `(stem, source, tilt deg, selection)` for every model that reached the
/// Orientation stage; a model that did not is `None` in the source slot.
fn sources(report: &PrepareReport) -> Vec<(String, Option<OrientationSource>, f64, String)> {
    report
        .models
        .iter()
        .map(|model| match &model.orientation {
            Some(orientation) => (
                model.model.as_str().to_owned(),
                Some(orientation.source.clone()),
                orientation.original_up_tilt_deg,
                orientation.selection.clone(),
            ),
            None => (model.model.as_str().to_owned(), None, f64::NAN, String::new()),
        })
        .collect()
}

fn as_drawn(reason: AsDrawnReason) -> Option<OrientationSource> {
    Some(OrientationSource::AsDrawn { reason })
}

/// Every model that reached orientation stopped there, as asked.
fn assert_all_stopped_at_orient(report: &PrepareReport) {
    for model in &report.models {
        assert!(
            matches!(
                model.outcome,
                ModelPreparationOutcome::Stopped {
                    after: StageStop::Orient
                }
            ),
            "{} did not stop after Orient: {:?}",
            model.model.as_str(),
            model.outcome
        );
    }
}

#[test]
fn each_mark_makes_only_its_own_model_as_drawn_and_the_unmarked_one_is_searched() {
    let scratch = Scratch::new("marks");
    scratch.place("marked");
    scratch.mark("marked");
    scratch.place("plain");
    // The tagged model sits in a "library" folder outside the input folder and
    // is also put in the input folder TWICE: once as a copy (`tagged`) and once
    // as a symlink to the library file (`linked`), the way a plate folder may be
    // built from links into the model library. The tag lives on the library
    // file only, so `linked` is marked only if the probe reads through the link.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let library = {
        let library = scratch.0.join("library");
        std::fs::create_dir_all(&library).expect("library folder");
        library.join("tagged-in-library.stl")
    };
    #[cfg(target_os = "macos")]
    {
        let original = workspace_root().join("../cube.stl");
        for destination in [scratch.input().join("tagged.stl"), library.clone()] {
            let status = std::process::Command::new("/bin/cp")
                .arg("-p")
                .arg(&original)
                .arg(&destination)
                .status()
                .expect("run /bin/cp -p");
            assert!(status.success(), "copy the owner's tagged cube");
        }
    }
    #[cfg(target_os = "linux")]
    {
        let tagged = scratch.place("tagged");
        xattr::set(&tagged, "user.xdg.tags", b"plinth-no-rotate").expect("set user.xdg.tags");
        std::fs::copy(&tagged, &library).expect("library copy");
        xattr::set(&library, "user.xdg.tags", b"plinth-no-rotate").expect("set user.xdg.tags");
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    std::os::unix::fs::symlink(&library, scratch.input().join("linked.stl"))
        .expect("symlink into the library");

    let report = prepare(scratch.request(OrientationPolicy::DenseSweep)).expect("the run runs");
    assert_all_stopped_at_orient(&report);
    let seen = sources(&report);
    println!("{seen:?}");

    let by_stem = |stem: &str| {
        seen.iter()
            .find(|(name, ..)| name == stem)
            .unwrap_or_else(|| panic!("no model {stem}: {seen:?}"))
            .clone()
    };
    let marked = by_stem("marked");
    assert_eq!(
        marked.1,
        as_drawn(AsDrawnReason::MarkerFile {
            marker: scratch.input().join("marked.asdrawn"),
        })
    );
    assert_eq!((marked.2.to_bits(), marked.3.as_str()), (0.0_f64.to_bits(), "identity"));
    assert_eq!(by_stem("plain").1, Some(OrientationSource::Searched));

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let tagged = by_stem("tagged");
        #[cfg(target_os = "macos")]
        let expected = AsDrawnReason::FinderTag;
        #[cfg(target_os = "linux")]
        let expected = AsDrawnReason::XdgTag;
        assert_eq!(tagged.1, as_drawn(expected.clone()));
        assert_eq!((tagged.2.to_bits(), tagged.3.as_str()), (0.0_f64.to_bits(), "identity"));
        let linked = by_stem("linked");
        assert_eq!(
            linked.1,
            as_drawn(expected),
            "a symlink to a tagged file must be used as drawn"
        );
        assert_eq!((linked.2.to_bits(), linked.3.as_str()), (0.0_f64.to_bits(), "identity"));
    }

    // Path-hit count: the marked models, and only they, took the as-drawn path;
    // the marker file was not enumerated as a model.
    let hits = seen
        .iter()
        .filter(|(_, source, ..)| matches!(source, Some(OrientationSource::AsDrawn { .. })))
        .count();
    let expected_hits = if cfg!(any(target_os = "macos", target_os = "linux")) { 3 } else { 1 };
    assert_eq!(hits, expected_hits, "{seen:?}");
    assert_eq!(seen.len(), expected_hits + 1, "{seen:?}");
}

#[test]
fn orient_none_makes_every_model_as_drawn() {
    let scratch = Scratch::new("none");
    scratch.place("first");
    scratch.place("second");
    let report = prepare(scratch.request(OrientationPolicy::AsDrawn)).expect("the run runs");
    assert_all_stopped_at_orient(&report);
    let seen = sources(&report);
    assert_eq!(seen.len(), 2);
    for (stem, source, tilt, selection) in &seen {
        assert_eq!(source, &as_drawn(AsDrawnReason::OrientNone), "{stem}");
        assert_eq!(
            (tilt.to_bits(), selection.as_str()),
            (0.0_f64.to_bits(), "identity"),
            "{stem}"
        );
    }
}

/// Owner rule: ANY switch makes the model as drawn, so a mark beats a pinned
/// run too, and a marked model needs no baseline. The unmarked model the map
/// does not name is still refused by name, as it always was.
#[test]
fn a_mark_beats_a_pinned_run_and_needs_no_baseline() {
    let scratch = Scratch::new("pinned");
    scratch.place("marked");
    scratch.mark("marked");
    scratch.place("unpinned");
    let report = prepare(scratch.request(OrientationPolicy::Pinned(Default::default())))
        .expect("the run runs");
    let marked = report
        .models
        .iter()
        .find(|model| model.model.as_str() == "marked")
        .expect("marked model");
    assert!(
        matches!(
            marked.outcome,
            ModelPreparationOutcome::Stopped {
                after: StageStop::Orient
            }
        ),
        "{:?}",
        marked.outcome
    );
    assert_eq!(
        marked.orientation.as_ref().map(|orientation| orientation.source.clone()),
        as_drawn(AsDrawnReason::MarkerFile {
            marker: scratch.input().join("marked.asdrawn"),
        })
    );
    let unpinned = report
        .models
        .iter()
        .find(|model| model.model.as_str() == "unpinned")
        .expect("unpinned model");
    assert!(
        matches!(
            &unpinned.outcome,
            ModelPreparationOutcome::Failed { exit_code: PipelineExitCode::Input, detail, .. }
                if detail.contains("no pinned orientation baseline")
        ),
        "{:?}",
        unpinned.outcome
    );
}

fn listed(entry: &str) -> Option<OrientationSource> {
    as_drawn(AsDrawnReason::Listed {
        entry: entry.to_owned(),
    })
}

/// `--no-orient=<list>` (owner spec 2026-10-01) through the real executor:
/// each kind of entry makes exactly the models it names as drawn, at
/// identity; a listed model that also has a marker is as drawn once, for the
/// list (the first reason that holds); a marker-only model still answers to
/// its marker; an unlisted, unmarked model is still searched; and the entries
/// that name nothing come back on the report while every model still runs.
#[test]
fn the_no_orient_list_makes_exactly_its_models_as_drawn_and_reports_names_that_match_nothing() {
    let scratch = Scratch::new("no-orient");
    for stem in [
        "exact",
        "bare",
        "glob_1",
        "glob_2",
        "both",
        "marked_only",
        "plain",
    ] {
        scratch.place(stem);
    }
    scratch.mark("both");
    scratch.mark("marked_only");
    let mut request = scratch.request(OrientationPolicy::DenseSweep);
    request.no_orient = NoOrientList::parse("exact.stl, bare, glob_?, both, nothing_here, *.obj");

    let report = prepare(request).expect("an unmatched entry never refuses the run");
    assert_all_stopped_at_orient(&report);
    let seen = sources(&report);
    println!("{seen:?}");
    let expected: [(&str, Option<OrientationSource>); 7] = [
        ("bare", listed("bare")),
        ("both", listed("both")),
        ("exact", listed("exact.stl")),
        ("glob_1", listed("glob_?")),
        ("glob_2", listed("glob_?")),
        (
            "marked_only",
            as_drawn(AsDrawnReason::MarkerFile {
                marker: scratch.input().join("marked_only.asdrawn"),
            }),
        ),
        ("plain", Some(OrientationSource::Searched)),
    ];
    assert_eq!(
        seen.iter()
            .map(|(stem, source, ..)| (stem.as_str(), source.clone()))
            .collect::<Vec<_>>(),
        expected.to_vec(),
    );
    for (stem, source, tilt, selection) in &seen {
        if matches!(source, Some(OrientationSource::AsDrawn { .. })) {
            assert_eq!(
                (tilt.to_bits(), selection.as_str()),
                (0.0_f64.to_bits(), "identity"),
                "{stem} is as drawn and must not be turned"
            );
        }
    }
    // The searched neighbour is the same slab bytes and IS turned, so identity
    // above is the as-drawn path, not the search agreeing by chance.
    let plain = seen.iter().find(|(stem, ..)| stem == "plain").expect("plain");
    assert_ne!(plain.3, "identity", "{plain:?}");

    // Path hits: 5 models through the list, 1 through its marker, 1 searched.
    let through_list = seen
        .iter()
        .filter(|(_, source, ..)| {
            matches!(
                source,
                Some(OrientationSource::AsDrawn {
                    reason: AsDrawnReason::Listed { .. }
                })
            )
        })
        .count();
    assert_eq!(through_list, 5, "{seen:?}");
    assert_eq!(report.no_orient_unmatched, ["nothing_here", "*.obj"]);
}

/// The list beats a pinned run (a listed model needs no baseline), and under
/// `--orient none` the run-wide switch is the reason recorded; in both, a name
/// that matches nothing is still reported.
#[test]
fn the_no_orient_list_under_a_pinned_run_and_under_orient_none() {
    let scratch = Scratch::new("no-orient-pinned");
    scratch.place("listed");
    scratch.place("unpinned");

    let mut pinned = scratch.request(OrientationPolicy::Pinned(Default::default()));
    pinned.no_orient = NoOrientList::parse("listed,typo");
    let report = prepare(pinned).expect("the run runs");
    let by_stem = |report: &PrepareReport, stem: &str| {
        report
            .models
            .iter()
            .find(|model| model.model.as_str() == stem)
            .unwrap_or_else(|| panic!("no model {stem}"))
            .clone()
    };
    let listed_model = by_stem(&report, "listed");
    assert!(
        matches!(
            listed_model.outcome,
            ModelPreparationOutcome::Stopped {
                after: StageStop::Orient
            }
        ),
        "{:?}",
        listed_model.outcome
    );
    assert_eq!(
        listed_model.orientation.map(|orientation| orientation.source),
        listed("listed")
    );
    assert!(
        matches!(
            &by_stem(&report, "unpinned").outcome,
            ModelPreparationOutcome::Failed { exit_code: PipelineExitCode::Input, detail, .. }
                if detail.contains("no pinned orientation baseline")
        ),
        "the unlisted, unpinned model is still refused by name"
    );
    assert_eq!(report.no_orient_unmatched, ["typo"]);

    let mut none = scratch.request(OrientationPolicy::AsDrawn);
    none.no_orient = NoOrientList::parse("listed,typo");
    let report = prepare(none).expect("the run runs");
    assert_all_stopped_at_orient(&report);
    for (stem, source, ..) in sources(&report) {
        assert_eq!(source, as_drawn(AsDrawnReason::OrientNone), "{stem}");
    }
    assert_eq!(report.no_orient_unmatched, ["typo"]);
}
