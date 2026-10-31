//! The data the FR-21 report and the review pictures are made from (lane B; design
//! `step11-construction.md` section 9).
//!
//! Plain data, assembled by the run from the written file and the load model, rendered by
//! `plinth-cli`'s report and by `plinth-render`. Every number states what it counts; the
//! fields here carry the count and the unit, the report text carries the denominator.

use crate::load::{LoadReport, LoadWarning, SiteClass};

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
    /// The supports block.
    pub supports: SupportsSummary,
    /// Volumes and height.
    pub volumes: Volumes,
    /// Run data.
    pub run: RunData,
    /// All warnings, by class (a copy of the load warnings plus the placement ones).
    pub warnings: Vec<LoadWarning>,
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
