//! **A footprint move never breaks a rule the finished field was held to**
//! (review-fixes, 2026-09-29).
//!
//! # The defect
//!
//! The footprint rule slides a seat whose contact disc hangs off its face
//! along the face, and every member joined to the moved axis follows it. It
//! ran AFTER the lean law, the slenderness lane and the drain keep-out had all
//! been satisfied, and accepted a move on contact-disc coverage alone: the
//! displacement is applied later, in the adapter, so nothing re-checked the
//! geometry the file actually gets. An outward-leaning rim arm with a 5 mm
//! drop and 7.5 mm of lateral travel is inside the lean law's ~1.57 lateral
//! per unit drop; a 0.95 mm move that adds lateral travel takes it to 1.69.
//!
//! # The rule
//!
//! Every proposed set of moves is judged on the DRAWN ends of every member it
//! displaces: lateral travel per unit drop within the lean law (or no worse
//! than before the move), the drawn chord within the tightest unbraced run's
//! limit (or no longer than before), and no new lamina inside the drain
//! keep-out. A move that breaks one is refused and the search goes on.
//!
//! # How it is graded
//!
//! On the production sweep and the production adapter: the arms the file gets
//! (`arms_of` of the finished plan) against the arms it would get with every
//! footprint move taken back (the same plan with each moved seat's verdict
//! reset), read as geometry — lateral over drop, chord length — with the lean
//! limit the report publishes and the run limits the production
//! `unbraced_runs` measures. Fails on zero moved seats.
//!
//! Sabotage: admit every on-face move unchecked (the pre-fix footprint rule)
//! and this goes red on the flipped Round 25.

use plinth_core::{m0_constants, LayerPlan};
use plinth_mesh::Mesh;
use plinth_support::SweepArm;
use plinth_sweep::{arms_of, unbraced_runs, Footprint, SlendernessLaw, SweepPlan, SweepPolicy};

const LIFT_MM: f64 = 5.0;
/// Float noise on a comparison the geometry designs for (an arm drawn
/// exactly at the lean law's limit), and nothing more.
const RELATIVE_SLACK: f64 = 1e-9;

fn layer_height_mm() -> f64 {
    *m0_constants()
        .expect("registry")
        .evidence()
        .layer_height_default_mm()
        .value()
}

fn law() -> SlendernessLaw {
    let constants = m0_constants().expect("the registry constructs");
    let placeholders = constants.placeholders();
    SlendernessLaw::new(
        placeholders.slenderness_limit_ratio().diagnostic().get(),
        placeholders.tip_radius_default_mm().diagnostic().get(),
        placeholders.tip_radius_max_legacy_mm().diagnostic().get(),
        placeholders.overhang_angle_deg().diagnostic().get()
            - 0.5
                * placeholders
                    .overhang_weight_transition_deg()
                    .diagnostic()
                    .get(),
    )
}

/// `Round 25` (the `make_recessed_disc` dimensions the production tip-trim
/// test uses), turned 180° about X: the stiffener ring down, the pose the
/// owner ruled correct, where the footprint rule moves rim seats.
fn round_25_flipped() -> Mesh {
    let mesh = plinth_mesh::make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96)
        .expect("the Round 25 fixture");
    let verts = mesh
        .verts()
        .iter()
        .map(|vertex| [vertex[0], -vertex[1], -vertex[2]])
        .collect();
    Mesh::new(verts, mesh.tris().to_vec()).expect("a rigid rotation preserves a valid mesh")
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a layer count is a finite positive quotient of model-scale lengths"
)]
fn place(mesh: &Mesh) -> SweepPlan {
    let mut min_z = f64::INFINITY;
    let mut max_z = f64::NEG_INFINITY;
    for vertex in mesh.verts() {
        min_z = min_z.min(vertex[2]);
        max_z = max_z.max(vertex[2]);
    }
    let plate_z = min_z - LIFT_MM;
    let height = layer_height_mm();
    let count = (((max_z - plate_z) / height).ceil() as usize).saturating_add(1);
    let layers = LayerPlan::new(plate_z, vec![height; count]).expect("a uniform layer plan");
    let policy = SweepPolicy::from_registry().expect("the registry supplies every sweep number");
    crate::harness::memo_run_sweep(mesh, &layers, &policy)
        .expect("the sweep emits and warns; it never refuses")
}

fn lean(arm: &SweepArm) -> f64 {
    let drop = arm.seat_mm[2] - arm.base_mm[2];
    let lateral = (arm.seat_mm[0] - arm.base_mm[0]).hypot(arm.seat_mm[1] - arm.base_mm[1]);
    if drop > 0.0 {
        lateral / drop
    } else if lateral > 0.0 {
        f64::INFINITY
    } else {
        0.0
    }
}

fn chord(arm: &SweepArm) -> f64 {
    let d: [f64; 3] = std::array::from_fn(|axis| arm.seat_mm[axis] - arm.base_mm[axis]);
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// What [`grade`] measured on one plan: members displaced by a footprint
/// move, and those past the lean law `(member, before, after)` or past a
/// run's limit `(member, growth, slack)`.
pub(crate) struct Graded {
    pub displaced: usize,
    pub max_lean: f64,
    pub over_lean: Vec<(usize, f64, f64)>,
    pub over_run: Vec<(usize, f64, f64)>,
}

/// Every member the footprint moves displace, graded as geometry on the arms
/// the file gets against the arms it would get with every move taken back.
pub(crate) fn grade(plan: &SweepPlan) -> Graded {
    let after = arms_of(plan);
    let mut unmoved = plan.clone();
    for strut in &mut unmoved.struts {
        if strut.seat_face.footprint == Footprint::MovedOntoFace {
            strut.seat_face.footprint = Footprint::OnFace;
        }
    }
    let before = arms_of(&unmoved);
    let height = plan.layer_height_mm;
    let max_lean = plan.report.lean_max_step_mm / height;
    let mut slack = vec![f64::INFINITY; plan.struts.len()];
    for run in unbraced_runs(&plan.grid, &plan.struts, height, &law()) {
        if run.length_mm > 0.0 {
            let entry = &mut slack[run.strut_id as usize];
            *entry = entry.min(run.limit_mm / run.length_mm);
        }
    }
    let mut displaced = 0;
    let mut over_lean = Vec::new();
    let mut over_run = Vec::new();
    for (index, (was, now)) in before.iter().zip(&after).enumerate() {
        // Bit identity: a member nothing moved is emitted unchanged.
        let bits = |point: [f64; 3]| point.map(f64::to_bits);
        if bits(was.seat_mm) == bits(now.seat_mm) && bits(was.base_mm) == bits(now.base_mm) {
            continue;
        }
        displaced += 1;
        let allowed = lean(was).max(max_lean);
        if lean(now) > allowed * (1.0 + RELATIVE_SLACK) {
            over_lean.push((index, lean(was), lean(now)));
        }
        let growth = chord(now) / chord(was);
        if growth > slack[index].max(1.0) * (1.0 + RELATIVE_SLACK) {
            over_run.push((index, growth, slack[index]));
        }
    }
    Graded {
        displaced,
        max_lean,
        over_lean,
        over_run,
    }
}

#[test]
fn every_member_a_footprint_move_displaces_still_holds_the_lean_law_and_its_runs() {
    let plan = place(&round_25_flipped());
    let footprint = &plan.report.seat_footprint;
    assert!(
        footprint.tips_moved_onto_face > 0,
        "no seat moved, so the move check is graded on nothing"
    );
    let Graded {
        displaced,
        max_lean,
        over_lean,
        over_run,
    } = grade(&plan);
    println!(
        "Round 25 flipped: {} seats moved, {displaced} members displaced, {} moves refused \
         (lean {}, run {}, drain {}; {} member checks); lean limit {max_lean:.4}; over the lean \
         law {over_lean:?}; over a run {over_run:?}",
        footprint.tips_moved_onto_face,
        footprint.moves_refused.len(),
        footprint.moves_refused_lean,
        footprint.moves_refused_slenderness,
        footprint.moves_refused_drain,
        footprint.members_checked,
    );
    assert!(displaced > 0, "no member was displaced");
    assert!(
        over_lean.is_empty(),
        "{} displaced members lean past the law and past where they stood: \
         (member, before, after) {over_lean:?}",
        over_lean.len()
    );
    assert!(
        over_run.is_empty(),
        "{} displaced members grew a run past its limit: (member, growth, slack) {over_run:?}",
        over_run.len()
    );
}
