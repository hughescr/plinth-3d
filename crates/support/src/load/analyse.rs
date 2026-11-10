//! The FR-20a load model (design `step11-construction.md` section 4), layer by layer.
//!
//! For each layer and each print-order component:
//!
//! * tension-only rigid-carrier springs (stiffness proportional to certified
//!   section available in that layer), together with the additive requirement;
//! * nearest-3D tributary is a sensitivity diagnostic ONLY, beside equal share;
//! * the load-centroid test against the spread of the active contacts.
//!
//! One layer peels at a time: nothing is summed over layers. A contact is active from the
//! layer where its ball first overlaps the model, and only the patch printed by then counts
//! (a disc in the plane tilted as the patch's height range says, **[U]**).

use std::collections::BTreeMap;

use super::data::{
    ContactView, LayerLoad, LayerStack, LayoutView, LoadReport, LoadStatus, PartChain, PartKind,
    PartSection, SectionLoad, Sharing,
};
use super::lever::{Spring, distance_to_hull, rigid_lever};
use super::needs::{Coverage, NeedsMap};
use super::structure::Structure;

/// Optional inputs besides the stack and the layout.
#[derive(Debug, Clone, Copy, Default)]
pub struct Extras<'a> {
    /// Islands, cups and reach of the model.
    pub structure: Option<&'a Structure>,
    /// The overhang samples, to check cover.
    pub needs: Option<&'a NeedsMap>,
}

/// One contact's worst case.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactLoad {
    /// Index into [`LayoutView::contacts`].
    pub contact: u32,
    /// The model component it holds (at its first layer), if it touches one.
    pub component: Option<u32>,
    /// First layer in which it is active.
    pub first_layer: u32,
    /// Layer with the largest rigid-carrier/additive sizing ratio over its sections.
    pub worst_layer: u32,
    /// Reaction plus upstream self-peel, mm² (provisional if the solve is unresolved).
    pub load_mm2: f64,
    /// Largest rigid-carrier/additive sizing ratio; unresolved solves remain flagged.
    pub ratio: f64,
    /// Where its tributary area is centred at that layer (x, y), mm.
    pub tributary_xy: [f64; 2],
    /// Distance from the contact to that centre, mm.
    pub offset_mm: f64,
    /// Number of layers with a sizing ratio above the design ratio.
    pub layers_over: u32,
    /// First layer above the design ratio, if any.
    pub first_over: Option<u32>,
    /// Last layer above the design ratio, if any.
    pub last_over: Option<u32>,
}

/// The worst layer of each sharing result for one section.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionDetail {
    /// Chain index.
    pub chain: u32,
    /// Part index in the chain.
    pub part: u32,
    /// What the section is.
    pub kind: PartKind,
    /// Native record index, if any.
    pub record: Option<u32>,
    /// Section area used as the denominator (the patch: the patch printed at the worst layer), mm².
    pub area_mm2: f64,
    /// Largest tributary ratio and its layer.
    pub tributary: (f64, u32),
    /// Largest additive group ratio and its layer.
    pub additive: (f64, u32),
    /// Largest equal-share ratio and its layer.
    pub equal_share: (f64, u32),
    /// Largest rigid-lever ratio and its layer.
    pub rigid_lever: (f64, u32),
    /// Largest sizing ratio (rigid-carrier and additive) and its layer.
    pub sizing: (f64, u32),
    /// All four models at THAT layer, not four unrelated maxima.
    pub at_worst: Sharing,
    /// Reaction (or explicitly unresolved provisional share) plus own peel, mm².
    pub load_mm2: f64,
}

/// One contact at one exposure; used for located activation/patch-growth histories.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactState {
    /// Contact index.
    pub contact: u32,
    /// Limiting loaded part index.
    pub part: u32,
    /// Printed patch area, mm².
    pub patch_mm2: f64,
    /// Limiting section area, mm².
    pub section_mm2: f64,
    /// Assigned reaction plus upstream self-peel, mm² (provisional if unresolved).
    pub load_mm2: f64,
    /// Four simultaneous ratios; rigid-lever is not valid when the group is unresolved.
    pub ratios: Sharing,
}

/// One component in one layer.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupLoad {
    /// Layer.
    pub layer: u32,
    /// Component.
    pub component: u32,
    /// Peel area of the component in this layer, mm².
    pub area_mm2: f64,
    /// Active contacts.
    pub active: Vec<u32>,
    /// The tributary peel area given to each active contact (same order), mm². The model's
    /// own area only: the support's own material is not in it. The values add up to
    /// `area_mm2`.
    pub loads_mm2: Vec<f64>,
    /// [load including upstream support peel, section area] at each active
    /// contact's worst section in THIS layer, same order as `active`.
    pub contact_sections_mm2: Vec<[f64; 2]>,
    /// Sum of their limiting section areas, mm².
    pub section_sum_mm2: f64,
    /// Additive area ratio `A / sum B` (infinite when `sum B` is 0).
    pub x_group: f64,
    /// Additive design-capacity utilisation `x_group / 12`.
    pub utilisation: f64,
    /// Area-weighted centre of the layer's peel (x, y), mm.
    pub centroid_xy: [f64; 2],
    /// Distance from the centre to the spread of the active contacts, mm (0 inside).
    pub outside_hull_mm: f64,
    /// Relative residual of the rigid-lever equilibrium.
    pub lever_residual: f64,
    /// Separate equilibrium and conditioning results (thresholds in lever.rs [S]).
    pub force_residual: f64,
    /// Normalised moment residual.
    pub moment_residual: f64,
    /// Principal tilt stiffness condition number.
    pub condition: f64,
    /// Reaction sensitivity to the pre-run 0.01 mm resultant-position probe.
    pub position_sensitivity: f64,
    /// An unresolved solve must never be called a low-ratio pass.
    pub equilibrium_assessed: bool,
    /// Simultaneous section and patch history at this exposure.
    pub states: Vec<ContactState>,
    /// Every downstream section at this exposure, not only each contact's worst.
    /// Complete candidate replay uses this to prevent hiding another violation.
    pub section_states: Vec<ContactState>,
}

/// The layer with the most peel area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeakLayer {
    /// Layer.
    pub layer: u32,
    /// Peel area, mm².
    pub area_mm2: f64,
    /// Design ratio times the active section capacity, mm².
    pub capacity_mm2: f64,
    /// Active contacts.
    pub active_contacts: u32,
}

/// The report and the numbers behind it.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadDetail {
    /// The plain-data answer.
    pub report: LoadReport,
    /// One entry per contact.
    pub contacts: Vec<ContactLoad>,
    /// One entry per section that was loaded.
    pub sections: Vec<SectionDetail>,
    /// One entry per (layer, component) with positive peel area.
    pub groups: Vec<GroupLoad>,
    /// The layer with the most peel area.
    pub peak: Option<PeakLayer>,
    /// Cover of the overhang samples, when they were given.
    pub coverage: Option<Coverage>,
}

/// What stops a (layer, component) from being carried.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Unsupported {
    pub layer: u32,
    pub component: u32,
    pub area: f64,
}

/// A route that is not printed by the exposure layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BrokenRoute {
    pub layer: u32,
    pub component: u32,
    pub contact: u32,
}

/// What the layer pass collected, for the warnings.
#[derive(Debug, Default)]
pub(super) struct Collected {
    pub unsupported: Vec<Unsupported>,
    pub broken: Vec<BrokenRoute>,
}

/// One part of a chain as the pass uses it.
#[derive(Debug, Clone, Copy)]
struct Part {
    kind: PartKind,
    area: f64,
    z: [f64; 2],
}

/// A contact prepared for the layer pass.
struct Ctx {
    centre: [f64; 3],
    component: Option<u32>,
    first_layer: u32,
    patch_lower: f64,
    patch_z: [f64; 2],
    patch_history: Vec<(f64, f64)>,
    parts: Vec<Part>,
    braces: Vec<Part>,
}

/// The fraction of a disc below a chord at fractional height `t` of its diameter.
fn disc_fraction(t: f64) -> f64 {
    let u = 1.0 - 2.0 * t.clamp(0.0, 1.0);
    (u.acos() - u * (1.0 - u * u).max(0.0).sqrt()) / std::f64::consts::PI
}

impl Ctx {
    fn patch_printed(&self, z_top: f64) -> f64 {
        if !self.patch_history.is_empty() {
            return self
                .patch_history
                .iter()
                .rev()
                .find(|(z, _)| *z <= z_top + 1e-9)
                .map_or(0.0, |(_, a)| *a);
        }
        let [lo, hi] = self.patch_z;
        let t = if hi - lo <= 1e-9 {
            f64::from(u8::from(z_top >= lo))
        } else {
            (z_top - lo) / (hi - lo)
        };
        self.patch_lower * disc_fraction(t)
    }

    /// The limiting section area at the layer ending at `z_top`: the patch printed so far and
    /// every other section of the path that exists by then.
    fn capacity(&self, z_top: f64) -> f64 {
        self.parts
            .iter()
            .map(|p| match p.kind {
                PartKind::Patch => self.patch_printed(z_top),
                _ if p.z[0] <= z_top + 1e-9 => p.area,
                _ => f64::INFINITY,
            })
            .fold(self.patch_printed(z_top), f64::min)
    }

    fn route_broken(&self, z_top: f64) -> bool {
        self.parts
            .iter()
            .any(|p| p.kind != PartKind::Patch && p.z[0] > z_top + 1e-9)
    }
}

/// True when part `p` has material in the layer `[z_bot, z_top)`: it overlaps the layer in a
/// positive height (a part that only touches the layer's bottom plane belongs to the layer
/// below), or it has no height and lies in the layer.
fn printed_in(p: &Part, z_bot: f64, z_top: f64) -> bool {
    if p.kind == PartKind::Patch {
        return false;
    }
    if p.z[1] - p.z[0] < 1e-9 {
        p.z[0] >= z_bot && p.z[0] < z_top
    } else {
        p.z[0] < z_top && p.z[1] > z_bot
    }
}

fn ratio(q: f64, b: f64) -> f64 {
    if b > 0.0 {
        q / b
    } else if q > 0.0 {
        f64::INFINITY
    } else {
        0.0
    }
}

fn root(stack: &LayerStack, mut c: u32, k: u32) -> u32 {
    while let Some((m, at)) = stack.components.get(c as usize).and_then(|i| i.merged_into) {
        if at > k {
            break;
        }
        c = m;
    }
    c
}

/// The component whose material is nearest to a contact at its first layers.
fn contact_component(stack: &LayerStack, c: &ContactView) -> Option<u32> {
    for dk in 0..=2 {
        let Some(layer) = stack.layers.get((c.first_layer + dk) as usize) else {
            continue;
        };
        let mut best: Option<(f64, u32)> = None;
        for cs in &layer.components {
            for e in &cs.sources {
                let d2: f64 = (0..3).map(|a| (e.xyz[a] - c.centre[a]).powi(2)).sum();
                if best.is_none_or(|b| d2 < b.0) {
                    best = Some((d2, cs.component));
                }
            }
        }
        if let Some((d2, comp)) = best.filter(|b| b.0 <= 2.5 * 2.5) {
            let _ = d2;
            return Some(comp);
        }
    }
    None
}

fn parts_of(chain: Option<&PartChain>, c: &ContactView) -> Vec<Part> {
    let from = |p: &PartSection| Part {
        kind: p.kind,
        area: p.area_lower_mm2,
        z: p.z_range_mm,
    };
    match chain {
        Some(ch) if !ch.parts.is_empty() => ch.parts.iter().map(from).collect(),
        _ => vec![
            Part {
                kind: PartKind::Patch,
                area: c.patch_lower_mm2,
                z: [c.centre[2], c.centre[2]],
            },
            Part {
                kind: PartKind::Neck,
                area: c.neck_area_mm2,
                z: [c.centre[2], c.centre[2]],
            },
        ],
    }
}

fn prepare(stack: &LayerStack, layout: &LayoutView) -> Vec<Ctx> {
    let h = stack.layer_height_mm;
    let mut ctx: Vec<Ctx> = layout
        .contacts
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let chain = layout.chains.get(i).filter(|ch| ch.contact as usize == i);
            let parts = parts_of(chain, c);
            let first_z = stack.layer_origin_mm + f64::from(c.first_layer) * h;
            let patch_z = parts
                .iter()
                .find(|p| p.kind == PartKind::Patch)
                .map_or([first_z, first_z], |p| p.z);
            // The patch lower bound of the view is the whole patch; the chain's patch section
            // may carry the same number.
            Ctx {
                centre: c.centre,
                component: contact_component(stack, c),
                first_layer: c.first_layer,
                patch_lower: c.patch_lower_mm2,
                patch_z,
                patch_history: layout
                    .patch_history
                    .get(i)
                    .into_iter()
                    .flatten()
                    .map(|&(k, a)| (stack.layer_origin_mm + f64::from(k + 1) * h, a))
                    .collect(),
                parts,
                braces: Vec::new(),
            }
        })
        .collect();
    for ch in layout.chains.iter().skip(layout.contacts.len()) {
        if let Some(carrier) = ctx.get_mut(ch.contact as usize) {
            carrier.braces.extend(ch.parts.iter().map(|p| Part {
                kind: PartKind::Brace,
                area: p.area_lower_mm2,
                z: p.z_range_mm,
            }));
        }
    }
    ctx
}

/// Per-section running maxima.
#[derive(Debug, Clone, Copy)]
struct Acc {
    tributary: (f64, u32),
    additive: (f64, u32),
    equal: (f64, u32),
    lever: (f64, u32),
    area: f64,
    at_worst: Sharing,
    sizing: (f64, u32),
    load: f64,
    seen: bool,
}

impl Acc {
    fn new() -> Self {
        Self {
            tributary: (0.0, 0),
            additive: (0.0, 0),
            equal: (0.0, 0),
            lever: (0.0, 0),
            area: 0.0,
            at_worst: Sharing::default(),
            sizing: (0.0, 0),
            load: 0.0,
            seen: false,
        }
    }

    fn push(&mut self, k: u32, s: Sharing, area: f64, load: f64) {
        if !self.seen || s.tributary > self.tributary.0 {
            self.tributary = (s.tributary, k);
        }
        let sizing = s.rigid_lever.max(s.additive);
        if !self.seen || sizing > self.sizing.0 {
            self.sizing = (sizing, k);
            self.at_worst = s;
            self.area = area;
            self.load = load;
        }
        if !self.seen || s.additive > self.additive.0 {
            self.additive = (s.additive, k);
        }
        if !self.seen || s.equal_share > self.equal.0 {
            self.equal = (s.equal_share, k);
        }
        if !self.seen || s.rigid_lever > self.lever.0 {
            self.lever = (s.rigid_lever, k);
        }
        self.seen = true;
    }
}

/// Nearest-contact split of one component's source elements. Returns, per active contact,
/// the area and the first moments (x, y) of what it got.
fn tributary_split(sources: &[super::data::SourceElement], centres: &[[f64; 3]]) -> Vec<[f64; 3]> {
    let mut out = vec![[0.0; 3]; centres.len()];
    let mut d2 = vec![0.0; centres.len()];
    for e in sources {
        let mut dmin = f64::INFINITY;
        for (slot, c) in d2.iter_mut().zip(centres) {
            *slot = (0..3).map(|a| (e.xyz[a] - c[a]).powi(2)).sum();
            dmin = dmin.min(*slot);
        }
        let tol = dmin * (1.0 + 1e-12) + 1e-18;
        let ties = d2.iter().filter(|&&d| d <= tol).count();
        for (o, &d) in out.iter_mut().zip(&d2) {
            if d <= tol {
                let a = e.area / ties as f64;
                o[0] += a;
                o[1] += a * e.xyz[0];
                o[2] += a * e.xyz[1];
            }
        }
    }
    out
}

/// Everything the pass keeps per contact.
struct Pass<'a> {
    stack: &'a LayerStack,
    layout: &'a LayoutView,
    ctx: Vec<Ctx>,
    acc: Vec<Vec<Acc>>,
    contacts: Vec<ContactLoad>,
    seen: Vec<bool>,
    groups: Vec<GroupLoad>,
    collected: Collected,
    layers: Vec<LayerLoad>,
}

pub(super) fn run(stack: &LayerStack, layout: &LayoutView) -> (LoadDetail, Collected) {
    let ctx = prepare(stack, layout);
    let acc = ctx
        .iter()
        .map(|c| vec![Acc::new(); c.parts.len()])
        .collect();
    let contacts = ctx
        .iter()
        .enumerate()
        .map(|(i, c)| ContactLoad {
            #[allow(clippy::cast_possible_truncation)]
            contact: i as u32,
            component: c.component,
            first_layer: c.first_layer,
            worst_layer: c.first_layer,
            load_mm2: 0.0,
            ratio: 0.0,
            tributary_xy: [c.centre[0], c.centre[1]],
            offset_mm: 0.0,
            layers_over: 0,
            first_over: None,
            last_over: None,
        })
        .collect();
    let seen = vec![false; layout.contacts.len()];
    let mut pass = Pass {
        stack,
        layout,
        ctx,
        acc,
        contacts,
        seen,
        groups: Vec::new(),
        collected: Collected::default(),
        layers: Vec::with_capacity(stack.layers.len()),
    };
    for k in 0..stack.layers.len() {
        #[allow(clippy::cast_possible_truncation)]
        pass.layer(k as u32);
    }
    pass.finish()
}

impl Pass<'_> {
    fn layer(&mut self, k: u32) {
        let stack = self.stack;
        let h = stack.layer_height_mm;
        let zb = stack.layer_origin_mm + f64::from(k) * h;
        let zt = zb + h;
        let layer = &stack.layers[k as usize];
        let mut active: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for (i, c) in self.ctx.iter().enumerate() {
            let (true, Some(c0)) = (k >= c.first_layer, c.component) else {
                continue;
            };
            let comp = root(stack, c0, k);
            if c.route_broken(zt) {
                #[allow(clippy::cast_possible_truncation)]
                self.collected.broken.push(BrokenRoute {
                    layer: k,
                    component: comp,
                    contact: i as u32,
                });
            } else {
                active.entry(comp).or_default().push(i);
            }
        }
        let mut capacity = 0.0;
        for cs in &layer.components {
            if cs.area <= 0.0 {
                continue;
            }
            let list = active.get(&cs.component).map_or(&[][..], Vec::as_slice);
            capacity += self.group(Lyr { k, zb, zt }, cs, list);
        }
        self.layers.push(LayerLoad {
            area_mm2: layer.model_envelope,
            capacity_mm2: capacity * super::DESIGN_RATIO,
        });
    }

    /// One component in one layer. Returns the sum of the active limiting sections.
    fn group(&mut self, lyr: Lyr, cs: &super::data::ComponentSlice, list: &[usize]) -> f64 {
        let Lyr { k, zt, .. } = lyr;
        let n = list.len();
        if n == 0 {
            self.collected.unsupported.push(Unsupported {
                layer: k,
                component: cs.component,
                area: cs.area,
            });
            self.groups.push(GroupLoad {
                layer: k,
                component: cs.component,
                area_mm2: cs.area,
                active: Vec::new(),
                loads_mm2: Vec::new(),
                contact_sections_mm2: Vec::new(),
                section_sum_mm2: 0.0,
                x_group: f64::INFINITY,
                utilisation: f64::INFINITY,
                centroid_xy: centroid(&cs.sources),
                outside_hull_mm: f64::INFINITY,
                lever_residual: f64::INFINITY,
                force_residual: 1.0,
                moment_residual: f64::INFINITY,
                condition: f64::INFINITY,
                position_sensitivity: f64::INFINITY,
                equilibrium_assessed: false,
                states: Vec::new(),
                section_states: Vec::new(),
            });
            return 0.0;
        }
        let centres: Vec<[f64; 3]> = list.iter().map(|&i| self.ctx[i].centre).collect();
        let split = tributary_split(&cs.sources, &centres);
        let b: Vec<f64> = list.iter().map(|&i| self.ctx[i].capacity(zt)).collect();
        let bsum: f64 = b.iter().sum();
        let x_group = ratio(cs.area, bsum);
        let c_xy = centroid(&cs.sources);
        let xy: Vec<[f64; 2]> = centres.iter().map(|c| [c[0], c[1]]).collect();
        let springs: Vec<Spring> = xy
            .iter()
            .zip(&b)
            .map(|(&xy, &k)| Spring { xy, k })
            .collect();
        let lever = rigid_lever(&springs, cs.area, c_xy);
        let shares = Shares {
            x_group,
            equal: cs.area / n as f64,
        };
        let mut section_states = Vec::with_capacity(list.len() * 7);
        let states: Vec<_> = list
            .iter()
            .enumerate()
            .map(|(a, &i)| {
                let (worst, all) =
                    self.contact_sections(lyr, i, split[a], (lever.force[a], lever.valid), shares);
                section_states.extend(all);
                worst
            })
            .collect();
        let contact_sections_mm2 = states.iter().map(|s| [s.load_mm2, s.section_mm2]).collect();
        #[allow(clippy::cast_possible_truncation)]
        self.groups.push(GroupLoad {
            layer: k,
            component: cs.component,
            area_mm2: cs.area,
            active: list.iter().map(|&i| i as u32).collect(),
            loads_mm2: split.iter().map(|s| s[0]).collect(),
            contact_sections_mm2,
            section_sum_mm2: bsum,
            x_group,
            utilisation: x_group / super::DESIGN_RATIO,
            centroid_xy: c_xy,
            outside_hull_mm: distance_to_hull(&xy, c_xy),
            lever_residual: lever.residual,
            force_residual: lever.force_residual,
            moment_residual: lever.moment_residual,
            condition: lever.condition,
            position_sensitivity: lever.position_sensitivity,
            equilibrium_assessed: lever.valid,
            states,
            section_states,
        });
        bsum
    }

    fn contact_sections(
        &mut self,
        lyr: Lyr,
        i: usize,
        mine: [f64; 3],
        lever: (f64, bool),
        sh: Shares,
    ) -> (ContactState, Vec<ContactState>) {
        let (lever_force, valid) = lever;
        let Lyr { k, zb, zt } = lyr;
        let c = &self.ctx[i];
        let own: Vec<f64> = c
            .parts
            .iter()
            .map(|p| if printed_in(p, zb, zt) { p.area } else { 0.0 })
            .collect();
        let braces: f64 = c
            .braces
            .iter()
            .filter(|p| printed_in(p, zb, zt))
            .map(|p| p.area)
            .sum();
        let patch_now = c.patch_printed(zt);
        let mut upstream = 0.0;
        let mut worst = ContactState {
            contact: i as u32,
            part: 0,
            patch_mm2: patch_now,
            section_mm2: patch_now,
            load_mm2: 0.0,
            ratios: Sharing::default(),
        };
        let mut worst_ratio = -1.0_f64;
        let mut all = Vec::with_capacity(c.parts.len());
        for (j, p) in c.parts.iter().enumerate() {
            upstream += own[j];
            let extra = upstream
                + if p.kind == PartKind::Column {
                    braces
                } else {
                    0.0
                };
            let denom = if p.kind == PartKind::Patch {
                patch_now
            } else {
                p.area
            };
            let s = Sharing {
                tributary: ratio(mine[0] + extra, denom),
                additive: sh.x_group,
                equal_share: ratio(sh.equal + extra, denom),
                // Zero here is a storage sentinel, NEVER a passing result:
                // the group's assessed flag accompanies every reported value.
                rigid_lever: if valid {
                    ratio(lever_force + extra, denom)
                } else {
                    0.0
                },
            };
            let load = lever_force + extra;
            self.acc[i][j].push(k, s, denom, load);
            let state = ContactState {
                contact: i as u32,
                part: j as u32,
                patch_mm2: patch_now,
                section_mm2: denom,
                load_mm2: load,
                ratios: s,
            };
            all.push(state);
            let r = s.rigid_lever.max(s.additive);
            if r > worst_ratio {
                worst_ratio = r;
                worst = state;
            }
        }
        let r = worst_ratio;
        let load = worst.load_mm2;
        let cl = &mut self.contacts[i];
        if r > super::DESIGN_RATIO + 1e-9 {
            cl.layers_over += 1;
            cl.first_over.get_or_insert(k);
            cl.last_over = Some(k);
        }
        if !self.seen[i] || r > cl.ratio {
            self.seen[i] = true;
            let xy = if mine[0] > 0.0 {
                [mine[1] / mine[0], mine[2] / mine[0]]
            } else {
                [c.centre[0], c.centre[1]]
            };
            cl.ratio = r;
            cl.worst_layer = k;
            cl.load_mm2 = load;
            cl.tributary_xy = xy;
            cl.offset_mm = (xy[0] - c.centre[0]).hypot(xy[1] - c.centre[1]);
        }
        (worst, all)
    }

    fn finish(self) -> (LoadDetail, Collected) {
        let mut sections = Vec::new();
        let mut report_sections = Vec::new();
        for (i, per_part) in self.acc.iter().enumerate() {
            let Some(chain) = self
                .layout
                .chains
                .get(i)
                .filter(|ch| ch.contact as usize == i)
            else {
                continue;
            };
            for (j, a) in per_part.iter().enumerate() {
                if !a.seen {
                    continue;
                }
                let part = chain.parts.get(j);
                #[allow(clippy::cast_possible_truncation)]
                let (ci, pj) = (i as u32, j as u32);
                sections.push(SectionDetail {
                    chain: ci,
                    part: pj,
                    kind: self.ctx[i].parts[j].kind,
                    record: part.and_then(|p| p.record),
                    area_mm2: a.area,
                    tributary: a.tributary,
                    additive: a.additive,
                    equal_share: a.equal,
                    rigid_lever: a.lever,
                    sizing: a.sizing,
                    at_worst: a.at_worst,
                    load_mm2: a.load,
                });
                report_sections.push(SectionLoad {
                    chain: ci,
                    part: pj,
                    worst_layer: a.sizing.1,
                    ratios: a.at_worst,
                });
            }
        }
        let peak = self
            .layers
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.area_mm2.total_cmp(&b.1.area_mm2))
            .map(|(k, l)| PeakLayer {
                #[allow(clippy::cast_possible_truncation)]
                layer: k as u32,
                area_mm2: l.area_mm2,
                capacity_mm2: l.capacity_mm2,
                active_contacts: self
                    .ctx
                    .iter()
                    .filter(|c| c.component.is_some() && k >= c.first_layer as usize)
                    .count() as u32,
            });
        let report = LoadReport {
            status: LoadStatus::Computed,
            design_ratio: super::DESIGN_RATIO,
            layers: self.layers,
            sections: report_sections,
            warnings: Vec::new(),
        };
        (
            LoadDetail {
                report,
                contacts: self.contacts,
                sections,
                groups: self.groups,
                peak,
                coverage: None,
            },
            self.collected,
        )
    }
}

/// A source element and the contact the tributary rule gives it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AssignedSource {
    /// Centre of the element (x, y, z of the layer), mm.
    pub xyz: [f64; 3],
    /// Area, mm².
    pub area: f64,
    /// The component.
    pub component: u32,
    /// The contact that holds it (nearest active contact, lowest index on an exact tie), or
    /// `None` when the component has no active contact in this layer.
    pub contact: Option<u32>,
}

/// Which contact each source element of `layer` goes to: the same rule [`run`] uses, for the
/// pictures (the peeling layer, its source regions and their contacts).
#[must_use]
pub fn assign_sources(stack: &LayerStack, layout: &LayoutView, layer: u32) -> Vec<AssignedSource> {
    let ctx = prepare(stack, layout);
    let h = stack.layer_height_mm;
    let zt = stack.layer_origin_mm + f64::from(layer + 1) * h;
    let Some(la) = stack.layers.get(layer as usize) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for cs in &la.components {
        let act: Vec<usize> = ctx
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                layer >= c.first_layer
                    && !c.route_broken(zt)
                    && c.component
                        .is_some_and(|c0| root(stack, c0, layer) == cs.component)
            })
            .map(|(i, _)| i)
            .collect();
        for e in &cs.sources {
            let best = act
                .iter()
                .map(|&i| {
                    (
                        i,
                        (0..3)
                            .map(|a| (e.xyz[a] - ctx[i].centre[a]).powi(2))
                            .sum::<f64>(),
                    )
                })
                .min_by(|a, b| a.1.total_cmp(&b.1));
            #[allow(clippy::cast_possible_truncation)]
            out.push(AssignedSource {
                xyz: e.xyz,
                area: e.area,
                component: cs.component,
                contact: best.map(|b| b.0 as u32),
            });
        }
    }
    out
}

/// One layer: its index and the heights of its bottom and top.
#[derive(Debug, Clone, Copy)]
struct Lyr {
    k: u32,
    zb: f64,
    zt: f64,
}

#[derive(Debug, Clone, Copy)]
struct Shares {
    x_group: f64,
    equal: f64,
}

fn centroid(src: &[super::data::SourceElement]) -> [f64; 2] {
    let w: f64 = src.iter().map(|e| e.area).sum();
    if w <= 0.0 {
        return [0.0, 0.0];
    }
    [
        src.iter().map(|e| e.xyz[0] * e.area).sum::<f64>() / w,
        src.iter().map(|e| e.xyz[1] * e.area).sum::<f64>() / w,
    ]
}
