use crate::{Finite, FiniteError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// A stable, machine-readable metric key.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MetricId(String);

impl MetricId {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for MetricId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for MetricId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// The numeric value of one reported metric.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MetricValue(Finite);

impl MetricValue {
    pub fn new(value: f64) -> Result<Self, FiniteError> {
        Ok(Self(Finite::new(value)?))
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0.get()
    }
}

/// An ordered metric map. Ordering is part of its serialized determinism.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MetricSet(BTreeMap<MetricId, MetricValue>);

impl MetricSet {
    #[must_use]
    pub fn new(entries: impl IntoIterator<Item = (MetricId, MetricValue)>) -> Self {
        Self(entries.into_iter().collect())
    }

    #[must_use]
    pub fn get(&self, id: &MetricId) -> Option<MetricValue> {
        self.0.get(id).copied()
    }

    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&MetricId, &MetricValue)> {
        self.0.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
