//! Scanline rasterisation of one normalized layer into the two masks the
//! sweep reads.
//!
//! **This is where root cause RC3 dies.** The legacy coverage stage refined
//! each TRIANGLE by its longest edge, so a 98 mm sliver cost 65,536 cells and
//! a flat disc's underside cost 4.8 M cells against a 2 M cap and roughly 47 k
//! physically needed. Nothing here looks at a triangle. The input is the
//! layer's normalized loops and the cost is
//! `O(edges · rows_touched + spans)` — proportional to boundary length and
//! filled area, and to no edge-length function whatever. A disc's 102-triangle
//! fan and its 800-triangle uniform mesh produce identical runs, which is
//! sabotage S6's assertion.
//!
//! Two masks come out of one pass, with opposite and deliberate error
//! directions:
//!
//! * `cover` — **any-overlap**, rounded outward. This is the DEMAND mask.
//!   Over-stating demand is the safe direction: the worst it costs is a strut
//!   nobody needed.
//! * `carry` — **full-containment**, rounded inward and then eroded by one
//!   cell in Y. This is the CARRIER mask: what counts as cured material for
//!   reach and for load. Under-stating carriers is the safe direction.
//!
//! `carry ⊆ cover`, and their difference is the one-cell boundary ring.

#![expect(
    clippy::float_cmp,
    reason = "the horizontal-edge test is an EXACT equality on purpose: an edge whose two endpoints \
          carry the same y crosses no row centre, and skipping exactly those is what makes the \
          half-open [y_lo, y_hi) rule count every crossing once. A tolerance here would drop \
          near-horizontal edges and open the fill"
)]

use plinth_support::perception::SliceSolid;

use crate::grid::Grid;

/// The two masks for one layer.
#[derive(Clone, Debug, Default)]
pub struct LayerMasks {
    pub cover: Vec<bool>,
    pub carry: Vec<bool>,
}

impl LayerMasks {
    #[must_use]
    pub fn covered_cells(&self) -> usize {
        self.cover.iter().filter(|filled| **filled).count()
    }

    #[must_use]
    pub fn carrier_cells(&self) -> usize {
        self.carry.iter().filter(|filled| **filled).count()
    }
}

/// Reusable per-call scratch, so a K-layer sweep allocates once rather than
/// once per layer.
#[derive(Clone, Debug, Default)]
pub struct RasterScratch {
    crossings: Vec<Vec<f64>>,
    carry_raw: Vec<bool>,
}

impl RasterScratch {
    #[must_use]
    pub fn new(grid: &Grid) -> Self {
        Self {
            crossings: vec![Vec::new(); grid.rows()],
            carry_raw: vec![false; grid.cells()],
        }
    }
}

/// Rasterise one layer's solids into `masks`, reusing its buffers.
///
/// Returns the number of edges walked, which is the sabotage suite's
/// path-hit witness for "the rasteriser ran on this layer".
pub fn rasterize_into(
    grid: &Grid,
    solids: &[SliceSolid],
    scratch: &mut RasterScratch,
    masks: &mut LayerMasks,
) -> usize {
    masks.cover.clear();
    masks.cover.resize(grid.cells(), false);
    masks.carry.clear();
    masks.carry.resize(grid.cells(), false);
    scratch.carry_raw.clear();
    scratch.carry_raw.resize(grid.cells(), false);
    for row in &mut scratch.crossings {
        row.clear();
    }
    scratch.crossings.resize_with(grid.rows(), Vec::new);

    let mut edges = 0_usize;
    for solid in solids {
        edges += push_loop(grid, solid.outer().points_xy_mm(), &mut scratch.crossings);
        for hole in solid.holes() {
            edges += push_loop(grid, hole.points_xy_mm(), &mut scratch.crossings);
        }
    }

    for row in 0..grid.rows() {
        let crossings = &mut scratch.crossings[row];
        if crossings.is_empty() {
            continue;
        }
        crossings.sort_by(f64::total_cmp);
        // Even-odd pairing. The normalizer guarantees disjoint solids with
        // counter-clockwise outers and clockwise holes under which even-odd
        // and non-zero agree (`plinth-layers/src/lib.rs`:9-15), so consecutive
        // pairs are exactly the filled intervals and no winding count is
        // needed here.
        for pair in crossings.chunks_exact(2) {
            fill_spans(grid, row, pair[0], pair[1], masks, &mut scratch.carry_raw);
        }
    }

    erode_carry_in_y(grid, &scratch.carry_raw, &mut masks.carry);
    edges
}

fn push_loop(grid: &Grid, points: &[[f64; 2]], crossings: &mut [Vec<f64>]) -> usize {
    if points.len() < 2 {
        return 0;
    }
    let mut edges = 0_usize;
    for index in 0..points.len() {
        let a = points[index];
        let b = points[(index + 1) % points.len()];
        edges += 1;
        push_edge(grid, a, b, crossings);
    }
    edges
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "row indices are clamped into [0, rows) before every cast, and a row index bounded by \
          the cell-layer budget is exact in f64"
)]
fn push_edge(grid: &Grid, a: [f64; 2], b: [f64; 2], crossings: &mut [Vec<f64>]) {
    if !(a[0].is_finite() && a[1].is_finite() && b[0].is_finite() && b[1].is_finite()) {
        return;
    }
    // A horizontal edge crosses no row centre; skipping it is what makes the
    // half-open [y_lo, y_hi) rule count every crossing exactly once.
    if a[1] == b[1] {
        return;
    }
    let (lo, hi) = if a[1] < b[1] { (a, b) } else { (b, a) };
    // Rows whose CENTRE lies in [lo.y, hi.y).
    let first = (grid.row_coord(lo[1]) - 0.5).ceil();
    let last = (grid.row_coord(hi[1]) - 0.5).ceil() - 1.0;
    if !(first.is_finite() && last.is_finite()) || last < first {
        return;
    }
    let rows = crossings.len() as f64;
    let first = first.max(0.0);
    let last = last.min(rows - 1.0);
    if last < first {
        return;
    }
    let inverse_slope = (hi[0] - lo[0]) / (hi[1] - lo[1]);
    let mut row = first as usize;
    let last = last as usize;
    while row <= last {
        let y = grid.center_y_mm(row);
        crossings[row].push(lo[0] + (y - lo[1]) * inverse_slope);
        row += 1;
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "both span bounds are clamped into [0, cols) as f64 before the cast; a column index \
          bounded by the cell-layer budget is exact in f64"
)]
fn fill_spans(
    grid: &Grid,
    row: usize,
    x_lo: f64,
    x_hi: f64,
    masks: &mut LayerMasks,
    carry_raw: &mut [bool],
) {
    if !(x_lo.is_finite() && x_hi.is_finite()) || x_hi <= x_lo {
        return;
    }
    let cols = grid.cols() as f64;
    let base = row * grid.cols();

    // Any-overlap: cell `i` overlaps [x_lo, x_hi) iff i < coord(x_hi) and
    // i + 1 > coord(x_lo).
    let cover_lo = grid.col_coord(x_lo).floor().max(0.0);
    let cover_hi = (grid.col_coord(x_hi).ceil() - 1.0).min(cols - 1.0);
    if cover_hi >= cover_lo {
        let (lo, hi) = (cover_lo as usize, cover_hi as usize);
        masks.cover[base + lo..=base + hi].fill(true);
    }

    // Full containment: cell `i` is wholly inside iff i >= coord(x_lo) and
    // i + 1 <= coord(x_hi).
    let carry_lo = grid.col_coord(x_lo).ceil().max(0.0);
    let carry_hi = (grid.col_coord(x_hi).floor() - 1.0).min(cols - 1.0);
    if carry_hi >= carry_lo {
        let (lo, hi) = (carry_lo as usize, carry_hi as usize);
        carry_raw[base + lo..=base + hi].fill(true);
    }
}

/// Erode the raw carrier mask by one cell in Y.
///
/// The span rule above gives full containment in X only, because the scanline
/// samples one y per row. Requiring both row neighbours restores containment
/// in Y at the same one-cell resolution, and it does so in the safe direction:
/// a carrier is dropped, never invented. The cost is that a wall thinner than
/// three cells (0.15 mm at the default pitch) carries nothing — which is the
/// correct reading of a feature the printer itself resolves in three pixels.
fn erode_carry_in_y(grid: &Grid, raw: &[bool], carry: &mut [bool]) {
    let cols = grid.cols();
    for row in 0..grid.rows() {
        if row == 0 || row + 1 == grid.rows() {
            continue;
        }
        let base = row * cols;
        let above = base - cols;
        let below = base + cols;
        for col in 0..cols {
            carry[base + col] =
                raw[base + col] && raw[above + col] && raw[below + col];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_carrier_mask_is_a_subset_of_the_demand_mask() {
        // Exercised end to end through the oracles; this is the cheap
        // structural invariant that holds for any input at all.
        let masks = LayerMasks {
            cover: vec![true, true, false],
            carry: vec![true, false, false],
        };
        assert!(masks
            .carry
            .iter()
            .zip(&masks.cover)
            .all(|(carry, cover)| !*carry || *cover));
        assert_eq!(masks.covered_cells(), 2);
        assert_eq!(masks.carrier_cells(), 1);
    }
}
