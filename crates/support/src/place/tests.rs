use super::*;

#[test]
fn invalid_layers_do_not_establish_placement_success() {
    assert!(unavailable(&LayerStack::default()));
    let mut stack = LayerStack {
        layer_height_mm: f64::NAN,
        ..LayerStack::default()
    };
    stack.layers.push(load::LayerArea::default());
    assert!(unavailable(&stack));
}

#[test]
fn search_settings_pin_owner_process_and_starting_values() {
    let s = PlanSettings::default();
    assert_eq!(s.max_rounds, 8);
    assert!((s.coverage_reach_mm - 3.).abs() < 1e-12);
    assert!((load::DESIGN_RATIO - 20.).abs() < 1e-12);
}

#[test]
fn column_sizing_uses_the_later_maximum_not_the_early_worst_ratio() {
    let detail = load::LoadDetail {
        report: load::LoadReport::unverified(load::DESIGN_RATIO, "controlled sizing fixture"),
        contacts: vec![load::ContactLoad {
            contact: 0,
            component: Some(0),
            first_layer: 0,
            worst_layer: 1,
            load_mm2: 1.,
            ratio: 1000.,
            tributary_xy: [0.; 2],
            offset_mm: 0.,
            layers_over: 2,
            first_over: Some(1),
            last_over: Some(10),
        }],
        sections: vec![
            load::SectionDetail {
                chain: 0,
                part: 0,
                kind: load::PartKind::Patch,
                record: None,
                area_mm2: 0.001,
                tributary: (1000., 1),
                additive: (0., 0),
                equal_share: (0., 0),
                rigid_lever: (22.5, 10),
                sizing: (22.5, 10),
                at_worst: load::Sharing::default(),
                load_mm2: 0.0,
            },
            load::SectionDetail {
                chain: 0,
                part: 1,
                kind: load::PartKind::Neck,
                record: None,
                area_mm2: 0.1,
                tributary: (80., 10),
                additive: (0., 0),
                equal_share: (0., 0),
                rigid_lever: (22.5, 10),
                sizing: (22.5, 10),
                at_worst: load::Sharing::default(),
                load_mm2: 0.0,
            },
            load::SectionDetail {
                chain: 0,
                part: 2,
                kind: load::PartKind::Column,
                record: None,
                area_mm2: 0.4,
                tributary: (22.5, 10),
                additive: (0., 0),
                equal_share: (0., 0),
                rigid_lever: (22.5, 10),
                sizing: (22.5, 10),
                at_worst: load::Sharing::default(),
                load_mm2: 0.0,
            },
        ],
        groups: vec![],
        peak: None,
        coverage: None,
    };
    let loads = routing_loads(&detail);
    assert_eq!(loads.len(), 1);
    assert!((loads[0] - 9.0).abs() < 1e-12);
}

#[test]
fn mutually_exclusive_alternatives_cannot_bypass_atomic_pair_admission() {
    use crate::construct::*;
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
    let mut builder = PlanBuilder::new(
        model,
        limits,
        ProcessRecord {
            layer_height_mm: 0.05,
            layer_origin_mm: 0.,
        },
    );
    let a = ContactCandidate {
        id: CandId(0),
        site: [0.; 3],
        ball_r: 0.212,
        standoff: 0.0772,
        axis: [0., 0., -1.],
        neck: 0.2,
        joint_r: 0.3,
        head_len: 2.4,
    };
    let b = ContactCandidate { id: CandId(1), ..a };
    let mut results = Vec::new();
    builder.probe_alternatives(&[a, b], &mut results);
    assert!(results.iter().all(Result::is_ok));
    let batch = QuoteBatch::from_results(results).unwrap();
    assert!(matches!(
        builder.commit(batch),
        Err(Reject::WouldFuse { .. })
    ));
    assert_eq!(builder.contact_count(), 0);
}

#[test]
fn e2e_rounds_round25_private_planner_reports_exhaustion_honestly() {
    use crate::construct::*;
    use plinth_geom::{Mesh, measure::Measure, slice::LayerGrid};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/round/approved/Round 25.keepup.stl");
    let mesh = plinth_geom::read_stl_file(&path).unwrap();
    let limits = Limits::current().unwrap();
    let posed = PosedModel::new(
        &mesh,
        &KeepUpRecord {
            rotation_x_deg: 115.,
            approved_sha256: "853709e4063de2e3c9dc7a646f13d5ede5a2f26c4a4372ab220cfe153480750a"
                .into(),
        },
        &limits,
    )
    .unwrap();
    let shift = -f64::from(posed.plate_z());
    let local = posed.mesh();
    let world = Mesh::new(
        local.xs().to_vec(),
        local.ys().to_vec(),
        local.zs().iter().map(|z| z + shift).collect(),
        local.indices().to_vec(),
    )
    .unwrap();
    let grid = LayerGrid::covering(&world, 0.05).unwrap();
    let Measure::Known(layers) =
        load::build_model_layers(&world, &grid, &load::StackOptions::default())
    else {
        panic!("layers unavailable")
    };
    let builder = PlanBuilder::new(
        posed,
        limits,
        ProcessRecord {
            layer_height_mm: 0.05,
            layer_origin_mm: 0.,
        },
    );
    let builder = builder.with_workers(3).unwrap();
    let t = std::time::Instant::now();
    let p = plan_contacts(&builder, &layers, &PlanSettings::default());
    println!(
        "planner {:?}; {} contacts; {:.3}s; {:?}",
        p.termination,
        p.candidates.len(),
        t.elapsed().as_secs_f64(),
        p.rejections
    );
    println!(
        "tips {:?}",
        p.candidates.iter().map(|c| c.ball_r).collect::<Vec<_>>()
    );
    assert!(p.load.is_computed());
    assert!(
        p.candidates.len() >= 12,
        "only {} contacts",
        p.candidates.len()
    );
    let worst = p
        .load
        .sections
        .iter()
        .map(|s| s.ratios.rigid_lever.max(s.ratios.additive))
        .fold(0.0, f64::max);
    assert!(
        p.load.sections.iter().all(|s| {
            s.ratios.rigid_lever.max(s.ratios.additive) <= load::DESIGN_RATIO
                || p.load.warnings.iter().any(|w| {
                    w.class != load::WarningClass::Information
                        && (w.kind.contains("load") || w.kind.contains("ratio"))
                        && !w.message.is_empty()
                        && (w.contacts.contains(&s.chain) || w.layers.contains(&s.worst_layer))
                })
        }),
        "unreported overload {worst}"
    );
    assert!(
        p.routed,
        "private route is not complete: {:?}",
        p.routing_blocker
    );
    assert_ne!(p.termination, Termination::ProvedNoLegalAnchor);
    let mut out = Vec::new();
    builder.probe(&p.candidates, &mut out);
    assert!(
        out.iter().all(Result::is_ok),
        "whole-layout replay: {:?}",
        out.iter().map(|r| r.as_ref().err()).collect::<Vec<_>>()
    );
}

#[test]
fn resting_up_plus_z_converts_to_the_planner_up_vector_in_one_place() {
    use crate::construct::{KeepUpRecord, RestingUp};
    let record = KeepUpRecord::resting_up(RestingUp::PlusZ, "0".repeat(64));
    let a = record.rotation_x_deg.to_radians();
    // The planner derives up = [0, sin a, -cos a] (see `plan_contacts`).
    assert!(a.sin().abs() < 1e-12 && (-a.cos() - 1.0).abs() < 1e-12);
}

#[test]
fn a_passed_deadline_stops_the_planner_before_any_contact_and_says_where() {
    use crate::construct::*;
    use plinth_geom::{Mesh, measure::Measure, slice::LayerGrid};
    let quads = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    let corner = |i: u32, bit: u32| if i & bit != 0 { 4.0 } else { 0.0 };
    let mesh = Mesh::new(
        (0..8).map(|i| corner(i, 1)).collect(),
        (0..8).map(|i| corner(i, 2)).collect(),
        (0..8).map(|i| corner(i, 4)).collect(),
        quads
            .iter()
            .flat_map(|q| [q[0], q[1], q[2], q[0], q[2], q[3]])
            .collect(),
    )
    .unwrap();
    let limits = Limits::current().unwrap();
    let posed = PosedModel::new(
        &mesh,
        &KeepUpRecord::resting_up(RestingUp::PlusZ, "0".repeat(64)),
        &limits,
    )
    .unwrap();
    let shift = -f64::from(posed.plate_z());
    let local = posed.mesh();
    let world = Mesh::new(
        local.xs().to_vec(),
        local.ys().to_vec(),
        local.zs().iter().map(|z| z + shift).collect(),
        local.indices().to_vec(),
    )
    .unwrap();
    let grid = LayerGrid::covering(&world, 0.05).unwrap();
    let Measure::Known(layers) =
        load::build_model_layers(&world, &grid, &load::StackOptions::default())
    else {
        panic!("layers unavailable")
    };
    let builder = PlanBuilder::new(
        posed,
        limits,
        ProcessRecord {
            layer_height_mm: 0.05,
            layer_origin_mm: 0.,
        },
    );
    let past = PlanSettings {
        deadline: Some(std::time::Instant::now()),
        ..PlanSettings::default()
    };
    let stopped = plan_contacts(&builder, &layers, &past);
    // With nothing chosen the final route replay fails and names itself; the stop is
    // still recorded.
    assert!(matches!(
        stopped.termination,
        Termination::TimeBudget | Termination::RouteFailed
    ));
    assert_eq!(stopped.stopped_at_budget, Some("round start"));
    assert!(stopped.candidates.is_empty());
    let free = plan_contacts(&builder, &layers, &PlanSettings::default());
    assert_eq!(free.stopped_at_budget, None);
    assert_ne!(free.termination, Termination::TimeBudget);
}
