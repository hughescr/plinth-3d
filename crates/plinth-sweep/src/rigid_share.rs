//! **The rigid-body reading of the load share: a WARNING, never sizing**
//! (owner ruling 2026-09-29).
//!
//! The engine sizes by column routing per event ([`crate::per_event`]): a
//! flexible-body reading, which matches a thin body that has just cured. A body
//! that has cured stiff instead shares its peel among every support tree
//! attached to it, in proportion to each tree's axial stiffness `k = A/L`.
//! Neither reading is the truth; a real body lies between them. Measured on the
//! production fields (`plinth-evidence/.../field-2026-09-28/
//! cylinder-load-sharing.md`, §3-4), the rigid reading is the better one on the
//! hollow Cylinder but overloads the laid tube's short, stiff lowest-end arms
//! (53.53 mm² against ℓ_max = 37.70 mm²) and adds 21 % volume to a flat disc,
//! and resizing to it does not converge. So it does not size anything. It is
//! published per arm, and an arm whose rigid-body share would need more than
//! `X_ALLOWED` at the largest legal radius — a share above `ℓ_max` — is named
//! in a WARNING, so the owner can look at that arm.
//!
//! # The model (the probe's rule (b), unchanged)
//!
//! * Every event's peel goes into the cured BODY it belongs to at that moment:
//!   bodies are the connected components of the cover, tracked event by event.
//! * The cells of an event whose FIRST hop (the engine's own routing) is an arm
//!   load that arm and go down its own chain instead: the local-lamina term.
//!   Without it the cavity arms carry nothing.
//! * The body load is shared by every support tree attached to that body: three
//!   unknowns per body (drop and two tilts), a free node at every arm-on-arm
//!   junction, axial stiffness `A/L` integrated per layer along each member
//!   from the engine's own radii. The modulus cancels. Compression is allowed
//!   and reported.
//!
//! Limits, stated so the number is never over-read: axial, vertical, one
//! dimension; braces are not members; the body is rigid once attached; the
//! vat and film compliance are not modelled.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "cell, layer, node and member indices are bounded by the cell-layer budget and the \
          strut cap: they fit u32, and a grid row or column fits i64"
)]
#![expect(
    clippy::needless_range_loop,
    reason = "batch-first: these loops walk parallel per-cell and per-segment arrays at one index"
)]

use std::collections::HashMap;

use plinth_support::perception::SliceSolid;

use crate::grid::Grid;
use crate::per_event::{self, RouteLog, LOST, PLATE};
use crate::{Landing, StrutKind, SweepStrut};

const NONE: u32 = u32::MAX;

/// One arm whose rigid-body share passes `ℓ_max`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RigidShareWarning {
    pub strut_id: u32,
    /// The largest force the rigid reading puts in any segment of this arm, mm².
    pub share_mm2: f64,
    /// `share / (π r_max²)`: the `X` it would realize at the largest legal
    /// radius. Above `X_ALLOWED` by definition of the warning.
    pub realized_x: f64,
    /// What the engine sized this arm for (its root ledger), mm².
    pub sized_mm2: f64,
    /// The layer of the event that governs the share.
    pub event_layer: usize,
    /// Members that land on this arm (a merged trunk is stiff, and a stiff
    /// member attracts rigid share), and where the arm itself lands.
    pub members_landed_on_it: usize,
    pub landing: Landing,
    /// The arm's root radius as sized, mm.
    pub root_radius_mm: f64,
}

/// The diagnostic's own report.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RigidShareReport {
    /// Whether it ran at all (the path-hit witness: "no warning" must not be
    /// readable as "never ran").
    pub ran: bool,
    pub events_solved: usize,
    pub members: usize,
    pub trees: usize,
    /// The heaviest rigid-body share on any arm, and which arm.
    pub max_share_mm2: f64,
    pub max_share_id: u32,
    /// Arms whose share passes `ℓ_max`, heaviest first.
    pub over_limit: Vec<RigidShareWarning>,
    /// The largest compression any member takes under the rigid reading, mm².
    pub compressive_max_mm2: f64,
    /// The worst per-event conservation residual, relative.
    pub conservation_residual: f64,
}

// ------------------------------------------------------------------ network

#[derive(Clone, Copy, PartialEq)]
enum Bottom {
    Plate,
    Model,
    Parent { seg: usize },
}

struct Member {
    strut: usize,
    is_arm: bool,
    seg0: usize,
    nseg: usize,
    bottom: Bottom,
    bottom_node: usize,
}

struct Seg {
    member: usize,
    upper: usize,
    lower: usize,
    /// Inclusive layer bounds; `lay_lo > lay_hi` is an empty segment (a
    /// junction at the member's own top slot).
    lay_lo: usize,
    lay_hi: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum NodeKind {
    Tip(usize),
    Junction,
    Ground,
    ModelPoint(usize),
}

struct Node {
    kind: NodeKind,
    down: usize,
    ups: Vec<usize>,
    x: f64,
    y: f64,
}

struct Tree {
    root: usize,
    /// Nodes in pre-order from the root (parents before children).
    order: Vec<usize>,
    segs: Vec<usize>,
    tips: Vec<usize>,
}

struct Net {
    mem_of: Vec<usize>,
    members: Vec<Member>,
    segs: Vec<Seg>,
    nodes: Vec<Node>,
    /// Members, each before the member it hands load to.
    topo: Vec<usize>,
    trees: Vec<Tree>,
}

impl Net {
    fn seg_of(&self, member: usize, layer: usize) -> usize {
        let mem = &self.members[member];
        for s in mem.seg0..mem.seg0 + mem.nseg {
            let seg = &self.segs[s];
            if seg.lay_lo <= seg.lay_hi && layer >= seg.lay_lo && layer <= seg.lay_hi {
                return s;
            }
        }
        if layer >= self.segs[mem.seg0].lay_lo {
            for s in mem.seg0..mem.seg0 + mem.nseg {
                if self.segs[s].lay_lo <= self.segs[s].lay_hi {
                    return s;
                }
            }
            return mem.seg0;
        }
        mem.seg0 + mem.nseg - 1
    }
}

fn cell_xy(grid: &Grid, cell: usize) -> (f64, f64) {
    (
        grid.center_x_mm(grid.col_of(cell)),
        grid.center_y_mm(grid.row_of(cell)),
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "one network build: members, junction nodes, segments, bottoms, trees and the order, \
          each a short pass over the previous one's output"
)]
fn build_net(grid: &Grid, struts: &[SweepStrut]) -> Net {
    let mut mem_of = vec![usize::MAX; struts.len()];
    let mut order: Vec<usize> = Vec::new();
    for (i, s) in struts.iter().enumerate() {
        if s.kind == StrutKind::Brace || s.k_bottom == usize::MAX || s.k_top <= s.k_bottom {
            continue;
        }
        mem_of[i] = order.len();
        order.push(i);
    }
    let parent_of = |i: usize| -> Option<usize> {
        if let Landing::Strut { id, .. } = struts[i].landing {
            let p = id as usize;
            if p < struts.len() && mem_of[p] != usize::MAX && p != i {
                return Some(p);
            }
        }
        None
    };
    let junction = |i: usize, p: usize| -> usize {
        let ps = &struts[p];
        struts[i].k_bottom.clamp(ps.k_bottom, ps.k_top - 1)
    };
    let mut junctions: Vec<Vec<usize>> = vec![Vec::new(); order.len()];
    for &i in &order {
        if let Some(p) = parent_of(i) {
            junctions[mem_of[p]].push(junction(i, p));
        }
    }
    for j in &mut junctions {
        j.sort_unstable_by(|a, b| b.cmp(a));
        j.dedup();
    }
    let mut nodes: Vec<Node> = Vec::new();
    let mut segs: Vec<Seg> = Vec::new();
    let mut members: Vec<Member> = Vec::new();
    let mut junction_node: HashMap<(usize, usize), usize> = HashMap::new();
    for (m, &i) in order.iter().enumerate() {
        let s = &struts[i];
        let (tx, ty) = cell_xy(grid, s.cell);
        let tip = nodes.len();
        nodes.push(Node {
            kind: NodeKind::Tip(m),
            down: usize::MAX,
            ups: Vec::new(),
            x: tx,
            y: ty,
        });
        let mut chain = vec![tip];
        for &j in &junctions[m] {
            let (jx, jy) = cell_xy(grid, s.cell_at(j));
            let n = nodes.len();
            nodes.push(Node {
                kind: NodeKind::Junction,
                down: usize::MAX,
                ups: Vec::new(),
                x: jx,
                y: jy,
            });
            junction_node.insert((m, j), n);
            chain.push(n);
        }
        let seg0 = segs.len();
        let mut bounds: Vec<(usize, usize)> = Vec::new();
        let mut hi = s.k_top - 1;
        for &j in &junctions[m] {
            bounds.push((j + 1, hi));
            hi = j;
        }
        bounds.push((s.k_bottom, hi));
        for (bi, &(lo, hi)) in bounds.iter().enumerate() {
            let upper = chain[bi];
            let lower = chain.get(bi + 1).copied().unwrap_or(usize::MAX);
            let sid = segs.len();
            segs.push(Seg {
                member: m,
                upper,
                lower,
                lay_lo: lo,
                lay_hi: hi,
            });
            nodes[upper].down = sid;
        }
        members.push(Member {
            strut: i,
            is_arm: s.kind == StrutKind::Arm,
            seg0,
            nseg: segs.len() - seg0,
            bottom: Bottom::Plate,
            bottom_node: usize::MAX,
        });
    }
    for m in 0..members.len() {
        let i = members[m].strut;
        let s = &struts[i];
        let last = members[m].seg0 + members[m].nseg - 1;
        let (bx, by) = cell_xy(grid, s.base_cell());
        let (bottom, node) = if let Some(p) = parent_of(i) {
            let n = junction_node[&(mem_of[p], junction(i, p))];
            (Bottom::Parent { seg: nodes[n].down }, n)
        } else {
            let model = matches!(s.landing, Landing::Model { .. });
            let n = nodes.len();
            nodes.push(Node {
                kind: if model {
                    NodeKind::ModelPoint(m)
                } else {
                    NodeKind::Ground
                },
                down: usize::MAX,
                ups: Vec::new(),
                x: bx,
                y: by,
            });
            (if model { Bottom::Model } else { Bottom::Plate }, n)
        };
        members[m].bottom = bottom;
        members[m].bottom_node = node;
        segs[last].lower = node;
    }
    for (sid, seg) in segs.iter().enumerate() {
        if seg.lower != usize::MAX {
            nodes[seg.lower].ups.push(sid);
        }
    }
    let mut trees = Vec::new();
    for (n, node) in nodes.iter().enumerate() {
        if matches!(node.kind, NodeKind::Ground | NodeKind::ModelPoint(_)) {
            let mut order = Vec::new();
            let mut tsegs = Vec::new();
            let mut tips = Vec::new();
            let mut stack = vec![n];
            while let Some(v) = stack.pop() {
                order.push(v);
                if let NodeKind::Tip(_) = nodes[v].kind {
                    tips.push(v);
                }
                for &e in &nodes[v].ups {
                    tsegs.push(e);
                    stack.push(segs[e].upper);
                }
            }
            trees.push(Tree {
                root: n,
                order,
                segs: tsegs,
                tips,
            });
        }
    }
    // Children before parents (Kahn on the parent edges).
    let mut indeg = vec![0_usize; members.len()];
    let parent_member = |m: &Member| -> Option<usize> {
        match m.bottom {
            Bottom::Parent { seg } => Some(segs[seg].member),
            _ => None,
        }
    };
    for m in &members {
        if let Some(p) = parent_member(m) {
            indeg[p] += 1;
        }
    }
    let mut queue: Vec<usize> = (0..members.len()).filter(|&m| indeg[m] == 0).collect();
    let mut topo = Vec::with_capacity(members.len());
    let mut head = 0;
    while head < queue.len() {
        let m = queue[head];
        head += 1;
        topo.push(m);
        if let Some(p) = parent_member(&members[m]) {
            indeg[p] -= 1;
            if indeg[p] == 0 {
                queue.push(p);
            }
        }
    }
    let mut placed = vec![false; members.len()];
    for &m in &topo {
        placed[m] = true;
    }
    topo.extend((0..members.len()).filter(|&m| !placed[m]));
    Net {
        mem_of,
        members,
        segs,
        nodes,
        topo,
        trees,
    }
}

// --------------------------------------------------------------- the events

struct Uf {
    parent: Vec<u32>,
}

impl Uf {
    fn add(&mut self) -> u32 {
        let n = self.parent.len() as u32;
        self.parent.push(n);
        n
    }
    fn find(&mut self, mut x: u32) -> u32 {
        while self.parent[x as usize] != x {
            let grand = self.parent[self.parent[x as usize] as usize];
            self.parent[x as usize] = grand;
            x = grand;
        }
        x
    }
    fn union(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
            self.parent[hi as usize] = lo;
        }
    }
}

#[derive(Clone, Copy)]
struct BodyLoad {
    root: u32,
    force: f64,
    sx: f64,
    sy: f64,
}

#[derive(Default)]
struct Event {
    layer: usize,
    area: f64,
    lost: f64,
    plate_direct: f64,
    bodies: Vec<BodyLoad>,
    /// `(net member, entry layer, load)` for cells whose first hop is an arm.
    local: Vec<(usize, usize, f64)>,
}

/// The nearest cover cell to `cell` within `radius` cells, or the cell itself.
fn find_cover(grid: &Grid, cover: &[bool], cell: usize, radius: i64) -> Option<usize> {
    if cover[cell] {
        return Some(cell);
    }
    let (r0, c0) = (grid.row_of(cell) as i64, grid.col_of(cell) as i64);
    let mut best: Option<(i64, usize)> = None;
    for dr in -radius..=radius {
        for dc in -radius..=radius {
            let (r, c) = (r0 + dr, c0 + dc);
            if r < 0 || c < 0 || r >= grid.rows() as i64 || c >= grid.cols() as i64 {
                continue;
            }
            #[expect(
                clippy::cast_sign_loss,
                reason = "both checked non-negative just above"
            )]
            let idx = grid.index(r as usize, c as usize);
            if cover[idx] {
                let d = dr * dr + dc * dc;
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, idx));
                }
            }
        }
    }
    best.map(|(_, i)| i)
}

/// Where a seat or a model landing is looked for around its own cell: the
/// probe's six cells, which at the working pitch is a third of a millimetre.
const SNAP_SEARCH_CELLS: i64 = 6;

/// Everything the rigid solve needs from one upward walk.
struct Walked {
    events: Vec<Event>,
    /// Per event, every prescribed point's body as of that event.
    snaps: Vec<Vec<u32>>,
    tip_snap: Vec<u32>,
    land_snap: Vec<u32>,
}

#[expect(
    clippy::too_many_lines,
    reason = "one observer over the walk: body labels, prescribed points and the event's split \
          into local, body, plate and lost load, each a short pass over the same layer"
)]
fn walk_events(
    grid: &Grid,
    slices: &[Vec<SliceSolid>],
    struts: &[SweepStrut],
    log: &RouteLog,
    net: &Net,
) -> Walked {
    let cells = grid.cells();
    let area = grid.cell_area_mm2();
    let layers = slices.len();
    let mut seat_at: Vec<Vec<usize>> = vec![Vec::new(); layers + 1];
    let mut land_at: Vec<Vec<usize>> = vec![Vec::new(); layers + 1];
    for (m, mem) in net.members.iter().enumerate() {
        let s = &struts[mem.strut];
        if mem.is_arm && s.k_top <= layers {
            seat_at[s.k_top].push(m);
        }
        if mem.bottom == Bottom::Model {
            if let Landing::Model { layer } = s.landing {
                land_at[layer.min(layers)].push(m);
            }
        }
    }
    let mut uf = Uf { parent: Vec::new() };
    let mut lab_prev = vec![NONE; cells];
    let mut lab_cur = vec![NONE; cells];
    let mut cover_prev = vec![false; cells];
    let mut snap_nodes: Vec<u32> = Vec::new();
    let mut tip_snap = vec![NONE; net.members.len()];
    let mut land_snap = vec![NONE; net.members.len()];
    let mut events: Vec<Event> = Vec::new();
    let mut snaps: Vec<Vec<u32>> = Vec::new();
    let mut cont = vec![LOST; struts.len()];
    let mut body_index: HashMap<u32, usize> = HashMap::new();
    let mut local: HashMap<u64, f64> = HashMap::new();
    per_event::walk(grid, slices, struts, log, &mut cont, |view, _| {
        let k = view.layer;
        let cover = &view.masks.cover;
        let cols = grid.cols();
        lab_cur.fill(NONE);
        for cell in 0..cells {
            if !cover[cell] {
                continue;
            }
            let col = cell % cols;
            let left = if col > 0 && cover[cell - 1] {
                lab_cur[cell - 1]
            } else {
                NONE
            };
            let up = if cell >= cols && cover[cell - cols] {
                lab_cur[cell - cols]
            } else {
                NONE
            };
            lab_cur[cell] = match (left != NONE, up != NONE) {
                (false, false) => uf.add(),
                (true, false) => left,
                (false, true) => up,
                (true, true) => {
                    uf.union(left, up);
                    left
                }
            };
        }
        for cell in 0..cells {
            if cover[cell] && cover_prev[cell] {
                uf.union(lab_cur[cell], lab_prev[cell]);
            }
        }
        for &m in &seat_at[k] {
            let s = &struts[net.members[m].strut];
            if let Some(c) = find_cover(grid, cover, s.cell, SNAP_SEARCH_CELLS) {
                tip_snap[m] = snap_nodes.len() as u32;
                snap_nodes.push(lab_cur[c]);
            }
        }
        for &m in &land_at[k] {
            let s = &struts[net.members[m].strut];
            let base = s.base_cell();
            let here = find_cover(grid, cover, base, SNAP_SEARCH_CELLS).map(|c| lab_cur[c]);
            let found = here.or_else(|| {
                (k > 0)
                    .then(|| find_cover(grid, &cover_prev, base, SNAP_SEARCH_CELLS))
                    .flatten()
                    .map(|c| lab_prev[c])
            });
            if let Some(label) = found {
                land_snap[m] = snap_nodes.len() as u32;
                snap_nodes.push(label);
            }
        }
        // This layer's event, split by where each cell's load goes FIRST.
        let mut event = Event {
            layer: k,
            ..Event::default()
        };
        body_index.clear();
        local.clear();
        for cell in 0..cells {
            if !view.masks.carry[cell] {
                continue;
            }
            event.area += area;
            let first_arm = (view.hop)(cell).and_then(|target| {
                let key = view.dest_below[target];
                if key == LOST || key == PLATE {
                    return None;
                }
                let (strut, layer) = per_event::unpack(key);
                (layer + 1 == k).then_some((strut, layer))
            });
            if let Some((strut, layer)) = first_arm {
                *local.entry(per_event::entry(strut, layer)).or_insert(0.0) += area;
            } else if view.dest[cell] == LOST {
                event.lost += area;
            } else if k == 0 {
                event.plate_direct += area;
            } else {
                let root = uf.find(lab_cur[cell]);
                let (x, y) = cell_xy(grid, cell);
                let at = *body_index.entry(root).or_insert_with(|| {
                    event.bodies.push(BodyLoad {
                        root,
                        force: 0.0,
                        sx: 0.0,
                        sy: 0.0,
                    });
                    event.bodies.len() - 1
                });
                let body = &mut event.bodies[at];
                body.force += area;
                body.sx += x * area;
                body.sy += y * area;
            }
        }
        for (&key, &load) in &local {
            let (strut, layer) = per_event::unpack(key);
            match net.mem_of.get(strut).copied() {
                Some(m) if m != usize::MAX => event.local.push((m, layer, load)),
                _ => event.lost += load,
            }
        }
        event
            .local
            .sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        if event.area > 0.0 {
            snaps.push(snap_nodes.iter().map(|&n| uf.find(n)).collect());
            events.push(event);
        }
        core::mem::swap(&mut lab_prev, &mut lab_cur);
        cover_prev.copy_from_slice(cover);
    });
    Walked {
        events,
        snaps,
        tip_snap,
        land_snap,
    }
}

// ----------------------------------------------------------------- the solve

/// Axial compliance of each segment, `Σ L³ / (π r² h²)` per layer along its
/// own (possibly leaned) axis. The modulus cancels, so it is omitted.
fn seg_compliance(grid: &Grid, struts: &[SweepStrut], net: &Net, height: f64) -> Vec<f64> {
    let mut out = vec![0.0; net.segs.len()];
    for (sid, seg) in net.segs.iter().enumerate() {
        let s = &struts[net.members[seg.member].strut];
        let radius_at = |slot: usize| s.radius_mm.get(slot).copied().unwrap_or(0.0).max(1.0e-3);
        let mut c = 0.0;
        if seg.lay_lo <= seg.lay_hi {
            for l in seg.lay_lo..=seg.lay_hi {
                let r = radius_at(l - s.k_bottom);
                let side = if s.path.is_empty() || l + 1 >= s.k_top {
                    0.0
                } else {
                    crate::cell_distance_mm(grid, s.cell_at(l), s.cell_at(l + 1))
                };
                let len = height.hypot(side);
                c += len * len * len / (core::f64::consts::PI * r * r * height * height);
            }
        } else {
            let r = s.radius_mm.last().copied().unwrap_or(0.0).max(1.0e-3);
            c = 0.5 * height / (core::f64::consts::PI * r * r);
        }
        out[sid] = c;
    }
    out
}

/// Push `adds` down every member's chain, children first. Model landings
/// become body loads (`to_body`), plate landings plate reaction.
fn propagate(
    net: &Net,
    adds: &mut [f64],
    force: &mut [f64],
    to_body: &mut Vec<(usize, f64)>,
    plate: &mut f64,
) {
    for &m in &net.topo {
        let mem = &net.members[m];
        let mut running = 0.0;
        for s in mem.seg0..mem.seg0 + mem.nseg {
            running += adds[s];
            adds[s] = 0.0;
            force[s] = running;
        }
        if running <= 0.0 {
            continue;
        }
        match mem.bottom {
            Bottom::Plate => *plate += running,
            Bottom::Parent { seg } => adds[seg] += running,
            Bottom::Model => to_body.push((m, running)),
        }
    }
}

/// Dense solve with partial pivoting and a small regularization (the tilt
/// unknowns of a body held by one tree are singular).
fn solve(kmat: &[f64], rhs: &[f64], dim: usize) -> Vec<f64> {
    if dim == 0 {
        return Vec::new();
    }
    let mut m = kmat.to_vec();
    let mut b = rhs.to_vec();
    let mut maxd: f64 = 0.0;
    for i in 0..dim {
        maxd = maxd.max(m[i * dim + i].abs());
    }
    for i in 0..dim {
        for j in (i + 1)..dim {
            let s = 0.5 * (m[i * dim + j] + m[j * dim + i]);
            m[i * dim + j] = s;
            m[j * dim + i] = s;
        }
        let eps = if i % 3 == 0 { 1.0e-12 } else { 1.0e-7 };
        m[i * dim + i] += eps * maxd.max(1.0e-12);
    }
    for col in 0..dim {
        let mut piv = col;
        for r in col..dim {
            if m[r * dim + col].abs() > m[piv * dim + col].abs() {
                piv = r;
            }
        }
        if m[piv * dim + col].abs() < 1.0e-300 {
            continue;
        }
        if piv != col {
            for j in 0..dim {
                m.swap(col * dim + j, piv * dim + j);
            }
            b.swap(col, piv);
        }
        let d = m[col * dim + col];
        for r in (col + 1)..dim {
            let f = m[r * dim + col] / d;
            for j in col..dim {
                m[r * dim + j] -= f * m[col * dim + j];
            }
            b[r] -= f * b[col];
        }
    }
    let mut x = vec![0.0; dim];
    for i in (0..dim).rev() {
        let mut s = b[i];
        for j in (i + 1)..dim {
            s -= m[i * dim + j] * x[j];
        }
        let d = m[i * dim + i];
        x[i] = if d.abs() < 1.0e-300 { 0.0 } else { s / d };
    }
    x
}

/// The rigid-body share on every segment, as the maximum over events, and the
/// event that governs it.
#[expect(
    clippy::too_many_lines,
    reason = "the probe's rigid solve kept whole so it stays line-comparable with the measured \
          rule (b): local term, bodies, the per-tree Thevenin pass, the body solve, the forces"
)]
fn evaluate(
    struts: &[SweepStrut],
    net: &Net,
    walked: &Walked,
    compliance: &[f64],
    report: &mut RigidShareReport,
) -> (Vec<f64>, Vec<usize>) {
    let n = net.segs.len();
    let k_seg: Vec<f64> = compliance.iter().map(|c| 1.0 / c.max(1.0e-12)).collect();
    let mut design = vec![0.0; n];
    let mut arg_event = vec![0_usize; n];
    let mut adds = vec![0.0; n];
    let mut local_force = vec![0.0; n];
    let mut rigid_force = vec![0.0; n];
    let mut to_body: Vec<(usize, f64)> = Vec::new();
    let nodes = net.nodes.len();
    let mut a = vec![0.0; nodes];
    let mut bcoef = vec![0.0; nodes];
    let mut u = vec![0.0; nodes];
    let mut presc = vec![f64::NAN; nodes];
    for (ei, ev) in walked.events.iter().enumerate() {
        let k = ev.layer;
        let snap = &walked.snaps[ei];
        let mut plate_local = ev.plate_direct;
        let mut lost = ev.lost;
        to_body.clear();
        let mut bodies: Vec<BodyLoad> = ev.bodies.clone();
        for &(m, l, v) in &ev.local {
            adds[net.seg_of(m, l)] += v;
        }
        propagate(
            net,
            &mut adds,
            &mut local_force,
            &mut to_body,
            &mut plate_local,
        );
        for &(m, v) in &to_body {
            let ls = walked.land_snap[m];
            let node = &net.nodes[net.members[m].bottom_node];
            if ls != NONE && (ls as usize) < snap.len() {
                let root = snap[ls as usize];
                if let Some(body) = bodies.iter_mut().find(|body| body.root == root) {
                    body.force += v;
                    body.sx += v * node.x;
                    body.sy += v * node.y;
                } else {
                    bodies.push(BodyLoad {
                        root,
                        force: v,
                        sx: v * node.x,
                        sy: v * node.y,
                    });
                }
            } else {
                lost += v;
            }
        }
        let comp_of_node = |node: usize| -> Option<u32> {
            let snap_ix = match net.nodes[node].kind {
                NodeKind::Tip(m) => {
                    let s = &struts[net.members[m].strut];
                    if !net.members[m].is_arm || s.k_top > k {
                        return None;
                    }
                    walked.tip_snap[m]
                }
                NodeKind::ModelPoint(m) => walked.land_snap[m],
                _ => return None,
            };
            (snap_ix != NONE && (snap_ix as usize) < snap.len()).then(|| snap[snap_ix as usize])
        };
        let mut comp_index: HashMap<u32, usize> = HashMap::new();
        let mut ref_sum: Vec<(f64, f64, f64)> = Vec::new();
        let mut active: Vec<(usize, Vec<usize>)> = Vec::new();
        for (ti, tree) in net.trees.iter().enumerate() {
            let mut tcomps: Vec<u32> = Vec::new();
            for &t in &tree.tips {
                if let Some(c) = comp_of_node(t) {
                    if !tcomps.contains(&c) {
                        tcomps.push(c);
                    }
                }
            }
            if tcomps.is_empty() {
                continue;
            }
            if let NodeKind::ModelPoint(_) = net.nodes[tree.root].kind {
                if let Some(rc) = comp_of_node(tree.root) {
                    if tcomps.len() == 1 && tcomps[0] == rc {
                        // Internal to one rigid body: it carries no force.
                        continue;
                    }
                    if !tcomps.contains(&rc) {
                        tcomps.push(rc);
                    }
                }
            }
            let mut cix = Vec::new();
            for c in tcomps {
                let next = comp_index.len();
                let ix = *comp_index.entry(c).or_insert(next);
                if ix == ref_sum.len() {
                    ref_sum.push((0.0, 0.0, 0.0));
                }
                cix.push(ix);
            }
            for &t in &tree.tips {
                if let Some(c) = comp_of_node(t) {
                    let ix = comp_index[&c];
                    ref_sum[ix].0 += 1.0;
                    ref_sum[ix].1 += net.nodes[t].x;
                    ref_sum[ix].2 += net.nodes[t].y;
                }
            }
            active.push((ti, cix));
        }
        let refp: Vec<(f64, f64)> = ref_sum
            .iter()
            .map(|&(c, x, y)| if c > 0.0 { (x / c, y / c) } else { (0.0, 0.0) })
            .collect();
        let dim = 3 * comp_index.len();
        let mut kmat = vec![0.0; dim * dim];
        let mut tree_modes: Vec<Vec<(usize, Vec<f64>)>> = Vec::with_capacity(active.len());
        for (ti, cix) in &active {
            let tree = &net.trees[*ti];
            let mut modes_out = Vec::new();
            for &ci in cix {
                for j in 0..3 {
                    let mode = 3 * ci + j;
                    for &v in &tree.order {
                        presc[v] = f64::NAN;
                        if let Some(c) = comp_of_node(v) {
                            presc[v] = if comp_index.get(&c).copied() == Some(ci) {
                                let (rx, ry) = refp[ci];
                                match j {
                                    0 => 1.0,
                                    1 => net.nodes[v].x - rx,
                                    _ => net.nodes[v].y - ry,
                                }
                            } else {
                                0.0
                            };
                        }
                    }
                    if presc[tree.root].is_nan() {
                        presc[tree.root] = 0.0;
                    }
                    for &v in tree.order.iter().rev() {
                        if !presc[v].is_nan() && v != tree.root {
                            continue;
                        }
                        let mut av = 0.0;
                        let mut bv = 0.0;
                        for &e in &net.nodes[v].ups {
                            let w = net.segs[e].upper;
                            let ke = k_seg[e];
                            if !presc[w].is_nan() {
                                av += ke * presc[w];
                                bv += ke;
                            } else if bcoef[w] > 0.0 {
                                let d = bcoef[w] + ke;
                                av += ke * a[w] / d;
                                bv += ke * bcoef[w] / d;
                            }
                        }
                        a[v] = av;
                        bcoef[v] = bv;
                    }
                    let mut tens = vec![0.0; tree.segs.len()];
                    u[tree.root] = presc[tree.root];
                    for &v in &tree.order {
                        let uv = if presc[v].is_nan() { u[v] } else { presc[v] };
                        u[v] = uv;
                        for &e in &net.nodes[v].ups {
                            let w = net.segs[e].upper;
                            let ke = k_seg[e];
                            if presc[w].is_nan() {
                                u[w] = if bcoef[w] > 0.0 {
                                    (a[w] + ke * uv) / (bcoef[w] + ke)
                                } else {
                                    uv
                                };
                            }
                        }
                    }
                    for (li, &e) in tree.segs.iter().enumerate() {
                        let seg = &net.segs[e];
                        let uu = if presc[seg.upper].is_nan() {
                            u[seg.upper]
                        } else {
                            presc[seg.upper]
                        };
                        let ul = if presc[seg.lower].is_nan() {
                            u[seg.lower]
                        } else {
                            presc[seg.lower]
                        };
                        tens[li] = k_seg[e] * (uu - ul);
                    }
                    for &v in &tree.order {
                        let Some(c) = comp_of_node(v) else { continue };
                        let Some(&cv) = comp_index.get(&c) else {
                            continue;
                        };
                        let (rx, ry) = refp[cv];
                        let tvec = [1.0, net.nodes[v].x - rx, net.nodes[v].y - ry];
                        let seg_tension = |e: usize| {
                            tree.segs
                                .iter()
                                .position(|&x| x == e)
                                .map_or(0.0, |li| tens[li])
                        };
                        let t = match net.nodes[v].kind {
                            NodeKind::Tip(_) => {
                                let mut t = seg_tension(net.nodes[v].down);
                                for &e in &net.nodes[v].ups {
                                    t -= seg_tension(e);
                                }
                                t
                            }
                            NodeKind::ModelPoint(_) => {
                                let mut t = 0.0;
                                for &e in &net.nodes[v].ups {
                                    t -= seg_tension(e);
                                }
                                t
                            }
                            _ => continue,
                        };
                        for (jj, component) in tvec.iter().enumerate() {
                            kmat[(3 * cv + jj) * dim + mode] += t * component;
                        }
                    }
                    modes_out.push((mode, tens));
                }
            }
            tree_modes.push(modes_out);
        }
        let mut rhs = vec![0.0; dim];
        for body in &bodies {
            if body.force <= 0.0 {
                continue;
            }
            match comp_index.get(&body.root) {
                Some(&ci) => {
                    let (rx, ry) = refp[ci];
                    rhs[3 * ci] += body.force;
                    rhs[3 * ci + 1] += body.sx - body.force * rx;
                    rhs[3 * ci + 2] += body.sy - body.force * ry;
                }
                // A body no tree holds yet: nothing carries it.
                None => lost += body.force,
            }
        }
        let solution = solve(&kmat, &rhs, dim);
        rigid_force.iter_mut().for_each(|x| *x = 0.0);
        for (ai, (ti, _)) in active.iter().enumerate() {
            let tree = &net.trees[*ti];
            for (mode, tens) in &tree_modes[ai] {
                let coef = solution[*mode];
                for (li, &e) in tree.segs.iter().enumerate() {
                    rigid_force[e] += coef * tens[li];
                }
            }
        }
        let mut plate_rigid = 0.0;
        for (ti, _) in &active {
            let tree = &net.trees[*ti];
            if let NodeKind::Ground = net.nodes[tree.root].kind {
                for &e in &net.nodes[tree.root].ups {
                    plate_rigid += rigid_force[e];
                }
            }
        }
        let residual = ((plate_rigid + plate_local + lost) - ev.area).abs() / ev.area;
        report.conservation_residual = report.conservation_residual.max(residual);
        for i in 0..n {
            let f = rigid_force[i] + local_force[i];
            if f < 0.0 {
                report.compressive_max_mm2 = report.compressive_max_mm2.max(-f);
            }
            if f > design[i] {
                design[i] = f;
                arg_event[i] = k;
            }
            local_force[i] = 0.0;
        }
        report.events_solved += 1;
    }
    (design, arg_event)
}

/// Run the rigid-body reading on a FINISHED, sized field and report the arms
/// whose share passes `ℓ_max`. Never changes the field.
#[must_use]
pub fn diagnose(
    grid: &Grid,
    slices: &[Vec<SliceSolid>],
    struts: &[SweepStrut],
    log: &RouteLog,
    layer_height_mm: f64,
    l_max: f64,
    tip_radius_max_mm: f64,
) -> RigidShareReport {
    let mut report = RigidShareReport::default();
    if log.is_empty() || struts.is_empty() {
        return report;
    }
    let net = build_net(grid, struts);
    let walked = walk_events(grid, slices, struts, log, &net);
    let compliance = seg_compliance(grid, struts, &net, layer_height_mm);
    let (design, arg_event) = evaluate(struts, &net, &walked, &compliance, &mut report);
    report.ran = true;
    report.members = net.members.len();
    report.trees = net.trees.len();
    report.max_share_id = u32::MAX;
    let capacity = core::f64::consts::PI * tip_radius_max_mm * tip_radius_max_mm;
    let mut landed_on = vec![0_usize; struts.len()];
    for strut in struts {
        if let Landing::Strut { id, .. } = strut.landing {
            if let Some(count) = landed_on.get_mut(id as usize) {
                *count += 1;
            }
        }
    }
    for mem in net.members.iter().filter(|mem| mem.is_arm) {
        let strut = &struts[mem.strut];
        let (share, event_layer) = (mem.seg0..mem.seg0 + mem.nseg)
            .map(|s| (design[s], arg_event[s]))
            .fold(
                (0.0_f64, 0_usize),
                |acc, x| if x.0 > acc.0 { x } else { acc },
            );
        if share > report.max_share_mm2 {
            report.max_share_mm2 = share;
            report.max_share_id = strut.id;
        }
        if share > l_max {
            report.over_limit.push(RigidShareWarning {
                strut_id: strut.id,
                share_mm2: share,
                realized_x: share / capacity,
                sized_mm2: strut.carried_mm2.first().copied().unwrap_or(0.0),
                event_layer,
                members_landed_on_it: landed_on[mem.strut],
                landing: strut.landing,
                root_radius_mm: strut.radius_mm.first().copied().unwrap_or(0.0),
            });
        }
    }
    report.over_limit.sort_by(|a, b| {
        b.share_mm2
            .total_cmp(&a.share_mm2)
            .then(a.strut_id.cmp(&b.strut_id))
    });
    report
}
