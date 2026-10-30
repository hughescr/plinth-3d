//! A second, different way to find the parts of a .chitubox file: the sequential walk.
//!
//! The reader in `chitubox` follows pointers. This walk uses only the counts and lengths the
//! format reference says are stored (model count, preview length, descriptor preset counts,
//! preset name lengths, mesh sizes, support index counts) and places every part directly after
//! the one before it, as the format reference's layout table says. In a clean file both methods
//! give the same regions; any difference is a pointer that does not match the layout.

use crate::bytes::{Res, slice, u32_at, u64_at};
use crate::chitubox::{Region, kind};

struct Walk {
    regions: Vec<Region>,
    cursor: u64,
}

impl Walk {
    /// Place the next part directly after the previous one.
    fn take(&mut self, kind: &'static str, model: Option<usize>, len: u64) {
        if len > 0 {
            let end = self.cursor.saturating_add(len);
            self.regions.push(Region {
                start: self.cursor,
                end,
                kind,
                model,
            });
        }
        self.cursor = self.cursor.saturating_add(len);
    }
}

/// The regions of the file in `buf`, found by walking it front to back.
pub fn walk(buf: &[u8]) -> Res<Vec<Region>> {
    let count = u64::from(u32_at(buf, 4, "model count")?);
    let has_preview = u32_at(buf, 12, "header f12")? != 0;
    let mut w = Walk {
        regions: Vec::new(),
        cursor: 0,
    };
    w.take(kind::HEADER, None, 24);
    w.take(kind::ZERO_BLOCK, None, 396);
    if has_preview {
        let data_len = u32_at(buf, 432, "preview data length")?;
        w.take(kind::PREVIEW_HEADER, None, 32);
        w.take(kind::PREVIEW_DATA, None, u64::from(data_len));
    }
    let table = w.cursor;
    w.take(kind::MODEL_TABLE, None, count.saturating_mul(680));
    let descriptors = w.cursor;
    if count > 0 {
        w.take(kind::DESCRIPTORS, None, count.saturating_mul(24));
    }
    for i in 0..count {
        let rec = table + 680 * i;
        let support_off = u64::from(u32_at(buf, rec + 268, "support index offset")?);
        walk_preset_block(&mut w, buf, i, descriptors + 24 * i, support_off)?;
    }
    for i in 0..count {
        let rec = table + 680 * i;
        walk_model_data(&mut w, buf, i, rec)?;
    }
    Ok(w.regions)
}

/// The preset table, names and attr array of one supported model (nothing for a zero descriptor).
fn walk_preset_block(w: &mut Walk, buf: &[u8], i: u64, desc: u64, support_off: u64) -> Res<()> {
    let presets = u64_at(buf, desc, "preset count")?;
    if presets == 0 {
        return Ok(());
    }
    let model = Some(usize::try_from(i).unwrap_or(usize::MAX));
    let table = slice(buf, w.cursor, presets.saturating_mul(24), "preset table")?;
    let mut names = 0u64;
    for e in table.chunks_exact(24) {
        names = names.saturating_add(u64_at(e, 16, "preset name length")?);
    }
    let n_records = if support_off == 0 {
        0
    } else {
        u64::from(u32_at(buf, support_off, "support index record count")?)
    };
    w.take(kind::PRESET_TABLE, model, presets.saturating_mul(24));
    w.take(kind::PRESET_NAMES, model, names);
    w.take(kind::ATTR_ARRAY, model, n_records * 8);
    Ok(())
}

/// A model's mesh and support section, placed after everything before them.
fn walk_model_data(w: &mut Walk, buf: &[u8], i: u64, rec: u64) -> Res<()> {
    let model = Some(usize::try_from(i).unwrap_or(usize::MAX));
    let mesh_size = u64::from(u32_at(buf, rec + 276, "mesh size")?);
    let support_off = u64::from(u32_at(buf, rec + 268, "support index offset")?);
    w.take(kind::MESH, model, mesh_size);
    if support_off == 0 {
        return Ok(());
    }
    let n_records = u64::from(u32_at(buf, support_off, "support index record count")?);
    let blob_size = u64::from(u32_at(buf, support_off + 12, "support blob size")?);
    w.take(kind::SUPPORT_BLOB, model, blob_size);
    w.take(kind::SUPPORT_INDEX, model, 16);
    w.take(kind::GROUP_HEADER, model, 400);
    w.take(kind::RECORDS, model, n_records * 72);
    w.take(kind::TRAILER, model, 20);
    Ok(())
}

fn describe(r: &Region) -> String {
    let who = r.model.map_or(String::new(), |m| format!(" of model {m}"));
    format!("{}{who} [{}, {})", r.kind, r.start, r.end)
}

/// Differences between two region lists (both sorted). Empty if they are the same.
#[must_use]
pub fn diff(reader: &[Region], walked: &[Region]) -> Vec<String> {
    let only = |a: &[Region], b: &[Region], tag: &str| -> Vec<String> {
        a.iter()
            .filter(|r| !b.contains(r))
            .map(|r| format!("{tag} only: {}", describe(r)))
            .collect()
    };
    let mut out = only(reader, walked, "reader");
    out.extend(only(walked, reader, "walk"));
    out
}

/// Total bytes covered by a region list (overlaps counted twice).
#[must_use]
pub fn covered(regions: &[Region]) -> u64 {
    regions.iter().map(|r| r.end - r.start).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(kind: &'static str, start: u64, end: u64) -> Region {
        Region {
            start,
            end,
            kind,
            model: None,
        }
    }

    #[test]
    fn diff_names_the_regions_only_one_side_has() {
        let a = vec![r("header", 0, 24), r("mesh", 24, 60)];
        let b = vec![r("header", 0, 24), r("mesh", 24, 72)];
        assert!(diff(&a, &a).is_empty());
        let d = diff(&a, &b);
        assert_eq!(d.len(), 2);
        assert!(d[0].starts_with("reader only: mesh"), "{d:?}");
        assert!(d[1].starts_with("walk only: mesh"), "{d:?}");
        assert_eq!(covered(&a), 60);
    }

    #[test]
    fn walk_of_an_empty_project_is_header_and_zero_block() {
        let mut b = vec![0u8; 420];
        b[..4].copy_from_slice(&crate::chitubox::MAGIC.to_le_bytes());
        let w = walk(&b).expect("walks");
        assert_eq!(w, vec![r("header", 0, 24), r("zero block", 24, 420)]);
    }
}
