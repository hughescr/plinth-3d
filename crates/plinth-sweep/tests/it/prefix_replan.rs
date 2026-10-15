//! **The second contact's re-plan starts from the first plan's cap-independent
//! prefix** (PX). `plan_to_c` is two functions now: the setup and the up-sweep
//! (`plan_prefix`, which never reads the contact caps) and the capped remainder
//! (`plan_after_prefix`). The sweep that made the first plan keeps a clone of
//! the prefix beside its seed, and `replan_and_choose` runs only the remainder
//! from a clone of it. The claims under test:
//!
//! * a re-plan from the prefix is the re-plan from scratch, on the complete
//!   `Debug` of the plan (the grid, every member, the whole report; an `f64`
//!   prints its shortest round-trip form, so equal text is equal bits), on a
//!   disc and on a drained, vented hollow sphere, including a fixture where the
//!   chooser KEEPS the second plan (so the path where it wins is exercised) and
//!   the retained finalist of the orientation search;
//! * the witness fired: the prefix start is counted on the calling thread, and
//!   the scratch path does not count;
//! * the kept prefix is untouched by any number of re-plans, and the seed still
//!   finishes into the first plan;
//! * every key miss (mesh, layers, policy, drains, vents, first plan) and the
//!   missing seed run today's full path, and give today's plan.
//!
//! The first plan has real short-tip rows injected on its most loaded tips (the
//! fixtures have no feature narrow enough to leave one short), so the caps are
//! real, as in `second_contact_edit.rs`.

use std::sync::{Arc, OnceLock};

use plinth_core::LayerPlan;
use plinth_mesh::{make_recessed_disc, Mesh};
use plinth_orient::demand::{DemandFidelity, SupportDemandMeasurer};
use plinth_orient::{OrientFaceTable, UpDirection};
use plinth_sweep::contact_growth::{
    ContactShort, SecondContactMode, SecondContactResult, ShortReason,
};
use plinth_sweep::orient_demand::RetainingSweepDemand;
use plinth_sweep::{
    prefix_starts_on_this_thread, run_sweep_with_holes_seeded,
    run_sweep_with_second_contact_seeded, DrainSpec, EditSeed, StrutKind, SweepPlan, SweepPolicy,
};

use crate::harness;

struct Fixture {
    name: &'static str,
    mesh: Mesh,
    layers: LayerPlan,
    policy: SweepPolicy,
    drains: Vec<DrainSpec>,
    vents: Vec<DrainSpec>,
    /// The first plan, with short-tip rows injected.
    first: SweepPlan,
    seed: Arc<EditSeed>,
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a small positive layer count"
)]
fn layer_plan(mesh: &Mesh) -> LayerPlan {
    let height = harness::layer_height_mm();
    let (min_z, max_z) = harness::z_span(mesh);
    let plate_z = min_z - harness::LIFT_MM;
    let layers = (((max_z - plate_z) / height).ceil() as usize).max(1);
    LayerPlan::new(plate_z, vec![height; layers]).expect("a uniform layer plan")
}

/// The six most loaded arms each get a short row: a contact that exists
/// (`real >= one pixel`) and is a fifth of what the load needs.
fn inject_short_tips(first: &mut SweepPlan, policy: &SweepPolicy) {
    let mut loaded: Vec<(f64, usize)> = first
        .struts
        .iter()
        .enumerate()
        .filter(|(_, strut)| strut.kind == StrutKind::Arm)
        .filter_map(|(index, strut)| Some((*strut.carried_mm2.last()?, index)))
        .collect();
    loaded.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (load, index) in loaded.into_iter().take(6) {
        let need = load / policy.x_allowed;
        first.report.seat_contact.short.push(ContactShort {
            strut: index,
            real_mm2: 0.2 * need,
            planned_mm2: need,
            profile_disc_mm2: need,
            ball_radius_mm: 0.2,
            planned_ball_radius_mm: 0.2,
            reason: ShortReason::FeatureWidth,
        });
    }
    assert!(
        first.report.seat_contact.short.iter().all(|short| {
            short.real_mm2 >= first.grid.pitch_mm() * first.grid.pitch_mm()
        }),
        "every injected row is a contact that exists, so it caps"
    );
}

fn fixture(
    name: &'static str,
    mesh: Mesh,
    drains: Vec<DrainSpec>,
    vents: Vec<DrainSpec>,
) -> Fixture {
    let layers = layer_plan(&mesh);
    let policy = SweepPolicy::from_registry().expect("the registry");
    let (mut first, seed) = run_sweep_with_holes_seeded(&mesh, &layers, &policy, &drains, &vents)
        .expect("the seeded sweep");
    inject_short_tips(&mut first, &policy);
    Fixture { name, mesh, layers, policy, drains, vents, first, seed }
}

#[cfg(feature = "slow-tests")]
fn uv_sphere(
    radius: f64,
    rings: u32,
    segments: u32,
    inward: bool,
    verts: &mut Vec<[f64; 3]>,
    tris: &mut Vec<[u32; 3]>,
) {
    let base = u32::try_from(verts.len()).expect("vertex index");
    verts.push([0.0, 0.0, radius]);
    for ring in 1..rings {
        let theta = core::f64::consts::PI * f64::from(ring) / f64::from(rings);
        for segment in 0..segments {
            let phi = 2.0 * core::f64::consts::PI * f64::from(segment) / f64::from(segments);
            verts.push([
                radius * theta.sin() * phi.cos(),
                radius * theta.sin() * phi.sin(),
                radius * theta.cos(),
            ]);
        }
    }
    verts.push([0.0, 0.0, -radius]);
    let south = base + 1 + (rings - 1) * segments;
    let at = |ring: u32, segment: u32| base + 1 + (ring - 1) * segments + segment % segments;
    let mut push = |t: [u32; 3]| tris.push(if inward { [t[0], t[2], t[1]] } else { t });
    for segment in 0..segments {
        push([base, at(1, segment), at(1, segment + 1)]);
        push([south, at(rings - 1, segment + 1), at(rings - 1, segment)]);
    }
    for ring in 1..rings - 1 {
        for segment in 0..segments {
            push([at(ring, segment), at(ring + 1, segment), at(ring + 1, segment + 1)]);
            push([at(ring, segment), at(ring + 1, segment + 1), at(ring, segment + 1)]);
        }
    }
}

const OUTER_MM: f64 = 9.0;
const WALL_MM: f64 = 2.5;

#[cfg(feature = "slow-tests")]
fn hollow_sphere() -> Mesh {
    let mut verts = Vec::new();
    let mut tris = Vec::new();
    uv_sphere(OUTER_MM, 24, 36, false, &mut verts, &mut tris);
    uv_sphere(OUTER_MM - WALL_MM, 24, 36, true, &mut verts, &mut tris);
    Mesh::new(verts, tris).expect("a closed shell and a closed cavity")
}

fn bore_radius_mm() -> f64 {
    plinth_core::m0_constants()
        .expect("registry")
        .placeholders()
        .hollow_drain_r_mm()
        .diagnostic()
        .get()
}

fn bottom_drain() -> DrainSpec {
    DrainSpec::from_bore(
        [0.0, 0.0, -(OUTER_MM - WALL_MM)],
        [0.0, 0.0, -OUTER_MM],
        bore_radius_mm(),
    )
}

fn top_vent() -> DrainSpec {
    DrainSpec {
        outlet_mm: [0.0, 0.0, OUTER_MM],
        outward_axis: [0.0, 0.0, 1.0],
        radius_mm: 0.5 * bore_radius_mm(),
    }
}

fn disc() -> &'static Fixture {
    static DISC: OnceLock<Fixture> = OnceLock::new();
    DISC.get_or_init(|| {
        let mesh = make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96).expect("the Round 25 profile");
        fixture("round-25 disc", mesh, Vec::new(), Vec::new())
    })
}

/// The fixtures every test runs on. The drained, vented hollow sphere (the
/// keep-out, the hidden cavity and the escape verdict are all live on it) takes
/// minutes to sweep a dozen times, so it joins only where the slow tests run
/// (`scripts/gate-full.sh`, `scripts/test-fast.sh --slow`).
#[cfg(feature = "slow-tests")]
fn fixtures() -> Vec<&'static Fixture> {
    vec![disc(), sphere()]
}

#[cfg(not(feature = "slow-tests"))]
fn fixtures() -> Vec<&'static Fixture> {
    vec![disc()]
}

#[cfg(feature = "slow-tests")]
fn sphere() -> &'static Fixture {
    static SPHERE: OnceLock<Fixture> = OnceLock::new();
    SPHERE.get_or_init(|| {
        fixture(
            "drained, vented hollow sphere",
            hollow_sphere(),
            vec![bottom_drain()],
            vec![top_vent()],
        )
    })
}

/// One re-plan of `first` with this request and seed, in `mode`; the plan and
/// how many prefix starts this thread counted during it.
fn replan(
    first: &SweepPlan,
    seed: Option<&EditSeed>,
    mesh: &Mesh,
    layers: &LayerPlan,
    policy: &SweepPolicy,
    drains: &[DrainSpec],
    vents: &[DrainSpec],
) -> (SweepPlan, u64) {
    let policy = SweepPolicy { second_contact_mode: SecondContactMode::Replan, ..*policy };
    let before = prefix_starts_on_this_thread();
    let plan = run_sweep_with_second_contact_seeded(
        first.clone(),
        seed,
        mesh,
        layers,
        &policy,
        drains,
        vents,
    )
    .expect("the second contact");
    (plan, prefix_starts_on_this_thread() - before)
}

fn same(left: &SweepPlan, right: &SweepPlan) -> bool {
    format!("{left:?}") == format!("{right:?}")
}

fn replan_of(fixture: &Fixture, seed: Option<&EditSeed>) -> (SweepPlan, u64) {
    replan(
        &fixture.first,
        seed,
        &fixture.mesh,
        &fixture.layers,
        &fixture.policy,
        &fixture.drains,
        &fixture.vents,
    )
}

/// The re-plan from the prefix is the re-plan from scratch, by every bit of the
/// plan and its report, and the prefix path is the one that ran.
#[test]
fn a_replan_from_the_prefix_is_the_replan_from_scratch() {
    let mut kept_somewhere = false;
    for fixture in fixtures() {
        let name = fixture.name;
        let (scratch, scratch_starts) = replan_of(fixture, None);
        let (from_prefix, starts) = replan_of(fixture, Some(&fixture.seed));
        assert_eq!(scratch_starts, 0, "{name}: no seed, no prefix start");
        assert_eq!(starts, 1, "{name}: the re-plan started from the prefix, once");
        let outcome = &from_prefix.report.seat_second_contact;
        // (A fixture may have short tips of its own besides the injected six.)
        assert!(
            outcome.attempted && outcome.tips_capped >= 1,
            "{name}: the re-plan ran on real caps ({} tips)",
            outcome.tips_capped
        );
        assert!(
            same(&scratch, &from_prefix),
            "{name}: the re-plan from the prefix differs from the re-plan from scratch"
        );
        eprintln!(
            "PX {name}: result {:?}, kept {}, arms {} -> {}, shortfall {:.4} -> {:.4}",
            outcome.result,
            outcome.kept_second,
            outcome.first_arms,
            outcome.second_arms,
            outcome.first_shortfall_mm2,
            outcome.second_shortfall_mm2,
        );
        kept_somewhere |= outcome.kept_second;
    }
    // The path where the second plan WINS the chooser is part of the claim: the
    // plan returned is the capped remainder's own field, not the first plan.
    assert!(kept_somewhere, "no fixture kept its second plan, so the winning path was not exercised");
    let (won, _) = replan_of(disc(), Some(&disc().seed));
    assert_eq!(won.report.seat_second_contact.result, SecondContactResult::Replanned);
    assert!(
        won.struts.len() != disc().first.struts.len()
            || format!("{:?}", won.struts) != format!("{:?}", disc().first.struts),
        "the kept plan is a different field from the first plan"
    );
}

/// The kept prefix is cloned for each use: its content is the same after, and
/// the seed still finishes into the first plan. A second re-plan from the same
/// seed is the first one.
#[test]
fn the_kept_prefix_is_untouched_by_a_replan() {
    for fixture in fixtures() {
        let name = fixture.name;
        let seed = &fixture.seed;
        let prefix_before = seed.prefix_fingerprint();
        let lane_before = format!("{:?}", seed.lane());
        let bytes_before = seed.approx_bytes();
        let finished_before = seed
            .finish_clone(&fixture.mesh, &fixture.layers, &fixture.policy)
            .expect("finish");
        let (once, _) = replan_of(fixture, Some(seed));
        assert_eq!(prefix_before, seed.prefix_fingerprint(), "{name}: the prefix moved");
        let (twice, starts) = replan_of(fixture, Some(seed));
        assert_eq!(starts, 1, "{name}");
        assert!(same(&once, &twice), "{name}: two re-plans from one seed differ");
        assert_eq!(prefix_before, seed.prefix_fingerprint(), "{name}: the prefix moved");
        assert_eq!(lane_before, format!("{:?}", seed.lane()), "{name}: the lane state moved");
        assert_eq!(bytes_before, seed.approx_bytes(), "{name}");
        let finished_after = seed
            .finish_clone(&fixture.mesh, &fixture.layers, &fixture.policy)
            .expect("finish");
        assert!(same(&finished_before, &finished_after), "{name}: the seed no longer finishes into P1");
    }
}

/// The prefix is counted, and counted in the seed's size.
#[test]
fn the_prefix_has_a_size() {
    for fixture in fixtures() {
        let prefix = fixture.seed.prefix_bytes();
        assert!(prefix > 0, "{}: the kept prefix is sized", fixture.name);
        assert!(
            fixture.seed.approx_bytes() >= prefix + fixture.seed.env().approx_bytes(),
            "{}: the seed's size includes the prefix",
            fixture.name
        );
    }
}

fn nudged_mesh(mesh: &Mesh) -> Mesh {
    let mut verts = mesh.verts().to_vec();
    verts[3][1] = f64::from_bits(verts[3][1].to_bits() + 1);
    Mesh::new(verts, mesh.tris().to_vec()).expect("a valid mesh")
}

/// Every key miss runs today's full path and gives today's plan: the prefix is
/// never used for a request that is not the seed's.
#[test]
fn every_key_miss_runs_the_full_path() {
    for fixture in fixtures() {
        let name = fixture.name;
        let seed = &fixture.seed;

        // mesh: one vertex, one ulp.
        let mesh = nudged_mesh(&fixture.mesh);
        let args = |mesh: &Mesh,
                    layers: &LayerPlan,
                    policy: &SweepPolicy,
                    drains: &[DrainSpec],
                    vents: &[DrainSpec],
                    first: &SweepPlan| {
            let (with_seed, starts) = replan(first, Some(seed), mesh, layers, policy, drains, vents);
            let (scratch, scratch_starts) = replan(first, None, mesh, layers, policy, drains, vents);
            (with_seed, starts, scratch, scratch_starts)
        };
        let (with_seed, starts, scratch, _) = args(
            &mesh,
            &fixture.layers,
            &fixture.policy,
            &fixture.drains,
            &fixture.vents,
            &fixture.first,
        );
        assert_eq!(starts, 0, "{name}: mesh miss used the prefix");
        assert!(same(&with_seed, &scratch), "{name}: mesh miss");

        // layers: one height, one ulp.
        let mut heights = fixture.layers.heights_mm().to_vec();
        let last = heights.len() - 1;
        heights[last] = f64::from_bits(heights[last].to_bits() + 1);
        let layers = LayerPlan::new(fixture.layers.z0_mm(), heights).expect("a layer plan");
        let (with_seed, starts, scratch, _) = args(
            &fixture.mesh,
            &layers,
            &fixture.policy,
            &fixture.drains,
            &fixture.vents,
            &fixture.first,
        );
        assert_eq!(starts, 0, "{name}: layers miss used the prefix");
        assert!(same(&with_seed, &scratch), "{name}: layers miss");

        // policy: one number that changes the prefix.
        let mut policy = fixture.policy;
        policy.x_allowed += 1.0;
        let (with_seed, starts, scratch, _) = args(
            &fixture.mesh,
            &fixture.layers,
            &policy,
            &fixture.drains,
            &fixture.vents,
            &fixture.first,
        );
        assert_eq!(starts, 0, "{name}: policy miss used the prefix");
        assert!(same(&with_seed, &scratch), "{name}: policy miss");

        // drains: one more; vents: one more.
        let mut drains = fixture.drains.clone();
        drains.push(bottom_drain());
        let (with_seed, starts, scratch, _) = args(
            &fixture.mesh,
            &fixture.layers,
            &fixture.policy,
            &drains,
            &fixture.vents,
            &fixture.first,
        );
        assert_eq!(starts, 0, "{name}: drains miss used the prefix");
        assert!(same(&with_seed, &scratch), "{name}: drains miss");
        let mut vents = fixture.vents.clone();
        vents.push(top_vent());
        let (with_seed, starts, scratch, _) = args(
            &fixture.mesh,
            &fixture.layers,
            &fixture.policy,
            &fixture.drains,
            &vents,
            &fixture.first,
        );
        assert_eq!(starts, 0, "{name}: vents miss used the prefix");
        assert!(same(&with_seed, &scratch), "{name}: vents miss");

        // the first plan the seed finished into: another plan's digest.
        let mut foreign = fixture.first.clone();
        foreign.report.sweep_digest.push('x');
        let (with_seed, starts, scratch, _) = args(
            &fixture.mesh,
            &fixture.layers,
            &fixture.policy,
            &fixture.drains,
            &fixture.vents,
            &foreign,
        );
        assert_eq!(starts, 0, "{name}: first-plan miss used the prefix");
        assert!(same(&with_seed, &scratch), "{name}: first-plan miss");
    }
}

/// The edit modes' fallback is the same re-plan, so it starts from the prefix
/// too, and gives the plan of the re-plan from scratch.
#[test]
fn the_edit_fallback_replans_from_the_prefix() {
    use plinth_sweep::contact_growth::EditExit;
    for fixture in fixtures() {
        let (scratch, _) = replan_of(fixture, None);
        let policy = SweepPolicy {
            second_contact_mode: SecondContactMode::EditThenReplan,
            ..fixture.policy
        };
        let before = prefix_starts_on_this_thread();
        let out = run_sweep_with_second_contact_seeded(
            fixture.first.clone(),
            Some(&fixture.seed),
            &fixture.mesh,
            &fixture.layers,
            &policy,
            &fixture.drains,
            &fixture.vents,
        )
        .expect("the second contact");
        let starts = prefix_starts_on_this_thread() - before;
        let outcome = &out.report.seat_second_contact;
        eprintln!(
            "PX {} EditThenReplan: edit exit {:?}, result {:?}, prefix starts {starts}",
            fixture.name, outcome.edit.exit, outcome.result
        );
        if outcome.edit.exit == EditExit::Kept {
            assert_eq!(starts, 0, "a kept edit makes no re-plan");
        } else {
            // The edit was thrown away and today's re-plan ran from the prefix.
            assert_eq!(starts, 1, "{}: the fallback re-plan started from the prefix", fixture.name);
            assert_eq!(format!("{:?}", out.struts), format!("{:?}", scratch.struts));
        }
    }
}

/// The orientation search's retained finalist carries its seed (with the prefix
/// it kept at the measurement policy); a re-plan of its plan from that seed is
/// the re-plan from scratch.
#[test]
fn a_retained_finalists_prefix_replans_to_the_same_plan() {
    let mesh = disc().mesh.clone();
    let table = OrientFaceTable::build(&mesh, "exact").expect("a closed fixture scores");
    let demand = RetainingSweepDemand::production();
    let measured = demand.measure_support_demand(
        &mesh,
        &table,
        &[UpDirection::MODEL_UP],
        DemandFidelity::Production,
    );
    assert!(measured[0].is_ok(), "the pose measured");
    let plans = demand.drain();
    assert_eq!(plans.len(), 1, "the finalist was kept");
    let retained = &plans[0];
    let seed = retained.seed.as_ref().expect("a retained pose keeps its seed");
    let emitted = SweepPolicy::from_registry().expect("the registry");
    let mut first = retained.plan.clone();
    inject_short_tips(&mut first, &emitted);
    let (scratch, _) = replan(&first, None, &retained.mesh, &retained.layers, &emitted, &[], &[]);
    let (from_prefix, starts) =
        replan(&first, Some(seed), &retained.mesh, &retained.layers, &emitted, &[], &[]);
    assert_eq!(starts, 1, "the retained seed's key matches the emission policy: prefix used");
    assert!(same(&scratch, &from_prefix), "the retained finalist's re-plan differs");
}

/// **Sabotage of the comparison.** `same` turns red on the smallest difference
/// that could matter, and a re-plan of other caps is another plan, so equality
/// above means equal.
#[test]
fn sabotage_the_comparison_sees_one_ulp() {
    let fixture = disc();
    let (plan, _) = replan_of(fixture, Some(&fixture.seed));
    let mut changed = plan.clone();
    assert!(same(&changed, &plan));
    let strut = changed
        .struts
        .iter_mut()
        .find(|strut| !strut.radius_mm.is_empty())
        .expect("a sized member");
    strut.radius_mm[0] = f64::from_bits(strut.radius_mm[0].to_bits() + 1);
    assert!(!same(&changed, &plan), "one ulp in one radius is seen");
    // Other caps give another plan (the caps are read by the remainder).
    let mut other = fixture.first.clone();
    other.report.seat_contact.short.truncate(2);
    let (fewer, starts) = replan(
        &other,
        Some(&fixture.seed),
        &fixture.mesh,
        &fixture.layers,
        &fixture.policy,
        &[],
        &[],
    );
    assert_eq!(starts, 1);
    assert_eq!(fewer.report.seat_second_contact.tips_capped, 2);
    assert!(!same(&fewer, &plan), "two caps are not six caps");
}
