//! Enforcement gate for `docs/specs/geometry-canon.md` §2: this crate must never
//! grow a private 3-vector/2D-vector algebra implementation that duplicates
//! `geom_kernel::vec3` / `geom_kernel::poly2`.
//!
//! `plinth-core` was swept for the geometry-consolidation census
//! (`docs/plans/geometry-consolidation.md`) and carries **zero** sites: no
//! `dot`/`cross`/`norm`/`normalize`/`sub`/`subtract` definitions and no
//! `.mul_add(` fused-multiply-add call anywhere in `src/`. This test pins that
//! fact so a future addition fails the build instead of silently reintroducing
//! OP-6/OP-7/OP-9-shaped duplication.
//!
//! The allowlist below is intentionally empty. If a legitimate need for one of
//! these names ever arises in this crate, add a `// GEOM-EXEMPT(reason, ...)`
//! marker per the canon's triage rule (§2) and list the file+line here.
//!
//! Gated on a scanned-file count, per the project's xargs-incident rule
//! (`docs/plans/geometry-consolidation.md`, "Gate on counts, never on exit
//! codes"): a matcher that finds no files must fail, not silently pass.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// (file path relative to `crates/plinth-core/src`, exact allowed pattern).
/// Empty by design -- see module doc comment.
const ALLOWLIST: &[(&str, &str)] = &[];

const FORBIDDEN_PATTERNS: &[&str] = &[
    "fn dot(",
    "fn cross(",
    "fn norm(",
    "fn normalize(",
    "fn sub(",
    "fn subtract(",
    ".mul_add(",
];

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read directory {dir:?}: {error}"));
    for entry in entries {
        let entry =
            entry.unwrap_or_else(|error| panic!("failed to read entry in {dir:?}: {error}"));
        let path = entry.path();
        let file_type = entry
            .file_type()
            .unwrap_or_else(|error| panic!("failed to stat {path:?}: {error}"));
        if file_type.is_dir() {
            collect_rs_files(&path, out);
        } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn crate_src_has_no_private_vector_algebra() {
    let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_root, &mut files);

    // The xargs-incident gate: a scan that silently touched zero files must
    // fail loudly, not report a false-clean pass.
    assert!(
        !files.is_empty(),
        "scanned zero .rs files under {src_root:?} -- the walker is broken, \
         this is not evidence the crate is clean"
    );

    let mut violations = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file)
            .unwrap_or_else(|error| panic!("failed to read {file:?}: {error}"));
        let rel = file
            .strip_prefix(&src_root)
            .unwrap_or(file)
            .to_string_lossy()
            .into_owned();
        for (line_no, line) in text.lines().enumerate() {
            for pattern in FORBIDDEN_PATTERNS {
                if line.contains(pattern) {
                    let allowed = ALLOWLIST.iter().any(|(allow_file, allow_pattern)| {
                        *allow_file == rel && *allow_pattern == *pattern
                    });
                    if !allowed {
                        violations.push(format!(
                            "{rel}:{line}: matches forbidden pattern `{pattern}` -- consolidate to \
                             geom_kernel::vec3 / geom_kernel::poly2 per docs/specs/geometry-canon.md, \
                             or add a GEOM-EXEMPT marker and an ALLOWLIST entry with a real reason",
                            line = line_no + 1
                        ));
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "private vector algebra reintroduced in plinth-core/src (scanned {} files):\n{}",
        files.len(),
        violations.join("\n")
    );
}
