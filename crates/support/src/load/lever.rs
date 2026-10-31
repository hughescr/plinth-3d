//! The rigid-lever sharing index and the load-centroid tests (design
//! `step11-construction.md` section 4.3).
//!
//! The part is a rigid body on axial springs at the contacts, stiffness proportional to the
//! section area; the layer's resultant acts at the layer centroid; a spring can only pull.
//! The result is unmeasured and very sensitive to a near-straight row of contacts.

/// One spring: position in xy, mm, and stiffness (section area, mm²).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    /// x, y of the contact.
    pub xy: [f64; 2],
    /// Stiffness, proportional to the section area.
    pub k: f64,
}

/// The forces on the springs and whether the layer's resultant could be balanced.
#[derive(Debug, Clone, PartialEq)]
pub struct Lever {
    /// The force on each spring, in the units of the load (mm² of peel area). Springs that
    /// cannot take a pull get 0.
    pub force: Vec<f64>,
    /// Relative residual of the three equilibrium equations (force, two moments). Near 0
    /// when the resultant is balanced; of order 1 when it is not.
    pub residual: f64,
}

fn solve3(a: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let mut m = [
        [a[0][0], a[0][1], a[0][2], b[0]],
        [a[1][0], a[1][1], a[1][2], b[1]],
        [a[2][0], a[2][1], a[2][2], b[2]],
    ];
    for c in 0..3 {
        let p = (c..3).max_by(|&i, &j| m[i][c].abs().total_cmp(&m[j][c].abs()))?;
        if m[p][c].abs() < 1e-300 {
            return None;
        }
        m.swap(c, p);
        let pivot = m[c];
        for row in m.iter_mut().skip(c + 1) {
            let f = row[c] / pivot[c];
            for (x, p) in row[c..].iter_mut().zip(&pivot[c..]) {
                *x -= f * p;
            }
        }
    }
    let mut x = [0.0; 3];
    for r in (0..3).rev() {
        let s: f64 = ((r + 1)..3).map(|c| m[r][c] * x[c]).sum();
        x[r] = (m[r][3] - s) / m[r][r];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

/// Distribute a vertical resultant `load` acting at `centroid` over `springs`.
#[must_use]
pub fn rigid_lever(springs: &[Spring], load: f64, centroid: [f64; 2]) -> Lever {
    let n = springs.len();
    let mut on = vec![true; n];
    let mut force = vec![0.0; n];
    for _ in 0..=n {
        let idx: Vec<usize> = (0..n).filter(|&i| on[i]).collect();
        force.iter_mut().for_each(|f| *f = 0.0);
        if idx.is_empty() {
            break;
        }
        let ksum: f64 = idx.iter().map(|&i| springs[i].k).sum();
        let cx = idx
            .iter()
            .map(|&i| springs[i].k * springs[i].xy[0])
            .sum::<f64>()
            / ksum;
        let cy = idx
            .iter()
            .map(|&i| springs[i].k * springs[i].xy[1])
            .sum::<f64>()
            / ksum;
        let mut a = [[0.0; 3]; 3];
        for &i in &idx {
            let (k, dx, dy) = (springs[i].k, springs[i].xy[0] - cx, springs[i].xy[1] - cy);
            let row = [1.0, dx, dy];
            for r in 0..3 {
                for c in 0..3 {
                    a[r][c] += k * row[r] * row[c];
                }
            }
        }
        // Resultant about the spring centroid.
        let b = [load, load * (centroid[0] - cx), load * (centroid[1] - cy)];
        let trace = a[0][0] + a[1][1] + a[2][2];
        for (d, row) in a.iter_mut().enumerate() {
            row[d] += 1e-9 * trace;
        }
        let Some([c0, c1, c2]) = solve3(a, b) else {
            break;
        };
        for &i in &idx {
            force[i] =
                springs[i].k * (c0 + c1 * (springs[i].xy[0] - cx) + c2 * (springs[i].xy[1] - cy));
        }
        let worst = idx
            .iter()
            .copied()
            .min_by(|&i, &j| force[i].total_cmp(&force[j]));
        match worst {
            Some(w) if force[w] < -1e-12 * load.abs().max(1e-30) => on[w] = false,
            _ => break,
        }
    }
    let residual = residual_of(springs, &force, load, centroid);
    Lever { force, residual }
}

fn residual_of(springs: &[Spring], force: &[f64], load: f64, centroid: [f64; 2]) -> f64 {
    if load.abs() < 1e-300 {
        return 0.0;
    }
    let sum: f64 = force.iter().sum();
    let mx: f64 = springs
        .iter()
        .zip(force)
        .map(|(s, f)| f * (s.xy[0] - centroid[0]))
        .sum();
    let my: f64 = springs
        .iter()
        .zip(force)
        .map(|(s, f)| f * (s.xy[1] - centroid[1]))
        .sum();
    // Moments about the load centroid should vanish; scale them by a length of 1 mm so a
    // millimetre of lever arm on the whole load counts as a residual of 1.
    ((sum - load) / load)
        .abs()
        .max((mx / load).abs())
        .max((my / load).abs())
}

/// Distance from `q` to the convex hull of `pts` (0 inside). With one point it is the point
/// distance; with collinear points the hull is the segment.
#[must_use]
pub fn distance_to_hull(pts: &[[f64; 2]], q: [f64; 2]) -> f64 {
    let hull = convex_hull(pts);
    match hull.len() {
        0 => f64::INFINITY,
        1 => (q[0] - hull[0][0]).hypot(q[1] - hull[0][1]),
        2 => dist_to_segment(q, hull[0], hull[1]),
        n => {
            let mut inside = true;
            let mut best = f64::INFINITY;
            for i in 0..n {
                let (a, b) = (hull[i], hull[(i + 1) % n]);
                let cross = (b[0] - a[0]) * (q[1] - a[1]) - (b[1] - a[1]) * (q[0] - a[0]);
                if cross < 0.0 {
                    inside = false;
                }
                best = best.min(dist_to_segment(q, a, b));
            }
            if inside { 0.0 } else { best }
        }
    }
}

fn dist_to_segment(q: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (ux, uy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = ux * ux + uy * uy;
    let t = if len2 > 0.0 {
        (((q[0] - a[0]) * ux + (q[1] - a[1]) * uy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (q[0] - a[0] - t * ux).hypot(q[1] - a[1] - t * uy)
}

/// Andrew's monotone chain; counter-clockwise, no repeated first point. Collinear input
/// gives the two end points.
fn convex_hull(pts: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut p: Vec<[f64; 2]> = pts.to_vec();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup_by(|a, b| a[0].total_cmp(&b[0]).is_eq() && a[1].total_cmp(&b[1]).is_eq());
    if p.len() < 3 {
        return p;
    }
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut h: Vec<[f64; 2]> = Vec::with_capacity(2 * p.len());
    for &q in &p {
        while h.len() >= 2 && cross(h[h.len() - 2], h[h.len() - 1], q) <= 0.0 {
            h.pop();
        }
        h.push(q);
    }
    let lower = h.len() + 1;
    for &q in p.iter().rev().skip(1) {
        while h.len() >= lower && cross(h[h.len() - 2], h[h.len() - 1], q) <= 0.0 {
            h.pop();
        }
        h.push(q);
    }
    h.pop();
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn a_centred_load_on_three_equal_springs_splits_equally() {
        let s = [
            Spring {
                xy: [0.0, 0.0],
                k: 1.0,
            },
            Spring {
                xy: [3.0, 0.0],
                k: 1.0,
            },
            Spring {
                xy: [0.0, 3.0],
                k: 1.0,
            },
        ];
        let l = rigid_lever(&s, 6.0, [1.0, 1.0]);
        assert!(l.force.iter().all(|&f| close(f, 2.0)), "{:?}", l.force);
        assert!(l.residual < 1e-6);
    }

    #[test]
    fn the_force_follows_the_stiffness_and_the_lever_arm() {
        let s = [
            Spring {
                xy: [0.0, 0.0],
                k: 3.0,
            },
            Spring {
                xy: [4.0, 0.0],
                k: 1.0,
            },
            Spring {
                xy: [2.0, 4.0],
                k: 1.0,
            },
        ];
        // The stiffness-weighted centre is (1.2, 0.8): a load there is shared by stiffness.
        let l = rigid_lever(&s, 5.0, [1.2, 0.8]);
        assert!(
            close(l.force[0], 3.0) && close(l.force[1], 1.0) && close(l.force[2], 1.0),
            "{:?}",
            l.force
        );
    }

    #[test]
    fn a_load_off_a_straight_row_cannot_be_balanced() {
        let s: Vec<Spring> = (0..4)
            .map(|i| Spring {
                xy: [f64::from(i), 0.0],
                k: 1.0,
            })
            .collect();
        let off = rigid_lever(&s, 4.0, [1.5, 2.0]);
        assert!(off.residual > 0.5, "{}", off.residual);
        let on = rigid_lever(&s, 4.0, [1.5, 0.0]);
        assert!(on.residual < 1e-6, "{}", on.residual);
    }

    #[test]
    fn springs_never_push() {
        // A load far outside the triangle would need a push on one spring; it is dropped.
        let s = [
            Spring {
                xy: [0.0, 0.0],
                k: 1.0,
            },
            Spring {
                xy: [2.0, 0.0],
                k: 1.0,
            },
            Spring {
                xy: [1.0, 2.0],
                k: 1.0,
            },
        ];
        let l = rigid_lever(&s, 3.0, [4.0, 0.0]);
        assert!(l.force.iter().all(|&f| f >= -1e-9), "{:?}", l.force);
    }

    #[test]
    fn the_hull_distance_is_zero_inside_and_positive_outside() {
        let sq = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0], [2.0, 2.0]];
        assert!(close(distance_to_hull(&sq, [1.0, 1.0]), 0.0));
        assert!(close(distance_to_hull(&sq, [6.0, 2.0]), 2.0));
        assert!(close(distance_to_hull(&sq, [7.0, 8.0]), 5.0));
        let line = [[0.0, 0.0], [1.0, 0.0], [3.0, 0.0]];
        assert!(close(distance_to_hull(&line, [1.0, 2.0]), 2.0));
        assert!(close(distance_to_hull(&line, [5.0, 0.0]), 2.0));
        assert!(close(distance_to_hull(&[[1.0, 1.0]], [4.0, 5.0]), 5.0));
        assert!(distance_to_hull(&[], [0.0, 0.0]).is_infinite());
    }
}
