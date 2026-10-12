//! **A mast is plate-rooted, and its root flare has to clear the roots that
//! are already there** — the follow-up to Lane 3's own finding of 2026-09-06,
//! that after the congestion lane had spent four rounds spreading a footprint,
//! the SLENDERNESS lane put masts back on top of it.
//!
//! The defect was one line of reasoning, not one line of code: the mast search
//! asked whether a plate cell was EMPTY (`shaft_head == u32::MAX`), which is a
//! question about a single cell, and then landed a disc of
//! `FLARE_R_FACTOR × r_m` on it. A cell one grid pitch from a 0.6 mm root is
//! empty and is not clear.
//!
//! What is graded here:
//!
//! * every mast's flared disc clears every other plate root's flared disc, on
//!   the two fixtures that place masts and stay clear (asserted non-vacuous),
//!   and the one that does NOT stay clear publishes what it leaves;
//! * with the clearance test deleted, 127 overlapping mast pairs come back on
//!   the laid tube;
//! * on the laid tube the FINISHED field's congestion ratio stays within a
//!   stated tolerance of the congestion lane's own exit, which is the claim
//!   Lane 3 could not make; and its lapping root pairs, which crosses raise
//!   well past the exit's, are PUBLISHED exactly as the file draws them and
//!   held to a known-defect ratchet (see that test);
//! * the flank arms' pre-sizing radius estimate leaves no overlapping root
//!   behind once the down-sweep has sized the field.

use crate::harness;

use harness::sweep_with;
use plinth_core::{Finite, LayerPlan, PositiveFinite};
use plinth_mesh::{make_brace_pair, make_laid_tube, make_lonely_peak, Mesh};
use plinth_support::{BuildFrame, SupportSegmentKind};
use plinth_sweep::{to_primitives, Landing, StrutKind, SweepPlan, SweepPolicy, SweepStrut};

/// The production policy with ONE rung switched off: thickening.
///
/// Since 2026-09-29 the ladder thickens a post's root before it drops a mast
/// (owner ruling: merge, `|X|`, thicken, mast, warn), and on these fixtures
/// thickening now does the work the masts did — production places no mast on
/// any of them. Every claim in this file is about where a MAST's root lands,
/// so the rung above the mast is taken out and everything else (merges,
/// crosses, the clearance rule under test) stays as production has it.
///
/// And at the S these fixtures were built at
/// (`harness::PATH_FIXTURE_SLENDERNESS_RATIO`, 50): at the ruled 30 the braced
/// panel cannot hold their 33-35 mm posts at any mast radius, every mast cell
/// is refused by the panel check, and no mast is placed (measured 2026-10-01
/// on the brace pair, the laid tube and the lonely peak).
///
/// **Re-pinned 2026-10-04 (fix/drawn-solid-clearance):** and under the
/// centreline-only descent these fixtures were built on. With the drawn-solid
/// descent the laid tube's one descent beside a foot is replaced, its crosses
/// close the runs it needed masts for, and it places 0 masts (measured), so
/// the laid-tube claims below would be vacuous. The rule under test is where
/// a mast's ROOT lands; it reads no descent clearance.
fn policy() -> SweepPolicy {
    SweepPolicy {
        slenderness_thicken: false,
        ..harness::at_path_fixture_slenderness(
            SweepPolicy::from_registry()
                .expect("the registry supplies every number the sweep reads")
                .without_drawn_solid_descent(),
        )
    }
}

/// The registered root flare, read from the registry here rather than from the
/// crate under test, so a wrong factor in the code cannot agree with itself.
fn flare_factor() -> f64 {
    policy().flare_r_factor
}

/// Every plate root's centre and FLARED radius, in millimetres — the census's
/// crowding disc (the drawn root times `FLARE_R_FACTOR`). Until 2026-09-30
/// that was also the foot the emitter lays; since then the emitter stands
/// every root on the owner's ChituBox boss scaled to the root, and the census keeps this
/// crowding measure unchanged (`SweepPolicy::flare_r_factor`). A brace gets no
/// root boss and never lands on the plate, so it is not one.
fn flared_roots(plan: &SweepPlan) -> Vec<(u32, StrutKind, f64, f64, f64)> {
    let flare = flare_factor();
    plan.struts
        .iter()
        .filter(|strut| matches!(strut.landing, Landing::Plate) && strut.kind != StrutKind::Brace)
        .filter_map(|strut: &SweepStrut| {
            let root = *strut.radius_mm.first()?;
            (root.is_finite() && root > 0.0).then(|| {
                let cell = strut.base_cell();
                (
                    strut.id,
                    strut.kind,
                    plan.grid.center_x_mm(plan.grid.col_of(cell)),
                    plan.grid.center_y_mm(plan.grid.row_of(cell)),
                    root * flare,
                )
            })
        })
        .collect()
}

/// Unordered pairs of plate roots whose flared discs actually intersect, with
/// at least one of them a mast. `O(n²)` on purpose: a test that re-implemented
/// the production sweep line would grade its own copy of the algorithm rather
/// than the field.
fn overlapping_pairs_touching_a_mast(plan: &SweepPlan) -> Vec<(u32, u32, f64, f64)> {
    let roots = flared_roots(plan);
    let mut out = Vec::new();
    for (slot, first) in roots.iter().enumerate() {
        for second in &roots[slot + 1..] {
            if first.1 != StrutKind::Mast && second.1 != StrutKind::Mast {
                continue;
            }
            let dx = second.2 - first.2;
            let dy = second.3 - first.3;
            let centres = dx.hypot(dy);
            let reach = first.4 + second.4;
            if centres < reach {
                out.push((first.0, second.0, centres, reach));
            }
        }
    }
    out
}

fn masts(plan: &SweepPlan) -> usize {
    plan.struts
        .iter()
        .filter(|strut| strut.kind == StrutKind::Mast)
        .count()
}

/// The two slenderness fixtures at their AUTHORED pose — the harness runs a
/// mesh exactly as it is given, with no orientation search, which is what makes
/// these two place masts at all.
fn lonely_peak() -> Mesh {
    make_lonely_peak().expect("the lonely peak fixture")
}

fn brace_pair() -> Mesh {
    make_brace_pair().expect("the brace pair fixture")
}

fn laid_tube() -> Mesh {
    make_laid_tube().expect("the laid tube fixture")
}

/// **THE CLAIM.** No mast's flared disc laps over any plate root's flared disc,
/// on the finished, SIZED field.
///
/// Asserted pairwise over the real field, and asserted NON-VACUOUS: a fixture
/// that placed no mast would pass this trivially, so the mast count is graded
/// first and by name.
#[test]
fn every_mast_clears_every_plate_root_on_the_brace_pair() {
    let plan = sweep_with(&brace_pair(), 0.0, &policy());
    let placed = masts(&plan);
    assert!(
        placed > 0,
        "anti-vacuity: make_brace_pair must place at least one mast for the clearance claim to \
         mean anything; it placed {placed}"
    );
    let overlaps = overlapping_pairs_touching_a_mast(&plan);
    assert!(
        overlaps.is_empty(),
        "{placed} masts placed and {} of them lap over a plate root: {overlaps:?} \
         (id, id, centre distance mm, sum of flared radii mm)",
        overlaps.len()
    );
}

/// The same claim on the fixture that placed FIFTY-THREE masts before
/// 2026-09-29 (six since the bracing lane), which is where the rule is
/// actually load-bearing.
#[test]
fn every_mast_clears_every_plate_root_on_the_laid_tube() {
    let plan = sweep_with(&laid_tube(), 0.0, &policy());
    let placed = masts(&plan);
    // RE-DERIVED 2026-09-29 (bracing lane), on purpose: this was `> 10` when
    // the laid tube placed 53 masts. Trying every partner in reach before a
    // mast, and admitting a mast cell only where the braced panel holds, now
    // leave it 6 (with thickening switched off, see `policy`). The rule is
    // still what keeps them clear: the sabotage drill below measures more than
    // ten lapping pairs with it deleted.
    assert!(
        placed > 0,
        "anti-vacuity: make_laid_tube must place masts for this to be the load-bearing case; \
         it placed {placed}"
    );
    let overlaps = overlapping_pairs_touching_a_mast(&plan);
    assert!(
        overlaps.is_empty(),
        "{placed} masts placed and {} of them lap over a plate root: {overlaps:?}",
        overlaps.len()
    );
}

/// **WHERE THE CLAIM IS FALSE, SAID OUT LOUD.** `make_lonely_peak` finishes
/// with two lapping mast roots, and it finishes with exactly the same two
/// whether the clearance rule is in force or deleted.
///
/// That is not a placement failure: both cells were clear at the radius the
/// mast had when they were chosen (`r_m = max(r_min, H_first/S)`), and the
/// down-sweep afterwards sized both roots larger than the radius the search
/// was given. The residual is the SAME class of defect one level up — a disc
/// registered before its size is known — and it is published as
/// `mast_roots_overlapping_after_sizing` rather than asserted away.
///
/// This test grades HONESTY, not the count: the number the report publishes
/// must equal the number the field actually has. A future change that closes
/// the residual makes both zero and this test stays green.
#[test]
fn the_lonely_peak_publishes_the_mast_laps_its_sizing_pass_leaves() {
    // RE-PINNED 2026-10-02 (fix/morel-long-posts): graded with the lip-seat
    // rule off. That lane's clearance gate refuses one of the fixture's five
    // lip-seat re-landings (its drawn solid would stand inside the 0.3 mm
    // final gap of the peak); with that arm left where the up-sweep put it,
    // this configuration places no mast at all, and the honesty graded here
    // is about masts. With the lip-seat rule off the masts are back.
    let plan = sweep_with(
        &lonely_peak(),
        0.0,
        &SweepPolicy {
            lip_seat: false,
            ..policy()
        },
    );
    assert!(
        masts(&plan) > 0,
        "anti-vacuity: make_lonely_peak must place a mast"
    );
    let measured = overlapping_pairs_touching_a_mast(&plan);
    let published = plan
        .report
        .slenderness_lane
        .mast_roots_overlapping_after_sizing;
    // The published figure counts MASTS that lap; the pairwise measurement
    // counts PAIRS. A lapping mast is in at least one pair, so a non-zero
    // measurement demands a non-zero publication and the other way round.
    assert_eq!(
        published == 0,
        measured.is_empty(),
        "the report publishes {published} lapping mast roots and the field has {} lapping pairs: \
         {measured:?}",
        measured.len()
    );
}

/// **THE SABOTAGE DRILL.** Delete the clearance rule and the defect comes back.
///
/// A green clearance test only means something if the check is what makes it
/// green. `without_mast_root_clearance` restores the pre-2026-09-07 per-cell
/// occupancy test — is this cell empty — and nothing else.
///
/// **The fixture is `make_laid_tube`, and it was CHOSEN BY MEASUREMENT.**
/// `make_tall_stilt_plate` was tried first and is not a witness: it places one
/// mast and leaves zero lapping mast pairs with the rule on AND off, so it
/// cannot tell the two engines apart. That is reported rather than hidden. The
/// laid tube can: 0 lapping mast pairs in force, 127 with the rule deleted.
#[test]
fn deleting_the_clearance_rule_brings_the_overlapping_masts_back() {
    // RE-SCOPED 2026-09-30 (fix/chitubox-geometry), measured: with the owner's
    // ChituBox ball ratios the production laid tube (X 18, thickening off)
    // places 2 masts and 0 lapping pairs with the rule in force AND deleted,
    // so the drill told nothing apart. The drill now runs at the X the
    // fixture's mast path was built at (`crate::harness::PATH_FIXTURE_X_ALLOWED`
    // = 12); the clearance rule reads no X and no ball factor.
    let mesh = laid_tube();
    // Graded at the seat placement the fixture was built for
    // (`harness::at_path_fixture_seating`): at the production placement the
    // laid tube places 1 mast that laps nothing with the rule on or off
    // (measured 2026-09-30), so the drill cannot tell the two engines apart.
    // The clearance rule reads no placement.
    //
    // MERGED 2026-10-01 (land of fix/chitubox-geometry onto develop 3372093):
    // the two re-scopings answer two different causes (the seat placement from
    // fix/lattice-align, the ball ratio from fix/chitubox-geometry), and the
    // merged tree has both, so the drill runs under both.
    //
    // RE-PINNED 2026-10-02 (fix/morel-long-posts merged with develop 7826593):
    // the drill also runs with the creep-45 overhang cut off
    // (`without_creep_overhang_cut`, the owner's 2026-09-30 ruling). Measured
    // on the merged tree (`lanes/morel-long-posts/r12/diag-masts*.log`): with
    // the cut in force the field places 0 masts with the rule in force and 1
    // with it deleted, and still 0 and 1 with the lip-seat rule off, so the
    // lip-seat lane is not what took them; with the cut off it places 4 and 3.
    // The clearance rule reads no creep.
    //
    // RE-PINNED 2026-10-03 (fix/brace-window-cap, rule R2): and with the
    // brace cap off (`without_brace_cap`). Under R2 a mast's diagonal may be
    // no thicker than the arm it ties; on this configuration that refused
    // every mast with the clearance rule in force and deleted (0 and 0,
    // measured 2026-10-03). The clearance rule reads no brace radius.
    //
    // RE-PINNED 2026-10-11 (merge of quality Q12, Q1 + Q2, into develop): and
    // with the lip-seat rule off (`without_lip_seat`). Q1 keeps the seat of a
    // lean the slenderness law cannot hold, so on this configuration the field
    // places 0 masts with the clearance rule in force and 0 with it deleted
    // (was 4 and 3, measured 2026-10-11 on develop 2d2381a); with the rule off
    // it places 1 and 3 masts, 0 and 2 lapping pairs (measured 2026-10-11,
    // release). The clearance rule reads no lip-seat lean; refusing the twin
    // merge as well changes nothing.
    let fixture =
        crate::harness::at_path_fixture_seating(crate::harness::at_path_fixture_x(policy()))
            .without_creep_overhang_cut()
            .without_brace_cap()
            .without_lip_seat();
    let clean = sweep_with(&mesh, 0.0, &fixture);
    let sabotaged = sweep_with(&mesh, 0.0, &fixture.without_mast_root_clearance());

    let clean_masts = masts(&clean);
    let sabotaged_masts = masts(&sabotaged);
    assert!(
        clean_masts > 0 && sabotaged_masts > 0,
        "anti-vacuity: both fields must place masts; production {clean_masts}, \
         sabotaged {sabotaged_masts}"
    );

    let clean_overlaps = overlapping_pairs_touching_a_mast(&clean);
    let sabotaged_overlaps = overlapping_pairs_touching_a_mast(&sabotaged);
    assert!(
        clean_overlaps.is_empty(),
        "production placed {clean_masts} masts and {} of them lap: {clean_overlaps:?}",
        clean_overlaps.len()
    );
    // RE-DERIVED 2026-09-29 (fix/member-clearance): `> 10` → `> 0` pairs. With
    // crosses at one layer credited together, the laid tube (thickening off)
    // needs 2 masts, not 6, so a drill that deletes the rule can lap at most a
    // handful; it laps 1 where production laps 0, and that difference is what
    // this drill exists to show.
    println!(
        "laid tube, thickening off: {clean_masts} masts in force with {} lapping pairs; \
         {sabotaged_masts} masts and {} lapping pairs with the clearance rule deleted",
        clean_overlaps.len(),
        sabotaged_overlaps.len()
    );
    assert!(
        sabotaged_overlaps.len() > clean_overlaps.len(),
        "the sabotage drill deleted the clearance rule and only {} pairs overlapped \
         ({sabotaged_masts} masts placed). Either this fixture no longer exercises the rule, or \
         the rule is not what is keeping the production field clean — say which before believing \
         the green test above.",
        sabotaged_overlaps.len()
    );

    // And the ratio the owner reads in ChituBox moves with it.
    assert!(
        clean.report.congestion_lane.after.all.overlap_pairs
            < sabotaged.report.congestion_lane.after.all.overlap_pairs,
        "finished overlapping root pairs: {} in force against {} with the rule deleted",
        clean.report.congestion_lane.after.all.overlap_pairs,
        sabotaged.report.congestion_lane.after.all.overlap_pairs,
    );
}

/// **WHAT LANE 3 COULD NOT CLAIM, AND THE DEFECT THE HONEST CENSUS FOUND.**
///
/// The finished congestion RATIO stays within a stated tolerance of the
/// congestion lane's own exit. At 3ff52c7 the laid tube left the congestion
/// lane at `1.0004` with 2 overlapping pairs and its FINISHED file read
/// `1.0926` with 155, every bit of the difference put there by masts. The
/// ratio claim is the one that change made available, and it still holds.
///
/// **The tolerance is a TEST TOLERANCE and not a registered constant.** `0.01`
/// is one percent of the ratio, which is an order of magnitude below the
/// `0.0922` the defect used to add and an order of magnitude above the rounding
/// of a rasterised area.
///
/// **KNOWN DEFECT, OPEN (re-scoped 2026-09-29, orchestrator ruling): a cross
/// raises its posts' drawn roots, and the raised root flares lap their
/// neighbours on the plate.** A cross (brace) joining a post is sized by its
/// own slenderness minimum; the junction clause raises the post's DRAWN
/// profile down its trunk chain to the plate foot, so the foot the file flares
/// is bigger than the ledger root whose cell was chosen to clear. Until the
/// review fixes (`b24fedf`) the census sized its discs from the LEDGER root and
/// did not see this. Pair counts were then "5 at the lane exit, at most 4 x 5
/// finished", and this test was green on an undersized census. The honest
/// census (discs from `plinth_support::drawn_root_radius_mm`, the lowering's
/// own rule) reads **42** lapping pairs on the finished laid tube; crosses grow
/// 187 plate discs and 50 of them lap another root (published as
/// `slenderness_lane.cross_roots_lapping`). The laps were always in the file.
/// Recorded as an open defect in the field evidence's `lanes/LANDED.md`;
/// the remedy (extend the root clearance rule to every cross and accept more
/// masts, keep the laps as a published warning, or choose cross partners whose
/// raise clears) is the owner's call.
///
/// So this test grades two things about the pair count instead of a bound
/// relative to the exit:
/// * **HONESTY.** The published finished census equals the lapping pairs
///   among the root flares the production lowering actually emits, read off
///   the flares' own end points and radii. An undersized census goes red here.
/// * **A RATCHET.** The count may not grow past `KNOWN_ROOT_LAP_PAIRS`. A
///   fix that lowers it should lower the pin with it.
#[test]
fn the_finished_laid_tube_publishes_its_drawn_root_laps_and_they_do_not_grow() {
    const RATIO_TOLERANCE: f64 = 0.01;
    /// The honest finished count at the review-fixes land (2026-09-29), with
    /// this file's policy (thickening off). A ratchet on a known defect, not a
    /// tolerance: see the test's doc comment.
    const KNOWN_ROOT_LAP_PAIRS: usize = 42;

    let plan = sweep_with(&laid_tube(), 0.0, &policy());
    let lane = &plan.report.congestion_lane;

    // Anti-vacuity: the lane has to have run and the slenderness lane has to
    // have placed masts, or there is nothing between the two censuses.
    assert!(
        lane.rounds_used > 0,
        "anti-vacuity: the congestion lane ran zero rounds on the laid tube"
    );
    assert!(
        plan.report.slenderness_lane.masts_dropped > 0,
        "anti-vacuity: no mast was placed, so the finished census cannot differ from the exit one \
         for the reason this test exists"
    );

    let exit = lane.at_lane_exit.all.ratio;
    let finished = lane.after.all.ratio;
    assert!(
        finished <= exit + RATIO_TOLERANCE,
        "the finished field is congested at {finished:.4} against a lane exit of {exit:.4}, more \
         than the {RATIO_TOLERANCE} tolerance; the masts are back on top of the footprint"
    );

    // The pair count, which is the number that matches what the eye sees.
    // HONESTY first: the published census against the feet the file draws.
    let layers = LayerPlan::new(plan.plate_z_mm, vec![plan.layer_height_mm; plan.layers])
        .expect("the plan's own uniform layer plan");
    let frame = BuildFrame::new(
        Finite::new(plan.plate_z_mm).expect("a finite plate"),
        PositiveFinite::new(harness::LIFT_MM).expect("a positive lift"),
    );
    let emission = to_primitives(&plan, frame, &layers, 4_000_000, 1 << 30)
        .expect("the lowering refuses only about the request");
    // The census's crowding disc of each root the file draws: the `RootFlare`
    // record's TOP ring, the drawn root, times the crowding factor. (Until
    // 2026-09-30 this read the flare's foot, which WAS that disc; since then
    // the boss's foot is the owner's ChituBox boss scaled to the root, see
    // `flared_roots`.)
    let flare = flare_factor();
    let feet: Vec<([f64; 3], f64)> = emission
        .primitives
        .iter()
        .filter(|primitive| primitive.segment_kind == SupportSegmentKind::RootFlare)
        .map(|boss| {
            (
                boss.start_mm.map(Finite::get),
                boss.start_radius_mm.get() * flare,
            )
        })
        .collect();
    // `O(n²)` on purpose, as in `overlapping_pairs_touching_a_mast`.
    let mut drawn_pairs = 0_usize;
    for (slot, (a, ra)) in feet.iter().enumerate() {
        for (b, rb) in &feet[slot + 1..] {
            drawn_pairs += usize::from((a[0] - b[0]).hypot(a[1] - b[1]) < ra + rb);
        }
    }
    let exit_pairs = lane.at_lane_exit.all.overlap_pairs;
    let finished_pairs = lane.after.all.overlap_pairs;
    println!(
        "laid tube: {exit_pairs} lapping root pairs at the lane exit, {finished_pairs} published \
         finished, {drawn_pairs} among the {} emitted root flares; crosses grew {} plate discs, \
         {} lapping (ratio {finished:.4} against an exit of {exit:.4})",
        feet.len(),
        plan.report.slenderness_lane.cross_roots_grown,
        plan.report.slenderness_lane.cross_roots_lapping,
    );
    assert_eq!(
        lane.after.all.roots,
        feet.len(),
        "the finished census counts {} roots; the file flares {}",
        lane.after.all.roots,
        feet.len()
    );
    assert_eq!(
        finished_pairs, drawn_pairs,
        "the finished census publishes {finished_pairs} lapping root pairs; the root flares the \
         file draws form {drawn_pairs}"
    );

    // The RATCHET on the known defect named above.
    assert!(
        finished_pairs <= KNOWN_ROOT_LAP_PAIRS,
        "lapping root pairs on the finished laid tube grew: {finished_pairs} against the \
         known-defect pin of {KNOWN_ROOT_LAP_PAIRS} (lane exit {exit_pairs}; crosses lapping {})",
        plan.report.slenderness_lane.cross_roots_lapping,
    );
}

/// **THE FLANK ARMS' PRE-SIZING ESTIMATE, GRADED ON THE SIZED FIELD.**
///
/// A flank arm's plate cell is chosen before the arm exists, so its root radius
/// is an ESTIMATE; until 2026-09-06 the estimate was the smallest printable
/// radius, which is optimistic by construction and let the sizing pass grow the
/// discs into one another. The count is published either way — this asserts it
/// is zero, and prints it when it is not so the number is never silent.
#[test]
fn the_flank_arms_leave_no_overlapping_root_once_the_field_is_sized() {
    let plan = sweep_with(&laid_tube(), 0.0, &policy());
    let lane = &plan.report.congestion_lane;
    assert!(
        lane.arms_inserted > 0,
        "anti-vacuity: no flank arm was inserted on the laid tube, so the estimate was never used"
    );
    assert!(
        lane.flank_radius_used_mm > 0.0,
        "the estimate in force must be published: it read {}",
        lane.flank_radius_used_mm
    );
    assert_eq!(
        lane.flank_roots_overlapping_after_sizing, 0,
        "{} of the {} inserted flank roots still overlap another plate root once the down-sweep \
         has sized them (estimate in force {:.4} mm; mean placed root {:.4} mm; hex-cell area law \
         {:.4} mm)",
        lane.flank_roots_overlapping_after_sizing,
        lane.arms_inserted,
        lane.flank_radius_used_mm,
        lane.flank_radius_mean_placed_mm,
        lane.flank_radius_area_law_mm,
    );
}

/// **`shaft_spans_recorded` DESCRIBES THE FINISHED FIELD.**
///
/// It used to be read off the UP-SWEEP's column index, which nothing writes to
/// after the descent, the congestion lane and the slenderness lane have each
/// built their own. On a field where those lanes add members it was reporting a
/// pre-descent number. A field with more members than the up-sweep placed must
/// record at least as many spans as it has members.
#[test]
fn the_shaft_span_count_is_read_from_the_finished_field() {
    let plan = sweep_with(&laid_tube(), 0.0, &policy());
    let members = plan
        .struts
        .iter()
        .filter(|strut| strut.k_bottom != usize::MAX && strut.k_top >= strut.k_bottom)
        .count();
    assert!(
        plan.report.shaft_spans_recorded >= members,
        "{} shaft spans recorded against {members} members that have a shaft; the counter is \
         being read off an index the finished field never reached",
        plan.report.shaft_spans_recorded
    );
}
