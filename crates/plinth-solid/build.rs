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

fn main() {
    let upstream = "vendor/manifold/upstream";
    let source = format!("{upstream}/src");

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
