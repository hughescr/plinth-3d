//! Connected native raft: 2.2 mm pads and bridges, 0.2 mm thick.
//! A stable Prim tree on squared distance needs O(n) storage, not an n² edge list.

/// Pad diameter, mm **[O]**.
pub const PAD_DIAMETER_MM: f32 = 2.2;
/// Thickness, mm **[O]**.
pub const THICKNESS_MM: f32 = 0.2;
/// Bridge width, mm **[O]**.
pub const BRIDGE_WIDTH_MM: f32 = 2.2;

/// Minimum spanning tree, stable in input order under exact ties. Non-finite inputs
/// produce no tree; the sealed raft constructor refuses that result.
pub fn bridge_pairs(centres: &[[f32; 2]], out: &mut Vec<[u32; 2]>) {
    out.clear();
    if centres.is_empty() || centres.iter().flatten().any(|v| !v.is_finite()) {
        return;
    }
    let n = centres.len();
    let mut used = vec![false; n];
    let mut costs = vec![f64::INFINITY; n];
    let mut parent = vec![0; n];
    costs[0] = 0.0;
    for _ in 0..n {
        let Some(at) = (0..n)
            .filter(|&i| !used[i])
            .min_by(|&i, &j| costs[i].total_cmp(&costs[j]).then(i.cmp(&j)))
        else {
            break;
        };
        used[at] = true;
        if at != 0 {
            out.push([parent[at] as u32, at as u32]);
        }
        for j in 0..n {
            if used[j] {
                continue;
            }
            let d = (f64::from(centres[at][0]) - f64::from(centres[j][0])).powi(2)
                + (f64::from(centres[at][1]) - f64::from(centres[j][1])).powi(2);
            if d < costs[j] {
                costs[j] = d;
                parent[j] = at;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tree_has_measured_connectivity_and_minimum_length() {
        let c = [[0., 0.], [3., 0.], [3., 4.], [0., 4.]];
        let mut out = Vec::new();
        bridge_pairs(&c, &mut out);
        assert_eq!(out.len(), 3);
        let length: f64 = out
            .iter()
            .map(|&[a, b]| {
                let a = c[a as usize];
                let b = c[b as usize];
                (f64::from(a[0] - b[0]).powi(2) + f64::from(a[1] - b[1]).powi(2)).sqrt()
            })
            .sum();
        assert!((length - 10.0).abs() < 1e-10);
        let mut seen = [false; 4];
        seen[0] = true;
        for &[a, b] in &out {
            assert!(seen[a as usize]);
            seen[b as usize] = true;
        }
        assert!(seen.iter().all(|&v| v));
    }
}
