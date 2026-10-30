//! The checker's own .chitubox reader, written from docs/format/chitubox-v1.4.0.md.
//!
//! It follows the pointer chain (header, model table, support index, support records) and
//! records which bytes each part occupies (`Region`). Nothing is skipped: the regions of a clean
//! file tile it exactly, and anything that does not fit the format becomes an anomaly in the
//! result. A file that cannot be followed at all (truncated, wrong magic) is an error.

use crate::bytes::{
    ReadError, Res, all_zero, f32_at, f32_batch, f32x3_at, fail, slice, u32_at, u64_at, wide,
};

pub const MAGIC: u32 = 0xAB23_1243;
pub const RECORD_MAGIC: u32 = 0xEA34_2389;
const ZERO_BLOCK_END: u64 = 420;
const PREVIEW_DATA_OFF: u64 = 452;
const PREVIEW_HEADER_LEN: u64 = 32;
const MODEL_RECORD_LEN: u64 = 680;
const DESCRIPTOR_LEN: u64 = 24;
const PRESET_ENTRY_LEN: u64 = 24;
const TRIANGLE_LEN: u64 = 36;
const INDEX_LEN: u64 = 16;
const GROUP_HEADER_LEN: u64 = 400;
const RECORD_LEN: u64 = 72;
const TRAILER_LEN: u64 = 20;

/// Region names, shared with the sequential walk in `layout`.
pub mod kind {
    pub const HEADER: &str = "header";
    pub const ZERO_BLOCK: &str = "zero block";
    pub const PREVIEW_HEADER: &str = "preview header";
    pub const PREVIEW_DATA: &str = "preview data";
    pub const MODEL_TABLE: &str = "model table";
    pub const DESCRIPTORS: &str = "descriptors";
    pub const PRESET_TABLE: &str = "preset table";
    pub const PRESET_NAMES: &str = "preset names";
    pub const ATTR_ARRAY: &str = "attr array";
    pub const MESH: &str = "mesh";
    pub const SUPPORT_BLOB: &str = "support blob";
    pub const SUPPORT_INDEX: &str = "support index";
    pub const GROUP_HEADER: &str = "group header";
    pub const RECORDS: &str = "support records";
    pub const TRAILER: &str = "trailer";
}

/// A run of file bytes that belongs to one part of the format.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Region {
    pub start: u64,
    pub end: u64,
    pub kind: &'static str,
    pub model: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub magic: u32,
    pub model_count: u32,
    pub off8: u32,
    pub f12: u32,
    pub f16: u32,
    pub off20: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub width: u32,
    pub height: u32,
    pub data_off: u32,
    pub data_len: u32,
    /// Pixels the run-length data decodes to, if it decodes cleanly.
    pub pixels: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preset {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Front {
    pub preset_count: u64,
    pub table_ptr: u64,
    pub attr_ptr: u64,
    pub presets: Vec<Preset>,
    pub attrs: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportIndex {
    pub index_off: u64,
    pub n_records: u32,
    pub records_off: u32,
    pub blob_off: u32,
    pub blob_size: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SupportRecord {
    pub magic: u32,
    pub kind: u32,
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub r1: f32,
    pub r2: f32,
    pub mesh_off: u32,
    pub mesh_len: u32,
    pub tail: [u32; 6],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Support {
    pub index: SupportIndex,
    pub records: Vec<SupportRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub name: String,
    pub translate: [f32; 3],
    pub support_off: u32,
    pub mesh_off: u32,
    pub mesh_size: u32,
    pub front: Option<Front>,
    pub support: Option<Support>,
}

impl Model {
    #[must_use]
    pub fn triangles(&self) -> u64 {
        u64::from(self.mesh_size) / TRIANGLE_LEN
    }
}

/// Everything the reader found in one file.
#[derive(Debug, Clone, PartialEq)]
pub struct ChituFile {
    pub len: u64,
    pub header: Header,
    pub preview: Option<Preview>,
    pub models: Vec<Model>,
    /// Sorted by start.
    pub regions: Vec<Region>,
    /// True if the regions cover every byte exactly once.
    pub tiles: bool,
    /// Everything that does not match the format reference. Empty for a clean native file.
    pub anomalies: Vec<String>,
}

impl ChituFile {
    /// One model's mesh as 9 f32 per triangle, read in one pass from `buf` (the same bytes that
    /// were parsed).
    pub fn mesh(&self, buf: &[u8], model: usize) -> Res<Vec<f32>> {
        let Some(m) = self.models.get(model) else {
            return fail(format!("no model {model}"));
        };
        let bytes = slice(buf, u64::from(m.mesh_off), u64::from(m.mesh_size), "mesh")?;
        Ok(f32_batch(bytes))
    }
}

/// Reader state: the regions and anomalies found so far.
struct Cx<'a> {
    buf: &'a [u8],
    regions: Vec<Region>,
    anomalies: Vec<String>,
}

impl Cx<'_> {
    fn region(&mut self, kind: &'static str, model: Option<usize>, start: u64, len: u64) {
        if len > 0 {
            self.regions.push(Region {
                start,
                end: start.saturating_add(len),
                kind,
                model,
            });
        }
    }

    fn note(&mut self, msg: impl Into<String>) {
        self.anomalies.push(msg.into());
    }

    /// Note an anomaly unless `ok`.
    fn expect(&mut self, ok: bool, msg: impl FnOnce() -> String) {
        if !ok {
            self.anomalies.push(msg());
        }
    }
}

/// Read a whole .chitubox file.
pub fn parse(buf: &[u8]) -> Res<ChituFile> {
    let mut cx = Cx {
        buf,
        regions: Vec::new(),
        anomalies: Vec::new(),
    };
    let header = read_header(&mut cx)?;
    let preview = read_preview(&mut cx, &header)?;
    let mut models = read_model_table(&mut cx, &header)?;
    for (i, m) in models.iter_mut().enumerate() {
        read_model_data(&mut cx, i, m)?;
    }
    read_front(&mut cx, &header, &mut models)?;
    check_offsets(&mut cx, &header, preview.as_ref());
    let tiles = check_tiling(&mut cx);
    let mut regions = cx.regions;
    regions.sort();
    Ok(ChituFile {
        len: wide(buf.len()),
        header,
        preview,
        models,
        regions,
        tiles,
        anomalies: cx.anomalies,
    })
}

fn read_header(cx: &mut Cx) -> Res<Header> {
    let b = cx.buf;
    let header = Header {
        magic: u32_at(b, 0, "header magic")?,
        model_count: u32_at(b, 4, "model count")?,
        off8: u32_at(b, 8, "model table offset")?,
        f12: u32_at(b, 12, "header f12")?,
        f16: u32_at(b, 16, "header f16")?,
        off20: u32_at(b, 20, "front block offset")?,
    };
    if header.magic != MAGIC {
        return fail(format!(
            "not a .chitubox file: magic is {:#010x}, expected {MAGIC:#010x}",
            header.magic
        ));
    }
    cx.region(kind::HEADER, None, 0, 24);
    let zeros = slice(b, 24, ZERO_BLOCK_END - 24, "zero block")?;
    cx.expect(all_zero(zeros), || {
        "zero block [24, 420) is not all zero".into()
    });
    cx.region(kind::ZERO_BLOCK, None, 24, ZERO_BLOCK_END - 24);
    Ok(header)
}

/// Number of pixels a preview's run-length data decodes to.
fn preview_pixels(data: &[u8]) -> Res<u64> {
    if !data.len().is_multiple_of(2) {
        return fail("preview data has an odd byte count");
    }
    let words: Vec<u16> = data
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let (mut i, mut pixels) = (0usize, 0u64);
    while i < words.len() {
        if words[i] & 0x0020 == 0 {
            pixels += 1;
            i += 1;
            continue;
        }
        let Some(run_word) = words.get(i + 1) else {
            return fail("preview ends between a repeat word and its count");
        };
        if run_word & 0xF000 != 0x3000 {
            return fail(format!(
                "preview repeat count word {run_word:#06x} is not 0x3xxx"
            ));
        }
        pixels += u64::from(run_word & 0x0FFF) + 1;
        i += 2;
    }
    Ok(pixels)
}

fn read_preview(cx: &mut Cx, header: &Header) -> Res<Option<Preview>> {
    if header.f12 == 0 {
        return Ok(None);
    }
    let b = cx.buf;
    let at = ZERO_BLOCK_END;
    let (width, height) = (
        u32_at(b, at, "preview width")?,
        u32_at(b, at + 4, "preview height")?,
    );
    let (data_off, data_len) = (
        u32_at(b, at + 8, "preview data offset")?,
        u32_at(b, at + 12, "preview data length")?,
    );
    let pad = slice(b, at + 16, 16, "preview header padding")?;
    cx.expect(all_zero(pad), || {
        "preview header words 4..8 are not zero".into()
    });
    cx.region(kind::PREVIEW_HEADER, None, at, PREVIEW_HEADER_LEN);
    let data = slice(b, u64::from(data_off), u64::from(data_len), "preview data")?;
    cx.region(
        kind::PREVIEW_DATA,
        None,
        u64::from(data_off),
        u64::from(data_len),
    );
    cx.expect(u64::from(data_off) == PREVIEW_DATA_OFF, || {
        format!("preview data offset is {data_off}, expected {PREVIEW_DATA_OFF}")
    });
    let pixels = match preview_pixels(data) {
        Ok(n) => Some(n),
        Err(ReadError(e)) => {
            cx.note(e);
            None
        }
    };
    let want = u64::from(width) * u64::from(height);
    cx.expect(pixels.is_none_or(|n| n == want), || {
        format!(
            "preview decodes to {} pixels, header says {width}x{height} = {want}",
            pixels.unwrap_or(0)
        )
    });
    Ok(Some(Preview {
        width,
        height,
        data_off,
        data_len,
        pixels,
    }))
}

fn read_model_table(cx: &mut Cx, header: &Header) -> Res<Vec<Model>> {
    let n = u64::from(header.model_count);
    let Some(table_len) = n.checked_mul(MODEL_RECORD_LEN) else {
        return fail("model table length overflows");
    };
    let start = u64::from(header.off8);
    let table = slice(cx.buf, start, table_len, "model table")?;
    cx.region(kind::MODEL_TABLE, None, start, table_len);
    let mut models = Vec::new();
    for (i, rec) in table.chunks_exact(680).enumerate() {
        let name_bytes = &rec[..256];
        let name_end = name_bytes.iter().position(|c| *c == 0).unwrap_or(256);
        let name = String::from_utf8_lossy(&name_bytes[..name_end]).into_owned();
        cx.expect(all_zero(&name_bytes[name_end..]), || {
            format!("model {i}: bytes after the name's NUL are not zero")
        });
        cx.expect(all_zero(&rec[280..]), || {
            format!("model {i}: record bytes 280..680 are not zero")
        });
        let model = Model {
            name,
            translate: f32x3_at(rec, 256, "model translate")?,
            support_off: u32_at(rec, 268, "support index offset")?,
            mesh_off: u32_at(rec, 272, "mesh offset")?,
            mesh_size: u32_at(rec, 276, "mesh size")?,
            front: None,
            support: None,
        };
        cx.expect(
            u64::from(model.mesh_size).is_multiple_of(TRIANGLE_LEN),
            || {
                format!(
                    "model {i}: mesh size {} is not a multiple of 36",
                    model.mesh_size
                )
            },
        );
        models.push(model);
    }
    Ok(models)
}

/// A model's mesh and, if it has them, its support section.
fn read_model_data(cx: &mut Cx, i: usize, m: &mut Model) -> Res<()> {
    let (mesh_off, mesh_size) = (u64::from(m.mesh_off), u64::from(m.mesh_size));
    slice(cx.buf, mesh_off, mesh_size, "mesh")?;
    cx.region(kind::MESH, Some(i), mesh_off, mesh_size);
    if m.support_off != 0 {
        m.support = Some(read_support(
            cx,
            i,
            u64::from(m.support_off),
            mesh_off + mesh_size,
        )?);
    }
    Ok(())
}

fn read_support_record(r: &[u8]) -> Res<SupportRecord> {
    let mut tail = [0u32; 6];
    for (k, t) in tail.iter_mut().enumerate() {
        *t = u32_at(r, 48 + 4 * wide(k), "record tail word")?;
    }
    Ok(SupportRecord {
        magic: u32_at(r, 0, "record magic")?,
        kind: u32_at(r, 4, "record type")?,
        start: f32x3_at(r, 8, "record start")?,
        end: f32x3_at(r, 20, "record end")?,
        r1: f32_at(r, 32, "record r1")?,
        r2: f32_at(r, 36, "record r2")?,
        mesh_off: u32_at(r, 40, "record mesh offset")?,
        mesh_len: u32_at(r, 44, "record mesh length")?,
        tail,
    })
}

fn read_support(cx: &mut Cx, i: usize, index_off: u64, mesh_end: u64) -> Res<Support> {
    let b = cx.buf;
    let index = SupportIndex {
        index_off,
        n_records: u32_at(b, index_off, "support index record count")?,
        records_off: u32_at(b, index_off + 4, "support index records offset")?,
        blob_off: u32_at(b, index_off + 8, "support index blob offset")?,
        blob_size: u32_at(b, index_off + 12, "support index blob size")?,
    };
    let (blob_off, blob_size) = (u64::from(index.blob_off), u64::from(index.blob_size));
    slice(b, blob_off, blob_size, "support blob")?;
    cx.region(kind::SUPPORT_BLOB, Some(i), blob_off, blob_size);
    cx.region(kind::SUPPORT_INDEX, Some(i), index_off, INDEX_LEN);
    let group = slice(b, index_off + INDEX_LEN, GROUP_HEADER_LEN, "group header")?;
    cx.expect(all_zero(group), || {
        format!("model {i}: group header is not zero")
    });
    cx.region(
        kind::GROUP_HEADER,
        Some(i),
        index_off + INDEX_LEN,
        GROUP_HEADER_LEN,
    );
    let rec_start = u64::from(index.records_off);
    let rec_len = u64::from(index.n_records) * RECORD_LEN;
    let raw = slice(b, rec_start, rec_len, "support records")?;
    cx.region(kind::RECORDS, Some(i), rec_start, rec_len);
    let trailer = slice(b, rec_start + rec_len, TRAILER_LEN, "support trailer")?;
    cx.expect(all_zero(trailer), || {
        format!("model {i}: trailer is not zero")
    });
    cx.region(kind::TRAILER, Some(i), rec_start + rec_len, TRAILER_LEN);
    let records = raw
        .chunks_exact(72)
        .map(read_support_record)
        .collect::<Res<Vec<_>>>()?;
    check_support(cx, i, &index, &records, mesh_end);
    Ok(Support { index, records })
}

/// The relations between a support index and its records (format reference sections 9 and 13).
fn check_support(cx: &mut Cx, i: usize, ix: &SupportIndex, recs: &[SupportRecord], mesh_end: u64) {
    cx.expect(u64::from(ix.blob_off) == mesh_end, || {
        format!(
            "model {i}: support blob starts at {}, the mesh ends at {mesh_end}",
            ix.blob_off
        )
    });
    cx.expect(
        u64::from(ix.blob_off) + u64::from(ix.blob_size) == ix.index_off,
        || format!("model {i}: support blob does not end where the support index starts"),
    );
    cx.expect(
        u64::from(ix.records_off) == ix.index_off + INDEX_LEN + GROUP_HEADER_LEN,
        || {
            format!(
                "model {i}: records start at {}, expected index + 416",
                ix.records_off
            )
        },
    );
    let bad_magic = recs.iter().filter(|r| r.magic != RECORD_MAGIC).count();
    cx.expect(bad_magic == 0, || {
        format!("model {i}: {bad_magic} records have a wrong magic")
    });
    let mut next = u64::from(ix.blob_off);
    let mut first_bad = None;
    for (k, r) in recs.iter().enumerate() {
        if u64::from(r.mesh_off) != next && first_bad.is_none() {
            first_bad = Some(k);
        }
        next = u64::from(r.mesh_off) + u64::from(r.mesh_len);
    }
    cx.expect(first_bad.is_none(), || {
        format!(
            "model {i}: record meshes are not contiguous in record order (first at record {})",
            first_bad.unwrap_or(0)
        )
    });
    let sum: u64 = recs.iter().map(|r| u64::from(r.mesh_len)).sum();
    cx.expect(sum == u64::from(ix.blob_size), || {
        format!(
            "model {i}: record mesh lengths add up to {sum}, the blob is {} bytes",
            ix.blob_size
        )
    });
}

fn read_front(cx: &mut Cx, header: &Header, models: &mut [Model]) -> Res<()> {
    if models.is_empty() {
        return Ok(());
    }
    let n = wide(models.len());
    let start = u64::from(header.off20);
    let raw = slice(cx.buf, start, n * DESCRIPTOR_LEN, "descriptors")?;
    cx.region(kind::DESCRIPTORS, None, start, n * DESCRIPTOR_LEN);
    for (i, d) in raw.chunks_exact(24).enumerate() {
        let (count, table_ptr, attr_ptr) = (
            u64_at(d, 0, "preset count")?,
            u64_at(d, 8, "preset table pointer")?,
            u64_at(d, 16, "attr array pointer")?,
        );
        let supported = models[i].support.is_some();
        if all_zero(d) {
            cx.expect(!supported, || {
                format!("model {i}: has supports but a zero descriptor")
            });
            continue;
        }
        cx.expect(supported, || {
            format!("model {i}: has a descriptor but no supports")
        });
        let n_attr = models[i]
            .support
            .as_ref()
            .map_or(0, |s| u64::from(s.index.n_records));
        let front = read_preset_block(cx, i, (count, table_ptr, attr_ptr), n_attr)?;
        models[i].front = Some(front);
    }
    Ok(())
}

fn read_preset_block(cx: &mut Cx, i: usize, d: (u64, u64, u64), n_attr: u64) -> Res<Front> {
    let (count, table_ptr, attr_ptr) = d;
    let Some(table_len) = count.checked_mul(PRESET_ENTRY_LEN) else {
        return fail(format!("model {i}: preset table length overflows"));
    };
    let table = slice(cx.buf, table_ptr, table_len, "preset table")?;
    cx.region(kind::PRESET_TABLE, Some(i), table_ptr, table_len);
    let mut presets = Vec::new();
    let (mut names_len, mut sequential) = (0u64, true);
    for e in table.chunks_exact(24) {
        let (id, name_ptr, name_len) = (
            u64_at(e, 0, "preset id")?,
            u64_at(e, 8, "preset name pointer")?,
            u64_at(e, 16, "preset name length")?,
        );
        let name = slice(cx.buf, name_ptr, name_len, "preset name")?;
        sequential &= name_ptr == table_ptr + table_len + names_len;
        names_len = names_len.saturating_add(name_len);
        presets.push(Preset {
            id,
            name: String::from_utf8_lossy(name).into_owned(),
        });
    }
    cx.expect(sequential, || {
        format!("model {i}: preset names do not follow the table in order")
    });
    let names_start = table_ptr + table_len;
    cx.region(kind::PRESET_NAMES, Some(i), names_start, names_len);
    cx.expect(attr_ptr == names_start + names_len, || {
        format!("model {i}: attr array does not start right after the preset names")
    });
    let raw = slice(cx.buf, attr_ptr, n_attr * 8, "attr array")?;
    cx.region(kind::ATTR_ARRAY, Some(i), attr_ptr, n_attr * 8);
    let attrs = raw
        .chunks_exact(8)
        .map(|c| u64_at(c, 0, "attr"))
        .collect::<Res<Vec<_>>>()?;
    Ok(Front {
        preset_count: count,
        table_ptr,
        attr_ptr,
        presets,
        attrs,
    })
}

/// The header's own offsets against the layout they should describe.
fn check_offsets(cx: &mut Cx, h: &Header, preview: Option<&Preview>) {
    cx.expect(h.f12 == 0 || h.f12 == 420, || {
        format!("header f12 is {}, expected 0 or 420", h.f12)
    });
    cx.expect(h.f12 != 0 || h.model_count == 0, || {
        "models without a preview".into()
    });
    cx.expect(h.f16 == 412, || {
        format!("header f16 is {}, expected 412", h.f16)
    });
    let table = preview.map_or(ZERO_BLOCK_END, |p| PREVIEW_DATA_OFF + u64::from(p.data_len));
    cx.expect(u64::from(h.off8) == table, || {
        format!("model table offset is {}, expected {table}", h.off8)
    });
    let front = if h.model_count == 0 {
        0
    } else {
        table + MODEL_RECORD_LEN * u64::from(h.model_count)
    };
    cx.expect(u64::from(h.off20) == front, || {
        format!("front block offset is {}, expected {front}", h.off20)
    });
}

/// Check that the regions cover the file exactly once. Returns true if they do.
fn check_tiling(cx: &mut Cx) -> bool {
    let mut sorted = cx.regions.clone();
    sorted.sort();
    let mut cursor = 0u64;
    let mut found = Vec::new();
    for r in &sorted {
        let who = |m: Option<usize>| m.map_or(String::new(), |k| format!(" (model {k})"));
        if r.start > cursor {
            found.push(format!(
                "gap [{cursor}, {}) before {}{}",
                r.start,
                r.kind,
                who(r.model)
            ));
        } else if r.start < cursor {
            found.push(format!(
                "{}{} starts at {}, inside the region before it (ends {cursor})",
                r.kind,
                who(r.model),
                r.start
            ));
        }
        cursor = cursor.max(r.end);
    }
    let len = wide(cx.buf.len());
    if cursor < len {
        found.push(format!("{} bytes after the last region", len - cursor));
    }
    let tiles = found.is_empty();
    for f in found {
        cx.note(f);
    }
    tiles
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_run_length_counts_pixels() {
        // one plain pixel, then a run word (bit 5 set) with count word 0x3000|(4-1) = 4 pixels
        let words: [u16; 3] = [0x0841, 0x0861, 0x3003];
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        assert_eq!(preview_pixels(&bytes), Ok(5));
        assert!(preview_pixels(&bytes[..5]).is_err());
        assert!(
            preview_pixels(&bytes[..4]).is_err(),
            "repeat word without its count"
        );
    }

    #[test]
    fn rejects_wrong_magic_and_truncation() {
        assert!(parse(&[0u8; 8]).is_err());
        let mut b = vec![0u8; 420];
        b[..4].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        assert!(parse(&b).unwrap_err().0.contains("magic"));
    }

    #[test]
    fn empty_project_parses_clean() {
        let mut b = vec![0u8; 420];
        b[..4].copy_from_slice(&MAGIC.to_le_bytes());
        b[8..12].copy_from_slice(&420u32.to_le_bytes());
        b[16..20].copy_from_slice(&412u32.to_le_bytes());
        let f = parse(&b).expect("parses");
        assert!(f.tiles && f.anomalies.is_empty(), "{:?}", f.anomalies);
        assert!(f.models.is_empty() && f.preview.is_none());
    }

    #[test]
    fn stray_bytes_after_the_last_region_are_an_anomaly() {
        let mut b = vec![0u8; 424];
        b[..4].copy_from_slice(&MAGIC.to_le_bytes());
        b[8..12].copy_from_slice(&420u32.to_le_bytes());
        b[16..20].copy_from_slice(&412u32.to_le_bytes());
        let f = parse(&b).expect("parses");
        assert!(!f.tiles);
        assert!(
            f.anomalies
                .iter()
                .any(|a| a.contains("after the last region"))
        );
    }
}
