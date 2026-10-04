//! A small three-dimensional beam frame: straight Euler-Bernoulli members of
//! circular section, rigidly joined at nodes, some nodes fixed, point loads at
//! nodes. What the brace rules of 2026-10-03 need to answer "how far does this
//! printing top move" without treating any band as a clamp
//! (`field-2026-09-28/bracing-mechanics.md`, revision 2, rules R6 and R9).
//!
//! It is a port of the study's own solver (`bracing-mechanics/tools/frame.ts`):
//! the same element stiffness, the same local axes, the same node merge, so
//! the band counts it gives can be held to the study's numbers on the study's
//! own fixtures (`frame_bands::tests`). Two things differ, and neither
//! changes an answer:
//!
//! * **The solve is sparse.** The study eliminated a dense matrix. Here every
//!   connected piece of the frame is ordered by reverse Cuthill-McKee and
//!   factored in skyline (profile) storage by Cholesky; a piece that carries
//!   no load is not solved at all (it does not move). Batch-first: one
//!   assembly and one factorisation per piece per load set.
//! * **A piece that cannot carry its load** (no fixed node, so the stiffness
//!   is singular) is reported as unsolved, never guessed.
//!
//! Units are the caller's (mm), with `E = 1`: every displacement is in units
//! of `load / E`. The callers turn that into pixels through the slenderness
//! law `S`, which is where the modulus, the drag and the pixel already cancel
//! (`slenderness.rs`, "The constraint").

use std::collections::{HashMap, VecDeque};
use std::hash::{BuildHasherDefault, Hasher};

use geom_kernel::vec3;

/// Two nodes closer than this are one node (the study's `frame.ts`
/// `node()`, 2e-3 mm). A MESHING tolerance, not a physical one: it is a
/// tenth of a pixel, and it is what lets a band end meet the post level it
/// was rounded onto.
pub(crate) const NODE_MERGE_MM: f64 = 2.0e-3;

/// `G / E` for the members' torsion: `1 / (2 (1 + ν))` with `ν = 0.3`, the
/// study's `frame.ts` (`G = E / 2.6`). Unmeasured for this resin; it moves
/// only twisting, which neither the two-post panel (planar) nor a printing
/// top's sideways move depends on to first order.
const SHEAR_OVER_YOUNG: f64 = 1.0 / 2.6;

/// Relative size below which a Cholesky pivot is read as zero: the piece is a
/// mechanism (it has no fixed node, or a free member hangs on nothing).
/// Numerical, not physical.
const PIVOT_NOISE: f64 = 1.0e-12;

/// A cheap multiplicative hasher for the merge buckets (three small integers
/// per key; the standard library's SipHash was a tenth of a panel solve). Only
/// the lookup speed depends on it: the buckets are never iterated.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BucketHasher(u64);

impl BucketHasher {
    fn mix(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl Hasher for BucketHasher {
    fn finish(&self) -> u64 {
        // The multiply leaves the low bits of the key in the low bits of the
        // hash; fold the high ones down so a table's bucket index and a
        // shard choice both see all of them.
        let folded = self.0 ^ (self.0 >> 32);
        folded.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ (self.0 >> 40)
    }

    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0_u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.mix(u64::from_le_bytes(word));
        }
    }

    fn write_usize(&mut self, value: usize) {
        self.mix(value as u64);
    }
}

type Buckets = HashMap<[i64; 3], Vec<usize>, BuildHasherDefault<BucketHasher>>;

/// A member's stiffness from its two end points and its radius, over `N`
/// freedoms (12 in space, 6 in the plane).
type MemberStiffness<const N: usize> = fn([f64; 3], [f64; 3], f64) -> [[f64; N]; N];

/// One frame under assembly.
#[derive(Clone, Debug, Default)]
pub(crate) struct Frame {
    points: Vec<[f64; 3]>,
    fixed: Vec<bool>,
    /// `(node a, node b, radius)`.
    members: Vec<(usize, usize, f64)>,
    /// Merge buckets, `NODE_MERGE_MM` on a side.
    buckets: Buckets,
    /// The lowest and highest bucket coordinate in use on each axis, so a
    /// merge lookup never asks for a bucket outside them (it would be empty:
    /// a planar frame looks up 9 buckets, not 27).
    bucket_range: Option<([i64; 3], [i64; 3])>,
}

/// Displacements of every node, and which nodes belong to a piece that could
/// not be solved (no fixed node under a load).
#[derive(Clone, Debug)]
pub(crate) struct FrameSolution {
    pub moved: Vec<[f64; 3]>,
    pub solved: Vec<bool>,
}

fn bucket_of(p: [f64; 3]) -> [i64; 3] {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "model coordinates in mm over a 2e-3 mm bucket are far inside i64"
    )]
    p.map(|c| (c / NODE_MERGE_MM).floor() as i64)
}

impl Frame {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The node at `p`: an existing node within [`NODE_MERGE_MM`] (the
    /// EARLIEST such, as the study's linear scan finds it), else a new one.
    /// A node asked for as fixed stays fixed.
    pub(crate) fn node(&mut self, p: [f64; 3], fixed: bool) -> usize {
        let home = bucket_of(p);
        let mut found: Option<usize> = None;
        if let Some((low, high)) = self.bucket_range {
            let span = |axis: usize| (home[axis] - 1).max(low[axis])..=(home[axis] + 1).min(high[axis]);
            for x in span(0) {
                for y in span(1) {
                    for z in span(2) {
                        let Some(list) = self.buckets.get(&[x, y, z]) else {
                            continue;
                        };
                        for &index in list {
                            if vec3::dist(self.points[index], p) < NODE_MERGE_MM
                                && found.is_none_or(|known| index < known)
                            {
                                found = Some(index);
                            }
                        }
                    }
                }
            }
        }
        if let Some(index) = found {
            self.fixed[index] |= fixed;
            return index;
        }
        let index = self.points.len();
        self.points.push(p);
        self.fixed.push(fixed);
        self.buckets.entry(home).or_default().push(index);
        self.bucket_range = Some(match self.bucket_range {
            None => (home, home),
            Some((low, high)) => (
                [0, 1, 2].map(|axis| low[axis].min(home[axis])),
                [0, 1, 2].map(|axis| high[axis].max(home[axis])),
            ),
        });
        index
    }

    /// A member between two DISTINCT nodes (a member of zero length is not a
    /// member, and is not added).
    pub(crate) fn member(&mut self, a: usize, b: usize, radius_mm: f64) {
        if a != b && radius_mm.is_finite() && radius_mm > 0.0 {
            self.members.push((a, b, radius_mm));
        }
    }

    pub(crate) fn point(&self, node: usize) -> [f64; 3] {
        self.points[node]
    }

    #[cfg(test)]
    pub(crate) fn nodes(&self) -> usize {
        self.points.len()
    }

    /// Solve `K u = f` for the loads given (node, force), `E = 1`.
    ///
    /// The frame is split into its connected pieces; a piece carrying no load
    /// does not move and is not factored. Each loaded piece is ordered by
    /// reverse Cuthill-McKee over its nodes and factored in skyline storage.
    pub(crate) fn solve(&self, loads: &[(usize, [f64; 3])]) -> FrameSolution {
        self.solve_with::<12>(loads, global_stiffness)
    }

    /// [`Frame::solve`] for a frame that lies in one plane (every node at
    /// `y = 0`) under loads that act in it (none along `y`): the same answer
    /// to the last bit, from the three in-plane freedoms of each node (`x`,
    /// `z` and the turn about `y`) instead of all six.
    ///
    /// The out-of-plane freedoms of such a frame do not couple to the
    /// in-plane ones: every stiffness entry between them is an exact zero (the
    /// member axes are `x`, `y` and `z` combinations with exact zeros in the
    /// `y` column), and under in-plane loads they carry no load. The
    /// factorisation then does, on the in-plane freedoms in the same order,
    /// exactly the arithmetic the full solve does on them, minus the products
    /// with those zeros. What it does NOT do is notice an out-of-plane
    /// mechanism (a pivot there that [`PIVOT_NOISE`] would have refused); a
    /// frame the full solve calls singular out of plane and sound in plane
    /// is solved here. A frame that is not planar is solved in full.
    pub(crate) fn solve_in_plane(&self, loads: &[(usize, [f64; 3])]) -> FrameSolution {
        let planar = self.points.iter().all(|p| p[1] == 0.0)
            && loads.iter().all(|(_, f)| f[1] == 0.0);
        if planar {
            self.solve_with::<6>(loads, planar_stiffness)
        } else {
            self.solve_with::<12>(loads, global_stiffness)
        }
    }

    /// The solve, for members of `N` freedoms (two nodes of `N / 2` each).
    fn solve_with<const N: usize>(
        &self,
        loads: &[(usize, [f64; 3])],
        stiffness: MemberStiffness<N>,
    ) -> FrameSolution {
        let n = self.points.len();
        let mut moved = vec![[0.0; 3]; n];
        let mut solved = vec![true; n];
        if n == 0 {
            return FrameSolution { moved, solved };
        }
        let mut adjacent: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut members_of: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (index, &(a, b, _)) in self.members.iter().enumerate() {
            adjacent[a].push(b);
            adjacent[b].push(a);
            members_of[a].push(index);
        }
        let mut force = vec![[0.0; 3]; n];
        for &(node, f) in loads {
            if let Some(slot) = force.get_mut(node) {
                *slot = vec3::add(*slot, f);
            }
        }
        // Connected pieces, each listed in reverse Cuthill-McKee order.
        let mut piece_of = vec![usize::MAX; n];
        let mut pieces: Vec<Vec<usize>> = Vec::new();
        let degree = |node: usize| adjacent[node].len();
        let mut starts: Vec<usize> = (0..n).collect();
        starts.sort_by_key(|&node| (degree(node), node));
        for start in starts {
            if piece_of[start] != usize::MAX {
                continue;
            }
            let id = pieces.len();
            let mut order = Vec::new();
            let mut queue = VecDeque::from([start]);
            piece_of[start] = id;
            while let Some(node) = queue.pop_front() {
                order.push(node);
                let mut next: Vec<usize> = adjacent[node]
                    .iter()
                    .copied()
                    .filter(|&other| piece_of[other] == usize::MAX)
                    .collect();
                next.sort_by_key(|&other| (degree(other), other));
                next.dedup();
                for other in next {
                    if piece_of[other] == usize::MAX {
                        piece_of[other] = id;
                        queue.push_back(other);
                    }
                }
            }
            order.reverse();
            pieces.push(order);
        }
        for order in &pieces {
            let loaded = order
                .iter()
                .any(|&node| force[node].iter().any(|&c| c != 0.0) && !self.fixed[node]);
            if !loaded {
                continue;
            }
            match self.solve_piece::<N>(order, &members_of, &force, stiffness) {
                Some(piece) => {
                    for (node, u) in piece {
                        moved[node] = u;
                    }
                }
                None => {
                    for &node in order {
                        solved[node] = false;
                    }
                }
            }
        }
        FrameSolution { moved, solved }
    }

    /// One connected piece, nodes in solve order. `None` when it is singular.
    fn solve_piece<const N: usize>(
        &self,
        order: &[usize],
        members_of: &[Vec<usize>],
        force: &[[f64; 3]],
        member_stiffness: MemberStiffness<N>,
    ) -> Option<Vec<(usize, [f64; 3])>> {
        // Freedoms per node, and the force axes the translations carry: all
        // three in a full solve, `x` and `z` in a planar one.
        let per_node = N / 2;
        let axes: &[usize] = if per_node == 6 { &[0, 1, 2] } else { &[0, 2] };
        // Free dofs, `per_node` per free node, numbered in the piece's order.
        let mut dof_of: Vec<Option<usize>> = vec![None; self.points.len()];
        let mut free_nodes: Vec<usize> = Vec::new();
        for &node in order {
            if !self.fixed[node] {
                dof_of[node] = Some(per_node * free_nodes.len());
                free_nodes.push(node);
            }
        }
        let size = per_node * free_nodes.len();
        if size == 0 {
            return Some(Vec::new());
        }
        // The skyline: for each column, the first row any member couples in.
        let mut first: Vec<usize> = (0..size).collect();
        let mut stiffness: Vec<([usize; N], [[f64; N]; N])> = Vec::new();
        for &node in order {
            for &member in &members_of[node] {
                let (a, b, radius) = self.members[member];
                let k = member_stiffness(self.points[a], self.points[b], radius);
                let mut dofs = [usize::MAX; N];
                for (end, at) in [a, b].into_iter().enumerate() {
                    if let Some(base) = dof_of[at] {
                        for q in 0..per_node {
                            dofs[per_node * end + q] = base + q;
                        }
                    }
                }
                let lowest = dofs.iter().copied().filter(|&d| d != usize::MAX).min();
                if let Some(lowest) = lowest {
                    for &d in dofs.iter().filter(|&&d| d != usize::MAX) {
                        first[d] = first[d].min(lowest);
                    }
                }
                stiffness.push((dofs, k));
            }
        }
        // Column `j` holds rows `first[j]..=j`, starting at `start[j]`.
        let mut start = vec![0_usize; size + 1];
        for j in 0..size {
            start[j + 1] = start[j] + (j - first[j] + 1);
        }
        let mut sky = vec![0.0_f64; start[size]];
        let at = |row: usize, col: usize| start[col] + (row - first[col]);
        for (dofs, k) in &stiffness {
            for (p, &row) in dofs.iter().enumerate() {
                if row == usize::MAX {
                    continue;
                }
                for (q, &col) in dofs.iter().enumerate() {
                    if col == usize::MAX || row > col {
                        continue;
                    }
                    sky[at(row, col)] += k[p][q];
                }
            }
        }
        let diagonal: Vec<f64> = (0..size).map(|j| sky[at(j, j)]).collect();
        // Cholesky K = Uᵀ U, column by column, in place.
        for j in 0..size {
            for i in first[j]..j {
                let from = first[i].max(first[j]);
                let mut sum = sky[at(i, j)];
                for m in from..i {
                    sum -= sky[at(m, i)] * sky[at(m, j)];
                }
                sky[at(i, j)] = sum / sky[at(i, i)];
            }
            let mut pivot = sky[at(j, j)];
            for m in first[j]..j {
                pivot -= sky[at(m, j)] * sky[at(m, j)];
            }
            if !(pivot.is_finite() && pivot > PIVOT_NOISE * diagonal[j].abs().max(f64::MIN_POSITIVE))
            {
                return None;
            }
            sky[at(j, j)] = pivot.sqrt();
        }
        let mut rhs = vec![0.0_f64; size];
        for (index, &node) in free_nodes.iter().enumerate() {
            for (slot, &axis) in axes.iter().enumerate() {
                rhs[per_node * index + slot] = force[node][axis];
            }
        }
        // Uᵀ y = f.
        for j in 0..size {
            let mut sum = rhs[j];
            for m in first[j]..j {
                sum -= sky[at(m, j)] * rhs[m];
            }
            rhs[j] = sum / sky[at(j, j)];
        }
        // U x = y.
        for j in (0..size).rev() {
            rhs[j] /= sky[at(j, j)];
            let value = rhs[j];
            for m in first[j]..j {
                rhs[m] -= sky[at(m, j)] * value;
            }
        }
        if rhs.iter().any(|value| !value.is_finite()) {
            return None;
        }
        Some(
            free_nodes
                .iter()
                .enumerate()
                .map(|(index, &node)| {
                    let mut moved = [0.0; 3];
                    for (slot, &axis) in axes.iter().enumerate() {
                        moved[axis] = rhs[per_node * index + slot];
                    }
                    (node, moved)
                })
                .collect(),
        )
    }
}

/// The 12 x 12 stiffness of one member in global axes, `E = 1`: the study's
/// `frame.ts` element (axial, torsion, bending in both local planes), rotated
/// by its local axes (`x` along the member, `y` from `ẑ × x`, or `x̂ × x` for a
/// member within 25.8° of vertical).
fn global_stiffness(a: [f64; 3], b: [f64; 3], radius_mm: f64) -> [[f64; 12]; 12] {
    let (rotation, k) = local_stiffness(a, b, radius_mm);
    // K = Tᵀ k T with T = diag(R, R, R, R), block by block.
    let mut global = [[0.0_f64; 12]; 12];
    for bi in 0..4 {
        for bj in 0..4 {
            for i in 0..3 {
                for j in 0..3 {
                    global[3 * bi + i][3 * bj + j] = rotated_entry(&rotation, &k, bi, i, bj, j);
                }
            }
        }
    }
    global
}

/// The global freedoms of a member in the plane `y = 0` that the in-plane
/// load reaches: per end, the translations along `x` and `z` and the turn
/// about `y`, as indices into [`global_stiffness`]'s 12.
#[cfg(test)]
const IN_PLANE: [usize; 6] = [0, 2, 4, 6, 8, 10];

/// The 6 x 6 in-plane block of [`global_stiffness`] for a member in the plane
/// `y = 0`, entry for entry the same numbers (`tests::the_planar_stiffness_is_
/// the_in_plane_block_of_the_full_one_to_the_last_bit`): the freedoms are
/// [`IN_PLANE`], in the order the full matrix lists them. The out-of-plane
/// entries it leaves out are not needed by an in-plane load, and the ones
/// that couple to these are exact zeros (see [`Frame::solve_in_plane`]).
///
/// Formed from the member's six in-plane LOCAL freedoms only (axial and
/// bending in the member's `x`-`z` plane). The full sum of an entry runs over
/// every local freedom, but the member's `y` axis is `±ŷ`, so a global `x`,
/// `z` or turn-about-`y` freedom has a nonzero rotation factor on the local
/// in-plane freedoms alone and the terms it leaves out have an exact zero
/// factor; the terms kept are formed in the full sum's order.
fn planar_stiffness(a: [f64; 3], b: [f64; 3], radius_mm: f64) -> [[f64; 6]; 6] {
    let (rotation, length) = member_axes(a, b);
    let beam = BeamTerms::new(length, radius_mm);
    // Local in-plane stiffness over [x, z, turn about y] at each end, from
    // the same values `local_stiffness` sets for its freedoms 0, 2, 4, 6, 8
    // and 10.
    let sign = -1.0_f64;
    let mut local = [[0.0_f64; 6]; 6];
    let mut set = |i: usize, j: usize, v: f64| {
        local[i][j] = v;
        local[j][i] = v;
    };
    set(0, 0, beam.axial);
    set(0, 3, -beam.axial);
    set(3, 3, beam.axial);
    set(1, 1, beam.c1);
    set(1, 4, -beam.c1);
    set(4, 4, beam.c1);
    set(1, 2, sign * beam.c2);
    set(1, 5, sign * beam.c2);
    set(4, 2, -sign * beam.c2);
    set(4, 5, -sign * beam.c2);
    set(2, 2, beam.c3);
    set(5, 5, beam.c3);
    set(2, 5, beam.c4);
    // Per global in-plane freedom: the axis component it reads, and the
    // (rotation row, local freedom) pairs that can be nonzero.
    const COMPONENT: [usize; 6] = [0, 2, 1, 0, 2, 1];
    const REACH: [&[(usize, usize)]; 6] = [
        &[(0, 0), (2, 1)],
        &[(0, 0), (2, 1)],
        &[(1, 2)],
        &[(0, 3), (2, 4)],
        &[(0, 3), (2, 4)],
        &[(1, 5)],
    ];
    let mut block = [[0.0_f64; 6]; 6];
    for row in 0..6 {
        for col in 0..6 {
            let mut sum = 0.0;
            for &(p, local_row) in REACH[row] {
                let from = rotation[p][COMPONENT[row]];
                if from == 0.0 {
                    continue;
                }
                for &(q, local_col) in REACH[col] {
                    let to = rotation[q][COMPONENT[col]];
                    if to == 0.0 {
                        continue;
                    }
                    sum += from * local[local_row][local_col] * to;
                }
            }
            block[row][col] = sum;
        }
    }
    block
}

/// The in-plane block of [`global_stiffness`] picked out of the full matrix:
/// what [`planar_stiffness`] must equal, bit for bit.
#[cfg(test)]
fn planar_stiffness_by_extraction(a: [f64; 3], b: [f64; 3], radius_mm: f64) -> [[f64; 6]; 6] {
    let full = global_stiffness(a, b, radius_mm);
    let mut block = [[0.0_f64; 6]; 6];
    for (row, &i) in IN_PLANE.iter().enumerate() {
        for (col, &j) in IN_PLANE.iter().enumerate() {
            block[row][col] = full[i][j];
        }
    }
    block
}

/// One entry of `Tᵀ k T`: block `(bi, bj)`, component `(i, j)`; the full
/// sum in the full matrix's order. A term whose rotation factor is an exact
/// zero adds an exact zero to the sum and is not formed.
fn rotated_entry(
    rotation: &[[f64; 3]; 3],
    k: &[[f64; 12]; 12],
    bi: usize,
    i: usize,
    bj: usize,
    j: usize,
) -> f64 {
    let mut sum = 0.0;
    for p in 0..3 {
        if rotation[p][i] == 0.0 {
            continue;
        }
        for q in 0..3 {
            if rotation[q][j] == 0.0 {
                continue;
            }
            sum += rotation[p][i] * k[3 * bi + p][3 * bj + q] * rotation[q][j];
        }
    }
    sum
}

/// A member's local axes (rows `x`, `y`, `z`) and its 12 x 12 stiffness in
/// them.
fn local_stiffness(a: [f64; 3], b: [f64; 3], radius_mm: f64) -> ([[f64; 3]; 3], [[f64; 12]; 12]) {
    let (rotation, length) = member_axes(a, b);
    let beam = BeamTerms::new(length, radius_mm);
    let mut k = [[0.0_f64; 12]; 12];
    let mut set = |i: usize, j: usize, v: f64| {
        k[i][j] += v;
        if i != j {
            k[j][i] += v;
        }
    };
    set(0, 0, beam.axial);
    set(0, 6, -beam.axial);
    set(6, 6, beam.axial);
    set(3, 3, beam.torsion);
    set(3, 9, -beam.torsion);
    set(9, 9, beam.torsion);
    let BeamTerms { c1, c2, c3, c4, .. } = beam;
    for (v, th, sign) in [(1_usize, 5_usize, 1.0_f64), (2, 4, -1.0)] {
        set(v, v, c1);
        set(v, v + 6, -c1);
        set(v + 6, v + 6, c1);
        set(v, th, sign * c2);
        set(v, th + 6, sign * c2);
        set(v + 6, th, -sign * c2);
        set(v + 6, th + 6, -sign * c2);
        set(th, th, c3);
        set(th + 6, th + 6, c3);
        set(th, th + 6, c4);
    }
    (rotation, k)
}

/// A member's local axes as rows (`x` along it, `y`, `z`) and its length.
fn member_axes(a: [f64; 3], b: [f64; 3]) -> ([[f64; 3]; 3], f64) {
    let d = vec3::sub(b, a);
    let length = vec3::norm(d);
    let ex = vec3::scale(d, 1.0 / length);
    let reference = if ex[2].abs() < 0.9 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let ey = vec3::normalize(vec3::cross(reference, ex)).unwrap_or([0.0, 1.0, 0.0]);
    let ez = vec3::cross(ex, ey);
    ([ex, ey, ez], length)
}

/// The stiffness numbers of one straight circular member, `E = 1`.
struct BeamTerms {
    axial: f64,
    torsion: f64,
    c1: f64,
    c2: f64,
    c3: f64,
    c4: f64,
}

impl BeamTerms {
    fn new(length: f64, radius_mm: f64) -> Self {
        let area = core::f64::consts::PI * radius_mm * radius_mm;
        let second = core::f64::consts::PI * radius_mm.powi(4) / 4.0;
        let polar = 2.0 * second;
        Self {
            axial: area / length,
            torsion: SHEAR_OVER_YOUNG * polar / length,
            c1: 12.0 * second / length.powi(3),
            c2: 6.0 * second / (length * length),
            c3: 4.0 * second / length,
            c4: 2.0 * second / length,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Frame;

    /// A straight cantilever, fixed at its foot, tip force sideways: the
    /// closed form `F L³ / (3 E I)` (the study's own synthetic check), and a
    /// tip moment: `M L² / (2 E I)`. Split into several members, which a beam
    /// element represents exactly under nodal loads.
    #[test]
    fn a_cantilever_matches_its_closed_form() {
        let (radius, length, force) = (0.3_f64, 12.0_f64, 2.5e-3_f64);
        let mut frame = Frame::new();
        let mut previous = frame.node([0.0, 0.0, 0.0], true);
        for step in 1..=7 {
            let z = length * f64::from(step) / 7.0;
            let node = frame.node([0.0, 0.0, z], false);
            frame.member(previous, node, radius);
            previous = node;
        }
        let solution = frame.solve(&[(previous, [force, 0.0, 0.0])]);
        let second = core::f64::consts::PI * radius.powi(4) / 4.0;
        let expected = force * length.powi(3) / (3.0 * second);
        let got = solution.moved[previous][0];
        assert!(
            (got - expected).abs() <= 1.0e-9 * expected,
            "tip move {got} against the closed form {expected}"
        );
        // A tilted copy moves the same distance along the same force.
        let mut tilted = Frame::new();
        let foot = tilted.node([1.0, 2.0, 0.0], true);
        let top = tilted.node([1.0, 2.0 + length * 0.6, length * 0.8], false);
        tilted.member(foot, top, radius);
        let direction = [0.0, 0.8, -0.6];
        let push = direction.map(|c| c * force);
        let moved = tilted.solve(&[(top, push)]).moved[top];
        let along = geom_kernel::vec3::dot(moved, direction);
        assert!(
            (along - expected).abs() <= 1.0e-9 * expected,
            "tilted tip move {along} against {expected}"
        );
    }

    /// A piece with nothing fixed cannot carry a load: it is reported
    /// unsolved, and a piece with no load does not move.
    #[test]
    fn a_floating_piece_is_unsolved_and_an_unloaded_one_stays_put() {
        let mut frame = Frame::new();
        let a = frame.node([0.0, 0.0, 0.0], false);
        let b = frame.node([0.0, 0.0, 5.0], false);
        frame.member(a, b, 0.2);
        let c = frame.node([3.0, 0.0, 0.0], true);
        let d = frame.node([3.0, 0.0, 5.0], false);
        frame.member(c, d, 0.2);
        let solution = frame.solve(&[(b, [1.0, 0.0, 0.0])]);
        assert!(!solution.solved[b]);
        assert!(solution.solved[d]);
        assert!(solution.moved[d].iter().all(|c| c.abs() < f64::MIN_POSITIVE));
    }

    /// **The planar stiffness is the full one's in-plane block, to the last
    /// bit**: random members in the plane `y = 0` (vertical, near vertical,
    /// leaning, flat, in both directions along `x`).
    #[test]
    fn the_planar_stiffness_is_the_in_plane_block_of_the_full_one_to_the_last_bit() {
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut unit = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            f64::from(u32::try_from(state >> 33).expect("31 bits")) / f64::from(1_u32 << 31)
        };
        for case in 0..2_000 {
            let a = [10.0 * unit() - 5.0, 0.0, 20.0 * unit()];
            let b = match case % 4 {
                0 => [a[0], 0.0, a[2] + 0.1 + 5.0 * unit()],
                1 => [a[0] + 0.3 * (unit() - 0.5), 0.0, a[2] + 0.1 + 5.0 * unit()],
                2 => [a[0] + 6.0 * (unit() - 0.5), 0.0, a[2] + 3.0 * (unit() - 0.5)],
                _ => [a[0] - 4.0 * unit() - 0.01, 0.0, a[2] - 4.0 * unit() - 0.01],
            };
            let radius = 0.05 + unit();
            let fast = super::planar_stiffness(a, b, radius).map(|row| row.map(f64::to_bits));
            let full =
                super::planar_stiffness_by_extraction(a, b, radius).map(|row| row.map(f64::to_bits));
            assert_eq!(fast, full, "member {a:?} to {b:?} at radius {radius}");
        }
    }

    /// **The in-plane solve is the full solve, to the last bit**, on a planar
    /// frame (a two-post portal with a cross brace, a lumped drag), and a
    /// frame that is not planar is solved in full.
    #[test]
    fn the_in_plane_solve_is_the_full_solve_to_the_last_bit() {
        let mut frame = Frame::new();
        let foot_a = frame.node([0.0, 0.0, 0.0], true);
        let foot_b = frame.node([2.2, 0.0, 0.0], true);
        let (mut below_a, mut below_b) = (foot_a, foot_b);
        let mut loads = Vec::new();
        for step in 1..=6 {
            let z = 1.7 * f64::from(step);
            let a = frame.node([0.0, 0.0, z], false);
            let b = frame.node([2.2, 0.0, z], false);
            frame.member(below_a, a, 0.15);
            frame.member(below_b, b, 0.21);
            loads.push((a, [0.4, 0.0, -0.1]));
            loads.push((b, [0.7, 0.0, 0.0]));
            below_a = a;
            below_b = b;
        }
        let middle = frame.node([1.1, 0.0, 4.0], false);
        let (a0, a1) = (frame.node([0.0, 0.0, 3.4], false), frame.node([0.0, 0.0, 5.1], false));
        let (b0, b1) = (frame.node([2.2, 0.0, 3.4], false), frame.node([2.2, 0.0, 5.1], false));
        for (from, to) in [(a0, middle), (middle, b1), (b0, middle), (middle, a1)] {
            frame.member(from, to, 0.07);
        }
        let full = frame.solve(&loads);
        let planar = frame.solve_in_plane(&loads);
        assert_eq!(full.solved, planar.solved);
        for (node, (f, p)) in full.moved.iter().zip(&planar.moved).enumerate() {
            assert_eq!(
                [f[0].to_bits(), f[2].to_bits()],
                [p[0].to_bits(), p[2].to_bits()],
                "node {node}"
            );
        }
        assert!(full.moved[below_a][0].abs() > 0.0, "the frame moved");
        // Out of plane: the same call falls back to the full solve.
        let mut bent = frame.clone();
        let out = bent.node([0.0, 0.5, 3.0], false);
        bent.member(below_a, out, 0.1);
        let sideways = [(out, [0.0, 0.3, 0.0])];
        let (one, two) = (bent.solve(&sideways), bent.solve_in_plane(&sideways));
        assert_eq!(one.moved, two.moved);
    }

    /// Nodes within the merge tolerance are one node.
    #[test]
    fn close_nodes_merge_to_the_earliest() {
        let mut frame = Frame::new();
        let a = frame.node([0.0, 0.0, 1.0], false);
        let b = frame.node([0.0, 0.0, 1.0015], true);
        let c = frame.node([0.0, 0.0, 1.003], false);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(frame.nodes(), 2);
    }
}
