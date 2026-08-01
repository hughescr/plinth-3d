//! Deliberate component triage before hollowing.
//!
//! This is not a repair pass.  It only removes tightly qualified modelling
//! shards and refuses a closed shell proved to be inside another shell's
//! material.  The latter is the safe response to a pre-hollowed model: carving
//! it again would destroy its wall and emit trapped resin.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use thiserror::Error;

use crate::topology::{TopologyError, WeldedView};
use crate::{Mesh, MeshError, WeldPolicy};

pub const HOLLOW_SHARD_MAX_FACES: usize = 64;
pub const HOLLOW_SHARD_MAX_DIM_MM: f64 = 1.0;
pub const HOLLOW_SHARD_MAX_VOLUME_MM3: f64 = 0.01;
pub const HOLLOW_CONTAINMENT_SAMPLES: usize = 32;
pub const HOLLOW_CONTAINMENT_MIN_FRACTION: f64 = 0.9;

#[derive(Clone, Debug, PartialEq)]
pub struct PreHollowed {
    cavity_components: Vec<ComponentDescription>,
}

impl PreHollowed {
    #[must_use]
    pub fn cavity_components(&self) -> &[ComponentDescription] {
        &self.cavity_components
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentDescription {
    component_id: usize,
    face_count: usize,
    dims_mm: [f64; 3],
}

impl ComponentDescription {
    #[must_use]
    pub const fn component_id(&self) -> usize {
        self.component_id
    }
    #[must_use]
    pub const fn face_count(&self) -> usize {
        self.face_count
    }
    #[must_use]
    pub const fn dims_mm(&self) -> [f64; 3] {
        self.dims_mm
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriageSummary {
    component_count: usize,
    main_component: usize,
    stripped_components: Vec<usize>,
}

impl TriageSummary {
    #[must_use]
    pub const fn component_count(&self) -> usize {
        self.component_count
    }
    #[must_use]
    pub const fn main_component(&self) -> usize {
        self.main_component
    }
    #[must_use]
    pub fn stripped_components(&self) -> &[usize] {
        &self.stripped_components
    }
}

/// A triage-stage mesh whose policy is truthful: its returned arrays are
/// really welded, unlike the source `Mesh` loaded from an STL.
#[derive(Clone, Debug, PartialEq)]
pub struct TriagedMesh {
    mesh: Mesh,
    summary: TriageSummary,
}

impl TriagedMesh {
    #[must_use]
    pub const fn mesh(&self) -> &Mesh {
        &self.mesh
    }
    #[must_use]
    pub const fn summary(&self) -> &TriageSummary {
        &self.summary
    }
    #[must_use]
    pub fn into_mesh(self) -> Mesh {
        self.mesh
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum TriageError {
    #[error(transparent)]
    Topology(#[from] TopologyError),
    #[error(transparent)]
    Mesh(#[from] MeshError),
    #[error("triage requires an actual welding policy, not WeldPolicy::None")]
    UnweldedInput,
    #[error("pre-hollowed input: {0:?}")]
    PreHollowed(PreHollowed),
    #[error("ray-parity arithmetic failed for containment")]
    RayArithmetic,
}

/// Weld, classify, strip only measured shards, and refuse sealed cavities.
///
/// `WeldPolicy::ExactBits` mirrors the legacy `merge_vertices()` behavior;
/// callers may deliberately select an epsilon weld instead.  `None` is
/// rejected rather than quietly labelling an unwelded triangle soup as welded.
pub fn triage_components(mesh: &Mesh, weld_policy: WeldPolicy) -> Result<TriagedMesh, TriageError> {
    if matches!(weld_policy, WeldPolicy::None) {
        return Err(TriageError::UnweldedInput);
    }
    let view = WeldedView::build(mesh, weld_policy)?;
    let faces = remap_faces(mesh, &view.map);
    let components = face_components(&faces);
    let component_count = components
        .iter()
        .copied()
        .max()
        .map_or(0, |value| value + 1);
    debug_assert!(component_count > 0, "Mesh rejects empty triangle arrays");
    let details = ComponentDetail::collect(&view.verts, &faces, &components, component_count);
    let main = details
        .iter()
        .enumerate()
        .max_by(|(left_id, left), (right_id, right)| {
            left.diagonal()
                .total_cmp(&right.diagonal())
                .then_with(|| left.face_ids.len().cmp(&right.face_ids.len()))
                .then_with(|| left_id.cmp(right_id))
        })
        .map(|(id, _)| id)
        .expect("nonempty mesh has a component");

    let foreign_edges = components_with_foreign_edge(&faces, &components);
    let main_vertices = details[main].vertices.clone();
    let mut shards = BTreeSet::new();
    for (component, detail) in details.iter().enumerate() {
        if component == main {
            continue;
        }
        if !foreign_edges.contains(&component)
            && detail.touches_any(&main_vertices)
            && detail.inside_bbox(&details[main])
            && detail.face_ids.len() <= HOLLOW_SHARD_MAX_FACES
            && detail.max_dim() <= HOLLOW_SHARD_MAX_DIM_MM
            && detail.is_watertight(&faces)
            && detail.signed_volume(&view.verts, &faces).abs() <= HOLLOW_SHARD_MAX_VOLUME_MM3
        {
            shards.insert(component);
        }
    }

    let remaining = (0..component_count)
        .filter(|component| !shards.contains(component))
        .collect::<Vec<_>>();
    let closed = remaining
        .iter()
        .copied()
        .filter(|component| *component != main && details[*component].is_watertight(&faces))
        .collect::<Vec<_>>();
    let mut cavities = BTreeSet::new();
    // Gather all candidate boundary probes for one container before running a
    // single batched parity query, then scatter the boolean answers back.
    for container in remaining
        .iter()
        .copied()
        .filter(|component| details[*component].is_watertight(&faces))
    {
        let candidates = closed
            .iter()
            .copied()
            .filter(|candidate| {
                *candidate != container && details[*candidate].inside_bbox(&details[container])
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            continue;
        }
        let mut gathered = Vec::new();
        let mut ranges = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let start = gathered.len();
            gathered
                .extend(details[candidate].sample_points(&view.verts, HOLLOW_CONTAINMENT_SAMPLES));
            ranges.push((candidate, start, gathered.len()));
        }
        let inside =
            contains_points_batch(&view.verts, &faces, &details[container].face_ids, &gathered)?;
        for (candidate, start, end) in ranges {
            let samples = end - start;
            if samples > 0
                && inside[start..end].iter().filter(|value| **value).count() as f64 / samples as f64
                    >= HOLLOW_CONTAINMENT_MIN_FRACTION
            {
                cavities.insert(candidate);
            }
        }
    }
    if !cavities.is_empty() {
        let descriptions = cavities
            .into_iter()
            .map(|component| ComponentDescription {
                component_id: component,
                face_count: details[component].face_ids.len(),
                dims_mm: details[component].dims(),
            })
            .collect();
        return Err(TriageError::PreHollowed(PreHollowed {
            cavity_components: descriptions,
        }));
    }

    let kept_faces = faces
        .iter()
        .enumerate()
        .filter_map(|(face, triangle)| (!shards.contains(&components[face])).then_some(*triangle))
        .collect::<Vec<_>>();
    let compacted = compact_mesh(&view.verts, &kept_faces, weld_policy)?;
    Ok(TriagedMesh {
        mesh: compacted,
        summary: TriageSummary {
            component_count,
            main_component: main,
            stripped_components: shards.into_iter().collect(),
        },
    })
}

fn remap_faces(mesh: &Mesh, map: &[u32]) -> Vec<[u32; 3]> {
    mesh.tris()
        .iter()
        .map(|triangle| {
            [
                map[triangle[0] as usize],
                map[triangle[1] as usize],
                map[triangle[2] as usize],
            ]
        })
        .collect()
}

fn face_components(faces: &[[u32; 3]]) -> Vec<usize> {
    let mut by_edge = BTreeMap::<(u32, u32), Vec<usize>>::new();
    for (face, triangle) in faces.iter().enumerate() {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            by_edge.entry((a.min(b), a.max(b))).or_default().push(face);
        }
    }
    let mut adjacency = vec![Vec::new(); faces.len()];
    for occurrences in by_edge.values() {
        if occurrences.len() == 2 && occurrences[0] != occurrences[1] {
            adjacency[occurrences[0]].push(occurrences[1]);
            adjacency[occurrences[1]].push(occurrences[0]);
        }
    }
    let mut labels = vec![usize::MAX; faces.len()];
    let mut next = 0;
    for start in 0..faces.len() {
        if labels[start] != usize::MAX {
            continue;
        }
        labels[start] = next;
        let mut queue = VecDeque::from([start]);
        while let Some(face) = queue.pop_front() {
            for &neighbor in &adjacency[face] {
                if labels[neighbor] == usize::MAX {
                    labels[neighbor] = next;
                    queue.push_back(neighbor);
                }
            }
        }
        next += 1;
    }
    labels
}

fn components_with_foreign_edge(faces: &[[u32; 3]], components: &[usize]) -> BTreeSet<usize> {
    let mut by_edge = BTreeMap::<(u32, u32), BTreeSet<usize>>::new();
    for (face, triangle) in faces.iter().enumerate() {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            by_edge
                .entry((a.min(b), a.max(b)))
                .or_default()
                .insert(components[face]);
        }
    }
    by_edge
        .into_values()
        .filter(|owners| owners.len() > 1)
        .flatten()
        .collect()
}

#[derive(Clone, Debug)]
struct ComponentDetail {
    face_ids: Vec<usize>,
    vertices: BTreeSet<u32>,
    lo: [f64; 3],
    hi: [f64; 3],
}

impl ComponentDetail {
    fn collect(
        verts: &[[f64; 3]],
        faces: &[[u32; 3]],
        labels: &[usize],
        count: usize,
    ) -> Vec<Self> {
        let mut result = (0..count)
            .map(|_| Self {
                face_ids: Vec::new(),
                vertices: BTreeSet::new(),
                lo: [f64::INFINITY; 3],
                hi: [f64::NEG_INFINITY; 3],
            })
            .collect::<Vec<_>>();
        for (face, triangle) in faces.iter().enumerate() {
            let detail = &mut result[labels[face]];
            detail.face_ids.push(face);
            for vertex in triangle {
                detail.vertices.insert(*vertex);
                for (axis, coordinate) in verts[*vertex as usize].iter().enumerate() {
                    detail.lo[axis] = detail.lo[axis].min(*coordinate);
                    detail.hi[axis] = detail.hi[axis].max(*coordinate);
                }
            }
        }
        result
    }
    fn dims(&self) -> [f64; 3] {
        [
            self.hi[0] - self.lo[0],
            self.hi[1] - self.lo[1],
            self.hi[2] - self.lo[2],
        ]
    }
    fn max_dim(&self) -> f64 {
        self.dims().into_iter().fold(0.0, f64::max)
    }
    fn diagonal(&self) -> f64 {
        self.dims()
            .into_iter()
            .map(|value| value * value)
            .sum::<f64>()
            .sqrt()
    }
    fn touches_any(&self, other: &BTreeSet<u32>) -> bool {
        self.vertices.iter().any(|vertex| other.contains(vertex))
    }
    fn inside_bbox(&self, other: &Self) -> bool {
        (0..3).all(|axis| {
            self.lo[axis] >= other.lo[axis] - 1e-9 && self.hi[axis] <= other.hi[axis] + 1e-9
        })
    }
    fn is_watertight(&self, faces: &[[u32; 3]]) -> bool {
        let mut counts = BTreeMap::<(u32, u32), usize>::new();
        for &face in &self.face_ids {
            let triangle = faces[face];
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                *counts.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        counts.values().all(|count| *count == 2)
    }
    fn signed_volume(&self, verts: &[[f64; 3]], faces: &[[u32; 3]]) -> f64 {
        self.face_ids
            .iter()
            .map(|face| {
                let [a, b, c] = faces[*face];
                dot(
                    verts[a as usize],
                    cross(verts[b as usize], verts[c as usize]),
                ) / 6.0
            })
            .sum()
    }
    fn sample_points(&self, verts: &[[f64; 3]], limit: usize) -> Vec<[f64; 3]> {
        let ordered = self.vertices.iter().copied().collect::<Vec<_>>();
        let count = ordered.len().min(limit);
        (0..count)
            .map(|sample| verts[ordered[sample * ordered.len() / count] as usize])
            .collect()
    }
}

/// One parity operation over a gathered point batch.  This is deliberately
/// private: callers cannot accidentally turn containment into scalar calls.
fn contains_points_batch(
    verts: &[[f64; 3]],
    faces: &[[u32; 3]],
    container_faces: &[usize],
    points: &[[f64; 3]],
) -> Result<Vec<bool>, TriageError> {
    const DIRECTION: [f64; 3] = [1.0, 0.371_390_676_354_103_7, 0.137_503_523_749_935_02];
    let mut result = Vec::with_capacity(points.len());
    for point in points {
        let mut hits = Vec::new();
        for face in container_faces {
            let [a, b, c] = faces[*face];
            if let Some(distance) = ray_triangle_distance(
                *point,
                DIRECTION,
                verts[a as usize],
                verts[b as usize],
                verts[c as usize],
            )? {
                hits.push(distance);
            }
        }
        hits.sort_by(f64::total_cmp);
        hits.dedup_by(|left, right| {
            (*left - *right).abs() <= 1e-9 * left.abs().max(right.abs()).max(1.0)
        });
        result.push(hits.len() % 2 == 1);
    }
    Ok(result)
}

fn ray_triangle_distance(
    origin: [f64; 3],
    direction: [f64; 3],
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
) -> Result<Option<f64>, TriageError> {
    let edge_a = subtract(b, a);
    let edge_b = subtract(c, a);
    let p = cross(direction, edge_b);
    let determinant = dot(edge_a, p);
    if !determinant.is_finite() {
        return Err(TriageError::RayArithmetic);
    }
    if determinant.abs() < 1e-12 {
        return Ok(None);
    }
    let inverse = determinant.recip();
    let offset = subtract(origin, a);
    let u = dot(offset, p) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return Ok(None);
    }
    let q = cross(offset, edge_a);
    let v = dot(direction, q) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return Ok(None);
    }
    let distance = dot(edge_b, q) * inverse;
    if !distance.is_finite() {
        return Err(TriageError::RayArithmetic);
    }
    Ok((distance > 1e-9).then_some(distance))
}

fn compact_mesh(
    verts: &[[f64; 3]],
    faces: &[[u32; 3]],
    policy: WeldPolicy,
) -> Result<Mesh, TriageError> {
    let mut map = BTreeMap::new();
    let mut compact = Vec::new();
    let mut output = Vec::with_capacity(faces.len());
    for triangle in faces {
        let mut remapped = [0_u32; 3];
        for (corner, vertex) in triangle.iter().enumerate() {
            let id = match map.get(vertex) {
                Some(id) => *id,
                None => {
                    let id = u32::try_from(compact.len())
                        .map_err(|_| TopologyError::VertexCountOverflow)?;
                    map.insert(*vertex, id);
                    compact.push(verts[*vertex as usize]);
                    id
                }
            };
            remapped[corner] = id;
        }
        output.push(remapped);
    }
    Ok(Mesh::new_welded(compact, output, policy)?)
}

fn subtract(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}
fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}
fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}
