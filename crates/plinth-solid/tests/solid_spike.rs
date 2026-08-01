use std::collections::{BTreeMap, BTreeSet, VecDeque};

use plinth_solid::{backend_configuration, ManifoldBackend, Mesh, SolidBoolean};

fn cuboid(min: [f64; 3], max: [f64; 3]) -> Mesh {
    let vertices = vec![
        [min[0], min[1], min[2]],
        [max[0], min[1], min[2]],
        [max[0], max[1], min[2]],
        [min[0], max[1], min[2]],
        [min[0], min[1], max[2]],
        [max[0], min[1], max[2]],
        [max[0], max[1], max[2]],
        [min[0], max[1], max[2]],
    ];
    // CCW from outside on all six faces.
    let triangles = vec![
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
    Mesh::new(vertices, triangles).expect("fixture cuboid must be structurally valid")
}

#[derive(Debug)]
struct Audit {
    signed_volume: f64,
    shell_count: usize,
}

fn audit(mesh: &Mesh) -> Audit {
    assert!(!mesh.vertices().is_empty(), "output must contain vertices");
    assert!(
        !mesh.triangles().is_empty(),
        "output must contain triangles"
    );
    assert!(
        mesh.vertices()
            .iter()
            .flatten()
            .all(|value| value.is_finite()),
        "every emitted coordinate must be finite"
    );

    let mut directed_edges = BTreeMap::<(u64, u64), usize>::new();
    let mut edge_faces = BTreeMap::<(u64, u64), Vec<usize>>::new();
    for (face, triangle) in mesh.triangles().iter().enumerate() {
        for &(from, to) in &[
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            *directed_edges.entry((from, to)).or_default() += 1;
            let edge = if from < to { (from, to) } else { (to, from) };
            edge_faces.entry(edge).or_default().push(face);
        }
    }
    for (edge, faces) in &edge_faces {
        assert_eq!(
            faces.len(),
            2,
            "edge {edge:?} must have exactly two incident faces"
        );
        assert_eq!(
            directed_edges.get(&(edge.0, edge.1)),
            Some(&1),
            "edge {edge:?} must appear once in the forward direction"
        );
        assert_eq!(
            directed_edges.get(&(edge.1, edge.0)),
            Some(&1),
            "edge {edge:?} must appear once in the reverse direction"
        );
    }

    let mut adjacency = vec![Vec::new(); mesh.triangles().len()];
    for faces in edge_faces.values() {
        adjacency[faces[0]].push(faces[1]);
        adjacency[faces[1]].push(faces[0]);
    }
    let mut unseen: BTreeSet<_> = (0..mesh.triangles().len()).collect();
    let mut shell_volumes = Vec::new();
    while let Some(seed) = unseen.pop_first() {
        let mut queue = VecDeque::from([seed]);
        let mut volume = 0.0;
        while let Some(face) = queue.pop_front() {
            let [a, b, c] = mesh.triangles()[face].map(|index| {
                mesh.vertices()[usize::try_from(index).expect("u64 mesh index must fit usize")]
            });
            volume += dot(a, cross(b, c)) / 6.0;
            for neighbor in &adjacency[face] {
                if unseen.remove(neighbor) {
                    queue.push_back(*neighbor);
                }
            }
        }
        shell_volumes.push(volume);
    }
    assert!(
        shell_volumes.iter().all(|volume| *volume > 0.0),
        "every connected shell must have positive winding: {shell_volumes:?}"
    );
    Audit {
        signed_volume: shell_volumes.iter().sum(),
        shell_count: shell_volumes.len(),
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn assert_volume(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1.0e-10,
        "expected volume {expected}, got {actual}"
    );
}

#[test]
fn backend_is_the_pinned_serial_f64_configuration() {
    let config = backend_configuration();
    assert_eq!(config.version, (3, 5, 2));
    assert_eq!(config.parallel_mode, -1);
    assert_eq!(config.scalar_bytes, 8);
    assert_eq!(config.index_bytes, 8);
    assert!(!config.cross_section_enabled);
}

#[test]
fn batch_union_is_watertight_wound_finite_and_deterministic() {
    let backend = ManifoldBackend;
    let inputs = [
        cuboid([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]),
        cuboid([1.0, 0.0, 0.0], [3.0, 2.0, 2.0]),
    ];
    let result = backend
        .union(&inputs)
        .expect("overlapping cuboids must union");
    let repeated = backend.union(&inputs).expect("repeat union must succeed");
    assert_eq!(
        result, repeated,
        "serial union output must be deterministic"
    );
    let report = audit(&result);
    assert_eq!(report.shell_count, 1);
    assert_volume(report.signed_volume, 12.0);
}

#[test]
fn batch_difference_is_watertight_wound_finite_and_deterministic() {
    let backend = ManifoldBackend;
    let inputs = [
        cuboid([0.0, 0.0, 0.0], [3.0, 3.0, 3.0]),
        cuboid([0.5, 0.5, -0.5], [1.0, 1.0, 3.5]),
        cuboid([2.0, 2.0, -0.5], [2.5, 2.5, 3.5]),
    ];
    let result = backend
        .difference(&inputs)
        .expect("two through-holes must subtract as one batch-unioned tail");
    let repeated = backend
        .difference(&inputs)
        .expect("repeat difference must succeed");
    assert_eq!(
        result, repeated,
        "serial difference output must be deterministic"
    );
    let report = audit(&result);
    assert_eq!(report.shell_count, 1);
    assert_volume(report.signed_volume, 25.5);
}

#[test]
fn validation_and_batch_shape_fail_loudly() {
    assert!(Mesh::new(vec![[f64::NAN, 0.0, 0.0]], vec![[0, 0, 0]]).is_err());
    let backend = ManifoldBackend;
    assert!(backend.union(&[]).is_err());
    let cube = cuboid([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    assert!(backend.difference(&[cube]).is_err());
}
