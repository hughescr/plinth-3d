# Decision — the delicacy producer is built inside the orientation-search feature

**Ruling given 2026-08-10 by the owner (Craig Hughes).** The dense-orientation-search
design ([`02-target-architecture.md`](../02-target-architecture.md) D-12,
[`03-physics-and-constants.md`](../03-physics-and-constants.md) §3.8) carried exactly one
blocking question: the scar term it specifies needs a per-face delicacy field, and **no
producer-side delicacy evaluator existed**. Asked whether to ship the sweep with the scar
term disabled until a producer landed, or to build the producer inside this feature, the
owner selected **(b): build the producer-side delicacy evaluator inside this feature.**

The sweep therefore ships scoring **all four objective terms from day one**. There is no
`ScarTerm::Disabled` interim state, and no interim in which the fourth term is present in
the record and absent from the arithmetic.

## What the ruling overturns

The design as written planned to ship the scar term "present, wired, and **disabled by
declaration** until the delicacy producer lands", with a `ScarTerm::{Registered, Disabled}`
policy, a `ScarDisabled{reason}` stamp in every report, and a byte-identity test (T4)
making the omission honest rather than quiet. That plan is superseded: the producer lands
first, so there is nothing to declare disabled. `ScarTerm` as a *shipped* two-state policy
does not get built.

This does not retire T4's underlying property — that a zero scar weight must reproduce the
scar-free search structurally rather than arithmetically. That remains a requirement of
the sweep and is unaffected by this ruling either way.

## The five consequences, as ruled

1. **The evaluator lives in `plinth-mesh`, scoped to the `FaceTable` at the LOD being
   scored.** It implements the §3.8 Placeholder row — a curvature proxy and a thinness
   proxy blended at `DELICACY_CURVATURE_WEIGHT` / `DELICACY_THINNESS_WEIGHT` (0.6 / 0.4),
   the thinness proxy saturating at `THIN_FEATURE_MM` (1.5 mm). Placing it behind the
   `FaceTable` binding is what makes the recorded ruling at
   [`02-target-architecture.md:251-253`](../02-target-architecture.md) structural: the
   Python defect — delicacy taken from the full mesh while areas came from a 90 %-decimated
   proxy, so scar magnitudes moved with the decimation ratio — cannot recur, because the
   field and its evaluation mesh are one object and the LOD name is inside the field's own
   digest.
2. **Auditor independence is preserved.** There is no dependency in either direction
   between the producer evaluator and `plinth-metrics`'s `scar.rs`. The two remain
   independent implementations of the same intent, and `plinth-metrics` keeps auditing
   producer output. An audit that shared the producer's implementation would be checking a
   thing against itself.
3. **`prepare.rs`'s `vec![0.0]` feed to the support stage is replaced with the real
   field.** The standing zero-field defect closes in this feature.
4. **If real delicacy values move any pinned baseline, committed artifact digest, or
   corpus expectation, the repo's recorded re-pin protocol applies** — a decision-neutrality
   proof, or a documented from→to re-pin in the journal. Never a silent edit. Where the
   protocol itself demands an owner decision for a specific re-pin, the question goes on
   the follow-ups ledger and into the implementer's report rather than being guessed.
5. **No gate, cap, or bound moves.**

## Why the zero field was not a neutral default

`strut_model_anchor_max_delicacy` (0.5) rejects an anchor candidate whose face is **more**
delicate than the cap (`crates/plinth-support/src/anchor.rs`). An all-zeros delicacy field
does not mean "delicacy unknown" or "delicacy not consulted": it is the positive assertion
that no face on any model is delicate, and it leaves the cap structurally unreachable. The
support stage has carried the machinery to protect delicate geometry since it was ported,
and has been handed, on every model, a measurement that could never trip it. That is the
defect the ruling closes — and it is why the evaluator's failure is a typed pipeline
refusal (`PipelineError::Delicacy`) rather than a fallback to zeros. There is no
substitute value for a measurement that did not happen.

## What was built

`plinth_mesh::evaluate_delicacy(mesh, lod, settings, config)`
(`crates/plinth-mesh/src/delicacy_eval.rs`), filling the contract
`crates/plinth-mesh/src/delicacy.rs` has held since it was written.

- **Curvature** — the normalized absolute angle defect of the welded vertex,
  `|2π − Σ incident corner angles| / 2π`, averaged over the face's three corners. Scale
  free and dimensionless: a flat region scores 0, a cube corner exactly 0.25, a needle tip
  approaches 1. No neighbourhood radius, no percentile, no sampling — and so no constant
  beyond the 2π the definition itself supplies. Vertices where the incident faces do not
  close one manifold disc (boundary, non-manifold, duplicate, degenerate, point-only
  contact — read from `build_face_topology`'s own cut flags, the crate's sole topology
  implementation) score 0, because the defect identity does not hold there and its value
  would be an artifact rather than a measurement.
- **Thinness** — `1 − thickness / THIN_FEATURE_MM`, clamped into `[0, 1]`, where
  `thickness` is the distance from the face centroid, along that face's own inward normal,
  to the first surface that faces back at it (`n_hit · n_face < 0`). The facing-back test
  is what makes the hit the *opposite wall* rather than a neighbour across a convex crease,
  and it excludes the originating face without a self-hit epsilon, since a face's normal
  dotted with itself is `+1`.
- **The blend** — the convex combination `(w_c·curvature + w_t·thinness) / (w_c + w_t)`.
  At the registered 0.6 / 0.4 this is exactly the plain weighted sum; writing it as a
  convex combination means the field stays in `[0, 1]` by construction if the weights are
  ever re-registered, instead of relying on them summing to one.

**Batch-first (D-37).** The evaluation is a fixed sequence of whole-array passes: one
face-geometry pass, one topology build, one welded angle accumulation, one geometry-kernel
index build, **one** batched ray cast covering every face at once, and one blend pass.
There is no per-face spatial query anywhere. Every fold is serial in source index order,
so the result does not depend on how the kernel's index-parallel map-collect was scheduled.

**Determinism.** No sampling, so no seed; no wall clock; no unordered iteration feeding a
result. The field's digest is a pure function of (mesh, LOD, settings).

**Spatial work goes to the kernel.** Owner override O-5 makes `geom_kernel` the sole
spatial-geometry implementation, so the thinness proxy uses its batched `cast_rays` rather
than growing `plinth-mesh` a second BVH. `plinth-mesh` gains a `geom_kernel` dependency;
the direction is acyclic, since `geom_kernel` depends on no `plinth-*` crate.

## Constants

**None were registered, because none were needed.** The evaluator reads exactly three
already-registered Placeholder values — `DELICACY_CURVATURE_WEIGHT`,
`DELICACY_THINNESS_WEIGHT`, `THIN_FEATURE_MM` — through
`DelicacySettings::production()`, which is the sole registry reader in this crate for
them. Everything else the evaluator needs is supplied by its own definitions (2π) or by
the length scale already registered (the geometry kernel's grid pitch is the saturation
length, so there is not a second number to keep aligned with the first).

`DelicacyBuildConfig` is a **resource guard, not a registry member**, on the rule the
orientation design states for `OrientSearchLimits`: everything output-changing is a
registered Placeholder; a limit that can only refuse is not output-changing. Its single
field caps the geometry kernel's conservative, allocation-free peak index estimate. Over
the cap the evaluator refuses naming both the measured demand and the cap; it never
coarsens the evaluation to fit, because a cap reached is a finding.

The values these proxies produce are **Placeholder-classed guesses**, not measured truths.
The field selects where support arms may land. It never accepts a model — acceptance is
the `DefectReport`, and no scalarization weight is on that path.

## What the field actually measures, on committed geometry

Measured 2026-08-10 on the checked-in `corpus/baseline/*.stl` meshes, `dev-opt` profile,
before the hollow stage (so these are the source meshes, not the prepared ones the
pipeline scores). `w` is the blended weight the support stage reads; `>0.5` counts faces
above `STRUT_MODEL_ANCHOR_MAX_DELICACY`, the one decision the field drives.

| mesh | faces | time | mean `w` | max `w` | faces > 0.5 |
|---|---:|---:|---:|---:|---:|
| `Cylinder.prepped.oriented-solid` | 244 | 1.9 ms | 0.0097 | 0.0098 | 0 |
| `Sphere.prepped` | 4,760 | 12 ms | 0.0005 | 0.0008 | 0 |
| `make_box.prepped.oriented-solid` | 12 | 0.4 ms | 0.1500 | 0.1500 | 0 |
| `make_overhang_table.prepped` | 60 | 0.7 ms | 0.1500 | 0.1500 | 0 |
| `make_raking_blade.prepped` | 24 | 0.3 ms | 0.2314 | 0.5420 | 5 (20.8 %) |
| `make_thin_fin.prepped` | 24 | 0.3 ms | 0.2067 | 0.5500 | 1 (4.2 %) |
| `make_toed_slab.prepped` | 140 | 0.7 ms | 0.1138 | 0.4200 | 0 |
| `Weeping Treant v1.2.prepped` | 429,120 | 1.65 s | 0.2743 | 0.9369 | 58,192 (13.6 %) |

Two things this pins. **The field is not a zero field** — the geometry the corpus was
chosen to contain (a fin, a blade, a treant) measures delicate, and the flat boxes measure
the analytic 0.25 curvature and nothing else. **The cost is negligible against the stage it
feeds** — 1.65 s on the treant, whose support run is ~470 s, so ~0.35 %.

## Why no committed expectation moved

`cargo test --workspace` is green with the real field in place, and the reason is
attributable rather than lucky. The support stage's only delicacy-driven decision is
`strut_model_anchor_max_delicacy` (0.5): a candidate whose face is *more* delicate than the
cap is rejected. Every model the committed prepare fixtures seal — `make_toed_slab.prepped`,
`Sphere.prepped`, and the procedural `make_overhang_table` — stays under that cap on every
face, so no anchor candidate that was admissible under the old zero field is inadmissible
under the real one. That is asserted, not inspected:
`the_committed_prepare_fixtures_stay_under_the_anchor_delicacy_cap` in
`crates/plinth-pipeline/src/lib.rs` re-measures it on the prepared meshes and fails loudly
if any fixture ever crosses the cap, which is precisely when the recorded re-pin protocol
would be owed something.

**This is a statement about those fixtures, not about all geometry.** The treant carries
13.6 % of its faces above the cap. No test seals the treant, so nothing moved — but the
first run that does will see different anchors than the zero field would have given, and
that difference is a correction, not a regression.

## The one thing this landing deliberately did not close

`crates/plinth-support/examples/embed_incidence_census.rs` still feeds
`vec![0.0; faces]`. It is throwaway, feature-gated instrumentation that never runs in
`cargo test`, and its seven recorded OD-8 census digests are pinned in this journal.
Giving it the real field would move all seven. Under the recorded protocol
(`CURRENT-STATUS.md` `#fu10-repin-2026-08-08`) that is a **stop-and-report**: full
attribution first, then an owner ruling, then a re-pin measured from the merged tip —
never a silent edit folded into an implementation commit. So the harness is left as it
stands, and the question is opened on the follow-ups ledger rather than answered here.
The consequence, stated plainly: **the census harness now measures a support run
production no longer performs.**

## What this record does not claim

It does not claim the two proxies are the right physics; §3.8 already marks the row
"P — ad hoc, DO NOT PORT AS-IS", and this evaluator is a faithful, honest implementation of
an admittedly ad-hoc shape, not a calibration of it. It does not claim agreement with the
`plinth-metrics` auditor's independently-shaped field — the two are deliberately different
implementations, and a numeric agreement between them was never the point of keeping both.
