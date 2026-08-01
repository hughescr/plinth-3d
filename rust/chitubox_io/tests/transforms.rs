//! Transform probes (§7.3): translation reads exactly, scale is baked 1.5x,
//! rotation matches the corpus-pinned `Ry(45)·Rz(45)` vertex set; then the
//! library's own placement + Z-rotation operations.

mod common;
use chitubox_io::{Placement, PlacementUpdate, Project, ZRotation};
use common::*;

fn open_required(name: &str) -> Project {
    let bytes = read_required_fixture(name);
    Project::from_bytes(bytes).unwrap_or_else(|error| panic!("parse {name}: {error}"))
}

#[test]
fn probe_translation_reads_exact() {
    let (base, moved) = (
        open_required("sphere-cylinder.chitubox"),
        open_required("sphere-moved.chitubox"),
    );
    let b = base.placements()[0];
    let m = moved.placements()[0];
    assert_eq!(
        b,
        Placement {
            x: 0.0,
            y: 0.0,
            z_lift: 0.0
        }
    );
    // sphere moved +10.5 X, +5.25 Y exactly
    assert!((m.x - 10.5).abs() < 1e-4, "x = {}", m.x);
    assert!((m.y - 5.25).abs() < 1e-4, "y = {}", m.y);
    assert_eq!(m.z_lift, 0.0);
}

#[test]
fn probe_scale_is_baked_1_5x() {
    let (base, scaled) = (
        open_required("sphere-cylinder.chitubox"),
        open_required("sphere-scaled.chitubox"),
    );
    // model 0 = Sphere; same triangle count, bbox exactly 1.5x
    let bt = base.model_triangles(0).unwrap();
    let st = scaled.model_triangles(0).unwrap();
    assert_eq!(
        base.model(0).unwrap().triangle_count,
        scaled.model(0).unwrap().triangle_count
    );
    let be = extents(&bt);
    let se = extents(&st);
    for k in 0..3 {
        let ratio = se[k] / be[k];
        assert!((ratio - 1.5).abs() < 1e-3, "axis {k} ratio {ratio}");
    }
}

/// Reference rotation: v' = Ry(deg) * Rz(deg) * v  (world Z first, then Y).
fn ry_rz(v: [f32; 3], deg: f32) -> [f32; 3] {
    let (s, c) = deg.to_radians().sin_cos();
    // Rz
    let x1 = v[0] * c - v[1] * s;
    let y1 = v[0] * s + v[1] * c;
    let z1 = v[2];
    // Ry
    let x2 = x1 * c + z1 * s;
    let y2 = y1;
    let z2 = -x1 * s + z1 * c;
    [x2, y2, z2]
}

#[test]
fn probe_rotation_matches_ry_rz_vertex_set() {
    let (base, rot) = (
        open_required("sphere-cylinder.chitubox"),
        open_required("cylinder-rotated.chitubox"),
    );
    // model 1 = Cylinder (the faceted, non-symmetric probe)
    let bt = base.model_triangles(1).unwrap();
    let rt = rot.model_triangles(1).unwrap();
    let rotated: Vec<f32> = bt
        .chunks_exact(3)
        .flat_map(|c| ry_rz([c[0], c[1], c[2]], 45.0))
        .collect();
    let diff = sorted_set_max_diff(&rotated, &rt);
    assert!(diff < 0.005, "Ry(45)*Rz(45) vertex-set diff {diff} mm");
}

#[test]
fn library_set_placements_persists_exact() {
    let mut proj = open_required("sphere-cylinder.chitubox");
    proj.set_placements(&[PlacementUpdate {
        model_index: 0,
        placement: Placement {
            x: 10.5,
            y: 5.25,
            z_lift: 0.0,
        },
    }])
    .unwrap();
    let bytes = proj.to_bytes().unwrap();
    let re = Project::from_bytes(bytes).unwrap();
    let p = re.placements()[0];
    assert_eq!(p.x, 10.5);
    assert_eq!(p.y, 5.25);
    assert_eq!(p.z_lift, 0.0);
}

#[test]
fn library_rotate_z_90_rotates_vertices() {
    let mut proj = open_required("sphere-cylinder.chitubox");
    let before = proj.model_triangles(1).unwrap();
    proj.rotate_model_z(1, 90.0).unwrap();
    let after = proj.model_triangles(1).unwrap();
    assert_eq!(before.len(), after.len());
    // 90 deg CCW about Z: (x,y,z) -> (-y, x, z), within f32 trig noise
    for (b, a) in before.chunks_exact(3).zip(after.chunks_exact(3)) {
        assert!((a[0] - (-b[1])).abs() < 1e-3, "x: {} vs {}", a[0], -b[1]);
        assert!((a[1] - b[0]).abs() < 1e-3, "y: {} vs {}", a[1], b[0]);
        assert_eq!(a[2], b[2], "z unchanged");
    }
    // translation fields untouched by rotation
    let p = proj.placements()[1];
    let base = Project::from_bytes(read_required_fixture("sphere-cylinder.chitubox"))
        .unwrap()
        .placements()[1];
    assert_eq!(p, base);
    assert!((proj.transform_info(1).unwrap().session_z_rot_deg - 90.0).abs() < 1e-6);
}

#[test]
fn library_rotate_rebake_is_deterministic() {
    // 45 + 45 == one 90 turn, bit-for-bit (rebake from pristine).
    let base_bytes = read_required_fixture("sphere-cylinder.chitubox");
    let mut a = Project::from_bytes(base_bytes.clone()).unwrap();
    let mut b = Project::from_bytes(base_bytes).unwrap();
    a.rotate_model_z(1, 90.0).unwrap();
    b.rotate_models_z(&[ZRotation {
        model_index: 1,
        delta_deg: 45.0,
    }])
    .unwrap();
    b.rotate_models_z(&[ZRotation {
        model_index: 1,
        delta_deg: 45.0,
    }])
    .unwrap();
    assert_eq!(
        a.model_triangles(1).unwrap(),
        b.model_triangles(1).unwrap(),
        "45+45 must equal one 90 bit-for-bit"
    );
    // and the whole serialized file matches
    assert_eq!(a.to_bytes().unwrap(), b.to_bytes().unwrap());
}

#[test]
fn rotate_supported_assembly_moves_records_and_blobs() {
    // Rotating a supported model rebakes model mesh, support blobs, AND record
    // endpoints as one rigid assembly.
    let mut proj = open_required("sphere-cylinder-supported.chitubox");
    let before = proj.supports(0).unwrap();
    proj.rotate_model_z(0, 90.0).unwrap();
    let after = proj.supports(0).unwrap();
    assert_eq!(before.len(), after.len());
    for (b, a) in before.iter().zip(&after) {
        // endpoints rotated (-y, x, z)
        assert!((a.start[0] - (-b.start[1])).abs() < 1e-3);
        assert!((a.start[1] - b.start[0]).abs() < 1e-3);
        assert_eq!(a.start[2], b.start[2]);
        // mesh blob rotated too (first vertex)
        if !b.mesh.xyz.is_empty() {
            assert!((a.mesh.xyz[0] - (-b.mesh.xyz[1])).abs() < 1e-3);
        }
    }
    // still verifies + round-trips structurally
    let bytes = proj.to_bytes().unwrap();
    let re = Project::from_bytes(bytes).unwrap();
    assert_eq!(re.support_counts(), proj.support_counts());
}
