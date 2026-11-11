//! Load-only search ranking and complete-replay retention. Search settings are
//! not geometric spacing laws or a proof that no continuous site exists.
use super::*;

pub(super) fn record(cache: &mut Cache, (c, k): (u32, u32), reason: &str, count: u64) {
    *cache
        .rejected
        .entry(format!("sharing C{c} L{k}: {reason}"))
        .or_default() += count;
}

/// Target distance has a substantial weight beside the spread reward [S],
/// unlike the old 0.02 weight. Z distance distinguishes exposure heights too.
/// This is a proposal heuristic; complete replay alone can retain a trial.
pub(super) fn site_score(p: &NeedPoint, target: Target, chosen: &[Chosen]) -> f64 {
    let nearest = chosen
        .iter()
        .map(|c| (c.site.xyz[0] - p.xyz[0]).powi(2) + (c.site.xyz[1] - p.xyz[1]).powi(2))
        .fold(25.0, f64::min);
    0.25 * ((p.xyz[0] - target.xyz[0]).powi(2) + (p.xyz[1] - target.xyz[1]).powi(2))
        + 0.02 * (target.xyz[2] - p.xyz[2]).powi(2)
        - nearest
        + if p.class == SiteTag::Hidden {
            0.0
        } else {
            10000.0
        }
}

pub(super) fn spread_sites(
    cache: &mut Cache,
    target: Target,
    sites: &[NeedPoint],
    shift: f64,
    chosen: &[Chosen],
) -> Vec<NeedPoint> {
    let mut reasons = BTreeMap::<&str, u64>::new();
    // Eight spatially distinct cells [S], rather than eight almost coincident
    // samples nearest the centroid. Search the whole printed component; its
    // resultant need not have an exposed support face within a 5 mm sphere.
    let score = |p: &NeedPoint| site_score(p, target, chosen);
    let mut cells = BTreeMap::<(i32, i32), NeedPoint>::new();
    for p in sites {
        let reason = if p.xyz[2] + shift >= target.deadline {
            "future"
        } else if !cache.same_group(p, target) {
            "other component"
        } else if chosen
            .iter()
            .any(|c| d2(c.site.xyz, p.xyz) <= 0.85_f64.powi(2))
        {
            "within 0.85mm of existing site"
        } else if cache.blocked.contains(&site_key(p.xyz)) {
            "blocked by mandatory search"
        } else if cache.sharing_tried.contains(&site_key(p.xyz)) {
            "new-contact overload at site in this layout"
        } else if cache.sharing_pair_blocked.contains(&site_key(p.xyz)) {
            "measured pair-incompatible site in this layout"
        } else if cache.sharing_target_tried.contains(&site_key(p.xyz)) {
            "site already tried for this exposure"
        } else {
            "eligible samples"
        };
        *reasons.entry(reason).or_default() += 1;
        if reason != "eligible samples" {
            continue;
        }
        let cell = (p.xyz[0].floor() as i32, p.xyz[1].floor() as i32);
        cells
            .entry(cell)
            .and_modify(|old| {
                if score(p) < score(old) {
                    *old = *p;
                }
            })
            .or_insert(*p);
    }
    let mut near: Vec<_> = cells.into_values().collect();
    near.sort_by(|a, b| {
        score(a)
            .total_cmp(&score(b))
            .then(a.triangle.cmp(&b.triangle))
    });
    record(
        cache,
        target.group.unwrap_or((u32::MAX, 0)),
        "eligible 1mm XY cells",
        near.len() as u64,
    );
    near.truncate(8);
    for (reason, count) in reasons {
        record(cache, target.group.unwrap_or((u32::MAX, 0)), reason, count);
    }
    near
}

fn worse(old: f64, new: f64, limit: f64) -> bool {
    new.is_nan() || (new > limit && new > old + 1e-7)
}

/// Every existing violation is compared numerically, not just by warning class.
/// Only improvement at the requested exposure can retain a load-only addition.
pub(super) fn replay_improves(
    before: &load::LoadDetail,
    after: &load::LoadDetail,
    target: (u32, u32),
) -> bool {
    replay_check(before, after, Some(target))
}
pub(super) fn replay_no_worse(before: &load::LoadDetail, after: &load::LoadDetail) -> bool {
    replay_check(before, after, None)
}
fn replay_check(
    before: &load::LoadDetail,
    after: &load::LoadDetail,
    target: Option<(u32, u32)>,
) -> bool {
    if before.groups.len() != after.groups.len() {
        return false;
    }
    let mut improvement = false;
    for (b, a) in before.groups.iter().zip(&after.groups) {
        if (b.component, b.layer) != (a.component, a.layer)
            || (!b.active.is_empty() && a.active.is_empty())
            || (b.equilibrium_assessed && !a.equilibrium_assessed)
            || worse(b.x_group, a.x_group, P5)
        {
            return false;
        }
        let targeted = Some((b.component, b.layer)) == target;
        improvement |= targeted && b.x_group > P5 && a.x_group + 1e-6 < b.x_group;
        if !b.equilibrium_assessed {
            for (old, new, limit) in [
                (b.outside_hull_mm, a.outside_hull_mm, 1e-6),
                (
                    b.force_residual,
                    a.force_residual,
                    load::lever::EQUILIBRIUM_LIMIT,
                ),
                (
                    b.moment_residual,
                    a.moment_residual,
                    load::lever::EQUILIBRIUM_LIMIT,
                ),
                (b.condition, a.condition, load::lever::CONDITION_LIMIT),
                (
                    b.position_sensitivity,
                    a.position_sensitivity,
                    load::lever::SENSITIVITY_LIMIT,
                ),
            ] {
                if worse(old, new, limit) {
                    return false;
                }
                improvement |= targeted && old > limit && new + 1e-6 < old;
            }
            improvement |= targeted && a.equilibrium_assessed;
        }
        if a.section_states.len() < b.section_states.len() {
            return false;
        }
        // Stable existing chains are a prefix. Check every section, not only
        // the maximum: a falling maximum can conceal a worsening old section.
        for (old, new) in b.section_states.iter().zip(&a.section_states) {
            if (old.contact, old.part) != (new.contact, new.part) {
                return false;
            }
            for (o, n, comparable) in [
                (
                    old.ratios.rigid_lever,
                    new.ratios.rigid_lever,
                    a.equilibrium_assessed,
                ),
                (old.ratios.equal_share, new.ratios.equal_share, true),
            ] {
                if comparable && worse(o, n, P5) {
                    return false;
                }
                improvement |= targeted && comparable && o > P5 && n + 1e-6 < o;
            }
        }
        // New paths cannot introduce a new overload hidden below an old maximum.
        if a.section_states[b.section_states.len()..].iter().any(|s| {
            s.ratios.equal_share > P5 + 1e-7
                || (a.equilibrium_assessed && s.ratios.rigid_lever > P5 + 1e-7)
        }) {
            return false;
        }
    }
    improvement || target.is_none()
}

/// First numerical blocker, with its actual replay location and values.
pub(super) fn replay_blocker(before: &load::LoadDetail, after: &load::LoadDetail) -> String {
    for (b, a) in before.groups.iter().zip(&after.groups) {
        let where_ = format!("C{} L{}", b.component, b.layer);
        if b.equilibrium_assessed && !a.equilibrium_assessed {
            return format!("{where_} equilibrium becomes unresolved");
        }
        for (name, old, new, limit) in [
            ("additive", b.x_group, a.x_group, P5),
            (
                "outside spread mm",
                b.outside_hull_mm,
                a.outside_hull_mm,
                1e-6,
            ),
            (
                "force residual",
                b.force_residual,
                a.force_residual,
                load::lever::EQUILIBRIUM_LIMIT,
            ),
            (
                "moment residual",
                b.moment_residual,
                a.moment_residual,
                load::lever::EQUILIBRIUM_LIMIT,
            ),
            (
                "condition",
                b.condition,
                a.condition,
                load::lever::CONDITION_LIMIT,
            ),
            (
                "position sensitivity",
                b.position_sensitivity,
                a.position_sensitivity,
                load::lever::SENSITIVITY_LIMIT,
            ),
        ] {
            if worse(old, new, limit) {
                return format!("{where_} {name} {old:.6} -> {new:.6}");
            }
        }
        for (old, new) in b.section_states.iter().zip(&a.section_states) {
            for (name, o, n, valid) in [
                (
                    "rigid",
                    old.ratios.rigid_lever,
                    new.ratios.rigid_lever,
                    a.equilibrium_assessed,
                ),
                (
                    "equal",
                    old.ratios.equal_share,
                    new.ratios.equal_share,
                    true,
                ),
            ] {
                if valid && worse(o, n, P5) {
                    return format!(
                        "{where_} contact {} part {} {name} {o:.6} -> {n:.6}",
                        old.contact, old.part
                    );
                }
            }
        }
        for new in a.section_states.iter().skip(b.section_states.len()) {
            if new.ratios.equal_share > P5 + 1e-7
                || (a.equilibrium_assessed && new.ratios.rigid_lever > P5 + 1e-7)
            {
                return format!(
                    "{where_} new contact {} part {} overloaded: equal {:.6}, rigid {:.6}",
                    new.contact, new.part, new.ratios.equal_share, new.ratios.rigid_lever
                );
            }
        }
    }
    "target did not improve or complete route unavailable".into()
}

pub(super) fn load_targets(
    detail: &load::LoadDetail,
    chosen: &[Chosen],
    capped: &BTreeSet<[u32; 3]>,
    grid: &plinth_geom::slice::LayerGrid,
    shift: f64,
) -> Vec<Target> {
    let severity = |g: &load::GroupLoad| {
        g.section_states.iter().fold(g.x_group, |m, s| {
            m.max(s.ratios.equal_share).max(if g.equilibrium_assessed {
                s.ratios.rigid_lever
            } else {
                0.0
            })
        })
    };
    let mut rows: Vec<_> = detail
        .groups
        .iter()
        .filter(|g| {
            g.x_group > P5
                || (!g.equilibrium_assessed && !g.active.is_empty())
                || g.section_states.iter().any(|s| {
                    s.ratios.equal_share > P5
                        || (g.equilibrium_assessed
                            && s.ratios.rigid_lever > P5
                            && capped.contains(&site_key(chosen[s.contact as usize].site.xyz)))
                })
        })
        .collect();
    rows.sort_by(|a, b| {
        // The actual worst exposure across ALL components. No island-first
        // starvation and no one-exposure-per-component truncation.
        severity(b)
            .total_cmp(&severity(a))
            .then(b.outside_hull_mm.total_cmp(&a.outside_hull_mm))
            .then(a.layer.cmp(&b.layer))
            .then(a.component.cmp(&b.component))
    });
    // Start with the worst exposure of each active-contact/assessment cohort,
    // not one row per component. Every exposure participates in this ranking.
    let mut cohorts = BTreeSet::new();
    rows.into_iter()
        .filter(|g| cohorts.insert((g.component, g.active.clone(), g.equilibrium_assessed)))
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
            group: Some((g.component, g.layer)),
        })
        .collect()
}
