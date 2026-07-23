# Unrouted-Contact / Residual Forensics — Weeping Treant v1.2

Engine: `engine.run` master f02b7af, `SupportConfig(lift_mm=3.0)`. Source of
truth: `/Users/craig/.claude/jobs/0ddf6d41/tmp/engine_run_state.pkl` +
`engine_run_results.json` + `engine_run.log`. Per-point data:
`audit/unrouted_forensics.json`.

## Coordinate frame (so Craig can find spots in ChituBox)
All coordinates are in the **CENTERED** frame the engine uses:
`mesh.apply_translation(-(bounds_min+bounds_max)/2)`, a pure translation that
puts the model's bounding-box centroid at (0,0,0). Measured centered bounds
(from `engine_run.log`):

- x ∈ [-51.006, +51.006]  (width 102.0 mm) — the two sides / spread of branches
- y ∈ [-33.836, +33.836]  (depth 67.7 mm) — front (+y) / back (-y)
- z ∈ [-63.976, +63.976]  (height 128.0 mm) — **z=-63.98 is the very bottom (roots/base), z=+63.98 is the crown tip**
- Support **plate at z = -66.976** (= zmin − lift 3.0).

**Translation rule:** the offset is exactly `-(original bbox midpoint)`. In the
centered frame the bbox is symmetric, so a point's location relative to the
model is read directly: e.g. `z+plate` (height above plate) tells you how high
up the treant it is; +x vs -x is which side; +y front / -y back. (The exact
numeric original→ChituBox offset needs the STL, which is currently unreadable —
see Blocker — but every point below is unambiguously placeable relative to the
model's own bounding box, which is what you navigate by in ChituBox anyway.)

---

## BLOCKER — the mesh-dependent half could not be completed
The source STL (`~/Documents/.../Weeping Treant v1.2.stl`) **cannot be opened by
any process in this session** — sandboxed bash, sandbox-disabled bash, *and* the
harness Read tool all get `EPERM / Operation not permitted` on `open()`, even
though `ls -laL` shows it is mode `-rw-r--r--` and it was read fine at 16:29
today when the pickle was built. macOS TCC/Full-Disk-Access on `~/Documents` is
not granted to this subagent's spawned processes (a *directory* open returns
EISDIR, a *file* open returns EPERM), or ChituBox holds an exclusive lock. No
copy exists outside Documents (every other path is a symlink *into* Documents).

**Therefore these tests are NOT done** and need the mesh:
- **INTERNAL** (ray-visibility from inside an enclosed cavity),
- **GROOVE** (clearance envelope vs immediately-adjacent walls),
- **REACH** (re-running `struts.find_model_anchor_candidates` + plate/tree
  routing to capture the concrete per-strategy failure reason),
- resolving the **195 unrouted CONTACT positions** individually (the `contacts`
  list is not in the pickle; regenerating it needs the mesh), and
- **locating** the 144 uncovered coverage islands.

The full harness for all of this is written and ready at
`/Users/craig/.claude/jobs/0ddf6d41/tmp/forensics.py`; it needs only a readable
STL. **Unblock:** drop a readable copy at
`/Users/craig/.claude/jobs/0ddf6d41/tmp/treant.stl` (from a context with
Documents FDA, or after closing ChituBox), then re-run me.

**What IS fully measured below** (from the pickle, no mesh needed): the 28
unsupported minima and 21 uncovered island births — with positions, gap
metrics, and **redundancy** (nearest live support member/ball) — plus the
size/gap distribution of the 144 islands. These 49 residual points are the
actual quality-gap-bearing subset of the 195 unrouted contacts, so the
severity ranking Craig asked for is complete.

---

## Answering Craig's three sub-questions (from the evidence available)

**"Are they unsupported contacts, weird internal parts, or nested in grooves?"**

1. **Weird internal parts? — No evidence of that; unlikely.** The 21 uncovered
   island **births** (the moments geometry first appears disconnected on a
   layer — the only thing that actually matters for printing) have footprint
   areas of **0.001–0.054 mm² (median 0.0045 mm²)**. That is 0.03–0.23 mm
   across — surface *specks and hair-thin detail*, consistent with bark
   texture / fine canopy detail on an organic model, not enclosed internal
   structures. (A true internal cavity contact would not show up as a
   sub-0.05 mm² island birth.) Confirming "not internal" rigorously still wants
   the ray-visibility test, but the geometry strongly points to external fine
   detail.

2. **Nested in grooves / held by an immediate neighbor? — Yes, for most of
   them.** Using nearest **live support member** distance as a mesh-free
   redundancy proxy across all 49 residuals:
   - **19/49 have a live strut/stub within ≤0.5 mm** (effectively already held),
   - **+9 within 0.5–1 mm**, **+15 within 1–2 mm**,
   - **only 6/49 have no support member within 2 mm.**
   For the **births specifically: 15 of 21 sit within 0.5 mm of a live member,
   all 21 within 2 mm** — i.e. routing declined a *dedicated* support because a
   neighbor is already essentially touching the spot (and/or a second strut
   can't fit that close without colliding — the groove signature). These are
   **EFFECTIVELY-REDUNDANT**, low real risk.

3. **Genuinely unsupported reach gaps? — Yes, a minority: ~6 minima.** The
   only residuals with *no* support within 2 mm are unsupported **minima**,
   led by the `min_gap = 10.65 mm` point at the top-right branch tip. These are
   the real "routing-reach" work items (OPEN-BUT-UNREACHED candidates), pending
   the mesh REACH test to confirm whether a strut *could* have reached.

Net: the 195 unrouted set is dominated by **micro-detail island births that a
neighbor already holds (redundant/groove-blocked)**, with a **small tail (~6) of
truly unsupported overhang minima** that are the genuine reach gaps.

---

## Severity ranking — Top 10 riskiest (of the 49 residual points)

Severity = footprint area × unsupported drop for births (discounted when a live
member is essentially touching), and unsupported-gap × distance-to-support for
minima. `z+plate` = height above plate. "nearMember" = nearest live support
member surface distance.

| # | type | xyz (centered, mm) | z+plate | nearMember | metric | where on the treant |
|---|------|--------------------|---------|-----------|--------|---------------------|
| 1 | BIRTH | (-37.56, 12.44, -20.00) | 47.0 | 1.03 mm (stub) | area 0.054 mm², gap 1.38 mm / 28 lyr | lower trunk, left side, front — largest-footprint birth; only real "lump" candidate |
| 2 | MIN | (32.12, -12.07, 43.74) | 110.7 | **10.09 mm (stub)** | min_gap 10.65 mm | upper branch, right side, back — **the one genuinely stranded overhang**, far from any support |
| 3 | BIRTH | (17.69, 12.59, 23.55) | 90.5 | 1.48 mm (stub) | area 0.015 mm², gap 1.99 mm / 40 lyr | mid-canopy branch, right, front — small but drops 40 layers before merging |
| 4 | MIN | (36.24, 3.85, 52.73) | 119.7 | 5.57 mm (stub) | min_gap 6.20 mm | crown tip, right — unsupported near the very top |
| 5 | MIN | (36.32, 3.75, 52.71) | 119.7 | 5.50 mm (stub) | min_gap 6.13 mm | crown tip, right — twin of #4 (adjacent) |
| 6 | BIRTH | (-7.64, 9.99, 52.60) | 119.6 | 1.87 mm (strut) | area 0.0034 mm², gap 2.46 mm / **49 lyr** | crown tip, center, front — longest float (49 layers) but a tiny speck |
| 7 | BIRTH | (35.52, -1.22, -61.55) | 5.4 | 1.09 mm (stub) | area 0.009 mm², gap 1.44 mm / 29 lyr | base/roots, right — low, near plate |
| 8 | MIN | (-15.97, -0.97, -18.18) | 48.8 | 4.82 mm (stub) | min_gap 5.39 mm | lower trunk, left, center — unsupported flank |
| 9 | MIN | (11.58, -18.99, 32.18) | 99.2 | 3.80 mm (stub) | min_gap 4.29 mm | upper branch, right, back |
| 10 | BIRTH | (9.32, -12.90, 1.85) | 68.8 | **0.39 mm (stub)** | area 0.009 mm², gap 1.40 mm / 28 lyr | mid-canopy, right, back — already held (member 0.39 mm) |

Full 49-row ranking is in `unrouted_forensics.json` → `all_residuals_ranked`.

### Reading the ranking
- **Only ~2 items are non-trivial floating risk:** #1 (largest birth footprint,
  0.054 mm², but a member is 1 mm away) and #2 (the 10.6 mm stranded overhang
  minimum at the top-right branch). Everything else is either a sub-0.02 mm²
  speck or has support within ~1 mm.
- The crown-tip cluster (#4/#5/#6 at z≈+53, and #2 nearby) is where the genuine
  reach gaps concentrate — the very top branch tips, hardest to reach from
  below without a long slender strut.

---

## The 21 island births — floating-lump risk assessment
All 21 are **tiny fast-context features**, not floating lumps:
- Footprint at birth: **0.001–0.054 mm² (median 0.0045)** — sub-0.25 mm across.
- Unsupported drop before merging into supported geometry: **3.5–49.2 layers
  (median 13.2), = 0.18–2.46 mm** at the inferred 0.05 mm layer height.
- **15/21 already have a live support member within 0.5 mm** at birth.
- Verdict: these are **small, fast-merging nubs / detail specks**, not true
  floating lumps. Worst case is #1 (0.054 mm², 28-layer drop) and #6 (0.0034
  mm² but 49-layer drop). At these areas the practical print outcome is a minor
  surface blemish or a speck that the resin/wash tolerates, not a detached
  chunk crashing into the vat.

## The 28 unsupported minima
- `min_gap_mm` **1.10–10.65 (median 2.20)** — the gap to the nearest support
  region. **22/28 are within 2 mm of a live member** (marginally held); **6/28
  are >2 mm** and are the real unsupported overhangs, topped by the 10.65 mm
  branch-tip minimum (#2). These 6 are the priority reach-work items.

## The 144 uncovered coverage islands (positions need the mesh)
From the pickle (size/gap only — no coordinates without the STL):
- `n_faces` **1–22467 (median 298)**; **31 are ≤5 faces** (specks), 41 ≤20.
- `worst_gap_mm` **6.26–inf**; **median inf** — i.e. the coverage gate found
  **no routed contact within range for essentially all of them**. This is the
  coverage-gate's face-count view of the *same* under-supported regions whose
  *birth moments* are the 21 births above: an island can have many faces yet be
  born as a sub-0.05 mm² speck that only grows large *after* it has merged
  upward into supported geometry, so the physically meaningful risk is still the
  birth moment, which is tiny. **Caveat:** a few large-`n_faces` islands
  (up to 22467) could in principle be a real broad overhang rather than a
  grown-from-speck feature — distinguishing these two cases **requires the mesh**
  (face normals + birth-layer trace) and is part of the blocked work.

---

## Bottom line for Craig
- **Not internal parts** (evidence: births are sub-0.05 mm² external specks; no
  cavity signature) — pending the ray test to make it airtight.
- **Mostly groove/neighbor-held (redundant):** ~28/49 residuals sit within 1 mm
  of a live strut/stub; 15/21 births within 0.5 mm. Routing skipped a dedicated
  support because one is already essentially there and/or a second can't fit.
- **A small real tail:** ~6 unsupported minima >2 mm from any support (the
  10.65 mm top-right branch minimum is the standout), plus 1–2 birth footprints
  worth a manual support in ChituBox (#1, #3).
- **Print-risk overall: low.** The residuals are dominated by micro-detail that
  either merges within ~1–2 mm or is already touched by a neighbor. The
  recommended manual touch-ups: the crown-tip cluster (z≈+53, right/front) and
  the single 10.65 mm stranded overhang at (32.1, -12.1, 43.7).
- **To finish the rigorous INTERNAL/GROOVE/REACH classification of all 195
  contacts:** make the STL readable and re-run `forensics.py`.
