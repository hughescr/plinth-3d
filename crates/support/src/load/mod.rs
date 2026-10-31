//! The FR-20a load model (lane B; design `step11-construction.md` section 4).
//!
//! [`data`] is the plain-data API shared with the planner: [`LayerStack`], [`LayoutView`]
//! and [`LoadReport`]. [`analyse`] is the one entry point; it is a stub that answers an
//! unverified report until the model is built.

pub mod data;

pub use data::{
    ComponentInfo, ComponentSlice, ContactView, LayerArea, LayerLoad, LayerStack, LayoutView,
    LoadReport, LoadStatus, LoadWarning, PartChain, PartKind, PartSection, SectionLoad, Sharing,
    SiteClass, SourceElement, WarningClass,
};

/// The owner's design ratio (P5): the largest allowed peel area per mm² of section.
pub const DESIGN_RATIO: f64 = 12.0;

/// Run the load model of `stack` on `layout`: the tributary result that sizes contacts, the
/// other three sharing results, the moment-balance test and the warnings of section 4.8.
#[must_use]
pub fn analyse(stack: &LayerStack, layout: &LayoutView) -> LoadReport {
    let _ = (stack, layout);
    LoadReport::unverified(DESIGN_RATIO, "the load model is not built (step 0 stub)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stub_never_reports_a_result() {
        let r = analyse(&LayerStack::default(), &LayoutView::default());
        assert!(!r.is_computed());
        assert!(r.sections.is_empty() && r.warnings.is_empty());
        assert!((r.design_ratio - DESIGN_RATIO).abs() < f64::EPSILON);
    }
}
