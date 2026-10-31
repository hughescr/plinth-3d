#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;

use super::{PlanSettings, Proposal, Termination, layout_from_contacts, unavailable};
use crate::construct::{CandId, ColumnCheck, ContactCandidate, Contacts, PlanBuilder, Quote};
use crate::load::needs::{NeedPoint, NeedsMap, SiteTag};
use crate::load::{self, LayoutView, ModelLayers, SiteClass};
use std::collections::{BTreeMap, BTreeSet};

type V3 = [f64; 3];
fn d2(a: V3, b: V3) -> f64 {
    (0..3).map(|k| (a[k] - b[k]).powi(2)).sum()
}
fn site_key(p: V3) -> [u32; 3] {
    // Search resolution only: tiny differences between neighbouring triangle-edge
    // samples must not spend all eight rounds on the same physical location.
    p.map(|v| ((v * 8.0).round() as f32).to_bits())
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
fn candidate(id: u32, p: &NeedPoint, neck: f32, fan: f64, load: f64) -> ContactCandidate {
    let ball_r = 1.06 * neck;
    let standoff = ((f64::from(ball_r).powi(2)
        - 3.0615 * f64::from(neck).powi(2) / std::f64::consts::PI)
        .max(0.))
    .sqrt() as f32;
    let column = ((load / 12.0 / 3.0615).sqrt() as f32).next_up().max(0.25);
    let joint_r = 1.04 * (1.5 * neck).max(column);
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
fn separated(a: &Quote, b: &Quote) -> bool {
    a.compatible(b)
}
struct Chosen {
    candidate: ContactCandidate,
    quote: Quote,
    column: ColumnCheck,
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
    ring: Option<V3>,
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
                ring: None,
            });
        }
    }
    // Seed the hidden carrier faces too, not only the sub-45 degree bands.
    // Without these seeds the first load replay assigns an entire upper strip
    // to a rim contact, and removing its impossible column repeats forever.
    let pitch = settings.coverage_reach_mm * 3.0_f64.sqrt();
    let mut seeds: Vec<Target> = Vec::new();
    let mut hidden: Vec<_> = sites
        .iter()
        .filter(|p| p.class == SiteTag::Hidden && p.area_mm2 > 0.)
        .collect();
    hidden.sort_by(|a, b| {
        a.xyz[2]
            .total_cmp(&b.xyz[2])
            .then(a.xyz[0].total_cmp(&b.xyz[0]))
    });
    for p in hidden {
        let t = Target {
            xyz: p.xyz,
            deadline: p.xyz[2] + shift + 0.05,
            load: 3.0,
            anchor: false,
            ring: None,
        };
        if !covered(t, chosen, pitch, shift)
            && !seeds.iter().any(|s| d2(s.xyz, t.xyz) < pitch * pitch)
        {
            seeds.push(t);
        }
    }
    targets.extend(seeds);
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
                ring: None,
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
            ring: None,
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
    layers: &ModelLayers,
    shift: f64,
) -> Vec<Target> {
    let mut out = Vec::new();
    for (c, x) in detail
        .contacts
        .iter()
        .zip(chosen)
        .filter(|(c, _)| c.ratio > 12.0)
    {
        // Keep the early-layer neighbour search AND the largest-load region.
        // An early partial-patch overload must not pin all future additions to
        // the very first layer while a later strip still has one carrier.
        let first = c.first_over.unwrap_or(c.worst_layer);
        for layer in [first, c.worst_layer] {
            let deadline = layers.grid.z_top(layer);
            let n = x.site.normal;
            let length = n[0].hypot(n[1]);
            let u = if length > 1e-8 {
                [-n[1] / length, n[0] / length, 0.]
            } else {
                [1., 0., 0.]
            };
            let v = [
                n[1] * u[2] - n[2] * u[1],
                n[2] * u[0] - n[0] * u[2],
                n[0] * u[1] - n[1] * u[0],
            ];
            let delta = [
                c.tributary_xy[0] - x.site.xyz[0],
                c.tributary_xy[1] - x.site.xyz[1],
                deadline - shift - x.site.xyz[2],
            ];
            let project = |a: V3| (0..3).map(|k| a[k] * delta[k]).sum::<f64>();
            let towards = project(v).atan2(project(u));
            // All sides of the contact, with the most-loaded direction first.
            // The ring describes search targets, not permission to move the site.
            for step in [0, 1, -1, 2, -2, 3, -3, 4] {
                let angle = towards + f64::from(step) * std::f64::consts::FRAC_PI_4;
                out.push(Target {
                    xyz: std::array::from_fn(|k| {
                        x.site.xyz[k] + radius * (u[k] * angle.cos() + v[k] * angle.sin())
                    }),
                    deadline,
                    load: (c.load_mm2 * 0.5).clamp(0.25, 12.0),
                    anchor: false,
                    ring: Some(x.site.xyz),
                });
            }
        }
    }
    out
}
#[derive(Default)]
struct Cache {
    next_id: u32,
    seen: BTreeSet<Vec<u32>>,
    available: Vec<Chosen>,
    rejected: BTreeMap<String, u64>,
    blocked: BTreeSet<[u32; 3]>,
    load_envelope: BTreeMap<[u32; 3], f64>,
    pairs: BTreeMap<(CandId, CandId), bool>,
}
impl Cache {
    fn near(
        &self,
        target: Target,
        old: Option<NeedPoint>,
        sites: &[NeedPoint],
        shift: f64,
        chosen: &[Chosen],
    ) -> Vec<NeedPoint> {
        let mut near = old.map_or_else(
            || {
                sites
                    .iter()
                    .copied()
                    .filter(|p| {
                        p.xyz[2] + shift < target.deadline
                            && target
                                .ring
                                .is_none_or(|centre| (1.0..=6.25).contains(&d2(centre, p.xyz)))
                            && (!target.anchor || p.xyz[2] + shift >= target.deadline - 0.05)
                            && chosen
                                .iter()
                                .all(|c| d2(c.site.xyz, p.xyz) > 1.5_f64.powi(2))
                            && !self.blocked.contains(&site_key(p.xyz))
                    })
                    .collect::<Vec<_>>()
            },
            |p| vec![p],
        );
        let score = |p: &NeedPoint| {
            d2(p.xyz, target.xyz)
                + if target.anchor && p.class != SiteTag::Hidden {
                    10000.
                } else {
                    0.
                }
        };
        let cmp = |a: &NeedPoint, b: &NeedPoint| {
            score(a)
                .total_cmp(&score(b))
                .then(a.triangle.cmp(&b.triangle))
        };
        // Linear top-k partition rather than a full sort per target.
        if near.len() > 32 {
            near.select_nth_unstable_by(32, cmp);
            near.truncate(32);
        }
        near.sort_by(cmp);
        near.dedup_by(|a, b| site_key(a.xyz) == site_key(b.xyz));
        near
    }
    fn measure(
        &mut self,
        builder: &PlanBuilder<Contacts>,
        tasks: &[(Target, Option<NeedPoint>)],
        sites: &[NeedPoint],
        shift: f64,
        chosen: &[Chosen],
    ) -> Vec<Vec<NeedPoint>> {
        let mut alternatives = Vec::new();
        let mut sampled = Vec::new();
        let mut sample_loads = Vec::new();
        let mut task_sites = Vec::new();
        for (target, old) in tasks {
            let near = self.near(*target, *old, sites, shift, chosen);
            task_sites.push(near.iter().take(8).copied().collect::<Vec<_>>());
            // Seeds have no tributary assignment yet. This is an estimate, not a
            // capacity certificate: their real loads are replayed before commit.
            let load = if old.is_some() {
                target.load
            } else {
                target.load.max(3.0)
            };
            let proposed =
                (((load / 12. / 2.85).sqrt() * 1000.0).ceil() / 1000.0).clamp(0.053, 0.39) as f32;
            for p in near {
                let before = alternatives.len();
                let neck = proposed;
                {
                    for fan in [20., 0.] {
                        let cand = candidate(self.next_id, &p, neck, fan, load);
                        let key = site_key(p.xyz)
                            .into_iter()
                            .chain(
                                cand.axis
                                    .into_iter()
                                    .chain([
                                        cand.ball_r,
                                        cand.standoff,
                                        cand.neck,
                                        cand.joint_r,
                                        cand.head_len,
                                    ])
                                    .map(f32::to_bits),
                            )
                            .collect();
                        if alternatives.len() >= 384 || !self.seen.insert(key) {
                            continue;
                        }
                        alternatives.push(cand);
                        self.next_id += 1;
                        sampled.push(p);
                        sample_loads.push(load);
                    }
                }
                if alternatives.len() > before {
                    task_sites.last_mut().expect("current target").push(p);
                    break;
                }
            }
        }
        let mut answers = Vec::new();
        builder.probe_alternatives(&alternatives, &mut answers);
        let valid: Vec<_> = answers.iter().filter_map(|q| q.as_ref().ok()).collect();
        let loads: Vec<_> = answers
            .iter()
            .zip(&sample_loads)
            .filter_map(|(q, &load)| q.is_ok().then_some(load))
            .collect();
        let columns = builder.probe_column_alternatives(&valid, &loads);
        let mut columns = columns.into_iter();
        for (((candidate, site), _load), answer) in alternatives
            .into_iter()
            .zip(sampled)
            .zip(sample_loads)
            .zip(answers)
        {
            let answer = answer.and_then(|quote| {
                columns
                    .next()
                    .expect("one column per valid quote")
                    .map(|column| (quote, column))
            });
            match answer {
                Ok((quote, column)) => self.available.push(Chosen {
                    candidate,
                    quote,
                    column,
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
    // Selection changes tributaries. Size private joints/columns from that new
    // assignment BEFORE whole-route admission, never commit an undersized joint.
    // All changed heads and their columns are measured in two batches.
    fn size_columns(
        &mut self,
        builder: &PlanBuilder<Contacts>,
        chosen: &mut [Chosen],
        loads: &[f64],
    ) {
        let mut candidates = Vec::new();
        let mut owners = Vec::new();
        let mut estimates = Vec::new();
        for (owner, (c, &load)) in chosen.iter().zip(loads).enumerate() {
            let envelope = self.load_envelope.entry(site_key(c.site.xyz)).or_default();
            *envelope = envelope.max(load);
            if c.quote.contact().required_column_radius(*envelope) <= c.candidate.joint_r / 1.04 {
                continue;
            }
            // Conservative max-over-rounds input; no removal can make an earlier
            // sizing certificate shrink. A failed resize keeps the contact and
            // its load target, rather than banning a perfectly usable site.
            let estimate = *envelope * 1.1 + 1.0;
            let radius = ((estimate / 12.0 / 3.0615).sqrt() as f32)
                .next_up()
                .max(0.25);
            for fan in [35., 30., 25., 20., 15., 10., 5., 0.] {
                for reach in [2.6, 2.8, 2.94] {
                    let mut cand = c.candidate;
                    cand.id = CandId(self.next_id);
                    self.next_id += 1;
                    cand.joint_r = 1.04 * radius.max(1.5 * cand.neck);
                    cand.head_len = (reach - cand.joint_r - cand.standoff).max(1.5);
                    cand.axis = axis(c.site.normal, fan);
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
                    if !self.seen.insert(key) {
                        continue;
                    }
                    candidates.push(cand);
                    owners.push(owner);
                    estimates.push(estimate);
                }
            }
        }
        let mut answers = Vec::new();
        builder.probe_alternatives(&candidates, &mut answers);
        let valid: Vec<_> = answers.iter().filter_map(|q| q.as_ref().ok()).collect();
        let loads: Vec<_> = answers
            .iter()
            .zip(&estimates)
            .filter_map(|(q, &l)| q.is_ok().then_some(l))
            .collect();
        let mut columns = builder
            .probe_column_alternatives(&valid, &loads)
            .into_iter();
        let mut resized = BTreeSet::new();
        for ((candidate, owner), answer) in candidates.into_iter().zip(owners).zip(answers) {
            let answer = answer.and_then(|quote| {
                columns
                    .next()
                    .expect("one column per quote")
                    .map(|column| (quote, column))
            });
            match answer {
                Ok((quote, column))
                    if !resized.contains(&owner)
                        && chosen.iter().enumerate().all(|(i, c)| {
                            i == owner
                                || (quote.compatible(&c.quote) && column.compatible(&c.column))
                        }) =>
                {
                    let site = chosen[owner].site;
                    chosen[owner] = Chosen {
                        candidate,
                        site,
                        quote,
                        column,
                    };
                    resized.insert(owner);
                }
                Err(e) => {
                    *self.rejected.entry(format!("sizing {e:?}")).or_default() += 1;
                }
                Ok((quote, column)) => {
                    // Keep measured alternatives for later rounds; a changed
                    // neighbour can remove a pair conflict without re-measuring
                    // this immutable head against the same model.
                    self.available.push(Chosen {
                        candidate,
                        quote,
                        column,
                        site: chosen[owner].site,
                    });
                }
            }
        }
    }
    fn select(
        &mut self,
        tasks: Vec<(Target, Option<NeedPoint>)>,
        near: Vec<Vec<NeedPoint>>,
        chosen: &mut Vec<Chosen>,
    ) {
        for ((target, old), near) in tasks.into_iter().zip(near) {
            let replacing =
                old.and_then(|p| chosen.iter().position(|c| d2(c.site.xyz, p.xyz) < 1e-12));
            let pairs = &mut self.pairs;
            let mut opts: Vec<_> = self
                .available
                .iter()
                .enumerate()
                .filter(|(_, q)| {
                    near.iter().any(|p| d2(q.site.xyz, p.xyz) < 1e-12)
                        && !self
                            .blocked
                            .contains(&site_key(q.candidate.site.map(f64::from)))
                        && q.quote.contact().required_column_radius(target.load)
                            <= q.column.radius()
                        && q.quote.contact().required_column_radius(target.load)
                            <= q.candidate.joint_r / 1.04
                        && chosen.iter().enumerate().all(|(i, c)| {
                            Some(i) == replacing
                                || *pairs
                                    .entry((
                                        q.candidate.id.min(c.candidate.id),
                                        q.candidate.id.max(c.candidate.id),
                                    ))
                                    .or_insert_with(|| {
                                        separated(&q.quote, &c.quote)
                                            && q.column.compatible(&c.column)
                                    })
                        })
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
                let replacement = self.available.swap_remove(i);
                if let Some(at) = replacing {
                    let old = std::mem::replace(&mut chosen[at], replacement);
                    self.available.push(old);
                } else {
                    chosen.push(replacement);
                }
            }
        }
    }
}
pub(super) fn run(
    builder: &PlanBuilder<Contacts>,
    layers: &ModelLayers,
    sites: &[NeedPoint],
    needs: &NeedsMap,
    settings: &PlanSettings,
    shift: f64,
) -> Proposal {
    let mut chosen = Vec::new();
    let mut pending = Vec::new();
    let mut cache = Cache::default();
    let valid = !unavailable(&layers.stack)
        && settings.coverage_reach_mm.is_finite()
        && settings.coverage_reach_mm > 0.
        && settings.ring_mm[0] > 0.
        && settings.ring_mm[1] >= settings.ring_mm[0];
    let mut termination = Termination::NotBuilt;
    for round in 0..settings.max_rounds.min(8) {
        if !valid {
            break;
        }
        let layout = view(&chosen, layers, shift);
        let detail = load::analyse_full(&layers.stack, &layout, &load::Extras::default());
        let mut targets = needs_targets(layers, sites, &chosen, settings, shift);
        targets.append(&mut pending);
        targets.extend(load_targets(
            &detail,
            &chosen,
            (1.9 + 0.1 * f64::from(round % 3)).clamp(settings.ring_mm[0], settings.ring_mm[1]),
            layers,
            shift,
        ));
        let sizing_loads = super::routing_loads(&detail);
        let mut tasks: Vec<_> = chosen
            .iter()
            .zip(&sizing_loads)
            .map(|(x, &load)| {
                let envelope = cache.load_envelope.entry(site_key(x.site.xyz)).or_default();
                *envelope = envelope.max(load);
                let load = *envelope;
                (
                    Target {
                        xyz: x.site.xyz,
                        deadline: f64::INFINITY,
                        load,
                        anchor: false,
                        ring: None,
                    },
                    Some(x.site),
                )
            })
            .collect();
        tasks.extend(targets.into_iter().map(|t| (t, None)));
        // Every target participates. When the per-round geometric budget fills,
        // rotate the order next time so late targets are not starved by retries.
        if !tasks.is_empty() {
            let start = (round as usize * 17) % tasks.len();
            tasks.rotate_left(start);
        }
        let near = cache.measure(builder, &tasks, sites, shift, &chosen);
        cache.select(tasks, near, &mut chosen);
        let selected = load::analyse_full(
            &layers.stack,
            &view(&chosen, layers, shift),
            &load::Extras::default(),
        );
        pending = load_targets(&selected, &chosen, 1.9, layers, shift);
        cache.size_columns(builder, &mut chosen, &super::routing_loads(&selected));
        let layout = view(&chosen, layers, shift);
        let detail = load::analyse_full(&layers.stack, &layout, &load::Extras::default());
        // Selected heads/columns already carry pair and model certificates.
        // Braces are optional admitted alternatives. Replay the full route once
        // at the end, not eight times with transient tributary assignments.
        if needs_targets(layers, sites, &chosen, settings, shift).is_empty()
            && !chosen.is_empty()
            && detail.contacts.iter().all(|c| c.ratio <= 12.0)
            && detail
                .report
                .warnings
                .iter()
                .all(|w| w.class == load::WarningClass::Information)
        {
            termination = Termination::Satisfied;
            break;
        }
        // No changed==false exit: all eight rounds can open fresh sites.
        termination = Termination::SearchBudgetExhausted;
    }
    finish(builder, layers, needs, shift, chosen, (cache, termination))
}

fn finish(
    builder: &PlanBuilder<Contacts>,
    layers: &ModelLayers,
    needs: &NeedsMap,
    shift: f64,
    mut chosen: Vec<Chosen>,
    state: (Cache, Termination),
) -> Proposal {
    let (cache, mut termination) = state;
    chosen.sort_by_key(|c| c.candidate.id);
    let layout = view(&chosen, layers, shift);
    let detail = load::analyse_full(&layers.stack, &layout, &load::Extras::default());
    let refs: Vec<_> = chosen.iter().map(|c| &c.quote).collect();
    let loads = super::routing_loads(&detail);
    let routing = builder.preview_plan(&refs, &loads);
    let routing_blocker = routing.as_ref().err().map(|e| format!("{:?}", e.reject));
    let mut full_layout = layout.clone();
    if let Ok(plan) = &routing {
        super::add_members(plan, &mut full_layout);
    }
    let detail = load::analyse_full(
        &layers.stack,
        &full_layout,
        &load::Extras {
            structure: Some(&layers.structure),
            needs: Some(needs),
        },
    );
    if routing.is_err() {
        termination = Termination::RouteFailed;
    } else if termination == Termination::Satisfied
        && (detail.contacts.iter().any(|c| c.ratio > 12.0)
            || !layers.structure.needs_anchor().into_iter().all(|island| {
                chosen.iter().any(|c| {
                    let z = c.site.xyz[2] + shift;
                    z >= layers.grid.z_bottom(island.first_layer) - 1e-8
                        && z < layers.grid.z_top(island.first_layer)
                        && (c.site.xyz[0] - island.first_xy[0])
                            .hypot(c.site.xyz[1] - island.first_xy[1])
                            < 0.5
                })
            }))
    {
        termination = Termination::FinalReplayUnsatisfied;
    }
    let load = detail.report;
    Proposal {
        routed: routing.is_ok(),
        routing_blocker,
        termination,
        candidates: chosen.into_iter().map(|x| x.candidate).collect(),
        load,
        layout,
        needs: NeedsMap::default(),
        rejections: cache.rejected.into_iter().collect(),
    }
}
