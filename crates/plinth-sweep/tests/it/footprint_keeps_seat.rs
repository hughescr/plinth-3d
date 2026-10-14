//! **A footprint slide keeps the seat it was placed for** (joint review P2,
//! 2026-10-12).
//!
//! # The defect
//!
//! The footprint rule slides a seat whose contact disc hangs off its face to
//! the nearest seat whose whole disc is on the face. On a TILTED face a slide
//! up the slope is also a slide up the layers: Deinonychus's ball 310 slid
//! 0.79 mm up its face onto a whole-disc contact, and the 829 + 1356 pixels
//! it had held at 7.575 and 7.625 mm printed over nothing.
//!
//! # The fixture
//!
//! A slab tilted 25 degrees about Y: its underside is one tilted face, cured
//! lamina by lamina up the slope, and the tips placed on it are each placed for
//! the lamina at their seat.
//!
//! # How it is graded
//!
//! On the production sweep, as geometry: every graded tip's EMITTED ball (the
//! arms the file gets, the disc by the lowering's own derivation) must still
//! meet the face within one lamina of its ORIGINAL seat's height (the arms with
//! every footprint move taken back). The ball meets the face plane in a disc,
//! and that disc spans a band of heights on the tilted face. The path hit is
//! the refusal count. Sabotage: accept every on-face slide (the pre-fix rule).

use plinth_core::{m0_constants, LayerPlan};
use plinth_mesh::Mesh;
use plinth_sweep::{arms_of, Footprint, SweepPolicy};

const LIFT_MM: f64 = 5.0;
const TILT_DEG: f64 = 25.0;

fn layer_height_mm() -> f64 {
    *m0_constants()
        .expect("registry")
        .evidence()
        .layer_height_default_mm()
        .value()
}

/// A 6 x 4 x 1.5 mm slab turned `TILT_DEG` about Y.
fn tilted_slab() -> Mesh {
    let mesh = plinth_mesh::make_box([6.0, 4.0, 1.5], [0.0, 0.0, 0.0]).expect("a box");
    let (sin, cos) = TILT_DEG.to_radians().sin_cos();
    let verts = mesh
        .verts()
        .iter()
        .map(|v| [cos * v[0] + sin * v[2], v[1], -sin * v[0] + cos * v[2]])
        .collect();
    Mesh::new(verts, mesh.tris().to_vec()).expect("a rigid rotation preserves a valid mesh")
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a layer count is a finite positive quotient of model-scale lengths"
)]
fn layers_for(mesh: &Mesh) -> LayerPlan {
    let (mut min_z, mut max_z) = (f64::INFINITY, f64::NEG_INFINITY);
    for vertex in mesh.verts() {
        min_z = min_z.min(vertex[2]);
        max_z = max_z.max(vertex[2]);
    }
    let plate_z = min_z - LIFT_MM;
    let height = layer_height_mm();
    let count = (((max_z - plate_z) / height).ceil() as usize).saturating_add(1);
    LayerPlan::new(plate_z, vec![height; count]).expect("a uniform layer plan")
}

#[test]
fn a_slide_up_a_tilted_face_never_leaves_the_lamina_its_tip_was_placed_for() {
    let mesh = tilted_slab();
    let layers = layers_for(&mesh);
    let height = layer_height_mm();
    let policy = SweepPolicy::from_registry().expect("the registry supplies every sweep number");
    let plan = plinth_sweep::run_sweep(&mesh, &layers, &policy)
        .expect("the sweep emits and warns; it never refuses");
    let footprint = &plan.report.seat_footprint;
    // The underside: outward normal (-sin, 0, -cos), through the low edge.
    let (sin, cos) = TILT_DEG.to_radians().sin_cos();
    let outward = [-sin, 0.0, -cos];
    let low = mesh
        .verts()
        .iter()
        .copied()
        .min_by(|a, b| a[2].total_cmp(&b[2]))
        .expect("a vertex");
    let after = arms_of(&plan);
    let mut unmoved = plan.clone();
    for strut in &mut unmoved.struts {
        if strut.seat_face.footprint == Footprint::MovedOntoFace {
            strut.seat_face.footprint = Footprint::OnFace;
        }
    }
    let before = arms_of(&unmoved);
    let discs = plinth_support::tip_contact_discs(&after, &layers).expect("discs");
    let mut graded = 0;
    let mut moved = 0;
    let mut left: Vec<(usize, f64, [f64; 2])> = Vec::new();
    for (index, disc) in discs.iter().enumerate() {
        let Some(disc) = disc else { continue };
        if plan.struts[index].seat_face.footprint == Footprint::NotGraded {
            continue;
        }
        graded += 1;
        moved += usize::from(plan.struts[index].seat_face.footprint == Footprint::MovedOntoFace);
        // The emitted ball meets the face plane in a disc of radius rho about
        // the centre's foot; on a face tilted by TILT_DEG it spans heights
        // foot_z +- rho sin.
        let c = disc.ball_centre_mm;
        let d = geom_kernel::vec3::dot(geom_kernel::vec3::sub(c, low), outward);
        let rho = (disc.ball_radius_mm * disc.ball_radius_mm - d * d).max(0.0).sqrt();
        let foot_z = c[2] - d * outward[2];
        let span = [foot_z - rho * sin, foot_z + rho * sin];
        let seat_z = before[index].seat_mm[2];
        if span[1] < seat_z - height || span[0] > seat_z + height {
            left.push((index, seat_z, span));
        }
    }
    println!(
        "tilted slab: {graded} tips graded, {} on face, {moved} moved (longest {:.4} mm), {} knife \
         edges, {} kept by a refused move, {} candidates refused for leaving their seat; tips \
         whose ball left their lamina (strut, seat z, ball's span of face heights): {left:?}",
        footprint.tips_on_face,
        footprint.max_move_mm,
        footprint.knife_edges.len(),
        footprint.moves_refused.len(),
        footprint.moves_refused_off_seat,
    );
    assert!(graded > 0, "no tip was graded");
    assert!(
        footprint.moves_refused_off_seat > 0,
        "no slide left its seat, so the rule is graded on nothing"
    );
    assert!(
        left.is_empty(),
        "tips whose emitted ball no longer meets the lamina they were placed for: {left:?}"
    );
}
