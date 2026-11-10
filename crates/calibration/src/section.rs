//! Written-triangle section measurements, independent of record radii. Each native
//! part is convex. Sections are hulls of triangle/plane intersections; overlapping
//! sections are unioned by inclusion/exclusion of convex polygon intersections.
//! This is a calibration measurement, not a replacement for plinth-check.
use plinth_chitubox::Triangle;
type V = [f64; 3];
type P = [f64; 2];
pub fn sub(a: V, b: V) -> V {
    std::array::from_fn(|i| a[i] - b[i])
}
pub fn dot(a: V, b: V) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(a: V) -> V {
    let l = dot(a, a).sqrt();
    a.map(|v| v / l)
}
fn side(a: P, b: P, p: P) -> f64 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}
fn hull(mut p: Vec<P>) -> Vec<P> {
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-10 && (a[1] - b[1]).abs() < 1e-10);
    if p.len() < 3 {
        return vec![];
    }
    let mut h: Vec<P> = vec![];
    for pass in 0..2 {
        let start = h.len();
        for &q in if pass == 0 {
            p.clone()
        } else {
            p.iter().rev().copied().collect()
        }
        .iter()
        {
            while h.len() >= start + 2 && side(h[h.len() - 2], h[h.len() - 1], q) <= 1e-15 {
                h.pop();
            }
            h.push(q);
        }
        h.pop();
    }
    h
}
fn area(p: &[P]) -> f64 {
    (0..p.len())
        .map(|i| {
            let j = (i + 1) % p.len();
            p[i][0] * p[j][1] - p[j][0] * p[i][1]
        })
        .sum::<f64>()
        .abs()
        / 2.0
}
fn intersection(mut p: Vec<P>, q: &[P]) -> Vec<P> {
    for i in 0..q.len() {
        if p.is_empty() {
            break;
        }
        let (a, b) = (q[i], q[(i + 1) % q.len()]);
        let prev = std::mem::take(&mut p);
        for j in 0..prev.len() {
            let (s, e) = (prev[j], prev[(j + 1) % prev.len()]);
            let (ds, de) = (side(a, b, s), side(a, b, e));
            if ds >= 0.0 {
                p.push(s);
            }
            if (ds < 0.0) != (de < 0.0) {
                let t = ds / (ds - de);
                p.push([s[0] + t * (e[0] - s[0]), s[1] + t * (e[1] - s[1])]);
            }
        }
    }
    p
}

#[derive(Debug)]
pub struct SectionMesh {
    // Vertices preprojected once, all section stations batched over these arrays.
    triangles: Vec<[[f64; 3]; 3]>,
    min: f64,
    max: f64,
}
impl SectionMesh {
    /// Axis-normal sections. origin and station are model-local, in mm.
    #[must_use]
    pub fn new(mesh: &[Triangle], origin: V, axis: V) -> Self {
        let n = unit(axis);
        let helper = if n[2].abs() < 0.9 {
            [0.0, 0.0, 1.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let u = unit(cross(n, helper));
        let v = cross(n, u);
        let triangles: Vec<_> = mesh
            .iter()
            .map(|t| {
                t.map(|p| {
                    let d = sub(p.map(f64::from), origin);
                    [dot(d, u), dot(d, v), dot(d, n)]
                })
            })
            .collect();
        let min = triangles
            .iter()
            .flatten()
            .map(|p| p[2])
            .fold(f64::INFINITY, f64::min);
        let max = triangles
            .iter()
            .flatten()
            .map(|p| p[2])
            .fold(f64::NEG_INFINITY, f64::max);
        Self {
            triangles,
            min,
            max,
        }
    }
    fn polygon(&self, t: f64) -> Vec<P> {
        if t < self.min || t > self.max {
            return vec![];
        }
        let mut p = vec![];
        for tri in &self.triangles {
            for i in 0..3 {
                let (a, b) = (tri[i], tri[(i + 1) % 3]);
                if (a[2] - t).abs() < 1e-12 {
                    p.push([a[0], a[1]]);
                }
                if (a[2] < t && b[2] > t) || (a[2] > t && b[2] < t) {
                    let f = (t - a[2]) / (b[2] - a[2]);
                    p.push([a[0] + f * (b[0] - a[0]), a[1] + f * (b[1] - a[1])]);
                }
            }
        }
        hull(p)
    }
}

/// Exact polygon union area for this station (apart from f64 arithmetic). Never
/// takes the convex hull of the union: that would invent material between parts.
#[must_use]
pub fn union_area(meshes: &[SectionMesh], t: f64) -> f64 {
    let p: Vec<_> = meshes
        .iter()
        .map(|m| m.polygon(t))
        .filter(|p| p.len() > 2)
        .collect();
    assert!(
        p.len() <= 8,
        "too many overlapping parts for the bounded CAL-1 measurement"
    );
    let mut sum = 0.0;
    for mask in 1usize..(1 << p.len()) {
        let mut selected = p
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, p)| p);
        let mut poly = selected.next().unwrap().clone();
        for q in selected {
            poly = intersection(poly, q);
        }
        sum += if mask.count_ones() % 2 == 1 {
            area(&poly)
        } else {
            -area(&poly)
        };
    }
    sum.max(0.0)
}

#[derive(Debug, Clone, Copy)]
pub struct Minimum {
    pub area: f64,
    pub at: f64,
    pub step: f64,
}
/// A measured sampled minimum, not a certified continuous lower bound. Reports
/// include station resolution. Native piece joins are also checked by plinth-check.
#[must_use]
pub fn minimum(meshes: &[SectionMesh], lo: f64, hi: f64, steps: usize) -> Minimum {
    let step = (hi - lo) / steps as f64;
    let mut result = Minimum {
        area: f64::INFINITY,
        at: lo,
        step,
    };
    for i in 0..=steps {
        let t = lo + i as f64 * step;
        let a = union_area(meshes, t);
        if a < result.area {
            result.area = a;
            result.at = t;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build, spec::PlateSpec};
    fn spec() -> PlateSpec {
        serde_json::from_str(include_str!("../../../fixtures/calibration/cal1.json")).unwrap()
    }
    #[test]
    fn both_written_waists_are_minimum_on_each_entire_head_path() {
        let spec = spec();
        for id in ["B-100", "S-324"] {
            let s = spec.specimens.iter().find(|s| s.id == id).unwrap();
            let bytes = build::specimen(s, &spec.head).to_bytes().unwrap();
            let read = plinth_chitubox::parse(&bytes).unwrap();
            let records = &read.models[0].supports.as_ref().unwrap().records;
            let expected = 8.0
                * (std::f64::consts::TAU / 16.0).sin()
                * (f64::from(s.waist_diameters[0]) / 2.0).powi(2);
            for (i, head) in records[..4 * build::PARTS_PER_HEAD]
                .chunks_exact(build::PARTS_PER_HEAD)
                .enumerate()
            {
                let pad = &records[records.len() - 4 + i];
                let meshes: Vec<_> = head
                    .iter()
                    .chain(std::iter::once(pad))
                    .map(|r| SectionMesh::new(&r.mesh, [0.0, 0.0, -7.5], [0.0, 0.0, 1.0]))
                    .collect();
                let m = minimum(&meshes, 0.001, 5.0, 1000);
                assert!(
                    (m.area / expected - 1.0).abs() < 0.001,
                    "{m:?} expected {expected}"
                );
                assert!(
                    m.at > 4.17 && m.at < 4.88,
                    "minimum must be on the waist: {m:?}"
                );
                assert!((union_area(&meshes, 4.5) / expected - 1.0).abs() < 0.001);
            }
        }
    }
    #[test]
    fn sabotage_written_mesh_not_radius_field_decides_section() {
        let spec = spec();
        let s = &spec.specimens[0];
        let mut p = build::specimen(s, &spec.head);
        let r = &mut p.models[0].supports.as_mut().unwrap().records[1];
        // Leave r1/r2 correct. Halve the actual mesh's radius in X and Y only.
        for point in r.mesh.iter_mut().flatten() {
            for (v, c) in point[..2].iter_mut().zip(s.contact_sites[0]) {
                *v = c + (*v - c) * 0.5;
            }
        }
        let read = plinth_chitubox::parse(&p.to_bytes().unwrap()).unwrap();
        let r = &read.models[0].supports.as_ref().unwrap().records[1];
        let m = SectionMesh::new(&r.mesh, [0.0; 3], [0.0, 0.0, 1.0]);
        let actual = union_area(&[m], -3.0);
        let design = 8.0 * (std::f64::consts::TAU / 16.0).sin() * f64::from(r.r1).powi(2);
        assert!((actual / design - 0.25).abs() < 0.001);
        assert!(
            (actual / design - 1.0).abs() > 0.10,
            "sabotage must fail the ±10% criterion"
        );
    }
    #[test]
    fn controls_keep_free_arm_cap_area_and_reconstructed_heights() {
        let spec = spec();
        for (id, radius, cap_start, top) in [
            ("K-15", 0.798f64, 9.1f64, 9.6f64),
            ("K-60", 0.399, 9.2, 9.7),
        ] {
            let s = spec.specimens.iter().find(|s| s.id == id).unwrap();
            let written = build::specimen(s, &spec.head).to_bytes().unwrap();
            let p = plinth_chitubox::parse(&written).unwrap();
            let m = &p.models[0].mesh;
            let bottom = m
                .iter()
                .flatten()
                .map(|p| f64::from(p[2]))
                .fold(f64::INFINITY, f64::min);
            let meshes = [SectionMesh::new(m, [0.0, 0.0, bottom], [0.0, 0.0, 1.0])];
            let design = 24.0 * (std::f64::consts::TAU / 48.0).sin() * radius * radius;
            for z in [4.5001, 6.5, 8.4999] {
                assert!((union_area(&meshes, z) / design - 1.0).abs() < 1e-5);
            }
            assert!((union_area(&meshes, cap_start + 0.25) - 29.9978).abs() < 1e-4);
            assert!(
                (m.iter()
                    .flatten()
                    .map(|p| f64::from(p[2]))
                    .fold(f64::NEG_INFINITY, f64::max)
                    - bottom
                    - top)
                    .abs()
                    < 1e-5
            );
            let min = minimum(&meshes, 0.001, top - 0.001, 2000);
            assert!((min.area / design - 1.0).abs() < 1e-5);
        }
    }
    #[test]
    fn union_does_not_fill_the_gap_between_disconnected_parts() {
        let mesh = plinth_geom::shapes::pad([0.0, 0.0], 1.0, 0.0, 1.0);
        let other = plinth_geom::shapes::pad([3.0, 0.0], 1.0, 0.0, 1.0);
        let a = SectionMesh::new(&mesh, [0.0; 3], [0.0, 0.0, 1.0]);
        let b = SectionMesh::new(&other, [0.0; 3], [0.0, 0.0, 1.0]);
        assert!(
            (union_area(&[a, b], 0.5) - 20.0 * (std::f64::consts::TAU / 20.0).sin()).abs() < 1e-5
        );
    }
}
