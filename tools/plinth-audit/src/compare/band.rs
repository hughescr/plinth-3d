//! Run-length structure on the printer grid: grounding, model islands, thin
//! joins, the 1-voxel tolerance band and drift blobs. Ported from the
//! calibration prototype (`metric-raw/simdiff`), cleaned of its environment
//! switches. Everything works on runs; nothing loops over pixels.

use super::consts as k;
use crate::raster::{InPlaneJoin, Layer, Run, RasterGrid, RowRuns, RunRaster};
use std::collections::{BTreeMap, HashMap};

/// A connected cured component on one layer.
#[derive(Clone, Debug)]
pub struct Comp {
    pub k: i64,
    pub px: i64,
    pub cx: f64,
    pub cy: f64,
    pub bb: [f64; 4],
    /// Floating: 0. Island: support pixels bearing it. Grounded: overlap below.
    pub attach: i64,
}

/// Visit every overlapping (run in a, run in b, columns) pair; both sorted by row then column.
pub fn overlap_join(a: &RowRuns, b: &RowRuns, mut f: impl FnMut(usize, usize, i64)) {
    let (mut gi, mut gj) = (0usize, 0usize);
    while gi < a.groups.len() && gj < b.groups.len() {
        let (ra, sa, ea) = a.groups[gi];
        let (rb, sb, eb) = b.groups[gj];
        if ra < rb {
            gi += 1;
            continue;
        }
        if rb < ra {
            gj += 1;
            continue;
        }
        let (mut i, mut j) = (sa, sb);
        while i < ea && j < eb {
            let (x, y) = (a.runs[i].1, b.runs[j].1);
            let lo = x.c0.max(y.c0);
            let hi = x.c1.min(y.c1);
            if hi > lo {
                f(i, j, hi - lo);
            }
            if x.c1 < y.c1 {
                i += 1;
            } else {
                j += 1;
            }
        }
        gi += 1;
        gj += 1;
    }
}

/// Visit every (run in a, run in b) pair whose pixels touch by a face or a corner:
/// rows differ by at most one and the column spans overlap or abut. This is the
/// 3D corner rule between two adjacent layers (a voxel and any of the nine voxels
/// above or below it), the same rule the in-plane `SharedFaceOrCorner` label uses.
pub fn corner_join(a: &RowRuns, b: &RowRuns, mut f: impl FnMut(usize, usize)) {
    for &(ra, sa, ea) in &a.groups {
        for rb in (ra - 1)..=(ra + 1) {
            let Ok(g) = b.groups.binary_search_by_key(&rb, |x| x.0) else { continue };
            let (_, sb, eb) = b.groups[g];
            let bruns = &b.runs[sb..eb];
            for i in sa..ea {
                let x = a.runs[i].1;
                let first = bruns.partition_point(|(_, y)| y.c1 < x.c0);
                for (off, (_, y)) in bruns[first..].iter().enumerate() {
                    if y.c0 > x.c1 {
                        break;
                    }
                    f(i, sb + first + off);
                }
            }
        }
    }
}

pub fn comp_stats(rr: &RowRuns, labels: &[u32], n: usize, g: &RasterGrid, kk: i64) -> Vec<Comp> {
    let mut v: Vec<Comp> = (0..n)
        .map(|_| Comp {
            k: kk,
            px: 0,
            cx: 0.0,
            cy: 0.0,
            bb: [f64::MAX, f64::MIN, f64::MAX, f64::MIN],
            attach: 0,
        })
        .collect();
    for (i, (row, run)) in rr.runs.iter().enumerate() {
        let c = &mut v[labels[i] as usize];
        let len = run.len();
        let y = (*row as f64 + 0.5) * g.pitch_y_mm;
        let xm = (run.c0 + run.c1) as f64 * 0.5 * g.pitch_x_mm;
        c.px += len;
        c.cx += xm * len as f64;
        c.cy += y * len as f64;
        c.bb[0] = c.bb[0].min(run.c0 as f64 * g.pitch_x_mm);
        c.bb[1] = c.bb[1].max(run.c1 as f64 * g.pitch_x_mm);
        c.bb[2] = c.bb[2].min(*row as f64 * g.pitch_y_mm);
        c.bb[3] = c.bb[3].max((*row + 1) as f64 * g.pitch_y_mm);
    }
    for c in &mut v {
        if c.px > 0 {
            c.cx /= c.px as f64;
            c.cy /= c.px as f64;
        }
    }
    v
}

pub fn layer_rr(r: &RunRaster) -> HashMap<i64, RowRuns> {
    r.layers.iter().map(|l| (l.k, RowRuns::from_layer(l))).collect()
}

fn merge_runs(mut v: Vec<Run>) -> Vec<Run> {
    v.sort_by_key(|x| x.c0);
    let mut out: Vec<Run> = Vec::new();
    for x in v {
        match out.last_mut() {
            Some(l) if x.c0 <= l.c1 => l.c1 = l.c1.max(x.c1),
            _ => out.push(x),
        }
    }
    out
}

/// What the grounding pass found.
#[derive(Debug, Default)]
pub struct Grounding {
    /// Cured components with no grounded pixel within the lateral tolerance below.
    pub floating: Vec<Comp>,
    /// Grounded components whose overlap with the layer below is under [`k::N4_THIN_PX`].
    pub thin_joins: usize,
}

/// Bottom-up grounding on the cured union: a component is grounded when it
/// overlaps (or lies within `H1_LATERAL_PX` of) a grounded component one layer
/// below, or sits on layer 0 or below.
pub fn grounding(u: &RunRaster, g: &RasterGrid, lateral_px: i64) -> Grounding {
    let mut out = Grounding::default();
    let mut prev: Option<(i64, RowRuns, Vec<bool>)> = None;
    for l in &u.layers {
        let rr = RowRuns::from_layer(l);
        let (labels, n) = rr.label(InPlaneJoin::SharedFace);
        let mut grounded = vec![l.k <= 0; n];
        let mut attach = vec![0i64; n];
        if let Some((pk, prr, pg)) = &prev {
            if *pk == l.k - 1 {
                overlap_join(&rr, prr, |i, j, len| {
                    if pg[j] {
                        let c = labels[i] as usize;
                        grounded[c] = true;
                        attach[c] += len;
                    }
                });
                if lateral_px > 0 && grounded.iter().any(|x| !x) {
                    let mut rows: BTreeMap<i64, Vec<Run>> = BTreeMap::new();
                    for (j, (row, run)) in prr.runs.iter().enumerate() {
                        if pg[j] {
                            for dr in -lateral_px..=lateral_px {
                                rows.entry(row + dr).or_default().push(Run {
                                    c0: run.c0 - lateral_px,
                                    c1: run.c1 + lateral_px,
                                });
                            }
                        }
                    }
                    let table: Vec<(i64, Vec<Run>)> =
                        rows.into_iter().map(|(r, v)| (r, merge_runs(v))).collect();
                    let dil = RowRuns::from_row_table(&table);
                    overlap_join(&rr, &dil, |i, _, _| grounded[labels[i] as usize] = true);
                }
            }
        }
        if l.k > 0 || grounded.iter().any(|x| !x) {
            let stats = comp_stats(&rr, &labels, n, g, l.k);
            for (c, mut s) in stats.into_iter().enumerate() {
                if grounded[c] {
                    if l.k > 0 && attach[c] < k::N4_THIN_PX {
                        out.thin_joins += 1;
                    }
                } else {
                    s.attach = 0;
                    out.floating.push(s);
                }
            }
        }
        let per_run: Vec<bool> = labels.iter().map(|&c| grounded[c as usize]).collect();
        prev = Some((l.k, rr, per_run));
    }
    out
}

/// Model island births (I7-style): components of the model raster with no model
/// pixel below. `attach` is the support pixels in the island's own runs at
/// layers k-1..=k+2 (its bearing).
pub fn islands(m: &RunRaster, s_rr: &HashMap<i64, RowRuns>, g: &RasterGrid) -> Vec<Comp> {
    let mut out = Vec::new();
    let mut prev: Option<(i64, RowRuns)> = None;
    for l in &m.layers {
        let rr = RowRuns::from_layer(l);
        let (labels, n) = rr.label(InPlaneJoin::SharedFace);
        let mut touched = vec![false; n];
        if let Some((pk, prr)) = &prev {
            if *pk == l.k - 1 {
                overlap_join(&rr, prr, |i, _, _| touched[labels[i] as usize] = true);
            }
        }
        if l.k > 0 && touched.iter().any(|t| !t) {
            let mut bearing = vec![0i64; n];
            for kk in (l.k - 1)..=(l.k + 2) {
                if let Some(srr) = s_rr.get(&kk) {
                    overlap_join(&rr, srr, |i, _, len| {
                        let c = labels[i] as usize;
                        if !touched[c] {
                            bearing[c] += len;
                        }
                    });
                }
            }
            for (c, mut st) in comp_stats(&rr, &labels, n, g, l.k).into_iter().enumerate() {
                if !touched[c] {
                    st.attach = bearing[c];
                    out.push(st);
                }
            }
        }
        prev = Some((l.k, rr));
    }
    out
}

/// 3D 1-voxel tolerance mask: layers k-1..=k+1 of `src`, each run grown by one
/// row and one column. A voxel of the other file inside this mask is "within
/// one voxel" of `src` and is not counted as drift. `grow` = 0 gives the
/// sabotage case (band set to zero).
pub fn tolerance(src: &RunRaster, grow: i64) -> RunRaster {
    let Some((k0, k1)) = src.layer_range() else {
        return RunRaster::empty(src.grid);
    };
    let mut layers = Vec::new();
    for kk in (k0 - grow)..=(k1 + grow) {
        let mut rows: BTreeMap<i64, Vec<Run>> = BTreeMap::new();
        for k3 in (kk - grow)..=(kk + grow) {
            if let Some(l) = src.layer(k3) {
                for (row, runs) in &l.rows {
                    for r in runs {
                        for dr in -grow..=grow {
                            rows.entry(row + dr)
                                .or_default()
                                .push(Run { c0: r.c0 - grow, c1: r.c1 + grow });
                        }
                    }
                }
            }
        }
        if rows.is_empty() {
            continue;
        }
        let rows = rows.into_iter().map(|(r, v)| (r, merge_runs(v))).collect();
        layers.push(Layer { k: kk, z_mm: src.grid.layer_z_mm(kk), rows });
    }
    RunRaster { grid: src.grid, layers, pixel_count: 0, run_count: 0 }
}

/// A 3D drift blob (corner-connected) with its extent.
#[derive(Clone, Debug)]
pub struct Blob {
    pub vox: i64,
    /// Voxels of the blob that exist only in the candidate.
    pub added: i64,
    pub k0: i64,
    pub k1: i64,
    /// x0, x1, y0, y1 in mm.
    pub bb: [f64; 4],
    pub centroid: [f64; 3],
}

/// Connected drift blobs of `x`; `add` marks the candidate-only voxels.
pub fn blobs(x: &RunRaster, add: &RunRaster, g: &RasterGrid) -> Vec<Blob> {
    struct Acc {
        vox: i64,
        add: i64,
        k0: i64,
        k1: i64,
        bb: [f64; 4],
        sx: f64,
        sy: f64,
        sz: f64,
    }
    fn find(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    let add_rr = layer_rr(add);
    let mut parent: Vec<usize> = Vec::new();
    let mut stats: Vec<Acc> = Vec::new();
    let mut prev: Option<(i64, RowRuns, Vec<usize>)> = None;
    for l in &x.layers {
        let rr = RowRuns::from_layer(l);
        let (labels, n) = rr.label(InPlaneJoin::SharedFaceOrCorner);
        let base = parent.len();
        let cs = comp_stats(&rr, &labels, n, g, l.k);
        let mut addpx = vec![0i64; n];
        if let Some(arr) = add_rr.get(&l.k) {
            overlap_join(&rr, arr, |i, _, len| addpx[labels[i] as usize] += len);
        }
        for (c, st) in cs.iter().enumerate() {
            parent.push(base + c);
            stats.push(Acc {
                vox: st.px,
                add: addpx[c],
                k0: l.k,
                k1: l.k,
                bb: st.bb,
                sx: st.cx * st.px as f64,
                sy: st.cy * st.px as f64,
                sz: g.layer_z_mm(l.k) * st.px as f64,
            });
        }
        let gid: Vec<usize> = labels.iter().map(|&c| base + c as usize).collect();
        if let Some((pk, prr, pgid)) = &prev {
            if *pk == l.k - 1 {
                let mut pairs = Vec::new();
                corner_join(&rr, prr, |i, j| pairs.push((gid[i], pgid[j])));
                for (a, b) in pairs {
                    let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                    if ra != rb {
                        parent[ra] = rb;
                    }
                }
            }
        }
        prev = Some((l.k, rr, gid));
    }
    let mut acc: HashMap<usize, Acc> = HashMap::new();
    for i in 0..parent.len() {
        let r = find(&mut parent, i);
        let s = &stats[i];
        let e = acc.entry(r).or_insert(Acc {
            vox: 0,
            add: 0,
            k0: i64::MAX,
            k1: i64::MIN,
            bb: [f64::MAX, f64::MIN, f64::MAX, f64::MIN],
            sx: 0.0,
            sy: 0.0,
            sz: 0.0,
        });
        e.vox += s.vox;
        e.add += s.add;
        e.k0 = e.k0.min(s.k0);
        e.k1 = e.k1.max(s.k1);
        e.bb = [
            e.bb[0].min(s.bb[0]),
            e.bb[1].max(s.bb[1]),
            e.bb[2].min(s.bb[2]),
            e.bb[3].max(s.bb[3]),
        ];
        e.sx += s.sx;
        e.sy += s.sy;
        e.sz += s.sz;
    }
    let mut v: Vec<Blob> = acc
        .into_values()
        .map(|e| {
            let n = e.vox.max(1) as f64;
            Blob {
                vox: e.vox,
                added: e.add,
                k0: e.k0,
                k1: e.k1,
                bb: e.bb,
                centroid: [e.sx / n, e.sy / n, e.sz / n],
            }
        })
        .collect();
    v.sort_by(|a, b| b.vox.cmp(&a.vox).then(a.k0.cmp(&b.k0)));
    v
}

/// Drift beyond the band between two rasters, both ways.
pub struct Drift {
    pub removed: RunRaster,
    pub added: RunRaster,
    pub beyond: RunRaster,
    pub voxels: u64,
    pub blobs: Vec<Blob>,
    /// The tolerance mask of the first raster (what "within the band of `a`" means).
    pub tol_a: RunRaster,
}

/// For each component, the number of its pixels that lie inside `tol` (a tolerance mask) on the
/// same layer. The components are those of [`grounding`] on `u` (shared-face labelling); each is
/// found again by its layer, pixel count and centroid.
#[must_use]
pub fn pixels_inside(u: &RunRaster, tol: &RunRaster, comps: &[&Comp], g: &RasterGrid) -> Vec<i64> {
    let mut by_layer: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for (i, c) in comps.iter().enumerate() {
        by_layer.entry(c.k).or_default().push(i);
    }
    let mut out = vec![0i64; comps.len()];
    for (kk, idxs) in by_layer {
        let Some(l) = u.layer(kk) else { continue };
        let rr = RowRuns::from_layer(l);
        let (labels, n) = rr.label(InPlaneJoin::SharedFace);
        let stats = comp_stats(&rr, &labels, n, g, kk);
        let mut inside = vec![0i64; n];
        if let Some(tl) = tol.layer(kk) {
            let trr = RowRuns::from_layer(tl);
            overlap_join(&rr, &trr, |i, _, len| inside[labels[i] as usize] += len);
        }
        for ci in idxs {
            let c = comps[ci];
            if let Some(s) = stats.iter().position(|s| s.px == c.px && (s.cx - c.cx).abs() < 1e-9 && (s.cy - c.cy).abs() < 1e-9) {
                out[ci] = inside[s];
            }
        }
    }
    out
}

/// Voxels of one raster that are more than `grow` voxels from the other.
pub fn drift(a: &RunRaster, b: &RunRaster, g: &RasterGrid, grow: i64) -> Drift {
    let (ta, tb) = std::thread::scope(|sc| {
        let h = sc.spawn(|| tolerance(a, grow));
        let tb = tolerance(b, grow);
        (h.join().unwrap_or_else(|_| tolerance(a, grow)), tb)
    });
    let removed = a.difference(&tb);
    let added = b.difference(&ta);
    let beyond = removed.union(&added);
    let blobs = blobs(&beyond, &added, g);
    Drift { voxels: beyond.pixel_count, removed, added, beyond, blobs, tol_a: ta }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raster(g: RasterGrid, voxels: &[(i64, i64, i64)]) -> RunRaster {
        let mut by_layer: BTreeMap<i64, BTreeMap<i64, Vec<Run>>> = BTreeMap::new();
        for &(k, row, col) in voxels {
            by_layer.entry(k).or_default().entry(row).or_default().push(Run { c0: col, c1: col + 1 });
        }
        let layers: Vec<Layer> = by_layer.into_iter().map(|(k, rows)| Layer { k, z_mm: g.layer_z_mm(k), rows }).collect();
        let pixel_count = voxels.len() as u64;
        RunRaster { grid: g, layers, pixel_count, run_count: pixel_count }
    }

    /// One voxel per layer, stepping one column per layer: corner-connected in 3D, and no
    /// exact XY overlap between layers. The blob is one blob of all 64 voxels.
    #[test]
    fn diagonal_staircase_across_layers_is_one_blob() {
        let g = RasterGrid::saturn4(0.0);
        let vox: Vec<_> = (0..64).map(|i| (i, 10, 100 + i)).collect();
        let x = raster(g, &vox);
        let b = blobs(&x, &RunRaster::empty(g), &g);
        assert_eq!(b.len(), 1, "{b:?}");
        assert_eq!(b[0].vox, 64);
        // Row steps and mixed steps join too; a gap of two columns or two rows does not.
        let vox: Vec<_> = (0..8).map(|i| (i, 10 + i, 100 - i)).collect();
        assert_eq!(blobs(&raster(g, &vox), &RunRaster::empty(g), &g).len(), 1);
        let apart = raster(g, &[(0, 10, 100), (1, 10, 102), (2, 12, 102)]);
        assert_eq!(blobs(&apart, &RunRaster::empty(g), &g).len(), 3);
    }

    #[test]
    fn corner_join_pairs_touching_runs_only() {
        let t = |rows: &[(i64, Vec<Run>)]| RowRuns::from_row_table(rows);
        let a = t(&[(5, vec![Run { c0: 10, c1: 12 }])]);
        let b = t(&[(4, vec![Run { c0: 12, c1: 14 }, Run { c0: 15, c1: 16 }]), (7, vec![Run { c0: 10, c1: 12 }])]);
        let mut got = Vec::new();
        corner_join(&a, &b, |i, j| got.push((i, j)));
        assert_eq!(got, vec![(0, 0)], "abutting at a corner joins; a gap of one pixel and two rows do not");
    }
}
