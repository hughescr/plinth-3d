//! Acceptance-artifact generator — uses ONLY the public `chitubox_io` library.
//!
//! Given the campaign directory as argv[1], emits three `.chitubox` files for
//! the owner to open in ChituBox as the human gate, and re-parses each one
//! (the library's own structural verification) before declaring it landed.
//!
//!   (a) rebuilt-sphere-cylinder.chitubox — empty template + Sphere.stl +
//!       Cylinder.stl at the SAME placements as the owner's
//!       sphere-cylinder.chitubox.
//!   (b) packed-demo.chitubox — same two models at NEW positions: sphere at
//!       (-20,-15), cylinder at (+25,+10) rotated 30deg about Z.
//!   (c) supports-move-demo.chitubox — parse the owner's
//!       sphere-cylinder-supported.chitubox and translate the sphere AND its
//!       supports as one rigid assembly by (+15, 0).
//!
//! Run: `cargo run --example acceptance_artifacts -- <campaign_dir>`

use std::path::{Path, PathBuf};

use chitubox_io::{
    ModelGeometry, ModelInfo, ModelSource, Placement, PlacementUpdate, Project, ZRotation,
};

fn main() {
    if let Err(e) = run() {
        eprintln!("ERROR: {e}");
        std::process::exit(1);
    }
}

/// Classify a model as "sphere" or "cylinder" by name, else fall back to plate
/// order (index 0 -> sphere, 1 -> cylinder), matching the two-model corpus.
fn stl_for(m: &ModelInfo, base: &Path) -> PathBuf {
    let lname = m.name.to_ascii_lowercase();
    let file = if lname.contains("sphere") {
        "Sphere.stl"
    } else if lname.contains("cylinder") {
        "Cylinder.stl"
    } else if m.index == 0 {
        "Sphere.stl"
    } else {
        "Cylinder.stl"
    };
    base.join(file)
}

fn describe(tag: &str, p: &Project) {
    eprintln!("[{tag}] {} model(s):", p.model_count());
    for m in p.models() {
        eprintln!(
            "  idx {} name={:?} pos=({:.4},{:.4},{:.4}) tris={} supports={} ({} recs)",
            m.index,
            m.name,
            m.placement.x,
            m.placement.y,
            m.placement.z_lift,
            m.triangle_count,
            m.has_supports,
            m.support_record_count,
        );
    }
}

/// Re-open a just-written file through the full parser (the library's own
/// verification path) and report its model layout.
fn reverify(tag: &str, path: &Path) {
    let p = Project::open(path).unwrap_or_else(|e| panic!("re-parse of {path:?} failed: {e}"));
    describe(&format!("{tag}:reparse-OK"), &p);
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let base = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: acceptance_artifacts <campaign_dir>"),
    );

    // ---- (a) rebuilt-sphere-cylinder.chitubox ----------------------------
    let owner_sc = base.join("sphere-cylinder.chitubox");
    let src = Project::open(&owner_sc)?;
    describe("a:owner-source", &src);

    let mut rebuilt = Project::new_empty();
    let sources: Vec<ModelSource> = src
        .models()
        .iter()
        .map(|m| ModelSource {
            name: m.name.clone(),
            geometry: ModelGeometry::StlPath(stl_for(m, &base)),
            placement: m.placement,
        })
        .collect();
    rebuilt.add_models(&sources)?;
    let out_a = base.join("rebuilt-sphere-cylinder.chitubox");
    rebuilt.save(&out_a)?;
    reverify("a", &out_a);

    // ---- (b) packed-demo.chitubox ----------------------------------------
    // Same two models; sphere -> (-20,-15), cylinder -> (+25,+10) rot 30deg Z.
    let mut packed = Project::new_empty();
    // Rebuild the two models fresh from the STLs (library-only), preserving the
    // owner's per-model names and STL mapping, but at zero placement first.
    let base_sources: Vec<ModelSource> = src
        .models()
        .iter()
        .map(|m| ModelSource {
            name: m.name.clone(),
            geometry: ModelGeometry::StlPath(stl_for(m, &base)),
            placement: Placement { x: 0.0, y: 0.0, z_lift: 0.0 },
        })
        .collect();
    packed.add_models(&base_sources)?;

    // Resolve which index is sphere / cylinder by the same name rule.
    let idx_of = |kind: &str| -> usize {
        packed
            .models()
            .iter()
            .position(|m| {
                let l = m.name.to_ascii_lowercase();
                l.contains(kind)
            })
            .unwrap_or(if kind == "sphere" { 0 } else { 1 })
    };
    let sphere_i = idx_of("sphere");
    let cyl_i = idx_of("cylinder");

    packed.set_placements(&[
        PlacementUpdate {
            model_index: sphere_i,
            placement: Placement { x: -20.0, y: -15.0, z_lift: 0.0 },
        },
        PlacementUpdate {
            model_index: cyl_i,
            placement: Placement { x: 25.0, y: 10.0, z_lift: 0.0 },
        },
    ])?;
    // 30deg Z rotation applied to the cylinder (a sphere's Z-rotation is
    // geometrically invisible; the cylinder makes the rotation legible).
    packed.rotate_models_z(&[ZRotation { model_index: cyl_i, delta_deg: 30.0 }])?;
    let out_b = base.join("packed-demo.chitubox");
    packed.save(&out_b)?;
    reverify("b", &out_b);

    // ---- (c) supports-move-demo.chitubox ---------------------------------
    // Translate the sphere AND its supports as one rigid assembly by (+15,0).
    // The model record's translate positions the whole assembly (model mesh +
    // its support section share that frame), so a placement delta moves them
    // rigidly together.
    let owner_scs = base.join("sphere-cylinder-supported.chitubox");
    let mut supp = Project::open(&owner_scs)?;
    describe("c:owner-source", &supp);
    // Identify the sphere: prefer name, else the supported model.
    let sphere_c = supp
        .models()
        .iter()
        .position(|m| m.name.to_ascii_lowercase().contains("sphere"))
        .or_else(|| supp.models().iter().position(|m| m.has_supports))
        .expect("could not locate sphere model in supported source");
    let cur = supp.model(sphere_c)?.placement;
    supp.set_placement(
        sphere_c,
        Placement { x: cur.x + 15.0, y: cur.y, z_lift: cur.z_lift },
    )?;
    let out_c = base.join("supports-move-demo.chitubox");
    supp.save(&out_c)?;
    reverify("c", &out_c);

    eprintln!("\nDONE: wrote 3 artifacts to {}", base.display());
    Ok(())
}
