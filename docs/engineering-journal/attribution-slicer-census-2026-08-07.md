> **Provenance banner (added on landing, 2026-08-08).** This document is copied
> **verbatim** below the rule into `docs/rust-migration/`; nothing in the body was
> edited, corrected, or reformatted. It was produced by the census-attribution
> analyst attributing the FU-10 phase-C perception cutover at
> `feature/slicer-union` `2ea9996` against its parent `1e17c8c`, and was
> **adversarially verified by an independent exact GEOS overlay of all 196
> differing layers** before the owner ruled on it. It is the evidence record
> behind the owner ruling of 2026-08-08:
>
> > Asked *"How should the frozen digests be re-pinned?"*, Craig selected:
> > **"Re-pin all 4, accept merge (Recommended)"** — accepting the
> > `make_raking_blade` tooth-body merge: a 0.029886 mm-deep fusion prints as
> > fused at 0.05 mm layers, so the merged solid matches physical reality
> > (PR-1(a) precedent: sub-layer features are not resolvable).
>
> The ruling and its three discharged conditions are recorded in
> [`CURRENT-STATUS.md`](archive/STATUS-ARCHIVE-2026-08-06-2026-08-08-part2.md#fu10-repin-2026-08-08). Two of this
> report's own readings are superseded by more precise later measurement and are
> **left uncorrected here** (this document is append-only/verbatim); the
> corrections are recorded at the ruling site: the Finding 2 grid reading of
> `make_toed_slab` material gained, `88.043086 mm²` (a 600×600 grid, noise floor
> ≈1e-3 mm²/cell), is superseded by the exact-polygon figure `87.9727 mm²`
> (exactly the sum of the seven false-hole areas); and the Finding 1 shared-edge
> claim in the "Merge along a shared edge" row (body line 225) — *"every one has a
> ≥1.0 mm shared edge"* — is superseded by the measured shared-edge floor
> `0.7249 mm`, on `make_toed_slab` layer 244. That row's *conclusion* stands (no
> merge in this corpus rests on a point contact; 0.7249 mm is still ~14 layer
> heights and ~38 X pixels), but **the ≥1.0 mm floor as stated is wrong and must
> not be quoted**. Both corrections are recorded at
> [`§8.6`](slicer-union-design-2026-08-07.md) of the design and at the ruling site
> [`CURRENT-STATUS.md`](archive/STATUS-ARCHIVE-2026-08-06-2026-08-08-part2.md#fu10-repin-2026-08-08). Nothing else in
> the body below is amended. Everything from the horizontal rule immediately below
> this banner to the end of the file is the analyst's original, unedited text.

---

# FU-10 phase-C census attribution — `feature/slicer-union` @ `2ea9996`

**Analyst:** census-attribution analyst · **Date measured:** 2026-08-07/08
**Question before the owner:** re-pin or reject the four moved OD-8 census digests.
**This document is evidence, not advocacy.** Nothing was committed, nothing re-pinned. Both
worktrees are clean; the scratch parent worktree was removed and pruned.

## 0. Method and provenance

| item | value |
|---|---|
| tip | `feature/slicer-union` `2ea9996` — *feat(perception): FU-10 phase C — both stitch sites consume normalized layers* |
| parent | `1e17c8c` — *feat(layers): FU-10 phase B — plinth-layers per-layer union normalizer* |
| instrument | scratch `attr_dump_layers` in `perception::prepare_generic_contacts`, env-gated (`PLINTH_LAYER_DUMP`), dumping every print-layer `SliceSolid` — signed areas, vertex counts, all vertices at `{:.17e}` — plus `PLINTH_PASSTHRU_DUMP` (per-layer `passed_through` / fill rule) and `PLINTH_DOWNSTREAM_DUMP` (islands / island births / local minima). Identical code in both trees. |
| driver | `cargo run --release --offline --features census --example embed_incidence_census`, one model per process via `PLINTH_CENSUS_ONLY` |
| instrumentation removed | yes — both trees verified `git diff --quiet`, tip at `2ea9996`, scratch worktree `git worktree remove` + `prune` |

The parent build reproduced **all seven frozen OD-8 digests byte-for-byte**, so the parent
is a valid baseline and the reported movement is genuinely the cutover.

Corpus-wide sanity: the print-layer dumps for `Sphere`, `Cylinder` and `make_box` are
**byte-identical files** between parent and tip — not merely equal digests.

---

## Finding 1 — WHAT changed in the differing layers

196 layers differ across the four moved models. Every one of them is a **loop merge**; 7 of
them additionally carry a **parity correction**. Nothing else changed anywhere.

| change class | layers | models |
|---|---:|---|
| Two or more counter-clockwise loops merged into one solid (design case D1) | **189** | all four |
| Merge **plus** a legacy loop that had been mis-classified as a hole (parity correction) | **7** | `make_toed_slab` only |
| Previously-dropped unowned hole now present | **0** | — |
| Hole re-assigned from one owner to another | **0** | — |
| Loops split, lost, or newly appearing | **0** | — |

**Corpus-wide hole census — the single most informative number in this report:**

| | parent | tip |
|---|---:|---:|
| print-layer solids, all 7 models | 4,525 | 4,332 |
| print-layer **holes**, all 7 models | **7** | **0** |

The seven holes the legacy stitcher produced over the entire corpus are the seven this
report shows to be false. After the cutover the corpus contains no holes at all — which is
correct: none of the seven models has an interior void in any slice plane. The silent
unowned-hole drop and the bbox hole-owner tier were therefore **never exercised** by the
corpus; they are not part of this delta.

### 1.1 `make_overhang_table` — 80 of 686 layers, two bands

Bands `346–385` (z 17.325–19.275 mm, solids 5→3) and `546–585` (z 27.325–29.275 mm,
solids 3→1). Table top fusing with its legs.

| layer / z | parent solids (area mm², nv) | tip solids | absorbed sum vs merged |
|---|---|---|---|
| 346 / 17.3250 | `104.2342580994`(7), `18.4752086141`(8), `18.4752086141`(8), `18.4337139947`(7), `18.4337139947`(7) | `141.1016861000`(18), `18.4752086400`(8), `18.4752086400`(8) | `104.2342580994+18.4337139947×2 = 141.1016860888` vs `141.1016861000` → **+1.1e-8 mm²** |
| 546 / 27.3250 | `240.0000000000`(8), `18.4337139947`(7), `18.4337139947`(7) | `276.8674280000`(19) | `276.8674279894` vs `276.8674280000` → **+1.1e-8 mm²** |
| 366 / 18.3250 | `173.516…`(7) + two `9.196…` legs | one merged outer | shared boundary **4.0000 mm** per leg |

Shared-boundary measurement: each absorbed leg shares an **exactly 4.0000 mm** straight edge
with the table top. This is a full-width structural fusion, not a tangency.

**Collateral requantization:** in these 80 layers the two *disjoint* legs (at y ∈ [−14.617,
−9.998]) are structurally untouched but were re-emitted through the quantizer:
`18.4752086141 → 18.4752086400` (Δ area **2.59e-8 mm²**, max vertex movement **4.997e-9 mm**
— five picometres, exactly half a `SLICE_UNION_QUANTUM_MM`). 80 of 80 bystander solids moved;
0 stayed bit-identical. This is the per-layer (rather than per-solid) pass-through granularity
the phase-C record flags in §8.5, measured.

### 1.2 `make_toed_slab` — 90 of 369 layers, two bands, two mechanisms

Bands `40–99` (z 2.025–4.975 mm) and `215–244` (z 10.775–12.225 mm). The two toes growing
out of the slab's side.

**Mechanism A — merge (83 layers).** The toe abuts the slab along a shared vertical edge.

| layer / z | parent | tip | shared edge |
|---|---|---|---|
| 40 / 2.0250 | `140.2961154…`(7), `14.5197791…`(34) | `154.8158945…`(40) | 1.0153 mm |
| 45 / 2.2750 | `157.6166234888`(7), `15.8318160079`(34) | `173.4484395021`(34) | 3.4416 mm |

Sum vs merged at L45: `173.4484394966` vs `173.4484395021` → **−5.4e-9 mm²**;
independently grid-measured polygon intersection = **0.000e+00 mm²**. Genuine abutment.

**Mechanism B — parity correction (7 layers: 46, 47, 48, 50, 51, 225, 227).** Here the
legacy stitcher labelled *the same toe* a **hole**.

| layer / z | parent | tip | parent `area_mm2` | true filled |
|---|---|---|---|---|
| 46 / 2.3250 | outer `+161.0807251039`(7v) **hole `−15.9033129797`(34v)** | `+176.9840379805`(34v) | `145.1774121243` | `176.9840379805` |
| 47 / 2.3750 | outer `+164.5448267190`(7v) **hole `−15.9333340900`(34v)** | `+180.4781608923`(34v) | `148.6114926290` | `180.4781608923` |
| 225 / 11.2750 | outer `+240.0000000000`(8v) **hole `−4.6265454173`(34v)** | `+244.6265454092`(29v) | `235.3734545827` | `244.6265454092` |

Two things are wrong in the parent, not one:

1. **The "hole" is not inside its owner.** At L225 the outer's bbox is x ∈ [−0.4615, 7.5385]
   and the "hole"'s is x ∈ [7.5385, 9.5062] — they share exactly one edge; the hole lies
   entirely *outside* the solid it was attached to. Grid-measured intersection between the
   two loops: **0.000e+00 mm²** at L46, L225, and L45.
2. **Therefore `SliceSolid::area_mm2 = |outer| − Σ|holes|` disagrees with the polygon's own
   point-in-polygon region.** At L46 the parent *reports* 145.177 mm²; rasterizing the
   parent's own loops under its own fill semantics gives 161.081 mm² (the toe is simply
   absent); the true geometry is 176.984 mm². The legacy layer was internally inconsistent
   by 15.90 mm² and wrong against geometry by **31.81 mm²** — 18 % of the layer.

### 1.3 The parity probe, reproduced exactly

I re-ran the deleted `stitch_layer` classification rule (min-`PointKey` vertex → even-odd
`point_in_polygon` against every other loop) on the parent's own dumped loops. It reproduces
the parent's verdict on every layer tested. Adjacent layers, same physical feature:

```
parent L45 z=2.2750  toe probe = (-8.049965469547, -1.720822852057)
                     distance from probe to the slab's boundary = 1.11e-15 mm
                     point_in_polygon -> False  -> depth 0 -> SOLID
parent L46 z=2.3250  toe probe = (-7.963362929169, -1.842673368786)
                     distance from probe to the slab's boundary = 1.60e-15 mm
                     point_in_polygon -> True   -> depth 1 -> HOLE
parent L224 z=11.2250 probe 9.93e-16 mm from boundary -> False -> SOLID
parent L225 z=11.2750 probe 1.11e-15 mm from boundary -> True  -> HOLE
```

The probe point is, **by construction**, the loop's minimum-`PointKey` vertex — and for a
loop that abuts a neighbour along its minimum-x edge, that is *exactly the touching vertex*.
The legacy stitcher's containment query was therefore always evaluated at the single most
boundary-degenerate point available on the loop, at a distance of ~1e-15 mm from the
neighbour's edge. The answer is decided by f64 round-off in
`point[0] < (b[0]-a[0])*(point[1]-a[1])/(b[1]-a[1]) + a[0]`, and it flips between adjacent
0.05 mm layers on a continuously-growing feature. This is not a marginal case; it is a coin
toss, and 0.05 mm of z is enough to re-toss it.

### 1.4 `make_raking_blade` — 20 of 269 layers (`90–109`, z 4.525–5.475), and it is genuinely OVERLAPPING

| layer / z | parent | tip | sum − union |
|---|---|---|---|
| 90 / 4.5250 | `160.0000000000`(8), `0.3000039556`(8) | `160.2974321110`(16) | **2.571845e-3 mm²** |
| 95 / 4.7750 | `160.0000000000`(8), `0.3000039556`(8) | `160.2974321411`(17) | **2.571814e-3 mm²** |
| 109 / 5.4750 | `160.0000000000`(8), `0.0198733253`(7) | `160.0173014939`(15) | **2.571831e-3 mm²** |

The rake tooth's upper edge sits at y = −0.14738962 while the blade body's lower edge sits at
y = −0.17727587 — the tooth is **embedded 0.0299 mm into the body**. Shared-boundary length:
**zero** (there is no coincident edge); polygon intersection area, measured independently on a
4000×4000 grid: **2.5619e-3 / 2.5619e-3 / 2.5719e-3 mm²**, agreeing with `sum − union` to
grid resolution. The union's answer is arithmetically correct; the parent double-counted the
overlap sliver in `Σ area_mm2`.

> **Correction to the phase-C record.** §8.5 states: *"In every case the merged outer's area
> equals the arithmetic sum of the legacy loops it absorbed, to within 2e-8 mm² … An
> intersection of measure zero … is the signature of abutting, not overlapping, sub-solids."*
> That is **false for `make_raking_blade`**, and §8.5's own table falsifies it:
> `160.00000000 + 0.30000396 = 160.30000396` against a quoted `160.29743211`, a gap of
> 2.57e-3 mm² — five orders of magnitude outside the stated tolerance. `make_raking_blade`
> is an overlap case (design D1 proper), not an abutment case. The engine is right; the
> narrative is wrong and should be fixed before this is cited as attribution.
> (Secondary: even for the true abutment cases the 2e-8 bound is too tight in general —
> `make_toed_slab` L46 shows +1.03e-7 mm². The honest bound is ~perimeter × quantum,
> ≈ 4e-7 mm² for a 40 mm loop at 1e-8 mm.)

### 1.5 `make_thin_fin` — 6 of 432 layers (`166–171`, z 8.325–8.575)

| layer / z | parent | tip | shared edge | sum − union |
|---|---|---|---|---|
| 166 / 8.3250 | `160.0000000000`(8), `0.2516875550`(7) | `160.2516876000`(13) | **10.0000 mm** | −4.5e-8 mm² |
| 171 / 8.5750 | `160.0000000000`(8), `6.0251902469`(…) | `166.0251903000` | 10.0000 mm | −5.3e-8 mm² |

The 0.0252 mm-thick fin shares an exactly 10.0000 mm edge (x = −1.24093106) with the slab.
Textbook abutment; grid intersection 0.

---

## Finding 2 — Is the new geometry defensibly MORE CORRECT?

**Yes, on every change class, and the evidence is measured rather than argued.**

The decisive test: for all 196 differing layers I rasterized the parent's filled region
(`inside(outer) AND NOT inside(any of its holes)` — exactly what `area_mm2` and every
downstream point query assume) and the tip's, on a common 600×600 grid, and compared them.

| model | material LOST (parent filled, tip not) | material GAINED (tip filled, parent not) |
|---|---:|---:|
| `make_overhang_table` | **0.000000 mm²** | 0.000000 mm² |
| `make_toed_slab` | **0.000000 mm²** | **88.043086 mm²** |
| `make_raking_blade` | **0.000000 mm²** | 0.000000 mm² |
| `make_thin_fin` | **0.000000 mm²** | 0.000000 mm² |

**Not one square micron of material is lost anywhere.** The only region-level change in the
whole corpus is 88.04 mm² of solid toe *recovered* on `make_toed_slab` — the material the
parity flip had declared void.

Per change class:

| class | physical meaning | was the old behaviour a known fallback defect? |
|---|---|---|
| **Merge along a shared edge** (189 layers; edges 1.0–10.0 mm long) | Two contours that share a real boundary segment are one piece of material. Under the nonzero rule with `ẑ×n`-derived winding, they union — which is the definition of the fill for a union of solids. | Not a *named* Tier-1 defect, but it is the direct consequence of the legacy stitcher deciding incidence only at snapped vertices and never along an edge (design D1). No merge in this corpus rests on a point contact: every one has a ≥1.0 mm shared edge or a real area overlap. |
| **Overlap resolution** (`make_raking_blade`, 20 layers) | Two contours whose interiors intersect are one piece of material with the intersection counted once. The parent double-counted 2.57e-3 mm² per layer in `Σ area_mm2`. | Same class (D1/D5). The parent's *region* was already right; only its arithmetic was wrong. |
| **Parity correction** (`make_toed_slab`, 7 layers) | A solid toe was labelled a void, and the void was attached as a hole to a solid that does not contain it. Both the hole's own 15.9 mm² and an equal amount of the slab's area were removed from the reported layer. | **Yes — this is exactly the "arbitrary-vertex parity probe" defect** named in the design (F2, D3) and in the deleted code's own comment (*"a strictly interior probe is the better containment query … Normalizing parity itself waits on the fill-rule work"*). Reproduced numerically in §1.3. |
| Silent unowned-hole drop | — | **Not exercised**: 0 unowned holes on this corpus. |
| Bbox hole-owner guess | — | **Not exercised**: at L225 the bbox tier would have *failed* (`box_contains` is false — the hole's max-x 9.506 exceeds the outer's 7.538). Ownership came from tier 1, the same degenerate vertex probe. |

**Normalization invariants on the tip output, checked over all 4,332 tip solids in all seven
models:** 0 non-positive outers, 0 non-negative holes, 0 holes outside their owner, 0 nested
outers, 0 degenerate (<3 vertex) loops, 0 duplicate consecutive vertices, 0 sub-threshold
outers. Zero violations.

---

## Finding 3 — Does this explain the parked FU-7 `make_toed_slab` parity discrepancy?

# YES — it reproduces the parked number exactly.

FU-7's record (`archive/STATUS-ARCHIVE-2026-08-04-2026-08-05.md:424-429`, and *Provisional freeze: make_toed_slab*)
says containment-probe hardening was deferred mid-implementation *"because it moves the
`make_toed_slab` digest (member count **72 → 42**)"*, and that the freeze is PROVISIONAL
until it lands.

The failing specialized fixture at this tip:

```
toed slab:    counts actual=(42, 34304, 8)  expected=(72, 50688, 9)
raking blade: counts actual=(33, 34176, 8)  expected=(40, 35008, 11)
```

`plan.contacts().len()` moves **72 → 42**. That is FU-7's number, digit for digit, arrived at
from the other direction: FU-7 proposed to *harden* the containment probe; phase C **retires**
it, replacing parity with the union engine's structural shape hierarchy. Both land on 42.

Supporting evidence that the mechanism is the same one FU-7 named:

- FU-7 is titled *"vertex-probe vs interior-probe containment"*. §1.3 shows the vertex probe
  firing at 1e-15 mm from a boundary and flipping between adjacent layers — the exact
  degeneracy `interior_probe`'s doc comment describes (*"Anchoring a containment query on a
  contour vertex is boundary-degenerate"*), whose promotion was *"gated behind the fill-rule
  normalization work"*. This is that work.
- The affected model is `make_toed_slab`, and the affected layers are precisely the seven
  where the probe's verdict was parity-dependent.

**Scope caveat, stated plainly.** FU-7 also touches a *different* probe on a different
question: the OD-9 `BuriedSurface` screen on `make_toed_slab` contacts **#22 #26 #31 #59 #62**
runs through `surface.inside` in 3D, and the 2026-08-05 record explicitly disambiguates
(*"Both agreed with the independent parity sweep … That is evidence about these seats, not a
resolution of FU-7, which is about a different probe on a different question."*). Those five
contacts are **not** touched by this cutover — the 3D exterior screen is untouched code. So:
the FU-7 **freeze-blocking, digest-moving 72 → 42 containment-probe question is answered and
discharged here**; the OD-9 3D `inside` question that shares the FU-7 row is not, and should
be split out rather than closed by association.

---

## Finding 4 — DOWNSTREAM: did support placement change at the seal?

Measured, parent vs tip, on the census path (`run_support` → `verification_snapshot`), plus
instrumented perception counts:

| model | routed contacts | final live members | islands | **island births** | local minima |
|---|---|---|---|---|---|
| `make_overhang_table` | 73 → **73** | 261 → **261** | 1 → 1 | 5 → **5** | 4 → 4 |
| `make_toed_slab` | 64 → **64** | 179 → **170** (−9) | 3 → 3 | 7 → **3** (−4) | 3 → 3 |
| `make_raking_blade` | 32 → **27** (−5) | 89 → **72** (−17) | 2 → 2 | 29 → **9** (−20) | 2 → 2 |
| `make_thin_fin` | 17 → **17** | 41 → **41** | 1 → 1 | 1 → **1** | 1 → 1 |
| `Sphere` / `Cylinder` / `make_box` | unchanged | unchanged | — | — | — |

**Two of the four moved models change support by exactly nothing.** `make_overhang_table` and
`make_thin_fin` move their digests and produce byte-identical support: same contacts, same
members, same births, same minima. Their digests moved only because
`GenericContactPlan::digest()` hashes slice-layer loop vertices directly — including the
5-picometre bystander requantization of §1.1.

The two that do change are fully attributable, one removed birth at a time:

**`make_raking_blade` — 20 phantom births removed, and they are the 20 differing layers.**
Parent births 0–8 (layers 0, 82, 83, 84, 85, 86, 87, 88, 89) are **preserved bit-identically**
at the tip. Parent births 9–28 sit at layers **90, 91, 92, …, 109** — one per layer, the same
rake tooth re-born on every single layer, with footprint areas walking smoothly
`0.30000396 → 0.29639194 → 0.25835318 → … → 0.01987333`. Those are exactly the 20 layers where
the tooth becomes embedded in the blade body. A feature that is fused to the body is not an
island being born in mid-air; the parent nominated support for it 20 times. Removing them
costs 5 routed contacts and 17 live members. The tooth is still supported: its 8 genuine
births at layers 82–89, while it really was a free-standing solid, survive untouched.

**`make_toed_slab` — 4 phantom births removed, one per parity-flip run.** The hole-layers
form four maximal runs: `{46,47,48}`, `{50,51}`, `{225}`, `{227}`. The removed births sit at
layers **49, 52, 226, 228** — the layer immediately after each run, in every case:

| removed birth | z (mm) | footprint area (mm²) | preceded by hole-run |
|---|---|---|---|
| L49 | 2.475 | 15.872316568850739 | 46–48 |
| L52 | 2.625 | 15.552590758130954 | 50–51 |
| L226 | 11.325 | 4.371083508748554 | 225 |
| L228 | 11.425 | 3.835584103917618 | 227 |

Mechanism: while the toe was a phantom *hole*, the layer's filled region excluded it, so when
the toe reappeared as a solid one layer later it overlapped nothing in the layer below and
`find_births` minted an island birth. Four hole-runs, four phantom births, four-for-four. Cost:
9 live members. Routed contacts are unchanged at 64.

Aggregate: **26 of the 29 fewer support members corpus-wide are the direct removal of 24
phantom island births**, each one traceable to a named layer.

---

## Finding 5 — Are the three failing tests failing ONLY because of the upstream layer change?

Gate re-run at the tip, on a clean tree, default profile, `--no-fail-fast`:

```
cargo test --workspace --offline --no-fail-fast
  TOTAL 1016 passed / 3 failed / 0 ignored     (matches the phase-C record exactly)
cargo fmt --all -- --check                      CLEAN
cargo clippy --workspace --all-targets --all-features --offline -- -D warnings   CLEAN
git diff --check                                CLEAN
```
(Gate 2, `--all-features`, was not re-run here; the record states 1017/3/0, same three
failures plus one feature-gated test.)

All three failures are pure golden equality. **No panic, no typed error, no resource refusal,
no `OpenSliceSeam`, no `LayerUnion` error, anywhere in the workspace.**

| test | assertion | verdict |
|---|---|---|
| `public_runner_seals_toed_slab_and_raking_blade_after_seat_coalescing` | `toed_slab (64,170,13346779153602682749)` vs pinned `(64,179,4826202191472851835)`; `raking_blade (27,72,8660653339934460766)` vs pinned `(32,89,1709218010932846012)`. `distinct_seats == identities` holds on both (64 of 64, 27 of 27). | Tuple move exactly matches the census (Finding 4). Nothing else in the test's invariants moved. |
| `specialized::tests::oriented_toed_slab_and_raking_blade_finalize_after_terminal_seat_coalescing` | toed slab `(42, 34304, 8)` vs `(72, 50688, 9)`, shared_seats 3 vs 4; raking blade `(33, 34176, 8)` vs `(40, 35008, 11)`, shared_seats 3 vs 3. `non_worsening=true` on **both**; `distinct_positions` = full count on both. | The gate's own health predicates still pass; only the frozen numbers moved. This is the FU-7 72→42. |
| `public_coverage_report_is_frozen_without_evidence_loss` | full-report JSON equality | **I diffed the two JSON documents field by field: exactly one field differs — `snapshot_digest` `5304257786453935365 → 5470664714994646368`.** Every `request`, every `resources` counter, all 9 islands, all 9 patches (`uncovered_area_mm2`, `cut_ambiguous`, `assessment_failed`, `worst_gap`), and all 9 defects are byte-identical. The phase-C record's claim here is verified, not taken on trust. |

**Nothing else is lurking in the failures.** The one thing *not* covered is flagged in
Finding 6.3.

---

## Finding 6 — RISK: where could the NEW output be wrong?

I hunted specifically for the tip being wrong. Four candidate risks; two dismissed with
measurement, two are real and should be on the ruling.

### 6.1 DISMISSED — "the raking blade lost 2.57e-3 mm² of area on 20 layers"

My first automated scan flagged this as `AREA_LOSS` on all 20 layers. It is not loss. The
region rasterization (Finding 2) reports **0.000000 mm² lost and 0.000000 mm² gained** on
those layers: the filled region is *identical*. The 2.57e-3 mm² is the double-count the
parent's `Σ area_mm2` carried because two overlapping loops each counted the intersection.
An independent 4000×4000 grid measurement of the polygon intersection returns
2.5619e-3 mm², agreeing with `sum − union`. i_overlay's answer is right.

### 6.2 DISMISSED — "the union might fuse solids that only touch at a point"

A point contact is not a load path, and merging on one would be a real defect. Measured
shared-boundary length for every merge sampled: `make_thin_fin` **10.0000 mm**,
`make_overhang_table` **4.0000 mm** per leg, `make_toed_slab` **1.0153–3.6853 mm**,
`make_raking_blade` **no shared edge at all — a true area overlap**. No merge in this corpus
rests on a tangency. No degenerate, collapsed, or sub-`BIRTH_MIN_AREA_MM2` loop appears in
any tip output.

### 6.3 REAL, and not in the phase-C record — the OQ-3 adequacy consumer has ZERO corpus coverage

§8.5 measurement (3) reads *"The adequacy-probe path moves zero rows on all seven models. OQ-3's
new consumer contributes nothing to the delta."* True — but **vacuously**. My
`PLINTH_PASSTHRU_DUMP` instrumentation, which fires once per normalized layer at *both* sites,
recorded across all seven models:

```
print-layer rows : 686 + 369 + 269 + 432 + 400 + 600 + 200 = 2956
adequacy rows    : 0
```

`find_local_minima` reaches `normalized_slice_layers(SliceStitchSpace::AdequacyProbe, …)` on
every model, but `probe_heights` is empty every time — every local minimum matched a birth, so
`suspects` is empty and the batch has zero planes. The adequacy site's **new** behaviour
(normalization, and the newly-*binding* `max_loop_vertices` cap that phase-C deviation 2 adds
to a site that previously had no loop-vertex cap at all) therefore has **no corpus-level
evidence whatsoever**. Its only coverage is unit tests. "Zero rows moved" is not the same
claim as "zero rows ran", and the record does not distinguish them. This is a coverage gap,
not a known defect — but it is the part of the cutover the census cannot vouch for, and the
newly-binding cap is a *new refusal path* that has never executed on real data.

### 6.4 REAL, and a judgement call for the owner — the raking-blade fusion is 0.0299 mm deep

The 20 removed births on `make_raking_blade` (5 contacts, 17 members) rest on the tooth being
fused to the body. The fusion is an in-plane overlap **0.0299 mm deep and ~0.172 mm wide**
(area 2.57e-3 mm²). Against this project's own precedents:

- 0.0299 mm is **below the 0.05 mm layer height** and below the 0.05 mm process resolution.
- It is ~1.6 pixels at the 19 µm Y pixel pitch, ~2 pixels at the 14 µm X pitch — printable,
  but barely.
- PR-1(a), as cited in the 2026-08-05 `NotAttached` attribution, holds that *"a fused region
  thinner than one layer is not resolvable by the printer, so there is no interface … and
  nothing to keep."* That precedent is about depth along a contact normal, not an in-plane
  slice overlap, so it does not govern directly — but it is the closest ruling on the books
  and it points the other way.

Note the mitigation before treating this as a blocker: the tooth still receives its 8 genuine
births at layers 82–89, where it is a free-standing solid. The question is only whether the
*additional* 20 re-births at 90–109 were buying real support or padding. The parent's own
answer is incoherent either way — it births the same tooth 29 times.

**Balance of risk:** the geometry engine is not the risk. Zero material lost, zero invariant
violations, 88 mm² of real material recovered, every merge on a ≥1 mm edge or a real overlap.
The risks are (6.3) an untested code path the census cannot see, and (6.4) one physics
judgement about a sub-layer-thickness fusion.

---

## Recommendation — RE-PIN, with three conditions

The delta is fully attributed, and every unit of it is a correction. Four models moved; two of
them (`make_overhang_table`, `make_thin_fin`) change support by *exactly nothing* and moved
only because the digest hashes loop vertices that were requantized by five picometres. The
other two are driven by 24 phantom island births, each traceable to a named layer: 20 on
`make_raking_blade` where the legacy stitcher re-birthed one fused rake tooth on every layer,
and 4 on `make_toed_slab` where a boundary-degenerate parity probe — evaluated 1e-15 mm from a
neighbour's edge, flipping its verdict between adjacent 0.05 mm layers — labelled a solid toe a
void, attached that void as a hole to a solid that provably does not contain it, and made the
layer's reported area disagree with its own rasterization by 15.9 mm² and with geometry by
31.8 mm². No material is lost anywhere in the corpus; 88.04 mm² of real material is recovered;
the tip satisfies every normalization invariant on all 4,332 solids; the three failing tests
are pure golden equality with every non-digest evidence field byte-identical; and the cutover
reproduces FU-7's parked `72 → 42` digit for digit, which discharges the freeze-blocking half
of that item. **Conditions:** (1) correct §8.5's "abutting, not overlapping … within 2e-8 mm²"
claim, which `make_raking_blade`'s own numbers falsify by five orders of magnitude — the
attribution record must not ship with a wrong mechanism in it; (2) record that the OQ-3
adequacy consumer ran **zero rows** on the corpus, so "moves zero rows" is a coverage gap and
not a measurement, and open a follow-up to exercise it (the newly-binding `max_loop_vertices`
cap on that site is an unexercised refusal path); (3) rule explicitly on the 0.0299 mm
`make_raking_blade` fusion — it is below one layer height, and PR-1(a)'s precedent points the
other way, so the −5/−17 there deserves a named decision rather than inheriting the toed-slab
verdict. Conditions (1) and (2) are record fixes; only (3) could change the answer, and only
for one model. If the owner declines (3), the honest partial is: re-pin all four, and carry
`make_raking_blade` as provisional pending a sub-layer-fusion ruling — which is the same
posture `make_toed_slab` has held since 2026-08-04, and which this work now retires.
