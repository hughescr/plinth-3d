//! Exercises the shared products against the real emitted bytes.
//!
//! The checks are stubs, so nothing here grades a file. What it does is prove
//! the products the eight implementing agents will consume actually run on the
//! corpus, at a cost they can afford: the reader, the weld, the support graph,
//! the seat reconstruction, both rasters and the ownership partition.
//!
//! Everything skips cleanly when `exports/` is absent, because run outputs are
//! not in git.

use plinth_audit::context::{AuditContext, AuditOptions, ModelContext};
use plinth_audit::mesh::Inside;
use plinth_audit::raster::{Budget, RunRaster};
use plinth_audit::reader::ChituboxFile;
use std::path::{Path, PathBuf};

fn exports() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("exports")
}

fn artifact(name: &str) -> Option<PathBuf> {
    let p = exports().join(name);
    p.exists().then_some(p)
}

#[test]
fn every_emitted_artifact_parses_and_reconstructs() {
    let Ok(entries) = std::fs::read_dir(exports()) else {
        return;
    };
    let mut seen = 0usize;
    for e in entries.filter_map(Result::ok) {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("chitubox") {
            continue;
        }
        let file = ChituboxFile::open(&p).unwrap_or_else(|err| panic!("{}: {err}", p.display()));
        let options = AuditOptions::default();
        let audit = AuditContext {
            file: &file,
            options: &options,
            sidecar: None,
            sidecar_absence: Some("none on disk".into()),
        };
        for mi in 0..file.models.len() {
            let ctx = ModelContext::build(&audit, mi);
            assert!(
                ctx.model_mesh.closure.is_closed(),
                "{}: model mesh is not closed: {}",
                p.display(),
                ctx.model_mesh.closure.summary()
            );
            // The frame must be established: no support below the plate.
            let frame = ctx.frame_check();
            assert!(
                frame.is_established(),
                "{}: {:.6} mm of support below the plate",
                p.display(),
                frame.support_below_plate_mm
            );
            // Every record must yield an analytic solid, or be a named
            // degeneracy -- never a silent skip.
            for (i, prim) in ctx.primitives.iter().enumerate() {
                if let Err(err) = prim {
                    eprintln!(
                        "{} record {i} ({}) is degenerate: {err:?}",
                        p.display(),
                        ctx.model.records[i].kind.label()
                    );
                }
            }
            // Every seat must be established or carry a reason token.
            for s in &ctx.seats {
                assert!(
                    s.h_delivered_mm.is_some() || s.indeterminate.is_some(),
                    "{}: contact {} has neither a depth nor a reason",
                    p.display(),
                    s.contact_id
                );
            }
        }
        seen += 1;
    }
    assert!(seen > 0, "no artifacts found; nothing was exercised");
}

#[test]
fn the_sphere_reconstruction_reproduces_the_measured_facts() {
    let Some(p) = artifact("sphere-first-artifact.chitubox") else {
        return;
    };
    let file = ChituboxFile::open(&p).expect("parse");
    // audit-contract.md §1.6: our emitter leaves no unreferenced mesh bytes,
    // no duplicate offsets and no overlaps, and every measured all-zero span
    // is zero.
    assert_eq!(file.findings.total(), 0, "{:?}", file.findings);
    // §2.1: every emitted ball carries exactly 224 triangles, which is also
    // what classifies the file as emitted rather than native.
    assert_eq!(
        file.ball_triangle_counts
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![224]
    );

    let options = AuditOptions::default();
    let audit = AuditContext {
        file: &file,
        options: &options,
        sidecar: None,
        sidecar_absence: None,
    };
    let ctx = ModelContext::build(&audit, 0);
    // The three-way tip radius triangulation of §3.1 must agree on every
    // contact: R/f from the TIP, r_start of the TAPER, and the sphere fitted
    // to the TIP soup.
    for s in &ctx.seats {
        assert!(
            s.r_tip_agrees,
            "contact {}: three tip radii spread by {} mm against a tolerance of {}",
            s.contact_id, s.r_tip_spread_mm, s.r_tip_tol_mm
        );
    }
    // Every taper declares 0.300 mm -- the known lie this audit exists to
    // measure against delivered geometry.
    for s in &ctx.seats {
        let declared = s.declared_depth_mm.expect("a taper per tip");
        assert!(
            (declared - 0.3).abs() < 1e-6,
            "contact {} declares {declared}",
            s.contact_id
        );
    }
    // And the delivered depths are a mix, which is the whole finding.
    let zero = ctx
        .seats
        .iter()
        .filter(|s| s.h_delivered_mm == Some(0.0))
        .count();
    let established = ctx
        .seats
        .iter()
        .filter(|s| s.indeterminate.is_none())
        .count();
    eprintln!(
        "sphere: {} contacts, {} established, {} at exactly zero delivered depth",
        ctx.seats.len(),
        established,
        zero
    );
    assert_eq!(
        established,
        ctx.seats.len(),
        "every seat must be established or carry a reason"
    );
}

#[test]
fn both_rasters_build_on_a_real_artifact_and_agree_with_the_solids() {
    let Some(p) = artifact("sphere-first-artifact.chitubox") else {
        return;
    };
    let file = ChituboxFile::open(&p).expect("parse");
    let options = AuditOptions::default();
    let audit = AuditContext {
        file: &file,
        options: &options,
        sidecar: None,
        sidecar_absence: None,
    };
    let ctx = ModelContext::build(&audit, 0);

    let model = ctx.model_raster().expect("model raster");
    let support = ctx.support_raster().expect("support raster");
    assert!(model.pixel_count > 0 && support.pixel_count > 0);

    // The cured model volume must be at or below the mesh's own volume: full
    // containment erodes, it never adds.
    let mesh_vol = ctx.model_mesh.closed_volume_mm3().expect("closed");
    let cured = model.volume_mm3();
    assert!(
        cured <= mesh_vol,
        "cured {cured} exceeds mesh volume {mesh_vol}"
    );
    assert!(
        cured > mesh_vol * 0.9,
        "cured {cured} is far below mesh volume {mesh_vol}; the slicer is losing material"
    );

    // The support raster must be below the top of the model and at or above
    // the plate: a cured voxel under the plate is impossible.
    let (k0, _) = support.layer_range().expect("layers");
    assert!(
        support.grid.layer_z_mm(k0) >= ctx.z_plate_mm - support.grid.layer_height_mm,
        "support cures below the plate"
    );

    // Spot-check the raster against the analytic solids it came from: a cured
    // voxel centre must lie inside at least one support primitive.
    let sample_layer = &support.layers[support.layers.len() / 2];
    let mut checked = 0usize;
    for (&row, runs) in sample_layer.rows.iter().take(8) {
        for r in runs {
            let col = r.c0;
            let b = support.grid.voxel_box(sample_layer.k, row, col);
            let centre = b.center();
            let inside_any = ctx
                .live_primitives()
                .iter()
                .any(|(_, prim)| prim.contains(centre));
            assert!(
                inside_any,
                "cured voxel centre {centre:?} is in no primitive"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "no cured voxels were spot-checked");
    eprintln!(
        "sphere raster: model {:.3} mL cured vs {:.3} mL mesh, support {:.3} mL, \
         {} model runs, {} support runs, {checked} voxels spot-checked",
        model.volume_ml(),
        mesh_vol / 1000.0,
        support.volume_ml(),
        model.run_count,
        support.run_count
    );
}

#[test]
fn the_ownership_partition_conserves_the_demand_it_was_given() {
    let Some(p) = artifact("sphere-first-artifact.chitubox") else {
        return;
    };
    let file = ChituboxFile::open(&p).expect("parse");
    let options = AuditOptions::default();
    let audit = AuditContext {
        file: &file,
        options: &options,
        sidecar: None,
        sidecar_absence: None,
    };
    let ctx = ModelContext::build(&audit, 0);
    let demand = ctx.demand();
    let own = ctx.ownership();
    assert!(own.budget_exceeded.is_none(), "{:?}", own.budget_exceeded);
    // I4 clause (b), the partition identity: a Voronoi partition cannot
    // double-count, so a violation is a defect in the instrument itself.
    let slack = (own.accounted_mm2 - demand.total_mm2).abs();
    assert!(
        slack <= plinth_audit::consts::tol_sum(demand.total_mm2),
        "partition lost {slack} mm2 of {} mm2",
        demand.total_mm2
    );
    eprintln!(
        "sphere ownership: D_total {:.4} mm2 over {} down-facing triangles, \
         {} samples, {:.4} mm2 unowned across {} orphan components",
        demand.total_mm2,
        demand.down_facing_triangles,
        own.samples,
        own.a_unowned_mm2,
        own.components_without_contact.len()
    );
}

#[test]
fn the_inside_test_is_decided_across_a_real_model() {
    let Some(p) = artifact("make_box-first-artifact.chitubox") else {
        return;
    };
    let file = ChituboxFile::open(&p).expect("parse");
    let mesh = plinth_audit::mesh::Mesh::weld_exact(&file.models[0].model_soup);
    assert!(mesh.closure.is_closed());
    // A lattice over the bounding box: every query must be decided. An
    // indeterminate answer is a failure of the instrument, not of the model.
    let bb = mesh.bbox;
    let ext = bb.extent();
    let mut indeterminate = 0usize;
    let mut inside = 0usize;
    let n = 9;
    for i in 1..n {
        for j in 1..n {
            for k in 1..n {
                let q = [
                    bb.min[0] + ext[0] * f64::from(i) / f64::from(n),
                    bb.min[1] + ext[1] * f64::from(j) / f64::from(n),
                    bb.min[2] + ext[2] * f64::from(k) / f64::from(n),
                ];
                match mesh.point_inside(q) {
                    Inside::Indeterminate => indeterminate += 1,
                    Inside::Inside => inside += 1,
                    Inside::Outside => {}
                }
            }
        }
    }
    assert_eq!(indeterminate, 0, "the parity test left queries undecided");
    assert!(inside > 0, "nothing was found inside a solid model");
}

#[test]
fn a_native_file_reconstructs_without_being_mistaken_for_an_emitted_one() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("scripts/tests/fixtures/chitubox/two-supports.chitubox");
    if !p.exists() {
        return;
    }
    let file = ChituboxFile::open(&p).expect("parse");
    assert_eq!(
        file.file_class,
        plinth_audit::reader::FileClass::Native,
        "native balls carry 480 triangles, not our 224"
    );
    assert!(
        (file.file_class.join_tol_mm() - plinth_audit::consts::JOIN_TOL_NATIVE_MM).abs() < 1e-15,
        "a native file must get the looser endpoint tolerance"
    );
    let options = AuditOptions::default();
    let audit = AuditContext {
        file: &file,
        options: &options,
        sidecar: None,
        sidecar_absence: None,
    };
    let ctx = ModelContext::build(&audit, 0);
    // Native ChituBox writes its raft records at z = 0 while the rest of the
    // support tree is in the model-centred frame. That is a FINDING about the
    // file class, reported and not gated; the frame itself is still
    // established because no support reaches below the plate.
    //
    // Re-recorded 2026-09-29 (fix/sweep-raft): `raft_agrees` Some(false) ->
    // Some(true). The audit now reads a raft record at exactly z = 0 as the
    // native raft frame -- the raft on the plate -- because every native file
    // (and, from the same change, plinth's own emitter) writes that frame
    // while the raft MESH sits on the plate (`raft-diagnosis.md` §4.2).
    // Measured on this file: the pads' mesh bottoms are at the plate,
    // -20.3205 mm, and their records at z = 0.
    let frame = ctx.frame_check();
    assert_eq!(frame.raft_agrees, Some(true));
    assert_eq!(
        frame
            .min_raft_z_mm
            .map(|z| (z - frame.z_plate_mm).abs() <= frame.tolerance_mm),
        Some(true),
        "{frame:?}"
    );
    assert!(frame.is_established(), "{frame:?}");
}

#[test]
fn the_largest_model_in_the_corpus_stays_inside_the_raster_budget() {
    let Some(p) = artifact("weeping-treant-v1.2-first-artifact.chitubox") else {
        return;
    };
    let file = ChituboxFile::open(&p).expect("parse");
    let options = AuditOptions::default();
    let audit = AuditContext {
        file: &file,
        options: &options,
        sidecar: None,
        sidecar_absence: None,
    };
    let ctx = ModelContext::build(&audit, 0);
    eprintln!(
        "treant: {} model triangles, {} support records, {} contacts",
        ctx.model_mesh.tri_count(),
        ctx.model.records.len(),
        ctx.seats.len()
    );
    // Only the support raster is exercised here: it is the cheaper of the two
    // and it proves the budget accounting on the corpus's largest plan. The
    // model raster of a 429k-face model is a check's own cost to pay, and the
    // budget refuses rather than truncating if it is too large.
    let prims: Vec<_> = ctx.live_primitives().into_iter().map(|(_, p)| p).collect();
    match RunRaster::from_primitives(&prims, ctx.grid, &Budget::default()) {
        Ok(r) => eprintln!(
            "treant support raster: {:.3} mL, {} runs, {} layers",
            r.volume_ml(),
            r.run_count,
            r.layers.len()
        ),
        Err(e) => panic!("the corpus's largest plan must fit the default budget: {e}"),
    }
}
