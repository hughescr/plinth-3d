//! Enforcement test for `docs/specs/geometry-canon.md` §2 ("Enforcement").
//!
//! `crates/plinth-solid/src/` must not grow a private re-implementation of
//! the canonical vector-algebra primitives in `geom_kernel::vec3`. If a
//! future change adds one of the forbidden function shapes below, this test
//! fails with the offending file and line so the change is routed through
//! `geom_kernel::vec3` instead (or, if that is genuinely wrong for the site,
//! added to `ALLOWLIST` with a citation into the canon, per canon §2 step 1).
//!
//! This crate's `src/` currently has none: `plinth-solid` is a Manifold FFI
//! boolean-operations wrapper and does no in-house vector-algebra work. The
//! two structurally-identical `dot`/`cross` helpers in
//! `tests/solid_spike.rs` are a test-only volume-audit oracle, deliberately
//! independent of production code (canon §2 step 3, "in tests, leave it");
//! this grep-test therefore does not scan `tests/`.
//!
//! Gated on a scanned-file count, not on the match count being zero: a
//! matcher that silently walks zero files must fail loudly rather than pass
//! by accident (the project's recorded `xargs -d` incident).

use std::fs;
use std::path::{Path, PathBuf};

/// Sites this crate's `src/` is permitted to keep, each cited to the canon
/// section that rules it exempt. Empty today; add entries here (not by
/// deleting the checks below) if a future site earns an exemption.
const ALLOWLIST: &[(&str, u32)] = &[];

const FORBIDDEN_NEEDLES: &[&str] = &[
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
        .unwrap_or_else(|error| panic!("read_dir({}) failed: {error}", dir.display()));
    for entry in entries {
        let entry = entry.expect("directory entry must be readable");
        let path = entry.path();
        let file_type = entry.file_type().expect("file type must be readable");
        if file_type.is_dir() {
            collect_rs_files(&path, out);
        } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn src_has_no_private_vector_algebra() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_dir, &mut files);

    // The xargs-incident rule: a scan that silently touches nothing must
    // fail, not pass. plinth-solid's src/ has at least `lib.rs` and
    // `ffi.rs`.
    assert!(
        !files.is_empty(),
        "no_private_vector_algebra scanned zero files under {} -- the scan is broken, \
         this is not a pass",
        src_dir.display()
    );

    let mut violations = Vec::new();
    for path in &files {
        let contents = fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("read_to_string({}) failed: {error}", path.display()));
        for (line_number, line) in contents.lines().enumerate() {
            let line_number = u32::try_from(line_number + 1).expect("line number fits in u32");
            for needle in FORBIDDEN_NEEDLES {
                if !line.contains(needle) {
                    continue;
                }
                let relative = path
                    .strip_prefix(Path::new(env!("CARGO_MANIFEST_DIR")))
                    .unwrap_or(path);
                let relative_str = relative.to_string_lossy();
                if ALLOWLIST
                    .iter()
                    .any(|(file, line)| relative_str == *file && *line == line_number)
                {
                    continue;
                }
                violations.push(format!(
                    "{}:{line_number}: matches forbidden pattern `{needle}` -- use \
                     geom_kernel::vec3 (docs/specs/geometry-canon.md §1 OP-6/OP-9), or add a \
                     cited ALLOWLIST entry if this site is a genuine exemption",
                    relative.display()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "private vector-algebra sites found in plinth-solid/src (scanned {} files):\n{}",
        files.len(),
        violations.join("\n")
    );
}
