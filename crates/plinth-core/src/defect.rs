use crate::{MetricSet, ModelId, SearchEvidence, StageId, ViolationKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Blocking,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum SiteId {
    Member(u32),
    Quarantine(u32),
    Ball(u32),
    Contact(u32),
    Face(u32),
    Cavity(u32),
    Model(ModelId),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Defect {
    pub kind: ViolationKind,
    pub site: SiteId,
    pub severity: Severity,
    pub measure: f64,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Termination {
    Converged,
    CapExhausted {
        kind: ViolationKind,
        site: SiteId,
        attempts: u32,
        before: f64,
        after: f64,
        note: String,
    },
    PoolExhausted {
        kind: ViolationKind,
        site: SiteId,
    },
    Aborted {
        reason: String,
    },
}

/// The one and only run-time acceptance authority.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DefectReport {
    pub subject: ModelId,
    pub defects: Vec<Defect>,
    pub metrics: MetricSet,
    pub stage: StageId,
    pub termination: Termination,
    pub search: SearchEvidence,
}

impl DefectReport {
    #[must_use]
    pub fn new(
        subject: ModelId,
        defects: Vec<Defect>,
        metrics: MetricSet,
        stage: StageId,
        termination: Termination,
        search: SearchEvidence,
    ) -> Self {
        Self {
            subject,
            defects,
            metrics,
            stage,
            termination,
            search,
        }
    }

    /// Acceptance is intentionally only this predicate: no score or status may
    /// override a blocking defect.
    #[must_use]
    pub fn accepted(&self) -> bool {
        !self
            .defects
            .iter()
            .any(|defect| defect.severity == Severity::Blocking)
    }
}
