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
            for dx in [radius, -radius] {
                out.push(Target {
                    xyz: if layer == first {
                        [
                            x.site.xyz[0] + dx,
                            x.site.xyz[1],
                            (deadline - shift).min(x.site.xyz[2] + radius),
                        ]
                    } else {
                        [
                            c.tributary_xy[0] + dx,
                            c.tributary_xy[1],
                            deadline - shift - 0.025,
                        ]
                    },
                    deadline,
                    load: (c.load_mm2 * 0.5).clamp(0.25, 12.0),
                    anchor: false,
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
                        let cand = candidate(self.next_id, &p, neck, fan, load.max(12.0));
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
                        if alternatives.len() >= 96 || !self.seen.insert(key) {
                            continue;
                        }
                        alternatives.push(cand);
                        self.next_id += 1;
                        sampled.push(p);
                        sample_loads.push(load.max(12.0));
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
        let mut column_rejects = Vec::new();
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
                    .inspect_err(|_| column_rejects.push(site_key(candidate.site.map(f64::from))))
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
        for site in column_rejects {
            if !self
                .available
                .iter()
                .chain(chosen)
                .any(|q| site_key(q.candidate.site.map(f64::from)) == site)
            {
                self.blocked.insert(site);
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
        chosen: &mut Vec<Chosen>,
        loads: &[f64],
    ) {
        let mut candidates = Vec::new();
        let mut sites = Vec::new();
        let mut estimates = Vec::new();
        let old = std::mem::take(chosen);
        for (mut c, &load) in old.into_iter().zip(loads) {
            if c.quote.contact().column_radius(load) <= c.candidate.joint_r / 1.04 {
                chosen.push(c);
                continue;
            }
            // Reserve for the larger head's self-peel; the exact new load is
            // replayed at route admission. This reserve is a search value [S].
            let estimate = load * 1.1 + 1.0;
            c.candidate.id = CandId(self.next_id);
            self.next_id += 1;
            let radius = ((estimate / 12.0 / 3.0615).sqrt() as f32)
                .next_up()
                .max(0.25);
            c.candidate.joint_r = 1.04 * radius.max(1.5 * c.candidate.neck);
            c.candidate.head_len = (2.6 - c.candidate.joint_r - c.candidate.standoff).max(1.5);
            // A larger joint may need a less vertical departure to keep the
            // neighbouring sheet gap. Re-probe the normal direction; do not
            // reuse the smaller head's departure certificate.
            c.candidate.axis = axis(c.site.normal, 0.0);
            candidates.push(c.candidate);
            sites.push(c.site);
            estimates.push(estimate);
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
        for (((candidate, site), _load), answer) in candidates
            .into_iter()
            .zip(sites)
            .zip(estimates)
            .zip(answers)
        {
            let answer = answer.and_then(|quote| {
                columns
                    .next()
                    .expect("one column per quote")
                    .map(|column| (quote, column))
            });
            match answer {
                Ok((quote, column)) => chosen.push(Chosen {
                    candidate,
                    site,
                    quote,
                    column,
                }),
                Err(e) => {
                    self.blocked.insert(site_key(candidate.site.map(f64::from)));
                    *self.rejected.entry(format!("sizing {e:?}")).or_default() += 1;
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
        self.available.append(chosen);
        for ((target, _), near) in tasks.into_iter().zip(near) {
            let mut opts: Vec<_> = self
                .available
                .iter()
                .enumerate()
                .filter(|(_, q)| {
                    near.iter().any(|p| d2(q.site.xyz, p.xyz) < 1e-12)
                        && !self
                            .blocked
                            .contains(&site_key(q.candidate.site.map(f64::from)))
                        && q.quote.contact().column_radius(target.load) <= q.column.radius()
                        && q.quote.contact().column_radius(target.load)
                            <= q.candidate.joint_r / 1.04
                        && chosen.iter().all(|c| {
                            separated(&q.quote, &c.quote) && q.column.compatible(&c.column)
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
                chosen.push(self.available.swap_remove(i));
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
                (
                    Target {
                        xyz: x.site.xyz,
                        deadline: f64::INFINITY,
                        load,
                        anchor: false,
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
        let detail = load::analyse_full(
            &layers.stack,
            &layout,
            &load::Extras {
                structure: Some(&layers.structure),
                needs: Some(needs),
            },
        );
        let refs: Vec<_> = chosen.iter().map(|c| &c.quote).collect();
        let loads = super::routing_loads(&detail);
        let routing = builder.preview_routing_located(&refs, &loads);
        if routing.is_ok()
            && needs_targets(layers, sites, &chosen, settings, shift).is_empty()
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
        if let Err(failure) = routing {
            *cache
                .rejected
                .entry(format!("route {:?}", failure.reject))
                .or_default() += 1;
            // A route rejection is feedback, not an after-the-budget surprise.
            // Keep the last round's rejected layout for an honest diagnostic.
            if round + 1 < settings.max_rounds.min(8) {
                for c in &chosen {
                    if failure.candidates.contains(&c.candidate.id) {
                        cache
                            .blocked
                            .insert(site_key(c.candidate.site.map(f64::from)));
                    }
                }
                chosen.retain(|c| {
                    !cache
                        .blocked
                        .contains(&site_key(c.candidate.site.map(f64::from)))
                });
            }
        }
        // No changed==false exit: all eight rounds can open fresh sites.
        termination = Termination::SearchBudgetExhausted;
    }
    finish(builder, layers, shift, chosen, cache, termination)
}

fn finish(
    builder: &PlanBuilder<Contacts>,
    layers: &ModelLayers,
    shift: f64,
    mut chosen: Vec<Chosen>,
    cache: Cache,
    mut termination: Termination,
) -> Proposal {
    chosen.sort_by_key(|c| c.candidate.id);
    let layout = view(&chosen, layers, shift);
    let detail = load::analyse_full(&layers.stack, &layout, &load::Extras::default());
    let refs: Vec<_> = chosen.iter().map(|c| &c.quote).collect();
    let loads = super::routing_loads(&detail);
    let routing = builder.preview_routing(&refs, &loads);
    let routing_blocker = routing.as_ref().err().map(|e| format!("{e:?}"));
    if routing.is_err() && termination == Termination::Satisfied {
        termination = Termination::SearchBudgetExhausted;
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
