//! Sabotage witnesses for the owner-ruled sharing hypothesis. No contact is added
//! in the sharing comparison; all denominators are exposure-specific inputs.
use super::*;
fn src(x: f64, y: f64, z: f64, area: f64) -> SourceElement {
    SourceElement {
        xyz: [x, y, z],
        area,
    }
}
fn slab(k: u32, c: u32, source: SourceElement) -> LayerArea {
    let _ = k;
    LayerArea {
        model_envelope: source.area,
        model_midplane: source.area,
        components: vec![ComponentSlice {
            component: c,
            area: source.area,
            sources: vec![source],
        }],
        ..LayerArea::default()
    }
}
fn contact(x: f64, y: f64, z: f64, first: u32, patch: f64) -> ContactView {
    ContactView {
        centre: [x, y, z],
        first_layer: first,
        patch_lower_mm2: patch,
        neck_area_mm2: patch,
        site_class: SiteClass::HiddenUnderside,
    }
}
fn layout(contacts: Vec<ContactView>) -> LayoutView {
    let chains = contacts
        .iter()
        .enumerate()
        .map(|(i, c)| PartChain {
            contact: i as u32,
            parts: vec![
                PartSection {
                    kind: PartKind::Patch,
                    record: None,
                    area_lower_mm2: c.patch_lower_mm2,
                    z_range_mm: [-0.01, -0.01],
                },
                PartSection {
                    kind: PartKind::Neck,
                    record: None,
                    area_lower_mm2: c.neck_area_mm2,
                    z_range_mm: [-1.0, -0.5],
                },
            ],
        })
        .collect();
    LayoutView {
        contacts,
        chains,
        patch_history: Vec::new(),
    }
}
#[test]
fn tilted_growing_strip_new_contact_is_not_sized_by_nearest_3d_tributary() {
    // Synthetic tilted carrier: its exposure resultant moves uphill from
    // (-.25,-.25,.025) to (0,0,.075). The fourth corner has just become active.
    // Nearest-3D gives it the whole later layer, despite its small printed patch.
    let stack = LayerStack {
        layer_height_mm: 0.05,
        components: vec![ComponentInfo::default()],
        layers: vec![
            slab(0, 0, src(-0.25, -0.25, 0.025, 1.2)),
            slab(1, 0, src(0., 0., 0.075, 1.2)),
        ],
        ..LayerStack::default()
    };
    let mut v = layout(vec![
        contact(-1., -1., -0.02, 0, 1.),
        contact(1., -1., -0.02, 0, 1.),
        contact(-1., 1., -0.02, 0, 1.),
        contact(1., 1., 0.075, 1, 0.1),
    ]);
    v.patch_history = vec![
        vec![(0, 1.)],
        vec![(0, 1.)],
        vec![(0, 1.)],
        vec![(1, 0.05), (2, 0.1)],
    ];
    let d = analyse_full(&stack, &v, &Extras::default());
    assert_eq!(d.contacts.len(), 4, "sharing changes no contact count");
    let g = d.groups.iter().find(|g| g.layer == 1).unwrap();
    assert!(g.equilibrium_assessed, "{g:?}");
    let s = g.states.iter().find(|s| s.contact == 3).unwrap();
    assert!(
        s.ratios.tributary > 12.,
        "nearest-3D really overloads the new patch: {s:?}"
    );
    assert!(
        s.ratios.rigid_lever < 12. && s.ratios.additive < 12.,
        "{s:?}"
    );
    assert!(
        d.contacts.iter().all(|c| c.ratio < 12.),
        "tributary must never size a contact"
    );
    assert!(
        !d.report
            .warnings
            .iter()
            .any(|w| w.kind == "load not held at design ratio")
    );
    assert!(
        d.report
            .warnings
            .iter()
            .any(|w| w.kind == "local load path not assessed"),
        "global sharing never certifies local growing-edge transfer"
    );
}
#[test]
fn a_near_collinear_contact_row_is_unresolved_even_with_low_ratios_and_balanced_force() {
    let stack = LayerStack {
        layer_height_mm: 0.05,
        components: vec![ComponentInfo::default()],
        layers: vec![slab(0, 0, src(1.5, 0., 0.025, 1.))],
        ..LayerStack::default()
    };
    let v = layout(vec![
        contact(0., -0.01, 0., 0, 1.),
        contact(1., 0.01, 0., 0, 1.),
        contact(2., -0.01, 0., 0, 1.),
        contact(3., 0.01, 0., 0, 1.),
    ]);
    let d = analyse_full(&stack, &v, &Extras::default());
    let g = &d.groups[0];
    assert!(g.x_group < 12. && g.force_residual < 1e-6);
    assert!(
        g.condition < lever::CONDITION_LIMIT
            && g.position_sensitivity > lever::SENSITIVITY_LIMIT
            && !g.equilibrium_assessed,
        "{g:?}"
    );
    assert!(
        d.report
            .warnings
            .iter()
            .any(|w| w.kind == "load path not assessed"
                && w.layers.contains(&0)
                && !w.contacts.is_empty()),
        "a row is NOT a low-ratio pass"
    );
}
#[test]
fn a_separate_rib_island_cannot_borrow_main_carrier_capacity_before_merger() {
    let mut stack = LayerStack {
        layer_height_mm: 0.05,
        components: vec![
            ComponentInfo::default(),
            ComponentInfo {
                first_layer: 1,
                merged_into: Some((0, 2)),
                is_island: true,
                exempt: false,
            },
        ],
        layers: vec![
            slab(0, 0, src(0., 0., 0.025, 1.)),
            slab(1, 0, src(0., 0., 0.075, 1.)),
            slab(2, 0, src(0., 0., 0.125, 2.2)),
        ],
        ..LayerStack::default()
    };
    stack.layers[1].components.push(ComponentSlice {
        component: 1,
        area: 1.8,
        sources: vec![src(5., 0., 0.075, 1.8)],
    });
    stack.layers[1].model_envelope += 1.8;
    let v = layout(vec![
        contact(-1., -1., 0., 0, 1.),
        contact(1., -1., 0., 0, 1.),
        contact(0., 1., 0., 0, 1.),
        contact(5., 0., 0.05, 1, 0.04),
    ]);
    let d = analyse_full(&stack, &v, &Extras::default());
    let rib = d
        .groups
        .iter()
        .find(|g| g.layer == 1 && g.component == 1)
        .unwrap();
    assert_eq!(rib.active, vec![3]);
    assert!(
        (rib.x_group - 45.).abs() < 1e-9,
        "main part's ample sections cannot clear the rib's deficit"
    );
    assert!(d.report.warnings.iter().any(|w| w.kind
        == "additive area ratio beyond the unprintable limit"
        && w.class == WarningClass::Unprintable
        && w.contacts == vec![3]));
    let merged = d.groups.iter().find(|g| g.layer == 2).unwrap();
    assert_eq!(
        merged.active.len(),
        4,
        "pooling becomes legal only at the merger layer"
    );
    assert!(merged.x_group < 12.);
    let join = d
        .report
        .warnings
        .iter()
        .find(|w| w.kind == "local load path not assessed" && w.layers == vec![2])
        .expect("the merger needs its own local-transfer intervention");
    assert_eq!(join.location_mm, Some([2.5, 0., 0.125]));
    assert!(join.message.contains("connection") && join.message.contains("picture 04"));
    let lip = d
        .report
        .warnings
        .iter()
        .find(|w| w.kind == "local load path not assessed" && w.layers.first() == Some(&0))
        .expect("the first lip must be located, not the area peak");
    assert_eq!(lip.location_mm, Some([0., 0., 0.025]));
}
