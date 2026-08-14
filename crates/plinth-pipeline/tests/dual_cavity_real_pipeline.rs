//! **The E1 dual-cavity fixture, measured after the stages production
//! actually runs.**
//!
//! `plinth-hollow`'s own `dual_cavity_doctrine` module measures the fixture on
//! its raw, lattice-phase-exact mesh — which is what makes its throat
//! clearances *exact* equalities, and which is also exactly what a reviewer
//! can call a rigged bench: the 37-mini rerun that motivated this fixture ran
//! `repair -> decimate -> dense orientation sweep` first, and both of those
//! stages move the model onto an arbitrary lattice phase.
//!
//! So this file runs the real chain —
//! [`plinth_mesh::repair_mesh`] -> [`plinth_decimate::run`] (registered
//! decimation budget) -> [`plinth_orient::search_and_orient`] (registered
//! dense sweep, production work limits) -> `plinth_hollow::create_shell` ->
//! `plan_drains` -> `drill_planned_shell`, every one of them under
//! **genuinely unmodified** [`H0AdmissionLimits::production()`] — and asserts
//! the *doctrine* rather than the phase-dependent numbers:
//!
//! * every registered throat setting still hollows and still clears the
//!   registered savings gate, recomputed from the candidate's own evidence;
//! * ABOVE_BORE selects **one** resin-flow interior that spans **both**
//!   chambers, proved by a rotation-invariant measurement (the cavity's own
//!   radius about its centroid) rather than by a bare "bigger than" — after
//!   orientation there is no "up" left to appeal to;
//! * AT_BORE and SEPARATED select the lower chamber **alone**, by the same
//!   rotation-invariant measurement;
//! * the planner still places exactly one drain and one vent, drilling still
//!   succeeds (its own `BlockedOpening` check is the penetration proof), and
//!   the drilled shell survives a write/re-read round trip as a closed,
//!   oriented solid.
//!
//! Nothing here is fail-open: every stage's failure is an assertion failure.

use std::path::PathBuf;

use plinth_hollow::drains::{plan_drains, HoleRole};
use plinth_hollow::drilling::drill_planned_shell;
use plinth_hollow::{create_shell, H0AdmissionLimits, HollowRequest, HollowSettings};
use plinth_mesh::{
    make_dual_cavity_pawn_with_throat_radius, DUAL_CAVITY_CAVITY_THROAT_ABOVE_BORE_MM,
    DUAL_CAVITY_CAVITY_THROAT_AT_BORE_MM, DUAL_CAVITY_THROAT_ABOVE_BORE_MM,
    DUAL_CAVITY_THROAT_SEPARATED_MM, DUAL_CAVITY_UPPER_SPHERE_R_MM,
};
use plinth_pipeline::prepare::registered_decimation_budget;
use plinth_pipeline::PrepareResourcePolicy;

/// The registered erosion `t = HOLLOW_WALL_DEFAULT_MM +
/// HOLLOW_CAVITY_MARCH_MARGIN_MM`, read live off the settings rather than
/// restated as a literal.
fn registered_erosion_mm(settings: HollowSettings) -> f64 {
    settings.wall_thickness_mm().get() + settings.cavity_march_margin_mm().get()
}

/// The registered settings `plinth prepare` itself builds, via the Saturn 4
/// Ultra printer profile and the registered wall default.
fn registered_settings() -> HollowSettings {
    let constants = plinth_core::m0_constants().expect("registered M0 constants");
    let request = HollowRequest::with_registered_wall_default(constants.saturn4_ultra().clone())
        .expect("registered wall default");
    HollowSettings::from_request(&request).expect("valid hollow settings")
}

/// The fixture, through `repair -> decimate -> dense orientation sweep` —
/// the exact three stages `plinth prepare` runs before hollowing, with the
/// registered budgets and the production work limits.
fn through_the_real_preprocessing(throat_radius_mm: f64) -> plinth_mesh::Mesh {
    let source =
        make_dual_cavity_pawn_with_throat_radius(throat_radius_mm).expect("valid throat radius");
    let (repaired, _evidence) = plinth_mesh::repair_mesh(&source).into_parts();

    let budget = registered_decimation_budget().expect("registered decimation budget");
    let policy =
        PrepareResourcePolicy::for_saturn4_ultra(budget).expect("saturn4-ultra resource policy");
    let decimated = plinth_decimate::run(&repaired, policy.decimation())
        .expect("decimation run")
        .into_decimated_model()
        .expect("the fixture is not scale-suspicious");

    let mut sweep = plinth_orient::registered_sweep_request().expect("registered sweep request");
    sweep.limits = policy.orient();
    let oriented = plinth_orient::search_and_orient(decimated, &sweep)
        .expect("the registered dense sweep finds an orientation for this fixture");
    let (mesh, _history) = oriented.into_parts();
    mesh
}

/// The same fixture, deliberately shifted **off** the 0.5 mm voxel lattice by
/// a quarter pitch on all three axes.
///
/// The orientation sweep already lands the model on an arbitrary phase (it
/// picks a tilted pose for this silhouette), but that is an incidental
/// property of today's scoring, not something a test should depend on. This
/// translation is the direct, deterministic version of the same hostility:
/// after it, `grid_spec_from_bounds` cannot anchor a lattice sample on the
/// model's axis, so every exact clearance equality
/// `plinth-hollow`'s `dual_cavity_doctrine` pins is unavailable — and the
/// doctrine must survive anyway.
fn shifted_off_the_lattice(throat_radius_mm: f64) -> plinth_mesh::Mesh {
    let source =
        make_dual_cavity_pawn_with_throat_radius(throat_radius_mm).expect("valid throat radius");
    let offset = 0.25;
    let verts = source
        .verts()
        .iter()
        .map(|point| [point[0] + offset, point[1] + offset, point[2] + offset])
        .collect();
    plinth_mesh::Mesh::new(verts, source.tris().to_vec()).expect("a translated mesh is still valid")
}

/// The clearance that separates "a chamber" from "the throat, and whatever
/// dead space a flood legitimately claimed inside it", for
/// [`plinth_hollow::CavityField::deep_lobe_count`].
///
/// Derived, never picked: the widest cavity-side throat any registered
/// setting uses is [`DUAL_CAVITY_CAVITY_THROAT_ABOVE_BORE_MM`] = 2.45 mm, and
/// the *smaller* of the two chambers has cavity radius
/// `DUAL_CAVITY_UPPER_SPHERE_R_MM - t` = 4.634 mm. Anything strictly between
/// them counts chambers exactly; the midpoint is the value farthest from both
/// mistakes.
fn chamber_clearance_mm(settings: HollowSettings) -> f64 {
    let smaller_chamber = DUAL_CAVITY_UPPER_SPHERE_R_MM - registered_erosion_mm(settings);
    0.5 * (DUAL_CAVITY_CAVITY_THROAT_ABOVE_BORE_MM + smaller_chamber)
}

/// Hollow, plan, and drill the mesh under production limits, asserting each
/// stage rather than reporting it — then prove the drilled shell survives a
/// write and a re-read from disk as a closed, oriented solid, so the numbers
/// this test reports belong to the bytes a printer would receive.
///
/// Returns the selected cavity's rotation-invariant centroid radius, measured
/// on the very candidate that went on to be planned and drilled.
///
/// `drill` selects whether the drilling stage and the STL round trip are
/// asserted as well. Drilling's clear-passage check is a verdict about one
/// **pose**: a vent site that lands exactly on a spherical cavity crown's
/// axis has no clear horizontal passage, and the planner will pick exactly
/// that site when the model sits axis-aligned on the lattice. Every pose the
/// production chain actually produces is drilled and round-tripped here; the
/// synthetic off-lattice translation in
/// `the_doctrine_survives_a_quarter_pitch_phase_shift` is not such a pose and
/// exists to interrogate the flow doctrine, so it stops at the plan.
fn hollow_plan_drill_and_round_trip(name: &str, mesh: plinth_mesh::Mesh, drill: bool) -> usize {
    let settings = registered_settings();
    let source_volume_mm3 = closed_solid_volume_mm3(&mesh);
    let candidate = create_shell(mesh, settings).unwrap_or_else(|error| {
        panic!("{name}: create_shell refused the real-pipeline geometry under production limits: {error}")
    });

    // The registered savings gate, recomputed from the candidate's own
    // evidence and the live settings -- never against a literal. 2026-08-13:
    // the gate is absolute volume only (`HOLLOW_SAVE_MIN_ML`); there is no
    // percentage limb left to recompute.
    let cavity_mm3 = candidate.cavity().cavity_volume_mm3().get();
    let required_mm3 = settings.save_min_mm3().get();
    assert!(
        cavity_mm3 >= required_mm3,
        "{name}: cavity {cavity_mm3} mm^3 did not clear the registered {required_mm3} mm^3 savings minimum"
    );
    // Retargeted 2026-08-13 onto `HOLLOW_WALL_MIN_MM` by the owner's ruling.
    // The requested wall is a TARGET the cavity is cut at, and the
    // surface-nets extractor lands the delivered boundary up to one voxel
    // body diagonal inside it, so `realized >= requested` is no longer the
    // claim and by ruling is not meant to be. What a delivered shell must
    // clear is the registered minimum wall, and it is checked here on the
    // REAL pipeline geometry — after repair, decimation and the dense
    // orientation sweep, on an arbitrary lattice phase, which is the case
    // most likely to produce a thin spot.
    let realized_wall_mm = candidate.cavity().realized_wall().wall_mm().get();
    assert!(
        realized_wall_mm >= settings.minimum_wall_mm().get(),
        "{name}: realized wall {realized_wall_mm} is under the registered minimum wall {}",
        settings.minimum_wall_mm().get()
    );
    // And it is a target, not a licence to drift: the delivered minimum sits
    // within one voxel body diagonal below what was asked for.
    let body_diagonal_mm = settings.voxel_pitch_mm().get() * 3.0_f64.sqrt();
    assert!(
        realized_wall_mm > settings.wall_thickness_mm().get() - body_diagonal_mm,
        "{name}: realized wall {realized_wall_mm} is more than one voxel body diagonal \
         ({body_diagonal_mm} mm) under the {} mm target it was cut at",
        settings.wall_thickness_mm().get()
    );
    assert!(
        (candidate.cavity().source_volume_mm3().get() - source_volume_mm3).abs()
            < 1e-6 * source_volume_mm3,
        "{name}: the candidate measured a different source volume than the mesh handed to it"
    );

    let chambers = candidate
        .cavity_field()
        .deep_lobe_count(chamber_clearance_mm(settings))
        .expect("the cavity field admits its own distance transform");
    let planned = plan_drains(candidate)
        .unwrap_or_else(|failure| panic!("{name}: plan_drains refused under production limits: {}", failure.reason()));
    let holes = planned.drain_plan().holes();
    let drains = holes.iter().filter(|hole| hole.role == HoleRole::Drain).count();
    let vents = holes.iter().filter(|hole| hole.role == HoleRole::Vent).count();
    assert_eq!(drains, 1, "{name}: expected exactly one drain, got {holes:?}");
    assert_eq!(vents, 1, "{name}: expected exactly one vent, got {holes:?}");
    if !drill {
        return chambers;
    }

    // `drill_planned_shell`'s own clear-passage check (`BlockedOpening` on
    // failure) is the penetration proof: a hole that does not cut a clear
    // cavity-to-exterior passage refuses here.
    let drilled = drill_planned_shell(planned)
        .unwrap_or_else(|failure| panic!("{name}: drilling refused: {}", failure.into_reason()));
    assert_eq!(drilled.evidence().holes().len(), 2);

    // The STL bytes, not just the in-memory mesh. The Manifold-derived
    // drilled shell carries f64 coordinates the strict writer refuses, so it
    // is snapped onto the f32 wire first -- and then certified *after* the
    // snap, because that is the mesh that becomes the bytes. A snap can in
    // principle weld two vertices and open a boundary edge; certifying the
    // pre-snap mesh would prove nothing about the file.
    let on_wire = on_the_wire(drilled.mesh());
    let path = scratch_path(name);
    plinth_mesh::write_binary_stl(&path, &on_wire)
        .unwrap_or_else(|error| panic!("{name}: writing {} failed: {error}", path.display()));
    // The loader only produces raw triangle soup (`WeldPolicy::None`);
    // `build_face_topology` below is what welds it by exact bits, which is
    // the same welding every closed-solid gate in the workspace applies.
    let reloaded = plinth_mesh::load_binary_stl(&path, plinth_mesh::WeldPolicy::None)
        .unwrap_or_else(|error| panic!("{name}: re-reading {} failed: {error}", path.display()));
    let _ = std::fs::remove_file(&path);
    let topology = plinth_mesh::build_face_topology(&reloaded, plinth_mesh::WeldPolicy::ExactBits)
        .unwrap_or_else(|error| panic!("{name}: reloaded topology: {error}"));
    assert_eq!(
        topology.counts().boundary_edges,
        0,
        "{name}: the drilled STL on disk is not watertight"
    );
    let certificate = plinth_mesh::certify_closed_components(&reloaded, &topology)
        .unwrap_or_else(|error| panic!("{name}: the drilled STL on disk is not a closed oriented solid: {error}"));
    // Two closed components: the outer skin and the inner cavity skin, joined
    // by nothing -- the drilled bores pass through both, so the drilled shell
    // is a single genus-raised solid. Whichever it certifies as, the volume
    // that came off the disk has to be the hollowed volume, not the solid's.
    let disk_volume_mm3 = certificate.signed_volume_mm3();
    let expected_mm3 = source_volume_mm3 - cavity_mm3;
    assert!(
        (disk_volume_mm3 - expected_mm3).abs() <= 0.1 * expected_mm3,
        "{name}: the STL on disk encloses {disk_volume_mm3} mm^3, not the hollowed \
         {expected_mm3} mm^3 (source {source_volume_mm3} - cavity {cavity_mm3}) -- the reported \
         savings do not belong to the written bytes"
    );

    chambers
}

/// Snap every coordinate onto the STL f32 wire. Only meshes that passed
/// through the Manifold boolean kernel need it; the fixture's own source mesh
/// is f32-exact by construction.
fn on_the_wire(mesh: &plinth_mesh::Mesh) -> plinth_mesh::Mesh {
    let verts = mesh
        .verts()
        .iter()
        .map(|point| {
            [
                f64::from(point[0] as f32),
                f64::from(point[1] as f32),
                f64::from(point[2] as f32),
            ]
        })
        .collect();
    plinth_mesh::Mesh::new(verts, mesh.tris().to_vec()).expect("f32-snapped mesh is a valid mesh")
}

fn closed_solid_volume_mm3(mesh: &plinth_mesh::Mesh) -> f64 {
    let topology = plinth_mesh::build_face_topology(mesh, plinth_mesh::WeldPolicy::ExactBits)
        .expect("topology is buildable");
    plinth_mesh::certify_closed_components(mesh, &topology)
        .expect("closed, oriented solid")
        .signed_volume_mm3()
}

/// A per-process, per-case scratch path that is removed on the success path
/// and swept by the OS otherwise.
fn scratch_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "plinth-dual-cavity-pipeline-{}-{name}.stl",
        std::process::id()
    ))
}

/// **The witness the 37-mini rerun's `Solid{BelowThreshold}` gap asked for.**
/// The fixture's default (ABOVE_BORE) setting hollows, plans, and drills after
/// the real preprocessing chain, under production admission limits, and the one
/// selected resin-flow interior occupies **both** chambers.
#[test]
fn above_bore_hollows_and_spans_both_chambers_after_the_real_preprocessing() {
    let chambers = hollow_plan_drill_and_round_trip(
        "above-bore",
        through_the_real_preprocessing(DUAL_CAVITY_THROAT_ABOVE_BORE_MM),
        true,
    );
    assert_eq!(
        chambers, 2,
        "ABOVE_BORE's one selected interior occupies {chambers} chamber(s), not both -- the far \
         chamber is not in it"
    );
}

/// The other side of the doctrine boundary, through the same real chain: a
/// throat at the registered bore is refused by the corrected core threshold, so
/// the far chamber ships solid and the selected interior is the near chamber
/// alone (plus whatever sub-bore throat dead space the flood legitimately
/// claimed, which carries no deep voxel and so counts as no chamber).
#[test]
fn at_bore_hollows_the_near_chamber_alone_after_the_real_preprocessing() {
    let settings = registered_settings();
    let throat_radius_mm = registered_erosion_mm(settings) + DUAL_CAVITY_CAVITY_THROAT_AT_BORE_MM;
    let chambers = hollow_plan_drill_and_round_trip(
        "at-bore",
        through_the_real_preprocessing(throat_radius_mm),
        true,
    );
    assert_eq!(
        chambers, 1,
        "AT_BORE's selected interior occupies {chambers} chamber(s) -- the far chamber leaked in"
    );
}

/// The control: with no cavity-level throat at all the two interiors are not
/// even 6-connected, and the real chain still hollows the near chamber alone.
#[test]
fn separated_hollows_the_near_chamber_alone_after_the_real_preprocessing() {
    let chambers = hollow_plan_drill_and_round_trip(
        "separated",
        through_the_real_preprocessing(DUAL_CAVITY_THROAT_SEPARATED_MM),
        true,
    );
    assert_eq!(
        chambers, 1,
        "SEPARATED's selected interior occupies {chambers} chamber(s), not the near one alone"
    );
}

/// The lattice-phase guard, stated directly rather than left to the orientation
/// sweep's current scoring: a quarter-pitch translation on all three axes
/// destroys every exact clearance equality the raw-phase doctrine tests pin,
/// and the doctrine still has to hold on both sides of its boundary.
///
/// This is what sizes [`DUAL_CAVITY_CAVITY_THROAT_ABOVE_BORE_MM`] at 2.45 mm
/// rather than at the smallest value that reads as admitted on the fixture's
/// own phase. At the pre-2026-08-12 value of 1.90 mm the ABOVE_BORE half of
/// this test fails.
#[test]
fn the_doctrine_survives_a_quarter_pitch_phase_shift() {
    let settings = registered_settings();

    let above_bore = hollow_plan_drill_and_round_trip(
        "above-bore-shifted",
        shifted_off_the_lattice(DUAL_CAVITY_THROAT_ABOVE_BORE_MM),
        false,
    );
    assert_eq!(
        above_bore, 2,
        "off-phase ABOVE_BORE's interior occupies {above_bore} chamber(s) -- it lost the far chamber"
    );

    let at_bore = hollow_plan_drill_and_round_trip(
        "at-bore-shifted",
        shifted_off_the_lattice(
            registered_erosion_mm(settings) + DUAL_CAVITY_CAVITY_THROAT_AT_BORE_MM,
        ),
        false,
    );
    assert_eq!(
        at_bore, 1,
        "off-phase AT_BORE's interior occupies {at_bore} chamber(s) -- the far chamber leaked in"
    );
}


/// The whole point of the fixture's redesign on 2026-08-12: it fits under the
/// **shipped** `H0AdmissionLimits::production()` drain-planning ceiling at
/// every registered setting, on real-pipeline geometry. Before that redesign
/// the article existed only under a raised, caller-owned budget, so
/// `plinth prepare` shipped a solid and no user-runnable configuration
/// produced the hollow. This asserts the property directly, naming it, so a
/// future geometry change that reinflates the fixture fails here.
#[test]
fn every_setting_fits_under_the_shipped_drain_planning_ceiling() {
    // `create_shell`'s own production path already carries the production
    // ceiling into the candidate; `plan_drains` refuses against it. The three
    // tests above all call `plan_drains` under `create_shell`, so this test's
    // job is only to state the invariant with the cap in the assertion text.
    let settings = registered_settings();
    let throat_settings = [
        ("separated", DUAL_CAVITY_THROAT_SEPARATED_MM),
        (
            "at-bore",
            registered_erosion_mm(settings) + DUAL_CAVITY_CAVITY_THROAT_AT_BORE_MM,
        ),
        ("above-bore", DUAL_CAVITY_THROAT_ABOVE_BORE_MM),
    ];
    let cap = H0AdmissionLimits::production().drain_planning_bytes();
    for (name, throat_radius_mm) in throat_settings {
        let source =
            make_dual_cavity_pawn_with_throat_radius(throat_radius_mm).expect("valid throat");
        let candidate = create_shell(source, settings)
            .unwrap_or_else(|error| panic!("{name}: create_shell refused: {error}"));
        assert_eq!(
            candidate.cavity().drain_planning_cap_bytes(),
            cap,
            "{name}: the candidate is not carrying the production drain-planning ceiling"
        );
        plan_drains(candidate).unwrap_or_else(|failure| {
            panic!(
                "{name}: the fixture no longer fits under the shipped {cap}-byte drain-planning \
                 ceiling: {}",
                failure.reason()
            )
        });
    }
}
