//! **A support may land ANYWHERE on another support's shaft** — the owner's
//! ruling of 2026-09-05, graded on a real field rather than on a fixture built
//! to agree with it.
//!
//! The unit drills in `up_sweep::descent_tests` say what the descent CHOOSES.
//! This says what the finished plan is: that the junctions really do land
//! inside their parents, that force crosses each one under the registered area
//! law, and that turning the rule off with
//! [`SweepPolicy::without_shaft_landings`] takes every mid-shaft junction away
//! on the SAME fixture. Without that second run a green test here would mean
//! "the plan has junctions", which was already true of the top-only rule.

mod harness;

use harness::{sweep_with, x_allowed};
use plinth_core::m0_constants;
use plinth_mesh::make_recessed_disc;
use plinth_sweep::{Landing, SweepPlan, SweepPolicy, SweepStrut};

/// The Round 25 profile the disc oracle runs, in its own pose. The corpus
/// prints it upside down, and its rim arms sit at many different layers — which
/// is the situation shaft landings exist for, because a neighbour's TOP is then
/// a few layers below a seat that is a whole hex spacing away.
fn round_25() -> plinth_mesh::Mesh {
    make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96).expect("the Round 25 profile")
}

/// Junctions the top-only rule could not have made: the landing layer is below
/// the parent's own top, so it is somewhere on the parent's SHAFT. Paired with
/// the parent, and with the layer.
fn mid_shaft_junctions(plan: &SweepPlan) -> Vec<(&SweepStrut, &SweepStrut, usize)> {
    plan.struts
        .iter()
        // SUPPORT ARMS ONLY (2026-09-06): an `|X|` diagonal lands on a shaft by
        // CONSTRUCTION, so counting one here would report the slenderness lane
        // as a descent decision and would survive the sabotage knob below.
        .filter(|child| child.kind == plinth_sweep::StrutKind::Arm)
        .filter_map(|child| {
            let Landing::Strut { id, layer } = child.landing else {
                return None;
            };
            let parent = plan.struts.get(id as usize)?;
            (parent.k_bottom <= layer && layer < parent.k_top).then_some((child, parent, layer))
        })
        .collect()
}

/// `r = sqrt(ℓ / (X · π))` inside the registered clamps — the area law, read
/// from the registry here rather than from the crate under test, so a change to
/// the sizing cannot agree with itself.
fn area_law_radius_mm(carried_mm2: f64) -> f64 {
    let constants = m0_constants().expect("the registry constructs");
    let placeholders = constants.placeholders();
    let minimum = placeholders.tip_radius_default_mm().diagnostic().get();
    let maximum = placeholders.tip_radius_max_legacy_mm().diagnostic().get();
    (carried_mm2 / (x_allowed() * core::f64::consts::PI))
        .sqrt()
        .clamp(minimum, maximum)
}

#[test]
fn a_field_lands_arms_inside_other_arms_shafts_and_the_knob_takes_every_one_away() {
    let mesh = round_25();
    let with = sweep_with(&mesh, 0.0, &SweepPolicy::from_registry().expect("registry"));
    let without = sweep_with(
        &mesh,
        0.0,
        &SweepPolicy::from_registry()
            .expect("registry")
            .without_shaft_landings(),
    );

    let junctions = mid_shaft_junctions(&with);
    assert!(
        !junctions.is_empty(),
        "shaft landings must actually fire on this fixture; the report says {} mid-shaft",
        with.report.junctions_mid_shaft
    );
    assert_eq!(
        with.report.junctions_mid_shaft,
        junctions.len(),
        "the published census must equal what the finished field holds"
    );
    assert!(
        junctions
            .iter()
            .any(|(_, parent, layer)| parent.k_bottom < *layer),
        "and at least one junction must be STRICTLY inside a parent, above its own base"
    );
    assert_eq!(
        mid_shaft_junctions(&without).len(),
        0,
        "SABOTAGE: with shaft landings off, no junction may be inside a parent"
    );
    assert_eq!(without.report.junctions_mid_shaft, 0);
    assert!(
        with.report.landings.plate < without.report.landings.plate,
        "and the arms that merged must have come OFF the plate: {} against {}",
        with.report.landings.plate,
        without.report.landings.plate
    );
}

/// **FORCE CROSSES A MID-SHAFT JUNCTION, AND IT CROSSES IT AT THE JUNCTION.**
///
/// A child that lands part-way down a trunk splits that trunk in effect: the
/// area law must size the parent from the summed load BELOW the junction and
/// leave it exactly as it was ABOVE. Both halves are asserted, because a parent
/// that grew everywhere would pass a "the parent got bigger" test while
/// printing a trunk nobody asked for.
#[test]
fn force_crosses_a_mid_shaft_junction_and_the_parent_grows_only_below_it() {
    let plan = sweep_with(
        &round_25(),
        0.0,
        &SweepPolicy::from_registry().expect("registry"),
    );
    let junctions = mid_shaft_junctions(&plan);
    assert!(!junctions.is_empty(), "the mechanism must have fired");

    let mut checked = 0_usize;
    for (child, parent, layer) in &junctions {
        let slot = layer - parent.k_bottom;
        let Some(&above) = parent.carried_mm2.get(slot + 1) else {
            // A junction at the parent's own foot has no layer above it inside
            // the parent; the load arithmetic below is about the two sides of a
            // junction, so there is nothing here to grade.
            continue;
        };
        let below = parent.carried_mm2[slot];
        let handed = child.carried_mm2.first().copied().expect("a sized child");
        assert!(
            below >= above + handed - 1.0e-9,
            "arm {} handed {handed} mm2 to arm {} at layer {layer}, which carried {above} mm2 \
             just above the junction and only {below} mm2 just below it",
            child.id,
            parent.id
        );
        // The area law, at the junction and one layer above it.
        let expected_below = area_law_radius_mm(below);
        let expected_above = area_law_radius_mm(above);
        assert!(
            (parent.radius_mm[slot] - expected_below).abs() < 1.0e-9,
            "arm {} is {} mm at the junction against the area law's {expected_below} mm",
            parent.id,
            parent.radius_mm[slot]
        );
        assert!(
            (parent.radius_mm[slot + 1] - expected_above).abs() < 1.0e-9,
            "arm {} is {} mm one layer ABOVE the junction against the area law's \
             {expected_above} mm",
            parent.id,
            parent.radius_mm[slot + 1]
        );
        checked += 1;
    }
    assert!(checked > 0);
    assert!(
        plan.report.conservation_residual < 1.0e-9,
        "the whole field must still conserve force: residual {}",
        plan.report.conservation_residual
    );
}
