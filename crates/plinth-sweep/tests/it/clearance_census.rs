//! **The census of every member's drawn solid reads the frustum the file
//! draws** (fix/census-drawn-frustum, 2026-10-04; rule R3 of
//! `plinth-evidence/slice-sweep-2026-09-02/field-2026-09-28/clearance-census-diagnosis.md`).
//!
//! The census (`member_clearance::model_clearance_census`) read each drawn
//! span at its WIDER end for the whole span, and the geom kernel subtracted
//! `max(r0, r1)` for the whole segment. A long tapered span (one chord from a
//! step to the root) was read at its root radius all the way up the shaft, and
//! 15 of the 37 members the v12 census named were that error and nothing else
//! (brace pair 5 and 11, Sphere 9, 23 and 26, Morel 73, 74, 77, 87, 88, 92,
//! 104, 108, 109 and 114). The census is a WARNING quantity: the rule changes
//! what it says, not what the engine places.
//!
//! The reading below is the test's own: every axis sample of every drawn span,
//! 0.01 mm apart, against the model triangles by point-to-triangle distance
//! and ray parity (`member_clearance::signed`), minus the radius the span draws
//! at that sample. It restates no production clearance code; the spans and the
//! shaft ends are the lowering's own ([`plinth_support::drawn_spans`],
//! [`plinth_support::SweepRootFlare::plate_shaft_of`]).

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use plinth_mesh::{make_brace_pair, Mesh};
use plinth_sweep::{arms_of, Landing, SweepPlan, SweepPolicy};

use super::member_clearance::signed;
use crate::harness::sweep_with;

type V = [f64; 3];

fn policy() -> SweepPolicy {
    SweepPolicy::from_registry().expect("the registry supplies every number the sweep reads")
}

/// One drawn member as the test reads it: its id, and its drawn span ends as
/// `(top point, bottom point, top radius, bottom radius)`.
struct Drawn {
    id: u32,
    spans: Vec<(V, V, f64, f64)>,
}

/// Every live member's drawn spans, from the finished plan.
fn drawn_members(plan: &SweepPlan) -> Vec<Drawn> {
    let arms = arms_of(plan);
    let flare = plinth_support::SweepRootFlare::registered(plan.layer_height_mm).ok();
    let mut out = Vec::new();
    for (strut, arm) in plan.struts.iter().zip(&arms) {
        if strut.k_bottom == usize::MAX || strut.k_top <= strut.k_bottom {
            continue;
        }
        let top = arm.seat_mm;
        let bottom = if strut.landing == Landing::Plate {
            match flare
                .as_ref()
                .and_then(|f| f.plate_shaft_of(arm, plan.plate_z_mm))
            {
                Some((_, bottom)) => bottom,
                None => continue,
            }
        } else {
            arm.base_mm
        };
        let rise = top[2] - bottom[2];
        if rise <= 0.0 {
            continue;
        }
        let root = plinth_support::drawn_root_radius_mm(&arm.radius_profile_mm, arm.tip_radius_mm);
        let at = |z: f64| -> V {
            let t = (top[2] - z) / rise;
            std::array::from_fn(|i| top[i] + t * (bottom[i] - top[i]))
        };
        let spans = plinth_support::drawn_spans(
            &arm.radius_profile_mm,
            arm.base_mm[2],
            arm.seat_mm[2],
            top[2],
            bottom[2],
            arm.tip_radius_mm,
            root,
        )
        .into_iter()
        .map(|s| {
            (
                at(s.top_z_mm),
                at(s.bottom_z_mm),
                s.top_radius_mm,
                s.bottom_radius_mm,
            )
        })
        .collect();
        out.push(Drawn {
            id: strut.id,
            spans,
        });
    }
    out
}

/// The drawn solid's smallest surface gap, sampled every 0.01 mm, over the
/// samples `keep` accepts (the test's way of leaving out a member's own zones).
fn drawn_gap_mm(mesh: &Mesh, member: &Drawn, keep: &dyn Fn(f64) -> bool) -> Option<(f64, V)> {
    let mut best: Option<(f64, V)> = None;
    let total: f64 = member
        .spans
        .iter()
        .map(|(a, b, _, _)| norm(sub(*b, *a)))
        .sum();
    let mut walked = 0.0;
    for &(a, b, ra, rb) in &member.spans {
        let length = norm(sub(b, a));
        let n = ((length / 0.01).ceil() as usize).max(2);
        for i in 0..=n {
            let f = i as f64 / n as f64;
            let along = walked + f * length;
            if !keep(along / total.max(1e-12)) {
                continue;
            }
            let p: V = std::array::from_fn(|k| a[k] + (b[k] - a[k]) * f);
            let gap = signed(mesh, p) - (ra + (rb - ra) * f);
            if best.is_none_or(|(now, _)| gap < now) {
                best = Some((gap, p));
            }
        }
        walked += length;
    }
    best
}

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn norm(a: V) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

/// How much of a member's top the test leaves out as its own seat zone: the
/// tip ball and the shaft within the gap of the seat plane. The brace pair's
/// seats are on flat underside faces, where the zone is `R + gap` at most
/// (tip radius under 0.8 mm), so 1.5 mm covers it. The tapered members this
/// test reads stand beside a wall far below their seats.
const SEAT_ZONE_SKIP_MM: f64 = 1.5;

/// The brace pair, at production placement, through the whole sweep.
///
/// RED before the fix: members 5 and 11, each drawn as a tapered chord, were
/// named by the census at 0.294 and 0.274 mm (their root radius read all the
/// way up) while the drawn file clears the model by 0.358 and 0.329 mm. The
/// two tapered members here are exactly those.
#[test]
fn the_brace_pair_census_matches_the_drawn_file() {
    let mesh = make_brace_pair().expect("the brace pair");
    let plan = sweep_with(&mesh, 0.0, &policy());
    let census = &plan.report.member_model_clearance;
    assert!(census.ran, "{census:?}");
    let gap = census.gap_mm;
    let named: Vec<u32> = census.within_gap.iter().map(|near| near.member).collect();

    let members = drawn_members(&plan);
    let tapered: Vec<&Drawn> = members
        .iter()
        .filter(|member| {
            member
                .spans
                .iter()
                .any(|span| (span.2 - span.3).abs() > 1e-9)
        })
        .collect();
    // Anti-vacuity and path hits: the brace pair draws tapered spans (the
    // thickening's one chord), and the rule is about exactly those.
    assert!(
        !tapered.is_empty(),
        "the brace pair draws no tapered span; the test would prove nothing"
    );
    assert!(
        census.rows_tapered >= tapered.len(),
        "the census read {} tapered rows for {} tapered members: {census:?}",
        census.rows_tapered,
        tapered.len()
    );
    println!(
        "tapered members: {:?}",
        tapered.iter().map(|m| m.id).collect::<Vec<_>>()
    );

    for member in tapered {
        let total: f64 = member.spans.iter().map(|s| norm(sub(s.1, s.0))).sum();
        let skip = SEAT_ZONE_SKIP_MM / total;
        let (drawn, at) = drawn_gap_mm(&mesh, member, &|along| along >= skip)
            .expect("a tapered member has samples below its seat zone");
        println!(
            "member {}: drawn file clears the model by {drawn:.4} mm at {at:?}",
            member.id
        );
        assert!(
            drawn >= gap - 0.01,
            "member {} is drawn {drawn} mm off the model: not a tapered clear member",
            member.id
        );
        assert!(
            !named.contains(&member.id),
            "member {} clears the model by {drawn:.3} mm as drawn, and the census names it within {gap} mm: {census:?}",
            member.id
        );
    }
}

// ---------------------------------------------------------------------------
// fix/drawn-solid-clearance (2026-10-04): R1 of clearance-census-diagnosis.md.
// ---------------------------------------------------------------------------

/// The finished field's census, on the stock overhang table at production
/// placement. The table's legs are 4 x 4 x 20 mm; the up-sweep's hex covering
/// puts a lattice seat 0.325 mm from a leg's inner face, and the post under it
/// (member 30) used to drop straight beside the leg's whole 20 mm and then be
/// thickened into it (the census read it at -0.445 mm; the file drew it 0.374
/// mm inside the leg).
///
/// RED before R1: member 30 named at -0.445 mm. After it the seat moved to a
/// cell covering the same demand, or kept and warned; no member is named at a
/// NEGATIVE gap (a solid inside the model). The two ruled exceptions
/// (`clearance` ruling 2026-10-04: lip-seat's "no nearer than it stood" gate
/// stays, so branch 79 stays a warning) may still stand within the final gap,
/// and the test lists what stands rather than hiding it.
#[test]
#[cfg_attr(not(feature = "slow-tests"), ignore = "slow: scripts/gate-full.sh runs it")]
fn no_member_enters_the_overhang_table() {
    let mesh = plinth_mesh::make_overhang_table().expect("the overhang table");
    let plan = sweep_with(&mesh, 0.0, &policy());
    let census = &plan.report.member_model_clearance;
    assert!(census.ran, "{census:?}");
    let counters = &plan.report.counters;
    // Path hits: the solid test ran, and it refused something on this model.
    assert!(
        counters.descent_solid_tested > 0,
        "the drawn-solid test never ran: {counters:?}"
    );
    assert!(
        counters.descent_solid_refused_at_final_gap > 0,
        "the drawn-solid test never refused a descent on the table: {counters:?}"
    );
    println!(
        "descent solid: tested {} refused-at-final-gap {} cleared-by-lean {} final-only {} uncleared {} lean-refused {} seats-moved {}",
        counters.descent_solid_tested,
        counters.descent_solid_refused_at_final_gap,
        counters.descent_solid_cleared_by_lean,
        counters.descent_solid_at_final_gap_only,
        counters.descent_solid_uncleared,
        counters.descent_solid_lean_candidates_refused,
        counters.descent_solid_seats_moved
    );
    println!("clearance re-route: {:?}", plan.report.clearance_reroute);
    for near in &census.within_gap {
        println!(
            "within the gap: member {} {:?} at {:.4} mm",
            near.member, near.landing, near.clearance_mm
        );
    }
    let inside: Vec<_> = census
        .within_gap
        .iter()
        .filter(|near| near.clearance_mm < 0.0)
        .collect();
    assert!(
        inside.is_empty(),
        "members whose drawn solid is inside the model: {inside:?}"
    );
}

/// THE SABOTAGE of the test above: the same table with the centreline-only
/// descent (`without_drawn_solid_descent`) puts a member inside the leg again.
/// A test that only asserted the clean census would pass on a census that
/// never reads the walls.
#[test]
#[cfg_attr(not(feature = "slow-tests"), ignore = "slow: scripts/gate-full.sh runs it")]
fn sabotage_without_the_drawn_solid_descent_a_member_enters_the_overhang_table() {
    let mesh = plinth_mesh::make_overhang_table().expect("the overhang table");
    // The re-route is off too: after the final sizing it would move a member
    // the retired descent put inside the leg, which is its job (the test above).
    let plan = sweep_with(
        &mesh,
        0.0,
        &policy().without_drawn_solid_descent().without_clearance_reroute(),
    );
    let census = &plan.report.member_model_clearance;
    assert!(census.ran, "{census:?}");
    assert_eq!(
        plan.report.counters.descent_solid_tested, 0,
        "the knob is off: no solid is read"
    );
    assert!(
        census.within_gap.iter().any(|near| near.clearance_mm < 0.0),
        "the retired descent should put a member inside the model: {:?}",
        census.within_gap
    );
}

/// **On the Morel base: no lander's solid is inside the model where it joins
/// its host** (R1; Morel 95 and 112 were branches on 76 and 77 whose solid
/// reached the model at the junction, at -0.069 and -0.055 mm). Local input,
/// like the other Morel tests, so `#[ignore]`d in the gate.
#[test]
#[ignore = "reads the local, uncommitted Morel base mini (PLINTH_MINIS_DIR)"]
fn a_lander_never_joins_inside_its_hosts_contact() {
    let path = super::morel_pose::morel_path();
    let raw = plinth_mesh::load_binary_stl(&path, plinth_mesh::WeldPolicy::None)
        .unwrap_or_else(|error| panic!("{}: {error:?}", path.display()));
    let repaired = plinth_mesh::repair_mesh(&raw).into_mesh();
    let mesh = plinth_decimate::run(&repaired, super::morel_pose::registered_decimation_budget())
        .expect("decimation must run")
        .into_decimated_model()
        .expect("scale sanity must pass")
        .into_parts()
        .0;
    let plan = sweep_with(&mesh, 0.0, &policy());
    let census = &plan.report.member_model_clearance;
    assert!(census.ran, "{census:?}");
    let counters = &plan.report.counters;
    println!(
        "Morel: descent solid tested {} refused-at-final-gap {} cleared-by-lean {} final-only {} uncleared {} seats-moved {}; {} members within the gap",
        counters.descent_solid_tested,
        counters.descent_solid_refused_at_final_gap,
        counters.descent_solid_cleared_by_lean,
        counters.descent_solid_at_final_gap_only,
        counters.descent_solid_uncleared,
        counters.descent_solid_seats_moved,
        census.within_gap.len()
    );
    for near in &census.within_gap {
        println!("within the gap: member {} {:?} at {:.4} mm", near.member, near.landing, near.clearance_mm);
    }
    let inside: Vec<_> = census
        .within_gap
        .iter()
        .filter(|near| matches!(near.landing, Landing::Strut { .. }) && near.clearance_mm < 0.0)
        .collect();
    assert!(inside.is_empty(), "landers whose solid is inside the model: {inside:?}");
}

/// **A post the sizing grew into the gap is re-routed** (R2.2 of
/// `clearance-census-diagnosis.md`, fix/drawn-solid-clearance, 2026-10-04).
/// The overhang table's post 26 is placed clear at the radius the up-sweep can
/// bound (0.05 mm), then the area law sizes it for the 55.6 mm² of four members
/// that land on it (r = 0.99 mm), and its drawn solid stands 0.228 mm from the
/// corner of a leg (the final gap is 0.3 mm). It carries three branches, one of
/// which carries two more: the re-route moves the whole subtree.
///
/// After the re-route the census names only what the lip-seat rule itself
/// re-landed (Craig's 2026-10-04 ruling keeps that rule's gate "no nearer than
/// it stood", so branch 79 stays a warning), and the counters witness that the
/// pass ran, named the post, moved it with the arms that rest on it, and asked
/// the moved solid for the final gap outright.
///
/// RED before the pass existed: member 26 named at 0.2279 mm.
#[test]
#[cfg_attr(not(feature = "slow-tests"), ignore = "slow: scripts/gate-full.sh runs it")]
fn a_post_grown_into_the_gap_by_the_sizing_is_re_routed_on_the_overhang_table() {
    let mesh = plinth_mesh::make_overhang_table().expect("the overhang table");
    let plan = sweep_with(&mesh, 0.0, &policy());
    let census = &plan.report.member_model_clearance;
    assert!(census.ran, "{census:?}");
    let reroute = &plan.report.clearance_reroute;
    // Path hits: the pass ran, named a post, moved it with its subtree, and
    // refused candidates by the clearance of the moved solid.
    assert!(reroute.enabled, "{reroute:?}");
    assert!(reroute.clearance_violators_first > 0, "no arm was named: {reroute:?}");
    assert!(reroute.arms_relanded > 0, "no arm was moved: {reroute:?}");
    assert!(reroute.children_moved_with_host > 0, "no subtree followed: {reroute:?}");
    assert!(reroute.candidates_refused_clearance > 0, "no candidate was refused for its solid: {reroute:?}");
    assert_eq!(reroute.clearance_violators_last, 0, "{reroute:?}");
    let ruled = &plan.report.lip_seat.relanded_ids;
    let unruled: Vec<_> = census
        .within_gap
        .iter()
        .filter(|near| !ruled.contains(&near.member))
        .collect();
    assert!(
        unruled.is_empty(),
        "members within the final gap that the lip-seat rule did not move: {unruled:?}"
    );
}

/// THE SABOTAGE of the test above: without the re-route the same table keeps
/// post 26 inside the gap, and the pass reports it never ran.
#[test]
#[cfg_attr(not(feature = "slow-tests"), ignore = "slow: scripts/gate-full.sh runs it")]
fn sabotage_without_the_clearance_reroute_the_post_stays_inside_the_gap() {
    let mesh = plinth_mesh::make_overhang_table().expect("the overhang table");
    let plan = sweep_with(&mesh, 0.0, &policy().without_clearance_reroute());
    let census = &plan.report.member_model_clearance;
    assert!(census.ran, "{census:?}");
    let reroute = &plan.report.clearance_reroute;
    assert!(!reroute.enabled && reroute.arms_relanded == 0, "{reroute:?}");
    let ruled = &plan.report.lip_seat.relanded_ids;
    assert!(
        census.within_gap.iter().any(|near| !ruled.contains(&near.member)),
        "the sizing grows post 26 into the gap, and nothing moves it: {:?}",
        census.within_gap
    );
}

/// **A RE-ROUTED FIELD LEAVES NO RUN OVER ITS LIMIT** (lane cylinder-bracing,
/// rule D, 2026-10-05; `cylinder-v15-regression.md`). The tie passes ran on the
/// field BEFORE the clearance re-route, so a run the move made (the Cylinder's
/// branch 169, re-landed on a new host: 8.53 mm against 4.85 mm) or a limit
/// the move lowered (196's re-size: 24.55 mm against a limit that fell to
/// 23.78 mm) had no rung left to hold it, and the census warned. The passes
/// now run again on the field the re-route left.
///
/// On the overhang table, where the re-route moves post 26 with its subtree:
/// the pass moved arms, and the finished field, re-measured by the public run
/// measure, has no run over its limit and no `unbraced_run_over_limit`
/// warning. THIS IS A GUARD, NOT A RED TEST: the table's moved arms never
/// leave a run over its limit (the re-tie finds none: `retie_remedies` stays
/// zero, measured), so it passes without the rule. The red witness of rule D
/// is the Cylinder in its production pose (the hollowed model with its drain),
/// 169 and 196, which the unit-sized fixtures here cannot reproduce; the lane
/// report records its run, before and after. The path hit that the rule ran on
/// the Cylinder is `retie_passes_after_reroute` / `retie_remedies_after_reroute`.
#[test]
#[cfg_attr(not(feature = "slow-tests"), ignore = "slow: scripts/gate-full.sh runs it")]
fn a_rerouted_field_leaves_no_run_over_its_limit() {
    let mesh = plinth_mesh::make_overhang_table().expect("the overhang table");
    let plan = sweep_with(&mesh, 0.0, &policy());
    let reroute = &plan.report.clearance_reroute;
    assert!(reroute.arms_relanded > 0, "anti-vacuity: the re-route moved no arm: {reroute:?}");
    let lane = &plan.report.slenderness_lane;
    assert!(
        lane.warnings.is_empty(),
        "unbraced_run_over_limit warnings after the re-route: {:?}",
        lane.warnings
    );
    let policy = policy();
    let law = plinth_sweep::SlendernessLaw::new(
        policy.slenderness_ratio,
        policy.tip_radius_min_mm,
        policy.tip_radius_max_mm,
        policy.overhang_centre_deg - 0.5 * policy.overhang_transition_deg,
    );
    let over: Vec<_> = plinth_sweep::unbraced_runs(&plan.grid, &plan.struts, plan.layer_height_mm, &law)
        .into_iter()
        .filter(|run| run.over_limit())
        .map(|run| (run.strut_id, run.k_lo, run.k_top, run.length_mm, run.limit_mm))
        .collect();
    assert!(over.is_empty(), "runs over their limit on the re-routed field: {over:?}");
    println!(
        "re-tie after the re-route: {} passes, {} remedies",
        lane.retie_passes_after_reroute, lane.retie_remedies_after_reroute
    );
}
