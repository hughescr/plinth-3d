//! Generator for `hollow_drained.chitubox` -- the KG-4 `kg_recess_posted`
//! design of `docs/specs/audit-known-good.md` §3.4, built at this task's
//! assigned directory name (`hollow_drained`) rather than the spec's own
//! fixture id (same convention as `fixtures/long_ledge`, whose own
//! generate.rs states the same substitution for `span_ledge_long`).
//!
//! Geometry: an 8.40 x 8.40 x 4.50 mm block with a 4.20 x 4.20 mm square
//! recess milled 3.00 mm up into its underside (roof 1.50 mm thick, open
//! downward so nothing traps resin), floating 4.00 mm above the plate.
//! Sixteen posts on a 2.10 mm grid: twelve seat on the rim's underside,
//! four run up inside the recess mouth and seat on the roof's underside.
//! Every post carries an equal 2.1 x 2.1 mm = 4.41 mm^2 cell by symmetry.
//!
//! Not built by this crate's own `Cargo.toml` (there is no `[[bin]]` entry
//! for it) -- kept alongside the fixture as the exact, re-runnable record of
//! how the bytes were produced, same convention as `fixtures/starved_patch`,
//! `fixtures/double_load` and `fixtures/long_ledge`'s own `generate.rs`
//! files. To regenerate: copy this file into a throwaway crate with
//! `plinth-audit` as a `path` dependency, `cargo build --release`, then run
//! the binary with one argument, the output `.chitubox` path. The ten
//! byte-identical copies in this directory (`hollow_drained*.chitubox`) are
//! then just `cp` of that one output -- see README.md "Reproducing".
//!
//! All ten gating checks were run against the output of this exact
//! generator via `plinth-audit <path> --json`, not read from `src/checks/*`
//! -- see README.md "What I did and did not read".

use plinth_audit::reader::RecordKind;
use plinth_audit::synth::{sphere_soup, SynthFile, SynthModel, SynthRecord};
use std::f64::consts::PI;

// ---- design constants, all derived by hand in the fixture README ----------
const OUT: f64 = 4.2; // outer half-width, x and y
const IN: f64 = 2.1; // recess half-width, x and y
const Z0: f64 = -2.25; // rim underside / block bottom, bounding-box-centred
const Z1: f64 = 0.75; // roof underside / cavity ceiling
const Z2: f64 = 2.25; // block top
const Z_PLATE: f64 = -6.25; // Z0 - 4.00 mm float height
// The reconstruction formula (confirmed against fixtures/double_load's own
// comment, and against the "z_plate=-8.5" this generator's FIRST draft
// measured when it wrongly set translate_z = -Z_PLATE) is
//   z_plate = mesh.bbox.min.z - translate_z
// and the mesh here is bounding-box-centred so bbox.min.z = -Z2 = -2.25, NOT
// 0. audit-known-good.md sec 2.3's shorthand "z_plate = -translate_z"
// implicitly assumes a mesh whose bbox.min.z is 0; solving the real formula
// for translate_z gives translate_z = bbox.min.z - z_plate = -2.25 -
// (-6.25) = 4.00 -- which is exactly the "floats 4.00 mm above the plate"
// figure common to all five KG files, so translate_z is just that number.
const TRANSLATE_Z: f64 = 4.00;

const A_CELL: f64 = 4.41; // 2.1 * 2.1, exact by the bisector symmetry
const BETA: f64 = 0.60;

/// Step an f32 value by `n` ULPs (n may be negative).
fn step_f32(x: f32, n: i64) -> f32 {
    let bits = x.to_bits() as i64;
    f32::from_bits((bits + n) as u32)
}

/// R6 (audit-known-good.md sec 2.4): a tip radius `r_tip` is "clean" for the
/// ball_factor=1.5 relationship when `1.5*r_tip`, ROUNDED TO f32 for storage
/// on the TIP record, divides back by 1.5 and rounds to EXACTLY `r_tip`
/// again. Without this, R (the TIP's own stored radius) and r_tip (stored
/// independently on the TAPER's r_start) are two DIFFERENT f32 roundings of
/// the same real design number, and I4's summed-capacity bookkeeping --
/// found empirically to disagree with this generator's own achieved-sum
/// arithmetic by ~4e-6 mm^2 on the first draft, four orders above the 1e-9
/// floor -- reads whichever of the two is authoritative. Filtering the ULP
/// sweep to only "clean" r_tip candidates removes the ambiguity: both
/// storage sites hold the identical value, so it does not matter which one
/// any downstream reconstruction trusts.
///
/// MEASURED refinement: the first version of this filter only asked for a
/// round-trip through TWO f32 roundings to land back on `r_tip_f32`, which
/// is weaker than what the audit actually needs. Its own I1/I2/I4 output
/// (`r_tip_a` = R/1.5, computed and used at FULL f64 precision, never
/// re-rounded to f32) disagreed with the stored taper radius by ~1.06e-8 mm
/// per contact even when the round-trip filter passed -- explaining almost
/// exactly the 4.10e-6 mm^2 / 16-contact I4(d) residual this generator first
/// measured. The condition that actually matches R/1.5-at-f64-precision to
/// the stored r_tip is stricter: `1.5 * r_tip_f64` must be EXACTLY
/// representable in f32 with NO rounding at all (not merely round-trip
/// stable), so that `R_f32_as_f64 / 1.5` reproduces `r_tip_f64` exactly
/// rather than merely to the nearest f32.
fn clean_ball_radius(r_tip_f32: f32) -> Option<f32> {
    let r_tip = f64::from(r_tip_f32);
    let exact_product = 1.5 * r_tip;
    let ball_f32 = exact_product as f32;
    (f64::from(ball_f32) == exact_product).then_some(ball_f32)
}

fn drop_zero_area(tris: Vec<[[f32; 3]; 3]>) -> Vec<[[f32; 3]; 3]> {
    tris.into_iter()
        .filter(|t| {
            let a = [
                (t[1][0] - t[0][0]) as f64,
                (t[1][1] - t[0][1]) as f64,
                (t[1][2] - t[0][2]) as f64,
            ];
            let b = [
                (t[2][0] - t[0][0]) as f64,
                (t[2][1] - t[0][1]) as f64,
                (t[2][2] - t[0][2]) as f64,
            ];
            let cx = a[1] * b[2] - a[2] * b[1];
            let cy = a[2] * b[0] - a[0] * b[2];
            let cz = a[0] * b[1] - a[1] * b[0];
            (cx * cx + cy * cy + cz * cz).sqrt() > 1e-9
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Axis-aligned rectangle -> two triangles, outward winding, matching the
// exact convention `synth::box_soup` uses for each of its six faces (derived
// by hand from that function's FACES table, not copied from any check).
// ---------------------------------------------------------------------------

fn p(x: f64, y: f64, z: f64) -> [f32; 3] {
    [x as f32, y as f32, z as f32]
}

/// Horizontal rectangle at height `z`. `down`: outward normal -z (the
/// underside of material above it) when true, +z (topside of material
/// below it) when false.
fn horiz_rect(xmin: f64, xmax: f64, ymin: f64, ymax: f64, z: f64, down: bool) -> Vec<[[f32; 3]; 3]> {
    let a = p(xmin, ymin, z);
    let b = p(xmax, ymin, z);
    let cc = p(xmax, ymax, z);
    let d = p(xmin, ymax, z);
    if down {
        vec![[a, cc, b], [a, d, cc]]
    } else {
        vec![[a, b, cc], [a, cc, d]]
    }
}

/// Vertical wall at constant y = `yc`, spanning x in [xmin,xmax], z in
/// [zmin,zmax]. `normal_plus_y`: outward normal +y when true, -y when false.
fn wall_const_y(xmin: f64, xmax: f64, yc: f64, zmin: f64, zmax: f64, normal_plus_y: bool) -> Vec<[[f32; 3]; 3]> {
    let (x0, x1) = if normal_plus_y { (xmax, xmin) } else { (xmin, xmax) };
    let p0 = p(x0, yc, zmin);
    let p1 = p(x1, yc, zmin);
    let p2 = p(x1, yc, zmax);
    let p3 = p(x0, yc, zmax);
    vec![[p0, p1, p2], [p0, p2, p3]]
}

/// Vertical wall at constant x = `xc`, spanning y in [ymin,ymax], z in
/// [zmin,zmax]. `normal_plus_x`: outward normal +x when true, -x when false.
fn wall_const_x(xc: f64, ymin: f64, ymax: f64, zmin: f64, zmax: f64, normal_plus_x: bool) -> Vec<[[f32; 3]; 3]> {
    let (y0, y1) = if normal_plus_x { (ymin, ymax) } else { (ymax, ymin) };
    let p0 = p(xc, y0, zmin);
    let p1 = p(xc, y1, zmin);
    let p2 = p(xc, y1, zmax);
    let p3 = p(xc, y0, zmax);
    vec![[p0, p1, p2], [p0, p2, p3]]
}

/// Voxel-boundary construction. A first draft built each planar region
/// (rim underside, outer walls, top, cavity walls, cavity ceiling) as ONE
/// rectangle per region, `front-strip`-style. That produces T-JUNCTIONS:
/// e.g. the rim underside's edge at y = -IN runs the full x in [-OUT,OUT]
/// with vertices only at its own two ends, while the three pieces that
/// border it there (the outer-wall corner cell, the cavity wall, and the
/// opposite corner cell) each introduce a vertex at x = +-IN that the single
/// long edge does not have. `i.e. one edge meets three` is not a manifold
/// join under exact-vertex welding, and the audit correctly reported it:
/// `FILE_MESH_NOT_CLOSED boundary_edges=16`. Building on the SAME grid
/// everywhere -- one rectangle per grid cell face, on both sides of every
/// internal boundary -- removes the T-junctions by construction, because
/// every edge is then shared by exactly the two faces that meet there, at
/// the same subdivision. This is plain voxel surface extraction, not
/// anything read from a check.
fn build_mesh() -> Vec<[[f32; 3]; 3]> {
    let xedges = [-OUT, -IN, IN, OUT];
    let yedges = [-OUT, -IN, IN, OUT];
    let zedges = [Z0, Z1, Z2];

    // cz=0: the lower layer, z in [Z0,Z1] -- solid except the centre cell
    // (the recess). cz=1: the upper layer, z in [Z1,Z2] -- solid everywhere
    // (rim and roof are one continuous volume there).
    let occupied = |cx: i32, cy: i32, cz: i32| -> bool {
        if !(0..3).contains(&cx) || !(0..3).contains(&cy) || !(0..2).contains(&cz) {
            return false;
        }
        cz == 1 || !(cx == 1 && cy == 1)
    };

    let mut m = Vec::new();
    for cx in 0..3i32 {
        for cy in 0..3i32 {
            for cz in 0..2i32 {
                if !occupied(cx, cy, cz) {
                    continue;
                }
                let xmin = xedges[cx as usize];
                let xmax = xedges[cx as usize + 1];
                let ymin = yedges[cy as usize];
                let ymax = yedges[cy as usize + 1];
                let zmin = zedges[cz as usize];
                let zmax = zedges[cz as usize + 1];

                if !occupied(cx, cy, cz - 1) {
                    m.extend(horiz_rect(xmin, xmax, ymin, ymax, zmin, true)); // -z
                }
                if !occupied(cx, cy, cz + 1) {
                    m.extend(horiz_rect(xmin, xmax, ymin, ymax, zmax, false)); // +z
                }
                if !occupied(cx - 1, cy, cz) {
                    m.extend(wall_const_x(xmin, ymin, ymax, zmin, zmax, false)); // -x
                }
                if !occupied(cx + 1, cy, cz) {
                    m.extend(wall_const_x(xmax, ymin, ymax, zmin, zmax, true)); // +x
                }
                if !occupied(cx, cy - 1, cz) {
                    m.extend(wall_const_y(xmin, xmax, ymin, zmin, zmax, false)); // -y
                }
                if !occupied(cx, cy + 1, cz) {
                    m.extend(wall_const_y(xmin, xmax, ymax, zmin, zmax, true)); // +y
                }
            }
        }
    }
    m
}

/// Reconstruct the down-facing demand area exactly as the audit would: sum
/// the area of every triangle whose outward normal has a negative z
/// component, using the STORED f32 vertices promoted to f64. Plain triangle
/// geometry, not read from any check.
fn reconstruct_down_facing_area(mesh: &[[[f32; 3]; 3]]) -> f64 {
    let mut total = 0.0f64;
    for t in mesh {
        let v0 = [t[0][0] as f64, t[0][1] as f64, t[0][2] as f64];
        let v1 = [t[1][0] as f64, t[1][1] as f64, t[1][2] as f64];
        let v2 = [t[2][0] as f64, t[2][1] as f64, t[2][2] as f64];
        let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
        let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
        let cross = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let area2 = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        if cross[2] < 0.0 {
            total += 0.5 * area2;
        }
    }
    total
}

struct Post {
    x: f64,
    y: f64,
    inner: bool,
}

fn posts() -> Vec<Post> {
    let coords: [f64; 4] = [-3.15, -1.05, 1.05, 3.15];
    let mut v = Vec::new();
    for &x in &coords {
        for &y in &coords {
            let inner = x.abs() < IN && y.abs() < IN;
            v.push(Post { x, y, inner });
        }
    }
    v
}

fn main() {
    let mesh = build_mesh();
    let d_total = reconstruct_down_facing_area(&mesh);
    eprintln!("reconstructed D_total = {d_total:.12} mm^2 (design target 70.56)");

    let posts = posts();
    assert_eq!(posts.len(), 16);
    let n_inner = posts.iter().filter(|p| p.inner).count();
    assert_eq!(n_inner, 4);

    // ---- I4(d) exactness: 15 posts share a common f32 tip radius, the 16th
    // is solved so the f64 sum 12*pi*sum(r_i^2) matches d_total as closely
    // as an f32-resolution search can land it (same method as
    // fixtures/starved_patch and fixtures/double_load's generate.rs).
    let r_tip_design = (A_CELL / (12.0 * PI)).sqrt();
    let target_sum_sq = d_total / (12.0 * PI); // sum(r_i^2) target
    let n_shared = (posts.len() - 1) as f64; // 15

    // Deliberately NARROW range, centred on the design value: r_a must stay
    // physically indistinguishable from r_tip_design (it is what I2/I3 grade
    // for those 15 posts), so this sweep is only meant to absorb the last
    // few ULPs of f32-rounding slack, not to go hunting for an accidental
    // low residual far from the design point. +-2000 ULPs at r~0.34 is
    // +-4.8e-5 mm, already three orders below anything visible.
    let mut best: Option<(f64, f32, f32)> = None; // (residual, r_a_f32, r_last_f32)
    for step in -2_000i64..=2_000 {
        let r_a_f32 = step_f32(r_tip_design as f32, step);
        if clean_ball_radius(r_a_f32).is_none() {
            continue;
        }
        let r_a = f64::from(r_a_f32);
        if r_a <= 0.0 {
            continue;
        }
        let last_sq = target_sum_sq - n_shared * r_a * r_a;
        if last_sq <= 0.0 {
            continue;
        }
        // Search nearby f32 candidates for the last tip too, since the exact
        // sqrt is very unlikely to itself be "clean".
        let approx = last_sq.sqrt() as f32;
        for lstep in -4_000i64..=4_000 {
            let r_last_f32 = step_f32(approx, lstep);
            if clean_ball_radius(r_last_f32).is_none() {
                continue;
            }
            let r_last = f64::from(r_last_f32);
            if r_last <= 0.0 {
                continue;
            }
            let sum_sq = n_shared * r_a * r_a + r_last * r_last;
            let achieved = 12.0 * PI * sum_sq;
            let residual = (achieved - d_total).abs();
            if best.is_none_or(|(bres, ..)| residual < bres) {
                best = Some((residual, r_a_f32, r_last_f32));
            }
        }
    }
    let (residual, r_a_f32, r_last_f32) = best.expect("nonempty ULP sweep");
    eprintln!(
        "ULP sweep: r_a={r_a_f32:.10} r_last={r_last_f32:.10} residual={residual:.3e} mm^2 (target <1e-9), \
         |r_last - r_a|={:.3e} mm",
        (f64::from(r_last_f32) - f64::from(r_a_f32)).abs()
    );

    let r_a = f64::from(r_a_f32);
    let r_last = f64::from(r_last_f32);
    let r_arm_design = 1.5 * r_a; // shared arm radius, one value used everywhere

    let mut records = Vec::new();
    for (i, post) in posts.iter().enumerate() {
        let r_tip = if i == posts.len() - 1 { r_last } else { r_a };
        let r_ball = 1.5 * r_tip;
        let r_arm = 1.5 * r_tip; // "r_arm = 1.5 * r_tip" per audit-known-good.md sec 2.1 step 5
        let z_face = if post.inner { Z1 } else { Z0 };

        // p0: stored as f32, so the DECLARED depth is recomputed from the
        // stored value, never from the intended real h (rule R7).
        let h_intended = BETA * r_tip;
        let p0_z_f32 = (z_face + h_intended) as f32;
        let p0_z = f64::from(p0_z_f32);
        let z_face_f32 = z_face as f32; // Z0/Z1 are exact in binary32 (2.25, 0.75)
        let declared_depth = p0_z - f64::from(z_face_f32);

        let p0 = [post.x, post.y, p0_z];
        let ball_centre = [post.x, post.y, p0_z - r_ball];
        let tip_end = [post.x, post.y, p0_z - 2.0 * r_ball];
        // audit-known-good.md sec 2.2's table fixes the taper's drop at a
        // flat 0.50 mm below the ball centre, i.e. 0.50 mm regardless of R.
        // That is safe for KG-1 (R=0.4886 mm < 0.50), but THIS file's own
        // r_tip gives R=0.5130 mm > 0.50 mm -- the ball's bottom pole then
        // sits 0.013 mm BELOW the fixed taper end, so a member starting
        // right there (near-zero graph-join gap, needed so I5/I3's
        // record-adjacency graph -- which links records by DECLARED
        // endpoint coincidence within ~0.02 mm, not by 3D solid overlap --
        // actually treats taper and member as one component) comes within
        // 0.013 mm of the tip's OWN geometry: inside the d3=0.0553 mm
        // fusion-uncertain band, and PATH_INDETERMINATE fires between the
        // tip and the member. MEASURED, not assumed: the flat 0.50 mm drop
        // reproduced this exactly (see the README's "What the audit found,
        // and what I changed" section). The fix used here is a taper drop
        // of R + 0.10 mm (instead of the flat 0.50 mm), which guarantees at
        // least 0.10 mm of clearance between the tip and whatever starts at
        // the taper's end, on ANY r_tip -- a strictly more conservative
        // version of the same rule, not a different one.
        let taper_end = [post.x, post.y, ball_centre[2] - (r_ball + 0.10)];
        let member_start = taper_end;
        let member_end = [post.x, post.y, Z_PLATE + 0.90];
        let flare_start = member_end;
        let flare_end = [post.x, post.y, Z_PLATE];

        let tip = SynthRecord::new(RecordKind::ContactSphere, p0, tip_end, r_ball, r_ball)
            .with_soup(drop_zero_area(sphere_soup(ball_centre, r_ball, 8, 16)));

        let taper = SynthRecord::new(RecordKind::ContactTaper, ball_centre, taper_end, r_tip, r_arm)
            .with_declared_depth(declared_depth)
            .with_matching_soup();

        let member = SynthRecord::new(RecordKind::Member, member_start, member_end, r_arm, r_arm)
            .with_matching_soup();

        let flare = SynthRecord::new(RecordKind::RootFlare, flare_start, flare_end, r_arm, 0.90)
            .with_matching_soup();

        records.push(tip);
        records.push(taper);
        records.push(member);
        records.push(flare);
    }
    assert_eq!(records.len(), 64);

    eprintln!("r_arm (shared design value) = {r_arm_design:.10}");

    let model = SynthModel {
        name: "hollow_drained".into(),
        translate_mm: [0.0, 0.0, TRANSLATE_Z],
        mesh,
        records,
    };
    let file = SynthFile::new().with_model(model);

    let out = std::env::args()
        .nth(1)
        .expect("usage: gen_hollow_drained <out.chitubox>");
    file.write(std::path::Path::new(&out)).expect("write");
    eprintln!("wrote {out}");
}
