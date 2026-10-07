//! **A tip at a corner seats on the face whose overhang it holds up, never on
//! the wall beside it** (orchestrator ruling 2026-09-30,
//! `tip_landing::SeatFaceRule`).
//!
//! # The defect
//!
//! `tip_landing::tip_seat_faces` gave each tip the nearest face with ANY
//! downward component (`n_z < 0`). A vertical wall that a rotation tips a hair
//! downward passes that test, so a tip standing beside a wall and just under
//! the edge of the underside it holds took the WALL as its seat face. On the
//! recessed Round 25 fixture at the flip a leaned tip at the ring's inner
//! corner took the recess wall (`n_z = -5.2e-17`) and the seat trim cut its
//! ball against a vertical plane, leaving a 0.416 mm cap above the ring
//! underside (`plinth-pipeline tip_trim::round_25_at_the_flip…`). The lane
//! models' finished plans had 8 Morel members on faces 1-31° off vertical and
//! 59 Cylinder members beside a nearer wall or steep face.
//!
//! # The rule
//!
//! A seat face is a face the registered overhang law weighs as an overhang:
//! shallower than the steep end of its 10-to-90 band, `centre + transition/2`
//! from horizontal (57.5° at the registered 45° and 25°). A mesh with no such
//! face keeps every down-facing face and says so (emit and warn).
//!
//! # How it is graded
//!
//! | test | what it asserts | anti-vacuity |
//! |---|---|---|
//! | `a_tip_beside_a_tipped_wall_seats_on_the_underside_it_holds` | the carried face is the underside, not the wall a 1 mrad tilt made down-facing | the SAME tip under the pre-ruling rule (`any_down_facing`) takes the wall, and the counted call names one passed-over face |
//! | `the_band_is_read_from_the_registered_angles` | the production rule's limit is `-cos(centre + transition/2)` from the registry | — |
//! | `a_model_with_no_overhang_face_still_seats_every_tip` | a steep-sided model keeps a face for every tip and reports the fall-back | the fixture has no admitted face, by construction |
//!
//! Sabotage: build the production rule as `any_down_facing()` in `run_sweep`
//! and `plinth-pipeline tip_trim::round_25_at_the_flip…` goes red again.

use plinth_core::m0_constants;
use plinth_mesh::Mesh;
use plinth_sweep::grid::lay_out_at;
use plinth_sweep::tip_landing::{tip_seat_faces, tip_seat_faces_counted, SeatFaceRule};
use plinth_sweep::{Landing, SeatFace, StrutKind, SweepStrut};

const PITCH_MM: f64 = 0.05;
const LAYER_MM: f64 = 0.05;

fn registered_rule() -> SeatFaceRule {
    let constants = m0_constants().expect("registry");
    let placeholders = constants.placeholders();
    SeatFaceRule::from_overhang_band(
        placeholders.overhang_angle_deg().diagnostic().get(),
        placeholders
            .overhang_weight_transition_deg()
            .diagnostic()
            .get(),
    )
}

/// One straight arm at `cell`, its tip at layer `k_top`.
fn arm(cell: usize, k_top: usize) -> SweepStrut {
    SweepStrut {
        id: 0,
        cell,
        center_mm: [0.0, 0.0],
        kind: StrutKind::Arm,
        min_radius_mm: 0.0,
        thickened_mm: Vec::new(),
        seat_on_strut: None,
        k_top,
        k_bottom: 0,
        landing: Landing::Plate,
        interior: false,
        radius_mm: vec![0.1; k_top],
        carried_mm2: vec![0.0; k_top],
        covered_demand: Vec::new(),
        inserted_by_load: false,
        path: Vec::new(),
        straight_fallback: None,
        fallback_path: Vec::new(),
        head: None,
        seat_face: SeatFace::default(),
        seat_normal: [0.0; 3],
    }
}

/// The closed box `[0, 10] × [0, 10] × [1, 2]`, rotated about X by `tilt`
/// radians: its `y = 0` wall's outward normal becomes `(0, -cos t, -sin t)`,
/// down-facing by `sin t`, and its underside `(0, sin t, -cos t)`.
fn tipped_box(tilt: f64) -> Mesh {
    let (s, c) = tilt.sin_cos();
    let rotate = |[x, y, z]: [f64; 3]| [x, c * y - s * z, s * y + c * z];
    let mut verts = Vec::with_capacity(8);
    for z in [1.0, 2.0] {
        for y in [0.0, 10.0] {
            for x in [0.0, 10.0] {
                verts.push(rotate([x, y, z]));
            }
        }
    }
    // Index = 4·(z is top) + 2·(y is far) + (x is far); outward winding.
    let tris = vec![
        // underside z = 1, normal -z
        [0, 2, 3],
        [0, 3, 1],
        // top z = 2, normal +z
        [4, 5, 7],
        [4, 7, 6],
        // y = 0 wall, normal -y
        [0, 1, 5],
        [0, 5, 4],
        // y = 10 wall, normal +y
        [2, 6, 7],
        [2, 7, 3],
        // x = 0 wall, normal -x
        [0, 4, 6],
        [0, 6, 2],
        // x = 10 wall, normal +x
        [1, 3, 7],
        [1, 7, 5],
    ];
    Mesh::new(verts, tris).expect("a closed box")
}

#[test]
fn a_tip_beside_a_tipped_wall_seats_on_the_underside_it_holds() {
    // A wall tipped 1 mrad downward: the order of the dense orientation's
    // residual tilt on the real Round 50 (n_z = -3.3e-3 measured on its walls).
    let tilt = 1.0e-3;
    let mesh = tipped_box(tilt);
    // One tip 0.175 mm outside the y = 0 wall and 0.075 mm above the
    // underside's plane, beside the wall: the wall is 0.175 mm away, the
    // underside's edge 0.190 mm. The tip holds the underside; the wall is only
    // nearer.
    let grid = lay_out_at(-2.0, -2.0, PITCH_MM, 200, 100);
    let cell = grid.index(36, 100); // x = 3.025 mm, y = -0.175 mm
    let k_top = 21; // z = (21 + 0.5) · 0.05 = 1.075 mm
    let struts = vec![arm(cell, k_top)];

    let (faces, hits) =
        tip_seat_faces_counted(&mesh, &grid, &struts, 0.0, LAYER_MM, registered_rule());
    let (normal, point) = faces[0].expect("the tip carries a face");
    println!(
        "carried face normal {normal:?} at {point:?}; hits {hits:?}; wall normal z {:.3e}",
        -tilt.sin()
    );
    assert!(
        normal[2] < -0.999,
        "the tip must carry the underside it holds (n_z ≈ -1), not the tipped wall; got {normal:?}"
    );
    assert_eq!(hits.tips, 1);
    assert_eq!(
        hits.steeper_face_passed_over, 1,
        "the rule must be seen to fire: the wall was the nearer down-facing face"
    );
    assert!(!hits.fallback_any_down_facing);

    // The same tip under the rule before the ruling takes the wall: the
    // fixture really is a corner the old rule got wrong.
    let before = tip_seat_faces(
        &mesh,
        &grid,
        &struts,
        0.0,
        LAYER_MM,
        SeatFaceRule::any_down_facing(),
    );
    let (wall, _) = before[0].expect("the old rule also finds a face");
    assert!(
        wall[2] < 0.0 && wall[2] > -0.01 && wall[1] < -0.99,
        "under the pre-ruling rule the nearest down-facing face is the tipped wall; got {wall:?}"
    );
}

#[test]
fn the_band_is_read_from_the_registered_angles() {
    let constants = m0_constants().expect("registry");
    let placeholders = constants.placeholders();
    let centre = placeholders.overhang_angle_deg().diagnostic().get();
    let width = placeholders
        .overhang_weight_transition_deg()
        .diagnostic()
        .get();
    let expected = -(centre + 0.5 * width).to_radians().cos();
    assert!((registered_rule().max_normal_z() - expected).abs() < 1e-15);
    // At the registered 45° and 25°: 57.5° from horizontal.
    assert!(
        (expected + 57.5_f64.to_radians().cos()).abs() < 1e-12,
        "{expected}"
    );
    // A nonsense band does not refuse; it degrades to the old rule.
    assert_eq!(
        SeatFaceRule::from_overhang_band(f64::NAN, 25.0),
        SeatFaceRule::any_down_facing()
    );
    assert_eq!(
        SeatFaceRule::from_overhang_band(80.0, 25.0),
        SeatFaceRule::any_down_facing()
    );
}

#[test]
fn a_model_with_no_overhang_face_still_seats_every_tip() {
    // An upside-down square pyramid, apex at z = 1, top at z = 5, half-width 1:
    // its four sides are 76° from horizontal (n_z = -0.24), steeper than the
    // band, and its only other face points up.
    let verts = vec![
        [0.0, 0.0, 1.0],
        [-1.0, -1.0, 5.0],
        [1.0, -1.0, 5.0],
        [1.0, 1.0, 5.0],
        [-1.0, 1.0, 5.0],
    ];
    let tris = vec![
        [0, 2, 1],
        [0, 3, 2],
        [0, 4, 3],
        [0, 1, 4],
        [1, 2, 3],
        [1, 3, 4],
    ];
    let mesh = Mesh::new(verts, tris).expect("a closed pyramid");
    let grid = lay_out_at(-2.0, -2.0, PITCH_MM, 80, 80);
    let cell = grid.index(40, 40); // under the apex
    let struts = vec![arm(cell, 15)];
    let (faces, hits) =
        tip_seat_faces_counted(&mesh, &grid, &struts, 0.0, LAYER_MM, registered_rule());
    let (normal, _) = faces[0].expect("emit and warn: the tip still carries a face");
    assert!(
        normal[2] < 0.0 && normal[2] > -0.5,
        "a steep side; got {normal:?}"
    );
    assert!(
        hits.fallback_any_down_facing,
        "the fall-back must be reported: {hits:?}"
    );
    assert_eq!(hits.tips, 1);
}
