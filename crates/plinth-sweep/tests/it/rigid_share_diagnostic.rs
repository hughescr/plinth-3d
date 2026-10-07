//! **The rigid-body reading of the load share is a WARNING and never sizes**
//! (owner ruling 2026-09-29;
//! `plinth-evidence/slice-sweep-2026-09-02/field-2026-09-28/cylinder-load-sharing.md`, §7).
//!
//! Its witness is the laid tube at its production pose (a turn of −22.5°
//! about Y): the short, stiff arms at the tube's lowest end attract a rigid
//! share above `ℓ_max` that column routing does not give them, and the report
//! must name exactly those two. A field with no such arm reads "ran, 0 above
//! the limit", which is what tells "no warning" apart from "never ran".

use crate::harness;

use harness::sweep_with;
use plinth_mesh::{make_laid_tube, Mesh};
use plinth_sweep::SweepPolicy;

/// A rigid rotation about Y, `degrees` counter-clockwise.
fn turn_about_y(mesh: &Mesh, degrees: f64) -> Mesh {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let verts = mesh
        .verts()
        .iter()
        .map(|v| [v[0] * cos + v[2] * sin, v[1], -v[0] * sin + v[2] * cos])
        .collect();
    Mesh::new(verts, mesh.tris().to_vec()).expect("a rigid rotation preserves a valid mesh")
}

#[test]
fn the_rigid_share_names_the_laid_tubes_stiff_low_end_arms_and_sizes_nothing() {
    let mesh = turn_about_y(&make_laid_tube().expect("the laid tube fixture"), -22.5);
    // Graded at the seat placement the fixture was built for
    // (`harness::at_path_fixture_seating`): the witness below is "the tube's
    // two stiff low-end arms" of the field the old placement gave it. At the
    // production placement the tilted tube is seated differently and the
    // diagnostic names 5 arms (measured 2026-09-30); it still sizes nothing.
    // The diagnostic reads no placement.
    //
    // Graded also on the seat faces from before the seat-face band
    // (`without_seat_face_band`), as `head_join.rs` grades its pre-band
    // witness. The band (lattice-align `b28f15c`) moves which face some tips
    // carry; that shifts the tube's load just enough that arm 18, the second
    // low-end arm, falls under `ℓ_max`. Bisected at the round-8 milestone
    // (2026-10-01, `lanes/round8-milestone/`): at `2e09334` the report named
    // arm 17 (59.34 mm2) and arm 18 (57.27 mm2, 1.3 % over `ℓ_max`); from
    // `b28f15c` on, with the band, it names arm 17 alone (58.59 mm2 on develop
    // `16952e5`). On `16952e5` without the band it names arm 17 (59.22 mm2) and
    // arm 18 (58.08 mm2) again. The diagnostic reads no seat face either; the
    // band only decides whether this fixture reaches its two-arm witness.
    //
    // And without the drawn-solid descent (2026-10-04, fix/drawn-solid-clearance,
    // round 3): its zones are now each face's own, so the tilted tube's descents
    // next to the legs differ from the longest-zone read of round 2, and the
    // load moves by the same margin again (measured: the report names arm 18
    // alone, 59.91 mm2, with the descent on and the re-route off or on; arm 17
    // falls under `ℓ_max`). The diagnostic reads no clearance.
    //
    // And on the seat triangle's own normal, without the footprint normal
    // (Craig ruling 2026-10-05, `footprint_normal`, merged at `f94710e`). The
    // area-weighted footprint normal turns some tips' seat normals, which moves
    // the tube's load by the same margin once more. Measured at the round-17
    // milestone (2026-10-06, `lanes/round17-milestone/`): on `f94710e` with the
    // footprint normal the report names arm 17 alone (59.45 mm2); arm 18 falls
    // under `ℓ_max`. With it off (and the switches above) it names arm 17
    // (59.30 mm2) and arm 18 (57.62 mm2) again. The diagnostic reads no seat
    // normal.
    let policy =
        harness::at_path_fixture_seating(SweepPolicy::from_registry().expect("the registry"))
            .without_seat_face_band()
            .without_drawn_solid_descent()
            .without_clearance_reroute()
            .without_footprint_normal()
            .without_heads();
    let plan = sweep_with(&mesh, 0.0, &policy);
    let rigid = &plan.report.rigid_share;
    println!(
        "laid tube at -22.5 deg: rigid share ran {}, {} events, {} members, heaviest {:.4} mm2 on \
         arm {}, compression up to {:.4} mm2, residual {:.3e}; above the limit: {:?}",
        rigid.ran,
        rigid.events_solved,
        rigid.members,
        rigid.max_share_mm2,
        rigid.max_share_id,
        rigid.compressive_max_mm2,
        rigid.conservation_residual,
        rigid.over_limit,
    );
    assert!(
        rigid.ran && rigid.events_solved > 0,
        "the diagnostic never ran"
    );
    assert!(
        rigid.conservation_residual <= 1.0e-6,
        "the rigid solve does not conserve each event: residual {:.3e}",
        rigid.conservation_residual
    );
    assert_eq!(
        rigid.over_limit.len(),
        2,
        "the witness is the tube's two stiff low-end arms; the report named {:?}",
        rigid.over_limit
    );
    let l_max = harness::x_allowed() * core::f64::consts::PI * policy.tip_radius_max_mm.powi(2);
    for warning in &rigid.over_limit {
        let strut = &plan.struts[warning.strut_id as usize];
        assert!(
            warning.share_mm2 > l_max,
            "a warning below the limit: {warning:?}"
        );
        // It sizes NOTHING: the arm keeps its per-event ledger.
        assert!(
            (strut.carried_mm2[0] - warning.sized_mm2).abs() < 1.0e-12
                && strut.carried_mm2[0] <= l_max,
            "arm {} was sized from the rigid share ({} mm2 at its root)",
            strut.id,
            strut.carried_mm2[0]
        );
    }
    // And the orientation search's entry never runs it (no score reads it).
    let search = plinth_sweep::run_sweep(
        &mesh,
        &plinth_core::LayerPlan::new(
            harness::z_span(&mesh).0 - harness::LIFT_MM,
            vec![harness::layer_height_mm(); plan.layers],
        )
        .expect("a uniform layer plan"),
        &policy,
    )
    .expect("the sweep emits");
    assert!(
        !search.report.rigid_share.ran,
        "the search's pricing sweep ran the diagnostic"
    );
}
