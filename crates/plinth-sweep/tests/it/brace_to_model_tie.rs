//! **Ties to the model** (Craig ruling 2026-10-03, decision D8 of
//! `field-2026-09-28/bracing-mechanics.md`, revision 2): where a tall post
//! cannot be braced any other way — every `|X|` to another post passes
//! through the model, a mast would be thicker than the post it braces,
//! thickening runs past the maximum radius — a member TIES the post to the
//! model's own nearby surface. Craig's terms: the tie lands on the model
//! with a rounded tip exactly like any other support-to-model contact (the
//! same seat law, contact sizing, trim and clearance rules), it counts as a
//! contact scar, and the support math accounts for it (sized by the area law
//! and the slenderness rule, in per-event routing, in the clearance census).
//! It is the last rung before a warning.
//!
//! Graded on the two fixtures that motivated it, at the production policy
//! and their authored pose (the CLI pins both to identity):
//!
//! | test | what it asserts |
//! |---|---|
//! | `the_lonely_peak_posts_are_tied_and_none_warns` | no `unbraced_run_over_limit` warning, and every run of every member inside its limit on the finished field (re-measured by the public run measure); model ties built |
//! | `the_brace_pair_posts_are_tied_and_none_warns` | the same on the brace pair |
//! | `every_tie_ends_in_a_seated_tip_on_the_model` | on both: every tie lands on a post, is lowered with a seated tip (a contact disc from the lowering's own seat derivation, on a measured face), and that face is the model's surface within its ball of the seat; one contact sphere per seated member in the emitted file |
//! | `no_tie_is_within_the_final_gap_outside_its_seat_zone` | on both: the finished-field census measures every tie and lists none within 0.3 mm |
//! | `a_tie_is_a_scar_and_is_sized_like_a_member` | on both: the published tip scar includes every tie's contact; each tie is at least its own slenderness radius and no thicker than its post at the joint (R2) |
//! | `sabotage_without_model_ties_the_runs_warn` | the drill: with the rung off, the lonely peak warns on 3 runs and the brace pair on at least 1, and no tie is built |
//!
//! Red on develop `c8b1ede` (the same as this lane's sabotage policy, which
//! the drill pins): lonely peak 3 unbraced warnings (arms 1, 2, 4), brace
//! pair 3 (arms 2, 5, 9). Lane report: `lanes/brace-to-model-tie.md`.

use std::sync::OnceLock;

use plinth_mesh::{make_brace_pair, make_lonely_peak, Mesh};
use plinth_support::SupportSegmentKind;
use plinth_sweep::{
    arms_of, to_primitives, unbraced_runs, Landing, SlendernessLaw, StrutKind, SweepPlan,
    SweepPolicy, SweepStrut,
};

use crate::harness::sweep_with;

fn policy() -> SweepPolicy {
    SweepPolicy::from_registry().expect("the registry supplies every sweep number")
}

fn lonely_peak() -> Mesh {
    make_lonely_peak().expect("the lonely peak fixture")
}

fn brace_pair() -> Mesh {
    make_brace_pair().expect("the brace pair fixture")
}

fn peak_plan() -> &'static SweepPlan {
    static PLAN: OnceLock<SweepPlan> = OnceLock::new();
    PLAN.get_or_init(|| sweep_with(&lonely_peak(), 0.0, &policy()))
}

fn pair_plan() -> &'static SweepPlan {
    static PLAN: OnceLock<SweepPlan> = OnceLock::new();
    PLAN.get_or_init(|| sweep_with(&brace_pair(), 0.0, &policy()))
}

fn ties(plan: &SweepPlan) -> Vec<&SweepStrut> {
    plan.struts
        .iter()
        .filter(|strut| strut.kind == StrutKind::Tie && strut.k_bottom != usize::MAX)
        .collect()
}

/// The slenderness law in force, from the policy (not from the crate's
/// working values).
fn law() -> SlendernessLaw {
    let policy = policy();
    SlendernessLaw::new(
        policy.slenderness_ratio,
        policy.tip_radius_min_mm,
        policy.tip_radius_max_mm,
        policy.overhang_centre_deg - 0.5 * policy.overhang_transition_deg,
    )
}

/// The finished field's runs over their limit, re-measured from the strut
/// set by the public run measure.
fn runs_over(plan: &SweepPlan) -> Vec<(u32, usize, usize, f64, f64)> {
    unbraced_runs(&plan.grid, &plan.struts, plan.layer_height_mm, &law())
        .into_iter()
        .filter(|run| run.over_limit())
        .map(|run| (run.strut_id, run.k_lo, run.k_top, run.length_mm, run.limit_mm))
        .collect()
}

/// The rung's own tally, for every failure message.
fn tally(plan: &SweepPlan) -> String {
    let lane = &plan.report.slenderness_lane;
    format!(
        "ties built {} (ids {:?}), sets {}; runs tried {}, refused: no surface {}, no candidate \
         {}, frame {}; candidates {} (off shaft {}, thicker than post {}, path drain {} model {} \
         member {}, solid model {}, disc off face {}, solid member {})",
        lane.model_ties_built,
        lane.model_tie_ids,
        lane.model_tie_sets,
        lane.model_tie_runs_tried,
        lane.model_tie_refused_no_surface,
        lane.model_tie_refused_no_candidate,
        lane.model_tie_refused_frame,
        lane.model_tie_candidates,
        lane.model_tie_refused_off_shaft,
        lane.model_tie_refused_thicker_than_post,
        lane.model_tie_refused_path_drain,
        lane.model_tie_refused_path_model,
        lane.model_tie_refused_path_member,
        lane.model_tie_refused_solid_model,
        lane.model_tie_refused_disc_off_face,
        lane.model_tie_refused_solid_member,
    )
}

fn assert_tied_and_quiet(plan: &SweepPlan, name: &str) {
    let lane = &plan.report.slenderness_lane;
    let built = ties(plan);
    eprintln!("{name}: {}", tally(plan));
    assert!(
        !built.is_empty() && lane.model_ties_built == built.len(),
        "{name}: {} tie members in the field; {}",
        built.len(),
        tally(plan)
    );
    assert!(
        lane.warnings.is_empty(),
        "{name}: unbraced_run_over_limit warnings remain: {:?}",
        lane.warnings
    );
    let over = runs_over(plan);
    assert!(over.is_empty(), "{name}: runs over their limit on the finished field: {over:?}");
    // The frame the sets were counted through held every built set.
    assert!(
        lane.model_tie_worst_sway_px > 0.0 && lane.model_tie_worst_sway_px <= 1.0 + 1.0e-9,
        "{name}: worst tied-post sway {} px",
        lane.model_tie_worst_sway_px
    );
    eprintln!(
        "{name}: {} ties in {} sets ({} beyond the fixed-point count, {} thickened for the frame), \
         worst tied sway {:.3} px; ties {:?}",
        lane.model_ties_built,
        lane.model_tie_sets,
        lane.model_tie_sets_beyond_fixed_point,
        lane.model_ties_thickened_for_the_frame,
        lane.model_tie_worst_sway_px,
        built
            .iter()
            .map(|tie| (tie.id, tie.landing, tie.k_top, tie.tip_radius_mm()))
            .collect::<Vec<_>>(),
    );
}

#[test]
fn the_lonely_peak_posts_are_tied_and_none_warns() {
    assert_tied_and_quiet(peak_plan(), "lonely peak");
}

#[test]
fn the_brace_pair_posts_are_tied_and_none_warns() {
    assert_tied_and_quiet(pair_plan(), "brace pair");
}

#[test]
fn every_tie_ends_in_a_seated_tip_on_the_model() {
    for (name, plan, mesh) in [
        ("lonely peak", peak_plan(), lonely_peak()),
        ("brace pair", pair_plan(), brace_pair()),
    ] {
        let triangles: Vec<[[f64; 3]; 3]> = mesh
            .tris()
            .iter()
            .map(|t| [mesh.verts()[t[0] as usize], mesh.verts()[t[1] as usize], mesh.verts()[t[2] as usize]])
            .collect();
        // How far a point is from the model's surface: the nearest point of
        // any face (the canonical closest point, never a private copy).
        let off_surface = |p: [f64; 3]| {
            triangles
                .iter()
                .map(|&[a, b, c]| geom_kernel::vec3::dist(p, geom_kernel::closest_on_tri(p, a, b, c)))
                .fold(f64::INFINITY, f64::min)
        };
        let arms = arms_of(plan);
        let layer_plan = plinth_core::LayerPlan::new(
            plan.plate_z_mm,
            vec![plan.layer_height_mm; plan.layers.max(1)],
        )
        .expect("a uniform layer plan");
        let discs = plinth_support::tip_contact_discs(&arms, &layer_plan).expect("discs derive");
        let built = ties(plan);
        assert!(!built.is_empty(), "{name}: no tie to grade");
        for tie in &built {
            let index = tie.id as usize;
            let Landing::Strut { id: post, layer } = tie.landing else {
                panic!("{name}: tie {} does not land on a post: {:?}", tie.id, tie.landing);
            };
            let host = &plan.struts[post as usize];
            assert!(
                matches!(host.kind, StrutKind::Arm | StrutKind::Mast)
                    && host.k_bottom <= layer
                    && layer < host.k_top
                    && tie.k_top > tie.k_bottom,
                "{name}: tie {} lands on member {post} ({:?}, layers {}..{}) at layer {layer}",
                tie.id,
                host.kind,
                host.k_bottom,
                host.k_top,
            );
            assert!(arms[index].seats_into_model, "{name}: tie {} is lowered without a tip", tie.id);
            let face = tie.seat_face.point_mm.unwrap_or_else(|| {
                panic!("{name}: tie {} carries no measured face", tie.id);
            });
            let disc = discs[index].unwrap_or_else(|| {
                panic!("{name}: tie {} has no contact disc (no seated tip)", tie.id);
            });
            assert!(
                disc.radius_mm > 0.0
                    && geom_kernel::vec3::dist(face, disc.centre_mm) <= disc.ball_radius_mm + 1.0e-9,
                "{name}: tie {}'s contact disc (radius {:.4} mm at {:?}) is not on its face at {:?}",
                tie.id,
                disc.radius_mm,
                disc.centre_mm,
                face,
            );
            // THE WHOLE DISC ON THE MODEL: its centre and 32 points of its
            // rim lie on the model's surface (a disc that hangs off a
            // column's corner has rim points in the air beside it).
            let normal = geom_kernel::vec3::normalize(disc.outward_normal).expect("a unit normal");
            let helper = if normal[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
            let u = geom_kernel::vec3::normalize(geom_kernel::vec3::cross(normal, helper)).unwrap();
            let v = geom_kernel::vec3::cross(normal, u);
            let worst = (0..32)
                .map(|step| {
                    let (s, c) = (core::f64::consts::TAU * f64::from(step) / 32.0).sin_cos();
                    let rim: [f64; 3] = std::array::from_fn(|k| {
                        disc.centre_mm[k] + disc.radius_mm * (c * u[k] + s * v[k])
                    });
                    off_surface(rim)
                })
                .fold(off_surface(disc.centre_mm), f64::max);
            assert!(
                worst <= 1.0e-6,
                "{name}: tie {}'s contact disc (radius {:.4} mm at {:?}) stands {worst:.4} mm off \
                 the model's surface at its rim",
                tie.id,
                disc.radius_mm,
                disc.centre_mm,
            );
        }
        // One contact sphere per member seated on the model, ties included,
        // in the emitted file.
        let frame = plinth_support::BuildFrame::new(
            plinth_core::Finite::new(plan.plate_z_mm).expect("a finite plate"),
            plinth_core::PositiveFinite::new(crate::harness::LIFT_MM).expect("a positive lift"),
        );
        let emission = to_primitives(plan, frame, &layer_plan, 4_000_000, 1 << 30)
            .expect("the lowering refuses only about the request");
        let spheres = emission
            .primitives
            .iter()
            .filter(|primitive| primitive.segment_kind == SupportSegmentKind::ContactSphere)
            .count();
        let seated = plan
            .struts
            .iter()
            .filter(|strut| strut.kind.seats_on_model() && strut.k_bottom != usize::MAX)
            .count();
        assert_eq!(spheres, seated, "{name}: contact spheres against seated members");
    }
}

#[test]
fn no_tie_is_within_the_final_gap_outside_its_seat_zone() {
    for (name, plan) in [("lonely peak", peak_plan()), ("brace pair", pair_plan())] {
        let census = &plan.report.member_model_clearance;
        assert!(census.ran, "{name}: the clearance census did not run");
        let near: Vec<_> = census
            .within_gap
            .iter()
            .filter(|row| row.kind == StrutKind::Tie)
            .collect();
        assert!(near.is_empty(), "{name}: ties within the final gap: {near:?}");
        assert!(!ties(plan).is_empty(), "{name}: no tie measured");
    }
}

#[test]
fn a_tie_is_a_scar_and_is_sized_like_a_member() {
    let law = law();
    for (name, plan) in [("lonely peak", peak_plan()), ("brace pair", pair_plan())] {
        let built = ties(plan);
        let tie_scar: f64 = built
            .iter()
            .map(|tie| core::f64::consts::PI * tie.tip_radius_mm().powi(2))
            .sum();
        let arm_scar: f64 = plan
            .struts
            .iter()
            .filter(|strut| strut.kind == StrutKind::Arm)
            .map(|strut| core::f64::consts::PI * strut.tip_radius_mm().powi(2))
            .sum();
        let published = plan.report.total_scar_mm2;
        assert!(
            tie_scar > 0.0 && (published - (arm_scar + tie_scar)).abs() <= 1.0e-9 * published,
            "{name}: published scar {published} against arms {arm_scar} + ties {tie_scar}"
        );
        for tie in &built {
            let (top, base) = (tie.cell, tie.base_cell());
            let grid = &plan.grid;
            let lateral = (grid.center_x_mm(grid.col_of(top)) - grid.center_x_mm(grid.col_of(base)))
                .hypot(grid.center_y_mm(grid.row_of(top)) - grid.center_y_mm(grid.row_of(base)));
            let length = ((tie.k_top - tie.k_bottom) as f64 * plan.layer_height_mm).hypot(lateral);
            let own = law.brace_radius_mm(length);
            let thinnest = tie.radius_mm.iter().copied().fold(f64::INFINITY, f64::min);
            let Landing::Strut { id: post, layer } = tie.landing else {
                unreachable!("graded above");
            };
            let host = &plan.struts[post as usize];
            let at_joint = host.radius_mm[layer - host.k_bottom].max(host.min_radius_mm);
            assert!(
                thinnest >= own * (1.0 - 1.0e-9),
                "{name}: tie {} drawn at {thinnest:.4} mm under its own slenderness radius {own:.4}",
                tie.id
            );
            assert!(
                tie.min_radius_mm <= at_joint * (1.0 + 1.0e-9),
                "{name}: tie {} sized {:.4} mm, thicker than its post {post} ({at_joint:.4} mm) at \
                 the joint (R2)",
                tie.id,
                tie.min_radius_mm,
            );
        }
    }
}

/// The routing hands a tie no peel (its seat is on a wall, under no face the
/// routing loads), so the field's load still closes: injected = plate
/// reaction + uncarried + absorbed, and no tie carries load. Red when a tie
/// was given an arm's one-cell floor: a gap of −0.0075 mm² on the brace pair
/// at its path-fixture seating (three ties; `logs/fail-1.log`).
#[test]
fn a_tie_adds_no_load_and_the_field_still_closes() {
    for (name, plan) in [("lonely peak", peak_plan()), ("brace pair", pair_plan())] {
        let check = &plan.report.landing_check;
        let gap = check.load_gap_mm2 - check.absorbed_mm2;
        assert!(gap.abs() <= 1.0e-9, "{name}: load gap {gap} mm2 with ties in the field");
        for tie in ties(plan) {
            let most = tie.carried_mm2.iter().copied().fold(0.0_f64, f64::max);
            assert!(most == 0.0, "{name}: tie {} carries {most} mm2", tie.id);
        }
    }
}

#[test]
fn sabotage_without_model_ties_the_runs_warn() {
    let off = policy().without_model_ties();
    let peak = sweep_with(&lonely_peak(), 0.0, &off);
    let pair = sweep_with(&brace_pair(), 0.0, &off);
    for (name, plan, at_least) in [("lonely peak", &peak, 3), ("brace pair", &pair, 1)] {
        let lane = &plan.report.slenderness_lane;
        assert!(ties(plan).is_empty() && lane.model_ties_built == 0, "{name}: ties built with the rung off");
        assert!(
            lane.warnings.len() >= at_least,
            "{name}: with the rung off, {} unbraced warnings ({:?}), expected at least {at_least}",
            lane.warnings.len(),
            lane.warnings,
        );
        eprintln!(
            "{name} (rung off): {} unbraced warnings {:?}",
            lane.warnings.len(),
            lane.warnings.iter().map(|w| w.strut_id).collect::<Vec<_>>()
        );
    }
}
