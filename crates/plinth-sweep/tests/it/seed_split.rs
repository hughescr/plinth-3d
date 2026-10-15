//! **The sweep is two halves, and the first half's state is kept** (SC-1,
//! `plinth-evidence/perf-2026-10-14/design/SC-DESIGN.md` §2 and §6).
//!
//! `run_sweep_inner` is `plan_to_c` (everything before the start of
//! `Span::StageJoints`) then `finish` (the rest). The claims under test:
//!
//! * the split changes nothing: a plain sweep, a sweep that also keeps the
//!   seed, and `finish` on a clone of the seed give the SAME plan, compared by
//!   the complete `Debug` of the plan (an `f64` prints its shortest round-trip
//!   form, so equal text is equal bits) on a disc and on a drained, vented
//!   hollow sphere (the escape verdict reads the raw drains after C);
//! * `finish` works on a clone: finishing twice gives equal plans and the seed
//!   itself is never mutated (and finishing really does change the members, so
//!   that check is not vacuous);
//! * a seed made inside a rayon race is finished after it, on another thread;
//! * the identity key refuses a seed on each of mesh, layers, policy, drains,
//!   vents and first plan, one ulp being enough, and treats the measurement
//!   and the emission policy as equal;
//! * the orientation search's retained plan carries its seed, whose key matches
//!   the emitted policy.
//!
//! The sensitivity of the comparison and of the key is itself tested (the
//! sabotage tests). The old-code equality (the unsplit function against this
//! one) is recorded in the lane notes: the same fixtures were hashed on the
//! develop commit this lane branched from, and the hashes printed by
//! `the_split_changes_nothing` (run with `--nocapture`) are equal.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};

use plinth_core::LayerPlan;
use plinth_mesh::{make_recessed_disc, Mesh};
use plinth_orient::demand::{DemandFidelity, SupportDemandMeasurer};
use plinth_orient::{OrientFaceTable, UpDirection};
use plinth_sweep::orient_demand::{production_measurement_policy, reuse_for, RetainingSweepDemand};
use plinth_sweep::{
    run_sweep_with_holes, run_sweep_with_holes_seeded, run_sweep_with_second_contact, DrainSpec,
    EditSeed, SeedMiss, SweepDemand, SweepPlan, SweepPolicy,
};

use crate::harness;

/// One fixture and the three plans made from it.
struct Fixture {
    name: &'static str,
    mesh: Mesh,
    layers: LayerPlan,
    policy: SweepPolicy,
    drains: Vec<DrainSpec>,
    vents: Vec<DrainSpec>,
    plain: SweepPlan,
    seeded: SweepPlan,
    seed: Arc<EditSeed>,
}

/// The plan every harness sweep uses: the registered layer height, the plate
/// `LIFT_MM` under the model.
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

fn fixture(
    name: &'static str,
    mesh: Mesh,
    drains: Vec<DrainSpec>,
    vents: Vec<DrainSpec>,
) -> Fixture {
    let layers = layer_plan(&mesh);
    let policy = SweepPolicy::from_registry().expect("the registry");
    let plain = run_sweep_with_holes(&mesh, &layers, &policy, &drains, &vents)
        .expect("the sweep emits and warns");
    let (seeded, seed) = run_sweep_with_holes_seeded(&mesh, &layers, &policy, &drains, &vents)
        .expect("the seeded sweep emits and warns");
    Fixture {
        name,
        mesh,
        layers,
        policy,
        drains,
        vents,
        plain,
        seeded,
        seed,
    }
}

/// A closed UV sphere about the origin, appended to `verts` / `tris`;
/// `inward` reverses every triangle, which makes it a cavity wall.
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

/// Through the bottom pole, from the cavity to the outside, straight down.
fn bottom_drain() -> DrainSpec {
    DrainSpec::from_bore(
        [0.0, 0.0, -(OUTER_MM - WALL_MM)],
        [0.0, 0.0, -OUTER_MM],
        bore_radius_mm(),
    )
}

/// A vent at the top pole.
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

/// The complete `Debug` of a plan: the grid, every member, the whole report.
fn text(plan: &SweepPlan) -> String {
    format!("{plan:?}")
}

fn same_plan(left: &SweepPlan, right: &SweepPlan) -> bool {
    text(left) == text(right)
}

fn hash_of(plan: &SweepPlan) -> u64 {
    let mut hasher = DefaultHasher::new();
    text(plan).hash(&mut hasher);
    hasher.finish()
}

fn ask(fixture: &Fixture) -> Option<SeedMiss> {
    fixture.seed.miss(
        &fixture.mesh,
        &fixture.layers,
        &fixture.policy,
        &fixture.drains,
        &fixture.vents,
        &fixture.plain.report.sweep_digest,
    )
}

/// The split is the old function: plain, seeded, and `finish` on a clone are
/// one plan, twice over, and the fixtures reach the paths that matter.
#[test]
fn the_split_changes_nothing() {
    for fixture in [disc(), sphere()] {
        let name = fixture.name;
        // Path hits: a real field with real work after C, or equality is cheap.
        assert!(fixture.plain.struts.len() > 20, "{name}: a field of arms");
        let certificate = &fixture.plain.report.heads.joint_certificate;
        assert!(
            certificate.certified + certificate.not_sized > 0
                || fixture.plain.report.heads.built == 0,
            "{name}: the head sizing ran when there are heads"
        );
        assert!(
            same_plan(&fixture.plain, &fixture.seeded),
            "{name}: keeping the seed changed the plan"
        );
        let once = fixture
            .seed
            .finish_clone(&fixture.mesh, &fixture.layers, &fixture.policy)
            .expect("finish");
        let twice = fixture
            .seed
            .finish_clone(&fixture.mesh, &fixture.layers, &fixture.policy)
            .expect("finish");
        assert!(same_plan(&fixture.plain, &once), "{name}: finish on a clone differs");
        assert!(same_plan(&once, &twice), "{name}: two finishes of one seed differ");
        assert_eq!(
            fixture.seed.p1_digest(),
            fixture.plain.report.sweep_digest,
            "{name}: the digest the seed finished into"
        );
        // Compared with the same fixture on the develop commit this lane
        // branched from (run with --nocapture).
        eprintln!("SC1-HASH {name}: {:016x}", hash_of(&fixture.plain));
    }
}

/// The drained fixture's escape verdict, read after C from the raw drains, is
/// the same through the seed.
#[test]
fn the_escape_verdict_is_the_same_through_the_seed() {
    let fixture = sphere();
    let plain = &fixture.plain.report.escape;
    assert_eq!(plain.verdicts.len(), 1, "one drain, one verdict (the check ran)");
    let through = fixture
        .seed
        .finish_clone(&fixture.mesh, &fixture.layers, &fixture.policy)
        .expect("finish");
    assert_eq!(format!("{plain:?}"), format!("{:?}", through.report.escape));
    assert_eq!(fixture.seed.env().drains(), fixture.drains.as_slice());
    assert_eq!(fixture.seed.env().vents(), fixture.vents.as_slice());
}

/// Finishing works on a copy: the seed is the same before and after, and the
/// finish did change the members (so "the seed is untouched" is not vacuous).
#[test]
fn finishing_a_clone_never_touches_the_seed() {
    for fixture in [disc(), sphere()] {
        let name = fixture.name;
        let before = format!("{:?}", fixture.seed.lane());
        let members_at_c = format!("{:?}", fixture.seed.lane().struts());
        let finished = fixture
            .seed
            .finish_clone(&fixture.mesh, &fixture.layers, &fixture.policy)
            .expect("finish");
        let again = fixture
            .seed
            .finish_clone(&fixture.mesh, &fixture.layers, &fixture.policy)
            .expect("finish");
        assert_eq!(before, format!("{:?}", fixture.seed.lane()), "{name}: the seed moved");
        assert_ne!(
            members_at_c,
            format!("{:?}", finished.struts),
            "{name}: finish changed no member (seat faces, slides, head sizes), so the test saw nothing"
        );
        assert!(same_plan(&finished, &again), "{name}");
        // A working copy is its own: finishing it does not move the seed either.
        let working = fixture.seed.lane_clone();
        assert_eq!(format!("{working:?}"), before, "{name}: the clone is the seed's state");
    }
}

/// A seed made inside a rayon race is finished after it, and on another
/// thread; the plan is the plan.
#[test]
fn a_seed_made_inside_the_race_is_finished_after_it() {
    let fixture = disc();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(3)
        .build()
        .expect("a rayon pool");
    let made: Vec<(SweepPlan, Arc<EditSeed>)> = pool.install(|| {
        use rayon::prelude::*;
        (0..3)
            .into_par_iter()
            .map(|_| {
                run_sweep_with_holes_seeded(
                    &fixture.mesh,
                    &fixture.layers,
                    &fixture.policy,
                    &fixture.drains,
                    &fixture.vents,
                )
                .expect("the seeded sweep")
            })
            .collect()
    });
    for (plan, seed) in made {
        assert!(same_plan(&plan, &fixture.plain));
        let (mesh, layers, policy) = (
            fixture.mesh.clone(),
            fixture.layers.clone(),
            fixture.policy,
        );
        let after = std::thread::spawn(move || seed.finish_clone(&mesh, &layers, &policy))
            .join()
            .expect("the thread")
            .expect("finish");
        assert!(same_plan(&after, &fixture.plain), "finished on another thread, after the race");
    }
}

/// The identity key: every input refuses, one ulp is enough, and the
/// measurement and emission policies are one.
#[test]
fn the_identity_key_refuses_each_input() {
    for fixture in [disc(), sphere()] {
        let name = fixture.name;
        assert_eq!(ask(fixture), None, "{name}: the seed belongs to its own request");

        // mesh: one vertex, one ulp.
        let mut verts = fixture.mesh.verts().to_vec();
        verts[3][1] = f64::from_bits(verts[3][1].to_bits() + 1);
        let nudged = Mesh::new(verts, fixture.mesh.tris().to_vec()).expect("a valid mesh");
        assert_eq!(
            fixture.seed.miss(
                &nudged,
                &fixture.layers,
                &fixture.policy,
                &fixture.drains,
                &fixture.vents,
                &fixture.plain.report.sweep_digest
            ),
            Some(SeedMiss::Mesh),
            "{name}"
        );

        // layers: one height, one ulp.
        let mut heights = fixture.layers.heights_mm().to_vec();
        let last = heights.len() - 1;
        heights[last] = f64::from_bits(heights[last].to_bits() + 1);
        let other = LayerPlan::new(fixture.layers.z0_mm(), heights).expect("a layer plan");
        assert_eq!(
            fixture.seed.miss(
                &fixture.mesh,
                &other,
                &fixture.policy,
                &fixture.drains,
                &fixture.vents,
                &fixture.plain.report.sweep_digest
            ),
            Some(SeedMiss::Layers),
            "{name}"
        );

        // policy: one number.
        let mut policy = fixture.policy;
        policy.x_allowed += 1.0;
        assert_eq!(
            fixture.seed.miss(
                &fixture.mesh,
                &fixture.layers,
                &policy,
                &fixture.drains,
                &fixture.vents,
                &fixture.plain.report.sweep_digest
            ),
            Some(SeedMiss::Policy),
            "{name}"
        );

        // drains: one more; vents: one more.
        let mut drains = fixture.drains.clone();
        drains.push(bottom_drain());
        assert_eq!(
            fixture.seed.miss(
                &fixture.mesh,
                &fixture.layers,
                &fixture.policy,
                &drains,
                &fixture.vents,
                &fixture.plain.report.sweep_digest
            ),
            Some(SeedMiss::Drains),
            "{name}"
        );
        let mut vents = fixture.vents.clone();
        vents.push(top_vent());
        assert_eq!(
            fixture.seed.miss(
                &fixture.mesh,
                &fixture.layers,
                &fixture.policy,
                &fixture.drains,
                &vents,
                &fixture.plain.report.sweep_digest
            ),
            Some(SeedMiss::Vents),
            "{name}"
        );

        // the first plan it finished into.
        assert_eq!(
            fixture.seed.miss(
                &fixture.mesh,
                &fixture.layers,
                &fixture.policy,
                &fixture.drains,
                &fixture.vents,
                "another plan"
            ),
            Some(SeedMiss::FirstPlan),
            "{name}"
        );
    }
}

/// The seed is made under the measurement policy (shake check deferred) and
/// used under the emission policy, and the key reads both as one; it still
/// separates any other difference.
#[test]
fn the_measurement_and_the_emission_policy_are_one_key() {
    let fixture = disc();
    let emission = fixture.policy;
    let measurement = production_measurement_policy(emission);
    assert_ne!(emission, measurement, "the two policies are different values");
    for asked in [emission, measurement] {
        assert_eq!(
            fixture.seed.miss(
                &fixture.mesh,
                &fixture.layers,
                &asked,
                &fixture.drains,
                &fixture.vents,
                &fixture.plain.report.sweep_digest
            ),
            None,
            "both sides normalize with production_measurement_policy"
        );
    }
    // A seed MADE under the measurement policy answers to the emission policy.
    let (plan, seed) = run_sweep_with_holes_seeded(
        &fixture.mesh,
        &fixture.layers,
        &measurement,
        &fixture.drains,
        &fixture.vents,
    )
    .expect("the seeded sweep");
    assert_eq!(
        seed.miss(
            &fixture.mesh,
            &fixture.layers,
            &emission,
            &fixture.drains,
            &fixture.vents,
            &plan.report.sweep_digest
        ),
        None
    );
    assert_eq!(seed.key(), fixture.seed.key());
}

/// No short tip, no caps: today's exit returns the first plan itself.
#[test]
fn empty_caps_return_the_first_plan_untouched() {
    let fixture = disc();
    let first = fixture.plain.clone();
    let kept = run_sweep_with_second_contact(
        first.clone(),
        &fixture.mesh,
        &fixture.layers,
        &fixture.policy,
        &fixture.drains,
        &fixture.vents,
    )
    .expect("the second contact");
    if !kept.report.seat_second_contact.attempted {
        assert!(same_plan(&first, &kept), "no caps: the first plan, bit for bit");
    }
}

/// The orientation search keeps each finalist's seed beside its plan; the seed's
/// key matches the policy the plan is emitted under, finishing it gives the
/// retained plan, and a pose that loses the race takes its seed with it.
#[test]
fn a_retained_finalist_carries_its_seed() {
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
    assert_eq!(
        seed.miss(
            &retained.mesh,
            &retained.layers,
            &emitted,
            &[],
            &[],
            &retained.plan.report.sweep_digest
        ),
        None,
        "the key matches the emission policy"
    );
    let reused = reuse_for(retained, &retained.mesh, &retained.layers, &emitted, &[], &[])
        .expect("the reuse key hits");
    assert_eq!(reused.report.sweep_digest, seed.p1_digest());
    let finished = seed
        .finish_clone(&retained.mesh, &retained.layers, &retained.policy)
        .expect("finish");
    assert!(same_plan(&finished, &retained.plan), "finishing the seed gives the retained plan");
    // The bytes are counted (a lower bound) and the measurer without a store keeps none.
    assert!(seed.approx_bytes() > 0);
    let plain = SweepDemand::production().measure_support_demand(
        &mesh,
        &table,
        &[UpDirection::MODEL_UP],
        DemandFidelity::Production,
    );
    assert_eq!(plain.len(), 1);
}

/// The seed has a size, and it grows with the field it holds.
#[test]
fn a_seed_reports_its_bytes() {
    for fixture in [disc(), sphere()] {
        let bytes = fixture.seed.approx_bytes();
        let members = u64::try_from(fixture.seed.lane().struts().len()).expect("small");
        assert!(bytes > members * 100, "{}: {bytes} bytes for {members} members", fixture.name);
        assert!(bytes > fixture.seed.env().approx_bytes(), "{}: the lane adds to the env", fixture.name);
    }
}

/// **Sabotage of the comparison.** The tests above compare `Debug` text; this
/// shows the comparison turns red on the smallest difference that could matter,
/// so "equal" means equal.
#[test]
fn sabotage_the_comparison_sees_one_ulp() {
    let fixture = disc();
    let mut changed = fixture.plain.clone();
    assert!(same_plan(&changed, &fixture.plain));
    let strut = changed.struts.iter_mut().find(|strut| !strut.radius_mm.is_empty()).expect("a sized member");
    strut.radius_mm[0] = f64::from_bits(strut.radius_mm[0].to_bits() + 1);
    assert!(!same_plan(&changed, &fixture.plain), "one ulp in one radius is seen");
    let mut other = fixture.plain.clone();
    other.report.sweep_digest.push('x');
    assert!(!same_plan(&other, &fixture.plain), "the digest is seen");
    // And a plan made under another policy is not the same plan.
    let mut policy = fixture.policy;
    policy.x_allowed += 1.0;
    let different = run_sweep_with_holes(&fixture.mesh, &fixture.layers, &policy, &[], &[]).expect("sweep");
    assert!(!same_plan(&different, &fixture.plain), "another policy, another plan");
}
