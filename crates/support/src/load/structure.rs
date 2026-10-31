//! What the layer stack says about the model before any support exists: the print-order
//! islands, cups and unsupported reach (design `step11-construction.md` sections 4.2, 4.6 and
//! 5.3). Plain data, filled by [`super::stack::build_model_layers`].

use super::data::{LoadWarning, WarningClass};

/// The P18 island size limit in pixels, in x and in y (owner ruling 2026-10-30) **[O]**.
pub const P18_MAX_PX: u32 = 25;
/// The P18 join limit in layers **[S]**.
pub const P18_MAX_JOIN_LAYERS: u32 = 3;
/// The reach beyond rooted material that needs no support, mm (P8, no-loss value) **[M]**.
pub const REACH_LIMIT_MM: f64 = 2.13;
/// A cup piece at or above this area, mm², is unprintable as is; smaller ones are an
/// intervention **[S]**.
pub const CUP_UNPRINTABLE_MM2: f64 = 0.5;

/// A model component that first appears with nothing below it (every print-order component
/// is one), with the facts the P18 rule and FR-29 need.
#[derive(Debug, Clone, PartialEq)]
pub struct IslandInfo {
    /// Component id (index into the stack's components).
    pub component: u32,
    /// First layer.
    pub first_layer: u32,
    /// The component it merges into and the layer where they first touch.
    pub merged: Option<(u32, u32)>,
    /// Largest x extent while it was its own component, pixels.
    pub width_px: u32,
    /// Largest y extent while it was its own component, pixels.
    pub height_px: u32,
    /// Largest x extent, mm.
    pub width_mm: f64,
    /// Largest y extent, mm.
    pub height_mm: f64,
    /// Envelope area in its first layer, mm².
    pub first_area_mm2: f64,
    /// Largest envelope area while it was its own component, mm².
    pub peak_area_mm2: f64,
    /// Layers in which it was its own component.
    pub layers_alone: u32,
    /// Where it first appears (x, y), mm.
    pub first_xy: [f64; 2],
    /// P18: small enough and joins soon enough to stay unsupported.
    pub exempt: bool,
    /// The rings of the slab envelope in its first layer (outer rings counter-clockwise,
    /// holes clockwise), for the island picture. Empty until [`super::stack`] fills it.
    pub first_layer_rings: Vec<Vec<[f64; 2]>>,
    /// Index into `first_layer_rings` of the ring the island is: the outer ring that holds
    /// [`IslandInfo::first_xy`], or the nearest one.
    pub ring: Option<usize>,
}

/// A closed pocket of uncured resin in one layer (FR-17a). The area is a pixel count times
/// the pixel area: a structure estimate, not a peel area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CupInfo {
    /// Layer.
    pub layer: u32,
    /// Pocket area, mm² (pixel count based).
    pub area_mm2: f64,
    /// Centre (x, y), mm.
    pub centre_xy: [f64; 2],
}

/// New material whose reach beyond supported pixels exceeds the P8 limit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReachInfo {
    /// Layer.
    pub layer: u32,
    /// Reach, mm (infinite when nothing supported is near).
    pub reach_mm: f64,
    /// The farthest new pixel (x, y), mm.
    pub farthest_xy: [f64; 2],
    /// Size of the piece, mm² (pixel count based).
    pub area_mm2: f64,
}

/// The model's structure before supports.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Structure {
    /// Every print-order component, as an island record.
    pub islands: Vec<IslandInfo>,
    /// Closed pockets, by layer.
    pub cups: Vec<CupInfo>,
    /// Material that reaches too far from support.
    pub reach: Vec<ReachInfo>,
    /// Envelope area that could not be given to any component, summed over layers, mm². A
    /// diagnostic: it is not counted in any component.
    pub unassigned_area_mm2: f64,
}

impl Structure {
    /// The islands that need an anchor: every one that is not P18-exempt.
    #[must_use]
    pub fn needs_anchor(&self) -> Vec<&IslandInfo> {
        self.islands.iter().filter(|i| !i.exempt).collect()
    }

    /// The warnings the structure alone decides (design section 4.8): cups, and the
    /// P18 islands left unsupported (information, FR-29). A non-exempt island with no anchor is
    /// decided by the layout, in [`super::analyse_full`].
    #[must_use]
    pub fn warnings(&self) -> Vec<LoadWarning> {
        let mut out = Vec::new();
        for c in &self.cups {
            let class = if c.area_mm2 >= CUP_UNPRINTABLE_MM2 {
                WarningClass::Unprintable
            } else {
                WarningClass::Intervention
            };
            out.push(LoadWarning {
                class,
                kind: "print-order cup".to_owned(),
                layers: vec![c.layer],
                contacts: Vec::new(),
                message: format!(
                    "A closed pocket of about {:.2} mm² at ({:.2}, {:.2}) mm in layer {} has no \
                     path for resin to the outside, so it can hold suction. No suction factor \
                     is invented and no hole is made; the pocket needs an opening or an \
                     owner decision.",
                    c.area_mm2, c.centre_xy[0], c.centre_xy[1], c.layer
                ),
            });
        }
        for i in self.islands.iter().filter(|i| i.exempt) {
            out.push(LoadWarning {
                class: WarningClass::Information,
                kind: "small island left unsupported (P18)".to_owned(),
                layers: vec![i.first_layer],
                contacts: Vec::new(),
                message: format!(
                    "Island {} first appears in layer {} at ({:.2}, {:.2}) mm with {:.3} mm², \
                     at most {:.3} x {:.3} mm ({} x {} pixels), and joins other material in \
                     layer {}. It stays unsupported and carries no load.",
                    i.component,
                    i.first_layer,
                    i.first_xy[0],
                    i.first_xy[1],
                    i.peak_area_mm2,
                    i.width_mm,
                    i.height_mm,
                    i.width_px,
                    i.height_px,
                    i.merged.map_or(0, |m| m.1)
                ),
            });
        }
        out
    }
}
