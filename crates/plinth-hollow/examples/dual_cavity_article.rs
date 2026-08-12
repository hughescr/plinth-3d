//! The E1 dual-cavity pawn's printable article.
//!
//! Emits, for each of the fixture's three named throat settings (SEPARATED,
//! AT_BORE, ABOVE_BORE), the source solid, the hollowed-and-drilled shell,
//! and a half-section cutaway of both -- so the owner can inspect the
//! interior on screen or print the article and see with their own eyes
//! whether resin drains out of the far chamber.
//!
//! ```sh
//! cargo run --release --offline -p plinth-hollow --example dual_cavity_article -- \
//!   /path/to/an/output/directory
//! ```
//!
//! The output directory is required as `argv[1]` and is never defaulted to a
//! path inside the repository -- this binary writes nothing into the repo,
//! matching every other run-output rule in this workspace.
//!
//! **This is the direct hollow chain** (`create_shell_recording_repair` ->
//! `plan_drains` -> `drill_planned_shell`), deliberately without decimation
//! or the orientation sweep, so the printed geometry is exactly the geometry
//! `plinth-hollow`'s `dual_cavity_doctrine` tests measured. Feeding
//! `<name>.source.stl` to `plinth prepare` instead runs both of those stages,
//! either of which can move the model off the 0.5 mm lattice phase the
//! throat thresholds depend on -- fine for exercising the full CLI, but it
//! will not reproduce the flow-component numbers this fixture is built to
//! witness.
//!
//! **What to print.** `above-bore.drilled.stl` is the drainage experiment:
//! one registered bore out of the foot, one registered vent at the crown,
//! both chambers hollow and joined through the throat. `at-bore.drilled.stl`
//! is the counter-example: the upper chamber ships solid, so there is
//! nothing up there to drain. `above-bore.drilled.cutaway.stl` is the
//! inspection article -- print it to see the realized wall and the throat
//! with your own eyes -- but note it drains freely on its own, cut open, and
//! so cannot answer the drainage question by itself.

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use plinth_hollow::drains::{plan_drains, HoleRole};
use plinth_hollow::drilling::drill_planned_shell;
use plinth_hollow::{create_shell_recording_repair, H0AdmissionLimits, HollowSettings};
use plinth_mesh::{
    make_box, make_dual_cavity_pawn_with_throat_radius, write_binary_stl, Mesh, MeshError,
    TriageTopologyLimits, DUAL_CAVITY_CAVITY_THROAT_AT_BORE_MM, DUAL_CAVITY_THROAT_ABOVE_BORE_MM,
    DUAL_CAVITY_THROAT_SEPARATED_MM,
};
use plinth_solid::{BooleanLimits, ManifoldBackend, Mesh as SolidMesh};

/// This binary's own admission limits. `H0AdmissionLimits`'s private fields
/// (and its `production()` default) are not reachable from an example
/// binary, so this goes through the public `new` constructor instead, with
/// every ceiling at least as generous as `production()`'s and
/// `drain_planning_bytes` raised well past it.
///
/// **Why `drain_planning_bytes` needs raising at all.** The fixture's
/// surface-nets cavity boundary is denser (relative to its candidate ray
/// count) than any committed corpus member, so the 1 GiB production default
/// is not enough for `plan_drains`'s own preflight arithmetic. This is a
/// caller-owned resource ceiling (`H0AdmissionLimits`'s own doc: "not a
/// whole-process or third-party private-heap cap") — see the identical note
/// on the crate's own `dual_cavity_doctrine::drain_planning_limits` test
/// helper — never a FU-8b gate: no wall, margin, pitch, or savings threshold
/// moves here, only a memory-accounting budget.
fn admission_limits() -> H0AdmissionLimits {
    H0AdmissionLimits::new(
        TriageTopologyLimits::production(),
        16 * 1024 * 1024,
        8 * 1024 * 1024,
        16 * 1024 * 1024,
        BooleanLimits {
            max_input_primitive_units: 1_000_000_000,
            max_logical_marshaled_input_bytes: 8 * 1024 * 1024 * 1024,
            max_output_vertices: 200_000_000,
            max_output_triangles: 800_000_000,
        },
        8 * 1024 * 1024 * 1024,
        512 * 1024 * 1024,
        8,
    )
    .expect("hand-picked admission limits are internally valid")
}

/// Snap every coordinate onto the STL f32 wire so the canonical strict
/// writer will take it. Same normalization `plinth-mesh/examples/
/// corpus_gate.rs::normalize_stl_wire` already applies to the native
/// builders before comparing them to the frozen corpus. Only meshes that
/// passed through the Manifold boolean kernel need this: the fixture itself
/// is f32-exact by construction and passes the strict writer untouched.
fn on_the_wire(mesh: &Mesh) -> Result<Mesh, MeshError> {
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
    Mesh::new(verts, mesh.tris().to_vec())
}

/// `plinth_mesh::Mesh` -> `plinth_solid::Mesh`. A local copy of
/// `plinth_hollow::to_solid`, which is `pub(crate)` there and therefore not
/// reachable from an example binary.
fn to_solid(mesh: &Mesh) -> SolidMesh {
    let triangles = mesh
        .tris()
        .iter()
        .map(|triangle| triangle.map(u64::from))
        .collect();
    SolidMesh::new(mesh.verts().to_vec(), triangles).expect("plinth-mesh mesh is solid-valid")
}

/// `plinth_solid::Mesh` -> `plinth_mesh::Mesh`. The other half of the local
/// `to_solid`/`from_solid` pair.
fn from_solid(mesh: SolidMesh) -> Mesh {
    let triangles = mesh
        .triangles()
        .iter()
        .map(|indices| {
            [
                u32::try_from(indices[0]).expect("boolean output index fits u32"),
                u32::try_from(indices[1]).expect("boolean output index fits u32"),
                u32::try_from(indices[2]).expect("boolean output index fits u32"),
            ]
        })
        .collect();
    Mesh::new(mesh.vertices().to_vec(), triangles).expect("boolean output is a valid mesh")
}

/// A half-section cutaway: the difference of `mesh` with a box occupying
/// `y < 0`, so the interior -- both chambers and the throat -- is legible
/// without a slicer.
fn cutaway(mesh: &Mesh) -> Mesh {
    let cutter = make_box([80.0, 40.0, 80.0], [-40.0, -40.0, -10.0]).expect("cutter box");
    let operands = [to_solid(mesh), to_solid(&cutter)];
    let solid = ManifoldBackend
        .difference_limited(&operands, BooleanLimits::UNLIMITED)
        .expect("cutaway boolean succeeds on a closed, oriented, printable-sized shell");
    on_the_wire(&from_solid(solid)).expect("cutaway is f32-representable")
}

fn write(dir: &Path, name: &str, mesh: &Mesh) {
    let path = dir.join(name);
    write_binary_stl(&path, mesh).unwrap_or_else(|error| {
        panic!("writing {} failed: {error}", path.display());
    });
    println!("  wrote {}", path.display());
}

fn run_setting(dir: &Path, name: &str, throat_radius_mm: f64) {
    println!("== {name} (throat_radius_mm = {throat_radius_mm}) ==");
    let source =
        make_dual_cavity_pawn_with_throat_radius(throat_radius_mm).expect("valid throat radius");
    write(dir, &format!("{name}.source.stl"), &source);
    write(
        dir,
        &format!("{name}.source.cutaway.stl"),
        &cutaway(&source),
    );

    let settings = HollowSettings::from_registered_constants().expect("M0 settings");
    let source_volume_mm3 = closed_solid_volume_mm3(&source);
    let candidate = create_shell_recording_repair(source, settings, admission_limits())
        .unwrap_or_else(|failure| {
            panic!("{name}: expected this setting to hollow, got {failure:?}")
        });
    let cavity_volume_mm3 = candidate.cavity().cavity_volume_mm3().get();
    let realized_wall = candidate.cavity().realized_wall();
    let savings_pct = 100.0 * cavity_volume_mm3 / source_volume_mm3;

    let planned = plan_drains(candidate).unwrap_or_else(|failure| {
        panic!("{name}: expected a drain plan, got {failure:?}");
    });
    let shell_on_the_wire =
        on_the_wire(planned.shell()).expect("boolean-derived shell is f32-representable");
    write(dir, &format!("{name}.shell.stl"), &shell_on_the_wire);
    let drain_count = planned
        .drain_plan()
        .holes()
        .iter()
        .filter(|hole| hole.role == HoleRole::Drain)
        .count();
    let vent_count = planned
        .drain_plan()
        .holes()
        .iter()
        .filter(|hole| hole.role == HoleRole::Vent)
        .count();
    let drain_origin_mm = planned
        .drain_plan()
        .holes()
        .iter()
        .find(|hole| hole.role == HoleRole::Drain)
        .map(|hole| hole.center_mm.map(|value| value.get()));

    // Drilling can legitimately refuse a geometrically valid shell -- e.g.
    // `DrillingRefusal::BlockedOpening`, a real safety check on the drilled
    // hole's own clear path, not a resource ceiling this binary owns. The
    // doctrine tests (`dual_cavity_doctrine::the_dual_cavity_fixture_drains_
    // and_drills`) only exercise drilling at the default ABOVE_BORE setting;
    // an off-default setting refusing to drill here is real information
    // about that setting's own geometry, printed rather than papered over,
    // and never a reason to relax the refusal that produced it.
    match drill_planned_shell(planned) {
        Ok(drilled) => {
            let drilled_on_the_wire =
                on_the_wire(drilled.mesh()).expect("drilled shell is f32-representable");
            write(dir, &format!("{name}.drilled.stl"), &drilled_on_the_wire);
            write(
                dir,
                &format!("{name}.drilled.cutaway.stl"),
                &cutaway(&drilled_on_the_wire),
            );
        }
        Err(failure) => {
            println!("  drilling refused for {name}: {}", failure.into_reason());
            println!("  (the undrilled shell above is still a valid witness of the hollow itself)");
        }
    }

    println!(
        "  source={source_volume_mm3:.1} mm^3  cavity={cavity_volume_mm3:.1} mm^3  savings={savings_pct:.1}%  \
         realized_wall={realized_wall}  drains={drain_count}  drain_origin={drain_origin_mm:?}  vents={vent_count}"
    );
}

/// The closed, oriented source solid's own signed volume -- the same
/// certification `create_shell` runs internally, reproduced here because the
/// `ShellCandidate` this binary receives back no longer owns the source mesh.
fn closed_solid_volume_mm3(mesh: &Mesh) -> f64 {
    let topology =
        plinth_mesh::build_face_topology(mesh, plinth_mesh::WeldPolicy::ExactBits)
            .expect("source topology is buildable");
    let certificate = plinth_mesh::certify_closed_components(mesh, &topology)
        .expect("source is a closed, oriented solid");
    certificate.signed_volume_mm3()
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(output_dir) = args.next() else {
        eprintln!(
            "usage: dual_cavity_article <output-directory>\n\
             refuses to run without an explicit output directory outside the repo"
        );
        return ExitCode::FAILURE;
    };
    if args.next().is_some() {
        eprintln!("unrecognized extra argument");
        return ExitCode::FAILURE;
    }
    let output_dir = PathBuf::from(output_dir);
    if let Err(error) = std::fs::create_dir_all(&output_dir) {
        eprintln!("could not create {}: {error}", output_dir.display());
        return ExitCode::FAILURE;
    }

    let settings = HollowSettings::from_registered_constants().expect("M0 settings");
    let at_bore_throat_mm = settings.wall_thickness_mm().get()
        + settings.cavity_march_margin_mm().get()
        + DUAL_CAVITY_CAVITY_THROAT_AT_BORE_MM;

    run_setting(&output_dir, "separated", DUAL_CAVITY_THROAT_SEPARATED_MM);
    run_setting(&output_dir, "at-bore", at_bore_throat_mm);
    run_setting(&output_dir, "above-bore", DUAL_CAVITY_THROAT_ABOVE_BORE_MM);

    ExitCode::SUCCESS
}
