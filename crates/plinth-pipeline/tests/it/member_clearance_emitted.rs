//! **No support solid in the FILE pierces the model, and no brace or mast in
//! the file comes within the final clearance gap of it** — the Cylinder's
//! complaint ("the very highest supports pierce the model"), graded on the
//! bytes of the Cylinder's analogue (fix/member-clearance, 2026-09-29).
//!
//! `plinth-evidence/slice-sweep-2026-09-02/field-2026-09-28/brace-collide-diagnosis.md`
//! measured on develop `c33dc8e`: braces on the Cylinder 0.011-0.091 mm INSIDE
//! the model, a mast 0.154 mm inside, and 163 records kept whole inside the
//! model by the seat trim; after wall creep (develop `bece991`) the emitted
//! Cylinder still has 4 braces 0.10-0.20 mm from the model and the laid tube
//! one 0.047 mm from it (`lanes/member-clearance/probe/gap.ts`).
//!
//! # What is graded, and how the file is read
//!
//! Only the bytes: the model's triangles and every support record, in the
//! file's one model-centred frame. A brace or a mast is a `TAPER` record that
//! does not follow a `TIP` (a tip ball starts every seated arm; a brace and a
//! mast carry none), read from its own `start`, `end` and radii.
//!
//! 1. Every brace and mast record, sampled every 0.01 mm along its axis at its
//!    own interpolated radius, clears the model surface by the registered
//!    FINAL clearance gap (0.3 mm) less the brief's 1e-3 mm tolerance.
//! 2. No record's emitted mesh lies wholly inside the model (every vertex
//!    inside by more than the tolerance): the seat trim never keeps one.
//!
//! The fixture is `make_laid_tube`, a 32-sided prism lying on its side, pinned
//! at its authored pose (identity), so this runs in a fresh clone with no
//! provisioned corpus and does not depend on the dense orientation search.

use std::path::PathBuf;
use std::sync::OnceLock;

use plinth_pipeline::{
    prepare, registered_decimation_budget, registered_support_request, ModelPreparationOutcome,
    OrientationPolicy, OutputProfile, PrepareRequest,
};

const TOLERANCE_MM: f64 = 1e-3;
const STEM: &str = "make_laid_tube";
const IDENTITY_PIN: &str = r#"{"stages":[{"name":"hollow","status":"ok","report":{"oriented":true,"orientation":{"axis":[1,0,0],"deg":0}}}]}"#;

type Point = [f64; 3];
type Triangle = [Point; 3];

struct Member {
    record: usize,
    start: Point,
    end: Point,
    r_start: f64,
    r_end: f64,
}

struct Emitted {
    model: Vec<Triangle>,
    members: Vec<Member>,
    /// Every non-ball record's mesh vertices, with its record index.
    meshes: Vec<(usize, Vec<Point>)>,
    trim: plinth_emit::SeatTrimReport,
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("plinth-member-clearance-{}", std::process::id()));
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

fn final_gap_mm() -> f64 {
    *plinth_core::m0_constants()
        .expect("the registry constructs")
        .evidence()
        .final_clearance_gap_mm()
        .value()
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
        let mesh = plinth_mesh::make_laid_tube().expect("the laid tube fixture");
        plinth_mesh::write_binary_stl(scratch.0.join("in").join(format!("{STEM}.stl")), &mesh)
            .expect("write the laid tube");
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
            .expect("the laid tube is in the report");
        let ModelPreparationOutcome::Artifact { emit, .. } = &entry.outcome else {
            panic!(
                "the laid tube must reach an artifact; it reported {:?}",
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
        let mut members = Vec::new();
        let mut meshes = Vec::new();
        for (index, record) in records.iter().enumerate() {
            if record.segment_type == chitubox_io::SegmentType::TIP {
                continue;
            }
            meshes.push((index, points(&record.mesh.xyz)));
            let after_tip =
                index > 0 && records[index - 1].segment_type == chitubox_io::SegmentType::TIP;
            if record.segment_type == chitubox_io::SegmentType::TAPER && !after_tip {
                members.push(Member {
                    record: index,
                    start: f64_point(record.start),
                    end: f64_point(record.end),
                    r_start: f64::from(record.r_start),
                    r_end: f64::from(record.r_end),
                });
            }
        }
        Emitted {
            model,
            members,
            meshes,
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

fn norm(a: Point) -> f64 {
    dot(a, a).sqrt()
}

/// Point-to-triangle distance (Ericson 5.1.5), a test-side grader.
fn point_triangle(p: Point, [a, b, c]: Triangle) -> f64 {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return norm(ap);
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return norm(bp);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return norm(sub(ap, [ab[0] * v, ab[1] * v, ab[2] * v]));
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return norm(cp);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return norm(sub(ap, [ac[0] * w, ac[1] * w, ac[2] * w]));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        let bc = sub(c, b);
        return norm(sub(bp, [bc[0] * w, bc[1] * w, bc[2] * w]));
    }
    let den = 1.0 / (va + vb + vc);
    let (v, w) = (vb * den, vc * den);
    norm(sub(
        ap,
        [
            ab[0] * v + ac[0] * w,
            ab[1] * v + ac[1] * w,
            ab[2] * v + ac[2] * w,
        ],
    ))
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

fn surface_distance(model: &[Triangle], p: Point) -> f64 {
    model
        .iter()
        .map(|triangle| point_triangle(p, *triangle))
        .fold(f64::INFINITY, f64::min)
}

/// Rule 1: every brace and mast in the file clears the model by the final gap.
#[test]
fn every_brace_and_mast_in_the_file_clears_the_model_by_the_final_gap() {
    let file = emitted();
    let gap = final_gap_mm();
    let mut close = Vec::new();
    for member in &file.members {
        let length = norm(sub(member.end, member.start));
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a sample count from a bounded member length"
        )]
        let samples = ((length / 0.01).ceil() as usize).max(2);
        let mut worst = f64::INFINITY;
        for i in 0..=samples {
            #[expect(clippy::cast_precision_loss, reason = "a bounded sample index")]
            let f = i as f64 / samples as f64;
            let p = [
                member.start[0] + (member.end[0] - member.start[0]) * f,
                member.start[1] + (member.end[1] - member.start[1]) * f,
                member.start[2] + (member.end[2] - member.start[2]) * f,
            ];
            let radius = member.r_start + (member.r_end - member.r_start) * f;
            let d = surface_distance(&file.model, p);
            let signed = if inside(&file.model, p) { -d } else { d };
            worst = worst.min(signed - radius);
        }
        if worst < gap - TOLERANCE_MM {
            close.push((member.record, (worst * 1000.0).round() / 1000.0));
        }
    }
    println!(
        "laid tube: {} braces and masts in the file; {} within the {gap} mm final gap: {close:?}",
        file.members.len(),
        close.len()
    );
    assert!(
        !file.members.is_empty(),
        "anti-vacuity: the file carries no brace or mast"
    );
    assert!(
        close.is_empty(),
        "{} braces or masts in the file come within the {gap} mm final gap of the model \
         (record, surface gap mm): {close:?}",
        close.len()
    );
}

/// Rule 1, the seat trim: no record in the file lies wholly inside the model.
#[test]
fn no_record_in_the_file_lies_wholly_inside_the_model() {
    let file = emitted();
    let wholly: Vec<usize> = file
        .meshes
        .iter()
        .filter(|(_, vertices)| {
            !vertices.is_empty()
                && vertices.iter().all(|&p| {
                    inside(&file.model, p) && surface_distance(&file.model, p) > TOLERANCE_MM
                })
        })
        .map(|(record, _)| *record)
        .collect();
    println!(
        "laid tube: {} non-ball records; {} wholly inside the model {wholly:?}; the trim kept {} \
         outside the model, dropped {}, left {} whole",
        file.meshes.len(),
        wholly.len(),
        file.trim.records_kept_outside_model_after_empty_trim,
        file.trim.records_dropped_wholly_inside_model,
        file.trim.records_left_whole_after_empty_trim,
    );
    assert!(
        !file.meshes.is_empty(),
        "anti-vacuity: the file carries no support record"
    );
    assert!(
        wholly.is_empty(),
        "{} records lie wholly inside the model: {wholly:?}",
        wholly.len()
    );
}
