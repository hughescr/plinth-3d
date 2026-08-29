//! # THE SEALED-INPUT RULE
//!
//! This module is the ONE place the digest-memo policy is written.  Every
//! cached fingerprint in this crate obeys the rule below, or it does not
//! exist.  A reviewer should be able to argue with the rule here, without
//! reading the walks it governs.
//!
//! ## What the rule claims
//!
//! A fingerprint may be computed once and served many times **only when every
//! byte it folds is *sealed*** — written exactly once, into the struct literal
//! that constructs its owner, and never assigned, pushed to, drained, or
//! mutably borrowed for the rest of that value's life.
//!
//! The rule is deliberately not "invalidate when something changes".  There is
//! no invalidation event, because a sealed input has no event to detect.  A
//! value whose inputs can change does not get a memo at all; it is refolded,
//! every time, forever.
//!
//! ## The three obligations — all three, or none
//!
//! 1. **Seal at birth.**  The cached word is produced by the same walk it
//!    replaces, over the same field values, in the same struct literal that
//!    places those values into the owner.  There is no lazy fill, no `OnceCell`,
//!    and no "is it warm yet" state, so there is no instant at which a cold or
//!    stale word can be served.  [`SealedDigest::seal`] is the only
//!    constructor and it is `const`, so the compiler will not let a seal be
//!    manufactured anywhere but at a construction site.
//!
//! 2. **Never inherit.**  A derived value re-seals from its own fields.  A
//!    cached word may be carried by `Clone` — a clone copies every folded field
//!    along with it, so it stays true — but it is NEVER copied from one value
//!    onto a different one.  In particular
//!    `SpecializedContactPlanEdit::derive_child_from` builds its child through
//!    a struct literal and re-seals there; it does not copy the parent's seal.
//!
//! 3. **Keep the check honest.**  A fingerprint has two kinds of reader.  Most
//!    readers *report* it: they stamp it into a transcript, bind it into a
//!    parent hash, or hand it to a preflight that only needs the value.  A few
//!    readers *check* it: they recompute a digest and compare it against a word
//!    sealed earlier, precisely to catch an authority that no longer matches
//!    its seal.  Serving a memo to a checker would make the comparison a
//!    tautology and silently retire the check.  So the checking sites — see
//!    `SpecializedContactPlan::validate_binding` and
//!    `::validate_edit_binding` — call the FULL WALK with the memo bypassed,
//!    and additionally assert that the memo equals that fresh walk.  Those
//!    sites run about four times per repair trial against about eighty
//!    reporting reads, so the check keeps every bit of its old strength while
//!    the memo still removes the great majority of the work.
//!
//! ## Why this is bit-identical, not merely equivalent
//!
//! Every fingerprint here is one running FNV-1a `u64`
//! (`state = (state ^ byte).wrapping_mul(0x100000001b3)`).  Such a chain is not
//! decomposable — you cannot hash pieces independently and combine them — but
//! its accumulator IS a valid resume point: given the state after byte *k*, the
//! same bytes *k+1..n* fed in the same order yield the same final word, by
//! construction.  So the two shapes the rule permits are:
//!
//! * **A sealed value.**  A finished `u64` that a parent folds as eight bytes at
//!   a fixed position (`GenericContactPlan::digest`, `SelectedContacts::digest`).
//!   Nothing about the parent's byte order changes; the same eight bytes arrive.
//!
//! * **A sealed prefix.**  The accumulator state reached after a leading run of
//!   the stream whose bytes are all sealed, with the mutable remainder refolded
//!   on every call (`SpecializedContactPlan`'s head resume — see
//!   `SpecializedContactPlan::digest`).  The cached state is literally the
//!   state those same bytes produce.
//!
//! Neither shape reorders a byte, adds one, or drops one.  A moved digest is
//! therefore proof of a bug in the seal, never a value to refresh.
//!
//! ## The sealed inputs in this crate
//!
//! | Owner | Sealed at | Shape | Why its inputs cannot change |
//! |---|---|---|---|
//! | [`crate::perception::GenericContactPlan`] | `perceive_generic_contacts` | value | fields are private, the type has no `&mut self` method, and it is replaced (never edited) when perception re-runs |
//! | [`crate::SelectedContacts`] | `select_contacts` | value | the immutable S1 hand-off: private fields, no mutator, replaced when selection re-runs |
//! | [`crate::specialized::SpecializedContactPlan`] head | both plan literals in `specialized` | prefix | the head fields are written once per literal and the module has no code that assigns or mutably borrows any of them afterwards; a repair edit produces a NEW plan through `derive_child_from` |
//!
//! Anything not in that table is refolded in full on every call, on purpose:
//! `Forest::digest` (slot recycling can splice a new member into the middle of
//! the order, and a member's bytes include its children list), `Forest::validate`,
//! the routed/projection/live-member/transcript/snapshot digests, and the
//! contact/load-partition/gate-evidence tail of the plan digest.

/// A fingerprint word — a finished digest, or an FNV accumulator resumed
/// mid-stream — computed once at the construction of the value it describes,
/// and served unchanged for that value's whole life.
///
/// This wrapper exists so that a seal is greppable and so that the rule above
/// has a type to hang on.  It carries no logic; the guarantee is the
/// discipline the module doc states, and the honest-check obligation
/// (obligation 3) is what makes that discipline load-bearing rather than
/// assumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SealedDigest(u64);

impl SealedDigest {
    /// Seal `value`.
    ///
    /// The only constructor.  Call it inside the struct literal that builds the
    /// owner, computing `value` from the very fields being placed into that
    /// literal — never from another instance's seal.
    pub(crate) const fn seal(value: u64) -> Self {
        Self(value)
    }

    /// The sealed word.
    pub(crate) const fn get(self) -> u64 {
        self.0
    }
}
