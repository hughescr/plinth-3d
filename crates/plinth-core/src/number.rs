use serde::{de, Deserialize, Deserializer, Serialize};
use thiserror::Error;

/// A finite IEEE-754 scalar.
///
/// The wrapper is transparent on the wire: JSON contains an ordinary number.
/// Signed zero is intentionally preserved.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize)]
#[repr(transparent)]
#[serde(transparent)]
pub struct Finite(f64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum FiniteError {
    #[error("value must be finite")]
    NonFinite,
}

impl Finite {
    pub fn new(value: f64) -> Result<Self, FiniteError> {
        if value.is_finite() {
            Ok(Self(value))
        } else {
            Err(FiniteError::NonFinite)
        }
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Finite {
    type Error = FiniteError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Finite> for f64 {
    fn from(value: Finite) -> Self {
        value.get()
    }
}

impl<'de> Deserialize<'de> for Finite {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(f64::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}
