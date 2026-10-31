//! From a posed mesh to the load model's input: slab envelopes, rasters, print-order
//! components, source area elements, islands, cups and reach (design
//! `step11-construction.md` sections 4.1, 4.2 and 5.9).
//!
//! The peel areas and the source elements come from the vector polygons of the slab
//! envelopes. The raster is used only to say which component a piece of material belongs to
//! and for structure (islands, cups, reach).

use std::collections::BTreeMap;

use plinth_geom::Mesh;
use plinth_geom::label::{Labels, find_cups, find_reach, label_components};
use plinth_geom::measure::Measure;
use plinth_geom::raster::{PIXEL_PITCH_MM, RasterStack, rasterise_par};
use plinth_geom::slice::{LayerGrid, LayerSlab, Rings, slice_layers_par};

use super::data::{ComponentInfo, ComponentSlice, LayerArea, LayerStack, SourceElement};
use super::structure::{
    CupInfo, IslandInfo, P18_MAX_JOIN_LAYERS, P18_MAX_PX, REACH_LIMIT_MM, ReachInfo, Structure,
};

/// Settings of the stack builder.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StackOptions {
    /// Side of a source element cell, mm **[S]**.
    pub cell_mm: f64,
    /// P18 size limit, pixels.
    pub p18_max_px: u32,
    /// P18 join limit, layers.
    pub p18_max_join_layers: u32,
    /// Reach limit, mm.
    pub reach_limit_mm: f64,
    /// Threads for the slicing and rasterising batches.
    pub threads: usize,
}

impl Default for StackOptions {
    fn default() -> Self {
        Self {
            cell_mm: 0.25,
            p18_max_px: P18_MAX_PX,
            p18_max_join_layers: P18_MAX_JOIN_LAYERS,
            reach_limit_mm: REACH_LIMIT_MM,
            threads: 1,
        }
    }
}

/// Everything built from one mesh.
#[derive(Debug, Clone)]
pub struct ModelLayers {
    /// The layer grid.
    pub grid: LayerGrid,
    /// The slab envelope and mid-plane polygons.
    pub slabs: Vec<LayerSlab>,
    /// The layer rasters.
    pub rasters: RasterStack,
    /// The print-order components of the rasters.
    pub labels: Labels,
    /// The load model's input.
    pub stack: LayerStack,
    /// Islands, cups and reach.
    pub structure: Structure,
}

/// Slice, rasterise and label `mesh` on `grid` and build the load model's input.
#[must_use]
pub fn build_model_layers(
    mesh: &Mesh,
    grid: &LayerGrid,
    opts: &StackOptions,
) -> Measure<ModelLayers> {
    let slabs = match slice_layers_par(mesh, grid, opts.threads) {
        Measure::Known(s) => s,
        Measure::Unverified(w) => return Measure::Unverified(w),
    };
    let rasters = match rasterise_par(&slabs, PIXEL_PITCH_MM, opts.threads) {
        Measure::Known(r) => r,
        Measure::Unverified(w) => return Measure::Unverified(w),
    };
    let labels = match label_components(&rasters) {
        Measure::Known(l) => l,
        Measure::Unverified(w) => return Measure::Unverified(w),
    };
    let mut structure = Structure::default();
    let stack = build_stack(grid, &slabs, &rasters, &labels, opts, &mut structure);
    fill_islands(&mut structure, &stack, &labels, &rasters, opts);
    attach_outlines(&mut structure, &slabs);
    if let Some(c) = find_cups(&rasters).known() {
        structure.cups = c.iter().map(|p| cup_info(&rasters, p)).collect();
    }
    if let Some(r) = find_reach(&rasters, opts.reach_limit_mm).known() {
        structure.reach = r.iter().map(|p| reach_info(&rasters, p)).collect();
    }
    Measure::Known(ModelLayers {
        grid: *grid,
        slabs,
        rasters,
        labels,
        stack,
        structure,
    })
}

fn pixel_area(r: &RasterStack) -> f64 {
    r.pitch[0] * r.pitch[1]
}

fn cup_info(r: &RasterStack, p: &plinth_geom::label::CupPiece) -> CupInfo {
    CupInfo {
        layer: p.layer,
        area_mm2: p.pixels as f64 * pixel_area(r),
        centre_xy: r.centre(p.centre_px[0] - 0.5, p.centre_px[1] - 0.5),
    }
}

fn reach_info(r: &RasterStack, p: &plinth_geom::label::ReachPiece) -> ReachInfo {
    ReachInfo {
        layer: p.layer,
        reach_mm: p.reach_mm,
        farthest_xy: r.centre(f64::from(p.farthest_px[0]), f64::from(p.farthest_px[1])),
        area_mm2: p.pixels as f64 * pixel_area(r),
    }
}

/// One cell of one layer: net area and its first moments.
type Cell = [f64; 3];

/// Sutherland-Hodgman against one half-plane `coord(axis) >= bound` (or `<=`).
fn clip_half(poly: &[[f64; 2]], axis: usize, bound: f64, keep_ge: bool) -> Vec<[f64; 2]> {
    let inside = |p: &[f64; 2]| {
        if keep_ge {
            p[axis] >= bound
        } else {
            p[axis] <= bound
        }
    };
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (ia, ib) = (inside(&a), inside(&b));
        if ia {
            out.push(a);
        }
        if ia != ib {
            let t = (bound - a[axis]) / (b[axis] - a[axis]);
            let mut p = [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])];
            p[axis] = bound;
            out.push(p);
        }
    }
    out
}

fn clip_band(poly: &[[f64; 2]], axis: usize, lo: f64, hi: f64) -> Vec<[f64; 2]> {
    let a = clip_half(poly, axis, lo, true);
    if a.len() < 3 {
        return Vec::new();
    }
    clip_half(&a, axis, hi, false)
}

/// Net area and first moments of a polygon (signed: clockwise rings subtract).
fn moments(poly: &[[f64; 2]]) -> Cell {
    let mut m = [0.0; 3];
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let c = p[0] * q[1] - q[0] * p[1];
        m[0] += 0.5 * c;
        m[1] += (p[0] + q[0]) * c / 6.0;
        m[2] += (p[1] + q[1]) * c / 6.0;
    }
    m
}

fn cell_index(v: f64, cell: f64) -> i64 {
    #[allow(clippy::cast_possible_truncation)]
    {
        (v / cell).floor() as i64
    }
}

/// The net area and moments of `rings` in every cell of side `cell` they touch.
fn layer_cells(rings: &Rings, cell: f64) -> BTreeMap<(i64, i64), Cell> {
    let mut out: BTreeMap<(i64, i64), Cell> = BTreeMap::new();
    for ring in &rings.rings {
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in ring {
            for c in 0..2 {
                lo[c] = lo[c].min(p[c]);
                hi[c] = hi[c].max(p[c]);
            }
        }
        for i in cell_index(lo[0], cell)..=cell_index(hi[0], cell) {
            #[allow(clippy::cast_precision_loss)]
            let (x0, x1) = (i as f64 * cell, (i + 1) as f64 * cell);
            let col = clip_band(ring, 0, x0, x1);
            if col.len() < 3 {
                continue;
            }
            let (ylo, yhi) = col.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, p| {
                (a.0.min(p[1]), a.1.max(p[1]))
            });
            for j in cell_index(ylo, cell)..=cell_index(yhi, cell) {
                #[allow(clippy::cast_precision_loss)]
                let (y0, y1) = (j as f64 * cell, (j + 1) as f64 * cell);
                let piece = clip_band(&col, 1, y0, y1);
                if piece.len() < 3 {
                    continue;
                }
                let m = moments(&piece);
                let e = out.entry((i, j)).or_insert([0.0; 3]);
                for c in 0..3 {
                    e[c] += m[c];
                }
            }
        }
    }
    out
}

/// The component of the filled pixel nearest to `(x, y)` within a few pixels.
fn component_near(r: &RasterStack, labels: &Labels, layer: usize, x: f64, y: f64) -> Option<u32> {
    let [px, py] = r.pixel_of(x, y)?;
    let lay = r.layers.get(layer)?;
    for radius in 0_i64..=6 {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs().max(dy.abs()) != radius {
                    continue;
                }
                let (qx, qy) = (i64::from(px) + dx, i64::from(py) + dy);
                let (Ok(qx), Ok(qy)) = (u32::try_from(qx), u32::try_from(qy)) else {
                    continue;
                };
                if let Some(run) = lay.run_index(qx, qy) {
                    return labels.run_component.get(layer)?.get(run).copied();
                }
            }
        }
    }
    None
}

fn build_stack(
    grid: &LayerGrid,
    slabs: &[LayerSlab],
    rasters: &RasterStack,
    labels: &Labels,
    opts: &StackOptions,
    structure: &mut Structure,
) -> LayerStack {
    let mut layers = Vec::with_capacity(slabs.len());
    let cell = opts.cell_mm;
    for (k, slab) in slabs.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let z = 0.5 * (grid.z_bottom(k as u32) + grid.z_top(k as u32));
        let mut by_comp: BTreeMap<u32, ComponentSlice> = BTreeMap::new();
        for (&(i, j), m) in &layer_cells(&slab.envelope, cell) {
            if m[0] <= 1e-12 {
                continue;
            }
            let (cx, cy) = (m[1] / m[0], m[2] / m[0]);
            let Some(c) = component_near(rasters, labels, k, cx, cy) else {
                structure.unassigned_area_mm2 += m[0];
                let _ = (i, j);
                continue;
            };
            let e = by_comp.entry(c).or_insert_with(|| ComponentSlice {
                component: c,
                ..ComponentSlice::default()
            });
            e.area += m[0];
            e.sources.push(SourceElement {
                xyz: [cx, cy, z],
                area: m[0],
            });
        }
        layers.push(LayerArea {
            model_envelope: slab.envelope.area,
            model_midplane: slab.midplane.area,
            support: 0.0,
            raft: 0.0,
            overlap: 0.0,
            components: by_comp.into_values().collect(),
        });
    }
    let components = labels
        .components
        .iter()
        .map(|c| ComponentInfo {
            first_layer: c.first_layer,
            merged_into: c.merged_into,
            is_island: true,
            exempt: c.first_layer_width_px <= opts.p18_max_px
                && c.first_layer_height_px <= opts.p18_max_px
                && c.merged_into
                    .is_some_and(|(_, at)| at - c.first_layer <= opts.p18_max_join_layers),
        })
        .collect();
    LayerStack {
        layer_height_mm: grid.height,
        layer_origin_mm: grid.origin,
        pixel_pitch_mm: rasters.pitch,
        layers,
        components,
    }
}

fn fill_islands(
    structure: &mut Structure,
    stack: &LayerStack,
    labels: &Labels,
    rasters: &RasterStack,
    _opts: &StackOptions,
) {
    let [px, py] = rasters.pitch;
    for (id, c) in labels.components.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let id = id as u32;
        let last_alone = c.merged_into.map_or(u32::MAX, |(_, at)| at);
        let mut first_area = 0.0;
        let mut peak: f64 = 0.0;
        let mut alone = 0;
        let mut first_xy = [0.0; 2];
        for (k, layer) in stack.layers.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let k32 = k as u32;
            if k32 < c.first_layer || k32 >= last_alone {
                continue;
            }
            let Some(s) = layer.components.iter().find(|s| s.component == id) else {
                continue;
            };
            if k32 == c.first_layer {
                first_area = s.area;
                let w: f64 = s.sources.iter().map(|e| e.area).sum();
                if w > 0.0 {
                    first_xy = [
                        s.sources.iter().map(|e| e.xyz[0] * e.area).sum::<f64>() / w,
                        s.sources.iter().map(|e| e.xyz[1] * e.area).sum::<f64>() / w,
                    ];
                }
            }
            peak = peak.max(s.area);
            alone += 1;
        }
        structure.islands.push(IslandInfo {
            component: id,
            first_layer: c.first_layer,
            merged: c.merged_into,
            width_px: c.first_layer_width_px,
            height_px: c.first_layer_height_px,
            width_mm: f64::from(c.first_layer_width_px) * px,
            height_mm: f64::from(c.first_layer_height_px) * py,
            first_area_mm2: first_area,
            peak_area_mm2: peak,
            layers_alone: alone,
            first_xy,
            exempt: stack.components[id as usize].exempt,
            first_layer_rings: Vec::new(),
            ring: None,
        });
    }
}

/// True when `p` is inside the ring (even-odd).
fn ring_contains(ring: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut inside = false;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < a[0] + (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1])
        {
            inside = !inside;
        }
    }
    inside
}

/// Give every island the rings of its first layer and say which one it is.
fn attach_outlines(structure: &mut Structure, slabs: &[LayerSlab]) {
    for isl in &mut structure.islands {
        let Some(slab) = slabs.get(isl.first_layer as usize) else {
            continue;
        };
        let rings = &slab.envelope.rings;
        let area = |r: &[[f64; 2]]| {
            (0..r.len())
                .map(|i| r[i][0] * r[(i + 1) % r.len()][1] - r[(i + 1) % r.len()][0] * r[i][1])
                .sum::<f64>()
        };
        let holding = (0..rings.len())
            .find(|&i| area(&rings[i]) > 0.0 && ring_contains(&rings[i], isl.first_xy));
        let nearest = || {
            (0..rings.len())
                .filter(|&i| area(&rings[i]) > 0.0)
                .min_by(|&a, &b| {
                    let d = |i: usize| {
                        rings[i]
                            .iter()
                            .map(|p| (p[0] - isl.first_xy[0]).hypot(p[1] - isl.first_xy[1]))
                            .fold(f64::INFINITY, f64::min)
                    };
                    d(a).total_cmp(&d(b))
                })
        };
        isl.ring = holding.or_else(nearest);
        isl.first_layer_rings = rings.clone();
    }
}

#[cfg(test)]
mod tests;
