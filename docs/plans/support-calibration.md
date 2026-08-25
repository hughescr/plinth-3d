# Plan: support calibration

Physical calibration plates are how the engine's load constants stop being guesses. Two are
queued; neither has produced a promoted engine constant yet. The completed plates (5 and 6,
support-arm capacity) are recorded in
[`docs/engineering-journal/reference/`](../engineering-journal/reference/) (`plate-05-results.md`,
`plate-06-results.md`, code-pinned and never moved), not here — this page is only the work
still ahead.

## Why every plate is preregistered before it is printed

A plate is fabricated only after an independent challenge has reviewed and either approved or
revised its design, controls, hypotheses, scoring, and failure criteria — recorded in the
results note. The preregistration fixes, without post-hoc changes:

- a **geometry manifest** — immutable IDs, exact dimensions, and cryptographic digests for
  every arm, cap, load proxy, brace, coupon location, and orientation, pinned before
  fabrication;
- the **factorial conditions** actually varied (angle, free length, diameter, orientation,
  brace/no-brace), so the result identifies what it claims to;
- **controlled loading** — the load proxy, its application point and direction, the failure
  definition, and the measurement method, held constant within each comparison;
- **controls and repeats** predeclared, including how failed prints are distinguished from
  failed coupons — repeats are evidence, not optional retries;
- a **fixed production environment** — same printer, resin lot, and profile for every run in
  a comparison, recorded per run;
- **hypotheses and scoring thresholds** preregistered before any print, so a condition is
  never promoted to `Measured` merely because it printed or didn't crash; and
- a dated **results note** as the record of record, including the challenge disposition, all
  conditions and repeats, and the resulting provenance decision.

A result may remain unmeasured or inconclusive. It must never be converted into a safety
claim by compatibility-parity alone, and it is never promoted into an engine constant from a
single plate — that needs **three independently prepared plates** agreeing.

## Queued plate 1 — HOME-SUPPORT-CAL-P1

A home-printer process-screening protocol: does each of 76 specimens (vertical and inclined
arms across a span/radius matrix, braced and unbraced, plus removal-tile and marginal/strong
controls) survive the production print-and-cure workflow and a controlled lateral load, on
the Saturn 4 Ultra 16K in production resin. It is a **screening test**, not a
material-characterisation test — it does not by itself establish wet-green modulus, printer
peel force, or a general safe support policy.

**Non-negotiable safety boundary.** Never touch, measure, load, remove, or straighten a
resin-wet specimen. Print, drain, wash, fully dry, and fully cure the whole plate first, using
the resin manufacturer's normal workflow. Coupons are separated only after full cure, and only
the 20 specimens the manifest marks `detachable_for_loading=true` may be cut free.

**What the protocol requires, in outline** (full field protocol and blank data sheets:
[`home-support-calibration-p1-protocol.typ`](home-support-calibration-p1-protocol.typ), the
printable companion to this page):

1. Preregister and pin geometry digests, printer/profile, resin/process, and the visual
   ChituBox gate (opens clean, no repair/rescale/auto-support) before slicing.
2. After full cure, photograph and score every one of the 76 IDs
   (`present_intact` / `missing` / `cracked` / `permanently_bent` / `delaminated` /
   `brace_separated` / `catcher_engaged`) before any separation.
3. Verify the `HOME-CAL-LEVER-4TO1` loading fixture's actual arm ratio by caliper (nominal
   10.0 mm : 40.0 mm) before trusting it as a verified 4:1 lever — use the measured ratio if
   it differs.
4. Load each surviving coupon on an ascending mass schedule (20 g -> 320 g at the mass end,
   0.049 N -> 0.785 N at the specimen), ten load/unload cycles per surviving level, stopping
   immediately and permanently at the first audible crack, visible whitening, brace
   separation, or permanent set over 1.0 mm.
5. Report every plate run, including failed controls and descriptive-only runs. One currently
   printed plate is a valid screening run. It supports a safe-policy claim only if all four
   `CS` (must-survive) controls survive and at least one `CM` (deliberately marginal) control
   demonstrates the intended marginal sensitivity.

**Status.** Preregistered; not yet reported as run to completion in this repository. The
generated-plate and loading-fixture source artifacts, their commits, and their SHA-256 digests
are pinned in the field protocol linked above.

## Queued plate 2 — underside-angle coupon (task #66)

Measures the real critical overhang angle by printing, to replace the currently-guessed
`OVERHANG_ANGLE_DEG` constant with a measured one. The value worth measuring has been sized;
the plate still needs Craig's go-ahead to print.

## Blocked on calibration

**Task #60 — the missing sag/peel demand model.** There is currently no physics model for how
wide a support's coverage circle needs to be to stop resin sagging or peeling; building one is
blocked on a physical coupon-print calibration (not yet designed as its own plate).
