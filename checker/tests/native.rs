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

/// The status of every rule of a check report, by id, from the report text.
fn statuses(out: &str) -> std::collections::BTreeMap<String, String> {
    let mut map = std::collections::BTreeMap::new();
    let mut id = None;
    for line in out.lines().map(str::trim) {
        if let Some(v) = line.strip_prefix("\"id\": \"") {
            id = Some(v.trim_end_matches("\",").to_owned());
        } else if let Some(v) = line.strip_prefix("\"status\": \"")
            && let Some(i) = id.take()
        {
            map.insert(i, v.trim_end_matches("\",").to_owned());
        }
    }
    map
}

#[test]
fn native_check_report_on_a_lifted_model_only_file() {
    // Sample 02: the Round 25 base raised by hand to 5.0 mm, no supports. Nothing that needs
    // supports may pass; the lift, the fidelity and the closed-solid proof can.
    let args = [
        "check",
        "--chitubox",
        "../test-data/chitubox-native/samples/02-round25-lifted.chitubox",
        "--stl",
        "../fixtures/round/original/Round 25.stl",
        "--limits",
        "../limits.toml",
    ];
    let (code, out, err) = run(&args);
    assert_eq!(code, 2, "unverified exit code; stderr: {err}");
    let st = statuses(&out);
    let want = [
        ("H1", "not_applicable"),
        ("H2", "not_applicable"),
        ("H3", "unverified"),
        ("H4", "pass"),
        ("H5", "not_applicable"),
        ("H6", "not_applicable"),
        ("H7", "pass"),
        ("H8", "unverified"),
        ("H9", "not_applicable"),
    ];
    for (id, s) in want {
        assert_eq!(st.get(id).map(String::as_str), Some(s), "{id}: {out}");
    }
    assert!(
        out.contains("\"verdict\": \"unverified\"") && out.contains("\"h1_patch_area_mm2\": 0.5"),
        "{out}"
    );
    assert!(
        out.contains("\"sha256\"") && out.contains("\"anomalies\": []"),
        "{out}"
    );
    // Not accepted, not ready for review: the lifted model has nothing under it.
    assert!(out.contains("\"ready_for_owner_review\": false"), "{out}");
    assert!(
        out.contains("H5: not applicable — emitted model has 0 enclosed cavities and 0 drain bores; topology verified."),
        "{out}"
    );
}

#[test]
fn native_check_report_on_the_unlifted_plain_file_is_an_h4_violation() {
    let args = [
        "check",
        "--chitubox",
        "../test-data/chitubox-native/samples/01-round25-plain.chitubox",
        "--stl",
        "../fixtures/round/original/Round 25.stl",
        "--limits",
        "../limits.toml",
    ];
    let (code, out, _) = run(&args);
    assert_eq!(code, 1, "{out}");
    let st = statuses(&out);
    assert_eq!(st.get("H4").map(String::as_str), Some("violation"), "{out}");
    assert_eq!(st.get("H7").map(String::as_str), Some("pass"), "{out}");
    assert!(out.contains("\"rule\": \"H4\""), "{out}");
}

#[test]
fn native_check_report_on_a_file_that_is_not_chitubox_fails_and_passes_nothing() {
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

// ---- the measurements on ChituBox's own supports ------------------------------------------

use plinth_check::limits::{self, Limits};
use plinth_check::parts::{Geom, Part, Role};
use plinth_check::patch::{self, Rep};
use plinth_check::scene::Scene;
use plinth_check::v3::{V, add, cross, dot, f, frame, mul, sub, unit};

/// The five samples whose support records the design validates against, and how many
/// supported models each holds.
const SUPPORTED_SAMPLES: [(&str, usize); 5] = [
    ("03-round25-supports", 1),
    ("04-round25-supports-raft-connected", 1),
    ("04-round25-supports-raft-hex", 1),
    ("05-two-plates", 2),
    ("07-round25-on-edge-braced", 1),
];

fn limits_from_file() -> Limits {
    let text = std::fs::read_to_string("../limits.toml").expect("../limits.toml");
    limits::parse(&text).expect("limits parse")
}

fn sample_scene(sample: &str, model: usize) -> Scene {
    let p = PathBuf::from(format!(
        "../test-data/chitubox-native/samples/{sample}.chitubox"
    ));
    let (bytes, file) = load(&p);
    Scene::build(&limits_from_file(), 1, &file, &bytes, model).expect("scene")
}

#[test]
fn native_every_support_record_of_the_samples_is_the_canonical_mesh_but_the_hex_mat() {
    // The canonical-mesh validation (`canon`): the checker rebuilds the native mesh of every
    // record from its fields alone and requires the written vertices within 2 units in the
    // last place per coordinate, with the native topology.
    let (mut records, mut invalid, mut tip_balls) = (0usize, Vec::new(), 0usize);
    let mut worst: std::collections::BTreeMap<&'static str, (usize, f64)> =
        std::collections::BTreeMap::new();
    for (sample, models) in SUPPORTED_SAMPLES {
        for m in 0..models {
            let scene = sample_scene(sample, m);
            for p in &scene.parts {
                records += 1;
                tip_balls += usize::from(p.role == Role::TipBall);
                if let Some(why) = &p.invalid {
                    invalid.push(format!(
                        "{sample} model {m} record {} ({}, {:?} to {:?}, {:?}): {why}",
                        p.rec,
                        p.role.name(),
                        p.start,
                        p.end,
                        p.geom
                    ));
                } else {
                    let e = worst.entry(p.role.name()).or_insert((0, 0.0));
                    e.0 += 1;
                    e.1 = e.1.max(p.canon_ulps);
                }
            }
        }
    }
    println!("records {records}, tip balls {tip_balls}, invalid {invalid:?}");
    let passed: usize = worst.values().map(|e| e.0).sum();
    println!(
        "canonical mesh: {passed} of {records} records pass; per kind (count, worst ULP): {worst:?}"
    );
    assert_eq!(passed, 653, "all but the hex raft mat pass: {worst:?}");
    for (kind, (_, ulps)) in &worst {
        assert!(*ulps <= plinth_check::canon::ULPS, "{kind}: {ulps} ULP");
    }
    assert_eq!(tip_balls, 97, "the design counts 97 native tip balls");
    assert_eq!(invalid.len(), 1, "{invalid:?}");
    assert!(
        invalid[0].contains("04-round25-supports-raft-hex"),
        "{invalid:?}"
    );
    assert!(invalid[0].contains("not closed"), "{invalid:?}");
}

#[test]
fn native_unverified_hex_raft_mat_is_refused_by_the_checker() {
    let scene = sample_scene("04-round25-supports-raft-hex", 0);
    let hex: Vec<&Part> = scene.parts.iter().filter(|p| p.invalid.is_some()).collect();
    assert_eq!(hex.len(), 1);
    assert_eq!(hex[0].role, Role::Raft);
    println!(
        "PLINTH-UNVERIFIED: hex raft mat: {}",
        hex[0].invalid.as_deref().unwrap_or("")
    );
    // Every rule that needs the mat says unverified; none passes on a guess.
    let rules = plinth_check::measure::run_scene(&scene, None);
    for id in ["H1", "H2", "H3", "H9"] {
        let r = rules.iter().find(|r| r.id == id).expect("rule");
        assert_ne!(r.status.name(), "pass", "{id}: {}", r.reason);
    }
}

// The oracle. It is exact up to f64 rounding (about 1e-12 mm2), so the checker's interval is
// compared with it with no tolerance added: there is no oracle error to enclose beyond
// rounding. The analytic patch is the area of a disc cut by a triangle, by the closed form
// (circular sectors for the parts of each triangle edge outside the disc, triangles for the
// parts inside). The mesh patch is the area of a convex mesh section (the hull of the points
// where the mesh edges cross the plane) cut by the triangle (Sutherland-Hodgman). Neither
// shares code with the measurement, which clips each triangle by half-spaces.

type P2 = [f64; 2];

/// Signed area of (disc of radius `r` about the origin) intersected with the triangle
/// `(origin, a, b)`.
fn disc_wedge(a: P2, b: P2, r: f64) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let (qa, qb, qc) = (
        d[0] * d[0] + d[1] * d[1],
        2.0 * (a[0] * d[0] + a[1] * d[1]),
        a[0] * a[0] + a[1] * a[1] - r * r,
    );
    let mut ts = vec![0.0];
    let disc = qb * qb - 4.0 * qa * qc;
    if qa > 0.0 && disc > 0.0 {
        let s = disc.sqrt();
        for t in [(-qb - s) / (2.0 * qa), (-qb + s) / (2.0 * qa)] {
            if t > 0.0 && t < 1.0 {
                ts.push(t);
            }
        }
    }
    ts.push(1.0);
    ts.windows(2)
        .map(|w| {
            let at = |t: f64| [a[0] + d[0] * t, a[1] + d[1] * t];
            let (p, q, m) = (at(w[0]), at(w[1]), at(0.5 * (w[0] + w[1])));
            let cr = p[0] * q[1] - p[1] * q[0];
            if m[0].hypot(m[1]) <= r {
                0.5 * cr
            } else {
                0.5 * r * r * cr.atan2(p[0] * q[0] + p[1] * q[1])
            }
        })
        .sum()
}

fn ccw(mut poly: Vec<P2>) -> Vec<P2> {
    let a: f64 = (0..poly.len())
        .map(|i| {
            let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
            p[0] * q[1] - q[0] * p[1]
        })
        .sum();
    if a < 0.0 {
        poly.reverse();
    }
    poly
}

fn area2(poly: &[P2]) -> f64 {
    0.5 * (0..poly.len())
        .map(|i| {
            let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
            p[0] * q[1] - q[0] * p[1]
        })
        .sum::<f64>()
}

/// Convex hull by the monotone chain.
fn hull(points: &[P2]) -> Vec<P2> {
    let mut p = points.to_vec();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup();
    let cr = |o: P2, a: P2, b: P2| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut h: Vec<P2> = Vec::new();
    for pass in 0..2 {
        let start = h.len();
        let seq: Vec<P2> = if pass == 0 {
            p.clone()
        } else {
            p.iter().rev().copied().collect()
        };
        for q in seq {
            while h.len() >= start + 2 && cr(h[h.len() - 2], h[h.len() - 1], q) <= 0.0 {
                h.pop();
            }
            h.push(q);
        }
        h.pop();
    }
    h
}

/// A convex subject polygon cut by a convex counter-clockwise clip polygon.
fn clip_convex(subject: Vec<P2>, clip: &[P2]) -> Vec<P2> {
    let mut out = subject;
    for i in 0..clip.len() {
        let (a, b) = (clip[i], clip[(i + 1) % clip.len()]);
        let side = |p: P2| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        let input = std::mem::take(&mut out);
        for j in 0..input.len() {
            let (p, q) = (input[j], input[(j + 1) % input.len()]);
            let (sp, sq) = (side(p), side(q));
            if sp >= 0.0 {
                out.push(p);
            }
            if (sp > 0.0 && sq < 0.0) || (sp < 0.0 && sq > 0.0) {
                let t = sp / (sp - sq);
                out.push([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]);
            }
        }
        if out.is_empty() {
            break;
        }
    }
    out
}

/// The exact patch area of a ball on the model, summed over the model triangles it reaches.
fn oracle_area(scene: &Scene, part: &Part, mesh: bool) -> f64 {
    let Geom::Ball { c, r } = part.geom else {
        panic!("a ball")
    };
    let mut total = 0.0;
    for ti in scene.model_near(&part.bbox.inflated(1e-6)) {
        let t = &scene.model[ti as usize];
        let n = unit(cross(sub(t[1], t[0]), sub(t[2], t[0])));
        let s = dot(n, sub(c, t[0]));
        let (e1, e2) = frame(n);
        // The plane's origin is the foot of the ball centre; 2D coordinates are along e1, e2.
        let q = sub(c, mul(n, s));
        let to2 = |p: V| [dot(sub(p, q), e1), dot(sub(p, q), e2)];
        let tri2 = ccw(t.iter().map(|p| to2(*p)).collect());
        if mesh {
            let mut pts: Vec<P2> = Vec::new();
            for mt in &part.tris {
                for k in 0..3 {
                    let (p, qq) = (mt[k], mt[(k + 1) % 3]);
                    let (dp, dq) = (dot(n, sub(p, t[0])), dot(n, sub(qq, t[0])));
                    if dp == 0.0 {
                        pts.push(to2(p));
                    }
                    if (dp < 0.0 && dq > 0.0) || (dp > 0.0 && dq < 0.0) {
                        let u = dp / (dp - dq);
                        pts.push(to2(add(p, mul(sub(qq, p), u))));
                    }
                }
            }
            if pts.len() >= 3 {
                total += area2(&clip_convex(hull(&pts), &tri2)).abs();
            }
        } else if s.abs() < r {
            let rho = (r * r - s * s).sqrt();
            total += (0..3)
                .map(|i| disc_wedge(tri2[i], tri2[(i + 1) % 3], rho))
                .sum::<f64>()
                .abs();
        }
    }
    total
}

/// The first sample alone: the representative the fast level runs. Every sample is
/// `e2e_native_tip_ball_patches_enclose_a_fine_oracle_in_every_sample` (L2 only).
#[test]
fn native_tip_ball_patches_enclose_a_fine_oracle() {
    let balls = tip_ball_oracle(&SUPPORTED_SAMPLES[..1]);
    assert!(balls > 0, "the sample has tip balls");
}

#[test]
fn e2e_native_tip_ball_patches_enclose_a_fine_oracle_in_every_sample() {
    assert_eq!(tip_ball_oracle(&SUPPORTED_SAMPLES), 97);
}

/// Check every tip ball of the samples against the oracle; the number of balls checked.
fn tip_ball_oracle(samples: &[(&str, usize)]) -> usize {
    let mut worst_width = 0.0f64;
    let (mut balls, mut max_err) = (0usize, 0.0f64);
    let mut by_radius: std::collections::BTreeMap<String, (usize, f64)> =
        std::collections::BTreeMap::new();
    for &(sample, models) in samples {
        for m in 0..models {
            let full = sample_scene(sample, m);
            let tips: Vec<Part> = full
                .parts
                .iter()
                .filter(|p| p.role == Role::TipBall)
                .cloned()
                .collect();
            let scene = Scene::from_parts(
                &limits_from_file(),
                full.model.clone(),
                full.translate,
                tips,
            );
            let out = patch::measure(&scene);
            for rep_out in &out.reps {
                for (pi, part) in scene.parts.iter().enumerate() {
                    let chain = scene.chains.of_part[pi];
                    let (lo, hi): (f64, f64) = rep_out
                        .comps
                        .iter()
                        .filter(|c| c.chain == chain)
                        .fold((0.0, 0.0), |(l, h), c| (l + c.lo, h + c.hi));
                    let mesh = rep_out.rep == Rep::Mesh;
                    // The oracle is exact to rounding (1e-9 mm2 allowed): no tolerance for
                    // the oracle's own error is added to the checker's interval.
                    let truth = oracle_area(&scene, part, mesh);
                    let tol = 1e-9;
                    assert!(
                        lo - tol <= truth && truth <= hi + tol,
                        "{sample} model {m} record {} ({}): [{lo}, {hi}] vs oracle {truth}",
                        part.rec,
                        rep_out.rep.name()
                    );
                    worst_width = worst_width.max(hi - lo);
                    max_err = max_err.max((truth - 0.5 * (lo + hi)).abs());
                    if !mesh {
                        balls += 1;
                        let key = format!(
                            "r={:.2}",
                            match part.geom {
                                Geom::Ball { r, .. } => r,
                                _ => 0.0,
                            }
                        );
                        let e = by_radius.entry(key).or_insert((0, 0.0));
                        e.0 += 1;
                        e.1 = e.1.max(hi);
                    }
                }
            }
        }
    }
    println!(
        "balls {balls}; widest interval {worst_width:.6} mm2; largest |oracle - midpoint| {max_err:.6} mm2; largest upper bound by tip radius {by_radius:?}"
    );
    assert!(worst_width <= 0.010, "interval width {worst_width}");
    balls
}

/// The first 3 damaged files: the representative the fast level runs. All 12 are
/// `e2e_native_random_byte_damage_over_twelve_files_never_panics_or_passes` (L2 only).
#[test]
fn native_random_byte_damage_never_panics_and_never_passes_a_broken_rule() {
    byte_damage_sweep(3);
}

#[test]
fn e2e_native_random_byte_damage_over_twelve_files_never_panics_or_passes() {
    byte_damage_sweep(12);
}

fn byte_damage_sweep(files: usize) {
    // Damage a small supported native file at random places (inside the mesh and support
    // region, where the new measurements read) and run the full report in process. Any answer
    // is acceptable except a panic; a damaged file must also never say `pass` for H8.
    let p = Path::new("../test-data/chitubox-native/two-supports.chitubox");
    let (bytes, f) = load(p);
    let start = usize::try_from(f.models[0].mesh_off).expect("small");
    let span = bytes.len() - start;
    let limits = limits_from_file();
    let mut state = 0x1234_5678_9abc_def0u64;
    let mut next = move || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    let mut verdicts = [0usize; 3];
    for _ in 0..files {
        let mut damaged = bytes.clone();
        for _ in 0..=(next() % 3) {
            let at = start + usize::try_from(next()).unwrap_or(0) % span;
            damaged[at] ^= u8::try_from(1 + next() % 255).unwrap_or(1);
        }
        let input = plinth_check::report::Input {
            path: "damaged.chitubox".to_owned(),
            data: Ok(damaged),
        };
        let r = plinth_check::report::build_with(&limits, &input, None, 1);
        verdicts[match r.verdict {
            plinth_check::report::Verdict::Pass => 0,
            plinth_check::report::Verdict::Unverified => 1,
            plinth_check::report::Verdict::Fail => 2,
        }] += 1;
        assert!(!r.json.render().is_empty());
    }
    println!(
        "verdicts over {files} damaged files: pass {} unverified {} fail {}",
        verdicts[0], verdicts[1], verdicts[2]
    );
    assert_eq!(verdicts[0], 0, "H8 is never a pass, so no file is a pass");
}

#[test]
fn native_h7_pose_rotation_reproduces_chitubox_own_flip() {
    // Sample 06 is sample 01 turned 180 degrees about X by ChituBox (format reference
    // section 6.1): every triangle matches the original under that pose to under 1e-5 mm. Its
    // translate Y is 0.3536 mm off the rotated centre (an unexplained ChituBox quirk, section
    // 6.3), which H7 reports as a plan-view move, so the answer is a violation for that
    // reason alone.
    let h7_reason = |deg: &str| {
        let (_, out, err) = run(&[
            "check",
            "--chitubox",
            "../test-data/chitubox-native/samples/06-round25-flipped.chitubox",
            "--stl",
            "../fixtures/round/original/Round 25.stl",
            "--limits",
            "../limits.toml",
            "--pose-rotate-x-deg",
            deg,
        ]);
        assert!(!out.is_empty(), "{err}");
        let at = out.find("\"id\": \"H7\"").expect("H7 in the report");
        let reason = out[at..]
            .lines()
            .find(|l| l.contains("\"reason\""))
            .expect("reason");
        reason.to_owned()
    };
    let flipped = h7_reason("180");
    assert!(
        !flipped.contains("not in the input STL") && flipped.contains("translate X, Y"),
        "{flipped}"
    );
    let unrotated = h7_reason("0");
    assert!(unrotated.contains("not in the input STL"), "{unrotated}");
}

/// The thread allocation of this test run (set by `scripts/t`; 1 when absent).
#[allow(
    clippy::disallowed_methods,
    reason = "test-only read of the allocation scripts/t sets"
)]
fn allocation() -> usize {
    std::env::var("NEXTEST_TEST_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n >= 1)
        .unwrap_or(1)
}

#[test]
fn threads_checker_report_is_identical_at_one_thread_and_the_allocation() {
    let many = allocation().min(4);
    let report = |threads: usize| {
        let t = threads.to_string();
        let (code, out, err) = run(&[
            "check",
            "--chitubox",
            "../test-data/chitubox-native/samples/03-round25-supports.chitubox",
            "--stl",
            "../fixtures/round/original/Round 25.stl",
            "--limits",
            "../limits.toml",
            "--threads",
            &t,
        ]);
        assert_eq!(code, 1, "{err}");
        out.replace(&format!("\"threads\": {threads}"), "\"threads\": N")
    };
    // The two runs are independent processes: run them side by side.
    let (one, several) = std::thread::scope(|s| {
        let a = s.spawn(|| report(1));
        let b = report(many);
        (a.join().expect("the one-thread run"), b)
    });
    assert_eq!(one, several, "the report differs at {many} threads");
}

#[test]
fn native_round25_h3_distinguishes_model_contact_from_clear_air_and_bounds_work() {
    use plinth_check::json::Json;
    let scene = sample_scene("03-round25-supports", 0);
    let out = plinth_check::h3::measure(&scene, &patch::measure(&scene));
    assert_eq!(
        out.reason,
        "33 support part(s) or model island(s) have no connection to the plate or to support printed in that layer or below"
    );
    for (key, value) in [
        ("pieces", 7593),
        ("sample_planes", 610),
        ("unrooted_pieces", 1883),
        ("model_standing_feet_checked", 7),
    ] {
        assert_eq!(out.detail.get(key), Some(&Json::Uint(value)), "{key}");
    }
    let records: Vec<_> = out
        .located
        .iter()
        .filter(|l| l.reason.contains("has no connection"))
        .filter_map(|l| l.location.record)
        .collect();
    assert_eq!(records, [6, 32, 24, 21, 94]);
    let Some(Json::Arr(unknown)) = out.detail.get("unverified_items") else {
        panic!("unverified items")
    };
    let unknown: Vec<_> = unknown.iter().map(Json::render).collect();
    for (record, layer, evidence) in [
        (9, 77, "partly overlaps"),
        (12, 79, "partly overlaps"),
        (18, 83, "partly overlaps"),
        (72, 99, "overlaps local model material by the strict depth"),
        (91, 124, "partly overlaps"),
    ] {
        assert!(
            unknown.iter().any(
                |s| s.contains(&format!("record {record} (tip ball) {evidence}"))
                    && s.contains(&format!("in layer {layer};"))
            ),
            "record {record}: {unknown:?}"
        );
    }
    let Some(Json::Uint(batches)) = out.detail.get("model_slice_batches") else {
        panic!("slice work missing")
    };
    assert!(
        *batches == 17,
        "seven foot starts plus ten local air/contact checks: {batches}"
    );
    let Some(Json::Uint(tiles)) = out.detail.get("model_tiles") else {
        panic!("tile work missing")
    };
    assert!(
        *tiles > 0 && *tiles < 2000,
        "local tiles, not whole-model slices: {tiles}"
    );
}

#[test]
fn native_connected_raft_keeps_the_same_model_contact_classification() {
    use plinth_check::json::Json;
    let scene = sample_scene("04-round25-supports-raft-connected", 0);
    let out = plinth_check::h3::measure(&scene, &patch::measure(&scene));
    assert!(out.reason.starts_with("33 support part(s)"));
    let records: Vec<_> = out
        .located
        .iter()
        .filter(|l| l.reason.contains("has no connection"))
        .filter_map(|l| l.location.record)
        .collect();
    assert_eq!(records, [6, 32, 24, 21, 64]);
    let Some(Json::Arr(unknown)) = out.detail.get("unverified_items") else {
        panic!("unverified items")
    };
    for (record, evidence) in [
        (9, "partly overlaps"),
        (12, "partly overlaps"),
        (18, "partly overlaps"),
        (42, "overlaps local model material by the strict depth"),
        (61, "partly overlaps"),
    ] {
        assert!(unknown.iter().any(|s| {
            s.render()
                .contains(&format!("record {record} (tip ball) {evidence}"))
        }));
    }
    assert_eq!(out.detail.get("model_slice_batches"), Some(&Json::Uint(17)));
}

#[test]
fn native_on_edge_foot_partial_material_contact_is_not_certain_air() {
    use plinth_check::json::Json;
    let scene = sample_scene("07-round25-on-edge-braced", 0);
    let patches = patch::measure(&scene);
    assert!(patches.mesh().comps.iter().any(|c| c.parts.contains(&17)));
    let out = plinth_check::h3::measure(&scene, &patches);
    assert_eq!(
        out.reason,
        "22 support part(s) or model island(s) have no connection to the plate or to support printed in that layer or below"
    );
    assert!(out.located.iter().all(|l| l.location.record != Some(17)));
    let Some(Json::Arr(unknown)) = out.detail.get("unverified_items") else {
        panic!("unverified items")
    };
    assert!(unknown.iter().any(|s| s.render().contains("record 17 model-standing foot partly overlaps model material at its first printable section")));
    assert_eq!(
        out.detail.get("model_standing_feet_checked"),
        Some(&Json::Uint(2))
    );
    assert_eq!(out.detail.get("model_slice_batches"), Some(&Json::Uint(15)));
    for record in [24, 53, 123, 69, 65, 12, 19, 3, 50] {
        assert!(unknown.iter().any(|s| s.render().contains(&format!(
            "record {record} (tip ball) overlaps local model material by the strict depth"
        ))));
    }
}

#[test]
fn native_h9_records_the_thin_native_arms_as_a_finding_not_a_limit_change() {
    // Design section 12, point 4: ChituBox's own r 0.05 parts are 0.098 mm across flats.
    let scene = sample_scene("03-round25-supports", 0);
    let h9 = plinth_check::h9::measure(&scene);
    assert_eq!(h9.status.name(), "violation", "{}", h9.reason);
    let w = match h9.detail.get("thinnest_section_mm") {
        Some(plinth_check::json::Json::Num(w)) => *w,
        other => panic!("no thinnest section width: {other:?}"),
    };
    println!("thinnest native part: {w:.5} mm across flats");
    assert!((0.0980..0.0982).contains(&w), "{w}");
    // The joins of the native supports are measured too (and some of ChituBox's own are thin);
    // the number is recorded, not pinned.
    println!(
        "thinnest native join: {:?}",
        h9.detail.get("thinnest_join_mm")
    );
    let _ = f([0.0; 3]);
}

#[test]
fn native_approved_round25_has_the_two_islands_of_the_design() {
    // The approved Round 25 (65 degrees from flat), lifted 5 mm. Design section 0: island 1
    // is model layer 0 (the lip edge, a point); island 2 is the central annular rib's lowest
    // point at model layer 152. Both are listed from the closed mesh alone.
    let bytes = std::fs::read("../fixtures/round/approved/Round 25.keepup.stl").expect("stl");
    let stl = plinth_check::stl::parse(&bytes).expect("parse");
    let model: Vec<[V; 3]> = stl
        .coords
        .chunks_exact(9)
        .map(|t| {
            let p = |i: usize| [f64::from(t[i]), f64::from(t[i + 1]), f64::from(t[i + 2])];
            [p(0), p(3), p(6)]
        })
        .collect();
    let scene = Scene::from_parts(&limits_from_file(), model, [0.0, 0.0, 5.0], Vec::new());
    let births = plinth_check::islands::births(&scene, scene.plate_z()).expect("closed model");
    let listed: Vec<(usize, bool)> = births
        .iter()
        .map(|b| (b.layer - 100, b.certainly_over_small_size()))
        .collect();
    println!("births (model layer, over the small size): {listed:?}");
    assert!(
        listed.iter().any(|(k, _)| *k == 0),
        "island 1 (model layer 0): {listed:?}"
    );
    assert!(
        listed.iter().any(|(k, _)| (150..=154).contains(k)),
        "island 2 (model layer 152): {listed:?}"
    );
}
