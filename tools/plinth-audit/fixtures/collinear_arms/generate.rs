// Standalone generator for `collinear_arms.chitubox` -- NOT part of the
// `plinth-audit` crate, NOT built by its `Cargo.toml`. Kept alongside the
// fixture only as the exact, re-runnable record of how the bytes were
// produced (same convention as the `double_load` sibling fixture's
// `generate.rs`: compiled ad hoc against the already-built `plinth_audit`
// rlib, never wired into the crate's own build).
//
// Design: one model (a flat box), THREE independent support chains
// (TIP -> TAPER -> MEMBER -> RAFT pad each), seated at x = -S, 0, +S, all at
// y = 0 -- perfectly collinear along the model's local X axis. Each tip is
// sized via the same inversion `double_load` uses,
// `A_implied = x_allowed * pi * r_tip^2`, against ITS OWN Voronoi-owned
// share of the box's down-facing patch, so every one of the three arms
// individually satisfies the per-arm area law.
//
// The box's footprint is NOT centred on the contacts' line: its y-extent
// runs from `-f` to `+b` with `b > f`, so the three contacts (all at y=0)
// sit off-centre and the box's own down-facing centroid sits `(b-f)/2` mm
// off the line the three contacts define. This is `docs/physics.md` §2(c)
// made physical: "three collinear arms have no lateral stability
// perpendicular to their line, at any X" -- every arm passes its own area
// law, and the arrangement is still unrecoverable because the load does not
// act on the line the arms hold.
//
// Every geometric margin below is DERIVED, not guessed -- see "Construction
// notes: how the margins were derived" further down, which explains the
// `normal_ambiguous` trap this generator works around.

use plinth_audit::consts as c;
use plinth_audit::reader::RecordKind;
use plinth_audit::synth::{box_soup, frustum_soup, sphere_soup, SynthFile, SynthModel, SynthRecord};

/// Same zero-area-triangle workaround as `double_load` / `starved_patch`:
/// `synth::sphere_soup`'s pole ring at `la == 0` (and `la == lat`) emits
/// triangles with two bit-identical vertices, which `i8_no_dead_geometry`
/// correctly flags `GEOM_DEGENERATE reason=zero_area_triangle`. Pre-existing
/// defect in the shared test helper, not owned by this fixture; dropped here
/// so it does not pollute this fixture's own i8 reading.
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

fn main() {
    let pi = std::f64::consts::PI;
    let x_allowed = c::X_ALLOWED; // 12.0
    let ball_factor = c::BALL_FACTOR_STUB; // 1.5

    // --- Per-contact sizing -------------------------------------------
    //
    // Chosen so r_tip sits safely under `TIP_RADIUS_MAX_MM = 1.000 mm`
    // (see "PANIC avoided" below for what happens if it does not).
    let r_tip = 0.95_f64;
    let a_owned = x_allowed * pi * r_tip * r_tip; // A_implied = A_owned, by construction
    let r_ball = ball_factor * r_tip; // R = f * r_tip, the emitted ball radius (r1=r2)
    let arm_r = r_tip * 1.5; // comfortably above r_tip; see I3 margin note below

    let h_req_ball_term = c::ball_seat_factor(ball_factor) * r_tip;
    let h_delivered = h_req_ball_term * 1.3; // 30% margin over the ball-seat term
    assert!(h_delivered > h_req_ball_term);

    // --- Geometric margins, derived (not guessed) -----------------------
    //
    // `seat::seat_for` cross-checks the entry triangle's own normal against
    // the AREA-WEIGHTED MEAN normal of every model triangle whose CLOSEST
    // POINT lies within `r_ball` of the tip ball's CENTRE (not the entry
    // point) -- `src/seat.rs`, the `mean_normal` block. On a box, the ball
    // centre sits `r_ball` below the entry point, i.e. at
    // `z = -2 + h_delivered - r_ball`, which is BELOW the box's bottom face
    // (z=-2) by `r_ball - h_delivered` whenever `r_ball > h_delivered` (true
    // here). The box's own SIDE WALLS are vertical planes running from
    // z=-2 to z=+2, so the closest point of a side wall to the ball centre
    // is the wall's bottom edge, at 3-D distance
    //   dist = sqrt(horiz_margin^2 + (r_ball - h_delivered)^2)
    // where `horiz_margin` is the plan-view distance from the contact to
    // the nearest box edge. If that distance is <= r_ball, the side wall's
    // normal (horizontal) gets folded into the mean against the bottom
    // face's normal (vertical, (0,0,-1)) and the two disagree by ~90
    // degrees, tripping `seat.normal_ambiguous` (the 5-degree gate) and
    // cascading to SEAT_DEPTH_INDETERMINATE / CONTACT_AREA_INDETERMINATE /
    // PATH_INDETERMINATE / GEOM_INDETERMINATE.
    //
    // Solving `dist > r_ball` for `horiz_margin` gives the minimum safe
    // margin:
    //   horiz_margin_min = sqrt(h_delivered * (2*r_ball - h_delivered))
    // A first attempt at this fixture used a 1 mm front margin against
    // r_ball = 1.4658 mm / h_delivered = 0.5 mm (horiz_margin_min = 1.10 mm)
    // and every one of the three contacts came back `normal_ambiguous` --
    // see "Construction notes" in the README for the measured evidence.
    // This generator now derives the margin instead of guessing at it, with
    // a full 1 mm added on top as a working buffer.
    let horiz_margin_min = (h_delivered * (2.0 * r_ball - h_delivered)).sqrt();
    let margin = horiz_margin_min + 1.0;

    // Eccentricity: how far the box's down-facing centroid sits off the
    // contacts' line (y=0). Chosen as 60% of the largest value that still
    // leaves BOTH the x-margin (`0.5*spacing >= margin`) and the y-margins
    // (`f >= margin`, `b >= margin`) satisfied -- see the derivation in the
    // README. `f` is pinned at `margin` (the tightest the near edge may be)
    // and `spacing` follows from `a_owned = spacing * (f + b)`.
    let max_eccentricity = a_owned / (4.0 * margin) - margin;
    assert!(max_eccentricity > 0.0, "geometry too tight for any eccentricity at all");
    let eccentricity = max_eccentricity * 0.6;

    let f = margin; // near-edge margin, y = -f
    let b = f + 2.0 * eccentricity; // far-edge margin, y = +b; centroid at (b-f)/2 = eccentricity
    let h = f + b; // total y-extent
    let spacing = a_owned / h; // half-spacing between the three contacts along x
    let half_x_margin = 0.5 * spacing; // plan-view distance, outer contact to box edge in x

    eprintln!("r_tip = {r_tip:.9}  r_ball = {r_ball:.9}  arm_r = {arm_r:.9}");
    eprintln!("h_req_ball_term = {h_req_ball_term:.9}  h_delivered = {h_delivered:.9}");
    eprintln!("a_owned per contact = {a_owned:.9}  (D_total = {:.9})", a_owned * 3.0);
    eprintln!(
        "horiz_margin_min = {horiz_margin_min:.9}  margin (with 1mm buffer) = {margin:.9}"
    );
    eprintln!(
        "max_eccentricity = {max_eccentricity:.9}  chosen eccentricity = {eccentricity:.9}"
    );
    eprintln!("f (near margin) = {f:.9}  b (far margin) = {b:.9}  h (y-extent) = {h:.9}");
    eprintln!(
        "spacing (half) = {spacing:.9}  half_x_margin = {half_x_margin:.9} (must be >= margin = {margin:.9})"
    );
    assert!(half_x_margin >= margin, "x margin insufficient");
    assert!(f >= margin && b >= margin, "y margin insufficient");

    eprintln!(
        "X at arm (taper/member) section = {:.6} (a_owned / (pi*arm_r^2))",
        a_owned / (pi * arm_r * arm_r)
    );
    let raft_r = c::RAFT_PAD_DIAMETER_MM * 0.5;
    eprintln!(
        "X at raft pad section (fixed diameter, worst case) = {:.6}",
        a_owned / (pi * raft_r * raft_r)
    );
    eprintln!(
        "box: x in [{}, {}], y in [{}, {}], centroid y = {}",
        -1.5 * spacing,
        1.5 * spacing,
        -f,
        b,
        (b - f) / 2.0
    );

    let mesh = box_soup([-1.5 * spacing, -f, -2.0], [1.5 * spacing, b, 2.0]);

    let xs = [-spacing, 0.0, spacing];

    let column = |x0: f64| -> Vec<SynthRecord> {
        let y0 = 0.0_f64;
        // TIP: start = p0 (deepest/topmost point of the ball), end = far pole.
        let p0 = [x0, y0, -2.0 + h_delivered];
        let end = [x0, y0, p0[2] - 2.0 * r_ball];
        let centre = [x0, y0, (p0[2] + end[2]) * 0.5];

        let tip = SynthRecord::new(RecordKind::ContactSphere, p0, end, r_ball, r_ball)
            .with_soup(drop_zero_area(sphere_soup(centre, r_ball, 8, 16)));

        // TAPER: start = tip's connective point (ball CENTRE, per graph.rs),
        // narrow end = r_tip, widening toward arm_r.
        let taper_end = [x0, y0, -4.2];
        let taper = SynthRecord::new(RecordKind::ContactTaper, centre, taper_end, r_tip, arm_r)
            .with_declared_depth(h_delivered)
            .with_matching_soup();

        // MEMBER: constant-radius arm from the taper's wide end down to the
        // plate. The offset (-7.03, not a round -7.0) keeps the model's
        // entry plane (z = -2.0) off a raster layer boundary, the same
        // reason double_load's translate_z is 5.03 rather than 5.0 (see
        // that fixture's README for the mechanism).
        let member_end = [x0, y0, -7.03];
        let member = SynthRecord::new(RecordKind::Member, taper_end, member_end, arm_r, arm_r)
            .with_matching_soup();

        // RAFT pad: built by hand (a cone with equal end radii = a cylinder),
        // same reason as double_load: `SynthRecord::with_matching_soup` does
        // not cover RaftPadOrBridge.
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
    for &x0 in &xs {
        records.extend(column(x0));
    }

    let file = SynthFile::new().with_model(SynthModel {
        name: "collinear_arms".into(),
        translate_mm: [0.0, 0.0, 5.03],
        mesh,
        records,
    });

    let out = std::env::args()
        .nth(1)
        .expect("usage: gen_collinear_arms <out.chitubox>");
    file.write(std::path::Path::new(&out)).expect("write");
    eprintln!("wrote {out}");
}
