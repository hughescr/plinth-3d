//! Frozen pre-P8 joint-ball placement and exact enclosure certificates.
//!
//! The implementation is deliberately conservative: a ball is removed only
//! when every spherical patch is certified covered by a frustum whose solid
//! contains the ball centre.  Any numerical, depth, work, or memory limit
//! leaves the ball in the result.

use std::collections::{BTreeMap, BTreeSet};

use plinth_core::{Finite, PositiveFinite};
use thiserror::Error;

use crate::forest::{EmbedEnd, Forest, Member, MemberKey, MemberKind, Owner};

const MODEL_FOOT_FACTOR: f64 = 1.4;
const COINCIDE_TOL_MM: f64 = 0.15;
const ON_AXIS_TOL_MM: f64 = 0.2;
const PLATE_BAND_MM: f64 = 0.2;
const COLINEAR_RADIUS_FRAC: f64 = 0.20;
const COLINEAR_COS_MAX: f64 = -0.984_807_753_012_208;
const ENCLOSURE_PROOF_TOL_MM: f64 = 0.018;
const DEGENERATE_LENGTH_SQUARED: f64 = 1e-18;

/// Bounded current-parity policy.  The defaults are the frozen Python limits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointConfig {
    plate_z_mm: Finite,
    joint_ball_factor: PositiveFinite,
    max_depth: u8,
    max_patches: usize,
    max_work_items: usize,
    max_scratch_bytes: usize,
}

impl JointConfig {
    pub fn current(plate_z_mm: Finite, joint_ball_factor: PositiveFinite) -> Self {
        Self {
            plate_z_mm,
            joint_ball_factor,
            max_depth: 22,
            max_patches: 2_000_000,
            max_work_items: 50_000_000,
            max_scratch_bytes: 512 * 1024 * 1024,
        }
    }

    #[cfg(test)]
    fn with_limits(mut self, max_depth: u8, max_patches: usize) -> Self {
        self.max_depth = max_depth;
        self.max_patches = max_patches;
        self
    }

    fn fingerprint(self) -> ConfigFingerprint {
        ConfigFingerprint {
            plate_z: self.plate_z_mm.get().to_bits(),
            factor: self.joint_ball_factor.get().to_bits(),
            max_depth: self.max_depth,
            max_patches: self.max_patches,
            max_work_items: self.max_work_items,
            max_scratch_bytes: self.max_scratch_bytes,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConfigFingerprint {
    plate_z: u64,
    factor: u64,
    max_depth: u8,
    max_patches: usize,
    max_work_items: usize,
    max_scratch_bytes: usize,
}

/// A retained ball.  There is no public constructor: output geometry remains
/// inseparable from the proof-bound plan which authorized it.
#[derive(Clone, Debug, PartialEq)]
pub struct JointBall {
    center_mm: [Finite; 3],
    radius_mm: PositiveFinite,
    owner_key: MemberKey,
    owner: Owner,
}

impl JointBall {
    pub const fn center_mm(&self) -> [Finite; 3] {
        self.center_mm
    }
    pub const fn radius_mm(&self) -> PositiveFinite {
        self.radius_mm
    }
    pub fn owner(&self) -> &Owner {
        &self.owner
    }
    pub const fn owner_key(&self) -> MemberKey {
        self.owner_key
    }
}

/// Opaque result bound to the exact live forest and all result-shaping policy.
#[derive(Clone, Debug)]
pub struct JointPlan {
    forest_digest: u64,
    config: ConfigFingerprint,
    balls: Vec<JointBall>,
    certified_dropped: usize,
}

impl JointPlan {
    pub fn validate<'a>(
        &'a self,
        forest: &Forest,
        config: JointConfig,
    ) -> Result<&'a [JointBall], JointError> {
        if self.forest_digest != forest.digest() || self.config != config.fingerprint() {
            return Err(JointError::StaleAuthority);
        }
        Ok(&self.balls)
    }

    pub const fn certified_dropped(&self) -> usize {
        self.certified_dropped
    }

    /// Stable semantic digest used by the thread-count determinism gate.
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut digest = 0xcbf2_9ce4_8422_2325_u64;
        let mut byte = |value: u8| {
            digest ^= u64::from(value);
            digest = digest.wrapping_mul(0x0000_0100_0000_01b3);
        };
        for value in self.forest_digest.to_le_bytes() {
            byte(value);
        }
        for value in self.config.plate_z.to_le_bytes() {
            byte(value);
        }
        for value in self.config.factor.to_le_bytes() {
            byte(value);
        }
        byte(self.config.max_depth);
        for value in u64::try_from(self.config.max_patches)
            .expect("platform usize fits u64")
            .to_le_bytes()
        {
            byte(value);
        }
        for value in u64::try_from(self.config.max_work_items)
            .expect("platform usize fits u64")
            .to_le_bytes()
        {
            byte(value);
        }
        for value in u64::try_from(self.config.max_scratch_bytes)
            .expect("platform usize fits u64")
            .to_le_bytes()
        {
            byte(value);
        }
        for value in u64::try_from(self.balls.len())
            .expect("platform usize fits u64")
            .to_le_bytes()
        {
            byte(value);
        }
        for ball in &self.balls {
            for coordinate in ball.center_mm {
                for value in coordinate.get().to_bits().to_le_bytes() {
                    byte(value);
                }
            }
            for value in ball.radius_mm.get().to_bits().to_le_bytes() {
                byte(value);
            }
            for value in ball.owner_key.slot().to_le_bytes() {
                byte(value);
            }
            for value in ball.owner_key.generation().to_le_bytes() {
                byte(value);
            }
            match &ball.owner {
                Owner::Contact(contact) => {
                    byte(0);
                    for value in contact.to_le_bytes() {
                        byte(value);
                    }
                }
                Owner::ModelAnchor(anchor) => {
                    byte(1);
                    for value in anchor.to_le_bytes() {
                        byte(value);
                    }
                }
                Owner::Generated { family, ordinal } => {
                    byte(2);
                    for value in u64::try_from(family.len())
                        .expect("platform usize fits u64")
                        .to_le_bytes()
                    {
                        byte(value);
                    }
                    for value in family.bytes() {
                        byte(value);
                    }
                    for value in ordinal.to_le_bytes() {
                        byte(value);
                    }
                }
            }
        }
        for value in u64::try_from(self.certified_dropped)
            .expect("platform usize fits u64")
            .to_le_bytes()
        {
            byte(value);
        }
        digest
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum JointError {
    #[error("joint policy has a zero bound or exceeds the frozen certificate depth")]
    InvalidLimits,
    #[error("joint planning would exceed its typed work limit")]
    WorkLimit,
    #[error("joint planning would exceed its typed scratch-memory limit")]
    ScratchLimit,
    #[error("joint result contains non-finite or non-positive geometry")]
    InvalidGeometry,
    #[error("joint plan authority is stale or belongs to another input")]
    StaleAuthority,
    #[error(transparent)]
    NativeBatch(#[from] geom_kernel::PointSegmentBatchError),
    #[error(transparent)]
    OrderedMap(#[from] geom_kernel::OrderedMapError),
}

#[derive(Clone)]
struct LiveMember {
    key: MemberKey,
    p0: [f64; 3],
    p1: [f64; 3],
    r0: f64,
    r1: f64,
    kind: MemberKind,
    owner: Owner,
    embed_end: Option<EmbedEnd>,
}

impl LiveMember {
    fn from_forest(key: MemberKey, member: &Member) -> Self {
        Self {
            key,
            p0: member.p0_mm().map(Finite::get),
            p1: member.p1_mm().map(Finite::get),
            r0: member.terminal_radius_mm().get().get(),
            r1: member.rootward_radius_mm().get().get(),
            kind: member.kind().clone(),
            owner: member.owner().clone(),
            embed_end: member.embed_end(),
        }
    }
}

#[derive(Clone)]
struct Node {
    point: [f64; 3],
    radius: f64,
    owner: Owner,
    embedded: bool,
    outward: [f64; 3],
    member: usize,
}

#[derive(Clone)]
struct CandidateBall {
    center: [f64; 3],
    radius: f64,
    owner_key: MemberKey,
    owner: Owner,
    touching: Vec<usize>,
}

/// Build all current pre-P8 joint balls from one immutable live-forest view.
/// Retired and quarantined members are unreachable through `live_members`.
pub fn plan_joint_balls(forest: &Forest, config: JointConfig) -> Result<JointPlan, JointError> {
    validate_limits(config)?;
    let keys = forest.live_members_in_creation_order();
    let members: Vec<_> = keys
        .into_iter()
        .filter_map(|key| {
            forest
                .member(key)
                .map(|member| LiveMember::from_forest(key, member))
        })
        .collect();
    let forest_digest = forest.digest();
    let config_fingerprint = config.fingerprint();
    if members.is_empty() {
        return Ok(JointPlan {
            forest_digest,
            config: config_fingerprint,
            balls: Vec::new(),
            certified_dropped: 0,
        });
    }

    let nodes = gather_nodes(&members, config.plate_z_mm.get());
    if nodes.is_empty() {
        return Ok(JointPlan {
            forest_digest,
            config: config_fingerprint,
            balls: Vec::new(),
            certified_dropped: 0,
        });
    }
    let clusters = cluster_nodes(&nodes);
    let axis_members: Vec<_> = members
        .iter()
        .enumerate()
        .filter_map(|(index, member)| (member.kind != MemberKind::Stub).then_some(index))
        .collect();
    let pairs = clusters
        .len()
        .checked_mul(axis_members.len())
        .ok_or(JointError::WorkLimit)?;
    if pairs > config.max_work_items {
        return Err(JointError::WorkLimit);
    }
    let scratch = pairs.checked_mul(16).ok_or(JointError::ScratchLimit)?;
    if scratch > config.max_scratch_bytes {
        return Err(JointError::ScratchLimit);
    }

    let centers: Vec<_> = clusters
        .iter()
        .map(|cluster| cluster_center(cluster, &nodes))
        .collect();
    let segment_starts: Vec<_> = axis_members
        .iter()
        .map(|index| members[*index].p0)
        .collect();
    let segment_ends: Vec<_> = axis_members
        .iter()
        .map(|index| members[*index].p1)
        .collect();
    let start_radii: Vec<_> = axis_members
        .iter()
        .map(|index| members[*index].r0)
        .collect();
    let end_radii: Vec<_> = axis_members
        .iter()
        .map(|index| members[*index].r1)
        .collect();
    let axis_batch = geom_kernel::point_segment_taper_batch(
        &centers,
        &segment_starts,
        &segment_ends,
        geom_kernel::FrozenJointTaperBatch::new(&start_radii, &end_radii),
        geom_kernel::PointSegmentBatchLimits::new(pairs, scratch),
    )?;
    debug_assert_eq!(
        axis_batch.effective_threads(),
        geom_kernel::current_threads()
    );
    let mut candidates = Vec::new();
    for (cluster_index, cluster) in clusters.iter().enumerate() {
        if is_colinear_seam(cluster, &nodes) {
            continue;
        }
        let center = centers[cluster_index];
        let fattest_node = cluster
            .iter()
            .copied()
            .reduce(|best, candidate| {
                if nodes[candidate].radius > nodes[best].radius {
                    candidate
                } else {
                    best
                }
            })
            .expect("cluster nonempty");
        let mut radius = nodes[fattest_node].radius;
        let mut owner = None;
        let mut touching: BTreeSet<usize> = cluster.iter().map(|i| nodes[*i].member).collect();
        let distance_row = axis_batch
            .distance_row(cluster_index)
            .expect("native batch retains one row per gathered cluster");
        let radius_row = axis_batch
            .local_radius_row(cluster_index)
            .expect("native batch retains one radius row per gathered cluster");
        for (axis_col, member_index) in axis_members.iter().copied().enumerate() {
            if distance_row[axis_col] <= ON_AXIS_TOL_MM {
                touching.insert(member_index);
                let local = radius_row[axis_col];
                if local >= radius {
                    radius = local;
                    owner = Some((
                        members[member_index].key,
                        members[member_index].owner.clone(),
                    ));
                }
            }
        }
        let embedded = cluster.iter().any(|i| nodes[*i].embedded);
        let mut ball_radius = config.joint_ball_factor.get() * radius;
        if embedded && cluster.len() == 1 {
            ball_radius = MODEL_FOOT_FACTOR * nodes[fattest_node].radius;
        }
        if center[2] - ball_radius < config.plate_z_mm.get() {
            continue;
        }
        candidates.push(CandidateBall {
            center,
            radius: ball_radius,
            owner_key: owner
                .as_ref()
                .map_or(members[nodes[fattest_node].member].key, |(key, _)| *key),
            owner: owner
                .map(|(_, owner)| owner)
                .unwrap_or_else(|| nodes[fattest_node].owner.clone()),
            touching: touching.into_iter().collect(),
        });
    }

    let decisions = geom_kernel::ordered_index_map(
        &candidates,
        geom_kernel::OrderedMapLimits::new(config.max_work_items, config.max_scratch_bytes),
        |_, ball| ball_enclosed(ball, &members, config),
    )?;
    debug_assert_eq!(
        decisions.effective_threads(),
        geom_kernel::current_threads()
    );
    let mut balls = Vec::new();
    let mut certified_dropped = 0;
    for (candidate, enclosed) in candidates.into_iter().zip(decisions.into_values()) {
        if enclosed {
            certified_dropped += 1;
            continue;
        }
        balls.push(JointBall {
            center_mm: candidate
                .center
                .map(|value| Finite::new(value).map_err(|_| JointError::InvalidGeometry))
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?
                .try_into()
                .map_err(|_| JointError::InvalidGeometry)?,
            radius_mm: PositiveFinite::new(candidate.radius)
                .map_err(|_| JointError::InvalidGeometry)?,
            owner_key: candidate.owner_key,
            owner: candidate.owner,
        });
    }
    Ok(JointPlan {
        forest_digest,
        config: config_fingerprint,
        balls,
        certified_dropped,
    })
}

fn validate_limits(config: JointConfig) -> Result<(), JointError> {
    if config.max_depth > 22
        || config.max_patches == 0
        || config.max_work_items == 0
        || config.max_scratch_bytes == 0
    {
        return Err(JointError::InvalidLimits);
    }
    Ok(())
}

fn gather_nodes(members: &[LiveMember], plate_z: f64) -> Vec<Node> {
    let mut nodes = Vec::new();
    for (index, member) in members.iter().enumerate() {
        let d = sub(member.p1, member.p0);
        let length = norm(d);
        let unit = if length > 1e-9 {
            scale(d, 1.0 / length)
        } else {
            [0.0; 3]
        };
        if member.kind != MemberKind::Stub && (member.p0[2] - plate_z).abs() > PLATE_BAND_MM {
            nodes.push(Node {
                point: member.p0,
                radius: member.r0,
                owner: member.owner.clone(),
                embedded: member.embed_end == Some(EmbedEnd::Terminalward),
                outward: unit,
                member: index,
            });
        }
        if (member.p1[2] - plate_z).abs() > PLATE_BAND_MM {
            nodes.push(Node {
                point: member.p1,
                radius: member.r1,
                owner: member.owner.clone(),
                embedded: member.embed_end == Some(EmbedEnd::Rootward),
                outward: scale(unit, -1.0),
                member: index,
            });
        }
    }
    nodes
}

fn cluster_nodes(nodes: &[Node]) -> Vec<Vec<usize>> {
    let mut parent: Vec<_> = (0..nodes.len()).collect();
    for a in 0..nodes.len() {
        for b in (a + 1)..nodes.len() {
            if norm(sub(nodes[a].point, nodes[b].point)) <= COINCIDE_TOL_MM {
                let ra = find_root(&mut parent, a);
                let rb = find_root(&mut parent, b);
                if ra != rb {
                    parent[rb] = ra;
                }
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for index in 0..nodes.len() {
        let root = find_root(&mut parent, index);
        groups.entry(root).or_default().push(index);
    }
    groups.into_values().collect()
}

fn find_root(parent: &mut [usize], mut index: usize) -> usize {
    while parent[index] != index {
        parent[index] = parent[parent[index]];
        index = parent[index];
    }
    index
}

fn cluster_center(cluster: &[usize], nodes: &[Node]) -> [f64; 3] {
    let index = cluster
        .iter()
        .copied()
        .reduce(|best, candidate| {
            if nodes[candidate].radius > nodes[best].radius {
                candidate
            } else {
                best
            }
        })
        .expect("cluster nonempty");
    nodes[index].point
}

fn is_colinear_seam(cluster: &[usize], nodes: &[Node]) -> bool {
    if cluster.len() != 2 {
        return false;
    }
    let a = &nodes[cluster[0]];
    let b = &nodes[cluster[1]];
    let cosine = dot(a.outward, b.outward);
    let high = a.radius.max(b.radius);
    let close = high <= 1e-12 || high - a.radius.min(b.radius) <= COLINEAR_RADIUS_FRAC * high;
    cosine <= COLINEAR_COS_MAX && close
}

fn ball_enclosed(ball: &CandidateBall, members: &[LiveMember], config: JointConfig) -> bool {
    if ball.radius <= 0.0 || ball.touching.is_empty() {
        return false;
    }
    let credited: Vec<_> = ball
        .touching
        .iter()
        .copied()
        .filter(|index| {
            signed_distance_to_frustum(ball.center, &members[*index]) <= ENCLOSURE_PROOF_TOL_MM
        })
        .collect();
    if credited.is_empty() {
        return false;
    }
    certify_sphere(ball, &credited, members, config)
}

#[derive(Clone, Copy)]
struct Patch {
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
    depth: u8,
}

fn certify_sphere(
    ball: &CandidateBall,
    credited: &[usize],
    members: &[LiveMember],
    config: JointConfig,
) -> bool {
    let mut patches = icosahedron();
    let mut total = patches.len();
    if total > config.max_patches {
        return false;
    }
    let mut work = 0_usize;
    while !patches.is_empty() {
        work = match work.checked_add(patches.len().saturating_mul(credited.len())) {
            Some(value) if value <= config.max_work_items => value,
            _ => return false,
        };
        let mut theta = Vec::with_capacity(patches.len());
        let mut points = Vec::with_capacity(patches.len());
        for patch in &patches {
            let direction = normalize(add(add(patch.a, patch.b), patch.c));
            theta.push(
                angle(direction, patch.a)
                    .max(angle(direction, patch.b))
                    .max(angle(direction, patch.c)),
            );
            points.push(add(ball.center, scale(direction, ball.radius)));
        }
        let mut best = vec![f64::NEG_INFINITY; patches.len()];
        let mut signed = vec![0.0; patches.len()];
        // Gather one complete patch frontier, make one batched distance call
        // per credited member, then scatter into the union maximum.
        for member in credited {
            signed_distances_to_frustum(&points, &members[*member], &mut signed);
            for (destination, distance) in best.iter_mut().zip(&signed) {
                *destination = destination.max(ENCLOSURE_PROOF_TOL_MM - *distance);
            }
        }
        let mut unresolved = Vec::new();
        for ((patch, best), theta) in patches.into_iter().zip(best).zip(theta) {
            if best < 0.0 {
                return false;
            }
            if best >= ball.radius * theta {
                continue;
            }
            if patch.depth >= config.max_depth {
                return false;
            }
            unresolved.push(patch);
        }
        if unresolved.is_empty() {
            return true;
        }
        total = match total.checked_add(3 * unresolved.len()) {
            Some(value) => value,
            None => return false,
        };
        if total > config.max_patches {
            return false;
        }
        let scratch = match unresolved
            .len()
            .checked_mul(4)
            .and_then(|count| count.checked_mul(std::mem::size_of::<Patch>()))
        {
            Some(value) => value,
            None => return false,
        };
        if scratch > config.max_scratch_bytes {
            return false;
        }
        let mut children = Vec::with_capacity(unresolved.len() * 4);
        for patch in unresolved {
            let ab = normalize(add(patch.a, patch.b));
            let bc = normalize(add(patch.b, patch.c));
            let ca = normalize(add(patch.c, patch.a));
            let depth = patch.depth + 1;
            children.extend([
                Patch {
                    a: patch.a,
                    b: ab,
                    c: ca,
                    depth,
                },
                Patch {
                    a: patch.b,
                    b: bc,
                    c: ab,
                    depth,
                },
                Patch {
                    a: patch.c,
                    b: ca,
                    c: bc,
                    depth,
                },
                Patch {
                    a: ab,
                    b: bc,
                    c: ca,
                    depth,
                },
            ]);
        }
        patches = children;
    }
    true
}

fn signed_distances_to_frustum(points: &[[f64; 3]], member: &LiveMember, output: &mut [f64]) {
    debug_assert_eq!(points.len(), output.len());
    for (point, destination) in points.iter().copied().zip(output) {
        *destination = signed_distance_to_frustum(point, member);
    }
}

fn signed_distance_to_frustum(point: [f64; 3], member: &LiveMember) -> f64 {
    let d = sub(member.p1, member.p0);
    let l2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if l2 < DEGENERATE_LENGTH_SQUARED {
        return norm(sub(point, member.p0)) - member.r0;
    }
    let length = l2.sqrt();
    let axis = scale(d, 1.0 / length);
    let delta = sub(point, member.p0);
    let axial = dot(delta, axis);
    let radial_vector = sub(sub(point, member.p0), scale(axis, axial));
    let radial = (radial_vector[0] * radial_vector[0]
        + radial_vector[1] * radial_vector[1]
        + radial_vector[2] * radial_vector[2])
        .sqrt();
    let cap0 = segment_distance_2d(axial, radial, 0.0, 0.0, 0.0, member.r0);
    let slant = segment_distance_2d(axial, radial, 0.0, member.r0, length, member.r1);
    let cap1 = segment_distance_2d(axial, radial, length, member.r1, length, 0.0);
    let distance = cap0.min(slant).min(cap1);
    let local_radius = member.r0 + (member.r1 - member.r0) * axial.clamp(0.0, length) / length;
    if axial >= 0.0 && axial <= length && radial <= local_radius {
        -distance
    } else {
        distance
    }
}

fn segment_distance_2d(qx: f64, qy: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx = bx - ax;
    let dy = by - ay;
    let l2 = dx * dx + dy * dy;
    if l2 < DEGENERATE_LENGTH_SQUARED {
        return (qx - ax).hypot(qy - ay);
    }
    let t = (((qx - ax) * dx + (qy - ay) * dy) / l2).clamp(0.0, 1.0);
    (qx - (ax + t * dx)).hypot(qy - (ay + t * dy))
}

fn icosahedron() -> Vec<Patch> {
    let phi = (1.0 + 5.0_f64.sqrt()) / 2.0;
    let mut vertices = Vec::new();
    for a in [-1.0, 1.0] {
        for b in [-1.0, 1.0] {
            vertices.extend([[0.0, a, b * phi], [a, b * phi, 0.0], [b * phi, 0.0, a]]);
        }
    }
    for vertex in &mut vertices {
        *vertex = normalize(*vertex);
    }
    let mut nearest = f64::INFINITY;
    for i in 0..vertices.len() {
        for j in (i + 1)..vertices.len() {
            nearest = nearest.min(norm(sub(vertices[i], vertices[j])));
        }
    }
    let adjacent = |i: usize, j: usize| norm(sub(vertices[i], vertices[j])) <= nearest * 1.01;
    let mut faces = Vec::new();
    for i in 0..vertices.len() {
        let neighbors: Vec<_> = ((i + 1)..vertices.len())
            .filter(|j| adjacent(i, *j))
            .collect();
        for a in 0..neighbors.len() {
            for b in (a + 1)..neighbors.len() {
                if adjacent(neighbors[a], neighbors[b]) {
                    faces.push(Patch {
                        a: vertices[i],
                        b: vertices[neighbors[a]],
                        c: vertices[neighbors[b]],
                        depth: 0,
                    });
                }
            }
        }
    }
    debug_assert_eq!(faces.len(), 20);
    faces
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
fn normalize(a: [f64; 3]) -> [f64; 3] {
    scale(a, 1.0 / norm(a))
}
fn angle(a: [f64; 3], b: [f64; 3]) -> f64 {
    dot(a, b).clamp(-1.0, 1.0).acos()
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;
    use crate::forest::{MemberKey, MemberSpec, RootwardEndRadius, TerminalEndRadius};

    fn finite3(value: [f64; 3]) -> [Finite; 3] {
        value.map(|v| Finite::new(v).unwrap())
    }
    fn radius(value: f64) -> PositiveFinite {
        PositiveFinite::new(value).unwrap()
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "test fixture helper mirrors the frozen member schema field-for-field"
    )]
    fn add_member(
        forest: &mut Forest,
        p0: [f64; 3],
        p1: [f64; 3],
        r0: f64,
        r1: f64,
        kind: MemberKind,
        owner: Owner,
        embed_end: Option<EmbedEnd>,
    ) -> MemberKey {
        forest.add_member(Member::new(MemberSpec {
            p0_mm: finite3(p0),
            p1_mm: finite3(p1),
            r0_mm: TerminalEndRadius::new(radius(r0)),
            r1_mm: RootwardEndRadius::new(radius(r1)),
            kind,
            owner,
            embed_end,
            carried_mm2: Finite::new(0.0).unwrap(),
        }))
    }
    fn config(plate: f64) -> JointConfig {
        JointConfig::current(Finite::new(plate).unwrap(), radius(1.0))
    }

    #[test]
    fn mid_column_enclosure_drops_but_elbow_direct_witness_keeps() {
        let mut forest = Forest::new();
        add_member(
            &mut forest,
            [4.0, 0.0, -5.0],
            [4.0, 0.0, 30.0],
            2.0,
            2.0,
            MemberKind::Trunk,
            Owner::Generated {
                family: "trunk".into(),
                ordinal: 0,
            },
            None,
        );
        add_member(
            &mut forest,
            [10.0, 0.0, 10.0],
            [4.0, 0.0, 10.0],
            0.3,
            0.3,
            MemberKind::Brace,
            Owner::Generated {
                family: "brace".into(),
                ordinal: 0,
            },
            None,
        );
        add_member(
            &mut forest,
            [20.0, 0.0, 0.0],
            [20.0, 0.0, 10.0],
            0.6,
            0.6,
            MemberKind::Trunk,
            Owner::Generated {
                family: "trunk".into(),
                ordinal: 1,
            },
            None,
        );
        add_member(
            &mut forest,
            [20.0, 0.0, 10.0],
            [30.0, 0.0, 10.0],
            0.6,
            0.6,
            MemberKind::Trunk,
            Owner::Generated {
                family: "trunk".into(),
                ordinal: 2,
            },
            None,
        );
        let plan = plan_joint_balls(&forest, config(-100.0)).unwrap();
        let balls = plan.validate(&forest, config(-100.0)).unwrap();
        assert!(plan.certified_dropped() >= 1);
        assert!(balls
            .iter()
            .any(|ball| ball.center_mm().map(Finite::get) == [20.0, 0.0, 10.0]));
        assert!(!balls
            .iter()
            .any(|ball| ball.center_mm().map(Finite::get) == [4.0, 0.0, 10.0]));
    }

    #[test]
    fn credited_member_restriction_keeps_jittered_real_hub() {
        let mut forest = Forest::new();
        let rows = [
            (
                [
                    -0.0012763566500100013,
                    -0.02018765375559623,
                    -0.0255239036087158,
                ],
                [0.8935354235585666, -9.178482782952365, -0.33559696897299834],
                0.37905493221448316,
            ),
            (
                [
                    -0.03286932321566698,
                    0.016295075739409165,
                    -0.046784730941915216,
                ],
                [-3.7516483645842533, 5.112004744985008, -2.272011145171266],
                0.3855562378313358,
            ),
            (
                [
                    -0.07496186943470244,
                    0.00031463602499591867,
                    0.012583727280577122,
                ],
                [1.8226767721828085, 3.1898157854197215, -3.832257784358506],
                0.3126512418157271,
            ),
            (
                [
                    -0.0006109511611611406,
                    -0.02637707564935988,
                    -0.04956143384038976,
                ],
                [1.6189135678109392, -4.498665682157377, 9.512731015295648],
                0.38313887477261743,
            ),
        ];
        for (ordinal, (p0, p1, r)) in rows.into_iter().enumerate() {
            add_member(
                &mut forest,
                p0,
                p1,
                r,
                r,
                MemberKind::Trunk,
                Owner::Generated {
                    family: "trunk".into(),
                    ordinal: ordinal as u32,
                },
                None,
            );
        }
        let plan = plan_joint_balls(&forest, config(-1000.0)).unwrap();
        let ball = plan
            .validate(&forest, config(-1000.0))
            .unwrap()
            .iter()
            .find(|b| norm(b.center_mm().map(Finite::get)) < 0.2)
            .unwrap();
        assert_eq!(ball.radius_mm().get().to_bits(), 0x3fd8_acf4_1210_47d9);
    }

    #[test]
    fn frozen_noncoplanar_taper_gap_is_kept() {
        let mut forest = Forest::new();
        add_member(
            &mut forest,
            [0.9364904936757851, 0.2714744007119751, 0.752930226115027],
            [0.0, 0.0, 0.0],
            0.04431473021227952,
            0.16790109202464978,
            MemberKind::Trunk,
            Owner::Contact(0),
            None,
        );
        add_member(
            &mut forest,
            [0.0, 0.0, 0.0],
            [
                -1.0818439371612572,
                -0.3694209493362457,
                -0.45908450006900064,
            ],
            0.1676098823931195,
            0.6619919334695765,
            MemberKind::Trunk,
            Owner::Contact(1),
            None,
        );
        let plan = plan_joint_balls(&forest, config(-1000.0)).unwrap();
        let ball = plan
            .validate(&forest, config(-1000.0))
            .unwrap()
            .iter()
            .find(|ball| norm(ball.center_mm().map(Finite::get)) < 1e-6)
            .expect("the measured 0.023 mm gap must retain its joint ball");
        assert_eq!(ball.radius_mm().get().to_bits(), 0x3fc5_7dc8_719a_b201);
    }

    #[test]
    fn real_plate_crossing_specimen_is_dropped() {
        let mut forest = Forest::new();
        let plate_z = -66.97575;
        add_member(
            &mut forest,
            [13.40, -7.33, -40.0],
            [13.40, -7.33, -66.17],
            0.8268,
            0.8268,
            MemberKind::Trunk,
            Owner::Contact(0),
            None,
        );
        let cfg = config(plate_z);
        let plan = plan_joint_balls(&forest, cfg).unwrap();
        assert!(!plan
            .validate(&forest, cfg)
            .unwrap()
            .iter()
            .any(|ball| { ball.center_mm().map(Finite::get) == [13.40, -7.33, -66.17] }));
    }

    #[test]
    fn taper_distance_and_batch_arithmetic_are_component_ordered() {
        let mut forest = Forest::new();
        let key = add_member(
            &mut forest,
            [0.0, 0.0, 0.0],
            [3.0, 4.0, 12.0],
            0.25,
            1.25,
            MemberKind::Trunk,
            Owner::Contact(0),
            None,
        );
        let member = LiveMember::from_forest(key, forest.member(key).unwrap());
        let centers = [[1.5, 2.0, 6.0], [10.0, -2.0, 1.0]];
        let batch = geom_kernel::point_segment_taper_batch(
            &centers,
            &[member.p0],
            &[member.p1],
            geom_kernel::FrozenJointTaperBatch::new(&[member.r0], &[member.r1]),
            geom_kernel::PointSegmentBatchLimits::new(2, 32),
        )
        .unwrap();
        let distance = [
            batch.distance_row(0).unwrap()[0],
            batch.distance_row(1).unwrap()[0],
        ];
        let radius = [
            batch.local_radius_row(0).unwrap()[0],
            batch.local_radius_row(1).unwrap()[0],
        ];
        assert_eq!(distance[0].to_bits(), 0x0000_0000_0000_0000);
        assert_eq!(radius[0].to_bits(), 0x3fe8_0000_0000_0000);
        assert_eq!(
            distance[1].to_bits(),
            dist_and_radius_scalar(centers[1], &member).0.to_bits()
        );
        assert_eq!(
            radius[1].to_bits(),
            dist_and_radius_scalar(centers[1], &member).1.to_bits()
        );
        let endpoint = signed_distance_to_frustum([0.0, 0.0, 0.0], &member);
        assert!(endpoint.is_sign_negative());
    }

    #[test]
    fn coincidence_boundary_and_equal_radius_center_ties_are_frozen() {
        let node = |point, ordinal| Node {
            point,
            radius: 0.5,
            owner: Owner::Contact(ordinal),
            embedded: false,
            outward: [0.0, 0.0, 1.0],
            member: ordinal as usize,
        };
        let inclusive = vec![node([-0.0, 0.0, 1.0], 0), node([0.15, 0.0, 1.0], 1)];
        let clusters = cluster_nodes(&inclusive);
        assert_eq!(clusters, vec![vec![0, 1]]);
        let center = cluster_center(&clusters[0], &inclusive);
        assert!(center[0].is_sign_negative());

        let outside = vec![
            node([0.0, 0.0, 1.0], 0),
            node([f64::from_bits(0.15_f64.to_bits() + 1), 0.0, 1.0], 1),
        ];
        assert_eq!(cluster_nodes(&outside), vec![vec![0], vec![1]]);
    }

    #[test]
    fn retire_reuse_keeps_python_birth_order_for_center_owner_and_ball_order() {
        let mut forest = Forest::new();
        let first = add_member(
            &mut forest,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 10.0],
            0.6,
            0.6,
            MemberKind::Trunk,
            Owner::Contact(10),
            None,
        );
        let filler = add_member(
            &mut forest,
            [100.0, 0.0, 0.0],
            [100.0, 0.0, 1.0],
            0.2,
            0.2,
            MemberKind::Shoot,
            Owner::Contact(99),
            None,
        );
        let older_at_joint = add_member(
            &mut forest,
            [0.0, 0.0, 10.0],
            [10.0, 0.0, 10.0],
            0.6,
            0.6,
            MemberKind::Shoot,
            Owner::Contact(11),
            None,
        );
        forest.retire(filler).unwrap();
        let later_reused = add_member(
            &mut forest,
            [0.0, 0.0, 10.0],
            [0.0, 10.0, 10.0],
            0.6,
            0.6,
            MemberKind::Strut,
            Owner::Contact(12),
            None,
        );
        assert_eq!(later_reused.slot(), filler.slot());
        assert_eq!(
            forest.live_members_in_creation_order(),
            vec![first, older_at_joint, later_reused]
        );
        assert_eq!(
            forest.live_members(),
            vec![first, later_reused, older_at_joint]
        );

        let cfg = config(-100.0);
        let plan = plan_joint_balls(&forest, cfg).unwrap();
        let joint = plan
            .validate(&forest, cfg)
            .unwrap()
            .iter()
            .find(|ball| ball.center_mm().map(Finite::get) == [0.0, 0.0, 10.0])
            .unwrap();
        assert_eq!(joint.owner_key(), later_reused);
        assert_eq!(joint.owner(), &Owner::Contact(12));
    }

    #[test]
    fn plan_digest_covers_config_ball_owner_key_generation_and_every_result_field() {
        let mut forest = Forest::new();
        let first = add_member(
            &mut forest,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            0.2,
            0.2,
            MemberKind::Trunk,
            Owner::Contact(1),
            None,
        );
        let old = add_member(
            &mut forest,
            [2.0, 0.0, 0.0],
            [2.0, 0.0, 1.0],
            0.2,
            0.2,
            MemberKind::Trunk,
            Owner::Contact(1),
            None,
        );
        forest.retire(old).unwrap();
        let reused = add_member(
            &mut forest,
            [3.0, 0.0, 0.0],
            [3.0, 0.0, 1.0],
            0.2,
            0.2,
            MemberKind::Trunk,
            Owner::Contact(1),
            None,
        );
        assert_eq!(old.slot(), reused.slot());
        assert_ne!(old.generation(), reused.generation());
        let ball = JointBall {
            center_mm: finite3([1.0, 2.0, 3.0]),
            radius_mm: radius(0.5),
            owner_key: old,
            owner: Owner::Contact(7),
        };
        let base = JointPlan {
            forest_digest: 5,
            config: config(-100.0).fingerprint(),
            balls: vec![ball],
            certified_dropped: 2,
        };
        let base_digest = base.digest();
        let mut variants = Vec::new();
        let mut changed = base.clone();
        changed.forest_digest += 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.config.plate_z ^= 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.config.factor ^= 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.config.max_depth -= 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.config.max_patches -= 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.config.max_work_items -= 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.config.max_scratch_bytes -= 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.balls[0].center_mm[0] = Finite::new(1.5).unwrap();
        variants.push(changed);
        let mut changed = base.clone();
        changed.balls[0].radius_mm = radius(0.6);
        variants.push(changed);
        let mut changed = base.clone();
        changed.balls[0].owner_key = first;
        variants.push(changed);
        let mut changed = base.clone();
        changed.balls[0].owner_key = reused;
        variants.push(changed);
        let mut changed = base.clone();
        changed.balls[0].owner = Owner::Contact(8);
        variants.push(changed);
        let mut changed = base.clone();
        changed.certified_dropped += 1;
        variants.push(changed);
        assert!(variants
            .iter()
            .all(|variant| variant.digest() != base_digest));
    }

    fn dist_and_radius_scalar(center: [f64; 3], member: &LiveMember) -> (f64, f64) {
        let d = sub(member.p1, member.p0);
        let l2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        let rel = sub(center, member.p0);
        if l2 < DEGENERATE_LENGTH_SQUARED {
            return (norm(rel), member.r0);
        }
        let t = ((rel[0] * d[0] + rel[1] * d[1] + rel[2] * d[2]) / l2).clamp(0.0, 1.0);
        let e = [
            center[0] - (member.p0[0] + t * d[0]),
            center[1] - (member.p0[1] + t * d[1]),
            center[2] - (member.p0[2] + t * d[2]),
        ];
        (
            (e[0] * e[0] + e[1] * e[1] + e[2] * e[2]).sqrt(),
            member.r0 + (member.r1 - member.r0) * t,
        )
    }

    #[test]
    fn cap_and_depth_boundary_keep_when_uncertified() {
        let mut forest = Forest::new();
        add_member(
            &mut forest,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 10.0],
            0.6,
            0.6,
            MemberKind::Trunk,
            Owner::Contact(0),
            None,
        );
        add_member(
            &mut forest,
            [0.0, 0.0, 10.0],
            [10.0, 0.0, 10.0],
            0.6,
            0.6,
            MemberKind::Trunk,
            Owner::Contact(1),
            None,
        );
        let bounded = config(-100.0).with_limits(0, 20);
        let plan = plan_joint_balls(&forest, bounded).unwrap();
        assert!(plan
            .validate(&forest, bounded)
            .unwrap()
            .iter()
            .any(|b| b.center_mm().map(Finite::get) == [0.0, 0.0, 10.0]));

        let live_key = forest.live_members_in_creation_order()[0];
        let live = LiveMember::from_forest(live_key, forest.member(live_key).unwrap());
        let easy = CandidateBall {
            center: [0.0, 0.0, 5.0],
            radius: 0.01,
            owner_key: live_key,
            owner: Owner::Contact(0),
            touching: vec![0],
        };
        assert!(!certify_sphere(
            &easy,
            &[0],
            std::slice::from_ref(&live),
            config(-100.0).with_limits(22, 19)
        ));
        assert!(certify_sphere(
            &easy,
            &[0],
            std::slice::from_ref(&live),
            config(-100.0).with_limits(22, 20)
        ));
        assert!(matches!(
            plan_joint_balls(&forest, config(-100.0).with_limits(23, 20)),
            Err(JointError::InvalidLimits)
        ));
        assert!(matches!(
            plan_joint_balls(&forest, config(-100.0).with_limits(1, 0)),
            Err(JointError::InvalidLimits)
        ));
    }

    #[test]
    fn authority_binds_geometry_config_and_lifecycle() {
        let mut forest = Forest::new();
        let key = add_member(
            &mut forest,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 10.0],
            0.5,
            0.5,
            MemberKind::Trunk,
            Owner::Contact(0),
            None,
        );
        let cfg = config(-100.0);
        let plan = plan_joint_balls(&forest, cfg).unwrap();
        assert_eq!(
            plan.validate(&forest, config(-99.0)),
            Err(JointError::StaleAuthority)
        );
        forest.retire(key).unwrap();
        assert_eq!(plan.validate(&forest, cfg), Err(JointError::StaleAuthority));
    }

    #[test]
    fn full_plan_digest_is_identical_at_one_four_and_sixteen_threads() {
        const MARKER: &str = "PLINTH_JOINT_THREAD_CHILD";
        if std::env::var_os(MARKER).is_some() {
            let expected_threads = std::env::var("PLINTH_THREADS")
                .unwrap()
                .parse::<usize>()
                .unwrap();
            assert_eq!(geom_kernel::current_threads(), expected_threads);
            let mut forest = Forest::new();
            for ordinal in 0..96_u32 {
                let x = f64::from(ordinal) * 25.0;
                add_member(
                    &mut forest,
                    [x, 0.0, 0.0],
                    [x, 0.0, 10.0],
                    0.6,
                    0.6,
                    MemberKind::Trunk,
                    Owner::Generated {
                        family: "joint-thread-fixture".into(),
                        ordinal: ordinal * 2,
                    },
                    None,
                );
                add_member(
                    &mut forest,
                    [x, 0.0, 10.0],
                    [x + 10.0, 0.0, 10.0],
                    0.6,
                    0.6,
                    MemberKind::Shoot,
                    Owner::Generated {
                        family: "joint-thread-fixture".into(),
                        ordinal: ordinal * 2 + 1,
                    },
                    None,
                );
            }
            let cfg = config(-100.0);
            let plan = plan_joint_balls(&forest, cfg).unwrap();
            assert!(plan.validate(&forest, cfg).unwrap().len() >= 96);
            println!("JOINT_PLAN={:016x}", plan.digest());
            return;
        }
        let test_name =
            "joints::tests::full_plan_digest_is_identical_at_one_four_and_sixteen_threads";
        let outputs = ["1", "4", "16"].map(|threads| {
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", test_name, "--nocapture"])
                .env(MARKER, "1")
                .env("PLINTH_THREADS", threads)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .find(|line| line.starts_with("JOINT_PLAN="))
                .unwrap()
                .to_owned()
        });
        assert!(outputs.windows(2).all(|pair| pair[0] == pair[1]));
    }
}
