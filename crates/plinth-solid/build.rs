const MANIFOLD_TRANSLATION_UNITS: [&str; 18] = [
    "boolean3.cpp",
    "boolean_result.cpp",
    "constructors.cpp",
    "csg_tree.cpp",
    "edge_op.cpp",
    "execution_impl.cpp",
    "face_op.cpp",
    "impl.cpp",
    "manifold.cpp",
    "minkowski.cpp",
    "polygon.cpp",
    "properties.cpp",
    "quickhull.cpp",
    "sdf.cpp",
    "smoothing.cpp",
    "sort.cpp",
    "subdivision.cpp",
    "tree2d.cpp",
];

/// Verify every vendored Manifold file named in `vendor/manifold/FILES.sha256`
/// against its recorded sha256 digest, failing the build on any mismatch or
/// missing file.
///
/// Before this, `FILES.sha256` was read only by
/// `cargo:rerun-if-changed=vendor/manifold/FILES.sha256`: nothing ever opened
/// it, hashed a vendored file, or compared the two, so it looked like a
/// supply-chain checksum manifest while doing nothing a manifest is for —
/// editing a vendored source triggered a rebuild and the rebuild always
/// passed, checksum or no checksum. This makes it a real precondition: the
/// vendored tree named in the manifest must match the manifest, checked
/// before a single translation unit compiles.
fn verify_vendor_checksums(vendor_root: &std::path::Path) {
    use sha2::{Digest, Sha256};

    let manifest_path = vendor_root.join("FILES.sha256");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap_or_else(|error| {
        panic!(
            "plinth-solid build.rs: cannot read vendor checksum manifest {}: {error}",
            manifest_path.display()
        )
    });

    let mut checked = 0usize;
    for (zero_based_line, line) in manifest.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        // sha256sum/shasum format: a 64 hex-digit digest, two spaces, then the
        // path relative to `vendor_root`.
        let Some((digest_hex, relative_path)) = line.split_once("  ") else {
            panic!(
                "plinth-solid build.rs: {}:{}: expected `<sha256-hex>  <path>`, got {line:?}",
                manifest_path.display(),
                zero_based_line + 1,
            );
        };
        let file_path = vendor_root.join(relative_path);
        let bytes = std::fs::read(&file_path).unwrap_or_else(|error| {
            panic!(
                "plinth-solid build.rs: vendored file listed in {} is missing or unreadable: {} \
                 ({error})",
                manifest_path.display(),
                file_path.display()
            )
        });
        let computed_hex = format!("{:x}", Sha256::digest(&bytes));
        assert!(
            computed_hex.eq_ignore_ascii_case(digest_hex.trim()),
            "plinth-solid build.rs: checksum mismatch for vendored file {}: {} recorded \
             {digest_hex}, computed {computed_hex}. The vendored Manifold tree was edited \
             without updating FILES.sha256, or the manifest is stale -- regenerate it from \
             the tree you intend to vendor, do not just patch the mismatched line.",
            file_path.display(),
            manifest_path.display(),
        );
        checked += 1;
    }
    assert!(
        checked > 0,
        "plinth-solid build.rs: {} named zero files -- an empty manifest verifies nothing",
        manifest_path.display()
    );
}

fn main() {
    let upstream = "vendor/manifold/upstream";
    let source = format!("{upstream}/src");

    verify_vendor_checksums(std::path::Path::new("vendor/manifold"));

    let mut build = cxx_build::bridge("src/ffi.rs");
    build
        .file("src/manifold_bridge.cc")
        .include(".")
        .include(format!("{upstream}/include"))
        .include(&source)
        .std("c++17")
        .define("MANIFOLD_PAR", "-1")
        .flag_if_supported("-ffp-contract=off")
        .flag_if_supported("-fexcess-precision=standard")
        // cxx-build enables warnings that the pristine upstream CMake build
        // suppresses. Keep the vendor subtree byte-identical and match that
        // policy at the compiler boundary rather than patching third-party code.
        .flag_if_supported("-Wno-missing-field-initializers")
        .flag_if_supported("-Wno-unused-const-variable")
        .flag_if_supported("-Wno-unused-lambda-capture")
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-variable")
        .flag_if_supported("-Wno-sign-compare")
        .flag_if_supported("/fp:precise")
        .flag_if_supported("/EHsc")
        .flag_if_supported("/bigobj")
        .flag_if_supported("/DNOMINMAX");

    // This allowlist is deliberately explicit. Cross-section sources and their
    // Clipper2 dependency are not part of the M0 boolean-solid backend.
    for unit in MANIFOLD_TRANSLATION_UNITS {
        build.file(format!("{source}/{unit}"));
    }
    build.compile("plinth_solid_manifold");

    println!("cargo:rerun-if-changed=src/ffi.rs");
    println!("cargo:rerun-if-changed=src/manifold_bridge.h");
    println!("cargo:rerun-if-changed=src/manifold_bridge.cc");
    println!("cargo:rerun-if-changed=vendor/manifold/FILES.sha256");
    println!("cargo:rerun-if-changed=vendor/manifold/upstream/include");
    println!("cargo:rerun-if-changed=vendor/manifold/upstream/src");
}
