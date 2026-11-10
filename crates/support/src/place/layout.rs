use crate::construct::{Contact, PartFields, PartKind as NativeKind};
use crate::load::{
    ContactView, LayerStack, LayoutView, PartChain, PartKind, PartSection, SiteClass,
};

/// Read-only load view of rounded admitted or quoted contacts, in the plate frame.
/// Early patch growth uses the load model's disc approximation **[U]**. It is not
/// an emitted-file certificate, and cannot establish print eligibility.
#[must_use]
pub fn layout_from_contacts(
    contacts: &[&Contact],
    classes: &[SiteClass],
    stack: &LayerStack,
    shift: f64,
) -> LayoutView {
    let mut layout = LayoutView::default();
    for (i, c) in contacts.iter().enumerate() {
        let mut centre = c.centre().map(f64::from);
        centre[2] += shift;
        let mut parts = Vec::new();
        let mut neck = c.patch().lower();
        for part in c.parts() {
            let (kind, area) = match part.fields() {
                PartFields::Ball { radius, .. } => {
                    if part.kind() == NativeKind::TipBall {
                        (PartKind::Patch, c.patch().lower())
                    } else {
                        (
                            PartKind::JointThroat,
                            std::f64::consts::PI * f64::from(radius).powi(2),
                        )
                    }
                }
                PartFields::Frustum { r1, r2, .. } => {
                    let area =
                        8. * f64::from(r1.min(r2)).powi(2) * (std::f64::consts::PI / 8.).sin();
                    neck = neck.min(area);
                    (PartKind::Neck, area)
                }
                _ => continue,
            };
            let lo = part
                .mesh()
                .iter()
                .flatten()
                .map(|v| f64::from(v[2]) + shift)
                .fold(f64::INFINITY, f64::min);
            let hi = part
                .mesh()
                .iter()
                .flatten()
                .map(|v| f64::from(v[2]) + shift)
                .fold(f64::NEG_INFINITY, f64::max);
            parts.push(PartSection {
                kind,
                record: Some((i * 3 + parts.len()) as u32),
                area_lower_mm2: area,
                z_range_mm: [lo, hi],
            });
        }
        let low = parts.first().map_or(centre[2], |p| p.z_range_mm[0]);
        let k = ((low - stack.layer_origin_mm) / stack.layer_height_mm)
            .floor()
            .max(0.) as u32;
        layout.contacts.push(ContactView {
            centre,
            patch_lower_mm2: c.patch().lower(),
            neck_area_mm2: neck,
            first_layer: k,
            site_class: classes
                .get(i)
                .copied()
                .unwrap_or(SiteClass::PartlyVisibleRim),
        });
        layout.chains.push(PartChain {
            contact: i as u32,
            parts,
        });
    }
    layout
}

/// Append the exact fixed members to the contact load view. Shared by the final
/// private replay and CLI report so they use identical self-peel inputs.
pub fn add_members(plan: &crate::construct::SupportPlan, layout: &mut LayoutView) {
    let shift = -f64::from(plan.model().plate_z());
    for (record, part) in plan
        .parts()
        .iter()
        .enumerate()
        .skip(plan.contacts().len() * 3)
    {
        let PartFields::Frustum { start, end, r1, r2 } = part.fields() else {
            continue;
        };
        let vertical = (start[0] - end[0]).abs() < 1e-6 && (start[1] - end[1]).abs() < 1e-6;
        let kind = if part.kind() == NativeKind::BaseFlare {
            PartKind::Flare
        } else if vertical {
            PartKind::Column
        } else {
            PartKind::Brace
        };
        let low = if start[2] < end[2] { start } else { end };
        let owner = plan.contacts().iter().position(|c| {
            let j = c.joint();
            (j[0] - low[0]).abs() < 1e-5 && (j[1] - low[1]).abs() < 1e-5
        });
        let Some(owner) = owner else { continue };
        let section = PartSection {
            kind,
            record: Some(record as u32),
            area_lower_mm2: 8. * f64::from(r1.min(r2)).powi(2) * (std::f64::consts::PI / 8.).sin(),
            z_range_mm: [
                f64::from(start[2].min(end[2])) + shift,
                f64::from(start[2].max(end[2])) + shift,
            ],
        };
        if kind == PartKind::Brace {
            layout.chains.push(PartChain {
                contact: owner as u32,
                parts: vec![section],
            });
        } else {
            layout.chains[owner].parts.push(section);
        }
    }
}
