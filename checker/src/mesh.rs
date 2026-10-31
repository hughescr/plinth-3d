//! Triangle-soup topology: welding, edge pairing, shells, volume and Euler characteristic.
//! Used for every support record mesh (native topology check) and for the model (the H5
//! "not applicable" proof).

use crate::v3::{V, cross, dot, f};
use std::collections::BTreeMap;

/// A welded mesh: vertices equal in f32 (bit for bit, -0 equal to +0) are one vertex.
#[derive(Debug, Clone)]
pub struct Welded {
    pub verts: Vec<[f32; 3]>,
    pub tris: Vec<[u32; 3]>,
}

fn key(v: [f32; 3]) -> [u32; 3] {
    v.map(|x| if x == 0.0 { 0 } else { x.to_bits() })
}

/// Weld a soup of nine f32 per triangle. Non-finite coordinates give `None`.
#[must_use]
pub fn weld(coords: &[f32]) -> Option<Welded> {
    if !coords.len().is_multiple_of(9) || coords.iter().any(|c| !c.is_finite()) {
        return None;
    }
    let mut ids: BTreeMap<[u32; 3], u32> = BTreeMap::new();
    let mut verts = Vec::new();
    let mut tris = Vec::with_capacity(coords.len() / 9);
    for t in coords.chunks_exact(9) {
        let mut tri = [0u32; 3];
        for (k, slot) in tri.iter_mut().enumerate() {
            let v = [t[3 * k], t[3 * k + 1], t[3 * k + 2]];
            let next = u32::try_from(verts.len()).ok()?;
            *slot = *ids.entry(key(v)).or_insert_with(|| {
                verts.push(v);
                next
            });
        }
        tris.push(tri);
    }
    Some(Welded { verts, tris })
}

/// What the edges say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeReport {
    /// Distinct undirected edges.
    pub edges: usize,
    /// Directed edges used more than once, or without an opposite partner, or degenerate
    /// triangles.
    pub bad: usize,
}

impl EdgeReport {
    /// Every edge is used exactly twice, once in each direction.
    #[must_use]
    pub fn closed_and_oriented(&self) -> bool {
        self.bad == 0
    }
}

/// Pair up directed edges.
#[must_use]
pub fn edge_report(m: &Welded) -> EdgeReport {
    let mut directed: Vec<(u32, u32)> = Vec::with_capacity(m.tris.len() * 3);
    let mut bad = 0usize;
    for t in &m.tris {
        if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
            bad += 1;
        }
        for k in 0..3 {
            directed.push((t[k], t[(k + 1) % 3]));
        }
    }
    directed.sort_unstable();
    for w in directed.windows(2) {
        if w[0] == w[1] {
            bad += 1;
        }
    }
    let mut edges = 0usize;
    for &(a, b) in &directed {
        if a < b {
            edges += 1;
        }
        if directed.binary_search(&(b, a)).is_err() {
            bad += 1;
        }
    }
    // A directed edge that has its opposite is counted once per orientation in `edges` only
    // when a < b; an unpaired edge with a > b is already in `bad`.
    EdgeReport { edges, bad }
}

/// Shell number of every vertex (connected components over the triangles), and the count.
#[must_use]
pub fn shells(m: &Welded) -> (Vec<u32>, u32) {
    let mut parent: Vec<u32> = (0..m.verts.len())
        .map(|i| u32::try_from(i).unwrap_or(0))
        .collect();
    fn find(p: &mut [u32], mut x: u32) -> u32 {
        while p[x as usize] != x {
            p[x as usize] = p[p[x as usize] as usize];
            x = p[x as usize];
        }
        x
    }
    for t in &m.tris {
        for k in 1..3 {
            let (a, b) = (find(&mut parent, t[0]), find(&mut parent, t[k]));
            if a != b {
                parent[a as usize] = b;
            }
        }
    }
    let mut label: BTreeMap<u32, u32> = BTreeMap::new();
    let mut out = Vec::with_capacity(m.verts.len());
    for i in 0..m.verts.len() {
        let r = find(&mut parent, u32::try_from(i).unwrap_or(0));
        let next = u32::try_from(label.len()).unwrap_or(u32::MAX);
        out.push(*label.entry(r).or_insert(next));
    }
    (out, u32::try_from(label.len()).unwrap_or(u32::MAX))
}

/// Signed volume (mm3) of the triangles whose first vertex is in each shell, indexed by
/// shell. Positive for outward counter-clockwise winding.
#[must_use]
pub fn shell_volumes(m: &Welded, shell_of: &[u32], n_shells: u32) -> Vec<f64> {
    let mut vol = vec![0.0f64; n_shells as usize];
    for t in &m.tris {
        let p = |i: usize| -> V { f(m.verts[t[i] as usize]) };
        let (a, b, c) = (p(0), p(1), p(2));
        vol[shell_of[t[0] as usize] as usize] += dot(a, cross(b, c)) / 6.0;
    }
    vol
}

/// Euler characteristic `V - E + F` of the shell.
#[must_use]
pub fn shell_euler(m: &Welded, shell_of: &[u32], shell: u32) -> i64 {
    let v = shell_of.iter().filter(|s| **s == shell).count();
    let faces: Vec<&[u32; 3]> = m
        .tris
        .iter()
        .filter(|t| shell_of[t[0] as usize] == shell)
        .collect();
    let mut e: Vec<(u32, u32)> = Vec::with_capacity(faces.len() * 3);
    for t in &faces {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            e.push((a.min(b), a.max(b)));
        }
    }
    e.sort_unstable();
    e.dedup();
    let to_i = |x: usize| i64::try_from(x).unwrap_or(i64::MAX);
    to_i(v) - to_i(e.len()) + to_i(faces.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit cube as 12 outward triangles.
    fn cube(inward: bool) -> Vec<f32> {
        let p = |i: u8| {
            [
                f32::from(i & 1),
                f32::from((i >> 1) & 1),
                f32::from((i >> 2) & 1),
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
            let tris = if inward {
                [[q[0], q[2], q[1]], [q[0], q[3], q[2]]]
            } else {
                [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]
            };
            for t in tris {
                for i in t {
                    out.extend(p(i));
                }
            }
        }
        out
    }

    #[test]
    fn a_cube_is_closed_oriented_one_shell_volume_one_euler_two() {
        let w = weld(&cube(false)).expect("welds");
        assert_eq!((w.verts.len(), w.tris.len()), (8, 12));
        let r = edge_report(&w);
        assert!(r.closed_and_oriented(), "{r:?}");
        assert_eq!(r.edges, 18);
        let (sh, n) = shells(&w);
        assert_eq!(n, 1);
        assert!((shell_volumes(&w, &sh, n)[0] - 1.0).abs() < 1e-12);
        assert_eq!(shell_euler(&w, &sh, 0), 2);
    }

    #[test]
    fn a_missing_face_or_a_flipped_face_is_not_closed_and_oriented() {
        let mut c = cube(false);
        c.truncate(c.len() - 9);
        assert!(!edge_report(&weld(&c).expect("welds")).closed_and_oriented());
        let mut flipped = cube(false);
        let (a, b) = (flipped[0..3].to_vec(), flipped[3..6].to_vec());
        flipped[0..3].copy_from_slice(&b);
        flipped[3..6].copy_from_slice(&a);
        assert!(!edge_report(&weld(&flipped).expect("welds")).closed_and_oriented());
    }

    #[test]
    fn an_inward_cube_has_negative_volume_and_two_cubes_are_two_shells() {
        let w = weld(&cube(true)).expect("welds");
        let (sh, n) = shells(&w);
        assert!(shell_volumes(&w, &sh, n)[0] < -0.99);
        let mut two = cube(false);
        let shifted: Vec<f32> = cube(false)
            .chunks(3)
            .flat_map(|v| [v[0] + 5.0, v[1], v[2]])
            .collect();
        two.extend(shifted);
        let w2 = weld(&two).expect("welds");
        assert_eq!(shells(&w2).1, 2);
        assert!(weld(&[f32::NAN; 9]).is_none());
    }
}
