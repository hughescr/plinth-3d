//! Enforcement test for `docs/specs/geometry-canon.md` §2: this crate must
//! not grow a private re-implementation of the kernel's vector or 2D-polygon
//! primitives. `geom_kernel::vec3` and `geom_kernel::poly2` are the
//! consolidated homes (canon §1 OP-3, OP-8, OP-9); a private `fn dot(` /
//! `fn cross(` / `fn norm(` / `fn normalize(` / `fn sub(` / `fn subtract(` /
//! a `.mul_add(` fusion appearing in this crate's `src/` is either an
//! un-consolidated duplicate or a named, cited exemption -- never silent.
//!
//! Gated on the scanned-file count being greater than zero (the
//! `xargs -d`-on-BSD incident rule, `docs/specs/geometry-canon.md`
//! "the failure mode this project reproduces"): a matcher that silently
//! walks zero files must fail loudly, not report green.

use std::fs;
use std::path::{Path, PathBuf};

/// Named, cited exceptions. Empty today: `plinth-layers` has no private
/// vector/polygon algebra left after the C2 consolidation (this crate's
/// `signed_area` and the `tests/common` `point_in_polygon` helper both now
/// delegate to `geom_kernel::poly2`).
const ALLOWLIST: &[(&str, &str)] = &[];

const FORBIDDEN: &[&str] = &[
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
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn src_has_no_private_vector_algebra_outside_the_allowlist() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_dir, &mut files);

    // Gate on a counted row, not an exit code: a matcher that scanned
    // nothing must fail, not pass.
    assert!(
        !files.is_empty(),
        "scanned zero .rs files under {} -- the walk is broken, not the crate clean",
        src_dir.display()
    );

    let mut violations = Vec::new();
    for file in &files {
        let contents = fs::read_to_string(file)
            .unwrap_or_else(|error| panic!("read {} failed: {error}", file.display()));
        let file_name = file
            .strip_prefix(&src_dir)
            .unwrap_or(file)
            .to_string_lossy()
            .into_owned();
        for (line_number, line) in contents.lines().enumerate() {
            for pattern in FORBIDDEN {
                if line.contains(pattern) {
                    let allowed = ALLOWLIST.iter().any(|(allowed_file, allowed_pattern)| {
                        *allowed_file == file_name && *allowed_pattern == *pattern
                    });
                    if !allowed {
                        violations.push(format!(
                            "{file_name}:{}: `{pattern}` (not in ALLOWLIST)",
                            line_number + 1
                        ));
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "private vector/polygon algebra found outside the allowlist (scanned {} files):\n{}",
        files.len(),
        violations.join("\n")
    );
}
