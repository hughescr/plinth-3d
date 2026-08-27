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
//! **This is the direct hollow chain** (`create_shell` -> `plan_drains` ->
//! `drill_planned_shell`) under genuinely unmodified
//! `H0AdmissionLimits::production()`, deliberately without decimation or the
//! orientation sweep, so the printed geometry is exactly the geometry
//! `plinth-hollow`'s `dual_cavity_doctrine` tests measured -- including its
//! *exact* throat clearances, which belong to the fixture's own 0.5 mm
//! lattice phase.
//!
//! Feeding `<name>.source.stl` to `plinth prepare` instead runs decimation
//! and the orientation sweep first, which land the model on an arbitrary
//! phase. That no longer changes the verdict: since the 2026-08-12 resizing
//! the doctrine survives the real chain, and
//! `plinth-pipeline/tests/dual_cavity_real_pipeline.rs` asserts exactly that.
//! What it does change is the exact clearance *numbers*, which is why this
//! binary still exists as the direct chain.
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
    DUAL_CAVITY_CAVITY_THROAT_AT_BORE_MM, DUAL_CAVITY_THROAT_ABOVE_BORE_MM,
    DUAL_CAVITY_THROAT_SEPARATED_MM,
};
use plinth_solid::{BooleanLimits, ManifoldBackend, Mesh as SolidMesh};

/// Snap every coordinate onto the STL f32 wire so the canonical strict
/// writer will take it. Same normalization `plinth-mesh/examples/
/// corpus_gate.rs::normalize_stl_wire` already applies to the native
/// builders before comparing them to the frozen corpus. Only meshes that
/// passed through the Manifold boolean kernel need this: the fixture itself
/// is f32-exact by construction and passes the strict writer untouched.
#[expect(
    clippy::cast_possible_truncation,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
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
///
/// **`make_box`'s `translate` is the box's CENTRE on all three axes**, not
/// its low corner. Getting that backwards produces a cutter disjoint from
/// the model and a "cutaway" that is a byte-for-byte copy of its input --
/// which is what this function shipped until 2026-08-12. The panic below is
/// the guard: a cutaway that removed no volume is not a cutaway, and it is
/// better to stop than to write an uncut file under a name that promises a
/// section.
fn cutaway(mesh: &Mesh) -> Mesh {
    let (lo, hi) = bounds(mesh);
    // Centred on the model in x and z, and covering exactly the `y < 0` half
    // of it, with a millimetre of overhang on every open side so the boolean
    // never has to resolve a coplanar face.
    let size = [
        (hi[0] - lo[0]) + 2.0,
        (hi[1] - lo[1]) + 2.0,
        (hi[2] - lo[2]) + 2.0,
    ];
    let centre = [
        0.5 * (lo[0] + hi[0]),
        0.0 - size[1] / 2.0,
        0.5 * (lo[2] + hi[2]),
    ];
    let cutter = make_box(size, centre).expect("cutter box");
    let operands = [to_solid(mesh), to_solid(&cutter)];
    let solid = ManifoldBackend
        .difference_limited(&operands, cutaway_boolean_limits(mesh, &cutter))
        .expect("cutaway boolean succeeds on a closed, oriented, printable-sized shell");
    let cut = on_the_wire(&from_solid(solid)).expect("cutaway is f32-representable");
    let before = closed_solid_volume_mm3(mesh);
    let after = closed_solid_volume_mm3(&cut);
    assert!(
        after < 0.9 * before,
        "the cutaway removed only {} of {before} mm^3 -- the cutter is not intersecting the model",
        before - after
    );
    cut
}

/// The presentation-only cutaway boolean's own ceilings, sized from the
/// operands in hand rather than switched off. `BooleanLimits::UNLIMITED`
/// would make this binary's output unfalsifiable evidence of what a bounded
/// kernel can do; nothing here is on an evidence path, but a limit that is
/// stated and generous is still worth more than one that is absent.
fn cutaway_boolean_limits(mesh: &Mesh, cutter: &Mesh) -> BooleanLimits {
    let vertices = mesh.verts().len() + cutter.verts().len();
    let triangles = mesh.tris().len() + cutter.tris().len();
    BooleanLimits {
        max_input_primitive_units: (vertices + triangles) as u64 * 8,
        max_logical_marshaled_input_bytes: (vertices * 24 + triangles * 12) as u64 * 8,
        // A difference against a half-space box cannot multiply the input
        // triangle count by more than a small constant; eight is slack, not
        // a licence.
        max_output_vertices: vertices as u64 * 8,
        max_output_triangles: triangles as u64 * 8,
    }
}

fn bounds(mesh: &Mesh) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for vertex in mesh.verts() {
        for axis in 0..3 {
            lo[axis] = lo[axis].min(vertex[axis]);
            hi[axis] = hi[axis].max(vertex[axis]);
        }
    }
    (lo, hi)
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
    let candidate =
        create_shell_recording_repair(source, settings, H0AdmissionLimits::production())
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
    // The section of the undrilled shell, so every setting -- including the
    // two whose vent the drilling stage refuses -- still has an article the
    // owner can look inside.
    write(
        dir,
        &format!("{name}.shell.cutaway.stl"),
        &cutaway(&shell_on_the_wire),
    );
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
