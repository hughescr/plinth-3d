//! One slicing pass over every layer (lane B; design `step11-construction.md` sections 4.1
//! and 5.9). Step 0 holds the signatures only.
//!
//! Layer k covers `[origin + k h, origin + (k + 1) h)` with the plate at z = 0. The result
//! per layer is the exact vector polygon set of the slab envelope (bottom slice, top slice
//! and the projection of every triangle clipped to the slab) and, beside it, the mid-plane
//! slice. Areas are f64 sums over the polygons.

use crate::measure::Measure;
use crate::mesh::Mesh;

/// The layer grid: where layer 0 starts, the layer height and the layer count.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerGrid {
    /// Layer height, mm (0.05 for the owner's profile).
    pub height: f64,
    /// z of the bottom of layer 0, mm.
    pub origin: f64,
    /// Number of layers.
    pub layers: u32,
}

/// A set of closed rings in the XY plane. Outer rings run counter-clockwise, holes clockwise.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rings {
    /// The rings.
    pub rings: Vec<Vec<[f64; 2]>>,
    /// Signed area of the whole set, mm² (holes subtract).
    pub area: f64,
}

/// What one layer holds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayerSlab {
    /// Layer index, 0 at the grid origin.
    pub layer: u32,
    /// The slab envelope: the peel area convention.
    pub envelope: Rings,
    /// The mid-plane slice, reported beside it.
    pub midplane: Rings,
}

/// Slice `mesh` into every layer of `grid` in one pass, in layer order.
#[must_use]
pub fn slice_layers(mesh: &Mesh, grid: &LayerGrid) -> Measure<Vec<LayerSlab>> {
    let _ = (mesh, grid);
    Measure::Unverified("slice_layers is not built yet")
}
