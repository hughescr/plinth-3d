use crate::AcceptSafe;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerPlanError {
    NonFiniteOrigin,
    NonPositiveHeight,
}

/// Per-layer geometry from the beginning; consumers never receive one global height.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerPlan {
    z0_mm: f64,
    heights_mm: Vec<f64>,
    cumulative_mm: Vec<f64>,
}

impl LayerPlan {
    pub fn constant(height: &impl AcceptSafe<f64>, count: usize) -> Result<Self, LayerPlanError> {
        Self::new(0.0, vec![height.get(); count])
    }

    pub fn new(z0_mm: f64, heights_mm: Vec<f64>) -> Result<Self, LayerPlanError> {
        if !z0_mm.is_finite() {
            return Err(LayerPlanError::NonFiniteOrigin);
        }
        if heights_mm
            .iter()
            .any(|height| !height.is_finite() || *height <= 0.0)
        {
            return Err(LayerPlanError::NonPositiveHeight);
        }
        let mut cumulative_mm = Vec::with_capacity(heights_mm.len() + 1);
        cumulative_mm.push(z0_mm);
        for height in &heights_mm {
            let next = cumulative_mm.last().copied().unwrap_or(z0_mm) + height;
            cumulative_mm.push(next);
        }
        Ok(Self {
            z0_mm,
            heights_mm,
            cumulative_mm,
        })
    }

    #[must_use]
    pub const fn z0_mm(&self) -> f64 {
        self.z0_mm
    }

    #[must_use]
    pub fn heights_mm(&self) -> &[f64] {
        &self.heights_mm
    }

    /// The lower Z boundary of layer `index`, or `None` when it does not exist.
    #[must_use]
    pub fn z_of(&self, index: usize) -> Option<f64> {
        self.cumulative_mm.get(index).copied()
    }

    /// Returns the layer containing `z`, with the upper boundary belonging to
    /// the next layer. Values outside the plan return `None`.
    #[must_use]
    pub fn index_at(&self, z: f64) -> Option<usize> {
        if !z.is_finite() || z < self.z0_mm || z >= *self.cumulative_mm.last()? {
            return None;
        }
        match self
            .cumulative_mm
            .binary_search_by(|edge| edge.total_cmp(&z))
        {
            Ok(index) => Some(index),
            Err(next) => Some(next - 1),
        }
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.heights_mm.len()
    }
}
