//! **Every arm that lands on the model ends in a FOOT, cut to the model mesh,
//! wholly on the face it lands on** — the production slice-sweep route, end to
//! end, read back from the bytes on disk (lane 2, 2026-10-12,
//! `plinth-evidence/.../quality-2026-10-10/joint/MODEL-FEET.md`).
//!
//! # The defect this pins
//!
//! Until 2026-10-12 an arm that landed on the model ended in its shaft's own
//! flat end: no ball, no trim, the shaft's widest section. On Deinonychus 7 of
//! 16 such feet had under 80 % of their end on the model, up to 0.29 mm of it
//! hanging in the air, and the widest were 49-75 times the area their load
//! needs.
//!
//! # The fixture
//!
//! Nine 1.5 x 1.5 x 0.5 mm plates on a 5 mm lattice, floating 4 mm over a
//! 20 x 20 x 2 mm block (the print-scale window wants a model of miniature
//! size), pinned at the authored pose. Each plate's underside is carried by
//! arms that descend onto the block's top face: model landings, every one of
//! which has room for its whole foot. Procedural, so it runs in a fresh clone.
//!
//! # What is graded, and how the file is read
//!
//! Only the bytes: the model's triangles and the support records. A FOOT is a
//! `TIP` (type 7) record whose centre is the LOWER end of another record (the
//! cone, or the shaft's last span, runs down into it); a top tip is the UPPER
//! end of its taper. Tolerance 1e-3 mm (the tip-trim brief's).
//!
//! 1. there are feet at all (a floor on the count, so the test cannot pass on
//!    an empty field), and every foot's own mesh has no vertex inside the
//!    model and is ONE connected piece;
//! 2. every foot touches the model, and every vertex of it that lies on the
//!    model surface lies on an UP-facing face (the block's top), so the foot's
//!    contact is on the face it lands on and not hanging off it;
//! 3. the record that runs into a foot never widens into it.
//!
//! The sabotage drills for the rule are at the lowering
//! (`plinth_support::lower::model_foot_tests`, the flat end) and at the
//! descent (`plinth_sweep::up_sweep::foot_fit_tests`, a zero foot).

use std::path::PathBuf;
use std::sync::OnceLock;

use plinth_pipeline::{
    prepare, registered_decimation_budget, registered_support_request, ModelPreparationOutcome,
    OrientationPolicy, OutputProfile, PrepareRequest,
};

const TOLERANCE_MM: f64 = 1e-3;
const STEM: &str = "floating_plate_over_block";
const IDENTITY_PIN: &str = r#"{"stages":[{"name":"hollow","status":"ok","report":{"oriented":true,"orientation":{"axis":[1,0,0],"deg":0}}}]}"#;
/// Nine plates, each landing at least one arm on the block. Set at or under
/// the count measured, so it bounds "graded something real", never the answer.
const MIN_FEET: usize = 9;

type Point = [f64; 3];
type Triangle = [Point; 3];

struct Record {
    start: Point,
    end: Point,
    r_start: f64,
    r_end: f64,
    tip: bool,
    vertices: Vec<Point>,
}

struct Emitted {
    model: Vec<Triangle>,
    records: Vec<Record>,
    trim: plinth_emit::SeatTrimReport,
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("plinth-model-feet-{}", std::process::id()));
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

fn fixture() -> plinth_mesh::Mesh {
    let block = plinth_mesh::make_box([20.0, 20.0, 2.0], [0.0, 0.0, 0.0]).expect("the block");
    let mut verts = block.verts().to_vec();
    let mut tris = block.tris().to_vec();
    // Nine small plates on a 5 mm lattice, each carried by its own arms down
    // to the block, so the field has several independent model feet.
    for x in [-5.0, 0.0, 5.0] {
        for y in [-5.0, 0.0, 5.0] {
            let plate = plinth_mesh::make_box([1.5, 1.5, 0.5], [x, y, 6.0]).expect("a plate");
            let offset = u32::try_from(verts.len()).expect("a small mesh");
            verts.extend_from_slice(plate.verts());
            tris.extend(plate.tris().iter().map(|t| t.map(|corner| corner + offset)));
        }
    }
    plinth_mesh::Mesh::new(verts, tris).expect("ten closed boxes")
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
            STEM,
            plinth_orient::PinnedBaseline {
                pipeline_report: scratch.0.join("pins").join(format!("{STEM}.pipeline.json")),
            },
        ),
        no_orient: Default::default(),
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
        plinth_mesh::write_binary_stl(scratch.0.join("in").join(format!("{STEM}.stl")), &fixture())
            .expect("write the fixture");
        std::fs::write(
            scratch.0.join("pins").join(format!("{STEM}.pipeline.json")),
            IDENTITY_PIN,
        )
        .expect("write the identity pin");
        let report = prepare(request(&scratch)).expect("the sweep route emits and warns");
        let entry = report
            .models
            .iter()
            .find(|entry| entry.model.as_str() == STEM)
            .expect("the fixture is in the report");
        let ModelPreparationOutcome::Artifact { emit, .. } = &entry.outcome else {
            panic!("the fixture must reach an artifact; it reported {:?}", entry.outcome);
        };
        let project =
            chitubox_io::Project::open(&emit.artifact_path).expect("the artifact must reopen");
        let model: Vec<Triangle> = points(&project.model_triangles(0).expect("model triangles"))
            .chunks_exact(3)
            .map(|t| [t[0], t[1], t[2]])
            .collect();
        let records = project
            .supports(0)
            .expect("support records")
            .iter()
            .map(|record| Record {
                start: f64_point(record.start),
                end: f64_point(record.end),
                r_start: f64::from(record.r_start),
                r_end: f64::from(record.r_end),
                tip: record.segment_type == chitubox_io::SegmentType::TIP,
                vertices: points(&record.mesh.xyz),
            })
            .collect();
        Emitted {
            model,
            records,
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
    dot(sub(a, b), sub(a, b)).sqrt()
}

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

/// The nearest model triangle to `p` and the distance to it.
fn nearest(model: &[Triangle], p: Point) -> (usize, f64) {
    model
        .iter()
        .enumerate()
        .map(|(index, triangle)| (index, distance(p, closest_on_triangle(p, *triangle))))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("a model")
}

/// Ray parity along +Z: whether `p` is inside the closed model.
fn inside(model: &[Triangle], p: Point) -> bool {
    let (px, py) = (p[0] + 1.234_567e-7, p[1] + 7.654_321e-8);
    let mut crossings = 0_usize;
    for [a, b, c] in model {
        let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
        if d == 0.0 {
            continue;
        }
        let l1 = ((b[1] - c[1]) * (px - c[0]) + (c[0] - b[0]) * (py - c[1])) / d;
        let l2 = ((c[1] - a[1]) * (px - c[0]) + (a[0] - c[0]) * (py - c[1])) / d;
        let l3 = 1.0 - l1 - l2;
        if l1 < 0.0 || l2 < 0.0 || l3 < 0.0 {
            continue;
        }
        if l1 * a[2] + l2 * b[2] + l3 * c[2] > p[2] {
            crossings += 1;
        }
    }
    crossings % 2 == 1
}

/// Connected pieces of a triangle soup, joined by shared corner positions.
fn pieces(vertices: &[Point]) -> usize {
    let mut parent: Vec<usize> = (0..vertices.len() / 3).collect();
    fn find(parent: &mut [usize], mut node: usize) -> usize {
        while parent[node] != node {
            parent[node] = parent[parent[node]];
            node = parent[node];
        }
        node
    }
    let mut first: std::collections::HashMap<[u64; 3], usize> = std::collections::HashMap::new();
    for (index, vertex) in vertices.iter().enumerate() {
        let key = vertex.map(f64::to_bits);
        let triangle = index / 3;
        match first.get(&key) {
            Some(&other) => {
                let (x, y) = (find(&mut parent, triangle), find(&mut parent, other));
                if x != y {
                    parent[x.max(y)] = x.min(y);
                }
            }
            None => {
                first.insert(key, triangle);
            }
        }
    }
    (0..parent.len())
        .filter(|&node| find(&mut parent, node) == node)
        .count()
}

/// Every foot: the TIP record whose centre is the LOWER end of another record,
/// with that record's index.
fn feet(file: &Emitted) -> Vec<(usize, usize)> {
    file.records
        .iter()
        .enumerate()
        .filter(|(_, record)| record.tip)
        .filter_map(|(index, ball)| {
            let centre: Point = std::array::from_fn(|axis| 0.5 * (ball.start[axis] + ball.end[axis]));
            file.records
                .iter()
                .enumerate()
                .find(|(_, other)| {
                    !other.tip
                        && distance(other.end, centre) <= 1e-4
                        && other.start[2] > other.end[2]
                })
                .map(|(above, _)| (index, above))
        })
        .collect()
}

/// Rule 1: there are feet, and each is cut to the model in one piece.
#[test]
fn every_model_foot_in_the_file_is_cut_to_the_model_in_one_piece() {
    let file = emitted();
    let found = feet(file);
    assert!(
        found.len() >= MIN_FEET,
        "{} feet in the file, at least {MIN_FEET} expected: the arms under the plate land on \
         the block",
        found.len()
    );
    for &(ball, _) in &found {
        let vertices = &file.records[ball].vertices;
        let deepest = vertices
            .iter()
            .filter(|&&p| inside(&file.model, p))
            .map(|&p| nearest(&file.model, p).1)
            .fold(0.0_f64, f64::max);
        assert!(deepest <= TOLERANCE_MM, "foot record {ball} reaches {deepest} mm into the model");
        assert_eq!(pieces(vertices), 1, "foot record {ball} is more than one piece");
    }
    assert!(file.trim.batch_error.is_none(), "{:?}", file.trim.batch_error);
}

/// Rule 2: every foot touches the model, on an up-facing face.
#[test]
fn every_model_foot_touches_the_face_it_lands_on() {
    let file = emitted();
    for (ball, _) in feet(file) {
        let mut touching = 0_usize;
        for &p in &file.records[ball].vertices {
            let (triangle, gap) = nearest(&file.model, p);
            if gap > TOLERANCE_MM {
                continue;
            }
            touching += 1;
            let [a, b, c] = file.model[triangle];
            let normal = cross(sub(b, a), sub(c, a));
            assert!(
                normal[2] > 0.0,
                "foot record {ball} touches a face that does not look up ({normal:?})"
            );
        }
        assert!(touching > 0, "foot record {ball} does not touch the model");
    }
}

/// Rule 3: the record that runs into a foot never widens into it: the post
/// keeps its width in its span and narrows (or holds) to the foot.
#[test]
fn every_model_foot_is_reached_by_a_narrowing_record() {
    let file = emitted();
    for (ball, above) in feet(file) {
        let into = &file.records[above];
        assert!(
            into.r_end <= into.r_start + TOLERANCE_MM,
            "the record above foot {ball} widens into it ({} -> {})",
            into.r_start,
            into.r_end
        );
    }
}
