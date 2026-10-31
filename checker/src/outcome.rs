//! What one rule measurement returns, before it is put in the report.

use crate::json::Json;
use crate::report::Status;
use crate::status::{Located, Location};

/// The result of one rule.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub status: Status,
    pub reason: String,
    /// Places in the file that explain a violation or an unverified answer.
    pub located: Vec<Located>,
    /// The numbers behind the answer (report field `detail`).
    pub detail: Json,
}

impl Outcome {
    fn new(status: Status, reason: impl Into<String>) -> Self {
        Self {
            status,
            reason: reason.into(),
            located: Vec::new(),
            detail: Json::Null,
        }
    }

    #[must_use]
    pub fn pass(reason: impl Into<String>) -> Self {
        Self::new(Status::Pass, reason)
    }

    #[must_use]
    pub fn violation(reason: impl Into<String>) -> Self {
        Self::new(Status::Violation, reason)
    }

    #[must_use]
    pub fn unverified(reason: impl Into<String>) -> Self {
        Self::new(Status::Unverified, reason)
    }

    #[must_use]
    pub fn not_applicable(reason: impl Into<String>) -> Self {
        Self::new(Status::NotApplicable, reason)
    }

    #[must_use]
    pub fn with_detail(mut self, detail: Json) -> Self {
        self.detail = detail;
        self
    }

    #[must_use]
    pub fn with_located(mut self, located: Vec<Located>) -> Self {
        self.located = located;
        self
    }
}

/// A located note: record index and a point in the world frame.
#[must_use]
pub fn at(
    record: Option<usize>,
    xyz: Option<[f64; 3]>,
    layer: Option<u32>,
    why: impl Into<String>,
) -> Located {
    Located {
        location: Location {
            record: record.and_then(|r| u32::try_from(r).ok()),
            xyz,
            layer,
        },
        reason: why.into(),
    }
}

/// What a hard-limit verdict (H1, H2, H3, H6, H9) was measured on.
pub const MEASURED_ON: &str = "the written triangles";

/// The scope of a hard-limit verdict, stated in every report that carries one.
pub const VERDICT_SCOPE: &str = "measured on the written triangles; valid for ChituBox v1.4.0 if it slices the stored support meshes - pending the H8 differential slice test";

/// How many located entries a rule lists (the rest are counted).
pub const MAX_LOCATED: usize = 20;

/// Fold the verdicts of several checks into one: any violation wins, then any unverified,
/// then pass.
#[must_use]
pub fn worst(parts: &[Status]) -> Status {
    if parts.contains(&Status::Violation) {
        Status::Violation
    } else if parts.contains(&Status::Unverified) {
        Status::Unverified
    } else if parts.contains(&Status::Pass) {
        Status::Pass
    } else {
        Status::NotApplicable
    }
}
