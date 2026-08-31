//! `spread_wide` — KG-1 (`kg_slab_four_post`) of `docs/specs/audit-known-good.md`
//! §3.1: the trivially-correct known-good case. A 4.00 x 4.00 x 2.00 mm slab
//! floating 4.00 mm over the plate on four posts, one under the centre of
//! each 2x2 mm quadrant of its underside.
//!
//! Every number here is derived in the README from `docs/physics.md` and the
//! printer profile (`src/consts.rs`, restated from `audit-invariants.md`
//! §1.3/§1.5), NOT read from any `src/checks/*.rs` file. See the README's
//! "What I did and did not consult" section.
//!
//! Usage: `gen_spread_wide <out.chitubox>`

use plinth_audit::consts as c;
use plinth_audit::reader::RecordKind;
use plinth_audit::synth::{box_soup, sphere_soup, SynthFile, SynthModel, SynthRecord};

/// `sphere_soup`'s pole triangles carry two bit-identical vertices at
/// `la == 0` and `la == lat-1` (both a documented pre-existing defect in the
/// shared test helper `src/synth.rs`, already reproduced and worked around by
/// `fixtures/double_load/generate.rs`). Rule R8 of `audit-known-good.md` §2.4
/// forbids a zero-area triangle at the poles in a known-good file, so every
/// ball soup in this generator is filtered through this before use.
fn drop_zero_area(tris: Vec<[[f32; 3]; 3]>) -> Vec<[[f32; 3]; 3]> {
    tris.into_iter()
        .filter(|t| {
            let a = [
                f64::from(t[1][0] - t[0][0]),
                f64::from(t[1][1] - t[0][1]),
                f64::from(t[1][2] - t[0][2]),
            ];
            let b = [
                f64::from(t[2][0] - t[0][0]),
                f64::from(t[2][1] - t[0][1]),
                f64::from(t[2][2] - t[0][2]),
            ];
            let cx = a[1] * b[2] - a[2] * b[1];
            let cy = a[2] * b[0] - a[0] * b[2];
            let cz = a[0] * b[1] - a[1] * b[0];
            (cx * cx + cy * cy + cz * cz).sqrt() > 1e-9
        })
        .collect()
}

/// Rule R6 (`audit-known-good.md` §2.4): `r_tip` must be chosen so that
/// `R = 1.5 * r_tip`, evaluated in f32, loses no bits to rounding -- I1
/// triangulates `r_tip` three ways (`R/f`, the taper's own `r_start`, the
/// fitted soup) and fails on any disagreement, so the three must be
/// bit-identical rather than merely close.
fn is_1_5x_exact_f32(r: f32) -> bool {
    let exact = f64::from(r) * 1.5;
    f64::from(exact as f32) == exact
}

fn f32_step(x: f32, steps: i64) -> f32 {
    let bits = i64::from(x.to_bits());
    f32::from_bits((bits + steps) as u32)
}

/// Rule R7: the TAPER's declared contact depth (trailing slot 0) must equal
/// what the STORED f32 fields actually deliver -- `p0.z - z_face`, both
/// rounded to f32 first -- never the intended real-valued `h`. Returns the
/// f64 value to store, computed from f32-rounded inputs so it agrees exactly
/// with what a reader reconstructing from the same bytes will compute.
fn delivered_depth(z_face: f64, p0_z: f64) -> f64 {
    let z_face_f32 = z_face as f32;
    let p0_z_f32 = p0_z as f32;
    f64::from(p0_z_f32) - f64::from(z_face_f32)
}

/// Find `(r_a, r_n, residual)`: three tips at `r_a` and one at `r_n`, all
/// satisfying R6, minimising the I4 clause-(d) residual
/// `|12*pi*(3*r_a^2 + r_n^2) - d_total|` against the floor of `1e-9 mm^2`
/// (`audit-invariants.md` §3.4(d); `src/consts.rs::tol_area`). This is the
/// f32-exactness step of `audit-known-good.md` §2.5, applied to a 4-contact,
/// exactly-symmetric cell layout: even with every cell dimensionally
/// identical, `12*pi*r^2` cannot equal a rational `D_total` for any single
/// rational `r`, because pi is irrational -- so one tip of the four is
/// perturbed by a fraction of an f32 ULP relative to the other three.
fn solve_exact_pair(a_cell: f64, d_total: f64, x_allowed: f64) -> (f32, f32, f64) {
    let pi = std::f64::consts::PI;
    let target = (a_cell / (x_allowed * pi)).sqrt();
    let centre = target as f32;

    // Tolerance floor this must clear: audit-invariants.md section 3.4(d) /
    // src/consts.rs::tol_area(a_owned, x_allowed) = max(SUM_K*eps64*A/x, 1e-9),
    // which for a_owned=16.0, x_allowed=12.0 is dominated by the 1e-9 mm^2
    // floor. Once a candidate clears it, prefer the one closest to the
    // symmetric ideal (both tips nearest `target`) rather than the single
    // global residual minimum -- a solution 2% off-centre satisfies I4(d)
    // exactly as well as one in the 7th decimal, and the latter keeps the
    // "exactly equal by symmetry" property the design relies on for
    // hand-checkability (audit-known-good.md section 1.4).
    const TOL: f64 = 1.0e-9;
    const WINDOW: i64 = 60_000;
    const NWINDOW: i64 = 400;
    let mut best: Option<(f32, f32, f64, i64)> = None; // (ra, rn, residual, |step|)

    for step in -WINDOW..=WINDOW {
        let ra = f32_step(centre, step);
        if !(ra > 0.0 && ra.is_finite()) || !is_1_5x_exact_f32(ra) {
            continue;
        }
        let ra64 = f64::from(ra);
        let remaining = d_total / (x_allowed * pi) - 3.0 * ra64 * ra64;
        if remaining <= 0.0 {
            continue;
        }
        let rn_ideal = remaining.sqrt() as f32;
        for nstep in -NWINDOW..=NWINDOW {
            let rn = f32_step(rn_ideal, nstep);
            if !(rn > 0.0 && rn.is_finite()) || !is_1_5x_exact_f32(rn) {
                continue;
            }
            let rn64 = f64::from(rn);
            let sum = x_allowed * pi * (3.0 * ra64 * ra64 + rn64 * rn64);
            let residual = (sum - d_total).abs();
            if residual >= TOL {
                continue;
            }
            let ordering_key = step.abs();
            let better = match best {
                None => true,
                Some((_, _, _, bkey)) => ordering_key < bkey,
            };
            if better {
                best = Some((ra, rn, residual, ordering_key));
            }
        }
    }
    if let Some((ra, rn, residual, key)) = best {
        eprintln!("  (closest-to-symmetric candidate under the {TOL:e} mm^2 floor: |step|={key})");
        return (ra, rn, residual);
    }
    eprintln!("  no candidate under the {TOL:e} mm^2 floor -- falling back to global minimum");
    let mut fallback: Option<(f32, f32, f64)> = None;
    for step in -WINDOW..=WINDOW {
        let ra = f32_step(centre, step);
        if !(ra > 0.0 && ra.is_finite()) || !is_1_5x_exact_f32(ra) {
            continue;
        }
        let ra64 = f64::from(ra);
        let remaining = d_total / (x_allowed * pi) - 3.0 * ra64 * ra64;
        if remaining <= 0.0 {
            continue;
        }
        let rn_ideal = remaining.sqrt() as f32;
        for nstep in -NWINDOW..=NWINDOW {
            let rn = f32_step(rn_ideal, nstep);
            if !(rn > 0.0 && rn.is_finite()) || !is_1_5x_exact_f32(rn) {
                continue;
            }
            let rn64 = f64::from(rn);
            let sum = x_allowed * pi * (3.0 * ra64 * ra64 + rn64 * rn64);
            let residual = (sum - d_total).abs();
            if fallback.is_none_or(|(_, _, br)| residual < br) {
                fallback = Some((ra, rn, residual));
            }
        }
    }
    fallback.expect("no (r_a, r_n) pair satisfying R6 found in the ULP-sweep window")
}

fn main() {
    let x_allowed = c::X_ALLOWED; // 12.0, physics.md section 2
    let f_ball = c::BALL_FACTOR_STUB; // 1.5
    let beta = 0.60_f64; // audit-known-good.md section 1.3/2.1 step 4, KG-1's factor

    // Geometry: 4.00 x 4.00 x 2.00mm slab, bbox-centred, four posts at
    // (+-1.00, +-1.00) -- the perpendicular-bisector centres of the square's
    // four 2x2mm quadrants, so the Voronoi partition of the underside is
    // exactly those four quadrants (audit-known-good.md section 3.1).
    let a_cell = 4.0_f64; // one 2x2mm quadrant, exact
    let d_total = 16.0_f64; // the whole 4x4mm footprint, exact (both walls and
                             // the top face carry zero down-facing demand)

    let (r_a, r_n, residual) = solve_exact_pair(a_cell, d_total, x_allowed);
    eprintln!(
        "I4(d) sweep: r_a={r_a:.10} r_n={r_n:.10} residual={residual:.6e} mm^2 (floor 1e-9 mm^2)"
    );

    // Frame. The audit reconstructs z_plate as `mesh.bbox.min[2] -
    // translate_mm[2]` (src/context.rs::resolve_plate, and already used
    // correctly by fixtures/double_load/generate.rs), NOT the simplified
    // `z_plate = -translate_z` that audit-invariants.md section 1.2 states --
    // that simplification is only exact when the mesh's own bbox-min z is 0.
    // This generator's mesh is bounding-box-CENTRED (bbox_min_z = -1.0), so
    // translate_z must be chosen to land z_plate at the intended -5.0: with
    // z_face == bbox_min_z for this flat-bottomed slab, translate_z equals
    // the physical float height (4.00mm, section 3's own header) directly.
    let z_face = -1.0_f64; // slab underside == mesh bbox-min z, bbox-centred frame
    let z_plate = -5.0_f64; // target: 4.00mm float height below z_face
    let mesh_bbox_min_z = -1.0_f64;
    let translate_z = mesh_bbox_min_z - z_plate; // = 4.0
    assert!(
        (mesh_bbox_min_z - translate_z - z_plate).abs() < 1e-12,
        "z_plate = mesh.bbox.min.z - translate_z"
    );

    let posts = [(-1.0_f64, -1.0_f64), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)];
    // Three posts at r_a; the fourth (arbitrarily the last) carries the tiny
    // f32-ULP perturbation r_n that makes I4(d) close. The difference is in
    // the 7th decimal -- physically identical, invisible in ChituBox, and
    // disclosed in the README per audit-known-good.md section 2.5.
    let r_tips = [r_a, r_a, r_a, r_n];

    let mesh = box_soup([-2.0, -2.0, -1.0], [2.0, 2.0, 1.0]);

    let mut records = Vec::new();
    eprintln!("contacts:");
    for (i, (&(x0, y0), &r_tip_f32)) in posts.iter().zip(r_tips.iter()).enumerate() {
        let r_tip = f64::from(r_tip_f32);
        let big_r = f_ball * r_tip; // ball radius R, and (by construction) r_arm too
        let h = beta * r_tip;

        // TIP (wire 7): start p0 = the ball's deepest/topmost point, inside
        // the model; end = the far pole. u = (0,0,-1).
        let p0 = [x0, y0, z_face + h];
        let tip_end = [x0, y0, p0[2] - 2.0 * big_r];
        let ball_centre = [x0, y0, (p0[2] + tip_end[2]) * 0.5];
        let tip = SynthRecord::new(RecordKind::ContactSphere, p0, tip_end, big_r, big_r)
            .with_soup(drop_zero_area(sphere_soup(ball_centre, big_r, 8, 16)));

        let h_delivered = delivered_depth(z_face, p0[2]);
        assert!(h < big_r, "R3: ball centre must stay outside the model (h < R)");

        // TAPER (wire 1): start = the ball's own connective point (its
        // CENTRE, per docs/specs/chitubox-format.md section 5), narrow end
        // r_tip; end = a node 0.50mm below, widening to r_arm (= big_r).
        let centre = [x0, y0, p0[2] - big_r];
        let node = [x0, y0, centre[2] - 0.50];
        let taper = SynthRecord::new(RecordKind::ContactTaper, centre, node, r_tip, big_r)
            .with_declared_depth(h_delivered)
            .with_matching_soup();

        // MEMBER (wire 3): starts 0.10mm above the node (R1: >=0.10mm axial
        // overlap into the taper), constant radius big_r, down to
        // z_plate+0.90.
        let member_start = [x0, y0, node[2] + 0.10];
        let member_end = [x0, y0, z_plate + 0.90];
        let member = SynthRecord::new(RecordKind::Member, member_start, member_end, big_r, big_r)
            .with_matching_soup();

        // FLARE (wire 4): starts at z_plate+1.00 (R1: 0.10mm overlap into the
        // member, which ends at z_plate+0.90), flares from big_r down to
        // 0.90mm at the plate itself. No raft: audit-known-good.md section
        // 2.2 -- a flare landing on the plate is grounded by
        // audit-invariants.md section 3.5(i) on its own.
        let flare_start = [x0, y0, z_plate + 1.00];
        let flare_end = [x0, y0, z_plate];
        let flare = SynthRecord::new(RecordKind::RootFlare, flare_start, flare_end, big_r, 0.90)
            .with_matching_soup();

        eprintln!(
            "  post {i} ({x0:+.2},{y0:+.2}): r_tip={r_tip:.9} R=r_arm={big_r:.9} h={h:.9} h_delivered={h_delivered:.9}"
        );

        records.push(tip);
        records.push(taper);
        records.push(member);
        records.push(flare);
    }

    // STLs for owner inspection independent of ChituBox (audit-known-good.md
    // section 7.4): the model mesh and the concatenated support soup, in the
    // SAME bounding-box-centred local frame the .chitubox stores (no
    // translate applied), written before `records` is consumed below.
    let support_soup: Vec<[[f32; 3]; 3]> = records.iter().flat_map(|r| r.soup.clone()).collect();

    let file = SynthFile::new().with_model(SynthModel {
        name: "spread_wide".into(),
        translate_mm: [0.0, 0.0, translate_z],
        mesh: mesh.clone(),
        records,
    });

    let out = std::env::args()
        .nth(1)
        .expect("usage: gen_spread_wide <out.chitubox>");
    let mut bytes = file.to_bytes();
    splice_for_chitubox_openability(&mut bytes);
    std::fs::write(&out, &bytes).expect("write");
    eprintln!("wrote {out} ({} bytes)", bytes.len());

    let stem = out.trim_end_matches(".chitubox");
    let model_stl = format!("{stem}-model.stl");
    let supports_stl = format!("{stem}-supports.stl");
    write_stl_ascii(&model_stl, "spread_wide_model", &mesh);
    write_stl_ascii(&supports_stl, "spread_wide_supports", &support_soup);
    eprintln!("wrote {model_stl} ({} tris), {supports_stl} ({} tris)", mesh.len(), support_soup.len());
}

/// A minimal ASCII STL writer, so the owner can inspect the model and support
/// geometry in any STL viewer, not only ChituBox (audit-known-good.md section
/// 7.4). Coordinates are written exactly as stored (f32, local frame).
fn write_stl_ascii(path: &str, name: &str, tris: &[[[f32; 3]; 3]]) {
    use std::fmt::Write as _;
    let mut s = String::new();
    let _ = writeln!(s, "solid {name}");
    for t in tris {
        let e1 = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
        let e2 = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
        let n = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let n = if len > 0.0 {
            [n[0] / len, n[1] / len, n[2] / len]
        } else {
            [0.0, 0.0, 0.0]
        };
        let _ = writeln!(s, "  facet normal {:e} {:e} {:e}", n[0], n[1], n[2]);
        let _ = writeln!(s, "    outer loop");
        for v in t {
            let _ = writeln!(s, "      vertex {:e} {:e} {:e}", v[0], v[1], v[2]);
        }
        let _ = writeln!(s, "    endloop");
        let _ = writeln!(s, "  endfacet");
    }
    let _ = writeln!(s, "endsolid {name}");
    std::fs::write(path, s).expect("write stl");
}

/// Splice a 396-byte all-zero settings blob after the 24-byte header and
/// shift every downstream absolute file offset by 396 to match, so the file
/// carries the `settings_size=420` a real emitted project always carries
/// (`docs/specs/chitubox-format.md` section 1/6; `audit-known-good.md`
/// section 7.2). `SynthFile::to_bytes()` writes `settings_size=0, off8=24`
/// (no settings blob, no preview) which the known-good file, unlike the
/// other eleven deliberately-broken fixtures, must not ship: the owner
/// inspects this file in ChituBox, so a file it cannot open is not a
/// known-good file.
///
/// This extends `audit-known-good.md` section 7.2's own list of fields to
/// shift (`off8`, `off20`, each model's `mesh_off` and
/// `support_index_ptr`, each support index's own `mesh_off`) by two more
/// that the document does not name but that the reader's own parsing
/// requires to stay correct: the support index's `records_off` (validated
/// against `index_off + 16 + 400` by `reader.rs`'s own invariant, so leaving
/// it unshifted would plant a `records_off` mismatch finding of the audit's
/// own making) and each individual support record's own `mesh_off` (each
/// record's soup triangles are themselves data that physically moved).
fn splice_for_chitubox_openability(bytes: &mut Vec<u8>) {
    const SHIFT: u32 = 396;
    let rd_u32 = |b: &[u8], o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let wr_u32 = |b: &mut [u8], o: usize, v: u32| b[o..o + 4].copy_from_slice(&v.to_le_bytes());

    let model_count = rd_u32(bytes, 4);
    let old_off8 = rd_u32(bytes, 8);
    let old_off20 = rd_u32(bytes, 20);

    let mut spliced = Vec::with_capacity(bytes.len() + SHIFT as usize);
    spliced.extend_from_slice(&bytes[0..24]);
    spliced.resize(spliced.len() + SHIFT as usize, 0u8);
    spliced.extend_from_slice(&bytes[24..]);

    wr_u32(&mut spliced, 12, 420); // settings_size: 0 -> 420
    wr_u32(&mut spliced, 8, old_off8 + SHIFT); // off8
    wr_u32(&mut spliced, 20, old_off20 + SHIFT); // off20

    let table = (old_off8 + SHIFT) as usize; // the model table's new start
    for mi in 0..model_count as usize {
        let moff = table + mi * 680;

        let old_mesh_off = rd_u32(&spliced, moff + 272); // M_MESH_OFF
        wr_u32(&mut spliced, moff + 272, old_mesh_off + SHIFT);

        let old_sip = rd_u32(&spliced, moff + 268); // M_SUPPORT_INDEX_PTR
        if old_sip == 0 {
            continue;
        }
        let new_sip = old_sip + SHIFT;
        wr_u32(&mut spliced, moff + 268, new_sip);

        let sio = new_sip as usize;
        let n_records = rd_u32(&spliced, sio); // SI_N_RECORDS
        let old_records_off = rd_u32(&spliced, sio + 4); // SI_RECORDS_OFF
        let old_si_mesh_off = rd_u32(&spliced, sio + 8); // SI_MESH_OFF
        wr_u32(&mut spliced, sio + 4, old_records_off + SHIFT);
        wr_u32(&mut spliced, sio + 8, old_si_mesh_off + SHIFT);

        let new_records_off = (old_records_off + SHIFT) as usize;
        for ri in 0..n_records as usize {
            let ro = new_records_off + ri * 72;
            let old_s_mesh_off = rd_u32(&spliced, ro + 40); // S_MESH_OFF
            if old_s_mesh_off != 0 {
                wr_u32(&mut spliced, ro + 40, old_s_mesh_off + SHIFT);
            }
        }
    }

    *bytes = spliced;
}
