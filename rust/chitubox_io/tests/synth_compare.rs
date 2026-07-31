//! Synthesized-vs-reference (§7.2): build sphere-cylinder from an empty
//! project + Sphere.stl + Cylinder.stl and compare against the owner's
//! sphere-cylinder.chitubox modulo known-variable regions. The load-bearing
//! claim: STL decode + identity-centering reproduces ChituBox's embedded mesh
//! as a bit-identical sorted vertex set.

mod common;
use chitubox_io::{ModelGeometry, ModelSource, Placement, Project};
use common::*;
use std::path::PathBuf;

fn stl(name: &str) -> Option<PathBuf> {
    corpus_path(name)
}

#[test]
fn build_from_scratch_matches_reference_geometry() {
    let (Some(sphere), Some(cylinder), Some(reference)) = (
        stl("Sphere.stl"),
        stl("Cylinder.stl"),
        read_corpus("sphere-cylinder.chitubox"),
    ) else {
        return;
    };
    let refproj = Project::from_bytes(reference).unwrap();

    let mut proj = Project::new_empty();
    let idx = proj
        .add_models(&[
            ModelSource {
                name: "Sphere.stl".into(),
                geometry: ModelGeometry::StlPath(sphere),
                placement: Placement {
                    x: 0.0,
                    y: 0.0,
                    z_lift: 0.0,
                },
            },
            ModelSource {
                name: "Cylinder.stl".into(),
                geometry: ModelGeometry::StlPath(cylinder),
                // reference cylinder placement
                placement: Placement {
                    x: 43.7035,
                    y: 7.1448,
                    z_lift: 0.0,
                },
            },
        ])
        .unwrap();
    assert_eq!(idx, vec![0, 1]);
    assert_eq!(proj.model_count(), 2);

    // tri counts match the reference
    assert_eq!(proj.model(0).unwrap().triangle_count, 4760);
    assert_eq!(proj.model(1).unwrap().triangle_count, 244);
    assert_eq!(
        proj.model(0).unwrap().triangle_count,
        refproj.model(0).unwrap().triangle_count
    );

    // geometry equals the reference as a bit-identical sorted vertex set
    // (import reorders triangles; order must NOT be asserted)
    for m in 0..2 {
        let mine = proj.model_triangles(m).unwrap();
        let theirs = refproj.model_triangles(m).unwrap();
        let diff = sorted_set_max_diff(&mine, &theirs);
        assert_eq!(
            diff, 0.0,
            "model {m} mesh sorted-set diff {diff} (must be bit-exact)"
        );
    }

    // the synthesized file passes structural verify + re-parses to itself
    let bytes = proj.to_bytes().expect("synth to_bytes verifies");
    let re = Project::from_bytes(bytes).unwrap();
    assert_eq!(re.model_count(), 2);
    // placement survives within ChituBox's own plate jitter tolerance (±0.02)
    let p = re.placements()[1];
    assert!((p.x - 43.7035).abs() < 0.02 && (p.y - 7.1448).abs() < 0.02);
}

#[test]
fn triangles_input_path_centers_like_stl() {
    // Feeding the same geometry as raw Triangles must center identically.
    let Some(sphere) = stl("Sphere.stl") else {
        return;
    };
    let via_stl = {
        let mut p = Project::new_empty();
        p.add_model(ModelSource {
            name: "s".into(),
            geometry: ModelGeometry::StlPath(sphere.clone()),
            placement: Placement::default(),
        })
        .unwrap();
        p.model_triangles(0).unwrap()
    };
    let via_tris = {
        let mut p = Project::new_empty();
        p.add_model(ModelSource {
            name: "s".into(),
            geometry: ModelGeometry::Triangles(chitubox_io::TriangleSoup::new(via_stl.clone())),
            placement: Placement::default(),
        })
        .unwrap();
        p.model_triangles(0).unwrap()
    };
    // via_stl is already centered; centering again is a no-op → identical
    assert_eq!(via_stl, via_tris);
}
