//! The one error type of this crate. Every variant names where the problem is, so a
//! caller can print a located diagnostic.

use std::fmt;

/// Why a read, a mesh build or a batch query was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeomError {
    /// The file could not be read.
    Io {
        /// The path that failed.
        path: String,
        /// The operating system message.
        message: String,
    },
    /// The bytes are neither a binary nor an ASCII STL file.
    StlUnrecognized {
        /// Length of the input.
        bytes: usize,
    },
    /// A binary STL header declares more triangles than the file holds (or fewer).
    StlSizeMismatch {
        /// Triangle count in the header.
        declared_triangles: u32,
        /// File size the header implies.
        expected_bytes: u64,
        /// Actual file size.
        actual_bytes: usize,
    },
    /// An ASCII STL file is malformed.
    StlAscii {
        /// 1-based line number.
        line: usize,
        /// What is wrong.
        message: String,
    },
    /// A coordinate is NaN or infinite.
    NonFinite {
        /// Which array.
        what: &'static str,
        /// Index of the first bad value.
        index: usize,
    },
    /// A count does not fit the 32-bit indices of the mesh.
    TooLarge {
        /// What was counted.
        what: &'static str,
        /// The count.
        count: usize,
    },
    /// A triangle refers to a vertex that does not exist.
    BadIndex {
        /// Triangle number.
        triangle: usize,
        /// The bad vertex index.
        index: u32,
        /// Number of vertices.
        vertices: usize,
    },
    /// Two arrays that must have the same length differ.
    LengthMismatch {
        /// Which array.
        what: &'static str,
        /// Length required.
        expected: usize,
        /// Length given.
        actual: usize,
    },
    /// The mesh has no triangles, so no acceleration structure can be built.
    EmptyMesh,
}

impl fmt::Display for GeomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, message } => write!(f, "cannot read {path}: {message}"),
            Self::StlUnrecognized { bytes } => {
                write!(
                    f,
                    "not an STL file ({bytes} bytes): neither binary nor ASCII"
                )
            }
            Self::StlSizeMismatch {
                declared_triangles,
                expected_bytes,
                actual_bytes,
            } => write!(
                f,
                "binary STL header declares {declared_triangles} triangles \
                 ({expected_bytes} bytes) but the file has {actual_bytes} bytes"
            ),
            Self::StlAscii { line, message } => write!(f, "ASCII STL line {line}: {message}"),
            Self::NonFinite { what, index } => {
                write!(f, "{what}[{index}] is not a finite number")
            }
            Self::TooLarge { what, count } => {
                write!(f, "{what} count {count} does not fit 32-bit indices")
            }
            Self::BadIndex {
                triangle,
                index,
                vertices,
            } => write!(
                f,
                "triangle {triangle} uses vertex {index} but the mesh has {vertices} vertices"
            ),
            Self::LengthMismatch {
                what,
                expected,
                actual,
            } => {
                write!(f, "{what} has length {actual}, expected {expected}")
            }
            Self::EmptyMesh => write!(f, "the mesh has no triangles"),
        }
    }
}

impl std::error::Error for GeomError {}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, GeomError>;
