//! Oracle for `geom_kernel::nearest`: the bounding-box tree must answer exactly
//! what the scan of every face answers, bit for bit, ties included. The scan is
//! also re-derived here from the public `closest_on_tri` so the module's own
//! brute-force function is not the only witness.

use geom_kernel::nearest::{
    nearest_triangles, nearest_triangles_brute, nearest_triangles_bvh, Tri, BVH_MIN_POINTS,
};
use geom_kernel::{closest_on_tri, vec3, V3};

/// Deterministic generator; no dependency, no platform variation.
struct Lcg(u64);

impl Lcg {
    fn next_unit(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // 53 high bits to [0, 1).
        let bits = self.0 >> 11;
        // The value is below 2^53, so the conversion is exact.
        #[allow(clippy::cast_precision_loss)]
        let value = bits as f64;
        value / 9_007_199_254_740_992.0
    }

    fn point(&mut self, lo: f64, hi: f64) -> V3 {
        [
            lo + (hi - lo) * self.next_unit(),
            lo + (hi - lo) * self.next_unit(),
            lo + (hi - lo) * self.next_unit(),
        ]
    }
}

/// The scan, spelled out in the test: ascending ids, strict `<` on the squared
/// length of `p - closest_on_tri`.
fn scan(triangles: &[Tri], p: V3) -> Option<(usize, u64)> {
    let mut best = f64::INFINITY;
    let mut found = None;
    for (id, t) in triangles.iter().enumerate() {
        let closest = closest_on_tri(p, t[0], t[1], t[2]);
        let offset = vec3::sub(p, closest);
        let d2 = vec3::dot(offset, offset);
        if d2 < best {
            best = d2;
            found = Some(id);
        }
    }
    found.map(|id| (id, best.sqrt().to_bits()))
}

/// How many faces share the winning squared distance (>1 is an exact tie).
#[allow(clippy::float_cmp, reason = "counting EXACT ties is the point")]
fn tie_width(triangles: &[Tri], p: V3) -> usize {
    let d2s: Vec<f64> = triangles
        .iter()
        .map(|t| {
            let offset = vec3::sub(p, closest_on_tri(p, t[0], t[1], t[2]));
            vec3::dot(offset, offset)
        })
        .collect();
    let best = d2s.iter().copied().fold(f64::INFINITY, f64::min);
    d2s.iter().filter(|&&d| d == best).count()
}

fn bits(hits: &[Option<(usize, f64)>]) -> Vec<Option<(usize, u64)>> {
    hits.iter()
        .map(|hit| hit.map(|(id, d)| (id, d.to_bits())))
        .collect()
}

/// Brute (module), tree, dispatcher and the test's own scan must agree on every
/// point. Returns the number of points that had an exact tie.
fn assert_all_agree(triangles: &[Tri], points: &[V3]) -> (usize, u64, u64) {
    let own: Vec<Option<(usize, u64)>> = points.iter().map(|&p| scan(triangles, p)).collect();
    let brute = nearest_triangles_brute(triangles, points);
    let tree = nearest_triangles_bvh(triangles, points);
    let auto = nearest_triangles(triangles, points);
    assert_eq!(bits(&brute.hits), own, "module brute != test scan");
    assert_eq!(bits(&tree.hits), own, "tree != scan");
    assert_eq!(bits(&auto.hits), own, "dispatcher != scan");
    let ties = points
        .iter()
        .filter(|&&p| tie_width(triangles, p) > 1)
        .count();
    (ties, brute.tris_tested, tree.tris_tested)
}

/// Two unit-lattice sheets, each quad split into two triangles, with a third
/// sheet that duplicates the first exactly: equal faces at distinct ids.
fn lattice_with_duplicates(n: u32) -> Vec<Tri> {
    let mut tris = Vec::new();
    for z in [0.0, 3.0] {
        for i in 0..n {
            for j in 0..n {
                let (x, y) = (f64::from(i), f64::from(j));
                tris.push([[x, y, z], [x + 1.0, y, z], [x, y + 1.0, z]]);
                tris.push([[x + 1.0, y, z], [x + 1.0, y + 1.0, z], [x, y + 1.0, z]]);
            }
        }
    }
    let first_sheet: Vec<Tri> = tris[..(2 * n * n) as usize].to_vec();
    tris.extend(first_sheet);
    tris
}

fn lattice_points(n: u32) -> Vec<V3> {
    let mut points = Vec::new();
    let top = f64::from(n);
    for i in 0..=n {
        for j in 0..=n {
            let (x, y) = (f64::from(i), f64::from(j));
            // On a lattice vertex, over a vertex, over an edge midpoint, over
            // a quad centre, between the sheets (equidistant from both), and
            // below the lot. All coordinates are exact binary fractions.
            points.push([x, y, 0.0]);
            points.push([x, y, 1.0]);
            points.push([x + 0.5, y, 1.5]);
            points.push([x + 0.5, y + 0.5, 1.5]);
            points.push([x, y, -2.0]);
            points.push([x * 0.5, y * 0.5, top]);
        }
    }
    points
}

/// An octahedron refined `levels` times and projected to the sphere, plus the
/// octahedron's own symmetry points.
fn sphere(levels: u32) -> Vec<Tri> {
    let mut tris: Vec<Tri> = vec![
        [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
        [[0., 1., 0.], [-1., 0., 0.], [0., 0., 1.]],
        [[-1., 0., 0.], [0., -1., 0.], [0., 0., 1.]],
        [[0., -1., 0.], [1., 0., 0.], [0., 0., 1.]],
        [[0., 1., 0.], [1., 0., 0.], [0., 0., -1.]],
        [[-1., 0., 0.], [0., 1., 0.], [0., 0., -1.]],
        [[0., -1., 0.], [-1., 0., 0.], [0., 0., -1.]],
        [[1., 0., 0.], [0., -1., 0.], [0., 0., -1.]],
    ];
    let unit = |v: V3| {
        let n = vec3::norm(v);
        [v[0] / n, v[1] / n, v[2] / n]
    };
    let mid = |a: V3, b: V3| unit([a[0] + b[0], a[1] + b[1], a[2] + b[2]]);
    for _ in 0..levels {
        let mut next = Vec::with_capacity(tris.len() * 4);
        for [a, b, c] in tris {
            let (ab, bc, ca) = (mid(a, b), mid(b, c), mid(c, a));
            next.push([a, ab, ca]);
            next.push([ab, b, bc]);
            next.push([ca, bc, c]);
            next.push([ab, bc, ca]);
        }
        tris = next;
    }
    // Scale and shift so the coordinates are not unit-sized.
    tris.iter()
        .map(|t| t.map(|v| [20.0 * v[0] + 100.0, 20.0 * v[1] - 50.0, 20.0 * v[2] + 7.0]))
        .collect()
}

#[test]
fn lattice_with_exact_ties_matches_the_scan() {
    let n = 14;
    let triangles = lattice_with_duplicates(n);
    let points = lattice_points(n);
    let (ties, brute_tested, tree_tested) = assert_all_agree(&triangles, &points);
    // Every point over a vertex, edge or centre ties between faces, and every
    // point near the first sheet ties with its duplicate.
    assert!(ties > points.len() / 2, "only {ties} of {} tied", points.len());
    assert!(tree_tested < brute_tested);
}

#[test]
fn random_soup_with_slivers_matches_the_scan() {
    let mut rng = Lcg(0x5EED_0001);
    let mut triangles: Vec<Tri> = Vec::new();
    for _ in 0..3000 {
        let centre = rng.point(-60.0, 60.0);
        let size = 0.2 + 6.0 * rng.next_unit();
        let jitter = |rng: &mut Lcg| {
            let o = rng.point(-size, size);
            [centre[0] + o[0], centre[1] + o[1], centre[2] + o[2]]
        };
        triangles.push([jitter(&mut rng), jitter(&mut rng), jitter(&mut rng)]);
    }
    // Slivers, a zero-area triangle, a point-triangle and a repeated-vertex one.
    let mut sliver_points: Vec<V3> = Vec::new();
    for _ in 0..600 {
        let a = rng.point(-60.0, 60.0);
        let d = rng.point(-8.0, 8.0);
        // Width from 1e-10 to 1e-3 of the length: sin of the apex angle.
        let squeeze = 10_f64.powf(-10.0 + 7.0 * rng.next_unit()) * vec3::norm(d);
        let along = rng.next_unit();
        let off = rng.point(-1.0, 1.0);
        sliver_points.push([
            a[0] + along * d[0] + off[0],
            a[1] + along * d[1] + off[1],
            a[2] + along * d[2] + off[2],
        ]);
        triangles.push([
            a,
            [a[0] + d[0], a[1] + d[1], a[2] + d[2]],
            [
                a[0] + 0.5 * d[0] + squeeze,
                a[1] + 0.5 * d[1],
                a[2] + 0.5 * d[2],
            ],
        ]);
    }
    triangles.push([[1., 2., 3.], [1., 2., 3.], [1., 2., 3.]]);
    triangles.push([[0., 0., 0.], [4., 0., 0.], [4., 0., 0.]]);
    let mut points: Vec<V3> = (0..1500).map(|_| rng.point(-70.0, 70.0)).collect();
    points.extend(sliver_points);
    // On vertices (squared distance zero, shared by neighbours, ties across ids).
    for t in triangles.iter().step_by(7) {
        points.push(t[0]);
        points.push([t[1][0], t[1][1], t[1][2] + 1.0]);
    }
    let (_, brute_tested, tree_tested) = assert_all_agree(&triangles, &points);
    assert!(tree_tested * 5 < brute_tested);
}

#[test]
fn refined_sphere_matches_the_scan_and_tests_far_fewer_faces() {
    let triangles = sphere(5);
    assert_eq!(triangles.len(), 8 * 4usize.pow(5));
    let mut rng = Lcg(0x5EED_0002);
    let centre = [100.0, -50.0, 7.0];
    let mut points: Vec<V3> = vec![centre];
    // The octahedron's symmetry axes, inside, on and outside the sphere.
    for r in [5.0, 20.0, 35.0] {
        for axis in 0..3 {
            for sign in [-1.0, 1.0] {
                let mut p = centre;
                p[axis] += sign * r;
                points.push(p);
            }
        }
    }
    for _ in 0..800 {
        let p = rng.point(-1.0, 1.0);
        let r = 12.0 + 16.0 * rng.next_unit();
        let n = vec3::norm(p);
        points.push([
            centre[0] + r * p[0] / n,
            centre[1] + r * p[1] / n,
            centre[2] + r * p[2] / n,
        ]);
    }
    for t in triangles.iter().step_by(97) {
        points.push(t[2]);
    }
    let (_, brute_tested, tree_tested) = assert_all_agree(&triangles, &points);
    eprintln!("sphere: tree tested {tree_tested} of {brute_tested} pairs");
    assert!(
        tree_tested * 50 <= brute_tested,
        "tree tested {tree_tested} of {brute_tested}: under 50x"
    );
}

#[test]
fn non_finite_input_takes_the_scan_and_agrees() {
    let mut triangles = lattice_with_duplicates(6);
    triangles[3][1][0] = f64::NAN;
    let mut points = lattice_points(6);
    points.push([f64::INFINITY, 0.0, 0.0]);
    let _ = assert_all_agree(&triangles, &points);
    let finite = lattice_with_duplicates(6);
    let _ = assert_all_agree(&finite, &points);
}

#[test]
fn empty_and_tiny_inputs() {
    let triangles = lattice_with_duplicates(6);
    assert!(nearest_triangles(&triangles, &[]).hits.is_empty());
    let none = nearest_triangles(&[], &[[0.0; 3]; BVH_MIN_POINTS + 4]);
    assert!(none.hits.iter().all(Option::is_none));
    assert_eq!(none.tris_tested, 0);
    // Under the point threshold the dispatcher scans: it tests every pair.
    let few = nearest_triangles(&triangles, &[[0.5, 0.5, 1.0]; 3]);
    assert_eq!(few.tris_tested, 3 * u64::try_from(triangles.len()).unwrap());
}
