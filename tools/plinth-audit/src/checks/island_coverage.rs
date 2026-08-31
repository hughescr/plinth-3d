//! I7 island coverage -- `docs/specs/audit-invariants.md` §3.7.
//!
//! ## The predicate, exactly as specified
//!
//! From the layer raster of the MODEL solid `M` (`ModelContext::model_raster`):
//! pixel column `(row, col)` is *attached* at layer `k` if the same column is
//! cured at layer `k-1`. A connected run-component `C` of cured pixels at
//! layer `k` (`RunRaster::layer_components`) is a **detached island** if it
//! contains zero attached pixels AND `k` is not the first layer above the
//! plate (`grid.layer_at_or_above(z_plate)`, which is `0` on this grid by
//! construction). For every detached island whose area exceeds one printer
//! pixel, the cured SUPPORT raster (`ModelContext::support_raster`) must
//! intersect it -- any pixel-level overlap at the SAME layer, since this
//! invariant is a presence test, not an adequacy test (that is I2/I3/I4's
//! job: the demand set `D` already counts every cantilever and cavity
//! ceiling by projected area, so a present-but-undersized support is caught
//! there, not here).
//!
//! The definition is fully derived from the printer grid: no self-support
//! angle, no reach radius. It is uncontestable physics -- material with
//! nothing under it and nothing beside it falls into the vat.
//!
//! ## What this predicate does NOT gate, by the specification's own admission
//!
//! **The cantilever hole is the largest in the whole document (§3.7,
//! §6.1).** A run-component that is attached along even one edge is never a
//! "detached island" by this definition, no matter how far it cantilevers
//! unsupported -- a 20 mm ledge, a cavity ceiling, a wing that grows sideways
//! one pixel per layer are all invisible here. Gating that needs a measured
//! self-support span coupon the project does not have. It is measured and
//! disclosed in `report.overhang` (owned by `report_metrics.rs`), never
//! passed.
//!
//! ## What this implementation additionally cannot see
//!
//! 1. **Presence, not connectivity.** The support raster this check tests
//!    against is the union of every reconstructed primitive, live or dead,
//!    grounded or orphaned. A support blob that geometrically overlaps an
//!    island but traces to no ground (I5's job) or is itself degenerate (I8's
//!    job) still satisfies I7. The eight gating invariants are meant to be
//!    read together; I7 alone only proves "something cured is touching this
//!    island here", not "something load-bearing is".
//! 2. **Same-layer strictness.** The predicate as specified tests the support
//!    raster at exactly layer `k`, the island's birth layer. A support column
//!    that fully embeds a few layers into the model (the normal, intended
//!    case, since the seat depth invariant requires embedding) is fine; a
//!    support that stopped exactly one layer short of the island's birth --
//!    inside the fusion-uncertain band of `audit-invariants.md` §1.5 -- would
//!    read as unsupported here even though it may fuse in the vat. This
//!    invariant does not consult that band; I6 and I5 are where fusion
//!    uncertainty is judged.
//!
//! ## Failure codes
//!
//! `ISLAND_UNSUPPORTED`, `ISLAND_INDETERMINATE` (reason `slice_failed` or
//! `raster_budget_exceeded`).

use crate::check::{Check, CheckOutcome, Failure};
use crate::context::ModelContext;
use crate::json::Json;
use crate::raster::{RasterError, RasterGrid, Run, RunRaster};

pub struct IslandCoverage;

impl Check for IslandCoverage {
    fn id(&self) -> &'static str {
        "i7_island_coverage"
    }

    fn title(&self) -> &'static str {
        "I7 island coverage"
    }

    fn scope(&self, ctx: &ModelContext) -> usize {
        // Prefer the real layer count once the raster is available -- it is
        // memoized, so asking here costs nothing extra when `run` asks again.
        if let Ok(r) = ctx.model_raster() {
            return r.layers.len().max(1);
        }
        // Before the raster exists (or when it never will), a bbox-based
        // estimate is the honest fallback the stub used.
        let e = ctx.model_mesh.bbox.extent()[2];
        ((e / ctx.grid.layer_height_mm).ceil() as usize).max(1)
    }

    fn run(&self, ctx: &ModelContext) -> CheckOutcome {
        let file = ctx.file();
        let grid = ctx.grid;
        let pixel_area = grid.pixel_area_mm2();

        let model_raster = match ctx.model_raster() {
            Ok(r) => r,
            Err(e) => {
                return raster_unavailable(&file, ctx.model_index, self.scope(ctx), "model", e);
            }
        };
        let support_raster = match ctx.support_raster() {
            Ok(r) => r,
            Err(e) => {
                return raster_unavailable(&file, ctx.model_index, self.scope(ctx), "support", e);
            }
        };

        // The plate sits at grid.z_plate_mm by construction (ModelContext
        // builds the grid from the same z_plate the frame check resolved), so
        // this is always layer 0 -- computed rather than hard-coded so a
        // future frame convention change cannot silently desync it from the
        // grid it is compared against.
        let first_layer_k = grid.layer_at_or_above(grid.z_plate_mm);

        let mut out = CheckOutcome::new(0);
        let mut island_rows: Vec<Json> = Vec::new();
        let mut layers_scanned = 0usize;
        let mut worst: Option<(f64, f64, i64)> = None; // (area_mm2, z_mm, k)

        for layer in &model_raster.layers {
            layers_scanned += 1;
            let k = layer.k;
            if k == first_layer_k {
                // The plate itself is the support at the first layer above
                // it; nothing here is a "detached island" by definition.
                continue;
            }
            for comp in model_raster.layer_components(k) {
                let area_mm2 = component_area_mm2(&comp, &grid);
                if area_mm2 <= pixel_area {
                    // The printer's own censor: one pixel is noise, not an
                    // island (audit-invariants.md §3.7 predicate clause).
                    continue;
                }
                if is_attached(&comp, model_raster, k) {
                    // Continuing material, not a detached island at all.
                    continue;
                }

                out.checked += 1;
                let (cx_mm, cy_mm) = component_centroid_mm(&comp, &grid);
                let z_mm = grid.layer_z_mm(k);
                let pixels = component_pixel_count(&comp);
                let supported = is_supported(&comp, support_raster, k);

                let mut row = Json::obj();
                row.set("layer", Json::int(k));
                row.set("z_mm", Json::num(z_mm));
                row.set("area_mm2", Json::num(area_mm2));
                row.set("centroid_mm", Json::nums([cx_mm, cy_mm]));
                row.set("pixels", Json::int(pixels));
                row.set("supported", Json::Bool(supported));
                island_rows.push(row);

                if !supported {
                    let mut values = Json::obj();
                    values.set("layer", Json::int(k));
                    values.set("z_mm", Json::num(z_mm));
                    values.set("area_mm2", Json::num(area_mm2));
                    values.set("centroid_mm", Json::nums([cx_mm, cy_mm]));
                    values.set("pixels", Json::int(pixels));
                    out.fail(Failure::line(
                        "ISLAND_UNSUPPORTED",
                        &file,
                        ctx.model_index,
                        None,
                        None,
                        &[
                            ("layer", k.to_string()),
                            ("z_mm", format!("{z_mm}")),
                            ("area_mm2", format!("{area_mm2}")),
                            ("centroid", format!("{cx_mm},{cy_mm}")),
                            ("pixels", pixels.to_string()),
                        ],
                        values,
                    ));
                    let worse = worst.is_none_or(|(a, ..)| area_mm2 > a);
                    if worse {
                        worst = Some((area_mm2, z_mm, k));
                    }
                }
            }
        }

        out.measured.set("islands", Json::Arr(island_rows));
        out.measured.set("layers_scanned", Json::usize(layers_scanned));
        out.measured
            .set("detached_islands_checked", Json::usize(out.checked));
        out.measured.set("first_layer_k", Json::int(first_layer_k));
        out.measured.set("pixel_area_mm2", Json::num(pixel_area));

        let headline = if let Some((a, z, k)) = worst {
            format!(
                "{} of {} detached islands unsupported; worst area {a:.6} mm2 at layer {k} \
                 (z={z:.3} mm)",
                out.failed, out.checked
            )
        } else if out.checked == 0 {
            "no detached islands above the one-pixel censor across \
             the model; cantilevers are disclosed, not gated, in report.overhang"
                .to_string()
        } else {
            format!(
                "{} detached islands above the one-pixel censor, all intersected by support",
                out.checked
            )
        };
        out.measured.set("headline", Json::str(headline));
        out
    }
}

/// Build the whole-check indeterminate outcome when a raster this invariant
/// needs could not be built at all. `audit-invariants.md` §3.7 frames its
/// `ISLAND_INDETERMINATE` line per layer (`slice_failed` at "this layer"),
/// but both rasters here are built once for the whole model
/// (`ModelContext::model_raster` / `support_raster`), so a build failure is a
/// whole-model fact with no single layer to name -- reported as `layer=n/a`
/// rather than inventing a layer number the reconstruction never reached.
/// "Not measured" and "indeterminate" never map to pass regardless.
fn raster_unavailable(
    file: &str,
    model: usize,
    scope: usize,
    which: &'static str,
    e: &RasterError,
) -> CheckOutcome {
    let scope = scope.max(1);
    let reason = match e {
        RasterError::BudgetExceeded { .. } => "raster_budget_exceeded",
        RasterError::MeshNotClosed | RasterError::EmptyOrNonFinite => "slice_failed",
    };
    let mut values = Json::obj();
    values.set("reason", Json::str(reason));
    values.set("raster", Json::str(which));
    values.set("detail", Json::str(e.to_string()));
    let failure = Failure::line(
        "ISLAND_INDETERMINATE",
        file,
        model,
        None,
        None,
        &[("layer", "n/a".to_string()), ("reason", reason.to_string())],
        values,
    );
    let mut measured = Json::obj();
    measured.set(
        "headline",
        Json::str(format!(
            "{which} raster unavailable ({reason}); island coverage could not be established"
        )),
    );
    CheckOutcome {
        checked: scope,
        failed: 0,
        indeterminate: scope,
        measured,
        failures: vec![failure],
    }
}

/// Cured pixel count of a run-component.
fn component_pixel_count(comp: &[(i64, Run)]) -> i64 {
    comp.iter().map(|(_, r)| r.len().max(0)).sum()
}

/// Cured area of a run-component, mm^2. Exact: an integer pixel count times
/// the printer's own pixel footprint.
fn component_area_mm2(comp: &[(i64, Run)], grid: &RasterGrid) -> f64 {
    component_pixel_count(comp) as f64 * grid.pixel_area_mm2()
}

/// The pixel-count-weighted centroid of a run-component, in the model-centred
/// frame, mm. Column centres are `(c0+c1)/2 * pitch_x`; row centres are
/// `(row+0.5) * pitch_y`.
fn component_centroid_mm(comp: &[(i64, Run)], grid: &RasterGrid) -> (f64, f64) {
    let mut sx = 0.0f64;
    let mut sy = 0.0f64;
    let mut n = 0i64;
    for (row, run) in comp {
        let cnt = run.len().max(0);
        if cnt <= 0 {
            continue;
        }
        let x_avg = (run.c0 as f64 + run.c1 as f64) * 0.5 * grid.pitch_x_mm;
        let y = (*row as f64 + 0.5) * grid.pitch_y_mm;
        sx += x_avg * cnt as f64;
        sy += y * cnt as f64;
        n += cnt;
    }
    if n == 0 {
        (0.0, 0.0)
    } else {
        (sx / n as f64, sy / n as f64)
    }
}

/// Does this component share even one pixel column with the same raster's
/// layer directly below it? `false` when there is no layer `k-1` at all --
/// which is the correct answer for the model's own lowest material, exactly
/// the case a detached-island check must catch.
fn is_attached(comp: &[(i64, Run)], model_raster: &RunRaster, k: i64) -> bool {
    let Some(prev) = model_raster.layer(k - 1) else {
        return false;
    };
    comp.iter().any(|(row, run)| {
        prev.runs_in_row(*row)
            .iter()
            .any(|pr| run.c0 < pr.c1 && pr.c0 < run.c1)
    })
}

/// Does the cured support raster intersect this component at layer `k`? A
/// pure presence test, on the same grid, at the same layer -- see the module
/// docs for what this deliberately does not establish.
fn is_supported(comp: &[(i64, Run)], support_raster: &RunRaster, k: i64) -> bool {
    let Some(layer) = support_raster.layer(k) else {
        return false;
    };
    comp.iter().any(|(row, run)| {
        layer
            .runs_in_row(*row)
            .iter()
            .any(|sr| run.c0 < sr.c1 && sr.c0 < run.c1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::Status;
    use crate::consts as c;
    use crate::context::{AuditContext, AuditOptions};
    use crate::primitives::Primitive;
    use crate::raster::Budget;
    use crate::reader::{ChituboxFile, RecordKind};
    use crate::synth::{box_soup, SynthFile, SynthModel, SynthRecord};
    use std::path::Path;

    fn parse(sf: &SynthFile) -> ChituboxFile {
        ChituboxFile::parse(Path::new("island_coverage_test.chitubox"), &sf.to_bytes())
            .expect("synthetic file must parse")
    }

    /// Box A sits with its bottom exactly on the plate (translate_z = 0, so
    /// `z_plate = bbox.min.z - 0 = 0`), 2x2x1 mm. Box B is a detached 1x1x1
    /// mm island floating 2 mm above box A's top -- no shared vertex, no
    /// touching pixel, a genuine "born in mid-air" overhang.
    fn two_box_model(support: Option<SynthRecord>) -> SynthFile {
        let mut mesh = box_soup([0.0, 0.0, 0.0], [2.0, 2.0, 1.0]);
        mesh.extend(box_soup([0.0, 0.0, 3.0], [1.0, 1.0, 4.0]));
        let records = support.into_iter().collect::<Vec<_>>();
        SynthFile::new().with_model(SynthModel {
            name: "two-box".into(),
            translate_mm: [0.0, 0.0, 0.0],
            mesh,
            records,
        })
    }

    fn ctx_and_run<F: FnOnce(&ModelContext) -> ()>(sf: &SynthFile, f: F) {
        let file = parse(sf);
        let options = AuditOptions::default();
        let audit = AuditContext {
            file: &file,
            options: &options,
            sidecar: None,
            sidecar_absence: None,
        };
        let ctx = ModelContext::build(&audit, 0);
        f(&ctx);
    }

    #[test]
    fn an_unsupported_floating_island_fails_with_a_named_location() {
        ctx_and_run(&two_box_model(None), |ctx| {
            assert!(
                ctx.model_mesh.closure.is_closed(),
                "{}",
                ctx.model_mesh.closure.summary()
            );
            let outcome = IslandCoverage.run(ctx);
            assert_eq!(outcome.status(), Status::Fail);
            assert_eq!(outcome.checked, 1, "exactly one island: box B's birth layer");
            assert_eq!(outcome.failed, 1);
            assert_eq!(outcome.indeterminate, 0);
            let f = &outcome.failures[0];
            assert_eq!(f.code, "ISLAND_UNSUPPORTED");
            // The birth layer is box B's bottom, z = 3.0 mm, so the reported
            // z must land inside box B's own layer band, not box A's.
            let z = f.values.get("z_mm").and_then(Json::as_f64).unwrap();
            assert!((3.0..3.05).contains(&z), "z_mm={z}");
            let area = f.values.get("area_mm2").and_then(Json::as_f64).unwrap();
            // NOT 1.0 mm^2. The island is a 1x1 mm square whose faces lie
            // EXACTLY on x = 0 and y = 0, which are exactly a pixel-column and
            // a pixel-row boundary of the 0.014 x 0.019 mm grid. Under the
            // full-containment fill rule (`raster.rs`, and §1.6) the edge
            // sitting on a boundary blocks that column and that row, so the
            // cured square is 70 columns by 51 rows, not the 71 x 52 an
            // off-lattice square of the same size would give:
            //
            //     70 * 0.014 * 51 * 0.019 = 0.949620 mm^2   (measured)
            //     71 * 0.014 * 52 * 0.019 = 0.982072 mm^2   (off-lattice)
            //                               1.000000 mm^2   (ideal geometry)
            //
            // Asserting against the ideal 1.0 mm^2 would be asserting that the
            // rasteriser does not erode, which it does, deliberately, in the
            // failing direction. The bound below is pixel-exact and would
            // catch a real regression; the previous +/- 0.05 mm^2 band around
            // 1.0 mm^2 missed the true value by 0.0004 mm^2.
            let pixel_exact = 70.0 * c::PIXEL_PITCH_X_MM * 51.0 * c::PIXEL_PITCH_Y_MM;
            assert!(
                (area - pixel_exact).abs() < 1.0e-6,
                "area_mm2={area}, pixel-exact full-containment area is {pixel_exact}"
            );
            assert!(area < 1.0, "erosion is toward failing: area_mm2={area}");
        });
    }

    #[test]
    fn a_column_reaching_into_the_island_makes_it_supported() {
        let support = SynthRecord::new(
            RecordKind::Member,
            [0.5, 0.5, 1.0],
            [0.5, 0.5, 3.5], // embeds 0.5 mm past box B's own birth layer
            0.3,
            0.3,
        )
        .with_matching_soup();
        ctx_and_run(&two_box_model(Some(support)), |ctx| {
            let outcome = IslandCoverage.run(ctx);
            assert_eq!(outcome.status(), Status::Pass, "{:?}", outcome.failures);
            assert_eq!(outcome.checked, 1);
            assert_eq!(outcome.failed, 0);
            assert_eq!(outcome.indeterminate, 0);
        });
    }

    #[test]
    fn a_model_resting_directly_on_the_plate_is_exempt_at_its_own_birth_layer() {
        // Box A alone: its bottom sits at z_plate (translate_z = 0), so its
        // own birth layer is layer 0, which is exempt. It must produce no
        // checked items at all.
        let sf = SynthFile::new().with_model(SynthModel {
            name: "plate-resting".into(),
            translate_mm: [0.0, 0.0, 0.0],
            mesh: box_soup([0.0, 0.0, 0.0], [2.0, 2.0, 1.0]),
            records: Vec::new(),
        });
        ctx_and_run(&sf, |ctx| {
            let outcome = IslandCoverage.run(ctx);
            assert_eq!(outcome.status(), Status::Pass);
            assert_eq!(outcome.checked, 0);
        });
    }

    #[test]
    fn an_unmeasurable_model_fails_indeterminate_never_pass() {
        // Drop one triangle from box A so the mesh is open: no interior is
        // defined, so `model_raster()` must refuse rather than guess.
        let mut mesh = box_soup([0.0, 0.0, 0.0], [2.0, 2.0, 1.0]);
        mesh.pop();
        let sf = SynthFile::new().with_model(SynthModel {
            name: "open".into(),
            translate_mm: [0.0, 0.0, 0.0],
            mesh,
            records: Vec::new(),
        });
        ctx_and_run(&sf, |ctx| {
            assert!(!ctx.model_mesh.closure.is_closed());
            let outcome = IslandCoverage.run(ctx);
            assert_eq!(outcome.status(), Status::Fail);
            assert!(outcome.indeterminate > 0, "unmeasurable must never pass");
            assert_eq!(outcome.failed, 0);
            let f = &outcome.failures[0];
            assert_eq!(f.code, "ISLAND_INDETERMINATE");
            assert!(f.message.contains("reason=slice_failed"), "{}", f.message);
        });
    }

    #[test]
    fn the_one_pixel_censor_is_a_strict_boundary() {
        let grid = RasterGrid::saturn4(0.0);
        let one_pixel = vec![(0i64, Run { c0: 0, c1: 1 })];
        let two_pixel = vec![(0i64, Run { c0: 0, c1: 2 })];
        let a1 = component_area_mm2(&one_pixel, &grid);
        let a2 = component_area_mm2(&two_pixel, &grid);
        assert!((a1 - grid.pixel_area_mm2()).abs() < 1e-15);
        assert!(a1 <= grid.pixel_area_mm2(), "one pixel must NOT qualify as an island");
        assert!(a2 > grid.pixel_area_mm2(), "two pixels must qualify");
    }

    #[test]
    fn support_presence_is_pixel_overlap_at_the_same_layer_only() {
        let grid = RasterGrid::saturn4(0.0);
        let budget = Budget::default();
        let island = Primitive::Frustum {
            a: [0.0, 0.0, 0.0],
            ra: 0.5,
            b: [0.0, 0.0, 1.0],
            rb: 0.5,
        };
        let touching = Primitive::Frustum {
            a: [0.2, 0.0, 0.0],
            ra: 0.5,
            b: [0.2, 0.0, 1.0],
            rb: 0.5,
        };
        let far = Primitive::Frustum {
            a: [10.0, 0.0, 0.0],
            ra: 0.5,
            b: [10.0, 0.0, 1.0],
            rb: 0.5,
        };
        let island_r = RunRaster::from_primitives(&[island], grid, &budget).expect("island");
        let touching_r = RunRaster::from_primitives(&[touching], grid, &budget).expect("touching");
        let far_r = RunRaster::from_primitives(&[far], grid, &budget).expect("far");
        let (k0, _) = island_r.layer_range().expect("layers");
        let k = k0 + 2;
        let comp = island_r
            .layer_components(k)
            .into_iter()
            .next()
            .expect("one component");
        assert!(is_supported(&comp, &touching_r, k));
        assert!(!is_supported(&comp, &far_r, k));
        assert!(!is_attached(&comp, &island_r, k0 - 1000), "no such layer exists");
    }
}
