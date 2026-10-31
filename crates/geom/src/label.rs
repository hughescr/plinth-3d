//! Print-order connected components over the layer rasters (lane B; design
//! `step11-construction.md` section 4.2).
//!
//! Pixels link by 4-connectivity inside a layer and to overlapping pixels of layer k-1.
//! Components merge when they first touch; the merge layer is recorded.
//!
//! Rules that the numbers depend on (all choices of this module, stated so the reader can
//! check them):
//!
//! * A piece of layer k that touches no pixel of layer k-1 starts a new component.
//! * When one piece touches several components of layer k-1, they merge at layer k. The
//!   component with the most pixels in layer k-1 survives (ties: the lower id), so a small
//!   island that joins a big body is the one marked `merged_into`.
//! * `first_layer_width_px` and `first_layer_height_px` are the largest bounding-box width
//!   and height over the layers in which the component was its own component (before it
//!   merged into another). For a component that never merges they are its largest extents.

use crate::counters::{self, Op};
use crate::measure::Measure;
use crate::raster::{LayerRaster, RasterStack, Run};

/// One print-order component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Component {
    /// Component id, dense from 0.
    pub id: u32,
    /// The first layer in which it has pixels.
    pub first_layer: u32,
    /// Largest extent in x over its first layers, pixels.
    pub first_layer_width_px: u32,
    /// Largest extent in y over its first layers, pixels.
    pub first_layer_height_px: u32,
    /// The component this one merges into, and the layer where they first touch.
    pub merged_into: Option<(u32, u32)>,
}

/// The components of a raster stack.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Labels {
    /// Every component, by id.
    pub components: Vec<Component>,
    /// For each layer, for each run of that layer's raster (same order), the component id.
    pub run_component: Vec<Vec<u32>>,
}

/// A tiny union-find over `usize` (path halving, smaller index is the root).
#[derive(Debug, Default)]
struct Dsu(Vec<usize>);

impl Dsu {
    fn with(n: usize) -> Self {
        Self((0..n).collect())
    }

    fn find(&mut self, mut a: usize) -> usize {
        while self.0[a] != a {
            self.0[a] = self.0[self.0[a]];
            a = self.0[a];
        }
        a
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
            self.0[hi] = lo;
        }
    }
}

fn overlap(a: &Run, b: &Run) -> bool {
    a.x0 < b.x1 && b.x0 < a.x1
}

/// Calls `f(i, j)` for every pair of runs `i` of `a` and `j` of `b` that overlap in x, where
/// the row of `b` is the row of `a` plus `row_shift` (0 or 1). Both slices are sorted by
/// (row, x0).
fn for_overlaps(a: &[Run], b: &[Run], row_shift: u32, mut f: impl FnMut(usize, usize)) {
    let mut j0 = 0;
    for (i, ra) in a.iter().enumerate() {
        let want = ra.y + row_shift;
        while j0 < b.len() && b[j0].y < want {
            j0 += 1;
        }
        let mut j = j0;
        while j < b.len() && b[j].y == want && b[j].x0 < ra.x1 {
            if overlap(ra, &b[j]) {
                f(i, j);
            }
            j += 1;
        }
    }
}

/// The in-plane pieces of one layer: for each run, a piece id (the lowest run index of it).
fn pieces(runs: &[Run]) -> Vec<usize> {
    let mut d = Dsu::with(runs.len());
    for_overlaps(runs, runs, 1, |i, j| d.union(i, j));
    (0..runs.len()).map(|i| d.find(i)).collect()
}

/// Per-component running facts.
#[derive(Debug, Clone, Copy, Default)]
struct Live {
    pixels: u64,
}

struct Builder {
    comps: Vec<Component>,
    parent: Vec<u32>,
    live: Vec<Live>,
}

impl Builder {
    fn root(&mut self, mut c: u32) -> u32 {
        while self.parent[c as usize] != c {
            let p = self.parent[c as usize];
            self.parent[c as usize] = self.parent[p as usize];
            c = self.parent[c as usize];
        }
        c
    }

    /// Merge the roots in `touched` at `layer`; return the survivor.
    fn merge(&mut self, touched: &[u32], layer: u32) -> u32 {
        let mut best = touched[0];
        for &c in &touched[1..] {
            let (pb, pc) = (
                self.live[best as usize].pixels,
                self.live[c as usize].pixels,
            );
            if pc > pb || (pc == pb && c < best) {
                best = c;
            }
        }
        for &c in touched {
            if c != best {
                self.parent[c as usize] = best;
                self.comps[c as usize].merged_into = Some((best, layer));
            }
        }
        best
    }

    fn new_component(&mut self, layer: u32) -> u32 {
        #[allow(clippy::cast_possible_truncation)]
        let id = self.comps.len() as u32;
        self.comps.push(Component {
            id,
            first_layer: layer,
            first_layer_width_px: 0,
            first_layer_height_px: 0,
            merged_into: None,
        });
        self.parent.push(id);
        self.live.push(Live::default());
        id
    }
}

/// Label the components of `stack`.
#[must_use]
pub fn label_components(stack: &RasterStack) -> Measure<Labels> {
    counters::record(Op::LabelComponents, stack.layers.len());
    let mut b = Builder {
        comps: Vec::new(),
        parent: Vec::new(),
        live: Vec::new(),
    };
    let mut run_component: Vec<Vec<u32>> = Vec::with_capacity(stack.layers.len());
    let empty = LayerRaster::default();
    for (k, layer) in stack.layers.iter().enumerate() {
        let prev = if k == 0 { &empty } else { &stack.layers[k - 1] };
        let prev_ids: &[u32] = if k == 0 { &[] } else { &run_component[k - 1] };
        #[allow(clippy::cast_possible_truncation)]
        let ids = label_layer(&mut b, layer, prev, prev_ids, k as u32);
        run_component.push(ids);
    }
    Measure::Known(Labels {
        components: b.comps,
        run_component,
    })
}

fn label_layer(
    b: &mut Builder,
    layer: &LayerRaster,
    prev: &LayerRaster,
    prev_ids: &[u32],
    k: u32,
) -> Vec<u32> {
    let piece = pieces(&layer.runs);
    // The components of layer k-1 each piece touches.
    let mut touched: Vec<Vec<u32>> = vec![Vec::new(); layer.runs.len()];
    for_overlaps(&layer.runs, &prev.runs, 0, |i, j| {
        let c = b.root(prev_ids[j]);
        let t = &mut touched[piece[i]];
        if !t.contains(&c) {
            t.push(c);
        }
    });
    // Decide each piece's component, in piece order.
    let mut of_piece: Vec<Option<u32>> = vec![None; layer.runs.len()];
    for (i, &p) in piece.iter().enumerate() {
        if p != i {
            continue;
        }
        let mut roots: Vec<u32> = touched[i].iter().map(|&c| b.root(c)).collect();
        roots.sort_unstable();
        roots.dedup();
        of_piece[i] = Some(if roots.is_empty() {
            b.new_component(k)
        } else {
            b.merge(&roots, k)
        });
    }
    // Final roots after every merge of this layer, then the layer's per-component facts.
    let ids: Vec<u32> = (0..layer.runs.len())
        .map(|i| b.root(of_piece[piece[i]].unwrap_or(0)))
        .collect();
    record_layer(b, layer, &ids);
    ids
}

fn record_layer(b: &mut Builder, layer: &LayerRaster, ids: &[u32]) {
    let mut seen: Vec<u32> = ids.to_vec();
    seen.sort_unstable();
    seen.dedup();
    for &c in &seen {
        b.live[c as usize].pixels = 0;
    }
    // (min x0, max x1, min y, max y) per component of this layer.
    let mut boxes: Vec<(u32, u32, u32, u32)> = vec![(u32::MAX, 0, u32::MAX, 0); seen.len()];
    for (run, &c) in layer.runs.iter().zip(ids) {
        let at = seen.binary_search(&c).unwrap_or(0);
        b.live[c as usize].pixels += u64::from(run.x1 - run.x0);
        let e = &mut boxes[at];
        *e = (
            e.0.min(run.x0),
            e.1.max(run.x1),
            e.2.min(run.y),
            e.3.max(run.y + 1),
        );
    }
    for (c, e) in seen.iter().zip(boxes) {
        let comp = &mut b.comps[*c as usize];
        comp.first_layer_width_px = comp.first_layer_width_px.max(e.1 - e.0);
        comp.first_layer_height_px = comp.first_layer_height_px.max(e.3 - e.2);
    }
}

/// A closed pocket of uncured resin (design section 4.6): an unfilled in-plane piece of one
/// layer with no path through unfilled pixels of layers up to that one to the outside.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CupPiece {
    /// The layer.
    pub layer: u32,
    /// Unfilled pixels in the piece.
    pub pixels: u64,
    /// Centre of the piece in pixel coordinates (x, y).
    pub centre_px: [f64; 2],
}

/// Complement runs of one layer: the unfilled gaps of every row, ends included.
fn gaps(layer: &LayerRaster, width: u32, height: u32) -> Vec<Run> {
    let mut out = Vec::new();
    let mut it = layer.runs.iter().peekable();
    for y in 0..height {
        let mut x = 0;
        while let Some(r) = it.next_if(|r| r.y == y) {
            if r.x0 > x {
                out.push(Run { y, x0: x, x1: r.x0 });
            }
            x = r.x1;
        }
        if x < width {
            out.push(Run {
                y,
                x0: x,
                x1: width,
            });
        }
    }
    out
}

/// Find the print-order cups of `stack`: per layer, the unfilled in-plane pieces that cannot
/// reach the outside through unfilled pixels of layers up to and including that one. Layer
/// 0 has nothing below it, so none of its pieces is closed. Output is in layer order.
#[must_use]
pub fn find_cups(stack: &RasterStack) -> Measure<Vec<CupPiece>> {
    let [w, h] = stack.size;
    let mut cups = Vec::new();
    let mut prev_gaps: Vec<Run> = Vec::new();
    let mut prev_open: Vec<bool> = Vec::new();
    for (k, layer) in stack.layers.iter().enumerate() {
        let g = gaps(layer, w, h);
        let piece = pieces(&g);
        // A piece is open when it touches the grid edge or overlaps an open gap below.
        let mut open = vec![false; g.len()];
        for (i, r) in g.iter().enumerate() {
            if k == 0 || r.y == 0 || r.y + 1 == h || r.x0 == 0 || r.x1 == w {
                open[piece[i]] = true;
            }
        }
        for_overlaps(&g, &prev_gaps, 0, |i, j| {
            if prev_open[j] {
                open[piece[i]] = true;
            }
        });
        let run_open: Vec<bool> = (0..g.len()).map(|i| open[piece[i]]).collect();
        let mut sums: Vec<(usize, u64, f64, f64)> = Vec::new();
        for (i, r) in g.iter().enumerate() {
            if run_open[i] {
                continue;
            }
            let n = u64::from(r.x1 - r.x0);
            let cx = 0.5 * (f64::from(r.x0) + f64::from(r.x1));
            let cy = f64::from(r.y) + 0.5;
            match sums.binary_search_by_key(&piece[i], |s| s.0) {
                Ok(at) => {
                    let s = &mut sums[at];
                    s.1 += n;
                    s.2 += cx * n as f64;
                    s.3 += cy * n as f64;
                }
                Err(at) => sums.insert(at, (piece[i], n, cx * n as f64, cy * n as f64)),
            }
        }
        for (_, n, sx, sy) in sums {
            cups.push(CupPiece {
                #[allow(clippy::cast_possible_truncation)]
                layer: k as u32,
                pixels: n,
                centre_px: [sx / n as f64, sy / n as f64],
            });
        }
        prev_open = run_open;
        prev_gaps = g;
    }
    Measure::Known(cups)
}

/// Material that appears in a layer with nothing below it, and how far it reaches.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReachPiece {
    /// The layer.
    pub layer: u32,
    /// The farthest a new pixel of the piece is from a pixel that has material below it,
    /// mm. Infinite when the piece touches no supported pixel within the search window.
    pub reach_mm: f64,
    /// Pixel count of the piece (a structure count, not an area).
    pub pixels: u64,
    /// The new pixel that is farthest from support, as (x, y) pixel coordinates.
    pub farthest_px: [u32; 2],
}

/// The new material of each layer (pixels filled in layer k but not in k-1), grouped into
/// in-plane pieces, with each piece's reach beyond supported pixels. Only pieces whose reach
/// is above `limit_mm` are listed. The search window around a piece is `limit_mm` plus one
/// pixel wide; a piece with no supported pixel in its window has infinite reach.
#[must_use]
pub fn find_reach(stack: &RasterStack, limit_mm: f64) -> Measure<Vec<ReachPiece>> {
    let mut out = Vec::new();
    for k in 1..stack.layers.len() {
        let (cur, below) = (&stack.layers[k], &stack.layers[k - 1]);
        let new_runs = subtract_runs(&cur.runs, &below.runs);
        if new_runs.is_empty() {
            continue;
        }
        let supp = intersect_runs(&cur.runs, &below.runs);
        let piece = pieces(&new_runs);
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut ids: Vec<usize> = Vec::new();
        for (i, &pc) in piece.iter().enumerate() {
            let at = ids.binary_search(&pc).unwrap_or_else(|p| {
                ids.insert(p, pc);
                groups.insert(p, Vec::new());
                p
            });
            groups[at].push(i);
        }
        for members in groups {
            #[allow(clippy::cast_possible_truncation)]
            if let Some(p) = reach_of(stack, &new_runs, &members, &supp, limit_mm, k as u32) {
                out.push(p);
            }
        }
    }
    Measure::Known(out)
}

/// The runs of `a` not covered by `b` (both sorted by row, x0).
fn subtract_runs(a: &[Run], b: &[Run]) -> Vec<Run> {
    let mut out = Vec::new();
    let mut j0 = 0;
    for r in a {
        while j0 < b.len() && b[j0].y < r.y {
            j0 += 1;
        }
        let mut x = r.x0;
        let mut j = j0;
        while j < b.len() && b[j].y == r.y && b[j].x0 < r.x1 {
            if b[j].x1 > x {
                if b[j].x0 > x {
                    out.push(Run {
                        y: r.y,
                        x0: x,
                        x1: b[j].x0,
                    });
                }
                x = x.max(b[j].x1);
            }
            j += 1;
        }
        if x < r.x1 {
            out.push(Run {
                y: r.y,
                x0: x,
                x1: r.x1,
            });
        }
    }
    out
}

/// The runs of `a` that `b` covers (both sorted by row, x0).
fn intersect_runs(a: &[Run], b: &[Run]) -> Vec<Run> {
    let mut out = Vec::new();
    let mut j0 = 0;
    for r in a {
        while j0 < b.len() && b[j0].y < r.y {
            j0 += 1;
        }
        let mut j = j0;
        while j < b.len() && b[j].y == r.y && b[j].x0 < r.x1 {
            let (lo, hi) = (r.x0.max(b[j].x0), r.x1.min(b[j].x1));
            if lo < hi {
                out.push(Run {
                    y: r.y,
                    x0: lo,
                    x1: hi,
                });
            }
            j += 1;
        }
    }
    out
}

/// An upper bound, in pixels, on how far a pixel of the new run `r` is from a supported
/// pixel of the same row (infinite when the row has none on one side and the run touches
/// no supported pixel on the other).
fn row_bound_px(r: &Run, supp: &[Run]) -> f64 {
    let at = supp.partition_point(|s| (s.y, s.x1) <= (r.y, r.x0));
    let right = supp.get(at).filter(|s| s.y == r.y).map(|s| s.x0);
    let left = at
        .checked_sub(1)
        .and_then(|i| supp.get(i))
        .filter(|s| s.y == r.y)
        .map(|s| s.x1 - 1);
    let (a, b) = (f64::from(r.x0), f64::from(r.x1 - 1));
    match (left, right) {
        (Some(l), Some(t)) => {
            let (l, t) = (f64::from(l), f64::from(t));
            (0.5 * (t - l)).min(b - l).min(t - a)
        }
        (Some(l), None) => b - f64::from(l),
        (None, Some(t)) => f64::from(t) - a,
        (None, None) => f64::INFINITY,
    }
}

/// One piece's reach by an exact distance transform in a window around it.
fn reach_of(
    stack: &RasterStack,
    new_runs: &[Run],
    members: &[usize],
    supp: &[Run],
    limit_mm: f64,
    layer: u32,
) -> Option<ReachPiece> {
    let [px, py] = stack.pitch;
    // Cheap exit: along each row the nearest supported pixel bounds the true distance.
    let worst_row = members
        .iter()
        .map(|&i| row_bound_px(&new_runs[i], supp))
        .fold(0.0_f64, f64::max);
    if worst_row * px <= limit_mm {
        return None;
    }
    let mx = (limit_mm / px).ceil() as u32 + 1;
    let my = (limit_mm / py).ceil() as u32 + 1;
    let (mut x0, mut x1, mut y0, mut y1) = (u32::MAX, 0, u32::MAX, 0);
    let mut pixels = 0;
    for &i in members {
        let r = new_runs[i];
        x0 = x0.min(r.x0);
        x1 = x1.max(r.x1);
        y0 = y0.min(r.y);
        y1 = y1.max(r.y + 1);
        pixels += u64::from(r.x1 - r.x0);
    }
    // The distance transform runs on a coarse grid of COARSE x COARSE pixel cells: a cell is
    // supported when it holds any supported pixel. That makes the reach at most one coarse
    // cell short of the pixel answer (0.06 x 0.08 mm); the limit is judged on the coarse value.
    let (wx0, wx1) = (
        x0.saturating_sub(mx) / COARSE,
        (x1 + mx).min(stack.size[0]).div_ceil(COARSE),
    );
    let (wy0, wy1) = (
        y0.saturating_sub(my) / COARSE,
        (y1 + my).min(stack.size[1]).div_ceil(COARSE),
    );
    let (ww, wh) = ((wx1 - wx0) as usize, (wy1 - wy0) as usize);
    let mut f = vec![f64::INFINITY; ww * wh];
    let lo_row = wy0 * COARSE;
    let hi_row = wy1 * COARSE;
    let first = supp.partition_point(|r| r.y < lo_row);
    for r in supp[first..].iter().take_while(|r| r.y < hi_row) {
        let (a, b) = (
            r.x0.max(wx0 * COARSE) / COARSE,
            (r.x1 - 1).min(wx1 * COARSE - 1) / COARSE,
        );
        if r.x1 <= wx0 * COARSE || r.x0 >= wx1 * COARSE {
            continue;
        }
        let cy = (r.y / COARSE - wy0) as usize;
        for cx in a..=b {
            f[cy * ww + (cx - wx0) as usize] = 0.0;
        }
    }
    let d2 = squared_distance(
        &mut f,
        ww,
        wh,
        px * f64::from(COARSE),
        py * f64::from(COARSE),
    );
    let mut best = (0.0_f64, [0_u32, 0_u32]);
    for &i in members {
        let r = new_runs[i];
        let cy = (r.y / COARSE - wy0) as usize;
        for cx in r.x0 / COARSE..=(r.x1 - 1) / COARSE {
            let v = d2[cy * ww + (cx - wx0) as usize];
            if v > best.0 {
                best = (v, [cx * COARSE + COARSE / 2, r.y]);
            }
        }
    }
    let reach = best.0.sqrt();
    (reach > limit_mm).then_some(ReachPiece {
        layer,
        reach_mm: reach,
        pixels,
        farthest_px: best.1,
    })
}

/// Side of a coarse reach cell, in pixels.
const COARSE: u32 = 4;

/// Squared Euclidean distance (mm²) from every cell to the nearest cell of value 0, with
/// anisotropic pixel pitch (Felzenszwalb and Huttenlocher's two-pass lower envelope).
fn squared_distance(f: &mut [f64], w: usize, h: usize, px: f64, py: f64) -> Vec<f64> {
    let mut tmp = vec![0.0; w.max(h)];
    for y in 0..h {
        let row: Vec<f64> = f[y * w..(y + 1) * w].to_vec();
        transform_1d(&row, px, &mut tmp[..w]);
        f[y * w..(y + 1) * w].copy_from_slice(&tmp[..w]);
    }
    let mut col = vec![0.0; h];
    for x in 0..w {
        for y in 0..h {
            col[y] = f[y * w + x];
        }
        transform_1d(&col, py, &mut tmp[..h]);
        for y in 0..h {
            f[y * w + x] = tmp[y];
        }
    }
    f.to_vec()
}

/// 1-D lower envelope of parabolas: `out[p] = min_q (step (p - q))^2 + f[q]`.
fn transform_1d(f: &[f64], step: f64, out: &mut [f64]) {
    let n = f.len();
    // v[i]: the cell of parabola i. z[i]: where parabola i starts; z[v.len()] is infinity.
    let mut v: Vec<usize> = Vec::with_capacity(n);
    let mut z: Vec<f64> = Vec::with_capacity(n + 1);
    for q in 0..n {
        if !f[q].is_finite() {
            continue;
        }
        if v.is_empty() {
            v.push(q);
            z.push(f64::NEG_INFINITY);
            z.push(f64::INFINITY);
            continue;
        }
        let qf = q as f64 * step;
        let s = loop {
            let last = v[v.len() - 1];
            let vf = last as f64 * step;
            let s = ((f[q] + qf * qf) - (f[last] + vf * vf)) / (2.0 * (qf - vf));
            if s <= z[v.len() - 1] {
                v.pop();
                z.pop();
            } else {
                break s;
            }
        };
        let m = v.len();
        z[m] = s;
        z.push(f64::INFINITY);
        v.push(q);
    }
    if v.is_empty() {
        out.fill(f64::INFINITY);
        return;
    }
    let mut k = 0;
    for (p, o) in out.iter_mut().enumerate() {
        let pos = p as f64 * step;
        while z[k + 1] < pos {
            k += 1;
        }
        let d = pos - v[k] as f64 * step;
        *o = d * d + f[v[k]];
    }
}

/// Scalar reference: dense per-pixel versions of the run-based kernels.
pub mod reference {
    use crate::raster::{LayerRaster, RasterStack};

    fn dense(layer: &LayerRaster, size: [u32; 2]) -> Vec<bool> {
        let mut d = vec![false; (size[0] as usize) * (size[1] as usize)];
        for r in &layer.runs {
            for x in r.x0..r.x1 {
                d[(r.y as usize) * (size[0] as usize) + x as usize] = true;
            }
        }
        d
    }

    /// The component id of every pixel of every layer (`u32::MAX` for empty pixels), by flood
    /// fill with the same survivor rule as [`super::label_components`].
    #[must_use]
    pub fn label_dense(stack: &RasterStack) -> Vec<Vec<u32>> {
        let [w, h] = stack.size;
        let n = (w as usize) * (h as usize);
        let mut out: Vec<Vec<u32>> = Vec::new();
        let mut parent: Vec<u32> = Vec::new();
        let mut size_px: Vec<u64> = Vec::new();
        for (k, layer) in stack.layers.iter().enumerate() {
            let fill = dense(layer, stack.size);
            let below = if k == 0 {
                vec![u32::MAX; n]
            } else {
                out[k - 1].clone()
            };
            let mut cur = vec![u32::MAX; n];
            let mut seen = vec![false; n];
            for start in 0..n {
                if !fill[start] || seen[start] {
                    continue;
                }
                let mut stack_px = vec![start];
                seen[start] = true;
                let mut members = Vec::new();
                let mut touched: Vec<u32> = Vec::new();
                while let Some(p) = stack_px.pop() {
                    members.push(p);
                    if below[p] != u32::MAX {
                        let mut c = below[p];
                        while parent[c as usize] != c {
                            c = parent[c as usize];
                        }
                        if !touched.contains(&c) {
                            touched.push(c);
                        }
                    }
                    let (x, y) = (p % (w as usize), p / (w as usize));
                    let mut nb = Vec::new();
                    if x > 0 {
                        nb.push(p - 1);
                    }
                    if x + 1 < w as usize {
                        nb.push(p + 1);
                    }
                    if y > 0 {
                        nb.push(p - w as usize);
                    }
                    if y + 1 < h as usize {
                        nb.push(p + w as usize);
                    }
                    for q in nb {
                        if fill[q] && !seen[q] {
                            seen[q] = true;
                            stack_px.push(q);
                        }
                    }
                }
                touched.sort_unstable();
                let id = if touched.is_empty() {
                    #[allow(clippy::cast_possible_truncation)]
                    let id = parent.len() as u32;
                    parent.push(id);
                    size_px.push(0);
                    id
                } else {
                    let mut best = touched[0];
                    for &c in &touched[1..] {
                        if size_px[c as usize] > size_px[best as usize] {
                            best = c;
                        }
                    }
                    for &c in &touched {
                        parent[c as usize] = best;
                    }
                    parent[best as usize] = best;
                    best
                };
                for p in members {
                    cur[p] = id;
                }
            }
            // The roots may have changed through later pieces of this layer.
            for c in &mut cur {
                if *c != u32::MAX {
                    while parent[*c as usize] != *c {
                        *c = parent[*c as usize];
                    }
                }
            }
            size_px.fill(0);
            for &c in &cur {
                if c != u32::MAX {
                    size_px[c as usize] += 1;
                }
            }
            out.push(cur);
        }
        out
    }

    /// The dense component image implied by production labels.
    #[must_use]
    pub fn image_of(stack: &RasterStack, labels: &super::Labels) -> Vec<Vec<u32>> {
        let n = (stack.size[0] as usize) * (stack.size[1] as usize);
        stack
            .layers
            .iter()
            .enumerate()
            .map(|(k, l)| {
                let mut img = vec![u32::MAX; n];
                for (r, &c) in l.runs.iter().zip(&labels.run_component[k]) {
                    for x in r.x0..r.x1 {
                        img[(r.y as usize) * (stack.size[0] as usize) + x as usize] = c;
                    }
                }
                img
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
