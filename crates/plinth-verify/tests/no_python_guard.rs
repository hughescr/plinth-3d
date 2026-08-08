//! Repository-wide guard: this project contains no Python, and no crate
//! declares a Python binding dependency.
//!
//! # Why this exists
//!
//! The owner's standing rule, recorded in
//! `docs/decisions/2026-07-28-rust-rewrite.md`, is that the decision core is
//! restarted in Rust and **no new Python is written here**. That rule is a
//! training-data guardrail as much as a performance one: Python-shaped
//! instincts (scalar loops, per-item calls in hot paths) leak into whatever
//! language is next, and the cheapest way to stop the leak is to remove the
//! substrate. The policy has always been elimination, not coexistence — each
//! Python surface deleted as soon as its native Rust boundary lands, with no
//! optional compatibility tail kept behind a feature flag.
//!
//! On **2026-08-08** the last of it went (export-path phase E8): the
//! `chitubox_io` PyO3 binding (`src/py.rs`, the `python` feature, the optional
//! `pyo3`/`numpy` dependencies, the maturin `pyproject.toml`, the mixed-layout
//! `python/` package, and the `build.rs` whose only consumer was that
//! binding), the `scripts/tests` pytest harness, the `scripts/tools` legacy
//! pipeline, the `scripts/support_placement` engine, and the venv that served
//! them. The parity knowledge those suites carried had already been ported:
//! `frozen_baseline`, `roundtrip`, `synth_compare` and the support-parity
//! tests are Rust-native and run against committed corpus and baseline
//! artifacts, which is why the deletion cost no coverage.
//!
//! This guard is what makes that state durable rather than merely current. It
//! is deliberately a **tree check, not a build check**: a build check would
//! only notice Python that something compiles or imports, and the failure mode
//! being guarded against is precisely a stray script that nothing builds —
//! a one-off analysis helper, a scaffolded venv, a "temporary" driver — quietly
//! taking root next to the Rust.
//!
//! # What is deliberately still allowed
//!
//! *Prose* may say "Python" freely, and does: the migration records, the
//! decision record above, and the frozen-baseline provenance strings in
//! `rust/chitubox_io/tests/frozen_baseline.rs` all name the retired engine and
//! `scripts/tools/prep_one.py` on purpose. Those strings are the birth
//! certificate of the frozen baseline artifacts — they record which tool at
//! which git revision produced them — and erasing them would destroy the
//! provenance the baselines are trusted on. Comments in `Cargo.toml` are
//! likewise exempt below, so this file's own history can stay searchable.
//!
//! `scripts/tests/fixtures/` is kept in full. It holds binary `.chitubox` and
//! `.stl` specimens plus frozen JSON, loaded today by Rust tests
//! (`rust/chitubox_io/tests/common/mod.rs`, `crates/plinth-verify/tests/
//! format_gate.rs`, `crates/plinth-emit/src/lib.rs`). Only the `.py` files
//! that used to sit beside them were removed.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Directory names never descended into, matched at any depth.
///
/// `.git` is history, `target` is build output, and `exports` is E-phase run
/// output — none of them is source, and all three can be large. Everything
/// else in the tree is fair game, **including ignored paths**: a `.venv` is
/// invisible to git but is still Python sitting in the checkout, and this
/// guard is meant to fail on exactly that.
const SKIPPED_DIRS: [&str; 3] = [".git", "target", "exports"];

/// Dependency names that would reintroduce a Python binding.
const BANNED_DEPS: [&str; 2] = ["pyo3", "numpy"];

/// The repository root, derived from this crate's manifest directory so the
/// guard works from any checkout path — a worktree under `/private/tmp`, a
/// CI scratch dir, or the definitive checkout. Never hardcode a location.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root must resolve from CARGO_MANIFEST_DIR")
}

/// One walk of the tree, collecting every file path (relative to the root).
///
/// Symlinks are recorded but never followed: `DirEntry::file_type` reports the
/// link itself, so `corpus/models/*.stl` (symlinked into place in scratch
/// worktrees) cannot lead the walk out of the repository.
fn walk(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .unwrap_or_else(|error| panic!("{} must be readable: {error}", dir.display()));
        for entry in entries {
            let entry = entry.expect("directory entry must be readable");
            let path = entry.path();
            let file_type = entry.file_type().expect("file type must be readable");
            if file_type.is_dir() {
                let name = entry.file_name();
                if SKIPPED_DIRS.iter().any(|skip| name == *skip) {
                    continue;
                }
                stack.push(path);
            } else {
                files.push(
                    path.strip_prefix(root)
                        .expect("walked path must be under the root")
                        .to_path_buf(),
                );
            }
        }
    }
    files
}

/// True when a `Cargo.toml` line declares one of the banned dependencies.
///
/// `#`-comments are stripped first, so prose explaining what was removed is
/// exempt while a live declaration is not — the same prose-exempt discipline
/// as `crates/plinth-support/tests/retired_formula_guard.rs`. Matches a bare
/// key (`pyo3 = ...`, `numpy = { ... }`), a dotted or table header form
/// (`[dependencies.pyo3]`), and a feature activation (`dep:pyo3`,
/// `pyo3/extension-module`).
fn declares_banned_dep(line: &str) -> Option<&'static str> {
    let code = match line.find('#') {
        Some(index) => &line[..index],
        None => line,
    };
    for dep in BANNED_DEPS {
        let bare_key = code
            .split('=')
            .next()
            .map(str::trim)
            .is_some_and(|key| key == dep);
        let table_header = code.trim().starts_with('[') && code.contains(&format!(".{dep}]"));
        let activation = code.contains(&format!("dep:{dep}"))
            || code.contains(&format!("\"{dep}/"))
            || code.contains(&format!("\"{dep}\""));
        if bare_key || table_header || activation {
            return Some(dep);
        }
    }
    None
}

#[test]
fn the_repository_contains_no_python_and_no_python_bindings() {
    let root = repo_root();
    let files = walk(&root);

    // --- The walk must actually have seen the repository -------------------
    //
    // A guard that silently walks an empty or wrong directory passes forever.
    // Anchor it on files that cannot disappear without this test being
    // rewritten anyway, and on a floor for the tree size.
    let seen: BTreeSet<&Path> = files.iter().map(PathBuf::as_path).collect();
    for anchor in [
        "Cargo.toml",
        "Cargo.lock",
        "README.md",
        "docs/decisions/2026-07-28-rust-rewrite.md",
        "rust/chitubox_io/src/lib.rs",
        "scripts/tests/fixtures/chitubox/two-supports.chitubox",
    ] {
        assert!(
            seen.contains(Path::new(anchor)),
            "the walk did not reach `{anchor}` from {}; the guard is looking at \
             the wrong tree and would pass vacuously",
            root.display()
        );
    }
    assert!(
        files.len() > 200,
        "the walk found only {} files under {}; that is far too few for this \
         repository, so the guard is not seeing what it claims to check",
        files.len(),
        root.display()
    );

    // --- The classifier must be able to see what it guards against ---------
    //
    // Prove the two detectors fire on synthetic inputs, so a refactor cannot
    // disarm them while leaving a green test behind.
    assert!(Path::new("scripts/tools/prep_one.py")
        .extension()
        .is_some_and(|ext| ext == "py"));
    assert_eq!(
        declares_banned_dep("numpy = { version = \"0.23\" }"),
        Some("numpy")
    );
    assert_eq!(declares_banned_dep("[dependencies.pyo3]"), Some("pyo3"));
    assert_eq!(declares_banned_dep("python = [\"dep:pyo3\"]"), Some("pyo3"));
    assert_eq!(
        declares_banned_dep("features = [\"pyo3/extension-module\"]"),
        Some("pyo3")
    );
    assert_eq!(
        declares_banned_dep("# maturin passes pyo3/extension-module"),
        None
    );
    assert_eq!(declares_banned_dep("thiserror = \"1\""), None);

    // --- Assertion 1: zero Python source files -----------------------------
    let python: Vec<String> = files
        .iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "py"))
        .map(|path| path.display().to_string())
        .collect();
    assert!(
        python.is_empty(),
        "Python has reappeared in the repository ({} file(s)).\n\
         The owner's standing rule (docs/decisions/2026-07-28-rust-rewrite.md) \
         is that no new Python is written here, and the last of it was deleted \
         on 2026-08-08. Write it in Rust; if you need a scratch tool, write a \
         Rust example or an integration test rather than a script.\n\
         Offending files:\n  {}",
        python.len(),
        python.join("\n  ")
    );

    // --- Assertion 2: no manifest declares a Python binding ----------------
    let mut offences = Vec::new();
    for path in files
        .iter()
        .filter(|path| path.file_name().is_some_and(|name| name == "Cargo.toml"))
    {
        let source = std::fs::read_to_string(root.join(path))
            .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()));
        for (number, line) in source.lines().enumerate() {
            if let Some(dep) = declares_banned_dep(line) {
                offences.push(format!(
                    "{}:{}: declares `{dep}`: {}",
                    path.display(),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(
        offences.is_empty(),
        "a Cargo manifest declares a Python binding dependency.\n\
         `pyo3` and `numpy` were removed from `rust/chitubox_io` on 2026-08-08 \
         along with the extension module they served; reintroducing either \
         brings back the interpreter this project deliberately does not have. \
         Comments naming them are exempt — a live declaration is not.\n\
         Offending lines:\n  {}",
        offences.join("\n  ")
    );
}
