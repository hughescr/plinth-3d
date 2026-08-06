use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! string_id {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            #[must_use]
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self::new(value)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self::new(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

string_id!(ModelId);
// Stable logical identity for an emitted on-disk artifact. This is deliberately
// distinct from ModelId: a format verifier must report an unreadable artifact
// before it can discover any models in it, and absolute paths are not stable
// report identities.
string_id!(ArtifactId);
string_id!(OrientationId);
string_id!(PlateId);
string_id!(ConstantId);
string_id!(DecisionId);

/// A stable passive name for a repair operation.
///
/// Dispatch and behavior remain owned by `plinth-support`; keeping this
/// vocabulary in core lets termination reports name the attempted repair
/// without reversing the workspace dependency graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixerId {
    AddSeatAtMarginalArea,
    AddBirthLadderSeat,
    AddMinimumSeat,
    SplitLoadAcrossArms,
    RelocateForLateralSpread,
    ReaimReseatOrQuarantine,
    SlideOrReseatJoint,
    ReaimReduceEmbed,
    RerouteToPlateOrAnchor,
    SteepenOrBrace,
    ResizeAncestors,
    RerouteOrDropRegular,
    AddAuditDrainOrRefill,
}

/// A stable name for the pipeline stage that emitted a report.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageId {
    Load,
    Triage,
    Build,
    Orient,
    Hollow,
    Drill,
    Support,
    Plate,
    Slice,
    Emit,
    Audit,
    Report,
}
