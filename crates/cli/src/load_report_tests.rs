use super::*;
use plinth_support::load::{self, analyse::ContactState, *};

fn group() -> GroupLoad {
    let stack = LayerStack {
        layer_height_mm: 0.05,
        layers: vec![LayerArea {
            components: vec![ComponentSlice {
                component: 0,
                area: 1.0,
                sources: vec![SourceElement {
                    xyz: [0.0; 3],
                    area: 1.0,
                }],
            }],
            ..LayerArea::default()
        }],
        components: vec![ComponentInfo::default()],
        ..LayerStack::default()
    };
    load::analyse_full(&stack, &LayoutView::default(), &Extras::default())
        .groups
        .remove(0)
}
fn state(contact: u32, ratios: Sharing) -> ContactState {
    ContactState {
        contact,
        part: 0,
        patch_mm2: 0.2,
        section_mm2: 0.2,
        load_mm2: 1.0,
        ratios,
    }
}
#[test]
fn critical_region_uses_the_worst_sizing_contact_and_exposure() {
    let mut early = group();
    early.layer = 0;
    early.x_group = 10.0;
    early.equilibrium_assessed = true;
    early.states = vec![state(
        0,
        Sharing {
            additive: 10.0,
            ..Sharing::default()
        },
    )];
    let mut later = early.clone();
    later.layer = 52;
    later.x_group = 7.0;
    later.states = vec![
        state(
            0,
            Sharing {
                rigid_lever: 22.0,
                ..Sharing::default()
            },
        ),
        state(
            32,
            Sharing {
                equal_share: 37.0,
                ..Sharing::default()
            },
        ),
    ];
    let groups = [early, later];
    let choose = |groups: &[GroupLoad]| {
        worst_in_region(
            groups
                .iter()
                .flat_map(|g| g.states.iter().map(move |s| (g, s))),
        )
        .map(|(g, s)| (s.contact, g.layer))
    };
    assert_eq!(
        choose(&groups),
        Some((32, 52)),
        "neither earliest contact nor additive peak is critical"
    );
    let mut groups = groups;
    groups[0].states[0].ratios.tributary = 40.0;
    assert_eq!(
        choose(&groups),
        Some((32, 52)),
        "diagnostic tributary cannot hide the critical sizing location"
    );
    groups[0].states[0].ratios.tributary = 0.0;
    groups[0].states[0].ratios.rigid_lever = 100.0;
    groups[0].equilibrium_assessed = false;
    assert_eq!(
        choose(&groups),
        Some((32, 52)),
        "unresolved rigid-lever values cannot win"
    );
}
