//! Verdicts, check rows and the exit-code order.

use crate::json::Json;

/// One check's outcome. `Indeterminate` blocks promotion and is never a pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pass,
    Fail,
    Indeterminate,
    /// Not run in this class (for example Tier 2 under a pose change), with the reason in the row.
    Skipped,
    /// A measurement reported for the reader. It decides nothing.
    Info,
}

impl Status {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Indeterminate => "indeterminate",
            Self::Skipped => "skipped",
            Self::Info => "info",
        }
    }
}

/// The verdict, from least to most severe in the order the exit code uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verdict {
    Identical,
    Equivalent,
    PlanChanged,
    MeshChanged,
    PoseChanged,
    Indeterminate,
    Regression,
}

impl Verdict {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Identical => "IDENTICAL",
            Self::Equivalent => "EQUIVALENT",
            Self::PlanChanged => "PLAN-CHANGED",
            Self::MeshChanged => "MESH-CHANGED",
            Self::PoseChanged => "POSE-CHANGED",
            Self::Indeterminate => "INDETERMINATE",
            Self::Regression => "REGRESSION",
        }
    }
    /// 1 REGRESSION, 3 INDETERMINATE, 2 POSE-CHANGED, 4 MESH-CHANGED, 10 PLAN-CHANGED, 0 otherwise.
    #[must_use]
    pub const fn exit_code(self) -> i32 {
        match self {
            Self::Regression => 1,
            Self::Indeterminate => 3,
            Self::PoseChanged => 2,
            Self::MeshChanged => 4,
            Self::PlanChanged => 10,
            Self::Identical | Self::Equivalent => 0,
        }
    }
}

/// One row of the verdict table.
#[derive(Clone, Debug)]
pub struct Row {
    /// `T0.parse`, `H1`, `H4.i2`, `N3`, ...
    pub id: String,
    pub name: &'static str,
    pub status: Status,
    /// What was measured, in words and numbers.
    pub value: String,
    /// The limit it was held to.
    pub threshold: String,
    /// Locations and items behind the status (at most a screenful).
    pub detail: Vec<String>,
}

impl Row {
    #[must_use]
    pub fn new(id: &str, name: &'static str, status: Status, value: String, threshold: &str) -> Self {
        Self { id: id.to_string(), name, status, value, threshold: threshold.to_string(), detail: Vec::new() }
    }
    #[must_use]
    pub fn with_detail(mut self, d: Vec<String>) -> Self {
        self.detail = d;
        self
    }
    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::obj();
        o.set("id", Json::str(self.id.clone()));
        o.set("name", Json::str(self.name));
        o.set("status", Json::str(self.status.as_str()));
        o.set("value", Json::str(self.value.clone()));
        o.set("threshold", Json::str(self.threshold.clone()));
        o.set("detail", Json::Arr(self.detail.iter().cloned().map(Json::str).collect()));
        o
    }
}

/// Which classes the pair belongs to. A run can carry several.
#[derive(Clone, Copy, Debug, Default)]
pub struct Classes {
    pub identical: bool,
    pub equivalent: bool,
    pub plan_changed: bool,
    pub mesh_changed: bool,
    pub pose_changed: bool,
    pub regression: bool,
    pub indeterminate: bool,
}

impl Classes {
    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::obj();
        o.set("identical", Json::Bool(self.identical));
        o.set("equivalent", Json::Bool(self.equivalent));
        o.set("plan_changed", Json::Bool(self.plan_changed));
        o.set("mesh_changed", Json::Bool(self.mesh_changed));
        o.set("pose_changed", Json::Bool(self.pose_changed));
        o.set("regression", Json::Bool(self.regression));
        o.set("indeterminate", Json::Bool(self.indeterminate));
        o
    }
}
