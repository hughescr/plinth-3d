//! Enforcement gate for `docs/specs/geometry-canon.md` §2 ("Enforcement",
//! census §2 fix 3): this crate must not grow a private copy of the geometry
//! primitives `geom_kernel::vec3` already publishes canonically.
//!
//! Consolidation census result for THIS crate (`docs/plans/geometry-consolidation.md`
//! OP-9, `feature/stage0-bytes-audit`): `plinth-orient/src/score.rs:1139` carried
//! the crate's one private `subtract`/`cross`/`norm` block (census: "subtract
//! cross norm"). It has been retired in favour of `geom_kernel::vec3::{sub,
//! cross, norm}` -- Batch 8a, byte-neutral (0/2,000,000 measured difference for
//! the expanded-vs-`dot(a,a).sqrt()` norm spelling). The remaining ~23
//! ast-grep-flagged sites in `grid.rs`/`search.rs`/`score.rs` were all plain
//! `dot` products (rotation/scoring) already kernel-identical in association
//! and were swapped to `vec3::dot` for the same reason; one site
//! (`UpDirection::new`) was a normalize (a divide of a vector by its own norm)
//! and moved to `vec3::normalize`, which is a documented ≤1-ULP-per-component
//! behaviour change (see the doc comment on `UpDirection::new`), not a
//! byte-neutral one. Two sites are deliberately NOT consolidated and are
//! recorded here rather than in `ALLOWLIST` because neither matches this
//! test's forbidden-signature grep (they are inline arithmetic, not a private
//! function definition):
//!
//! * `score.rs`'s `rotation_to_build_frame` computes `x*x + y*y` as the
//!   well-conditioned twin of `1 + c` in a hand-derived, extensively documented
//!   closed-form rotation formula. It is not an instance of any OP-1..OP-9
//!   operation (there is no canon 2-component squared-length primitive), so
//!   the triage rule's step 5 applies: the canon's ruling does not fit this
//!   site, and it is left alone rather than forced through `vec3::dot` on a
//!   padded 3-vector.
//! * `lib.rs`'s `rotate_vertex` performs a checked matrix-vector product with
//!   per-term and per-accumulation finiteness refusal (`OrientError::
//!   InvalidTransformedMesh`); this is deliberately NOT the kernel's
//!   propagate-on-non-finite `dot` contract, so consolidating it would delete
//!   a refusal path this function exists to provide.
//!
//! Per the canon's own rule: gate on a **count of files actually scanned**
//! being greater than zero, not on the grep's exit code -- an empty match
//! set and an empty file set both "succeed" silently otherwise (the xargs
//! incident this project keeps citing).

use std::fs;
use std::path::{Path, PathBuf};

/// `(file, line, reason)` for a private-vector-algebra site this crate is
/// permitted to keep. Empty -- see the module doc for the two sites that were
/// reviewed and deliberately left in place; neither is a private function
/// definition, so neither trips `FORBIDDEN_PATTERNS`.
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
        "private vector-algebra sites found in plinth-orient/src (scanned {} file(s)):\n{}",
        files.len(),
        violations.join("\n")
    );
}
