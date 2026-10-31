use super::*;
use crate::testutil::{cube_soup, slot_threads, soup_from_triangles};

fn approx(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn grid(h: f64, origin: f64, layers: u32) -> LayerGrid {
    LayerGrid {
        height: h,
        origin,
        layers,
    }
}

fn known(m: Measure<Vec<LayerSlab>>) -> Vec<LayerSlab> {
    match m {
        Measure::Known(v) => v,
        Measure::Unverified(why) => panic!("unverified: {why}"),
    }
}

/// A closed convex solid from triangles, each flipped to face away from `inside`.
fn convex_mesh(tris: &[[[f64; 3]; 3]], inside: [f64; 3]) -> Mesh {
    let fixed: Vec<[[f64; 3]; 3]> = tris
        .iter()
        .map(|t| {
            let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
            let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
            let n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let to = [
                t[0][0] - inside[0],
                t[0][1] - inside[1],
                t[0][2] - inside[2],
            ];
            if n[0] * to[0] + n[1] * to[1] + n[2] * to[2] < 0.0 {
                [t[0], t[2], t[1]]
            } else {
                *t
            }
        })
        .collect();
    let s = soup_from_triangles(&fixed);
    Mesh::from_soup(&s.0, &s.1, &s.2).expect("mesh")
}

/// The wedge `0 <= y <= 2, 0 <= x <= 4, 0 <= z <= x`.
fn wedge() -> Mesh {
    let v = |x: f64, y: f64, z: f64| [x, y, z];
    let (a, b, c) = (v(0.0, 0.0, 0.0), v(4.0, 0.0, 0.0), v(4.0, 0.0, 4.0));
    let (d, e, f) = (v(0.0, 2.0, 0.0), v(4.0, 2.0, 0.0), v(4.0, 2.0, 4.0));
    convex_mesh(
        &[
            [a, b, c],
            [d, e, f],
            [a, b, e],
            [a, e, d],
            [b, c, f],
            [b, f, e],
            [a, c, f],
            [a, f, d],
        ],
        [3.0, 1.0, 1.0],
    )
}

#[test]
fn a_cube_has_the_same_area_in_every_layer_and_one_four_corner_ring() {
    let s = cube_soup();
    let m = Mesh::from_soup(&s.0, &s.1, &s.2).unwrap();
    let slabs = known(slice_layers(&m, &grid(0.25, 0.0, 4)));
    assert_eq!(slabs.len(), 4);
    for (k, s) in slabs.iter().enumerate() {
        assert_eq!(s.layer as usize, k);
        assert!(
            approx(s.envelope.area, 1.0, 1e-12),
            "{k} {}",
            s.envelope.area
        );
        assert!(approx(s.midplane.area, 1.0, 1e-12));
        assert_eq!(s.envelope.rings.len(), 1);
        assert_eq!(s.envelope.rings[0].len(), 4);
    }
}

#[test]
fn layers_off_the_cube_ends_are_empty_and_a_partial_layer_is_full() {
    let s = cube_soup();
    let m = Mesh::from_soup(&s.0, &s.1, &s.2).unwrap();
    // Layers [-0.15,-0.05) ... : layer 0 is below, layer 1 straddles z = 0, the last straddles z = 1.
    let slabs = known(slice_layers(&m, &grid(0.3, -0.45, 7)));
    let areas: Vec<f64> = slabs.iter().map(|s| s.envelope.area).collect();
    assert!(approx(areas[0], 0.0, 1e-12), "{areas:?}");
    assert!(approx(areas[1], 1.0, 1e-12), "{areas:?}");
    assert!(approx(areas[4], 1.0, 1e-12), "{areas:?}");
    assert!(
        approx(areas[5], 0.0, 1e-12) && approx(areas[6], 0.0, 1e-12),
        "{areas:?}"
    );
}

#[test]
fn a_slab_on_a_cube_face_sees_only_the_material_inside_it() {
    // Layer boundaries exactly on the cube's bottom and top faces.
    let s = cube_soup();
    let m = Mesh::from_soup(&s.0, &s.1, &s.2).unwrap();
    let slabs = known(slice_layers(&m, &grid(0.5, -0.5, 4)));
    let areas: Vec<f64> = slabs.iter().map(|s| s.envelope.area).collect();
    assert!(
        approx(areas[0], 0.0, 1e-12),
        "below the bottom face: {areas:?}"
    );
    assert!(approx(areas[1], 1.0, 1e-12) && approx(areas[2], 1.0, 1e-12));
    assert!(
        approx(areas[3], 0.0, 1e-12),
        "above the top face: {areas:?}"
    );
}

#[test]
fn a_slope_makes_the_envelope_exceed_the_midplane_by_half_a_layer_of_run() {
    let m = wedge();
    let h = 0.5;
    let slabs = known(slice_layers(&m, &grid(h, 0.0, 8)));
    for s in &slabs {
        let k = f64::from(s.layer);
        let (z0, zm) = (k * h, (k + 0.5) * h);
        assert!(
            approx(s.envelope.area, 2.0 * (4.0 - z0), 1e-9),
            "{}",
            s.layer
        );
        assert!(approx(s.midplane.area, 2.0 * (4.0 - zm), 1e-9));
    }
}

#[test]
fn the_batched_pass_equals_the_scalar_references() {
    let m = wedge();
    let g = grid(0.37, 0.05, 11);
    let slabs = known(slice_layers(&m, &g));
    for s in &slabs {
        let env = reference::envelope_area(&m, &g, s.layer);
        assert!(
            approx(s.envelope.area, env, 1e-9),
            "{} {} {env}",
            s.layer,
            s.envelope.area
        );
        let mid = reference::slice_area(&m, 0.5 * (g.z_bottom(s.layer) + g.z_top(s.layer)), false);
        assert!(approx(s.midplane.area, mid, 1e-9));
    }
}

#[test]
fn threads_slice_gives_the_same_layers_at_any_thread_count() {
    let m = wedge();
    let g = grid(0.1, 0.0, 41);
    let one = known(slice_layers_par(&m, &g, 1));
    let limit = slot_threads();
    for t in [1, limit.min(2), limit] {
        let _ = counters::take_par();
        let many = known(slice_layers_par(&m, &g, t));
        let r = counters::take_par().expect("report");
        assert!(
            r.tasks <= limit && r.threads <= limit,
            "{r:?} limit {limit}"
        );
        assert_eq!(one, many, "threads {t}");
    }
}

#[test]
fn a_batch_call_counts_one_call_with_one_item_per_layer() {
    counters::reset();
    let m = wedge();
    let _ = known(slice_layers(&m, &grid(0.5, 0.0, 8)));
    let c = counters::snapshot(Op::SliceLayers);
    assert_eq!((c.calls, c.items), (1, 8));
}

#[test]
fn a_bad_grid_is_refused_not_answered() {
    let m = wedge();
    assert!(slice_layers(&m, &grid(0.0, 0.0, 4)).is_unverified());
    assert!(slice_layers(&m, &grid(f64::NAN, 0.0, 4)).is_unverified());
}

#[test]
fn a_thin_horizontal_plate_inside_one_slab_is_seen() {
    // A 2x2 plate 0.01 thick sitting inside the slab [0.2, 0.3): neither slice plane cuts it.
    let (z0, z1) = (0.24, 0.25);
    let a = [0.0, 0.0, z0];
    let b = [2.0, 0.0, z0];
    let c = [2.0, 2.0, z0];
    let d = [0.0, 2.0, z0];
    let (a1, b1, c1, d1) = (
        [0.0, 0.0, z1],
        [2.0, 0.0, z1],
        [2.0, 2.0, z1],
        [0.0, 2.0, z1],
    );
    let m = convex_mesh(
        &[
            [a, c, b],
            [a, d, c],
            [a1, b1, c1],
            [a1, c1, d1],
            [a, b, b1],
            [a, b1, a1],
            [b, c, c1],
            [b, c1, b1],
            [c, d, d1],
            [c, d1, c1],
            [d, a, a1],
            [d, a1, d1],
        ],
        [1.0, 1.0, 0.245],
    );
    let slabs = known(slice_layers(&m, &grid(0.1, 0.2, 2)));
    assert!(
        approx(slabs[0].envelope.area, 4.0, 1e-9),
        "{}",
        slabs[0].envelope.area
    );
    assert!(approx(slabs[0].midplane.area, 0.0, 1e-12));
    assert!(approx(slabs[1].envelope.area, 0.0, 1e-12));
}

fn round25() -> Mesh {
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/round/approved/Round 25.keepup.stl"
    );
    crate::read_stl_file(std::path::Path::new(p)).expect("Round 25 fixture")
}

#[test]
fn round25_layers_agree_with_the_scalar_references_and_the_volume() {
    let m = round25();
    let zmin = m.bounds().min[2];
    let g = grid(0.05, zmin, 465);
    let t = std::time::Instant::now();
    let slabs = known(slice_layers_par(&m, &g, slot_threads()));
    let secs = t.elapsed().as_secs_f64();
    assert!(secs < 5.0, "465 layers took {secs} s");
    // Volume from the mid-plane slices (layer-centred) against the 851.9 mm^3 of the design.
    let vol: f64 = slabs.iter().map(|s| s.midplane.area * 0.05).sum();
    assert!(approx(vol, 851.9, 0.02 * 851.9), "volume {vol}");
    for k in [0_u32, 1, 40, 152, 173, 290, 300, 400, 464] {
        let s = &slabs[k as usize];
        let mid = reference::slice_area(&m, 0.5 * (g.z_bottom(k) + g.z_top(k)), false);
        assert!(
            approx(s.midplane.area, mid, 1e-6),
            "mid {k}: {} {mid}",
            s.midplane.area
        );
        let env = reference::envelope_area(&m, &g, k);
        assert!(
            approx(s.envelope.area, env, 1e-6),
            "env {k}: {} {env}",
            s.envelope.area
        );
        assert!(s.envelope.area >= s.midplane.area - 1e-9, "layer {k}");
    }
}

#[test]
fn the_ring_area_equals_the_trapezoid_area_on_a_real_layer() {
    let m = round25();
    let g = grid(0.05, m.bounds().min[2], 465);
    let b = Buckets::of(&m, &g);
    let mut env = Vec::new();
    let mut mid = Vec::new();
    for k in [0_u32, 100, 152, 160, 173, 290, 400, 464] {
        layer_segments(
            &m,
            b.layer(k as usize),
            g.z_bottom(k),
            g.z_top(k),
            &mut env,
            &mut mid,
        );
        let st = sweep::strips(&env);
        let r = sweep::rings(&st);
        assert!(approx(r.area, sweep::area(&st), 1e-6), "layer {k}");
        let sum: f64 = r.rings.iter().map(|g| sweep::ring_area(g)).sum();
        assert!(approx(sum, r.area, 1e-12));
    }
}
