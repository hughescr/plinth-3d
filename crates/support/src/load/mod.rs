//! The FR-20a load model (lane B; design `step11-construction.md` section 4).
//!
//! * [`data`] is the plain-data API shared with the planner: [`LayerStack`], [`LayoutView`]
//!   and [`LoadReport`].
//! * [`stack`] builds the [`LayerStack`] from a posed mesh (slab envelopes, rasters,
//!   print-order components, source elements) with the structure of the model ([`structure`]).
//! * [`analyse`] and [`analyse_full`] size with rigid-carrier springs and the additive
//!   requirement. Nearest-3D tributary is diagnostic only. Equilibrium, conditioning,
//!   local-transfer unknowns and section 4.8 warnings are explicit ([`warn`]).
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
/// 20 by the owner's ruling of 2026-11-11 after CAL-2 (every coupon at ratio 45 or below
/// printed); the unprintable limit stays 40.
pub const DESIGN_RATIO: f64 = 20.0;

/// Run layer-specific rigid-carrier/additive sizing, all four sharing diagnostics,
/// separate equilibrium and conditioning assessments, and section 4.8 warnings.
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
mod sharing_tests;
#[cfg(test)]
mod tests;
