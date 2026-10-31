//! Local-admission witnesses. The raster oracle is test-only and does not call
//! patch, clearance, BVH or the production departure certificate.
use super::Reject;
use super::native::{cross, dot, norm, sub};
use super::tests::{builder, candidate};
use plinth_geom::Mesh;

fn triangle(mesh: &Mesh, i: usize) -> [[f64; 3]; 3] {
    std::array::from_fn(|k| {
        let j = mesh.indices()[3 * i + k] as usize;
        [mesh.xs()[j], mesh.ys()[j], mesh.zs()[j]]
    })
}

// Every uncertain square counts toward U, never L. Convexity certifies inside
// squares; separating triangle halfplanes and circle/box distance certify outside
// squares. Thus the unresolved-cell area, not agreement with a finer grid, bounds
// the oracle error. The independent square grid has spacing <= 0.0005 mm.
fn oracle_patch(mesh: &Mesh, centre: [f64; 3], radius: f64) -> (f64, f64) {
    let (mut lo, mut hi) = (0.0, 0.0);
    for i in 0..mesh.triangle_count() {
        let t = triangle(mesh, i);
        let e = sub(t[1], t[0]);
        let e = e.map(|x| x / norm(e));
        let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
        let n = n.map(|x| x / norm(n));
        let height = dot(sub(centre, t[0]), n);
        if height.abs() >= radius + 1e-10 {
            continue;
        }
        let f = cross(n, e);
        let p: [[f64; 2]; 3] = t.map(|p| {
            let v = sub(p, centre);
            [dot(v, e), dot(v, f)]
        });
        let r = (radius * radius - height * height).max(0.0).sqrt();
        let min: [f64; 2] =
            std::array::from_fn(|k| p.iter().map(|p| p[k]).fold(f64::INFINITY, f64::min).max(-r));
        let max: [f64; 2] = std::array::from_fn(|k| {
            p.iter()
                .map(|p| p[k])
                .fold(f64::NEG_INFINITY, f64::max)
                .min(r)
        });
        if min[0] >= max[0] || min[1] >= max[1] {
            continue;
        }
        let counts: [usize; 2] =
            std::array::from_fn(|k| ((max[k] - min[k]) / 0.0005).ceil() as usize);
        let h: [f64; 2] = std::array::from_fn(|k| (max[k] - min[k]) / counts[k] as f64);
        let area = h[0] * h[1];
        for ix in 0..counts[0] {
            for iy in 0..counts[1] {
                let a = [min[0] + ix as f64 * h[0], min[1] + iy as f64 * h[1]];
                let b = [a[0] + h[0], a[1] + h[1]];
                let nearest: [f64; 2] = std::array::from_fn(|k| a[k].max(-b[k]).max(0.0));
                if nearest[0].powi(2) + nearest[1].powi(2) > r * r + 1e-12 {
                    continue;
                }
                let corners = [a, [a[0], b[1]], b, [b[0], a[1]]];
                let mut inside = corners
                    .iter()
                    .all(|p| p[0] * p[0] + p[1] * p[1] < r * r - 1e-12);
                let mut outside = false;
                for edge in 0..3 {
                    let u = p[edge];
                    let v = p[(edge + 1) % 3];
                    let side =
                        |p: [f64; 2]| (v[0] - u[0]) * (p[1] - u[1]) - (v[1] - u[1]) * (p[0] - u[0]);
                    let sides = corners.map(side);
                    outside |= sides.iter().all(|&s| s < -1e-10);
                    inside &= sides.iter().all(|&s| s > 1e-10);
                }
                if outside {
                    continue;
                }
                hi += area;
                if inside {
                    lo += area;
                }
            }
        }
    }
    // Covers projected-coordinate arithmetic and area summation for these fixtures.
    (lo - 1e-7, hi + 1e-7)
}

fn verify(mesh: &Mesh, site: [f32; 3], axis: [f32; 3]) {
    let p = builder(mesh);
    let mut c = candidate(0, 0.0);
    c.site = site;
    c.axis = axis;
    let mut out = Vec::new();
    p.probe(&[c], &mut out);
    let q = out.remove(0).unwrap_or_else(|e| {
        let ball = plinth_geom::patch::Ball {
            centre: std::array::from_fn(|k| f64::from(site[k]) + f64::from(axis[k] * c.standoff)),
            radius: f64::from(c.ball_r) + 1e-5,
        };
        panic!(
            "local admission site={site:?}, axis={axis:?}: {e:?}; diagnostic patch={:?}",
            plinth_geom::patch::ball_patch_reference(p.model.mesh(), ball)
        )
    });
    let contact = &q.contact;
    let oracle = oracle_patch(
        p.model.mesh(),
        contact.centre.map(f64::from),
        f64::from(contact.radius) + 1e-5,
    );
    assert!(
        oracle.0 > 0.0 && oracle.1 <= 0.48 && oracle.1 - oracle.0 < 0.010,
        "{oracle:?}"
    );
    assert!(contact.patch().upper() >= oracle.0 && contact.patch().lower() <= oracle.1);
    eprintln!(
        "local oracle area={oracle:?} mm2; certificate=[{},{}] mm2",
        contact.patch().lower(),
        contact.patch().upper()
    );
}

#[test]
fn capped_departure_rejects_a_clear_cone_that_returns_below_cap() {
    for x in [0.35, 0.7] {
        let mesh = Mesh::new(
            vec![-30., 30., 30., -30., x, x, x],
            vec![-30., -30., 30., 30., -0.05, 0.05, 0.0],
            vec![0., 0., 0., 0., -1.3, -1.3, -1.1],
            vec![0, 2, 1, 0, 3, 2, 4, 5, 6],
        )
        .unwrap();
        let p = builder(&mesh);
        let mut tip = candidate(0, 0.0);
        tip.site[2] = 0.65;
        let mut out = Vec::new();
        p.probe(&[tip], &mut out);
        if x < 0.5 {
            // Ball touches only its main face; cone keeps >0.02 mm; joint >0.32.
            // The extra triangle nevertheless draws the gap back below the cap
            // after the cone has left the first face. The departure guard must fire.
            assert!(
                matches!(out[0], Err(Reject::DepartureNotMonotone)),
                "{:?}",
                out[0].as_ref().err()
            );
        } else {
            assert!(out[0].is_ok(), "{:?}", out[0].as_ref().err());
        }
    }
}

#[test]
fn ridge_and_v_groove_admit_with_independent_patch_oracles() {
    for slope in [-0.25, 0.25] {
        let mesh = Mesh::new(
            vec![-3., -3., 0., 0., 3., 3.],
            vec![-3., 3., -3., 3., -3., 3.],
            vec![3. * slope, 3. * slope, 0., 0., 3. * slope, 3. * slope],
            vec![0, 1, 2, 1, 3, 2, 2, 3, 4, 3, 5, 4],
        )
        .unwrap();
        // The ball crosses the ridge seam, not just one flat triangle. The axis
        // is vertical and differs from the selected facet normal by ~14 degrees.
        let centred_z = 0.05 * slope - 1.5 * slope;
        verify(&mesh, [0.05, 0.0, centred_z as f32], [0., 0., -1.]);
    }
}

#[test]
fn cylinder_and_sphere_admit_with_independent_patch_oracles() {
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let mut zs = Vec::new();
    let mut indices = Vec::new();
    for i in 0..64 {
        let theta = i as f64 * std::f64::consts::TAU / 64.0;
        for y in [-2., 2.] {
            xs.push(0.5 * theta.cos());
            ys.push(y);
            zs.push(0.5 * theta.sin());
        }
    }
    for i in 0..64 {
        let a = 2 * i;
        let b = 2 * ((i + 1) % 64);
        indices.extend([a, a + 1, b, b, a + 1, b + 1]);
    }
    let cylinder = Mesh::new(xs, ys, zs, indices).unwrap();
    let t = triangle(&cylinder, 96);
    let site: [f32; 3] = std::array::from_fn(|k| ((t[0][k] + t[1][k] + t[2][k]) / 3.0) as f32);
    verify(&cylinder, site, [0., 0., -1.]);
    let tris = plinth_geom::shapes::sphere([0., 0., 0.5], [0., 0., -0.5], 0.5);
    let mut axes: [Vec<f64>; 3] = std::array::from_fn(|_| Vec::new());
    for v in tris.iter().flatten() {
        for k in 0..3 {
            axes[k].push(f64::from(v[k]));
        }
    }
    let sphere = Mesh::from_soup(&axes[0], &axes[1], &axes[2]).unwrap();
    let tri = (0..sphere.triangle_count())
        .map(|i| triangle(&sphere, i))
        .find(|t| t.iter().all(|v| v[2] < -0.4))
        .unwrap();
    let site: [f32; 3] =
        std::array::from_fn(|k| ((tri[0][k] + tri[1][k] + tri[2][k]) / 3.0) as f32);
    let n = cross(sub(tri[1], tri[0]), sub(tri[2], tri[0]));
    let axis = n.map(|x| (x / norm(n)) as f32);
    verify(&sphere, site, axis);
}

#[test]
fn native_round25_recess_admits_with_an_independent_patch_oracle() {
    let bytes = include_bytes!("../../../../fixtures/round/approved/Round 25.keepup.stl");
    let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
    let mut axes: [Vec<f64>; 3] = std::array::from_fn(|_| Vec::with_capacity(3 * count));
    for t in bytes[84..].chunks_exact(50) {
        for k in 0..3 {
            for (axis, values) in axes.iter_mut().enumerate() {
                let at = 12 + k * 12 + axis * 4;
                values.push(f64::from(f32::from_le_bytes(
                    t[at..at + 4].try_into().unwrap(),
                )));
            }
        }
    }
    let mesh = Mesh::from_soup(&axes[0], &axes[1], &axes[2]).unwrap();
    assert_eq!(mesh.triangle_count(), 2996);
    let p = builder(&mesh);
    let mut candidates = Vec::new();
    for i in 0..p.model.mesh().triangle_count() {
        let t = triangle(p.model.mesh(), i);
        let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
        let area = norm(n) * 0.5;
        let n = n.map(|v| v / norm(n));
        let site: [f32; 3] = std::array::from_fn(|k| ((t[0][k] + t[1][k] + t[2][k]) / 3.0) as f32);
        // Recess face: tilted underside plane, inside both the lip and the outer
        // radius, outside the central rib; not the partly visible outer wall.
        let resting_radius = (f64::from(site[0]).powi(2)
            + (f64::from(site[1]) * 0.422618 - f64::from(site[2]) * 0.906308).powi(2))
        .sqrt();
        if n[1] < -0.89
            && n[2] < -0.40
            && n[2] > -0.45
            && area > 0.1
            && (5.0..10.0).contains(&resting_radius)
        {
            let mut c = candidate(candidates.len() as u32, 0.0);
            c.site = site;
            c.axis = n.map(|v| v as f32);
            candidates.push(c);
        }
    }
    assert!(!candidates.is_empty());
    // Independent probes avoid head-reservation conflicts hiding geometric admission.
    let mut admitted = None;
    for c in candidates {
        let mut out = Vec::new();
        p.probe(&[c], &mut out);
        if let Ok(q) = out.remove(0) {
            admitted = Some(q);
            break;
        }
    }
    let q = admitted.expect("no certified real underside-recess contact");
    let oracle = oracle_patch(
        p.model.mesh(),
        q.contact.centre.map(f64::from),
        f64::from(q.contact.radius) + 1e-5,
    );
    assert!(
        oracle.0 > 0.0 && oracle.1 <= 0.48 && oracle.1 - oracle.0 < 0.010,
        "{oracle:?}"
    );
    assert!(q.contact.patch().upper() >= oracle.0 && q.contact.patch().lower() <= oracle.1);
    eprintln!(
        "real Round25 recess centre={:?}; oracle={oracle:?} mm2; G={} mm2",
        q.contact.centre,
        q.contact.patch().upper()
    );
}
