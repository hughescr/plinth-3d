use super::*;

fn fixture(name: &str) -> Mesh {
    crate::read_stl_file(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../test-data/chitubox-native/{name}.stl")),
    )
    .unwrap()
}
#[test]
fn hollow_both_native_fixtures_and_measure_f32() {
    for name in ["Sphere", "Cylinder"] {
        let input = fixture(name);
        let h = hollow(&input).unwrap();
        let e = measure(&input, &h.mesh).unwrap();
        e.require_dimensions().unwrap();
        assert!(e.cavity_mm3 > 1000.);
        assert!(e.preserved_uncut_triangles > 100);
        assert!(!e.phases.is_empty());
        assert_eq!(e.phases.len() % 3, 0);
        for p in &e.phases {
            assert_eq!(
                p.slab_top_mm.to_bits(),
                f64::from(p.slab_top_mm as f32).to_bits()
            );
        }
        assert_eq!(
            e.first_material_z_mm.to_bits(),
            h.mesh.bounds().min[2].to_bits()
        );
        assert!(e.holes[0].z_range_mm[1] < e.holes[1].z_range_mm[0]);
        if name == "Sphere" {
            assert!(h.convexity.maximum_mm > 1e-6 && h.convexity.maximum_mm < 1e-5);
            assert!(e.phases.iter().any(|p| p.pieces > 1));
            assert!(e.first_edge_slab.area_mm2 > 3.);
        }
    }
}
#[test]
fn sabotage_wall_and_bore_are_detected() {
    let input = fixture("Cylinder");
    for (offset, diameter, reason) in [(2.4, 3., "wall"), (2.501, 2.9, "diameter")] {
        let h = build(&input, offset, diameter).unwrap();
        let e = measure(&input, &h.mesh).unwrap();
        assert!(e.require_dimensions().unwrap_err().contains(reason));
        assert!(
            evidence::certify_dimensions(&input, &h.mesh)
                .unwrap_err()
                .contains(reason)
        );
    }
}
#[test]
fn emitted_exterior_inward_rounding_is_deducted_from_wall() {
    let input = fixture("Cylinder");
    let h = hollow(&input).unwrap();
    let before = measure(&input, &h.mesh).unwrap();
    let c = before.holes[0].centre_xy_mm;
    let mut z = h.mesh.zs().to_vec();
    let i = z
        .iter()
        .enumerate()
        .find(|(i, z)| {
            (**z - h.mesh.bounds().min[2]).abs() < 1e-8
                && (h.mesh.xs()[*i] - c[0]).hypot(h.mesh.ys()[*i] - c[1]) < 1.51
        })
        .map(|(i, _)| i)
        .unwrap();
    // This cut-fragment vertex is not part of any retained original triangle.
    z[i] += 0.00002;
    let rounded = Mesh::new(
        h.mesh.xs().to_vec(),
        h.mesh.ys().to_vec(),
        z,
        h.mesh.indices().to_vec(),
    )
    .unwrap();
    let after = measure(&input, &rounded).unwrap();
    assert!(after.exterior_inward_deviation_mm >= 0.0000199);
    assert!(after.minimum_wall_mm < before.minimum_wall_mm - 0.000019);
}

#[test]
fn accepts_small_native_noise_but_refuses_a_real_dent() {
    // A box with each face split about its centre supplies a true dent witness.
    let cube = Manifold::cube(20., 20., 20., true).refine(2);
    let input = frozen(&cube).unwrap();
    for (dent, accept) in [(0.00001, true), (0.2, false)] {
        let b = input.bounds();
        let i = input
            .zs()
            .iter()
            .enumerate()
            .find(|(i, z)| **z > b.max[2] - 1e-6 && input.xs()[*i].hypot(input.ys()[*i]) < 1.)
            .map(|(i, _)| i)
            .unwrap();
        let mut z = input.zs().to_vec();
        z[i] -= dent;
        let dented = Mesh::new(
            input.xs().to_vec(),
            input.ys().to_vec(),
            z,
            input.indices().to_vec(),
        )
        .unwrap();
        let result = hollow(&dented);
        assert_eq!(result.is_ok(), accept, "{dent}: {result:?}");
        if accept {
            let e = measure(&dented, &result.unwrap().mesh).unwrap();
            e.require_dimensions().unwrap();
        } else {
            assert!(result.unwrap_err().contains("triangle"));
        }
    }
}
#[test]
fn rejects_open_reversed_and_pinched_meshes() {
    let input = fixture("Cylinder");
    let mesh = |indices| {
        Mesh::new(
            input.xs().to_vec(),
            input.ys().to_vec(),
            input.zs().to_vec(),
            indices,
        )
        .unwrap()
    };
    assert!(closed(&mesh(input.indices()[3..].to_vec())).is_err());
    let mut reversed = input.indices().to_vec();
    for t in reversed.chunks_exact_mut(3) {
        t.swap(1, 2);
    }
    assert!(closed(&mesh(reversed)).unwrap_err().contains("volume"));
    // Two otherwise manifold cubes touch at exactly one vertex, not an edge.
    let a = frozen(&Manifold::cube(2., 2., 2., false)).unwrap();
    let mut xyz = [Vec::new(), Vec::new(), Vec::new()];
    for shift in [0., 2.] {
        for i in 0..a.triangle_count() {
            for v in a.triangle_corners(i) {
                for k in 0..3 {
                    xyz[k].push(v[k] + shift);
                }
            }
        }
    }
    let pinched = Mesh::from_soup(&xyz[0], &xyz[1], &xyz[2]).unwrap();
    assert!(closed(&pinched).unwrap_err().contains("vertex link"));
}
#[test]
fn bore_intrusion_uses_ball_surface_not_its_clear_axis() {
    let input = fixture("Cylinder");
    let h = hollow(&input).unwrap();
    let e = measure(&input, &h.mesh).unwrap();
    let c = e.holes[0].centre_xy_mm;
    let z = e.holes[0].z_range_mm[0] - 1.;
    let parts = [2., 2.5].map(|x| {
        let m = frozen(&Manifold::sphere(0.75, 16).translate(c[0] + x, c[1], z)).unwrap();
        (0..m.triangle_count())
            .map(|i| m.triangle_corners(i))
            .collect()
    });
    assert_eq!(
        bore_intrusions(
            &parts,
            &e,
            BoreExtent::WithOutlets {
                plate_z: input.bounds().min[2] - 5.
            }
        ),
        vec![0]
    );
    assert!(bore_intrusions(&parts, &e, BoreExtent::ThroughWall).is_empty());
}
