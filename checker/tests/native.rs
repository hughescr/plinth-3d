//! Native-file tests (design dev-loop.md section 6, step 5): the checker's reader, its
//! sequential structural walk and the file bytes must agree on every native .chitubox file,
//! and its STL reader must read every STL in the evidence folders.
//!
//! Test names start with `native_` so nextest gives them the native-file time cap.

use plinth_check::bytes::{f32_at, u32_at};
use plinth_check::chitubox::{self, ChituFile, kind};
use plinth_check::{layout, stl};
use std::path::{Path, PathBuf};
use std::process::Command;

const MUDSKIPPER: &str = "morel_mudskipper_supports.chitubox";

fn files_under(dir: &str, ext: &str) -> Vec<PathBuf> {
    fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, ext, out);
            } else if p.extension().is_some_and(|x| x.to_str() == Some(ext)) {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(Path::new(dir), ext, &mut out);
    out.sort();
    out
}

fn name(p: &Path) -> String {
    p.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

fn load(p: &Path) -> (Vec<u8>, ChituFile) {
    let bytes = std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let f = chitubox::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    (bytes, f)
}

/// Facts about one file that the reader, the walk and the raw bytes must all agree on.
fn assert_agreement(p: &Path, bytes: &[u8], f: &ChituFile) {
    let who = name(p);
    let walked = layout::walk(bytes).unwrap_or_else(|e| panic!("{who}: walk failed: {e}"));
    let diff = layout::diff(&f.regions, &walked);
    assert!(diff.is_empty(), "{who}: reader and walk differ: {diff:?}");
    assert!(f.anomalies.is_empty(), "{who}: anomalies {:?}", f.anomalies);
    assert!(f.tiles, "{who}: regions do not tile the file");
    assert_eq!(
        layout::covered(&f.regions),
        f.len,
        "{who}: region sizes do not add up to the file size"
    );
    assert_eq!(f.len, bytes.len() as u64, "{who}");
    let count = u32_at(bytes, 4, "count").expect("count");
    assert_eq!(
        usize::try_from(count).expect("small"),
        f.models.len(),
        "{who}: model count"
    );
    for (i, m) in f.models.iter().enumerate() {
        let mesh = f.mesh(bytes, i).expect("mesh");
        assert_eq!(
            mesh.len() as u64,
            9 * m.triangles(),
            "{who}: model {i} mesh length"
        );
        // The first triangle, read straight from the bytes at the model record's mesh offset.
        if let Some(first) = mesh.get(..9) {
            for (k, v) in first.iter().enumerate() {
                let raw =
                    f32_at(bytes, u64::from(m.mesh_off) + 4 * k as u64, "vertex").expect("vertex");
                assert_eq!(
                    raw.to_bits(),
                    v.to_bits(),
                    "{who}: model {i} first triangle word {k}"
                );
            }
        }
        if let (Some(front), Some(s)) = (&m.front, &m.support) {
            assert_eq!(
                front.attrs.len(),
                s.records.len(),
                "{who}: model {i} attr entries vs records"
            );
        }
        assert_eq!(
            m.front.is_some(),
            m.support.is_some(),
            "{who}: model {i} front block vs supports"
        );
    }
}

#[test]
fn native_readers_agree_on_every_file() {
    let files = files_under("../test-data", "chitubox");
    assert!(
        files.len() >= 15,
        "expected the 15 native files, found {}",
        files.len()
    );
    let mut clean = 0;
    for p in &files {
        let (bytes, f) = load(p);
        if name(p) == MUDSKIPPER {
            // Not a clean ChituBox save (format reference section 14): its attr array is the
            // wrong length, so the reader must say so and the two methods must disagree.
            assert!(
                !f.anomalies.is_empty(),
                "{MUDSKIPPER} should have anomalies"
            );
            let diff = layout::diff(&f.regions, &layout::walk(&bytes).expect("walk"));
            assert!(
                !diff.is_empty(),
                "{MUDSKIPPER}: the walk should differ from the reader"
            );
        } else {
            assert_agreement(p, &bytes, &f);
            clean += 1;
        }
    }
    assert_eq!(clean, files.len() - 1);
}

#[test]
fn native_round25_plain_matches_the_format_reference() {
    // The layout table of file 01 in the format reference, section 1.
    let p = Path::new("../test-data/chitubox-native/samples/01-round25-plain.chitubox");
    let (_, f) = load(p);
    let got: Vec<(&str, u64, u64)> = f.regions.iter().map(|r| (r.kind, r.start, r.end)).collect();
    let want = [
        (kind::HEADER, 0, 24),
        (kind::ZERO_BLOCK, 24, 420),
        (kind::PREVIEW_HEADER, 420, 452),
        (kind::PREVIEW_DATA, 452, 76086),
        (kind::MODEL_TABLE, 76086, 76766),
        (kind::DESCRIPTORS, 76766, 76790),
        (kind::MESH, 76790, 184_646),
    ];
    assert_eq!(got, want);
    let pv = f.preview.as_ref().expect("preview");
    assert_eq!((pv.width, pv.height, pv.data_len), (2103, 1580, 75_634));
    assert_eq!(pv.pixels, Some(2103 * 1580));
    assert_eq!(f.models[0].name, "Round 25.stl");
    assert_eq!(f.models[0].triangles(), 2996);
}

#[test]
fn native_supported_files_have_the_documented_record_counts() {
    let dir = "../test-data/chitubox-native/samples/";
    let count = |file: &str, model: usize| {
        let (_, f) = load(&Path::new(dir).join(file));
        let m = &f.models[model];
        (
            m.support.as_ref().map(|s| s.records.len()),
            m.front.as_ref().map(|x| x.attrs.len()),
        )
    };
    // Format reference sections 7, 10 and 11: 03 has 98 records, the connected raft 107.
    assert_eq!(
        count("03-round25-supports.chitubox", 0),
        (Some(98), Some(98))
    );
    assert_eq!(
        count("04-round25-supports-raft-connected.chitubox", 0),
        (Some(107), Some(107))
    );
    assert_eq!(count("05-two-plates.chitubox", 1), (Some(107), Some(107)));
    assert_eq!(count("01-round25-plain.chitubox", 0), (None, None));
    let (_, f5) = load(&Path::new(dir).join("05-two-plates.chitubox"));
    assert_eq!(f5.models[1].mesh_off, 987_950);
    assert_eq!(f5.models[1].translate[0].to_bits(), 0x437e_fffe);
}

#[test]
fn native_stl_reader_reads_every_stl() {
    let mut files = files_under("../test-data", "stl");
    files.extend(files_under("../fixtures", "stl"));
    assert!(files.len() >= 10, "found only {} STL files", files.len());
    for p in &files {
        let bytes = std::fs::read(p).expect("read");
        let m = stl::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        assert!(m.triangles() > 0, "{}", p.display());
        assert!(
            m.bounds().is_some(),
            "{}: non-finite coordinate",
            p.display()
        );
        if m.format == stl::StlFormat::Binary {
            assert_eq!(
                m.triangles(),
                (bytes.len() as u64 - 84) / 50,
                "{}",
                p.display()
            );
        }
    }
}

#[test]
fn native_stl_reader_and_chitubox_reader_agree_on_round25() {
    // Format reference section 5: file 01's mesh is the source STL minus its bounding-box centre.
    let stl_bytes = std::fs::read("../fixtures/round/original/Round 25.stl").expect("Round 25.stl");
    let m = stl::parse(&stl_bytes).expect("stl");
    let (bytes, f) = load(Path::new(
        "../test-data/chitubox-native/samples/01-round25-plain.chitubox",
    ));
    let mesh = f.mesh(&bytes, 0).expect("mesh");
    assert_eq!(m.triangles(), f.models[0].triangles());
    let (lo, hi) = m.bounds().expect("bounds");
    let centre = [
        (lo[0] + hi[0]) / 2.0,
        (lo[1] + hi[1]) / 2.0,
        (lo[2] + hi[2]) / 2.0,
    ];
    let worst = m
        .coords
        .chunks_exact(3)
        .zip(mesh.chunks_exact(3))
        .flat_map(|(s, c)| (0..3).map(move |k| (s[k] - centre[k] - c[k]).abs()))
        .fold(0.0f32, f32::max);
    assert!(worst < 1e-5, "largest difference {worst} mm");
}

/// True if the reader, the tiling check or the walk notices something wrong with `bytes`.
fn structure_flags_a_problem(bytes: &[u8]) -> bool {
    let Ok(f) = chitubox::parse(bytes) else {
        return true;
    };
    let walk_differs =
        layout::walk(bytes).map_or(true, |w| !layout::diff(&f.regions, &w).is_empty());
    !f.anomalies.is_empty() || !f.tiles || walk_differs
}

#[test]
fn native_sabotage_every_structural_field_of_a_supported_file_is_noticed() {
    // Sabotage of the readers-agree test: change one structural field of file 03 at a time.
    // Each change must make the reader, the tiling check or the walk complain.
    let p = Path::new("../test-data/chitubox-native/samples/03-round25-supports.chitubox");
    let (bytes, f) = load(p);
    assert!(
        !structure_flags_a_problem(&bytes),
        "the unchanged file must be clean"
    );
    let m = &f.models[0];
    let (rec, ix) = (
        u64::from(f.header.off8),
        m.support.as_ref().expect("supports").index.clone(),
    );
    let desc = u64::from(f.header.off20);
    let fr = m.front.as_ref().expect("front block");
    let r0 = u64::from(ix.records_off);
    let fields: [(&str, u64); 30] = [
        ("header model count", 4),
        ("header off8", 8),
        ("header f12", 12),
        ("header f16", 16),
        ("header off20", 20),
        ("zero block", 100),
        ("preview width", 420),
        ("preview data offset", 428),
        ("preview data length", 432),
        ("model name after NUL", rec + 100),
        ("model record padding", rec + 300),
        ("model support index offset", rec + 268),
        ("model mesh offset", rec + 272),
        ("model mesh size", rec + 276),
        ("descriptor preset count", desc),
        ("descriptor table pointer", desc + 8),
        ("descriptor attr pointer", desc + 16),
        ("preset name pointer", fr.table_ptr + 8),
        ("preset name length", fr.table_ptr + 16),
        ("support index record count", ix.index_off),
        ("support index records offset", ix.index_off + 4),
        ("support index blob offset", ix.index_off + 8),
        ("support index blob size", ix.index_off + 12),
        ("group header", ix.index_off + 16 + 50),
        ("record magic", r0),
        ("record 0 mesh offset", r0 + 40),
        ("record 0 mesh length", r0 + 44),
        ("record 5 mesh offset", r0 + 5 * 72 + 40),
        ("last record mesh length", r0 + 97 * 72 + 44),
        ("trailer", r0 + 98 * 72 + 3),
    ];
    for (what, at) in fields {
        let mut b = bytes.clone();
        let i = usize::try_from(at).expect("small");
        b[i] ^= 8;
        assert!(
            structure_flags_a_problem(&b),
            "{what}: an edit at byte {at} was not noticed"
        );
    }
    // What the structure checks cannot see: a changed coordinate or translate is data, not
    // structure. Those are for the rule measurements (H7 and later), not for this test.
    let mut b = bytes.clone();
    let mesh = usize::try_from(m.mesh_off).expect("small") + 8;
    b[mesh] = b[mesh].wrapping_add(1);
    assert!(
        !structure_flags_a_problem(&b),
        "a coordinate edit is invisible to the structure checks"
    );
}

fn run(args: &[&str]) -> (i32, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_plinth-check"))
        .args(args)
        .output()
        .expect("runs");
    (
        o.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

#[test]
fn native_dump_command_describes_a_sample() {
    let (code, out, err) = run(&[
        "dump",
        "../test-data/chitubox-native/samples/03-round25-supports.chitubox",
    ]);
    assert_eq!(code, 0, "{err}\n{out}");
    assert!(
        out.contains("triangles=2996") && out.contains("support records: 98"),
        "{out}"
    );
    assert!(
        out.contains("sequential walk: same") && out.contains("anomalies: none"),
        "{out}"
    );
    let (code, out, _) = run(&[
        "dump",
        "../test-data/chitubox-native/samples/../morel_mudskipper_supports.chitubox",
    ]);
    assert_eq!(code, 1, "the mudskipper file is not clean");
    assert!(out.contains("DIFFERENT"), "{out}");
    let (code, out, _) = run(&["dump", "../fixtures/round/original/Round 25.stl"]);
    assert_eq!(code, 0);
    assert!(out.contains("triangles=2996"), "{out}");
}

/// The neutral record lines of `dump` read back to exactly the records the reader holds
/// (same count, type, end points and radii, bit for bit), on every native file. The
/// acceptance test against the production reader parses these same lines.
#[test]
fn native_dump_record_lines_equal_the_reader_records() {
    let mut total = 0;
    for p in files_under("../test-data", "chitubox") {
        let (_, f) = load(&p);
        let (_, out, err) = run(&["dump", &p.to_string_lossy()]);
        let lines: Vec<Vec<&str>> = out
            .lines()
            .filter(|l| l.starts_with("record "))
            .map(|l| l.split(' ').collect())
            .collect();
        let expected: usize = f
            .models
            .iter()
            .map(|m| m.support.as_ref().map_or(0, |s| s.records.len()))
            .sum();
        assert_eq!(lines.len(), expected, "{}: record lines\n{err}", name(&p));
        let mut at = 0;
        for (mi, m) in f.models.iter().enumerate() {
            for (k, r) in m.support.iter().flat_map(|s| s.records.iter()).enumerate() {
                let l = &lines[at];
                at += 1;
                assert_eq!(l.len(), 12, "{}: {l:?}", name(&p));
                assert_eq!(l[1], mi.to_string());
                assert_eq!(l[2], k.to_string());
                assert_eq!(l[3], r.kind.to_string());
                let nums: Vec<u32> = l[4..]
                    .iter()
                    .map(|t| t.parse::<f32>().expect("number").to_bits())
                    .collect();
                let want = [
                    r.start[0], r.start[1], r.start[2], r.end[0], r.end[1], r.end[2], r.r1, r.r2,
                ]
                .map(f32::to_bits);
                assert_eq!(nums, want, "{}: model {mi} record {k}", name(&p));
                total += 1;
            }
        }
    }
    assert!(total > 100, "only {total} records compared");
}

#[test]
fn native_check_report_marks_every_rule_unverified() {
    let args = [
        "check",
        "--chitubox",
        "../test-data/chitubox-native/samples/01-round25-plain.chitubox",
        "--stl",
        "../fixtures/round/original/Round 25.stl",
        "--limits",
        "../limits.toml",
    ];
    let (code, out, err) = run(&args);
    assert_eq!(code, 2, "unverified exit code; stderr: {err}");
    for id in ["H1", "H2", "H3", "H4", "H5", "H6", "H7", "H8", "H9"] {
        assert!(
            out.contains(&format!("\"id\": \"{id}\"")),
            "{id} missing: {out}"
        );
    }
    assert_eq!(
        out.matches("\"status\": \"unverified\"").count(),
        9,
        "{out}"
    );
    assert!(
        !out.contains("\"status\": \"pass\"") && !out.contains("\"status\": \"violation\""),
        "{out}"
    );
    assert!(
        out.contains("\"verdict\": \"unverified\"") && out.contains("\"h1_patch_area_mm2\": 0.5"),
        "{out}"
    );
    assert!(
        out.contains("\"sha256\"") && out.contains("\"anomalies\": []"),
        "{out}"
    );
    // A file that is not a .chitubox file fails, with the rules still unverified, never passed.
    let bad = [
        "check",
        "--chitubox",
        "../limits.toml",
        "--limits",
        "../limits.toml",
    ];
    let (code, out, _) = run(&bad);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("\"verdict\": \"fail\"") && out.contains("\"parse_error\""),
        "{out}"
    );
    assert!(!out.contains("\"status\": \"pass\""), "{out}");
}
