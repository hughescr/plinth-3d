//! Test support: the checker's own builders for synthetic support parts with the native
//! topology (a 242-vertex ball, a 32-vertex frustum, a 42-vertex raft pad, an 8-vertex raft
//! bridge). They exist so the checker's unit tests need nothing from the generator. The cap
//! triangulation is a plain fan, not ChituBox's table: the checker accepts any triangulation
//! of the right counts.

use crate::canon;
use crate::chitubox::{RECORD_MAGIC, SupportRecord};
use crate::limits::Limits;
use crate::parts::{self, Part};
use crate::v3::{V, add, cross, dot, mul, sub, unit};

/// The PRD limits, for tests.
#[must_use]
pub fn test_limits() -> Limits {
    Limits {
        h1_patch_area_mm2: 0.5,
        h4_lift_mm: 5.0,
        h6_gap_mm: 0.3,
        h9_thickness_mm: 0.1,
        region_length_mm: 3.0,
        drain_min_mm: 3.0,
        mouth_clearance_mm: 5.0,
        hollow_defaults_used: false,
    }
}

/// A closed box with outward-wound triangles, from `lo` to `hi`.
#[must_use]
pub fn closed_box(lo: V, hi: V) -> Vec<[V; 3]> {
    let p = |i: u8| -> V {
        [
            if i & 1 == 0 { lo[0] } else { hi[0] },
            if i & 2 == 0 { lo[1] } else { hi[1] },
            if i & 4 == 0 { lo[2] } else { hi[2] },
        ]
    };
    let quads = [
        [0u8, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    let mut out = Vec::new();
    for q in quads {
        out.push([p(q[0]), p(q[1]), p(q[2])]);
        out.push([p(q[0]), p(q[2]), p(q[3])]);
    }
    out
}

/// A record with the given fields and no mesh location.
#[must_use]
pub fn rec(kind: u32, s: [f32; 3], e: [f32; 3], r1: f32, r2: f32) -> SupportRecord {
    SupportRecord {
        magic: RECORD_MAGIC,
        kind,
        start: s,
        end: e,
        r1,
        r2,
        mesh_off: 0,
        mesh_len: 0,
        tail: [0; 6],
    }
}

fn p64(v: [f32; 3]) -> V {
    [f64::from(v[0]), f64::from(v[1]), f64::from(v[2])]
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "the file's precision is f32"
)]
fn out32(tris: &[[V; 3]]) -> Vec<f32> {
    // Flip all triangles if the solid is inside out.
    let vol: f64 = tris
        .iter()
        .map(|t| dot(t[0], cross(t[1], t[2])) / 6.0)
        .sum();
    let mut out = Vec::with_capacity(tris.len() * 9);
    for t in tris {
        let order = if vol < 0.0 { [0, 2, 1] } else { [0, 1, 2] };
        for i in order {
            out.extend(t[i].map(|x| x as f32));
        }
    }
    out
}

fn ring(c: V, e1: V, e2: V, r: f64, n: usize) -> Vec<V> {
    (0..n)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / n as f64;
            add(c, add(mul(e1, r * a.cos()), mul(e2, r * a.sin())))
        })
        .collect()
}

/// A 16-sided frustum mesh from `s` (radius `ra`) to `e` (radius `rb`), as f32 coordinates.
#[must_use]
pub fn frustum_coords(s: [f32; 3], e: [f32; 3], ra: f32, rb: f32) -> Vec<f32> {
    let (a, b) = (p64(s), p64(e));
    let turn = canon::turns(unit(sub(b, a)))[0];
    let c = canon::frustum(a, b, f64::from(ra), f64::from(rb), turn).expect("a frustum");
    out32(&canon::triangles(&c, true))
}

/// The same with the ring turned about the axis by `turn` radians against the canonical one
/// (a mesh that is closed and regular but not the native one), for tests of the refusal.
#[must_use]
pub fn frustum_coords_turned(s: [f32; 3], e: [f32; 3], ra: f32, rb: f32, turn: f64) -> Vec<f32> {
    let (a, b) = (p64(s), p64(e));
    let u = unit(sub(b, a));
    let c =
        canon::frustum(a, b, f64::from(ra), f64::from(rb), canon::turns(u)[0]).expect("a frustum");
    let rot = |p: V| -> V {
        // Rodrigues rotation of p about the axis through a along u.
        let x = sub(p, a);
        let (c0, s0) = (turn.cos(), turn.sin());
        add(
            a,
            add(
                add(mul(x, c0), mul(cross(u, x), s0)),
                mul(u, dot(u, x) * (1.0 - c0)),
            ),
        )
    };
    let tris: Vec<[V; 3]> = canon::triangles(&c, true)
        .into_iter()
        .map(|t| [rot(t[0]), rot(t[1]), rot(t[2])])
        .collect();
    out32(&tris)
}

/// A UV-sphere mesh (242 vertices, 480 triangles) whose poles are `s` and `e`.
#[must_use]
pub fn ball_coords(s: [f32; 3], e: [f32; 3], r: f32) -> Vec<f32> {
    let (ps, pe) = (p64(s), p64(e));
    let turn = canon::turns(unit(sub(pe, ps)))[0];
    let c = canon::ball(ps, pe, f64::from(r), turn).expect("a ball");
    out32(&canon::triangles(&c, true))
}

/// A 20-sided raft pad mesh (42 vertices, 80 triangles).
#[must_use]
pub fn pad_coords(centre: [f32; 2], z_top: f32, z_bottom: f32, radius: f32) -> Vec<f32> {
    let (c, zt, zb) = (
        [f64::from(centre[0]), f64::from(centre[1])],
        f64::from(z_top),
        f64::from(z_bottom),
    );
    let at = |z: f64| {
        ring(
            [c[0], c[1], z],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            f64::from(radius),
            20,
        )
    };
    let (top, bot) = (at(zt), at(zb));
    let (ct, cb) = ([c[0], c[1], zt], [c[0], c[1], zb]);
    let mut t = Vec::new();
    for i in 0..20 {
        let j = (i + 1) % 20;
        t.push([ct, top[i], top[j]]);
        t.push([cb, bot[j], bot[i]]);
        t.push([bot[i], bot[j], top[j]]);
        t.push([bot[i], top[j], top[i]]);
    }
    out32(&t)
}

/// A raft bridge mesh (8 vertices, 12 triangles).
#[must_use]
pub fn bridge_coords(
    from: [f32; 2],
    to: [f32; 2],
    z_top: f32,
    z_bottom: f32,
    half: f32,
) -> Vec<f32> {
    let (f0, t0) = (
        [f64::from(from[0]), f64::from(from[1])],
        [f64::from(to[0]), f64::from(to[1])],
    );
    let d = [t0[0] - f0[0], t0[1] - f0[1]];
    let l = d[0].hypot(d[1]);
    let n = [-d[1] / l * f64::from(half), d[0] / l * f64::from(half)];
    let p = |i: usize| -> V {
        let base = if i & 1 == 0 { f0 } else { t0 };
        let sg = if i & 2 == 0 { -1.0 } else { 1.0 };
        let z = if i & 4 == 0 {
            f64::from(z_bottom)
        } else {
            f64::from(z_top)
        };
        [base[0] + sg * n[0], base[1] + sg * n[1], z]
    };
    let quads = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    let mut t = Vec::new();
    for q in quads {
        t.push([p(q[0]), p(q[1]), p(q[2])]);
        t.push([p(q[0]), p(q[2]), p(q[3])]);
    }
    out32(&t)
}

/// A decoded part of record type `kind` with a native-shaped mesh built for its fields.
/// Kinds 1 to 5 get a frustum, 7 to 9 a ball. (Type 5's mesh radius at the start is `r2`.)
#[must_use]
pub fn part(ix: usize, kind: u32, s: [f32; 3], e: [f32; 3], r1: f32, r2: f32) -> Part {
    let coords = match kind {
        7..=9 => ball_coords(s, e, r1),
        // Type 5 is built from the record's end (radius r1) to its start (radius r2).
        5 => frustum_coords(e, s, r1, r2),
        _ => frustum_coords(s, e, r1, r2),
    };
    part_with_mesh(ix, &rec(kind, s, e, r1, r2), &coords)
}

/// Decode a record with the given mesh coordinates.
#[must_use]
pub fn part_with_mesh(ix: usize, r: &SupportRecord, coords: &[f32]) -> Part {
    let bytes: Vec<u8> = coords.iter().flat_map(|x| x.to_le_bytes()).collect();
    parts::decode(ix, r, &bytes)
}

/// A raft pad part at `centre` (top at `z_top`, 0.2 thick).
#[must_use]
pub fn raft_pad(ix: usize, centre: [f32; 2], z_top: f32) -> Part {
    let coords = pad_coords(centre, z_top, z_top - 0.2, 1.1);
    let r = rec(
        6,
        [centre[0], centre[1], 0.0],
        [centre[0], centre[1], 0.0],
        2.2,
        0.2,
    );
    part_with_mesh(ix, &r, &coords)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_builders_make_valid_native_topology() {
        let b = part(0, 7, [0.0, 0.0, -0.3], [0.0, 0.0, 0.3], 0.3, 0.3);
        assert!(b.is_valid(), "{:?}", b.invalid);
        let c = part(1, 1, [0.0, 0.0, 0.0], [0.5, 0.2, -2.0], 0.1, 0.4);
        assert!(c.is_valid(), "{:?}", c.invalid);
        let flare = part(2, 4, [0.0, 0.0, -2.0], [0.0, 0.0, -3.0], 0.25, 1.1);
        assert!(flare.is_valid(), "{:?}", flare.invalid);
        let pad5 = part(3, 5, [0.0, 0.0, -3.95], [0.0, 0.0, -4.75], 3.64, 5.0);
        assert!(pad5.is_valid(), "{:?}", pad5.invalid);
        let rp = raft_pad(4, [1.0, 2.0], -1.0);
        assert!(rp.is_valid(), "{:?}", rp.invalid);
        let r = rec(6, [0.0; 3], [3.0, 1.0, 0.0], 2.2, 0.2);
        let br = part_with_mesh(
            5,
            &r,
            &bridge_coords([0.0, 0.0], [3.0, 1.0], -1.0, -1.2, 1.1),
        );
        assert!(br.is_valid(), "{:?}", br.invalid);
    }

    #[test]
    fn a_wrong_shape_for_the_fields_is_invalid_not_guessed() {
        // A ball whose mesh has the radius 0.31 while the fields say 0.3.
        let r = rec(7, [0.0, 0.0, -0.3], [0.0, 0.0, 0.3], 0.3, 0.3);
        let bad = part_with_mesh(
            0,
            &r,
            &ball_coords([0.0, 0.0, -0.31], [0.0, 0.0, 0.31], 0.31),
        );
        assert!(bad.invalid.is_some());
        // A frustum with a missing triangle.
        let mut coords = frustum_coords([0.0; 3], [0.0, 0.0, -2.0], 0.3, 0.3);
        coords.truncate(coords.len() - 9);
        let r2 = rec(3, [0.0; 3], [0.0, 0.0, -2.0], 0.3, 0.3);
        assert!(part_with_mesh(1, &r2, &coords).invalid.is_some());
        // Swapped radii are caught: the mesh is wide where the fields say narrow.
        let r3 = rec(3, [0.0; 3], [0.0, 0.0, -2.0], 0.3, 0.6);
        let swapped = frustum_coords([0.0; 3], [0.0, 0.0, -2.0], 0.6, 0.3);
        assert!(part_with_mesh(2, &r3, &swapped).invalid.is_some());
        // An unknown record type.
        let r4 = rec(42, [0.0; 3], [0.0, 0.0, -2.0], 0.3, 0.3);
        assert!(
            part_with_mesh(
                3,
                &r4,
                &frustum_coords([0.0; 3], [0.0, 0.0, -2.0], 0.3, 0.3)
            )
            .invalid
            .is_some()
        );
    }
}
