//! Broken files (T1n; design dev-loop.md section 6, section 10 step 8).
//!
//! Each file here is built by test code through the production writer (`plinth-chitubox`)
//! and then handed to the real `plinth-check` program through the production hand-off
//! (`plinth_cli::check::run`). Nothing is committed as a binary.
//!
//! What a result may be: a broken file for rule Hn must be rejected by Hn (status
//! "violation", verdict "fail"), or Hn must be reported "unverified" while its measurement
//! is not built. It must never be "pass". [`MEASURED`] lists the rules whose measurement the
//! checker has; a step that builds a measurement adds the rule there in the same commit, and
//! these tests then demand "violation" for that rule's files. Today `MEASURED` is empty, so
//! every rule file must come back "unverified".
//!
//! What this does not show: a file that comes back "unverified" proves only that the checker
//! does not claim a pass. It does not show the geometry is wrong, and no check is shown to see
//! any defect yet. The smallest defect each check can see is recorded per rule when its
//! measurement is built (design section 6, "Broken files"): for every rule today that is
//! "none; the rule is unverified".

use crate::readers_agree::{checker_binary, repo_root};
use plinth_chitubox::preview::BACKGROUND;
use plinth_chitubox::{
    Model, Preset, Preview, Project, RecordType, SupportRecord, Supports, Triangle,
};
use plinth_cli::check::{self, Checked, Job, Verdict};
use std::fs;
use std::path::PathBuf;

/// Rules whose measurement the checker has. Empty until step 11.
const MEASURED: &[&str] = &[];

const RULES: [&str; 9] = ["H1", "H2", "H3", "H4", "H5", "H6", "H7", "H8", "H9"];

type V3 = [f32; 3];

// ---- geometry the cases are made of ---------------------------------------------------

/// A closed box centred on `c` with half-width `h`, outward winding; `inward` flips it (a
/// cavity wall).
fn cube(c: V3, h: f32, inward: bool) -> Vec<Triangle> {
    let p = |i: usize| {
        [
            c[0] + if i & 1 != 0 { h } else { -h },
            c[1] + if i & 2 != 0 { h } else { -h },
            c[2] + if i & 4 != 0 { h } else { -h },
        ]
    };
    let quads = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    let mut out = Vec::new();
    for q in quads {
        let (a, b, c2, d) = (p(q[0]), p(q[1]), p(q[2]), p(q[3]));
        if inward {
            out.push([a, c2, b]);
            out.push([a, d, c2]);
        } else {
            out.push([a, b, c2]);
            out.push([a, c2, d]);
        }
    }
    out
}

/// A square tube along z: outer half-width `a`, a square bore of half-width `b` through it,
/// height `h`, centred on the origin. Closed (both ends are frames), outward winding.
fn tube(a: f32, b: f32, h: f32) -> Vec<Triangle> {
    let corner = |half: f32, i: usize, z: f32| {
        let (sx, sy) = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)][i % 4];
        [sx * half, sy * half, z]
    };
    let (lo, hi) = (-0.5 * h, 0.5 * h);
    let mut out = Vec::new();
    for i in 0..4 {
        for (half, flip) in [(a, false), (b, true)] {
            let (b0, b1) = (corner(half, i, lo), corner(half, i + 1, lo));
            let (t0, t1) = (corner(half, i, hi), corner(half, i + 1, hi));
            if flip {
                out.push([b0, t1, b1]);
                out.push([b0, t0, t1]);
            } else {
                out.push([b0, b1, t1]);
                out.push([b0, t1, t0]);
            }
        }
        for (z, up) in [(hi, true), (lo, false)] {
            let (o0, o1) = (corner(a, i, z), corner(a, i + 1, z));
            let (i0, i1) = (corner(b, i, z), corner(b, i + 1, z));
            if up {
                out.push([o0, o1, i1]);
                out.push([o0, i1, i0]);
            } else {
                out.push([o0, i1, o1]);
                out.push([o0, i0, i1]);
            }
        }
    }
    out
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(a: V3) -> V3 {
    let n = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    [a[0] / n, a[1] / n, a[2] / n]
}

/// An 8-sided frustum from `s` (radius `r1`) to `e` (radius `r2`).
fn frustum(s: V3, e: V3, r1: f32, r2: f32) -> Vec<Triangle> {
    let axis = unit(sub(e, s));
    let helper = if axis[2].abs() < 0.9 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let u = unit(cross(axis, helper));
    let v = cross(axis, u);
    let ring = |c: V3, r: f32, k: usize| {
        let ang = std::f32::consts::TAU * (k % 8) as f32 / 8.0;
        let (sn, cs) = ang.sin_cos();
        [
            c[0] + r * (cs * u[0] + sn * v[0]),
            c[1] + r * (cs * u[1] + sn * v[1]),
            c[2] + r * (cs * u[2] + sn * v[2]),
        ]
    };
    let mut out = Vec::new();
    for k in 0..8 {
        let (a0, a1) = (ring(s, r1, k), ring(s, r1, k + 1));
        let (b0, b1) = (ring(e, r2, k), ring(e, r2, k + 1));
        out.push([a0, a1, b1]);
        out.push([a0, b1, b0]);
        out.push([s, a1, a0]);
        out.push([e, b0, b1]);
    }
    out
}

/// An octahedron standing in for a ball of radius `r` at `c`.
fn ball(c: V3, r: f32) -> Vec<Triangle> {
    let p = [
        [c[0] + r, c[1], c[2]],
        [c[0] - r, c[1], c[2]],
        [c[0], c[1] + r, c[2]],
        [c[0], c[1] - r, c[2]],
        [c[0], c[1], c[2] + r],
        [c[0], c[1], c[2] - r],
    ];
    let faces = [
        [0, 2, 4],
        [2, 1, 4],
        [1, 3, 4],
        [3, 0, 4],
        [2, 0, 5],
        [1, 2, 5],
        [3, 1, 5],
        [0, 3, 5],
    ];
    faces.iter().map(|f| [p[f[0]], p[f[1]], p[f[2]]]).collect()
}

fn rod(kind: RecordType, s: V3, e: V3, r1: f32, r2: f32) -> SupportRecord {
    SupportRecord {
        record_type: kind,
        start: s,
        end: e,
        r1,
        r2,
        tail: [0; 6],
        mesh: frustum(s, e, r1, r2),
    }
}

fn sphere(kind: RecordType, c: V3, r: f32) -> SupportRecord {
    SupportRecord {
        record_type: kind,
        start: c,
        end: c,
        r1: r,
        r2: r,
        tail: [0; 6],
        mesh: ball(c, r),
    }
}

// ---- cases ------------------------------------------------------------------------------

/// A file under test and the STL it claims to come from.
struct Built {
    mesh: Vec<Triangle>,
    translate: V3,
    records: Vec<SupportRecord>,
    /// The input STL's triangles (world frame); `None` means the model as written.
    stl: Option<Vec<Triangle>>,
}

impl Built {
    fn new(mesh: Vec<Triangle>, lift: f32, records: Vec<SupportRecord>) -> Self {
        Self {
            mesh,
            translate: [0.0, 0.0, lift],
            records,
            stl: None,
        }
    }

    fn with_stl(mut self, stl: Vec<Triangle>) -> Self {
        self.stl = Some(stl);
        self
    }

    /// The bytes of the project, written by the production writer.
    fn chitubox(&self) -> Vec<u8> {
        let supports = (!self.records.is_empty()).then(|| Supports {
            presets: vec![Preset {
                id: 0,
                name: "Ultralight".to_owned(),
            }],
            attrs: vec![0; self.records.len()],
            unreferenced_blob: Vec::new(),
            records: self.records.clone(),
        });
        let project = Project {
            preview: Some(Preview::solid(64, 64, BACKGROUND).expect("preview")),
            models: vec![Model {
                name: "broken case".to_owned(),
                translate: self.translate,
                mesh: self.mesh.clone(),
                supports,
            }],
            ..Project::default()
        };
        project.to_bytes().expect("the production writer writes it")
    }

    /// The input STL, binary.
    fn stl_bytes(&self) -> Vec<u8> {
        let tris = self.stl.as_ref().unwrap_or(&self.mesh);
        let mut out = vec![0_u8; 80];
        out.extend_from_slice(&u32::try_from(tris.len()).expect("count").to_le_bytes());
        for t in tris {
            out.extend_from_slice(&[0; 12]);
            for v in t.iter().flatten() {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(&[0; 2]);
        }
        out
    }
}

struct Case {
    rule: &'static str,
    name: &'static str,
    build: fn() -> Built,
}

const fn case(rule: &'static str, name: &'static str, build: fn() -> Built) -> Case {
    Case { rule, name, build }
}

/// A straight post of one radius.
fn post(s: V3, e: V3, r: f32) -> SupportRecord {
    rod(RecordType::ROD, s, e, r, r)
}

const LIFT: f32 = 5.0;
const TIP: V3 = [0.0, 0.0, -5.0];

fn cube10(records: Vec<SupportRecord>, lift: f32) -> Built {
    Built::new(cube([0.0; 3], 5.0, false), lift, records)
}

/// A tip ball, tip cone and rod that carry one contact on the underside of a cube.
fn contact(at: V3, r_tip: f32) -> Vec<SupportRecord> {
    let below = [at[0], at[1], at[2] - 3.0];
    vec![
        sphere(RecordType::TIP_BALL, at, r_tip),
        rod(RecordType::TIP_CONE, at, below, r_tip, 0.4),
        post(below, [at[0], at[1], -18.0], 0.4),
    ]
}

fn h1_oversized_tip_patch() -> Built {
    // Tip radius 1.2 mm: a patch of about 4.5 mm^2 against the 0.50 mm^2 limit.
    cube10(contact(TIP, 1.2), LIFT)
}

fn h1_oversized_foot_on_model() -> Built {
    // A model-to-model foot of radius 1.0 mm resting on the cube's top face.
    let top = [0.0, 0.0, 5.0];
    let recs = vec![
        sphere(RecordType::FOOT_BALL, top, 1.0),
        rod(RecordType::FOOT_CONE, top, [0.0, 0.0, 8.0], 1.0, 0.4),
        post([0.0, 0.0, 8.0], [0.0, 0.0, 14.0], 0.4),
    ];
    cube10(recs, LIFT)
}

fn h2_fused_patches() -> Built {
    // Two 0.3 mm tips whose centres are 0.5 mm apart on the same face: the patches touch.
    let mut recs = contact(TIP, 0.3);
    recs.extend(contact([0.5, 0.0, -5.0], 0.3));
    cube10(recs, LIFT)
}

fn h3_brace_before_its_carrier() -> Built {
    // The carrier post starts at z = -12.5. The brace is joined to the carrier's lower end
    // (connected in the graph) but starts at z = -14, a layer before the carrier exists.
    let recs = vec![
        post([0.0, 0.0, -12.5], [0.0, 0.0, -5.0], 0.4),
        post([3.0, 0.0, -14.0], [0.0, 0.0, -12.5], 0.4),
        sphere(RecordType::TIP_BALL, TIP, 0.3),
    ];
    cube10(recs, LIFT)
}

fn h4_lift_below_five_mm() -> Built {
    cube10(contact(TIP, 0.3), 2.0)
}

fn h4_model_touches_the_raft() -> Built {
    // Lift 0 and a raft pad directly under the model.
    let recs = vec![rod(
        RecordType::RAFT,
        [0.0, 0.0, -5.0],
        [0.0, 0.0, -5.5],
        4.0,
        4.0,
    )];
    cube10(recs, 0.0)
}

fn h5_cavity_without_vent() -> Built {
    // A sealed 4 mm cavity inside the 10 mm cube: no drain, no vent.
    let mut mesh = cube([0.0; 3], 5.0, false);
    mesh.extend(cube([0.0; 3], 2.0, true));
    Built::new(mesh, LIFT, contact(TIP, 0.3))
}

fn h5_support_inside_a_drain_bore() -> Built {
    // A 4 x 4 mm square tube with a 2 x 2 mm bore, and a 0.4 mm post running up the bore.
    let recs = vec![post([0.0, 0.0, -9.0], [0.0, 0.0, 1.0], 0.4)];
    Built::new(tube(2.0, 1.0, 10.0), LIFT, recs)
}

fn h6_rod_too_close_to_the_model() -> Built {
    // A 0.3 mm radius post with its axis 0.4 mm off the cube's side face: gap 0.1 mm.
    let recs = vec![post([5.4, 0.0, -10.0], [5.4, 0.0, 4.0], 0.3)];
    cube10(recs, LIFT)
}

fn h6_rod_inside_the_model() -> Built {
    let recs = vec![post([3.0, 0.0, -10.0], [3.0, 0.0, 4.0], 0.3)];
    cube10(recs, LIFT)
}

fn world(mesh: &[Triangle], shift: V3) -> Vec<Triangle> {
    mesh.iter()
        .map(|t| t.map(|v| [v[0] + shift[0], v[1] + shift[1], v[2] + shift[2]]))
        .collect()
}

fn h7_scale_change() -> Built {
    // The STL says 11 mm; the file holds 10 mm.
    cube10(contact(TIP, 0.3), LIFT).with_stl(cube([0.0; 3], 5.5, false))
}

fn h7_missing_part() -> Built {
    // The STL has a second shell that the file lacks.
    let mut stl = cube([0.0; 3], 5.0, false);
    stl.extend(cube([20.0, 0.0, 0.0], 3.0, false));
    cube10(contact(TIP, 0.3), LIFT).with_stl(stl)
}

fn h7_undeclared_move() -> Built {
    // The STL cube is 3 mm to one side of where the file puts it, with no recorded pose.
    cube10(contact(TIP, 0.3), LIFT).with_stl(world(&cube([0.0; 3], 5.0, false), [3.0, 0.0, 0.0]))
}

fn h8_never_opened_in_chitubox() -> Built {
    // A tidy file. No ChituBox record exists for it, so H8 cannot be anything but unverified.
    cube10(contact(TIP, 0.3), LIFT)
}

fn h9_rod_too_thin() -> Built {
    // A 0.04 mm radius post: 0.08 mm across against the 0.1 mm minimum.
    let mut recs = contact(TIP, 0.3);
    recs.push(post([4.0, 4.0, -18.0], [4.0, 4.0, -8.0], 0.04));
    cube10(recs, LIFT)
}

/// The z of the lower thick rod's top cap, and of the upper thick rod's bottom cap.
const JOIN_LOW_CAP: f32 = -12.0;
const JOIN_HIGH_CAP: f32 = -11.92;

/// Two 0.25 mm radius posts with a 0.08 mm gap between their caps. The only thing that
/// connects them is a ball of 0.04 mm radius (0.08 mm across) whose bottom touches the lower
/// cap and whose top touches the upper cap. Each rod is thick enough; the load-bearing
/// section between them is not.
fn thin_join_records() -> Vec<SupportRecord> {
    let mid = 0.5 * (JOIN_LOW_CAP + JOIN_HIGH_CAP);
    vec![
        post([0.0, 0.0, -18.0], [0.0, 0.0, JOIN_LOW_CAP], 0.25),
        sphere(RecordType::JOINT_BALL, [0.0, 0.0, mid], 0.04),
        post([0.0, 0.0, JOIN_HIGH_CAP], [0.0, 0.0, -5.0], 0.25),
    ]
}

fn h9_thin_join_between_thick_rods() -> Built {
    cube10(thin_join_records(), LIFT)
}

const CASES: &[Case] = &[
    case("H1", "oversized tip patch", h1_oversized_tip_patch),
    case("H1", "oversized foot on model", h1_oversized_foot_on_model),
    case("H2", "fused patches", h2_fused_patches),
    case("H3", "brace before carrier", h3_brace_before_its_carrier),
    case("H4", "lift below 5 mm", h4_lift_below_five_mm),
    case("H4", "model touches raft", h4_model_touches_the_raft),
    case("H5", "cavity without vent", h5_cavity_without_vent),
    case("H5", "drain bore", h5_support_inside_a_drain_bore),
    case("H6", "rod too close", h6_rod_too_close_to_the_model),
    case("H6", "rod inside model", h6_rod_inside_the_model),
    case("H7", "scale change", h7_scale_change),
    case("H7", "missing part", h7_missing_part),
    case("H7", "undeclared move", h7_undeclared_move),
    case("H8", "never opened", h8_never_opened_in_chitubox),
    case("H9", "rod too thin", h9_rod_too_thin),
    case("H9", "thin join", h9_thin_join_between_thick_rods),
];

// ---- running and judging ------------------------------------------------------------------

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("plinth-accept-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).expect("make scratch folder");
    d
}

/// Write `bytes` and `stl` into `dir`, run the real checker through the production hand-off.
fn check_bytes(dir: &std::path::Path, bytes: &[u8], stl: &[u8]) -> Result<Checked, String> {
    let file = dir.join("f.chitubox");
    let stl_path = dir.join("f.stl");
    let report = dir.join("r.json");
    fs::write(&file, bytes).expect("write file");
    fs::write(&stl_path, stl).expect("write stl");
    let limits = repo_root().join("limits.toml");
    check::run(&Job {
        checker: &checker_binary(),
        limits: &limits,
        chitubox: &file,
        stl: &stl_path,
        report: &report,
        written: bytes,
    })
}

fn status<'a>(c: &'a Checked, id: &str) -> &'a str {
    c.rules
        .iter()
        .find(|r| r.id == id)
        .map_or("missing", |r| r.status.as_str())
}

/// What the case's rule must say today.
fn expected_status(rule: &str) -> &'static str {
    if MEASURED.contains(&rule) {
        "violation"
    } else {
        "unverified"
    }
}

/// Judge the checker's answer for a broken file of `rule`: `Err` says what is wrong.
fn judge(rule: &str, c: &Checked) -> Result<(), String> {
    let (got, want) = (status(c, rule), expected_status(rule));
    if got == "pass" {
        return Err(format!("{rule} passed a broken file"));
    }
    if got != want {
        return Err(format!("{rule} is \"{got}\"; expected \"{want}\""));
    }
    if status(c, "H8") == "pass" {
        return Err("H8 passed, but no ChituBox record exists".to_owned());
    }
    if !c.structure_notes.is_empty() {
        return Err(format!(
            "the file must be sound apart from its rule; structure notes: {:?}",
            c.structure_notes
        ));
    }
    let want_verdict = if want == "violation" {
        Verdict::Fail
    } else {
        Verdict::Unverified
    };
    if c.verdict != want_verdict {
        return Err(format!(
            "verdict {}; expected {}",
            c.verdict.name(),
            want_verdict.name()
        ));
    }
    Ok(())
}

fn run_rule(rule: &str) {
    let cases: Vec<&Case> = CASES.iter().filter(|c| c.rule == rule).collect();
    assert!(!cases.is_empty(), "{rule} has no broken file");
    for case in cases {
        let built = (case.build)();
        let dir = scratch(&format!("{}-{}", case.rule, case.name.replace(' ', "-")));
        let c = check_bytes(&dir, &built.chitubox(), &built.stl_bytes())
            .unwrap_or_else(|e| panic!("{} / {}: the checker's answer: {e}", case.rule, case.name));
        if let Err(why) = judge(rule, &c) {
            panic!("{} / {}: {why}", case.rule, case.name);
        }
        let _ = fs::remove_dir_all(&dir);
    }
}

macro_rules! rule_tests {
    ($($name:ident => $rule:literal),* $(,)?) => {
        $( #[test] fn $name() { run_rule($rule); } )*
    };
}

rule_tests! {
    broken_h1_patch_area => "H1",
    broken_h2_fused_contacts => "H2",
    broken_h3_continuity => "H3",
    broken_h4_lift => "H4",
    broken_h5_drains => "H5",
    broken_h6_gap => "H6",
    broken_h7_fidelity => "H7",
    broken_h8_chitubox_record => "H8",
    broken_h9_thickness => "H9",
}

#[test]
fn broken_every_rule_has_a_file_and_no_file_is_empty() {
    for id in RULES {
        let found = CASES.iter().any(|c| c.rule == id);
        assert!(found, "{id} has no broken file");
    }
    for case in CASES {
        let built = (case.build)();
        assert!(!built.mesh.is_empty(), "{}: empty model", case.name);
        let bytes = built.chitubox();
        let back = plinth_chitubox::parse(&bytes).expect("the production reader reads it");
        assert_eq!(back.models.len(), 1, "{}", case.name);
        assert!(back.deviations().is_empty(), "{}", case.name);
    }
}

/// Per-axis (min, max) of the vertices of `mesh`.
fn extent(mesh: &[Triangle]) -> [(f32, f32); 3] {
    let mut e = [(f32::INFINITY, f32::NEG_INFINITY); 3];
    for v in mesh.iter().flatten() {
        for (axis, slot) in e.iter_mut().enumerate() {
            slot.0 = slot.0.min(v[axis]);
            slot.1 = slot.1.max(v[axis]);
        }
    }
    e
}

#[test]
fn broken_thin_join_fixture_is_thin_in_the_emitted_geometry() {
    // The fixture's claim, checked on the bytes the production writer emitted and read back
    // by the production reader, with no help from the H9 measurement (which is not built):
    // two thick rods that do not touch, joined only by a section under 0.1 mm across.
    let bytes = h9_thin_join_between_thick_rods().chitubox();
    let back = plinth_chitubox::parse(&bytes).expect("the production reader reads it");
    let model = &back.models[0];
    let records = &model.supports.as_ref().expect("supports").records;
    assert_eq!(records.len(), 3, "two rods and one join");
    let [low, join, high] = [
        extent(&records[0].mesh),
        extent(&records[1].mesh),
        extent(&records[2].mesh),
    ];
    let span = |e: &[(f32, f32); 3], axis: usize| e[axis].1 - e[axis].0;
    let limit = 0.1;
    // Each rod is thick enough across its whole length: both caps are wide in X and Y.
    for (name, rod_extent, rec) in [("lower", low, &records[0]), ("upper", high, &records[2])] {
        assert!(span(&rod_extent, 0) >= 0.49, "{name} rod X span");
        assert!(span(&rod_extent, 1) >= 0.49, "{name} rod Y span");
        for cap in [rec.start, rec.end] {
            let ring_x: Vec<f32> = rec
                .mesh
                .iter()
                .flatten()
                .filter(|v| (v[2] - cap[2]).abs() < 1e-4)
                .map(|v| v[0])
                .collect();
            let wide = ring_x.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                - ring_x.iter().copied().fold(f32::INFINITY, f32::min);
            assert!(
                wide >= 0.49,
                "{name} rod cap at z={} is {wide} wide",
                cap[2]
            );
        }
    }
    // The rods are separated: a gap along z, smaller than the limit, with nothing of either
    // rod inside it.
    let gap = high[2].0 - low[2].1;
    assert!(gap > 0.0 && gap < limit, "gap between the rods is {gap} mm");
    // The join is the only record in that gap, and it is under the limit in every direction:
    // no section of it can be 0.1 mm across.
    for axis in 0..3 {
        assert!(span(&join, axis) < limit, "join span on axis {axis}");
    }
    let eps = 1e-4;
    assert!(
        (join[2].0 - low[2].1).abs() < eps,
        "join touches the lower rod's cap"
    );
    assert!(
        (join[2].1 - high[2].0).abs() < eps,
        "join touches the upper rod's cap"
    );
    // The join sits inside the rods' footprint, so it is the narrowing between them.
    assert!(join[0].0 > low[0].0 && join[0].1 < low[0].1);
    assert!(join[1].0 > low[1].0 && join[1].1 < low[1].1);
    // The model itself lies above the gap, so it adds no other connection between the rods.
    let model_low = extent(&model.mesh)[2].0;
    assert!(model_low > join[2].1, "model lies above the join region");
}

#[test]
fn broken_the_judge_catches_a_pass_and_a_wrong_status() {
    // The judge is what makes a broken-file test fail when a rule wrongly passes. Prove it
    // fires on a synthetic answer in which every rule passes.
    let rule = |id: &str, s: &str| check::Rule {
        id: id.to_owned(),
        status: s.to_owned(),
        reason: "synthetic".to_owned(),
    };
    let answer = |s: &str, verdict| Checked {
        verdict,
        sha256: String::new(),
        rules: RULES.iter().map(|id| rule(id, s)).collect(),
        structure_notes: Vec::new(),
    };
    let passed = judge("H1", &answer("pass", Verdict::Pass)).unwrap_err();
    assert!(passed.contains("passed"), "{passed}");
    assert!(judge("H1", &answer("violation", Verdict::Fail)).is_err());
    // The honest answer for today (every rule unverified) is accepted.
    let honest = answer("unverified", Verdict::Unverified);
    assert_eq!(judge("H1", &honest), Ok(()));
    // H8 passing while the rule under test is unverified is also refused.
    let mut sneaky = answer("unverified", Verdict::Unverified);
    for r in &mut sneaky.rules {
        if r.id == "H8" {
            "pass".clone_into(&mut r.status);
        }
    }
    assert!(judge("H1", &sneaky).unwrap_err().contains("H8"));
    // A structural problem is not a rule rejection.
    let mut unsound = answer("unverified", Verdict::Unverified);
    let note = "chitubox unreadable: x".to_owned();
    unsound.structure_notes.push(note);
    assert!(judge("H1", &unsound).is_err());
}

// ---- structural breaks -----------------------------------------------------------------------

/// A sound supported file to break.
fn sound_bytes() -> Vec<u8> {
    cube10(contact(TIP, 0.3), LIFT).chitubox()
}

fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

/// (name, a function that breaks the bytes, text the checker's note must contain: the place
/// or region where the file breaks)
type Mutation = (&'static str, fn(Vec<u8>) -> Vec<u8>, &'static str);

const STRUCTURAL: &[Mutation] = &[
    (
        "truncated",
        |mut b| {
            b.truncate(b.len() - 100);
            b
        },
        "support records: bytes [",
    ),
    ("empty file", |_| Vec::new(), "header magic: bytes [0, 4)"),
    (
        "bad magic",
        |mut b| {
            b[0] ^= 0xff;
            b
        },
        "magic is 0xab2312bc",
    ),
    (
        "model count huge",
        |mut b| {
            put32(&mut b, 4, 0xffff_fff0);
            b
        },
        "model table: bytes [",
    ),
    (
        "model table pointer past the end",
        |mut b| {
            let len = u32::try_from(b.len()).expect("small");
            put32(&mut b, 8, len + 1000);
            b
        },
        "lie past the end of the file",
    ),
    (
        "model table pointer into the header",
        |mut b| {
            put32(&mut b, 8, 4);
            b
        },
        "model table offset is 4, expected",
    ),
    (
        "non-zero byte in the zero block",
        |mut b| {
            b[100] = 1;
            b
        },
        "zero block [24, 420) is not all zero",
    ),
    (
        "trailing bytes",
        |mut b| {
            b.extend_from_slice(&[0xab; 64]);
            b
        },
        "64 bytes after the last region",
    ),
];

#[test]
fn broken_structural_breaks_are_rejected_with_a_located_note() {
    // Control: the unbroken file is read cleanly (so the breaks below are what fail).
    let dir = scratch("structural");
    let sound = sound_bytes();
    let stl = cube10(Vec::new(), LIFT).stl_bytes();
    let c = check_bytes(&dir, &sound, &stl).expect("control answer");
    assert!(c.structure_notes.is_empty(), "{:?}", c.structure_notes);
    assert_eq!(c.verdict, Verdict::Unverified);
    for (name, mutate, place) in STRUCTURAL {
        let broken = mutate(sound.clone());
        assert_ne!(broken, sound, "{name}: the mutation changed nothing");
        let c = check_bytes(&dir, &broken, &stl).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(c.verdict, Verdict::Fail, "{name}");
        let notes = c.structure_notes.join("; ");
        let found = notes.contains(place);
        assert!(found, "{name}: expected {place:?} in {notes}");
        assert!(
            c.rules.iter().all(|r| r.status != "pass"),
            "{name}: a rule passed a broken file"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}
