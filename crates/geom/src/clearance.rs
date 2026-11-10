//! Conservative finite-solid/surface distance enclosures, including caps.
//! Outward-rounded plane projections provide certified lower bounds. A containing
//! capsule and triangle box give a second lower bound; actual vertex witnesses give
//! upper bounds. These admission intervals need not have the checker's 0.005 mm width.

mod pairs;
pub use pairs::{solid_pair_lower, solid_pair_lowers};

use crate::patch::{cross, dot, frustum_plane_gap, plane_distance_bounds, sub, triangle};
use crate::{Bvh, Mesh, measure::Measure, patch::Ball};

/// Native analytic enclosing solids.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Solid {
    /// Whole sphere.
    Ball(Ball),
    /// Closed finite frustum (caps included).
    Frustum {
        /// Start centre.
        start: [f64; 3],
        /// End centre.
        end: [f64; 3],
        /// Start radius.
        r_start: f64,
        /// End radius.
        r_end: f64,
    },
}
/// Enclosure of the surface distance, mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GapBounds {
    /// Lower bound (may be conservative).
    pub lo: f64,
    /// Upper bound (a geometric witness).
    pub hi: f64,
}

// At coordinates <= 1000 mm the following box and witness expressions use at most
// 32 scalar operations at magnitude <= 1.2e7. Their absolute distance rounding error
// is enclosed by 1e-8 mm. No ill-conditioned point/triangle solve enters a lower bound.
const ARITHMETIC_MM: f64 = 1e-8;
fn box_gap(p: [f64; 3], t: [[f64; 3]; 3]) -> f64 {
    let mut squared = 0.0;
    for k in 0..3 {
        let lo = t.iter().map(|v| v[k]).fold(f64::INFINITY, f64::min);
        let hi = t.iter().map(|v| v[k]).fold(f64::NEG_INFINITY, f64::max);
        let d = (lo - p[k]).max(p[k] - hi).max(0.0);
        squared += d * d;
    }
    (squared.sqrt() - ARITHMETIC_MM).max(0.0)
}
fn vertex_witness(p: [f64; 3], t: [[f64; 3]; 3]) -> f64 {
    t.into_iter()
        .map(|v| {
            let d = sub(v, p);
            dot(d, d).sqrt() + ARITHMETIC_MM
        })
        .fold(f64::INFINITY, f64::min)
}

fn frustum_triangle(t: [[f64; 3]; 3], a: [f64; 3], b: [f64; 3], ra: f64, rb: f64) -> GapBounds {
    let d = sub(b, a);
    let length = dot(d, d).sqrt();
    let plane = frustum_plane_gap(t, a, b, ra, rb);
    // A sub-frustum is inside the radius balls swept along its finite axis.
    // Distance is 1-Lipschitz: box-distance(mid) - half-axis-length - max-radius
    // bounds the entire sub-frustum. This is NOT an exact finite-cap distance.
    let mut lower = f64::INFINITY;
    let mut upper = f64::INFINITY;
    const PIECES: usize = 16;
    for i in 0..PIECES {
        let u = i as f64 / PIECES as f64;
        let v = (i + 1) as f64 / PIECES as f64;
        let mid = (u + v) * 0.5;
        let p = std::array::from_fn(|k| a[k] + mid * d[k]);
        let radius = (ra + (rb - ra) * u).max(ra + (rb - ra) * v);
        lower = lower.min(box_gap(p, t) - length / (2.0 * PIECES as f64) - radius - ARITHMETIC_MM);
        upper = upper.min(vertex_witness(p, t) + ARITHMETIC_MM);
    }
    let axis = d.map(|v| v / length);
    let mut support = 0.0_f64;
    for u in [0.0, 0.5, 1.0] {
        let p = std::array::from_fn(|k| a[k] + u * d[k]);
        let q = crate::tri::closest_on_triangle(p, t[0], t[1], t[2]);
        let w = sub(p, q);
        let length = dot(w, w).sqrt();
        if length <= ARITHMETIC_MM {
            continue;
        }
        let n = w.map(|v| v / length);
        let radial_vector = cross(n, axis);
        let radial = dot(radial_vector, radial_vector).sqrt();
        let endpoint = |p, radius| {
            t.iter()
                .map(|&y| dot(sub(p, y), n))
                .fold(f64::INFINITY, f64::min)
                - radius * radial
        };
        support = support.max(endpoint(a, ra).min(endpoint(b, rb)) - ARITHMETIC_MM);
    }
    GapBounds {
        lo: plane.max(lower).max(support).max(0.0),
        hi: upper,
    }
}

/// Scalar reference. Out-of-domain and degenerate solids fail closed. An interval
/// can be wide; a caller may use its lower bound for admission but not claim precision.
#[must_use]
pub fn solid_gap_reference(mesh: &Mesh, solid: Solid) -> Measure<GapBounds> {
    if mesh
        .bounds()
        .min
        .iter()
        .chain(&mesh.bounds().max)
        .any(|v| !v.is_finite() || v.abs() > 1000.0)
    {
        return Measure::Unverified("model outside clearance domain");
    }
    let mut bound = GapBounds {
        lo: f64::INFINITY,
        hi: f64::INFINITY,
    };
    match solid {
        Solid::Ball(b) => {
            if !valid_point(b.centre) || !b.radius.is_finite() || b.radius <= 0.0 || b.radius > 10.0
            {
                return Measure::Unverified("invalid ball");
            }
            for i in 0..mesh.triangle_count() {
                let t = triangle(mesh, i);
                let plane = plane_distance_bounds(b.centre, t).map_or(0.0, |d| d.lo);
                let lo = (plane.max(box_gap(b.centre, t)) - b.radius).max(0.0);
                let hi = (vertex_witness(b.centre, t) - b.radius).max(0.0);
                bound.lo = bound.lo.min(lo);
                bound.hi = bound.hi.min(hi);
            }
        }
        Solid::Frustum {
            start,
            end,
            r_start,
            r_end,
        } => {
            if !valid_point(start)
                || !valid_point(end)
                || !r_start.is_finite()
                || !r_end.is_finite()
                || r_start <= 0.0
                || r_end <= 0.0
                || r_start.max(r_end) > 10.0
                || dot(sub(start, end), sub(start, end)) < 1e-20
            {
                return Measure::Unverified("invalid frustum");
            }
            for i in 0..mesh.triangle_count() {
                let d = frustum_triangle(triangle(mesh, i), start, end, r_start, r_end);
                bound.lo = bound.lo.min(d.lo);
                bound.hi = bound.hi.min(d.hi);
            }
        }
    }
    if !bound.hi.is_finite() {
        return Measure::Unverified("empty mesh");
    }
    Measure::Known(bound)
}
fn valid_point(p: [f64; 3]) -> bool {
    p.iter().all(|x| x.is_finite() && x.abs() <= 1000.0)
}
/// One ordered answer per input solid; one worker by default.
pub fn solid_gaps(bvh: &Bvh<'_>, solids: &[Solid], out: &mut Vec<Measure<GapBounds>>) {
    solid_gaps_with_workers(bvh, solids, out, 1);
}
/// Bounded-worker stage-level probe; allocation comes from the caller.
pub fn solid_gaps_with_workers(
    bvh: &Bvh<'_>,
    solids: &[Solid],
    out: &mut Vec<Measure<GapBounds>>,
    workers: usize,
) {
    crate::counters::record(crate::counters::Op::SolidGaps, solids.len());
    crate::patch::map_batch(solids, out, workers, |&s| solid_gap_bvh(bvh, s));
}

// Reconstruct a convex-combination witness rather than trusting a numerically
// ill-conditioned closest-point solve to remain inside a very thin triangle.
fn triangle_witness(p: [f64; 3], t: [[f64; 3]; 3]) -> [f64; 3] {
    let q = crate::tri::closest_on_triangle(p, t[0], t[1], t[2]);
    let a = sub(t[1], t[0]);
    let b = sub(t[2], t[0]);
    let n = cross(a, b);
    let denominator = dot(n, n);
    if denominator <= 0.0 {
        return t[0];
    }
    let d = sub(q, t[0]);
    let v = (dot(cross(d, b), n) / denominator).clamp(0.0, 1.0);
    let w = (dot(cross(a, d), n) / denominator).clamp(0.0, 1.0);
    if !v.is_finite() || !w.is_finite() {
        return t[0];
    }
    let weights = [(1.0 - v - w).max(0.0), v, w];
    let sum: f64 = weights.iter().sum();
    std::array::from_fn(|k| (0..3).map(|i| t[i][k] * (weights[i] / sum)).sum())
}

/// Distance from a finite disc to a triangle. Alternating projections supply only
/// witnesses; the lower bound comes from a separating support plane, not convergence.
fn disc_triangle(t: [[f64; 3]; 3], centre: [f64; 3], axis: [f64; 3], radius: f64) -> GapBounds {
    let mut x = centre;
    let mut y = t[0];
    for _ in 0..64 {
        y = triangle_witness(x, t);
        let v = sub(y, centre);
        let axial = dot(v, axis);
        let radial = std::array::from_fn(|k| v[k] - axial * axis[k]);
        let length = dot(radial, radial).sqrt();
        let scale = if length > radius {
            radius / length
        } else {
            1.0
        };
        x = std::array::from_fn(|k| centre[k] + scale * radial[k]);
    }
    let w = sub(x, y);
    let distance = dot(w, w).sqrt();
    if distance <= ARITHMETIC_MM {
        return GapBounds {
            lo: 0.0,
            hi: distance + ARITHMETIC_MM,
        };
    }
    let n = w.map(|v| v / distance);
    let radial_vector = cross(n, axis);
    let radial = dot(radial_vector, radial_vector).sqrt();
    let support = t
        .into_iter()
        .map(|p| dot(sub(centre, p), n))
        .fold(f64::INFINITY, f64::min);
    GapBounds {
        lo: (support - radius * radial - ARITHMETIC_MM).max(0.0),
        hi: distance + ARITHMETIC_MM,
    }
}

/// Certified capped departure (cap 0.35 mm) for complete finite cones.
/// For a triangle, distance to the linearly moving/growing disc is convex in t.
/// A nonnegative backward secant, enclosed by support-plane/witness bounds, proves
/// nondecreasing distance for ALL subsequent t, not just sampled stations. Triangles
/// proved at least 0.35 mm from the entire cone need no monotonicity certificate.
/// The minimum over these functions cannot decrease below the cap. Every triangle
/// within reach is included by the BVH enclosing-ball query; omitted triangles are
/// farther than 0.35 mm. The final disc must also certify the 0.32 mm end gap.
/// This is sufficient, not necessary: unresolved secants return false, never pass.
pub fn capped_departures(bvh: &Bvh<'_>, cones: &[Solid], out: &mut Vec<Measure<bool>>) {
    let balls: Vec<_> = cones
        .iter()
        .map(|s| match *s {
            Solid::Frustum {
                start,
                end,
                r_start,
                r_end,
            } => {
                let d = sub(end, start);
                Ball {
                    centre: std::array::from_fn(|k| (start[k] + end[k]) * 0.5),
                    radius: dot(d, d).sqrt() * 0.5 + r_start.max(r_end) + 0.350001,
                }
            }
            Solid::Ball(b) => b,
        })
        .collect();
    let mut candidates = Vec::new();
    bvh.ball_candidates(&balls, &mut candidates);
    out.clear();
    out.extend(cones.iter().zip(candidates).map(|(s, candidates)| {
        let Solid::Frustum {
            start,
            end,
            r_start,
            r_end,
        } = *s
        else {
            return Measure::Unverified("departure needs a cone");
        };
        let d = sub(end, start);
        let length = dot(d, d).sqrt();
        if !valid_point(start)
            || !valid_point(end)
            || !r_start.is_finite()
            || !r_end.is_finite()
            || bvh
                .mesh()
                .bounds()
                .min
                .iter()
                .chain(&bvh.mesh().bounds().max)
                .any(|v| !v.is_finite() || v.abs() > 1000.0)
            || length <= 1e-8
            || r_start <= 0.0
            || r_end < r_start
            || r_end > 10.0
        {
            return Measure::Unverified("invalid departure cone");
        }
        let axis = d.map(|v| v / length);
        let slope = (r_end - r_start) / length;
        let back = if slope > 0.0 {
            0.1_f64.min(r_start / (2.0 * slope))
        } else {
            0.1
        };
        for i in candidates {
            let t = triangle(bvh.mesh(), i as usize);
            if frustum_triangle(t, start, end, r_start, r_end).lo >= 0.35 {
                continue;
            }
            let now = disc_triangle(t, start, axis, r_start);
            let before = disc_triangle(
                t,
                std::array::from_fn(|k| start[k] - back * axis[k]),
                axis,
                r_start - back * slope,
            );
            if now.lo < before.hi || disc_triangle(t, end, axis, r_end).lo < 0.32 {
                return Measure::Known(false);
            }
        }
        Measure::Known(true)
    }));
}

fn solid_gap_bvh(bvh: &Bvh<'_>, solid: Solid) -> Measure<GapBounds> {
    let mesh = bvh.mesh();
    if mesh
        .bounds()
        .min
        .iter()
        .chain(&mesh.bounds().max)
        .any(|v| !v.is_finite() || v.abs() > 1000.0)
    {
        return Measure::Unverified("model outside clearance domain");
    }
    let (start, end, ra, rb) = match solid {
        Solid::Ball(b) => (b.centre, b.centre, b.radius, b.radius),
        Solid::Frustum {
            start,
            end,
            r_start,
            r_end,
        } => {
            if dot(sub(start, end), sub(start, end)) < 1e-20 {
                return Measure::Unverified("invalid frustum");
            }
            (start, end, r_start, r_end)
        }
    };
    if !valid_point(start)
        || !valid_point(end)
        || !ra.is_finite()
        || !rb.is_finite()
        || ra.min(rb) <= 0.0
        || ra.max(rb) > 10.0
    {
        return Measure::Unverified(match solid {
            Solid::Ball(_) => "invalid ball",
            _ => "invalid frustum",
        });
    }
    let narrow = |t| match solid {
        Solid::Ball(b) => {
            let plane = plane_distance_bounds(b.centre, t).map_or(0.0, |d| d.lo);
            GapBounds {
                lo: (plane.max(box_gap(b.centre, t)) - b.radius).max(0.0),
                hi: (vertex_witness(b.centre, t) - b.radius).max(0.0),
            }
        }
        _ => frustum_triangle(t, start, end, ra, rb),
    };
    let mid = std::array::from_fn(|k| (start[k] + end[k]) * 0.5);
    let seed = bvh.nearest(mid).tri as usize;
    let best = std::cell::Cell::new(narrow(triangle(mesh, seed)));
    bvh.visit_bounds(
        |lo, hi| {
            let t = [lo, hi, lo];
            let lower = match solid {
                Solid::Ball(b) => (box_gap(b.centre, t) - b.radius).max(0.0),
                _ => {
                    let d = sub(end, start);
                    let length = dot(d, d).sqrt();
                    (0..16)
                        .map(|i| {
                            let u = i as f64 / 16.0;
                            let v = (i + 1) as f64 / 16.0;
                            let p = std::array::from_fn(|k| start[k] + (u + v) * 0.5 * d[k]);
                            let r = (ra + (rb - ra) * u).max(ra + (rb - ra) * v);
                            box_gap(p, t) - length / 32.0 - r - ARITHMETIC_MM
                        })
                        .fold(f64::INFINITY, f64::min)
                        .max(0.0)
                }
            };
            // This is no larger than either triangle narrow-phase lower bound.
            // Prune beyond BOTH current minima to retain reference-identical bounds.
            lower <= best.get().hi + ARITHMETIC_MM
        },
        |i| {
            let b = narrow(triangle(mesh, i as usize));
            let old = best.get();
            best.set(GapBounds {
                lo: old.lo.min(b.lo),
                hi: old.hi.min(b.hi),
            });
        },
    );
    Measure::Known(best.get())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thin_triangle_disc_witnesses_keep_the_analytic_plane_distance_enclosed() {
        let t = [[-1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [1.0, 1.0 + 1e-12, 0.0]];
        let b = disc_triangle(t, [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], 0.1);
        assert!(b.lo <= 1.0 && b.hi >= 1.0, "{b:?}");
        assert!(b.hi - b.lo < 1e-5, "{b:?}");
    }

    #[test]
    fn randomized_bvh_solids_match_brute_force() {
        let mesh = crate::bvh::query_test_mesh();
        let bvh = Bvh::build(&mesh).unwrap();
        let mut seed = 293_u64;
        let solids: Vec<_> = (0..64)
            .flat_map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let i = (seed >> 32) as usize % 100;
                let z = 0.3 + (seed & 65535) as f64 / 65535.0;
                let p = [
                    (i % 10) as f64 * 4.0 + 0.5,
                    (i / 10) as f64 * 4.0 + 0.5,
                    i as f64 * 0.01 + z,
                ];
                [
                    Solid::Ball(Ball {
                        centre: p,
                        radius: 0.1,
                    }),
                    Solid::Frustum {
                        start: p,
                        end: [p[0] + 0.3, p[1] - 0.2, p[2] + 1.0],
                        r_start: 0.1,
                        r_end: 0.3,
                    },
                ]
            })
            .collect();
        let mut out = Vec::new();
        solid_gaps(&bvh, &solids, &mut out);
        for (&solid, actual) in solids.iter().zip(out) {
            assert_eq!(actual, solid_gap_reference(&mesh, solid));
        }
    }

    #[test]
    fn randomized_long_solids_find_closer_end_walls_than_midpoint_seeds() {
        // Isolated squares cannot exercise pruning: their midpoint seed already
        // supplies the answer. Here every seed is a wall beside the midpoint,
        // while the closest wall is beside one end of a long, possibly tilted rod.
        // Thin rods expose a 0.2 mm pruning subtraction; column-sized rods expose
        // a 1.0 mm subtraction that would turn a sub-H6 gap into an admission.
        // This is not a proof against arbitrarily small pruning errors or all meshes.
        let mut rng = 0x6a09_e667_f3bc_c909_u64;
        let mut random = || {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            (rng >> 32) as f64 / u32::MAX as f64
        };
        let mut vertices = Vec::new();
        let mut solids = Vec::new();
        let mut truths = Vec::new();
        for case in 0..64 {
            for column in [false, true] {
                let slot = solids.len();
                let origin = [
                    (slot % 16) as f64 * 20.0 + random(),
                    (slot / 16) as f64 * 20.0 + random(),
                    2.0 + random(),
                ];
                let length = 8.0 + 4.0 * random();
                let angle = random() - 0.5;
                let axis = [0.0, angle.sin(), angle.cos()];
                let lateral = [0.0, angle.cos(), -angle.sin()];
                let radius = if column { 0.25 } else { 0.052 } + 0.005 * random();
                let seed_distance = if column { 0.7 } else { 0.18 } + 0.01 * random();
                let truth = if column { 0.1 } else { 0.03 } + 0.01 * random();
                let point = |x, y, u| {
                    std::array::from_fn(|k| {
                        origin[k]
                            + if k == 0 { x } else { 0.0 }
                            + y * lateral[k]
                            + u * length * axis[k]
                    })
                };
                // Both walls have vertices at the narrow phase's axis stations;
                // the end wall has exact solid distance `truth` (its plane is
                // parallel to the axis and its edge contains the radial witness).
                for (distance, u) in [(seed_distance, 0.5), (radius + truth, 1.0 / 32.0)] {
                    vertices.extend([
                        point(distance, 0.0, u - 1.0 / 32.0),
                        point(distance, 0.0, u),
                        point(distance, 0.3, u + 1.0 / 32.0),
                    ]);
                }
                // Random clutter makes a real multi-level BVH. None can beat the
                // end wall: its x distance exceeds every rod radius by > 2 mm.
                for _ in 0..6 {
                    let x = 3.0 + random();
                    let y = random() - 0.5;
                    let u = random();
                    vertices.extend([point(x, y, u), point(x, y + 0.2, u), point(x, y, u + 0.02)]);
                }
                solids.push(Solid::Frustum {
                    start: origin,
                    end: point(0.0, 0.0, 1.0),
                    r_start: radius,
                    r_end: radius,
                });
                truths.push((case, column, truth));
            }
        }
        let mesh = Mesh::from_soup(
            &vertices.iter().map(|p| p[0]).collect::<Vec<_>>(),
            &vertices.iter().map(|p| p[1]).collect::<Vec<_>>(),
            &vertices.iter().map(|p| p[2]).collect::<Vec<_>>(),
        )
        .unwrap();
        let bvh = Bvh::build(&mesh).unwrap();
        assert_eq!(mesh.triangle_count(), 1024);
        assert!(bvh.node_count() > 1);
        let mut out = Vec::new();
        solid_gaps_with_workers(&bvh, &solids, &mut out, 3);
        let mut non_seed_minima = 0;
        let mut false_h6_passes_if_seed_only = 0;
        let mut mismatches = Vec::new();
        let mut column_mismatches = 0;
        for ((&solid, actual), &(case, column, truth)) in solids.iter().zip(out).zip(&truths) {
            let Solid::Frustum {
                start,
                end,
                r_start,
                r_end,
            } = solid
            else {
                unreachable!()
            };
            let mid = std::array::from_fn(|k| (start[k] + end[k]) * 0.5);
            let seed = bvh.nearest(mid).tri as usize;
            let seed_bound = frustum_triangle(triangle(&mesh, seed), start, end, r_start, r_end);
            // The midpoint seed must NOT already supply either minimum. Compare
            // the batch path to the independent whole-mesh traversal, not itself.
            let reference = solid_gap_reference(&mesh, solid);
            let bound = reference.known().unwrap();
            assert!(
                seed_bound.lo > bound.hi + 0.01,
                "case {case}: {seed_bound:?}, {bound:?}"
            );
            // Building and transforming these coordinates (magnitude < 320 mm)
            // takes < 100 f64 operations: 100 * EPSILON * 320 < 1e-10 mm.
            assert!(
                bound.lo <= truth + 1e-10 && truth <= bound.hi + 1e-10,
                "case {case}: {bound:?}, truth={truth}"
            );
            if column {
                assert!(seed_bound.lo > 0.32 && bound.lo < 0.32 && truth < 0.32);
                false_h6_passes_if_seed_only += 1;
            } else {
                assert!(seed_bound.hi < 0.2);
            }
            non_seed_minima += 1;
            if actual != reference {
                column_mismatches += usize::from(column);
                mismatches.push(format!(
                    "case {case}, column={column}: batch={actual:?}, reference={reference:?}"
                ));
            }
        }
        assert_eq!(non_seed_minima, 128);
        assert_eq!(false_h6_passes_if_seed_only, 64);
        eprintln!(
            "BVH pruning witnesses: {non_seed_minima}/128 non-seed minima, \
             {false_h6_passes_if_seed_only}/64 column gaps would falsely pass H6 with seed only; \
             batch/reference mismatches: {} thin, {column_mismatches} column",
            mismatches.len() - column_mismatches
        );
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    }

    #[test]
    fn lateral_cap_edge_encloses_analytic_gap_between_axis_samples() {
        // The triangle lies in the start-cap plane, outside its rim. Its closest
        // point is (0, 0.35, 0); the cylinder rim is (0, 0.25, 0): gap = 0.10.
        // The first axis sample is 0.625 mm away. Dropping the half-length term
        // incorrectly reports lo > 0.46. Infinite-plane distance is zero here,
        // so this witness specifically exercises the finite box bound.
        let m = Mesh::new(
            vec![-0.01, 0.01, 0.0],
            vec![0.35, 0.35, 0.36],
            vec![0.0; 3],
            vec![0, 1, 2],
        )
        .unwrap();
        let solids = [Solid::Frustum {
            start: [0.0; 3],
            end: [0.0, 0.0, 20.0],
            r_start: 0.25,
            r_end: 0.25,
        }];
        let mut out = Vec::new();
        solid_gaps(&Bvh::build(&m).unwrap(), &solids, &mut out);
        let bound = out[0].known().unwrap();
        let truth = 0.35 - 0.25;
        assert!(
            bound.lo <= truth && truth <= bound.hi,
            "{bound:?}, truth={truth}"
        );
        assert_eq!(out[0], solid_gap_reference(&m, solids[0]));
        eprintln!("finite cap-edge analytic truth={truth} mm, interval={bound:?}");
    }

    #[test]
    fn finite_cap_plane_and_parallel_plane() {
        let m = Mesh::new(
            vec![-5., 5., 0.],
            vec![-5., -5., 5.],
            vec![0.; 3],
            vec![0, 1, 2],
        )
        .unwrap();
        let solids = [
            Solid::Frustum {
                start: [0., 0., 0.33],
                end: [0., 0., 2.],
                r_start: 0.25,
                r_end: 0.25,
            },
            Solid::Frustum {
                start: [-1., 0., 0.58],
                end: [1., 0., 0.58],
                r_start: 0.25,
                r_end: 0.25,
            },
        ];
        let mut out = Vec::new();
        solid_gaps(&Bvh::build(&m).unwrap(), &solids, &mut out);
        for (s, r) in solids.into_iter().zip(out) {
            assert_eq!(r, solid_gap_reference(&m, s));
            let b = r.known().unwrap();
            assert!(b.lo > 0.32999 && b.lo <= 0.33);
        }
    }
}
