//! The ordered-map implementation of [`build_face_topology`] that the sorted
//! and union-find one replaced, kept as the test oracle for it.
//!
//! The production builder was rewritten for speed (sorted vectors and a
//! union-find where this one walks `BTreeMap`s, a hash table in the exact
//! weld, block-staged hashing).  The claim of that rewrite is that every
//! field of the [`FaceTopology`] it returns, digest included, is unchanged,
//! so the proof is this file: the earlier code, verbatim in its logic, run
//! against the production code over many meshes, and compared whole.

#![expect(
    clippy::cast_possible_truncation,
    reason = "test oracle over meshes of a few dozen faces, and the earlier builder's own casts kept as they were; \
              nothing here approaches a u32"
)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use sha2::{Digest, Sha256};

use super::{
    canonical_zero_point, component_labels, hash_bytes, hash_tagged_bytes, hex_digest,
    validate_face_topology, FaceTopology, PointOnlyVertex, TopologyCounts, TopologyDigest,
    TopologyError, TopologySchema, WeldedView, TOPOLOGY_CUT_BOUNDARY, TOPOLOGY_CUT_DEGENERATE,
    TOPOLOGY_CUT_DUPLICATE, TOPOLOGY_CUT_NONMANIFOLD, TOPOLOGY_CUT_POINT_ONLY,
    TOPOLOGY_CUT_SAME_DIRECTION,
};
use crate::{Mesh, WeldPolicy};

fn reference_fingerprint(mesh: &Mesh) -> String {
    let mut hasher = Sha256::new();
    hash_tagged_bytes(
        &mut hasher,
        b"vertices",
        &(mesh.verts().len() as u64).to_le_bytes(),
    );
    for point in mesh.verts() {
        for coordinate in point {
            let canonical = if *coordinate == 0.0 { 0.0 } else { *coordinate };
            hasher.update(canonical.to_bits().to_le_bytes());
        }
    }
    hash_tagged_bytes(
        &mut hasher,
        b"triangles",
        &(mesh.tris().len() as u64).to_le_bytes(),
    );
    for triangle in mesh.tris() {
        for index in triangle {
            hasher.update(index.to_le_bytes());
        }
    }
    hex_digest(hasher)
}

fn reference_u32_array(hasher: &mut Sha256, values: &[u32]) {
    hasher.update((values.len() as u64).to_le_bytes());
    for value in values {
        hasher.update(value.to_le_bytes());
    }
}

fn reference_digest(topology: &FaceTopology) -> TopologyDigest {
    let mut hasher = Sha256::new();
    hasher.update(b"plinth/topology-digest\0");
    hasher.update(TopologySchema::V4.version().to_le_bytes());
    hash_bytes(&mut hasher, topology.mesh_fingerprint.as_bytes());
    reference_u32_array(&mut hasher, &topology.indptr);
    reference_u32_array(&mut hasher, &topology.indices);
    reference_u32_array(&mut hasher, &topology.component_ids);
    hash_bytes(&mut hasher, &topology.cut_flags);
    TopologyDigest {
        schema: TopologySchema::V4,
        hex: hex_digest(hasher),
    }
}

fn reference_exact_weld(points: &[[f64; 3]]) -> WeldedView {
    let mut ids = BTreeMap::<[u64; 3], u32>::new();
    let mut verts = Vec::new();
    let mut map = Vec::with_capacity(points.len());
    for point in points {
        let key = point.map(|coordinate| {
            if coordinate == 0.0 {
                0.0_f64.to_bits()
            } else {
                coordinate.to_bits()
            }
        });
        let id = match ids.get(&key) {
            Some(id) => *id,
            None => {
                let id = u32::try_from(verts.len()).expect("test meshes are small");
                ids.insert(key, id);
                verts.push(canonical_zero_point(*point));
                id
            }
        };
        map.push(id);
    }
    WeldedView {
        vertex_count: verts.len() as u32,
        verts,
        map,
    }
}

/// The ordered-map builder, for the exact-bits weld policy.
#[allow(
    clippy::too_many_lines,
    reason = "the earlier builder, kept whole as an oracle"
)]
fn reference_build(mesh: &Mesh) -> Result<FaceTopology, TopologyError> {
    let fingerprint = reference_fingerprint(mesh);
    let welded = reference_exact_weld(mesh.verts());
    let n_faces = mesh.tris().len();

    let mut cut_flags = vec![0_u8; n_faces];
    let mut canonical_faces = Vec::with_capacity(n_faces);
    for triangle in mesh.tris() {
        canonical_faces.push([
            welded.map[triangle[0] as usize],
            welded.map[triangle[1] as usize],
            welded.map[triangle[2] as usize],
        ]);
    }

    let mut triangles_by_key: BTreeMap<[u32; 3], Vec<usize>> = BTreeMap::new();
    for (face, triangle) in canonical_faces.iter().enumerate() {
        let mut key = *triangle;
        key.sort_unstable();
        triangles_by_key.entry(key).or_default().push(face);
    }
    let mut duplicate_face = vec![false; n_faces];
    let mut counts = TopologyCounts::default();
    for faces in triangles_by_key.values() {
        if faces.len() > 1 {
            counts.duplicate_face_groups += 1;
            counts.duplicate_faces += faces.len();
            for &face in faces {
                duplicate_face[face] = true;
                cut_flags[face] |= TOPOLOGY_CUT_DUPLICATE;
            }
        }
    }

    #[derive(Clone, Copy)]
    struct EdgeOccurrence {
        face: usize,
        forward: bool,
        degenerate: bool,
    }
    let mut edges: BTreeMap<(u32, u32), Vec<EdgeOccurrence>> = BTreeMap::new();
    for (face, triangle) in canonical_faces.iter().enumerate() {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            edges
                .entry((a.min(b), a.max(b)))
                .or_default()
                .push(EdgeOccurrence {
                    face,
                    forward: a < b,
                    degenerate: a == b,
                });
        }
    }

    let mut adjacency = vec![Vec::<u32>::new(); n_faces];
    let mut accepted_edges = Vec::<(usize, usize)>::new();
    for occurrences in edges.values() {
        let degenerate = occurrences.iter().any(|occurrence| occurrence.degenerate);
        let duplicate = occurrences
            .iter()
            .any(|occurrence| cut_flags[occurrence.face] & TOPOLOGY_CUT_DUPLICATE != 0);
        let valid_pair = occurrences.len() == 2
            && occurrences[0].face != occurrences[1].face
            && occurrences[0].forward != occurrences[1].forward
            && !degenerate
            && !duplicate;
        if valid_pair {
            let a = occurrences[0].face;
            let b = occurrences[1].face;
            adjacency[a].push(b as u32);
            adjacency[b].push(a as u32);
            accepted_edges.push((a, b));
            continue;
        }
        for occurrence in occurrences {
            if occurrences.len() == 1 {
                cut_flags[occurrence.face] |= TOPOLOGY_CUT_BOUNDARY;
            }
            if degenerate {
                cut_flags[occurrence.face] |= TOPOLOGY_CUT_DEGENERATE;
            }
            if duplicate {
                cut_flags[occurrence.face] |= TOPOLOGY_CUT_DUPLICATE;
            }
            if occurrences.len() == 2
                && occurrences[0].face != occurrences[1].face
                && occurrences[0].forward == occurrences[1].forward
            {
                cut_flags[occurrence.face] |= TOPOLOGY_CUT_SAME_DIRECTION;
            } else if occurrences.len() != 1
                && !degenerate
                && !duplicate
                && !(occurrences.len() == 2
                    && occurrences[0].face != occurrences[1].face
                    && occurrences[0].forward == occurrences[1].forward)
            {
                cut_flags[occurrence.face] |= TOPOLOGY_CUT_NONMANIFOLD;
            }
        }
    }

    for occurrences in edges.values() {
        let degenerate = occurrences.iter().any(|occurrence| occurrence.degenerate);
        let duplicate = occurrences
            .iter()
            .any(|occurrence| duplicate_face[occurrence.face]);
        if occurrences.len() == 1 {
            counts.boundary_edges += 1;
        }
        if occurrences.len() > 2 && !degenerate && !duplicate {
            counts.nonmanifold_edges += 1;
        }
    }

    let mut incidence: BTreeMap<u32, BTreeMap<usize, BTreeSet<usize>>> = BTreeMap::new();
    for (face, triangle) in canonical_faces.iter().enumerate() {
        for vertex in triangle {
            incidence
                .entry(*vertex)
                .or_default()
                .entry(face)
                .or_default();
        }
    }
    for (a, b) in &accepted_edges {
        let ta = canonical_faces[*a];
        let tb = canonical_faces[*b];
        for vertex in [ta[0], ta[1], ta[2]] {
            if tb.contains(&vertex) {
                let fan = incidence.get_mut(&vertex).expect("face incidence inserted");
                fan.get_mut(a).expect("face incidence inserted").insert(*b);
                fan.get_mut(b).expect("face incidence inserted").insert(*a);
            }
        }
    }
    let mut point_only_vertices = Vec::new();
    for (vertex, fan) in &incidence {
        let mut unseen: BTreeSet<usize> = fan.keys().copied().collect();
        let mut fans: Vec<Vec<u32>> = Vec::new();
        while let Some(&start) = unseen.iter().next() {
            let mut members = Vec::new();
            let mut queue = VecDeque::from([start]);
            unseen.remove(&start);
            while let Some(face) = queue.pop_front() {
                members.push(face as u32);
                for neighbor in &fan[&face] {
                    if unseen.remove(neighbor) {
                        queue.push_back(*neighbor);
                    }
                }
            }
            members.sort_unstable();
            fans.push(members);
        }
        if fans.len() > 1 {
            for face in fan.keys() {
                cut_flags[*face] |= TOPOLOGY_CUT_POINT_ONLY;
            }
            fans.sort_by(|left, right| left[0].cmp(&right[0]));
            point_only_vertices.push(PointOnlyVertex {
                welded_vertex: *vertex,
                fans,
            });
        }
    }

    for flags in &cut_flags {
        if flags & TOPOLOGY_CUT_POINT_ONLY != 0 {
            counts.point_only_cut_faces += 1;
        }
        if flags & TOPOLOGY_CUT_SAME_DIRECTION != 0 {
            counts.same_direction_cut_faces += 1;
        }
        if flags & TOPOLOGY_CUT_DEGENERATE != 0 {
            counts.degenerate_cut_faces += 1;
        }
    }

    for row in &mut adjacency {
        row.sort_unstable();
        row.dedup();
    }
    let mut indptr = Vec::with_capacity(n_faces + 1);
    indptr.push(0);
    let mut indices = Vec::new();
    for row in &adjacency {
        indices.extend(row);
        indptr.push(u32::try_from(indices.len()).map_err(|_| TopologyError::FaceCountOverflow)?);
    }
    let component_ids = component_labels(&adjacency)?;
    let mut topology = FaceTopology {
        indptr,
        indices,
        component_ids,
        cut_flags,
        counts,
        point_only_vertices,
        source_vertex_to_welded: welded.map,
        welded_vertex_count: welded.vertex_count,
        weld_policy: WeldPolicy::ExactBits,
        mesh_fingerprint: fingerprint,
        content_digest: TopologyDigest {
            schema: TopologySchema::V4,
            hex: String::new(),
        },
    };
    topology.content_digest = reference_digest(&topology);
    Ok(topology)
}

/// A deterministic generator: a test oracle needs repeatable cases, not
/// statistical quality.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % bound
    }
}

/// A closed tetrahedron's four faces over the given vertex ids, outward.
fn tetrahedron(ids: [u32; 4]) -> [[u32; 3]; 4] {
    let [a, b, c, d] = ids;
    [[a, c, b], [a, b, d], [b, c, d], [c, a, d]]
}

/// Which of the cases the comparison exists for a corpus actually reached.
#[derive(Default)]
struct Reached {
    pinch_vertices: usize,
    nonmanifold_edges: usize,
    duplicate_faces: usize,
    same_direction_faces: usize,
    degenerate_faces: usize,
    boundary_edges: usize,
    welded_apart: usize,
}

impl Reached {
    fn note(&mut self, mesh: &Mesh, topology: &FaceTopology) {
        self.pinch_vertices += topology.point_only_vertices.len();
        self.nonmanifold_edges += topology.counts.nonmanifold_edges;
        self.duplicate_faces += topology.counts.duplicate_faces;
        self.same_direction_faces += topology.counts.same_direction_cut_faces;
        self.degenerate_faces += topology.counts.degenerate_cut_faces;
        self.boundary_edges += topology.counts.boundary_edges;
        self.welded_apart += mesh.verts().len() - topology.welded_vertex_count as usize;
    }

    /// A corpus that never reaches a case proves nothing about it.
    fn assert_all_reached(&self, what: &str) {
        assert!(self.pinch_vertices > 0, "{what}: no pinch vertex reached");
        assert!(
            self.nonmanifold_edges > 0,
            "{what}: no non-manifold edge reached"
        );
        assert!(
            self.duplicate_faces > 0,
            "{what}: no duplicate face reached"
        );
        assert!(
            self.same_direction_faces > 0,
            "{what}: no same-direction face reached"
        );
        assert!(
            self.degenerate_faces > 0,
            "{what}: no degenerate face reached"
        );
        assert!(self.boundary_edges > 0, "{what}: no boundary edge reached");
        assert!(
            self.welded_apart > 0,
            "{what}: no coincident vertices reached"
        );
    }
}

fn agree(mesh: &Mesh, what: &str, reached: &mut Reached) {
    let reference = reference_build(mesh).expect("reference builds");
    let production =
        super::build_face_topology(mesh, WeldPolicy::ExactBits).expect("production builds");
    assert_eq!(production, reference, "{what}: topology differs");
    assert_eq!(
        validate_face_topology(&production).is_ok(),
        validate_face_topology(&reference).is_ok(),
        "{what}: validation verdict differs"
    );
    reached.note(mesh, &production);
}

#[test]
fn random_triangle_soups_build_the_same_topology_as_the_ordered_map_builder() {
    let mut rng = Lcg(0x5EED_0001);
    let mut reached = Reached::default();
    for case in 0..4000_u32 {
        // Few vertices relative to faces, positions drawn from a coarse grid
        // so duplicate coordinates (welds) and degenerate faces both occur.
        let vertex_count = 4 + rng.next(14) as usize;
        let face_count = 1 + rng.next(40) as usize;
        let verts: Vec<[f64; 3]> = (0..vertex_count)
            .map(|_| {
                [
                    rng.next(4) as f64,
                    rng.next(4) as f64,
                    // `-0.0` and `0.0` must weld together.
                    if rng.next(5) == 0 {
                        -0.0
                    } else {
                        rng.next(3) as f64
                    },
                ]
            })
            .collect();
        let tris: Vec<[u32; 3]> = (0..face_count)
            .map(|_| {
                [
                    rng.next(vertex_count as u64) as u32,
                    rng.next(vertex_count as u64) as u32,
                    rng.next(vertex_count as u64) as u32,
                ]
            })
            .collect();
        let mesh = Mesh::new(verts, tris).expect("indices are in range");
        agree(&mesh, &format!("random soup {case}"), &mut reached);
    }
    reached.assert_all_reached("random soups");
}

#[test]
fn closed_solids_pinches_and_fans_build_the_same_topology_as_the_ordered_map_builder() {
    let mut rng = Lcg(0x5EED_0002);
    let mut reached = Reached::default();
    for case in 0..1500_u32 {
        // Several tetrahedra drawn over a shared small vertex pool: shared
        // vertices give pinch points with two or more fans, shared edges give
        // non-manifold edges, shared whole faces give duplicates.
        let pool = 5 + rng.next(8) as u32;
        let solids = 1 + rng.next(4) as usize;
        let mut tris: Vec<[u32; 3]> = Vec::new();
        for _ in 0..solids {
            let mut ids = [0_u32; 4];
            for slot in &mut ids {
                *slot = rng.next(u64::from(pool)) as u32;
            }
            tris.extend(tetrahedron(ids));
        }
        // Occasionally flip one face so same-direction edges appear.
        if rng.next(3) == 0 {
            let flip = rng.next(tris.len() as u64) as usize;
            tris[flip].swap(0, 1);
        }
        let verts: Vec<[f64; 3]> = (0..pool)
            .map(|index| [f64::from(index), f64::from(index * index % 7), 0.0])
            .collect();
        let mesh = Mesh::new(verts, tris).expect("indices are in range");
        agree(&mesh, &format!("tetrahedra {case}"), &mut reached);
    }
    assert!(reached.pinch_vertices > 0, "no pinch vertex reached");
    assert!(
        reached.nonmanifold_edges > 0,
        "no non-manifold edge reached"
    );
    assert!(reached.duplicate_faces > 0, "no duplicate face reached");
}

#[test]
fn a_soup_with_every_corner_listed_separately_builds_the_same_topology() {
    // The triaged-input shape of the production pipeline: one vertex per
    // corner, welded by coordinates alone.
    let mut rng = Lcg(0x5EED_0003);
    let mut reached = Reached::default();
    for case in 0..300_u32 {
        let solids = 1 + rng.next(3) as usize;
        let mut verts: Vec<[f64; 3]> = Vec::new();
        let mut tris: Vec<[u32; 3]> = Vec::new();
        for _ in 0..solids {
            let corners: Vec<[f64; 3]> = (0..4)
                .map(|_| [rng.next(3) as f64, rng.next(3) as f64, rng.next(3) as f64])
                .collect();
            for face in tetrahedron([0, 1, 2, 3]) {
                let base = verts.len() as u32;
                for corner in face {
                    verts.push(corners[corner as usize]);
                }
                tris.push([base, base + 1, base + 2]);
            }
        }
        let mesh = Mesh::new(verts, tris).expect("indices are in range");
        agree(&mesh, &format!("corner soup {case}"), &mut reached);
    }
    assert!(reached.welded_apart > 0, "no coincident vertices reached");
}

/// One welded vertex shared by `fan_count` fans that touch nowhere else: fan
/// `k` is a strip of `1 + k % 3` faces around the centre over rim vertices of
/// its own, so its faces share edges with each other and with no other fan.
/// Faces are emitted round-robin across the fans, so a fan's faces are not
/// adjacent in face order.
fn many_fan_star(fan_count: usize) -> Mesh {
    let mut verts: Vec<[f64; 3]> = vec![[0.0, 0.0, 0.0]];
    let mut rims: Vec<Vec<u32>> = Vec::with_capacity(fan_count);
    for fan in 0..fan_count {
        let rim: Vec<u32> = (0..2 + fan % 3)
            .map(|_| {
                let id = verts.len() as u32;
                verts.push([f64::from(id), f64::from(id % 97), 1.0]);
                id
            })
            .collect();
        rims.push(rim);
    }
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for round in 0..3 {
        for rim in &rims {
            if round + 1 < rim.len() {
                tris.push([0, rim[round], rim[round + 1]]);
            }
        }
    }
    Mesh::new(verts, tris).expect("indices are in range")
}

#[test]
fn a_vertex_with_many_fans_builds_the_same_topology_as_the_ordered_map_builder() {
    let mesh = many_fan_star(300);
    let production =
        super::build_face_topology(&mesh, WeldPolicy::ExactBits).expect("production builds");
    let centre = production
        .point_only_vertices
        .first()
        .expect("the shared vertex is a pinch");
    assert_eq!(centre.fans.len(), 300);
    // Fans ascend by smallest face, members ascending.
    assert!(centre.fans.windows(2).all(|pair| pair[0][0] < pair[1][0]));
    assert!(centre
        .fans
        .iter()
        .all(|fan| fan.windows(2).all(|pair| pair[0] < pair[1])));
    agree(&mesh, "many-fan star", &mut Reached::default());
}

/// The fan materialization must be linear in the faces at a vertex: a scan of
/// every earlier fan per face costs about 2e10 comparisons at this size,
/// which is tens of seconds; the table lookup takes milliseconds.  The bound is
/// loose enough for a loaded machine and tight enough to catch the quadratic.
#[test]
fn a_vertex_with_two_hundred_thousand_fans_builds_in_linear_time() {
    let mesh = many_fan_star(200_000);
    let start = std::time::Instant::now();
    let topology =
        super::build_face_topology(&mesh, WeldPolicy::ExactBits).expect("production builds");
    let elapsed = start.elapsed();
    assert_eq!(topology.point_only_vertices[0].fans.len(), 200_000);
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "building a 200000-fan vertex took {elapsed:?}"
    );
}

/// The kept fingerprint and exact-bits topology of a mesh are the values a
/// fresh computation gives, on the first call, on a repeat and on a clone, and
/// the keeping never crosses from one mesh to another.
#[test]
fn the_kept_fingerprint_and_topology_equal_fresh_computations() {
    let mut rng = Lcg(0x5EED_0A11);
    // Block boundaries of the chunked fingerprint: vertex and triangle counts
    // just under, at and over its chunk sizes.
    for (vertex_count, face_count) in [
        (3_usize, 1_usize),
        (255, 170),
        (256, 171),
        (257, 512),
        (1024, 1025),
    ] {
        let verts: Vec<[f64; 3]> = (0..vertex_count)
            .map(|_| {
                [
                    rng.next(64) as f64 * 0.25,
                    if rng.next(2) == 0 { -0.0 } else { 0.0 },
                    rng.next(64) as f64 * 0.5 - 8.0,
                ]
            })
            .collect();
        let tris: Vec<[u32; 3]> = (0..face_count)
            .map(|_| {
                [
                    rng.next(vertex_count as u64) as u32,
                    rng.next(vertex_count as u64) as u32,
                    rng.next(vertex_count as u64) as u32,
                ]
            })
            .collect();
        let mesh = Mesh::new(verts, tris).expect("indices are in range");
        let fresh_fingerprint = reference_fingerprint(&mesh);
        let fresh = reference_build(&mesh).expect("reference builds");
        for call in 0..3 {
            assert_eq!(
                super::mesh_fingerprint(&mesh).expect("fingerprint"),
                fresh_fingerprint,
                "{vertex_count} vertices, call {call}"
            );
            assert_eq!(
                super::build_face_topology(&mesh, WeldPolicy::ExactBits).expect("topology"),
                fresh,
                "{vertex_count} vertices, call {call}"
            );
            assert_eq!(
                WeldedView::build(&mesh, WeldPolicy::ExactBits).expect("weld"),
                reference_exact_weld(mesh.verts()),
                "{vertex_count} vertices, call {call}"
            );
        }
        let copy = mesh.clone();
        assert_eq!(copy, mesh);
        assert_eq!(
            super::build_face_topology(&copy, WeldPolicy::ExactBits).expect("copy topology"),
            fresh
        );
        // A different mesh does not see this one's kept values.
        let mut other_verts = mesh.verts().to_vec();
        other_verts[0][0] += 1.0;
        let other = Mesh::new(other_verts, mesh.tris().to_vec()).expect("same indices");
        assert_ne!(
            super::mesh_fingerprint(&other).expect("fingerprint"),
            fresh_fingerprint
        );
        assert_eq!(
            super::build_face_topology(&other, WeldPolicy::ExactBits).expect("topology"),
            reference_build(&other).expect("reference builds")
        );
        // A policy other than exact bits is never served from the kept value.
        assert_eq!(
            super::build_face_topology(&mesh, WeldPolicy::None)
                .expect("soup topology")
                .weld_policy(),
            WeldPolicy::None
        );
    }
}
