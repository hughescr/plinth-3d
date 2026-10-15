//! The cap-independent prefix (PX): the capped remainder started from a clone of
//! the prefix is the capped sweep from scratch, bit for bit, and the equality
//! is sensitive (a perturbed prefix gives another plan). The whole entry
//! (chooser, seed key, edit fallback, retained finalist) is tested in
//! `tests/it/prefix_replan.rs`; this file reaches the private halves.

use crate::down_sweep::ContactCaps;
use crate::sweep_state::Prefix;
use crate::{
    finish, plan_after_prefix, plan_prefix, run_sweep_inner, LayerPlan, Mesh, StrutKind,
    SweepPlan, SweepPolicy,
};

fn fixture() -> (Mesh, LayerPlan) {
    let mesh = plinth_mesh::make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96)
        .expect("the Round 25 profile");
    let height = *plinth_core::m0_constants()
        .expect("registry")
        .evidence()
        .layer_height_default_mm()
        .value();
    let min_z = mesh.verts().iter().map(|v| v[2]).fold(f64::INFINITY, f64::min);
    let max_z = mesh.verts().iter().map(|v| v[2]).fold(f64::NEG_INFINITY, f64::max);
    let plate_z = min_z - 5.0;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small positive layer count"
    )]
    let layers = (((max_z - plate_z) / height).ceil() as usize).max(1);
    (mesh, LayerPlan::new(plate_z, vec![height; layers]).expect("a uniform layer plan"))
}

/// Caps on the six most loaded arms of `first`, each a fifth of what its load
/// needs.
fn caps_of(first: &SweepPlan, policy: &SweepPolicy) -> ContactCaps {
    let mut loaded: Vec<(f64, usize)> = first
        .struts
        .iter()
        .enumerate()
        .filter(|(_, strut)| strut.kind == StrutKind::Arm)
        .filter_map(|(index, strut)| Some((*strut.carried_mm2.last()?, index)))
        .collect();
    loaded.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut caps = ContactCaps::default();
    for (load, index) in loaded.into_iter().take(6) {
        let strut = &first.struts[index];
        caps.insert(strut.cell, strut.k_top, 0.2 * load / policy.x_allowed);
    }
    assert!(!caps.is_empty());
    caps
}

fn text(plan: &SweepPlan) -> String {
    format!("{plan:?}")
}

/// The capped remainder from `prefix`, finished.
fn from_prefix(
    prefix: Prefix,
    mesh: &Mesh,
    layers: &LayerPlan,
    policy: &SweepPolicy,
    caps: &ContactCaps,
) -> SweepPlan {
    // No lent layer cache: the remainder builds its own, as a kept prefix's does.
    let (env, lane) = plan_after_prefix(prefix, None, mesh, layers, policy, &[], &[], Some(caps))
        .expect("the capped remainder");
    finish(&env, lane, mesh, layers, policy).expect("finish")
}

/// The prefix of a sweep of this request, kept (its layer cache dropped).
fn prefix_of(mesh: &Mesh, layers: &LayerPlan, policy: &SweepPolicy) -> Prefix {
    plan_prefix(mesh, layers, policy, &[], &[], |prefix, _| Ok(prefix)).expect("prefix")
}

/// The remainder from a prefix is the capped sweep, and a prefix with one thing
/// changed is not: each perturbation moves the plan AND the fingerprint, so
/// neither "equal" nor "untouched" is vacuous.
#[test]
fn a_perturbed_prefix_is_seen_and_an_intact_one_is_the_capped_sweep() {
    let (mesh, layers) = fixture();
    let policy = SweepPolicy::from_registry().expect("the registry");
    let first = run_sweep_inner(&mesh, &layers, &policy, &[], &[], None).expect("first");
    let caps = caps_of(&first, &policy);
    let scratch = run_sweep_inner(&mesh, &layers, &policy, &[], &[], Some(&caps)).expect("capped");
    assert_ne!(text(&scratch), text(&first), "the caps changed the field, or the test sees nothing");

    let prefix = prefix_of(&mesh, &layers, &policy);
    let intact = prefix.fingerprint();
    let copy = from_prefix(prefix.clone(), &mesh, &layers, &policy, &caps);
    assert_eq!(text(&scratch), text(&copy), "the remainder from the prefix is the capped sweep");
    assert_eq!(intact, prefix.fingerprint(), "the remainder ran on a clone");

    type Sabotage = (&'static str, fn(&mut Prefix));
    let sabotages: [Sabotage; 5] = [
        ("one member dropped", |prefix| {
            prefix.struts.pop();
        }),
        ("rasteriser entries", |prefix| prefix.rasteriser_entries += 1),
        ("reach tests", |prefix| prefix.reach.reach_tests += 1),
        ("demand cells", |prefix| prefix.demand_cells_total += 1),
        ("keep-out refusals", |prefix| prefix.keep_out_refusals += 1),
    ];
    for (name, sabotage) in sabotages {
        let mut broken = prefix.clone();
        sabotage(&mut broken);
        assert_ne!(intact, broken.fingerprint(), "{name}: the fingerprint did not see it");
        let plan = from_prefix(broken, &mesh, &layers, &policy, &caps);
        assert_ne!(text(&plan), text(&scratch), "{name}: the comparison did not see it");
    }
}

/// A model surface a later stage builds is not seen by a remainder that must
/// start without one: with the early build off (no drawn-solid descent, no
/// thickening refusal), the prefix keeps no surface, and a re-plan after the
/// first plan's finish (which builds its own) is still the capped sweep.
#[test]
fn a_late_model_surface_is_not_shared_with_a_sibling_remainder() {
    let (mesh, layers) = fixture();
    let mut policy = SweepPolicy::from_registry().expect("the registry");
    policy.drawn_solid_descent = false;
    policy.slenderness_thicken = false;
    let first = run_sweep_inner(&mesh, &layers, &policy, &[], &[], None).expect("first");
    let caps = caps_of(&first, &policy);
    let scratch = run_sweep_inner(&mesh, &layers, &policy, &[], &[], Some(&caps)).expect("capped");
    let prefix = prefix_of(&mesh, &layers, &policy);
    assert!(!prefix.surface_early, "the early build is off in this fixture");
    // Two remainders in a row: the first fills its own surface cell on the way.
    let one = from_prefix(prefix.clone(), &mesh, &layers, &policy, &caps);
    let two = from_prefix(prefix.clone(), &mesh, &layers, &policy, &caps);
    assert_eq!(text(&scratch), text(&one));
    assert_eq!(text(&scratch), text(&two));
    assert!(prefix.model_surface.get().is_none(), "the prefix's own cell stayed empty");
}
