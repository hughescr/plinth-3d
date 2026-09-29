//! Guard 2 (standing guard): no hand-written geometry outside the kernel.
//!
//! Four ast-grep structural patterns -- a 3D dot product, a 3D cross
//! product, a 2D squared length, and a 3D squared length -- scanned over
//! every crate's `src/` OUTSIDE `rust/geom_kernel`
//! (`docs/plans/geometry-consolidation.md`'s own workspace-wide ast-grep
//! census: dot 56 sites, cross 24, squared-length-3D 24, squared-length-2D
//! 30, all outside the kernel, before this sweep). A hit fails the build
//! UNLESS the site carries an exemption marker on the line(s) immediately
//! above it (`// GEOM-EXEMPT(<reason>)`, per
//! `docs/specs/geometry-canon.md` §2) or sits inside the file's own inline
//! `#[cfg(test)] mod ... { }` block (canon §2 step 3, MAY-STAY: test code
//! needs no marker).
//!
//! # Why structural matching, not a name-based grep
//!
//! The per-crate `tests/no_private_vector_algebra.rs` guards (one per swept
//! crate, canon §2 "Enforcement") catch a NAMED private helper --
//! `fn dot(`, `fn cross(`, etc. They do not catch the same arithmetic
//! spelled out inline with no function at all, which is exactly how OP-1
//! (`docs/plans/geometry-consolidation.md`) hid for months: `drilling.rs`'s
//! bug was in an inline expression, not a suspiciously-named function. This
//! guard is the other half: it matches the AST SHAPE
//! (`$A[0] * $B[0] + $A[1] * $B[1] + $A[2] * $B[2]`, with `$A`/`$B` binding
//! to any expression, including two different names or the same name
//! twice), so it catches the arithmetic regardless of what the author
//! called their variables.
//!
//! # Implementation note the brief calls out explicitly, verified here
//!
//! ast-grep's YAML rule form for "not preceded by a matching comment" does
//! not validate on the installed version (confirmed independently while
//! building this guard: an `all: [pattern, not: {inside: {kind: mod_item}}]`
//! rule was ALSO rejected the same way -- "Rule must specify a set of AST
//! kinds to match" -- so this is not specific to comment-adjacency, it is
//! this ast-grep version rejecting composite relational rules generally).
//! So: plain `ast-grep run -p <pattern>` extraction
//! (`tests/guard_scripts/no_inline_geometry.sh`), classification done here
//! in Rust instead of in the rule.
//!
//! ast-grep's JSON line numbers are 0-indexed. This file's
//! `has_marker_above` and `test_module_boundary` both take that 0-indexed
//! number AS the 1-indexed line immediately above the match -- i.e. no `+1`
//! for "the line above", a `+1` for "the match's own 1-indexed line". Get
//! this backwards and every marker check silently reads the wrong line.
//! `line_arithmetic_matches_a_known_case` below pins it against a real file
//! read two ways, and the fixture self-test below exercises the full
//! extraction -> classification pipeline end to end so this guard's own
//! ability to fire is proven every run, not asserted once and forgotten
//! (`docs/plans/geometry-consolidation.md`: "a green suite that never
//! called the new function is the default outcome, not the unlikely one").

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone)]
struct Hit {
    pattern: String,
    file: String,
    /// 0-indexed, exactly as ast-grep's `range.start.line` reports it.
    line0: usize,
}

#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    /// Inside the file's own trailing `#[cfg(test)] mod ... { }` block.
    TestCode,
    /// A `// GEOM-EXEMPT(...)` marker sits on the contiguous comment block
    /// immediately above the match.
    Marked,
    /// Neither: a real, unattributed hit.
    Violation,
}

fn guard_script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/guard_scripts/no_inline_geometry.sh")
}

/// Runs the extraction script against `roots` (interpreted relative to
/// `cwd`) and returns every candidate hit plus the independently-counted
/// number of `.rs` files the script walked. Panics loudly (never returns an
/// empty, "looks clean" result) if `ast-grep`/`jq` are missing or the script
/// otherwise fails -- a tool that could not run is not evidence of a clean
/// tree.
fn run_guard_script(cwd: &Path, roots: &[&str]) -> (Vec<Hit>, usize) {
    let script = guard_script_path();
    assert!(script.is_file(), "guard script missing at {script:?}");
    let output = Command::new("sh")
        .arg(&script)
        .args(roots)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|error| panic!("failed to spawn {script:?}: {error}"));
    assert!(
        output.status.success(),
        "{script:?} failed (status {:?}). This means the extraction pass \
         itself did not run -- treat as UNKNOWN, never as a pass.\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut hits = Vec::new();
    let mut scanned_files = None;
    for line in stdout.lines() {
        let mut parts = line.split('\t');
        let a = parts.next().unwrap_or_default();
        let b = parts.next().unwrap_or_default();
        let c = parts.next();
        if a == "SCANNED_FILES" {
            scanned_files = Some(
                b.parse::<usize>()
                    .unwrap_or_else(|error| panic!("bad SCANNED_FILES row {line:?}: {error}")),
            );
            continue;
        }
        let line0 = c
            .unwrap_or_else(|| panic!("malformed guard script row: {line:?}"))
            .parse::<usize>()
            .unwrap_or_else(|error| panic!("bad line number in row {line:?}: {error}"));
        hits.push(Hit {
            pattern: a.to_string(),
            file: b.to_string(),
            line0,
        });
    }
    let scanned_files = scanned_files.unwrap_or_else(|| {
        panic!("guard script produced no SCANNED_FILES row -- output was:\n{stdout}")
    });
    (hits, scanned_files)
}

/// The 1-indexed line number at which `text`'s own inline unit-test module
/// begins, if it has one: a column-0 `#[cfg(test)]` line immediately
/// followed by a column-0 `mod <name> {` line. Lines at or after that
/// number are MAY-STAY test code (`docs/specs/geometry-canon.md` §2 step 3)
/// and need no exemption marker.
///
/// Deliberately narrower than "any `#[cfg(test)]`": a bare
/// `#[cfg(test)] fn helper_used_only_by_tests()` at column 0 is common in
/// this codebase (verified: `plinth-hollow/src/lib.rs`,
/// `plinth-decimate/src/lib.rs`, `plinth-mesh/src/triage.rs` and others all
/// have one) and is NOT the start of the trailing test module, so treating
/// every `#[cfg(test)]` as a boundary would misclassify production-adjacent
/// helper functions that happen to sit earlier in the file as "test code"
/// too early -- and, worse in the other direction, would treat everything
/// AFTER that stray line, including real production code, as exempt.
/// Requiring the very next line to be `mod ... {` avoids both.
fn test_module_boundary(lines: &[&str]) -> Option<usize> {
    for (index, line) in lines.iter().enumerate() {
        if *line == "#[cfg(test)]" {
            if let Some(next) = lines.get(index + 1) {
                if next.starts_with("mod ") && next.trim_end().ends_with('{') {
                    return Some(index + 2); // 1-indexed line number of the `mod` line
                }
            }
        }
    }
    None
}

/// Walks upward from `line_above_1indexed` (the line immediately above a
/// match; despite the name this is numerically the match's 0-indexed line,
/// see the module doc) through a CONTIGUOUS run of `//` comment lines,
/// returning true if any of them contains `GEOM-EXEMPT`. Contiguous: a
/// blank line or any non-comment line breaks the chain, so a marker must be
/// tight against its site rather than merely somewhere above it in the
/// file. Capped at 60 lines as a sanity bound, not a real limit -- the
/// longest real marker block in the tree today (`joints.rs::segment_distance_2d`'s
/// doc-comment marker) is under 15 lines.
fn has_marker_above(lines: &[&str], line_above_1indexed: usize) -> bool {
    let mut idx = line_above_1indexed;
    let floor = line_above_1indexed.saturating_sub(60);
    while idx >= 1 && idx > floor {
        let Some(text) = lines.get(idx - 1) else {
            break;
        };
        let trimmed = text.trim_start();
        if !trimmed.starts_with("//") {
            break;
        }
        if trimmed.contains("GEOM-EXEMPT") {
            return true;
        }
        idx -= 1;
    }
    false
}

fn classify(lines: &[&str], hit: &Hit) -> Verdict {
    let match_line_1indexed = hit.line0 + 1;
    if let Some(boundary) = test_module_boundary(lines) {
        if match_line_1indexed >= boundary {
            return Verdict::TestCode;
        }
    }
    if has_marker_above(lines, hit.line0) {
        return Verdict::Marked;
    }
    Verdict::Violation
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("rust/geom_kernel has a workspace root two levels up")
        .to_path_buf()
}

/// `src`-only scan roots (relative to the workspace root), matching the
/// scope of every per-crate `no_private_vector_algebra.rs` guard already in
/// the tree: test/bench/example code is canon's own MAY-STAY category
/// (`docs/specs/geometry-canon.md` §2 step 3) and simply is not scanned,
/// rather than being scanned and then blanket-exempted. Excludes
/// `rust/geom_kernel` (the kernel is the canon, not a violation of it) and
/// never reads `tools/plinth-audit` at all
/// (`docs/plans/geometry-consolidation.md` §5.1, permanent).
fn production_src_roots(root: &Path) -> Vec<String> {
    let crates_dir = root.join("crates");
    let mut roots: Vec<String> = fs::read_dir(&crates_dir)
        .unwrap_or_else(|error| panic!("failed to read {crates_dir:?}: {error}"))
        .map(|entry| entry.expect("readable crates/ entry").path())
        .filter(|path| path.is_dir() && path.join("src").is_dir())
        .map(|path| {
            path.join("src")
                .strip_prefix(root)
                .expect("crate src under workspace root")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    roots.sort();
    let chitubox_src = root.join("rust").join("chitubox_io").join("src");
    if chitubox_src.is_dir() {
        roots.push("rust/chitubox_io/src".to_string());
    }
    roots
}

#[test]
fn no_hand_written_geometry_outside_the_kernel() {
    let root = workspace_root();
    let roots = production_src_roots(&root);
    assert!(
        roots.len() >= 10,
        "found only {} production src roots -- the discovery logic is \
         almost certainly broken (this workspace has 12 crates)",
        roots.len()
    );
    let root_refs: Vec<&str> = roots.iter().map(String::as_str).collect();
    let (hits, scanned_files) = run_guard_script(&root, &root_refs);

    assert!(
        scanned_files > 50,
        "the extraction script scanned only {scanned_files} .rs files across \
         {roots:?} -- measured baseline is 91 src files across 12 crates plus \
         chitubox_io (2026-08-31); treat a count far below that as a broken \
         scan, not a small workspace"
    );

    let mut file_cache: HashMap<String, Vec<String>> = HashMap::new();
    let mut violations = Vec::new();
    let mut marked = Vec::new();
    let mut test_code = Vec::new();

    for hit in &hits {
        let owned_lines = file_cache.entry(hit.file.clone()).or_insert_with(|| {
            let path = root.join(&hit.file);
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {path:?}: {error}"));
            text.lines().map(str::to_owned).collect()
        });
        let lines: Vec<&str> = owned_lines.iter().map(String::as_str).collect();
        match classify(&lines, hit) {
            Verdict::TestCode => test_code.push(hit),
            Verdict::Marked => marked.push(hit),
            Verdict::Violation => violations.push(hit),
        }
    }

    println!(
        "Guard 2: scanned {scanned_files} .rs files across {} src roots; \
         {} raw pattern hits -> {} test-code (MAY-STAY), {} GEOM-EXEMPT \
         marked, {} violations",
        roots.len(),
        hits.len(),
        test_code.len(),
        marked.len(),
        violations.len(),
    );
    for hit in &marked {
        println!(
            "  [exempt, marker]   {}:{} ({})",
            hit.file,
            hit.line0 + 1,
            hit.pattern
        );
    }
    for hit in &test_code {
        println!(
            "  [exempt, test]     {}:{} ({})",
            hit.file,
            hit.line0 + 1,
            hit.pattern
        );
    }

    assert!(
        violations.is_empty(),
        "hand-written geometry found outside the kernel with no exemption \
         marker and not in test code. Consolidate to geom_kernel::vec3 / \
         geom_kernel::poly2 (docs/specs/geometry-canon.md), or add \
         `// GEOM-EXEMPT(<reason>)` on the line directly above the site:\n{}",
        violations
            .iter()
            .map(|hit| format!("  {}:{} ({})", hit.file, hit.line0 + 1, hit.pattern))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

// ---------------------------------------------------------------------------
// Self-test: proves the extraction -> classification pipeline can actually
// fail, every time this suite runs. Writes fixture files to a scratch
// directory (never inside any crate this checkout owns) and runs the real
// script and the real `classify` function against them.
// ---------------------------------------------------------------------------

/// A fresh scratch directory private to `name` (pass the calling test's own
/// name). `cargo test` runs test functions concurrently by default within
/// one binary, so a directory shared across tests raced: one test's
/// `remove_dir_all` could delete the fixture out from under another test
/// mid-read. Caught by this file's own test suite while it was being built,
/// not assumed away.
fn scratch_dir(name: &str) -> PathBuf {
    let base = std::env::var("CARGO_TARGET_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    let dir = base.join(format!("kn_no_inline_geometry_guard_fixture_{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).expect("create fixture src dir");
    dir
}

const FIXTURE_SOURCE: &str = r#"// A synthetic file, never part of any real crate, used only to prove Guard
// 2's own detection logic can fail. See kn_no_inline_geometry_guard.rs.

fn production_code_without_marker() -> f64 {
    let a = [1.0, 2.0, 3.0];
    let b = [4.0, 5.0, 6.0];
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn production_code_with_single_line_marker() -> f64 {
    let a = [1.0, 2.0, 3.0];
    let b = [4.0, 5.0, 6.0];
    // GEOM-EXEMPT(fixture: a single-line marker directly above the site)
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn production_code_with_multiline_marker_block() -> f64 {
    let a = [1.0, 2.0, 3.0];
    let b = [4.0, 5.0, 6.0];
    // GEOM-EXEMPT(fixture: multi-line block form, marker text on the FIRST
    // line, several lines above the code -- this is the real convention
    // used across the tree today, e.g. anchor.rs:1997-1998,
    // dyadic.rs:851-852, perception.rs:2461-2462) --
    // this continuation line carries no marker text at all
    // neither does this one
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[cfg(test)]
mod tests {
    fn test_code_without_marker() -> f64 {
        let a = [1.0, 2.0, 3.0];
        let b = [4.0, 5.0, 6.0];
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }
}
"#;

#[test]
fn detector_fires_on_an_unmarked_site_and_respects_both_marker_forms_and_test_code() {
    let dir = scratch_dir("detector_fires");
    let fixture_path = dir.join("src").join("lib.rs");
    fs::write(&fixture_path, FIXTURE_SOURCE).expect("write fixture");

    let (hits, scanned) = run_guard_script(&dir, &["src"]);
    assert_eq!(
        scanned, 1,
        "fixture scan should see exactly the one file it wrote"
    );
    assert!(
        !hits.is_empty(),
        "the extraction pipeline found NO hits at all in a fixture file that \
         contains four textbook dot-product expressions -- ast-grep or the \
         script itself is broken, which means the real guard cannot be \
         trusted to have run either"
    );

    let text = fs::read_to_string(&fixture_path).expect("read fixture back");
    let lines: Vec<&str> = text.lines().collect();

    // Every dot-product-3d hit (there is exactly one qualifying expression
    // per function above), classified, keyed by which function it landed in.
    let mut by_line0: HashMap<usize, Verdict> = HashMap::new();
    for hit in hits.iter().filter(|h| h.pattern == "dot-product-3d") {
        by_line0.insert(hit.line0, classify(&lines, hit));
    }

    let line_of = |needle: &str| -> usize {
        lines
            .iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("fixture missing expected line {needle:?}"))
    };

    let unmarked_line0 = line_of("a[0] * b[0] + a[1] * b[1] + a[2] * b[2]");
    // ^ first occurrence: inside production_code_without_marker.
    assert_eq!(
        by_line0.get(&unmarked_line0),
        Some(&Verdict::Violation),
        "an unmarked, non-test dot product must classify as a VIOLATION -- \
         if this assertion fails, the guard has lost the ability to catch \
         the exact defect it exists for"
    );

    let single_marker_line0 = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.contains("a[0] * b[0] + a[1] * b[1] + a[2] * b[2]"))
        .nth(1)
        .map(|(index, _)| index)
        .expect("second occurrence: single-line-marker function");
    assert_eq!(
        by_line0.get(&single_marker_line0),
        Some(&Verdict::Marked),
        "a site with a GEOM-EXEMPT marker on the immediately preceding line \
         must classify as Marked, not Violation"
    );

    let multiline_marker_line0 = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.contains("a[0] * b[0] + a[1] * b[1] + a[2] * b[2]"))
        .nth(2)
        .map(|(index, _)| index)
        .expect("third occurrence: multiline-marker-block function");
    assert_eq!(
        by_line0.get(&multiline_marker_line0),
        Some(&Verdict::Marked),
        "a site whose GEOM-EXEMPT marker is on the FIRST line of a multi-line \
         comment block above it (not the immediately-preceding line) must \
         still classify as Marked -- this is the real convention used at \
         anchor.rs/dyadic.rs/perception.rs today. A naive 'check only the \
         single line directly above' implementation gets this WRONG, which \
         is exactly why this guard walks a contiguous comment block instead \
         of checking one line (verified as a known failure mode before \
         trusting the real implementation, per the brief)."
    );
    // Prove the naive single-line check really would have failed here, so
    // the block-scan is a demonstrated improvement, not an assumed one.
    let naive_single_line_sees_marker = lines
        .get(multiline_marker_line0.saturating_sub(1))
        .is_some_and(|line| line.contains("GEOM-EXEMPT"));
    assert!(
        !naive_single_line_sees_marker,
        "fixture is wrong: the line immediately above the multiline-marker \
         site should NOT itself contain GEOM-EXEMPT, or this test proves \
         nothing about the block-scan being necessary"
    );

    let test_code_line0 = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.contains("a[0] * b[0] + a[1] * b[1] + a[2] * b[2]"))
        .nth(3)
        .map(|(index, _)| index)
        .expect("fourth occurrence: inline #[cfg(test)] mod");
    assert_eq!(
        by_line0.get(&test_code_line0),
        Some(&Verdict::TestCode),
        "a site inside the file's own trailing #[cfg(test)] mod block must \
         classify as TestCode (MAY-STAY, canon §2 step 3) even with no \
         marker at all"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Pins the 0-indexed/1-indexed line arithmetic against a real file read two
/// independent ways, so the convention documented at the top of this file
/// (and used throughout `classify`) cannot silently drift.
#[test]
fn line_arithmetic_matches_a_known_case() {
    let dir = scratch_dir("line_arithmetic");
    let fixture_path = dir.join("src").join("lib.rs");
    fs::write(
        &fixture_path,
        "fn f() -> f64 {\n    let a = [1.0, 2.0, 3.0];\n    let b = [4.0, 5.0, 6.0];\n    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]\n}\n",
    )
    .expect("write fixture");
    let (hits, _) = run_guard_script(&dir, &["src"]);
    let hit = hits
        .iter()
        .find(|h| h.pattern == "dot-product-3d")
        .expect("dot-product-3d hit in the known fixture");
    // The expression is on line 4 when counting from 1 (the `fn f()` line is
    // 1). ast-grep must report 0-indexed 3.
    assert_eq!(hit.line0, 3, "ast-grep line numbering assumption changed -- re-derive the +1/+0 convention throughout this file before trusting any classification");
    let text = fs::read_to_string(&fixture_path).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[hit.line0 + 1 - 1],
        "    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]",
        "line0 + 1 (1-indexed) minus 1 (array index) must land on the matched line itself"
    );
    assert_eq!(
        lines[hit.line0 - 1],
        "    let b = [4.0, 5.0, 6.0];",
        "line0 used directly as a 1-indexed 'line above' must land one line above the match \
         -- this is the exact convention the brief's sed -n \"${{l}}p\" relies on"
    );
    let _ = fs::remove_dir_all(&dir);
}
