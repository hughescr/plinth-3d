# Unrouted-Contact Mesh Classification — Weeping Treant v1.2

Finishes the mesh-dependent half the pickle-only forensics honestly marked
BLOCKED. Engine `engine.run` cfg `SupportConfig(lift_mm=3.0)`. Real mesh:
`audit/treant_real_copy.stl` (576,026 faces), centered
`-(bounds_min+bounds_max)/2` so all coordinates match the pickle/engine frame.
Per-contact data: `audit/unrouted_mesh_classes.json`.

## How the 195 positions were recovered (the pickle omits `contacts`)
The state pickle stores `unrouted` = 195 contact **indices** but not the
`contacts` list. I re-ran `engine.run` on the real-copy STL; because every RNG
in the pipeline is seeded (`seed=0`), the run is deterministic: it reproduced
the pickle's **195 unrouted indices EXACTLY** and its verify counts exactly
(28 unsupported_minima, 21 uncovered_island_births, 144 uncovered_islands,
plate_z −66.9757). `contacts[i]` for the 195 indices therefore gives the exact
positions/normals the engine used. This is a reproduction, not a re-derivation.

## Test method (ray-based, normal-agnostic, memory-bounded)
The STL is **non-watertight** and loaded `process=False`, so face winding is
inconsistent and `trimesh.contains()` is unreliable (both-sides-inside for 33
of 195, neither-side for 27). Two consequences drove the method:
- **Normal-agnostic:** every ray is cast from `p + dir*0.05mm`, so directions
  into solid self-terminate and directions into air escape — the stored contact
  normal (often flipped into the solid here) is never trusted.
- **Memory-bounded:** infinite rays over the full 576k-face mesh make trimesh's
  `ray_triangle` return every face as a candidate for every ray (a single
  120-ray batch ballooned to >7 GB / OOM-killed at 173 GB). Fix: a local-crop
  submesh (R=10 mm, KDTree on face centroids) for clearance/corridor, full mesh
  only for enclosure confirmation in tiny ray batches. Peak RSS 1.61 GB.

**Classes** (mutually exclusive) + one orthogonal annotation:
- **INTERNAL** — 0 of 300 full-sphere rays escape to infinity on the full mesh
  (enclosed / not visible from outside).
- **GROOVE-BLOCKED** — not internal, but no approach direction admits the
  thinnest support's swept envelope clear for the stub length: any support tip
  collides with adjacent model. Envelope radius `= min_shoot_r_mm(0.35) +
  standoff_mm(0.6) = 0.95 mm`; required clear reach `= stub_length_mm = 2.5 mm`
  (config.py:116,119,224; abs-thinnest shaft envelope 0.30+0.6=0.90 mm).
- **OPEN-BUT-UNREACHED** — an admissible corridor exists; routing just didn't
  reach it (the real routing-work class).
- **EFFECTIVELY-REDUNDANT** (annotation kept alongside) — nearest LIVE support
  member surface ≤0.5 mm (already held by a neighbor).

---

## Class counts

**195 unrouted contacts**
| class | count |
|---|---|
| INTERNAL | **78** |
| GROOVE-BLOCKED | **94** |
| OPEN-BUT-UNREACHED | **23** |

**49 quality-bearing residuals** (28 minima + 21 island births)
| class | count | of which |
|---|---|---|
| GROOVE-BLOCKED | **30** | 19 births + 11 minima |
| INTERNAL | **12** | 12 minima (incl. crown-tip #4/#5, #8) |
| OPEN-BUT-UNREACHED | **7** | 5 minima + 2 births |

**Redundancy overlay (195):** EFFECTIVELY-HELD(≤0.5mm) 89 · MARGINAL(0.5–1) 15
· WEAK(1–2) 23 · UNSUPPORTED(>2) 68.
Class × redundancy tells the story: **INTERNAL 64/78 are member-overlapped**
(a stub was placed but overlaps the model = buried); **GROOVE-BLOCKED 50/94 are
UNSUPPORTED>2mm** (wedged specks, no neighbor); **OPEN 13/23 are UNSUPPORTED>2mm**
(the genuine reach gaps).

---

## Craig's three questions, answered with counts (of the 195)

**(a) Genuinely unsupported spots that matter: 23** are OPEN-BUT-UNREACHED (a
support *could* be placed; routing didn't reach). Of these, **13 have no
neighbor within 2 mm** — the real reach-work items; the other 10 already have a
strut within 2 mm. In the quality-bearing 49, this class is **7**.

**(b) Weird internal parts: 78** are INTERNAL — genuinely enclosed. This is the
big correction the pickle-only pass could not see. **12 of the 49 residuals**
(all minima) are internal too.

**(c) Nested in grooves where any support would collide: 94** are GROOVE-BLOCKED
— external, but so tightly nested that even the thinnest strut+standoff (0.95 mm
envelope) can't approach within the 2.5 mm stub without hitting adjacent model.

78 + 94 + 23 = 195.

---

## The 78 INTERNAL contacts — what they actually are (sanity check)

The pickle-only pass said "no evidence of internal parts." That was correct **for
the 49 residuals it could see** (it inferred from tiny birth-footprint areas that
those were external specks). It could not see that a large fraction of the full
195-contact set sits on **interior** surfaces — the contacts list wasn't in the
pickle and the mesh was unreadable.

These are **not perception noise and not a sampling artifact** — I stress-tested:
- **Denser rays don't open them:** every internal point sampled has **0 escapes
  at 300, 800, AND 2000 directions** (4.5° resolution — no slot a support could
  thread was missed). Controls scale correctly: the stranded overhang #2 escapes
  87→226→576 as density rises, far-exterior 297→1995, model-center (0,0,0)
  833 (it's open air between branches, correctly *not* internal).
- **They are buried under stacked shells:** vertical shell-crossing counts show
  the points sandwiched inside many interpenetrating surfaces — e.g. the 500 mm²
  island at (4.2,−7.8,58.5) sits under **238 downward crossings**; (3.0,−7.9,50.2)
  under **261 upward crossings**; (0.3,−7.5,46.4) under 45 down.

**Interpretation:** the STL is a **non-watertight assembly of overlapping organic
primitives** (branches / bark / foliage clumps that interpenetrate). The engine's
face-based overhang/island perception seeds contacts on down-facing faces that
are actually **interior to the solid union**. They print as solid, so they are
**not a print risk** — but they are genuinely enclosed, and they **inflate the
unrouted / unsupported-minima / island-birth counts**. They cluster in a handful
of spots: upper-center crown (0–5, −8, 46–59), lower-left trunk (−33, 15, −21),
mid-left (−19, 4, 41), i.e. exactly where branch masses overlap.

---

## Top-10 risk list — mesh re-verdict

| # | type | (x,y,z) | mesh class | verdict vs pickle-only |
|---|---|---|---|---|
| 1 | birth | (−37.6,12.4,−20.0) | GROOVE-BLOCKED (ext) | **confirm speck**, but a strut can't fit (corr 0.90mm) — manual support would collide |
| 2 | min | (32.1,−12.1,43.7) | GROOVE-BLOCKED (ext, esc 87→576) | **CONFIRM top real risk** (stranded ~10mm). Mesh adds: tight clearance — fine tip only |
| 3 | birth | (17.7,12.6,23.6) | GROOVE-BLOCKED (ext) | confirm speck, corr 0.51mm — wedged |
| 4 | min | (36.2,3.9,52.7) | **INTERNAL** (0/2000 esc) | **DOWNGRADE — buried interior, not an overhang** |
| 5 | min | (36.3,3.8,52.7) | **INTERNAL** (0/2000 esc) | **DOWNGRADE — twin of #4, interior** |
| 6 | birth | (−7.6,10.0,52.6) | GROOVE-BLOCKED (ext) | confirm tiny speck, corr 0.08mm |
| 7 | birth | (35.5,−1.2,−61.5) | GROOVE-BLOCKED (ext) | confirm, near plate (z+5mm), corr 2.23mm |
| 8 | min | (−16.0,−1.0,−18.2) | **INTERNAL** (0/2000 esc) | **DOWNGRADE — buried interior, not a flank overhang** |
| 9 | min | (11.6,−19.0,32.2) | **OPEN-BUT-UNREACHED** | **UPGRADE to genuine reach gap** (supportable, unheld 3.95mm, corridor clear) |
| 10 | birth | (9.3,−12.9,1.9) | OPEN-BUT-UNREACHED | confirm **low risk — neighbor holds it** (0.46mm) |

### The two manual-support recommendations
1. **Priority-1: stranded overhang (32.1,−12.1,43.7) → CONFIRMED.** External and
   genuinely stranded (escapes grow with ray density; nearest live member 10.24mm).
   **Caveat the mesh adds:** local clearance is tight — the standard 0.95mm strut
   envelope collides within ~0.8mm — so use a **fine-tip** manual support at a
   favorable angle rather than a fat one.
2. **Priority-2: crown-tip cluster (36.2,3.9,52.7) → RETRACT.** #4 and #5 are
   **INTERNAL** (0 escapes at 2000 directions, buried in interpenetrating crown
   geometry). They are not exposed overhangs; they cannot and need not be
   supported. The "float" flagged here was a perception artifact of overlapping
   geometry.

**Net:** of the two manual supports, one is confirmed (with a fine-tip caveat)
and one is retracted. The external work that *does* remain is the
OPEN-BUT-UNREACHED set (below), led by #9.

---

## NEW risks the mesh reveals (invisible to the pickle-only pass)

1. **Non-watertight, self-interpenetrating mesh.** 78/195 unrouted contacts and
   12/49 residuals sit on buried interior faces of overlapping primitives. Two
   takeaways: (a) ~40% of the "unrouted" set and ~⅓ of the "unsupported minima"
   are **non-actionable interior geometry**, so the real support-gap problem is
   much smaller than the raw counts suggest; (b) a self-intersecting STL is worth
   a **mesh-repair / boolean-union pass before slicing** (and the engine's
   perception would benefit from a solid-union/visibility filter so it stops
   seeding contacts inside the model).

2. **A genuine BROAD down-facing overhang that is OPEN and unreached** — exactly
   the "large island could be a real overhang, not grown-from-speck" case the
   pickle-only pass flagged as needing the mesh. **(−19.5, 13.2, −11.7)**, island
   area **77.3 mm²**, air-normal z = −0.95 (facing straight down), ~55mm above
   plate, corridor clear to 2.5mm, straight-down path to plate clear, **no
   neighbor within 3.7mm**. A second, smaller one at **(3.8,−6.9,−10.2)** (36.5
   mm², down-facing, OPEN). These are real, previously-unconfirmed support gaps —
   worth a manual support each (lower-left/center, ~55mm up).

3. **#9 (11.6,−19.0,32.2)** upgraded to a genuine OPEN reach gap (supportable,
   unheld) — the router simply didn't reach it. Add to the manual-support list
   ahead of the retracted crown-tip pair.

## Bottom line
- The raw 195 splits **78 interior / 94 groove-wedged / 23 open**; the interior
  and groove classes are dominated by non-actionable buried geometry and wedged
  specks. **The genuinely actionable set is small: ~13 open-and-unheld contacts**,
  of which the standouts are the stranded overhang #2, the broad down-facing
  overhang at (−19.5,13.2,−11.7), and #9.
- **Manual-support list, revised:** (1) #2 stranded overhang (fine tip);
  (2) broad overhang (−19.5,13.2,−11.7); (3) #9 (11.6,−19,32.2). **Drop** the
  crown-tip pair (#4/#5) — it's interior.
- **Print-risk overall stays low**, and is *lower* than the raw residual counts
  implied once interior phantoms are removed — but fix the self-intersecting mesh
  before slicing.

## Limitations
- INTERNAL is a straight-line visibility test at ≤4.5° resolution; a hypothetical
  escape channel narrower than that would be missed, but such a channel is also
  un-threadable by any real support, so the print conclusion is unchanged.
- GROOVE clearance uses a 6-direction corridor probe with a 6-ray envelope ring;
  it can under-count a lucky approach angle a human might find in ChituBox
  (hence "fine tip at a favorable angle" for #2), but never over-states room.
- `nearest_member_surface_mm` reproduces the independently-verified pickle proxy;
  negative values = a live member overlaps the contact.
