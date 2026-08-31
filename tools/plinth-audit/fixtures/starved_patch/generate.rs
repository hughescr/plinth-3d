//! Standalone generator for `starved_patch.chitubox` -- NOT part of the
//! `plinth-audit` crate, NOT built by its `Cargo.toml`. Kept alongside the
//! fixture only as the exact, re-runnable record of how the bytes were
//! produced (same convention as the `double_load` sibling fixture's
//! `generate.rs`). Built and run as a throwaway crate outside this repo,
//! `plinth-audit` pulled in as a `path` dependency, per
//! `tools/plinth-audit/README.md`'s fixture-building instructions.
//!
//! Design: one model (a box), one support chain (TIP -> TAPER -> MEMBER ->
//! RAFT pad), `r_tip = 0.185 mm`, `h_delivered = 0.103 mm` -- comfortably
//! embedded (`h_required = 0.100 mm`, the ruled default), but the tip's
//! DELIVERED contact patch, measured on the CURED (rasterised) support
//! solid, comes up short of the area law even though the IDEAL geometric
//! disc at that same depth would pass it. See `README.md` for the full
//! account and the measured numbers.

use plinth_audit::consts as c;
use plinth_audit::reader::RecordKind;
use plinth_audit::synth::{sphere_soup, box_soup, SynthFile, SynthModel, SynthRecord};

const R_TIP: f64 = 0.185; // mm, tip radius
const H_DELIVERED: f64 = 0.103; // mm, real embedded depth (h_required = 0.100 mm)
const F: f64 = 1.5; // ball_factor_stub

/// `synth::sphere_soup`'s pole triangles carry two bit-identical vertices at
/// `la == 0` and `la == lat` (both computed from `sin(0.0) == 0.0` exactly),
/// which `i8_no_dead_geometry` correctly flags `GEOM_DEGENERATE
/// reason=zero_area_triangle` -- a pre-existing defect in the shared test
/// helper (`tools/plinth-audit/src/synth.rs`), reproduced on that check's
/// own bundled fixture and on the `double_load` / `shallow_seat` sibling
/// fixtures, and NOT something this fixture owns or may fix. Dropping the
/// zero-area triangles here keeps the plant surgical: only the tip's own
/// analytic geometry (never its soup) drives I1/I2, so this has no effect
/// on the planted defect.
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

/// Step an f32 value by `n` ULPs (n may be negative).
fn step_f32(x: f32, n: i64) -> f32 {
    let bits = x.to_bits() as i64;
    f32::from_bits((bits + n) as u32)
}

/// Search nearby f32 half-widths so the ACTUAL stored box footprint (both
/// half-widths independently f32-rounded, exactly as `box_soup` stores them)
/// lands as close as a 2-triangle rectangle can get to `target_area`. Needed
/// because `i4_load_ownership` clause (d) demands `|D_total - A_implied| <=
/// max(4096*eps64*D_total, 1e-9)` -- effectively 1e-9 mm^2 absolute at this
/// scale -- far tighter than a single f32-rounded dimension can hit (see
/// README "Known non-target failures").
fn find_matching_halfwidths(target_area: f64, hx0: f64, tol: f64) -> (f64, f64, f64) {
    let mut best: Option<(f64, f64, f64, f64)> = None;
    for n in -30_000i64..=30_000 {
        let hx_f32 = step_f32(hx0 as f32, n);
        let hx = f64::from(hx_f32);
        if hx <= 0.0 {
            continue;
        }
        let hy_target = target_area / (4.0 * hx);
        let hy_f32 = hy_target as f32;
        let hy = f64::from(hy_f32);
        let area = 4.0 * hx * hy;
        let diff = (area - target_area).abs();
        if best.is_none_or(|(bd, ..)| diff < bd) {
            best = Some((diff, hx, hy, area));
            if diff <= tol {
                break;
            }
        }
    }
    let (diff, hx, hy, area) = best.expect("nonempty search");
    eprintln!("halfwidth search: best |diff|={diff:.3e} (tol={tol:.3e}) hx={hx} hy={hy} area={area}");
    (hx, hy, area)
}

fn main() {
    let r_tip = R_TIP;
    let h_delivered = H_DELIVERED;
    let f = F;
    let r_ball = f * r_tip;
    let ball_term = c::ball_seat_factor(f) * r_tip;
    let h_required = c::DEFAULT_CONTACT_DEPTH_MM
        .max(c::LAYER_HEIGHT_DEFAULT_MM)
        .max(ball_term);
    assert!(
        h_delivered > h_required,
        "h_delivered must clear h_required: {h_delivered} vs {h_required}"
    );
    eprintln!("r_tip={r_tip} r_ball={r_ball} h_required={h_required:.6} h_delivered={h_delivered}");

    // The box footprint is sized so the WHOLE footprint's demand
    // (D_total, the down-facing area) equals x_allowed * pi * r_tip^2 --
    // exactly what the engine would have sized THIS r_tip for at X = 12.000
    // (the measured corpus fact: every real contact is sized to that ratio
    // exactly). Since this is the file's only contact, it owns all of it
    // (A_owned = D_total). This makes the plant as narrow as possible: I3's
    // arm-section ceiling and I4's declared/reconstructed bracket are both
    // satisfied by the SAME choice, so only I2's rasterisation gap is left
    // to fail.
    let target_a_owned = c::X_ALLOWED * std::f64::consts::PI * r_tip * r_tip;
    let tol4 = (4096.0 * c::EPS64 * target_a_owned).max(1e-9);
    let (hx, hy, achieved_area) = find_matching_halfwidths(target_a_owned, 0.5, tol4 * 0.5);
    let lx = 2.0 * hx;
    let ly = 2.0 * hy;
    eprintln!(
        "target_a_owned={target_a_owned:.10} achieved_area={achieved_area:.10} |diff|={:.3e} tol4={tol4:.3e}",
        (achieved_area - target_a_owned).abs()
    );
    eprintln!("lx={lx:.10} ly={ly:.10}");

    // Box bottom face at z=0 (model-centred frame), interior above it. The
    // tip's p0 embeds H_DELIVERED inside; the axis points -Z, down through
    // the bottom face and into the support column below, toward the plate.
    let p0 = [0.0, 0.0, h_delivered];
    let ball_centre = [0.0, 0.0, h_delivered - r_ball];
    let tip_end = [0.0, 0.0, h_delivered - 2.0 * r_ball];

    // TAPER: ball centre (its connective point, NOT a pole -- a ball's
    // connective point is its midpoint, `docs/specs/chitubox-format.md` §5)
    // to a plain, exactly-matched design joint with MEMBER (both 0.35 mm).
    let taper_r0 = r_tip;
    let taper_r1 = 0.35;
    let taper_end = [0.0, 0.0, tip_end[2] - 1.5];

    let member_r = 0.35;
    let member_end = [0.0, 0.0, taper_end[2] - 3.0];

    let pad_base_z = member_end[2] - c::RAFT_PAD_HEIGHT_MM;

    let tip = SynthRecord::new(RecordKind::ContactSphere, p0, tip_end, r_ball, r_ball)
        .with_soup(drop_zero_area(sphere_soup(ball_centre, r_ball, 8, 16)));

    let taper = SynthRecord::new(RecordKind::ContactTaper, ball_centre, taper_end, taper_r0, taper_r1)
        .with_declared_depth(h_delivered) // matches h_delivered exactly -- only I2 is meant to fire
        .with_matching_soup();

    let member = SynthRecord::new(RecordKind::Member, taper_end, member_end, member_r, member_r)
        .with_matching_soup();

    // RAFT pad: left with NO soup (`.as_pad()` alone) -- SynthRecord::
    // with_matching_soup's own documented contract for RaftPadOrBridge is an
    // empty soup, matching the `broken_path` / `double_load` / `bare_island`
    // sibling fixtures. A hand-built substitute soup was tried and reverted;
    // see README "Known non-target failures" (I8).
    let pad = SynthRecord::new(
        RecordKind::RaftPadOrBridge,
        [0.0, 0.0, pad_base_z],
        [0.0, 0.0, pad_base_z],
        c::RAFT_PAD_DIAMETER_MM,
        c::RAFT_PAD_HEIGHT_MM,
    )
    .as_pad();

    // z_plate = -translate_mm.z must match the frame check's reconstructed
    // plate height: the lowest NON-RAFT support point (member's own end).
    // The raft/pad frame is a separate, already-disclosed anomaly
    // (native ChituBox writes type-6 raft records at z=0; see
    // `tools/plinth-audit/README.md` finding 2) that frame_check does not
    // gate on.
    let translate_z = -member_end[2];

    let model = SynthModel {
        name: "starved_patch".into(),
        translate_mm: [0.0, 0.0, translate_z],
        mesh: box_soup([-lx / 2.0, -ly / 2.0, 0.0], [lx / 2.0, ly / 2.0, 2.0]),
        records: vec![tip, taper, member, pad],
    };
    let file = SynthFile::new().with_model(model);

    let out = std::env::args()
        .nth(1)
        .expect("usage: gen_starved_patch <out.chitubox>");
    file.write(std::path::Path::new(&out)).expect("write");
    eprintln!("wrote {out}");
}
