//! E1 WITNESS measurement: run the E1 dual-cavity pawn fixture through the
//! **real production chain** -- `plinth_mesh::repair_mesh` ->
//! `plinth_decimate::run` (registered decimation budget) ->
//! `plinth_orient::search_and_orient` (registered dense sweep, production
//! work limits) -> `plinth_hollow::create_shell_recording_repair` (**genuinely
//! unmodified** `H0AdmissionLimits::production()`) -- and reports exactly
//! what the engine measured, at both the SEPARATED (below-bore) and
//! ABOVE_BORE throat settings.
//!
//! This is deliberately *not* `plinth_hollow::run` / `plinth prepare`: both
//! silently reclassify a `plan_drains` resource-cap refusal as a generic
//! `SolidDecisionReason::BelowThreshold` (see `hollow_error_refuses_this_geometry`
//! in `plinth-pipeline/src/prepare.rs`), which is indistinguishable on the
//! wire from a genuine savings-gate refusal. Stopping right after
//! `create_shell_recording_repair` -- before `plan_drains` -- lets this
//! harness report the hollow stage's own verdict (cavity volume, requested
//! and realized wall, disposition) on its own terms, and then report the
//! *separate* `plan_drains` resource ceiling as its own finding with its own
//! exact numbers, rather than let one obscure the other.
//!
//! A second pass, clearly labelled, re-runs `create_shell` and then
//! `plan_drains` + `drill_planned_shell` with an explicitly **raised**
//! `drain_planning_bytes` (the same caller-owned resource-ceiling knob
//! `dual_cavity_article.rs` already uses and documents) to still produce a
//! printable drilled STL and real drain/vent placement -- never to paper over
//! the production-cap finding, only to keep answering the fixture's other
//! three questions (drain placement, printable STL, dimensions) once the
//! production-cap finding has been reported in its own right.
//!
//! ```sh
//! cargo run --release --offline -p plinth-pipeline --example dual_cavity_real_pipeline -- \
//!   /path/to/an/output/directory
//! ```

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use plinth_hollow::drains::plan_drains;
use plinth_hollow::drilling::drill_planned_shell;
use plinth_hollow::{create_shell_recording_repair, H0AdmissionLimits, HollowRequest, HollowSettings};
use plinth_mesh::{
    make_dual_cavity_pawn_with_throat_radius, write_binary_stl, Mesh, TriageTopologyLimits,
    DUAL_CAVITY_THROAT_ABOVE_BORE_MM, DUAL_CAVITY_THROAT_SEPARATED_MM,
};
use plinth_pipeline::prepare::registered_decimation_budget;
use plinth_pipeline::PrepareResourcePolicy;
use plinth_solid::BooleanLimits;

/// The one raised knob, used only in the second pass, exactly as
/// `dual_cavity_article.rs` documents it: a caller-owned memory-accounting
/// budget (`H0AdmissionLimits`'s own doc: "not a whole-process or
/// third-party private-heap cap"), never a FU-8b gate. Every other field is
/// `production()`'s own value.
fn raised_drain_budget_limits() -> H0AdmissionLimits {
    let production = H0AdmissionLimits::production();
    // `BooleanLimits` has no public accessors on `H0AdmissionLimits::production()`
    // to read back, so these are the same generous, at-least-production-sized
    // figures `dual_cavity_article.rs::admission_limits()` already uses and has
    // committed -- every field here is at or above `production()`'s own, except
    // `drain_planning_bytes`, the one field this function exists to raise.
    H0AdmissionLimits::new(
        TriageTopologyLimits::production(),
        production.max_voxel_cells(),
        production.query_working_bytes(),
        production.max_raster_work_items(),
        BooleanLimits {
            max_input_primitive_units: 1_000_000_000,
            max_logical_marshaled_input_bytes: 8 * 1024 * 1024 * 1024,
            max_output_vertices: 200_000_000,
            max_output_triangles: 800_000_000,
        },
        8 * 1024 * 1024 * 1024,
        production.max_cavity_distance_index_bytes(),
        production.max_cavity_repair_passes(),
    )
    .expect("production limits with only drain_planning_bytes raised are internally valid")
}

fn run_setting(dir: &Path, name: &str, throat_radius_mm: f64) {
    println!("\n================ {name} (throat_radius_mm = {throat_radius_mm}) ================");
    let source = make_dual_cavity_pawn_with_throat_radius(throat_radius_mm)
        .expect("valid throat radius");
    let source_faces = source.tris().len();
    write(dir, &format!("{name}.source.stl"), &source);

    let constants = plinth_core::m0_constants().expect("registered M0 constants");
    let printer = constants.saturn4_ultra().clone();
    let request =
        HollowRequest::with_registered_wall_default(printer).expect("registered wall default");
    let settings = HollowSettings::from_request(&request).expect("valid hollow settings");

    // PASS 0: the direct chain, no decimate/orient -- the exact construction
    // the crate's own `dual_cavity_doctrine` B1/B6 tests measure, on the
    // fixture's raw, lattice-phase-exact mesh. Run here as a same-process,
    // apples-to-apples CONTROL for PASS 1/2's decimated+oriented cavity
    // volumes: the fixture's own docs warn that decimation and the
    // orientation sweep can move the geometry off the 0.5 mm lattice phase
    // the throat's admitted/refused clearance depends on, and this control is
    // what makes that visible as a number instead of a caveat.
    match create_shell_recording_repair(source.clone(), settings, H0AdmissionLimits::production())
    {
        Ok(candidate) => {
            println!(
                "  PASS 0 (direct chain, no decimate/orient, CONTROL) cavity_volume={:.1} mm^3",
                candidate.cavity().cavity_volume_mm3().get()
            );
        }
        Err(failure) => {
            println!(
                "  PASS 0 (direct chain, no decimate/orient, CONTROL): DID NOT HOLLOW -- {}",
                failure.into_error()
            );
        }
    }

    let (repaired, _evidence) = plinth_mesh::repair_mesh(&source).into_parts();

    let budget = registered_decimation_budget().expect("registered decimation budget");
    let policy = PrepareResourcePolicy::for_saturn4_ultra(budget)
        .expect("saturn4-ultra resource policy");
    let decimated = plinth_decimate::run(&repaired, policy.decimation())
        .expect("decimation run")
        .into_decimated_model()
        .expect("not scale-suspicious");
    let decimated_faces = decimated.mesh().tris().len();

    let mut sweep =
        plinth_orient::registered_sweep_request().expect("registered sweep request");
    sweep.limits = policy.orient();
    let oriented = plinth_orient::search_and_orient(decimated, &sweep)
        .expect("dense sweep finds an orientation for this fixture");
    let (bbox_lo, bbox_hi) = bbox(oriented.mesh());
    println!(
        "  faces  source={source_faces}  decimated={decimated_faces}"
    );
    println!(
        "  real dimensions after decimate+dense-orient (mm)  x={:.3}  y={:.3}  z={:.3}",
        bbox_hi[0] - bbox_lo[0],
        bbox_hi[1] - bbox_lo[1],
        bbox_hi[2] - bbox_lo[2]
    );

    println!("  wall requested (registered default)  {:.4} mm", settings.wall_thickness_mm().get());

    let (solid_mesh, orientation_history) = oriented.into_parts();

    // PASS 1: create_shell only, genuinely unmodified H0AdmissionLimits::production().
    // This is the hollow stage's own verdict, on its own terms -- the same
    // limits `plinth_hollow::run` (and therefore `plinth prepare`) actually use
    // for create_shell.
    match create_shell_recording_repair(
        solid_mesh.clone(),
        settings,
        H0AdmissionLimits::production(),
    ) {
        Ok(candidate) => {
            let cavity_volume_mm3 = candidate.cavity().cavity_volume_mm3().get();
            let realized_wall = candidate.cavity().realized_wall();
            let source_volume_mm3 = closed_solid_volume_mm3(&solid_mesh);
            let savings_pct = 100.0 * cavity_volume_mm3 / source_volume_mm3;
            println!(
                "  PASS 1 (production H0AdmissionLimits) create_shell: HOLLOWED"
            );
            println!(
                "    source_volume={source_volume_mm3:.1} mm^3  cavity_volume={cavity_volume_mm3:.1} mm^3  savings={savings_pct:.1}%"
            );
            println!("    realized_wall={realized_wall}");

            // Now attempt plan_drains under the SAME production limits, to
            // report its own resource ceiling with its own exact numbers --
            // this is the stage `plinth_hollow::run` calls next, and the one
            // that actually refuses this fixture in the real pipeline.
            match plan_drains(candidate) {
                Ok(_planned) => {
                    println!("    plan_drains (production drain_planning_bytes): SUCCEEDED");
                }
                Err(failure) => {
                    println!(
                        "    plan_drains (production drain_planning_bytes): REFUSED -- {}",
                        failure.reason()
                    );
                    println!(
                        "    (this is the exact reason `plinth_hollow::run` -- and therefore \
                         `plinth prepare` -- silently substitutes a solid disposition for this \
                         fixture at this throat setting; see the module doc)"
                    );
                }
            }
        }
        Err(failure) => {
            println!(
                "  PASS 1 (production H0AdmissionLimits) create_shell: DID NOT HOLLOW -- {}",
                failure.into_error()
            );
        }
    }

    // PASS 2: raised drain_planning_bytes only (documented, caller-owned
    // budget), so the fixture's other three questions -- drain placement,
    // penetration, a printable drilled STL -- can still be answered on real
    // decimated+oriented geometry. Never a substitute for PASS 1's finding.
    println!("  PASS 2 (drain_planning_bytes raised to 8 GiB, every other limit production):");
    let candidate2 = match create_shell_recording_repair(
        solid_mesh,
        settings,
        raised_drain_budget_limits(),
    ) {
        Ok(candidate) => candidate,
        Err(failure) => {
            println!("    create_shell: DID NOT HOLLOW -- {}", failure.into_error());
            return;
        }
    };
    let planned = match plan_drains(candidate2) {
        Ok(planned) => planned,
        Err(failure) => {
            println!("    plan_drains: REFUSED even at the raised budget -- {}", failure.reason());
            return;
        }
    };
    let shell = planned.shell().clone();
    write(dir, &format!("{name}.shell.stl"), &shell);
    let drilled = match drill_planned_shell(planned) {
        Ok(drilled) => drilled,
        Err(failure) => {
            println!(
                "    drill_planned_shell: REFUSED -- {}",
                failure.into_reason()
            );
            println!("    (the undrilled shell above is still a valid witness of the hollow itself)");
            return;
        }
    };
    let drilled_mesh = drilled.mesh().clone();
    write(dir, &format!("{name}.drilled.stl"), &drilled_mesh);
    let hollowed = drilled.into_hollowed_model(orientation_history);
    let report = hollowed.history().hollow();
    println!("    disposition  {:?}", report.disposition());
    for cavity in report.cavities() {
        println!(
            "    cavity id={}  volume={:.4} mL  drains={:?}",
            cavity.cavity_id,
            cavity.volume_ml.get(),
            cavity.drain_ids
        );
    }
    for drain in report.drains() {
        println!(
            "    hole cavity_id={}  radius={:.3} mm  entrance=({:.3},{:.3},{:.3}) mm  exit=({:.3},{:.3},{:.3}) mm",
            drain.cavity_id,
            drain.radius_mm.get(),
            drain.entrance_mm[0].get(),
            drain.entrance_mm[1].get(),
            drain.entrance_mm[2].get(),
            drain.exit_mm[0].get(),
            drain.exit_mm[1].get(),
            drain.exit_mm[2].get(),
        );
    }
    println!(
        "    drill_planned_shell SUCCEEDED -- the production drilling algorithm's own clear-passage \
         check (DrillingRefusal::BlockedOpening on failure) is the FU-61 penetration proof: every \
         hole above cuts a genuinely clear cavity-to-exterior passage."
    );
}

/// Snap every coordinate onto the STL f32 wire before writing. Only meshes
/// that passed through the Manifold boolean kernel (the shell and drilled
/// shell, both produced downstream of `create_shell`/`plan_drains`/
/// `drill_planned_shell`) need this -- the fixture's own source mesh is
/// already f32-exact by construction. Same normalization
/// `dual_cavity_article.rs::on_the_wire` already uses.
fn on_the_wire(mesh: &Mesh) -> Mesh {
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
    Mesh::new(verts, mesh.tris().to_vec()).expect("f32-snapped mesh is still a valid mesh")
}

fn write(dir: &Path, name: &str, mesh: &Mesh) {
    let path = dir.join(name);
    let on_wire = on_the_wire(mesh);
    write_binary_stl(&path, &on_wire).unwrap_or_else(|error| {
        panic!("writing {} failed: {error}", path.display());
    });
    println!("  wrote {}", path.display());
}

fn bbox(mesh: &Mesh) -> ([f64; 3], [f64; 3]) {
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

fn closed_solid_volume_mm3(mesh: &Mesh) -> f64 {
    let topology = plinth_mesh::build_face_topology(mesh, plinth_mesh::WeldPolicy::ExactBits)
        .expect("topology is buildable");
    let certificate = plinth_mesh::certify_closed_components(mesh, &topology)
        .expect("closed, oriented solid");
    certificate.signed_volume_mm3()
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(output_dir) = args.next() else {
        eprintln!(
            "usage: dual_cavity_real_pipeline <output-directory>\n\
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

    run_setting(&output_dir, "below-bore", DUAL_CAVITY_THROAT_SEPARATED_MM);
    run_setting(&output_dir, "above-bore", DUAL_CAVITY_THROAT_ABOVE_BORE_MM);

    ExitCode::SUCCESS
}
