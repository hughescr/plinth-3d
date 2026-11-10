//! Warnings of design `step11-construction.md` section 4.8 from a finished layer pass.
//!
//! When one (component, layer) matches an unprintable row and an intervention row, it is
//! unprintable as is: the intervention is not reported for it.

use std::collections::BTreeSet;

use super::analyse::{Collected, Extras, GroupLoad, LoadDetail};
use super::data::{LayerStack, LayoutView, LoadWarning, PartKind, WarningClass};
use super::needs::check_coverage;

/// Beyond this additive area ratio a component is unprintable as is. It is the owner's limit
/// (ruling 2026-11-10), not a measured breaking point: supports survived at ratio about 33.5
/// (ChituBox Better supports on Round 25) and 60.15 (CAL-1 K-60); 40 keeps margin below 60.
pub const UNPRINTABLE_RATIO: f64 = 40.0;
/// A layer's centroid farther than this from the spread of the active contacts is outside, mm.
const OUTSIDE_EPS_MM: f64 = 1e-6;
/// A rigid-lever residual above this means the layer's load cannot be balanced.
const LEVER_RESIDUAL_LIMIT: f64 = super::lever::EQUILIBRIUM_LIMIT;
/// Load centred farther than this from its contact makes the neck bend, mm **[U]**.
pub const NECK_OFFSET_MM: f64 = 2.0;
/// A column longer than this, mm, needs bracing by the owner's Light rule **[O]**.
pub const LONG_COLUMN_MM: f64 = 4.0;

fn layers_of(v: &[u32], worst: u32) -> Vec<u32> {
    let mut out: Vec<u32> = v
        .first()
        .into_iter()
        .chain(Some(&worst))
        .chain(v.last())
        .copied()
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

fn warn(
    class: WarningClass,
    kind: &str,
    layers: Vec<u32>,
    contacts: Vec<u32>,
    message: String,
) -> LoadWarning {
    LoadWarning {
        class,
        kind: kind.to_owned(),
        layers,
        contacts,
        location_mm: None,
        message,
    }
}

/// All warnings, unprintable first, then interventions, then information.
pub(super) fn all(
    detail: &LoadDetail,
    col: &Collected,
    stack: &LayerStack,
    layout: &LayoutView,
    extras: &Extras<'_>,
) -> (Vec<LoadWarning>, Option<super::needs::Coverage>) {
    let mut out = Vec::new();
    let mut blocked: BTreeSet<(u32, u32)> = BTreeSet::new();
    unsupported(col, stack, &mut out, &mut blocked);
    broken_routes(col, &mut out, &mut blocked);
    beyond_unprintable_ratio(&detail.groups, &mut out, &mut blocked);
    sizing_over(detail, &blocked, &mut out);
    sharing_over(detail, layout, &mut out);
    not_assessed(&detail.groups, &blocked, &mut out);
    local_transfer(&detail.groups, stack, &blocked, &mut out);
    structural_unknowns(detail, layout, &mut out);
    let mut coverage = None;
    if let Some(s) = extras.structure {
        out.extend(s.warnings());
    }
    if let Some(n) = extras.needs {
        let (cov, w) = check_coverage(n, layout, stack);
        coverage = Some(cov);
        out.extend(w);
    }
    let rank = |c: WarningClass| match c {
        WarningClass::Unprintable => 0,
        WarningClass::Intervention => 1,
        WarningClass::Information => 2,
    };
    out.sort_by_key(|w| rank(w.class));
    (out, coverage)
}

fn unsupported(
    col: &Collected,
    stack: &LayerStack,
    out: &mut Vec<LoadWarning>,
    blocked: &mut BTreeSet<(u32, u32)>,
) {
    let comps: BTreeSet<u32> = col.unsupported.iter().map(|u| u.component).collect();
    for c in comps {
        let rows: Vec<_> = col
            .unsupported
            .iter()
            .filter(|u| u.component == c)
            .collect();
        // A small P18 island is exempt while it is separate: it carries nothing.
        let exempt = stack.components.get(c as usize).is_some_and(|i| {
            i.exempt
                && i.merged_into
                    .is_none_or(|(_, at)| rows.iter().all(|u| u.layer < at))
        });
        if exempt {
            continue;
        }
        let layers: Vec<u32> = rows.iter().map(|u| u.layer).collect();
        let big = rows.iter().max_by(|a, b| a.area.total_cmp(&b.area));
        let first = stack
            .components
            .get(c as usize)
            .map_or(0, |i| i.first_layer);
        let anchor = layers.first() == Some(&first);
        let (kind, msg) = if anchor {
            (
                "island without a rooted first-layer anchor",
                format!(
                    "Component {c} first appears in layer {first} with no contact on it, and \
                     has no active support section in {} layers (up to {:.2} mm²). The layout \
                     holds no anchor for it. Whether a legal anchor exists was not searched \
                     here. Only changed geometry clears this.",
                    layers.len(),
                    big.map_or(0.0, |u| u.area)
                ),
            )
        } else {
            (
                "positive peel area with no active support section",
                format!(
                    "Component {c} has peel area (up to {:.2} mm²) in {} layers with no active \
                     support section. Its load is not zero; it is not held.",
                    big.map_or(0.0, |u| u.area),
                    layers.len()
                ),
            )
        };
        for &l in &layers {
            blocked.insert((c, l));
        }
        out.push(warn(
            WarningClass::Unprintable,
            kind,
            layers_of(&layers, big.map_or(0, |u| u.layer)),
            Vec::new(),
            msg,
        ));
    }
}

fn broken_routes(col: &Collected, out: &mut Vec<LoadWarning>, blocked: &mut BTreeSet<(u32, u32)>) {
    if col.broken.is_empty() {
        return;
    }
    let mut contacts: Vec<u32> = col.broken.iter().map(|b| b.contact).collect();
    contacts.sort_unstable();
    contacts.dedup();
    let layers: Vec<u32> = col.broken.iter().map(|b| b.layer).collect();
    for b in &col.broken {
        blocked.insert((b.component, b.layer));
    }
    out.push(warn(
        WarningClass::Unprintable,
        "no material route printed by the exposure layer",
        layers_of(&layers, layers[0]),
        contacts.clone(),
        format!(
            "{} contact(s) have a part of their route to the plate that is not printed yet \
             when the layers above load them. Nothing later carries anything earlier. Only \
             changed geometry clears this.",
            contacts.len()
        ),
    ));
}

fn beyond_unprintable_ratio(
    groups: &[GroupLoad],
    out: &mut Vec<LoadWarning>,
    blocked: &mut BTreeSet<(u32, u32)>,
) {
    let comps: BTreeSet<u32> = groups
        .iter()
        .filter(|g| !g.active.is_empty() && g.x_group > UNPRINTABLE_RATIO)
        .map(|g| g.component)
        .collect();
    for c in comps {
        let rows: Vec<&GroupLoad> = groups
            .iter()
            .filter(|g| g.component == c && !g.active.is_empty() && g.x_group > UNPRINTABLE_RATIO)
            .collect();
        let Some(worst) = rows.iter().max_by(|a, b| a.x_group.total_cmp(&b.x_group)) else {
            continue;
        };
        let layers: Vec<u32> = rows.iter().map(|g| g.layer).collect();
        for g in &rows {
            blocked.insert((c, g.layer));
        }
        out.push(warn(
            WarningClass::Unprintable,
            "additive area ratio beyond the unprintable limit",
            layers_of(&layers, worst.layer),
            worst.active.clone(),
            format!(
                "Component {c}: in layer {} the peel area {:.2} mm² over the additive section \
                 {:.3} mm² is a ratio of {:.1} (utilisation {:.2} of the design ratio), beyond \
                 the owner's limit of {UNPRINTABLE_RATIO}. The limit is not a measured breaking \
                 point: supports survived at ratios of about 33.5 and 60, and {UNPRINTABLE_RATIO} \
                 keeps margin below 60. {} layers are affected.",
                worst.layer,
                worst.area_mm2,
                worst.section_sum_mm2,
                worst.x_group,
                worst.utilisation,
                layers.len()
            ),
        ));
    }
}

fn component_blocked(blocked: &BTreeSet<(u32, u32)>, comp: Option<u32>, lo: u32, hi: u32) -> bool {
    comp.is_some_and(|c| blocked.range((c, lo)..=(c, hi)).next().is_some())
}

fn sizing_over(detail: &LoadDetail, blocked: &BTreeSet<(u32, u32)>, out: &mut Vec<LoadWarning>) {
    for c in detail.contacts.iter().filter(|c| c.layers_over > 0) {
        let (lo, hi) = (c.first_over.unwrap_or(0), c.last_over.unwrap_or(0));
        if component_blocked(blocked, c.component, lo, hi) {
            continue;
        }
        out.push(warn(
            WarningClass::Intervention,
            "load not held at design ratio",
            layers_of(&[lo, hi], c.worst_layer),
            vec![c.contact],
            format!(
                "Contact {}: the rigid-carrier/additive sizing ratio reaches {:.1} in layer {} ({:.3} mm² of peel \
                 area, centred at ({:.2}, {:.2}) mm), above the design ratio of {}, in {} \
                 layers. It is a sizing result, not a load bound. Add a ChituBox support near \
                 that place, or accept the stated risk.",
                c.contact,
                c.ratio,
                c.worst_layer,
                c.load_mm2,
                c.tributary_xy[0],
                c.tributary_xy[1],
                super::DESIGN_RATIO,
                c.layers_over
            ),
        ));
    }
}

fn sharing_over(detail: &LoadDetail, layout: &LayoutView, out: &mut Vec<LoadWarning>) {
    type Pick = fn(&super::analyse::SectionDetail) -> (f64, u32);
    // The additive result above the unprintable ratio is unprintable and reported there.
    let models: [(&str, Pick, f64); 3] = [
        (
            "additive sharing exceeds the design ratio",
            |s| s.additive,
            UNPRINTABLE_RATIO,
        ),
        (
            "equal-share result exceeds the design ratio",
            |s| s.equal_share,
            f64::INFINITY,
        ),
        (
            "rigid-lever index exceeds the design ratio",
            |s| s.rigid_lever,
            f64::INFINITY,
        ),
    ];
    for (kind, pick, cap) in models {
        let over: Vec<_> = detail
            .sections
            .iter()
            .filter(|s| pick(s).0 > super::DESIGN_RATIO + 1e-9 && pick(s).0 <= cap)
            .collect();
        let Some(worst) = over.iter().max_by(|a, b| pick(a).0.total_cmp(&pick(b).0)) else {
            continue;
        };
        let mut contacts: Vec<u32> = over.iter().map(|s| s.chain).collect();
        contacts.sort_unstable();
        contacts.dedup();
        let layers: Vec<u32> = over.iter().map(|s| pick(s).1).collect();
        let at = layout
            .contacts
            .get(worst.chain as usize)
            .map_or([0.0; 3], |c| c.centre);
        out.push(warn(
            WarningClass::Intervention,
            kind,
            layers_of(
                &{
                    let mut l = layers;
                    l.sort_unstable();
                    l
                },
                pick(worst).1,
            ),
            contacts.clone(),
            format!(
                "{} contact(s): the section {:?} of contact {} reaches {:.1} in layer {} \
                 (contact at ({:.2}, {:.2}, {:.2}) mm). Rigid-carrier and additive size contacts; equal-share is a sensitivity \
                 check. A valid result above {} is an intervention. Add a ChituBox support \
                 near the marked contacts, or accept the stated risk.",
                contacts.len(),
                worst.kind,
                worst.chain,
                pick(worst).0,
                pick(worst).1,
                at[0],
                at[1],
                at[2],
                super::DESIGN_RATIO
            ),
        ));
    }
}

fn not_assessed(groups: &[GroupLoad], blocked: &BTreeSet<(u32, u32)>, out: &mut Vec<LoadWarning>) {
    let bad = |g: &&GroupLoad| {
        !g.active.is_empty()
            && !blocked.contains(&(g.component, g.layer))
            && (!g.equilibrium_assessed
                || g.outside_hull_mm > OUTSIDE_EPS_MM
                || g.force_residual > LEVER_RESIDUAL_LIMIT
                || g.moment_residual > LEVER_RESIDUAL_LIMIT)
    };
    let comps: BTreeSet<u32> = groups.iter().filter(bad).map(|g| g.component).collect();
    for c in comps {
        let rows: Vec<&GroupLoad> = groups
            .iter()
            .filter(|g| g.component == c)
            .filter(bad)
            .collect();
        let Some(worst) = rows.iter().max_by(|a, b| {
            a.outside_hull_mm
                .max(a.lever_residual)
                .total_cmp(&b.outside_hull_mm.max(b.lever_residual))
        }) else {
            continue;
        };
        let layers: Vec<u32> = rows.iter().map(|g| g.layer).collect();
        let outside = rows
            .iter()
            .filter(|g| g.outside_hull_mm > OUTSIDE_EPS_MM)
            .count();
        out.push(warn(
            WarningClass::Intervention,
            "load path not assessed",
            layers_of(&layers, worst.layer),
            worst.active.clone(),
            format!(
                "Component {c}, {} layers: the load of a layer cannot be balanced by the active \
                 contacts as the model assumes. In layer {} the layer's centre ({:.2}, {:.2}) mm \
                 is {:.2} mm outside the spread of its {} active contacts (rigid-lever residual \
                 {:.2}); {} layers have the centre outside. Conditioning at that layer: {} \
                 (limit 1e6 [S]); 0.01 mm position sensitivity {} (limit 0.25 of stiffness-proportional share [S]); relative force residual {}, normalised moment residual {} \
                 (limits 1e-6 [S]). Singular, ill-conditioned or unbalanced solves are unresolved, \
                 NOT a low-ratio pass; any provisional area-weighted forces only guide search. What is unknown: how the load \
                 really reaches the contacts and which of them take the moment. Add a support \
                 near the layer's centre or accept the stated risk.",
                layers.len(),
                worst.layer,
                worst.centroid_xy[0],
                worst.centroid_xy[1],
                worst.outside_hull_mm,
                worst.active.len(),
                worst.lever_residual,
                outside,
                worst.condition,
                worst.position_sensitivity,
                worst.force_residual,
                worst.moment_residual
            ),
        ));
    }
}

fn local_transfer(
    groups: &[GroupLoad],
    stack: &LayerStack,
    blocked: &BTreeSet<(u32, u32)>,
    out: &mut Vec<LoadWarning>,
) {
    // A geometric locator, NOT a transfer certificate: first four exposures and
    // source-centre spans <=1 mm [S] identify growing thin edges. Keep separate
    // contiguous runs; never locate an edge at the whole component's area peak.
    for (c, info) in stack.components.iter().enumerate() {
        let c = c as u32;
        let rows: Vec<_> = groups
            .iter()
            .filter(|g| {
                if g.component != c || blocked.contains(&(c, g.layer)) {
                    return false;
                }
                let source = stack
                    .layers
                    .get(g.layer as usize)
                    .and_then(|l| l.components.iter().find(|s| s.component == c));
                let narrow = source.is_some_and(|s| {
                    (0..2).any(|axis| {
                        let (lo, hi) = s
                            .sources
                            .iter()
                            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                                (lo.min(p.xyz[axis]), hi.max(p.xyz[axis]))
                            });
                        hi - lo <= 1.0
                    })
                });
                g.layer < info.first_layer.saturating_add(4) || narrow
            })
            .collect();
        let mut start = 0;
        while start < rows.len() {
            let mut end = start + 1;
            while end < rows.len() && rows[end].layer == rows[end - 1].layer + 1 {
                end += 1;
            }
            let g = rows[start];
            let region = if info.first_layer == 0 {
                "lip / growing outer edge (picture 03 or 12)"
            } else {
                "rib / growing island edge (picture 04)"
            };
            let mut w = warn(
                WarningClass::Intervention,
                "local load path not assessed",
                vec![g.layer, rows[end - 1].layer],
                g.active.clone(),
                format!(
                    "Component {c}, {region}, layers {}–{}: local transfer through this thin growing edge is not assessed. Marker local transfer L{} at ({:.2}, {:.2}) mm (source centre, not the largest-area layer). First four exposures or source span <=1 mm [S] trigger this conservative locator, not a stiffness test. Global ratios do not certify local transfer. Add support at this edge or accept the unmeasured transfer; no absent route is claimed.",
                    g.layer,
                    rows[end - 1].layer,
                    g.layer,
                    g.centroid_xy[0],
                    g.centroid_xy[1]
                ),
            );
            w.location_mm = Some([
                g.centroid_xy[0],
                g.centroid_xy[1],
                stack.layer_origin_mm + (f64::from(g.layer) + 0.5) * stack.layer_height_mm,
            ]);
            out.push(w);
            start = end;
        }
        let Some((parent, at)) = info.merged_into else {
            continue;
        };
        let Some(prior) = at.checked_sub(1).and_then(|k| stack.layers.get(k as usize)) else {
            continue;
        };
        let child = prior.components.iter().find(|s| s.component == c);
        let carrier = prior.components.iter().find(|s| s.component == parent);
        let Some((a, b)) = child.zip(carrier).and_then(|(a, b)| {
            a.sources
                .iter()
                .flat_map(|a| b.sources.iter().map(move |b| (a, b)))
                .min_by(|(a, b), (x, y)| {
                    let d = |p: &super::data::SourceElement, q: &super::data::SourceElement| {
                        (p.xyz[0] - q.xyz[0]).hypot(p.xyz[1] - q.xyz[1])
                    };
                    d(a, b).total_cmp(&d(x, y))
                })
        }) else {
            continue;
        };
        let xyz = [
            (a.xyz[0] + b.xyz[0]) * 0.5,
            (a.xyz[1] + b.xyz[1]) * 0.5,
            stack.layer_origin_mm + (f64::from(at) + 0.5) * stack.layer_height_mm,
        ];
        let active = groups
            .iter()
            .find(|g| g.component == parent && g.layer == at)
            .map_or_else(Vec::new, |g| g.active.clone());
        let mut w = warn(
            WarningClass::Intervention,
            "local load path not assessed",
            vec![at],
            active,
            format!(
                "Rib/island {c} to carrier {parent} connection, marker local transfer L{at}, layer {at}, picture 04: transfer through the newly joined material is not assessed. Join locator ({:.2}, {:.2}, {:.2}) mm is the midpoint of the closest pre-merge source cells, not a measured connection width. Separate island capacity is checked before merger; pooled global ratios after merger do not certify this local join. Add a support at the join or accept this unmeasured transfer.",
                xyz[0], xyz[1], xyz[2]
            ),
        );
        w.location_mm = Some(xyz);
        out.push(w);
    }
}

fn structural_unknowns(detail: &LoadDetail, layout: &LayoutView, out: &mut Vec<LoadWarning>) {
    let n = layout.contacts.len();
    let carriers: BTreeSet<u32> = layout.chains.iter().skip(n).map(|c| c.contact).collect();
    if !carriers.is_empty() {
        out.push(warn(
            WarningClass::Intervention,
            "load path not assessed: brace bending and redistribution",
            Vec::new(),
            carriers.iter().copied().collect(),
            format!(
                "{} column(s) carry braces. The brace's own peel is counted on its carrier \
                 column, but the brace's bending and any redistribution of model load it causes \
                 are not assessed.",
                carriers.len()
            ),
        ));
    }
    let bent: Vec<_> = detail
        .contacts
        .iter()
        .filter(|c| c.offset_mm > NECK_OFFSET_MM)
        .collect();
    if let Some(w) = bent
        .iter()
        .max_by(|a, b| a.offset_mm.total_cmp(&b.offset_mm))
    {
        out.push(warn(
            WarningClass::Intervention,
            "load path not assessed: neck bending",
            vec![w.worst_layer],
            bent.iter().map(|c| c.contact).collect(),
            format!(
                "{} contact(s) take load from an area centred more than {NECK_OFFSET_MM} mm away \
                 (up to {:.2} mm for contact {}). The model has no neck bending check, so the \
                 neck's bending under that offset is unknown.",
                bent.len(),
                w.offset_mm,
                w.contact
            ),
        ));
    }
    let long: Vec<u32> = layout
        .chains
        .iter()
        .take(n)
        .filter(|ch| {
            !carriers.contains(&ch.contact)
                && ch.parts.iter().any(|p| {
                    p.kind == PartKind::Column && p.z_range_mm[1] - p.z_range_mm[0] > LONG_COLUMN_MM
                })
        })
        .map(|ch| ch.contact)
        .collect();
    if !long.is_empty() {
        out.push(warn(
            WarningClass::Intervention,
            "long column without bracing",
            Vec::new(),
            long.clone(),
            format!(
                "{} column(s) are longer than {LONG_COLUMN_MM} mm with no brace. The owner's \
                 Light profile braces them; no measured slenderness rule exists, so their \
                 stability cannot be judged here.",
                long.len()
            ),
        ));
    }
}
