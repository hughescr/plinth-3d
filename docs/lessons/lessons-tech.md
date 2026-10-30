# plinth: technical and architectural lessons for the restart (2026-10-29)

Read-only research. Paths: `R/` = `plinth-3d/` (develop), `E/` = `plinth-evidence/`, `V/` = `E/perf-2026-10-14/validator/`,
`M/` = `~/.claude/projects/-Users-craig-code-hughescr-dnd-campaigns/memory/`. Numbers are measured from the repo today unless marked (est).
Knowledge, not code, is what carries over. Section 6 is the short list.

## 0. Scale of what was built

- 2098 commits, 2026-07-20 to 2026-10-27 (about 30/day, peak 154 on 2026-08-01). 306 merge commits. Net +1.1 M / -0.2 M lines touched.
- About 444 k lines of Rust (incl. inline tests) in 13 crates plus `rust/geom_kernel` (28 k) and `rust/chitubox_io` (6 k). 3435 `#[test]` fns.
  `plinth-sweep` 155 k, `plinth-support` 125 k, `plinth-pipeline` 28 k, `plinth-hollow` 23 k, `plinth-mesh` 18 k, `plinth-orient` 17 k, core 16 k.
- `crates/plinth-sweep/src/lib.rs` is 8288 lines; `plan_after_prefix` alone is about 1360 lines. `slenderness.rs` 12 k, `up_sweep.rs` 9 k, `lip_seat.rs` 9 k.
- 93 fields in `SweepPolicy`, 407 distinct `PLINTH_*` names in code/scripts, 42 `too_many_arguments` suppressions (`R/crates/plinth-sweep/src/lib.rs:848`).
- Outcome: on 2026-10-21 Craig still said "not printable yet" (M/plinth-state-2026-10-21). The 2026-08-30 metric read 0 of 16 print candidates
  (M/print-candidate-metric). Round_25/50/100 discs, "the simplest possible support job", still failed the contact cap after 8 rounds (section 1.6).

## 1. Architecture as built, and its structural mistakes

### 1.1 Pipeline (R/docs/architecture.md; run order in `R/crates/plinth-pipeline`)
STL -> repair (declines, never refuses) -> decimate (QEM via meshopt, becomes the only mesh) -> orient (dense 3-tier pose search over a candidate grid,
or pinned/as-drawn) -> hollow (voxel EDT + surface-nets cavity, Manifold boolean, drains/vents, drilling; since 2026-10-16 run per pose, "hollow-first")
-> supports -> lower (pure, serial: members to frustum/sphere/pad primitives) -> pre-write gate -> emit (f64 to f32 `.chitubox`, then audit a fresh re-open).
Supports in production = `plinth-sweep` (the "slice-sweep core"): raster the oriented model per 0.05 mm layer, UP sweep places seats by a reach potential
(`reach.rs`, `up_sweep.rs`), DOWN sweep routes load and sizes every member per layer by the area law `A_arm >= A_supported / 18`
(`down_sweep.rs`, `per_event.rs`), then about 20 repair/refinement passes (list in 1.3), then a final audit of the written file.

### 1.2 Data model
- One mutable `Vec<SweepStrut>` (a "member": polyline of per-layer slots with radius ledger, landing, seat normal/face, kind Arm/Brace/Mast, thickened/min
  radius vectors, foot radius, seat-on-strut ...; `lib.rs:254`) plus side tables (`ColumnHistory`, `TipField`, `Grid`, layer cache, reservations, clearance memo).
- Contacts (model-touching tips and feet) are NOT a type. A contact is "slot 0 of a member plus whatever the seat stage stamps later" (`seat_normal`, `seat_face.point_mm`
  are zero until StageSeat, `lib.rs:4953`; V/PLAN.md section 0). Several readers each re-derive the drawn geometry (lip-seat `live_arms`, adapter, lowering, audit).
- Three generations of support engine coexist in-tree: the Python v4 engine (deleted), the Rust triangle-keyed engine (`plinth-support` cco/forest/rewire/brace/joints/
  runner/specialized/perception, still compiled; its lowering/raft/contact code is reused by the sweep), and the sweep. `plinth-cli`'s default is still
  `--placement legacy` and its help text calls sweep "EXPERIMENTAL ... runs NO pre-write gate" (`R/crates/plinth-cli/src/lib.rs:30,100`), while every perf run, every
  Craig review and every evidence folder used `--placement sweep`. `R/docs/status.md` (24 of 37 emit, 2026-08-25) and `architecture.md` describe the legacy path.

### 1.3 Late limits: the contact is an output of a loop, then edited by about fifteen passes
Evidence: V/RETHINK.md section 1.2, 3. Order of the sweep (`lib.rs` perf-span list, line numbers at HEAD): setup 2759, up 3106, down_initial 3416, removal 3560,
unmerge 3683, congestion 3737, stranded 3825, forced_merges 3895, lip_seat_a 4034, graze 4065, head_join 4116, lip_seat_b 4158, ties 4244, model_ties 4292,
reroute 4370, graze_late 4564, joints 4774, seat 5231, then footprint-normal, real-contact growth x2, footprint slides, head rebuild, normal rewrite, feet sizing,
hair tips/pins, bore relocation (V/RETHINK.md lists the mover sites `lib.rs:3984...5747`).
- The 0.8 mm contact cap, the no-fusion rule, drain keep-out, 0.3 mm gap, and the unbraced-run limit were each added AFTER the contact existed, as a later pass or
  a "validator" bolted over earlier passes. Each later pass could re-break an earlier guarantee; the guarantee had no owner.
- Contact patch size is a consequence of: reach pitch (3.63 mm, from reach only) -> Voronoi load (11.4 mm2 per seat vs 9.05 allowed at the cap) -> load-sized radius
  (clamped to a legacy 1.0 mm max, not the 0.4 cap) -> seat law (depth = max(allowance, h + r sin(theta), 0.382 r)) -> lowering -> trim. Discs were wrong "by
  arithmetic, not by bug" (V/RETHINK.md section 0). ChituBox, by contrast, takes contact as INPUT: a fixed sphere (heavy: 0.8 mm dia at 0.3 mm depth) leaving
  along the face normal; the patch is the chord 2*sqrt(d(D-d)) whatever the angle (V/RETHINK.md 1.1; `dnd-campaigns/Chitubox Projects/chitubox-support-specs.cfgx` is the reference export).
- Symptom: every limit needed its own repair lane. Lanes pcap 1-8, CBT 0a-c, cr1, cpa 1-6 (V/CAP-CONVERGENCE.md, cr1/HANDBACK.md, cpa*/HANDBACK.md) tried to
  retrofit the cap; none passed. Round_50: 198 arms -> 340 -> 466 (bound 223), down passes 4 -> 24 (V/cr1). cpa5/6 reached CS=FU=0 on discs but with 283 arms vs
  273 allowed, 7 down passes vs 4, and new clearance faults (cpa6 handback). V/CAP-CONVERGENCE.md: "three constraints (geometry, capacity, structure) are solved
  separately although each depends on the others; the interface returns a radius, not a feasible assignment."
- Craig's response (2026-10-28): "Redesign it properly". Fable's RETHINK: make a contact an append-only immutable primitive created with capacity, patch,
  reservation, approach axis and stub; pitch from capacity; nothing after creation may move it (V/RETHINK.md section 2). Astra's challenge accepted the
  direction, rejected the claims (p_cap is a density prescription not a bound; normal stubs are a candidate not an answer; routing needs provisional routes;
  one resample wave can under-cover) (V/RETHINK-CHALLENGE.md). Neither design has been built; cpa stopped at Round_50.

### 1.4 Shared mutable field, 20+ writers
- Every pass takes `&mut [SweepStrut]` and edits radius, landing, lean, host or seat. Passes are ordered by history, not by dependency. Examples: slenderness
  "hand-down" writes every slot up to the contact slot, widening INTO the contact (`slenderness.rs:7107`); `commit_seat` checks twins only in the same cell/layer
  (`up_sweep.rs:2797`); inserted load arms bypass `commit_seat` entirely (V/UPSWEEP-DESIGN.md "Why the chooser admits faults").
- Because geometry was judged by earlier passes on a field that later changed, measurements lied: a 0.318 mm clearance at radius 0.257 became 0.067 mm at 0.508
  after load growth (V/UPSWEEP-DESIGN.md); jx fix: drawn hosts up to 7.7x under their ledger (M/plinth-state: 73a6d044).
- Cost: any change re-pinned fixtures ("zero-field-divergence" fixture re-recorded eight times, `R/docs/status.md`), pinned digests drifted per lane, and
  reviewers could not tell intended from accidental movement.

### 1.5 Repair-on-repair, and compute-then-discard
- Coverage repair loop (legacy engine): up to 10 222 candidates, 110 s per trial on the treant, 24.6 s to commit nothing on a synthetic blade
  (`R/docs/evidence/repair-cap-evidence.md`). "30 of 34 corpus failures are cap exhaustion in nested retry loops whose repairs structurally cannot fix the defects
  they gate on" (R/docs/engineering-journal/decisions/2026-07-28-rust-rewrite.md, the argument for the restart that then recreated a milder version of it).
- "P2" second-contact re-plan: a full second placement, discarded on all three profiled models (Deinonychus 35 of 72 s; E/perf-2026-10-14/profile4/PROFILE4.md
  section a). The chooser needed the footprint republish at the very end, so no sound early exit existed.
- Pose search planned 3 whole poses and then re-planned the winner: pose search about 375 s of 769 s on Deinonychus (M/lane-model-run-budget). Hollow-first (2026-10-16)
  fixed weight/drain/vent being scored on the wrong geometry.
- Mover attempts that were tried and reverted "no measured benefit" appear in merge subjects (a11efd7e merges/un-merge; L4 coarse lip search dropped; SC-3 edit
  transaction off by default; MORNING-NOTES). 8 WIP commits, about 10 reverts, 405 "fix" commits (19%), 99 follow-up/again/regress commits. The real churn is not git
  reverts; it is abandoned lanes living in evidence folders and `wip/*` branches.

### 1.6 Proxy measurements vs the written file (the same error four times)
- v19 reported "neck median 0.96 mm" from planned head parameters; the drawing had no neck (M/measure-from-emitted-file).
- cs lane capped contact radius on one sizing path; emitted file still had 1.70 and 2.19 mm contacts (M/limit-means-hard-limit-in-file).
- Orientation/placement forecasts ("Level A" in UPSWEEP-DESIGN) vs sized drawing ("Level B") vs written-file audit (11 rules, `R/docs/development.md` "final audit"):
  three levels of truth, each with its own reader; LL (load) unmeasured; clearance of hulls/knees/collars/raft, post-trim section and lean on lowered geometry
  not audited (S2-S5 not built).
- 24 of 37 minis "emitted" but approx 0 would print (M/print-candidate-metric). A success metric defined by the producer (emitted) was wrong; the unit was wrong too
  (4 of the 24 were display bases).
- Audit fixtures built by hand all failed to load in ChituBox while matching the working header byte-for-byte (M/verify-against-the-real-consumer).
- "Defects climb the stack": harness correct at record, wiring, summary and test layers one at a time; a test helper that re-implemented production logic let a
  sabotaged call site pass (M/defects-climb-the-stack). Sabotage proves a check fires, not that it watches the right quantity (M/sabotage-proves-firing-not-sufficiency).
- Duplicated geometry: 63 private vector helpers across 15 files, 9 operations implemented 2-6 times (R/docs/evidence/geometry-consolidation-result.md).

### 1.7 Knobs, flags and docs as accretion
- Each lane landed behind an env var/policy field (`PLINTH_CONSTRUCTION_CONTACTS=0`, `PLINTH_APPROACH_HEADS=blocked`, `PLINTH_CAPACITY_PLACEMENT`, ...): 407 names.
  Defaults and "legacy" paths diverged from what was tested. The tool that reads the written file (`tools/plinth-audit`) was deliberately independent (excluded from the
  workspace so it cannot share a type), which is the one design that kept paying off (R/Cargo.toml comment; docs/specs/audit-invariants.md section 0).
- Docs drifted: status.md stops at 2026-08-25; the physics doc grew to 1674 lines with dated rulings embedded; an arch doc described a pipeline not in production.
- 273 of 306 registered constants are "placeholder" (unmeasured) (`R/docs/physics.md` section 11); doctrine said placeholders prune, never decide, yet fixed mm values
  (0.6 mm margin, 2.85 mm drain keep-out, 4.0 mm target spacing) kept deciding (M/physics-first-constants-doctrine; `TARGET_SPACING_MM` "governs nothing": median NN 1.03 mm).

### 1.8 Process-shaped architecture (est.)
Work ran as many parallel agent lanes in worktrees, each told "add X behind a flag, byte-identical when off". That reward structure favours adding a pass over
changing the core. It produced the pass list, the knob count, the pinned-digest churn, and design memos whose headlines outran their arithmetic (M/design-framing-optimism-bias:
"125 recoveries" cut to 80). Reports needed git ground truth (M/agent-reports-need-git-ground-truth).

## 2. What worked, and why

- Physics from measured plates: X* in [25.00, 25.34] from plate 5, cylinder cross-check (19.91, 30.48); area law `A_arm >= A_supported/X`, designed at 18 (Craig
  2026-09-30, was 12). Failure section is the narrowest arm section just above the contact, so "flare fast above the tip" (R/docs/physics.md section 2). Strong, small, reusable.
- Typed constant provenance (measured/derived/placeholder in the type system) made unmeasured values visible and recalibration mechanical (physics.md section 12).
- Disk-first verification: `chitubox_io` re-parses its own bytes, runs a second independent pointer-chain audit, writes via temp+rename, and never says "ChituBox-valid"
  (chitubox-format.md section 11). `tools/plinth-audit` shares no types. The final 11-rule audit reads the written file (CS, FU, DK, HS, SL, LG, AR, CL, TR, DC, LL).
  "Emit and warn, never refuse" (M/emit-and-warn-never-refuse): a bad file is evidence; a refusal destroys it.
- Determinism by construction: f64 in the data path, index-parallel map-collect only, no float reduction across parallel items, one thread budget (`PLINTH_THREADS`),
  canonical wire order for triangles, quantise exactly once (1e-8 mm grid) in slicing. Byte-identical output at any thread count made every perf change checkable
  with a sha256 and let 40+ perf lanes land safely (E/perf-2026-10-14/MORNING-NOTES.md).
- Batch-first kernel (`geom_kernel`: BVH, closest point, ray/segment batches, slicing, parity grids; every query takes a slice, returns a slice). Hot-path scalar calls are defects
  (M/batch-first-architecture). Progress reporting off the hot path (counters + timer thread).
- Performance record (all byte-identical unless noted): BCO 112.7 -> 33.8 s (R0+R1, 2026-10-10: seal field skipped, curvature parallel, winner plan reused) -> 17.2 s
  (develop 6b04d3f, 2026-10-15); Deinonychus 769 -> 317 -> 259 -> 188 -> 115 -> 100 -> 84 -> 71.75 s (A1, B1, D1 layer cache + Kc kernel, D2 sparse sizing, UN neighbour index,
  PX prefix reuse, RC reach sparse + RP replay reuse). What paid: (1) counters first (M0/L0 `--perf`, `perf.count_overhead` itself was 28% of a profiled run and had to be
  subtracted), (2) report-only checks run in every pose (stage-shake check = 37% of Giant_Bat CPU), (3) caching load-independent per-layer data (24 GiB byte pool + streaming
  fallback), (4) sparse instead of dense sizing, (5) neighbour indices, (6) reuse the prefix between plans, (7) target-cpu=apple-m3 (+ opt-in mimalloc/PGO, measured at milestones).
  Treant final sweep 952 -> 561 s by overlapping the shake check beside the re-plan on its own lower-priority pool.
  What did not: L4 coarse lip search (saved only through an unsafe cap/window); parallel per-arm path tests were 1.9x SLOWER ("tiny parallel tasks starve the pool": about
  9 M rayon tasks, 10x sys time); memo/read-ahead wins were each 3-10%.
- Hard numerics lesson: the pose search was chaotic under rounding-level jitter (5 of 6 runs flipped the winner) because `orient/section.rs` prefix-summed absolute-height
  quadratics (a sliver facet gave c2 = 7e30). Fixed with a segment tree on local heights, +0.1 s; winner flips 0/5 (M/plinth-state, performance-goal "PEAK-TERM.md").
- Fast gate and gate profile: `--profile gate` (release semantics, cheap compile) for lanes; `--release` at merge; `scripts/test-fast.sh`, `known-reds.txt`, seed target
  fingerprinting (stale seed after rustflags change caused a silent invalidation, 28ee040d). Binary provenance: second no-op build must print 0 Compiling lines.
- Orientation as a measured sweep with logged winners, hollow-first scoring, "as drawn" marks (Finder tag `plinth-no-rotate`, `<stem>.asdrawn` marker; git loses xattrs).

## 3. Hard-won domain and format knowledge

### 3.1 ChituBox `.chitubox` (CHITUBOX Basic 1.3.0; R/docs/specs/chitubox-format.md and chitubox-io.md)
- Header 24 B: magic `0xAB231243`, model count, model-table offset `off8`, settings size (0 if empty, else 420), const 412, support anchor `off20 = off8 + 680n`.
  396 B settings blob, raw preview/thumbnail buffer (unmapped, preserve), then 680-byte model records (name@0, translate xyz f32 @256/260/264, support-index ptr @268, mesh
  off/size @272/276). Follow the pointer chain; never scan for magics. Absolute offsets are never stable.
- Mesh = raw f32 triangle soup, 36 B/tri, no normals, no count. ChituBox centres on the bbox centre at import, REORDERS triangles, bakes rotation and scale into vertices
  (only translate is a field). Rotation convention probed: Ry(45)*Rz(45). Supports bind to a model by POSITION (contiguous block after that model's mesh), 16-byte index
  `{n, records_off, mesh_off, mesh_size}`, 400 B zero group header, then 72-byte records (magic `0xEA342389`).
- Record types: 7 sphere-only contact, 1 taper/cone (contact depth in trailing float slot 0), 9 joint sphere, 3 column, 4 root boss/flare, 6 raft pad/bridge. Pad = 2.2 mm dia
  / 0.2 mm high / 80 tris, bridge 12 tris; type-6 records write z = 0.0; flare ends on pad top minus 0.002 mm; flares stopping at plate_z+0.2 floated four layers in
  ChituBox. Bridges come first in the raft block. Adding supports auto-lifts models by 3.0 mm (off264); whether that is independent of supports is unprobed.
- What ChituBox rejected: five hand-synthesised "known-good" fixtures whose headers matched byte-for-byte ("The current file cannot be successfully parse"), regenerated
  fixtures were circular. No agent could launch ChituBox, so only Craig's open/slice is the oracle. Only native files are ground truth (`two-supports`, `basic-cube-supports-raft`,
  `cylinder-supported-bottom-connected`, morel, treant). `Chitubox Projects/chitubox-support-specs.cfgx` (support profile export) and `cylinder-cube-sphere.{chitubox,ctp}`
  (a sliced `.ctp`, never used as raster ground truth) are the reference exports. The treant file: 27 MiB, 429 120 tris, 2585 support records.
- Units trap: ChituBox speaks DIAMETER, the code speaks RADIUS (tip min 0.05 mm radius = 0.1 mm dia; max 1.0). Craig's field tips: 0.1 mm hair, 0.8 mm heaviest sphere.

### 3.2 Printer / slicing / raster
- Saturn 4 Ultra 16K, tilt-release: front (-Y) travels furthest; layers peel front to back; Y position changes peel load (est. 2.1x-5.7x, M/saturn4-tilt-peel-geometry).
  Later measurement says front-to-back is at most 1 rung; randomise, don't measure (M/y-sensitivity-settled). Layer 0.05 mm; pixel 0.014 x 0.019 mm; build volume 211.68 x 118.37 x 220.
- Peel is the only modelled load; no gravity. Suction cup effect "not a huge problem" on tilt-release. Hollowing makes weight scale with area, so one area law serves both.
- Never rest on the plate (elephant's foot): lift always; a flat base is correctly supported across its whole underside (M/lift-always-elephants-foot).
- Slice-sweep: raster per layer; reach potential U carried upward, cells with U>1 are demand; reach scales with cured thickness (kappa); full-containment vs any-overlap
  masks bias demand/carriers in opposite directions so keep both (E/slice-sweep-2026-09-02/specs/sweep-algorithm.md A10). Per-layer polygon union with fill-rule
  normalisation: layers already in normal form pass through verbatim.
- Overhang angle limit never measured; logistic centred 45 deg (30-55 deg) is an estimate; arm-snap ratio does NOT transfer to overhang strips (M/overhang-limit-estimate).
  Single-lamina reach 2.13-4.67 mm flat.
- Peel-driven demand is assigned by down-facing area; steep-face (grazing) contacts are misplaced; re-land, don't neck or thicken (M/steep-face-contacts-reland).

### 3.3 Mesh repair
- Real corpus defects, measured on 21 refused/suspect models: 113 pinch vertices, 3 duplicate-face groups, about 10 dust shards. All watertight, consistently oriented, no
  non-manifold edges (`R/crates/plinth-mesh/src/repair.rs` header). Pinch vertices are legitimate fingertip touches (ruling 2026-08-12), not defects. Strip dust, drop exact
  duplicate faces, never keep-largest. Load meshes without implicit weld/reorder/f32 conversion. Decimation must not change surface > 0.01 mm and must be the only mesh later stages see.
- A float32 round-trip fragments adjacency, flips topology flags and breaks coverage: f64 until the writer.

### 3.4 Orientation / hollowing / drains
- No flat-face heuristic (a conic base wants supports, a table top must stay clean; only the user knows). Use "as drawn" marks. Round bases: glue face UP. Orientation
  must be scored on the hollowed, drilled shell. Everything in hollowing depends on pose through the voxel lattice, so hollow per pose (hollow stage 0.26 s vs 45 s sweep).
- Hollowing is for Large+ creatures (saves resin); 30 mm minis correctly refuse. A Large mini that ships solid is a defect. Treant hollowing was a regression from the
  Python engine (multi-chamber, drains through both soles); fix = fill sealed sub-voxel sliver voids rather than refuse (f3c4d046). Giant Toad needed a 9.1 GB drain-planning
  charge against a 1 GiB cap: "fix the charge, not the cap". Resin must drain at each cavity's local low point with a vent above; multi-cavity allowed; chains must end at the exterior.
- Drain keep-out is only the cylinder under the hole (bore + radius), not a disc; thin tips may touch the rim (M/drain-rim-ruling). A cap's job is to refuse superlinear work
  (`cap = max(min, per_face x faces)`), not large models.

### 3.5 Support-quality rulings that are domain facts
Contact <= 0.8 mm dia incl. feet; any fused model contact is a defect; 0.3 mm final gap (field-proven), 0.6 mm only a placement margin; approach angle of a tip is a design
variable (cone poking into the model is a placement fault, not a trim problem); departure rule near contacts; thin rods are braced to thick columns; tiny islands that join
supported structure within 1-2 layers get no support; tool access between marks matters, plate packing may fuse. Brace tie to model allowed with a rounded tip.

## 4. Test corpus inventory

| Path | Size | Role |
|---|---|---|
| `R/corpus/models/` (gitignored; manifest `R/corpus/manifest.json`) | | provisioned from Craig's folders |
| `Sphere.stl` | 238 KB, 4760 tris | dome, drain at lowest point, rim layers 100-106 (drain rim ruling); pinned verdict fixture (flips on seat-zone changes) |
| `Cylinder.stl` | 12 KB, 244 tris | faceted prism, flat underside, lowest-line load case, 13.9 GB RSS, 117-337 s; native ChituBox fixtures exist |
| `make_box`, `make_overhang_table`, `make_raking_blade`, `make_thin_fin`, `make_toed_slab` | 0.7-7 KB | synthetic one-feature probes (overhang, thin fin, toe) |
| `Round_25/50/100.stl` + `R/corpus/as-drawn/*.asdrawn` | 150 / 301 / 98 KB (3.0k / 6.0k / 2.0k tris) | THE gate model class ("goal: Round x working before anything else"); flat disc, glue face up; CS/FU counts diagnose the contact cap |
| `Weeping Treant v1.2.stl` (also `dnd-campaigns/` root) | 21.5 MB, 429 120 tris | flagship: multi-cavity hollow, 429 k tri, rotation-locked upright, 561-952 s sweep |
| `dnd-campaigns/minis-to-process/` (37 real minis) | 0.1-23 MB | the real job. Heavy: Giant Toad 23 MB/459 k tris (drain cap), Awakened Tree 21.5 MB (~50 min), zombie-1..9 14-20 MB each, Rotting Warhorse 14 MB |
| `Deinonychus.stl` | 5.9 MB, 117 k tris | primary placement-quality model: CS 12, FU 13, TR 49, 71.8 s, the cap/construction acid test |
| `Bat Cat Owl.stl` (BCO) | 6.5 MB, 129 k tris | performance reference (target "a few seconds"; now 17 s) |
| `Giant Bat.stl` | 8.0 MB, 159 k tris | Large, hollow regression, stacked-seat and plate-packing witness; 17.9 GB RSS |
| `Warhorse`, `Giant Toad`, `Morel Mudskipper/base`, Tavey set, Skeletons, `Round *` copies | 2-14 MB | Large+ hollow suite; Morel has a native supported ChituBox file |
| `cube.stl`, `dnd-campaigns/Chitubox Projects/cylinder-cube-sphere.chitubox/.ctp` + `chitubox-support-specs.cfgx` | 684 B / 4.0 MB / 301 KB / 35 KB | native cube and the owner's support profile (heavy: sphere dia 0.8 at depth 0.3, cone 0.6-1.5 over 3.0 mm, pillar 1.5) |
| `R/scripts/tests/fixtures/chitubox/` | small | native specimens (two-supports, basic-cube-supports-raft, cylinder-supported-bottom-connected) = only ground truth for PAD/BRIDGE; regenerated fixtures are circular |

`Chitubox Projects` and `printable-chitubox-minis` (empty) are Craig's local. The 14-model "fast set" and the 16-job manifest (`R/docs/job-manifest.md`) are the usual run lists.

## 5. Did Rust shape bad design choices?

Mostly no. The rewrite's own reason (Python's scalar-loop idiom degrading agent output, M/resin-rust-rewrite-decision) was borne out by the kernel: batch-first, deterministic,
typed refusals, `forbid(unsafe_code)` in 11 crate roots, 13 unsafe sites total. Bad choices that Rust made easier or failed to prevent (est., not proven):
- Borrow-checker friendliness of "one `&mut Vec<SweepStrut>` handed to pass after pass" with 14-20 parameter functions (`place_layer_cleared`, 42 `too_many_arguments` suppressions) rather
  than an immutable-then-commit design. The language had no objection to a 1360-line function.
- Long compile/link and rebuild costs (full `release` vs `gate`, `seed` target caches, binary provenance scripts) pushed lanes towards "add behind flag" and sparse testing;
  scripts/known-reds.txt tolerated reds.
- Exposing every constant as a typed `Measured/Derived/Placeholder` value invited 306 registry entries before 22 were measured.
- f64 "everywhere" plus byte-identical digest gate blocked cheap output-changing speedups (Craig 2026-10-10: f64 is "insane overkill", f32/16-bit fine) until a similarity gate existed.
- C++ Manifold through `cxx` for booleans (`plinth-solid`) is fine but adds a native build and process-global IDs that had to be canonicalised for determinism.
Where Rust helped: rayon plus index-parallel collect for determinism; sum-type refusals; per-crate `forbid(unsafe_code)`.

## 6. Design principles for the next system (each tied to a lesson)

1. Contact is an immutable primitive created with its capacity, patch, reservation and approach, and it is the first thing placed. (1.3; V/RETHINK.md). Structure grows below it.
2. Every hard limit has one owner at creation. If a pass can break a guarantee, the guarantee must be in the data type, not a later audit. (1.3, 1.4)
3. No shared mutable field mutated by an open-ended pass list. Passes read an immutable snapshot and return a proposed edit; one transaction commits, re-measures, rolls back.
   (V/CAP-CONVERGENCE.md "shared contact-budget transaction"; 1.4)
4. Verify against the real consumer, the written file, read by an independent reader; the plan is never evidence. (1.6; M/measure-from-emitted-file, M/verify-against-the-real-consumer)
5. Keep disk-first writer discipline: start from an empty project, re-parse and independently audit, temp+rename, say "self-consistent" not "ChituBox-valid". (2; format sec. 11)
6. Warn, never refuse; emit the partial file with named gaps; a refusal destroys evidence. (2; M/emit-and-warn-never-refuse)
7. Success metric = print candidates against a job manifest, judged on emitted bytes plus Craig opening it in ChituBox. Not "emits". (1.6; M/print-candidate-metric)
8. Gate model class first: Round_25/50/100 must be perfect (CS=FU=0, arms within 1.3x of the area bound, seconds) before Deinonychus; Deinonychus before the army. (M/round-bases-first)
9. Pick approach direction at placement (candidate set: normal, vertical, in between; rank zero overlap, then physics). Do not retrofit by trimming. (M/tip-approach-angle-ruling)
10. One geometry kernel, one drawing function, one reader for "what the file contains". Never a second implementation in a test or tool except the deliberately independent auditor. (1.6)
11. Determinism and f64 internally, f32 at the writer; no float reduction across parallel items; fix catastrophic cancellation (prefix sums of absolute coordinates). (2; section 2 orient)
12. Counters before optimisation; subtract the counting cost; parallelise coarse units only; cache load-independent layer data; sparse over dense. (2)
13. Constants carry provenance; placeholders may prune search, never decide; derive pitch/spacing from the physics law (p_cap = r_cap * sqrt(2 pi X / sqrt 3)). (1.7; M/physics-first-constants-doctrine)
14. A flag is a decision debt: no new env var without an owner and a removal date; one default path equals the tested path; docs describe production only. (1.2, 1.7)
15. Cap superlinear work, not large models; fix the charge, not the cap. (3.4)
16. Kill or park a design line after its stated number of rounds (cap line: 8 pcap rounds then CBT then cr1 then cpa); a design memo must carry brackets with its headline. (1.3; M/design-framing-optimism-bias)
17. One engine in tree. Delete the old core when the new one passes the gate; do not leave three generations compiled. (1.2)
18. Small iteration loop: pinned orientations, cropped fixtures that run in seconds, at most 2 full runs per lane, one final verification. (M/fast-iteration-loop, M/lane-model-run-budget)

## 10-line summary

1. plinth is 444 k lines of Rust, 2098 commits in 14 weeks; it still produced no print-ready output for Craig, even on flat discs after 8 rounds of cap retrofits.
2. Root structural mistake: model contact is an output of a reach-pitch -> Voronoi load -> radius loop, then edited by about 15 later passes; limits arrived late and could be re-broken.
3. One mutable `Vec<SweepStrut>` with 20+ writing passes, 8288-line `lib.rs`, 93 policy fields, 407 env knobs, three engine generations coexisting; CLI default is the legacy engine nobody used.
4. Truth was assessed at three levels (plan, drawn, written file); proxy numbers repeatedly contradicted the file Craig opened; the independent written-file auditor was the best design decision.
5. What worked: measured-plate area law (X* 25.0-25.3, design 18), typed constant provenance, disk-first `.chitubox` writer/audit, byte-identical determinism, batch-first kernel, counters-first perf (BCO 113 -> 17 s, Deinonychus 769 -> 72 s).
6. Format knowledge to keep: 24 B header + 680 B model records + 72 B segment records (types 7/1/9/3/4/6), triangle soup 36 B/tri, rotation baked, hand-built fixtures fail to open in ChituBox, only native files are ground truth.
7. Domain knowledge to keep: peel-only load, lift always, tilt-release, hollow Large+ only and per pose, drain keep-out only under the bore, contacts <= 0.8 mm incl. feet, no fused marks, approach angle is a design variable.
8. Real corpus meshes are watertight; only pinch vertices (legitimate), a few duplicate faces and dust shards need repair. Test set: Round_25/50/100 first, then Deinonychus/BCO/Giant_Bat/Cylinder/Sphere, treant as flagship, 37 minis as the army.
9. Rust did not cause the architecture; process (additive flagged lanes, pinned digests) and a shared mutable field did. Rust made 14-argument pass functions and giant files painless.
10. New system principles: immutable contacts placed first with capacity/approach/reservation, transactional edits, one drawing and one file reader, warn-and-emit, a flag-free single path, and the discs as the gate.
