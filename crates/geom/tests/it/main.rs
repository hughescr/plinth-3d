//! The one integration-test binary of plinth-geom (design section 4.3): T0p property tests
//! (fixed seed, 64 cases) and tests on the original Round STLs.
//!
//! Ground truth for the batched closest-point query:
//! * the scalar reference (`reference::closest_points_reference`): same kernel, brute
//!   force; the batched result must equal it exactly, bit for bit;
//! * an independent oracle written here (plane projection plus edge distances), which must
//!   agree on the distance within a stated tolerance of 1e-9 (relative to 1 + distance^2).

use plinth_geom::counters::{self, Op};
use plinth_geom::reference::closest_points_reference;
use plinth_geom::{
    Bvh, ClosestHits, Mesh, Points, closest_points, closest_points_par, parse_stl, read_stl_file,
};
use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use std::path::PathBuf;

const ORACLE_TOLERANCE: f64 = 1e-9;

// ---------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------

fn round_stl(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/round/original")
        .join(name)
}

/// The thread count this slot may use. Tests read the allocation `scripts/t` sets; library
/// code never reads the environment.
#[allow(
    clippy::disallowed_methods,
    reason = "test-only read of the slot allocation set by scripts/t"
)]
fn slot_threads() -> usize {
    std::env::var("NEXTEST_TEST_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2)
        .clamp(1, 8)
}

/// Deterministic generator for query points (no rand crate).
struct Lcg(u64);

impl Lcg {
    fn next_unit(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1_u64 << 53) as f64
    }
}

/// `n` query points: random inside the mesh box inflated by `margin`, then every mesh
/// vertex, then every triangle centroid (exact-contact and tie cases).
fn queries_for(mesh: &Mesh, n: usize, margin: f64, seed: u64) -> Points {
    let b = mesh.bounds();
    let mut g = Lcg(seed);
    let (mut xs, mut ys, mut zs) = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..n {
        xs.push(b.min[0] - margin + g.next_unit() * (b.max[0] - b.min[0] + 2.0 * margin));
        ys.push(b.min[1] - margin + g.next_unit() * (b.max[1] - b.min[1] + 2.0 * margin));
        zs.push(b.min[2] - margin + g.next_unit() * (b.max[2] - b.min[2] + 2.0 * margin));
    }
    for i in 0..mesh.vertex_count() {
        xs.push(mesh.xs()[i]);
        ys.push(mesh.ys()[i]);
        zs.push(mesh.zs()[i]);
    }
    for t in 0..mesh.triangle_count() {
        let [a, b, c] = mesh.triangle_corners(t);
        xs.push((a[0] + b[0] + c[0]) / 3.0);
        ys.push((a[1] + b[1] + c[1]) / 3.0);
        zs.push((a[2] + b[2] + c[2]) / 3.0);
    }
    Points::new(xs, ys, zs).expect("finite queries")
}

fn assert_hits_identical(got: &ClosestHits, want: &ClosestHits, what: &str) {
    assert_eq!(got.len(), want.len(), "{what}: length");
    for i in 0..got.len() {
        let same = got.tri[i] == want.tri[i]
            && got.x[i].to_bits() == want.x[i].to_bits()
            && got.y[i].to_bits() == want.y[i].to_bits()
            && got.z[i].to_bits() == want.z[i].to_bits()
            && got.dist2[i].to_bits() == want.dist2[i].to_bits();
        assert!(
            same,
            "{what}: query {i} differs: tri {} vs {}, dist2 {} vs {}",
            got.tri[i], want.tri[i], got.dist2[i], want.dist2[i]
        );
    }
}

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Squared distance from `p` to segment `ab`, by minimising the quadratic in t.
fn segment_d2(p: V3, a: V3, b: V3) -> f64 {
    let ab = sub(b, a);
    let l2 = dot(ab, ab);
    let t = if l2 > 0.0 {
        (dot(sub(p, a), ab) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let q = [a[0] + t * ab[0], a[1] + t * ab[1], a[2] + t * ab[2]];
    let d = sub(p, q);
    dot(d, d)
}

/// Independent oracle: squared distance from `p` to triangle `abc`. The plane distance
/// applies only when the projection falls inside (checked by edge-normal signs); the three
/// edge distances always apply.
fn oracle_d2(p: V3, a: V3, b: V3, c: V3) -> f64 {
    let mut best = segment_d2(p, a, b)
        .min(segment_d2(p, b, c))
        .min(segment_d2(p, c, a));
    let n = cross(sub(b, a), sub(c, a));
    let n2 = dot(n, n);
    if n2 > 0.0 {
        let h = dot(sub(p, a), n);
        let inside = [(a, b), (b, c), (c, a)]
            .iter()
            .all(|&(u, v)| dot(cross(sub(v, u), sub(p, u)), n) >= 0.0);
        if inside {
            best = best.min(h * h / n2);
        }
    }
    best
}

fn oracle_mesh_d2(mesh: &Mesh, p: V3) -> f64 {
    (0..mesh.triangle_count())
        .map(|t| {
            let [a, b, c] = mesh.triangle_corners(t);
            oracle_d2(p, a, b, c)
        })
        .fold(f64::INFINITY, f64::min)
}

fn assert_matches_oracle(mesh: &Mesh, q: &Points, got: &ClosestHits, every: usize) {
    for i in (0..q.len()).step_by(every) {
        let want = oracle_mesh_d2(mesh, [q.xs()[i], q.ys()[i], q.zs()[i]]);
        assert!(
            (got.dist2[i] - want).abs() <= ORACLE_TOLERANCE * (1.0 + want),
            "query {i}: batched {} vs oracle {want}",
            got.dist2[i]
        );
    }
}

/// Run the batched query serially and compare it to the reference (exactly) and to the
/// oracle (within tolerance). Returns the batched hits.
fn check_against_reference(mesh: &Mesh, q: &Points, oracle_every: usize) -> ClosestHits {
    let bvh = Bvh::build(mesh).expect("bvh");
    let mut got = ClosestHits::with_len(q.len());
    closest_points(&bvh, q, &mut got).expect("batched");
    let want = closest_points_reference(mesh, q).expect("reference");
    assert_hits_identical(&got, &want, "batched vs reference");
    assert_matches_oracle(mesh, q, &got, oracle_every);
    got
}

/// Sorted multiset of triangles as coordinate bit patterns (corner order inside a triangle
/// kept, since winding matters).
fn triangle_multiset(mesh: &Mesh) -> Vec<[u64; 9]> {
    let mut v: Vec<[u64; 9]> = (0..mesh.triangle_count())
        .map(|t| {
            let c = mesh.triangle_corners(t);
            let mut k = [0_u64; 9];
            for (i, p) in c.iter().enumerate() {
                for (j, x) in p.iter().enumerate() {
                    k[3 * i + j] = (x + 0.0).to_bits();
                }
            }
            k
        })
        .collect();
    v.sort_unstable();
    v
}

// ---------------------------------------------------------------------------------------
// Round STL fixtures
// ---------------------------------------------------------------------------------------

const ROUNDS: [(&str, usize); 3] = [
    ("Round 25.stl", 2996),
    ("Round 50.stl", 6014),
    ("Round 100.stl", 1966),
];

#[test]
fn round_stls_read_with_the_documented_triangle_counts_and_bounds() {
    for (name, tris) in ROUNDS {
        let m = read_stl_file(&round_stl(name)).expect(name);
        assert_eq!(m.triangle_count(), tris, "{name}");
        assert!(
            m.vertex_count() < 3 * tris,
            "{name}: welding must share vertices"
        );
        let b = m.bounds();
        // Fixture README: the model spans z = -3.5 to 0 mm.
        assert!(
            (b.min[2] + 3.5).abs() < 1e-6 && b.max[2].abs() < 1e-6,
            "{name}: z {b:?}"
        );
        assert!(m.indices().iter().all(|&i| (i as usize) < m.vertex_count()));
    }
}

#[test]
fn round_batched_closest_points_equal_the_reference_exactly() {
    for (name, _) in ROUNDS {
        let m = read_stl_file(&round_stl(name)).expect(name);
        let q = queries_for(&m, 3000, 8.0, 0x5eed_0001);
        counters::reset();
        let bvh = Bvh::build(&m).expect("bvh");
        let mut got = ClosestHits::with_len(q.len());
        closest_points(&bvh, &q, &mut got).expect("batched");
        let c = counters::snapshot(Op::ClosestPoints);
        assert_eq!(c.calls, 1, "{name}: one batch call, not one per point");
        assert_eq!(c.items as usize, q.len(), "{name}: items");
        assert!(c.mean_items_per_call().expect("calls") >= 64.0, "{name}");
        let want = closest_points_reference(&m, &q).expect("reference");
        assert_hits_identical(&got, &want, name);
        assert_matches_oracle(&m, &q, &got, 25);
        // Vertices and centroids lie on the surface: distance is (near) zero.
        let on_surface = &got.dist2[3000..];
        let worst = on_surface.iter().copied().fold(0.0_f64, f64::max);
        let at = on_surface
            .iter()
            .position(|&d| d.to_bits() == worst.to_bits())
            .expect("worst");
        eprintln!(
            "{name}: worst surface-query dist2 {worst:e} at {at} (of {})",
            on_surface.len()
        );
        // Vertices are exact; a centroid of a sliver triangle may be off by rounding in the
        // face-region arithmetic. 1e-12 mm^2 is a distance of 1e-6 mm.
        assert!(
            worst < 1e-12,
            "{name}: surface queries: worst dist2 {worst:e}"
        );
    }
}

#[test]
fn threads_round_output_is_identical_at_one_thread_and_the_slot_allocation() {
    let alloc = slot_threads();
    let m = read_stl_file(&round_stl("Round 25.stl")).expect("Round 25");
    let q = queries_for(&m, 6000, 6.0, 0x5eed_0002);
    let bvh = Bvh::build(&m).expect("bvh");
    let mut serial = ClosestHits::with_len(q.len());
    closest_points(&bvh, &q, &mut serial).expect("serial");
    for threads in [1, 2, alloc] {
        let mut par = ClosestHits::with_len(q.len());
        closest_points_par(&bvh, &q, &mut par, threads).expect("parallel");
        assert_hits_identical(&par, &serial, &format!("{threads} threads"));
    }
    let want = closest_points_reference(&m, &q).expect("reference");
    assert_hits_identical(&serial, &want, "serial vs reference");
}

#[test]
fn round_ascii_and_binary_forms_of_the_same_model_read_identically() {
    let bytes = std::fs::read(round_stl("Round 100.stl")).expect("read");
    let bin = parse_stl(&bytes).expect("binary");
    let mut ascii = String::from("solid round\n");
    for t in 0..bin.triangle_count() {
        ascii.push_str("facet normal 0 0 0\nouter loop\n");
        for p in bin.triangle_corners(t) {
            ascii.push_str(&format!(
                "vertex {:e} {:e} {:e}\n",
                p[0] as f32, p[1] as f32, p[2] as f32
            ));
        }
        ascii.push_str("endloop\nendfacet\n");
    }
    ascii.push_str("endsolid round\n");
    let asc = parse_stl(ascii.as_bytes()).expect("ascii");
    assert_eq!(asc, bin);
}

// ---------------------------------------------------------------------------------------
// T0p properties (fixed seed, 64 cases)
// ---------------------------------------------------------------------------------------

type Corner = [i8; 3];
type Tri = [Corner; 3];

fn corner() -> impl Strategy<Value = Corner> {
    proptest::array::uniform3(-6_i8..=6)
}

fn tri_list() -> impl Strategy<Value = Vec<Tri>> {
    proptest::collection::vec(proptest::array::uniform3(corner()), 1..60)
}

/// Half-integer grid coordinates are exact in f32 and f64, so ties and degenerate
/// triangles (repeated or collinear corners) are common.
fn soup_of(tris: &[Tri]) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let mut s = (Vec::new(), Vec::new(), Vec::new());
    for c in tris.iter().flatten() {
        s.0.push(f64::from(c[0]) * 0.5);
        s.1.push(f64::from(c[1]) * 0.5);
        s.2.push(f64::from(c[2]) * 0.5);
    }
    s
}

fn mesh_of(tris: &[Tri]) -> Mesh {
    let (x, y, z) = soup_of(tris);
    Mesh::from_soup(&x, &y, &z).expect("grid soup")
}

/// Query points on a quarter-grid wider than the mesh, so exact ties are frequent.
fn grid_points(n: std::ops::Range<usize>) -> impl Strategy<Value = Vec<[i16; 3]>> {
    proptest::collection::vec(proptest::array::uniform3(-30_i16..=30), n)
}

fn points_of(g: &[[i16; 3]]) -> Points {
    Points::new(
        g.iter().map(|p| f64::from(p[0]) * 0.25).collect(),
        g.iter().map(|p| f64::from(p[1]) * 0.25).collect(),
        g.iter().map(|p| f64::from(p[2]) * 0.25).collect(),
    )
    .expect("grid points")
}

fn stl_bytes(tris: &[Tri]) -> Vec<u8> {
    let (x, y, z) = soup_of(tris);
    let mut b = vec![0_u8; 80];
    b.extend_from_slice(&(tris.len() as u32).to_le_bytes());
    for t in 0..tris.len() {
        b.extend_from_slice(&[0_u8; 12]);
        for k in 3 * t..3 * t + 3 {
            for v in [x[k], y[k], z[k]] {
                b.extend_from_slice(&(v as f32).to_le_bytes());
            }
        }
        b.extend_from_slice(&[0_u8; 2]);
    }
    b
}

fn fast_config() -> ProptestConfig {
    ProptestConfig {
        cases: 64,
        rng_seed: RngSeed::Fixed(0x0070_6c69_6e74_6833),
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(fast_config())]

    /// Batch equals the scalar reference exactly, and the independent oracle within 1e-9.
    #[test]
    fn prop_batched_closest_points_equal_reference(tris in tri_list(), g in grid_points(1..80)) {
        let mesh = mesh_of(&tris);
        let q = points_of(&g);
        check_against_reference(&mesh, &q, 1);
    }

    /// Same output at 1 thread and at 2 threads (the smallest slot allocation), with
    /// enough queries to split into more than one task.
    #[test]
    fn threads_prop_parallel_equals_serial(tris in tri_list(), g in grid_points(2100..2600)) {
        let mesh = mesh_of(&tris);
        let q = points_of(&g);
        let bvh = Bvh::build(&mesh).expect("bvh");
        let mut serial = ClosestHits::with_len(q.len());
        closest_points(&bvh, &q, &mut serial).expect("serial");
        for threads in [1, 2] {
            let mut par = ClosestHits::with_len(q.len());
            closest_points_par(&bvh, &q, &mut par, threads).expect("parallel");
            assert_hits_identical(&par, &serial, "parallel vs serial");
        }
    }

    /// Welding and Morton reordering keep the triangle set and the bounds, and reordering
    /// twice changes nothing.
    #[test]
    fn prop_reorder_keeps_the_triangles(tris in tri_list()) {
        let (x, y, z) = soup_of(&tris);
        let mesh = Mesh::from_soup(&x, &y, &z).expect("soup");
        let expected = {
            let mut v: Vec<[u64; 9]> = (0..tris.len()).map(|t| {
                let mut k = [0_u64; 9];
                for i in 0..3 {
                    k[3 * i] = (x[3 * t + i] + 0.0).to_bits();
                    k[3 * i + 1] = (y[3 * t + i] + 0.0).to_bits();
                    k[3 * i + 2] = (z[3 * t + i] + 0.0).to_bits();
                }
                k
            }).collect();
            v.sort_unstable();
            v
        };
        prop_assert_eq!(triangle_multiset(&mesh), expected);
        prop_assert_eq!(mesh.triangle_count(), tris.len());
        prop_assert!(mesh.vertex_count() <= 3 * tris.len());
        prop_assert_eq!(mesh.morton_reordered(), mesh.clone());
        // Every vertex is used or at least inside the bounds; indices are in range.
        prop_assert!(mesh.indices().iter().all(|&i| (i as usize) < mesh.vertex_count()));
    }

    /// Binary STL written from a triangle list reads back to the same triangles, and the
    /// same list written as ASCII gives an identical mesh.
    #[test]
    fn prop_stl_binary_and_ascii_round_trip(tris in tri_list()) {
        let bin = parse_stl(&stl_bytes(&tris)).expect("binary");
        prop_assert_eq!(triangle_multiset(&bin), triangle_multiset(&mesh_of(&tris)));
        let (x, y, z) = soup_of(&tris);
        let mut text = String::from("solid p\n");
        for t in 0..tris.len() {
            text.push_str("facet normal 0 0 0\nouter loop\n");
            for k in 3 * t..3 * t + 3 {
                text.push_str(&format!("vertex {} {} {}\n", x[k], y[k], z[k]));
            }
            text.push_str("endloop\nendfacet\n");
        }
        text.push_str("endsolid p\n");
        let asc = parse_stl(text.as_bytes()).expect("ascii");
        prop_assert_eq!(asc, bin);
    }

    /// The reader returns a result for any bytes and never panics: random bytes, a valid
    /// file cut anywhere, and a valid file with random bytes changed.
    #[test]
    fn prop_reader_survives_malformed_input(
        noise in proptest::collection::vec(any::<u8>(), 0..400),
        tris in tri_list(),
        cut in any::<prop::sample::Index>(),
        flips in proptest::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 0..8),
    ) {
        let _ = parse_stl(&noise);
        let mut text = b"solid ".to_vec();
        text.extend_from_slice(&noise);
        let _ = parse_stl(&text);
        let valid = stl_bytes(&tris);
        let _ = parse_stl(&valid[..cut.index(valid.len() + 1)]);
        let mut mutated = valid.clone();
        for (at, byte) in &flips {
            let i = at.index(mutated.len());
            mutated[i] = *byte;
        }
        let _ = parse_stl(&mutated);
        // A huge declared count on a short file must be an error, not an allocation.
        let mut huge = valid;
        huge[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
        prop_assert!(parse_stl(&huge).is_err());
    }
}
