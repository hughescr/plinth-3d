use super::*;
use plinth_geom::Mesh;

pub(super) fn plate() -> Mesh {
    Mesh::new(
        vec![-30., 30., 30., -30.],
        vec![-30., -30., 30., 30.],
        vec![0.; 4],
        vec![0, 2, 1, 0, 3, 2],
    )
    .unwrap()
}
pub(super) fn builder(mesh: &Mesh) -> PlanBuilder<Contacts> {
    let limits = Limits::current().unwrap();
    let keep = KeepUpRecord {
        rotation_x_deg: 115.,
        approved_sha256: "0".repeat(64),
    };
    PlanBuilder::new(
        PosedModel::new(mesh, &keep, &limits).unwrap(),
        limits,
        ProcessRecord {
            layer_height_mm: 0.05,
            layer_origin_mm: 0.0,
        },
    )
}
pub(super) fn candidate(id: u32, x: f32) -> ContactCandidate {
    ContactCandidate {
        id: CandId(id),
        site: [x, 0., 0.],
        ball_r: 0.212,
        standoff: 0.0772,
        axis: [0., 0., -1.],
        neck: 0.2,
        joint_r: 0.3,
        head_len: 2.4,
    }
}
fn quotes(p: &PlanBuilder<Contacts>, c: &[ContactCandidate]) -> Vec<Result<Quote, Reject>> {
    let mut out = Vec::new();
    p.probe(c, &mut out);
    out
}

pub(super) fn rejection_counts(
    errors: &[Reject],
) -> std::collections::BTreeMap<&'static str, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for error in errors {
        let name = match error {
            Reject::SecondSheet => "SecondSheet",
            Reject::PatchUnverified => "PatchUnverified",
            Reject::ResourceExhausted => "ResourceExhausted",
            Reject::MeasureExhausted => "MeasureExhausted",
            Reject::NarrowJoin => "NarrowJoin",
            Reject::PrintOrder { .. } => "PrintOrder",
            Reject::NotBuilt => "NotBuilt",
            Reject::RaftTouchesModel => "RaftTouchesModel",
            Reject::GapTooSmall { .. } => "GapTooSmall",
            other => panic!("unexpected rejection in targeted witness: {other:?}"),
        };
        *counts.entry(name).or_insert(0) += 1;
    }
    counts
}

#[test]
fn rejection_counts_measure_multiplicity_and_do_not_invent_hits() {
    let observed = [
        Reject::GapTooSmall { gap: 0.1 },
        Reject::GapTooSmall { gap: 0.2 },
        Reject::ResourceExhausted,
    ];
    let counts = rejection_counts(&observed);
    assert_eq!(counts.get("GapTooSmall"), Some(&2));
    assert_eq!(counts.get("ResourceExhausted"), Some(&1));
    assert!(!counts.contains_key("NotBuilt"));
    assert_eq!(counts.values().sum::<usize>(), observed.len());
}

#[test]
fn fixed_contact_capacity_is_not_analytic_capacity() {
    let p = builder(&plate());
    let mut out = quotes(&p, &[candidate(0, 0.)]);
    let q = out.remove(0).unwrap();
    let analytic = std::f64::consts::PI * (0.212f64.powi(2) - 0.0772f64.powi(2));
    assert!(q.full_capacity() < 12.0 * analytic);
    assert_eq!(
        require_capacity(&q, 12.0 * analytic),
        Err(Reject::InsufficientCapacity)
    );
    assert!(require_capacity(&q, q.full_capacity()).is_ok());
    eprintln!(
        "certified full-patch capacity={} mm2 peel; analytic-sized ratio={}",
        q.full_capacity(),
        12.0 * analytic / (q.full_capacity() / 12.0)
    );
}
#[test]
fn atomic_commit_stale_foreign_and_intrabatch_conflicts() {
    let mut p = builder(&plate());
    let q = QuoteBatch::from_results(quotes(&p, &[candidate(0, -10.)])).unwrap();
    let stale = QuoteBatch::from_results(quotes(&p, &[candidate(1, 10.)])).unwrap();
    let ids = p.commit(q).unwrap();
    assert_eq!(p.commit(stale).unwrap_err(), Reject::Stale);
    assert_eq!(p.contact_count(), 1);
    let mut foreign = builder(&plate());
    assert_eq!(foreign.get(ids[0]).unwrap_err(), Reject::ForeignHandle);
    let q = QuoteBatch::from_results(quotes(&p, &[candidate(2, 10.)])).unwrap();
    assert_eq!(foreign.commit(q).unwrap_err(), Reject::ForeignHandle);
    assert_eq!(foreign.contact_count(), 0);
    let fused = quotes(&foreign, &[candidate(0, 0.), candidate(1, 0.001)]);
    assert!(
        fused
            .iter()
            .all(|r| matches!(r, Err(Reject::WouldFuse { .. })))
    );
    let blocked = quotes(&foreign, &[candidate(0, 0.), candidate(1, 1.)]);
    assert!(
        blocked
            .iter()
            .all(|r| matches!(r, Err(Reject::RegionBlocked { .. })))
    );
}
#[test]
fn tip_family_radius_bound_is_inclusive_and_native_field_exact() {
    let p = builder(&plate());
    let mut c = candidate(0, 11.0);
    c.ball_r = 0.42;
    c.standoff = 0.24;
    assert!(quotes(&p, &[c])[0].is_ok());
    c.ball_r = f32::from_bits(c.ball_r.to_bits() + 1);
    assert!(matches!(quotes(&p, &[c])[0], Err(Reject::UnknownShape)));
}

#[test]
fn rounded_head_reach_includes_standoff_from_the_contact_site() {
    let p = builder(&plate());
    for (standoff, accepted) in [(0.249, true), (0.251, false)] {
        let mut c = candidate(0, 11.0);
        c.ball_r = 0.42;
        c.standoff = standoff;
        let out = quotes(&p, &[c]);
        assert_eq!(out[0].is_ok(), accepted, "{out:?}");
        if let Ok(q) = &out[0] {
            for part in q.contact.parts() {
                for vertex in part.mesh().iter().flatten() {
                    assert!(native::distance(*vertex, c.site) <= 2.95);
                }
            }
        } else {
            assert!(matches!(out[0], Err(Reject::UnknownShape)), "{out:?}");
        }
    }
}

#[test]
fn in_family_cone_tail_beyond_contact_region_is_not_exempted() {
    let p = builder(&plate());
    let mut c = candidate(0, 0.0);
    c.ball_r = 0.42;
    c.standoff = 0.39;
    c.head_len = 2.64;
    // The previous ball-centred bound passed (2.64 + 0.30 < 2.95), although
    // even the cone's end CENTRE lies 3.03 mm from the contact site. Such a
    // tail may not use the local >0.02 mm clearance exemption. §5.4's complete
    // head maximum refuses it outright, even if it could meet a 0.32 mm gap.
    assert!(c.head_len + c.joint_r < 2.95);
    assert!(c.standoff + c.head_len > 3.0);
    let out = quotes(&p, &[c]);
    assert!(matches!(out[0], Err(Reject::UnknownShape)), "{out:?}");
}

#[test]
fn joint_second_sheet_is_gap_too_small_before_commit() {
    let z = -2.4 - 0.0772 - 0.3 - 0.1;
    let mesh = Mesh::new(
        vec![-30., 30., 30., -30., -30., 30., 30., -30.],
        vec![-30., -30., 30., 30., -30., -30., 30., 30.],
        vec![0., 0., 0., 0., z, z, z, z],
        vec![0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7],
    )
    .unwrap();
    let p = builder(&mesh);
    let mut c = candidate(0, 0.);
    c.site[2] = p.model.mesh().bounds().max[2] as f32;
    let out = quotes(&p, &[c]);
    assert!(
        matches!(&out[0], Err(Reject::GapTooSmall { gap }) if *gap > 0.099 && *gap < 0.101),
        "{out:?}"
    );
    assert_eq!(p.contact_count(), 0);
}
#[test]
fn joint_gap_straddles_h6_build_reserve() {
    for gap in [0.31f32, 0.33] {
        let mut p = builder(&plate());
        let mut c = candidate(0, 0.);
        c.head_len = 1.5;
        c.joint_r = c.standoff + c.head_len - gap;
        let out = quotes(&p, &[c]);
        if gap < 0.32 {
            assert!(
                matches!(&out[0], Err(Reject::GapTooSmall { gap })
                if (gap - 0.31).abs() < 0.0001),
                "gap rejection missing: {:?}",
                out[0].as_ref().err()
            );
        } else {
            p.commit(QuoteBatch::from_results(out).unwrap()).unwrap();
            assert_eq!(p.contact_count(), 1);
        }
    }
}

#[test]
fn geometric_pole_cover_rejects_a_downward_but_shallow_head() {
    let p = builder(&plate());
    let mut c = candidate(0, 0.0);
    c.axis = [0.96, 0.0, -0.28];
    assert!(c.axis[2] < -0.01);
    let out = quotes(&p, &[c]);
    assert!(matches!(out[0], Err(Reject::PoleUncovered)), "{out:?}");
}

#[test]
fn departure_certifies_a_concave_recess() {
    // A connected stepped recess. Distant wall and apron triangles need not lie
    // behind a global supporting plane; the local all-triangle certificate suffices.
    let recess = Mesh::new(
        vec![-30., -30., 5., 5., 5., 5., 30., 30.],
        vec![-30., 30., -30., 30., -30., 30., -30., 30.],
        vec![0., 0., 0., 0., -1., -1., -1., -1.],
        vec![0, 1, 2, 1, 3, 2, 2, 3, 4, 3, 5, 4, 4, 5, 6, 5, 7, 6],
    )
    .unwrap();
    let p = builder(&recess);
    let mut c = candidate(0, 0.);
    c.site[2] = 0.5; // Frozen local plane, after bounding-box centring.
    let recess_result = quotes(&p, &[c]).remove(0);
    assert!(recess_result.is_ok(), "{:?}", recess_result.as_ref().err());
}

#[test]
fn departure_certifies_a_nearby_boundary_edge() {
    let p = builder(&plate());
    // Finite edges are measured, not forbidden by a single-flat-face restriction.
    let boundary_result = quotes(&p, &[candidate(0, 29.8)]).remove(0);
    assert!(
        boundary_result.is_ok(),
        "{:?}",
        boundary_result.as_ref().err()
    );
}

#[test]
fn departure_rejects_reversed_winding_on_a_thick_solid() {
    // Closed thick box, with all winding reversed. On its top face the inferred
    // normal points INTO the solid. Surface gaps alone cannot detect containment.
    let inward = Mesh::new(
        vec![-30., 30., 30., -30., -30., 30., 30., -30.],
        vec![-30., -30., 30., 30., -30., -30., 30., 30.],
        vec![-8., -8., -8., -8., 0., 0., 0., 0.],
        vec![
            0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6, 0, 5, 1, 0, 4, 5, 1, 6, 2, 1, 5, 6, 2, 7, 3, 2, 6,
            7, 3, 4, 0, 3, 7, 4,
        ],
    )
    .unwrap();
    let p = builder(&inward);
    let mut c = candidate(0, 0.);
    c.site[2] = 4.;
    let inward_result = quotes(&p, &[c]).remove(0);
    assert!(
        matches!(inward_result, Err(Reject::DepartureNotMonotone)),
        "{inward_result:?}"
    );
}

#[test]
fn deterministic_h1_reserve_window_is_rejected() {
    let p = builder(&plate());
    let mut c = candidate(0, 0.);
    c.ball_r = (0.49 / std::f64::consts::PI + f64::from(c.standoff).powi(2)).sqrt() as f32;
    let out = quotes(&p, &[c]);
    assert!(
        matches!(out[0], Err(Reject::AreaOverLimit { hi, max })
        if hi > 0.48 && hi < 0.50 && max.to_bits() == 0.48f64.to_bits()),
        "{out:?}"
    );
}

#[test]
fn simultaneous_conflicts_straddle_h2_margin_in_both_orders() {
    let a = candidate(0, 0.);
    let mut measured = std::collections::BTreeMap::new();
    for (gap, fused) in [(0.0119, true), (0.0121, false)] {
        let b = candidate(1, 2.0 * a.ball_r + 2e-5 + gap);
        for reverse in [false, true] {
            let mut p = builder(&plate());
            let inputs = if reverse { [b, a] } else { [a, b] };
            let out = quotes(&p, &inputs);
            for result in out {
                let path = match result {
                    Err(Reject::WouldFuse { .. }) if fused => "patch-envelope",
                    Err(Reject::RegionBlocked { .. }) if !fused => "head-reservation",
                    other => panic!("gap={gap}, reverse={reverse}: {other:?}"),
                };
                *measured.entry(path).or_insert(0) += 1;
            }
            // Independently probed siblings still conflict at atomic commit.
            let qa = quotes(&p, &[a]).remove(0).unwrap();
            let qb = quotes(&p, &[b]).remove(0).unwrap();
            let batch = QuoteBatch {
                quotes: vec![qa, qb],
            };
            // The quotes belong to p; a foreign-plan refusal would be vacuous.
            let result = p.commit(batch);
            assert_eq!(matches!(result, Err(Reject::WouldFuse { .. })), fused);
            if !fused {
                assert!(matches!(result, Err(Reject::RegionBlocked { .. })));
            }
            assert_eq!(p.contact_count(), 0);
        }
    }
    assert_eq!(measured.get("patch-envelope"), Some(&4));
    assert_eq!(measured.get("head-reservation"), Some(&4));
    eprintln!(
        "H2 boundary path hits: {measured:?}; near-margin admitted pairs=0 (head reservations forbid them)"
    );
}

#[test]
fn construction_guards_targeted_and_measured() {
    let p = builder(&plate());
    let base = candidate(0, 0.);
    let mut cases = Vec::new();
    let mut c = base;
    c.ball_r = 0.41; // In-family, so this still witnesses H1, not the radius guard.
    c.standoff = 0.;
    cases.push((c, "area"));
    let mut c = base;
    c.neck = 0.05;
    cases.push((c, "thin"));
    let mut c = base;
    c.ball_r = f32::NAN;
    cases.push((c, "finite"));
    let mut c = base;
    c.head_len = 3.;
    cases.push((c, "shape"));
    let mut c = base;
    c.axis = [1., 0., 0.];
    cases.push((c, "pole"));
    let mut c = base;
    c.standoff = 0.01;
    cases.push((c, "head"));
    let mut c = base;
    c.axis = [0.1, 0., -(0.99f32).sqrt()];
    cases.push((c, "departure"));
    let inputs: Vec<_> = cases.iter().map(|(c, _)| *c).collect();
    let out = quotes(&p, &inputs);
    let mut counts = std::collections::BTreeMap::new();
    for ((_, name), r) in cases.into_iter().zip(out) {
        let hit = matches!(
            (name, &r),
            ("area", Err(Reject::AreaOverLimit { .. }))
                | ("thin", Err(Reject::TooThin { .. }))
                | ("finite", Err(Reject::NonFinite))
                | ("shape", Err(Reject::UnknownShape))
                | ("pole", Err(Reject::PoleUncovered))
                | ("head", Err(Reject::HeadTouchesModel))
                | ("departure", Err(Reject::DepartureNotMonotone))
        );
        assert!(hit, "{name}: {r:?}");
        *counts.entry(name).or_insert(0) += 1;
    }
    assert!(counts.values().all(|&n| n > 0));
    eprintln!("production rejection hits: {counts:?}");
    assert!(Section::new(0.052).is_ok());
    assert!(Section::new(0.050).is_err());
}
#[test]
fn fixed_seed_64_admissions_have_no_oversized_or_fused_patches() {
    let mut seed = 0x7353cafeu64;
    let mut admitted = 0;
    let mut oracle_bad = 0;
    for i in 0..64 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let radius = 0.10 + (seed >> 32) as f64 / u32::MAX as f64 * 0.38;
        let mut c = candidate(i, 0.);
        c.ball_r = radius as f32;
        c.neck = 0.08;
        c.standoff = (radius * 0.4) as f32;
        let p = builder(&plate());
        let mut out = quotes(&p, &[c]);
        if let Ok(q) = out.remove(0) {
            admitted += 1;
            let r = f64::from(c.ball_r);
            let s = f64::from(c.standoff);
            let truth = std::f64::consts::PI * (r * r - s * s);
            if truth > 0.48 || q.contact.patch().upper() < truth {
                oracle_bad += 1;
            }
        }
    }
    assert!(admitted > 0);
    assert_eq!(oracle_bad, 0);
    eprintln!(
        "64 fixed-seed planar cases; admitted={admitted}; analytic oracle H1 violations={oracle_bad}; pairs not exercised by this property"
    );
}
#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "test-only slot allocation from scripts/t"
)]
fn threads_probe_bytes_equal_at_one_and_slot_allocation() {
    let workers = std::env::var("NEXTEST_TEST_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1);
    let mesh = Mesh::new(
        vec![-90., 90., 90., -90.],
        vec![-90., -90., 90., 90.],
        vec![0.; 4],
        vec![0, 2, 1, 0, 3, 2],
    )
    .unwrap();
    let inputs: Vec<_> = (0..128)
        .map(|i| {
            let mut c = candidate(i, (i % 16) as f32 * 7.0 - 52.5);
            c.site[1] = (i / 16) as f32 * 7.0 - 24.5;
            c
        })
        .collect();
    plinth_geom::counters::reset();
    let serial = quotes(&builder(&mesh), &inputs);
    let parallel = quotes(&builder(&mesh).with_workers(workers).unwrap(), &inputs);
    let report = plinth_geom::counters::take_par().unwrap();
    assert_eq!(report.threads, workers.min(4));
    let words = |q: &Quote| {
        q.contact
            .parts()
            .iter()
            .map(NativePart::field_words)
            .collect::<Vec<_>>()
    };
    let mut admitted = 0;
    for (a, b) in serial.iter().zip(&parallel) {
        match (a, b) {
            (Ok(a), Ok(b)) => {
                admitted += 1;
                assert_eq!(words(a), words(b));
                assert_eq!(
                    a.contact.patch().lower().to_bits(),
                    b.contact.patch().lower().to_bits()
                );
                assert_eq!(
                    a.contact.patch().upper().to_bits(),
                    b.contact.patch().upper().to_bits()
                );
            }
            (Err(a), Err(b)) => assert_eq!(a, b),
            _ => panic!("thread-dependent admission"),
        }
    }
    assert_eq!(admitted, 128);
    let mut fused = 0;
    let mut pairs = 0;
    for (i, a) in parallel.iter().enumerate() {
        let a = a.as_ref().unwrap();
        for b in &parallel[..i] {
            let b = b.as_ref().unwrap();
            pairs += 1;
            let patch_radius = |q: &Quote| {
                let radius = f64::from(q.contact.radius) + 1e-5;
                let stand = f64::from(q.contact.centre[2]);
                (radius * radius - stand * stand).sqrt()
            };
            let axy = a.contact.centre;
            let bxy = b.contact.centre;
            let distance =
                (f64::from(axy[0] - bxy[0]).powi(2) + f64::from(axy[1] - bxy[1]).powi(2)).sqrt();
            if distance - patch_radius(a) - patch_radius(b) <= 0.010 {
                fused += 1;
            }
        }
    }
    assert_eq!(fused, 0);
    assert_eq!(pairs, 8128);
    for op in [
        plinth_geom::counters::Op::BallPatches,
        plinth_geom::counters::Op::SolidGaps,
    ] {
        let c = plinth_geom::counters::snapshot(op);
        assert_eq!(c.calls, 2);
        assert_eq!(c.items, 512);
    }
    eprintln!(
        "independent planar patch-envelope oracle: {pairs} admitted pairs, fused={fused}; wide-spacing control, not an H2 boundary witness"
    );
    eprintln!(
        "probe path witness: 128 admitted fixed contacts; serial/slot field and vertex digest mismatches=0; tasks={} threads={}",
        report.tasks, report.threads
    );
}

#[test]
fn fixed_tilted_heads_and_rooted_x_braces_freeze() {
    let angle = 65f64.to_radians();
    let (s, c) = angle.sin_cos();
    let mesh = Mesh::new(
        vec![-30., 30., 30., -30.],
        vec![-30. * c, -30. * c, 30. * c, 30. * c],
        vec![-30. * s, -30. * s, 30. * s, 30. * s],
        vec![0, 2, 1, 0, 3, 2],
    )
    .unwrap();
    let mut p = builder(&mesh);
    let inputs: Vec<_> = [-3.1f32, 3.1]
        .into_iter()
        .enumerate()
        .map(|(i, x)| {
            let mut tip = candidate(i as u32, x);
            tip.site = [x, (10. * c) as f32, (10. * s) as f32];
            tip.axis = [0., s as f32, -c as f32];
            tip
        })
        .collect();
    let contacts = p
        .commit(QuoteBatch::from_results(quotes(&p, &inputs)).unwrap())
        .unwrap();
    let mut p = p.into_routing();
    let proposals: Vec<_> = contacts
        .into_iter()
        .map(|contact| MemberProposal::Column {
            contact,
            section: Section::new(0.25).unwrap(),
        })
        .collect();
    let mut out = Vec::new();
    p.probe_members(&proposals, &mut out);
    p.commit_members(MemberQuoteBatch::from_results(out).unwrap())
        .unwrap();
    let braces = p.brace_proposals().unwrap();
    assert_eq!(braces.len(), 2);
    let mut out = Vec::new();
    p.probe_members(&braces, &mut out);
    p.commit_members(MemberQuoteBatch::from_results(out).unwrap())
        .unwrap();
    p.raft_connected().unwrap();
    let frozen = p.freeze().unwrap();
    assert_eq!(frozen.contacts().len(), 2);
    eprintln!(
        "tilted fixed fixture: 2 admitted heads, 2 rooted columns, 2 native 45-degree X braces, 1 connected raft"
    );
}

#[test]
fn brace_between_a_and_b_cannot_cut_a_third_chains_joint() {
    let (s, c) = 65f64.to_radians().sin_cos();
    let mesh = Mesh::new(
        vec![-30., 30., 30., -30.],
        vec![-30. * c, -30. * c, 30. * c, 30. * c],
        vec![-30. * s, -30. * s, 30. * s, 30. * s],
        vec![0, 2, 1, 0, 3, 2],
    )
    .unwrap();
    let mut p = builder(&mesh);
    let x = (4.3 * (s * s - c * c).sqrt()) as f32;
    // C sorts first so the rejection proves the unrelated chain was checked,
    // not merely a conservative reservation at a brace's carrier endpoint.
    let tips: Vec<_> = [(0, 0., 0.), (1, -x, -4.3), (2, x, 4.3)]
        .into_iter()
        .map(|(id, x, u)| {
            let mut tip = candidate(id, x);
            tip.site = [x, (u * c) as f32, (u * s) as f32];
            tip.axis = [0., s as f32, -c as f32];
            tip
        })
        .collect();
    let contacts = p
        .commit(QuoteBatch::from_results(quotes(&p, &tips)).unwrap())
        .unwrap();
    let joint = p.get(contacts[0]).unwrap().joint;
    let mut p = p.into_routing();
    let columns: Vec<_> = contacts
        .iter()
        .map(|&contact| MemberProposal::Column {
            contact,
            section: Section::new(0.25).unwrap(),
        })
        .collect();
    let mut out = Vec::new();
    p.probe_members(&columns, &mut out);
    let ids = p
        .commit_members(MemberQuoteBatch::from_results(out).unwrap())
        .unwrap();
    let a = p.get(contacts[1]).unwrap().joint;
    let b = p.get(contacts[2]).unwrap().joint;
    let brace = MemberProposal::Brace {
        from: ids[1],
        to: ids[2],
        from_z: a[2] - 0.0002,
        to_z: b[2] - 0.0002,
        section: Section::new(0.25).unwrap(),
    };
    let mid_z = (a[2] + b[2]) * 0.5 - 0.0002;
    assert!((mid_z - joint[2]).abs() < 0.0003);
    let before = (p.rev(), p.members.len());
    let mut out = Vec::new();
    p.probe_members(&[brace], &mut out);
    assert!(
        matches!(out[0], Err(Reject::RegionBlocked { by: CandId(0) })),
        "third-chain rejection missing: {:?}",
        out[0].as_ref().err()
    );
    assert_eq!((p.rev(), p.members.len()), before);
}

#[test]
fn fixed_angle_controls_admit_only_certified_head_approaches() {
    let mut admitted = 0;
    for angle in [0f64, 30., 45., 65., 90.] {
        let (s, c) = angle.to_radians().sin_cos();
        let mesh = Mesh::new(
            vec![-30., 30., 30., -30.],
            vec![-30. * c, -30. * c, 30. * c, 30. * c],
            vec![-30. * s, -30. * s, 30. * s, 30. * s],
            vec![0, 2, 1, 0, 3, 2],
        )
        .unwrap();
        let p = builder(&mesh);
        let mut tip = candidate(0, 0.);
        tip.axis = [0., s as f32, -c as f32];
        let result = quotes(&p, &[tip]).remove(0);
        if angle < 90. {
            let q = result.unwrap();
            admitted += 1;
            assert!(q.contact.patch().upper() <= 0.48);
            let joint = &q.contact.parts()[2];
            if let PartFields::Ball { start, end, radius } = joint.fields() {
                let centre =
                    std::array::from_fn::<_, 3, _>(|i| f64::from((start[i] + end[i]) * 0.5));
                let gap = centre[1] * s - centre[2] * c - f64::from(radius);
                assert!(gap >= 0.32, "plane oracle joint gap={gap}");
            }
        } else {
            assert!(matches!(result, Err(Reject::PoleUncovered)));
        }
    }
    assert_eq!(admitted, 4);
    eprintln!(
        "angle controls: 4 admitted normal-departure heads at 0/30/45/65 deg; 1 horizontal-axis rejection at 90 deg; plane-oracle joint gap violations=0"
    );
}

#[test]
fn thin_wall_second_sheet_on_model_side_is_not_a_departure_failure() {
    let mesh = Mesh::new(
        vec![-30., 30., 30., -30., -30., 30., 30., -30.],
        vec![-30., -30., 30., 30., -30., -30., 30., 30.],
        vec![-0.15, -0.15, -0.15, -0.15, 0.15, 0.15, 0.15, 0.15],
        vec![0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7],
    )
    .unwrap();
    let p = builder(&mesh);
    let mut tip = candidate(0, 0.);
    tip.site[2] = -0.15;
    tip.ball_r = 0.34;
    tip.standoff = 0.03;
    // Both planar intersections fit under H1 together: it is sheet multiplicity,
    // not area or departure, that must refuse this otherwise clear head.
    let r = f64::from(tip.ball_r);
    let stand = f64::from(tip.standoff);
    let truth = std::f64::consts::PI * (2.0 * r * r - stand * stand - (stand + 0.3).powi(2));
    assert!(truth > 0.0 && truth < 0.48);
    let out = quotes(&p, &[tip]);
    assert!(matches!(out[0], Err(Reject::SecondSheet)));
}

#[test]
fn second_sheet_measurement_exhaustion_and_resource_guards_fire() {
    let mut mesh = plate();
    let mut zs = vec![0.; 4];
    zs.extend([-0.3; 4]);
    let mut xs = mesh.xs().to_vec();
    xs.extend(mesh.xs());
    let mut ys = mesh.ys().to_vec();
    ys.extend(mesh.ys());
    mesh = Mesh::new(xs, ys, zs, vec![0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7]).unwrap();
    let p = builder(&mesh);
    let mut tip = candidate(0, 0.);
    tip.site[2] = 0.15;
    tip.ball_r = 0.4;
    tip.standoff = 0.1;
    let mut errors = vec![quotes(&p, &[tip]).remove(0).unwrap_err()];
    let p = builder(&plate());
    let mut tip = candidate(0, 0.);
    tip.site[2] = 1.0;
    errors.push(quotes(&p, &[tip]).remove(0).unwrap_err());
    errors.extend(
        quotes(&p, &vec![candidate(0, 0.); 4097])
            .into_iter()
            .map(Result::unwrap_err),
    );
    let mesh = Mesh::new(
        vec![-30., 30., 30., -30., 0.],
        vec![-30., -30., 30., 30., 0.],
        vec![0.; 5],
        vec![0, 2, 1, 0, 3, 2, 4, 4, 4],
    )
    .unwrap();
    let p = builder(&mesh);
    errors.push(quotes(&p, &[candidate(0, 0.)]).remove(0).unwrap_err());
    let counts = rejection_counts(&errors);
    assert_eq!(counts.get("SecondSheet"), Some(&1));
    assert_eq!(counts.get("PatchUnverified"), Some(&1));
    assert_eq!(counts.get("ResourceExhausted"), Some(&4097));
    assert_eq!(counts.get("MeasureExhausted"), Some(&1));
    eprintln!("additional production reject hits: {counts:?}");
}

#[test]
fn rooted_braces_crossing_a_v_bottom_hit_member_gap_rejection() {
    // Two outward-facing supporting planes, z = 5*abs(x). Both heads and
    // columns are outside the model. The X-pair cuts across the V surfaces.
    let mesh = Mesh::new(
        vec![-4., -4., 0., 0., 4., 4.],
        vec![-30., 30., -30., 30., -30., 30.],
        vec![20., 20., 0., 0., 20., 20.],
        vec![0, 1, 2, 1, 3, 2, 2, 3, 4, 3, 5, 4],
    )
    .unwrap();
    let mut p = builder(&mesh);
    let axis_z = -1.0 / 26.0f32.sqrt();
    let tips: Vec<_> = [-3.0f32, 3.0]
        .into_iter()
        .enumerate()
        .map(|(i, x)| {
            let mut c = candidate(i as u32, x);
            c.site[2] = 5.0;
            c.axis = [x.signum() * 5.0 / 26.0f32.sqrt(), 0.0, axis_z];
            c.neck = 0.22;
            c
        })
        .collect();
    let contacts = p
        .commit(QuoteBatch::from_results(quotes(&p, &tips)).unwrap())
        .unwrap();
    let mut p = p.into_routing();
    let columns: Vec<_> = contacts
        .into_iter()
        .map(|contact| MemberProposal::Column {
            contact,
            section: Section::new(0.25).unwrap(),
        })
        .collect();
    let mut out = Vec::new();
    p.probe_members(&columns, &mut out);
    p.commit_members(MemberQuoteBatch::from_results(out).unwrap())
        .unwrap();
    let braces = p.brace_proposals().unwrap();
    assert_eq!(braces.len(), 2);
    let before = (p.rev(), p.members.len());
    let mut out = Vec::new();
    p.probe_members(&braces, &mut out);
    let errors: Vec<_> = out.into_iter().map(Result::unwrap_err).collect();
    let counts = rejection_counts(&errors);
    assert_eq!(counts.get("GapTooSmall"), Some(&2));
    assert!(
        errors
            .iter()
            .all(|e| matches!(e, Reject::GapTooSmall { gap } if *gap < 0.32))
    );
    assert_eq!((p.rev(), p.members.len()), before);
    eprintln!("member model-clearance path hits: {counts:?}; admitted crossing braces=0");
}

#[test]
fn fixed_plan_freezes_exact_fields_vertices_and_connected_raft() {
    let mut p = builder(&plate());
    let batch =
        QuoteBatch::from_results(quotes(&p, &[candidate(0, -10.), candidate(1, 10.)])).unwrap();
    let contacts = p.commit(batch).unwrap();
    let mut p = p.into_routing();
    let req: Vec<_> = contacts
        .into_iter()
        .map(|contact| MemberProposal::Column {
            contact,
            section: Section::new(0.25).unwrap(),
        })
        .collect();
    let mut out = Vec::new();
    p.probe_members(&req, &mut out);
    p.commit_members(MemberQuoteBatch::from_results(out).unwrap())
        .unwrap();
    p.raft_connected().unwrap();
    let frozen = p.freeze().unwrap();
    assert_eq!(frozen.contacts().len(), 2);
    assert_eq!(
        frozen
            .parts()
            .iter()
            .filter(|p| p.kind() == PartKind::RaftPad)
            .count(),
        2
    );
    assert_eq!(
        frozen
            .parts()
            .iter()
            .filter(|p| p.kind() == PartKind::RaftBridge)
            .count(),
        1
    );
    for c in frozen.contacts() {
        for part in c.parts() {
            assert!(
                frozen
                    .parts()
                    .iter()
                    .any(|p| p.field_words() == part.field_words())
            );
        }
    }
}
