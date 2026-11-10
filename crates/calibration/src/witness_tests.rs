use crate::{
    build,
    section::{self, SectionMesh},
    spec::{Body, PlateSpec},
    witness,
};
use plinth_chitubox::Triangle;
use std::collections::{BTreeMap, BTreeSet};

fn written() -> (PlateSpec, plinth_chitubox::Project) {
    let spec: PlateSpec =
        serde_json::from_str(include_str!("../../../fixtures/calibration/cal1.json")).unwrap();
    let s = spec
        .specimens
        .iter()
        .find(|s| s.body == Body::Witness)
        .unwrap();
    let p = plinth_chitubox::parse(&build::specimen(s, &spec.head).to_bytes().unwrap()).unwrap();
    (spec, p)
}
fn topology(mesh: &[Triangle]) -> bool {
    let key = |p: [f32; 3]| p.map(|v| if v == 0.0 { 0 } else { v.to_bits() });
    let mut edges = BTreeMap::<_, Vec<(usize, bool)>>::new();
    let mut volume = 0.0;
    for (ti, t) in mesh.iter().enumerate() {
        let [a, b, c] = t.map(|p| p.map(f64::from));
        let u = section::sub(b, a);
        let v = section::sub(c, a);
        let cross = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        if section::dot(cross, cross) <= 1e-22 {
            eprintln!("degenerate {ti}: {t:?}");
            return false;
        }
        volume += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
        for i in 0..3 {
            let (a, b) = (key(t[i]), key(t[(i + 1) % 3]));
            edges
                .entry(if a < b { (a, b) } else { (b, a) })
                .or_default()
                .push((ti, a < b));
        }
    }
    if volume <= 0.0 {
        eprintln!("volume {volume}");
        return false;
    }
    if let Some((edge, entries)) = edges.iter().find(|(_, v)| v.len() != 2 || v[0].1 == v[1].1) {
        eprintln!("bad edge {edge:?}: {entries:?}");
        return false;
    }
    let mut neighbours = vec![vec![]; mesh.len()];
    for e in edges.values() {
        neighbours[e[0].0].push(e[1].0);
        neighbours[e[1].0].push(e[0].0);
    }
    let mut seen = BTreeSet::new();
    let mut todo = vec![0];
    while let Some(i) = todo.pop() {
        if seen.insert(i) {
            todo.extend(&neighbours[i]);
        }
    }
    seen.len() == mesh.len()
}
#[test]
fn written_w_is_one_closed_outward_surface_on_plate() {
    let (_, p) = written();
    let m = &p.models[0];
    assert!(m.supports.is_none());
    assert!(m.translate[2].abs() < f32::EPSILON);
    assert!(topology(&m.mesh));
    let bottom = m
        .mesh
        .iter()
        .flatten()
        .map(|p| p[2])
        .fold(f32::INFINITY, f32::min);
    let top = m
        .mesh
        .iter()
        .flatten()
        .map(|p| p[2])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!((top - bottom - witness::TOP).abs() < 1e-6);
    let mut sabotage = m.mesh.clone();
    sabotage.remove(100);
    assert!(!topology(&sabotage), "missing face must fail closure");
    let mut sabotage = m.mesh.clone();
    sabotage[100].swap(0, 1);
    assert!(!topology(&sabotage), "reversed face must fail orientation");
}
#[test]
fn six_written_waists_and_mesh_sabotage() {
    let (spec, p) = written();
    let s = spec
        .specimens
        .iter()
        .find(|s| s.body == Body::Witness)
        .unwrap();
    for (i, &xy) in s.contact_sites.iter().enumerate() {
        let triangles = witness::column(&p.models[0].mesh, xy);
        let measure = |triangles: &[Triangle]| {
            section::minimum(
                &[SectionMesh::new(
                    triangles,
                    [0.0, 0.0, -f64::from(witness::TOP) / 2.0],
                    [0.0, 0.0, 1.0],
                )],
                0.001,
                4.88,
                2000,
            )
        };
        let min = measure(&triangles);
        let design = 8.0
            * (std::f64::consts::TAU / 16.0).sin()
            * (f64::from(s.waist_diameters[i]) / 2.0).powi(2);
        assert!((min.area / design - 1.0).abs() < 0.10, "{i}: {min:?}");
        assert!(min.at > 4.3 && min.at < 4.68, "{i}: {min:?}");
        for z in [4.4, 4.5, 4.6] {
            let area = section::union_area(
                &[SectionMesh::new(
                    &triangles,
                    [0.0, 0.0, -f64::from(witness::TOP) / 2.0],
                    [0.0, 0.0, 1.0],
                )],
                z,
            );
            assert!((area / design - 1.0).abs() < 0.001);
        }
        let mut sabotage = triangles;
        for p in sabotage.iter_mut().flatten() {
            let z = p[2] + witness::TOP / 2.0;
            if (4.4..=4.6).contains(&z) {
                for j in 0..2 {
                    p[j] = xy[j] + (p[j] - xy[j]) * 0.5;
                }
            }
        }
        assert!(
            measure(&sabotage).area / design < 0.4,
            "mesh-only sabotage must fail ±10% criterion"
        );
    }
}
