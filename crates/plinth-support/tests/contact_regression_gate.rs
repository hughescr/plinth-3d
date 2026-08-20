//! Structural regression gate for the owner's governing contact requirement.
//!
//! > "be sure that the new code properly attaches the tips to the mini for all
//! > cases" — owner, 2026-08-04
//!
//! PR-8 makes that requirement executable: `initial.rs` attests every stub it
//! mints, the OD-9 exterior-ness screen refuses buried surface, and the verdicts
//! are sealed into `PreparedSupportDigests::contact_attestation`.
//!
//! **Why this gate is structural rather than behavioural.** The measured corpus
//! delta of the whole attestation stage is ZERO placed geometry: 117 contacts
//! attested, none refused, and every `snapshot_digest` byte-identical. That is
//! the desired outcome — but it means that deleting the attestation entirely
//! would ALSO leave every existing behavioural test, every golden and every
//! forest digest green. An attested contact and an unattested one are the same
//! geometry. The requirement is therefore invisible to behaviour on today's
//! corpus, and the only way to notice its removal is to check that the wiring is
//! still there.
//!
//! This is the same reasoning, and the same technique, as
//! `retired_formula_guard.rs`: a rule the corpus cannot currently exercise needs
//! a guard that does not depend on the corpus exercising it.
//!
//! The digest chain does the complementary job at runtime — it catches an
//! attestation whose ANSWERS changed. This catches one that stopped being asked.

use std::path::{Path, PathBuf};

fn support_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn read(file: &str) -> String {
    let path = support_src().join(file);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("guarded source {file} must be readable: {error}"))
}

/// Strip `//`-style comments so prose naming a symbol cannot satisfy a
/// requirement that the symbol be CALLED.
///
/// Deliberately conservative in the same direction as the retired-formula
/// guard: it does not parse string literals or block comments, so it can
/// over-report but never under-report.
fn code_only(line: &str) -> &str {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return "";
    }
    match line.find("//") {
        Some(index) => &line[..index],
        None => line,
    }
}

fn code_of(file: &str) -> String {
    read(file)
        .lines()
        .map(code_only)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Live code only: everything above the crate's `#[cfg(test)] mod tests`.
///
/// The call-SITE counts below are claims about production wiring. A fixture
/// that calls the same kernel entry point to establish its own non-vacuity —
/// the OD-9 fixture asserts `inside` directly, precisely so the test cannot
/// pass for the wrong reason — is not a per-item production call and must not
/// be counted as one.
fn production_code_of(file: &str) -> String {
    let source = read(file);
    let end = source
        .find("\nmod tests {")
        .or_else(|| source.find("\n#[cfg(test)]"))
        .unwrap_or(source.len());
    source[..end]
        .lines()
        .map(code_only)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every clause the owner's requirement decomposes into, and the live-code
/// evidence that it is still wired.
///
/// Each row is `(file, needle, what its absence would mean)`.
const REQUIRED: [(&str, &str, &str); 28] = [
    (
        "initial.rs",
        "attest_initial_contacts(",
        "no stub is attested at all: the governing requirement is unmet again",
    ),
    // --- GROW-TO-ATTEST: a tip must be sized for BOTH duties ------------
    //
    // PR-8 probes attachment with a ray ring AT THE TIP RADIUS, so a tip sized
    // only by the load law and seated near a model silhouette edge can find
    // nothing but air and be refused for being SMALL. Deleting the remedy is
    // behaviourally visible on the toed slab (routed contacts fall 65 -> 64),
    // but the DISCLOSURE half is not: strip the abandonment record and every
    // healthy-fixture `is_empty()` assertion stays green while a run goes back
    // to dropping load in silence.
    (
        "designed_embed.rs",
        "pub(crate) fn grow_tips_to_attest(",
        "there is no growth remedy at all: a load-sized tip that cannot attest \
         is refused again, and the load it carried is abandoned",
    ),
    (
        "designed_embed.rs",
        "fn tip_growth_ladder(",
        "the growth ladder is gone, so growth has no principled sequence of \
         radii and no termination at the registered maximum",
    ),
    (
        "designed_embed.rs",
        "fn tip_refinement_scan(",
        "the descending refinement scan is gone, so the run emits the first \
         LADDER rung that attests instead of the smallest radius that does — a \
         scar up to a whole area doubling larger than physics asked for, and \
         invisible to every count-based assertion",
    ),
    (
        "initial.rs",
        "grow_tips_to_attest(",
        "the initial-contact stage no longer asks for the remedy, so a curable \
         NotAttached is quarantined and its load abandoned",
    ),
    // --- THE SCAR BOUND: Craig's ruling of 2026-08-20 --------------------
    //
    // A grow-for-attachment rescue may not leave a scar wider than
    // TIP_RADIUS_MAX_GROWN_SCAR_MM = 0.400 mm RADIUS (0.800 mm DIAMETER), his
    // measured field ceiling. Deleting the truncation IS behaviourally visible
    // today (the toed slab goes 64 -> 65 routed contacts and its digest moves),
    // so these three rows are belt-and-braces rather than the sole guard —
    // but a rename that silently routed the ladder back to the legacy 1.000 mm
    // maximum would otherwise be a one-word edit with no named guard, and this
    // is the same technique the neighbouring entries use.
    (
        "designed_embed.rs",
        "pub(crate) fn growth_ceiling_radius_mm(",
        "the truncation seam is gone: the ladder no longer takes the SMALLER \
         of the scar bound and the legacy maximum, so a rescue may leave a scar \
         wider than any support the owner has ever printed",
    ),
    (
        "designed_embed.rs",
        "scar_bound_radius_mm",
        "the growth request no longer CARRIES the scar bound, so the ceiling \
         falls back to the legacy maximum however the truncation is written",
    ),
    (
        "initial.rs",
        "scar_bound_radius_mm: tip_radius_max_grown_scar_mm",
        "the initial-contact stage no longer hands the REGISTERED scar bound to \
         the ladder, so the bound is registered but unread",
    ),
    (
        "initial.rs",
        "abandoned_mm2",
        "a refusal no longer records the load it abandons, which is the one \
         number that tells a refused support from one that vanished",
    ),
    (
        "initial.rs",
        "attest_members_batch(",
        "the attestation no longer reaches the batched verifier",
    ),
    (
        "initial.rs",
        "exterior_probe",
        "the OD-9 probe is no longer computed at the mint site, which is the \
         only place that knows the OUTWARD sense of the contact normal",
    ),
    (
        "initial.rs",
        "carried_mm2: seed.carried_mm2.get()",
        "MAJOR-7 has regressed: PR-4's area law is being handed zero load \
         again, and with zero load every interface is sufficient",
    ),
    (
        "designed_embed.rs",
        "surface.inside(&probes)",
        "the OD-9 exterior-ness screen is gone; buried surface would pass, and \
         it passes TRIVIALLY because an embedded tip's whole neighbourhood is \
         interior",
    ),
    (
        "designed_embed.rs",
        "ContactAttestationRefusal::BuriedSurface",
        "burial is no longer a refusal",
    ),
    (
        "designed_embed.rs",
        "crate::contact::area_sufficient(",
        "the attested interface is no longer graded against the H2 allowance",
    ),
    // --- F3: the two TOUCHING families ---------------------------------
    //
    // These need the gate even more than the stub family does. On today's
    // corpus neither family wins a tournament, so deleting their attestation
    // moves NOTHING -- not a member, not a digest, not a census cell. There is
    // no behavioural test that could notice.
    (
        "designed_embed.rs",
        "pub(crate) fn attest_seats_batch(",
        "the touching families have no batched attestation entry point, so a \
         brace model rung and a CCO model anchor would seat on unproven \
         surface again",
    ),
    (
        "brace.rs",
        "attest_model_rung_seats(",
        "the brace model rung is no longer attested at its seat",
    ),
    (
        "brace.rs",
        "designed_embed::attest_seats_batch(",
        "the rung attestation no longer reaches the batched verifier",
    ),
    (
        "cco.rs",
        "attest_model_anchor_seats(",
        "the CCO model anchor is no longer attested at its seat",
    ),
    (
        "cco.rs",
        "designed_embed::attest_seats_batch(",
        "the anchor attestation no longer reaches the batched verifier",
    ),
    (
        "cco.rs",
        "AnchorFootRefusal::seat_buried(",
        "OD-9 burial no longer refuses a model anchor; a buried seat would \
         re-enter the tournament",
    ),
    // --- FU-88: an unusable CANDIDATE costs the candidate, not the model ---
    //
    // Same argument as the F3 entries above, one step further. Measured on the
    // seven large minis: the screen discards 6 candidates on the Giant Toad, 54
    // on the swallowing Giant Toad and 576 on the Rotting Warhorse, and NOTHING
    // on the other four; every shape that emits a file today emits a
    // byte-identical one. So deleting either half of this moves no shipped
    // geometry and no shipped digest — it only restores the abort that stopped
    // those three from producing a file at all.
    (
        "designed_embed.rs",
        "fn screen_seat_row(",
        "candidate rows are no longer screened one at a time, so one unusable \
         candidate refuses the whole batch again and the model dies for it",
    ),
    (
        "designed_embed.rs",
        "ContactAttestationRefusal::UnusableGeometry",
        "an unusable candidate has no verdict of its own, so it can only be \
         reported by failing the batch",
    ),
    (
        "cco.rs",
        "AnchorFootRefusal::seat_geometry_unusable(",
        "an unusable model-anchor candidate no longer leaves the tournament \
         through the refusal channel, so it would either be committed or take \
         the model down with it",
    ),
    // --- TRIPWIRE: recorded during candidacy, ENFORCED at commit -------
    //
    // The CCO model anchor's `NotAttached` verdict is deliberately not a
    // candidacy refusal (the family is unseated until PR-7), so the ONLY thing
    // standing between an unattested anchor and printed geometry is the gate at
    // each winner-commit point. Both are unreachable on today's corpus -- the
    // family never wins -- so nothing behavioural would notice their removal.
    (
        "cco.rs",
        "AnchorFootRefusal::seat_not_attached(",
        "the terminal-growth commit gate is gone: a model anchor that WON its \
         tournament with a seat that attested NotAttached would be committed \
         into the forest instead of refused",
    ),
    (
        "rewire.rs",
        "winning_generation_index(",
        "the rewire winner is no longer elected through the commit gate, so an \
         unattested model anchor would be replayed and spliced into the live \
         forest",
    ),
    (
        "runner.rs",
        "extend_contact_attestation_chain(",
        "the three families' transcripts are no longer chained, so an \
         attestation that stopped running in ANY of them leaves no trace",
    ),
    (
        "brace.rs",
        "BraceFixedPointTermination::ContactRefused",
        "PR-9 has regressed: a refused contact would hard-error the pipeline \
         again, which OD-4 forbids",
    ),
];

#[test]
fn the_contact_attestation_is_still_wired_into_the_production_path() {
    let mut missing = Vec::new();
    for (file, needle, consequence) in REQUIRED {
        if !code_of(file).contains(needle) {
            missing.push(format!(
                "{file} no longer contains `{needle}` — {consequence}"
            ));
        }
    }
    assert!(
        missing.is_empty(),
        "the owner's governing contact requirement has been disconnected from \
         the production path.\n\
         This gate is structural because the requirement is behaviourally \
         invisible on today's corpus: the attestation refuses nothing, so \
         deleting it moves no geometry and every golden stays green.\n\
         See docs/engineering-journal/contact-physics-2026-08-04.md, sections PR-8, \
         PR-9, OD-9 and MAJOR-7.\n\
         Missing:\n  {}",
        missing.join("\n  ")
    );
}

/// A refusal must route to quarantine, never to an error return. This is OD-4's
/// requirement and it is the difference between "this contact is rejected" and
/// "this model cannot be prepared".
#[test]
fn an_attestation_refusal_routes_to_quarantine_rather_than_a_pipeline_error() {
    let code = code_of("initial.rs");
    assert!(
        code.contains("forest.quarantine("),
        "the attestation refusal path no longer quarantines"
    );
    assert!(
        code.contains("UnattestedInitialStub"),
        "the refusal evidence type is gone; a refusal would be unexplained"
    );
    // The ONE hard error this stage may raise is a refused CONFIGURATION -- a
    // layer height at or above the contact depth, which leaves no positive
    // extent for any contact in the run. Its presence is required; a SECOND
    // attestation-shaped error variant would be the OD-4 violation.
    assert!(
        code.contains("UnattestableContactDepth"),
        "the unattestable-configuration refusal is gone, so a run that can \
         attest nothing would silently attest nothing"
    );
}

/// Batch-first. The attestation must cost a fixed number of native calls per
/// stage, not one per contact.
#[test]
fn the_attestation_makes_no_per_contact_native_call() {
    let code = production_code_of("initial.rs");
    // The attestation gathers its requests, hands the whole slice to
    // `attest_members_batch`, and scatters the verdicts. A `.iter()` loop that
    // called the batch entry point per element would show up as the call
    // appearing more than once.
    assert_eq!(
        code.matches("attest_members_batch(").count(),
        1,
        "`attest_members_batch` has more than one call site in initial.rs, \
         which is how a batched stage becomes a per-item native call"
    );
    let designed = production_code_of("designed_embed.rs");
    assert_eq!(
        designed.matches("surface.inside(").count(),
        1,
        "the OD-9 parity screen is issued from more than one site; it must be \
         exactly one batch for the whole stage"
    );
    // F3. The touching families delegate to the same batched verifier, so the
    // same one-call-site rule applies to each of them. A `for candidate in ..`
    // loop that called `attest_seats_batch` per candidate is exactly how this
    // regresses, and it would show up as a second call site.
    for file in ["brace.rs", "cco.rs"] {
        assert_eq!(
            production_code_of(file)
                .matches("designed_embed::attest_seats_batch(")
                .count(),
            1,
            "`attest_seats_batch` has more than one call site in {file}, which \
             is how a batched stage becomes a per-candidate native call"
        );
    }
    assert_eq!(
        designed
            .matches("penetration_intervals_batch_bounded(")
            .count(),
        1,
        "the connectivity witness is issued from more than one site"
    );
}

/// The guard must be able to see what it guards against, or a rename would
/// silently disarm it.
#[test]
fn the_gate_detects_a_disconnected_requirement() {
    // Every guarded file exists...
    for (file, _, _) in REQUIRED {
        assert!(
            support_src().join(file).is_file(),
            "guarded source {file} is missing; the gate would silently pass"
        );
    }
    // ...and prose alone does not satisfy a call-site requirement.
    assert!(code_only("    // calls attest_members_batch( here").is_empty());
    assert!(code_only("/// see `surface.inside(&probes)`").is_empty());
    assert!(code_only("    let x = surface.inside(&probes);").contains("surface.inside(&probes)"));
    // ...and the production/test split is real: `designed_embed.rs` genuinely
    // calls `inside` twice in the file, once in production and once in the
    // OD-9 fixture's own non-vacuity assertion. If this ever stops being true
    // the per-site counts above are measuring the wrong thing.
    assert!(
        code_of("designed_embed.rs")
            .matches("surface.inside(")
            .count()
            > production_code_of("designed_embed.rs")
                .matches("surface.inside(")
                .count(),
        "the production/test split has stopped excluding anything, so the \
         call-site counts are no longer scoped to live code"
    );
}
