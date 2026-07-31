//! Integer-micron chord comparison primitives.
//!
//! Canonicalization is comparison-only. The raw-chord `< 0.08` firm/moot
//! classification remains authoritative and must never use canonical microns.

use serde::{
    de::{self, Unexpected, Visitor},
    ser::SerializeStruct,
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{collections::BTreeMap, fmt, num::NonZeroU32};
use thiserror::Error;

/// A positive integer number of microns used as a chord-comparison bucket size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MicronGranularity(NonZeroU32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum MicronGranularityError {
    #[error("micron granularity must be greater than zero")]
    Zero,
    #[error("micron granularity must be positive, got {value}")]
    Negative { value: i64 },
    #[error("micron granularity {value} exceeds u32::MAX")]
    TooLarge { value: u64 },
}

impl MicronGranularity {
    /// The normative one-micron comparison resolution.
    pub const ONE_MICRON: Self = Self(NonZeroU32::MIN);

    pub fn new(microns: u32) -> Result<Self, MicronGranularityError> {
        NonZeroU32::new(microns)
            .map(Self)
            .ok_or(MicronGranularityError::Zero)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl Default for MicronGranularity {
    fn default() -> Self {
        Self::ONE_MICRON
    }
}

impl TryFrom<i64> for MicronGranularity {
    type Error = MicronGranularityError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        if value < 0 {
            return Err(MicronGranularityError::Negative { value });
        }
        if value > i64::from(u32::MAX) {
            return Err(MicronGranularityError::TooLarge {
                value: value as u64,
            });
        }
        Self::new(value as u32)
    }
}

impl Serialize for MicronGranularity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(self.get())
    }
}

impl<'de> Deserialize<'de> for MicronGranularity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct GranularityVisitor;

        impl<'de> Visitor<'de> for GranularityVisitor {
            type Value = MicronGranularity;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a positive integer micron granularity")
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                MicronGranularity::try_from(value).map_err(E::custom)
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let value = u32::try_from(value)
                    .map_err(|_| E::custom(MicronGranularityError::TooLarge { value }))?;
                MicronGranularity::new(value).map_err(E::custom)
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Err(E::invalid_type(Unexpected::Float(value), &self))
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Err(E::invalid_type(Unexpected::Bool(value), &self))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Err(E::invalid_type(Unexpected::Str(value), &self))
            }
        }

        deserializer.deserialize_any(GranularityVisitor)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ChordError {
    #[error("chord must be finite")]
    NonFiniteChord,
    #[error("scaling chord by 1000 microns per millimeter was non-finite")]
    ScaledNonFinite,
    #[error("rounded micron bucket count is outside the i64 domain")]
    RoundedBucketOutOfRange,
    #[error(
        "canonical micron value overflowed for {buckets} buckets at {micron} micron granularity"
    )]
    CanonicalMicronOverflow { buckets: i64, micron: u32 },
}

/// Canonicalize a raw crossing chord using the Python oracle's actual IEEE ties-to-even rule.
///
/// This comparison representation must not be used for raw-chord firm/moot classification.
pub fn canon_chord_micron(chord_mm: f64, micron: MicronGranularity) -> Result<i64, ChordError> {
    if !chord_mm.is_finite() {
        return Err(ChordError::NonFiniteChord);
    }

    // Preserve this operation order: it is the executable Python oracle.
    let microns_raw = chord_mm * 1000.0;
    if !microns_raw.is_finite() {
        return Err(ChordError::ScaledNonFinite);
    }
    let bucket_ratio = microns_raw / f64::from(micron.get());
    let rounded_buckets = bucket_ratio.round_ties_even();

    // `i64::MAX` rounds to 2^63 as f64, so the upper bound must be exclusive.
    let i64_upper_exclusive = -(i64::MIN as f64);
    if !(rounded_buckets >= i64::MIN as f64 && rounded_buckets < i64_upper_exclusive) {
        return Err(ChordError::RoundedBucketOutOfRange);
    }
    let buckets = rounded_buckets as i64;
    buckets
        .checked_mul(i64::from(micron.get()))
        .ok_or(ChordError::CanonicalMicronOverflow {
            buckets,
            micron: micron.get(),
        })
}

/// Compare two raw chords after canonicalizing them at the same resolution.
pub fn chords_match_micron(
    a_mm: f64,
    b_mm: f64,
    micron: MicronGranularity,
) -> Result<bool, ChordError> {
    Ok(canon_chord_micron(a_mm, micron)? == canon_chord_micron(b_mm, micron)?)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChordSource {
    Engine,
    Census,
}

impl fmt::Display for ChordSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Engine => formatter.write_str("engine"),
            Self::Census => formatter.write_str("census"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ChordParityError {
    #[error("{population} member {member_idx} has an empty chord list; omit members with no counted crossing")]
    EmptyChordList {
        population: ChordSource,
        member_idx: u32,
    },
    #[error("{population} member {member_idx} chord {chord_index} is invalid")]
    InvalidChord {
        population: ChordSource,
        member_idx: u32,
        chord_index: usize,
        #[source]
        error: ChordError,
    },
}

/// A shared member whose canonical chord multisets disagree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChordMismatch {
    member_idx: u32,
    engine_micron: Vec<i64>,
    census_micron: Vec<i64>,
}

impl ChordMismatch {
    #[must_use]
    pub const fn member_idx(&self) -> u32 {
        self.member_idx
    }

    #[must_use]
    pub fn engine_micron(&self) -> &[i64] {
        &self.engine_micron
    }

    #[must_use]
    pub fn census_micron(&self) -> &[i64] {
        &self.census_micron
    }
}

impl Serialize for ChordMismatch {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("ChordMismatch", 3)?;
        state.serialize_field("member_idx", &self.member_idx)?;
        state.serialize_field("engine_micron", &self.engine_micron)?;
        state.serialize_field("census_micron", &self.census_micron)?;
        state.end()
    }
}

/// A diagnostic parity comparison; it is not a pipeline acceptance decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChordParityReport {
    micron: MicronGranularity,
    only_engine: Vec<u32>,
    only_census: Vec<u32>,
    mismatches: Vec<ChordMismatch>,
    n_engine: usize,
    n_census: usize,
    n_shared: usize,
}

impl ChordParityReport {
    #[must_use]
    pub const fn micron(&self) -> MicronGranularity {
        self.micron
    }

    #[must_use]
    pub fn only_engine(&self) -> &[u32] {
        &self.only_engine
    }

    #[must_use]
    pub fn only_census(&self) -> &[u32] {
        &self.only_census
    }

    #[must_use]
    pub fn mismatches(&self) -> &[ChordMismatch] {
        &self.mismatches
    }

    #[must_use]
    pub const fn n_engine(&self) -> usize {
        self.n_engine
    }

    #[must_use]
    pub const fn n_census(&self) -> usize {
        self.n_census
    }

    #[must_use]
    pub const fn n_shared(&self) -> usize {
        self.n_shared
    }

    /// Diagnostic parity only; `DefectReport` remains the sole accept authority.
    #[must_use]
    pub fn matches(&self) -> bool {
        self.only_engine.is_empty() && self.only_census.is_empty() && self.mismatches.is_empty()
    }
}

impl Serialize for ChordParityReport {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("ChordParityReport", 8)?;
        state.serialize_field("matches", &self.matches())?;
        state.serialize_field("only_engine", &self.only_engine)?;
        state.serialize_field("only_census", &self.only_census)?;
        state.serialize_field("mismatches", &self.mismatches)?;
        state.serialize_field("micron", &self.micron)?;
        state.serialize_field("n_engine", &self.n_engine)?;
        state.serialize_field("n_census", &self.n_census)?;
        state.serialize_field("n_shared", &self.n_shared)?;
        state.end()
    }
}

/// Gather, canonicalize, then compare two complete member populations.
pub fn chord_parity_report(
    engine_chords_by_member: &BTreeMap<u32, Vec<f64>>,
    census_chords_by_member: &BTreeMap<u32, Vec<f64>>,
    micron: MicronGranularity,
) -> Result<ChordParityReport, ChordParityError> {
    let engine = canonicalize_population(engine_chords_by_member, micron, ChordSource::Engine)?;
    let census = canonicalize_population(census_chords_by_member, micron, ChordSource::Census)?;

    let only_engine = engine
        .keys()
        .filter(|member_idx| !census.contains_key(member_idx))
        .copied()
        .collect();
    let only_census = census
        .keys()
        .filter(|member_idx| !engine.contains_key(member_idx))
        .copied()
        .collect();
    let mismatches = engine
        .iter()
        .filter_map(|(&member_idx, engine_micron)| {
            let census_micron = census.get(&member_idx)?;
            (engine_micron != census_micron).then(|| ChordMismatch {
                member_idx,
                engine_micron: engine_micron.clone(),
                census_micron: census_micron.clone(),
            })
        })
        .collect();

    let n_engine = engine.len();
    let n_census = census.len();
    let n_shared = engine
        .keys()
        .filter(|member_idx| census.contains_key(member_idx))
        .count();
    Ok(ChordParityReport {
        micron,
        only_engine,
        only_census,
        mismatches,
        n_engine,
        n_census,
        n_shared,
    })
}

fn canonicalize_population(
    chords_by_member: &BTreeMap<u32, Vec<f64>>,
    micron: MicronGranularity,
    source: ChordSource,
) -> Result<BTreeMap<u32, Vec<i64>>, ChordParityError> {
    chords_by_member
        .iter()
        .map(|(&member_idx, chords)| {
            if chords.is_empty() {
                return Err(ChordParityError::EmptyChordList {
                    population: source,
                    member_idx,
                });
            }
            let mut canonical = chords
                .iter()
                .enumerate()
                .map(|(chord_index, &chord_mm)| {
                    canon_chord_micron(chord_mm, micron).map_err(|error| {
                        ChordParityError::InvalidChord {
                            population: source,
                            member_idx,
                            chord_index,
                            error,
                        }
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            canonical.sort_unstable();
            Ok((member_idx, canonical))
        })
        .collect()
}
