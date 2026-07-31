//! bytes -> Document. Navigates ONLY the pointer chain (header -> model
//! table -> per-record mesh ptr at 272/276 AND support-index ptr at 268 ->
//! index quad -> records_off/mesh_off). No magic-scanning anywhere. Total
//! coverage: every input byte lands in exactly one region or parse fails.

use crate::codec::{add, mul, read_u32};
use crate::consts::*;
use crate::document::{Document, ModelEntry, SupportSection};
use crate::error::{ChituboxError, Result};
use crate::types::ParseWarning;

struct RawSupport {
    section: SupportSection,
    /// [start, end) of the whole support section in the file
    end: usize,
}

pub(crate) fn parse(bytes: Vec<u8>) -> Result<Document> {
    let n = bytes.len();
    if n < HEADER_LEN {
        return Err(ChituboxError::Truncated {
            need: HEADER_LEN,
            have: n,
            context: "header",
        });
    }
    let magic = read_u32(&bytes, H_MAGIC, "magic")?;
    if magic != FILE_MAGIC {
        return Err(ChituboxError::NotChitubox { found_magic: magic });
    }
    let model_count = read_u32(&bytes, H_MODEL_COUNT, "model_count")? as usize;
    let off8 = read_u32(&bytes, H_OFF8, "off8")? as usize;
    let settings_size = read_u32(&bytes, H_SETTINGS_SIZE, "settings_size")?;
    let const16 = read_u32(&bytes, H_CONST16, "const16")?;
    let off20 = read_u32(&bytes, H_OFF20, "off20")? as usize;

    // Header field 12 ("settings size") is 0 in an empty project and 420 in a
    // populated one — NOT a pinned constant (the format map overstated this).
    // It is stored and re-emitted verbatim; only field 16 is pinned.
    if const16 != CONST16 {
        return Err(ChituboxError::FormatDrift {
            detail: format!("const16 {const16} != {CONST16}"),
        });
    }
    if n < EMPTY_BASELINE_LEN {
        return Err(ChituboxError::Truncated {
            need: EMPTY_BASELINE_LEN,
            have: n,
            context: "settings blob",
        });
    }

    // header invariant
    let table_end = add(
        off8,
        mul(MODEL_RECORD_LEN, model_count, "table")?,
        "table_end",
    )?;
    if model_count == 0 {
        if off20 != 0 {
            return Err(ChituboxError::HeaderInvariant {
                detail: format!("empty project must have off20==0, got {off20}"),
            });
        }
    } else if off20 != table_end {
        return Err(ChituboxError::HeaderInvariant {
            detail: format!("off20 {off20} != off8 + 680*count {table_end}"),
        });
    }

    if off8 < EMPTY_BASELINE_LEN {
        return Err(ChituboxError::HeaderInvariant {
            detail: format!("off8 {off8} < 420"),
        });
    }
    if table_end > n {
        return Err(ChituboxError::Truncated {
            need: table_end,
            have: n,
            context: "model table",
        });
    }

    let settings = bytes[HEADER_LEN..EMPTY_BASELINE_LEN].to_vec();
    let preview = bytes[EMPTY_BASELINE_LEN..off8].to_vec();

    let mut warnings: Vec<ParseWarning> = Vec::new();

    // Parse each model record + locate its regions.
    struct ModelLayout {
        raw_record: [u8; MODEL_RECORD_LEN],
        mesh: Vec<u8>,
        mesh_start: usize,
        support: Option<SupportSection>,
        region_start: usize,
        region_end: usize,
    }
    let mut layouts: Vec<ModelLayout> = Vec::with_capacity(model_count);

    for m in 0..model_count {
        let ro = add(off8, mul(MODEL_RECORD_LEN, m, "record off")?, "record off")?;
        let mut raw = [0u8; MODEL_RECORD_LEN];
        raw.copy_from_slice(&bytes[ro..ro + MODEL_RECORD_LEN]);

        let sptr = read_u32(&raw, M_SUPPORT_INDEX_PTR, "support ptr")? as usize;
        let mesh_off = read_u32(&raw, M_MESH_OFF, "mesh off")? as usize;
        let mesh_sz = read_u32(&raw, M_MESH_SIZE, "mesh size")? as usize;

        let mesh_end = add(mesh_off, mesh_sz, "mesh end")?;
        if mesh_end > n {
            return Err(ChituboxError::Truncated {
                need: mesh_end,
                have: n,
                context: "model mesh",
            });
        }
        if !mesh_sz.is_multiple_of(MESH_BYTES_PER_TRI) {
            return Err(ChituboxError::HeaderInvariant {
                detail: format!("model {m} mesh size {mesh_sz} not divisible by 36"),
            });
        }
        // warn on nonzero unmapped record bytes (post-name-NUL..256, 280..680)
        check_unmapped(&raw, m, &mut warnings);

        let mesh = bytes[mesh_off..mesh_end].to_vec();

        let (support, region_end) = if sptr == 0 {
            (None, mesh_end)
        } else {
            let rs = parse_support(&bytes, m, sptr, mesh_end)?;
            (Some(rs.section), rs.end)
        };

        layouts.push(ModelLayout {
            raw_record: raw,
            mesh,
            mesh_start: mesh_off,
            support,
            region_start: mesh_off,
            region_end,
        });
    }

    // Require layout order == index order (all corpus specimens satisfy this).
    for w in layouts.windows(2) {
        if w[1].region_start < w[0].region_end {
            return Err(ChituboxError::SupportSectionMalformed {
                detail: "model regions overlap or are not in index order".into(),
            });
        }
    }

    // front filler + per-model post filler (total coverage of the tail).
    let mut models: Vec<ModelEntry> = Vec::with_capacity(model_count);
    let front_filler = if model_count == 0 {
        if off8 != n {
            // trailing bytes after an empty project's baseline — capture as
            // preview already did [420..off8]; nothing else. Require off8==n.
            return Err(ChituboxError::HeaderInvariant {
                detail: format!("empty project off8 {off8} != EOF {n}"),
            });
        }
        Vec::new()
    } else {
        let first = layouts[0].mesh_start;
        if first < off20 {
            return Err(ChituboxError::SupportSectionMalformed {
                detail: "first mesh precedes model table end".into(),
            });
        }
        bytes[off20..first].to_vec()
    };

    for (i, ml) in layouts.iter().enumerate() {
        let next_start = if i + 1 < layouts.len() {
            layouts[i + 1].mesh_start
        } else {
            n
        };
        if next_start < ml.region_end {
            return Err(ChituboxError::SupportSectionMalformed {
                detail: format!("model {i} region end exceeds next region / EOF"),
            });
        }
        let post_filler = bytes[ml.region_end..next_start].to_vec();
        models.push(ModelEntry {
            raw_record: ml.raw_record,
            mesh: ml.mesh.clone(),
            support: ml.support.clone(),
            post_filler,
            pristine: None,
            session_z_rot_deg: 0.0,
        });
    }

    Ok(Document {
        settings_size,
        const16,
        settings,
        preview,
        models,
        front_filler,
        original: Some(bytes),
        dirty: false,
        warnings,
    })
}

fn parse_support(bytes: &[u8], m: usize, sptr: usize, mesh_end: usize) -> Result<RawSupport> {
    let n = bytes.len();
    let idx_end = add(sptr, SUPPORT_INDEX_LEN, "support index")?;
    if idx_end > n {
        return Err(ChituboxError::Truncated {
            need: idx_end,
            have: n,
            context: "support index",
        });
    }
    let n_rec = read_u32(bytes, sptr, "index.n")? as usize;
    let rec_off = read_u32(bytes, sptr + 4, "index.rec_off")? as usize;
    let smesh_off = read_u32(bytes, sptr + 8, "index.mesh_off")? as usize;
    let smesh_sz = read_u32(bytes, sptr + 12, "index.mesh_size")? as usize;

    // index-quad consistency
    let smesh_end = add(smesh_off, smesh_sz, "smesh end")?;
    if smesh_end != sptr {
        return Err(ChituboxError::SupportSectionMalformed {
            detail: format!("model {m}: mesh_off+mesh_size {smesh_end} != index off {sptr}"),
        });
    }
    if smesh_off != mesh_end {
        return Err(ChituboxError::SupportSectionMalformed {
            detail: format!(
                "model {m}: support mesh start {smesh_off} != model mesh end {mesh_end}"
            ),
        });
    }
    // `rec_off` must lie AT OR AFTER the end of the 16-byte index quad (so the
    // group-header slice `[idx_end..rec_off]` is well-formed) and within the
    // file. A `rec_off` inside the index quad (`sptr < rec_off < idx_end`) or
    // past EOF would otherwise panic the slice below on malformed input.
    if rec_off < idx_end || rec_off > n {
        return Err(ChituboxError::SupportPointerInvalid {
            model: m,
            ptr: sptr as u32,
        });
    }
    let group_header = bytes[idx_end..rec_off].to_vec();

    let recs_bytes = mul(SEG_RECORD_LEN, n_rec, "records")?;
    let recs_end = add(rec_off, recs_bytes, "records end")?;
    if recs_end > n {
        return Err(ChituboxError::Truncated {
            need: recs_end,
            have: n,
            context: "support records",
        });
    }
    let mut records = Vec::with_capacity(n_rec);
    // (start, end) of every record's referenced blob, for the disjointness
    // audit below.
    let mut blob_ivals: Vec<(usize, usize)> = Vec::with_capacity(n_rec);
    for k in 0..n_rec {
        let o = rec_off + k * SEG_RECORD_LEN;
        let magic = read_u32(bytes, o, "record magic")?;
        if magic != SEG_MAGIC {
            return Err(ChituboxError::SupportSectionMalformed {
                detail: format!("model {m} record {k}: bad magic {magic:#010x} at {o}"),
            });
        }
        // record blob must lie within the support mesh region
        let bo = read_u32(bytes, o + S_MESH_OFF, "record mesh off")? as usize;
        let bl = read_u32(bytes, o + S_MESH_LEN, "record mesh len")? as usize;
        let be = add(bo, bl, "record blob end")?;
        if bo < smesh_off || be > smesh_end {
            return Err(ChituboxError::SupportSectionMalformed {
                detail: format!(
                    "model {m} record {k}: blob [{bo}..{be}] outside support mesh [{smesh_off}..{smesh_end}]"
                ),
            });
        }
        // A non-empty blob participates in the disjointness audit. Zero-length
        // blobs reference no bytes and cannot alias, so they are exempt.
        if bl > 0 {
            blob_ivals.push((bo, be));
        }
        let mut r = [0u8; SEG_RECORD_LEN];
        r.copy_from_slice(&bytes[o..o + SEG_RECORD_LEN]);
        records.push(r);
    }

    // Reject aliased / overlapping record blob ranges. `rotate_one` rotates
    // each record's referenced range independently, so two records sharing (or
    // partially overlapping) a byte range would rotate that range MORE THAN
    // ONCE — silently detaching the shared support mesh from its record
    // endpoints (rigid-assembly ruling violated), while still passing every
    // advertised invariant. No corpus specimen aliases; a well-formed section
    // has strictly disjoint (touching-allowed) blobs.
    blob_ivals.sort_unstable();
    for w in blob_ivals.windows(2) {
        if w[1].0 < w[0].1 {
            return Err(ChituboxError::SupportSectionMalformed {
                detail: format!(
                    "model {m}: record blob ranges alias/overlap ([{}..{}] and [{}..{}])",
                    w[0].0, w[0].1, w[1].0, w[1].1
                ),
            });
        }
    }

    Ok(RawSupport {
        section: SupportSection {
            smesh: bytes[smesh_off..smesh_end].to_vec(),
            group_header,
            records,
            orig_smesh_off: smesh_off as u32,
        },
        end: recs_end,
    })
}

/// Warn (never error) on nonzero bytes in the assumed-zero unmapped spans:
/// after the name NUL up to 256, and 280..680.
fn check_unmapped(raw: &[u8; MODEL_RECORD_LEN], m: usize, warnings: &mut Vec<ParseWarning>) {
    let name_end = raw[..256].iter().position(|&b| b == 0).unwrap_or(256);
    // span 1: after the NUL terminator up to 256
    let s1 = (name_end + 1).min(256);
    let nz1 = raw[s1..256].iter().filter(|&&b| b != 0).count();
    let nz2 = raw[280..MODEL_RECORD_LEN]
        .iter()
        .filter(|&&b| b != 0)
        .count();
    if nz1 + nz2 > 0 {
        warnings.push(ParseWarning {
            context: format!("model {m} record unmapped bytes ({s1}..256, 280..680)"),
            detail: format!(
                "{} nonzero bytes in assumed-zero spans (preserved verbatim)",
                nz1 + nz2
            ),
        });
    }
}
