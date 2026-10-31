#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;

use super::{PlanSettings, Proposal, Termination, layout_from_contacts, unavailable};
use crate::construct::{CandId, ColumnCheck, ContactCandidate, Contacts, PlanBuilder, Quote};
use crate::load::needs::{NeedPoint, NeedsMap, SiteTag};
use crate::load::{self, LayoutView, ModelLayers, SiteClass};
use std::collections::{BTreeMap, BTreeSet};

type V3 = [f64; 3];
const FANS: [f64; 8] = [35., 30., 25., 20., 15., 10., 5., 0.];
const REACHES: [f32; 3] = [2.6, 2.8, 2.94];
const MAX_NECK: f32 = 0.39; // Search maximum; the measured 0.48 mm² bound still admits each tip.
fn geometry_key(c: &ContactCandidate) -> Vec<u32> {
    c.site
        .into_iter()
        .chain(c.axis)
        .chain([c.ball_r, c.standoff, c.neck, c.joint_r, c.head_len])
        .map(f32::to_bits)
        .collect()
}
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
    patch_history: Vec<(u32, f64)>,
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
    v.patch_history = chosen.iter().map(|x| x.patch_history.clone()).collect();
    v
}
#[derive(Clone, Copy)]
struct Target {
    xyz: V3,
    deadline: f64,
    load: f64,
    anchor: bool,
    grow: bool,
    sharing: bool,
}
fn covered(t: Target, chosen: &[Chosen], reach: f64, shift: f64) -> bool {
    chosen.iter().any(|x| {
        let centre = x.quote.contact().centre().map(f64::from);
        x.site.xyz[2] + shift < t.deadline + 1e-8 && d2(centre, t.xyz) <= reach * reach
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
                grow: false,
                sharing: false,
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
            deadline: layers.grid.z_top(
                ((p.xyz[2] + shift - layers.grid.origin) / layers.grid.height)
                    .floor()
                    .max(0.0) as u32,
            ) - 1e-7,
            load: 3.0,
            anchor: false,
            grow: false,
            sharing: false,
        };
        if !covered(t, chosen, pitch, shift)
            && !seeds.iter().any(|s| d2(s.xyz, t.xyz) < pitch * pitch)
        {
            seeds.push(t);
        }
    }
    // Coverage precedes optional carrier seeds; neither shares a budget with
    // load-sharing candidates. Earlier samples precede later samples.
    // Automatic rim contacts are mandatory too, but hidden sites have priority.
    for tag in [SiteTag::Hidden, SiteTag::PartlyVisible] {
        let mut region: Vec<Target> = Vec::new();
        let mut region_sites: Vec<_> = sites
            .iter()
            .filter(|p| p.class == tag && p.angle_deg < 45. && p.area_mm2 > 0.)
            .collect();
        region_sites.sort_by(|a, b| a.xyz[2].total_cmp(&b.xyz[2]));
        for p in region_sites {
            let t = Target {
                xyz: p.xyz,
                deadline: layers.grid.z_top(
                    ((p.xyz[2] + shift - layers.grid.origin) / layers.grid.height)
                        .floor()
                        .max(0.0) as u32,
                ) - 1e-7,
                load: 0.,
                anchor: false,
                grow: false,
                sharing: false,
            };
            if !covered(t, chosen, settings.coverage_reach_mm, shift)
                && !region.iter().any(|q| {
                    q.deadline <= t.deadline
                        && d2(q.xyz, t.xyz) < settings.coverage_reach_mm.powi(2)
                })
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
            grow: false,
            sharing: false,
        };
        if !covered(t, chosen, 2.13, shift) {
            targets.push(t);
        }
    }
    targets.extend(seeds);
    targets
}
fn load_targets(
    detail: &load::LoadDetail,
    chosen: &[Chosen],
    capped: &BTreeSet<[u32; 3]>,
    grid: &plinth_geom::slice::LayerGrid,
    shift: f64,
) -> Vec<Target> {
    // No distance ring. Target the actual component's load centroid, with its
    // exposure deadline. Additive deficits take precedence over spread search.
    let mut rows: Vec<_> = detail
        .groups
        .iter()
        .filter(|g| {
            g.x_group > 12.0
                || (!g.equilibrium_assessed && !g.active.is_empty())
                || g.states.iter().any(|s| {
                    s.ratios.rigid_lever > 12.0
                        && capped.contains(&site_key(chosen[s.contact as usize].site.xyz))
                })
        })
        .collect();
    rows.sort_by(|a, b| {
        b.x_group
            .total_cmp(&a.x_group)
            .then(b.outside_hull_mm.total_cmp(&a.outside_hull_mm))
            .then(a.layer.cmp(&b.layer))
    });
    let mut components = BTreeSet::new();
    rows.into_iter()
        .filter(|g| components.insert(g.component))
        .map(|g| Target {
            xyz: [
                g.centroid_xy[0],
                g.centroid_xy[1],
                grid.z_top(g.layer) - shift - 1e-6,
            ],
            deadline: grid.z_top(g.layer) - 1e-6,
            load: (g.area_mm2 / (g.active.len() + 1) as f64).clamp(0.25, 5.0),
            anchor: false,
            grow: false,
            sharing: true,
        })
        .collect()
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
    growth_searched: BTreeSet<[u32; 3]>,
    sharing_tried: BTreeSet<[u32; 3]>,
    patch_curves: BTreeMap<Vec<u32>, Vec<(u32, f64)>>,
}
impl Cache {
    fn certify_selected(
        &mut self,
        builder: &PlanBuilder<Contacts>,
        layers: &ModelLayers,
        chosen: &mut [Chosen],
        shift: f64,
    ) {
        let mut queries = Vec::new();
        let mut owners = Vec::new();
        for (i, x) in chosen.iter_mut().enumerate() {
            if !x.patch_history.is_empty() {
                continue;
            }
            let key = geometry_key(&x.candidate);
            if let Some(h) = self.patch_curves.get(&key) {
                x.patch_history = h.clone();
                continue;
            }
            let Ok(ball) = x.quote.contact().inner_ball() else {
                x.patch_history = vec![(0, 0.0)];
                continue;
            };
            let first = ((x.site.xyz[2] + shift - layers.grid.origin) / layers.grid.height)
                .floor()
                .max(0.0) as u32;
            let last = ((ball.centre[2] + ball.radius + shift - layers.grid.origin)
                / layers.grid.height)
                .ceil()
                .max(f64::from(first)) as u32;
            for k in first..=last {
                queries.push((ball, layers.grid.z_top(k) - shift));
                owners.push((i, k));
            }
        }
        if !queries.is_empty() {
            let mut bounds = Vec::new();
            if let Ok(bvh) = plinth_geom::Bvh::build(builder.model().mesh()) {
                plinth_geom::patch::ball_patches_below_with_workers(
                    &bvh,
                    &queries,
                    &mut bounds,
                    builder.workers(),
                );
            }
            for ((i, k), area) in owners.into_iter().zip(bounds) {
                let x = &mut chosen[i];
                // Measurement exhaustion is zero available certified area, not
                // permission to use the whole future patch or an analytic disc.
                let lo = area
                    .known()
                    .map_or(0.0, |a| a.lo)
                    .min(x.quote.contact().patch().lower());
                x.patch_history.push((k, lo));
            }
        }
        for x in chosen {
            if x.patch_history.is_empty() {
                x.patch_history.push((0, 0.0));
            }
            self.patch_curves
                .insert(geometry_key(&x.candidate), x.patch_history.clone());
        }
    }
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
                            && (!target.sharing || d2(p.xyz, target.xyz) <= 25.0)
                            && (!target.anchor || p.xyz[2] + shift >= target.deadline - 0.05)
                            && chosen
                                .iter()
                                .all(|c| d2(c.site.xyz, p.xyz) > 0.85_f64.powi(2))
                            && !self.blocked.contains(&site_key(p.xyz))
                            && (!target.sharing || !self.sharing_tried.contains(&site_key(p.xyz)))
                    })
                    .collect::<Vec<_>>()
            },
            |p| vec![p],
        );
        let score = |p: &NeedPoint| {
            d2(p.xyz, target.xyz)
                - if target.sharing {
                    1.2 * chosen
                        .iter()
                        .map(|c| d2(c.site.xyz, p.xyz))
                        .fold(25.0, f64::min)
                } else {
                    0.0
                }
                + if (target.anchor || target.sharing) && p.class != SiteTag::Hidden {
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
        domain: (&[NeedPoint], f64),
        chosen: &[Chosen],
        fallback: bool,
    ) -> Vec<Vec<NeedPoint>> {
        let (sites, shift) = domain;
        self.growth_searched.clear();
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
            let proposed = (((load / 12. / 2.85).sqrt() * 1000.0).ceil() / 1000.0)
                .clamp(0.053, f64::from(MAX_NECK)) as f32;
            for p in near {
                // Growth gets the SAME complete fan x length domain as column
                // sizing, before a single load-sharing neighbour is proposed.
                // Never call a budget-skipped search a geometric cap.
                let fans: &[f64] = if target.grow || target.sharing {
                    &FANS
                } else {
                    &[20., 0.]
                };
                let reaches: &[f32] = if target.grow || target.sharing {
                    &REACHES
                } else {
                    &[2.6]
                };
                let mut sizes = vec![proposed];
                if target.sharing && proposed > 0.25 {
                    sizes.push(0.25);
                }
                if target.grow && fallback {
                    let current = chosen
                        .iter()
                        .find(|x| d2(x.site.xyz, p.xyz) < 1e-12)
                        .expect("growth replaces an existing site")
                        .candidate
                        .neck;
                    // Only after the requested enlargement fails, search an
                    // intermediate size and the 0.001 mm step with the full fan.
                    sizes = vec![(proposed + current) * 0.5, (current + 0.001).min(MAX_NECK)];
                    sizes.sort_by(|a, b| b.total_cmp(a));
                    sizes.dedup();
                }
                if alternatives.len() + fans.len() * reaches.len() * sizes.len() > 384 {
                    continue;
                }
                let before = alternatives.len();
                for neck in sizes {
                    for &fan in fans {
                        for &reach in reaches {
                            // The load envelope sizes COLUMNS, not permission to grow
                            // the tip. Start with the smallest joint for this tip;
                            // size_columns can try a larger one after load replay.
                            let joint_load = if target.grow { 0.0 } else { load };
                            let mut cand = candidate(self.next_id, &p, neck, fan, joint_load);
                            cand.head_len = (reach - cand.joint_r - cand.standoff).max(1.5);
                            if !self.seen.insert(geometry_key(&cand)) {
                                continue;
                            }
                            alternatives.push(cand);
                            self.next_id += 1;
                            sampled.push(p);
                            sample_loads.push(load);
                        }
                    }
                }
                if target.grow {
                    self.growth_searched.insert(site_key(p.xyz));
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
                    patch_history: Vec::new(),
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
            for fan in FANS {
                for reach in REACHES {
                    let mut cand = c.candidate;
                    cand.id = CandId(self.next_id);
                    self.next_id += 1;
                    cand.joint_r = 1.04 * radius.max(1.5 * cand.neck);
                    cand.head_len = (reach - cand.joint_r - cand.standoff).max(1.5);
                    cand.axis = axis(c.site.normal, fan);
                    if !self.seen.insert(geometry_key(&cand)) {
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
                        patch_history: Vec::new(),
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
                        patch_history: Vec::new(),
                    });
                }
            }
        }
    }
    /// Return only sites at the tip search limit, or whose full growth search
    /// cannot select a larger certified tip. Newly grown tips get a load replay
    /// and another round before any neighbours can be added for them.
    fn grow_tips(
        &mut self,
        builder: &PlanBuilder<Contacts>,
        chosen: &mut Vec<Chosen>,
        detail: &load::LoadDetail,
    ) -> BTreeSet<[u32; 3]> {
        let loads = super::routing_loads(detail);
        let mut capped = BTreeSet::new();
        let mut tasks = Vec::new();
        let mut before = Vec::new();
        for ((x, c), load) in chosen.iter().zip(&detail.contacts).zip(loads) {
            if c.ratio <= 12.0 {
                continue;
            }
            let key = site_key(x.site.xyz);
            if x.candidate.neck >= MAX_NECK {
                capped.insert(key);
                continue;
            }
            let envelope = self.load_envelope.entry(key).or_default();
            *envelope = envelope.max(load);
            // Include the layer-specific shortfall, not only the full-patch load.
            let load = (*envelope)
                .max(x.quote.full_capacity() * c.ratio / 12.0)
                .max(12.0 * 2.85 * f64::from(x.candidate.neck + 0.001).powi(2));
            tasks.push((
                Target {
                    xyz: x.site.xyz,
                    deadline: f64::INFINITY,
                    load,
                    anchor: false,
                    grow: true,
                    sharing: false,
                },
                Some(x.site),
            ));
            before.push((key, x.candidate.ball_r));
        }
        let near = self.measure(builder, &tasks, (&[], 0.0), chosen, false);
        self.select(tasks.clone(), near, chosen);
        let fallback: Vec<_> = tasks
            .into_iter()
            .filter(|(t, _)| {
                let key = site_key(t.xyz);
                self.growth_searched.contains(&key)
                    && before.iter().any(|&(k, r)| {
                        k == key
                            && chosen
                                .iter()
                                .any(|x| site_key(x.site.xyz) == key && x.candidate.ball_r <= r)
                    })
            })
            .collect();
        let near = self.measure(builder, &fallback, (&[], 0.0), chosen, true);
        self.select(fallback, near, chosen);
        for (key, radius) in before {
            if self.growth_searched.contains(&key)
                && chosen
                    .iter()
                    .any(|x| site_key(x.site.xyz) == key && x.candidate.ball_r <= radius)
            {
                capped.insert(key);
            }
        }
        capped
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
                        && (old.is_some()
                            || target.sharing
                            || d2(q.quote.contact().centre().map(f64::from), target.xyz)
                                <= 3.0_f64.powi(2))
                        && (old.is_some()
                            || !self
                                .blocked
                                .contains(&site_key(q.candidate.site.map(f64::from))))
                        && if target.grow {
                            replacing.is_some_and(|i| {
                                q.candidate.ball_r > chosen[i].candidate.ball_r
                                    && q.quote.full_capacity()
                                        > chosen[i].quote.full_capacity() + 1e-9
                            })
                        } else {
                            q.quote.contact().required_column_radius(target.load)
                                <= q.column.radius()
                                && q.quote.contact().required_column_radius(target.load)
                                    <= q.candidate.joint_r / 1.04
                        }
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
            } else if old.is_none() {
                // Move the bounded search on from measured but pair-incompatible
                // sites. Otherwise the same nearest-32 window repeats forever.
                for p in near {
                    if self.available.iter().any(|q| d2(q.site.xyz, p.xyz) < 1e-12) {
                        self.blocked.insert(site_key(p.xyz));
                    }
                }
            }
        }
    }
}
/// Replay the exact routed members, including self-peel and braces, at ALL
/// layers/components. A routing failure cannot retain a load-only candidate.
fn complete_replay(
    builder: &PlanBuilder<Contacts>,
    layers: &ModelLayers,
    chosen: &[Chosen],
    shift: f64,
) -> Option<load::LoadDetail> {
    let mut layout = view(chosen, layers, shift);
    let detail = load::analyse_full(&layers.stack, &layout, &load::Extras::default());
    let refs: Vec<_> = chosen.iter().map(|c| &c.quote).collect();
    let plan = builder
        .preview_plan(&refs, &super::routing_loads(&detail))
        .ok()?;
    super::add_members(&plan, &mut layout);
    Some(load::analyse_full(
        &layers.stack,
        &layout,
        &load::Extras::default(),
    ))
}

fn replay_improves(before: &load::LoadDetail, after: &load::LoadDetail) -> bool {
    let mut improvement = false;
    for (b, a) in before.groups.iter().zip(&after.groups) {
        if (b.component, b.layer) != (a.component, a.layer) {
            return false;
        }
        let model_max = |g: &load::GroupLoad, rigid: bool| {
            g.states
                .iter()
                .map(|s| {
                    if rigid {
                        s.ratios.rigid_lever
                    } else {
                        s.ratios.equal_share
                    }
                })
                .fold(0.0, f64::max)
        };
        for (old, new, comparable) in [
            (b.x_group, a.x_group, true),
            (
                if b.equilibrium_assessed {
                    model_max(b, true)
                } else {
                    b.x_group
                },
                model_max(a, true),
                b.equilibrium_assessed && a.equilibrium_assessed,
            ),
            (model_max(b, false), model_max(a, false), true),
        ] {
            if new > old.max(12.0) + 1e-7 {
                return false;
            }
            improvement |= comparable && old > 12.0 && new + 1e-6 < old;
        }
        // No previously valid exposure can become unresolved. An existing
        // unresolved exposure may improve its spread, without worsening another.
        if b.equilibrium_assessed && !a.equilibrium_assessed {
            return false;
        }
        if !b.equilibrium_assessed {
            let metric = |g: &load::GroupLoad| {
                g.outside_hull_mm
                    + g.force_residual
                    + g.moment_residual
                    + g.condition.clamp(1.0, 1e12).log10()
            };
            if metric(a) > metric(b) + 1e-7 {
                return false;
            }
            improvement |= a.equilibrium_assessed || metric(a) + 1e-6 < metric(b);
        }
        // Check each pre-existing contact too: a lower group maximum cannot
        // hide a newly overloaded old section elsewhere in the same layer.
        for old in &b.states {
            if let Some(new) = a.states.iter().find(|s| s.contact == old.contact) {
                for (o, n) in [
                    (
                        if b.equilibrium_assessed {
                            old.ratios.rigid_lever
                        } else {
                            b.x_group
                        },
                        new.ratios.rigid_lever,
                    ),
                    (old.ratios.equal_share, new.ratios.equal_share),
                ] {
                    if n > o.max(12.0) + 1e-7 {
                        return false;
                    }
                }
            }
        }
    }
    improvement && before.groups.len() == after.groups.len()
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
    let mut cache = Cache::default();
    let valid = !unavailable(&layers.stack)
        && settings.coverage_reach_mm.is_finite()
        && settings.coverage_reach_mm > 0.;
    let mut termination = Termination::NotBuilt;
    for round in 0..settings.max_rounds.min(8) {
        if !valid {
            break;
        }
        let layout = view(&chosen, layers, shift);
        let detail = load::analyse_full(&layers.stack, &layout, &load::Extras::default());
        // [O] Load grows tips first. Only a completed, unsuccessful growth search
        // (or a tip already at the search cap) can request load-sharing neighbours.
        let capped = cache.grow_tips(builder, &mut chosen, &detail);
        cache.certify_selected(builder, layers, &mut chosen, shift);
        let detail = load::analyse_full(
            &layers.stack,
            &view(&chosen, layers, shift),
            &load::Extras::default(),
        );
        // Mandatory anchoring/coverage has a separate batch. The old rotation
        // mixed replacements and 16 ring tasks per capped tip into this same
        // 384-alternative budget, starving coverage. Never do that again.
        let targets = needs_targets(layers, sites, &chosen, settings, shift);
        let tasks: Vec<_> = targets.into_iter().map(|t| (t, None)).collect();
        let near = cache.measure(builder, &tasks, (sites, shift), &chosen, false);
        cache.select(tasks, near, &mut chosen);
        cache.certify_selected(builder, layers, &mut chosen, shift);
        // One load-only proposal per round [S], not a contact-count target.
        // Its complete routed geometry must improve the replay before retention.
        let targets = load_targets(&detail, &chosen, &capped, &layers.grid, shift);
        let tasks: Vec<_> = targets.into_iter().take(1).map(|t| (t, None)).collect();
        if !tasks.is_empty() {
            let before = complete_replay(builder, layers, &chosen, shift);
            let count = chosen.len();
            let near = cache.measure(builder, &tasks, (sites, shift), &chosen, false);
            cache.select(tasks, near, &mut chosen);
            cache.certify_selected(builder, layers, &mut chosen, shift);
            if chosen.len() > count {
                let key = site_key(chosen.last().expect("added sharing candidate").site.xyz);
                cache.sharing_tried.insert(key);
                let after = complete_replay(builder, layers, &chosen, shift);
                let keep = before
                    .as_ref()
                    .zip(after.as_ref())
                    .is_some_and(|(b, a)| replay_improves(b, a));
                *cache
                    .rejected
                    .entry(
                        if keep {
                            "load-only complete replay kept"
                        } else {
                            "load-only complete replay dropped"
                        }
                        .into(),
                    )
                    .or_default() += 1;
                if !keep {
                    cache.available.push(chosen.pop().expect("added candidate"));
                }
            }
        }
        let _ = round;
        let selected = load::analyse_full(
            &layers.stack,
            &view(&chosen, layers, shift),
            &load::Extras::default(),
        );
        cache.size_columns(builder, &mut chosen, &super::routing_loads(&selected));
        cache.certify_selected(builder, layers, &mut chosen, shift);
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
    let routing_blocker = routing
        .as_ref()
        .err()
        .map(|e| format!("{:?}; candidates {:?}", e.reject, e.candidates));
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
