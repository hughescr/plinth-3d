//! The typed project: everything in a `.chitubox` file except offsets, counts and
//! lengths, which the writer rebuilds (spec section 1).

use crate::mesh::Triangle;
use crate::preview::Preview;
use crate::supports::{Deviation, Supports};

/// Magic number at offset 0 (`43 12 23 ab` on disk).
pub const MAGIC: u32 = 0xAB23_1243;
/// Header size in bytes.
pub const HEADER_BYTES: u64 = 24;
/// Size of the all-zero block after the header.
pub const ZERO_BLOCK_BYTES: u64 = 396;
/// Offset of the preview header; also the file size of an empty project.
pub const PREVIEW_HEADER_OFFSET: u64 = 420;
/// Preview header size in bytes.
pub const PREVIEW_HEADER_BYTES: u64 = 32;
/// Offset of the preview data (`PREVIEW_HEADER_OFFSET + PREVIEW_HEADER_BYTES`).
pub const PREVIEW_DATA_OFFSET: u64 = 452;
/// Bytes per model table record.
pub const MODEL_RECORD_BYTES: u64 = 680;
/// Bytes per front-block descriptor.
pub const DESCRIPTOR_BYTES: u64 = 24;
/// Bytes of a model name field, including the NUL padding.
pub const NAME_BYTES: usize = 256;
/// Header field `f16`; 412 in every ChituBox file, meaning unknown.
pub const HEADER_F16: u32 = 412;

/// One model: name, translation, mesh and, if supported, its support data.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    /// Name, UTF-8, shorter than [`NAME_BYTES`] bytes, no NUL.
    pub name: String,
    /// Translate X, Y, Z in mm. Z is the lift above the plate (spec section 6.2).
    pub translate: [f32; 3],
    /// Mesh triangles. Rotation and scale are baked in.
    pub mesh: Vec<Triangle>,
    /// Support data; `None` for an unsupported model.
    pub supports: Option<Supports>,
}

/// A whole `.chitubox` file as typed data.
#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    /// Header field `f16` (412 in every ChituBox file; meaning unknown), kept as read.
    pub header_f16: u32,
    /// Preview image; `None` writes `f12 = 0` and no preview (as `empty.chitubox`).
    pub preview: Option<Preview>,
    /// Models in file order.
    pub models: Vec<Model>,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            header_f16: HEADER_F16,
            preview: None,
            models: Vec::new(),
        }
    }
}

impl Project {
    /// Where this project differs from a clean ChituBox save (empty for every clean
    /// native file). Reading and writing do not depend on this list.
    #[must_use]
    pub fn deviations(&self) -> Vec<Deviation> {
        let mut out = Vec::new();
        for (i, m) in self.models.iter().enumerate() {
            if let Some(s) = &m.supports {
                s.deviations(i, &mut out);
            }
        }
        out
    }
}
