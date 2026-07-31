use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A stable FNV-1a content digest for numeric buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentDigest(pub u64);

/// A numeric buffer that contains an IEEE value not valid for a content
/// identity. Such input must be rejected rather than silently normalized.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum DigestError {
    #[error("numeric buffer contains a non-finite value at index {index}")]
    NonFinite { index: usize },
}

/// Normalizes values that compare equal but have distinct IEEE encodings.
pub fn canonical_f64_bits(value: f64) -> Result<u64, DigestError> {
    if !value.is_finite() {
        return Err(DigestError::NonFinite { index: 0 });
    }
    if value == 0.0 {
        Ok(0)
    } else {
        Ok(value.to_bits())
    }
}

pub fn digest_f64(value: f64) -> Result<ContentDigest, DigestError> {
    digest_f64_slice(&[value])
}

pub fn digest_f64_slice(values: &[f64]) -> Result<ContentDigest, DigestError> {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut state = OFFSET;
    for (index, value) in values.iter().enumerate() {
        if !value.is_finite() {
            return Err(DigestError::NonFinite { index });
        }
        for byte in canonical_f64_bits(*value)
            .expect("finite value was checked above")
            .to_le_bytes()
        {
            state ^= u64::from(byte);
            state = state.wrapping_mul(PRIME);
        }
    }
    Ok(ContentDigest(state))
}
