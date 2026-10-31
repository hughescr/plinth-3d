use super::*;
use crate::construct::{KeepUpRecord, Limits, PosedModel, ProcessRecord};

fn fixture() -> (PlanBuilder<Contacts>, Vec<Chosen>) {
    let mesh = plinth_geom::Mesh::new(
        vec![-20., 20., 20., -20.],
        vec![-20., -20., 20., 20.],
        vec![0.; 4],
        vec![0, 2, 1, 0, 3, 2],
    )
    .unwrap();
    let limits = Limits::current().unwrap();
    let model = PosedModel::new(
        &mesh,
        &KeepUpRecord {
            rotation_x_deg: 115.,
            approved_sha256: "0".repeat(64),
        },
        &limits,
    )
    .unwrap();
    let builder = PlanBuilder::new(
        model,
        limits,
        ProcessRecord {
            layer_height_mm: 0.05,
            layer_origin_mm: 0.,
        },
    );
    let site = NeedPoint {
        xyz: [0.; 3],
        normal: [0., 0., -1.],
        angle_deg: 0.,
        area_mm2: 1.,
        triangle: 0,
        class: SiteTag::Hidden,
    };
    let mut cand = candidate(0, &site, 0.2, 0., 1.);
    cand.joint_r = 0.312;
    let mut out = Vec::new();
    builder.probe_alternatives(&[cand], &mut out);
    let quote = out.pop().unwrap().unwrap();
    let column = builder
        .probe_column_alternatives(&[&quote], &[1.])
        .pop()
        .unwrap()
        .unwrap();
    (
        builder,
        vec![Chosen {
            candidate: cand,
            quote,
            column,
            site,
        }],
    )
}

#[test]
fn failed_size_search_keeps_the_contact_and_never_bans_its_site() {
    let (builder, mut chosen) = fixture();
    let original = chosen[0].candidate;
    let mut cache = Cache {
        next_id: 1,
        ..Cache::default()
    };
    // No head in this family can contain the required joint under the reach cap.
    cache.size_columns(&builder, &mut chosen, &[1200.]);
    assert_eq!(chosen.len(), 1);
    assert_eq!(chosen[0].candidate, original);
    assert!(cache.blocked.is_empty());
    assert_eq!(
        cache.next_id, 25,
        "8 fan directions x 3 head lengths were tried"
    );
    assert!((cache.load_envelope[&site_key(chosen[0].site.xyz)] - 1200.).abs() < 1e-10);
    cache.size_columns(&builder, &mut chosen, &[1.]);
    assert_eq!(
        chosen[0].candidate, original,
        "a later smaller load does not shrink the envelope"
    );
    assert_eq!(cache.next_id, 49);
    assert!(cache.blocked.is_empty());
    // Selection also keeps a retained contact when no replacement is available.
    let target = Target {
        xyz: chosen[0].site.xyz,
        deadline: f64::INFINITY,
        load: 1200.,
        anchor: false,
        ring: None,
    };
    cache.select(
        vec![(target, Some(chosen[0].site))],
        vec![vec![chosen[0].site]],
        &mut chosen,
    );
    assert_eq!(chosen.len(), 1);
    assert_eq!(chosen[0].candidate, original);
}

#[test]
fn successful_private_column_resize_rechecks_before_replacement() {
    let (builder, mut chosen) = fixture();
    let old = chosen[0].candidate.joint_r;
    let mut cache = Cache {
        next_id: 1,
        ..Cache::default()
    };
    cache.size_columns(&builder, &mut chosen, &[4.]);
    assert_eq!(chosen.len(), 1);
    assert!(chosen[0].candidate.joint_r > old);
    assert!(chosen[0].column.radius() >= chosen[0].quote.contact().column_radius(4.));
    assert!(cache.blocked.is_empty());
}

#[test]
fn load_sharing_search_never_leaves_the_one_to_two_point_five_mm_ring() {
    let cache = Cache::default();
    let sites: Vec<_> = [
        [0.1, 0., 0.],
        [1.1, 0., 0.],
        [-1.1, 0., 0.],
        [0., 1.1, 0.],
        [0., -1.1, 0.],
        [2.6, 0., 0.],
    ]
    .into_iter()
    .map(|xyz| NeedPoint {
        xyz,
        normal: [0., 0., -1.],
        angle_deg: 0.,
        area_mm2: 1.,
        triangle: 0,
        class: SiteTag::Hidden,
    })
    .collect();
    let target = Target {
        xyz: [1.9, 0., 0.],
        deadline: 1.,
        load: 2.,
        anchor: false,
        ring: Some([0.; 3]),
    };
    let near = cache.near(target, None, &sites, 0., &[]);
    assert_eq!(near.len(), 4);
    assert!(
        near.iter()
            .all(|p| (1.0..=6.25).contains(&d2(p.xyz, [0.; 3])))
    );
    assert!(near.iter().any(|p| p.xyz[1] > 0.));
    assert!(near.iter().any(|p| p.xyz[1] < 0.));
    assert!(near.iter().any(|p| p.xyz[0] < 0.));
}
