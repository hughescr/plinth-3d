use super::*;
use plinth_geom::Mesh;
use plinth_geom::measure::Measure;
use plinth_geom::shapes::{frustum, pad, sphere};
use plinth_geom::slice::LayerGrid;
use plinth_support::load::needs::{OVERHANG_DEG, SAMPLE_SPACING_MM, overhang_points};
use plinth_support::load::{
    ContactView, LayoutView, PartChain, PartKind, PartSection, StackOptions, build_model_layers,
};
use plinth_support::report_data::{ProcessValues, ReportData, SupportsSummary};

/// Round 25 on the plate: the approved STL lifted so its lowest point is at 5.0 mm.
fn posed_round25() -> Mesh {
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/round/approved/Round 25.keepup.stl"
    );
    let m = plinth_geom::read_stl_file(std::path::Path::new(p)).expect("fixture");
    let dz = 5.0 - m.bounds().min[2];
    Mesh::new(
        m.xs().to_vec(),
        m.ys().to_vec(),
        m.zs().iter().map(|z| z + dz).collect(),
        m.indices().to_vec(),
    )
    .expect("mesh")
}

fn tris_of(m: &Mesh) -> Vec<Tri> {
    (0..m.triangle_count())
        .map(|t| {
            let c = m.triangle_corners(t);
            [0, 1, 2].map(|k| [c[k][0] as f32, c[k][1] as f32, c[k][2] as f32])
        })
        .collect()
}

/// A stand-in layout and its parts: not a plan, only something to draw and to analyse.
fn stand_in(
    mesh: &Mesh,
    ml: &plinth_support::load::ModelLayers,
    needs: &plinth_support::load::needs::NeedsMap,
) -> (LayoutView, Vec<Part>) {
    let mut pts: Vec<_> = needs.points.iter().collect();
    pts.sort_by(|a, b| {
        a.xyz[2]
            .total_cmp(&b.xyz[2])
            .then(a.xyz[0].total_cmp(&b.xyz[0]))
    });
    let mut chosen: Vec<&plinth_support::load::needs::NeedPoint> = Vec::new();
    for p in pts {
        let near = chosen
            .iter()
            .any(|c| (0..3).map(|a| (c.xyz[a] - p.xyz[a]).powi(2)).sum::<f64>() < 2.2 * 2.2);
        if !near {
            chosen.push(p);
        }
    }
    let _ = mesh;
    let (mut contacts, mut chains, mut parts) = (Vec::new(), Vec::new(), Vec::new());
    let mut raft_xy: Vec<[f32; 2]> = Vec::new();
    for (i, p) in chosen.iter().enumerate() {
        let c = [
            p.xyz[0] + 0.1 * p.normal[0],
            p.xyz[1] + 0.1 * p.normal[1],
            p.xyz[2] + 0.1 * p.normal[2],
        ];
        let k = ((p.xyz[2] - ml.grid.origin) / ml.grid.height)
            .floor()
            .max(0.0) as u32;
        let rim = p.xyz[1] > 7.0 || p.xyz[2] > 14.0;
        contacts.push(ContactView {
            centre: c,
            patch_lower_mm2: 0.1 + 0.05 * (i % 7) as f64,
            neck_area_mm2: 0.12,
            first_layer: k,
            site_class: if rim {
                SiteClass::PartlyVisibleRim
            } else {
                SiteClass::HiddenUnderside
            },
        });
        let sec = |kind, area, z: [f64; 2]| PartSection {
            kind,
            record: None,
            area_lower_mm2: area,
            z_range_mm: z,
        };
        let zc = c[2];
        chains.push(PartChain {
            contact: i as u32,
            parts: vec![
                sec(PartKind::Patch, 0.2, [zc, zc + 0.3]),
                sec(PartKind::Neck, 0.12, [zc - 0.4, zc]),
                sec(PartKind::Column, 0.2, [0.0, zc - 0.4]),
            ],
        });
        let (x, y, z) = (c[0] as f32, c[1] as f32, c[2] as f32);
        let r = 0.2_f32;
        let own = Some(i as u32);
        parts.push(Part {
            tris: sphere([x, y, z - r], [x, y, z + r], r),
            role: Role::TipBall,
            contact: own,
        });
        parts.push(Part {
            tris: frustum([x, y, z - 0.1], [x, y, z - 1.4], 0.12, 0.25),
            role: Role::TipCone,
            contact: own,
        });
        parts.push(Part {
            tris: frustum([x, y, z - 1.4], [x, y, 1.0], 0.25, 0.25),
            role: Role::Column,
            contact: own,
        });
        parts.push(Part {
            tris: frustum([x, y, 1.0], [x, y, 0.2], 0.25, 1.1),
            role: Role::Flare,
            contact: own,
        });
        raft_xy.push([x, y]);
    }
    for w in raft_xy.windows(2) {
        parts.push(Part {
            tris: plinth_geom::shapes::bridge(
                [w[0][0], w[0][1]],
                [w[1][0], w[1][1]],
                0.2,
                0.0,
                2.2,
            ),
            role: Role::Raft,
            contact: None,
        });
    }
    for xy in &raft_xy {
        parts.push(Part {
            tris: pad(*xy, 0.2, 0.0, 1.1),
            role: Role::Raft,
            contact: None,
        });
    }
    (LayoutView { contacts, chains }, parts)
}

#[test]
fn all_twelve_pictures_are_made_from_a_scene_and_the_report_data() {
    let mesh = posed_round25();
    let grid = LayerGrid::covering(&mesh, 0.05).expect("grid");
    let Measure::Known(ml) = build_model_layers(&mesh, &grid, &StackOptions::default()) else {
        panic!("unverified");
    };
    let mut needs = overhang_points(&mesh, OVERHANG_DEG, SAMPLE_SPACING_MM);
    for p in &mut needs.points {
        p.class = if p.angle_deg > 40.0 && p.xyz[1] > 5.0 {
            SiteTag::PartlyVisible
        } else {
            SiteTag::Hidden
        };
    }
    let (layout, parts) = stand_in(&mesh, &ml, &needs);
    let ld = LoadData::build(&ml.stack, layout, ml.structure.clone(), needs);
    let data = ReportData {
        process: ProcessValues {
            layer_height_mm: 0.05,
            layer_height_source: "owner default profile".into(),
            design_ratio: 12.0,
            h1_admission_mm2: 0.48,
            coverage_reach_mm: 3.0,
            overhang_deg: 45.0,
            lift_mm: 5.0,
        },
        warnings: ld.detail.report.warnings.clone(),
        load: Some(ld.detail.report.clone()),
        load_data: Some(ld),
        supports: SupportsSummary::default(),
        ..ReportData::default()
    };
    let scene = Scene {
        model: tris_of(&mesh),
        parts,
    };
    let t = std::time::Instant::now();
    let pics = render_pictures(&scene, &data);
    eprintln!("12 pictures in {:.2} s", t.elapsed().as_secs_f64());
    assert_eq!(pics.len(), 12);
    let names: std::collections::BTreeSet<&str> = pics.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names.len(), 12);
    for p in &pics {
        assert_eq!((p.image.w, p.image.h), (W, H), "{}", p.name);
        assert!(
            p.image
                .rgb
                .chunks_exact(3)
                .filter(|c| **c != BACKGROUND)
                .count()
                > 20_000,
            "{} is nearly empty",
            p.name
        );
        assert!(!p.caption.is_empty() && !p.title.is_empty());
    }
    let dir = std::env::temp_dir().join("plinth-render-test");
    let written = crate::write_all(&dir, &pics).expect("write");
    for w in &written {
        let bytes = std::fs::read(w).expect("read back");
        assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);
    }
    eprintln!("pictures in {}", dir.display());
}

#[test]
fn without_load_data_the_load_pictures_say_unverified() {
    let scene = Scene::default();
    let data = ReportData::default();
    let pics = render_pictures(&scene, &data);
    assert_eq!(pics.len(), 12);
    let unverified: Vec<&str> = pics
        .iter()
        .filter(|p| {
            p.caption.contains("no load data")
                || p.title.contains("Worst")
                || p.caption.starts_with("no")
        })
        .map(|p| p.name.as_str())
        .collect();
    assert!(
        unverified.contains(&"07-worst-load") && unverified.contains(&"06-worst-contact"),
        "{unverified:?}"
    );
}
