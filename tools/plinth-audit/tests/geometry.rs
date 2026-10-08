//! The `geometry` subcommand: planted scene first, then the v19 anchors.
//!
//! The planted scene is built with the audit crate's own test writer
//! (`synth`), because this crate takes no engine dependency. It plants one of
//! each thing the subcommand measures, with a hand-computed answer.
//!
//! The v19 files are not in git; the real-file tests skip with a printed reason
//! when the folder is absent. Override the folder with `PLINTH_V19_DIR`.

use plinth_audit::geometry::report::{analyse, analyse_file, ModelGeometry};
use plinth_audit::geometry::Params;
use plinth_audit::reader::{ChituboxFile, RecordKind};
use plinth_audit::synth::{box_soup, SynthFile, SynthModel, SynthRecord};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// The planted scene
// ---------------------------------------------------------------------------

/// A box model whose underside is the plane z = 0 (outward normal -z), and the
/// supports below it. Record indices are returned in the order they are pushed.
struct Scene {
    records: Vec<SynthRecord>,
}

impl Scene {
    fn push(&mut self, r: SynthRecord) -> usize {
        self.records.push(r.with_matching_soup());
        self.records.len() - 1
    }
    fn tip(&mut self, centre: [f64; 3], radius: f64, out: [f64; 3]) -> usize {
        let a = [
            centre[0] - radius * out[0],
            centre[1] - radius * out[1],
            centre[2] - radius * out[2],
        ];
        let b = [
            centre[0] + radius * out[0],
            centre[1] + radius * out[1],
            centre[2] + radius * out[2],
        ];
        self.push(SynthRecord::new(RecordKind::ContactSphere, a, b, radius, radius))
    }
    fn frustum(&mut self, kind: RecordKind, a: [f64; 3], ra: f64, b: [f64; 3], rb: f64) -> usize {
        self.push(SynthRecord::new(kind, a, b, ra, rb))
    }
    fn joint(&mut self, k: [f64; 3], r: f64) -> usize {
        self.push(SynthRecord::new(
            RecordKind::JointSphere,
            [k[0], k[1], k[2] - r],
            [k[0], k[1], k[2] + r],
            r,
            r,
        ))
    }
}

struct Ids {
    head1_tip: usize,
    head2_tip: usize,
    post1: usize,
    intruder: usize,
    tilted_tip: usize,
    square_tip: usize,
    slivers: Vec<usize>,
    good_box: usize,
    inverted_box: usize,
    deep: usize,
    buried_tip: usize,
    landed_in_hull: usize,
}

fn build() -> (Vec<u8>, Ids) {
    use RecordKind::{ContactTaper as T, Member as M};
    let mut s = Scene { records: Vec::new() };

    // Head 1: ball R 0.1 with its centre 0.03 BELOW the surface plane (outside),
    // taper wider than the ball (link 0), a bend, a joint at K, a leaning post.
    let k1 = [0.3, 0.0, -1.0];
    let head1_tip = s.tip([0.0, 0.0, -0.03], 0.10, [0.0, 0.0, -1.0]);
    s.frustum(T, [0.0, 0.0, -0.03], 0.15, [0.0, 0.0, -0.5], 0.25);
    s.frustum(M, [0.0, 0.0, -0.5], 0.25, k1, 0.30);
    s.joint(k1, 0.32);
    let post1 = s.frustum(M, k1, 0.30, [1.0, 0.0, -3.0], 0.30);

    // Head 2: ball R 0.2, a THIN straight link (r 0.1) then a cone that widens.
    // The link is the bare constant-radius run (83866bf): 0.95 long from the
    // ball centre, leaving the ball sphere at sqrt(0.2^2 - 0.1^2) = 0.1732
    // along; link = 0.95 - 0.1732 = 0.7768, radius 0.1.
    let k2 = [-3.0, 0.0, -1.4];
    let head2_tip = s.tip([-3.0, 0.0, -0.05], 0.20, [0.0, 0.0, -1.0]);
    s.frustum(T, [-3.0, 0.0, -0.05], 0.10, [-3.0, 0.0, -1.0], 0.10);
    s.frustum(M, [-3.0, 0.0, -1.0], 0.10, k2, 0.40);
    s.joint(k2, 0.45);
    s.frustum(M, k2, 0.40, [-3.0, 0.0, -4.0], 0.40);

    // A member that runs 1.0 mm up into the box (its end cap centre is 1.0 deep).
    let intruder = s.frustum(M, [3.0, 3.0, -1.0], 0.2, [3.0, 3.0, 1.0], 0.2);

    // Straight contacts: one 50 degrees off the normal, one square.
    let u50 = [50.0f64.to_radians().sin(), 0.0, -50.0f64.to_radians().cos()];
    let tc = [3.0, -3.0, -0.02];
    let tilted_tip = s.tip(tc, 0.10, u50);
    s.frustum(
        T,
        tc,
        0.08,
        [tc[0] + 2.0 * u50[0], tc[1], tc[2] + 2.0 * u50[2]],
        0.08,
    );
    let square_tip = s.tip([-4.0, -3.0, -0.02], 0.10, [0.0, 0.0, -1.0]);
    s.frustum(T, [-4.0, -3.0, -0.02], 0.08, [-4.0, -3.0, -2.0], 0.08);

    // Chain at x = 6 (clear of the box): a radius step with no cone, then an
    // equal-radius collinear join whose rings must coincide.
    s.frustum(M, [6.0, 0.0, -1.0], 0.30, [6.0, 0.0, -2.0], 0.30);
    s.frustum(M, [6.0, 0.0, -2.0], 0.20, [6.0, 0.0, -3.0], 0.20);
    s.frustum(M, [6.0, 0.0, -3.0], 0.20, [6.0, 0.0, -4.0], 0.20);

    // Two slivers (shorter than a pixel; one also under 0.01 mm).
    let sl1 = s.frustum(M, [7.0, 0.0, -1.0], 0.10, [7.0, 0.0, -1.015], 0.10);
    let sl2 = s.frustum(M, [7.5, 0.0, -1.0], 0.10, [7.5, 0.0, -1.005], 0.10);

    // A host post and a THICKER branch whose end lands 0.2 mm off its axis.
    // End ring: centre (9.2, 0, -2), radius 0.6, in the plane x = 9.2. The
    // vertex at y = 0.6 is sqrt(0.2^2 + 0.6^2) = 0.6325 from the host axis,
    // so it stands 0.1325 proud of the host (r 0.5).
    s.frustum(M, [9.0, 0.0, -1.0], 0.50, [9.0, 0.0, -3.0], 0.50);
    s.frustum(M, [11.0, 0.0, -2.0], 0.60, [9.2, 0.0, -2.0], 0.60);

    // A frustum with one cap triangle missing: three open edges.
    let open = s.frustum(M, [-6.0, 0.0, -1.0], 0.2, [-6.0, 0.0, -2.0], 0.2);
    s.records[open].soup.pop();

    // Two Members whose meshes are closed boxes, not their header frusta: one
    // wound outward, one inward.
    let good_box = s.frustum(M, [12.0, 0.0, -1.0], 0.2, [12.0, 0.0, -2.0], 0.2);
    s.records[good_box].soup = box_soup([11.8, -0.2, -2.0], [12.2, 0.2, -1.0]);
    let inverted_box = s.frustum(M, [14.0, 0.0, -1.0], 0.2, [14.0, 0.0, -2.0], 0.2);
    s.records[inverted_box].soup = box_soup([13.8, -0.2, -2.0], [14.2, 0.2, -1.0])
        .into_iter()
        .map(|[a, b, c]| [a, c, b])
        .collect();

    // A member buried 1.2 mm deep in the middle of the box: every surface sample
    // is at least 1.2 mm from the model boundary (deeper than any search radius).
    let deep = s.frustum(M, [3.0, -1.0, 1.2], 0.2, [3.0, -1.0, 1.8], 0.2);

    // A contact buried in the model: ball centre 0.15 inside the underside, a
    // r 0.08 taper running outward 0.1 mm. The shaft samples outside the ball
    // sit 0.07 and 0.05 mm inside the model.
    let buried_tip = s.tip([0.0, 4.0, 0.15], 0.10, [0.0, 0.0, -1.0]);
    s.frustum(T, [0.0, 4.0, 0.15], 0.08, [0.0, 4.0, 0.05], 0.08);

    // A thin branch whose end lands inside the emitted box of `good_box` but
    // outside that record's header cylinder (radius 0.2 about x = 12, y = 0):
    // (12.15, 0.15) is 0.212 from the axis.
    let landed_in_hull = s.frustum(M, [12.15, 0.15, -0.2], 0.02, [12.15, 0.15, -1.5], 0.02);

    let file = SynthFile::new().with_model(SynthModel {
        name: "planted".into(),
        translate_mm: [0.0, 0.0, 5.0],
        mesh: box_soup([-5.0, -5.0, 0.0], [5.0, 5.0, 3.0]),
        records: s.records,
    });
    (
        file.to_bytes(),
        Ids {
            head1_tip,
            head2_tip,
            post1,
            intruder,
            tilted_tip,
            square_tip,
            slivers: vec![sl1, sl2],
            good_box,
            inverted_box,
            deep,
            buried_tip,
            landed_in_hull,
        },
    )
}

fn analyse_planted() -> (ModelGeometry, Ids) {
    let (bytes, ids) = build();
    let file = ChituboxFile::parse(Path::new("planted.chitubox"), &bytes).expect("parse");
    let params = Params {
        threads: 2,
        ..Params::default()
    };
    let mut fg = analyse(&file, params);
    (fg.models.remove(0), ids)
}

#[test]
fn the_planted_heads_are_measured_from_the_drawn_records() {
    let (m, ids) = analyse_planted();
    assert_eq!(m.heads.len(), 2, "two heads reach a joint");
    let h1 = m.heads.iter().find(|h| h.tip == ids.head1_tip).expect("head 1");
    let h2 = m.heads.iter().find(|h| h.tip == ids.head2_tip).expect("head 2");

    // Ball centre 0.03 outside the underside plane.
    assert!((h1.ball_height_mm.expect("sign") - 0.03).abs() < 1e-4, "{:?}", h1.ball_height_mm);
    // The cone at the ball exit (0.15 + ...) is already wider than R 0.1.
    assert!(h1.link_mm.abs() < 1e-9, "link {}", h1.link_mm);
    // Ring: radius 0.15 about the centre in the plane z = -0.03; the nearest
    // triangle is the underside, 0.03 away.
    assert!((h1.ring_clearance_mm - 0.03).abs() < 1e-4, "{}", h1.ring_clearance_mm);
    assert!((h1.joint_r_mm - 0.32).abs() < 1e-6);
    assert_eq!(h1.post, Some(ids.post1));
    let lean = (0.7f64 / 2.0).atan().to_degrees();
    assert!((h1.post_lean_deg.expect("lean") - lean).abs() < 1e-3);
    // phi': head chord (0.3, 0, -0.97) against the post (0.7, 0, -2.0).
    let a = (0.3f64 / 0.97).atan();
    let b = (0.7f64 / 2.0).atan();
    assert!((h1.phi_deg.expect("phi") - (b - a).abs().to_degrees()).abs() < 1e-3);

    // Head 2: a thin link, the bare constant-radius run from where it leaves
    // the ball sphere to where the cone starts (link definition of 83866bf).
    let want = 0.95 - (0.2f64 * 0.2 - 0.1 * 0.1).sqrt();
    assert!((h2.link_mm - want).abs() < 1e-4, "link {} want {want}", h2.link_mm);
    assert!((h2.link_r_mm - 0.1).abs() < 1e-6, "link radius {}", h2.link_r_mm);
    assert!(h2.post_lean_deg.expect("lean").abs() < 1e-6, "vertical post");
}

#[test]
fn the_planted_intrusion_is_found_in_the_header_geometry() {
    let (m, ids) = analyse_planted();
    let r = m
        .intrusion
        .ranked
        .iter()
        .find(|r| r.record == ids.intruder)
        .expect("the intruder is ranked");
    // The end cap centre is 1.0 above the underside and 2.0 from every other face.
    assert!((r.depth_mm - 1.0).abs() < 1e-3, "{}", r.depth_mm);
    // Three records enter the model: the intruder, the buried member and the
    // buried contact's taper (beyond its own ball).
    assert_eq!(m.intrusion.count_above, 3, "{:?}", m.intrusion.ranked);

    // A member wholly inside the box, at least 1.2 mm from every face and 1.5 mm
    // at its middle ring: the depth is not capped by any search radius.
    let deep = m
        .intrusion
        .ranked
        .iter()
        .find(|r| r.record == ids.deep)
        .expect("the buried member is ranked");
    assert!((deep.depth_mm - 1.5).abs() < 1e-3, "{}", deep.depth_mm);
}

#[test]
fn the_planted_junctions_are_counted() {
    let (m, ids) = analyse_planted();
    let t = &m.topology;
    // The writer's UV spheres are open at the poles (a known defect of the test
    // writer, seven balls here), and one frustum has a cap triangle removed: all
    // eight are found, the other frusta are closed.
    assert_eq!(t.mesh.open_records, 7 + 1, "open edges {}", t.mesh.open_edges);
    assert!(t.mesh.open_edges >= 3);
    // An inward-wound closed record has a volume <= 0; the outward one does not.
    // (The writer's own frusta wind inconsistently, so the list has more.)
    assert!(t.mesh.nonpositive_volume.contains(&ids.inverted_box));
    assert!(!t.mesh.nonpositive_volume.contains(&ids.good_box));
    // A Member whose mesh is not its header frustum is a hull record; its
    // lateral facets here are the four vertical box sides (n_z = 0).
    let hull = t.hulls.iter().find(|h| h.record == ids.good_box).expect("hull");
    assert!(hull.min_lateral_nz.expect("nz").abs() < 1e-9);
    assert!(
        t.hulls.iter().all(|h| h.record == ids.good_box || h.record == ids.inverted_box),
        "{:?}",
        t.hulls
    );
    // One step of 0.1 mm (0.30 -> 0.20) with no cone, on a straight chain.
    assert_eq!(t.flat_shoulders().len(), 1, "{:?}", t.shoulders);
    // The 0.20 -> 0.20 collinear join exists and its rings coincide.
    assert!(t.ring_pairs_checked >= 1);
    assert!(t.ring_mismatches.is_empty(), "{:?}", t.ring_mismatches);
    // Two slivers under a pixel; one also under 0.01 mm.
    let under_pixel: Vec<usize> = t.slivers_pixel.iter().map(|x| x.0).collect();
    assert_eq!(under_pixel, ids.slivers);
    assert_eq!(t.slivers_tiny.len(), 1);
    // The thick branch stands 0.1325 proud of its host.
    let proud: Vec<f64> = t.landed.iter().map(|l| l.proud_mm).filter(|&p| p > 0.05).collect();
    assert_eq!(proud.len(), 1, "{:?}", t.landed);
    let want = (0.2f64 * 0.2 + 0.6 * 0.6).sqrt() - 0.5;
    assert!((proud[0] - want).abs() < 1e-3, "got {} want {want}", proud[0]);

    // A thin branch ends inside the EMITTED box of `good_box` (a hull-like
    // record) though outside its header cylinder: it is a landed end, and its
    // ring is wholly inside the host.
    let l = t
        .landed
        .iter()
        .find(|l| l.record == ids.landed_in_hull && l.slot == 1)
        .unwrap_or_else(|| panic!("landed end in the emitted box: {:?}", t.landed));
    assert_eq!(l.host, Some(ids.good_box));
    assert!(l.ring_vertices > 0 && l.proud_mm < 1e-9 && l.proud_host_mm < 1e-9, "{l:?}");
}

#[test]
fn the_planted_straight_contacts_are_graded_by_the_skim_law() {
    let (m, ids) = analyse_planted();
    let c = &m.contacts;
    assert_eq!(c.straight, 3, "the two heads are not straight contacts");
    let tilted = c.rows.iter().find(|r| r.tip == ids.tilted_tip).expect("tilted");
    let square = c.rows.iter().find(|r| r.tip == ids.square_tip).expect("square");
    assert!((tilted.off_normal_deg.expect("x") - 50.0).abs() < 0.5, "{:?}", tilted.off_normal_deg);
    assert!(tilted.skims, "(0.3 + 0.08)(sec 50 - 1) = 0.21 > 0.069");
    assert!(square.off_normal_deg.expect("x").abs() < 0.5);
    assert!(!square.skims);
    assert_eq!(c.off_normal_beyond_skim, 1);
    // The square arrival's shaft is r 0.08 and leaves the plane straight away;
    // the tilted one stays nearer.
    let (sq, tl) = (
        square.shaft_clearance_mm.expect("clearance"),
        tilted.shaft_clearance_mm.expect("clearance"),
    );
    assert!(sq > tl, "square {sq} tilted {tl}");
    assert!((tilted.ball_height_mm.expect("h") - 0.02).abs() < 1e-4);

    // A buried contact: the shaft leaves the ball sphere (R 0.1) at 0.06 mm
    // along, where it is 0.09 mm inside the underside, and ends 0.05 inside.
    // The clearance is NEGATIVE (about -0.09; the ring spacing resolves it to
    // within 0.01), not the +0.05 an unsigned distance would give.
    let buried = c.rows.iter().find(|r| r.tip == ids.buried_tip).expect("buried");
    assert!((buried.ball_height_mm.expect("h") + 0.15).abs() < 1e-4);
    let bc = buried.shaft_clearance_mm.expect("clearance");
    assert!((bc + 0.09).abs() < 0.01, "buried shaft clearance {bc}");
}

// ---------------------------------------------------------------------------
// The v19 files
// ---------------------------------------------------------------------------

fn v19_dir() -> PathBuf {
    std::env::var_os("PLINTH_V19_DIR").map_or_else(
        || {
            PathBuf::from(
                "/Users/craig/code/hughescr/dnd-campaigns/plinth-evidence/slice-sweep-2026-09-02/field-2026-09-28/for-craig-v19",
            )
        },
        PathBuf::from,
    )
}

fn v19(stem: &str) -> Option<ModelGeometry> {
    let p = v19_dir().join(stem).join(format!("{stem}.chitubox"));
    if !p.exists() {
        eprintln!("SKIP {stem}: {} is absent (run outputs are not in git)", p.display());
        return None;
    }
    let mut fg = analyse_file(&p, Params::default()).expect("analyse");
    Some(fg.models.remove(0))
}

#[test]
fn v19_cylinder_reproduces_the_diagnosis_anchors() {
    let Some(m) = v19("Cylinder") else { return };
    let h = m.headline();
    // Heads: 33, a 0 mm link on every one, the ring clearance the diagnosis gave.
    assert_eq!(h.heads, 33);
    assert_eq!(h.link_zero, 33);
    assert!(h.ring_min < 0.005, "min {}", h.ring_min);
    assert!((h.ring_median.expect("median") - 0.020).abs() < 0.005);
    assert!((h.ring_max - 0.171).abs() < 0.005, "max {}", h.ring_max);
    assert_eq!(h.ring_flush, 14, "14 of 33 under 0.01 mm");

    // Intrusion past the record headers (the diagnosis counted the ball sphere).
    let with_ball = |rec: usize| {
        m.intrusion
            .ranked
            .iter()
            .find(|r| r.record == rec)
            .map_or(0.0, |r| r.depth_with_ball_mm)
    };
    assert!((with_ball(459) - 0.355).abs() < 0.02, "459: {}", with_ball(459));
    assert!((with_ball(460) - 0.144).abs() < 0.02, "460: {}", with_ball(460));
    assert!((with_ball(496) - 0.248).abs() < 0.02, "496: {}", with_ball(496));
    // Beyond the contact ball the two tapers are shallower: the ball is seated.
    assert!(m.intrusion.depth_of(459) < with_ball(459) - 0.05);
    let ball458 = m.contacts.rows.iter().find(|r| r.tip == 458).expect("tip 458");
    assert!((ball458.ball_height_mm.expect("h") + 0.110).abs() < 0.01, "{:?}", ball458.ball_height_mm);

    // Topology: nothing open, nothing inverted, the five slivers under 0.01 mm.
    assert_eq!(h.open_edges, 0);
    assert_eq!(h.nonpositive_volume, 0);
    let mut tiny: Vec<usize> = m.topology.slivers_tiny.iter().map(|x| x.0).collect();
    tiny.sort_unstable();
    assert_eq!(tiny, vec![437, 475, 518, 582, 599]);

    // Post 566 leans 60.9 degrees from the joint 565.
    let head = m.heads.iter().find(|x| x.post == Some(566)).expect("head with post 566");
    assert_eq!(head.joint, 565);
    assert!((head.post_lean_deg.expect("lean") - 60.9).abs() < 0.2);

    // Off-normal straight contacts beyond the skim law: the diagnosis said 74 with
    // its own footprint radius; this tool reads r_t from the drawn taper.
    assert!((67..=81).contains(&h.off_normal_skim), "{}", h.off_normal_skim);

    // MEASURED BASELINE where the diagnosis figure is not reproduced (the
    // diagnosis' definitions were not kept; see the lane report): flat shoulders
    // 14 (anchor 15); landed ends proud >= 0.3 / 0.2 mm: 0 / 7 of every other
    // solid, 15 / 20 of the host alone (anchor 9 / 12).
    assert_eq!(h.shoulders, 14);
    assert_eq!((h.landed_proud_03, h.landed_proud_02), (0, 7));
    assert_eq!((h.landed_host_03, h.landed_host_02), (15, 20));
}

#[test]
fn v19_sphere_has_eighteen_heads_with_no_link() {
    let Some(m) = v19("Sphere") else { return };
    let h = m.headline();
    assert_eq!(h.heads, 18);
    assert_eq!(h.link_zero, 18);
}

#[test]
fn v19_lonely_peak_straight_shafts_clear_the_model_by_the_anchor_amounts() {
    let Some(m) = v19("make_lonely_peak") else { return };
    for (tip, want) in [(12usize, 0.043), (19, 0.013), (21, 0.022)] {
        let row = m.contacts.rows.iter().find(|r| r.tip == tip).expect("tip");
        let got = row.shaft_clearance_mm.expect("clearance");
        assert!((got - want).abs() < 0.005, "tip {tip}: got {got:.4}, anchor {want}");
    }
    assert_eq!(m.headline().off_normal_skim, 12, "the diagnosis counted 12");
}

// ---------------------------------------------------------------------------
// The consumer path: the compiled binary, its dispatch and its --json
// ---------------------------------------------------------------------------

fn run_cli(args: &[&std::ffi::OsStr]) -> (bool, String, String) {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_plinth-audit"))
        .arg("geometry")
        .args(args)
        .env("PLINTH_THREADS", "2")
        .output()
        .expect("spawn plinth-audit");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn cli_json(path: &Path) -> plinth_audit::json::Json {
    let (ok, stdout, stderr) = run_cli(&[path.as_os_str(), std::ffi::OsStr::new("--json")]);
    assert!(ok, "geometry --json failed: {stderr}");
    plinth_audit::json::parse(&stdout).unwrap_or_else(|e| panic!("bad JSON ({e:?}): {stdout}"))
}

#[test]
fn the_binary_dispatches_geometry_and_writes_json_for_the_planted_scene() {
    let (bytes, ids) = build();
    let dir = std::env::temp_dir().join(format!("plinth-audit-geometry-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("planted.chitubox");
    std::fs::write(&path, &bytes).expect("write planted file");

    let doc = cli_json(&path);
    let model = &doc.get("models").and_then(|m| m.as_array()).expect("models")[0];
    assert_eq!(model.get("heads").and_then(|h| h.as_array()).expect("heads").len(), 2);
    let ranked = model
        .get("intrusion")
        .and_then(|i| i.get("ranked"))
        .and_then(|r| r.as_array())
        .expect("ranked");
    let deep = ranked
        .iter()
        .find(|r| r.get("record").and_then(|x| x.as_f64()) == Some(ids.deep as f64))
        .expect("the buried member is in the JSON");
    let d = deep.get("depth_mm").and_then(|x| x.as_f64()).expect("depth");
    assert!((d - 1.5).abs() < 1e-3, "{d}");
    let landed = model
        .get("topology")
        .and_then(|t| t.get("landed_ends"))
        .and_then(|l| l.as_array())
        .expect("landed_ends");
    assert!(
        landed
            .iter()
            .any(|l| l.get("record").and_then(|x| x.as_f64()) == Some(ids.landed_in_hull as f64)),
        "the end inside the emitted box is a landed end"
    );

    // The human summary is the default output and names the file.
    let (ok, text, stderr) = run_cli(&[path.as_os_str()]);
    assert!(ok, "{stderr}");
    assert!(text.contains("planted.chitubox") && text.contains("sha256"), "{text}");

    // A bad path is a usage error, not a crash.
    let (ok, _, stderr) = run_cli(&[std::ffi::OsStr::new("/nonexistent/none.chitubox")]);
    assert!(!ok && stderr.contains("plinth-audit geometry"), "{stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A file the real emitter wrote (committed under `corpus/baseline`), through
/// the real binary. It has no joint records, so it has no heads; what matters
/// is that the reader, the four reports and the JSON all accept real bytes.
#[test]
fn the_binary_reads_a_file_the_real_emitter_wrote() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("corpus")
        .join("baseline")
        .join("Sphere.chitubox");
    if !p.exists() {
        eprintln!("SKIP: {} is absent", p.display());
        return;
    }
    let doc = cli_json(&p);
    let model = &doc.get("models").and_then(|m| m.as_array()).expect("models")[0];
    let records = model.get("records").and_then(|r| r.as_f64()).expect("records");
    assert!(records > 0.0, "the real file has support records");
    for key in ["heads", "intrusion", "topology", "contacts"] {
        assert!(model.get(key).is_some(), "missing {key}");
    }
}

#[test]
fn the_binary_prints_the_baseline_table_for_the_v19_files() {
    let dir = v19_dir();
    let files: Vec<PathBuf> = ["Cylinder", "Sphere"]
        .iter()
        .map(|s| dir.join(s).join(format!("{s}.chitubox")))
        .collect();
    if files.iter().any(|f| !f.exists()) {
        eprintln!("SKIP: v19 files absent under {}", dir.display());
        return;
    }
    let args: Vec<&std::ffi::OsStr> = files.iter().map(|f| f.as_os_str()).collect();
    let (ok, text, stderr) = run_cli(&args);
    assert!(ok, "{stderr}");
    let row = |name: &str| {
        text.lines()
            .find(|l| l.starts_with(name) && l.contains(char::is_numeric))
            .unwrap_or_else(|| panic!("no table row for {name}:\n{text}"))
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    assert_eq!(row("Cylinder")[1], "33");
    assert_eq!(row("Sphere")[1], "18");
}
