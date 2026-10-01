//! **The down-sweep does not pour load into material the up-sweep left
//! unrooted** (drain-rim routing, 2026-09-30).
//!
//! # The defect this pins
//!
//! On the hollow Sphere, every mm² of uncarried load sat on the same 100 cells
//! of the drain rim, and it rose 376.26 -> 405.00 mm² when lattice-align moved
//! the lower arm rings one or two layers up (`lanes/lattice-align.md`, round 7,
//! "Blocker 2"). The up-sweep roots the shell above the rim LATERALLY, from
//! the ring of arms beside the drain's keep-out, and leaves the rim itself
//! unrooted. The down-sweep sent a continuing model column's load straight
//! down whatever it stood on, so the peel of the whole lower cap walked down
//! the shell into the rim, where nothing carries it.
//!
//! # The rule
//!
//! One predicate, the up-sweep's own (`ReachState::is_rooted`): a model column
//! that is not rooted at a layer is not a routing root there. Load arriving in
//! it goes sideways to the nearest rooted carrier within the one-lamina bound
//! every routed cell obeys, and down its own column only where there is none
//! ([`plinth_sweep::down_sweep::UnrootedColumns`]).
//!
//! # The ledger
//!
//! Built bare (round 7's experiment), a column whose load went sideways
//! re-added its own peel at every layer without counting it as injected, and
//! the whole-run identity `injected = plate + uncarried + absorbed` missed by
//! -558 mm² on the Sphere. The rule's ledger holds that peel out of the raster
//! while the column keeps going sideways, and puts it back, counted as
//! injected, where the column's next step is not sideways. Both halves are
//! graded here: the identity must close, and the max-field raster must never
//! read less uncarried load than one single peel event loses (the per-event
//! pass, which routes each event on its own along the recorded hops).
//!
//! # The fixture
//!
//! The Sphere's own geometry, built here so the test needs no provisioned
//! corpus and no hollowing stage: a 20 mm sphere with a 2.5 mm wall (the
//! corpus Sphere is 19.98 x 19.96 x 20.00 mm, wall requested 2.5 mm), and the
//! registered 1.25 mm bore at its bottom pole, pointing straight down. A
//! sphere has no preferred pose, so the corpus run's X 30° pose is the same
//! shape. Measured at this commit, gate profile:
//!
//! | policy | uncarried mm² | largest one event mm² | load gap mm² | plate mm² |
//! |---|---|---|---|---|
//! | production | 21.8000 | 21.8000 | 0.0000 | 500.8300 |
//! | rule off (`without_unrooted_routing`) | 396.6375 | 115.0600 | 0.0000 | 92.4575 |
//! | ledger off (`without_handed_column_ledger`) | 21.8000 | 21.8000 | -679.1650 | 1146.3375 |

use std::sync::OnceLock;

use crate::harness;

use plinth_sweep::{DrainSpec, SweepPlan, SweepPolicy};

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
            push([
                at(ring, segment),
                at(ring + 1, segment),
                at(ring + 1, segment + 1),
            ]);
            push([
                at(ring, segment),
                at(ring + 1, segment + 1),
                at(ring, segment + 1),
            ]);
        }
    }
}

const OUTER_MM: f64 = 10.0;
const WALL_MM: f64 = 2.5;

/// The hollow Sphere: 4,800 triangles a shell (the corpus file has 4,760).
fn hollow_sphere() -> plinth_mesh::Mesh {
    let mut verts = Vec::new();
    let mut tris = Vec::new();
    uv_sphere(OUTER_MM, 40, 60, false, &mut verts, &mut tris);
    uv_sphere(OUTER_MM - WALL_MM, 40, 60, true, &mut verts, &mut tris);
    plinth_mesh::Mesh::new(verts, tris).expect("a closed shell and a closed cavity")
}

/// The registered bore, read rather than restated.
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

struct Runs {
    production: SweepPlan,
    rule_off: SweepPlan,
    ledger_off: SweepPlan,
}

/// The three sweeps, run once for every test in this file.
fn runs() -> &'static Runs {
    static RUNS: OnceLock<Runs> = OnceLock::new();
    RUNS.get_or_init(|| {
        let mesh = hollow_sphere();
        let drains = [bottom_drain()];
        let policy = SweepPolicy::from_registry().expect("registry");
        Runs {
            production: harness::sweep_with_drains(&mesh, &policy, &drains),
            rule_off: harness::sweep_with_drains(
                &mesh,
                &policy.without_unrooted_routing(),
                &drains,
            ),
            ledger_off: harness::sweep_with_drains(
                &mesh,
                &policy.without_handed_column_ledger(),
                &drains,
            ),
        }
    })
}

/// `injected - plate - uncarried - absorbed`: load the down-sweep cannot name.
fn unexplained(plan: &SweepPlan) -> f64 {
    plan.report.landing_check.unexplained_gap_mm2()
}

fn summary(name: &str, plan: &SweepPlan) -> String {
    let report = &plan.report;
    let rule = &report.unrooted_routing;
    format!(
        "{name}: uncarried {:.4}, largest one event {:.4}, injected {:.4}, plate {:.4}, \
         unexplained {:.4}; refused {}, sent sideways {:.4}, kept {:.4}, held {} ({:.4}), \
         re-entered {} ({:.4}); overloaded {}",
        report.uncarried_load_mm2,
        report.per_event.max_event_uncarried_mm2,
        report.injected_load_mm2,
        report.plate_reaction_mm2,
        unexplained(plan),
        rule.columns_refused,
        rule.load_rerouted_mm2,
        rule.load_kept_mm2,
        rule.handed_columns_held,
        rule.handed_own_withheld_mm2,
        rule.handed_events_reentered,
        rule.handed_reentered_mm2,
        report.overloaded_struts.len(),
    )
}

/// THE RULE, AND ITS SABOTAGE. With the rule off the lower cap's peel walks
/// down the shell into the rim and is lost there; with it on, that load goes to
/// the rooted carriers beside the keep-out. The bound is a factor, not a
/// number: the rule must remove at least nine tenths of what the routing
/// without it loses (measured: 21.80 against 396.64 mm², 5.5 %), and what is
/// left must be no more than one event's worth (the rim's own peel, which no
/// carrier is in reach of under either routing).
#[test]
fn load_reaching_an_unrooted_column_goes_to_the_rooted_carrier_beside_it() {
    let runs = runs();
    let on = &runs.production.report;
    let off = &runs.rule_off.report;
    eprintln!("{}", summary("production", &runs.production));
    eprintln!("{}", summary("rule off", &runs.rule_off));
    // Path hits: the up-sweep left the rim unrooted, the rule refused those
    // columns, and load actually went sideways.
    assert!(
        on.residual_demand_cells > 0 && on.unrooted_routing.recorded_cells > 0,
        "the fixture must leave something unrooted, or this grades nothing: {}",
        summary("production", &runs.production)
    );
    assert!(
        on.unrooted_routing.columns_refused > 0 && on.unrooted_routing.load_rerouted_mm2 > 0.0,
        "the rule must fire on this fixture: {}",
        summary("production", &runs.production)
    );
    assert_eq!(
        off.unrooted_routing.columns_refused, 0,
        "the sabotage must actually switch the rule off"
    );
    assert!(
        off.uncarried_load_mm2 > 0.0,
        "without the rule the rim must lose load, or the fixture is not adversarial"
    );
    assert!(
        on.uncarried_load_mm2 <= 0.1 * off.uncarried_load_mm2,
        "the rule must remove at least 90 % of the rim's uncarried load: {} against {}",
        summary("production", &runs.production),
        summary("rule off", &runs.rule_off)
    );
    assert!(
        on.uncarried_load_mm2 <= on.per_event.max_event_mm2,
        "what is left must be at most one event's worth: {}",
        summary("production", &runs.production)
    );
    // The same field, sized: nothing the rule moved may leave an arm over
    // the area law.
    assert!(
        on.overloaded_struts.is_empty(),
        "the load moved onto the ring must be sized, not overloaded: {:?}",
        on.overloaded_struts
    );
}

/// THE LEDGER, AND ITS SABOTAGE. On the re-routed field the whole-run identity
/// closes (to 1e-9 of the injected total), and the max-field raster reads at
/// least as much uncarried load as any one event loses: a ledger that held a
/// re-routed column's later events out of the raster for good closed the
/// identity too, and read 0 lost where one event lost 3.5 mm² (the
/// slab-with-a-drain fixture, round 8 exploration). The sabotage, which
/// re-adds a re-routed column's peel at every layer uncounted, must break the
/// identity (measured -679.17 mm² here; -558 mm² on the corpus Sphere).
#[test]
fn a_rerouted_column_keeps_the_whole_run_identity() {
    let runs = runs();
    let on = &runs.production;
    eprintln!("{}", summary("production", on));
    eprintln!("{}", summary("ledger off", &runs.ledger_off));
    let tolerance = 1.0e-9 * on.report.injected_load_mm2.max(1.0);
    assert!(
        unexplained(on).abs() <= tolerance,
        "injected = plate + uncarried + absorbed must close on the re-routed field: {}",
        summary("production", on)
    );
    assert!(
        on.report.per_event.max_event_uncarried_mm2 <= on.report.uncarried_load_mm2 + tolerance,
        "the max-field raster must not read less lost load than one event loses: {}",
        summary("production", on)
    );
    assert!(
        on.report.per_event.conservation_residual <= 1.0e-9,
        "per-event conservation must hold: {}",
        summary("production", on)
    );
    assert!(
        on.report.unrooted_routing.handed_columns_held > 0
            && on.report.unrooted_routing.handed_events_reentered > 0,
        "both halves of the ledger must run on this fixture: {}",
        summary("production", on)
    );
    assert!(
        unexplained(&runs.ledger_off) < -1.0,
        "the sabotaged ledger must break the identity, or this test cannot see it: {}",
        summary("ledger off", &runs.ledger_off)
    );
}
