use plinth_audit::consts as c;
use plinth_audit::reader::RecordKind;
use plinth_audit::synth::{box_soup, frustum_soup, sphere_soup, SynthFile, SynthModel, SynthRecord};

/// `synth::sphere_soup`'s pole triangles carry two bit-identical vertices at
/// `la == 0` (both computed from `sin(0.0) == 0.0` exactly), which
/// `i8_no_dead_geometry` correctly flags `GEOM_DEGENERATE
/// reason=zero_area_triangle` -- a pre-existing defect in the shared test
/// helper (`tools/plinth-audit/src/synth.rs`), reproduced on that check's own
/// bundled fixture (`checks::dead_geometry::tests::clean_chain_file`) and NOT
/// something this fixture owns or may fix. Dropping the zero-area triangles
/// here keeps the plant surgical: only I4 should fire on this file.
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
    let f = std::f64::consts::PI;
    let x_allowed = c::X_ALLOWED; // 12.0
    let ball_factor = c::BALL_FACTOR_STUB; // 1.5

    // The single down-facing patch both contacts will claim in full.
    // Box: x in [-2.5, 2.5], y in [-2, 2], z in [-2, 2].
    // Bottom face (z = -2, outward normal (0,0,-1)) is the ONLY down-facing
    // geometry -> D_total = 5 * 4 = 20.0 mm^2 exactly.
    let d_total = 20.0_f64;

    // Size EACH tip as though it alone carries the whole patch:
    //   A_implied = x_allowed * pi * r_tip^2 = d_total  =>  r_tip = sqrt(d_total / (x_allowed*pi))
    let r_tip = (d_total / (x_allowed * f)).sqrt();
    let r_ball = ball_factor * r_tip; // R = f * r_tip, the emitted ball radius (r1=r2)

    // Embedment: axis straight down (-z), so the cap height along the axis
    // equals the vertical depth exactly. Owner-ruling h_req at normal
    // incidence: max(0.100, 0.050, ball_seat_factor(1.5)*r_tip).
    let h_req_ball_term = c::ball_seat_factor(ball_factor) * r_tip;
    let h_delivered = 0.30_f64; // comfortably above h_req_ball_term (~0.2782)
    assert!(h_delivered > h_req_ball_term);

    eprintln!("r_tip = {r_tip:.9}  r_ball(R) = {r_ball:.9}");
    eprintln!("h_req ball term = {h_req_ball_term:.9}  h_delivered = {h_delivered}");
    eprintln!(
        "A_implied per contact = {:.9} (should equal d_total = {d_total})",
        x_allowed * f * r_tip * r_tip
    );

    let mesh = box_soup([-2.5, -2.0, -2.0], [2.5, 2.0, 2.0]);

    let column = |x0: f64| -> Vec<SynthRecord> {
        // TIP: start = p0 (deepest/topmost point of the ball), end = far pole.
        let p0 = [x0, 0.0, -2.0 + h_delivered]; // = [x0, 0, -1.70]
        let end = [x0, 0.0, p0[2] - 2.0 * r_ball];
        let centre = [x0, 0.0, (p0[2] + end[2]) * 0.5];

        let tip = SynthRecord::new(RecordKind::ContactSphere, p0, end, r_ball, r_ball)
            .with_soup(drop_zero_area(sphere_soup(centre, r_ball, 8, 16)));

        // TAPER: start = tip's connective point (ball CENTRE, per graph.rs),
        // narrow end = r_tip (matches r_tip_A/r_tip_C for the three-way
        // check), widening toward the member radius.
        let taper_end = [x0, 0.0, -4.2];
        let arm_r = 0.8_f64;
        let taper = SynthRecord::new(RecordKind::ContactTaper, centre, taper_end, r_tip, arm_r)
            .with_declared_depth(h_delivered)
            .with_matching_soup();

        // MEMBER: constant-radius arm from the taper's wide end down to the
        // plate. z_plate = mesh.bbox.min.z - translate_z = -2 - 5.03 = -7.03.
        // translate_z is deliberately NOT a multiple of layer_height (0.05mm)
        // so the model's entry plane (z = -2.0, which sits exactly
        // translate_z above z_plate) does not land on a raster layer
        // boundary -- see the coincidence-handling note in contact_area.rs.
        let member_end = [x0, 0.0, -7.03];
        let member = SynthRecord::new(RecordKind::Member, taper_end, member_end, arm_r, arm_r)
            .with_matching_soup();

        // RAFT pad: connective point exactly at the member's end, so the
        // join graph closes. `SynthRecord::with_matching_soup` does not cover
        // RaftPadOrBridge (it returns an empty soup for that kind, per
        // synth.rs's own doc comment) -- an empty soup is itself
        // GEOM_DEGENERATE, so the soup is built by hand here from the SAME
        // cone the audit's own `Primitive::RaftPad::signed_distance` checks
        // it against (`base=start, r=r_start*0.5, top=base+[0,0,r_end]`):
        // a vertical cylinder is a cone with equal end radii.
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

    let mut records = column(-1.25);
    records.extend(column(1.25));

    let file = SynthFile::new().with_model(SynthModel {
        name: "double_load".into(),
        translate_mm: [0.0, 0.0, 5.03],
        mesh,
        records,
    });

    let out = std::env::args()
        .nth(1)
        .expect("usage: gen_double_load <out.chitubox>");
    file.write(std::path::Path::new(&out)).expect("write");
    eprintln!("wrote {out}");
}
