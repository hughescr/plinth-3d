//! The MODEL-LANDING SCAR (`plinth_sweep::model_landing`): the fourth named
//! half of the orientation objective's scar term, measured on a real
//! production sweep.
//!
//! # What these tests found, stated before they are read
//!
//! The change was commissioned on the expectation that seeing "a strut lands
//! ON the blade" would flip `make_raking_blade` to blade-up. **It does not,
//! and these tests assert the measurement rather than the expectation.**
//! Measured at the production pitch (`examples/landing_scar_probe.rs` prints
//! every number below):
//!
//! | pose | arms | model landings | charged to | landing scar |
//! |---|---|---|---|---|
//! | identity, blade DOWN | 70 | 1 | blade face 14, fragility 0.542, **interior** | 0.20574 mm² |
//! | inverted, blade UP | 67 | 8 | slab face 3, fragility 0.150 | 0.05581 mm² |
//!
//! So both poses land on the model, the blade-down pose is charged ~3.7x more,
//! and the term moves the two scores 0.000823 and 0.000223 apart — against a
//! 0.003570 lead the blade-down pose already held on the display half. The
//! pose does not move. Three things that follow, none of which this lane may
//! fix by tuning:
//!
//! 1. The blade-down pose's only model landing is INTERIOR — the owner's
//!    2026-08-31 ruling is that such a scar is never seen and never removed.
//!    Discounting it would make this term push the other way on this fixture.
//! 2. The blade's own display value is `D = 0.00001`, while the plain 20x20 mm
//!    slab top carries enough `D` to put 0.92 mm² of display scar on the
//!    blade-up pose. The pose is being decided by the slab, not by the blade.
//! 3. The damage a person would actually see on the blade in the blade-down
//!    pose is arm TIPS on its down face, not the one arm seated on it. Tip
//!    contact is a different measurement and is not in scope here.
//!
//! **Superseded 2026-09-29 on finding 2.** The owner's ruling that detail is
//! the SPREAD of the fold angles (round-pose lane, rule B) scores the slab top
//! `D = 0`: every fold on this fixture is 90°, one magnitude, so it is change,
//! not detail. With the 0.92 mm² display charge gone, the two poses are
//! separated by burden and by the landing and tip scars, all in blade UP's
//! favour, and the committed pose is now blade UP by 0.00145 points. The
//! table's arm and landing counts still hold (the 2026-09-28 baseline already
//! read the blade-down landing scar as 0.20483 mm²); (d) pins the new pose.
//!
//! Every assertion below is a comparison between two measurements or a
//! comparison against zero, per the brief.

use plinth_core::{m0_constants, LayerPlan};
use plinth_orient::demand::{DemandFidelity, MeasuredSupportDemand, SupportDemandMeasurer};
use plinth_orient::search::{search, SweepRequest};
use plinth_orient::{OrientFaceTable, UpDirection};
use crate::harness::memo_run_sweep;
use plinth_sweep::{model_landing_scar, SweepDemand, SweepPlan, SweepPolicy};

/// The model's authored +Z stays up: the blade, authored hanging below the
/// slab, is DOWN.
const BLADE_DOWN: [f64; 3] = [0.0, 0.0, 1.0];
/// The model is inverted, so the blade is UP.
const BLADE_UP: [f64; 3] = [0.0, 0.0, -1.0];

fn table_for(mesh: &plinth_mesh::Mesh) -> OrientFaceTable {
    OrientFaceTable::build(mesh, "exact").expect("a closed fixture always scores")
}

/// One pose, measured through the production measurer exactly as the commit
/// tier's second stage measures it.
fn measure(
    mesh: &plinth_mesh::Mesh,
    table: &OrientFaceTable,
    direction: [f64; 3],
) -> MeasuredSupportDemand {
    let candidate = UpDirection::new(direction).expect("a unit direction");
    SweepDemand::production()
        .measure_support_demand(mesh, table, &[candidate], DemandFidelity::Production)
        .into_iter()
        .next()
        .expect("one candidate in, one result out")
        .expect("a well-formed fixture measures")
}

/// The same pose, swept directly, so a test can read the per-seat charges the
/// measurer folds into one number.
fn swept(mesh: &plinth_mesh::Mesh, direction: [f64; 3]) -> (plinth_mesh::Mesh, SweepPlan) {
    let candidate = UpDirection::new(direction).expect("a unit direction");
    let transformed =
        plinth_orient::oriented_recentered_mesh(mesh, candidate).expect("an orientable fixture");
    let constants = m0_constants().expect("the registry");
    let policy = SweepPolicy::from_registry().expect("the registered sweep policy");
    let lift_mm = constants.placeholders().lift_mm().diagnostic().get();
    let layer_height_mm = *constants.evidence().layer_height_default_mm().value();
    let max_z = transformed
        .verts()
        .iter()
        .fold(f64::NEG_INFINITY, |acc, vertex| acc.max(vertex[2]));
    let plate_z = -lift_mm;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the same layer-count arithmetic orient_demand.rs's measure_one performs, so this \
                  helper sweeps the pose the measurer swept"
    )]
    let count = (((max_z - plate_z) / layer_height_mm).ceil().max(1.0) as usize) + 1;
    let layers = LayerPlan::new(plate_z, vec![layer_height_mm; count]).expect("a layer plan");
    let plan = memo_run_sweep(&transformed, &layers, &policy).expect("a sweepable fixture");
    (transformed, plan)
}

/// (a) The term sees the strut on the delicate feature — and what it sees is a
/// COMPARISON, not a magnitude.
///
/// The brief expected the blade-up pose to have no model landing at all. It has
/// eight. What separates the two poses is not whether they land but WHERE:
/// every blade-down seat is charged against a face more fragile than every
/// blade-up seat, and the blade-down pose pays more scar for one seat than the
/// blade-up pose pays for eight.
///
/// Non-vacuity is asserted twice over: both poses must actually seat arms on
/// the model, or the comparison is about nothing.
#[test]
fn the_blade_down_pose_is_charged_against_a_more_fragile_face_than_the_blade_up_pose() {
    let mesh = plinth_mesh::make_raking_blade().expect("the raking blade fixture");
    let table = table_for(&mesh);

    let mut totals = Vec::new();
    let mut worst_fragility = Vec::new();
    for direction in [BLADE_DOWN, BLADE_UP] {
        let (oriented, plan) = swept(&mesh, direction);
        let scar = model_landing_scar(&oriented, &plan, table.delicacy(), table.display())
            .expect("the table carries a fragility field");
        assert!(
            scar.landings > 0,
            "this pose seats no arm on the model at all, so the comparison would be vacuous"
        );
        assert_eq!(
            scar.mapped, scar.landings,
            "every seat on a closed mesh must find a face to be charged against"
        );
        assert!(
            scar.raw_mm2 > 0.0,
            "a pose that seats arms on a mesh with a fragility field must be charged for them"
        );
        let fragilities: Vec<f64> = scar
            .charges
            .iter()
            .map(|charge| table.delicacy()[charge.face])
            .collect();
        let least = fragilities
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        let most = fragilities.iter().copied().fold(0.0_f64, f64::max);
        totals.push(scar.raw_mm2);
        worst_fragility.push((least, most));
    }

    let (down_least, _) = worst_fragility[0];
    let (_, up_most) = worst_fragility[1];
    assert!(
        down_least > up_most,
        "the blade-down pose's least fragile seat ({down_least}) should still be charged against a \
         more delicate face than the blade-up pose's most fragile seat ({up_most}); if this \
         inverts, the term is no longer reporting that the arm is on the delicate feature"
    );
    assert!(
        totals[0] > totals[1],
        "the blade-down pose ({}) should be charged more landing scar than the blade-up pose ({})",
        totals[0],
        totals[1]
    );
}

/// (b) A model whose arms all reach the plate is charged nothing, and its
/// measurement is otherwise untouched.
///
/// Round 50 in both of the poses the ratchet cares about. `Some(0.0)` is the
/// positive assertion "a production sweep ran and nothing it placed touched the
/// model" — distinct from the `None` a pose nobody swept carries.
#[test]
fn a_disc_that_lands_entirely_on_the_plate_is_charged_no_landing_scar() {
    let mesh = plinth_mesh::make_recessed_disc(24.0, 25.0, 3.5, 2.5, 22.5, 128)
        .expect("the Round 50 profile");
    let table = table_for(&mesh);
    for direction in [BLADE_DOWN, BLADE_UP] {
        let measured = measure(&mesh, &table, direction);
        assert_eq!(
            measured.model_landings, 0,
            "a flat disc seats every arm on the plate"
        );
        assert_eq!(
            measured.landing_scar_raw_mm2,
            Some(0.0),
            "a measured pose with no model landing is charged exactly zero, and says so"
        );
    }
}

/// (b, second half) A candidate charged a ZERO landing scar keeps every
/// committed number, bit for bit.
///
/// Measured as a comparison of two whole searches of the SAME disc: one through
/// the production measurer, one through the same measurer with the term
/// withheld. If folding a zero could perturb a score — a different
/// association, a re-rounded quotient — the two would part.
///
/// **Not every Round 50 finalist lands entirely on the plate.** The flat pose
/// does (0 model landings, 243 arms) and so does the committed winner; a
/// tilted finalist at `(0, -0.1736, 0.9848)` seats 7 arms on the model and is
/// charged 0.0736 mm². That candidate is expected to move and is excluded by
/// its own measurement, never by name.
#[test]
fn a_zero_landing_scar_changes_no_committed_number_on_a_disc() {
    let mesh = plinth_mesh::make_recessed_disc(24.0, 25.0, 3.5, 2.5, 22.5, 128)
        .expect("the Round 50 profile");
    let request = SweepRequest::registered().expect("the registered sweep configuration");
    let with = search(&mesh, &request, Some(&SweepDemand::production())).expect("a scorable disc");
    let without = search(&mesh, &request, Some(&SweepDemand::without_model_landing_scar()))
        .expect("a scorable disc");

    assert_eq!(
        with.winner().selection,
        without.winner().selection,
        "a disc with no model landing must commit to the same pose either way"
    );
    let with_commit = with.record().tiers.last().expect("a commit tier").clone();
    let without_commit = without.record().tiers.last().expect("a commit tier").clone();
    let mut zero_charged = 0_usize;
    for left in &with_commit.ranking {
        let Some(right) = without_commit
            .ranking
            .iter()
            .find(|entry| entry.candidate == left.candidate)
        else {
            continue;
        };
        // Selected by what was MEASURED, not by which pose it is.
        if left.terms.landing_scar_raw_mm2.map(plinth_core::Finite::get) != Some(0.0) {
            continue;
        }
        zero_charged += 1;
        assert_eq!(
            left.terms.score, right.terms.score,
            "candidate {} moved on a zero landing scar",
            left.candidate
        );
        assert_eq!(left.terms.scar_term, right.terms.scar_term);
        assert_eq!(left.terms.scar_raw_mm2, right.terms.scar_raw_mm2);
    }
    assert!(
        zero_charged > 0,
        "no candidate was charged a measured zero, so this test compared two runs that both did \
         nothing"
    );
    assert_eq!(
        with_commit.ranking[0].terms.landing_scar_raw_mm2.map(plinth_core::Finite::get),
        Some(0.0),
        "the disc's committed winner is the pose that lands entirely on the plate, so its own \
         numbers are among the ones proved unmoved above"
    );
}

/// (c) The identity the record must satisfy:
/// `scar_raw = fragility + display + landing`.
///
/// Checked on every commit-tier candidate of both flip poses of the blade — the
/// ones a production sweep priced (where the landing half is present) and the
/// ones only the prefilter reached (where it is absent, and the identity is the
/// two-half one).
///
/// **Measured with the tip-contact scar withheld** (`without_tip_contact_scar`),
/// so this test still isolates the THREE halves it names: `scar_raw`'s fifth
/// named half (`plinth_sweep::tip_landing`, `tests/tip_contact_scar.rs`) is a
/// separate, independently withholdable term, and folding it in here would
/// make this identity a four-half one under a three-half name.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "the claim under test IS bit equality: scar_raw is the sum of its three named halves \
              and of nothing else. A tolerance would let a fourth, unnamed contribution hide \
              inside it, which is the defect this test exists to catch"
)]
fn the_scar_raw_is_exactly_its_three_named_halves() {
    let mesh = plinth_mesh::make_raking_blade().expect("the raking blade fixture");
    let request = SweepRequest::registered().expect("the registered sweep configuration");
    let outcome = search(&mesh, &request, Some(&SweepDemand::without_tip_contact_scar()))
        .expect("a scorable fixture");
    let commit = outcome.record().tiers.last().expect("a commit tier").clone();

    let mut with_landing = 0_usize;
    for entry in &commit.ranking {
        let landing = entry
            .terms
            .landing_scar_raw_mm2
            .map_or(0.0, plinth_core::Finite::get);
        if entry.terms.landing_scar_raw_mm2.is_some() {
            with_landing += 1;
        }
        let halves = entry.terms.fragility_raw_mm2.get() + entry.terms.display_raw_mm2.get();
        assert_eq!(
            entry.terms.scar_raw_mm2.get(),
            halves + landing,
            "candidate {}'s scar_raw is not its three named halves",
            entry.candidate
        );
    }
    assert!(
        with_landing > 0,
        "no candidate carried a landing half, so the identity was only ever checked with a zero"
    );
}

/// (d) The sabotage: withhold the term and the committed numbers move.
///
/// The brief asked this drill to flip the committed pose. **It does not, and
/// the honest form of the drill is the one that can still fail.** What is
/// asserted is that the term is load-bearing rather than inert: withholding it
/// changes the winner's score and its `scar_raw` by exactly the landing scar
/// that was measured, and it changes the two poses by DIFFERENT amounts, so it
/// is capable of moving a ranking. Deleting the fold in
/// `TermsRecord::repriced`, or returning `None` from the measurer, reddens
/// every assertion here.
///
/// The pose itself is pinned, deliberately: a future change that flips
/// `make_raking_blade` must come back through this test and say so.
///
/// # Re-cut 2026-09-29: the pose DID flip, and bit equality went with it
///
/// Rule B of the round-pose lane (detail is the SPREAD of the fold angles,
/// owner ruling 2026-09-05) took away the display charge that had kept the
/// blade DOWN since 2026-09-04: every fold on this two-box fixture is 90°,
/// one magnitude, so `D` is 0 on every face. The committed pose is now the
/// 180° flip, blade UP (`u = −Z`), by 0.00145 points — the pose the owner
/// judged good on 2026-09-03 (`plinth-evidence/slice-sweep-2026-09-02/
/// field-2026-09-28/lanes/round-pose.md`). Withholding the landing scar does
/// not move it back. Two assertions here had followed the old winner rather
/// than the claim:
///
/// * Bit equality of `with == without + landing` held only by coincidence of
///   the old association: the engine adds `base + (landing + tip)`
///   (`TermsRecord::repriced`), this compares `(base + tip) + landing`, and at
///   the new winner they differ by one ULP (65.83742749999999 against
///   65.8374275). It is now the 1e-9 relative band of
///   `tip_contact_scar.rs`'s four-halves identity (re-cut 2026-09-06), with
///   the guard that earns it: the landing scar itself must lie far outside
///   the band, so a missing term can never pass as association noise.
/// * "The winner is charged MORE than the runner-up" was a fact about blade
///   down, which seats one arm on the model against blade up's eight but
///   lands a larger scar. The claim is that the two poses are charged
///   DIFFERENT amounts, so the term can reorder them — as the tip-scar drill
///   already states it.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "the pinned pose is an exact axis direction, and the two poses' shifts are compared for \
              inequality: a tolerance would let two different charges pass as equal"
)]
fn withholding_the_landing_scar_moves_the_committed_numbers_but_not_this_pose() {
    let mesh = plinth_mesh::make_raking_blade().expect("the raking blade fixture");
    let request = SweepRequest::registered().expect("the registered sweep configuration");
    let with =
        search(&mesh, &request, Some(&SweepDemand::production())).expect("a scorable fixture");
    let without = search(&mesh, &request, Some(&SweepDemand::without_model_landing_scar()))
        .expect("a scorable fixture");

    let with_commit = with.record().tiers.last().expect("a commit tier").clone();
    let without_commit = without.record().tiers.last().expect("a commit tier").clone();
    let winner = &with_commit.ranking[0];
    let winner_without = without_commit
        .ranking
        .iter()
        .find(|entry| entry.candidate == winner.candidate)
        .expect("the same candidate is ranked in both searches");

    let landing = winner
        .terms
        .landing_scar_raw_mm2
        .expect("the winner was measured at production fidelity")
        .get();
    assert!(
        landing > 0.0,
        "the winner seats no arm on the model, so withholding the term would be a no-op and this \
         drill would pass vacuously"
    );
    // Stated as an addition, within the association band (doc above): the
    // run with the term measured the run without it, plus the landing scar,
    // and nothing else. The guard first: the band must be far narrower than
    // the term it could otherwise hide.
    let whole = winner.terms.scar_raw_mm2.get();
    let tolerance = 1.0e-9 * whole.abs().max(1.0);
    assert!(
        landing > tolerance,
        "the landing scar {landing} lies inside the association band {tolerance}, so the band \
         could hide it"
    );
    let rebuilt = winner_without.terms.scar_raw_mm2.get() + landing;
    assert!(
        (whole - rebuilt).abs() <= tolerance,
        "the whole difference between the two runs' scar measurement is the landing scar: \
         {whole} against {rebuilt}, a gap of {} against a band of {tolerance}",
        (whole - rebuilt).abs()
    );
    assert!(
        winner.terms.score.get() > winner_without.terms.score.get(),
        "withholding a positive scar must make the winner score better, not worse"
    );
    assert_eq!(
        winner_without.terms.landing_scar_raw_mm2, None,
        "a withheld term is a stated absence, not a zero"
    );

    // Two poses, moved by different amounts: the term can reorder, it just
    // does not reorder THIS fixture.
    let runner_up = &with_commit.ranking[1];
    let runner_up_without = without_commit
        .ranking
        .iter()
        .find(|entry| entry.candidate == runner_up.candidate)
        .expect("the same candidate is ranked in both searches");
    let winner_shift = winner.terms.score.get() - winner_without.terms.score.get();
    let runner_up_shift = runner_up.terms.score.get() - runner_up_without.terms.score.get();
    assert!(
        winner_shift != runner_up_shift,
        "the term charged the winning pose and the runner-up by the identical amount \
         ({winner_shift}); it would then be structurally incapable of ever reordering them"
    );

    // The finding, pinned: withholding the term does not move this fixture's
    // pose, and that pose is blade UP since 2026-09-29 (doc above).
    assert_eq!(
        with.winner().selection,
        without.winner().selection,
        "make_raking_blade's committed pose moved when the landing scar was withheld. That is a \
         real change in the engine's answer: re-measure and re-record it."
    );
    assert_eq!(
        with.record().winner.up_model_frame.map(plinth_core::Finite::get),
        [0.0, 0.0, -1.0],
        "make_raking_blade no longer commits the 180 deg flip (blade UP). That is a real change in \
         the engine's answer: re-measure it and re-record lanes/round-pose.md's finding."
    );
}

/// (e) A seat on a display face costs the same however sturdy the face is:
/// fragility and display ADD (2026-09-29, `plinth_orient::score`'s module
/// doc), so a face with no fragility and full display value is charged
/// exactly what a fully fragile face with no display value is — the seat's
/// contact area.
///
/// The same plan is priced twice against two synthetic per-face fields over
/// the real mesh, so only the pricing rule is under test. Under the old
/// `fragility × (1 + D)` rule the first pricing was exactly zero.
#[test]
fn a_seat_on_a_sturdy_display_face_costs_what_a_seat_on_a_fragile_plain_face_costs() {
    let mesh = plinth_mesh::make_raking_blade().expect("the raking blade fixture");
    let (oriented, plan) = swept(&mesh, BLADE_UP);
    let faces = oriented.tris().len();
    let (zeros, ones) = (vec![0.0_f64; faces], vec![1.0_f64; faces]);

    let sturdy_display = model_landing_scar(&oriented, &plan, &zeros, &ones)
        .expect("a fragility field of the right length");
    let fragile_plain = model_landing_scar(&oriented, &plan, &ones, &zeros)
        .expect("a fragility field of the right length");
    assert!(
        fragile_plain.landings > 0 && fragile_plain.raw_mm2 > 0.0,
        "the blade-up pose seats no arm on the model, so the comparison would be vacuous"
    );
    assert!(
        (sturdy_display.raw_mm2 - fragile_plain.raw_mm2).abs() <= 1e-12 * fragile_plain.raw_mm2,
        "seats on faces with no fragility and full display value cost {} mm2; the same seats on \
         fully fragile faces with no display value cost {}. The display harm is following \
         fragility again, so a sturdy display face is left unprotected",
        sturdy_display.raw_mm2,
        fragile_plain.raw_mm2,
    );
}
