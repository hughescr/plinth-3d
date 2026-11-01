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
            patch_history: Vec::new(),
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
        grow: false,
        sharing: false,
        group: None,
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

fn seed_load(chosen: &[Chosen]) -> load::LoadDetail {
    let stack = load::LayerStack {
        layer_height_mm: 0.05,
        layer_origin_mm: 6.0, // whole heads are printed by this controlled peel layer
        layers: vec![load::LayerArea {
            model_envelope: 20.0,
            model_midplane: 20.0,
            components: vec![load::ComponentSlice {
                component: 0,
                area: 20.0,
                sources: vec![load::SourceElement {
                    xyz: [0., 0., 6.025],
                    area: 20.0,
                }],
            }],
            ..load::LayerArea::default()
        }],
        components: vec![load::ComponentInfo::default()],
        ..load::LayerStack::default()
    };
    let contacts: Vec<_> = chosen.iter().map(|c| c.quote.contact()).collect();
    let layout = layout_from_contacts(
        &contacts,
        &vec![SiteClass::HiddenUnderside; contacts.len()],
        &stack,
        5.0,
    );
    load::analyse_full(&stack, &layout, &load::Extras::default())
}

#[test]
fn overloaded_seed_grows_before_any_sharing_neighbour_despite_capped_column_envelope() {
    let (builder, mut chosen) = fixture();
    let old_radius = chosen[0].candidate.ball_r;
    let before = seed_load(&chosen);
    assert!(before.contacts[0].ratio > 12.0);
    let mut cache = Cache {
        next_id: 1,
        ..Cache::default()
    };
    cache
        .load_envelope
        .insert(site_key(chosen[0].site.xyz), 1200.0);
    // A placement retry may block another target's site key, but that must
    // never forbid sizing an already retained coverage/anchoring contact.
    cache.blocked.insert(site_key(chosen[0].site.xyz));
    let capped = cache.grow_tips(&builder, &mut chosen, &before);
    assert_eq!(chosen.len(), 1, "growth never adds a neighbour");
    assert!(chosen[0].candidate.ball_r > old_radius);
    assert!(chosen[0].column.radius() < chosen[0].quote.contact().required_column_radius(1200.0));
    assert!(
        capped.is_empty(),
        "a newly grown seed cannot request sharing neighbours this round"
    );
    let after = seed_load(&chosen);
    let grid = plinth_geom::slice::LayerGrid {
        origin: 6.0,
        height: 0.05,
        layers: 1,
    };
    assert!(
        load_targets(&after, &chosen, &capped, &grid, 5.0)
            .iter()
            .all(|t| t.sharing),
        "unresolved equilibrium may request a spread trial; retention still needs complete replay"
    );
    assert!(after.contacts[0].ratio < before.contacts[0].ratio);
    assert!(
        after.contacts[0].ratio > 12.0,
        "remaining overload stays visible"
    );
    assert!(chosen[0].quote.contact().patch().upper() <= 0.48);
    let capped = cache.grow_tips(&builder, &mut chosen, &after);
    assert!(
        capped.contains(&site_key(chosen[0].site.xyz)),
        "only now can the capped seed request neighbours"
    );
    assert_eq!(
        load_targets(&after, &chosen, &capped, &grid, 5.0).len(),
        1,
        "one component-directed spread target, not sixteen ring targets"
    );
}

#[test]
fn private_column_resize_retains_the_original_for_atomic_trial_rollback() {
    let (builder, mut chosen) = fixture();
    let original = chosen[0].candidate.id;
    let patch_key = patch_curve_key(&chosen[0], 0, 5.0);
    let mut cache = Cache {
        next_id: 1,
        ..Cache::default()
    };
    cache.size_columns(&builder, &mut chosen, &[5.0]);
    assert_ne!(
        chosen[0].candidate.id, original,
        "the witness must resize a column"
    );
    assert_eq!(
        patch_curve_key(&chosen[0], 0, 5.0),
        patch_key,
        "a column resize reuses the identical certified patch geometry"
    );
    assert_ne!(
        patch_curve_key(&chosen[0], 1, 5.0),
        patch_key,
        "different activation cannot borrow earlier capacity"
    );
    let at = cache
        .available
        .iter()
        .position(|c| c.candidate.id == original)
        .expect("a rejected trial must still own the old certificate");
    std::mem::swap(&mut chosen[0], &mut cache.available[at]);
    assert_eq!(chosen[0].candidate.id, original);
}

#[test]
fn coverage_selection_cannot_claim_a_nearby_search_site_outside_the_actual_reach() {
    let (_builder, mut measured) = fixture();
    let q = measured.pop().unwrap();
    let p = q.site;
    let mut cache = Cache::default();
    cache.available.push(q);
    let mut chosen = Vec::new();
    cache.select(
        vec![(
            Target {
                xyz: [4., 0., 0.],
                deadline: 1.,
                load: 0.,
                anchor: false,
                grow: false,
                sharing: false,
                group: None,
            },
            None,
        )],
        vec![vec![p]],
        &mut chosen,
    );
    assert!(
        chosen.is_empty(),
        "search proximity is NOT coverage; the actual ball centre must be within 3 mm"
    );
}

#[test]
fn load_only_keep_rule_requires_improvement_and_refuses_a_worse_violation_elsewhere() {
    let (_, chosen) = fixture();
    let mut before = seed_load(&chosen);
    let mut second = before.groups[0].clone();
    second.layer = 1;
    before.groups.push(second);
    assert!(
        !replay_improves(&before, &before),
        "no-op load additions are discarded"
    );
    let mut after = before.clone();
    after.groups[0].x_group *= 0.8;
    assert!(
        replay_improves(&before, &after),
        "genuine additive improvement is measured"
    );
    before.groups[0].x_group = 29.24;
    after.groups[0].x_group = 11.75;
    before.groups[1].x_group = 18.66;
    after.groups[1].x_group = 20.14;
    assert!(
        replay_improves(&before, &after),
        "removing an unprintable island deficit outweighs a larger existing intervention"
    );
    after.groups[1].x_group = 25.1;
    assert!(
        !replay_improves(&before, &after),
        "no new unprintable exposure"
    );
    before.groups[1].x_group = 11.88;
    after.groups[1].x_group = 12.10;
    assert!(
        !replay_improves(&before, &after),
        "no new intervention exposure"
    );
    after.groups[1].x_group = before.groups[1].x_group;
    before.groups[1].equilibrium_assessed = true;
    assert!(
        !replay_improves(&before, &after),
        "newly unresolved equilibrium is not a low-ratio pass"
    );
}

#[test]
fn mandatory_island_capacity_is_not_an_optional_parent_sharing_refinement() {
    let (_, chosen) = fixture();
    let mut before = seed_load(&chosen);
    before.groups[0].x_group = 29.24;
    let mut parent = before.groups[0].clone();
    parent.component = 1;
    parent.x_group = 11.88;
    parent.equilibrium_assessed = true;
    before.groups.push(parent);
    let mut after = before.clone();
    after.groups[0].x_group = 11.75;
    after.groups[1].x_group = 12.10;
    after.groups[1].equilibrium_assessed = false;
    assert!(
        !replay_improves(&before, &after),
        "a load-only refinement cannot add a violation"
    );
    assert!(
        island_capacity_improves(&before, &after, 0),
        "independent island capacity must survive a new reported parent intervention"
    );
    after.groups[1].x_group = 25.1;
    assert!(
        !island_capacity_improves(&before, &after, 0),
        "never create another unprintable deficit"
    );
    after.groups[1].x_group = 12.10;
    after.groups[0].x_group = 29.24;
    assert!(
        !island_capacity_improves(&before, &after, 0),
        "the island itself must improve"
    );
}

#[test]
fn replay_compares_unresolved_measures_by_class_not_incompatible_units() {
    let (_, chosen) = fixture();
    let mut before = seed_load(&chosen);
    before.groups[0].condition = 1e8;
    before.groups[0].moment_residual = 0.1;
    let mut after = before.clone();
    after.groups[0].condition = 1e7;
    assert!(
        replay_improves(&before, &after),
        "conditioning alone improves"
    );
    after.groups[0].moment_residual = 0.2;
    assert!(
        replay_improves(&before, &after),
        "a larger already-unresolved moment is still an intervention, not a new class"
    );
    before.groups[0].moment_residual = 0.0;
    assert!(
        !replay_improves(&before, &after),
        "conditioning improvement cannot hide a newly failed moment assessment"
    );
}

#[test]
fn load_sharing_search_has_no_mandatory_neighbour_ring() {
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
        grow: false,
        sharing: true,
        group: None,
    };
    let near = cache.near(target, None, &sites, 0., &[]);
    assert_eq!(near.len(), 6);
    assert!(near.iter().any(|p| p.xyz[0] > 2.5));
    assert!(near.iter().any(|p| p.xyz[0] < 1.0 && p.xyz[0] > 0.0));
    assert!(near.iter().any(|p| p.xyz[1] > 0.));
    assert!(near.iter().any(|p| p.xyz[1] < 0.));
    assert!(near.iter().any(|p| p.xyz[0] < 0.));
}
