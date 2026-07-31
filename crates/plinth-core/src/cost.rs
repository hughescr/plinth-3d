use crate::OrientationId;
use serde::{Deserialize, Serialize};

/// The fidelity level of a cost estimate; a cost always states how much it lied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fidelity {
    Surrogate,
    ProxyRefined,
    Exact,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Resin {
    model_ml: f64,
    support_ml: f64,
    trapped_ml: f64,
}

impl Resin {
    #[must_use]
    pub const fn new(model_ml: f64, support_ml: f64, trapped_ml: f64) -> Self {
        Self {
            model_ml,
            support_ml,
            trapped_ml,
        }
    }
    #[must_use]
    pub const fn model_ml(self) -> f64 {
        self.model_ml
    }
    #[must_use]
    pub const fn support_ml(self) -> f64 {
        self.support_ml
    }
    #[must_use]
    pub const fn trapped_ml(self) -> f64 {
        self.trapped_ml
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scar {
    area_mm2: f64,
    delicacy_weighted: f64,
    worst_patch_mm2: f64,
}

impl Scar {
    #[must_use]
    pub const fn new(area_mm2: f64, delicacy_weighted: f64, worst_patch_mm2: f64) -> Self {
        Self {
            area_mm2,
            delicacy_weighted,
            worst_patch_mm2,
        }
    }
    #[must_use]
    pub const fn area_mm2(self) -> f64 {
        self.area_mm2
    }
    #[must_use]
    pub const fn delicacy_weighted(self) -> f64 {
        self.delicacy_weighted
    }
    #[must_use]
    pub const fn worst_patch_mm2(self) -> f64 {
        self.worst_patch_mm2
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Risk {
    max_effective_x: f64,
    zipper_arms: u32,
    min_lateral_spread_mm: f64,
}

impl Risk {
    #[must_use]
    pub const fn new(max_effective_x: f64, zipper_arms: u32, min_lateral_spread_mm: f64) -> Self {
        Self {
            max_effective_x,
            zipper_arms,
            min_lateral_spread_mm,
        }
    }
    #[must_use]
    pub const fn max_effective_x(self) -> f64 {
        self.max_effective_x
    }
    #[must_use]
    pub const fn zipper_arms(self) -> u32 {
        self.zipper_arms
    }
    #[must_use]
    pub const fn min_lateral_spread_mm(self) -> f64 {
        self.min_lateral_spread_mm
    }
}

/// A complete cost record. Its fields are private so a record cannot be
/// partially mutated after construction.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CostVector {
    z_height_mm: f64,
    print_time_s: f64,
    resin: Resin,
    scar: Scar,
    risk: Risk,
    fidelity: Fidelity,
}

impl CostVector {
    #[must_use]
    pub const fn new(
        z_height_mm: f64,
        print_time_s: f64,
        resin: Resin,
        scar: Scar,
        risk: Risk,
        fidelity: Fidelity,
    ) -> Self {
        Self {
            z_height_mm,
            print_time_s,
            resin,
            scar,
            risk,
            fidelity,
        }
    }
    #[must_use]
    pub const fn z_height_mm(self) -> f64 {
        self.z_height_mm
    }
    #[must_use]
    pub const fn print_time_s(self) -> f64 {
        self.print_time_s
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

/// A partial evaluation whose omitted terms are explicit in the type.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PartialCost {
    fidelity: Fidelity,
    z_height_mm: Option<f64>,
    print_time_s: Option<f64>,
    resin: Option<Resin>,
    scar: Option<Scar>,
    risk: Option<Risk>,
}

impl PartialCost {
    #[must_use]
    pub const fn new(
        fidelity: Fidelity,
        z_height_mm: Option<f64>,
        print_time_s: Option<f64>,
        resin: Option<Resin>,
        scar: Option<Scar>,
        risk: Option<Risk>,
    ) -> Self {
        Self {
            fidelity,
            z_height_mm,
            print_time_s,
            resin,
            scar,
            risk,
        }
    }
    #[must_use]
    pub const fn fidelity(self) -> Fidelity {
        self.fidelity
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptimizationSummary {
    selected: OrientationCost,
    pareto: Vec<ParetoPoint>,
    candidates_examined: u64,
}

impl OptimizationSummary {
    #[must_use]
    pub fn new(
        selected: OrientationCost,
        candidates_examined: u64,
        pareto: Vec<ParetoPoint>,
    ) -> Self {
        Self {
            selected,
            pareto,
            candidates_examined,
        }
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
