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
//!    tautology and silently retire the check.  So the checking site —
//!    `SpecializedContactPlan::validate_sealed_digests`, which both
//!    `validate_binding` and `validate_edit_binding` run — bypasses the SPAN
//!    memo and folds those bytes the long way, and first asserts that the memo
//!    equals that fresh walk.  It runs about four times per repair trial
//!    against about eighty reporting reads.
//!
//!    **What that check does NOT cover, stated exactly, because an earlier
//!    draft of this paragraph overstated it.**  `digest_full_walk` bypasses the
//!    span; it is not a walk of every byte.  Where it reaches a nested SEALED
//!    VALUE it folds that value's memo, so the nested seal is checked by its
//!    own owner or not at all.  Four sites therefore compare a memo against a
//!    stamp derived from the same memo, and are tautologies today:
//!
//!    * `hash_specialized_plan_authority` (`specialized.rs`) folds
//!      `generic_authority.digest()`, a memo, inside the "full" walk.
//!    * `validate_brace_preflight` (`runner.rs`) tests
//!      `self.generic.digest() != self.digests.generic`.
//!    * `validate_rewire_preflight` (`runner.rs`) tests the same pair.
//!    * `validate_accepted_contact_set` (`runner.rs`) judges
//!      `accepted.digest() != state.accepted_digest`.
//!
//!    These lost strength when the value seals landed.  They are recorded here
//!    rather than in a commit message because a reader asking "is this check
//!    real?" looks at the doctrine, not at history.  Closing them needs a
//!    release-available full walk at the value-seal checking sites; the
//!    `digest_full_walk` methods on `GenericContactPlan` and `SelectedContacts`
//!    are `#[cfg(test)]` today, so in a release build nothing re-derives either
//!    value seal from its fields.
//!
//!    The fifth exception is deliberate and predates the memo: the hot-borrow
//!    gate in `finalized_loads()`.
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
//! * **A sealed span.**  A contiguous run in the middle of a stream whose bytes
//!   are all sealed, skipped by remembering the accumulator on BOTH sides of it
//!   ([`SealedSpan`]).  Everything before the span is refolded on every call
//!   and must arrive at the state the seal was taken at, or the span is folded
//!   the long way; everything after it is refolded unconditionally.  See
//!   `SpecializedContactPlan::digest`.
//!
//! Neither shape reorders a byte, adds one, or drops one.  A moved digest is
//! therefore proof of a bug in the seal, never a value to refresh.
//!
//! ## The sealed inputs in this crate
//!
//! | Owner | Sealed at | Shape | Why its inputs cannot change |
//! |---|---|---|---|
//! | [`crate::perception::GenericContactPlan`] | `prepare_generic_contacts` | value | fields are private, the type has no `&mut self` method, and it is replaced (never edited) when perception re-runs |
//! | [`crate::SelectedContacts`] | `SelectedContacts::new` | value | the immutable S1 hand-off: fields private TO ITS OWN MODULE (`selected_contacts`), no mutator, replaced when selection re-runs.  The module boundary is the enforcement: while this type lived in the crate root its "private" fields were reachable from every module in the crate, so the seal rested on convention |
//! | [`crate::specialized::SpecializedContactPlan`] frozen bulk | both plan literals in `specialized` | span | the repair pool, the islands and the coverage samples are written once per literal and the module has no code that assigns or mutably borrows any of them afterwards; a repair edit produces a NEW plan through `derive_child_from` |
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

/// A sealed SPAN of an FNV-1a stream: the accumulator on both sides of a
/// contiguous run of bytes that the SEALED-INPUT RULE says cannot change.
///
/// # THE INVALIDATION RULE, in code
///
/// [`Self::resume`] is the whole rule.  A caller refolds the stream up to the
/// span the long way, hands the arriving accumulator here, and is allowed to
/// skip the span ONLY if that accumulator is bit-for-bit the one the seal was
/// taken at.  Any difference at all — in any byte, anywhere before the span —
/// and the caller is told to fold the span itself.
///
/// So the span is skipped only when everything ahead of it is provably
/// unchanged, and the cheap check is not a summary of the prefix: it IS the
/// prefix, folded.  The one thing this cannot see is a change INSIDE the span,
/// which is why a span may only be declared over fields the rule's obligation 1
/// covers, and why obligation 3 requires a checking site that folds the span in
/// full at every stage boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SealedSpan {
    entry: SealedDigest,
    exit: SealedDigest,
}

impl SealedSpan {
    /// Seal a span: `entry` is the accumulator arriving at its first byte,
    /// `exit` the accumulator leaving its last.  Both must be observed on the
    /// same walk, at construction of the value that owns the span's bytes.
    pub(crate) const fn seal(entry: u64, exit: u64) -> Self {
        Self {
            entry: SealedDigest::seal(entry),
            exit: SealedDigest::seal(exit),
        }
    }

    /// The accumulator on the far side of the span, but only for a walk that
    /// arrived exactly where the sealed walk arrived.
    ///
    /// `None` means "fold the span yourself" — never "the value is wrong".
    /// Answering `None` is always safe; answering `Some` is safe exactly
    /// because the entry states matched.
    pub(crate) fn resume(self, arriving: u64) -> Option<u64> {
        (arriving == self.entry.get()).then(|| self.exit.get())
    }
}
