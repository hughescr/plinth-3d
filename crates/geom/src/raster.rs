//! Run-length rasters of the layer slabs on the printer pixel grid (lane B; design
//! `step11-construction.md` section 4.2). Step 0 holds the signatures only.
//!
//! The raster gives connectivity, island sizes and cups. Areas always come from the vector
//! polygons, never from pixel counts.

use crate::measure::Measure;
use crate::slice::LayerSlab;

/// The printer pixel pitch in x and y, mm (ChituBox convention, P1).
pub const PIXEL_PITCH_MM: [f64; 2] = [0.014, 0.019];

/// One run of filled pixels in one row: columns `x0..x1` of row `y`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    /// Row.
    pub y: u32,
    /// First filled column.
    pub x0: u32,
    /// One past the last filled column.
    pub x1: u32,
}

/// One layer's filled pixels, runs sorted by row then column.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LayerRaster {
    /// Layer index.
    pub layer: u32,
    /// The runs.
    pub runs: Vec<Run>,
}

/// All layers on one pixel grid.
#[derive(Debug, Clone, PartialEq)]
pub struct RasterStack {
    /// Pixel pitch in x and y, mm.
    pub pitch: [f64; 2],
    /// World xy of pixel (0, 0)'s lower corner, mm.
    pub origin: [f64; 2],
    /// Grid width and height in pixels.
    pub size: [u32; 2],
    /// One raster per layer, in layer order.
    pub layers: Vec<LayerRaster>,
}

/// Rasterise every slab envelope on one grid with the given pixel pitch.
#[must_use]
pub fn rasterise(slabs: &[LayerSlab], pitch: [f64; 2]) -> Measure<RasterStack> {
    let _ = (slabs, pitch);
    Measure::Unverified("rasterise is not built yet")
}
