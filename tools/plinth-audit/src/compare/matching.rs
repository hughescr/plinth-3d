//! Matching of items between two files that share a frame (same pose): support
//! records by their ends, audit failure items by their anchor point. Greedy,
//! nearest pair first, one-to-one, deterministic (ties broken by index).

use super::consts as k;
use super::side::{Item, RecInfo};
use crate::reader::RecordKind;
use crate::vec3::{self as v, V3};
use std::collections::HashMap;

/// A spatial hash over points, cell side `cell`.
pub struct Hash3 {
    cell: f64,
    map: HashMap<(i64, i64, i64), Vec<usize>>,
}

impl Hash3 {
    #[must_use]
    pub fn new(points: impl IntoIterator<Item = (usize, V3)>, cell: f64) -> Self {
        let mut map: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
        for (i, p) in points {
            map.entry(Self::key(p, cell)).or_default().push(i);
        }
        Self { cell, map }
    }
    fn key(p: V3, cell: f64) -> (i64, i64, i64) {
        ((p[0] / cell).floor() as i64, (p[1] / cell).floor() as i64, (p[2] / cell).floor() as i64)
    }
    /// Indices in the 27 cells around `p`, in no particular order.
    pub fn near(&self, p: V3) -> impl Iterator<Item = usize> + '_ {
        let (cx, cy, cz) = Self::key(p, self.cell);
        (-1..=1).flat_map(move |dx| {
            (-1..=1).flat_map(move |dy| (-1..=1).map(move |dz| (cx + dx, cy + dy, cz + dz)))
        })
        .filter_map(|c| self.map.get(&c))
        .flat_map(|v| v.iter().copied())
    }
}

/// Cost of pairing two records of the same kind, or `None` when they are too far apart.
fn record_cost(a: &RecInfo, b: &RecInfo) -> Option<f64> {
    if a.kind != b.kind {
        return None;
    }
    let c = if a.kind.is_ball() {
        2.0 * v::dist(a.mid, b.mid)
    } else {
        let straight = v::dist(a.a, b.a) + v::dist(a.b, b.b);
        let flipped = v::dist(a.a, b.b) + v::dist(a.b, b.a);
        straight.min(flipped)
    };
    (c <= 2.0 * k::MATCH_MM).then_some(c)
}

/// One-to-one nearest-first matching of support records. Returns, for each
/// reference record, the candidate record it matched.
#[must_use]
pub fn match_records(r: &[RecInfo], c: &[RecInfo]) -> Vec<Option<usize>> {
    let h = Hash3::new(c.iter().enumerate().map(|(i, x)| (i, x.mid)), 0.4);
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for (i, a) in r.iter().enumerate() {
        for j in h.near(a.mid) {
            if let Some(cost) = record_cost(a, &c[j]) {
                pairs.push((cost, i, j));
            }
        }
    }
    pairs.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
    let mut out = vec![None; r.len()];
    let mut taken = vec![false; c.len()];
    for (_, i, j) in pairs {
        if out[i].is_none() && !taken[j] {
            out[i] = Some(j);
            taken[j] = true;
        }
    }
    out
}

/// What a pair of audit items means for one invariant.
#[derive(Clone, Debug, Default)]
pub struct ItemDiff {
    /// Candidate failing items with no failing reference item at the place.
    pub new_failing: Vec<usize>,
    /// Matched failing items whose severity grew by more than the limit.
    pub worse: Vec<(usize, usize)>,
    /// Candidate items that cannot be evaluated where the reference evaluated.
    pub lost_evaluation: Vec<usize>,
    /// Reference failing items that no longer fail.
    pub resolved: usize,
    /// Candidate items that moved across their own threshold inside the measured noise
    /// band ([`Allowance`]): `(candidate index, "new" or "lost")`. They are reported and
    /// counted, and they decide nothing.
    pub absorbed: Vec<(usize, &'static str)>,
}

/// The flicker allowance for one invariant (see `consts::flicker_band`).
///
/// An audit item sits near its own threshold in a legitimate file and moves across it
/// under noise that changes nothing physical. Such a crossing is not a regression when
/// it is small (`band`: the candidate item's severity is within the measured noise of its
/// code) and rare (`cap`: no more crossings than the noise ever produced, as a count of
/// the invariant's checked items). Past the cap nothing is absorbed: many crossings in
/// one invariant are a shift, not flicker. And noise is noise on an unchanged design: an item
/// beside a record that exists in only one of the two files (`veto`) is a consequence of that
/// change, never flicker.
pub struct Allowance<'a> {
    /// Noise band of an item, in the severity units of its code: for a new failing item
    /// (`false`) or for the growth of an item that already failed (`true`).
    pub band: &'a dyn Fn(&Item, bool) -> f64,
    /// True for an item that stands beside a record present in only one file.
    pub veto: &'a dyn Fn(&Item) -> bool,
    /// The most items one invariant may absorb.
    pub cap: usize,
}

impl Allowance<'_> {
    /// No allowance: every crossing counts (the strict reading, and the unit tests').
    #[must_use]
    pub fn none() -> Allowance<'static> {
        Allowance { band: &|_, _| 0.0, veto: &|_| false, cap: 0 }
    }
}

fn item_pairs(ri: &[&Item], ci: &[&Item]) -> Vec<(usize, usize)> {
    // (rank: 0 same code, 1 other code), distance, ref idx, cand idx
    let mut cand: Vec<(u8, f64, usize, usize)> = Vec::new();
    for (i, a) in ri.iter().enumerate() {
        for (j, b) in ci.iter().enumerate() {
            let d = match (a.anchor, b.anchor) {
                (Some(p), Some(q)) => {
                    let d = v::dist(p, q);
                    if d > k::H4_MATCH_MM {
                        continue;
                    }
                    d
                }
                (None, None) => {
                    if a.code != b.code {
                        continue;
                    }
                    0.0
                }
                _ => continue,
            };
            cand.push((u8::from(a.code != b.code), d, i, j));
        }
    }
    cand.sort_by(|x, y| x.0.cmp(&y.0).then(x.1.total_cmp(&y.1)).then(x.2.cmp(&y.2)).then(x.3.cmp(&y.3)));
    let (mut ur, mut uc) = (vec![false; ri.len()], vec![false; ci.len()]);
    let mut out = Vec::new();
    for (_, _, i, j) in cand {
        if !ur[i] && !uc[j] {
            ur[i] = true;
            uc[j] = true;
            out.push((i, j));
        }
    }
    out
}

/// Diff one invariant's items, matched by place (and code where there is a choice).
#[must_use]
pub fn diff_items(r: &[Item], c: &[Item]) -> ItemDiff {
    diff_items_with(r, c, &Allowance::none())
}

/// [`diff_items`] with a flicker allowance.
#[must_use]
pub fn diff_items_with(r: &[Item], c: &[Item], allow: &Allowance) -> ItemDiff {
    // A failing item whose severity is under the noise floor of its own
    // quantity (one pixel of area, one voxel of volume, one voxel of length)
    // is quantisation, not a defect: it is not an item at all.
    let significant = |x: &&Item| x.indeterminate || x.floor <= 0.0 || x.severity > x.floor;
    let rmap: Vec<usize> = (0..r.len()).filter(|&i| significant(&&r[i])).collect();
    let cmap: Vec<usize> = (0..c.len()).filter(|&i| significant(&&c[i])).collect();
    let ri: Vec<&Item> = rmap.iter().map(|&i| &r[i]).collect();
    let ci: Vec<&Item> = cmap.iter().map(|&i| &c[i]).collect();
    // Spatial pre-filter keeps this near-linear when the lists are long.
    let pairs = if ri.len() * ci.len() <= 4_000_000 {
        item_pairs(&ri, &ci)
    } else {
        let h = Hash3::new(
            ci.iter().enumerate().filter_map(|(j, x)| x.anchor.map(|p| (j, p))),
            k::H4_MATCH_MM.max(0.2),
        );
        let mut cand: Vec<(u8, f64, usize, usize)> = Vec::new();
        for (i, a) in ri.iter().enumerate() {
            if let Some(p) = a.anchor {
                for j in h.near(p) {
                    if let Some(q) = ci[j].anchor {
                        let d = v::dist(p, q);
                        if d <= k::H4_MATCH_MM {
                            cand.push((u8::from(a.code != ci[j].code), d, i, j));
                        }
                    }
                }
            }
        }
        cand.sort_by(|x, y| x.0.cmp(&y.0).then(x.1.total_cmp(&y.1)).then(x.2.cmp(&y.2)).then(x.3.cmp(&y.3)));
        let (mut ur, mut uc) = (vec![false; ri.len()], vec![false; ci.len()]);
        let mut out = Vec::new();
        for (_, _, i, j) in cand {
            if !ur[i] && !uc[j] {
                ur[i] = true;
                uc[j] = true;
                out.push((i, j));
            }
        }
        out
    };
    let mut d = ItemDiff::default();
    let mut matched_c = vec![false; ci.len()];
    let mut matched_r = vec![false; ri.len()];
    for (i, j) in pairs {
        matched_r[i] = true;
        matched_c[j] = true;
        let (a, b) = (ri[i], ci[j]);
        match (a.indeterminate, b.indeterminate) {
            (false, false) => {
                if b.severity > a.severity * (1.0 + k::H4_WORSE_FRACTION) + b.floor.max(a.floor).max(if (allow.veto)(b) { 0.0 } else { (allow.band)(b, true) }) {
                    d.worse.push((i, j));
                }
            }
            // The reference already fails here, so the candidate not being able to decide is the
            // same gate (a failing item and an indeterminate one both block) and no step back.
            // An indeterminate candidate item with NO reference item at its place is different:
            // the reference passed there, and that is counted below as lost evaluation.
            (false, true) => {}
            (true, false) => d.new_failing.push(j),
            (true, true) => {}
        }
    }
    for (j, b) in ci.iter().enumerate() {
        if matched_c[j] {
            continue;
        }
        if b.indeterminate {
            d.lost_evaluation.push(j);
        } else {
            d.new_failing.push(j);
        }
    }
    d.resolved = ri.iter().enumerate().filter(|(i, a)| !matched_r[*i] && !a.indeterminate).count();
    // Indices were into the filtered lists; hand back indices into the callers' lists.
    for j in &mut d.new_failing {
        *j = cmap[*j];
    }
    for j in &mut d.lost_evaluation {
        *j = cmap[*j];
    }
    for p in &mut d.worse {
        *p = (rmap[p.0], cmap[p.1]);
    }
    // Flicker: a new failing item inside its code's noise band, or an item the candidate can no
    // longer evaluate. All of them or none: past the cap the invariant has shifted.
    let small = |j: &usize| !c[*j].indeterminate && !(allow.veto)(&c[*j]) && c[*j].severity <= (allow.band)(&c[*j], false);
    let loose = |j: &usize| !(allow.veto)(&c[*j]);
    let n_new = d.new_failing.iter().filter(|j| small(j)).count();
    let n_lost = d.lost_evaluation.iter().filter(|j| loose(j)).count();
    if n_new + n_lost > 0 && n_new + n_lost <= allow.cap {
        d.absorbed.extend(d.new_failing.iter().filter(|j| small(j)).map(|&j| (j, "new")));
        d.new_failing.retain(|j| !small(j));
        d.absorbed.extend(d.lost_evaluation.iter().filter(|j| loose(j)).map(|&j| (j, "lost")));
        d.lost_evaluation.retain(|j| !loose(j));
    }
    d
}

/// Pairs of matched contact balls, `(ref tip record, cand tip record, distance)`.
#[must_use]
pub fn match_tips(r: &[RecInfo], rt: &[usize], c: &[RecInfo], ct: &[usize]) -> Vec<(usize, usize, f64)> {
    let h = Hash3::new(ct.iter().map(|&j| (j, c[j].mid)), 0.4);
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for &i in rt {
        for j in h.near(r[i].mid) {
            if c[j].kind == RecordKind::ContactSphere {
                let d = v::dist(r[i].mid, c[j].mid);
                if d <= k::MATCH_MM {
                    pairs.push((d, i, j));
                }
            }
        }
    }
    pairs.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
    let mut used_r = std::collections::HashSet::new();
    let mut used_c = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (d, i, j) in pairs {
        if !used_r.contains(&i) && !used_c.contains(&j) {
            used_r.insert(i);
            used_c.insert(j);
            out.push((i, j, d));
        }
    }
    out.sort_by_key(|p| p.0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(code: &str, at: V3, sev: f64, ind: bool) -> Item {
        Item {
            inv: 1,
            code: code.into(),
            indeterminate: ind,
            anchor: Some(at),
            severity: sev,
            floor: 0.0,
            scale: 1.0,
            contact: None,
            record: None,
        }
    }

    #[test]
    fn a_new_failure_cannot_hide_behind_an_improvement_elsewhere() {
        let r = vec![item("A", [0.0, 0.0, 0.0], 5.0, false)];
        let c = vec![item("A", [3.0, 0.0, 0.0], 1.0, false)];
        let d = diff_items(&r, &c);
        assert_eq!(d.new_failing.len(), 1, "the failure moved, so it is new");
        assert_eq!(d.resolved, 1);
    }

    #[test]
    fn matched_items_are_worse_only_past_the_limit() {
        let r = vec![item("A", [0.0; 3], 1.00, false)];
        assert!(diff_items(&r, &[item("A", [0.01, 0.0, 0.0], 1.09, false)]).worse.is_empty());
        assert_eq!(diff_items(&r, &[item("A", [0.01, 0.0, 0.0], 1.20, false)]).worse.len(), 1);
    }

    #[test]
    fn losing_the_ability_to_evaluate_is_not_a_pass() {
        let r = vec![item("A", [0.0; 3], 1.0, false)];
        let c = vec![item("A_INDETERMINATE", [0.0; 3], 1.0, true)];
        // Where the reference passed (no item), the candidate that cannot decide is a loss.
        assert_eq!(diff_items(&[], &c).lost_evaluation.len(), 1);
        // Where the reference already failed, indeterminate blocks just as a failure does.
        assert_eq!(diff_items(&r, &c).lost_evaluation.len(), 0);
        assert_eq!(diff_items(&r, &c).new_failing.len(), 0);
    }

    #[test]
    fn flicker_is_absorbed_only_inside_the_band_and_the_cap() {
        let band = |it: &Item, _worse: bool| if it.code == "A" { 0.5 } else { 0.0 };
        let never = |_: &Item| false;
        let r: Vec<Item> = vec![];
        // Two small new failures and one lost evaluation, cap 3: all absorbed.
        let c = vec![item("A", [0.0; 3], 0.4, false), item("A", [5.0; 3], 0.1, false), item("A_INDETERMINATE", [9.0; 3], 1.0, true)];
        let d = diff_items_with(&r, &c, &Allowance { band: &band, veto: &never, cap: 3 });
        assert!(d.new_failing.is_empty() && d.lost_evaluation.is_empty(), "{d:?}");
        assert_eq!(d.absorbed.len(), 3);
        // Cap 2: three crossings are a shift, so none is absorbed.
        let d = diff_items_with(&r, &c, &Allowance { band: &band, veto: &never, cap: 2 });
        assert_eq!((d.new_failing.len(), d.lost_evaluation.len(), d.absorbed.len()), (2, 1, 0));
        // An item past its band is never absorbed, whatever the cap.
        let big = vec![item("A", [0.0; 3], 0.6, false)];
        let d = diff_items_with(&r, &big, &Allowance { band: &band, veto: &never, cap: 9 });
        assert_eq!((d.new_failing.len(), d.absorbed.len()), (1, 0));
        // An item beside a record that exists in one file only is a consequence, not noise.
        let near_change = |it: &Item| it.anchor.is_some_and(|p| p[0] < 1.0);
        let d = diff_items_with(&r, &c, &Allowance { band: &band, veto: &near_change, cap: 3 });
        assert_eq!((d.new_failing.len(), d.absorbed.len()), (1, 2), "the item at x=0 stays a finding");
        // A matched item grows by the band without being worse; past it, it is.
        let r1 = vec![item("A", [0.0; 3], 1.0, false)];
        let w = |sev: f64| diff_items_with(&r1, &[item("A", [0.001, 0.0, 0.0], sev, false)], &Allowance { band: &band, veto: &never, cap: 0 }).worse.len();
        assert_eq!(w(1.55), 0, "10 % plus the 0.5 band");
        assert_eq!(w(1.65), 1);
    }
}
