//! THROWAWAY INSTRUMENTATION -- FU-50 raster-rewrite delta harness.
//!
//! The ruling (decisions/2026-08-12-owner-rulings-fu-batch.md §3) requires
//! the new area-proportional conservative rasterizer's marked-voxel set to be
//! measured against the frozen midpoint-quartering rasterizer's, on the
//! committed corpus, **before** the new rasterizer ships. This binary is that
//! measurement, read-only: it loads mesh geometry, runs both rasterizers
//! locally (production no longer has the old one; see below), and prints one
//! row per model.
//!
//! `cargo run --release --offline --example raster_delta -p plinth-hollow`
//!
//! runs the eight committed `corpus/baseline/*.prepped*.stl` models. Extra
//! models -- `Round 50.stl` and `Round 100.stl`, which live outside the repo
//! under `minis-to-process/` and are read-only, never committed -- are added
//! with repeated `--model <path>` arguments:
//!
//! ```text
//! cargo run --release --offline --example raster_delta -p plinth-hollow -- \
//!   --model "/Users/craig/code/hughescr/dnd-campaigns/minis-to-process/Round 50.stl" \
//!   --model "/Users/craig/code/hughescr/dnd-campaigns/minis-to-process/Round 100.stl"
//! ```
//!
//! **Why this duplicates two whole rasterizers instead of calling the crate.**
//! `raster_work_item_count`/`voxelize_filled`/`walk_raster_candidates` and the
//! frozen `walk_raster_leaves` they replaced are crate-private, and the old
//! one is gone from production entirely (FU-50's dead-code removal). The only
//! way to measure the delta between "what the old rasterizer would have
//! marked" and "what the new one marks" is to run both, so both are
//! reproduced here verbatim as a read-only reference oracle -- exactly the
//! `#[cfg(test)]`/example-only retention the ruling calls for. The in-crate
//! unit test `frozen_walker_marks_a_subset_of_the_area_proportional_rasterizer`
//! makes the same superset claim mechanically, against the *real* production
//! `walk_raster_candidates` (this binary's production-side numbers are cross-
//! checked against that test's fixtures, not asserted by it).
//!
//! Every other column -- cavity voxels, cavity volume, post-fill material,
//! and an approximate realized minimum wall -- is computed locally too, from
//! whichever marked set the row is about, using local copies of
//! `fill_voxel_holes` and the padded EDT wrapper (`frozen_distance_transform`
//! in `lib.rs`). This keeps every number in one row internally consistent
//! (same fill, same EDT, same wall threshold) even though the "new" column
//! duplicates logic the production path also runs.
//!
//! **The approximate realized minimum wall** is not FU-55's measurement
//! primitive (not landed as of this writing) -- it is the minimum EDT
//! distance among material voxels 6-connected-adjacent to a selected cavity
//! voxel, which reads as a local wall thickness at the thinnest point the
//! cavity selection touches. Reported and labelled as an approximation.

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use plinth_hollow::{H0AdmissionLimits, HollowSettings};
use plinth_mesh::{load_binary_stl, Mesh, WeldPolicy};
use scirs2_core::ndarray::Array3;
use scirs2_ndimage::distance_transform_edt;
use scirs2_ndimage::morphology3d::label3d;

/// Above this the old walker is reported as "would exceed" rather than run to
/// completion -- a diagnostic ceiling only, exactly as the FU-50 journal row's
/// own 37-mini measurement used one. It is far above every measured demand on
/// this corpus (worst known case, `Round 100`, is ~22.5M) and is never read by
/// any production code path.
const OLD_WALKER_DIAGNOSTIC_CEILING: usize = 200_000_000;

#[derive(Clone, Copy)]
struct GridSpec {
    origin: [f64; 3],
    dims: [usize; 3],
    pitch: f64,
    count: usize,
}

fn round_ties_even(value: f64) -> f64 {
    let lower = value.floor();
    let fractional = value - lower;
    if fractional < 0.5 {
        lower
    } else if fractional > 0.5 {
        lower + 1.0
    } else if lower.rem_euclid(2.0) == 0.0 {
        lower
    } else {
        lower + 1.0
    }
}

fn grid_spec(mesh: &Mesh, pitch: f64) -> GridSpec {
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for vertex in mesh.verts() {
        for axis in 0..3 {
            low[axis] = low[axis].min(vertex[axis]);
            high[axis] = high[axis].max(vertex[axis]);
        }
    }
    let mut origin = [0.0; 3];
    let mut dims = [0_usize; 3];
    for axis in 0..3 {
        let origin_index = round_ties_even(low[axis] / pitch);
        let end_index = round_ties_even(high[axis] / pitch);
        origin[axis] = origin_index * pitch;
        dims[axis] = (end_index - origin_index) as usize + 1;
    }
    let count = dims[0] * dims[1] * dims[2];
    GridSpec {
        origin,
        dims,
        pitch,
        count,
    }
}

fn mark_index(cell: [usize; 3], spec: GridSpec, occupancy: &mut [bool]) {
    let flat = cell[0] + cell[1] * spec.dims[0] + cell[2] * spec.dims[0] * spec.dims[1];
    occupancy[flat] = true;
}

// ---------------------------------------------------------------------
// The frozen midpoint-quartering rasterizer, reproduced verbatim as a
// read-only reference oracle (FU-50 removed it from production).
// ---------------------------------------------------------------------

const MAX_RASTER_SUBDIVISION_ROUNDS: u8 = 10;

#[derive(Clone, Copy)]
struct RasterNode {
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
    depth: u8,
}

fn midpoint(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        (left[0] + right[0]) / 2.0,
        (left[1] + right[1]) / 2.0,
        (left[2] + right[2]) / 2.0,
    ]
}

fn edge_too_long(left: [f64; 3], right: [f64; 3], max_edge: f64) -> bool {
    left.iter()
        .zip(right)
        .map(|(left, right)| (left - right).powi(2))
        .sum::<f64>()
        .sqrt()
        > max_edge
}

fn raster_leaf(node: RasterNode, pitch: f64) -> bool {
    let max_edge = pitch / 2.0;
    !edge_too_long(node.a, node.b, max_edge)
        && !edge_too_long(node.b, node.c, max_edge)
        && !edge_too_long(node.c, node.a, max_edge)
}

fn mark_vertex_old(point: [f64; 3], spec: GridSpec, occupancy: &mut [bool]) {
    let mut index = [0_usize; 3];
    for axis in 0..3 {
        let grid = round_ties_even(point[axis] / spec.pitch);
        let origin = round_ties_even(spec.origin[axis] / spec.pitch);
        index[axis] = (grid - origin) as usize;
    }
    mark_index(index, spec, occupancy);
}

/// `Ok(work_items)` on completion, `Err(())` if the diagnostic ceiling or the
/// frozen ten-round subdivision limit was hit -- both purely measurement
/// bookkeeping, never a production cap.
fn old_marked_set(mesh: &Mesh, spec: GridSpec) -> Result<(usize, Vec<bool>), &'static str> {
    let mut occupancy = vec![false; spec.count];
    let mut count = 0_usize;
    for triangle in mesh.tris() {
        let [ia, ib, ic] = *triangle;
        let point = |index: u32| mesh.verts()[index as usize];
        let mut stack = vec![RasterNode {
            a: point(ia),
            b: point(ib),
            c: point(ic),
            depth: 0,
        }];
        while let Some(node) = stack.pop() {
            if raster_leaf(node, spec.pitch) {
                count += 1;
                if count > OLD_WALKER_DIAGNOSTIC_CEILING {
                    return Err("exceeded the diagnostic work-item ceiling");
                }
                mark_vertex_old(node.a, spec, &mut occupancy);
                mark_vertex_old(node.b, spec, &mut occupancy);
                mark_vertex_old(node.c, spec, &mut occupancy);
                continue;
            }
            if node.depth >= MAX_RASTER_SUBDIVISION_ROUNDS {
                return Err("exceeded the frozen ten-round subdivision limit");
            }
            let ab = midpoint(node.a, node.b);
            let bc = midpoint(node.b, node.c);
            let ca = midpoint(node.c, node.a);
            let next = node.depth + 1;
            stack.push(RasterNode {
                a: ab,
                b: bc,
                c: ca,
                depth: next,
            });
            stack.push(RasterNode {
                a: ca,
                b: bc,
                c: node.c,
                depth: next,
            });
            stack.push(RasterNode {
                a: ab,
                b: node.b,
                c: bc,
                depth: next,
            });
            stack.push(RasterNode {
                a: node.a,
                b: ab,
                c: ca,
                depth: next,
            });
        }
    }
    Ok((count, occupancy))
}

// ---------------------------------------------------------------------
// The area-proportional conservative rasterizer (FU-50), reproduced
// verbatim from `plinth_hollow::lib` for local marked-set comparison.
// The real production copy is what `create_shell` runs; this is only for
// measuring the delta against the old walker above in one process.
// ---------------------------------------------------------------------

const DEGENERATE_NORMAL_SQ: f64 = 1e-18;

fn sub3(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn cross3(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn dot3(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn to_index_space(point: [f64; 3], spec: GridSpec) -> [f64; 3] {
    [
        (point[0] - spec.origin[0]) / spec.pitch,
        (point[1] - spec.origin[1]) / spec.pitch,
        (point[2] - spec.origin[2]) / spec.pitch,
    ]
}

fn triangle_cube_overlaps(tri: [[f64; 3]; 3], center: [f64; 3], half: [f64; 3]) -> bool {
    let v = [
        sub3(tri[0], center),
        sub3(tri[1], center),
        sub3(tri[2], center),
    ];
    let e = [sub3(v[1], v[0]), sub3(v[2], v[1]), sub3(v[0], v[2])];

    for axis in 0..3 {
        let lo = v[0][axis].min(v[1][axis]).min(v[2][axis]);
        let hi = v[0][axis].max(v[1][axis]).max(v[2][axis]);
        if lo > half[axis] || hi < -half[axis] {
            return false;
        }
    }

    const BOX_AXES: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for box_axis in BOX_AXES {
        for edge in e {
            let axis = cross3(box_axis, edge);
            let p0 = dot3(axis, v[0]);
            let p1 = dot3(axis, v[1]);
            let p2 = dot3(axis, v[2]);
            let r = half[0] * axis[0].abs() + half[1] * axis[1].abs() + half[2] * axis[2].abs();
            let lo = p0.min(p1).min(p2);
            let hi = p0.max(p1).max(p2);
            if lo > r || hi < -r {
                return false;
            }
        }
    }

    let normal = cross3(e[0], e[1]);
    let d = dot3(normal, v[0]);
    let r = half[0] * normal[0].abs() + half[1] * normal[1].abs() + half[2] * normal[2].abs();
    d <= r && d >= -r
}

fn dilated_index_aabb(tri: [[f64; 3]; 3], spec: GridSpec) -> [[isize; 2]; 3] {
    let mut axis_range = [[0_isize; 2]; 3];
    for axis in 0..3 {
        let lo = tri[0][axis].min(tri[1][axis]).min(tri[2][axis]) - 0.5;
        let hi = tri[0][axis].max(tri[1][axis]).max(tri[2][axis]) + 0.5;
        let dim_max = (spec.dims[axis].saturating_sub(1)) as isize;
        axis_range[axis] = [(lo.floor() as isize).max(0), (hi.ceil() as isize).min(dim_max)];
    }
    axis_range
}

fn scan_dilated_aabb(
    tri: [[f64; 3]; 3],
    axis_range: [[isize; 2]; 3],
    spec: GridSpec,
    count: &mut usize,
    occupancy: &mut [bool],
) {
    for x in axis_range[0][0]..=axis_range[0][1] {
        for y in axis_range[1][0]..=axis_range[1][1] {
            for z in axis_range[2][0]..=axis_range[2][1] {
                let cell = [x as usize, y as usize, z as usize];
                let center = [x as f64, y as f64, z as f64];
                *count += 1;
                if triangle_cube_overlaps(tri, center, [0.5, 0.5, 0.5]) {
                    mark_index(cell, spec, occupancy);
                }
            }
        }
    }
}

fn walk_triangle_candidates(
    tri: [[f64; 3]; 3],
    spec: GridSpec,
    count: &mut usize,
    occupancy: &mut [bool],
) {
    let axis_range = dilated_index_aabb(tri, spec);
    if axis_range.iter().any(|range| range[0] > range[1]) {
        return;
    }
    let [a, b, c] = tri;
    let normal = cross3(sub3(b, a), sub3(c, a));
    let normal_sq = dot3(normal, normal);
    if !normal_sq.is_finite() || normal_sq < DEGENERATE_NORMAL_SQ {
        scan_dilated_aabb(tri, axis_range, spec, count, occupancy);
        return;
    }
    let k_axis = if normal[0].abs() >= normal[1].abs() && normal[0].abs() >= normal[2].abs() {
        0
    } else if normal[1].abs() >= normal[2].abs() {
        1
    } else {
        2
    };
    let (p_axis, q_axis) = match k_axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let n_k = normal[k_axis];
    for i in axis_range[p_axis][0]..=axis_range[p_axis][1] {
        for j in axis_range[q_axis][0]..=axis_range[q_axis][1] {
            let mut wmin = f64::INFINITY;
            let mut wmax = f64::NEG_INFINITY;
            for dp in [-0.5, 0.5] {
                for dq in [-0.5, 0.5] {
                    let p = i as f64 + dp;
                    let q = j as f64 + dq;
                    let w = a[k_axis]
                        - (normal[p_axis] * (p - a[p_axis]) + normal[q_axis] * (q - a[q_axis]))
                            / n_k;
                    wmin = wmin.min(w);
                    wmax = wmax.max(w);
                }
            }
            let k_lo = ((wmin - 0.5).ceil() as isize).max(axis_range[k_axis][0]);
            let k_hi = ((wmax + 0.5).floor() as isize).min(axis_range[k_axis][1]);
            if k_lo > k_hi {
                continue;
            }
            for k in k_lo..=k_hi {
                let mut cell = [0_usize; 3];
                cell[p_axis] = i as usize;
                cell[q_axis] = j as usize;
                cell[k_axis] = k as usize;
                let center = [cell[0] as f64, cell[1] as f64, cell[2] as f64];
                *count += 1;
                if triangle_cube_overlaps(tri, center, [0.5, 0.5, 0.5]) {
                    mark_index(cell, spec, occupancy);
                }
            }
        }
    }
}

fn new_marked_set(mesh: &Mesh, spec: GridSpec) -> (usize, Vec<bool>) {
    let mut occupancy = vec![false; spec.count];
    let mut count = 0_usize;
    for triangle in mesh.tris() {
        let [ia, ib, ic] = *triangle;
        let tri = [
            to_index_space(mesh.verts()[ia as usize], spec),
            to_index_space(mesh.verts()[ib as usize], spec),
            to_index_space(mesh.verts()[ic as usize], spec),
        ];
        walk_triangle_candidates(tri, spec, &mut count, &mut occupancy);
    }
    (count, occupancy)
}

// ---------------------------------------------------------------------
// Shared fill + EDT, reproduced from `lib.rs`'s private
// `fill_voxel_holes`/`frozen_distance_transform`.
// ---------------------------------------------------------------------

fn fill_voxel_holes(occupancy: &mut [bool], dims: [usize; 3]) {
    let [nx, ny, nz] = dims;
    let plane = nx * ny;
    let mut exterior = vec![false; occupancy.len()];
    let mut queue = Vec::with_capacity(occupancy.len());
    let push = |x: usize, y: usize, z: usize, exterior: &mut [bool], queue: &mut Vec<usize>| {
        let flat = x + y * nx + z * plane;
        if !occupancy[flat] && !exterior[flat] {
            exterior[flat] = true;
            queue.push(flat);
        }
    };
    for z in 0..nz {
        for y in 0..ny {
            push(0, y, z, &mut exterior, &mut queue);
            push(nx - 1, y, z, &mut exterior, &mut queue);
        }
    }
    for z in 0..nz {
        for x in 0..nx {
            push(x, 0, z, &mut exterior, &mut queue);
            push(x, ny - 1, z, &mut exterior, &mut queue);
        }
    }
    for y in 0..ny {
        for x in 0..nx {
            push(x, y, 0, &mut exterior, &mut queue);
            push(x, y, nz - 1, &mut exterior, &mut queue);
        }
    }
    let mut head = 0;
    while head < queue.len() {
        let flat = queue[head];
        head += 1;
        let z = flat / plane;
        let remaining = flat % plane;
        let y = remaining / nx;
        let x = remaining % nx;
        if x > 0 {
            push(x - 1, y, z, &mut exterior, &mut queue);
        }
        if x + 1 < nx {
            push(x + 1, y, z, &mut exterior, &mut queue);
        }
        if y > 0 {
            push(x, y - 1, z, &mut exterior, &mut queue);
        }
        if y + 1 < ny {
            push(x, y + 1, z, &mut exterior, &mut queue);
        }
        if z > 0 {
            push(x, y, z - 1, &mut exterior, &mut queue);
        }
        if z + 1 < nz {
            push(x, y, z + 1, &mut exterior, &mut queue);
        }
    }
    for (filled, exterior) in occupancy.iter_mut().zip(exterior) {
        if !exterior {
            *filled = true;
        }
    }
}

fn padded_edt(material: &Array3<bool>, pitch: f64) -> Array3<f64> {
    let (nz, ny, nx) = material.dim();
    let bordered = Array3::from_shape_fn((nz + 2, ny + 2, nx + 2), |(z, y, x)| {
        z >= 1
            && z <= nz
            && y >= 1
            && y <= ny
            && x >= 1
            && x <= nx
            && material[[z - 1, y - 1, x - 1]]
    });
    let (distances, _) = distance_transform_edt(&bordered.into_dyn(), Some(&[pitch; 3]), true, false)
        .expect("EDT");
    let distances: Array3<f64> = distances.expect("distances").into_dimensionality().expect("3d");
    Array3::from_shape_fn((nz, ny, nx), |(z, y, x)| distances[[z + 1, y + 1, x + 1]])
}

struct Measurement {
    material_count: usize,
    cavity_voxel_count: usize,
    cavity_volume_mm3: f64,
    approx_min_wall_mm: Option<f64>,
}

fn measure(occupancy: Vec<bool>, spec: GridSpec, wall_mm: f64) -> Measurement {
    let mut occupancy = occupancy;
    fill_voxel_holes(&mut occupancy, spec.dims);
    let material_count = occupancy.iter().filter(|value| **value).count();
    let material = Array3::from_shape_vec((spec.dims[2], spec.dims[1], spec.dims[0]), occupancy)
        .expect("shape");
    let edt = padded_edt(&material, spec.pitch);
    let cavity = edt.mapv(|distance| distance > wall_mm);
    let (labels, label_count) = label3d(cavity.view(), 6).expect("labels");
    let cavity_voxel_count = if label_count == 0 {
        0
    } else {
        let mut counts = vec![0_usize; label_count + 1];
        for label in &labels {
            counts[*label as usize] += 1;
        }
        counts[1..].iter().copied().max().unwrap_or(0)
    };
    let largest_label = if label_count == 0 {
        None
    } else {
        let mut counts = vec![0_usize; label_count + 1];
        for label in &labels {
            counts[*label as usize] += 1;
        }
        counts[1..]
            .iter()
            .enumerate()
            .max_by_key(|(_, count)| **count)
            .map(|(index, _)| index as i32 + 1)
    };
    let approx_min_wall_mm = largest_label.and_then(|label| {
        let (nz, ny, nx) = material.dim();
        let mut min_wall: Option<f64> = None;
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    if labels[[z, y, x]] != label {
                        continue;
                    }
                    // A material 6-neighbour of a selected cavity voxel: its
                    // own EDT value is a local wall-thickness reading.
                    for (dz, dy, dx) in [
                        (-1_i64, 0_i64, 0_i64),
                        (1, 0, 0),
                        (0, -1, 0),
                        (0, 1, 0),
                        (0, 0, -1),
                        (0, 0, 1),
                    ] {
                        let (nz2, ny2, nx2) = (z as i64 + dz, y as i64 + dy, x as i64 + dx);
                        if nz2 < 0 || ny2 < 0 || nx2 < 0 {
                            continue;
                        }
                        let (nz2, ny2, nx2) = (nz2 as usize, ny2 as usize, nx2 as usize);
                        if nz2 >= nz || ny2 >= ny || nx2 >= nx {
                            continue;
                        }
                        if material[[nz2, ny2, nx2]] {
                            let candidate = edt[[nz2, ny2, nx2]];
                            min_wall = Some(min_wall.map_or(candidate, |current: f64| current.min(candidate)));
                        }
                    }
                }
            }
        }
        min_wall
    });
    Measurement {
        material_count,
        cavity_voxel_count,
        cavity_volume_mm3: cavity_voxel_count as f64 * spec.pitch.powi(3),
        approx_min_wall_mm,
    }
}

fn set_difference_count(a: &[bool], b: &[bool]) -> usize {
    a.iter().zip(b).filter(|(a, b)| **a && !**b).count()
}

fn run_model(name: &str, path: &Path, settings: HollowSettings, cap: usize) {
    let mesh = load_binary_stl(path, WeldPolicy::None)
        .unwrap_or_else(|error| panic!("{name}: could not load {}: {error}", path.display()));
    let pitch = settings.voxel_pitch_mm().get();
    let wall_mm = settings.wall_thickness_mm().get();
    let spec = grid_spec(&mesh, pitch);

    println!("== {name} ({} triangles, grid {:?}) ==", mesh.tris().len(), spec.dims);

    let old = old_marked_set(&mesh, spec);
    let (new_count, new_occupancy) = new_marked_set(&mesh, spec);

    match &old {
        Ok((old_count, old_occupancy)) => {
            let new_minus_old = set_difference_count(&new_occupancy, old_occupancy);
            let old_minus_new = set_difference_count(old_occupancy, &new_occupancy);
            println!(
                "  old work items: {old_count} ({:.3}x cap {cap})",
                *old_count as f64 / cap as f64
            );
            println!(
                "  new work items: {new_count} ({:.3}x cap {cap})",
                new_count as f64 / cap as f64
            );
            let old_marked = old_occupancy.iter().filter(|v| **v).count();
            let new_marked = new_occupancy.iter().filter(|v| **v).count();
            println!("  |old marked| = {old_marked}, |new marked| = {new_marked}");
            println!(
                "  |new \\ old| = {new_minus_old}, |old \\ new| = {old_minus_new} (superset property requires 0)"
            );

            let old_measure = measure(old_occupancy.clone(), spec, wall_mm);
            let new_measure = measure(new_occupancy.clone(), spec, wall_mm);
            print_measure("old", &old_measure);
            print_measure("new", &new_measure);
        }
        Err(reason) => {
            println!("  old work items: REFUSED ({reason})");
            println!(
                "  new work items: {new_count} ({:.3}x cap {cap}) -- {}",
                new_count as f64 / cap as f64,
                if new_count <= cap {
                    "PASSES the unchanged production cap"
                } else {
                    "still exceeds the unchanged production cap"
                }
            );
            let new_measure = measure(new_occupancy, spec, wall_mm);
            print_measure("new", &new_measure);
        }
    }
    println!();
}

fn print_measure(label: &str, measurement: &Measurement) {
    println!(
        "  [{label}] post-fill material = {}, cavity voxels = {}, cavity volume = {:.3} mm3, approx min wall = {}",
        measurement.material_count,
        measurement.cavity_voxel_count,
        measurement.cavity_volume_mm3,
        measurement
            .approx_min_wall_mm
            .map_or_else(|| "n/a".to_owned(), |value| format!("{value:.3} mm")),
    );
}

const CORPUS_MODELS: &[&str] = &[
    "Cylinder.prepped.oriented-solid.stl",
    "make_box.prepped.oriented-solid.stl",
    "make_overhang_table.prepped.stl",
    "make_raking_blade.prepped.stl",
    "make_thin_fin.prepped.stl",
    "make_toed_slab.prepped.stl",
    "Sphere.prepped.oriented-solid.stl",
    "Weeping Treant v1.2.prepped.oriented-solid.stl",
];

fn main() -> ExitCode {
    let settings = HollowSettings::from_registered_constants().expect("M0 settings");
    let cap = H0AdmissionLimits::production().max_raster_work_items();

    let baseline = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/baseline");
    let mut extra_models: Vec<PathBuf> = Vec::new();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--model" {
            let Some(path) = args.next() else {
                eprintln!("--model requires a path argument");
                return ExitCode::FAILURE;
            };
            extra_models.push(PathBuf::from(path));
        } else {
            eprintln!("unrecognized argument: {arg}");
            return ExitCode::FAILURE;
        }
    }

    for file in CORPUS_MODELS {
        let path = baseline.join(file);
        if !path.is_file() {
            eprintln!("RequiredCorpusFixtureMissing: {} is committed evidence and must be present", path.display());
            return ExitCode::FAILURE;
        }
        run_model(file, &path, settings, cap);
    }

    for path in &extra_models {
        if !path.is_file() {
            eprintln!("model not found: {}", path.display());
            return ExitCode::FAILURE;
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("<model>");
        run_model(name, path, settings, cap);
    }

    ExitCode::SUCCESS
}
