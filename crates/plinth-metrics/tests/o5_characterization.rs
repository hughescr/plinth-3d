//! Characterization evidence for the O-5 canonical geometry cutover.
//!
//! These fixtures deliberately derive just enough triangle-soup metadata to
//! construct the native surface.  They are analytic test evidence for unit
//! cubes, not reusable geometry code or a second implementation.

use std::collections::BTreeSet;

use geom_kernel::{NativeSurface, TriangleSoup};
use plinth_metrics::{analyze_topology, closest_points, contains_points, Mesh, Vec3};

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

/// Fixed analytic cube metadata for native-kernel characterization only.  This
/// never accepts a `Mesh`, computes cross products, or reduces bounds: the
/// face normals, areas, and caller-supplied bounds are known cube facts.
fn native_analytic_cubes(cubes: &[(Vec3, f64)], bounds: [[f64; 3]; 2]) -> NativeSurface {
    const FACES: [[usize; 3]; 12] = [
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ];
    const NORMALS: [[f64; 3]; 12] = [
        [0.0, 0.0, -1.0],
        [0.0, 0.0, -1.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [0.0, -1.0, 0.0],
        [0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
    ];
    let mut triangles = Vec::with_capacity(cubes.len() * FACES.len());
    let mut normals = Vec::with_capacity(cubes.len() * NORMALS.len());
    let mut areas = Vec::with_capacity(cubes.len() * FACES.len());
    for &(origin, edge) in cubes {
        let [x, y, z] = origin;
        let vertices = [
            [x, y, z],
            [x + edge, y, z],
            [x + edge, y + edge, z],
            [x, y + edge, z],
            [x, y, z + edge],
            [x + edge, y, z + edge],
            [x + edge, y + edge, z + edge],
            [x, y + edge, z + edge],
        ];
        triangles.extend(FACES.map(|[a, b, c]| [vertices[a], vertices[b], vertices[c]]));
        normals.extend(NORMALS);
        areas.extend([edge * edge * 0.5; FACES.len()]);
    }
    NativeSurface::from_triangle_soup(
        TriangleSoup::owned(triangles, normals, areas, bounds),
        2.0,
        usize::MAX,
    )
    .expect("fixed analytic cube fixture must construct a native surface")
}

#[test]
fn metrics_boundary_wrapper_and_kernel_inside_are_intentionally_not_equivalent() {
    let mesh = cube([0.0, 0.0, 0.0], 1.0);
    let native = native_analytic_cubes(&[([0.0, 0.0, 0.0], 1.0)], [[0.0; 3], [1.0; 3]]);
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
fn cavity_counts_and_actual_boundary_probe_subset_contract_are_characterized() {
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

    // This is the actual candidate-shell boundary batch used by closed_cavities:
    // source-face traversal, bitwise coordinate deduplication, then one batch.
    let inner = cube([2.0, 2.0, 2.0], 8.0);
    let mut seen = BTreeSet::new();
    let mut probes = Vec::new();
    for &[a, b, c] in inner.triangles() {
        for point in [
            inner.vertices()[a],
            inner.vertices()[b],
            inner.vertices()[c],
        ] {
            if seen.insert(point.map(f64::to_bits)) {
                probes.push(point);
            }
        }
    }
    let full_nearest = closest_points(&depth_one, &probes).unwrap();
    assert!(
        full_nearest
            .iter()
            .any(|nearest| nearest.squared_distance <= METRICS_BOUNDARY_TOLERANCE.powi(2)),
        "an actual inner-shell boundary probe must be admitted by the full-model metrics wrapper"
    );
    let outer_subset = cube([0.0, 0.0, 0.0], 16.0);
    let outer_metrics = contains_points(&outer_subset, &probes).unwrap();
    let full_native = native_analytic_cubes(
        &[([0.0, 0.0, 0.0], 16.0), ([2.0, 2.0, 2.0], 8.0)],
        [[0.0; 3], [16.0; 3]],
    )
    .inside(&probes);
    assert_eq!(
        outer_metrics,
        vec![true; probes.len()],
        "the actual outer-subset boundary-probe decision admits the inner shell"
    );
    assert_ne!(
        full_native, outer_metrics,
        "bare full-model kernel containment diverges from the required outer-subset decision"
    );
}
