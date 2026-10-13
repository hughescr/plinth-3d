//! Enforcement gate for `docs/specs/geometry-canon.md` §2 ("Enforcement",
//! census §2 fix 3): this crate must not grow a private copy of the geometry
//! primitives `geom_kernel::vec3` already publishes canonically.
//!
//! Consolidation census result for THIS crate
//! (`docs/plans/geometry-consolidation.md` OP-7/OP-9,
//! `feature/stage0-bytes-audit`): `plinth-pipeline` depends on `geom_kernel`
//! for the canonical `vec3` algebra alone (the not-hollowed warning's solid
//! volume, 2026-10-13) and its `src/` carries exactly one private-vector-algebra site,
//! `coverage_evidence.rs`'s `island_view_basis` render-basis construction and
//! its sibling `dot` helper. Both are evidence-only: they feed an SVG panel
//! shown to the owner on a coverage refusal, never a gate input
//! (`gates.rs` never imports this module -- stated in the module's own doc
//! header, and ratified in canon §1 OP-7 / §4.6 for the named `dot` site,
//! extended here to the sibling FMA arithmetic the census's `fn`-name grep
//! did not itemize separately because it is inlined rather than named).
//! Each site carries a `// GEOM-EXEMPT(...)` comment on the line above it in
//! `coverage_evidence.rs` in addition to the citation here.
//!
//! Two more sites live in this crate but outside `src/` and so are NOT
//! covered by this test, matching the scope canon §2 states ("a
//! `tests/no_private_vector_algebra.rs` grep-test over its own `src/`") and
//! the precedent set by `plinth-solid`'s equivalent test (which excludes its
//! test-only volume-audit oracle for the same reason): both are independent
//! diagnostic harnesses whose value depends on NOT sharing implementation
//! with the production code they check --
//! `examples/fu61_bore_penetration.rs`'s `add`/`sub`/`scale`/`dot`/`cross`/
//! `norm`/`normalize`/`normalize_or_zero` block (an independent ray-cast
//! measurement harness, by the file's own doc comment) and
//! `examples/shared/first_artifact_chain.rs`'s `signed_volume_f32` (an
//! independent on-disk audit that re-derives outward-winding from bytes
//! rather than trusting the producer). Each carries its own `GEOM-EXEMPT`
//! comment citing the same canon §4.1/§4.7 independence principle by
//! analogy. If a future change wants those sites covered by an automated
//! gate too, that is a deliberate scope expansion for the owner to weigh,
//! not a silent addition here.
//!
//! Gated on the scanned-file count being greater than zero (the
//! `xargs -d`-on-BSD incident rule, `docs/specs/geometry-canon.md`
//! "the failure mode this project reproduces"): a matcher that silently
//! walks zero files must fail loudly, not report green.

use std::fs;
use std::path::{Path, PathBuf};

/// `(file, line, reason)` for a private-vector-algebra site this crate's
/// `src/` is permitted to keep, each cited to the canon section that rules
/// it exempt.
const ALLOWLIST: &[(&str, u32, &str)] = &[
    (
        "src/coverage_evidence.rs",
        893,
        "island_view_basis cross term -- canon §1 OP-7/§4.6 extended (evidence-only SVG basis, never a gate input)",
    ),
    (
        "src/coverage_evidence.rs",
        894,
        "island_view_basis cross term -- canon §1 OP-7/§4.6 extended",
    ),
    (
        "src/coverage_evidence.rs",
        895,
        "island_view_basis cross term -- canon §1 OP-7/§4.6 extended",
    ),
    (
        "src/coverage_evidence.rs",
        899,
        "island_view_basis normal-length fold -- canon §1 OP-7/§4.6 extended",
    ),
    (
        "src/coverage_evidence.rs",
        915,
        "island_view_basis helper-cross term -- canon §1 OP-7/§4.6 extended",
    ),
    (
        "src/coverage_evidence.rs",
        916,
        "island_view_basis helper-cross term -- canon §1 OP-7/§4.6 extended",
    ),
    (
        "src/coverage_evidence.rs",
        917,
        "island_view_basis helper-cross term -- canon §1 OP-7/§4.6 extended",
    ),
    (
        "src/coverage_evidence.rs",
        921,
        "island_view_basis u-length fold -- canon §1 OP-7/§4.6 extended",
    ),
    (
        "src/coverage_evidence.rs",
        928,
        "island_view_basis v cross term -- canon §1 OP-7/§4.6 extended",
    ),
    (
        "src/coverage_evidence.rs",
        929,
        "island_view_basis v cross term -- canon §1 OP-7/§4.6 extended",
    ),
    (
        "src/coverage_evidence.rs",
        930,
        "island_view_basis v cross term -- canon §1 OP-7/§4.6 extended",
    ),
    (
        "src/coverage_evidence.rs",
        939,
        "named `dot` -- the exact site canon §1 OP-7/§4.6 exempts by name",
    ),
    (
        "src/coverage_evidence.rs",
        940,
        "named `dot` body (two `.mul_add(` calls on one line) -- canon §1 OP-7/§4.6",
    ),
];

/// Forbidden signatures: a private redefinition of a canon `vec3` primitive,
/// or a bare FMA outside a named, cited exemption.
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
fn crate_src_carries_no_uncited_private_vector_algebra() {
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
    let mut allowlist_hits = vec![false; ALLOWLIST.len()];
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
                let allowed_index =
                    ALLOWLIST
                        .iter()
                        .position(|(allowed_file, allowed_line, _reason)| {
                            relative_str.ends_with(allowed_file) && *allowed_line == line_number
                        });
                match allowed_index {
                    Some(index) => allowlist_hits[index] = true,
                    None => violations.push(format!(
                        "{relative_str}:{line_number}: `{pattern}` -- consolidate to \
                         geom_kernel::vec3 (docs/specs/geometry-canon.md §1 OP-6/OP-7/OP-9), \
                         or add it to ALLOWLIST here with a canon-section citation"
                    )),
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "private vector-algebra sites found in plinth-pipeline/src (scanned {} file(s)):\n{}",
        files.len(),
        violations.join("\n")
    );

    // An allowlist entry that stopped matching (moved, deleted, or the line
    // shifted) is exactly the "test that no longer says it was bad" failure
    // this project keeps reproducing in reverse: it would silently widen the
    // allowlist to a line that no longer needs it. Fail loudly instead.
    let stale: Vec<String> = ALLOWLIST
        .iter()
        .zip(&allowlist_hits)
        .filter(|(_, hit)| !**hit)
        .map(|((file, line, reason), _)| format!("{file}:{line}: {reason}"))
        .collect();
    assert!(
        stale.is_empty(),
        "ALLOWLIST entries that matched nothing (stale -- update the line number or remove \
         the entry):\n{}",
        stale.join("\n")
    );
}
