use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{cross, dot, Mesh};

/// An undirected vertex edge.  Its endpoints are always in ascending order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Edge {
    pub low: usize,
    pub high: usize,
}

/// Incidence for one unique edge, retaining each face and directed use.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EdgeIncidence {
    pub edge: Edge,
    pub uses: Vec<EdgeUse>,
}

/// One face's directed traversal of an undirected edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EdgeUse {
    pub face: usize,
    pub forward: bool,
}

/// A face-connected shell with a signed f64 volume.
#[derive(Clone, Debug, PartialEq)]
pub struct Shell {
    pub faces: Vec<usize>,
    pub signed_volume: f64,
}

/// Independent topology findings used by M1 format metrics.
#[derive(Clone, Debug, PartialEq)]
pub struct TopologyAnalysis {
    pub boundary_edges: Vec<EdgeIncidence>,
    pub nonmanifold_edges: Vec<EdgeIncidence>,
    pub inconsistent_winding_faces: Vec<usize>,
    pub shells: Vec<Shell>,
}

/// Runs all brute-force edge-incidence, shell, volume, and winding checks.
pub fn analyze_topology(mesh: &Mesh) -> TopologyAnalysis {
    let incidences = all_edge_incidences(mesh);
    let boundary_edges = incidences
        .iter()
        .filter(|incidence| incidence.uses.len() == 1)
        .cloned()
        .collect();
    let nonmanifold_edges = incidences
        .iter()
        .filter(|incidence| incidence.uses.len() > 2)
        .cloned()
        .collect();
    let mut inconsistent = BTreeSet::new();
    for incidence in &incidences {
        if incidence.uses.len() == 2 && incidence.uses[0].forward == incidence.uses[1].forward {
            inconsistent.insert(incidence.uses[0].face);
            inconsistent.insert(incidence.uses[1].face);
        }
    }
    let shells = shell_faces(mesh, &incidences)
        .into_iter()
        .map(|faces| Shell {
            signed_volume: signed_volume(mesh, &faces),
            faces,
        })
        .collect();
    TopologyAnalysis {
        boundary_edges,
        nonmanifold_edges,
        inconsistent_winding_faces: inconsistent.into_iter().collect(),
        shells,
    }
}

/// Returns only edges with exactly one incident triangle.
pub fn boundary_edges(mesh: &Mesh) -> Vec<EdgeIncidence> {
    analyze_topology(mesh).boundary_edges
}

/// Calculates each face-connected shell's signed volume in stable face order.
pub fn signed_shell_volumes(mesh: &Mesh) -> Vec<Shell> {
    analyze_topology(mesh).shells
}

fn all_edge_incidences(mesh: &Mesh) -> Vec<EdgeIncidence> {
    let mut map: BTreeMap<Edge, Vec<EdgeUse>> = BTreeMap::new();
    for (face, &[a, b, c]) in mesh.triangles().iter().enumerate() {
        for (start, end) in [(a, b), (b, c), (c, a)] {
            let edge = Edge {
                low: start.min(end),
                high: start.max(end),
            };
            map.entry(edge).or_default().push(EdgeUse {
                face,
                forward: start == edge.low,
            });
        }
    }
    map.into_iter()
        .map(|(edge, uses)| EdgeIncidence { edge, uses })
        .collect()
}

fn shell_faces(mesh: &Mesh, incidences: &[EdgeIncidence]) -> Vec<Vec<usize>> {
    let mut neighbors = vec![Vec::new(); mesh.triangles().len()];
    for incidence in incidences {
        if incidence.uses.len() >= 2 {
            for (index, use_a) in incidence.uses.iter().enumerate() {
                for use_b in &incidence.uses[index + 1..] {
                    neighbors[use_a.face].push(use_b.face);
                    neighbors[use_b.face].push(use_a.face);
                }
            }
        }
    }
    let mut seen = vec![false; mesh.triangles().len()];
    let mut shells = Vec::new();
    for start in 0..mesh.triangles().len() {
        if seen[start] {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        let mut faces = Vec::new();
        seen[start] = true;
        while let Some(face) = queue.pop_front() {
            faces.push(face);
            for &neighbor in &neighbors[face] {
                if !seen[neighbor] {
                    seen[neighbor] = true;
                    queue.push_back(neighbor);
                }
            }
        }
        faces.sort_unstable();
        shells.push(faces);
    }
    shells
}

fn signed_volume(mesh: &Mesh, faces: &[usize]) -> f64 {
    // Serial ordered summation is intentional: float reductions must not change
    // when a future caller changes a thread count.
    faces.iter().fold(0.0, |sum, &face| {
        let (a, b, c) = mesh.triangle_points(face);
        sum + dot(a, cross(b, c)) / 6.0
    })
}
