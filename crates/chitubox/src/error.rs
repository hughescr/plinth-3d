//! Error types for the reader and the writer. No dependencies: both implement
//! `std::error::Error` by hand.

use std::fmt;

/// Why a byte string is not a `.chitubox` file this crate accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// File offset where the problem was found (best effort; 0 when no single offset fits).
    pub offset: u64,
    /// What went wrong.
    pub kind: ParseErrorKind,
}

/// The reasons a parse can fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// The file ends before `what` is complete.
    Truncated {
        /// The region being read.
        what: &'static str,
        /// Bytes that region needs from the offset.
        need: u64,
    },
    /// The first four bytes are not the `.chitubox` magic.
    BadMagic(u32),
    /// A pointer or length does not match where the layout says the region is.
    Mismatch {
        /// The field.
        what: &'static str,
        /// Value the layout requires.
        expected: u64,
        /// Value found in the file.
        found: u64,
    },
    /// A region that is zero in every ChituBox file holds a nonzero byte.
    NonZero {
        /// The region.
        what: &'static str,
    },
    /// A field holds a value outside its allowed range.
    Invalid {
        /// The field.
        what: &'static str,
        /// The value found.
        value: u64,
    },
    /// A float that must be finite is NaN or infinite.
    NonFinite {
        /// The field.
        what: &'static str,
    },
    /// Arithmetic on sizes or offsets overflowed.
    Overflow {
        /// The computation.
        what: &'static str,
    },
    /// Bytes remain after the last region.
    TrailingBytes {
        /// How many bytes remain.
        count: u64,
    },
    /// The preview data is not the canonical run-length encoding of its pixels.
    NonCanonicalPreview,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at offset {}: ", self.offset)?;
        match &self.kind {
            ParseErrorKind::Truncated { what, need } => {
                write!(f, "file ends inside {what} (needs {need} bytes)")
            }
            ParseErrorKind::BadMagic(m) => write!(f, "bad magic 0x{m:08x}"),
            ParseErrorKind::Mismatch {
                what,
                expected,
                found,
            } => write!(f, "{what} is {found}, layout requires {expected}"),
            ParseErrorKind::NonZero { what } => write!(f, "{what} must be all zero"),
            ParseErrorKind::Invalid { what, value } => {
                write!(f, "{what} has invalid value {value}")
            }
            ParseErrorKind::NonFinite { what } => write!(f, "{what} is not a finite number"),
            ParseErrorKind::Overflow { what } => write!(f, "arithmetic overflow in {what}"),
            ParseErrorKind::TrailingBytes { count } => {
                write!(f, "{count} bytes after the last region")
            }
            ParseErrorKind::NonCanonicalPreview => {
                write!(f, "preview is not canonical run-length data")
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// Why a `Project` cannot be written. Checked before any byte is produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    /// A size, count or offset does not fit its field.
    TooLarge {
        /// The quantity.
        what: &'static str,
    },
    /// A model name is 256 bytes or longer, or holds a NUL byte.
    BadName {
        /// Index of the model.
        model: usize,
    },
    /// A float that must be finite is NaN or infinite.
    NonFinite {
        /// The field.
        what: &'static str,
    },
    /// The preview is above the pixel limit, its pixel count is not width times height, or a
    /// pixel has the repeat bit set.
    BadPreview,
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WriteError::TooLarge { what } => write!(f, "{what} does not fit its field"),
            WriteError::BadName { model } => write!(f, "model {model} has an invalid name"),
            WriteError::NonFinite { what } => write!(f, "{what} is not a finite number"),
            WriteError::BadPreview => write!(
                f,
                "preview is too large, or its pixels do not match width x height"
            ),
        }
    }
}

impl std::error::Error for WriteError {}
