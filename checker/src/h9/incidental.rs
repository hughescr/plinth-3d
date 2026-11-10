//! Thin material is forbidden; a redundant contact between independently rooted thick
//! interiors is not thin material. Remove *all* uncertified edges before testing reachability.
//! A weak cycle must never certify itself. This proof uses only convex native written parts.

use super::*;

/// A native part is already one closed outward shell and matches a canonical convex primitive
/// (`parts::validate`). Recheck its written face half-spaces with the per-face rounding
/// tolerance. Invalid/non-convex geometry never receives the incidental-contact exemption.
fn convex(p: &Part) -> bool {
    let Some(b) = body(p) else { return false };
    p.is_valid()
        && !b.planes.is_empty()
        && b.planes
            .iter()
            .zip(b.slack)
            .all(|((n, o), slack)| p.tris.iter().flatten().all(|v| dot(*n, *v) <= o + slack))
}

/// Certify the part's own connected thick interior, not its record number or a bounding box.
/// In particular a fat frustum with a thin tail cannot act as a graph node. Axial backing by
/// other records is not used here: those records must earn their own nodes and certified joins.
fn own_wide(scene: &Scene, p: &Part, limit: f64) -> bool {
    if !convex(p) {
        return false;
    }
    match p.geom {
        Geom::Frust { .. } => axis_of(scene, p).is_some_and(|mut axis| {
            axis.balls.clear();
            axis.dev_union = axis.dev;
            matches!(axis.scan(limit), Fate::Wide { .. })
                && matches!(raft_finding(p, limit).fate, Fate::Wide { .. })
        }),
        Geom::Ball { c, .. } => {
            body(p).is_some_and(|b| 2.0 * insphere(&b, c) - allowance_mm(p.bbox.max_abs()) >= limit)
        }
        Geom::Raft => matches!(raft_finding(p, limit).fate, Fate::Wide { .. }),
    }
}

fn on_plate(scene: &Scene, p: &Part) -> bool {
    let low = p
        .tris
        .iter()
        .flatten()
        .map(|v| v[2])
        .fold(f64::INFINITY, f64::min);
    p.role.is_plate_part() && (low - scene.plate_z()).abs() <= crate::h4::PLATE_AGREE_MM
}

/// Flood only certified edges in one batch. No exempted edge is ever put back in the graph.
fn rooted(roots: &[bool], edges: &[(usize, usize)]) -> Vec<bool> {
    let mut adjacent = vec![Vec::new(); roots.len()];
    for &(i, j) in edges {
        adjacent[i].push(j);
        adjacent[j].push(i);
    }
    let mut reached = roots.to_vec();
    let mut stack: Vec<usize> = (0..roots.len()).filter(|&i| roots[i]).collect();
    while let Some(i) = stack.pop() {
        for &j in &adjacent[i] {
            if !reached[j] {
                reached[j] = true;
                stack.push(j);
            }
        }
    }
    reached
}

fn pair_finding(p: &Part, q: &Part, limit: f64) -> Option<Finding> {
    if !p.bbox.inflated(1e-6).overlaps(&q.bbox) {
        return None;
    }
    let (bp, bq) = (body(p)?, body(q)?);
    let allow = allowance_mm(p.bbox.max_abs().max(q.bbox.max_abs()));
    let (fate, at) = join_fate(&bp, &bq, limit, TOUCH_MM, allow)?;
    Some(Finding {
        rec: p.rec,
        partner: Some(q.rec),
        incidental: false,
        role: p.role,
        what: "join",
        fate,
        analytic: None,
        at,
    })
}

/// Findings preserve the old axial/pairwise rule unless every part independently qualifies
/// and still reaches the plate after simultaneous removal of every uncertified contact.
/// This intentionally refuses an exemption when a node/root proof is unavailable.
pub(super) fn join_findings(scene: &Scene, limit: f64) -> Vec<Finding> {
    let parts = &scene.parts;
    let mut out = Vec::new();
    let mut edges = Vec::new();
    for (i, p) in parts.iter().enumerate().filter(|(_, p)| p.is_valid()) {
        for (j, q) in parts
            .iter()
            .enumerate()
            .skip(i + 1)
            .filter(|(_, q)| q.is_valid())
        {
            if accounted(p, q) {
                continue;
            }
            if let Some(f) = pair_finding(p, q, limit) {
                if matches!(f.fate, Fate::Wide { .. }) {
                    edges.push((i, j));
                }
                out.push(f);
            }
        }
    }
    // No weak contact means no exemption to prove. Keep ordinary measurements unchanged.
    if out.iter().all(|f| matches!(f.fate, Fate::Wide { .. }))
        || !parts.iter().all(|p| own_wide(scene, p, limit))
    {
        return out;
    }
    // Axial scans cover these joins in the ordinary report, but a graph edge still needs the
    // same union-of-two-written-parts width certificate, never just axial record ownership.
    for (i, p) in parts.iter().enumerate() {
        for (j, q) in parts.iter().enumerate().skip(i + 1) {
            if accounted(p, q)
                && pair_finding(p, q, limit).is_some_and(|f| matches!(f.fate, Fate::Wide { .. }))
            {
                edges.push((i, j));
            }
        }
    }
    let roots: Vec<bool> = parts.iter().map(|p| on_plate(scene, p)).collect();
    if rooted(&roots, &edges).iter().all(|r| *r) {
        for f in &mut out {
            f.incidental = !matches!(f.fate, Fate::Wide { .. });
        }
    }
    out
}

pub(super) fn contacts_json(scene: &Scene, findings: &[Finding], incidental_only: bool) -> Json {
    let number = |v: f64| {
        if v.is_finite() {
            Json::Num(v)
        } else {
            Json::Null
        }
    };
    Json::Arr(
        findings
            .iter()
            .filter_map(|f| {
                let partner = f.partner?;
                if incidental_only && !f.incidental {
                    return None;
                }
                Some(Json::obj([
                    (
                        "records",
                        Json::Arr(vec![Json::Uint(f.rec as u64), Json::Uint(partner as u64)]),
                    ),
                    (
                        "at_mm",
                        Json::Arr(scene.world(f.at).map(Json::Num).to_vec()),
                    ),
                    (
                        "location_method",
                        Json::str("surface-area centroid of the written contact region"),
                    ),
                    ("width_lower_mm", number(f.lb())),
                    ("width_upper_mm", number(f.ub())),
                    ("measurement", Json::str(f.fate.class())),
                    (
                        "classification",
                        Json::str(if f.incidental {
                            "redundant incidental contact"
                        } else {
                            "required join"
                        }),
                    ),
                    ("strength_credit", Json::Bool(false)),
                ]))
            })
            .collect(),
    )
}

#[cfg(test)]
mod round25;
#[cfg(test)]
mod tests;
