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

/// One frame under assembly.
#[derive(Clone, Debug, Default)]
pub(crate) struct Frame {
    points: Vec<[f64; 3]>,
    fixed: Vec<bool>,
    /// `(node a, node b, radius)`.
    members: Vec<(usize, usize, f64)>,
    /// Merge buckets, `NODE_MERGE_MM` on a side.
    buckets: HashMap<[i64; 3], Vec<usize>>,
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
        for dx in -1..=1_i64 {
            for dy in -1..=1_i64 {
                for dz in -1..=1_i64 {
                    let key = [home[0] + dx, home[1] + dy, home[2] + dz];
                    let Some(list) = self.buckets.get(&key) else {
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
        if let Some(index) = found {
            self.fixed[index] |= fixed;
            return index;
        }
        let index = self.points.len();
        self.points.push(p);
        self.fixed.push(fixed);
        self.buckets.entry(home).or_default().push(index);
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
            match self.solve_piece(order, &members_of, &force) {
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
    fn solve_piece(
        &self,
        order: &[usize],
        members_of: &[Vec<usize>],
        force: &[[f64; 3]],
    ) -> Option<Vec<(usize, [f64; 3])>> {
        // Free dofs, six per free node, numbered in the piece's order.
        let mut dof_of: HashMap<usize, usize> = HashMap::new();
        let mut free_nodes: Vec<usize> = Vec::new();
        for &node in order {
            if !self.fixed[node] {
                dof_of.insert(node, 6 * free_nodes.len());
                free_nodes.push(node);
            }
        }
        let size = 6 * free_nodes.len();
        if size == 0 {
            return Some(Vec::new());
        }
        // The skyline: for each column, the first row any member couples in.
        let mut first: Vec<usize> = (0..size).collect();
        let mut stiffness: Vec<([usize; 12], [[f64; 12]; 12])> = Vec::new();
        for &node in order {
            for &member in &members_of[node] {
                let (a, b, radius) = self.members[member];
                let k = global_stiffness(self.points[a], self.points[b], radius);
                let mut dofs = [usize::MAX; 12];
                for (end, at) in [a, b].into_iter().enumerate() {
                    if let Some(&base) = dof_of.get(&at) {
                        for q in 0..6 {
                            dofs[6 * end + q] = base + q;
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
            for q in 0..3 {
                rhs[6 * index + q] = force[node][q];
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
                    (
                        node,
                        [rhs[6 * index], rhs[6 * index + 1], rhs[6 * index + 2]],
                    )
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
    let rotation = [ex, ey, ez];
    let area = core::f64::consts::PI * radius_mm * radius_mm;
    let second = core::f64::consts::PI * radius_mm.powi(4) / 4.0;
    let polar = 2.0 * second;
    let mut k = [[0.0_f64; 12]; 12];
    let mut set = |i: usize, j: usize, v: f64| {
        k[i][j] += v;
        if i != j {
            k[j][i] += v;
        }
    };
    let axial = area / length;
    set(0, 0, axial);
    set(0, 6, -axial);
    set(6, 6, axial);
    let torsion = SHEAR_OVER_YOUNG * polar / length;
    set(3, 3, torsion);
    set(3, 9, -torsion);
    set(9, 9, torsion);
    let c1 = 12.0 * second / length.powi(3);
    let c2 = 6.0 * second / (length * length);
    let c3 = 4.0 * second / length;
    let c4 = 2.0 * second / length;
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
    // K = Tᵀ k T with T = diag(R, R, R, R), block by block.
    let mut global = [[0.0_f64; 12]; 12];
    for bi in 0..4 {
        for bj in 0..4 {
            for i in 0..3 {
                for j in 0..3 {
                    let mut sum = 0.0;
                    for p in 0..3 {
                        for q in 0..3 {
                            sum += rotation[p][i] * k[3 * bi + p][3 * bj + q] * rotation[q][j];
                        }
                    }
                    global[3 * bi + i][3 * bj + j] = sum;
                }
            }
        }
    }
    global
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
