//! **The drain must still drain once the supports are on it.**
//!
//! The owner opened the emitted Sphere in ChituBox on 2026-09-03 and found the
//! hollow model's drain outlet "completely covered by a clutter of thick
//! overlapping supports at the bottom". Placement had never been told the bore
//! exists.
//!
//! Three assertions, and the middle one is the load-bearing one:
//!
//! 1. A drain that opens into free air reads CONNECTED. Without this the whole
//!    check could be a constant `false`.
//! 2. **Sabotage.** Delete the keep-out — the one thing this lane added to
//!    placement — and the same flat-underside fixture's drain reads ENCLOSED.
//!    This is what proves the keep-out is what holds the mouth open, rather
//!    than the fixture never having been in danger.
//! 3. With the keep-out in force, the mouth is measurably open — and where the
//!    resin still cannot cross the arm field, that shortfall is a published
//!    WARNING on an emitted file rather than a silence.
//!
//! The gap-width invariant (a flood must not squeeze through a passage
//! narrower than the bore diameter) is asserted one level down, on the field
//! itself, in `plinth_hollow::passage`'s own tests, because that is where the
//! threshold lives.

use crate::harness;

use plinth_sweep::{DrainSpec, Landing, StrutKind, SweepPolicy};

/// The fixture: a table whose whole underside is flat and stands WELL clear of
/// the plate, so the up-sweep covers that underside with a hex lattice of arms
/// at the registered reach spacing — the geometry that walls a mouth in — and
/// there is genuine vertical room under it for a bore-wide passage.
///
/// The room matters and is not incidental. Under a slab resting at the
/// registered LIFT alone there is 2.500 mm of gap, and a 1.250 mm bore needs
/// 3.000 mm of passage; NO support field can make that drain, which would make
/// a slab fixture unwinnable and the keep-out untestable on it. The table top
/// is 12 mm up.
fn flat_underside_slab() -> plinth_mesh::Mesh {
    plinth_mesh::make_overhang_table_with([3.0, 3.0, 12.0], [30.0, 30.0, 4.0], 0.0)
        .expect("a table is a valid mesh")
}

/// The registered bore. Read rather than restated, so this test cannot drift
/// from the drill stage.
fn bore_radius_mm() -> f64 {
    plinth_core::m0_constants()
        .expect("registry")
        .placeholders()
        .hollow_drain_r_mm()
        .diagnostic()
        .get()
}

/// **The bore this fixture has to be adversarial against, derived from the
/// registered pitch — rebuilt 2026-09-06.**
///
/// The two sabotage tests below assert that WITHOUT the keep-out the plain hex
/// field walls the mouth in. At the registered bore of 1.250 mm that premise
/// stopped being true when the descent learned to land part-way down a
/// neighbour's shaft: pulling arms off the plate and into trunks opens lanes
/// through the field by itself, and the plain field measured a best clearance
/// of 1.5811 mm against the 1.5000 mm that bore requires. A premise that is
/// false makes the tests below prove nothing, so the fixture is rebuilt rather
/// than the assertion loosened.
///
/// The bore is not picked to fail; it is DERIVED so that no hex field at the
/// registered pitch can pass it, whatever the landing rule does:
///
/// * the lattice is pitched at `√3 · ρ` (`docs/physics.md`:128-135), so the
///   widest free half-gap between two neighbouring arms is `√3·ρ/2` less the
///   arms' own radius;
/// * every arm carries at least the registered minimum tip radius, so that
///   half-gap is at most `√3·ρ/2 − r_min` for ANY field this engine places;
/// * the escape check calls a passage wide enough when its free radius reaches
///   `r_bore + ½·voxel pitch` (`plinth_hollow::flow`).
///
/// Taking `r_bore = √3·ρ/2 − r_min` therefore requires MORE clearance than the
/// lattice can ever leave, by exactly half a voxel plus whatever section the
/// arms carry above the minimum. The keep-out, which clears a cylinder around
/// the bore itself, is unaffected by the choice — which is the whole point.
fn adversarial_bore_radius_mm() -> f64 {
    let policy = SweepPolicy::from_registry().expect("registry");
    let lattice_pitch_mm = 3.0_f64.sqrt() * harness::lamina_reach_mm();
    lattice_pitch_mm / 2.0 - policy.tip_radius_min_mm
}

/// A drain through the middle of the flat underside, pointing straight down —
/// the Sphere's own case, with the sphere's curvature taken out so the fixture
/// tests the support field and nothing else.
fn downward_drain() -> DrainSpec {
    DrainSpec::from_bore([0.0, 0.0, 14.0], [0.0, 0.0, 12.0], adversarial_bore_radius_mm())
}

#[test]
fn a_drain_opening_into_free_air_is_connected() {
    // Out of the side of the slab, half way up, pointing away from the model:
    // nothing but air in that direction, and no arm is anywhere near it.
    let radius = bore_radius_mm();
    let sideways = DrainSpec::from_bore([13.0, 0.0, 14.0], [15.0, 0.0, 14.0], radius);
    let policy = SweepPolicy::from_registry().expect("registry");
    let plan = harness::sweep_with_drains(&flat_underside_slab(), &policy, &[sideways]);
    let escape = &plan.report.escape;
    assert_eq!(escape.verdicts.len(), 1, "one drain in, one verdict out");
    assert!(
        escape.verdicts[0].probed_voxels > 0,
        "the probe must have looked at something: {:?}",
        escape.verdicts[0]
    );
    assert!(
        escape.verdicts[0].connected,
        "a drain opening sideways into free air must read connected: {:?}",
        escape.verdicts[0]
    );
}

/// SABOTAGE. `without_drain_keep_out` deletes the placement change this lane
/// made and leaves everything else — the same mesh, the same pose, the same
/// reach, the same area law, the same escape check. If the field still reads
/// connected, the keep-out is decorative.
#[test]
fn deleting_the_keep_out_walls_the_outlet_in() {
    let drain = downward_drain();
    // Graded at the X this fixture was built at (12,
    // `harness::PATH_FIXTURE_X_ALLOWED`). At the ruled 18 the thinner arms no
    // longer wall this mouth in even WITHOUT the keep-out (connected, best
    // clearance 2.550 mm against 2.045 required; measured 2026-09-30), so the
    // fixture stops being adversarial. The keep-out reads no X.
    let sabotaged =
        harness::at_path_fixture_x(SweepPolicy::from_registry().expect("registry"))
            .without_drain_keep_out()
            // The graze conversion (2026-10-08) roots grazing tips on the plate
            // on both sides of the comparison below; the claim is about the
            // descent's landings.
            .without_graze_heads();
    let plan = harness::sweep_with_drains(&flat_underside_slab(), &sabotaged, &[drain]);
    let escape = &plan.report.escape;
    assert_eq!(
        plan.report.drain_keep_out_cells, 0,
        "the sabotage must actually have removed the keep-out"
    );
    assert_eq!(
        plan.report.drain_keep_out_refusals, 0,
        "a deleted keep-out cannot refuse anything"
    );
    assert!(
        !plan.struts.is_empty(),
        "the fixture must actually get a support field, or the test proves nothing"
    );
    assert_eq!(escape.verdicts.len(), 1);
    assert!(
        escape.verdicts[0].probed_voxels > 0,
        "the probe must have looked at something: {:?}",
        escape.verdicts[0]
    );
    assert!(
        !escape.verdicts[0].connected,
        "with the keep-out deleted the hex field must close this mouth; if it does not, the \
         fixture is not adversarial and the test below proves nothing: {:?}",
        escape.verdicts[0]
    );

    // ---- AND A NEW POSITIVE CLAIM, MEASURED HERE 2026-09-06 ---------------
    //
    // The premise above had to be rebuilt because MERGING opens the field by
    // itself: an arm that lands on a neighbour's shaft is an arm that no
    // longer stands between the mouth and the outside. That is a real effect
    // and worth a claim of its own rather than only a reason to re-derive the
    // bore, so the same plain field is run with the landing rule switched off
    // and the two clearances compared. It is the only place in this suite that
    // grades the landing rule as a DRAINAGE mechanism.
    // The slenderness lane is deleted on BOTH sides (2026-09-06): with it on,
    // switching shaft landings off leaves more members over their own limit and
    // the lane answers with braces, which land on shafts by construction and
    // would make this knob look broken. The claim here is about the DESCENT.
    let straight = harness::sweep_with_drains(
        &flat_underside_slab(),
        // The clearance re-route is off too (2026-10-04, fix/drawn-solid-clearance
        // R2.2): it re-lands a post the sizing grew into a wall onto the shaft
        // beside it (measured: 2 members on shafts), and its candidates do not
        // read the shaft-landing knob; the claim here is about the DESCENT.
        &sabotaged
            .without_shaft_landings()
            .without_slenderness()
            .without_clearance_reroute(),
        &[drain],
    );
    let open = escape.verdicts[0].best_clearance_mm;
    let closed = straight.report.escape.verdicts[0].best_clearance_mm;
    println!(
        "plain field, no keep-out: best clearance {open:.4} mm with shaft landings, {closed:.4} \
         mm without ({} arms against {})",
        plan.struts.len(),
        straight.struts.len(),
    );
    assert_eq!(
        straight.report.landings.strut, 0,
        "the knob must actually put the engine back on the straight descent"
    );
    assert!(
        open > closed,
        "pulling arms off the plate and into trunks must widen the way out, not narrow it: \
         {open} mm with shaft landings against {closed} mm without"
    );
}

/// The keep-out's own claim, and ONLY its own claim: the mouth is clear.
///
/// It does not claim the drain then vents. Under an overhang the resin still
/// has to cross the arm field sideways, and at the registered spacing arms `s`
/// apart with radius `a` leave a free radius of `s/2 - a` — below what the bore
/// needs. Clearing a channel through that field was built and measured on this
/// branch and cost the Sphere 37 of its 139 arms and 3199 mm² of residual
/// demand, so it is not shipped (see `drain::DrainKeepOut::build`). What this
/// asserts is the part that IS true and IS the owner's stated requirement — the
/// outlet is not walled in "to a limited distance" — plus the fact that the
/// remaining shortfall is measured and published rather than hidden.
#[test]
fn the_keep_out_clears_the_mouth_and_the_rest_is_reported() {
    let drain = downward_drain();
    // At the X this fixture was built at (12): see
    // `deleting_the_keep_out_walls_the_outlet_in` for why 18 is not adversarial.
    let policy = harness::at_path_fixture_x(SweepPolicy::from_registry().expect("registry"));
    let mesh = flat_underside_slab();
    let kept = harness::sweep_with_drains(&mesh, &policy, &[drain]);
    let sabotaged = harness::sweep_with_drains(
        &mesh,
        &policy.without_drain_keep_out(),
        &[drain],
    );
    assert!(
        kept.report.drain_keep_out_cells > 0,
        "the keep-out must cover working cells on a model that has a drain"
    );
    assert!(
        kept.report.drain_keep_out_refusals > 0,
        "the keep-out must actually have refused candidates -- otherwise this run and the \
         sabotage run differ in nothing but a flag"
    );
    let with = kept.report.escape.verdicts[0];
    let without = sabotaged.report.escape.verdicts[0];
    // THE PREMISE, stated as the verdict itself rather than as a proxy for it
    // (2026-09-06). This used to compare `best_clearance` against
    // `required_clearance`, which is the widest free radius ANY probe around
    // the outlet reads — including probes that lead nowhere. With the derived
    // bore the plain field leaves a 2.0616 mm pocket at the mouth that does
    // not reach the outside at all: walled in, with a clearance number above
    // the requirement. `connected` is the quantity the owner cares about and
    // the one the keep-out exists to change, so it is what the premise is
    // asserted on.
    assert!(
        !without.connected,
        "without the keep-out the mouth must be walled in, or this fixture proves nothing: \
         {without:?}"
    );
    assert!(
        with.best_clearance_mm >= with.required_clearance_mm,
        "with the keep-out the mouth must be at least a bore wide: {with:?}"
    );
    assert!(
        with.best_clearance_mm > without.best_clearance_mm,
        "the keep-out must MEASURABLY open the mouth: {with:?} against {without:?}"
    );
    // And the part it does not fix is a published warning, not a silence.
    assert!(
        !with.connected,
        "this fixture is the un-fixed case on purpose; if the field now vents, the arm-field \
         crossing has changed and this test's premise needs re-measuring: {with:?}"
    );
    assert!(
        kept.report
            .render_disclosure()
            .contains("ENCLOSED"),
        "an enclosed drain must be named in the disclosure the emitted file carries"
    );
}

/// A model with no drains must be untouched: no keep-out, no verdicts, and the
/// same field the sweep placed before this lane existed.
#[test]
fn a_model_with_no_drains_is_placed_exactly_as_before() {
    let policy = SweepPolicy::from_registry().expect("registry");
    let mesh = flat_underside_slab();
    let with_none = harness::sweep_with_drains(&mesh, &policy, &[]);
    let plain = harness::sweep_with(&mesh, 0.0, &policy);
    assert_eq!(with_none.report.drain_keep_out_cells, 0);
    assert!(with_none.report.escape.verdicts.is_empty());
    assert_eq!(
        with_none.report.sweep_digest, plain.report.sweep_digest,
        "an empty drain list must not move a single arm"
    );
    assert_eq!(with_none.struts.len(), plain.struts.len());
}

/// Not an assertion — a printout, so the two failures above can be diagnosed
/// from measurements instead of guesses.
#[test]
#[ignore = "diagnostic printout, run explicitly"]
fn probe() {
    let policy = SweepPolicy::from_registry().expect("registry");
    let mesh = flat_underside_slab();
    let (lo, hi) = harness::z_span(&mesh);
    println!("mesh z span {lo} .. {hi}");
    let (mut x0, mut x1, mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for v in mesh.verts() { x0 = x0.min(v[0]); x1 = x1.max(v[0]); y0 = y0.min(v[1]); y1 = y1.max(v[1]); }
    println!("mesh x {x0}..{x1} y {y0}..{y1}");
    let plan = harness::sweep_with_drains(
        &mesh,
        &policy.without_drain_keep_out(),
        &[downward_drain(), DrainSpec::from_bore([13.0, 0.0, 14.0], [15.0, 0.0, 14.0], 1.25)],
    );
    let e = &plan.report.escape;
    println!(
        "arms {} voxels {} solid {} passable {} escaping {} pitch {}",
        plan.struts.len(), e.voxels, e.solid_voxels, e.passable_voxels, e.escaping_voxels,
        e.voxel_pitch_mm
    );
    for v in &e.verdicts {
        println!("{v:?}");
    }
    let mut radii: Vec<f64> = plan.struts.iter().map(plinth_sweep::SweepStrut::tip_radius_mm).collect();
    radii.sort_by(f64::total_cmp);
    println!("tip radii min {:?} med {:?} max {:?}", radii.first(), radii.get(radii.len()/2), radii.last());
}

/// **Demand over a drain is seated from the keep-out's rim (2026-10-01).**
///
/// The keep-out is `bore + widest arm + standoff` in radius: 2.85 mm at the
/// registered 1.25 mm bore, against a lamina reach of 2.13 mm. A seat may only
/// stand on demand, so once the ring just outside the keep-out is rooted by
/// the layer's first seats, the cells inside it that a seat on that ring would
/// reach have no seat left and were reported BARE. That is what lost the
/// Cylinder's ceiling over its drain (`plinth-evidence/slice-sweep-2026-09-02/
/// field-2026-09-28/lanes/creep-45.md`). The rule graded here
/// (`up_sweep::stranded_seat_for`): such a pivot may seat on a rooted underside
/// cell outside the cylinder that reaches it.
///
/// Hand-derived before the run: from the rim, a seat reaches `ρ − p` inward
/// (the candidate disc is one pitch inside the reach), so the disc that NO
/// legal seat can reach has radius `R_k − (ρ − p)`, about 0.77 mm. That disc
/// stays bare on the underside layer, and its columns stay unrooted up through
/// the 4 mm top, so the residual is at most that disc (one pitch of raster
/// slack on its radius) times the top's layers.
#[test]
fn demand_over_a_drain_is_seated_from_the_keep_out_rim() {
    let policy = SweepPolicy::from_registry().expect("registry");
    let drain = DrainSpec::from_bore([0.0, 0.0, 14.0], [0.0, 0.0, 12.0], bore_radius_mm());
    let mesh = flat_underside_slab();
    let seated = harness::sweep_with_drains(&mesh, &policy, &[drain]);
    let stranded = harness::sweep_with_drains(&mesh, &policy.without_stranded_seating(), &[drain]);

    let pitch = seated.grid.pitch_mm();
    let cell_area = pitch * pitch;
    // The keep-out is one disc (a single vertical drain), so its radius is
    // read off its own cell count rather than restated from the registry.
    let keep_out_radius =
        (seated.report.drain_keep_out_cells as f64 * cell_area / core::f64::consts::PI).sqrt();
    let seat_reach = harness::lamina_reach_mm() - pitch;
    let bare_radius = (keep_out_radius - seat_reach).max(0.0) + pitch;
    let top_layers = (4.0 / harness::layer_height_mm()).ceil() + 1.0;
    let bound_cells = core::f64::consts::PI * bare_radius * bare_radius / cell_area * top_layers;
    println!(
        "keep-out radius {keep_out_radius:.3} mm, seat reach {seat_reach:.3} mm, unreachable disc \
         radius <= {bare_radius:.3} mm over {top_layers} layers: bound {bound_cells:.0} cells; \
         residual with stranded seating {} cells ({} stranded seats, {} arms, uncarried {:.4} \
         mm2), without {} cells ({} arms, uncarried {:.4} mm2)",
        seated.report.residual_demand_cells,
        seated.report.counters.stranded_seats,
        seated.struts.len(),
        seated.report.uncarried_load_mm2,
        stranded.report.residual_demand_cells,
        stranded.struts.len(),
        stranded.report.uncarried_load_mm2,
    );
    assert!(
        keep_out_radius > harness::lamina_reach_mm(),
        "the premise: the keep-out must be wider than one reach, or nothing is stranded"
    );
    assert!(
        seated.report.counters.stranded_seats > 0,
        "the rule never seated anything, so this test cannot see it"
    );
    assert_eq!(
        stranded.report.counters.stranded_seats, 0,
        "the knob must switch the rule off"
    );
    assert!(
        stranded.report.residual_demand_cells > seated.report.residual_demand_cells,
        "sabotage: without the rule the drain's ceiling must lose cells a legal seat reaches"
    );
    assert!(
        (seated.report.residual_demand_cells as f64) <= bound_cells,
        "the residual over the drain must be only the disc no legal seat reaches"
    );
    assert!(
        seated.report.uncarried_load_mm2 <= stranded.report.uncarried_load_mm2,
        "seating stranded demand must not lose load"
    );
}

/// The laid tube with a bore down through the middle of its bottom strip:
/// the Cylinder's production pose (`for-craig-v11`: laid on its side, the
/// drain pointing down), with the hollow cavity taken out so the fixture
/// tests the support field and nothing else. A laid prism's underside WIDENS
/// layer by layer, so the keep-out's demand comes back on every layer of the
/// first few, which is the mechanism behind the Cylinder's drain ring.
fn tube_drain() -> DrainSpec {
    // The tube's flat bottom strip stands at the foot height, 3 mm; the bore
    // runs from 0.5 mm inside it to 0.5 mm below it, pointing down.
    DrainSpec::from_bore([0.0, 0.0, 3.5], [0.0, 0.0, 2.5], bore_radius_mm())
}

/// **A stranded contact is a tip, not a new root (2026-10-02).**
///
/// The Cylinder in `for-craig-v11`: 14 stranded seats on its bottom, each on
/// the keep-out rim one layer above the last and 0.54-1.1 mm from it, each a
/// new plate post; 14 of the 16 overlapping root pairs
/// (`v11-feedback-diagnosis.md` item 1). The rule graded here
/// (`stranded_branch.rs`): a stranded-seat member whose plate root disc laps a
/// standing root lands on that root's member as a branch, lean law and path
/// test permitting, and is put back if the trunk would overload.
///
/// Graded on the laid tube with a downward drain, the production mechanism
/// (see [`tube_drain`]), against the same field with the rule switched off:
/// * premise: the field without the rule HAS stranded members standing on a
///   lapped root, or nothing here can see the rule;
/// * the rule: FEWER stranded members stand on a lapped root on the finished
///   field, and at least one branch was kept (the path-hit witness);
/// * no silent survivor: every stranded member still on a lapped root when the
///   rule finished was judged in its last round and refused for a logged
///   reason (something resting on it that cannot move with it, or no landing
///   the lean law and the path test admit). Measured on this fixture at
///   develop `469d482`, the rule running after the congestion lane: 7 when
///   the rule began (the lane had leaned the leaves out), 2 when it finished,
///   6 on the finished field against 9 without the rule (the slenderness
///   merges and lip seats grow discs back into a lap); it is a WARNING on the
///   file, never a refusal;
/// * the census the owner sees: no more overlapping root pairs and no more
///   cells under three or more roots than without the rule, on the FINISHED
///   field (measured: 6 pairs against 8; with the rule run before the
///   congestion lane it was 9 against 8, the integration failure that moved
///   it);
/// * nothing paid for it: no more uncarried load, no more overloaded members,
///   every member still rooted.
#[test]
fn a_stranded_seat_lands_on_the_root_it_laps_instead_of_standing_beside_it() {
    let policy = SweepPolicy::from_registry().expect("registry");
    let mesh = plinth_mesh::make_laid_tube().expect("a tube is a valid mesh");
    let drain = tube_drain();
    let branched = harness::sweep_with_drains(&mesh, &policy, &[drain]);
    let standing = harness::sweep_with_drains(&mesh, &policy.without_stranded_branch(), &[drain]);
    let (b, s) = (&branched.report, &standing.report);
    println!(
        "stranded seats {} / {}; members {} / {}; lapping before {} / {}, after {} / {}, finished \
         {:?} / {:?}; branches made {} kept {} put back {} refused carrying {} no landing {}; \
         priced {} blocked {}; overlap pairs {} / {}; cells under 3+ roots {} / {}; overloaded \
         {} / {}; uncarried {:.4} / {:.4} mm2; unrooted {} / {}",
        b.counters.stranded_seats,
        s.counters.stranded_seats,
        b.stranded_branch.stranded_members,
        s.stranded_branch.stranded_members,
        b.stranded_branch.lapping_before,
        s.stranded_branch.lapping_before,
        b.stranded_branch.lapping_after,
        s.stranded_branch.lapping_after,
        b.stranded_branch.lapping_ids_on_finished_field,
        s.stranded_branch.lapping_ids_on_finished_field,
        b.stranded_branch.branches_made,
        b.stranded_branch.branches_kept,
        b.stranded_branch.put_back_overloaded,
        b.stranded_branch.refused_carrying,
        b.stranded_branch.refused_no_landing,
        b.stranded_branch.landings_priced,
        b.stranded_branch.paths_blocked,
        b.congestion_lane.after.all.overlap_pairs,
        s.congestion_lane.after.all.overlap_pairs,
        b.congestion_lane.after.all.stack_max,
        s.congestion_lane.after.all.stack_max,
        b.overloaded_struts.len(),
        s.overloaded_struts.len(),
        b.uncarried_load_mm2,
        s.uncarried_load_mm2,
        b.landing_check.finished.unrooted.len(),
        s.landing_check.finished.unrooted.len(),
    );
    println!(
        "after each later lane: {:?}; branches {:?}",
        b.stranded_branch.lapping_by_lane, b.stranded_branch.branches
    );
    // The overlapping root pairs at each stage, with / without the rule:
    // entering the congestion lane (what this rule left), leaving it, and on
    // the finished field.
    println!(
        "overlap pairs: congestion entry {} / {}, lane exit {} / {}, finished {} / {}; congestion \
         rounds {} / {}, arms leaned {} / {}, flank arms {} / {}",
        b.congestion_lane.before.all.overlap_pairs,
        s.congestion_lane.before.all.overlap_pairs,
        b.congestion_lane.at_lane_exit.all.overlap_pairs,
        s.congestion_lane.at_lane_exit.all.overlap_pairs,
        b.congestion_lane.after.all.overlap_pairs,
        s.congestion_lane.after.all.overlap_pairs,
        b.congestion_lane.rounds_used,
        s.congestion_lane.rounds_used,
        b.congestion_lane.arms_leaned,
        s.congestion_lane.arms_leaned,
        b.congestion_lane.arms_inserted,
        s.congestion_lane.arms_inserted,
    );
    for decision in &b.stranded_branch.decisions {
        println!("decision {decision:?}");
    }
    // What rests on each member still lapping: the rule refuses to move a
    // member something else stands on.
    for &id in &b.stranded_branch.lapping_ids_on_finished_field {
        let member = &branched.struts[id as usize];
        let resting: Vec<(u32, StrutKind, usize, Landing)> = branched
            .struts
            .iter()
            .filter(|other| {
                matches!(other.landing, Landing::Strut { id: parent, .. } if parent == id)
                    || other.seat_on_strut == Some(id)
            })
            .map(|other| (other.id, other.kind, other.k_top, other.landing))
            .collect();
        println!(
            "lapping member {id}: seat layer {} landing {:?} at ({:.3}, {:.3}); resting on it {:?}",
            member.k_top, member.landing, member.center_mm[0], member.center_mm[1], resting
        );
    }
    assert!(
        s.counters.stranded_seats > 0 && s.stranded_branch.lapping_on_finished_field > 0,
        "premise: without the rule a stranded-seat member must stand on a lapped root, or this \
         fixture cannot see the rule"
    );
    assert!(
        !s.stranded_branch.enabled && s.stranded_branch.branches_made == 0,
        "the knob must switch the rule off"
    );
    assert!(
        b.stranded_branch.lapping_on_finished_field < s.stranded_branch.lapping_on_finished_field,
        "the rule must leave fewer stranded-seat members on a lapped root: {:?} against {:?}",
        b.stranded_branch.lapping_ids_on_finished_field,
        s.stranded_branch.lapping_ids_on_finished_field
    );
    assert!(
        b.stranded_branch.branches_kept > 0,
        "path hit: the rule must have landed at least one stranded member as a branch"
    );
    let last_round = b
        .stranded_branch
        .decisions
        .iter()
        .map(|decision| decision.round)
        .max()
        .unwrap_or(0);
    for &id in &b.stranded_branch.lapping_ids_after {
        assert!(
            b.stranded_branch.decisions.iter().any(|decision| decision.round == last_round
                && decision.member == id
                && decision.outcome != "branch"),
            "member {id} was left on a lapped root without a logged refusal in the last round"
        );
    }
    assert!(
        b.congestion_lane.after.all.overlap_pairs <= s.congestion_lane.after.all.overlap_pairs,
        "the rule must not leave more overlapping root pairs than standing posts did"
    );
    assert!(
        b.congestion_lane.after.all.stack_max <= s.congestion_lane.after.all.stack_max,
        "the rule must not leave more cells under three or more roots"
    );
    assert!(
        b.overloaded_struts.len() <= s.overloaded_struts.len(),
        "a branch must never overload its trunk"
    );
    assert!(
        b.uncarried_load_mm2 <= s.uncarried_load_mm2 + 1.0e-9 * s.uncarried_load_mm2.max(1.0),
        "a branch must not lose load"
    );
    assert!(b.landing_check.finished.unrooted.is_empty(), "every member must stay rooted");
}

/// The diagnosis's unit form on the flat drain slab (`v11-feedback-diagnosis.md`
/// item 1): with stranded seating on, the field may not hold more overlapping
/// root pairs than with stranded seating off. Measured before the rule: 2
/// against 0 (two stranded seats standing on a lapped root), so the slab DOES
/// show the defect on one layer.
///
/// RE-PINNED PREMISE (2026-10-02, after merging develop `469d482`): the
/// stranded-seat rule now runs AFTER the congestion lane (see
/// `stranded_branch.rs`, "Why after the congestion lane"). On this slab the
/// lane leans both lapping stranded posts out to clear plate, because they are
/// leaves, so the rule finds nothing left to branch (`lapping_before` 0, which
/// is the census when the rule begins). The defect's premise is therefore read
/// where stranded seating leaves the field, at the congestion lane's ENTRY:
/// more overlapping pairs with stranded seating than without. What is graded
/// is unchanged: the congestion lane's exit, the rule's own exit
/// (`lapping_after`), and every later lane up to the ties. With nothing to
/// branch the rule does not touch the field, so the finished slab is
/// develop's own (measured at `469d482`: 4 overlapping pairs with stranded
/// seating against 0 without, all grown by the tie lane, as below).
///
/// Graded where the placement lanes end: at the congestion lane's exit, and
/// through every later lane up to the ties. The ties are not graded here, on
/// purpose and as a measurement: on this slab the tie lane's cross raises grow
/// two root discs back into a lap (`lapping_by_lane`: 0 through the lip seats
/// and head joins, 2 after "slenderness ties"). That is the tie lane growing a
/// disc it already counts (`cross_roots_lapping`), not a stranded seat standing
/// a root, and it is reported as an open question in the lane report rather
/// than pinned here.
#[test]
fn stranded_seating_adds_no_overlapping_root_on_the_flat_slab() {
    let policy = SweepPolicy::from_registry().expect("registry");
    let drain = DrainSpec::from_bore([0.0, 0.0, 14.0], [0.0, 0.0, 12.0], bore_radius_mm());
    let mesh = flat_underside_slab();
    let seated = harness::sweep_with_drains(&mesh, &policy, &[drain]);
    let off = harness::sweep_with_drains(&mesh, &policy.without_stranded_seating(), &[drain]);
    println!(
        "flat slab: stranded seats {}, lapping before {} after {} finished {}, branches kept {}; \
         overlap pairs with / without stranded seating: congestion entry {} / {}, lane exit {} / \
         {}, finished {} / {}",
        seated.report.counters.stranded_seats,
        seated.report.stranded_branch.lapping_before,
        seated.report.stranded_branch.lapping_after,
        seated.report.stranded_branch.lapping_on_finished_field,
        seated.report.stranded_branch.branches_kept,
        seated.report.congestion_lane.before.all.overlap_pairs,
        off.report.congestion_lane.before.all.overlap_pairs,
        seated.report.congestion_lane.at_lane_exit.all.overlap_pairs,
        off.report.congestion_lane.at_lane_exit.all.overlap_pairs,
        seated.report.congestion_lane.after.all.overlap_pairs,
        off.report.congestion_lane.after.all.overlap_pairs,
    );
    println!(
        "flat slab after each later lane: {:?}; branches {:?}",
        seated.report.stranded_branch.lapping_by_lane, seated.report.stranded_branch.branches
    );
    assert!(seated.report.counters.stranded_seats > 0, "premise: the rule seats here");
    assert!(
        seated.report.congestion_lane.before.all.overlap_pairs
            > off.report.congestion_lane.before.all.overlap_pairs,
        "premise: stranded seating stands a root on another before the congestion lane"
    );
    assert!(
        seated.report.congestion_lane.at_lane_exit.all.overlap_pairs
            <= off.report.congestion_lane.at_lane_exit.all.overlap_pairs,
        "stranded seating must not stand a root on another"
    );
    assert_eq!(seated.report.stranded_branch.lapping_after, 0);
    for (lane, count) in &seated.report.stranded_branch.lapping_by_lane {
        if *lane == "slenderness ties" {
            continue;
        }
        assert_eq!(*count, 0, "after {lane} a stranded seat stands on a lapped root again");
    }
}
