//! **Every tip and every first shaft segment stays on the SUPPORT side of the
//! face it seats on** — the production slice-sweep route, end to end, read
//! back from the bytes on disk. ChituBox renders OUR mesh, so the mesh is what
//! is graded here, never the producer's own account of it.
//!
//! # The defects this pins (owner, ChituBox, 2026-09-28)
//!
//! `plinth-evidence/slice-sweep-2026-09-02/field-2026-09-28/tip-clip.md`
//! measured them on `Round 25` at the flip (the stiffener ring DOWN, the pose
//! the owner ruled correct):
//!
//! 1. every tip ball was emitted WHOLE, buried in the model (105 of 105);
//! 2. the only clip was a horizontal "far face" plane, so 14 rim balls hung
//!    past the disc's knife edge with their tops cut flat, and the taper
//!    (post) under each of them rose through that plane — "a chunk of post";
//! 3. 68 tapers had vertices inside the model (up to 0.68 mm deep).
//!
//! # The rules
//!
//! * **Rule 2 (trim).** A tip ball and the first shaft segment of its arm keep
//!   only `primitive ∩ {support side of the seat face's plane} ∖ model solid`.
//! * **Rule 1 (footprint).** The whole contact disc that trim leaves lies ON
//!   the seat face: a seat whose disc would hang off an edge is moved onto the
//!   face; only a true knife edge keeps an overhang, with a WARNING.
//!
//! # What is graded, and how the file is read
//!
//! Only the bytes: the model's own triangles and every `TIP` (type 7) and
//! `TAPER` (type 1) record, all in the file's one model-centred frame. The
//! grader knows nothing the producer computed. The seat faces are found in the
//! file's model mesh (its down-facing horizontal faces: the stiffener ring's
//! underside and the recess ceiling); a tip belongs to the highest such plane
//! at or below its own upper pole. A taper belongs to the tip whose ball
//! centre is its top end. Tolerance 1e-3 mm throughout (the brief's).
//!
//! 1. no tip/taper vertex lies inside the model by more than the tolerance
//!    (parity of a ray cast, AND farther than the tolerance from every model
//!    triangle, so a vertex ON the surface is never counted);
//! 2. no tip/taper vertex lies on the MODEL side of its tip's seat plane;
//! 3. no tip/taper vertex lies outside the model's silhouette (its plan-view
//!    convex hull; the disc is convex in plan) while above the underside plane;
//! 4. (rule 1) every tip vertex lying ON its seat plane lies on a down-facing
//!    model face of that plane — the contact disc does not hang off the face.
//!
//! The fixture is the PROCEDURAL `Round 25` (the same `make_recessed_disc`
//! dimensions `plate_contact.rs` and `round_bases_ratchet.rs` use), pinned at
//! the flip — 180° about X — so this does not depend on which pose the dense
//! search picks, and runs in a fresh clone with no provisioned corpus.

use std::path::PathBuf;
use std::sync::OnceLock;

use plinth_pipeline::{
    prepare, registered_decimation_budget, registered_support_request, ModelPreparationOutcome,
    OrientationPolicy, OutputProfile, PrepareRequest,
};

/// The brief's resolution for every geometric test in this file.
const TOLERANCE_MM: f64 = 1e-3;

/// Field-size floors, so an empty or shrunken field cannot pass. Set from the
/// pre-fix run of this very fixture (88 tips, 88 first segments; rim count in
/// the lane report), well under it, so they bound "graded something real",
/// never the answer.
///
/// RE-PINNED 2026-10-01 (land of fix/chitubox-geometry onto develop 3372093):
/// rim tips 10 → 3. The rim band is one ball diameter, so it narrows with the
/// owner's ChituBox ball ratio (1.27-1.33 × r here, where it was 1.5), and the
/// straight-line reach places fewer tips near the edge. Measured rim tips on
/// this fixture: 25 of 88 at fix/chitubox-geometry 4fd22f0, 11 of 65 at
/// develop 3372093, 5 of 65 on the merged tree, with 0 rule-2 violations. The
/// minimum is set well under 5 and still bounds "graded something real".
const MIN_GRADED_TIPS: usize = 60;
const MIN_GRADED_RIM_TIPS: usize = 3;

const ROUND_25_BOTTOM_RADIUS_MM: f64 = 11.5;
const ROUND_25_TOP_RADIUS_MM: f64 = 12.5;
const ROUND_25_HEIGHT_MM: f64 = 3.5;
const ROUND_25_RECESS_DEPTH_MM: f64 = 2.5;
const ROUND_25_RECESS_RADIUS_MM: f64 = 10.0;
const ROUND_25_SEGMENTS: usize = 96;

/// The flip: 180° about X, so the authored top (the stiffener ring) faces the
/// plate. Written as the legacy pinned-orientation record the CLI's
/// `--orient pinned:<dir>` reads.
const FLIP_PIN: &str = r#"{"stages":[{"name":"hollow","status":"ok","report":{"oriented":true,"orientation":{"axis":[1,0,0],"deg":180}}}]}"#;

type Point = [f64; 3];
type Triangle = [Point; 3];

/// What the file holds, read once for every test in this module.
struct Emitted {
    model: Vec<Triangle>,
    tips: Vec<Tip>,
    tapers: Vec<Taper>,
    /// Tapers whose top is no tip's ball centre (a brace or a mast, which
    /// never seat into the model); counted, not graded.
    unmatched_tapers: usize,
    /// The emitter's own trim counters, as the run report carries them.
    trim: plinth_emit::SeatTrimReport,
}

struct Tip {
    record: usize,
    centre: Point,
    radius: f64,
    upper_pole: Point,
    vertices: Vec<Point>,
}

struct Taper {
    record: usize,
    tip: usize,
    vertices: Vec<Point>,
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("plinth-tip-trim-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("in")).expect("scratch input folder");
        std::fs::create_dir_all(path.join("pins")).expect("scratch pin folder");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "test-local f32 snap of a bounded procedural fixture, the narrowing an .stl performs"
)]
fn place_round_25(scratch: &Scratch) {
    let mesh = plinth_mesh::make_recessed_disc(
        ROUND_25_BOTTOM_RADIUS_MM,
        ROUND_25_TOP_RADIUS_MM,
        ROUND_25_HEIGHT_MM,
        ROUND_25_RECESS_DEPTH_MM,
        ROUND_25_RECESS_RADIUS_MM,
        ROUND_25_SEGMENTS,
    )
    .expect("a valid Round 25 profile");
    let verts: Vec<[f64; 3]> = mesh
        .verts()
        .iter()
        .map(|vertex| vertex.map(|value| f64::from(value as f32)))
        .collect();
    let snapped = plinth_mesh::Mesh::new(verts, mesh.tris().to_vec())
        .expect("an f32-snapped procedural fixture is still a valid mesh");
    plinth_mesh::write_binary_stl(scratch.0.join("in").join("Round_25.stl"), &snapped)
        .expect("write the Round 25 fixture");
    std::fs::write(
        scratch.0.join("pins").join("Round_25.pipeline.json"),
        FLIP_PIN,
    )
    .expect("write the flip pin");
}

fn request(scratch: &Scratch) -> PrepareRequest {
    let printer = OutputProfile::Saturn4Ultra
        .resolve()
        .expect("M0 Saturn profile")
        .clone();
    PrepareRequest {
        input_models_dir: scratch.0.join("in"),
        output_dir: scratch.0.join("out"),
        orientation: OrientationPolicy::pinned_for(
            "Round_25",
            plinth_orient::PinnedBaseline {
                pipeline_report: scratch.0.join("pins").join("Round_25.pipeline.json"),
            },
        ),
        placement: plinth_pipeline::Placement::Sweep,
        output_profile: OutputProfile::Saturn4Ultra,
        decimation_budget: registered_decimation_budget().expect("registered budget"),
        hollow: plinth_hollow::HollowRequest::with_registered_wall_default(printer.clone())
            .expect("registered wall"),
        support: registered_support_request(&printer).expect("registered lift"),
        no_cache: true,
        stop_after: None,
    }
}

fn points(xyz: &[f32]) -> Vec<Point> {
    xyz.chunks_exact(3)
        .map(|v| [f64::from(v[0]), f64::from(v[1]), f64::from(v[2])])
        .collect()
}

fn f64_point(p: [f32; 3]) -> Point {
    [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]
}

fn emitted() -> &'static Emitted {
    static EMITTED: OnceLock<Emitted> = OnceLock::new();
    EMITTED.get_or_init(|| {
        let scratch = Scratch::new();
        place_round_25(&scratch);
        let report = prepare(request(&scratch)).expect("the sweep route emits and warns");
        let entry = report
            .models
            .iter()
            .find(|entry| entry.model.as_str() == "Round_25")
            .expect("Round_25 is in the report");
        let ModelPreparationOutcome::Artifact { emit, .. } = &entry.outcome else {
            panic!(
                "Round 25 must reach an artifact; it reported {:?}",
                entry.outcome
            );
        };
        let project =
            chitubox_io::Project::open(&emit.artifact_path).expect("the artifact must reopen");
        let model: Vec<Triangle> = points(&project.model_triangles(0).expect("model triangles"))
            .chunks_exact(3)
            .map(|t| [t[0], t[1], t[2]])
            .collect();
        let records = project.supports(0).expect("support records");
        let mut tips = Vec::new();
        for (index, record) in records.iter().enumerate() {
            if record.segment_type != chitubox_io::SegmentType::TIP {
                continue;
            }
            let (a, b) = (f64_point(record.start), f64_point(record.end));
            tips.push(Tip {
                record: index,
                centre: std::array::from_fn(|axis| 0.5 * (a[axis] + b[axis])),
                radius: f64::from(record.r_start),
                upper_pole: if a[2] >= b[2] { a } else { b },
                vertices: points(&record.mesh.xyz),
            });
        }
        let mut tapers = Vec::new();
        let mut unmatched_tapers = 0;
        for (index, record) in records.iter().enumerate() {
            if record.segment_type != chitubox_io::SegmentType::TAPER {
                continue;
            }
            let top = f64_point(record.start);
            let owner = tips
                .iter()
                .position(|tip| distance(tip.centre, top) <= 1e-4);
            match owner {
                Some(tip) => tapers.push(Taper {
                    record: index,
                    tip,
                    vertices: points(&record.mesh.xyz),
                }),
                None => unmatched_tapers += 1,
            }
        }
        Emitted {
            model,
            tips,
            tapers,
            unmatched_tapers,
            trim: (*emit.seat_trim).clone(),
        }
    })
}

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Point, b: Point) -> Point {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn distance(a: Point, b: Point) -> f64 {
    let d = sub(a, b);
    dot(d, d).sqrt()
}

/// Closest point on a triangle (Ericson, Real-Time Collision Detection 5.1.5).
/// A test-side grader, independent of every producer-side geometry routine.
fn closest_on_triangle(p: Point, [a, b, c]: Triangle) -> Point {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return [a[0] + v * ab[0], a[1] + v * ab[1], a[2] + v * ab[2]];
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return [a[0] + w * ac[0], a[1] + w * ac[1], a[2] + w * ac[2]];
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return [
            b[0] + w * (c[0] - b[0]),
            b[1] + w * (c[1] - b[1]),
            b[2] + w * (c[2] - b[2]),
        ];
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    [
        a[0] + ab[0] * v + ac[0] * w,
        a[1] + ab[1] * v + ac[1] * w,
        a[2] + ab[2] * v + ac[2] * w,
    ]
}

fn distance_to_model(p: Point, model: &[Triangle]) -> f64 {
    model
        .iter()
        .map(|triangle| distance(p, closest_on_triangle(p, *triangle)))
        .fold(f64::INFINITY, f64::min)
}

/// Parity of a ray from `p` along a direction no model edge or face is
/// parallel to (Möller–Trumbore), so a ray grazing an edge is measure-zero.
fn inside_by_parity(p: Point, model: &[Triangle]) -> bool {
    const DIRECTION: Point = [0.013_7, 0.029_1, 0.999_48];
    let mut crossings = 0_usize;
    for [a, b, c] in model {
        let e1 = sub(*b, *a);
        let e2 = sub(*c, *a);
        let h = cross(DIRECTION, e2);
        let det = dot(e1, h);
        if det.abs() < 1e-15 {
            continue;
        }
        let inv = 1.0 / det;
        let s = sub(p, *a);
        let u = inv * dot(s, h);
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let q = cross(s, e1);
        let v = inv * dot(DIRECTION, q);
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        if inv * dot(e2, q) > 0.0 {
            crossings += 1;
        }
    }
    crossings % 2 == 1
}

/// The model's plan-view convex hull, counter-clockwise (Andrew's chain).
fn silhouette(model: &[Triangle]) -> Vec<[f64; 2]> {
    let mut xy: Vec<[f64; 2]> = model
        .iter()
        .flat_map(|t| t.iter().map(|p| [p[0], p[1]]))
        .collect();
    xy.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    xy.dedup();
    let turn = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut hull: Vec<[f64; 2]> = Vec::new();
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &[f64; 2]>> = if pass == 0 {
            Box::new(xy.iter())
        } else {
            Box::new(xy.iter().rev())
        };
        for &p in iter {
            while hull.len() >= start + 2
                && turn(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0
            {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

/// How far `p` lies OUTSIDE the convex hull in plan (negative inside).
fn outside_silhouette_by(p: Point, hull: &[[f64; 2]]) -> f64 {
    let mut worst = f64::NEG_INFINITY;
    for index in 0..hull.len() {
        let a = hull[index];
        let b = hull[(index + 1) % hull.len()];
        let edge = [b[0] - a[0], b[1] - a[1]];
        let length = edge[0].hypot(edge[1]);
        // Outward normal of a counter-clockwise edge.
        let outward = [edge[1] / length, -edge[0] / length];
        worst = worst.max((p[0] - a[0]) * outward[0] + (p[1] - a[1]) * outward[1]);
    }
    worst
}

/// The model's down-facing horizontal faces, grouped by plane height,
/// ascending. On this fixture: the ring's underside and the recess ceiling.
fn down_facing_planes(model: &[Triangle]) -> Vec<(f64, Vec<Triangle>)> {
    let mut planes: Vec<(f64, Vec<Triangle>)> = Vec::new();
    for triangle in model {
        let normal = cross(sub(triangle[1], triangle[0]), sub(triangle[2], triangle[0]));
        let length = dot(normal, normal).sqrt();
        if length <= 0.0 || normal[2] / length > -0.999_999 {
            continue;
        }
        let z = (triangle[0][2] + triangle[1][2] + triangle[2][2]) / 3.0;
        match planes
            .iter_mut()
            .find(|(plane, _)| (plane - z).abs() <= 1e-4)
        {
            Some((_, faces)) => faces.push(*triangle),
            None => planes.push((z, vec![*triangle])),
        }
    }
    planes.sort_by(|a, b| a.0.total_cmp(&b.0));
    planes
}

/// Plan-view distance from `p` to a triangle's plan projection (0 inside).
fn plan_distance_to_triangle(p: Point, triangle: &Triangle) -> f64 {
    let flat = |q: Point| [q[0], q[1], 0.0];
    distance(flat(p), closest_on_triangle(flat(p), triangle.map(flat)))
}

/// The seat plane of each tip: the highest down-facing plane at or below its
/// upper pole.
fn seat_planes(emitted: &Emitted, planes: &[(f64, Vec<Triangle>)]) -> Vec<usize> {
    emitted
        .tips
        .iter()
        .map(|tip| {
            planes
                .iter()
                .rposition(|(z, _)| *z <= tip.upper_pole[2] + TOLERANCE_MM)
                .unwrap_or_else(|| {
                    panic!(
                        "tip record {} (upper pole z {:.4}) is below every down-facing face",
                        tip.record, tip.upper_pole[2]
                    )
                })
        })
        .collect()
}

#[derive(Default)]
struct Tally {
    records: usize,
    worst_mm: f64,
    first: Option<String>,
}

impl Tally {
    fn add(&mut self, record: usize, by_mm: f64, what: &str) {
        self.records += 1;
        if by_mm > self.worst_mm {
            self.worst_mm = by_mm;
        }
        if self.first.is_none() {
            self.first = Some(format!("record {record}: {what} by {by_mm:.4} mm"));
        }
    }
}

/// One record's vertices graded against the three rule-2 checks.
fn grade_rule_2(
    record: usize,
    vertices: &[Point],
    plane_z: f64,
    underside_z: f64,
    emitted: &Emitted,
    hull: &[[f64; 2]],
    tallies: &mut [Tally; 3],
) {
    let mut inside = 0.0_f64;
    let mut model_side = 0.0_f64;
    let mut outside = 0.0_f64;
    for &p in vertices {
        if inside_by_parity(p, &emitted.model) {
            let depth = distance_to_model(p, &emitted.model);
            if depth > TOLERANCE_MM {
                inside = inside.max(depth);
            }
        }
        let above = p[2] - plane_z;
        if above > TOLERANCE_MM {
            model_side = model_side.max(above);
        }
        let out = outside_silhouette_by(p, hull);
        if out > TOLERANCE_MM && p[2] > underside_z + TOLERANCE_MM {
            outside = outside.max(out);
        }
    }
    if inside > 0.0 {
        tallies[0].add(record, inside, "vertex inside the model");
    }
    if model_side > 0.0 {
        tallies[1].add(
            record,
            model_side,
            "vertex on the model side of the seat plane",
        );
    }
    if outside > 0.0 {
        tallies[2].add(
            record,
            outside,
            "vertex outside the silhouette above the underside",
        );
    }
}

#[test]
fn round_25_at_the_flip_keeps_every_tip_and_first_segment_on_the_support_side_of_its_seat() {
    let emitted = emitted();
    let planes = down_facing_planes(&emitted.model);
    let hull = silhouette(&emitted.model);
    let underside_z = emitted
        .model
        .iter()
        .flat_map(|t| t.iter().map(|p| p[2]))
        .fold(f64::INFINITY, f64::min);
    assert!(
        planes.len() >= 2,
        "the flipped disc has two down-facing faces (ring underside, recess ceiling); found {}",
        planes.len()
    );
    let seats = seat_planes(emitted, &planes);

    // A grader that saw nothing proves nothing. Rim tips are the population
    // the defect lives in: a ball centre within one ball diameter of the edge.
    let rim_tips = emitted
        .tips
        .iter()
        .filter(|tip| -outside_silhouette_by(tip.centre, &hull) <= 2.0 * tip.radius)
        .count();

    let mut tip_tallies: [Tally; 3] = Default::default();
    for (tip, &seat) in emitted.tips.iter().zip(&seats) {
        grade_rule_2(
            tip.record,
            &tip.vertices,
            planes[seat].0,
            underside_z,
            emitted,
            &hull,
            &mut tip_tallies,
        );
    }
    let mut taper_tallies: [Tally; 3] = Default::default();
    for taper in &emitted.tapers {
        grade_rule_2(
            taper.record,
            &taper.vertices,
            planes[seats[taper.tip]].0,
            underside_z,
            emitted,
            &hull,
            &mut taper_tallies,
        );
    }
    let summary = |label: &str, tallies: &[Tally; 3]| {
        format!(
            "{label}: inside the model {} (worst {:.4} mm; {:?}), model side of the seat plane {} \
             (worst {:.4} mm; {:?}), outside the silhouette above the underside {} (worst {:.4} \
             mm; {:?})",
            tallies[0].records,
            tallies[0].worst_mm,
            tallies[0].first,
            tallies[1].records,
            tallies[1].worst_mm,
            tallies[1].first,
            tallies[2].records,
            tallies[2].worst_mm,
            tallies[2].first,
        )
    };
    println!(
        "graded {} tips ({rim_tips} rim), {} first segments ({} tapers of braces/masts not graded); \
         seat planes {:?}; emitter trim path hits: {:?}",
        emitted.tips.len(),
        emitted.tapers.len(),
        emitted.unmatched_tapers,
        planes.iter().map(|(z, faces)| (*z, faces.len())).collect::<Vec<_>>(),
        emitted.trim,
    );
    println!("{}", summary("tips", &tip_tallies));
    println!("{}", summary("first segments", &taper_tallies));
    assert!(
        emitted.tips.len() >= MIN_GRADED_TIPS && emitted.tapers.len() >= MIN_GRADED_TIPS,
        "graded only {} tips and {} first segments",
        emitted.tips.len(),
        emitted.tapers.len()
    );
    assert!(
        rim_tips >= MIN_GRADED_RIM_TIPS,
        "graded only {rim_tips} rim tips (ball centre within a ball diameter of the edge)"
    );
    let failing: usize = tip_tallies
        .iter()
        .chain(taper_tallies.iter())
        .map(|tally| tally.records)
        .sum();
    assert_eq!(
        failing,
        0,
        "rule 2 violated:\n{}\n{}",
        summary("tips", &tip_tallies),
        summary("first segments", &taper_tallies)
    );
    // The path fired: every graded tip and first segment went through the
    // emitter's one batched trim.
    assert!(
        emitted.trim.records_trimmed >= emitted.tips.len() + emitted.tapers.len(),
        "the trim path did not fire for every graded record: {:?}",
        emitted.trim
    );
}

#[test]
fn round_25_at_the_flip_seats_every_contact_disc_wholly_on_its_face() {
    let emitted = emitted();
    let planes = down_facing_planes(&emitted.model);
    let seats = seat_planes(emitted, &planes);
    let mut off_face = Tally::default();
    let mut on_plane_vertices = 0_usize;
    let mut tips_with_a_disc = 0_usize;
    for (tip, &seat) in emitted.tips.iter().zip(&seats) {
        let (plane_z, faces) = (&planes[seat].0, &planes[seat].1);
        let mut worst = 0.0_f64;
        let mut any = false;
        for &p in &tip.vertices {
            if (p[2] - plane_z).abs() > TOLERANCE_MM {
                continue;
            }
            any = true;
            on_plane_vertices += 1;
            let off = faces
                .iter()
                .map(|face| plan_distance_to_triangle(p, face))
                .fold(f64::INFINITY, f64::min);
            if off > TOLERANCE_MM {
                worst = worst.max(off);
            }
        }
        tips_with_a_disc += usize::from(any);
        if worst > 0.0 {
            off_face.add(tip.record, worst, "contact-disc vertex off its seat face");
        }
    }
    println!(
        "{tips_with_a_disc} of {} tips have a contact disc on their seat plane ({on_plane_vertices} \
         vertices); {} discs hang off their face (worst {:.4} mm; {:?})",
        emitted.tips.len(),
        off_face.records,
        off_face.worst_mm,
        off_face.first
    );
    assert_eq!(
        tips_with_a_disc,
        emitted.tips.len(),
        "every tip must meet its seat face in a contact disc"
    );
    assert_eq!(
        off_face.records, 0,
        "rule 1 violated: {} contact discs hang off their face (worst {:.4} mm; {:?})",
        off_face.records, off_face.worst_mm, off_face.first
    );
}
