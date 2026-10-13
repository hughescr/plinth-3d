//! **A first layer over nothing gets a tip inside it** (joint review,
//! 2026-10-13).
//!
//! # The defect
//!
//! The up-sweep's claim is that islands need no special case: an island
//! cell's potential is `+∞` because nothing below it is cured, so it falls
//! out of the reach test as demand ([`crate::up_sweep`]). That holds at the
//! working raster, not at the printer's pixel. The cover rounds every span
//! outward to whole cells, so a new piece of model that stands a fraction of
//! a cell from cured, rooted material of its own layer reads as one piece
//! with it, inherits its potential laterally, and is never demand. The slicer
//! cures the two apart, and the new piece's first layer has nothing under it.
//! Measured on Deinonychus (`joint/out-fix2/final`, slicer view, one pixel
//! of lateral allowance): 131 pixels at 7.725 mm, three to four pixels
//! (0.04-0.06 mm) from the grounded model beside them, held by no record;
//! `for-craig-joint` held them only because a footprint slide of arm 92
//! happened to cover them, and the rules no longer make that slide.
//!
//! # The rule
//!
//! A **birth** is a piece of a layer's model section, sliced as the printer
//! cures it (every solid filled at pixel centres, the registered pixel pitch
//! in X and Y), with no model pixel on the layer below within one pixel (the
//! printer cannot part two solids closer than that; the same allowance the
//! footprint rule and the acceptance slicer read). Only a support can hold
//! it. Where the reach field roots a birth laterally — any of its cells has
//! a potential at or under 1 before this layer's seats go in, which is
//! exactly the case the cover joined to rooted material — the birth gets an
//! arm seated INSIDE it ([`crate::up_sweep::seat_births`]) unless an arm of
//! this layer already stands in one of its cells. A birth the reach field
//! already leaves unrooted is demand and is seated by the placement as
//! before; nothing changes for it.
//!
//! The reach field reads the same rule wherever it is replayed
//! ([`crate::demand_replay`]): a bridged birth with no arm of its layer
//! standing in its cells is unrooted there, so the removal pass cannot take
//! the arm away on the strength of the lateral bridge that hid the birth in
//! the first place.
//!
//! No new number: the pixel pitch is the printer's, the one-pixel allowance
//! is the printer's, and the potential's limit is the reach law's own.
//!
//! # Batching
//!
//! Every layer is sliced at the pixel once, in one pass over the stack, with
//! one sort of that layer's crossings; the pieces are labelled run by run. No
//! per-cell or per-pixel query reaches the mesh.

use std::collections::BTreeMap;

use plinth_support::perception::SliceSolid;

use crate::grid::Grid;
use crate::reach::ReachState;

/// One birth: the working-raster cells its pixel centres fall in, and how
/// many of its pixels each holds (parallel, sorted by cell).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Birth {
    pub cells: Vec<usize>,
    pub pixels: Vec<u32>,
}

/// Every layer's births, found once for the whole stack.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Births {
    layers: Vec<Vec<Birth>>,
}

/// What the rule found and did over one run of the up-sweep.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BirthTally {
    /// Births found over the stack.
    pub found: usize,
    /// Of those, births the reach field rooted only laterally.
    pub bridged: usize,
    /// Bridged births an arm of their layer already stood in.
    pub held: usize,
    /// Bridged births given an arm seated inside them.
    pub seated: usize,
    /// Bridged births left without an arm inside (a WARNING).
    pub unseated: usize,
}

/// One horizontal run of cured pixels: row, first column, end column
/// (exclusive).
type Run = (i64, i64, i64);

impl Births {
    /// No births: the rule reads nothing (a run that turns it off, or a test
    /// that builds the replay by hand).
    pub(crate) fn none() -> Self {
        Self::default()
    }

    /// Slice every layer at the printer's pixel and keep its births.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "pixel indices of a model a few hundred mm across at a pitch of ~0.015 mm are \
                  ~1e5, exact in i64 after the floor or ceil"
    )]
    pub(crate) fn build(grid: &Grid, slices: &[Vec<SliceSolid>], pitch_x_mm: f64, pitch_y_mm: f64) -> Self {
        if !(pitch_x_mm.is_finite() && pitch_x_mm > 0.0 && pitch_y_mm.is_finite() && pitch_y_mm > 0.0) {
            return Self::none();
        }
        let mut layers = Vec::with_capacity(slices.len());
        let mut previous: Vec<Run> = Vec::new();
        let mut crossings: Vec<(i64, f64)> = Vec::new();
        for solids in slices {
            crossings.clear();
            for solid in solids {
                for points in std::iter::once(solid.outer().points_xy_mm())
                    .chain(solid.holes().iter().map(|hole| hole.points_xy_mm()))
                {
                    for (index, &a) in points.iter().enumerate() {
                        let b = points[(index + 1) % points.len()];
                        let (low, high) = if a[1] < b[1] { (a, b) } else { (b, a) };
                        // Pixel rows whose centre `(j + 0.5)·py` is in
                        // `[low, high)`: half-open, so a vertex shared by two
                        // edges is crossed once, and a level edge never.
                        let first = (low[1] / pitch_y_mm - 0.5).ceil() as i64;
                        let end = (high[1] / pitch_y_mm - 0.5).ceil() as i64;
                        for row in first..end {
                            #[expect(
                                clippy::cast_precision_loss,
                                reason = "a pixel row index ~1e5 is exact in f64"
                            )]
                            let y = (row as f64 + 0.5) * pitch_y_mm;
                            let t = (y - low[1]) / (high[1] - low[1]);
                            crossings.push((row, low[0] + (high[0] - low[0]) * t));
                        }
                    }
                }
            }
            crossings.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
            // Even-odd pairing per row: the slices are disjoint solids with
            // owned holes, under which even-odd is the fill
            // (`raster::rasterize_into`). A pixel is cured where its centre is
            // inside.
            let mut runs: Vec<Run> = Vec::new();
            let mut start = 0;
            while start < crossings.len() {
                let row = crossings[start].0;
                let mut end = start;
                while end < crossings.len() && crossings[end].0 == row {
                    end += 1;
                }
                for pair in crossings[start..end].chunks_exact(2) {
                    let first = (pair[0].1 / pitch_x_mm - 0.5).ceil() as i64;
                    let last = (pair[1].1 / pitch_x_mm - 0.5).ceil() as i64;
                    if last <= first {
                        continue;
                    }
                    match runs.last_mut() {
                        Some(run) if run.0 == row && first <= run.2 => run.2 = run.2.max(last),
                        _ => runs.push((row, first, last)),
                    }
                }
                start = end;
            }
            layers.push(births_of(grid, &runs, &previous, pitch_x_mm, pitch_y_mm));
            previous = runs;
        }
        Self { layers }
    }

    /// This layer's births (none past the stack).
    pub(crate) fn at(&self, layer: usize) -> &[Birth] {
        self.layers.get(layer).map_or(&[], Vec::as_slice)
    }

    /// Every birth found, over the stack.
    pub(crate) fn count(&self) -> usize {
        self.layers.iter().map(Vec::len).sum()
    }

    /// The births of this layer the reach field roots laterally: any of their
    /// cells, in the cover, at a potential at or under 1. Read after the
    /// layer's potential is computed and before any seat of the layer roots
    /// anything, which is when a rooted birth can only be a bridged one.
    pub(crate) fn bridged(&self, layer: usize, cover: &[bool], reach: &ReachState) -> Vec<usize> {
        self.at(layer)
            .iter()
            .enumerate()
            .filter(|(_, birth)| {
                birth
                    .cells
                    .iter()
                    .any(|&cell| cover[cell] && reach.is_rooted(cell))
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// Unroot every bridged birth of this layer that no root of the layer
    /// stands in (`roots`: the cells of the arms seated on this layer, in any
    /// order). Returns the cover cells so unrooted.
    pub(crate) fn unroot_unseated(
        &self,
        layer: usize,
        bridged: &[usize],
        roots: &[usize],
        cover: &[bool],
        reach: &mut ReachState,
    ) -> usize {
        let births = self.at(layer);
        let mut unrooted = 0;
        for &index in bridged {
            let Some(birth) = births.get(index) else {
                continue;
            };
            if birth.cells.iter().any(|cell| roots.contains(cell)) {
                continue;
            }
            for &cell in &birth.cells {
                if cover[cell] {
                    reach.unroot(cell);
                    unrooted += 1;
                }
            }
        }
        unrooted
    }
}

/// The births of one layer: the pieces of `runs` (4-connected, as the
/// slicer's own piece) with no run of `previous` within one pixel, each
/// mapped onto the working raster.
fn births_of(grid: &Grid, runs: &[Run], previous: &[Run], pitch_x_mm: f64, pitch_y_mm: f64) -> Vec<Birth> {
    if runs.is_empty() {
        return Vec::new();
    }
    // Union-find over the runs, joining runs of adjacent rows that share a
    // column. Runs are sorted by row, then column.
    let mut parent: Vec<usize> = (0..runs.len()).collect();
    let row_start = |row: i64, of: &[Run]| of.partition_point(|run| run.0 < row);
    for (index, &(row, first, end)) in runs.iter().enumerate() {
        let mut other = row_start(row + 1, runs);
        while other < runs.len() && runs[other].0 == row + 1 && runs[other].1 < end {
            if first < runs[other].2 {
                let (a, b) = (find(&mut parent, index), find(&mut parent, other));
                parent[a] = b;
            }
            other += 1;
        }
    }
    // A piece that touches the layer below, within one pixel, is carried by
    // it.
    let mut carried: BTreeMap<usize, bool> = BTreeMap::new();
    for (index, &(row, first, end)) in runs.iter().enumerate() {
        let root = find(&mut parent, index);
        let entry = carried.entry(root).or_insert(false);
        if *entry {
            continue;
        }
        *entry = (row - 1..=row + 1).any(|near| {
            let mut other = row_start(near, previous);
            while other < previous.len() && previous[other].0 == near && previous[other].1 - 1 < end {
                if first < previous[other].2 + 1 {
                    return true;
                }
                other += 1;
            }
            false
        });
    }
    let mut births: BTreeMap<usize, BTreeMap<usize, u32>> = BTreeMap::new();
    for (index, &(row, first, end)) in runs.iter().enumerate() {
        let root = find(&mut parent, index);
        if carried.get(&root).copied().unwrap_or(true) {
            continue;
        }
        let cells = births.entry(root).or_default();
        add_run_cells(grid, row, first, end, pitch_x_mm, pitch_y_mm, cells);
    }
    births
        .into_values()
        .filter(|cells| !cells.is_empty())
        .map(|cells| {
            let (cells, pixels) = cells.into_iter().unzip();
            Birth { cells, pixels }
        })
        .collect()
}

/// The root of a run's piece, halving the path as it goes.
fn find(parent: &mut [usize], mut index: usize) -> usize {
    while parent[index] != index {
        parent[index] = parent[parent[index]];
        index = parent[index];
    }
    index
}

/// Count one run's pixel centres into the working-raster cells they fall in.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "pixel and cell indices are ~1e5, exact in f64; a cell index is cast only once it \
              is checked to lie inside the raster"
)]
fn add_run_cells(
    grid: &Grid,
    row: i64,
    first: i64,
    end: i64,
    pitch_x_mm: f64,
    pitch_y_mm: f64,
    cells: &mut BTreeMap<usize, u32>,
) {
    let cell_row = grid.row_coord((row as f64 + 0.5) * pitch_y_mm).floor();
    if !(cell_row >= 0.0 && cell_row < grid.rows() as f64) {
        return;
    }
    let cell_row = cell_row as usize;
    let col_of = |pixel: i64| grid.col_coord((pixel as f64 + 0.5) * pitch_x_mm).floor();
    let mut pixel = first;
    while pixel < end {
        let col = col_of(pixel);
        // The pixels of this run in the same cell: those before the next
        // cell's left edge.
        let next_edge = grid.col_coord(0.0).mul_add(-grid.pitch_mm(), (col + 1.0) * grid.pitch_mm());
        let next = ((next_edge / pitch_x_mm - 0.5).ceil() as i64).clamp(pixel + 1, end);
        if col >= 0.0 && col < grid.cols() as f64 {
            let count = u32::try_from(next - pixel).unwrap_or(u32::MAX);
            *cells.entry(grid.index(cell_row, col as usize)).or_insert(0) += count;
        }
        pixel = next;
    }
}

#[cfg(test)]
mod tests {
    use plinth_support::perception::{slice_stack, SliceStackLimits};

    use super::*;

    const H: f64 = 0.05;
    const PX: f64 = 0.014;
    const PY: f64 = 0.019;

    /// Closed axis-aligned boxes, `[min, max]` each, as one mesh.
    fn boxes(spans: &[([f64; 3], [f64; 3])]) -> plinth_mesh::Mesh {
        const TRIS: [[u32; 3]; 12] = [
            [0, 2, 1],
            [1, 2, 3],
            [4, 5, 6],
            [5, 7, 6],
            [0, 1, 4],
            [1, 5, 4],
            [2, 6, 3],
            [3, 6, 7],
            [0, 4, 2],
            [2, 4, 6],
            [1, 3, 5],
            [3, 7, 5],
        ];
        let mut verts = Vec::new();
        let mut tris = Vec::new();
        for (index, (low, high)) in spans.iter().enumerate() {
            let base = u32::try_from(index * 8).unwrap();
            for corner in 0..8_u32 {
                let pick = |axis: usize, bit: u32| if corner >> bit & 1 == 1 { high[axis] } else { low[axis] };
                verts.push([pick(0, 0), pick(1, 1), pick(2, 2)]);
            }
            tris.extend(TRIS.iter().map(|tri| tri.map(|v| v + base)));
        }
        plinth_mesh::Mesh::new(verts, tris).expect("closed boxes")
    }

    fn births_of_boxes(spans: &[([f64; 3], [f64; 3])]) -> (Grid, Births) {
        let mesh = boxes(spans);
        let planes: Vec<f64> = (0..24).map(|layer| (f64::from(layer) + 0.5) * H).collect();
        let slices = slice_stack(&mesh, &planes, SliceStackLimits::production()).expect("slices");
        let grid = crate::grid::lay_out_at(-0.5, -0.5, 0.05, 80, 80);
        let births = Births::build(&grid, &slices, PX, PY);
        (grid, births)
    }

    /// A piece that appears 0.05 mm (three to four pixels) beside cured,
    /// carried material of its own layer, with nothing under it, is a birth,
    /// though one working cell would join the two; the material it stands
    /// beside, carried from below, is not. Its pixels are all counted into
    /// the cells they fall in.
    #[test]
    fn a_piece_a_few_pixels_beside_carried_material_is_a_birth_and_the_carried_one_is_not() {
        let body = ([0.0, 0.0, 0.0], [2.0, 2.0, 1.0]);
        let island = ([2.05, 0.5, 0.5], [2.35, 0.8, 1.0]);
        let (grid, births) = births_of_boxes(&[body, island]);
        // Layer 0 is the body's first layer, over the plate: a birth.
        assert_eq!(births.at(0).len(), 1);
        for layer in 1..10 {
            assert!(births.at(layer).is_empty(), "layer {layer}: the body is carried");
        }
        let at = births.at(10);
        assert_eq!(at.len(), 1, "the island's first layer");
        let birth = &at[0];
        let pixels: u32 = birth.pixels.iter().sum();
        // 0.30 mm x 0.30 mm at 0.014 x 0.019 mm: 21 or 22 columns, 15 or 16 rows.
        assert!((315..=352).contains(&pixels), "{pixels} pixels");
        for &cell in &birth.cells {
            let x = grid.center_x_mm(cell % grid.cols());
            assert!(x > 2.0, "every cell of the birth is the island's, not the body's: {x}");
        }
        for layer in 11..20 {
            assert!(births.at(layer).is_empty(), "layer {layer}: the island is carried now");
        }
    }

    /// One pixel of lateral allowance: a piece whose first layer's pixels
    /// neighbour the layer below's sideways is carried; three pixels away it
    /// is not.
    #[test]
    fn a_piece_over_the_layer_below_within_one_pixel_is_carried() {
        let step = ([0.0, 0.0, 0.0], [1.0, 1.0, 0.5]);
        // The step's last pixel column is 70 (centre 0.987 mm); this piece's
        // first is 71 (centre 1.001 mm).
        let near = ([1.0005, 0.0, 0.5], [1.3, 1.0, 1.0]);
        let (_, births) = births_of_boxes(&[step, near]);
        assert!(births.at(10).is_empty(), "the next pixel column over: within one pixel");
        let far = ([1.05, 0.0, 0.5], [1.3, 1.0, 1.0]);
        let (_, births) = births_of_boxes(&[step, far]);
        assert_eq!(births.at(10).len(), 1, "0.05 mm beside the step: over air");
    }
}
