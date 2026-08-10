use std::collections::{BTreeMap, BTreeSet, VecDeque};

use plinth_core::{MetricId, MetricSet, MetricValue};
use thiserror::Error;

use crate::{contains_points, cross, dot, DiskArtifact, Mesh, MetricsError, Vec3};

/// The fixed M1 topology metric order from `04-migration-plan.md` §3.
pub const TOPOLOGY_METRIC_ORDER: [&str; 5] = [
    "islands",
    "cavities",
    "boundary_edges",
    "negative_shells",
    "winding",
];

const CAVITY_CONTAINMENT_MIN_NUMERATOR: usize = 9;
const CAVITY_CONTAINMENT_MIN_DENOMINATOR: usize = 10;
const CAVITY_PROBE_LIMIT: usize = 32;

/// An undirected vertex edge. Its endpoints are always in ascending order.
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

/// The winding state of one face-connected shell.
///
/// `Sealed` is intentionally separate from this state: a shell can be
/// watertight while its faces need reorientation. This mirrors the frozen
/// hollow containment rule, which tests closure from incidence counts before
/// considering winding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShellWinding {
    /// A globally outward assignment exists; these original faces differ from it.
    Outward { faces_requiring_flip: Vec<usize> },
    /// An open component cannot establish an outward shell orientation.
    Open { faces: Vec<usize> },
    /// A non-manifold component cannot establish an outward shell orientation.
    NonManifold { faces: Vec<usize> },
    /// Pairwise orientation constraints are mutually contradictory.
    Contradictory { faces: Vec<usize> },
    /// The constraints are satisfiable but have no signed-volume polarity.
    Degenerate { faces: Vec<usize> },
}

impl ShellWinding {
    fn metric_count(&self) -> usize {
        match self {
            Self::Outward {
                faces_requiring_flip,
            } => faces_requiring_flip.len(),
            Self::Open { faces }
            | Self::NonManifold { faces }
            | Self::Contradictory { faces }
            | Self::Degenerate { faces } => faces.len(),
        }
    }
}

/// A face-connected shell with original and outward-corrected signed volume.
#[derive(Clone, Debug, PartialEq)]
pub struct Shell {
    pub faces: Vec<usize>,
    /// Signed tetrahedron sum in the artifact's original orientation.
    pub signed_volume: f64,
    /// Whether every canonical edge has exactly two incident faces. Winding is
    /// deliberately not part of this orientation-blind sealedness predicate.
    pub sealed: bool,
    pub winding: ShellWinding,
    /// The signed volume after selecting the outward global orientation.
    /// `None` means that no trustworthy outward assignment exists.
    pub outward_signed_volume: Option<f64>,
}

/// Independent topology findings used by M1 format metrics.
#[derive(Clone, Debug, PartialEq)]
pub struct TopologyAnalysis {
    pub boundary_edges: Vec<EdgeIncidence>,
    pub nonmanifold_edges: Vec<EdgeIncidence>,
    /// Local edge-orientation disagreements, retained as diagnostic evidence.
    pub inconsistent_winding_faces: Vec<usize>,
    pub shells: Vec<Shell>,
    /// Sealed shells nested at odd depth inside another sealed shell.
    pub cavities: usize,
    /// Indices into `shells` of the negative-signed-volume cavities that have
    /// the *shape* of the one a correct hollow is required to have, ascending.
    /// This is a structural classification, not a licence: a correct hollow
    /// has exactly one, so only one of these is ever exempt from
    /// `negative_shells` and any surplus is counted as a defect. See
    /// [`mandated_cavity_shells`] and [`negative_shell_count`].
    pub mandated_cavity_shells: Vec<usize>,
}

/// The artifact-level portion of the M1 format metrics.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ArtifactTopologyMetrics {
    pub cavities: usize,
    pub boundary_edges: usize,
    pub negative_shells: usize,
    pub winding: usize,
}

/// An independently supplied island count for one disk-read model row.
///
/// M1 wires this count into the fixed score shape but does not lie that it can
/// produce slice-based overhang perception before that implementation exists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TopologyMetricInput {
    pub source_index: usize,
    pub islands: usize,
}

/// One row of M1 metrics in the normative order.
#[derive(Clone, Debug, PartialEq)]
pub struct TopologyMetricRow {
    pub source_index: usize,
    values: [MetricValue; TOPOLOGY_METRIC_ORDER.len()],
}

impl TopologyMetricRow {
    /// Returns the fixed metric entries in the exact M1 order, not map order.
    #[must_use]
    pub fn entries(&self) -> [(MetricId, MetricValue); TOPOLOGY_METRIC_ORDER.len()] {
        std::array::from_fn(|index| {
            (
                MetricId::from(TOPOLOGY_METRIC_ORDER[index]),
                self.values[index],
            )
        })
    }

    /// Converts this ordered transport row into core's passive deterministic map DTO.
    #[must_use]
    pub fn metric_set(&self) -> MetricSet {
        MetricSet::new(self.entries())
    }
}

/// Ordering/analysis errors for the public M1 topology metric boundary.
#[derive(Debug, Error)]
pub enum TopologyMetricError {
    #[error(
        "topology metric input count {actual} does not match disk artifact model count {expected}"
    )]
    InputCount { expected: usize, actual: usize },
    #[error("topology metric input row {row} names source model {actual}, expected {expected}")]
    InputSourceIndex {
        row: usize,
        expected: usize,
        actual: usize,
    },
    #[error(transparent)]
    Analysis(#[from] MetricsError),
}

/// One model's independent topology result, retaining its disk source slot.
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

/// Runs all brute-force edge-incidence, shell, volume, winding, and cavity checks.
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
    let inconsistent_winding_faces = inconsistent_faces(&incidences);
    let shells = shell_faces(mesh, &incidences)
        .into_iter()
        .map(|faces| analyze_shell(mesh, &faces, &incidences))
        .collect::<Result<Vec<_>, _>>()?;
    let nesting = shell_nesting(mesh, &shells)?;
    let cavities = nesting
        .depths
        .iter()
        .filter(|depth| *depth % 2 == 1)
        .count();
    let mandated_cavity_shells = mandated_cavity_shells(&shells, &nesting);
    Ok(TopologyAnalysis {
        boundary_edges,
        nonmanifold_edges,
        inconsistent_winding_faces: inconsistent_winding_faces.into_iter().collect(),
        shells,
        cavities,
        mandated_cavity_shells,
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
/// The caller must obtain `artifact` from [`crate::load_artifacts`]; there is
/// intentionally no path or producer-memory variant.
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
        let mut metrics = topology_metrics(&model);
        for support in &supports {
            add_metrics(&mut metrics, topology_metrics(support));
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

/// Adds independently perceived islands to disk-derived topology metrics.
///
/// Inputs are a batch and must exactly match the artifact's model source order;
/// a mismatched or partial batch is a named error rather than silently paired.
pub fn topology_metric_rows(
    artifact: &DiskArtifact,
    inputs: &[TopologyMetricInput],
) -> Result<Vec<TopologyMetricRow>, TopologyMetricError> {
    if inputs.len() != artifact.models().len() {
        return Err(TopologyMetricError::InputCount {
            expected: artifact.models().len(),
            actual: inputs.len(),
        });
    }
    let topology = analyze_artifact_topology(artifact)?;
    topology
        .models
        .iter()
        .zip(inputs)
        .enumerate()
        .map(|(row, (model, input))| {
            if input.source_index != model.source_index {
                return Err(TopologyMetricError::InputSourceIndex {
                    row,
                    expected: model.source_index,
                    actual: input.source_index,
                });
            }
            let metrics = model.metrics;
            Ok(TopologyMetricRow {
                source_index: model.source_index,
                values: [
                    metric_value(input.islands),
                    metric_value(metrics.cavities),
                    metric_value(metrics.boundary_edges),
                    metric_value(metrics.negative_shells),
                    metric_value(metrics.winding),
                ],
            })
        })
        .collect()
}

fn metric_value(value: usize) -> MetricValue {
    // usize is always finite when represented as f64. Precision is sufficient
    // for mesh counts in the file format; a count this large cannot be loaded.
    MetricValue::new(value as f64).expect("usize converts to finite f64")
}

/// The artifact-level format metrics implied by one shell analysis.
///
/// Public so that the negative-shell and cavity accounting can be exercised
/// directly on an analytic mesh, without routing a fixture through a written
/// `.chitubox` first.
#[must_use]
pub fn topology_metrics(analysis: &TopologyAnalysis) -> ArtifactTopologyMetrics {
    ArtifactTopologyMetrics {
        cavities: analysis.cavities,
        boundary_edges: analysis.boundary_edges.len(),
        // The mandated hollow cavity is exempt: a correct hollow *requires*
        // exactly one negative-signed-volume cavity, so counting it would make
        // the metric's `= 0` bound unsatisfiable for every hollowed model. Only
        // inversion defects are counted. See [`mandated_cavity_shells`].
        //
        // **At most one.** The rule this exemption rests on is
        // `plinth-hollow::validate_shell_topology`'s *exactly one* negative
        // cavity, so the surplus of a multi-cavity mesh is counted back in. A
        // genuine cavity paired with a nested inverted duplicate of the outer
        // surface is structurally indistinguishable from two mandated cavities
        // here, and the `cavities` metric carries no absolute bound, so this
        // independent disk auditor is the only thing that would catch it.
        negative_shells: negative_shell_count(analysis),
        winding: analysis
            .shells
            .iter()
            .map(|shell| shell.winding.metric_count())
            .sum(),
    }
}

/// Inward-wound shells that are not the single cavity a correct hollow is
/// required to have.
///
/// Exactly one mandated cavity is exempt, because that is exactly what
/// `plinth-hollow::validate_shell_topology` proves. Every further one is a
/// surplus negative shell and is counted, so `negative_shells = 0` still
/// refuses an inverted duplicate nested inside a genuine cavity.
fn negative_shell_count(analysis: &TopologyAnalysis) -> usize {
    let mandated = &analysis.mandated_cavity_shells;
    let unexplained = analysis
        .shells
        .iter()
        .enumerate()
        .filter(|(index, shell)| {
            // `mandated_cavity_shells` is ascending by construction.
            shell.signed_volume < 0.0 && mandated.binary_search(index).is_err()
        })
        .count();
    unexplained + mandated.len().saturating_sub(1)
}

fn add_metrics(total: &mut ArtifactTopologyMetrics, item: ArtifactTopologyMetrics) {
    total.cavities += item.cavities;
    total.boundary_edges += item.boundary_edges;
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
/// ChituBox stores triangle soup, so this local independent auditor must find
/// shared edges without mutating source coordinates or borrowing a producer
/// weld implementation. Only numeric zero is canonicalized; all other finite
/// f64 bits remain exact artifact data.
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

fn analyze_shell(
    mesh: &Mesh,
    faces: &[usize],
    incidences: &[EdgeIncidence],
) -> Result<Shell, MetricsError> {
    let original_signed_volume = signed_volume(mesh, faces, None)?;
    let face_set = faces.iter().copied().collect::<BTreeSet<_>>();
    let local_incidences = incidences
        .iter()
        .filter(|incidence| {
            incidence
                .uses
                .iter()
                .any(|item| face_set.contains(&item.face))
        })
        .collect::<Vec<_>>();
    let sealed = local_incidences
        .iter()
        .all(|incidence| incidence.uses.len() == 2);
    let winding = if !sealed {
        if local_incidences
            .iter()
            .any(|incidence| incidence.uses.len() > 2)
        {
            ShellWinding::NonManifold {
                faces: faces.to_vec(),
            }
        } else {
            ShellWinding::Open {
                faces: faces.to_vec(),
            }
        }
    } else {
        solve_outward_winding(mesh, faces, &local_incidences)?
    };
    let outward_signed_volume = match &winding {
        ShellWinding::Outward {
            faces_requiring_flip,
        } => {
            let flips = faces_requiring_flip
                .iter()
                .copied()
                .collect::<BTreeSet<_>>();
            Some(signed_volume(mesh, faces, Some(&flips))?.abs())
        }
        ShellWinding::Open { .. }
        | ShellWinding::NonManifold { .. }
        | ShellWinding::Contradictory { .. }
        | ShellWinding::Degenerate { .. } => None,
    };
    Ok(Shell {
        faces: faces.to_vec(),
        signed_volume: original_signed_volume,
        sealed,
        winding,
        outward_signed_volume,
    })
}

fn solve_outward_winding(
    mesh: &Mesh,
    faces: &[usize],
    incidences: &[&EdgeIncidence],
) -> Result<ShellWinding, MetricsError> {
    let mut neighbors = BTreeMap::<usize, Vec<(usize, bool)>>::new();
    for &EdgeIncidence { uses, .. } in incidences {
        let left = uses[0];
        let right = uses[1];
        // Equal directions need one face reversed; opposing directions need
        // equal flip parity.
        let xor_required = left.forward == right.forward;
        neighbors
            .entry(left.face)
            .or_default()
            .push((right.face, xor_required));
        neighbors
            .entry(right.face)
            .or_default()
            .push((left.face, xor_required));
    }
    let mut flips = BTreeMap::<usize, bool>::new();
    for &start in faces {
        if flips.contains_key(&start) {
            continue;
        }
        flips.insert(start, false);
        let mut queue = VecDeque::from([start]);
        while let Some(face) = queue.pop_front() {
            let flip = flips[&face];
            for &(neighbor, xor_required) in neighbors.get(&face).into_iter().flatten() {
                let expected = flip ^ xor_required;
                match flips.get(&neighbor) {
                    Some(actual) if *actual != expected => {
                        return Ok(ShellWinding::Contradictory {
                            faces: faces.to_vec(),
                        });
                    }
                    Some(_) => {}
                    None => {
                        flips.insert(neighbor, expected);
                        queue.push_back(neighbor);
                    }
                }
            }
        }
    }
    let initial_flips = flips
        .iter()
        .filter_map(|(&face, &flipped)| flipped.then_some(face))
        .collect::<BTreeSet<_>>();
    let corrected = signed_volume(mesh, faces, Some(&initial_flips))?;
    if corrected == 0.0 {
        return Ok(ShellWinding::Degenerate {
            faces: faces.to_vec(),
        });
    }
    let faces_requiring_flip = if corrected > 0.0 {
        initial_flips.into_iter().collect()
    } else {
        faces
            .iter()
            .copied()
            .filter(|face| !initial_flips.contains(face))
            .collect()
    };
    Ok(ShellWinding::Outward {
        faces_requiring_flip,
    })
}

fn signed_volume(
    mesh: &Mesh,
    faces: &[usize],
    flipped: Option<&BTreeSet<usize>>,
) -> Result<f64, MetricsError> {
    // Serial ordered summation is intentional: float reductions cannot vary
    // with a future caller's thread count.
    let mut sum = 0.0;
    for &face in faces {
        let (a, b, c) = mesh.triangle_points(face);
        let mut term = dot(a, cross(b, c)) / 6.0;
        if flipped.is_some_and(|set| set.contains(&face)) {
            term = -term;
        }
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

/// Per-shell containment nesting, indexed exactly like [`TopologyAnalysis::shells`].
struct Nesting {
    /// How many other sealed shells enclose this shell. Unsealed shells are 0.
    depths: Vec<usize>,
    /// The innermost enclosing shell index, when this shell is enclosed at all.
    innermost_container: Vec<Option<usize>>,
}

fn shell_nesting(mesh: &Mesh, shells: &[Shell]) -> Result<Nesting, MetricsError> {
    let sealed_indices = shells
        .iter()
        .enumerate()
        .filter(|(_, shell)| shell.sealed)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let sealed = sealed_indices
        .iter()
        .map(|&index| &shells[index])
        .collect::<Vec<_>>();
    let bounds = sealed
        .iter()
        .map(|shell| shell_bounds(mesh, shell))
        .collect::<Result<Vec<_>, _>>()?;
    let probes = sealed
        .iter()
        .map(|shell| shell_boundary_probes(mesh, shell))
        .collect::<Vec<_>>();
    let mut depths = vec![0_usize; sealed.len()];
    // Containers in ascending outer-shell order, so the innermost selection
    // below is order-independent and therefore deterministic.
    let mut containers: Vec<Vec<usize>> = vec![Vec::new(); sealed.len()];
    for (outer_index, outer) in sealed.iter().enumerate() {
        let shell_mesh = shell_mesh(mesh, &outer.faces)?;
        let mut gathered = Vec::new();
        let mut rows = Vec::new();
        for (inner_index, _) in sealed.iter().enumerate() {
            if outer_index == inner_index || !bounds[outer_index].contains(bounds[inner_index]) {
                continue;
            }
            let start = gathered.len();
            gathered.extend_from_slice(&probes[inner_index]);
            rows.push((inner_index, start, gathered.len()));
        }
        // Different shells require distinct triangle sets, so one global ray
        // traversal is not meaningful. This is the required batch boundary:
        // all candidate probe points for this outer shell go through exactly
        // one independent parity call, then results scatter by saved ranges.
        let contained = contains_points(&shell_mesh, &gathered)?;
        for (inner_index, start, end) in rows {
            let inside = contained[start..end].iter().filter(|&&value| value).count();
            let count = end - start;
            if inside * CAVITY_CONTAINMENT_MIN_DENOMINATOR
                >= count * CAVITY_CONTAINMENT_MIN_NUMERATOR
            {
                depths[inner_index] += 1;
                containers[inner_index].push(outer_index);
            }
        }
    }

    // Scatter the sealed-only result back onto the full shell list.
    let mut shell_depths = vec![0_usize; shells.len()];
    let mut innermost_container = vec![None; shells.len()];
    for (sealed_index, &shell_index) in sealed_indices.iter().enumerate() {
        shell_depths[shell_index] = depths[sealed_index];
        // The innermost container is the enclosing shell that is itself
        // enclosed by the most others; ties keep the lowest shell index.
        innermost_container[shell_index] = containers[sealed_index]
            .iter()
            .copied()
            .max_by_key(|&container| (depths[container], std::cmp::Reverse(container)))
            .map(|container| sealed_indices[container]);
    }
    Ok(Nesting {
        depths: shell_depths,
        innermost_container,
    })
}

/// Shells shaped like the one negative-signed-volume cavity a correct hollow
/// is *required* to have, rather than like an inversion defect.
///
/// This function classifies; it does not bound. The cardinality rule — at most
/// one such cavity is exempt, because `validate_shell_topology` proves exactly
/// one — is applied in [`negative_shell_count`], which counts every surplus
/// back in.
///
/// `plinth-hollow` proves a drilled shell only when the post-Boolean mesh is
/// exactly one negative-signed-volume cavity component enclosed by positive
/// outer material (`validate_shell_topology`). Counting that mandated cavity as
/// a `negative_shells` defect made the metric's `= 0` bound unsatisfiable for
/// every correctly hollowed model, which is a defect in the counter and not in
/// the bound. The exemption is narrow and structural: the shell must be sealed,
/// enclosed at odd depth, and its innermost enclosing shell must have positive
/// signed volume. An inverted outer shell is at depth 0 and keeps its count; a
/// mesh whose outer shell is inverted *and* whose cavity is wound outward still
/// counts the inverted outer shell, so the bound still catches it.
fn mandated_cavity_shells(shells: &[Shell], nesting: &Nesting) -> Vec<usize> {
    shells
        .iter()
        .enumerate()
        .filter(|(index, shell)| {
            shell.sealed
                && shell.signed_volume < 0.0
                && nesting.depths[*index] % 2 == 1
                && nesting.innermost_container[*index]
                    .is_some_and(|container| shells[container].signed_volume > 0.0)
        })
        .map(|(index, _)| index)
        .collect()
}

#[derive(Clone, Copy)]
struct Bounds {
    lower: Vec3,
    upper: Vec3,
}

impl Bounds {
    fn contains(self, other: Self) -> bool {
        (0..3).all(|component| {
            self.lower[component] <= other.lower[component]
                && other.upper[component] <= self.upper[component]
        })
    }
}

fn shell_bounds(mesh: &Mesh, shell: &Shell) -> Result<Bounds, MetricsError> {
    let mut lower = [f64::INFINITY; 3];
    let mut upper = [f64::NEG_INFINITY; 3];
    for point in shell_vertices(mesh, shell) {
        for component in 0..3 {
            lower[component] = lower[component].min(point[component]);
            upper[component] = upper[component].max(point[component]);
        }
    }
    if lower
        .iter()
        .chain(upper.iter())
        .any(|value| !value.is_finite())
    {
        return Err(MetricsError::ArithmeticOverflow {
            triangle: shell.faces[0],
            primitive: crate::GeometryPrimitive::SignedVolume,
        });
    }
    Ok(Bounds { lower, upper })
}

fn shell_boundary_probes(mesh: &Mesh, shell: &Shell) -> Vec<Vec3> {
    let mut seen = BTreeSet::new();
    let mut probes = Vec::new();
    for point in shell_vertices(mesh, shell) {
        let key = point.map(|value| if value == 0.0 { 0 } else { value.to_bits() });
        if seen.insert(key) {
            probes.push(point);
            if probes.len() == CAVITY_PROBE_LIMIT {
                break;
            }
        }
    }
    probes
}

fn shell_vertices<'a>(mesh: &'a Mesh, shell: &'a Shell) -> impl Iterator<Item = Vec3> + 'a {
    shell.faces.iter().flat_map(|&face| {
        let [a, b, c] = mesh.triangles()[face];
        [mesh.vertices()[a], mesh.vertices()[b], mesh.vertices()[c]]
    })
}

fn shell_mesh(mesh: &Mesh, faces: &[usize]) -> Result<Mesh, MetricsError> {
    Mesh::new(
        mesh.vertices().to_vec(),
        faces.iter().map(|&face| mesh.triangles()[face]).collect(),
    )
}
