//! Locate the vendored, pinned upstream manifold and Clipper2 sources.
//!
//! Plinth vendors the exact C++ sources into the repository
//! (`vendor/manifold-src/`, `vendor/clipper2-src/`; pins recorded in
//! `vendor/PIN`). A build never runs git and never touches the network: this
//! module only checks that the directories are present and match the pin,
//! stages a copy of Clipper2 under `$OUT_DIR` (its CMake writes a generated
//! header into its own source tree, which must not be the committed copy), and
//! resets the cached cmake build directory when the source location changes.
//! Called from the host/emscripten build paths; the wasm32-unknown-unknown
//! path has its own FetchContent-based flow inside `super::wasm`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::MANIFOLD_VERSION;

/// Vendored source locations, relative to this crate's manifest directory.
const MANIFOLD_SRC_REL: &str = "../manifold-src";
const CLIPPER2_SRC_REL: &str = "../clipper2-src";
const PIN_REL: &str = "../PIN";

/// Source trees handed to cmake.
pub struct VendoredSources {
    /// Committed manifold tree (cmake only reads it).
    pub manifold: PathBuf,
    /// Copy of the committed Clipper2 tree under `$OUT_DIR`.
    pub clipper2: PathBuf,
}

fn require_file(path: &Path, what: &str) {
    assert!(
        path.is_file(),
        "manifold-csg-sys: vendored {what} is missing: {} not found. The C++ sources are \
         committed to the repository under vendor/manifold-src and vendor/clipper2-src \
         (see vendor/PIN); the build never downloads them. Restore the directories from \
         git.",
        path.display()
    );
}

fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Resolve and validate the vendored sources and stage Clipper2. Emits
/// `rerun-if-changed` for every vendored input so Cargo reruns this build
/// script (and so re-stages) only when they change.
pub fn locate(manifest_dir: &Path, out_dir: &Path) -> VendoredSources {
    let manifold = manifest_dir.join(MANIFOLD_SRC_REL);
    let clipper2_vendored = manifest_dir.join(CLIPPER2_SRC_REL);
    let pin = manifest_dir.join(PIN_REL);

    require_file(&manifold.join("CMakeLists.txt"), "manifold source tree");
    require_file(
        &clipper2_vendored.join("CPP").join("CMakeLists.txt"),
        "Clipper2 source tree",
    );
    require_file(&pin, "pin record");

    let pin_text = fs::read_to_string(&pin).unwrap_or_default();
    assert!(
        pin_text.contains(MANIFOLD_VERSION),
        "manifold-csg-sys: vendor/PIN does not record the manifold commit {MANIFOLD_VERSION} \
         that build.rs expects; update vendor/PIN and the vendored sources together."
    );

    println!("cargo:rerun-if-changed={}", manifold.display());
    println!("cargo:rerun-if-changed={}", clipper2_vendored.display());
    println!("cargo:rerun-if-changed={}", pin.display());

    let clipper2 = out_dir.join("clipper2-src");
    let _ = fs::remove_dir_all(&clipper2);
    copy_tree(&clipper2_vendored, &clipper2)
        .unwrap_or_else(|e| panic!("manifold-csg-sys: staging Clipper2 sources failed: {e}"));

    VendoredSources { manifold, clipper2 }
}

/// cmake refuses to reuse a build directory configured from a different
/// source directory (for example after the checkout moved). Record the source
/// path in a stamp and wipe the cached build directory when it changes.
pub fn reset_stale_build_dir(out_dir: &Path, build_dir: &Path, sources: &VendoredSources) {
    let stamp_file = out_dir.join(".source-stamp");
    let current = format!("{}\n", sources.manifold.display());
    let old = fs::read_to_string(&stamp_file).unwrap_or_default();
    if old != current {
        if build_dir.exists() {
            fs::remove_dir_all(build_dir).unwrap_or_else(|e| {
                panic!(
                    "manifold-csg-sys: cannot remove stale cmake build dir {}: {e}",
                    build_dir.display()
                )
            });
        }
        fs::write(&stamp_file, &current)
            .unwrap_or_else(|e| panic!("manifold-csg-sys: cannot write source stamp: {e}"));
    }
}
