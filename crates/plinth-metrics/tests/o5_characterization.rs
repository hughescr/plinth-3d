//! Characterization evidence for the O-5 canonical geometry cutover.
//!
//! These fixtures deliberately derive just enough triangle-soup metadata to
//! construct the native surface.  They are analytic test evidence for unit
//! cubes, not reusable geometry code or a second implementation.

use std::collections::BTreeSet;

use geom_kernel::{NativeSurface, TriangleSoup};
use plinth_metrics::{analyze_topology, contains_points, Mesh, Vec3};

const METRICS_BOUNDARY_TOLERANCE: f64 = 1.0e-9;

fn cube(origin: Vec3, edge: f64) -> Mesh {
    let [x, y, z] = origin;
    let vertices = vec![
        [x, y, z],
        [x + edge, y, z],
        [x + edge, y + edge, z],
        [x, y + edge, z],
        [x, y, z + edge],
        [x + edge, y, z + edge],
        [x + edge, y + edge, z + edge],
        [x, y + edge, z + edge],
    ];
    // Ordered, outward-facing source triangles: source order is itself part
    // of this characterization evidence.
    let triangles = vec![
        [0, 2, 1],
        [0, 3, 2], // -z
        [4, 5, 6],
        [4, 6, 7], // +z
        [0, 1, 5],
        [0, 5, 4], // -y
        [1, 2, 6],
        [1, 6, 5], // +x
        [2, 3, 7],
        [2, 7, 6], // +y
        [3, 0, 4],
        [3, 4, 7], // -x
    ];
    Mesh::new(vertices, triangles).expect("analytic cube must be a valid metrics mesh")
}

fn join(parts: &[Mesh]) -> Mesh {
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for part in parts {
        let offset = vertices.len();
        vertices.extend_from_slice(part.vertices());
        triangles.extend(
            part.triangles()
                .iter()
                .map(|[a, b, c]| [a + offset, b + offset, c + offset]),
        );
    }
    Mesh::new(vertices, triangles).expect("joined analytic cubes must remain valid")
}

fn add_quad(vertices: &mut Vec<Vec3>, triangles: &mut Vec<[usize; 3]>, corners: [Vec3; 4]) {
    let start = vertices.len();
    vertices.extend(corners);
    triangles.push([start, start + 1, start + 2]);
    triangles.push([start, start + 2, start + 3]);
}

/// Same closed 3x3 extruded voxel ring style as the established topology
/// semantics suite: the volume centroid lies in an empty concave void.
fn concave_voxel_ring() -> Mesh {
    let cells = (0..3)
        .flat_map(|x| (0..3).map(move |y| (x, y, 0)))
        .filter(|&(x, y, _)| (x, y) != (1, 1))
        .collect::<BTreeSet<_>>();
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for &(x, y, z) in &cells {
        let x = f64::from(x);
        let y = f64::from(y);
        let z = f64::from(z);
        let neighbor = |dx, dy, dz| cells.contains(&(x as i32 + dx, y as i32 + dy, z as i32 + dz));
        if !neighbor(-1, 0, 0) {
            add_quad(
                &mut vertices,
                &mut triangles,
                [
                    [x, y, z],
                    [x, y, z + 1.0],
                    [x, y + 1.0, z + 1.0],
                    [x, y + 1.0, z],
                ],
            );
        }
        if !neighbor(1, 0, 0) {
            add_quad(
                &mut vertices,
                &mut triangles,
                [
                    [x + 1.0, y, z],
                    [x + 1.0, y + 1.0, z],
                    [x + 1.0, y + 1.0, z + 1.0],
                    [x + 1.0, y, z + 1.0],
                ],
            );
        }
        if !neighbor(0, -1, 0) {
            add_quad(
                &mut vertices,
                &mut triangles,
                [
                    [x, y, z],
                    [x + 1.0, y, z],
                    [x + 1.0, y, z + 1.0],
                    [x, y, z + 1.0],
                ],
            );
        }
        if !neighbor(0, 1, 0) {
            add_quad(
                &mut vertices,
                &mut triangles,
                [
                    [x, y + 1.0, z],
                    [x, y + 1.0, z + 1.0],
                    [x + 1.0, y + 1.0, z + 1.0],
                    [x + 1.0, y + 1.0, z],
                ],
            );
        }
        if !neighbor(0, 0, -1) {
            add_quad(
                &mut vertices,
                &mut triangles,
                [
                    [x, y, z],
                    [x, y + 1.0, z],
                    [x + 1.0, y + 1.0, z],
                    [x + 1.0, y, z],
                ],
            );
        }
        if !neighbor(0, 0, 1) {
            add_quad(
                &mut vertices,
                &mut triangles,
                [
                    [x, y, z + 1.0],
                    [x + 1.0, y, z + 1.0],
                    [x + 1.0, y + 1.0, z + 1.0],
                    [x, y + 1.0, z + 1.0],
                ],
            );
        }
    }
    Mesh::new(vertices, triangles).expect("analytic concave ring must be valid")
}

fn native_surface(mesh: &Mesh) -> NativeSurface {
    let triangles = mesh
        .triangles()
        .iter()
        .map(|&[a, b, c]| [mesh.vertices()[a], mesh.vertices()[b], mesh.vertices()[c]])
        .collect::<Vec<_>>();
    let mut normals = Vec::with_capacity(triangles.len());
    let mut areas = Vec::with_capacity(triangles.len());
    for &[a, b, c] in &triangles {
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cross = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        let twice_area = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        assert!(
            twice_area.is_finite() && twice_area > 0.0,
            "analytic cube face must have finite area"
        );
        normals.push([
            cross[0] / twice_area,
            cross[1] / twice_area,
            cross[2] / twice_area,
        ]);
        areas.push(twice_area * 0.5);
    }
    let mut lower = [f64::INFINITY; 3];
    let mut upper = [f64::NEG_INFINITY; 3];
    for point in mesh.vertices() {
        for axis in 0..3 {
            lower[axis] = lower[axis].min(point[axis]);
            upper[axis] = upper[axis].max(point[axis]);
        }
    }
    NativeSurface::from_triangle_soup(
        TriangleSoup::owned(triangles, normals, areas, [lower, upper]),
        2.0,
        usize::MAX,
    )
    .expect("analytic cube must construct a native surface")
}

#[test]
fn metrics_boundary_wrapper_and_kernel_inside_are_intentionally_not_equivalent() {
    let mesh = cube([0.0, 0.0, 0.0], 1.0);
    let native = native_surface(&mesh);
    let at_face = [0.0, 0.5, 0.5];
    let points = [
        at_face,
        [f64::from_bits(1), 0.5, 0.5],
        [-f64::from_bits(1), 0.5, 0.5],
        [-METRICS_BOUNDARY_TOLERANCE.next_down(), 0.5, 0.5],
        [-METRICS_BOUNDARY_TOLERANCE, 0.5, 0.5],
        [-METRICS_BOUNDARY_TOLERANCE.next_up(), 0.5, 0.5],
        [0.0, 0.0, 0.5],
        [0.0, 0.0, 0.0],
        [
            -(METRICS_BOUNDARY_TOLERANCE * 0.75),
            -(METRICS_BOUNDARY_TOLERANCE * 0.75),
            0.5,
        ],
    ];

    // One gathered batch per API is the contract under test; results are then
    // scattered by position.  Do not replace metrics with bare kernel parity:
    // metrics first admits project distance <= 1e-9, then does containment.
    let metrics = contains_points(&mesh, &points).expect("metrics containment batch");
    let kernel = native.inside(&points);
    assert_eq!(
        metrics,
        vec![true, true, true, true, true, false, true, true, false]
    );
    assert_eq!(
        kernel,
        vec![true, true, true, true, true, false, true, true, true]
    );
    assert_ne!(
        metrics, kernel,
        "O-5 must preserve the metrics boundary wrapper"
    );
}

#[test]
fn cavity_count_preserves_per_outer_subset_isolation_for_ordered_shells() {
    let outer = cube([0.0, 0.0, 0.0], 16.0);
    let depth_one = join(&[outer.clone(), cube([2.0, 2.0, 2.0], 8.0)]);
    let depth_two = join(&[
        outer.clone(),
        cube([2.0, 2.0, 2.0], 8.0),
        cube([4.0, 4.0, 4.0], 2.0),
    ]);
    let depth_three = join(&[
        outer.clone(),
        cube([2.0, 2.0, 2.0], 8.0),
        cube([4.0, 4.0, 4.0], 2.0),
        cube([4.5, 4.5, 4.5], 0.5),
    ]);
    let disconnected = join(&[outer.clone(), cube([32.0, 0.0, 0.0], 4.0)]);
    let boundary_touching = join(&[outer, cube([0.0, 4.0, 4.0], 4.0)]);
    let concave = join(&[concave_voxel_ring(), cube([0.35, 0.35, 0.35], 0.1)]);

    assert_eq!(analyze_topology(&depth_one).unwrap().cavities, 1);
    assert_eq!(analyze_topology(&depth_two).unwrap().cavities, 1);
    assert_eq!(analyze_topology(&depth_three).unwrap().cavities, 2);
    assert_eq!(analyze_topology(&disconnected).unwrap().cavities, 0);
    assert_eq!(analyze_topology(&boundary_touching).unwrap().cavities, 1);
    assert_eq!(analyze_topology(&concave).unwrap().cavities, 1);

    // The combined parity model alternates material inside the nested shell.
    // The topology metric instead gathers each candidate shell's boundary
    // probes and queries the corresponding outer subset once; using full-model
    // containment for that outer decision would change its result.
    let intershell_point = [[3.0, 3.0, 3.0]];
    assert_eq!(
        contains_points(&depth_one, &intershell_point).unwrap(),
        vec![false],
        "full-model containment is not a substitute for per-outer-shell isolation"
    );
    assert_eq!(
        contains_points(&cube([0.0, 0.0, 0.0], 16.0), &intershell_point).unwrap(),
        vec![true]
    );
}
