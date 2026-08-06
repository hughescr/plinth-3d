//! Structural re-parse verification (gold invariant b) plus an INDEPENDENT
//! interval audit that does not reuse parser code paths. "Verify passed"
//! means structurally self-consistent, never "ChituBox-valid".

use crate::consts::*;
use crate::error::{ChituboxError, Result};
use crate::parse::parse;

/// Re-parse the serialized output and run the independent interval audit.
/// Any failure is a `VerifyFailed`.
pub(crate) fn verify(bytes: &[u8]) -> Result<()> {
    // (1) structural re-parse must succeed
    parse(bytes.to_vec()).map_err(|e| ChituboxError::VerifyFailed {
        detail: format!("re-parse rejected output: {e}"),
    })?;
    // (2) independent interval audit (fresh code path)
    audit(bytes).map_err(|d| ChituboxError::VerifyFailed { detail: d })
}

struct Interval {
    start: usize,
    end: usize,
    what: &'static str,
}

/// First-principles layout audit. Reads the pointer chain directly (no shared
/// parser helpers), collects every live region, and asserts non-overlap,
/// forward ranges, pointer sanity, per-model section ordering, mesh
/// divisibility, and record-blob containment.
fn audit(b: &[u8]) -> std::result::Result<(), String> {
    let n = b.len();
    if n > u32::MAX as usize {
        return Err(format!("output size {n} exceeds u32"));
    }
    let rd = |o: usize| -> std::result::Result<u32, String> {
        b.get(o..o + 4)
            .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
            .ok_or_else(|| format!("read u32 out of bounds at {o}"))
    };

    let count = rd(H_MODEL_COUNT)? as usize;
    let off8 = rd(H_OFF8)? as usize;
    let off20 = rd(H_OFF20)? as usize;
    let table_end = off8
        .checked_add(
            MODEL_RECORD_LEN
                .checked_mul(count)
                .ok_or("table overflow")?,
        )
        .ok_or("table_end overflow")?;
    if count > 0 && off20 != table_end {
        return Err(format!("off20 {off20} != table_end {table_end}"));
    }

    let mut ivs: Vec<Interval> = vec![
        Interval {
            start: 0,
            end: HEADER_LEN,
            what: "header",
        },
        Interval {
            start: HEADER_LEN,
            end: EMPTY_BASELINE_LEN,
            what: "settings",
        },
        Interval {
            start: EMPTY_BASELINE_LEN,
            end: off8,
            what: "preview",
        },
        Interval {
            start: off8,
            end: table_end,
            what: "table",
        },
    ];

    if count > 0 {
        let descriptor_bytes = FROM_SCRATCH_MODEL_DESCRIPTOR_LEN
            .checked_mul(count)
            .ok_or("front descriptor overflow")?;
        let descriptor_end = off20
            .checked_add(descriptor_bytes)
            .ok_or("front descriptor end overflow")?;
        let first_mesh = rd(off8 + M_MESH_OFF)? as usize;
        if first_mesh < descriptor_end {
            return Err(format!(
                "first mesh {first_mesh} overlaps/underfills {descriptor_bytes}-byte front descriptor region [{off20}..{descriptor_end}]"
            ));
        }
        ivs.push(Interval {
            start: off20,
            end: descriptor_end,
            what: "front_descriptors",
        });
        ivs.push(Interval {
            start: descriptor_end,
            end: first_mesh,
            what: "front_opaque",
        });
    }

    let in_header_or_table = |x: usize| x < HEADER_LEN || (count > 0 && x >= off8 && x < table_end);

    for m in 0..count {
        let ro = off8 + m * MODEL_RECORD_LEN;
        let sptr = rd(ro + M_SUPPORT_INDEX_PTR)? as usize;
        let mesh_off = rd(ro + M_MESH_OFF)? as usize;
        let mesh_sz = rd(ro + M_MESH_SIZE)? as usize;
        let mesh_end = mesh_off.checked_add(mesh_sz).ok_or("mesh end overflow")?;
        if mesh_end > n {
            return Err(format!("model {m} mesh end {mesh_end} > EOF {n}"));
        }
        if !mesh_sz.is_multiple_of(MESH_BYTES_PER_TRI) {
            return Err(format!("model {m} mesh size {mesh_sz} not /36"));
        }
        if in_header_or_table(mesh_off) {
            return Err(format!("model {m} mesh_off {mesh_off} into header/table"));
        }
        ivs.push(Interval {
            start: mesh_off,
            end: mesh_end,
            what: "mesh",
        });

        if sptr != 0 {
            if in_header_or_table(sptr) {
                return Err(format!("model {m} sptr {sptr} into header/table"));
            }
            let n_rec = rd(sptr)? as usize;
            let rec_off = rd(sptr + 4)? as usize;
            let smesh_off = rd(sptr + 8)? as usize;
            let smesh_sz = rd(sptr + 12)? as usize;
            let smesh_end = smesh_off
                .checked_add(smesh_sz)
                .ok_or("smesh end overflow")?;
            // ordering: mesh < smesh < index < group < records
            if smesh_off != mesh_end {
                return Err(format!(
                    "model {m} smesh_off {smesh_off} != mesh_end {mesh_end}"
                ));
            }
            if smesh_end != sptr {
                return Err(format!("model {m} smesh_end {smesh_end} != index {sptr}"));
            }
            // `rec_off` may sit AT the end of the index quad (empty group
            // header) or after it; only a `rec_off` pointing INTO the index
            // quad is malformed. Parse (parse.rs) accepts `rec_off == idx_end`,
            // so the audit must too, or gold invariant (a) — parse-then-save of
            // an unmodified file is byte-identical — breaks for empty headers.
            if rec_off < sptr + SUPPORT_INDEX_LEN {
                return Err(format!("model {m} rec_off {rec_off} into index quad"));
            }
            let recs_end = rec_off
                .checked_add(SEG_RECORD_LEN.checked_mul(n_rec).ok_or("recs overflow")?)
                .ok_or("recs end overflow")?;
            if recs_end > n {
                return Err(format!("model {m} records end {recs_end} > EOF {n}"));
            }
            ivs.push(Interval {
                start: smesh_off,
                end: smesh_end,
                what: "smesh",
            });
            ivs.push(Interval {
                start: sptr,
                end: sptr + SUPPORT_INDEX_LEN,
                what: "index",
            });
            ivs.push(Interval {
                start: sptr + SUPPORT_INDEX_LEN,
                end: rec_off,
                what: "group_header",
            });
            ivs.push(Interval {
                start: rec_off,
                end: recs_end,
                what: "records",
            });
            for k in 0..n_rec {
                let o = rec_off + k * SEG_RECORD_LEN;
                if rd(o)? != SEG_MAGIC {
                    return Err(format!("model {m} record {k} bad magic"));
                }
                let bo = rd(o + S_MESH_OFF)? as usize;
                let bl = rd(o + S_MESH_LEN)? as usize;
                let be = bo.checked_add(bl).ok_or("blob end overflow")?;
                if bo < smesh_off || be > smesh_end {
                    return Err(format!(
                        "model {m} record {k} blob [{bo}..{be}] outside smesh [{smesh_off}..{smesh_end}]"
                    ));
                }
            }
        }
    }

    // non-overlap + forward-range audit
    ivs.retain(|iv| iv.end > iv.start); // drop empty (e.g. zero-length preview)
    ivs.sort_by_key(|iv| iv.start);
    for iv in &ivs {
        if iv.start > iv.end || iv.end > n {
            return Err(format!(
                "bad interval {} [{}..{}]",
                iv.what, iv.start, iv.end
            ));
        }
    }
    for w in ivs.windows(2) {
        if w[1].start < w[0].end {
            return Err(format!(
                "overlap: {} [{}..{}] vs {} [{}..{}]",
                w[0].what, w[0].start, w[0].end, w[1].what, w[1].start, w[1].end
            ));
        }
    }
    Ok(())
}
