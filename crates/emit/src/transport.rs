//! The transport path: a model lifted above the plate and written alone, with no supports.
//!
//! A transport project proves the read, lift, write and check chain. It is never a print
//! candidate: its model name carries [`TRANSPORT_LABEL`], and the command line and report
//! say the same.
//!
//! **How the lift is written.** The only transform field in the file is translate
//! (format reference section 6): `01` to `02` changed only translate Z. The meaning of Z for
//! a mesh that is not centred is an open question (section 15, question 4: "lowest point
//! above the plate" or "centre offset from the seated position"). Native files keep the mesh
//! centred on its bounding box (section 6.1). So the mesh is centred on its bounding box
//! here, the centre goes into translate X and Y (the model stays where the STL put it in
//! X and Y, as in `07`), and translate Z is the lift. For a centred mesh both readings of Z
//! give the same lowest point, so the lift does not depend on the open question.

use plinth_chitubox::preview::BACKGROUND;
use plinth_chitubox::{Model, Preview, Project, Triangle, WriteError};
use plinth_geom::Mesh;
use std::fmt;
use std::thread::ThreadId;

/// Lift of the lowest model point above the plate, in mm. This is the H4 limit
/// (`limits.toml`, `h4_lift_mm`). No checker rule measures it yet (H4 is reported
/// "unverified").
pub const TRANSPORT_LIFT_MM: f32 = 5.0;

/// Text in the model name of every transport file.
pub const TRANSPORT_LABEL: &str = "transport, not a print candidate";

/// A parallel task gets at least this many triangles (design section 8: coarse tasks only).
pub const MIN_TRIANGLES_PER_TASK: usize = 1000;

/// Longest input name kept in the model name, in bytes (the field holds 255 plus a NUL).
const MAX_STEM_BYTES: usize = 200;

/// Size of the blank preview image, in pixels. Every native file with a model has a preview
/// (format reference section 3), and an older ChituBox refused an empty one, so the
/// transport file carries a plain background image rather than none. Samples run from
/// 650 x 1945 up; whether ChituBox v1.4.0 accepts this is not known (section 15, question 3).
pub const PREVIEW_SIZE: (u32, u32) = (800, 600);

/// Why a transport project cannot be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmitError {
    /// The mesh has no triangles.
    Empty,
    /// The blank preview could not be made.
    Preview(WriteError),
}

impl fmt::Display for EmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "the model has no triangles"),
            Self::Preview(e) => write!(f, "cannot make the preview: {e}"),
        }
    }
}

impl std::error::Error for EmitError {}

/// What a parallel step used, for the run report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parallel {
    /// Tasks the work was split into.
    pub tasks: usize,
    /// Distinct threads that ran those tasks (the calling thread is one of them).
    pub threads_ran: usize,
}

/// The converted triangles and what the conversion used.
#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    /// File-form triangles, in mesh order, centred on `centre`.
    pub triangles: Vec<Triangle>,
    /// The bounding-box centre that was subtracted, in mm.
    pub centre: [f64; 3],
    /// Tasks and threads the conversion used.
    pub parallel: Parallel,
}

/// Number of tasks for `n` triangles at `threads` workers: at most one per worker, and at
/// most `n / MIN_TRIANGLES_PER_TASK`, so every task but the last holds at least
/// [`MIN_TRIANGLES_PER_TASK`] triangles. The last task takes the remainder after equal
/// slices of `ceil(n / tasks)` and may hold fewer (Round 50: five slices of 1003, one of
/// 999). A small mesh gets a single task.
#[must_use]
pub fn task_count(n: usize, threads: usize) -> usize {
    (n / MIN_TRIANGLES_PER_TASK).clamp(1, threads.max(1))
}

/// Narrowing to the file's f32 is the writer's precision step (design section 8).
#[allow(clippy::cast_possible_truncation, reason = "f32 is the file precision")]
fn f32_of(v: f64) -> f32 {
    v as f32
}

fn fill(mesh: &Mesh, centre: [f64; 3], first: usize, out: &mut [Triangle]) {
    let idx = mesh.indices();
    let (xs, ys, zs) = (mesh.xs(), mesh.ys(), mesh.zs());
    let corner = |i: u32| {
        let i = i as usize;
        [
            f32_of(xs[i] - centre[0]),
            f32_of(ys[i] - centre[1]),
            f32_of(zs[i] - centre[2]),
        ]
    };
    for (k, slot) in out.iter_mut().enumerate() {
        let t = 3 * (first + k);
        *slot = [corner(idx[t]), corner(idx[t + 1]), corner(idx[t + 2])];
    }
}

/// Convert the whole mesh to file-form triangles in one batch, centred on its bounding box.
/// Each task fills its own slice of the output, so the result is the same at any thread
/// count. The calling thread runs the last task.
#[must_use]
pub fn mesh_triangles(mesh: &Mesh, threads: usize) -> Converted {
    let n = mesh.triangle_count();
    let b = mesh.bounds();
    let centre = if b.is_empty() {
        [0.0; 3]
    } else {
        [0, 1, 2].map(|a| 0.5 * (b.min[a] + b.max[a]))
    };
    let per = n.div_ceil(task_count(n, threads)).max(1);
    let mut triangles: Vec<Triangle> = vec![[[0.0; 3]; 3]; n];
    let mut chunks: Vec<(usize, &mut [Triangle])> = triangles
        .chunks_mut(per)
        .enumerate()
        .map(|(c, s)| (c * per, s))
        .collect();
    let last = chunks.pop();
    let mut ran: Vec<ThreadId> = vec![std::thread::current().id()];
    std::thread::scope(|s| {
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|(first, out)| {
                s.spawn(move || {
                    fill(mesh, centre, first, out);
                    std::thread::current().id()
                })
            })
            .collect();
        if let Some((first, out)) = last {
            fill(mesh, centre, first, out);
        }
        for h in handles {
            match h.join() {
                Ok(id) => {
                    if !ran.contains(&id) {
                        ran.push(id);
                    }
                }
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
    });
    Converted {
        triangles,
        centre,
        parallel: Parallel {
            tasks: n.div_ceil(per).max(1),
            threads_ran: ran.len(),
        },
    }
}

/// The model name of a transport file: the input's stem and the label.
#[must_use]
pub fn transport_model_name(stem: &str) -> String {
    let mut end = stem.len().min(MAX_STEM_BYTES);
    while !stem.is_char_boundary(end) {
        end -= 1;
    }
    format!("{} [{TRANSPORT_LABEL}]", stem[..end].replace('\0', "_"))
}

/// A one-model project: the mesh centred on its bounding box, the centre in translate X and
/// Y, the lift [`TRANSPORT_LIFT_MM`] in translate Z, no supports.
///
/// # Errors
/// [`EmitError::Empty`] for a mesh with no triangles.
pub fn transport_project(
    stem: &str,
    mesh: &Mesh,
    threads: usize,
) -> Result<(Project, Parallel), EmitError> {
    if mesh.triangle_count() == 0 {
        return Err(EmitError::Empty);
    }
    let Converted {
        triangles,
        centre,
        parallel,
    } = mesh_triangles(mesh, threads);
    let model = Model {
        name: transport_model_name(stem),
        translate: [f32_of(centre[0]), f32_of(centre[1]), TRANSPORT_LIFT_MM],
        mesh: triangles,
        supports: None,
    };
    let preview =
        Preview::solid(PREVIEW_SIZE.0, PREVIEW_SIZE.1, BACKGROUND).map_err(EmitError::Preview)?;
    let project = Project {
        preview: Some(preview),
        models: vec![model],
        ..Project::default()
    };
    Ok((project, parallel))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `n` triangles on a grid, z from 10 up, so the mesh is not centred on the origin.
    fn grid_mesh(n: usize) -> Mesh {
        let mut sx = Vec::new();
        let mut sy = Vec::new();
        let mut sz = Vec::new();
        for i in 0..n {
            let (x, y) = ((i % 70) as f64 + 100.0, (i / 70) as f64 - 5.0);
            for (dx, dy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)] {
                sx.push(x + dx);
                sy.push(y + dy);
                sz.push(10.0 + 0.5 * (i % 7) as f64);
            }
        }
        Mesh::from_soup(&sx, &sy, &sz).expect("mesh")
    }

    fn bounds_of(tris: &[Triangle]) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for v in tris.iter().flatten() {
            for a in 0..3 {
                lo[a] = lo[a].min(v[a]);
                hi[a] = hi[a].max(v[a]);
            }
        }
        (lo, hi)
    }

    #[test]
    fn task_count_is_coarse_and_capped() {
        assert_eq!(task_count(0, 8), 1);
        assert_eq!(task_count(999, 8), 1);
        assert_eq!(task_count(2996, 8), 2);
        assert_eq!(task_count(100_000, 8), 8);
        assert_eq!(task_count(100_000, 3), 3);
        assert_eq!(task_count(100_000, 0), 1);
    }

    #[test]
    fn project_is_model_only_centred_and_lifted_by_translate() {
        let mesh = grid_mesh(10);
        let (p, _) = transport_project("Round 25", &mesh, 1).expect("project");
        assert_eq!(p.models.len(), 1);
        let m = &p.models[0];
        assert!(m.supports.is_none());
        assert_eq!(m.mesh.len(), mesh.triangle_count());
        assert_eq!(m.name, "Round 25 [transport, not a print candidate]");
        // Centred on the bounding box, so the mesh straddles the origin in every axis ...
        let (lo, hi) = bounds_of(&m.mesh);
        for a in 0..3 {
            assert!((lo[a] + hi[a]).abs() < 1e-4, "axis {a}: {lo:?} {hi:?}");
        }
        // ... with the centre in translate X and Y (the model stays where the STL put it)
        // and the lift in translate Z.
        let b = mesh.bounds();
        assert!((f64::from(m.translate[0]) - 0.5 * (b.min[0] + b.max[0])).abs() < 1e-4);
        assert!((f64::from(m.translate[1]) - 0.5 * (b.min[1] + b.max[1])).abs() < 1e-4);
        assert_eq!(m.translate[2].to_bits(), TRANSPORT_LIFT_MM.to_bits());
    }

    #[test]
    fn project_has_a_preview_and_survives_write_and_read() {
        let mesh = grid_mesh(30);
        let (p, _) = transport_project("x", &mesh, 1).expect("project");
        let pv = p.preview.as_ref().expect("a model needs a preview");
        assert_eq!((pv.width, pv.height), PREVIEW_SIZE);
        let bytes = p.to_bytes().expect("bytes");
        let back = plinth_chitubox::parse(&bytes).expect("parses");
        assert_eq!(back, p);
        assert!(back.deviations().is_empty());
    }

    #[test]
    fn centring_keeps_shape_and_winding() {
        let mesh = grid_mesh(5);
        let c = mesh_triangles(&mesh, 1);
        for (t, tri) in c.triangles.iter().enumerate() {
            let corners = mesh.triangle_corners(t);
            for (k, v) in tri.iter().enumerate() {
                for a in 0..3 {
                    let want = corners[k][a] - c.centre[a];
                    assert!((f64::from(v[a]) - want).abs() < 1e-5, "t {t} k {k} a {a}");
                }
            }
        }
    }

    #[test]
    fn empty_mesh_is_refused() {
        let empty = Mesh::new(vec![], vec![], vec![], vec![]).expect("empty mesh");
        assert_eq!(
            transport_project("x", &empty, 1).unwrap_err(),
            EmitError::Empty
        );
    }

    #[test]
    fn long_and_odd_names_fit_the_name_field() {
        let long = "é".repeat(300);
        let name = transport_model_name(&long);
        assert!(name.len() < 256, "{}", name.len());
        assert!(name.ends_with("[transport, not a print candidate]"));
        assert!(!transport_model_name("a\0b").contains('\0'));
    }

    /// The thread count this slot may use (set by `scripts/t`); library code never reads it.
    #[allow(
        clippy::disallowed_methods,
        reason = "test-only read of the slot allocation set by scripts/t"
    )]
    fn slot_threads() -> usize {
        std::env::var("NEXTEST_TEST_THREADS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1)
            .clamp(1, 8)
    }

    /// The 3 and 8 worker case. It starts more workers than a writing slot may use, so it
    /// runs only in the bench epoch (nextest profile `bench`, 8 threads); started anywhere
    /// else it fails, never passes unrun.
    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "test-only read of the profile name nextest exports"
    )]
    fn threads_bench_conversion_is_identical_at_one_three_and_eight_workers() {
        assert!(
            std::env::var("NEXTEST_PROFILE").is_ok_and(|p| p == "bench") && slot_threads() >= 8,
            "bench-epoch test started outside the bench epoch (profile bench, 8 threads)"
        );
        let mesh = grid_mesh(9000);
        let one = mesh_triangles(&mesh, 1);
        for workers in [3, 8] {
            let many = mesh_triangles(&mesh, workers);
            assert_eq!(
                many.parallel.tasks,
                workers.min(9000 / MIN_TRIANGLES_PER_TASK)
            );
            assert_eq!(many.parallel.threads_ran, many.parallel.tasks);
            assert_eq!(one.triangles, many.triangles, "{workers} workers");
        }
    }

    #[test]
    fn threads_conversion_is_identical_at_one_worker_and_the_slot_allocation() {
        let alloc = slot_threads();
        let mesh = grid_mesh(5200);
        let one = mesh_triangles(&mesh, 1);
        assert_eq!((one.parallel.tasks, one.parallel.threads_ran), (1, 1));
        let many = mesh_triangles(&mesh, alloc);
        assert_eq!(many.parallel.tasks, task_count(5200, alloc));
        assert!(
            many.parallel.tasks <= alloc && many.parallel.threads_ran <= alloc,
            "{:?} on a slot allocation of {alloc}",
            many.parallel
        );
        assert_eq!(one.triangles, many.triangles);
    }
}
