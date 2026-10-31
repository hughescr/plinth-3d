//! The FR-20a load model (lane B; design `step11-construction.md` section 4).
//!
//! * [`data`] is the plain-data API shared with the planner: [`LayerStack`], [`LayoutView`]
//!   and [`LoadReport`].
//! * [`stack`] builds the [`LayerStack`] from a posed mesh (slab envelopes, rasters,
//!   print-order components, source elements) with the structure of the model ([`structure`]).
//! * [`analyse`] and [`analyse_full`] are the entry points: the tributary result that sizes
//!   contacts, the other three sharing results, the load-centroid test and the warnings of
//!   section 4.8 ([`warn`]). [`analyse_full`] also returns the numbers behind the report.
//! * [`needs`] holds the overhang and site-needs map.

pub mod analyse;
pub mod data;
pub mod lever;
pub mod needs;
pub mod stack;
pub mod structure;
pub mod warn;

pub use analyse::{
    AssignedSource, ContactLoad, Extras, GroupLoad, LoadDetail, PeakLayer, SectionDetail,
    assign_sources,
};
pub use data::{
    ComponentInfo, ComponentSlice, ContactView, LayerArea, LayerLoad, LayerStack, LayoutView,
    LoadReport, LoadStatus, LoadWarning, PartChain, PartKind, PartSection, SectionLoad, Sharing,
    SiteClass, SourceElement, WarningClass,
};
pub use stack::{ModelLayers, StackOptions, build_model_layers};
pub use structure::{CupInfo, IslandInfo, ReachInfo, Structure};

/// The owner's design ratio (P5): the largest allowed peel area per mm² of section.
pub const DESIGN_RATIO: f64 = 12.0;

/// Run the load model of `stack` on `layout`: the tributary result that sizes contacts, the
/// other three sharing results, the moment-balance test and the warnings of section 4.8.
#[must_use]
pub fn analyse(stack: &LayerStack, layout: &LayoutView) -> LoadReport {
    analyse_full(stack, layout, &Extras::default()).report
}

/// [`analyse`] with the numbers behind the report, and optional structure and needs inputs
/// whose warnings join the report's.
///
/// A stack with no layers, or a layer height that is not positive, answers an unverified
/// report: nothing was computed and no number in it is a result.
#[must_use]
pub fn analyse_full(stack: &LayerStack, layout: &LayoutView, extras: &Extras<'_>) -> LoadDetail {
    if stack.layers.is_empty() || stack.layer_height_mm.is_nan() || stack.layer_height_mm <= 0.0 {
        return LoadDetail {
            report: LoadReport::unverified(
                DESIGN_RATIO,
                "the layer stack is empty or has no layer height",
            ),
            contacts: Vec::new(),
            sections: Vec::new(),
            groups: Vec::new(),
            peak: None,
            coverage: None,
        };
    }
    let (mut detail, collected) = analyse::run(stack, layout);
    let (warnings, coverage) = warn::all(&detail, &collected, stack, layout, extras);
    detail.report.warnings = warnings;
    detail.coverage = coverage;
    detail
}

#[cfg(test)]
mod tests;
