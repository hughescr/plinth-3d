//! Typed support data of one model: preset table, attr array and the support
//! records with their meshes (spec sections 8 to 10).

use crate::mesh::Triangle;

/// Magic at the start of every support record.
pub const RECORD_MAGIC: u32 = 0xEA34_2389;
/// Bytes per support record.
pub const RECORD_BYTES: u64 = 72;
/// Bytes of the support index.
pub const INDEX_BYTES: u64 = 16;
/// Bytes of the zero group header between the index and the records.
pub const GROUP_HEADER_BYTES: u64 = 400;
/// Bytes of the zero trailer after the records.
pub const TRAILER_BYTES: u64 = 20;
/// Bytes per preset table entry (`id`, `name_ptr`, `name_len`, each u64).
pub const PRESET_ENTRY_BYTES: u64 = 24;

/// The numeric record type. Unknown values pass through unchanged. The role names
/// are inferred from position in the native samples (spec section 10.1); the shape
/// is decided by the record's own fields and mesh, not by the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RecordType(pub u32);

impl RecordType {
    /// Tip cone from the tip ball to the first joint (frustum).
    pub const TIP_CONE: Self = Self(1);
    /// Lower foot cone of a model-to-model support (frustum).
    pub const FOOT_CONE: Self = Self(2);
    /// Rod: pillar, trunk, branch, link, leg, post or brace (frustum).
    pub const ROD: Self = Self(3);
    /// Base flare (frustum).
    pub const BASE_FLARE: Self = Self(4);
    /// Base pad of the default mat (frustum, radii reversed in the record).
    pub const BASE_PAD: Self = Self(5);
    /// Raft part: pad, bridge or hex mat.
    pub const RAFT: Self = Self(6);
    /// Upper tip ball at the model contact (sphere).
    pub const TIP_BALL: Self = Self(7);
    /// Lower foot ball of a model-to-model support (sphere).
    pub const FOOT_BALL: Self = Self(8);
    /// Joint ball (sphere).
    pub const JOINT_BALL: Self = Self(9);
}

/// One support record: a part (rod, ball, pad...) with its own mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct SupportRecord {
    /// The record type.
    pub record_type: RecordType,
    /// Start point, model-local frame.
    pub start: [f32; 3],
    /// End point, model-local frame.
    pub end: [f32; 3],
    /// Radius field r1.
    pub r1: f32,
    /// Radius field r2.
    pub r2: f32,
    /// The six tail words at record offset 48, raw. See [`SupportRecord::tail_f32`].
    pub tail: [u32; 6],
    /// The part's mesh (its bytes in the support blob).
    pub mesh: Vec<Triangle>,
}

impl SupportRecord {
    /// Tail word `i` read as an f32 (type 1 holds the contact value here, type 2 holds 0.15).
    #[must_use]
    pub fn tail_f32(&self, i: usize) -> f32 {
        f32::from_bits(self.tail.get(i).copied().unwrap_or(0))
    }
}

/// One entry of a model's preset table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preset {
    /// The preset id. The low byte of an attr is `id << 2`.
    pub id: u64,
    /// The preset name (for example `Ultralight`).
    pub name: String,
}

/// Everything a supported model carries beyond its mesh.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Supports {
    /// Preset table, in file order.
    pub presets: Vec<Preset>,
    /// Attr array in the front block: one u64 per record in a clean save.
    pub attrs: Vec<u64>,
    /// Bytes at the start of the support-mesh blob that no record refers to.
    /// Empty in every clean ChituBox save.
    pub unreferenced_blob: Vec<u8>,
    /// Support records, in file order. Record meshes follow each other in the blob.
    pub records: Vec<SupportRecord>,
}

/// A way a project differs from a clean ChituBox save. Such files still read and
/// write; the list says where the file is not what ChituBox writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Deviation {
    /// The attr array does not have one entry per record.
    AttrCount {
        /// Model index.
        model: usize,
        /// Entries in the attr array.
        attrs: usize,
        /// Support records.
        records: usize,
    },
    /// The support blob holds bytes that no record refers to.
    UnreferencedBlob {
        /// Model index.
        model: usize,
        /// Number of unreferenced bytes.
        bytes: usize,
    },
}

impl Supports {
    /// Deviations from a clean save for the model at index `model`.
    pub(crate) fn deviations(&self, model: usize, out: &mut Vec<Deviation>) {
        if self.attrs.len() != self.records.len() {
            out.push(Deviation::AttrCount {
                model,
                attrs: self.attrs.len(),
                records: self.records.len(),
            });
        }
        if !self.unreferenced_blob.is_empty() {
            out.push(Deviation::UnreferencedBlob {
                model,
                bytes: self.unreferenced_blob.len(),
            });
        }
    }
}
