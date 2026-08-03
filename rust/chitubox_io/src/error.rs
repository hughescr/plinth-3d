//! The single error type for the crate. No panics on malformed input.

use thiserror::Error;

/// Every fallible operation in the crate returns `Result<_, ChituboxError>`.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ChituboxError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("not a .chitubox file: found magic {found_magic:#010x}")]
    NotChitubox { found_magic: u32 },

    #[error("truncated input in {context}: need {need} bytes, have {have}")]
    Truncated {
        need: usize,
        have: usize,
        context: &'static str,
    },

    #[error("header invariant violated: {detail}")]
    HeaderInvariant { detail: String },

    #[error("model index {index} out of range (count {count})")]
    ModelIndexOutOfRange { index: usize, count: usize },

    #[error("bad triangle buffer: length {len} (must be nonzero, divisible by 9 floats)")]
    BadTriangleBuffer { len: usize },

    #[error("non-finite value in {context}")]
    NonFiniteValue { context: &'static str },

    #[error("STL parse error: {detail}")]
    StlParse { detail: String },

    #[error("ASCII STL is not supported (binary STL only)")]
    AsciiStlUnsupported,

    #[error("support section malformed: {detail}")]
    SupportSectionMalformed { detail: String },

    #[error("support-index pointer for model {model} is invalid: {ptr}")]
    SupportPointerInvalid { model: usize, ptr: u32 },

    #[error("invalid model name: {detail}")]
    NameInvalid { detail: String },

    #[error("duplicate model index {index} in one batch")]
    DuplicateBatchIndex { index: usize },

    #[error("arithmetic overflow in {context}")]
    ArithmeticOverflow { context: &'static str },

    #[error("file too large: {size} bytes would exceed u32 offsets")]
    FileTooLarge { size: u64 },

    #[error("opaque overwrite length mismatch: expected {expected}, got {got}")]
    OpaqueLengthMismatch { expected: usize, got: usize },

    #[error("write verification failed (structural re-parse rejected output): {detail}")]
    VerifyFailed { detail: String },

    #[error("format drift: {detail}")]
    FormatDrift { detail: String },

    #[error("unknown opaque region id: {0}")]
    UnknownOpaqueRegion(u32),

    #[error("unsupported opaque-layout mutation: {operation}")]
    UnsupportedOpaqueLayoutMutation { operation: &'static str },
}

pub(crate) type Result<T> = std::result::Result<T, ChituboxError>;
