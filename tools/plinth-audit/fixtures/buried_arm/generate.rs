//! Generator for `buried_arm.chitubox`. Not built by this crate's own
//! `Cargo.toml` (there is no `[[bin]]` entry for it) -- it is a record of how
//! the fixture was produced, in the same spirit as
//! `fixtures/double_load/generate.rs` and `fixtures/starved_patch/generate.rs`.
//! To regenerate: copy this file into a throwaway crate with `plinth-audit`
//! as a path dependency (see the fixture's README, "How this was verified")
//! and run it with one argument, the output path.
//!
//! Two independent support chains on one thin shared wall:
//!
//! * **Chain A** (x = -2): a TIP driven clean through the wall -- its own
//!   `p0` sits 0.1 mm past the FAR face, reproducing
//!   `checks/intersection.rs`'s own bundled unit test
//!   `a_tip_driven_clean_through_a_thin_wall_fires_the_far_side_clause`
//!   (`punched_file(wall_half=0.1, p0_z=0.2)`) byte-for-byte in its numbers,
//!   translated sideways. Plants `INTERSECT_FAR_SIDE_BREACH`.
//! * **Chain B** (x = 2): a TIP seated healthily and shallowly (h_delivered
//!   = 0.15 mm > h_required = 0.11459 mm -- I1 PASSES for this contact), but
//!   its own ARM turns back and drives through the same wall a second time,
//!   1.6 mm away from its own seat's zone, before escaping sideways and
//!   descending to the plate. Plants `INTERSECT_OUTSIDE_SEAT` on that MEMBER
//!   record.

use plinth_audit::consts as c;
use plinth_audit::reader::RecordKind;
use plinth_audit::synth::{box_soup, frustum_soup, sphere_soup, SynthFile, SynthModel, SynthRecord};

/// Drop the zero-area pole triangles `synth::sphere_soup` emits -- a
/// pre-existing, documented property of the shared test helper (both poles
/// share a vertex; see `fixtures/double_load/generate.rs`'s own copy of this
/// filter and its comment), not something this fixture owns.
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

/// A raft pad's soup, built as a cylinder the same way
/// `fixtures/double_load/generate.rs` does: `with_matching_soup` returns an
/// empty soup for `RaftPadOrBridge` by its own documented contract, and an
/// empty soup is itself `GEOM_DEGENERATE`.
fn raft_soup(pad: [f64; 3]) -> Vec<[[f32; 3]; 3]> {
    let r = c::RAFT_PAD_DIAMETER_MM * 0.5;
    let top = [pad[0], pad[1], pad[2] + c::RAFT_PAD_HEIGHT_MM];
    frustum_soup(pad, r, top, r, 20)
}

fn main() {
    let f = c::BALL_FACTOR_STUB; // 1.5

    // ---- the shared wall --------------------------------------------------
    // x in [-4,4], y in [-2,2], z in [-0.1,0.1]: a thin, wide slab, 0.2 mm
    // thick -- the same thickness as `punched_file`'s `wall_half=0.1` case.
    let wall_half_z = 0.1_f64;
    let mesh = box_soup([-4.0, -2.0, -wall_half_z], [4.0, 2.0, wall_half_z]);

    // pad_z is chosen first and translate_z is solved backward from it, the
    // same trick `checks/intersection.rs`'s own `punched_file` test uses, so
    // both chains' rafts sit exactly on the reconstructed plate:
    //   z_plate = mesh.bbox.min.z - translate_z = -wall_half_z - translate_z
    // Setting translate_z = -wall_half_z - pad_z makes z_plate == pad_z.
    let pad_z = -5.1_f64;
    let translate_z = -wall_half_z - pad_z; // = 5.0

    let mut records = Vec::new();

    // ---- Chain A: tip driven clean through the wall (I6 clause b) --------
    {
        let r_ball = 0.3_f64;
        let r_tip = r_ball / f; // 0.2
        let p0 = [-2.0, 0.0, 0.2]; // 0.1 mm PAST the wall's far face (z=0.1)
        let end = [-2.0, 0.0, p0[2] - 2.0 * r_ball];
        let centre = [-2.0, 0.0, p0[2] - r_ball];

        let tip = SynthRecord::new(RecordKind::ContactSphere, p0, end, r_ball, r_ball)
            .with_soup(drop_zero_area(sphere_soup(centre, r_ball, 8, 16)));
        // h_delivered reconstructs to exactly 0.0 (p0 is outside the model),
        // so the declared depth is set to match it exactly -- this isolates
        // the target codes from SEAT_DEPTH_DECLARED_MISMATCH, the same
        // discipline `fixtures/shallow_seat` uses for its own zero-depth
        // chain.
        let taper_end = [-2.0, 0.0, centre[2] - 0.9];
        let taper = SynthRecord::new(RecordKind::ContactTaper, centre, taper_end, r_tip, 0.35)
            .with_declared_depth(0.0)
            .with_matching_soup();
        let member_end = [-2.0, 0.0, taper_end[2] - 3.0];
        let member = SynthRecord::new(RecordKind::Member, taper_end, member_end, 0.35, 0.35)
            .with_matching_soup();
        let flare_end = [-2.0, 0.0, member_end[2] - 0.9];
        let flare = SynthRecord::new(RecordKind::RootFlare, member_end, flare_end, 0.35, 1.1)
            .with_matching_soup();
        let pad = [-2.0, 0.0, pad_z];
        let raft = SynthRecord::new(
            RecordKind::RaftPadOrBridge,
            pad,
            pad,
            c::RAFT_PAD_DIAMETER_MM,
            c::RAFT_PAD_HEIGHT_MM,
        )
        .as_pad()
        .with_soup(raft_soup(pad));
        records.extend([tip, taper, member, flare, raft]);
    }

    // ---- Chain B: healthy seat, arm re-buried away from it (I6 clause a) -
    {
        let r_ball = 0.45_f64;
        let r_tip = r_ball / f; // 0.3
        let h_delivered = 0.15_f64; // > h_required = 0.11459 -- I1 PASSES
        let p0 = [2.0, 0.0, -wall_half_z + h_delivered]; // = [2,0,0.05], inside the wall
        let end = [2.0, 0.0, p0[2] - 2.0 * r_ball];
        let centre = [2.0, 0.0, p0[2] - r_ball];

        let tip = SynthRecord::new(RecordKind::ContactSphere, p0, end, r_ball, r_ball)
            .with_soup(drop_zero_area(sphere_soup(centre, r_ball, 8, 16)));
        let taper_end = [2.0, 0.0, centre[2] - 0.9];
        let taper = SynthRecord::new(RecordKind::ContactTaper, centre, taper_end, r_tip, 0.35)
            .with_declared_depth(h_delivered)
            .with_matching_soup();

        // The arm: instead of retreating from the wall, it turns back and
        // drives straight through it a second time -- 1.6 mm from the tip's
        // own axis, well clear of chain B's own seat zone -- then escapes
        // sideways at z=0.6 (comfortably more than one radius, 0.35 mm,
        // above the wall's top face at 0.1, so the TUBE clears the slab, not
        // just its centreline) and descends to the plate at x=10, far
        // outside the wall's x in [-4,4] footprint.
        let m1_end = [3.0, 0.0, 0.6];
        let member1 = SynthRecord::new(RecordKind::Member, taper_end, m1_end, 0.35, 0.35)
            .with_matching_soup();
        let m1b_end = [10.0, 0.0, 0.6];
        let member1b = SynthRecord::new(RecordKind::Member, m1_end, m1b_end, 0.35, 0.35)
            .with_matching_soup();
        let m2_end = [10.0, 0.0, -4.0];
        let member2 = SynthRecord::new(RecordKind::Member, m1b_end, m2_end, 0.35, 0.35)
            .with_matching_soup();
        let flare_end = [10.0, 0.0, -4.9];
        let flare = SynthRecord::new(RecordKind::RootFlare, m2_end, flare_end, 0.35, 1.1)
            .with_matching_soup();
        let pad = [10.0, 0.0, pad_z];
        let raft = SynthRecord::new(
            RecordKind::RaftPadOrBridge,
            pad,
            pad,
            c::RAFT_PAD_DIAMETER_MM,
            c::RAFT_PAD_HEIGHT_MM,
        )
        .as_pad()
        .with_soup(raft_soup(pad));
        records.extend([tip, taper, member1, member1b, member2, flare, raft]);
    }

    let file = SynthFile::new().with_model(SynthModel {
        name: "buried_arm".into(),
        translate_mm: [0.0, 0.0, translate_z],
        mesh,
        records,
    });

    let out = std::env::args()
        .nth(1)
        .expect("usage: gen_buried_arm <out.chitubox>");
    file.write(std::path::Path::new(&out)).expect("write");
    eprintln!("wrote {out}");
}
