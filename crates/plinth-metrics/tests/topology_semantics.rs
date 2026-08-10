use std::collections::BTreeSet;

use plinth_metrics::{
    analyze_topology, contains_points, topology_metrics, Mesh, ShellWinding, Vec3,
};

fn tetrahedron(origin: Vec3, scale: f64) -> Mesh {
    let vertices = vec![
        origin,
        [origin[0] + scale, origin[1], origin[2]],
        [origin[0], origin[1] + scale, origin[2]],
        [origin[0], origin[1], origin[2] + scale],
    ];
    Mesh::new(vertices, vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]]).unwrap()
}

fn flip_face(mesh: &Mesh, face: usize) -> Mesh {
    let mut triangles = mesh.triangles().to_vec();
    triangles[face].swap(1, 2);
    Mesh::new(mesh.vertices().to_vec(), triangles).unwrap()
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
    Mesh::new(vertices, triangles).unwrap()
}

fn add_quad(vertices: &mut Vec<Vec3>, triangles: &mut Vec<[usize; 3]>, corners: [Vec3; 4]) {
    let start = vertices.len();
    vertices.extend(corners);
    triangles.push([start, start + 1, start + 2]);
    triangles.push([start, start + 2, start + 3]);
}

/// Closed 3×3 voxel ring, extruded one unit: its volume centroid is the open
/// center cell, so a center-point containment shortcut is invalid.
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
    Mesh::new(vertices, triangles).unwrap()
}

#[test]
fn globally_solved_winding_counts_original_faces_not_edge_incidents() {
    let upright = tetrahedron([0.0, 0.0, 0.0], 1.0);
    let one_flipped = flip_face(&upright, 2);
    let analysis = analyze_topology(&one_flipped).unwrap();
    assert!(analysis.shells[0].sealed);
    assert!(matches!(
        &analysis.shells[0].winding,
        ShellWinding::Outward { faces_requiring_flip } if faces_requiring_flip == &[2]
    ));

    let mut reversed = upright.triangles().to_vec();
    for triangle in &mut reversed {
        triangle.swap(1, 2);
    }
    let all_reversed = Mesh::new(upright.vertices().to_vec(), reversed).unwrap();
    let reversed_analysis = analyze_topology(&all_reversed).unwrap();
    assert!(reversed_analysis.shells[0].signed_volume < 0.0);
    assert!(matches!(
        &reversed_analysis.shells[0].winding,
        ShellWinding::Outward { faces_requiring_flip } if faces_requiring_flip.len() == 4
    ));

    let disconnected = join(&[upright, flip_face(&tetrahedron([4.0, 0.0, 0.0], 1.0), 1)]);
    let disconnected_analysis = analyze_topology(&disconnected).unwrap();
    assert_eq!(disconnected_analysis.shells.len(), 2);
    assert!(matches!(
        &disconnected_analysis.shells[0].winding,
        ShellWinding::Outward { faces_requiring_flip } if faces_requiring_flip.is_empty()
    ));
    assert!(matches!(
        &disconnected_analysis.shells[1].winding,
        ShellWinding::Outward { faces_requiring_flip } if faces_requiring_flip == &[5]
    ));
}

#[test]
fn sealedness_is_orientation_blind_and_cavities_use_boundary_probe_batches() {
    let outer = tetrahedron([-1.0, -1.0, -1.0], 4.0);
    let flipped_inner = flip_face(&tetrahedron([0.0, 0.0, 0.0], 0.2), 1);
    let nested = analyze_topology(&join(&[outer, flipped_inner])).unwrap();
    assert_eq!(
        nested.cavities, 1,
        "sealed inner shell remains a cavity despite winding defect"
    );
    assert!(nested.shells[1].sealed);
    assert!(matches!(
        &nested.shells[1].winding,
        ShellWinding::Outward { faces_requiring_flip } if faces_requiring_flip == &[5]
    ));

    let concave = concave_voxel_ring();
    assert_eq!(
        contains_points(&concave, &[[1.5, 1.5, 0.5]]).unwrap(),
        vec![false],
        "the concave outer shell's volume centroid lies in its central void"
    );
    let inner = tetrahedron([0.35, 0.35, 0.35], 0.1);
    let combined = join(&[concave, inner]);
    let first = analyze_topology(&combined).unwrap();
    let second = analyze_topology(&combined).unwrap();
    assert_eq!(
        first, second,
        "probe gathering/scattering order is deterministic"
    );
    assert_eq!(
        first.cavities, 1,
        "boundary probes find the inner sealed shell in concave material"
    );
}

/// Flip every triangle of a mesh, inverting its winding and the sign of its
/// signed volume.
fn flip_all(mesh: &Mesh) -> Mesh {
    let mut triangles = mesh.triangles().to_vec();
    for triangle in &mut triangles {
        triangle.swap(1, 2);
    }
    Mesh::new(mesh.vertices().to_vec(), triangles).unwrap()
}

/// The shape `plinth-hollow` proves and hands downstream: outer material wound
/// outward, one cavity wound inward. That inward cavity is *required* — the
/// hollow stage refuses a shell that does not have exactly one of them — so it
/// must not be counted against the `negative_shells = 0` acceptance bound.
#[test]
fn a_correct_two_shell_hollow_has_no_negative_shells() {
    let outer = tetrahedron([-1.0, -1.0, -1.0], 4.0);
    let cavity = flip_all(&tetrahedron([0.0, 0.0, 0.0], 0.2));
    let analysis = analyze_topology(&join(&[outer, cavity])).unwrap();

    // The fixture really is a hollow: two sealed shells, the inner one nested
    // and inward-wound, the outer one outward-wound.
    assert_eq!(analysis.shells.len(), 2);
    assert!(analysis.shells.iter().all(|shell| shell.sealed));
    assert!(
        analysis.shells[0].signed_volume > 0.0,
        "the outer shell must enclose positive material"
    );
    assert!(
        analysis.shells[1].signed_volume < 0.0,
        "the cavity shell must be inward-wound"
    );
    assert_eq!(analysis.cavities, 1);
    assert_eq!(
        analysis.mandated_cavity_shells,
        vec![1],
        "the nested inward-wound shell is the mandated cavity"
    );

    assert_eq!(
        topology_metrics(&analysis).negative_shells,
        0,
        "a correct hollow's mandated cavity is not an inversion defect"
    );
}

/// **The exemption is for one cavity, because the rule is "exactly one".**
///
/// `plinth-hollow::validate_shell_topology` proves a drilled shell only when it
/// is *one* negative-signed-volume cavity enclosed by positive outer material.
/// A per-shell exemption with no cardinality bound would let a mesh with two
/// nested inward-wound shells — a genuine cavity plus, say, an inverted
/// duplicate surface nested inside the same material — report
/// `negative_shells == 0` on this independent disk auditor, and the `cavities`
/// metric carries no absolute bound in `04-migration-plan.md`, so nothing
/// downstream would catch the surplus. Exactly one is exempt; every further
/// one is counted.
#[test]
fn a_second_nested_cavity_is_a_defect_the_bound_still_catches() {
    let outer = tetrahedron([-1.0, -1.0, -1.0], 4.0);
    let mandated = flip_all(&tetrahedron([0.0, 0.0, 0.0], 0.2));
    let surplus = flip_all(&tetrahedron([0.4, 0.2, 0.1], 0.2));
    let analysis = analyze_topology(&join(&[outer, mandated, surplus])).unwrap();

    // The fixture is what it claims: one positive container, two sealed
    // inward-wound shells nested inside it, both structurally cavity-shaped.
    assert_eq!(analysis.shells.len(), 3);
    assert!(analysis.shells.iter().all(|shell| shell.sealed));
    assert!(analysis.shells[0].signed_volume > 0.0);
    assert!(analysis.shells[1].signed_volume < 0.0);
    assert!(analysis.shells[2].signed_volume < 0.0);
    assert_eq!(analysis.cavities, 2);
    assert_eq!(
        analysis.mandated_cavity_shells,
        vec![1, 2],
        "both shells have the shape of the mandated cavity, which is exactly \
         why the count needs a cardinality bound"
    );

    assert_eq!(
        topology_metrics(&analysis).negative_shells,
        1,
        "only one cavity is mandated; the second is a surplus negative shell \
         and must still fail the negative_shells = 0 bound"
    );
}

/// The defect the bound exists for: an inverted outer shell, at depth 0, with
/// nothing enclosing it. It is still counted, and the bound still refuses it.
#[test]
fn a_genuinely_inverted_mesh_still_counts_its_negative_shell() {
    let inverted = flip_all(&tetrahedron([-1.0, -1.0, -1.0], 4.0));
    let analysis = analyze_topology(&inverted).unwrap();

    assert_eq!(analysis.shells.len(), 1);
    assert!(analysis.shells[0].signed_volume < 0.0);
    assert_eq!(analysis.cavities, 0);
    assert!(
        analysis.mandated_cavity_shells.is_empty(),
        "an unenclosed shell can never be a mandated cavity"
    );

    assert_eq!(
        topology_metrics(&analysis).negative_shells,
        1,
        "an inverted mesh must still fail the negative_shells bound"
    );

    // And the exemption is not a hole an inverted *hollow* can slip through:
    // flipping both shells of the correct hollow leaves the outer shell
    // inverted and the inner one wound outward, so the count is still 1.
    let outer = flip_all(&tetrahedron([-1.0, -1.0, -1.0], 4.0));
    let cavity = tetrahedron([0.0, 0.0, 0.0], 0.2);
    let inverted_hollow = analyze_topology(&join(&[outer, cavity])).unwrap();
    assert!(
        inverted_hollow.mandated_cavity_shells.is_empty(),
        "a cavity enclosed by inverted material is not the mandated cavity"
    );
    assert_eq!(topology_metrics(&inverted_hollow).negative_shells, 1);
}
