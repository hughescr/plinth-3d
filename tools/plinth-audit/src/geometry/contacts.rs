//! Report 4: straight contacts that arrive off the surface normal.
//!
//! A straight contact is a tip with a unique taper whose chain does not reach
//! a joint (see [`super::heads`]). For each one:
//!
//! * the arrival axis is the direction from the ball centre along the taper
//!   (the shaft), pointing away from the model;
//! * the local normal is the area-weighted outward normal of the model
//!   triangles within `max(2R, 0.15)` mm of the ball centre;
//! * `x` is the angle between them, and the contact skims when
//!   `(g + r_t)(sec x - 1) > layer + pixel` (`g`, layer and pixel from
//!   [`super::Params`], that is the registry). `r_t` is the taper's start
//!   radius: the radius of the shaft that is drawn at the ball. A
//!   non-positive cosine skims;
//! * the shaft clearance past the ball is the smallest distance from the
//!   shaft's lateral surface to the model, over the axial range from the ball
//!   surface out to `R + (g + r_t) sec x` from the ball centre (the skim zone;
//!   the whole taper when the arrival grazes), negative = inside. It excludes
//!   points inside the ball sphere.

use super::{par_map, ring_points, GeoCtx};
use crate::graph::Link;
use crate::mesh::RayHit;
use crate::vec3::{self as v, Aabb, V3};

/// Floor of the normal-estimation radius, mm.
pub const NORMAL_RADIUS_FLOOR_MM: f64 = 0.15;
/// Axial spacing of the shaft clearance rings, mm.
const CLEARANCE_STEP_MM: f64 = 0.02;
const CLEARANCE_AZIMUTH: usize = 24;
/// Points nearer than this to the model get an inside test, mm.
const SIGN_NEAR_MM: f64 = 0.05;

#[derive(Clone, Debug)]
pub struct ContactRow {
    pub tip: usize,
    pub ball_r_mm: f64,
    pub tip_r_mm: f64,
    pub ball_height_mm: Option<f64>,
    pub off_normal_deg: Option<f64>,
    pub skims: bool,
    pub shaft_clearance_mm: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct ContactReport {
    pub straight: usize,
    pub off_normal_beyond_skim: usize,
    pub rows: Vec<ContactRow>,
}

/// The skim law, `g`, layer and pixel passed in.
#[must_use]
pub fn skims(r_t: f64, cos_x: f64, gap: f64, allowance: f64) -> bool {
    if cos_x.is_nan() || cos_x <= 0.0 {
        return true;
    }
    (gap + r_t) * (1.0 / cos_x - 1.0) > allowance
}

fn area_weighted_normal(ctx: &GeoCtx, centre: V3, rho: f64) -> Option<V3> {
    let bb = Aabb {
        min: [centre[0] - rho, centre[1] - rho, centre[2] - rho],
        max: [centre[0] + rho, centre[1] + rho, centre[2] + rho],
    };
    let mut acc = [0.0; 3];
    for t in ctx.mesh.triangles_in_box(&bb) {
        let [a, b, c] = ctx.mesh.tri(t);
        if v::point_triangle_closest(centre, a, b, c).0 <= rho {
            acc = v::add(acc, v::mul(ctx.mesh.normals[t], ctx.mesh.areas[t]));
        }
    }
    v::normalize(acc)
}

fn shaft_clearance(
    ctx: &GeoCtx,
    centre: V3,
    ball_r: f64,
    taper: usize,
    r_t: f64,
    cos_x: Option<f64>,
    hits: &mut Vec<RayHit>,
) -> Option<f64> {
    let t = ctx.rec(taper);
    let (p0, p1, r0, r1) = if v::dist(t.start_mm, centre) <= v::dist(t.end_mm, centre) {
        (t.start_mm, t.end_mm, t.r_start, t.r_end)
    } else {
        (t.end_mm, t.start_mm, t.r_end, t.r_start)
    };
    let axis_v = v::sub(p1, p0);
    let len = v::norm(axis_v);
    let u = v::normalize(axis_v)?;
    // The skim zone: out to the axial distance at which a shaft leaning `x`
    // off the normal has left the final gap the way an on-normal arrival
    // would, `(g + r_t) sec x` past the ball centre. A grazing or unknown
    // arrival (cos <= 0) has no end to its zone: the whole taper.
    let zone_end = match cos_x {
        Some(c) if c > 0.0 => ball_r + (ctx.params.gap_mm + r_t) / c,
        _ => len,
    }
    .min(len);
    let n = ((zone_end / CLEARANCE_STEP_MM).ceil() as usize).max(1);
    let mut pts: Vec<V3> = Vec::new();
    for i in 0..=n {
        let s = zone_end * i as f64 / n as f64;
        let f = if len > 0.0 { s / len } else { 0.0 };
        let r = r0 + (r1 - r0) * f;
        let phase = if i % 2 == 1 {
            std::f64::consts::PI / CLEARANCE_AZIMUTH as f64
        } else {
            0.0
        };
        for p in ring_points(v::add(p0, v::mul(u, s)), u, r, CLEARANCE_AZIMUTH, phase) {
            if v::dist(p, centre) > ball_r {
                pts.push(p);
            }
        }
    }
    if pts.is_empty() {
        return None;
    }
    let mut bb = Aabb::empty();
    for p in &pts {
        bb.add_point(*p);
    }
    let reach = 1.0 + ctx.params.gap_mm;
    let tris: Vec<[V3; 3]> = ctx
        .mesh
        .triangles_in_box(&bb.expanded(reach))
        .iter()
        .map(|&i| ctx.mesh.tri(i))
        .collect();
    let mut best = f64::INFINITY;
    for p in pts {
        let mut d = f64::INFINITY;
        for t in &tris {
            d = d.min(v::point_triangle_closest(p, t[0], t[1], t[2]).0);
        }
        let signed = if d < SIGN_NEAR_MM && ctx.inside(p, hits) == Some(true) {
            -d
        } else {
            d
        };
        best = best.min(signed);
    }
    best.is_finite().then_some(best)
}

#[must_use]
pub fn measure(ctx: &GeoCtx, head_tips: &[usize]) -> ContactReport {
    let head_set: std::collections::HashSet<usize> = head_tips.iter().copied().collect();
    let tips: Vec<(usize, usize)> = ctx
        .model
        .contact_records()
        .into_iter()
        .filter(|t| !head_set.contains(t))
        .filter_map(|t| match ctx.graph.taper_for_tip(ctx.model, t) {
            Link::Unique(tp) => Some((t, tp)),
            _ => None,
        })
        .collect();
    let allowance = ctx.params.allowance_mm();
    let gap = ctx.params.gap_mm;
    let rows = par_map(tips.len(), ctx.params.threads, |k| {
        let (tip, taper) = tips[k];
        let tr = ctx.rec(tip);
        let t = ctx.rec(taper);
        let centre = tr.ball_centre().unwrap_or([0.0; 3]);
        let r = tr.r_start;
        let (far, r_t) = if v::dist(t.start_mm, centre) <= v::dist(t.end_mm, centre) {
            (t.end_mm, t.r_start)
        } else {
            (t.start_mm, t.r_end)
        };
        let axis = v::normalize(v::sub(far, centre));
        let normal = area_weighted_normal(ctx, centre, (2.0 * r).max(NORMAL_RADIUS_FLOOR_MM));
        let cos_x = match (axis, normal) {
            (Some(a), Some(n)) => Some(v::dot(a, n).clamp(-1.0, 1.0)),
            _ => None,
        };
        let mut hits = Vec::new();
        let (d, inside) = ctx.signed_to_model(centre, &mut hits);
        ContactRow {
            tip,
            ball_r_mm: r,
            tip_r_mm: r_t,
            ball_height_mm: inside.map(|i| if i { -d } else { d }),
            off_normal_deg: cos_x.map(|c| c.acos().to_degrees()),
            skims: cos_x.is_some_and(|c| skims(r_t, c, gap, allowance)),
            shaft_clearance_mm: shaft_clearance(ctx, centre, r, taper, r_t, cos_x, &mut hits),
        }
    });
    ContactReport {
        straight: rows.len(),
        off_normal_beyond_skim: rows.iter().filter(|r| r.skims).count(),
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_square_arrival_never_skims_and_a_grazing_one_always_does() {
        assert!(!skims(0.1, 1.0, 0.3, 0.069));
        assert!(skims(0.1, 0.0, 0.3, 0.069));
        assert!(skims(0.1, -0.5, 0.3, 0.069));
    }

    #[test]
    fn the_skim_law_switches_at_its_own_angle() {
        // (0.3 + 0.1)(sec x - 1) = 0.069  =>  sec x = 1.1725
        let x = (1.0f64 / 1.1725).acos();
        assert!(skims(0.1, (x + 0.01).cos(), 0.3, 0.069));
        assert!(!skims(0.1, (x - 0.01).cos(), 0.3, 0.069));
    }
}
