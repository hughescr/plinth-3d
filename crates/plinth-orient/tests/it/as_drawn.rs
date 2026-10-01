//! **"Use this model as drawn"** (GitHub issue #1, owner spec 2026-10-01):
//! each per-model switch on its own, and the identity orientation it selects.
//!
//! - the `<stem>.asdrawn` marker: every platform;
//! - the macOS Finder tag `plinth-no-rotate`: macOS only, on a COPY of the
//!   owner's real tagged file `dnd-campaigns/cube.stl` (copied with
//!   `/bin/cp -p`, which keeps extended attributes; the original is only read);
//! - the Linux `user.xdg.tags` entry: Linux only;
//! - an untagged, unmarked STL is NOT marked.
//!
//! `--orient none` is the caller's decision (`plinth-pipeline`), graded there
//! and in `plinth-cli`'s process tests.
//!
//! Scratch files live under cargo's per-target temporary directory, not the
//! system one: a tmpfs `/tmp` may refuse `user.*` attributes on older Linux
//! kernels, which would make the xdg test fail for a reason that is not the
//! code's.

use std::path::{Path, PathBuf};

use plinth_decimate::{run as run_decimation, DecimationBudget, ProbeConfig};
use plinth_orient::as_drawn::{
    marker_path, probe_source, strip_finder_colour, xdg_tags, AsDrawnReason, AS_DRAWN_TAG,
};
use plinth_orient::{orient_as_drawn, BaselineSelection, OrientationProvenance};

/// A per-test scratch folder, removed on the way in and on the way out.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("plinth-as-drawn-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch folder");
        Self(path)
    }

    /// A fresh STL-named file with NO extended attributes (written, not
    /// copied: a macOS copy carries the source's attributes with it).
    fn untagged(&self, stem: &str) -> PathBuf {
        let path = self.0.join(format!("{stem}.stl"));
        std::fs::write(&path, b"not read by the probe").expect("write scratch file");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_marker_file_beside_the_stl_marks_that_model_and_no_other() {
    let scratch = Scratch::new("marker");
    let marked = scratch.untagged("Round_25");
    let neighbour = scratch.untagged("Round_50");
    std::fs::write(scratch.0.join("Round_25.asdrawn"), b"").expect("write the marker");

    let probe = probe_source(&marked);
    assert_eq!(
        probe.reason,
        Some(AsDrawnReason::MarkerFile {
            marker: scratch.0.join("Round_25.asdrawn"),
        }),
        "a <stem>.asdrawn beside <stem>.stl must mark it"
    );
    assert!(probe.warnings.is_empty(), "{:?}", probe.warnings);
    assert_eq!(probe_source(&neighbour).reason, None, "the marker is per stem");

    // A stem with spaces and dots keeps all of it: `Weeping Treant v1.2.stl`
    // is marked by `Weeping Treant v1.2.asdrawn`.
    let dotted = scratch.untagged("Weeping Treant v1.2");
    assert_eq!(
        marker_path(&dotted),
        scratch.0.join("Weeping Treant v1.2.asdrawn")
    );
    // A marker that is a DIRECTORY is not a marker file.
    std::fs::create_dir(scratch.0.join("Weeping Treant v1.2.asdrawn")).expect("mkdir");
    assert_eq!(probe_source(&dotted).reason, None);
}

#[test]
fn an_untagged_unmarked_stl_is_not_marked() {
    let scratch = Scratch::new("untagged");
    let probe = probe_source(&scratch.untagged("plain"));
    assert_eq!(probe.reason, None);
    assert!(probe.warnings.is_empty(), "{:?}", probe.warnings);
}

#[test]
fn a_finder_colour_suffix_is_stripped_and_nothing_else() {
    assert_eq!(strip_finder_colour("plinth-no-rotate\n3"), AS_DRAWN_TAG);
    assert_eq!(strip_finder_colour("plinth-no-rotate\n0"), AS_DRAWN_TAG);
    assert_eq!(strip_finder_colour("plinth-no-rotate"), AS_DRAWN_TAG);
    // Not a colour: left as written, so it does not match the tag.
    assert_eq!(strip_finder_colour("plinth-no-rotate\nx"), "plinth-no-rotate\nx");
    assert_eq!(strip_finder_colour("plinth-no-rotate\n"), "plinth-no-rotate\n");
}

#[test]
fn xdg_tags_are_comma_separated_and_trimmed() {
    assert_eq!(
        xdg_tags(b"red, plinth-no-rotate ,,blue").expect("UTF-8"),
        vec!["red", "plinth-no-rotate", "blue"]
    );
    assert_eq!(xdg_tags(b"").expect("UTF-8"), Vec::<String>::new());
    assert!(xdg_tags(&[0xff, 0xfe]).is_err());
}

/// The owner's real tagged file, copied with its attributes.
#[cfg(target_os = "macos")]
fn copy_of_the_owners_tagged_cube(scratch: &Scratch) -> PathBuf {
    let original = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../cube.stl");
    assert!(
        original.is_file(),
        "the owner's tagged example {} is missing",
        original.display()
    );
    let copy = scratch.0.join("cube.stl");
    let status = std::process::Command::new("/bin/cp")
        .arg("-p")
        .arg(&original)
        .arg(&copy)
        .status()
        .expect("run /bin/cp -p");
    assert!(status.success(), "/bin/cp -p failed");
    copy
}

#[cfg(target_os = "macos")]
#[test]
fn the_finder_tag_on_the_owners_real_tagged_file_marks_it_as_drawn() {
    use plinth_orient::as_drawn::{finder_tags, FINDER_TAGS_XATTR};

    let scratch = Scratch::new("finder");
    let copy = copy_of_the_owners_tagged_cube(&scratch);
    // Precondition, so the test cannot pass on a copy that lost the tag.
    let raw = xattr::get(&copy, FINDER_TAGS_XATTR)
        .expect("read the Finder tag attribute")
        .expect("the copy must still carry its Finder tags");
    assert_eq!(
        finder_tags(&raw).expect("a binary plist array of strings"),
        vec![AS_DRAWN_TAG.to_owned()],
        "the owner's file carries plinth-no-rotate with a colour suffix (\\n3)"
    );

    let probe = probe_source(&copy);
    assert_eq!(probe.reason, Some(AsDrawnReason::FinderTag));
    assert!(probe.warnings.is_empty(), "{:?}", probe.warnings);
}

#[cfg(target_os = "macos")]
#[test]
fn other_finder_tags_do_not_mark_and_a_broken_tag_attribute_warns() {
    use plinth_orient::as_drawn::FINDER_TAGS_XATTR;

    let scratch = Scratch::new("finder-other");
    let path = scratch.untagged("other");
    let mut bytes = Vec::new();
    plist::Value::Array(vec![
        plist::Value::String("Red\n6".to_owned()),
        plist::Value::String("plinth-no-rotate-later".to_owned()),
    ])
    .to_writer_binary(&mut bytes)
    .expect("encode a binary plist");
    xattr::set(&path, FINDER_TAGS_XATTR, &bytes).expect("tag the scratch file");
    let probe = probe_source(&path);
    assert_eq!(probe.reason, None, "only the exact tag marks");
    assert!(probe.warnings.is_empty(), "{:?}", probe.warnings);

    // Emit and warn: an attribute that is not a plist is no tag, and says so.
    xattr::set(&path, FINDER_TAGS_XATTR, b"not a plist").expect("write junk");
    let probe = probe_source(&path);
    assert_eq!(probe.reason, None);
    assert_eq!(probe.warnings.len(), 1, "{:?}", probe.warnings);
    assert!(
        probe.warnings[0].contains(FINDER_TAGS_XATTR),
        "{:?}",
        probe.warnings
    );
}

#[cfg(target_os = "linux")]
#[test]
fn the_xdg_tag_marks_it_as_drawn_and_other_tags_do_not() {
    use plinth_orient::as_drawn::XDG_TAGS_XATTR;

    let scratch = Scratch::new("xdg");
    let tagged = scratch.untagged("tagged");
    xattr::set(&tagged, XDG_TAGS_XATTR, b"red,plinth-no-rotate").expect("set user.xdg.tags");
    let probe = probe_source(&tagged);
    assert_eq!(probe.reason, Some(AsDrawnReason::XdgTag));
    assert!(probe.warnings.is_empty(), "{:?}", probe.warnings);

    let other = scratch.untagged("other");
    xattr::set(&other, XDG_TAGS_XATTR, b"plinth-no-rotate-later").expect("set user.xdg.tags");
    assert_eq!(probe_source(&other).reason, None);
}

/// The as-drawn orientation is identity, re-centred like every pose, with the
/// operator's reason in its provenance.
#[test]
fn orient_as_drawn_applies_identity_recentres_and_records_the_reason() {
    // A box standing on a corner of the authored frame, so re-centring is
    // visible and any rotation would move its extents.
    let mesh = plinth_mesh::make_box([10.0, 20.0, 30.0], [3.0, -4.0, 5.0]).expect("box");
    let budget =
        DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
            .expect("registered fixture budget");
    let decimated = run_decimation(&mesh, budget)
        .expect("decimation runs")
        .into_decimated_model()
        .expect("print scale");
    let reason = AsDrawnReason::MarkerFile {
        marker: PathBuf::from("box.asdrawn"),
    };
    let oriented = orient_as_drawn(decimated, reason.clone()).expect("as drawn");
    let report = oriented.history().orientation();
    assert_eq!(report.selection, BaselineSelection::Identity);
    assert_eq!(report.provenance, OrientationProvenance::AsDrawn { reason });
    // Exactly the identity, compared bit for bit.
    for (row, expected) in report.transform.rotation.iter().zip([
        [1.0_f64, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ]) {
        assert_eq!(
            row.map(|value| value.get().to_bits()),
            expected.map(f64::to_bits)
        );
    }

    let (mut min, mut max) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for vertex in oriented.mesh().verts() {
        for axis in 0..3 {
            min[axis] = min[axis].min(vertex[axis]);
            max[axis] = max[axis].max(vertex[axis]);
        }
    }
    let extent = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    assert!(
        (extent[0] - 10.0).abs() < 1e-9 && (extent[1] - 20.0).abs() < 1e-9 && (extent[2] - 30.0).abs() < 1e-9,
        "the box was turned: extents {extent:?}"
    );
    assert!(min[2].abs() < 1e-9, "rests at z = 0, got {}", min[2]);
    assert!((min[0] + max[0]).abs() < 1e-9 && (min[1] + max[1]).abs() < 1e-9);
}
