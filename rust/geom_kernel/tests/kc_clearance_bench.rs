//! Kc microbenchmark: wall time of the batched clearance entry points on a
//! realistic row set. Public API only, so the same file times any revision of
//! the kernel (copy it into a checkout of the revision to time that one).
//!
//! Run (release-like profile; add `PLINTH_THREADS=1` for one thread):
//!
//! ```text
//! KC_STL=/path/model.stl cargo test --profile gate -p geom_kernel \
//!     --test kc_clearance_bench -- --ignored --nocapture
//! ```
//!
//! Without `KC_STL` a noisy icosphere of about 80 000 faces stands in. Rows are
//! the lip read-ahead's shape: batches of 50 short capsule axes beside the
//! surface at 0 to 4 mm, radius 0.15 to 0.5 mm, standoff 0.3 mm. The printed
//! `checksum` folds every result's bits: it must be equal between revisions.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "benchmark fixture: small index casts"
)]

use std::time::Instant;

use geom_kernel::testkit::build_from_tris;
use geom_kernel::{NativeSurface, SegmentClearanceClassified, V3};

struct Rng(u64);

impl Rng {
    fn unit(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let bits = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11;
        (bits as f64) / ((1u64 << 53) as f64)
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn len(a: V3) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

fn read_stl(path: &str) -> Option<Vec<f64>> {
    let bytes = std::fs::read(path).ok()?;
    let n = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
    if bytes.len() < 84 + 50 * n {
        return None;
    }
    let mut t = Vec::with_capacity(n * 9);
    for i in 0..n {
        let base = 84 + 50 * i + 12;
        for k in 0..9 {
            let o = base + 4 * k;
            t.push(f64::from(f32::from_le_bytes([
                bytes[o],
                bytes[o + 1],
                bytes[o + 2],
                bytes[o + 3],
            ])));
        }
    }
    Some(t)
}

fn sphere(subdiv: usize) -> Vec<f64> {
    let g = (1.0 + 5.0_f64.sqrt()) / 2.0;
    let mut verts: Vec<V3> = vec![
        [-1.0, g, 0.0],
        [1.0, g, 0.0],
        [-1.0, -g, 0.0],
        [1.0, -g, 0.0],
        [0.0, -1.0, g],
        [0.0, 1.0, g],
        [0.0, -1.0, -g],
        [0.0, 1.0, -g],
        [g, 0.0, -1.0],
        [g, 0.0, 1.0],
        [-g, 0.0, -1.0],
        [-g, 0.0, 1.0],
    ];
    let mut faces: Vec<[usize; 3]> = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    for _ in 0..subdiv {
        let mut cache = std::collections::BTreeMap::new();
        let mut next = Vec::with_capacity(faces.len() * 4);
        for f in &faces {
            let mut mid = |i: usize, j: usize| -> usize {
                *cache.entry((i.min(j), i.max(j))).or_insert_with(|| {
                    let m = mul(add(verts[i], verts[j]), 0.5);
                    verts.push(m);
                    verts.len() - 1
                })
            };
            let ab = mid(f[0], f[1]);
            let bc = mid(f[1], f[2]);
            let ca = mid(f[2], f[0]);
            next.push([f[0], ab, ca]);
            next.push([f[1], bc, ab]);
            next.push([f[2], ca, bc]);
            next.push([ab, bc, ca]);
        }
        faces = next;
    }
    let pts: Vec<V3> = verts
        .iter()
        .map(|&v| {
            let u = mul(v, 1.0 / len(v));
            let lobe = 0.5 * (3.0 * u[0]).sin() * (2.0 * u[1]).cos() + 0.3 * (4.0 * u[2]).sin()
                + 0.1 * (23.0 * u[0] + 17.0 * u[1]).sin();
            mul(u, 20.0 * (1.0 + 0.15 * lobe))
        })
        .collect();
    let mut t = Vec::new();
    for f in &faces {
        for &i in f {
            t.extend_from_slice(&pts[i]);
        }
    }
    t
}

struct Rows {
    p0: Vec<V3>,
    p1: Vec<V3>,
    r0: Vec<f64>,
    r1: Vec<f64>,
    inset: Vec<f64>,
    at_least: Vec<f64>,
}

fn rows(tris: &[f64], n: usize) -> Rows {
    let nf = tris.len() / 9;
    let mut rng = Rng(0x00C0_FFEE_D00D_1234);
    let mut r = Rows {
        p0: Vec::new(),
        p1: Vec::new(),
        r0: Vec::new(),
        r1: Vec::new(),
        inset: vec![0.0; n],
        at_least: vec![0.3; n],
    };
    for _ in 0..n {
        let f = ((rng.unit() * nf as f64) as usize).min(nf - 1);
        let v = |k: usize| -> V3 { [tris[f * 9 + 3 * k], tris[f * 9 + 3 * k + 1], tris[f * 9 + 3 * k + 2]] };
        let (p, q, s) = (v(0), v(1), v(2));
        let (mut u, mut w) = (rng.unit(), rng.unit());
        if u + w > 1.0 {
            u = 1.0 - u;
            w = 1.0 - w;
        }
        let on = add(add(p, mul(sub(q, p), u)), mul(sub(s, p), w));
        let c = cross(sub(q, p), sub(s, p));
        let nrm = mul(c, 1.0 / len(c).max(1e-30));
        let a = add(on, mul(nrm, rng.range(0.0, 4.0)));
        // a downward-leaning axis, 3 to 12 mm
        let d = [rng.range(-0.5, 0.5), rng.range(-0.5, 0.5), -1.0];
        let b = add(a, mul(d, rng.range(3.0, 12.0) / len(d)));
        r.p0.push(a);
        r.p1.push(b);
        let rad = rng.range(0.15, 0.5);
        r.r0.push(rad);
        r.r1.push(rad);
    }
    r
}

fn fold(rows: &[Option<SegmentClearanceClassified>]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    let mut eat = |x: u64| {
        h ^= x;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    for row in rows {
        match row {
            None => eat(1),
            Some(c) => {
                let (tag, mm, w) = match *c {
                    SegmentClearanceClassified::Measured { clearance_mm, witness } => (2, clearance_mm, witness),
                    SegmentClearanceClassified::UpperBound { clearance_mm, witness, .. } => (3, clearance_mm, witness),
                    SegmentClearanceClassified::Unmeasured(_) => (4, 0.0, [0.0; 3]),
                };
                eat(tag);
                eat(mm.to_bits());
                for x in w {
                    eat(x.to_bits());
                }
            }
        }
    }
    h
}

fn time_batches(s: &NativeSurface, r: &Rows, batch: usize, bounded: bool) -> (f64, u64) {
    let n = r.p0.len();
    let mut all = Vec::with_capacity(n);
    let started = Instant::now();
    let mut i = 0;
    while i < n {
        let j = (i + batch).min(n);
        let out = if bounded {
            s.segment_clearance_classified_at_least(
                &r.p0[i..j],
                &r.p1[i..j],
                &r.r0[i..j],
                &r.r1[i..j],
                0.3,
                &r.inset[i..j],
                &r.inset[i..j],
                &r.at_least[i..j],
            )
            .expect("rows")
        } else {
            s.segment_clearance_classified(
                &r.p0[i..j],
                &r.p1[i..j],
                &r.r0[i..j],
                &r.r1[i..j],
                0.3,
                &r.inset[i..j],
                &r.inset[i..j],
            )
            .expect("rows")
            .into_iter()
            .map(Some)
            .collect()
        };
        all.extend(out);
        i = j;
    }
    (started.elapsed().as_secs_f64(), fold(&all))
}

#[test]
#[ignore = "benchmark: run on demand"]
fn kc_bench() {
    let tris = std::env::var("KC_STL")
        .ok()
        .and_then(|p| read_stl(&p))
        .unwrap_or_else(|| sphere(6));
    let nf = tris.len() / 9;
    let n: usize = std::env::var("KC_ROWS").ok().and_then(|s| s.parse().ok()).unwrap_or(40_000);
    let started = Instant::now();
    let s = build_from_tris(tris.clone());
    eprintln!("faces {nf}, surface build {:.2} s", started.elapsed().as_secs_f64());
    let r = rows(&tris, n);
    // warm up
    let _ = time_batches(&s, &rows(&tris, 2_000), 50, true);
    for rep in 0..3 {
        let (t_b, h_b) = time_batches(&s, &r, 50, true);
        let (t_u, h_u) = time_batches(&s, &r, 50, false);
        eprintln!(
            "rep {rep}: {n} rows in batches of 50: bounded {t_b:.3} s ({:.1} us/row) checksum {h_b:016x}; \
             unbounded {t_u:.3} s ({:.1} us/row) checksum {h_u:016x}",
            1e6 * t_b / n as f64,
            1e6 * t_u / n as f64
        );
    }
}
