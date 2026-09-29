//! Enforcement test for the geometry-consolidation sweep
//! (`docs/specs/geometry-canon.md` §2): this crate's `src/` must not grow a
//! new private copy of 3-vector algebra now that `geom_kernel::vec3` is the
//! canonical home (canon §1 OP-9, plan item C1).
//!
//! Scans `crates/plinth-emit/src/` for definitions of the retired private
//! helpers. Any hit that is not on the allowlist below fails the build.
//! The allowlist is empty: C1 deleted the crate's only private copy
//! (`emit/lib.rs:1010-1031`) with no exemption.
//!
//! Gated on a *count of files actually scanned* being greater than zero
//! (`docs/plans/geometry-consolidation.md` "xargs incident" rule) so a
//! matcher that silently finds nothing fails loudly instead of passing.

use std::path::Path;

/// `(file, line)` pairs explicitly permitted to define one of the banned
/// helper names, each citing the canon section that exempts it. Empty for
/// this crate.
const ALLOWLIST: &[(&str, usize)] = &[];

const BANNED_PATTERNS: &[&str] = &[
    "fn dot(",
    "fn cross(",
    "fn norm(",
    "fn normalize(",
    "fn sub(",
    "fn subtract(",
    ".mul_add(",
];

fn rust_files_under(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("read_dir({}) failed: {err}", dir.display()));
    for entry in entries {
        let entry = entry.expect("dir entry readable");
        let path = entry.path();
        if path.is_dir() {
            rust_files_under(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_private_vector_algebra_in_src() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files_under(&src_dir, &mut files);

    // The xargs-incident rule: a scan that touches zero files must fail,
    // never silently pass as green.
    assert!(
        !files.is_empty(),
        "scanned 0 files under {} — the scan itself is broken, not proof the crate is clean",
        src_dir.display()
    );

    let mut violations = Vec::new();
    for file in &files {
        let contents = std::fs::read_to_string(file)
            .unwrap_or_else(|err| panic!("read {} failed: {err}", file.display()));
        for (line_index, line) in contents.lines().enumerate() {
            let line_no = line_index + 1;
            for pattern in BANNED_PATTERNS {
                if !line.contains(pattern) {
                    continue;
                }
                let rel = file
                    .strip_prefix(Path::new(env!("CARGO_MANIFEST_DIR")))
                    .unwrap_or(file);
                let rel_str = rel.to_string_lossy().to_string();
                if ALLOWLIST.contains(&(rel_str.as_str(), line_no)) {
                    continue;
                }
                violations.push(format!(
                    "{rel_str}:{line_no}: matches banned pattern `{pattern}` — \
                     consolidate to geom_kernel::vec3 (docs/specs/geometry-canon.md §1 OP-9) \
                     or add it to ALLOWLIST here with a canon-section citation"
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "private vector-algebra sites found in plinth-emit/src (scanned {} files):\n{}",
        files.len(),
        violations.join("\n")
    );
}
