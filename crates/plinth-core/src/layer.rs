use crate::{AcceptSafe, Finite};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum LayerPlanError {
    #[error("layer-plan origin must be finite")]
    NonFiniteOrigin,
    #[error("layer height at index {index} must be finite")]
    NonFiniteHeight { index: usize },
    #[error("layer height at index {index} must be greater than zero")]
    NonPositiveHeight { index: usize },
    #[error("cumulative layer boundary overflowed at index {index}")]
    CumulativeOverflow { index: usize },
    #[error("layer height at index {index} does not advance the cumulative boundary")]
    NonIncreasingBoundary { index: usize },
}

/// Per-layer geometry from the beginning; consumers never receive one global height.
#[derive(Clone, Debug, PartialEq)]
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
        Finite::new(z0_mm).map_err(|_| LayerPlanError::NonFiniteOrigin)?;
        let heights_mm = heights_mm
            .into_iter()
            .enumerate()
            .map(|(index, height)| {
                let height =
                    Finite::new(height).map_err(|_| LayerPlanError::NonFiniteHeight { index })?;
                if height.get() <= 0.0 {
                    return Err(LayerPlanError::NonPositiveHeight { index });
                }
                Ok(height.get())
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut cumulative_mm = Vec::with_capacity(heights_mm.len() + 1);
        cumulative_mm.push(z0_mm);
        for (index, height) in heights_mm.iter().enumerate() {
            let previous = *cumulative_mm
                .last()
                .expect("the origin is always the first cumulative boundary");
            let next = previous + height;
            Finite::new(next).map_err(|_| LayerPlanError::CumulativeOverflow { index })?;
            if next <= previous {
                return Err(LayerPlanError::NonIncreasingBoundary { index });
            }
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
        (index < self.count()).then(|| self.cumulative_mm[index])
    }

    /// Returns the layer containing `z`, with the upper boundary belonging to
    /// the next layer. Values outside the plan return `None`.
    #[must_use]
    pub fn index_at(&self, z: f64) -> Option<usize> {
        if !z.is_finite() || z < self.z0_mm || z >= *self.cumulative_mm.last()? {
            return None;
        }
        match self.cumulative_mm.binary_search_by(|edge| {
            edge.partial_cmp(&z)
                .expect("layer boundaries and query were checked finite")
        }) {
            Ok(index) => Some(index),
            Err(next) => Some(next - 1),
        }
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.heights_mm.len()
    }
}

impl Serialize for LayerPlan {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct Wire<'a> {
            z0_mm: f64,
            heights_mm: &'a [f64],
        }

        Wire {
            z0_mm: self.z0_mm,
            heights_mm: &self.heights_mm,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for LayerPlan {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            z0_mm: f64,
            heights_mm: Vec<f64>,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.z0_mm, wire.heights_mm).map_err(de::Error::custom)
    }
}
