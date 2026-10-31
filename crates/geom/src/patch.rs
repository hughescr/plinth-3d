//! Enclosed sphere/surface patch areas, including arithmetic error.
//!
//! The construction certificate uses adaptive triangle subdivision, not a point grid.
//! A triangle wholly in the sphere contributes its entire area to both bounds; a
//! disjoint bounding box contributes nothing; unresolved triangles contribute only
//! to the upper bound. The exact area of each child is one quarter of its parent.
//! This avoids treating agreement between two floating circle integrals as a proof.

use crate::{Bvh, Mesh, measure::Measure};

/// Written sphere converted to f64.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ball {
    /// Centre, mm.
    pub centre: [f64; 3],
    /// Radius, mm.
    pub radius: f64,
}
/// Certified area interval, mm².
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AreaBounds {
    /// Lower bound.
    pub lo: f64,
    /// Upper bound.
    pub hi: f64,
}

pub(crate) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
pub(crate) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
pub(crate) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub(crate) fn triangle(m: &Mesh, t: usize) -> [[f64; 3]; 3] {
    std::array::from_fn(|k| {
        let i = m.indices()[3 * t + k] as usize;
        [m.xs()[i], m.ys()[i], m.zs()[i]]
    })
}

#[derive(Clone, Copy)]
struct Interval(f64, f64);
impl Interval {
    fn exact(v: f64) -> Self {
        Self(v, v)
    }
    fn add(self, b: Self) -> Self {
        Self((self.0 + b.0).next_down(), (self.1 + b.1).next_up())
    }
    fn sub(self, b: Self) -> Self {
        Self((self.0 - b.1).next_down(), (self.1 - b.0).next_up())
    }
    fn mul(self, b: Self) -> Self {
        let products = [self.0 * b.0, self.0 * b.1, self.1 * b.0, self.1 * b.1];
        Self(
            products
                .into_iter()
                .fold(f64::INFINITY, f64::min)
                .next_down(),
            products
                .into_iter()
                .fold(f64::NEG_INFINITY, f64::max)
                .next_up(),
        )
    }
    fn div_positive(self, b: Self) -> Self {
        self.mul(Self((1.0 / b.1).next_down(), (1.0 / b.0).next_up()))
    }
    fn square(self) -> Self {
        let hi = (self.0 * self.0).max(self.1 * self.1).next_up();
        let lo = if self.0 <= 0.0 && self.1 >= 0.0 {
            0.0
        } else {
            (self.0 * self.0).min(self.1 * self.1).next_down().max(0.0)
        };
        Self(lo, hi)
    }
    fn sqrt(self) -> Self {
        Self(
            self.0.max(0.0).sqrt().next_down().max(0.0),
            self.1.sqrt().next_up(),
        )
    }
}
fn differences(a: [f64; 3], b: [f64; 3]) -> [Interval; 3] {
    std::array::from_fn(|i| Interval::exact(a[i]).sub(Interval::exact(b[i])))
}
fn cross_interval(a: [Interval; 3], b: [Interval; 3]) -> [Interval; 3] {
    [
        a[1].mul(b[2]).sub(a[2].mul(b[1])),
        a[2].mul(b[0]).sub(a[0].mul(b[2])),
        a[0].mul(b[1]).sub(a[1].mul(b[0])),
    ]
}
fn norm_interval(a: [Interval; 3]) -> Interval {
    a.into_iter()
        .fold(Interval::exact(0.0), |sum, v| sum.add(v.square()))
        .sqrt()
}
fn triangle_area(t: [[f64; 3]; 3]) -> Interval {
    norm_interval(cross_interval(
        differences(t[1], t[0]),
        differences(t[2], t[0]),
    ))
    .mul(Interval::exact(0.5))
}

/// Distance from a point to a triangle's infinite plane, with outward arithmetic.
/// Degenerate or numerically unresolved planes return None. Used for the native
/// ball's face in-sphere certificate, never inferred from vertex inscription.
#[must_use]
pub fn plane_distance_bounds(
    point: [f64; 3],
    t: [[f64; 3]; 3],
) -> Option<crate::clearance::GapBounds> {
    let n = cross_interval(differences(t[1], t[0]), differences(t[2], t[0]));
    let denominator = norm_interval(n);
    let d = differences(t[0], point);
    let numerator = (0..3).fold(Interval::exact(0.0), |sum, i| sum.add(d[i].mul(n[i])));
    if denominator.0 <= 0.0 || (numerator.0 <= 0.0 && numerator.1 >= 0.0) {
        return None;
    }
    let lo = numerator.0.abs().min(numerator.1.abs());
    let hi = numerator.0.abs().max(numerator.1.abs());
    Some(crate::clearance::GapBounds {
        lo: (lo / denominator.1).next_down().max(0.0),
        hi: (hi / denominator.0).next_up(),
    })
}

pub(crate) fn frustum_plane_gap(
    t: [[f64; 3]; 3],
    start: [f64; 3],
    end: [f64; 3],
    ra: f64,
    rb: f64,
) -> f64 {
    let n = cross_interval(differences(t[1], t[0]), differences(t[2], t[0]));
    let normal_length = norm_interval(n);
    let d = differences(end, start);
    let length = norm_interval(d);
    if length.0 <= 0.0 || normal_length.0 <= 0.0 {
        return 0.0;
    }
    let axis = d.map(|x| x.div_positive(length));
    let axial = (0..3).fold(Interval::exact(0.0), |sum, i| sum.add(n[i].mul(axis[i])));
    let radial = normal_length.square().sub(axial.square()).sqrt();
    let project = |p| {
        let delta = differences(p, t[0]);
        (0..3).fold(Interval::exact(0.0), |sum, i| sum.add(delta[i].mul(n[i])))
    };
    let a = project(start);
    let b = project(end);
    let ar = radial.mul(Interval::exact(ra));
    let br = radial.mul(Interval::exact(rb));
    let lo = a.sub(ar).0.min(b.sub(br).0);
    let hi = a.add(ar).1.max(b.add(br).1);
    (lo.max(-hi).max(0.0) / normal_length.1)
        .next_down()
        .max(0.0)
}

// At most 20 successive midpoint roundings, coordinates bounded by 1000 mm.
// Division by 2 is exact except subnormals. Even 64 ulps at 1000 encloses the
// accumulated coordinate error (including subnormal underflow) on each axis.
const POSITION_ERROR: f64 = 64.0 * f64::EPSILON * 1000.0;
// Depth limits precision, never enclosure soundness: all unresolved area remains
// in the upper bound. Sixteen levels meet the fast-suite width requirement without
// spending four times the boundary work for precision admission does not require.
const MAX_DEPTH: u32 = 16;
const MAX_NODES: usize = 2_000_000;

fn classify(t: [[f64; 3]; 3], ball: Ball) -> i8 {
    let r2 = Interval::exact(ball.radius).square();
    let mut box_distance = Interval::exact(0.0);
    for k in 0..3 {
        let lo = t.iter().map(|p| p[k]).fold(f64::INFINITY, f64::min) - POSITION_ERROR;
        let hi = t.iter().map(|p| p[k]).fold(f64::NEG_INFINITY, f64::max) + POSITION_ERROR;
        let distance = (lo - ball.centre[k]).max(ball.centre[k] - hi).max(0.0);
        // Both the box subtraction and distance subtraction get outward slack.
        let d = Interval(
            (distance - POSITION_ERROR).max(0.0),
            distance + POSITION_ERROR,
        );
        box_distance = box_distance.add(d.square());
    }
    if box_distance.0 > r2.1 {
        return -1;
    }
    if t.iter().all(|p| {
        let distance = (0..3).fold(Interval::exact(0.0), |sum, k| {
            let d = Interval(p[k] - POSITION_ERROR, p[k] + POSITION_ERROR)
                .sub(Interval::exact(ball.centre[k]));
            sum.add(d.square())
        });
        distance.1 < r2.0
    }) {
        return 1;
    }
    0
}

/// Scalar enclosure reference. Unresolved boundary triangles are INCLUDED in the
/// upper bound, never dropped. A work-budget failure returns Unverified.
#[must_use]
pub fn ball_patch_reference(mesh: &Mesh, ball: Ball) -> Measure<AreaBounds> {
    if ball
        .centre
        .iter()
        .any(|x| !x.is_finite() || x.abs() > 1000.0)
        || !ball.radius.is_finite()
        || ball.radius <= 0.0
        || ball.radius > 10.0
        || mesh
            .bounds()
            .min
            .iter()
            .chain(&mesh.bounds().max)
            .any(|x| x.abs() > 1000.0)
    {
        return Measure::Unverified("patch outside numerical domain");
    }
    let mut lo = 0.0_f64;
    let mut hi = 0.0_f64;
    let mut nodes = 0;
    let mut stack = Vec::with_capacity(64);
    for i in 0..mesh.triangle_count() {
        let t = triangle(mesh, i);
        let area = triangle_area(t);
        if area.0 <= 0.0 {
            return Measure::Unverified("degenerate patch triangle");
        }
        stack.push((t, area, 0));
        while let Some((t, area, depth)) = stack.pop() {
            nodes += 1;
            if nodes > MAX_NODES {
                return Measure::Unverified("patch subdivision budget exhausted");
            }
            match classify(t, ball) {
                -1 => continue,
                1 => {
                    lo = (lo + area.0).next_down().max(0.0);
                    hi = (hi + area.1).next_up();
                }
                _ if depth == MAX_DEPTH => hi = (hi + area.1).next_up(),
                _ => {
                    let midpoint =
                        |a: [f64; 3], b: [f64; 3]| std::array::from_fn(|k| (a[k] + b[k]) * 0.5);
                    let [a, b, c] = t;
                    let ab = midpoint(a, b);
                    let bc = midpoint(b, c);
                    let ca = midpoint(c, a);
                    let child = area.mul(Interval::exact(0.25));
                    for tri in [[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]] {
                        stack.push((tri, child, depth + 1));
                    }
                }
            }
        }
    }
    Measure::Known(AreaBounds { lo, hi })
}

/// One stage-level call, one enclosure per sphere, stable input order.
pub fn ball_patches(bvh: &Bvh<'_>, balls: &[Ball], out: &mut Vec<Measure<AreaBounds>>) {
    ball_patches_with_workers(bvh, balls, out, 1);
}

/// Explicit bounded-worker version. Every task holds at least 64 balls except
/// the final remainder. The caller supplies its allocation; no environment lookup.
pub fn ball_patches_with_workers(
    bvh: &Bvh<'_>,
    balls: &[Ball],
    out: &mut Vec<Measure<AreaBounds>>,
    workers: usize,
) {
    crate::counters::record(crate::counters::Op::BallPatches, balls.len());
    map_batch(balls, out, workers, |b| {
        ball_patch_reference(bvh.mesh(), *b)
    });
}

pub(crate) fn map_batch<T: Sync, U: Send>(
    items: &[T],
    out: &mut Vec<U>,
    workers: usize,
    f: impl Fn(&T) -> U + Sync,
) {
    let tasks = (items.len() / 64).clamp(1, workers.clamp(1, 8));
    let per = items.len().div_ceil(tasks).max(1);
    out.clear();
    let mut threads = usize::from(!items.is_empty());
    std::thread::scope(|scope| {
        let mut chunks = items.chunks(per).collect::<Vec<_>>();
        let last = chunks.pop();
        let f = &f;
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| scope.spawn(move || chunk.iter().map(f).collect::<Vec<_>>()))
            .collect();
        let final_chunk = last.map(|chunk| chunk.iter().map(f).collect::<Vec<_>>());
        for handle in handles {
            match handle.join() {
                Ok(values) => {
                    threads += 1;
                    out.extend(values);
                }
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
        if let Some(values) = final_chunk {
            out.extend(values);
        }
    });
    crate::counters::record_par(crate::counters::ParReport {
        tasks: if items.is_empty() { 0 } else { tasks },
        threads,
    });
}

/// Connected touched-triangle sets per ball. Non-welded seams can cause a
/// conservative rejection, never a false single-sheet certificate.
pub fn ball_patch_components(bvh: &Bvh<'_>, balls: &[Ball], out: &mut Vec<usize>) {
    out.clear();
    let mesh = bvh.mesh();
    for ball in balls {
        let touched: Vec<_> = (0..mesh.triangle_count())
            .filter(|&i| {
                let [a, b, c] = triangle(mesh, i);
                crate::tri::dist2(
                    ball.centre,
                    crate::tri::closest_on_triangle(ball.centre, a, b, c),
                ) < ball.radius.powi(2)
            })
            .collect();
        let mut parent: Vec<_> = (0..touched.len()).collect();
        let mut edges = std::collections::BTreeMap::<(u32, u32), usize>::new();
        let root = |parent: &[usize], mut i: usize| {
            while parent[i] != i {
                i = parent[i];
            }
            i
        };
        let vertex = |ix: u32| {
            let ix = ix as usize;
            [mesh.xs()[ix], mesh.ys()[ix], mesh.zs()[ix]]
        };
        for (at, &i) in touched.iter().enumerate() {
            let indices = &mesh.indices()[3 * i..3 * i + 3];
            for k in 0..3 {
                let a = indices[k];
                let b = indices[(k + 1) % 3];
                let p = vertex(a);
                let q = vertex(b);
                let d = sub(q, p);
                let len2 = dot(d, d);
                if len2 < 1e-20 {
                    continue;
                }
                let t = (dot(sub(ball.centre, p), d) / len2).clamp(0.0, 1.0);
                let nearest = std::array::from_fn(|k| p[k] + t * d[k]);
                if crate::tri::dist2(nearest, ball.centre) >= ball.radius.powi(2) - 1e-12 {
                    continue;
                }
                if let Some(other) = edges.insert((a.min(b), a.max(b)), at) {
                    let x = root(&parent, at);
                    let y = root(&parent, other);
                    parent[x] = y;
                }
            }
        }
        out.push((0..parent.len()).filter(|&i| root(&parent, i) == i).count());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disc_truth_and_batch_reference() {
        let m = Mesh::new(
            vec![-5., 5., 5., -5.],
            vec![-5., -5., 5., 5.],
            vec![0.; 4],
            vec![0, 1, 2, 0, 2, 3],
        )
        .unwrap();
        let balls: Vec<_> = (0..64)
            .map(|i| Ball {
                centre: [0., 0., i as f64 / 100.],
                radius: 0.7,
            })
            .collect();
        let mut out = Vec::new();
        ball_patches(&Bvh::build(&m).unwrap(), &balls, &mut out);
        for (b, r) in balls.iter().zip(out) {
            assert_eq!(r, ball_patch_reference(&m, *b));
            let truth = std::f64::consts::PI * (b.radius * b.radius - b.centre[2] * b.centre[2]);
            let bounds = r.known().unwrap();
            assert!(
                bounds.lo <= truth && bounds.hi >= truth,
                "{bounds:?} truth={truth}"
            );
            assert!(bounds.hi - bounds.lo < 0.001);
        }
    }
}
