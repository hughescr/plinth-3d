//! Segment-record codec and support-section assembly. Mirrors the validated
//! Python `build_record` / `write_replace` 1:1: magic, type, start, end,
//! radii, library-assigned mesh off/len, trailing bytes verbatim.

use crate::codec::u32_of;
use crate::consts::*;
use crate::document::{bytes_to_f32, f32_to_bytes, SupportSection};
use crate::error::{ChituboxError, Result};
use crate::stl::all_finite;
use crate::types::{SegmentType, SupportRecord, TriangleSoup};

/// Build a fresh support section from typed records. Mesh offsets are assigned
/// sequentially from smesh start; `orig_smesh_off` is 0 so the write path
/// relocates them to absolute positions. `group_header` is 400 zeros unless a
/// donor (existing section's header) is supplied (Replace keeps it verbatim).
pub(crate) fn build_section(
    records: &[SupportRecord],
    group_header: Vec<u8>,
) -> Result<SupportSection> {
    let mut smesh: Vec<u8> = Vec::new();
    let mut raw_records: Vec<[u8; SEG_RECORD_LEN]> = Vec::with_capacity(records.len());
    let mut off: usize = 0;
    for (i, r) in records.iter().enumerate() {
        if r.mesh.xyz.len() % FLOATS_PER_TRI != 0 {
            return Err(ChituboxError::BadTriangleBuffer { len: r.mesh.xyz.len() });
        }
        if !all_finite(&r.mesh.xyz) {
            return Err(ChituboxError::NonFiniteValue {
                context: "support record mesh",
            });
        }
        for v in r.start.iter().chain(r.end.iter()).chain([r.r_start, r.r_end].iter()) {
            if !v.is_finite() {
                return Err(ChituboxError::NonFiniteValue {
                    context: "support record geometry",
                });
            }
        }
        let blob = f32_to_bytes(&r.mesh.xyz);
        let mesh_len = blob.len();
        let raw = encode_record(r, u32_of(off)?, u32_of(mesh_len)?);
        raw_records.push(raw);
        smesh.extend_from_slice(&blob);
        off = off
            .checked_add(mesh_len)
            .ok_or(ChituboxError::ArithmeticOverflow { context: "smesh assembly" })?;
        let _ = i;
    }
    Ok(SupportSection {
        smesh,
        group_header,
        records: raw_records,
        orig_smesh_off: 0,
    })
}

/// Encode one 72-byte record. `mesh_off` is relative to smesh start.
fn encode_record(r: &SupportRecord, mesh_off: u32, mesh_len: u32) -> [u8; SEG_RECORD_LEN] {
    let mut raw = [0u8; SEG_RECORD_LEN];
    raw[S_MAGIC..S_MAGIC + 4].copy_from_slice(&SEG_MAGIC.to_le_bytes());
    raw[S_TYPE..S_TYPE + 4].copy_from_slice(&r.segment_type.0.to_le_bytes());
    for k in 0..3 {
        raw[S_START + k * 4..S_START + k * 4 + 4].copy_from_slice(&r.start[k].to_le_bytes());
        raw[S_END + k * 4..S_END + k * 4 + 4].copy_from_slice(&r.end[k].to_le_bytes());
    }
    raw[S_R1..S_R1 + 4].copy_from_slice(&r.r_start.to_le_bytes());
    raw[S_R2..S_R2 + 4].copy_from_slice(&r.r_end.to_le_bytes());
    raw[S_MESH_OFF..S_MESH_OFF + 4].copy_from_slice(&mesh_off.to_le_bytes());
    raw[S_MESH_LEN..S_MESH_LEN + 4].copy_from_slice(&mesh_len.to_le_bytes());
    raw[S_TRAILING..S_TRAILING + 24].copy_from_slice(&r.trailing);
    raw
}

/// Decode a raw record + its blob (located via mesh_off relative to
/// `orig_smesh_off`) into a public `SupportRecord`.
pub(crate) fn decode_record(
    raw: &[u8; SEG_RECORD_LEN],
    smesh: &[u8],
    orig_smesh_off: u32,
) -> Result<SupportRecord> {
    let g = |o: usize| f32::from_le_bytes(raw[o..o + 4].try_into().unwrap());
    let u = |o: usize| u32::from_le_bytes(raw[o..o + 4].try_into().unwrap());
    let seg_type = SegmentType(u(S_TYPE));
    let start = [g(S_START), g(S_START + 4), g(S_START + 8)];
    let end = [g(S_END), g(S_END + 4), g(S_END + 8)];
    let r_start = g(S_R1);
    let r_end = g(S_R2);
    let mesh_off = u(S_MESH_OFF) as usize;
    let mesh_len = u(S_MESH_LEN) as usize;
    let rel = mesh_off
        .checked_sub(orig_smesh_off as usize)
        .ok_or(ChituboxError::SupportSectionMalformed {
            detail: "record mesh_off precedes smesh start".into(),
        })?;
    let blob = smesh
        .get(rel..rel + mesh_len)
        .ok_or(ChituboxError::SupportSectionMalformed {
            detail: "record blob out of smesh bounds".into(),
        })?;
    let mut trailing = [0u8; 24];
    trailing.copy_from_slice(&raw[S_TRAILING..S_TRAILING + 24]);
    Ok(SupportRecord {
        segment_type: seg_type,
        start,
        end,
        r_start,
        r_end,
        mesh: TriangleSoup::new(bytes_to_f32(blob)),
        trailing,
    })
}

/// Decode every record of a section (batch).
pub(crate) fn decode_section(section: &SupportSection) -> Result<Vec<SupportRecord>> {
    section
        .records
        .iter()
        .map(|r| decode_record(r, &section.smesh, section.orig_smesh_off))
        .collect()
}
