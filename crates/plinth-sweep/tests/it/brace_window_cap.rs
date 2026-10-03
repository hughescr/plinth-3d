//! **The brace window and the brace cap** (Craig ruling 2026-10-03, rules R2,
//! R7 + D5 and D9 of `field-2026-09-28/bracing-mechanics.md`, all interim
//! pending the slenderness coupon), graded on the Cylinder.
//!
//! * **R2.** A brace is never thicker than the thinner post it joins, so its
//!   reach is at most `S` times that post's radius, and the brace-junction
//!   raise (the post drawn at the brace's radius from the joint down to its
//!   root) is deleted. Craig saw that raise in ChituBox as "a fat post with a
//!   sudden thin top": 26 of the 45 steps the round-11 diagnosis measured
//!   (`v11-feedback-diagnosis.md` item 3), and 6 of the Cylinder's 6 steps
//!   left in round 12 sit within 0.065 mm of a brace end.
//! * **R7 + D5.** A cross is built only where it costs less resin than
//!   thickening the run; below that window the run is thickened.
//! * **D9.** Past a pair's two-post ceiling no band is added between that
//!   pair; the run is thickened into the window or braced to a wider frame.
//!
//! What is graded is read from the EMITTED primitives (the reader of
//! `emitted_profile_covers_ledger`) and the lane's own published counts;
//! nothing here re-implements the lane:
//!
//! | test | what it asserts |
//! |---|---|
//! | `no_brace_joint_draws_a_shoulder_on_the_cylinder` | at every brace joint (a diagonal's seat on its host, and its landing), the host's emitted section narrows across the joint by no more than the lean law's step per layer, and the brace is drawn no thicker than the host there; the lane's own census of braces thicker than a post is 0 |
//! | `sabotage_with_the_cap_off_braces_are_drawn_thicker_than_their_posts` | the drill: with R2 off (and the raise deleted) the same grading finds braces drawn thicker than their posts, and the census counts them |
//! | `the_window_thickens_some_runs_and_still_braces_long_ones` | path hits: crosses refused because thickening was cheaper, runs thickened for it, and crosses still built (long skinny posts get braced) |
//! | `sabotage_without_the_window_no_cross_is_refused_for_resin` | the drill: with the window off, nothing is refused for resin or for the ceiling |
//!
//! Exempt from the shoulder grading, by the rules' own terms: a joint within
//! two layers of an arm landing on the same host (a LOAD step, drawn as a
//! cone by `tapered_load_steps`), and a joint within two layers of either end
//! of its host.
//!
//! Red on develop's ladder (measured 2026-10-03 with the brace raise put back
//! and both new rules off, which is develop `a73e40a`'s behaviour): 738 brace
//! joints graded, 3 shoulders (posts 150, 154, 153: 0.218 -> 0.060 mm,
//! 0.322 -> 0.079 mm, 0.322 -> 0.078 mm across the joint) and 3 braces drawn
//! thicker than their post. Lane report: `lanes/brace-window-cap.md`.

#![allow(
    clippy::cast_precision_loss,
    reason = "layer indices of a 40 mm model are exact in f64"
)]

use std::sync::OnceLock;

use plinth_support::SupportSegmentKind;
use plinth_sweep::{Landing, StrutKind, SweepEmission, SweepPlan, SweepPolicy};

use crate::emitted_profile_covers_ledger::{drawn_radius_at, members, plan_of_with};
use crate::tapered_load_steps::cylinder;

fn policy() -> SweepPolicy {
    SweepPolicy::from_registry().expect("the registry supplies every sweep number")
}

/// The steepest narrowing per layer the lean law allows, from the registered
/// overhang band (the same step `tapered_load_steps` grades cones against).
fn step_mm(policy: &SweepPolicy, layer_height_mm: f64) -> f64 {
    let saturated = policy.overhang_centre_deg - 0.5 * policy.overhang_transition_deg;
    layer_height_mm / saturated.to_radians().tan()
}

fn production() -> &'static (SweepPlan, SweepEmission) {
    static PLAN: OnceLock<(SweepPlan, SweepEmission)> = OnceLock::new();
    PLAN.get_or_init(|| plan_of_with(&cylinder(), &policy(), |_| {}))
}

#[derive(Debug, Default)]
struct Joints {
    /// Brace joints graded, and those exempt (near a load step or an end).
    graded: usize,
    exempt: usize,
    /// (host id, brace id, layer, host mm just below, host mm just above):
    /// joints where the host narrows across the joint by more than the lean
    /// law allows over those two layers.
    shoulders: Vec<(u32, u32, usize, f64, f64)>,
    /// (brace id, host id, layer, brace mm, host mm at the joint): braces
    /// drawn thicker than the host they join.
    thicker: Vec<(u32, u32, usize, f64, f64)>,
}

fn grade(plan: &SweepPlan, emission: &SweepEmission, step: f64) -> Joints {
    let groups = members(emission);
    assert_eq!(emission.arms_emitted(), emission.arms_offered, "every arm emitted");
    assert_eq!(groups.len(), plan.struts.len(), "one emitted member per strut");
    let height = plan.layer_height_mm;
    let radius_at = |host: usize, layer: usize| {
        drawn_radius_at(&groups[host], plan.plate_z_mm + (layer as f64 + 0.5) * height)
    };
    // Layers where a support arm lands on each member: its load steps.
    let mut landings: Vec<Vec<usize>> = vec![Vec::new(); plan.struts.len()];
    for strut in plan.struts.iter().filter(|strut| strut.kind == StrutKind::Arm) {
        if let Landing::Strut { id, layer } = strut.landing {
            if let Some(list) = landings.get_mut(id as usize) {
                list.push(layer);
            }
        }
    }
    let mut out = Joints::default();
    for (index, brace) in plan.struts.iter().enumerate() {
        if brace.kind != StrutKind::Brace {
            continue;
        }
        // The brace's drawn radius: its widest emitted section.
        let brace_mm = groups[index]
            .iter()
            .filter(|primitive| {
                !matches!(
                    primitive.segment_kind,
                    SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere
                )
            })
            .flat_map(|primitive| [primitive.start_radius_mm.get(), primitive.end_radius_mm.get()])
            .fold(0.0_f64, f64::max);
        let base = match brace.landing {
            Landing::Strut { id, .. } => Some((id, brace.k_bottom)),
            Landing::Plate | Landing::Model { .. } => None,
        };
        let seat = brace.seat_on_strut.map(|id| (id, brace.k_top));
        for (host_id, layer) in [seat, base].into_iter().flatten() {
            let host_index = host_id as usize;
            let Some(host) = plan.struts.get(host_index) else {
                continue;
            };
            let near_end = layer < host.k_bottom + 2 || layer + 2 >= host.k_top;
            let near_load = landings[host_index]
                .iter()
                .any(|&landing| landing.abs_diff(layer) <= 2);
            if near_end || near_load {
                out.exempt += 1;
                continue;
            }
            let (Some(below), Some(above), Some(at)) = (
                radius_at(host_index, layer - 1),
                radius_at(host_index, layer + 1),
                radius_at(host_index, layer),
            ) else {
                out.exempt += 1;
                continue;
            };
            out.graded += 1;
            if below - above > 2.0 * step * (1.0 + 1.0e-9) + 1.0e-12 {
                out.shoulders.push((host.id, brace.id, layer, below, above));
            }
            if brace_mm > at * (1.0 + 1.0e-9) + 1.0e-12 {
                out.thicker.push((brace.id, host.id, layer, brace_mm, at));
            }
        }
    }
    out
}

fn report(name: &str, plan: &SweepPlan, joints: &Joints) {
    let lane = &plan.report.slenderness_lane;
    println!(
        "{name}: {} brace joints graded ({} exempt); {} shoulders: {:?}; {} braces thicker \
         than a post: {:?}; lane: {} braces, {} members thickened, census of braces thicker \
         than a post {}, R2 band refusals {}, crosses refused for resin {} (then thickened {}, \
         highest L/limit {:.3}, lowest braced {:.3}), past the ceiling {} (into the window \
         {}), shared {}",
        joints.graded,
        joints.exempt,
        joints.shoulders.len(),
        joints.shoulders,
        joints.thicker.len(),
        joints.thicker.iter().take(10).collect::<Vec<_>>(),
        lane.braces_added,
        lane.members_thickened,
        lane.braces_thicker_than_a_post,
        lane.brace_band_refusals.thicker_than_post,
        lane.crosses_refused_thicken_cheaper,
        lane.thickened_where_cheaper_than_bracing,
        lane.window_highest_thickened_ratio,
        lane.window_lowest_braced_ratio,
        lane.crosses_refused_past_pair_ceiling,
        lane.members_thickened_into_the_window,
        lane.crosses_shared_by_both_posts,
    );
}

/// R2 on the emitted Cylinder: no brace joint draws a shoulder, and no brace
/// is drawn thicker than the post it joins.
#[test]
fn no_brace_joint_draws_a_shoulder_on_the_cylinder() {
    let (plan, emission) = production();
    let joints = grade(plan, emission, step_mm(&policy(), plan.layer_height_mm));
    report("Cylinder", plan, &joints);
    assert!(joints.graded > 0, "anti-vacuity: no brace joint was graded");
    assert!(
        joints.shoulders.is_empty(),
        "{} brace joints draw a fat post with a thin top (host, brace, layer, below mm, above \
         mm): {:?}",
        joints.shoulders.len(),
        joints.shoulders
    );
    assert!(
        joints.thicker.is_empty(),
        "{} braces are drawn thicker than the post they join (brace, host, layer, brace mm, \
         host mm): {:?}",
        joints.thicker.len(),
        joints.thicker
    );
    assert_eq!(
        plan.report.slenderness_lane.braces_thicker_than_a_post, 0,
        "the lane's own census counts braces thicker than a post"
    );
}

/// The drill: with R2 off, and the junction raise deleted, braces come back
/// thicker than their posts, and both the grading and the lane's census see
/// them.
#[test]
fn sabotage_with_the_cap_off_braces_are_drawn_thicker_than_their_posts() {
    let sabotaged = policy().without_brace_cap();
    let (plan, emission) = plan_of_with(&cylinder(), &sabotaged, |_| {});
    let joints = grade(&plan, &emission, step_mm(&sabotaged, plan.layer_height_mm));
    report("Cylinder, R2 off", &plan, &joints);
    assert_eq!(plan.report.slenderness_lane.brace_band_refusals.thicker_than_post, 0);
    assert!(
        !joints.thicker.is_empty(),
        "with R2 off the Cylinder must draw braces thicker than their posts again"
    );
    assert!(
        plan.report.slenderness_lane.braces_thicker_than_a_post > 0,
        "the lane's census must count them"
    );
}

/// R7 + D5 path hits on the Cylinder: the window refuses some crosses for
/// resin and thickens those runs, and still braces where bracing is cheaper.
#[test]
fn the_window_thickens_some_runs_and_still_braces_long_ones() {
    let (plan, emission) = production();
    let joints = grade(plan, emission, step_mm(&policy(), plan.layer_height_mm));
    report("Cylinder", plan, &joints);
    let lane = &plan.report.slenderness_lane;
    assert!(
        lane.crosses_refused_thicken_cheaper > 0,
        "path hit: no cross was refused because thickening was cheaper"
    );
    assert!(
        lane.thickened_where_cheaper_than_bracing > 0,
        "path hit: no run was thickened after its cross was refused for resin"
    );
    assert!(
        lane.braces_added > 0,
        "the window must still brace where bracing is cheaper"
    );
    assert!(
        lane.window_lowest_braced_ratio > 1.0 && lane.window_highest_thickened_ratio > 1.0,
        "both window readings are over-limit runs ({:.3}, {:.3})",
        lane.window_lowest_braced_ratio,
        lane.window_highest_thickened_ratio
    );
}

/// The drill: with the window off, no cross is refused for resin or for the
/// two-post ceiling, and nothing is thickened into a window.
#[test]
fn sabotage_without_the_window_no_cross_is_refused_for_resin() {
    let sabotaged = policy().without_brace_window();
    let (plan, emission) = plan_of_with(&cylinder(), &sabotaged, |_| {});
    let joints = grade(&plan, &emission, step_mm(&sabotaged, plan.layer_height_mm));
    report("Cylinder, window off", &plan, &joints);
    let lane = &plan.report.slenderness_lane;
    assert_eq!(lane.crosses_refused_thicken_cheaper, 0);
    assert_eq!(lane.thickened_where_cheaper_than_bracing, 0);
    assert_eq!(lane.crosses_refused_past_pair_ceiling, 0);
    assert_eq!(lane.members_thickened_into_the_window, 0);
    let (production, _) = production();
    println!(
        "Cylinder braces: window on {}, off {}; members thickened: on {}, off {}",
        production.report.slenderness_lane.braces_added,
        lane.braces_added,
        production.report.slenderness_lane.members_thickened,
        lane.members_thickened
    );
}
