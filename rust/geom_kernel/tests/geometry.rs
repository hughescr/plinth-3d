//! Native unit tests: closed-form geometry cases + BVH structural properties
//! (spec sec 6 items 8, R2). These exercise the geometry modules directly via
//! the crate's testkit API.

use geom_kernel::testkit::*;
use geom_kernel::MOLLER_PARAMETER_SLACK;

const TEST_UNBOUNDED_GRID_BYTES: usize = usize::MAX;

fn tri(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> ([f64; 3], [f64; 3], [f64; 3]) {
    (a, b, c)
}

#[test]
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
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
fn public_moller_parameter_slack_matches_the_native_acceptance_boundary() {
    let v0 = [0.0, 0.0, 0.0];
    let e1 = [1.0, 0.0, 0.0];
    let e2 = [0.0, 1.0, 0.0];
    for t in [-MOLLER_PARAMETER_SLACK / 2.0, -MOLLER_PARAMETER_SLACK] {
        let (hit, observed_t) = kern_moller([0.25, 0.25, t], [0.0, 0.0, -1.0], v0, e1, e2, 1.0);
        assert!(hit);
        assert_eq!(observed_t.to_bits(), t.to_bits());
    }
    assert!(
        !kern_moller(
            [0.25, 0.25, -MOLLER_PARAMETER_SLACK * 1.000_001],
            [0.0, 0.0, -1.0],
            v0,
            e1,
            e2,
            1.0
        )
        .0
    );
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
        assert!(
            (dk - db).abs() <= 1e-9 * db.max(1.0),
            "dist {} vs {}",
            dk,
            db
        );
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

#[test]
fn subset_views_share_geometry_and_build_global_id_indexes() {
    let mesh = make_grid_mesh(4);
    let full = build_surface(&mesh);
    let subset = full.subset(&[12, 3, 7]).unwrap();
    let nested = subset.subset(&[7, 3]).unwrap();

    assert!(surfaces_share_geometry(&full, &subset));
    assert!(surfaces_share_geometry(&subset, &nested));
    assert_eq!(subset.face_ids(), &[3, 7, 12]);
    assert_eq!(nested.face_ids(), &[3, 7]);
    assert_eq!(surf_bvh_face_ids(&subset), vec![3, 7, 12]);
    assert_eq!(surf_parity_face_ids(&subset), vec![3, 7, 12]);
    assert_ne!(surf_index_stats(&full), surf_index_stats(&subset));

    let mut expected = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
    for &face_id in subset.face_ids() {
        let bounds = full.geometry().face_bounds()[face_id as usize];
        for axis in 0..3 {
            expected[0][axis] = expected[0][axis].min(bounds[0][axis]);
            expected[1][axis] = expected[1][axis].max(bounds[1][axis]);
        }
    }
    assert_eq!(subset.bounds(), expected);
}

// ---- Amendment A11: conservative ray pad guard ---------------------------
// These rays graze just OUTSIDE an AABB-boundary triangle edge, at a
// barycentric offset inside Moller's tolerant [-EPS,0) band. The tolerant
// Moller ACCEPTS them, so the padded broad phase MUST surface them. With the
// per-triangle pad forced to 0 the exact AABB slab rejects the ray before
// Moller ever runs, silently dropping the crossing — a parity/breach under-flag
// (spec sec 3.3/6 item 8; the MUTATION-CONFIRMED gap these tests close).
#[test]
fn a11_pad_captures_edge_ray_v0v1() {
    // single triangle in z=0: v0=(0,0,0), v1=(1,0,0), v2=(0,1,0)
    let surf = build_from_tris(vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    // vertical ray at y = -0.5e-6 (just outside edge v0-v1, |v| < EPS)
    let o = [0.5, -0.5e-6, 1.0];
    let d = [0.0, 0.0, -2.0];
    let hits = surf_collect_hits(&surf, o, d, 1.0);
    assert!(
        hits.contains(&0),
        "A11 pad: tolerant-Moller hit just outside edge v0-v1 (below the exact \
         AABB) must be collected; pad=0 misses it"
    );
}

#[test]
fn a11_pad_captures_edge_ray_v0v2() {
    let surf = build_from_tris(vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    // vertical ray at x = -0.5e-6 (just outside edge v0-v2, |u| < EPS)
    let o = [-0.5e-6, 0.5, 1.0];
    let d = [0.0, 0.0, -2.0];
    let hits = surf_collect_hits(&surf, o, d, 1.0);
    assert!(
        hits.contains(&0),
        "A11 pad: tolerant-Moller hit just outside edge v0-v2 must be collected; \
         pad=0 misses it"
    );
}

// ---- Amendment A17: segment pruning lower bound vs brute force ------------
#[test]
fn a17_segment_pruning_matches_brute_force() {
    let mesh = make_grid_mesh(6);
    let surf = build_surface(&mesh);
    let pts = sample_points(&mesh, 120);
    // pair consecutive sample points into segments
    for w in pts.chunks_exact(2) {
        let (a, b) = (w[0], w[1]);
        let (dk, _, np_k) = surf_seg_min(&surf, a, b);
        let (db, _, np_b) = brute_seg_min(&surf, a, b);
        // same per-triangle primitive => the BVH's axis-gap lower bound must
        // never prune the true nearest triangle: identical minimum distance.
        assert!(
            (dk - db).abs() <= 1e-12 * db.max(1.0),
            "A17 segment lower bound unsound: pruned {dk} vs brute {db}"
        );
        // The pruned walk visits a SUBSET of the faces the brute walk visits,
        // so its uncertainty tally can never exceed the brute one. On a clean
        // probe mesh both are zero; the assertion is the relation, not the
        // zero, so it still says something if a mesh ever plants an
        // undecidable face here.
        assert!(
            np_k <= np_b,
            "pruned walk reported more near-parallel faces ({np_k}) than the \
             brute walk over every face ({np_b})"
        );
    }
}

// ---- Amendment A17: parity grid fails cleanly past a memory budget --------
#[test]
fn a17_parity_grid_budget_fails_cleanly() {
    let mesh = grid_mesh(6);
    // generous budget builds fine
    assert!(
        build_surface_budget(&mesh, TEST_UNBOUNDED_GRID_BYTES).is_ok(),
        "unbounded budget must build"
    );
    // a tiny budget must fail with a clean Err, never panic or OOM
    let res = build_surface_budget(&mesh, 1000);
    assert!(res.is_err(), "tiny grid budget must fail construction");
    let err = res.err().unwrap();
    assert!(
        err.contains("memory budget"),
        "budget failure must be reported cleanly, got: {}",
        err
    );
}

#[test]
fn r6_no_hashmap_in_src() {
    // Determinism tripwire (spec R6 / sec 4.2): HashMap/HashSet iteration order
    // leaks RandomState into results. src/ must contain neither.
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().map(|e| e == "rs").unwrap_or(false) {
            let txt = std::fs::read_to_string(&p).unwrap();
            // match real USAGE tokens, not the words in doc comments
            for bad in ["HashMap<", "HashMap::", "HashSet<", "HashSet::"] {
                assert!(!txt.contains(bad), "forbidden {} in {:?} (spec R6)", bad, p);
            }
        }
    }
}
