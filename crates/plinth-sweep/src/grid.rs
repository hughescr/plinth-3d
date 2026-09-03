//! The working raster: a square, axis-aligned cell grid over the model's
//! bounding box, and the budget loop that coarsens it instead of refusing.

/// One square working raster.
///
/// Cell `(row, col)` covers `[x0 + col·p, x0 + (col+1)·p) × [y0 + row·p, y0 + (row+1)·p)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    pitch_mm: f64,
    origin_x_mm: f64,
    origin_y_mm: f64,
    cols: usize,
    rows: usize,
}

impl Grid {
    #[must_use]
    pub const fn pitch_mm(&self) -> f64 {
        self.pitch_mm
    }

    #[must_use]
    pub const fn cols(&self) -> usize {
        self.cols
    }

    #[must_use]
    pub const fn rows(&self) -> usize {
        self.rows
    }

    #[must_use]
    pub const fn cells(&self) -> usize {
        self.rows * self.cols
    }

    /// Area of one cell. This is the quantum the load raster is denominated in:
    /// with a uniform peel field, one cell of cured material carries exactly
    /// this many mm² of reference peel.
    #[must_use]
    pub fn cell_area_mm2(&self) -> f64 {
        self.pitch_mm * self.pitch_mm
    }

    #[must_use]
    pub const fn index(&self, row: usize, col: usize) -> usize {
        row * self.cols + col
    }

    #[must_use]
    pub const fn row_of(&self, cell: usize) -> usize {
        cell / self.cols
    }

    #[must_use]
    pub const fn col_of(&self, cell: usize) -> usize {
        cell % self.cols
    }

    /// Centre of the cell in column `col`.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "a raster column index, bounded by the cell-layer budget at ~1e5, is exact in f64"
    )]
    pub fn center_x_mm(&self, col: usize) -> f64 {
        self.origin_x_mm + (col as f64 + 0.5) * self.pitch_mm
    }

    /// Centre of the cell in row `row`.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "a raster row index, bounded by the cell-layer budget at ~1e5, is exact in f64"
    )]
    pub fn center_y_mm(&self, row: usize) -> f64 {
        self.origin_y_mm + (row as f64 + 0.5) * self.pitch_mm
    }

    /// Fractional column coordinate of an x in mm; the caller rounds according
    /// to whether it wants any-overlap or full-containment.
    #[must_use]
    pub fn col_coord(&self, x_mm: f64) -> f64 {
        (x_mm - self.origin_x_mm) / self.pitch_mm
    }

    /// Fractional row coordinate of a y in mm.
    #[must_use]
    pub fn row_coord(&self, y_mm: f64) -> f64 {
        (y_mm - self.origin_y_mm) / self.pitch_mm
    }
}

/// What the budget loop did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridPlan {
    /// How many times the pitch doubled to fit the budget. Reported, never a
    /// refusal: a flat cap that failed the stage is root cause RC3, and it
    /// converted a resolution question into a lost artifact.
    pub coarsening_steps: u32,
}

/// Build the working raster for a model bounding box, doubling the pitch until
/// the whole stack fits the cell-layer budget.
///
/// `margin_mm` is added on every side so a maximum-radius strut seated at an
/// edge cell still lies inside the grid.
///
/// # Panics
///
/// Never. The loop terminates because doubling the pitch quarters the cell
/// count and the layer count is fixed; a one-cell grid is below any positive
/// budget.
#[must_use]
#[expect(
    clippy::too_many_arguments,
    reason = "a raster layout is four bbox bounds, a layer count, a pitch, a margin and a budget; \
          wrapping eight scalars in a struct at one call site moves the list rather than shortens it"
)]
pub fn plan_grid(
    min_x_mm: f64,
    min_y_mm: f64,
    max_x_mm: f64,
    max_y_mm: f64,
    layers: usize,
    requested_pitch_mm: f64,
    margin_mm: f64,
    cell_layer_budget: u64,
) -> (Grid, GridPlan) {
    let mut pitch = requested_pitch_mm;
    let mut steps = 0_u32;
    loop {
        let grid = lay_out(min_x_mm, min_y_mm, max_x_mm, max_y_mm, pitch, margin_mm);
        let cell_layers = (grid.cells() as u64).saturating_mul(layers as u64);
        if cell_layers <= cell_layer_budget || grid.cells() <= 1 {
            return (grid, GridPlan {
                coarsening_steps: steps,
            });
        }
        pitch *= 2.0;
        steps += 1;
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the span/pitch quotient is finite, non-negative and bounded by the caller's own bbox \
          and pitch; it is `ceil`ed and then raised to at least 1, so neither truncation nor sign \
          loss can reach the result"
)]
fn lay_out(
    min_x_mm: f64,
    min_y_mm: f64,
    max_x_mm: f64,
    max_y_mm: f64,
    pitch_mm: f64,
    margin_mm: f64,
) -> Grid {
    let origin_x_mm = min_x_mm - margin_mm - pitch_mm;
    let origin_y_mm = min_y_mm - margin_mm - pitch_mm;
    let span_x = (max_x_mm + margin_mm + pitch_mm) - origin_x_mm;
    let span_y = (max_y_mm + margin_mm + pitch_mm) - origin_y_mm;
    let cols = ((span_x / pitch_mm).ceil().max(1.0) as usize).max(1);
    let rows = ((span_y / pitch_mm).ceil().max(1.0) as usize).max(1);
    Grid {
        pitch_mm,
        origin_x_mm,
        origin_y_mm,
        cols,
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cell_contains_its_own_centre_and_its_neighbours_do_not() {
        let (grid, plan) = plan_grid(-25.0, -25.0, 25.0, 25.0, 170, 0.05, 1.0, 2_000_000_000);
        assert_eq!(plan.coarsening_steps, 0);
        for col in [0_usize, 7, grid.cols() - 1] {
            let coord = grid.col_coord(grid.center_x_mm(col));
            assert!(coord > col as f64 && coord < (col + 1) as f64);
        }
    }

    #[test]
    fn an_impossible_budget_coarsens_and_still_produces_a_grid() {
        // The RC3 lesson as a test: a budget far below the model's need must
        // COARSEN, never refuse.
        let (grid, plan) = plan_grid(-50.0, -50.0, 50.0, 50.0, 170, 0.05, 1.0, 10_000);
        assert!(plan.coarsening_steps > 0);
        assert!(grid.cells() >= 1);
        assert!(grid.pitch_mm() > 0.05);
    }
}
