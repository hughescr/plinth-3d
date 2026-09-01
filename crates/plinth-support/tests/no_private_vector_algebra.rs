//! The consolidation ratchet for this crate.
//!
//! `docs/specs/geometry-canon.md` §2 ("Enforcement") requires every swept crate
//! to carry a grep test over its own `src/` that fails when private vector
//! algebra grows back. This is `plinth-support`'s.
//!
//! # What it forbids, and why each pattern is on the list
//!
//! | pattern | canon |
//! |---|---|
//! | `fn dot(`, `fn cross(`, `fn norm(`, `fn normalize(`, `fn sub(`, `fn subtract(` | §1 OP-6/OP-7/OP-9 — `geom_kernel::vec3` is the one 3-vector algebra |
//! | `.mul_add(` | §1 OP-4/OP-7 — no FMA; a fused value is a DIFFERENT value from the kernel's |
//! | `.hypot(` on three or more terms | §1 OP-6 — the `hypot` chain is the retired third norm formula (plan item C8b) |
//!
//! # The second matcher: inline 3-vector SHAPE, production code only
//!
//! The canon's `fn`-name list has a blind spot a rename walks straight
//! through: `fn vector_norm(`, `fn seat_len(`, or no function at all — the
//! arithmetic simply written out at the call site, which is how the ~134-site
//! census got that large in the first place. So this test also fails on any
//! PRODUCTION line carrying the shape `[0] * `, `[1] * ` and `[2] * `
//! together, which is what a hand-written 3-vector dot, squared length or
//! cross always looks like.
//!
//! Restricted to production code — everything above a file's first top-level
//! `#[cfg(test)]` — because the canon's §2 step 3 explicitly leaves inline
//! arithmetic in tests alone: "a test re-implementing production arithmetic
//! through the production crate is one step toward the
//! test-re-implements-production defect already on this project's record."
//! `plinth-support` has two such deliberate scalar oracles (`joints.rs`'s
//! `dist_and_radius_scalar`, `contact.rs`'s seat-cosine checks) and they must
//! stay independent.
//!
//! Measured at the time of writing: this matcher finds **0** production hits
//! in this crate and 14 test-side hits it correctly ignores.
//!
//! `.hypot(` is not in the canon's minimum list. It is added here because C8b
//! removed four `hypot`-chain norms from this crate (`anchor`, `cco`,
//! `specialized::distance3`, `member_sizing`), and an unguarded `hypot` is the
//! exact shape they would grow back in. The 2-ARGUMENT form is legitimate — it
//! is the XY magnitude, not a 3-vector norm — so those sites are allowlisted
//! individually rather than the pattern being dropped.
//!
//! # Why the allowlist is keyed on line TEXT, not line number
//!
//! The canon asks for "file + line + canon-section citation per entry". A line
//! number is the one part of that triple which is wrong the moment anyone edits
//! the file above it, and a stale allowlist that silently stops matching is
//! precisely the failure this project keeps reproducing: a check that reports
//! success having examined nothing. Keying on the exact source text makes a
//! moved line still match and an EDITED line stop matching, which is the
//! behaviour worth having. The line number is still reported in failures.
//!
//! # The gates that stop this test passing vacuously
//!
//! 1. It fails unless it actually read at least [`MIN_SCANNED_FILES`] `.rs`
//!    files (the crate has 29; a wrong path or a broken walk reads 0).
//! 2. It fails unless it actually examined at least [`MIN_SCANNED_LINES`]
//!    lines.
//! 3. It fails unless EVERY allowlist entry was matched at least once. An
//!    entry that stops matching means either the code was fixed — delete the
//!    entry — or the matcher broke. Both must be loud.
//!
//! Gate 3 is the one that matters: it is a live sabotage check. If the pattern
//! list, the comment stripping, or the file walk silently stopped working,
//! every allowlist entry would go unhit and this test would fail, rather than
//! reporting a clean crate having looked at nothing.

use std::fs;
use std::path::{Path, PathBuf};

/// Substrings that may not appear in this crate's `src/`, outside [`ALLOWLIST`].
const FORBIDDEN: &[&str] = &[
    "fn dot(",
    "fn cross(",
    "fn norm(",
    "fn normalize(",
    "fn sub(",
    "fn subtract(",
    ".mul_add(",
    ".hypot(",
];

/// Component subscripts that, appearing together on ONE production line, mean
/// somebody hand-wrote 3-vector arithmetic instead of calling the kernel.
const INLINE_VECTOR_SHAPE: &[&str] = &["[0] * ", "[1] * ", "[2] * "];

/// The crate has 29 files in `src/`; require most of them to have been read.
const MIN_SCANNED_FILES: usize = 25;
/// ...and require the line walk to have produced a plausible volume of work.
/// Measured at the time of writing: 102,088 lines across 30 files.
const MIN_SCANNED_LINES: usize = 50_000;

/// Lines ABOVE each file's first top-level `#[cfg(test)]` — the only ones the
/// inline-shape matcher looks at. Measured at the time of writing: 17,573 of
/// the 102,088 scanned, because this crate is ~83 % test code by line. The
/// gate exists so a misplaced cut cannot silence that matcher while the test
/// still reports green.
const MIN_PRODUCTION_LINES: usize = 12_000;

/// One permitted occurrence: the file it lives in, its exact source line
/// (trimmed), and the canon section that permits it.
struct Exempt {
    file: &'static str,
    line: &'static str,
    reason: &'static str,
}

const ALLOWLIST: &[Exempt] = &[
    Exempt {
        file: "coverage_repair.rs",
        line: "unit.mul_add(40.0, -20.0)",
        reason: "canon §2 step 3 (MAY-STAY, test code): a test-local LCG mapping a \
                 unit scalar onto [-20, 20] mm. No vector, no tolerance comparison, \
                 no verdict — the no-FMA rule has nothing to bite on.",
    },
    Exempt {
        file: "dyadic.rs",
        line: "u[1].mul_add(v[2], -u[2] * v[1]),",
        reason: "canon §2 step 3 (MAY-STAY, test code): the RETIRED fused cross, \
                 quoted inside `face_normal_and_area_are_plain_not_fused` so that \
                 test can assert the production form differs from it bit for bit. \
                 Deleting these lines would make that test unable to discriminate.",
    },
    Exempt {
        file: "dyadic.rs",
        line: "u[2].mul_add(v[0], -u[0] * v[2]),",
        reason: "canon §2 step 3 — second component of the quoted retired cross \
                 above.",
    },
    Exempt {
        file: "dyadic.rs",
        line: "u[0].mul_add(v[1], -u[1] * v[0]),",
        reason: "canon §2 step 3 — third component of the quoted retired cross \
                 above.",
    },
    Exempt {
        file: "anchor.rs",
        line: "delta[0].hypot(delta[1]).atan2(delta[2].abs()).to_degrees()",
        reason: "canon §1 OP-6 retires the hypot CHAIN as a 3-vector norm. This is \
                 the 2-argument XY magnitude feeding `atan2` for a lean angle; there \
                 is no third `hypot` and no 3-vector length here.",
    },
    Exempt {
        file: "cco.rs",
        line: "delta[0].hypot(delta[1]).atan2(delta[2].abs()).to_degrees()",
        reason: "canon §1 OP-6 — as `anchor.rs`'s `lean_deg`: 2-argument XY \
                 magnitude into `atan2`, not a norm.",
    },
    Exempt {
        file: "brace.rs",
        line: "let distance = dx.hypot(dy);",
        reason: "canon §1 OP-6 — 2-D cross-link spacing in the XY plane. The kernel \
                 publishes no 2-D distance (`poly2` carries polygon primitives only, \
                 canon §1 OP-8), so there is nothing to consolidate to.",
    },
    Exempt {
        file: "member_sizing.rs",
        line: "let horizontal_mm = dx.hypot(dy);",
        reason: "canon §1 OP-6 — the XY magnitude, reused by `atan2` for the frozen \
                 lean angle. The 3-D length on the next line WAS a chained `hypot` \
                 and is now `vec3::norm` (plan item C8b).",
    },
    Exempt {
        file: "perception.rs",
        line: "0.5 * ((max_x - min_x).hypot(max_y - min_y)),",
        reason: "canon §1 OP-6 — half the XY diagonal of a slice bounding box. \
                 2-argument form, no 3-vector norm.",
    },
    Exempt {
        file: "joints.rs",
        line: "return (qx - ax).hypot(qy - ay);",
        reason: "canon §4 item 5 — `segment_distance_2d` operates on `(axial, \
                 radial)` frustum-profile data, not slice loops, and is EXEMPT from \
                 the OP-3 consolidation. `hypot` is its 2-argument distance, the \
                 same spelling `geom_kernel::poly2::point_segment_distance` uses.",
    },
    Exempt {
        file: "joints.rs",
        line: "(qx - (ax + t * dx)).hypot(qy - (ay + t * dy))",
        reason: "canon §4 item 5 — the projected-point arm of the same exempt \
                 `segment_distance_2d`.",
    },
];

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("reading {} failed: {error}", dir.display()));
    let mut paths = entries
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    // Sorted so the failure report is stable run to run.
    paths.sort();
    for path in paths {
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// A line that is entirely a comment cannot be code. Trailing comments are NOT
/// stripped: `let x = a.mul_add(b, c); // why` must still fail.
fn is_comment_line(trimmed: &str) -> bool {
    trimmed.starts_with("//")
}

#[test]
fn no_private_vector_algebra() {
    let root = src_dir();
    let mut files = Vec::new();
    rust_files(&root, &mut files);

    let mut scanned_lines = 0_usize;
    let mut production_lines = 0_usize;
    let mut allow_hits = vec![0_usize; ALLOWLIST.len()];
    let mut violations: Vec<String> = Vec::new();

    for path in &files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("utf-8 file name")
            .to_owned();
        let text = fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("reading {} failed: {error}", path.display()));
        // Everything from the top-level `#[cfg(test)] mod ...` onward is test
        // code. The cut must be the `#[cfg(test)]` that gates a MODULE, not
        // just any `#[cfg(test)]`: `dyadic.rs` carries six of them on
        // individual test-only helper items from line 460 onward, and cutting
        // at the first one would have hidden that file's real production
        // geometry — `max_edge_mm`, `dist_sq`, `triangle_area_mm2`,
        // `face_normal`, all of which live BELOW it — from the shape matcher.
        // Measured before this was fixed: the cut landed at 460 of 1050.
        let lines = text.lines().collect::<Vec<_>>();
        let test_cut = lines
            .iter()
            .enumerate()
            .position(|(index, line)| {
                line.starts_with("#[cfg(test)]")
                    && lines
                        .get(index + 1)
                        .is_some_and(|next| next.trim_start().starts_with("mod "))
            })
            .unwrap_or(usize::MAX);
        for (index, raw) in text.lines().enumerate() {
            scanned_lines += 1;
            let trimmed = raw.trim();
            if is_comment_line(trimmed) {
                continue;
            }
            let production = index < test_cut;
            if production {
                production_lines += 1;
            }
            let shape = production
                && INLINE_VECTOR_SHAPE
                    .iter()
                    .all(|needle| raw.contains(*needle));
            let Some(pattern) = FORBIDDEN
                .iter()
                .find(|needle| raw.contains(**needle))
                .copied()
                .or(shape.then_some("inline 3-vector arithmetic"))
            else {
                continue;
            };
            let allowed = ALLOWLIST
                .iter()
                .position(|entry| entry.file == name && entry.line == trimmed);
            match allowed {
                Some(slot) => allow_hits[slot] += 1,
                None => violations.push(format!(
                    "  {}:{}: `{pattern}` in `{trimmed}`\n    \
                     Consolidate to geom_kernel::vec3 / geom_kernel::poly2 \
                     (docs/specs/geometry-canon.md §1), or add an ALLOWLIST entry \
                     here AND a `// GEOM-EXEMPT(...)` marker at the site.",
                    name,
                    index + 1,
                )),
            }
        }
    }

    // Gate 1 and 2: a matcher that examined nothing must fail, not pass.
    assert!(
        files.len() >= MIN_SCANNED_FILES,
        "scanned only {} .rs files under {} — expected at least {MIN_SCANNED_FILES}. \
         A check that finds no files reports success having done nothing.",
        files.len(),
        root.display(),
    );
    assert!(
        scanned_lines >= MIN_SCANNED_LINES,
        "scanned only {scanned_lines} lines — expected at least {MIN_SCANNED_LINES}.",
    );
    // The shape matcher only ever looks at production lines. If the
    // `#[cfg(test)]` cut ever swallowed the whole crate, that matcher would go
    // silent while this test still reported green — so gate on it too.
    assert!(
        production_lines >= MIN_PRODUCTION_LINES,
        "only {production_lines} of {scanned_lines} scanned lines counted as \
         production (expected at least {MIN_PRODUCTION_LINES}), which means the \
         `#[cfg(test)]` cut is misplaced and the inline-shape matcher examined \
         almost nothing.",
    );

    // Gate 3: every allowlist entry must still be reachable by the matcher.
    let unhit = ALLOWLIST
        .iter()
        .zip(&allow_hits)
        .filter(|(_, hits)| **hits == 0)
        .map(|(entry, _)| format!("  {}: `{}`", entry.file, entry.line))
        .collect::<Vec<_>>();
    assert!(
        unhit.is_empty(),
        "{} ALLOWLIST entries matched nothing. Either the site was consolidated \
         (delete the entry) or this test's matcher stopped working (fix it — a \
         silently non-matching grep test is the failure mode this file exists to \
         prevent):\n{}",
        unhit.len(),
        unhit.join("\n"),
    );

    assert!(
        violations.is_empty(),
        "{} private-vector-algebra site(s) in plinth-support/src:\n{}",
        violations.len(),
        violations.join("\n"),
    );
}

/// The allowlist is a list of EXCEPTIONS, not a second implementation of the
/// rule. If it ever grows past a size a reviewer will actually read, the crate
/// has drifted and the sweep needs redoing rather than the list extending.
/// An exemption without a ruling behind it is a preference, and the next
/// census cannot tell the two apart. This also keeps `Exempt::reason` a read
/// field rather than dead documentation.
#[test]
fn every_allowlist_entry_cites_the_canon() {
    for entry in ALLOWLIST {
        assert!(
            entry.reason.contains("canon §"),
            "ALLOWLIST entry {}: `{}` carries no canon citation. Reason given: {}",
            entry.file,
            entry.line,
            entry.reason,
        );
    }
}

#[test]
fn allowlist_stays_small_enough_to_read() {
    assert!(
        ALLOWLIST.len() <= 14,
        "the geometry-canon allowlist has grown to {} entries; \
         re-run the consolidation instead of extending it \
         (docs/specs/geometry-canon.md §2)",
        ALLOWLIST.len(),
    );
}
