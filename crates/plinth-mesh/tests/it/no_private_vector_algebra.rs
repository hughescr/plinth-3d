//! `plinth-mesh` may not grow a private copy of the shared vector algebra.
//!
//! Mandated by `docs/specs/geometry-canon.md` §2 ("Enforcement"). The crate's
//! duplicated geometry was retired under plan items C5 (ray vs triangle),
//! C8a (the byte-neutral `vec3` copies) and the `hypot` sites of OP-6; this
//! test is what stops it coming back one helper at a time, the way it arrived
//! the first time (`docs/plans/geometry-consolidation.md` §"Cause B": the
//! copies predate the crate's kernel dependency).
//!
//! Two rules, and the second is the one that matters:
//!
//! 1. No banned spelling may appear in `src/` outside the named allowlist.
//! 2. **The scan must prove it scanned.** A matcher that discovers no files
//!    must FAIL, not pass. This crate's own history has an audit fixture
//!    harness that reported GREEN having found zero fixtures, and a batch
//!    script that processed zero files on BSD `xargs` and still wrote its
//!    completion marker. Every gate here is a counted row.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Spellings that mean "someone re-implemented the kernel here".
///
/// `fn norm(`/`fn dot(` etc. catch a new private copy; `.mul_add(` catches an
/// FMA, which the kernel forbids because its operand order is pinned to the
/// retired oracle (`rust/geom_kernel/src/vec3.rs`, spec sec 5.1); `.hypot(`
/// catches the retired third length formula (canon §1 OP-6).
const BANNED: [&str; 9] = [
    "fn dot(",
    "fn cross(",
    "fn norm(",
    "fn normalize(",
    "fn sub(",
    "fn subtract(",
    "fn length(",
    ".mul_add(",
    ".hypot(",
];

/// Every permitted occurrence, as `(file, line, spelling, canon citation)`.
///
/// Empty, deliberately: after C5/C8a this crate has no surviving private
/// vector algebra at all. An entry added here must cite the section of
/// `docs/specs/geometry-canon.md` that permits it.
const ALLOWLIST: [(&str, u32, &str, &str); 0] = [];

#[test]
fn the_crate_has_no_private_vector_algebra_outside_its_allowlist() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = rust_files(&src);

    // Gate 1: the scan found files. A zero-file scan is a broken instrument,
    // not a clean crate.
    assert!(
        files.len() >= 8,
        "scanned {} files under {}; this crate has at least 8 modules, so a scan finding fewer \
         has not scanned the crate and must not report success",
        files.len(),
        src.display()
    );

    // Gate 2: the matcher works. If none of the banned spellings can be found
    // anywhere, the matcher itself is broken and rule 1 would pass vacuously.
    // `geom_kernel`'s own `vec3.rs` is the positive control: it is where these
    // functions legitimately live.
    let kernel_vec3 = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../rust/geom_kernel/src/vec3.rs")
        .canonicalize()
        .expect("the kernel's vec3.rs is the positive control for this matcher");
    let control = fs::read_to_string(&kernel_vec3).expect("read the positive control");
    let control_hits = BANNED
        .iter()
        .filter(|needle| control.contains(*needle))
        .count();
    assert!(
        control_hits >= 4,
        "the matcher found only {control_hits} of its banned spellings in {}, which is where \
         they all legitimately live; the matcher is broken and rule 1 below would pass \
         vacuously",
        kernel_vec3.display()
    );

    let permitted = ALLOWLIST
        .iter()
        .map(|(file, line, spelling, _)| (*file, *line, *spelling))
        .collect::<BTreeSet<_>>();

    let mut violations = Vec::new();
    let mut lines_scanned = 0_usize;
    for path in &files {
        let relative = path
            .strip_prefix(&src)
            .expect("every scanned file is under src")
            .to_string_lossy()
            .replace('\\', "/");
        let text = fs::read_to_string(path).expect("read a source file");
        for (index, line) in text.lines().enumerate() {
            lines_scanned += 1;
            let number = u32::try_from(index + 1).expect("source files are not 4-billion lines");
            for needle in BANNED {
                if line.contains(needle)
                    && !permitted.contains(&(relative.as_str(), number, needle))
                {
                    violations.push(format!("{relative}:{number}: {needle}  |  {}", line.trim()));
                }
            }
        }
    }

    // Gate 3: the scan read content, not empty files.
    assert!(
        lines_scanned > 1_000,
        "scanned only {lines_scanned} lines across {} files; that is not this crate",
        files.len()
    );

    assert!(
        violations.is_empty(),
        "private vector algebra found in plinth-mesh/src. Call `geom_kernel::vec3` (or \
         `geom_kernel::ray_triangle`) instead; if a site genuinely must stay, add it to \
         ALLOWLIST in this file with the section of docs/specs/geometry-canon.md that permits \
         it. Findings:\n{}",
        violations.join("\n")
    );
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let entries =
        fs::read_dir(dir).unwrap_or_else(|error| panic!("cannot read {}: {error}", dir.display()));
    for entry in entries {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            found.extend(rust_files(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
    found.sort();
    found
}
