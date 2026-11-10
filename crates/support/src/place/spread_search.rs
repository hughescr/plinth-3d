//! Private spread trials. One complete replay baseline per immutable layout.
use super::*;

pub(super) struct Search<'a> {
    pub builder: &'a PlanBuilder<Contacts>,
    pub layers: &'a ModelLayers,
    pub sites: &'a [NeedPoint],
    pub shift: f64,
}
impl Search<'_> {
    pub fn refine(
        &self,
        cache: &mut Cache,
        chosen: &mut Vec<Chosen>,
        detail: &mut load::LoadDetail,
        capped: &BTreeSet<[u32; 3]>,
        stable: bool,
    ) -> bool {
        let current: Vec<_> = chosen.iter().map(|c| c.candidate.id).collect();
        if current != cache.sharing_layout {
            cache.exhausted_targets.clear();
            cache.sharing_tried.clear();
            cache.sharing_pair_blocked.clear();
            cache.unsafe_trials.clear();
            cache.rejected_legal_trial = false;
            cache.sharing_layout = current;
        }
        let mut targets = load_targets(detail, chosen, capped, &self.layers.grid, self.shift);
        // Mandatory, independent island capacity is not optional parent sharing.
        targets.sort_by_key(|t| {
            !t.group.is_some_and(|(c, k)| {
                cache.mergers[c as usize].is_some()
                    && detail
                        .groups
                        .iter()
                        .any(|g| g.component == c && g.layer == k && g.x_group > 12.0)
            })
        });
        let mut baseline = None;
        for target in targets {
            let group = target.group.expect("sharing exposure");
            if cache.exhausted_targets.contains(&group) {
                continue;
            }
            let capacity_component = target.group.map(|g| g.0).filter(|&c| {
                cache.mergers[c as usize].is_some()
                    && detail
                        .groups
                        .iter()
                        .any(|g| g.component == c && g.x_group > 12.0)
            });
            // Do not replay doomed optional trials on a transient carrier that
            // mandatory placement or tip growth will change again next round.
            if !stable && capacity_component.is_none() {
                continue;
            }
            cache.island_capacity_target = capacity_component.is_some();
            // One canonical load-only proposal family [S] (5 mm² seed load,
            // necks 0.39/0.383 mm) reuses identical immutable head/column probes
            // across exposures. This is NOT a sizing certificate: full replay
            // still recomputes every actual reaction and available section.
            let target = if capacity_component.is_none() {
                Target {
                    load: 5.0,
                    ..target
                }
            } else {
                target
            };
            if self.try_target(cache, chosen, detail, target, &mut baseline) {
                return true;
            }
            cache.exhausted_targets.insert(group);
        }
        false
    }
    fn try_target(
        &self,
        cache: &mut Cache,
        chosen: &mut Vec<Chosen>,
        detail: &mut load::LoadDetail,
        target: Target,
        baseline: &mut Option<Option<load::LoadDetail>>,
    ) -> bool {
        let group = target.group.expect("sharing exposure");
        let capacity_component = cache.island_capacity_target.then_some(group.0);
        let tasks = vec![(target, None)];
        cache.sharing_target_tried.clear();
        let mut near = cache.measure(
            self.builder,
            &tasks,
            (self.sites, self.shift),
            chosen,
            false,
        );
        // Three distinct site pairs per exposure [S], not one rejected pair
        // called exhaustion. This is a finite batch, not a continuous-site proof.
        let mut tried_here = BTreeSet::new();
        for attempt in 0..3 {
            let filtered: Vec<Vec<NeedPoint>> = near
                .iter()
                .map(|sites| {
                    sites
                        .iter()
                        .copied()
                        .filter(|p| {
                            !tried_here.contains(&site_key(p.xyz))
                                && (capacity_component.is_some()
                                    || !cache.sharing_tried.contains(&site_key(p.xyz)))
                        })
                        .collect()
                })
                .collect();
            if filtered.iter().all(Vec::is_empty) {
                sharing::record(cache, group, "SelectedSiteBatchExhausted", 1);
                if attempt < 2 && capacity_component.is_none() {
                    near = cache.measure(
                        self.builder,
                        &tasks,
                        (self.sites, self.shift),
                        chosen,
                        false,
                    );
                    continue;
                }
                break;
            }
            let count = chosen.len();
            // Two springs can be necessary together to widen a singular row.
            // The private pair is atomic; rejection restores the exact baseline.
            cache.select(
                tasks.iter().copied().cycle().take(2).collect(),
                filtered.iter().cloned().cycle().take(2).collect(),
                chosen,
            );
            if chosen.len() == count {
                // Search exclusions (0.85 mm, cells and tried sites) are not
                // geometric rejections. Never label their effect "no legal site".
                sharing::record(cache, group, "NoPairCompatibleCandidateInFilteredBatch", 1);
                if attempt < 2 && capacity_component.is_none() {
                    near = cache.measure(
                        self.builder,
                        &tasks,
                        (self.sites, self.shift),
                        chosen,
                        false,
                    );
                    continue;
                }
                break;
            }
            for c in &chosen[count..] {
                tried_here.insert(site_key(c.site.xyz));
                cache.sharing_target_tried.insert(site_key(c.site.xyz));
                sharing::record(
                    cache,
                    group,
                    &format!(
                        "trial site ({:.3},{:.3},{:.3}) neck {:.3}",
                        c.site.xyz[0], c.site.xyz[1], c.site.xyz[2], c.candidate.neck
                    ),
                    1,
                );
            }
            if self.test_pair(cache, chosen, detail, (target, count), baseline) {
                return true;
            }
        }
        sharing::record(cache, group, "FinitePairBatchFinishedNotInfeasibility", 1);
        false
    }
    fn test_pair(
        &self,
        cache: &mut Cache,
        chosen: &mut Vec<Chosen>,
        detail: &mut load::LoadDetail,
        trial: (Target, usize),
        baseline: &mut Option<Option<load::LoadDetail>>,
    ) -> bool {
        let (target, count) = trial;
        let group = target.group.expect("sharing exposure");
        let capacity_component = cache.island_capacity_target.then_some(group.0);
        let originals = chosen[..count].iter().map(|c| c.candidate.id).collect();
        let mut trial_key: Vec<_> = chosen[count..]
            .iter()
            .map(|c| geometry_key(&c.candidate))
            .collect();
        trial_key.sort();
        if capacity_component.is_none() && cache.unsafe_trials.contains(&trial_key) {
            cache.available.extend(chosen.drain(count..));
            cache.rejected_legal_trial = true;
            sharing::record(cache, group, "cached numerically worse trial skipped", 1);
            return false;
        }
        cache.certify_selected(self.builder, self.layers, chosen, self.shift);
        // Only append before capturing the baseline. No old private geometry
        // has been resized yet, so this is exactly the pre-trial prefix.
        let before = baseline.get_or_insert_with(|| {
            complete_replay(self.builder, self.layers, &chosen[..count], self.shift)
        });
        let original_envelope = cache.load_envelope.clone();
        let trial = load::analyse_full(
            &self.layers.stack,
            &view(chosen, self.layers, self.shift),
            &load::Extras::default(),
        );
        // Re-size columns on the entire private trial, not unrelated old tips.
        // Rejection restores both all replacements and the load envelope.
        cache.size_columns(self.builder, chosen, &crate::place::routing_loads(&trial));
        cache.certify_selected(self.builder, self.layers, chosen, self.shift);
        let mut layout = view(chosen, self.layers, self.shift);
        let model_load = load::analyse_full(&self.layers.stack, &layout, &load::Extras::default());
        // Equal-share at a fixed new patch is a lower bound before member
        // self-peel: routing adds nonnegative area, no contacts or patch
        // capacity. This can REJECT only. Every kept trial still needs the
        // complete routed replay, with every old violation compared.
        let early_reject =
            capacity_component.is_none() && new_patch_equal_overload(&model_load, &layout, count);
        let after = if early_reject {
            Some(model_load)
        } else {
            route_replay(self.builder, self.layers, chosen, &mut layout, &model_load)
        };
        let keep = !early_reject
            && before.as_ref().zip(after.as_ref()).is_some_and(|(b, a)| {
                capacity_component.map_or_else(
                    || replay_improves(b, a, group),
                    |c| island_capacity_improves(b, a, c),
                )
            });
        let event = if keep && capacity_component.is_some() {
            "mandatory island capacity complete replay kept"
        } else if keep {
            "load-only complete replay kept"
        } else if early_reject {
            "load-only model-area patch lower bound rejected"
        } else {
            "load-only complete replay dropped"
        };
        *cache.rejected.entry(event.into()).or_default() += 1;
        if !keep {
            let reason = if early_reject {
                patch_lower_bound_blocker(
                    after.as_ref().expect("model-area result"),
                    &layout,
                    count,
                )
            } else {
                before.as_ref().zip(after.as_ref()).map_or_else(
                    || "complete route unavailable".into(),
                    |(b, a)| sharing::replay_blocker(b, a),
                )
            };
            sharing::record(
                cache,
                group,
                &format!(
                    "{} blocker {reason}",
                    if early_reject {
                        "model-area patch lower bound"
                    } else {
                        "replay"
                    }
                ),
                1,
            );
            if capacity_component.is_none()
                && let Some(after) = &after
            {
                exclude_own_overloads(cache, group, after, &layout, chosen, (count, early_reject));
            }
            if capacity_component.is_none()
                && !early_reject
                && before
                    .as_ref()
                    .zip(after.as_ref())
                    .is_some_and(|(b, a)| !sharing::replay_no_worse(b, a))
            {
                cache.unsafe_trials.insert(trial_key);
            }
        }
        sharing::record(
            cache,
            group,
            if keep {
                "complete replay kept"
            } else if early_reject {
                "model-area patch lower bound rejected"
            } else {
                "complete replay rejected"
            },
            1,
        );
        if keep {
            *detail = load::analyse_full(
                &self.layers.stack,
                &view(chosen, self.layers, self.shift),
                &load::Extras::default(),
            );
            return true;
        } else {
            cache.rejected_legal_trial = true;
            cache.rollback_trial(chosen, original_envelope, originals);
        }
        keep
    }
}

// The sampled site's own overload fails independently of which exposure
// requested this trial. This layout-local exclusion is a search filter, not a
// geometric infeasibility claim. The reject-only shortcut uses patch equal-share
// ONLY; its provisional rigid response is not a routed replay.
fn exclude_own_overloads(
    cache: &mut Cache,
    group: (u32, u32),
    after: &load::LoadDetail,
    layout: &LayoutView,
    chosen: &[Chosen],
    trial: (usize, bool),
) {
    let (count, early_reject) = trial;
    for g in &after.groups {
        for s in &g.section_states {
            if s.contact as usize >= count
                && (!early_reject || is_patch(layout, s))
                && (s.ratios.equal_share > 12.0 + 1e-7
                    || (!early_reject
                        && g.equilibrium_assessed
                        && s.ratios.rigid_lever > 12.0 + 1e-7))
            {
                let c = &chosen[s.contact as usize];
                if cache.sharing_tried.insert(site_key(c.site.xyz)) {
                    sharing::record(
                        cache,
                        group,
                        &format!(
                            "exclude own-overload site ({:.3},{:.3},{:.3}) C{} L{} equal {:.6} rigid {:.6}",
                            c.site.xyz[0],
                            c.site.xyz[1],
                            c.site.xyz[2],
                            g.component,
                            g.layer,
                            s.ratios.equal_share,
                            s.ratios.rigid_lever
                        ),
                        1,
                    );
                }
            }
        }
    }
}

fn is_patch(layout: &LayoutView, s: &load::analyse::ContactState) -> bool {
    layout
        .chains
        .get(s.contact as usize)
        .and_then(|c| c.parts.get(s.part as usize))
        .is_some_and(|p| p.kind == load::PartKind::Patch)
}
pub(super) fn new_patch_equal_overload(
    detail: &load::LoadDetail,
    layout: &LayoutView,
    first_new: usize,
) -> bool {
    first_new_patch_overload(detail, layout, first_new).is_some()
}
fn first_new_patch_overload<'a>(
    detail: &'a load::LoadDetail,
    layout: &LayoutView,
    first_new: usize,
) -> Option<(&'a load::GroupLoad, &'a load::analyse::ContactState)> {
    detail.groups.iter().find_map(|g| {
        g.section_states
            .iter()
            .find(|s| {
                s.contact as usize >= first_new
                    && is_patch(layout, s)
                    && s.ratios.equal_share > 12.0 + 1e-7
            })
            .map(|s| (g, s))
    })
}
fn patch_lower_bound_blocker(
    detail: &load::LoadDetail,
    layout: &LayoutView,
    first_new: usize,
) -> String {
    first_new_patch_overload(detail, layout, first_new).map_or_else(
        || "no new-patch equal-share lower-bound violation".into(),
        |(g, s)| {
            format!(
                "C{} L{} new contact {} patch equal {:.6} on {:.6} mm2 printed patch",
                g.component, g.layer, s.contact, s.ratios.equal_share, s.patch_mm2
            )
        },
    )
}

pub(super) fn cache_pair_conflicts(
    cache: &mut Cache,
    near: &[NeedPoint],
    chosen: &[Chosen],
    opts: &[usize],
) {
    if !cache.island_capacity_target && chosen.len() == cache.sharing_layout.len() {
        // Cache only conflicts against the immutable baseline, never against
        // the first member of a transient private pair.
        for p in near {
            if !opts
                .iter()
                .any(|&i| d2(cache.available[i].site.xyz, p.xyz) < 1e-12)
            {
                cache.sharing_pair_blocked.insert(site_key(p.xyz));
            }
        }
    }
}

pub(super) fn audit_selection(
    cache: &mut Cache,
    target: Target,
    near: &[NeedPoint],
    chosen: &[Chosen],
) {
    let mut counts = BTreeMap::<&str, u64>::new();
    for q in &cache.available {
        if !near.iter().any(|p| d2(q.site.xyz, p.xyz) < 1e-12) {
            continue;
        }
        let pair_failed = |kind| {
            chosen.iter().any(|c| {
                cache.pairs.get(&(
                    q.candidate.id.min(c.candidate.id),
                    q.candidate.id.max(c.candidate.id),
                )) == Some(&kind)
            })
        };
        let reason = if cache
            .blocked
            .contains(&site_key(q.candidate.site.map(f64::from)))
        {
            "blocked"
        } else if q.quote.contact().required_column_radius(target.load) > q.column.radius()
            || q.quote.contact().required_column_radius(target.load) > q.candidate.joint_r / 1.04
        {
            "undersized column/joint"
        } else if pair_failed(PairFit::HeadCollision) {
            "head pair collision"
        } else if pair_failed(PairFit::ColumnCollision) {
            "column pair collision"
        } else {
            "eligible"
        };
        *counts.entry(reason).or_default() += 1;
    }
    for (reason, count) in counts {
        sharing::record(
            cache,
            target.group.expect("sharing exposure"),
            &format!("selection {reason}"),
            count,
        );
    }
}
