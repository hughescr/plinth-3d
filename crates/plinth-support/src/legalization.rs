//! **Stage B — the legalization retry's vocabulary.**
//!
//! Design authority: `docs/specs/seal-semantics.md`
//! §1.3 and §4 Stage B, authorized by Craig 2026-08-23.
//!
//! The routing stage seals a record saying a terminal is `Routed`. The brace
//! stage then measures that terminal's arm at its true radius, finds it inside
//! the model, and — since Stage A — abandons the pass rather than minting the
//! `Routed`/`Quarantined` pair the accounting forbids. Stage A stops there.
//! This module is what lets the *next* pass route differently.
//!
//! **Two channels, and they are not the same instruction.**
//!
//! * A [`NoGood`] **steers**. It names one `(terminal, prefix context, arm)`
//!   triple and claims exactly one sentence: *this candidate was measured
//!   illegal in at least one world that had this prefix; while another choice
//!   is available here, take it.* It is a heuristic with respect to the suffix
//!   — the true radius is assigned afterwards by the whole-forest resize, off a
//!   path whose worst span depends on every terminal routed AFTER this one — so
//!   it may suppress a choice and it may **never** retire a terminal. Nothing in
//!   this module counts no-goods toward a retirement.
//! * A [`ConfirmationRound`] **forces**. It drives one terminal through its
//!   remaining arms, one arm per pass, in the tournament's own order, with the
//!   no-good set held frozen. That is the only thing allowed to earn a
//!   retirement, and the round is UNCAPPED in engine semantics (Craig's
//!   authorization item 4). The only ceiling anywhere is the test/eval
//!   harness's wall clock, which lives outside the engine.
//!
//! **The finiteness obligation, stated where the hazard is.** §1.3's
//! termination argument needs the context universe to be finite, which needs
//! the no-good set — and anything derived from it — to be no input to any
//! digest in the context tuple, in particular `controller_digest_before`. The
//! set is therefore never a field of anything a `digest()` walks: it travels as
//! a borrowed parameter, from the controller loop that owns it down to the
//! tournament that reads it, and back out again. If it ever leaked in, every
//! context would be fresh on every pass, every no-good would miss forever, and
//! the loop would abandon forever while satisfying the abandon rule at every
//! step. `a_growing_no_good_set_moves_no_context_digest` is the standing guard.

use std::collections::{BTreeMap, BTreeSet};

use crate::cco::{CandidateFamily, CcoCandidateEvidence, TerminalRouteEvidence};

/// The **prefix** half of a no-good's key: the digests of the inputs the
/// terminal's tournament reads *before* it chooses.
///
/// §2.3's rule is *key on the digests of the inputs the computation reads*.
/// `generate_descriptors` reads the forest (`forest_digest_before`), the
/// terminal's own request (pinned by `plan_digest` and `loads_digest`), the
/// qualified anchor seats and the surface/topology (`geometry_digest`), and the
/// registered config, which is run-invariant. `hosts.nearest` reads the same
/// forest. `controller_digest_before` closes the cadence position. The S-1
/// measurement — routing cadence bit-identical across replays and across
/// `RAYON_NUM_THREADS` 2/3/default — is the evidence that the closure holds.
///
/// It is deliberately NOT widened with a post-brace digest. §1.3: widen the key
/// to include the suffix and the no-good becomes sound and never hits, because
/// any no-good anywhere changes the suffix; the loop then replays identically,
/// the abandon rule bites on the first repeat, and the mechanism degenerates
/// into a refusal that legalizes nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PrefixKey {
    plan_digest: u64,
    loads_digest: u64,
    geometry_digest: u64,
    forest_digest_before: u64,
    controller_digest_before: u64,
}

impl PrefixKey {
    /// Project the sealed route evidence onto the five digests. The identity
    /// fields (`physical_contact_id`, `terminal_id`, `original_key`) are
    /// deliberately left out: the terminal is named beside the key, and folding
    /// it twice would make two rows for one fact.
    pub(crate) const fn of(route: &TerminalRouteEvidence) -> Self {
        Self {
            plan_digest: route.plan_digest,
            loads_digest: route.loads_digest,
            geometry_digest: route.geometry_digest,
            forest_digest_before: route.forest_digest_before,
            controller_digest_before: route.controller_digest_before,
        }
    }

    fn render(&self) -> String {
        format!(
            "plan={:016x}/loads={:016x}/geom={:016x}/forest={:016x}/ctrl={:016x}",
            self.plan_digest,
            self.loads_digest,
            self.geometry_digest,
            self.forest_digest_before,
            self.controller_digest_before,
        )
    }
}

/// The **arm** half of the key: the four fields `CcoCandidateEvidence` already
/// discriminates a candidate on, compared by exact bit pattern.
///
/// **This is a deliberate, recorded deviation from §4 Stage B piece 2**, which
/// asked for the arm's endpoint coordinates *together with* these four. The
/// reason is provenance. The only endpoint source available where a no-good is
/// minted is the brace-stage forest, and that forest is POST-REWIRE: rewire may
/// have re-parented the stub, so its endpoints are not the tournament's own
/// record of what the tournament chose. An endpoint pair read there and
/// compared against a fresh candidate's would MISS, and a no-good that misses
/// makes the whole mechanism a silent no-op — a refusal indistinguishable from
/// "this model genuinely cannot be legalized".
///
/// The four fields are the tournament's own record, sealed in
/// `TerminalDisposition::Routed { winner, .. }` and already folded into
/// `final_controller_digest`. Under a fixed [`PrefixKey`], `generation_index`
/// is a candidate's identity by construction — it indexes the descriptor list
/// `generate_descriptors` produced from inputs the prefix digests cover — so
/// the pair (prefix, generation_index) alone pins the arm, and `family`,
/// `price_ml` and `clearance_rows` are three independent cross-checks that turn
/// a prefix key which under-determines the descriptor list into a MISS instead
/// of a wrong suppression.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ArmIdentity {
    family: CandidateFamily,
    generation_index: usize,
    /// `f64` by exact bits. Not a tolerance and not a comparison — the value is
    /// either the same bit pattern the tournament sealed or it is a different
    /// candidate.
    price_ml_bits: u64,
    clearance_rows: usize,
}

impl ArmIdentity {
    pub(crate) fn of(winner: &CcoCandidateEvidence) -> Self {
        Self {
            family: winner.family,
            generation_index: winner.generation_index,
            price_ml_bits: winner.price_ml.to_bits(),
            clearance_rows: winner.clearance_rows,
        }
    }

    /// The same four fields, read off a candidate that has not been sealed —
    /// the tournament side of the comparison.
    pub(crate) fn live(
        family: CandidateFamily,
        generation_index: usize,
        price_ml: f64,
        clearance_rows: usize,
    ) -> Self {
        Self {
            family,
            generation_index,
            price_ml_bits: price_ml.to_bits(),
            clearance_rows,
        }
    }

    /// Fold this identity into a digest, by exact bits.
    pub(crate) fn digest(&self, word: &mut impl FnMut(u64)) {
        word(match self.family {
            CandidateFamily::PlateDescent => 0,
            CandidateFamily::ModelAnchor => 1,
            CandidateFamily::Bifurcation => 2,
        });
        word(self.generation_index as u64);
        word(self.price_ml_bits);
        word(self.clearance_rows as u64);
    }

    fn render(&self) -> String {
        format!(
            "{:?}/gen{}/price{:016x}/rows{}",
            self.family, self.generation_index, self.price_ml_bits, self.clearance_rows,
        )
    }
}

/// One steering no-good: *this candidate was measured illegal in at least one
/// world that had this prefix.*
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct NoGood {
    terminal_id: u32,
    prefix: PrefixKey,
    arm: ArmIdentity,
}

impl NoGood {
    pub(crate) const fn arm(&self) -> ArmIdentity {
        self.arm
    }

    pub(crate) const fn new(terminal_id: u32, prefix: PrefixKey, arm: ArmIdentity) -> Self {
        Self {
            terminal_id,
            prefix,
            arm,
        }
    }

    pub(crate) const fn terminal_id(&self) -> u32 {
        self.terminal_id
    }

    pub(crate) fn render(&self) -> String {
        format!(
            "t{}:{}:{}",
            self.terminal_id,
            self.arm.render(),
            self.prefix.render()
        )
    }
}

/// The round that is the only thing allowed to earn a retirement.
///
/// One round belongs to one terminal. It is entered when that terminal reports
/// its steering spent — every arm it can still generate at its current context
/// carries a no-good — and it measures the terminal's remaining arms one per
/// pass, in the tournament's own order, with the steering set frozen.
///
/// **Uncapped, on Craig's authorization item 4.** `arm_rank` counts up without
/// a ceiling; the round ends when an arm survives bracing or when the
/// tournament reports it has no arm at that rank. There is deliberately no
/// maximum here and none anywhere below it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ConfirmationRound {
    /// The terminal the round is measuring.
    pub(crate) terminal_id: u32,
    /// Which arm of the tournament's own order this pass must take, counting
    /// from zero.
    pub(crate) arm_rank: usize,
    /// **What the round has actually MEASURED**, one row per arm it has driven,
    /// in rank order. This is the round's whole justification: §1.3 requires
    /// that what a reader can check is not *"here are k negative numbers"* but
    /// *"here is a round in which nothing this terminal could have chosen was
    /// legal"*, so the rows carry the rank they were measured at as well as
    /// the millimetres.
    ///
    /// These rows are recorded AGAINST THE ROUND and never added to the
    /// steering set — §1.3: *"Refusals measured during the round are recorded
    /// against the round, not added to the steering set."* The set is frozen
    /// for the round's duration, which is what makes the round replayable.
    pub(crate) measured: Vec<RoundArm>,
}

/// The measured physics of one refused arm, as the brace pass reported it.
///
/// Separate from [`RoundArm`] because a steering pass produces these and
/// discards them, while a round pairs one with the rank it was measured at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ArmRefusalPhysics {
    pub(crate) gap_mm: f64,
    pub(crate) distance_clearance_mm: f64,
    pub(crate) axis_transit_illegal: bool,
    pub(crate) terminal_radius_mm: f64,
    pub(crate) rootward_radius_mm: f64,
}

impl ArmRefusalPhysics {
    /// Pair this measurement with the arm and the rank the round drove it at.
    pub(crate) const fn at_rank(self, rank: usize, arm: ArmIdentity) -> RoundArm {
        RoundArm {
            rank,
            arm,
            gap_mm: self.gap_mm,
            distance_clearance_mm: self.distance_clearance_mm,
            axis_transit_illegal: self.axis_transit_illegal,
            terminal_radius_mm: self.terminal_radius_mm,
            rootward_radius_mm: self.rootward_radius_mm,
        }
    }
}

/// One arm a confirmation round measured, with the physics that refused it.
///
/// The margins are MEASURED, in millimetres, and the radii are the ones
/// `resize_forest_current_parity` actually assigned — the deciding input the
/// refutation proved cannot be predicted at candidacy. `gap_mm` is the verdict
/// number the engine acted on; when `axis_transit_illegal` is set it is a
/// sentinel and carries no distance, and `distance_clearance_mm` is the honest
/// distance-path value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RoundArm {
    /// Which rank of the tournament's own order this was.
    pub(crate) rank: usize,
    pub(crate) arm: ArmIdentity,
    pub(crate) gap_mm: f64,
    pub(crate) distance_clearance_mm: f64,
    pub(crate) axis_transit_illegal: bool,
    pub(crate) terminal_radius_mm: f64,
    pub(crate) rootward_radius_mm: f64,
}

impl RoundArm {
    pub(crate) fn render(&self) -> String {
        let kind = if self.axis_transit_illegal {
            "axis-transit"
        } else {
            "distance"
        };
        format!(
            "rank{}={}({}) gap={:.6}mm distance_clearance={:.6}mm r={:.6}/{:.6}mm",
            self.rank,
            self.arm.render(),
            kind,
            self.gap_mm,
            self.distance_clearance_mm,
            self.terminal_radius_mm,
            self.rootward_radius_mm,
        )
    }
}

/// **Why a pass stopped, and what the next one should do differently.**
///
/// An abandoned pass is not a failure. It produced no lowered run, therefore no
/// sealed authority, therefore nothing that can reach an artifact — which is
/// what makes "a pass that was abandoned may never reach output" true by
/// construction rather than by a check (§1.3 obligation 2).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LegalizationAbandon {
    /// The bracing pass measured a sealed-`Routed` terminal's OWN chosen arm
    /// inside the model. The arms travel out as no-goods and the next pass
    /// routes steered away from them.
    ///
    /// `cascade_victims` are terminals disturbed purely by re-parenting — their
    /// own arm was never measured illegal. They are reported and they **steer
    /// nothing**: a victim whose carrier sits later in the cadence would never
    /// see its own prefix move, so its no-goods would hit every pass and it
    /// would lose one legal arm per pass as punishment for another member's
    /// failure (§1.3).
    Condemned {
        learned: Vec<NoGood>,
        own_arm_terminals: Vec<u32>,
        cascade_victims: Vec<u32>,
        /// The MEASURED physics behind each learned no-good, in the same order:
        /// the millimetres the brace pass actually measured and the radii the
        /// resize actually assigned. A steering pass throws these away — a
        /// no-good claims nothing about magnitude. A CONFIRMATION ROUND keeps
        /// them, because a retirement has to show measured physics and not
        /// arithmetic (§1.3 verifier addition 1).
        measured: Vec<ArmRefusalPhysics>,
    },
    /// The routing pass found a terminal with a steering no-good on every arm
    /// it can still take. Nothing is retired; a confirmation round opens.
    SteeringSpent { terminal_id: u32 },
}

impl LegalizationAbandon {
    pub(crate) fn render(&self) -> String {
        match self {
            Self::Condemned {
                learned,
                own_arm_terminals,
                cascade_victims,
                ..
            } => {
                let ids = |ids: &[u32]| {
                    if ids.is_empty() {
                        "none".to_owned()
                    } else {
                        ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
                    }
                };
                format!(
                    "condemned own_arm_terminals={} cascade_victims={} learned=[{}]",
                    ids(own_arm_terminals),
                    ids(cascade_victims),
                    learned
                        .iter()
                        .map(NoGood::render)
                        .collect::<Vec<_>>()
                        .join(" "),
                )
            }
            Self::SteeringSpent { terminal_id } => {
                format!("steering-spent terminal={terminal_id}")
            }
        }
    }
}

/// The instruction set one legalization pass is driven with — **borrowed, never
/// owned by anything that digests.**
///
/// A pass carries at most one round: §1.3's round holds the steering set fixed
/// for its duration, so a second concurrent round would be measuring against a
/// set the first round is not allowed to see move.
#[derive(Clone, Debug, Default)]
pub(crate) struct LegalizationSteering {
    no_goods: BTreeSet<NoGood>,
    round: Option<ConfirmationRound>,
}

impl LegalizationSteering {
    /// Is there nothing to do? An empty steering is the attempt-0 pass every
    /// model that never abandons runs, and the tournament skips every read
    /// below on it — so a model with no exact-clearance refusals pays nothing
    /// for this mechanism existing.
    #[allow(
        dead_code,
        reason = "the vocabulary lands one commit ahead of the attempt-0 loop that drives it; \
                  docs/specs/seal-semantics.md §\"Stage B — the legalization retry (attempt 0 \
                  only)\" piece 4 (\"The loop\") names the reader"
    )]
    pub(crate) fn is_idle(&self) -> bool {
        self.no_goods.is_empty() && self.round.is_none()
    }

    pub(crate) fn len(&self) -> usize {
        self.no_goods.len()
    }

    #[allow(
        dead_code,
        reason = "the vocabulary lands one commit ahead of the confirmation round that drives \
                  it; docs/specs/seal-semantics.md §\"Stage B — the legalization retry (attempt \
                  0 only)\" piece 5 (\"The confirmation round\") names the reader"
    )]
    pub(crate) const fn round(&self) -> Option<&ConfirmationRound> {
        self.round.as_ref()
    }

    /// The round, mutably, so the loop can advance its rank and append what it
    /// measured. The loop is the only thing that drives a round.
    pub(crate) const fn round_mut(&mut self) -> Option<&mut ConfirmationRound> {
        self.round.as_mut()
    }

    pub(crate) fn set_round(&mut self, round: Option<ConfirmationRound>) {
        self.round = round;
    }

    /// Learn one no-good. Returns `true` when the set actually grew — which is
    /// exactly the abandon rule's test: **every abandoned pass must add at
    /// least one no-good that was not already in the set.** A pass that would
    /// abandon while learning nothing new must refuse by name instead of
    /// looping.
    pub(crate) fn learn(&mut self, no_good: NoGood) -> bool {
        self.no_goods.insert(no_good)
    }

    /// How many no-goods each terminal carries, for the report. Read by no
    /// branch.
    pub(crate) fn counts_by_terminal(&self) -> BTreeMap<u32, usize> {
        let mut counts = BTreeMap::new();
        for no_good in &self.no_goods {
            *counts.entry(no_good.terminal_id).or_insert(0_usize) += 1;
        }
        counts
    }

    /// Project the whole set onto the one terminal the tournament is about to
    /// route. Called once per terminal per pass, never per candidate.
    pub(crate) fn for_terminal(&self, terminal_id: u32) -> TerminalSteering<'_> {
        TerminalSteering {
            terminal_id,
            no_goods: self
                .no_goods
                .iter()
                .filter(|no_good| no_good.terminal_id == terminal_id)
                .collect(),
            forced_rank: self
                .round
                .as_ref()
                .filter(|round| round.terminal_id == terminal_id)
                .map(|round| round.arm_rank),
            round_measured: self
                .round
                .as_ref()
                .filter(|round| round.terminal_id == terminal_id)
                .map_or(&[][..], |round| round.measured.as_slice()),
        }
    }
}

/// The read-only view one terminal's tournament consults.
///
/// Built once per terminal from [`LegalizationSteering::for_terminal`]; the
/// tournament asks it two questions and asks each of them over the whole
/// candidate slate at once, never one native call at a time.
#[derive(Clone, Debug)]
pub(crate) struct TerminalSteering<'a> {
    terminal_id: u32,
    no_goods: Vec<&'a NoGood>,
    forced_rank: Option<usize>,
    /// Everything the open round has measured so far. The tournament seals this
    /// into `NoWinnerEvidence` at the moment it retires the terminal, so the
    /// retirement carries the round that earned it rather than a count.
    round_measured: &'a [RoundArm],
}

/// One candidate's steering verdict, with both compared halves retained so a
/// run log can tell a live mechanism from a no-op.
///
/// This is the instrument the whole stage's honesty rests on: if a no-good
/// misses every pass, the second pass learns nothing new, the abandon rule
/// fires, and the model refuses by name — a refusal that reads exactly like
/// "this model genuinely cannot be legalized". Streaming both halves of every
/// comparison is what separates the two.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SteeringProbe {
    pub(crate) hit: bool,
    pub(crate) candidate: ArmIdentity,
    pub(crate) live_prefix: PrefixKey,
    /// The no-good that matched, or the nearest one considered — the sealed
    /// halves, for the log line.
    pub(crate) against: Option<NoGood>,
}

impl SteeringProbe {
    pub(crate) fn render(&self) -> String {
        let verdict = if self.hit { "HIT" } else { "MISS" };
        match &self.against {
            Some(no_good) => format!(
                "{verdict} live={}@{} sealed={}",
                self.candidate.render(),
                self.live_prefix.render(),
                no_good.render(),
            ),
            None => format!(
                "{verdict} live={}@{} sealed=none",
                self.candidate.render(),
                self.live_prefix.render(),
            ),
        }
    }
}

/// What one pass's steering says about one terminal's whole slate.
///
/// **The decision lives here, in the module that owns the vocabulary, rather
/// than inline in the tournament** — because one arm of it is the guard against
/// the defect that made the second design review return FLAWED, and a guard is
/// worth more where it can be tested on a synthetic slate in microseconds than
/// where testing it costs seventy full routing passes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SteeringVerdict {
    /// A confirmation round: take the arm at this rank of the tournament's own
    /// order, suppressing nothing. A rank past the end of the slate is how a
    /// round ends in exhaustion.
    Forced { rank: usize },
    /// **Every arm carries a no-good.** The pass must STOP — not drop the slate
    /// and route on, because a terminal that reaches the cadence with no winner
    /// is retired `UnroutedNoWinner` on the spot, its contact removed from the
    /// accepted set and its carried area repartitioned onto its neighbours.
    /// That retirement would be earned by nothing but a count of refusals, each
    /// measured in a world that has since been replaced — §1.3 forbids it, and
    /// a confirmation round is what earns it honestly instead.
    Spent { ruled_out: usize },
    /// Ordinary steering. `keep` is parallel to the slate; at least one entry
    /// is `true`.
    Suppressed { ruled_out: usize, keep: Vec<bool> },
}

impl TerminalSteering<'_> {
    pub(crate) const fn terminal_id(&self) -> u32 {
        self.terminal_id
    }

    /// Decide what happens to a whole sorted slate at once.
    ///
    /// Batched by construction: one walk over the slate crossed with this
    /// terminal's own no-goods, both in memory. No native call, and nothing
    /// per-candidate beyond the probe row the log line needs.
    pub(crate) fn verdict(
        &self,
        live_prefix: PrefixKey,
        slate: &[ArmIdentity],
    ) -> (SteeringVerdict, Vec<SteeringProbe>) {
        let probes = self.probe(live_prefix, slate);
        if let Some(rank) = self.forced_rank {
            return (SteeringVerdict::Forced { rank }, probes);
        }
        let ruled_out = probes.iter().filter(|probe| probe.hit).count();
        let verdict = if ruled_out > 0 && ruled_out == slate.len() {
            SteeringVerdict::Spent { ruled_out }
        } else {
            SteeringVerdict::Suppressed {
                ruled_out,
                keep: probes.iter().map(|probe| !probe.hit).collect(),
            }
        };
        (verdict, probes)
    }

    pub(crate) fn is_idle(&self) -> bool {
        self.no_goods.is_empty() && self.forced_rank.is_none()
    }

    /// The rows the open round has measured, for the seal.
    pub(crate) const fn round_measured(&self) -> &[RoundArm] {
        self.round_measured
    }

    /// Probe the whole slate at once. One pass over the candidates times one
    /// pass over this terminal's no-goods — both small, both in memory, no
    /// native call and no per-candidate allocation beyond the verdict row.
    ///
    /// **During a confirmation round the steering set is FROZEN and not
    /// consulted**: the round's job is to re-measure arms the steering had
    /// suppressed, so honouring the suppression inside the round would measure
    /// nothing. §1.3: *the terminal takes each remaining arm in turn, one arm
    /// per pass, ordered by the tournament's own order.*
    pub(crate) fn probe(
        &self,
        live_prefix: PrefixKey,
        slate: &[ArmIdentity],
    ) -> Vec<SteeringProbe> {
        slate
            .iter()
            .map(|candidate| {
                let against = self
                    .no_goods
                    .iter()
                    .copied()
                    .find(|no_good| no_good.prefix == live_prefix && no_good.arm == *candidate);
                SteeringProbe {
                    // A round suppresses nothing: it forces a rank instead.
                    hit: self.forced_rank.is_none() && against.is_some(),
                    candidate: *candidate,
                    live_prefix,
                    against: against.copied().or_else(|| {
                        // Nothing matched. Report the sealed row this terminal
                        // DOES carry, so the log shows which half disagreed
                        // rather than only that something did.
                        self.no_goods.first().map(|no_good| **no_good)
                    }),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(forest_digest_before: u64) -> TerminalRouteEvidence {
        TerminalRouteEvidence {
            physical_contact_id: 7,
            terminal_id: 7,
            original_key: crate::forest::MemberKey::transit_test_fixture(1, 0),
            plan_digest: 11,
            loads_digest: 22,
            geometry_digest: 33,
            forest_digest_before,
            controller_digest_before: 55,
        }
    }

    fn winner(generation_index: usize) -> CcoCandidateEvidence {
        CcoCandidateEvidence {
            family: CandidateFamily::PlateDescent,
            generation_index,
            price_ml: 1.5,
            clearance_rows: 4,
        }
    }

    #[test]
    fn a_no_good_hits_only_its_own_prefix_and_its_own_arm() {
        let sealed = route(44);
        let mut steering = LegalizationSteering::default();
        assert!(steering.learn(NoGood::new(
            7,
            PrefixKey::of(&sealed),
            ArmIdentity::of(&winner(3)),
        )));
        let terminal = steering.for_terminal(7);

        let live = PrefixKey::of(&sealed);
        let slate = [ArmIdentity::of(&winner(3)), ArmIdentity::of(&winner(4))];
        let probes = terminal.probe(live, &slate);
        assert!(
            probes[0].hit,
            "the sealed arm at the sealed prefix must hit"
        );
        assert!(!probes[1].hit, "a different generation is a different arm");

        // One bit of the prefix moved: the world is not the one the refusal was
        // measured in, so the no-good MISSES rather than erring.
        let moved = PrefixKey::of(&route(45));
        assert!(!terminal.probe(moved, &slate)[0].hit);
    }

    #[test]
    fn a_no_good_never_reaches_another_terminal() {
        let sealed = route(44);
        let mut steering = LegalizationSteering::default();
        steering.learn(NoGood::new(
            7,
            PrefixKey::of(&sealed),
            ArmIdentity::of(&winner(3)),
        ));
        let other = steering.for_terminal(8);
        assert!(other.is_idle());
        assert!(!other.probe(PrefixKey::of(&sealed), &[ArmIdentity::of(&winner(3))])[0].hit);
    }

    #[test]
    fn a_round_forces_a_rank_and_suppresses_nothing() {
        let sealed = route(44);
        let mut steering = LegalizationSteering::default();
        steering.learn(NoGood::new(
            7,
            PrefixKey::of(&sealed),
            ArmIdentity::of(&winner(3)),
        ));
        steering.set_round(Some(ConfirmationRound {
            terminal_id: 7,
            arm_rank: 2,
            measured: Vec::new(),
        }));
        let terminal = steering.for_terminal(7);
        let slate = [ArmIdentity::of(&winner(3))];
        let (verdict, probes) = terminal.verdict(PrefixKey::of(&sealed), &slate);
        assert_eq!(verdict, SteeringVerdict::Forced { rank: 2 });
        // The arm the steering set names is measured, not passed over: that is
        // the whole point of a round.
        assert!(!probes[0].hit);
    }

    /// **The forbidden retirement, refused — design §1.3, revert proof 4's
    /// guard at the decision itself.**
    ///
    /// # Why this test has to exist
    ///
    /// The cadence retires a winnerless terminal on the spot:
    /// `grow_terminal_with_finalized_anchor`'s `None` arm calls
    /// `retire_unrouted_terminal`, SPEC §6 then removes that contact from the
    /// accepted set, and its carried area is repartitioned onto its
    /// neighbours. So if the steering channel simply dropped every suppressed
    /// candidate, a terminal carrying a no-good on each of its arms would
    /// retire — a real support gone, silently, under a clean seal, earned by a
    /// count of refusals each measured in a world that has since been replaced.
    ///
    /// That is the exact defect that made the second design review return
    /// FLAWED. Every retirement goes through a confirmation round instead.
    ///
    /// **Sabotage-shaped, deliberately.** Replace the `Spent` arm with the
    /// obvious-looking simplification — drop the suppressed candidates and let
    /// the tournament report no winner — and this test goes red on the
    /// retirement. The failure it catches is silent otherwise.
    #[test]
    fn a_slate_with_a_no_good_on_every_arm_is_spent_and_never_empty() {
        let sealed = route(44);
        let live = PrefixKey::of(&sealed);
        let mut steering = LegalizationSteering::default();
        let slate = [
            ArmIdentity::of(&winner(1)),
            ArmIdentity::of(&winner(2)),
            ArmIdentity::of(&winner(3)),
        ];
        for arm in &slate {
            steering.learn(NoGood::new(7, live, *arm));
        }

        let (verdict, probes) = steering.for_terminal(7).verdict(live, &slate);
        assert!(probes.iter().all(|probe| probe.hit));
        assert_eq!(
            verdict,
            SteeringVerdict::Spent { ruled_out: 3 },
            "a slate whose every arm carries a no-good must STOP the pass, not empty itself: \
             an empty slate is a winnerless terminal, and a winnerless terminal is retired"
        );
        assert!(
            !matches!(verdict, SteeringVerdict::Suppressed { .. }),
            "a spent slate must never arrive as ordinary suppression"
        );
    }

    /// Partial suppression is ordinary steering: the arms that carry a no-good
    /// are passed over, the rest are weighed, and the count is disclosed. No
    /// terminal is retired and no round is opened — there is still a choice.
    #[test]
    fn a_partly_suppressed_slate_keeps_its_remaining_arms_in_tournament_order() {
        let sealed = route(44);
        let live = PrefixKey::of(&sealed);
        let mut steering = LegalizationSteering::default();
        let slate = [
            ArmIdentity::of(&winner(1)),
            ArmIdentity::of(&winner(2)),
            ArmIdentity::of(&winner(3)),
        ];
        steering.learn(NoGood::new(7, live, slate[0]));
        steering.learn(NoGood::new(7, live, slate[2]));

        let (verdict, _) = steering.for_terminal(7).verdict(live, &slate);
        assert_eq!(
            verdict,
            SteeringVerdict::Suppressed {
                ruled_out: 2,
                keep: vec![false, true, false],
            }
        );
    }

    /// A slate every one of whose arms is *unknown* to the set is untouched —
    /// the miss is silent and safe, and `candidates_ruled_out` stays zero.
    #[test]
    fn a_slate_no_no_good_matches_is_suppressed_by_nothing() {
        let sealed = route(44);
        let mut steering = LegalizationSteering::default();
        steering.learn(NoGood::new(
            7,
            PrefixKey::of(&route(45)),
            ArmIdentity::of(&winner(1)),
        ));
        let slate = [ArmIdentity::of(&winner(1)), ArmIdentity::of(&winner(2))];
        let (verdict, _) = steering
            .for_terminal(7)
            .verdict(PrefixKey::of(&sealed), &slate);
        assert_eq!(
            verdict,
            SteeringVerdict::Suppressed {
                ruled_out: 0,
                keep: vec![true, true],
            }
        );
    }

    /// A round past the end of the slate is how a round ends in exhaustion:
    /// the verdict still says `Forced`, and it is the tournament's `nth` that
    /// finds nothing. Nothing here decides a retirement.
    #[test]
    fn a_round_rank_past_the_slate_is_still_a_round() {
        let sealed = route(44);
        let live = PrefixKey::of(&sealed);
        let mut steering = LegalizationSteering::default();
        let slate = [ArmIdentity::of(&winner(1))];
        steering.learn(NoGood::new(7, live, slate[0]));
        steering.set_round(Some(ConfirmationRound {
            terminal_id: 7,
            arm_rank: 9,
            measured: Vec::new(),
        }));
        let (verdict, probes) = steering.for_terminal(7).verdict(live, &slate);
        assert_eq!(verdict, SteeringVerdict::Forced { rank: 9 });
        assert!(
            probes.iter().all(|probe| !probe.hit),
            "the set is frozen during a round and suppresses nothing — the round exists to \
             RE-MEASURE the arms the steering passed over"
        );
    }

    #[test]
    fn the_abandon_rule_reads_the_set_growing() {
        let sealed = route(44);
        let mut steering = LegalizationSteering::default();
        let no_good = NoGood::new(7, PrefixKey::of(&sealed), ArmIdentity::of(&winner(3)));
        assert!(steering.learn(no_good), "the first insert grows the set");
        assert!(
            !steering.learn(no_good),
            "a repeat teaches nothing, and a pass that abandons on it must \
             refuse by name instead of looping"
        );
        assert_eq!(steering.len(), 1);
        assert_eq!(steering.counts_by_terminal().get(&7).copied(), Some(1));
    }
}
