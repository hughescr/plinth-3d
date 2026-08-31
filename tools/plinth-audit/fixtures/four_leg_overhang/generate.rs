//! Generator for `four_leg_overhang.chitubox` -- the KG-1 known-good fixture
//! (`docs/specs/audit-known-good.md` §3.1: `kg_slab_four_post`, built here
//! under this directory's assigned name `four_leg_overhang`). See that
//! document for the full design derivation and `README.md` in this directory
//! for the arithmetic re-derived for this exact build, the measured audit
//! results, and the two findings this build produced.
//!
//! Standalone binary -- NOT part of the `plinth-audit` crate, NOT built by
//! its `Cargo.toml` (same convention as `fixtures/starved_patch/generate.rs`
//! and `fixtures/double_load/generate.rs`). Built and run as a throwaway
//! crate outside this repo, with `plinth-audit` pulled in as a `path`
//! dependency:
//!
//! ```text
//! cargo new --bin /tmp/kg1gen && cd /tmp/kg1gen
//! # Cargo.toml: [dependencies] plinth-audit = { path = "<repo>/tools/plinth-audit" }
//! cp <repo>/tools/plinth-audit/fixtures/four_leg_overhang/generate.rs src/main.rs
//! RUSTC_WRAPPER="" cargo build --release
//! ./target/release/kg1gen four_leg_overhang.chitubox            # the shipped fixture
//! ./target/release/kg1gen probe_tight_joint.chitubox 0.0005     # the diagnostic probe
//! ```
//!
//! The second, optional CLI argument overrides the design-joint axial overlap
//! (R1, default 0.10 mm) -- built ONLY to produce the diagnostic probe that
//! confirms the I3/I8 finding in `README.md`; the shipped fixture always uses
//! the default.

use plinth_audit::consts as c;
use plinth_audit::reader::RecordKind;
use plinth_audit::synth::{box_soup, sphere_soup, SynthFile, SynthModel, SynthRecord};
use std::f64::consts::PI;

const BETA: f64 = 0.60; // KG-1 uses beta = 0.60 (KG-2 in the design doc uses 0.54)
// Flare foot radius landing on the plate. NOT audited by any of the ten
// checks (grep confirms FINAL_CLEARANCE_GAP_MM appears nowhere under src/ --
// R5/physics.md §5's 0.300mm field-clearance rule is unimplemented in this
// crate). Caught and fixed by hand before shipping, not by a check: the
// design doc's own R5 arithmetic (0.90mm) leaves adjacent flare feet only
// 2.00 - 2*0.90 = 0.20mm apart at the plate (post spacing 2.00mm), UNDER the
// 0.300mm rule. 0.65mm leaves 2.00 - 2*0.65 = 0.70mm, 2.33x the rule, while
// still flaring out from r_arm=0.4886mm.
const FOOT_R_MM: f64 = 0.65;

/// R1's design-joint axial overlap. 0.10 mm is the design-doc default; pass a
/// second CLI arg to override it -- used ONLY to build a diagnostic probe
/// confirming the I3/I8 SupportGraph-coincidence finding (README "Findings"),
/// never to quietly retune the shipped fixture.
fn overlap_mm() -> f64 {
    std::env::args()
        .nth(2)
        .map_or(0.10, |s| s.parse().expect("overlap arg must be a float"))
}

/// Step an f32 value by `n` ULPs (n may be negative). Copied verbatim from the
/// `starved_patch` generator convention.
fn step_f32(x: f32, n: i64) -> f32 {
    let bits = x.to_bits() as i64;
    f32::from_bits((bits + n) as u32)
}

/// True iff `1.5 * f64::from(r)` round-trips through f32 with zero loss --
/// the R6 exactness property the known-good spec requires of every tip.
fn r6_exact(r: f32) -> bool {
    let r64 = f64::from(r);
    let big = 1.5_f64 * r64;
    (big as f32) as f64 == big
}

/// Nearest f32 to `target` satisfying [`r6_exact`], searched within `window` ULPs.
fn nearest_r6(target: f64, window: i64) -> f32 {
    let base = target as f32;
    let mut best: Option<(f64, f32)> = None;
    for n in -window..=window {
        let cand = step_f32(base, n);
        if cand <= 0.0 || !r6_exact(cand) {
            continue;
        }
        let d = (f64::from(cand) - target).abs();
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, cand));
        }
    }
    best.expect("an R6-exact f32 near target").1
}

/// The `audit-invariants.md` I4 clause (d) tolerance for one summed input.
fn tol4(a_owned: f64) -> f64 {
    (4096.0 * c::EPS64 * a_owned.abs() / c::X_ALLOWED).max(1e-9)
}

fn main() {
    let d_total = 16.0_f64; // exact: 4.00 x 4.00 mm footprint, both f32-exact
    let a_cell = 4.0_f64; // exact: 2.00 x 2.00 mm Voronoi quadrant, 4 identical cells
    let x_allowed = c::X_ALLOWED; // 12.0
    let ideal_r_tip = (a_cell / (x_allowed * PI)).sqrt();
    eprintln!("ideal r_tip (real) = {ideal_r_tip:.10}");

    // R6: every tip individually needs R = 1.5*r_tip exact in f32. Three posts
    // share the nearest R6-exact candidate; the fourth is solved (per
    // audit-known-good.md §2.5) to close I4 clause (d)'s residual to the floor,
    // then itself re-snapped to the nearest R6-exact candidate and swept.
    let r_a = nearest_r6(ideal_r_tip, 4000);
    let r_a64 = f64::from(r_a);
    let target4 = tol4(d_total);
    eprintln!(
        "r_a = {r_a:.10} (f32 bits {:#010x}), diff from ideal = {:.3e}",
        r_a.to_bits(),
        f64::from(r_a) - ideal_r_tip
    );

    let a_owed_3 = 3.0 * x_allowed * PI * r_a64 * r_a64;
    let r4_target_real = ((d_total - a_owed_3) / (x_allowed * PI)).sqrt();
    eprintln!("r4 target (real, unrounded) = {r4_target_real:.12}");

    // Sweep r_a itself over a wide ULP window too (not just r4) to find the
    // globally best (r_a, r_4) pair, exactly as audit-known-good.md §2.5
    // describes ("sweep a common radius ... over ~1e5 f32 ULPs").
    const SWEEP: i64 = 150_000;
    const R4_WINDOW: i64 = 4000;
    let base_a = ideal_r_tip as f32;
    let mut best: Option<(f64, f32, f32)> = None;
    // Search candidate ULP offsets from r_a's ideal value OUTWARD (0, +1, -1,
    // +2, -2, ...), not linearly from -SWEEP to +SWEEP, so the sweep finds
    // the candidate CLOSEST to the ideal target that clears the residual
    // floor, rather than stopping at whichever extreme of the window happens
    // to satisfy it first. audit-known-good.md says the cost of this
    // technique is "about the seventh decimal" -- searching outward is what
    // makes that true rather than accidental.
    let offsets: Vec<i64> = {
        let mut v = Vec::with_capacity((2 * SWEEP + 1) as usize);
        v.push(0);
        for k in 1..=SWEEP {
            v.push(k);
            v.push(-k);
        }
        v
    };
    for n in offsets {
        let cand_a = step_f32(base_a, n);
        if cand_a <= 0.0 || !r6_exact(cand_a) {
            continue;
        }
        let cand_a64 = f64::from(cand_a);
        let owed3 = 3.0 * x_allowed * PI * cand_a64 * cand_a64;
        let rem = d_total - owed3;
        if rem <= 0.0 {
            continue;
        }
        let r4_target = (rem / (x_allowed * PI)).sqrt();
        let r4 = nearest_r6(r4_target, R4_WINDOW);
        let r4_64 = f64::from(r4);
        let sum = owed3 + x_allowed * PI * r4_64 * r4_64;
        let residual = (d_total - sum).abs();
        if best.is_none_or(|(bd, ..)| residual < bd) {
            best = Some((residual, cand_a, r4));
            if residual <= target4 {
                break;
            }
        }
    }
    let (residual, r_a_final, r_4_final) = best.expect("nonempty sweep");
    eprintln!(
        "SWEEP RESULT: residual = {residual:.6e} mm^2 (I4(d) floor = {target4:.6e} mm^2), \
         r_a = {:.10} r_4 = {:.10} (diff 7th decimal: {:.3e})",
        f64::from(r_a_final),
        f64::from(r_4_final),
        f64::from(r_4_final) - f64::from(r_a_final)
    );
    assert!(
        residual <= target4,
        "sweep did not close the I4(d) residual under the floor -- report this as a finding, \
         do not loosen the sweep silently"
    );

    // Four posts: three share r_a, the fourth (post index 3, at (-1,-1)) is
    // r_4. This is disclosed in the README (KG design doc §2.5's required
    // disclosure).
    let posts: [(f64, f64, f32); 4] = [
        (1.0, 1.0, r_a_final),
        (1.0, -1.0, r_a_final),
        (-1.0, 1.0, r_a_final),
        (-1.0, -1.0, r_4_final),
    ];

    // FRAME, corrected against the audit's actual reconstruction (a build
    // defect found by running the audit -- see README "Findings" #1): the
    // audit computes z_plate_mm = mesh.bbox.min[2] - translate_z
    // (src/context.rs::resolve_plate, ContractRelation branch), NOT
    // "-translate_z" alone. The design doc (audit-known-good.md §2.3, §3.1)
    // assumed a Z-CENTRED slab (z in [-1,+1]) while also asserting
    // "z_plate = -translate_z", which is only true when the model mesh's
    // OWN bbox.min[2] is exactly 0. A Z-centred slab with translate_z=5.0
    // reconstructs to z_plate=-6.0, not the doc's claimed -5.0 -- exactly
    // the 1.0 mm discrepancy this build first hit (I5 PATH_FREE_END on
    // every flare foot, because the flare's designed landing point at
    // z_plate=-5.0 sat 1.0 mm above the audit's own reconstructed plate).
    // Fix: anchor the slab underside at LOCAL z=0 (matching the working
    // `starved_patch`/`double_load` convention: `box_soup([...,0.0],[...,2.0])`),
    // so mesh.bbox.min[2]=0 and z_plate=-translate_z holds by construction.
    // Same physical design (4.00 mm float height), corrected bookkeeping only.
    let z_face = 0.0_f64; // slab underside, LOCAL frame anchored at 0
    let translate_z = 4.0_f64; // model floats 4.00 mm above the plate
    let z_plate = -translate_z; // now exact: mesh.bbox.min[2] (=0) - translate_z

    let mut records = Vec::new();
    let mut report: Vec<String> = Vec::new();
    let mut support_soup: Vec<[[f32; 3]; 3]> = Vec::new();

    for (idx, (x, y, r_tip_f32)) in posts.iter().enumerate() {
        let r_tip = f64::from(*r_tip_f32);
        let r_ball = 1.5_f64 * r_tip;
        // Verify R6 exactness end to end: R stored as f32 must equal 1.5*r_tip exactly.
        let r_ball_f32 = r_ball as f32;
        assert!(
            f64::from(r_ball_f32) == r_ball,
            "R6 violated for post {idx}: 1.5*r_tip did not round-trip"
        );

        // p0: the ball's deepest pole, buried h = beta*r_tip above the down-face.
        let p0_target_z = z_face + BETA * r_tip;
        let p0_z_f32 = p0_target_z as f32;
        let z_face_f32 = z_face as f32;
        assert!(f64::from(z_face_f32) == z_face, "z_face must be f32-exact");
        let h_delivered = f64::from(p0_z_f32) - f64::from(z_face_f32);

        let p0 = [*x, *y, f64::from(p0_z_f32)];
        let tip_end = [*x, *y, f64::from(p0_z_f32) - 2.0 * r_ball];
        let ball_centre = [*x, *y, f64::from(p0_z_f32) - r_ball];

        let ov = overlap_mm();
        let node_z = ball_centre[2] - 0.50;
        let taper_end = [*x, *y, node_z];

        let member_start = [*x, *y, node_z + ov];
        let member_end = [*x, *y, z_plate + 0.90];

        let flare_start = [*x, *y, member_end[2] + ov];
        let flare_end = [*x, *y, z_plate];

        let a_ideal = 1.2 * r_tip; // a(0.60r) = 1.2 r_tip, cut-disc radius at h=0.6r
        let a_req_real = a_cell / x_allowed; // all four cells are 4.00 mm^2 exactly by symmetry
        let a_ideal_area = PI * a_ideal * a_ideal;

        report.push(format!(
            "post {idx} (x={x:+.2}, y={y:+.2}): r_tip={r_tip:.10} R={r_ball:.10} \
             h_delivered={h_delivered:.10} (target {:.10}) a_ideal={a_ideal:.10} \
             A_ideal={a_ideal_area:.10} A_req={a_req_real:.10}",
            BETA * r_tip
        ));

        // TIP (wire 7, ContactSphere): poles p0 -> p0 + 2R*u, u=(0,0,-1)
        let tip = SynthRecord::new(RecordKind::ContactSphere, p0, tip_end, r_ball, r_ball)
            .with_soup(drop_zero_area(sphere_soup(ball_centre, r_ball, 8, 16)));

        // TAPER (wire 1, ContactTaper): ball centre -> node 0.50mm below
        let taper = SynthRecord::new(
            RecordKind::ContactTaper,
            ball_centre,
            taper_end,
            r_tip,
            r_ball,
        )
        .with_declared_depth(h_delivered)
        .with_matching_soup();

        // MEMBER (wire 3): node+0.10mm (into the taper) -> z_plate+0.90, r_arm=R constant
        let member = SynthRecord::new(RecordKind::Member, member_start, member_end, r_ball, r_ball)
            .with_matching_soup();

        // FLARE (wire 4, RootFlare): z_plate+1.00 (into member) -> z_plate, R -> 0.90mm foot
        let flare = SynthRecord::new(
            RecordKind::RootFlare,
            flare_start,
            flare_end,
            r_ball,
            FOOT_R_MM,
        )
        .with_matching_soup();

        support_soup.extend(tip.soup.iter().copied());
        support_soup.extend(taper.soup.iter().copied());
        support_soup.extend(member.soup.iter().copied());
        support_soup.extend(flare.soup.iter().copied());
        records.push(tip);
        records.push(taper);
        records.push(member);
        records.push(flare);
    }

    for line in &report {
        eprintln!("{line}");
    }

    let model_soup = box_soup([-2.0, -2.0, z_face], [2.0, 2.0, z_face + 2.0]);
    let model = SynthModel {
        name: "four_leg_overhang".into(),
        translate_mm: [0.0, 0.0, translate_z],
        mesh: model_soup.clone(),
        records,
    };
    let file = SynthFile::new().with_model(model);
    let raw = file.to_bytes();
    eprintln!("raw synth bytes: {}", raw.len());

    let spliced = splice_settings_blob(raw);
    eprintln!("spliced bytes: {}", spliced.len());

    let out = std::env::args()
        .nth(1)
        .expect("usage: kg1gen <out.chitubox> [overlap_mm]");
    std::fs::write(&out, &spliced).expect("write");
    eprintln!("wrote {out}");

    let stem = out.trim_end_matches(".chitubox");
    write_stl(&format!("{stem}-model.stl"), &model_soup);
    write_stl(&format!("{stem}-supports.stl"), &support_soup);
    eprintln!("wrote {stem}-model.stl and {stem}-supports.stl (LOCAL/untranslated frame, same as the .chitubox records -- both files share one frame so their relative geometry is correct even though neither carries translate_mm)");
}

/// Minimal binary STL writer: 80-byte header, u32 triangle count, then
/// 50 bytes/triangle (12-byte zero normal + 3x12-byte f32 vertices + 2-byte
/// attribute). No dependency, matches every other byte-level tool in this
/// project.
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

/// `synth::sphere_soup`'s pole triangles carry two bit-identical vertices at
/// `la == 0` and `la == lat` (both from `sin(0.0) == 0.0` exactly), which
/// `i8_no_dead_geometry` flags `GEOM_DEGENERATE reason=zero_area_triangle` --
/// a pre-existing defect in the shared test helper (`synth.rs`), documented
/// and worked around identically in `fixtures/starved_patch/generate.rs`.
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

fn get_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}
fn set_u32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

/// `docs/specs/audit-known-good.md` §7.2: insert the 396-byte all-zero
/// "empty project" settings blob after the 24-byte header, set header field
/// 12 (`H_SETTINGS_SIZE`) to 420, and shift every ABSOLUTE pointer field that
/// pointed at content originally beyond the header by +396. The design doc's
/// own bullet names four fields (off8, off20, each model's mesh_off and
/// support_index_ptr, and each support index's own mesh_off); a full,
/// self-consistent file additionally needs the support index's records_off
/// and each individual record's own mesh_off (its slice of the shared soup
/// region) shifted too, or the file parses as garbage past the settings
/// blob. All of those are updated here.
fn splice_settings_blob(old: Vec<u8>) -> Vec<u8> {
    const SHIFT: u32 = 396;
    let old_off8 = get_u32(&old, c::H_OFF8) as usize;
    let old_off20 = get_u32(&old, c::H_OFF20);
    let model_count = get_u32(&old, c::H_MODEL_COUNT) as usize;

    let mut out = Vec::with_capacity(old.len() + SHIFT as usize);
    out.extend_from_slice(&old[0..c::HEADER_LEN]);
    out.extend(std::iter::repeat(0u8).take(SHIFT as usize));
    out.extend_from_slice(&old[c::HEADER_LEN..]);

    set_u32(&mut out, c::H_SETTINGS_SIZE, c::SETTINGS_SIZE_POPULATED);
    set_u32(&mut out, c::H_OFF8, old_off8 as u32 + SHIFT);
    set_u32(&mut out, c::H_OFF20, old_off20 + SHIFT);

    for mi in 0..model_count {
        // Every position >= HEADER_LEN in `old` sits at position+SHIFT in `out`.
        let model_pos_new = old_off8 + mi * c::MODEL_RECORD_LEN + SHIFT as usize;

        let old_mesh_off = get_u32(&out, model_pos_new + c::M_MESH_OFF);
        set_u32(&mut out, model_pos_new + c::M_MESH_OFF, old_mesh_off + SHIFT);

        let old_index_ptr = get_u32(&out, model_pos_new + c::M_SUPPORT_INDEX_PTR);
        if old_index_ptr == 0 {
            continue;
        }
        let new_index_ptr = old_index_ptr + SHIFT;
        set_u32(&mut out, model_pos_new + c::M_SUPPORT_INDEX_PTR, new_index_ptr);
        let idx_pos = new_index_ptr as usize;

        let n_records = get_u32(&out, idx_pos + c::SI_N_RECORDS) as usize;
        let old_records_off = get_u32(&out, idx_pos + c::SI_RECORDS_OFF);
        let new_records_off = old_records_off + SHIFT;
        set_u32(&mut out, idx_pos + c::SI_RECORDS_OFF, new_records_off);

        let old_si_mesh_off = get_u32(&out, idx_pos + c::SI_MESH_OFF);
        set_u32(&mut out, idx_pos + c::SI_MESH_OFF, old_si_mesh_off + SHIFT);

        for ri in 0..n_records {
            let rec_pos = new_records_off as usize + ri * c::SEG_RECORD_LEN;
            let old_rec_mesh_off = get_u32(&out, rec_pos + c::S_MESH_OFF);
            if old_rec_mesh_off != 0 {
                set_u32(&mut out, rec_pos + c::S_MESH_OFF, old_rec_mesh_off + SHIFT);
            }
        }
    }
    out
}
