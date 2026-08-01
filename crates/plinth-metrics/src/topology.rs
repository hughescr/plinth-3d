use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{contains_points, cross, dot, DiskArtifact, Mesh, MetricsError, Vec3};

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
    /// A shell is closed only when every one of its edges has exactly two
    /// incident faces and those faces traverse the edge in opposing directions.
    pub closed: bool,
}

/// Independent topology findings used by M1 format metrics.
#[derive(Clone, Debug, PartialEq)]
pub struct TopologyAnalysis {
    pub boundary_edges: Vec<EdgeIncidence>,
    pub nonmanifold_edges: Vec<EdgeIncidence>,
    pub inconsistent_winding_faces: Vec<usize>,
    pub shells: Vec<Shell>,
    /// Closed shells nested at odd depth inside another closed shell.
    pub cavities: usize,
}

/// The artifact-level portion of the M1 format metrics.
///
/// This deliberately reports only metrics that topology can establish from a
/// fresh disk read.  `islands` is a later independent support-perception
/// metric, not a topology proxy, and therefore is never fabricated here.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ArtifactTopologyMetrics {
    pub boundary_edges: usize,
    pub cavities: usize,
    pub negative_shells: usize,
    pub winding: usize,
}

/// One model's independent topology result, retaining the source slot from
/// the on-disk artifact for a caller's per-model report row.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelTopologyAnalysis {
    pub source_index: usize,
    pub model: TopologyAnalysis,
    pub supports: Vec<TopologyAnalysis>,
    pub metrics: ArtifactTopologyMetrics,
}

/// Complete topology audit of one artifact re-opened from disk.
#[derive(Clone, Debug, PartialEq)]
pub struct ArtifactTopologyAnalysis {
    pub models: Vec<ModelTopologyAnalysis>,
    pub metrics: ArtifactTopologyMetrics,
}

/// Runs all brute-force edge-incidence, shell, volume, and winding checks.
pub fn analyze_topology(mesh: &Mesh) -> Result<TopologyAnalysis, MetricsError> {
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
    let inconsistent = inconsistent_faces(&incidences);
    let shells = shell_faces(mesh, &incidences)
        .into_iter()
        .map(|faces| {
            let closed = shell_is_closed(&faces, &incidences, &inconsistent);
            Ok(Shell {
                signed_volume: signed_volume(mesh, &faces)?,
                faces,
                closed,
            })
        })
        .collect::<Result<Vec<_>, MetricsError>>()?;
    let cavities = closed_cavities(mesh, &shells)?;
    Ok(TopologyAnalysis {
        boundary_edges,
        nonmanifold_edges,
        inconsistent_winding_faces: inconsistent.into_iter().collect(),
        shells,
        cavities,
    })
}

/// Returns only edges with exactly one incident triangle.
pub fn boundary_edges(mesh: &Mesh) -> Result<Vec<EdgeIncidence>, MetricsError> {
    Ok(analyze_topology(mesh)?.boundary_edges)
}

/// Calculates each face-connected shell's signed volume in stable face order.
pub fn signed_shell_volumes(mesh: &Mesh) -> Result<Vec<Shell>, MetricsError> {
    Ok(analyze_topology(mesh)?.shells)
}

/// Audits every model and support soup in one artifact loaded from disk.
///
/// The caller must obtain `artifact` from [`crate::load_artifacts`]; this API
/// intentionally has no path or producer-memory variant.  All components are
/// gathered first, then analyzed in deterministic source order.
pub fn analyze_artifact_topology(
    artifact: &DiskArtifact,
) -> Result<ArtifactTopologyAnalysis, MetricsError> {
    let mut models = Vec::with_capacity(artifact.models().len());
    let mut total = ArtifactTopologyMetrics::default();
    for disk_model in artifact.models() {
        let model = analyze_topology(disk_model.mesh())?;
        let supports = disk_model
            .supports()
            .iter()
            .map(|support| analyze_topology(support.mesh()))
            .collect::<Result<Vec<_>, _>>()?;
        let mut metrics = metrics_for(&model);
        for support in &supports {
            add_metrics(&mut metrics, metrics_for(support));
        }
        add_metrics(&mut total, metrics);
        models.push(ModelTopologyAnalysis {
            source_index: disk_model.source_index(),
            model,
            supports,
            metrics,
        });
    }
    Ok(ArtifactTopologyAnalysis {
        models,
        metrics: total,
    })
}

fn metrics_for(analysis: &TopologyAnalysis) -> ArtifactTopologyMetrics {
    let negative_shells = analysis
        .shells
        .iter()
        .filter(|shell| shell.signed_volume < 0.0)
        .count();
    let mut winding_faces = BTreeSet::new();
    winding_faces.extend(analysis.inconsistent_winding_faces.iter().copied());
    // A coherent but globally inverted shell is also a winding failure.  This
    // is deliberately separate from edge-local consistency: the M1 corruption
    // fixture flips a whole shell and must not evade the winding metric.
    for shell in analysis
        .shells
        .iter()
        .filter(|shell| shell.signed_volume < 0.0)
    {
        winding_faces.extend(shell.faces.iter().copied());
    }
    ArtifactTopologyMetrics {
        boundary_edges: analysis.boundary_edges.len(),
        cavities: analysis.cavities,
        negative_shells,
        winding: winding_faces.len(),
    }
}

fn add_metrics(total: &mut ArtifactTopologyMetrics, item: ArtifactTopologyMetrics) {
    total.boundary_edges += item.boundary_edges;
    total.cavities += item.cavities;
    total.negative_shells += item.negative_shells;
    total.winding += item.winding;
}

fn all_edge_incidences(mesh: &Mesh) -> Vec<EdgeIncidence> {
    let canonical_vertices = canonical_vertex_indices(mesh);
    let mut map: BTreeMap<Edge, Vec<EdgeUse>> = BTreeMap::new();
    for (face, &[a, b, c]) in mesh.triangles().iter().enumerate() {
        for (start, end) in [
            (canonical_vertices[a], canonical_vertices[b]),
            (canonical_vertices[b], canonical_vertices[c]),
            (canonical_vertices[c], canonical_vertices[a]),
        ] {
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

/// Gives repeated triangle-soup coordinates a local topological identity.
///
/// ChituBox stores triangle soup, so a file parsed into [`Mesh`] has distinct
/// indices for the same f32 vertex.  The metric must find its shared edges
/// without mutating/replacing source coordinates or borrowing the producer's
/// weld implementation.  Finite coordinates have already been checked by
/// `Mesh::new`; only numeric zero is canonicalized so `-0.0` and `+0.0` meet
/// geometrically while all other f64 bits remain exact artifact data.
fn canonical_vertex_indices(mesh: &Mesh) -> Vec<usize> {
    #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
    struct Key([u64; 3]);

    let mut ids = BTreeMap::new();
    mesh.vertices()
        .iter()
        .map(|point| {
            let key = Key(point.map(|value| if value == 0.0 { 0 } else { value.to_bits() }));
            let next = ids.len();
            *ids.entry(key).or_insert(next)
        })
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

fn inconsistent_faces(incidences: &[EdgeIncidence]) -> BTreeSet<usize> {
    let mut inconsistent = BTreeSet::new();
    for incidence in incidences {
        if incidence.uses.len() == 2 && incidence.uses[0].forward == incidence.uses[1].forward {
            inconsistent.insert(incidence.uses[0].face);
            inconsistent.insert(incidence.uses[1].face);
        }
    }
    inconsistent
}

fn shell_is_closed(
    faces: &[usize],
    incidences: &[EdgeIncidence],
    inconsistent: &BTreeSet<usize>,
) -> bool {
    let face_set = faces.iter().copied().collect::<BTreeSet<_>>();
    !faces.iter().any(|face| inconsistent.contains(face))
        && incidences
            .iter()
            .filter(|incidence| {
                incidence
                    .uses
                    .iter()
                    .any(|item| face_set.contains(&item.face))
            })
            .all(|incidence| incidence.uses.len() == 2)
}

fn signed_volume(mesh: &Mesh, faces: &[usize]) -> Result<f64, MetricsError> {
    // Serial ordered summation is intentional: float reductions must not change
    // when a future caller changes a thread count.
    let mut sum = 0.0;
    for &face in faces {
        let (a, b, c) = mesh.triangle_points(face);
        let term = dot(a, cross(b, c)) / 6.0;
        if !term.is_finite() {
            return Err(MetricsError::ArithmeticOverflow {
                triangle: face,
                primitive: crate::GeometryPrimitive::SignedVolume,
            });
        }
        sum += term;
        if !sum.is_finite() {
            return Err(MetricsError::ArithmeticOverflow {
                triangle: face,
                primitive: crate::GeometryPrimitive::SignedVolume,
            });
        }
    }
    Ok(sum)
}

fn closed_cavities(mesh: &Mesh, shells: &[Shell]) -> Result<usize, MetricsError> {
    let closed = shells
        .iter()
        .enumerate()
        .filter(|(_, shell)| shell.closed && shell.signed_volume != 0.0)
        .collect::<Vec<_>>();
    let probes = closed
        .iter()
        .map(|(_, shell)| shell_volume_centroid(mesh, shell))
        .collect::<Result<Vec<_>, _>>()?;
    let mut depths = vec![0_usize; closed.len()];
    // Gather all points to test against a shell, perform one batched parity
    // query, then scatter containment votes back into depth counters.
    for (outer_index, (_, outer)) in closed.iter().enumerate() {
        let shell_mesh = shell_mesh(mesh, &outer.faces)?;
        let contains = contains_points(&shell_mesh, &probes)?;
        for (inner_index, contained) in contains.into_iter().enumerate() {
            if inner_index != outer_index && contained {
                depths[inner_index] += 1;
            }
        }
    }
    Ok(depths.into_iter().filter(|depth| depth % 2 == 1).count())
}

fn shell_mesh(mesh: &Mesh, faces: &[usize]) -> Result<Mesh, MetricsError> {
    Mesh::new(
        mesh.vertices().to_vec(),
        faces.iter().map(|&face| mesh.triangles()[face]).collect(),
    )
}

fn shell_volume_centroid(mesh: &Mesh, shell: &Shell) -> Result<Vec3, MetricsError> {
    let mut volume = 0.0;
    let mut weighted = [0.0; 3];
    for &face in &shell.faces {
        let (a, b, c) = mesh.triangle_points(face);
        let tetra_volume = dot(a, cross(b, c)) / 6.0;
        let tetra_centroid = [
            (a[0] + b[0] + c[0]) / 4.0,
            (a[1] + b[1] + c[1]) / 4.0,
            (a[2] + b[2] + c[2]) / 4.0,
        ];
        volume += tetra_volume;
        for component in 0..3 {
            weighted[component] += tetra_volume * tetra_centroid[component];
        }
        if !volume.is_finite() || weighted.iter().any(|value| !value.is_finite()) {
            return Err(MetricsError::ArithmeticOverflow {
                triangle: face,
                primitive: crate::GeometryPrimitive::SignedVolume,
            });
        }
    }
    let centroid = weighted.map(|value| value / volume);
    if centroid.iter().any(|value| !value.is_finite()) {
        return Err(MetricsError::ArithmeticOverflow {
            triangle: shell.faces[0],
            primitive: crate::GeometryPrimitive::SignedVolume,
        });
    }
    Ok(centroid)
}
