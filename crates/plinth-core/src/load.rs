use crate::Finite;
use serde::{de, Deserialize, Deserializer, Serialize};
use thiserror::Error;

/// A validated effective-arm-count cap for additive load sharing.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct EffectiveArmCount(Finite);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum LoadSharingError {
    #[error("n_eff_cap must be finite")]
    NonFinite,
    #[error("n_eff_cap must be at least 1.0")]
    BelowOne,
}

impl EffectiveArmCount {
    pub fn new(value: f64) -> Result<Self, LoadSharingError> {
        let value = Finite::new(value).map_err(|_| LoadSharingError::NonFinite)?;
        if value.get() < 1.0 {
            return Err(LoadSharingError::BelowOne);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0.get()
    }
}

impl<'de> Deserialize<'de> for EffectiveArmCount {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(f64::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

/// Passive load-sharing vocabulary. Group mechanics belong to `plinth-support`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LoadSharing {
    Zipper,
    Additive { n_eff_cap: EffectiveArmCount },
}

impl LoadSharing {
    pub fn additive(n_eff_cap: f64) -> Result<Self, LoadSharingError> {
        Ok(Self::Additive {
            n_eff_cap: EffectiveArmCount::new(n_eff_cap)?,
        })
    }

    #[must_use]
    pub const fn n_eff_cap(self) -> Option<f64> {
        match self {
            Self::Zipper => None,
            Self::Additive { n_eff_cap } => Some(n_eff_cap.get()),
        }
    }
}

impl<'de> Deserialize<'de> for LoadSharing {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum ZipperKind {
            Zipper,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum AdditiveKind {
            Additive,
        }

        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ZipperWire {
            kind: ZipperKind,
        }

        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct AdditiveWire {
            kind: AdditiveKind,
            n_eff_cap: f64,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Zipper(ZipperWire),
            Additive(AdditiveWire),
        }

        match Wire::deserialize(deserializer)? {
            Wire::Zipper(ZipperWire {
                kind: ZipperKind::Zipper,
            }) => Ok(Self::Zipper),
            Wire::Additive(AdditiveWire {
                kind: AdditiveKind::Additive,
                n_eff_cap,
            }) => Self::additive(n_eff_cap).map_err(de::Error::custom),
        }
    }
}
