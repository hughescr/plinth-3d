use super::*;
use crate::report::Status;
use crate::testkit::{self as kit, test_limits};

fn scene(parts: Vec<Part>) -> Scene {
    Scene::from_parts(
        &test_limits(),
        kit::closed_box([-1.0, -1.0, 5.0], [1.0, 1.0, 6.0]),
        [0.0, 0.0, 5.0],
        parts,
    )
}

fn strip(rec: usize, from: [f32; 2], to: [f32; 2], bottom: f32, top: f32, width: f32) -> Part {
    kit::part_with_mesh(
        rec,
        &kit::rec(
            6,
            [from[0], from[1], 0.0],
            [to[0], to[1], 0.0],
            width,
            top - bottom,
        ),
        &kit::bridge_coords(from, to, top, bottom, width * 0.5),
    )
}

fn column(rec: usize, x: f32, y: f32, bottom: f32) -> Part {
    kit::part(rec, 3, [x, y, bottom], [x, y, 3.0], 0.3, 0.3)
}

fn bypass() -> Vec<Part> {
    vec![
        kit::part(10, 4, [0.0, 0.0, 1.0], [0.0, 0.0, 0.0], 0.3, 0.35),
        column(20, 0.0, 0.0, 0.998),
        column(30, 0.59, 0.0, 1.5),
        // A remote, thick backing joins the upper ends; it is not near the weak side lens.
        strip(40, [-0.2, 0.0], [0.79, 0.0], 2.8, 3.1, 0.5),
    ]
}

fn incidental_count(o: &Outcome) -> usize {
    let Some(Json::Arr(cs)) = o.detail.get("incidental_contacts") else {
        panic!("{o:?}")
    };
    cs.len()
}

#[test]
fn weak_only_cycle_fails_even_though_none_of_its_weak_edges_is_a_bridge() {
    let s = scene(vec![
        kit::part(0, 4, [0.0, 0.0, 0.5], [0.0, 0.0, 0.0], 0.3, 0.3),
        kit::part(1, 3, [0.0, 0.0, 0.49], [0.0, 0.0, 1.3], 0.3, 0.3),
        kit::part(2, 3, [0.59, 0.0, 1.0], [0.59, 0.0, 1.3], 0.3, 0.3),
        kit::part(3, 3, [0.295, 0.509, 1.0], [0.295, 0.509, 1.3], 0.3, 0.3),
    ]);
    assert!(s.parts.iter().all(|p| own_wide(&s, p, 0.1)));
    let fs = join_findings(&s, 0.1);
    for (a, b) in [(1, 2), (1, 3), (2, 3)] {
        assert!(
            fs.iter()
                .any(|f| f.rec == a && f.partner == Some(b) && matches!(f.fate, Fate::Thin { .. })),
            "missing weak cycle edge {a}-{b}: {:?}",
            fs.iter()
                .map(|f| (f.rec, f.partner, &f.fate))
                .collect::<Vec<_>>()
        );
    }
    let o = measure(&s);
    assert_eq!(o.status, Status::Violation, "{o:?}");
    assert_eq!(incidental_count(&o), 0);
}

#[test]
fn thick_remote_bypass_makes_the_graze_incidental_without_strength_credit() {
    let s = scene(bypass());
    let o = measure(&s);
    assert_eq!(o.status, Status::Pass, "{o:?}");
    assert!(
        incidental_count(&o) >= 1,
        "the exemption path must run: {o:?}"
    );
    let fs = join_findings(&s, 0.1);
    let f = fs
        .iter()
        .find(|f| f.rec == 20 && f.partner == Some(30))
        .expect("graze");
    assert!(f.incidental && matches!(f.fate, Fate::Thin { .. }));
    assert!((f.at[0] - 0.295).abs() < 1e-5);
    assert!(
        dist(f.at, s.parts[1].bbox.centre()) > 0.2,
        "not the record centre"
    );
    let Some(Json::Arr(cs)) = o.detail.get("incidental_contacts") else {
        panic!("{o:?}")
    };
    assert!(
        cs.iter()
            .all(|c| c.get("strength_credit") == Some(&Json::Bool(false)))
    );
    // Geometric sabotage: remove the remote backing. The same graze must now fail.
    let mut broken = bypass();
    broken.pop();
    assert_eq!(measure(&scene(broken)).status, Status::Violation);
}

#[test]
fn a_gap_never_supplies_a_certified_route_but_a_contact_does() {
    let parts = |bottom| {
        vec![
            kit::part(0, 4, [0.0, 0.0, 0.5], [0.0, 0.0, 0.0], 0.3, 0.3),
            column(1, 0.0, 0.0, bottom),
        ]
    };
    let gap = scene(parts(0.500_001));
    assert!(pair_finding(&gap.parts[0], &gap.parts[1], 0.1).is_none());
    let contact = scene(parts(0.5));
    let f = pair_finding(&contact.parts[0], &contact.parts[1], 0.1).expect("contact");
    assert!(matches!(f.fate, Fate::Wide { .. }));
    assert!(f.at[2] > 0.499_999 && f.at[2] < 0.500_001);
}

#[test]
fn a_nonconvex_thin_neck_gets_no_exemption_and_keeps_the_pairwise_failure() {
    let mut ps = bypass();
    // A connected waist in a nominal rod: move its upper ring inward and append a fat lobe.
    // Deliberately retain stale metadata as sabotage: exemption must check the actual mesh.
    let mut bad = column(90, 5.0, 0.0, 1.0);
    bad.tris = [
        kit::closed_box([4.8, -0.2, 0.0], [5.2, 0.2, 1.0]),
        kit::closed_box([4.98, -0.02, 1.0], [5.02, 0.02, 2.0]),
        kit::closed_box([4.8, -0.2, 2.0], [5.2, 0.2, 3.0]),
    ]
    .concat();
    bad.mesh = Some(Solid::polytope(bad.tris.clone()));
    assert!(!convex(&bad));
    ps.push(bad);
    let fs = join_findings(&scene(ps), 0.1);
    assert!(fs.iter().any(|f| f.rec == 20
        && f.partner == Some(30)
        && !f.incidental
        && matches!(f.fate, Fate::Thin { .. })));
}

#[test]
fn fat_part_with_thin_tail_is_caught_before_contact_exemptions() {
    let mut ps = bypass();
    ps.push(kit::part(
        90,
        3,
        [5.0, 0.0, 0.0],
        [5.0, 0.0, 3.0],
        0.3,
        0.02,
    ));
    let s = scene(ps);
    assert!(s.parts.last().expect("tail").is_valid());
    assert!(!own_wide(&s, s.parts.last().expect("tail"), 0.1));
    let o = measure(&s);
    assert_eq!(o.status, Status::Violation, "{o:?}");
    assert!(
        o.located.iter().any(|p| p.location.record == Some(90)),
        "the part-width path must run: {o:?}"
    );
}

#[test]
fn multi_part_local_backing_uses_only_certified_edges() {
    let mut ps = bypass();
    ps.pop();
    // Replace the remote backing by two overlapping thick parts. Neither is enough alone.
    ps.push(strip(40, [-0.2, 0.0], [0.295, 0.0], 2.8, 3.1, 0.5));
    ps.push(strip(50, [0.195, 0.0], [0.79, 0.0], 2.8, 3.1, 0.5));
    let s = scene(ps.clone());
    let o = measure(&s);
    assert_eq!(o.status, Status::Pass, "{o:?}");
    assert!(incidental_count(&o) >= 1);
    ps.pop();
    assert_ne!(
        measure(&scene(ps)).status,
        Status::Pass,
        "a backing segment must be necessary"
    );
}

fn round_scene(parts: Vec<Part>) -> Scene {
    // Same local plate and XY translation as the rejected file. The model is irrelevant to H9.
    let mut s = scene(parts);
    s.model_min_z = -11.615_062_713_623_047;
    s.translate = [0.000_466_823_58, 1.374_729_9, 5.0];
    s
}

#[test]
fn exact_round25_flare184_strip261_is_a_graze_at_bed_point198_to_point200() {
    let mut ps = vec![round25::part(184), round25::part(261)];
    let f = pair_finding(&ps[0], &ps[1], 0.1).expect("actual contact");
    assert!(matches!(f.fate, Fate::Thin { .. }), "{:?}", f.fate);
    let world = round_scene(Vec::new()).world(f.at);
    assert!((0.198..0.200_01).contains(&world[2]), "{world:?}");
    assert_eq!(measure(&round_scene(ps.clone())).status, Status::Violation);
    // Equivalent minimal native backing under the flare. The exact offending pair is intact.
    ps.push(kit::raft_pad(
        300,
        [-1.145_510_4, 1.902_362_8],
        f32::from_bits(3246608908),
    ));
    let o = measure(&round_scene(ps));
    assert_eq!(o.status, Status::Pass, "{o:?}");
    assert!(incidental_count(&o) >= 1);
}

#[test]
fn exact_round25_pad225_strip260_reports_the_contact_not_the_pad_centre() {
    let ps = vec![round25::part(225), round25::part(260)];
    let f = pair_finding(&ps[0], &ps[1], 0.1).expect("actual contact");
    assert!(matches!(f.fate, Fate::Thin { .. }), "{:?}", f.fate);
    let world = round_scene(Vec::new()).world(f.at);
    assert!(
        (world[0] + 2.45).abs() < 0.15 && (world[1] + 1.91).abs() < 0.15,
        "{world:?}"
    );
    let o = measure(&round_scene(ps));
    assert_eq!(o.status, Status::Pass, "{o:?}");
    assert_eq!(incidental_count(&o), 1);
    let Some(Json::Arr(cs)) = o.detail.get("incidental_contacts") else {
        panic!("{o:?}")
    };
    assert_eq!(
        cs[0].get("records"),
        Some(&Json::Arr(vec![Json::Uint(225), Json::Uint(260)]))
    );
    // Geometric sabotage: move the plate away. A record role is not a root certificate.
    let mut unsupported = round_scene(vec![round25::part(225), round25::part(260)]);
    unsupported.translate[2] += 1.0;
    assert_eq!(measure(&unsupported).status, Status::Violation);
}

#[test]
fn reachability_removes_all_weak_edges_simultaneously() {
    let roots = [true, false, false, false];
    // Certified bypass only. The weak triangle (1,2),(2,3),(3,1) is absent, not tested as bridges.
    assert_eq!(rooted(&roots, &[(0, 1)]), [true, true, false, false]);
    assert_eq!(rooted(&roots, &[(0, 1), (1, 2), (2, 3)]), [true; 4]);
}
