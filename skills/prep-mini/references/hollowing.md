# Hollowing, orientation, and drain drilling

Run by `scripts/tools/hollow_drill.py` (STL in, STL out; pure Python, no
Blender: `pip install trimesh scipy scikit-image manifold3d
fast-simplification`). Read its docstring for the pipeline. This file
holds the reasoning and the slicer-facing lessons.

## Why supervise this instead of using the slicer

Slicer hollowing erodes blindly: where thickness varies it creates
sealed micro-pockets (resin traps), and hole placement is manual
guesswork. The voxel EDT field gives exact answers: which cavity
components are worth having, where resin pools, where the thinnest
low-detail wall is for each hole. A real treant model: 1 true cavity
(20.6ml) + 21-23 junk pockets correctly left solid; one drain would
have trapped 14ml, per-lobe drains cut it to 0.27ml.

## Decision thresholds (encoded in the script)

- Hollow only if max bulk thickness > 10mm (largest inscribed sphere,
  via 0.5mm voxel EDT) AND a 2.5mm-wall cavity saves >15ml or >25% of
  volume. Below that, print solid: drain holes cost surface and risk
  for negligible resin.
- Wall 2.5mm default (3mm for handling-heavy pieces).
- Drains 3mm dia, one per low lobe; vent 2.5mm near cavity top.
  The vent is not optional -- a sealed cavity alternates suction and
  pressure every layer and can blow out or crack.
- **Drainability doctrine: every cavity voxel must either drain or not
  exist.** After drain placement the script computes a per-voxel trapped
  map (a voxel traps resin iff its cavity component below its own height
  contains no drain). Trapped pockets with a short downward exit get an
  extra drain (holes on down-facing surfaces are invisible; max 3);
  pockets whose only exits are visible surfaces -- hollowed limbs, e.g.
  drooping arms -- are REFILLED to solid. Refilling beats drilling
  display surfaces and beats sealed resin: trapped liquid leaks at the
  first pinhole and expands/cracks in heat. Bonus: refilled thin limbs
  are stronger. The report lists `refilled_ml`; final trapped resin
  should read 0.0. The trapped map is computed exactly (bottom-up
  union-find, judged at each voxel's own height) -- an earlier banded
  approximation missed pockets shorter than the band that saddle into
  the main cavity just above themselves; a user caught two such 1mm arm
  stubs in slice preview. Cross-check slice preview after hollowing:
  limbs should show solid cross-sections.
- Orientation is decided BEFORE drilling (holes are gravity-relative).
  In the slicer, the model must then NOT be auto-rotated.

## Operational notes

- Run hollow_drill.py foreground in one shell call (~20s typical); in
  sandboxed VMs background processes are reaped between calls.
- **Already-hollowed input is refused (exit 1, `error_type:
  PreHollowedError`), not re-hollowed.** Step 2 voxelizes and FILLS, so an
  existing cavity is read as solid bulk (measured: 40.4mm of "bulk" on a
  sphere whose wall is 2.5mm), the HOLLOW decision fires and a brand new
  cavity replaces the real one. Such a file wants the drill-only path.
  A component counts as a cavity when it is a closed shell CONTAINED in
  another component's material -- not when its winding is inward. Winding
  sign got this wrong both ways: one flipped triangle hid a real 0.5ml void
  (destroyed, `volume_balance` passed, file written), and an inside-out
  export of a mini plus its base was refused as "hollowed" when
  `fix_normals` would have repaired it.
- **Degenerate shards are stripped on load**, and every drop is reported
  (`components`, `dropped_shards`, `dropped_shard_faces`) on the console as
  well as in the JSON. A component qualifies only as the conjunction of what
  the corpus's 13 real shards all measure: it shares no EDGE with any other
  component (which is what makes removal provably hole-free), it is
  bowtie-pinched to the main body by a shared VERTEX, it sits inside that
  body's bbox, it has <=64 faces, it is <=1mm across, and it is a CLOSED
  shell enclosing <=0.01mm^3 either way round. Openness or inverted normals
  never condemn a component -- that is how a 1.53mm gem and a spray of
  sub-millimetre spell-effect shells were being deleted. A genuine detached
  body or an internal cavity is never touched, and do not substitute "keep
  the largest component" for any of this -- that rule deletes real internal
  cavities.
- **Trust the exit code, not the JSON.** 0 = out.stl written (hollowed or
  a legitimate solid decision); 1 = bad input; 2 = self-verification failed
  and nothing was written, with the failed checks (`single_body`,
  `watertight`, `winding_consistent`, `volume_balance`, `cavity_breathes`)
  named on stderr and listed under `failed_checks` in the report. The tool
  used to exit 0 in that case, which read as a success with no output file.
- The orientation search changes bounding dims versus the sized input
  (a 119mm-tall upright model may report 113mm tall x 107mm deep after
  a lean). Footprint scaling is unaffected -- not a sizing error.
- The pooling audit auto-adds drains (up to 3) when >0.5ml is trapped;
  the report lists them with "added_by_audit": true.

## Slicer error-detection decode (CHITUBOX "Detect Errors")

On a correctly hollowed+drilled mesh expect false positives:
- "Excess shell: 1" = the cavity's inner surface (connected through the
  drill tunnels; verify independently: 1 body).
- "Hole: 2x(number of drilled holes)" = each tunnel's two apertures.
- "Bad side: small n" = normal-estimation jitter on tiny valid seam
  triangles (verify winding consistency independently).

**Never click the slicer's auto-repair on a hollowed model** -- it may
delete the "excess shell" (the cavity: model silently becomes solid) or
patch the "holes" (seals the drains). Validate with the slice preview:
two contours in hollow regions, open drains where tunnels pierce.

## Do not "clean up" detector noise

Verified the hard way, twice: sub-pixel seam/interior slivers are
intrinsic (marching-cubes mosaic + legitimate fine detail; true
degenerates = zero) and identical whether holes are drilled by mesh
boolean or carved in voxel space. Collapse-sweeping them fragmented a
watertight 1-body mesh into 41 non-watertight pieces. Acceptance is the
verification triad -- 1 body, watertight, winding-consistent -- plus
volume match and a flood-fill proving the cavity breathes. When those
pass, detector warnings get explained, not repaired. Any future cleanup
attempt: snapshot -> gentle collapse -> re-verify -> auto-revert.

## Slice-time expectations (set them for the user)

Decimation fixes mesh HANDLING (load, repair checks, support editing).
Slice time is layers x LCD resolution x anti-aliasing, nearly
independent of triangle count: a 120mm-tall model at 50um is ~2400
16K-rasterized layers and will be slow regardless. Levers, in order:
anti-aliasing 4x->2x (invisible on organic texture), layer height
(100um on bark-like models; halves print time too), GPU slicing if the
slicer version has it, variable layers via CHITUBOX Pro height ranges.
Known support lift + baked orientation makes height ranges computable:
range boundary = model-space z + lift.
