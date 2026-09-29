//! Enforcement gate for `docs/specs/geometry-canon.md` §2 ("Enforcement",
//! census §2 fix 3): this crate must not grow a private copy of the geometry
//! primitives `geom_kernel::vec3` already publishes canonically.
//!
//! Consolidation census result for THIS crate (`docs/plans/geometry-consolidation.md`
//! OP-9, `feature/stage0-bytes-audit`): `plinth-decimate/src/lib.rs` (6,466
//! lines at time of sweep) contains none of the OP-1..OP-9 duplicated
//! operations. Its geometric distance work is fully delegated to
//! `geom_kernel::NativeSurface` / `PointCloudIndex` batch queries
//! (`probe_two_sided`, `backward_error_map`, `select_points`); it never
//! hand-rolls `dot`/`cross`/`norm`/`normalize`/a squared-length reduction.
//! Measured (ast-grep, four canon patterns, plus this grep) across both
//! `src/` and `tests/`: zero matches, zero exemptions needed. The allowlist
//! below is therefore empty by measurement, not by omission -- if a future
//! change adds a real site, name it here with a canon citation rather than
//! deleting this test.
//!
//! Per the canon's own rule: gate on a **count of files actually scanned**
//! being greater than zero, not on the grep's exit code -- an empty match
//! set and an empty file set both "succeed" silently otherwise (the xargs
//! incident this project keeps citing).

use std::fs;
use std::path::{Path, PathBuf};

/// `(file, line, reason)` for a private-vector-algebra site this crate is
/// permitted to keep. Empty at time of sweep -- see the module doc.
const ALLOWLIST: &[(&str, u32, &str)] = &[];

/// Forbidden signatures: a private redefinition of a canon `vec3` primitive,
/// or a bare FMA outside the one owner-exempted evidence site (which does not
/// live in this crate at all).
const FORBIDDEN_PATTERNS: &[&str] = &[
    "fn dot(",
    "fn cross(",
    "fn norm(",
    "fn normalize(",
    "fn sub(",
    "fn subtract(",
    ".mul_add(",
];

fn rust_files_under(directory: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            rust_files_under(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn crate_src_carries_no_private_vector_algebra() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files_under(&source_root, &mut files);

    // Gate on a counted row, never on an exit code: a matcher that finds no
    // files must FAIL this test, not pass it vacuously.
    assert!(
        !files.is_empty(),
        "no_private_vector_algebra scanned zero files under {}; the scan is broken, \
         not the crate clean",
        source_root.display()
    );

    let mut violations = Vec::new();
    for file in &files {
        let contents = fs::read_to_string(file)
            .unwrap_or_else(|error| panic!("read {}: {error}", file.display()));
        for (line_number, line) in contents.lines().enumerate() {
            let line_number = u32::try_from(line_number + 1).unwrap_or(u32::MAX);
            for pattern in FORBIDDEN_PATTERNS {
                if !line.contains(pattern) {
                    continue;
                }
                let relative = file
                    .strip_prefix(Path::new(env!("CARGO_MANIFEST_DIR")))
                    .unwrap_or(file);
                let relative_str = relative.to_string_lossy();
                let allowed = ALLOWLIST
                    .iter()
                    .any(|(allowed_file, allowed_line, _reason)| {
                        relative_str.ends_with(allowed_file) && *allowed_line == line_number
                    });
                if !allowed {
                    violations.push(format!(
                        "{relative_str}:{line_number}: `{pattern}` -- consolidate to \
                         geom_kernel::vec3 (docs/specs/geometry-canon.md §1 OP-6/OP-7/OP-9), \
                         or add it to ALLOWLIST here with a canon-section citation"
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "private vector-algebra sites found in plinth-decimate/src (scanned {} file(s)):\n{}",
        files.len(),
        violations.join("\n")
    );
}
