//! Private automatic bracing. Failures here consume alternatives, never sites.
use super::*;
use std::collections::BTreeSet;

pub(super) fn schedule(plan: &PlanBuilder<Routed>) -> Result<Vec<MemberProposal>, Reject> {
    let section = Section::new(0.25)?;
    let mut proposals = Vec::new();
    let mut seen = BTreeSet::new();
    for (i, a) in plan.columns.iter().enumerate() {
        if f64::from(a.top[2] - a.bottom[2]) <= crate::route::BRACE_FREE_LENGTH_MM {
            continue;
        }
        let mut partners: Vec<_> = plan
            .columns
            .iter()
            .enumerate()
            .filter(|(j, b)| *j != i && (4.0..=15.0).contains(&xy_distance(a, b)))
            .collect();
        partners.sort_by(|(_, a1), (_, b1)| {
            xy_distance(a, a1)
                .total_cmp(&xy_distance(a, b1))
                .then(a1.contact.cmp(&b1.contact))
        });
        for (j, b) in partners {
            if !seen.insert((i.min(j), i.max(j))) {
                continue;
            }
            let distance = xy_distance(a, b) as f32;
            let top = a.top[2].min(b.top[2]);
            let from = MemberId {
                plan: plan.id,
                ix: a.member,
            };
            let to = MemberId {
                plan: plan.id,
                ix: b.member,
            };
            let mut lo = plan.model.plate + 4.2;
            while lo + distance <= top {
                let hi = lo + distance;
                proposals.extend([
                    MemberProposal::Brace {
                        from,
                        to,
                        from_z: lo,
                        to_z: hi,
                        section,
                    },
                    MemberProposal::Brace {
                        from,
                        to,
                        from_z: hi,
                        to_z: lo,
                        section,
                    },
                ]);
                lo += 4.0;
            }
        }
    }
    // Kernel batches, not a model query per pair. Each pair stays in one batch.
    let mut accepted: Vec<MemberQuote> = Vec::new();
    let mut nodes = Vec::new();
    let mut braced = BTreeSet::new();
    let mut selected = Vec::new();
    for batch in proposals.chunks(8192) {
        let mut answers = Vec::new();
        plan.measure_members(batch, &mut answers, true);
        let mut answers = answers.into_iter();
        for pair in batch.chunks_exact(2) {
            let (Some(Ok(a)), Some(Ok(b))) = (answers.next(), answers.next()) else {
                continue;
            };
            let (Some(ba), Some(bb)) = (a.brace, b.brace) else {
                continue;
            };
            if braced.contains(&ba.from) && braced.contains(&ba.to) {
                continue;
            }
            let start = nodes.len();
            nodes.extend([ba, bb]);
            let fits = plan.check_node_spacing(&nodes).is_ok()
                && accepted.iter().all(|old| {
                    [&a, &b].into_iter().all(|q| {
                        old.parts
                            .iter()
                            .enumerate()
                            .all(|(k, p)| keep_member(q, p, old.role(k), plan.limits.gap).is_ok())
                    })
                })
                && b.parts
                    .iter()
                    .enumerate()
                    .all(|(k, p)| keep_member(&a, p, b.role(k), plan.limits.gap).is_ok());
            if !fits {
                nodes.truncate(start);
                continue;
            }
            braced.extend([ba.from, ba.to]);
            accepted.extend([a, b]);
            selected.extend_from_slice(pair);
        }
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construct::tests::{builder, candidate, plate};

    #[test]
    fn blocked_nearest_partner_tries_the_next_and_shares_node_heights() {
        let mut plan = builder(&plate());
        // Keep the intermediate column clear of the near-tangent base band.
        let points = [[0., 0.], [4.6, 0.], [0., 6.], [2.3, 0.]];
        let candidates: Vec<_> = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mut c = candidate(i as u32, p[0]);
                c.site[1] = p[1];
                c
            })
            .collect();
        let mut out = Vec::new();
        plan.probe(&candidates, &mut out);
        let contacts = plan.commit(QuoteBatch::from_results(out).unwrap()).unwrap();
        let mut plan = plan.into_routing();
        // Private policy fixture: give ordinary planar heads long columns. No
        // public pose mutation or model-clearance certificate is claimed here.
        plan.model.plate -= 20.0;
        let columns: Vec<_> = contacts
            .into_iter()
            .map(|contact| MemberProposal::Column {
                contact,
                section: Section::new(0.25).unwrap(),
            })
            .collect();
        let mut out = Vec::new();
        plan.probe_members(&columns, &mut out);
        let ids = plan
            .commit_members(MemberQuoteBatch::from_results(out).unwrap())
            .unwrap();
        let proposals = plan.brace_proposals().unwrap();
        let pair = |a, b| {
            proposals.iter().any(|p| {
                matches!(p,
            MemberProposal::Brace { from, to, .. } if
            (*from == ids[a] && *to == ids[b]) || (*from == ids[b] && *to == ids[a]))
            })
        };
        assert!(!pair(0, 1), "nearer pair crosses the column at x=2.3");
        assert!(pair(0, 2), "the next partner must be tried after collision");
        let mut out = Vec::new();
        plan.probe_members(&proposals, &mut out);
        assert!(
            out.iter().all(Result::is_ok),
            "{:?}",
            out.iter().map(|q| q.as_ref().err()).collect::<Vec<_>>()
        );
        plan.commit_members(MemberQuoteBatch::from_results(out).unwrap())
            .unwrap();
        assert!(plan.check_node_spacing(&[]).is_ok());
        plan.raft_connected().unwrap();
        let frozen = plan.freeze().unwrap();
        assert!(!frozen.unbraced_columns().contains(&0));
    }
}
