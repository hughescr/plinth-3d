//! The data the FR-21 report and the review pictures are made from (lane B; design
//! `step11-construction.md` section 9).
//!
//! Plain data, assembled by the run from the written file and the load model, rendered by
//! `plinth-cli`'s report and by `plinth-render`. Every number states what it counts; the
//! fields here carry the count and the unit, the report text carries the denominator.

use crate::load::needs::{Coverage, NeedsMap, SiteTag};
use crate::load::{
    AssignedSource, Extras, LayerStack, LayoutView, LoadDetail, LoadReport, LoadWarning, SiteClass,
    Structure, analyse_full, assign_sources,
};

/// Hashes and pose: which files this report is about.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Identity {
    /// SHA-256 of the original STL.
    pub original_sha256: String,
    /// SHA-256 of the owner-approved tilted STL.
    pub approved_sha256: String,
    /// SHA-256 of the generated `.chitubox`.
    pub generated_sha256: String,
    /// The pose line (FR-12): the keep-up rotation and what it means.
    pub pose: String,
}

/// The process values the report states first (design section 4.7).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProcessValues {
    /// Layer height, mm, and where it came from.
    pub layer_height_mm: f64,
    /// Source of the layer height.
    pub layer_height_source: String,
    /// P5: the design ratio.
    pub design_ratio: f64,
    /// The H1 admission bound, mm².
    pub h1_admission_mm2: f64,
    /// The coverage reach, mm.
    pub coverage_reach_mm: f64,
    /// The overhang angle, degrees from horizontal.
    pub overhang_deg: f64,
    /// Lift of the lowest model point above the plate, mm (H4).
    pub lift_mm: f64,
}

/// Contacts counted by site class (not native record count).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClassCount {
    /// Class 1: hidden underside.
    pub hidden_underside: u32,
    /// Class 2: partly visible rim.
    pub partly_visible_rim: u32,
}

impl ClassCount {
    /// Add one contact of `class`.
    pub fn add(&mut self, class: SiteClass) {
        match class {
            SiteClass::HiddenUnderside => self.hidden_underside += 1,
            SiteClass::PartlyVisibleRim => self.partly_visible_rim += 1,
        }
    }
}

/// The supports block of the report.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SupportsSummary {
    /// Contacts by site class.
    pub contacts: ClassCount,
    /// Largest patch, mm² (the checker's upper bound).
    pub largest_patch_mm2: f64,
    /// Smallest patch spacing, mm.
    pub smallest_spacing_mm: f64,
    /// Thinnest section, mm across flats.
    pub thinnest_section_mm: f64,
    /// Smallest gap outside the contact regions, mm.
    pub smallest_gap_mm: f64,
    /// Contacts per cm² of the hidden underside.
    pub density_per_cm2: f64,
}

/// Volumes and height, mL and mm.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Volumes {
    /// Model volume, mL.
    pub model_ml: f64,
    /// Support volume, mL.
    pub supports_ml: f64,
    /// Raft volume, mL.
    pub raft_ml: f64,
    /// How overlap between the three was counted.
    pub overlap_treatment: String,
    /// Print height, mm.
    pub print_height_mm: f64,
}

/// Time of one run step, seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct StepTime {
    /// Step name.
    pub name: String,
    /// Seconds.
    pub seconds: f64,
}

/// Run data: time per step, batch counts, rejections, peak memory.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunData {
    /// Time per step.
    pub steps: Vec<StepTime>,
    /// Rejection reasons and how often each fired (name, count).
    pub rejections: Vec<(String, u64)>,
    /// Job peak memory, bytes.
    pub peak_memory_bytes: u64,
}

/// Everything the report and the pictures need.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReportData {
    /// Why the data is not a result, when it is not. `None` for a computed report.
    pub unverified: Option<String>,
    /// Files and pose.
    pub identity: Identity,
    /// Process values.
    pub process: ProcessValues,
    /// The load model's answer, if it ran.
    pub load: Option<LoadReport>,
    /// The numbers behind the load answer, the layout it was run on and the model structure.
    pub load_data: Option<LoadData>,
    /// The supports block.
    pub supports: SupportsSummary,
    /// Volumes and height.
    pub volumes: Volumes,
    /// Run data.
    pub run: RunData,
    /// All warnings, by class (a copy of the load warnings plus the placement ones).
    pub warnings: Vec<LoadWarning>,
    /// Measurements decoded from the exact emitted bytes (not planning counts).
    pub written_metrics: Option<WrittenMetrics>,
}

/// Written contact sizes and column-spacing/bracing measurements.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WrittenMetrics {
    /// Ball radius and count, exact f32 values promoted to f64.
    pub radius_histogram: Vec<(f64, u32)>,
    /// Per-column nearest-neighbour XY distances, each column counted once.
    pub nearest_column_mm: Vec<f64>,
    /// Long (>4 mm) columns; each tuple is (native record, length, braced).
    pub long_columns: Vec<(u32, f64, bool)>,
    /// Actual written brace diagonals, not proposals.
    pub braces: u32,
}

/// What the overhang samples say, in surface and plan-view area (the design's section 0 lists
/// plan-view areas; the surface area is larger).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NeedsSummary {
    /// Samples taken.
    pub samples: usize,
    /// Surface area of the faces flatter than the limit, mm².
    pub surface_mm2: f64,
    /// The same area projected on the horizontal plane, mm².
    pub projected_mm2: f64,
    /// Plan-view area below 30 degrees, mm².
    pub below_30_projected_mm2: f64,
    /// Plan-view area from 30 to the limit, mm².
    pub from_30_projected_mm2: f64,
    /// Surface area by site class: hidden, partly visible, top, undecided (mm²).
    pub class_surface_mm2: [f64; 4],
    /// Cover of the samples by the layout.
    pub coverage: Option<Coverage>,
}

impl NeedsSummary {
    /// Summarise a needs map and the coverage found with it.
    #[must_use]
    pub fn of(needs: &NeedsMap, coverage: Option<Coverage>) -> Self {
        let class = |f: fn(SiteTag) -> bool| needs.class_area_mm2(f);
        Self {
            samples: needs.points.len(),
            surface_mm2: needs.total_area_mm2(),
            projected_mm2: needs.band_projected_mm2(0.0, 180.0),
            below_30_projected_mm2: needs.band_projected_mm2(0.0, 30.0),
            from_30_projected_mm2: needs.band_projected_mm2(30.0, needs.limit_deg),
            class_surface_mm2: [
                class(|c| c == SiteTag::Hidden),
                class(|c| c == SiteTag::PartlyVisible),
                class(|c| c == SiteTag::Top),
                class(|c| matches!(c, SiteTag::Unverified(_))),
            ],
            coverage,
        }
    }
}

/// How the largest ratio changes when the assumptions change (design section 4.7).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sensitivity {
    /// Largest tributary ratio with the slab envelope (the headline convention).
    pub envelope_largest: f64,
    /// Largest tributary ratio with the mid-plane slice areas instead.
    pub midplane_largest: f64,
    /// A lower bound on the contacts that hold the peak layer at P5 = 18: the peak area over
    /// 18 times the largest limiting section in the layout. Not the planner's count (that
    /// needs the planner); `None` when there is no contact.
    pub contacts_lower_bound_p5_18: Option<u32>,
}

/// The load model's inputs and answer for one supported model.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadData {
    /// The numbers behind the report.
    pub detail: LoadDetail,
    /// The layout it was run on.
    pub layout: LayoutView,
    /// Islands, cups and reach.
    pub structure: Structure,
    /// The overhang samples, with their site class.
    pub needs: NeedsMap,
    /// Their summary.
    pub needs_summary: NeedsSummary,
    /// Sensitivity to the area convention and to P5.
    pub sensitivity: Sensitivity,
    /// The source elements of the peak layer and the contact each goes to (picture 7b).
    pub peak_sources: Vec<AssignedSource>,
}

/// The stack with every layer's areas scaled from the envelope to the mid-plane slice.
fn midplane_variant(stack: &LayerStack) -> LayerStack {
    let mut out = stack.clone();
    for l in &mut out.layers {
        let s = if l.model_envelope > 0.0 {
            l.model_midplane / l.model_envelope
        } else {
            1.0
        };
        l.model_envelope *= s;
        for c in &mut l.components {
            c.area *= s;
            c.sources.iter_mut().for_each(|e| e.area *= s);
        }
    }
    out
}

fn largest_tributary(d: &LoadDetail) -> f64 {
    d.sections.iter().map(|s| s.tributary.0).fold(0.0, f64::max)
}

impl LoadData {
    /// Run the load model on `stack` and `layout` and gather everything the report needs.
    #[must_use]
    pub fn build(
        stack: &LayerStack,
        layout: LayoutView,
        structure: Structure,
        needs: NeedsMap,
    ) -> Self {
        let extras = Extras {
            structure: Some(&structure),
            needs: Some(&needs),
        };
        let detail = analyse_full(stack, &layout, &extras);
        let mid = analyse_full(&midplane_variant(stack), &layout, &Extras::default());
        let best_section = layout
            .contacts
            .iter()
            .map(|c| c.patch_lower_mm2.min(c.neck_area_mm2))
            .fold(0.0, f64::max);
        let lower_bound = detail.peak.filter(|_| best_section > 0.0).map(|p| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                (p.area_mm2 / (18.0 * best_section)).ceil() as u32
            }
        });
        let sensitivity = Sensitivity {
            envelope_largest: largest_tributary(&detail),
            midplane_largest: largest_tributary(&mid),
            contacts_lower_bound_p5_18: lower_bound,
        };
        let needs_summary = NeedsSummary::of(&needs, detail.coverage);
        let peak_sources = detail
            .peak
            .map(|p| assign_sources(stack, &layout, p.layer))
            .unwrap_or_default();
        Self {
            peak_sources,
            detail,
            layout,
            structure,
            needs,
            needs_summary,
            sensitivity,
        }
    }
}

impl ReportData {
    /// Report data with no result and the reason.
    #[must_use]
    pub fn unverified(why: &str) -> Self {
        Self {
            unverified: Some(why.to_owned()),
            ..Self::default()
        }
    }

    /// True when the data is a result.
    #[must_use]
    pub fn is_computed(&self) -> bool {
        self.unverified.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unverified_data_is_not_a_result_and_class_counts_add() {
        let d = ReportData::unverified("not built");
        assert!(!d.is_computed());
        let mut c = ClassCount::default();
        c.add(SiteClass::PartlyVisibleRim);
        c.add(SiteClass::HiddenUnderside);
        c.add(SiteClass::HiddenUnderside);
        assert_eq!((c.hidden_underside, c.partly_visible_rim), (2, 1));
    }
}
