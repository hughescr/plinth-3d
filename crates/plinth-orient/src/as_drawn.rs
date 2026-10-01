//! **"Use this model exactly as drawn"**: the operator's per-model and per-run
//! switches that skip the orientation search (GitHub issue #1,
//! `hughescr/plinth-3d#1`, owner spec 2026-10-01).
//!
//! A model is used AS DRAWN — identity rotation, then the same re-centring and
//! the same lift off the plate as every other model — when ANY of these holds:
//!
//! 1. the run was started with `--orient none` (every model in the run;
//!    [`AsDrawnReason::OrientNone`], decided by the caller, not here);
//! 2. macOS: the STL file carries the Finder tag [`AS_DRAWN_TAG`], read from the
//!    file's own `com.apple.metadata:_kMDItemUserTags` extended attribute (a
//!    binary plist array of strings, each optionally suffixed `"\n<colour>"`),
//!    never through Spotlight, whose index can lag the file or be switched off.
//!    A symlinked STL is read through the link (the target's tag counts; so
//!    does a tag on the link itself);
//! 3. Linux: the extended attribute `user.xdg.tags` (comma-separated) contains
//!    [`AS_DRAWN_TAG`];
//! 4. any platform: a marker file `<stem>.asdrawn` sits next to `<stem>.stl`.
//!    This is the PORTABLE form: git, zip and most copy tools drop extended
//!    attributes, so a repository fixture is marked this way.
//!
//! **This is an operator decision, not an inference.** The crate's rule that
//! "identity is chosen only by winning" is about the SEARCH: the search never
//! falls back to identity when it cannot measure. Here nobody searched; the
//! operator said how the part is to stand, and the provenance records who said
//! so and how ([`crate::OrientationProvenance::AsDrawn`]).
//!
//! **Emit and warn, never refuse.** A tag attribute that cannot be read or
//! decoded does not stop the model: it is treated as "no tag", and the reason
//! is returned in [`AsDrawnProbe::warnings`] for the run report to print. On a
//! platform or a volume without extended attributes the probe finds no tag and
//! says nothing; the marker file still works there.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The tag that marks a model as drawn: a Finder tag on macOS, an entry of
/// `user.xdg.tags` on Linux. Compared exactly, case and all, after a Finder
/// colour suffix is removed.
pub const AS_DRAWN_TAG: &str = "plinth-no-rotate";

/// The extension of the portable marker: `<stem>.asdrawn` beside `<stem>.stl`.
/// The marker's contents are not read; its presence is the whole statement.
pub const AS_DRAWN_MARKER_EXTENSION: &str = "asdrawn";

/// The extended attribute macOS Finder keeps a file's tags in.
pub const FINDER_TAGS_XATTR: &str = "com.apple.metadata:_kMDItemUserTags";

/// The extended attribute the freedesktop.org convention keeps a file's tags
/// in (comma-separated UTF-8), as written by Dolphin, Nautilus extensions and
/// `setfattr -n user.xdg.tags -v a,b <file>`.
pub const XDG_TAGS_XATTR: &str = "user.xdg.tags";

/// Why a model was used as drawn. One reason is recorded: the first that holds
/// in the order the module doc lists them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AsDrawnReason {
    /// The run was started with `--orient none`.
    OrientNone,
    /// macOS Finder tag [`AS_DRAWN_TAG`] on the STL.
    FinderTag,
    /// Linux `user.xdg.tags` entry [`AS_DRAWN_TAG`] on the STL.
    XdgTag,
    /// The marker file `<stem>.asdrawn` beside the STL.
    MarkerFile { marker: PathBuf },
}

impl fmt::Display for AsDrawnReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OrientNone => formatter.write_str("--orient none"),
            Self::FinderTag => write!(formatter, "Finder tag {AS_DRAWN_TAG}"),
            Self::XdgTag => write!(formatter, "{XDG_TAGS_XATTR} {AS_DRAWN_TAG}"),
            Self::MarkerFile { marker } => write!(
                formatter,
                "marker file {}",
                marker
                    .file_name()
                    .map_or_else(|| marker.display().to_string(), |name| name.to_string_lossy().into_owned())
            ),
        }
    }
}

/// What [`probe_source`] found for one STL.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AsDrawnProbe {
    /// `Some` when the model is to be used as drawn.
    pub reason: Option<AsDrawnReason>,
    /// A tag attribute that was present but could not be read or decoded, one
    /// line each. The model is then treated as untagged; these lines say so.
    pub warnings: Vec<String>,
}

/// Look for the per-model "as drawn" marks on one source STL: the platform's
/// tag attribute first, then the portable marker file.
///
/// A symlinked source is probed through the link: the tag on the file it
/// points at counts, and so does a tag on the link itself; the marker is
/// looked for beside the link (the name the operator put in the folder).
///
/// Reads one extended attribute (two for a symlink whose target carries none)
/// and checks one sibling path; it never opens the STL itself.
#[must_use]
pub fn probe_source(source: &Path) -> AsDrawnProbe {
    let mut warnings = Vec::new();
    let reason = platform_tag(source, &mut warnings).or_else(|| {
        let marker = marker_path(source);
        // `is_file` follows a symlink, as the executor's own enumeration does;
        // a directory of that name is not a marker.
        marker
            .is_file()
            .then_some(AsDrawnReason::MarkerFile { marker })
    });
    AsDrawnProbe { reason, warnings }
}

/// The platform's tag attribute, if it holds [`AS_DRAWN_TAG`].
#[cfg(target_os = "macos")]
fn platform_tag(source: &Path, warnings: &mut Vec<String>) -> Option<AsDrawnReason> {
    let value = read_attribute(source, FINDER_TAGS_XATTR, warnings)?;
    match finder_tags(&value) {
        Ok(tags) => tags
            .iter()
            .any(|tag| tag == AS_DRAWN_TAG)
            .then_some(AsDrawnReason::FinderTag),
        Err(problem) => {
            warnings.push(format!(
                "{}: {FINDER_TAGS_XATTR} could not be decoded ({problem}); read as untagged",
                source.display()
            ));
            None
        }
    }
}

/// The platform's tag attribute, if it holds [`AS_DRAWN_TAG`].
#[cfg(target_os = "linux")]
fn platform_tag(source: &Path, warnings: &mut Vec<String>) -> Option<AsDrawnReason> {
    let value = read_attribute(source, XDG_TAGS_XATTR, warnings)?;
    match xdg_tags(&value) {
        Ok(tags) => tags
            .iter()
            .any(|tag| tag == AS_DRAWN_TAG)
            .then_some(AsDrawnReason::XdgTag),
        Err(problem) => {
            warnings.push(format!(
                "{}: {XDG_TAGS_XATTR} is not UTF-8 ({problem}); read as untagged",
                source.display()
            ));
            None
        }
    }
}

/// No tag convention is read on any other platform; the marker file still is.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn platform_tag(_source: &Path, _warnings: &mut Vec<String>) -> Option<AsDrawnReason> {
    None
}

/// One extended attribute's bytes, read THROUGH a symlink: the executor takes
/// symlinked STLs on purpose (the corpus is provisioned as links, and a plate
/// folder may be built from links into the model library), so the tag that
/// counts is the one on the file the link points at. If that file has no such
/// attribute and the source is itself a symlink, the link's own attribute is
/// read too: Finder can tag a link, and that is the operator marking it.
/// (Linux keeps no `user.*` attribute on a symlink; that second read finds
/// nothing there.)
///
/// An absent attribute, and a file system or platform that has no extended
/// attributes at all, are both "no tag" and silent; any other read failure is
/// "no tag" with a warning.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn read_attribute(source: &Path, name: &str, warnings: &mut Vec<String>) -> Option<Vec<u8>> {
    read_one_attribute(source, name, Follow::Target, warnings).or_else(|| {
        let is_link = source
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_symlink());
        is_link
            .then(|| read_one_attribute(source, name, Follow::LinkItself, warnings))
            .flatten()
    })
}

/// Which file a [`read_one_attribute`] call reads at a symlink.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[derive(Clone, Copy)]
enum Follow {
    /// The file the link points at (`getxattr`).
    Target,
    /// The link itself (`lgetxattr` / `XATTR_NOFOLLOW`).
    LinkItself,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn read_one_attribute(
    source: &Path,
    name: &str,
    follow: Follow,
    warnings: &mut Vec<String>,
) -> Option<Vec<u8>> {
    let read = match follow {
        Follow::Target => xattr::get_deref(source, name),
        Follow::LinkItself => xattr::get(source, name),
    };
    match read {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::Unsupported => None,
        Err(error) => {
            warnings.push(format!(
                "{}: {name} could not be read ({error}); read as untagged",
                source.display()
            ));
            None
        }
    }
}

/// The marker path for one source: `<dir>/<stem>.asdrawn`.
#[must_use]
pub fn marker_path(source: &Path) -> PathBuf {
    source.with_extension(AS_DRAWN_MARKER_EXTENSION)
}

/// One Finder tag with its colour suffix removed: Finder stores a coloured tag
/// as `"<name>\n<colour-number>"`. Only an all-digit suffix after the LAST
/// newline is a colour; anything else is left as written.
#[must_use]
pub fn strip_finder_colour(tag: &str) -> &str {
    match tag.rsplit_once('\n') {
        Some((name, colour))
            if !colour.is_empty() && colour.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            name
        }
        _ => tag,
    }
}

/// The entries of a `user.xdg.tags` value: comma-separated, each trimmed of
/// surrounding whitespace, empty entries dropped.
///
/// # Errors
///
/// The value is not UTF-8.
pub fn xdg_tags(value: &[u8]) -> Result<Vec<String>, std::str::Utf8Error> {
    Ok(std::str::from_utf8(value)?
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect())
}

/// The tag names in a `com.apple.metadata:_kMDItemUserTags` value (a binary
/// plist array of strings), colour suffixes removed.
///
/// # Errors
///
/// The value is not a plist, or not an array, or holds a non-string entry.
#[cfg(target_os = "macos")]
pub fn finder_tags(value: &[u8]) -> Result<Vec<String>, String> {
    let decoded = plist::Value::from_reader(std::io::Cursor::new(value))
        .map_err(|error| format!("not a property list: {error}"))?;
    let entries = decoded
        .as_array()
        .ok_or_else(|| "the property list is not an array".to_owned())?;
    entries
        .iter()
        .map(|entry| {
            entry
                .as_string()
                .map(|tag| strip_finder_colour(tag).to_owned())
                .ok_or_else(|| "an entry of the tag array is not a string".to_owned())
        })
        .collect()
}
