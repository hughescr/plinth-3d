//! **On Round_25 at its committed pose, no arm loses more than its tip-ball
//! cap to the seat trim** (fix/lip-seat, 2026-09-29), graded from the bytes on
//! disk.
//!
//! # The defect this pins
//!
//! Owner, ChituBox, `for-craig-v3` (and `for-craig-v4`, byte-identical on
//! Round_25): "a very fat side-branch is then almost completely shaved off".
//! The seat trim keeps only the support side of a tip's seat plane, and the
//! plane is infinite. An arm that meets a steep face nearly side-on (the
//! drafted inner wall of the ring lip, 75° from horizontal) runs its shaft
//! along the plane, and the trim removes the shaft there, outside the ball, in
//! air. Measured on `for-craig-v4/Round_25`: one arm lost 1.05 mm³ of its
//! 2.14 mm³ taper outside its ball, and five more lost 0.13-0.31 mm³.
//!
//! # The rule
//!
//! A seated arm meets its seat face within the angle at which the seat trim
//! keeps the arm's own section: the trim may remove only the ball's cap
//! region. Placement re-lands the arm at a smaller angle to the same seat,
//! else slides the seat further along the same face, else keeps it and warns.
//!
//! # What is graded, and how
//!
//! Only the file. A TIP record (type 7) is a ball: centre midway between its
//! poles, radius its `r_start`. Its shaft is the TAPER (type 1) that starts at
//! the ball centre and every JOINER (type 3) chained end to start below it.
//! For every shaft record the grader samples the record's OWN solid as its
//! fields describe it (a frustum between its end points with its two radii,
//! rings normal to the axis, or horizontal where the file's end ring is
//! horizontal), inscribed in the emitted polygon so an untrimmed record has
//! every sample inside its mesh. A sample that is outside the ball, and not
//! inside the record's emitted (trimmed) mesh, is shaft the trim removed
//! outside the ball. It is split by whether the sample lies inside the MODEL:
//!
//! * **in air**: removed by the seat plane alone, where no model stands — the
//!   resin drawn and thrown away, and the shaved look;
//! * **in the model**: under the seat face the plane is the model's own
//!   surface, so a shaft that dips below it outside the ball is cut there too.
//!
//! Every arm must lose at most [`TOLERANCE_MM3`] of either outside its ball.
//! A cheap exact screen runs first (the record's polygon volume against its
//! mesh's volume), and only the records the trim touched are sampled.
//!
//! The fixture is the PROVISIONED `corpus/models/Round_25.stl` (gitignored, as
//! `round100_emit.rs` uses Round_100), pinned at its committed pose, the flip
//! (180° about X), which the dense search commits (`for-craig-v4/README.md`).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use plinth_pipeline::{
    prepare, registered_decimation_budget, registered_support_request, ModelPreparationOutcome,
    OrientationPolicy, OutputProfile, PrepareRequest,
};

const STEM: &str = "Round_25";
const FLIP_PIN: &str = r#"{"stages":[{"name":"hollow","status":"ok","report":{"oriented":true,"orientation":{"axis":[1,0,0],"deg":180}}}]}"#;

/// Lattice step of the sampled grader, mm.
const STEP_MM: f64 = 0.02;
/// Per arm, in mm³: two lattice cells at [`STEP_MM`], the grader's own
/// resolution. Its noise floor is zero: every sample lies strictly inside the
/// record's inscribed polygon solid and strictly outside the ball, so a sample
/// is counted only where the trim really removed the solid. At one tenth of
/// the flat-face slivers' volume on develop (0.001-0.006 mm³), and five
/// orders under the worst arm (1.05 mm³).
const TOLERANCE_MM3: f64 = 2.0 * STEP_MM * STEP_MM * STEP_MM;
/// Field floors, so an empty or shrunken field cannot pass. The v4 file has 87
/// tips, 49 of them on leaned arms.
///
/// RE-PINNED 60 → 45 on 2026-09-30 (lane lattice-align), on purpose: with the
/// straight-line reach and the island seating the real Round_25 field is 54
/// arms (11 leaned over 20°), because the rim annulus and the rib tops are
/// seated as one midline row each rather than by the global lattice plus
/// fill arms. The floor still refuses an empty or halved field.
const MIN_ARMS: usize = 45;
const MIN_LEANED_ARMS: usize = 10;

type Point = [f64; 3];

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("plinth-lip-seat-{}", std::process::id()));
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

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/plinth-pipeline sits two levels under the repo root")
        .to_path_buf()
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
        exact_delicacy: false,
        stop_after: None,
    }
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

fn norm(a: Point) -> f64 {
    dot(a, a).sqrt()
}

fn point(p: [f32; 3]) -> Point {
    [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]
}

fn triangles(xyz: &[f32]) -> Vec<[Point; 3]> {
    xyz.chunks_exact(9)
        .map(|t| {
            [
                [f64::from(t[0]), f64::from(t[1]), f64::from(t[2])],
                [f64::from(t[3]), f64::from(t[4]), f64::from(t[5])],
                [f64::from(t[6]), f64::from(t[7]), f64::from(t[8])],
            ]
        })
        .collect()
}

/// A closed triangle soup's volume, by the divergence theorem.
fn volume(soup: &[[Point; 3]]) -> f64 {
    soup.iter()
        .map(|[a, b, c]| dot(*a, cross(*b, *c)))
        .sum::<f64>()
        / 6.0
}

/// A soup indexed for "is this point inside": a +Z ray's crossings, over the
/// triangles whose plan-view box holds the point (an XY bucket grid).
struct Solid {
    soup: Vec<[Point; 3]>,
    lo: [f64; 2],
    cell: f64,
    cols: usize,
    rows: usize,
    buckets: Vec<Vec<usize>>,
}

impl Solid {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "test-local bucket indices of a bounded soup, range-checked"
    )]
    fn new(soup: Vec<[Point; 3]>, cell: f64) -> Self {
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for triangle in &soup {
            for vertex in triangle {
                for axis in 0..2 {
                    lo[axis] = lo[axis].min(vertex[axis]);
                    hi[axis] = hi[axis].max(vertex[axis]);
                }
            }
        }
        let cols = (((hi[0] - lo[0]) / cell).floor() as usize + 1).max(1);
        let rows = (((hi[1] - lo[1]) / cell).floor() as usize + 1).max(1);
        let mut buckets = vec![Vec::new(); cols * rows];
        for (index, triangle) in soup.iter().enumerate() {
            let x0 = triangle.iter().map(|v| v[0]).fold(f64::INFINITY, f64::min);
            let x1 = triangle
                .iter()
                .map(|v| v[0])
                .fold(f64::NEG_INFINITY, f64::max);
            let y0 = triangle.iter().map(|v| v[1]).fold(f64::INFINITY, f64::min);
            let y1 = triangle
                .iter()
                .map(|v| v[1])
                .fold(f64::NEG_INFINITY, f64::max);
            let c0 = ((x0 - lo[0]) / cell).floor() as usize;
            let c1 = (((x1 - lo[0]) / cell).floor() as usize).min(cols - 1);
            let r0 = ((y0 - lo[1]) / cell).floor() as usize;
            let r1 = (((y1 - lo[1]) / cell).floor() as usize).min(rows - 1);
            for row in r0..=r1 {
                for col in c0..=c1 {
                    buckets[row * cols + col].push(index);
                }
            }
        }
        Self {
            soup,
            lo,
            cell,
            cols,
            rows,
            buckets,
        }
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "test-local bucket indices, range-checked as f64 first"
    )]
    fn contains(&self, p: Point) -> bool {
        // A jitter far below the lattice step keeps the ray off shared edges.
        let (px, py) = (p[0] + 1.234_567e-7, p[1] + 7.654_321e-8);
        let col = ((px - self.lo[0]) / self.cell).floor();
        let row = ((py - self.lo[1]) / self.cell).floor();
        if col < 0.0 || row < 0.0 || col as usize >= self.cols || row as usize >= self.rows {
            return false;
        }
        let mut crossings = 0_usize;
        for &index in &self.buckets[row as usize * self.cols + col as usize] {
            let [a, b, c] = self.soup[index];
            let det = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
            if det == 0.0 {
                continue;
            }
            let l1 = ((b[1] - c[1]) * (px - c[0]) + (c[0] - b[0]) * (py - c[1])) / det;
            let l2 = ((c[1] - a[1]) * (px - c[0]) + (a[0] - c[0]) * (py - c[1])) / det;
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
}

/// One shaft record as its fields describe it.
struct Shaft {
    start: Point,
    end: Point,
    r_start: f64,
    r_end: f64,
    /// Rings normal to the axis (`false`: horizontal rings, a plate column).
    axial_rings: bool,
    mesh: Solid,
}

/// One arm: its ball, its seat plane as the file shows it, and its shaft
/// records.
struct Arm {
    centre: Point,
    radius: f64,
    lean_deg: f64,
    /// A point of the seat plane and the plane's normal pointing INTO the
    /// model, read off the ball's trimmed mesh; `None` for a ball the trim
    /// left whole.
    seat_plane: Option<(Point, Point)>,
    shafts: Vec<Shaft>,
}

/// What one arm lost outside its ball, mm³: in air; inside the model on the
/// model side of its seat plane (the plane's cut under the face); and inside
/// the model on the SUPPORT side of it (the shaft passing through model
/// material away from its seat, which the model difference removes whatever
/// the seat).
#[derive(Clone, Copy, Debug, Default)]
struct Loss {
    in_air: f64,
    in_model_past_seat: f64,
    in_model_elsewhere: f64,
}

/// The seat plane of a trimmed ball, from its own mesh: the flat cap is the
/// triangles whose normal is not radial (a sphere facet's is, to within its
/// own facet angle), and the largest coplanar group of them by area is the
/// plane. Its outward normal points away from the ball, into the model.
fn seat_plane(centre: Point, radius: f64, soup: &[[Point; 3]]) -> Option<(Point, Point)> {
    let mut groups: Vec<(Point, Point, f64)> = Vec::new();
    for [a, b, c] in soup {
        let n = cross(sub(*b, *a), sub(*c, *a));
        let doubled = norm(n);
        if doubled <= 0.0 {
            continue;
        }
        let unit = n.map(|v| v / doubled);
        let centroid: Point = std::array::from_fn(|i| (a[i] + b[i] + c[i]) / 3.0);
        let radial = sub(centroid, centre);
        let distance = norm(radial);
        if distance > 0.0 && dot(unit, radial) / distance > 0.95 && distance > 0.9 * radius {
            continue;
        }
        match groups
            .iter_mut()
            .find(|(normal, _, _)| dot(*normal, unit) > 0.9999)
        {
            Some(group) => group.2 += 0.5 * doubled,
            None => groups.push((unit, centroid, 0.5 * doubled)),
        }
    }
    groups
        .into_iter()
        .max_by(|x, y| x.2.total_cmp(&y.2))
        .map(|(normal, point, _)| (point, normal))
}

/// Which way a record's rings lie, read from the file's own end ring: the
/// vertices at distance `r_end` from `end` lie in a plane normal to the axis,
/// or in a horizontal plane.
///
/// A vertex of the START ring (at `r_start` from `start`) is not an end-ring
/// vertex even where it happens to lie `r_end` from `end` too. On a short
/// record (length about its radius) it can: round 2 of this lane met a plate
/// column's last joiner on Round_25 (record 177, length 0.707 mm, radius
/// 0.733 mm, horizontal rings) whose start ring put five vertices at
/// 0.734 mm from its end, 0.6 mm below it, and the grader read the rings as
/// axial and counted the difference between the two solids (0.40 mm³) as
/// trimmed shaft. The file's own mesh volume (1.0025 mm³) is the horizontal
/// ring solid's (1.0024 mm³), not the axial one's (1.1812 mm³).
fn rings_are_axial(
    start: Point,
    end: Point,
    r_start: f64,
    r_end: f64,
    soup: &[[Point; 3]],
) -> bool {
    let axis = sub(end, start);
    let length = norm(axis);
    let unit = axis.map(|v| v / length);
    let (mut along, mut vertical) = (0.0_f64, 0.0_f64);
    for vertex in soup.iter().flatten() {
        let offset = sub(*vertex, end);
        if (norm(offset) - r_end).abs() > 1e-4 * (1.0 + r_end) {
            continue;
        }
        if (norm(sub(*vertex, start)) - r_start).abs() <= 1e-4 * (1.0 + r_start) {
            continue;
        }
        along = along.max(dot(offset, unit).abs());
        vertical = vertical.max(offset[2].abs());
    }
    along <= vertical
}

/// The number of sides the file draws a frustum with, from the registry.
fn sides() -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a registered side count is a small integer"
    )]
    let sides = plinth_core::m0_constants()
        .expect("registry")
        .placeholders()
        .chitubox_emit_n_sides()
        .diagnostic()
        .get() as f64;
    sides
}

/// The record's solid at parameter `t ∈ [0, 1]`: its ring centre and the
/// radius of the polygon's inscribed circle there.
fn ring_at(shaft: &Shaft, t: f64, inscribed: f64) -> (Point, f64) {
    (
        std::array::from_fn(|axis| shaft.start[axis] + t * (shaft.end[axis] - shaft.start[axis])),
        (shaft.r_start + t * (shaft.r_end - shaft.r_start)) * inscribed,
    )
}

impl Shaft {
    /// Whether `p` lies in this record's inscribed solid.
    fn holds(&self, p: Point, inscribed: f64) -> bool {
        let axis = sub(self.end, self.start);
        let t = if self.axial_rings {
            dot(sub(p, self.start), axis) / dot(axis, axis)
        } else {
            (p[2] - self.start[2]) / axis[2]
        };
        if !(0.0..=1.0).contains(&t) {
            return false;
        }
        let (centre, radius) = ring_at(self, t, inscribed);
        let offset = sub(p, centre);
        let spread = if self.axial_rings {
            let along = dot(offset, axis) / norm(axis);
            (dot(offset, offset) - along * along).max(0.0).sqrt()
        } else {
            offset[0].hypot(offset[1])
        };
        spread <= radius
    }

    /// The polygon solid's exact volume (a frustum with similar polygonal
    /// rings; a sheared column keeps its horizontal sections' volume).
    fn polygon_volume(&self, sides: f64) -> f64 {
        let area_factor = 0.5 * sides * (2.0 * core::f64::consts::PI / sides).sin();
        let height = if self.axial_rings {
            norm(sub(self.end, self.start))
        } else {
            (self.end[2] - self.start[2]).abs()
        };
        let (a, b) = (self.r_start, self.r_end);
        // The frustum's `a² + ab + b²`.
        let sum_of_squares = a.mul_add(a, b * b);
        area_factor * height / 3.0 * a.mul_add(b, sum_of_squares)
    }
}

/// Lattice points of `shaft`'s inscribed solid, in its own bounding box.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test-local lattice counts of a bounded record"
)]
fn lattice(shaft: &Shaft, inscribed: f64, mut visit: impl FnMut(Point)) {
    let reach = shaft.r_start.max(shaft.r_end);
    let lo: Point = std::array::from_fn(|axis| shaft.start[axis].min(shaft.end[axis]) - reach);
    let hi: Point = std::array::from_fn(|axis| shaft.start[axis].max(shaft.end[axis]) + reach);
    let steps: [usize; 3] = std::array::from_fn(|axis| ((hi[axis] - lo[axis]) / STEP_MM) as usize);
    for i in 0..=steps[0] {
        for j in 0..=steps[1] {
            for k in 0..=steps[2] {
                let p = [
                    lo[0] + (i as f64 + 0.5) * STEP_MM,
                    lo[1] + (j as f64 + 0.5) * STEP_MM,
                    lo[2] + (k as f64 + 0.5) * STEP_MM,
                ];
                if shaft.holds(p, inscribed) {
                    visit(p);
                }
            }
        }
    }
}

struct Graded {
    arms: Vec<(usize, Arm, Loss)>,
}

fn graded() -> &'static Graded {
    static GRADED: OnceLock<Graded> = OnceLock::new();
    GRADED.get_or_init(|| {
        let source = repo_root()
            .join("corpus/models")
            .join(format!("{STEM}.stl"));
        assert!(
            source.is_file(),
            "RequiredCorpusModelMissing: {} is not provisioned (corpus/models is gitignored)",
            source.display()
        );
        let scratch = Scratch::new();
        std::fs::copy(&source, scratch.0.join("in").join(format!("{STEM}.stl")))
            .expect("stage Round_25");
        std::fs::write(
            scratch.0.join("pins").join(format!("{STEM}.pipeline.json")),
            FLIP_PIN,
        )
        .expect("write the flip pin");
        let report = prepare(request(&scratch)).expect("the sweep route emits and warns");
        let entry = report
            .models
            .iter()
            .find(|entry| entry.model.as_str() == STEM)
            .expect("Round_25 is in the report");
        let ModelPreparationOutcome::Artifact { emit, .. } = &entry.outcome else {
            panic!(
                "Round_25 must reach an artifact; it reported {:?}",
                entry.outcome
            );
        };
        let project =
            chitubox_io::Project::open(&emit.artifact_path).expect("the artifact must reopen");
        let model = Solid::new(
            triangles(&project.model_triangles(0).expect("model triangles")),
            0.5,
        );
        let records = project.supports(0).expect("support records");
        let mut arms = Vec::new();
        for (index, record) in records.iter().enumerate() {
            if record.segment_type != chitubox_io::SegmentType::TIP {
                continue;
            }
            let (a, b) = (point(record.start), point(record.end));
            let centre: Point = std::array::from_fn(|axis| 0.5 * (a[axis] + b[axis]));
            let mut shafts = Vec::new();
            let mut joint = centre;
            for next in &records[index + 1..] {
                let kind = next.segment_type;
                let first = shafts.is_empty();
                if !((first && kind == chitubox_io::SegmentType::TAPER)
                    || (!first && kind == chitubox_io::SegmentType::JOINER))
                {
                    break;
                }
                let start = point(next.start);
                if norm(sub(start, joint)) > 1e-4 {
                    break;
                }
                let end = point(next.end);
                let soup = triangles(&next.mesh.xyz);
                let (r_start, r_end) = (f64::from(next.r_start), f64::from(next.r_end));
                shafts.push(Shaft {
                    start,
                    end,
                    r_start,
                    r_end,
                    axial_rings: rings_are_axial(start, end, r_start, r_end, &soup),
                    mesh: Solid::new(soup, 0.05),
                });
                joint = end;
            }
            let Some(first) = shafts.first() else {
                continue;
            };
            let axis = sub(first.end, first.start);
            let lean_deg = (axis[2].abs() / norm(axis)).acos().to_degrees();
            let radius = f64::from(record.r_start);
            arms.push((
                index,
                Arm {
                    centre,
                    radius,
                    lean_deg,
                    seat_plane: seat_plane(centre, radius, &triangles(&record.mesh.xyz)),
                    shafts,
                },
            ));
        }
        let sides = sides();
        let inscribed = (core::f64::consts::PI / sides).cos() * (1.0 - 1e-9);
        let cell = STEP_MM * STEP_MM * STEP_MM;
        let graded = arms
            .into_iter()
            .map(|(index, arm)| {
                let mut loss = Loss::default();
                for shaft in &arm.shafts {
                    // THE EXACT SCREEN: a record whose mesh holds its whole
                    // polygon solid (to the file's binary32) lost nothing.
                    let whole = shaft.polygon_volume(sides);
                    let removed = whole - volume(&shaft.mesh.soup);
                    if removed <= 1e-5 * whole {
                        continue;
                    }
                    lattice(shaft, inscribed, |p| {
                        if norm(sub(p, arm.centre)) <= arm.radius || shaft.mesh.contains(p) {
                            return;
                        }
                        if !model.contains(p) {
                            loss.in_air += cell;
                        } else if arm
                            .seat_plane
                            .is_some_and(|(point, into)| dot(sub(p, point), into) > 0.0)
                        {
                            loss.in_model_past_seat += cell;
                        } else {
                            loss.in_model_elsewhere += cell;
                        }
                    });
                }
                (index, arm, loss)
            })
            .collect();
        Graded { arms: graded }
    })
}

#[test]
fn no_arm_of_round_25_loses_more_than_its_tip_ball_cap_to_the_seat_trim() {
    let graded = graded();
    let leaned = graded
        .arms
        .iter()
        .filter(|(_, arm, _)| arm.lean_deg > 20.0)
        .count();
    assert!(
        graded.arms.len() >= MIN_ARMS && leaned >= MIN_LEANED_ARMS,
        "a real Round_25 field was graded: {} arms, {} leaned over 20 deg",
        graded.arms.len(),
        leaned
    );
    let mut offenders: Vec<String> = Vec::new();
    let mut elsewhere: Vec<String> = Vec::new();
    let (mut air_total, mut past_total, mut elsewhere_total) = (0.0, 0.0, 0.0);
    for (record, arm, loss) in &graded.arms {
        air_total += loss.in_air;
        past_total += loss.in_model_past_seat;
        elsewhere_total += loss.in_model_elsewhere;
        let line = format!(
            "tip record {record} (lean {:.1} deg, ball {:.3} mm): shaft removed outside its ball \
             {:.4} mm3 in air, {:.4} mm3 in the model past its seat plane, {:.4} mm3 in the model \
             on the support side of it",
            arm.lean_deg, arm.radius, loss.in_air, loss.in_model_past_seat, loss.in_model_elsewhere
        );
        if loss.in_air > TOLERANCE_MM3 || loss.in_model_past_seat > TOLERANCE_MM3 {
            offenders.push(line);
        } else if loss.in_model_elsewhere > TOLERANCE_MM3 {
            elsewhere.push(line);
        }
    }
    println!(
        "graded {} arms ({} leaned over 20 deg); shaft removed outside the ball: {:.4} mm3 in \
         air, {:.4} mm3 in the model past the seat plane, {:.4} mm3 in the model on its support \
         side; offenders {}",
        graded.arms.len(),
        leaned,
        air_total,
        past_total,
        elsewhere_total,
        offenders.len()
    );
    for line in &offenders {
        println!("  {line}");
    }
    // NOT this rule's defect, and published rather than hidden: a shaft that
    // passes through model material on the support side of its own seat plane
    // is cut by the model difference whatever the seat (the path test is the
    // centreline's; an arm's own solid is not clearance-checked, see
    // `lanes/member-clearance.md` Q1).
    for line in &elsewhere {
        println!("  (shaft through the model, not graded here) {line}");
    }
    assert!(
        offenders.is_empty(),
        "{} arms lose shaft to the seat trim outside their tip ball:\n{}",
        offenders.len(),
        offenders.join("\n")
    );
    // A RATCHET on the known defect above: develop `2fc80b6` has exactly one
    // such arm on this fixture (tip record 234, lean 46.2 deg, 0.2315 mm3; two
    // since X_ALLOWED became 18, see the constant), and
    // the lip-seat rule re-lands arms, so it must never add another (its
    // re-landings are held to the kernel's solid clearance for this reason).
    assert!(
        elsewhere.len() <= KNOWN_SHAFTS_THROUGH_THE_MODEL,
        "shafts through the model grew: {} against the known-defect pin of {}:\n{}",
        elsewhere.len(),
        KNOWN_SHAFTS_THROUGH_THE_MODEL,
        elsewhere.join("\n")
    );
}

/// Arms of this fixture whose shaft passes through model material on the
/// support side of their seat plane, on develop `2fc80b6` (measured by this
/// grader: tip record 234). A path-clearance defect, not the seat angle.
///
/// RE-PINNED 1 -> 2 on 2026-09-30, lane x-allowed-18: Craig ruled X_ALLOWED
/// 12 -> 18, which re-sizes every arm of the field (1/1.5 of the section per
/// unit load) and so re-lays the leans and seats the lip-seat pass works on.
/// Measured on the ruled field: tip records 213 (lean 50.0 deg, ball 0.490 mm,
/// 0.1748 mm3) and 238 (lean 44.9 deg, ball 0.390 mm, 0.1359 mm3), 0.3107 mm3
/// in all, against one arm and 0.2315 mm3 at X 12. Nothing in the seat trim
/// or the lip-seat rule changed; the defect is the same (an ARM's own solid is
/// not clearance-checked, `lanes/member-clearance.md` Q1), and it now shows on
/// two arms. The pin still holds the lip-seat rule to adding none.
const KNOWN_SHAFTS_THROUGH_THE_MODEL: usize = 2;
