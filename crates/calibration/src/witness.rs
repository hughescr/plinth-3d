//! One connected exterior: six radial envelopes welded into a tiled raft.
//! No overlapping shells or hidden caps. Dimensions are plate-relative until centred.
use crate::spec::{Head, Specimen};
use plinth_chitubox::Triangle;
use plinth_geom::shapes::math::unit_ring;

pub const TOP: f32 = 5.13;
const N: usize = 16;

fn radius(h: &Head, d: f32, z: f32) -> f32 {
    let c = 5.0 - h.outside;
    let j = c - h.waist_length;
    let k = j - h.cone_length;
    let sphere = |centre: f32, r: f32| (r * r - (z - centre).powi(2)).max(0.0).sqrt();
    let mut r = sphere(c, h.ball_diameter / 2.0)
        .max(sphere(j, h.junction_diameter / 2.0))
        .max(sphere(k, 0.52));
    if (j..=c).contains(&z) {
        r = r.max(d / 2.0);
    }
    if (k..=j).contains(&z) {
        r = r.max(
            (h.cone_end_diameter
                + (h.cone_start_diameter - h.cone_end_diameter) * (z - k) / (j - k))
                / 2.0,
        );
    }
    if (1.198..=k).contains(&z) {
        r = r.max(h.column_diameter / 2.0);
    }
    if (0.2..=1.2).contains(&z) {
        r = r.max(1.1 + (h.column_diameter / 2.0 - 1.1) * (z - 0.2));
    }
    r
}
fn join(out: &mut Vec<Triangle>, a: &[[f32; 3]], b: &[[f32; 3]]) {
    for i in 0..N {
        let j = (i + 1) % N;
        out.extend([[a[i], a[j], b[j]], [a[i], b[j], b[i]]]);
    }
}

#[must_use]
pub fn mesh(s: &Specimen, h: &Head) -> Vec<Triangle> {
    let ring = unit_ring(N);
    let mut out = vec![];
    for (index, (&xy, &d)) in s.contact_sites.iter().zip(&s.waist_diameters).enumerate() {
        // All tile boundaries have the same subdivision, including exact corners.
        let outer: Vec<_> = (0..N)
            .map(|i| {
                let side = i / 4;
                let ix = i % 4;
                let xs = [-1.25, -0.625, 0.0, 0.625, 1.25];
                let ys = [-1.2, -0.6, 0.0, 0.6, 1.2];
                let (x, y) = match side {
                    0 => (1.25, ys[ix]),
                    1 => (xs[4 - ix], 1.2),
                    2 => (-1.25, ys[4 - ix]),
                    _ => (xs[ix], -1.2),
                };
                [xy[0] + x, xy[1] + y, 0.2]
            })
            .collect();
        // Ring angles match the perimeter's CCW order, starting at lower right.
        let circle = |r: f32, z: f32| -> Vec<_> {
            ring.iter()
                .map(|&(x, y)| {
                    let q = std::f32::consts::FRAC_1_SQRT_2;
                    [xy[0] + r * (x + y) * q, xy[1] + r * (y - x) * q, z]
                })
                .collect()
        };
        let mut prev = circle(1.1, 0.2);
        // Top annulus faces up (outer-to-inner is the reverse of a wall).
        join(&mut out, &outer, &prev);
        let bottom: Vec<_> = outer.iter().map(|p| [p[0], p[1], 0.0]).collect();
        for i in 0..N {
            let j = (i + 1) % N;
            out.push([[xy[0], xy[1], 0.0], bottom[j], bottom[i]]);
            // Omit shared internal tile walls, welding all six columns to one raft.
            if (i / 4 != 0 || index == 5) && (i / 4 != 2 || index == 0) {
                out.extend([
                    [bottom[i], bottom[j], outer[j]],
                    [bottom[i], outer[j], outer[i]],
                ]);
            }
        }
        // Dense radial envelope of the ball/rod/cone union, including exact boundaries.
        let mut zs: Vec<_> = (21..513).map(|i| i as f32 / 100.0).collect();
        zs.extend([1.198, 1.2, 2.67, 4.17, 4.378, 4.4, 4.6, 4.63, 4.88]);
        zs.sort_by(f32::total_cmp);
        zs.dedup();
        for z in zs {
            let next = circle(radius(h, d, z), z);
            join(&mut out, &prev, &next);
            prev = next;
        }
        for i in 0..N {
            out.push([prev[i], prev[(i + 1) % N], [xy[0], xy[1], TOP]]);
        }
    }
    for p in out.iter_mut().flatten() {
        p[2] -= TOP / 2.0;
    }
    out
}

/// Select one load path from the written mesh, never from the radius fields.
/// The common raft is measured separately; the free ball crown is not a loaded neck.
#[must_use]
pub fn column(mesh: &[Triangle], xy: [f32; 2]) -> Vec<Triangle> {
    mesh.iter()
        .filter(|t| {
            t.iter()
                .all(|p| (p[0] - xy[0]).abs() <= 1.251 && (p[1] - xy[1]).abs() <= 1.201)
        })
        .copied()
        .collect()
}
