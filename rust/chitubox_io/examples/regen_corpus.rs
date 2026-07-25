//! Regenerate the small `.chitubox` test corpus from Sphere.stl/Cylinder.stl
//! plus one donor file, using this library only.
//!
//! The ChituBox-authored originals were lost on 2026-07-24; these are
//! golden-master snapshots of THIS library's output. Keeping the generator in
//! the tree means a reader can re-derive every specimen and see exactly how it
//! was made rather than trusting an opaque blob.
//!
//! Usage:
//!   cargo run --example regen_corpus -- <stl_dir> <donor.chitubox> <out_dir>

use chitubox_io::{
    ModelGeometry, ModelSource, Placement, Project, SegmentType, SupportBatch, SupportMode,
    SupportRecord, TriangleSoup,
};
use std::path::{Path, PathBuf};

/// Cylinder placement pinned by `synth_compare.rs`.
const CYL_X: f32 = 43.7035;
const CYL_Y: f32 = 7.1448;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: regen_corpus <stl_dir> <donor.chitubox> <out_dir>");
        std::process::exit(2);
    }
    let stl_dir = PathBuf::from(&args[1]);
    let donor = PathBuf::from(&args[2]);
    let out = PathBuf::from(&args[3]);
    std::fs::create_dir_all(&out).unwrap();

    let sphere = stl_dir.join("Sphere.stl");
    let cylinder = stl_dir.join("Cylinder.stl");

    // -- 1. sphere-cylinder: exactly what synth_compare.rs builds ------------
    let base = scene(
        ModelGeometry::StlPath(sphere.clone()),
        ModelGeometry::StlPath(cylinder.clone()),
    );
    println!(
        "sphere-cylinder: tris {} / {}, placements {:?}",
        base.model(0).unwrap().triangle_count,
        base.model(1).unwrap().triangle_count,
        base.placements()
    );
    save(&base, &out, "sphere-cylinder.chitubox");

    let sphere_tris = base.model_triangles(0).unwrap();
    let cyl_tris = base.model_triangles(1).unwrap();

    // -- 2. sphere-moved: translation probe ---------------------------------
    let mut moved = scene(
        ModelGeometry::StlPath(sphere.clone()),
        ModelGeometry::StlPath(cylinder.clone()),
    );
    moved
        .set_placement(
            0,
            Placement {
                x: 10.5,
                y: 5.25,
                z_lift: 0.0,
            },
        )
        .unwrap();
    save(&moved, &out, "sphere-moved.chitubox");

    // -- 3. sphere-scaled: model 0 baked 1.5x -------------------------------
    let scaled_sphere: Vec<f32> = sphere_tris.iter().map(|v| v * 1.5).collect();
    let scaled = scene(
        ModelGeometry::Triangles(TriangleSoup::new(scaled_sphere)),
        ModelGeometry::StlPath(cylinder.clone()),
    );
    let be = extents(&sphere_tris);
    let se = extents(&scaled.model_triangles(0).unwrap());
    println!(
        "sphere-scaled: extent ratios {:?}",
        [se[0] / be[0], se[1] / be[1], se[2] / be[2]]
    );
    save(&scaled, &out, "sphere-scaled.chitubox");

    // -- 4. cylinder-rotated: model 1 baked Ry(45)*Rz(45) -------------------
    // No library op exists for an arbitrary-axis rotation, so the bake is done
    // here. `add_models` re-centers on the bbox center, which is what makes the
    // residual below non-zero; transforms.rs allows 0.005 mm.
    let rotated: Vec<f32> = cyl_tris
        .chunks_exact(3)
        .flat_map(|c| ry_rz([c[0], c[1], c[2]], 45.0))
        .collect();
    let rot = scene(
        ModelGeometry::StlPath(sphere.clone()),
        ModelGeometry::Triangles(TriangleSoup::new(rotated.clone())),
    );
    let residual = sorted_set_max_diff(&rotated, &rot.model_triangles(1).unwrap());
    println!("cylinder-rotated: re-centering residual {residual} mm (budget 0.005)");
    assert!(residual < 0.005, "re-centering residual blows the budget");
    save(&rot, &out, "cylinder-rotated.chitubox");

    // -- 5. sphere-cylinder-supported: 7 on model 0, 13 on model 1 ----------
    // The 13 is pinned by support_parity.rs::replace_empty_removes_section.
    let mut supported = scene(
        ModelGeometry::StlPath(sphere),
        ModelGeometry::StlPath(cylinder),
    );
    let s0: Vec<SupportRecord> = (0..7).map(|i| support(i, 0)).collect();
    let s1: Vec<SupportRecord> = (0..13).map(|i| support(i, 1)).collect();
    supported
        .attach_supports(&[
            SupportBatch {
                model_index: 0,
                records: s0,
                mode: SupportMode::Replace,
                lift_mm: Some(3.0),
            },
            SupportBatch {
                model_index: 1,
                records: s1,
                mode: SupportMode::Replace,
                lift_mm: Some(3.0),
            },
        ])
        .unwrap();
    println!(
        "sphere-cylinder-supported: counts {:?}",
        supported.support_counts()
    );
    save(&supported, &out, "sphere-cylinder-supported.chitubox");

    // -- 6. pad_bridge_donor: real type-6 records on a tiny carrier ---------
    // test_chitubox_emit.py clones a PAD record; pads/bridges are the only
    // record types that cannot be authored from scratch, so genuine ones are
    // lifted out of the donor and re-hosted on a small model. Its parser scans
    // the WHOLE file for segment magics and requires one contiguous run, so the
    // carrier must be single-model.
    let donor_proj = Project::from_bytes(std::fs::read(&donor).unwrap()).unwrap();
    let mut pad: Option<SupportRecord> = None;
    let mut bridge: Option<SupportRecord> = None;
    for m in 0..donor_proj.model_count() {
        for r in donor_proj.supports(m).unwrap() {
            if r.segment_type != SegmentType::PAD_OR_BRIDGE {
                continue;
            }
            let flat = (r.start[0] - r.end[0]).abs() < 1e-6 && (r.start[1] - r.end[1]).abs() < 1e-6;
            if flat && pad.is_none() {
                pad = Some(r);
            } else if !flat && bridge.is_none() {
                bridge = Some(r);
            }
        }
    }
    let (pad, bridge) = (
        pad.expect("donor has no PAD (type 6, start==end)"),
        bridge.expect("donor has no BRIDGE (type 6, start!=end)"),
    );
    println!(
        "pad_bridge_donor: pad {:?}->{:?} {} tris; bridge {:?}->{:?} {} tris",
        pad.start,
        pad.end,
        pad.mesh.triangle_count(),
        bridge.start,
        bridge.end,
        bridge.mesh.triangle_count()
    );
    let mut carrier = Project::new_empty();
    carrier
        .add_model(ModelSource {
            name: "donor.stl".into(),
            geometry: ModelGeometry::Triangles(TriangleSoup::new(cyl_tris)),
            placement: Placement::default(),
        })
        .unwrap();
    carrier
        .attach_supports(&[SupportBatch {
            model_index: 0,
            records: vec![pad, bridge],
            mode: SupportMode::Replace,
            lift_mm: Some(3.0),
        }])
        .unwrap();
    save(&carrier, &out, "pad_bridge_donor.chitubox");
}

// -- helpers ----------------------------------------------------------------

/// The two-model scene every probe specimen is a variation of.
fn scene(m0: ModelGeometry, m1: ModelGeometry) -> Project {
    let mut p = Project::new_empty();
    p.add_models(&[
        ModelSource {
            name: "Sphere.stl".into(),
            geometry: m0,
            placement: Placement {
                x: 0.0,
                y: 0.0,
                z_lift: 0.0,
            },
        },
        ModelSource {
            name: "Cylinder.stl".into(),
            geometry: m1,
            placement: Placement {
                x: CYL_X,
                y: CYL_Y,
                z_lift: 0.0,
            },
        },
    ])
    .unwrap();
    p
}

/// A plausible 8-sided frustum support standing under the plate.
fn support(i: usize, model: usize) -> SupportRecord {
    let a = (i as f32) * 0.7 + (model as f32) * 3.1;
    let (x, y) = (a.cos() * 4.0, a.sin() * 4.0);
    let start = [x, y, -8.0 + (i as f32) * 0.1];
    let end = [x, y, 2.0 + (i as f32) * 0.1];
    let (r0, r1) = (0.6, 0.35);
    let ty = match i % 4 {
        0 => SegmentType::TIP,
        1 => SegmentType::SHAFT,
        2 => SegmentType::TAPER,
        _ => SegmentType::FLARE,
    };
    let rec = SupportRecord::new(ty, start, end, r0, r1, frustum(start, end, r0, r1, 8));
    if ty == SegmentType::TAPER {
        rec.with_contact_depth(0.25)
    } else {
        rec
    }
}

/// Capped frustum triangle soup: 2 wall triangles per facet + a fan cap at
/// each end.
fn frustum(a: [f32; 3], b: [f32; 3], r0: f32, r1: f32, sides: usize) -> TriangleSoup {
    let mut t: Vec<f32> = Vec::with_capacity(sides * 4 * 9);
    let ring = |c: [f32; 3], r: f32, k: usize| -> [f32; 3] {
        let th = std::f32::consts::TAU * (k as f32) / (sides as f32);
        [c[0] + r * th.cos(), c[1] + r * th.sin(), c[2]]
    };
    let mut push = |p: [f32; 3], q: [f32; 3], s: [f32; 3]| {
        t.extend_from_slice(&[p[0], p[1], p[2], q[0], q[1], q[2], s[0], s[1], s[2]]);
    };
    for k in 0..sides {
        let (a0, a1) = (ring(a, r0, k), ring(a, r0, k + 1));
        let (b0, b1) = (ring(b, r1, k), ring(b, r1, k + 1));
        push(a0, b0, b1);
        push(a0, b1, a1);
        push(a, a1, a0); // bottom cap
        push(b, b0, b1); // top cap
    }
    TriangleSoup::new(t)
}

fn ry_rz(v: [f32; 3], deg: f32) -> [f32; 3] {
    let (s, c) = deg.to_radians().sin_cos();
    let (x1, y1, z1) = (v[0] * c - v[1] * s, v[0] * s + v[1] * c, v[2]);
    [x1 * c + z1 * s, y1, -x1 * s + z1 * c]
}

fn extents(xyz: &[f32]) -> [f32; 3] {
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for v in xyz.chunks_exact(3) {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]
}

fn sorted_set_max_diff(a: &[f32], b: &[f32]) -> f32 {
    let sort = |x: &[f32]| {
        let mut p: Vec<[f32; 3]> = x.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
        p.sort_by(|u, v| {
            u[0].total_cmp(&v[0])
                .then(u[1].total_cmp(&v[1]))
                .then(u[2].total_cmp(&v[2]))
        });
        p
    };
    let (pa, pb) = (sort(a), sort(b));
    assert_eq!(pa.len(), pb.len(), "vertex count mismatch");
    let mut m = 0.0f32;
    for (u, v) in pa.iter().zip(&pb) {
        for k in 0..3 {
            m = m.max((u[k] - v[k]).abs());
        }
    }
    m
}

fn save(p: &Project, dir: &Path, name: &str) {
    let path = dir.join(name);
    p.save(&path).unwrap_or_else(|e| panic!("save {name}: {e}"));
    println!("  wrote {} ({} bytes)", path.display(), path.metadata().unwrap().len());
}
