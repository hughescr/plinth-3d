# 07 — Post-Python Support Calibration Plate (ROADMAP)

## Status and boundary

This is the concrete post-Python task required by O-10 (owner clarification,
2026-08-01). It determines support lateral-load/deflection capacity and brace
behavior that the frozen Python support pipeline did not establish. That pipeline
is a non-crashing, non-viable legacy compatibility target, not safety evidence.

This task starts only after the O-6 Python-elimination gates. It is **not** a
deletion blocker and grants no pre-deletion metric waiver. Before any plate is
fabricated, an independent challenge must review and either approve or revise the
plate design, controls, hypotheses, scoring, and failure criteria. Record that
challenge disposition in the results note.

## Preregistration required before fabrication

Create a dated preregistration that fixes the following without post-hoc changes:

1. **Geometry manifest.** Assign immutable IDs and exact dimensions/files to every
   arm, cap, load-proxy, brace, coupon location, and orientation. Record member
   angle, free length, diameter, brace geometry, and orientation relative to the
   peel sweep.
2. **Factorial conditions.** Vary member angle, free length, diameter, and
   orientation to the peel sweep. Include brace/no-brace (and the declared brace
   geometry) so the result identifies brace behavior rather than assuming it.
3. **Controlled loading.** Hold the declared cap geometry and load proxy constant
   within each comparison. State the lateral-load/deflection proxy, its application
   point and direction, the failure definition, and the measurement method.
4. **Controls and repeats.** Predeclare control coupons, randomized/blocked plate
   placement as applicable, repeat count per condition, exclusion rules, and how
   failed prints versus failed coupons are reported. Repeats are evidence, not
   optional retries.
5. **Fixed production environment.** Use the Saturn 4 Ultra 16K with the unchanged
   production resin and production print profile. Record printer, resin lot, profile
   digest/settings, plate file digest, and print date for every run.
6. **Hypotheses and scoring.** Preregister directional hypotheses for lateral-load
   capacity, deflection, and brace effect; the scoring equations/thresholds; and
   how uncertainty and inconclusive outcomes are represented. Do not promote a
   condition to `Measured` merely because it prints or does not crash.
7. **Evidence package.** Capture raw measurements, setup and post-test photos,
   print/coupon failures, geometry-manifest references, and deviations. Publish a
   dated results note as the record of record, including the challenge disposition,
   all conditions/repeats, analysis, limitations, and resulting provenance decision.

## Acceptance of the calibration task

The task is complete only when the preregistered protocol was challenged before
fabrication, the plate was fabricated under the fixed environment, every declared
repeat is accounted for, and the dated results note is the record of record. A
result may remain unmeasured or inconclusive; it must never be converted into a
safety claim by compatibility parity alone.
