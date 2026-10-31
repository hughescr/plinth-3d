use super::{PlanSettings, Proposal, Termination, layout_from_contacts, unavailable};
use crate::construct::{CandId, ContactCandidate, Contacts, PlanBuilder, Quote};
use crate::load::needs::{NeedPoint, NeedsMap, SiteTag};
use crate::load::{self, LayoutView, ModelLayers, SiteClass};
use std::collections::{BTreeMap, BTreeSet};

type V3 = [f64; 3];
fn d2(a: V3, b: V3) -> f64 {
    (0..3).map(|k| (a[k] - b[k]).powi(2)).sum()
}
fn class(p: &NeedPoint) -> SiteClass {
    if p.class == SiteTag::Hidden {
        SiteClass::HiddenUnderside
    } else {
        SiteClass::PartlyVisibleRim
    }
}
fn axis(n: V3, degrees: f64) -> [f32; 3] {
    let angle = (-n[2]).clamp(-1., 1.).acos();
    if angle < 1e-8 {
        return [0., 0., -1.];
    }
    let t = degrees.to_radians().min(angle) / angle;
    let a = ((1. - t) * angle).sin() / angle.sin();
    let b = (t * angle).sin() / angle.sin();
    [(a * n[0]) as f32, (a * n[1]) as f32, (a * n[2] - b) as f32]
}
fn candidate(id: u32, p: &NeedPoint, neck: f32, fan: f64) -> ContactCandidate {
    let ball_r = 1.06 * neck;
    let standoff = ((f64::from(ball_r).powi(2)
        - 3.0615 * f64::from(neck).powi(2) / std::f64::consts::PI)
        .max(0.))
    .sqrt() as f32;
    let joint_r = 1.04 * (1.5 * neck).max(0.25);
    ContactCandidate {
        id: CandId(id),
        site: p.xyz.map(|v| v as f32),
        ball_r,
        standoff,
        axis: axis(p.normal, fan),
        neck,
        joint_r,
        head_len: (2.6 - joint_r - standoff).max(1.5),
    }
}
// Sufficient enclosing-ball separation for the existing admission reservation.
fn separated(a: &Quote, b: &Quote) -> bool {
    let a = a.contact();
    let b = b.contact();
    let ar = d2(a.centre().map(f64::from), a.joint().map(f64::from)).sqrt()
        + f64::from(a.joint_radius());
    let br = d2(b.centre().map(f64::from), b.joint().map(f64::from)).sqrt()
        + f64::from(b.joint_radius());
    d2(a.centre().map(f64::from), b.centre().map(f64::from)).sqrt() >= ar + br + 0.0001
}
struct Chosen {
    candidate: ContactCandidate,
    quote: Quote,
    site: NeedPoint,
}
fn view(chosen: &[Chosen], layers: &ModelLayers, shift: f64) -> LayoutView {
    let contacts: Vec<_> = chosen.iter().map(|x| x.quote.contact()).collect();
    let classes: Vec<_> = chosen.iter().map(|x| class(&x.site)).collect();
    let mut v = layout_from_contacts(&contacts, &classes, &layers.stack, shift);
    for (c, x) in v.contacts.iter_mut().zip(chosen) {
        // Strictly interior face site inside the native in-sphere: overlap exists
        // by its own layer. The earlier enclosing-ball pole is not an anchor.
        c.first_layer = ((x.site.xyz[2] + shift - layers.grid.origin) / layers.grid.height)
            .floor()
            .max(0.) as u32;
    }
    v
}
#[derive(Clone, Copy)]
struct Target {
    xyz: V3,
    deadline: f64,
    load: f64,
    anchor: bool,
}
fn covered(t: Target, chosen: &[Chosen], reach: f64, shift: f64) -> bool {
    chosen.iter().any(|x| {
        x.site.xyz[2] + shift <= t.deadline + 1e-8 && d2(x.site.xyz, t.xyz) <= reach * reach
    })
}
fn needs_targets(
    layers: &ModelLayers,
    sites: &[NeedPoint],
    chosen: &[Chosen],
    settings: &PlanSettings,
    shift: f64,
) -> Vec<Target> {
    let mut targets = Vec::new();
    for isl in layers.structure.needs_anchor() {
        let top = layers.grid.z_top(isl.first_layer);
        if !chosen.iter().any(|x| {
            let z = x.site.xyz[2] + shift;
            z >= layers.grid.z_bottom(isl.first_layer) - 1e-8
                && z < top
                && (x.site.xyz[0] - isl.first_xy[0]).hypot(x.site.xyz[1] - isl.first_xy[1]) < 0.5
        }) {
            targets.push(Target {
                xyz: [isl.first_xy[0], isl.first_xy[1], top - shift - 0.0001],
                deadline: top - 0.000001,
                load: 0.,
                anchor: true,
            });
        }
    }
    // Automatic rim contacts are mandatory too, but hidden sites have priority.
    for tag in [SiteTag::Hidden, SiteTag::PartlyVisible] {
        let mut region: Vec<Target> = Vec::new();
        for p in sites
            .iter()
            .filter(|p| p.class == tag && p.angle_deg < 45. && p.area_mm2 > 0.)
        {
            let t = Target {
                xyz: p.xyz,
                deadline: p.xyz[2] + shift + 0.05,
                load: 0.,
                anchor: false,
            };
            if !covered(t, chosen, settings.coverage_reach_mm, shift)
                && !region
                    .iter()
                    .any(|q| d2(q.xyz, t.xyz) < settings.coverage_reach_mm.powi(2))
            {
                region.push(t);
            }
        }
        targets.extend(region);
    }
    for r in &layers.structure.reach {
        let t = Target {
            xyz: [
                r.farthest_xy[0],
                r.farthest_xy[1],
                layers.grid.z_top(r.layer) - shift,
            ],
            deadline: layers.grid.z_top(r.layer),
            load: 0.,
            anchor: false,
        };
        if !covered(t, chosen, 2.13, shift) {
            targets.push(t);
        }
    }
    targets
}
fn load_targets(
    detail: &load::LoadDetail,
    chosen: &[Chosen],
    radius: f64,
    shift: f64,
) -> Vec<Target> {
    detail
        .contacts
        .iter()
        .zip(chosen)
        .filter(|(c, x)| c.load_mm2 > x.quote.full_capacity())
        .map(|(c, x)| {
            let dx = c.tributary_xy[0] - x.site.xyz[0];
            let dy = c.tributary_xy[1] - x.site.xyz[1];
            let len = dx.hypot(dy).max(1e-8);
            Target {
                xyz: [
                    x.site.xyz[0] + radius * dx / len,
                    x.site.xyz[1] + radius * dy / len,
                    x.site.xyz[2],
                ],
                deadline: x.site.xyz[2] + shift + 0.05,
                load: 0.,
                anchor: false,
            }
        })
        .collect()
}
#[derive(Default)]
struct Cache {
    next_id: u32,
    seen: BTreeSet<Vec<u32>>,
    available: Vec<Chosen>,
    rejected: BTreeMap<String, u64>,
}
impl Cache {
    fn measure(
        &mut self,
        builder: &PlanBuilder<Contacts>,
        tasks: &[(Target, Option<NeedPoint>)],
        sites: &[NeedPoint],
        shift: f64,
    ) -> Vec<Vec<NeedPoint>> {
        let mut alternatives = Vec::new();
        let mut sampled = Vec::new();
        let mut task_sites = Vec::new();
        for (target, old) in tasks {
            let mut near: Vec<_> = old.map_or_else(
                || {
                    sites
                        .iter()
                        .copied()
                        .filter(|p| {
                            p.xyz[2] + shift < target.deadline
                                && (!target.anchor || p.xyz[2] + shift >= target.deadline - 0.05)
                        })
                        .collect()
                },
                |p| vec![p],
            );
            near.sort_by(|a, b| {
                let score = |p: &NeedPoint| {
                    d2(p.xyz, target.xyz)
                        + if target.anchor && p.class != SiteTag::Hidden {
                            10000.
                        } else {
                            0.
                        }
                };
                score(a).total_cmp(&score(b))
            });
            near.dedup_by(|a, b| d2(a.xyz, b.xyz) < 1e-8);
            near.truncate(if old.is_some() { 1 } else { 2 });
            task_sites.push(near.clone());
            let proposed = ((target.load / 12. / 2.85).sqrt() as f32).clamp(0.053, 0.39);
            for p in near {
                for neck in [0.053, proposed, 0.39] {
                    for fan in [20., 0.] {
                        let cand = candidate(self.next_id, &p, neck, fan);
                        let key = cand
                            .site
                            .into_iter()
                            .chain(cand.axis)
                            .chain([
                                cand.ball_r,
                                cand.standoff,
                                cand.neck,
                                cand.joint_r,
                                cand.head_len,
                            ])
                            .map(f32::to_bits)
                            .collect();
                        if alternatives.len() >= 4096 || !self.seen.insert(key) {
                            continue;
                        }
                        alternatives.push(cand);
                        self.next_id += 1;
                        sampled.push(p);
                    }
                }
            }
        }
        let mut answers = Vec::new();
        builder.probe_alternatives(&alternatives, &mut answers);
        for ((candidate, site), answer) in alternatives.into_iter().zip(sampled).zip(answers) {
            match answer {
                Ok(quote) => self.available.push(Chosen {
                    candidate,
                    quote,
                    site,
                }),
                Err(e) => {
                    *self
                        .rejected
                        .entry(
                            format!("{e:?}")
                                .split(" {")
                                .next()
                                .unwrap_or("unknown")
                                .to_owned(),
                        )
                        .or_default() += 1
                }
            }
        }
        task_sites
    }
    fn select(
        &mut self,
        tasks: Vec<(Target, Option<NeedPoint>)>,
        near: Vec<Vec<NeedPoint>>,
        chosen: &mut Vec<Chosen>,
    ) {
        self.available.append(chosen);
        for ((target, _), near) in tasks.into_iter().zip(near) {
            let mut opts: Vec<_> = self
                .available
                .iter()
                .enumerate()
                .filter(|(_, q)| {
                    near.iter().any(|p| d2(q.site.xyz, p.xyz) < 1e-12)
                        && chosen.iter().all(|c| separated(&q.quote, &c.quote))
                })
                .map(|(i, _)| i)
                .collect();
            opts.sort_by(|&a, &b| {
                let a = &self.available[a];
                let b = &self.available[b];
                let enough_a = a.quote.full_capacity() >= target.load;
                let enough_b = b.quote.full_capacity() >= target.load;
                enough_b.cmp(&enough_a).then_with(|| {
                    if enough_a {
                        a.candidate
                            .ball_r
                            .total_cmp(&b.candidate.ball_r)
                            .then(a.candidate.axis[2].total_cmp(&b.candidate.axis[2]))
                    } else {
                        b.quote.full_capacity().total_cmp(&a.quote.full_capacity())
                    }
                })
            });
            if let Some(&i) = opts.first() {
                chosen.push(self.available.swap_remove(i));
            }
        }
    }
}
pub(super) fn run(
    builder: &PlanBuilder<Contacts>,
    layers: &ModelLayers,
    sites: &[NeedPoint],
    settings: &PlanSettings,
    shift: f64,
) -> Proposal {
    let mut chosen = Vec::new();
    let mut cache = Cache::default();
    let valid = !unavailable(&layers.stack)
        && settings.coverage_reach_mm.is_finite()
        && settings.coverage_reach_mm > 0.
        && settings.ring_mm[0] > 0.
        && settings.ring_mm[1] >= settings.ring_mm[0];
    for _round in 0..settings.max_rounds.min(8) {
        if !valid {
            break;
        }
        let layout = view(&chosen, layers, shift);
        let detail = load::analyse_full(&layers.stack, &layout, &load::Extras::default());
        let mut targets = needs_targets(layers, sites, &chosen, settings, shift);
        targets.extend(load_targets(
            &detail,
            &chosen,
            settings.ring_mm[1].min(2.5),
            shift,
        ));
        let mut tasks: Vec<_> = chosen
            .iter()
            .zip(&detail.contacts)
            .map(|(x, c)| {
                (
                    Target {
                        xyz: x.site.xyz,
                        deadline: f64::INFINITY,
                        load: c.load_mm2,
                        anchor: false,
                    },
                    Some(x.site),
                )
            })
            .collect();
        tasks.extend(targets.into_iter().take(12).map(|t| (t, None)));
        let near = cache.measure(builder, &tasks, sites, shift);
        let previous: Vec<_> = chosen.iter().map(|c| c.candidate).collect();
        cache.select(tasks, near, &mut chosen);
        let changed = chosen.len() != previous.len()
            || chosen.iter().zip(&previous).any(|(a, b)| {
                a.candidate.ball_r.to_bits() != b.ball_r.to_bits()
                    || a.candidate.site.map(f32::to_bits) != b.site.map(f32::to_bits)
            });
        if !changed {
            break;
        }
    }
    let layout = view(&chosen, layers, shift);
    let load = load::analyse(&layers.stack, &layout);
    Proposal {
        termination: if valid {
            Termination::SearchBudgetExhausted
        } else {
            Termination::NotBuilt
        },
        candidates: chosen.into_iter().map(|x| x.candidate).collect(),
        load,
        layout,
        needs: NeedsMap::default(),
        rejections: cache.rejected.into_iter().collect(),
    }
}
