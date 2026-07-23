//! Pure-Rust unit tests: closed-form geometry cases + BVH structural
//! properties (spec sec 6 items 8, R2). These exercise the geometry modules
//! directly (no Python), using the crate's public-in-crate API via a tiny
//! re-export shim in lib.rs test builds.

use geom_kernel::testkit::*;

fn tri(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> ([f64; 3], [f64; 3], [f64; 3]) {
    (a, b, c)
}

#[test]
fn closest_point_vertex_edge_face_regions() {
    // unit right triangle in z=0
    let (a, b, c) = tri([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    // point above the interior -> projects to interior, dist = height
    let cp = kern_closest_on_tri([0.2, 0.2, 1.0], a, b, c);
    assert!((cp[0] - 0.2).abs() < 1e-12 && (cp[1] - 0.2).abs() < 1e-12 && cp[2].abs() < 1e-12);
    // point beyond vertex A -> closest is A
    let cp = kern_closest_on_tri([-1.0, -1.0, 0.0], a, b, c);
    assert_eq!(cp, [0.0, 0.0, 0.0]);
    // point beyond edge AB midpoint -> on edge
    let cp = kern_closest_on_tri([0.5, -1.0, 0.0], a, b, c);
    assert!((cp[0] - 0.5).abs() < 1e-12 && cp[1].abs() < 1e-12);
}

#[test]
fn moller_hits_and_misses() {
    // triangle in z=0, ray from +z straight down through interior
    let v0 = [0.0, 0.0, 0.0];
    let e1 = [1.0, 0.0, 0.0];
    let e2 = [0.0, 1.0, 0.0];
    let (hit, t) = kern_moller([0.25, 0.25, 1.0], [0.0, 0.0, -1.0], v0, e1, e2, 2.0);
    assert!(hit);
    assert!((t - 1.0).abs() < 1e-12);
    // ray missing the triangle
    let (hit, _) = kern_moller([2.0, 2.0, 1.0], [0.0, 0.0, -1.0], v0, e1, e2, 2.0);
    assert!(!hit);
}

#[test]
fn bvh_nearest_matches_brute_force() {
    // a small soup: two boxes worth of triangles via a random-ish deterministic set
    let mesh = make_grid_mesh(6);
    let surf = build_surface(&mesh);
    let pts = sample_points(&mesh, 200);
    for &p in &pts {
        let (dk, ik, _) = surf_nearest(&surf, p);
        let (db, ib) = brute_nearest(&mesh, p);
        assert!((dk - db).abs() <= 1e-9 * db.max(1.0), "dist {} vs {}", dk, db);
        // index may differ only on exact ties; assert distance-consistency
        let _ = (ik, ib);
    }
}

#[test]
fn bvh_ray_broadphase_is_superset() {
    let mesh = make_grid_mesh(5);
    let surf = build_surface(&mesh);
    let rays = sample_rays(&mesh, 100);
    for (o, d) in rays {
        let brute = brute_moller_hits(&mesh, o, d, 1.0);
        let kern = surf_collect_hits(&surf, o, d, 1.0);
        // every brute hit face must appear in the kernel's collected set
        for f in brute {
            assert!(kern.contains(&f), "broad phase missed face {}", f);
        }
    }
}

#[test]
fn bvh_pathological_soups_build_and_query() {
    // all-identical triangles (coincident duplicates)
    let mesh = make_identical_tris(64);
    let surf = build_surface(&mesh);
    let (d, _, _) = surf_nearest(&surf, [10.0, 0.0, 0.0]);
    assert!(d.is_finite());
    // slivers (near-zero area)
    let mesh = make_slivers(64);
    let surf = build_surface(&mesh);
    let (d, _, _) = surf_nearest(&surf, [0.0, 0.0, 5.0]);
    assert!(d.is_finite());
}

#[test]
fn deterministic_build_bit_identical() {
    let mesh = make_grid_mesh(6);
    let s1 = build_surface(&mesh);
    let s2 = build_surface(&mesh);
    assert_eq!(surf_bvh_signature(&s1), surf_bvh_signature(&s2));
}
