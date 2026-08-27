//! Document -> bytes. Emits regions in canonical file order (which equals the
//! observed order for every corpus specimen) and recomputes ONLY counters and
//! absolute offsets. Absent mutation this reproduces the input byte-for-byte.

use crate::codec::{add, mul, u32_of, write_u32};
use crate::consts::*;
use crate::document::Document;
use crate::error::{ChituboxError, Result};

/// Per-model computed absolute offsets for the emitted layout.
struct ModelPlan {
    mesh_off: usize,
    smesh_off: usize,
    index_off: usize,
    rec_off: usize,
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged as a \
          class, and the one architecture item the class carried (geom_kernel's 32-bit BVH index \
          width) is now a named, enforced limit. TODO(cast-width-audit): retire site by site.  See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
pub(crate) fn serialize(doc: &Document) -> Result<Vec<u8>> {
    let count = doc.models.len();
    let off8 = add(EMPTY_BASELINE_LEN, doc.preview.len(), "off8")?;
    let table_bytes = mul(MODEL_RECORD_LEN, count, "table")?;
    let off20 = add(off8, table_bytes, "off20")?;

    // -- pass 1: compute layout offsets --
    let mut pos = add(off20, doc.front_filler.len(), "front filler")?;
    let mut plans: Vec<ModelPlan> = Vec::with_capacity(count);
    for me in &doc.models {
        let mesh_off = pos;
        pos = add(pos, me.mesh.len(), "mesh")?;
        let (smesh_off, index_off, rec_off) = if let Some(s) = &me.support {
            let smesh_off = pos;
            pos = add(pos, s.smesh.len(), "smesh")?;
            let index_off = pos;
            pos = add(pos, SUPPORT_INDEX_LEN, "index")?;
            pos = add(pos, s.group_header.len(), "group header")?;
            let rec_off = pos;
            pos = add(
                pos,
                mul(SEG_RECORD_LEN, s.records.len(), "records")?,
                "records",
            )?;
            (smesh_off, index_off, rec_off)
        } else {
            (0, 0, 0)
        };
        pos = add(pos, me.post_filler.len(), "post filler")?;
        plans.push(ModelPlan {
            mesh_off,
            smesh_off,
            index_off,
            rec_off,
        });
    }
    let total = pos;
    u32_of(total)?; // FileTooLarge if > u32

    // -- pass 2: emit --
    let mut out = Vec::with_capacity(total);

    // header
    let mut header = [0u8; HEADER_LEN];
    write_u32(&mut header, H_MAGIC, FILE_MAGIC);
    write_u32(&mut header, H_MODEL_COUNT, u32_of(count)?);
    write_u32(&mut header, H_OFF8, u32_of(off8)?);
    write_u32(&mut header, H_SETTINGS_SIZE, doc.settings_size);
    write_u32(&mut header, H_CONST16, doc.const16);
    write_u32(
        &mut header,
        H_OFF20,
        if count == 0 { 0 } else { u32_of(off20)? },
    );
    out.extend_from_slice(&header);

    out.extend_from_slice(&doc.settings);
    out.extend_from_slice(&doc.preview);

    // model table (records patched with computed offsets)
    for (me, plan) in doc.models.iter().zip(&plans) {
        let mut rec = me.raw_record;
        let sptr = if me.support.is_some() {
            u32_of(plan.index_off)?
        } else {
            0
        };
        write_u32(&mut rec, M_SUPPORT_INDEX_PTR, sptr);
        write_u32(&mut rec, M_MESH_OFF, u32_of(plan.mesh_off)?);
        write_u32(&mut rec, M_MESH_SIZE, u32_of(me.mesh.len())?);
        out.extend_from_slice(&rec);
    }

    out.extend_from_slice(&doc.front_filler);

    // per-model data regions
    for (me, plan) in doc.models.iter().zip(&plans) {
        out.extend_from_slice(&me.mesh);
        if let Some(s) = &me.support {
            out.extend_from_slice(&s.smesh);
            // index quad
            let mut idx = [0u8; SUPPORT_INDEX_LEN];
            write_u32(&mut idx, 0, u32_of(s.records.len())?);
            write_u32(&mut idx, 4, u32_of(plan.rec_off)?);
            write_u32(&mut idx, 8, u32_of(plan.smesh_off)?);
            write_u32(&mut idx, 12, u32_of(s.smesh.len())?);
            out.extend_from_slice(&idx);
            out.extend_from_slice(&s.group_header);
            // records with relocated blob offsets
            let base = plan.smesh_off as i64 - s.orig_smesh_off as i64;
            for r in &s.records {
                let mut rr = *r;
                let old = u32::from_le_bytes(rr[S_MESH_OFF..S_MESH_OFF + 4].try_into().unwrap());
                let new = old as i64 + base;
                if new < 0 || new > u32::MAX as i64 {
                    return Err(ChituboxError::ArithmeticOverflow {
                        context: "record blob relocation",
                    });
                }
                write_u32(&mut rr, S_MESH_OFF, new as u32);
                out.extend_from_slice(&rr);
            }
        }
        out.extend_from_slice(&me.post_filler);
    }

    debug_assert_eq!(out.len(), total);
    Ok(out)
}
