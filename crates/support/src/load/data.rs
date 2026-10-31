//! The load model's data API (design `step11-construction.md` sections 4 and 10).
//!
//! Plain data, not sealed types. Lane B fills a [`LayerStack`] from the slicing pass and
//! builds a [`LoadReport`]; lane A fills a [`LayoutView`] from a plan (private proposal or
//! frozen). Nothing here enforces a rule: the planner reads the report and decides.
//!
//! Units: mm, mm², layer indices from 0 at the layer grid origin (plate z = 0).

/// The model sliced in print order (the input of the load model).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayerStack {
    /// Layer height, mm. Recorded with the candidate's hash (ChituBox applies it at slicing).
    pub layer_height_mm: f64,
    /// z of the bottom of layer 0, mm.
    pub layer_origin_mm: f64,
    /// Printer pixel pitch in x and y, mm.
    pub pixel_pitch_mm: [f64; 2],
    /// One entry per layer, in layer order.
    pub layers: Vec<LayerArea>,
    /// Every print-order model component, by id.
    pub components: Vec<ComponentInfo>,
}

/// The areas of one layer, mm². Model, support and raft are separate, with an overlap term so
/// the parts add up to the union.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayerArea {
    /// Model slab envelope (the peel area convention).
    pub model_envelope: f64,
    /// Model mid-plane slice, reported beside the envelope.
    pub model_midplane: f64,
    /// Support material (columns, braces, heads), counted once.
    pub support: f64,
    /// Raft material.
    pub raft: f64,
    /// Area counted in more than one of the three above.
    pub overlap: f64,
    /// The model components present in this layer.
    pub components: Vec<ComponentSlice>,
}

/// One component's share of a layer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComponentSlice {
    /// Component id (index into [`LayerStack::components`]).
    pub component: u32,
    /// Slab envelope area of the component in this layer, mm².
    pub area: f64,
    /// Source area elements: where this layer's material is, for the tributary rule.
    pub sources: Vec<SourceElement>,
}

/// A small piece of peel area at a position.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SourceElement {
    /// Centre, mm (x, y, and z of the layer).
    pub xyz: [f64; 3],
    /// Area, mm².
    pub area: f64,
}

/// A print-order model component.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ComponentInfo {
    /// First layer in which it has material.
    pub first_layer: u32,
    /// The component it merges into and the layer where they first touch.
    pub merged_into: Option<(u32, u32)>,
    /// A non-exempt island: it first appears with no rooted material below it.
    pub is_island: bool,
    /// An island that the P18 exemption lets stay unsupported (small and joins quickly).
    pub exempt: bool,
}

/// Which site class a contact sits on (design section 5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteClass {
    /// Class 1: hidden underside.
    HiddenUnderside,
    /// Class 2: partly visible rim.
    PartlyVisibleRim,
}

/// What kind of section a part of a chain is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartKind {
    /// The contact patch on the model.
    Patch,
    /// The tip cone's neck where it leaves the ball.
    Neck,
    /// The tip cone.
    TipCone,
    /// The joint throat.
    JointThroat,
    /// A column.
    Column,
    /// A base flare.
    Flare,
    /// A brace.
    Brace,
}

/// One section on a chain from a contact down to the plate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartSection {
    /// What the section is.
    pub kind: PartKind,
    /// Native record index in the written file, if the part has one.
    pub record: Option<u32>,
    /// Lower bound of the load-bearing section area, mm².
    pub area_lower_mm2: f64,
    /// Lowest and highest z of the part, mm: the layers by which it is printed.
    pub z_range_mm: [f64; 2],
}

/// The parts that carry one contact's load to the plate, tip first.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PartChain {
    /// Index of the contact this chain belongs to (into [`LayoutView::contacts`]).
    pub contact: u32,
    /// The sections, from the patch down.
    pub parts: Vec<PartSection>,
}

/// One contact, as the load model needs it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactView {
    /// Contact centre (the ball centre), mm.
    pub centre: [f64; 3],
    /// Lower bound of the patch area, valid for every still-possible representation, mm².
    pub patch_lower_mm2: f64,
    /// Area of the neck section, mm².
    pub neck_area_mm2: f64,
    /// The layer at which the contact's ball first overlaps the model.
    pub first_layer: u32,
    /// The site class.
    pub site_class: SiteClass,
}

/// A support layout, as the load model sees it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayoutView {
    /// Every contact.
    pub contacts: Vec<ContactView>,
    /// One chain per contact (same order), plus any brace chains after them.
    pub chains: Vec<PartChain>,
    /// Certified lower patch area per exposure, in contact order. Empty entries
    /// are permitted only for controlled synthetic inputs (disc approximation).
    pub patch_history: Vec<Vec<(u32, f64)>>,
}

/// How a result is classified (design section 4.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningClass {
    /// Unprintable as is: only changed geometry clears it.
    Unprintable,
    /// A located intervention: the owner adds a support or accepts the stated risk.
    Intervention,
    /// Information only.
    Information,
}

/// A located warning.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadWarning {
    /// Class.
    pub class: WarningClass,
    /// A short stable name, for example `load not held at design ratio`.
    pub kind: String,
    /// Layers affected.
    pub layers: Vec<u32>,
    /// Contacts affected (indices into [`LayoutView::contacts`]).
    pub contacts: Vec<u32>,
    /// What is unknown or what to change, in plain sentences.
    pub message: String,
}

/// The four sharing results for one section, as ratios of load to section area.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Sharing {
    /// Nearest-3D tributary sensitivity diagnostic; never a sizing constraint.
    pub tributary: f64,
    /// The additive group area ratio `X_group`.
    pub additive: f64,
    /// The equal-share result.
    pub equal_share: f64,
    /// The rigid-lever index.
    pub rigid_lever: f64,
}

/// The worst case of one section.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionLoad {
    /// Chain (into [`LayoutView::chains`]).
    pub chain: u32,
    /// Part within the chain.
    pub part: u32,
    /// The layer that maximises the tributary ratio.
    pub worst_layer: u32,
    /// The sharing results at that layer.
    pub ratios: Sharing,
}

/// One layer's totals.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LayerLoad {
    /// Peel area, mm².
    pub area_mm2: f64,
    /// The design ratio times the active section capacity, mm².
    pub capacity_mm2: f64,
}

/// Whether the report was computed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadStatus {
    /// Not computed, and why. No ratio in the report is a result.
    Unverified(String),
    /// Computed.
    Computed,
}

/// The load model's answer.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadReport {
    /// Whether the numbers below are results.
    pub status: LoadStatus,
    /// The design ratio the sections were judged against (P5: 12).
    pub design_ratio: f64,
    /// One entry per layer.
    pub layers: Vec<LayerLoad>,
    /// One entry per loaded section, worst layer each.
    pub sections: Vec<SectionLoad>,
    /// Warnings, by class.
    pub warnings: Vec<LoadWarning>,
}

impl LoadReport {
    /// A report with no result, for a model that is not built or an input it cannot judge.
    #[must_use]
    pub fn unverified(design_ratio: f64, why: &str) -> Self {
        Self {
            status: LoadStatus::Unverified(why.to_owned()),
            design_ratio,
            layers: Vec::new(),
            sections: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// True when the report holds results.
    #[must_use]
    pub fn is_computed(&self) -> bool {
        self.status == LoadStatus::Computed
    }
}
