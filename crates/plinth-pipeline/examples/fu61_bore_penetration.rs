//! FU-61 verification harness: do drain/vent bores fully penetrate, at full
//! bore width, through the *realized* voxel-stepped cavity wall?
//!
//! This is an analysis harness, not a pipeline stage. It runs the production
//! chain (load -> repair -> decimate -> orient -> create_shell -> plan_drains
//! -> drill) and then measures the drilled geometry with an **independent**
//! ray caster written here, so the measurement shares no code with the
//! `geom_kernel` casts `prove_centerlines` already uses.
//!
//! Geometry, once, so the numbers below read as one picture. Every hole has a
//! `center` inside the cavity void, a unit `direction` pointing out, and a
//! cutter cylinder spanning `t` from `-before` to `sampled_exit + after`,
//! where the three rungs come from the registered extension ladder
//! (`HOLLOW_EXTRA_DRAIN_POLICY`, 4/8/9 mm -> before 4, drain after 4, vent
//! after 5). `t = 0` is the cavity origin; the shell's inner surface is met at
//! `inner_crossing`, its outer surface at `exact_exit`, and `sampled_exit` is
//! the lattice-stepped exit the planner recorded.
//!
//! (A) CLEAR-RADIUS SCAN, on the drilled mesh. Probe rays parallel to the bore
//!     axis at radial offsets rho in [0, 1.02 R] (dense near R) and 96 angles.
//!     A probe is clear when it crosses no surface between the cavity origin
//!     plane and the cutter's far cap. The largest rho at which every angle is
//!     clear is the realized opening radius: the opening width at the stepped
//!     cavity boundary, measured rather than assumed. Combined with "t = 0 is
//!     void on the axis" and "the axis leaves the shell before the far cap",
//!     a clear cylinder of radius rho joins the void to open air.
//!
//! (B) MARGIN SCAN, on the undrilled shell. For the same probes the harness
//!     locates the material the bore has to remove and reports how much of the
//!     cutter's overshoot each end actually consumes:
//!         outer margin = (sampled_exit + after) - outermost material at rho
//!         inner margin = innermost material at rho    - (-before)
//!     Their minima over every probe, hole and model answer the "~0.9 mm"
//!     question: how much overshoot the voxel stepping and the bore's
//!     obliquity eat, and how much is left.
//!
//! ```sh
//! cargo run --release --offline -p plinth-pipeline --example fu61_bore_penetration -- <stl>...
//! ```

use std::path::{Path, PathBuf};

use plinth_core::Finite;
use plinth_decimate::run as run_decimation;
use plinth_hollow::drains::{plan_drains, HoleRole};
use plinth_hollow::drilling::{drill_planned_shell, DrilledHoleSource};
use plinth_hollow::{
    create_shell_recording_repair, H0AdmissionLimits, HollowRequest, HollowSettings,
};
use plinth_mesh::{load_binary_stl, Mesh, WeldPolicy};
use plinth_pipeline::prepare::registered_decimation_budget;
use plinth_pipeline::PrepareResourcePolicy;

type V3 = [f64; 3];

const ANGLES: usize = 96;
/// Coarse sweep to 0.9 R, then 0.001 R steps through the facet band, so the
/// realized radius is resolved finely enough to separate a lattice defect from
/// the cutter's 24-gon inradius.
const COARSE_STEPS: usize = 18;
const FINE_START: f64 = 0.9;
const FINE_STEP: f64 = 0.001;
const FINE_END: f64 = 1.02;
/// Keeps a probe that starts exactly on a freshly cut cap plane from counting
/// that cap as an obstruction.
const WINDOW_EPS_MM: f64 = 1.0e-6;
/// Two crossings closer than this are one crossing seen twice across a shared
/// edge or vertex.
const DEDUPE_MM: f64 = 1.0e-9;
/// The drilling cutter is an inscribed 24-gon prism (`CYLINDER_SECTIONS`), so
/// the widest circular channel it can possibly cut has this fraction of the
/// nominal radius. Measuring against it separates "the lattice narrowed the
/// opening" -- the thing FU-61 asks about -- from "the cutter is a polygon".
const CUTTER_SECTIONS: f64 = 24.0;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // `--wall-mm X` is a STRESS configuration, not a production claim: it is
    // the same operator knob `plinth prepare --wall-mm` exposes, used here
    // only to obtain drilled bores on real minis that decide solid at the
    // registered 2.5 mm wall. Every reported number says which wall it came
    // from.
    let mut wall_override = None;
    if let Some(position) = args.iter().position(|arg| arg == "--wall-mm") {
        let value: f64 = args
            .get(position + 1)
            .and_then(|text| text.parse().ok())
            .expect("--wall-mm takes a number");
        args.drain(position..=position + 1);
        wall_override = Some(value);
    }
    // `--dump-dir DIR` writes the shell and the drilled shell as binary STL so
    // the measured bores can be rendered. Evidence only; nothing reads it back.
    let mut dump_dir = None;
    if let Some(position) = args.iter().position(|arg| arg == "--dump-dir") {
        let value = args
            .get(position + 1)
            .cloned()
            .expect("--dump-dir takes a path");
        args.drain(position..=position + 1);
        dump_dir = Some(PathBuf::from(value));
    }
    let sources: Vec<PathBuf> = if args.is_empty() {
        vec![workspace_root().join("corpus/models/Sphere.stl")]
    } else {
        args.iter().map(PathBuf::from).collect()
    };

    let mut all = Vec::new();
    for source in &sources {
        match measure_model(source, wall_override, dump_dir.as_deref()) {
            Ok(record) => {
                print_model(&record);
                all.push(record);
            }
            Err(error) => {
                eprintln!("!! {}: {error}", source.display());
                let mut record = ModelRecord::empty(stem(source));
                record.error = Some(error);
                all.push(record);
            }
        }
    }

    println!("\n================ FU-61 SUMMARY ================");
    let mut worst_clear = f64::INFINITY;
    let mut worst_inner = f64::INFINITY;
    let mut worst_outer = f64::INFINITY;
    let mut holes = 0usize;
    let mut errors = 0usize;
    let mut penetrates = true;
    for record in &all {
        if let Some(error) = &record.error {
            println!("{:<28} ERROR {error}", record.model);
            errors += 1;
            continue;
        }
        for hole in &record.holes {
            holes += 1;
            worst_clear = worst_clear.min(hole.clear_radius_mm / hole.radius_mm);
            worst_inner = worst_inner.min(hole.min_inner_margin_mm);
            worst_outer = worst_outer.min(hole.min_outer_margin_mm);
            if !hole.penetrates {
                penetrates = false;
            }
        }
    }
    println!("models measured        {}", all.len() - errors);
    println!("holes measured         {holes}");
    println!("every hole penetrates  {penetrates}");
    println!("min clear/nominal      {worst_clear:.6}");
    println!("min outer margin mm    {worst_outer:.4}");
    println!("min inner margin mm    {worst_inner:.4}");
    println!("voxel body diagonal mm {:.4}", 0.5 * 3.0_f64.sqrt());
    println!(
        "24-gon inradius factor {:.6}",
        (std::f64::consts::PI / 24.0).cos()
    );

    match serde_json::to_string_pretty(&all.iter().map(json_of).collect::<Vec<_>>()) {
        Ok(text) => println!("\n----- JSON -----\n{text}"),
        Err(error) => eprintln!("json: {error}"),
    }
}

#[derive(Debug)]
struct HoleRecord {
    index: usize,
    source: &'static str,
    role: &'static str,
    center: V3,
    direction: V3,
    radius_mm: f64,
    inner_crossing_mm: f64,
    exact_exit_mm: f64,
    sampled_exit_mm: f64,
    t_start: f64,
    t_end: f64,
    /// The three facts that together mean "penetrates at full bore width".
    origin_in_void: bool,
    axis_exits_before_cap: bool,
    clear_at_nominal_radius: bool,
    clear_at_cutter_inradius: bool,
    cutter_inradius_mm: f64,
    penetrates: bool,
    /// Largest probe offset at which every angle is clear on the drilled mesh.
    clear_radius_mm: f64,
    first_blocked_radius_mm: Option<f64>,
    /// Margins at rho <= R, on the undrilled shell.
    min_outer_margin_mm: f64,
    min_inner_margin_mm: f64,
    min_local_wall_mm: f64,
    max_local_wall_mm: f64,
    /// Probes whose origin was already inside material -- the worst-case
    /// stepped-cavity placement the ruling names.
    probes_origin_in_material: usize,
    probes_total: usize,
    /// Probes that met no material at all inside the cutter span.
    probes_no_material: usize,
    /// Probes with shell material beyond the cutter's far cap.
    probes_material_beyond_cap: usize,
}

#[derive(Debug)]
struct ModelRecord {
    model: String,
    error: Option<String>,
    source_faces: usize,
    decimated_faces: usize,
    wall_requested_mm: f64,
    pitch_mm: f64,
    cavity_voxels: usize,
    holes: Vec<HoleRecord>,
}

impl ModelRecord {
    fn empty(model: String) -> Self {
        Self {
            model,
            error: None,
            source_faces: 0,
            decimated_faces: 0,
            wall_requested_mm: 0.0,
            pitch_mm: 0.0,
            cavity_voxels: 0,
            holes: Vec::new(),
        }
    }
}

fn measure_model(
    source: &Path,
    wall_override: Option<f64>,
    dump_dir: Option<&Path>,
) -> Result<ModelRecord, String> {
    let name = stem(source);
    let raw = load_binary_stl(source, WeldPolicy::None).map_err(|e| format!("load: {e}"))?;
    let source_faces = raw.tris().len();
    let (repaired, _evidence) = plinth_mesh::repair_mesh(&raw).into_parts();

    let budget = registered_decimation_budget().map_err(|e| format!("budget: {e}"))?;
    let policy =
        PrepareResourcePolicy::for_saturn4_ultra(budget).map_err(|e| format!("policy: {e}"))?;
    let decimated = run_decimation(&repaired, policy.decimation())
        .map_err(|e| format!("decimate: {e}"))?
        .into_decimated_model()
        .map_err(|scale| format!("scale suspicious: {} mm", scale.longest_axis_mm))?;
    let decimated_faces = decimated.mesh().tris().len();

    let mut sweep = plinth_orient::registered_sweep_request().map_err(|e| format!("orient: {e}"))?;
    sweep.limits = policy.orient();
    let oriented =
        plinth_orient::search_and_orient(decimated, &sweep).map_err(|e| format!("orient: {e}"))?;
    let (solid, _history) = oriented.into_parts();

    let constants = plinth_core::m0_constants().map_err(|e| format!("constants: {e}"))?;
    let mut request = HollowRequest::with_registered_wall_default(constants.saturn4_ultra().clone())
        .map_err(|e| format!("request: {e}"))?;
    if let Some(wall) = wall_override {
        request.wall_mm = plinth_core::PositiveFinite::new(wall)
            .map_err(|_| format!("wall {wall} is not a positive finite"))?;
    }
    let settings = HollowSettings::from_request(&request).map_err(|e| format!("settings: {e}"))?;

    let candidate = create_shell_recording_repair(solid, settings, H0AdmissionLimits::production())
        .map_err(|e| format!("create_shell: {}", e.into_error()))?;
    let pitch_mm = candidate.cavity_field().pitch_mm().get();
    let planned = plan_drains(candidate).map_err(|e| format!("plan_drains: {:?}", e.reason()))?;
    let shell = planned.shell().clone();
    let drilled =
        drill_planned_shell(planned).map_err(|e| format!("drill: {:?}", e.into_reason()))?;

    let mut ladder = constants
        .placeholders()
        .hollow_extra_drain_policy()
        .diagnostic()
        .extensions_mm();
    let before = ladder.next().ok_or("empty extension ladder")?;
    let drain_after = ladder.next().ok_or("short extension ladder")? - before;
    let vent_after = ladder.next().ok_or("short extension ladder")? - before;

    let drilled_mesh = drilled.mesh().clone();
    if let Some(directory) = dump_dir {
        std::fs::create_dir_all(directory).map_err(|e| format!("dump dir: {e}"))?;
        write_binary_stl(&directory.join(format!("{name}.shell.stl")), &shell)?;
        write_binary_stl(&directory.join(format!("{name}.drilled.stl")), &drilled_mesh)?;
    }
    let evidence = drilled.evidence();
    let mut holes = Vec::with_capacity(evidence.holes().len());
    for (index, hole) in evidence.holes().iter().enumerate() {
        let center = fin3(hole.center_mm());
        let exit = fin3(hole.exact_outer_exit_mm());
        let delta = sub(exit, center);
        let exact_exit_mm = norm(delta);
        if exact_exit_mm <= 0.0 {
            return Err(format!("hole {index} has a degenerate axis"));
        }
        let direction = scale(delta, 1.0 / exact_exit_mm);
        let radius_mm = hole.radius_mm().get();
        let sampled_exit_mm = hole.sampled_exit_mm().get();
        let after = match hole.role() {
            HoleRole::Drain => drain_after,
            HoleRole::Vent => vent_after,
        };
        holes.push(measure_hole(
            HoleInputs {
                index,
                source: hole.source(),
                role: hole.role(),
                center,
                direction,
                radius_mm,
                inner_crossing_mm: hole.inner_crossing_mm().get(),
                exact_exit_mm,
                sampled_exit_mm,
                t_start: -before,
                t_end: sampled_exit_mm + after,
            },
            &drilled_mesh,
            &shell,
        ));
        if let Some(directory) = dump_dir {
            let record = holes.last().expect("just pushed");
            write_section_svg(
                &directory.join(format!("{name}.hole{index}-{}.svg", record.role)),
                &shell,
                &drilled_mesh,
                record,
            )?;
        }
    }

    Ok(ModelRecord {
        model: name,
        error: None,
        source_faces,
        decimated_faces,
        wall_requested_mm: settings.wall_thickness_mm().get(),
        pitch_mm,
        cavity_voxels: evidence.cavity_voxels(),
        holes,
    })
}

struct HoleInputs {
    index: usize,
    source: DrilledHoleSource,
    role: HoleRole,
    center: V3,
    direction: V3,
    radius_mm: f64,
    inner_crossing_mm: f64,
    exact_exit_mm: f64,
    sampled_exit_mm: f64,
    t_start: f64,
    t_end: f64,
}

/// Probe offsets, built once per hole: every (rho, theta) probe is then a
/// table lookup rather than a per-probe basis construction.
struct Probes {
    offsets: Vec<V3>,
    radii: Vec<f64>,
}

fn probe_offsets(direction: V3, radius_mm: f64) -> Probes {
    let w = direction;
    let reference: V3 = if w[0].abs() <= w[1].abs() && w[0].abs() <= w[2].abs() {
        [1.0, 0.0, 0.0]
    } else if w[1].abs() <= w[2].abs() {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let u = normalize(cross(reference, w));
    let v = cross(w, u);
    let offsets = (0..ANGLES)
        .map(|k| {
            let angle = std::f64::consts::TAU * k as f64 / ANGLES as f64;
            add(scale(u, angle.cos()), scale(v, angle.sin()))
        })
        .collect();
    let mut fractions: Vec<f64> = (0..COARSE_STEPS)
        .map(|k| FINE_START * k as f64 / COARSE_STEPS as f64)
        .collect();
    let mut fraction = FINE_START;
    while fraction <= FINE_END + 1.0e-12 {
        fractions.push(fraction);
        fraction += FINE_STEP;
    }
    let radii = fractions.iter().map(|f| radius_mm * f).collect();
    Probes { offsets, radii }
}

fn measure_hole(inputs: HoleInputs, drilled: &Mesh, shell: &Mesh) -> HoleRecord {
    let HoleInputs {
        index,
        source,
        role,
        center,
        direction,
        radius_mm,
        inner_crossing_mm,
        exact_exit_mm,
        sampled_exit_mm,
        t_start,
        t_end,
    } = inputs;
    let probes = probe_offsets(direction, radius_mm);
    let probe_radius = radius_mm * FINE_END;

    // Windows wide enough to leave both meshes entirely, so a crossing count
    // is a true inside/outside parity rather than a windowed sample.
    let (dl, dh) = line_window(drilled, center, direction, probe_radius);
    let (sl, sh) = line_window(shell, center, direction, probe_radius);
    let drilled_set = gather(drilled, center, direction, probe_radius, dl, dh);
    let shell_set = gather(shell, center, direction, probe_radius, sl, sh);

    // The two axis facts the full-width claim rests on.
    let axis_shell = crossings(&shell_set, shell, center, direction);
    let origin_in_void = !inside_at(&axis_shell, 0.0);
    let axis_exits_before_cap = exact_exit_mm < t_end && !inside_at(&axis_shell, t_end);

    // (A) clear-radius scan on the drilled mesh.
    let mut clear_radius_mm = 0.0;
    let mut first_blocked_radius_mm = None;
    for &rho in &probes.radii {
        let mut clear = true;
        for offset in &probes.offsets {
            let origin = add(center, scale(*offset, rho));
            let hits = crossings(&drilled_set, drilled, origin, direction);
            // Both halves are needed. "No crossing in the span" alone would
            // also be true of a probe buried in material for the whole span,
            // so the parity at the cavity-origin plane is checked as well.
            let obstructed = hits
                .iter()
                .any(|t| *t > WINDOW_EPS_MM && *t < t_end - WINDOW_EPS_MM);
            if obstructed || inside_at(&hits, 0.0) {
                clear = false;
                break;
            }
            if rho == 0.0 {
                break;
            }
        }
        if clear {
            clear_radius_mm = rho;
        } else {
            first_blocked_radius_mm = Some(rho);
            break;
        }
    }
    let clear_at_nominal_radius = clear_radius_mm >= radius_mm - 1.0e-12;
    let cutter_inradius_mm = radius_mm * (std::f64::consts::PI / CUTTER_SECTIONS).cos();
    // One radial grid step of slack, because the scan brackets rather than
    // solves for the boundary.
    let clear_at_cutter_inradius = clear_radius_mm >= cutter_inradius_mm - radius_mm * FINE_STEP;

    // (B) margin scan on the undrilled shell, at rho <= R.
    let mut min_outer = f64::INFINITY;
    let mut min_inner = f64::INFINITY;
    let mut min_wall = f64::INFINITY;
    let mut max_wall: f64 = 0.0;
    let mut probes_origin_in_material = 0usize;
    let mut probes_total = 0usize;
    let mut probes_no_material = 0usize;
    let mut probes_material_beyond_cap = 0usize;
    for &rho in &probes.radii {
        if rho > radius_mm + 1.0e-12 {
            break;
        }
        for offset in &probes.offsets {
            let origin = add(center, scale(*offset, rho));
            let hits = crossings(&shell_set, shell, origin, direction);
            probes_total += 1;

            let inside_origin = inside_at(&hits, 0.0);
            if inside_origin {
                probes_origin_in_material += 1;
            }
            // Outermost material the cutter has to clear, and the innermost
            // point of the run the origin sits in (only binding when the
            // origin is already inside material).
            let outermost = hits
                .iter()
                .copied()
                .filter(|t| *t > 0.0 && *t <= t_end)
                .fold(f64::NEG_INFINITY, f64::max);
            if outermost.is_finite() {
                min_outer = min_outer.min(t_end - outermost);
            } else if !inside_origin {
                probes_no_material += 1;
            }
            if hits.iter().any(|t| *t > t_end) {
                probes_material_beyond_cap += 1;
            }
            if inside_origin {
                let back = hits
                    .iter()
                    .copied()
                    .filter(|t| *t <= 0.0)
                    .fold(f64::NEG_INFINITY, f64::max);
                if back.is_finite() {
                    min_inner = min_inner.min(back - t_start);
                    if outermost.is_finite() {
                        min_wall = min_wall.min(outermost - back);
                        max_wall = max_wall.max(outermost - back);
                    }
                } else {
                    min_inner = f64::NEG_INFINITY;
                }
            } else {
                let entry = hits
                    .iter()
                    .copied()
                    .filter(|t| *t > 0.0)
                    .fold(f64::INFINITY, f64::min);
                if entry.is_finite() && entry <= t_end {
                    min_inner = min_inner.min(entry - t_start);
                    if outermost.is_finite() {
                        min_wall = min_wall.min(outermost - entry);
                        max_wall = max_wall.max(outermost - entry);
                    }
                }
            }
            if rho == 0.0 {
                break;
            }
        }
    }

    let penetrates = origin_in_void && axis_exits_before_cap && clear_at_cutter_inradius;

    HoleRecord {
        index,
        source: match source {
            DrilledHoleSource::Planned => "planned",
            DrilledHoleSource::Audit => "audit",
        },
        role: match role {
            HoleRole::Drain => "drain",
            HoleRole::Vent => "vent",
        },
        center,
        direction,
        radius_mm,
        inner_crossing_mm,
        exact_exit_mm,
        sampled_exit_mm,
        t_start,
        t_end,
        origin_in_void,
        axis_exits_before_cap,
        clear_at_nominal_radius,
        clear_at_cutter_inradius,
        cutter_inradius_mm,
        penetrates,
        clear_radius_mm,
        first_blocked_radius_mm,
        min_outer_margin_mm: min_outer,
        min_inner_margin_mm: min_inner,
        min_local_wall_mm: min_wall,
        max_local_wall_mm: max_wall,
        probes_origin_in_material,
        probes_total,
        probes_no_material,
        probes_material_beyond_cap,
    }
}

/// Odd number of crossings ahead means the point at `t` is inside material.
/// Only true when the crossing list covers the whole ray, which
/// [`line_window`] guarantees.
fn inside_at(hits: &[f64], t: f64) -> bool {
    hits.iter().filter(|hit| **hit > t).count() % 2 == 1
}

/// The `t` interval over which the probe bundle can meet this mesh at all:
/// the axis parameters where the padded mesh AABB is entered and left.
fn line_window(mesh: &Mesh, center: V3, direction: V3, radius: f64) -> (f64, f64) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for vertex in mesh.verts() {
        for axis in 0..3 {
            lo[axis] = lo[axis].min(vertex[axis]);
            hi[axis] = hi[axis].max(vertex[axis]);
        }
    }
    let pad = radius + 1.0;
    let mut t_lo = f64::NEG_INFINITY;
    let mut t_hi = f64::INFINITY;
    for axis in 0..3 {
        let (a, b) = (lo[axis] - pad, hi[axis] + pad);
        if direction[axis].abs() < 1.0e-12 {
            continue;
        }
        let mut t0 = (a - center[axis]) / direction[axis];
        let mut t1 = (b - center[axis]) / direction[axis];
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        t_lo = t_lo.max(t0);
        t_hi = t_hi.min(t1);
    }
    if !t_lo.is_finite() || !t_hi.is_finite() || t_lo > t_hi {
        // Axis parallel to every slab it could leave: fall back to the
        // diagonal of the padded box, which always covers the mesh.
        let diagonal = norm(sub(hi, lo)) + 2.0 * pad;
        return (-diagonal, diagonal);
    }
    (t_lo - 1.0, t_hi + 1.0)
}

/// Conservative candidate gather: every triangle whose AABB meets the AABB of
/// the probe cylinder. Supersets on both sides, so no crossing is dropped.
fn gather(mesh: &Mesh, center: V3, direction: V3, radius: f64, t_lo: f64, t_hi: f64) -> Vec<u32> {
    let a = add(center, scale(direction, t_lo));
    let b = add(center, scale(direction, t_hi));
    let pad = radius + 1.0e-6;
    let lo = [
        a[0].min(b[0]) - pad,
        a[1].min(b[1]) - pad,
        a[2].min(b[2]) - pad,
    ];
    let hi = [
        a[0].max(b[0]) + pad,
        a[1].max(b[1]) + pad,
        a[2].max(b[2]) + pad,
    ];
    let verts = mesh.verts();
    let mut keep = Vec::new();
    for (index, tri) in mesh.tris().iter().enumerate() {
        let p = [
            verts[tri[0] as usize],
            verts[tri[1] as usize],
            verts[tri[2] as usize],
        ];
        let mut overlaps = true;
        for axis in 0..3 {
            let tmin = p[0][axis].min(p[1][axis]).min(p[2][axis]);
            let tmax = p[0][axis].max(p[1][axis]).max(p[2][axis]);
            if tmax < lo[axis] || tmin > hi[axis] {
                overlaps = false;
                break;
            }
        }
        if overlaps {
            keep.push(index as u32);
        }
    }
    keep
}

/// Moller-Trumbore against the gathered subset, deduplicated and sorted.
fn crossings(subset: &[u32], mesh: &Mesh, origin: V3, direction: V3) -> Vec<f64> {
    let verts = mesh.verts();
    let tris = mesh.tris();
    let mut out = Vec::new();
    for &index in subset {
        let tri = tris[index as usize];
        let v0 = verts[tri[0] as usize];
        let v1 = verts[tri[1] as usize];
        let v2 = verts[tri[2] as usize];
        let e1 = sub(v1, v0);
        let e2 = sub(v2, v0);
        let pvec = cross(direction, e2);
        let det = dot(e1, pvec);
        if det.abs() < 1.0e-16 {
            continue;
        }
        let inv = 1.0 / det;
        let tvec = sub(origin, v0);
        let u = dot(tvec, pvec) * inv;
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let qvec = cross(tvec, e1);
        let v = dot(direction, qvec) * inv;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        out.push(dot(e2, qvec) * inv);
    }
    out.sort_by(|a, b| a.partial_cmp(b).expect("finite crossing"));
    out.dedup_by(|a, b| (*a - *b).abs() <= DEDUPE_MM);
    out
}

fn print_model(record: &ModelRecord) {
    println!("\n=== {} ===", record.model);
    println!(
        "faces {} -> {}   wall {:.2} mm   pitch {:.2} mm   cavity voxels {}",
        record.source_faces,
        record.decimated_faces,
        record.wall_requested_mm,
        record.pitch_mm,
        record.cavity_voxels
    );
    println!(
        "{:<3} {:<8} {:<6} {:>6} {:>7} {:>8} {:>8} {:>8} {:>9} {:>9} {:>9} {:>8} {:>6}",
        "#",
        "source",
        "role",
        "R",
        "inner",
        "exit",
        "sampled",
        "clearR",
        "clear/R",
        "outerMar",
        "innerMar",
        "wallMin",
        "pen"
    );
    for hole in &record.holes {
        println!(
            "{:<3} {:<8} {:<6} {:>6.2} {:>7.3} {:>8.3} {:>8.3} {:>8.4} {:>9.5} {:>9.4} {:>9.4} {:>8.4} {:>6}",
            hole.index,
            hole.source,
            hole.role,
            hole.radius_mm,
            hole.inner_crossing_mm,
            hole.exact_exit_mm,
            hole.sampled_exit_mm,
            hole.clear_radius_mm,
            hole.clear_radius_mm / hole.radius_mm,
            hole.min_outer_margin_mm,
            hole.min_inner_margin_mm,
            hole.min_local_wall_mm,
            hole.penetrates,
        );
        println!(
            "     probes {} | origin already in material {} | no material {} | material beyond cap {} | first blocked rho {:?}",
            hole.probes_total,
            hole.probes_origin_in_material,
            hole.probes_no_material,
            hole.probes_material_beyond_cap,
            hole.first_blocked_radius_mm,
        );
        if !hole.origin_in_void {
            println!("     !! bore origin is NOT in the void on the undrilled shell");
        }
        if !hole.axis_exits_before_cap {
            println!("     !! the axis does not reach open air before the cutter's far cap");
        }
    }
}

fn json_of(record: &ModelRecord) -> serde_json::Value {
    serde_json::json!({
        "model": record.model,
        "error": record.error,
        "source_faces": record.source_faces,
        "decimated_faces": record.decimated_faces,
        "wall_requested_mm": record.wall_requested_mm,
        "pitch_mm": record.pitch_mm,
        "cavity_voxels": record.cavity_voxels,
        "holes": record.holes.iter().map(|hole| serde_json::json!({
            "index": hole.index,
            "source": hole.source,
            "role": hole.role,
            "center_mm": hole.center,
            "direction": hole.direction,
            "radius_mm": hole.radius_mm,
            "inner_crossing_mm": hole.inner_crossing_mm,
            "exact_exit_mm": hole.exact_exit_mm,
            "sampled_exit_mm": hole.sampled_exit_mm,
            "cutter_t_start_mm": hole.t_start,
            "cutter_t_end_mm": hole.t_end,
            "origin_in_void": hole.origin_in_void,
            "axis_exits_before_cap": hole.axis_exits_before_cap,
            "clear_at_nominal_radius": hole.clear_at_nominal_radius,
            "clear_at_cutter_inradius": hole.clear_at_cutter_inradius,
            "cutter_inradius_mm": hole.cutter_inradius_mm,
            "penetrates": hole.penetrates,
            "clear_radius_mm": hole.clear_radius_mm,
            "clear_over_nominal": hole.clear_radius_mm / hole.radius_mm,
            "first_blocked_radius_mm": hole.first_blocked_radius_mm,
            "min_outer_margin_mm": hole.min_outer_margin_mm,
            "min_inner_margin_mm": hole.min_inner_margin_mm,
            "min_local_wall_mm": hole.min_local_wall_mm,
            "max_local_wall_mm": hole.max_local_wall_mm,
            "probes_total": hole.probes_total,
            "probes_origin_in_material": hole.probes_origin_in_material,
            "probes_no_material": hole.probes_no_material,
            "probes_material_beyond_cap": hole.probes_material_beyond_cap,
        })).collect::<Vec<_>>(),
    })
}

/// An exact cross-section through the bore axis, as SVG: the shell before
/// drilling in one colour, the drilled shell over it, and the cutter's own
/// rectangle. This is the render, drawn from the same geometry the numbers
/// come from — Blender's only scripting interface is Python, which this
/// project does not admit, and a slice through the axis shows more of the
/// question than a shaded view would anyway.
fn write_section_svg(
    path: &Path,
    shell: &Mesh,
    drilled: &Mesh,
    hole: &HoleRecord,
) -> Result<(), String> {
    // In-plane basis: the bore direction, and one perpendicular. The cutting
    // plane's normal is the remaining perpendicular, so the plane contains the
    // axis and the section shows the bore along its own centreline.
    let d = hole.direction;
    let reference: V3 = if d[0].abs() <= d[1].abs() && d[0].abs() <= d[2].abs() {
        [1.0, 0.0, 0.0]
    } else if d[1].abs() <= d[2].abs() {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let g = normalize(cross(reference, d));
    let n = cross(d, g);

    let view_lo = hole.t_start - 2.0;
    let view_hi = hole.t_end + 2.0;
    let half = 8.0_f64.max(hole.radius_mm * 4.0);
    let scale_px = 34.0;
    let width = (view_hi - view_lo) * scale_px;
    let height = 2.0 * half * scale_px;
    // t runs left to right, g runs down the page.
    let to_px = |t: f64, s: f64| ((t - view_lo) * scale_px, (s + half) * scale_px);

    let mut body = String::new();
    for (mesh, colour, stroke) in [(shell, "#6b7280", 0.9_f64), (drilled, "#22d3ee", 1.6)] {
        let mut path_data = String::new();
        for segment in slice_plane(mesh, hole.center, n) {
            let a = to_px(
                dot(sub(segment.0, hole.center), d),
                dot(sub(segment.0, hole.center), g),
            );
            let b = to_px(
                dot(sub(segment.1, hole.center), d),
                dot(sub(segment.1, hole.center), g),
            );
            if a.0 < -50.0 || a.0 > width + 50.0 || b.0 < -50.0 || b.0 > width + 50.0 {
                continue;
            }
            path_data.push_str(&format!(
                "M{:.2} {:.2}L{:.2} {:.2}",
                a.0, a.1, b.0, b.1
            ));
        }
        body.push_str(&format!(
            "<path d=\"{path_data}\" stroke=\"{colour}\" stroke-width=\"{stroke}\" fill=\"none\"/>\n"
        ));
    }
    // The cutter rectangle, and the nominal bore width as a dashed pair.
    let (x0, y0) = to_px(hole.t_start, -hole.radius_mm);
    let (x1, y1) = to_px(hole.t_end, hole.radius_mm);
    body.push_str(&format!(
        "<rect x=\"{x0:.2}\" y=\"{y0:.2}\" width=\"{:.2}\" height=\"{:.2}\" \
         stroke=\"#f59e0b\" stroke-width=\"1.2\" fill=\"none\" stroke-dasharray=\"6 4\"/>\n",
        x1 - x0,
        y1 - y0
    ));
    let (_, yc) = to_px(0.0, hole.clear_radius_mm);
    let (_, ync) = to_px(0.0, -hole.clear_radius_mm);
    for y in [yc, ync] {
        body.push_str(&format!(
            "<line x1=\"0\" y1=\"{y:.2}\" x2=\"{width:.2}\" y2=\"{y:.2}\" \
             stroke=\"#a3e635\" stroke-width=\"0.8\" stroke-dasharray=\"3 3\"/>\n"
        ));
    }
    let (xz, _) = to_px(0.0, 0.0);
    body.push_str(&format!(
        "<line x1=\"{xz:.2}\" y1=\"0\" x2=\"{xz:.2}\" y2=\"{height:.2}\" \
         stroke=\"#f472b6\" stroke-width=\"0.8\"/>\n"
    ));
    let caption = format!(
        "{} {} R={:.2} mm | realized opening radius {:.4} mm ({:.4} of nominal) | \
         outer margin {:.3} mm | cutter span [{:.1}, {:.1}] mm",
        hole.source,
        hole.role,
        hole.radius_mm,
        hole.clear_radius_mm,
        hole.clear_radius_mm / hole.radius_mm,
        hole.min_outer_margin_mm,
        hole.t_start,
        hole.t_end
    );
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.0}\" height=\"{:.0}\" \
         viewBox=\"0 0 {width:.2} {:.2}\">\n\
         <rect width=\"100%\" height=\"100%\" fill=\"#0b0f14\"/>\n{body}\
         <text x=\"8\" y=\"{:.2}\" fill=\"#e5e7eb\" font-family=\"monospace\" \
         font-size=\"13\">{caption}</text>\n</svg>\n",
        height + 26.0,
        height + 26.0,
        height + 18.0
    );
    std::fs::write(path, svg).map_err(|error| format!("write {}: {error}", path.display()))
}

/// Every triangle/plane intersection segment, gathered in one pass.
fn slice_plane(mesh: &Mesh, point: V3, normal: V3) -> Vec<(V3, V3)> {
    let verts = mesh.verts();
    let mut out = Vec::new();
    for tri in mesh.tris() {
        let p = [
            verts[tri[0] as usize],
            verts[tri[1] as usize],
            verts[tri[2] as usize],
        ];
        let s = p.map(|v| dot(sub(v, point), normal));
        let mut crossings = Vec::with_capacity(2);
        for edge in 0..3 {
            let (a, b) = (edge, (edge + 1) % 3);
            if (s[a] > 0.0) == (s[b] > 0.0) {
                continue;
            }
            let denominator = s[a] - s[b];
            if denominator == 0.0 {
                continue;
            }
            let ratio = s[a] / denominator;
            crossings.push(add(p[a], scale(sub(p[b], p[a]), ratio)));
        }
        if crossings.len() == 2 {
            out.push((crossings[0], crossings[1]));
        }
    }
    out
}

/// Binary STL, written in one gathered buffer rather than per-triangle writes.
fn write_binary_stl(path: &Path, mesh: &Mesh) -> Result<(), String> {
    let verts = mesh.verts();
    let tris = mesh.tris();
    let mut bytes = Vec::with_capacity(84 + tris.len() * 50);
    bytes.extend_from_slice(&[0_u8; 80]);
    bytes.extend_from_slice(&(tris.len() as u32).to_le_bytes());
    for tri in tris {
        let p = [
            verts[tri[0] as usize],
            verts[tri[1] as usize],
            verts[tri[2] as usize],
        ];
        let normal = normalize_or_zero(cross(sub(p[1], p[0]), sub(p[2], p[0])));
        for value in normal {
            bytes.extend_from_slice(&(value as f32).to_le_bytes());
        }
        for point in p {
            for value in point {
                bytes.extend_from_slice(&(value as f32).to_le_bytes());
            }
        }
        bytes.extend_from_slice(&0_u16.to_le_bytes());
    }
    std::fs::write(path, bytes).map_err(|error| format!("write {}: {error}", path.display()))
}

fn normalize_or_zero(a: V3) -> V3 {
    let n = norm(a);
    if n > 0.0 {
        scale(a, 1.0 / n)
    } else {
        [0.0, 0.0, 0.0]
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| path.display().to_string(), |s| s.to_string_lossy().into())
}

fn fin3(value: [Finite; 3]) -> V3 {
    [value[0].get(), value[1].get(), value[2].get()]
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}
fn normalize(a: V3) -> V3 {
    let n = norm(a);
    scale(a, 1.0 / n)
}
