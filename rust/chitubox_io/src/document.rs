//! Internal raw-backed structural model. Every structural element owns its
//! original bytes; typed accessors patch in place; the serializer re-emits
//! regions in canonical file order recomputing only counters and absolute
//! offsets. This is what makes the gold invariants cheap.

use crate::consts::*;
use crate::stl::bbox;

/// A model's support layer.
#[derive(Clone, Debug)]
pub(crate) struct SupportSection {
    /// concatenated segment mesh blobs (may contain dead/unreferenced bytes)
    pub smesh: Vec<u8>,
    /// 400-byte group header, verbatim (all-zero in every specimen)
    pub group_header: Vec<u8>,
    /// raw 72-byte records; internal mesh_off (bytes 40..44) is RELATIVE to
    /// `orig_smesh_off` and relocated on write
    pub records: Vec<[u8; SEG_RECORD_LEN]>,
    /// absolute smesh start at parse time; 0 for freshly-built sections whose
    /// record mesh_off fields are already relative to smesh start
    pub orig_smesh_off: u32,
}

impl SupportSection {
    pub fn record_count(&self) -> usize {
        self.records.len()
    }
}

/// Snapshot of a model's geometry taken before its first rotation, plus the
/// cumulative session angle — the rebake source (avoids f32 drift over many
/// small turns).
#[derive(Clone, Debug)]
pub(crate) struct Pristine {
    pub mesh: Vec<u8>,
    pub smesh: Vec<u8>,
    /// (start, end) per support record, model-centered
    pub record_geo: Vec<([f32; 3], [f32; 3])>,
}

/// One model: verbatim 680-byte record + mesh + optional supports + trailing
/// filler up to the next model's mesh (or EOF).
#[derive(Clone, Debug)]
pub(crate) struct ModelEntry {
    pub raw_record: [u8; MODEL_RECORD_LEN],
    pub mesh: Vec<u8>,
    pub support: Option<SupportSection>,
    /// bytes after this model's last region up to the next model's mesh /
    /// EOF (the 20-byte inter-model gaps and trailers seen on supported files)
    pub post_filler: Vec<u8>,
    pub pristine: Option<Pristine>,
    pub session_z_rot_deg: f32,
}

impl ModelEntry {
    pub fn name(&self) -> String {
        let end = self.raw_record[..256]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(256);
        String::from_utf8_lossy(&self.raw_record[..end]).into_owned()
    }

    pub fn translate(&self) -> [f32; 3] {
        [
            f32::from_le_bytes(self.raw_record[M_TRANSLATE_X..M_TRANSLATE_X + 4].try_into().unwrap()),
            f32::from_le_bytes(self.raw_record[M_TRANSLATE_Y..M_TRANSLATE_Y + 4].try_into().unwrap()),
            f32::from_le_bytes(self.raw_record[M_TRANSLATE_Z..M_TRANSLATE_Z + 4].try_into().unwrap()),
        ]
    }

    pub fn set_translate(&mut self, t: [f32; 3]) {
        self.raw_record[M_TRANSLATE_X..M_TRANSLATE_X + 4].copy_from_slice(&t[0].to_le_bytes());
        self.raw_record[M_TRANSLATE_Y..M_TRANSLATE_Y + 4].copy_from_slice(&t[1].to_le_bytes());
        self.raw_record[M_TRANSLATE_Z..M_TRANSLATE_Z + 4].copy_from_slice(&t[2].to_le_bytes());
    }

    pub fn triangle_count(&self) -> usize {
        self.mesh.len() / MESH_BYTES_PER_TRI
    }

    /// Decode the centered vertex stream (9 f32 / tri).
    pub fn mesh_f32(&self) -> Vec<f32> {
        bytes_to_f32(&self.mesh)
    }

    pub fn local_bbox(&self) -> ([f32; 3], [f32; 3]) {
        bbox(&self.mesh_f32())
    }
}

/// The parsed container.
#[derive(Clone, Debug)]
pub(crate) struct Document {
    /// header field 12 (settings size, pinned 420) — stored for fidelity
    pub settings_size: u32,
    /// header field 16 (pinned 412)
    pub const16: u32,
    pub settings: Vec<u8>,
    pub preview: Vec<u8>,
    pub models: Vec<ModelEntry>,
    /// filler between model table end (off20) and first mesh
    pub front_filler: Vec<u8>,
    /// exact opened bytes, kept while clean as a belt-and-braces guard
    pub original: Option<Vec<u8>>,
    pub dirty: bool,
    pub warnings: Vec<crate::types::ParseWarning>,
}

// -- f32 <-> bytes helpers --------------------------------------------------

pub(crate) fn bytes_to_f32(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

pub(crate) fn f32_to_bytes(f: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(f.len() * 4);
    for v in f {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}
