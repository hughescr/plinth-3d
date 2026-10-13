//! Enforcement gate for `docs/specs/geometry-canon.md` §2 ("Enforcement",
//! census §2 fix 3): this crate must not grow a private copy of the geometry
//! primitives `geom_kernel::vec3` already publishes canonically.
//!
//! Consolidation census result for THIS crate
//! (`docs/plans/geometry-consolidation.md` OP-1, OP-6, OP-9,
//! `feature/stage0-bytes-audit`):
//!
//! * `drilling.rs:1962` `segment_distance` -- the census's one measured *bug*
//!   (OP-1, plan item C3). Retired for `geom_kernel::segment_segment_distance`.
//!   Its parallel short-circuit compared an mm^4 determinant against the mm
//!   constant `PROOF_EPSILON_MM = 1e-7`, firing on 35.98 % of pairs at
//!   drain-keepout scale and returning distances wrong by up to 4.494e-2 mm.
//!   The caller's refusals stayed at the caller, in `keepout_axis_distance`.
//! * `drilling.rs:1925-1946` `add`/`sub`/`scale`/`dot`/`cross`/`norm` and
//!   `drains.rs:2085-2103` `add`/`sub`/`cross`/`scale` -- private copies with
//!   kernel-identical formulas, retired for `geom_kernel::vec3` (OP-9, batch
//!   C8a, byte-neutral).
//! * `drains.rs:2075` `normalize` -- a `hypot` chain plus three divisions.
//!   Retired for `geom_kernel::vec3::normalize` (OP-6, plan items C4/C8b).
//!   NOT byte-neutral, by ruling; see the doc comment on the wrapper.
//! * `lib.rs:3082-3096` `sub3`/`cross3`/`dot3`, the separating-axis
//!   rasterizer's private algebra, retired for `geom_kernel::vec3::{sub, cross,
//!   dot}` (OP-9, batch C8a, byte-neutral: identical formulas, identical
//!   association).
//!
//! Sites reviewed and deliberately NOT consolidated, recorded here rather than
//! in `ALLOWLIST` because none of them matches this test's forbidden-signature
//! grep (they are inline arithmetic in test code, not private function
//! definitions):
//!
//! * `lib.rs:4521` (inside `#[cfg(test)]`) -- `mesh_surface_area_mm2`, a
//!   test-local triangle-area sum. MAY-STAY: the canon's triage rule step 3
//!   says to leave inline arithmetic in test code alone, because a test that
//!   reaches production arithmetic through the production crate is one step
//!   toward the test-re-implements-production defect already on this project's
//!   record.
//! * `lib.rs:8642` (inside `#[cfg(test)]`) -- `(i * i + j * j) as f64` over
//!   INTEGER lattice indices in the drill-clearance rung fixture. Not a
//!   floating-point vector length and not an instance of any OP-1..OP-9
//!   operation; triage rule step 5 (the canon's ruling does not fit), left
//!   alone.
//! * `examples/wall_distribution.rs:255,260` -- a diagnostic's own vertex-area
//!   histogram. Example code, kernel-identical association, feeds no verdict.
//!   MAY-STAY.
//! * `drains.rs:2133` (inside `#[cfg(test)]`) -- an inline
//!   `(x*x + y*y + z*z).sqrt()` ADDED by this sweep, in the test that asserts
//!   `normalize` returns a unit vector. It is written out by hand on purpose:
//!   the canon's triage rule step 3 says a test should not reach production
//!   arithmetic through the production crate, and an independent length is
//!   what makes "the result is unit" a claim rather than a restatement. A
//!   future census will flag it; this is the answer.
//!
//! This scan covers `src/` only, matching the canon's wording and every
//! sibling crate's gate. `examples/raster_delta.rs` no longer carries a copy of
//! the production rasterizer at all -- it calls
//! `plinth_hollow::diagnostics::triangle_cube_overlaps`.
//!
//! **Allowlist line numbers re-pointed 2026-10-13**, and only the numbers:
//! `src/drilling.rs` 2195 and `src/drains.rs` 2320. The ranked drill
//! fallback (drilling) and the re-plan entry (drains) added code above both,
//! so the two entries drifted off their targets and
//! `every_allowlist_entry_still_points_at_a_matching_line` said so — which is
//! that test doing its job. Both targets were re-read before the move and are
//! the same typed-refusal `normalize` wrappers with the same bodies, so the
//! reasons below are unchanged and still true (`unit_vector` at
//! `drilling.rs:8` is `geom_kernel::vec3::normalize` under an import rename).
//! No pattern was weakened and no entry was deleted.
//!
//! The line numbers in the census paragraphs ABOVE are deliberately not
//! re-pointed: they record where retired sites used to live, and rewriting
//! them would erase the history they exist to carry.
//!
//! Per the canon's own rule: gate on a **count of files actually scanned**
//! being greater than zero, not on the grep's exit code -- an empty match
//! set and an empty file set both "succeed" silently otherwise (the xargs
//! incident this project keeps citing).

use std::fs;
use std::path::{Path, PathBuf};

/// `(file, line, reason)` for a private-vector-algebra site this crate is
/// permitted to keep.
const ALLOWLIST: &[(&str, u32, &str)] = &[
    (
        "src/drilling.rs",
        2195,
        "canon §1 OP-6 / plan item C4: not a private normalize but the typed \
         refusal wrapper the canon REQUIRES at each call site. The body is \
         `geom_kernel::vec3::normalize(a).ok_or(InvalidHoleGeometry)`; no \
         caller may unwrap the kernel's `None` into a default vector, so the \
         mapping to drilling's own error has to live somewhere.",
    ),
    (
        "src/drains.rs",
        2320,
        "canon §1 OP-6 / plan item C4: the same typed refusal wrapper, mapping \
         `geom_kernel::vec3::normalize`'s `None` to \
         `DrainPlanningRefusal::NonFinite`.",
    ),
];

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
        "private vector-algebra sites found in plinth-hollow/src (scanned {} file(s)):\n{}",
        files.len(),
        violations.join("\n")
    );
}

/// The allowlist must not rot: every entry has to point at a line that still
/// exists and still carries the pattern it excuses. Without this, a refactor
/// that moves `normalize` leaves a silently-dead allowlist entry behind and the
/// gate stops covering the line it was written for -- a check reporting success
/// having done nothing, which is the failure class this whole sweep exists to
/// close.
#[test]
fn every_allowlist_entry_still_points_at_a_matching_line() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut checked = 0_usize;
    for (file, line_number, _reason) in ALLOWLIST {
        let path = manifest.join(file);
        let contents = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("allowlist entry {file}: read failed: {error}"));
        let line = contents
            .lines()
            .nth((*line_number as usize) - 1)
            .unwrap_or_else(|| panic!("allowlist entry {file}:{line_number} is past end of file"));
        assert!(
            FORBIDDEN_PATTERNS.iter().any(|p| line.contains(p)),
            "allowlist entry {file}:{line_number} no longer carries a forbidden pattern; the \
             line now reads `{line}`. Re-point or delete the entry."
        );
        checked += 1;
    }
    assert_eq!(
        checked,
        ALLOWLIST.len(),
        "allowlist verification visited {checked} of {} entries",
        ALLOWLIST.len()
    );
}
