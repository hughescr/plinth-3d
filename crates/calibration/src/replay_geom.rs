//! Calibration-only written convex head clipping. Not a production checker.
use crate::section::{dot, sub};
use plinth_chitubox::Triangle;
pub type V = [f64; 3];
pub fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn length(a: V) -> f64 {
    dot(a, a).sqrt()
}
pub fn centre(a: [f32; 3], b: [f32; 3]) -> V {
    std::array::from_fn(|k| (f64::from(a[k]) + f64::from(b[k])) / 2.0)
}
pub fn near(a: V, b: V) -> bool {
    length(sub(a, b)) < 0.001
}
pub fn clip(p: &[V], n: V, b: f64) -> Vec<V> {
    let mut out = Vec::new();
    for i in 0..p.len() {
        let (s, e) = (p[i], p[(i + 1) % p.len()]);
        let (ds, de) = (dot(n, s) - b, dot(n, e) - b);
        if ds <= 0.0 {
            out.push(s);
        }
        if (ds < 0.0 && de > 0.0) || (ds > 0.0 && de < 0.0) {
            let t = ds / (ds - de);
            out.push(std::array::from_fn(|k| s[k] + t * (e[k] - s[k])));
        }
    }
    out
}
pub fn area(p: &[V]) -> f64 {
    if p.len() < 3 {
        return 0.0;
    }
    (1..p.len() - 1)
        .map(|i| length(cross(sub(p[i], p[0]), sub(p[i + 1], p[0]))) / 2.0)
        .sum()
}
#[derive(Debug)]
pub struct Convex {
    planes: Vec<(V, f64)>,
    lo: V,
    hi: V,
}
impl Convex {
    pub fn new(mesh: &[Triangle]) -> Self {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in mesh.iter().flatten() {
            for k in 0..3 {
                lo[k] = lo[k].min(f64::from(p[k]));
                hi[k] = hi[k].max(f64::from(p[k]));
            }
        }
        let mid = std::array::from_fn(|k| (lo[k] + hi[k]) / 2.0);
        let mut planes: Vec<(V, f64)> = Vec::new();
        for t in mesh {
            let t = t.map(|p| p.map(f64::from));
            let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
            let len = length(n);
            if len < 1e-12 {
                continue;
            }
            let mut n = n.map(|v| v / len);
            let mut b = dot(n, t[0]);
            if dot(n, mid) > b {
                n = n.map(|v| -v);
                b = -b;
            }
            if !planes
                .iter()
                .any(|(q, c)| near_plane(*q, n) && (*c - b).abs() < 1e-7)
            {
                planes.push((n, b));
            }
        }
        Self { planes, lo, hi }
    }
    fn intersect(&self, p: &[V]) -> Vec<V> {
        if (0..3).any(|k| {
            p.iter().all(|v| v[k] < self.lo[k] - 1e-7) || p.iter().all(|v| v[k] > self.hi[k] + 1e-7)
        }) {
            return vec![];
        }
        let mut p = p.to_vec();
        for &(n, b) in &self.planes {
            p = clip(&p, n, b);
            if p.len() < 3 {
                return vec![];
            }
        }
        p
    }
}
fn near_plane(a: V, b: V) -> bool {
    length(sub(a, b)) < 1e-7
}
/// Surface patch of the union of the written ball and tip. Inclusion/exclusion
/// prevents the overlap of the two solids being counted twice.
pub fn patch(body: &[Triangle], ball: &[Triangle], tip: &[Triangle]) -> (f64, [f64; 2]) {
    let (a, b) = (Convex::new(ball), Convex::new(tip));
    let mut total = 0.0;
    let mut z = [f64::INFINITY, f64::NEG_INFINITY];
    for t in body {
        let p = t.map(|v| v.map(f64::from));
        let pa = a.intersect(&p);
        let pb = b.intersect(&p);
        let both = b.intersect(&pa);
        total += area(&pa) + area(&pb) - area(&both);
        for p in [&pa, &pb] {
            if area(p) > 1e-14 {
                for v in p {
                    z[0] = z[0].min(v[2]);
                    z[1] = z[1].max(v[2]);
                }
            }
        }
    }
    (total, z)
}
/// Clip the upper loaded part of a solid, closing its horizontal cut with a fan.
/// Used only on one convex cross-section (one W column, or one K arm).
pub fn above(mesh: &[Triangle], z: f64) -> Vec<Triangle> {
    let mut out = Vec::new();
    let mut cut = Vec::new();
    for t in mesh {
        let p = clip(&t.map(|p| p.map(f64::from)), [0.0, 0.0, -1.0], -z);
        for i in 1..p.len().saturating_sub(1) {
            out.push([p[0], p[i], p[i + 1]].map(|v| v.map(|x| x as f32)));
        }
        for p in p {
            if (p[2] - z).abs() < 1e-7 {
                cut.push(p);
            }
        }
    }
    cut.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    cut.dedup_by(|a, b| length(sub(*a, *b)) < 1e-7);
    assert!(cut.len() >= 3, "solid cut has no section");
    let c: V = std::array::from_fn(|k| cut.iter().map(|p| p[k]).sum::<f64>() / cut.len() as f64);
    cut.sort_by(|a, b| {
        (a[1] - c[1])
            .atan2(a[0] - c[0])
            .total_cmp(&(b[1] - c[1]).atan2(b[0] - c[0]))
    });
    for i in 0..cut.len() {
        out.push([c, cut[(i + 1) % cut.len()], cut[i]].map(|p| p.map(|v| v as f32)));
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn union_patch_does_not_double_count() {
        let ball = plinth_geom::shapes::frustum([0., 0., -1.], [0., 0., 1.], 1., 1.);
        let body = vec![[[-2., -2., 0.], [2., -2., 0.], [0., 2., 0.]]];
        let (one, _) = patch(&body, &ball, &[]);
        let (two, _) = patch(&body, &ball, &ball);
        assert!(one > 1.0 && (one - two).abs() < 1e-9);
    }
}
