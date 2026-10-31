//! The canonical mesh of a support record: the exact mesh that the native shape is defined to
//! have for the record's fields (`docs/format/chitubox-v1.4.0.md` sections 10.1 and 19.2),
//! rebuilt here in f64 from the fields alone.
//!
//! A record whose written mesh is not this mesh, vertex for vertex within two units in the
//! last place of its largest coordinate (the step-10 tolerance, section 19.3), or whose
//! topology differs, is not decoded: every geometric rule is then unverified for it. Once a
//! record has passed, the field-defined (analytic) solid and the written mesh differ only by
//! two known amounts (the hard-limit verdicts are read from the written mesh; these amounts
//! are reported as information and bound the written ball's in-sphere):
//!
//! * the written mesh stands at most `Canon::round` outside the analytic solid (the vertex
//!   rounding), and
//! * the analytic solid stands at most `Canon::sag` outside the written mesh (the inscribed
//!   polygon of the canonical mesh, plus the same rounding).
//!
//! The end faces of a frustum and the poles of a ball sit at the field positions to within
//! the rounding, so no end or section position differs by more than `round`.
//!
//! Shapes:
//!
//! * **Ball (7, 8, 9).** Centre (start + end) / 2; the poles are the fields' start and end;
//!   15 rings of 16 vertices at polar angles 11.25 degrees x k from the start pole; 16 + 14 x
//!   32 + 16 triangles in the documented order.
//! * **Frustum (1 to 5).** Two rings of 16 vertices at 22.5 degrees x j, counter-clockwise
//!   from +x in a frame whose axis points straight down, turned onto the real axis by the
//!   shortest arc. The 32 side triangles are in the documented order. The two caps (14
//!   triangles each) are accepted as any triangulation of their ring that faces outward: the
//!   triangulation table is not documented, and every triangulation of a convex cap is the
//!   same solid. Type 5 is built from the record's end (radius r1) to its start (radius r2).
//! * **Raft pad and bridge (6).** The vertex set of the 20-sided prism (rim radius r1 / 2) or
//!   the box (half width r1 / 2), at the z range the mesh itself holds (the record stores z
//!   = 0): the mesh is a closed, convex, oriented shell whose vertices are exactly these, so
//!   it is exactly that solid whatever its triangulation.

use crate::mesh::Welded;
use crate::v3::{V, add, cross, dist, dot, f, len, mul, sub};
use std::f64::consts::{PI, TAU};

/// Sides of a native frustum ring.
pub const RING: usize = 16;
/// Rings of a native ball between its poles.
pub const BALL_RINGS: usize = 15;
/// Sides of a raft pad.
pub const PAD_SIDES: usize = 20;

/// Units in the last place a written vertex may differ from the canonical one, per coordinate.
pub const ULPS: f64 = 2.0;

/// Spacing of f32 numbers at `x`.
#[must_use]
pub fn ulp32(x: f64) -> f64 {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the spacing of f32 near x is wanted"
    )]
    let a = (x.abs() as f32).max(f32::MIN_POSITIVE);
    f64::from(f32::from_bits(a.to_bits() + 1)) - f64::from(a)
}

/// The canonical mesh of one record.
#[derive(Debug, Clone)]
pub struct Canon {
    /// Canonical position of every labelled vertex, as the producer builds it (what a written
    /// mesh is compared with).
    pub pos: Vec<V>,
    /// The same vertices with the exact shortest-arc turn: the canonical mesh that lies
    /// inscribed in the analytic solid. Equal to `pos` unless the producer's half-turn rule
    /// applies.
    pub inner: Vec<V>,
    /// Triangles in the documented order, as labels, for the part that has a documented order
    /// (empty for a raft part, which is compared as a vertex set).
    pub tris: Vec<[u32; 3]>,
    /// Triangle ranges that only need to be a valid outward triangulation of a ring.
    pub caps: Vec<Cap>,
    /// Per-coordinate tolerance, mm.
    pub tol_coord: f64,
    /// The most a written vertex may stand from its canonical position, mm.
    pub round: f64,
    /// How far the analytic solid may stand outside the canonical mesh, mm (no rounding).
    pub sag: f64,
}

/// A cap: `count` triangles from triangle `first`, on the labels in `ring`, facing `outward`.
#[derive(Debug, Clone)]
pub struct Cap {
    pub first: usize,
    pub count: usize,
    pub ring: std::ops::Range<u32>,
    pub outward: V,
}

/// How the straight-down frame is turned onto the real axis `u`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    /// The shortest arc, exactly.
    Arc,
    /// A half turn about +y: what Qt's `QQuaternion::rotationTo` returns when `1 + a.u` is
    /// null within 1e-5 (an axis within about 4.5e-3 rad of straight up). The mesh axis is
    /// then exactly vertical, a little off the fields' axis: that offset is part of `round`.
    Half,
}

/// `1 + a.u` for the straight-down `a` and a unit `u`, written to stay exact near straight up.
fn arc_d(u: V) -> f64 {
    if u[2] >= 0.0 {
        (u[0] * u[0] + u[1] * u[1]) / (1.0 + u[2])
    } else {
        1.0 - u[2]
    }
}

/// Qt's "fuzzy null" bound for a single-precision value.
const QT_FUZZY: f64 = 1e-5;

/// The turns a native mesh for the unit axis `u` may have been built with. Near the Qt
/// threshold the producer's single-precision `1 + a.u` may land on either side, so both are
/// allowed within 10 percent of it.
#[must_use]
pub fn turns(u: V) -> Vec<Turn> {
    let d = arc_d(u);
    let mut out = Vec::with_capacity(2);
    if d >= 0.9 * QT_FUZZY {
        out.push(Turn::Arc);
    }
    if d <= 1.1 * QT_FUZZY {
        out.push(Turn::Half);
    }
    out
}

/// The rotation that turns the axis pointing straight down onto `u`, as a unit quaternion
/// `(w, v)`.
fn rotation(turn: Turn, u: V) -> (f64, V) {
    let v = cross([0.0, 0.0, -1.0], u);
    let w = arc_d(u);
    let n = (w * w + dot(v, v)).sqrt();
    if turn == Turn::Half || n == 0.0 {
        return (0.0, [0.0, 1.0, 0.0]);
    }
    (w / n, mul(v, 1.0 / n))
}

fn rotate((w, v): (f64, V), x: V) -> V {
    let t = mul(cross(v, x), 2.0);
    add(add(x, mul(t, w)), cross(v, t))
}

fn max_abs(pos: &[V]) -> f64 {
    pos.iter().flatten().fold(0.0f64, |m, x| m.max(x.abs()))
}

/// The parts of a canonical mesh that do not depend on the turn.
struct Shape {
    pos: Vec<V>,
    tris: Vec<[u32; 3]>,
    caps: Vec<Cap>,
    sag: f64,
}

/// `built` is the shape with the producer's turn; `ideal` the same with the exact arc. The
/// producer's mesh may stand `disp` from the ideal one (zero for the exact arc).
fn finish(built: Shape, ideal: Option<Vec<V>>) -> Canon {
    let Shape {
        pos,
        tris,
        caps,
        sag,
    } = built;
    let scale = max_abs(&pos);
    let tol_coord = ULPS * ulp32(scale);
    let disp = ideal.as_ref().map_or(0.0, |i| {
        i.iter()
            .zip(&pos)
            .map(|(a, b)| dist(*a, *b))
            .fold(0.0, f64::max)
    });
    // The vertex moves by at most `tol_coord` in each coordinate, so by `sqrt(3)` times that;
    // the f64 reconstruction adds a few units of f64 rounding at the largest coordinate.
    let round = 3.0f64.sqrt() * tol_coord + 8.0 * f64::EPSILON * scale + disp;
    Canon {
        inner: ideal.unwrap_or_else(|| pos.clone()),
        pos,
        tris,
        caps,
        tol_coord,
        round,
        sag,
    }
}

/// The ball with poles `start` and `end` and radius `r`, built with `turn`. `None` unless the
/// radius is positive and finite (the poles being 2r apart is checked by the caller against
/// the written fields).
#[must_use]
pub fn ball(start: V, end: V, r: f64, turn: Turn) -> Option<Canon> {
    if !(r.is_finite() && r > 0.0) {
        return None;
    }
    let axis = sub(end, start);
    let l = len(axis);
    if l <= 0.0 {
        return None;
    }
    let u = mul(axis, 1.0 / l);
    let built = ball_shape(start, end, r, rotation(turn, u));
    let ideal = (turn == Turn::Half).then(|| ball_shape(start, end, r, rotation(Turn::Arc, u)).pos);
    Some(finish(built, ideal))
}

fn ball_shape(start: V, end: V, r: f64, q: (f64, V)) -> Shape {
    let c = mul(add(start, end), 0.5);
    // Labels: 0 start pole, 1 + 16 (k - 1) + j for ring k in 1..=15, 241 end pole.
    let mut pos: Vec<V> = Vec::with_capacity(242);
    pos.push(start);
    for k in 1..=BALL_RINGS {
        let th = PI * k as f64 / 16.0;
        for j in 0..RING {
            let ph = TAU * j as f64 / RING as f64;
            let off = [
                r * th.sin() * ph.cos(),
                r * th.sin() * ph.sin(),
                r * th.cos(),
            ];
            pos.push(add(c, rotate(q, off)));
        }
    }
    pos.push(end);
    let ring = |k: usize, j: usize| -> u32 { (1 + (k - 1) * RING + j % RING) as u32 };
    let (s, e) = (0u32, (1 + BALL_RINGS * RING) as u32);
    let mut tris = Vec::with_capacity(480);
    for j in 0..RING {
        tris.push([s, ring(1, j), ring(1, j + 1)]);
    }
    for k in 1..BALL_RINGS {
        for j in 0..RING {
            tris.push([ring(k, j), ring(k + 1, j), ring(k + 1, j + 1)]);
            tris.push([ring(k, j), ring(k + 1, j + 1), ring(k, j + 1)]);
        }
    }
    for j in 0..RING {
        tris.push([ring(BALL_RINGS, j), e, ring(BALL_RINGS, j + 1)]);
    }
    // The in-sphere of the canonical mesh: the smallest distance from the centre to a face
    // plane. The mesh contains that ball, so the sphere is within `r - d_min` of the mesh.
    let mut d_min = f64::INFINITY;
    for t in &tris {
        let (a, b, d) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
        let n = cross(sub(b, a), sub(d, a));
        let nl = len(n);
        if nl > 0.0 {
            d_min = d_min.min(dot(n, sub(a, c)) / nl);
        }
    }
    let sag = (r - d_min).max(0.0);
    Shape {
        pos,
        tris,
        caps: Vec::new(),
        sag,
    }
}

/// The frustum from `a` (radius `ra`) to `b` (radius `rb`), the mesh start being `a`, built
/// with `turn`.
#[must_use]
pub fn frustum(a: V, b: V, ra: f64, rb: f64, turn: Turn) -> Option<Canon> {
    if !(ra.is_finite() && rb.is_finite() && ra > 0.0 && rb > 0.0) {
        return None;
    }
    let axis = sub(b, a);
    let l = len(axis);
    if l <= 0.0 {
        return None;
    }
    let u = mul(axis, 1.0 / l);
    let built = frustum_shape((a, b, u, l), (ra, rb), rotation(turn, u));
    let ideal = (turn == Turn::Half)
        .then(|| frustum_shape((a, b, u, l), (ra, rb), rotation(Turn::Arc, u)).pos);
    Some(finish(built, ideal))
}

fn frustum_shape((a, _b, u, l): (V, V, V, f64), (ra, rb): (f64, f64), q: (f64, V)) -> Shape {
    // Labels: start ring 0..16, end ring 16..32.
    let mut pos: Vec<V> = Vec::with_capacity(32);
    for (r, z) in [(ra, 0.0), (rb, -l)] {
        for j in 0..RING {
            let ph = TAU * j as f64 / RING as f64;
            pos.push(add(a, rotate(q, [r * ph.cos(), r * ph.sin(), z])));
        }
    }
    let s = |j: usize| (j % RING) as u32;
    let e = |j: usize| (RING + j % RING) as u32;
    let mut tris = Vec::with_capacity(32);
    for j in 0..RING {
        tris.push([s(j), e(j), e(j + 1)]);
        tris.push([s(j), e(j + 1), s(j + 1)]);
    }
    let caps = vec![
        Cap {
            first: 32,
            count: 14,
            ring: 0..16,
            outward: mul(u, -1.0),
        },
        Cap {
            first: 46,
            count: 14,
            ring: 16..32,
            outward: u,
        },
    ];
    // Every section is a regular 16-gon of circumradius r(t): the cone is within
    // `r (1 - cos(pi / 16))` of the polygon.
    let sag = ra.max(rb) * (1.0 - (PI / RING as f64).cos());
    Shape {
        pos,
        tris,
        caps,
        sag,
    }
}

/// The vertex set of a raft pad centred at `(x, y)` with the top at `z_top`.
#[must_use]
pub fn pad(centre: [f64; 2], diameter: f64, thickness: f64, z_top: f64) -> Option<Canon> {
    if !(diameter > 0.0 && thickness > 0.0 && diameter.is_finite() && thickness.is_finite()) {
        return None;
    }
    let r = 0.5 * diameter;
    let mut pos = Vec::with_capacity(2 * PAD_SIDES + 2);
    for z in [z_top, z_top - thickness] {
        for i in 0..PAD_SIDES {
            let ph = TAU * i as f64 / PAD_SIDES as f64;
            pos.push([centre[0] + r * ph.cos(), centre[1] + r * ph.sin(), z]);
        }
        pos.push([centre[0], centre[1], z]);
    }
    Some(finish(
        Shape {
            pos,
            tris: Vec::new(),
            caps: Vec::new(),
            sag: 0.0,
        },
        None,
    ))
}

/// The vertex set of a raft bridge between two pad centres.
#[must_use]
pub fn bridge(
    from: [f64; 2],
    to: [f64; 2],
    width: f64,
    thickness: f64,
    z_top: f64,
) -> Option<Canon> {
    let d = [to[0] - from[0], to[1] - from[1]];
    let l = d[0].hypot(d[1]);
    if !(l > 0.0 && width > 0.0 && thickness > 0.0 && l.is_finite()) {
        return None;
    }
    let n = [-d[1] / l * 0.5 * width, d[0] / l * 0.5 * width];
    let mut pos = Vec::with_capacity(8);
    for z in [z_top, z_top - thickness] {
        for base in [from, to] {
            for sg in [-1.0, 1.0] {
                pos.push([base[0] + sg * n[0], base[1] + sg * n[1], z]);
            }
        }
    }
    Some(finish(
        Shape {
            pos,
            tris: Vec::new(),
            caps: Vec::new(),
            sag: 0.0,
        },
        None,
    ))
}

/// The canonical triangles, caps included (a fan from the first ring vertex, facing outward),
/// for a shape with a documented order. Empty for a raft part. `built` takes the producer's
/// vertices (what a native file holds) rather than the exact-arc ones inscribed in the
/// analytic solid.
#[must_use]
pub fn triangles(c: &Canon, built: bool) -> Vec<[V; 3]> {
    let pos = if built { &c.pos } else { &c.inner };
    let p = |l: u32| pos[l as usize];
    let mut out: Vec<[V; 3]> = c.tris.iter().map(|t| [p(t[0]), p(t[1]), p(t[2])]).collect();
    for cap in &c.caps {
        let (first, last) = (cap.ring.start, cap.ring.end);
        for l in first + 1..last - 1 {
            let t = [p(first), p(l), p(l + 1)];
            let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
            out.push(if dot(n, cap.outward) > 0.0 {
                t
            } else {
                [t[0], t[2], t[1]]
            });
        }
    }
    out
}

/// Why a written mesh is not the canonical one, or the written vertex position of every label.
///
/// # Errors
/// A message naming the first difference.
pub fn match_ordered(canon: &Canon, w: &Welded) -> Result<Vec<V>, String> {
    if w.tris.len() < canon.tris.len() {
        return Err(format!(
            "the mesh has {} triangles, fewer than the native shape's documented {}",
            w.tris.len(),
            canon.tris.len()
        ));
    }
    let mut l2w: Vec<Option<u32>> = vec![None; canon.pos.len()];
    let mut w2l: Vec<Option<u32>> = vec![None; w.verts.len()];
    for (i, ct) in canon.tris.iter().enumerate() {
        for (k, &label) in ct.iter().enumerate() {
            let (l, wi) = (label as usize, w.tris[i][k] as usize);
            match (l2w[l], w2l[wi]) {
                (None, None) => {
                    l2w[l] = Some(wi as u32);
                    w2l[wi] = Some(l as u32);
                }
                (Some(a), Some(b)) if a as usize == wi && b as usize == l => {}
                _ => {
                    return Err(format!(
                        "the mesh connectivity differs from the native shape at triangle {i}"
                    ));
                }
            }
        }
    }
    let mut written = vec![[0.0; 3]; canon.pos.len()];
    for (l, slot) in l2w.iter().enumerate() {
        let Some(wi) = slot else {
            return Err("a native vertex is missing from the mesh".to_owned());
        };
        written[l] = f(w.verts[*wi as usize]);
        close_enough(canon, written[l], canon.pos[l], l)?;
    }
    for cap in &canon.caps {
        for i in cap.first..cap.first + cap.count {
            let Some(tri) = w.tris.get(i) else {
                return Err("a cap triangle is missing".to_owned());
            };
            let mut p = [[0.0; 3]; 3];
            for k in 0..3 {
                let wi = tri[k] as usize;
                let on_ring = w2l[wi].is_some_and(|l| cap.ring.contains(&l));
                if !on_ring {
                    return Err(format!(
                        "cap triangle {i} uses a vertex that is not on its ring"
                    ));
                }
                p[k] = f(w.verts[wi]);
            }
            let n = cross(sub(p[1], p[0]), sub(p[2], p[0]));
            if dot(n, cap.outward) <= 0.0 {
                return Err(format!("cap triangle {i} does not face outward"));
            }
        }
    }
    if w.tris.len()
        != canon
            .caps
            .last()
            .map_or(canon.tris.len(), |c| c.first + c.count)
    {
        return Err(format!(
            "the mesh has {} triangles, not the native count",
            w.tris.len()
        ));
    }
    Ok(written)
}

fn close_enough(canon: &Canon, got: V, want: V, label: usize) -> Result<(), String> {
    let worst = (0..3)
        .map(|k| (got[k] - want[k]).abs())
        .fold(0.0f64, f64::max);
    if worst > canon.tol_coord {
        return Err(format!(
            "vertex {label} is {worst:.3e} mm from the native shape's, more than {ULPS} units in the last place ({:.3e} mm)",
            canon.tol_coord
        ));
    }
    Ok(())
}

/// Match a written vertex set with the canonical set (a raft part): every canonical vertex has
/// exactly one written vertex within the tolerance, and the counts agree.
///
/// # Errors
/// A message naming the first canonical vertex with no match.
pub fn match_set(canon: &Canon, w: &Welded) -> Result<Vec<V>, String> {
    if w.verts.len() != canon.pos.len() {
        return Err(format!(
            "the mesh has {} vertices, the native shape {}",
            w.verts.len(),
            canon.pos.len()
        ));
    }
    let mut used = vec![false; w.verts.len()];
    let mut written = Vec::with_capacity(canon.pos.len());
    for (l, p) in canon.pos.iter().enumerate() {
        let mut best: Option<(usize, f64)> = None;
        for (i, v) in w.verts.iter().enumerate() {
            let d = dist(f(*v), *p);
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        let Some((i, _)) = best else {
            return Err("no vertex".to_owned());
        };
        if used[i] {
            return Err(format!(
                "native vertex {l} and another share one written vertex"
            ));
        }
        used[i] = true;
        close_enough(canon, f(w.verts[i]), *p, l)?;
        written.push(f(w.verts[i]));
    }
    Ok(written)
}

/// The largest per-coordinate difference between `written` and the canonical vertices, in
/// units in the last place of the canonical mesh's largest coordinate.
#[must_use]
pub fn worst_ulps(canon: &Canon, written: &[V]) -> f64 {
    let unit = ulp32(max_abs(&canon.pos));
    canon
        .pos
        .iter()
        .zip(written)
        .flat_map(|(c, w)| (0..3).map(move |k| (c[k] - w[k]).abs()))
        .fold(0.0f64, f64::max)
        / unit
}

/// The measured deviation of the written vertices from the canonical ones, mm: the largest
/// distance between a written vertex and its canonical position, plus the f64 rounding of the
/// reconstruction (8 units in the last place at the largest coordinate). This is the record's
/// own deviation (at most the validation allowance), carried into every bound of a rule that
/// reads the written mesh.
#[must_use]
pub fn deviation_mm(canon: &Canon, written: &[V]) -> f64 {
    let worst = canon
        .pos
        .iter()
        .zip(written)
        .map(|(c, w)| dist(*c, *w))
        .fold(0.0f64, f64::max);
    worst + 8.0 * f64::EPSILON * max_abs(&canon.pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ball_has_the_documented_counts_and_a_known_in_sphere() {
        let c = ball([0.0, 0.0, 1.0], [0.0, 0.0, -1.0], 1.0, Turn::Arc).expect("ball");
        assert_eq!((c.pos.len(), c.tris.len()), (242, 480));
        // The equator-most face is the shallowest: the in-sphere is a little under the radius.
        assert!(c.sag > 0.005 && c.sag < 0.03, "{}", c.sag);
    }

    #[test]
    fn a_frustum_straight_up_is_a_half_turn_about_y() {
        assert_eq!(turns([0.0, 0.0, 1.0]), vec![Turn::Half]);
        assert_eq!(turns([0.0, 0.0, -1.0]), vec![Turn::Arc]);
        let c = frustum([0.0; 3], [0.0, 0.0, 2.0], 0.3, 0.3, Turn::Half).expect("frustum");
        // Ring vertex 0 starts at +x in the straight-down frame; a half turn about y sends it
        // to -x.
        assert!((c.pos[0][0] + 0.3).abs() < 1e-12 && c.pos[0][1].abs() < 1e-12);
        assert!((c.pos[16][2] - 2.0).abs() < 1e-12);
    }

    fn rod_with(coords: &[f32]) -> crate::parts::Part {
        let r = crate::testkit::rec(3, [0.0, 0.0, 0.0], [0.5, 0.2, -2.0], 0.3, 0.3);
        crate::testkit::part_with_mesh(0, &r, coords)
    }

    #[test]
    fn a_mesh_is_canonical_only_within_two_units_in_the_last_place_and_with_the_native_topology() {
        use crate::testkit as kit;
        let (s, e) = ([0.0f32, 0.0, 0.0], [0.5f32, 0.2, -2.0]);
        let good = kit::frustum_coords(s, e, 0.3, 0.3);
        assert!(rod_with(&good).is_valid());
        // One unit in the last place of the largest coordinate (2.0: 2.4e-7 mm) on a vertex
        // is within the tolerance; three are not.
        let unit = ulp32(2.0) as f32;
        // (Every copy of the vertex moves, so the mesh stays welded.)
        let nudge = |by: f32| {
            let mut m = good.clone();
            let v0 = [good[0], good[1], good[2]];
            for c in m.chunks_exact_mut(3) {
                if c == v0 {
                    c[0] += by;
                }
            }
            m
        };
        let near = nudge(unit);
        assert!(rod_with(&near).is_valid(), "1 ULP is inside");
        let far = nudge(3.0 * unit);
        assert!(rod_with(&far).invalid.is_some(), "3 ULP is outside");
        // The ring turned by half a step against the native phase is a regular 16-gon but not
        // the native mesh.
        let turned = kit::frustum_coords_turned(s, e, 0.3, 0.3, PI / 16.0);
        let why = rod_with(&turned).invalid.expect("a turned ring is refused");
        assert!(why.contains("native shape"), "{why}");
        // Two side triangles swapped: the same solid, a different topology.
        let mut swapped = good.clone();
        let (a, b) = (swapped[0..9].to_vec(), swapped[9..18].to_vec());
        swapped[0..9].copy_from_slice(&b);
        swapped[9..18].copy_from_slice(&a);
        assert!(
            rod_with(&swapped).invalid.is_some(),
            "a different order is refused"
        );
        // The mesh of a rod of another radius, and the mesh of a rod turned about an end.
        assert!(
            rod_with(&kit::frustum_coords(s, e, 0.3001, 0.3001))
                .invalid
                .is_some()
        );
        assert!(
            rod_with(&kit::frustum_coords(s, [0.5, 0.2001, -2.0], 0.3, 0.3))
                .invalid
                .is_some()
        );
    }

    #[test]
    fn the_unit_in_the_last_place_is_the_f32_spacing() {
        assert!((ulp32(1.0) - 2f64.powi(-23)).abs() < 1e-30);
        assert!((ulp32(3000.0) - 2f64.powi(-12)).abs() < 1e-30);
    }
}
