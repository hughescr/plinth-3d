// Standalone generator for `trivial_cube.chitubox` -- NOT part of the
// `plinth-audit` crate, NOT built by its `Cargo.toml`. Kept alongside the
// fixture as the exact, re-runnable record of how the bytes were produced
// (same convention as `collinear_arms/generate.rs`, `double_load/generate.rs`:
// compiled ad hoc against the already-built `plinth_audit` rlib).
//
// This is KG-1 `kg_slab_four_post` from `docs/specs/audit-known-good.md`
// section 3.1, under this round's directory name `trivial_cube`: the FIRST
// known-good fixture in the tool -- a file the ten checks are asked to PASS,
// not fail. See that document for the full derivation; this generator
// reproduces the arithmetic in code and, where a real value must be READ BACK
// from its own f32 storage (rule R7), computes it that way rather than
// trusting the design target.
//
// Geometry: a 4.00 x 4.00 x 2.00 mm slab, bounding-box-centred, floating
// 4.00 mm above the plate on four identical posts at local (x,y) =
// (+-1.00, +-1.00). Each post's chain is TIP -> TAPER -> MEMBER -> FLARE,
// landing on the plate with no raft pad (a flare's bottom disc grounds on its
// own, `audit-invariants.md` section 3.5 clause (i)).
//
// Deliberately NOT consulted while writing this file: src/checks/*.rs,
// src/seat.rs, src/ownership.rs, src/raster.rs, src/graph.rs. Every number
// below is derived from docs/physics.md's area law and scar invariant, and
// from the printer/tolerance constants in src/consts.rs (which restate
// audit-invariants.md's own declared inputs, not a check's logic).
//
// --- The I4(d) sizing search, and a false start corrected before shipping --
//
// `audit-known-good.md` section 2.5 proposes giving one of the four tips a
// slightly different radius from the other three, chosen by an f32-ULP
// sweep, so the sum `sum(12*pi*r_i^2)` lands within I4 clause (d)'s 1e-9 mm^2
// floor of `D_total`. A first implementation of that sweep here (window
// +-100,000 to +-200,000 ULPs, picking whichever candidate pair minimised the
// RAW RESIDUAL globally) DID zero the residual (down to ~7e-11 mm^2) but only
// by landing on a candidate far from the symmetric ideal -- one tip's own
// `X_delivered` overshot 12 by up to 12% (r_b = 0.3075196147 mm against an
// ideal 0.3257350079 mm, X_b = 13.46), which reads as undersizing a tip to
// chase a bookkeeping tolerance, precisely what section 1.3 forbids.
//
// That was a SEARCH-STRATEGY DEFECT, not a genuine impossibility -- caught
// by cross-checking against `fixtures/spread_wide/generate.rs`, an
// independently-built copy of this same KG-1 design by a different agent in
// this round, which measurably passes I4 (books ratio 1.0000000000166) with
// its own tips at X = 9.86 (three tips) and X = 9.848 (the fourth) --
// negligibly apart, not 12 vs 13.46. The fix is the SELECTION rule, not the
// search: among every (r_a, r_n) pair whose residual already clears the
// 1e-9 mm^2 floor, prefer the one CLOSEST to the exact-symmetric ideal
// (smallest ULP step), not the one with the globally smallest residual --
// many candidates clear the floor once R6-exactness is required of both
// radii, and the nearest one costs a few ULPs (a relative deviation near
// 1e-4), not tens of thousands. Re-implemented that way below; the achieved
// deviation is reported as a measured number in the README, not asserted.
//
// --- Two BUILD DEFECTS found and fixed, and why they are build defects and
// --- not tool findings (distinguished per audit-known-good.md section 8
// --- step 2: "re-derive by hand ... the most likely explanation is a build
// --- defect ... before suspecting the check") -------------------------------
//
// 1. z_plate. `audit-known-good.md` section 3.1's own prose gives
//    `translate_z = 5.0` for "4.00 mm above the plate". That follows
//    `audit-invariants.md` section 1.2's SIMPLIFIED relation
//    `z_plate = -translate_z`. The actual reconstruction
//    (`src/context.rs::resolve_plate`, confirmed independently by
//    `fixtures/buried_arm/generate.rs`'s own comment on the identical
//    formula) is `z_plate = mesh.bbox.min.z - translate_z`, which only
//    coincides with the simplified form when the mesh's bbox bottom sits at
//    local z=0 -- true for NONE of these bounding-box-centred meshes. With
//    `translate_z=5.0` this file measurably put the plate at z=-6.0 (one
//    whole millimetre below the intended -5.0), stranding every flare foot
//    1.00 mm above the real plate -- I5 correctly fired `PATH_FREE_END` on
//    all four. Fixed by deriving `translate_z = z_face - z_plate = 4.0` from
//    the two frame values directly, rather than trusting the document's
//    simplified formula.
//
// 2. The joint between the TAPER and the MEMBER. Section 2.2's table
//    specifies pushing the member's start point 0.10 mm axially "into" the
//    taper (an explicit, deliberate overlap, per rule R1). Built literally,
//    that push (0.10 mm out of the taper's own 0.50 mm length) lands the
//    member's start point INSIDE the ball's own volume -- not just
//    overlapping the taper, but creating a three-way overlap among ball,
//    taper and member at once. I5 tolerated it (a real, if excessive,
//    overlap), but I3's and I8's own path walk could not: `MIN_SECTION_INDETERMINATE
//    reason=path_open` on every contact and `GEOM_INDETERMINATE
//    reason=path_indeterminate` on 12 of 16 records. Measurably NOT a
//    tolerance-boundary question -- reverting to an EXACT-coincidence join
//    (member.start = taper's node point exactly, radius-matched, the same
//    pattern `fixtures/collinear_arms/generate.rs` already uses successfully)
//    resolved I3 and I8 outright (worst A_min measured 0.4815 mm^2 against
//    a required 0.3333 mm^2, matching section 3.1's predicted 0.480 mm^2 to
//    three figures) but then exposed a SECOND, narrower defect: at the
//    taper's originally-specified 0.50 mm length, the ball and the member --
//    not adjacent in the declared chain, and not overlapping -- come within
//    0.026666 mm of each other (`PATH_INDETERMINATE reason=fusion_uncertain`,
//    `separation_mm=0.026666`, band `5.529014e-2` mm), because this file's
//    `r_ball` (0.4886 mm) is close enough to the 0.50 mm taper length that
//    the ball's own bulge nearly reaches the member's start disc around the
//    taper's narrow waist. Lengthening the taper from 0.50 mm to 0.60 mm
//    (member.start still exactly coincident with the taper's new node, radii
//    and every other number unchanged) clears the ball-to-member separation
//    past the fusion-uncertain band and the file now measures I1 through I3
//    and I5 through I10 all PASS. This is a genuine geometric ambiguity this
//    file's own specific radii created, not a tolerance dodge: no area,
//    radius, or depth number changed, only how far the (already-specified)
//    taper travels before the arm takes over.

use plinth_audit::consts as c;
use plinth_audit::reader::RecordKind;
use plinth_audit::synth::{box_soup, sphere_soup, SynthFile, SynthModel, SynthRecord};

/// Same zero-area-triangle workaround as `collinear_arms` / `double_load` /
/// `starved_patch`: `synth::sphere_soup`'s pole ring emits triangles with two
/// bit-identical vertices (both pole-adjacent verts collapse to the same
/// point), which `i8_no_dead_geometry` correctly flags
/// `GEOM_DEGENERATE reason=zero_area_triangle`. Pre-existing defect in the
/// shared test helper, not owned by this fixture; dropped here so it does not
/// pollute this fixture's own i8/i6 reading.
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

/// R6: `R = 1.5 * r` must be exact in f32 -- the f64 product of the f32 value
/// by 1.5 must itself be exactly representable as f32, so I1's three ways of
/// reconstructing the ball radius (`R/f`, the taper's own `r_start`, the
/// fitted soup) agree bit-for-bit rather than merely nearly.
fn r6_exact(r: f32) -> bool {
    let prod = f64::from(r) * 1.5;
    (prod as f32) as f64 == prod
}

fn f32_step(x: f32, steps: i64) -> f32 {
    let bits = i64::from(x.to_bits());
    f32::from_bits((bits + steps) as u32)
}

/// Find `(r_a, r_n, residual)`: three tips at `r_a`, one at `r_n`, both
/// R6-exact, minimising physical deviation from the exact-symmetric ideal
/// subject to the I4(d) residual `|12*pi*(3*r_a^2 + r_n^2) - d_total|`
/// clearing the `1e-9 mm^2` floor (`audit-invariants.md` section 3.4(d)).
/// `pi` is irrational, so no single rational `r` can hit `D_total` exactly
/// (`audit-known-good.md` section 2.5) -- but among the many (r_a, r_n)
/// pairs that clear the floor, the one closest to centre costs only a few
/// ULPs, not the tens of thousands a global-residual-minimum search can
/// wander to (see the file-header note above).
fn solve_i4_pair(a_cell: f64, d_total: f64, x_allowed: f64) -> (f32, f32, f64) {
    let pi = std::f64::consts::PI;
    let ideal = (a_cell / (x_allowed * pi)).sqrt();
    let centre = ideal as f32;
    const TOL: f64 = 1.0e-9;
    const WINDOW: i64 = 60_000;
    const NWINDOW: i64 = 400;

    let mut best: Option<(f32, f32, f64, i64)> = None; // (ra, rn, residual, |step|)
    for step in -WINDOW..=WINDOW {
        let ra = f32_step(centre, step);
        if !(ra > 0.0 && ra.is_finite()) || !r6_exact(ra) {
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
            if !(rn > 0.0 && rn.is_finite()) || !r6_exact(rn) {
                continue;
            }
            let rn64 = f64::from(rn);
            let sum = x_allowed * pi * (3.0 * ra64 * ra64 + rn64 * rn64);
            let residual = (sum - d_total).abs();
            if residual >= TOL {
                continue;
            }
            let key = step.abs();
            let better = match best {
                None => true,
                Some((_, _, _, bkey)) => key < bkey,
            };
            if better {
                best = Some((ra, rn, residual, key));
            }
        }
    }
    let (ra, rn, residual, key) =
        best.expect("no (r_a, r_n) pair under the I4(d) floor found in the sweep window");
    eprintln!("  I4(d) sweep: closest-to-symmetric candidate at |step|={key} ULPs, residual={residual:.3e} mm^2");
    (ra, rn, residual)
}

struct Chain {
    tip: SynthRecord,
    taper: SynthRecord,
    member: SynthRecord,
    flare: SynthRecord,
    delivered_h: f64,
}

#[allow(clippy::too_many_arguments)]
fn build_chain(x0: f64, y0: f64, z_face: f64, z_plate: f64, r_tip: f64, h_ideal: f64) -> Chain {
    let r_ball = 1.5 * r_tip; // R = f * r_tip, f = BALL_FACTOR_STUB
    let r_arm = 1.5 * r_tip; // step 5 of the sizing procedure: r_arm = 1.5 * r_tip = R

    // p0: the ball's deepest pole, INSIDE the model, h above the down-face.
    // Stored as f32 (all coordinates are), so the DELIVERED depth (rule R7)
    // is read back from the two stored f32s, not from `h_ideal`.
    let p0_z_f32 = (z_face + h_ideal) as f32;
    let z_face_f32 = z_face as f32;
    let delivered_h = f64::from(p0_z_f32) - f64::from(z_face_f32);
    let p0 = [x0, y0, f64::from(p0_z_f32)];

    let tip_end = [x0, y0, p0[2] - 2.0 * r_ball];
    let ball_centre = [x0, y0, p0[2] - r_ball];

    let tip = SynthRecord::new(RecordKind::ContactSphere, p0, tip_end, r_ball, r_ball)
        .with_soup(drop_zero_area(sphere_soup(ball_centre, r_ball, 8, 16)));

    // TAPER: ball centre (connective point, per graph.rs) -> node 0.60 mm
    // below (0.50 mm as section 2.2's table literally states, PLUS 0.10 mm --
    // see the file-header note "2." on why: at 0.50 mm this file's own
    // r_ball comes within the fusion-uncertain band of the member's start
    // disc, a ball-to-member ambiguity unrelated to the taper-member joint
    // itself). Narrow end (at the ball) = r_tip; wide end (at the node) =
    // r_arm. Trailing slot 0 carries the DELIVERED depth (R7), not h_ideal.
    let node = [x0, y0, ball_centre[2] - 0.60];
    let taper = SynthRecord::new(RecordKind::ContactTaper, ball_centre, node, r_tip, r_arm)
        .with_declared_depth(delivered_h)
        .with_matching_soup();

    // MEMBER: starts EXACTLY at the taper's node (radius-matched, zero
    // separation -- the same join pattern `collinear_arms/generate.rs`
    // already uses successfully), not the 0.10 mm "push into the taper"
    // section 2.2 describes -- see the file-header note "2." on why that
    // literal push instead created a three-way ball/taper/member overlap
    // that I3 and I8 could not walk. Constant radius r_arm to z_plate + 0.90.
    let member_start = node;
    let member_end = [x0, y0, z_plate + 0.90];
    let member = SynthRecord::new(RecordKind::Member, member_start, member_end, r_arm, r_arm)
        .with_matching_soup();

    // FLARE: starts exactly at the member's end (same exact-coincidence join
    // convention), flares from r_arm out to a 0.90 mm foot, lands exactly on
    // the plate. No raft pad -- a flare on the plate is its own ground
    // (audit-invariants.md section 3.5 clause (i); confirmed measured, once
    // z_plate was corrected -- see note "1.").
    let flare_start = member_end;
    let flare_end = [x0, y0, z_plate];
    let foot_r = 0.90_f64;
    let flare = SynthRecord::new(RecordKind::RootFlare, flare_start, flare_end, r_arm, foot_r)
        .with_matching_soup();

    Chain {
        tip,
        taper,
        member,
        flare,
        delivered_h,
    }
}

/// Splice the container into ChituBox-openable shape
/// (`docs/specs/audit-known-good.md` section 7.2): insert a 396-byte settings
/// blob (copied verbatim from a real corpus file, `corpus/baseline/Sphere.chitubox`,
/// whose own off8 = 420 shows it carries ZERO preview bytes -- settings only,
/// no thumbnail region) right after the 24-byte header, flip header field 12
/// (settings_size) from 0 to 420, and shift every ABSOLUTE offset field that
/// points at or past byte 24 by +396: H_OFF8, H_OFF20, each model's
/// M_MESH_OFF and (if nonzero) M_SUPPORT_INDEX_PTR, each support index's
/// SI_RECORDS_OFF and SI_MESH_OFF, and each segment record's S_MESH_OFF
/// (skipped when it is 0, meaning "no mesh" -- shifting a sentinel would turn
/// it into a bogus pointer).
fn make_chitubox_openable(bytes: &[u8], settings_blob: &[u8; 396]) -> Vec<u8> {
    fn rd_u32(b: &[u8], o: usize) -> u32 {
        u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    }
    fn wr_u32(b: &mut [u8], o: usize, v: u32) {
        b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }

    let mut out = Vec::with_capacity(bytes.len() + 396);
    out.extend_from_slice(&bytes[0..24]);
    out.extend_from_slice(settings_blob);
    out.extend_from_slice(&bytes[24..]);

    const SHIFT: u32 = 396;
    let shift_if_past_header = |v: u32| -> u32 {
        if v == 0 {
            0
        } else {
            v + SHIFT
        }
    };

    let new_off8 = rd_u32(&out, c::H_OFF8) + SHIFT;
    wr_u32(&mut out, c::H_OFF8, new_off8);
    wr_u32(&mut out, c::H_SETTINGS_SIZE, c::SETTINGS_SIZE_POPULATED);
    let new_off20 = rd_u32(&out, c::H_OFF20) + SHIFT;
    wr_u32(&mut out, c::H_OFF20, new_off20);

    let model_count = rd_u32(&out, c::H_MODEL_COUNT) as usize;
    let table_off = rd_u32(&out, c::H_OFF8) as usize; // already shifted
    for mi in 0..model_count {
        let moff = table_off + mi * c::MODEL_RECORD_LEN;
        let mesh_off = rd_u32(&out, moff + c::M_MESH_OFF);
        wr_u32(&mut out, moff + c::M_MESH_OFF, shift_if_past_header(mesh_off));

        let sip = rd_u32(&out, moff + c::M_SUPPORT_INDEX_PTR);
        if sip == 0 {
            continue;
        }
        let new_sip = sip + SHIFT;
        wr_u32(&mut out, moff + c::M_SUPPORT_INDEX_PTR, new_sip);

        let sio = new_sip as usize;
        let n_records = rd_u32(&out, sio + c::SI_N_RECORDS);
        let records_off = rd_u32(&out, sio + c::SI_RECORDS_OFF);
        let si_mesh_off = rd_u32(&out, sio + c::SI_MESH_OFF);
        wr_u32(&mut out, sio + c::SI_RECORDS_OFF, records_off + SHIFT);
        wr_u32(&mut out, sio + c::SI_MESH_OFF, shift_if_past_header(si_mesh_off));

        let new_records_off = (records_off + SHIFT) as usize;
        for ri in 0..n_records as usize {
            let ro = new_records_off + ri * c::SEG_RECORD_LEN;
            let smo = rd_u32(&out, ro + c::S_MESH_OFF);
            wr_u32(&mut out, ro + c::S_MESH_OFF, shift_if_past_header(smo));
        }
    }
    out
}

/// Minimal binary STL writer: 80-byte header, u32 triangle count, then
/// 50 bytes/triangle (12-byte zero normal + 3x12-byte f32 vertices + 2-byte
/// attribute). No dependency; same convention as
/// `fixtures/four_leg_overhang/generate.rs`'s own copy. Ships the model and
/// support soups so the owner's ChituBox inspection (`audit-known-good.md`
/// section 7.2) does not depend on one program.
fn write_stl(path: &str, tris: &[[[f32; 3]; 3]]) {
    let mut out = vec![0u8; 80];
    out.extend_from_slice(&(tris.len() as u32).to_le_bytes());
    for t in tris {
        out.extend_from_slice(&0f32.to_le_bytes());
        out.extend_from_slice(&0f32.to_le_bytes());
        out.extend_from_slice(&0f32.to_le_bytes());
        for v in t {
            for c in v {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
        out.extend_from_slice(&0u16.to_le_bytes());
    }
    std::fs::write(path, out).expect("write stl");
}

fn main() {
    let pi = std::f64::consts::PI;
    let twelve_pi = 12.0 * pi;

    // --- Frame -------------------------------------------------------------
    // The audit reconstructs z_plate as `mesh.bbox.min.z - translate_z`
    // (`src/context.rs::resolve_plate`, ContractRelation branch; independently
    // confirmed by `fixtures/buried_arm/generate.rs`'s own comment on the same
    // formula) -- NOT the simplified `z_plate = -translate_z` that
    // `audit-invariants.md` section 1.2's prose states, which only coincides
    // with the real rule when the mesh's bbox happens to sit with its bottom
    // at local z=0. A first draft of this generator followed that simplified
    // prose literally (translate_z = 5.0, intending z_plate = -5.0) and it
    // measurably reconstructed z_plate = -6.0, stranding every flare foot
    // 1.00 mm above the real plate -- I5 correctly fired PATH_FREE_END on all
    // four. This is a BUILD DEFECT this generator now corrects, not a tool
    // finding: with the slab's bbox bottom at local z = -1.0 and the design
    // intent of "4.00 mm above the plate" (audit-known-good.md section 3,
    // common to all five KG files), the frame requires
    // translate_z = z_face - z_plate = -1.0 - (-5.0) = 4.0, not the 5.0 that
    // document's own section 3.1 states.
    let z_face = -1.0_f64; // slab bottom, local frame == mesh bbox.min.z
    let z_plate = -5.0_f64; // 4.00 mm below z_face, per design intent
    let translate_z = z_face - z_plate; // = 4.0, NOT -z_plate

    // --- Sizing procedure (audit-known-good.md section 2.1) --------------
    let a_face = 16.0_f64; // 4.00 x 4.00 mm underside
    let n = 4.0_f64;
    let a_cell = a_face / n; // 4.0000 mm^2 exactly, by symmetry
    let a_req = a_cell / c::X_ALLOWED; // 0.333333... mm^2
    let d_total = a_face; // reconstructed down-face area: the box's vertical
                           // sides and up-facing top carry zero demand.

    // r_tip: exact X = 12 sizing, 12*pi*r_tip^2 = a_cell for each of the four
    // equal-by-symmetry Voronoi cells. `pi` is irrational, so no single
    // rational r_tip can make the sum hit D_total exactly (section 2.5) --
    // three posts share the closest-to-ideal R6-exact radius `r_a`, the
    // fourth carries `r_n`, chosen together as the pair closest to the
    // symmetric ideal among every candidate whose summed I4(d) books clear
    // the 1e-9 mm^2 floor (see the file-header note on the search-strategy
    // false start this corrects).
    let r_tip_ideal = (a_cell / twelve_pi).sqrt();
    let (r_a_f32, r_n_f32, residual) = solve_i4_pair(a_cell, d_total, c::X_ALLOWED);
    let r_a = f64::from(r_a_f32);
    let r_n = f64::from(r_n_f32);

    let beta = 0.60_f64; // KG-1's chosen margin, section 1.3

    // Posts 0-2 share r_a; post 3 carries r_n -- disclosed in the README,
    // per section 2.5's required disclosure.
    let posts = [
        (1.0_f64, 1.0_f64, r_a),
        (1.0, -1.0, r_a),
        (-1.0, 1.0, r_a),
        (-1.0, -1.0, r_n),
    ];

    let mesh = box_soup([-2.0, -2.0, -1.0], [2.0, 2.0, 1.0]);
    let mut records = Vec::new();
    let mut delivered_hs = Vec::new();
    let mut support_soup: Vec<[[f32; 3]; 3]> = Vec::new();
    for &(x0, y0, r_tip) in &posts {
        let h_ideal = beta * r_tip;
        let chain = build_chain(x0, y0, z_face, z_plate, r_tip, h_ideal);
        delivered_hs.push(chain.delivered_h);
        support_soup.extend(chain.tip.soup.iter().copied());
        support_soup.extend(chain.taper.soup.iter().copied());
        support_soup.extend(chain.member.soup.iter().copied());
        support_soup.extend(chain.flare.soup.iter().copied());
        records.push(chain.tip);
        records.push(chain.taper);
        records.push(chain.member);
        records.push(chain.flare);
    }

    let file = SynthFile::new().with_model(SynthModel {
        name: "trivial_cube".into(),
        translate_mm: [0.0, 0.0, translate_z],
        mesh: mesh.clone(),
        records,
    });

    let raw = file.to_bytes();

    // Settings blob donor: corpus/baseline/Sphere.chitubox, bytes [24,420).
    // That file's own off8 = 420 (checked: xxd -l24 shows off8 = 0x1a4 = 420
    // = 24 + 396), i.e. ZERO preview bytes -- settings only, so no preview
    // splice is needed here.
    let donor_path = std::env::args()
        .nth(2)
        .expect("usage: gen_trivial_cube <out.chitubox> <settings-donor.chitubox>");
    let donor = std::fs::read(&donor_path).expect("read settings donor");
    assert!(donor.len() >= 420, "donor too short for a settings blob");
    let off8_donor = u32::from_le_bytes([donor[8], donor[9], donor[10], donor[11]]);
    assert_eq!(off8_donor, 420, "donor must carry zero preview bytes (off8 == 420)");
    let mut settings_blob = [0u8; 396];
    settings_blob.copy_from_slice(&donor[24..420]);

    let out_bytes = make_chitubox_openable(&raw, &settings_blob);

    let out = std::env::args()
        .nth(1)
        .expect("usage: gen_trivial_cube <out.chitubox> <settings-donor.chitubox>");
    std::fs::write(&out, &out_bytes).expect("write");

    let stem = out.trim_end_matches(".chitubox");
    write_stl(&format!("{stem}-model.stl"), &mesh);
    write_stl(&format!("{stem}-supports.stl"), &support_soup);
    eprintln!(
        "wrote {stem}-model.stl and {stem}-supports.stl (LOCAL/untranslated frame, same as \
         the .chitubox records)"
    );

    // --- Report (feeds the README verbatim) -------------------------------
    eprintln!("=== trivial_cube (KG-1 kg_slab_four_post) ===");
    eprintln!("a_face = {a_face:.6}  a_cell = {a_cell:.6}  a_req = {a_req:.6}");
    eprintln!("r_tip target (ideal, unrounded) = {r_tip_ideal:.10}");
    for (label, r_tip) in [("r_a (posts 0-2)", r_a), ("r_n (post 3)", r_n)] {
        let r_arm = 1.5 * r_tip;
        let x_delivered = a_cell / (pi * r_tip * r_tip);
        eprintln!(
            "  {label}: r_tip={r_tip:.10}  R=r_arm={r_arm:.10}  X_delivered={x_delivered:.6} \
             (X_ALLOWED=12.0)  arm section={:.6} mm^2 = {:.4}xA_req",
            pi * r_arm * r_arm,
            pi * r_arm * r_arm / a_req
        );
    }
    for (i, h) in delivered_hs.iter().enumerate() {
        eprintln!("  post {i}: delivered_h (from stored f32s) = {h:.10}");
    }
    eprintln!(
        "I4(d) residual: |12*pi*(3*r_a^2+r_n^2) - D_total| = {residual:.6e} mm^2  (floor 1e-9 mm^2)"
    );
    eprintln!("z_face = {z_face}  z_plate = {z_plate}  translate_z = {translate_z}");
    eprintln!("out = {out}  bytes = {}", out_bytes.len());
}
