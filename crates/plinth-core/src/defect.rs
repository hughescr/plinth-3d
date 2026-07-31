use crate::{
    Finite, FixerId, MetricSet, ModelId, ProvenanceManifest, SearchEvidence, StageId, ViolationKind,
};
use serde::{de, Deserialize, Deserializer, Serialize};
use thiserror::Error;

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
    pub measure: Finite,
    pub detail: String,
}

impl Defect {
    pub fn new(
        kind: ViolationKind,
        site: SiteId,
        severity: Severity,
        measure: f64,
        detail: impl Into<String>,
    ) -> Result<Self, DefectError> {
        Ok(Self {
            kind,
            site,
            severity,
            measure: Finite::new(measure).map_err(|_| DefectError::NonFiniteMeasure)?,
            detail: detail.into(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum DefectError {
    #[error("defect measure must be finite")]
    NonFiniteMeasure,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Termination {
    Converged,
    CapExhausted {
        kind: ViolationKind,
        fixer: FixerId,
        site: SiteId,
        attempts: u32,
        before: Finite,
        after: Finite,
        note: String,
    },
    PoolExhausted {
        kind: ViolationKind,
        site: SiteId,
    },
}

impl Termination {
    pub fn cap_exhausted(
        kind: ViolationKind,
        fixer: FixerId,
        site: SiteId,
        attempts: u32,
        before: f64,
        after: f64,
        note: impl Into<String>,
    ) -> Result<Self, TerminationError> {
        let termination = Self::CapExhausted {
            kind,
            fixer,
            site,
            attempts,
            before: Finite::new(before).map_err(|_| TerminationError::NonFiniteBefore)?,
            after: Finite::new(after).map_err(|_| TerminationError::NonFiniteAfter)?,
            note: note.into(),
        };
        termination.validate()?;
        Ok(termination)
    }

    #[must_use]
    pub fn pool_exhausted(kind: ViolationKind, site: SiteId) -> Self {
        Self::PoolExhausted { kind, site }
    }

    fn validate(&self) -> Result<(), TerminationError> {
        if let Self::CapExhausted {
            kind,
            attempts,
            before,
            after,
            ..
        } = self
        {
            if *attempts == 0 {
                return Err(TerminationError::ZeroAttempts);
            }
            if *kind == ViolationKind::RepairCapExhausted {
                return Err(TerminationError::RepairCapAsOriginalKind);
            }
            if !before.get().is_finite() {
                return Err(TerminationError::NonFiniteBefore);
            }
            if !after.get().is_finite() {
                return Err(TerminationError::NonFiniteAfter);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum TerminationError {
    #[error("cap exhaustion attempts must be greater than zero")]
    ZeroAttempts,
    #[error("cap exhaustion kind names the original defect, not repair_cap_exhausted")]
    RepairCapAsOriginalKind,
    #[error("cap exhaustion before measure must be finite")]
    NonFiniteBefore,
    #[error("cap exhaustion after measure must be finite")]
    NonFiniteAfter,
}

impl<'de> Deserialize<'de> for Termination {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire {
            Converged,
            CapExhausted {
                kind: ViolationKind,
                fixer: FixerId,
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
        }

        match Wire::deserialize(deserializer)? {
            Wire::Converged => Ok(Self::Converged),
            Wire::CapExhausted {
                kind,
                fixer,
                site,
                attempts,
                before,
                after,
                note,
            } => Self::cap_exhausted(kind, fixer, site, attempts, before, after, note)
                .map_err(de::Error::custom),
            Wire::PoolExhausted { kind, site } => Ok(Self::pool_exhausted(kind, site)),
        }
    }
}

/// The one and only run-time acceptance authority.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DefectReport {
    subject: ModelId,
    defects: Vec<Defect>,
    metrics: MetricSet,
    stage: StageId,
    provenance: ProvenanceManifest,
    termination: Termination,
    search: SearchEvidence,
}

impl DefectReport {
    pub fn new(
        subject: ModelId,
        defects: Vec<Defect>,
        metrics: MetricSet,
        stage: StageId,
        provenance: ProvenanceManifest,
        termination: Termination,
        search: SearchEvidence,
    ) -> Result<Self, ReportError> {
        termination.validate()?;
        match &termination {
            Termination::Converged => {}
            Termination::CapExhausted { site, .. } => {
                let has_cap_defect = defects.iter().any(|defect| {
                    defect.kind == ViolationKind::RepairCapExhausted
                        && defect.site == *site
                        && defect.severity == Severity::Blocking
                });
                if !has_cap_defect {
                    return Err(ReportError::MissingCapExhaustedDefect);
                }
            }
            Termination::PoolExhausted { kind, site } => {
                let has_pool_defect = defects.iter().any(|defect| {
                    defect.kind == *kind
                        && defect.site == *site
                        && defect.severity == Severity::Blocking
                });
                if !has_pool_defect {
                    return Err(ReportError::MissingPoolExhaustedDefect);
                }
            }
        }

        Ok(Self {
            subject,
            defects,
            metrics,
            stage,
            provenance,
            termination,
            search,
        })
    }

    /// Acceptance is intentionally only this predicate: no score, search
    /// status, or termination state may override a blocking defect.
    #[must_use]
    pub fn accepted(&self) -> bool {
        !self
            .defects
            .iter()
            .any(|defect| defect.severity == Severity::Blocking)
    }

    #[must_use]
    pub fn subject(&self) -> &ModelId {
        &self.subject
    }

    #[must_use]
    pub fn defects(&self) -> &[Defect] {
        &self.defects
    }

    #[must_use]
    pub const fn metrics(&self) -> &MetricSet {
        &self.metrics
    }

    #[must_use]
    pub const fn stage(&self) -> StageId {
        self.stage
    }

    #[must_use]
    pub const fn provenance(&self) -> &ProvenanceManifest {
        &self.provenance
    }

    #[must_use]
    pub const fn termination(&self) -> &Termination {
        &self.termination
    }

    #[must_use]
    pub const fn search(&self) -> &SearchEvidence {
        &self.search
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ReportError {
    #[error(transparent)]
    InvalidTermination(#[from] TerminationError),
    #[error(
        "cap exhaustion requires a matching blocking repair_cap_exhausted defect at the same site"
    )]
    MissingCapExhaustedDefect,
    #[error("pool exhaustion requires a matching blocking defect at the same site")]
    MissingPoolExhaustedDefect,
}

impl<'de> Deserialize<'de> for DefectReport {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            subject: ModelId,
            defects: Vec<Defect>,
            metrics: MetricSet,
            stage: StageId,
            provenance: ProvenanceManifest,
            termination: Termination,
            search: SearchEvidence,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.subject,
            wire.defects,
            wire.metrics,
            wire.stage,
            wire.provenance,
            wire.termination,
            wire.search,
        )
        .map_err(de::Error::custom)
    }
}
