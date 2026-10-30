//! The `dump` command: a plain-text structural listing of a .chitubox or STL file. For a
//! .chitubox file it prints what the pointer-following reader found, the regions, and whether
//! the sequential walk (`layout`) found the same regions.

use std::fmt::Write;

use crate::bytes::Res;
use crate::chitubox::{self, ChituFile};
use crate::{layout, stl};

/// The listing and whether the file is clean (no anomalies, regions tile it, walk agrees).
#[derive(Debug)]
pub struct Dump {
    pub text: String,
    pub clean: bool,
}

fn put(out: &mut String, line: &str) {
    out.push_str(line);
    out.push('\n');
}

/// The neutral record list, one line per support record, in file order:
/// `record <model> <index> <type> <start x y z> <end x y z> <r1> <r2>`.
/// Numbers use Rust's shortest decimal form, which reads back to the same `f32`.
/// A comparison against another reader parses these lines; no types are shared.
fn records_text(out: &mut String, model: usize, records: &[chitubox::SupportRecord]) {
    for (k, r) in records.iter().enumerate() {
        let [sx, sy, sz] = r.start;
        let [ex, ey, ez] = r.end;
        let _ = writeln!(
            out,
            "record {model} {k} {} {sx} {sy} {sz} {ex} {ey} {ez} {} {}",
            r.kind, r.r1, r.r2
        );
    }
}

fn models_text(out: &mut String, f: &ChituFile) {
    for (i, m) in f.models.iter().enumerate() {
        let [x, y, z] = m.translate;
        let _ = writeln!(
            out,
            "model {i} \"{}\": translate=({x}, {y}, {z}) mesh=[{}, {}) triangles={}",
            m.name,
            m.mesh_off,
            u64::from(m.mesh_off) + u64::from(m.mesh_size),
            m.triangles()
        );
        if let Some(fr) = &m.front {
            let names: Vec<String> = fr
                .presets
                .iter()
                .map(|p| format!("{}={}", p.name, p.id))
                .collect();
            let _ = writeln!(
                out,
                "  presets: {}; attr entries: {}",
                names.join(" "),
                fr.attrs.len()
            );
        }
        if let Some(s) = &m.support {
            let mut kinds: std::collections::BTreeMap<u32, usize> =
                std::collections::BTreeMap::new();
            for r in &s.records {
                *kinds.entry(r.kind).or_insert(0) += 1;
            }
            let hist: Vec<String> = kinds
                .iter()
                .map(|(k, n)| format!("type {k} x{n}"))
                .collect();
            let _ = writeln!(
                out,
                "  support records: {} ({})",
                s.records.len(),
                hist.join(", ")
            );
            records_text(out, i, &s.records);
        } else {
            put(out, "  no supports");
        }
    }
}

fn chitubox_dump(path: &str, buf: &[u8]) -> Res<Dump> {
    let f = chitubox::parse(buf)?;
    let walked = layout::walk(buf)?;
    let diff = layout::diff(&f.regions, &walked);
    let h = &f.header;
    let mut out = String::new();
    let _ = writeln!(out, "file: {path} ({} bytes)", f.len);
    let _ = writeln!(
        out,
        "header: magic={:#010x} models={} off8={} f12={} f16={} off20={}",
        h.magic, h.model_count, h.off8, h.f12, h.f16, h.off20
    );
    match &f.preview {
        Some(p) => {
            let px = p.pixels.map_or("undecodable".to_owned(), |n| n.to_string());
            let _ = writeln!(
                out,
                "preview: {}x{} data_len={} pixels_decoded={px}",
                p.width, p.height, p.data_len
            );
        }
        None => put(&mut out, "preview: none"),
    }
    models_text(&mut out, &f);
    put(&mut out, "regions (start end size kind):");
    for r in &f.regions {
        let who = r.model.map_or(String::new(), |m| format!(" model {m}"));
        let _ = writeln!(
            out,
            "  {:>9} {:>9} {:>9}  {}{who}",
            r.start,
            r.end,
            r.end - r.start,
            r.kind
        );
    }
    let _ = writeln!(
        out,
        "regions tile the file: {}",
        if f.tiles { "yes" } else { "NO" }
    );
    if diff.is_empty() {
        let _ = writeln!(
            out,
            "sequential walk: same {} regions as the reader",
            walked.len()
        );
    } else {
        put(&mut out, "sequential walk: DIFFERENT from the reader:");
        for d in &diff {
            let _ = writeln!(out, "  {d}");
        }
    }
    if f.anomalies.is_empty() {
        put(&mut out, "anomalies: none");
    } else {
        put(&mut out, "anomalies:");
        for a in &f.anomalies {
            let _ = writeln!(out, "  {a}");
        }
    }
    let clean = f.anomalies.is_empty() && f.tiles && diff.is_empty();
    Ok(Dump { text: out, clean })
}

fn stl_dump(path: &str, buf: &[u8]) -> Res<Dump> {
    let m = stl::parse(buf)?;
    let mut out = String::new();
    let _ = writeln!(out, "file: {path} ({} bytes)", buf.len());
    let _ = writeln!(out, "stl: {} triangles={}", m.format.name(), m.triangles());
    if let Some((lo, hi)) = m.bounds() {
        let _ = writeln!(out, "bounds: min={lo:?} max={hi:?}");
    } else {
        put(&mut out, "bounds: none (empty or non-finite)");
    }
    let _ = writeln!(out, "signed volume: {} mm3", m.signed_volume());
    Ok(Dump {
        text: out,
        clean: true,
    })
}

/// Dump a file, choosing the reader by its first bytes: the .chitubox magic, else STL.
pub fn dump(path: &str, buf: &[u8]) -> Res<Dump> {
    if buf.starts_with(&chitubox::MAGIC.to_le_bytes()) {
        chitubox_dump(path, buf)
    } else {
        stl_dump(path, buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_lists_an_empty_project_and_is_clean() {
        let mut b = vec![0u8; 420];
        b[..4].copy_from_slice(&chitubox::MAGIC.to_le_bytes());
        b[8..12].copy_from_slice(&420u32.to_le_bytes());
        b[16..20].copy_from_slice(&412u32.to_le_bytes());
        let d = dump("empty", &b).expect("dumps");
        assert!(d.clean, "{}", d.text);
        assert!(d.text.contains("preview: none") && d.text.contains("anomalies: none"));
    }

    #[test]
    fn dump_rejects_files_that_are_neither_format() {
        assert!(dump("x", b"hello world").is_err());
    }
}
