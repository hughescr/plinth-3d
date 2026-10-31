//! The four solids ChituBox v1.4.0 uses for support parts (format spec sections 10.1 and
//! 11): a 16-sided frustum, a 242-vertex UV sphere, a 20-sided raft pad and a raft bridge
//! box. Vertex positions come from the construction below; the triangle order of the two
//! frustum end caps is a table read from the native files (ChituBox's own triangulator
//! made it, and no formula for it is known).

use super::math::{Quat, V3, add, div, length, sub, unit_ring};
use plinth_chitubox::Triangle;

/// Sides of a frustum.
pub const FRUSTUM_SIDES: usize = 16;
/// Triangles of a frustum: 32 side + 2 x 14 cap.
pub const FRUSTUM_TRIANGLES: usize = 60;
/// Rings of a sphere between the poles.
pub const SPHERE_RINGS: usize = 15;
/// Vertices per sphere ring.
pub const SPHERE_SIDES: usize = 16;
/// Triangles of a sphere: 2 x 16 pole fans + 14 bands x 32.
pub const SPHERE_TRIANGLES: usize = 480;
/// Sides of a raft pad.
pub const PAD_SIDES: usize = 20;
/// Triangles of a raft pad: 20 top + 20 bottom + 40 side.
pub const PAD_TRIANGLES: usize = 80;
/// Triangles of a raft bridge box.
pub const BRIDGE_TRIANGLES: usize = 12;

/// Start-cap triangles of the 16-gon, as indices into the start ring, in file order. Read
/// from record 37 of `03-round25-supports` and equal in every one of the 326 frustum
/// records of samples 03, 04, 05 and 07 (the tests check this on every record).
const START_CAP: [[usize; 3]; 14] = [
    [11, 12, 13],
    [14, 15, 0],
    [9, 10, 11],
    [2, 3, 4],
    [6, 7, 8],
    [4, 5, 6],
    [0, 1, 2],
    [13, 14, 0],
    [8, 9, 11],
    [4, 6, 8],
    [0, 2, 4],
    [11, 13, 0],
    [4, 8, 11],
    [11, 0, 4],
];

/// A closed frustum from the ring at `start` (radius `r_start`) to the ring at `end`
/// (radius `r_end`). Ring vertex `j` lies at `360 j / 16` degrees, counter-clockwise seen
/// from +z when the axis points straight down (the native frame); other axes rotate that
/// frame by the shortest arc from -z. The caller has checked the axis has length > 0.
#[must_use]
pub fn frustum(start: V3, end: V3, r_start: f32, r_end: f32) -> Vec<Triangle> {
    let d = sub(end, start);
    let q = Quat::rotation_to([0.0, 0.0, -1.0], div(d, length(d)));
    let ring = unit_ring(FRUSTUM_SIDES);
    let place = |base: V3, r: f32| -> Vec<V3> {
        ring.iter()
            .map(|&(c, s)| add(base, q.rotate([r * c, r * s, 0.0])))
            .collect()
    };
    let s = place(start, r_start);
    let e = place(end, r_end);
    let mut out = Vec::with_capacity(FRUSTUM_TRIANGLES);
    for j in 0..FRUSTUM_SIDES {
        let k = (j + 1) % FRUSTUM_SIDES;
        out.push([s[j], e[j], e[k]]);
        out.push([s[j], e[k], s[k]]);
    }
    for t in START_CAP {
        out.push([s[t[0]], s[t[1]], s[t[2]]]);
    }
    // The end cap is the start cap seen in a mirror: index i becomes (16 - i) mod 16.
    let mirror = |i: usize| (FRUSTUM_SIDES - i) % FRUSTUM_SIDES;
    for t in START_CAP {
        out.push([e[mirror(t[0])], e[mirror(t[1])], e[mirror(t[2])]]);
    }
    out
}

/// A closed UV sphere whose poles are exactly `start` and `end` and whose radius is
/// `radius`. Ring `k` (1 to 15) lies at `k * 11.25` degrees from the start pole; vertex
/// `j` of a ring at `22.5 j` degrees. The frame is the same as the frustum's.
#[must_use]
pub fn sphere(start: V3, end: V3, radius: f32) -> Vec<Triangle> {
    let d = sub(end, start);
    let q = Quat::rotation_to([0.0, 0.0, -1.0], div(d, length(d)));
    let centre = [
        (start[0] + end[0]) / 2.0,
        (start[1] + end[1]) / 2.0,
        (start[2] + end[2]) / 2.0,
    ];
    let around = unit_ring(SPHERE_SIDES);
    let polar = unit_ring(2 * (SPHERE_RINGS + 1));
    // rings[k][j] for k in 0..15 (polar angle (k + 1) * 11.25 degrees).
    let rings: Vec<Vec<V3>> = (1..=SPHERE_RINGS)
        .map(|k| {
            let (cos_t, sin_t) = polar[k];
            around
                .iter()
                .map(|&(c, s)| {
                    let rs = radius * sin_t;
                    add(centre, q.rotate([rs * c, rs * s, radius * cos_t]))
                })
                .collect()
        })
        .collect();
    let mut out = Vec::with_capacity(SPHERE_TRIANGLES);
    for j in 0..SPHERE_SIDES {
        let k = (j + 1) % SPHERE_SIDES;
        out.push([start, rings[0][j], rings[0][k]]);
    }
    for band in rings.windows(2) {
        for j in 0..SPHERE_SIDES {
            let k = (j + 1) % SPHERE_SIDES;
            out.push([band[0][j], band[1][j], band[1][k]]);
            out.push([band[0][j], band[1][k], band[0][k]]);
        }
    }
    let last = &rings[SPHERE_RINGS - 1];
    for j in 0..SPHERE_SIDES {
        let k = (j + 1) % SPHERE_SIDES;
        out.push([last[j], end, last[k]]);
    }
    out
}

/// A raft pad: a 20-sided prism centred on `centre`, top at `z_top`, bottom at
/// `z_bottom`, rim radius `radius`. Rim vertex `i` lies at `18 i` degrees from +x. The top
/// is a fan from its centre, the bottom a fan from its centre with reversed order.
#[must_use]
pub fn pad(centre: [f32; 2], z_top: f32, z_bottom: f32, radius: f32) -> Vec<Triangle> {
    let ring = unit_ring(PAD_SIDES);
    let at = |z: f32| -> Vec<V3> {
        ring.iter()
            .map(|&(c, s)| [centre[0] + radius * c, centre[1] + radius * s, z])
            .collect()
    };
    let top = at(z_top);
    let bottom = at(z_bottom);
    let ct = [centre[0], centre[1], z_top];
    let cb = [centre[0], centre[1], z_bottom];
    let mut out = Vec::with_capacity(PAD_TRIANGLES);
    for i in 0..PAD_SIDES {
        out.push([ct, top[i], top[(i + 1) % PAD_SIDES]]);
    }
    for i in 0..PAD_SIDES {
        let a = (2 * PAD_SIDES - 1 - i) % PAD_SIDES;
        let b = (2 * PAD_SIDES - 2 - i) % PAD_SIDES;
        out.push([cb, bottom[a], bottom[b]]);
    }
    for j in 0..PAD_SIDES {
        let k = (j + 1) % PAD_SIDES;
        out.push([bottom[j], top[k], top[j]]);
        out.push([top[k], bottom[j], bottom[k]]);
    }
    out
}

/// A raft bridge: a box between the two points `from` and `to` (flat ends, nothing
/// beyond them), `half_width` to each side, top at `z_top`, bottom at `z_bottom`. The
/// caller has checked the two points differ.
#[must_use]
pub fn bridge(
    from: [f32; 2],
    to: [f32; 2],
    z_top: f32,
    z_bottom: f32,
    half_width: f32,
) -> Vec<Triangle> {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let l = (dx * dx + dy * dy).sqrt();
    let n = [-dy / l * half_width, dx / l * half_width];
    // Corners: A = to + n, B = from + n, C = from - n, D = to - n.
    let corner = |p: [f32; 2], sign: f32, z: f32| [p[0] + sign * n[0], p[1] + sign * n[1], z];
    let (at, bt, ct, dt) = (
        corner(to, 1.0, z_top),
        corner(from, 1.0, z_top),
        corner(from, -1.0, z_top),
        corner(to, -1.0, z_top),
    );
    let (ab, bb, cb, db) = (
        corner(to, 1.0, z_bottom),
        corner(from, 1.0, z_bottom),
        corner(from, -1.0, z_bottom),
        corner(to, -1.0, z_bottom),
    );
    vec![
        [at, bt, ct],
        [at, ct, dt],
        [bb, ab, db],
        [bb, db, cb],
        [at, ab, bb],
        [at, bb, bt],
        [ct, cb, db],
        [ct, db, dt],
        [dt, db, ab],
        [dt, ab, at],
        [bt, bb, cb],
        [bt, cb, ct],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area_vector_sum(tris: &[Triangle]) -> V3 {
        let mut s = [0f64; 3];
        for t in tris {
            let a = sub(t[1], t[0]);
            let b = sub(t[2], t[0]);
            let c = super::super::math::cross(a, b);
            for k in 0..3 {
                s[k] += f64::from(c[k]) / 2.0;
            }
        }
        [s[0] as f32, s[1] as f32, s[2] as f32]
    }

    #[test]
    fn counts_match_the_native_files() {
        assert_eq!(
            frustum([0.0; 3], [0.0, 0.0, -2.0], 1.0, 0.5).len(),
            FRUSTUM_TRIANGLES
        );
        assert_eq!(
            sphere([0.0; 3], [0.0, 0.0, -2.0], 1.0).len(),
            SPHERE_TRIANGLES
        );
        assert_eq!(pad([0.0; 2], 0.0, -0.2, 1.1).len(), PAD_TRIANGLES);
        assert_eq!(bridge([0.0; 2], [3.0, 0.0], 0.0, -0.2, 1.1).len(), 12);
    }

    #[test]
    fn closed_solids_have_zero_area_vector_sum() {
        for tris in [
            frustum([0.0; 3], [1.0, 0.5, -2.0], 1.0, 0.5),
            sphere([0.0; 3], [1.0, 0.5, -2.0], 1.2),
            pad([1.0, 2.0], 0.0, -0.2, 1.1),
            bridge([0.0; 2], [3.0, 1.0], 0.0, -0.2, 1.1),
        ] {
            let s = area_vector_sum(&tris);
            assert!(s.iter().all(|x| x.abs() < 1e-4), "{s:?}");
        }
    }

    #[test]
    fn sphere_poles_are_the_given_points() {
        let t = sphere([1.0, 2.0, 3.0], [1.0, 2.0, 1.0], 1.0);
        assert_eq!(
            t[0][0].map(f32::to_bits),
            [1.0f32, 2.0, 3.0].map(f32::to_bits)
        );
        assert_eq!(
            t[SPHERE_TRIANGLES - 1][1].map(f32::to_bits),
            [1.0f32, 2.0, 1.0].map(f32::to_bits)
        );
    }
}
