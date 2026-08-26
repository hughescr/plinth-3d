# Synthetic geometric-defect fixtures — provenance and reasoning

The four `.stl` files beside this document are the committed inputs to
`crates/plinth-hollow/tests/hollow_chain_end_to_end.rs`. Each is a small,
deterministic solid that targets ONE named defect from the remediation review
(`REVIEW.md`, "short-list" numbering) exactly, rather than a sculpted or
scanned model. Small FILE, large OBJECT: every fixture is a dozen-to-a-hundred
triangles but is dimensioned in tens of millimetres, so it actually clears the
hollowing gates it needs to clear.

## Why this is prose and not a script

These fixtures were first cut with a Blender 5.2.1 script
(`blender --background --factory-startup --python generate.py -- <fixture> <out.stl>`).
That script is **not** kept in the tree. `crates/plinth-verify/tests/no_python_guard.rs`
enforces the owner's standing rule from
`docs/engineering-journal/decisions/2026-07-28-rust-rewrite.md` — no new Python
is written here, and the guard is a tree check precisely so that a one-off
generator cannot take root next to the Rust. Blender's only scripting interface
is Python, so a Blender generator cannot satisfy that rule and cannot live here.

Nothing is lost at test time: the `.stl` files are committed, so a clean
checkout runs the whole suite with no generator present. What this document
carries is the part that mattered — the exact box table each fixture was built
from, and the reasoning and measurements behind each one. The box dimensions
below are the source of truth that `hollow_chain_end_to_end.rs` restates as
constants; they are inputs to the tests, never expected outputs of the engine.

**Open item for the owner.** Regenerating these fixtures from the tree is not
currently possible. Rebuilding the generator as a Rust example that emits the
STLs directly would restore that, but the two boolean-union fixtures would need
CSG the workspace does not have, and re-cutting them would change the committed
meshes the measurements below were taken on. Left as a deliberate gap.

## Registered constants the dimensions derive from

Read live from `crates/plinth-core/src/constants/placeholders.rs`; restated
here only for the reasoning, never as a second source of truth.

| Constant | Value | Role |
| --- | --- | --- |
| `HOLLOW_SAVE_MIN_ML` | 1.5 mL | savings gate (the whole gate since 2026-08-13) |
| `HOLLOW_BULK_MIN_MM` | 10.0 mm | bulk-size policy |
| `HOLLOW_WALL_DEFAULT_MM` | 2.5 mm | wall target |
| `HOLLOW_WALL_MIN_MM` | 2.0 mm | hard minimum wall (never lower this) |
| `HOLLOW_PITCH_MM` | 0.5 mm | hollowing voxel pitch |
| `HOLLOW_EXTRA_DRAIN_POLICY.min_pocket_ml` | 0.5 mL | audit-rescue size minimum |
| `HOLLOW_EXTRA_DRAIN_POLICY.max_extra_drains` | 3 | audit-rescue budget |
| `HOLLOW_EXTRA_DRAIN_POLICY.exit_max_mm` | 8.0 mm | audit candidate ray length |
| `NEAR_WALL_SKIP` (`rust/geom_kernel/src/consts.rs:35`) | 0.05 mm | near-wall crossing skip |

Boxes below are written `box(size_x, size_y, size_z, centre_x, centre_y, centre_z)`,
all in millimetres.

---

## 1. `cube.stl` — the baseline

12 triangles, 684 bytes. NOT generated: the owner's supplied 20 mm cube,
copied in from outside git, sitting on the plate at Z 0..20.

It is the baseline. Hollowed at the 2.5 mm wall target it carves a 15 mm cubic
cavity, 15³ = 3375 mm³ = 3.375 mL saved, clearing every gate (savings, bulk,
wall) with no drain-planning wrinkle at all — one chamber, one drain, chain
runs end to end.

---

## 2. `legs-uneven.stl` — short-list item 3

Boolean union of three boxes:

```
body  = box(40, 40, 20,   0, 0, 50.0)
leg_a = box(12, 12, 40, -12, 0, 20.0)   # Z 0..40, reaches the plate
leg_b = box(12, 12, 15,  12, 0, 32.5)   # Z 25..40, stops short
```

**What it targets.** Drain sites are searched only around each CHAMBER's single
global low point (`drains.rs`'s `low_lobes`, `band_mm` = 15 mm), but a
6-connected "chamber" can span multiple legs of very different length. Body and
both legs are ONE 6-connected cavity chamber, so `low_lobes` only admits
planned-drain candidates within 15 mm of the chamber's single lowest point —
the long leg's cavity bottom, at `HOLLOW_WALL_DEFAULT_MM` (2.5 mm) above Z0,
i.e. about Z 2.5. The band therefore reaches about Z 17.5. The short leg's own
cavity bottom sits at about 25 + 2.5 = Z 27.5 — outside that band, so it gets
no planned drain candidate at all.

This is a SEPARATE mechanism from drilling's own downhill/level-restricted
reachability (`drilling.rs:1613-1710`, `drainability_map_admitted`): even
though the short leg's cavity IS physically connected to the body and hence,
transitively, to the long leg's cavity, it only reaches that connection by
rising from its own bottom (Z 27.5) up to Z 40 where it meets the body — i.e.
it hangs from ITS OWN TOP, with nothing below it but a dead end. Resin pools at
the short leg's own bottom under gravity and cannot rise 12.5 mm on its own to
escape into the body. So drilling's own trapped/audit-rescue pass, independently
of the `drains.rs` band gap, finds the short leg's cavity undrainable via the
sole planned hole.

**MEASURED, not assumed.** Run through `create_shell` → `plan_drains` →
`drill_planned_shell` with the registered M0 constants, this exact fixture plans
exactly the two holes short-list item 3 predicts (one Drain near the LONG leg's
base, X −12 Z 3.5; one Vent up in the body, nowhere near the short leg) —
confirming the planning-band gap is real — but it does NOT ship with the short
leg refilled solid. `prepare_audit_plan`'s own rescue pass finds the short leg's
trapped cavity, qualifies it (well over the 0.5 mL
`HOLLOW_EXTRA_DRAIN_POLICY` minimum) and a valid candidate direction out its own
thin wall, and drills it its own dedicated drain (`audit_added_drains` = 1,
`refilled_voxels` = 0, `drainable_voxels` == `cavity_voxels` — the whole model
ships hollow, both legs).

The `drains.rs` planning gap short-list item 3 names is real and this fixture
proves it directly. The "refilled to solid" CONSEQUENCE the review infers from
that gap is a separate claim this fixture does not currently reproduce, because
the later audit-rescue safety net (`qualifies_audit_drain` /
`select_exact_audit_candidate`, the same machinery short-list item 4 discusses)
evidently already covers a leg this large and this exposed. A fixture that
reproduces an actually UNRESCUABLE trapped limb — too small, or with no clear
line-of-sight exit within `exit_max_mm` — would still need to be built to
witness that consequence.

---

## 3. `cascade-pocket.stl` — short-list item 4

Boolean union of five boxes:

```
spine       = box(16, 16, 90,   0, 0, 45)
neck_w      = box( 6, 10,  6,  11, 0, 63)   # X   8..14,  Z 60..66
chamber_w   = box(10, 10, 10,  19, 0, 61)   # X  14..24,  Z 56..66
neck_mid    = box( 6, 10,  6, -11, 0, 43)   # X -14..-8,  Z 40..46
chamber_mid = box(20, 20, 20, -24, 0, 40)   # X -34..-14, Z 30..50
```

**What it targets.** `prepare_audit_plan` (`drilling.rs`) marks
trapped-and-unrescued cavity for refill in ONE relaxation pass
(`drilling.rs:1025-1040`) — it recomputes level-restricted drainability once,
excluding whatever a component's own audit-rescue attempt failed to cover, and
refills whatever still does not drain. `drill` then DEMANDS that this was
already a fixpoint (`drilling.rs:712-722`): it recomputes drainability against
the SAME final hole set and refill mask and requires the count to match exactly,
else `DrillingRefusal::TrappedCavity`.

**Geometry, built to exercise exactly that machinery.** A tall SPINE (16×16,
Z 0..90) carries the one planned drain near its base and is always fully
drainable on its own (a single straight monotonic column, no dead end). Two
chambers hang off its opposite faces through narrow, OFFSET necks — each neck
bridges in from the SIDE, above the chamber's own bottom, so each chamber has
real depth below where its neck actually enters (the same "hangs from its own
top" shape as fixture 2's short leg):

- **CHAMBER-W** (10×10×10, X 14..24, Z 56..66): its neck meets it only over
  Z 60..66, so its bottom (Z 56..60) is a dead end. At the 2.5 mm wall its
  cavity is roughly 5×5×7.5 mm = ~188 mm³ = ~0.19 mL — comfortably under
  `min_pocket_ml` (0.5 mL), so `qualifies_audit_drain` refuses it on size
  alone: it is never even offered a candidate ray, and step 3 refills it
  unconditionally.
- **CHAMBER-MID** (20×20×20, X −34..−14, Z 30..50): its neck meets it only over
  Z 40..46, so ONLY its Z 30..40 slice is a genuine dead end (the Z 40..50 slice
  above the neck already drains straight down through it to the spine, no rescue
  needed). That Z 30..40 slice's cavity is large enough (~1.7 mL) and close
  enough to the exterior (walls are 2.5 mm everywhere) to both qualify by size
  and find a valid candidate direction within `exit_max_mm` (8 mm), so it
  receives its own dedicated audit drain.

This constructs precisely the two building blocks short-list item 4 names — a
component disqualified and refilled outright, and a SEPARATE component that is
audit-rescued but only after the single relaxation pass has to work out which of
its own voxels the new hole actually reaches — in one branched, multi-chamber
cavity, which is the shape ("trunk plus limb plus limb") the review calls out as
the concrete trigger shape.

**MEASURED, not assumed.** This exact fixture, run through `create_shell` →
`plan_drains` → `drill_planned_shell` with the registered M0 constants, drills
successfully (no `DrillingRefusal`): 1 audit drain added, CHAMBER-W's 738 voxels
(0.092 mL) refilled, and the `drill()`-side recount of drainable voxels equals
`prepare_audit_plan`'s own retained-voxel count exactly (both 100,397).

Tracing why: `drainability_map_admitted`'s single pass is a full union-find
recompute over the WHOLE field for a FIXED exclusion set, not an incremental
patch, so it already finds the complete set of voxels unreachable given that
exclusion in one shot; excluding that whole set afterward cannot newly strand
anything that had a genuinely independent path, because `has_drain` only ever
propagates by OR into a shared root — a voxel already recorded as draining can
never have been draining *through* a voxel later found not to drain, on pain of
them sharing that root and therefore that same true/false value.

This fixture is kept, and is genuinely useful, as a live regression test of that
invariant (and of the audit-rescue-with-a-residual-dead-end path in general): it
should keep drilling successfully after any change to this machinery. It is NOT
(currently) a witness that reproduces short-list item 4's refusal outcome — that
would need either a genuinely different mechanism than the one reasoned about
above, or a construction this fixture's author did not find. Report that gap
rather than paper over it.

---

## 4. `thin-fin.stl` — short-list item 11

Two boxes JOINED, not booleaned:

```
bulk = box(20, 20,    20, 0, 0, 10.0)
fin  = box(10,  0.02, 15, 0, 0, 27.5)
```

**What it targets.** `NEAR_WALL_SKIP` (`rust/geom_kernel/src/consts.rs:35`,
0.05 mm) discards any second ray crossing within 0.05 mm of the first when
`cone_thickness` (`queries.rs:2930-2968`) measures far-wall thickness for the
support foot-ball breach predicate — `breach_one` filters hits with
`dd > nd + NEAR_WALL_SKIP`, so a genuine far wall thinner than that margin is
invisible to the predicate, which reports the NEXT surface behind it (or no
breach at all) instead. No witness geometry for this existed before this fixture
(REVIEW.md: "needs a constructed geometry satisfying the surrounding
precondition and was not built").

**Geometry.** A 20 mm cube BULK (identical to `cube.stl`'s hollowing arithmetic:
a 15 mm cavity at the 2.5 mm wall target, 3.375 mL saved, clears every hollow
gate on its own) carries a FIN on its top face — 10 mm wide, 15 mm tall, and
0.02 mm thick in Y: exactly half of `NEAR_WALL_SKIP`, so a ray passing through
it front-to-back has its far-side crossing well inside the skip margin of its
near-side crossing, and is a direct, deliberately minimal instance of the
precondition the predicate is supposed to bound. The two boxes are joined (not
booleaned) at their shared touching face — the same construction
`plinth_mesh`'s own `make_thin_fin_with` fixture uses for base+fin — so each
box's 12 edges stay independently manifold; no CSG is needed or wanted at
0.02 mm.

**INTENDED CONSUMER, and a MEASURED limit.** `NEAR_WALL_SKIP` is consumed by
`geom_kernel`'s own `NativeSurface` / `cone_thickness` / `breach_one` (exact
triangle-mesh ray casting, `rust/geom_kernel/src/queries.rs`), which this
fixture's raw mesh and vertices feed directly; that is the intended use, and the
bulk's own hollow-gate arithmetic above (identical to `cube.stl`, verified)
establishes that the carrying shape is a genuine, practical hollowing target and
not a contrived toy.

It is NOT the intended use to push this fixture (bulk+fin as ONE mesh) through
`plinth-hollow`'s own `create_shell` — measured directly: at the registered
`HOLLOW_PITCH_MM` (0.5 mm), the fin's 0.02 mm thickness is 25× below one voxel,
and `create_shell`'s own surface-nets reconstruction of the carved cavity comes
back with an extra spurious closed surface near the fin
(`DrillingRefusal::SourceSurfaceCount { found: 3, expected: 2 }` when the
drilled plan is pushed through `drill_planned_shell`). That is an honest,
separate finding — a sub-voxel-pitch feature breaking H0's own reconstruction,
orthogonal to the near-wall-skip question this fixture targets — not a defect in
this fixture's geometry, and not something a smaller or larger
sub-`NEAR_WALL_SKIP` thickness would avoid (anything satisfying "thinner than
0.05 mm" is, by construction, sub-voxel at the registered 0.5 mm pitch). Report
it; do not paper over it by thickening the fin past the value short-list item 11
actually asks for.
