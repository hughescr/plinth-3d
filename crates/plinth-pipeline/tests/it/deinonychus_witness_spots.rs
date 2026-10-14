//! **No first layer over nothing where the joint lane left one** (fix lane A,
//! 2026-10-12): the production route on Deinonychus, read back from the bytes
//! on disk, at the spots the cross-family review sliced as new floating
//! material (`joint/out-fix/final` against `for-craig-joint`, slicer view,
//! one pixel of lateral allowance).
//!
//! | spot (mm, layer) | what floated | mechanism |
//! |---|---|---|
//! | (12.380, 15.395) k158 | arm 101's seat island, 121 px | footprint slide 0.51 mm down its face (`keeps_seat`'s no-climb clause) |
//! | (0.250, 20.397) k135 | 97 px beside arm 60's seat | footprint slide 0.19 mm, the seat point still inside the ball |
//! | (7.586, 21.270) k169 | 44 px beside arm 133's seat | footprint slide 0.19 mm, the same |
//! | (1.580, -23.459) k173 | arm 160's lower end, 55 px | post 162's ball grew to 0.533 mm and its drawn shaft left the planned axis arm 160 landed on |
//! | (-11.919, -6.408) k154 | 131 px of model, no record | a first layer over nothing 0.04-0.06 mm beside grounded model, which the cover's outward rounding joined to it, so no tip was placed; `for-craig-joint` held it by a footprint slide of arm 92 the rules no longer make |
//! | (1.414, 9.439) k101 | 14 px of the chest tip's ball | the owner's 0.8947 mm ball on a 0.6710 mm shaft leaning 52.1 degrees: its lowest point stood outside the shaft |
//!
//! The first three are pinned by `footprint::keeps_ground`, the fourth by
//! `adapter::drawn_line_offset` reading every seated member's drawn shaft,
//! the fifth by the sweep's birth seats (`births.rs`: a birth at the
//! printer's pixel that the reach field roots only laterally gets an arm
//! inside it) and the sixth by the lowering's pole collar
//! (`plinth_support::lower::pole_collar`).
//!
//! # What is graded
//!
//! Each spot's layer `k` and the one under it are sliced as a slicer cures
//! them (every solid filled at pixel centres, the model and every support
//! record unioned) in a window around the spot. Every cured component of
//! layer `k` with a pixel within 0.3 mm of the spot must overlap cured
//! material of layer `k - 1`, within one pixel, inside the window (a
//! component carried only outside a 2 mm window would read as floating: the
//! check errs toward a failure, never toward a pass).
//!
//! **What it cannot see**: a component that rests on layer `k - 1` material
//! which itself floats lower down. The defect here was a first layer over
//! nothing, and that is what is graded.
//!
//! Sabotage: the same predicate with layer `k - 1` emptied must flag every
//! spot (it fires where the defect would be).
//!
//! Local input (`PLINTH_MINIS_DIR`, the uncommitted Deinonychus mini) and a
//! five-minute production run, so `#[ignore]`d in the gate.

// Pixel arithmetic of a window a millimetre or so across, in test-local code.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use std::path::PathBuf;

use plinth_pipeline::{
    prepare, registered_decimation_budget, registered_support_request, ModelPreparationOutcome,
    OrientationPolicy, OutputProfile, PrepareRequest,
};

const STEM: &str = "Deinonychus";
const PX: f64 = 0.014;
const PY: f64 = 0.019;
const LAYER: f64 = 0.05;
const HALF_WINDOW_MM: f64 = 1.0;
/// How near the spot a component must reach to be graded: the slides and the
/// follow moved material up to 0.25 mm, so the spot's own material, wherever
/// the fixed file draws it, is inside this.
const NEAR_MM: f64 = 0.3;

/// (x, y, layer) of each spot, in the emitted file's frame.
const SPOTS: [(f64, f64, usize); 6] = [
    (12.380, 15.395, 158),
    (0.250, 20.397, 135),
    (7.586, 21.270, 169),
    (1.580, -23.459, 173),
    (-11.919, -6.408, 154),
    (1.414, 9.439, 101),
];

type Tri = [[f64; 3]; 3];

fn input() -> PathBuf {
    std::env::var_os("PLINTH_MINIS_DIR")
        .map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..").join("minis-to-process"),
            PathBuf::from,
        )
        .join(format!("{STEM}.stl"))
}

/// The emitted file's solids: the model first, then one per support record.
fn emitted() -> Vec<Vec<Tri>> {
    let scratch = std::env::temp_dir().join(format!("plinth-witness-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(scratch.join("in")).expect("scratch input folder");
    std::fs::copy(input(), scratch.join("in").join(format!("{STEM}.stl")))
        .expect("the local Deinonychus mini (PLINTH_MINIS_DIR)");
    let printer = OutputProfile::Saturn4Ultra.resolve().expect("M0 Saturn profile").clone();
    let report = prepare(PrepareRequest {
        input_models_dir: scratch.join("in"),
        output_dir: scratch.join("out"),
        orientation: OrientationPolicy::DenseSweep,
        no_orient: Default::default(),
        placement: plinth_pipeline::Placement::Sweep,
        output_profile: OutputProfile::Saturn4Ultra,
        decimation_budget: registered_decimation_budget().expect("registered budget"),
        hollow: plinth_hollow::HollowRequest::with_registered_wall_default(printer.clone())
            .expect("registered wall"),
        support: registered_support_request(&printer).expect("registered lift"),
        no_cache: false,
        stop_after: None,
    })
    .expect("the sweep route emits and warns");
    let entry = report.models.iter().find(|entry| entry.model.as_str() == STEM).expect("reported");
    let ModelPreparationOutcome::Artifact { emit, .. } = &entry.outcome else {
        panic!("Deinonychus must reach an artifact: {:?}", entry.outcome);
    };
    let project = chitubox_io::Project::open(&emit.artifact_path).expect("the artifact reopens");
    let tris = |xyz: &[f32]| -> Vec<Tri> {
        xyz.chunks_exact(9)
            .map(|t| std::array::from_fn(|v| std::array::from_fn(|a| f64::from(t[3 * v + a]))))
            .collect()
    };
    let mut solids = vec![tris(&project.model_triangles(0).expect("model triangles"))];
    solids.extend(project.supports(0).expect("support records").iter().map(|record| tris(&record.mesh.xyz)));
    let _ = std::fs::remove_dir_all(&scratch);
    solids
}

/// One layer of a window, a slicer's way: each solid filled at pixel
/// centres (even-odd on its own closed surface), all of them unioned.
struct Window {
    col0: i64,
    row0: i64,
    cols: usize,
    rows: usize,
    on: Vec<bool>,
}

fn slice(solids: &[Vec<Tri>], z: f64, x: f64, y: f64) -> Window {
    let (col0, row0) = (((x - HALF_WINDOW_MM) / PX).floor() as i64, ((y - HALF_WINDOW_MM) / PY).floor() as i64);
    let (cols, rows) = ((2.0 * HALF_WINDOW_MM / PX).ceil() as usize, (2.0 * HALF_WINDOW_MM / PY).ceil() as usize);
    let mut on = vec![false; cols * rows];
    for solid in solids {
        let segments: Vec<[[f64; 2]; 2]> = solid
            .iter()
            .filter_map(|t| {
                let mut points = Vec::with_capacity(2);
                for i in 0..3 {
                    let (a, b) = (t[i], t[(i + 1) % 3]);
                    if (a[2] >= z) != (b[2] >= z) {
                        let w = (z - a[2]) / (b[2] - a[2]);
                        points.push([a[0] + w * (b[0] - a[0]), a[1] + w * (b[1] - a[1])]);
                    }
                }
                (points.len() == 2).then(|| [points[0], points[1]])
            })
            .collect();
        if segments.is_empty() {
            continue;
        }
        for r in 0..rows {
            let yc = ((row0 + r as i64) as f64 + 0.5) * PY;
            let mut xs: Vec<f64> = segments
                .iter()
                .filter(|s| (s[0][1] <= yc) != (s[1][1] <= yc))
                .map(|s| s[0][0] + (yc - s[0][1]) * (s[1][0] - s[0][0]) / (s[1][1] - s[0][1]))
                .collect();
            xs.sort_by(f64::total_cmp);
            for pair in xs.chunks_exact(2) {
                for c in 0..cols {
                    let xc = ((col0 + c as i64) as f64 + 0.5) * PX;
                    if xc >= pair[0] && xc < pair[1] {
                        on[r * cols + c] = true;
                    }
                }
            }
        }
    }
    Window { col0, row0, cols, rows, on }
}

/// The components of `layer` near the spot that rest on nothing in `below`.
fn floating(layer: &Window, below: &Window, x: f64, y: f64) -> usize {
    let (cols, rows) = (layer.cols, layer.rows);
    let mut seen = vec![false; cols * rows];
    let mut bad = 0;
    for start in 0..cols * rows {
        if !layer.on[start] || seen[start] {
            continue;
        }
        let (mut near, mut rests) = (false, false);
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(at) = stack.pop() {
            let (r, c) = (at / cols, at % cols);
            let (xc, yc) = (
                ((layer.col0 + c as i64) as f64 + 0.5) * PX,
                ((layer.row0 + r as i64) as f64 + 0.5) * PY,
            );
            near |= (xc - x).hypot(yc - y) <= NEAR_MM;
            for dr in -1_i64..=1 {
                for dc in -1_i64..=1 {
                    let (rr, cc) = (r as i64 + dr, c as i64 + dc);
                    if rr >= 0 && cc >= 0 && (rr as usize) < rows && (cc as usize) < cols {
                        rests |= below.on[rr as usize * cols + cc as usize];
                    }
                }
            }
            for (dr, dc) in [(-1_i64, 0_i64), (1, 0), (0, -1), (0, 1)] {
                let (rr, cc) = (r as i64 + dr, c as i64 + dc);
                if rr >= 0 && cc >= 0 && (rr as usize) < rows && (cc as usize) < cols {
                    let next = rr as usize * cols + cc as usize;
                    if layer.on[next] && !seen[next] {
                        seen[next] = true;
                        stack.push(next);
                    }
                }
            }
        }
        if near && !rests {
            bad += 1;
        }
    }
    bad
}

#[test]
#[ignore = "reads the local, uncommitted Deinonychus mini (PLINTH_MINIS_DIR) and runs the production route"]
fn the_joint_lanes_floating_spots_print_on_something() {
    let solids = emitted();
    let plate = solids[1..]
        .iter()
        .flatten()
        .flat_map(|t| t.iter().map(|v| v[2]))
        .fold(f64::INFINITY, f64::min);
    for (x, y, k) in SPOTS {
        let z = |layer: usize| plate + (layer as f64 + 0.5) * LAYER;
        let layer = slice(&solids, z(k), x, y);
        let below = slice(&solids, z(k - 1), x, y);
        let near = layer.on.iter().filter(|on| **on).count();
        let found = floating(&layer, &below, x, y);
        // Sabotage: nothing under the layer. Every spot must then be flagged.
        let empty = Window { on: vec![false; below.on.len()], ..below };
        let sabotaged = floating(&layer, &empty, x, y);
        println!("spot ({x}, {y}) k{k}: {near} cured px in the window, {found} floating near the spot (sabotage {sabotaged})");
        assert!(sabotaged > 0, "anti-vacuity at ({x}, {y}) k{k}: the check sees nothing there");
        assert_eq!(found, 0, "({x}, {y}) k{k}: a first layer over nothing");
    }
}
