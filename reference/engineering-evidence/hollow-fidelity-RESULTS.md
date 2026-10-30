# Lane hf (hollow first): results (2026-10-16)

Branch `feature/hollow-first-2026-10-16` (worktree `wt-hf`), base develop `a9053e75`. Commits:
`29bad949` design note, `bbbe70c0` implementation and tests, `3ce34df0` an allowlist line number
(test-only). Binary `bin/plinth-hf-bbbe70c` (sha256 e6d88105..., provenance beside it: built twice,
second build 0 Compiling lines, clean tracked tree). `3ce34df0` changes one test file, so the binary
is the code at the branch head.

Design: `DESIGN.md` (also committed as `docs/plans/hollow-first-2026-10-16.md`).

## 0. Review fix (gpt-astra-high, ACCEPT-WITH-FIXES): commit `ee08ac97`, binary `bin/plinth-hf-ee08ac9`

- **Defect:** a cut outer fragment took the solid face at a shared vertex that faced most nearly its
  way. Coplanar neighbours tie there, so the source could be a face that does not contain it (the
  reviewer: 33 of 88 exterior fragments of a drilled subdivided box).
- **Fix:** the Boolean gives no face provenance back, so a non-kept fragment is resolved by geometry.
  - Four sample points are taken: the centroid, and the three points halfway from it to each vertex.
  - Each is looked up among the solid faces that contain it: in the face's plane, inside its triangle,
    facing the same way. A uniform grid over the solid's faces does the lookup. The tolerance is 1e-9 of
    the bounding diagonal and is numerical only.
  - A fragment inside one source face takes that face's values.
  - **A fragment that spans several source faces, or a sample on a shared edge, takes the largest
    fragility and the largest display among them.** This is conservative: a scar is never priced below
    a face it may sit on.
  - Faces on no solid face (the cavity and the bore walls) stay at 0.
- **Tests:**
  - `shell_fields_tests::a_fragment_takes_the_face_that_contains_it_and_the_largest_when_it_spans_two`:
    the contained, spanning, reversed and off-plane cases.
  - `shell_fields_tests::on_a_drilled_subdivided_box_each_outer_fragment_takes_its_own_source_face`: the
    reviewer's box. The production hollow stage drills it, each of the 36 faces carries its own value, and
    the result is checked against 2-D containment on the box's sides.
  - A sabotage that skips the containment test turns both red. For example, shell face 121, which lies
    wholly in source face 2, took (9, 108) instead of (3, 102). Log: `dev/fix-sabotage.log`.
- **Re-measured with the fixed binary:**
  - Flag-on poses, scores and shas are unchanged: Sphere 4484c5db (X -30, 8.8303024 against 8.8303382),
    Cylinder f442e573 (+Z, 12.4474721 against 12.4474784). The orientation records match the earlier
    ones to the last digit.
  - On these two models the earlier rule happened to pick containing faces. The class counts are
    unchanged too: the cut fragments that touched a solid vertex are exactly the fragments now found
    by containment.
  - Solo timings, flag on: Sphere 17.15 s / 3.19 GB; Cylinder 48.81 s / 10.77 GB.
  - Flag off, shared, all unchanged: BCO 5132fb1a, Deinonychus d73742b2, Morel_base 6179139b, Cylinder
    f442e573, Sphere f55f909d.
  - The Sphere file is unchanged, so `for-craig-hf` stands as it was.
  - Runs: `fx-*`. Checks: `scripts/test-fast.sh plinth-sweep` (with plinth-pipeline and plinth-cli)
    PASS, with only the known reds plus the cube.stl FIXED-RED. Workspace clippy is clean, and the
    hollow-first sweep and pipeline tests, including the ignored hollowed-block reuse test, pass.
- **Memory, stated plainly:** there is no process-wide memory guarantee.
  - The hollow stage's admission limits are per call.
  - Up to three finalists run the hollow stage and their shell sweeps at once, and nothing bounds the
    sum at 20 GB.
  - Giant_Bat's 17.93 GB is one measurement, not a proof that a larger model stays under the limit.

## 1. What was built

- **What depends on pose.** Almost all of `plinth_hollow::run` does, through the voxel lattice: the
  cavity, flow selection, the well-composed repair, the printable-bounds check, drain and vent planning,
  drilling and refill. A model-frame hollow would need the cavity field resampled into each pose's
  lattice before drains and drilling can run; that resample is inexact and the whole stage costs only
  0.13-0.25 s on Sphere and Cylinder. So the **whole hollow stage runs per swept pose**, on that pose's
  oriented solid (`plinth_hollow::drilled_shell_for_pose`, the same body `run` now calls). The winner's
  drilled shell, drains and vents are therefore bit-for-bit what the seal makes.
- **Per production pose** (`orient_demand::measure_one`): hollow, sweep the drilled shell with its
  drains and vents, price the scars on the shell (kept faces, matched by vertex bits, keep their values;
  other fragments on the solid's surface take the largest values of the solid faces that contain their
  sample points (section 0); cavity and bore faces 0), and
  keep the plan with its holes and its oriented solid. A pose that does not hollow is measured on the
  solid exactly as before.
- **Reuse key**: mesh bits, layer bits, policy, then drains and vents bit for bit (`HolesDiffer`). Not
  weakened: a retained solid plan has no holes, so with the flag off a hollowed model misses as before.
- **Prefilter stays on the solid** (DESIGN.md §5).
- **Flag**: `--hollow-first` or `PLINTH_HOLLOW_FIRST=1`, default off; `StagePolicy::hollow_first`.
  Under `--no-cache` the finalists are still measured on their shells (same pose) and only the reuse is
  off.
- **Lane spec**: not merged here. Whichever lane merges second must skip the hollowed-sweep speculation
  when `StagePolicy::hollow_first` is true (its field doc says so); with hollow first on the winner's
  plan already is the hollowed plan.

## 2. Byte identity (flag off; flag on for models that do not hollow)

| model | develop sha | flag off | flag on |
|---|---|---|---|
| Bat_Cat_Owl | 5132fb1a | 5132fb1a (shared) | 5132fb1a (shared) |
| Deinonychus | d73742b2 | d73742b2 (shared) | d73742b2 (shared) |
| Morel_base | 6179139b | 6179139b (shared) | 6179139b (shared) |
| Cylinder | f442e573 | f442e573 (shared, solo, traced) | **f442e573** (shared, solo, traced): same pose, reused shell plan |
| Sphere | f55f909d | f55f909d (shared, solo, traced) | **4484c5db**: the pose changed (section 3) |
| Giant_Bat | 02cb733a (lane spec, S0 corpus) | not re-run | **02cb733a** (shared): same pose, reused shell plan |

Shared = 4 threads, shared lock, light perf. Solo = timing lock, 12 threads. Traced = 4 threads, full
`--perf` with the orientation record (for the scores below).

## 3. Did the pose change? Scores, solid against shell

Score = the search's objective (lower wins). Measured terms per finalist: arms (the burden), model
landings, landing scar, tip scar. From `tr-0-*/perf/*.orient.json` (solid) and `tr-1-*` (shell).

**Sphere: changed, X +30 deg to X -30 deg** (a mirror flip on a ball; the margin is in the sixth digit
either way).

| finalist | solid: arms / landings / tip scar | solid score | shell: arms / landings / tip scar | shell score |
|---|---|---|---|---|
| 6, X +30 (up 0,+0.5,0.866) | 40 / 0 / 0.00007 | **8.8304563** (won) | 37 / 3 / 0 | 8.8303382 |
| 5, X -30 (up 0,-0.5,0.866) | 40 / 0 / 0 | 8.8304593 | 36 / 2 / 0.00005 | **8.8303024** (wins) |
| 72 (tilted) | certified loser (bound 8.83052) | | certified loser (bound 8.83052) | |

On the solid the two mirror poses tie on arms and the analytic display scar decides by 3e-6. On the
shell X -30 needs one arm fewer (36 against 37), and that decides by 3.6e-5.

**Cylinder: unchanged, +Z.** Shell arms are 137 for both +Z and -Z (76 on the solid), 21 model landings
(all inside the cavity), the same tip scars, so the same 6.3e-6 gap decides as before: solid 12.4450766
against 12.4450829, shell 12.4474721 against 12.4474784. The tilted third finalist is a certified loser
in the traced run (bound 12.98686); in the solo run it finished (216 arms on its shell). The emitted file
is the same bytes: the reused shell plan is the plan the seal made before.

**Giant_Bat: unchanged, X -20 deg (up 0,-0.342,0.940).** Measured terms from the run logs (no traced
run: it would cost about 2.7 times a 19-minute run), solid (lane spec's run) against shell (this run):

| finalist | solid: arms / model landings / landing scar / tip scar | shell: arms / model landings / landing scar / tip scar |
|---|---|---|
| X -20 (0,-0.342,0.940), winner both times | 629 / 127 / 5.259 / 18.418 | 1851 / 147 / 6.564 / 18.536 |
| upright (0,0,1) | 602 / 120 / 3.423 / 8.117 | 4344 / 164 / 5.610 / 12.472 |
| X -30 (0,-0.5,0.866) | stopped (certified loser) | 700 / 139 / 2.331 / 16.812 |

The shell triples or more the arm count (the cavity ceiling), and it reorders the arms: X -30 needs
1151 fewer arms than the winner on the shell. The winner still wins on the analytic terms (height and
peel), which the arm term is deliberately too small to outweigh (orient_demand's "unit bridge": the
exchange rate between arms and height or peel is a ruling nobody has made). The file is the same bytes
as develop's: the winner's reused shell plan is the plan the seal made before.

## 4. Measured from the emitted files

Resin: `vol/target/release/resin` (scratch, built on `tools/plinth-audit`'s own rasters): the union of
every support record's emitted mesh on the printer lattice, less the model. Full support:
`fullsupport/` (lane unroot's printer-view method, every layer of the file; validated against
unroot/RESULTS.md's layer-655 numbers to the pixel): bare model pixels (nothing cured under them) whose
run through the layer's cured pixels to a held pixel exceeds the 2.13 mm reach.

| model | support resin before | after | delta | full support before | after |
|---|---|---|---|---|---|
| Cylinder | 893.97 mm3 | 893.97 mm3 | 0 (same file) | printer view 0.863 mm2 beyond reach, all on the lid underside inside the hollow (layer 655; the drain-centre patch and the slid-tip gap unroot found) | same file |
| Sphere | 119.56 mm3 | 121.52 mm3 | +1.96 mm3 (+1.6 %) | 1.047 mm2 unreachable: the first layer, the very bottom point of the ball; every other layer within reach | the same 1.047 mm2 at the bottom point; every other layer within reach |
| Giant_Bat | 3702.93 mm3 | 3702.93 mm3 | 0 (same file) | 0.481 mm2 unreachable in 39 specks on 33 layers, worst finite run 1.47 mm | same file |

**Warnings, Sphere** (log, before -> after): tips inside another member's solid 1 -> 4 (by 0.001-0.034
mm); tips with less contact than their load needs 2 -> 1; balls wider than the feature's chord 6 -> 5;
model feet partly off the model 2 -> 2; model sections over X_ALLOWED 2 -> 2; "residual demand" (the
planner's cells x layers count) 321.7 -> 371.7 mm2, against which the file-measured check above finds no
change; drain ENCLOSED warning 1 -> 1 (same drain, same numbers). `plinth-audit compare` (pose changed,
so absolute): REGRESSION on H1 floating components of 16 px or more 105 -> 251 (floating voxels 174,243
-> 183,908, about 2.3 -> 2.4 mm3), I3 min section 0 -> 1 failing item, I6 intersections 8 -> 10,
three records whose mesh is up to 0.20 mm from its header (reference: 2 within 0.04 mm), slenderness
merges refused 2 -> 3; better on H6 (deepest intrusion 1.24 -> 0.01 mm, skim violations 3 -> 1).
`audit/sphere.txt`.

**Warnings, Cylinder:** none moved (same file).

**Warnings, Giant_Bat:** none moved (same file).

## 5. Time and memory

Solo (timing lock, 12 threads, light perf), one run each:

| model | flag off | flag on | phases off -> on (production / final sweep) | peak RSS off -> on |
|---|---|---|---|---|
| Sphere | 30.18 s | **17.29 s (-43 %)** | 10.80 / 18.73 -> 12.32 / 4.26 s | 3.89 -> 3.18 GB |
| Cylinder | 89.67 s | **48.74 s (-46 %)** | 43.38 / 44.47 -> 44.91 / 2.24 s | 14.05 -> 11.03 GB |

The production stage grows by the per-pose hollow stage and the shell sweeps (Sphere +1.5 s, Cylinder
+1.5 s); the final sweep becomes the reused plan's completion (Cylinder) or the second contact's re-plan
from the reused seed (Sphere). Cyl lane base figures for comparison: 94.04 s and 31.04 s.

Shared (4 threads, other lanes on the machine; not timing): Cylinder 105.84 -> 58.67 s, 13.47 -> 10.44
GB; Sphere 33.57 -> 18.73 s, 3.76 -> 2.96 GB; BCO 14.75 -> 14.40 s, 1.68 -> 1.92 GB; Deinonychus
48.22 -> 49.53 s, 6.25 -> 6.43 GB; Morel_base 37.61 -> 29.94 s, 2.35 -> 2.91 GB. The models that do not
hollow pay one refused hollow stage per finalist (its voxel grid is the RSS rise, up to +0.56 GB); their
wall is within the shared machine's noise.

Giant_Bat (shared, 4 threads, flag on, one run, other lanes on the machine): **1123.38 s, peak RSS
17.93 GB** (under the 20 GB limit). Develop's shared runs of the same file: 996.18 s / 18.29 GB (lane
spec) and 1280.89 s / 17.58 GB (S0 corpus). Phases: prefilter 28.6 s, production 178.5 s (130.2 s on
the solid in lane spec's run), hollow 3.3 s, final 907.3 s. The final phase is the second contact's
re-plan (`sweep.run` 657.9 s) and the shake checks (565.4 s thread time, two calls), which hollow first
does not remove; it removes only the first shell plan (about 164 s on this pose, measured in
production). One shared run against runs at other times and loads cannot price that: no saving is
claimed for Giant_Bat, and the run shows no memory or time blow-up.

## 6. Tests and checks

- `plinth-sweep` `orient_demand::shell_fields_tests` (unit, two tests since the review fix; section 0):
  kept, contained, spanning, reversed and off-plane fragments on a square, and the drilled subdivided
  box checked against 2-D containment; an empty field stays empty.
- `plinth-sweep` `tests/it/hollow_first.rs` (17.6 s, in the fast loop): both poses of an 18 x 20 x 22 mm
  block are measured on their drilled shells (path hits: the kept mesh is not the solid, drains exist);
  the hollow stage run again on each pose's solid (what the seal does) hits the key. **Sabotage**: the
  winner's plan offered the other pose's holes; the other pose's plan relabelled with the winner's mesh
  and layers; the winner's plan offered no holes; one drain one ulp off: all `HolesDiffer`.
- `plinth-pipeline` `tests/it/hollow_first.rs`: a model that does not hollow (Round 25) emits the same
  bytes with the flag on and off, both through the reused plan; **a hollowed block's final placement
  reuses the winner's shell plan** (`SweepPlanSource::Reused`, disposition `HollowedAndDrilled`) and is
  the same bytes as the seal planning it itself (`--no-cache`); with the flag off it misses on
  `MeshDiffers` as before (ignored in the fast loop, 63 s; run by name, passed).
- `plinth-cli`: `--hollow-first` is off by default and given at most once.
- `cargo clippy --workspace --all-targets --all-features --offline -- -D warnings`: clean.
- `scripts/test-fast.sh plinth-hollow plinth-sweep plinth-pipeline plinth-cli` (7 crates with
  dependents): NEW-RED 1, `plinth-hollow` `no_private_vector_algebra::every_allowlist_entry_...`: my
  `drain_bores` accessor moved the allowlisted `normalize` from line 2195 to 2205; re-pointed in
  `3ce34df0` and re-run green (the known red beside it keeps its signature). FIXED-RED 1, not mine:
  `orient_as_drawn::each_mark_...` needs `cube.stl`, which is present on this machine now.

## 7. Open points for Craig

1. The cavity and bore faces are priced 0 in the scar terms (DESIGN.md §4). A ruling, not a measurement.
2. The burden counts arms inside the cavity like outside ones, though nobody clips them.
3. The prefilter still prunes on the solid.
4. On Sphere the new pose is a mirror flip decided by one arm; the audit flags the new field as worse on
   several items it judges in absolute terms. Whether that is the new pose or ordinary field-to-field
   variation is not separated here.
5. On Giant_Bat the shell changes the arm counts a lot (629 -> 1851 for the winner, and X -30 now needs
   1151 fewer arms than the winner) but not the pose, because the arm term is small against height and
   peel. Whether arms inside a hollow should weigh more is the same open exchange-rate ruling.
6. Speed: the prize is the second sweep. It is -41 s on Cylinder and -13 s on Sphere solo; on a model
   whose final phase is dominated by the second contact's re-plan (Giant_Bat) it is at most the first
   shell plan, and one shared run could not show it.

## Files

Runs: `t-{off,on}-*` (solo timing), `sh-{off,on}-*` (shared), `tr-{0,1}-*` (traced, orient records),
`gb-on` (Giant_Bat), `smoke-sph-hf` (a dev build before the touching-face rule; superseded).
`run.sh`, `shared.sh`, `timing.sh`, `trace.sh`, `eval.sh`, `build.sh`; per run `<name>/<model>.log`,
`.time`, `.sha`, `perf/`; `driver.log`; `fs/` (full-support summaries and clusters); `audit/`;
`fullsupport/` and `vol/` (scratch tools); `test-fast.log`, `test-fast/`; `dev/` (clippy and the new
tests' logs).
