//! Direct native records; the pure geom shape kernel supplies every support mesh.
use crate::spec::{Body, Head, Specimen};
use plinth_chitubox::{
    Model, Preset, Project, RecordType as Ty, SupportRecord, Supports, Triangle,
};
use plinth_geom::shapes;

pub const PARTS_PER_HEAD: usize = 7;

fn record(
    ty: Ty,
    start: [f32; 3],
    end: [f32; 3],
    r1: f32,
    r2: f32,
    mesh: Vec<Triangle>,
) -> SupportRecord {
    SupportRecord {
        record_type: ty,
        start,
        end,
        r1,
        r2,
        tail: [0; 6],
        mesh,
    }
}

fn rod(ty: Ty, start: [f32; 3], end: [f32; 3], r1: f32, r2: f32) -> SupportRecord {
    record(ty, start, end, r1, r2, shapes::frustum(start, end, r1, r2))
}

fn ball(ty: Ty, c: [f32; 3], r: f32) -> SupportRecord {
    let a = [c[0], c[1], c[2] + r];
    let b = [c[0], c[1], c[2] - r];
    record(ty, a, b, r, r, shapes::sphere(a, b, r))
}

/// From contact to plate: ball, constant-radius type-1 tip, junction, fixed cone,
/// joint, column, base flare. Raft pads/strips are added for the complete set below.
#[must_use]
pub fn head(h: &Head, xy: [f32; 2], diameter: f32, lift: f32, face_z: f32) -> Vec<SupportRecord> {
    let at = |z| [xy[0], xy[1], z];
    // Build in the record frame, never translate a finished f32 mesh: that can
    // move its vertices beyond the native canonical-shape ULP allowance.
    let c = face_z - h.outside;
    let j = c - h.waist_length;
    let k = j - h.cone_length;
    let mut waist = rod(Ty::TIP_CONE, at(c), at(j), diameter / 2.0, diameter / 2.0);
    waist.tail[0] = (h.ball_diameter / 2.0 - h.outside).to_bits();
    vec![
        ball(Ty::TIP_BALL, at(c), h.ball_diameter / 2.0),
        waist,
        ball(Ty::JOINT_BALL, at(j), h.junction_diameter / 2.0),
        rod(
            Ty::ROD,
            at(j),
            at(k),
            h.cone_start_diameter / 2.0,
            h.cone_end_diameter / 2.0,
        ),
        ball(Ty::JOINT_BALL, at(k), 0.52),
        rod(
            Ty::ROD,
            at(k),
            at(face_z + 1.198 - lift),
            h.column_diameter / 2.0,
            h.column_diameter / 2.0,
        ),
        rod(
            Ty::BASE_FLARE,
            at(face_z + 1.2 - lift),
            at(face_z + 0.2 - lift),
            h.column_diameter / 2.0,
            1.1,
        ),
    ]
}

/// Prim MST, deterministic tie-break by index. A strip goes only pad-centre to
/// pad-centre, 2.2 mm wide; pads are native 20-gons of diameter 2.2 and height 0.2.
fn raft(s: &mut Supports, sites: &[[f32; 2]], lift: f32) {
    let mut joined = vec![false; sites.len()];
    joined[0] = true;
    for _ in 1..sites.len() {
        let mut best = (f32::INFINITY, 0, 0);
        for (i, a) in sites.iter().enumerate().filter(|(i, _)| joined[*i]) {
            for (j, b) in sites.iter().enumerate().filter(|(j, _)| !joined[*j]) {
                let d = (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2);
                if d < best.0 {
                    best = (d, i, j);
                }
            }
        }
        let (a, b) = (sites[best.1], sites[best.2]);
        let mut r = record(
            Ty::RAFT,
            [a[0], a[1], 0.0],
            [b[0], b[1], 0.0],
            2.2,
            0.2,
            shapes::bridge(a, b, 0.2 - lift, -lift, 1.1),
        );
        r.tail[0] = 1.0f32.to_bits();
        r.tail[1] = 4.0f32.to_bits();
        s.records.push(r);
        s.attrs.push(0x1e00);
        joined[best.2] = true;
    }
    for &xy in sites {
        let p = [xy[0], xy[1], 0.0];
        let mut r = record(
            Ty::RAFT,
            p,
            p,
            2.2,
            0.2,
            shapes::pad(xy, 0.2 - lift, -lift, 1.1),
        );
        r.tail[1] = 4.0f32.to_bits();
        s.records.push(r);
        s.attrs.push(0x1e00);
    }
}

/// One specimen per project. W deliberately has no model mesh: no dummy contact/body
/// is inserted just to coax a favourable checker verdict out of an unsupported use case.
#[must_use]
pub fn specimen(s: &Specimen, h: &Head) -> Project {
    let (mesh, supports) = match s.body {
        Body::K15 => (control(0.798, 0.60), None),
        Body::K60 => (control(0.399, 0.70), None),
        _ => {
            let mesh = match s.body {
                Body::Block => box_mesh(4.25, 4.25, -2.5, 2.5),
                Body::Strip => box_mesh(12.0, 1.5, -2.5, 2.5),
                _ => vec![],
            };
            // Native bodies are centred on their bounding box. Build the supports
            // directly in that frame. W has no body, so its virtual face is z=0.
            let face_z = if s.body == Body::Witness { 0.0 } else { -2.5 };
            let mut sup = Supports {
                presets: vec![Preset {
                    id: 1,
                    name: "Light".into(),
                }],
                ..Supports::default()
            };
            for (&xy, &d) in s.contact_sites.iter().zip(&s.waist_diameters) {
                sup.records.extend(head(h, xy, d, s.lift, face_z));
                sup.attrs.extend([0x0104; PARTS_PER_HEAD]);
            }
            raft(&mut sup, &s.contact_sites, s.lift - face_z);
            (mesh, Some(sup))
        }
    };
    Project {
        preview: Some(crate::blank_preview()),
        models: vec![Model {
            name: format!("{}; {}", s.id, crate::LABEL),
            translate: [s.pose[0], s.pose[1], s.lift],
            mesh,
            supports,
        }],
        ..Project::default()
    }
}

fn box_mesh(x: f32, y: f32, lo: f32, hi: f32) -> Vec<Triangle> {
    shapes::bridge([-x / 2.0, 0.0], [x / 2.0, 0.0], hi, lo, y / 2.0)
}

/// One watertight surface of revolution, not overlapping STL shells. 48-gon sections.
/// Base (choice [S]) is an 8 mm diameter pedestal, top 4.50 mm; free arm stays 4 mm.
fn control(arm: f32, flare: f32) -> Vec<Triangle> {
    let cap = (29.9978f64 / (24.0 * (std::f64::consts::TAU / 48.0).sin())).sqrt() as f32;
    let profile = [
        (4.0, 0.0),
        (4.0, 4.5),
        (arm, 4.5),
        (arm, 8.5),
        (cap, 8.5 + flare),
        (cap, 9.0 + flare),
    ];
    let ring = shapes::math::unit_ring(48);
    let rings: Vec<Vec<[f32; 3]>> = profile
        .iter()
        .map(|&(r, z)| ring.iter().map(|&(c, s)| [r * c, r * s, z]).collect())
        .collect();
    let mut out = Vec::new();
    for pair in rings.windows(2) {
        for i in 0..48 {
            let j = (i + 1) % 48;
            out.extend([
                [pair[0][i], pair[0][j], pair[1][j]],
                [pair[0][i], pair[1][j], pair[1][i]],
            ]);
        }
    }
    for i in 0..48 {
        let j = (i + 1) % 48;
        out.push([[0.0, 0.0, 0.0], rings[0][j], rings[0][i]]);
        out.push([[0.0, 0.0, profile[5].1], rings[5][i], rings[5][j]]);
    }
    let centre = profile[5].1 / 2.0;
    for p in out.iter_mut().flatten() {
        p[2] -= centre;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_shapes_match_local_fields_and_raft_is_the_connected_mst() {
        let spec: crate::spec::PlateSpec =
            serde_json::from_str(include_str!("../../../fixtures/calibration/cal1.json")).unwrap();
        spec.validate().unwrap();
        for s in spec
            .specimens
            .iter()
            .filter(|s| !s.contact_sites.is_empty())
        {
            let p = plinth_chitubox::parse(&specimen(s, &spec.head).to_bytes().unwrap()).unwrap();
            let sup = p.models[0].supports.as_ref().unwrap();
            let n = s.contact_sites.len();
            let mut edges = vec![];
            let mut pads = 0;
            for (r, attr) in sup.records.iter().zip(&sup.attrs) {
                let canonical = match r.record_type {
                    Ty::TIP_BALL | Ty::JOINT_BALL => shapes::sphere(r.start, r.end, r.r1),
                    Ty::TIP_CONE | Ty::ROD | Ty::BASE_FLARE => {
                        shapes::frustum(r.start, r.end, r.r1, r.r2)
                    }
                    Ty::RAFT => {
                        assert_eq!(*attr, 0x1e00);
                        assert!(r.start[2].abs() < 1e-9 && r.end[2].abs() < 1e-9);
                        assert!((r.r1 - 2.2).abs() < 1e-6 && (r.r2 - 0.2).abs() < 1e-6);
                        let top = r
                            .mesh
                            .iter()
                            .flatten()
                            .map(|p| p[2])
                            .fold(f32::NEG_INFINITY, f32::max);
                        let bottom = r
                            .mesh
                            .iter()
                            .flatten()
                            .map(|p| p[2])
                            .fold(f32::INFINITY, f32::min);
                        let a = [r.start[0], r.start[1]];
                        if r.tail[0] == 0 {
                            pads += 1;
                            assert_eq!(r.mesh.len(), 80);
                            shapes::pad(a, top, bottom, 1.1)
                        } else {
                            let b = [r.end[0], r.end[1]];
                            edges.push((a, b));
                            shapes::bridge(a, b, top, bottom, 1.1)
                        }
                    }
                    _ => panic!("unexpected calibration record"),
                };
                assert!(
                    shapes::mesh_error_ulps(&r.mesh, &canonical).unwrap().abs() < 1e-9,
                    "{} type {}: build from fields in the final frame",
                    s.id,
                    r.record_type.0
                );
            }
            assert_eq!(pads, n);
            assert_eq!(edges.len(), n - 1);
            let length: f32 = edges
                .iter()
                .map(|(a, b)| (a[0] - b[0]).hypot(a[1] - b[1]))
                .sum();
            let pitch = if s.body == Body::Strip { 3.0 } else { 2.5 };
            assert!((length - (n - 1) as f32 * pitch).abs() < 1e-6);
            let mut reached = vec![s.contact_sites[0]];
            for _ in 0..n {
                for &(a, b) in &edges {
                    if reached.contains(&a) && !reached.contains(&b) {
                        reached.push(b);
                    }
                    if reached.contains(&b) && !reached.contains(&a) {
                        reached.push(a);
                    }
                }
            }
            assert!(s.contact_sites.iter().all(|p| reached.contains(p)));
        }
    }
}
