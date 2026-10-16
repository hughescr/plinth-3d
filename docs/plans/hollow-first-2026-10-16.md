# Lane hf (hollow first): design note

Branch `feature/hollow-first-2026-10-16` (worktree `wt-hf`), base develop `a9053e75`.
Owner direction (Craig): score each orientation on the hollowed, drilled shell that will print, not on the solid.
Output-changing for hollowed models; behind a flag, default OFF.

## 1. What in hollowing depends on the pose (checked in `plinth-hollow`)

The claim to check was "the shell interior does not depend on pose, so hollow once in the model frame".
In the engine as built, almost all of it does depend on pose, through the voxel grid:

| step (`plinth_hollow::run`) | depends on pose? | why |
|---|---|---|
| triage, weld, closed-solid check | no | topology only |
| voxel grid (`grid_spec`, `voxelize_filled`) | **yes** | the grid is axis-aligned in the frame the mesh is in; a rotated solid gets a different lattice |
| EDT, exact distance, cavity mask (`wall + margin`) | **yes (discretization)** | computed on that lattice; the cavity surface is a surface-nets march of it |
| bulk gate, savings gate (small-model refusal) | in principle no; in practice at the voxel margin | both read the lattice (`2 * max edt`, voxel count * pitch^3) |
| flow selection (chambers joined by a bore-wide passage) | **yes (discretization)** | voxel EDT on the lattice |
| well-composed repair | **yes (discretization)** | lattice cells |
| realized-wall check (FU-55) | no in principle | measured against the source surface |
| Boolean shell (Manifold difference) | follows the cavity | |
| `validate_printable_bounds` | **yes** | per-axis build volume (211.68 x 118.37 x 220 mm) |
| `CavityField` handed to H1 (drains, drilling) | **yes** | voxel membership in the lattice frame, "not reconstructed from the shell" |
| drain planning (`plan_drains`) | **yes** | lowest cavity lobes along -Z, the plate side |
| vents | **yes** | highest cavity points along +Z |
| drilling, refill of undrainable chambers, drainability recount | **yes** | bores along Z in the lattice; refill flips lattice cells |
| inner-ceiling handling (the sweep's hidden-cavity map, interior arms) | **yes** | done by the sweep from the drilled mesh and the vents |

So "hollow once in the model frame" would need the cavity field resampled into each pose's lattice
before drains and drilling can run. That resample is a new, inexact step (the drill proof and the refill
both read lattice membership), and it would make the shell of a pose differ from what today's hollow stage
makes for that same pose. It would also save little: the whole hollow stage is 0.26 s on Cylinder and
0.14 s on Sphere (cyl lane, `hollow.stage`), against 45-47 s and 12 s for one production sweep.

**Decision: run the whole hollow stage per swept pose, on that pose's oriented solid.** It is the same
function the final path runs (`plinth_hollow::run`'s body, factored so both call one function), on a
bit-equal input, so the pose's drilled shell, drains and vents are exactly what the final path will make if
that pose wins. Craig's goal (true weight, inside surfaces, drill holes planned per pose) is met exactly,
not by an approximation of it.

## 2. Per pose (production finalists only)

`measure_one` (crates/plinth-sweep/src/orient_demand.rs), when the demand carries a hollow request:

1. orient the solid as today (`oriented_recentered_mesh`);
2. run the hollow stage on it (`plinth_hollow::drilled_shell_for_pose`). If it does not hollow (any solid
   decision or error), the pose is measured on the solid exactly as today;
3. otherwise sweep the drilled shell with its drains and vents
   (`run_sweep_with_holes_seeded_in_token_scope(shell, layers, policy, drains, vents)`), the layer plan built
   the way `sweep_and_emit` builds it (plate at the shell's own min Z minus lift);
4. price the scars on the shell (section 4);
5. retain the plan with its mesh (the shell), layers, policy, **drains and vents**, and the oriented solid it
   was made from (so `take_winner` can still match the committed pose).

## 3. Reuse: one plan, honest key

`reuse_miss` keeps its order (mesh bits, layer bits, whole policy) and now compares holes **bit for bit**
against the retained plan's own holes, instead of requiring none. A retained solid plan has no holes, so with
the flag off a hollowed model still misses (`MeshDiffers` first, as today) and nothing changes.

With the flag on, the final path hollows the committed oriented solid again (0.1-0.4 s), and its drilled
mesh, drains and vents are compared bit for bit against the winner's retained ones. Same function, same
input bits: they match, the key hits, and the second plan disappears. Any difference (another pose's
drains, a different shell) misses and the field is planned as today. Nothing in the key is weakened.

## 4. The scar terms on a shell (the objective change)

The landing and tip scars read per-face fragility and display from the orientation face table, indexed by
face of the oriented solid. A drilled shell has other faces. The rule:

- KEPT: a shell face whose three vertices, in winding order, are bit-equal to a solid face's (the Boolean
  keeps every face it does not cut) takes that face's fragility and display;
- CONTAINED: a face that is not kept but lies on the solid's surface (an outer face the drill cut again,
  or one the Boolean re-triangulated across coplanar faces). The Boolean does not hand back its face
  provenance, so the source is found by geometry: four sample points (the centroid and the three points
  halfway from it to each vertex) are looked up among the solid faces that contain them (in the face's
  plane, inside its triangle, facing the same way; a uniform grid over the solid's faces). A fragment in
  one source face takes that face's values. A fragment that spans several source faces, or a sample on a
  shared edge, takes the **largest** fragility and the **largest** display among all faces its samples lie
  in, so a scar is never priced below a face it may sit on. (The first version took the face at a shared
  vertex that faced most nearly the same way; coplanar neighbours tie there, and the review reproduced
  wrong sources on a drilled subdivided box.)
- every other face (the cavity surface, bore walls) is priced **0 / 0**: a seat inside the cavity or a bore
  is never seen and the wall there is at least the registered minimum wall, not a delicate feature.

The one number involved is a numerical allowance for "this point lies on that face":
`CONTAINMENT_RELATIVE_TOLERANCE` = 1e-9 times the solid's bounding diagonal (the Boolean's recomputed
vertices are doubles, about 1e-15 relative; the cavity is a wall away, and a bore-wall sample at least a
sixth of its triangle inside the solid). It is not a physical constant. The run log prints, per pose, the
face count and area of each class, so the rule's reach is visible. This is the one new pricing choice in
the lane and needs Craig's confirmation.

The analytic terms (height, peel corners, the analytic down-facing fold) stay on the solid table. The burden
(arm count) counts every arm that seats on the model, interior arms included.

## 5. The prefilter stays on the solid

The prefilter only prunes 15 poses to 3 finalists. It runs at 10x the working pitch, where a 2.25 mm wall is
about 4-5 cells, with no braces, heads or clearance reroute, and its arm counts are already known to rank
wrong. Hollowing every prefilter pose costs 15 hollow stages and their voxel grids, which on the Large+
models is real memory. The shell is priced where the ranking is decided, at the production pitch. Risk: a pose
that would only win on its shell could be pruned. The evaluation shows whether the finalist set looks
suspicious (a finalist near the cut on the shell).

## 6. Flag

`--hollow-first` (CLI) or `PLINTH_HOLLOW_FIRST=1` (environment; any other value is off). Default off.
It reaches `StagePolicy::hollow_first`. It has effect only on `--orient dense --placement sweep`. With
`--no-cache` the finalists are still measured on their shells (so the same pose wins) and only the reuse is
off: the seal plans the shell itself, and the file must be the same bytes. Off: every sha is develop's. On: non-hollowed models measure the solid on every
pose (the hollow stage refuses them per pose), so they stay byte-identical.

## 7. Lane spec

Lane spec runs the hollowed final sweep in parallel with production. With hollow-first on, the winner's plan
already is the hollowed plan, so that speculation must not start: spec should skip when
`StagePolicy::hollow_first` is true. The prepare.rs change here is small and kept in one place so either lane
can merge the other forward.

## 8. Memory

Each finalist now holds a voxel grid briefly (the hollow stage) and then sweeps a shell instead of a solid.
Measured peaks: Cylinder 14.05 -> 11.03 GB, Sphere 3.89 -> 3.18 GB, Giant_Bat 17.93 GB (RESULTS.md).
**There is no process-wide memory guarantee.** The hollow stage's admission limits are per call, and up to
three finalists run it and their shell sweeps at once; nothing bounds the sum at 20 GB. Giant_Bat's 17.93 GB
is one measurement, not a proof that a larger model stays under the limit.
