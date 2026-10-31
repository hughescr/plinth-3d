//! Print-order connected components over the layer rasters (lane B; design
//! `step11-construction.md` section 4.2). Step 0 holds the signatures only.
//!
//! Pixels link by 4-connectivity inside a layer and to overlapping pixels of layer k-1.
//! Components merge when they first touch; the merge layer is recorded.

use crate::measure::Measure;
use crate::raster::RasterStack;

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

/// Label the components of `stack`.
#[must_use]
pub fn label_components(stack: &RasterStack) -> Measure<Labels> {
    let _ = stack;
    Measure::Unverified("label_components is not built yet")
}
