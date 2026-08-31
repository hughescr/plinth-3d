//! Builds `eccentric_polygon.chitubox`: one floating slab body carried by
//! THREE support columns, each individually well within `X_ALLOWED = 12`,
//! clustered together near one end of the slab so their triangle does NOT
//! contain the slab's footprint centroid.
//!
//! This is the planted-defect fixture for `i10_arrangement_spread`
//! (`docs/specs/audit-spread-criterion.md` §7's `spread_eccentric` row):
//! "the same slab, three tips clustered at one end so the centroid falls
//! outside their hull" -> `check_fails`, `SPREAD_NO_EQUILIBRIUM`.
//!
//! Each column is a straight vertical TIP -> TAPER -> MEMBER -> RAFT chain,
//! built the same way as `fixtures/double_load/generate.rs` (reused
//! deliberately: that geometry is already proven clean against I1/I2/I3/I5/I6
//! by that fixture's own passing collateral report). The only things this
//! file changes are: three columns instead of two, sized to an EXACT
//! Voronoi-fair one-third share each (so I4's tight float-level bracket
//! holds), and positioned as a small equilateral triangle offset to one side
//! of a much larger footprint (so the triangle cannot come close to
//! containing the centroid, and so no two columns' balls, arms or raft pads
//! come near each other).
//!
//! `plinth_audit::synth` validates nothing, so this defect stays planted.

use plinth_audit::consts as c;
use plinth_audit::reader::RecordKind;
use plinth_audit::synth::{box_soup, frustum_soup, sphere_soup, SynthFile, SynthModel, SynthRecord};

/// Same pre-existing `synth::sphere_soup` pole-triangle defect noted in
/// `fixtures/double_load/generate.rs`: `lat=0`'s two pole vertices are
/// bit-identical, which `i8_no_dead_geometry` correctly flags as
/// `GEOM_DEGENERATE reason=zero_area_triangle`. Not owned here (it lives in
/// `src/synth.rs`); dropped so only the planted defect is visible.
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

// Real Voronoi-owned shares (mm^2) at the fixed column positions below,
// order [top, bottom-left, bottom-right]. PASS 1 measurement: run this
// generator with these set to an equal split, run the actual audit, read
// `i2_contact_area.measured.contacts[].a_owned_mm2` back for each
// `contact_id`, and paste the real numbers here (see README "measurement
// provenance"). Ownership depends only on XY position, never on tip radius,
// so this two-pass process is exact, not an approximation.
// PASS 1 measurement, 2026-08-30, against this exact file with equal r_tip
// (see git history of this file / the README's "measurement provenance"
// section for the bootstrap run): contact_id 0 (top) = 11.049520616189607,
// contact_id 4 (bottom-left) = 17.729310045138444, contact_id 8
// (bottom-right) = 7.941171017283121. Sum = 36.720000 = D_total, exactly.
const OWNED_TOP: f64 = 11.049520616189607;
const OWNED_BL: f64 = 17.729310045138444;
const OWNED_BR: f64 = 7.941171017283121;

fn main() {
    let f = std::f64::consts::PI;
    let x_allowed = c::X_ALLOWED; // 12.0
    let ball_factor = c::BALL_FACTOR_STUB; // 1.5

    // The slab: x in [-hx,hx], y in [-hy,hy], z in [-2,2].
    // Bottom face (z = -2) is the ONLY down-facing geometry, so
    // D_total = 2*hx * 2*hy exactly, and its footprint centroid is
    // (0, 0) by construction (box is symmetric about both axes).
    let (hx, hy, hz) = (3.4_f64, 2.7_f64, 2.0_f64);
    let d_total = (2.0 * hx) * (2.0 * hy); // 56.0

    // Three column centres: an equilateral triangle, side length 3.2 mm
    // (> RAFT_PAD_DIAMETER_MM = 2.2 mm, so no two raft pads touch; also
    // will be > 2*r_ball for every r_ball used below, so no two tip balls
    // touch), centred at (cx, 0.0). `cx` is chosen just past the threshold
    // at which the origin crosses the triangle's left edge (see the printed
    // `centroid clearance` below) -- a SMALL, deliberate eccentricity, not a
    // maximal one: keeping the triangle close to the slab's own centroid is
    // what keeps the three contacts' real Voronoi-owned shares from
    // spreading over an unbounded range (see README, "why a small
    // eccentricity, not a dramatic one").
    let side = 2.6_f64;
    let rc = side / 3.0_f64.sqrt(); // circumradius
    let cx = 1.05_f64;
    let cy = 0.0_f64;
    let cols: [(f64, f64); 3] = [
        (cx, cy + rc),
        (cx - rc * 0.8660254037844387, cy - rc * 0.5),
        (cx + rc * 0.8660254037844387, cy - rc * 0.5),
    ];

    // Signed clearance of the origin outside the triangle, along y=0: the
    // left edge (top -> bottom-left vertex) crosses y=0 at
    // x = cx - rc*0.5/cos(30deg)*... -- computed directly here rather than
    // asserted symbolically, so a future edit to `cx`/`side` cannot silently
    // flip the sign without the assertion catching it.
    let (top, bl, br) = (cols[0], cols[1], cols[2]);
    let cross_y0 = |a: (f64, f64), b: (f64, f64)| -> Option<f64> {
        if (a.1 - b.1).abs() < 1e-12 {
            return None;
        }
        let t = (0.0 - a.1) / (b.1 - a.1);
        if !(0.0..=1.0).contains(&t) {
            return None;
        }
        Some(a.0 + t * (b.0 - a.0))
    };
    let left_x = cross_y0(top, bl).expect("top-BL edge must cross y=0");
    let right_x = cross_y0(top, br).expect("top-BR edge must cross y=0");
    assert!(left_x > 0.0, "left edge must cross y=0 to the right of x=0");
    assert!(right_x > left_x);
    let clearance_mm = left_x; // distance from x=0 to the nearest edge, along y=0

    eprintln!("triangle side = {side} mm   circumradius = {rc:.6} mm   centre = ({cx},{cy})");
    for (i, &(x, y)) in cols.iter().enumerate() {
        eprintln!("  column {i}: ({x:.6}, {y:.6})");
    }
    eprintln!("centroid clearance outside triangle, along y=0: {clearance_mm:.6} mm");
    eprintln!(
        "distance from centroid (0,0) to nearest column vertex = {:.6} mm",
        cols
            .iter()
            .map(|&(x, y)| (x * x + y * y).sqrt())
            .fold(f64::INFINITY, f64::min)
    );

    // Per-column r_tip, sized with a SAFETY_MARGIN over that column's own
    // real Voronoi-owned share (measured empirically by running the actual
    // audit against a first draft of this file with equal-sized tips, then
    // hard-coded here -- see README "measurement provenance"). Ownership
    // depends only on contact XY position, never on r_tip, so this is a
    // legitimate two-pass measurement, not circular.
    const SAFETY_MARGIN: f64 = 1.35;
    let owned_mm2: [f64; 3] = [OWNED_TOP, OWNED_BL, OWNED_BR];
    let mut r_tip = [0.0_f64; 3];
    let mut r_ball = [0.0_f64; 3];
    for i in 0..3 {
        let a_implied = SAFETY_MARGIN * owned_mm2[i];
        r_tip[i] = (a_implied / (x_allowed * f)).sqrt();
        r_ball[i] = ball_factor * r_tip[i];
        assert!(
            r_tip[i] > c::TIP_RADIUS_MIN_ALLOWED_MM && r_tip[i] < c::TIP_RADIUS_MAX_MM,
            "column {i}: r_tip {} out of the declarable range",
            r_tip[i]
        );
    }
    let h_delivered = 0.45_f64;
    for i in 0..3 {
        let h_req_ball_term = c::ball_seat_factor(ball_factor) * r_tip[i];
        assert!(
            h_delivered > h_req_ball_term,
            "column {i}: seat must clear I1's floor ({h_req_ball_term})"
        );
    }

    eprintln!("SAFETY_MARGIN = {SAFETY_MARGIN}   h_delivered = {h_delivered}");
    for i in 0..3 {
        eprintln!(
            "  column {i}: owned={:.6} mm^2  a_implied={:.6} mm^2  r_tip={:.9} mm  r_ball={:.9} mm",
            owned_mm2[i],
            SAFETY_MARGIN * owned_mm2[i],
            r_tip[i],
            r_ball[i]
        );
    }
    eprintln!(
        "sum(a_implied) = {:.6} mm^2   D_total = {d_total:.6} mm^2   (I4 clause d gap expected -- see README)",
        owned_mm2.iter().map(|o| SAFETY_MARGIN * o).sum::<f64>()
    );

    // Sanity: every column must clear the side walls by at least r_ball, and
    // every pairwise spacing must clear both the raft pad diameter and 2*r_ball.
    let raft_min_spacing = c::RAFT_PAD_DIAMETER_MM;
    for (i, &(x, y)) in cols.iter().enumerate() {
        assert!(x.abs() + r_ball[i] < hx, "column {i}: tip ball must clear the +/-X wall");
        assert!(y.abs() + r_ball[i] < hy, "column {i}: tip ball must clear the +/-Y wall");
    }
    for i in 0..3 {
        for j in (i + 1)..3 {
            let (x0, y0) = cols[i];
            let (x1, y1) = cols[j];
            let d = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
            assert!(d > raft_min_spacing, "raft pads must not overlap");
            assert!(d > r_ball[i] + r_ball[j], "tip balls must not overlap");
        }
    }

    let mesh = box_soup([-hx, -hy, -hz], [hx, hy, hz]);

    let column = |i: usize, x0: f64, y0: f64| -> Vec<SynthRecord> {
        let rt = r_tip[i];
        let rb = r_ball[i];
        let p0 = [x0, y0, -hz + h_delivered];
        let end = [x0, y0, p0[2] - 2.0 * rb];
        let centre = [x0, y0, (p0[2] + end[2]) * 0.5];

        let tip = SynthRecord::new(RecordKind::ContactSphere, p0, end, rb, rb)
            .with_soup(drop_zero_area(sphere_soup(centre, rb, 8, 16)));

        let arm_r = (1.5 * rt).max(rt + 0.3); // generous headroom above r_tip, see README
        let taper_end = [x0, y0, -4.2];
        let taper = SynthRecord::new(RecordKind::ContactTaper, centre, taper_end, rt, arm_r)
            .with_declared_depth(h_delivered)
            .with_matching_soup();

        // z_plate = mesh.bbox.min.z - translate_z = -2 - 5.03 = -7.03, same
        // convention as fixtures/double_load (translate_z deliberately not a
        // multiple of layer_height_mm = 0.05).
        let member_end = [x0, y0, -7.03];
        let member = SynthRecord::new(RecordKind::Member, taper_end, member_end, arm_r, arm_r)
            .with_matching_soup();

        let raft_r = c::RAFT_PAD_DIAMETER_MM * 0.5;
        let raft_top = [member_end[0], member_end[1], member_end[2] + c::RAFT_PAD_HEIGHT_MM];
        let raft = SynthRecord::new(
            RecordKind::RaftPadOrBridge,
            member_end,
            member_end,
            c::RAFT_PAD_DIAMETER_MM,
            c::RAFT_PAD_HEIGHT_MM,
        )
        .as_pad()
        .with_soup(frustum_soup(member_end, raft_r, raft_top, raft_r, 20));

        vec![tip, taper, member, raft]
    };

    let mut records = Vec::new();
    for (i, &(x, y)) in cols.iter().enumerate() {
        records.extend(column(i, x, y));
    }

    let file = SynthFile::new().with_model(SynthModel {
        name: "eccentric_polygon".into(),
        translate_mm: [0.0, 0.0, 5.03],
        mesh,
        records,
    });

    let out = std::env::args()
        .nth(1)
        .expect("usage: gen_eccentric_polygon <out.chitubox>");
    file.write(std::path::Path::new(&out)).expect("write");
    eprintln!("wrote {out}");
}
