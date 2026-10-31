use super::*;
use crate::construct::tests::{builder, candidate};
use plinth_geom::Mesh;

fn tilted() -> Mesh {
    let (s, c) = 65_f64.to_radians().sin_cos();
    Mesh::new(
        vec![-30., 30., 30., -30.],
        vec![-30. * c, -30. * c, 30. * c, 30. * c],
        vec![-30. * s, -30. * s, 30. * s, 30. * s],
        vec![0, 2, 1, 0, 3, 2],
    )
    .unwrap()
}

#[test]
fn far_joint_side_reservation_keeps_the_full_build_gap() {
    let (s, c) = 65_f64.to_radians().sin_cos();
    let mut plan = builder(&tilted());
    let mut tip = candidate(0, 0.0);
    tip.axis = [0., s as f32, -c as f32];
    let mut out = Vec::new();
    plan.probe(&[tip], &mut out);
    plan.commit(QuoteBatch::from_results(out).unwrap()).unwrap();
    let plan = plan.into_routing();
    let head = &plan.contacts[0];
    let radius = distance(head.centre, head.joint)
        + f64::from(head.joint_radius.max(head.radius))
        + 2.0 * ENVELOPE;
    // A true 45-degree brace axis perpendicular to the contact axis. Its nearest
    // station is beyond the far side of the joint, not through the joint centre.
    let uz = std::f64::consts::FRAC_1_SQRT_2;
    let uy = c / s * uz;
    let u = [(0.5 - uy * uy).sqrt(), uy, uz];
    for (extra, blocked) in [(-0.001, true), (0.001, false)] {
        let at: [f64; 3] = std::array::from_fn(|k| {
            f64::from(head.centre[k]) + [0., s, -c][k] * (radius + 0.25 + 0.32 + extra)
        });
        let start = std::array::from_fn(|k| (at[k] - u[k]) as f32);
        let end = std::array::from_fn(|k| (at[k] + u[k]) as f32);
        let q = MemberQuote {
            plan: plan.id,
            rev: plan.rev,
            parts: vec![
                NativePart::frustum(
                    PartKind::Rod,
                    start,
                    end,
                    Section::new(0.25).unwrap(),
                    Section::new(0.25).unwrap(),
                )
                .unwrap(),
            ],
            column: None,
            brace: Some(BraceAnchor {
                from: 1,
                to: 2,
                from_z: start[2],
                to_z: end[2],
            }),
        };
        let result = plan.keep_heads(&q);
        assert_eq!(
            matches!(result, Err(Reject::RegionBlocked { by: CandId(0) })),
            blocked,
            "extra={extra}, {result:?}"
        );
    }
}

#[test]
fn another_chains_column_cannot_drop_through_a_lower_head() {
    let (s, c) = 65_f64.to_radians().sin_cos();
    let joint_y = s * 2.4;
    let mut xs = vec![-0.5, 0.5, 0.5, -0.5];
    let mut ys = vec![-0.5 * c, -0.5 * c, 0.5 * c, 0.5 * c];
    let mut zs = vec![-0.5 * s, -0.5 * s, 0.5 * s, 0.5 * s];
    xs.extend([-1., 1., 1., -1.]);
    ys.extend([joint_y - 1., joint_y - 1., joint_y + 1., joint_y + 1.]);
    zs.extend([8.; 4]);
    let mesh = Mesh::new(xs, ys, zs, vec![0, 2, 1, 0, 3, 2, 4, 6, 5, 4, 7, 6]).unwrap();
    let centre: [f64; 3] =
        std::array::from_fn(|k| (mesh.bounds().min[k] + mesh.bounds().max[k]) * 0.5);
    let mut tips = [candidate(0, 0.), candidate(1, 0.)];
    tips[0].site = std::array::from_fn(|k| (-centre[k]) as f32);
    tips[0].axis = [0., s as f32, -c as f32];
    // Align the upper column exactly with the rounded lower joint, including standoff.
    tips[1].site = [
        0.,
        (joint_y + s * f64::from(tips[0].standoff) - centre[1]) as f32,
        (8. - centre[2]) as f32,
    ];
    let mut plan = builder(&mesh);
    let mut out = Vec::new();
    plan.probe(&tips, &mut out);
    let ids = plan.commit(QuoteBatch::from_results(out).unwrap()).unwrap();
    let plan = plan.into_routing();
    let mut out = Vec::new();
    plan.probe_members(
        &[MemberProposal::Column {
            contact: ids[1],
            section: Section::new(0.25).unwrap(),
        }],
        &mut out,
    );
    assert!(
        matches!(out[0], Err(Reject::RegionBlocked { by: CandId(0) })),
        "{:?}",
        out[0].as_ref().err()
    );
}

fn lens_fixture(commit_c: bool, offset: f64) -> (PlanBuilder<Routed>, MemberProposal, ContactId) {
    let (s, c) = 65_f64.to_radians().sin_cos();
    let x = 6.0 * (s * s - c * c).sqrt();
    let xy = 2.0 * (x * x + (6.0 * c).powi(2)).sqrt();
    // Offset C perpendicular to A--B in XY; the lens regression uses 0.49 mm.
    let cx = -offset * (12.0 * c) / xy;
    let cy = offset * (2.0 * x) / xy;
    let mut plan = builder(&tilted());
    let tips: Vec<_> = [(0, cx, cy / c), (1, -x, -6.), (2, x, 6.)]
        .into_iter()
        .map(|(id, x, u)| {
            let mut tip = candidate(id, x as f32);
            tip.site = [x as f32, (u * c) as f32, (u * s) as f32];
            tip.axis = [0., s as f32, -c as f32];
            tip
        })
        .collect();
    let mut out = Vec::new();
    plan.probe(&tips, &mut out);
    let ids = plan.commit(QuoteBatch::from_results(out).unwrap()).unwrap();
    let mut plan = plan.into_routing();
    let columns: Vec<_> = ids
        .iter()
        .skip(usize::from(!commit_c))
        .map(|&contact| MemberProposal::Column {
            contact,
            section: Section::new(0.25).unwrap(),
        })
        .collect();
    let mut member_out = Vec::new();
    plan.probe_members(&columns, &mut member_out);
    let members = plan
        .commit_members(MemberQuoteBatch::from_results(member_out).unwrap())
        .unwrap();
    let a = plan.get(ids[1]).unwrap().joint();
    let b = plan.get(ids[2]).unwrap().joint();
    let brace = MemberProposal::Brace {
        from: members[usize::from(commit_c)],
        to: members[usize::from(commit_c) + 1],
        from_z: a[2] - 3.0,
        to_z: b[2] - 3.0,
        section: Section::new(0.25).unwrap(),
    };
    (plan, brace, ids[0])
}

#[test]
fn unrelated_offset_column_lens_is_rejected_in_probe_and_atomic_commit() {
    let (mut plan, brace, _) = lens_fixture(true, 0.49);
    let mut answers = Vec::new();
    plan.probe_members(&[brace], &mut answers);
    assert!(
        matches!(answers[0],Err(Reject::GapTooSmall {gap}) if gap<0.001),
        "{:?}",
        answers[0].as_ref().err()
    );
    // Private quote construction isolates the redundant commit guard: public
    // probe has already refused this member. A failed commit changes nothing.
    let q = plan.prepare_member(brace).unwrap();
    let before = (plan.rev(), plan.members.len());
    assert!(matches!(
        plan.commit_members(MemberQuoteBatch { quotes: vec![q] }),
        Err(Reject::GapTooSmall { .. })
    ));
    assert_eq!(before, (plan.rev(), plan.members.len()));
}

#[test]
fn unrelated_members_straddle_the_h6_build_gap() {
    for (offset, reject) in [(0.819, true), (0.821, false)] {
        let (mut plan, brace, _) = lens_fixture(true, offset);
        let mut out = Vec::new();
        plan.probe_members(&[brace], &mut out);
        if reject {
            assert!(
                matches!(out[0],Err(Reject::GapTooSmall {gap}) if (gap-0.319).abs()<0.0001),
                "{:?}",
                out[0].as_ref().err()
            );
        } else {
            plan.commit_members(MemberQuoteBatch::from_results(out).unwrap())
                .unwrap();
        }
    }
}

#[test]
fn same_batch_offset_lens_is_symmetric_and_separate_quotes_cannot_bypass_commit() {
    let (mut plan, brace, contact) = lens_fixture(false, 0.49);
    let column = MemberProposal::Column {
        contact,
        section: Section::new(0.25).unwrap(),
    };
    for proposals in [[column, brace], [brace, column]] {
        let mut out = Vec::new();
        plan.probe_members(&proposals, &mut out);
        assert_eq!(out.len(), 2);
        assert!(
            out.iter()
                .all(|q| matches!(q,Err(Reject::GapTooSmall {gap}) if *gap<0.001)),
            "{:?}",
            out.iter().map(|q| q.as_ref().err()).collect::<Vec<_>>()
        );
    }
    let mut qs = Vec::new();
    for proposal in [column, brace] {
        let mut out = Vec::new();
        plan.probe_members(&[proposal], &mut out);
        qs.push(out.remove(0).expect("each member alone is legal"));
    }
    let before = (plan.rev(), plan.members.len());
    assert!(matches!(
        plan.commit_members(MemberQuoteBatch { quotes: qs }),
        Err(Reject::GapTooSmall { .. })
    ));
    assert_eq!(before, (plan.rev(), plan.members.len()));
}
