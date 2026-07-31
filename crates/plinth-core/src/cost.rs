use crate::{Finite, OrientationId};
use serde::{de, Deserialize, Deserializer, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

/// The fidelity level of a cost estimate; a cost always states how much it lied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fidelity {
    Surrogate,
    ProxyRefined,
    Exact,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum CostError {
    #[error("{field} must be finite")]
    NonFinite { field: &'static str },
    #[error("{field} must be nonnegative")]
    Negative { field: &'static str },
}

fn nonnegative(value: f64, field: &'static str) -> Result<Finite, CostError> {
    let value = Finite::new(value).map_err(|_| CostError::NonFinite { field })?;
    if value.get() < 0.0 {
        return Err(CostError::Negative { field });
    }
    Ok(value)
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Resin {
    model_ml: Finite,
    support_ml: Finite,
    trapped_ml: Finite,
}

impl Resin {
    pub fn new(model_ml: f64, support_ml: f64, trapped_ml: f64) -> Result<Self, CostError> {
        Ok(Self {
            model_ml: nonnegative(model_ml, "model_ml")?,
            support_ml: nonnegative(support_ml, "support_ml")?,
            trapped_ml: nonnegative(trapped_ml, "trapped_ml")?,
        })
    }

    #[must_use]
    pub const fn model_ml(self) -> f64 {
        self.model_ml.get()
    }

    #[must_use]
    pub const fn support_ml(self) -> f64 {
        self.support_ml.get()
    }

    #[must_use]
    pub const fn trapped_ml(self) -> f64 {
        self.trapped_ml.get()
    }
}

impl<'de> Deserialize<'de> for Resin {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            model_ml: f64,
            support_ml: f64,
            trapped_ml: f64,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.model_ml, wire.support_ml, wire.trapped_ml).map_err(de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Scar {
    area_mm2: Finite,
    delicacy_weighted: Finite,
    worst_patch_mm2: Finite,
}

impl Scar {
    pub fn new(
        area_mm2: f64,
        delicacy_weighted: f64,
        worst_patch_mm2: f64,
    ) -> Result<Self, CostError> {
        Ok(Self {
            area_mm2: nonnegative(area_mm2, "area_mm2")?,
            delicacy_weighted: nonnegative(delicacy_weighted, "delicacy_weighted")?,
            worst_patch_mm2: nonnegative(worst_patch_mm2, "worst_patch_mm2")?,
        })
    }

    #[must_use]
    pub const fn area_mm2(self) -> f64 {
        self.area_mm2.get()
    }

    #[must_use]
    pub const fn delicacy_weighted(self) -> f64 {
        self.delicacy_weighted.get()
    }

    #[must_use]
    pub const fn worst_patch_mm2(self) -> f64 {
        self.worst_patch_mm2.get()
    }
}

impl<'de> Deserialize<'de> for Scar {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            area_mm2: f64,
            delicacy_weighted: f64,
            worst_patch_mm2: f64,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.area_mm2, wire.delicacy_weighted, wire.worst_patch_mm2)
            .map_err(de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Risk {
    max_effective_x: Finite,
    zipper_arms: u32,
    min_lateral_spread_mm: Finite,
}

impl Risk {
    pub fn new(
        max_effective_x: f64,
        zipper_arms: u32,
        min_lateral_spread_mm: f64,
    ) -> Result<Self, CostError> {
        Ok(Self {
            max_effective_x: nonnegative(max_effective_x, "max_effective_x")?,
            zipper_arms,
            min_lateral_spread_mm: nonnegative(min_lateral_spread_mm, "min_lateral_spread_mm")?,
        })
    }

    #[must_use]
    pub const fn max_effective_x(self) -> f64 {
        self.max_effective_x.get()
    }

    #[must_use]
    pub const fn zipper_arms(self) -> u32 {
        self.zipper_arms
    }

    #[must_use]
    pub const fn min_lateral_spread_mm(self) -> f64 {
        self.min_lateral_spread_mm.get()
    }
}

impl<'de> Deserialize<'de> for Risk {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            max_effective_x: f64,
            zipper_arms: u32,
            min_lateral_spread_mm: f64,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.max_effective_x,
            wire.zipper_arms,
            wire.min_lateral_spread_mm,
        )
        .map_err(de::Error::custom)
    }
}

/// A complete cost record. Its fields are private so a record cannot be
/// partially mutated after construction.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct CostVector {
    z_height_mm: Finite,
    print_time_s: Finite,
    resin: Resin,
    scar: Scar,
    risk: Risk,
    fidelity: Fidelity,
}

impl CostVector {
    pub fn new(
        z_height_mm: f64,
        print_time_s: f64,
        resin: Resin,
        scar: Scar,
        risk: Risk,
        fidelity: Fidelity,
    ) -> Result<Self, CostError> {
        Ok(Self {
            z_height_mm: nonnegative(z_height_mm, "z_height_mm")?,
            print_time_s: nonnegative(print_time_s, "print_time_s")?,
            resin,
            scar,
            risk,
            fidelity,
        })
    }

    #[must_use]
    pub const fn z_height_mm(self) -> f64 {
        self.z_height_mm.get()
    }

    #[must_use]
    pub const fn print_time_s(self) -> f64 {
        self.print_time_s.get()
    }

    #[must_use]
    pub const fn resin(self) -> Resin {
        self.resin
    }

    #[must_use]
    pub const fn scar(self) -> Scar {
        self.scar
    }

    #[must_use]
    pub const fn risk(self) -> Risk {
        self.risk
    }

    #[must_use]
    pub const fn fidelity(self) -> Fidelity {
        self.fidelity
    }
}

impl<'de> Deserialize<'de> for CostVector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            z_height_mm: f64,
            print_time_s: f64,
            resin: Resin,
            scar: Scar,
            risk: Risk,
            fidelity: Fidelity,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.z_height_mm,
            wire.print_time_s,
            wire.resin,
            wire.scar,
            wire.risk,
            wire.fidelity,
        )
        .map_err(de::Error::custom)
    }
}

/// A partial evaluation whose omitted terms are explicit in the type.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct PartialCost {
    fidelity: Fidelity,
    z_height_mm: Option<Finite>,
    print_time_s: Option<Finite>,
    resin: Option<Resin>,
    scar: Option<Scar>,
    risk: Option<Risk>,
}

impl PartialCost {
    pub fn new(
        fidelity: Fidelity,
        z_height_mm: Option<f64>,
        print_time_s: Option<f64>,
        resin: Option<Resin>,
        scar: Option<Scar>,
        risk: Option<Risk>,
    ) -> Result<Self, CostError> {
        Ok(Self {
            fidelity,
            z_height_mm: z_height_mm
                .map(|value| nonnegative(value, "z_height_mm"))
                .transpose()?,
            print_time_s: print_time_s
                .map(|value| nonnegative(value, "print_time_s"))
                .transpose()?,
            resin,
            scar,
            risk,
        })
    }

    #[must_use]
    pub const fn fidelity(self) -> Fidelity {
        self.fidelity
    }
}

impl<'de> Deserialize<'de> for PartialCost {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            fidelity: Fidelity,
            z_height_mm: Option<f64>,
            print_time_s: Option<f64>,
            resin: Option<Resin>,
            scar: Option<Scar>,
            risk: Option<Risk>,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.fidelity,
            wire.z_height_mm,
            wire.print_time_s,
            wire.resin,
            wire.scar,
            wire.risk,
        )
        .map_err(de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrientationCost {
    orientation: OrientationId,
    cost: CostVector,
}

impl OrientationCost {
    #[must_use]
    pub fn new(orientation: OrientationId, cost: CostVector) -> Self {
        Self { orientation, cost }
    }

    #[must_use]
    pub fn orientation(&self) -> &OrientationId {
        &self.orientation
    }

    #[must_use]
    pub const fn cost(&self) -> CostVector {
        self.cost
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParetoPoint {
    orientation: OrientationId,
    cost: CostVector,
}

impl ParetoPoint {
    #[must_use]
    pub fn new(orientation: OrientationId, cost: CostVector) -> Self {
        Self { orientation, cost }
    }

    #[must_use]
    pub fn orientation(&self) -> &OrientationId {
        &self.orientation
    }

    #[must_use]
    pub const fn cost(&self) -> CostVector {
        self.cost
    }
}

/// Search must explicitly state when it has not happened; no cost sentinel is allowed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", content = "summary", rename_all = "snake_case")]
pub enum SearchEvidence {
    NotEvaluated,
    Evaluated(OptimizationSummary),
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum OptimizationError {
    #[error("an evaluated search must examine at least one candidate")]
    NoCandidatesExamined,
    #[error("an evaluated search must contain at least one Pareto point")]
    EmptyPareto,
    #[error("Pareto points exceed candidates examined")]
    ParetoExceedsCandidates,
    #[error("orientation {orientation} appears more than once in the Pareto set")]
    DuplicateOrientation { orientation: OrientationId },
    #[error("the selected orientation and cost must appear exactly in the Pareto set")]
    SelectedNotInPareto,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OptimizationSummary {
    selected: OrientationCost,
    pareto: Vec<ParetoPoint>,
    candidates_examined: u64,
}

impl OptimizationSummary {
    pub fn new(
        selected: OrientationCost,
        candidates_examined: u64,
        pareto: Vec<ParetoPoint>,
    ) -> Result<Self, OptimizationError> {
        if candidates_examined == 0 {
            return Err(OptimizationError::NoCandidatesExamined);
        }
        if pareto.is_empty() {
            return Err(OptimizationError::EmptyPareto);
        }
        if candidates_examined < pareto.len() as u64 {
            return Err(OptimizationError::ParetoExceedsCandidates);
        }

        let mut orientations = BTreeSet::new();
        for point in &pareto {
            if !orientations.insert(point.orientation().clone()) {
                return Err(OptimizationError::DuplicateOrientation {
                    orientation: point.orientation().clone(),
                });
            }
        }

        let selected_present = pareto.iter().any(|point| {
            point.orientation() == selected.orientation() && point.cost() == selected.cost()
        });
        if !selected_present {
            return Err(OptimizationError::SelectedNotInPareto);
        }

        Ok(Self {
            selected,
            pareto,
            candidates_examined,
        })
    }

    #[must_use]
    pub fn selected(&self) -> &OrientationCost {
        &self.selected
    }

    #[must_use]
    pub fn pareto(&self) -> &[ParetoPoint] {
        &self.pareto
    }

    #[must_use]
    pub const fn candidates_examined(&self) -> u64 {
        self.candidates_examined
    }
}

impl<'de> Deserialize<'de> for OptimizationSummary {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            selected: OrientationCost,
            pareto: Vec<ParetoPoint>,
            candidates_examined: u64,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.selected, wire.candidates_examined, wire.pareto).map_err(de::Error::custom)
    }
}
