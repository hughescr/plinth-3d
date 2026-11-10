use super::needs::{NeedPoint, NeedsMap, SiteTag};
use super::*;

const H: f64 = 0.05;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// `n x n` source elements over the rectangle, each with an equal share of its area.
fn rect(x: [f64; 2], y: [f64; 2], z: f64, n: usize) -> Vec<SourceElement> {
    let area = (x[1] - x[0]) * (y[1] - y[0]) / (n * n) as f64;
    let mut v = Vec::new();
    for i in 0..n {
        for j in 0..n {
            v.push(SourceElement {
                xyz: [
                    x[0] + (x[1] - x[0]) * (i as f64 + 0.5) / n as f64,
                    y[0] + (y[1] - y[0]) * (j as f64 + 0.5) / n as f64,
                    z,
                ],
                area,
            });
        }
    }
    v
}

fn slice(comp: u32, src: Vec<SourceElement>) -> ComponentSlice {
    ComponentSlice {
        component: comp,
        area: src.iter().map(|e| e.area).sum(),
        sources: src,
    }
}

fn layer(slices: Vec<ComponentSlice>) -> LayerArea {
    let a: f64 = slices.iter().map(|s| s.area).sum();
    LayerArea {
        model_envelope: a,
        model_midplane: a,
        components: slices,
        ..LayerArea::default()
    }
}

fn comp(first: u32) -> ComponentInfo {
    ComponentInfo {
        first_layer: first,
        merged_into: None,
        is_island: true,
        exempt: false,
    }
}

/// A body of `areas.len()` layers: a 12 x 4 mm rectangle scaled to each layer's area.
fn body(areas: &[f64]) -> LayerStack {
    let layers = areas
        .iter()
        .enumerate()
        .map(|(k, &a)| {
            let w = a / 4.0;
            layer(vec![slice(
                0,
                rect([0.0, w], [0.0, 4.0], k as f64 * H + 0.025, 12),
            )])
        })
        .collect();
    LayerStack {
        layer_height_mm: H,
        layer_origin_mm: 0.0,
        pixel_pitch_mm: [0.014, 0.019],
        layers,
        components: vec![comp(0)],
    }
}

fn contact(x: f64, y: f64, first_layer: u32, patch: f64, neck: f64) -> ContactView {
    ContactView {
        centre: [x, y, 0.0],
        patch_lower_mm2: patch,
        neck_area_mm2: neck,
        first_layer,
        site_class: SiteClass::HiddenUnderside,
    }
}

fn part(kind: PartKind, area: f64, z: [f64; 2]) -> PartSection {
    PartSection {
        kind,
        record: None,
        area_lower_mm2: area,
        z_range_mm: z,
    }
}

/// A layout with the contacts and, for each, a chain Patch / Neck / Column of the given areas.
fn layout(cs: Vec<ContactView>, neck: f64, column: f64) -> LayoutView {
    let chains = cs
        .iter()
        .enumerate()
        .map(|(i, c)| PartChain {
            contact: i as u32,
            parts: vec![
                part(PartKind::Patch, c.patch_lower_mm2, [0.0, 0.0]),
                part(PartKind::Neck, neck, [-2.0, -1.0]),
                part(PartKind::Column, column, [-1.0, -0.5]),
            ],
        })
        .collect();
    LayoutView {
        contacts: cs,
        chains,
        patch_history: Vec::new(),
    }
}

fn kinds(r: &LoadReport, class: WarningClass) -> Vec<String> {
    r.warnings
        .iter()
        .filter(|w| w.class == class)
        .map(|w| w.kind.clone())
        .collect()
}

#[test]
fn shares_sum_to_the_layer_area_and_layers_are_never_summed() {
    // Layers of 12 mm²: three contacts along the 3 mm x 4 mm body, each with a section of 1.
    let stack = body(&[12.0, 12.0, 12.0]);
    let lay = layout(
        vec![
            contact(0.5, 2.0, 0, 1.0, 1.0),
            contact(1.5, 2.0, 0, 1.0, 1.0),
            contact(2.5, 2.0, 0, 1.0, 1.0),
        ],
        1.0,
        1.0,
    );
    let d = analyse_full(&stack, &lay, &Extras::default());
    assert!(d.report.is_computed());
    for g in &d.groups {
        assert!(
            close(g.loads_mm2.iter().sum::<f64>(), g.area_mm2),
            "layer {}",
            g.layer
        );
        assert!(
            g.loads_mm2.iter().all(|&l| close(l, 4.0)),
            "{:?}",
            g.loads_mm2
        );
    }
    // 4 mm² on a section of 1 mm² is 4, in every layer: three layers do not make 12.
    let top = d.sections.iter().map(|s| s.tributary.0).fold(0.0, f64::max);
    assert!(close(top, 4.0), "{top}");
    assert_eq!(d.report.layers.len(), 3);
    assert!(close(d.report.layers[0].area_mm2, 12.0));
    assert!(close(d.report.layers[0].capacity_mm2, 12.0 * 3.0));
}

#[test]
fn per_layer_labels_use_each_contacts_own_worst_section_not_the_group_average() {
    let stack = body(&[12.0]);
    let lay = layout(
        vec![
            contact(0.5, 2.0, 0, 0.1, 1.0),
            contact(2.5, 2.0, 0, 0.8, 1.0),
        ],
        1.0,
        1.0,
    );
    let d = analyse_full(&stack, &lay, &Extras::default());
    let g = &d.groups[0];
    assert!(close(g.contact_sections_mm2[0][1], 0.1));
    assert!(close(g.contact_sections_mm2[1][1], 0.8));
    for (&i, &[load, section]) in g.active.iter().zip(&g.contact_sections_mm2) {
        assert!(close(load / section, d.contacts[i as usize].ratio));
        assert!(!close(
            load / (g.section_sum_mm2 / 2.0),
            d.contacts[i as usize].ratio
        ));
    }
}

#[test]
fn a_contact_not_yet_printed_gets_no_share() {
    let stack = body(&[8.0, 8.0, 8.0, 8.0]);
    let lay = layout(
        vec![
            contact(0.5, 2.0, 0, 1.0, 1.0),
            contact(1.5, 2.0, 2, 1.0, 1.0),
        ],
        1.0,
        1.0,
    );
    let d = analyse_full(&stack, &lay, &Extras::default());
    let by_layer = |k: u32| d.groups.iter().find(|g| g.layer == k).unwrap();
    assert_eq!(by_layer(0).active, vec![0]);
    assert_eq!(by_layer(1).active, vec![0]);
    assert_eq!(by_layer(2).active, vec![0, 1]);
    assert!(close(by_layer(1).loads_mm2[0], 8.0));
    // Layers 0 and 1 are all on contact 0: 8 mm² on a section of 1 is 8.
    assert!(close(
        d.sections
            .iter()
            .find(|s| s.chain == 0)
            .unwrap()
            .tributary
            .0,
        8.0
    ));
}

#[test]
fn a_bottom_step_is_found_as_the_worst_layer() {
    let stack = body(&[1.0, 1.0, 1.0, 1.0, 8.0, 1.0]);
    let lay = layout(vec![contact(1.0, 2.0, 0, 2.0, 2.0)], 2.0, 2.0);
    let d = analyse_full(&stack, &lay, &Extras::default());
    let s = d
        .sections
        .iter()
        .find(|s| s.kind == PartKind::Neck)
        .unwrap();
    assert_eq!(s.tributary.1, 4, "{s:?}");
    assert!(close(s.tributary.0, 4.0));
    assert_eq!(d.contacts[0].worst_layer, 4);
    assert_eq!(
        d.report
            .sections
            .iter()
            .find(|s| s.part == 1)
            .unwrap()
            .worst_layer,
        4
    );
    assert_eq!(d.peak.unwrap().layer, 4);
}

#[test]
fn the_supports_own_material_in_the_layer_is_counted_upstream_of_a_section() {
    let stack = body(&[4.0]);
    let mut lay = layout(vec![contact(1.0, 2.0, 0, 2.0, 0.5)], 0.5, 2.0);
    // Neck and column are both printed in layer 0.
    lay.chains[0].parts[1].z_range_mm = [0.0, 0.1];
    lay.chains[0].parts[2].z_range_mm = [0.0, 0.6];
    let d = analyse_full(&stack, &lay, &Extras::default());
    let neck = d
        .sections
        .iter()
        .find(|s| s.kind == PartKind::Neck)
        .unwrap();
    let col = d
        .sections
        .iter()
        .find(|s| s.kind == PartKind::Column)
        .unwrap();
    // Neck: (4 + its own 0.5) / 0.5. Column: (4 + 0.5 + its own 2.0) / 2.0.
    assert!(close(neck.tributary.0, 9.0), "{neck:?}");
    assert!(close(col.tributary.0, 3.25), "{col:?}");
}

#[test]
fn a_brace_adds_its_own_peel_to_the_carrier_column_and_its_bending_is_an_intervention() {
    let stack = body(&[4.0]);
    let mut lay = layout(vec![contact(1.0, 2.0, 0, 2.0, 2.0)], 2.0, 2.0);
    lay.chains[0].parts[2].z_range_mm = [0.0, 0.6];
    let base = analyse_full(&stack, &lay, &Extras::default());
    lay.chains.push(PartChain {
        contact: 0,
        parts: vec![part(PartKind::Brace, 0.4, [0.0, 0.3])],
    });
    let d = analyse_full(&stack, &lay, &Extras::default());
    let col = |d: &LoadDetail| {
        d.sections
            .iter()
            .find(|s| s.kind == PartKind::Column)
            .unwrap()
            .tributary
            .0
    };
    // The brace grows in the same layer: 0.4 / 2.0 more on the column's ratio.
    assert!(
        close(col(&d) - col(&base), 0.2),
        "{} {}",
        col(&d),
        col(&base)
    );
    assert!(
        kinds(&d.report, WarningClass::Intervention)
            .iter()
            .any(|k| k.contains("brace bending"))
    );
    assert!(
        !kinds(&base.report, WarningClass::Intervention)
            .iter()
            .any(|k| k.contains("brace"))
    );
}

#[test]
fn a_centre_off_a_straight_row_is_an_intervention_even_with_low_ratios() {
    // Body 3 x 4 mm; four contacts in a row along y = 0.2 with generous sections.
    let stack = body(&[12.0]);
    let cs: Vec<ContactView> = (0..4)
        .map(|i| contact(0.4 + 0.7 * f64::from(i), 0.2, 0, 10.0, 10.0))
        .collect();
    let d = analyse_full(&stack, &layout(cs, 10.0, 10.0), &Extras::default());
    let trib = d.sections.iter().map(|s| s.tributary.0).fold(0.0, f64::max);
    assert!(trib < 12.0, "{trib}");
    let w = kinds(&d.report, WarningClass::Intervention);
    assert!(w.iter().any(|k| k == "load path not assessed"), "{w:?}");
    assert!(d.groups[0].outside_hull_mm > 1.0);
}

#[test]
fn sixty_mm2_over_two_mm2_of_section_is_ratio_30_utilisation_2_5_and_unprintable() {
    let stack = body(&[60.0]);
    let d = analyse_full(
        &stack,
        &layout(vec![contact(1.0, 2.0, 0, 2.0, 2.0)], 2.0, 2.0),
        &Extras::default(),
    );
    let g = &d.groups[0];
    assert!(close(g.x_group, 30.0) && close(g.utilisation, 2.5), "{g:?}");
    assert!(
        kinds(&d.report, WarningClass::Unprintable)
            .iter()
            .any(|k| k == "additive area ratio beyond breaking")
    );
    // The unprintable case takes precedence: no intervention for the same layer.
    let iv = kinds(&d.report, WarningClass::Intervention);
    assert!(
        !iv.iter().any(|k| k == "load not held at design ratio"),
        "{iv:?}"
    );
    assert!(!iv.iter().any(|k| k == "load path not assessed"), "{iv:?}");
}

#[test]
fn positive_area_over_zero_section_is_unprintable_not_zero() {
    let stack = body(&[4.0]);
    let d = analyse_full(
        &stack,
        &layout(vec![contact(1.0, 2.0, 0, 0.0, 0.0)], 0.0, 0.0),
        &Extras::default(),
    );
    assert!(d.groups[0].x_group.is_infinite());
    assert!(
        kinds(&d.report, WarningClass::Unprintable)
            .iter()
            .any(|k| k == "additive area ratio beyond breaking")
    );
}

#[test]
fn a_component_with_no_active_contact_is_unprintable_never_zero_load() {
    let stack = body(&[4.0, 4.0]);
    let d = analyse_full(
        &stack,
        &layout(vec![contact(1.0, 2.0, 5, 2.0, 2.0)], 2.0, 2.0),
        &Extras::default(),
    );
    assert!(
        d.groups
            .iter()
            .all(|g| g.active.is_empty() && g.x_group.is_infinite())
    );
    let u = kinds(&d.report, WarningClass::Unprintable);
    assert!(
        u.iter()
            .any(|k| k.contains("island without a rooted first-layer anchor")),
        "{u:?}"
    );
}

#[test]
fn an_exempt_island_carries_nothing_and_raises_no_unprintable_warning() {
    // Component 1 is a small exempt island in layers 0..2 that merges into component 0 at 3.
    let mk = |k: usize| {
        let mut s = vec![slice(0, rect([0.0, 3.0], [0.0, 4.0], k as f64 * H, 6))];
        if k < 3 {
            s.push(slice(1, rect([5.0, 5.2], [0.0, 0.2], k as f64 * H, 2)));
        }
        layer(s)
    };
    let stack = LayerStack {
        layer_height_mm: H,
        layer_origin_mm: 0.0,
        pixel_pitch_mm: [0.014, 0.019],
        layers: (0..5).map(mk).collect(),
        components: vec![
            comp(0),
            ComponentInfo {
                first_layer: 0,
                merged_into: Some((0, 3)),
                is_island: true,
                exempt: true,
            },
        ],
    };
    let d = analyse_full(
        &stack,
        &layout(vec![contact(1.5, 2.0, 0, 5.0, 5.0)], 5.0, 5.0),
        &Extras::default(),
    );
    assert!(
        kinds(&d.report, WarningClass::Unprintable).is_empty(),
        "{:?}",
        d.report.warnings
    );
    // Its area is in no contact's load: the tributary loads in layer 0 are component 0's only.
    let g0 = d
        .groups
        .iter()
        .find(|g| g.layer == 0 && g.component == 0)
        .unwrap();
    assert!(close(g0.loads_mm2.iter().sum::<f64>(), 12.0));
}

#[test]
fn a_route_not_printed_by_the_exposure_layer_is_unprintable() {
    let stack = body(&[4.0, 4.0]);
    let mut lay = layout(vec![contact(1.0, 2.0, 0, 2.0, 2.0)], 2.0, 2.0);
    lay.chains[0].parts[2].z_range_mm = [1.0, 2.0];
    let d = analyse_full(&stack, &lay, &Extras::default());
    let u = kinds(&d.report, WarningClass::Unprintable);
    assert!(
        u.iter()
            .any(|k| k == "no material route printed by the exposure layer"),
        "{u:?}"
    );
}

#[test]
fn an_exact_tie_splits_the_element_equally() {
    // One element exactly between two contacts.
    let stack = LayerStack {
        layer_height_mm: H,
        layer_origin_mm: 0.0,
        pixel_pitch_mm: [0.014, 0.019],
        layers: vec![layer(vec![slice(
            0,
            vec![SourceElement {
                xyz: [1.0, 0.0, 0.0],
                area: 2.0,
            }],
        )])],
        components: vec![comp(0)],
    };
    let d = analyse_full(
        &stack,
        &layout(
            vec![
                contact(0.0, 0.0, 0, 2.0, 2.0),
                contact(2.0, 0.0, 0, 2.0, 2.0),
            ],
            2.0,
            2.0,
        ),
        &Extras::default(),
    );
    assert!(
        d.groups[0].loads_mm2.iter().all(|&l| close(l, 1.0)),
        "{:?}",
        d.groups[0].loads_mm2
    );
}

#[test]
fn the_patch_counts_only_as_far_as_it_is_printed() {
    // A patch spread over 0.5 mm of height: in the first layer only a sliver is printed, so a
    // load of 4 mm² against about 0.3 mm² of the 6 mm² patch is above 12.
    let stack = body(&[4.0, 4.0, 4.0]);
    let mut lay = layout(vec![contact(0.2, 2.0, 0, 6.0, 6.0)], 6.0, 6.0);
    lay.chains[0].parts[0].z_range_mm = [0.0, 0.5];
    let d = analyse_full(&stack, &lay, &Extras::default());
    let patch = d
        .sections
        .iter()
        .find(|s| s.kind == PartKind::Patch)
        .unwrap();
    assert_eq!(patch.tributary.1, 0, "{patch:?}");
    assert!(patch.tributary.0 > 12.0, "{patch:?}");
    assert!(
        kinds(&d.report, WarningClass::Intervention)
            .iter()
            .any(|k| k == "load not held at design ratio")
    );
}

#[test]
fn an_overhang_sample_without_a_contact_within_reach_is_an_intervention() {
    let stack = body(&[4.0]);
    let lay = layout(vec![contact(0.0, 0.0, 0, 2.0, 2.0)], 2.0, 2.0);
    let mk = |x: f64, class| NeedPoint {
        xyz: [x, 0.0, 0.0],
        normal: [0.0, 0.0, -1.0],
        angle_deg: 0.0,
        area_mm2: 0.1,
        triangle: 0,
        class,
    };
    let needs = NeedsMap {
        points: vec![
            mk(1.0, SiteTag::Hidden),
            mk(5.0, SiteTag::PartlyVisible),
            mk(9.0, SiteTag::Top),
        ],
        limit_deg: 45.0,
        spacing_mm: 0.25,
    };
    let d = analyse_full(
        &stack,
        &lay,
        &Extras {
            structure: None,
            needs: Some(&needs),
        },
    );
    let cov = d.coverage.unwrap();
    assert_eq!((cov.needing, cov.covered), (2, 1));
    assert!(close(cov.uncovered_mm2, 0.1));
    assert!(
        kinds(&d.report, WarningClass::Intervention)
            .iter()
            .any(|k| k.starts_with("overhang without a contact"))
    );
}

/// Contacts on the underside of Round 25 by a greedy spacing rule: a stand-in layout for the
/// load model, not a plan (the planner is lane A's).
fn synthetic_round25_layout(ml: &ModelLayers, needs: &NeedsMap, spacing: f64) -> LayoutView {
    let mut pts: Vec<&NeedPoint> = needs.points.iter().collect();
    pts.sort_by(|a, b| {
        a.xyz[2]
            .total_cmp(&b.xyz[2])
            .then(a.xyz[0].total_cmp(&b.xyz[0]))
    });
    let mut chosen: Vec<&NeedPoint> = Vec::new();
    for p in pts {
        let near = chosen.iter().any(|c| {
            (0..3).map(|a| (c.xyz[a] - p.xyz[a]).powi(2)).sum::<f64>() < spacing * spacing
        });
        if !near {
            chosen.push(p);
        }
    }
    let h = ml.grid.height;
    let mut contacts = Vec::new();
    let mut chains = Vec::new();
    for (i, p) in chosen.iter().enumerate() {
        let k = ((p.xyz[2] - ml.grid.origin) / h).floor().max(0.0) as u32;
        let z = p.xyz[2];
        contacts.push(ContactView {
            centre: [
                p.xyz[0] + 0.1 * p.normal[0],
                p.xyz[1] + 0.1 * p.normal[1],
                z + 0.1 * p.normal[2],
            ],
            patch_lower_mm2: 0.3,
            neck_area_mm2: 0.12,
            first_layer: k,
            site_class: SiteClass::HiddenUnderside,
        });
        chains.push(PartChain {
            contact: i as u32,
            parts: vec![
                part(PartKind::Patch, 0.3, [z, z + 0.2]),
                part(PartKind::Neck, 0.12, [z - 0.3, z]),
                part(PartKind::Column, 0.2, [ml.grid.origin - 5.0, z - 0.3]),
            ],
        });
    }
    LayoutView {
        contacts,
        chains,
        patch_history: Vec::new(),
    }
}

#[test]
fn round25_runs_through_the_whole_model_with_a_stand_in_layout() {
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/round/approved/Round 25.keepup.stl"
    );
    let mesh = plinth_geom::read_stl_file(std::path::Path::new(p)).unwrap();
    let grid = plinth_geom::slice::LayerGrid::covering(&mesh, 0.05).unwrap();
    let ml = match build_model_layers(&mesh, &grid, &StackOptions::default()) {
        plinth_geom::measure::Measure::Known(m) => m,
        plinth_geom::measure::Measure::Unverified(w) => panic!("{w}"),
    };
    let needs = needs::overhang_points(&mesh, needs::OVERHANG_DEG, needs::SAMPLE_SPACING_MM);
    let layout = synthetic_round25_layout(&ml, &needs, 2.0);
    assert!(layout.contacts.len() > 20, "{}", layout.contacts.len());
    let t = std::time::Instant::now();
    let d = analyse_full(
        &ml.stack,
        &layout,
        &Extras {
            structure: Some(&ml.structure),
            needs: Some(&needs),
        },
    );
    let secs = t.elapsed().as_secs_f64();
    eprintln!(
        "contacts {} analysed in {secs:.2} s, warnings {}",
        layout.contacts.len(),
        d.report.warnings.len()
    );
    for w in &d.report.warnings {
        eprintln!(
            "{:?} {} layers {:?} contacts {}",
            w.class,
            w.kind,
            w.layers,
            w.contacts.len()
        );
    }
    assert!(d.report.is_computed());
    assert!(secs < 1.5, "{secs}");
    assert_eq!(d.report.layers.len(), 465);
    // Every layer's shares add up to the layer's area.
    for g in d.groups.iter().filter(|g| !g.active.is_empty()) {
        assert!((g.loads_mm2.iter().sum::<f64>() - g.area_mm2).abs() < 1e-9);
    }
    // The stand-in layout has no contact for the rib island's first layer unless the greedy
    // rule happened to put one there; the report must say which.
    assert!(d.peak.is_some());
}
