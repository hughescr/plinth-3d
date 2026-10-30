//! The reader. It follows the pointer chain in file order and requires every pointer
//! to equal the position where the previous region ended, so every byte belongs to
//! exactly one region and no region overlaps another. Sizes are checked before any
//! allocation, so a hostile count cannot allocate more than the file holds (the
//! preview is capped by [`crate::preview::MAX_PIXELS`]).

use crate::bytes::{Reader, err, usize_of};
use crate::error::{ParseError, ParseErrorKind};
use crate::mesh;
use crate::preview::{self, Preview};
use crate::project::{
    DESCRIPTOR_BYTES, MAGIC, MODEL_RECORD_BYTES, Model, NAME_BYTES, PREVIEW_DATA_OFFSET,
    PREVIEW_HEADER_OFFSET, Project, ZERO_BLOCK_BYTES,
};
use crate::supports::{
    GROUP_HEADER_BYTES, INDEX_BYTES, PRESET_ENTRY_BYTES, Preset, RECORD_BYTES, RECORD_MAGIC,
    RecordType, SupportRecord, Supports, TRAILER_BYTES,
};

struct Header {
    count: u32,
    off8: u32,
    f12: u32,
    f16: u32,
    off20: u32,
}

struct RawModel {
    name: String,
    translate: [f32; 3],
    index_off: u32,
    mesh_off: u32,
    mesh_size: u32,
}

#[derive(Clone, Copy)]
struct Descriptor {
    count: u64,
    table: u64,
    attr: u64,
}

impl Descriptor {
    fn is_zero(self) -> bool {
        self.count == 0 && self.table == 0 && self.attr == 0
    }
}

type Block = (Vec<Preset>, Vec<u64>);

/// Parse a `.chitubox` file. Any deviation from the layout in the format document
/// is an error; a file that parses writes back byte for byte.
///
/// # Errors
/// Returns [`ParseError`] for truncation, a wrong pointer or length, a nonzero byte
/// in a region that is zero in every ChituBox file, a non-finite number, a bad name
/// or a non-canonical preview. It never panics.
pub fn parse(data: &[u8]) -> Result<Project, ParseError> {
    let mut r = Reader::new(data);
    let hdr = read_header(&mut r)?;
    r.expect_zero("zero block", ZERO_BLOCK_BYTES)?;
    let preview = read_preview(&mut r, &hdr)?;
    r.expect_pos("model table offset", u64::from(hdr.off8))?;
    let raw = read_model_table(&mut r, hdr.count)?;
    let descs = read_descriptors(&mut r, &hdr)?;
    let blocks = read_front_blocks(&mut r, &descs, &raw)?;
    let models = read_models(&mut r, raw, blocks)?;
    r.finish()?;
    Ok(Project {
        header_f16: hdr.f16,
        preview,
        models,
    })
}

fn read_header(r: &mut Reader<'_>) -> Result<Header, ParseError> {
    let magic = r.u32("magic")?;
    if magic != MAGIC {
        return Err(err(0, ParseErrorKind::BadMagic(magic)));
    }
    let hdr = Header {
        count: r.u32("model count")?,
        off8: r.u32("model table offset")?,
        f12: r.u32("f12")?,
        f16: r.u32("f16")?,
        off20: r.u32("front block offset")?,
    };
    if u64::from(hdr.f12) != 0 && u64::from(hdr.f12) != PREVIEW_HEADER_OFFSET {
        return Err(err(
            12,
            ParseErrorKind::Invalid {
                what: "f12",
                value: u64::from(hdr.f12),
            },
        ));
    }
    Ok(hdr)
}

fn read_preview(r: &mut Reader<'_>, hdr: &Header) -> Result<Option<Preview>, ParseError> {
    if hdr.f12 == 0 {
        return Ok(None);
    }
    r.expect_pos("preview header offset", u64::from(hdr.f12))?;
    let width = r.u32("preview width")?;
    let height = r.u32("preview height")?;
    let data_off = r.u32("preview data offset")?;
    if u64::from(data_off) != PREVIEW_DATA_OFFSET {
        return Err(err(
            r.pos(),
            ParseErrorKind::Mismatch {
                what: "preview data offset",
                expected: PREVIEW_DATA_OFFSET,
                found: u64::from(data_off),
            },
        ));
    }
    let data_len = r.u32("preview data length")?;
    r.expect_zero("preview header padding", 16)?;
    let at = r.pos();
    let data = r.take("preview data", u64::from(data_len))?;
    preview::decode(width, height, data, at).map(Some)
}

fn read_model_table(r: &mut Reader<'_>, count: u32) -> Result<Vec<RawModel>, ParseError> {
    let mut table = r.sub("model table", u64::from(count) * MODEL_RECORD_BYTES)?;
    let mut out = Vec::with_capacity(usize_of(u64::from(count), "model count", 0)?);
    for _ in 0..count {
        let mut rec = table.sub("model record", MODEL_RECORD_BYTES)?;
        let at = rec.pos();
        let name = parse_name(rec.take("model name", NAME_BYTES as u64)?, at)?;
        let translate = rec.f32x3("model translate")?;
        let index_off = rec.u32("support index offset")?;
        let mesh_off = rec.u32("mesh offset")?;
        let mesh_size = rec.u32("mesh size")?;
        rec.expect_zero("model record reserved", 400)?;
        out.push(RawModel {
            name,
            translate,
            index_off,
            mesh_off,
            mesh_size,
        });
    }
    Ok(out)
}

/// The name is UTF-8 up to the first NUL; the rest of the 256 bytes is zero.
fn parse_name(field: &[u8], at: u64) -> Result<String, ParseError> {
    let bad = |what| err(at, ParseErrorKind::Invalid { what, value: 0 });
    let nul = field
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| bad("model name without NUL"))?;
    if field[nul..].iter().any(|&b| b != 0) {
        return Err(bad("model name padding"));
    }
    String::from_utf8(field[..nul].to_vec()).map_err(|_| bad("model name UTF-8"))
}

fn read_descriptors(r: &mut Reader<'_>, hdr: &Header) -> Result<Vec<Descriptor>, ParseError> {
    if hdr.count == 0 {
        if hdr.off20 != 0 {
            return Err(err(
                20,
                ParseErrorKind::Mismatch {
                    what: "front block offset (empty file)",
                    expected: 0,
                    found: u64::from(hdr.off20),
                },
            ));
        }
        return Ok(Vec::new());
    }
    r.expect_pos("front block offset", u64::from(hdr.off20))?;
    let mut raw = r.sub("descriptors", u64::from(hdr.count) * DESCRIPTOR_BYTES)?;
    let mut out = Vec::with_capacity(usize_of(u64::from(hdr.count), "model count", 0)?);
    for _ in 0..hdr.count {
        out.push(Descriptor {
            count: raw.u64("preset count")?,
            table: raw.u64("preset table pointer")?,
            attr: raw.u64("attr pointer")?,
        });
    }
    Ok(out)
}

fn read_front_blocks(
    r: &mut Reader<'_>,
    descs: &[Descriptor],
    raw: &[RawModel],
) -> Result<Vec<Option<Block>>, ParseError> {
    let front_end = raw.first().map_or(0, |m| u64::from(m.mesh_off));
    let mut out = Vec::with_capacity(descs.len());
    for (i, d) in descs.iter().enumerate() {
        if d.is_zero() {
            out.push(None);
            continue;
        }
        // The attr array has no length field: it ends where the next block starts,
        // or where the first mesh starts.
        let attr_end = descs[i + 1..]
            .iter()
            .find(|n| !n.is_zero())
            .map_or(front_end, |n| n.table);
        out.push(Some(read_block(r, *d, attr_end)?));
    }
    Ok(out)
}

fn read_block(r: &mut Reader<'_>, d: Descriptor, attr_end: u64) -> Result<Block, ParseError> {
    r.expect_pos("preset table pointer", d.table)?;
    let overflow = || {
        err(
            d.table,
            ParseErrorKind::Overflow {
                what: "preset table",
            },
        )
    };
    let mut table = r.sub(
        "preset table",
        d.count
            .checked_mul(PRESET_ENTRY_BYTES)
            .ok_or_else(overflow)?,
    )?;
    let mut entries = Vec::with_capacity(usize_of(d.count, "preset count", d.table)?);
    for _ in 0..d.count {
        entries.push((
            table.u64("preset id")?,
            table.u64("preset name pointer")?,
            table.u64("preset name length")?,
        ));
    }
    let mut presets = Vec::with_capacity(entries.len());
    for (id, ptr, len) in entries {
        r.expect_pos("preset name pointer", ptr)?;
        let at = r.pos();
        let bytes = r.take("preset name", len)?;
        let name = String::from_utf8(bytes.to_vec()).map_err(|_| {
            err(
                at,
                ParseErrorKind::Invalid {
                    what: "preset name UTF-8",
                    value: id,
                },
            )
        })?;
        presets.push(Preset { id, name });
    }
    r.expect_pos("attr pointer", d.attr)?;
    let len = attr_end.checked_sub(r.pos()).ok_or_else(|| {
        err(
            r.pos(),
            ParseErrorKind::Mismatch {
                what: "attr array end",
                expected: r.pos(),
                found: attr_end,
            },
        )
    })?;
    if len % 8 != 0 {
        return Err(err(
            r.pos(),
            ParseErrorKind::Invalid {
                what: "attr array length",
                value: len,
            },
        ));
    }
    let bytes = r.take("attr array", len)?;
    let attrs = bytes
        .chunks_exact(8)
        .map(|c| u64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]))
        .collect();
    Ok((presets, attrs))
}

fn read_models(
    r: &mut Reader<'_>,
    raw: Vec<RawModel>,
    blocks: Vec<Option<Block>>,
) -> Result<Vec<Model>, ParseError> {
    let mut out = Vec::with_capacity(raw.len());
    for (m, block) in raw.into_iter().zip(blocks) {
        r.expect_pos("model mesh offset", u64::from(m.mesh_off))?;
        let at = r.pos();
        let bytes = r.take("model mesh", u64::from(m.mesh_size))?;
        let tris = mesh::decode(bytes, at, "model mesh")?;
        let supports = match (m.index_off, block) {
            (0, None) => None,
            (off, Some(b)) if off != 0 => Some(read_supports(r, u64::from(off), b)?),
            (off, _) => {
                return Err(err(
                    r.pos(),
                    ParseErrorKind::Invalid {
                        what: "support index offset against descriptor",
                        value: u64::from(off),
                    },
                ));
            }
        };
        out.push(Model {
            name: m.name,
            translate: m.translate,
            mesh: tris,
            supports,
        });
    }
    Ok(out)
}

fn read_supports(r: &mut Reader<'_>, idx_off: u64, block: Block) -> Result<Supports, ParseError> {
    let mut idx = r.peek_at("support index", idx_off, INDEX_BYTES)?;
    let n = u64::from(idx.u32("record count")?);
    let records_off = u64::from(idx.u32("records offset")?);
    let blob_off = u64::from(idx.u32("support mesh offset")?);
    let blob_size = u64::from(idx.u32("support mesh size")?);
    r.expect_pos("support mesh offset", blob_off)?;
    let blob_start = r.pos();
    let mismatch = |what, expected, found| {
        err(
            idx_off,
            ParseErrorKind::Mismatch {
                what,
                expected,
                found,
            },
        )
    };
    let end = blob_start.checked_add(blob_size).ok_or_else(|| {
        err(
            idx_off,
            ParseErrorKind::Overflow {
                what: "support mesh size",
            },
        )
    })?;
    if end != idx_off {
        return Err(mismatch("support index offset", end, idx_off));
    }
    let blob = r.take("support mesh blob", blob_size)?;
    r.take("support index", INDEX_BYTES)?;
    r.expect_zero("group header", GROUP_HEADER_BYTES)?;
    r.expect_pos("records offset", records_off)?;
    let size = n.checked_mul(RECORD_BYTES).ok_or_else(|| {
        err(
            idx_off,
            ParseErrorKind::Overflow {
                what: "record count",
            },
        )
    })?;
    let recs = r.sub("support records", size)?;
    r.expect_zero("support trailer", TRAILER_BYTES)?;
    let (lead, records) = read_records(recs, blob, blob_start, n)?;
    let (presets, attrs) = block;
    Ok(Supports {
        presets,
        attrs,
        unreferenced_blob: lead.to_vec(),
        records,
    })
}

/// Records and their meshes. Meshes follow each other in record order; the bytes of
/// the blob before the first mesh (empty in a clean save) are returned as `lead`.
fn read_records<'a>(
    mut recs: Reader<'_>,
    blob: &'a [u8],
    blob_start: u64,
    n: u64,
) -> Result<(&'a [u8], Vec<SupportRecord>), ParseError> {
    let mut out = Vec::with_capacity(usize_of(n, "record count", blob_start)?);
    let blob_len = blob.len() as u64;
    let mut cursor = 0u64;
    let mut lead = 0u64;
    for k in 0..n {
        let mut rec = recs.sub("support record", RECORD_BYTES)?;
        let at = rec.pos();
        let magic = rec.u32("record magic")?;
        if magic != RECORD_MAGIC {
            return Err(err(
                at,
                ParseErrorKind::Mismatch {
                    what: "record magic",
                    expected: u64::from(RECORD_MAGIC),
                    found: u64::from(magic),
                },
            ));
        }
        let mut record = read_record_fields(&mut rec)?;
        let mesh_off = u64::from(rec.u32("record mesh offset")?);
        let mesh_len = u64::from(rec.u32("record mesh length")?);
        for w in &mut record.tail {
            *w = rec.u32("record tail")?;
        }
        let rel = mesh_off.checked_sub(blob_start).ok_or_else(|| {
            err(
                at,
                ParseErrorKind::Invalid {
                    what: "record mesh offset",
                    value: mesh_off,
                },
            )
        })?;
        if k == 0 {
            lead = rel;
            cursor = rel;
        }
        if rel != cursor {
            return Err(err(
                at,
                ParseErrorKind::Mismatch {
                    what: "record mesh offset",
                    expected: blob_start.saturating_add(cursor),
                    found: mesh_off,
                },
            ));
        }
        let end = rel
            .checked_add(mesh_len)
            .filter(|&e| e <= blob_len)
            .ok_or_else(|| {
                err(
                    at,
                    ParseErrorKind::Invalid {
                        what: "record mesh length",
                        value: mesh_len,
                    },
                )
            })?;
        let bytes = &blob[usize_of(rel, "record mesh", at)?..usize_of(end, "record mesh", at)?];
        record.mesh = mesh::decode(bytes, mesh_off, "record mesh")?;
        cursor = end;
        out.push(record);
    }
    if n == 0 {
        lead = blob_len;
        cursor = blob_len;
    }
    if cursor != blob_len {
        return Err(err(
            blob_start,
            ParseErrorKind::Mismatch {
                what: "support mesh blob end",
                expected: blob_start + cursor,
                found: blob_start + blob_len,
            },
        ));
    }
    Ok((&blob[..usize_of(lead, "blob", blob_start)?], out))
}

fn read_record_fields(rec: &mut Reader<'_>) -> Result<SupportRecord, ParseError> {
    Ok(SupportRecord {
        record_type: RecordType(rec.u32("record type")?),
        start: rec.f32x3("record start")?,
        end: rec.f32x3("record end")?,
        r1: rec.f32("record r1")?,
        r2: rec.f32("record r2")?,
        tail: [0; 6],
        mesh: Vec::new(),
    })
}
