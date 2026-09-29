//! Guard 1 (standing guard, not a one-off census check): the import rule.
//!
//! > Every public module in the geometry kernel must have at least one
//! > consumer outside the kernel.
//!
//! Zero consumers means either nobody needs the surface (delete it) or
//! somebody could not reach it and wrote their own copy instead (fix it).
//! Both are findings worth failing the build on. This is not hypothetical:
//! it is exactly how OP-1/OP-2/OP-6/OP-7/OP-9
//! (`docs/plans/geometry-consolidation.md`) came to exist. `vec3` and `par`
//! were made public in `dc2e0d0` (2026-08-26) "to gate the testkit feature",
//! announced nowhere, with no follow-up task -- so the opening changed what
//! was POSSIBLE and nothing about what was DONE. Twenty commits later,
//! `grep -rn 'geom_kernel::vec3'` over `crates/` returned exactly one hit,
//! and it was a doc comment complaining about the gap. `moller` and
//! `closest` fared worse: never public at all in the crate's history, so
//! `plinth-mesh::triage` and `plinth-hollow::drilling` could not have
//! reached them no matter how disciplined anyone was.
//!
//! **Measured before the geometry-consolidation sweep landed** (the census,
//! `docs/plans/geometry-consolidation.md` §2): `par` 0 importers outside the
//! kernel, `moller`'s ray/triangle primitive 0, `vec3` 1 of 12 functions
//! reached (a doc comment, not code), `poly2` did not exist yet. This test
//! pins the post-consolidation state -- every one of those now non-zero --
//! so the fix cannot silently come undone: a future edit that deletes the
//! last consumer of a kernel module, or reintroduces a private copy instead
//! of reaching for the public one, fails right here instead of drifting back
//! to the state the census found.
//!
//! # Methodology
//!
//! A plain substring scan over every `.rs` file in `crates/*/src` and
//! `rust/chitubox_io/src` -- the workspace's production code OUTSIDE this
//! crate. `tools/plinth-audit` is never read at all: it is excluded from the
//! Cargo workspace by design and `docs/plans/geometry-consolidation.md` §5.1
//! rules it out of scope permanently (its independently-duplicated vector
//! maths is the entire reason its verdicts are evidence about the kernel
//! rather than about themselves).
//!
//! A file counts as a consumer of a surface if it contains any of that
//! surface's needle strings. Bare (unqualified) needles such as
//! `current_threads()` are checked only against files that also mention
//! `geom_kernel` somewhere, so a same-named function in an unrelated crate
//! cannot be mistaken for a kernel consumer -- verified now, and re-verified
//! by this file's own guard-fires self-test, that no such collision exists
//! in this tree (`fn current_threads(`, `fn run_in_pool(`, `fn ray_triangle(`,
//! `fn segment_segment_distance(` -- exact name, not a bare prefix -- all
//! return zero hits outside `rust/geom_kernel` at the time this guard was
//! written; `triage.rs`'s own `fn ray_triangle_distance(` is a DIFFERENT,
//! longer name and is triage's call-site policy wrapper around the kernel
//! primitive, not a competing definition of it -- a real false positive the
//! collision self-test below caught with the bare-prefix form before this
//! fix).
//!
//! Gated on the project's own xargs-incident rule
//! (`docs/plans/geometry-consolidation.md`, "gate on counts, never on exit
//! codes"): the file walk asserts a non-zero file count independently of
//! whether any surface matched, so a walker that silently found nothing
//! cannot be mistaken for a clean result.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// One piece of the kernel's public surface and the evidence that counts as
/// "reached" for it.
struct Surface {
    /// Human-readable label, used only in failure output.
    label: &'static str,
    /// A file is a consumer if it contains ANY of these substrings.
    needles: &'static [&'static str],
    /// If true, a needle only counts when the file ALSO contains the literal
    /// substring "geom_kernel" somewhere -- needed for needles that are not
    /// themselves qualified paths (e.g. a bare re-exported function name).
    require_geom_kernel_mention: bool,
}

/// The kernel's public modules (`pub mod ...` in `rust/geom_kernel/src/lib.rs`)
/// plus the individually re-exported primitives the census and the canon
/// name by their own operation, not their (private) module
/// (`docs/specs/geometry-canon.md` §1 OP-1, OP-2). Every entry here is a
/// GATING surface: zero consumers fails the test.
const GATING_SURFACES: &[Surface] = &[
    Surface {
        label: "geom_kernel::vec3 (pub mod vec3 -- OP-6/OP-7/OP-9 survivor)",
        needles: &["geom_kernel::vec3"],
        require_geom_kernel_mention: false,
    },
    Surface {
        label: "geom_kernel::poly2 (pub mod poly2 -- OP-3/OP-8 survivor)",
        needles: &["geom_kernel::poly2"],
        require_geom_kernel_mention: false,
    },
    Surface {
        label: "geom_kernel::par (pub mod par, reached only through its \
                 re-exports current_threads/run_in_pool -- nobody spells the \
                 module path itself, which is fine: the re-export IS the \
                 public surface)",
        needles: &[
            "geom_kernel::current_threads",
            "geom_kernel::run_in_pool",
            "current_threads()",
            "run_in_pool(",
        ],
        require_geom_kernel_mention: true,
    },
    Surface {
        label: "geom_kernel::testkit (pub mod testkit, feature-gated)",
        needles: &["geom_kernel::testkit", "testkit::"],
        require_geom_kernel_mention: true,
    },
    Surface {
        label: "ray/triangle (moller::moller, re-exported as ray_triangle -- \
                 OP-2 survivor; moller itself stays a private module, only \
                 the primitive is public)",
        needles: &["geom_kernel::ray_triangle", "ray_triangle("],
        require_geom_kernel_mention: true,
    },
    Surface {
        label: "segment/segment distance (closest::segseg, re-exported as \
                 segment_segment_distance -- OP-1 survivor; closest itself \
                 stays private)",
        needles: &[
            "geom_kernel::segment_segment_distance",
            "segment_segment_distance(",
        ],
        require_geom_kernel_mention: true,
    },
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

fn workspace_root() -> PathBuf {
    // This file lives at `rust/geom_kernel/tests/`, so the workspace root
    // (the directory holding the top-level `Cargo.toml`) is two levels above
    // `CARGO_MANIFEST_DIR` (`rust/geom_kernel`).
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("rust/geom_kernel has a workspace root two levels up")
        .to_path_buf()
}

/// Every `.rs` file that is part of the workspace's production surface
/// OUTSIDE this crate: `crates/*/src` and `rust/chitubox_io/src`.
///
/// Deliberately excludes `crates/*/tests`, `crates/*/examples`, and
/// `crates/*/benches`: this guard asks whether the kernel's PUBLIC API has a
/// real caller, and a caller in test/example code still counts as a real
/// caller for that question (unlike Guard 2, which is about whether
/// PRODUCTION code re-implements geometry -- a different question with a
/// different scope, `docs/specs/geometry-canon.md` §2 step 3). Restricting
/// to `src` here is a choice to keep the "consumer" count meaningful (a
/// crate importing a symbol only to exercise it in one throwaway test would
/// otherwise look identical to a crate depending on it for real work); `src`
/// is where that distinction holds.
///
/// Never reads `tools/plinth-audit` (excluded from the Cargo workspace, and
/// permanently out of scope: `docs/plans/geometry-consolidation.md` §5.1)
/// or `rust/geom_kernel` itself (consuming your own public API is not
/// evidence of anything).
fn outside_kernel_src_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let crates_dir = root.join("crates");
    let mut crate_dirs: Vec<PathBuf> = fs::read_dir(&crates_dir)
        .unwrap_or_else(|error| panic!("failed to read {crates_dir:?}: {error}"))
        .map(|entry| entry.expect("readable crates/ entry").path())
        .filter(|path| path.is_dir())
        .collect();
    crate_dirs.sort();
    for crate_dir in crate_dirs {
        let src = crate_dir.join("src");
        if src.is_dir() {
            collect_rs_files(&src, &mut files);
        }
    }
    let chitubox_src = root.join("rust").join("chitubox_io").join("src");
    if chitubox_src.is_dir() {
        collect_rs_files(&chitubox_src, &mut files);
    }
    files
}

/// Files (already loaded) counted as consumers of `surface`.
fn consumers<'a>(contents: &'a [(PathBuf, String)], surface: &Surface) -> Vec<&'a Path> {
    contents
        .iter()
        .filter(|(_, text)| {
            let mentions_geom_kernel = text.contains("geom_kernel");
            if surface.require_geom_kernel_mention && !mentions_geom_kernel {
                return false;
            }
            surface.needles.iter().any(|needle| text.contains(needle))
        })
        .map(|(path, _)| path.as_path())
        .collect()
}

#[test]
fn every_public_kernel_surface_has_a_consumer_outside_the_kernel() {
    let root = workspace_root();
    let files = outside_kernel_src_files(&root);

    // The xargs-incident gate: independent of anything below, a walk that
    // touched zero files is a broken walker, not evidence the kernel is
    // unused.
    assert!(
        files.len() > 50,
        "scanned only {} outside-kernel source files under {:?} -- the walker \
         is almost certainly broken (this workspace has 12 crates with far \
         more than 50 source files combined), not evidence of anything about \
         the kernel's consumers",
        files.len(),
        root,
    );

    let contents: Vec<(PathBuf, String)> = files
        .into_iter()
        .map(|path| {
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {path:?}: {error}"));
            (path, text)
        })
        .collect();

    let mut failures = Vec::new();
    let mut report = String::new();
    for surface in GATING_SURFACES {
        let hits = consumers(&contents, surface);
        report.push_str(&format!(
            "  {} consumer file(s): {}\n",
            hits.len(),
            surface.label
        ));
        for path in &hits {
            report.push_str(&format!("      - {}\n", path.display()));
        }
        if hits.is_empty() {
            failures.push(surface.label);
        }
    }

    // Informational only, not gated: plinth-core::chord is a measured
    // "zero consumers" finding from the same census family (`mod chord;` is
    // private in `crates/plinth-core/src/lib.rs`, so by construction it has
    // no callers outside its own crate -- confirmed, `grep -rn
    // 'plinth_core::chord|use plinth_core::\{[^}]*chord'` over every other
    // crate returns nothing). It is NOT part of the geometry kernel and its
    // fix is not this sweep's C1..C8 (`docs/plans/geometry-consolidation.md`),
    // so this guard reports it rather than failing on it -- surfacing the
    // same class of finding without expanding this guard's scope past what
    // its own rule says ("every public module in the GEOMETRY KERNEL").
    let chord_mentioned_outside = contents.iter().any(|(path, text)| {
        !path.starts_with(root.join("crates").join("plinth-core"))
            && (text.contains("plinth_core::chord") || text.contains("plinth-core::chord"))
    });
    report.push_str(&format!(
        "  [informational, not gated] plinth-core::chord external callers: {}\n",
        usize::from(chord_mentioned_outside)
    ));

    println!("{report}");

    assert!(
        failures.is_empty(),
        "the following kernel surfaces have ZERO consumers outside \
         `rust/geom_kernel` -- either nobody needs them (delete) or someone \
         reimplemented the operation instead of reaching for the public one \
         (fix; this is exactly how OP-1/OP-2/OP-9 came to exist per \
         docs/plans/geometry-consolidation.md):\n{}\n\nfull report:\n{report}",
        failures.join("\n"),
    );
}

/// Proof the detector above can actually FAIL, run every time this suite
/// runs rather than demonstrated once and thrown away
/// (`docs/plans/geometry-consolidation.md`: "a green suite that never called
/// the new function is the default outcome, not the unlikely one"; the
/// project's own xargs incident is the same lesson at a different layer).
///
/// This does not touch any crate's source -- it exercises the exact same
/// `consumers()` function the real guard above uses, against the exact same
/// file contents, but asks about a surface that does not exist. If this
/// ever reports a "consumer", the detector itself is broken (a needle this
/// generic matched something), not the kernel.
#[test]
fn the_detector_reports_zero_for_a_surface_that_does_not_exist() {
    let root = workspace_root();
    let files = outside_kernel_src_files(&root);
    assert!(
        !files.is_empty(),
        "same walker as the real guard; must not be empty here either"
    );
    let contents: Vec<(PathBuf, String)> = files
        .into_iter()
        .map(|path| {
            let text = fs::read_to_string(&path).expect("readable source file");
            (path, text)
        })
        .collect();

    let nonexistent = Surface {
        label: "a made-up symbol nothing imports",
        needles: &["geom_kernel::this_symbol_does_not_exist_kn_import_rule_guard_v1"],
        require_geom_kernel_mention: false,
    };
    let hits = consumers(&contents, &nonexistent);
    assert!(
        hits.is_empty(),
        "the detector found a 'consumer' of a symbol that does not exist \
         anywhere in the tree ({:?}) -- the needle-matching logic itself is \
         broken, which means the real guard above cannot be trusted either",
        hits,
    );

    // And the mirror image, on a surface known to be real and consumed, so
    // this test is not merely proving "empty needle list -> empty result".
    let real = Surface {
        label: "geom_kernel::vec3",
        needles: &["geom_kernel::vec3"],
        require_geom_kernel_mention: false,
    };
    let real_hits = consumers(&contents, &real);
    assert!(
        !real_hits.is_empty(),
        "the detector found zero consumers of geom_kernel::vec3, which is \
         imported in dozens of files -- the needle-matching logic is broken \
         in the other direction (it cannot find a match that is really \
         there), which means a genuine zero-consumer regression could be \
         hidden by the same bug",
    );
}

/// The `require_geom_kernel_mention` collision guard, verified: none of the
/// bare (unqualified) needles used above collide with a same-named item
/// defined OUTSIDE `rust/geom_kernel`. If this ever fails, a bare needle in
/// `GATING_SURFACES` is no longer safe to use unqualified and must gain (or
/// keep) `require_geom_kernel_mention: true`, or be replaced with a fully
/// qualified path.
#[test]
fn bare_surface_needles_do_not_collide_with_unrelated_definitions() {
    let root = workspace_root();
    let files = outside_kernel_src_files(&root);
    assert!(!files.is_empty());

    // Exact function-name boundary (the opening paren), not a bare prefix:
    // `fn ray_triangle` alone also matches the retired-in-spirit but still
    // present `fn ray_triangle_distance(` in `triage.rs` (that function is
    // triage's CALL-SITE POLICY WRAPPER around the kernel's `ray_triangle`,
    // not a competing definition of it -- see its own doc comment, canon §1
    // OP-2) -- a real false positive caught while building this guard.
    let collision_patterns = [
        "fn current_threads(",
        "fn run_in_pool(",
        "fn ray_triangle(",
        "fn segment_segment_distance(",
    ];
    let mut collisions = Vec::new();
    for path in &files {
        let text = fs::read_to_string(path).expect("readable source file");
        for pattern in collision_patterns {
            if text.contains(pattern) {
                collisions.push(format!("{}: {pattern}", path.display()));
            }
        }
    }
    assert!(
        collisions.is_empty(),
        "a surface needle used unqualified above (current_threads/run_in_pool/\
         ray_triangle/segment_segment_distance) is ALSO independently defined \
         outside the kernel, so counting its bare name as a kernel-consumer \
         signal is unsound there:\n{}",
        collisions.join("\n"),
    );
}
