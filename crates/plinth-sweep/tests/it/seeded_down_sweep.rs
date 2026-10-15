//! **A second contact's first pass is a checked edit of the first run's last
//! pass** (SC-2, `plinth-evidence/perf-2026-10-14/design/SC-DESIGN.md` §2, §6).
//!
//! `down_sweep::run_seeded` takes a [`RunSeed`] (the route log of a finished
//! call's last pass, that pass's accounting, its refusals and a digest of the
//! field they are valid for) and, on an equal field, replaces the walk of
//! pass 1 by the log. The claim under test is the whole of SC-2: the seeded
//! call's outcome and every member it leaves equal those of the call that
//! walked pass 1, BY BITS, on fixtures that reach the second-contact topology
//! (a finished first run, then the same field under a contact cap); a seed made
//! on another field is stale and walks; a carried refusal skips the re-try but
//! only at the limit it was refused at; insertion off is one pass.
//!
//! The comparison is [`same_run`]: the complete `Debug` of the outcome and of
//! every member (an `f64` prints its shortest round-trip form, so equal text
//! is equal bits, and `-0.0` differs from `0.0`), the route log by `==`, and
//! the ledgers once more by `to_bits`. Its sensitivity is itself tested: a
//! seed whose log or accounting is tampered with makes it report a
//! difference.

use plinth_mesh::{make_box, Mesh};
use plinth_support::perception::{slice_stack, SliceSolid, SliceStackLimits};
use plinth_sweep::down_sweep::{
    self, ContactCaps, DownSweepAccounting, DownSweepOutcome, InsertionPolicy, RunSeed,
    SizingPolicy, UnrootedColumns,
};
use plinth_sweep::grid::{plan_grid, Grid};
use plinth_sweep::layer_cache::LayerCache;
use plinth_sweep::per_event;
use plinth_sweep::{DrainKeepOut, Footprint, Landing, SeatFace, StrutKind, SweepPolicy, SweepStrut};

use crate::harness;

const PITCH_MM: f64 = 0.05;
const OFFSET_MM: f64 = 0.0125;

fn height() -> f64 {
    harness::layer_height_mm()
}

fn boxes(parts: &[([f64; 3], f64, f64, f64)]) -> Mesh {
    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for &(size, x, y, z) in parts {
        let part = make_box(size, [x + OFFSET_MM, y + OFFSET_MM, z]).expect("a positive box");
        let offset = u32::try_from(verts.len()).expect("a small fixture");
        verts.extend_from_slice(part.verts());
        tris.extend(
            part.tris()
                .iter()
                .map(|t| [t[0] + offset, t[1] + offset, t[2] + offset]),
        );
    }
    Mesh::new(verts, tris).expect("disjoint closed boxes make a valid mesh")
}

struct Setup {
    grid: Grid,
    slices: Vec<Vec<SliceSolid>>,
    sizing: SizingPolicy,
}

fn setup(mesh: &Mesh, top_z: f64, extent: f64, x_allowed: Option<f64>) -> Setup {
    let h = height();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small positive layer count"
    )]
    let layers = (top_z / h).ceil() as usize;
    #[expect(clippy::cast_precision_loss, reason = "a small layer index")]
    let planes: Vec<f64> = (0..layers).map(|layer| (layer as f64 + 0.5) * h).collect();
    let slices =
        slice_stack(mesh, &planes, SliceStackLimits::production()).expect("the fixture slices");
    let (grid, _) = plan_grid(-extent, -extent, extent, extent, layers, PITCH_MM, 1.0, u64::MAX);
    let policy = SweepPolicy::from_registry().expect("the registry");
    let sizing = SizingPolicy {
        x_allowed: x_allowed.unwrap_or(policy.x_allowed),
        tip_radius_min_mm: policy.tip_radius_min_mm,
        tip_radius_max_mm: policy.tip_radius_max_mm,
        owner_growth_ceiling_mm: policy.owner_growth_ceiling_mm,
        alpha: plinth_sweep::peel::alpha(policy.peel_field_mode),
        lamina_reach_mm: policy.kappa * h,
        straight_reach: policy.straight_reach,
        tip_column_roots: true,
        per_event: true,
        handed_column_ledger: true,
        layer_height_mm: h,
        plate_z_mm: 0.0,
    };
    Setup {
        grid,
        slices,
        sizing,
    }
}

fn cell_at(grid: &Grid, x: f64, y: f64) -> usize {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a coordinate inside the grid"
    )]
    let (col, row) = (
        grid.col_coord(x + OFFSET_MM).floor() as usize,
        grid.row_coord(y + OFFSET_MM).floor() as usize,
    );
    grid.index(row, col)
}

fn arm(cell: usize, k_top: usize, path: Vec<u32>) -> SweepStrut {
    SweepStrut {
        id: 0,
        cell,
        center_mm: [0.0, 0.0],
        kind: StrutKind::Arm,
        min_radius_mm: 0.0,
        thickened_mm: Vec::new(),
        foot_radius_mm: None,
        seat_on_strut: None,
        k_top,
        k_bottom: 0,
        landing: Landing::Plate,
        interior: false,
        radius_mm: Vec::new(),
        carried_mm2: Vec::new(),
        covered_demand: Vec::new(),
        held_births: Vec::new(),
        twin_host: false,
        inserted_by_load: false,
        path,
        straight_fallback: None,
        fallback_path: Vec::new(),
        head: None,
        seat_face: SeatFace {
            point_mm: None,
            shift_mm: [0.0; 3],
            footprint: Footprint::NotGraded,
            contact_ball_radius_mm: None,
            foot: None,
        },
        seat_normal: [0.0; 3],
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a small positive layer index"
)]
fn layer_of(z: f64) -> usize {
    (z / height()).round() as usize
}

/// One fixture: a mesh, the arm that holds it up, the cell and layer that arm
/// seats at, and the area-law limit to run it under.
struct Case {
    name: &'static str,
    setup: Setup,
    arm: SweepStrut,
    seat: usize,
    k_top: usize,
}

/// A slab on one arm, at the registered `X_ALLOWED`: the first run inserts
/// nothing; the cap makes the second place a carrier.
fn slab_case() -> Case {
    let h = height();
    let slab_bottom = 8.0 * h;
    let mesh = boxes(&[([2.0, 1.0, 3.0 * h], 0.0, 0.0, slab_bottom)]);
    let setup = setup(&mesh, slab_bottom + 4.0 * h, 3.0, None);
    let seat = cell_at(&setup.grid, 0.0, 0.0);
    let k_top = layer_of(slab_bottom);
    Case {
        name: "slab",
        arm: arm(seat, k_top, Vec::new()),
        setup,
        seat,
        k_top,
    }
}

/// A wider slab at a low `X_ALLOWED`: the FIRST run already inserts arms (so
/// its last pass is one that inserted), and the cap relieves the host further.
fn wide_slab_case() -> Case {
    let h = height();
    let slab_bottom = 8.0 * h;
    let mesh = boxes(&[([4.0, 4.0, 3.0 * h], 0.0, 0.0, slab_bottom)]);
    let setup = setup(&mesh, slab_bottom + 4.0 * h, 3.0, Some(2.0));
    let seat = cell_at(&setup.grid, 0.0, 0.0);
    let k_top = layer_of(slab_bottom);
    Case {
        name: "wide slab, low X",
        arm: arm(seat, k_top, Vec::new()),
        setup,
        seat,
        k_top,
    }
}

/// The leaned arm of `per_event_sizing`: one arm catching one event at two
/// heights, under a slab and a block, seated under the slab.
fn leaned_case() -> Case {
    let h = height();
    let block_bottom = 6.0 * h;
    let slab_bottom = block_bottom + 10.0 * h;
    let mesh = boxes(&[
        ([2.0, 0.6, 3.0 * h], 1.0, 0.0, slab_bottom),
        ([0.5, 0.6, slab_bottom - block_bottom], 1.75, 0.0, block_bottom),
    ]);
    let setup = setup(&mesh, slab_bottom + 4.0 * h, 3.0, None);
    let under_block = cell_at(&setup.grid, 1.8, 0.0);
    let under_slab = cell_at(&setup.grid, 1.0, 0.0);
    let (k_block, k_slab) = (layer_of(block_bottom), layer_of(slab_bottom));
    let path: Vec<u32> = (0..k_slab)
        .map(|layer| {
            let cell = if layer < k_block { under_block } else { under_slab };
            u32::try_from(cell).expect("a small grid")
        })
        .collect();
    Case {
        name: "leaned arm",
        arm: arm(under_slab, k_slab, path),
        setup,
        seat: under_slab,
        k_top: k_slab,
    }
}

/// A slab at a low `X_ALLOWED`, but on an arm in a cell the lamina reach
/// covers only partly: some load is lost, so the accounting holds a non-zero
/// `uncarried` term the seed must carry.
fn lossy_case() -> Case {
    let h = height();
    let slab_bottom = 10.0 * h;
    let mesh = boxes(&[([6.0, 1.2, 3.0 * h], 0.0, 0.0, slab_bottom)]);
    let setup = setup(&mesh, slab_bottom + 4.0 * h, 4.0, Some(3.0));
    let seat = cell_at(&setup.grid, 0.0, 0.0);
    let k_top = layer_of(slab_bottom);
    Case {
        name: "lossy long slab",
        arm: arm(seat, k_top, Vec::new()),
        setup,
        seat,
        k_top,
    }
}

fn cases() -> Vec<Case> {
    vec![slab_case(), wide_slab_case(), leaned_case(), lossy_case()]
}

fn call(
    case: &Case,
    struts: &mut Vec<SweepStrut>,
    insertion: InsertionPolicy,
    seed: Option<&RunSeed>,
) -> DownSweepOutcome {
    let cache = LayerCache::build(&case.setup.grid, &case.setup.slices);
    down_sweep::run_seeded(
        &cache,
        struts,
        case.setup.sizing,
        insertion,
        &DrainKeepOut::none(),
        &UnrootedColumns::none(),
        seed,
    )
}

/// The first run of a case, and the contact caps a second run would relieve:
/// the host arm's tip is recorded as having made a fifth of the contact its
/// load needs.
fn first_run(case: &Case) -> (Vec<SweepStrut>, DownSweepOutcome, ContactCaps) {
    let mut struts = vec![case.arm.clone()];
    let outcome = call(case, &mut struts, InsertionPolicy::new(1000), None);
    let tip_load = struts[0].carried_mm2.last().copied().expect("a ledger");
    let need = tip_load / case.setup.sizing.x_allowed;
    let mut caps = ContactCaps::default();
    caps.insert(case.seat, case.k_top, 0.2 * need);
    (struts, outcome, caps)
}

/// The outcome with the fields that SAY how the call ran blanked, so what is
/// left is what the call computed.
fn computed(outcome: &DownSweepOutcome) -> DownSweepOutcome {
    let mut copy = outcome.clone();
    copy.seeded_pass = false;
    copy.seed_stale = false;
    copy.refusals_carried_skipped = 0;
    copy
}

/// Equal by bits: the whole outcome, the route log and every member.
fn same_run(
    (left, left_struts): (&DownSweepOutcome, &[SweepStrut]),
    (right, right_struts): (&DownSweepOutcome, &[SweepStrut]),
) -> Result<(), String> {
    let (a, b) = (computed(left), computed(right));
    if format!("{a:?}") != format!("{b:?}") {
        let (text_a, text_b) = (format!("{a:#?}"), format!("{b:#?}"));
        let differing: Vec<String> = text_a
            .lines()
            .zip(text_b.lines())
            .filter(|(x, y)| x != y)
            .map(|(x, y)| format!("  {} -> {}", x.trim(), y.trim()))
            .take(12)
            .collect();
        return Err(format!("outcomes differ on:\n{}", differing.join("\n")));
    }
    if a.route_log != b.route_log {
        return Err("route logs differ".to_owned());
    }
    if left_struts.len() != right_struts.len() {
        return Err(format!(
            "{} members vs {}",
            left_struts.len(),
            right_struts.len()
        ));
    }
    for (index, (x, y)) in left_struts.iter().zip(right_struts).enumerate() {
        if format!("{x:?}") != format!("{y:?}") {
            let (text_x, text_y) = (format!("{x:#?}"), format!("{y:#?}"));
            let differing: Vec<String> = text_x
                .lines()
                .zip(text_y.lines())
                .filter(|(p, q)| p != q)
                .map(|(p, q)| format!("  {} -> {}", p.trim(), q.trim()))
                .take(12)
                .collect();
            return Err(format!("member {index} differs on:\n{}", differing.join("\n")));
        }
        let bits = |values: &[f64]| values.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
        if bits(&x.carried_mm2) != bits(&y.carried_mm2) || bits(&x.radius_mm) != bits(&y.radius_mm)
        {
            return Err(format!("member {index}: ledger bits differ"));
        }
    }
    Ok(())
}

#[test]
fn a_seeded_pass_one_equals_the_walked_pass_one_on_second_contact_fixtures() {
    for case in cases() {
        let (first, outcome, caps) = first_run(&case);
        let seed = outcome.seed().expect("a per-event run leaves a seed");
        assert!(seed.accounting.is_some() && seed.geometry_digest.is_some());
        let policy = InsertionPolicy::new(1000).with_contact_caps(&caps);
        let (mut seeded_struts, mut walked_struts) = (first.clone(), first.clone());
        let seeded = call(&case, &mut seeded_struts, policy, Some(&seed));
        let walked = call(&case, &mut walked_struts, policy, None);
        println!(
            "{}: first run {} arms ({} by load, {} passes); second run {} arms, {} inserted, {} \
             passes, {} refused, uncarried {:.4}",
            case.name,
            first.len(),
            outcome.arms_inserted_by_load,
            outcome.per_event.passes,
            seeded_struts.len(),
            seeded.arms_inserted_by_load,
            seeded.per_event.passes,
            seeded.refused.len(),
            seeded.uncarried_load_mm2,
        );
        assert!(seeded.seeded_pass && !seeded.seed_stale, "{}: pass 1 walked", case.name);
        assert!(!walked.seeded_pass, "{}: the control was seeded", case.name);
        // Anti-vacuity: the second run does what a second contact is for.
        assert!(
            seeded.arms_inserted_by_load > 0 && seeded_struts.len() > first.len(),
            "{}: the cap placed no carrier, so this fixture is not the second-contact topology",
            case.name
        );
        assert!(seeded.per_event.passes >= 2, "{}: one pass", case.name);
        same_run((&seeded, &seeded_struts), (&walked, &walked_struts))
            .unwrap_or_else(|why| panic!("{}: seeded != walked: {why}", case.name));
    }
}

/// Where the first run's own last pass inserted arms, the seed must still be
/// the pass 1 a fresh call on the finished field walks.
#[test]
fn a_seed_from_a_run_whose_last_pass_inserted_equals_a_walk_on_its_field() {
    let case = wide_slab_case();
    let (first, outcome, _) = first_run(&case);
    assert!(
        outcome.arms_inserted_by_load > 0,
        "anti-vacuity: the first run inserts nothing"
    );
    let mut seed = outcome.seed().expect("a seed");
    // This field still holds a trigger the first run refused. With its refusal
    // carried the seeded call skips the re-try the walk makes (the next test);
    // here the refusals are dropped so that only pass 1 is under comparison.
    seed.refused.clear();
    let policy = InsertionPolicy::new(1000);
    let (mut seeded_struts, mut walked_struts) = (first.clone(), first);
    let seeded = call(&case, &mut seeded_struts, policy, Some(&seed));
    let walked = call(&case, &mut walked_struts, policy, None);
    assert!(seeded.seeded_pass);
    same_run((&seeded, &seeded_struts), (&walked, &walked_struts)).expect("equal");
    // And the finished field is a fixed point: the call inserts nothing.
    assert_eq!(seeded.arms_inserted_by_load, 0);
}

/// The same finished field, the seed's refusals kept: the one trigger the
/// first run could not relieve is not re-tried, and nothing else differs.
#[test]
fn the_refusals_of_a_run_that_inserted_are_carried_into_a_call_on_its_field() {
    let case = wide_slab_case();
    let (first, outcome, _) = first_run(&case);
    assert!(outcome.arms_inserted_by_load > 0 && !outcome.refused.is_empty());
    let seed = outcome.seed().expect("a seed");
    let policy = InsertionPolicy::new(1000);
    let (mut seeded_struts, mut walked_struts) = (first.clone(), first);
    let seeded = call(&case, &mut seeded_struts, policy, Some(&seed));
    let walked = call(&case, &mut walked_struts, policy, None);
    assert!(seeded.refusals_carried_skipped > 0);
    assert_eq!(seeded.per_event.passes + 1, walked.per_event.passes);
    assert_eq!(seeded_struts, walked_struts);
    assert_eq!(seeded.refused, outcome.refused);
}

#[test]
fn a_seed_made_on_another_field_is_stale_and_the_call_walks() {
    let case = slab_case();
    let (first, outcome, caps) = first_run(&case);
    let policy = InsertionPolicy::new(1000).with_contact_caps(&caps);
    let seed = outcome.seed().expect("a seed");
    let mut walked_struts = first.clone();
    let walked = call(&case, &mut walked_struts, policy, None);
    let check = |label: &str, struts: &mut Vec<SweepStrut>, seed: &RunSeed| {
        let mut control = struts.clone();
        let control_outcome = call(&case, &mut control, policy, None);
        let outcome = call(&case, struts, policy, Some(seed));
        assert!(outcome.seed_stale && !outcome.seeded_pass, "{label}: the seed was used");
        assert_eq!(outcome.refusals_carried_skipped, 0, "{label}");
        same_run((&outcome, struts), (&control_outcome, &control))
            .unwrap_or_else(|why| panic!("{label}: a stale seed changed the answer: {why}"));
    };
    // The field changed under the seed: the arm lands elsewhere.
    let mut moved = first.clone();
    moved[0].landing = Landing::Model { layer: 2 };
    check("landing changed", &mut moved, &seed);
    // A member was added.
    let mut grown = first.clone();
    let mut extra = case.arm.clone();
    extra.id = 1;
    extra.cell += 1;
    grown.push(extra);
    check("member added", &mut grown, &seed);
    // A different arm cell.
    let mut shifted = first.clone();
    shifted[0].cell += 1;
    shifted[0].path.clear();
    check("cell moved", &mut shifted, &seed);
    // No accounting snapshot.
    let mut bare = seed.clone();
    bare.accounting = None;
    check("no accounting", &mut first.clone(), &bare);
    // A log of another depth.
    let mut shallow = seed.clone();
    shallow.log = per_event::RouteLog::default();
    check("empty log", &mut first.clone(), &shallow);
    // A routing input changed: the sizing the pass reads.
    let mut other_sizing = case.setup.sizing;
    other_sizing.lamina_reach_mm += 0.5;
    let cache = LayerCache::build(&case.setup.grid, &case.setup.slices);
    let mut struts = first.clone();
    let outcome = down_sweep::run_seeded(
        &cache,
        &mut struts,
        other_sizing,
        policy,
        &DrainKeepOut::none(),
        &UnrootedColumns::none(),
        Some(&seed),
    );
    assert!(outcome.seed_stale && !outcome.seeded_pass, "sizing changed");
    // A stale seed carries nothing and the fresh call was not seeded either.
    assert!(!walked.seed_stale && !walked.seeded_pass);
    // The unmodified field still takes the seed (the control of the above).
    let mut same = first;
    let used = call(&case, &mut same, policy, Some(&seed));
    assert!(used.seeded_pass && !used.seed_stale);
}

/// The comparison has teeth: a seed whose log, or whose accounting, is not the
/// field's own makes a seeded call differ from a walked one, and `same_run`
/// says so.
#[test]
fn sabotage_a_seed_with_the_wrong_log_or_accounting_is_caught_by_the_comparison() {
    let case = slab_case();
    let (first, outcome, caps) = first_run(&case);
    let policy = InsertionPolicy::new(1000).with_contact_caps(&caps);
    let good = outcome.seed().expect("a seed");
    let mut walked_struts = first.clone();
    let walked = call(&case, &mut walked_struts, policy, None);
    // The control: the good seed passes.
    let mut struts = first.clone();
    let seeded = call(&case, &mut struts, policy, Some(&good));
    same_run((&seeded, &struts), (&walked, &walked_struts)).expect("the control");

    // (1) The accounting of an empty pass, digest intact. The accounting
    // reaches the outcome only when pass 1 is the whole call, so this runs with
    // insertion off.
    let off = InsertionPolicy::disabled();
    let mut off_walked_struts = first.clone();
    let off_walked = call(&case, &mut off_walked_struts, off, None);
    let mut struts = first.clone();
    let control = call(&case, &mut struts, off, Some(&good));
    same_run((&control, &struts), (&off_walked, &off_walked_struts)).expect("the control");
    let mut flat = good.clone();
    flat.accounting = Some(DownSweepAccounting::default());
    let mut struts = first.clone();
    let seeded = call(&case, &mut struts, off, Some(&flat));
    assert!(seeded.seeded_pass, "the tampered seed must still be used");
    assert!(
        same_run((&seeded, &struts), (&off_walked, &off_walked_struts)).is_err(),
        "an accounting that is not the pass's own went unnoticed"
    );

    // (2) The log of a different field (the wide slab's), digest and
    // accounting intact: the sizing follows the wrong routing.
    // Another arm on the same mesh, out of the slab's reach: it reads other
    // cells, so the sizing built on its log is another sizing.
    let mut other = slab_case();
    other.seat = cell_at(&other.setup.grid, 2.5, 0.0);
    other.arm = arm(other.seat, other.k_top, Vec::new());
    let (_, other_outcome, _) = first_run(&other);
    let mut wrong = good.clone();
    wrong.log = other_outcome.seed().expect("a seed").log;
    let mut struts = first.clone();
    let seeded = call(&case, &mut struts, policy, Some(&wrong));
    assert!(seeded.seeded_pass, "the tampered seed must still be used");
    assert!(
        same_run((&seeded, &struts), (&walked, &walked_struts)).is_err(),
        "a log that is not the pass's own went unnoticed"
    );
}

/// A field whose only trigger the geometry cannot relieve (the strut cap is
/// already reached): the first run refuses it and says so.
fn refusing_policy() -> InsertionPolicy<'static> {
    InsertionPolicy::new(1)
}

#[test]
fn a_carried_refusal_skips_the_retry_and_is_counted() {
    let case = wide_slab_case();
    let policy = refusing_policy();
    let mut first = vec![case.arm.clone()];
    let outcome = call(&case, &mut first, policy, None);
    assert!(
        !outcome.refused.is_empty() && outcome.insertion_geometry_exhausted > 0,
        "anti-vacuity: the first run refused nothing"
    );
    assert_eq!(first.len(), 1, "the cap held");
    let seed = outcome.seed().expect("a seed");
    assert_eq!(seed.refused, outcome.refused);

    let (mut seeded_struts, mut walked_struts) = (first.clone(), first);
    let seeded = call(&case, &mut seeded_struts, policy, Some(&seed));
    let walked = call(&case, &mut walked_struts, policy, None);
    // The walk tries the trigger again (pass 2) and is refused again; the
    // seeded call does not try.
    assert!(walked.per_event.passes >= 2 && walked.insertion_geometry_exhausted > 0);
    assert_eq!(seeded.per_event.passes, 1, "the carried refusal was re-tried");
    assert_eq!(seeded.insertion_geometry_exhausted, 0);
    assert!(seeded.refusals_carried_skipped > 0 && walked.refusals_carried_skipped == 0);
    // The answer is the same: the same members, the same ledgers, the same
    // refusals handed on.
    assert_eq!(seeded_struts, walked_struts);
    assert_eq!(seeded.refused, walked.refused);
    assert_eq!(seeded.refused, outcome.refused);
}

#[test]
fn a_refusal_is_carried_only_against_the_limit_it_was_refused_at() {
    let case = wide_slab_case();
    let mut first = vec![case.arm.clone()];
    let outcome = call(&case, &mut first, refusing_policy(), None);
    assert!(!outcome.refused.is_empty());
    let seed = outcome.seed().expect("a seed");
    // The second run lowers the host tip's limit with a contact cap and may
    // place arms: the refusal at the area law's limit says nothing about it.
    let tip_load = first[0].carried_mm2.last().copied().expect("a ledger");
    let mut caps = ContactCaps::default();
    caps.insert(case.seat, case.k_top, 0.2 * tip_load / case.setup.sizing.x_allowed);
    let policy = InsertionPolicy::new(1000).with_contact_caps(&caps);
    let (mut seeded_struts, mut walked_struts) = (first.clone(), first);
    let seeded = call(&case, &mut seeded_struts, policy, Some(&seed));
    let walked = call(&case, &mut walked_struts, policy, None);
    assert!(seeded.seeded_pass);
    assert!(walked.arms_inserted_by_load > 0, "anti-vacuity: nothing to retry");
    assert_eq!(
        seeded.refusals_carried_skipped, 0,
        "a refusal at another limit was carried"
    );
    same_run((&seeded, &seeded_struts), (&walked, &walked_struts)).expect("equal to a walk");
}

#[test]
fn insertion_off_is_exactly_one_pass_and_inserts_nothing() {
    for case in cases() {
        let (first, outcome, _) = first_run(&case);
        let seed = outcome.seed().expect("a seed");
        let (mut seeded_struts, mut walked_struts) = (first.clone(), first.clone());
        let mut seeded = call(&case, &mut seeded_struts, InsertionPolicy::disabled(), Some(&seed));
        let walked = call(&case, &mut walked_struts, InsertionPolicy::disabled(), None);
        assert!(seeded.seeded_pass, "{}: not seeded", case.name);
        // A call that tries nothing learns no refusal and forgets none: the
        // seed's pass on to the next call. (The walk has nothing to hand on.)
        assert_eq!(seeded.refused, outcome.refused, "{}", case.name);
        assert!(walked.refused.is_empty(), "{}", case.name);
        seeded.refused.clear();
        assert_eq!(seeded.per_event.passes, 1, "{}", case.name);
        assert_eq!(seeded.arms_inserted_by_load, 0, "{}", case.name);
        assert!(seeded.inserted_by.is_empty(), "{}", case.name);
        assert_eq!(seeded_struts.len(), first.len(), "{}", case.name);
        same_run((&seeded, &seeded_struts), (&walked, &walked_struts))
            .unwrap_or_else(|why| panic!("{}: seeded != walked: {why}", case.name));
    }
}

#[test]
fn each_inserted_arm_names_the_trigger_that_placed_it() {
    let case = slab_case();
    let (first, outcome, caps) = first_run(&case);
    assert!(outcome.inserted_by.is_empty(), "the first run inserted: {:?}", outcome.inserted_by);
    let policy = InsertionPolicy::new(1000).with_contact_caps(&caps);
    let mut struts = first.clone();
    let seeded = call(&case, &mut struts, policy, outcome.seed().as_ref());
    assert!(seeded.arms_inserted_by_load > 0);
    assert_eq!(seeded.inserted_by.len() as u64, seeded.arms_inserted_by_load);
    for (position, &(id, key)) in seeded.inserted_by.iter().enumerate() {
        assert_eq!(id as usize, first.len() + position, "ids are consecutive from the old tail");
        assert!(struts[id as usize].inserted_by_load);
        // `per_event::entry(member, layer)`: the member in the high word.
        let (host, layer) = (key >> 32, key & 0xffff_ffff);
        assert_eq!(host, 0, "the cap host is arm 0");
        assert_eq!(layer + 1, case.k_top as u64, "the trigger is the tip's own layer");
    }
}

#[test]
fn a_seeded_call_leaves_a_seed_only_when_its_last_pass_placed_nothing() {
    // The chain the second contact runs: first run -> seeded run -> a third
    // run seeded by the second. A pass that placed arms read its growing arms
    // first, so its log is not the pass 1 of a walk on the field it left (the
    // same reads, in another order, which the per-event sizing sees at a
    // shared cell): such a run leaves no accounting snapshot and the third
    // call walks. A run whose last pass placed nothing leaves a seed its own
    // field accepts. Either way the third call equals a walk.
    let (mut usable, mut unusable) = (0, 0);
    for case in cases() {
        let (first, outcome, caps) = first_run(&case);
        let policy = InsertionPolicy::new(1000).with_contact_caps(&caps);
        let mut second_struts = first;
        let second = call(&case, &mut second_struts, policy, outcome.seed().as_ref());
        assert!(second.seeded_pass, "{}", case.name);
        let mut seed = second.seed().expect("a seed");
        // Refusals are compared in the tests of their own.
        seed.refused.clear();
        let mut third_struts = second_struts.clone();
        let third = call(&case, &mut third_struts, policy, Some(&seed));
        let mut walked_struts = second_struts.clone();
        let walked = call(&case, &mut walked_struts, policy, None);
        assert_eq!(third.seeded_pass, seed.accounting.is_some(), "{}", case.name);
        assert_eq!(third.seed_stale, seed.accounting.is_none(), "{}", case.name);
        if third.seeded_pass {
            usable += 1;
        } else {
            unusable += 1;
        }
        same_run((&third, &third_struts), (&walked, &walked_struts))
            .unwrap_or_else(|why| panic!("{}: seeded != walked: {why}", case.name));
    }
    assert!(
        usable > 0 && unusable > 0,
        "the fixtures must show both kinds of run: {usable} usable, {unusable} not"
    );
}
